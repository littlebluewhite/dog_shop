//! 後台訂單 API（規格 §10）。權限靠 AdminUser 擷取器（未登入 401、非 admin 403）。
//! 動作（Task 6、7）做完都回整份明細，前端直接覆蓋
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::{
    auth::extract::AdminUser,
    domain::{
        admin_orders::{self, AdminOrderDetail, AdminOrderListItem, Flag},
        orders::{self, SHIPPING_CVS, SHIPPING_HOME, STATUS_PAID},
        products::{self, Page},
        settings,
        shipments::{self, CREATE_ERROR, CREATE_FAILED, SHIPMENT_PENDING},
        users::is_tw_mobile,
    },
    ecpay::{
        aio::CheckoutForm,
        logistics::{self, CreateError, CreateRequest},
    },
    error::{ApiError, ApiResult},
    extract::{AppJson, AppPath, AppQuery},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/orders", get(list))
        .route("/api/admin/orders/{id}", get(detail))
        .route("/api/admin/orders/{id}/ship-cvs", post(ship_cvs))
        .route("/api/admin/orders/{id}/ship-home", post(ship_home))
        .route("/api/admin/orders/{id}/print-label", post(print_label))
        .route("/api/admin/orders/{id}/complete", post(complete))
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub flag: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

async fn list(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppQuery(query): AppQuery<ListQuery>,
) -> ApiResult<Json<Page<AdminOrderListItem>>> {
    let status = products::clean(&query.status);
    if let Some(s) = status.as_deref()
        && !admin_orders::STATUSES.contains(&s)
    {
        return Err(ApiError::field("status", "狀態不正確"));
    }
    let flag = match products::clean(&query.flag) {
        None => None,
        Some(f) => Some(Flag::parse(&f).ok_or_else(|| {
            ApiError::field(
                "flag",
                "篩選只能是 needs_refund、cvs_returned 或 invoice_failed",
            )
        })?),
    };
    let (page, per_page) = products::clamp_paging(query.page, query.per_page, 20, 100);
    let q = products::clean(&query.q);
    let result = admin_orders::list(
        &state.db,
        q.as_deref(),
        status.as_deref(),
        flag,
        page,
        per_page,
    )
    .await?;
    Ok(Json(result))
}

async fn detail(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminOrderDetail>> {
    admin_detail(&state, id).await
}

/// 動作結束後回整份明細（Task 6、7 共用）
pub(crate) async fn admin_detail(state: &AppState, id: Uuid) -> ApiResult<Json<AdminOrderDetail>> {
    admin_orders::get_detail(&state.db, id)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

/// 建立綠界 C2C 物流單（規格 §8.3、與規格不同之處 36、39、40、41）：同步呼叫，錯誤直接回給後台。
/// 認領 → 打綠界 → 存單號 → 收尾（shipments created、orders shipped、排出貨信）
async fn ship_cvs(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminOrderDetail>> {
    let detail = orders::get_detail(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if detail.order.status != STATUS_PAID {
        return Err(ApiError::field("status", "只有已付款的訂單能出貨"));
    }
    if detail.order.shipping_method != SHIPPING_CVS {
        return Err(ApiError::field(
            "shipping_method",
            "這筆訂單是宅配，請填貨運公司與單號",
        ));
    }
    let shipment = shipments::get_by_order(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let sub_type = shipment
        .cvs_sub_type
        .clone()
        .filter(|s| logistics::is_sub_type(s))
        .ok_or_else(|| ApiError::field("shipment", "訂單沒有超商資料"))?;
    let store_id = shipment
        .cvs_store_id
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::field("shipment", "訂單沒有取貨門市"))?;

    // 上次綠界建單成功、單號已存，但收尾沒做完（例如當時資料庫斷線）：不再打綠界，只補收尾
    if shipment.ecpay_logistics_id.is_some() && shipment.status == SHIPMENT_PENDING {
        if !shipments::finalize_cvs(&state.db, id).await? {
            return Err(ApiError::field("status", "只有已付款的訂單能出貨"));
        }
        return admin_detail(&state, id).await;
    }
    if shipment.status != SHIPMENT_PENDING {
        return Err(ApiError::field("status", "物流單已建立"));
    }

    let all = settings::get_all(&state.db).await?;
    let sender_name = logistics::sanitize_name(&all.sender.name);
    if sender_name.is_empty() || !is_tw_mobile(all.sender.phone.trim()) {
        return Err(ApiError::field(
            "sender",
            "請先到「設定」填寫寄件人姓名（中文 5 字內）與手機",
        ));
    }
    let return_store_id = (all.return_store.sub_type == sub_type
        && !all.return_store.store_id.trim().is_empty())
    .then(|| all.return_store.store_id.trim().to_string());
    let merchant_trade_no = shipments::next_merchant_trade_no(
        &detail.order.order_no,
        shipment.ecpay_merchant_trade_no.as_deref(),
    )?;
    let req = CreateRequest {
        merchant_trade_no,
        sub_type,
        goods_amount: detail.order.subtotal,
        goods_name: logistics::goods_name(&detail),
        sender_name,
        sender_phone: all.sender.phone.trim().to_string(),
        receiver_name: logistics::sanitize_name(&detail.order.recipient_name),
        receiver_phone: detail.order.recipient_phone.clone(),
        receiver_email: detail.order.email.clone(),
        receiver_store_id: store_id,
        return_store_id,
    };
    let fields = logistics::create_fields(
        &state.config.ecpay,
        &state.config.public_base_url,
        &req,
        Utc::now(),
    );
    let claimed =
        shipments::claim_create(&state.db, id, &req.merchant_trade_no, &json!(fields)).await?;
    if !claimed {
        return Err(ApiError::field(
            "status",
            "物流單建立中或已建立，請重新整理",
        ));
    }

    let url = logistics::create_url(&state.config.ecpay);
    let body = match state.logistics.post_form(&url, &fields).await {
        Ok(body) => body,
        Err(e) => {
            tracing::error!(order_id = %id, error = %format!("{e:#}"), "綠界建立物流單連線失敗");
            shipments::record_create_failure(&state.db, id, CREATE_ERROR, "連線綠界失敗", None)
                .await?;
            return Err(ApiError::EcpayError(
                "連線綠界物流失敗，請稍後再試".to_string(),
            ));
        }
    };
    match logistics::parse_create_response(&state.config.ecpay, &body) {
        Ok(ok) => {
            tracing::info!(order_id = %id, logistics_id = %ok.logistics_id, rtn_code = ok.rtn_code, "綠界物流單已建立");
            shipments::record_create_ok(&state.db, id, &ok).await?;
            if !shipments::finalize_cvs(&state.db, id).await? {
                return Err(ApiError::field("status", "訂單狀態已改變，請重新整理"));
            }
            admin_detail(&state, id).await
        }
        Err(err) => {
            // 綠界原文只進 log 與 shipments.last_status_msg，不進回應的 message（計畫 3 審查交接 2）
            tracing::warn!(order_id = %id, error = %err, "綠界建立物流單失敗");
            let msg = match &err {
                CreateError::Rejected(m) => m.clone(),
                CreateError::BadMac => {
                    format!(
                        "回應簽章不符：{}",
                        body.chars().take(200).collect::<String>()
                    )
                }
                CreateError::Malformed(m) => format!("回應格式不符：{m}"),
            };
            shipments::record_create_failure(&state.db, id, CREATE_FAILED, &msg, Some(&body))
                .await?;
            Err(ApiError::EcpayError(
                "綠界沒有接受這張物流單，原因請看訂單頁的出貨區".to_string(),
            ))
        }
    }
}

#[derive(Deserialize)]
pub struct ShipHomeInput {
    #[serde(default)]
    pub carrier: String,
    #[serde(default)]
    pub tracking_no: String,
}

/// 宅配出貨：填貨運公司與單號（規格 §6.1）
async fn ship_home(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppJson(input): AppJson<ShipHomeInput>,
) -> ApiResult<Json<AdminOrderDetail>> {
    let carrier = input.carrier.trim().to_string();
    let tracking_no = input.tracking_no.trim().to_string();
    let mut errors = crate::error::FieldErrors::new();
    if carrier.is_empty() || carrier.chars().count() > 30 {
        errors.add("carrier", "必填，最多 30 字");
    }
    if tracking_no.is_empty() || tracking_no.chars().count() > 50 {
        errors.add("tracking_no", "必填，最多 50 字");
    }
    errors.into_result()?;
    let detail = orders::get_detail(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if detail.order.shipping_method != SHIPPING_HOME {
        return Err(ApiError::field(
            "shipping_method",
            "這筆訂單是超商取貨，請用「建立物流單」",
        ));
    }
    if !shipments::ship_home(&state.db, id, &carrier, &tracking_no).await? {
        return Err(ApiError::field("status", "只有已付款的訂單能出貨"));
    }
    admin_detail(&state, id).await
}

/// 列印託運單的表單（規格 §8.3）：前端在新分頁 POST 到綠界
async fn print_label(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<CheckoutForm>> {
    let shipment = shipments::get_by_order(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if shipment.method != SHIPPING_CVS {
        return Err(ApiError::field("shipping_method", "宅配沒有託運單可列印"));
    }
    let (Some(sub_type), Some(logistics_id), Some(payment_no)) = (
        shipment.cvs_sub_type.as_deref(),
        shipment.ecpay_logistics_id.as_deref(),
        shipment.cvs_payment_no.as_deref(),
    ) else {
        return Err(ApiError::field("shipment", "還沒有綠界物流單，請先建立"));
    };
    let form = logistics::print_form(
        &state.config.ecpay,
        sub_type,
        logistics_id,
        payment_no,
        shipment.cvs_validation_no.as_deref().unwrap_or(""),
    )
    .map_err(ApiError::Internal)?;
    Ok(Json(form))
}

/// 後台標記完成（規格 §4）
async fn complete(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminOrderDetail>> {
    if orders::get_detail(&state.db, id).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    if !admin_orders::complete(&state.db, id).await? {
        return Err(ApiError::field("status", "只有已出貨的訂單能標記完成"));
    }
    admin_detail(&state, id).await
}
