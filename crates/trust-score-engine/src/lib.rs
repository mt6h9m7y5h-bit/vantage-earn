pub struct TrustScore {
    pub score: i32,
}

pub struct TrustScoreEngine;

impl TrustScoreEngine {
    pub fn calculate(
        account_age_days: i32,
        fraud_probability: f64,
        payout_history: i32,
        referral_quality: f64,
    ) -> TrustScore {
        let mut score = 50i32;
        score += account_age_days / 5;
        score += payout_history * 3;
        score += referral_quality as i32;
        score -= (fraud_probability * 100.0).round() as i32;
        TrustScore {
            score: score.clamp(0, 100),
        }
    }

    pub fn allows_instant_payout(score: i32) -> bool {
        score >= 40
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_user_mid_range() {
        let ts = TrustScoreEngine::calculate(0, 0.0, 0, 0.0);
        assert_eq!(ts.score, 50);
    }

    #[test]
    fn fraud_probability_reduces_score() {
        let ts = TrustScoreEngine::calculate(0, 0.3, 0, 0.0);
        assert_eq!(ts.score, 20);
    }

    #[test]
    fn veteran_user_high_score() {
        let ts = TrustScoreEngine::calculate(100, 0.0, 5, 10.0);
        assert!(ts.score > 60);
    }
}
