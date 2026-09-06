use axum::{Json, Router, extract::State, routing::get};
use serde::Deserialize;

use crate::{
    domain::products::{self, Page, PublicListItem, PublicProduct, SORT_OPTIONS},
    error::{ApiError, ApiResult},
    extract::{AppPath, AppQuery},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/products", get(list))
        .route("/api/products/{slug}", get(detail))
}

#[derive(Deserialize)]
pub struct PublicListQuery {
    pub q: Option<String>,
    pub category: Option<String>,
    pub sort: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    AppQuery(query): AppQuery<PublicListQuery>,
) -> ApiResult<Json<Page<PublicListItem>>> {
    let sort = products::clean(&query.sort).unwrap_or_else(|| "newest".to_string());
    if !SORT_OPTIONS.contains(&sort.as_str()) {
        return Err(ApiError::field(
            "sort",
            "排序只能是 newest、price_asc 或 price_desc",
        ));
    }
    let (page, per_page) = products::clamp_paging(query.page, query.per_page, 24, 60);
    let q = products::clean(&query.q);
    let category = products::clean(&query.category);
    let result = products::list_public(
        &state.db,
        q.as_deref(),
        category.as_deref(),
        &sort,
        page,
        per_page,
    )
    .await?;
    Ok(Json(result))
}

async fn detail(
    State(state): State<AppState>,
    AppPath(slug): AppPath<String>,
) -> ApiResult<Json<PublicProduct>> {
    products::get_public(&state.db, &slug)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}
