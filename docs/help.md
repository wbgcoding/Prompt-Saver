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
- Colour picker *(off by default)* — pick any on-screen pixel, copies as hex/RGB/HSL
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

## Copying

### Copying

- Prompt chaining — click several tiles in a row, they combine into one clipboard copy
- Per-prompt hotkeys — copy a specific prompt from anywhere, without opening the app
- Drag-out — drag a tile onto another window to paste directly, no clipboard round-trip
- Auto-paste — after copying, the app switches to your last window and presses
  paste for you

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
window, over whatever app you're using. Pill folders and grid folders (both off by
default) merge several of them, or several grid tiles, into one expandable group.

### Tray

The system tray icon's right-click menu offers a "recently copied" and a
"favourites" section for quick access without opening the main window.

### Standalone: Quick launcher

A small always-on-top popup (default hotkey Ctrl+Alt+P) with fuzzy search across
every prompt's name and text — pick one and it copies immediately, no need to bring
the main window forward.

## Look and feel

### Appearance & themes

16 built-in themes. The theme editor (opt-in) lets you tweak any theme's colours
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

### Standalone: Backup manager

Encrypted, compressed snapshots of your entire prompt library and settings, on an
automatic schedule with smart GFS-style retention tiers, and a restore that can
target data only, settings only, or both.

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

**Kopieren** — Verketten, Prompt-Hotkeys, Herausziehen, automatisches Einfügen.

**Schwebende Buttons & Ordner** — Pillen-Ordner und Raster-Ordner (beide aus).

**Tray** — Zuletzt kopiert, Favoriten.

**Zwischenablage-Wächter** — aus, protokolliert bei Bedarf alles Kopierte im System.

**Kachelverhalten & Aussehen** — Menü, Umsortieren, Hover-Anhebung, Optik.

**Tooltips** — Verzögerung, Ausblenden, Breite.

**Favoriten** — Stern-Markierung, eigene Favoriten-Ansicht.

**Skalierung & Zoom** — UI-, Dialog-, Composer- und Icon-Größe getrennt einstellbar.

**Aussehen & Themes** — 16 mitgelieferte Themes, optionaler Theme-Editor.

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

**Eigenständig: Backup-Manager** — verschlüsselte Backups, GFS-Aufbewahrung,
gezieltes Wiederherstellen.

**Eigenständig: Schnellstarter** — Strg+Alt+P, unscharfe Suche, sofortiges Kopieren.

**Eigenständig: Mehrfachansichten** — mehrere Rasterseiten, Ansichtsgruppen als Reiter.

**Eigenständig: Theme-Editor** — eigene Farbvarianten bestehender Themes.
