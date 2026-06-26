use serde::Serialize;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Serialize)]
pub struct FraudSummaryResponse {
    pub high_risk_count: usize,
    pub pending_review_payouts: i64,
    pub low_trust_users: usize,
    pub fast_watch_users: usize,
    pub banned_users: usize,
    pub repeated_ip_tracking: String,
    pub wallet_pattern_tracking: String,
}

#[derive(Serialize)]
pub struct HighRiskUserRow {
    pub user_id: Uuid,
    pub referral_code: String,
    pub trust_score: i32,
    pub risk_score: i32,
    pub risk_level: String,
    pub sessions_last_hour: u32,
    pub watches_today: u32,
    pub banned: bool,
    pub flags: Vec<String>,
    pub pending_payouts: i64,
}

pub async fn fraud_summary(state: &AppState) -> FraudSummaryResponse {
    let users = state.fraud_scan_users().await;
    let high_risk_count = users
        .iter()
        .filter(|u| u.risk_level == "HIGH" || u.risk_level == "CRITICAL")
        .count();
    let low_trust_users = users.iter().filter(|u| u.trust_score < 40).count();
    let fast_watch_users = users.iter().filter(|u| u.sessions_last_hour > 8).count();
    let banned_users = users.iter().filter(|u| u.banned).count();
    let pending_review = state
        .store
        .pending_payout_request_count()
        .await
        .unwrap_or(0);

    FraudSummaryResponse {
        high_risk_count,
        pending_review_payouts: pending_review,
        low_trust_users,
        fast_watch_users,
        banned_users,
        repeated_ip_tracking: "nicht verfügbar".into(),
        wallet_pattern_tracking: "nicht verfügbar".into(),
    }
}

pub async fn high_risk_users(state: &AppState, limit: u32) -> Vec<HighRiskUserRow> {
    let mut users = state.fraud_scan_users().await;
    users.retain(|u| u.risk_level == "HIGH" || u.risk_level == "CRITICAL" || !u.flags.is_empty());
    users.sort_by(|a, b| b.risk_score.cmp(&a.risk_score));
    users.truncate(limit as usize);
    users
}
