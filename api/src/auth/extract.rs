use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

use crate::{
    auth::{cookie, session},
    domain::users::User,
    error::ApiError,
    state::AppState,
};

/// 有登入就是 Some(user)，沒登入是 None；只在資料庫壞掉時才會失敗
pub struct CurrentUser(pub Option<User>);

/// 一定要登入，否則 401
pub struct AuthUser(pub User);

/// 一定要是 admin：沒登入 401、不是 admin 403
pub struct AdminUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(raw) = cookie::get_cookie(&parts.headers, cookie::SESSION_COOKIE) else {
            return Ok(Self(None));
        };
        let Ok(sid) = Uuid::parse_str(&raw) else {
            return Ok(Self(None));
        };
        let user = session::find_user(&state.db, sid).await?;
        Ok(Self(user))
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match CurrentUser::from_request_parts(parts, state).await?.0 {
            Some(user) => Ok(Self(user)),
            None => Err(ApiError::Unauthorized("請先登入")),
        }
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        if user.is_admin() {
            Ok(Self(user))
        } else {
            Err(ApiError::Forbidden("需要管理員權限"))
        }
    }
}
