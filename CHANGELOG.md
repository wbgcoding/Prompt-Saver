# Changelog

All notable changes to **Prompt Saver**. Download the latest version from the
[releases page](../../releases/latest).


## 2.7.0 (unreleased)

Twenty-five new features: tags, custom themes, folders for view tabs and for
floating buttons, self-filling and choice variables, a markdown preview with
syntax colors, private notes, per-prompt hotkeys, drag-out into other apps, a
tray quick-copy menu, a screen color picker, a standalone quick launcher,
multi-select in the grid, a trash for deleted prompts, placing one prompt in
several views at once, restoring a backup's data and settings separately,
rotating prompts, a screenshot editor, offline text recognition (OCR), offline
voice dictation, screen recording, a video editor, and a first-run
introduction tour.
Everything new is an expert-menu switch, most of it off until you turn it on
(text recognition, voice dictation, the screenshot editor, screen recording,
the video editor and the introduction tour ship on).

### Added

- **A short-lived certificate for Apple devices.** A new expert switch in the
  remote access group makes the receiver's certificate valid for 825 days
  instead of ten years, which iPhone, iPad and Mac require. Switching asks
  first and creates a new certificate, so devices have to trust it again. A
  stored certificate that has run out is now replaced when the receiver
  starts.
- **Eight more expert settings for things that were fixed values.** A switch
  and a colour for the frame around a region recording (it still never appears
  in the video), a choice of what goes between two chained prompts (blank line,
  line break, space or nothing), how many recent searches the search window
  keeps (0 keeps none), how many results the quick launcher lists, and, for the
  video editor, the starting outline width of a new text overlay, the colour of
  a pressed key in a new keys overlay and the starting volume of a newly
  inserted video overlay. Every default is the old fixed value.
- **Command line.** `prompt-saver.exe <command> ...` runs the app as a
  command line tool instead of opening its window: list, get, copy, add,
  edit, delete and restore prompts, manage tags and views, search, export,
  import, create/list/restore backups, and read or write one setting. Talks
  to an already running copy over its own channel when there is one, or runs
  alone against the database with no window otherwise, so exactly one
  process ever touches it at a time. Every command takes `--help`, `--json`
  switches its output to machine readable form, and any command that changes
  something is refused without `--yes`. Full list: `prompt-saver.exe --help`.
- **Remote access and promptsaver:// links.** Two switches, both off by
  default. A `promptsaver://open/<id>` link brings the window to that
  prompt, and `promptsaver://copy/<id>` copies it; a link can never delete
  or change anything, an unknown id does nothing, and the first open and the
  first copy each ask for confirmation once. The remote access receiver
  exposes the same commands as the command line to another program on this
  computer, loopback only unless a separate network switch opens it to
  address ranges you configure. It refuses to start without a password,
  generated and shown once when you turn it on; ten wrong attempts lock out
  the caller for ten minutes. Every request is capped in size, rate limited,
  logged without the password, and a locked prompt stays locked here too. A
  banner stays on screen with the port number and its own off switch while
  the receiver runs.
- **Automation reference.** `docs/automation.md` documents the command line
  and the remote access receiver for a script or another program: every
  command's arguments and JSON response shape, the error codes, the link
  scheme, and worked examples. `prompt-saver.exe describe` prints the same
  command list as JSON.
- **Tags** — label prompts with free-form, colored tags. The editor gets a chip
  row with type-ahead over existing tags and a per-chip color popover; the
  library gets a filter row above the results (multi-select, switchable between
  matching all or any of the picked tags) that combines with search and the
  other filters, plus a small manager to rename, recolor or delete a tag
  everywhere at once, with a usage count per tag. Tags travel with TXT and CSV
  exports. The copy history gets its own tag filter alongside its type and
  color filters, same picking logic as the library's.
- **Custom themes** — build your own theme on top of any of the 7 built-in
  ones: a palette button beside the theme dropdown opens an editor with a color
  input per themable value, grouped by background, surfaces, text and accent,
  with a live preview while you edit. Saved themes appear in the dropdown under
  a separator, and the floating buttons follow them too.
- **View groups** — collect view tabs into a folder tab. Drag one tab onto
  another to group them; the group tab opens a popover listing its views, and
  its context menu renames, recolors or dissolves it. Ctrl+Tab cycles through
  views in visual order.
- **Drag a tile onto a view tab** — files it into that view; hold Ctrl while
  dropping to copy it there instead, so it stays in both. Hovering a collapsed
  view group opens it after a moment, so the drop can target a view inside.
  A target with no room left refuses the whole drop and says how many cells
  are missing.
- **Self-filling variables** — `date`, `time`, `datetime`, `weekday`,
  `clipboard` and `uuid` fill themselves in on copy, with no dialog; a prompt
  mixing them with your own variables only asks for yours. Date and time follow
  the app language, with the format switchable in the expert menu. The editor
  lists all six as insertable chips with a one-line explanation each.
- **Choice variables** — write `{#{tone|formal,casual,witty}#}` and the copy
  dialog offers a dropdown instead of a text field, remembering your last pick.
- **Markdown preview** — the editor shows the prompt rendered: headings, lists,
  quotes, rules, emphasis and fenced code blocks. It is ONE field, not two: the
  rendered view stands where the text field was and hands it back the moment you
  click into it, in the same box, at the same size. Code blocks are syntax-colored
  for JavaScript, TypeScript, Python, HTML, CSS, JSON, SQL, Bash and Rust.
- **Private notes** — a collapsible notes row on every prompt for context you
  want to keep but never copy. Notes show as a dimmed section in the tile
  tooltip, are found by the library search, and travel with TXT and CSV
  exports — but never reach the clipboard through any copy path.
- **Per-prompt hotkeys** — give a prompt its own global key combination that
  copies it from anywhere, even with the app in the tray, and optionally pastes
  it straight away. Conflicts with the app hotkey or another prompt are refused
  in the editor. A tile with its own hotkey carries a small keyboard badge, on
  by default.
- **Drag-out** — hold Ctrl and drag a tile, a library row or a floating button
  into any other app to drop its text there. The key is switchable, and a second
  switch swaps the two gestures so a plain drag leaves the app and the key moves
  the tile instead.
- **Tray quick menu** — the tray icon's menu can list your recently used and
  your favorite prompts; clicking one copies it without opening the window. It
  draws as a rounded, themed window in the Windows 11 style instead of the
  plain system popup; an expert-menu switch falls back to the plain menu. How
  many characters a prompt name may use before it is cut off is also tunable.
- **Floating-button folders** — drag one floating button onto another and they
  merge into a folder button showing up to four mini previews and a count. Click
  it and it opens in place into a small grid; drag a member out to free it
  again, and the folder dissolves when one is left.
- **Color picker** — a pipette next to the screenshot button freezes the screen
  and shows a magnified pixel grid with a live readout; click to copy the color
  as hex, rgb or hsl. Nothing is saved — the value only goes to the clipboard.
- **Grid folders** — drop one tile onto another and both move into a folder in
  that cell. The folder shows a preview of what is inside, opens its contents in
  a small grid, and hands a prompt back to the layout when you drag it out.
- **Allow multiple instances** (expert menu, off by default) — opens the door to
  running the app more than once at the same time, for example against two
  different data folders side by side. Off keeps the usual behaviour: a second
  launch just brings the first window forward. Takes effect after a restart.
- **Quick launcher** — the global hotkey (**Ctrl+Alt+P** by default) now opens a
  small standalone search window instead of the main library: type to fuzzy-find
  a prompt by name or text, press Enter or click to copy it instantly. A fresh
  install starts with the hotkey already bound; an existing one keeps whatever
  you already had, including turned off.
- **Multi-select in the grid** — point at a tile to reveal a checkbox in the
  corner; once anything is checked, every tile shows one. A toolbar appears
  above the grid to recolor, move to another view, or delete everything you
  picked at once.
- **Trash** — deleting a prompt now moves it to a trash instead of removing it
  right away. Restore it or delete it forever from a dialog in the expert
  menu's Data group, or let it clear itself automatically after a retention
  period you choose (30 days by default, or never).
- **Prompt in multiple views** (expert menu, off by default) — right-click a
  tile and pick which other views should also show it. It stays the same
  prompt everywhere: editing it in one view updates it in all of them.
- **Restore data and settings separately** — restoring a backup now offers two
  checkboxes instead of an all-or-nothing swap: bring back your prompts
  without touching your current setup, or the other way around. Importing a
  `.psb`/`.json` backup file gets the same choice.
- **Rotating prompts** — a text prompt can hold a list of texts instead of one;
  each copy serves the next entry, in order or shuffled, and a small badge on
  the tile shows the position. The editor turns the text field into a numbered
  list with a mode switch and a "from the start" reset. Entries survive
  export/import, backups, Markdown-sync and version history, and the library
  search finds a match inside any entry, not just the first. An eligible entry
  (2+ items, no image/file) can also auto-advance on every real paste instead
  of only on copy — a per-tile switch in the editor or a tile's right-click
  menu, tunable session timeout and timing window in the expert menu, and a
  toast instead of a loop if pasting happens too fast too often.
- **Markdown folder sync** (expert menu, off by default, beta) — mirrors every
  prompt as its own readable `.md` file in a folder you pick: a short header
  with name, color, tags and notes, then the text. Edit a file by hand or with
  another tool and the change comes back a few seconds later (the poll interval
  is tunable in the expert menu). The header also
  carries where the prompt sits — view, grid cell, grid folder and pill folder —
  so a second machine reading the same folder reproduces the arrangement,
  creating a view or folder it does not know yet by name. An attached file, icon
  or PDF page travels as a path reference, never as data; a screenshot pasted
  straight into a prompt has no path and stays in the database. Switching the
  feature on takes a backup first, and the folder is written the moment you pick
  it. **The files are plain text** — the database is not — so keep anything
  sensitive out of a folder other people or other services can read. The folder
  is `prompt-saver-sync` next to your data unless you pick another one, and
  picking another one asks whether the old folder should be deleted: that delete
  only ever removes the files this app wrote itself, never a sub-folder, and
  stops without touching anything if a file it did not write is in there. A file
  is read once it has stopped changing, so a half-written file is never imported;
  an unreachable folder pauses the sync after five failed rounds instead of
  retrying forever, and the folder row shows the last round, the number of files
  and why it is paused. When a prompt and its file changed at the same time,
  neither version is lost: the one that is replaced is kept, in the prompt's
  version history or as a conflict copy beside the file.
- **An About dialog** — a button beside Check for updates opens the current
  version and a link to the project page.
- **A real help search** — searching now matches several words in any order,
  ranks title hits above body hits, highlights the matching words, shows which
  chapter each result belongs to, and collapses results down to one line each
  so a broad search stays scannable instead of a wall of expanded text.
- **Diagrams in the help window**: five small hand-drawn diagrams (the way to
  copy a prompt, the three folder kinds, a placeholder's parts, the video
  editor timeline, a chain of steps) next to the articles they belong to,
  drawn in the current theme's own colours instead of a screenshot that would
  go stale with the next redesign. Off switches back to text only.
- **Real button icons in the help text**: wherever an article names a button
  (screenshot, record, OCR, microphone, library, theme editor) the sentence
  now shows that button's actual icon, read live from the button itself so it
  always matches, instead of only the word.
- **Twenty-one settings that were fixed constants are now expert values**: the
  colour-history swatch size, the colour-picker loupe grid size, screenshot
  quality and an optional capture delay, a rotating prompt's tooltip preview
  length, the Markdown-preview and
  syntax-highlight size caps, the version-history diff word cap and preview
  length (with a switch to turn the word diff off entirely), the drag-vs-click
  distance, the prompt box's max height, popover edge padding, the toast width
  and position, dialog max height, the library search delay, the copy-history
  window width, the tag-filter popover's column count and sort order, how long
  a confirm button stays armed, and the background update-check interval.
- **A screenshot editor** (on by default) — every capture opens for editing before
  it reaches a tile, the clipboard or a folder. Thirteen tools: crop, pen, marker,
  highlighter-style multiply blend, line, arrow, rectangle, ellipse, text, a
  self-numbering step marker for annotating instructions, pixelation, a
  spotlight that dims everything outside a chosen area, and a hand tool to pan
  around while zoomed in (also reachable by holding Space or the middle mouse
  button) for pixel-precise work on a large capture. The crop tool shows a
  directional resize cursor at each handle and a move cursor inside the current
  selection. Full undo/redo, an eyedropper, live width×height while dragging a
  crop or shape, snapping to the image edges and center, nudging the
  last-placed mark with the arrow keys, and holding **V** to peek at the
  unedited shot. A resize action changes the image's actual pixel size (width,
  height or percent fields, presets for 25/50/200% and "fit 1920 wide") as an
  undoable step of its own, with a halving-step downscale for readable text and
  an expert setting to pick sharp/nearest-neighbour resampling for pixel art.
  Cancel leaves the original capture untouched; Apply re-copies
  the edited image and re-saves it to the screenshot
  folder if that setting is on.
- **Text recognition (OCR)** (expert menu, on by default) — pull editable text
  out of a screenshot or an image file, fully offline through Windows' own
  recognizer. A composer button opens a small menu to pick an image file or grab
  a new screenshot; the screenshot dialog also gets a "Recognize text" button that
  works on the capture you already have. The result opens in an editable dialog
  with the detected language and character count, ready to copy or create as a
  new prompt. Small on-screen text is enlarged before recognition so it reads
  reliably, wrapped lines can be joined back into paragraphs, and the result can
  copy itself to the clipboard automatically — all three tunable in the expert
  menu, along with which installed language to use. Missing a language pack? A
  "Manage languages" dialog lists everything Windows can recognize, installed or
  not, with the download size right on the button so nothing downloads blind;
  picking one walks through Windows' own installer, asking for administrator
  rights exactly once and explaining why beforehand. On systems where Windows
  demands administrator rights even to list the packs, the dialog says so and
  offers to run that listing elevated — it only lists, it installs nothing.
  Installed packs can also be kept current automatically, with a quiet,
  cancelable notice when one needs an update.
- **Voice dictation** (expert menu, on by default) — a microphone button next
  to the composer and in the editor turns speech into text through Windows'
  own on-device recognizer, fully offline unless online recognition is
  explicitly allowed. Text streams in as it's recognized; click the button
  again, press Escape, or save/close the editor to stop. A spoken vocabulary
  of punctuation and formatting commands ("comma", "new line", "new
  paragraph", …) turns into real characters, capitalization follows
  automatically after a sentence or paragraph break, and saying "literal"
  before a word types it instead of triggering it as a command — all
  tunable, and all localized to each of the 11 UI languages. Interim
  results, auto-capitalization, a silence auto-stop and a hard time cap are
  each their own switch; insertion position (cursor or end), the recognition
  language and a global hotkey (delivering to the focused field, the
  clipboard, or pasted directly) round it out. Missing a language pack opens
  the same "Manage languages" dialog OCR uses, shared install and elevation
  flow included.
- **Introduction tour** (expert menu, on by default) — a five-step walkthrough
  on first launch, opened right after the example prompts are seeded so step
  two already has a tile to point at: what the app is and that everything
  stays offline, how to save and use a tile, organizing tiles and views,
  the library and search, then a table of the three optional features
  (screenshot, color picker, OCR) with the shipped default shown next to
  each and a one-click reset back to it — nothing is written until you
  finish, and only the switches you actually changed. Step-by-step dots,
  ←/→ and Enter to navigate, Esc or "Skip" to leave any time — either asks to
  confirm first. Reopen it later from the settings dialog or the expert menu;
  a data-folder or WebView2 prompt on first launch always wins, and the tour
  just waits for the next one.
- **Start a chain directly from a tile** — hovering a text prompt now shows a
  small chain icon next to its selection checkbox; clicking it starts a chain
  with that tile as the first entry, skipping the header button. Switchable
  off on its own in the expert menu (on by default). The icon can also be
  switched to stay dimly visible all the time instead of appearing only on
  hover, brightening on hover or keyboard focus either way.
- **Screen recording** — a record button next to the screenshot button opens the
  same picker overlay: click a window, drag a region, or click empty desktop
  for the whole screen. An optional countdown precedes capture, and a small
  floating control bar (elapsed time, live file size, Pause/Resume, Stop,
  Discard) follows a moved window and is itself never part of the recording;
  two optional global hotkeys and the tray menu can also stop or pause/resume
  it, live only while a recording is actually running. System audio and
  microphone audio both record on by default, each as their own track — the
  microphone track is captured but not mixed in audibly until you bring it in
  yourself, from the video editor below (turn microphone capture off in the
  expert menu if you would rather it never record at all); unplugging the
  microphone mid-recording keeps picture and system audio going and just marks
  that track as lost. Stopping offers Save, Save as…, Copy, Edit or Discard —
  Save and Copy share one recording folder (changeable in the expert menu or
  right from the dialog). MP4 (H.264) is what gets written while recording;
  three quality steps (Small/High/Lossless) and an optional WebM, AVI, MOV,
  MKV, animated WebP or APNG format convert it afterward through ffmpeg
  (offered as an inline download when missing) — GIF, WebP and APNG are
  capped at 30 seconds and carry no audio. A failed or canceled conversion
  always falls back to the untouched original recording, and a write error (for example a full disk)
  stops the recording cleanly and keeps whatever was captured so far. If the
  recorded window closes on its own, the recording stops automatically. A
  recording that never reached its own stop path (a crash, a forced shutdown)
  is removed automatically once it clears a grace period, both adjustable in
  the expert menu alongside a manual clean-up action.
- **Video editor** — opens from a recording's own result dialog or from any
  video prompt's right-click menu, in its own window with a live preview and
  no processing until you export. Trim and cut (a lossless fast path applies
  when nothing else changes), crop, change speed, fade in/out, resize (width/
  height/percent fields, an aspect-ratio lock, and presets from 4K down to
  25% — a preset never enlarges past the source, but the percent field and
  free typing can), and overlay images, freeze-frames, text, pixelation or a
  spotlight anywhere on a zoomable filmstrip-and-waveform timeline. The preview itself zooms with the
  scroll wheel (up to 8×) and pans by dragging — the middle mouse button
  always, the left button once zoomed in — for placing a small overlay
  precisely on a large recording. A few export presets (unchanged quality, a
  smaller size for chat, an animated GIF) remember your last choice. Export
  format also reaches beyond MP4, WebM and GIF to AVI, MOV, MKV, animated
  WebP and APNG, each with a one-line note on what it's for; GIF, WebP and
  APNG never carry audio, warn past ten seconds and get their long side
  capped, since all three grow large fast. Undo/redo covers every edit,
  capped by an adjustable limit.
- **Every value a screenshot or video editor sets automatically when it opens**
  — stroke width and color per drawing tool, the video editor's start tool,
  starting tab and default font, and more — now has its own expert-menu row
  for the default, alongside a new row for the video export folder (which
  already remembered your last choice; it just had no row of its own before).
- **The video editor now remembers unfinished cuts and overlays.** Closing it
  with something not yet exported offers to pick up right where you left off
  the next time you open the same recording; declining discards the saved
  state. Kept for 30 days by default (adjustable, zero keeps it forever) and
  cleaned up automatically once the recording itself is gone.
- **The video editor can now insert another video, not just images.** "Add a
  video" offers it either as a new overlay kind (its own rectangle, time
  range and volume, muted by default, playing once or on repeat) or as a
  whole extra section spliced in at the start, the end or the current
  position. Two files rarely share a resolution, frame rate or sound format,
  so every inserted section is matched to the main video's own values first;
  a section with no sound of its own gets silence instead of shifting
  everything after it. Each extra source is checked once when picked, up to
  a small fixed number per project, and stays visibly marked as missing if it
  is later moved or deleted.
- **Open an existing file in the editor.** Right-click the screenshot or
  record button, or an image/video attachment in the library, for "Open a
  file in the editor". One file dialog covers both kinds; the file's own
  content decides which editor opens, not its extension, so a video saved
  with a picture extension still opens the video editor. The editor always
  works on a copy, never the original, and saving asks where to put the
  result, starting in the original file's own folder. A file past a
  configurable size is rejected before it loads.
- **The expert-menu search now also finds a hit outside the expert menu** —
  the normal settings page, the toolbar's grid-size control, the library's
  sort order, or a setting in the backup manager. A match appears in its own
  section with a small badge saying where it lives; clicking it opens the
  right place and briefly highlights the row.
- **Search everything**: a magnifier icon in the toolbar, or Ctrl+K, opens
  an overlay searching prompts, copy history, attachments, views, settings
  and help all at once, each in its own group with up to five hits and a
  "show more". Arrow keys move the highlight across group boundaries; Enter
  runs the fitting action for whatever is highlighted. The field remembers
  the last five searches. Expert menu switch, on by default, with a value
  for how many hits each group shows.
- **Demo mode**: an expert-menu switch swaps the whole window to a bundled
  sample library covering most features at once (prompts with and without
  an image, a rotating prompt, a placeholder, tags, a favorite, two views,
  a folder, an attachment and a video), never touching the real data.
  Rebuilds the sample library fresh every time and restarts into it. A
  permanent marker stays visible while it is on; switching back asks first,
  since anything changed during the session is discarded. The same sample
  library is also the source for the help window's picture and for
  `docs/screenshot.png`.
- **Duplicate a shape or overlay** — right-click a rectangle, ellipse, line,
  arrow, spotlight or pixelation in the screenshot editor, or an overlay's row
  in the video editor, for a menu to duplicate or delete it. A duplicate keeps
  the same values and time range and is offset slightly so it is easy to spot
  next to the original; the offset is adjustable. The video editor also
  remembers the last colour, thickness, opacity and font size used for each
  overlay kind and starts the next one of that kind from it, across restarts
  too, with an expert switch to turn that off again.
- **Snapping while dragging, in both editors.** A shape (screenshot editor) or
  a crop box or overlay (video editor) now snaps to the canvas or frame edges,
  its centre in both directions, and the edges and centre of every other shape
  or overlay already placed. A thin accent-coloured guide line marks whichever
  line it caught, gone again the instant the drag ends. Holding Alt turns it
  off for that one drag; each editor keeps its own on/off switch, sharing one
  distance setting between them. The screenshot editor already snapped to the
  image edges and centre before this; it now also catches other shapes.
- **Text outline, in both editors.** The text tool's background is now a
  choice of none, a solid plate or an outline, so white or light-coloured
  text stays readable on a light background too. Plate was the only option
  before and stays the default; an expert setting picks the outline width,
  and its colour, black or white, follows the text colour for the best
  contrast.
- **Move a selected shape or overlay with the arrow keys.** Holding Ctrl
  resizes it instead of moving it, and Shift takes a bigger step; both step
  sizes are expert settings shared by every place that uses them. In the
  video editor this only applies while something is selected, since the
  arrow keys normally step through frames there; comma and period take over
  that job for as long as it is selected, and a short line above the
  transport controls always says what the arrow keys currently do. Escape
  deselects and hands the arrow keys back.
- **Compare against the original, in both editors.** A toolbar button (and
  the V key) shows the untouched capture while held down, or locks that view
  on with a tap until tapped again; a small badge marks it either way so it
  is never mistaken for the edit. In the video editor this only hides
  overlays, since fades and the export resolution were never shown in the
  live preview to begin with, and the current cuts stay in effect.
- **Video length, resolution and file size in the library and copy history.**
  A duration badge sits on a video's thumbnail, and its resolution and file
  size show in the tooltip; an attachment from before this feature picks
  these up in the background the first time it is shown. The main grid is
  unchanged.
- **Search can look inside an attached picture** (expert menu, off by
  default). Recognized text is never shown anywhere, never exported and
  never synchronized; it exists only to be searched, ranks below a match on
  the name or the prompt body, and a hit found only there carries a "found
  in image" badge. Turning the setting on adds an action that reads every
  existing picture once, in the background, with progress and a cancel
  button.
- **Keep an old version as its own entry.** Next to "Restore", the version
  history gets a second button that leaves the current entry untouched and
  adds the chosen version as a new entry instead, named after the version's
  date and selected right away.
- **Compare two entries side by side.** Pick two entries in the library, or
  choose "Compare with" on a tile, to see both next to each other with the
  differences highlighted, and take either side's text over into the other.
  The duplicate finder uses the same highlighting for its pairs.
- **A notice after an update.** Once a new version has started, a small message
  links to what changed; the full history stays reachable from the About box at
  any time.
- **Better contrast in every theme.** Field borders, small accent colored
  labels, the video editor's tabs and tinted tile text now clear the contrast
  requirement everywhere, and an automated check keeps it that way.
- **Floating buttons remember their screen.** A pill or folder whose saved
  screen is gone opens on the main screen, marked as parked, and returns on its
  own once that screen is back. An expert action brings every floating window
  back at once, and an optional cleanup can hide a long parked view, never
  delete it.
- **Undo right in the message.** Deleting a view, clearing the copy history or
  the clipboard inbox, deleting a tag, and batch deletes or tag changes now
  offer an undo button on the message that confirms them.
- **An archive.** Set a prompt aside without deleting it: archived prompts
  leave the library and the grid, stay findable by search, and travel with
  exports and backups. A toggle above the library brings them back into view.
- **Saved searches.** Keep a query with its tag filters and sort order as a
  chip above the search field and bring the whole search back with one click.
  Rename, delete or reorder them as you like.
- **The library as tiles.** A toggle next to the type filter switches the
  library to large preview tiles with the name below; clicking, double click,
  the context menu and multi-select behave exactly as in the list.
- **Export the usage statistics.** A button next to the statistics themselves
  saves them as CSV, TXT or JSON: per-entry copy counts and last-used dates,
  plus the daily activity chart. Only there while statistics are being
  recorded.
- **A storage section in the statistics/backups window.** Shows how much disk
  space the database, attachments, screenshots, recordings, discarded
  recordings, backups, temporary files, edit states and the error log each
  use, with a total at the top and a button to open a listed folder. Measured
  once and cached; a button re-measures. Independent of the usage-statistics
  switch, since storage use and usage tracking are two different things.
- **Most-used tools in the statistics.** A new rubric next to the entry
  statistics counts how often each tool gets used: screenshots, the
  screenshot editor, text recognition, screen recording, the video editor and
  its export, dictation, the color picker, chaining, rotating prompts,
  creating a grid folder, the duplicate finder and making a backup. A short
  list beside it names the tools nobody has touched yet. Counted only while
  usage statistics are on, stays on this computer, and is included in the
  statistics export.
- **Text preview for attachments.** A plain text, Markdown, JSON or CSV file
  attached to a prompt now previews its start right in the editor, below the
  file row, always as plain text and never rendered as markup. How many
  kilobytes it reads is adjustable in the expert menu, where the feature can
  also be switched off.
- **Copy formatted.** A second copy entry, in a tile's right-click menu and in
  the editor, offered only for a prompt whose text actually contains Markdown
  syntax. It puts the rendered version on the clipboard alongside the plain
  Markdown text, so Word, Outlook or a browser shows the formatting while a
  plain text field still gets exactly the Markdown source a normal copy
  would have given it.
- **An overview of everything that is kept.** The statistics and backups window
  gains one row per kind of kept data: the trash, the copy history, the
  clipboard inbox, recordings, discarded recordings, temporary exports and edit
  states. Each row shows how much it holds and how long it is kept, jumps
  straight to the setting that governs it, and can clean it up on the spot.
  Kept recordings gained a retention window of their own, off by default.
- **The video editor's limit on extra video sources is adjustable.** It used to
  be a fixed eight; it is now a number in the expert menu, from one to
  thirty-two, and still eight unless changed.
- **Search from the tray menu.** The tray quick copy menu gained a filter row
  that narrows its entries while you type, so a prompt can be found without
  opening the window at all.
- **A hint when a very similar prompt already exists.** Saving a new prompt
  that closely matches one you already have shows a quiet note about it
  instead of silently creating a near duplicate. It uses the same similarity
  value as the duplicate finder and can be switched off.
- **The editor keeps a draft.** Open text is saved every few seconds, and if
  the window or the app goes away before you save, the next start offers the
  draft back once. How often it saves is adjustable.
- **Fixed values for recurring placeholders.** Keep a list of names and values
  you use over and over; the fill-in dialog suggests them and never inserts one
  on its own. The list travels with the settings export.
- **Recent messages.** An error that shows as a toast and then vanishes is now
  also kept in a short list, reachable from the expert menu's data group: time,
  what happened and where, with the original text available for the ones the
  app translates. Copy the whole list or clear it from the same window. On by
  default, how many it keeps is adjustable, and switching it off keeps nothing.
- **Reordering view tabs.** A tab's right-click menu gets "move further left"
  and "move further right", greyed out at either end. Dragging works too: drop
  a tab in the middle of another to group them as before, drop it on the outer
  edge to move it there instead, with its own marker (a border for grouping, a
  bar between tabs for moving) so the two are never confused. A tab inside an
  open group moves within that group the same way. How wide that outer edge
  is, is adjustable in the expert menu.
- **Import from a folder.** Turns a folder of .md/.txt files into tiles, one
  per file, read only, nothing is ever moved or deleted. The file name becomes
  the name, a short first line becomes the note, each subfolder becomes a tag,
  and a file already using the rotation separator becomes a rotating tile. A
  preview lists every file with its tags, size and a checkbox, and grays out
  anything unreadable with the reason why. How deep it looks into subfolders
  and how many files it stops at are both adjustable in the expert menu.
- **Export recordings in a batch.** The recording folder's cleanup dialog gets
  a checkbox per file plus Select all/none; Export selected opens a small
  dialog for format, quality and a destination folder, then encodes every
  checked recording in one go and reports how many finished and how many
  failed. How many export at once is adjustable in the expert menu.
- **Quick profiles for the expert menu.** Three one-click profiles sit above
  the tabs: Quiet turns off everything that sounds, flashes or pops up a
  notice; Standard puts every setting back to its shipped default; All on
  switches on every feature, without touching anything destructive or
  anything that sends data out or costs money. A dialog shows how many
  settings would change and names the ten most important before anything
  happens. A profile writes once and is never remembered as a mode.
- **Backup content viewer.** Each snapshot's row can now open its own content:
  entry, view and tag counts, the most recent change inside it, the program
  version that made it, and whether its rows are sealed the normal way or
  fell back to plain storage. Below that sits every prompt the snapshot
  holds, with its own edit date, size and kind, searchable by name and
  sortable by any column, with a button to restore that single prompt
  straight from the list.
- **Chain overview strip.** While chaining, a strip above the grid lists the
  picked steps in order, with small marks for the placeholders each step
  needs and the reserved names it fills by itself; a step turns red when
  nothing earlier in the chain answers a placeholder it needs. Drag a row to
  reorder the chain right there. Switchable off on its own in the expert
  menu.
- **Third-party licenses in the About window.** A new section lists every
  bundled open-source component (name, version, license), generated at build
  time from the real dependency list so it can never fall out of date; each
  entry expands to its full license text, searchable once the list passes
  twenty entries. The same generated list replaces the hand-maintained
  content of THIRD-PARTY-NOTICES.md.
- **Data folder lock.** Pointing the data folder at a synced folder (OneDrive,
  Dropbox, Google Drive, iCloud Drive, Nextcloud) now shows a one-time warning
  about the risk of two computers writing to the same database at once. A
  small lock file next to the database tracks which computer is using it: a
  second computer that finds another machine's fresh lock opens everything
  read-only instead and says so with a permanent banner, while a lock nobody
  has refreshed for a while is taken over automatically instead of leaving
  anyone locked out. Switchable off in the expert menu, on by default.
- **Key overlay for recordings** (expert menu, off by default). Records
  which keys are pressed anywhere on the screen while a recording runs, as
  key identifiers and timestamps only, never any text. By default every key
  is shown, including typed text, and a focused password field is skipped
  for as long as it stays focused and shows up as a gap instead, though this
  is not guaranteed since it relies on Windows reporting the field
  correctly. A "Shortcuts only" switch restricts capture to combinations
  with Ctrl, Alt or Win plus Enter, Tab, Esc, arrows and F keys, for a
  recording with confidential input. The video editor's new "Keys" overlay
  turns this into a picture: four keyboard-layout presets (keys pressed,
  half keyboard, full keyboard, game keys) plus a free selection, sharing
  the same range, duplicate, snap and export as every other overlay. How
  long a released key stays visible is adjustable in the expert menu.
- **Camera recording** (expert menu, off by default). A "Record camera"
  entry on the record button's right-click menu opens its own small window
  with a live preview and its own Start/Pause/Stop, audio source, resolution
  and target folder. The window itself, or its status dot in the recording
  bar, is always visible while the camera runs — no silent camera. It
  records to its own file, never burned into a screen recording, so either
  can be repositioned later. Running alongside a screen recording, both
  share the same start point, and the video editor opens the screen
  recording with the camera laid on top as an aligned overlay once both are
  finished; with no screen recording running, the camera take is its own
  zero point and opens on its own. Default camera, resolution and frame
  rate are all adjustable in the expert menu.
- **JSON export and import for prompts**, alongside CSV and TXT: one Export
  button plus a format dropdown replaces the three separate buttons. JSON is
  the only one of the three that can carry a rotating prompt's full entry
  list, tags, notes, a hotkey and a colour without joining them into a
  single field. Import decides CSV or JSON from the file's own content, never
  its extension. Images stay out of all three, same as before.
- **The recording control bar can be dragged to move it**, including onto
  another monitor; the moment you drag it, it stops following the recorded
  window for the rest of that recording, and follows again on the next one.
  Its two audio buttons also mute system sound or the microphone instantly
  while recording, without dropping the track: a muted source keeps
  recording silence and can be unmuted again in the same recording.
- **The recording folder's row in the expert menu now shows how many videos
  are in it and how much space they use**, with a "Clean up..." button that
  lists files older than 7, 30 or 90 days and the space it would free, for
  confirmation before anything is deleted.
- **The video editor's export destination is now an editable field**: type a
  path directly, use "Save as..." for a full save dialog, or "Export to..."
  to keep the file name and change only the folder; switching the format
  only rewrites the extension. "Export and copy" skips naming a destination
  entirely, exporting into a self-cleaning temporary copy (removed after an
  hour, and again on the next launch, both adjustable) and putting the
  finished file straight on the clipboard.
- **The video editor's export destination now has its own file name field**,
  separate from the folder path, with the extension shown beside it. A name
  with a path separator or a reserved Windows character is rejected on the
  spot, with Export disabled until it is fixed. An existing file is never
  replaced: the export takes the next free name.

### Improved

- **Demo mode speaks your language.** The sample entries, tags, tabs, folder
  and attached files are now shown in the language the app is set to, in all
  eleven interface languages. Starting demo mode again after a language
  switch uses the new language.

- Text on tiles, folders, view tabs, messages and floating buttons only turns
  black on genuinely light colors now; a medium color that used to flip to black
  text keeps white text instead. Both the automatic choice and the brightness
  cutoff are expert-menu settings.
- The chain-start button on a tile no longer draws its own box; it keeps its
  size and picks up a drop shadow instead, so it stays out of the way on a
  plain tile and still reads clearly over a photo or screenshot tile.
- The chain overview's steps now flow left to right and wrap instead of
  stacking one per line, so a long chain takes far less vertical space; dragging
  a step to reorder it still works across a wrapped line.
- The offer to adopt an installed copy's data into a portable run now names its
  size and prompt count before you decide, remembers your answer for that exact
  location so it does not ask again every launch (a "do not ask again" box lets
  you decline the memory instead), and shows where your prompts ended up, with a
  button to open that folder, once you accept.
- A long library list opens noticeably quicker. Each row draws three small icons,
  and their markup was rebuilt from scratch for every single row; it is now prepared
  once and copied per row. With 500 prompts the list is built in 45 ms instead of
  71 ms, and the rows look exactly the same.
- Resizing a floating folder by its edge is smoother. The folder was completely
  redrawn — every member preview included — for each report the mouse sent, which
  is several times per screen refresh. It now redraws at most once per frame while
  you drag, and once more when you let go, so the size you release is always the
  size you keep.
- A library full of images redraws far faster. The window no longer receives every
  stored picture each time the grid is rebuilt — it is told which prompts have one
  and asks for the ones it is about to show, keeping the last 80 in memory. With
  500 prompts, 60 of them carrying a screenshot, one redraw moved 24 MB before and
  moves 0.3 MB now, and takes 26 ms instead of 67 ms. A picture that is still on
  its way leaves the tile on its normal name-and-colour face and swaps itself in
  when it arrives, so nothing flickers.
- Saving is far quicker in a large library. Renaming one prompt, dragging one tile
  or changing one tag used to rewrite and re-encrypt every stored prompt; now only
  the prompts that actually changed are written, and moving prompts around updates
  nothing but their position. With 500 prompts, 60 of them carrying a screenshot,
  a single rename takes 36 ms instead of 1900 ms.
- Copying no longer writes to the disk on every single copy. Which entry a rotating
  prompt serves next, how often a prompt has been used and the date of its last use
  were saved the instant any of them changed — with auto-advance switched on, that
  meant a full save on every Ctrl+V. They are now collected and written once the
  copying has been quiet for a second: 100 pastes in quick succession cost 100 disk
  writes before and one or two now. Closing the app, logging off Windows, a backup,
  an export, an import, a reset and moving the data folder all write immediately, so
  nothing is left behind; only an outright kill of the program (task manager, a power
  cut) can cost the last second of counters — never a prompt, never any text, and
  never a setting you just changed.
- Saving a prompt can no longer clear its image by accident: the editor only sends
  a picture along when you actually changed it, and the only thing that removes one
  is the remove button.
- The color picker's magnifier now starts at 4× instead of 8×, so you see more of
  the area around the pixel you are aiming at; the wheel still changes it and the
  value you stop on is remembered.
- The global-hotkey field in the settings is wider, so a long combination is no
  longer clipped by the two buttons beside it.
- The help window's scrollbar sits at the window edge instead of floating inside
  it, and the backup password warning fits on one line instead of three.
- Deleting several prompts at once confirms on the button itself: the first press
  turns it red and names the count, the second deletes and reports what moved to
  the trash. No dialog jumps under the pointer any more.
- The help page is split into chapters with a row of jump buttons, instead of one
  long list of twenty-five topics.
- The colour-pick confirmation keeps its three formats on one line and marks the
  one already on the clipboard, instead of repeating it in the message and
  wrapping the rest onto a second row.
- The expert menu's "only changed" filter is a button beside the search field
  rather than a full row of its own.
- Usage statistics fold away as one block in the backup window, its own switch
  included.
- A folder no longer stamps a number on its preview; the member count is in the
  tooltip, where the rest of its description already was.
- Tag chips, the update button's width and the prompt count beside the library
  title were all tidied up.
- Backup retention is now a quick pick — Light (5), Normal (10), Heavy (20) or
  Advanced — instead of a switch plus a number field. Advanced still opens the
  same day/week/month/year settings as before.
- The editor's tags, hotkey and version history now share one collapsible
  "Advanced options" section instead of three separate controls.
- The grid can now be filtered by tag too, from a new button in the header —
  tiles that do not match dim instead of disappearing, so the layout never
  shifts underneath you.
- The backup list now shows what is actually in each snapshot: click the
  prompt count to reveal the names.
- The library can now be sorted by "last edited", alongside the existing name/
  most-used/recently-used options.
- Import now shows a preview — what's new and what's already in your library —
  before anything is actually imported.
- Floating pills and folders now have a "Hide until restart" option in their
  right-click menu, alongside the existing permanent Remove/Dissolve.
- A PDF attachment can now show a page other than the first — a page number
  field appears next to it in the editor.
- The changelog in the update dialog is laid out instead of dumped: headings,
  bullets, tables and highlighted terms, in a box that takes the height of the
  release notes and scrolls only once they outgrow the window.
- Dragging one button, tab or tile onto another now shows what would happen: the
  target dims behind a folder mark in the accent colour, the same gesture in all
  three places and readable on any tile colour and in every theme.
- Tags are findable and look like the rest of the app: the editor's tag field has a
  handle that opens the list of tags you already have — as a proper dropdown, with
  a line inside it when there is nothing to offer instead of an error message — and
  the library's tag filter is a labelled row of chips built like the type filter
  above it, with clearing and managing set apart as actions.
- The copy history reads like the library — same rows, same tags, a type chip
  where it is not plain text, and the timestamp as a quiet second line.
- Dropdowns, colour popovers and context menus flip above their anchor when there
  is no room below, and never open past the edge of the window.
- The favourites section of the tray menu is on out of the box.
- The privacy switch that keeps the window out of screenshots is now phrased from
  your side and named **Allow screenshots of Prompt Saver**. It ships off, so the
  window still stays out of screen captures by default — but anyone whose screenshots
  come out empty now finds the reason under privacy in the expert menu. An
  installation that had the old hiding turned off keeps its screenshots.
- The editor's font and size dropdowns moved up to sit right under the name row,
  with the other row-level settings, instead of down by the character/token count.
- The quick grid-size control in the header can now be set to appear only while
  you point at the header, for a tidier toolbar (expert menu, off by default).
- A floating button or folder left completely off-screen (say, after unplugging
  a second monitor) is dropped automatically on the next launch; one merely
  hanging over an edge is left alone.
- Dragging a tile to a new spot now shows a brief "Tile moved" toast with an
  Undo button, so a mis-drop can be reversed with one click instead of
  dragging it back by hand.
- The theme editor has a **Revert changes** button that undoes everything
  done since it was opened without closing it — Cancel still discards
  everything and closes the editor.
- Custom themes can be **exported to a file and imported back** — handy for
  sharing a theme or carrying it to another install. An imported theme is
  validated the same way a hand-typed one would be.
- Version history now highlights **what actually changed** between two
  versions — words added or removed — instead of just showing the raw old
  text for you to compare by eye.
- A backup is now either whole or absent: the snapshot is written under a
  temporary name and only then renamed, and a failed one leaves nothing behind.
- "Duplicate" on a prompt's context menu now opens the same dialog as creating a
  new one, prefilled with everything from the original — so you can change
  something before it exists, or cancel and leave nothing behind, instead of a
  silent copy landing in the grid.
- The variable fill-in dialog gets a "Clear all" button, so starting over on a
  prompt with several placeholders is one click instead of one per field.
- Saving a prompt with an empty `{#{ }#}` placeholder now warns first — it would
  otherwise sit in the copied text exactly like that, since it never becomes an
  actual variable.
- The backups window can open its snapshot folder in Explorer directly.
- A long export, recording conversion, ffmpeg download, backup or import now
  fills the Windows taskbar icon with its progress, so it stays visible while
  the app sits in the background. Its progress bar also shows a plain "about X
  left" once enough has run for the guess to be worth showing. Both switchable
  off in the expert menu.
- A video export or recording conversion that fails now names the actual
  reason in plain language — file incomplete, no space left, no access, and
  so on — instead of a raw ffmpeg log line. The exact log line stays
  reachable underneath in a closed "Technical details" row with a copy
  button; a cancel was never treated as a failure to begin with.
- Closing to the tray for the very first time now explains what just happened —
  right-click the tray icon to copy a prompt without reopening the window —
  before the window disappears. Every later close is instant again.
- The editor's automatic-placeholder chips call out `clipboard`: it is the only
  one that reads live data instead of a fixed value, so it now stands apart from
  the rest.
- A feature that ships off now gets a small dot next to its switch instead of a
  "(Default off)" suffix crowding the label text.
- Every visual transition now follows the animation-speed expert setting,
  including the keyboard-focus ring's fade-out and the privacy blur effects,
  which previously always ran at a fixed speed.
- A view tab's right-click menu gets a **Duplicate** entry: a new, empty view
  with the same name, color and grid size, ready to build a variant of it from
  scratch.
- The backup password field now says plainly, right where you set it, that a
  forgotten password can never be recovered — previously that only showed up
  in a toast after saving.
- The theme editor's per-color reset button is a crisp icon now instead of a
  small, pixelated "×" character.
- The expert menu's search box now also matches a word that only appears in a
  setting's tooltip, not its label — three settings (hide scrollbars, grid
  lines, paused toolbar icons) get their tooltip shown at all again in the
  process; it had quietly stopped reaching them.
- Restoring a backup now takes the same safety backup first that resetting or
  deleting everything already did, if that expert option is on.
- Creating a prompt by hand now gets the same near-duplicate warning that
  importing already gave.
- The variable fill-in dialog now says when a field's value is the one from
  last time, for both a free-text field and a choice variable (which had no
  checkbox to hint at it at all).
- Every copy-history row gets an edit icon, same as the library, so you can
  open a prompt from its history without going to find it in the grid or
  library first.
- The editor now offers your 3 most-used tags right below the tag field —
  one click to add instead of typing them out every time.
- The tag manager gets a **Colorize** button: one click gives every tag
  without a color one from the palette, round-robin. Tags you've already
  colored yourself are left alone.
- The library preview's line count (2, until now fixed) is an expert value,
  adjustable from 1 to 4.
- A grid click swallowed by the copy cooldown now shakes the tile briefly
  instead of doing nothing at all — the cooldown itself was already
  adjustable (down to 0 to turn it off) in the expert menu.
- The "Backup created" toast now shows for automatic backups too, not just
  the manual button.
- New expert option: a colored border around the grid itself, in the active
  view's color (off by default).
- The library search now supports `-word` to exclude prompts containing it,
  combinable with a normal search term.
- The private-note field in the editor now has a quieter fill than the
  prompt text box, so the two don't read as the same kind of field.
- **Saving is noticeably lighter.** Every write used to re-check the whole
  database layout first; that now happens once when the app starts. A copy
  went through it twice per click and no longer does.
- **Text recognition no longer decodes the same image three times** on its way
  through resizing — one pass now, which shows on large screenshots.
- **Taking a screenshot no longer blocks** while the image is compressed and
  written; the work moved off the thread that keeps the app responsive.
- Typing in the expert menu's search box no longer rebuilds all five tabs on
  every keystroke, and the quick launcher prepares its search index once
  instead of on every keypress.
- **The background chores share one timer instead of one each.** The automatic
  backup, the update check, the language-pack watch and the temp-file cleanup
  used to keep a thread apiece, each asleep almost all the time; they now take
  turns on a single one, which also means closing the app no longer has to wait
  on a chore that was parked for the next few hours. The temp-file cleanup runs
  every few hours now instead of only at startup, so an instance left running
  for days keeps tidying up after itself.
- **Keyboard navigation reaches every dialog and popover now** — Tab no
  longer leaks past whichever one is open into the dimmed page behind it, and
  closing the prompt editor, a confirmation dialog or the settings panel
  returns keyboard focus to whatever you had focused before you opened it.
- Six previously mouse-only row icons — favorite, edit, delete and
  add-to-layout, in both the library and the copy history — are now reachable
  and operable by keyboard.
- Eight error messages (failed save, import, export, dictation, update
  download or turning on the color picker) now say what you can do about it,
  not just that it failed.
- A handful of low-contrast colors (muted text in three themes, the dark
  theme's red) were nudged to meet accessibility contrast guidelines; nothing
  else about those themes changed.
- Opening the statistics dialog and switching prompt syncing on no longer make
  the rest of the app wait: both now do their reading and writing without
  holding everything else up.
- Importing a file, a theme or a synced Markdown file reads it under a size
  limit in one pass, so an unexpectedly huge file is refused instead of being
  pulled into memory whole.
- **An expert-menu category's reset button now also clears a setting hidden
  behind a switched-off toggle within that category**, instead of leaving it
  stuck at its changed value where it could never be reached again without
  first turning the toggle back on.
- Pill folders, grid folders and view-tab groups now read as the same idea
  everywhere: one wording and one folder icon for all three, instead of each
  kind describing itself a little differently.
- The chain button's tooltip now explains how chaining differs from rotation,
  so the two are easier to tell apart at a glance instead of only in the help.
- The dark shadow behind white text on a coloured tile, folder or floating
  button can now be switched off or dialed to a different strength in the
  expert menu, next to the black/white text switch.
- **Saving a prompt with a large attachment no longer freezes the other
  windows.** Copying the file, reading its properties and recognizing text in
  a new picture now happen before the library is locked, so copying and
  searching keep working while a big save runs.
- **Camera recording no longer stalls the window every second.** Saving each
  recorded chunk moved off the main thread, and the chunks are still written
  strictly in order.
- **The Markdown folder check no longer copies the whole library every few
  seconds.** The poll first looks at file sizes and dates only and copies the
  prompts and settings just when a file actually changed.
- **Saving with Markdown folder sync on no longer rereads every file.** The
  app remembers what it last wrote to each file and skips the ones that are
  still exactly that; an edit made outside the app is still noticed.
- **Opening the trash no longer loads every trashed picture.** The list asks
  only for names, a short text preview and dates, and runs off the main
  thread, so a large trash opens at once.
- **The batch export dialog reads the lengths of the selected recordings in
  parallel.** Choosing many recordings opens the dialog faster and no longer
  ties up a background worker one file at a time.
- **Searching the library with picture text search on draws the list once per
  keystroke pause instead of twice.**
- **Finding duplicates copies far less data while the library is locked.**
  Only names, texts, variants and tags are copied for the scan, not the stored
  pictures.

### Fixed

- **Uninstalling an all-users install leaves no entry behind.** The app no
  longer stays listed under installed apps, and its Start menu shortcut is
  removed, after uninstalling an install made for all users.
- **Full expert reset keeps the vault, the backup password and the
  certificate.** It used to wipe the vault's key data, which left every
  confidential entry unreadable for good, and the backup password. It now also
  stops a running remote receiver and removes the promptsaver:// link
  registration when the reset turns them off, and so does the settings reset.
- **Importing a backup that brings another vault locks the vault.** The old
  unlocked session no longer seals new confidential entries against a key that
  does not match the imported vault.
- **A failed restore from the trash no longer drops the remote access password
  or certificate key** from the running settings.
- **A command that was refused no longer changes the running app.** A delete,
  add or setting change sent through the command line or remote access while
  the data folder is read-only or locked is now turned away before it touches
  anything, and a write that fails is undone in memory too.
- **Changing the remote access port while it runs takes effect at once.**
  If the new port cannot be used, the old one comes back.
- **Markdown folder sync no longer loses or damages data.** A file with the id
  of a confidential entry can no longer overwrite it, a file whose prompt sits
  in the trash no longer comes back as a second prompt, and the cleanup of
  leftover files only removes files the app has known before, so a folder that
  already holds another machine's files, or a file you made or renamed by hand,
  is left alone. An edit the sync has noticed but not read yet is kept as a
  conflict copy instead of being overwritten. Notes with several lines and a
  text that ends with a new line now come back unchanged, and files saved with a
  byte order mark or as UTF-16 are read.
- **Markdown folder sync follows changes properly.** Open windows and the tray
  refresh when the sync imports something, a grid folder takes over the cell
  and colour from the other machine instead of both machines rewriting its files
  forever, choosing the same folder under another spelling no longer deletes it,
  and a paused sync resumes as soon as you switch to a working folder.
- **Restoring or importing entries takes the matching vault key along.**
  Confidential entries of a backup open with the backup's own vault password,
  and a backup without a vault keeps yours. If confidential entries in the
  trash can no longer be opened because of that, the import says so.
- **An old snapshot no longer brings back old remote access settings.**
  Restoring a local snapshot keeps this computer's remote access switches,
  password, allowed addresses, update choices and clipboard watcher.
- **A leftover emergency copy can no longer overwrite what you just saved.**
  Once a save reaches the database, the old fallback file of an earlier failed
  save is removed.
- **Confidential entries stay readable after a settings reset or a settings
  import.** Resetting all settings, importing settings from a file or
  restoring only the settings of a backup now keep the vault key material of
  this computer, so the same password still opens the entries that were kept.
- **Importing a backup file is all or nothing.** The entries and the settings
  are written together, so a failed save can no longer leave one half
  imported while an error is shown. After a failed import the library and the
  settings are exactly as they were.
- **Importing settings from a file is saved before it applies.** If the save
  fails the import is undone and reported, and after a successful import the
  theme, tray, screenshot protection, pill windows and hotkeys follow the new
  settings at once.
- **Every dropdown now opens the same rounded, themed list.** The library sort,
  the export and save-as format pickers, the statistics export, the screenshot
  editor's text options, the video editor's export and overlay dropdowns and the
  camera window's microphone and resolution pickers used to open the square
  Windows list. They share one popup now, also usable from the keyboard (Enter,
  Space or Alt+Down opens it, arrows move, Escape closes it), and their closed
  look (corner radius, arrow spacing, border) matches everywhere.
- **The video export panel no longer has two "Save as" buttons.** The button
  that picked a file before exporting is gone (the name field and "Export to…"
  cover it), and the one shown after an export now reads "Move to…" with a
  tooltip saying it moves the finished video to another place.
- **The 720p, 1080p and larger resolution presets can be chosen again.** A
  recording smaller than the preset (a small region, say) used to fall back to
  "keep original" whatever you clicked. A larger preset now enlarges the video,
  says so under the buttons and exports with a sharper scaler; a preset equal to
  the source height keeps the original.
- **The video editor no longer hangs when a second recording arrives during
  the restore question.** Opening another recording while the question about an
  unfinished edit was still open left the first load waiting forever. The old
  question is now closed and released, and only an explicit "start fresh" answer
  discards the saved edit.
- **The video export no longer fails after a tempo change.** A speed such as
  1.3x made the output length a fraction of a millisecond, which the export
  refused with an "invalid type" error. The length is now rounded to whole
  milliseconds before it is sent.
- Allow screenshots of the app now works for every capture program, including
  the app's own. The recording bar and the outline around a recorded region stay
  out of recordings whatever this switch says.
- The outline around a region recording now matches the drawn region on
  monitors with different scaling.
- **A start that could not open the database could cost every prompt.** If the
  emergency copy of the prompts was missing at that moment — the state an
  earlier failed save can leave behind — the app came up empty and the next
  save wrote that emptiness over the real data. Such a start now refuses to
  save prompts at all, so everything survives until the database can be read
  again.
- **Moving the data folder could lose the newest changes.** The current state
  was written back to the old folder first, but a failed write (a folder gone
  read-only, a changed permission) was ignored: the older files were carried
  across, the move reported success, and the cleanup afterwards offered to
  delete the only folder that still had the newer state. A failed write now
  cancels the move, and the old folder is only offered for deletion once the
  move has completely succeeded.
- **Taking over an installed setup into the portable app could leave it
  confused** when the copy failed halfway through (a full disk, a file in
  use): the app carried on with the setup it had just replaced, and the next
  save could delete the prompts that had already arrived. It now reloads what
  really landed before reporting the problem.
- **Restoring only the prompts from a backup made with version 2.6.3 or older
  failed completely**, including the prompts that would have come back fine,
  because such a backup's version history predates a column added in 2.7.0.
  Those backups restore again, with the missing detail left empty. Restoring
  a whole backup was never affected.
- Copying with the automatic rotation running leaked a small amount of memory
  each time Windows refused the clipboard.
- **Opening the version history of a long prompt could freeze the window.**
  Every version's word-by-word comparison was computed up front even though
  the rows start collapsed; it is now done when you actually open a row.
- **The screenshot editor silently erased old drawings.** Once you passed the
  undo limit, the oldest step vanished from the picture — a crop or a
  pixelated area included. Everything you draw now stays; only how far back
  undo can walk is limited.
- **The screenshot editor's language, and two more strings, were still
  English** in German and Dutch.
- **Switching off the video controls left the hover bar on the floating
  buttons**, and those buttons ignored the volume, mute, loop and autoplay
  settings entirely — they now follow the same settings as the grid.
- **A failed view switch, edit or duplicate did nothing at all.** The click
  now reports what went wrong instead of quietly leaving the old screen up.
- **A language pack could be installed twice.** The download row stayed
  clickable for a moment after the install had already started.
- **The reminder about outdated recognition packs came back on every start**
  instead of waiting for its interval.
- **Moving the window switched an active prompt chain off.** Dragging the
  window by its own drag region briefly blurs and refocuses it, and that was
  being treated as a real switch away to another app. A chain now only ends
  from its own toggle, cancelling, or finishing.
- **The floating buttons' menus flashed English labels** for a moment in every
  other language, and one of them spelled a word the app is free to rename.
- **The volume slider on a video could be cut off** at the top edge of the
  window; it now opens downwards when there is no room above.
- Clicking a floating button twice in quick succession could copy the same
  prompt twice while the first copy was still in flight.
- A handful of leftovers survived a crash: recognized-text images, the
  language-pack helper file, and the WebView2 setup download were never
  cleaned up. The startup sweep now covers them.
- **A handful of backend error messages always showed in English**, no
  matter the app language (deleting the last view, an empty theme/group/tag
  name, an oversized or corrupt update download, a backup over 512 MB, and a
  few others). They're translated now.
- **The PDF-preview library left every version's copy behind in the temp
  folder.** An update now removes the previous version's cached copy instead
  of accumulating one file per historical version forever.
- **Two high-severity denial-of-service dependencies were pulled in
  transitively** (a quadratic-runtime and a memory-exhaustion issue in an XML
  parser used by an unrelated OS-integration dependency). Routine dependency
  updates resolve both.
- **The bar-hide buttons sat far from the row they collapse.** They now sit
  directly left of Minimize (top) and Save (bottom), lined up with each
  other; Save is now exactly as wide as the Minimize–Maximize–Close row
  above it.
- **"Reset settings" and "Delete all data" wrapped to a second line** in
  several languages, making the button row taller than the rest of the
  dialog. Both buttons now show a single short word, with the full
  description as a tooltip.
- **The theme editor's export and import icons were swapped** — export drew
  a download arrow, import an upload arrow.
- **The installer appeared to close by itself when updating over an existing
  install.** Upgrading or reinstalling silently removes the previous version
  first; the window used to hide itself for as long as that took instead of
  staying visible with a status line. A second issue in the same step could
  let the old version's files still be mid-delete while the new install
  started writing into the same folder.
- **Every expert-menu tooltip carried a line from the last tile you hovered.**
  The tile tooltip's "next entry" line was left in place when the box switched to
  a plain setting hint, so it stayed glued under unrelated tooltips.
- **The editor left an empty band above the colors while rotation was on**, and
  with the Markdown preview switched on the rendered pane kept showing entry 1
  there, which read as a stray field of its own.
- **Picking a sync folder appeared to do nothing.** The folder stayed empty until
  the next unrelated save happened to flush it; the files are now written the
  moment you pick it. The row also shows the real default path instead of the
  word "default", and its reset arrow sits level with the Change button.
- **The trash opened invisibly.** Opening it from the expert menu drew it behind
  the still-open settings panel, so the dialog appeared not to open at all.
- **Prompt data could be written to disk unencrypted without any sign of it.**
  If the database write failed, the recovery file it fell back to was plain,
  readable JSON while every database row is encrypted; the same applied to the
  prompt-name list cached beside each backup. Both are now encrypted like the
  rest, and a failure of the Windows encryption service is written to the log
  instead of silently downgrading storage.
- **Typing in the library search no longer stutters** on large libraries — the
  result list is rebuilt once you pause, not on every keystroke.
- **Window titles in the Alt-Tab switcher were always English.** The floating
  pill, folder, variables and quick-launcher windows now carry the app name.
- Importing a file far too large to be a prompt export (a video, a disk image)
  now fails with a clear message instead of trying to load all of it into memory.
- **The tag manager was unreachable until you'd already tagged something.**
  The library's tag bar — the only place "Manage tags" lives — disappeared
  entirely with nothing yet to filter by, so there was no way to discover
  tags exist at all before creating the first one by hand in the editor. The
  button now stays put regardless.
- **Dissolving a grid folder could leave a member invisible or bump another
  folder off its spot.** The cell handed to each returning member never
  accounted for a second folder sharing the same view, since a folder's own
  cell lives outside the normal layout — it could land exactly there.
- **Exporting to CSV could hand Excel or LibreOffice a live formula.** A prompt
  name, text or note starting with `=`, `+`, `-`, `@` or a tab is read as a
  formula the moment the file is reopened there, even though the value was
  correctly CSV-quoted — quoting only protects the CSV syntax, not what the
  spreadsheet does with the cell afterwards. Such a value is now prefixed with
  a single quote on export (the standard fix) and the prefix is removed again
  on the next import into Prompt Saver, so nothing changes for data that never
  leaves this app.
- **A save could look successful and then quietly revert after a restart.** If
  the database write failed (locked or full disk) the change fell back to a
  recovery file that was only ever read back on a completely empty database —
  a normal restart always preferred the older database state instead, silently
  undoing the change. The recovery file is now always checked and retried on
  the next launch. A related case — replacing an attachment could delete the
  old file even though the change that replaced it failed to save, breaking
  the attachment for good — is fixed alongside it.
- **Double-clicking Save (or pressing Enter twice) while saving a prompt could
  create a duplicate.** The Save button had no protection against a second
  click while the first save was still in flight.
- **Moving prompts to another view could make them vanish if the target view
  was full**, or land a prompt on a grid folder's own cell and unexpectedly
  bump the folder aside on the next render. Moving now checks the target has
  room for everything first and shows a clear "grid is full" message instead.
- **The Markdown sync folder could delete your own documents.** If you pointed it
  at a folder that already held Markdown files of your own, they were removed the
  moment sync was switched on. It now only ever touches files it wrote itself, and
  leaves any other Markdown — including notes with their own frontmatter — alone,
  in both directions.
- **A prompt edited through the sync folder gained a blank line** at the end every
  time it was saved.
- **Switching prompt sync on kept going even when its safety backup failed.** It
  now stops, leaves the feature off and says why.
- **Two backups made in the same second overwrote each other**, so one of the two
  recovery points was silently lost.
- **New, restored and folder-imported prompts could land on a grid folder's own
  cell** and stay invisible until the folder was moved.
- **Moving the data folder left a pending change behind.** If a database write had
  failed earlier, the fallback file holding that change was not copied to the new
  location and was then deleted with the old folder.
- **The colour picker's history never appeared.** The last five picked colours now
  live in a strip of their own above the hint line, where they can actually be
  clicked — inside the loupe they moved away with the cursor.
- **The editor's text box could be drawn over the colour swatches** once the
  variable helper above it was expanded.
- **An unreadable recovery file could silently empty your whole library or
  settings** the next time anything saved. It is now left alone and logged
  instead of being treated as "nothing to restore."
- **Undo/restore could report success even when its own safety snapshot
  failed to save**, and several other actions (import, restoring a version,
  favoriting, batch edits, renaming or deleting a tag, recoloring, seeding the
  example prompts) could report success while the write to disk had actually
  failed — each now rolls back cleanly and reports the real error.
- **The prompt-sync folder could save a half-written file** if the app lost
  power mid-write; it now writes through a temp file and an atomic rename.
- **A hand-edited or imported folder position at the far edge of the grid
  could hang the app on startup.** Positions are now kept in range and the
  placement loop always terminates.
- **Emptying the trash could delete a prompt's version history even when the
  trash itself failed to save**, so the prompt came back with no history.
- **Importing a backup from another PC could silently clear your backup
  password**, leaving the next export unprotected with no warning.
- **Pill-button folders never worked on a fresh install** due to an inverted
  setting check.
- **The folder window stayed visible, and could appear in a screenshot,**
  while screen-capture-safe mode hid every other window.
- **Copying a rotating prompt from a tile, a folder member, or the library
  skipped rotation and always copied the first entry**; the placeholder-fill
  dialog could also fail to appear when only a later entry had a placeholder.
  Rotation also no longer advances on copying an image or file entry.
- The usage-statistics panel grew an extra header every time it was toggled.
- A number field could not be set to `0` (it fell back to the default),
  blocking a GFS backup tier from being turned off entirely.
- The rotation badge ignored the rotating-prompts toggle and kept polling in
  the background even with the feature switched off.
- Cached video thumbnails stopped starting or pausing with scroll after the
  library had been re-rendered once.
- A slow response for one prompt's history or rotation state could land in a
  different prompt's editor if you switched prompts quickly.
- The editor could get stuck showing a rotating-prompt variant list even
  after the feature was switched off.
- A pill hidden with "hide for now" could reappear on its own after being
  unpinned and re-pinned.
- Grid folders with the same name in two different views could merge into
  one, and moving the data folder left backup history behind in the old
  location.
- Leftover snip-preview images and update-staging folders from a crash were
  never cleaned up.
- A failed schema migration on an older database was silently ignored
  instead of being reported.
- Placing a prompt in a second view could duplicate it there once multi-view
  was switched off.
- The folder window did not pick up a changed grid-column count while
  already open.
- Enter inside a choice-style variable field could confirm the dialog with a
  stale value, or do nothing at all on a single-option choice.
- The colour-history strip's copy failures were silent, and a failed colour
  pick reused an unrelated error message.
- Some builds under a different product name still showed English fallback
  text before translations loaded, and Turkish mixed the loanword and the
  localized word for the same thing in the same build.
- A `$` in a rebranded product's text could corrupt the generated identity
  files, and rebrand-generated files could show as changed on a fresh
  checkout due to line-ending drift.
- The icon-only-button check only covered the five static windows, missing
  every dynamically created button; it now covers those too.
- A closing dialog could re-enable the wrong dialog's Cancel button if a
  second one opened while the first was still closing.
- Dragging a colour onto a grid folder redrew the entire grid on every
  pointer move.
- The help text had the screen-capture-protection default backwards, and
  documented the wrong syntax for self-filling placeholders.
- **The version-history diff never showed a change if only a rotating
  prompt's later entries changed** — only entry 1 was ever compared, and
  restoring a version left the editor's still-open rotation entries in place,
  so the next save quietly overwrote what had just been restored. Prompts
  with no visible text (images, files, colours) never showed version history
  at all, even though a name change still creates a snapshot for them.
- **The copy-text-size and screenshot/sync-folder path rows always showed
  as "changed"**, permanently offering an undo and matching the "only
  changed" filter, because their stored value was compared against a
  default of a different type.
- **The backup-count and retention rows in the backup dialog had no undo
  arrow at all**, unlike every other expert setting, and the color palette's
  reset button showed even when every colour was still the default.
- **Two expert-menu dropdowns paired side by side could look inconsistent**
  when one's label wrapped to a second line and the other's did not — the
  label now always sits above the field in that two-column layout instead.
- **The "Create backup" button was noticeably narrower than "Open in
  Explorer" right below it** in the backup dialog's left column; both now
  match the column width.
- **The expanded backup list showed every prompt name run together in one
  comma-separated line.** It is now a real list, one row per prompt with its
  type and character count; an older backup's cache still made before this
  existed upgrades the next time it is listed.
- **The floating pill's and pill folder's "Edit prompt" button showed no text
  at all** until the window's language finished loading, a brief blank button
  on every open.
- The selected size in a floating pill's size picker always showed white text,
  even on themes where that made it hard to read; it now follows the theme's
  own text colour like the rest of the menu.
- **Placing a prompt on the grid could silently do nothing** for a prompt that
  was already a member of a grid folder — the library's "add to layout" icon
  still offered it, since a folder member has no layout entry of its own.
- **Turning off the Favorites feature left its filter reachable in two
  places**: the copy history's star filter and the library's batch
  favorite/unfavorite buttons stayed visible and working.
- **Color filter and picker swatches without a written label had no
  accessible name** for keyboard and screen-reader users.
- Editing an expert-menu path setting (screenshot folder, data folder, sync
  folder) now shows its explanatory tooltip like every other expert row.
- Backup statistics ("average copies" and "copies per day") and the "prompts
  imported" count now use the app's number formatting instead of always the
  English decimal point.
- The password-reveal eye icon in the backup dialog now has a hover tooltip,
  not just a name for screen readers.
- Replacing a prompt's media or attached file while the editor was being
  closed at the same time could throw an error partway through instead of
  quietly doing nothing.
- A handful of remaining error toasts (among them a failed backup) still
  showed the raw English message instead of the translated one.
- **Escape** did not close the help panel, unlike every other panel in the app.
- The journal's favorites-only filter chip disappeared when it was the only
  active filter, making it impossible to turn back off.
- The journal did not redraw its content when the app language changed while
  it was open.
- Restoring a prompt version could overwrite a different prompt's editor if
  you switched away before the restore finished loading.
- Changing a PDF's active page did not count as an unsaved change, so closing
  the editor right after skipped the usual "discard changes?" prompt.
- A pill unpinning itself (its own "Remove" button) left the tile's context
  menu unaware: it kept offering "Remove floating button" for a prompt that no
  longer had one, and clicking it brought the pill back instead.
- Copying a prompt from a floating pill, the folder window or the tray did not
  update the library's "most used"/"recently used" sort in the main window —
  it stayed frozen at whatever it last saw until the next full reload.
- A fast double click on the same rotating prompt while picking a chain could
  leave the tile visibly selected with its text silently missing from the
  chain, instead of either staying picked or unpicking cleanly.
- A failed automatic backup used to fail completely silently; it is now logged
  and shown as a toast, the same way a successful one already was.
- Dictation could silently record without ever delivering text, or never
  report that it had stopped, if one of its Windows event hookups failed to
  register — that failure now ends the session with a proper error instead.
- If Windows itself refused a dictation stop request, the app could still show
  "stopped" while the microphone kept recording; it now reports the truth in
  that case.
- A language-pack install whose status file became permanently unreadable
  used to poll forever with no way to give up; it now stops after a generous
  wait and tells you.
- A failed language-pack update notification could go unnoticed for a whole
  check interval instead of retrying on the next scheduled check.
- Restoring the main window from the tray could reappear off-screen or
  straddling a monitor that had since been unplugged or rearranged; it now
  gets the same on-screen check the app already applies at startup.
- The language-pack manager could show a raw internal status word instead of
  translated text for a language that was not yet installed.
- A second click on a language pack's Install button while the elevated
  install was still starting could launch a second concurrent install for the
  same language.
- A rotating prompt's preview, fetched right as its rotation state changed
  elsewhere, could briefly land after and silently override the newer state.
- **A rotating prompt's "advance on paste" only worked from the floating
  buttons and folder windows.** Copying the same prompt from the grid, the
  library, the copy history, a hotkey or the tray never started the waiting
  session, so the entry never moved on. It now does, everywhere. Entries that
  contain placeholders are the one exception and still advance on copy.
- **Switching prompt syncing on could report success over an empty folder.** A
  folder that could not be created or written to left the switch on and nothing
  in it; the failure is now reported and the switch goes back off.
- Coloring every tag at once wrote the settings file once per tag. It is one
  write now, and a single invalid color rejects the whole batch instead of
  leaving half the tags colored.
- The copy history could show a copy's time taken from a different copy of the
  same prompt when the list was grouped.
- **The minus, plus and preset buttons on every number field (crop, speed,
  fade, resize and more) announced their accessibility label in whatever
  language the app started in, never the current one** — a screen reader kept
  reading the old language after switching, and in the video editor read a raw
  internal name instead of a translation from the very first launch.
- **The warning that the store is being written unencrypted now offers a way
  out instead of only stating the problem**: it carries a second button that
  saves a manual backup right away, so the data can be secured before anything
  else is written.
- The error log no longer collects noise from the test suite, and trimming it
  keeps the newest entries instead of deleting the whole file once it passes
  its size limit.
- **An account that can never install language packs kept being offered a
  "try again" that could not work.** On a locked-down machine, installing a
  text-recognition or dictation language pack now says plainly that only an
  administrator can do it, with no retry button in that case; a real one-off
  failure still gets to retry. The repeated PowerShell error this used to
  write to the crash log on every check is now logged once per session.
- **A declined admin prompt for a language pack install was treated as final
  too often.** Windows still lets a different administrator's password
  complete the same prompt, so a one-off decline now offers "try again"; only
  a computer whose policy blocks the prompt outright shows the "blocked"
  message with no retry button.
- **A promptsaver:// link ended without doing anything.** Since a recent
  startup change, every link exited the process before it could reach the
  window that was supposed to open or copy it.
- **The app no longer freezes during a recording while a backup runs.**
- **The compare view could open behind the duplicate finder, the raised
  library or the edit dialog's similar-prompt hint**, and Escape closed
  whichever of those sat underneath it instead of the compare view itself.
  It now always stacks above the dialog it was opened from, and both Tab and
  Escape reach it first.
- **The chain overview strip kept extra space above its title.** The padding
  above the steps is trimmed to match the space it already kept below them.
- **A tile's menu and select button still popped up on hover while a chain
  was running**, competing with the order badge in the same corner. They now
  stay hidden on every tile until the chain ends, unless multi-select is on.
- **The screenshot, recording and video-export folders could be redirected by
  a settings import or a restored backup**, and the video editor's export
  destination and recording cleanup could be pointed at a folder never
  actually chosen through their own dialogs. These folders are now changed
  only through their dedicated pickers, an import or restore can no longer
  touch them, and cleanup only ever counts or removes this app's own
  recordings.
- **Search everything had no title and its recent searches could not be
  removed.** The window now shows a proper title bar above the search field,
  and each remembered search can be deleted on its own or cleared all at
  once.
- **A recording's typed keys went missing from the keyboard overlay after
  reopening it through "Open an existing file"** (the record button's
  right-click, or a library video attachment). That path works on a
  throwaway copy, and only the copy of the video itself used to travel
  there, never its key-overlay data. Reopening a recording this way now
  brings that data along too.
- **The video editor's inserted-video overlay (a picture-in-picture clip
  added through "Add a video") showed nothing at all in the live preview**
  while you worked on it, although it exported correctly; every other
  overlay kind had its own live-preview drawing, this one never did. It now
  plays in the preview too, muted, starting and looping in step with its own
  time range.
- **That same inserted-video overlay could drift out of time with the
  exported result once a cut or a speed change was also on the timeline.**
  The live preview now follows the same output timeline the export itself
  uses.
- **An inserted video overlay was stretched to its box in the live preview**
  instead of keeping its own proportions the way the export does. It is now
  letterboxed in the preview too, with the same black bars export adds.

- **Main window, settings and screenshot editor fixes.** Dropping a tile on a
  folder now sends the folder to the cell the tile left, and the undo offer
  stays with the view it was made in. Switching to the favourites grid clears
  the grid selection. A folder's members behave like the tiles they stand for:
  a fast second click pastes, a click in chain mode adds the prompt to the
  chain, a missing file is flagged, and the prompt's own font, size and text
  colour are used. The remote access rate limit keeps showing Unlimited after a
  redraw, the video folder numbers and the sync readout follow the current
  folder and open, and a running language install stays busy when its dialog is
  reopened. A refused hotkey no longer leaves the field capturing the wrong
  combination, saving a new prompt without text keeps the dialog and what was
  typed, a dictation result arriving late replaces its live partial, a failed
  clipboard inbox clear offers no undo, undoing a resize no longer unlocks
  steps behind the undo limit, a shift-drag of a top crop corner keeps the
  bottom edge still, and a screenshot whose delete failed is not saved twice.
- **A failed still-frame extraction no longer leaves its picture in the temp
  folder, and abandoned ones are swept.** A frame cut from a video that
  failed half way is removed at once; leftovers from a crash are cleaned up
  with the other temporary files.
- **Key overlay and camera recording were shown as on.** Both are off until
  you switch them on, as intended; before, the expert menu showed them on
  while recording ignored them.

### Security

- **Imports and restores can no longer grant consent or lift locks.** A
  settings file, a full import or a backup restore (in the app and on the
  command line) keeps this computer's own value for the online speech
  consent, the data folder lock and the clipboard secret skipping.

- **`settings set` can no longer change access settings.** The command line
  and remote access refuse folder locations, the vault keys, the backup
  password, every remote access setting and the clipboard watcher and link
  switches; they stay changeable in the app only.

- **A settings or backup file can no longer switch on remote access or other
  machine features.** Importing settings from a file, or restoring a backup
  file, leaves the remote access switches, password, allowed addresses and
  port, the promptsaver:// links, the autostart entry, the certificate, the
  backup password and the folder locations exactly as this computer has
  them. The import preview says how many entries were skipped, and a settings
  export no longer contains the remote access password hash.
- **Remote access from other devices now runs over HTTPS with the app's own
  certificate.** With the network switch on, the receiver accepts only
  encrypted connections, so the password is no longer readable on the
  network. The new **Certificate...** button shows the certificate's
  fingerprint, saves it for other devices and creates a new one. Turning the
  network switch on or off now takes effect at once, without switching remote
  access off and on again.
- **Remote access settings are saved before they take effect.** Switching it
  on, a new password or wider address ranges are undone when the write fails,
  and switching it off always stops the receiver. A restart can no longer
  bring back a receiver you turned off, or one that wants a password you were
  never shown.
- **Stopping and restarting the receiver is reliable.** Each run has its own
  shutdown, so a stop can no longer end the next start, and a restart waits
  for the old listener to release the port.
- **The WebView2 runtime download is now checked for Microsoft's signature**
  before it is run. It is the one file the app may have to fetch before its
  own window exists, and until now only the connection vouched for it.
- **The updater refuses to install an older version than the one you are
  running.** The download was already rejected unless its checksum matched;
  the version itself is now re-checked as well.
- **You are now told when your data cannot be encrypted.** Windows seals the
  database with per-account encryption; on the rare machine where it refuses
  (a damaged user profile), the app keeps saving in readable form rather than
  losing the data — but that used to be noted in the error log and nowhere the
  user would see it. It now says so in a dialog at startup and keeps a
  permanent warning row at the top of the privacy settings for as long as it
  is true, with no switch to silence it. The next save that succeeds seals
  everything again by itself.
- **Backups are now off-limits to the app's own page.** Attached pictures, videos
  and PDFs may live in any folder you pick, so the window is allowed to read
  files by path — with the database and its settings files excluded by name.
  Backup snapshots and exported `.psb` files hold the same data and were not on
  that list; they are now. Nothing about attachments changes.
- **The script behind an elevated language-pack install is now protected
  against being swapped out from under it.** It briefly lived at a predictable
  path in the temp folder before the elevated process read it back; another
  program running as you could in theory have replaced it in that window and
  had its own code run with the permission you were about to grant.
- **Restoring a backup no longer proceeds if its own undo point could not be
  written.** It used to overwrite your live data even when that safety
  snapshot silently failed, leaving nothing to fall back to.
- **"Delete all data", clearing the copy history, and clearing or dismissing
  clipboard-inbox entries now tell you when something could not actually be
  removed**, instead of always reporting success while the data stayed on
  disk.
- **The emergency "purge from all backups" button (clipboard inbox) now
  reports snapshots it could not confirm were scrubbed**, instead of only
  counting the ones it definitely cleaned.
- Two dictation sessions started in quick succession could both begin
  recording; the first one then had no way to be stopped from the app.
- **You are now told when your saved settings cannot be read back.** The same
  protection the database already had for prompts now covers settings too: an
  unreadable or corrupted settings entry is never silently replaced with
  defaults on the next save. A permanent warning row appears in the privacy
  settings for as long as it is true, with no switch to silence it, and a
  factory reset or restoring a backup's settings clears it.
- **A hung background task (a stuck language-pack check, for example) could
  delay every other periodic task behind it** — settings saves, automatic
  backups, the update check — for as long as it stayed stuck. Each now runs on
  its own thread, so one stuck task can no longer hold up the others.
- **Dictation started right after a previous one no longer picks up the
  earlier session's last sentence or gets stuck on listening.**
- Update check, update download, PDF previews and clipboard-image reads no
  longer occupy the app's async runtime while they run.
- Copying an edited screenshot to the clipboard and saving one to disk now
  reject an oversized image up front, the same protection OCR already had.
- Confirming a CSV/TXT import now refuses if the file changed after the
  preview was shown, instead of importing something you never actually saw.
- **You are now told when a settings change could not be saved to disk**
  (a full data folder, missing write permission). The change still applies for
  the rest of the session, but a dialog says it will be lost on restart, and a
  permanent warning row stays in the privacy settings for as long as the next
  save keeps failing — clearing itself the moment one succeeds again.
- **A single prompt can now be marked confidential.** Its text and any
  variants are then encrypted with a vault password, asked once per session
  the first time a confidential prompt is opened or copied, and forgotten
  again on quit or after an idle timeout you can set (default: only on quit).
  Forgetting that password means those entries are lost for good, with no
  recovery, and the setup dialog says so before it lets you continue. A
  locked confidential prompt shows a redacted bar instead of its name, does
  not copy, and stays out of search, Markdown sync, and a plain export by
  default; export can keep it encrypted instead, and a full backup always
  does. Uses the same encryption as encrypted backups, with its own password.
- The recording canvas size limit and capture mode are now range-checked and
  restricted to known values in the app itself, not only in the expert menu,
  so an imported settings file cannot push either past what the menu allows.
- Video export and the recordings-folder tools (cleanup, discard, restore) now
  check every destination and folder argument against one the app itself
  resolved (a save dialog it ran, the configured recording folder, or the
  source file's own folder), instead of trusting that argument on its own.
  Converting, saving and copying a recording now require the same proof for
  their source file, and the screenshot, video-export, recording, sync and
  data folder settings now only accept a folder just returned by the app's
  own picker dialog (or the built-in default), instead of any string. Each
  picked folder is now also tied to the one setting it was picked for, so it
  cannot be spent on a different setting's own dialog. Exporting a video
  opened from outside the app, or a library attachment, now offers the
  export-folder picker instead of refusing outright, and "Save as" after an
  export accepts that export's own output alongside an ordinary recording. A
  sync-folder switch the backend actually refused now says so and keeps
  showing the folder still in effect, instead of a misleading "old folder
  kept" message.
- The crash log's folder-name scrubbing now handles a folder name that
  contains spaces and no longer skips the rest of an older log file over one
  broken byte in a single line. The error log's per-entry origin label is now
  restricted to a fixed, known set instead of accepting any text.
- Corrected the help text's count of the app's own network activity: it now
  also names the update download itself, the GitHub link from the logo or
  version label, and the Windows language-pack install.
- **"All on", the profiles and the resets in the expert menu no longer flip the
  switches that have their own confirmation.** Remote access, its network
  switch, the URL scheme and the Markdown sync are skipped by "All on", and
  demo mode is never written by a profile or a reset, so a running demo session
  also survives "Reset expert menu".
- **Editor drafts are stored encrypted, and a confidential prompt is never
  drafted.** The automatic draft of the open editor is sealed for your Windows
  account like the prompts themselves, so a revealed confidential prompt no
  longer lands in the data file or in a backup as plain text. Drafts saved by
  an earlier version still load, and a prompt turned confidential loses its
  draft.
- **The key overlay record next to a screen recording is stored encrypted.**
  The keys typed during a recording are sealed for your Windows account; older
  recordings with a plain record still open.
- **Uninstalling with "delete my data" also removes the temporary working
  files.** Picture crops, filmstrips and working copies in the temp folder go
  with it, and closing the app now removes its own working copies at once.
- **Switching off the picture text index erases it.** The text read out of
  attached pictures for searching is removed from every prompt (and the
  trash) the moment "Index text inside attached pictures for search" goes off, not only hidden.
- **Switching off the clipboard collector asks what to do with what it
  gathered.** Keep it or delete it; closing the question keeps it.
- **Saved editor work for pictures and videos is stored encrypted.** The
  remembered edits (including text overlays) and the file path they belong to
  are sealed for your Windows account; work saved by an earlier version still
  loads and is replaced in the new form on the next save.

### Removed

- **Eight themes are gone**: Gradient, Sunset, Forest, Midnight, Retro, Lavender,
  Candy and Coffee. Eight remain — System, Light, Dark, Programmer, Neon Violet,
  Ocean, Cyberpunk and Black & White — each covering something the others do not.
  An installation set to one of the removed themes (or a custom theme built on
  one) falls back to System; the theme editor still reproduces any of the old
  looks from a kept base.
- **The count badge on folders**, both in the grid and on the floating folder
  icon. The preview identifies the folder and the tooltip names its members, so
  the number only covered a preview cell.
- **Nine UI languages are gone**: French, Hindi, Indonesian, Vietnamese, Czech,
  Ukrainian, Swedish, Russian and Korean. Eleven remain — English, German,
  Spanish, Italian, Portuguese, Polish, Chinese, Japanese, Dutch, Turkish and
  Romanian — along with their installer translations. An installation still set
  to one of the removed languages falls back to English and shows "Automatic" in
  the language picker; nothing else about it changes.
- **The floating-button entry in a tile's menu could name the wrong action.**
  Right after pinning a prompt, opening the same menu again still offered "Show
  floating button". And once a pill had been dismissed with "Hide until
  restart", the menu correctly offered "Remove floating button" but the click
  brought the button back instead of removing it. The entry now always names
  what a click really does.
- **Dragging a file-backed prompt out of a floating button or a floating folder**
  handed the receiving app a broken path when the file name contained a `?` or a
  `#` — most apps cut the path off at that character. Dragging from the main
  window was already correct; all three now build the reference the same way.
- **Installing a language pack briefly flashed a PowerShell console.** The
  elevated install now runs fully hidden; only the admin-rights prompt itself
  still appears. The language pack dialog also got a clearer row layout: the
  language name is now the most prominent thing in its row, with the size in
  its own column and the state shown as a small chip.
- **Screen recordings can stop themselves at a length or file size you set.**
  Two new expert-menu limits, off by default, end a recording cleanly with a
  message instead of letting it run forever or fill the disk.
- **A low-disk warning now also appears while a recording is running,** not
  only before it starts, with its own icon in the recording bar.
- **Settings can be exported to a file and imported back,** for carrying your
  switches, values and text fields to another machine without prompts or
  hotkeys coming along. Importing shows a preview of what will change first.

- **Rarely used fine-tuning values are gone from the expert menu.** Internal
  timers and retry counts, size limits and pixel values nobody could judge
  without reading the code are no longer settings; the app keeps using their
  previous defaults.
- **The switch that hid the pin button.** The always-on-top pin in the
  header is always there; Settings keeps its own switch.

### Changed

- **The small buttons on a tile no longer show a tooltip.** Select, start a
  chain and the actions menu stay quiet on hover; screen readers still get
  their names.
- **Several expert-menu rows moved next to the feature they belong to.** The
  chain settings have their own "Chain" group, the recording cleanup rows sit
  under "Screen recording", the temporary-export lifetime under "Video editor",
  the image-text index fill action under "Library", the floating-button recall
  action under "Floating buttons & folders", and the grid border, the view
  limit and the grid size limit under "Views and quick grid". Rows that only
  matter with a switch on now hide with it and step in under it, including the
  colour pickers and the fixed-value list. The "Tiles & copying" group is now
  just "Tiles".
- **The safety backup before Reset settings and before Delete all data is now
  on by default**, and split into two separate switches so either case can be
  turned off on its own.
- **A prompt hotkey now also pastes into the previously focused app by
  default.** A saved choice to have it off is kept.
- **Every expert-menu row's "changed" tooltip no longer shows when it was last
  changed.** The undo arrow and the "only changed" filter stay exactly as
  before; only the date/time line is gone.
- **"Hide window after copy" is a renamed internal setting.** If you had
  turned it on, it resets to its default (off) this one time; everything
  else about it is unchanged. Multiple views and the quick grid size now
  have their own "Views and quick grid" group instead of sitting among plain
  toolbar-visibility switches.
- The help tip line now sits behind a lightbulb icon instead of a text prefix,
  and the warning line above the expert menu is shorter and carries a warning
  icon.
- The product name now lives in one generated file per layer instead of being
  spelled out across the code, the interface and the build scripts. Nothing about
  the app changes: it keeps the same data folder, autostart entry and update
  source, and a unit test pins every one of those values.
- **The expert menu has a new "Capture" tab** for everything to do with
  capturing the screen: screenshots and their editor, text recognition, the
  colour picker, screen recording and the video editor now have their own
  fine-grained groups there instead of sharing one long list under Features.
  No setting was removed or renamed, only regrouped, so every saved value keeps
  reading back exactly as before.
- **The screenshot editor's zoom step and maximum zoom are now tunable in the
  expert menu** and match the video editor's own defaults, replacing its
  separate hardcoded values.
- **The duplicate finder's compare view now shows one "keep this one" button
  under each entry** instead of a row of identical buttons below both columns.
- **The duplicate finder also has a shortcut in the header of All prompts**,
  next to sort, alongside its existing shortcuts in the settings dialog and
  the expert menu.
- The three export format entries now just read "CSV", "TXT" and "JSON",
  each already sitting next to its own export button.
- **Chaining moved from the Copying group to Tiles & copying in the expert
  menu**, next to the other tile settings, so the chain icon on a tile is
  easier to find. No setting was removed or renamed, only regrouped.
- White text on light colours gets a dark halo so it stays readable; the
  threshold for black text is unchanged. The halo now starts at 75 percent
  strength instead of 85 (the slider is unchanged).
- The caution line in the expert menu now sits centered and in red at the top
  of the page, stays visible while searching, and reads "Changes here can lead
  to unexpected behavior."
- The right-click menu no longer has "Compare with"; comparing two entries
  stays in the library and in the duplicate finder.
- Rescan in the duplicate finder now shows a small spinning ring for at least a
  second, so you can see it ran, and a second click while it works is ignored.
- A chain now survives switching to another program: the picks stay, locked or
  not, until you click the chain button again or paste with Ctrl+V in the app.
- The steps in the chain overview now reorder by dragging (it did not work
  reliably before) and a right-click removes a step; the clipboard text is
  rebuilt after either change.
- An inserted video overlay now starts at its own beginning and stays in
  sync with its own sound, instead of showing a later moment of the clip
  while the sound played from the start.
- **Closing the window keeps the app running.** A new install now hides to the
  tray on close, so floating buttons, the tray menu and the hotkeys keep
  working; the one-time hint on the first close is gone. Existing installs
  keep their setting.
- **Shorter retention for new installs.** Copy history is kept 30 days and
  unused recordings 90 days by default. Existing installs keep everything as
  before.
- **Confidential entries lock after 15 minutes.** The vault now locks again
  after 15 minutes by default instead of only on quit; a value you set
  yourself is kept.
- **Markdown preview, notes and search focus ship on.** The editor's Markdown
  preview and private notes are on by default, and opening the library puts
  the cursor in its search field. A switch you changed yourself stays as it
  is.
- **Quieter defaults.** Tile videos no longer autoplay or loop. A switch you
  changed yourself stays as it is.
- **Interface and icon size live only in Settings.** The duplicate sliders
  in the expert menu are gone; the two size choices in Settings set the same
  values.
- **One switch for the favorites view.** The settings switch and the expert
  switch for the favorites star are now the same setting.
- **Close after copy is one button.** The library's own button decides and
  remembers it; the two expert switches for it are gone.
- **One length for the copy history.** The window lists what the history
  keeps; the separate row limit is gone.
- **One screenshot quality.** The expert menu keeps only the quality that is
  saved, copied and exported; the preview quality is fixed.
- **One number for every search list.** Library suggestions, Search
  everything and the quick launcher share one entry limit (8 by default);
  the five separate limits are gone.
- **Fewer switches in the expert menu.** The copy bubble, the undo button, the
  editor's draft autosave and the tooltip delay are each one number now;
  setting it to Off turns the feature off.
- **Tooltips have four settings.** On or off, delay, width, and notes in the
  tooltip; the rest is fixed at its old default.
- **Updates have two settings.** Auto update and its interval sit together in
  Settings, and Never turns the automatic check off; the three expert switches
  for updates are gone.
- **Two hover settings.** Dim until hovered (tiles, media and the top bar
  together) and lift on hover are one number each; 0 turns them off.
- **One tile style instead of seven switches.** Bold, uppercase, monospace,
  italic, shadowed names, compact and gradient tiles are one choice now. If you
  had several on, the first of them in that order is kept.
- **The screenshot editor remembers tool sizes.** Each tool starts with the size
  you used last; the twelve default-size sliders are gone.
- **Expert menu regrouped.** Screen recording, while recording and camera
  recording are separate groups, the library splits into Search and Display,
  colours and media moved into their neighbours, and the header's right-click
  list only shows or hides buttons.

## 2.6.3 (2026-07-26)

The backups window grew up: automatic backups can be switched off and now keep
running while the app is open, retention gained a yearly tier, and the page was
rebuilt in two columns. The statistics were trimmed to the numbers that actually
say something, and fourteen tooltips finally describe the setting they sit on.

### Added
- **Automatic backups have an off switch**, right in the backups window — and they
  keep running while the app is open instead of only at launch. Switching them off
  or changing the interval takes effect straight away, without a restart.
- **Yearly retention** — smart retention (formerly "retention by period") keeps a
  number of backups per day, week, month **and year**, as a compact block of four
  fields.
- **Remove password** — a button of its own next to Save, with a confirmation, so
  the backup password can no longer be dropped by an absent-minded click on Save.

### Improved
- **The Backups button became "Backup manager"** and moved down to the other backup
  buttons in the settings, where you would look for it.
- **Backups window in two columns** — taking a backup, the automatic schedule and
  the password on the left, retention on the right.
- **Backup diagnostics trimmed to the eight figures that matter**: count, total
  size, largest and smallest, the period covered, and the next, newest and oldest
  backup.
- **Usage statistics reduced to twelve figures in three clear rows**, with the
  overview and the per-view breakdown side by side instead of stacked. Numbers that
  only repeated what a chart already showed are gone.
- **A backup password you are still typing survives** changing another setting in
  the same window — it used to be thrown away when the page rebuilt itself.
- **A stored password shows as dots**, so you can see that one is set without it
  ever being readable.
- **The update row got the same two-column shape as the toggles above it** — check
  button and version on the left, the auto-update switch on the right.
- Jumping from a statistics chart into the library raises the library over the
  backups window instead of closing it, so closing the library lands you back on
  the statistics you came from.

### Fixed
- **Fourteen tooltips described the wrong thing** and were rewritten in all 20
  languages — among them the backup button (it never opened a page in the expert
  menu), the duplicate finder (it offers no merge), the usage-statistics switch (it
  shows the dashboard, the copy history does the counting), the variable-defaults
  option (it remembers your last value), the view-tab border, the tile-name and
  preview-length options, and the update-check-at-launch switch (it needs
  auto-update on).
- **"Never used" means never used** — the cleanup list no longer mixes in prompts
  that were merely untouched for a month.
- **Switching smart retention on no longer overwrites the plain keep count**, so
  the number you set is still there when you switch it back off.
- **The clear button in the number fields of the backups window** ("Backups to
  keep", the retention tiers, the top-list length) sat beside the field instead of
  inside it, and disappeared for good once it had been used. It now sits at the
  right edge of the field and comes back as soon as the value does.
- Clearing one of those fields and clicking away lands on the setting's default
  instead of leaving the field empty or dropping to the lowest possible value.

## 2.6.2 (2026-07-26)

Backups get their own window, their own encryption and a lot less disk space. The
expert menu was re-sorted from scratch, the portable version is now a single file,
and a long list of rough edges is gone.

### Added
- **Backups window** — the Backups button in the settings opens a page of its own:
  interval and retention, diagnostics, the restore list and the usage statistics,
  all in one place instead of buried in the expert menu.
- **Encrypted backups** — exported backups are AES-256-GCM encrypted. By default a
  built-in key is used, so a backup still restores on any PC. Set an optional
  **backup password** to protect new backups with a key derived from it; older
  backups keep whatever protected them.
- **Password prompt when restoring** — if a backup file needs a different password
  than the stored one, the app asks for it (with a reveal toggle) and lets you retry
  without picking the file again.
- **Compressed backups** — snapshots and exported files are compressed, which takes
  a backup down to a fraction of its former size. Ten snapshots are kept by default,
  so it adds up. Older, uncompressed backups still restore.
- **Portable start asks about your data** — starting the portable version next to an
  existing installation offers to take those prompts along or to start empty.
  Nothing is moved or deleted either way.
- **Split reset buttons** — "Delete all data" and "Reset settings" are separate
  actions now, so you can clear one without losing the other, optionally with a
  safety backup first.
- **Pause the clipboard watcher from the inbox** — a switch in the inbox window
  starts and stops collecting.
- **New expert options**, all off by default: monospace editor, dim the toolbar
  until hovered, hide the window after copying, skip duplicate entries in the copy
  history, back up before a reset, and keep the buttons of switched-off features in
  the toolbar.
- **50 % option** for UI and icon size, for very high-resolution screens.

### Improved
- **The portable version is a single file.** The PDF preview library is built into
  the exe, so nothing has to travel next to it any more.
- **The installer runs through in one pass** — choosing "for all users" no longer
  makes it restart itself halfway through. It asks for permission once at the start.
  A portable installation now lands in its own folder next to the setup file.
- **Expert menu re-sorted** — the oversized "Tiles" block is split into *Tiles &
  copying*, *Tooltips* and *Tile appearance*; copy-history options sit with the copy
  history; folder settings sit with the feature they belong to. 77 settings whose
  name alone does not explain them now have a tooltip, in all 20 languages.
- **Richer backup diagnostics and statistics** — backups this week, the period the
  kept backups cover, copies per day, the most-used prompt, the share of prompts
  ever used and the average prompt length.
- **Tiles lift slightly on hover** by default, and update checks at launch are on
  by default (both switchable).
- Restoring a backup no longer reloads the window; it confirms which file was
  imported and how many prompts came back, or shows why it failed.
- Settings screen: shorter, without redundant sub-headings. The update row holds
  auto-update, the check button, the version and the Backups button in one line.
- Tool icons in the toolbar menu match the real buttons, expert dropdowns are
  themed with their label on the left, and the backup page remembers which sections
  were expanded.

### Fixed
- The version history in the editor is fully reachable on small windows, and only
  as tall as its contents.
- No more flicker when changing a setting, taking a backup, or collapsing a
  category in the expert menu.
- "Reset settings" kept the backup password instead of discarding it — losing it
  would have made every password-protected backup unreadable.
- Copying a prompt with variables follows the same rule as every other prompt now:
  a plain click copies, a fast second click or Enter also pastes. Double-clicking
  such a prompt works again.
- The backup password field shows one reveal icon instead of two, and is saved with
  its own button instead of whenever the field lost focus.
- Minimize, maximize, close and the settings button no longer show tooltips, and
  close no longer turns red on hover.
- Tiles keep their press animation while the pointer is over them.
- Folder settings in the expert menu follow their feature switch, and scrollbars
  keep an even margin in the remaining dialogs.

## 2.5.0 (2026-07-11)

A big feature release: a custom window frame, smarter search and sorting, batch
editing, version history, backups, statistics, a duplicate finder, auto-paste and
a clipboard inbox — plus finer tooltip control. Everything new has an on/off switch
in the expert menu, in all 20 languages.

### Added
- **Custom title bar** — the native Windows frame is gone; minimize, maximize and
  close now live in the app's own top-right corner and follow every theme.
- **Automatic backups & restore** — rotating snapshots of your whole store, taken
  on start and on demand, with settings, diagnostics and a restore list all on the
  expert **Backups** page. Optional period-based (day/week/month) retention.
- **Version history** — every prompt keeps a history of its previous name/text;
  open the collapsible **History** row in the editor to preview and restore.
- **Batch operations** — a **Select** mode in the library: multi-select (with
  Shift-range) to set color, (un)favorite, add/remove from a view, export or
  delete many prompts at once.
- **Usage statistics** — a dashboard on the expert **Backups** page with copy
  totals, most-used bars, cleanup candidates and per-view counts.
- **Duplicate finder** — find near-duplicate prompts (expert · Tools), compare
  side by side, keep one, or mark a pair as "not a duplicate".
- **Fuzzy library search** — typo-tolerant search ("promt" still finds "prompt").
- **Smart sort** — sort the library by **Most used** or **Recently used**.
- **Auto-paste** — a fast double-click (or Enter) on a tile, library row or pill
  copies **and** pastes into the last app you used (opt-in).
- **Clipboard inbox** — an optional watcher (normal settings toggle; capture
  length + inbox size in the expert menu) that collects text you copy anywhere,
  so you can save the good bits as prompts.
- **Live length counter** — chars · words · ~tokens under the editor textarea.

### Improved
- **Tooltips** now wait for an adjustable hover delay before appearing, so moving
  the mouse across the window no longer flashes every hint. A plain **Show
  tooltips** switch was added to the normal settings, above the screenshot toggle.
  Setting tooltips that point to expert options now break onto a tidy second line.
- **Click the app logo** to open the project's GitHub page; **click the version**
  next to *Check for updates* to open the releases page. After a check the version
  line reads the result ("Latest version already installed" / "Update to X available").
- **Settings** popup is wider and tidier: theme / language / font / text size / UI
  size / icon size sit three across; the on/off toggles fill two columns; full-width
  field labels moved to the left of their control. The per-page views editor is gone
  (views are managed from the top view tabs — the "+" adds one, right-click a tab to
  rename, resize its grid via dropdowns, recolor or delete); the **Favorites view**
  toggle moved into the toggle grid.
- **Right-click the top toolbar** to toggle *every* top-bar button — library, copy
  history, chain, pin, clipboard inbox, favorites, view tabs, quick grid, logo, title
  (everything except Settings). Each entry shows its own icon, and the menu scrolls if
  the window is too short.
- **Image tiles** now fill the whole button edge-to-edge and stay put while the
  window is resized.
- **Batch operations** are faster — the backend no longer echoes every prompt back
  over the bridge after a bulk change; the library search no longer re-scores every
  prompt three times per keystroke.
- Prompts with **variables** always ask for them before copy + auto-paste.
- **Icon size** now also scales the bottom prompt field, the Save button and the top
  view tabs and grid-size boxes, so the toolbars stay balanced instead of the icons
  dwarfing what sits beside them.
- **Settings** layout refinements: text size sits under font in the third column, the
  global hotkey is its own row under the toggles, "Updates" is a left-aligned heading,
  and the expert menu keeps a compact two-column width.
- **Updating / reinstalling** removes the previous version automatically and silently —
  no leftover uninstaller window, and your prompts and settings in AppData are kept.
- **Expert menu** reorganised: the scattered "Extras" groups are dissolved into their
  proper categories, every default-off feature is tagged **(Standard aus)**, and new
  sliders were added (grid padding, popup corner radius, backdrop dimming).
- **Statistics** show more: views, media prompts and variable prompts; the backups page
  gained largest/smallest-backup diagnostics, a tidier settings row, and an always-on
  auto-backup with "Backup now" beside the interval.
- **Screenshot dialog** always fits without scrolling (the preview scales down), and the
  save-to-folder toggle sits inline at the bottom.

### Fixed
- **Copy history no longer empties itself.** The retention window defaulted to 7 days,
  which silently deleted entries for anyone who copied less than weekly. It now **keeps
  forever** by default (bounded only by the entry count), with an "∞ forever" option
  plus 7 / 30 / 90 / 365-day choices in the expert menu.
- **UI size** scaling no longer breaks the layout — it uses the WebView's native zoom,
  which keeps popups, the grid and hit-testing aligned at every scale.
- The **clipboard inbox** icon shows a numbered badge whenever collected items are
  waiting, instead of clearing the count the first time you open it.
- The **Updates** line returns to showing the version number after you close settings,
  so a transient "already newest" message doesn't linger.
- The **view editor** dialog's close ✕ matches the other modals (and is a bit larger),
  and the title-bar **★** favorites toggle is a touch larger.
- The **settings popup** sizes to its content and centres — no empty band at the bottom
  on a large window — while still capping at the window height and scrolling inside.
- **Delete all prompts** and **Expert menu** reliably share one row (delete left, expert
  right); a later CSS rule had been stacking them.
- Keyboard focus now shows a **themed focus ring** on every button and chip instead of the
  WebView's off-theme blue outline.
- The **"Favorites only"** filter chip keeps its gold highlight and the **"Clear history"**
  button shows its red styling again (two more CSS cascade fixes).
- **Escape** now closes the copy-history panel too, and closing the variable-fill dialog no
  longer also closes the library behind it.

### Security & reliability
- The **updater** verifies the download is a real Windows executable before running it.
- A **failed save** (e.g. disk full, where both the database and JSON fallback fail) is now
  written to the crash log instead of being lost silently.
- **Screenshot deletion** is constrained to the screenshot folder — a matching file name can
  no longer point it at another location.
- **Signed-strength update check**: the auto-updater now verifies the downloaded installer's
  **SHA-256** against the hash published with each release before running it.
- **Update download** URL is now strictly pinned to the exact release path (rejects
  `..`, `\`, encoded traversal, wrong host or non-`.exe`) and refuses a truncated file.
- **Never lose prompts**: rows that fail to decrypt (e.g. after a Windows profile move
  or password reset) no longer get wiped by the next save — prompt writes are locked
  and the raw store is copied to `data.db.locked.bak` instead.
- **Clipboard inbox** honours the password-manager opt-out format, so copied secrets
  are not captured; its watcher can no longer deadlock the app when toggled quickly.
- Hardened `import all` (malformed backups can't crash the app), the "move data
  folder" cleanup (only ever deletes the folder just moved away from), the asset
  scope (store files are denied), and removed an unused window-creation permission.

## 2.4.1 (2026-07-11)

### Improved
- The **update notification** is now a clearer, more prominent card — an icon,
  the new version, and a one-click **Install now** button — while still matching
  your chosen theme.

### Fixed
- The scrollbar in the update changelog now looks exactly like every other
  scrollbar in the app.

## 2.4.0 (2026-07-11)

A polish release: a full-screen viewer for your images and videos, cleaner
tooltips, faster and better-named screenshots, and a tidier expert menu.

### Added
- **Full-screen media viewer**: click any preview image or video in an edit or
  library dialog to open it full-screen. Scroll to zoom in (up to 8×), drag to
  pan around, and close it with the **✕** in the corner or the Escape key.
- **Favorite star in the title bar**: the edit and add dialogs now show a gold
  **★** in their title bar — one click marks a prompt as a favorite, replacing
  the old checkbox row.
- **Forget recent searches**: each entry in the recent-searches dropdown now has
  its own **✕** to remove it from the list.
- **Tooltip toggle**: a new expert-menu option turns the small icon tooltips off
  if you prefer a quieter interface.

### Improved
- **Redesigned tooltips**: every hover hint now uses the same themed popup, and a
  tooltip that is left open hides itself automatically after a short while.
- **Better screenshot names**: captures are now saved as `Screenshot` plus an
  exact date and time; capturing a single window also adds that app's name.
- **Tidier expert menu**: the settings that adjust a feature now sit directly
  under that feature's on/off switch and disappear when it is turned off, so
  there are no more stray sliders for a disabled feature. The optional "extras"
  groups start collapsed.
- **The recent-searches dropdown no longer pops up on its own** when you open the
  library or history — it appears only when you click the search field again.
- **Consistent look**: the three-dot menu on every tile now matches your theme
  regardless of the tile's color or content, scrollbars sit flush against the
  edge, the close **✕** is in the same spot in every popup, and the grid-size
  **✕** is crisp and centered. A clearer chain-link icon marks chained prompts.
- The app title can no longer be accidentally selected as text.

### Fixed
- **Saving a large screenshot as a button no longer freezes the app** — captures
  are stored efficiently and appear instantly.

## 2.2.0 (2026-06-20)

A big update: a built-in screenshot tool, a searchable prompt library, prompt
variables, a copy history, a much larger expert menu, and smoother, sharper
floating buttons.

### Added
- **Screenshot tool**: a camera button captures a screen region or a whole
  window — even protected ones like the Task Manager — and turns it straight
  into a copy button. The capture overlay opens instantly.
- **Searchable prompt library**: the header list is now a search box over all
  your prompts — type to filter, copy with one click, edit with the pencil
  icon, and optionally close the library right after copying. Filter by color
  or type.
- **Prompt variables**: write `{{Name}}` placeholders in a prompt and you're
  asked to fill them in when you copy it.
- **Copy history**: a journal of recently copied and most-used prompts with
  optional timestamps and an adjustable retention time — and it can be turned
  off completely for privacy.
- **Drag & drop**: drop a file, image or text onto the window to create a
  button right away.
- **Expert menu, reorganised and expanded**: tidy tabs (Features / Appearance /
  Privacy / Media) with size scaling for the UI, popups, icons and buttons,
  options to hide the logo or the title, the **"Copied!" text size and font**,
  **floating-button opacity**, history settings, and a guarded reset.
- **Auto-fit "Copied!" text**: the confirmation text scales to the button or
  floating pill (or pick a fixed size and font).

### Improved
- The screenshot overlay now opens **instantly** instead of taking a few
  seconds.
- Floating text buttons resize **smoothly** and always keep their text fully
  visible — they never collapse into a circle and never clip; text and button
  scale up together to the screen size.
- You can drag a floating button only on the visible pill, not on the
  invisible area around its rounded corners.
- All 20 languages are fully translated, including every new menu and option.

### Fixed
- Right-clicking a floating button no longer changes its size.
- Screenshot capture works for windows that previously came out blank.
- The update check no longer mis-orders version numbers.
- Various small layout and resize glitches on the floating buttons.

## 1.8.0 (2026-06-07)

### Added
- **Video player on floating buttons**: hover the lower edge of a video pill
  for play/pause, scrubber, time, loop toggle and sound — same controls as on
  grid tiles
- **Volume slider**: hovering the sound button opens a vertical slider
  (grid tiles and floating buttons)
- **Saved player state**: volume, mute and the loop / play-once choice are
  remembered per prompt and restored on the next start
- **Close button** (X) in the floating button's right-click menu
- The WebView2 setup dialog on first start now speaks all 20 languages

### Improved
- Images and videos fill the floating button **exactly** — every pixel
  visible, no cropping; S/M/L scales the whole button
- Dragging a video button or a video tile no longer stutters
- Tile text keeps its size when the window moves between monitors with
  different display scaling
- The floating button's right-click menu is more compact
- Screen reader labels follow the chosen language

### Fixed
- The image on a floating button could appear wrong or not at all
- A frozen video on a floating button now recovers by itself
- Saving a prompt shows an error message if it fails instead of failing
  silently

## 1.7.0 (2026-06-06)

### Added
- **Collapsible bars**: subtle arrows hide the top bar and the input bar —
  the grid grows to use the freed space, the choice survives restarts, and
  small floating arrows bring each bar back
- **Grid-size picker**: the size fields open a scrollable dropdown with all
  values; the current one is highlighted and the mouse wheel steps through

### Improved
- Settings: font and text size now have their own labelled dropdowns
- The settings scrollbar stays inside the rounded corners
- The image and hide buttons in the input bar line up with the Save button

## 1.6.3 (2026-06-06)

### Fixed
- The auto-update label in the settings no longer wraps in languages with
  longer wording

## 1.6.2 (2026-06-06)

### Improved
- Upgrading over an existing installation no longer asks about the previous
  version — it is removed automatically (your prompts and settings are kept)

## 1.6.1 (2026-06-06)

### Fixed
- Installing an update via the notification or the settings no longer fails —
  the silent installer now starts reliably and the app restarts itself
- The update check runs right after launch (was: 30 seconds later)
- No more white flash at the window edges while resizing
- "You're up to date" in the settings makes way for the version number
  after a few seconds

## 1.6.0 (2026-06-06)

### Added
- **Auto update toggle** in the settings (on by default), with a tooltip
  explaining the once-a-day check

### Improved
- Updates now install **fully automatically**: silent installer, no clicks,
  the app restarts itself on the new version

## 1.5.0 (2026-06-06)

### Added
- **Automatic updates**: the app checks GitHub once a day for a new release
  and shows a notification with an **Install now** button; you can also check
  and install manually under Settings → Updates
- Current version is shown in the settings

### Fixed
- Tile text can no longer overflow its button when resizing the window right
  after startup — sizes are re-validated and tiles clip as a hard guarantee

## 1.4.0 (2026-06-06)

### Added
- **Image prompts**: the image button next to the input saves a picture from
  the clipboard (or via file dialog) — clicking the tile copies the image,
  ready to paste anywhere
- **Icon images**: any text prompt can show a picture on its tile instead of
  the name (the click still copies the text)
- Tiles and floating buttons display images edge to edge; the chosen color
  frames grid tiles, floating image buttons are borderless square boxes with
  much larger S / M / L sizes
- High-quality scaling (up to 1024 px, Lanczos) keeps images sharp

### Fixed
- The window now appears only after the first fully sized layout — no more
  visible text resizing on startup
- The "Copied!" overlay on floating image buttons matches the visible image

## 1.3.3 — HotFix (2026-06-04)

### Added
- **Prompt library**: new list button in the header shows every saved prompt
  with its full text — click an entry to edit, drag it onto the grid or use
  its add button to place it on the current layout
- **Quick grid size**: columns × rows of the active view directly in the
  header, next to the library button
- **More colors**: full-spectrum palette (12 colors) plus a free color picker
- Delete option inside the edit dialog
- Hovering a prompt button shows the stored text (what gets copied)
  in the tooltip

### Fixed
- Wrong text size right after starting on monitors with display scaling
- Tile text no longer shifts while dragging a prompt — the drag ghost now
  follows the cursor exactly and the picked-up tile keeps its size
- Letters with descenders (g, j, p, y) are no longer cut off at the bottom
  edge of prompt buttons
- Words are no longer cut mid-word: text hyphenates where possible,
  otherwise the font shrinks to fit

### Improved
- The installer now lets you choose between **installing for all users**
  (asks for administrator rights) or **only for the current user**
- Deleting a prompt always asks for a second confirmation
- The overflow tray is gone — unplaced prompts live in the prompt library

## 1.3.2 (2026-06-04)

### Added
- **Installer** (alongside the portable exe): desktop / start menu shortcuts
  and "run after install" — all pre-selected, all optional
- **Floating button menu**: right-click a floating pill for size presets
  (S / M / L), editing the prompt, or removing the pill
- Floating pills use the same font and text-size rules as the grid tiles
- The default "Home" view follows the app language in all 10 languages
- Automatic check for the WebView2 runtime with a guided one-click install

### Fixed
- App froze when toggling a floating button — fixed for good
- Floating buttons now appear reliably and spawn at the top-left of the
  primary monitor
- Floating button background is fully transparent (no halo, no box)
- Auto-fit tile text fills the whole button with clean word wrapping,
  never clipped at the sides
- Empty row in the floating button menu removed; "Copied!" feedback is
  smaller and unobtrusive

### Changed
- Default grid size is now 5×4
- "Minimize to background on close" is now off by default
- Windows-only build: all mobile (iOS/Android) leftovers removed

## 1.1.0 (2026-06-04)

- First Tauri release: ~3 MB portable exe
- Prompt grid with free placement, multiple views, per-prompt colors,
  10 languages, import/export, floating quick-copy buttons, tray mode,
  autostart
