//! Offerwall-style top offers (Bitlabs / CPX) — mock catalog until live API keys.
//!
//! Display rewards are fixed EUR estimates (30–80 ct) for UI; real payouts come from
//! provider postbacks once integrated.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopOfferCategory {
    Survey,
    AppTest,
    Registration,
    Cashback,
    Gaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopOfferProvider {
    Bitlabs,
    Cpx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopOfferStatus {
    ComingSoon,
    Mock,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TopOffer {
    pub id: &'static str,
    pub category: TopOfferCategory,
    pub label_de: &'static str,
    pub reward_eur_display: String,
    /// Reward in euro cents (30–80) for sorting and tests.
    pub reward_eur_cents: u32,
    pub effort_hint_de: &'static str,
    pub provider: TopOfferProvider,
    pub status: TopOfferStatus,
}

const CATALOG: [(&str, TopOfferCategory, &str, u32, &str, TopOfferProvider, TopOfferStatus); 5] = [
    (
        "top-survey-short",
        TopOfferCategory::Survey,
        "Umfrage",
        30,
        "ca. 2 Min.",
        TopOfferProvider::Bitlabs,
        TopOfferStatus::Mock,
    ),
    (
        "top-survey-standard",
        TopOfferCategory::Survey,
        "Umfrage",
        45,
        "ca. 5 Min.",
        TopOfferProvider::Cpx,
        TopOfferStatus::ComingSoon,
    ),
    (
        "top-app-test",
        TopOfferCategory::AppTest,
        "App testen",
        65,
        "ca. 3 Min.",
        TopOfferProvider::Bitlabs,
        TopOfferStatus::ComingSoon,
    ),
    (
        "top-registration",
        TopOfferCategory::Registration,
        "Registrieren",
        80,
        "ca. 10 Min.",
        TopOfferProvider::Cpx,
        TopOfferStatus::Mock,
    ),
    (
        "top-gaming",
        TopOfferCategory::Gaming,
        "Gaming-Angebot",
        55,
        "ca. 15 Min.",
        TopOfferProvider::Bitlabs,
        TopOfferStatus::ComingSoon,
    ),
];

pub fn format_eur_cents_display(cents: u32) -> String {
    let euros = cents / 100;
    let remainder = cents % 100;
    format!("≈ {euros},{remainder:02} €")
}

pub fn build_offers() -> Vec<TopOffer> {
    CATALOG
        .iter()
        .map(
            |(id, category, label_de, reward_eur_cents, effort_hint_de, provider, status)| {
                TopOffer {
                    id,
                    category: *category,
                    label_de,
                    reward_eur_display: format_eur_cents_display(*reward_eur_cents),
                    reward_eur_cents: *reward_eur_cents,
                    effort_hint_de,
                    provider: *provider,
                    status: *status,
                }
            },
        )
        .collect()
}

pub fn offers_for_user() -> Vec<TopOffer> {
    build_offers()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_five_offers_in_reward_range() {
        let offers = build_offers();
        assert_eq!(offers.len(), 5);
        for offer in &offers {
            assert!(offer.reward_eur_cents >= 30);
            assert!(offer.reward_eur_cents <= 80);
            assert!(offer.reward_eur_display.starts_with('≈'));
            assert!(offer.reward_eur_display.contains(','));
        }
    }

    #[test]
    fn german_labels_present() {
        let offers = build_offers();
        assert!(offers.iter().any(|o| o.label_de == "Umfrage"));
        assert!(offers.iter().any(|o| o.label_de == "App testen"));
    }

    #[test]
    fn statuses_are_mock_or_coming_soon() {
        let offers = build_offers();
        assert!(offers
            .iter()
            .all(|o| o.status == TopOfferStatus::Mock || o.status == TopOfferStatus::ComingSoon));
        assert!(offers.iter().any(|o| o.status == TopOfferStatus::Mock));
        assert!(offers
            .iter()
            .any(|o| o.status == TopOfferStatus::ComingSoon));
    }

    #[test]
    fn eur_display_uses_comma() {
        let s = format_eur_cents_display(45);
        assert_eq!(s, "≈ 0,45 €");
    }
}
