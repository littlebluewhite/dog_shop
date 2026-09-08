//! shipments 表（規格 §3、§4）。本任務只有狀態常數與不倒退規則；Task 4 加完整列與狀態套用，Task 6 加出貨寫入
pub const SHIPMENT_PENDING: &str = "pending";
pub const SHIPMENT_CREATED: &str = "created";
pub const SHIPMENT_IN_TRANSIT: &str = "in_transit";
pub const SHIPMENT_ARRIVED: &str = "arrived";
pub const SHIPMENT_PICKED_UP: &str = "picked_up";
pub const SHIPMENT_RETURNED: &str = "returned";
/// 宅配：老闆填單號就是 shipped
pub const SHIPMENT_SHIPPED: &str = "shipped";

/// 狀態的先後（與規格不同之處 42）：arrived 與 returned 同一階（退回後可能重新配達）；
/// picked_up 與宅配的 shipped 是終態
pub fn status_rank(status: &str) -> u8 {
    match status {
        SHIPMENT_PENDING => 0,
        SHIPMENT_CREATED => 1,
        SHIPMENT_IN_TRANSIT => 2,
        SHIPMENT_ARRIVED | SHIPMENT_RETURNED => 3,
        SHIPMENT_PICKED_UP | SHIPMENT_SHIPPED => 4,
        _ => 0,
    }
}

/// 綠界通知晚到、重複、亂序都不能讓狀態倒退：新狀態的階要 >= 目前的、而且不同才套用；終態不再改
pub fn should_apply(current: &str, next: &str) -> bool {
    current != next
        && current != SHIPMENT_PICKED_UP
        && current != SHIPMENT_SHIPPED
        && status_rank(next) >= status_rank(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_moves_apply_backward_moves_do_not() {
        assert!(should_apply(SHIPMENT_PENDING, SHIPMENT_CREATED));
        assert!(should_apply(SHIPMENT_CREATED, SHIPMENT_IN_TRANSIT));
        assert!(should_apply(SHIPMENT_IN_TRANSIT, SHIPMENT_ARRIVED));
        assert!(should_apply(SHIPMENT_ARRIVED, SHIPMENT_PICKED_UP));
        assert!(
            !should_apply(SHIPMENT_ARRIVED, SHIPMENT_IN_TRANSIT),
            "晚到的物流中心通知不能倒退"
        );
        assert!(!should_apply(SHIPMENT_IN_TRANSIT, SHIPMENT_CREATED));
        assert!(
            !should_apply(SHIPMENT_ARRIVED, SHIPMENT_ARRIVED),
            "同狀態不算變更"
        );
    }

    #[test]
    fn returned_and_arrived_can_swap_but_picked_up_is_final() {
        assert!(should_apply(SHIPMENT_ARRIVED, SHIPMENT_RETURNED));
        assert!(
            should_apply(SHIPMENT_RETURNED, SHIPMENT_ARRIVED),
            "退回後重新配達"
        );
        assert!(!should_apply(SHIPMENT_PICKED_UP, SHIPMENT_RETURNED));
        assert!(!should_apply(SHIPMENT_PICKED_UP, SHIPMENT_ARRIVED));
        assert!(
            !should_apply(SHIPMENT_SHIPPED, SHIPMENT_ARRIVED),
            "宅配不會收到超商通知"
        );
    }
}
