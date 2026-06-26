use referral_engine::ReferralEngine;
use uuid::Uuid;

use crate::gamification::{achievement_catalog, profile_completion_pct, onboarding_usdt, ONBOARDING_XP_REWARD};
use crate::state::{AppState, UserProfile};
use shared::AppResult;

impl AppState {
    pub async fn gamification_on_register(&self, user_id: Uuid) -> AppResult<()> {
        self.store.gamification_ensure(user_id).await?;
        self.store.gamification_on_login(user_id).await?;
        if self.store.gamification_is_early_user(user_id).await? {
            self.try_unlock_achievement(user_id, "early_user").await?;
        }
        Ok(())
    }

    pub async fn gamification_on_login(&self, user_id: Uuid) -> AppResult<()> {
        self.store.gamification_ensure(user_id).await?;
        self.store.gamification_on_login(user_id).await?;
        Ok(())
    }

    pub async fn gamification_on_watch(&self, user_id: Uuid, profile: &UserProfile) -> AppResult<()> {
        self.store.gamification_on_watch(user_id).await?;
        self.check_watch_achievements(user_id, profile).await?;
        self.check_streak_achievements(user_id, profile).await?;
        Ok(())
    }

    pub async fn gamification_on_referral(&self, referrer_id: Uuid, profile: &UserProfile) -> AppResult<()> {
        self.store.gamification_on_referral(referrer_id).await?;
        if profile.referral_count >= 1 {
            self.try_unlock_achievement(referrer_id, "first_referral").await?;
        }
        Ok(())
    }

    pub async fn gamification_on_withdrawal(&self, user_id: Uuid, profile: &UserProfile) -> AppResult<()> {
        self.store.gamification_on_withdrawal(user_id).await?;
        if profile.payout_history >= 1 {
            self.try_unlock_achievement(user_id, "first_withdrawal").await?;
        }
        Ok(())
    }

    async fn check_watch_achievements(&self, user_id: Uuid, profile: &UserProfile) -> AppResult<()> {
        let tw = profile.total_watches;
        if tw >= 1 {
            self.try_unlock_achievement(user_id, "first_ad").await?;
        }
        if tw >= 10 {
            self.try_unlock_achievement(user_id, "ads_10").await?;
        }
        if tw >= 100 {
            self.try_unlock_achievement(user_id, "ads_100").await?;
        }
        if tw >= 500 {
            self.try_unlock_achievement(user_id, "ads_500").await?;
        }
        Ok(())
    }

    async fn check_streak_achievements(&self, user_id: Uuid, profile: &UserProfile) -> AppResult<()> {
        if profile.streak_days >= 7 {
            self.try_unlock_achievement(user_id, "streak_7").await?;
        }
        if profile.streak_days >= 30 {
            self.try_unlock_achievement(user_id, "streak_30").await?;
        }
        Ok(())
    }

    async fn try_unlock_achievement(&self, user_id: Uuid, slug: &str) -> AppResult<()> {
        if let Some(def) = self.store.gamification_unlock_achievement(user_id, slug).await? {
            let _ = self
                .store
                .gamification_push_notification(
                    user_id,
                    "achievement",
                    &format!("Erfolg freigeschaltet: {}", def.title_de),
                    def.description_de,
                )
                .await;
        }
        Ok(())
    }

    pub async fn complete_onboarding(&self, user_id: Uuid) -> AppResult<serde_json::Value> {
        if self.store.gamification_onboarding_claimed(user_id).await? {
            return Err(shared::AppError::InvalidInput(
                "Onboarding-Belohnung bereits abgeholt".into(),
            ));
        }
        self.store.gamification_mark_onboarding_claimed(user_id).await?;
        self.store
            .gamification_add_xp(user_id, ONBOARDING_XP_REWARD)
            .await?;
        let usdt = onboarding_usdt();
        self.credit(user_id, usdt).await?;
        let _ = self
            .store
            .gamification_push_notification(
                user_id,
                "onboarding",
                "Willkommen bei VANTAGE!",
                &format!("+{usdt} USDT und {ONBOARDING_XP_REWARD} XP für deinen Start."),
            )
            .await;
        Ok(serde_json::json!({
            "xp_reward": ONBOARDING_XP_REWARD,
            "usdt_reward": usdt.to_string(),
        }))
    }

    pub async fn build_profile_stats(&self, user_id: Uuid) -> AppResult<serde_json::Value> {
        let profile = self.profile(user_id).await;
        let xp = self.store.gamification_get_user_xp(user_id).await?;
        let streak = self.store.gamification_get_login_streak(user_id).await?;
        let achievements = self.store.gamification_list_achievements(user_id).await?;
        let unlocked: Vec<_> = achievements
            .iter()
            .filter(|a| a.unlocked_at.is_some())
            .collect();
        let badges: Vec<_> = unlocked
            .iter()
            .map(|a| a.badge_slug.clone())
            .collect();
        let lifetime_earnings = self.user_total_earnings(user_id).await?;
        let ledger = self.ledger(user_id).await?;
        let withdrawals: rust_decimal::Decimal = ledger
            .iter()
            .filter(|e| e.kind == "debit")
            .map(|e| e.amount_usdt)
            .sum();
        let watch_time_secs = profile.total_watches as i64 * 30;
        let referral_earnings =
            ReferralEngine::referrer_bonus() * rust_decimal::Decimal::from(profile.referral_count);
        let completion = profile_completion_pct(
            profile.total_watches,
            profile.referral_count,
            profile.payout_history,
            unlocked.len(),
            achievement_catalog().len(),
        );

        Ok(serde_json::json!({
            "lifetime_earnings_usdt": lifetime_earnings.to_string(),
            "total_withdrawals_usdt": withdrawals.to_string(),
            "ads_watched": profile.total_watches,
            "watch_time_secs": watch_time_secs,
            "referral_earnings_usdt": referral_earnings.to_string(),
            "account_age_days": profile.account_age_days(),
            "streak_days": profile.streak_days,
            "login_streak": streak,
            "level": xp.level,
            "total_xp": xp.total_xp,
            "xp_in_current_level": xp.xp_in_current_level,
            "xp_to_next_level": xp.xp_to_next_level,
            "profile_completion_pct": completion,
            "badges": badges,
            "achievements": achievements,
        }))
    }

    pub async fn claim_mission_reward(
        &self,
        user_id: Uuid,
        mission_id: i32,
    ) -> AppResult<serde_json::Value> {
        let row = self.store.gamification_claim_mission(user_id, mission_id).await?;
        let usdt = self.store.gamification_mission_reward_usdt(mission_id).await?;
        if usdt > rust_decimal::Decimal::ZERO {
            self.credit(user_id, usdt).await?;
        }
        let _ = self
            .store
            .gamification_push_notification(
                user_id,
                "mission",
                &format!("Mission abgeschlossen: {}", row.title_de),
                &format!("+{} USDT und {} XP", row.reward_usdt, row.xp_reward),
            )
            .await;
        Ok(serde_json::json!({
            "mission": row,
            "credited_usdt": usdt.to_string(),
        }))
    }
}
