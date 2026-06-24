use rust_decimal::Decimal;
use shared::AppError;

pub struct LiquidityEngine;

impl LiquidityEngine {
    pub fn calculate_safe_payout_pool(
        total_revenue: Decimal,
        pending_payouts: Decimal,
        reserve_ratio: Decimal,
    ) -> Decimal {
        let reserve = total_revenue * reserve_ratio;
        total_revenue - pending_payouts - reserve
    }

    pub fn can_payout(
        total_revenue: Decimal,
        pending_payouts: Decimal,
        reserve_ratio: Decimal,
        requested: Decimal,
    ) -> Result<(), AppError> {
        let pool = Self::calculate_safe_payout_pool(total_revenue, pending_payouts, reserve_ratio);
        if requested > pool {
            return Err(AppError::InsufficientLiquidity {
                max_available: pool.max(Decimal::ZERO),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserves_funds() {
        let pool = LiquidityEngine::calculate_safe_payout_pool(
            Decimal::from(1000),
            Decimal::from(200),
            Decimal::new(10, 2), // 10%
        );
        assert_eq!(pool, Decimal::from(700));
    }
}
