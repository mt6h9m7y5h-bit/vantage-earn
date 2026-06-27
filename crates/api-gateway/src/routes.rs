use axum::{
    extract::{Path, Query, State},
    middleware,
    routing::{get, patch, post},
    Json, Router,
};

use chrono::{DateTime, NaiveDate, Utc};
use fraud_engine::{FraudEngine, WatchSessionCheck, MAX_WATCHES_PER_DAY};
use liquidity_engine::LiquidityEngine;
use referral_engine::ReferralEngine;
use reward_engine::{
    BonusCatalogItem, BonusEarned, BonusEngine, CatalogInput, DAILY_CHALLENGE_TARGET,
    RewardEngine,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use shared::{
    AppError, AppEvent, PayoutMethod, PayoutMethodInfo, PayoutRequestedPayload,
    PAYOUT_FIRST_TIME_NOTE_DE, WatchCompletedPayload,
};
use uuid::Uuid;

use crate::ad_config::AdConfig;
use crate::bitlabs::{self, BitlabsConfig};
use crate::error::{map_ai_error, ApiError};
use std::collections::HashMap;

use crate::announcements;
use crate::dev;
use crate::extractors::{AuthUser, OptionalAuthUser};
use crate::auth::{hash_password, normalize_email, validate_password, verify_password};
use crate::health;
use crate::pwa;
use crate::rate_limit::{self, RateLimiter};
use crate::state::{AppState, UserProfile};
use crate::top_offers::{self, TopOffer};
use crate::video_offers::{self, VideoOffer, VideoOfferTier};

pub fn router() -> Router<AppState> {
    let rate_limiter = RateLimiter::from_env();
    let auth_limiter = RateLimiter::auth_from_env();

    let public = Router::new()
        .route("/", get(pwa::root))
        .route("/health", get(health::public_health))
        .route("/config", get(public_config))
        .route("/demo", get(pwa::demo_page))
        .route("/legal/datenschutz", get(pwa::datenschutz_page))
        .route("/legal/impressum", get(pwa::impressum_page))
        .route("/legal/agb", get(pwa::agb_page))
        .route("/manifest.webmanifest", get(pwa::manifest))
        .route("/sw.js", get(pwa::service_worker))
        .route("/icons/icon-180.png", get(pwa::icon_180))
        .route("/icons/icon-192.png", get(pwa::icon_192))
        .route("/icons/icon-512.png", get(pwa::icon_512))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/forgot-password", post(forgot_password))
        .route("/auth/reset-password", post(reset_password))
        .route(
            "/webhooks/bitlabs",
            get(bitlabs::webhook).post(bitlabs::webhook),
        )
        .route("/leaderboard/weekly", get(weekly_leaderboard))
        .route("/admin", get(pwa::admin_page))
        .merge(crate::admin::router())
        .merge(announcements::router())
        .merge(dev::router())
        .layer(middleware::from_fn_with_state(
            auth_limiter,
            rate_limit::middleware,
        ));

    let protected = Router::new()
        .route("/users/me/wallet", get(get_wallet))
        .route("/users/me/ledger", get(get_ledger))
        .route("/users/me/wallet/history", get(get_wallet_history))
        .route("/users/me/referral", get(get_referral))
        .route("/users/me/referrals/dashboard", get(get_referrals_dashboard))
        .route("/users/me/stats", get(get_stats))
        .route("/users/me/video-offers", get(get_video_offers))
        .route("/users/me/top-offers", get(get_top_offers))
        .route("/users/me/profile-stats", get(get_profile_stats))
        .route("/users/me/missions", get(get_missions))
        .route("/users/me/missions/{id}/claim", post(claim_mission))
        .route("/users/me/achievements", get(get_achievements))
        .route("/users/me/notifications", get(get_notifications))
        .route("/users/me/notifications/read-all", patch(mark_all_notifications_read))
        .route("/users/me/notifications/{id}", patch(mark_notification_read))
        .route("/users/me/onboarding/complete", post(complete_onboarding))
        .route("/users/me/payouts", get(get_user_payouts))
        .route("/users/me/analytics/summary", get(get_analytics_summary))
        .route("/users/me/watch/complete", post(watch_complete))
        .route("/users/me/payout/request", post(payout_request))
        .route("/users/me/ai/context", get(ai_context))
        .route("/users/me/ai/chat", post(ai_chat))
        .layer(middleware::from_fn_with_state(
            rate_limiter.clone(),
            rate_limit::middleware,
        ));

    let api_v1 = Router::new()
        .route("/health", get(health::public_health))
        .route("/config", get(public_config))
        .route("/users/me/profile-stats", get(get_profile_stats))
        .layer(middleware::from_fn_with_state(
            rate_limiter.clone(),
            rate_limit::middleware,
        ));

    public
        .merge(protected)
        .nest("/api/v1", api_v1)
        .fallback(pwa::fallback_handler)
}

async fn public_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let mut json = AdConfig::default().public_json();
    let flags = state.feature_flags_view().await?;
    if let Some(obj) = json.as_object_mut() {
        obj.insert(
            "watch_duration_secs".to_string(),
            serde_json::json!(flags.watch_duration_secs),
        );
        obj.insert(
            "maintenance_mode".to_string(),
            serde_json::json!(flags.maintenance_mode),
        );
        obj.insert(
            "maintenance_message".to_string(),
            serde_json::json!(flags.maintenance_message),
        );
        obj.insert("offerwall".to_string(), BitlabsConfig::from_env().public_json());
    }
    Ok(Json(json))
}

#[derive(Deserialize)]
struct RegisterRequest {
    #[serde(default)]
    locale: Option<String>,
    #[serde(default)]
    referral_code: Option<String>,
    #[serde(default)]
    accept_terms: Option<bool>,
    #[serde(default)]
    accept_age_minimum: Option<bool>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct AuthResponse {
    user_id: Uuid,
    token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    welcome_bonus_usdt: Option<f64>,
}

async fn register(
    State(state): State<AppState>,
    OptionalAuthUser(link_user_id): OptionalAuthUser,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let has_credentials = body.email.is_some() || body.password.is_some();
    if has_credentials {
        return register_with_credentials(state, link_user_id, body).await;
    }

    if body.accept_terms != Some(true) {
        return Err(shared::AppError::InvalidInput(
            "accept_terms must be true".into(),
        )
        .into());
    }
    if body.accept_age_minimum != Some(true) {
        return Err(shared::AppError::InvalidInput(
            "accept_age_minimum must be true".into(),
        )
        .into());
    }

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

    let _ = state.gamification_on_register(user_id).await;

    let token = state.jwt.issue(user_id)?;
    Ok(Json(AuthResponse {
        user_id,
        token,
        email: None,
        welcome_bonus_usdt: None,
    }))
}

async fn register_with_credentials(
    state: AppState,
    link_user_id: Option<Uuid>,
    body: RegisterRequest,
) -> Result<Json<AuthResponse>, ApiError> {
    if body.accept_terms != Some(true) {
        return Err(shared::AppError::InvalidInput(
            "accept_terms must be true".into(),
        )
        .into());
    }
    if body.accept_age_minimum != Some(true) {
        return Err(shared::AppError::InvalidInput(
            "accept_age_minimum must be true".into(),
        )
        .into());
    }

    let email_raw = body.email.as_deref().ok_or_else(|| {
        ApiError(shared::AppError::InvalidInput("email is required".into()))
    })?;
    let password = body.password.as_deref().ok_or_else(|| {
        ApiError(shared::AppError::InvalidInput("password is required".into()))
    })?;
    let email = normalize_email(email_raw).ok_or_else(|| {
        ApiError(shared::AppError::InvalidInput("invalid email".into()))
    })?;
    validate_password(password)?;

    if state.find_user_by_email(&email).await?.is_some() {
        return Err(shared::AppError::EmailAlreadyRegistered.into());
    }

    let password_hash = hash_password(password)?;
    let (user_id, is_new) = if let Some(existing_id) = link_user_id {
        if !state.user_exists(existing_id).await {
            return Err(AppError::UserNotFound(existing_id).into());
        }
        state
            .set_user_credentials(existing_id, &email, &password_hash)
            .await?;
        (existing_id, false)
    } else {
        let user_id = Uuid::new_v4();
        state.ensure_user(user_id).await;
        state
            .set_user_credentials(user_id, &email, &password_hash)
            .await?;
        (user_id, true)
    };

    let mut profile = state.profile(user_id).await;
    if let Some(locale) = &body.locale {
        profile.locale = locale.clone();
    }
    if is_new {
        if let Some(code) = body.referral_code {
            if let Some(referrer) = state.find_user_by_referral_code(&code).await? {
                if referrer != user_id {
                    profile.referred_by = Some(referrer);
                }
            }
        }
        state.save_profile(user_id, &profile).await?;
        let _ = state.gamification_on_register(user_id).await;
    } else if body.locale.is_some() {
        state.save_profile(user_id, &profile).await?;
    }

    let welcome_bonus = state.try_grant_early_adopter_bonus(user_id).await?;
    state.send_registration_welcome_email(&email, welcome_bonus);

    let token = state.jwt.issue(user_id)?;
    Ok(Json(AuthResponse {
        user_id,
        token,
        email: Some(email),
        welcome_bonus_usdt: welcome_bonus.and_then(|a| a.to_string().parse().ok()),
    }))
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let email = normalize_email(&body.email)
        .ok_or_else(|| AppError::from(AppError::InvalidInput("invalid email".into())))?;

    let Some((user_id, password_hash)) = state.find_user_by_email(&email).await? else {
        return Err(AppError::Unauthorized.into());
    };

    if !verify_password(&body.password, &password_hash) {
        return Err(AppError::Unauthorized.into());
    }

    let _ = state.gamification_on_login(user_id).await;

    let token = state.jwt.issue(user_id)?;
    Ok(Json(AuthResponse {
        user_id,
        token,
        email: Some(email),
        welcome_bonus_usdt: None,
    }))
}

#[derive(Deserialize)]
struct ForgotPasswordRequest {
    email: String,
}

#[derive(Serialize)]
struct MessageResponse {
    message: &'static str,
}

const FORGOT_PASSWORD_MESSAGE: &str =
    "Falls ein Konto mit dieser E-Mail existiert, senden wir dir einen Link zum Zurücksetzen.";

async fn forgot_password(
    State(state): State<AppState>,
    Json(body): Json<ForgotPasswordRequest>,
) -> Result<Json<MessageResponse>, ApiError> {
    if let Some(email) = normalize_email(&body.email) {
        if let Some((user_id, _)) = state.find_user_by_email(&email).await? {
            if let Ok(reset_token) = state.create_password_reset_token(user_id).await {
                state.send_password_reset_email(&email, &reset_token);
            }
        }
    }
    Ok(Json(MessageResponse {
        message: FORGOT_PASSWORD_MESSAGE,
    }))
}

#[derive(Deserialize)]
struct ResetPasswordRequest {
    token: String,
    password: String,
}

async fn reset_password(
    State(state): State<AppState>,
    Json(body): Json<ResetPasswordRequest>,
) -> Result<Json<MessageResponse>, ApiError> {
    validate_password(&body.password)?;
    if body.token.trim().is_empty() {
        return Err(shared::AppError::InvalidInput("token is required".into()).into());
    }
    state
        .reset_password_with_token(body.token.trim(), &body.password)
        .await?;
    Ok(Json(MessageResponse {
        message: "Passwort wurde aktualisiert. Du kannst dich jetzt anmelden.",
    }))
}

#[derive(Serialize)]
struct WalletResponse {
    user_id: Uuid,
    #[serde(with = "rust_decimal::serde::float")]
    balance_usdt: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    localized_balance: Decimal,
    currency: String,
    trust_score: i32,
    payout_tier: String,
}

async fn get_wallet(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<WalletResponse>, ApiError> {
    state.ensure_user(user_id).await;
    let balance = state.balance(user_id).await?;
    let currency = state.local_currency_for_user(user_id).await;
    let localized = state
        .currency
        .usdt_to_local(balance, currency)
        .await
        .unwrap_or_else(|| balance.round_dp(2));
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
    user_id: Uuid,
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
    video_offers: Vec<VideoOffer>,
    top_offers: Vec<TopOffer>,
}

async fn build_user_stats(
    state: &AppState,
    profile: &UserProfile,
    user_id: Uuid,
) -> UserStatsResponse {
    let watches_today = profile.effective_watches_today();
    let remaining = MAX_WATCHES_PER_DAY.saturating_sub(watches_today);
    let min_payout = state.min_payout_usdt().await;
    let streak_bonus_percent = RewardEngine::streak_bonus_percent(profile.streak_days);
    let daily_bonus_claimed_today = AppState::daily_bonus_claimed_today(profile);
    let daily_challenge_completed_today = AppState::challenge_bonus_claimed_today(profile);
    let next_milestone =
        BonusEngine::next_milestone(profile.total_watches, profile.milestones_claimed);
    let bonus_catalog = BonusEngine::build_catalog(CatalogInput {
        streak_bonus_percent,
        total_watches: profile.total_watches,
        milestones_claimed: profile.milestones_claimed,
        daily_bonus_claimed_today,
        streak_days: profile.streak_days,
        streak_7_bonus_claimed: profile.streak_7_bonus_claimed,
        challenge_watches_today: watches_today,
        challenge_bonus_claimed_today: daily_challenge_completed_today,
    });
    let today = Utc::now().date_naive();
    let video_offers = video_offers::offers_for_user(user_id, profile.streak_days, today).await;
    let top_offers = top_offers::offers_for_user();

    UserStatsResponse {
        user_id,
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
        daily_challenge_completed_today,
        bonus_catalog,
        video_offers,
        top_offers,
    }
}

async fn get_stats(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<UserStatsResponse>, ApiError> {
    let profile = state.profile(user_id).await;
    Ok(Json(build_user_stats(&state, &profile, user_id).await))
}

#[derive(Serialize)]
struct VideoOffersResponse {
    offers: Vec<VideoOffer>,
}

async fn get_video_offers(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<VideoOffersResponse>, ApiError> {
    let profile = state.profile(user_id).await;
    let today = Utc::now().date_naive();
    let offers = video_offers::offers_for_user(user_id, profile.streak_days, today).await;
    Ok(Json(VideoOffersResponse { offers }))
}

#[derive(Serialize)]
struct TopOffersResponse {
    offers: Vec<TopOffer>,
}

async fn get_top_offers(
    AuthUser(_user_id): AuthUser,
) -> Result<Json<TopOffersResponse>, ApiError> {
    Ok(Json(TopOffersResponse {
        offers: top_offers::offers_for_user(),
    }))
}

async fn get_ledger(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<Vec<crate::store::LedgerItem>>, ApiError> {
    let entries = state.ledger(user_id).await?;
    Ok(Json(entries))
}

#[derive(Serialize)]
struct DailyEarnings {
    date: NaiveDate,
    usdt: Decimal,
    watch_count: u32,
}

#[derive(Serialize)]
struct AnalyticsConversionHints {
    avg_usdt_per_watch: Option<Decimal>,
    avg_usdt_per_active_day_7d: Decimal,
    active_days_7d: u32,
}

#[derive(Serialize)]
struct AnalyticsSummaryResponse {
    earnings_last_7_days: Vec<DailyEarnings>,
    earnings_last_30_days: Decimal,
    earnings_last_7_days_total: Decimal,
    earnings_today: Decimal,
    daily_earnings_30d: Vec<DailyEarnings>,
    total_watches: u32,
    watches_today: u32,
    streak_days: i32,
    referral_count: i32,
    conversion_hints: AnalyticsConversionHints,
}

fn build_analytics_summary(
    ledger: &[crate::store::LedgerItem],
    profile: &UserProfile,
) -> AnalyticsSummaryResponse {
    let today = Utc::now().date_naive();
    let mut by_date: HashMap<NaiveDate, (Decimal, u32)> = HashMap::new();

    for entry in ledger {
        if entry.kind != "credit" {
            continue;
        }
        let day = entry.created_at.date_naive();
        let slot = by_date.entry(day).or_insert((Decimal::ZERO, 0));
        slot.0 += entry.amount_usdt;
        slot.1 += 1;
    }

    let watches_today = profile.effective_watches_today();
    if watches_today > 0 {
        let slot = by_date.entry(today).or_insert((Decimal::ZERO, 0));
        slot.1 = slot.1.max(watches_today);
    }

    let daily_series = |days: i64| -> Vec<DailyEarnings> {
        (0..days)
            .map(|offset| {
                let date = today - chrono::Duration::days(days - 1 - offset);
                let (usdt, watch_count) = by_date
                    .get(&date)
                    .copied()
                    .unwrap_or((Decimal::ZERO, 0));
                DailyEarnings {
                    date,
                    usdt,
                    watch_count,
                }
            })
            .collect()
    };

    let earnings_last_7_days = daily_series(7);
    let daily_earnings_30d = daily_series(30);

    let earnings_last_7_days_total: Decimal = earnings_last_7_days.iter().map(|d| d.usdt).sum();
    let earnings_last_30_days: Decimal = daily_earnings_30d.iter().map(|d| d.usdt).sum();
    let earnings_today = earnings_last_7_days
        .last()
        .map(|d| d.usdt)
        .unwrap_or(Decimal::ZERO);

    let active_days_7d = earnings_last_7_days
        .iter()
        .filter(|d| d.usdt > Decimal::ZERO)
        .count() as u32;
    let avg_usdt_per_active_day_7d = if active_days_7d > 0 {
        earnings_last_7_days_total / Decimal::from(active_days_7d)
    } else {
        Decimal::ZERO
    };

    let total_credits: Decimal = ledger
        .iter()
        .filter(|e| e.kind == "credit")
        .map(|e| e.amount_usdt)
        .sum();
    let avg_usdt_per_watch = if profile.total_watches > 0 {
        Some(total_credits / Decimal::from(profile.total_watches))
    } else {
        None
    };

    AnalyticsSummaryResponse {
        earnings_last_7_days,
        earnings_last_30_days,
        earnings_last_7_days_total,
        earnings_today,
        daily_earnings_30d,
        total_watches: profile.total_watches,
        watches_today,
        streak_days: profile.streak_days,
        referral_count: profile.referral_count,
        conversion_hints: AnalyticsConversionHints {
            avg_usdt_per_watch,
            avg_usdt_per_active_day_7d,
            active_days_7d,
        },
    }
}

async fn get_analytics_summary(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<AnalyticsSummaryResponse>, ApiError> {
    let profile = state.profile(user_id).await;
    let ledger = state.ledger(user_id).await?;
    Ok(Json(build_analytics_summary(&ledger, &profile)))
}

#[derive(Deserialize)]
struct WatchCompleteRequest {
    watch_duration_secs: u32,
    #[serde(default)]
    is_emulator: bool,
    #[serde(default)]
    is_vpn: bool,
    /// Client-reported ad provider (`mock` | `adinplay` | `applixir`). Reserved for future SSV validation.
    #[serde(default)]
    ad_provider: Option<String>,
    /// AppLixir transaction / session id from ad completion callback (future SSV stub).
    #[serde(default)]
    ad_session_id: Option<String>,
    /// Selected video-offer tier (`quick` | `standard` | `premium` | `mega`).
    #[serde(default)]
    offer_tier: Option<VideoOfferTier>,
}

#[derive(Serialize)]
struct WatchCompleteResponse {
    user_id: Uuid,
    reward_usdt: Decimal,
    base_reward_usdt: Decimal,
    bonuses: Vec<BonusEarned>,
    message: String,
    stats: UserStatsResponse,
    wallet: WalletResponse,
}

async fn watch_complete(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(body): Json<WatchCompleteRequest>,
) -> Result<Json<WatchCompleteResponse>, ApiError> {
    let (maintenance, maintenance_msg) = state.maintenance_status().await?;
    if maintenance {
        return Err(AppError::InvalidInput(maintenance_msg).into());
    }
    if state.is_user_banned(user_id).await {
        return Err(AppError::FraudBlocked("account suspended".into()).into());
    }
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

    let mut base_reward =
        RewardEngine::calculate_watch_reward(body.watch_duration_secs, profile.streak_days);
    if body.offer_tier == Some(VideoOfferTier::Mega)
        && video_offers::bonus_slots_remaining(user_id, today).await > 0
    {
        base_reward = (base_reward * Decimal::from(video_offers::BONUS_MULTIPLIER)).round_dp(6);
        video_offers::consume_bonus_slot(user_id, today).await;
    }
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

    state.gamification_on_watch(user_id, &profile).await?;

    let flat_total = bonus_result.flat_total();
    let total_reward = watch_reward + flat_total;

    tracing::info!(
        user_id = %user_id,
        watch_duration_secs = body.watch_duration_secs,
        reward_usdt = %total_reward,
        watches_today = profile.effective_watches_today(),
        total_watches = profile.total_watches,
        "watch completed"
    );

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
    if let Some(multiplier) = bonus_result.surprise_multiplier {
        bonuses.insert(
            0,
            BonusEarned {
                id: "surprise".into(),
                title: format!("Überraschung {multiplier}×"),
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
        stats: build_user_stats(&state, &profile, user_id).await,
        wallet: {
            let balance = state.balance(user_id).await?;
            let currency = state.local_currency_for_user(user_id).await;
            let localized = state
                .currency
                .usdt_to_local(balance, currency)
                .await
                .unwrap_or_else(|| balance.round_dp(2));
            let tier = state.payout_tier_for_usdt(balance).await;
            let trust_score = state.trust_score(user_id).await;
            WalletResponse {
                user_id,
                balance_usdt: balance,
                localized_balance: localized,
                currency: currency.code().into(),
                trust_score,
                payout_tier: tier.as_str().into(),
            }
        },
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
    if state.is_user_banned(user_id).await {
        return Err(AppError::FraudBlocked("account suspended".into()).into());
    }
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
        .gamification_on_withdrawal(user_id, &profile)
        .await?;

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

#[derive(Deserialize)]
struct WalletHistoryQuery {
    #[serde(default)]
    filter: Option<String>,
}

async fn get_wallet_history(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Query(query): Query<WalletHistoryQuery>,
) -> Result<Json<Vec<crate::store::WalletHistoryItem>>, ApiError> {
    Ok(Json(
        state
            .store
            .wallet_history(user_id, query.filter.as_deref())
            .await?,
    ))
}

async fn get_profile_stats(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.build_profile_stats(user_id).await?))
}

#[derive(Serialize)]
struct MissionsResponse {
    daily: Vec<crate::store::MissionRow>,
    weekly: Vec<crate::store::MissionRow>,
    monthly: Vec<crate::store::MissionRow>,
}

async fn get_missions(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<MissionsResponse>, ApiError> {
    let all = state.store.gamification_list_missions(user_id).await?;
    Ok(Json(MissionsResponse {
        daily: all.iter().filter(|m| m.mission_type == "daily").cloned().collect(),
        weekly: all.iter().filter(|m| m.mission_type == "weekly").cloned().collect(),
        monthly: all
            .iter()
            .filter(|m| m.mission_type == "monthly")
            .cloned()
            .collect(),
    }))
}

async fn claim_mission(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.claim_mission_reward(user_id, id).await?))
}

async fn get_achievements(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<Vec<crate::store::AchievementRow>>, ApiError> {
    Ok(Json(
        state.store.gamification_list_achievements(user_id).await?,
    ))
}

#[derive(Serialize)]
struct NotificationsResponse {
    unread_count: i64,
    notifications: Vec<crate::store::NotificationRow>,
}

#[derive(Deserialize)]
struct NotificationsQuery {
    #[serde(default = "default_notifications_limit")]
    limit: u32,
}

fn default_notifications_limit() -> u32 {
    50
}

async fn get_notifications(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Query(query): Query<NotificationsQuery>,
) -> Result<Json<NotificationsResponse>, ApiError> {
    let unread_count = state.store.gamification_unread_count(user_id).await?;
    let notifications = state
        .store
        .gamification_list_notifications(user_id, query.limit)
        .await?;
    Ok(Json(NotificationsResponse {
        unread_count,
        notifications,
    }))
}

async fn mark_notification_read(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ok = state.store.gamification_mark_read(user_id, id).await?;
    Ok(Json(serde_json::json!({ "ok": ok })))
}

async fn mark_all_notifications_read(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let count = state.store.gamification_mark_all_read(user_id).await?;
    Ok(Json(serde_json::json!({ "marked": count })))
}

async fn get_referrals_dashboard(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<crate::store::ReferralDashboard>, ApiError> {
    Ok(Json(
        state.store.gamification_referral_dashboard(user_id).await?,
    ))
}

async fn complete_onboarding(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.complete_onboarding(user_id).await?))
}

#[derive(Serialize)]
struct PayoutTimelineStep {
    step: String,
    label_de: String,
    active: bool,
    done: bool,
}

#[derive(Serialize)]
struct UserPayoutRow {
    id: Uuid,
    amount_usdt: Decimal,
    payout_method: String,
    tier: String,
    status: String,
    created_at: DateTime<Utc>,
    timeline: Vec<PayoutTimelineStep>,
    estimated_time_de: String,
}

fn payout_timeline(status: &str) -> Vec<PayoutTimelineStep> {
    let steps = [
        ("requested", "Angefragt"),
        ("review", "Prüfung"),
        ("approved", "Freigegeben"),
        ("processing", "Bearbeitung"),
        ("completed", "Abgeschlossen"),
    ];
    let idx = match status {
        "pending_validation" | "pending_fraud_review" => 1,
        "approved" => 2,
        "paid_out" => 4,
        "rejected" => 4,
        _ => 0,
    };
    steps
        .iter()
        .enumerate()
        .map(|(i, (step, label))| {
            let done = if status == "rejected" {
                i <= 1
            } else {
                i < idx || (status == "paid_out" && i <= 4)
            };
            PayoutTimelineStep {
                step: (*step).into(),
                label_de: (*label).into(),
                active: i == idx,
                done,
            }
        })
        .collect()
}

async fn get_user_payouts(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<Vec<UserPayoutRow>>, ApiError> {
    let rows = state.user_payouts(user_id, 20).await?;
    Ok(Json(
        rows.into_iter()
            .map(|p| {
                let estimated = if p.status == "rejected" {
                    "Abgelehnt — Guthaben zurückerstattet".into()
                } else {
                    "1–3 Werktage Prüfung, danach je nach Zahlungsmethode".into()
                };
                UserPayoutRow {
                    id: p.id,
                    amount_usdt: p.amount_usdt,
                    payout_method: p.payout_method,
                    tier: p.tier,
                    status: p.status.clone(),
                    created_at: p.created_at,
                    timeline: payout_timeline(&p.status),
                    estimated_time_de: estimated,
                }
            })
            .collect(),
    ))
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
