use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Early-adopter signup bonus (first N email registrations within a time window).
///
/// Render / production env:
/// - `EARLY_ADOPTER_BONUS_USDT` — bonus amount (default `0.5`, `0` disables)
/// - `EARLY_ADOPTER_MAX_USERS` — max recipients (default `40`)
/// - `EARLY_ADOPTER_DAYS` — campaign length in days from start (default `30`)
/// - `EARLY_ADOPTER_START` — optional campaign start (`YYYY-MM-DD` or RFC3339). When unset,
///   the campaign starts at the first email registration timestamp.
#[derive(Debug, Clone)]
pub struct EarlyAdopterConfig {
    pub bonus_usdt: Decimal,
    pub max_users: u32,
    pub days: u32,
    pub start_override: Option<DateTime<Utc>>,
}

impl EarlyAdopterConfig {
    pub fn from_env() -> Self {
        let bonus_usdt = std::env::var("EARLY_ADOPTER_BONUS_USDT")
            .ok()
            .and_then(|s| Decimal::from_str(&s).ok())
            .unwrap_or_else(|| Decimal::from_str("0.5").expect("const"));
        let max_users = std::env::var("EARLY_ADOPTER_MAX_USERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(40);
        let days = std::env::var("EARLY_ADOPTER_DAYS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);
        let start_override = std::env::var("EARLY_ADOPTER_START")
            .ok()
            .and_then(|s| parse_start(&s));
        Self { bonus_usdt, max_users, days, start_override }
    }

    pub fn enabled(&self) -> bool {
        self.bonus_usdt > Decimal::ZERO && self.max_users > 0 && self.days > 0
    }

    pub fn campaign_end(&self, start: DateTime<Utc>) -> DateTime<Utc> {
        start + chrono::Duration::days(self.days as i64)
    }

    pub fn is_campaign_active(&self, now: DateTime<Utc>, start: DateTime<Utc>) -> bool {
        self.enabled() && now <= self.campaign_end(start)
    }

    pub fn registration_in_window(&self, registered_at: DateTime<Utc>, start: DateTime<Utc>) -> bool {
        registered_at >= start && registered_at <= self.campaign_end(start)
    }
}

fn parse_start(raw: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|t| t.and_utc())
}
