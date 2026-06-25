mod audit;
mod flags;
mod ledger;
mod memory;
mod payout;
mod postgres;

pub use flags::{BulkCreditFilter, BulkUserFilter, MAX_BULK_CREDIT_USERS};

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rust_decimal::Decimal;
use shared::AppResult;
use uuid::Uuid;

pub use audit::AdminAuditEntry;
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
}
