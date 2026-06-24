use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub const MIN_PAYOUT_EUR: i64 = 170;
pub const DEMO_MIN_PAYOUT_USDT: &str = "0.01";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayoutMethod {
    Crypto,
    AmazonGiftCard,
    Paypal,
}

impl PayoutMethod {
    pub const ALL: [PayoutMethod; 3] = [
        PayoutMethod::Crypto,
        PayoutMethod::AmazonGiftCard,
        PayoutMethod::Paypal,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Crypto => "crypto",
            Self::AmazonGiftCard => "amazon_gift_card",
            Self::Paypal => "paypal",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "crypto" => Some(Self::Crypto),
            "amazon_gift_card" => Some(Self::AmazonGiftCard),
            "paypal" => Some(Self::Paypal),
            _ => None,
        }
    }

    pub fn all_strings() -> Vec<&'static str> {
        Self::ALL.iter().map(|m| m.as_str()).collect()
    }
}

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
