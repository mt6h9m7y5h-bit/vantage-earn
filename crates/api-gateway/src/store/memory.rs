use std::collections::HashMap;
use std::sync::Arc;

use referral_engine::ReferralEngine;
use rust_decimal::Decimal;
use shared::AppResult;
use tokio::sync::RwLock;
use uuid::Uuid;
use wallet_engine::{LedgerKind, WalletEngine};

use crate::state::UserProfile;
use crate::store::LedgerItem;

#[derive(Clone)]
pub struct MemoryStore {
    wallet: Arc<WalletEngine>,
    users: Arc<RwLock<HashMap<Uuid, UserProfile>>>,
    trust_scores: Arc<RwLock<HashMap<Uuid, i32>>>,
    total_revenue: Arc<RwLock<Decimal>>,
    pending_payouts: Arc<RwLock<Decimal>>,
    held_payouts: Arc<RwLock<Decimal>>,
    payout_requests: Arc<RwLock<Vec<PayoutRecord>>>,
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
        self.wallet.balance(user_id).await
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
        });
        Ok(())
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
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}
