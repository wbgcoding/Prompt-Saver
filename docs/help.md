# Help

The same content shown in the app's own **Help** menu (Settings → Help), kept here so
it's readable without installing anything. English and German only — the in-app
version follows the same rule and falls back to English in every other language.

## Basics

### Getting started

A tile is a saved item on the grid. Double-click an empty grid cell (or the plus
button) to create one, click any tile to copy it. Three sample tiles (an email with
placeholders, an image, and a Markdown template) are already there on first launch —
a button above the grid removes them with one click once they're no longer needed.

### Introduction tour

A five-step walkthrough shown once on first launch, right after the sample tiles
appear. The last step offers to switch on the screenshot editor, colour picker,
OCR or voice dictation right there — nothing is written until the tour actually
finishes. Escape or "Skip" asks to confirm first instead of closing right away.
Reopen it anytime from Settings ("Introduction") or the expert menu; reopened
this way, a switch on that last step takes effect immediately instead of
waiting for a Finish click.

### Creating tiles

Beyond plain text, a tile can carry an attached file, a screenshot, or a picked
screen colour.

- File attach — clicking the tile copies the file itself, not text
- Screenshot tool — capture a region or a whole window straight into a new tile
- Colour picker — pick any on-screen pixel, copies as hex/RGB/HSL
- Drag and drop — drop a file or image straight onto the grid

### Variables & placeholders

Placeholder markup inside a tile's text opens a small popup asking for each value
before the finished text reaches the clipboard.

- Dynamic placeholders — `{#{/date\}#}`, `{#{/time\}#}`, `{#{/clipboard\}#}`
  fill themselves in, no popup; their own slash-fenced syntax (not the plain
  `{#{date}#}` form) is what triggers it — insert them from the chip buttons
  under the editor's text box rather than typing them by hand
- Choice placeholders — a dropdown of preset values instead of free text
- Remember choices — the popup pre-fills the last answer for that tile
- Popup in its own window — useful with the floating buttons, where the main
  window may be hidden

### The editor

The editor is a plain text box by default; a few opt-in features (**Markdown
preview**, **private notes** — both off by default) add structure and history to it:
a rendered Markdown preview with syntax-highlighted code blocks, a full version
history, and a private note field never copied or exported with the text.

### The grid

The main window's grid is a free-placement board of tiles — up to 20×20 cells by
default (adjustable to 200×200 in the expert menu), with its own layout per view.
Drag a tile to any empty cell, or drop it on an occupied one to swap the two; the
grid never rearranges itself on its own.

### Screenshot tool

Captures part or all of the screen straight into a new tile. Drag a region, or
click a window to capture it whole — even a normally-protected one like the Task
Manager. The app's own windows are hidden for the moment of capture so they never
end up in the shot; the confirmation dialog then offers saving it as a tile, saving
it as a file, or leaving it on the clipboard.

Before that dialog, an editor opens on the capture (on by default): crop, a
freehand pen and highlighter marker, straight lines and arrows, rectangles and
ellipses, text labels, numbered step markers, a pixelate tool for blurring out
sensitive details, and a spotlight that dims everything outside a chosen area —
with undo/redo.

### Text recognition (OCR)

On by default. A button on the composer and in the screenshot dialog pulls the
text out of a screenshot or an image file, entirely on this device via Windows'
own text recognizer — nothing is uploaded or sent anywhere. The composer button
opens a small menu: from an image file, or from a screenshot you take right now.
It needs a Windows language pack — see "Language packs" below for how that
install works. Recognized text can auto-copy to the clipboard and auto-join
wrapped lines into full sentences, both adjustable.

### Voice dictation

On by default. A microphone button on the composer and in the edit dialog turns
speech into text using Windows' own on-device recognizer — nothing is uploaded
unless online recognition is explicitly allowed (off by default; a one-time
dialog asks the first time it's actually needed). Click to start: the button
turns the accent colour and pulses while listening, and its title switches from
"Dictate" to "Stop dictating" — a second click, Escape in the field, saving the
prompt, or closing the window all end the session the same way. Text lands at
the cursor (or always at the end, an expert setting) and can stop itself after
a few seconds of silence.

Spoken punctuation and simple commands are turned into real characters unless
Windows already placed them itself: "comma", "period", "question mark",
"exclamation mark", "colon", "semicolon", "hyphen", "dash", "slash", "quote",
"open paren"/"close paren" and "percent" for punctuation, "new line", "new
paragraph" and "tab" for layout, each glued to the word before it with no
extra space. Saying "literal" dictates the very next word as-is, instead of
matching it as a command — useful for actually saying "comma" out loud. The
word right after a period, question mark, exclamation mark or paragraph break
is capitalized automatically. **The exact word matters**: only these specific
command words (in the app's own language) are recognized — anything else is
just dictated as plain text.

Like OCR, it needs a Windows language pack — see "Language packs" below, same
dialog either way. A global hotkey (off by default, in the expert menu) can
start or stop a session from anywhere without bringing the window forward —
the finished text goes to the clipboard, is pasted into whatever app was
active, or lands in the composer, whichever the setting picks.

### Language packs

Text recognition and voice dictation both use Windows' own language packs and
share one install dialog. The recognition-language dropdown offers a download
the first time a chosen language has no pack installed; installing needs
administrator rights, and Windows asks for them once through its own
confirmation prompt. The dropdown's "Manage languages…" entry reopens that
same dialog any time, listing what's installed and what's available — on most
systems listing needs no special rights, but on some Windows builds it does
too, and the dialog offers to run the listing itself with administrator
rights instead of coming up blank; that step only lists packs, it installs
nothing. A background check flags a pack whose version changed or that a
Windows update silently removed, without asking to reinstall right away —
"remind me later" dismisses it for a while, or the background check can be
switched off entirely.

### Colour picker & pick history

A pipette that freezes the screen, magnifies the pixel under the cursor, and copies
it as hex, rgb, or hsl. Right-click the pipette icon to reopen the last few picks —
the history is mirrored into settings, so it survives a restart too.

### Rotating tiles

A tile can hold a list of texts instead of one — each copy serves the next entry,
in order or shuffled without repeats, instead of always the same text. On by
default; the per-tile switch sits under Advanced options in the editor.

- A small badge counts up (like "2/5") on a tile, floating pill, or pill-folder
  member — a grid-folder member shows only its file-type icon instead. In order
  mode it is the absolute position; in shuffle mode it counts draws within the
  current cycle and restarts at 1 once every entry has been shown once, rather
  than jumping to an arbitrary position
- The count survives restarts and full backups; a CSV/TXT import starts a
  fresh pointer, since the imported tile gets a new id

An eligible tile (2+ entries, no image/file attached) can also **auto-advance on
paste**: switch it on with the Auto button next to the order/random control, or
from a tile's right-click menu once both the per-tile and the app-wide switch
allow it. With auto-advance on, the next entry loads on every real Ctrl+V or
right-click-Paste, not only when copying again — the badge picks up an accent
while a session is running. Placeholders still work; the popup asks once per
session and the same values are reused for every entry. Five expert-menu
settings (all under the same toggle) tune the session timeout and the timing
window that tells a real paste apart from other clipboard activity; pasting too
fast too often pauses the session with a toast instead of looping.

## Copying

### Copying

- Chaining — click several tiles in a row, they combine into one clipboard copy.
  Hovering a text tile also shows a small chain icon next to its selection
  checkbox that starts a chain with that tile as the first entry directly —
  switchable off on its own in the expert menu.
- Per-tile hotkeys — copy a specific tile from anywhere, without opening the app
- Drag-out — hold the modifier key and drag a tile onto another window to paste
  directly, no clipboard round-trip. An expert switch swaps the two gestures, so a
  plain drag leaves the app and the key moves the tile instead.
- Auto-paste — after copying, the app switches to the last active window and
  presses paste automatically

### Hotkeys

Two independent kinds: one global combination that opens the quick launcher
(default Ctrl+Alt+P), and a combination per tile that copies — and optionally
pastes — that one tile directly. A combination must include Ctrl or Alt; the
app hotkey always wins a conflict, and among tiles the first one in the list
keeps a combination two tiles happen to share. Importing a backup that carries
a colliding hotkey drops the losing one instead of failing the import.

"All hotkeys", next to the global hotkey field in Settings, lists every hotkey —
global and per-tile — in one searchable, live-rebindable list, so rebinding or
clearing one no longer means opening that tile's editor.

### Clipboard watcher

Off by default. When switched on, everything copied anywhere on the system (not
just from this app) is logged into an inbox that can be reviewed and turned into
tiles later — a minimum-length filter and an inbox cap keep it from growing
unbounded.

### Copy history

A log of what was copied and when, separate from the clipboard watcher (this
only records copies made through the app). Grouped view, automatic retention,
per-entry delete, and usage statistics including a 14-day activity chart.

## Organising

### Library & search

The library lists every tile as a searchable, filterable list — fuzzy search,
smart sort (most-used / most-recent), and type/colour filters.

### Tags

Free-form labels assigned to tiles; filter the library or the grid down to a set
of tags, matching any or all of them.

### Favourites

A star marks a tile as a favourite; an optional dedicated view lists only
starred tiles.

### Standalone: Multiple views

A view is one grid page with its own size and arrangement; a tile can live in
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
window, over whatever app is in use. Pill folders and grid folders (both on by
default) merge several of them, or several grid tiles, into one expandable group
the same way — a tile belongs to only one grid folder at a time.

### Tray

The system tray icon's right-click menu offers a "recently copied" and a
"favourites" section for quick access without opening the main window. It draws
as a rounded, themed window matching the rest of the app rather than the plain
Windows popup; the expert menu can switch back to the plain system menu if the
custom one ever misbehaves on your setup.

### Standalone: Quick launcher

A small always-on-top popup (default hotkey Ctrl+Alt+P) with fuzzy search across
every tile's name and text — pick one and it copies immediately, no need to bring
the main window forward.

## Look and feel

### Appearance & themes

Eight built-in themes: system, light, dark, Programmer, Neon Violet, Ocean,
Cyberpunk, and Black & White. The theme editor adjusts any theme's colours and
saves the result as a custom theme.

### Standalone: Theme editor

Starts from any built-in theme, changes individual colours, and saves the result
as a new theme — the original stays untouched.

### Scaling & zoom

Independent size sliders for the overall UI, dialogs, the composer, and icons.

### Tooltips

Hovering a tile shows its full text, length, and (if present) its private note.
Delay, auto-hide timeout, and width are all adjustable.

### Toolbar

Which buttons the header and composer bar show — quick grid-size control,
library/history/pin buttons, multiple views, and view groups.

## System and data

### Privacy

Excludes the window from screen capture (on by default), blurs tiles/media until
hovered, hides tile names, dims unhovered tiles.

Everything the database holds is sealed with Windows' own per-account
encryption, so another account on the same PC cannot read it. Very rarely
Windows refuses to do that — almost always a damaged user profile or account. In
that case saving keeps working, but the data is written in readable form rather
than being silently lost, and the app says so: once in a dialog at startup, and
permanently as a red row at the top of this privacy group for as long as it is
true. That row has no off switch on purpose — it describes what is on disk right
now, not a tip. Repair the Windows profile (or sign in on a healthy account) and
the next save that succeeds seals everything again by itself; nothing needs to be
re-entered.

Separately, if the saved settings themselves turn out to be unreadable or
corrupted on disk (extremely rare), the app shows defaults instead of guessing
and refuses to save any settings change until this is resolved — the original,
possibly-recoverable data is never overwritten. A backup of the raw database is
made automatically, and a permanent warning row (also with no off switch)
appears in this same privacy group for as long as it is true. A factory reset
or restoring a backup's settings both clear it.

### System

Update checks (never installs without asking), keyboard navigation, allowing
multiple instances at once, and where the data folder lives.

### Data maintenance

Duplicate finder, trash (with its own retention setting), CSV/TXT export, and
full backup export/import — reached from the expert menu's data group.

### Trash & restoring tiles

A deleted tile goes to the trash first, not straight to permanent deletion.
Retention is in days (0 means keep forever). Restoring places the tile in the
active view's first free cell — not necessarily its original view or cell — and
does not bring back a floating button, folder membership, or hotkey it had before
deletion; those need setting up again.

### Duplicate finder

Flags tiles that are textually very similar, by a similarity score rather than
an exact match, without deleting anything on its own. Review a flagged pair side
by side, keep one, or mark the pair as not a duplicate to stop it being flagged
again. A rotating tile is only compared on its first entry.

### Import & export

Plain CSV/TXT export for the text alone, or a full backup file with everything —
tiles, views, layouts, colours, styles, and language. A CSV/TXT import shows a
preview first — every incoming item's name and whether it would land as a
duplicate — before anything is written; a full backup restores directly.

### Markdown folder sync (beta)

An optional, off-by-default mirror that writes every text tile as its own
readable `.md` file in a chosen folder, alongside the encrypted database —
including which view, grid cell, and folder it sits in. A view or folder named in
an incoming file that does not exist yet on this machine is created
automatically, matched by name.
**The `.md` files are plain text** — unlike the database, anyone with access to
that folder can read the content, so a synced or shared folder should be avoided
for anything sensitive. A tile with an attached file, image, or PDF syncs as a
reference: the header carries the path and the PDF page, never the file itself. A
screenshot pasted straight into a tile has no path and stays in the database only.

### The expert menu

Every optional feature lives behind an on/off switch, with its own parameters
right underneath once it's on. The search box scans every tab at once; a feature
that ships switched off carries a small dot badge. Reset works at three levels:
one value's own undo icon, a whole group's reset, or everything at once.

### Keyboard-only use

Arrow keys move a focus ring across the grid; Enter or Space copies whichever
tile is focused. The ring fades out a short while after the window loses focus.

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
  privacy setting is working as intended; turn it off in the expert menu if
  that's not wanted
- *The app won't start* — it needs the Microsoft WebView2 runtime; the installer
  offers it automatically if missing
- *I can't find a setting* — check the expert menu's search box first; it scans
  every tab at once
- *A hotkey doesn't do anything* — another app may already hold that combination;
  the editor explains this instead of accepting it silently

### Standalone: Backup manager

Encrypted, compressed snapshots of the entire library and settings, on an
automatic schedule with smart GFS-style retention tiers, and a restore that can
target data only, settings only, or both. The same window also has a
usage-statistics section (totals, a 14-day activity chart, most-used tiles),
unrelated to the backups themselves.

---

## Hilfe (Deutsch)

Derselbe Inhalt wie oben, wie er auch im Hilfe-Menü der App steht (Einstellungen →
Hilfe). Nur Englisch und Deutsch — jede andere Sprache in der App fällt auf
Englisch zurück.

**Erste Schritte** — Eine Kachel ist ein gespeicherter Eintrag im Raster. Doppelklick
auf eine leere Rasterzelle (oder der Plus-Button) legt eine an, ein Klick kopiert sie.
Drei Beispielkacheln (E-Mail mit Platzhaltern, Bild, Markdown-Vorlage) sind beim
ersten Start bereits da — ein Button über dem Raster entfernt sie mit einem Klick.

**Einführungstour** — fünf Schritte beim ersten Start, direkt nach den
Beispielkacheln; der letzte Schritt bietet an, Screenshot-Editor, Farbpipette,
OCR oder Spracheingabe gleich einzuschalten — geschrieben wird erst, wenn die
Tour tatsächlich beendet ist. Escape oder „Überspringen" fragt zuerst nach, statt
sofort zu schließen. Jederzeit erneut aufrufbar über die Einstellungen
(„Einführung") oder das Experten-Menü — dort wirkt ein Schalter auf dem letzten
Schritt sofort statt erst bei „Fertig".

**Kacheln anlegen** — Dateianhang, Screenshot-Werkzeug, Farbpipette (aus),
Ziehen & Ablegen.

**Variablen & Platzhalter** — `{#{Name}#}` im Text öffnet beim Kopieren ein Popup
für jeden Wert. Dynamische Platzhalter (`{#{date}#}`, `{#{time}#}`,
`{#{clipboard}#}`) füllen sich selbst.

**Der Editor** — Markdown-Vorschau, Syntax-Hervorhebung, Versionsverlauf, private
Notizen (Markdown-Vorschau und Notizen standardmäßig aus).

**Das Raster** — freie Platzierung, standardmäßig bis 20×20 Zellen (im
Experten-Menü bis 200×200 einstellbar), eigenes Layout je Ansicht.

**Screenshot-Werkzeug** — Bereich oder ganzes Fenster (auch geschützte wie den
Task-Manager) direkt zu einer neuen Kachel. Davor öffnet sich standardmäßig ein
Editor: Zuschneiden, Stift, Marker, Linien, Pfeile, Rechtecke, Ellipsen, Text,
nummerierte Schritte, Pixelieren und ein Spotlight, das alles außerhalb eines
Bereichs abdunkelt — mit Rückgängig/Wiederholen.

**Texterkennung (OCR)** — standardmäßig an; liest per Knopf in der Leiste und
im Screenshot-Dialog Text aus einem Screenshot oder einer Bilddatei, komplett
offline über die eigene Texterkennung von Windows. Der Knopf in der Leiste öffnet
ein kleines Menü: aus einer Bilddatei oder aus einem Screenshot. Erkannter Text
kann automatisch kopiert und umgebrochene Zeilen zu ganzen Sätzen zusammengefügt
werden, beides einstellbar. Braucht ein Windows-Sprachpaket — siehe
„Sprachpakete" unten.

**Spracheingabe** — standardmäßig an; der Mikrofon-Knopf neben der Eingabeleiste
und im Kachel-Editor verwandelt Sprache in Text, komplett auf dem Gerät über die
eigene Spracherkennung von Windows. Gesprochene Satzzeichen-Wörter („Komma",
„neue Zeile" …) werden zu echten Zeichen; „wörtlich" vor einem Wort tippt es
statt es als Befehl zu erkennen. Ein optionaler globaler Kurzbefehl startet und
stoppt eine Aufnahme, ohne das Fenster zu öffnen. Braucht ebenfalls ein
Windows-Sprachpaket — derselbe Dialog wie bei OCR.

**Sprachpakete** — Texterkennung und Spracheingabe teilen sich Windows' eigene
Sprachpakete und denselben Installationsdialog. Die App bietet den Download beim
ersten Bedarf an; die Installation braucht Administratorrechte (übliche
Windows-Bestätigung). „Sprachen verwalten…" im Sprachen-Dropdown listet
Installiertes und Verfügbares und startet Installation/Update erneut; das
Auflisten selbst braucht normalerweise keine Sonderrechte, auf manchen
Windows-Systemen aber doch — der Dialog sagt das dann offen und bietet an, die
Abfrage mit Administratorrechten auszuführen, statt leer zu bleiben. Dabei wird
nur aufgelistet, nichts installiert. Eine Hintergrundprüfung meldet ein Paket,
dessen Version sich geändert hat oder das ein Windows-Update entfernt hat, ohne
gleich zur Neuinstallation aufzufordern — „später erinnern" blendet den Hinweis
vorübergehend aus, oder die Prüfung lässt sich ganz abschalten.

**Farbpipette & Auswahlverlauf** — Pixel als Hex/RGB/HSL kopieren; die letzten
Auswahlen überstehen einen Neustart. Standardmäßig aus.

**Wechselnde Kacheln** — eine Kachel trägt eine Liste von Texten, jede Kopie
liefert den nächsten Eintrag (Reihenfolge oder Zufall). Standardmäßig an; der
Schalter pro Kachel sitzt im Editor unter Erweiterte Optionen. Ein Badge auf
Kachel/Pille/Pillen-Ordner-Mitglied zählt hoch — im Zufalls-Modus die Ziehungen
im aktuellen Durchlauf statt einer springenden Position; übersteht Neustart und
Sicherung, aber keinen CSV/TXT-Import (neue Kachel, neuer Zeiger). Eine geeignete
Kachel (2+ Einträge, kein Bild/Datei) kann zusätzlich **beim Einfügen
automatisch weiterschalten** — Auto-Knopf im Editor oder Rechtsklickmenü,
solange Kachel- und App-Schalter beide an sind. Bei jedem echten Einfügen
rückt der nächste Eintrag nach, nicht nur beim Kopieren; das Abzeichen zeigt
eine Akzentfarbe, solange die Sitzung läuft. Platzhalter werden einmal pro
Sitzung abgefragt, dieselben Werte gelten für jeden Eintrag. Fünf
Experteneinstellungen regeln Zeitlimit und Erkennungsfenster; zu schnelles,
zu häufiges Einfügen pausiert die Sitzung mit einem Hinweis statt zu wiederholen.

**Kopieren** — Verketten (auch direkt über ein Kettensymbol an der Kachel,
einzeln abschaltbar), Hotkeys pro Kachel, Herausziehen (mit oder, per Schalter,
ohne Modifikatortaste), automatisches Einfügen.

**Hotkeys** — ein globaler Hotkey öffnet den Schnellstarter (Standard
Strg+Alt+P), ein eigener Hotkey pro Kachel kopiert (und fügt optional ein). Bei
einer Kollision gewinnt der App-Hotkey, sonst die erste Kachel in der Liste.
"Alle Hotkeys" neben dem globalen Feld listet jeden Hotkey durchsuchbar auf und
belegt jede Zeile direkt um, ohne den jeweiligen Editor zu öffnen.

**Schwebende Buttons & Ordner** — Pillen-Ordner und Raster-Ordner (beide aus);
eine Kachel gehört immer nur zu einem Raster-Ordner gleichzeitig.

**Taskleistenmenü** — Zuletzt kopiert, Favoriten. Erscheint als abgerundetes,
zum Thema passendes Fenster statt des einfachen Windows-Menüs; im Expertenmenü
lässt sich bei Bedarf auf das einfache Systemmenü zurückschalten.

**Zwischenablage-Überwachung** — aus, protokolliert bei Bedarf alles Kopierte im
System.

**Kachel-Verhalten & -Aussehen** — Menü, Umsortieren, Hover-Anhebung, Optik.

**Tooltips** — Verzögerung, Ausblenden, Breite.

**Favoriten** — Stern-Markierung, eigene Favoriten-Ansicht.

**Skalierung & Zoom** — UI-, Dialog-, Editor- und Icon-Größe getrennt einstellbar.

**Aussehen & Themes** — acht mitgelieferte Themes, optionaler Theme-Editor.

**Bibliothek & Suche** — unscharfe Suche, intelligente Sortierung, Filter.

**Tags** — freie Schlagwörter, UND/ODER-Filter.

**Kopierverlauf** — Protokoll, Gruppierung, Aufbewahrung, Einzeleintrag löschen,
Statistik mit 14-Tage-Diagramm.

**Medien** — Video-Steuerung, Graustufen/Abdunkeln.

**Werkzeugleiste** — welche Buttons Kopf- und Composer-Leiste zeigen.

**Datenschutz** — Bildschirmaufnahme-Ausschluss (an), Weichzeichnen, Namen
verbergen, Abdunkeln. Alles in der Datenbank ist mit der kontogebundenen
Windows-Verschlüsselung versiegelt; ein anderes Konto am selben PC kann es also
nicht lesen. Sehr selten verweigert Windows das — fast immer wegen eines
beschädigten Benutzerprofils oder Kontos. Dann wird weiter gespeichert, aber im
Klartext statt still verloren, und die App sagt es: einmal als Hinweis beim
Start und dauerhaft als rote Zeile oben in dieser Gruppe, solange es zutrifft.
Diese Zeile hat bewusst keinen Schalter — sie beschreibt, was gerade auf der
Festplatte liegt. Ist das Windows-Profil repariert, verschlüsselt der nächste
erfolgreiche Speichervorgang wieder alles von selbst; nichts muss neu erfasst
werden.

Getrennt davon: Sollten die gespeicherten Einstellungen selbst auf der
Festplatte unlesbar oder beschädigt sein (sehr selten), zeigt die App
Standardwerte an, statt zu raten, und verweigert jede Einstellungsänderung, bis
das behoben ist — die ursprünglichen, möglicherweise noch rettbaren Daten
werden nie überschrieben. Es wird automatisch eine Sicherung der rohen
Datenbank angelegt, und eine dauerhafte Warnzeile (ebenfalls ohne Schalter)
erscheint in derselben Datenschutz-Gruppe, solange es zutrifft. Ein Werksreset
oder das Wiederherstellen der Einstellungen aus einem Backup löschen sie
beide.

**System** — Updates, Tastatur-Navigation, mehrere Instanzen, Datenordner.

**Datenwartung** — Duplikatfinder, Papierkorb, Import/Export.

**Papierkorb & Kacheln wiederherstellen** — gelöschte Kacheln landen zuerst im
Papierkorb (Aufbewahrung in Tagen, 0 = für immer). Wiederherstellen legt in die
erste freie Zelle der aktiven Ansicht, nicht zwingend an den Originalplatz, und
ohne Pille, Ordner-Mitgliedschaft oder Hotkey von vorher.

**Duplikatfinder** — findet textlich sehr ähnliche Kacheln nach
Ähnlichkeitswert, keine exakte Übereinstimmung nötig, löscht nichts von sich
aus. Eine wechselnde Kachel wird nur nach ihrem ersten Eintrag verglichen.

**Import & Export** — CSV/TXT nur für den reinen Text, volle Sicherung für
alles. Ein CSV/TXT-Import zeigt vorher eine Vorschau mit Name und
Duplikat-Kennzeichnung; eine volle Sicherung stellt direkt wieder her.

**Markdown-Ordner-Sync (Beta)** — spiegelt jede Text-Kachel als lesbare
`.md`-Datei in einem gewählten Ordner, inklusive Ansicht, Rasterzelle und
Ordner. Angehängte Dateien, Bilder und PDFs wandern als Verweis mit (Pfad und
PDF-Seite im Kopf, nie die Datei selbst); ein direkt eingefügter Screenshot hat
keinen Pfad und bleibt nur in der Datenbank. Eine in einer eingehenden Datei
genannte Ansicht oder ein Ordner, den es auf diesem Rechner noch nicht gibt,
wird automatisch angelegt, per Namensabgleich. **Die Dateien sind Klartext** —
anders als die Datenbank, also kein synchronisierter oder geteilter Ordner für
Sensibles.

**Das Experten-Menü** — jede Funktion hat einen Schalter mit ihren Parametern
direkt darunter. Die Suche durchsucht alle Tabs auf einmal; ausgeschaltete
Funktionen tragen ein kleines Punkt-Abzeichen. Zurücksetzen auf drei Ebenen:
ein Wert, eine ganze Gruppe, oder alles auf einmal.

**Tastaturbedienung** — Pfeiltasten bewegen einen Fokusring über das Raster
(an), Enter oder Leertaste kopiert die fokussierte Kachel.

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
Nutzungsstatistik (Summen, 14-Tage-Diagramm, meistgenutzte Kacheln) —
unabhängig von den Backups selbst.

**Eigenständig: Schnellstarter** — Strg+Alt+P, unscharfe Suche, sofortiges Kopieren.

**Eigenständig: Mehrfachansichten** — mehrere Rasterseiten, Ansichtsgruppen als Reiter.

**Eigenständig: Theme-Editor** — eigene Farbvarianten bestehender Themes.
