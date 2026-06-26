# VANTAGE-EARN — UI Handoff for ChatGPT Redesign

**Date:** 2025-06-25  
**Screenshot:** [`docs/ui-screenshot-current.png`](./ui-screenshot-current.png) (mobile viewport 390×844, logged-in dashboard)  
**Live demo:** https://vantage-earn.onrender.com/demo  
**Local demo:** http://localhost:3000/demo  

---

## Current UI structure (`frontend/index.html`)

Single-page PWA, **mobile-first**, max-width **480px** centered column. Vanilla JS (no React). German (`lang="de"`).

### Layout (top → bottom)

| # | Section | Key IDs / classes | Notes |
|---|---------|-------------------|-------|
| — | Sticky header | `.app-header`, `#header-trust`, `#refresh-btn`, `#theme-toggle`, `#logout-btn` | Brand + trust badge + actions |
| — | Toasts | `#error-banner`, `#success-banner` | Floating alerts |
| — | PWA install | `#install-card`, `#install-btn` | Hidden until `beforeinstallprompt` |
| — | Auth (pre-login) | `#auth-card`, `#referral`, `#register-btn` | One-tap register, optional referral |
| 1 | Hero wallet | `#app` → `.hero-wallet`, `#balance`, `#local-balance`, `#watch-btn` | Primary balance + Auszahlen / Video ansehen |
| 2 | Statistics grid | `#watches-today`, `#total-watches`, `#streak`, `#trust` | 2×2 stat tiles |
| 3 | CTA + daily progress | `.cta-card`, `#cta-duration`, `#cta-reward`, `#challenge-progress-*` | Next video + 5/day challenge |
| 4 | Activity feed | `#ledger` | Transaction list from `/users/me/ledger` |
| 5 | Leaderboard | `#leaderboard-card`, `#leaderboard-list` | Weekly top earners (public API) |
| 6 | Referral | `#referral-banner`, `#ref-code` | Copy + share referral link |
| 7 | Bonuses (collapsible) | `#bonuses-card`, `#streak-bonus-*`, `#bonus-list` | Streak %, milestones, catalog |
| 8 | Payout | `#payout-section`, `#payout-method`, `#payout-btn` | Progress to min threshold, method select |
| — | Ad overlay | `#ad-overlay`, `#ad-timer` | Full-screen rewarded video flow |
| — | Footer | `.site-footer` | Systemstatus link → `/health` |

**Admin UI** is separate: `frontend/admin.html` at `/admin` (not in main dashboard).

---

## Design tokens (`:root` / `[data-theme="dark"]`)

| Token | Value | Usage |
|-------|-------|-------|
| `--bg` | `#0A0F1A` | Page background (navy) |
| `--surface` | `#111827` | Cards |
| `--surface-elevated` | `#1a2332` | Raised surfaces |
| `--primary` | `#3B82F6` | Brand accent (blue) |
| `--success` | `#10B981` | Earnings / positive |
| `--text` | `#FFFFFF` | Primary text |
| `--muted` | `#94A3B8` | Secondary text |
| `--danger` | `#EF4444` | Errors / cancel |
| `--warning` | `#F59E0B` | Warnings |
| `--border` | `rgba(255,255,255,0.08)` | Dividers |
| `--radius` | `12px` | Card corners |
| `--header-h` | `56px` | Sticky header |
| `theme-color` meta | `#0A0F1A` | PWA chrome (replaces old `#00d4aa` teal) |

**Typography:** Inter (400–700). Light theme via `[data-theme="light"]` + `#theme-toggle`.

**Visual identity:** Institutional fintech — navy base, blue primary, green for money/streaks. Not crypto-neon.

---

## Existing APIs (keep compatible)

### Public (no JWT)

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | `{ status, service, version, database }` |
| GET | `/config` | Ad provider config (mock / adinplay / applixir) |
| GET | `/demo` | Demo PWA page |
| POST | `/auth/register` | `{ locale?, referral_code? }` → JWT + user_id |
| POST | `/auth/login` | `{ user_id }` → JWT |
| GET | `/leaderboard/weekly` | Weekly anonymized rankings |
| GET | `/admin` | Admin HTML page |
| GET | `/admin/stats` | Platform stats (`X-Admin-Secret` header) |

### Protected (JWT `Authorization: Bearer`)

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/users/me/wallet` | `balance_usdt`, `localized_balance`, `currency`, `trust_score`, `payout_tier` |
| GET | `/users/me/stats` | Streak, watches today/remaining, milestones, challenge, bonus catalog, payout hints |
| GET | `/users/me/ledger` | Last 50 ledger entries |
| GET | `/users/me/referral` | `referral_code`, `referral_count` |
| POST | `/users/me/watch/complete` | `{ watch_duration_secs, is_emulator?, is_vpn?, ad_provider?, ad_session_id? }` |
| POST | `/users/me/payout/request` | Request payout (demo mode available) |
| GET | `/users/me/ai/context` | Safe AI context for copilot |
| POST | `/users/me/ai/chat` | `{ message }` → copilot reply |

### Admin stats response (`/admin/stats`)

`total_revenue`, `pending_payouts`, `held_payouts`, `user_count`, `recent_payout_count`

### Backend engines (data exists; not all exposed in main UI)

- **Wallet** — balances, ledger, payout tiers  
- **Trust / fraud** — `trust_score` on wallet; fraud checks on watch/payout (no dedicated fraud UI yet)  
- **Referral** — code generation, counts, bonuses  
- **Reward** — watch rewards, streaks, milestones, daily challenge  
- **Revenue** — platform revenue tracked server-side (`total_revenue` in admin only)

---

## What ChatGPT proposed

From user conversation — target **Family Office / unicorn** polish vs typical Watch-to-Earn apps:

1. **Sidebar navigation** — persistent nav (Dashboard, Wallet, Earn, Referrals, Leaderboard, Settings)  
2. **Wallet center** — dedicated balance view with history, tiers, payout pipeline  
3. **Revenue analytics** — user-facing or admin-style earnings breakdown (platform already has `total_revenue` server-side)  
4. **Fraud score** — surface `trust_score` / risk signals prominently (API returns `trust_score` on wallet)  
5. **Referral engine** — richer referral dashboard (codes, funnel, rewards; API: `/users/me/referral` + stats)  
6. **Admin area** — expand `/admin` into full ops console (revenue, holds, users, payouts)

ChatGPT asked for a **screenshot of the current page** to tailor the layout to existing UI rather than a greenfield mockup.

---

## Recommendation: mobile PWA vs desktop sidebar hybrid

**Ship as hybrid — mobile PWA first, desktop sidebar second.**

| Concern | Recommendation |
|---------|----------------|
| Primary audience | Watch-to-Earn users on phones → keep **single-column PWA** as default &lt;768px |
| Professional / “family office” feel | At **≥1024px**, use **collapsible left sidebar** + main content; wallet hero becomes **Wallet Center** panel |
| Navigation | Mobile: sticky header + bottom tab bar (Earn · Wallet · Referrals · More). Desktop: sidebar replaces long scroll |
| Revenue / fraud | Don’t duplicate admin on user app — add **“Trust & earnings”** card for users (`trust_score` + weekly summary); full **Revenue Analytics** stays in `/admin` |
| Referral | Promote to own tab/section with share CTA, count, and estimated bonus (data already in stats + referral APIs) |
| Tech constraint | Stay **vanilla JS** in `frontend/index.html` (or split CSS/JS files only) — no framework migration in v1 |
| PWA | Keep `manifest.webmanifest`, `sw.js`, `theme-color`, install card — sidebar layout must not break offline shell |

**Avoid:** Full desktop-only dashboard that hurts thumb reach on mobile. **Avoid:** Rewriting APIs — map new panels to existing endpoints above.

---

## Paste this to ChatGPT

```
I'm building VANTAGE-EARN, a Watch-to-Earn PWA (Rust/Axum backend, vanilla JS frontend).

Attached: screenshot of the current logged-in dashboard (mobile, navy/blue fintech theme).

Current stack:
- Single file: frontend/index.html (PWA, German UI, max-width 480px mobile-first)
- theme-color #0A0F1A, primary #3B82F6, success #10B981
- Sections today: sticky header → hero wallet → stats grid → watch CTA → activity ledger → weekly leaderboard → referral → bonuses → payout

APIs to preserve (JWT unless noted):
- POST /auth/register, POST /auth/login
- GET /users/me/wallet (balance, trust_score, payout_tier)
- GET /users/me/stats (streak, watches, challenge, bonuses)
- GET /users/me/ledger, GET /users/me/referral
- POST /users/me/watch/complete, POST /users/me/payout/request
- GET /leaderboard/weekly (public)
- GET /admin/stats with X-Admin-Secret (revenue, payouts, users)

You previously suggested: sidebar layout, wallet center, revenue analytics, fraud/trust score UI, referral engine dashboard, expanded admin.

Please design a Family Office / unicorn-level UI that:
1. Keeps mobile PWA as primary (<768px): bottom nav or condensed header, thumb-friendly watch CTA
2. Adds desktop sidebar hybrid (≥1024px): Wallet, Earn, Referrals, Leaderboard, Settings/Admin
3. Surfaces trust_score as "Trust & Safety" without scary fraud UX for normal users
4. Maps each new panel to the existing API fields (no new backend required for v1)
5. Stays implementable in vanilla HTML/CSS/JS — component structure + CSS tokens + wireframe descriptions, not React

Deliver: wireframe descriptions per breakpoint, sidebar item list, which existing DOM IDs to repurpose, and updated CSS token suggestions aligned with #0A0F1A / #3B82F6 palette.
```

---

## Implementation note (for dev team)

**Do not implement the full sidebar layout yet** — this document is the handoff only. Next step after ChatGPT returns wireframes: incremental PR (desktop sidebar shell first, then wallet center, then admin expansion).

---

## Fehlerbehebung (DE) — 404-Banner & Admin-Login

### Rotes „404“ / „Fehler 404“ oben in `/demo`

**Ursache:** Nach einem `git pull` oder Frontend-Update läuft oft noch ein **alter API-Server** (Port 3000). Der kennt neue Routen nicht, z. B. `/users/me/missions`, `/users/me/notifications`, `/users/me/profile-stats`. Beim Laden ruft `loadPremiumData()` diese Endpunkte auf; `api()` zeigt dann `Fehler 404` im roten Toast `#error-banner`.

**Lösung:**

1. `.env` anlegen (falls fehlt): `cp .env.example .env`
2. Server neu bauen und starten: `./scripts/restart-dev.sh` (liest `.env` beim Start ein)
3. Hart neu laden: `/demo?reset=1` oder DevTools → Application → Local Storage `ve_token` löschen

**Prüfen:** `curl -s http://localhost:3000/users/me/missions -H "Authorization: Bearer …"` sollte **401** (nicht 404) liefern, wenn die Route existiert.

### Admin `/admin` — Secret wird abgelehnt

**Ursache:** `ADMIN_SECRET` fehlt in der Server-Umgebung **oder** stimmt nicht mit dem eingegebenen Wert überein. Ohne Variable antwortet die API mit **503** (`ADMIN_SECRET not configured on server`).

**Lokale Dev-Lösung:**

1. In `.env` setzen: `ADMIN_SECRET=VantageAdmin2026!Xk9m` (oder Wert aus `.env.example`: `change-me-admin-secret` — **beides muss identisch sein**)
2. Server **neu starten** (Env wird nur beim Start gelesen): `./scripts/restart-dev.sh`
3. In `/admin` exakt denselben Wert eingeben (kein Leerzeichen; wird getrimmt)
4. Test: `curl -s -H "X-Admin-Secret: VantageAdmin2026!Xk9m" http://localhost:3000/admin/stats` → JSON mit `user_count`, nicht 401/503

**Render/Produktion:** Dashboard → Environment → `ADMIN_SECRET` setzen → Service redeployen. Gespeichertes Secret im Browser (`localStorage.ve_admin_secret`) ggf. löschen.
