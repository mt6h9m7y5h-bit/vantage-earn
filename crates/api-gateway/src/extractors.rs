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
        let user_id = optional_auth_user_id(parts, state)?
            .ok_or(AppError::Unauthorized)?;
        Ok(AuthUser(user_id))
    }
}

/// Optional authenticated user — `None` when no `Authorization` header is sent.
pub struct OptionalAuthUser(pub Option<Uuid>);

impl FromRequestParts<AppState> for OptionalAuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(OptionalAuthUser(optional_auth_user_id(parts, state)?))
    }
}

fn optional_auth_user_id(
    parts: &Parts,
    state: &AppState,
) -> Result<Option<Uuid>, ApiError> {
    let Some(header) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return Ok(None);
    };

    let token = header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)?;

    let user_id = state.jwt.verify(token)?;
    Ok(Some(user_id))
}
