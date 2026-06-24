CREATE TABLE users (
    id UUID PRIMARY KEY,
    locale TEXT NOT NULL DEFAULT 'en_US',
    streak_days INT NOT NULL DEFAULT 0,
    referral_count INT NOT NULL DEFAULT 0,
    account_age_days INT NOT NULL DEFAULT 1,
    payout_history INT NOT NULL DEFAULT 0,
    sessions_last_hour INT NOT NULL DEFAULT 0,
    sessions_window_started TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE wallets (
    user_id UUID PRIMARY KEY REFERENCES users(id),
    balance_usdt NUMERIC(28, 18) NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE ledger_entries (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    amount_usdt NUMERIC(28, 18) NOT NULL,
    balance_after NUMERIC(28, 18) NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('credit', 'debit')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ledger_user ON ledger_entries(user_id);

CREATE TABLE trust_scores (
    user_id UUID PRIMARY KEY REFERENCES users(id),
    score INT NOT NULL DEFAULT 50,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE platform_stats (
    id INT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    total_revenue NUMERIC(28, 18) NOT NULL DEFAULT 0,
    pending_payouts NUMERIC(28, 18) NOT NULL DEFAULT 0
);

INSERT INTO platform_stats (id) VALUES (1);
