use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::ad_config::AdConfig;
use crate::state::AppState;

pub const KEY_MAINTENANCE_MODE: &str = "maintenance_mode";
pub const KEY_MAINTENANCE_MESSAGE: &str = "maintenance_message";
pub const KEY_PAYOUT_DEMO_MODE: &str = "payout_demo_mode";
pub const KEY_WATCH_DURATION_SECS: &str = "watch_duration_secs";

pub const DEFAULT_MAINTENANCE_MESSAGE: &str =
    "Wartungsarbeiten – bitte später erneut versuchen.";

#[derive(Debug, Clone, Serialize)]
pub struct FeatureFlagsView {
    pub maintenance_mode: bool,
    pub maintenance_message: String,
    pub payout_demo_mode: bool,
    pub payout_demo_mode_source: String,
    pub watch_duration_secs: u32,
    pub watch_duration_source: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct FeatureFlagsPatch {
    pub maintenance_mode: Option<bool>,
    pub maintenance_message: Option<String>,
    pub payout_demo_mode: Option<bool>,
    #[serde(default)]
    pub clear_payout_demo_mode: bool,
    pub watch_duration_secs: Option<u32>,
    #[serde(default)]
    pub clear_watch_duration_secs: bool,
}

impl FeatureFlagsView {
    pub fn resolve(db: &HashMap<String, serde_json::Value>) -> Self {
        let maintenance_mode = db
            .get(KEY_MAINTENANCE_MODE)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let maintenance_message = db
            .get(KEY_MAINTENANCE_MESSAGE)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| DEFAULT_MAINTENANCE_MESSAGE.to_string());

        let payout_demo_env = AppState::payout_demo_mode_from_env();
        let payout_demo_mode_overridden = db.contains_key(KEY_PAYOUT_DEMO_MODE);
        let payout_demo_mode = if payout_demo_mode_overridden {
            db.get(KEY_PAYOUT_DEMO_MODE)
                .and_then(|v| v.as_bool())
                .unwrap_or(payout_demo_env)
        } else {
            payout_demo_env
        };
        let payout_demo_mode_source = if payout_demo_mode_overridden {
            "db".into()
        } else {
            "env".into()
        };

        let watch_env = AdConfig::default().watch_duration_secs;
        let watch_duration_overridden = db.contains_key(KEY_WATCH_DURATION_SECS);
        let watch_duration_secs = if watch_duration_overridden {
            db.get(KEY_WATCH_DURATION_SECS)
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(watch_env)
        } else {
            watch_env
        };
        let watch_duration_source = if watch_duration_overridden {
            "db".into()
        } else {
            "env".into()
        };

        Self {
            maintenance_mode,
            maintenance_message,
            payout_demo_mode,
            payout_demo_mode_source,
            watch_duration_secs,
            watch_duration_source,
        }
    }
}
