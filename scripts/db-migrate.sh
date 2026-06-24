#!/usr/bin/env bash
# Apply SQL migrations to DATABASE_URL (same files as api-gateway auto-migrate on connect).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export DATABASE_URL="${DATABASE_URL:-postgres://vantage:vantage@localhost:5432/vantage_earn}"

if command -v sqlx >/dev/null 2>&1; then
  echo "→ Applying migrations via sqlx-cli..."
  sqlx migrate run --source crates/api-gateway/migrations
  echo "✓ Migrations applied"
  exit 0
fi

echo "sqlx-cli not found — applying migrations by connecting (same as api-gateway startup)..."
cargo run -q -p api-gateway --bin db-migrate
echo "✓ Migrations applied"
