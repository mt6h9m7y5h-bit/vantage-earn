# Kostenlos deployen (ohne Cloud-Deploy)

## Option 1: Cloudflare Tunnel (empfohlen, 0 €)

Dein Mac wird zur Brücke — du bekommst eine **HTTPS-URL** für jedes Handy.

**Tab 1** — Server:
```bash
cargo run -p api-gateway
```

**Tab 2** — Tunnel:
```bash
./scripts/tunnel-free.sh
```

Du siehst z.B.:
```
https://random-name.trycloudflare.com
```

Am Handy öffnen: **`https://random-name.trycloudflare.com/demo`**

| Pro | Contra |
|-----|--------|
| 0 €, sofort | Mac muss laufen |
| HTTPS, jedes Handy | URL ändert sich bei Neustart |

---

## Option 2: Gleiches WLAN (0 €, nur lokal)

```
http://192.168.2.171:3000/demo
```

Nur Geräte im gleichen WLAN — kein HTTPS, keine PWA-Installation auf iPhone.

---

## Option 3: Render.com Free Tier (0 €, dauerhaft online)

- Repo auf GitHub pushen
- [render.com](https://render.com) → New → Blueprint → `render.yaml`
- Free Plan: App schläft nach 15 Min Inaktivität, wacht bei Request auf
- Automatische HTTPS-URL: `https://vantage-earn.onrender.com/demo`

Keine Kreditkarte für Free Tier nötig.

---

## Später: Render / eigene Domain

Wenn du echte Nutzer und 24/7-Betrieb willst → [DEPLOY.md](./DEPLOY.md)
