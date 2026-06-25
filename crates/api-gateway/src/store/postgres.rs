use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use shared::{AppError, AppResult};
use sqlx::PgPool;
use uuid::Uuid;

use crate::state::UserProfile;
use crate::store::week_start_utc;
use crate::store::LedgerItem;

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
        let pool = PgPool::connect(&url).await?;
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

    pub async fn profile(&self, user_id: Uuid) -> AppResult<UserProfile> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT created_at, locale, streak_days, referral_count,
                   payout_history, sessions_last_hour, sessions_window_started,
                   last_active_date, watches_today, total_watches, milestones_claimed,
                   last_daily_bonus_date, streak_7_bonus_claimed,
                   last_challenge_bonus_date,
                   referred_by, referral_bonus_paid
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
                referral_bonus_paid = $16
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
        sqlx::query(
            "UPDATE platform_stats SET total_revenue = total_revenue + $1 WHERE id = 1",
        )
        .bind(amount)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
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
        }
    }
}

fn db_err(err: sqlx::Error) -> AppError {
    AppError::InvalidInput(err.to_string())
}
