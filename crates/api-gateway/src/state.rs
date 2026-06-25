use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use event_bus::EventBus;
use reward_engine::{BonusEngine, RewardEngine, WatchBonusInput, WatchBonusResult};
use rust_decimal::Decimal;
use referral_engine::ReferralEngine;
use shared::{
    AppEvent, AppResult, Currency, PayoutMethod, PayoutTier, RewardCreditedPayload, SafeAIContext,
    TrustScoreUpdatedPayload, DEMO_MIN_PAYOUT_USDT, MIN_PAYOUT_EUR,
};
use trust_score_engine::TrustScoreEngine;
use uuid::Uuid;

use currency_engine::CurrencyEngine;
use ai_engine::AiCopilot;

use crate::auth::JwtService;
use crate::store::{AdminAuditEntry, LedgerItem, Store};

#[derive(Clone)]
pub struct UserProfile {
    pub created_at: DateTime<Utc>,
    pub streak_days: i32,
    pub referral_count: i32,
    pub payout_history: i32,
    pub sessions_last_hour: u32,
    pub sessions_window_started: DateTime<Utc>,
    pub last_active_date: Option<NaiveDate>,
    pub watches_today: u32,
    pub total_watches: u32,
    pub milestones_claimed: u8,
    pub last_daily_bonus_date: Option<NaiveDate>,
    pub streak_7_bonus_claimed: bool,
    pub last_challenge_bonus_date: Option<NaiveDate>,
    pub locale: String,
    pub referred_by: Option<Uuid>,
    pub referral_bonus_paid: bool,
    pub banned: bool,
}

impl Default for UserProfile {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            streak_days: 0,
            referral_count: 0,
            payout_history: 0,
            sessions_last_hour: 0,
            sessions_window_started: now,
            last_active_date: None,
            watches_today: 0,
            total_watches: 0,
            milestones_claimed: 0,
            last_daily_bonus_date: None,
            streak_7_bonus_claimed: false,
            last_challenge_bonus_date: None,
            locale: "en_US".into(),
            referred_by: None,
            referral_bonus_paid: false,
            banned: false,
        }
    }
}

impl UserProfile {
    pub fn account_age_days(&self) -> i32 {
        Utc::now()
            .signed_duration_since(self.created_at)
            .num_days()
            .max(1) as i32
    }

    pub fn effective_watches_today(&self) -> u32 {
        let today = Utc::now().date_naive();
        match self.last_active_date {
            Some(d) if d == today => self.watches_today,
            _ => 0,
        }
    }

    pub fn record_session(&mut self) {
        let now = Utc::now();
        if now
            .signed_duration_since(self.sessions_window_started)
            .num_hours()
            >= 1
        {
            self.sessions_last_hour = 0;
            self.sessions_window_started = now;
        }
        self.sessions_last_hour += 1;

        let today = now.date_naive();
        if self.last_active_date == Some(today) {
            self.watches_today += 1;
        } else {
            self.watches_today = 1;
        }

        match self.last_active_date {
            None => {
                self.streak_days = 1;
                self.last_active_date = Some(today);
            }
            Some(last) if last == today => {}
            Some(last) if (today - last).num_days() == 1 => {
                self.streak_days += 1;
                self.last_active_date = Some(today);
            }
            Some(_) => {
                self.streak_days = 1;
                self.last_active_date = Some(today);
                self.streak_7_bonus_claimed = false;
            }
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    store: Arc<Store>,
    pub currency: Arc<CurrencyEngine>,
    pub events: Arc<EventBus>,
    pub copilot: AiCopilot,
    pub jwt: JwtService,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::with_store(Store::memory())
    }

    pub async fn connect() -> Self {
        let store = if let Ok(url) = std::env::var("DATABASE_URL") {
            tracing::info!("connecting to PostgreSQL");
            connect_store_with_retry(&url).await
        } else {
            tracing::warn!("DATABASE_URL not set — using in-memory store");
            Store::memory()
        };
        Self::with_store(store)
    }

    fn with_store(store: Store) -> Self {
        let events = Arc::new(EventBus::new());
        events.on(|event| {
            tracing::debug!(?event, "event handler received");
        });

        Self {
            store: Arc::new(store),
            currency: Arc::new(CurrencyEngine::new()),
            events,
            copilot: AiCopilot::from_config(ai_engine::AiConfig::default()),
            jwt: JwtService::from_env(),
        }
    }

    pub fn liquidity_reserve_ratio() -> Decimal {
        std::env::var("LIQUIDITY_RESERVE_RATIO")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .and_then(Decimal::from_f64_retain)
            .unwrap_or_else(|| Decimal::new(10, 2))
    }

    pub fn payout_demo_mode() -> bool {
        if cfg!(test) {
            return true;
        }
        std::env::var("PAYOUT_DEMO_MODE")
            .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false)
    }

    pub async fn min_payout_usdt(&self) -> Decimal {
        if Self::payout_demo_mode() {
            Decimal::from_str_exact(DEMO_MIN_PAYOUT_USDT).unwrap()
        } else {
            self.currency
                .min_payout_usdt_from_eur(Decimal::from(MIN_PAYOUT_EUR))
                .await
        }
    }

    pub async fn store_healthy(&self) -> bool {
        self.store.ping().await.unwrap_or(false)
    }

    pub async fn balance(&self, user_id: Uuid) -> AppResult<Decimal> {
        self.ensure_user(user_id).await;
        self.store.balance(user_id).await
    }

    pub async fn credit(&self, user_id: Uuid, amount: Decimal) -> AppResult<Decimal> {
        self.store.credit(user_id, amount).await
    }

    pub async fn debit(&self, user_id: Uuid, amount: Decimal) -> AppResult<Decimal> {
        self.store.debit(user_id, amount).await
    }

    pub async fn add_revenue(&self, amount: Decimal) -> AppResult<()> {
        self.store.add_revenue(amount).await
    }

    pub async fn total_revenue(&self) -> AppResult<Decimal> {
        self.store.total_revenue().await
    }

    pub async fn pending_payouts(&self) -> AppResult<Decimal> {
        self.store.pending_payouts().await
    }

    pub async fn held_payouts(&self) -> AppResult<Decimal> {
        self.store.held_payouts().await
    }

    pub async fn add_pending_payout(&self, amount: Decimal) -> AppResult<()> {
        self.store.add_pending_payout(amount).await
    }

    pub async fn add_held_payout(&self, amount: Decimal) -> AppResult<()> {
        self.store.add_held_payout(amount).await
    }

    pub fn min_payout_eur() -> Decimal {
        Decimal::from(MIN_PAYOUT_EUR)
    }

    pub fn payout_methods() -> Vec<&'static str> {
        PayoutMethod::all_strings()
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
        self.store
            .record_payout_request(id, user_id, amount, tier, status, payout_method)
            .await
    }

    pub async fn find_user_by_referral_code(&self, code: &str) -> AppResult<Option<Uuid>> {
        self.store.find_user_by_referral_code(code).await
    }

    pub async fn ledger(&self, user_id: Uuid) -> AppResult<Vec<LedgerItem>> {
        self.store.ledger(user_id).await
    }

    pub async fn maybe_apply_referral_bonuses(
        &self,
        user_id: Uuid,
        profile: &mut UserProfile,
    ) -> AppResult<()> {
        if profile.referral_bonus_paid {
            return Ok(());
        }
        let Some(referrer_id) = profile.referred_by else {
            profile.referral_bonus_paid = true;
            return Ok(());
        };
        if referrer_id == user_id {
            profile.referral_bonus_paid = true;
            return Ok(());
        }

        let referee_bonus = ReferralEngine::referee_bonus();
        let referrer_bonus = ReferralEngine::referrer_bonus();

        self.credit(user_id, referee_bonus).await?;
        self.add_revenue(RewardEngine::calculate_ad_revenue(referee_bonus))
            .await?;

        self.credit(referrer_id, referrer_bonus).await?;
        self.add_revenue(RewardEngine::calculate_ad_revenue(referrer_bonus))
            .await?;

        let mut referrer_profile = self.profile(referrer_id).await;
        referrer_profile.referral_count += 1;
        self.save_profile(referrer_id, &referrer_profile).await?;

        profile.referral_bonus_paid = true;
        Ok(())
    }

    pub async fn credit_watch_reward(
        &self,
        user_id: Uuid,
        reward_usdt: Decimal,
        fraud_probability: f64,
        profile: &UserProfile,
    ) -> AppResult<Decimal> {
        let balance_after = self.store.credit(user_id, reward_usdt).await?;
        let ad_revenue = RewardEngine::calculate_ad_revenue(reward_usdt);
        self.store.add_revenue(ad_revenue).await?;

        self.events
            .publish(AppEvent::RewardCredited(RewardCreditedPayload {
                user_id,
                amount_usdt: reward_usdt,
                new_balance_usdt: balance_after,
                occurred_at: Utc::now(),
            }))
            .await;

        let score = TrustScoreEngine::calculate(
            profile.account_age_days(),
            fraud_probability,
            profile.payout_history,
            0.0,
        )
        .score;
        self.store.set_trust_score(user_id, score).await?;
        self.events
            .publish(AppEvent::TrustScoreUpdated(TrustScoreUpdatedPayload {
                user_id,
                score,
                occurred_at: Utc::now(),
            }))
            .await;

        Ok(balance_after)
    }

    pub async fn credit_bonus_rewards(
        &self,
        user_id: Uuid,
        bonuses: &[reward_engine::BonusEarned],
    ) -> AppResult<Decimal> {
        let mut last_balance = self.balance(user_id).await?;
        for bonus in bonuses {
            if bonus.amount_usdt <= Decimal::ZERO {
                continue;
            }
            last_balance = self.store.credit(user_id, bonus.amount_usdt).await?;
            let ad_revenue = RewardEngine::calculate_ad_revenue(bonus.amount_usdt);
            self.store.add_revenue(ad_revenue).await?;
            self.events
                .publish(AppEvent::RewardCredited(RewardCreditedPayload {
                    user_id,
                    amount_usdt: bonus.amount_usdt,
                    new_balance_usdt: last_balance,
                    occurred_at: Utc::now(),
                }))
                .await;
        }
        Ok(last_balance)
    }

    pub fn apply_watch_bonuses(
        profile: &mut UserProfile,
        is_first_watch_today: bool,
    ) -> WatchBonusResult {
        let today = Utc::now().date_naive();
        profile.total_watches += 1;

        let (bonus_result, milestones_claimed, streak_7_claimed, daily_date, challenge_date) =
            BonusEngine::evaluate_watch_bonuses(WatchBonusInput {
                is_first_watch_today,
                last_daily_bonus_date: profile.last_daily_bonus_date,
                today,
                total_watches: profile.total_watches,
                milestones_claimed: profile.milestones_claimed,
                streak_days: profile.streak_days,
                streak_7_bonus_claimed: profile.streak_7_bonus_claimed,
                watches_today: profile.watches_today,
                last_challenge_bonus_date: profile.last_challenge_bonus_date,
            });

        profile.milestones_claimed = milestones_claimed;
        profile.streak_7_bonus_claimed = streak_7_claimed;
        profile.last_daily_bonus_date = daily_date;
        profile.last_challenge_bonus_date = challenge_date.or(profile.last_challenge_bonus_date);

        bonus_result
    }

    pub fn challenge_bonus_claimed_today(profile: &UserProfile) -> bool {
        let today = Utc::now().date_naive();
        profile.last_challenge_bonus_date == Some(today)
    }

    pub fn daily_bonus_claimed_today(profile: &UserProfile) -> bool {
        let today = Utc::now().date_naive();
        profile.last_daily_bonus_date == Some(today)
    }

    pub async fn profile(&self, user_id: Uuid) -> UserProfile {
        self.store
            .profile(user_id)
            .await
            .unwrap_or_default()
    }

    pub async fn save_profile(&self, user_id: Uuid, profile: &UserProfile) -> AppResult<()> {
        self.store.save_profile(user_id, profile).await
    }

    pub async fn trust_score(&self, user_id: Uuid) -> i32 {
        self.store
            .trust_score(user_id)
            .await
            .unwrap_or(50)
    }

    pub async fn set_trust_score(&self, user_id: Uuid, score: i32) -> AppResult<()> {
        self.store.set_trust_score(user_id, score).await
    }

    pub async fn ensure_user(&self, user_id: Uuid) {
        let _ = self.store.ensure_user(user_id).await;
    }

    pub async fn user_exists(&self, user_id: Uuid) -> bool {
        self.store.user_exists(user_id).await.unwrap_or(false)
    }

    pub async fn local_currency_for_user(&self, user_id: Uuid) -> Currency {
        let profile = self.profile(user_id).await;
        localization_engine::LocalizationEngine::default_currency_for_locale(&profile.locale)
    }

    pub async fn build_ai_context(&self, user_id: Uuid) -> SafeAIContext {
        let profile = self.profile(user_id).await;
        let balance = self.balance(user_id).await.unwrap_or(Decimal::ZERO);
        let currency =
            localization_engine::LocalizationEngine::default_currency_for_locale(&profile.locale);
        let localized = self
            .currency
            .usdt_to_local(balance, currency)
            .await
            .unwrap_or(balance);
        let goal = SafeAIContext::payout_goal_eur();
        let progress = if goal > Decimal::ZERO {
            (localized / goal * Decimal::from(100)).min(Decimal::from(100))
        } else {
            Decimal::ZERO
        };
        let daily_reward = RewardEngine::calculate_watch_reward(3600, profile.streak_days);
        let days_until_goal = if daily_reward > Decimal::ZERO && localized < goal {
            use rust_decimal::prelude::ToPrimitive;
            ((goal - localized) / daily_reward)
                .ceil()
                .to_i32()
                .unwrap_or(30)
        } else {
            0
        };

        SafeAIContext {
            user_id,
            system_language: localization_engine::LocalizationEngine::detect_system_language(
                &profile.locale,
            ),
            current_balance_usdt: balance,
            localized_balance: localized,
            localized_currency: currency,
            avg_daily_revenue_usdt: daily_reward,
            referral_count: profile.referral_count,
            streak_days: profile.streak_days,
            estimated_days_until_goal: days_until_goal,
            payout_progress_percent: progress,
            top_offerwall_name: "TapJoy".into(),
            top_offerwall_reward_usdt: Decimal::new(25, 2),
            motivational_level: (profile.streak_days.clamp(0, 10) + 1),
        }
    }

    pub async fn payout_tier_for_usdt(&self, amount_usdt: Decimal) -> PayoutTier {
        let eur = self.currency.eur_equivalent(amount_usdt).await;
        PayoutTier::from_eur_amount(eur)
    }

    pub fn payout_status(tier: PayoutTier, trust_score: i32) -> &'static str {
        match tier {
            PayoutTier::Instant if TrustScoreEngine::allows_instant_payout(trust_score) => {
                "approved"
            }
            PayoutTier::Instant => "pending_validation",
            PayoutTier::DelayedValidation => "pending_validation",
            PayoutTier::DeepFraudReview => "pending_fraud_review",
        }
    }

    pub fn payout_is_approved(status: &str) -> bool {
        status == "approved"
    }

    pub async fn weekly_leaderboard(&self) -> AppResult<Vec<(Uuid, Decimal)>> {
        self.store.weekly_leaderboard().await
    }

    pub async fn user_count(&self) -> AppResult<i64> {
        self.store.user_count().await
    }

    pub async fn recent_payout_count(&self) -> AppResult<i64> {
        self.store.recent_payout_count(7).await
    }

    pub fn admin_secret_configured() -> bool {
        !Self::admin_secret_from_env().is_empty()
    }

    fn admin_secret_from_env() -> String {
        std::env::var("ADMIN_SECRET")
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    pub fn verify_admin_secret(provided: Option<&str>) -> AppResult<()> {
        let expected = Self::admin_secret_from_env();
        if expected.is_empty() {
            return Err(shared::AppError::AdminNotConfigured);
        }
        let provided = provided.map(str::trim).filter(|s| !s.is_empty());
        match provided {
            Some(s) if s == expected => Ok(()),
            _ => Err(shared::AppError::Unauthorized),
        }
    }

    pub async fn admin_stats_extended(&self) -> AppResult<crate::admin::AdminStatsExtended> {
        let today = Utc::now().date_naive();
        Ok(crate::admin::AdminStatsExtended {
            total_revenue: self.total_revenue().await?,
            pending_payouts: self.pending_payouts().await?,
            held_payouts: self.held_payouts().await?,
            user_count: self.user_count().await?,
            recent_payout_count: self.recent_payout_count().await?,
            active_users_today: self.store_active_users_today(today).await?,
            registrations_today: self.store_registrations_today(today).await?,
            videos_today: self.store_videos_today(today).await?,
            rewards_today_usdt: self.store_rewards_today_usdt(today).await?,
            avg_trust_score: self.store_avg_trust_score().await?,
            revenue_24h: self.store_revenue_in_period_hours(24).await?,
            revenue_7d: self.store_revenue_in_period_days(7).await?,
            revenue_30d: self.store_revenue_in_period_days(30).await?,
        })
    }

    pub async fn admin_lookup_users(&self, query: &str) -> AppResult<Vec<Uuid>> {
        self.store_search_users(query).await
    }

    pub async fn admin_audit_log(&self, limit: u32) -> AppResult<Vec<AdminAuditEntry>> {
        self.store_admin_audit_log(limit).await
    }

    pub async fn admin_log_action(
        &self,
        admin_ip: Option<String>,
        action: &str,
        user_id: Option<Uuid>,
        details: serde_json::Value,
    ) -> AppResult<()> {
        let entry = AdminAuditEntry::new(admin_ip, action, user_id, details);
        self.store_append_admin_audit(entry).await
    }

    pub async fn is_user_banned(&self, user_id: Uuid) -> bool {
        self.profile(user_id).await.banned
    }

    async fn store_active_users_today(&self, today: NaiveDate) -> AppResult<i64> {
        self.store.active_users_today(today).await
    }

    async fn store_registrations_today(&self, today: NaiveDate) -> AppResult<i64> {
        self.store.registrations_today(today).await
    }

    async fn store_videos_today(&self, today: NaiveDate) -> AppResult<i64> {
        self.store.videos_today(today).await
    }

    async fn store_rewards_today_usdt(&self, today: NaiveDate) -> AppResult<Decimal> {
        self.store.rewards_today_usdt(today).await
    }

    async fn store_avg_trust_score(&self) -> AppResult<f64> {
        self.store.avg_trust_score().await
    }

    async fn store_revenue_in_period_hours(&self, hours: i64) -> AppResult<Decimal> {
        self.store.revenue_in_period_hours(hours).await
    }

    async fn store_revenue_in_period_days(&self, days: i64) -> AppResult<Decimal> {
        self.store.revenue_in_period_days(days).await
    }

    async fn store_search_users(&self, query: &str) -> AppResult<Vec<Uuid>> {
        self.store.search_users(query).await
    }

    async fn store_admin_audit_log(&self, limit: u32) -> AppResult<Vec<AdminAuditEntry>> {
        self.store.admin_audit_log(limit).await
    }

    async fn store_append_admin_audit(&self, entry: AdminAuditEntry) -> AppResult<()> {
        self.store.append_admin_audit(entry).await
    }
}

async fn connect_store_with_retry(database_url: &str) -> Store {
    const MAX_ATTEMPTS: u32 = 15;
    const RETRY_SECS: u64 = 3;
    let mut last_err = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match Store::connect(database_url).await {
            Ok(s) => {
                if attempt > 1 {
                    tracing::info!(attempt, "database connection established");
                }
                return s;
            }
            Err(e) => {
                tracing::warn!(
                    attempt,
                    max = MAX_ATTEMPTS,
                    error = %e,
                    "database connection failed, retrying"
                );
                last_err = Some(e);
                if attempt < MAX_ATTEMPTS {
                    tokio::time::sleep(std::time::Duration::from_secs(RETRY_SECS)).await;
                }
            }
        }
    }
    tracing::error!(error = ?last_err, "database connection failed after retries");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn verify_admin_secret_trims_and_rejects_empty_config() {
        let prior = std::env::var("ADMIN_SECRET").ok();
        std::env::set_var("ADMIN_SECRET", "  secret  ");
        assert!(AppState::verify_admin_secret(Some("secret")).is_ok());
        assert!(AppState::verify_admin_secret(Some("  secret  ")).is_ok());
        assert!(AppState::verify_admin_secret(Some("wrong")).is_err());

        std::env::set_var("ADMIN_SECRET", "   ");
        assert!(matches!(
            AppState::verify_admin_secret(Some("anything")),
            Err(shared::AppError::AdminNotConfigured)
        ));

        match prior {
            Some(v) => std::env::set_var("ADMIN_SECRET", v),
            None => std::env::remove_var("ADMIN_SECRET"),
        }
    }

    #[test]
    fn streak_increments_once_per_day() {
        let mut profile = UserProfile::default();
        profile.record_session();
        assert_eq!(profile.streak_days, 1);

        profile.record_session();
        assert_eq!(profile.streak_days, 1);

        profile.last_active_date = Some(NaiveDate::from_ymd_opt(2026, 6, 22).unwrap());
        profile.record_session();
        assert_eq!(profile.streak_days, 1);
    }
}
