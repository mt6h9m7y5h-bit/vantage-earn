-- Composite indexes for user-scoped, time-ordered list queries
CREATE INDEX IF NOT EXISTS idx_ledger_user_created ON ledger_entries(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_payout_requests_user_created ON payout_requests(user_id, created_at DESC);
