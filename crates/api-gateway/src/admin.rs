use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use referral_engine::ReferralEngine;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use shared::AppError;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use crate::store::{AdminAuditEntry, AdminDailyMetric, PayoutListFilter, PayoutRequestRow};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/stats", get(admin_stats))
        .route("/admin/analytics/summary", get(admin_analytics_summary))
        .route("/admin/payouts", get(admin_list_payouts))
        .route("/admin/payouts/{id}/approve", post(admin_approve_payout))
        .route("/admin/payouts/{id}/reject", post(admin_reject_payout))
        .route("/admin/users/lookup", get(admin_user_lookup))
        .route("/admin/users/{user_id}", get(admin_user_detail))
        .route("/admin/users/{user_id}/credit", post(admin_credit))
        .route("/admin/users/{user_id}/debit", post(admin_debit))
        .route("/admin/users/{user_id}/trust-score", post(admin_trust_score))
        .route("/admin/users/{user_id}/ban", post(admin_ban))
        .route("/admin/audit-log", get(admin_audit_log))
}

fn verify_admin(headers: &HeaderMap) -> Result<(), ApiError> {
    let secret = headers
        .get("X-Admin-Secret")
        .and_then(|v| v.to_str().ok());
    AppState::verify_admin_secret(secret).map_err(ApiError::from)
}

fn admin_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        })
}

#[derive(Serialize)]
pub struct AdminStatsExtended {
    pub total_revenue: Decimal,
    pub pending_payouts: Decimal,
    pub held_payouts: Decimal,
    pub user_count: i64,
    pub recent_payout_count: i64,
    pub active_users_today: i64,
    pub registrations_today: i64,
    pub videos_today: i64,
    pub rewards_today_usdt: Decimal,
    pub avg_trust_score: f64,
    pub revenue_24h: Decimal,
    pub revenue_7d: Decimal,
    pub revenue_30d: Decimal,
}

async fn admin_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AdminStatsExtended>, ApiError> {
    verify_admin(&headers)?;
    Ok(Json(state.admin_stats_extended().await?))
}

#[derive(Deserialize)]
struct AnalyticsDaysQuery {
    #[serde(default = "default_analytics_days")]
    days: i64,
}

fn default_analytics_days() -> i64 {
    7
}

#[derive(Serialize)]
pub struct AdminAnalyticsSummary {
    pub days: i64,
    pub daily_earnings: Vec<AdminDailyMetric>,
    pub earnings_period_total: Decimal,
    pub total_users: i64,
    pub pending_payout_count: i64,
    pub pending_payouts_usdt: Decimal,
    pub held_payouts_usdt: Decimal,
    pub total_paid_out_usdt: Decimal,
    pub active_users_today: i64,
    pub total_revenue: Decimal,
}

async fn admin_analytics_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AnalyticsDaysQuery>,
) -> Result<Json<AdminAnalyticsSummary>, ApiError> {
    verify_admin(&headers)?;
    let days = if query.days == 30 { 30 } else { 7 };
    Ok(Json(state.admin_analytics_summary(days).await?))
}

#[derive(Deserialize)]
struct PayoutListQuery {
    #[serde(default = "default_payout_status")]
    status: String,
    #[serde(default = "default_payout_limit")]
    limit: u32,
}

fn default_payout_status() -> String {
    "pending".into()
}

fn default_payout_limit() -> u32 {
    50
}

#[derive(Serialize)]
struct PayoutListResponse {
    payouts: Vec<PayoutRequestRow>,
}

async fn admin_list_payouts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PayoutListQuery>,
) -> Result<Json<PayoutListResponse>, ApiError> {
    verify_admin(&headers)?;
    let filter = PayoutListFilter::parse(&query.status).ok_or_else(|| {
        shared::AppError::InvalidInput(
            "status must be pending, approved, rejected, or all".into(),
        )
    })?;
    let limit = query.limit.clamp(1, 200);
    let payouts = state.admin_list_payouts(filter, limit).await?;
    Ok(Json(PayoutListResponse { payouts }))
}

#[derive(Serialize)]
struct PayoutActionResponse {
    payout: PayoutRequestRow,
    action: String,
}

async fn admin_approve_payout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<PayoutActionResponse>, ApiError> {
    verify_admin(&headers)?;
    let before = state
        .get_payout_request(id)
        .await?
        .ok_or_else(|| shared::AppError::InvalidInput("payout not found".into()))?;
    let payout = state.admin_approve_payout(id).await?;
    state
        .admin_log_action(
            admin_ip(&headers),
            "payout_approve",
            Some(payout.user_id),
            serde_json::json!({
                "payout_id": id,
                "amount_usdt": payout.amount_usdt,
                "previous_status": before.status,
                "new_status": payout.status,
            }),
        )
        .await?;
    Ok(Json(PayoutActionResponse {
        payout,
        action: "approve".into(),
    }))
}

#[derive(Deserialize)]
struct RejectBody {
    reason: String,
}

async fn admin_reject_payout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectBody>,
) -> Result<Json<PayoutActionResponse>, ApiError> {
    verify_admin(&headers)?;
    let reason = body.reason.trim();
    if reason.is_empty() {
        return Err(shared::AppError::InvalidInput("reason is required".into()).into());
    }
    let before = state
        .get_payout_request(id)
        .await?
        .ok_or_else(|| shared::AppError::InvalidInput("payout not found".into()))?;
    let payout = state.admin_reject_payout(id).await?;
    state
        .admin_log_action(
            admin_ip(&headers),
            "payout_reject",
            Some(payout.user_id),
            serde_json::json!({
                "payout_id": id,
                "amount_usdt": payout.amount_usdt,
                "previous_status": before.status,
                "reason": reason,
            }),
        )
        .await?;
    Ok(Json(PayoutActionResponse {
        payout,
        action: "reject".into(),
    }))
}

#[derive(Deserialize)]
struct LookupQuery {
    q: String,
}

#[derive(Serialize)]
struct LookupUserHit {
    user_id: Uuid,
    referral_code: String,
}

#[derive(Serialize)]
struct LookupResponse {
    users: Vec<LookupUserHit>,
}

async fn admin_user_lookup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LookupQuery>,
) -> Result<Json<LookupResponse>, ApiError> {
    verify_admin(&headers)?;
    let ids = state.admin_lookup_users(&query.q).await?;
    let users = ids
        .into_iter()
        .map(|user_id| LookupUserHit {
            referral_code: ReferralEngine::code_for_user(user_id),
            user_id,
        })
        .collect();
    Ok(Json(LookupResponse { users }))
}

#[derive(Serialize)]
struct AdminUserProfile {
    user_id: Uuid,
    balance_usdt: Decimal,
    trust_score: i32,
    referral_code: String,
    banned: bool,
    created_at: DateTime<Utc>,
    locale: String,
    streak_days: i32,
    referral_count: i32,
    watches_today: u32,
    total_watches: u32,
    payout_history: i32,
    payout_tier: String,
    payout_demo_mode: bool,
}

async fn admin_user_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Json<AdminUserProfile>, ApiError> {
    verify_admin(&headers)?;
    if !state.user_exists(user_id).await {
        return Err(AppError::UserNotFound(user_id).into());
    }
    let profile = state.profile(user_id).await;
    let balance = state.balance(user_id).await?;
    let trust_score = state.trust_score(user_id).await;
    let tier = state.payout_tier_for_usdt(balance).await;
    let watches_today = profile.effective_watches_today();
    Ok(Json(AdminUserProfile {
        user_id,
        balance_usdt: balance,
        trust_score,
        referral_code: ReferralEngine::code_for_user(user_id),
        banned: profile.banned,
        created_at: profile.created_at,
        locale: profile.locale,
        streak_days: profile.streak_days,
        referral_count: profile.referral_count,
        watches_today,
        total_watches: profile.total_watches,
        payout_history: profile.payout_history,
        payout_tier: tier.as_str().into(),
        payout_demo_mode: AppState::payout_demo_mode(),
    }))
}

#[derive(Deserialize)]
struct AmountReasonBody {
    amount_usdt: Decimal,
    reason: String,
}

#[derive(Serialize)]
struct BalanceActionResponse {
    user_id: Uuid,
    balance_usdt: Decimal,
    action: String,
}

async fn admin_credit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(body): Json<AmountReasonBody>,
) -> Result<Json<BalanceActionResponse>, ApiError> {
    verify_admin(&headers)?;
    if !state.user_exists(user_id).await {
        return Err(AppError::UserNotFound(user_id).into());
    }
    if body.amount_usdt <= Decimal::ZERO {
        return Err(AppError::InvalidInput("amount must be positive".into()).into());
    }
    let balance = state.credit(user_id, body.amount_usdt).await?;
    state
        .admin_log_action(
            admin_ip(&headers),
            "credit",
            Some(user_id),
            serde_json::json!({
                "amount_usdt": body.amount_usdt,
                "reason": body.reason,
                "balance_after": balance,
            }),
        )
        .await?;
    Ok(Json(BalanceActionResponse {
        user_id,
        balance_usdt: balance,
        action: "credit".into(),
    }))
}

async fn admin_debit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(body): Json<AmountReasonBody>,
) -> Result<Json<BalanceActionResponse>, ApiError> {
    verify_admin(&headers)?;
    if !state.user_exists(user_id).await {
        return Err(AppError::UserNotFound(user_id).into());
    }
    if body.amount_usdt <= Decimal::ZERO {
        return Err(AppError::InvalidInput("amount must be positive".into()).into());
    }
    let balance = state.debit(user_id, body.amount_usdt).await?;
    state
        .admin_log_action(
            admin_ip(&headers),
            "debit",
            Some(user_id),
            serde_json::json!({
                "amount_usdt": body.amount_usdt,
                "reason": body.reason,
                "balance_after": balance,
            }),
        )
        .await?;
    Ok(Json(BalanceActionResponse {
        user_id,
        balance_usdt: balance,
        action: "debit".into(),
    }))
}

#[derive(Deserialize)]
struct TrustScoreBody {
    score: i32,
    reason: String,
}

#[derive(Serialize)]
struct TrustScoreResponse {
    user_id: Uuid,
    trust_score: i32,
}

async fn admin_trust_score(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(body): Json<TrustScoreBody>,
) -> Result<Json<TrustScoreResponse>, ApiError> {
    verify_admin(&headers)?;
    if !state.user_exists(user_id).await {
        return Err(AppError::UserNotFound(user_id).into());
    }
    let score = body.score.clamp(0, 100);
    state.set_trust_score(user_id, score).await?;
    state
        .admin_log_action(
            admin_ip(&headers),
            "trust_score",
            Some(user_id),
            serde_json::json!({
                "score": score,
                "reason": body.reason,
            }),
        )
        .await?;
    Ok(Json(TrustScoreResponse {
        user_id,
        trust_score: score,
    }))
}

#[derive(Deserialize)]
struct BanBody {
    banned: bool,
    reason: String,
}

#[derive(Serialize)]
struct BanResponse {
    user_id: Uuid,
    banned: bool,
}

async fn admin_ban(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(body): Json<BanBody>,
) -> Result<Json<BanResponse>, ApiError> {
    verify_admin(&headers)?;
    if !state.user_exists(user_id).await {
        return Err(AppError::UserNotFound(user_id).into());
    }
    let mut profile = state.profile(user_id).await;
    profile.banned = body.banned;
    state.save_profile(user_id, &profile).await?;
    let action = if body.banned { "ban" } else { "unban" };
    state
        .admin_log_action(
            admin_ip(&headers),
            action,
            Some(user_id),
            serde_json::json!({ "reason": body.reason }),
        )
        .await?;
    Ok(Json(BanResponse {
        user_id,
        banned: body.banned,
    }))
}

#[derive(Deserialize)]
struct AuditLogQuery {
    #[serde(default = "default_audit_limit")]
    limit: u32,
}

fn default_audit_limit() -> u32 {
    50
}

#[derive(Serialize)]
struct AuditLogResponse {
    entries: Vec<AdminAuditEntry>,
}

async fn admin_audit_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<AuditLogResponse>, ApiError> {
    verify_admin(&headers)?;
    let limit = query.limit.clamp(1, 200);
    let entries = state.admin_audit_log(limit).await?;
    Ok(Json(AuditLogResponse { entries }))
}
