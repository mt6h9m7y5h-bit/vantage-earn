use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use referral_engine::ReferralEngine;
use rust_decimal::Decimal;
use shared::AppResult;
use tokio::sync::RwLock;
use uuid::Uuid;
use wallet_engine::{LedgerKind, WalletEngine};

use crate::state::UserProfile;
use crate::store::week_start_utc;
use crate::store::{AdminAuditEntry, AdminDailyMetric, BulkUserFilter, LedgerItem, PayoutListFilter, PayoutRequestRow};

#[derive(Clone)]
pub struct MemoryStore {
    wallet: Arc<WalletEngine>,
    users: Arc<RwLock<HashMap<Uuid, UserProfile>>>,
    trust_scores: Arc<RwLock<HashMap<Uuid, i32>>>,
    total_revenue: Arc<RwLock<Decimal>>,
    pending_payouts: Arc<RwLock<Decimal>>,
    held_payouts: Arc<RwLock<Decimal>>,
    payout_requests: Arc<RwLock<Vec<PayoutRecord>>>,
    revenue_events: Arc<RwLock<Vec<(DateTime<Utc>, Decimal)>>>,
    audit_log: Arc<RwLock<Vec<AdminAuditEntry>>>,
    feature_flags: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

#[derive(Clone)]
#[allow(dead_code)]
struct PayoutRecord {
    id: Uuid,
    user_id: Uuid,
    amount_usdt: Decimal,
    tier: String,
    status: String,
    payout_method: String,
    created_at: DateTime<Utc>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            wallet: Arc::new(WalletEngine::new()),
            users: Arc::new(RwLock::new(HashMap::new())),
            trust_scores: Arc::new(RwLock::new(HashMap::new())),
            total_revenue: Arc::new(RwLock::new(Decimal::ZERO)),
            pending_payouts: Arc::new(RwLock::new(Decimal::ZERO)),
            held_payouts: Arc::new(RwLock::new(Decimal::ZERO)),
            payout_requests: Arc::new(RwLock::new(Vec::new())),
            revenue_events: Arc::new(RwLock::new(Vec::new())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
            feature_flags: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn ping(&self) -> AppResult<bool> {
        Ok(true)
    }

    pub async fn ensure_user(&self, user_id: Uuid) -> AppResult<()> {
        self.wallet.get_or_create(user_id).await;
        self.users
            .write()
            .await
            .entry(user_id)
            .or_insert_with(UserProfile::default);
        Ok(())
    }

    pub async fn user_exists(&self, user_id: Uuid) -> AppResult<bool> {
        Ok(self.users.read().await.contains_key(&user_id))
    }

    pub async fn profile(&self, user_id: Uuid) -> AppResult<UserProfile> {
        Ok(self
            .users
            .read()
            .await
            .get(&user_id)
            .cloned()
            .unwrap_or_default())
    }

    pub async fn save_profile(&self, user_id: Uuid, profile: &UserProfile) -> AppResult<()> {
        self.users.write().await.insert(user_id, profile.clone());
        Ok(())
    }

    pub async fn balance(&self, user_id: Uuid) -> AppResult<Decimal> {
        let wallet = self.wallet.get_or_create(user_id).await;
        Ok(wallet.balance_usdt)
    }

    pub async fn credit(&self, user_id: Uuid, amount: Decimal) -> AppResult<Decimal> {
        let entry = self.wallet.credit(user_id, amount).await?;
        Ok(entry.balance_after)
    }

    pub async fn debit(&self, user_id: Uuid, amount: Decimal) -> AppResult<Decimal> {
        let entry = self.wallet.debit(user_id, amount).await?;
        Ok(entry.balance_after)
    }

    pub async fn trust_score(&self, user_id: Uuid) -> AppResult<i32> {
        Ok(self
            .trust_scores
            .read()
            .await
            .get(&user_id)
            .copied()
            .unwrap_or(50))
    }

    pub async fn set_trust_score(&self, user_id: Uuid, score: i32) -> AppResult<()> {
        self.trust_scores.write().await.insert(user_id, score);
        Ok(())
    }

    pub async fn total_revenue(&self) -> AppResult<Decimal> {
        Ok(*self.total_revenue.read().await)
    }

    pub async fn add_revenue(&self, amount: Decimal) -> AppResult<()> {
        *self.total_revenue.write().await += amount;
        self.revenue_events
            .write()
            .await
            .push((Utc::now(), amount));
        Ok(())
    }

    pub async fn pending_payouts(&self) -> AppResult<Decimal> {
        Ok(*self.pending_payouts.read().await)
    }

    pub async fn held_payouts(&self) -> AppResult<Decimal> {
        Ok(*self.held_payouts.read().await)
    }

    pub async fn add_pending_payout(&self, amount: Decimal) -> AppResult<()> {
        *self.pending_payouts.write().await += amount;
        Ok(())
    }

    pub async fn add_held_payout(&self, amount: Decimal) -> AppResult<()> {
        *self.held_payouts.write().await += amount;
        Ok(())
    }

    pub async fn record_payout_request(
        &self,
        id: Uuid,
        user_id: Uuid,
        amount: Decimal,
        tier: &str,
        status: &str,
        payout_method: &str,
    ) -> AppResult<()> {
        self.payout_requests.write().await.push(PayoutRecord {
            id,
            user_id,
            amount_usdt: amount,
            tier: tier.into(),
            status: status.into(),
            payout_method: payout_method.into(),
            created_at: Utc::now(),
        });
        Ok(())
    }

    pub async fn weekly_leaderboard(&self) -> AppResult<Vec<(Uuid, Decimal)>> {
        let week_start = week_start_utc();
        let entries = self.wallet.all_ledger().await;
        let mut totals: HashMap<Uuid, Decimal> = HashMap::new();
        for entry in entries {
            if entry.kind == LedgerKind::Credit && entry.created_at >= week_start {
                *totals.entry(entry.user_id).or_insert(Decimal::ZERO) += entry.amount_usdt;
            }
        }
        let mut ranked: Vec<(Uuid, Decimal)> = totals.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        ranked.truncate(10);
        Ok(ranked)
    }

    pub async fn user_count(&self) -> AppResult<i64> {
        Ok(self.users.read().await.len() as i64)
    }

    pub async fn recent_payout_count(&self, days: i64) -> AppResult<i64> {
        let cutoff = Utc::now() - Duration::days(days);
        let count = self
            .payout_requests
            .read()
            .await
            .iter()
            .filter(|p| p.created_at >= cutoff)
            .count();
        Ok(count as i64)
    }

    pub async fn find_user_by_referral_code(&self, code: &str) -> AppResult<Option<Uuid>> {
        let users = self.users.read().await;
        Ok(users
            .keys()
            .find(|id| ReferralEngine::matches_user(code, **id))
            .copied())
    }

    pub async fn ledger(&self, user_id: Uuid) -> AppResult<Vec<LedgerItem>> {
        let entries = self.wallet.ledger_for_user(user_id).await;
        Ok(entries
            .into_iter()
            .map(|e| LedgerItem {
                id: e.id,
                amount_usdt: e.amount_usdt,
                balance_after: e.balance_after,
                kind: match e.kind {
                    LedgerKind::Credit => "credit".into(),
                    LedgerKind::Debit => "debit".into(),
                },
                created_at: e.created_at,
            })
            .collect())
    }

    pub async fn active_users_today(&self, today: NaiveDate) -> AppResult<i64> {
        let count = self
            .users
            .read()
            .await
            .values()
            .filter(|p| p.last_active_date == Some(today))
            .count();
        Ok(count as i64)
    }

    pub async fn registrations_today(&self, today: NaiveDate) -> AppResult<i64> {
        let count = self
            .users
            .read()
            .await
            .values()
            .filter(|p| p.created_at.date_naive() == today)
            .count();
        Ok(count as i64)
    }

    pub async fn videos_today(&self, today: NaiveDate) -> AppResult<i64> {
        let total: u32 = self
            .users
            .read()
            .await
            .values()
            .filter(|p| p.last_active_date == Some(today))
            .map(|p| p.watches_today)
            .sum();
        Ok(total as i64)
    }

    pub async fn rewards_today_usdt(&self, today: NaiveDate) -> AppResult<Decimal> {
        let start = today.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let entries = self.wallet.all_ledger().await;
        let sum = entries
            .iter()
            .filter(|e| e.kind == LedgerKind::Credit && e.created_at >= start)
            .map(|e| e.amount_usdt)
            .sum();
        Ok(sum)
    }

    pub async fn avg_trust_score(&self) -> AppResult<f64> {
        let scores = self.trust_scores.read().await;
        if scores.is_empty() {
            return Ok(50.0);
        }
        let sum: i64 = scores.values().map(|&s| s as i64).sum();
        Ok(sum as f64 / scores.len() as f64)
    }

    pub async fn revenue_in_period_hours(&self, hours: i64) -> AppResult<Decimal> {
        let cutoff = Utc::now() - Duration::hours(hours);
        let sum = self
            .revenue_events
            .read()
            .await
            .iter()
            .filter(|(t, _)| *t >= cutoff)
            .map(|(_, a)| *a)
            .sum();
        Ok(sum)
    }

    pub async fn revenue_in_period_days(&self, days: i64) -> AppResult<Decimal> {
        self.revenue_in_period_hours(days * 24).await
    }

    pub async fn search_users(&self, query: &str) -> AppResult<Vec<Uuid>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(vec![]);
        }

        if let Ok(id) = Uuid::parse_str(q) {
            if self.user_exists(id).await? {
                return Ok(vec![id]);
            }
        }

        if let Some(id) = self.find_user_by_referral_code(q).await? {
            return Ok(vec![id]);
        }

        let normalized = q.replace('-', "").to_uppercase();
        if normalized.len() >= 4 {
            let users = self.users.read().await;
            let mut matches: Vec<Uuid> = users
                .keys()
                .filter(|id| {
                    id.to_string()
                        .replace('-', "")
                        .to_uppercase()
                        .contains(&normalized)
                })
                .copied()
                .collect();
            matches.sort();
            matches.truncate(20);
            return Ok(matches);
        }

        Ok(vec![])
    }

    pub async fn append_admin_audit(&self, entry: AdminAuditEntry) -> AppResult<()> {
        self.audit_log.write().await.push(entry);
        Ok(())
    }

    pub async fn admin_audit_log(&self, limit: u32) -> AppResult<Vec<AdminAuditEntry>> {
        let log = self.audit_log.read().await;
        let start = log.len().saturating_sub(limit as usize);
        Ok(log[start..].iter().rev().cloned().collect())
    }

    pub async fn list_payout_requests(
        &self,
        filter: PayoutListFilter,
        limit: u32,
    ) -> AppResult<Vec<PayoutRequestRow>> {
        let requests = self.payout_requests.read().await;
        let mut rows: Vec<PayoutRequestRow> = requests
            .iter()
            .filter(|p| filter.matches(&p.status))
            .map(|p| PayoutRequestRow {
                id: p.id,
                user_id: p.user_id,
                amount_usdt: p.amount_usdt,
                tier: p.tier.clone(),
                status: p.status.clone(),
                payout_method: p.payout_method.clone(),
                created_at: p.created_at,
            })
            .collect();
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        rows.truncate(limit as usize);
        Ok(rows)
    }

    pub async fn get_payout_request(&self, id: Uuid) -> AppResult<Option<PayoutRequestRow>> {
        Ok(self
            .payout_requests
            .read()
            .await
            .iter()
            .find(|p| p.id == id)
            .map(|p| PayoutRequestRow {
                id: p.id,
                user_id: p.user_id,
                amount_usdt: p.amount_usdt,
                tier: p.tier.clone(),
                status: p.status.clone(),
                payout_method: p.payout_method.clone(),
                created_at: p.created_at,
            }))
    }

    pub async fn update_payout_status(&self, id: Uuid, status: &str) -> AppResult<()> {
        let mut requests = self.payout_requests.write().await;
        let Some(record) = requests.iter_mut().find(|p| p.id == id) else {
            return Ok(());
        };
        record.status = status.into();
        Ok(())
    }

    pub async fn subtract_held_payout(&self, amount: Decimal) -> AppResult<()> {
        let mut held = self.held_payouts.write().await;
        *held = (*held - amount).max(Decimal::ZERO);
        Ok(())
    }

    pub async fn subtract_pending_payout(&self, amount: Decimal) -> AppResult<()> {
        let mut pending = self.pending_payouts.write().await;
        *pending = (*pending - amount).max(Decimal::ZERO);
        Ok(())
    }

    pub async fn release_held_to_pending(&self, amount: Decimal) -> AppResult<()> {
        self.subtract_held_payout(amount).await?;
        *self.pending_payouts.write().await += amount;
        Ok(())
    }

    pub async fn admin_daily_metrics(&self, days: i64) -> AppResult<Vec<AdminDailyMetric>> {
        use std::collections::HashMap;

        let today = Utc::now().date_naive();
        let start = today - Duration::days(days - 1);
        let entries = self.wallet.all_ledger().await;

        let mut by_date: HashMap<NaiveDate, (Decimal, u32, std::collections::HashSet<Uuid>)> =
            HashMap::new();
        for entry in entries {
            if entry.kind != LedgerKind::Credit {
                continue;
            }
            let day = entry.created_at.date_naive();
            if day < start {
                continue;
            }
            let slot = by_date.entry(day).or_insert((
                Decimal::ZERO,
                0,
                std::collections::HashSet::new(),
            ));
            slot.0 += entry.amount_usdt;
            slot.1 += 1;
            slot.2.insert(entry.user_id);
        }

        Ok((0..days)
            .map(|offset| {
                let date = start + Duration::days(offset);
                let (usdt, watch_count, users) = by_date
                    .get(&date)
                    .map(|(u, w, set)| (*u, *w, set.len() as u32))
                    .unwrap_or((Decimal::ZERO, 0, 0));
                AdminDailyMetric {
                    date,
                    usdt,
                    watch_count,
                    active_users: users,
                }
            })
            .collect())
    }

    pub async fn pending_payout_request_count(&self) -> AppResult<i64> {
        let count = self
            .payout_requests
            .read()
            .await
            .iter()
            .filter(|p| {
                p.status == "pending_validation" || p.status == "pending_fraud_review"
            })
            .count();
        Ok(count as i64)
    }

    pub async fn total_paid_out_usdt(&self) -> AppResult<Decimal> {
        let sum = self
            .payout_requests
            .read()
            .await
            .iter()
            .filter(|p| p.status == "paid_out")
            .map(|p| p.amount_usdt)
            .sum();
        Ok(sum)
    }

    pub async fn get_all_feature_flags(&self) -> AppResult<HashMap<String, serde_json::Value>> {
        Ok(self.feature_flags.read().await.clone())
    }

    pub async fn set_feature_flag(&self, key: &str, value: serde_json::Value) -> AppResult<()> {
        self.feature_flags
            .write()
            .await
            .insert(key.to_string(), value);
        Ok(())
    }

    pub async fn delete_feature_flag(&self, key: &str) -> AppResult<()> {
        self.feature_flags.write().await.remove(key);
        Ok(())
    }

    pub async fn list_users_for_bulk(
        &self,
        filter: BulkUserFilter,
        limit: u32,
    ) -> AppResult<Vec<Uuid>> {
        let users = self.users.read().await;
        let mut ids: Vec<Uuid> = users
            .iter()
            .filter(|(_, profile)| !profile.banned)
            .filter(|(id, profile)| match &filter {
                BulkUserFilter::All => true,
                BulkUserFilter::ActiveDays(_) => {
                    profile
                        .last_active_date
                        .map(|d| filter.active_since().is_some_and(|since| d >= since))
                        .unwrap_or(false)
                }
                BulkUserFilter::UserIds(list) => list.contains(id),
            })
            .map(|(id, _)| *id)
            .collect();
        ids.sort();
        ids.truncate(limit as usize);
        Ok(ids)
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}
