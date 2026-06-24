use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

/// Flat bonus for the first watch of each calendar day.
pub const DAILY_LOGIN_BONUS_USDT: &str = "0.0005";

/// One-time bonus when the user reaches a 7-day streak.
pub const STREAK_7_BONUS_USDT: &str = "0.005";

/// Surprise multiplier applied to the base watch reward (not flat bonuses).
pub const SURPRISE_MULTIPLIER: u32 = 3;

/// Probability (0–100) that a watch triggers the surprise multiplier.
pub const SURPRISE_CHANCE_PERCENT: u32 = 5;

pub const MILESTONE_THRESHOLDS: [u32; 3] = [10, 50, 100];

const MILESTONE_AMOUNTS: [&str; 3] = ["0.001", "0.005", "0.01"];

const MILESTONE_BITS: [u8; 3] = [1, 2, 4];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BonusEarned {
    pub id: String,
    pub title: String,
    pub amount_usdt: Decimal,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BonusCatalogItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub amount: Decimal,
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct WatchBonusResult {
    pub flat_bonuses: Vec<BonusEarned>,
    pub surprise_multiplier: Option<u32>,
    pub surprise_extra_usdt: Decimal,
}

impl WatchBonusResult {
    pub fn flat_total(&self) -> Decimal {
        self.flat_bonuses
            .iter()
            .map(|b| b.amount_usdt)
            .fold(Decimal::ZERO, |a, b| a + b)
    }
}

pub struct BonusEngine;

impl BonusEngine {
    pub fn daily_bonus_amount() -> Decimal {
        Decimal::from_str_exact(DAILY_LOGIN_BONUS_USDT).unwrap()
    }

    pub fn streak_7_amount() -> Decimal {
        Decimal::from_str_exact(STREAK_7_BONUS_USDT).unwrap()
    }

    pub fn milestone_amount(index: usize) -> Decimal {
        Decimal::from_str_exact(MILESTONE_AMOUNTS[index]).unwrap()
    }

    pub fn is_milestone_claimed(milestones_claimed: u8, index: usize) -> bool {
        milestones_claimed & MILESTONE_BITS[index] != 0
    }

    pub fn claim_milestone(milestones_claimed: &mut u8, index: usize) {
        *milestones_claimed |= MILESTONE_BITS[index];
    }

    pub fn next_milestone(total_watches: u32, milestones_claimed: u8) -> Option<u32> {
        MILESTONE_THRESHOLDS
            .iter()
            .enumerate()
            .find(|(i, t)| total_watches < **t && !Self::is_milestone_claimed(milestones_claimed, *i))
            .map(|(_, t)| *t)
    }

    /// Deterministic surprise roll: hash(user_id + calendar date + watch index).
    /// Same inputs always produce the same outcome (auditable, no hidden randomness).
    pub fn surprise_triggered(user_id: Uuid, date: NaiveDate, watch_index: u32) -> bool {
        let seed = format!("{user_id}:{date}:{watch_index}");
        fnv1a(&seed) % 100 < u64::from(SURPRISE_CHANCE_PERCENT)
    }

    pub fn apply_surprise(base_reward: Decimal, user_id: Uuid, date: NaiveDate, watch_index: u32) -> (Decimal, Option<u32>) {
        if Self::surprise_triggered(user_id, date, watch_index) {
            let multiplied = (base_reward * Decimal::from(SURPRISE_MULTIPLIER)).round_dp(6);
            let _extra = multiplied - base_reward;
            (multiplied, Some(SURPRISE_MULTIPLIER))
        } else {
            (base_reward, None)
        }
    }

    /// Evaluate one-time / flat bonuses after a completed watch.
    pub fn evaluate_watch_bonuses(
        is_first_watch_today: bool,
        last_daily_bonus_date: Option<NaiveDate>,
        today: NaiveDate,
        total_watches: u32,
        milestones_claimed: u8,
        streak_days: i32,
        streak_7_bonus_claimed: bool,
    ) -> (WatchBonusResult, u8, bool, Option<NaiveDate>) {
        let mut result = WatchBonusResult::default();
        let mut claimed = milestones_claimed;
        let mut streak_7_claimed = streak_7_bonus_claimed;
        let mut daily_date = last_daily_bonus_date;

        if is_first_watch_today && last_daily_bonus_date != Some(today) {
            result.flat_bonuses.push(BonusEarned {
                id: "daily_login".into(),
                title: "Täglicher Login-Bonus".into(),
                amount_usdt: Self::daily_bonus_amount(),
            });
            daily_date = Some(today);
        }

        for (i, &threshold) in MILESTONE_THRESHOLDS.iter().enumerate() {
            if total_watches >= threshold && !Self::is_milestone_claimed(claimed, i) {
                Self::claim_milestone(&mut claimed, i);
                result.flat_bonuses.push(BonusEarned {
                    id: format!("milestone_{threshold}"),
                    title: format!("Meilenstein: {threshold} Videos"),
                    amount_usdt: Self::milestone_amount(i),
                });
            }
        }

        if streak_days == 7 && !streak_7_claimed {
            streak_7_claimed = true;
            result.flat_bonuses.push(BonusEarned {
                id: "streak_7".into(),
                title: "7-Tage-Streak-Bonus".into(),
                amount_usdt: Self::streak_7_amount(),
            });
        }

        (result, claimed, streak_7_claimed, daily_date)
    }

    pub fn build_catalog(
        streak_bonus_percent: u32,
        total_watches: u32,
        milestones_claimed: u8,
        daily_bonus_claimed_today: bool,
        streak_days: i32,
        streak_7_bonus_claimed: bool,
    ) -> Vec<BonusCatalogItem> {
        let mut items = Vec::new();

        items.push(BonusCatalogItem {
            id: "streak_percent".into(),
            title: "Streak-Prozent-Bonus".into(),
            description: format!(
                "+{streak_bonus_percent}% auf jede Belohnung pro Streak-Tag (max. 50%). Aktuell: {streak_days} Tage."
            ),
            amount: Decimal::ZERO,
            status: "active".into(),
        });

        items.push(BonusCatalogItem {
            id: "daily_login".into(),
            title: "Täglicher Login-Bonus".into(),
            description: "Beim ersten Video des Tages erhältst du einen Extra-Bonus.".into(),
            amount: Self::daily_bonus_amount(),
            status: if daily_bonus_claimed_today {
                "claimed".into()
            } else {
                "available".into()
            },
        });

        for (i, &threshold) in MILESTONE_THRESHOLDS.iter().enumerate() {
            let claimed = Self::is_milestone_claimed(milestones_claimed, i);
            let status = if claimed {
                "claimed".into()
            } else if total_watches >= threshold {
                "claimed".into()
            } else {
                "locked".into()
            };
            let remaining = threshold.saturating_sub(total_watches);
            let description = if claimed {
                format!("Einmaliger Bonus für {threshold} Videos insgesamt — bereits erhalten.")
            } else {
                format!("Einmaliger Bonus, wenn du insgesamt {threshold} Videos geschaut hast. Noch {remaining} Videos.")
            };
            items.push(BonusCatalogItem {
                id: format!("milestone_{threshold}"),
                title: format!("Meilenstein: {threshold} Videos"),
                description,
                amount: Self::milestone_amount(i),
                status,
            });
        }

        items.push(BonusCatalogItem {
            id: "surprise".into(),
            title: "Überraschungs-Multiplikator".into(),
            description: format!(
                "{SURPRISE_CHANCE_PERCENT}% Chance pro Video auf {SURPRISE_MULTIPLIER}× Belohnung (deterministisch pro Tag/Session)."
            ),
            amount: Decimal::ZERO,
            status: "active".into(),
        });

        let streak_7_status = if streak_7_bonus_claimed {
            "claimed".into()
        } else if streak_days >= 7 {
            "available".into()
        } else {
            "locked".into()
        };
        let days_left = 7_i32.saturating_sub(streak_days);
        let streak_7_desc = if streak_7_bonus_claimed {
            "7-Tage-Streak erreicht — Bonus erhalten. Erneut nach neuem 7-Tage-Streak.".into()
        } else if streak_days >= 7 {
            "7-Tage-Streak erreicht — Bonus beim nächsten Video.".into()
        } else {
            format!("Einmaliger Bonus bei 7 Tagen Streak in Folge. Noch {days_left} Tage.")
        };
        items.push(BonusCatalogItem {
            id: "streak_7".into(),
            title: "7-Tage-Streak-Bonus".into(),
            description: streak_7_desc,
            amount: Self::streak_7_amount(),
            status: streak_7_status,
        });

        items
    }
}

/// FNV-1a 64-bit hash — fast, deterministic, no external RNG crate.
fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn daily_bonus_only_on_first_watch_of_day() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let (r, _, _, date) = BonusEngine::evaluate_watch_bonuses(
            true, None, today, 1, 0, 1, false,
        );
        assert_eq!(r.flat_bonuses.len(), 1);
        assert_eq!(r.flat_bonuses[0].id, "daily_login");
        assert_eq!(date, Some(today));

        let (r2, _, _, _) = BonusEngine::evaluate_watch_bonuses(
            false, Some(today), today, 2, 0, 1, false,
        );
        assert!(r2.flat_bonuses.iter().all(|b| b.id != "daily_login"));
    }

    #[test]
    fn milestone_unlocks_at_threshold() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let (r, claimed, _, _) = BonusEngine::evaluate_watch_bonuses(
            false, Some(today), today, 10, 0, 3, false,
        );
        assert!(r.flat_bonuses.iter().any(|b| b.id == "milestone_10"));
        assert!(BonusEngine::is_milestone_claimed(claimed, 0));
        assert!(!BonusEngine::is_milestone_claimed(claimed, 1));
    }

    #[test]
    fn streak_7_bonus_once_per_cycle() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let (r, _, claimed, _) = BonusEngine::evaluate_watch_bonuses(
            false, Some(today), today, 5, 0, 7, false,
        );
        assert!(r.flat_bonuses.iter().any(|b| b.id == "streak_7"));
        assert!(claimed);

        let (r2, _, _, _) = BonusEngine::evaluate_watch_bonuses(
            false, Some(today), today, 6, 0, 8, true,
        );
        assert!(!r2.flat_bonuses.iter().any(|b| b.id == "streak_7"));
    }

    #[test]
    fn surprise_is_deterministic() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let a = BonusEngine::surprise_triggered(id, date, 3);
        let b = BonusEngine::surprise_triggered(id, date, 3);
        assert_eq!(a, b);
    }

    #[test]
    fn surprise_multiplier_triples_base() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let base = Decimal::from_str_exact("0.001").unwrap();
        // Find a watch_index that triggers surprise for this user/date
        let idx = (1..=200)
            .find(|&i| BonusEngine::surprise_triggered(id, date, i))
            .expect("should trigger within 200 tries at 5%");
        let (multiplied, mult) = BonusEngine::apply_surprise(base, id, date, idx);
        assert_eq!(mult, Some(3));
        assert_eq!(multiplied, Decimal::from_str_exact("0.003").unwrap());
    }

    #[test]
    fn next_milestone_skips_claimed() {
        let next = BonusEngine::next_milestone(15, 1); // 10 claimed
        assert_eq!(next, Some(50));
    }

    #[test]
    fn catalog_includes_all_bonus_types() {
        let catalog = BonusEngine::build_catalog(5, 3, 0, false, 2, false);
        let ids: Vec<_> = catalog.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"daily_login"));
        assert!(ids.contains(&"milestone_10"));
        assert!(ids.contains(&"surprise"));
        assert!(ids.contains(&"streak_7"));
        assert!(ids.contains(&"streak_percent"));
    }
}
