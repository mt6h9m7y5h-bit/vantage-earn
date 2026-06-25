# Cursor-Prompt: Nächste Woche UGC-Skripte generieren

Kopiere alles zwischen den `---` Linien in einen neuen Cursor-Chat (Agent-Modus). Passe die Platzhalter in `[KLAMMERN]` an.

---

```
Workspace: vantage-earn

Erstelle 5 TikTok-Skripte für die Content-Marketing-Serie „7-Tage VANTAGE Challenge“.

## Kontext
- App-Demo: https://vantage-earn.onrender.com/demo
- Zielgruppe: DE TikTok, Side-Hustle / Earn-Apps, ehrliches UGC
- Ordner: content/WOCHE-[NUMMER]/skripte.md (neu anlegen wenn nicht vorhanden)
- Bestehende Regeln: content/README.md und content/ANLEITUNG.md lesen und einhalten

## Diese Woche
- Woche: [z.B. 2]
- Serien-Winkel: [z.B. „Morgens 1 Video — Routine-Test“ oder „Vergleich Tag 1 vs Tag 7“]
- Optionale Schwerpunkte: [z.B. PWA installieren, Wallet-Auszahlung erklären, Streak verlieren & zurückkommen]

## Pflicht-Format pro Video (Markdown)

### Video N — [Titel]
#### Hook (erste 2 Sek.)
#### Story (15–25 Sek.)
#### B-Roll (echte App: register, watch, analytics, wallet)
#### Text Overlay
#### Caption (DE, bescheidene Hashtags)
#### CTA (Link in Bio)

## Videos (5 Stück)
1. Tag 0 — Challenge-Start / neuer Hook-Winkel
2. Tag 1 — erstes Guthaben (realistisch 0,001–0,01 USDT)
3. Tag 3 — Analytics oder Streak
4. Tag 5 — Referral oder Feature [THEMA]
5. Tag 7 — ehrliches Fazit (7-Tage-Gesamt 0,05–0,15 USDT)

## UGC-Regeln (strikt)
- KEINE Fake-Earnings, KEIN Lambo/Luxus-Hype
- Nur realistische USDT-Beträge
- Nur echte App-Screens beschreiben
- Kein Scam-Versprechen, kein „passives Einkommen garantiert“
- Side-Hustle-Framing, transparent

## Output
- Eine Datei: content/WOCHE-[NUMMER]/skripte.md
- Am Ende: Dreh-Checkliste wie in WOCHE-1
- Keine npm-Dependencies, keine Automation
- Sprache: Deutsch

Referenz-Stil: content/WOCHE-1/skripte.md
```

---

## Platzhalter-Schnellreferenz

| Platzhalter | Beispiel |
|-------------|----------|
| `[NUMMER]` | `2` |
| Serien-Winkel | „Studenten-Budget“, „nur abends 5 Min“ |
| Tag-5-Thema | Wallet, PWA-Install, Auszahlungsgrenze |

## Nach der Generierung

1. Skripte einmal in der echten App durchspielen — Zahlen anpassen
2. `ANLEITUNG.md` Workflow folgen
3. Commit: `Add week [N] UGC content scripts`
