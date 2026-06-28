# VANTAGE-EARN — Statusbericht für ChatGPT

**Stand:** Juni 2025 · **Zielgruppe:** KI-Assistent zur Fortführung der Entwicklung

---

## Projektüberblick

**VANTAGE-EARN** ist eine **Watch-to-Earn Progressive Web App (PWA)** im institutionellen Fintech-Look (Navy/Blau, Inter-Typografie). Nutzer sehen Rewarded-Video-Anzeigen (aktuell Mock-Timer), verdienen USDT-interne Guthaben und können Auszahlungen beantragen. Backend: **Rust-Monorepo** mit **Axum** (`api-gateway`); Frontend: **Vanilla HTML/JS/CSS** ohne Framework.

Architektur: Event-getriebene Engines (Reward, Wallet, Fraud, Trust, Currency, Localization, Liquidity, AI). Ledger in USDT (`rust_decimal`), Anzeige in lokaler Währung. Phasenplan in `ARCHITECTURE.md`.

---

## URLs & Repository

| | |
|---|---|
| **Production** | https://vantage-earn.onrender.com/demo |
| **Health** | https://vantage-earn.onrender.com/health |
| **Hosting** | Render.com (Frankfurt), Blueprint `render.yaml` (Web + Postgres Free Tier) |
| **GitHub** | https://github.com/mt6h9m7y5h-bit/vantage-earn |
| **Workspace-Pfad** | `vantage-earn/` (Monorepo-Root) |

---

## Was fertig ist

### Nutzer-PWA (`frontend/index.html`)
- Vollständiges Dashboard: Wallet, Verdienen (Watch-Flow), Belohnungen, Empfehlungen, Auszahlungen
- Sidebar-Navigation, Dark/Light-Theme, Pull-to-Refresh, PWA-Install-Prompt
- Registrierung ohne Passwort (JWT), optionaler Referral-Code, **AGB/Datenschutz-Checkbox** bei Signup
- Rewarded-Video-Overlay (Mock / AdInPlay / AppLixir via `/config`)
- Streaks, Daily Challenge (5 Videos/Tag), Bonus-Katalog, Leaderboard (wöchentlich)
- AI-Copilot (optional, `OPENAI_API_KEY`)
- Footer-Links: Impressum, Datenschutz, AGB

### Admin (`frontend/admin.html`, `/admin`)
- Stats (Umsatz, Nutzer, Payouts), erweiterte Analytics mit Chart-Daten
- Payout-Queue: approve / reject / hold
- User-Management: Credit, Ban, Audit-Log
- Feature Flags (z. B. `PAYOUT_DEMO_MODE`, Watch-Dauer) per UI patchbar
- Bulk Credit (Preview + Execute mit User-Filter)

### Legal (`frontend/legal/`)
- Impressum, Datenschutz, AGB — **mit Platzhalter-/Beispieldaten** (HRB 999999 B etc.)

### Backend & Daten
- PostgreSQL-Persistenz (Migrationen beim Start) oder In-Memory-Fallback ohne `DATABASE_URL`
- JWT-Auth, Rate-Limiting, Fraud/Trust auf Watch & Payout
- Payout-Tiers (instant / delayed / deep review), Demo-Modus für niedrige Schwellen
- Integrationstests (`cargo test -p api-gateway`), E2E-Skript `./scripts/test-e2e.sh`

---

## Tech-Stack & Schlüsseldateien

| Bereich | Pfad |
|---------|------|
| HTTP-API & Routen | `crates/api-gateway/src/routes.rs` |
| App-State, Feature Flags | `crates/api-gateway/src/state.rs` |
| Postgres Store | `crates/api-gateway/src/store/postgres.rs` |
| Admin-Endpoints | `crates/api-gateway/src/admin.rs` |
| Engines | `crates/reward-engine`, `wallet-engine`, `fraud-engine`, … |
| PWA UI | `frontend/index.html` |
| Admin UI | `frontend/admin.html` |
| Service Worker | `frontend/sw.js` (Cache-Version bump bei UI-Änderungen) |
| Deploy | `render.yaml`, `Dockerfile`, `docs/DEPLOY.md` |

**Stack:** Rust 2021, Axum, SQLx/Postgres, JWT, Vanilla JS PWA, Docker.

---

## Umgebungsvariablen (wichtig)

| Variable | Zweck |
|----------|--------|
| `JWT_SECRET` | Pflicht in Production — Token-Signierung |
| `ADMIN_SECRET` | Header `X-Admin-Secret` für `/admin/*` API |
| `DATABASE_URL` | Postgres-Connection (Render: aus Blueprint) |
| `PAYOUT_DEMO_MODE` | `true` = niedrige Auszahlungsschwelle für Demos |
| `AD_PROVIDER` | `mock` (default), `adinplay`, `applixir` |
| `ADINPLAY_PUBLISHER_ID`, `ADINPLAY_SITE_ID`, `ADINPLAY_TAG_URL` | Live-Ads AdInPlay |
| `APPLIXIR_API_KEY` / `APPLIXIR_APP_ID` | Alternative Ad-Provider |
| `OPENAI_API_KEY`, `OPENAI_MODEL` | Optional: AI-Copilot |
| `PORT` | Server-Port (Render setzt automatisch) |

Vorlage: `.env.example`

---

## Was noch offen ist

1. **Rechtliches:** Echte Firmendaten in Impressum/AGB/Datenschutz (aktuell Beispieltexte)
2. **AdInPlay:** Publisher-Freigabe und echte Tag-URLs eintragen
3. **Custom Domain:** z. B. `app.vantage-earn.de` statt `*.onrender.com`
4. **Referral-Tiers:** Basis-Referral läuft; gestaffelte Prämien/Marketing noch ausstehend
5. **Marketing & Content:** Skripte in `content/WOCHE-1/`, Social-Prompts in `content/prompts/`
6. **Phase 3:** Kafka, K8s, Offerwall, Device-Fingerprinting (laut Architektur-Roadmap)

---

## Lokal starten

```bash
cd vantage-earn
cp .env.example .env
./scripts/dev-with-db.sh          # Postgres + API
# → http://localhost:3000/demo
```

Ohne DB (Smoke-Test, Daten flüchtig):

```bash
unset DATABASE_URL && cargo run -p api-gateway
```

Tests:

```bash
cargo test -p api-gateway
./scripts/test-e2e.sh             # Server muss laufen
```

---

## Deploy (Render)

```bash
git push origin main
```

Render Blueprint (`render.yaml`) baut Docker-Image, verbindet Postgres, setzt `ADMIN_SECRET` (generateValue). Health-Check: `/health`. Erster Request nach Sleep kann ~30 s dauern (Free Tier).

---

## Hinweise für die Fortführung

- UI-Änderungen: `frontend/sw.js` Cache-Version erhöhen (`vantage-earn-vN`)
- Admin-API immer mit `X-Admin-Secret` testen
- Register-Body erfordert `"accept_terms": true`
- Design-Tokens und Screen-Map: `docs/CHATGPT-UI-HANDOFF.md`
- Keine `f64` für Geld — immer `Decimal` / USDT-Ledger
