# Abhängigkeiten — Bestandsaufnahme

Stand: Runde 2 zu 2.7.0. Zweck: entscheiden, was bleibt, was durch eigenen Code
ersetzt werden kann und was ausdrücklich nie angefasst wird. **Nichts davon ist
umgesetzt** — Umsetzung nur nach Bens Freigabe.

Bewertung je Eintrag: *Aufwand* (niedrig/mittel/hoch) heißt Arbeitsaufwand inklusive
Tests, *Token* ist die grobe Schätzung für eine Umsetzung in diesem Repo, *Risiko*
meint, was kaputtgehen kann, wenn die Eigenlösung nicht exakt gleich funktioniert.

## Nicht antasten (Bens Vorgabe + eigene Einschätzung)

| Crate | Wofür | Warum bleibt |
|---|---|---|
| `tauri` (+ `single-instance`, `global-shortcut`) | Fenster, IPC, Tray, Autostart-Hooks, Bundling | Das ist das Framework. Kein Ersatz denkbar. |
| `rusqlite` (bundled) | Der komplette Speicher | Eigene Speicherengine wäre ein zweites Projekt. |
| `pdfium-render` | Erste Seite einer PDF als Vorschau | PDF-Rendering von Hand ist unrealistisch. |
| `xcap` | Bildschirmaufnahme für Snip + Pipette | Multi-Monitor-DPI-Fallstricke, die dort schon gelöst sind. |
| `aes-gcm`, `argon2`, `rand`, `sha2` | Backup-Verschlüsselung, Passwort-KDF, Update-Prüfsumme | Krypto schreibt man nicht selbst. Ende der Diskussion. |
| `serde`, `serde_json` | Jede Struktur, die auf Platte oder durch die IPC geht | Faktischer Standard, Ersatz wäre reine Selbstbeschädigung. |

## Ersetzbar — Bericht, keine Umsetzung

| Crate | Wofür genutzt | Umfang | Aufwand | Token | Risiko |
|---|---|---|---|---|---|
| `sys-locale` | Genau ein Aufruf: Systemsprache beim ersten Start | 1 Zeile | niedrig | ~3k | gering — `GetUserDefaultLocaleName` über eine kleine `extern "system"`-Deklaration. Zieht aber die Frage nach sich, ob man dafür `windows-sys` als Abhängigkeit will; dann ist nichts gewonnen. |
| `winreg` | Autostart-Schlüssel schreiben/lesen/löschen | 3 Funktionen | mittel | ~15k | mittel — Registry-Handles von Hand sind fehleranfällig, und ein Fehler heißt „Autostart kaputt", was niemand sofort merkt. |
| `flate2` | Deflate für `.psb`, gzip für Snapshots | 4 Funktionen | hoch | — | **Nicht ersetzen.** Ein eigener Deflate müsste bestehende Backups bitgenau lesen können. |
| `rayon` | Eine parallele Schleife: BGRA→RGBA einer Mehrmonitor-Aufnahme | 1 Stelle | niedrig | ~8k | gering — `std::thread::scope` mit Chunks tut dasselbe. Spart eine Abhängigkeit samt Work-Stealing-Laufzeit; kostet etwas Lesbarkeit. Der einzige Kandidat, bei dem sich der Tausch wirklich rechnet. |
| `jpeg-encoder` | Snip-Vorschau als JPEG | 1 Stelle | hoch | — | **Nicht ersetzen.** `image` kann JPEG auch, aber deutlich langsamer — genau deshalb steht dieses Crate hier. |
| `image` | Dekodieren angehängter Bilder, Icon-Aufbereitung | breit | hoch | — | **Nicht ersetzen.** Acht Formate, jedes mit eigenem Dekoder. |
| `arboard` / `clipboard-win` | Zwischenablage lesen/schreiben (Text, Bild, Datei) | breit | mittel–hoch | ~40k | mittel — `clipboard-win` deckt Windows allein ab; `arboard` ist die Bildseite. Eine Zusammenlegung auf `clipboard-win` wäre denkbar, spart aber nur eine Abhängigkeit und riskiert Regressionen beim Bild-Clipboard. |
| `rfd` | Datei-/Ordnerdialoge und die zwei Dialoge vor dem Webview | mittel | mittel | ~25k | mittel — `IFileOpenDialog` direkt ist machbar, aber COM-Handhabung von Hand. Der Gewinn ist klein. |
| `ureq` | Update-Prüfung, Installer- und Hash-Download | 4 Stellen | hoch | — | **Nicht ersetzen.** Das wäre eine eigene TLS-Anbindung. |
| `tokio` (nur `time`) | Ein Timer, der die Snip-Overlay-Geometrie nach dem DPI-Settle erneut setzt | 1 Stelle | niedrig | ~5k | gering — ein `std::thread::spawn` mit `sleep` plus `app.run_on_main_thread` erledigt dasselbe. Tauri zieht `tokio` allerdings ohnehin herein, es fällt also keine Abhängigkeit weg. |

## Empfehlung

Genau **ein** Tausch lohnt sich unabhängig: `rayon` → `std::thread::scope`. Alles
andere spart entweder nichts (die Abhängigkeit kommt über Tauri sowieso mit) oder
tauscht eine erprobte Bibliothek gegen selbstgeschriebenen Windows-API-Code an einer
Stelle, an der ein Fehler still bleibt.

Die Lizenzen sind durchweg MIT/Apache-2.0 und damit für eine geschlossene Weitergabe
unproblematisch.

## Sicherheitsprüfung (`cargo audit`)

- **Behoben:** `quick-xml` 0.39.4 (RUSTSEC-2026-0194/0195, je 7.5 hoch — quadratische
  Laufzeit bzw. unbegrenzte Allokation beim XML-Parsen) kam über `plist` aus
  `tauri-utils`. `plist` 1.9.0 → 1.10.0 zieht `quick-xml` 0.41.0 nach; Tests grün.
- **Bleibt gemeldet, betrifft uns nicht:** zwei weitere `quick-xml`-Kopien hängen an
  `wayland-scanner` und `xcb` — beides Linux-Pfade, die in einem Windows-Build nie
  kompiliert werden. Dasselbe gilt für die GTK3-Warnungen („no longer maintained")
  und `glib`.
- Übrige Warnungen sind „unmaintained"/„unsound"-Hinweise ohne Angriffspfad in
  diesem Programm; keine davon liegt in einem Pfad, der fremde Daten verarbeitet.
