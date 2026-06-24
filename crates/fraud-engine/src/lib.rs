use rust_decimal::Decimal;
use shared::AppError;

pub const MAX_WATCHES_PER_DAY: u32 = 30;

const MAX_WATCH_SECS_PER_SESSION: u32 = 3600;
const MIN_WATCH_SECS: u32 = 15;
const MAX_SESSIONS_PER_HOUR: u32 = 120;

pub struct FraudEngine;

#[derive(Debug, Clone)]
pub struct WatchSessionCheck {
    pub watch_duration_secs: u32,
    pub sessions_last_hour: u32,
    pub watches_today: u32,
    pub is_emulator: bool,
    pub is_vpn: bool,
}

impl FraudEngine {
    pub fn validate_watch(check: &WatchSessionCheck) -> Result<f64, AppError> {
        if check.is_emulator {
            return Err(AppError::FraudBlocked("emulator detected".into()));
        }
        if check.is_vpn {
            return Err(AppError::FraudBlocked("vpn detected".into()));
        }
        if check.watch_duration_secs < MIN_WATCH_SECS {
            return Err(AppError::InvalidInput(format!(
                "watch too short (min {MIN_WATCH_SECS}s)"
            )));
        }
        if check.watch_duration_secs > MAX_WATCH_SECS_PER_SESSION {
            return Err(AppError::FraudBlocked("session duration exceeded".into()));
        }
        if check.sessions_last_hour > MAX_SESSIONS_PER_HOUR {
            return Err(AppError::FraudBlocked("rate limit exceeded".into()));
        }
        if check.watches_today >= MAX_WATCHES_PER_DAY {
            return Err(AppError::FraudBlocked("daily watch limit reached".into()));
        }

        let mut fraud_prob = 0.0f64;
        if check.sessions_last_hour > 60 {
            fraud_prob += 0.3;
        }
        if check.watch_duration_secs > 1800 {
            fraud_prob += 0.1;
        }
        Ok(fraud_prob)
    }

    /// Scales reward down as fraud probability rises; zero at >= 0.8.
    pub fn reward_multiplier(fraud_probability: f64) -> Decimal {
        if fraud_probability >= 0.8 {
            return Decimal::ZERO;
        }
        let mult = 1.0 - fraud_probability;
        Decimal::try_from(mult).unwrap_or(Decimal::ONE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_check() -> WatchSessionCheck {
        WatchSessionCheck {
            watch_duration_secs: 60,
            sessions_last_hour: 5,
            watches_today: 5,
            is_emulator: false,
            is_vpn: false,
        }
    }

    #[test]
    fn valid_session_passes() {
        assert!(FraudEngine::validate_watch(&valid_check()).is_ok());
    }

    #[test]
    fn emulator_blocked() {
        let mut c = valid_check();
        c.is_emulator = true;
        assert!(FraudEngine::validate_watch(&c).is_err());
    }

    #[test]
    fn daily_limit_blocks() {
        let mut c = valid_check();
        c.watches_today = MAX_WATCHES_PER_DAY;
        assert!(FraudEngine::validate_watch(&c).is_err());
    }

    #[test]
    fn high_fraud_zeroes_reward() {
        assert_eq!(FraudEngine::reward_multiplier(0.9), Decimal::ZERO);
        assert!(FraudEngine::reward_multiplier(0.1) < Decimal::ONE);
    }
}
