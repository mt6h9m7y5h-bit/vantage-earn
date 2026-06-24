use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppEvent {
    WatchCompleted(WatchCompletedPayload),
    RewardCredited(RewardCreditedPayload),
    PayoutRequested(PayoutRequestedPayload),
    TrustScoreUpdated(TrustScoreUpdatedPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchCompletedPayload {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub watch_duration_secs: u32,
    pub reward_usdt: Decimal,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardCreditedPayload {
    pub user_id: Uuid,
    pub amount_usdt: Decimal,
    pub new_balance_usdt: Decimal,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayoutRequestedPayload {
    pub user_id: Uuid,
    pub amount_usdt: Decimal,
    pub tier: String,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScoreUpdatedPayload {
    pub user_id: Uuid,
    pub score: i32,
    pub occurred_at: DateTime<Utc>,
}
