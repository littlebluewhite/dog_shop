use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    auth::{
        extract::AuthUser,
        password::{MIN_PASSWORD_CHARS, hash_password_async, verify_password_async},
    },
    domain::{
        addresses::{self, Address, AddressInput},
        orders::{self, OrderSummary},
        products::{self, Page},
        users,
    },
    error::{ApiError, ApiResult, FieldErrors},
    extract::{AppJson, AppPath, AppQuery},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/me/profile", get(get_profile).put(put_profile))
        .route(
            "/api/me/addresses",
            get(list_addresses).post(create_address),
        )
        .route(
            "/api/me/addresses/{id}",
            axum::routing::put(update_address).delete(delete_address),
        )
        .route("/api/me/orders", get(list_orders))
}

async fn get_profile(AuthUser(user): AuthUser) -> Json<Value> {
    Json(json!({ "user": user.public() }))
}

#[derive(Deserialize)]
pub struct ProfileBody {
    pub name: String,
    pub phone: Option<String>,
    pub current_password: Option<String>,
    pub new_password: Option<String>,
}

fn clean_opt(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 改名字、手機；要改密碼就要一起送 current_password（驗證）與 new_password
async fn put_profile(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    AppJson(body): AppJson<ProfileBody>,
) -> ApiResult<Json<Value>> {
    let mut errors = FieldErrors::new();
    let name = body.name.trim();
    let name_len = name.chars().count();
    if name_len == 0 || name_len > 50 {
        errors.add("name", "必填，最多 50 字");
    }
    let phone = clean_opt(&body.phone);
    if let Some(p) = &phone
        && !users::is_tw_mobile(p)
    {
        errors.add("phone", "手機格式：09 開頭共 10 碼");
    }
    let new_password = clean_opt(&body.new_password);
    if let Some(new) = &new_password {
        if new.chars().count() < MIN_PASSWORD_CHARS {
            errors.add("new_password", "密碼至少 8 碼");
        }
        let current = body.current_password.clone().unwrap_or_default();
        if !verify_password_async(current, user.password_hash.clone()).await? {
            errors.add("current_password", "目前密碼錯誤");
        }
    }
    errors.into_result()?;

    let updated = users::update_profile(&state.db, user.id, name, phone.as_deref()).await?;
    if let Some(new) = new_password {
        let hash = hash_password_async(new).await?;
        users::set_password(&state.db, user.id, &hash).await?;
    }
    Ok(Json(json!({ "user": updated.public() })))
}

async fn list_addresses(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<Address>>> {
    Ok(Json(addresses::list(&state.db, user.id).await?))
}

async fn create_address(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    AppJson(input): AppJson<AddressInput>,
) -> ApiResult<(StatusCode, Json<Address>)> {
    let address = addresses::create(&state.db, user.id, input).await?;
    Ok((StatusCode::CREATED, Json(address)))
}

async fn update_address(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppJson(input): AppJson<AddressInput>,
) -> ApiResult<Json<Address>> {
    Ok(Json(
        addresses::update(&state.db, user.id, id, input).await?,
    ))
}

async fn delete_address(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<StatusCode> {
    if addresses::delete(&state.db, user.id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[derive(Deserialize)]
pub struct OrdersQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

async fn list_orders(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    AppQuery(query): AppQuery<OrdersQuery>,
) -> ApiResult<Json<Page<OrderSummary>>> {
    let (page, per_page) = products::clamp_paging(query.page, query.per_page, 20, 50);
    Ok(Json(
        orders::list_for_user(&state.db, user.id, page, per_page).await?,
    ))
}
