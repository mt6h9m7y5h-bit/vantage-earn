mod admin_extra;
mod announcements;
mod audit;
mod flags;
mod gamification;
mod gamification_memory;
mod gamification_postgres;
mod ledger;
mod memory;
mod payout;
mod postgres;

pub use flags::{BulkCreditFilter, BulkUserFilter, MAX_BULK_CREDIT_USERS};

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rust_decimal::Decimal;
use shared::AppResult;
use uuid::Uuid;

pub use announcements::{
    valid_announcement_type, AnnouncementCreate, AnnouncementPatch, AnnouncementRow,
};
pub use admin_extra::{
    compute_risk, csv_escape, AdminExportUserRow, AdminLiveSnapshot, AdminSearchAuditHit,
    AdminSearchPayoutHit, AdminSearchReferralHit, AdminSearchResponse, AdminSearchUserHit,
    AdminTimelineEvent, AdminUserListRow, AdminUserNote,
};
pub use audit::AdminAuditEntry;
pub use gamification::{
    AchievementRow, MissionRow, NotificationRow, ReferralDashboard, UserStreakRow, UserXpRow,
    WalletHistoryItem,
};
pub use ledger::LedgerItem;
pub use memory::MemoryStore;
pub use payout::{AdminDailyMetric, PayoutListFilter, PayoutRequestRow};
pub use postgres::{normalize_database_url, PgStore};

use crate::state::UserProfile;

/// Start of the current ISO calendar week (Monday 00:00 UTC).
pub fn week_start_utc() -> DateTime<Utc> {
    let today = Utc::now().date_naive();
    let days_from_monday = today.weekday().num_days_from_monday();
    let monday = today - chrono::Duration::days(days_from_monday as i64);
    monday
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
}

#[derive(Clone)]
pub enum Store {
    Memory(MemoryStore),
    Postgres(PgStore),
}

impl Store {
    pub fn memory() -> Self {
        Self::Memory(MemoryStore::new())
    }

    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let store = PgStore::connect(database_url).await?;
        Ok(Self::Postgres(store))
    }

    pub async fn ping(&self) -> AppResult<bool> {
        match self {
            Self::Memory(s) => s.ping().await,
            Self::Postgres(s) => s.ping().await,
        }
    }

    pub async fn ping_ms(&self) -> AppResult<Option<u64>> {
        match self {
            Self::Memory(s) => s.ping_ms().await,
            Self::Postgres(s) => s.ping_ms().await,
        }
    }

    pub async fn open_connections(&self) -> AppResult<Option<i64>> {
        match self {
            Self::Memory(s) => s.open_connections().await,
            Self::Postgres(s) => s.open_connections().await,
        }
    }

    pub async fn dev_reset(&self) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.dev_reset().await,
            Self::Postgres(_) => Err(shared::AppError::InvalidInput(
                "reset only supported for in-memory store".into(),
            )),
        }
    }

    pub async fn user_pending_payout_count(&self, user_id: Uuid) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.user_pending_payout_count(user_id).await,
            Self::Postgres(s) => s.user_pending_payout_count(user_id).await,
        }
    }

    pub async fn list_active_announcements(&self) -> AppResult<Vec<AnnouncementRow>> {
        match self {
            Self::Memory(s) => s.list_active_announcements().await,
            Self::Postgres(s) => s.list_active_announcements().await,
        }
    }

    pub async fn list_all_announcements(&self) -> AppResult<Vec<AnnouncementRow>> {
        match self {
            Self::Memory(s) => s.list_all_announcements().await,
            Self::Postgres(s) => s.list_all_announcements().await,
        }
    }

    pub async fn get_announcement(&self, id: Uuid) -> AppResult<Option<AnnouncementRow>> {
        match self {
            Self::Memory(s) => s.get_announcement(id).await,
            Self::Postgres(s) => s.get_announcement(id).await,
        }
    }

    pub async fn create_announcement(
        &self,
        body: AnnouncementCreate,
    ) -> AppResult<AnnouncementRow> {
        match self {
            Self::Memory(s) => s.create_announcement(body).await,
            Self::Postgres(s) => s.create_announcement(body).await,
        }
    }

    pub async fn patch_announcement(
        &self,
        id: Uuid,
        patch: AnnouncementPatch,
    ) -> AppResult<AnnouncementRow> {
        match self {
            Self::Memory(s) => s.patch_announcement(id, patch).await,
            Self::Postgres(s) => s.patch_announcement(id, patch).await,
        }
    }

    pub async fn ensure_user(&self, user_id: Uuid) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.ensure_user(user_id).await,
            Self::Postgres(s) => s.ensure_user(user_id).await,
        }
    }

    pub async fn user_exists(&self, user_id: Uuid) -> AppResult<bool> {
        match self {
            Self::Memory(s) => s.user_exists(user_id).await,
            Self::Postgres(s) => s.user_exists(user_id).await,
        }
    }

    pub async fn user_email(&self, user_id: Uuid) -> AppResult<Option<String>> {
        match self {
            Self::Memory(s) => s.user_email(user_id).await,
            Self::Postgres(s) => s.user_email(user_id).await,
        }
    }

    pub async fn find_user_by_email(&self, email: &str) -> AppResult<Option<(Uuid, String)>> {
        match self {
            Self::Memory(s) => s.find_user_by_email(email).await,
            Self::Postgres(s) => s.find_user_by_email(email).await,
        }
    }

    pub async fn set_user_credentials(
        &self,
        user_id: Uuid,
        email: &str,
        password_hash: &str,
    ) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.set_user_credentials(user_id, email, password_hash).await,
            Self::Postgres(s) => s.set_user_credentials(user_id, email, password_hash).await,
        }
    }

    pub async fn create_password_reset_token(&self, user_id: Uuid) -> AppResult<String> {
        match self {
            Self::Memory(s) => s.create_password_reset_token(user_id).await,
            Self::Postgres(s) => s.create_password_reset_token(user_id).await,
        }
    }

    pub async fn consume_password_reset_token(&self, token: &str) -> AppResult<Option<Uuid>> {
        match self {
            Self::Memory(s) => s.consume_password_reset_token(token).await,
            Self::Postgres(s) => s.consume_password_reset_token(token).await,
        }
    }

    pub async fn update_password_hash(&self, user_id: Uuid, password_hash: &str) -> AppResult<bool> {
        match self {
            Self::Memory(s) => s.update_password_hash(user_id, password_hash).await,
            Self::Postgres(s) => s.update_password_hash(user_id, password_hash).await,
        }
    }

    pub async fn first_email_registration_at(&self) -> AppResult<Option<DateTime<Utc>>> {
        match self {
            Self::Memory(s) => s.first_email_registration_at().await,
            Self::Postgres(s) => s.first_email_registration_at().await,
        }
    }

    pub async fn try_grant_early_bonus(
        &self,
        user_id: Uuid,
        config: &crate::early_adopter::EarlyAdopterConfig,
    ) -> AppResult<Option<Decimal>> {
        match self {
            Self::Memory(s) => s.try_grant_early_bonus(user_id, config).await,
            Self::Postgres(s) => s.try_grant_early_bonus(user_id, config).await,
        }
    }

    pub async fn profile(&self, user_id: Uuid) -> AppResult<UserProfile> {
        match self {
            Self::Memory(s) => s.profile(user_id).await,
            Self::Postgres(s) => s.profile(user_id).await,
        }
    }

    pub async fn save_profile(&self, user_id: Uuid, profile: &UserProfile) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.save_profile(user_id, profile).await,
            Self::Postgres(s) => s.save_profile(user_id, profile).await,
        }
    }

    pub async fn balance(&self, user_id: Uuid) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.balance(user_id).await,
            Self::Postgres(s) => s.balance(user_id).await,
        }
    }

    pub async fn credit(&self, user_id: Uuid, amount: Decimal) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.credit(user_id, amount).await,
            Self::Postgres(s) => s.credit(user_id, amount).await,
        }
    }

    pub async fn debit(&self, user_id: Uuid, amount: Decimal) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.debit(user_id, amount).await,
            Self::Postgres(s) => s.debit(user_id, amount).await,
        }
    }

    pub async fn trust_score(&self, user_id: Uuid) -> AppResult<i32> {
        match self {
            Self::Memory(s) => s.trust_score(user_id).await,
            Self::Postgres(s) => s.trust_score(user_id).await,
        }
    }

    pub async fn set_trust_score(&self, user_id: Uuid, score: i32) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.set_trust_score(user_id, score).await,
            Self::Postgres(s) => s.set_trust_score(user_id, score).await,
        }
    }

    pub async fn total_revenue(&self) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.total_revenue().await,
            Self::Postgres(s) => s.total_revenue().await,
        }
    }

    pub async fn add_revenue(&self, amount: Decimal) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.add_revenue(amount).await,
            Self::Postgres(s) => s.add_revenue(amount).await,
        }
    }

    pub async fn pending_payouts(&self) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.pending_payouts().await,
            Self::Postgres(s) => s.pending_payouts().await,
        }
    }

    pub async fn held_payouts(&self) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.held_payouts().await,
            Self::Postgres(s) => s.held_payouts().await,
        }
    }

    pub async fn add_pending_payout(&self, amount: Decimal) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.add_pending_payout(amount).await,
            Self::Postgres(s) => s.add_pending_payout(amount).await,
        }
    }

    pub async fn add_held_payout(&self, amount: Decimal) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.add_held_payout(amount).await,
            Self::Postgres(s) => s.add_held_payout(amount).await,
        }
    }

    pub async fn record_payout_request(
        &self,
        id: Uuid,
        user_id: Uuid,
        amount: Decimal,
        tier: &str,
        status: &str,
        payout_method: &str,
    ) -> AppResult<()> {
        match self {
            Self::Memory(s) => {
                s.record_payout_request(id, user_id, amount, tier, status, payout_method)
                    .await
            }
            Self::Postgres(s) => {
                s.record_payout_request(id, user_id, amount, tier, status, payout_method)
                    .await
            }
        }
    }

    pub async fn find_user_by_referral_code(&self, code: &str) -> AppResult<Option<Uuid>> {
        match self {
            Self::Memory(s) => s.find_user_by_referral_code(code).await,
            Self::Postgres(s) => s.find_user_by_referral_code(code).await,
        }
    }

    pub async fn ledger(&self, user_id: Uuid) -> AppResult<Vec<LedgerItem>> {
        match self {
            Self::Memory(s) => s.ledger(user_id).await,
            Self::Postgres(s) => s.ledger(user_id).await,
        }
    }

    pub async fn weekly_leaderboard(&self) -> AppResult<Vec<(Uuid, Decimal)>> {
        match self {
            Self::Memory(s) => s.weekly_leaderboard().await,
            Self::Postgres(s) => s.weekly_leaderboard().await,
        }
    }

    pub async fn user_count(&self) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.user_count().await,
            Self::Postgres(s) => s.user_count().await,
        }
    }

    pub async fn users_with_email_count(&self) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.users_with_email_count().await,
            Self::Postgres(s) => s.users_with_email_count().await,
        }
    }

    pub async fn registrations_last_days(&self, days: i64) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.registrations_last_days(days).await,
            Self::Postgres(s) => s.registrations_last_days(days).await,
        }
    }

    pub async fn total_wallet_balance(&self) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.total_wallet_balance().await,
            Self::Postgres(s) => s.total_wallet_balance().await,
        }
    }

    pub async fn early_bonus_granted_count(&self) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.early_bonus_granted_count().await,
            Self::Postgres(s) => s.early_bonus_granted_count().await,
        }
    }

    pub async fn delete_user(&self, user_id: Uuid) -> AppResult<bool> {
        match self {
            Self::Memory(s) => s.delete_user(user_id).await,
            Self::Postgres(s) => s.delete_user(user_id).await,
        }
    }

    pub async fn recent_payout_count(&self, days: i64) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.recent_payout_count(days).await,
            Self::Postgres(s) => s.recent_payout_count(days).await,
        }
    }

    pub async fn active_users_today(&self, today: NaiveDate) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.active_users_today(today).await,
            Self::Postgres(s) => s.active_users_today(today).await,
        }
    }

    pub async fn registrations_today(&self, today: NaiveDate) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.registrations_today(today).await,
            Self::Postgres(s) => s.registrations_today(today).await,
        }
    }

    pub async fn videos_today(&self, today: NaiveDate) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.videos_today(today).await,
            Self::Postgres(s) => s.videos_today(today).await,
        }
    }

    pub async fn rewards_today_usdt(&self, today: NaiveDate) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.rewards_today_usdt(today).await,
            Self::Postgres(s) => s.rewards_today_usdt(today).await,
        }
    }

    pub async fn avg_trust_score(&self) -> AppResult<f64> {
        match self {
            Self::Memory(s) => s.avg_trust_score().await,
            Self::Postgres(s) => s.avg_trust_score().await,
        }
    }

    pub async fn revenue_in_period_hours(&self, hours: i64) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.revenue_in_period_hours(hours).await,
            Self::Postgres(s) => s.revenue_in_period_hours(hours).await,
        }
    }

    pub async fn revenue_in_period_days(&self, days: i64) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.revenue_in_period_days(days).await,
            Self::Postgres(s) => s.revenue_in_period_days(days).await,
        }
    }

    pub async fn search_users(&self, query: &str) -> AppResult<Vec<Uuid>> {
        match self {
            Self::Memory(s) => s.search_users(query).await,
            Self::Postgres(s) => s.search_users(query).await,
        }
    }

    pub async fn append_admin_audit(&self, entry: AdminAuditEntry) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.append_admin_audit(entry).await,
            Self::Postgres(s) => s.append_admin_audit(entry).await,
        }
    }

    pub async fn admin_audit_log(&self, limit: u32) -> AppResult<Vec<AdminAuditEntry>> {
        match self {
            Self::Memory(s) => s.admin_audit_log(limit).await,
            Self::Postgres(s) => s.admin_audit_log(limit).await,
        }
    }

    pub async fn list_payout_requests(
        &self,
        filter: PayoutListFilter,
        limit: u32,
    ) -> AppResult<Vec<PayoutRequestRow>> {
        match self {
            Self::Memory(s) => s.list_payout_requests(filter, limit).await,
            Self::Postgres(s) => s.list_payout_requests(filter, limit).await,
        }
    }

    pub async fn get_payout_request(&self, id: Uuid) -> AppResult<Option<PayoutRequestRow>> {
        match self {
            Self::Memory(s) => s.get_payout_request(id).await,
            Self::Postgres(s) => s.get_payout_request(id).await,
        }
    }

    pub async fn update_payout_status(&self, id: Uuid, status: &str) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.update_payout_status(id, status).await,
            Self::Postgres(s) => s.update_payout_status(id, status).await,
        }
    }

    pub async fn subtract_held_payout(&self, amount: Decimal) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.subtract_held_payout(amount).await,
            Self::Postgres(s) => s.subtract_held_payout(amount).await,
        }
    }

    pub async fn subtract_pending_payout(&self, amount: Decimal) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.subtract_pending_payout(amount).await,
            Self::Postgres(s) => s.subtract_pending_payout(amount).await,
        }
    }

    pub async fn release_held_to_pending(&self, amount: Decimal) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.release_held_to_pending(amount).await,
            Self::Postgres(s) => s.release_held_to_pending(amount).await,
        }
    }

    pub async fn admin_daily_metrics(&self, days: i64) -> AppResult<Vec<AdminDailyMetric>> {
        match self {
            Self::Memory(s) => s.admin_daily_metrics(days).await,
            Self::Postgres(s) => s.admin_daily_metrics(days).await,
        }
    }

    pub async fn pending_payout_request_count(&self) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.pending_payout_request_count().await,
            Self::Postgres(s) => s.pending_payout_request_count().await,
        }
    }

    pub async fn total_paid_out_usdt(&self) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.total_paid_out_usdt().await,
            Self::Postgres(s) => s.total_paid_out_usdt().await,
        }
    }

    pub async fn get_all_feature_flags(&self) -> AppResult<std::collections::HashMap<String, serde_json::Value>> {
        match self {
            Self::Memory(s) => s.get_all_feature_flags().await,
            Self::Postgres(s) => s.get_all_feature_flags().await,
        }
    }

    pub async fn set_feature_flag(&self, key: &str, value: serde_json::Value) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.set_feature_flag(key, value).await,
            Self::Postgres(s) => s.set_feature_flag(key, value).await,
        }
    }

    pub async fn delete_feature_flag(&self, key: &str) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.delete_feature_flag(key).await,
            Self::Postgres(s) => s.delete_feature_flag(key).await,
        }
    }

    pub async fn list_users_for_bulk(
        &self,
        filter: BulkUserFilter,
        limit: u32,
    ) -> AppResult<Vec<Uuid>> {
        match self {
            Self::Memory(s) => s.list_users_for_bulk(filter, limit).await,
            Self::Postgres(s) => s.list_users_for_bulk(filter, limit).await,
        }
    }

    pub async fn admin_list_users(&self, limit: u32) -> AppResult<Vec<AdminUserListRow>> {
        match self {
            Self::Memory(s) => s.admin_list_users(limit).await,
            Self::Postgres(s) => s.admin_list_users(limit).await,
        }
    }

    pub async fn admin_global_search(&self, query: &str, limit: u32) -> AppResult<AdminSearchResponse> {
        match self {
            Self::Memory(s) => s.admin_global_search(query, limit).await,
            Self::Postgres(s) => s.admin_global_search(query, limit).await,
        }
    }

    pub async fn admin_user_notes(&self, user_id: Uuid) -> AppResult<Vec<AdminUserNote>> {
        match self {
            Self::Memory(s) => s.admin_user_notes(user_id).await,
            Self::Postgres(s) => s.admin_user_notes(user_id).await,
        }
    }

    pub async fn admin_add_user_note(
        &self,
        user_id: Uuid,
        note: &str,
        created_by: &str,
    ) -> AppResult<AdminUserNote> {
        match self {
            Self::Memory(s) => s.admin_add_user_note(user_id, note, created_by).await,
            Self::Postgres(s) => s.admin_add_user_note(user_id, note, created_by).await,
        }
    }

    pub async fn admin_user_timeline(&self, user_id: Uuid, limit: u32) -> AppResult<Vec<AdminTimelineEvent>> {
        match self {
            Self::Memory(s) => s.admin_user_timeline(user_id, limit).await,
            Self::Postgres(s) => s.admin_user_timeline(user_id, limit).await,
        }
    }

    pub async fn admin_live_snapshot(&self, since: DateTime<Utc>) -> AppResult<AdminLiveSnapshot> {
        match self {
            Self::Memory(s) => s.admin_live_snapshot(since).await,
            Self::Postgres(s) => s.admin_live_snapshot(since).await,
        }
    }

    pub async fn admin_export_users(&self, limit: u32) -> AppResult<Vec<AdminExportUserRow>> {
        match self {
            Self::Memory(s) => s.admin_export_users(limit).await,
            Self::Postgres(s) => s.admin_export_users(limit).await,
        }
    }

    pub async fn admin_export_audit(&self, limit: u32) -> AppResult<Vec<AdminAuditEntry>> {
        match self {
            Self::Memory(s) => s.admin_export_audit(limit).await,
            Self::Postgres(s) => s.admin_export_audit(limit).await,
        }
    }

    pub async fn admin_export_payouts(&self, limit: u32) -> AppResult<Vec<PayoutRequestRow>> {
        match self {
            Self::Memory(s) => s.admin_export_payouts(limit).await,
            Self::Postgres(s) => s.admin_export_payouts(limit).await,
        }
    }

    pub async fn payout_actions_today(&self, day: NaiveDate) -> AppResult<(i64, i64)> {
        match self {
            Self::Memory(s) => s.payout_actions_today(day).await,
            Self::Postgres(s) => s.payout_actions_today(day).await,
        }
    }

    pub async fn registrations_on(&self, day: NaiveDate) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.registrations_on(day).await,
            Self::Postgres(s) => s.registrations_on(day).await,
        }
    }

    pub async fn rewards_on(&self, day: NaiveDate) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.rewards_on(day).await,
            Self::Postgres(s) => s.rewards_on(day).await,
        }
    }

    pub async fn active_users_on(&self, day: NaiveDate) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.active_users_on(day).await,
            Self::Postgres(s) => s.active_users_on(day).await,
        }
    }

    pub async fn user_payouts(&self, user_id: Uuid, limit: u32) -> AppResult<Vec<PayoutRequestRow>> {
        match self {
            Self::Memory(s) => s.user_payouts(user_id, limit).await,
            Self::Postgres(s) => s.user_payouts(user_id, limit).await,
        }
    }

    pub async fn user_total_earnings(&self, user_id: Uuid) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.user_total_earnings(user_id).await,
            Self::Postgres(s) => s.user_total_earnings(user_id).await,
        }
    }

    pub async fn user_last_activity(&self, user_id: Uuid) -> AppResult<Option<DateTime<Utc>>> {
        match self {
            Self::Memory(s) => s.user_last_activity(user_id).await,
            Self::Postgres(s) => s.user_last_activity(user_id).await,
        }
    }

    pub async fn feature_flag_timestamps(&self) -> AppResult<std::collections::HashMap<String, DateTime<Utc>>> {
        match self {
            Self::Memory(s) => s.feature_flag_timestamps().await,
            Self::Postgres(s) => s.feature_flag_timestamps().await,
        }
    }

    pub async fn latest_feature_flags_audit(&self) -> AppResult<Option<(DateTime<Utc>, serde_json::Value)>> {
        match self {
            Self::Memory(s) => s.latest_feature_flags_audit().await,
            Self::Postgres(s) => s.latest_feature_flags_audit().await,
        }
    }

    pub async fn gamification_ensure(&self, user_id: Uuid) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.gamification().ensure_user(user_id).await,
            Self::Postgres(s) => s.gamification().ensure_user(user_id).await,
        }
    }

    pub async fn gamification_get_user_xp(&self, user_id: Uuid) -> AppResult<UserXpRow> {
        match self {
            Self::Memory(s) => s.gamification().get_user_xp(user_id).await,
            Self::Postgres(s) => s.gamification().get_user_xp(user_id).await,
        }
    }

    pub async fn gamification_add_xp(&self, user_id: Uuid, amount: i32) -> AppResult<UserXpRow> {
        match self {
            Self::Memory(s) => s.gamification().add_xp(user_id, amount).await,
            Self::Postgres(s) => s.gamification().add_xp(user_id, amount).await,
        }
    }

    pub async fn gamification_get_login_streak(&self, user_id: Uuid) -> AppResult<UserStreakRow> {
        match self {
            Self::Memory(s) => s.gamification().get_login_streak(user_id).await,
            Self::Postgres(s) => s.gamification().get_login_streak(user_id).await,
        }
    }

    pub async fn gamification_on_login(&self, user_id: Uuid) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.gamification().on_login(user_id).await,
            Self::Postgres(s) => s.gamification().on_login(user_id).await,
        }
    }

    pub async fn gamification_on_watch(&self, user_id: Uuid) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.gamification().on_watch(user_id).await,
            Self::Postgres(s) => s.gamification().on_watch(user_id).await,
        }
    }

    pub async fn gamification_on_referral(&self, user_id: Uuid) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.gamification().on_referral(user_id).await,
            Self::Postgres(s) => s.gamification().on_referral(user_id).await,
        }
    }

    pub async fn gamification_on_withdrawal(&self, user_id: Uuid) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.gamification().on_withdrawal(user_id).await,
            Self::Postgres(s) => s.gamification().on_withdrawal(user_id).await,
        }
    }

    pub async fn gamification_list_achievements(&self, user_id: Uuid) -> AppResult<Vec<AchievementRow>> {
        match self {
            Self::Memory(s) => s.gamification().list_achievements_for_user(user_id).await,
            Self::Postgres(s) => s.gamification().list_achievements_for_user(user_id).await,
        }
    }

    pub async fn gamification_unlock_achievement(
        &self,
        user_id: Uuid,
        slug: &str,
    ) -> AppResult<Option<crate::gamification::AchievementDef>> {
        match self {
            Self::Memory(s) => s.gamification().unlock_achievement(user_id, slug).await,
            Self::Postgres(s) => s.gamification().unlock_achievement(user_id, slug).await,
        }
    }

    pub async fn gamification_list_missions(&self, user_id: Uuid) -> AppResult<Vec<MissionRow>> {
        match self {
            Self::Memory(s) => s.gamification().list_missions_for_user(user_id).await,
            Self::Postgres(s) => s.gamification().list_missions_for_user(user_id).await,
        }
    }

    pub async fn gamification_claim_mission(&self, user_id: Uuid, mission_id: i32) -> AppResult<MissionRow> {
        match self {
            Self::Memory(s) => s.gamification().claim_mission(user_id, mission_id).await,
            Self::Postgres(s) => s.gamification().claim_mission(user_id, mission_id).await,
        }
    }

    pub async fn gamification_mission_reward_usdt(&self, mission_id: i32) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.gamification().mission_reward_usdt(mission_id).await,
            Self::Postgres(s) => s.gamification().mission_reward_usdt(mission_id).await,
        }
    }

    pub async fn gamification_push_notification(
        &self,
        user_id: Uuid,
        category: &str,
        title: &str,
        body: &str,
    ) -> AppResult<NotificationRow> {
        match self {
            Self::Memory(s) => s.gamification().push_notification(user_id, category, title, body).await,
            Self::Postgres(s) => s.gamification().push_notification(user_id, category, title, body).await,
        }
    }

    pub async fn gamification_list_notifications(
        &self,
        user_id: Uuid,
        limit: u32,
    ) -> AppResult<Vec<NotificationRow>> {
        match self {
            Self::Memory(s) => s.gamification().list_notifications(user_id, limit).await,
            Self::Postgres(s) => s.gamification().list_notifications(user_id, limit).await,
        }
    }

    pub async fn gamification_unread_count(&self, user_id: Uuid) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.gamification().unread_notification_count(user_id).await,
            Self::Postgres(s) => s.gamification().unread_notification_count(user_id).await,
        }
    }

    pub async fn gamification_mark_read(&self, user_id: Uuid, id: Uuid) -> AppResult<bool> {
        match self {
            Self::Memory(s) => s.gamification().mark_notification_read(user_id, id).await,
            Self::Postgres(s) => s.gamification().mark_notification_read(user_id, id).await,
        }
    }

    pub async fn gamification_mark_all_read(&self, user_id: Uuid) -> AppResult<i64> {
        match self {
            Self::Memory(s) => s.gamification().mark_all_notifications_read(user_id).await,
            Self::Postgres(s) => s.gamification().mark_all_notifications_read(user_id).await,
        }
    }

    pub async fn gamification_referral_dashboard(&self, user_id: Uuid) -> AppResult<ReferralDashboard> {
        match self {
            Self::Memory(s) => s.gamification().referral_dashboard(user_id).await,
            Self::Postgres(s) => s.gamification().referral_dashboard(user_id).await,
        }
    }

    pub async fn gamification_is_early_user(&self, user_id: Uuid) -> AppResult<bool> {
        match self {
            Self::Memory(s) => s.gamification().is_early_user(user_id).await,
            Self::Postgres(s) => s.gamification().is_early_user(user_id).await,
        }
    }

    pub async fn gamification_onboarding_claimed(&self, user_id: Uuid) -> AppResult<bool> {
        match self {
            Self::Memory(s) => s.gamification().onboarding_claimed(user_id).await,
            Self::Postgres(s) => s.gamification().onboarding_claimed(user_id).await,
        }
    }

    pub async fn gamification_mark_onboarding_claimed(&self, user_id: Uuid) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.gamification().mark_onboarding_claimed(user_id).await,
            Self::Postgres(s) => s.gamification().mark_onboarding_claimed(user_id).await,
        }
    }

    pub async fn wallet_history(
        &self,
        user_id: Uuid,
        filter: Option<&str>,
    ) -> AppResult<Vec<WalletHistoryItem>> {
        let ledger = self.ledger(user_id).await?;
        Ok(gamification_memory::wallet_history_from_ledger(&ledger, filter))
    }

    pub async fn admin_insights(&self) -> AppResult<crate::admin::AdminInsights> {
        match self {
            Self::Memory(s) => s.admin_insights().await,
            Self::Postgres(s) => s.admin_insights().await,
        }
    }
}
