use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnnouncementRow {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub announcement_type: String,
    pub title: String,
    pub body: String,
    pub priority: i32,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AnnouncementCreate {
    #[serde(rename = "type")]
    pub announcement_type: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub starts_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ends_at: Option<DateTime<Utc>>,
    #[serde(default = "default_active")]
    pub active: bool,
}

fn default_active() -> bool {
    true
}

#[derive(Debug, Deserialize, Default)]
pub struct AnnouncementPatch {
    #[serde(rename = "type")]
    pub announcement_type: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub priority: Option<i32>,
    pub starts_at: Option<Option<DateTime<Utc>>>,
    pub ends_at: Option<Option<DateTime<Utc>>>,
    pub active: Option<bool>,
}

pub fn valid_announcement_type(t: &str) -> bool {
    matches!(
        t,
        "banner" | "popup" | "notification" | "maintenance"
    )
}
