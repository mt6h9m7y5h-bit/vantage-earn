use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};

use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use shared::AppError;

/// Authenticated user extracted from `Authorization: Bearer <jwt>`.
pub struct AuthUser(pub Uuid);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized)?;

        let user_id = state.jwt.verify(token)?;
        Ok(AuthUser(user_id))
    }
}
