use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Internal ledger currency — always USDT.
pub type Usdt = Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    Usdt,
    Eur,
    Usd,
    Gbp,
}

impl Currency {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Usdt => "USDT",
            Self::Eur => "EUR",
            Self::Usd => "USD",
            Self::Gbp => "GBP",
        }
    }
}
