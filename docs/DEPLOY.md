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

## Render.com (morgen — mit PostgreSQL)

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

## Empfohlen: Fly.io (kostenlos starten, HTTPS automatisch)

### Schnell (ein Befehl)

```bash
cd vantage-earn
./scripts/deploy-fly.sh
```

Beim ersten Mal: Browser öffnet sich für **Fly.io Login** (kostenloser Account).

Danach bekommst du z.B. `https://vantage-earn-deinname.fly.dev/demo` — **funktioniert auf jedem Handy weltweit**.

---

### Manuell

```bash
curl -L https://fly.io/install.sh | sh
export PATH="$HOME/.fly/bin:$PATH"
fly auth login

cd vantage-earn
fly launch --no-deploy
fly secrets set JWT_SECRET="$(openssl rand -hex 32)"
fly deploy
fly open /demo
```

Du bekommst z.B. `https://vantage-earn.fly.dev/demo` — diese URL auf **jedem Handy** öffnen oder zum Home-Bildschirm hinzufügen.

### PostgreSQL (empfohlen für echte Nutzer)

```bash
fly postgres create --name vantage-earn-db --region fra
fly postgres attach vantage-earn-db
fly deploy
```

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

Ohne SMTP werden E-Mails in Production nur geloggt (nicht zugestellt). Für Resend:

1. [resend.com](https://resend.com) — Domain verifizieren, API-Key erzeugen
2. Render Dashboard → **vantage-earn** → **Environment**:

| Variable | Wert |
|----------|------|
| `APP_URL` | `https://vantage-earn.onrender.com` |
| `SMTP_HOST` | `smtp.resend.com` |
| `SMTP_PORT` | `587` |
| `SMTP_USER` | `resend` |
| `SMTP_PASS` | dein Resend API-Key (`re_…`) — **Secret**, nicht ins Repo |
| `SMTP_FROM` | `VANTAGE-EARN <noreply@deine-domain.de>` (no-reply, verifizierte Absender-Domain) |

3. Service neu deployen (oder „Save“ triggert Redeploy)
4. Test: Auf `/demo` Konto registrieren (Willkommens-Mail) oder „Passwort vergessen“ — Reset-Link: `https://vantage-earn.onrender.com/demo?reset=TOKEN`

Logs: Render → Logs → `transactional email sent` oder `SMTP send failed`.

---

## Eigene Domain (später)

```bash
fly certs add earn.deine-domain.de
```

DNS: CNAME `earn` → `<app-name>.fly.dev`
