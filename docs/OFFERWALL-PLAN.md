# Offerwall-Plan: Bitlabs & CPX Research (Phase B)

## Ziel

VANTAGE Earn ergänzt **Top-Angebote** (Umfragen, App-Tests, Registrierungen) neben den Video-Tiers. Bis Publisher-Accounts und API-Keys vorliegen, liefert das Backend einen **Mock-Katalog**; die PWA zeigt Karten mit 30–80 ct und einen „kommt bald“-Hinweis.

## Anbieter & Registrierung

### BitLabs

| Thema | Details |
|-------|---------|
| **Publisher-Anmeldung** | [https://dashboard.bitlabs.ai/](https://dashboard.bitlabs.ai/) (Account erstellen, App/Placement anlegen) |
| **Dokumentation** | [https://developer.bitlabs.ai/docs/iframeweb-integration](https://developer.bitlabs.ai/docs/iframeweb-integration) |
| **Web-Integration (empfohlen)** | iFrame: `https://web.bitlabs.ai/?uid={USER_ID}&token={APP_TOKEN}` |
| **Alternative** | Neuer Tab: `window.open('https://web.bitlabs.ai/?uid=…&token=…', '_blank')` |
| **Pflichtparameter** | `uid` (stabile Nutzer-ID, max. 255 Zeichen, keine `% $ * & #`), `token` (App/API Token aus Dashboard → Apps → Placement → Integration) |
| **Callbacks** | Server-seitige Postbacks für Gutschriften (S2S); URL im BitLabs-Dashboard konfigurieren |
| **Typische Auszahlung** | Umfragen/Offers variieren stark nach GEO/Profil; UI-Zielband **0,30–0,80 €** pro Abschluss (Schätzung, keine Garantie) |

### CPX Research

| Thema | Details |
|-------|---------|
| **Publisher-Anmeldung** | [https://publisher.cpx-research.com/index.php?page=register](https://publisher.cpx-research.com/index.php?page=register) |
| **Produktseite** | [https://www.cpx-research.com/main/en/](https://www.cpx-research.com/main/en/) |
| **Dokumentation** | [https://www.cpx-research.com/main/en/doc.php](https://www.cpx-research.com/main/en/doc.php) |
| **Script (empfohlen für Web)** | Einzeiliges JS-Widget, passt sich dem App-Design an |
| **iFrame** | `https://offers.cpx-research.com/index.php?app_id={APP_ID}&ext_user_id={USER_ID}&secure_hash={HASH}&…` |
| **API (Katalog)** | `GET https://live-api.cpx-research.com/api/get-surveys.php?app_id=…&ext_user_id=…&output_method=api&ip_user=…&secure_hash=…` |
| **Pflicht für API** | `app_id`, `ext_user_id`, `output_method=api`, `ip_user`; `secure_hash` dringend empfohlen |
| **Postback** | Im Publisher-Dashboard unter „Postback Settings“ eintragen (Pflicht für Auszahlungs-Benachrichtigung) |
| **Typische Auszahlung** | CPX nennt Ø ~5,20 $/Woche pro aktivem Nutzer (GEO-abhängig); Einzel-Umfragen oft im Bereich **0,30–0,80 €** für DE-Mobile |

> **Hinweis:** Beide Anbieter richten sich an **Publisher/Business** — nicht als Umfrage-Teilnehmer registrieren.

## Integrationsoptionen (PWA / HTML5)

| Option | Bitlabs | CPX | Empfehlung |
|--------|---------|-----|------------|
| **iFrame eingebettet** | ✅ `web.bitlabs.ai` | ✅ `offers.cpx-research.com` | **Primär** — bleibt in der PWA, Theme per Query-Param (Bitlabs: `theme=DARK`) |
| **Neuer Tab / Deep Link** | ✅ | ✅ | Fallback wenn iFrame-CSP oder Mobile-WebView Probleme |
| **REST-API (eigene Karten)** | Offer-API + Deep-Link | `get-surveys.php` | Phase C — Top-Angebote aus Live-Katalog rendern |
| **Server Postback** | S2S Callback | Postback URL | **Pflicht** für echte Gutschrift ins Wallet |

### PWA-Hinweise

- Offerwall-iFrames liegen **außerhalb** des Service-Worker-Caches (`web.bitlabs.ai`, `offers.cpx-research.com`) — kein SW-Bump nötig für Provider-URLs.
- Stabile `uid` / `ext_user_id` = VANTAGE `user_id` (UUID), damit Postbacks zuordenbar sind.
- Vollbild-Modal mit Schließen-Button (X) für iFrame; bei Bitlabs: Zurück-Navigation wenn URL nicht mehr `web.bitlabs.ai` enthält.

## Erwartete Nutzer-Provision (UI)

| Angebotstyp | Anzeige (Mock) | Aufwand-Hinweis |
|-------------|----------------|-----------------|
| Kurz-Umfrage | ≈ 0,30 € | ca. 2 Min. |
| Standard-Umfrage | ≈ 0,45 € | ca. 5 Min. |
| App testen | ≈ 0,65 € | ca. 3 Min. |
| Gaming-Angebot | ≈ 0,55 € | ca. 15 Min. |
| Registrieren | ≈ 0,80 € | ca. 10 Min. |

Alle Beträge sind **Schätzungen** bis Live-Postbacks; keine Ertragsgarantie (siehe AGB-Microcopy in der App).

## Umgebungsvariablen (geplant)

```bash
# Bitlabs
BITLABS_APP_TOKEN=          # App/API Token aus Dashboard
BITLABS_SECRET_KEY=         # Für Callback-Signatur (S2S)
BITLABS_CALLBACK_URL=       # Öffentliche URL, z. B. https://api.vantage-earn.example/webhooks/bitlabs

# CPX Research
CPX_APP_ID=
CPX_SECURE_HASH_KEY=        # Für secure_hash bei iFrame/API
CPX_POSTBACK_URL=           # z. B. https://api.vantage-earn.example/webhooks/cpx

# Feature-Flags
OFFERWALL_ENABLED=false     # true wenn Keys gesetzt und Postbacks getestet
OFFERWALL_PRIMARY=bitlabs   # bitlabs | cpx | both
```

Bis `OFFERWALL_ENABLED=true` liefert das Backend `status: mock` / `coming_soon`.

## Backend (MVP — implementiert)

| Endpoint | Response |
|----------|----------|
| `GET /users/me/top-offers` | `{ "offers": [ … ] }` |
| `GET /users/me/stats` | enthält zusätzlich `top_offers` (gleiche Struktur) |

### Angebots-Felder

| Feld | Beispiel |
|------|----------|
| `id` | `top-survey-short` |
| `category` | `survey` |
| `label_de` | `Umfrage` |
| `reward_eur_display` | `≈ 0,45 €` |
| `reward_eur_cents` | `45` |
| `effort_hint_de` | `ca. 5 Min.` |
| `provider` | `bitlabs` \| `cpx` |
| `status` | `mock` \| `coming_soon` |

Modul: `crates/api-gateway/src/top_offers.rs` (analog zu `video_offers.rs`).

## Frontend (MVP — implementiert)

- Sektion **„Top-Angebote“** in der Earn-Ansicht (über den Video-Tiers)
- Karten mit Label, Aufwand, 30–80 ct
- Tap → Toast: *„Offerwall kommt bald — Bitlabs/CPX Anbindung in Arbeit“*

## Phasen-Roadmap

| Phase | Inhalt | Status |
|-------|--------|--------|
| **B** | Mock-Katalog, API, UI-Karten, dieser Plan | ✅ |
| **C** | Publisher-Accounts, Env-Keys, Postback-Endpoints, Signatur-Validierung | Geplant |
| **D** | Live-iFrame/Modal, `OFFERWALL_ENABLED`, Wallet-Gutschrift aus Postback | Geplant |
| **E** | CPX/Bitlabs API-Katalog → dynamische Top-Angebote, A/B Primary Provider | Geplant |

## Nächste Schritte für das Team

1. **Bitlabs:** Account → App „VANTAGE Earn Web“ → Token + Secret → Callback-URL in Staging testen  
2. **CPX:** Publisher-Registrierung → App anlegen → `app_id` + Secure-Hash-Key → Postback-URL  
3. **Rechtliches:** Offerwall-Hinweis in Datenschutz/AGB (Drittanbieter-Umfragen, Datenweitergabe)  
4. **Engineering Phase C:** `POST /webhooks/bitlabs`, `POST /webhooks/cpx`, Fraud-Checks, Ledger-Eintrag  

## Referenzen

- Bitlabs iFrame: https://developer.bitlabs.ai/docs/iframeweb-integration  
- Bitlabs Web: https://bitlabs.ai/integrations/web-integration  
- CPX Docs: https://www.cpx-research.com/main/en/doc.php  
- CPX Publisher Signup: https://publisher.cpx-research.com/index.php?page=register  
