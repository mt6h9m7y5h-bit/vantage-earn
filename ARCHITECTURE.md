# VANTAGE-EARN — Optimized Architecture

## Vision

Scalable, fraud-resistant Watch-to-Earn platform with AI copilot, instant UX feedback, and USDT-internal ledger.

## Key Optimizations (vs. v1 spec)

| Area | Before | After |
|------|--------|-------|
| Money | `f64` | `rust_decimal::Decimal` — no float drift |
| Services | 25 microservices day 1 | Phased: MVP monolith → split by load |
| AI crates | 6 separate crates | `ai-engine` with firewall/validator/copilot modules |
| Event bus | Kafka required | In-process bus (MVP) → Redis/Kafka later |
| API | Undefined | Single `api-gateway` (Axum), REST first |
| Auth | Separate service | JWT in gateway (MVP), extract later |

## Phased Rollout

### Phase 1 — MVP (current)
- `shared`, `event-bus`, `reward-engine`, `wallet-engine`
- `trust-score-engine`, `fraud-engine`, `currency-engine`, `localization-engine`
- `ai-engine`, `liquidity-engine`, `api-gateway`
- In-memory ledger, event-driven reward flow, health + watch endpoints

### Phase 2 — Growth
- PostgreSQL ledger, Redis cache, payout queue
- `referral-engine`, `presence-engine`, `night-engine`
- Flutter app + i18n (en, de, fr, es)

### Phase 3 — Enterprise
- Kafka event bus, K8s, admin dashboard
- `offerwall-engine`, `analytics-engine`, `prediction-engine`
- Device fingerprinting, VPN/emulator detection

## Core Principles

- **Ledger = USDT only** — display uses cached FX, never for settlement math
- **AI isolation** — copilot receives `SafeAIContext` only, no DB/wallet access
- **Event-driven** — user action → event bus → engines → wallet update
- **Fraud first** — trust score gates payout tiers

## Payout Tiers

| Amount (EUR equiv.) | Flow |
|---------------------|------|
| 0 – 20 € | Instant |
| 20 – 80 € | Delayed validation |
| 80 – 170 € | Deep fraud review |

## Project Structure

```
vantage-earn/
├── crates/
│   ├── shared/              # Types, events, errors
│   ├── event-bus/           # Pub/sub (in-memory → Kafka)
│   ├── reward-engine/
│   ├── wallet-engine/
│   ├── trust-score-engine/
│   ├── fraud-engine/
│   ├── currency-engine/
│   ├── localization-engine/
│   ├── ai-engine/           # firewall, validator, copilot prompt
│   ├── liquidity-engine/
│   └── api-gateway/         # HTTP entry point
├── frontend/                # Phase 2
└── infrastructure/          # Phase 3
```

## Event Flow

```
POST /users/me/watch/complete
        ↓
   FraudEngine (quick check)
        ↓
   RewardEngine → WalletEngine (credit USDT)
        ↓
   TrustScoreEngine (update)
        ↓
   EventBus::publish(WatchCompleted)
        ↓
   Analytics (Phase 2)
```

## Security Layers (progressive)

1. JWT + rate limiting (MVP)
2. AI firewall + response validator
3. Cloudflare WAF, device fingerprint, audit logs (Phase 3)
