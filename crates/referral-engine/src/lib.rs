use rust_decimal::Decimal;
use uuid::Uuid;

const REFERRER_BONUS_USDT: &str = "0.001";
const REFEREE_BONUS_USDT: &str = "0.001";

pub struct ReferralEngine;

impl ReferralEngine {
    /// Short shareable code derived from user ID (first 8 hex chars).
    pub fn code_for_user(user_id: Uuid) -> String {
        user_id
            .to_string()
            .replace('-', "")
            .chars()
            .take(8)
            .collect::<String>()
            .to_uppercase()
    }

    pub fn referrer_bonus() -> Decimal {
        Decimal::from_str_exact(REFERRER_BONUS_USDT).unwrap()
    }

    pub fn referee_bonus() -> Decimal {
        Decimal::from_str_exact(REFEREE_BONUS_USDT).unwrap()
    }

    /// Match a referral code prefix back to a user ID.
    pub fn matches_user(code: &str, user_id: Uuid) -> bool {
        let normalized = code.trim().to_uppercase();
        if normalized.is_empty() || normalized.len() > 8 {
            return false;
        }
        Self::code_for_user(user_id).starts_with(&normalized)
            || normalized.starts_with(&Self::code_for_user(user_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_eight_chars() {
        let id = Uuid::new_v4();
        let code = ReferralEngine::code_for_user(id);
        assert_eq!(code.len(), 8);
    }

    #[test]
    fn matches_own_code() {
        let id = Uuid::parse_str("570df3d2-3a9b-4c55-a190-0fd0c84192b5").unwrap();
        assert!(ReferralEngine::matches_user("570DF3D2", id));
    }
}
