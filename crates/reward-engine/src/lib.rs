use rust_decimal::Decimal;

/// Base reward per 30s watch segment.
const BASE_REWARD_USDT: &str = "0.001";

/// Share of ad revenue credited to the user; the rest is platform revenue.
const USER_REVENUE_SHARE: &str = "0.40";

/// Streak bonus: +5% per day, capped at 50%.
const STREAK_BONUS_PER_DAY: f64 = 0.05;
const MAX_STREAK_BONUS: f64 = 0.50;

pub struct RewardEngine;

impl RewardEngine {
    /// Gross ad revenue for a session given the user's net reward.
    pub fn calculate_ad_revenue(user_reward: Decimal) -> Decimal {
        let share = Decimal::from_str_exact(USER_REVENUE_SHARE).unwrap();
        if user_reward <= Decimal::ZERO || share <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        (user_reward / share).round_dp(6)
    }

    pub fn calculate_watch_reward(watch_duration_secs: u32, streak_days: i32) -> Decimal {
        let base = Decimal::from_str_exact(BASE_REWARD_USDT).unwrap();
        let segments = (watch_duration_secs / 30).max(1);
        let mut reward = base * Decimal::from(segments);

        let bonus_rate = (streak_days as f64 * STREAK_BONUS_PER_DAY).min(MAX_STREAK_BONUS);
        let bonus = reward * Decimal::try_from(bonus_rate).unwrap_or_default();
        reward += bonus;

        reward.round_dp(6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_reward_for_30_seconds() {
        let r = RewardEngine::calculate_watch_reward(30, 0);
        assert_eq!(r, Decimal::from_str_exact("0.001").unwrap());
    }

    #[test]
    fn streak_increases_reward() {
        let base = RewardEngine::calculate_watch_reward(30, 0);
        let with_streak = RewardEngine::calculate_watch_reward(30, 10);
        assert!(with_streak > base);
    }

    #[test]
    fn ad_revenue_exceeds_user_reward() {
        let reward = RewardEngine::calculate_watch_reward(60, 0);
        let ad = RewardEngine::calculate_ad_revenue(reward);
        assert!(ad > reward);
    }
}
