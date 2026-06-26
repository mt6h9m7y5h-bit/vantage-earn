#!/usr/bin/env bash
# Stoppt den laufenden Dev-Server auf PORT (default 3000) und startet neu.
set -euo pipefail
cd "$(dirname "$0")/.."
PORT="${PORT:-3000}"
PID="$(lsof -t -iTCP:3000 -sTCP:LISTEN 2>/dev/null || true)"
if [[ -n "${PID}" ]]; then
  echo "Stoppe Prozess auf Port ${PORT} (PID ${PID})…"
  kill "${PID}"
  sleep 1
fi
if [[ -f .env ]]; then
  if ! grep -qE '^[[:space:]]*DATABASE_URL=' .env; then
    unset DATABASE_URL
  fi
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
  echo "Umgebung aus .env geladen."
fi
echo "Starte vantage-earn auf Port ${PORT}…"
exec cargo run -p api-gateway
