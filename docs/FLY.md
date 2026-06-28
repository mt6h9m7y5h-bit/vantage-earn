# VANTAGE-EARN auf Fly.io deployen

Render-Pipeline-Minuten sind aufgebraucht; Hobby ($25/Monat) ist zu teuer. **Fly.io** ist die vorbereitete Alternative: HTTPS automatisch, Maschine schläft bei Inaktivität (`min_machines_running = 0`).

> **Bis Fly läuft:** Lokal entwickeln (`./scripts/dev-with-db.sh`) oder die bestehende Render-URL nutzen — Service `34e4941` läuft noch.

Alle Befehle unten führst **du selbst** im Terminal aus (Fly-Account nötig, keine Secrets ins Repo).

---

## Voraussetzungen

- Repo geklont: `cd vantage-earn`
- [Fly.io Account](https://fly.io/app/sign-up) (kostenlos registrieren)
- Optional: [Resend](https://resend.com) für E-Mail (Willkommen, Passwort-Reset)

### flyctl installieren

```bash
curl -L https://fly.io/install.sh | sh
export PATH="$HOME/.fly/bin:$PATH"
```

---

## Schnellstart (empfohlen)

```bash
cd vantage-earn
./scripts/deploy-fly.sh
```

Das Skript installiert `flyctl` falls nötig, öffnet den Login-Browser, wählt einen eindeutigen App-Namen (`vantage-earn-deinname`) und setzt `JWT_SECRET` + `ADMIN_SECRET`.

---

## Manuell Schritt für Schritt

### 1. Einloggen

```bash
fly auth login
```

### 2. App anlegen (nur beim ersten Mal)

```bash
cd vantage-earn
fly launch --no-deploy --copy-config --name vantage-earn-DEINNAME --region fra
```

`DEINNAME` = eindeutig (z. B. dein GitHub-User). Wenn `vantage-earn` schon vergeben ist, anderen Namen wählen.

### 3. Secrets setzen (Pflicht)

**Niemals** Secrets committen — nur per `fly secrets set`:

```bash
APP_NAME=vantage-earn-DEINNAME   # wie bei launch gewählt
FLY_URL="https://${APP_NAME}.fly.dev"

fly secrets set \
  JWT_SECRET="$(openssl rand -hex 32)" \
  ADMIN_SECRET="$(openssl rand -hex 32)" \
  APP_URL="$FLY_URL" \
  BITLABS_CALLBACK_BASE_URL="$FLY_URL" \
  --app "$APP_NAME"
```

| Variable | Pflicht | Beschreibung |
|----------|---------|--------------|
| `JWT_SECRET` | **Ja** | Session-/Token-Signierung (min. 32 Zeichen Zufall) |
| `ADMIN_SECRET` | **Ja** | Admin-Panel (`/admin`, Header `X-Admin-Secret`) |
| `DATABASE_URL` | Empfohlen | PostgreSQL — siehe Abschnitt Postgres unten |
| `APP_URL` | Empfohlen | Öffentliche URL für E-Mail-Links (`https://…fly.dev`) |
| `SMTP_PASS` oder `RESEND_API_KEY` | Für E-Mail | Resend API-Key (`re_…`) |
| `SMTP_FROM` | Mit E-Mail | z. B. `VANTAGE-EARN <noreply@deine-domain.de>` |
| `BITLABS_CALLBACK_BASE_URL` | Mit Offerwall | Gleiche Origin wie `APP_URL` |
| `EARLY_ADOPTER_BONUS_USDT` | Nein | Default `0.5` |
| `EARLY_ADOPTER_MAX_USERS` | Nein | Default `40` |
| `EARLY_ADOPTER_DAYS` | Nein | Default `30` |

E-Mail (Resend HTTP API — **kein** SMTP-Port nötig auf Fly):

```bash
fly secrets set \
  SMTP_PASS="re_DEIN_RESEND_KEY" \
  SMTP_FROM="VANTAGE-EARN <noreply@deine-domain.de>" \
  --app "$APP_NAME"
```

### 4. Deployen

```bash
fly deploy --app "$APP_NAME"
```

Erster Build dauert ~5–10 Min (Rust Release-Build im Dockerfile).

### 5. Prüfen

```bash
curl -s "https://${APP_NAME}.fly.dev/health" | jq
fly open /demo --app "$APP_NAME"
```

Erwartung: `"status": "ok"`. Mit Postgres zusätzlich `"database": true`.

### 6. Logs

```bash
fly logs --app "$APP_NAME"
```

---

## PostgreSQL auf Fly (empfohlen)

Ohne `DATABASE_URL` nutzt die API einen **In-Memory-Store** — Daten gehen beim Neustart verloren.

### Neue Fly-Postgres-DB

```bash
fly postgres create --name vantage-earn-db-DEINNAME --region fra
fly postgres attach vantage-earn-db-DEINNAME --app "$APP_NAME"
fly deploy --app "$APP_NAME"
```

`fly postgres attach` setzt `DATABASE_URL` automatisch als Secret. Migrationen laufen beim API-Start.

### Externe Postgres-URL

Falls du bereits eine DB hast (z. B. alter Render-Postgres, Supabase, Neon):

```bash
fly secrets set DATABASE_URL="postgres://USER:PASS@HOST:5432/DBNAME" --app "$APP_NAME"
fly deploy --app "$APP_NAME"
```

---

## Kosten-Hinweis

- `fly.toml`: `min_machines_running = 0` + `auto_stop_machines` → Maschine schläft bei Inaktivität (kälter Start ~10–30 s).
- Fly rechnet nach Nutzung ab; prüfe [fly.io/docs/about/pricing](https://fly.io/docs/about/pricing/) und dein Dashboard.
- Postgres auf Fly ist **nicht** dauerhaft kostenlos — für MVP reicht oft In-Memory oder eine kleine externe Free-DB.

---

## Updates nach Code-Änderungen

```bash
git pull
fly deploy --app "$APP_NAME"
```

---

## Eigene Domain (später)

```bash
fly certs add earn.deine-domain.de --app "$APP_NAME"
```

DNS: CNAME `earn` → `<app-name>.fly.dev`, danach `APP_URL` und `BITLABS_CALLBACK_BASE_URL` auf die neue Domain setzen.

---

## Troubleshooting

| Problem | Lösung |
|---------|--------|
| Build OOM | Dockerfile nutzt bereits `CARGO_BUILD_JOBS=1`; ggf. `fly scale memory 1024` |
| `JWT_SECRET must be set` | `fly secrets set JWT_SECRET=...` und redeploy |
| E-Mail nur in Logs | `SMTP_PASS`/`RESEND_API_KEY` + `SMTP_FROM` setzen |
| Health check failed | `fly logs`; Postgres-URL prüfen wenn `DATABASE_URL` gesetzt |
| App-Name vergeben | Anderen `--name` bei `fly launch` wählen |

---

## Checkliste „funktioniert auf jedem Handy“

- [ ] `https://DEIN-APP.fly.dev/demo` im Browser (HTTPS)
- [ ] `JWT_SECRET` und `ADMIN_SECRET` gesetzt
- [ ] `DATABASE_URL` für persistente Nutzerdaten
- [ ] PWA „Zum Home-Bildschirm“ auf iPhone/Android getestet
