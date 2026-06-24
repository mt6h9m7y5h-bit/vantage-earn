#!/usr/bin/env bash
# Start local PostgreSQL via docker compose (matches .env.example credentials).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "→ Starting PostgreSQL (docker compose)..."
docker compose up -d postgres

echo "→ Waiting for postgres to accept connections..."
for _ in $(seq 1 30); do
  if docker compose exec -T postgres pg_isready -U vantage -d vantage_earn >/dev/null 2>&1; then
    echo "✓ PostgreSQL ready on localhost:5432"
    echo ""
    echo "  export DATABASE_URL=postgres://vantage:vantage@localhost:5432/vantage_earn"
    exit 0
  fi
  sleep 1
done

echo "✗ Postgres did not become healthy in time" >&2
exit 1
