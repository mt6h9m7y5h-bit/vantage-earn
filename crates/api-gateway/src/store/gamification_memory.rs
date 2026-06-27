use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use referral_engine::ReferralEngine;
use rust_decimal::Decimal;
use shared::{AppError, AppResult};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::gamification::{
    achievement_catalog, ledger_label_de, level_from_xp, mission_catalog, period_start_for_type,
    resets_at_utc, xp_progress, AchievementDef, MissionDef, XP_PER_LOGIN, XP_PER_REFERRAL,
    XP_PER_WATCH, XP_PER_WITHDRAWAL,
};
use crate::state::UserProfile;
use crate::store::gamification::{
    AchievementRow, MissionRow, NotificationRow, ReferralDashboard, UserStreakRow, UserXpRow,
    WalletHistoryItem,
};

#[derive(Clone, Default)]
pub struct MemMissionProgress {
    pub progress: i32,
    pub completed_at: Option<DateTime<Utc>>,
    pub claimed_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct MemNotification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub category: String,
    pub title: String,
    pub body: String,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Default)]
pub struct GamificationMemState {
    pub user_xp: HashMap<Uuid, i32>,
    pub login_streaks: HashMap<Uuid, (i32, i32, Option<NaiveDate>)>,
    pub user_achievements: HashMap<(Uuid, i32), DateTime<Utc>>,
    pub mission_progress: HashMap<(Uuid, i32, NaiveDate), MemMissionProgress>,
    pub notifications: Vec<MemNotification>,
    pub onboarding_claimed: HashSet<Uuid>,
}

#[derive(Clone)]
pub struct GamificationMemStore {
    pub state: Arc<RwLock<GamificationMemState>>,
    pub users: Arc<RwLock<HashMap<Uuid, UserProfile>>>,
}

impl GamificationMemStore {
    pub fn new(users: Arc<RwLock<HashMap<Uuid, UserProfile>>>) -> Self {
        Self {
            state: Arc::new(RwLock::new(GamificationMemState::default())),
            users,
        }
    }

    pub async fn ensure_user(&self, user_id: Uuid) -> AppResult<()> {
        let mut s = self.state.write().await;
        s.user_xp.entry(user_id).or_insert(0);
        s.login_streaks
            .entry(user_id)
            .or_insert((0, 0, None));
        Ok(())
    }

    pub async fn purge_user(&self, user_id: Uuid) {
        let mut s = self.state.write().await;
        s.user_xp.remove(&user_id);
        s.login_streaks.remove(&user_id);
        s.onboarding_claimed.remove(&user_id);
        s.user_achievements.retain(|(uid, _), _| *uid != user_id);
        s.mission_progress.retain(|(uid, _, _), _| *uid != user_id);
        s.notifications.retain(|n| n.user_id != user_id);
    }

    fn xp_row(total_xp: i32) -> UserXpRow {
        let level = level_from_xp(total_xp);
        let (xp_in_current_level, xp_to_next_level) = xp_progress(total_xp, level);
        UserXpRow {
            total_xp,
            level,
            xp_in_current_level,
            xp_to_next_level,
        }
    }

    pub async fn get_user_xp(&self, user_id: Uuid) -> AppResult<UserXpRow> {
        self.ensure_user(user_id).await?;
        let s = self.state.read().await;
        let total = *s.user_xp.get(&user_id).unwrap_or(&0);
        Ok(Self::xp_row(total))
    }

    pub async fn add_xp(&self, user_id: Uuid, amount: i32) -> AppResult<UserXpRow> {
        if amount <= 0 {
            return self.get_user_xp(user_id).await;
        }
        self.ensure_user(user_id).await?;
        let mut s = self.state.write().await;
        let entry = s.user_xp.entry(user_id).or_insert(0);
        *entry += amount;
        Ok(Self::xp_row(*entry))
    }

    pub async fn get_login_streak(&self, user_id: Uuid) -> AppResult<UserStreakRow> {
        self.ensure_user(user_id).await?;
        let s = self.state.read().await;
        let (current, longest, last) = s
            .login_streaks
            .get(&user_id)
            .copied()
            .unwrap_or((0, 0, None));
        Ok(UserStreakRow {
            current_streak: current,
            longest_streak: longest,
            last_login_date: last,
        })
    }

    pub async fn record_login_streak(&self, user_id: Uuid) -> AppResult<UserStreakRow> {
        self.ensure_user(user_id).await?;
        let today = Utc::now().date_naive();
        let mut s = self.state.write().await;
        let entry = s.login_streaks.entry(user_id).or_insert((0, 0, None));
        let (current, longest, last) = *entry;
        let new_current = match last {
            Some(d) if d == today => current,
            Some(d) if (today - d).num_days() == 1 => current + 1,
            Some(_) => 1,
            None => 1,
        };
        let new_longest = longest.max(new_current);
        *entry = (new_current, new_longest, Some(today));
        Ok(UserStreakRow {
            current_streak: new_current,
            longest_streak: new_longest,
            last_login_date: Some(today),
        })
    }

    pub async fn list_achievements_for_user(&self, user_id: Uuid) -> AppResult<Vec<AchievementRow>> {
        self.ensure_user(user_id).await?;
        let s = self.state.read().await;
        Ok(achievement_catalog()
            .into_iter()
            .map(|a| AchievementRow {
                id: a.id,
                slug: a.slug.into(),
                title_de: a.title_de.into(),
                description_de: a.description_de.into(),
                xp_reward: a.xp_reward,
                badge_slug: a.badge_slug.into(),
                unlocked_at: s.user_achievements.get(&(user_id, a.id)).copied(),
            })
            .collect())
    }

    pub async fn unlock_achievement(
        &self,
        user_id: Uuid,
        slug: &str,
    ) -> AppResult<Option<AchievementDef>> {
        let Some(def) = achievement_catalog().into_iter().find(|a| a.slug == slug) else {
            return Ok(None);
        };
        self.ensure_user(user_id).await?;
        let mut s = self.state.write().await;
        if s.user_achievements.contains_key(&(user_id, def.id)) {
            return Ok(None);
        }
        s.user_achievements
            .insert((user_id, def.id), Utc::now());
        drop(s);
        self.add_xp(user_id, def.xp_reward).await?;
        Ok(Some(def))
    }

    pub async fn unlocked_slugs(&self, user_id: Uuid) -> AppResult<HashSet<String>> {
        let s = self.state.read().await;
        Ok(s.user_achievements
            .keys()
            .filter(|(uid, _)| *uid == user_id)
            .filter_map(|(_, id)| {
                achievement_catalog()
                    .into_iter()
                    .find(|a| a.id == *id)
                    .map(|a| a.slug.to_string())
            })
            .collect())
    }

    fn mission_row(
        def: &MissionDef,
        progress: &MemMissionProgress,
        today: NaiveDate,
    ) -> MissionRow {
        let period = period_start_for_type(def.mission_type, today);
        let completed = progress.progress >= def.target_count;
        MissionRow {
            id: def.id,
            slug: def.slug.into(),
            title_de: def.title_de.into(),
            mission_type: def.mission_type.into(),
            target_count: def.target_count,
            reward_usdt: Decimal::from_str_exact(def.reward_usdt).unwrap(),
            xp_reward: def.xp_reward,
            progress: progress.progress.min(def.target_count),
            completed,
            claimed: progress.claimed_at.is_some(),
            period_start: period,
            resets_at: resets_at_utc(def.mission_type, today),
        }
    }

    pub async fn list_missions_for_user(&self, user_id: Uuid) -> AppResult<Vec<MissionRow>> {
        self.ensure_user(user_id).await?;
        let today = Utc::now().date_naive();
        let s = self.state.read().await;
        Ok(mission_catalog()
            .iter()
            .map(|def| {
                let period = period_start_for_type(def.mission_type, today);
                let key = (user_id, def.id, period);
                let prog = s
                    .mission_progress
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                Self::mission_row(def, &prog, today)
            })
            .collect())
    }

    async fn bump_mission(
        &self,
        user_id: Uuid,
        slug: &str,
        delta: i32,
    ) -> AppResult<()> {
        let Some(def) = mission_catalog().into_iter().find(|m| m.slug == slug) else {
            return Ok(());
        };
        let today = Utc::now().date_naive();
        let period = period_start_for_type(def.mission_type, today);
        let mut s = self.state.write().await;
        let key = (user_id, def.id, period);
        let entry = s.mission_progress.entry(key).or_default();
        if entry.claimed_at.is_some() {
            return Ok(());
        }
        entry.progress += delta;
        if entry.progress >= def.target_count && entry.completed_at.is_none() {
            entry.completed_at = Some(Utc::now());
        }
        Ok(())
    }

    pub async fn on_watch(&self, user_id: Uuid) -> AppResult<()> {
        self.add_xp(user_id, XP_PER_WATCH).await?;
        self.bump_mission(user_id, "daily_watch_5", 1).await?;
        self.bump_mission(user_id, "daily_watch_15", 1).await?;
        self.bump_mission(user_id, "weekly_watch_100", 1).await?;
        self.bump_mission(user_id, "monthly_watch_500", 1).await?;
        Ok(())
    }

    pub async fn on_login(&self, user_id: Uuid) -> AppResult<()> {
        self.add_xp(user_id, XP_PER_LOGIN).await?;
        self.record_login_streak(user_id).await?;
        self.bump_mission(user_id, "daily_login", 1).await?;
        Ok(())
    }

    pub async fn on_referral(&self, user_id: Uuid) -> AppResult<()> {
        self.add_xp(user_id, XP_PER_REFERRAL).await?;
        self.bump_mission(user_id, "daily_invite", 1).await?;
        self.bump_mission(user_id, "weekly_invite_3", 1).await?;
        Ok(())
    }

    pub async fn on_withdrawal(&self, user_id: Uuid) -> AppResult<()> {
        self.add_xp(user_id, XP_PER_WITHDRAWAL).await?;
        self.bump_mission(user_id, "monthly_first_withdrawal", 1).await?;
        Ok(())
    }

    pub async fn claim_mission(
        &self,
        user_id: Uuid,
        mission_id: i32,
    ) -> AppResult<MissionRow> {
        let Some(def) = mission_catalog().into_iter().find(|m| m.id == mission_id) else {
            return Err(AppError::InvalidInput("mission not found".into()));
        };
        let today = Utc::now().date_naive();
        let period = period_start_for_type(def.mission_type, today);
        let mut s = self.state.write().await;
        let key = (user_id, mission_id, period);
        let entry = s.mission_progress.entry(key).or_default();
        if entry.progress < def.target_count {
            return Err(AppError::InvalidInput("mission not completed".into()));
        }
        if entry.claimed_at.is_some() {
            return Err(AppError::InvalidInput("mission already claimed".into()));
        }
        entry.claimed_at = Some(Utc::now());
        let row = Self::mission_row(&def, entry, today);
        drop(s);
        self.add_xp(user_id, def.xp_reward).await?;
        Ok(row)
    }

    pub async fn mission_reward_usdt(&self, mission_id: i32) -> AppResult<Decimal> {
        let def = mission_catalog()
            .into_iter()
            .find(|m| m.id == mission_id)
            .ok_or_else(|| AppError::InvalidInput("mission not found".into()))?;
        Ok(Decimal::from_str_exact(def.reward_usdt).unwrap())
    }

    pub async fn push_notification(
        &self,
        user_id: Uuid,
        category: &str,
        title: &str,
        body: &str,
    ) -> AppResult<NotificationRow> {
        let n = MemNotification {
            id: Uuid::new_v4(),
            user_id,
            category: category.into(),
            title: title.into(),
            body: body.into(),
            read_at: None,
            created_at: Utc::now(),
        };
        let row = NotificationRow {
            id: n.id,
            category: n.category.clone(),
            title: n.title.clone(),
            body: n.body.clone(),
            read: false,
            created_at: n.created_at,
        };
        self.state.write().await.notifications.push(n);
        Ok(row)
    }

    pub async fn list_notifications(
        &self,
        user_id: Uuid,
        limit: u32,
    ) -> AppResult<Vec<NotificationRow>> {
        let s = self.state.read().await;
        let mut items: Vec<_> = s
            .notifications
            .iter()
            .filter(|n| n.user_id == user_id && n.read_at.is_none() || true)
            .filter(|n| n.user_id == user_id)
            .map(|n| NotificationRow {
                id: n.id,
                category: n.category.clone(),
                title: n.title.clone(),
                body: n.body.clone(),
                read: n.read_at.is_some(),
                created_at: n.created_at,
            })
            .collect();
        items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        items.truncate(limit as usize);
        Ok(items)
    }

    pub async fn unread_notification_count(&self, user_id: Uuid) -> AppResult<i64> {
        let s = self.state.read().await;
        Ok(s.notifications
            .iter()
            .filter(|n| n.user_id == user_id && n.read_at.is_none())
            .count() as i64)
    }

    pub async fn mark_notification_read(&self, user_id: Uuid, id: Uuid) -> AppResult<bool> {
        let mut s = self.state.write().await;
        if let Some(n) = s.notifications.iter_mut().find(|n| n.id == id && n.user_id == user_id) {
            n.read_at = Some(Utc::now());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn mark_all_notifications_read(&self, user_id: Uuid) -> AppResult<i64> {
        let mut s = self.state.write().await;
        let now = Utc::now();
        let mut count = 0i64;
        for n in s.notifications.iter_mut().filter(|n| n.user_id == user_id) {
            if n.read_at.is_none() {
                n.read_at = Some(now);
                count += 1;
            }
        }
        Ok(count)
    }

    pub async fn referral_dashboard(&self, user_id: Uuid) -> AppResult<ReferralDashboard> {
        let users = self.users.read().await;
        let profile = users.get(&user_id).cloned().unwrap_or_default();
        let today = Utc::now().date_naive();
        let active_cutoff = today - chrono::Duration::days(7);

        let mut active = 0i32;
        let mut inactive = 0i32;
        let mut pending = 0i32;
        for (uid, p) in users.iter() {
            if p.referred_by != Some(user_id) {
                continue;
            }
            if !p.referral_bonus_paid {
                pending += 1;
            }
            if p.last_active_date >= Some(active_cutoff) {
                active += 1;
            } else {
                inactive += 1;
            }
            let _ = uid;
        }

        let total = profile.referral_count;
        let conversion = if total > 0 {
            (active as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let earnings = ReferralEngine::referrer_bonus() * Decimal::from(profile.referral_count);

        Ok(ReferralDashboard {
            referral_code: ReferralEngine::code_for_user(user_id),
            total_earnings_usdt: earnings,
            referral_count: total,
            active_referrals: active,
            inactive_referrals: inactive,
            pending_bonuses: pending,
            conversion_rate_pct: conversion,
        })
    }

    pub async fn is_early_user(&self, user_id: Uuid) -> AppResult<bool> {
        let users = self.users.read().await;
        let mut ids: Vec<_> = users
            .iter()
            .map(|(id, p)| (*id, p.created_at))
            .collect();
        ids.sort_by_key(|(_, created)| *created);
        Ok(ids.iter().take(100).any(|(id, _)| *id == user_id))
    }

    pub async fn onboarding_claimed(&self, user_id: Uuid) -> AppResult<bool> {
        Ok(self
            .state
            .read()
            .await
            .onboarding_claimed
            .contains(&user_id))
    }

    pub async fn mark_onboarding_claimed(&self, user_id: Uuid) -> AppResult<()> {
        self.state.write().await.onboarding_claimed.insert(user_id);
        Ok(())
    }
}

pub fn wallet_history_from_ledger(
    ledger: &[crate::store::LedgerItem],
    filter: Option<&str>,
) -> Vec<WalletHistoryItem> {
    let mut items: Vec<_> = ledger
        .iter()
        .filter(|e| match filter {
            Some("credit") => e.kind == "credit",
            Some("debit") => e.kind == "debit",
            Some("reward") => e.kind == "credit",
            Some("payout") => e.kind == "debit",
            _ => true,
        })
        .map(|e| WalletHistoryItem {
            id: e.id,
            amount_usdt: e.amount_usdt,
            balance_after: e.balance_after,
            kind: e.kind.clone(),
            label_de: ledger_label_de(&e.kind).into(),
            created_at: e.created_at,
        })
        .collect();
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    items
}
