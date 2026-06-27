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
/// - `EARLY_ADOPTER_UNTIL` — optional hard deadline (`YYYY-MM-DD`, e.g. `2026-07-31`). Overrides
///   the days-based end when set (recommended on Render).
#[derive(Debug, Clone)]
pub struct EarlyAdopterConfig {
    pub bonus_usdt: Decimal,
    pub max_users: u32,
    pub days: u32,
    pub start_override: Option<DateTime<Utc>>,
    pub until_override: Option<DateTime<Utc>>,
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
        let until_override = std::env::var("EARLY_ADOPTER_UNTIL")
            .ok()
            .and_then(|s| parse_until(&s));
        Self {
            bonus_usdt,
            max_users,
            days,
            start_override,
            until_override,
        }
    }

    pub fn enabled(&self) -> bool {
        self.bonus_usdt > Decimal::ZERO && self.max_users > 0 && self.days > 0
    }

    pub fn campaign_end(&self, start: DateTime<Utc>) -> DateTime<Utc> {
        start + chrono::Duration::days(self.days as i64)
    }

    pub fn is_campaign_active(&self, now: DateTime<Utc>, start: DateTime<Utc>) -> bool {
        if !self.enabled() {
            return false;
        }
        if let Some(until) = self.until_override {
            return now <= until;
        }
        now <= self.campaign_end(start)
    }

    pub fn registration_in_window(
        &self,
        registered_at: DateTime<Utc>,
        start: DateTime<Utc>,
    ) -> bool {
        registered_at >= start && registered_at <= self.campaign_end(start)
    }
}

fn parse_until(raw: &str) -> Option<DateTime<Utc>> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(23, 59, 59))
        .map(|t| t.and_utc())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_end_adds_days() {
        let config = EarlyAdopterConfig {
            bonus_usdt: Decimal::ONE,
            max_users: 40,
            days: 30,
            start_override: None,
            until_override: None,
        };
        let start = Utc::now();
        let end = config.campaign_end(start);
        assert_eq!((end - start).num_days(), 30);
    }

    #[test]
    fn registration_in_window_respects_bounds() {
        let config = EarlyAdopterConfig {
            bonus_usdt: Decimal::ONE,
            max_users: 40,
            days: 7,
            start_override: None,
            until_override: None,
        };
        let start = Utc::now();
        let end = config.campaign_end(start);
        assert!(config.registration_in_window(start, start));
        assert!(config.registration_in_window(end, start));
        assert!(!config.registration_in_window(start - chrono::Duration::seconds(1), start));
        assert!(!config.registration_in_window(end + chrono::Duration::seconds(1), start));
    }

    #[test]
    fn until_deadline_overrides_days_window() {
        let config = EarlyAdopterConfig {
            bonus_usdt: Decimal::ONE,
            max_users: 40,
            days: 30,
            start_override: None,
            until_override: Some(
                NaiveDate::from_ymd_opt(2020, 1, 1)
                    .unwrap()
                    .and_hms_opt(23, 59, 59)
                    .unwrap()
                    .and_utc(),
            ),
        };
        assert!(!config.is_campaign_active(Utc::now(), Utc::now()));
    }

    #[test]
    fn expired_campaign_is_inactive() {
        let config = EarlyAdopterConfig {
            bonus_usdt: Decimal::ONE,
            max_users: 40,
            days: 7,
            start_override: Some(
                NaiveDate::from_ymd_opt(2020, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc(),
            ),
            until_override: None,
        };
        let start = config.start_override.unwrap();
        assert!(!config.is_campaign_active(Utc::now(), start));
    }
}
