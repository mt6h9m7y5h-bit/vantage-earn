# Rechts- & Compliance-Audit — VANTAGE-EARN

**Stand der Prüfung:** 28. Juni 2026  
**Geprüfte Codebasis:** `/Users/erencan/Downloads/vantage-earn` (Branch `main`)  
**Production-URL:** https://vantage-earn.onrender.com/demo  
**Audit-Typ:** Technisch-dokumentarische Vorabprüfung (keine anwaltliche Beratung)

---

## 1. Hinweis & Disclaimer

**Dieses Dokument ist keine Rechtsberatung.** Es basiert auf einer statischen Code- und Dokumentenprüfung durch ein technisches Audit-Tool. Für einen produktiven Launch in Deutschland (und ggf. weiteren EU-Märkten) ist **zwingend anwaltliche Beratung** durch Fachanwälte für IT-Recht, Datenschutzrecht und ggf. Bank-/Finanzaufsichtsrecht erforderlich.

Rechtsgrundlagen werden nur dort zitiert, wo sie allgemein anerkannt und im Kontext plausibel sind (DSGVO, TDDDG, UWG, DDG, GwG, MiCA/BaFin-Leitlinien). **Es werden keine Rechtsnormen erfunden.** Unsicherheiten sind explizit als solche gekennzeichnet.

---

## 2. Executive Summary

VANTAGE-EARN ist eine Watch-to-Earn-PWA mit virtuellem USDT-Wallet, Offerwall-Integration (BitLabs), Gamification, Referral-Programm und manueller/automatisierter Auszahlungsprüfung. Die App enthält **MVP-Legaltexte mit expliziten Testdaten-Hinweisen**, ein **Consent-Banner (Frontend)**, **Registrierungs-Checkboxen (AGB + 18 Jahre)** und **serverseitige Validierung** von `accept_terms` / `accept_age_minimum`.

**Kernbefund:** Der Dienst ist für einen **öffentlichen Produktiv-Launch in Deutschland nicht rechtskonform aufgestellt**. Die kritischsten Blocker sind:

| Priorität | Thema | Status |
|-----------|-------|--------|
| 🔴 | Impressum / Anbieterkennzeichnung (DDG) | Platzhalter, nicht launchfähig |
| 🔴 | Datenschutzerklärung unvollständig (DSGVO Art. 13/14) | Fehlende Prozessoren, US-Transfers, Werbe-Partner |
| 🔴 | BaFin-/Zahlungsdienst-Risiko bei Krypto-Auszahlungen | Ungeklärt, keine Lizenzdokumentation |
| 🟠 | TDDDG-Consent bei Live-Werbung/Offerwall | Banner vorhanden, aber unvollständig / nicht CMP-konform für Drittanbieter |
| 🟠 | UWG-kritische Werbeaussagen | Meta-Tags, E-Mails, Onboarding widersprechen AGB/Disclaimern |
| 🟠 | GwG/AML bei Auszahlungen | Kein KYC, keine Risikoanalyse dokumentiert |
| 🟡 | Betroffenenrechte (Export, Auskunft) | Löschung ja, strukturierter Export nein |
| 🟡 | AV-Verträge (Render, Resend, BitLabs, OpenAI) | Nicht nachweisbar |

**Legal Readiness (Abschnitt 26): 28 / 100 %**

---

## 3. Impressumspflicht (DDG — ehem. TMG § 5)

### Befund

Datei: `frontend/legal/impressum.html`

- Expliziter Hinweis: *„Testdaten — vor Launch durch echte Angaben ersetzen“*
- Angaben: „VANTAGE-EARN Betreiber, Deutschland“ — **keine ladungsfähige Anschrift**
- Fehlend: Rechtsform, Vertretungsberechtigter, Handelsregister/Registernummer, USt-IdNr. (falls umsatzsteuerpflichtig)
- Verweis auf **§ 5 TMG** — seit Mai 2024 ist die Impressumspflicht im **Digitale-Dienste-Gesetz (DDG)** geregelt (funktional vergleichbar, aber Verweis veraltet)
- Verweis auf **§ 55 Abs. 2 RStV** — durch **MStV** (Medienstaatsvertrag) abgelöst; Formulierung sollte aktualisiert werden
- EU-OS-Plattform verlinkt ✓
- Verbraucherschlichtung: „nicht bereit“ — **nur zulässig, wenn keine gesetzliche Teilnahmepflicht** (z. B. bei bestimmten B2C-Verträgen); anwaltlich prüfen

### Rechtsfolge

Ein Impressum mit Platzhalterdaten erfüllt die **Anbieterkennzeichnungspflicht nicht** (DDG). Abmahn- und Bußgeldrisiko (v. a. Wettbewerbsrecht/UWG, auch DSGVO bei fehlendem Verantwortlichen).

### Empfohlene Maßnahmen

**Beispieltext (Auszug — durch Anwalt finalisieren):**

```
Max Mustermann
VANTAGE Earn — [Rechtsform, z. B. Einzelunternehmen]
Musterstraße 1
10115 Berlin
Deutschland

E-Mail: kontakt@ihre-domain.de
Telefon: +49 … (Pflicht, wenn vorhanden — empfohlen)

Umsatzsteuer-ID: DE… (falls vorhanden)
Registergericht … HRB … (falls eingetragen)
Vertretungsberechtigt: Max Mustermann
```

---

## 4. Unternehmensform, Gewerbe & Steuerliche Einordnung

### Befund

- Keine Angabe zur Rechtsform (Einzelunternehmen, UG, GmbH, …)
- Kein Hinweis auf Gewerbeanmeldung, Kleinunternehmerregelung (§ 19 UStG)
- Auszahlungen in USDT/Krypto, Amazon-Gutschein, PayPal — **unterschiedliche steuerliche und aufsichtsrechtliche Qualifikation**
- `render.yaml` / `.env.example`: Hosting in **Frankfurt (Render)** — Datenstandort teils EU, aber Render Inc. = US-Unternehmen

### Risiko

Ohne klare Betreibergestaltung sind Impressum, Datenschutz-Verantwortlicher, Vertragspartner in AGB und steuerliche Behandlung der Nutzerprämien **nicht schlüssig**.

### Empfehlung

- Rechtsform und wirtschaftlich Berechtigten (GwG) festlegen
- Steuerberater: Einordnung Nutzer-Einnahmen (Lohn/sonstige Einkünfte), eigene Umsätze (Werbeeinnahmen, Affiliate)
- Impressum + Datenschutz + AGB Ziffer 2 synchronisieren

---

## 5. AGB & Vertragsschluss

### Befund

Datei: `frontend/legal/agb.html` — strukturell solide MVP-Vorlage:

**Positiv:**
- Leistungsbeschreibung mit **Einnahmen-Disclaimer** (geringe Beträge, kein Anlageprodukt)
- Auszahlungsbedingungen, Trust Score, Betrugsverdacht, Referral-Regeln
- Haftungsbeschränkung mit Kardinalpflichten-Klausel (mustergültig prüfen lassen)
- Gerichtsstand: Berlin für Kaufleute; Verbraucher gesetzlich ✓
- AGB-Änderungsklausel mit 30-Tage-Widerspruch ✓
- Verweis auf Datenschutzerklärung ✓

**Kritisch:**
- Vertragspartner = Platzhalter („VANTAGE-EARN Betreiber, Deutschland“)
- **Altersgrenze:** AGB Ziffer 1 nennt „mindestens 18 Jahre“ **bzw. 16 mit elterlicher Zustimmung** — Frontend erzwingt **nur 18** (siehe Abschnitt 17)
- Kein **Widerrufsrecht** / Muster-Widerrufsbelehrung für Fernabsatz (§ 312g BGB) — bei **unentgeltlichem** Basisdienst ggf. eingeschränkt; sobald entgeltliche Elemente oder Premium-Features: prüfen
- Keine **Vertragssprache**-Klausel für Mehrsprachigkeit (Locale `de`/`en_US` im Backend)
- Wallet-Guthaben: korrekt als **keine Einlage** deklariert ✓ — aber Krypto-Qualifikation dennoch aufsichtsrechtlich relevant (Abschnitt 18)

### Technische Umsetzung Vertragsschluss

`crates/api-gateway/src/routes.rs`:

```rust
if body.accept_terms != Some(true) { ... }
if body.accept_age_minimum != Some(true) { ... }
```

Frontend: Checkboxen `#accept-terms`, `#accept-age` — **serverseitig durchgesetzt** ✓  
**Fehlend:** Speicherung des Einwilligungszeitpunkts / AGB-Version (Nachweis bei Streit)

### Empfehlung

- AGB-Version (`agb_v2026-06`) in DB bei Registrierung speichern
- Widerrufsbelehrung durch Anwalt ergänzen (falls anwendbar)
- AGB + Impressum + Datenschutz auf gleichen Stand bringen

---

## 6. Widerrufsrecht & Verbraucherinformationen

### Befund

- Keine Widerrufsbelehrung in AGB oder Checkout-Flow
- Auszahlung ist **Antrag**, kein Kauf — Widerruf eher bei **kostenpflichtigen** Leistungen relevant
- B2C-Pflichtinformationen nach BGB-InfoV teilweise über Impressum/AGB angedeutet, aber unvollständig ohne echte Betreiberdaten

### Empfehlung

Anwaltlich klären: Handelt es sich um **gratis Rewards** vs. **entgeltliche Dienstleistung**? Ggf. Muster-Widerrufsformular bereithalten, falls später Paid-Tiers.

---

## 7. DSGVO — Verantwortlicher & Transparenz (Art. 13, 14)

### Befund

Datei: `frontend/legal/datenschutz.html`

**Vorhanden:**
- Verantwortlicher (Platzhalter), Kontakt-E-Mail
- Datenkategorien: Nutzer-ID, JWT, Nutzungsdaten, Trust Score, Ledger
- Rechtsgrundlagen Art. 6 Abs. 1 lit. b, c, f
- Betroffenenrechte Art. 15–21, 77
- Hinweis: keine Passwörter im Klartext ✓ (Argon2 in `auth.rs`)

**Fehlend / unzureichend:**
| Pflichtangabe | Status |
|---------------|--------|
| Ladungsfähige Anschrift Verantwortlicher | ❌ |
| Datenschutzbeauftragter (falls erforderlich) | ❌ nicht erwähnt |
| **Resend** (E-Mail) | ❌ nicht in DSE |
| **BitLabs / CPX** (Offerwall) | ⚠️ nur generisch „AdInPlay/AppLixir“ |
| **OpenAI** (AI Copilot, optional) | ❌ |
| **PostgreSQL / Render** | ⚠️ nur „Render Hosting“ oberflächlich |
| **Google Fonts** | ⚠️ erwähnt, aber ohne Rechtsgrundlage/Einwilligung |
| Drittlandtransfer USA (Render, Resend, BitLabs, OpenAI, Google) | ❌ kein Art. 44 ff. Hinweis |
| Automatisierte Entscheidungsfindung (Trust Score, Fraud) | ❌ Art. 22 nicht adressiert |
| Kategorien Empfänger | unvollständig |

### Empfehlung — Beispielergänzung DSE

```
E-Mail-Versand: Resend, Inc. (USA) — Auftragsverarbeitung gem. Art. 28 DSGVO;
Datenübermittlung auf Basis von EU-Standardvertragsklauseln (SCC).

Offerwall: BitLabs GmbH / entsprechende Partner — bei Nutzung werden User-ID,
Gerätedaten und ggf. IP an den Anbieter übermittelt. Rechtsgrundlage: Art. 6 Abs. 1
lit. a DSGVO (Einwilligung über Consent-Banner).
```

---

## 8. DSGVO — Rechtsgrundlagen & Zweckbindung

### Befund

| Verarbeitung | Deklarierte Basis | Bewertung |
|--------------|-------------------|-----------|
| Konto, Wallet, Auszahlung | Art. 6 Abs. 1 lit. b | ✅ plausibel |
| Betrugsprävention (Trust Score) | Art. 6 Abs. 1 lit. f | ⚠️ Interessenabwägung dokumentieren |
| Technische Logs (IP, User-Agent) | Art. 6 Abs. 1 lit. f | ⚠️ ggf. Einwilligung wenn nicht strictly necessary |
| Werbe-/Offerwall-Tracking | Art. 6 Abs. 1 lit. a erforderlich | ⚠️ DSE nennt teils lit. b/f — **falsch für Marketing-Cookies** |
| Google Fonts extern | lit. f umstritten | 🔴 LG München: Einwilligung empfohlen → **lokal hosten** |

Backend speichert E-Mail + Passwort-Hash (`migrations/014_user_credentials.sql`) — in DSE erwähnt implizit, aber **Passwort-Hash** sollte explizit genannt werden.

---

## 9. TDDDG — Cookies, localStorage & Endeinrichtung (§ 25 TDDDG)

### Befund

**TDDDG § 25:** Speichern/Auslesen von Informationen auf der Endeinrichtung nur mit Einwilligung, außer **unbedingt erforderlich**.

| Mechanismus | Zweck | Einwilligung |
|-------------|-------|--------------|
| `localStorage` `ve_token` | Auth | ⚠️ technisch notwendig — DSE sagt lit. b ✅ |
| `localStorage` `ve_consent_v1` | Consent-Speicher | ✅ |
| `localStorage` `ve_wallet_snapshot` | UX-Cache | ⚠️ nicht zwingend — eher lit. f oder Einwilligung |
| Service Worker Cache | PWA | ⚠️ technisch notwendig für Offline — dokumentieren |
| **AdInPlay / AppLixir / BitLabs Cookies** | Werbung/Tracking | 🔴 bei Live-Betrieb **vor** Laden Einwilligung nötig |

Consent-Banner (`#consent-banner`) ist implementiert, aber:
- Bezeichnet sich als „Cookies“, nutzt aber primär **localStorage** — Text an TDDDG anpassen („Speichern auf Ihrem Gerät“)
- **Kein Blocking** von Drittanbieter-Skripten vor Opt-in nachweisbar (Ad-SDKs, BitLabs-iframe, Google Fonts im `<head>` **ohne Consent-Gate**)
- Google Fonts: `preconnect` zu `fonts.googleapis.com` **lädt sofort** — DSGVO/TDDDG-problematisch

### Empfehlung

1. Google Fonts **self-hosted** (`frontend/assets/fonts/`)
2. Ad/Offerwall-Skripte erst nach `veConsent.analytics === true` laden
3. Consent Management Platform (CMP) mit **IAB TCF 2.x** prüfen, sobald programmatische Werbung live geht

---

## 10. Consent-Management (Frontend-Implementierung)

### Befund

`frontend/index.html`:
- Banner mit „Alle akzeptieren“, „Auswahl speichern“, „Nur Notwendige“ ✅
- `CONSENT_STORAGE_KEY = 've_consent_v1'` — clientseitig only
- `#consent-limited-hint` bei fehlender Analytics-Einwilligung ✅
- Registrierung: separate Checkboxen AGB + Alter ✅

**Schwächen:**
- Keine **Widerrufsmöglichkeit** in der App (Settings → „Einwilligung verwalten“)
- Consent wird **nicht serverseitig** protokolliert (Nachweispflicht Art. 7 Abs. 1 DSGVO)
- „Alle akzeptieren“ als prominenteste Schaltfläche — **EuGH/DSGVO: gleichwertige Ablehnung** erforderlich (Cookie-Banner-Design)
- Briefing `docs/CHATGPT_PROJECT_BRIEFING.md` ist **veraltet** („GDPR Consent-Banner“ als nicht gebaut) — Code hat Banner, Doku driftet

---

## 11. DSGVO — Auftragsverarbeiter & AV-Verträge (Art. 28)

### Befund

| Dienst | Rolle | AV-Vertrag |
|--------|-------|------------|
| Render, Inc. | Hosting/DB | ❌ nicht dokumentiert |
| Resend | E-Mail | ❌ |
| BitLabs | Offerwall | ❌ |
| AdInPlay / AppLixir | Werbenetzwerk | ❌ |
| OpenAI | AI (optional) | ❌ |
| Google (Fonts) | CDN | ❌ |

`.env.example` / `render.yaml`: `RESEND_API_KEY`, `DATABASE_URL` — produktive Verarbeitung personenbezogener Daten **ohne nachweisbare AVV-Kette**.

### Empfehlung

- AVV mit Render, Resend abschließen und in DSE verlinken
- BitLabs Publisher Agreement auf Datenschutz prüfen
- **Verzeichnis von Verarbeitungstätigkeiten** (Art. 30 DSGVO) erstellen

---

## 12. DSGVO — Betroffenenrechte (Art. 15–22)

### Befund

| Recht | Umsetzung |
|-------|-----------|
| Auskunft (Art. 15) | Nur E-Mail-Kontakt — kein Self-Service |
| Berichtigung (Art. 16) | E-Mail über Konto — kein UI |
| Löschung (Art. 17) | ✅ `POST /users/me/delete-account` + Admin-Delete |
| Einschränkung (Art. 18) | ❌ |
| Datenübertragbarkeit (Art. 20) | ❌ kein Export-Endpoint |
| Widerspruch (Art. 21) | ❌ kein Prozess |

`delete_account` in `routes.rs`: Passwort-Pflicht bei E-Mail-Konten ✅; anonyme JWT-Konten ohne Passwort — Löschung möglich?

### Empfehlung

- `GET /users/me/export` (JSON: Profil, Ledger, Payouts)
- Datenschutz-Anfragen SLA (30 Tage) in DSE benennen
- Admin-Export (`/admin/export/users`) — **nur Admin**, nicht Nutzerrecht

---

## 13. DSGVO — Löschkonzept & Speicherdauer

### Befund

DSE Abschnitt 7: vage („nur so lange wie erforderlich“) — **keine konkreten Fristen**

Backend:
- Ledger, Payouts, Audit-Log — **keine automatische TTL**
- `admin_audit_log` — Admin-IPs gespeichert
- Password-Reset-Tokens (`migrations/016_password_reset_tokens.sql`) — Ablauf prüfen (E-Mail: 1 Stunde genannt ✅)

### Empfehlung — Beispielfristen (anwaltlich festlegen)

| Daten | Frist |
|-------|-------|
| Server-Logs | 7–30 Tage |
| Audit-Log | 2–10 Jahre (GwG-Abhängigkeit) |
| Steuerrelevante Ledger | 6–10 Jahre |
| Gelöschte Konten | Anonymisierung innerhalb 30 Tage |

---

## 14. DSGVO — Datensicherheit & TOM (Art. 32)

### Befund

**Positiv:**
- HTTPS (Render)
- Argon2 Passwort-Hashing (`auth.rs`)
- JWT mit `JWT_SECRET` / Production-Exit wenn fehlend
- Rate Limiting (`rate_limit.rs`)
- Security Headers (`middleware/security_headers.rs`): `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`
- Admin: `X-Admin-Secret` Header
- BitLabs: HMAC-SHA1 Callback-Verifikation ✅

**Kritisch:**
- **Admin-Link im Footer** (`/admin`) — öffentlich sichtbar, Angriffsfläche
- JWT Fallback auf `ADMIN_SECRET` (`auth.rs` Zeile 86–90) — **kryptographische Schwächung**
- Kein `Content-Security-Policy` Header
- Kein `Strict-Transport-Security` explizit gesetzt
- E-Mail: Default `onboarding@resend.dev` — nur Test-Domain
- Fraud: `repeated_ip_tracking: "nicht verfügbar"` — IP-Logging für Fraud **nicht implementiert**, aber in DSE als Betrugsprävention deklariert

---

## 15. DSFA & Datenschutzbeauftragter

### Befund

- **DSFA (Art. 35):** Trust Score, automatisierte Fraud-Einstufung, Finanz-Ledger — wahrscheinlich **erforderlich**, nicht durchgeführt
- **DSB (Art. 37):** Pflicht bei Kerntätigkeit „umfangreiche systematische Überwachung“ — **anwaltlich prüfen** (Fraud-Scoring könnte relevant sein)

---

## 16. UWG — Werbung, Marketing & Irreführung

### Befund

| Quelle | Aussage | Problem |
|--------|---------|---------|
| `<meta description>` | „USDT verdienen beim Schauen“ | ⚠️ Ergebnisversprechen |
| Welcome-E-Mail | „echte Belohnungen in USDT“ | ⚠️ ohne Kontext „geringe Beträge“ |
| Onboarding Step 3 | „Ab **10 USDT** Auszahlung“ | 🔴 **Widerspruch** zu 170 € Min (AGB + UI) |
| Hero / Rechner | Disclaimers vorhanden | ✅ teilweise |
| Referral | „verdient beide“ | ⚠️ Bedingungen verlinken |
| `docs/OFFERWALL-PLAN.md` | 0,30–0,80 € Schätzungen | ✅ als Schätzung markiert |

**§ 5 UWG:** Irreführende geschäftliche Handlungen verboten. Widersprüchliche Mindestauszahlungsangaben sind **abmahnfähig**.

### Empfehlung — Beispiel-Fix Onboarding

```
Ab Erreichen des Mindestguthabens (derzeit ca. 170 € Gegenwert in USDT;
im Demo-Modus ggf. niedriger — siehe Wallet) kannst du eine Auszahlung beantragen.
Kein Anspruch auf bestimmte Höhe oder Dauer bis zur Auszahlung.
```

Meta-Description:

```
Kurzvideos & Umfragen — kleine USDT-Belohnungen. Kein garantiertes Einkommen.
```

---

## 17. Jugendschutz & Mindestalter

### Befund

| Dokument | Alter |
|----------|-------|
| AGB | 18, alternativ 16 mit Zustimmung |
| Datenschutz | „nicht unter 16“ |
| Frontend `#accept-age` | **„mindestens 18 Jahre“** |
| BitLabs/Offerwalls | Publisher-AGBs oft **18+** |

**Inkonsistenz** — rechtlich und vertraglich widersprüchlich.

Kein technisches Altersgate (Geburtsdatum), nur Checkbox — **geringe Beweiskraft**.

### Empfehlung

- Einheitlich **18 Jahre** für DE-Launch (Offerwall-Standard)
- DSE auf 18 anpassen oder elterliche Zustimmung technisch implementieren
- Alters-Checkbox serverseitig + Timestamp speichern

---

## 18. BaFin, Krypto & Zahlungsdienstrecht

### Befund

VANTAGE-EARN:
- Führt **internes USDT-Wallet-Ledger** (keine On-Chain-Wallet pro Nutzer in Code sichtbar)
- Auszahlung via `crypto`, `paypal`, `amazon_gift_card` — **keine Zahlungsadresse** im API-Request (`PayoutRequest` nur `amount_usdt`, `payout_method`) — manuelle Abwicklung impliziert
- AGB: „keine Einlage“ — gut, aber **kein Bankguthaben**

**Aufsichtsrechtliche Einordnung (unsicher, anwaltlich klären):**

| Tätigkeit | Mögliche Einordnung |
|-----------|---------------------|
| Verwahrung virtueller USDT-Guthaben | ggf. **Kryptoverwahrung** (KWG) / **MiCA** |
| Auszahlung in Krypto | ggf. **Kryptodienstleistung** |
| PayPal-Auszahlung | ggf. **Geldübermittlung** (ZAG) wenn eigenständig |
| Nur Treuhand/Manuelle Gutscheine | ggf. unreguliert — **Einzelfall** |

**Keine BaFin-Lizenz, kein MiCA-Whitepaper, keine KYC-Pflichten dokumentiert.**

### Risiko 🔴

Produktiver Betrieb mit echten Krypto-Auszahlungen **ohne juristische Freigabe** = erhebliches Aufsichts- und Strafrecht-Risiko.

### Empfehlung

- Anwalt/BaFin-Vorabstimmung oder **Launch nur mit Gutschein/PayPal** über lizenzierte Partner
- Nutzer-Wallet als **Punkte-System** deklarieren bis Lizenz geklärt
- Keine „USDT“-Branding wenn rechtlich nur „Bonuspunkte“

---

## 19. GwG / AML & Betrugsprävention

### Befund

**Positiv:**
- AGB verbieten Geldwäsche, Multi-Accounting, Bots
- Trust Score, Fraud-Admin (`fraud_admin.rs`), Payout-Holds
- BitLabs Callback: Ban-Check, Duplicate-TX

**Kritisch:**
- **Kein KYC** (Identitätsprüfung) vor Auszahlung
- Kein **PEP/Sankions-Screening**
- Keine **Verdachtsmeldung**-Prozesse (§ 43 GwG)
- Schwellenwert 170 € — **GwG-Pflichten** können ab bestimmten Volumina greifen (Einzelfall)
- `repeated_ip_tracking: "nicht verfügbar"` — AML-Signal unvollständig
- Admin manuelle Freigabe — **ohne dokumentierte Prüfcheckliste**

### Empfehlung

- AML-Risikoanalyse durch Compliance-Berater
- Stufe 1: E-Mail-Verifikation; Stufe 2: ID vor erster Krypto-Auszahlung > X €
- GwG-Verdachtsprotokoll intern

---

## 20. Steuerrecht & Nutzerhinweise

### Befund

AGB Ziffer 8: Nutzer müssen steuerliche Pflichten selbst erfüllen ✅ — **ausreichend als Basis**, aber:

- Kein Hinweis in UI bei Auszahlung
- Keine **Freistellungsauftrag**-Thematik (nicht relevant für Plattform)
- USDT-Schwankungen in AGB erwähnt ✅

### Empfehlung

Wallet-UI vor Auszahlung:

```
Einnahmen können steuerpflichtig sein. Bitte wende dich an deinen Steuerberater.
Wir stellen keine Steuerbescheinigungen aus, sofern nicht gesetzlich verpflichtet.
```

---

## 21. E-Mail-Kommunikation & Telemedien

### Befund

Templates (`templates/email/`):

| Template | Impressum | Abmeldelink | DSGVO |
|----------|-----------|-------------|-------|
| registration.html | ❌ | n/a (transaktional) | ⚠️ Marketing-Sprache |
| password_reset.html | ❌ | n/a | ✅ |
| withdrawal_*.html | ❌ | n/a | minimal |
| referral_bonus.html | ❌ | n/a | ⚠️ |
| security_alert.html | ❌ | n/a | ⚠️ |

`email.rs`: Resend API, `SMTP_FROM` Warnung bei `onboarding@resend.dev` ✅

**UWG:** Willkommens-Mail „echte Belohnungen“ — werblicher Charakter, sollte **AGB-Link + Betreiberadresse** enthalten.

### Empfehlung — Footer für alle Mails

```
VANTAGE Earn · [Adresse] · kontakt@… · Datenschutz: https://…/legal/datenschutz
Diese servicebezogene E-Mail wurde dir als registriertem Nutzer zugesandt.
```

---

## 22. Referral-Programm & Pyramidensystem-Risiko

### Befund

AGB Ziffer 7: Referral-Boni, kein Code-Verkauf, Missbrauchssanktionen ✅

**Risiko:**
- Mehrstufiges Referral (nicht im Code gesehen) — aktuell **1 Ebene**, geringes Pyramid-Risiko
- „Verdient beide“-Werbung ohne **konkrete Bonushöhe** und Bedingungen im Banner

### Empfehlung

- Referral-Bonus-Betrag und Qualifikation in UI + AGB synchron
- UWG-konform: „Bis zu X USDT nach qualifizierter Erstaktivität der eingeladenen Person“

---

## 23. Drittanbieter-Integrationen (BitLabs, Ads, AI, Render)

### BitLabs (`bitlabs.rs`, `OFFERWALL-PLAN.md`)

- iframe `web.bitlabs.ai` mit User-UUID — **personenbezogen**
- S2S Callback mit HMAC ✅
- DSE muss BitLabs als **eigenständigen Verantwortlichen oder Auftragsverarbeiter** klären
- Publisher-Vertrag + Datenschutz-Folgenabschätzung

### Werbung (`docs/ADS.md`)

- AdInPlay/AppLixir: **SSV serverseitig „geplant“** — aktuell Client-Callback (`watch/complete`) — **Betrugs-/Compliance-Risiko**
- `ads.txt` Pflicht erwähnt — **nicht im Repo** für Production-Domain

### OpenAI (`ai_chat` in `routes.rs`)

- Nutzer-Nachrichten + Kontext an OpenAI — **nicht in DSE**
- Opt-in oder deaktivieren bis dokumentiert

### Render (`render.yaml`)

- Frankfurt Region ✅
- Free Tier — SLA/Verfügbarkeit für Auszahlungsplattform ⚠️

---

## 24. Gesamtrisiko-Rating

| Bereich | Rating | Begründung |
|---------|--------|------------|
| Impressum / DDG | 🔴 Kritisch | Platzhalter, Abmahnung wahrscheinlich |
| Datenschutz / DSGVO | 🔴 Kritisch | Unvollständige DSE, fehlende AVV, US-Transfer |
| TDDDG / Consent | 🟠 Hoch | Banner da, aber kein Script-Blocking, Google Fonts |
| AGB / Vertrag | 🟠 Hoch | Platzhalter, Alters-Inkonsistenz |
| UWG / Marketing | 🟠 Hoch | Widersprüchliche Auszahlungsangaben |
| BaFin / Krypto | 🔴 Kritisch | Ungeklärte Lizenzpflicht |
| GwG / AML | 🟠 Hoch | Kein KYC trotz Krypto-Auszahlung |
| Jugendschutz | 🟡 Mittel | Checkbox only, 16/18-Widerspruch |
| Betroffenenrechte | 🟡 Mittel | Löschung ok, Export fehlt |
| Technische Sicherheit | 🟡 Mittel | Basis ok, CSP/Admin-Link schwach |
| E-Mail-Compliance | 🟡 Mittel | Transaktional ok, Inhalte werblich |

**Gesamt: 🔴 Nicht launch-ready (Deutschland, B2C, echte Auszahlungen)**

---

## 25. Maßnahmenplan

| ID | Priorität | Befund | Maßnahme | Verantwortlich | Aufwand | Frist |
|----|-----------|--------|----------|----------------|---------|-------|
| M1 | 🔴 | Impressum Platzhalter | Echte Betreiberdaten, DDG/MStV-Texte durch Anwalt | Geschäftsführung + Anwalt | 1–2 Wochen | Vor Launch |
| M2 | 🔴 | DSE unvollständig | Art. 13/14 DSE: alle Prozessoren, SCC, Trust Score | Anwalt + Dev | 2 Wochen | Vor Launch |
| M3 | 🔴 | BaFin-Risiko Krypto | Rechtsgutachten; ggf. nur Gutschein-Launch | Anwalt/BaFin | 4–8 Wochen | Vor echten Auszahlungen |
| M4 | 🔴 | Google Fonts extern | Self-hosting Fonts | Dev | 2 h | Sofort |
| M5 | 🟠 | Consent ohne Script-Block | Gate Ad/Offerwall/Analytics hinter Opt-in | Dev | 1–2 Tage | Vor Live-Ads |
| M6 | 🟠 | Consent-Nachweis | Server: `consent_version`, Timestamp in DB | Dev | 1 Tag | Vor Launch |
| M7 | 🟠 | Onboarding „10 USDT“ | Text auf 170 € / Demo-Modus korrigieren | Dev | 15 min | Sofort |
| M8 | 🟠 | Meta/E-Mail UWG | Disclaimers, keine „echte“-Versprechen ohne Kontext | Marketing + Anwalt | 1 Tag | Vor Launch |
| M9 | 🟠 | Alters-Inkonsistenz | Einheitlich 18 in AGB, DSE, UI | Anwalt + Dev | 2 h | Vor Launch |
| M10 | 🟠 | AV-Verträge | Render, Resend, BitLabs AVV abschließen | Ops | 1–2 Wochen | Vor Launch |
| M11 | 🟠 | GwG/AML | KYC-Stufenkonzept, Checkliste Admin-Payout | Compliance | 2–4 Wochen | Vor Krypto-Payout |
| M12 | 🟡 | Art. 20 Export | `GET /users/me/export` JSON | Dev | 1 Tag | Post-Launch |
| M13 | 🟡 | AGB-Version speichern | Bei Registrierung `terms_version` | Dev | 4 h | Vor Launch |
| M14 | 🟡 | Admin-Link Footer | `/admin` aus Footer entfernen oder IP-Whitelist | Dev | 1 h | Sofort |
| M15 | 🟡 | CSP / HSTS | Security-Header erweitern | Dev | 4 h | Vor Launch |
| M16 | 🟡 | E-Mail-Footer | Impressum + Datenschutz-Link in Templates | Dev | 2 h | Vor Launch |
| M17 | 🟡 | ads.txt | Für Production-Domain bereitstellen | Ops | 1 h | Mit Live-Ads |
| M18 | 🟡 | OpenAI DSE | AI deaktivieren oder DSE + AVV ergänzen | Dev + Anwalt | 1 Tag | Vor AI-Launch |
| M19 | 🟡 | DSFA Trust Score | Datenschutz-Folgenabschätzung | DSB/Anwalt | 2 Wochen | Vor Scale |
| M20 | 🟢 | JWT != ADMIN_SECRET | Separates Secret erzwingen | Dev | 1 h | Sofort |

---

## 26. Legal Readiness Score

| Kategorie | Gewicht | Score (0–10) | Gewichtet |
|-----------|---------|--------------|-----------|
| Impressum / Anbieterkennzeichnung | 15 % | 1 | 0,15 |
| Datenschutz / DSGVO | 20 % | 3 | 0,60 |
| TDDDG / Consent | 10 % | 4 | 0,40 |
| AGB / Verbraucherrecht | 10 % | 5 | 0,50 |
| UWG / Marketing | 10 % | 4 | 0,40 |
| BaFin / Krypto / Zahlungsrecht | 15 % | 1 | 0,15 |
| GwG / AML | 10 % | 3 | 0,30 |
| Betroffenenrechte / Prozesse | 5 % | 5 | 0,25 |
| Technische Sicherheit | 5 % | 6 | 0,30 |
| **Summe** | 100 % | — | **3,05 / 10** |

### **Legal Readiness: 28 %** (gerundet)

**Interpretation:**
- **0–30 %:** Testbetrieb / Closed Beta — **kein öffentlicher Launch**
- **31–60 %:** Soft Launch mit eingeschränkten Features (kein Krypto, kein Live-Tracking)
- **61–80 %:** Launch mit anwaltlicher Freigabe und offenen Nachbesserungen
- **81–100 %:** Launch-ready

**Empfehlung:** VANTAGE-EARN bleibt im **internen/Testbetrieb**, bis **M1–M10** abgeschlossen und **schriftliche anwaltliche Launch-Freigabe** vorliegt.

---

## Anhang A — Geprüfte Dateien

- `frontend/legal/impressum.html`, `datenschutz.html`, `agb.html`
- `frontend/index.html` (Consent, Registrierung, Disclaimers, Onboarding)
- `templates/email/*`
- `crates/api-gateway/src/auth.rs`, `email.rs`, `bitlabs.rs`, `admin.rs`, `fraud_admin.rs`, `routes.rs`, `middleware/*`
- `render.yaml`, `.env.example`
- `docs/*` (DEPLOY, ADS, OFFERWALL-PLAN, CHATGPT_PROJECT_BRIEFING)

## Anhang B — Relevante Rechtsnormen (Auswahl)

| Norm | Relevanz |
|------|----------|
| DSGVO (EU) 2016/679 | Datenschutz |
| TDDDG | Cookies, Endeinrichtung |
| DDG | Impressum, Telemedien |
| UWG | Werbung, Irreführung |
| BGB §§ 305 ff., 312 ff. | AGB, Fernabsatz |
| GwG | Geldwäsche |
| KWG / MiCA / ZAG | Krypto, Zahlungsdienste (Einzelfall) |
| MStV | Medienrechtliche Verantwortlichkeit |
| TTDSG → TDDDG | Übergang Cookie-Recht DE |

---

*Erstellt durch automatisiertes Compliance-Audit am 28.06.2026. Vor Veröffentlichung und Launch durch einen in Deutschland zugelassenen Rechtsanwalt prüfen lassen.*
