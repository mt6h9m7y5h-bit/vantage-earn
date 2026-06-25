use chrono::{Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_BULK_CREDIT_USERS: u32 = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BulkCreditFilter {
    Preset(String),
    UserIds { user_ids: Vec<Uuid> },
}

impl BulkCreditFilter {
    pub fn to_user_filter(&self) -> Result<BulkUserFilter, String> {
        match self {
            Self::Preset(name) => match name.as_str() {
                "all" => Ok(BulkUserFilter::All),
                "active_7d" => Ok(BulkUserFilter::ActiveDays(7)),
                other => Err(format!("unknown filter preset: {other}")),
            },
            Self::UserIds { user_ids } => {
                if user_ids.is_empty() {
                    return Err("user_ids must not be empty".into());
                }
                Ok(BulkUserFilter::UserIds(user_ids.clone()))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum BulkUserFilter {
    All,
    ActiveDays(u32),
    UserIds(Vec<Uuid>),
}

impl BulkUserFilter {
    pub fn active_since(&self) -> Option<NaiveDate> {
        match self {
            Self::ActiveDays(days) => {
                Some(Utc::now().date_naive() - Duration::days(*days as i64))
            }
            _ => None,
        }
    }
}
