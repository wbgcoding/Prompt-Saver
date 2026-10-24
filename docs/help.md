# Help

The same content shown in the app's own **Help** menu (Settings → Help), kept here so
it's readable without installing anything. English and German only. The in-app
version follows the same rule and falls back to English in every other language.

## Basics

### Getting started

A tile is a saved item on the grid. Double-click an empty grid cell (or type into
the box at the bottom and press Save) to create one, click any tile to copy it. Three sample tiles (an email with
placeholders, an image, and a Markdown template) are already there on first launch;
a button above the grid removes them with one click once they're no longer needed.

### Settings

The gear button in the header opens the settings window with the everyday
choices; everything optional lives in the expert menu, opened from the same
window. It holds the language, the theme, the text size and font of the tiles,
whether the window stays on top, starts with Windows, starts in the background
and goes to the background instead of quitting when closed (on by default for a
new install; a portable copy always quits), the global hotkey,
auto-paste on double-click, and whether screenshots always go to the screenshots
folder (section Capture). Every change applies at once, with no save button. "Automatic" as the
language follows Windows and falls back to English. Reset puts every setting
back to its default and keeps the tiles; Delete removes all data and asks first.

### Introduction tour

A four-step walkthrough shown once on first launch, right after the sample tiles
appear. It changes no setting. Escape or "Skip" closes it at once; after Escape an
Undo notice reopens it. Reopen it anytime from the expert menu.

### The grid

The main window's grid is a free-placement board of tiles, up to 20×20 cells by
default (adjustable to 200×200 in the expert menu), with its own layout per view.
Drag a tile to any empty cell, or drop it on an occupied one to swap the two; the
grid never rearranges itself on its own. Drop a tile on a view tab above the grid
to move it into that view instead, or hold Ctrl while dropping to copy it there
so it stays in both. Change the grid size by
right-clicking a view tab or an empty spot of the grid and choosing Grid size.

### Creating tiles

Beyond plain text, a tile can carry an attached file, a screenshot, or a picked
screen color.

- File attach: clicking the tile copies the file itself, not text
- Screenshot tool: capture a region or a whole window straight into a new tile
- Color picker: pick any on-screen pixel, copies as hex/RGB/HSL
- Drag and drop: drop a file or image straight onto the grid

### The editor

The editor is a plain text box at heart; a few features (**Markdown preview**,
**private notes**: both on by default, switchable in the expert menu) add structure and versions to it:
a rendered Markdown preview with syntax-highlighted code blocks, a full list of
versions, and a private note field never copied or exported with the text.

For a file/PDF/video tile that also carries its own stored preview picture, the
image shown above the file row is only that tile's preview; the attached file
below it is a separate thing, and copying the tile puts the file's path on the
clipboard, never the picture.

A plain text, Markdown, JSON or CSV attachment shows its own start right below
the file row (switchable off in the expert menu, how many kilobytes it reads
is adjustable there too). It always shows as plain text, never rendered, so a
Markdown file with real formatting in it still shows its raw source.

The open editor saves its own text as a draft every few seconds (on by
default, the interval adjustable in the expert menu), so a crash or a closed
window never loses unsaved work. Only the text is drafted, never an attached
image, and a save or a cancel removes the draft right away. If a draft is
still there the next time the app starts, it asks once whether to restore it.

### The expert menu

Every optional feature lives behind an on/off switch, with its own parameters
right underneath once it's on. The search box scans every tab at once; a feature
that ships switched off carries a small dot badge. Reset works at three levels:
one value's own undo icon, a whole group's reset, or everything at once with
the "Reset" button at the end of the System tab (your prompts stay).

### Keyboard-only use

Arrow keys move a focus ring across the grid; Enter or Space copies whichever
tile is focused. The ring fades out a short while after the window loses focus.

### Common questions

- *My screenshot of this app came out empty*: the screen-capture exclusion
  privacy setting is working as intended; switch on "Allow screenshots of
  Prompt Saver" in the expert menu if that's not wanted
- *The app won't start*: it needs the Microsoft WebView2 runtime; the installer
  offers it automatically if missing
- *I can't find a setting*: check the expert menu's search box first; it scans
  every tab at once
- *A hotkey doesn't do anything*: another app may already hold that combination;
  the editor explains this instead of accepting it silently

## Features

### Copying

- Selecting tiles: right-click a tile (or Ctrl+click) to select it; while
  selecting, every tile shows its checkbox. The tile menu opens with a right-click.
- Chaining: not the same as rotation (see Rotation above). Several
  DIFFERENT tiles copied together as one text, rather than one tile changing
  what it holds. Click several tiles in a row, they combine into one clipboard
  copy. Start chain here: right-click a text tile to start a chain with that
  tile as the first entry directly, switchable off on its own in the expert
  menu under Chain, alongside the rest of the chain settings, including what goes between two
  chained tiles (a blank line unless changed). A chain stays when you
  switch to another program. The chain button in the top bar starts and ends
  a chain. Pasting with Ctrl+V in the app window ends it too.
- Chain overview: while chaining, a strip above the grid lists the picked
  steps in order, with small marks for the placeholders each step needs and
  the reserved names it fills by itself. A step turns red when nothing
  earlier in the chain answers a placeholder it needs, though the chain can
  still be started. Drag a row to reorder the chain right there, right-click a row to
  remove that step; the clipboard follows either change. Switchable
  off on its own in the expert menu.
- Chain stays on after copying: with it on, chain mode stays on after copying, for
  building several chains in a row.
- Per-tile hotkeys: copy a specific tile from anywhere, without opening the app
- Drag-out: hold the modifier key and drag a tile onto another window to paste
  directly, no clipboard round-trip. An expert switch swaps the two gestures, so a
  plain drag leaves the app and the key moves the tile instead.
- Auto-paste: after copying, the app switches to the last active window and
  presses paste automatically
- Copy formatted: a second copy entry, in the tile's right-click menu and in
  the editor, only offered for a prompt whose text actually contains Markdown
  syntax. It puts the rendered version on the clipboard alongside the plain
  Markdown text, so an app that reads rich text (Word, Outlook, a browser)
  shows the formatting, while a plain text field still gets exactly the
  Markdown source a normal copy would have given it

<p>
<svg viewBox="0 0 560 130" xmlns="http://www.w3.org/2000/svg" role="img"><rect x="20" y="15" width="110" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="75" y="36" font-size="11" fill="currentColor" text-anchor="middle">Create prompt</text><line x1="130" y1="32" x2="187" y2="32" stroke="currentColor" stroke-width="1.5"/><path d="M 185 28 L 195 32 L 185 36" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><rect x="195" y="15" width="170" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="280" y="36" font-size="11" fill="currentColor" text-anchor="middle">Copy</text><line x1="365" y1="32" x2="422" y2="32" stroke="currentColor" stroke-width="1.5"/><path d="M 420 28 L 430 32 L 420 36" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><rect x="430" y="15" width="110" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="485" y="36" font-size="11" fill="currentColor" text-anchor="middle">Paste</text><line x1="280" y1="49" x2="280" y2="70" stroke="currentColor" stroke-width="1" stroke-dasharray="2 2"/><rect x="60" y="78" width="115" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="117" y="99" font-size="10" fill="currentColor" text-anchor="middle">Tile</text><rect x="190" y="78" width="115" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="247" y="99" font-size="10" fill="currentColor" text-anchor="middle">Hotkey</text><rect x="320" y="78" width="115" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="377" y="99" font-size="10" fill="currentColor" text-anchor="middle">Floating button</text><rect x="450" y="78" width="115" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="507" y="99" font-size="10" fill="currentColor" text-anchor="middle">Tray</text></svg>
</p>

<p>
<svg viewBox="0 0 560 90" xmlns="http://www.w3.org/2000/svg" role="img"><rect x="30" y="20" width="100" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="80" y="41" font-size="11" fill="currentColor" text-anchor="middle">Step 1</text><rect x="230" y="20" width="100" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="280" y="41" font-size="11" fill="currentColor" text-anchor="middle">Step 2</text><rect x="430" y="20" width="100" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="480" y="41" font-size="11" fill="currentColor" text-anchor="middle">Step 3</text><line x1="130" y1="37" x2="217" y2="37" stroke="currentColor" stroke-width="1.5"/><path d="M 215 33 L 225 37 L 215 41" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><text x="177" y="25" font-size="10" fill="currentColor" text-anchor="middle">{?X}</text><line x1="330" y1="37" x2="417" y2="37" stroke="currentColor" stroke-width="1.5"/><path d="M 415 33 L 425 37 L 415 41" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><text x="377" y="25" font-size="10" fill="currentColor" text-anchor="middle">{?Y}</text></svg>
</p>

### Placeholders

Placeholder markup inside a tile's text opens a small popup asking for each value
before the finished text reaches the clipboard. A placeholder is written
`{?Name}`; `{?Name=default}` prefills a value and `{?Name|A, B}` offers a
dropdown. Prompts written with the older {#{Name}#} spelling are converted
automatically.

- Dynamic placeholders: `{?@date}`, `{?@time}`, `{?@clipboard}` fill
  themselves in, no popup; the `@` in front (not the plain `{?date}` form,
  which stays a placeholder of your own) is what triggers it; insert them from
  the chip buttons under the editor's text box rather than typing them by hand
- Choice placeholders: a dropdown of preset values instead of free text
- Remember choices: the popup pre-fills the last answer for that tile
- Fixed values (off by default, expert menu): a maintained list of name and
  value pairs the popup suggests for a matching placeholder, one step below a
  remembered last input and above the text's own default. Shown only as the
  field's placeholder text, marked with a small pin, never inserted until it
  is typed or picked.

<p>
<svg viewBox="0 0 560 130" xmlns="http://www.w3.org/2000/svg" role="img"><text x="30" y="45" font-size="15" fill="currentColor" text-anchor="start">{?Kunde=Meier|Meier, Schulz}</text><line x1="57" y1="55" x2="57" y2="80" stroke="currentColor" stroke-width="1"/><text x="57" y="95" font-size="11" fill="currentColor" text-anchor="middle">Name</text><line x1="110" y1="55" x2="142" y2="80" stroke="currentColor" stroke-width="1"/><text x="142" y="95" font-size="11" fill="currentColor" text-anchor="middle">Default</text><line x1="232" y1="55" x2="252" y2="80" stroke="currentColor" stroke-width="1"/><text x="252" y="95" font-size="11" fill="currentColor" text-anchor="middle">Choice list</text></svg>
</p>

### Hotkeys

Two independent kinds: one global combination that opens the quick launcher
(default Ctrl+Alt+P), and a combination per tile that copies, and optionally
pastes, that one tile directly. A combination must include Ctrl or Alt; the
app hotkey always wins a conflict, and among tiles the first one in the list
keeps a combination two tiles happen to share. Importing a backup that carries
a colliding hotkey drops the losing one instead of failing the import.

"All shortcuts", next to the global hotkey field in Settings, opens the shortcuts dialog with every hotkey,
global and per-tile, in one searchable, live-rebindable list, so rebinding or
clearing one no longer means opening that tile's editor.

A tile with its own hotkey shows a small keyboard symbol.

### Rotation

Not the same as chaining (see Copying below): rotation changes the text inside
ONE tile from copy to copy, chaining copies several DIFFERENT tiles together
as one text.

| I want... | use... |
| --- | --- |
| several wordings of the same prompt | rotation |
| several different prompts, one after another | chaining |

A tile can hold a list of texts instead of one: each copy serves the next rotation
text, in order or shuffled without repeats, instead of always the same text. On by
default; the per-tile switch is in the editor under Advanced options.

- A small badge counts up (like "2/5") on a tile, floating button, or desktop folder
  member; a grid folder member shows only its file-type icon instead. In order
  mode it is the absolute position; in shuffle mode it counts draws within the
  current cycle and restarts at 1 once every entry has been shown once, rather
  than jumping to an arbitrary position
- The count survives restarts and full backups; a CSV/TXT import starts a
  fresh pointer, since the imported tile gets a new id

An eligible tile (2+ entries, no image/file attached) can also **rotate on
paste**: switch it on with the "Rotate on paste" button next to the order/random control, or
from a tile's right-click menu once both the per-tile and the app-wide switch
allow it. With rotate on paste on, the next text loads on every real Ctrl+V or
right-click-Paste, not only when copying again; the badge picks up an accent
while a session is running. Placeholders still work; the popup asks once per
session and the same values are reused for every rotation text. Five expert-menu
settings (all under the same toggle) tune the session timeout and the timing
window that tells a real paste apart from other clipboard activity; pasting too
fast too often pauses the session with a toast instead of looping.

### Floating buttons

Small always-on-top copy buttons that sit outside the main window, over
whatever app is in use.

A button whose saved screen is no longer connected opens on the main screen,
marked as parked, and jumps back on its own once that screen returns. An action
in the expert menu brings every floating window to the main screen at once.

### Folders

One gesture works in two places: drop one onto another to gather it into a
folder. A floating
button dropped onto another makes a folder on the desktop. A tile dropped onto
another tile makes a folder in the grid, and belongs to only one grid folder at a
time. A folder lives where it was made.

<p>
<svg viewBox="200 0 360 120" xmlns="http://www.w3.org/2000/svg" role="img"><circle cx="255" cy="45" r="16" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="295" cy="45" r="16" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="335" cy="45" r="16" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="295" y="108" font-size="11" fill="currentColor" text-anchor="middle">Desktop folder</text><rect x="430" y="20" width="100" height="70" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><line x1="430" y1="55" x2="530" y2="55" stroke="currentColor" stroke-width="1"/><line x1="480" y1="20" x2="480" y2="90" stroke="currentColor" stroke-width="1"/><text x="480" y="108" font-size="11" fill="currentColor" text-anchor="middle">Grid folder</text></svg>
</p>

### Tray

The system tray icon's right-click menu offers a "recently copied" and a
"favorites" section for quick access without opening the main window. It draws
as a rounded, themed window matching the rest of the app rather than the plain
Windows popup; the expert menu can switch back to the plain system menu if the
custom one ever misbehaves on your setup.

Typing in the tray menu's search field opens the quick launcher with what you
typed. Arrow keys and Enter on the menu's own entries work as before. The field
ships on.

### Quick launcher

A small always-on-top popup (default hotkey Ctrl+Alt+P) with fuzzy search across
every tile's name and text; pick one and it copies immediately, no need to bring
the main window forward. The expert menu sets how many results it lists (8 by
default). A line at the bottom lists its keys.

### Multiple views

A view is one grid page with its own size and arrangement; a tile can live in
several views.

### Voice dictation

On by default. A microphone button on the composer and in the edit dialog turns
speech into text using Windows' own on-device recognizer; nothing is uploaded
unless online recognition is explicitly allowed (off by default; a one-time
dialog asks the first time it's actually needed). Click to start: the button
turns the accent color and pulses while listening, and its title switches from
"Dictate" to "Stop dictating"; a second click, Escape in the field, saving the
prompt, or closing the window all end the session the same way. Text lands at
the cursor (or always at the end, an expert setting) and can stop itself after
a few seconds of silence.

Spoken punctuation and simple commands are turned into real characters unless
Windows already placed them itself: "comma", "period", "question mark",
"exclamation mark", "colon", "semicolon", "hyphen", "dash", "slash", "quote",
"open paren"/"close paren" and "percent" for punctuation, "new line", "new
paragraph" and "tab" for layout, each glued to the word before it with no
extra space. Saying "literal" dictates the very next word as-is, instead of
matching it as a command, useful for actually saying "comma" out loud. The
word right after a period, question mark, exclamation mark or paragraph break
is capitalized automatically. **The exact word matters**: only these specific
command words (in the app's own language) are recognized; anything else is
just dictated as plain text.

Like OCR, it needs a Windows language pack: see "Language packs" below, same
dialog either way. A global hotkey (off by default, in the expert menu) can
start or stop a session from anywhere without bringing the window forward;
the finished text goes to the clipboard, is pasted into whatever app was
active, or lands in the composer, whichever the setting picks.

### Language packs

Text recognition and voice dictation both use Windows' own language packs and
share one install dialog. The recognition-language dropdown offers a download
the first time a chosen language has no pack installed; installing needs
administrator rights, and Windows asks for them once through its own
confirmation prompt. The dropdown's "Manage text recognition languages…" entry ("Manage
dictation languages…" for dictation) reopens that
same dialog any time, listing what's installed and what's available; on most
systems listing needs no special rights, but on some Windows builds it does
too, and the dialog offers to run the listing itself with administrator
rights instead of coming up blank; that step only lists packs, it installs
nothing. A background check flags a pack whose version changed or that a
Windows update silently removed, without asking to reinstall right away:
the notice's "Cancel" button dismisses it for a while, or the background check can be
switched off entirely.

### Markdown folder sync (beta)

An optional, off-by-default mirror that writes every text tile as its own
readable `.md` file in a chosen folder, alongside the encrypted database,
including which view, grid cell, and folder it sits in. A view or folder named in
an incoming file that does not exist yet on this machine is created
automatically, matched by name.
**The `.md` files are plain text**: unlike the database, anyone with access to
that folder can read the content, so a synced or shared folder should be avoided
for anything sensitive. A tile with an attached file, image, or PDF syncs as a
reference: the header carries the path and the PDF page, never the file itself. A
screenshot pasted straight into a tile has no path and stays in the database only.

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
  the version it replaced is kept in that tile's versions. The other way
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
  (after five failed rounds). The line under the folder in the expert
  menu says when the folder was last read, how many files were in it, and why it
  is paused.

## Capture

### Screenshot tool

Captures part or all of the screen straight into a new tile. Drag a region, or
click a window to capture it whole, even a normally-protected one like the Task
Manager. The app's own windows are hidden for the moment of capture so they never
end up in the shot; the confirmation dialog then offers saving it as a tile, saving
it as a file, or leaving it on the clipboard.

Before that dialog, an editor opens on the capture (on by default): crop, a
freehand pen and highlighter marker, straight lines and arrows, rectangles and
ellipses, text labels, numbered step markers, a pixelate tool for blurring out
sensitive details, and a focus area that dims everything outside a chosen area,
with undo/redo. Zoom in for pixel-precise work and pan around with the Hand
tool (or hold Space, or the middle mouse button) while zoomed in. Drag the
edges or corners of the frame to crop. Resize
changes the image's actual pixel size (not just the view): set a width,
height, or percentage, with presets for 25/50/200% and "fit 1920 wide"; it is
its own undoable step, so scaling back and forth never loses quality beyond
what the last resize itself introduced.

Both editors can also open a file you already have, not just a fresh
capture or recording: right-click the screenshot or record button and choose
"Open a file in the editor", or right-click an image or video tile or library
row and choose Edit image or Edit video. The content of the file decides which editor opens, no matter what
its extension says. The editor always works on a copy, so the original file
is never changed; saving asks where to put the result, starting in the
original file's own folder.

### Screen recording

Records a window, a monitor, or a dragged region as a video, via the record
button next to the screenshot tool. A region recording shows a thin frame
around the recorded area; it never appears in the video, and the expert menu
can switch it off or change its color. MP4 (H.264) is always what gets written
while recording; three quality steps trade file size for detail before you
start:

- **Small**: a compact bitrate (4000 kbps by default), for quick clips.
- **High** (the default): a higher bitrate (12000 kbps by default), for a
  sharper picture.
- **Lossless**: recorded at the same native bitrate as your current step,
  then automatically re-encoded losslessly once the recording stops. This can
  run into several gigabytes for longer recordings.

A bitrate slider next to the quality step lets you fine-tune small/high
further; picking a step resets the slider to that step's own default, and the
slider is then free to adjust. The slider has no effect on the lossless step,
since what actually makes that step lossless is the re-encode afterward, not
the recording bitrate.

A format setting picks what the finished file ends up as. Recordings and exports come as MP4, WebM, GIF or AVI. WebM and AVI are converted from the MP4; GIF is capped at 30 seconds and has no sound. Converting to anything other than
MP4, or to lossless MP4, needs ffmpeg (a separate, GPL-licensed tool the app
does not ship with its portable build). If it is missing, the format and
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
A small floating control bar then follows the recording: a red dot, elapsed
time, live file size, two audio buttons, Pause/Resume, Stop and a discard
(cancel) button. It follows a moved recorded window and is itself excluded
from the recording no matter what, even if the recording happens to include
the monitor the bar sits on. Drag the bar itself (anywhere except its
buttons) to move it, including onto another monitor; the moment you do, it
stops following the recorded window and stays exactly where you put it for
the rest of that one recording. The next recording starts following again.

System audio and microphone audio (pick a device in the expert menu) both
record on by default, each into its own track. The microphone track is
captured but not mixed in audibly; it only becomes part of what you hear
once you bring it in yourself, from the video editor below. Turn microphone
capture off in the expert menu if you would rather it never records at all.
If the microphone is unplugged mid-recording, the picture and system audio
keep going; the control bar's microphone icon switches to a "lost" state
instead of stopping anything. The bar's two audio buttons mute either source
instantly while recording, without cutting the track: a muted source keeps
recording silence instead of dropping out, so it can be brought back later
in the same recording, and the bar reopens showing whichever state you left
each one in.

A running recording can also be stopped or paused/resumed from anywhere via
two optional global hotkeys (set in the expert menu, live only while a
recording is actually running) and from the tray menu; both hotkeys live in
the expert menu's own recording group and, once set, also show up in "All
hotkeys" alongside the global and per-tile ones.

The expert menu's recording-folder row shows how many videos are in it and
how much space they use, with a "Clean up…" button next to it: pick 7, 30 or
90 days, review the exact list of files that age would remove and how much
it would free, then confirm. Nothing is ever deleted in the background;
only this dialog, and only after you confirm it, ever removes a file.

Stopping leads into a dialog offering Save, Save as…, Copy, Edit and Discard.
Save and Copy use one shared recording folder (changeable here or in the
expert menu); Edit opens the clip in the video editor below without deciding
its fate yet; closing that dialog afterward without ever saving or copying
still discards the file, exactly as if Edit had never been clicked. If the
chosen format or quality step needs converting the MP4 first, the dialog
shows a short "Converting…" status instead of a preview (click it to cancel);
a conversion failure or cancellation always falls back to the untouched
original recording rather than losing it. If the window being recorded
closes on its own, the recording stops automatically and keeps whatever was
captured up to that point.

A recording that never reached its own stop path (a crash, a forced
shutdown) leaves behind a file with no usable video track. The app checks
for these in the background and removes them once they clear a short grace
period, so a just-stopped recording that is still finalising is never
touched; both the grace period and the check interval are adjustable in the
expert menu (System tab, Troubleshooting group), alongside a manual "clean up now" action. A new install deletes
unused recordings after 90 days (expert menu, 0 = keep forever).

The same cleanup dialog can also export several finished recordings at once.
A checkbox per file, plus Select all/none, picks which ones; "Export
selected" opens a small dialog for format, quality and a destination folder,
then encodes every checked recording and reports how many finished and how
many failed.

An optional keys overlay (off by default, in the expert menu) records which
keys are pressed anywhere on the screen while a recording runs, so a tutorial
can show them afterward with the video editor's own "Keys" overlay. Only key
identifiers and their timestamps are ever recorded, never any text. By
default every key is shown, including typed text; a focused password field is
skipped for as long as it stays focused and shows as a gap instead, though
this is not guaranteed, since it relies on Windows correctly reporting the
field as a password field. A "Shortcuts only" switch (in the expert menu,
next to the keys overlay itself) restricts capture to combinations with
Ctrl, Alt or Win plus Enter, Tab, Esc, Backspace, Delete, Insert, arrows,
Home, End, Page Up, Page Down and F keys, letters and digits never included,
for a recording with confidential input.

An optional camera recording (off by default, in the expert menu) adds a
"Record camera" entry to the record button's right-click menu. It opens its
own small window with a live preview, its own Start/Pause/Stop, an audio
source, a resolution choice and a target folder: the same controls the
screen recorder itself has. A camera-shaped window with no visible preview
or indicator never happens: the window itself, or its status dot in the
recording bar, always shows while the camera is live. It records to its own
file, never burned into a screen recording, so either can be repositioned
later. If a screen recording is running at the same time, both share the
same start point; the video editor then opens on the screen recording with
the camera laid on top as an aligned overlay once both are finished. With no
screen recording running, the camera take is its own zero point and opens on
its own. Default camera, resolution and frame rate are all adjustable in the
expert menu.

### Video editor

On by default, reachable from a recording's own result dialog (More, then
Video editor) or from any video prompt's right-click menu ("Edit video"). It
opens the clip in its own window with a live preview and no processing until
you actually export: trim and cut (frame-exact, keeps the original quality
when nothing else changes), crop, change speed, fade in/out, resize (typed
width/height or a percentage, an aspect-ratio lock, and quick presets from
4K down to 25%; a preset larger than the source enlarges the video and says
so, one equal to the source height keeps the original), and overlay images,
freeze-frames, text, pixelation, a focus area or a keys overlay (four
keyboard-layout presets plus a free selection of keys, filled from a
recording's own key data if it captured any) anywhere on the timeline. A
filmstrip and waveform give you a zoomable overview to navigate by. The preview itself also zooms (scroll wheel, up to 8×) and pans by
dragging (the middle mouse button always, the left button once zoomed in)
for placing a small overlay precisely on a large recording. Exporting
re-encodes once, with a few ready-made presets (unchanged
quality, a smaller size for chat, or an animated GIF) that remember your last
choice. The format choice offers MP4, WebM, GIF and AVI, each with a short
note on what it's good for; GIF carries no sound at all (the source audio
stays untouched in the original recording) and warns past ten seconds, since
it grows large fast. The export destination is an editable field, not just a fixed name next
to the source file: type into it directly, or use "Export to…" to keep the
file name and only choose a different folder; once an export has finished,
"Save as…" moves that finished video to another place. Changing the format only
rewrites the extension, never a folder or name you already typed. The chosen folder is remembered as
the default for next time. The "Copy" button next to it skips naming anything
at all: it exports into a copy that cleans itself up on its own (after an hour,
and again on the next launch) and puts the
finished file straight on the clipboard for pasting into another app, the way
the recording dialog's own Copy button does. Undo/redo covers every editing
step. A failed export shows the
actual reason in plain language, with the raw ffmpeg log reachable underneath
in a closed "Technical details" row, never the log itself as the message. A
second video can be added too: "Add
a video" offers it either as another overlay (its own rectangle, time range
and volume, muted by default) or as a whole extra section spliced in at the
start, the end or the current position; each extra source is checked once
when picked and stays visibly marked as missing if it later moves or is
deleted, up to a limit per project (eight by default, adjustable in the
expert menu).

Both editors work the same way for their shared tools: Ctrl+Plus, Ctrl+Minus
and Ctrl+0 zoom in, zoom out and fit the view in either one, and Ctrl+Z,
Ctrl+Y (or Ctrl+Shift+Z) undo and redo through the same button pair. In the
screenshot editor, clicking an existing rectangle, ellipse, line or arrow
selects it and shows the same handles as dragging one out fresh. The video
editor does the same for its overlays: clicking one in the preview selects it
(the top-most one showing at the playhead) and a drag moves it, a click on an
empty spot deselects, and the overlay list still works as before; the two
toolbars stay where they are, a bar on top for the screenshot editor and
a side column next to the timeline for the video editor, because the
timeline needs the room the screenshot editor doesn't. In the screenshot
editor, undo, redo and zoom sit at the left end of the bottom bar.

Right-clicking a shape (screenshot editor) or an overlay's row (video editor)
opens a small menu to duplicate or delete it. A duplicate keeps the same
values and time range and is offset a little so it sits next to the
original instead of exactly on top of it. In the video editor, the color, thickness, opacity
and font size a new overlay starts with also follow whatever was last used
for that same kind (image, frame, text, pixelation or focus area), across
restarts too.

Dragging a shape (screenshot editor) or a crop box or overlay (video editor)
snaps to the image or frame edges, its centre in both directions, and the
edges and centre of every other shape or overlay already placed, once the
pointer gets close enough. A thin accent-colored line marks which one it
caught, and disappears the instant the drag ends. Holding Alt while dragging
turns snapping off for that one drag. Each editor has its own switch for this,
sharing one distance setting between them.

The text tool's background is a choice of none, white, a translucent plate or
an outline, so the text stays readable on any background; black text on a
white box is the default, and the text box in the editor is only as wide as
what you type. An expert setting
picks the outline width, and its color, black or white, follows the text
color automatically for the best contrast.

A selected shape (screenshot editor) or overlay (video editor) can be nudged
with the arrow keys, a step at a time; holding Ctrl changes its size instead
of its position, and Shift takes a bigger step. In the video editor the arrow
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

<p>
<svg viewBox="0 0 560 130" xmlns="http://www.w3.org/2000/svg" role="img"><rect x="60" y="15" width="120" height="30" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="120" y="34" font-size="10" fill="currentColor" text-anchor="middle">Overlay</text><rect x="20" y="55" width="520" height="22" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><line x1="180" y1="55" x2="180" y2="77" stroke="currentColor" stroke-width="1.5"/><line x1="340" y1="55" x2="340" y2="77" stroke="currentColor" stroke-width="1.5"/><text x="180" y="95" font-size="10" fill="currentColor" text-anchor="middle">Cut mark</text><path d="M 20 115 q 15 -15 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0" fill="none" stroke="currentColor" stroke-width="1"/><text x="480" y="95" font-size="10" fill="currentColor" text-anchor="middle">Audio track</text><text x="290" y="12" font-size="10" fill="currentColor" text-anchor="middle">Timeline</text></svg>
</p>

### Text recognition (OCR)

On by default. A button on the composer and in the screenshot editor pulls the
text out of a screenshot or an image file, entirely on this device via Windows'
own text recognizer; nothing is uploaded or sent anywhere. The composer button
opens a small menu: from an image file, or from a screenshot you take right now.
It needs a Windows language pack: see "Language packs" below for how that
install works. Recognized text can auto-copy to the clipboard and auto-join
wrapped lines into full sentences, both adjustable.

### Color picker & pick history

A pipette that freezes the screen, magnifies the pixel under the cursor, and copies
it as hex, rgb, or hsl. Right-click the pipette icon to reopen the last few picks;
the history is mirrored into settings, so it survives a restart too.

## Tiles

### Tile behavior & appearance

Tile menu, drag-to-reorder, hover lift, type badges, shadows, rounded corners and
one tile style (bold, uppercase, monospace, italic, shadowed names, compact,
gradient): behavior and pure looks, all adjustable.
Tile, folder, view-tab and message text on a colored area picks black or white
automatically; the expert menu can switch this off (always white) or move the
brightness cutoff where black text starts. White text on a light color gets a
dark shadow behind it so it stays readable; the expert menu can switch that
shadow off or dial its strength.

### Media

Video controls, autoplay and loop (both off by default), mute, plus a blur-until-hovered look for
privacy-leaning media tiles. The library and copy history also show a video's
length on its thumbnail and its resolution and file size in the tooltip; older
attachments pick this up in the background the first time they are shown. The
main grid stays as it always was.

### Favorites

A star marks a tile as a favorite; an optional dedicated view lists only
starred tiles. Use "Add to favorites" from a tile's right-click menu, or the
star in the library. A favorite tile shows a small star in its top-right corner.

### Tooltips

Hovering a tile shows its full text, length, and (if present) its private note.
The delay and the width are adjustable.

## Appearance

### Appearance & themes

Six built-in themes: system/light, dark, Programmer, Neon Violet, Ocean, and
Cyberpunk. The theme editor adjusts any theme's colors and
saves the result as a custom theme.

### Theme editor

Starts from any built-in theme, changes individual colors, and saves the result
as a new theme; the original stays untouched. Open it with "Custom theme…" at the
end of the theme list in Settings.

### Scaling & zoom

Independent size sliders for the overall UI and the icons.
Interface size is in Settings; icon size is in
Expert menu → Appearance → Visual.

## Library & copy history

### Library & search

The library lists every tile as a searchable, filterable list: fuzzy search,
smart sort (most-used / most-recent), and the type filter. Type -word to
exclude matches, or tag:word / var:word to search only tags or placeholder
names. A bookmark button inside the search field saves the current query, tag filters
and sort as a chip shown above the field; click it to bring the whole search
back. Right-click a saved search to rename or delete it, or drag it to
reorder the row. Up to five can be saved at once. The Tiles button next to the sort order switches the list to
tiles: a big preview picture with the name below, nothing else. Click,
double-click, the right-click menu and multi-select all work the same as in
the list, it is the same selection drawn differently. The toggle remembers
its last choice per type filter, so switching to "only images" brings tiles
back on its own next time.

### Search everything

The magnifier icon in the toolbar, or Ctrl+K, opens an overlay that searches
prompts, copy history, attachments, views, settings and help all at once,
each in its own group with up to eight hits and a "Show … more". Arrow keys
move the highlight across group boundaries; Enter runs the fitting action for
whatever is highlighted: copies a prompt or copy history entry, opens an
attachment, switches to a view, opens and highlights a setting, or opens the
matching help topic. The field remembers your recent searches (eight by
default; the expert menu sets one number for every search list) and shows them when it
is empty. Escape, a click outside the card, or the icon again
all close it. A flag in the expert menu turns it off.

### Tags

Free-form labels assigned to tiles; filter the library or the grid down to a set
of tags, matching any or all of them. The archive is one of them, an ordinary
tag with a reserved name: "Archive" in a tile's right-click menu (or the
library's batch bar) gives a tile that tag, and it disappears from the library
list and the grid until the archive chip above the list shows it again. Export,
backup and search still treat it like any other tile. "Unarchive" removes the
tag again.

### Copy history

A tab of the library (Library → Copy history): a log of what was copied and when, through the app and, if "Record copies from
other apps" is on, elsewhere. Grouped view, automatic retention
(30 days for a new install, adjustable, 0 keeps everything),
and per-entry delete.
The copy history is one list; the sort field switches between Recently copied and Most used (top 20).

### Copies from other apps

Off by default. When switched on, text copied anywhere on the system (not just
from this app) is also recorded in the copy history, marked as copied elsewhere;
the source filter in Library → Copy history shows only those, and a useful one
can be saved as a tile. A minimum-length filter and a cap keep the list from
growing unbounded.

## System & data

### System

Update checks (never installs without asking), keyboard navigation, and where
the data folder lives; allowing multiple instances at once sits in the
Troubleshooting group. A long export,
conversion, ffmpeg download, backup or import fills the app's taskbar icon
with its progress, and shows a plain "about X left" estimate once enough of
it has run for the guess to mean something, both switchable off.
Auto update in Settings has an interval list beside it that shows while the switch
is on; switching Auto update off turns the automatic check off.

### Toolbar

Which buttons the header and composer bar show: quick grid-size control,
the library button, and multiple views. Hide the top bar or the input bar: right-click the
gear or the top bar.

### Privacy

Excludes the window from screen capture (on by default), blurs media until
hovered, dims unhovered tiles. "Allow screenshots of Prompt
Saver" lets every capture program, the app's own screenshot tool included,
record its windows; the recording bar and the outline around a recorded region
never appear in a recording either way.

An optional, off-by-default setting reads text out of an attached picture so
the library search can find it too, and a hit found only there carries a
"found in image" badge. The recognized text is kept only for that search: it never
appears anywhere in the app, is never exported and never synchronized, and is
capped in length so a screenshot of a wall of text cannot inflate the
database. Removing or replacing the picture clears it immediately.

The screen-capture exclusion needs Windows 10 version 2004 (build 19041) or
newer; on an older Windows it is silently ignored and the window is captured
like any other.

Everything the database holds is sealed with Windows' own per-account
encryption, so another account on the same PC cannot read it. Very rarely
Windows refuses to do that, almost always a damaged user profile or account. In
that case saving keeps working, but the data is written in readable form rather
than being silently lost, and the app says so: once in a dialog at startup, and
permanently as a red row at the top of this privacy group for as long as it is
true. That row has no off switch on purpose: it describes what is on disk right
now, not a tip. Repair the Windows profile (or sign in on a healthy account) and
the next save that succeeds seals everything again by itself; nothing needs to be
re-entered.

A single prompt can be marked confidential from its editor (off by default).
Its text and any variants are then encrypted with a vault password, asked
once per session the first time a confidential prompt is opened or copied,
and forgotten again on quit or after an idle timeout set in the expert menu
(default: 15 minutes). Forgetting the vault password means those entries
are lost for good: there is no recovery, and the setup dialog says so before
it lets you continue. While the vault is locked, a confidential tile shows a
redacted bar instead of its name, does not copy, and is left out of search.
It is also left out of Markdown sync, and out of a plain export by default;
exporting offers a choice to keep it encrypted instead, and a full backup
always keeps it encrypted.

Separately, if the saved settings themselves turn out to be unreadable or
corrupted on disk (extremely rare), the app shows defaults instead of guessing
and refuses to save any settings change until this is resolved; the original,
possibly-recoverable data is never overwritten. A backup of the raw database is
made automatically, and a permanent warning row (also with no off switch)
appears in this same privacy group for as long as it is true. A factory reset
or restoring a backup's settings both clear it.

If a settings save itself cannot be written to disk (data folder full, no
write permission), changes stay active for the rest of the session but are
lost on restart. The app says so the same way: once in a dialog, and as a
permanent row in this privacy group for as long as the last save is still
failing; it clears itself the moment a save reaches disk again.

### Updates & security

On first launch the app asks whether it may check for updates; until you
answer, it never goes online. The update check is one of a short, fixed list of
things that ever reach the network on their own: the update check itself, a first-run download of the
Microsoft WebView2 installer if that runtime is missing, and the update
download that follows once you choose to install one. A downloaded update is
verified against a published checksum before it runs; a missing or
mismatching checksum blocks the install instead of running an unverified
file. Video export/conversion needs ffmpeg: an installed build ships it and
never touches the network for it, while a portable build offers a one-time
download only when a video feature actually needs it and only with your
explicit consent, never automatically, never in the background. Two more
things on that same short list are not update-related at all: opening the
app's GitHub page from the About window or the version label hands the address to
your default browser, and installing a text-recognition or dictation
language pack (see below) has Windows itself fetch that pack online.

After an update, the update dialog shows once what
changed since the version you last saw; its list ships with the app, so
opening it needs no network, only its own "Update from GitHub" button does.
Reopen the current version's list any time from the About dialog.

### Where your data lives

Everything lives in one local SQLite database (plus its small WAL file).
Installed: under AppData, shown and changeable in the expert menu. Portable:
right next to the exe, so the whole thing stays on a USB stick or synced folder
as one unit. Moving between the two is detected automatically and offered as a
one-time migration.

A folder that syncs across two computers (OneDrive, Dropbox, Google Drive,
iCloud Drive, Nextcloud) needs care: two machines saving to the same database
file at once can corrupt it. Pointing the data folder at one shows a one-time
warning explaining that risk. A small lock file next to the database then
tracks which computer is using it, refreshed every 30 seconds. A second
computer that finds a fresh lock from another machine opens everything
read-only instead and says so with a permanent banner, so nothing is silently
lost. A lock nobody has refreshed for a while (the other computer likely
crashed or closed) is taken over automatically. The switch for this, on by
default, sits in the Troubleshooting group of the expert menu's System tab. This is a
safety net, not a way to share the library across computers: for that,
Markdown folder sync below is the built path.

### Backup manager

Encrypted, compressed snapshots of the entire library and settings, on an
automatic schedule with daily, weekly, monthly and yearly retention tiers, and a restore that can
target data only, settings only, or both. Two buttons beside Create backup
save a backup to a file of your choice or import one from a file. The same
window also has a storage section that shows how much disk
space the database, attachments, screenshots, recordings, backups, temporary
files and the error log each use, with a button to open a listed folder.

A retention section lists one row per kind of data this app keeps around on
its own (trash, copy history, copies from other apps, recordings, discarded
recordings, temporary exports, and unfinished-edit states), with how much is
there right now, how long it is kept, a button to clean it up on the spot, and
a button that jumps straight to the matching expert-menu setting. The numbers
are measured in the background and labelled with when that happened, the same
way the storage section above already works, not recalculated on every open.

Each snapshot in the list can also show its full content on demand: entry,
view and tag counts, the most recent change inside it, the program version
that made it, and whether its rows are sealed the normal way or fell back to
plain storage. Below that, every prompt it holds, with its own edit date,
size and kind, searchable by name and sortable by any column, with a button
to restore that single prompt right from the list instead of the whole
snapshot.

### Usage statistics

A small window opened from Settings → Usage statistics, with four things: the
number of prompts, the copies of the last 7 days, the most used prompts by
name (a list of 5, 10 or 20, your choice) and the prompts that were never used,
which you can click to find them in the library. The "Show statistics" switch
turns the display on; the numbers themselves are collected by the copy
history. Everything stays on this computer. The export button saves the full
numbers, including daily counts and tool use, as CSV, TXT or JSON.

### Import & export

Plain CSV, TXT or JSON export for the text alone, or a full backup file with
everything: tiles, views, layouts, colors, styles, and language. A single
"Export" button and a format dropdown replace three separate buttons; JSON is
the only one of the three that can carry the full rotation texts of a tile
with rotation, tags, notes, a hotkey, and color, rather than joining them into one
field the way CSV and TXT have to. JSON is the default format of the "Export" button and keeps every detail of a prompt; CSV and TXT stay available in the dropdown. Images never travel in any of the three,
to keep the file small; use a full backup for those. Importing decides CSV or
JSON by looking at the file itself, never its extension, and shows a preview
first: every incoming item's name and whether it would land as a duplicate,
before anything is written; a full backup restores directly. If the file
changes after the preview is shown, confirming the import is refused rather
than importing something different from what was previewed.

Import → "From folder…" in Settings turns a folder of `.md`/`.txt` files into tiles, one per
file, without touching the original files at all. The file name becomes the
tile's name. A short first line becomes the note, with the rest carrying on as
the text. Each subfolder becomes a tag, one per level. A file that already
contains the rotation separator becomes a prompt with rotation texts. The preview lists
every file with its tags, size, and a checkbox, and grays out anything that
could not be read (too large or not plain text) with the reason why. How deep
into subfolders it looks and how many files it stops at are both expert-menu
settings.

### Data maintenance

Trash (with its own retention setting), CSV/TXT export, and full backup
export/import, all reached from the expert menu's data group. The duplicate
finder is opened from the "Find duplicates" button in the library.

"Recent messages" keeps a short list of error messages that flashed by in a
toast, with time and place; copy or clear the list from its window. Settings
alone can be saved to a file and loaded again from the same group.

### Trash & restoring tiles

A deleted tile goes to the trash first, not straight to permanent deletion.
Deleting takes one click and shows an Undo notice that brings it back. Retention is in days (0 means keep forever). Restoring places the tile in the
active view's first free cell, not necessarily its original view or cell. It
does not bring back a floating button, folder membership, or hotkey it had before
deletion; those need setting up again.

### Undo

A few actions outside the tile trash carry their own one-step Undo button on the
confirmation message: deleting a prompt or a batch of them, deleting a view, clearing the copy history, deleting or merging tags, and a batch tag change. There is only ever one undo waiting; a later destructive action
replaces it, and the button disappears once the message itself does. Turn it
off in the expert menu if you would rather the message stayed a plain
confirmation; deleting a view or merging tags then asks first, since nothing
could bring them back.

### Duplicate finder

Flags tiles that are textually very similar, by a similarity score rather than
an exact match, without deleting anything on its own. Review a flagged pair side
by side, keep one, or mark the pair as not a duplicate to stop it being flagged
again. A tile with rotation is only compared on its first text.

The same similarity score also drives a quiet hint after saving a prompt: if an
existing one is close enough, a message names it with a button to open it and
another to compare the two. It never holds up the save and never asks
anything, so ignoring it costs nothing. Both share one threshold in the expert
menu.

### Command line

`prompt-saver.exe <command> ...` runs the app as a command line tool instead
of opening its window: list, get, copy, add, edit, delete, restore prompts,
manage tags and views, search, export, import, and create, list or restore
backups, plus reading or writing one setting directly. If the app is already
running, the command is handed to that copy and its answer is printed here.
Otherwise the command runs on its own, straight against the database, and
exits without ever opening a window. Either way only one process ever touches
the database at a time.

Every command takes `--help`. `--json` switches a command's output to
machine readable JSON instead of plain text. Any command that changes
something needs `--yes`, or it only prints what it would have done and
changes nothing. The exceptions are `copy`, which only reads a prompt's text
onto the clipboard, `backup create`, which only ever adds a new snapshot, and
`export`, which only needs `--yes` to overwrite a file that already exists.
Exit codes: 0 success, 1 bad invocation, 2 not
found, 3 the store is locked or unavailable, 4 refused (missing `--yes`, or a
value that failed validation). Full command list: `prompt-saver.exe --help`.

For a script or another program, `prompt-saver describe` prints the full
command list as JSON, and [docs/automation.md](automation.md) has the complete
reference: every command's arguments and JSON response shape, the error codes
and worked examples, so nothing needs to be worked out by trial and error.

### Demo mode

An expert-menu switch (System tab, Troubleshooting group) swaps the whole window to a bundled sample
library covering most features at once: prompts with and without an image, a
prompt with rotation, a couple of plain prompts to start a chain with, a
placeholder, tags, a favorite, two views, a folder, an attachment and (when
available) a video. Turning it on rebuilds the sample library fresh every
time and restarts the app into it; the real library is never opened, written
or even read while any of this happens. A permanent marker stays visible in
the window for as long as demo mode is on, so nobody mistakes the sample data
for their own. Turning it back off asks for confirmation, since anything
changed during the session is discarded, then restarts back into the real
library exactly as it was. The same sample library is also the source for
this help window's own picture and for `docs/screenshot.png`: see the
README's "Building from source" section for the exact steps.

---

## Hilfe (Deutsch)

Derselbe Inhalt wie oben, wie er auch im Hilfe-Menü der App steht (Einstellungen →
Hilfe). Nur Englisch und Deutsch. Jede andere Sprache in der App fällt auf
Englisch zurück.

### Grundlagen

**Erste Schritte**: Eine Kachel ist ein gespeicherter Eintrag im Raster. Doppelklick
auf eine leere Rasterzelle (oder Text ins Feld unten tippen und „Speichern“ drücken)
legt eine an, ein Klick kopiert sie.
Drei Beispielkacheln (E-Mail mit Platzhaltern, Bild, Markdown-Vorlage) sind beim
ersten Start bereits da; ein Button über dem Raster entfernt sie mit einem Klick.

**Einstellungen**: Das Zahnrad in der Kopfzeile öffnet das Einstellungsfenster
mit den alltäglichen Entscheidungen; alles Optionale liegt im Expertenmenü, das
sich im selben Fenster öffnet. Dort stehen Sprache, Design, Textgröße und Schriftart
der Kacheln, ob das Fenster im Vordergrund bleibt, mit Windows startet, im
Hintergrund startet und sich beim Schließen in den Hintergrund legt (bei einer
neuen Installation ab Werk an; eine portable Kopie beendet sich immer), der globale
Tastenkürzel, Auto-Einfügen bei Doppelklick und das Speichern von Screenshots im
Screenshots-Ordner (Abschnitt „Aufnehmen“). Jede Änderung gilt sofort, ohne Speichern-Button.
„Automatisch“ als Sprache folgt Windows und fällt auf Englisch zurück.
Zurücksetzen stellt jede Einstellung auf die Vorgabe zurück und behält die Kacheln;
Löschen entfernt alle Daten und fragt vorher nach.

**Einführungstour**: vier Schritte beim ersten Start, direkt nach den
Beispielkacheln; sie ändert keine Einstellung. Escape oder „Überspringen" schließt
sofort; nach Escape öffnet eine Meldung mit Rückgängig die Tour wieder. Jederzeit
erneut aufrufbar über das Expertenmenü.

**Das Raster**: freie Platzierung, standardmäßig bis 20×20 Zellen (im
Expertenmenü bis 200×200 einstellbar), eigenes Layout je Ansicht. Eine Kachel
auf einen Ansichtsreiter über dem Raster gezogen verschiebt sie in diese
Ansicht, mit gedrückter Strg-Taste wird stattdessen kopiert und sie bleibt in
beiden. Die Rastergröße
ändert man per Rechtsklick auf einen Ansichtsreiter oder eine freie Stelle im
Raster unter Rastergröße.

**Kacheln anlegen**: Dateianhang, Screenshot-Werkzeug, Farbpipette (aus),
Ziehen & Ablegen.

**Der Editor**: Markdown-Vorschau, Syntax-Hervorhebung, Versionen, private
Notizen (Markdown-Vorschau und Notizen ab Werk an, im Expertenmenü abschaltbar). Bei einer Datei-Kachel
mit eigenem Vorschaubild: das Bild über der Dateizeile ist nur die Kachel-Vorschau,
der Anhang darunter ist eine eigene Sache, kopiert wird immer nur der Dateipfad.
Ein Anhang als reiner Text, Markdown, JSON oder CSV zeigt seinen Anfang direkt
unter der Dateizeile (im Expertenmenü abschaltbar, dort auch einstellbar, wie
viele Kilobyte gelesen werden). Immer als reiner Text, nie ausgeführt, also
zeigt eine Markdown-Datei mit echter Formatierung trotzdem ihren rohen Quelltext.

Der offene Editor sichert seinen Text alle paar Sekunden als Entwurf (standardmäßig
an, der Abstand im Expertenmenü einstellbar), damit ein Absturz oder ein
geschlossenes Fenster nie ungesicherte Arbeit verliert. Gesichert wird nur der
Text, nie ein angehängtes Bild, und Speichern oder Abbrechen entfernt den
Entwurf sofort wieder. Ist beim nächsten Start noch ein Entwurf vorhanden, wird
einmal gefragt, ob er wiederhergestellt werden soll.

**Das Expertenmenü**: jede Funktion hat einen Schalter mit ihren Parametern
direkt darunter. Die Suche durchsucht alle Tabs auf einmal; ausgeschaltete
Funktionen tragen ein kleines Punkt-Abzeichen. Zurücksetzen auf drei Ebenen:
ein Wert, eine ganze Gruppe, oder alles auf einmal mit der Schaltfläche
"Zurücksetzen" am Ende des Tabs System (deine Prompts bleiben erhalten).

**Tastaturbedienung**: Pfeiltasten bewegen einen Fokusring über das Raster
(an), Enter oder Leertaste kopiert die fokussierte Kachel.

**Häufige Fragen**: Bildschirmaufnahme dieser App leer? Der
Datenschutz-Ausschluss wirkt wie vorgesehen, mit "Screenshots von Prompt
Saver erlauben" im Expertenmenü aufhebbar. App
startet nicht? Die WebView2-Laufzeit fehlt, der Installer bietet sie an.
Einstellung nicht gefunden? Die Suche im Expertenmenü durchsucht alle Tabs
auf einmal. Tastenkürzel tut nichts? Eine andere App hält die Kombination
vermutlich schon.

### Funktionen

**Kacheln auswählen**: Rechtsklick auf eine Kachel (oder Strg+Klick) wählt sie aus;
solange du auswählst, zeigt jede Kachel ihr Kästchen. Das Kachelmenü öffnet per
Rechtsklick.

**Kopieren**: Verketten (mehrere verschiedene Kacheln als ein Text, anders als
Rotation oben; auch direkt über Kette hier starten: Rechtsklick auf eine Textkachel, einzeln
abschaltbar unter Kette im Expertenmenü). Eine Kette bleibt beim Wechsel in ein
anderes Programm bestehen. Der Ketten-Button oben startet
und beendet eine Kette. Auch Strg+V im App-Fenster beendet sie. Was zwischen zwei verketteten
Kacheln steht, legt das Expertenmenü fest (ab Werk eine Leerzeile). Beim Verketten zeigt
eine Leiste über dem Raster die gewählten
Schritte der Reihe nach, mit Marken für die Platzhalter, die jeder Schritt
braucht und die er selbst füllt; ein Schritt ohne Antwort aus einem früheren
Schritt wird rot markiert, lässt sich aber trotzdem starten. Die Zeilen lassen
sich per Ziehen umsortieren und per Rechtsklick entfernen (die Zwischenablage
folgt), die Leiste selbst ist einzeln abschaltbar.
Mit eingeschalteter Doppelklick-Dauer-an-Funktion bleibt der Kettenmodus nach dem
Kopieren an, um mehrere Ketten nacheinander zu bauen.
Tastenkürzel pro Kachel, Herausziehen (mit oder, per Schalter, ohne
Modifikatortaste), automatisches Einfügen. Formatiert kopieren (im Rechtsklick
und im Editor, nur bei Text mit echter Markdown-Syntax): legt zusätzlich zur
Markdown-Fassung eine gerenderte Fassung in die Zwischenablage, sodass Word,
Outlook oder ein Browser die Formatierung sehen, ein reines Textfeld aber
weiterhin genau den Markdown-Text bekommt wie bei einer normalen Kopie.

<p>
<svg viewBox="0 0 560 130" xmlns="http://www.w3.org/2000/svg" role="img"><rect x="20" y="15" width="110" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="75" y="36" font-size="11" fill="currentColor" text-anchor="middle">Prompt anlegen</text><line x1="130" y1="32" x2="187" y2="32" stroke="currentColor" stroke-width="1.5"/><path d="M 185 28 L 195 32 L 185 36" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><rect x="195" y="15" width="170" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="280" y="36" font-size="11" fill="currentColor" text-anchor="middle">kopieren</text><line x1="365" y1="32" x2="422" y2="32" stroke="currentColor" stroke-width="1.5"/><path d="M 420 28 L 430 32 L 420 36" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><rect x="430" y="15" width="110" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="485" y="36" font-size="11" fill="currentColor" text-anchor="middle">einfügen</text><line x1="280" y1="49" x2="280" y2="70" stroke="currentColor" stroke-width="1" stroke-dasharray="2 2"/><rect x="60" y="78" width="115" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="117" y="99" font-size="10" fill="currentColor" text-anchor="middle">Kachel</text><rect x="190" y="78" width="115" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="247" y="99" font-size="10" fill="currentColor" text-anchor="middle">Tastenkürzel</text><rect x="320" y="78" width="115" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="377" y="99" font-size="10" fill="currentColor" text-anchor="middle">Schwebeknopf</text><rect x="450" y="78" width="115" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="507" y="99" font-size="10" fill="currentColor" text-anchor="middle">Infobereich</text></svg>
</p>

<p>
<svg viewBox="0 0 560 90" xmlns="http://www.w3.org/2000/svg" role="img"><rect x="30" y="20" width="100" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="80" y="41" font-size="11" fill="currentColor" text-anchor="middle">Schritt 1</text><rect x="230" y="20" width="100" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="280" y="41" font-size="11" fill="currentColor" text-anchor="middle">Schritt 2</text><rect x="430" y="20" width="100" height="34" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="480" y="41" font-size="11" fill="currentColor" text-anchor="middle">Schritt 3</text><line x1="130" y1="37" x2="217" y2="37" stroke="currentColor" stroke-width="1.5"/><path d="M 215 33 L 225 37 L 215 41" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><text x="177" y="25" font-size="10" fill="currentColor" text-anchor="middle">{?X}</text><line x1="330" y1="37" x2="417" y2="37" stroke="currentColor" stroke-width="1.5"/><path d="M 415 33 L 425 37 L 415 41" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><text x="377" y="25" font-size="10" fill="currentColor" text-anchor="middle">{?Y}</text></svg>
</p>

**Platzhalter**: `{?Name}` im Text öffnet beim Kopieren ein Popup
für jeden Wert. Prompts mit der älteren Schreibweise {#{Name}#} werden
automatisch umgeschrieben. Dynamische Platzhalter (`{?@date}`, `{?@time}`,
`{?@clipboard}`) füllen sich selbst. Feste Werte (im Expertenmenü, aus)
schlagen für einen passenden Namen eine gepflegte Liste von Werten vor, nur
als Platzhaltertext im Feld, mit einer kleinen Stecknadel markiert, nie
eingesetzt, bevor er getippt oder übernommen wird.

<p>
<svg viewBox="0 0 560 130" xmlns="http://www.w3.org/2000/svg" role="img"><text x="30" y="45" font-size="15" fill="currentColor" text-anchor="start">{?Kunde=Meier|Meier, Schulz}</text><line x1="57" y1="55" x2="57" y2="80" stroke="currentColor" stroke-width="1"/><text x="57" y="95" font-size="11" fill="currentColor" text-anchor="middle">Name</text><line x1="110" y1="55" x2="142" y2="80" stroke="currentColor" stroke-width="1"/><text x="142" y="95" font-size="11" fill="currentColor" text-anchor="middle">Vorgabe</text><line x1="232" y1="55" x2="252" y2="80" stroke="currentColor" stroke-width="1"/><text x="252" y="95" font-size="11" fill="currentColor" text-anchor="middle">Auswahlliste</text></svg>
</p>

**Tastenkürzel**: ein globales Tastenkürzel öffnet den Schnellstarter (Standard
Strg+Alt+P), ein eigener Tastenkürzel pro Kachel kopiert (und fügt optional ein). Bei
einer Kollision gewinnt der App-Tastenkürzel, sonst die erste Kachel in der Liste.
„Alle Tastenkürzel“ neben dem globalen Feld öffnet die Tastenkürzel-Übersicht mit jedem Tastenkürzel, durchsuchbar, und
belegt jede Zeile direkt um, ohne den jeweiligen Editor zu öffnen. Eine Kachel
mit eigenem Tastenkürzel zeigt ein kleines Tastatursymbol.

**Rotation und Kette:** zwei verschiedene Dinge. Rotation ändert den Text
EINER Kachel, Kette kopiert MEHRERE verschiedene Kacheln zusammen.

| Ich will... | nutze... |
| --- | --- |
| verschiedene Formulierungen desselben Prompts | Rotation |
| mehrere verschiedene Prompts hintereinander | Kette |

**Rotation**: eine Kachel trägt eine Liste von Texten, jede Kopie
liefert den nächsten Rotationstext (Reihenfolge oder Zufall). Standardmäßig an; der
Schalter pro Kachel steht im Editor unter „Erweiterte Optionen“. Ein Badge auf
Kachel/schwebender Button/Mitglied eines Desktop-Ordners zählt hoch: im Zufalls-Modus die Ziehungen
im aktuellen Durchlauf statt einer springenden Position; übersteht Neustart und
Sicherung, aber keinen CSV/TXT-Import (neue Kachel, neuer Zeiger). Eine geeignete
Kachel (2+ Einträge, kein Bild/Datei) kann zusätzlich **beim Einfügen
rotieren**: Button „Beim Einfügen rotieren“ im Editor oder Rechtsklickmenü,
solange Kachel- und App-Schalter beide an sind. Bei jedem echten Einfügen
rückt der nächste Eintrag nach, nicht nur beim Kopieren; das Abzeichen zeigt
eine Akzentfarbe, solange die Sitzung läuft. Platzhalter werden einmal pro
Sitzung abgefragt, dieselben Werte gelten für jeden Rotationstext. Fünf
Experteneinstellungen regeln Zeitlimit und Erkennungsfenster; zu schnelles,
zu häufiges Einfügen pausiert die Sitzung mit einem Hinweis statt zu wiederholen.

**Schwebende Buttons:** kleine, immer sichtbare Kopier-Buttons außerhalb des
Hauptfensters, über jeder anderen App.
Ein Button, dessen gespeicherter Bildschirm nicht mehr angeschlossen ist, öffnet sich
auf dem Hauptbildschirm, als geparkt markiert, und springt von selbst zurück, sobald
dieser Bildschirm wieder da ist. Eine Aktion im Expertenmenü holt alle schwebenden
Fenster auf einmal auf den Hauptbildschirm.

**Ordner:** eine Geste an zwei Stellen, ein Ziehen auf ein anderes fasst es zu
einem Ordner zusammen. Ein schwebender Button auf einen anderen ergibt einen
Ordner auf dem Desktop. Eine Kachel auf eine andere ergibt einen Ordner im Raster, und
gehört immer nur zu einem Raster-Ordner gleichzeitig. Ein Ordner liegt dort, wo er angelegt wurde.

<p>
<svg viewBox="200 0 360 120" xmlns="http://www.w3.org/2000/svg" role="img"><circle cx="255" cy="45" r="16" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="295" cy="45" r="16" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="335" cy="45" r="16" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="295" y="108" font-size="11" fill="currentColor" text-anchor="middle">Desktop-Ordner</text><rect x="430" y="20" width="100" height="70" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><line x1="430" y1="55" x2="530" y2="55" stroke="currentColor" stroke-width="1"/><line x1="480" y1="20" x2="480" y2="90" stroke="currentColor" stroke-width="1"/><text x="480" y="108" font-size="11" fill="currentColor" text-anchor="middle">Raster-Ordner</text></svg>
</p>

**Infobereich-Menü**: Zuletzt kopiert, Favoriten. Erscheint als abgerundetes,
zum Thema passendes Fenster statt des einfachen Windows-Menüs; im Expertenmenü
lässt sich bei Bedarf auf das einfache Systemmenü zurückschalten. Tippen ins Suchfeld des
Infobereich-Menüs öffnet den Schnellstarter mit dem Getippten.

**Schneller Launcher.** Strg+Alt+P, unscharfe Suche, sofortiges Kopieren. Wie viele Treffer er zeigt, stellt das Expertenmenü ein (ab Werk 8). Eine Zeile unten nennt die Tasten.

**Mehrere Ansichten.** Mehrere Rasterseiten.

**Spracheingabe**: standardmäßig an; der Mikrofon-Button neben der Eingabeleiste
und im Kachel-Editor verwandelt Sprache in Text, komplett auf dem Gerät über die
eigene Spracherkennung von Windows. Gesprochene Satzzeichen-Wörter („Komma",
„neue Zeile" …) werden zu echten Zeichen; „wörtlich" vor einem Wort tippt es
statt es als Befehl zu erkennen. Ein optionaler globaler Kurzbefehl startet und
stoppt eine Aufnahme, ohne das Fenster zu öffnen. Braucht ebenfalls ein
Windows-Sprachpaket (derselbe Dialog wie bei OCR).

**Sprachpakete**: Texterkennung und Spracheingabe teilen sich Windows' eigene
Sprachpakete und denselben Installationsdialog. Die App bietet den Download beim
ersten Bedarf an; die Installation braucht Administratorrechte (übliche
Windows-Bestätigung). „Sprachen der Texterkennung verwalten…" (bei der Spracheingabe
„Diktatsprachen verwalten…") im Sprachen-Dropdown listet
Installiertes und Verfügbares und startet Installation/Update erneut; das
Auflisten selbst braucht normalerweise keine Sonderrechte, auf manchen
Windows-Systemen aber doch; der Dialog sagt das dann offen und bietet an, die
Abfrage mit Administratorrechten auszuführen, statt leer zu bleiben. Dabei wird
nur aufgelistet, nichts installiert. Eine Hintergrundprüfung meldet ein Paket,
dessen Version sich geändert hat oder das ein Windows-Update entfernt hat, ohne
gleich zur Neuinstallation aufzufordern: „Abbrechen“ im Hinweis blendet ihn
vorübergehend aus, oder die Prüfung lässt sich ganz abschalten.

**Markdown-Ordner-Sync (Beta)**: spiegelt jede Text-Kachel als lesbare
`.md`-Datei in einem gewählten Ordner, inklusive Ansicht, Rasterzelle und
Ordner. Angehängte Dateien, Bilder und PDFs wandern als Verweis mit (Pfad und
PDF-Seite im Kopf, nie die Datei selbst); ein direkt eingefügter Screenshot hat
keinen Pfad und bleibt nur in der Datenbank. Eine in einer eingehenden Datei
genannte Ansicht oder ein Ordner, den es auf diesem Rechner noch nicht gibt,
wird automatisch angelegt, per Namensabgleich. **Die Dateien sind Klartext**:
anders als die Datenbank, also kein synchronisierter oder geteilter Ordner für
Sensibles.

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
  paar Sekunden zu scheitern (nach fünf Fehlversuchen). Die Zeile
  unter dem Ordner im Expertenmenü nennt den letzten gelesenen Stand, die Zahl
  der Dateien und den Grund der Pause.

### Aufnehmen

**Screenshot-Werkzeug**: Bereich oder ganzes Fenster (auch geschützte wie den
Task-Manager) direkt zu einer neuen Kachel. Davor öffnet sich standardmäßig ein
Editor: Zuschneiden, Stift, Marker, Linien, Pfeile, Rechtecke, Ellipsen, Text,
nummerierte Schritte, Pixelieren und ein Fokusbereich, der alles außerhalb eines
Bereichs abdunkelt, mit Rückgängig/Wiederholen. Zum Heranzoomen für
pixelgenaues Arbeiten lässt sich das Bild mit dem Hand-Werkzeug (oder
gehaltener Leertaste bzw. der mittleren Maustaste) verschieben. Ziehe die
Ränder oder Ecken des Rahmens, um zuzuschneiden. Die
Größenänderung ändert die tatsächliche Pixelgröße des Bildes (nicht nur die
Ansicht): Breite, Höhe oder Prozentwert einstellbar, mit Voreinstellungen für
25/50/200 % und "auf 1920 Breite anpassen"; sie ist selbst ein rückgängig
machbarer Schritt.

**Bildschirmaufnahme**: nimmt ein Fenster, einen Monitor oder einen gezogenen
Bereich als Video auf, über den Aufnahme-Button neben dem Screenshot-Werkzeug.
Eine Bereichsaufnahme zeigt einen dünnen Rahmen um den Bereich; er erscheint nie
im Video, und das Expertenmenü schaltet ihn ab oder ändert seine Farbe.
Aufgenommen wird immer MP4 (H.264); drei Qualitätsstufen wählen davor die
Bitrate: Klein (4000 kbps), Hoch (12000 kbps, Standard) und Verlustfrei; dabei
wird mit der aktuellen Bitrate aufgenommen und die Aufnahme danach automatisch
verlustfrei neu kodiert, was bei längeren Aufnahmen mehrere Gigabyte groß
werden kann. Ein Bitratenregler neben der Qualitätsstufe passt Klein/Hoch
weiter an; er hat auf der Stufe Verlustfrei keine Wirkung. Ein Format-Feld
bestimmt die fertige Datei. Aufnahmen und Exporte gibt es als MP4, WebM, GIF oder AVI. WebM und AVI entstehen aus dem MP4; GIF ist auf 30 Sekunden begrenzt und hat keinen Ton. Für alles außer MP4 sowie für Verlustfrei wird ffmpeg gebraucht, ein
separates, GPL-lizenziertes Werkzeug; fehlt es, sagen Format- und
Qualitätsauswahl das direkt und bieten den Download an (einmalig, nur mit
Zustimmung); eine installierte Version bringt es bereits mit. Eine
fehlgeschlagene oder abgebrochene Umwandlung lässt die ursprüngliche
MP4-Aufnahme immer unangetastet. Ein Fehlschlag nennt den eigentlichen Grund
in Klartext (Datei unvollständig, kein Platz mehr und so weiter) statt einer
rohen ffmpeg-Protokollzeile; eine zugeklappte Zeile „Technische Angaben"
darunter hält diese Zeile samt Kopier-Button für alle bereit, die sie brauchen.
Ein Klick auf den Aufnahme-Button öffnet
dieselbe Bildschirm-Übersicht wie ein Screenshot: Fenster anklicken, Bereich
ziehen oder auf den freien Desktop klicken für den ganzen Bildschirm. Ein
Countdown zählt danach herunter, bevor die Aufnahme wirklich startet
(3 Sekunden Standard, im Expertenmenü einstellbar, `0` schaltet ihn ab). Eine
kleine Bedienleiste begleitet die Aufnahme (roter Punkt, Laufzeit,
Dateigröße, zwei Ton-Buttons, Pause/Fortsetzen, Stopp, Verwerfen), folgt einem
verschobenen Aufnahmefenster und ist selbst niemals im Video zu sehen. Zieht
man die Leiste selbst (nicht ihre Buttons), lässt sie sich frei verschieben,
auch auf einen anderen Monitor; ab dem ersten Ziehen folgt sie dem
Aufnahmefenster nicht mehr, sondern bleibt dort stehen, wo man sie
hingezogen hat, für den Rest dieser einen Aufnahme. Die nächste Aufnahme
folgt wieder von vorn. Systemton
(Standard an) und Mikrofon (Standard aus, Gerät wählbar im Expertenmenü)
werden je als eigene Spur aufgenommen; zieht man das Mikrofon während der
Aufnahme ab, laufen Bild und Systemton weiter, das Mikrofon-Symbol zeigt nur
„verloren". Die beiden Ton-Buttons in der Leiste schalten Systemton und
Mikrofon während der Aufnahme sofort stumm, ohne die Spur zu beenden: eine
stumme Quelle nimmt weiter Stille auf statt auszusetzen, lässt sich also
innerhalb derselben Aufnahme wieder zuschalten, und die Leiste zeigt beim
nächsten Öffnen wieder den zuletzt gewählten Zustand. Stopp führt in denselben
Anheften/Speichern/Verwerfen-Dialog wie ein
Screenshot; braucht das gewählte Format oder die Qualitätsstufe eine
Umwandlung, zeigt der Dialog zuerst „Wird konvertiert…" (anklicken zum
Abbrechen) statt einer Vorschau; eine fehlgeschlagene oder abgebrochene
Umwandlung fällt immer auf die unangetastete Originalaufnahme zurück.
Schließt sich das aufgenommene Fenster von selbst, stoppt die Aufnahme
automatisch und behält, was bis dahin aufgenommen wurde. Der
Aufräumen-Dialog im Aufnahmeordner (Expertenmenü) kann mehrere fertige
Aufnahmen auch in einem Rutsch exportieren: Häkchen setzen, dann Format,
Qualität und Zielordner für alle zusammen wählen. Die Aufnahmeordner-Zeile
im Expertenmenü zeigt außerdem, wie viele Videos darin liegen und wie viel
Platz sie belegen, mit einem „Aufräumen…"-Button daneben: 7, 30 oder 90 Tage
wählen, die genaue Dateiliste und die frei werdende Größe prüfen, dann
bestätigen. Gelöscht wird nie im Hintergrund, immer nur hier und nur nach
Bestätigung. Eine neue Installation löscht nicht verwendete Aufnahmen nach 90
Tagen (Expertenmenü, 0 = für immer). Eine Tastenanzeige
(Standard aus, im Expertenmenü) zeichnet während einer Aufnahme systemweit
mit, welche Tasten gedrückt werden, damit eine Anleitung sie hinterher über
die „Tasten“-Überlagerung des Video-Editors zeigen kann. Aufgezeichnet werden
nur Tastenkennungen mit Zeitstempeln, nie ein Textinhalt. Ab Werk wird jede
Taste angezeigt, auch getippter Text; ein fokussiertes Passwortfeld wird für
diese Zeit ausgelassen und erscheint als Lücke, ohne dass das garantiert ist,
da es davon abhängt, dass Windows das Feld richtig als Passwortfeld meldet.
Ein Schalter „Nur Tastenkürzel" (im Expertenmenü, direkt neben der
Tastenanzeige) beschränkt die Aufzeichnung auf Kombinationen mit Strg, Alt
oder Win sowie Enter, Tab, Esc, Rücktaste, Entf, Einfg, Pfeile, Pos1, Ende,
Bild auf, Bild ab und F-Tasten, Buchstaben und Ziffern nie eingeschlossen,
für Aufnahmen mit vertraulichen Eingaben.

Eine optionale Kameraaufnahme (Standard aus, im Expertenmenü) fügt dem
Rechtsklickmenü des Aufnahme-Buttons den Eintrag „Kamera aufnehmen“ hinzu. Sie
öffnet ein eigenes kleines Fenster mit Live-Vorschau, eigenem
Start/Pause/Stopp, einer Tonquelle, einer Auflösungswahl und einem
Zielordner: denselben Bedienelementen wie die Bildschirmaufnahme selbst. Ein
Kamerafenster ohne sichtbare Vorschau oder Anzeige gibt es nie: Entweder das
Fenster selbst oder sein Statuspunkt in der Bedienleiste zeigt immer, solange
die Kamera läuft. Sie zeichnet in eine eigene Datei auf, nie ins Bild der
Bildschirmaufnahme eingebrannt, damit beide später frei positioniert werden
können. Läuft gleichzeitig eine Bildschirmaufnahme, teilen sich beide
denselben Startpunkt; der Video-Editor öffnet dann die Bildschirmaufnahme mit
der Kamera deckungsgleich als Überlagerung obenauf, sobald beide fertig sind.
Läuft keine Bildschirmaufnahme, ist die Kameraaufnahme ihr eigener Nullpunkt
und öffnet sich allein. Standardkamera, Auflösung und Bildrate sind alle im
Expertenmenü einstellbar.

**Screenshot- und Video-Editor**: arbeiten bei den gemeinsamen Werkzeugen
gleich: Strg+Plus, Strg+Minus und Strg+0 zoomen in beiden hinein, heraus und
passen die Ansicht ein; Strg+Z, Strg+Y (oder Strg+Umschalt+Z) machen mit
demselben Button-Paar rückgängig oder wiederholen. Im Screenshot-Editor wählt
ein Klick auf ein vorhandenes Rechteck, eine Ellipse, eine Linie oder einen
Pfeil es mit denselben Anfassern aus, die auch beim frischen Ziehen
erscheinen. Der Video-Editor wählt seine Überlagerungen weiter über seine
eigene Liste, weil er die Anfasser einer Überlagerung erst zeichnet, sobald
sie schon ausgewählt ist; die Werkzeugleisten selbst bleiben ebenfalls an
ihrem Platz, oben beim Screenshot-Editor, seitlich neben der Zeitleiste beim
Video-Editor, weil die Zeitleiste den Platz braucht, den der
Screenshot-Editor nicht hat. Im Screenshot-Editor stehen Rückgängig,
Wiederholen und Zoom links in der unteren Leiste. Im Video-Editor lässt sich
außerdem ein zweites
Video einfügen: „Video hinzufügen" bietet es entweder als weitere
Überlagerung (eigenes Rechteck, eigener Zeitbereich, eigene Lautstärke,
standardmäßig stumm) oder als ganzen zusätzlichen Abschnitt am Anfang, am
Ende oder an der aktuellen Stelle; jede zusätzliche Quelle wird beim
Auswählen einmal geprüft und bleibt sichtbar als fehlend markiert, falls sie
später verschoben oder gelöscht wird, bis zu einer Höchstzahl je Projekt
(ab Werk acht, einstellbar im Expertenmenü). Das Exportziel ist ein
Eingabefeld, kein fester Name mehr: direkt hineintippen, oder „Exportieren nach…", um
nur den Ordner zu ändern und den Dateinamen zu behalten; nach einem fertigen
Export verschiebt „Speichern unter…" das fertige Video an einen anderen Ort.
Ein Formatwechsel ändert nur die Endung, nie Ordner oder Namen, die man selbst eingetippt hat.
Die Schaltfläche „Kopieren“ daneben verzichtet ganz auf einen Namen: es
exportiert in eine Kopie, die sich selbst aufräumt (nach einer Stunde, und
erneut beim nächsten Start), und legt die
fertige Datei direkt in die Zwischenablage zum Einfügen in eine andere App,
genauso wie der Kopieren-Button im Aufnahme-Dialog.

<p>
<svg viewBox="0 0 560 130" xmlns="http://www.w3.org/2000/svg" role="img"><rect x="60" y="15" width="120" height="30" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="120" y="34" font-size="10" fill="currentColor" text-anchor="middle">Überlagerung</text><rect x="20" y="55" width="520" height="22" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/><line x1="180" y1="55" x2="180" y2="77" stroke="currentColor" stroke-width="1.5"/><line x1="340" y1="55" x2="340" y2="77" stroke="currentColor" stroke-width="1.5"/><text x="180" y="95" font-size="10" fill="currentColor" text-anchor="middle">Schnittmarke</text><path d="M 20 115 q 15 -15 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0 t 30 0" fill="none" stroke="currentColor" stroke-width="1"/><text x="480" y="95" font-size="10" fill="currentColor" text-anchor="middle">Tonspur</text><text x="290" y="12" font-size="10" fill="currentColor" text-anchor="middle">Zeitleiste</text></svg>
</p>

Ein Rechtsklick auf eine Form (Screenshot-Editor) oder die Zeile einer
Überlagerung (Video-Editor) öffnet ein kleines Menü zum Duplizieren oder
Löschen. Ein Duplikat behält Werte und Zeitbereich und wird ein Stück
verschoben, damit es neben dem Original liegt statt genau darauf. Im Video-Editor
richten sich Farbe, Stärke, Deckkraft und Schriftgröße einer neuen
Überlagerung außerdem nach dem, was für dieselbe Art (Bild, Rahmen, Text,
Verpixeln oder Fokusbereich) zuletzt benutzt wurde, auch über einen Neustart
hinweg.

Beim Ziehen einer Form (Screenshot-Editor) oder eines Zuschnitts bzw. einer
Überlagerung (Video-Editor) rastet die Bewegung an den Kanten der Leinwand
oder des Bildes, an dessen Mitte in beide Richtungen und an den Kanten und der
Mitte jeder anderen schon platzierten Form oder Überlagerung ein, sobald der
Zeiger nah genug kommt. Eine dünne Linie in der Akzentfarbe zeigt, welche
Linie gerade getroffen wurde, und verschwindet sofort wieder, sobald das
Ziehen endet. Gedrückt gehaltenes Alt schaltet das Einrasten für diesen einen
Zug ab. Jeder Editor hat dafür einen eigenen Schalter, die Toleranz teilen sie
sich.

Der Hintergrund des Textwerkzeugs ist eine Wahl zwischen keinem, Weiß, einer
durchscheinenden Platte oder einem Umriss, damit der Text auf jedem Untergrund
lesbar bleibt; Vorgabe ist schwarze Schrift auf einem weißen Feld, und das
Textfeld im Editor ist nur so breit wie das, was du tippst. Eine Expertenzeile stellt die Umrissbreite ein, seine
Farbe, schwarz oder weiß, folgt für den besten Kontrast automatisch der
Textfarbe.

Eine ausgewählte Form (Screenshot-Editor) oder Überlagerung (Video-Editor)
lässt sich mit den Pfeiltasten schrittweise verschieben; gedrückt gehaltenes
Strg ändert stattdessen die Größe, Umschalt macht einen größeren Schritt.
Im Video-Editor übernehmen die Pfeiltasten diese Aufgabe nur,
solange etwas ausgewählt ist, weil sie dort sonst durch die Einzelbilder
schalten; solange ausgewählt, übernehmen stattdessen Komma und Punkt das
Weiterschalten der Einzelbilder, und eine kurze Zeile über der Wiedergabeleiste
zeigt immer, was die Pfeiltasten gerade tun. Escape hebt die Auswahl auf und
gibt den Pfeiltasten ihre gewohnte Aufgabe zurück.

Ein Button in der Werkzeugleiste beider Editoren vergleicht mit der
unbearbeiteten Aufnahme: gedrückt gehalten zeigt er das Original, solange er
gehalten wird, ein Klick lässt diese Ansicht stehen, bis erneut geklickt wird.
Dieselbe Taste V tut dasselbe, und eine kleine Marke „Original" erscheint,
solange es zu sehen ist, damit es nie für die Bearbeitung gehalten wird. Im
Video-Editor blendet das nur Überlagerungen aus (Blenden und die Export-
Auflösung zeigt die laufende Vorschau ohnehin nie), der laufende Schnitt
bleibt dabei bestehen, weil ein Sprung an eine herausgeschnittene Stelle nur
verwirren würde. Der Export bleibt davon unberührt.

**Texterkennung (OCR)**: standardmäßig an; liest per Button in der Leiste und
im Screenshot-Editor Text aus einem Screenshot oder einer Bilddatei, komplett
offline über die eigene Texterkennung von Windows. Der Button in der Leiste öffnet
ein kleines Menü: aus einer Bilddatei oder aus einem Screenshot. Erkannter Text
kann automatisch kopiert und umgebrochene Zeilen zu ganzen Sätzen zusammengefügt
werden, beides einstellbar. Braucht ein Windows-Sprachpaket (siehe
„Sprachpakete" unten).

**Farbpipette & Auswahlverlauf**: Pixel als Hex/RGB/HSL kopieren; die letzten
Auswahlen überstehen einen Neustart. Standardmäßig aus.

### Kacheln

**Kachel-Verhalten & -Aussehen**: Menü, Umsortieren, Hover-Anhebung, Optik. Die
Schrift auf Kacheln, Ordnern, Ansichts-Tabs und Meldungen wählt automatisch
Schwarz oder Weiß; im Expertenmenü lässt sich das abschalten (immer Weiß) oder
die Helligkeitsschwelle für Schwarz verschieben. Weiße Schrift auf einer hellen
Farbe bekommt einen dunklen Schatten, damit sie lesbar bleibt; im Expertenmenü
lässt sich der Schatten abschalten oder seine Stärke einstellen.

**Medien**: Video-Steuerung, Autoplay und Schleife (beide ab Werk aus), Unschärfe bis zum Überfahren. Bibliothek und Kopierverlauf
zeigen außerdem die Länge eines Videos auf seinem Vorschaubild sowie Auflösung
und Dateigröße in der Kurzhilfe; ältere Anhänge holen das im Hintergrund nach,
sobald sie zum ersten Mal angezeigt werden. Das Hauptraster bleibt unverändert.

**Favoriten**: Stern-Markierung, eigene Favoriten-Ansicht. „Zu Favoriten hinzufügen“ im Rechtsklickmenü einer Kachel oder den Stern in der Bibliothek nutzen; eine Favoriten-Kachel zeigt oben rechts einen kleinen Stern.

**Kurzinfos**: Verzögerung, Breite.

### Aussehen

**Aussehen & Designs**: sieben mitgelieferte Designs, optionaler Design-Editor.

**Design-Editor.** Eigene Farbvarianten bestehender Designs. Geöffnet wird er mit „Eigenes Design…“ am Ende der Design-Auswahl in den Einstellungen.

**Skalierung & Zoom**: UI- und Symbolgröße getrennt einstellbar.
Die Größe der Oberfläche steht in den Einstellungen; die Symbolgröße
im Expertenmenü → Aussehen → Visuell.

### Bibliothek & Kopierverlauf

**Bibliothek & Suche**: unscharfe Suche, intelligente Sortierung, Typfilter,
`tag:`/`var:`-Suchpräfixe (mit `-` auch verneint). Ein Stern neben dem Suchfeld
speichert Anfrage, Tag-Filter und Sortierung als Kachel darüber; Rechtsklick
bietet Umbenennen und Löschen, Ziehen sortiert die Zeile um. Standardmäßig
höchstens fünf. Ein Umschalter neben dem
Typfilter stellt die Liste als Kacheln dar: großes Vorschaubild, Name
darunter, sonst nichts. Klick, Doppelklick, Rechtsklickmenü und
Mehrfachauswahl verhalten sich genau wie in der Liste. Der Umschalter merkt
sich seinen Stand je Filter.

**Alles durchsuchen**: Lupensymbol in der Werkzeugleiste oder Strg+K öffnet
eine Überlagerung, die Prompts, Kopierverlauf, Anhänge, Ansichten,
Einstellungen und Hilfe zugleich durchsucht, jede Quelle in ihrer eigenen
Gruppe mit bis zu acht Treffern und „… weitere anzeigen“. Pfeiltasten bewegen
die Markierung über Gruppengrenzen hinweg; Eingabetaste löst die passende
Aktion aus: kopiert einen Prompt oder Eintrag im Kopierverlauf, öffnet einen Anhang,
wechselt die Ansicht, öffnet und markiert eine Einstellung, oder öffnet das
passende Hilfethema. Das Feld merkt sich die letzten Suchen (acht ab Werk; das
Expertenmenü stellt eine Zahl für alle Suchlisten ein) und zeigt sie, wenn es leer ist. Escape, ein Klick daneben oder das Symbol schließen
die Überlagerung wieder. Ein Schalter im Expertenmenü stellt sie ab.

**Tags**: freie Schlagwörter, UND/ODER-Filter. Das Archiv ist ein gewöhnlicher Tag mit
reserviertem Namen: „Archivieren“ im Rechtsklickmenü einer Kachel (oder in der
Sammelleiste der Bibliothek) gibt ihr diesen Tag, sie verschwindet aus
Bibliothek und Raster, bis der Archiv-Chip über der Liste sie wieder anzeigt.
Export, Sicherung und Suche behandeln sie wie jede andere Kachel; „Aus dem Archiv
holen“ entfernt den Tag wieder.

**Kopierverlauf** (Bibliothek → Reiter Kopierverlauf): Protokoll, Gruppierung, Aufbewahrung (bei einer neuen
Installation 30 Tage, einstellbar, 0 behält alles), Einzeleintrag löschen.
Der Kopierverlauf ist eine Liste; das Sortierfeld wechselt zwischen Zuletzt kopiert und Meistgenutzt (die 20 häufigsten).

**Kopien aus anderen Apps**: aus; eingeschaltet nimmt der Kopierverlauf auch Text
auf, der in anderen Apps kopiert wurde, filterbar nach Herkunft.

### System & Daten

**System**: Updates, Tastatur-Navigation, Datenordner; mehrere Instanzen stehen in der Gruppe Fehlersuche.
Lange Vorgänge (Export, Umwandlung, ffmpeg-Download, Sicherung, Import) füllen
das Taskleistensymbol und zeigen ab einer eingestellten Schwelle „noch etwa …",
beides abschaltbar.
Neben „Automatische Updates“ im Einstellungsfenster steht die Intervall-Auswahl, solange der
Schalter an ist; „Automatische Updates“ auszuschalten schaltet die automatische Prüfung ab.

**Werkzeugleiste**: welche Buttons Kopf- und Composer-Leiste zeigen. Obere Leiste
ausblenden: Rechtsklick auf Zahnrad oder Kopfzeile.

**Datenschutz**: Bildschirmaufnahme-Ausschluss (an; "Screenshots von Prompt
Saver erlauben" gilt für jedes Aufnahmeprogramm, auch das eigene, nur die
Aufnahmeleiste und der Rahmen um den aufgenommenen Bereich bleiben immer
ausgenommen), Weichzeichnen, Namen verbergen, Abdunkeln. Eine optionale, standardmäßig ausgeschaltete Einstellung
liest Text aus einem angehängten Bild, damit die Bibliothekssuche ihn auch
findet, und ein Treffer nur dort trägt die Kennzeichnung "im Bild gefunden". Der
erkannte Text dient ausschließlich dieser Suche: Er erscheint nirgends in der
App, wird nie exportiert und nie synchronisiert, und ist in der Länge
begrenzt, damit ein Screenshot voller Text die Datenbank nicht aufbläht.
Entfernen oder Ersetzen des Bilds löscht ihn sofort.

Der Bildschirmaufnahme-Ausschluss braucht Windows 10 ab
Version 2004 (Build 19041); auf einem älteren Windows wird er still ignoriert
und das Fenster wie jedes andere aufgenommen. Alles in der Datenbank ist mit
der kontogebundenen
Windows-Verschlüsselung versiegelt; ein anderes Konto am selben PC kann es also
nicht lesen. Sehr selten verweigert Windows das, fast immer wegen eines
beschädigten Benutzerprofils oder Kontos. Dann wird weiter gespeichert, aber im
Klartext statt still verloren, und die App sagt es: einmal als Hinweis beim
Start und dauerhaft als rote Zeile oben in dieser Gruppe, solange es zutrifft.
Diese Zeile hat bewusst keinen Schalter: sie beschreibt, was gerade auf der
Festplatte liegt. Ist das Windows-Profil repariert, verschlüsselt der nächste
erfolgreiche Speichervorgang wieder alles von selbst; nichts muss neu erfasst
werden.

Ein einzelner Prompt lässt sich im Editor als vertraulich markieren
(standardmäßig aus). Sein Text und seine Varianten werden dann mit einem
Tresor-Passwort verschlüsselt, das einmal je Sitzung abgefragt wird, sobald
zum ersten Mal ein vertraulicher Prompt geöffnet oder kopiert wird, und beim
Beenden oder nach einer im Expertenmenü einstellbaren Ruhezeit wieder
vergessen wird (Vorgabe: 15 Minuten). Ein vergessenes Tresor-Passwort
bedeutet den endgültigen Verlust dieser Einträge: Es gibt keine
Wiederherstellung, und der Einrichtungsdialog sagt das, bevor er weitermachen
lässt. Solange der Tresor gesperrt ist, zeigt eine vertrauliche Kachel einen
abgedeckten Balken statt ihres Namens, kopiert nicht und bleibt bei der Suche
außen vor. Sie bleibt auch bei der Markdown-Synchronisierung außen vor und
wird bei einem einfachen Export standardmäßig ausgelassen; der Export bietet
stattdessen die Wahl, sie verschlüsselt zu behalten, und eine vollständige
Sicherung behält sie immer verschlüsselt.

Getrennt davon: Sollten die gespeicherten Einstellungen selbst auf der
Festplatte unlesbar oder beschädigt sein (sehr selten), zeigt die App
Standardwerte an, statt zu raten, und verweigert jede Einstellungsänderung, bis
das behoben ist; die ursprünglichen, möglicherweise noch rettbaren Daten
werden nie überschrieben. Es wird automatisch eine Sicherung der rohen
Datenbank angelegt, und eine dauerhafte Warnzeile (ebenfalls ohne Schalter)
erscheint in derselben Datenschutz-Gruppe, solange es zutrifft. Ein Werksreset
oder das Wiederherstellen der Einstellungen aus einer Sicherung löschen sie
beide.

Kann ein Einstellungs-Speichervorgang selbst nicht auf die Festplatte
geschrieben werden (Datenordner voll, keine Schreibrechte), bleiben Änderungen
für den Rest der Sitzung aktiv, gehen aber beim Neustart verloren. Die App
sagt das genauso: einmal als Hinweis, und als dauerhafte Zeile in dieser
Datenschutz-Gruppe, solange der letzte Speichervorgang fehlschlägt; sie
verschwindet von selbst, sobald wieder erfolgreich gespeichert wird.

**Updates & Sicherheit**: Beim ersten Start fragt das Programm, ob es nach
Updates suchen darf; bis zur Antwort geht es nie online. Die Update-Prüfung ist
eines von wenigen Dingen, die überhaupt von sich aus ins Netz gehen: die Update-Prüfung selbst, ein
einmaliger WebView2-Installer-Download, falls die Laufzeit fehlt, und der
eigentliche Update-Download, sobald eine gefundene Version installiert wird.
Ein heruntergeladenes Update wird vor der Installation gegen eine
veröffentlichte Prüfsumme geprüft; fehlt sie oder passt sie nicht, wird die
Installation abgebrochen statt eine ungeprüfte Datei auszuführen. Für
Videoexport/-umwandlung wird ffmpeg gebraucht (siehe oben); eine installierte
Version bringt es bereits mit, eine portable bietet nur bei Bedarf und nur
mit Zustimmung einen einmaligen Download an. Zwei weitere Dinge auf dieser
kurzen Liste haben mit Updates nichts zu tun: Ein Klick im Fenster „Über“ oder auf die
Versionsanzeige öffnet die GitHub-Seite des Projekts im Standardbrowser, und
die Installation eines Sprachpakets für Texterkennung oder Diktat (siehe
unten) lässt Windows selbst dieses Paket herunterladen.

**Was ist neu**: nach einem Update zeigt das Update-Fenster einmal, was sich seit
der zuletzt gesehenen Version geändert hat; die Liste liegt bereits im
Programm, nur ihr Button „Von GitHub aktualisieren" braucht Netz. Über den
Über-Dialog jederzeit für die laufende Version erneut aufrufbar.

**Wo die Daten liegen**: eine lokale SQLite-Datenbank (plus ihre kleine
WAL-Datei). Installiert: unter AppData, im Expertenmenü einsehbar und
änderbar. Portabel: direkt neben der exe. Der Wechsel zwischen beiden wird
erkannt und als einmalige Migration angeboten.

Ein Ordner, der zwischen zwei Computern synchronisiert wird (OneDrive,
Dropbox, Google Drive, iCloud Drive, Nextcloud), braucht Vorsicht: Schreiben
zwei Rechner gleichzeitig in dieselbe Datenbankdatei, kann sie beschädigt
werden. Wird der Datenordner auf einen solchen gesetzt, erscheint einmalig
eine Warnung dazu. Eine kleine Sperrdatei neben der Datenbank hält dann fest,
welcher Computer sie gerade verwendet, alle 30 Sekunden aufgefrischt; findet
ein zweiter Computer eine frische Sperre eines anderen Rechners, öffnet er
alles nur lesend und zeigt das dauerhaft in einer Kopfzeile, damit nichts
still verloren geht, während eine länger nicht aufgefrischte Sperre (der
andere Rechner ist vermutlich abgestürzt oder geschlossen) automatisch
übernommen wird. Der zugehörige Schalter, standardmäßig an, liegt im
Expertenmenü, Reiter System, Gruppe Fehlersuche. Das ist ein Sicherheitsnetz, kein
Weg, die Bibliothek über Computer hinweg zu teilen: dafür ist die
Markdown-Ordnersynchronisierung weiter unten der gebaute Weg.

**Sicherungen.** Verschlüsselte Sicherungen, Aufbewahrung nach Tag, Woche, Monat und Jahr,
gezieltes Wiederherstellen. Mit den zwei Buttons neben „Sicherung erstellen“ lässt
sich eine Sicherung in eine Datei deiner Wahl speichern oder aus einer Datei
importieren. Im selben Fenster außerdem ein Speicherbereich, der zeigt,
wie viel Platz Datenbank, Anhänge, Bildschirmfotos, Aufnahmen, Sicherungen,
temporäre Dateien und das Fehlerprotokoll jeweils belegen, mit einem Button,
um einen aufgeführten Ordner zu öffnen. Ein Aufbewahrungsbereich listet eine Zeile je Art
von Daten, die diese App selbst aufhebt (Papierkorb, Kopierverlauf,
Kopien aus anderen Apps, Aufnahmen, verworfene Aufnahmen, temporäre Exporte
und unfertige Bearbeitungsstände), mit dem aktuellen Stand, wie lange er
bleibt, einem Button zum sofortigen Aufräumen und einem Button, der direkt zur
zugehörigen Einstellung im Expertenmenü springt. Die Zahlen werden im
Hintergrund ermittelt und mit dem Zeitpunkt der Messung beschriftet, genau
wie beim Speicherbereich darüber, nicht bei jedem Öffnen neu berechnet. Zu
jeder Sicherung in der Liste lässt sich auf Wunsch auch der volle Inhalt
anzeigen: Anzahl der Einträge, Ansichten und Tags, die jüngste Änderung
darin, die Programmfassung, die sie erstellt hat, und ob ihre Zeilen normal
versiegelt sind oder auf Klartext zurückgefallen sind. Darunter jeder
enthaltene Prompt mit eigenem Änderungsdatum, Größe und Art, durchsuchbar
nach Namen und nach jeder Spalte sortierbar, mit einem Button, um genau
diesen einen Prompt direkt aus der Liste wiederherzustellen statt der
ganzen Sicherung.

**Nutzungsstatistik** (Einstellungen → Nutzungsstatistik): Ein kleines Fenster mit vier Dingen: der Zahl der
Prompts, den Kopien der letzten 7 Tage, den meistgenutzten Prompts mit Namen
(eine Liste mit 5, 10 oder 20 Einträgen, nach Wahl) und den nie genutzten
Prompts, die sich anklicken lassen, um sie in der Bibliothek zu finden. Der
Schalter „Statistik anzeigen“ schaltet die Anzeige ein; die Zahlen selbst
sammelt der Kopierverlauf. Alles bleibt auf diesem Rechner. Der Export-Button
speichert die vollständigen Zahlen samt Tageswerten und Werkzeugnutzung als CSV,
TXT oder JSON.

**Import & Export**: CSV, TXT oder JSON für den reinen Text, volle Sicherung
für alles. Ein Export-Button und eine Formatauswahl ersetzen die drei früheren
Einzelknöpfe; nur JSON trägt die Rotationstexte einer Kachel mit Rotation, Tags,
Notiz, Tastenkürzel und Farbe vollständig, statt sie wie CSV und TXT in ein
Feld zu quetschen. JSON ist das Standardformat des Export-Buttons und behält jedes Detail eines Prompts; CSV und TXT bleiben in der Auswahl verfügbar. Bilder reisen in keinem der drei mit, dafür gibt es die
volle Sicherung. Beim Import entscheidet der Dateiinhalt zwischen CSV und
JSON, nie die Dateiendung, und zeigt vorher eine Vorschau mit Name und
Duplikat-Kennzeichnung; eine volle Sicherung stellt direkt wieder her. Ändert
sich die Datei nach der Vorschau, wird der Import verweigert statt etwas
anderes zu importieren als angezeigt.

**Importieren** → **Aus Ordner…** in den Einstellungen macht aus einem Ordner voller `.md`/`.txt`-
Dateien Kacheln, eine je Datei, ohne die Originaldateien anzurühren. Der
Dateiname wird der Name der Kachel. Eine kurze erste Zeile wird die Notiz, der
Rest wird der Text. Jeder Unterordner wird ein Tag, einer je Ebene. Eine Datei,
die den Rotationstrenner schon enthält, wird ein Prompt mit Rotationstexten. Die
Vorschau listet jede Datei mit ihren Tags, ihrer Größe und einem Häkchen und
graut aus, was nicht gelesen werden konnte (zu groß oder kein reiner Text),
mit dem Grund dafür. Wie tief in Unterordner gesucht wird und bei wie vielen
Dateien Schluss ist, sind beides Einstellungen im Expertenmenü.

**Datenwartung**: Papierkorb, Import/Export. Den Duplikatfinder öffnet der
Button „Duplikate finden“ in der Bibliothek.
„Letzte Meldungen“ hält eine kurze Liste der Fehlermeldungen fest, die nur kurz als
Hinweis erschienen, mit Zeit und Ort; die Liste lässt sich in ihrem Fenster kopieren
oder leeren. Die Einstellungen allein lassen sich in derselben Gruppe in eine Datei
speichern und wieder laden.

**Papierkorb & Kacheln wiederherstellen**: gelöschte Kacheln landen zuerst im
Papierkorb (Aufbewahrung in Tagen, 0 = für immer). Löschen geht mit einem Klick,
eine Meldung mit Rückgängig holt die Kachel zurück. Wiederherstellen legt in die
erste freie Zelle der aktiven Ansicht, nicht zwingend an den Originalplatz, und
ohne schwebenden Button, Ordner-Mitgliedschaft oder Tastenkürzel von vorher.

**Rückgängig**: einige Aktionen außerhalb des Kachel-Papierkorbs tragen einen
eigenen Rückgängig-Button direkt auf der Meldung: einen Prompt oder mehrere löschen,
eine Ansicht löschen, den Kopierverlauf leeren, Tags löschen oder zusammenführen
und eine Sammeländerung von Tags. Es wartet
immer nur eine einzige Rücknahme; eine spätere zerstörende Aktion ersetzt sie,
und der Button verschwindet mit der Meldung selbst. Im Expertenmenü lässt sich
das abschalten, dann bleibt die Meldung eine reine Bestätigung.

**Duplikatfinder**: findet textlich sehr ähnliche Kacheln nach
Ähnlichkeitswert, keine exakte Übereinstimmung nötig, löscht nichts von sich
aus. Eine Kachel mit Rotation wird nur nach ihrem ersten Text verglichen.
Derselbe Ähnlichkeitswert steuert auch einen stillen Hinweis nach dem
Speichern: gibt es einen ähnlichen vorhandenen Prompt, zeigt eine Meldung
seinen Namen mit einem Button zum Öffnen und einem zum Vergleichen. Sie hält
das Speichern nie auf und fragt nichts, wer sie ignoriert verliert nichts.
Beide teilen sich eine Schwelle im Expertenmenü.

**Befehlszeile:** `prompt-saver.exe <Befehl> ...` nutzt die App als
Kommandozeilen-Werkzeug statt ihr Fenster zu öffnen: Kacheln auflisten, lesen,
kopieren, anlegen, ändern, löschen, wiederherstellen, Tags und Ansichten
verwalten, suchen, exportieren, importieren, Sicherungen anlegen, auflisten oder
wiederherstellen, dazu eine einzelne Einstellung lesen oder schreiben. Läuft
die App bereits, geht der Befehl an diese laufende Kopie, und ihre Antwort
erscheint hier. Sonst arbeitet der Befehl allein, direkt gegen die Datenbank,
und beendet sich, ohne je ein Fenster zu zeigen. So greift immer nur ein
Prozess auf die Datenbank zu.

Jeder Befehl kennt `--help`. `--json` liefert die Ausgabe stattdessen
maschinenlesbar. Ein Befehl, der etwas ändert, braucht `--yes`, sonst zeigt er
nur, was er täte, und ändert nichts. Ausnahmen sind `copy`, das nur den Text
einer Kachel in die Zwischenablage liest, `backup create`, das nur je einen
neuen Schnappschuss hinzufügt, und `export`, das `--yes` nur zum
Überschreiben einer bereits vorhandenen Datei braucht. Rückgabewerte: 0
Erfolg, 1 falscher Aufruf, 2 nicht gefunden, 3 Bestand gesperrt oder nicht
erreichbar, 4 abgelehnt (fehlendes `--yes`, oder ein Wert, der die Prüfung
nicht bestanden hat). Vollständige Befehlsliste: `prompt-saver.exe --help`.

Für ein Skript oder ein anderes Programm gibt `prompt-saver describe` die
vollständige Befehlsliste als JSON aus, und
[docs/automation.md](automation.md) (englisch, wie der Rest des Verzeichnisses)
ist die vollständige Referenz: jeder Befehl mit seinen Argumenten und der Form
seiner JSON-Antwort, die Fehlercodes und durchgerechnete Beispiele, nichts
davon muss durch Ausprobieren herausgefunden werden.

**Vorführung**: Ein Schalter im Expertenmenü (Tab System, Gruppe Fehlersuche) schaltet das
ganze Fenster auf einen mitgelieferten Beispielbestand um, der möglichst
viele Funktionen auf einmal zeigt: Prompts mit und ohne Bild, ein
Prompt mit Rotation, zwei einfache Prompts für den Anfang einer Kette, ein
Platzhalter, Tags, ein Favorit, zwei Ansichten, ein Ordner, ein Anhang und
(wenn vorhanden) ein Video. Das Einschalten baut den Beispielbestand jedes
Mal frisch auf und startet die App darin neu; der echte Bestand wird dabei
nie geöffnet, geschrieben oder auch nur gelesen. Eine dauerhafte Marke bleibt
im Fenster sichtbar, solange die Vorführung läuft, damit niemand den
Beispielbestand für den eigenen hält. Das Zurückschalten fragt zuerst nach,
weil alles, was während der Vorführung geändert wurde, dabei verloren geht,
und startet dann genau in den echten Bestand zurück. Derselbe
Beispielbestand ist auch die Vorlage für das Bild in diesem Hilfefenster und
für `docs/screenshot.png`: der genaue Ablauf steht im Abschnitt „Building
from source" der README.
