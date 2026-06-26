use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct AdminUserListRow {
    pub user_id: Uuid,
    pub referral_code: String,
    pub balance_usdt: Decimal,
    pub trust_score: i32,
    pub banned: bool,
    pub created_at: DateTime<Utc>,
    pub total_watches: u32,
    pub referral_count: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdminSearchUserHit {
    pub user_id: Uuid,
    pub referral_code: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdminSearchPayoutHit {
    pub payout_id: Uuid,
    pub user_id: Uuid,
    pub amount_usdt: Decimal,
    pub status: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdminSearchAuditHit {
    pub audit_id: Uuid,
    pub action: String,
    pub user_id: Option<Uuid>,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdminSearchReferralHit {
    pub user_id: Uuid,
    pub referral_code: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdminSearchResponse {
    pub users: Vec<AdminSearchUserHit>,
    pub payouts: Vec<AdminSearchPayoutHit>,
    pub audit: Vec<AdminSearchAuditHit>,
    pub referrals: Vec<AdminSearchReferralHit>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminUserNote {
    pub id: Uuid,
    pub user_id: Uuid,
    pub admin_note: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdminTimelineEvent {
    pub kind: String,
    pub title: String,
    pub details: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdminLiveSnapshot {
    pub pending_payouts: i64,
    pub new_users_since: i64,
    pub recent_audit_count: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdminExportUserRow {
    pub user_id: Uuid,
    pub referral_code: String,
    pub balance_usdt: Decimal,
    pub trust_score: i32,
    pub banned: bool,
    pub created_at: DateTime<Utc>,
    pub total_watches: u32,
    pub referral_count: i32,
    pub locale: String,
}

pub fn compute_risk(trust_score: i32, banned: bool, payout_history: i32, sessions_last_hour: u32, watches_today: u32) -> (i32, &'static str) {
    let mut score = 0i32;
    if banned {
        score += 100;
    }
    score += (100 - trust_score.clamp(0, 100)) / 2;
    if payout_history >= 10 {
        score += 20;
    } else if payout_history >= 5 {
        score += 10;
    }
    if sessions_last_hour > 10 {
        score += 20;
    }
    if watches_today > 15 {
        score += 10;
    }
    let score = score.min(100);
    let level = if score >= 80 {
        "CRITICAL"
    } else if score >= 55 {
        "HIGH"
    } else if score >= 30 {
        "MEDIUM"
    } else {
        "LOW"
    };
    (score, level)
}

pub fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
