use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use shared::Currency;
use tokio::sync::RwLock;

/// Cached FX rates — refreshed on schedule to prevent arbitrage.
pub struct CurrencyEngine {
    rates: RwLock<HashMap<Currency, FxRate>>,
}

#[derive(Debug, Clone)]
pub struct FxRate {
    pub currency: Currency,
    pub usdt_to_local: Decimal,
    pub updated_at: DateTime<Utc>,
}

impl CurrencyEngine {
    pub fn new() -> Self {
        let mut rates = HashMap::new();
        rates.insert(
            Currency::Eur,
            FxRate {
                currency: Currency::Eur,
                usdt_to_local: Decimal::from_str_exact("0.92").unwrap(),
                updated_at: Utc::now(),
            },
        );
        rates.insert(
            Currency::Usd,
            FxRate {
                currency: Currency::Usd,
                usdt_to_local: Decimal::ONE,
                updated_at: Utc::now(),
            },
        );
        rates.insert(
            Currency::Gbp,
            FxRate {
                currency: Currency::Gbp,
                usdt_to_local: Decimal::from_str_exact("0.79").unwrap(),
                updated_at: Utc::now(),
            },
        );
        Self {
            rates: RwLock::new(rates),
        }
    }

    pub fn convert_usdt_to_local(amount_usdt: Decimal, exchange_rate: Decimal) -> Decimal {
        (amount_usdt * exchange_rate).round_dp(2)
    }

    pub async fn usdt_to_local(&self, amount_usdt: Decimal, currency: Currency) -> Option<Decimal> {
        let rates = self.rates.read().await;
        rates
            .get(&currency)
            .map(|r| Self::convert_usdt_to_local(amount_usdt, r.usdt_to_local))
    }

    pub async fn eur_equivalent(&self, amount_usdt: Decimal) -> Decimal {
        self.usdt_to_local(amount_usdt, Currency::Eur)
            .await
            .unwrap_or(amount_usdt)
    }

    pub async fn set_rate(&self, currency: Currency, usdt_to_local: Decimal) {
        self.rates.write().await.insert(
            currency,
            FxRate {
                currency,
                usdt_to_local,
                updated_at: Utc::now(),
            },
        );
    }
}

impl Default for CurrencyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_usdt_to_eur() {
        let result =
            CurrencyEngine::convert_usdt_to_local(Decimal::ONE, Decimal::from_str_exact("0.92").unwrap());
        assert_eq!(result, Decimal::from_str_exact("0.92").unwrap());
    }
}
