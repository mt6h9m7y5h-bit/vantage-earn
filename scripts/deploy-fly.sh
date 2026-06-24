#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

export PATH="${HOME}/.fly/bin:${PATH}"

if ! command -v flyctl &>/dev/null; then
  echo "Installiere flyctl..."
  curl -L https://fly.io/install.sh | sh
  export PATH="${HOME}/.fly/bin:${PATH}"
fi

if ! flyctl auth whoami &>/dev/null; then
  echo "Bitte bei Fly.io einloggen (Browser öffnet sich):"
  flyctl auth login
fi

APP_NAME="${FLY_APP_NAME:-vantage-earn-$(whoami | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9')}"

echo "App-Name: $APP_NAME"

if ! flyctl apps list 2>/dev/null | grep -q "$APP_NAME"; then
  flyctl launch --yes --copy-config --name "$APP_NAME" --region fra --no-deploy
fi

if [ -z "${JWT_SECRET:-}" ]; then
  JWT_SECRET=$(openssl rand -hex 32)
  echo "JWT_SECRET generiert (wird als Fly Secret gesetzt)"
fi

flyctl secrets set JWT_SECRET="$JWT_SECRET" --app "$APP_NAME"

echo "Deploy startet (dauert ~5–10 Min beim ersten Mal)..."
flyctl deploy --app "$APP_NAME"

echo ""
echo "Fertig! App-URL:"
echo "  https://${APP_NAME}.fly.dev/demo"
echo ""
flyctl open /demo --app "$APP_NAME"
