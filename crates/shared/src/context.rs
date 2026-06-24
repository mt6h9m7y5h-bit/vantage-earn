use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::money::{Currency, Usdt};

/// Isolated context passed to AI copilot — no secrets, no cross-user data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeAIContext {
    pub user_id: Uuid,
    pub system_language: String,
    pub current_balance_usdt: Usdt,
    pub localized_balance: Decimal,
    pub localized_currency: Currency,
    pub avg_daily_revenue_usdt: Usdt,
    pub referral_count: i32,
    pub streak_days: i32,
    pub estimated_days_until_goal: i32,
    pub payout_progress_percent: Decimal,
    pub top_offerwall_name: String,
    pub top_offerwall_reward_usdt: Usdt,
    pub motivational_level: i32,
}

impl SafeAIContext {
    pub fn payout_goal_eur() -> Decimal {
        Decimal::from(170)
    }
}
