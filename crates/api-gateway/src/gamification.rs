use chrono::{Datelike, NaiveDate, Utc};
use rust_decimal::Decimal;

pub const XP_PER_WATCH: i32 = 5;
pub const XP_PER_LOGIN: i32 = 10;
pub const XP_PER_REFERRAL: i32 = 25;
pub const XP_PER_WITHDRAWAL: i32 = 15;
pub const ONBOARDING_XP_REWARD: i32 = 50;
pub const ONBOARDING_USDT_REWARD: &str = "0.001";

/// Level 1–100: level = min(100, floor(sqrt(xp/100)) + 1)
pub fn level_from_xp(total_xp: i32) -> i32 {
    if total_xp <= 0 {
        return 1;
    }
    let raw = ((total_xp as f64) / 100.0).sqrt().floor() as i32 + 1;
    raw.clamp(1, 100)
}

/// XP threshold at the start of a given level (level 1 starts at 0).
pub fn xp_for_level(level: i32) -> i32 {
    if level <= 1 {
        return 0;
    }
    let l = (level - 1) as f64;
    (l * l * 100.0).ceil() as i32
}

pub fn xp_progress(total_xp: i32, level: i32) -> (i32, i32) {
    let current_start = xp_for_level(level);
    let next_start = if level >= 100 {
        current_start
    } else {
        xp_for_level(level + 1)
    };
    let in_level = total_xp - current_start;
    let needed = (next_start - current_start).max(1);
    (in_level, needed)
}

pub fn daily_period_start(today: NaiveDate) -> NaiveDate {
    today
}

pub fn weekly_period_start(today: NaiveDate) -> NaiveDate {
    let days_from_monday = today.weekday().num_days_from_monday();
    today - chrono::Duration::days(days_from_monday as i64)
}

pub fn monthly_period_start(today: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap()
}

pub fn period_start_for_type(mission_type: &str, today: NaiveDate) -> NaiveDate {
    match mission_type {
        "weekly" => weekly_period_start(today),
        "monthly" => monthly_period_start(today),
        _ => daily_period_start(today),
    }
}

pub fn resets_at_utc(mission_type: &str, today: NaiveDate) -> chrono::DateTime<Utc> {
    match mission_type {
        "weekly" => {
            let start = weekly_period_start(today);
            let next = start + chrono::Duration::days(7);
            next.and_hms_opt(0, 0, 0).unwrap().and_utc()
        }
        "monthly" => {
            let (y, m) = if today.month() == 12 {
                (today.year() + 1, 1)
            } else {
                (today.year(), today.month() + 1)
            };
            NaiveDate::from_ymd_opt(y, m, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc()
        }
        _ => {
            let next = today + chrono::Duration::days(1);
            next.and_hms_opt(0, 0, 0).unwrap().and_utc()
        }
    }
}

pub fn profile_completion_pct(
    total_watches: u32,
    referral_count: i32,
    payout_history: i32,
    achievements_unlocked: usize,
    total_achievements: usize,
) -> u8 {
    let mut score = 0u8;
    if total_watches > 0 {
        score += 25;
    }
    if referral_count > 0 {
        score += 20;
    }
    if payout_history > 0 {
        score += 25;
    }
    if total_achievements > 0 {
        let ach_pct = (achievements_unlocked * 30) / total_achievements;
        score += ach_pct as u8;
    }
    score.min(100)
}

pub fn onboarding_usdt() -> Decimal {
    Decimal::from_str_exact(ONBOARDING_USDT_REWARD).unwrap()
}

#[derive(Clone, Debug)]
pub struct AchievementDef {
    pub id: i32,
    pub slug: &'static str,
    pub title_de: &'static str,
    pub description_de: &'static str,
    pub xp_reward: i32,
    pub badge_slug: &'static str,
}

pub fn achievement_catalog() -> Vec<AchievementDef> {
    vec![
        AchievementDef {
            id: 1,
            slug: "first_ad",
            title_de: "Erstes Video",
            description_de: "Dein erstes Werbevideo geschaut",
            xp_reward: 25,
            badge_slug: "play",
        },
        AchievementDef {
            id: 2,
            slug: "ads_10",
            title_de: "10 Videos",
            description_de: "10 Werbevideos geschaut",
            xp_reward: 50,
            badge_slug: "play-10",
        },
        AchievementDef {
            id: 3,
            slug: "ads_100",
            title_de: "100 Videos",
            description_de: "100 Werbevideos geschaut",
            xp_reward: 150,
            badge_slug: "play-100",
        },
        AchievementDef {
            id: 4,
            slug: "ads_500",
            title_de: "500 Videos",
            description_de: "500 Werbevideos geschaut",
            xp_reward: 500,
            badge_slug: "play-500",
        },
        AchievementDef {
            id: 5,
            slug: "first_withdrawal",
            title_de: "Erste Auszahlung",
            description_de: "Erste Auszahlung beantragt",
            xp_reward: 100,
            badge_slug: "payout",
        },
        AchievementDef {
            id: 6,
            slug: "first_referral",
            title_de: "Erster Freund",
            description_de: "Ersten Freund eingeladen",
            xp_reward: 75,
            badge_slug: "referral",
        },
        AchievementDef {
            id: 7,
            slug: "streak_7",
            title_de: "7-Tage-Streak",
            description_de: "7 Tage in Folge aktiv",
            xp_reward: 100,
            badge_slug: "streak-7",
        },
        AchievementDef {
            id: 8,
            slug: "streak_30",
            title_de: "30-Tage-Streak",
            description_de: "30 Tage in Folge aktiv",
            xp_reward: 300,
            badge_slug: "streak-30",
        },
        AchievementDef {
            id: 9,
            slug: "early_user",
            title_de: "Early Adopter",
            description_de: "Unter den ersten Nutzern",
            xp_reward: 50,
            badge_slug: "star",
        },
    ]
}

#[derive(Clone, Debug)]
pub struct MissionDef {
    pub id: i32,
    pub slug: &'static str,
    pub title_de: &'static str,
    pub mission_type: &'static str,
    pub target_count: i32,
    pub reward_usdt: &'static str,
    pub xp_reward: i32,
}

pub fn mission_catalog() -> Vec<MissionDef> {
    vec![
        MissionDef {
            id: 1,
            slug: "daily_watch_5",
            title_de: "5 Videos heute",
            mission_type: "daily",
            target_count: 5,
            reward_usdt: "0.001",
            xp_reward: 20,
        },
        MissionDef {
            id: 2,
            slug: "daily_watch_15",
            title_de: "15 Videos heute",
            mission_type: "daily",
            target_count: 15,
            reward_usdt: "0.003",
            xp_reward: 50,
        },
        MissionDef {
            id: 3,
            slug: "daily_login",
            title_de: "Täglicher Login",
            mission_type: "daily",
            target_count: 1,
            reward_usdt: "0.0005",
            xp_reward: 10,
        },
        MissionDef {
            id: 4,
            slug: "daily_invite",
            title_de: "Freund einladen",
            mission_type: "daily",
            target_count: 1,
            reward_usdt: "0.002",
            xp_reward: 30,
        },
        MissionDef {
            id: 5,
            slug: "weekly_watch_100",
            title_de: "100 Videos diese Woche",
            mission_type: "weekly",
            target_count: 100,
            reward_usdt: "0.01",
            xp_reward: 100,
        },
        MissionDef {
            id: 6,
            slug: "weekly_invite_3",
            title_de: "3 Freunde diese Woche",
            mission_type: "weekly",
            target_count: 3,
            reward_usdt: "0.005",
            xp_reward: 75,
        },
        MissionDef {
            id: 7,
            slug: "monthly_watch_500",
            title_de: "500 Videos diesen Monat",
            mission_type: "monthly",
            target_count: 500,
            reward_usdt: "0.05",
            xp_reward: 250,
        },
        MissionDef {
            id: 8,
            slug: "monthly_first_withdrawal",
            title_de: "Erste Auszahlung",
            mission_type: "monthly",
            target_count: 1,
            reward_usdt: "0.01",
            xp_reward: 100,
        },
    ]
}

pub fn ledger_label_de(kind: &str) -> &'static str {
    match kind {
        "credit" => "Gutschrift",
        "debit" => "Abbuchung",
        _ => "Transaktion",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_curve_caps_at_100() {
        assert_eq!(level_from_xp(0), 1);
        assert_eq!(level_from_xp(100), 2);
        assert_eq!(level_from_xp(99_999_999), 100);
    }

    #[test]
    fn xp_progress_within_level() {
        let (cur, need) = xp_progress(150, 2);
        assert_eq!(cur, 50);
        assert_eq!(need, 300);
    }
}
