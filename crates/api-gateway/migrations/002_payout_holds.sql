ALTER TABLE users ADD COLUMN IF NOT EXISTS last_active_date DATE;

ALTER TABLE platform_stats
    ADD COLUMN IF NOT EXISTS held_payouts NUMERIC(28, 18) NOT NULL DEFAULT 0;

ALTER TABLE wallets
    ADD CONSTRAINT wallets_balance_non_negative CHECK (balance_usdt >= 0);

CREATE TABLE IF NOT EXISTS payout_requests (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    amount_usdt NUMERIC(28, 18) NOT NULL,
    tier TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_payout_requests_user ON payout_requests(user_id);
