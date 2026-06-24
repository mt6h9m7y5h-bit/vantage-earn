#!/usr/bin/env bash
# Manual smoke test: Postgres + api-gateway + E2E checks (run from repo root).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export DATABASE_URL="${DATABASE_URL:-postgres://vantage:vantage@localhost:5432/vantage_earn}"
export JWT_SECRET="${JWT_SECRET:-test-jwt-secret}"
export PORT="${PORT:-3000}"

"$ROOT/scripts/db-up.sh"
"$ROOT/scripts/db-migrate.sh"

echo "→ Starting api-gateway on port $PORT..."
cargo run -q -p api-gateway &
API_PID=$!
trap 'kill $API_PID 2>/dev/null || true' EXIT

for _ in $(seq 1 60); do
  if curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

HEALTH=$(curl -sf "http://127.0.0.1:$PORT/health")
echo "$HEALTH" | grep -q '"database":true' || {
  echo "FAIL: expected database:true in /health" >&2
  echo "$HEALTH" >&2
  exit 1
}
echo "✓ /health reports database:true"

"$ROOT/scripts/test-e2e.sh" "http://127.0.0.1:$PORT"
echo "✓ Postgres smoke test passed"
