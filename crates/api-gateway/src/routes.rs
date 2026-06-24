use axum::{
    extract::State,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use fraud_engine::{FraudEngine, WatchSessionCheck};
use liquidity_engine::LiquidityEngine;
use referral_engine::ReferralEngine;
use reward_engine::RewardEngine;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use shared::{AppError, AppEvent, PayoutRequestedPayload, WatchCompletedPayload};
use uuid::Uuid;

use crate::error::{map_ai_error, ApiError};
use crate::extractors::AuthUser;
use crate::pwa;
use crate::rate_limit::{self, RateLimiter};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    let rate_limiter = RateLimiter::from_env();
    let auth_limiter = RateLimiter::auth_from_env();

    let public = Router::new()
        .route("/", get(pwa::root))
        .route("/health", get(health))
        .route("/demo", get(pwa::demo_page))
        .route("/manifest.webmanifest", get(pwa::manifest))
        .route("/sw.js", get(pwa::service_worker))
        .route("/icons/icon-180.png", get(pwa::icon_180))
        .route("/icons/icon-192.png", get(pwa::icon_192))
        .route("/icons/icon-512.png", get(pwa::icon_512))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .layer(middleware::from_fn_with_state(
            auth_limiter,
            rate_limit::middleware,
        ));

    let protected = Router::new()
        .route("/users/me/wallet", get(get_wallet))
        .route("/users/me/ledger", get(get_ledger))
        .route("/users/me/referral", get(get_referral))
        .route("/users/me/watch/complete", post(watch_complete))
        .route("/users/me/payout/request", post(payout_request))
        .route("/users/me/ai/context", get(ai_context))
        .route("/users/me/ai/chat", post(ai_chat))
        .layer(middleware::from_fn_with_state(
            rate_limiter,
            rate_limit::middleware,
        ));

    public.merge(protected)
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = state.store_healthy().await;
    Json(serde_json::json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "service": "vantage-earn",
        "version": "0.1.0",
        "database": db_ok,
    }))
}

#[derive(Deserialize)]
struct RegisterRequest {
    #[serde(default)]
    locale: Option<String>,
    #[serde(default)]
    referral_code: Option<String>,
}

#[derive(Deserialize)]
struct LoginRequest {
    user_id: Uuid,
}

#[derive(Serialize)]
struct AuthResponse {
    user_id: Uuid,
    token: String,
}

async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let user_id = Uuid::new_v4();
    state.ensure_user(user_id).await;

    let mut profile = state.profile(user_id).await;
    if let Some(locale) = body.locale {
        profile.locale = locale;
    }
    if let Some(code) = body.referral_code {
        if let Some(referrer) = state.find_user_by_referral_code(&code).await? {
            if referrer != user_id {
                profile.referred_by = Some(referrer);
            }
        }
    }
    state.save_profile(user_id, &profile).await?;

    let token = state.jwt.issue(user_id)?;
    Ok(Json(AuthResponse { user_id, token }))
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    if !state.user_exists(body.user_id).await {
        return Err(AppError::UserNotFound(body.user_id).into());
    }

    let token = state.jwt.issue(body.user_id)?;
    Ok(Json(AuthResponse {
        user_id: body.user_id,
        token,
    }))
}

#[derive(Serialize)]
struct WalletResponse {
    user_id: Uuid,
    balance_usdt: Decimal,
    localized_balance: Decimal,
    currency: String,
    trust_score: i32,
    payout_tier: String,
}

async fn get_wallet(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<WalletResponse>, ApiError> {
    let balance = state.balance(user_id).await?;
    let currency = state.local_currency_for_user(user_id).await;
    let localized = state
        .currency
        .usdt_to_local(balance, currency)
        .await
        .unwrap_or(balance);
    let tier = state.payout_tier_for_usdt(balance).await;
    let trust_score = state.trust_score(user_id).await;

    Ok(Json(WalletResponse {
        user_id,
        balance_usdt: balance,
        localized_balance: localized,
        currency: currency.code().into(),
        trust_score,
        payout_tier: tier.as_str().into(),
    }))
}

#[derive(Serialize)]
struct ReferralResponse {
    user_id: Uuid,
    referral_code: String,
    referral_count: i32,
}

async fn get_referral(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<ReferralResponse>, ApiError> {
    let profile = state.profile(user_id).await;
    Ok(Json(ReferralResponse {
        user_id,
        referral_code: ReferralEngine::code_for_user(user_id),
        referral_count: profile.referral_count,
    }))
}

async fn get_ledger(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<Vec<crate::store::LedgerItem>>, ApiError> {
    let entries = state.ledger(user_id).await?;
    Ok(Json(entries))
}

#[derive(Deserialize)]
struct WatchCompleteRequest {
    watch_duration_secs: u32,
    #[serde(default)]
    is_emulator: bool,
    #[serde(default)]
    is_vpn: bool,
}

#[derive(Serialize)]
struct WatchCompleteResponse {
    user_id: Uuid,
    reward_usdt: Decimal,
    message: String,
}

async fn watch_complete(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(body): Json<WatchCompleteRequest>,
) -> Result<Json<WatchCompleteResponse>, ApiError> {
    let mut profile = state.profile(user_id).await;

    let fraud_prob = FraudEngine::validate_watch(&WatchSessionCheck {
        watch_duration_secs: body.watch_duration_secs,
        sessions_last_hour: profile.sessions_last_hour,
        is_emulator: body.is_emulator,
        is_vpn: body.is_vpn,
    })?;

    let base_reward =
        RewardEngine::calculate_watch_reward(body.watch_duration_secs, profile.streak_days);
    let reward = (base_reward * FraudEngine::reward_multiplier(fraud_prob)).round_dp(6);
    if reward <= Decimal::ZERO {
        return Err(AppError::FraudBlocked("reward withheld".into()).into());
    }

    state
        .credit_watch_reward(user_id, reward, fraud_prob, &profile)
        .await?;

    state
        .maybe_apply_referral_bonuses(user_id, &mut profile)
        .await?;

    profile.record_session();
    state.save_profile(user_id, &profile).await?;

    let payload = WatchCompletedPayload {
        user_id,
        session_id: Uuid::new_v4(),
        watch_duration_secs: body.watch_duration_secs,
        reward_usdt: reward,
        occurred_at: Utc::now(),
    };
    state
        .events
        .publish(AppEvent::WatchCompleted(payload))
        .await;

    Ok(Json(WatchCompleteResponse {
        user_id,
        reward_usdt: reward,
        message: format!("+{reward} USDT credited"),
    }))
}

#[derive(Deserialize)]
struct PayoutRequest {
    amount_usdt: Decimal,
}

#[derive(Serialize)]
struct PayoutResponse {
    user_id: Uuid,
    amount_usdt: Decimal,
    tier: String,
    status: String,
    payout_id: Uuid,
}

async fn payout_request(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(body): Json<PayoutRequest>,
) -> Result<Json<PayoutResponse>, ApiError> {
    let balance = state.balance(user_id).await?;
    if body.amount_usdt > balance {
        return Err(AppError::InsufficientBalance {
            have: balance,
            need: body.amount_usdt,
        }
        .into());
    }

    let revenue = state.total_revenue().await?;
    let pending = state.pending_payouts().await?;
    let held = state.held_payouts().await?;
    let obligations = pending + held;
    LiquidityEngine::can_payout(
        revenue,
        obligations,
        AppState::liquidity_reserve_ratio(),
        body.amount_usdt,
    )?;

    let tier = state.payout_tier_for_usdt(body.amount_usdt).await;
    let trust_score = state.trust_score(user_id).await;
    let status = AppState::payout_status(tier, trust_score);
    let payout_id = Uuid::new_v4();

    state.debit(user_id, body.amount_usdt).await?;
    if AppState::payout_is_approved(status) {
        state.add_pending_payout(body.amount_usdt).await?;
    } else {
        state.add_held_payout(body.amount_usdt).await?;
    }
    state
        .record_payout_request(payout_id, user_id, body.amount_usdt, tier.as_str(), status)
        .await?;

    let mut profile = state.profile(user_id).await;
    profile.payout_history += 1;
    state.save_profile(user_id, &profile).await?;

    state
        .events
        .publish(AppEvent::PayoutRequested(PayoutRequestedPayload {
            user_id,
            amount_usdt: body.amount_usdt,
            tier: tier.as_str().into(),
            occurred_at: Utc::now(),
        }))
        .await;

    Ok(Json(PayoutResponse {
        user_id,
        amount_usdt: body.amount_usdt,
        tier: tier.as_str().into(),
        status: status.into(),
        payout_id,
    }))
}

async fn ai_context(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<SafeAIContextResponse>, ApiError> {
    let ctx = state.build_ai_context(user_id).await;
    let prompt = ai_engine::build_system_prompt(&ctx);
    Ok(Json(SafeAIContextResponse {
        context: ctx,
        system_prompt_preview: prompt,
    }))
}

#[derive(Serialize)]
struct SafeAIContextResponse {
    context: shared::SafeAIContext,
    system_prompt_preview: String,
}

#[derive(Deserialize)]
struct AiChatRequest {
    message: String,
}

#[derive(Serialize)]
struct AiChatResponse {
    reply: String,
}

async fn ai_chat(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(body): Json<AiChatRequest>,
) -> Result<Json<AiChatResponse>, ApiError> {
    let ctx = state.build_ai_context(user_id).await;

    let reply = state
        .copilot
        .chat(&ctx, &body.message)
        .await
        .map_err(map_ai_error)?;

    Ok(Json(AiChatResponse { reply }))
}
