# Gemini Handoff: Video-Angebote & Provisions-Anzeige

## Kontext

VANTAGE Earn zeigt Nutzern **Video-Angebote** mit automatisch berechneter Provision nach Videolänge. Der Nutzer wählt selbst ein Angebot (30/60/90/120 Sekunden). Die Anzeige erfolgt primär in **EUR (geschätzt)**; die Gutschrift erfolgt in **USDT**.

## Formel (Backend, `RewardEngine` + `video_offers`)

```
segments = max(1, duration_secs / 30)
base_reward_usdt = 0.001 × segments
streak_mult = 1 + min(streak_days × 5%, 50%)
reward_usdt = base_reward_usdt × streak_mult   (vor Surprise & Fraud)
```

Optional bei Mega-Angebot (max. 2×/Tag/Nutzer, In-Memory MVP):
```
reward_usdt_display = base_reward_usdt × streak_mult × bonus_multiplier (2×)
```

Surprise (5 % Chance, 3×) wird erst bei `POST /users/me/watch/complete` angewendet — **nicht** in der Angebots-Vorschau.

EUR-Anzeige (UI):
```
reward_eur_display = reward_usdt × 0.92   (fester UI-Kurs, siehe currency-engine)
Format: „≈ 0,00 €“ (deutsches Komma)
```

Nutzeranteil an Werbeerlös: **40 %** (`USER_REVENUE_SHARE`).

## API

- `GET /users/me/video-offers` → `{ "offers": [ … ] }`
- `GET /users/me/stats` enthält zusätzlich `video_offers` (gleiche Struktur)
- `POST /users/me/watch/complete` mit `watch_duration_secs` + optional `offer_tier` (`quick`|`standard`|`premium`|`mega`)

### Angebots-Felder

| Feld | Beispiel |
|------|----------|
| `tier` | `quick` |
| `duration_secs` | 30 |
| `label_de` | Schnell |
| `reward_usdt` | `0.001` |
| `reward_eur_display` | `≈ 0,00 €` |
| `bonus_multiplier` | `2` (nur Mega, wenn Slots frei) |

## Aufgaben für Gemini

### 1. Plausibilität EUR vs. AdInPlay

Vergleiche unsere **geschätzte Nutzer-Provision** (USDT×0,92) mit typischen AdInPlay-Ranges von **8–25 € pro 1.000 Impressions** (CPM).

Beispielrechnung (0 % Streak, ohne Surprise):
- 30 s → 0,001 USDT ≈ 0,00092 €
- 120 s → 0,004 USDT ≈ 0,00368 €

Fragen:
- Sind die angezeigten Beträge für Endnutzer verständlich und nicht irreführend?
- Soll die UI einen Hinweis „kleine Beträge, ehrliche Erwartung“ ergänzen?
- Empfehlung für Mindest-/Maximal-Anzeige pro Tier?

### 2. Tier-Labels (DE)

Aktuell: **Schnell** (30s), **Standard** (60s), **Premium** (90s), **Mega** (120s).

Vorschläge für:
- Alternativ-Labels (z. B. „Kurz“, „Mittel“, „Lang“, „Max“)
- Microcopy auf den Karten (1 Zeile Motivation, ohne Hype)
- Reihenfolge / visuelle Hierarchie (welches Angebot hervorheben?)

### 3. Rechtssichere Formulierung „≈ geschätzt“

Entwurf für UI-Fußnote / Tooltip:
> „Die angezeigte Provision in Euro ist eine **geschätzte** Umrechnung zum aktuellen Kurs und kann von der tatsächlichen Gutschrift in USDT abweichen. Endgültige Beträge siehst du im Wallet.“

Bitte prüfen und verbessern (DE, AGB/Datenschutz-konform, keine Ertragsgarantie).

### 4. Wirtschaftlichkeit (optional)

Bei 40 % Nutzeranteil und CPM 8–25 €/1000:
- Grobe Break-even-Schätzung pro Watch
- Empfehlung, ob Tier-Struktur 30/60/90/120 oder 15/30/45/60 sinnvoller ist für DE-Mobile

## Prompt (Copy-Paste für Gemini)

```
Du bist Wirtschafts- und UX-Berater für VANTAGE Earn (Watch-to-Earn PWA, DE-Markt).

Daten:
- 4 Video-Tiers: 30/60/90/120 Sek., Labels Schnell/Standard/Premium/Mega
- Belohnung: 0,001 USDT pro 30s-Segment × Streak (max +50%) × optional 2× Mega-Bonus (2×/Tag)
- Anzeige: EUR = USDT × 0,92, Format „≈ X,XX €“
- Nutzeranteil Werbeerlös: 40 %
- AdInPlay-Benchmark: ca. 8–25 € / 1000 Impressions

Aufgaben:
1. Validiere, ob EUR-Anzeigen realistisch vs. AdInPlay sind (keine Übertreibung).
2. Schlage bessere deutsche Tier-Labels und 1-Zeiler-Microcopy vor.
3. Formuliere rechtssicheren Hinweis „≈ geschätzt“ (keine Ertragsgarantie).
4. Optional: Empfehlung Tier-Dauern und ob Fußnote unter den Angebots-Karten nötig ist.

Antwort auf Deutsch, strukturiert, umsetzbar für Produktteam.
```
