use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayoutTier {
    Instant,
    DelayedValidation,
    DeepFraudReview,
}

impl PayoutTier {
    /// EUR-equivalent thresholds (display/settlement snapshot).
    pub fn from_eur_amount(amount_eur: Decimal) -> Self {
        if amount_eur <= Decimal::from(20) {
            Self::Instant
        } else if amount_eur <= Decimal::from(80) {
            Self::DelayedValidation
        } else {
            Self::DeepFraudReview
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Instant => "instant",
            Self::DelayedValidation => "delayed_validation",
            Self::DeepFraudReview => "deep_fraud_review",
        }
    }
}
