# Werbung / Ad-Netzwerk

VANTAGE-EARN nutzt ein **Provider-Pattern** für belohnte Video-Werbung. Der Reward wird erst nach einem **verifizierten Completion-Callback** des Ad-Players gutgeschrieben (`POST /users/me/watch/complete`).

## Provider

| Modus | Env | Verhalten |
|-------|-----|-----------|
| **Mock** (Standard) | `AD_PROVIDER=mock` oder kein AppLixir-Key | 30-Sekunden-Timer im Browser — für lokale Entwicklung und Tests |
| **AppLixir** (Live) | `AD_PROVIDER=applixir` + `APPLIXIR_API_KEY` | Echtes belohntes Video über [AppLixir](https://www.applixir.com) SDK |

Ohne gültigen API-Key fällt der Server automatisch auf **mock** zurück (auch wenn `AD_PROVIDER=applixir` gesetzt ist).

## AppLixir-Konto einrichten

1. Registrieren unter [applixir.com](https://www.applixir.com) (Publisher / HTML5).
2. Im Dashboard eine **Site/App** anlegen und die **API Key** kopieren.
3. `ads.txt`-Einträge aus dem Dashboard auf deiner Domain bereitstellen (für Fill-Rate).
4. Optional: **Server-side Web Callback** (SSV) konfigurieren — für Production sollte die Belohnung serverseitig per signiertem Webhook bestätigt werden (geplant; aktuell sendet der Client `ad_session_id` als Stub).

## Umgebungsvariablen

```bash
# .env
AD_PROVIDER=mock              # mock | applixir
APPLIXIR_API_KEY=xxxx-xxxx  # öffentlicher SDK-Key (alias: APPLIXIR_APP_ID)
AD_WATCH_DURATION_SECS=30     # Mock-Timer / erwartete Videolänge
```

Öffentliche Konfiguration für die PWA: `GET /config`

```json
{
  "ad_provider": "mock",
  "applixir_app_id": null,
  "watch_duration_secs": 30,
  "applixir_sdk_url": "https://cdn.applixir.com/applixir.app.v6.0.1.js"
}
```

## Mock → Live umschalten

**Lokal / Tests (Mock):**

```bash
cargo run -p api-gateway
# oder explizit:
AD_PROVIDER=mock cargo run -p api-gateway
```

**Production (AppLixir):**

```bash
export AD_PROVIDER=applixir
export APPLIXIR_API_KEY=dein-api-key
cargo run -p api-gateway
```

Die PWA lädt beim Start `/config` und wählt automatisch den Provider. Im Overlay erscheint dann z. B. „Belohnte Video-Werbung (AppLixir)“ statt „Entwicklungsmodus — Timer-Werbung (Mock)“.

## Architektur (Kurz)

```
Nutzer tippt „Werbung schauen“
  → AdPlayer.providers[mock|applixir].play()
  → nur bei Completion-Callback (Timer Ende / AppLixir status complete)
  → POST /users/me/watch/complete { watch_duration_secs, ad_provider, ad_session_id? }
  → FraudEngine + RewardEngine → Wallet-Gutschrift
```

Frontend-Provider: `frontend/index.html` (`AdPlayer.providers`).

Backend-Config: `crates/api-gateway/src/ad_config.rs`, Route `GET /config`.
