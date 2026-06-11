# Zukünftig implementieren

Status-Legende: ✅ fertig · 🟡 teilweise · ⬜ offen

---

- ⬜ **Daten in einer Datenbank im AppData speichern**
  Aktuell: JSON-Dateien in `%APPDATA%` (`prompts.json`, `settings.json`).
  Noch keine echte DB.

- 🟡 **Ansichtsverwaltung (oben + Button, Namens-Popup, Rechtsklick zum Bearbeiten/Löschen)**
  Vorhanden: Ansichten anlegen/umbenennen/löschen mit doppelter Lösch-Abfrage —
  jedoch nur im **Einstellungen → Ansichten-Editor** (`add_view` / `rename_view` /
  `delete_view`, `renderViewsEditor`).
  Offen: „+"-Button oben in der Ansichtsleiste, Namens-Popup beim Anlegen,
  Rechtsklick auf die oberen Ansichts-Buttons öffnet das Namens-Popup
  (Bearbeiten + Löschen mit doppelter Abfrage).

- ⬜ **Screenshot-Feature (eigenes Snipping-Tool)**
  Ausgegrauter Vollbild-Overlay, Fenster anklicken oder Bereich markieren,
  Screenshot in Zwischenablage + Speichern im Standard-Screenshot-Ordner,
  Popup „Als Button speichern? (Ja/Nein)". Button links neben der Büroklammer
  mit passendem Icon. Modern, flüssig, intuitiv.

- ⬜ **Logo schöner machen**

- ⬜ **.pdf unterstützen (erste Seite als Bild)**

- ⬜ **Unterstützung aller gängigen Bild- und Videoformate**
  Aktuell feste Endungen (`IMAGE_EXT` / `GIF_EXT` / `VIDEO_EXT` in `media.js`).

- ⬜ **Experten-Menü in den Einstellungen**
  Zweite Einstellungsseite (zurück-Pfeil oben links, X oben rechts bleibt),
  Warnhinweis, alle Parameter/Schalter zentral. Standardmäßig alles aktiv,
  hier abwählbar: Datei-Unterstützung, Multi-Ansicht, Verstecken-Icons
  oben/unten, Tile-Kontextmenü, Hover-Highlight, Schnell-Grid im Hauptlayout,
  Maximalanzahlen, u. v. m.

- ✅ **Startup-Fenstergröße / DPI- & Multi-Monitor-Bug behoben**
  `resolve_geometry` (`lib.rs`): Größe in logischen Pixeln gespeichert,
  hart auf die Primärmonitor-Größe gedeckelt, Re-Zentrierung wenn der
  ursprüngliche Monitor fehlt. Kein Aufwachsen mehr pro Start.

- ⬜ **Update-Changelog-Popup mit drei Buttons**
  Vor der Installation Popup mit Changelog der neuen Version (Fallback-Text
  „kein Changelog"); Buttons Abbrechen / Version überspringen (interne
  Blacklist + Warnhinweis bei manueller Prüfung) / Installieren (silent);
  X oben rechts und Klick auf Hintergrund schließen.

- ⬜ **Einstellung „Immer im Vordergrund"**
  Hauptfenster optional always-on-top (außer minimiert). Aktuell nur die
  Floating-Buttons sind fest always-on-top (`lib.rs:579`).
