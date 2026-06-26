use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct UserXpRow {
    pub total_xp: i32,
    pub level: i32,
    pub xp_to_next_level: i32,
    pub xp_in_current_level: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct UserStreakRow {
    pub current_streak: i32,
    pub longest_streak: i32,
    pub last_login_date: Option<NaiveDate>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AchievementRow {
    pub id: i32,
    pub slug: String,
    pub title_de: String,
    pub description_de: String,
    pub xp_reward: i32,
    pub badge_slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlocked_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MissionRow {
    pub id: i32,
    pub slug: String,
    pub title_de: String,
    #[serde(rename = "type")]
    pub mission_type: String,
    pub target_count: i32,
    pub reward_usdt: Decimal,
    pub xp_reward: i32,
    pub progress: i32,
    pub completed: bool,
    pub claimed: bool,
    pub period_start: NaiveDate,
    pub resets_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NotificationRow {
    pub id: Uuid,
    pub category: String,
    pub title: String,
    pub body: String,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WalletHistoryItem {
    pub id: Uuid,
    pub amount_usdt: Decimal,
    pub balance_after: Decimal,
    pub kind: String,
    pub label_de: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReferralDashboard {
    pub referral_code: String,
    pub total_earnings_usdt: Decimal,
    pub referral_count: i32,
    pub active_referrals: i32,
    pub inactive_referrals: i32,
    pub pending_bonuses: i32,
    pub conversion_rate_pct: f64,
}

