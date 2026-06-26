//! Duration-based video offer catalog with auto-calculated provision display.
//!
//! Reward formula (base watch, before flat bonuses):
//! `reward_usdt = BASE * segments * streak_mult` where `segments = max(1, duration_secs / 30)`
//! and `streak_mult = 1 + min(streak_days * 5%, 50%)`. Surprise multiplier (5% chance, 3×)
//! is applied at watch completion, not in offer estimates.
//! EUR display uses fixed UI rate USDT×0.92 (see `currency-engine`).

use std::collections::HashMap;
use std::sync::LazyLock;

use chrono::NaiveDate;
use currency_engine::CurrencyEngine;
use reward_engine::RewardEngine;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

const EUR_UI_RATE: &str = "0.92";
const MAX_BONUS_SLOTS_PER_DAY: u32 = 2;
pub const BONUS_MULTIPLIER: u32 = 2;

static BONUS_SLOTS: LazyLock<RwLock<HashMap<(Uuid, NaiveDate), u32>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoOfferTier {
    Quick,
    Standard,
    Premium,
    Mega,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VideoOffer {
    pub tier: VideoOfferTier,
    pub duration_secs: u32,
    pub reward_usdt: Decimal,
    pub reward_eur_display: String,
    pub label_de: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bonus_multiplier: Option<u32>,
}

const CATALOG: [(VideoOfferTier, u32, &'static str); 4] = [
    (VideoOfferTier::Quick, 30, "Schnell"),
    (VideoOfferTier::Standard, 60, "Standard"),
    (VideoOfferTier::Premium, 90, "Premium"),
    (VideoOfferTier::Mega, 120, "Mega"),
];

fn eur_ui_rate() -> Decimal {
    Decimal::from_str_exact(EUR_UI_RATE).unwrap()
}

pub fn format_eur_display(usdt: Decimal) -> String {
    let eur = CurrencyEngine::convert_usdt_to_local(usdt, eur_ui_rate());
    let raw = format!("{eur:.2}");
    let de = raw.replace('.', ",");
    format!("≈ {de} €")
}

async fn bonus_slots_used_today(user_id: Uuid, today: NaiveDate) -> u32 {
    let map = BONUS_SLOTS.read().await;
    map.get(&(user_id, today)).copied().unwrap_or(0)
}

pub async fn bonus_slots_remaining(user_id: Uuid, today: NaiveDate) -> u32 {
    MAX_BONUS_SLOTS_PER_DAY.saturating_sub(bonus_slots_used_today(user_id, today).await)
}

/// Consume one bonus slot when a Mega bonus watch is completed.
pub async fn consume_bonus_slot(user_id: Uuid, today: NaiveDate) {
    let mut map = BONUS_SLOTS.write().await;
    let entry = map.entry((user_id, today)).or_insert(0);
    *entry = entry.saturating_add(1);
}

pub fn build_offers(streak_days: i32, bonus_slots_remaining: u32) -> Vec<VideoOffer> {
    CATALOG
        .iter()
        .map(|(tier, duration_secs, label_de)| {
            let base = RewardEngine::calculate_watch_reward(*duration_secs, streak_days);
            let bonus_multiplier = if *tier == VideoOfferTier::Mega && bonus_slots_remaining > 0 {
                Some(BONUS_MULTIPLIER)
            } else {
                None
            };
            let reward_usdt = if let Some(mult) = bonus_multiplier {
                (base * Decimal::from(mult)).round_dp(6)
            } else {
                base
            };
            VideoOffer {
                tier: *tier,
                duration_secs: *duration_secs,
                reward_usdt,
                reward_eur_display: format_eur_display(reward_usdt),
                label_de,
                bonus_multiplier,
            }
        })
        .collect()
}

pub async fn offers_for_user(user_id: Uuid, streak_days: i32, today: NaiveDate) -> Vec<VideoOffer> {
    let remaining = bonus_slots_remaining(user_id, today).await;
    build_offers(streak_days, remaining)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_tiers_with_german_labels() {
        let offers = build_offers(0, 2);
        assert_eq!(offers.len(), 4);
        assert_eq!(offers[0].label_de, "Schnell");
        assert_eq!(offers[0].duration_secs, 30);
        assert_eq!(offers[3].label_de, "Mega");
        assert_eq!(offers[3].duration_secs, 120);
    }

    #[test]
    fn streak_increases_offer_reward() {
        let base = build_offers(0, 0);
        let streaked = build_offers(10, 0);
        assert!(streaked[0].reward_usdt > base[0].reward_usdt);
    }

    #[test]
    fn mega_bonus_when_slots_remain() {
        let with_bonus = build_offers(0, 1);
        let mega = with_bonus.iter().find(|o| o.tier == VideoOfferTier::Mega).unwrap();
        assert_eq!(mega.bonus_multiplier, Some(BONUS_MULTIPLIER));

        let no_bonus = build_offers(0, 0);
        let mega_plain = no_bonus.iter().find(|o| o.tier == VideoOfferTier::Mega).unwrap();
        assert!(mega_plain.bonus_multiplier.is_none());
    }

    #[test]
    fn eur_display_uses_comma() {
        let s = format_eur_display(Decimal::from_str_exact("0.001").unwrap());
        assert!(s.starts_with('≈'));
        assert!(s.contains(','));
        assert!(s.ends_with('€'));
    }
}
