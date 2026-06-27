use std::collections::{HashMap, HashSet};
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
use crate::store::{
    AdminAuditEntry, AdminDailyMetric, AdminExportUserRow, AdminLiveSnapshot, AdminSearchAuditHit,
    AdminSearchPayoutHit, AdminSearchReferralHit, AdminSearchResponse, AdminSearchUserHit,
    AdminTimelineEvent, AdminUserListRow, AdminUserNote, AnnouncementCreate, AnnouncementPatch,
    AnnouncementRow, BulkUserFilter, LedgerItem, PayoutListFilter, PayoutRequestRow,
};
use crate::store::gamification_memory::GamificationMemStore;

#[derive(Clone)]
pub struct MemoryStore {
    wallet: Arc<WalletEngine>,
    users: Arc<RwLock<HashMap<Uuid, UserProfile>>>,
    user_credentials: Arc<RwLock<HashMap<Uuid, UserCredentialRecord>>>,
    email_index: Arc<RwLock<HashMap<String, Uuid>>>,
    early_bonus_granted: Arc<RwLock<HashSet<Uuid>>>,
    gamification: GamificationMemStore,
    trust_scores: Arc<RwLock<HashMap<Uuid, i32>>>,
    total_revenue: Arc<RwLock<Decimal>>,
    pending_payouts: Arc<RwLock<Decimal>>,
    held_payouts: Arc<RwLock<Decimal>>,
    payout_requests: Arc<RwLock<Vec<PayoutRecord>>>,
    revenue_events: Arc<RwLock<Vec<(DateTime<Utc>, Decimal)>>>,
    audit_log: Arc<RwLock<Vec<AdminAuditEntry>>>,
    feature_flags: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    feature_flag_times: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    admin_notes: Arc<RwLock<Vec<AdminUserNote>>>,
    announcements: Arc<RwLock<Vec<AnnouncementRow>>>,
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

#[derive(Clone)]
struct UserCredentialRecord {
    email: String,
    password_hash: String,
}

impl MemoryStore {
    pub fn new() -> Self {
        let users = Arc::new(RwLock::new(HashMap::new()));
        Self {
            wallet: Arc::new(WalletEngine::new()),
            users: users.clone(),
            user_credentials: Arc::new(RwLock::new(HashMap::new())),
            email_index: Arc::new(RwLock::new(HashMap::new())),
            early_bonus_granted: Arc::new(RwLock::new(HashSet::new())),
            gamification: GamificationMemStore::new(users),
            trust_scores: Arc::new(RwLock::new(HashMap::new())),
            total_revenue: Arc::new(RwLock::new(Decimal::ZERO)),
            pending_payouts: Arc::new(RwLock::new(Decimal::ZERO)),
            held_payouts: Arc::new(RwLock::new(Decimal::ZERO)),
            payout_requests: Arc::new(RwLock::new(Vec::new())),
            revenue_events: Arc::new(RwLock::new(Vec::new())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
            feature_flags: Arc::new(RwLock::new(HashMap::new())),
            feature_flag_times: Arc::new(RwLock::new(HashMap::new())),
            admin_notes: Arc::new(RwLock::new(Vec::new())),
            announcements: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn dev_reset(&self) -> AppResult<()> {
        *self.users.write().await = HashMap::new();
        *self.user_credentials.write().await = HashMap::new();
        *self.email_index.write().await = HashMap::new();
        *self.early_bonus_granted.write().await = HashSet::new();
        *self.trust_scores.write().await = HashMap::new();
        *self.total_revenue.write().await = Decimal::ZERO;
        *self.pending_payouts.write().await = Decimal::ZERO;
        *self.held_payouts.write().await = Decimal::ZERO;
        *self.payout_requests.write().await = Vec::new();
        *self.revenue_events.write().await = Vec::new();
        *self.audit_log.write().await = Vec::new();
        *self.feature_flags.write().await = HashMap::new();
        *self.feature_flag_times.write().await = HashMap::new();
        *self.admin_notes.write().await = Vec::new();
        *self.announcements.write().await = Vec::new();
        Ok(())
    }

    pub async fn ping(&self) -> AppResult<bool> {
        Ok(true)
    }

    pub async fn ping_ms(&self) -> AppResult<Option<u64>> {
        Ok(Some(0))
    }

    pub async fn open_connections(&self) -> AppResult<Option<i64>> {
        Ok(None)
    }

    pub async fn user_pending_payout_count(&self, user_id: Uuid) -> AppResult<i64> {
        let requests = self.payout_requests.read().await;
        Ok(requests
            .iter()
            .filter(|p| {
                p.user_id == user_id
                    && (p.status == "pending_validation" || p.status == "pending_fraud_review")
            })
            .count() as i64)
    }

    fn announcement_active(row: &AnnouncementRow, now: DateTime<Utc>) -> bool {
        if !row.active {
            return false;
        }
        if let Some(start) = row.starts_at {
            if now < start {
                return false;
            }
        }
        if let Some(end) = row.ends_at {
            if now > end {
                return false;
            }
        }
        true
    }

    pub async fn list_active_announcements(&self) -> AppResult<Vec<AnnouncementRow>> {
        let now = Utc::now();
        let rows = self.announcements.read().await;
        let mut active: Vec<_> = rows
            .iter()
            .filter(|r| Self::announcement_active(r, now))
            .cloned()
            .collect();
        active.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(active)
    }

    pub async fn list_all_announcements(&self) -> AppResult<Vec<AnnouncementRow>> {
        let mut rows = self.announcements.read().await.clone();
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(rows)
    }

    pub async fn get_announcement(&self, id: Uuid) -> AppResult<Option<AnnouncementRow>> {
        Ok(self
            .announcements
            .read()
            .await
            .iter()
            .find(|r| r.id == id)
            .cloned())
    }

    pub async fn create_announcement(&self, body: AnnouncementCreate) -> AppResult<AnnouncementRow> {
        let now = Utc::now();
        let row = AnnouncementRow {
            id: Uuid::new_v4(),
            announcement_type: body.announcement_type,
            title: body.title.trim().into(),
            body: body.body.trim().into(),
            priority: body.priority,
            starts_at: body.starts_at,
            ends_at: body.ends_at,
            active: body.active,
            created_at: now,
            updated_at: now,
        };
        self.announcements.write().await.push(row.clone());
        Ok(row)
    }

    pub async fn patch_announcement(
        &self,
        id: Uuid,
        patch: AnnouncementPatch,
    ) -> AppResult<AnnouncementRow> {
        let mut rows = self.announcements.write().await;
        let row = rows
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| shared::AppError::InvalidInput("announcement not found".into()))?;
        if let Some(t) = patch.announcement_type {
            row.announcement_type = t;
        }
        if let Some(title) = patch.title {
            row.title = title;
        }
        if let Some(body) = patch.body {
            row.body = body;
        }
        if let Some(priority) = patch.priority {
            row.priority = priority;
        }
        if let Some(starts_at) = patch.starts_at {
            row.starts_at = starts_at;
        }
        if let Some(ends_at) = patch.ends_at {
            row.ends_at = ends_at;
        }
        if let Some(active) = patch.active {
            row.active = active;
        }
        row.updated_at = Utc::now();
        Ok(row.clone())
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

    pub async fn user_email(&self, user_id: Uuid) -> AppResult<Option<String>> {
        Ok(self
            .user_credentials
            .read()
            .await
            .get(&user_id)
            .map(|c| c.email.clone()))
    }

    pub async fn find_user_by_email(&self, email: &str) -> AppResult<Option<(Uuid, String)>> {
        let user_id = self.email_index.read().await.get(email).copied();
        let Some(user_id) = user_id else {
            return Ok(None);
        };
        let creds = self.user_credentials.read().await;
        Ok(creds.get(&user_id).map(|c| (user_id, c.password_hash.clone())))
    }

    pub async fn set_user_credentials(
        &self,
        user_id: Uuid,
        email: &str,
        password_hash: &str,
    ) -> AppResult<()> {
        if self.email_index.read().await.contains_key(email) {
            return Err(shared::AppError::EmailAlreadyRegistered);
        }
        if self.user_credentials.read().await.contains_key(&user_id) {
            return Err(shared::AppError::InvalidInput(
                "account already has credentials".into(),
            ));
        }
        self.user_credentials.write().await.insert(
            user_id,
            UserCredentialRecord {
                email: email.to_string(),
                password_hash: password_hash.to_string(),
            },
        );
        self.email_index
            .write()
            .await
            .insert(email.to_string(), user_id);
        Ok(())
    }

    pub async fn first_email_registration_at(&self) -> AppResult<Option<DateTime<Utc>>> {
        let creds = self.user_credentials.read().await;
        let users = self.users.read().await;
        Ok(creds
            .keys()
            .filter_map(|id| users.get(id).map(|p| p.created_at))
            .min())
    }

    pub async fn try_grant_early_bonus(
        &self,
        user_id: Uuid,
        config: &crate::early_adopter::EarlyAdopterConfig,
    ) -> AppResult<Option<Decimal>> {
        if !config.enabled() {
            return Ok(None);
        }
        if self.early_bonus_granted.read().await.contains(&user_id) {
            return Ok(None);
        }
        if !self.user_credentials.read().await.contains_key(&user_id) {
            return Ok(None);
        }

        let now = Utc::now();
        let start = match config.start_override {
            Some(s) => s,
            None => self
                .first_email_registration_at()
                .await?
                .unwrap_or(now),
        };
        if !config.is_campaign_active(now, start) {
            return Ok(None);
        }

        let creds = self.user_credentials.read().await;
        let users = self.users.read().await;
        let Some(user_created) = users.get(&user_id).map(|p| p.created_at) else {
            return Ok(None);
        };
        if !config.registration_in_window(user_created, start) {
            return Ok(None);
        }

        let mut email_users: Vec<_> = creds
            .keys()
            .filter_map(|id| users.get(id).map(|p| (*id, p.created_at)))
            .filter(|(_, created)| config.registration_in_window(*created, start))
            .collect();
        email_users.sort_by_key(|(_, created)| *created);
        let eligible: HashSet<_> = email_users
            .iter()
            .take(config.max_users as usize)
            .map(|(id, _)| *id)
            .collect();
        if !eligible.contains(&user_id) {
            return Ok(None);
        }
        drop(creds);
        drop(users);

        self.early_bonus_granted.write().await.insert(user_id);
        self.credit(user_id, config.bonus_usdt).await?;
        Ok(Some(config.bonus_usdt))
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
        self.feature_flag_times
            .write()
            .await
            .insert(key.to_string(), Utc::now());
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

    pub async fn admin_list_users(&self, limit: u32) -> AppResult<Vec<AdminUserListRow>> {
        let users = self.users.read().await;
        let scores = self.trust_scores.read().await;
        let mut rows = Vec::new();
        for (user_id, profile) in users.iter() {
            let balance = self.wallet.balance(*user_id).await?;
            rows.push(AdminUserListRow {
                user_id: *user_id,
                referral_code: ReferralEngine::code_for_user(*user_id),
                balance_usdt: balance,
                trust_score: *scores.get(user_id).unwrap_or(&50),
                banned: profile.banned,
                created_at: profile.created_at,
                total_watches: profile.total_watches,
                referral_count: profile.referral_count,
            });
        }
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        rows.truncate(limit as usize);
        Ok(rows)
    }

    pub async fn admin_global_search(&self, query: &str, limit: u32) -> AppResult<AdminSearchResponse> {
        let q = query.trim();
        let pattern = format!("%{}%", q);
        let limit = limit.clamp(1, 50) as usize;
        let mut users = Vec::new();
        let mut payouts = Vec::new();
        let mut audit = Vec::new();
        let mut referrals = Vec::new();

        if !q.is_empty() {
            for id in self.search_users(q).await? {
                users.push(AdminSearchUserHit {
                    user_id: id,
                    referral_code: ReferralEngine::code_for_user(id),
                    label: format!("Nutzer {}", id),
                });
            }

            let requests = self.payout_requests.read().await;
            for p in requests.iter() {
                let hay = format!("{} {} {}", p.id, p.user_id, p.amount_usdt);
                if hay.to_lowercase().contains(&q.to_lowercase())
                    || p.id.to_string().to_lowercase().contains(&q.to_lowercase())
                {
                    payouts.push(AdminSearchPayoutHit {
                        payout_id: p.id,
                        user_id: p.user_id,
                        amount_usdt: p.amount_usdt,
                        status: p.status.clone(),
                        label: format!("Auszahlung {} — {}", p.amount_usdt, p.status),
                    });
                }
            }

            let log = self.audit_log.read().await;
            for e in log.iter().rev() {
                let hay = format!("{} {:?} {}", e.action, e.user_id, e.details);
                if hay.to_lowercase().contains(&q.to_lowercase()) {
                    audit.push(AdminSearchAuditHit {
                        audit_id: e.id,
                        action: e.action.clone(),
                        user_id: e.user_id,
                        label: format!("Audit: {}", e.action),
                    });
                }
            }

            if q.len() >= 3 {
                let all_users = self.users.read().await;
                for (id, _) in all_users.iter() {
                    let code = ReferralEngine::code_for_user(*id);
                    if code.to_lowercase().contains(&q.to_lowercase()) {
                        referrals.push(AdminSearchReferralHit {
                            user_id: *id,
                            referral_code: code.clone(),
                            label: format!("Referral {}", code),
                        });
                    }
                }
            }
        }

        users.truncate(limit);
        payouts.truncate(limit);
        audit.truncate(limit);
        referrals.truncate(limit);
        let _ = pattern;
        Ok(AdminSearchResponse {
            users,
            payouts,
            audit,
            referrals,
        })
    }

    pub async fn admin_user_notes(&self, user_id: Uuid) -> AppResult<Vec<AdminUserNote>> {
        let notes = self.admin_notes.read().await;
        let mut rows: Vec<AdminUserNote> = notes
            .iter()
            .filter(|n| n.user_id == user_id)
            .cloned()
            .collect();
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(rows)
    }

    pub async fn admin_add_user_note(
        &self,
        user_id: Uuid,
        note: &str,
        created_by: &str,
    ) -> AppResult<AdminUserNote> {
        let entry = AdminUserNote {
            id: Uuid::new_v4(),
            user_id,
            admin_note: note.to_string(),
            created_at: Utc::now(),
            created_by: created_by.to_string(),
        };
        self.admin_notes.write().await.push(entry.clone());
        Ok(entry)
    }

    pub async fn admin_user_timeline(&self, user_id: Uuid, limit: u32) -> AppResult<Vec<AdminTimelineEvent>> {
        let mut events = Vec::new();
        if let Ok(profile) = self.profile(user_id).await {
            events.push(AdminTimelineEvent {
                kind: "registration".into(),
                title: "Registrierung".into(),
                details: serde_json::json!({ "locale": profile.locale }),
                occurred_at: profile.created_at,
            });
        }

        let ledger = self.ledger(user_id).await?;
        for item in ledger {
            let title = if item.kind == "credit" {
                "Gutschrift"
            } else {
                "Abbuchung"
            };
            events.push(AdminTimelineEvent {
                kind: format!("ledger_{}", item.kind),
                title: title.into(),
                details: serde_json::json!({
                    "amount_usdt": item.amount_usdt,
                    "balance_after": item.balance_after,
                }),
                occurred_at: item.created_at,
            });
        }

        let requests = self.payout_requests.read().await;
        for p in requests.iter().filter(|p| p.user_id == user_id) {
            events.push(AdminTimelineEvent {
                kind: "payout".into(),
                title: format!("Auszahlung ({})", p.status),
                details: serde_json::json!({
                    "payout_id": p.id,
                    "amount_usdt": p.amount_usdt,
                    "status": p.status,
                    "method": p.payout_method,
                }),
                occurred_at: p.created_at,
            });
        }

        let log = self.audit_log.read().await;
        for e in log.iter().filter(|e| e.user_id == Some(user_id)) {
            events.push(AdminTimelineEvent {
                kind: "audit".into(),
                title: e.action.clone(),
                details: e.details.clone(),
                occurred_at: e.created_at,
            });
        }

        events.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));
        events.truncate(limit as usize);
        Ok(events)
    }

    pub async fn admin_live_snapshot(&self, since: DateTime<Utc>) -> AppResult<AdminLiveSnapshot> {
        let pending = self
            .payout_requests
            .read()
            .await
            .iter()
            .filter(|p| {
                p.status == "pending_validation" || p.status == "pending_fraud_review"
            })
            .count() as i64;
        let users = self.users.read().await;
        let new_users = users
            .values()
            .filter(|p| p.created_at >= since)
            .count() as i64;
        let audit_count = self
            .audit_log
            .read()
            .await
            .iter()
            .filter(|e| e.created_at >= since)
            .count() as i64;
        Ok(AdminLiveSnapshot {
            pending_payouts: pending,
            new_users_since: new_users,
            recent_audit_count: audit_count,
        })
    }

    pub async fn admin_export_users(&self, limit: u32) -> AppResult<Vec<AdminExportUserRow>> {
        let rows = self.admin_list_users(limit).await?;
        let users = self.users.read().await;
        Ok(rows
            .into_iter()
            .map(|r| {
                let locale = users
                    .get(&r.user_id)
                    .map(|p| p.locale.clone())
                    .unwrap_or_else(|| "en_US".into());
                AdminExportUserRow {
                    user_id: r.user_id,
                    referral_code: r.referral_code,
                    balance_usdt: r.balance_usdt,
                    trust_score: r.trust_score,
                    banned: r.banned,
                    created_at: r.created_at,
                    total_watches: r.total_watches,
                    referral_count: r.referral_count,
                    locale,
                }
            })
            .collect())
    }

    pub async fn admin_export_audit(&self, limit: u32) -> AppResult<Vec<AdminAuditEntry>> {
        self.admin_audit_log(limit).await
    }

    pub async fn admin_export_payouts(&self, limit: u32) -> AppResult<Vec<PayoutRequestRow>> {
        self.list_payout_requests(PayoutListFilter::All, limit).await
    }

    pub async fn payout_actions_today(&self, day: NaiveDate) -> AppResult<(i64, i64)> {
        let requests = self.payout_requests.read().await;
        let approved = requests
            .iter()
            .filter(|p| {
                p.created_at.date_naive() == day
                    && (p.status == "approved" || p.status == "paid_out")
            })
            .count() as i64;
        let rejected = requests
            .iter()
            .filter(|p| p.created_at.date_naive() == day && p.status == "rejected")
            .count() as i64;
        Ok((approved, rejected))
    }

    pub async fn registrations_on(&self, day: NaiveDate) -> AppResult<i64> {
        let users = self.users.read().await;
        Ok(users
            .values()
            .filter(|p| p.created_at.date_naive() == day)
            .count() as i64)
    }

    pub async fn rewards_on(&self, day: NaiveDate) -> AppResult<Decimal> {
        let ledger = self.wallet.all_ledger().await;
        Ok(ledger
            .iter()
            .filter(|e| e.kind == LedgerKind::Credit && e.created_at.date_naive() == day)
            .map(|e| e.amount_usdt)
            .sum())
    }

    pub async fn active_users_on(&self, day: NaiveDate) -> AppResult<i64> {
        let users = self.users.read().await;
        Ok(users
            .values()
            .filter(|p| p.last_active_date == Some(day))
            .count() as i64)
    }

    pub async fn user_payouts(&self, user_id: Uuid, limit: u32) -> AppResult<Vec<PayoutRequestRow>> {
        let requests = self.payout_requests.read().await;
        let mut rows: Vec<PayoutRequestRow> = requests
            .iter()
            .filter(|p| p.user_id == user_id)
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

    pub async fn user_total_earnings(&self, user_id: Uuid) -> AppResult<Decimal> {
        let ledger = self.ledger(user_id).await?;
        Ok(ledger
            .iter()
            .filter(|e| e.kind == "credit")
            .map(|e| e.amount_usdt)
            .sum())
    }

    pub async fn user_last_activity(&self, user_id: Uuid) -> AppResult<Option<DateTime<Utc>>> {
        let mut latest = None;
        let ledger = self.ledger(user_id).await?;
        for e in ledger {
            latest = Some(latest.map_or(e.created_at, |l: DateTime<Utc>| l.max(e.created_at)));
        }
        if let Ok(profile) = self.profile(user_id).await {
            if let Some(day) = profile.last_active_date {
                let dt = day.and_hms_opt(12, 0, 0).unwrap().and_utc();
                latest = Some(latest.map_or(dt, |l| l.max(dt)));
            }
        }
        Ok(latest)
    }

    pub async fn feature_flag_timestamps(&self) -> AppResult<HashMap<String, DateTime<Utc>>> {
        Ok(self.feature_flag_times.read().await.clone())
    }

    pub async fn latest_feature_flags_audit(&self) -> AppResult<Option<(DateTime<Utc>, serde_json::Value)>> {
        let log = self.audit_log.read().await;
        Ok(log
            .iter()
            .rev()
            .find(|e| e.action == "feature_flags_update")
            .map(|e| (e.created_at, e.details.clone())))
    }

    pub fn gamification(&self) -> &GamificationMemStore {
        &self.gamification
    }

    pub async fn admin_insights(&self) -> AppResult<crate::admin::AdminInsights> {
        let today = Utc::now().date_naive();
        let revenue_7d = self.revenue_in_period_days(7).await?;
        let revenue_30d = self.revenue_in_period_days(30).await?;
        let _user_count = self.user_count().await?;
        let rewards_today = self.rewards_today_usdt(today).await?;
        let videos_today = self.videos_today(today).await?;
        let avg_reward = if videos_today > 0 {
            rewards_today / Decimal::from(videos_today)
        } else {
            Decimal::ZERO
        };
        let paid = self.total_paid_out_usdt().await?;
        let payout_count = self.recent_payout_count(30).await?;
        let avg_payout = if payout_count > 0 {
            paid / Decimal::from(payout_count)
        } else {
            Decimal::ZERO
        };
        let cutoff = today - chrono::Duration::days(7);
        let active_7d = self
            .users
            .read()
            .await
            .values()
            .filter(|p| p.last_active_date >= Some(cutoff))
            .count() as i64;
        Ok(crate::admin::AdminInsights {
            revenue_7d,
            revenue_30d,
            avg_reward_usdt: avg_reward,
            avg_payout_usdt: avg_payout,
            active_users_7d: active_7d,
        })
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}
