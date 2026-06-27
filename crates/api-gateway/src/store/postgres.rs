use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use shared::{AppError, AppResult};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

use referral_engine::ReferralEngine;
use crate::state::UserProfile;
use crate::store::gamification_postgres::GamificationPgStore;
use crate::store::week_start_utc;
use crate::store::{
    AdminAuditEntry, AdminDailyMetric, AdminExportUserRow, AdminLiveSnapshot, AdminSearchAuditHit,
    AdminSearchPayoutHit, AdminSearchReferralHit, AdminSearchResponse, AdminSearchUserHit,
    AdminTimelineEvent, AdminUserListRow, AdminUserNote, AnnouncementCreate, AnnouncementPatch,
    AnnouncementRow, BulkUserFilter, LedgerItem, PayoutListFilter, PayoutRequestRow,
};

#[derive(Clone)]
pub struct PgStore {
    pool: PgPool,
}

/// Render and other managed Postgres providers require TLS unless sslmode is set.
pub fn normalize_database_url(url: &str) -> String {
    if url.contains("sslmode=") || url.contains("ssl=") {
        return url.to_string();
    }
    let sep = if url.contains('?') { "&" } else { "?" };
    format!("{url}{sep}sslmode=require")
}

impl PgStore {
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let url = normalize_database_url(database_url);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(&url)
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn ping(&self) -> AppResult<bool> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| true)
            .map_err(db_err)
    }

    pub async fn ping_ms(&self) -> AppResult<Option<u64>> {
        let start = std::time::Instant::now();
        let ok = self.ping().await?;
        Ok(if ok {
            Some(start.elapsed().as_millis() as u64)
        } else {
            None
        })
    }

    pub async fn open_connections(&self) -> AppResult<Option<i64>> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM pg_stat_activity WHERE datname = current_database()",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(Some(row.0))
    }

    pub async fn user_pending_payout_count(&self, user_id: Uuid) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)::bigint FROM payout_requests
            WHERE user_id = $1 AND status IN ('pending_validation', 'pending_fraud_review')
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn ensure_user(&self, user_id: Uuid) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO users (id) VALUES ($1)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query(
            r#"
            INSERT INTO wallets (user_id) VALUES ($1)
            ON CONFLICT (user_id) DO NOTHING
            "#,
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query(
            r#"
            INSERT INTO trust_scores (user_id, score) VALUES ($1, 50)
            ON CONFLICT (user_id) DO NOTHING
            "#,
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query("INSERT INTO user_xp (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
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

    pub async fn user_exists(&self, user_id: Uuid) -> AppResult<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(row.is_some())
    }

    pub async fn user_email(&self, user_id: Uuid) -> AppResult<Option<String>> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT email FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(row.and_then(|(email,)| email))
    }

    pub async fn find_user_by_email(&self, email: &str) -> AppResult<Option<(Uuid, String)>> {
        let row: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id, password_hash FROM users WHERE email = $1 AND password_hash IS NOT NULL",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row)
    }

    pub async fn set_user_credentials(
        &self,
        user_id: Uuid,
        email: &str,
        password_hash: &str,
    ) -> AppResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE users
            SET email = $2, password_hash = $3
            WHERE id = $1 AND email IS NULL
            "#,
        )
        .bind(user_id)
        .bind(email)
        .bind(password_hash)
        .execute(&self.pool)
        .await;

        match result {
            Ok(r) if r.rows_affected() == 1 => Ok(()),
            Ok(_) => {
                if self.user_email(user_id).await?.is_some() {
                    Err(shared::AppError::InvalidInput(
                        "account already has credentials".into(),
                    ))
                } else {
                    Err(shared::AppError::UserNotFound(user_id))
                }
            }
            Err(sqlx::Error::Database(db)) if db.code().as_deref() == Some("23505") => {
                Err(shared::AppError::EmailAlreadyRegistered)
            }
            Err(e) => Err(db_err(e)),
        }
    }

    pub async fn first_email_registration_at(&self) -> AppResult<Option<DateTime<Utc>>> {
        let row: Option<(DateTime<Utc>,)> = sqlx::query_as(
            "SELECT MIN(created_at) FROM users WHERE email IS NOT NULL",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.map(|(t,)| t))
    }

    pub async fn try_grant_early_bonus(
        &self,
        user_id: Uuid,
        config: &crate::early_adopter::EarlyAdopterConfig,
    ) -> AppResult<Option<Decimal>> {
        if !config.enabled() {
            return Ok(None);
        }

        let now = Utc::now();
        let start = match config.start_override {
            Some(s) => s,
            None => self
                .first_email_registration_at()
                .await?
                .unwrap_or(now),
        };
        let end = config.campaign_end(start);
        if !config.is_campaign_active(now, start) {
            return Ok(None);
        }

        let mut tx = self.pool.begin().await.map_err(db_err)?;

        let granted: Option<(bool,)> = sqlx::query_as(
            "SELECT early_bonus_granted FROM users WHERE id = $1 FOR UPDATE",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        let Some((already_granted,)) = granted else {
            return Ok(None);
        };
        if already_granted {
            return Ok(None);
        }

        let email: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT email FROM users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        if email.and_then(|(e,)| e).is_none() {
            return Ok(None);
        }

        let updated: Option<(Uuid,)> = sqlx::query_as(
            r#"
            WITH eligible AS (
                SELECT id FROM users
                WHERE email IS NOT NULL
                  AND created_at >= $3
                  AND created_at <= $4
                ORDER BY created_at ASC
                LIMIT $2
            )
            UPDATE users u
            SET early_bonus_granted = true
            WHERE u.id = $1
              AND u.early_bonus_granted = false
              AND u.email IS NOT NULL
              AND u.created_at >= $3
              AND u.created_at <= $4
              AND u.id IN (SELECT id FROM eligible)
              AND NOW() <= $4
            RETURNING u.id
            "#,
        )
        .bind(user_id)
        .bind(config.max_users as i64)
        .bind(start)
        .bind(end)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        if updated.is_none() {
            tx.rollback().await.map_err(db_err)?;
            return Ok(None);
        }

        tx.commit().await.map_err(db_err)?;
        self.credit(user_id, config.bonus_usdt).await?;
        Ok(Some(config.bonus_usdt))
    }

    pub async fn profile(&self, user_id: Uuid) -> AppResult<UserProfile> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT created_at, locale, streak_days, referral_count,
                   payout_history, sessions_last_hour, sessions_window_started,
                   last_active_date, watches_today, total_watches, milestones_claimed,
                   last_daily_bonus_date, streak_7_bonus_claimed,
                   last_challenge_bonus_date,
                   referred_by, referral_bonus_paid, banned
            FROM users WHERE id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(row.map(Into::into).unwrap_or_default())
    }

    pub async fn save_profile(&self, user_id: Uuid, profile: &UserProfile) -> AppResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE users SET
                locale = $2,
                streak_days = $3,
                referral_count = $4,
                payout_history = $5,
                sessions_last_hour = $6,
                sessions_window_started = $7,
                last_active_date = $8,
                watches_today = $9,
                total_watches = $10,
                milestones_claimed = $11,
                last_daily_bonus_date = $12,
                streak_7_bonus_claimed = $13,
                last_challenge_bonus_date = $14,
                referred_by = $15,
                referral_bonus_paid = $16,
                banned = $17
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .bind(&profile.locale)
        .bind(profile.streak_days)
        .bind(profile.referral_count)
        .bind(profile.payout_history)
        .bind(profile.sessions_last_hour as i32)
        .bind(profile.sessions_window_started)
        .bind(profile.last_active_date)
        .bind(profile.watches_today as i32)
        .bind(profile.total_watches as i32)
        .bind(profile.milestones_claimed as i16)
        .bind(profile.last_daily_bonus_date)
        .bind(profile.streak_7_bonus_claimed)
        .bind(profile.last_challenge_bonus_date)
        .bind(profile.referred_by)
        .bind(profile.referral_bonus_paid)
        .bind(profile.banned)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        if result.rows_affected() == 0 {
            return Err(AppError::UserNotFound(user_id));
        }
        Ok(())
    }

    pub async fn balance(&self, user_id: Uuid) -> AppResult<Decimal> {
        let row: Option<(Decimal,)> =
            sqlx::query_as("SELECT balance_usdt FROM wallets WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_err)?;

        row.map(|(b,)| b)
            .ok_or(AppError::UserNotFound(user_id))
    }

    pub async fn credit(&self, user_id: Uuid, amount: Decimal) -> AppResult<Decimal> {
        if amount <= Decimal::ZERO {
            return Err(AppError::InvalidInput("credit must be positive".into()));
        }
        self.apply_ledger(user_id, amount, "credit").await
    }

    pub async fn debit(&self, user_id: Uuid, amount: Decimal) -> AppResult<Decimal> {
        if amount <= Decimal::ZERO {
            return Err(AppError::InvalidInput("debit must be positive".into()));
        }
        self.apply_ledger(user_id, -amount, "debit").await
    }

    async fn apply_ledger(
        &self,
        user_id: Uuid,
        signed_amount: Decimal,
        kind: &str,
    ) -> AppResult<Decimal> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let debit_amount = signed_amount.abs();

        let row: Option<(Decimal,)> = if signed_amount < Decimal::ZERO {
            sqlx::query_as(
                r#"
                UPDATE wallets
                SET balance_usdt = balance_usdt + $2
                WHERE user_id = $1 AND balance_usdt >= $3
                RETURNING balance_usdt
                "#,
            )
            .bind(user_id)
            .bind(signed_amount)
            .bind(debit_amount)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?
        } else {
            sqlx::query_as(
                r#"
                UPDATE wallets
                SET balance_usdt = balance_usdt + $2
                WHERE user_id = $1
                RETURNING balance_usdt
                "#,
            )
            .bind(user_id)
            .bind(signed_amount)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?
        };

        let balance_after = row.ok_or_else(|| {
            if signed_amount < Decimal::ZERO {
                AppError::InsufficientBalance {
                    have: Decimal::ZERO,
                    need: debit_amount,
                }
            } else {
                AppError::UserNotFound(user_id)
            }
        })?;

        let entry_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO ledger_entries (id, user_id, amount_usdt, balance_after, kind)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(entry_id)
        .bind(user_id)
        .bind(debit_amount)
        .bind(balance_after.0)
        .bind(kind)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(balance_after.0)
    }

    pub async fn trust_score(&self, user_id: Uuid) -> AppResult<i32> {
        let row: Option<(i32,)> =
            sqlx::query_as("SELECT score FROM trust_scores WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_err)?;

        Ok(row.map(|(s,)| s).unwrap_or(50))
    }

    pub async fn set_trust_score(&self, user_id: Uuid, score: i32) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO trust_scores (user_id, score, updated_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (user_id) DO UPDATE SET score = $2, updated_at = NOW()
            "#,
        )
        .bind(user_id)
        .bind(score)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn total_revenue(&self) -> AppResult<Decimal> {
        let row: (Decimal,) =
            sqlx::query_as("SELECT total_revenue FROM platform_stats WHERE id = 1")
                .fetch_one(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn add_revenue(&self, amount: Decimal) -> AppResult<()> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        sqlx::query(
            "UPDATE platform_stats SET total_revenue = total_revenue + $1 WHERE id = 1",
        )
        .bind(amount)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        sqlx::query("INSERT INTO revenue_events (amount_usdt) VALUES ($1)")
            .bind(amount)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    pub async fn pending_payouts(&self) -> AppResult<Decimal> {
        let row: (Decimal,) =
            sqlx::query_as("SELECT pending_payouts FROM platform_stats WHERE id = 1")
                .fetch_one(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn held_payouts(&self) -> AppResult<Decimal> {
        let row: (Decimal,) =
            sqlx::query_as("SELECT held_payouts FROM platform_stats WHERE id = 1")
                .fetch_one(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn add_pending_payout(&self, amount: Decimal) -> AppResult<()> {
        sqlx::query(
            "UPDATE platform_stats SET pending_payouts = pending_payouts + $1 WHERE id = 1",
        )
        .bind(amount)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn add_held_payout(&self, amount: Decimal) -> AppResult<()> {
        sqlx::query(
            "UPDATE platform_stats SET held_payouts = held_payouts + $1 WHERE id = 1",
        )
        .bind(amount)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
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
        sqlx::query(
            r#"
            INSERT INTO payout_requests (id, user_id, amount_usdt, tier, status, payout_method)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(amount)
        .bind(tier)
        .bind(status)
        .bind(payout_method)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn find_user_by_referral_code(&self, code: &str) -> AppResult<Option<Uuid>> {
        let prefix = code.trim().to_uppercase();
        if prefix.is_empty() || prefix.len() > 8 {
            return Ok(None);
        }
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT id FROM users
            WHERE UPPER(REPLACE(id::text, '-', '')) LIKE $1 || '%'
            LIMIT 1
            "#,
        )
        .bind(&prefix)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.map(|(id,)| id))
    }

    pub async fn ledger(&self, user_id: Uuid) -> AppResult<Vec<LedgerItem>> {
        let rows = sqlx::query_as::<_, LedgerRow>(
            r#"
            SELECT id, amount_usdt, balance_after, kind, created_at
            FROM ledger_entries
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT 50
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn weekly_leaderboard(&self) -> AppResult<Vec<(Uuid, Decimal)>> {
        let week_start = week_start_utc();
        let rows = sqlx::query_as::<_, LeaderboardRow>(
            r#"
            SELECT user_id, SUM(amount_usdt) AS weekly_earnings
            FROM ledger_entries
            WHERE kind = 'credit' AND created_at >= $1
            GROUP BY user_id
            ORDER BY weekly_earnings DESC, user_id
            LIMIT 10
            "#,
        )
        .bind(week_start)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| (r.user_id, r.weekly_earnings))
            .collect())
    }

    pub async fn user_count(&self) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn recent_payout_count(&self, days: i64) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM payout_requests
            WHERE created_at >= NOW() - make_interval(days => $1)
            "#,
        )
        .bind(days as i32)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn active_users_today(&self, today: NaiveDate) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM users WHERE last_active_date = $1",
        )
        .bind(today)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn registrations_today(&self, today: NaiveDate) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM users WHERE created_at::date = $1",
        )
        .bind(today)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn videos_today(&self, today: NaiveDate) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(watches_today), 0)::bigint FROM users WHERE last_active_date = $1",
        )
        .bind(today)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn rewards_today_usdt(&self, today: NaiveDate) -> AppResult<Decimal> {
        let start = today.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let row: (Option<Decimal>,) = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM(amount_usdt), 0)
            FROM ledger_entries
            WHERE kind = 'credit' AND created_at >= $1
            "#,
        )
        .bind(start)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0.unwrap_or(Decimal::ZERO))
    }

    pub async fn avg_trust_score(&self) -> AppResult<f64> {
        let row: (Option<f64>,) =
            sqlx::query_as("SELECT AVG(score)::float8 FROM trust_scores")
                .fetch_one(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(row.0.unwrap_or(50.0))
    }

    pub async fn revenue_in_period_hours(&self, hours: i64) -> AppResult<Decimal> {
        let row: (Option<Decimal>,) = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM(amount_usdt), 0)
            FROM revenue_events
            WHERE created_at >= NOW() - make_interval(hours => $1)
            "#,
        )
        .bind(hours as i32)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0.unwrap_or(Decimal::ZERO))
    }

    pub async fn revenue_in_period_days(&self, days: i64) -> AppResult<Decimal> {
        self.revenue_in_period_hours(days * 24).await
    }

    pub async fn search_users(&self, query: &str) -> AppResult<Vec<Uuid>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(vec![]);
        }

        if let Ok(id) = Uuid::parse_str(q) {
            if self.user_exists(id).await? {
                return Ok(vec![id]);
            }
        }

        if let Some(id) = self.find_user_by_referral_code(q).await? {
            return Ok(vec![id]);
        }

        let normalized = q.replace('-', "").to_uppercase();
        if normalized.len() >= 4 {
            let pattern = format!("{normalized}%");
            let rows = sqlx::query_as::<_, (Uuid,)>(
                r#"
                SELECT id FROM users
                WHERE UPPER(REPLACE(id::text, '-', '')) LIKE $1
                ORDER BY id
                LIMIT 20
                "#,
            )
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
            return Ok(rows.into_iter().map(|(id,)| id).collect());
        }

        Ok(vec![])
    }

    pub async fn append_admin_audit(&self, entry: AdminAuditEntry) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO admin_audit_log (id, admin_ip, action, user_id, details, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(entry.id)
        .bind(&entry.admin_ip)
        .bind(&entry.action)
        .bind(entry.user_id)
        .bind(&entry.details)
        .bind(entry.created_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn admin_audit_log(&self, limit: u32) -> AppResult<Vec<AdminAuditEntry>> {
        let rows = sqlx::query_as::<_, AuditRow>(
            r#"
            SELECT id, admin_ip, action, user_id, details, created_at
            FROM admin_audit_log
            ORDER BY created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn list_payout_requests(
        &self,
        filter: PayoutListFilter,
        limit: u32,
    ) -> AppResult<Vec<PayoutRequestRow>> {
        let rows = match filter {
            PayoutListFilter::Pending => {
                sqlx::query_as::<_, PayoutRow>(
                    r#"
                    SELECT id, user_id, amount_usdt, tier, status, payout_method, created_at
                    FROM payout_requests
                    WHERE status IN ('pending_validation', 'pending_fraud_review')
                    ORDER BY created_at DESC
                    LIMIT $1
                    "#,
                )
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?
            }
            PayoutListFilter::Approved => {
                sqlx::query_as::<_, PayoutRow>(
                    r#"
                    SELECT id, user_id, amount_usdt, tier, status, payout_method, created_at
                    FROM payout_requests
                    WHERE status IN ('approved', 'paid_out')
                    ORDER BY created_at DESC
                    LIMIT $1
                    "#,
                )
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?
            }
            PayoutListFilter::Rejected => {
                sqlx::query_as::<_, PayoutRow>(
                    r#"
                    SELECT id, user_id, amount_usdt, tier, status, payout_method, created_at
                    FROM payout_requests
                    WHERE status = 'rejected'
                    ORDER BY created_at DESC
                    LIMIT $1
                    "#,
                )
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?
            }
            PayoutListFilter::All => {
                sqlx::query_as::<_, PayoutRow>(
                    r#"
                    SELECT id, user_id, amount_usdt, tier, status, payout_method, created_at
                    FROM payout_requests
                    ORDER BY created_at DESC
                    LIMIT $1
                    "#,
                )
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?
            }
        };
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_payout_request(&self, id: Uuid) -> AppResult<Option<PayoutRequestRow>> {
        let row = sqlx::query_as::<_, PayoutRow>(
            r#"
            SELECT id, user_id, amount_usdt, tier, status, payout_method, created_at
            FROM payout_requests
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.map(Into::into))
    }

    pub async fn update_payout_status(&self, id: Uuid, status: &str) -> AppResult<()> {
        sqlx::query("UPDATE payout_requests SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(status)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn subtract_held_payout(&self, amount: Decimal) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE platform_stats
            SET held_payouts = GREATEST(held_payouts - $1, 0)
            WHERE id = 1
            "#,
        )
        .bind(amount)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn subtract_pending_payout(&self, amount: Decimal) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE platform_stats
            SET pending_payouts = GREATEST(pending_payouts - $1, 0)
            WHERE id = 1
            "#,
        )
        .bind(amount)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn release_held_to_pending(&self, amount: Decimal) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE platform_stats
            SET held_payouts = GREATEST(held_payouts - $1, 0),
                pending_payouts = pending_payouts + $1
            WHERE id = 1
            "#,
        )
        .bind(amount)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn admin_daily_metrics(&self, days: i64) -> AppResult<Vec<AdminDailyMetric>> {
        let rows = sqlx::query_as::<_, DailyMetricRow>(
            r#"
            WITH day_series AS (
                SELECT generate_series(
                    (CURRENT_DATE - ($1::int - 1) * INTERVAL '1 day')::date,
                    CURRENT_DATE,
                    '1 day'
                )::date AS date
            ),
            credits AS (
                SELECT created_at::date AS date,
                       COALESCE(SUM(amount_usdt), 0) AS usdt,
                       COUNT(*)::int AS watch_count,
                       COUNT(DISTINCT user_id)::int AS active_users
                FROM ledger_entries
                WHERE kind = 'credit'
                  AND created_at >= (CURRENT_DATE - ($1::int - 1) * INTERVAL '1 day')
                GROUP BY 1
            )
            SELECT d.date,
                   COALESCE(c.usdt, 0) AS usdt,
                   COALESCE(c.watch_count, 0) AS watch_count,
                   COALESCE(c.active_users, 0) AS active_users
            FROM day_series d
            LEFT JOIN credits c ON c.date = d.date
            ORDER BY d.date
            "#,
        )
        .bind(days as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| AdminDailyMetric {
                date: r.date,
                usdt: r.usdt,
                watch_count: r.watch_count as u32,
                active_users: r.active_users as u32,
            })
            .collect())
    }

    pub async fn pending_payout_request_count(&self) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM payout_requests
            WHERE status IN ('pending_validation', 'pending_fraud_review')
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn total_paid_out_usdt(&self) -> AppResult<Decimal> {
        let row: (Option<Decimal>,) = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM(amount_usdt), 0)
            FROM payout_requests
            WHERE status = 'paid_out'
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0.unwrap_or(Decimal::ZERO))
    }

    pub async fn get_all_feature_flags(&self) -> AppResult<HashMap<String, serde_json::Value>> {
        let rows = sqlx::query_as::<_, (String, serde_json::Value)>(
            "SELECT key, value FROM feature_flags",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows.into_iter().collect())
    }

    pub async fn set_feature_flag(&self, key: &str, value: serde_json::Value) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO feature_flags (key, value, updated_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn delete_feature_flag(&self, key: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM feature_flags WHERE key = $1")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn list_users_for_bulk(
        &self,
        filter: BulkUserFilter,
        limit: u32,
    ) -> AppResult<Vec<Uuid>> {
        let limit = limit as i64;
        let ids = match filter {
            BulkUserFilter::All => {
                sqlx::query_as::<_, (Uuid,)>(
                    r#"
                    SELECT id FROM users
                    WHERE banned = FALSE
                    ORDER BY id
                    LIMIT $1
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?
            }
            BulkUserFilter::ActiveDays(_) => {
                let since = filter
                    .active_since()
                    .ok_or_else(|| AppError::InvalidInput("invalid active filter".into()))?;
                sqlx::query_as::<_, (Uuid,)>(
                    r#"
                    SELECT id FROM users
                    WHERE banned = FALSE AND last_active_date >= $1
                    ORDER BY id
                    LIMIT $2
                    "#,
                )
                .bind(since)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?
            }
            BulkUserFilter::UserIds(list) => {
                if list.is_empty() {
                    return Ok(vec![]);
                }
                sqlx::query_as::<_, (Uuid,)>(
                    r#"
                    SELECT id FROM users
                    WHERE banned = FALSE AND id = ANY($1)
                    ORDER BY id
                    LIMIT $2
                    "#,
                )
                .bind(&list)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?
            }
        };
        Ok(ids.into_iter().map(|(id,)| id).collect())
    }

    pub async fn admin_list_users(&self, limit: u32) -> AppResult<Vec<AdminUserListRow>> {
        let rows = sqlx::query_as::<_, UserListRow>(
            r#"
            SELECT u.id AS user_id,
                   COALESCE(w.balance_usdt, 0) AS balance_usdt,
                   COALESCE(t.score, 50) AS trust_score,
                   u.banned,
                   u.created_at,
                   u.total_watches,
                   u.referral_count
            FROM users u
            LEFT JOIN wallets w ON w.user_id = u.id
            LEFT JOIN trust_scores t ON t.user_id = u.id
            ORDER BY u.created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .map(|r| AdminUserListRow {
                user_id: r.user_id,
                referral_code: ReferralEngine::code_for_user(r.user_id),
                balance_usdt: r.balance_usdt,
                trust_score: r.trust_score,
                banned: r.banned,
                created_at: r.created_at,
                total_watches: r.total_watches as u32,
                referral_count: r.referral_count,
            })
            .collect())
    }

    pub async fn admin_global_search(&self, query: &str, limit: u32) -> AppResult<AdminSearchResponse> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(AdminSearchResponse {
                users: vec![],
                payouts: vec![],
                audit: vec![],
                referrals: vec![],
            });
        }
        let pattern = format!("%{q}%");
        let lim = limit.clamp(1, 50) as i64;

        let user_rows = sqlx::query_as::<_, (Uuid,)>(
            r#"
            SELECT u.id FROM users u
            LEFT JOIN wallets w ON w.user_id = u.id
            WHERE u.id::text ILIKE $1
               OR w.balance_usdt::text ILIKE $1
            ORDER BY u.created_at DESC
            LIMIT $2
            "#,
        )
        .bind(&pattern)
        .bind(lim)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        let mut users: Vec<AdminSearchUserHit> = user_rows
            .into_iter()
            .map(|(id,)| AdminSearchUserHit {
                user_id: id,
                referral_code: ReferralEngine::code_for_user(id),
                label: format!("Nutzer {id}"),
            })
            .collect();

        if users.is_empty() {
            users = self
                .search_users(q)
                .await?
                .into_iter()
                .map(|id| AdminSearchUserHit {
                    user_id: id,
                    referral_code: ReferralEngine::code_for_user(id),
                    label: format!("Nutzer {id}"),
                })
                .collect();
        }

        let payout_rows = sqlx::query_as::<_, PayoutRow>(
            r#"
            SELECT id, user_id, amount_usdt, tier, status, payout_method, created_at
            FROM payout_requests
            WHERE id::text ILIKE $1 OR user_id::text ILIKE $1 OR amount_usdt::text ILIKE $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(&pattern)
        .bind(lim)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        let payouts = payout_rows
            .into_iter()
            .map(|r| {
                let row: PayoutRequestRow = r.into();
                AdminSearchPayoutHit {
                    payout_id: row.id,
                    user_id: row.user_id,
                    amount_usdt: row.amount_usdt,
                    status: row.status.clone(),
                    label: format!("Auszahlung {} — {}", row.amount_usdt, row.status),
                }
            })
            .collect();

        let audit_rows = sqlx::query_as::<_, AuditRow>(
            r#"
            SELECT id, admin_ip, action, user_id, details, created_at
            FROM admin_audit_log
            WHERE action ILIKE $1 OR details::text ILIKE $1 OR user_id::text ILIKE $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(&pattern)
        .bind(lim)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        let audit = audit_rows
            .into_iter()
            .map(|r| {
                let entry: AdminAuditEntry = r.into();
                AdminSearchAuditHit {
                    audit_id: entry.id,
                    action: entry.action.clone(),
                    user_id: entry.user_id,
                    label: format!("Audit: {}", entry.action),
                }
            })
            .collect();

        let mut referrals = Vec::new();
        if q.len() >= 3 {
            if let Some(id) = self.find_user_by_referral_code(q).await? {
                referrals.push(AdminSearchReferralHit {
                    user_id: id,
                    referral_code: ReferralEngine::code_for_user(id),
                    label: format!("Referral {}", ReferralEngine::code_for_user(id)),
                });
            }
        }

        Ok(AdminSearchResponse {
            users,
            payouts,
            audit,
            referrals,
        })
    }

    pub async fn admin_user_notes(&self, user_id: Uuid) -> AppResult<Vec<AdminUserNote>> {
        let rows = sqlx::query_as::<_, NoteRow>(
            r#"
            SELECT id, user_id, admin_note, created_at, created_by
            FROM admin_user_notes
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn admin_add_user_note(
        &self,
        user_id: Uuid,
        note: &str,
        created_by: &str,
    ) -> AppResult<AdminUserNote> {
        let row = sqlx::query_as::<_, NoteRow>(
            r#"
            INSERT INTO admin_user_notes (user_id, admin_note, created_by)
            VALUES ($1, $2, $3)
            RETURNING id, user_id, admin_note, created_at, created_by
            "#,
        )
        .bind(user_id)
        .bind(note)
        .bind(created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.into())
    }

    pub async fn admin_user_timeline(&self, user_id: Uuid, limit: u32) -> AppResult<Vec<AdminTimelineEvent>> {
        let mut events = Vec::new();

        if let Ok(profile) = self.profile(user_id).await {
            events.push(AdminTimelineEvent {
                kind: "registration".into(),
                title: "Registrierung".into(),
                details: serde_json::json!({ "locale": profile.locale }),
                occurred_at: profile.created_at,
            });
        }

        let ledger = self.ledger(user_id).await?;
        for item in ledger {
            let title = if item.kind == "credit" {
                "Gutschrift"
            } else {
                "Abbuchung"
            };
            events.push(AdminTimelineEvent {
                kind: format!("ledger_{}", item.kind),
                title: title.into(),
                details: serde_json::json!({
                    "amount_usdt": item.amount_usdt,
                    "balance_after": item.balance_after,
                }),
                occurred_at: item.created_at,
            });
        }

        let payouts = self.user_payouts(user_id, 200).await?;
        for p in payouts {
            events.push(AdminTimelineEvent {
                kind: "payout".into(),
                title: format!("Auszahlung ({})", p.status),
                details: serde_json::json!({
                    "payout_id": p.id,
                    "amount_usdt": p.amount_usdt,
                    "status": p.status,
                    "method": p.payout_method,
                }),
                occurred_at: p.created_at,
            });
        }

        let audit_rows = sqlx::query_as::<_, AuditRow>(
            r#"
            SELECT id, admin_ip, action, user_id, details, created_at
            FROM admin_audit_log
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        for r in audit_rows {
            let e: AdminAuditEntry = r.into();
            events.push(AdminTimelineEvent {
                kind: "audit".into(),
                title: e.action.clone(),
                details: e.details.clone(),
                occurred_at: e.created_at,
            });
        }

        events.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));
        events.truncate(limit as usize);
        Ok(events)
    }

    pub async fn admin_live_snapshot(&self, since: DateTime<Utc>) -> AppResult<AdminLiveSnapshot> {
        let pending: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM payout_requests
            WHERE status IN ('pending_validation', 'pending_fraud_review')
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;

        let new_users: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM users WHERE created_at >= $1",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;

        let audit_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM admin_audit_log WHERE created_at >= $1",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(AdminLiveSnapshot {
            pending_payouts: pending.0,
            new_users_since: new_users.0,
            recent_audit_count: audit_count.0,
        })
    }

    pub async fn admin_export_users(&self, limit: u32) -> AppResult<Vec<AdminExportUserRow>> {
        let rows = sqlx::query_as::<_, ExportUserRow>(
            r#"
            SELECT u.id AS user_id,
                   COALESCE(w.balance_usdt, 0) AS balance_usdt,
                   COALESCE(t.score, 50) AS trust_score,
                   u.banned,
                   u.created_at,
                   u.total_watches,
                   u.referral_count,
                   u.locale
            FROM users u
            LEFT JOIN wallets w ON w.user_id = u.id
            LEFT JOIN trust_scores t ON t.user_id = u.id
            ORDER BY u.created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .map(|r| AdminExportUserRow {
                user_id: r.user_id,
                referral_code: ReferralEngine::code_for_user(r.user_id),
                balance_usdt: r.balance_usdt,
                trust_score: r.trust_score,
                banned: r.banned,
                created_at: r.created_at,
                total_watches: r.total_watches as u32,
                referral_count: r.referral_count,
                locale: r.locale,
            })
            .collect())
    }

    pub async fn admin_export_audit(&self, limit: u32) -> AppResult<Vec<AdminAuditEntry>> {
        self.admin_audit_log(limit).await
    }

    pub async fn admin_export_payouts(&self, limit: u32) -> AppResult<Vec<PayoutRequestRow>> {
        self.list_payout_requests(PayoutListFilter::All, limit).await
    }

    pub async fn payout_actions_today(&self, day: NaiveDate) -> AppResult<(i64, i64)> {
        let approved: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM payout_requests
            WHERE created_at::date = $1 AND status IN ('approved', 'paid_out')
            "#,
        )
        .bind(day)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        let rejected: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM payout_requests
            WHERE created_at::date = $1 AND status = 'rejected'
            "#,
        )
        .bind(day)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok((approved.0, rejected.0))
    }

    pub async fn registrations_on(&self, day: NaiveDate) -> AppResult<i64> {
        self.registrations_today(day).await
    }

    pub async fn rewards_on(&self, day: NaiveDate) -> AppResult<Decimal> {
        let row: (Option<Decimal>,) = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM(amount_usdt), 0)
            FROM ledger_entries
            WHERE kind = 'credit' AND created_at::date = $1
            "#,
        )
        .bind(day)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0.unwrap_or(Decimal::ZERO))
    }

    pub async fn active_users_on(&self, day: NaiveDate) -> AppResult<i64> {
        self.active_users_today(day).await
    }

    pub async fn user_payouts(&self, user_id: Uuid, limit: u32) -> AppResult<Vec<PayoutRequestRow>> {
        let rows = sqlx::query_as::<_, PayoutRow>(
            r#"
            SELECT id, user_id, amount_usdt, tier, status, payout_method, created_at
            FROM payout_requests
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn user_total_earnings(&self, user_id: Uuid) -> AppResult<Decimal> {
        let row: (Option<Decimal>,) = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM(amount_usdt), 0)
            FROM ledger_entries
            WHERE user_id = $1 AND kind = 'credit'
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0.unwrap_or(Decimal::ZERO))
    }

    pub async fn user_last_activity(&self, user_id: Uuid) -> AppResult<Option<DateTime<Utc>>> {
        let row: (Option<DateTime<Utc>>,) = sqlx::query_as(
            r#"
            SELECT MAX(created_at) FROM ledger_entries WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.0)
    }

    pub async fn feature_flag_timestamps(&self) -> AppResult<HashMap<String, DateTime<Utc>>> {
        let rows = sqlx::query_as::<_, (String, DateTime<Utc>)>(
            "SELECT key, updated_at FROM feature_flags",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows.into_iter().collect())
    }

    pub async fn latest_feature_flags_audit(&self) -> AppResult<Option<(DateTime<Utc>, serde_json::Value)>> {
        let row = sqlx::query_as::<_, (DateTime<Utc>, serde_json::Value)>(
            r#"
            SELECT created_at, details
            FROM admin_audit_log
            WHERE action = 'feature_flags_update'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row)
    }
}

#[derive(sqlx::FromRow)]
struct UserListRow {
    user_id: Uuid,
    balance_usdt: Decimal,
    trust_score: i32,
    banned: bool,
    created_at: DateTime<Utc>,
    total_watches: i32,
    referral_count: i32,
}

#[derive(sqlx::FromRow)]
struct ExportUserRow {
    user_id: Uuid,
    balance_usdt: Decimal,
    trust_score: i32,
    banned: bool,
    created_at: DateTime<Utc>,
    total_watches: i32,
    referral_count: i32,
    locale: String,
}

#[derive(sqlx::FromRow)]
struct NoteRow {
    id: Uuid,
    user_id: Uuid,
    admin_note: String,
    created_at: DateTime<Utc>,
    created_by: String,
}

impl From<NoteRow> for AdminUserNote {
    fn from(row: NoteRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            admin_note: row.admin_note,
            created_at: row.created_at,
            created_by: row.created_by,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LedgerRow {
    id: Uuid,
    amount_usdt: Decimal,
    balance_after: Decimal,
    kind: String,
    created_at: DateTime<Utc>,
}

impl From<LedgerRow> for LedgerItem {
    fn from(row: LedgerRow) -> Self {
        Self {
            id: row.id,
            amount_usdt: row.amount_usdt,
            balance_after: row.balance_after,
            kind: row.kind,
            created_at: row.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LeaderboardRow {
    user_id: Uuid,
    weekly_earnings: Decimal,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    created_at: DateTime<Utc>,
    locale: String,
    streak_days: i32,
    referral_count: i32,
    payout_history: i32,
    sessions_last_hour: i32,
    sessions_window_started: DateTime<Utc>,
    last_active_date: Option<NaiveDate>,
    watches_today: i32,
    total_watches: i32,
    milestones_claimed: i16,
    last_daily_bonus_date: Option<NaiveDate>,
    streak_7_bonus_claimed: bool,
    last_challenge_bonus_date: Option<NaiveDate>,
    referred_by: Option<Uuid>,
    referral_bonus_paid: bool,
    banned: bool,
}

impl From<UserRow> for UserProfile {
    fn from(row: UserRow) -> Self {
        Self {
            created_at: row.created_at,
            locale: row.locale,
            streak_days: row.streak_days,
            referral_count: row.referral_count,
            payout_history: row.payout_history,
            sessions_last_hour: row.sessions_last_hour as u32,
            sessions_window_started: row.sessions_window_started,
            last_active_date: row.last_active_date,
            watches_today: row.watches_today as u32,
            total_watches: row.total_watches as u32,
            milestones_claimed: row.milestones_claimed as u8,
            last_daily_bonus_date: row.last_daily_bonus_date,
            streak_7_bonus_claimed: row.streak_7_bonus_claimed,
            last_challenge_bonus_date: row.last_challenge_bonus_date,
            referred_by: row.referred_by,
            referral_bonus_paid: row.referral_bonus_paid,
            banned: row.banned,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AuditRow {
    id: Uuid,
    admin_ip: Option<String>,
    action: String,
    user_id: Option<Uuid>,
    details: serde_json::Value,
    created_at: DateTime<Utc>,
}

impl From<AuditRow> for AdminAuditEntry {
    fn from(row: AuditRow) -> Self {
        Self {
            id: row.id,
            admin_ip: row.admin_ip,
            action: row.action,
            user_id: row.user_id,
            details: row.details,
            created_at: row.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PayoutRow {
    id: Uuid,
    user_id: Uuid,
    amount_usdt: Decimal,
    tier: String,
    status: String,
    payout_method: String,
    created_at: DateTime<Utc>,
}

impl From<PayoutRow> for PayoutRequestRow {
    fn from(row: PayoutRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            amount_usdt: row.amount_usdt,
            tier: row.tier,
            status: row.status,
            payout_method: row.payout_method,
            created_at: row.created_at,
        }
    }
}

impl PgStore {
    pub fn gamification(&self) -> GamificationPgStore {
        GamificationPgStore::new(self.pool.clone())
    }

    pub async fn list_active_announcements(&self) -> AppResult<Vec<AnnouncementRow>> {
        let rows = sqlx::query_as::<_, AnnouncementDbRow>(
            r#"
            SELECT id, type, title, body, priority, starts_at, ends_at, active, created_at, updated_at
            FROM announcements
            WHERE active = TRUE
              AND (starts_at IS NULL OR starts_at <= NOW())
              AND (ends_at IS NULL OR ends_at >= NOW())
            ORDER BY priority DESC, created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows.into_iter().map(AnnouncementDbRow::into_row).collect())
    }

    pub async fn list_all_announcements(&self) -> AppResult<Vec<AnnouncementRow>> {
        let rows = sqlx::query_as::<_, AnnouncementDbRow>(
            r#"
            SELECT id, type, title, body, priority, starts_at, ends_at, active, created_at, updated_at
            FROM announcements
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows.into_iter().map(AnnouncementDbRow::into_row).collect())
    }

    pub async fn get_announcement(&self, id: Uuid) -> AppResult<Option<AnnouncementRow>> {
        let row = sqlx::query_as::<_, AnnouncementDbRow>(
            r#"
            SELECT id, type, title, body, priority, starts_at, ends_at, active, created_at, updated_at
            FROM announcements WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.map(AnnouncementDbRow::into_row))
    }

    pub async fn create_announcement(&self, body: AnnouncementCreate) -> AppResult<AnnouncementRow> {
        let id = Uuid::new_v4();
        let row = sqlx::query_as::<_, AnnouncementDbRow>(
            r#"
            INSERT INTO announcements (id, type, title, body, priority, starts_at, ends_at, active)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, type, title, body, priority, starts_at, ends_at, active, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(&body.announcement_type)
        .bind(body.title.trim())
        .bind(body.body.trim())
        .bind(body.priority)
        .bind(body.starts_at)
        .bind(body.ends_at)
        .bind(body.active)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.into_row())
    }

    pub async fn patch_announcement(
        &self,
        id: Uuid,
        patch: AnnouncementPatch,
    ) -> AppResult<AnnouncementRow> {
        let existing = self
            .get_announcement(id)
            .await?
            .ok_or_else(|| AppError::InvalidInput("announcement not found".into()))?;
        let announcement_type = patch
            .announcement_type
            .unwrap_or(existing.announcement_type);
        let title = patch.title.unwrap_or(existing.title);
        let body = patch.body.unwrap_or(existing.body);
        let priority = patch.priority.unwrap_or(existing.priority);
        let starts_at = patch.starts_at.unwrap_or(existing.starts_at);
        let ends_at = patch.ends_at.unwrap_or(existing.ends_at);
        let active = patch.active.unwrap_or(existing.active);
        let row = sqlx::query_as::<_, AnnouncementDbRow>(
            r#"
            UPDATE announcements
            SET type = $2, title = $3, body = $4, priority = $5,
                starts_at = $6, ends_at = $7, active = $8, updated_at = NOW()
            WHERE id = $1
            RETURNING id, type, title, body, priority, starts_at, ends_at, active, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(announcement_type)
        .bind(title)
        .bind(body)
        .bind(priority)
        .bind(starts_at)
        .bind(ends_at)
        .bind(active)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.into_row())
    }

    pub async fn admin_insights(&self) -> AppResult<crate::admin::AdminInsights> {
        let today = Utc::now().date_naive();
        let revenue_7d = self.revenue_in_period_days(7).await?;
        let revenue_30d = self.revenue_in_period_days(30).await?;
        let rewards_today = self.rewards_today_usdt(today).await?;
        let videos_today = self.videos_today(today).await?;
        let avg_reward = if videos_today > 0 {
            rewards_today / Decimal::from(videos_today)
        } else {
            Decimal::ZERO
        };
        let paid = self.total_paid_out_usdt().await?;
        let payout_count = self.recent_payout_count(30).await?;
        let avg_payout = if payout_count > 0 {
            paid / Decimal::from(payout_count)
        } else {
            Decimal::ZERO
        };
        let cutoff = today - chrono::Duration::days(7);
        let active_7d: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM users WHERE last_active_date >= $1::date",
        )
        .bind(cutoff)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(crate::admin::AdminInsights {
            revenue_7d,
            revenue_30d,
            avg_reward_usdt: avg_reward,
            avg_payout_usdt: avg_payout,
            active_users_7d: active_7d.0,
        })
    }
}

#[derive(sqlx::FromRow)]
struct DailyMetricRow {
    date: NaiveDate,
    usdt: Decimal,
    watch_count: i32,
    active_users: i32,
}

#[derive(sqlx::FromRow)]
struct AnnouncementDbRow {
    id: Uuid,
    #[sqlx(rename = "type")]
    announcement_type: String,
    title: String,
    body: String,
    priority: i32,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
    active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AnnouncementDbRow {
    fn into_row(self) -> AnnouncementRow {
        AnnouncementRow {
            id: self.id,
            announcement_type: self.announcement_type,
            title: self.title,
            body: self.body,
            priority: self.priority,
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            active: self.active,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

fn db_err(err: sqlx::Error) -> AppError {
    AppError::InvalidInput(err.to_string())
}
