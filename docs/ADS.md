# Werbung / Ad-Netzwerk

VANTAGE-EARN nutzt ein **Provider-Pattern** für belohnte Video-Werbung. Der Reward wird erst nach einem **verifizierten Completion-Callback** des Ad-Players gutgeschrieben (`POST /users/me/watch/complete`).

## Provider

| Modus | Env | Verhalten |
|-------|-----|-----------|
| **Mock** (Standard) | `AD_PROVIDER=mock` oder keine Live-Credentials | 30-Sekunden-Timer im Browser — für lokale Entwicklung und Tests |
| **AdInPlay** (empfohlen) | `AD_PROVIDER=adinplay` + Publisher/Site-ID | Echtes belohntes Video über [AdInPlay](https://adinplay.com) — indie-freundlich, keine Business-E-Mail nötig |
| **AppLixir** (Live) | `AD_PROVIDER=applixir` + `APPLIXIR_API_KEY` | Echtes belohntes Video über [AppLixir](https://www.applixir.com) SDK |

Ohne gültige Credentials fällt der Server automatisch auf **mock** zurück (auch wenn `AD_PROVIDER=adinplay` oder `applixir` gesetzt ist).

### Vergleich

| | Mock | AdInPlay | AppLixir |
|---|------|----------|----------|
| **Echte Ads** | Nein | Ja | Ja |
| **Signup** | — | [adinplay.com](https://adinplay.com) | [applixir.com](https://www.applixir.com) |
| **Business-E-Mail** | — | Nicht erforderlich | Oft erforderlich |
| **Indie / Solo-Dev** | Dev only | **Empfohlen** | Möglich |
| **Integration** | Timer | `aiptag` + rewarded video | SDK + API Key |
| **Reward-Callback** | Timer-Ende | `AIP_REWARDEDGRANTED` | `adStatusCallback` complete |

**Empfehlung für Solo-Entwickler:** AdInPlay als primärer Live-Provider — einfache HTML5-Integration, browser-game-fokussiert, keine Firmen-Mail für die Anmeldung.

## AdInPlay-Konto einrichten

1. Registrieren unter [adinplay.com/publishers](https://adinplay.com/publishers) (Publisher / HTML5-Spiele).
2. Nach Freischaltung erhältst du einen **Tag** mit Publisher-ID und Site-ID (Format: `api.adinplay.com/libs/aiptag/pub/{PUBLISHER}/{SITE}/tag.min.js`).
3. `ads.txt`-Einträge aus dem Dashboard auf deiner Domain bereitstellen (für Fill-Rate).
4. Optional: Rewarded-Video-Slots im Dashboard aktivieren.

## AppLixir-Konto einrichten

1. Registrieren unter [applixir.com](https://www.applixir.com) (Publisher / HTML5).
2. Im Dashboard eine **Site/App** anlegen und die **API Key** kopieren.
3. `ads.txt`-Einträge aus dem Dashboard auf deiner Domain bereitstellen (für Fill-Rate).
4. Optional: **Server-side Web Callback** (SSV) konfigurieren — für Production sollte die Belohnung serverseitig per signiertem Webhook bestätigt werden (geplant; aktuell sendet der Client `ad_session_id` als Stub).

## Umgebungsvariablen

```bash
# .env
AD_PROVIDER=mock              # mock | adinplay | applixir
AD_WATCH_DURATION_SECS=30     # Mock-Timer / erwartete Videolänge

# AdInPlay (empfohlen für Live)
ADINPLAY_PUBLISHER_ID=ABC     # aus AdInPlay-Dashboard (pub/ABC/...)
ADINPLAY_SITE_ID=deine-domain.com
# oder vollständige Tag-URL:
# ADINPLAY_TAG_URL=https://api.adinplay.com/libs/aiptag/pub/ABC/deine-domain.com/tag.min.js

# AppLixir (Alternative)
APPLIXIR_API_KEY=xxxx-xxxx  # öffentlicher SDK-Key (alias: APPLIXIR_APP_ID)
```

Öffentliche Konfiguration für die PWA: `GET /config`

```json
{
  "ad_provider": "mock",
  "adinplay_tag_url": null,
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

**Production (AdInPlay):**

```bash
export AD_PROVIDER=adinplay
export ADINPLAY_PUBLISHER_ID=dein-publisher-code
export ADINPLAY_SITE_ID=deine-domain.com
cargo run -p api-gateway
```

**Production (AppLixir):**

```bash
export AD_PROVIDER=applixir
export APPLIXIR_API_KEY=dein-api-key
cargo run -p api-gateway
```

Die PWA lädt beim Start `/config` und wählt automatisch den Provider. Im Overlay erscheint dann z. B. „Belohnte Video-Werbung (AdInPlay)“ statt „Entwicklungsmodus — Timer-Werbung (Mock)“.

## Architektur (Kurz)

```
Nutzer tippt „Werbung schauen“
  → AdPlayer.providers[mock|adinplay|applixir].play()
  → nur bei Completion-Callback (Timer Ende / AIP_REWARDEDGRANTED / AppLixir status complete)
  → POST /users/me/watch/complete { watch_duration_secs, ad_provider, ad_session_id? }
  → FraudEngine + RewardEngine → Wallet-Gutschrift
```

Frontend-Provider: `frontend/index.html` (`AdPlayer.providers`).

Backend-Config: `crates/api-gateway/src/ad_config.rs`, Route `GET /config`.
