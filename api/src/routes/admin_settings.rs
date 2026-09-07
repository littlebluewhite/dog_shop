use axum::{Json, Router, extract::State, routing::get};

use crate::{
    auth::extract::AdminUser,
    domain::settings::{self, AllSettings},
    error::ApiResult,
    extract::AppJson,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/admin/settings", get(get_all).put(put_all))
}

async fn get_all(_admin: AdminUser, State(state): State<AppState>) -> ApiResult<Json<AllSettings>> {
    Ok(Json(settings::get_all(&state.db).await?))
}

/// 整組送、整組存、整組回
async fn put_all(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppJson(input): AppJson<AllSettings>,
) -> ApiResult<Json<AllSettings>> {
    let input = input.trimmed();
    settings::validate(&input)?;
    settings::put_all(&state.db, &input).await?;
    Ok(Json(settings::get_all(&state.db).await?))
}
