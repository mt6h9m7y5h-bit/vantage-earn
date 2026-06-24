ALTER TABLE payout_requests
    ADD COLUMN IF NOT EXISTS payout_method TEXT;

UPDATE payout_requests
SET payout_method = 'crypto'
WHERE payout_method IS NULL;
