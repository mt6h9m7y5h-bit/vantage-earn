use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, patch},
    Json, Router,
};
use shared::AppError;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use crate::store::{
    valid_announcement_type, AnnouncementCreate, AnnouncementPatch, AnnouncementRow,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/announcements/active", get(list_active_announcements))
        .route(
            "/admin/announcements",
            get(admin_list_announcements).post(admin_create_announcement),
        )
        .route("/admin/announcements/{id}", patch(admin_patch_announcement))
}

async fn list_active_announcements(
    State(state): State<AppState>,
) -> Result<Json<Vec<AnnouncementRow>>, ApiError> {
    Ok(Json(state.list_active_announcements().await?))
}

async fn admin_list_announcements(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AnnouncementRow>>, ApiError> {
    crate::admin::verify_admin_headers(&headers)?;
    Ok(Json(state.list_all_announcements().await?))
}

async fn admin_create_announcement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AnnouncementCreate>,
) -> Result<Json<AnnouncementRow>, ApiError> {
    crate::admin::verify_admin_headers(&headers)?;
    if !valid_announcement_type(&body.announcement_type) {
        return Err(AppError::InvalidInput("invalid announcement type".into()).into());
    }
    if body.title.trim().is_empty() || body.body.trim().is_empty() {
        return Err(AppError::InvalidInput("title and body are required".into()).into());
    }
    let row = state.create_announcement(body).await?;
    state
        .admin_log_enriched(
            &headers,
            "announcement_create",
            None,
            serde_json::json!({
                "announcement_id": row.id,
                "type": row.announcement_type,
                "title": row.title,
            }),
        )
        .await?;
    Ok(Json(row))
}

async fn admin_patch_announcement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(patch): Json<AnnouncementPatch>,
) -> Result<Json<AnnouncementRow>, ApiError> {
    crate::admin::verify_admin_headers(&headers)?;
    if let Some(ref t) = patch.announcement_type {
        if !valid_announcement_type(t) {
            return Err(AppError::InvalidInput("invalid announcement type".into()).into());
        }
    }
    let before = state
        .get_announcement(id)
        .await?
        .ok_or_else(|| AppError::InvalidInput("announcement not found".into()))?;
    let after = state.patch_announcement(id, patch).await?;
    state
        .admin_log_enriched(
            &headers,
            "announcement_update",
            None,
            serde_json::json!({
                "announcement_id": id,
                "before": before,
                "after": after,
            }),
        )
        .await?;
    Ok(Json(after))
}
