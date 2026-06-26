use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use liquidity_engine::LiquidityEngine;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use shared::{DEMO_MIN_PAYOUT_USDT, PayoutMethod};
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

#[derive(Deserialize)]
struct SeedDemoQuery {
    #[serde(default = "default_seed_count")]
    count: u32,
}

fn default_seed_count() -> u32 {
    5
}

#[derive(Serialize)]
struct DevSeedResponse {
    user_id: Uuid,
    token: String,
    users_created: u32,
    watches_created: u32,
    notifications_created: u32,
    referrals_created: u32,
    pending_payouts_created: u32,
    message_de: String,
}

async fn seed_demo(
    State(state): State<AppState>,
    Query(query): Query<SeedDemoQuery>,
) -> Result<Json<DevSeedResponse>, StatusCode> {
    dev_guard()?;
    let count = query.count.clamp(1, 10);

    let mut user_ids = Vec::with_capacity(count as usize);
    let mut watches_created = 0u32;
    let mut notifications_created = 0u32;
    let mut referrals_created = 0u32;

    let referrer_id = Uuid::new_v4();
    state.ensure_user(referrer_id).await;
    let _ = state.gamification_on_register(referrer_id).await;
    user_ids.push(referrer_id);

    for _ in 1..count {
        let user_id = Uuid::new_v4();
        state.ensure_user(user_id).await;
        let _ = state.gamification_on_register(user_id).await;

        let mut profile = state.profile(user_id).await;
        profile.referred_by = Some(referrer_id);
        state
            .save_profile(user_id, &profile)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        referrals_created += 1;
        user_ids.push(user_id);
    }

    for (i, user_id) in user_ids.iter().enumerate() {
        let watch_count = 2 + (i as u32 % 3);
        for _ in 0..watch_count {
            let mut profile = state.profile(*user_id).await;
            profile.record_session();
            state
                .credit_watch_reward(*user_id, Decimal::new(1, 3), 0.0, &profile)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            state
                .save_profile(*user_id, &profile)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            watches_created += 1;
        }

        if i > 0 {
            let mut profile = state.profile(*user_id).await;
            state
                .maybe_apply_referral_bonuses(*user_id, &mut profile)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            state
                .save_profile(*user_id, &profile)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }

        if i < 3 {
            state
                .store
                .gamification_push_notification(
                    *user_id,
                    "system",
                    "Demo-Benachrichtigung",
                    "QA-Modus: Willkommen in der Demo-Umgebung.",
                )
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            state
                .store
                .gamification_push_notification(
                    *user_id,
                    "reward",
                    "Demo-Belohnung",
                    "Du hast Demo-Videos abgeschlossen.",
                )
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            notifications_created += 2;
        }
    }

    let payout_amount =
        Decimal::from_str_exact(DEMO_MIN_PAYOUT_USDT).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state
        .add_revenue(Decimal::ONE)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut pending_payouts_created = 0u32;
    for user_id in user_ids.iter().take(2) {
        state
            .set_trust_score(*user_id, 30)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let balance = state
            .balance(*user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if balance < payout_amount {
            let topup = payout_amount - balance;
            state
                .credit(*user_id, topup)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }

        let revenue = state.total_revenue().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let pending = state.pending_payouts().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let held = state.held_payouts().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        LiquidityEngine::can_payout(
            revenue,
            pending + held,
            AppState::liquidity_reserve_ratio(),
            payout_amount,
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let tier = state.payout_tier_for_usdt(payout_amount).await;
        let trust_score = state.trust_score(*user_id).await;
        let status = AppState::payout_status(tier, trust_score);
        let payout_id = Uuid::new_v4();

        state
            .debit(*user_id, payout_amount)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if AppState::payout_is_approved(status) {
            state
                .add_pending_payout(payout_amount)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        } else {
            state
                .add_held_payout(payout_amount)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        state
            .record_payout_request(
                payout_id,
                *user_id,
                payout_amount,
                tier.as_str(),
                status,
                PayoutMethod::Paypal.as_str(),
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let mut profile = state.profile(*user_id).await;
        profile.payout_history += 1;
        state
            .save_profile(*user_id, &profile)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        pending_payouts_created += 1;
    }

    let token = state
        .jwt
        .issue(referrer_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(DevSeedResponse {
        user_id: referrer_id,
        token,
        users_created: count,
        watches_created,
        notifications_created,
        referrals_created,
        pending_payouts_created,
        message_de: format!(
            "QA-Demo: {count} Nutzer, {watches_created} Videos, {referrals_created} Empfehlungen, {pending_payouts_created} Auszahlungen."
        ),
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
