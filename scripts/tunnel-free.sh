#!/usr/bin/env bash
# Kostenloser HTTPS-Tunnel — App auf jedem Handy, ohne Cloud-Deploy.
# Voraussetzung: Server läuft in Tab 1 (cargo run -p api-gateway)

set -euo pipefail

if ! command -v cloudflared &>/dev/null; then
  if [ -x /tmp/cloudflared ]; then
    CLOUDFLARED=/tmp/cloudflared
  else
    echo "Installiere cloudflared..."
    ARCH=$(uname -m)
    case "$ARCH" in
      arm64) CF_ARCH=arm64 ;;
      *) CF_ARCH=amd64 ;;
    esac
    curl -fsSL "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-darwin-${CF_ARCH}.tgz" -o /tmp/cloudflared.tgz
    tar -xzf /tmp/cloudflared.tgz -C /tmp
    CLOUDFLARED=/tmp/cloudflared
  fi
else
  CLOUDFLARED=cloudflared
fi

PORT="${PORT:-3000}"

if ! curl -s -m 2 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
  echo "Fehler: Server läuft nicht auf Port ${PORT}."
  echo "Starte zuerst in Tab 1: cargo run -p api-gateway"
  exit 1
fi

echo ""
echo "Tunnel startet — öffentliche HTTPS-URL kommt gleich..."
echo "(Mac muss an bleiben. Ctrl+C zum Stoppen.)"
echo ""

$CLOUDFLARED tunnel --url "http://127.0.0.1:${PORT}"
