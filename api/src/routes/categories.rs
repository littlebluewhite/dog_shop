use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, put},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::extract::AdminUser,
    domain::categories::{self, Category},
    error::{ApiError, ApiResult},
    extract::{AppJson, AppPath},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/categories", get(list_public))
        .route("/api/admin/categories", get(list_admin).post(create))
        .route("/api/admin/categories/{id}", put(update).delete(remove))
}

#[derive(Deserialize)]
pub struct CategoryBody {
    pub name: String,
    pub slug: Option<String>,
    pub sort_order: Option<i32>,
}

async fn list_public(State(state): State<AppState>) -> ApiResult<Json<Vec<Category>>> {
    Ok(Json(categories::list(&state.db).await?))
}

async fn list_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<Category>>> {
    Ok(Json(categories::list(&state.db).await?))
}

async fn create(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppJson(body): AppJson<CategoryBody>,
) -> ApiResult<(StatusCode, Json<Category>)> {
    let category = categories::create(
        &state.db,
        body.slug.as_deref(),
        &body.name,
        body.sort_order.unwrap_or(0),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(category)))
}

async fn update(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppJson(body): AppJson<CategoryBody>,
) -> ApiResult<Json<Category>> {
    let category = categories::update(
        &state.db,
        id,
        body.slug.as_deref(),
        &body.name,
        body.sort_order.unwrap_or(0),
    )
    .await?;
    Ok(Json(category))
}

async fn remove(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<StatusCode> {
    if categories::delete(&state.db, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
