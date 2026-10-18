## Prompt Saver 2.7.0

Twenty-five new features: tags, custom themes, folders for view tabs and for
floating buttons, self-filling and choice variables, a markdown preview with
syntax colors, a standalone quick launcher, multi-select in the grid, a trash
for deleted prompts, placing one prompt in several views at once, restoring a
backup's data and settings separately, rotating prompts, private notes,
per-prompt hotkeys, drag-out into other apps, a tray quick-copy menu, a screen
color picker, a screenshot editor, offline text recognition (OCR), offline
voice dictation, screen recording, a video editor, and a first-run
introduction tour.
Everything new is an expert-menu switch, most of it off until you turn it on
(text recognition, voice dictation, the screenshot editor, screen recording,
the video editor and the introduction tour ship on).

### Downloads

| File | Use it when |
|---|---|
| `Prompt.Saver_2.7.0_x64-setup.exe` | Installer: choose **all users** or **current user only**; upgrades replace the previous version automatically, your data is kept |
| `prompt-saver.exe` | Portable standalone version: one file, no installation, keeps its data next to itself |

Requires the Microsoft WebView2 runtime (preinstalled on Windows 11 and current Windows 10). If it is missing, the app offers the official Microsoft installer on first start.

### Getting started

- **Introduction tour**: a five-step walkthrough on first launch: what the app does and that it stays offline, saving and using a tile, organizing tiles and views, the library and search, then a quick table of the three optional features (screenshot editor, color picker, OCR) with a one-click reset to their shipped default. Nothing is written until you finish. Reopen it any time from Settings; ships on.

### Organise

- **Tags**: label prompts with free-form, colored tags. Type-ahead chips in the editor, a multi-select filter row in the library that matches all or any of the picked tags and stacks with the search, and a manager to rename, recolor or delete a tag everywhere at once.
- **View groups**: drag one view tab onto another and they become a folder tab with a popover listing its views. Rename, recolor or dissolve it from its context menu; Ctrl+Tab cycles through views in visual order.
- **Private notes**: a collapsible notes row per prompt for context you keep but never copy. Shown dimmed in the tile tooltip, found by the search, never on the clipboard.
- **Multi-select in the grid**: point at a tile to reveal a checkbox; a toolbar above the grid then recolors, moves to another view, or deletes everything checked at once.
- **Trash**: deleting a prompt moves it to a trash instead of removing it right away. Restore it or delete it forever from the expert menu's Data group, or let it clear itself after a retention period you choose (30 days by default, or never).
- **Prompt in multiple views** (expert menu, off by default): right-click a tile and pick which other views should also show it; editing it in one place updates every view.
- **Quick launcher**: the global hotkey (Ctrl+Alt+P by default) opens a small standalone search window instead of the main library: type to fuzzy-find a prompt, Enter or click copies it instantly.

### Write

- **Self-filling variables**: `date`, `time`, `datetime`, `weekday`, `clipboard` and `uuid` fill themselves in on copy with no dialog, formatted in the app language. A prompt that mixes them with your own variables only asks for yours.
- **Choice variables**: `{?tone|formal, casual, witty}` turns into a dropdown in the copy dialog and remembers your last pick.
- **Markdown preview**: the editor shows the prompt rendered: headings, lists, quotes, emphasis and code blocks. One field, not two: the rendered view stands where the text field was and hands it back the moment you click into it, in the same box at the same size. Code is syntax-colored for JavaScript, TypeScript, Python, HTML, CSS, JSON, SQL, Bash and Rust.
- **Rotating prompts**: a text prompt can hold a list of texts instead of one; each copy serves the next entry, in order or shuffled, with a small badge on the tile showing the position. Entries travel with export/import, backups, Markdown-sync and version history, and the search finds a match inside any of them. An eligible entry can also auto-advance on every real paste instead of only on copy.
- The fill-in dialog gets a "Clear all" button, and saving a prompt with an empty `{?}` placeholder now warns first instead of leaving it stuck in the copied text.
- **One short placeholder spelling**: `{?Name}`, `{?Name=default}`, `{?Name|A, B}` and `{?@date}`. Prompts, history, trash and imports with the old `{#{…}#}` spelling are converted automatically.

### Copy from anywhere

- **Per-prompt hotkeys**: give a prompt its own global key combination that copies it from any app, even with Prompt Saver in the tray, and optionally pastes it straight away.
- **Tray quick menu**: the tray menu can list your recent and favorite prompts; one click copies without opening the window. It draws as a rounded, themed window in the Windows 11 style instead of the plain system popup; an expert-menu switch falls back to the plain menu.
- **Drag-out**: hold Ctrl and drag a tile, library row or floating button into another app to drop its text there.
- **Floating-button folders**: drag one floating button onto another and they merge into a folder showing up to four previews and a count. Click to open it in place, drag a member out to free it, and it dissolves when one is left.

### Capture and read

- **Screenshot editor**: every capture opens for editing before it reaches a tile, the clipboard or a folder: crop, pen, marker, a highlighter-style blend, line, arrow, rectangle, ellipse, text, a self-numbering step marker, pixelation, a spotlight that dims everything outside a chosen area, resize (the image's actual pixel size, with width/height/percent fields and presets), and a hand tool to pan around while zoomed in (also Space or the middle mouse button) for precise work on a large capture. The crop tool shows a resize cursor at each handle and a move cursor inside the current selection. Full undo/redo, an eyedropper, live width×height while dragging, snapping to the image edges and center. Cancel keeps the original untouched; Apply re-copies the edited image. Ships on.
- **Text recognition (OCR)**: pull editable text out of a screenshot or an image file, fully offline through Windows' own recognizer. A composer button grabs a new screenshot or a file; the screenshot dialog also gets a "Recognize text" button for the capture you already have. The result opens ready to copy or save as a new prompt, with small text enlarged first so it reads reliably. Missing a language pack? A "Manage languages" dialog lists everything Windows can recognize with the download size shown up front, walks through Windows' own installer, and can keep installed packs current automatically. Ships on; the button hides itself if Windows has no recognizer at all.
- **Voice dictation**: a microphone button beside the composer and in the editor turns speech into text through Windows' own on-device recognizer, offline unless you explicitly allow online recognition. Spoken punctuation and commands ("comma", "new line", "delete that") are applied as you talk, in the app language, and a global hotkey dictates straight into whichever field is waiting. Language packs are managed from the same dialog as OCR's. Ships on; the button hides itself if Windows has no speech engine at all.
- **Screen recording**: a record button next to the screenshot button opens the same overlay to pick a window, a dragged region, or the whole screen, with an optional countdown before capture starts. A small floating control bar (elapsed time, live file size, Pause/Resume, Stop, Discard) follows a moved window and stays out of its own recording; two optional global hotkeys and the tray menu can stop or pause/resume it too, live only while it is actually running. System audio and microphone audio both record on by default as separate tracks; the microphone track is captured but not mixed in audibly until you bring it in yourself from the video editor below (switchable off in the expert menu if you would rather it never record); an unplugged microphone just marks its track as lost instead of stopping anything. Stopping offers Save, Save as…, Copy, Edit or Discard, Save/Copy sharing one recording folder. Recording always writes MP4; three quality steps and an optional WebM, AVI, MOV, MKV, animated WebP or APNG format convert it afterward through ffmpeg (offered as a download when missing); GIF, WebP and APNG are capped at 30 seconds and carry no audio, always falling back to the untouched original on a failed or canceled conversion. A recording that never reached its own stop path is removed automatically after a grace period.
- **Video editor**: opens from a recording's result dialog or any video prompt's right-click menu: trim, crop, speed, fades, resize (width/height/percent fields, an aspect-ratio lock, and presets from 4K down to 25%) and image/freeze-frame/text/pixelate/spotlight overlays on a zoomable filmstrip-and-waveform timeline, with a live preview and no processing until export. The preview itself zooms with the scroll wheel (up to 8×) and pans by dragging, for placing a small overlay precisely on a large recording. A few export presets (unchanged quality, smaller for chat, GIF) remember your last choice. Export format also covers AVI, MOV, MKV, animated WebP and APNG besides MP4, WebM and GIF, each with a short note on what it's for; GIF, WebP and APNG never carry audio and warn once a clip runs long, since all three grow large fast.

### Also in this release

- **Signed updates**: the updater installs only an installer signed by the publisher and rejects anything else, even if the download page was tampered with.
- **Confidential prompts stay out of Win+V**: copying one keeps it out of the Windows clipboard history and the cloud clipboard.
- **Each window only does its own job**: a side window such as the floating buttons can no longer delete data, move the data folder or read the remote access password.
- **Moving the data folder takes attachments and recordings along** and offers to clean up the old folder, naming what gets deleted.
- **Grid folders**: drop one tile onto another and both move into a folder in that cell; drag one out and it goes back to the layout.
- **Restore data and settings separately**: restoring a backup, or importing a `.psb`/`.json` backup file, now offers two checkboxes instead of an all-or-nothing swap: bring back your prompts without touching your current setup, or the other way around.
- **Markdown folder sync** (expert menu, off by default, beta): mirrors every prompt as its own readable `.md` file in a folder you pick, kept in sync both ways. The files are plain text (the database is not), so keep anything sensitive out of a folder other people or services can read. Switching to a different folder asks what should happen to the old one, and a delete there only ever removes the files the app wrote itself.
- **An About dialog** and **a real help search** that matches several words in any order and highlights what it found.
- **Allow multiple instances** (expert menu, off by default): run the app more than once at the same time, e.g. against two different data folders. Off keeps the familiar single-instance behaviour. Takes a restart to apply.
- The backups window can open its snapshot folder in Explorer directly.
- **Taskbar progress and a remaining-time estimate**: a long export, recording
  conversion, ffmpeg download, backup or import fills the app's Windows
  taskbar icon with its progress, and its progress bar shows a plain "about X
  left" once enough has run for the guess to mean something. Both switchable
  off in the expert menu.
- **Plain-language ffmpeg errors**: a failed video export or recording
  conversion now names the actual reason (file incomplete, no space left, no
  access, and so on) instead of a raw ffmpeg log line. The log line itself
  stays reachable underneath in a closed "Technical details" row with a copy
  button; a cancel was never shown as a failure to begin with.
- Dragging a button, tab or tile onto another shows what would happen: the target dims behind a folder mark in the accent colour, the same gesture in all three places.
- Dropdowns and menus flip above their anchor when there is no room below, and never open past the edge of the window.
- The copy history reads like the library: same rows, tags, a type chip and a quiet timestamp.
- The favourites section of the tray menu is on out of the box.
- The privacy switch that keeps the window out of screenshots is now called **Allow screenshots of Prompt Saver** and reads from your side. It ships off, so the window is still left out of screen captures, but if your screenshots come out empty, the switch that explains it is under privacy in the expert menu.
- A backup is either whole or absent: a failed one leaves nothing behind.
- **You are told when your data cannot be encrypted.** Windows seals the database with per-account encryption; on the rare machine where it refuses (almost always a damaged user profile), saving keeps working in readable form rather than losing anything, and you now hear about it: once at startup and permanently under privacy in the expert menu, with no switch to silence it. The next save that succeeds seals everything again by itself.
- "Duplicate" on a prompt now opens the create dialog prefilled with everything from the original, instead of a silent copy: change something first, or cancel and leave nothing behind.

### Make it yours

- **Custom themes**: build your own on top of any of the 16 built-ins: a color input per themable value, grouped and previewed live, saved into the theme dropdown. The floating buttons follow along.
- **Color picker**: a pipette beside the screenshot button freezes the screen and shows a magnified pixel grid with a live readout. Click to copy the color as hex, rgb or hsl; nothing is ever saved.

The complete list is in the [changelog](../../blob/master/CHANGELOG.md).
