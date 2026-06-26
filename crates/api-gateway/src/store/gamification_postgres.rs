use chrono::{DateTime, NaiveDate, Utc};
use referral_engine::ReferralEngine;
use rust_decimal::Decimal;
use shared::{AppError, AppResult};
use sqlx::PgPool;
use uuid::Uuid;

use crate::gamification::{
    achievement_catalog, level_from_xp, mission_catalog, period_start_for_type,
    resets_at_utc, xp_progress, AchievementDef, XP_PER_LOGIN, XP_PER_REFERRAL,
    XP_PER_WATCH, XP_PER_WITHDRAWAL,
};
use crate::store::gamification::{
    AchievementRow, MissionRow, NotificationRow, ReferralDashboard, UserStreakRow, UserXpRow,
};
fn db_err(err: sqlx::Error) -> AppError {
    AppError::InvalidInput(err.to_string())
}

pub struct GamificationPgStore {
    pool: PgPool,
}

impl GamificationPgStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn ensure_user(&self, user_id: Uuid) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO user_xp (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        sqlx::query(
            "INSERT INTO user_streaks (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
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
        let total: (i32,) = sqlx::query_as("SELECT total_xp FROM user_xp WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(Self::xp_row(total.0))
    }

    pub async fn add_xp(&self, user_id: Uuid, amount: i32) -> AppResult<UserXpRow> {
        if amount <= 0 {
            return self.get_user_xp(user_id).await;
        }
        self.ensure_user(user_id).await?;
        let row: (i32,) = sqlx::query_as(
            r#"
            UPDATE user_xp
            SET total_xp = total_xp + $2,
                level = LEAST(100, FLOOR(SQRT((total_xp + $2)::float / 100))::int + 1),
                updated_at = NOW()
            WHERE user_id = $1
            RETURNING total_xp
            "#,
        )
        .bind(user_id)
        .bind(amount)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(Self::xp_row(row.0))
    }

    pub async fn get_login_streak(&self, user_id: Uuid) -> AppResult<UserStreakRow> {
        self.ensure_user(user_id).await?;
        let row: (i32, i32, Option<NaiveDate>) = sqlx::query_as(
            "SELECT current_streak, longest_streak, last_login_date FROM user_streaks WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(UserStreakRow {
            current_streak: row.0,
            longest_streak: row.1,
            last_login_date: row.2,
        })
    }

    pub async fn record_login_streak(&self, user_id: Uuid) -> AppResult<UserStreakRow> {
        self.ensure_user(user_id).await?;
        let today = Utc::now().date_naive();
        let row: (i32, i32, Option<NaiveDate>) = sqlx::query_as(
            r#"
            UPDATE user_streaks SET
                current_streak = CASE
                    WHEN last_login_date = $2::date THEN current_streak
                    WHEN last_login_date = ($2::date - INTERVAL '1 day')::date THEN current_streak + 1
                    ELSE 1
                END,
                longest_streak = GREATEST(longest_streak, CASE
                    WHEN last_login_date = $2::date THEN current_streak
                    WHEN last_login_date = ($2::date - INTERVAL '1 day')::date THEN current_streak + 1
                    ELSE 1
                END),
                last_login_date = $2::date
            WHERE user_id = $1
            RETURNING current_streak, longest_streak, last_login_date
            "#,
        )
        .bind(user_id)
        .bind(today)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(UserStreakRow {
            current_streak: row.0,
            longest_streak: row.1,
            last_login_date: row.2,
        })
    }

    pub async fn list_achievements_for_user(&self, user_id: Uuid) -> AppResult<Vec<AchievementRow>> {
        self.ensure_user(user_id).await?;
        let rows = sqlx::query_as::<_, (i32, String, String, String, i32, String, Option<DateTime<Utc>>)>(
            r#"
            SELECT a.id, a.slug, a.title_de, a.description_de, a.xp_reward, a.badge_slug, ua.unlocked_at
            FROM achievements a
            LEFT JOIN user_achievements ua ON ua.achievement_id = a.id AND ua.user_id = $1
            ORDER BY a.id
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .map(|r| AchievementRow {
                id: r.0,
                slug: r.1,
                title_de: r.2,
                description_de: r.3,
                xp_reward: r.4,
                badge_slug: r.5,
                unlocked_at: r.6,
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
        let inserted = sqlx::query(
            r#"
            INSERT INTO user_achievements (user_id, achievement_id)
            SELECT $1, id FROM achievements WHERE slug = $2
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(slug)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        if inserted.rows_affected() == 0 {
            return Ok(None);
        }
        self.add_xp(user_id, def.xp_reward).await?;
        Ok(Some(def))
    }

    async fn bump_mission(&self, user_id: Uuid, slug: &str, delta: i32) -> AppResult<()> {
        let Some(def) = mission_catalog().into_iter().find(|m| m.slug == slug) else {
            return Ok(());
        };
        let today = Utc::now().date_naive();
        let period = period_start_for_type(def.mission_type, today);
        sqlx::query(
            r#"
            INSERT INTO user_mission_progress (user_id, mission_id, progress, period_start)
            SELECT $1, id, $3, $4::date FROM missions WHERE slug = $2
            ON CONFLICT (user_id, mission_id, period_start) DO UPDATE
            SET progress = user_mission_progress.progress + $3,
                completed_at = CASE
                    WHEN user_mission_progress.claimed_at IS NULL
                         AND user_mission_progress.progress + $3 >= (SELECT target_count FROM missions WHERE slug = $2)
                    THEN COALESCE(user_mission_progress.completed_at, NOW())
                    ELSE user_mission_progress.completed_at
                END
            WHERE user_mission_progress.claimed_at IS NULL
            "#,
        )
        .bind(user_id)
        .bind(slug)
        .bind(delta)
        .bind(period)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
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

    pub async fn list_missions_for_user(&self, user_id: Uuid) -> AppResult<Vec<MissionRow>> {
        self.ensure_user(user_id).await?;
        let today = Utc::now().date_naive();
        let rows = sqlx::query_as::<_, (i32, String, String, String, i32, Decimal, i32, i32, Option<DateTime<Utc>>, Option<DateTime<Utc>>)>(
            r#"
            SELECT m.id, m.slug, m.title_de, m.type, m.target_count, m.reward_usdt, m.xp_reward,
                   COALESCE(p.progress, 0), p.completed_at, p.claimed_at
            FROM missions m
            LEFT JOIN user_mission_progress p ON p.mission_id = m.id AND p.user_id = $1
                AND p.period_start = CASE m.type
                    WHEN 'weekly' THEN date_trunc('week', $2::date)::date
                    WHEN 'monthly' THEN date_trunc('month', $2::date)::date
                    ELSE $2::date
                END
            ORDER BY m.id
            "#,
        )
        .bind(user_id)
        .bind(today)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let mission_type = r.3.clone();
                let period = period_start_for_type(&mission_type, today);
                let progress = r.7.min(r.4);
                MissionRow {
                    id: r.0,
                    slug: r.1,
                    title_de: r.2,
                    mission_type,
                    target_count: r.4,
                    reward_usdt: r.5,
                    xp_reward: r.6,
                    progress,
                    completed: r.7 >= r.4,
                    claimed: r.9.is_some(),
                    period_start: period,
                    resets_at: resets_at_utc(&r.3, today),
                }
            })
            .collect())
    }

    pub async fn claim_mission(&self, user_id: Uuid, mission_id: i32) -> AppResult<MissionRow> {
        let today = Utc::now().date_naive();
        let mrow: (String, String, String, i32, Decimal, i32) = sqlx::query_as(
            "SELECT slug, title_de, type, target_count, reward_usdt, xp_reward FROM missions WHERE id = $1",
        )
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::InvalidInput("mission not found".into()))?;

        let period = period_start_for_type(&mrow.2, today);
        let updated = sqlx::query(
            r#"
            UPDATE user_mission_progress
            SET claimed_at = NOW()
            WHERE user_id = $1 AND mission_id = $2 AND period_start = $3::date
              AND progress >= $4 AND claimed_at IS NULL
            "#,
        )
        .bind(user_id)
        .bind(mission_id)
        .bind(period)
        .bind(mrow.3)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        if updated.rows_affected() == 0 {
            return Err(AppError::InvalidInput(
                "mission not completed or already claimed".into(),
            ));
        }
        self.add_xp(user_id, mrow.5).await?;
        let prog: (i32,) = sqlx::query_as(
            "SELECT progress FROM user_mission_progress WHERE user_id = $1 AND mission_id = $2 AND period_start = $3::date",
        )
        .bind(user_id)
        .bind(mission_id)
        .bind(period)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(MissionRow {
            id: mission_id,
            slug: mrow.0,
            title_de: mrow.1,
            mission_type: mrow.2.clone(),
            target_count: mrow.3,
            reward_usdt: mrow.4,
            xp_reward: mrow.5,
            progress: prog.0.min(mrow.3),
            completed: true,
            claimed: true,
            period_start: period,
            resets_at: resets_at_utc(&mrow.2, today),
        })
    }

    pub async fn mission_reward_usdt(&self, mission_id: i32) -> AppResult<Decimal> {
        let row: (Decimal,) =
            sqlx::query_as("SELECT reward_usdt FROM missions WHERE id = $1")
                .bind(mission_id)
                .fetch_one(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn push_notification(
        &self,
        user_id: Uuid,
        category: &str,
        title: &str,
        body: &str,
    ) -> AppResult<NotificationRow> {
        let id = Uuid::new_v4();
        let row: (DateTime<Utc>,) = sqlx::query_as(
            r#"
            INSERT INTO notifications (id, user_id, category, title, body)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING created_at
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(category)
        .bind(title)
        .bind(body)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(NotificationRow {
            id,
            category: category.into(),
            title: title.into(),
            body: body.into(),
            read: false,
            created_at: row.0,
        })
    }

    pub async fn list_notifications(
        &self,
        user_id: Uuid,
        limit: u32,
    ) -> AppResult<Vec<NotificationRow>> {
        let rows = sqlx::query_as::<_, (Uuid, String, String, String, Option<DateTime<Utc>>, DateTime<Utc>)>(
            r#"
            SELECT id, category, title, body, read_at, created_at
            FROM notifications
            WHERE user_id = $1 AND archived_at IS NULL
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .map(|r| NotificationRow {
                id: r.0,
                category: r.1,
                title: r.2,
                body: r.3,
                read: r.4.is_some(),
                created_at: r.5,
            })
            .collect())
    }

    pub async fn unread_notification_count(&self, user_id: Uuid) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read_at IS NULL AND archived_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn mark_notification_read(&self, user_id: Uuid, id: Uuid) -> AppResult<bool> {
        let res = sqlx::query(
            "UPDATE notifications SET read_at = NOW() WHERE id = $1 AND user_id = $2 AND read_at IS NULL",
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn mark_all_notifications_read(&self, user_id: Uuid) -> AppResult<i64> {
        let res = sqlx::query(
            "UPDATE notifications SET read_at = NOW() WHERE user_id = $1 AND read_at IS NULL",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(res.rows_affected() as i64)
    }

    pub async fn referral_dashboard(&self, user_id: Uuid) -> AppResult<ReferralDashboard> {
        let today = Utc::now().date_naive();
        let active_cutoff = today - chrono::Duration::days(7);
        let stats: (i32, i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT u.referral_count,
                   COUNT(r.id) FILTER (WHERE r.last_active_date >= $2::date),
                   COUNT(r.id) FILTER (WHERE r.last_active_date < $2::date OR r.last_active_date IS NULL),
                   COUNT(r.id) FILTER (WHERE r.referral_bonus_paid = false)
            FROM users u
            LEFT JOIN users r ON r.referred_by = u.id
            WHERE u.id = $1
            GROUP BY u.id, u.referral_count
            "#,
        )
        .bind(user_id)
        .bind(active_cutoff)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        let conversion = if stats.0 > 0 {
            (stats.1 as f64 / stats.0 as f64) * 100.0
        } else {
            0.0
        };
        Ok(ReferralDashboard {
            referral_code: ReferralEngine::code_for_user(user_id),
            total_earnings_usdt: ReferralEngine::referrer_bonus() * Decimal::from(stats.0),
            referral_count: stats.0,
            active_referrals: stats.1 as i32,
            inactive_referrals: stats.2 as i32,
            pending_bonuses: stats.3 as i32,
            conversion_rate_pct: conversion,
        })
    }

    pub async fn is_early_user(&self, user_id: Uuid) -> AppResult<bool> {
        let row: (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM (
                    SELECT id FROM users ORDER BY created_at ASC LIMIT 100
                ) early WHERE early.id = $1
            )
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn onboarding_claimed(&self, user_id: Uuid) -> AppResult<bool> {
        let row: (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM notifications WHERE user_id = $1 AND category = 'onboarding_claimed')",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn mark_onboarding_claimed(&self, user_id: Uuid) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO notifications (user_id, category, title, body) VALUES ($1, 'onboarding_claimed', 'claimed', '')",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }
}
