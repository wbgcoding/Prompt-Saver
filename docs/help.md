# Help

The same content shown in the app's own **Help** menu (Settings → Help), kept here so
it's readable without installing anything. English and German only — the in-app
version follows the same rule and falls back to English in every other language.

## Basics

### Getting started

A tile is one saved prompt. Double-click an empty grid cell (or the + button) to
create one, click any tile to copy it. Three example tiles (an email with
placeholders, an image, and a Markdown template) ship on first launch — a button
above the grid removes them with one click once you don't need them anymore.

### Creating prompts

Beyond plain text, a prompt can carry an attached file, a screenshot, or a picked
screen colour.

- File attach — clicking the tile copies the file itself, not text
- Screenshot tool — snip a region or a whole window straight into a new prompt
- Colour picker — pick any on-screen pixel, copies as hex/RGB/HSL
- Drag & drop — drop a file or image straight onto the grid

### Variables & placeholders

Write `{#{name}#}` anywhere in a prompt's text — copying it opens a small popup
asking for each placeholder's value before the filled-in text reaches the clipboard.

- Dynamic placeholders — `{#{date}#}`, `{#{time}#}`, `{#{clipboard}#}` fill
  themselves in, no popup
- Choice placeholders — a dropdown of preset values instead of free text
- Remember choices — the popup pre-fills your last answer per prompt
- Popup in its own window — useful with the floating buttons, where the main
  window may be hidden

### The editor

The prompt editor is a plain text box by default; a few opt-in features
(**Markdown preview**, **private notes** — both off by default) add structure and
history to it: rendered Markdown preview with syntax-highlighted code blocks, full
version history, and a private note field never copied or exported with the text.

### The grid

The main window's grid is a free-placement board of prompt tiles — up to 20×20
cells by default (adjustable to 200×200 in the expert menu), with its own layout
per view. Drag a tile to any empty cell, or drop it on an occupied one to swap
the two; the grid never rearranges itself on its own.

### Screenshot tool

Captures part or all of your screen straight into a new copy button. Drag a
region, or click a window to capture it whole — even a normally-protected one
like the Task Manager. The app's own windows are hidden for the moment of
capture so they never end up in the shot; the confirmation dialog then lets you
save it as a button, save it as a file, or just leave it on the clipboard.

### Colour picker & pick history

A pipette that freezes the screen, magnifies the pixel under
your cursor, and copies it as hex, rgb or hsl. Right-click the pipette icon to
reopen your last few picks — the history is mirrored into settings, so it
survives a restart too.

### Rotating prompts

A prompt can hold a list of texts instead of one — each copy serves the next
entry, in order or shuffled without repeats, instead of always the same text.
On by default; the per-prompt switch sits under Advanced options in the editor.

- A small badge counts up (like "2/5") on a tile, floating pill or pill-folder
  member — a grid-folder member shows only its file-type icon instead. In order
  mode it is the absolute position; in shuffle mode it counts draws within the
  current cycle and restarts at 1 once every entry has been shown once, rather
  than jumping to an arbitrary position
- The count survives restarts and full backups; a CSV/TXT import starts a
  fresh pointer, since the imported prompt gets a new id

## Copying

### Copying

- Prompt chaining — click several tiles in a row, they combine into one clipboard copy
- Per-prompt hotkeys — copy a specific prompt from anywhere, without opening the app
- Drag-out — hold the modifier key and drag a tile onto another window to paste
  directly, no clipboard round-trip. An expert switch swaps the two gestures, so a
  plain drag leaves the app and the key moves the tile instead.
- Auto-paste — after copying, the app switches to your last window and presses
  paste for you

### Hotkeys

Two independent kinds: one global combination that opens the quick launcher
(default Ctrl+Alt+P), and a combination per prompt that copies — and optionally
pastes — that one prompt directly. A combination must include Ctrl or Alt; the
app hotkey always wins a conflict, and among prompts the first one in the list
keeps a combination two prompts happen to share. Importing a backup that carries
a colliding hotkey drops the losing one instead of failing the import.

"All hotkeys", next to the global hotkey field in Settings, lists every hotkey —
global and per-prompt — in one searchable, live-rebindable list, so rebinding or
clearing one no longer means opening that prompt's editor.

### Clipboard watcher

Off by default. When switched on, everything you copy anywhere on the system (not
just from this app) is logged into an inbox you can review and turn into prompts
later — a minimum-length filter and an inbox cap keep it from growing unbounded.

### Copy history

A log of what you've copied and when, separate from the clipboard watcher (this
only records copies made through the app). Grouped view, automatic retention,
per-entry delete, and usage statistics including a 14-day activity chart.

## Organising

### Library & search

The library lists every prompt as a searchable, filterable list — fuzzy search,
smart sort (most-used / most-recent), and type/colour filters.

### Tags

Free-form labels assigned to prompts; filter the library or the grid down to a set
of tags, matching ANY or ALL of them.

### Favourites

Star a prompt to mark it as a favourite; an optional dedicated view lists only
starred prompts.

### Standalone: Multiple views

A view is one grid page with its own size and arrangement; a prompt can live in
several views. View groups turn views into tabs instead of a dropdown.

### Tile behaviour & appearance

Tile menu, drag-to-reorder, hover lift, type badges, shadows, gradients, rounded
corners, bold/uppercase/monospace text — behaviour and pure looks, all adjustable.

### Media

Video controls, autoplay, mute, loop, plus grayscale/dim-until-hovered looks for
privacy-leaning media tiles.

## Always on top

### Floating buttons & folders

Floating buttons are small always-on-top copy buttons that sit outside the main
window, over whatever app you're using. Pill folders and grid folders (both on by
default) merge several of them, or several grid tiles, into one expandable group
the same way — a tile can only ever belong to one grid folder at a time.

### Tray

The system tray icon's right-click menu offers a "recently copied" and a
"favourites" section for quick access without opening the main window.

### Standalone: Quick launcher

A small always-on-top popup (default hotkey Ctrl+Alt+P) with fuzzy search across
every prompt's name and text — pick one and it copies immediately, no need to bring
the main window forward.

## Look and feel

### Appearance & themes

Eight built-in themes: system, light, dark, Programmer, Neon Violet, Ocean,
Cyberpunk and Black & White. The theme editor lets you tweak any theme's colours
and save the result as your own custom theme.

### Standalone: Theme editor

Start from any built-in theme, change individual colours, and save the result as
your own theme — the original stays untouched.

### Scaling & zoom

Independent size sliders for the overall UI, dialogs, the composer, and icons.

### Tooltips

Hovering a tile shows its full text, length, and (if present) its private note.
Delay, auto-hide timeout, and width are all adjustable.

### Toolbar

Which buttons the header and composer bar show — quick grid-size control,
library/history/pin buttons, multiple views and view groups.

## System and data

### Privacy

Exclude the window from screen capture (off by default), blur tiles/media until
hovered, hide tile names, dim unhovered tiles.

### System

Update checks (never installs without asking), keyboard navigation, allowing
multiple instances at once, and where the data folder lives.

### Data maintenance

Duplicate finder, trash (with its own retention setting), CSV/TXT export, and full
backup export/import — reached from the expert menu's Data group.

### Trash & restoring prompts

A deleted prompt goes to the trash first, not straight to permanent deletion.
Retention is in days (0 means keep forever). Restoring places the prompt in the
active view's first free cell — not necessarily its original view or cell — and
does not bring back a floating button, folder membership or hotkey it had before
deletion; those need setting up again.

### Duplicate finder

Flags prompts that are textually very similar, by a similarity score rather than
an exact match, without deleting anything on its own. Review a flagged pair side
by side, keep one, or mark the pair as not a duplicate to stop it being flagged
again. A rotating prompt is only compared on its first entry.

### Import & export

Plain CSV/TXT export for the text alone, or a full backup file with everything —
prompts, views, layouts, colours, styles and language. A CSV/TXT import shows a
preview first — every incoming prompt's name and whether it would land as a
duplicate — before anything is written; a full backup restores directly.

### Markdown folder sync (beta)

An optional, off-by-default mirror that writes every prompt as its own
readable `.md` file in a folder you pick, alongside the encrypted database —
including which view, grid cell and folder it sits in. A view or folder named in
an incoming file that does not exist yet on this machine is created
automatically, matched by name.
**The `.md` files are plain text** — unlike the database, anyone with access to
that folder can read the prompts, so avoid a synced or shared folder for anything
sensitive. A prompt with an attached file, image or PDF syncs as a reference: the
header carries the path and the PDF page, never the file itself. A screenshot
pasted straight into a prompt has no path and stays in the database only.

### The expert menu

Every optional feature lives behind an on/off switch, with its own parameters
right underneath once it's on. The search box scans every tab at once; a feature
that ships switched off carries a small dot badge. Reset works at three levels:
one value's own undo icon, a whole group's reset, or everything at once.

### Keyboard-only use

Arrow keys move a focus ring across the grid; Enter or Space copies
whichever tile is focused. The ring fades out a short while after the window
loses focus.

### Where your data lives

Everything lives in one local SQLite database (plus its small WAL file).
Installed: under AppData, shown and changeable in the expert menu. Portable:
right next to the exe, so the whole thing stays on a USB stick or synced folder
as one unit. Moving between the two is detected automatically and offered as a
one-time migration.

### Updates & security

On by default, and the only network traffic the app makes on its own — the one
exception is a first-run download of the Microsoft WebView2 installer if that
runtime is missing. A downloaded update is verified against a published checksum
before it runs; a missing or mismatching checksum blocks the install instead of
running an unverified file.

### Common questions

- *My screenshot of this app came out empty* — the screen-capture exclusion
  privacy setting is working as intended; turn it off in the expert menu if you
  don't want that
- *The app won't start* — it needs the Microsoft WebView2 runtime; the installer
  offers it automatically if missing
- *I can't find a setting* — check the expert menu's search box first; it scans
  every tab at once
- *A hotkey doesn't do anything* — another app may already hold that combination;
  the editor explains this instead of accepting it silently

### Standalone: Backup manager

Encrypted, compressed snapshots of your entire prompt library and settings, on an
automatic schedule with smart GFS-style retention tiers, and a restore that can
target data only, settings only, or both. The same window also has a
usage-statistics section (totals, a 14-day activity chart, most-used prompts),
unrelated to the backups themselves.

---

## Hilfe (Deutsch)

Derselbe Inhalt wie oben, wie er auch im Hilfe-Menü der App steht (Einstellungen →
Hilfe). Nur Englisch und Deutsch — jede andere Sprache in der App fällt auf
Englisch zurück.

**Erste Schritte** — Eine Kachel ist ein gespeicherter Prompt. Doppelklick auf eine
leere Rasterzelle (oder der +-Knopf) legt eine an, ein Klick kopiert sie. Drei
Beispielkacheln (E-Mail mit Platzhaltern, Bild, Markdown-Vorlage) sind beim ersten
Start dabei — ein Knopf über dem Raster entfernt sie mit einem Klick.

**Prompts anlegen** — Datei-Anhang, Screenshot-Werkzeug, Farbpipette (aus),
Ziehen & Ablegen.

**Variablen & Platzhalter** — `{#{Name}#}` im Text öffnet beim Kopieren ein Popup
für jeden Wert. Dynamische Platzhalter (`{#{date}#}`, `{#{time}#}`,
`{#{clipboard}#}`) füllen sich selbst.

**Der Editor** — Markdown-Vorschau, Syntax-Hervorhebung, Versionsverlauf, private
Notizen (Markdown-Vorschau und Notizen standardmäßig aus).

**Das Raster** — freie Platzierung, standardmäßig bis 20×20 Zellen (im
Experten-Menü bis 200×200 einstellbar), eigenes Layout je Ansicht.

**Screenshot-Werkzeug** — Bereich oder ganzes Fenster (auch geschützte wie den
Task-Manager) direkt zu einem neuen Kopierknopf.

**Farbpipette & Auswahlverlauf** — Pixel als Hex/RGB/HSL kopieren; die letzten
Auswahlen überstehen einen Neustart. Standardmäßig aus.

**Wechselnde Prompts** — ein Prompt trägt eine Liste von Texten, jede Kopie
liefert den nächsten Eintrag (Reihenfolge oder Zufall). Standardmäßig an; der
Schalter pro Prompt sitzt im Editor unter Erweiterte Optionen. Abzeichen auf
Kachel/Pille/Pillen-Ordner-Mitglied zählt hoch — im Zufalls-Modus die Ziehungen
im aktuellen Durchlauf statt einer springenden Position; übersteht Neustart und
Sicherung, aber keinen CSV/TXT-Import (neuer Prompt, neuer Zeiger).

**Kopieren** — Verketten, Prompt-Hotkeys, Herausziehen (mit oder, per Schalter,
ohne Modifikatortaste), automatisches Einfügen.

**Hotkeys** — ein globaler Hotkey öffnet den Schnellstarter (Standard
Strg+Alt+P), ein eigener Hotkey pro Prompt kopiert (und fügt optional ein). Bei
einer Kollision gewinnt der App-Hotkey, sonst der erste Prompt in der Liste.
"Alle Hotkeys" neben dem globalen Feld listet jeden Hotkey durchsuchbar auf und
belegt jede Zeile direkt um, ohne den jeweiligen Prompt-Editor zu öffnen.

**Schwebende Buttons & Ordner** — Pillen-Ordner und Raster-Ordner (beide aus);
eine Kachel gehört immer nur zu einem Raster-Ordner gleichzeitig.

**Tray** — Zuletzt kopiert, Favoriten.

**Zwischenablage-Wächter** — aus, protokolliert bei Bedarf alles Kopierte im System.

**Kachelverhalten & Aussehen** — Menü, Umsortieren, Hover-Anhebung, Optik.

**Tooltips** — Verzögerung, Ausblenden, Breite.

**Favoriten** — Stern-Markierung, eigene Favoriten-Ansicht.

**Skalierung & Zoom** — UI-, Dialog-, Composer- und Icon-Größe getrennt einstellbar.

**Aussehen & Themes** — acht mitgelieferte Themes, optionaler Theme-Editor.

**Bibliothek & Suche** — unscharfe Suche, intelligente Sortierung, Filter.

**Tags** — freie Schlagwörter, UND/ODER-Filter.

**Kopierverlauf** — Protokoll, Gruppierung, Aufbewahrung, Einzeleintrag löschen,
Statistik mit 14-Tage-Diagramm.

**Medien** — Video-Steuerung, Graustufen/Abdunkeln.

**Symbolleiste** — welche Knöpfe Kopf-/Composer-Leiste zeigen.

**Datenschutz** — Bildschirmaufnahme-Ausschluss (aus), Weichzeichnen, Namen
verbergen, Abdunkeln.

**System** — Updates, Tastatur-Navigation, mehrere Instanzen, Datenordner.

**Datenpflege** — Duplikatfinder, Papierkorb, Import/Export.

**Papierkorb & Wiederherstellen** — gelöschte Prompts landen zuerst im
Papierkorb (Aufbewahrung in Tagen, 0 = für immer). Wiederherstellen legt in die
erste freie Zelle der aktiven Ansicht, nicht zwingend an den Originalplatz, und
ohne Pille, Ordner-Mitgliedschaft oder Hotkey von vorher.

**Duplikatfinder** — findet textlich sehr ähnliche Prompts nach
Ähnlichkeitswert, keine exakte Übereinstimmung nötig, löscht nichts von sich
aus. Ein wechselnder Prompt wird nur nach seinem ersten Eintrag verglichen.

**Import & Export** — CSV/TXT nur für den reinen Text, volle Sicherung für
alles. Ein CSV/TXT-Import zeigt vorher eine Vorschau mit Name und
Duplikat-Kennzeichnung; eine volle Sicherung stellt direkt wieder her.

**Markdown-Ordner-Sync (Beta)** — spiegelt jeden Prompt als lesbare
`.md`-Datei in einem gewählten Ordner, inklusive Ansicht, Rasterzelle und
Ordner. Angehängte Dateien, Bilder und PDFs wandern als Verweis mit (Pfad und
PDF-Seite im Kopf, nie die Datei selbst); ein direkt eingefügter Screenshot hat
keinen Pfad und bleibt nur in der Datenbank. Eine in einer eingehenden Datei genannte Ansicht oder ein Ordner, den
es auf diesem Rechner noch nicht gibt, wird automatisch angelegt, per
Namensabgleich. **Die Dateien sind Klartext** — anders als die Datenbank, also
kein synchronisierter oder geteilter Ordner für Sensibles.

**Das Experten-Menü** — jede Funktion hat einen Schalter mit ihren Parametern
direkt darunter. Die Suche durchsucht alle Tabs auf einmal; ausgeschaltete
Funktionen tragen ein kleines Punkt-Abzeichen. Zurücksetzen auf drei Ebenen:
ein Wert, eine ganze Gruppe, oder alles auf einmal.

**Tastaturbedienung** — Pfeiltasten bewegen einen Fokusring über das Raster
(aus), Enter oder Leertaste kopiert die fokussierte Kachel.

**Wo die Daten liegen** — eine lokale SQLite-Datenbank (plus ihre kleine
WAL-Datei). Installiert: unter AppData, im Experten-Menü einsehbar und
änderbar. Portabel: direkt neben der exe. Der Wechsel zwischen beiden wird
erkannt und als einmalige Migration angeboten.

**Updates & Sicherheit** — standardmäßig an, einzige eigene
Netzwerkaktivität außer einem einmaligen WebView2-Installer-Download, falls
die Laufzeit fehlt. Ein heruntergeladenes Update wird vor der Installation
gegen eine veröffentlichte Prüfsumme geprüft.

**Häufige Fragen** — Bildschirmaufnahme dieser App leer? Der
Datenschutz-Ausschluss wirkt wie vorgesehen, im Experten-Menü abschaltbar. App
startet nicht? Die WebView2-Laufzeit fehlt, der Installer bietet sie an.
Einstellung nicht gefunden? Die Suche im Experten-Menü durchsucht alle Tabs
auf einmal. Hotkey tut nichts? Eine andere App hält die Kombination
vermutlich schon.

**Eigenständig: Backup-Manager** — verschlüsselte Backups, GFS-Aufbewahrung,
gezieltes Wiederherstellen. Im selben Fenster außerdem eine
Nutzungsstatistik (Summen, 14-Tage-Diagramm, meistgenutzte Prompts) —
unabhängig von den Backups selbst.

**Eigenständig: Schnellstarter** — Strg+Alt+P, unscharfe Suche, sofortiges Kopieren.

**Eigenständig: Mehrfachansichten** — mehrere Rasterseiten, Ansichtsgruppen als Reiter.

**Eigenständig: Theme-Editor** — eigene Farbvarianten bestehender Themes.
