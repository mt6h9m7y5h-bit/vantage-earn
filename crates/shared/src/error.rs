use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("user not found: {0}")]
    UserNotFound(uuid::Uuid),

    #[error("insufficient balance: have {have}, need {need}")]
    InsufficientBalance {
        have: rust_decimal::Decimal,
        need: rust_decimal::Decimal,
    },

    #[error("fraud blocked: {0}")]
    FraudBlocked(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("payout pool insufficient: max {max_available} USDT available (10% platform reserve)")]
    InsufficientLiquidity {
        max_available: rust_decimal::Decimal,
    },

    #[error("unauthorized")]
    Unauthorized,

    #[error("rate limit exceeded")]
    RateLimited,

    #[error("ai blocked: {0}")]
    AiBlocked(String),

    #[error("ai unavailable: {0}")]
    AiUnavailable(String),
}

pub type AppResult<T> = Result<T, AppError>;
