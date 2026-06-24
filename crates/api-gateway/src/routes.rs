use axum::{
    extract::State,
    http::HeaderMap,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use fraud_engine::{FraudEngine, WatchSessionCheck, MAX_WATCHES_PER_DAY};
use liquidity_engine::LiquidityEngine;
use referral_engine::ReferralEngine;
use reward_engine::{
    BonusCatalogItem, BonusEarned, BonusEngine, DAILY_CHALLENGE_TARGET, RewardEngine,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use shared::{
    AppError, AppEvent, PayoutMethod, PayoutMethodInfo, PayoutRequestedPayload,
    PAYOUT_FIRST_TIME_NOTE_DE, WatchCompletedPayload,
};
use uuid::Uuid;

use crate::ad_config::AdConfig;
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
        .route("/config", get(public_config))
        .route("/demo", get(pwa::demo_page))
        .route("/manifest.webmanifest", get(pwa::manifest))
        .route("/sw.js", get(pwa::service_worker))
        .route("/icons/icon-180.png", get(pwa::icon_180))
        .route("/icons/icon-192.png", get(pwa::icon_192))
        .route("/icons/icon-512.png", get(pwa::icon_512))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/leaderboard/weekly", get(weekly_leaderboard))
        .route("/admin", get(pwa::admin_page))
        .route("/admin/stats", get(admin_stats))
        .layer(middleware::from_fn_with_state(
            auth_limiter,
            rate_limit::middleware,
        ));

    let protected = Router::new()
        .route("/users/me/wallet", get(get_wallet))
        .route("/users/me/ledger", get(get_ledger))
        .route("/users/me/referral", get(get_referral))
        .route("/users/me/stats", get(get_stats))
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

async fn public_config() -> impl IntoResponse {
    Json(AdConfig::default().public_json())
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

#[derive(Serialize)]
struct UserStatsResponse {
    streak_days: i32,
    streak_bonus_percent: u32,
    referral_count: i32,
    watches_today: u32,
    watches_remaining_today: u32,
    total_watches: u32,
    next_milestone: Option<u32>,
    milestones_claimed: u8,
    daily_bonus_claimed_today: bool,
    reward_estimate_30s: Decimal,
    reward_estimate_60s: Decimal,
    min_payout_usdt: Decimal,
    min_payout_eur: Decimal,
    payout_demo_mode: bool,
    payout_methods: Vec<&'static str>,
    payout_method_info: Vec<PayoutMethodInfo>,
    payout_first_time_note_de: &'static str,
    challenge_watches_today: u32,
    challenge_target: u32,
    daily_challenge_completed_today: bool,
    bonus_catalog: Vec<BonusCatalogItem>,
}

async fn get_stats(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<UserStatsResponse>, ApiError> {
    let profile = state.profile(user_id).await;
    let watches_today = profile.effective_watches_today();
    let remaining = MAX_WATCHES_PER_DAY.saturating_sub(watches_today);
    let min_payout = state.min_payout_usdt().await;
    let streak_bonus_percent = RewardEngine::streak_bonus_percent(profile.streak_days);
    let daily_bonus_claimed_today = AppState::daily_bonus_claimed_today(&profile);
    let daily_challenge_completed_today = AppState::challenge_bonus_claimed_today(&profile);
    let next_milestone =
        BonusEngine::next_milestone(profile.total_watches, profile.milestones_claimed);
    let bonus_catalog = BonusEngine::build_catalog(
        streak_bonus_percent,
        profile.total_watches,
        profile.milestones_claimed,
        daily_bonus_claimed_today,
        profile.streak_days,
        profile.streak_7_bonus_claimed,
        watches_today,
        daily_challenge_completed_today,
    );

    Ok(Json(UserStatsResponse {
        streak_days: profile.streak_days,
        streak_bonus_percent,
        referral_count: profile.referral_count,
        watches_today,
        watches_remaining_today: remaining,
        total_watches: profile.total_watches,
        next_milestone,
        milestones_claimed: profile.milestones_claimed,
        daily_bonus_claimed_today,
        reward_estimate_30s: RewardEngine::calculate_watch_reward(30, profile.streak_days),
        reward_estimate_60s: RewardEngine::calculate_watch_reward(60, profile.streak_days),
        min_payout_usdt: min_payout,
        min_payout_eur: AppState::min_payout_eur(),
        payout_demo_mode: AppState::payout_demo_mode(),
        payout_methods: AppState::payout_methods(),
        payout_method_info: PayoutMethod::all_info(),
        payout_first_time_note_de: PAYOUT_FIRST_TIME_NOTE_DE,
        challenge_watches_today: watches_today,
        challenge_target: DAILY_CHALLENGE_TARGET,
        daily_challenge_completed_today: daily_challenge_completed_today,
        bonus_catalog,
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
    /// Client-reported ad provider (`mock` | `applixir`). Reserved for future SSV validation.
    #[serde(default)]
    ad_provider: Option<String>,
    /// AppLixir transaction / session id from ad completion callback (future SSV stub).
    #[serde(default)]
    ad_session_id: Option<String>,
}

#[derive(Serialize)]
struct WatchCompleteResponse {
    user_id: Uuid,
    reward_usdt: Decimal,
    base_reward_usdt: Decimal,
    bonuses: Vec<BonusEarned>,
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
        watches_today: profile.effective_watches_today(),
        is_emulator: body.is_emulator,
        is_vpn: body.is_vpn,
    })?;

    let is_first_watch_today = profile.effective_watches_today() == 0;
    profile.record_session();

    let today = Utc::now().date_naive();
    let watch_index = profile.total_watches + 1;

    let base_reward =
        RewardEngine::calculate_watch_reward(body.watch_duration_secs, profile.streak_days);
    let fraud_mult = FraudEngine::reward_multiplier(fraud_prob);
    let base_with_fraud = (base_reward * fraud_mult).round_dp(6);
    let (after_surprise, surprise_mult) =
        BonusEngine::apply_surprise(base_reward, user_id, today, watch_index);
    let watch_reward = (after_surprise * fraud_mult).round_dp(6);
    if watch_reward <= Decimal::ZERO {
        return Err(AppError::FraudBlocked("reward withheld".into()).into());
    }
    let surprise_extra = watch_reward - base_with_fraud;

    state
        .credit_watch_reward(user_id, watch_reward, fraud_prob, &profile)
        .await?;

    let mut bonus_result = AppState::apply_watch_bonuses(&mut profile, is_first_watch_today);
    bonus_result.surprise_multiplier = surprise_mult;
    bonus_result.surprise_extra_usdt = surprise_extra.max(Decimal::ZERO);

    if !bonus_result.flat_bonuses.is_empty() {
        state
            .credit_bonus_rewards(user_id, &bonus_result.flat_bonuses)
            .await?;
    }

    state
        .maybe_apply_referral_bonuses(user_id, &mut profile)
        .await?;

    state.save_profile(user_id, &profile).await?;

    let flat_total = bonus_result.flat_total();
    let total_reward = watch_reward + flat_total;

    let session_id = body
        .ad_session_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::new_v4);
    let _ad_provider = body.ad_provider.as_deref();

    let payload = WatchCompletedPayload {
        user_id,
        session_id,
        watch_duration_secs: body.watch_duration_secs,
        reward_usdt: total_reward,
        occurred_at: Utc::now(),
    };
    state
        .events
        .publish(AppEvent::WatchCompleted(payload))
        .await;

    let mut bonus_lines: Vec<String> = Vec::new();
    if let Some(mult) = bonus_result.surprise_multiplier {
        bonus_lines.push(format!("Überraschung {mult}× (+{surprise_extra} USDT)"));
    }
    for b in &bonus_result.flat_bonuses {
        bonus_lines.push(format!("{} +{}", b.title, b.amount_usdt));
    }
    let message = if bonus_lines.is_empty() {
        format!("+{watch_reward} USDT gutgeschrieben")
    } else {
        format!(
            "+{total_reward} USDT gesamt (Video {watch_reward}; {})",
            bonus_lines.join(", ")
        )
    };

    let mut bonuses = bonus_result.flat_bonuses;
    if bonus_result.surprise_multiplier.is_some() {
        bonuses.insert(
            0,
            BonusEarned {
                id: "surprise".into(),
                title: format!(
                    "Überraschung {}×",
                    bonus_result.surprise_multiplier.unwrap()
                ),
                amount_usdt: bonus_result.surprise_extra_usdt,
            },
        );
    }

    Ok(Json(WatchCompleteResponse {
        user_id,
        reward_usdt: total_reward,
        base_reward_usdt: watch_reward,
        bonuses,
        message,
    }))
}

#[derive(Deserialize)]
struct PayoutRequest {
    amount_usdt: Decimal,
    payout_method: String,
}

#[derive(Serialize)]
struct PayoutResponse {
    user_id: Uuid,
    amount_usdt: Decimal,
    payout_method: String,
    tier: String,
    status: String,
    estimated_time_de: String,
    payout_id: Uuid,
}

async fn payout_request(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(body): Json<PayoutRequest>,
) -> Result<Json<PayoutResponse>, ApiError> {
    let Some(method) = PayoutMethod::parse(&body.payout_method) else {
        return Err(AppError::InvalidInput(format!(
            "invalid payout_method; allowed: {}",
            PayoutMethod::all_strings().join(", ")
        ))
        .into());
    };

    let balance = state.balance(user_id).await?;
    let min_payout = state.min_payout_usdt().await;
    if body.amount_usdt < min_payout {
        return Err(AppError::InvalidInput(format!(
            "minimum payout is {min_payout} USDT ({} EUR equivalent)",
            AppState::min_payout_eur()
        ))
        .into());
    }
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
        .record_payout_request(
            payout_id,
            user_id,
            body.amount_usdt,
            tier.as_str(),
            status,
            method.as_str(),
        )
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
        payout_method: method.as_str().into(),
        tier: tier.as_str().into(),
        status: status.into(),
        estimated_time_de: method.info().estimated_time_de,
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

#[derive(Serialize)]
struct LeaderboardEntry {
    rank: u32,
    display_name: String,
    weekly_earnings_usdt: Decimal,
}

#[derive(Serialize)]
struct WeeklyLeaderboardResponse {
    week_start: DateTime<Utc>,
    entries: Vec<LeaderboardEntry>,
}

fn anonymize_user(user_id: Uuid) -> String {
    let hex = user_id.to_string().replace('-', "").to_uppercase();
    let short: String = hex.chars().take(4).collect();
    format!("User #{short}")
}

async fn weekly_leaderboard(
    State(state): State<AppState>,
) -> Result<Json<WeeklyLeaderboardResponse>, ApiError> {
    let ranked = state.weekly_leaderboard().await?;
    let entries = ranked
        .into_iter()
        .enumerate()
        .map(|(i, (user_id, earnings))| LeaderboardEntry {
            rank: (i + 1) as u32,
            display_name: anonymize_user(user_id),
            weekly_earnings_usdt: earnings,
        })
        .collect();

    Ok(Json(WeeklyLeaderboardResponse {
        week_start: crate::store::week_start_utc(),
        entries,
    }))
}

#[derive(Serialize)]
struct AdminStatsResponse {
    total_revenue: Decimal,
    pending_payouts: Decimal,
    held_payouts: Decimal,
    user_count: i64,
    recent_payout_count: i64,
}

async fn admin_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AdminStatsResponse>, ApiError> {
    let secret = headers
        .get("X-Admin-Secret")
        .and_then(|v| v.to_str().ok());
    AppState::verify_admin_secret(secret)?;

    let total_revenue = state.total_revenue().await?;
    let pending_payouts = state.pending_payouts().await?;
    let held_payouts = state.held_payouts().await?;
    let user_count = state.user_count().await?;
    let recent_payout_count = state.recent_payout_count().await?;

    Ok(Json(AdminStatsResponse {
        total_revenue,
        pending_payouts,
        held_payouts,
        user_count,
        recent_payout_count,
    }))
}
