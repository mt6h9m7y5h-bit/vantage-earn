use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminAuditEntry {
    pub id: Uuid,
    pub admin_ip: Option<String>,
    pub action: String,
    pub user_id: Option<Uuid>,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl AdminAuditEntry {
    pub fn new(
        admin_ip: Option<String>,
        action: impl Into<String>,
        user_id: Option<Uuid>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            admin_ip,
            action: action.into(),
            user_id,
            details,
            created_at: Utc::now(),
        }
    }
}
