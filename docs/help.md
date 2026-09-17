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
Reopen it anytime from the expert menu; reopened this way, a switch on that
last step takes effect immediately instead of waiting for a Finish click.

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

For a file/PDF/video tile that also carries its own stored preview picture, the
image shown above the file row is only that tile's preview — the attached file
below it is a separate thing, and copying the tile puts the file's path on the
clipboard, never the picture.

### The grid

The main window's grid is a free-placement board of tiles — up to 20×20 cells by
default (adjustable to 200×200 in the expert menu), with its own layout per view.
Drag a tile to any empty cell, or drop it on an occupied one to swap the two; the
grid never rearranges itself on its own. Drop a tile on a view tab above the grid
to move it into that view instead, or hold Ctrl while dropping to copy it there
so it stays in both. Hovering over a collapsed view group opens it after a
moment, so the drop can target a view inside.

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
with undo/redo. Zoom in for pixel-precise work and pan around with the Hand
tool (or hold Space, or the middle mouse button) while zoomed in. Resize
changes the image's actual pixel size (not just the view): set a width,
height, or percentage, with presets for 25/50/200% and "fit 1920 wide"; it is
its own undoable step, so scaling back and forth never loses quality beyond
what the last resize itself introduced.

Both editors can also open a file you already have, not just a fresh
capture or recording: right-click the screenshot or record button (or an
image/video attachment in the library) and choose "Open a file in the
editor". The content of the file decides which editor opens, no matter what
its extension says. The editor always works on a copy, so the original file
is never changed; saving asks where to put the result, starting in the
original file's own folder.

### Screen recording

Records a window, a monitor, or a dragged region as a video, via the record
button next to the screenshot tool. MP4 (H.264) is always what gets written
while recording; three quality steps trade file size for detail before you
start:

- **Small** — a compact bitrate (4000 kbps by default), for quick clips.
- **High** (the default) — a higher bitrate (12000 kbps by default), for a
  sharper picture.
- **Lossless** — recorded at the same native bitrate as your current step,
  then automatically re-encoded losslessly once the recording stops. This can
  run into several gigabytes for longer recordings.

A bitrate slider next to the quality step lets you fine-tune small/high
further; picking a step resets the slider to that step's own default, and the
slider is then free to adjust. The slider has no effect on the lossless step,
since what actually makes that step lossless is the re-encode afterward, not
the recording bitrate.

A format setting picks what the finished file ends up as: MP4 needs nothing
further; WebM, AVI, MOV and MKV are produced by converting the MP4 afterward,
keeping its audio. GIF, animated WebP and APNG are converted the same way but
capped at 30 seconds and without audio. Converting to anything other than
MP4, or to lossless MP4, needs ffmpeg — a separate, GPL-licensed tool the app
does not ship with its portable build. If it is missing, the format and
quality controls say so directly and offer to fetch it (a one-time download,
only ever with your consent); an installed build already carries it and never
needs to download anything. A failed or cancelled conversion always leaves
the original MP4 recording untouched. A failure names the actual reason in
plain language (the file was incomplete, there was no space left, and so on)
instead of a raw ffmpeg log line; a "Technical details" row underneath, closed
by default, holds that log line with a copy button for anyone who wants it.

Clicking the record button opens the same frozen-desktop overlay a screenshot
uses: click a window, drag a region, or click empty desktop for the whole
screen. An optional countdown toast then counts down before capture actually
starts (3 seconds by default, adjustable in the expert menu, 0 turns it off).
A small floating control bar then follows the recording — a red dot, elapsed
time, live file size, Pause/Resume, Stop and a discard (cancel) button. It
follows a moved recorded window and is itself excluded from the recording no
matter what, even if the recording happens to include the monitor the bar
sits on.

System audio and microphone audio (pick a device in the expert menu) both
record on by default, each into its own track. The microphone track is
captured but not mixed in audibly — it only becomes part of what you hear
once you bring it in yourself, from the video editor below. Turn microphone
capture off in the expert menu if you would rather it never records at all.
If the microphone is unplugged mid-recording, the picture and system audio
keep going — the control bar's microphone icon switches to a "lost" state
instead of stopping anything.

A running recording can also be stopped or paused/resumed from anywhere via
two optional global hotkeys (set in the expert menu, live only while a
recording is actually running) and from the tray menu.

Stopping leads into a dialog offering Save, Save as…, Copy, Edit and Discard.
Save and Copy use one shared recording folder (changeable here or in the
expert menu); Edit opens the clip in the video editor below without deciding
its fate yet — closing that dialog afterward without ever saving or copying
still discards the file, exactly as if Edit had never been clicked. If the
chosen format or quality step needs converting the MP4 first, the dialog
shows a short "Converting…" status instead of a preview (click it to cancel);
a conversion failure or cancellation always falls back to the untouched
original recording rather than losing it. If the window being recorded
closes on its own, the recording stops automatically and keeps whatever was
captured up to that point.

A recording that never reached its own stop path — a crash, a forced
shutdown — leaves behind a file with no usable video track. The app checks
for these in the background and removes them once they clear a short grace
period, so a just-stopped recording that is still finalising is never
touched; both the grace period and the check interval are adjustable in the
expert menu, alongside a manual "clean up now" action.

### Video editor

On by default, reachable from a recording's own result dialog (the Edit
button above) or from any video prompt's right-click menu ("Edit video"). It
opens the clip in its own window with a live preview and no processing until
you actually export — trim and cut (frame-exact, keeps the original quality
when nothing else changes), crop, change speed, fade in/out, resize (typed
width/height or a percentage, an aspect-ratio lock, and quick presets from
4K down to 25% — a preset never enlarges past the source, but the percent
field and free typing can, for upscaling on purpose), and overlay images,
freeze-frames, text, pixelation or a spotlight anywhere on the timeline. A
filmstrip and waveform give you a zoomable overview to navigate by. The preview itself also zooms (scroll wheel, up to 8×) and pans by
dragging — the middle mouse button always, the left button once zoomed in —
for placing a small overlay precisely on a large recording. Exporting
re-encodes once, with a few ready-made presets (unchanged
quality, a smaller size for chat, or an animated GIF) that remember your last
choice. The format choice goes beyond MP4, WebM and GIF to AVI, MOV, MKV,
animated WebP and APNG as well, each with a short note on what it's good for;
GIF, WebP and APNG carry no sound at all (the source audio stays untouched in
the original recording) and warn past ten seconds, since all three grow large
fast. Undo/redo covers every editing step, with an adjustable limit in the
expert menu. A failed export shows the actual reason in plain language, with
the raw ffmpeg log reachable underneath in a closed "Technical details" row —
never the log itself as the message. A second video can be added too — "Add
a video" offers it either as another overlay (its own rectangle, time range
and volume, muted by default) or as a whole extra section spliced in at the
start, the end or the current position; each extra source is checked once
when picked and stays visibly marked as missing if it later moves or is
deleted, up to a small fixed number per project.

Both editors work the same way for their shared tools: Ctrl+Plus, Ctrl+Minus
and Ctrl+0 zoom in, zoom out and fit the view in either one, and Ctrl+Z,
Ctrl+Y (or Ctrl+Shift+Z) undo and redo through the same button pair. In the
screenshot editor, clicking an existing rectangle, ellipse, line or arrow
selects it and shows the same handles as dragging one out fresh. The video
editor keeps selecting its overlays from its own list instead, since it only
draws an overlay's handles once that overlay is already selected — the two
toolbars stay where they are too, a bar on top for the screenshot editor and
a side column next to the timeline for the video editor, because the
timeline needs the room the screenshot editor doesn't.

Right-clicking a shape (screenshot editor) or an overlay's row (video editor)
opens a small menu to duplicate or delete it. A duplicate keeps the same
values and time range and is offset a little so it sits next to the
original instead of exactly on top of it — how far, in an expert setting
shared by both editors. In the video editor, the colour, thickness, opacity
and font size a new overlay starts with also follow whatever was last used
for that same kind (image, frame, text, pixelation or spotlight), across
restarts too; an expert switch turns that off again in favour of the fixed
starting values.

Dragging a shape (screenshot editor) or a crop box or overlay (video editor)
snaps to the canvas or frame edges, its centre in both directions, and the
edges and centre of every other shape or overlay already placed, once the
pointer gets close enough. A thin accent-coloured line marks which one it
caught, and disappears the instant the drag ends. Holding Alt while dragging
turns snapping off for that one drag. Each editor has its own switch for this,
sharing one distance setting between them.

The text tool's background is a choice of none, a solid plate or an outline,
so white or light-coloured text stays readable on a light background too;
plate was the only option before, and stays the default. An expert setting
picks the outline width, and its colour, black or white, follows the text
colour automatically for the best contrast.

A selected shape (screenshot editor) or overlay (video editor) can be nudged
with the arrow keys, a step at a time; holding Ctrl changes its size instead
of its position, and Shift takes a bigger step. Both step sizes are expert
settings shared by every place that uses them. In the video editor the arrow
keys only take on this job while something is selected, since they normally
step through frames there; while selected, comma and period step frames
instead, and a short line above the transport controls always says what the
arrow keys currently do. Escape deselects and hands the arrow keys back to
their usual job.

A toolbar button in either editor compares against the untouched capture:
hold it down to peek at the original for as long as it is held, or tap it to
lock that view on until tapped again. The V key does the same, and a small
"Original" badge appears whenever it is showing so it is never mistaken for
the edit. In the video editor this only hides overlays (fades and the export
resolution never show in the live preview to begin with), and the current
cuts stay in effect, since jumping to a cut-out moment would only confuse.
None of it touches the export.

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

Not the same as chaining (see Copying below): rotation changes the text inside
ONE tile from copy to copy, chaining copies several DIFFERENT tiles together
as one text.

| I want... | use... |
| --- | --- |
| several wordings of the same prompt | rotation |
| several different prompts, one after another | chaining |

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

- Chaining: not the same as rotation (see Rotating tiles above). Several
  DIFFERENT tiles copied together as one text, rather than one tile changing
  what it holds. Click several tiles in a row, they combine into one clipboard
  copy. Hovering a text tile also shows a small chain icon next to its
  selection checkbox that starts a chain with that tile as the first entry
  directly, switchable off on its own in the expert menu.
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

A tile with its own hotkey shows a small keyboard symbol.

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
smart sort (most-used / most-recent), and type/colour filters. Type -word to
exclude matches, or tag:word / var:word to search only tags or placeholder
names.

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

### Floating buttons

Small always-on-top copy buttons that sit outside the main window, over
whatever app is in use.

### Folders

One gesture works in three places: drop one onto another to gather it into a
folder. A view tab dropped onto another view tab makes a group tab. A floating
button dropped onto another makes a pill folder. A tile dropped onto another
tile makes a grid folder, and belongs to only one grid folder at a time.

### Tray

The system tray icon's right-click menu offers a "recently copied" and a
"favourites" section for quick access without opening the main window. It draws
as a rounded, themed window matching the rest of the app rather than the plain
Windows popup; the expert menu can switch back to the plain system menu if the
custom one ever misbehaves on your setup. The expert menu also caps how long a
prompt name may get before it is cut off with an ellipsis in that menu.

### Standalone: Quick launcher

A small always-on-top popup (default hotkey Ctrl+Alt+P) with fuzzy search across
every tile's name and text — pick one and it copies immediately, no need to bring
the main window forward.

## Look and feel

### Appearance & themes

Seven built-in themes: system/light, dark, Programmer, Neon Violet, Ocean,
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

The screen-capture exclusion needs Windows 10 version 2004 (build 19041) or
newer; on an older Windows it is silently ignored and the window is captured
like any other.

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

If a settings save itself cannot be written to disk (data folder full, no
write permission), changes stay active for the rest of the session but are
lost on restart. The app says so the same way: once in a dialog, and as a
permanent row in this privacy group for as long as the last save is still
failing — it clears itself the moment a save reaches disk again.

### System

Update checks (never installs without asking), keyboard navigation, allowing
multiple instances at once, and where the data folder lives. A long export,
conversion, ffmpeg download, backup or import fills the app's taskbar icon
with its progress, and shows a plain "about X left" estimate once enough of
it has run for the guess to mean something — both switchable off.

### Data maintenance

Duplicate finder, trash (with its own retention setting), CSV/TXT export, and
full backup export/import — reached from the expert menu's data group. The
duplicate finder also has a shortcut in the settings dialog, next to import
and export, so it does not take opening the expert menu to find it.

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
duplicate — before anything is written; a full backup restores directly. If
the file changes after the preview is shown, confirming the import is refused
rather than importing something different from what was previewed.

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
The expert menu controls how often the sync folder is re-checked for files
edited outside the app.

The folder is `<data folder>\prompt-saver-sync` unless you pick another one; an
installation that was already syncing keeps the folder it has. A file is read
once it has stopped changing, so an edit made outside can take one extra round
to arrive. What happens in either direction is fixed and never silent:

- **A file deleted in the folder does not delete the tile.** It is written again
  from the app on the next round. Delete the tile in the app and its file goes
  with it.
- **Renaming a file makes a second tile.** The file name is the tile's permanent
  id, so the renamed file arrives as a new tile and the original is written back
  out under its own name. Rename in the app instead.
- **A tile and its file changed at the same time?** The file wins that round, and
  the version it replaced is kept in that tile's version history. The other way
  round the app wins the file, and what was in the file is kept next to it as
  `<id>.conflict-<number>.md`.
- **Files this app did not write are never touched** in either direction, and a
  file past 8 MB is skipped whole, never cut short. The error log says why.
- **Picking a different folder asks** what should happen to the old one: delete
  it, or keep it. Deleting only ever removes the `.md` files this app wrote
  itself, never a sub-folder, and the folder itself only if it ends up empty. One
  file in there that the app did not write and nothing at all is deleted, with a
  message naming what is in the way.
- **An unreachable folder pauses the sync** instead of failing every few seconds
  (after five failed rounds, adjustable). The line under the folder in the expert
  menu says when the folder was last read, how many files were in it, and why it
  is paused.

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

On by default, and — together with a first-run download of the Microsoft
WebView2 installer if that runtime is missing — one of the only two kinds of
network traffic the app makes on its own. A downloaded update is verified
against a published checksum before it runs; a missing or mismatching checksum
blocks the install instead of running an unverified file. The third and last
kind is ffmpeg, used for video export/conversion: an installed build ships it
and never touches the network for it, while a portable build offers a one-time
download only when a video feature actually needs it and only with your
explicit consent — never automatically, never in the background.

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
sofort zu schließen. Jederzeit erneut aufrufbar über das Experten-Menü — dort
wirkt ein Schalter auf dem letzten Schritt sofort statt erst bei „Fertig".

**Kacheln anlegen** — Dateianhang, Screenshot-Werkzeug, Farbpipette (aus),
Ziehen & Ablegen.

**Variablen & Platzhalter** — `{#{Name}#}` im Text öffnet beim Kopieren ein Popup
für jeden Wert. Dynamische Platzhalter (`{#{date}#}`, `{#{time}#}`,
`{#{clipboard}#}`) füllen sich selbst.

**Der Editor** — Markdown-Vorschau, Syntax-Hervorhebung, Versionsverlauf, private
Notizen (Markdown-Vorschau und Notizen standardmäßig aus). Bei einer Datei-Kachel
mit eigenem Vorschaubild: das Bild über der Dateizeile ist nur die Kachel-Vorschau,
der Anhang darunter ist eine eigene Sache, kopiert wird immer nur der Dateipfad.

**Das Raster** — freie Platzierung, standardmäßig bis 20×20 Zellen (im
Experten-Menü bis 200×200 einstellbar), eigenes Layout je Ansicht. Eine Kachel
auf einen Ansichtsreiter über dem Raster gezogen verschiebt sie in diese
Ansicht, mit gedrückter Strg-Taste wird stattdessen kopiert und sie bleibt in
beiden. Über einer eingeklappten Ansichtsgruppe verweilt klappt diese nach
kurzer Zeit auf, damit sich in eine Ansicht darin ablegen lässt.

**Screenshot-Werkzeug** — Bereich oder ganzes Fenster (auch geschützte wie den
Task-Manager) direkt zu einer neuen Kachel. Davor öffnet sich standardmäßig ein
Editor: Zuschneiden, Stift, Marker, Linien, Pfeile, Rechtecke, Ellipsen, Text,
nummerierte Schritte, Pixelieren und ein Spotlight, das alles außerhalb eines
Bereichs abdunkelt — mit Rückgängig/Wiederholen. Zum Heranzoomen für
pixelgenaues Arbeiten lässt sich das Bild mit dem Hand-Werkzeug (oder
gehaltener Leertaste bzw. der mittleren Maustaste) verschieben. Die
Größenänderung ändert die tatsächliche Pixelgröße des Bildes (nicht nur die
Ansicht): Breite, Höhe oder Prozentwert einstellbar, mit Voreinstellungen für
25/50/200 % und "auf 1920 Breite anpassen"; sie ist selbst ein rückgängig
machbarer Schritt.

**Bildschirmaufnahme** — nimmt ein Fenster, einen Monitor oder einen gezogenen
Bereich als Video auf, über den Aufnahme-Knopf neben dem Screenshot-Werkzeug.
Aufgenommen wird immer MP4 (H.264); drei Qualitätsstufen wählen davor die
Bitrate: Klein (4000 kbps), Hoch (12000 kbps, Standard) und Verlustfrei — dabei
wird mit der aktuellen Bitrate aufgenommen und die Aufnahme danach automatisch
verlustfrei neu kodiert, was bei längeren Aufnahmen mehrere Gigabyte groß
werden kann. Ein Bitratenregler neben der Qualitätsstufe passt Klein/Hoch
weiter an; er hat auf der Stufe Verlustfrei keine Wirkung. Ein Format-Feld
bestimmt die fertige Datei: MP4 braucht nichts weiter; WebM, AVI, MOV und MKV
entstehen durch Umwandeln der MP4-Datei danach und behalten ihren Ton. GIF,
animiertes WebP und APNG entstehen ebenso, sind aber auf 30 Sekunden begrenzt
und ohne Ton. Für alles außer MP4 sowie für Verlustfrei wird ffmpeg gebraucht, ein
separates, GPL-lizenziertes Werkzeug; fehlt es, sagen Format- und
Qualitätsauswahl das direkt und bieten den Download an (einmalig, nur mit
Zustimmung) — eine installierte Version bringt es bereits mit. Eine
fehlgeschlagene oder abgebrochene Umwandlung lässt die ursprüngliche
MP4-Aufnahme immer unangetastet. Ein Fehlschlag nennt den eigentlichen Grund
in Klartext (Datei unvollständig, kein Platz mehr und so weiter) statt einer
rohen ffmpeg-Protokollzeile; eine zugeklappte Zeile „Technische Angaben"
darunter hält diese Zeile samt Kopierknopf für alle bereit, die sie brauchen.
Ein Klick auf den Aufnahme-Knopf öffnet
dieselbe Bildschirm-Übersicht wie ein Screenshot: Fenster anklicken, Bereich
ziehen oder auf den freien Desktop klicken für den ganzen Bildschirm. Ein
Countdown zählt danach herunter, bevor die Aufnahme wirklich startet
(3 Sekunden Standard, im Expertenmenü einstellbar, `0` schaltet ihn ab). Eine
kleine Bedienleiste begleitet die Aufnahme (roter Punkt, Laufzeit,
Dateigröße, Pause/Fortsetzen, Stopp, Verwerfen), folgt einem verschobenen
Aufnahmefenster und ist selbst niemals im Video zu sehen. Systemton (Standard
an) und Mikrofon (Standard aus, Gerät wählbar im Expertenmenü) werden je als
eigene Spur aufgenommen; zieht man das Mikrofon während der Aufnahme ab,
laufen Bild und Systemton weiter, das Mikrofon-Symbol zeigt nur „verloren".
Stopp führt in denselben Anheften/Speichern/Verwerfen-Dialog wie ein
Screenshot; braucht das gewählte Format oder die Qualitätsstufe eine
Umwandlung, zeigt der Dialog zuerst „Wird konvertiert…" (anklicken zum
Abbrechen) statt einer Vorschau — eine fehlgeschlagene oder abgebrochene
Umwandlung fällt immer auf die unangetastete Originalaufnahme zurück.
Schließt sich das aufgenommene Fenster von selbst, stoppt die Aufnahme
automatisch und behält, was bis dahin aufgenommen wurde.

**Screenshot- und Video-Editor** — arbeiten bei den gemeinsamen Werkzeugen
gleich: Strg+Plus, Strg+Minus und Strg+0 zoomen in beiden hinein, heraus und
passen die Ansicht ein; Strg+Z, Strg+Y (oder Strg+Umschalt+Z) machen mit
demselben Knopfpaar rückgängig oder wiederholen. Im Screenshot-Editor wählt
ein Klick auf ein vorhandenes Rechteck, eine Ellipse, eine Linie oder einen
Pfeil es mit denselben Anfassern aus, die auch beim frischen Ziehen
erscheinen. Der Video-Editor wählt seine Überlagerungen weiter über seine
eigene Liste, weil er die Anfasser einer Überlagerung erst zeichnet, sobald
sie schon ausgewählt ist; die Werkzeugleisten selbst bleiben ebenfalls an
ihrem Platz, oben beim Screenshot-Editor, seitlich neben der Zeitleiste beim
Video-Editor, weil die Zeitleiste den Platz braucht, den der
Screenshot-Editor nicht hat. Im Video-Editor lässt sich außerdem ein zweites
Video einfügen — „Video hinzufügen" bietet es entweder als weitere
Überlagerung (eigenes Rechteck, eigener Zeitbereich, eigene Lautstärke,
standardmäßig stumm) oder als ganzen zusätzlichen Abschnitt am Anfang, am
Ende oder an der aktuellen Stelle; jede zusätzliche Quelle wird beim
Auswählen einmal geprüft und bleibt sichtbar als fehlend markiert, falls sie
später verschoben oder gelöscht wird, bis zu einer festen Höchstzahl je
Projekt.

Ein Rechtsklick auf eine Form (Screenshot-Editor) oder die Zeile einer
Überlagerung (Video-Editor) öffnet ein kleines Menü zum Duplizieren oder
Löschen. Ein Duplikat behält Werte und Zeitbereich und wird ein Stück
verschoben, damit es neben dem Original liegt statt genau darauf — wie weit,
in einer Expertenzeile, die sich beide Editoren teilen. Im Video-Editor
richten sich Farbe, Stärke, Deckkraft und Schriftgröße einer neuen
Überlagerung außerdem nach dem, was für dieselbe Art (Bild, Rahmen, Text,
Verpixeln oder Spotlight) zuletzt benutzt wurde, auch über einen Neustart
hinweg; ein Expertenschalter stellt stattdessen wieder feste Vorgabewerte ein.

Beim Ziehen einer Form (Screenshot-Editor) oder eines Zuschnitts bzw. einer
Überlagerung (Video-Editor) rastet die Bewegung an den Kanten der Leinwand
oder des Bildes, an dessen Mitte in beide Richtungen und an den Kanten und der
Mitte jeder anderen schon platzierten Form oder Überlagerung ein, sobald der
Zeiger nah genug kommt. Eine dünne Linie in der Akzentfarbe zeigt, welche
Linie gerade getroffen wurde, und verschwindet sofort wieder, sobald das
Ziehen endet. Gedrückt gehaltenes Alt schaltet das Einrasten für diesen einen
Zug ab. Jeder Editor hat dafür einen eigenen Schalter, die Toleranz teilen sie
sich.

Der Hintergrund des Textwerkzeugs ist jetzt eine Wahl zwischen keinem, einer
deckenden Platte oder einem Umriss, damit auch weißer oder heller Text auf
hellem Untergrund lesbar bleibt; Platte war bisher die einzige Möglichkeit und
bleibt die Vorgabe. Eine Expertenzeile stellt die Umrissbreite ein, seine
Farbe, schwarz oder weiß, folgt für den besten Kontrast automatisch der
Textfarbe.

Eine ausgewählte Form (Screenshot-Editor) oder Überlagerung (Video-Editor)
lässt sich mit den Pfeiltasten schrittweise verschieben; gedrückt gehaltenes
Strg ändert stattdessen die Größe, Umschalt macht einen größeren Schritt.
Beide Schrittweiten sind Expertenzeilen, die überall dort gelten, wo sie
gebraucht werden. Im Video-Editor übernehmen die Pfeiltasten diese Aufgabe nur,
solange etwas ausgewählt ist, weil sie dort sonst durch die Einzelbilder
schalten; solange ausgewählt, übernehmen stattdessen Komma und Punkt das
Weiterschalten der Einzelbilder, und eine kurze Zeile über der Wiedergabeleiste
zeigt immer, was die Pfeiltasten gerade tun. Escape hebt die Auswahl auf und
gibt den Pfeiltasten ihre gewohnte Aufgabe zurück.

Ein Knopf in der Werkzeugleiste beider Editoren vergleicht mit der
unbearbeiteten Aufnahme: gedrückt gehalten zeigt er das Original, solange er
gehalten wird, ein Klick lässt diese Ansicht stehen, bis erneut geklickt wird.
Dieselbe Taste V tut dasselbe, und eine kleine Marke „Original" erscheint,
solange es zu sehen ist, damit es nie für die Bearbeitung gehalten wird. Im
Video-Editor blendet das nur Überlagerungen aus (Blenden und die Export-
Auflösung zeigt die laufende Vorschau ohnehin nie), der laufende Schnitt
bleibt dabei bestehen, weil ein Sprung an eine herausgeschnittene Stelle nur
verwirren würde. Der Export bleibt davon unberührt.

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

**Rotation und Kette:** zwei verschiedene Dinge. Rotation ändert den Text
EINER Kachel, Kette kopiert MEHRERE verschiedene Kacheln zusammen.

| Ich will... | nutze... |
| --- | --- |
| verschiedene Formulierungen desselben Prompts | Rotation |
| mehrere verschiedene Prompts hintereinander | Kette |

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

**Kopieren** — Verketten (mehrere verschiedene Kacheln als ein Text, anders als
Rotation oben; auch direkt über ein Kettensymbol an der Kachel, einzeln
abschaltbar), Hotkeys pro Kachel, Herausziehen (mit oder, per Schalter, ohne
Modifikatortaste), automatisches Einfügen.

**Hotkeys** — ein globaler Hotkey öffnet den Schnellstarter (Standard
Strg+Alt+P), ein eigener Hotkey pro Kachel kopiert (und fügt optional ein). Bei
einer Kollision gewinnt der App-Hotkey, sonst die erste Kachel in der Liste.
"Alle Hotkeys" neben dem globalen Feld listet jeden Hotkey durchsuchbar auf und
belegt jede Zeile direkt um, ohne den jeweiligen Editor zu öffnen. Eine Kachel
mit eigenem Hotkey zeigt ein kleines Tastatursymbol.

**Schwebende Buttons:** kleine, immer sichtbare Kopier-Buttons außerhalb des
Hauptfensters, über jeder anderen App.

**Ordner:** eine Geste an drei Stellen, ein Ziehen auf ein anderes fasst es zu
einem Ordner zusammen. Ein Ansichts-Reiter auf einen anderen ergibt einen
Gruppen-Reiter. Ein schwebender Button auf einen anderen ergibt einen
Pillen-Ordner. Eine Kachel auf eine andere ergibt einen Raster-Ordner, und
gehört immer nur zu einem Raster-Ordner gleichzeitig.

**Taskleistenmenü** — Zuletzt kopiert, Favoriten. Erscheint als abgerundetes,
zum Thema passendes Fenster statt des einfachen Windows-Menüs; im Expertenmenü
lässt sich bei Bedarf auf das einfache Systemmenü zurückschalten. Dort auch
einstellbar: die maximale Länge eines Eintragsnamens.

**Zwischenablage-Überwachung** — aus, protokolliert bei Bedarf alles Kopierte im
System.

**Kachel-Verhalten & -Aussehen** — Menü, Umsortieren, Hover-Anhebung, Optik.

**Tooltips** — Verzögerung, Ausblenden, Breite.

**Favoriten** — Stern-Markierung, eigene Favoriten-Ansicht.

**Skalierung & Zoom** — UI-, Dialog-, Editor- und Icon-Größe getrennt einstellbar.

**Aussehen & Themes** — sieben mitgelieferte Themes, optionaler Theme-Editor.

**Bibliothek & Suche** — unscharfe Suche, intelligente Sortierung, Filter,
`tag:`/`var:`-Suchpräfixe (mit `-` auch verneint).

**Tags** — freie Schlagwörter, UND/ODER-Filter.

**Kopierverlauf** — Protokoll, Gruppierung, Aufbewahrung, Einzeleintrag löschen,
Statistik mit 14-Tage-Diagramm.

**Medien** — Video-Steuerung, Graustufen/Abdunkeln.

**Werkzeugleiste** — welche Buttons Kopf- und Composer-Leiste zeigen.

**Datenschutz** — Bildschirmaufnahme-Ausschluss (an), Weichzeichnen, Namen
verbergen, Abdunkeln. Der Bildschirmaufnahme-Ausschluss braucht Windows 10 ab
Version 2004 (Build 19041); auf einem älteren Windows wird er still ignoriert
und das Fenster wie jedes andere aufgenommen. Alles in der Datenbank ist mit
der kontogebundenen
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

Kann ein Einstellungs-Speichervorgang selbst nicht auf die Festplatte
geschrieben werden (Datenordner voll, keine Schreibrechte), bleiben Änderungen
für den Rest der Sitzung aktiv, gehen aber beim Neustart verloren. Die App
sagt das genauso: einmal als Hinweis, und als dauerhafte Zeile in dieser
Datenschutz-Gruppe, solange der letzte Speichervorgang fehlschlägt — sie
verschwindet von selbst, sobald wieder erfolgreich gespeichert wird.

**System** — Updates, Tastatur-Navigation, mehrere Instanzen, Datenordner.
Lange Vorgänge (Export, Umwandlung, ffmpeg-Download, Sicherung, Import) füllen
das Taskleistensymbol und zeigen ab einer eingestellten Schwelle „noch etwa …" —
beides abschaltbar.

**Datenwartung** — Duplikatfinder, Papierkorb, Import/Export. Der Duplikatfinder
hat zusätzlich eine Abkürzung im Einstellungsdialog, direkt neben Import und
Export, damit man dafür nicht erst das Expertenmenü öffnen muss.

**Papierkorb & Kacheln wiederherstellen** — gelöschte Kacheln landen zuerst im
Papierkorb (Aufbewahrung in Tagen, 0 = für immer). Wiederherstellen legt in die
erste freie Zelle der aktiven Ansicht, nicht zwingend an den Originalplatz, und
ohne Pille, Ordner-Mitgliedschaft oder Hotkey von vorher.

**Duplikatfinder** — findet textlich sehr ähnliche Kacheln nach
Ähnlichkeitswert, keine exakte Übereinstimmung nötig, löscht nichts von sich
aus. Eine wechselnde Kachel wird nur nach ihrem ersten Eintrag verglichen.

**Import & Export** — CSV/TXT nur für den reinen Text, volle Sicherung für
alles. Ein CSV/TXT-Import zeigt vorher eine Vorschau mit Name und
Duplikat-Kennzeichnung; eine volle Sicherung stellt direkt wieder her. Ändert
sich die Datei nach der Vorschau, wird der Import verweigert statt etwas
anderes zu importieren als angezeigt.

**Markdown-Ordner-Sync (Beta)** — spiegelt jede Text-Kachel als lesbare
`.md`-Datei in einem gewählten Ordner, inklusive Ansicht, Rasterzelle und
Ordner. Angehängte Dateien, Bilder und PDFs wandern als Verweis mit (Pfad und
PDF-Seite im Kopf, nie die Datei selbst); ein direkt eingefügter Screenshot hat
keinen Pfad und bleibt nur in der Datenbank. Eine in einer eingehenden Datei
genannte Ansicht oder ein Ordner, den es auf diesem Rechner noch nicht gibt,
wird automatisch angelegt, per Namensabgleich. **Die Dateien sind Klartext** —
anders als die Datenbank, also kein synchronisierter oder geteilter Ordner für
Sensibles. Im Expertenmenü lässt sich einstellen, wie oft der Sync-Ordner auf
außerhalb der App bearbeitete Dateien geprüft wird.

Der Ordner ist `<Datenordner>\prompt-saver-sync`, solange kein eigener gewählt
ist; wer schon synchronisiert, behält seinen bisherigen Ordner. Eine Datei wird
erst gelesen, wenn sie sich nicht mehr ändert, eine Änderung von außen kann also
einen Durchlauf länger brauchen. Was in welche Richtung passiert, steht fest und
geschieht nie stillschweigend:

- **Eine im Ordner gelöschte Datei löscht die Kachel nicht.** Sie wird im
  nächsten Durchlauf neu geschrieben. Wer die Kachel in der App löscht, löscht
  auch die Datei.
- **Umbenennen erzeugt eine zweite Kachel.** Der Dateiname ist die feste Kennung
  der Kachel; die umbenannte Datei kommt als neue Kachel an, das Original wird
  unter seinem eigenen Namen neu geschrieben. Also in der App umbenennen.
- **Kachel und Datei gleichzeitig geändert?** In diesem Durchlauf gewinnt die
  Datei, und die ersetzte Fassung landet in der Versionsgeschichte der Kachel.
  Umgekehrt gewinnt die App die Datei, und der Dateiinhalt bleibt daneben als
  `<Kennung>.conflict-<Zahl>.md` liegen.
- **Dateien, die diese App nicht geschrieben hat, werden nie angefasst**, in
  keiner Richtung, und eine Datei über 8 MB wird ganz übersprungen, nie
  abgeschnitten. Das Fehlerprotokoll sagt warum.
- **Ein Ordnerwechsel fragt nach**, was mit dem alten Ordner geschehen soll:
  löschen oder behalten. Gelöscht werden nur die `.md`-Dateien, die diese App
  selbst geschrieben hat, nie ein Unterordner, und der Ordner selbst nur, wenn er
  danach leer ist. Liegt eine fremde Datei darin, wird gar nichts gelöscht und
  eine Meldung nennt, was im Weg liegt.
- **Ein nicht erreichbarer Ordner pausiert die Synchronisierung**, statt alle
  paar Sekunden zu scheitern (nach fünf Fehlversuchen, einstellbar). Die Zeile
  unter dem Ordner im Expertenmenü nennt den letzten gelesenen Stand, die Zahl
  der Dateien und den Grund der Pause.

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
