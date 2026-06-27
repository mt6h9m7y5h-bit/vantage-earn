use std::sync::Arc;

use axum::http::HeaderMap;
use chrono::{DateTime, NaiveDate, Utc};
use event_bus::EventBus;
use reward_engine::{BonusEngine, RewardEngine, WatchBonusInput, WatchBonusResult};
use rust_decimal::Decimal;
use referral_engine::ReferralEngine;
use shared::{
    AppError, AppEvent, AppResult, Currency, PayoutMethod, PayoutTier, RewardCreditedPayload,
    SafeAIContext, TrustScoreUpdatedPayload, DEMO_MIN_PAYOUT_USDT, MIN_PAYOUT_EUR,
};
use trust_score_engine::TrustScoreEngine;
use uuid::Uuid;

use currency_engine::CurrencyEngine;
use ai_engine::AiCopilot;

use crate::auth::JwtService;
use crate::fraud_admin::HighRiskUserRow;
use crate::middleware::{admin_actor, client_ip};
use crate::store::{
    AnnouncementCreate, AnnouncementPatch, AnnouncementRow,
};
use crate::feature_flags::{
    FeatureFlagsPatch, FeatureFlagsView, KEY_MAINTENANCE_MESSAGE, KEY_MAINTENANCE_MODE,
    KEY_PAYOUT_DEMO_MODE, KEY_WATCH_DURATION_SECS,
};
use crate::store::{AdminAuditEntry, BulkCreditFilter, LedgerItem, Store, MAX_BULK_CREDIT_USERS};

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
    pub(crate) store: Arc<Store>,
    pub currency: Arc<CurrencyEngine>,
    pub events: Arc<EventBus>,
    pub copilot: AiCopilot,
    pub jwt: JwtService,
    pub started_at: DateTime<Utc>,
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
        let store = match std::env::var("DATABASE_URL") {
            Ok(url) if !url.trim().is_empty() => {
                tracing::info!("connecting to PostgreSQL");
                connect_store_with_retry(url.trim()).await
            }
            _ => {
                tracing::warn!("DATABASE_URL not set — using in-memory store");
                Store::memory()
            }
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
            started_at: Utc::now(),
        }
    }

    pub async fn store_ping_ms(&self) -> (bool, Option<u64>) {
        match self.store.ping_ms().await {
            Ok(Some(ms)) => (true, Some(ms)),
            Ok(None) => (false, None),
            Err(_) => (false, None),
        }
    }

    pub async fn store_open_connections(&self) -> AppResult<Option<i64>> {
        self.store.open_connections().await
    }

    pub async fn list_active_announcements(&self) -> AppResult<Vec<AnnouncementRow>> {
        self.store.list_active_announcements().await
    }

    pub async fn list_all_announcements(&self) -> AppResult<Vec<AnnouncementRow>> {
        self.store.list_all_announcements().await
    }

    pub async fn get_announcement(&self, id: Uuid) -> AppResult<Option<AnnouncementRow>> {
        self.store.get_announcement(id).await
    }

    pub async fn create_announcement(
        &self,
        body: AnnouncementCreate,
    ) -> AppResult<AnnouncementRow> {
        self.store.create_announcement(body).await
    }

    pub async fn patch_announcement(
        &self,
        id: Uuid,
        patch: AnnouncementPatch,
    ) -> AppResult<AnnouncementRow> {
        self.store.patch_announcement(id, patch).await
    }

    pub async fn fraud_scan_users(&self) -> Vec<HighRiskUserRow> {
        let rows = self.admin_list_users(500).await.unwrap_or_default();
        let mut out = Vec::new();
        for row in rows {
            let profile = self.profile(row.user_id).await;
            let watches_today = profile.effective_watches_today();
            let (risk_score, risk_level) = crate::store::compute_risk(
                row.trust_score,
                row.banned,
                profile.payout_history,
                profile.sessions_last_hour,
                watches_today,
            );
            let mut flags = Vec::new();
            if row.trust_score < 40 {
                flags.push("niedriger Trust-Score".into());
            }
            if profile.sessions_last_hour > 8 {
                flags.push("schnelle Watches".into());
            }
            if row.banned {
                flags.push("gesperrt".into());
            }
            let pending_payouts = self
                .store
                .user_pending_payout_count(row.user_id)
                .await
                .unwrap_or(0);
            if pending_payouts > 0 {
                flags.push("offene Auszahlung".into());
            }
            out.push(HighRiskUserRow {
                user_id: row.user_id,
                referral_code: row.referral_code,
                trust_score: row.trust_score,
                risk_score,
                risk_level: risk_level.into(),
                sessions_last_hour: profile.sessions_last_hour,
                watches_today,
                banned: row.banned,
                flags,
                pending_payouts,
            });
        }
        out
    }

    pub async fn dev_reset_store(&self) -> AppResult<()> {
        self.store.dev_reset().await
    }

    pub async fn admin_log_enriched(
        &self,
        headers: &HeaderMap,
        action: &str,
        user_id: Option<Uuid>,
        mut details: serde_json::Value,
    ) -> AppResult<()> {
        if let Some(obj) = details.as_object_mut() {
            obj.entry("actor")
                .or_insert(serde_json::json!(admin_actor(headers)));
            obj.entry("client_ip")
                .or_insert(serde_json::json!(client_ip(headers, None)));
        }
        self.admin_log_action(client_ip(headers, None), action, user_id, details)
            .await
    }

    pub fn liquidity_reserve_ratio() -> Decimal {
        std::env::var("LIQUIDITY_RESERVE_RATIO")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .and_then(Decimal::from_f64_retain)
            .unwrap_or_else(|| Decimal::new(10, 2))
    }

    pub fn payout_demo_mode_from_env() -> bool {
        if cfg!(test) {
            return true;
        }
        std::env::var("PAYOUT_DEMO_MODE")
            .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false)
    }

    pub fn payout_demo_mode() -> bool {
        Self::payout_demo_mode_from_env()
    }

    pub async fn effective_payout_demo_mode(&self) -> AppResult<bool> {
        let flags = self.store.get_all_feature_flags().await?;
        Ok(FeatureFlagsView::resolve(&flags).payout_demo_mode)
    }

    pub async fn effective_watch_duration_secs(&self) -> AppResult<u32> {
        let flags = self.store.get_all_feature_flags().await?;
        Ok(FeatureFlagsView::resolve(&flags).watch_duration_secs)
    }

    pub async fn maintenance_status(&self) -> AppResult<(bool, String)> {
        let flags = self.store.get_all_feature_flags().await?;
        let view = FeatureFlagsView::resolve(&flags);
        Ok((view.maintenance_mode, view.maintenance_message))
    }

    pub async fn feature_flags_view(&self) -> AppResult<FeatureFlagsView> {
        let flags = self.store.get_all_feature_flags().await?;
        let timestamps = self.store.feature_flag_timestamps().await?;
        let audit = self.store.latest_feature_flags_audit().await?;
        Ok(FeatureFlagsView::resolve_with_meta(
            &flags,
            &timestamps,
            audit.as_ref(),
        ))
    }

    pub async fn patch_feature_flags(&self, patch: FeatureFlagsPatch) -> AppResult<FeatureFlagsView> {
        if let Some(v) = patch.maintenance_mode {
            self.store
                .set_feature_flag(KEY_MAINTENANCE_MODE, serde_json::json!(v))
                .await?;
        }
        if let Some(msg) = patch.maintenance_message {
            let trimmed = msg.trim();
            if !trimmed.is_empty() {
                self.store
                    .set_feature_flag(KEY_MAINTENANCE_MESSAGE, serde_json::json!(trimmed))
                    .await?;
            }
        }
        if patch.clear_payout_demo_mode {
            self.store.delete_feature_flag(KEY_PAYOUT_DEMO_MODE).await?;
        } else if let Some(v) = patch.payout_demo_mode {
            self.store
                .set_feature_flag(KEY_PAYOUT_DEMO_MODE, serde_json::json!(v))
                .await?;
        }
        if patch.clear_watch_duration_secs {
            self.store
                .delete_feature_flag(KEY_WATCH_DURATION_SECS)
                .await?;
        } else if let Some(v) = patch.watch_duration_secs {
            if v == 0 {
                return Err(AppError::InvalidInput(
                    "watch_duration_secs must be positive".into(),
                ));
            }
            self.store
                .set_feature_flag(KEY_WATCH_DURATION_SECS, serde_json::json!(v))
                .await?;
        }
        self.feature_flags_view().await
    }

    pub async fn bulk_credit_preview(
        &self,
        filter: BulkCreditFilter,
        amount_usdt: Decimal,
    ) -> AppResult<(usize, Decimal)> {
        if amount_usdt <= Decimal::ZERO {
            return Err(AppError::InvalidInput("amount must be positive".into()));
        }
        let user_filter = filter
            .to_user_filter()
            .map_err(AppError::InvalidInput)?;
        let user_ids = self
            .store
            .list_users_for_bulk(user_filter, MAX_BULK_CREDIT_USERS + 1)
            .await?;
        if user_ids.len() > MAX_BULK_CREDIT_USERS as usize {
            return Err(AppError::InvalidInput(format!(
                "too many users ({}) — max {MAX_BULK_CREDIT_USERS}",
                user_ids.len()
            )));
        }
        let total = amount_usdt * Decimal::from(user_ids.len());
        Ok((user_ids.len(), total))
    }

    pub async fn bulk_credit_users(
        &self,
        filter: BulkCreditFilter,
        amount_usdt: Decimal,
    ) -> AppResult<(usize, Decimal)> {
        if amount_usdt <= Decimal::ZERO {
            return Err(AppError::InvalidInput("amount must be positive".into()));
        }
        let user_filter = filter
            .to_user_filter()
            .map_err(AppError::InvalidInput)?;
        let user_ids = self
            .store
            .list_users_for_bulk(user_filter, MAX_BULK_CREDIT_USERS + 1)
            .await?;
        if user_ids.is_empty() {
            return Err(AppError::InvalidInput("no matching users".into()));
        }
        if user_ids.len() > MAX_BULK_CREDIT_USERS as usize {
            return Err(AppError::InvalidInput(format!(
                "too many users ({}) — max {MAX_BULK_CREDIT_USERS}",
                user_ids.len()
            )));
        }
        for user_id in &user_ids {
            self.credit(*user_id, amount_usdt).await?;
        }
        let total = amount_usdt * Decimal::from(user_ids.len());
        Ok((user_ids.len(), total))
    }

    pub async fn min_payout_usdt(&self) -> Decimal {
        if self.effective_payout_demo_mode().await.unwrap_or_else(|_| Self::payout_demo_mode()) {
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
        let _ = self
            .gamification_on_referral(referrer_id, &referrer_profile)
            .await;

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

    pub async fn user_email(&self, user_id: Uuid) -> Option<String> {
        self.store.user_email(user_id).await.ok().flatten()
    }

    pub async fn find_user_by_email(&self, email: &str) -> AppResult<Option<(Uuid, String)>> {
        self.store.find_user_by_email(email).await
    }

    pub async fn set_user_credentials(
        &self,
        user_id: Uuid,
        email: &str,
        password_hash: &str,
    ) -> AppResult<()> {
        self.store
            .set_user_credentials(user_id, email, password_hash)
            .await
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
        let yesterday = today - chrono::Duration::days(1);
        let (approved_today, rejected_today) = self.store.payout_actions_today(today).await?;
        let pending_count = self.store.pending_payout_request_count().await?;
        let rewards_today = self.store_rewards_today_usdt(today).await?;
        let rewards_yesterday = self.store.rewards_on(yesterday).await?;
        let active_today = self.store_active_users_today(today).await?;
        let active_yesterday = self.store.active_users_on(yesterday).await?;
        let reg_today = self.store_registrations_today(today).await?;
        let reg_yesterday = self.store.registrations_on(yesterday).await?;
        let sparkline = self.store.admin_daily_metrics(7).await?;
        Ok(crate::admin::AdminStatsExtended {
            total_revenue: self.total_revenue().await?,
            pending_payouts: self.pending_payouts().await?,
            held_payouts: self.held_payouts().await?,
            user_count: self.user_count().await?,
            recent_payout_count: self.recent_payout_count().await?,
            active_users_today: active_today,
            registrations_today: reg_today,
            videos_today: self.store_videos_today(today).await?,
            rewards_today_usdt: rewards_today,
            avg_trust_score: self.store_avg_trust_score().await?,
            revenue_24h: self.store_revenue_in_period_hours(24).await?,
            revenue_7d: self.store_revenue_in_period_days(7).await?,
            revenue_30d: self.store_revenue_in_period_days(30).await?,
            pending_withdrawal_count: pending_count,
            approved_payouts_today: approved_today,
            rejected_payouts_today: rejected_today,
            active_users_yesterday: active_yesterday,
            rewards_yesterday_usdt: rewards_yesterday,
            registrations_yesterday: reg_yesterday,
            pending_sparkline: sparkline.iter().map(|d| d.watch_count as i64).collect(),
        })
    }

    pub async fn admin_lookup_users(&self, query: &str) -> AppResult<Vec<Uuid>> {
        self.store_search_users(query).await
    }

    pub async fn admin_live_snapshot(&self, since: DateTime<Utc>) -> AppResult<crate::store::AdminLiveSnapshot> {
        self.store.admin_live_snapshot(since).await
    }

    pub async fn admin_global_search(&self, query: &str, limit: u32) -> AppResult<crate::store::AdminSearchResponse> {
        self.store.admin_global_search(query, limit).await
    }

    pub async fn admin_list_users(&self, limit: u32) -> AppResult<Vec<crate::store::AdminUserListRow>> {
        self.store.admin_list_users(limit).await
    }

    pub async fn admin_user_notes(&self, user_id: Uuid) -> AppResult<Vec<crate::store::AdminUserNote>> {
        self.store.admin_user_notes(user_id).await
    }

    pub async fn admin_add_user_note(
        &self,
        user_id: Uuid,
        note: &str,
        created_by: &str,
    ) -> AppResult<crate::store::AdminUserNote> {
        self.store.admin_add_user_note(user_id, note, created_by).await
    }

    pub async fn admin_user_timeline(&self, user_id: Uuid, limit: u32) -> AppResult<Vec<crate::store::AdminTimelineEvent>> {
        self.store.admin_user_timeline(user_id, limit).await
    }

    pub async fn admin_export_users(&self, limit: u32) -> AppResult<Vec<crate::store::AdminExportUserRow>> {
        self.store.admin_export_users(limit).await
    }

    pub async fn admin_export_audit(&self, limit: u32) -> AppResult<Vec<AdminAuditEntry>> {
        self.store.admin_export_audit(limit).await
    }

    pub async fn admin_export_payouts(&self, limit: u32) -> AppResult<Vec<crate::store::PayoutRequestRow>> {
        self.store.admin_export_payouts(limit).await
    }

    pub async fn user_total_earnings(&self, user_id: Uuid) -> AppResult<Decimal> {
        self.store.user_total_earnings(user_id).await
    }

    pub async fn user_last_activity(&self, user_id: Uuid) -> AppResult<Option<DateTime<Utc>>> {
        self.store.user_last_activity(user_id).await
    }

    pub async fn user_payouts(&self, user_id: Uuid, limit: u32) -> AppResult<Vec<crate::store::PayoutRequestRow>> {
        self.store.user_payouts(user_id, limit).await
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

    pub async fn admin_analytics_summary(
        &self,
        days: i64,
    ) -> AppResult<crate::admin::AdminAnalyticsSummary> {
        let days = days.clamp(1, 30);
        let daily = self.store.admin_daily_metrics(days).await?;
        let earnings_period_total: Decimal = daily.iter().map(|d| d.usdt).sum();
        let today = Utc::now().date_naive();
        Ok(crate::admin::AdminAnalyticsSummary {
            days,
            daily_earnings: daily,
            earnings_period_total,
            total_users: self.user_count().await?,
            pending_payout_count: self.store.pending_payout_request_count().await?,
            pending_payouts_usdt: self.pending_payouts().await?,
            held_payouts_usdt: self.held_payouts().await?,
            total_paid_out_usdt: self.store.total_paid_out_usdt().await?,
            active_users_today: self.store_active_users_today(today).await?,
            total_revenue: self.total_revenue().await?,
        })
    }

    pub async fn admin_list_payouts(
        &self,
        filter: crate::store::PayoutListFilter,
        limit: u32,
    ) -> AppResult<Vec<crate::store::PayoutRequestRow>> {
        self.store.list_payout_requests(filter, limit).await
    }

    pub async fn get_payout_request(
        &self,
        payout_id: Uuid,
    ) -> AppResult<Option<crate::store::PayoutRequestRow>> {
        self.store.get_payout_request(payout_id).await
    }

    pub async fn admin_approve_payout(
        &self,
        payout_id: Uuid,
    ) -> AppResult<crate::store::PayoutRequestRow> {
        let payout = self
            .store
            .get_payout_request(payout_id)
            .await?
            .ok_or_else(|| AppError::InvalidInput("payout not found".into()))?;

        let new_status = match payout.status.as_str() {
            "pending_validation" | "pending_fraud_review" => {
                self.store
                    .release_held_to_pending(payout.amount_usdt)
                    .await?;
                "approved"
            }
            "approved" => {
                self.store
                    .subtract_pending_payout(payout.amount_usdt)
                    .await?;
                "paid_out"
            }
            _ => {
                return Err(AppError::InvalidInput(
                    "payout cannot be approved in its current status".into(),
                ));
            }
        };

        self.store
            .update_payout_status(payout_id, new_status)
            .await?;
        self.store
            .get_payout_request(payout_id)
            .await?
            .ok_or_else(|| AppError::InvalidInput("payout not found".into()))
    }

    pub async fn admin_reject_payout(
        &self,
        payout_id: Uuid,
    ) -> AppResult<crate::store::PayoutRequestRow> {
        let payout = self
            .store
            .get_payout_request(payout_id)
            .await?
            .ok_or_else(|| AppError::InvalidInput("payout not found".into()))?;

        if payout.status == "rejected" || payout.status == "paid_out" {
            return Err(AppError::InvalidInput(
                "payout cannot be rejected in its current status".into(),
            ));
        }

        match payout.status.as_str() {
            "pending_validation" | "pending_fraud_review" => {
                self.store
                    .subtract_held_payout(payout.amount_usdt)
                    .await?;
            }
            "approved" => {
                self.store
                    .subtract_pending_payout(payout.amount_usdt)
                    .await?;
            }
            _ => {
                return Err(AppError::InvalidInput(
                    "payout cannot be rejected in its current status".into(),
                ));
            }
        }

        self.credit(payout.user_id, payout.amount_usdt).await?;
        self.store
            .update_payout_status(payout_id, "rejected")
            .await?;
        self.store
            .get_payout_request(payout_id)
            .await?
            .ok_or_else(|| AppError::InvalidInput("payout not found".into()))
    }
}

async fn connect_store_with_retry(database_url: &str) -> Store {
    let dev = std::env::var("RUST_ENV").as_deref() == Ok("development");
    // Render free-tier Postgres can take 30s+ to wake; allow more attempts in production.
    let max_attempts = if dev { 1 } else { 8 };
    const RETRY_SECS: u64 = 5;
    let mut last_err = None;
    for attempt in 1..=max_attempts {
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
                    max = max_attempts,
                    error = %e,
                    "database connection failed, retrying"
                );
                last_err = Some(e);
                if attempt < max_attempts {
                    tokio::time::sleep(std::time::Duration::from_secs(RETRY_SECS)).await;
                }
            }
        }
    }
    tracing::error!(
        error = ?last_err,
        hint = "Local dev: comment DATABASE_URL in .env for in-memory store, or run ./scripts/db-up.sh for Docker Postgres. Do not use Render DATABASE_URL locally.",
        "database connection failed after retries"
    );
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
