#!/usr/bin/env bash
# End-to-end smoke test — run while server is up on PORT (default 3000)
set -euo pipefail

BASE="${1:-http://127.0.0.1:${PORT:-3000}}"

fail() { echo "FAIL: $1"; exit 1; }
ok() { echo "OK: $1"; }

echo "Testing $BASE"

H=$(curl -sf -m 5 "$BASE/health") || fail "health unreachable"
echo "$H" | grep -q '"status":"ok"' || fail "health status"
ok "health"

CFG=$(curl -sf -m 5 "$BASE/config") || fail "config"
echo "$CFG" | grep -q '"ad_provider"' || fail "config ad_provider"
ok "config"

curl -sf -m 5 -o /dev/null "$BASE/demo" || fail "demo page"
ok "demo"

curl -sf -m 5 -o /dev/null "$BASE/manifest.webmanifest" || fail "manifest"
ok "manifest"

curl -sf -m 5 -o /dev/null "$BASE/icons/icon-192.png" || fail "icon"
ok "icons"

AUTH=$(curl -sf -m 5 -X POST "$BASE/auth/register" \
  -H "Content-Type: application/json" \
  -d '{"locale":"de_DE"}') || fail "register"
TOKEN=$(echo "$AUTH" | jq -r .token)
[ "$TOKEN" != "null" ] && [ -n "$TOKEN" ] || fail "token missing"
ok "register"

W=$(curl -sf -m 5 -X POST "$BASE/users/me/watch/complete" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"watch_duration_secs":60}') || fail "watch"
echo "$W" | grep -q 'USDT' || fail "reward message"
ok "watch + reward"

B=$(curl -sf -m 5 "$BASE/users/me/wallet" -H "Authorization: Bearer $TOKEN") || fail "wallet"
echo "$B" | grep -q 'balance_usdt' || fail "balance"
ok "wallet balance"

S=$(curl -sf -m 5 "$BASE/users/me/stats" -H "Authorization: Bearer $TOKEN") || fail "stats"
echo "$S" | grep -q '"streak_days"' || fail "stats streak"
echo "$S" | grep -q '"reward_estimate_30s"' || fail "stats reward"
echo "$S" | grep -q '"bonus_catalog"' || fail "stats bonus_catalog"
echo "$S" | grep -q '"streak_bonus_percent"' || fail "stats streak_bonus_percent"
ok "stats"

L=$(curl -sf -m 5 "$BASE/users/me/ledger" -H "Authorization: Bearer $TOKEN") || fail "ledger"
echo "$L" | grep -q 'credit' || fail "ledger entry"
ok "ledger"

echo ""
echo "All E2E checks passed for $BASE"
