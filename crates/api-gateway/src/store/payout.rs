use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PayoutRequestRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub amount_usdt: Decimal,
    pub tier: String,
    pub status: String,
    pub payout_method: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdminDailyMetric {
    pub date: NaiveDate,
    pub usdt: Decimal,
    pub watch_count: u32,
    pub active_users: u32,
}

#[derive(Clone, Copy, Debug)]
pub enum PayoutListFilter {
    Pending,
    Approved,
    Rejected,
    All,
}

impl PayoutListFilter {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    pub fn matches(&self, status: &str) -> bool {
        match self {
            Self::Pending => {
                status == "pending_validation" || status == "pending_fraud_review"
            }
            Self::Approved => status == "approved" || status == "paid_out",
            Self::Rejected => status == "rejected",
            Self::All => true,
        }
    }
}

#[allow(dead_code)]
pub fn payout_awaiting_review(status: &str) -> bool {
    status == "pending_validation" || status == "pending_fraud_review"
}
