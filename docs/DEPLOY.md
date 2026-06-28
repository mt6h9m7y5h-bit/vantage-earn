# VANTAGE-EARN — Production (jedes Handy)

## Lokal mit PostgreSQL (heute Abend testen)

Docker muss laufen. Credentials wie in `.env.example`:

```bash
cd vantage-earn
cp .env.example .env          # DATABASE_URL ist voreingestellt
./scripts/dev-with-db.sh      # startet Postgres + API
```

Oder Schritt für Schritt:

```bash
docker compose up -d postgres
export DATABASE_URL=postgres://vantage:vantage@localhost:5432/vantage_earn
cargo run -p api-gateway
```

Prüfen:

```bash
curl -s http://localhost:3000/health | jq
# → "database": true
./scripts/test-e2e.sh
```

Migrationen laufen automatisch beim API-Start (oder `./scripts/db-migrate.sh`).

---

## Warum localhost nicht reicht

| URL | Wer kann zugreifen |
|-----|-------------------|
| `localhost` | Nur der Mac selbst |
| `192.168.x.x` | Nur Geräte im gleichen WLAN |
| **`https://deine-domain.de`** | **Jedes Handy weltweit** |

Handys brauchen eine **öffentliche HTTPS-URL**. PWA-Installation funktioniert auf iPhone/Android nur mit HTTPS (Ausnahme: `localhost` beim Entwickeln).

---

## Fly.io (empfohlen — kostenarm, HTTPS automatisch)

Render-Pipeline-Minuten sind aufgebraucht; Hobby ($25/Monat) ist zu teuer. **Fly.io ist vorbereitet** (`fly.toml`, `docs/FLY.md`).

> **Bis Fly läuft:** `./scripts/dev-with-db.sh` lokal oder bestehende Render-URL (Service `34e4941` läuft noch).

```bash
cd vantage-earn
./scripts/deploy-fly.sh
```

Oder Schritt für Schritt: **[docs/FLY.md](FLY.md)** (deutsch, inkl. Postgres + Secrets).

Kurzversion:

```bash
curl -L https://fly.io/install.sh | sh
export PATH="$HOME/.fly/bin:$PATH"
fly auth login
fly launch --no-deploy --copy-config --name vantage-earn-DEINNAME --region fra
fly secrets set JWT_SECRET="$(openssl rand -hex 32)" ADMIN_SECRET="$(openssl rand -hex 32)" \
  APP_URL="https://vantage-earn-DEINNAME.fly.dev" --app vantage-earn-DEINNAME
fly deploy --app vantage-earn-DEINNAME
```

---

## Render.com (pausiert — Pipeline-Minuten aufgebraucht)

Repo auf GitHub pushen, dann im [Render Dashboard](https://dashboard.render.com):

```bash
# Lokal: letzten Stand committen & pushen
git push origin main
```

1. **New → Blueprint** → Repo auswählen → `render.yaml` erkennt Web + Postgres automatisch
2. Deploy abwarten (~5–10 Min beim ersten Mal)
3. Prüfen:

```bash
curl -s https://DEIN-SERVICE.onrender.com/health | jq
# → "database": true, "status": "ok"
```

Demo-URL: `https://DEIN-SERVICE.onrender.com/demo`

Migrationen laufen beim ersten API-Start automatisch (`sqlx::migrate!` in `PgStore::connect`).

**Hinweis:** Free-Tier-Postgres und Web-Service schlafen nach Inaktivität — erster Request kann ~30 s dauern.

---

## Alternative: Docker auf eigenem Server

```bash
docker compose up -d
```

Vor dem Internet brauchst du **Caddy/nginx + Let's Encrypt** für HTTPS. Ohne HTTPS: App lädt evtl., PWA-Install auf iPhone oft nicht.

---

## Checkliste „funktioniert auf jedem Handy“

- [ ] Öffentliche URL mit **HTTPS**
- [ ] `JWT_SECRET` gesetzt (nicht Dev-Default)
- [ ] `DATABASE_URL` für persistente Daten (optional für Demo)
- [ ] `/demo` im Browser testen (Android + iPhone)
- [ ] „Zum Home-Bildschirm“ / „App installieren“ testen

---

## E-Mail (Willkommen & Passwort-Reset)

Ohne Resend-API-Key werden E-Mails in Production nur geloggt (nicht zugestellt). Render blockiert oft ausgehende SMTP-Ports (587/465) — daher **HTTPS API** statt SMTP:

1. [resend.com](https://resend.com) — Domain verifizieren, API-Key erzeugen (`re_…`)
2. Render Dashboard → **vantage-earn** → **Environment**:

| Variable | Wert |
|----------|------|
| `APP_URL` | `https://vantage-earn.onrender.com` |
| `SMTP_PASS` | dein Resend API-Key (`re_…`) — **Secret**, nicht ins Repo (Name bleibt aus Kompatibilität) |
| `SMTP_FROM` | `VANTAGE-EARN <noreply@deine-domain.de>` (no-reply, verifizierte Absender-Domain) |

Optional: `RESEND_API_KEY` statt `SMTP_PASS` (gleicher `re_…`-Wert).

`SMTP_HOST` / `SMTP_PORT` / `SMTP_USER` sind auf Render **nicht** nötig.

3. Service neu deployen (oder „Save“ triggert Redeploy)
4. Test: Auf `/demo` Konto registrieren (Willkommens-Mail) oder „Passwort vergessen“ — Reset-Link: `https://vantage-earn.onrender.com/demo?reset=TOKEN`

Logs: Render → Logs → `transactional email sent (Resend API)` oder `Resend API error`.

---

## Eigene Domain (später)

```bash
fly certs add earn.deine-domain.de
```

DNS: CNAME `earn` → `<app-name>.fly.dev`
