use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::extract::AdminUser,
    domain::products::{self, AdminListItem, AdminProduct, Page, ProductInput},
    error::{ApiError, ApiResult},
    extract::{AppJson, AppPath, AppQuery},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/products", get(list).post(create))
        .route(
            "/api/admin/products/{id}",
            get(get_one).put(update).delete(archive),
        )
}

#[derive(Deserialize)]
pub struct AdminListQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

async fn list(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppQuery(query): AppQuery<AdminListQuery>,
) -> ApiResult<Json<Page<AdminListItem>>> {
    let status = products::clean(&query.status);
    if let Some(s) = status.as_deref()
        && !matches!(
            s,
            products::STATUS_DRAFT | products::STATUS_ACTIVE | products::STATUS_ARCHIVED
        )
    {
        return Err(ApiError::field(
            "status",
            "狀態只能是 draft、active 或 archived",
        ));
    }
    let (page, per_page) = products::clamp_paging(query.page, query.per_page, 20, 100);
    let q = products::clean(&query.q);
    let result =
        products::list_admin(&state.db, q.as_deref(), status.as_deref(), page, per_page).await?;
    Ok(Json(result))
}

async fn create(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppJson(input): AppJson<ProductInput>,
) -> ApiResult<(StatusCode, Json<AdminProduct>)> {
    let product = products::create(&state.db, input).await?;
    Ok((StatusCode::CREATED, Json(product)))
}

async fn get_one(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminProduct>> {
    products::get_admin(&state.db, id)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

async fn update(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppJson(input): AppJson<ProductInput>,
) -> ApiResult<Json<AdminProduct>> {
    Ok(Json(products::update(&state.db, id, input).await?))
}

async fn archive(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<StatusCode> {
    if products::archive(&state.db, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
