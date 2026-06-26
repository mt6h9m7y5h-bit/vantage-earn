use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use crate::release_info;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dev/seed-demo", post(seed_demo))
        .route("/dev/reset", post(reset_dev))
}

fn dev_guard() -> Result<(), StatusCode> {
    if release_info::is_production() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(())
}

#[derive(Serialize)]
struct DevSeedResponse {
    user_id: Uuid,
    token: String,
    watches_created: u32,
    notifications_created: u32,
    message_de: String,
}

async fn seed_demo(State(state): State<AppState>) -> Result<Json<DevSeedResponse>, StatusCode> {
    dev_guard()?;
    let user_id = Uuid::new_v4();
    state.ensure_user(user_id).await;
    let token = state.jwt.issue(user_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = state.gamification_on_register(user_id).await;

    let mut watches_created = 0u32;
    for _ in 0..3 {
        let mut profile = state.profile(user_id).await;
        profile.record_session();
        state
            .credit_watch_reward(user_id, Decimal::new(1, 3), 0.0, &profile)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        state
            .save_profile(user_id, &profile)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        watches_created += 1;
    }

    state
        .store
        .gamification_push_notification(
            user_id,
            "system",
            "Demo-Benachrichtigung",
            "QA-Modus: Willkommen in der Demo-Umgebung.",
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state
        .store
        .gamification_push_notification(
            user_id,
            "reward",
            "Demo-Belohnung",
            "Du hast 3 Demo-Videos abgeschlossen.",
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(DevSeedResponse {
        user_id,
        token,
        watches_created,
        notifications_created: 2,
        message_de: "Demo-Nutzer mit Beispieldaten erstellt.".into(),
    }))
}

#[derive(Serialize)]
struct DevResetResponse {
    ok: bool,
    message_de: String,
}

async fn reset_dev(State(state): State<AppState>) -> Result<Json<DevResetResponse>, StatusCode> {
    dev_guard()?;
    state
        .dev_reset_store()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(DevResetResponse {
        ok: true,
        message_de: "In-Memory-Daten zurückgesetzt (nur Entwicklung).".into(),
    }))
}
