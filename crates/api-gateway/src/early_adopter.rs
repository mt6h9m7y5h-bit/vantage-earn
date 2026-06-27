use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Early-adopter signup bonus (first N email registrations before deadline).
///
/// Render / production env:
/// - `EARLY_ADOPTER_BONUS_USDT` — bonus amount (default `0.5`, `0` disables)
/// - `EARLY_ADOPTER_MAX_USERS` — max recipients (default `40`)
/// - `EARLY_ADOPTER_UNTIL` — last eligible day, `YYYY-MM-DD` (default `2026-07-31`)
#[derive(Debug, Clone)]
pub struct EarlyAdopterConfig {
    pub bonus_usdt: Decimal,
    pub max_users: u32,
    pub until: DateTime<Utc>,
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
        let until = std::env::var("EARLY_ADOPTER_UNTIL")
            .ok()
            .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
            .and_then(|d| d.and_hms_opt(23, 59, 59))
            .map(|t| t.and_utc())
            .unwrap_or_else(|| {
                NaiveDate::from_ymd_opt(2026, 7, 31)
                    .unwrap()
                    .and_hms_opt(23, 59, 59)
                    .unwrap()
                    .and_utc()
            });
        Self {
            bonus_usdt,
            max_users,
            until,
        }
    }

    pub fn is_active(&self) -> bool {
        self.bonus_usdt > Decimal::ZERO
            && self.max_users > 0
            && Utc::now() <= self.until
    }
}
