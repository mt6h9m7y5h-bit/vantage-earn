#!/usr/bin/env bash
# Start Postgres, apply migrations, run api-gateway with DATABASE_URL.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

"$ROOT/scripts/db-up.sh"
export DATABASE_URL="${DATABASE_URL:-postgres://vantage:vantage@localhost:5432/vantage_earn}"
export JWT_SECRET="${JWT_SECRET:-change-me-in-production}"

echo "→ Applying migrations..."
"$ROOT/scripts/db-migrate.sh"

echo "→ Starting api-gateway with PostgreSQL..."
exec cargo run -p api-gateway --bin vantage-earn
