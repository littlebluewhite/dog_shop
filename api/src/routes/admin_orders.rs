//! 後台訂單 API（規格 §10）。權限靠 AdminUser 擷取器（未登入 401、非 admin 403）。
//! 動作（Task 6、7）做完都回整份明細，前端直接覆蓋
use axum::{Json, Router, extract::State, routing::get};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::extract::AdminUser,
    domain::{
        admin_orders::{self, AdminOrderDetail, AdminOrderListItem, Flag},
        products::{self, Page},
    },
    error::{ApiError, ApiResult},
    extract::{AppPath, AppQuery},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/orders", get(list))
        .route("/api/admin/orders/{id}", get(detail))
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
