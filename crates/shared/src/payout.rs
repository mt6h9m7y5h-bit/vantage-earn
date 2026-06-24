use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub const MIN_PAYOUT_EUR: i64 = 170;
pub const DEMO_MIN_PAYOUT_USDT: &str = "0.01";

/// Shown in stats/UI — first payout may take longer due to fraud review.
pub const PAYOUT_FIRST_TIME_NOTE_DE: &str =
    "Die erste Auszahlung kann länger dauern (zusätzliche Betrugsprüfung).";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PayoutMethodInfo {
    pub method: String,
    pub label: String,
    /// Business-day estimate when applicable; `null` for hour-based methods (e.g. crypto).
    pub estimated_days_min: Option<u32>,
    pub estimated_days_max: Option<u32>,
    /// Human-readable German estimate for the UI.
    pub estimated_time_de: String,
    pub description_de: String,
}

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

    pub fn label_de(&self) -> &'static str {
        match self {
            Self::Crypto => "Krypto (USDT)",
            Self::AmazonGiftCard => "Amazon Gutschein",
            Self::Paypal => "PayPal",
        }
    }

    pub fn info(&self) -> PayoutMethodInfo {
        match self {
            Self::Crypto => PayoutMethodInfo {
                method: self.as_str().into(),
                label: self.label_de().into(),
                estimated_days_min: None,
                estimated_days_max: None,
                estimated_time_de: "ca. 1–24 Stunden (netzwerkabhängig)".into(),
                description_de: "USDT-Überweisung — Dauer hängt vom Blockchain-Netzwerk und der Auslastung ab.".into(),
            },
            Self::AmazonGiftCard => PayoutMethodInfo {
                method: self.as_str().into(),
                label: self.label_de().into(),
                estimated_days_min: Some(1),
                estimated_days_max: Some(3),
                estimated_time_de: "ca. 1–3 Werktage".into(),
                description_de: "Amazon-Gutscheincode per E-Mail — Ausstellung durch den Anbieter.".into(),
            },
            Self::Paypal => PayoutMethodInfo {
                method: self.as_str().into(),
                label: self.label_de().into(),
                estimated_days_min: Some(3),
                estimated_days_max: Some(5),
                estimated_time_de: "ca. 3–5 Werktage".into(),
                description_de: "PayPal-Überweisung — Bearbeitung durch PayPal und Banken.".into(),
            },
        }
    }

    pub fn all_info() -> Vec<PayoutMethodInfo> {
        Self::ALL.iter().map(|m| m.info()).collect()
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

    /// Brief German note on how payout tier affects processing speed (shown in UI).
    pub fn processing_note_de(&self) -> &'static str {
        match self {
            Self::Instant => {
                "Kleine Beträge werden oft schnell freigegeben; die Auszahlungsmethode bestimmt die endgültige Lieferzeit."
            }
            Self::DelayedValidation => {
                "Zusätzliche Prüfung kann 1–3 Werktage dauern, bevor der Anbieter die Auszahlung bearbeitet."
            }
            Self::DeepFraudReview => {
                "Manuelle Betrugsprüfung kann mehrere Werktage dauern, bevor die Auszahlung freigegeben wird."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payout_method_info_covers_all_methods() {
        let info = PayoutMethod::all_info();
        assert_eq!(info.len(), 3);
        assert!(info.iter().any(|i| i.method == "crypto"));
        assert!(info.iter().any(|i| i.method == "paypal"));
        assert!(info.iter().any(|i| i.method == "amazon_gift_card"));
    }

    #[test]
    fn paypal_estimated_business_days() {
        let paypal = PayoutMethod::Paypal.info();
        assert_eq!(paypal.estimated_days_min, Some(3));
        assert_eq!(paypal.estimated_days_max, Some(5));
        assert!(paypal.estimated_time_de.contains("Werktage"));
    }

    #[test]
    fn crypto_uses_hours_not_days() {
        let crypto = PayoutMethod::Crypto.info();
        assert!(crypto.estimated_days_min.is_none());
        assert!(crypto.estimated_time_de.contains("Stunden"));
    }
}
