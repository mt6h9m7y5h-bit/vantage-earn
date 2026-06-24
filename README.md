# VANTAGE-EARN

AI-powered Watch-to-Earn platform — Rust monorepo (Phase 1 MVP).

## Quick Start

```bash
cd vantage-earn
cp .env.example .env   # optional
cargo run -p api-gateway
```

Server: `http://localhost:3000`

**Web-App (PWA):** [http://localhost:3000/demo](http://localhost:3000/demo) — im Browser nutzen oder **zum Home-Bildschirm hinzufügen** (kein App Store nötig).

| Plattform | Installation |
|-----------|--------------|
| **Android (Chrome)** | „Zum Home-Bildschirm hinzufügen“ Banner oder Menü → App installieren |
| **iPhone (Safari)** | Teilen → Zum Home-Bildschirm |
| **Desktop (Chrome)** | Adressleiste → Installieren |

Für Production ist **HTTPS** erforderlich — dann funktioniert die App auf **jedem Handy weltweit**.

**Kostenlos jetzt:** [docs/FREE_HOSTING.md](./docs/FREE_HOSTING.md) (Cloudflare Tunnel, 0 €)  
**Später 24/7:** [docs/DEPLOY.md](./docs/DEPLOY.md) (Fly.io / Render)

### With PostgreSQL

```bash
docker compose up -d postgres
export DATABASE_URL=postgres://vantage:vantage@localhost:5432/vantage_earn
cargo run -p api-gateway
```

Without `DATABASE_URL`, the API uses an in-memory store (fine for local dev).

### AI Copilot (optional)

```bash
export OPENAI_API_KEY=sk-...
export OPENAI_MODEL=gpt-4o-mini   # optional
cargo run -p api-gateway
```

See [docs/AI_COPILOT.md](./docs/AI_COPILOT.md) for architecture details.

## API Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/health` | — | Health check |
| POST | `/auth/register` | — | Create account (`referral_code` optional), returns JWT |
| POST | `/auth/login` | — | Login with `user_id`, returns JWT |
| GET | `/users/me/wallet` | JWT | Balance + trust score |
| GET | `/users/me/ledger` | JWT | Transaction history (last 50) |
| GET | `/users/me/referral` | JWT | Your referral code + count |
| POST | `/users/me/watch/complete` | JWT | Complete watch session, earn USDT |
| POST | `/users/me/payout/request` | JWT | Request payout |
| GET | `/users/me/ai/context` | JWT | Safe AI context + prompt preview |
| POST | `/users/me/ai/chat` | JWT | AI copilot |

Protected routes are rate-limited per IP (default: 60 req/min). Auth routes: 20 req/min. Configure via `RATE_LIMIT_MAX`, `AUTH_RATE_LIMIT_MAX`, and `RATE_LIMIT_WINDOW_SECS`.

## Economics

Watch sessions generate **ad revenue** for the platform (40% share to user, 60% platform). User rewards never inflate the payout pool directly — only gross ad revenue does.

## Example

```bash
# Register
AUTH=$(curl -s -X POST http://localhost:3000/auth/register \
  -H "Content-Type: application/json" \
  -d '{"locale":"de_DE"}')
TOKEN=$(echo "$AUTH" | jq -r .token)

# Complete a 60s watch
curl -s -X POST http://localhost:3000/users/me/watch/complete \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"watch_duration_secs": 60}' | jq

# Check wallet
curl -s http://localhost:3000/users/me/wallet \
  -H "Authorization: Bearer $TOKEN" | jq
```

## Crates

- `shared` — types, events, money (Decimal)
- `event-bus` — in-process pub/sub
- `reward-engine`, `wallet-engine`, `fraud-engine`, `trust-score-engine`
- `currency-engine`, `localization-engine`, `ai-engine`, `liquidity-engine`
- `api-gateway` — HTTP server

See [ARCHITECTURE.md](./ARCHITECTURE.md) for phased rollout plan.

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```
