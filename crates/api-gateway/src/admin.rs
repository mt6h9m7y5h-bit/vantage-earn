use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
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
use crate::feature_flags::{FeatureFlagsPatch, FeatureFlagsView};
use crate::state::AppState;
use crate::store::{
    compute_risk, csv_escape, AdminAuditEntry, AdminDailyMetric, AdminExportUserRow,
    AdminLiveSnapshot, AdminSearchResponse, AdminTimelineEvent, AdminUserListRow, AdminUserNote,
    BulkCreditFilter, PayoutListFilter, PayoutRequestRow, MAX_BULK_CREDIT_USERS,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/stats", get(admin_stats))
        .route("/admin/live", get(admin_live))
        .route("/admin/search", get(admin_search))
        .route("/admin/analytics/summary", get(admin_analytics_summary))
        .route("/admin/insights", get(admin_insights))
        .route("/admin/payouts", get(admin_list_payouts))
        .route("/admin/payouts/{id}/approve", post(admin_approve_payout))
        .route("/admin/payouts/{id}/reject", post(admin_reject_payout))
        .route("/admin/users", get(admin_list_users))
        .route("/admin/users/lookup", get(admin_user_lookup))
        .route("/admin/users/{user_id}", get(admin_user_detail))
        .route("/admin/users/{user_id}/credit", post(admin_credit))
        .route("/admin/users/{user_id}/debit", post(admin_debit))
        .route("/admin/users/{user_id}/trust-score", post(admin_trust_score))
        .route("/admin/users/{user_id}/ban", post(admin_ban))
        .route("/admin/users/{user_id}/notes", get(admin_user_notes).post(admin_add_user_note))
        .route("/admin/users/{user_id}/timeline", get(admin_user_timeline))
        .route("/admin/audit-log", get(admin_audit_log))
        .route("/admin/feature-flags", get(admin_get_feature_flags).patch(admin_patch_feature_flags))
        .route("/admin/bulk/credit/preview", post(admin_bulk_credit_preview))
        .route("/admin/bulk/credit", post(admin_bulk_credit))
        .route("/admin/export/users", get(admin_export_users))
        .route("/admin/export/audit", get(admin_export_audit))
        .route("/admin/export/payouts", get(admin_export_payouts))
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
    pub pending_withdrawal_count: i64,
    pub approved_payouts_today: i64,
    pub rejected_payouts_today: i64,
    pub active_users_yesterday: i64,
    pub rewards_yesterday_usdt: Decimal,
    pub registrations_yesterday: i64,
    pub pending_sparkline: Vec<i64>,
}

async fn admin_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AdminStatsExtended>, ApiError> {
    verify_admin(&headers)?;
    Ok(Json(state.admin_stats_extended().await?))
}

#[derive(Deserialize)]
struct LiveQuery {
    #[serde(default)]
    since: Option<DateTime<Utc>>,
}

async fn admin_live(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LiveQuery>,
) -> Result<Json<AdminLiveSnapshot>, ApiError> {
    verify_admin(&headers)?;
    let since = query.since.unwrap_or_else(|| {
        Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
    });
    Ok(Json(state.admin_live_snapshot(since).await?))
}

async fn admin_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LookupQuery>,
) -> Result<Json<AdminSearchResponse>, ApiError> {
    verify_admin(&headers)?;
    Ok(Json(state.admin_global_search(&query.q, 20).await?))
}

#[derive(Deserialize)]
struct UserListQuery {
    #[serde(default = "default_user_list_limit")]
    limit: u32,
}

fn default_user_list_limit() -> u32 {
    200
}

#[derive(Serialize)]
struct UserListResponse {
    users: Vec<AdminUserListRow>,
}

async fn admin_list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UserListQuery>,
) -> Result<Json<UserListResponse>, ApiError> {
    verify_admin(&headers)?;
    let limit = query.limit.clamp(1, 500);
    let users = state.admin_list_users(limit).await?;
    Ok(Json(UserListResponse { users }))
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

#[derive(Serialize)]
pub struct AdminInsights {
    pub revenue_7d: Decimal,
    pub revenue_30d: Decimal,
    pub avg_reward_usdt: Decimal,
    pub avg_payout_usdt: Decimal,
    pub active_users_7d: i64,
}

async fn admin_insights(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AdminInsights>, ApiError> {
    verify_admin(&headers)?;
    Ok(Json(state.store.admin_insights().await?))
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
                "new_status": payout.status,
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
    total_earnings_usdt: Decimal,
    account_age_days: i32,
    last_activity: Option<DateTime<Utc>>,
    risk_score: i32,
    risk_level: String,
    sessions_last_hour: u32,
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
    let account_age_days = profile.account_age_days();
    let sessions_last_hour = profile.sessions_last_hour;
    let total_earnings = state.user_total_earnings(user_id).await?;
    let last_activity = state.user_last_activity(user_id).await?;
    let (risk_score, risk_level) = compute_risk(
        trust_score,
        profile.banned,
        profile.payout_history,
        sessions_last_hour,
        watches_today,
    );
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
        payout_demo_mode: state.effective_payout_demo_mode().await?,
        total_earnings_usdt: total_earnings,
        account_age_days,
        last_activity,
        risk_score,
        risk_level: risk_level.into(),
        sessions_last_hour,
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
    let balance_before = state.balance(user_id).await?;
    let balance = state.credit(user_id, body.amount_usdt).await?;
    state
        .admin_log_action(
            admin_ip(&headers),
            "credit",
            Some(user_id),
            serde_json::json!({
                "amount_usdt": body.amount_usdt,
                "reason": body.reason,
                "balance_before": balance_before,
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
    let balance_before = state.balance(user_id).await?;
    let balance = state.debit(user_id, body.amount_usdt).await?;
    state
        .admin_log_action(
            admin_ip(&headers),
            "debit",
            Some(user_id),
            serde_json::json!({
                "amount_usdt": body.amount_usdt,
                "reason": body.reason,
                "balance_before": balance_before,
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
    let previous = state.trust_score(user_id).await;
    state.set_trust_score(user_id, score).await?;
    state
        .admin_log_action(
            admin_ip(&headers),
            "trust_score",
            Some(user_id),
            serde_json::json!({
                "score": score,
                "previous_score": previous,
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
    let was_banned = profile.banned;
    profile.banned = body.banned;
    state.save_profile(user_id, &profile).await?;
    let action = if body.banned { "ban" } else { "unban" };
    state
        .admin_log_action(
            admin_ip(&headers),
            action,
            Some(user_id),
            serde_json::json!({
                "reason": body.reason,
                "previous_banned": was_banned,
                "new_banned": body.banned,
            }),
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

async fn admin_get_feature_flags(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<FeatureFlagsView>, ApiError> {
    verify_admin(&headers)?;
    Ok(Json(state.feature_flags_view().await?))
}

async fn admin_patch_feature_flags(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(patch): Json<FeatureFlagsPatch>,
) -> Result<Json<FeatureFlagsView>, ApiError> {
    verify_admin(&headers)?;
    let before = state.feature_flags_view().await?;
    let after = state.patch_feature_flags(patch).await?;
    state
        .admin_log_action(
            admin_ip(&headers),
            "feature_flags_update",
            None,
            serde_json::json!({
                "admin_ip": admin_ip(&headers),
                "before": before,
                "after": after,
            }),
        )
        .await?;
    Ok(Json(after))
}

#[derive(Deserialize)]
struct BulkCreditBody {
    amount_usdt: Decimal,
    reason: String,
    filter: BulkCreditFilter,
}

#[derive(Serialize)]
struct BulkCreditPreviewResponse {
    user_count: usize,
    amount_usdt: Decimal,
    total_usdt: Decimal,
    max_users: u32,
}

async fn admin_bulk_credit_preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BulkCreditBody>,
) -> Result<Json<BulkCreditPreviewResponse>, ApiError> {
    verify_admin(&headers)?;
    let (user_count, total_usdt) = state
        .bulk_credit_preview(body.filter, body.amount_usdt)
        .await?;
    Ok(Json(BulkCreditPreviewResponse {
        user_count,
        amount_usdt: body.amount_usdt,
        total_usdt,
        max_users: MAX_BULK_CREDIT_USERS,
    }))
}

#[derive(Serialize)]
struct BulkCreditResponse {
    user_count: usize,
    amount_usdt: Decimal,
    total_usdt: Decimal,
    action: String,
}

async fn admin_bulk_credit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BulkCreditBody>,
) -> Result<Json<BulkCreditResponse>, ApiError> {
    verify_admin(&headers)?;
    let reason = body.reason.trim();
    if reason.is_empty() {
        return Err(shared::AppError::InvalidInput("reason is required".into()).into());
    }
    let filter_json = serde_json::to_value(&body.filter).unwrap_or(serde_json::Value::Null);
    let (user_count, total_usdt) = state
        .bulk_credit_users(body.filter, body.amount_usdt)
        .await?;
    state
        .admin_log_action(
            admin_ip(&headers),
            "bulk_credit",
            None,
            serde_json::json!({
                "amount_usdt": body.amount_usdt,
                "reason": reason,
                "filter": filter_json,
                "user_count": user_count,
                "total_usdt": total_usdt,
            }),
        )
        .await?;
    Ok(Json(BulkCreditResponse {
        user_count,
        amount_usdt: body.amount_usdt,
        total_usdt,
        action: "bulk_credit".into(),
    }))
}

#[derive(Serialize)]
struct NotesResponse {
    notes: Vec<AdminUserNote>,
}

async fn admin_user_notes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Json<NotesResponse>, ApiError> {
    verify_admin(&headers)?;
    if !state.user_exists(user_id).await {
        return Err(AppError::UserNotFound(user_id).into());
    }
    let notes = state.admin_user_notes(user_id).await?;
    Ok(Json(NotesResponse { notes }))
}

#[derive(Deserialize)]
struct AddNoteBody {
    note: String,
    #[serde(default)]
    created_by: Option<String>,
}

async fn admin_add_user_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(body): Json<AddNoteBody>,
) -> Result<Json<AdminUserNote>, ApiError> {
    verify_admin(&headers)?;
    if !state.user_exists(user_id).await {
        return Err(AppError::UserNotFound(user_id).into());
    }
    let note = body.note.trim();
    if note.is_empty() {
        return Err(AppError::InvalidInput("note is required".into()).into());
    }
    let created_by = body
        .created_by
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| admin_ip(&headers).unwrap_or_else(|| "admin".into()));
    let entry = state
        .admin_add_user_note(user_id, note, &created_by)
        .await?;
    state
        .admin_log_action(
            admin_ip(&headers),
            "user_note",
            Some(user_id),
            serde_json::json!({ "note_id": entry.id }),
        )
        .await?;
    Ok(Json(entry))
}

#[derive(Serialize)]
struct TimelineResponse {
    events: Vec<AdminTimelineEvent>,
}

async fn admin_user_timeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Json<TimelineResponse>, ApiError> {
    verify_admin(&headers)?;
    if !state.user_exists(user_id).await {
        return Err(AppError::UserNotFound(user_id).into());
    }
    let events = state.admin_user_timeline(user_id, 100).await?;
    Ok(Json(TimelineResponse { events }))
}

#[derive(Deserialize)]
struct ExportQuery {
    #[serde(default = "default_export_format")]
    format: String,
    #[serde(default = "default_export_limit")]
    limit: u32,
}

fn default_export_format() -> String {
    "csv".into()
}

fn default_export_limit() -> u32 {
    1000
}

async fn admin_export_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    verify_admin(&headers)?;
    let rows = state.admin_export_users(query.limit.clamp(1, 5000)).await?;
    export_users_response(&query.format, rows)
}

async fn admin_export_audit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    verify_admin(&headers)?;
    let rows = state.admin_export_audit(query.limit.clamp(1, 5000)).await?;
    export_audit_response(&query.format, rows)
}

async fn admin_export_payouts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    verify_admin(&headers)?;
    let rows = state.admin_export_payouts(query.limit.clamp(1, 5000)).await?;
    export_payouts_response(&query.format, rows)
}

fn export_users_response(format: &str, rows: Vec<AdminExportUserRow>) -> Result<Response, ApiError> {
    if format.eq_ignore_ascii_case("json") {
        return Ok(Json(rows).into_response());
    }
    let mut csv = String::from(
        "user_id,referral_code,balance_usdt,trust_score,banned,created_at,total_watches,referral_count,locale\n",
    );
    for r in rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            r.user_id,
            csv_escape(&r.referral_code),
            r.balance_usdt,
            r.trust_score,
            r.banned,
            r.created_at.to_rfc3339(),
            r.total_watches,
            r.referral_count,
            csv_escape(&r.locale),
        ));
    }
    Ok(csv_attachment(csv, "users.csv"))
}

fn export_audit_response(format: &str, rows: Vec<AdminAuditEntry>) -> Result<Response, ApiError> {
    if format.eq_ignore_ascii_case("json") {
        return Ok(Json(rows).into_response());
    }
    let mut csv = String::from("id,created_at,action,user_id,admin_ip,details\n");
    for r in rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{}\n",
            r.id,
            r.created_at.to_rfc3339(),
            csv_escape(&r.action),
            r.user_id.map(|u| u.to_string()).unwrap_or_default(),
            csv_escape(r.admin_ip.as_deref().unwrap_or("")),
            csv_escape(&r.details.to_string()),
        ));
    }
    Ok(csv_attachment(csv, "audit.csv"))
}

fn export_payouts_response(format: &str, rows: Vec<PayoutRequestRow>) -> Result<Response, ApiError> {
    if format.eq_ignore_ascii_case("json") {
        return Ok(Json(rows).into_response());
    }
    let mut csv = String::from("id,user_id,amount_usdt,tier,status,payout_method,created_at\n");
    for r in rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            r.id,
            r.user_id,
            r.amount_usdt,
            csv_escape(&r.tier),
            csv_escape(&r.status),
            csv_escape(&r.payout_method),
            r.created_at.to_rfc3339(),
        ));
    }
    Ok(csv_attachment(csv, "payouts.csv"))
}

fn csv_attachment(body: String, filename: &str) -> Response {
    let mut response = (StatusCode::OK, body).into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    if let Ok(val) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        response
            .headers_mut()
            .insert(axum::http::header::CONTENT_DISPOSITION, val);
    }
    response
}
