#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

PRIMARY_REGION="${FLY_PRIMARY_REGION:-fra}"

# Leere FLY_REGION führt zu: "region  not found"
if [ -n "${FLY_REGION+x}" ] && [ -z "$FLY_REGION" ]; then
  unset FLY_REGION
fi

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
echo "Vollständige Anleitung: docs/FLY.md"
echo ""

if ! flyctl apps list 2>/dev/null | grep -q "$APP_NAME"; then
  flyctl launch --yes --copy-config --name "$APP_NAME" --region "$PRIMARY_REGION" --no-deploy
fi

FLY_URL="https://${APP_NAME}.fly.dev"

if [ -z "${JWT_SECRET:-}" ]; then
  JWT_SECRET=$(openssl rand -hex 32)
  echo "JWT_SECRET generiert"
fi
if [ -z "${ADMIN_SECRET:-}" ]; then
  ADMIN_SECRET=$(openssl rand -hex 32)
  echo "ADMIN_SECRET generiert"
fi

flyctl secrets set \
  JWT_SECRET="$JWT_SECRET" \
  ADMIN_SECRET="$ADMIN_SECRET" \
  APP_URL="$FLY_URL" \
  BITLABS_CALLBACK_BASE_URL="$FLY_URL" \
  --app "$APP_NAME"

echo ""
echo "Deploy startet (Region: $PRIMARY_REGION, dauert ~5–10 Min beim ersten Mal)..."
flyctl deploy --app "$APP_NAME" --primary-region "$PRIMARY_REGION"

echo ""
echo "Fertig!"
echo "  Demo:  ${FLY_URL}/demo"
echo "  Health: ${FLY_URL}/health"
echo ""
echo "Optional — Postgres (persistent):"
echo "  fly postgres create --name ${APP_NAME}-db --region $PRIMARY_REGION"
echo "  fly postgres attach ${APP_NAME}-db --app $APP_NAME"
echo "  fly deploy --app $APP_NAME"
echo ""
echo "Optional — E-Mail (Resend):"
echo "  fly secrets set SMTP_PASS=re_... SMTP_FROM='VANTAGE-EARN <noreply@domain.de>' --app $APP_NAME"
echo ""

flyctl open /demo --app "$APP_NAME"
