use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct LedgerItem {
    pub id: Uuid,
    pub amount_usdt: Decimal,
    pub balance_after: Decimal,
    pub kind: String,
    pub created_at: DateTime<Utc>,
}
