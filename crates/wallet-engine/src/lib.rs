use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use shared::{AppError, AppResult};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct LedgerEntry {
    pub id: Uuid,
    pub user_id: Uuid,
    pub amount_usdt: Decimal,
    pub balance_after: Decimal,
    pub kind: LedgerKind,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerKind {
    Credit,
    Debit,
}

#[derive(Debug, Clone)]
pub struct Wallet {
    pub user_id: Uuid,
    pub balance_usdt: Decimal,
    pub created_at: DateTime<Utc>,
}

pub struct WalletEngine {
    wallets: RwLock<HashMap<Uuid, Wallet>>,
    ledger: RwLock<Vec<LedgerEntry>>,
}

impl WalletEngine {
    pub fn new() -> Self {
        Self {
            wallets: RwLock::new(HashMap::new()),
            ledger: RwLock::new(Vec::new()),
        }
    }

    pub async fn get_or_create(&self, user_id: Uuid) -> Wallet {
        let mut wallets = self.wallets.write().await;
        wallets
            .entry(user_id)
            .or_insert_with(|| Wallet {
                user_id,
                balance_usdt: Decimal::ZERO,
                created_at: Utc::now(),
            })
            .clone()
    }

    pub async fn balance(&self, user_id: Uuid) -> AppResult<Decimal> {
        let wallets = self.wallets.read().await;
        wallets
            .get(&user_id)
            .map(|w| w.balance_usdt)
            .ok_or(AppError::UserNotFound(user_id))
    }

    pub async fn credit(&self, user_id: Uuid, amount: Decimal) -> AppResult<LedgerEntry> {
        if amount <= Decimal::ZERO {
            return Err(AppError::InvalidInput("credit must be positive".into()));
        }
        self.apply(user_id, amount, LedgerKind::Credit).await
    }

    pub async fn debit(&self, user_id: Uuid, amount: Decimal) -> AppResult<LedgerEntry> {
        if amount <= Decimal::ZERO {
            return Err(AppError::InvalidInput("debit must be positive".into()));
        }
        let balance = self.balance(user_id).await?;
        if balance < amount {
            return Err(AppError::InsufficientBalance {
                have: balance,
                need: amount,
            });
        }
        self.apply(user_id, -amount, LedgerKind::Debit).await
    }

    async fn apply(
        &self,
        user_id: Uuid,
        signed_amount: Decimal,
        kind: LedgerKind,
    ) -> AppResult<LedgerEntry> {
        let mut wallets = self.wallets.write().await;
        let wallet = wallets
            .entry(user_id)
            .or_insert_with(|| Wallet {
                user_id,
                balance_usdt: Decimal::ZERO,
                created_at: Utc::now(),
            });

        wallet.balance_usdt += signed_amount;
        let entry = LedgerEntry {
            id: Uuid::new_v4(),
            user_id,
            amount_usdt: signed_amount.abs(),
            balance_after: wallet.balance_usdt,
            kind,
            created_at: Utc::now(),
        };

        self.ledger.write().await.push(entry.clone());
        Ok(entry)
    }

    pub async fn ledger_for_user(&self, user_id: Uuid) -> Vec<LedgerEntry> {
        self.ledger
            .read()
            .await
            .iter()
            .filter(|e| e.user_id == user_id)
            .cloned()
            .collect()
    }
}

impl Default for WalletEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn credit_and_debit() {
        let engine = WalletEngine::new();
        let user = Uuid::new_v4();
        engine
            .credit(user, Decimal::from_str_exact("1.5").unwrap())
            .await
            .unwrap();
        assert_eq!(
            engine.balance(user).await.unwrap(),
            Decimal::from_str_exact("1.5").unwrap()
        );
        engine
            .debit(user, Decimal::from_str_exact("0.5").unwrap())
            .await
            .unwrap();
        assert_eq!(
            engine.balance(user).await.unwrap(),
            Decimal::from_str_exact("1.0").unwrap()
        );
    }
}
