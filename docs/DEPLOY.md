# VANTAGE-EARN — Production (jedes Handy)

## Warum localhost nicht reicht

| URL | Wer kann zugreifen |
|-----|-------------------|
| `localhost` | Nur der Mac selbst |
| `192.168.x.x` | Nur Geräte im gleichen WLAN |
| **`https://deine-domain.de`** | **Jedes Handy weltweit** |

Handys brauchen eine **öffentliche HTTPS-URL**. PWA-Installation funktioniert auf iPhone/Android nur mit HTTPS (Ausnahme: `localhost` beim Entwickeln).

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

## Eigene Domain (später)

```bash
fly certs add earn.deine-domain.de
```

DNS: CNAME `earn` → `<app-name>.fly.dev`
