use axum::{
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use shared::AppError;

pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match &self.0 {
            AppError::UserNotFound(_) => (StatusCode::NOT_FOUND, self.0.to_string()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.0.to_string()),
            AppError::AccountBanned => (StatusCode::FORBIDDEN, self.0.to_string()),
            AppError::AdminNotConfigured => (
                StatusCode::SERVICE_UNAVAILABLE,
                self.0.to_string(),
            ),
            AppError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, self.0.to_string()),
            AppError::InsufficientBalance { .. } => (StatusCode::BAD_REQUEST, self.0.to_string()),
            AppError::FraudBlocked(_) => (StatusCode::FORBIDDEN, self.0.to_string()),
            AppError::InvalidInput(_) => (StatusCode::BAD_REQUEST, self.0.to_string()),
            AppError::EmailAlreadyRegistered => (StatusCode::CONFLICT, self.0.to_string()),
            AppError::InsufficientLiquidity { .. } => {
                (StatusCode::SERVICE_UNAVAILABLE, self.0.to_string())
            }
            AppError::AiBlocked(_) => (StatusCode::FORBIDDEN, self.0.to_string()),
            AppError::AiUnavailable(_) => (StatusCode::BAD_GATEWAY, self.0.to_string()),
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

pub fn map_ai_error(err: ai_engine::AiError) -> AppError {
    match err {
        ai_engine::AiError::PromptInjection | ai_engine::AiError::ResponseRejected => {
            AppError::AiBlocked(err.to_string())
        }
        ai_engine::AiError::MessageTooLong { max } => {
            AppError::InvalidInput(format!("message exceeds {max} characters"))
        }
        _ => AppError::AiUnavailable(err.to_string()),
    }
}
