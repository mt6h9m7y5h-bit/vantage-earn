mod ledger;
mod memory;
mod postgres;

use rust_decimal::Decimal;
use shared::AppResult;
use uuid::Uuid;

pub use ledger::LedgerItem;
pub use memory::MemoryStore;
pub use postgres::PgStore;

use crate::state::UserProfile;

#[derive(Clone)]
pub enum Store {
    Memory(MemoryStore),
    Postgres(PgStore),
}

impl Store {
    pub fn memory() -> Self {
        Self::Memory(MemoryStore::new())
    }

    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let store = PgStore::connect(database_url).await?;
        Ok(Self::Postgres(store))
    }

    pub async fn ping(&self) -> AppResult<bool> {
        match self {
            Self::Memory(s) => s.ping().await,
            Self::Postgres(s) => s.ping().await,
        }
    }

    pub async fn ensure_user(&self, user_id: Uuid) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.ensure_user(user_id).await,
            Self::Postgres(s) => s.ensure_user(user_id).await,
        }
    }

    pub async fn user_exists(&self, user_id: Uuid) -> AppResult<bool> {
        match self {
            Self::Memory(s) => s.user_exists(user_id).await,
            Self::Postgres(s) => s.user_exists(user_id).await,
        }
    }

    pub async fn profile(&self, user_id: Uuid) -> AppResult<UserProfile> {
        match self {
            Self::Memory(s) => s.profile(user_id).await,
            Self::Postgres(s) => s.profile(user_id).await,
        }
    }

    pub async fn save_profile(&self, user_id: Uuid, profile: &UserProfile) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.save_profile(user_id, profile).await,
            Self::Postgres(s) => s.save_profile(user_id, profile).await,
        }
    }

    pub async fn balance(&self, user_id: Uuid) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.balance(user_id).await,
            Self::Postgres(s) => s.balance(user_id).await,
        }
    }

    pub async fn credit(&self, user_id: Uuid, amount: Decimal) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.credit(user_id, amount).await,
            Self::Postgres(s) => s.credit(user_id, amount).await,
        }
    }

    pub async fn debit(&self, user_id: Uuid, amount: Decimal) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.debit(user_id, amount).await,
            Self::Postgres(s) => s.debit(user_id, amount).await,
        }
    }

    pub async fn trust_score(&self, user_id: Uuid) -> AppResult<i32> {
        match self {
            Self::Memory(s) => s.trust_score(user_id).await,
            Self::Postgres(s) => s.trust_score(user_id).await,
        }
    }

    pub async fn set_trust_score(&self, user_id: Uuid, score: i32) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.set_trust_score(user_id, score).await,
            Self::Postgres(s) => s.set_trust_score(user_id, score).await,
        }
    }

    pub async fn total_revenue(&self) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.total_revenue().await,
            Self::Postgres(s) => s.total_revenue().await,
        }
    }

    pub async fn add_revenue(&self, amount: Decimal) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.add_revenue(amount).await,
            Self::Postgres(s) => s.add_revenue(amount).await,
        }
    }

    pub async fn pending_payouts(&self) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.pending_payouts().await,
            Self::Postgres(s) => s.pending_payouts().await,
        }
    }

    pub async fn held_payouts(&self) -> AppResult<Decimal> {
        match self {
            Self::Memory(s) => s.held_payouts().await,
            Self::Postgres(s) => s.held_payouts().await,
        }
    }

    pub async fn add_pending_payout(&self, amount: Decimal) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.add_pending_payout(amount).await,
            Self::Postgres(s) => s.add_pending_payout(amount).await,
        }
    }

    pub async fn add_held_payout(&self, amount: Decimal) -> AppResult<()> {
        match self {
            Self::Memory(s) => s.add_held_payout(amount).await,
            Self::Postgres(s) => s.add_held_payout(amount).await,
        }
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
        match self {
            Self::Memory(s) => {
                s.record_payout_request(id, user_id, amount, tier, status, payout_method)
                    .await
            }
            Self::Postgres(s) => {
                s.record_payout_request(id, user_id, amount, tier, status, payout_method)
                    .await
            }
        }
    }

    pub async fn find_user_by_referral_code(&self, code: &str) -> AppResult<Option<Uuid>> {
        match self {
            Self::Memory(s) => s.find_user_by_referral_code(code).await,
            Self::Postgres(s) => s.find_user_by_referral_code(code).await,
        }
    }

    pub async fn ledger(&self, user_id: Uuid) -> AppResult<Vec<LedgerItem>> {
        match self {
            Self::Memory(s) => s.ledger(user_id).await,
            Self::Postgres(s) => s.ledger(user_id).await,
        }
    }
}
