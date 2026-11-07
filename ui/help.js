// In-app documentation. EN + DE only (deliberate scope decision):
// every other language falls back to English. Kept out of i18n.js on purpose: that
// file's parity check (tests/i18n_check.js) requires every key in all UI languages,
// which this content deliberately does not follow.
//
// Mirrored (structure, not word-for-word) in docs/help.md: keep both in sync when
// a category changes.
//
// `id` is an optional stable key, present on the handful of articles something
// outside this file needs to jump straight to (the first-run tour's help buttons,
// so far): unlike `title`, it never changes with a rewrite and stays the same in
// both languages. Add one only when a new caller needs an anchor.

export const HELP_CONTENT = {
  en: [
    {
      section: "basics",
      title: "Getting started",
      whatItIs: "A tile is a saved item on the grid; clicking it copies its text to the clipboard.",
      howTo: [
        "Double-click an empty grid cell, or type into the box at the bottom and press Save, to create one.",
        "Type or paste the text, then save.",
        "Click any tile to copy it.",
      ],
      whatHappens:
        "Three sample tiles ship on first launch: an email with placeholders, an image, and a Markdown template. Remove them with one click above the grid once they are no longer needed.",
      settings: [],
    },
    {
      section: "basics",
      title: "Settings",
      whatItIs: "The settings window holds the everyday choices; everything optional lives in the expert menu, opened from the same window.",
      howTo: [
        "Click the gear button in the top bar to open it.",
        "Appearance: theme, language, interface size, tile font and size, tooltips and the favorites view.",
        "Behavior: always on top, keep running in the background when closed, all hotkeys, start with Windows and start in the background.",
        "Data: import, export, the backup manager and the usage statistics.",
        "System: the About window, the update check and automatic updates.",
        "A portable copy always quits when closed instead of going to the background.",
      ],
      whatHappens:
        "Every change applies at once, with no save button. \"Automatic\" as the language follows Windows and falls back to English. Reset puts every setting back to its default and keeps the tiles; Delete removes all data and asks first.",
      settings: [],
    },
    {
      section: "basics",
      title: "Introduction tour",
      whatItIs: "A four-step walkthrough of the basics, shown once automatically right after the example tiles are created.",
      howTo: [
        "First launch: opens on its own, right after the example prompts are seeded.",
        "Later: reopen it with the expert menu's \"Show the introduction again\" action.",
        "←/→ or the dots move between steps, Enter continues, ESC or \"Skip\" leaves any time; ESC shows an Undo notice that reopens the tour.",
      ],
      whatHappens:
        "The tour changes no setting; Finish and Skip only mark it as seen. A first-launch data-folder or WebView2 prompt always takes priority; the tour just waits for its turn.",
      settings: ["Open automatically on first launch: Expert menu → System & data → System"],
    },
    {
      section: "basics",
      title: "The grid",
      whatItIs: "The main window's grid is a free-placement board of tiles, sized per view.",
      howTo: [
        "Drag a tile to any empty cell.",
        "Drop a tile on an occupied cell to swap the two.",
        "Drop a tile on a view tab above the grid to move it into that view instead.",
        "Hold Ctrl while dropping on a view tab to copy the tile there, so it stays in both places.",
        "Change the grid size by right-clicking a view tab or an empty spot of the grid and choosing Grid size.",
      ],
      whatHappens:
        "A tile keeps its cell until moved; the grid never rearranges itself. Shrinking the grid does not delete tiles that fall outside the new size; they stay assigned and reappear once it grows back. The 20x20 default is only a starting point; the expert menu's limit setting raises it up to 200x200.",
      settings: ["Quick grid-size control, grid border, maximum grid size: Expert menu → Features → Views and quick grid"],
    },
    {
      section: "basics",
      title: "Creating tiles",
      whatItIs: "Beyond plain text, a tile can carry an attached file, a screenshot, or a picked screen color.",
      howTo: [
        "File attach: clicking the tile copies the file itself, not text.",
        "Screenshot tool: capture a region or a whole window into a new tile.",
        "Color picker: pick any on-screen pixel; copies as hex, RGB, or HSL.",
        "Drag and drop: drop a file or image onto the grid.",
      ],
      whatHappens: "The attached file, screenshot, or color becomes the tile's content; a later copy delivers that file, image, or color, never text. A plain text, Markdown, JSON or CSV attachment shows its first lines right in the editor, below the file row, always as plain text.",
      settings: ["File attach, keep a copy of an attached file and delete that copy with its tile, text preview for attachments, paste, drag and drop: Expert menu → Features → Attachments and input", "Screenshot tool: Expert menu → Capture → Screenshot", "Color picker: Expert menu → Capture → Color picker"],
    },
    {
      section: "basics",
      title: "The editor",
      whatItIs: "The editor is a plain text box at heart; a few features such as Markdown preview and private notes add structure and versions to it.",
      howTo: [
        "Open a tile's editor: double-click the tile, or Edit from its menu.",
        "Adjust Markdown preview, syntax highlighting, versions, or private notes in the expert menu.",
      ],
      whatHappens:
        "Versions keep every saved change: Restore brings an earlier one back, and the button next to it keeps that version as a new tile instead. Markdown preview shows next to or instead of the text box and never changes what is stored. Private notes are never copied or exported with the text. The open text is saved as a draft every few seconds; if the window or the app closes before saving, the next start offers it back once. A confidential tile never gets a draft. \"Copy formatted\" also puts the rendered Markdown on the clipboard, so Word or a mail app shows the formatting. Saving a new tile that closely matches an existing one shows a hint but still saves.",
      settings: ["Markdown preview, syntax highlighting, versions, private notes: Expert menu → Features → Editor", "Monospace text box: Expert menu → Features → Editor", "Formatted copy, live length counter, confirm discarding changes, draft autosave and its interval: Expert menu → Features → Editor", "Similar-prompt hint on save: Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "basics",
      title: "The expert menu",
      whatItIs: "The expert menu holds every optional feature behind its own switch and parameters.",
      howTo: [
        "Open Settings, then the expert menu.",
        "Pick a tab, or type in the search box to scan every tab at once.",
        "Turn on \"Only changed\" to see just what has been adjusted from the defaults.",
      ],
      whatHappens:
        "A feature that ships switched off carries a small dot badge on its label, with a tooltip explaining what it does. Reset works at three levels: one value's own undo icon, a whole group's reset icon in its heading, or every setting at once from the general reset action. The search also finds settings outside the expert menu, such as the settings window or the backup manager, marked with where they live; a click opens that place.",
      settings: [],
    },
    {
      section: "basics",
      title: "Keyboard-only use",
      whatItIs: "Arrow keys move a focus ring across the grid for keyboard-only copying.",
      howTo: [
        "Use the arrow keys to move the focus ring, Enter or Space to copy.",
        "Tab still moves through buttons and fields everywhere else in the app, as usual.",
      ],
      whatHappens: "The focus ring fades out a short while after the window loses focus, so it does not linger over a tile once another app is in use.",
      settings: ["Keyboard navigation, fade delay: Expert menu → System & data → System"],
    },
    {
      section: "basics",
      title: "Common questions",
      whatItIs: "This section answers the questions that come up most often.",
      howTo: [
        "\"My screenshot came out empty\": screen-capture exclusion is working as intended; switch on \"Allow screenshots of Prompt Saver\" in the expert menu if unwanted.",
        "\"The app will not start\": it needs the Microsoft WebView2 runtime, offered automatically by the installer.",
        "\"I cannot find a setting\": the expert menu's search box scans every tab at once.",
        "\"A hotkey does not do anything\": another app may already hold that combination; the editor explains why.",
      ],
      settings: [],
    },
    {
      section: "features",
      title: "Copying",
      whatItIs: "Copying is the sequence between clicking a tile and its text landing on the clipboard.",
      howTo: [
        "Click a tile once to copy.",
        "If auto-paste is on, click again quickly or press Enter to also paste.",
        "Ctrl-drag a tile onto another window to drop its text directly, with no clipboard round-trip.",
      ],
      whatHappens:
        "A short bubble or flash confirms the copy. Chaining combines several tiles clicked in a row into one clipboard entry instead of each overwriting the last. The chain button in the top bar starts a chain and ends it on the next tap; \"Start chain here\" in a text tile's right-click menu starts one with that tile first. A chain survives switching to another app; Ctrl+V in the app window ends it. While chaining, drag a step in the strip above the grid to reorder it, or right-click it to remove it. A red step needs a placeholder that no earlier step fills; the chain still runs. Chaining is not rotation: rotation changes what one tile holds from copy to copy (see Rotation).",
      settings: [
        "Chaining, and starting a chain from the tile menu: Expert menu → Features → Chain",
        "What goes between two chained steps: Expert menu → Features → Chain",
        "Keep chain mode on after copying: Expert menu → Features → Chain",
        "Auto-paste on double-click: Expert menu → Features → Copying",
        "Per-tile hotkeys, press Enter after auto-paste, drag-out, hide the window after copying, size of the copy bubble: Expert menu → Features → Copying",
        "Which key drags a tile out, and whether that key drags out or moves it: Expert menu → Features → Copying",
      ],
      sketches: ["flowToTarget", "chainSteps"],
    },
    {
      section: "features",
      title: "Placeholders",
      whatItIs:
        "Placeholder markup inside a tile's text opens a fill-in popup before the finished text reaches the clipboard.",
      howTo: [
        "Type {?PlaceholderName} inside the text.",
        "Or click Insert placeholder under the text box: it opens the placeholder section.",
        "For a self-filling value, click a dynamic-placeholder chip under the text box instead of typing it.",
        "Click the tile to copy: the fill-in popup opens automatically.",
        "Enter a value per placeholder, or pick one for choice placeholders, and confirm.",
      ],
      whatHappens:
        "Only the filled-in text reaches the clipboard; the raw {?...} markup is never copied. A dynamic placeholder (date, time, clipboard, UUID) is written with an @ in front, {?@date}, and fills itself in with no popup at all. The popup can remember the value last typed for a placeholder and offer it again. A list of fixed values, kept in the expert menu, is suggested for a placeholder of the same name; it is only ever suggested, never filled in on its own. Prompts written with the older {#{Name}#} spelling are converted automatically.",
      settings: ["Dynamic placeholders, choice placeholders, remember choices, remember typed values, fixed values list, date and time style: Expert menu → Features → Placeholders"],
      sketches: ["placeholderAnatomy"],
    },
    {
      section: "features",
      title: "Hotkeys",
      whatItIs:
        "Two independent hotkey types exist: one global combination that opens the quick launcher, and one combination per tile.",
      howTo: [
        "Set the global hotkey in Settings → Hotkeys, row \"Quick launcher\" (default: Ctrl+Alt+P).",
        "Give a tile its own hotkey from its editor.",
        "Choose whether it should paste as well as copy.",
        "\"Hotkeys\" in Settings opens one list: global and per-tile hotkeys on top, searchable, then the keys of every window.",
        "Each row rebinds or clears on the spot.",
        "If a combination is already used, the field explains why instead of accepting it silently.",
      ],
      whatHappens:
        "A combination must include Ctrl or Alt; Shift alone, or a bare key, is refused so ordinary typing is never swallowed. The app hotkey always wins a conflict; among tiles, the first one in the list keeps a combination two tiles happen to share. Importing a backup that carries a colliding hotkey drops the losing one instead of failing the import, and the import summary says how many were dropped. A tile with its own hotkey shows a small keyboard symbol on it.",
      settings: [
        "Global hotkey, all hotkeys: Settings → Hotkeys",
        "Per-tile hotkeys, paste-after-copy: Expert menu → Features → Copying",
        "Hotkey badge: Expert menu → Tiles → Tiles",
      ],
    },
    {
      section: "features",
      title: "Rotation",
      whatItIs: "A tile can hold a list of texts instead of one; each copy serves the next rotation text.",
      howTo: [
        "In a tile's editor, turn on \"Rotation\" above the text area.",
        "The list of rotation texts then replaces the plain text field.",
        "Add more rotation texts with \"+ Rotation text\"; icons beside each one reorder or remove it.",
        "Choose order (sequential) or shuffle (random, no repeats until every rotation text is used).",
        "Copy the tile as normal: it always serves the next rotation text, never asks which one.",
      ],
      whatHappens:
        "A small badge like \"2/5\" counts up on a grid tile, a floating button and a desktop folder member; a grid folder member shows only its file-type icon. In order mode it shows the position; in shuffle mode it counts draws in the current round and restarts at 1 once every text was used. The count survives restarts and full backups; a CSV or TXT import starts the new tile at its first text. Turning rotation off keeps all texts but uses only the first until it is back on. With \"Rotate on paste\", a tile also moves to its next text on every real paste, not only on copy.",
      settings: ["Rotation, default order or shuffle, badge, maximum rotation texts, rotate on paste: Expert menu → Features → Rotation. The per-tile switch itself is in the editor under Advanced options."],
    },
    {
      section: "features",
      title: "Floating buttons",
      whatItIs: "Floating buttons are small always-on-top copy buttons that sit outside the main window, over whatever app is in use.",
      howTo: [
        "Right-click a tile, then \"Show floating button\", to pin it as a floating button.",
        "Drag the floating button to move it; right-click it for size, edit, or remove.",
      ],
      whatHappens: "Positions survive app restarts. A button whose saved screen is no longer connected opens on the main screen, marked as parked, and jumps back on its own once that screen returns. An action in the expert menu brings every floating window to the main screen at once.",
      settings: ["Floating buttons: Expert menu → Features → Floating buttons & folders", "Hide long-parked buttons, bring all back: Expert menu → Features → Floating buttons & folders"],
    },
    {
      section: "features",
      title: "Folders",
      whatItIs: "The same gesture groups things in two places: drop one item onto another of its own kind to gather it into a folder.",
      howTo: [
        "Drag one floating button onto another to make a folder on the desktop.",
        "On the main grid, drag one tile onto another to make a folder in the grid.",
        "Drag a member back out to free it.",
      ],
      whatHappens:
        "A folder lives where it was made, in the grid or on the desktop. A desktop folder shows up to four member previews and a count; a grid folder shows how many tiles it holds and one color dot per tile. Dragging a member out frees it, and the folder dissolves once one member is left; a tile belongs to only one grid folder at a time.",
      settings: ["Folders: Expert menu → Features → Floating buttons & folders"],
      sketches: ["folderTypes"],
    },
    {
      section: "features",
      title: "Tray",
      whatItIs: "The tray icon's right-click menu offers quick access without opening the main window.",
      howTo: ["Right-click the tray icon.", "Pick an item from \"Recently copied\" or \"Favorites\" to copy it immediately."],
      whatHappens: "The menu copies straight from the tray; the main window never has to open for this. The menu is drawn as a rounded window in the app's own theme; switch that off for the plain Windows menu. Typing in the tray menu's search field opens the quick launcher with what you typed.",
      settings: ["Windows 11-style menu, search field, recently copied section, favorites section, entry counts: Expert menu → Features → Tray menu"],
    },
    {
      section: "features",
      title: "Quick launcher",
      whatItIs:
        "The quick launcher is a small always-on-top popup that searches every tile's name and text as you type: a plain substring match, not a fuzzy one, so a name hit ranks above a text hit but a typo will not find a match.",
      howTo: ["Press Ctrl+Alt+P, or the configured hotkey, from anywhere.", "Type to search, then click or press Enter on a result to copy it."],
      whatHappens: "The launcher closes itself right after copying, so it never lingers on screen. A line at the bottom lists its keys.",
      settings: [],
    },
    {
      section: "features",
      title: "Multiple views",
      whatItIs:
        "A view is one grid page with its own size and arrangement.",
      howTo: [
        "Click plus next to the view tabs to add a view.",
        "Right-click a tab to rename, resize, recolor, or delete it.",
        "Move a tab with \"move further left\" or \"move further right\" in its menu, or drag it onto another tab.",
      ],
      whatHappens: "A tile normally sits in exactly one view (adding it elsewhere moves it there) unless \"Prompt in multiple views\" is on. Deleting a view does not delete the tiles placed on it; a tile also shown in another view keeps working there.",
      settings: ["Multiple views, tile in several views, maximum views: Expert menu → Features → Views and quick grid"],
    },
    {
      section: "features",
      title: "Voice input",
      id: "voiceDictation",
      whatItIs: "The microphone button {icon:mic} turns speech into text through Microsoft's online speech recognition.",
      howTo: [
        "Click the microphone icon next to the input bar, or in a tile's editor.",
        "Speak; the text appears as it is recognized.",
        "Click the icon again, press ESC, or save/close to stop.",
      ],
      whatHappens:
        "Voice input uses Microsoft's online speech recognition, so your audio goes to Microsoft while you speak. It starts only after you agree in the consent dialog, which also turns on online speech recognition in Windows. Switching off \"Allow online recognition\" withdraws that consent. Spoken words like \"comma\" or \"new line\" become punctuation; say \"literal\" before a word to type it instead. A missing language pack opens the language pack dialog. An optional global hotkey starts and stops voice input without opening the window. The text goes to the clipboard, is pasted, or lands in the input bar; while it runs, the tray icon's tooltip shows \"Recording…\".",
      settings: ["Voice input, punctuation commands, auto-capitalize, silence auto-stop, insert position, global hotkey: Expert menu → Features → Voice input"],
    },
    {
      section: "features",
      title: "Language packs",
      whatItIs: "Text recognition and voice input share Windows' own language packs and the same install dialog.",
      howTo: [
        "Pick a language from the recognition-language dropdown; a missing pack opens the install dialog automatically.",
        "Installing a pack needs administrator rights: Windows asks for them once, through its own confirmation prompt.",
        "\"Manage text recognition languages…\" or \"Manage voice input languages…\", below the language dropdown in the expert menu, reopens this dialog any time.",
      ],
      whatHappens:
        "On some Windows builds even listing what is installed needs administrator rights; if so, a button re-runs that check through the same elevation prompt. A background watcher notices when an installed pack's version changed or a Windows update silently removed it, and flags it without reinstalling right away. Dismiss it for a while with the notice's \"Cancel\" button, or turn the watcher off entirely.",
      settings: [],
    },
    {
      section: "capture",
      title: "Screenshot tool",
      whatItIs: "The screenshot tool captures part or all of the screen into a new tile.",
      howTo: [
        "Click the camera icon.",
        "Drag a region, or click a window to capture it whole.",
        "The screenshot editor opens: save the shot as a file or a tile there, or just close it and paste.",
      ],
      whatHappens:
        "The app's own windows are hidden for the moment of capture so they never end up in the shot. The result reaches the clipboard immediately; saving it as a tile or file is a separate, optional step. Saving into the screenshots folder never replaces an existing file; it takes the next free name.",
      settings: ["Screenshot tool, save location, format and quality, capture delay: Expert menu → Capture → Screenshot", "Always save screenshots: Expert menu → Capture → Screenshot"],
    },
    {
      section: "capture",
      title: "Screenshot editor",
      id: "screenshotEditor",
      whatItIs: "Opens right after a capture, before the shot is saved: crop, mark up, and annotate it before it becomes a tile.",
      howTo: [
        "Take a screenshot; the editor opens automatically over the capture.",
        "Pick a tool from the toolbar: crop, pen, marker, line, arrow, rectangle, ellipse, text, step number, pixelate, focus area, or hand.",
        "Drag the edges or corners of the frame to crop.",
        "Right-click a rectangle, ellipse, line, arrow or pixelated area to duplicate or delete it; a focus area can only be deleted.",
        "Arrow keys move the selected shape, Ctrl+arrow resizes it, Shift takes bigger steps, ESC deselects.",
        "\"Resize\" changes the image's real pixel size; \"Show original\" (or V) shows the untouched capture while held, a tap locks it.",
        "Undo/redo steps back through edits.",
        "Edits reach the clipboard as you make them: half a second after the last change the edited image is copied, so you can paste it right away.",
        "The bottom bar saves the shot as a file in the format you pick, turns it into a button or starts a new screenshot.",
        "\"Close\" (also ESC) keeps the result; with unsaved edits it asks first.",
      ],
      whatHappens:
        "Every edit is one step in an undo/redo list capped by its own setting; pixelate always samples the original screenshot, never an already-edited layer, so blurring a spot never smears earlier marks. Snap-to-edges lines a shape up with the image border, its center and the other shapes while dragging, with a thin guide line; hold Alt to drag freely. A text label can get a background plate or an outline, so light text stays readable on a light background. Undo covers every step, including a duplicate, a delete, a resize and a run of arrow-key moves, which counts as one step. Once zoomed in, the hand tool, Space held down or the middle mouse button drags the view around for pixel-precise work. The crop tool shows a resize cursor at each handle and a move cursor inside the current selection.",
      settings: ["Screenshot editor, default tool, snap to edges, text background, default color: Expert menu → Capture → Screenshot editor. The editor remembers the last size you used for each tool. It also opens with the tool you used last, unless a fixed default tool is set.", "Mouse wheel on number fields: Expert menu → Capture → Both editors"],
    },
    {
      section: "capture",
      title: "Open a file in the editor",
      whatItIs: "Opens an existing picture or video in whichever editor its content calls for, instead of only ever reaching an editor through a fresh capture or recording.",
      howTo: [
        "Right-click the screenshot {icon:screenshot} or record {icon:record} button, choose \"Open a file in the editor\" and pick the file.",
        "Or right-click an image or video tile or library row and choose Edit image or Edit video.",
        "Edit as usual; saving asks where to put the result.",
      ],
      whatHappens:
        "The file's content decides which editor opens, not its extension: a video saved with a picture extension still opens the video editor. The editor always works on a copy, so the original file is never changed; the save dialog starts in the original file's own folder. A file past the configured size limit is rejected before it loads.",
      settings: ["Largest file to open: Expert menu → Capture → Both editors"],
    },
    {
      section: "capture",
      title: "Screen recording",
      id: "screenRecording",
      whatItIs: "Records a window, a monitor, or a dragged region as a video, next to the screenshot tool.",
      howTo: [
        "Click the record button {icon:record}.",
        "Pick a window, drag a region, or click empty desktop for the whole screen, in the same overlay as the screenshot tool.",
        "An optional countdown runs before the recording starts.",
        "A small control bar shows elapsed time and file size, two audio mute buttons, and Pause/Resume, Stop and Discard.",
        "Two optional global hotkeys and the tray menu can also stop or pause it while a recording runs.",
        "A window recording over all screens or the current screen also gets a button that switches its target without starting over.",
        "Stopping offers Save, Save as…, Copy, Edit or Discard.",
        "Right-click the record button and choose \"Manage recordings…\" to export or delete older recordings.",
      ],
      whatHappens:
        "The recording is always written as MP4 (H.264); the quality steps Small, High and Lossless trade file size for detail. A format setting can convert it afterward to WebM, AVI or GIF; GIF is capped at 30 seconds and has no sound. Every format other than MP4, and Lossless, needs ffmpeg, which the installer brings along; where it is missing, the app offers the download. A failed or cancelled conversion keeps the original recording. System audio and microphone record into separate tracks; the microphone track stays silent until you add it in the video editor. The two audio buttons in the bar mute a source at once and unmute it again; an unplugged microphone only ends its own track. Save and Copy use the recording folder; Edit opens the clip in the video editor, and closing the dialog without saving or copying still discards it. Drag the bar to move it anywhere; it then stops following the window for that recording and is never part of the video. If the recorded window closes, the recording stops and keeps what it has. A recording left behind by a crash or a forced shutdown is removed after a grace period. The recording folder row in the expert menu shows how many videos it holds and their size. \"Clean up…\" lists files older than 7, 30 or 90 days and deletes only after you confirm; \"Export selected\" converts the checked files in one go. By default, unused recordings are deleted after 90 days (0 keeps them forever). The optional keys overlay records key names and times, typed letters included, and skips a password field only when Windows reports it. For a recording with confidential input, turn on \"Shortcuts only\".",
      settings: ["Screen recording, countdown, bitrate, quality, format, recording folder and cleanup: Expert menu → Capture → Screen recording", "Cursor, audio sources and device, stop/pause hotkeys, tray entries, keys overlay, frame around a recorded region and its color: Expert menu → Capture → While recording", "Auto-remove broken recordings, clean up now: Expert menu → System & data → Troubleshooting"],
    },
    {
      section: "capture",
      title: "Camera recording",
      whatItIs: "Records the webcam into its own video, alone or next to a screen recording.",
      howTo: [
        "Turn on camera recording in the expert menu.",
        "Right-click the record button {icon:record} and choose \"Record camera\".",
        "In its window, check the live preview, pick the audio source, resolution and folder, then Start.",
      ],
      whatHappens:
        "The camera window, or its status dot in the recording bar, stays visible the whole time, so the camera never runs unnoticed. The camera goes into its own file and is never burned into a screen recording. Started during a screen recording, both share one start point, and the video editor then opens the screen recording with the camera laid on top as an overlay that can still be moved.",
      settings: ["Camera recording, default camera, resolution, frame rate: Expert menu → Capture → Camera recording"],
    },
    {
      section: "capture",
      title: "Video editor",
      whatItIs: "A small editor for a finished recording or any attached video, opened from the recording's own result dialog or a video prompt's right-click menu.",
      howTo: [
        "From a fresh recording: click More, then Video editor, in the result dialog.",
        "From any video already saved as a prompt: right-click it and choose \"Edit video\".",
        "Trim, crop, change speed, fade in or out, and resize by typed dimensions, a percentage or a quick preset.",
        "Add image, freeze-frame, text, pixelate or focus area overlays on the timeline.",
        "Export: a lossless fast path applies when only trimming; otherwise the whole clip re-encodes once.",
      ],
      whatHappens:
        "Nothing is processed until you export: the live preview plays your edits directly. A filmstrip and waveform give a zoomable overview of the whole clip for frame-exact navigation. The preview itself also zooms with the scroll wheel (up to 8×) and pans by dragging (the middle mouse button always, the left button once zoomed in) for placing a small overlay precisely on a large recording. The export destination is an editable field: type a path directly, or use \"Export to…\" to keep the file name and change only the folder; once an export has finished, \"Save as…\" moves that finished video to another place. Switching format only rewrites the extension. The \"Copy\" button next to the export field skips naming entirely. It exports into a temporary copy that cleans itself up after an hour and on the next launch, and places the finished file straight on the clipboard. A few ready-made export presets (unchanged quality, a smaller size for chat, an animated GIF) remember your last choice. The export format list offers MP4, WebM, GIF and AVI, each with a short note on what it is for. GIF never carries audio, warns once the clip runs past ten seconds, and is capped to a maximum resolution since it grows large fast. Undo/redo covers every editing step, capped by an adjustable limit. \"Add a video\" lays another clip on top as an overlay, and the Keys overlay turns the keys pressed during a recording into a keyboard picture. Right-click an overlay's row to duplicate or delete it. Dragging a crop box or overlay snaps to the frame edges, its center and other overlays; hold Alt to drag freely. A new overlay starts from the color, thickness and size last used for its kind. Unfinished cuts and overlays are remembered, so reopening the same recording offers to continue. The export file name has its own field; an existing file is never replaced, the export takes the next free name.",
      settings: ["Video editor, snap to edges: Expert menu → Capture → Video editor", "Remember unfinished video edits, and for how many days: Expert menu → Capture → Both editors"],
      sketches: ["editorTimeline"],
    },
    {
      section: "capture",
      title: "Text recognition (OCR)",
      id: "ocr",
      whatItIs: "Pulls text out of a screenshot or an image file using Windows' own on-device recognizer. It runs fully offline and nothing leaves the machine.",
      howTo: [
        "From the input bar: click the OCR button {icon:ocr} to recognize text in a screenshot, or right-click it to pick an existing image file.",
        "Review the result, then copy it or insert it directly.",
      ],
      whatHappens:
        "Recognition runs entirely on the device through Windows' own engine. The result dialog can auto-copy the text and join broken lines back into paragraphs, both switchable. A missing language pack opens the same dialog voice input uses; see \"Language packs\".",
      settings: ["Text recognition, auto-copy the result, join lines, recognition language: Expert menu → Capture → Text recognition"],
    },
    {
      section: "capture",
      title: "Color picker & pick history",
      id: "colorPicker",
      whatItIs: "The color picker is a pipette that reads any on-screen pixel as hex, RGB, or HSL.",
      howTo: [
        "Click the pipette icon in the input bar.",
        "Move the loupe to the pixel wanted; scroll or use plus/minus to zoom.",
        "Click to copy the color; right-click the pipette icon to reopen the last few picks.",
      ],
      whatHappens:
        "Nothing is ever saved to a tile automatically; a pick only reaches the clipboard. The pick history holds the last few colors and survives a restart, capped by its own setting.",
      settings: ["Color picker, zoom, format, pick-history length: Expert menu → Capture → Color picker"],
      tip: "Pick a color format (hex, RGB, or HSL) from the message that appears right after a pick.",
    },
    {
      section: "tiles",
      title: "Tile behavior & appearance",
      whatItIs: "These settings control how a tile reacts to the mouse and what it looks like.",
      howTo: [
        "Right-click a tile for its menu: edit, pin, hide, delete, and more.",
        "Right-click a tile twice quickly to open its editor straight away.",
        "Right-click a tile (or Ctrl+click) to select it; while selecting, every tile shows its checkbox.",
        "Drag a tile to any grid cell; drop it on an occupied cell to swap.",
        "Turn on hover lift, shadows, badges and similar looks in the expert menu.",
        "Pick a tile style there: bold, uppercase, monospace, italic, shadowed names, compact or gradient.",
      ],
      whatHappens: "These are interaction and looks only; none of them change what a tile copies.",
      settings: [
        "Tile menu, drag-to-reorder: Expert menu → Tiles → Tiles",
        "Dim until hovered, lift on hover: Expert menu → Tiles → Tiles",
        "Type badges, shadows, rounded corners, tile style (bold, uppercase, monospace, italic, shadowed names, compact, gradient): Expert menu → Tiles → Tile appearance",
      ],
    },
    {
      section: "tiles",
      title: "Media",
      whatItIs: "These settings control tiles that carry a video or an image.",
      howTo: [
        "Attach a video or image to a tile: file attach, paste, or drag and drop.",
        "Adjust playback and privacy-leaning looks in the expert menu.",
      ],
      whatHappens: "Video settings (autoplay, mute, loop, volume) apply to every video tile; blurring only changes how media looks, never what gets copied. The library and copy history also show a video's length on its thumbnail, and its resolution and file size in the tooltip; the main grid never shows either.",
      settings: ["Video controls, autoplay and loop, mute: Expert menu → Tiles → Media", "Accent border and rounded corners around media: Expert menu → Tiles → Media", "Duration/resolution/size in the library and copy history: Expert menu → Tiles → Media", "Fill in media info for older attachments: Expert menu → System & data → Troubleshooting"],
    },
    {
      section: "tiles",
      title: "Favorites",
      whatItIs: "A star marks a tile as a favorite for quick access later.",
      howTo: ["Use \"Add to favorites\" from a tile's right-click menu, or the star in the library.", "A favorite tile shows a small star in its top-right corner.", "Open the dedicated favorites view to see only starred tiles."],
      whatHappens: "Favorites get their own reorderable view, independent of wherever else the tile lives.",
      settings: ["Favorites view: Settings. Its own reordering: Expert menu → Tiles → Favorites"],
    },
    {
      section: "tiles",
      title: "Tooltips",
      whatItIs: "Hovering a tile shows its full text, length, and any private note.",
      howTo: ["Hover a tile and wait for the delay to elapse.", "Adjust the delay in the expert menu if needed."],
      whatHappens: "The tooltip auto-hides after its timeout even if the mouse stays put.",
      settings: ["Delay before showing: Expert menu → Tiles → Tooltips"],
    },
    {
      section: "appearance",
      title: "Appearance & themes",
      whatItIs: "Seven built-in themes cover System, Light, Dark and four accent styles: Programmer, Neon Violet, Ocean and Cyberpunk.",
      howTo: ["Pick a theme from Settings.", "Open the theme editor, once enabled, to adjust any theme's colors and save a custom one."],
      whatHappens: "\"System\" follows the OS light or dark setting live. A custom theme is saved separately; the built-in it was based on stays untouched.",
      settings: [
        "Theme choice: Settings",
        "Theme editor: Expert menu → Appearance → Visual",
        "Frosted dialogs, hidden scrollbars: Expert menu → Appearance → Visual",
      ],
      tip: "Switching custom themes off also removes the “Custom theme…” entry from the theme list.",
    },
    {
      section: "appearance",
      title: "Theme editor",
      whatItIs: "The theme editor adjusts any built-in theme's colors and saves the result as a new theme.",
      howTo: [
        "Open the theme editor: Expert menu → Appearance → Visual.",
        "Custom themes must be on.",
        "Pick a base theme, adjust colors, and save under a new name.",
      ],
      whatHappens: "A custom theme appears in the regular theme dropdown alongside the built-ins; deleting it does not affect the theme it was based on.",
      settings: [],
    },
    {
      section: "appearance",
      title: "Scaling & zoom",
      whatItIs: "Independent size sliders adjust different parts of the window separately.",
      howTo: ["Open Settings and find the scaling sliders.", "Adjust the overall UI scale or the icon scale independently."],
      whatHappens: "Each slider only affects its own part of the window: raising icon scale, for example, does not resize dialogs.",
      settings: ["Interface size: Settings", "Icon size: Expert menu → Appearance → Visual"],
    },
    {
      section: "library",
      title: "Library & search",
      whatItIs: "The library lists every tile as a searchable, filterable list, unlike the grid, which shows only what fits.",
      howTo: [
        "Click the library button {icon:library} in the toolbar to open it.",
        "Type to search, or use the type filter to narrow the list.",
        "Click a row to copy.",
        "The Close after copy box at the bottom of the library decides whether copying closes the library; it remembers your choice.",
        "Use its menu to edit, drag onto the grid, tag, or delete.",
        "Pin a search with the pin button in the field: it stays at the top of the recent searches, with its tag filter and sort order.",
        "The Tiles button next to Sort switches to large preview tiles.",
        "Pick two rows and click \"Compare\" in the selection bar to see both side by side with the differences marked.",
      ],
      whatHappens:
        "Search is typo-tolerant and, unless turned off, sorts by most-used or most-recently-used. In the comparison, either side's text can be taken over into the other. Tiles view behaves exactly like the list for clicks, menus and multi-select. An optional setting reads text out of attached pictures so the search finds it too; a hit found only there carries a \"found in image\" badge. That text is never shown, exported or synchronized, and its length is capped. Removing or replacing the picture clears it, and switching the setting off removes it from every tile, the trash included.",
      settings: [
        "Fuzzy search, type filter, recent searches: Expert menu → Library & copy history → Search",
        "Focus the search field on opening: Expert menu → Library & copy history → Search",
        "Entries per search list (library, Search everything, quick launcher): Expert menu → Library & copy history → Search",
        "Smart sort, multi-select for batch actions, image and video previews in the rows, tiles view: Expert menu → Library & copy history → Display",
        "Index text inside attached pictures for search: Expert menu → Library & copy history → Search",
      ],
    },
    {
      section: "library",
      title: "Search everything",
      whatItIs: "One search box for tiles, copy history, attachments, views, settings and this help, all at once.",
      howTo: [
        "Click the magnifier in the toolbar, or press Ctrl+K.",
        "Type at least two characters; hits appear in groups.",
        "Move with the arrow keys and press Enter, or click a hit.",
      ],
      whatHappens:
        "Enter does what fits the hit: it copies a tile or copy history entry, opens an attachment, switches to a view, opens and highlights a setting, or opens the help article. Each group shows a few hits and a \"Show … more\". With the box empty, recent searches are offered. ESC, a click outside, or the magnifier again closes it.",
      settings: ["Search everything: Expert menu → Library & copy history → Search"],
    },
    {
      section: "library",
      title: "Tags",
      whatItIs: "Tags are free-form labels assigned to tiles, used to filter the library.",
      howTo: [
        "Type a tag name in a tile's editor to add it, or pick an existing one.",
        "Use the tag filter row in the library to narrow the list.",
        "To archive a tile, choose \"Archive\" in its menu: it gets the archive tag and leaves the grid and the list until you show it again.",
      ],
      whatHappens: "Renaming, merging, or deleting a tag from the tag manager applies everywhere it is used, in one step. The archive is an ordinary tag with a reserved name: an archived tile still turns up in search and stays in exports and backups, and \"Unarchive\" in the same menu brings it back.",
      settings: ["Archive tag: Expert menu → Library & copy history → Display"],
    },
    {
      section: "library",
      title: "Copy history",
      whatItIs: "Copy history logs what was copied and when, in the app and, if switched on, in other apps.",
      howTo: [
        "Open the Library and choose the Copy history tab.",
        "Search, or filter by type and, while copies from other apps are recorded, by source.",
        "A time range narrows the recently copied list to today or the last 7 or 30 days.",
        "Delete a single row, or clear the whole copy history.",
      ],
      whatHappens: "Grouped view shows one row per tile (its most recent copy) instead of every event. Copy history older than the retention setting is dropped automatically (30 days for a new install, adjustable, 0 keeps everything). The copy history is one list; the sort field switches between Recently copied and Most used (top 20).",
      settings: ["Grouped view, retention: Expert menu → Library & copy history → Copy history", "Skip an entry that repeats the copy before it: Expert menu → Library & copy history → Copy history"],
    },
    {
      section: "library",
      title: "Copies from other apps",
      whatItIs: "When switched on, the copy history also records text copied anywhere on the system, marked as copied elsewhere.",
      howTo: [
        "Turn on \"Record copies from other apps\" in the expert menu.",
        "Copy text anywhere on the system as usual.",
        "Open Library → Copy history, set the source filter to \"Elsewhere\" and save the useful entries as tiles.",
      ],
      whatHappens:
        "Nothing is watched until it is turned on. Copies shorter than the minimum length are ignored, and the oldest of these entries drop once they pass their cap. With \"Skip likely passwords\" on, a copy that looks like a password (one line, no spaces, mixed letters, digits and symbols) is never added; it is a best guess, so it skips rather than risks keeping one. Switching the watcher off asks whether to keep or delete what was collected; closing that question keeps it. Clearing the inbox offers an undo button on its message.",
      settings: ["Skip likely passwords, minimum length, cap: Expert menu → Library & copy history → Copy history"],
    },
    {
      section: "system",
      title: "System",
      whatItIs: "System settings cover app-wide behavior that is not about any single tile.",
      howTo: ["Open the expert menu's System group for keyboard navigation, the data folder, taskbar progress and the video tools download."],
      whatHappens:
        "The update check in Settings → System never installs anything without asking first. Changing the data folder moves the existing database there. Allowing multiple instances takes a restart. Long jobs can fill the taskbar icon as a progress bar and show the time left.",
      settings: [
        "Keyboard navigation, data folder, taskbar progress, video tools download: Expert menu → System & data → System",
        "Allow multiple instances: Expert menu → System & data → Troubleshooting",
      ],
    },
    {
      section: "system",
      title: "Toolbar",
      whatItIs: "The toolbar settings control which buttons the top bar and the input bar show.",
      howTo: [
        "Right-click the top bar to show or hide its buttons; features themselves are switched in the expert menu.",
        "Hide the top bar or the input bar: right-click the gear or the top bar.",
      ],
      whatHappens: "Hiding a button only hides it; the feature behind it, if any, keeps working through its own hotkey or menu entry.",
      settings: ["Library button and double-click on the bar buttons: Expert menu → System & data → Toolbar; logo, title and bar buttons: right-click the top bar.", "Quick grid-size control and showing it on hover only, multiple views: Expert menu → Features → Views and quick grid"],
    },
    {
      section: "system",
      title: "Privacy",
      whatItIs: "These settings keep this window's contents out of screenshots, screen shares, and passers-by.",
      howTo: [
        "Turn on media blur in the expert menu, as needed.",
      ],
      whatHappens:
        "Screen-capture exclusion makes the window invisible to screenshot and recording tools, including its own, so if a capture comes out empty, check this setting first. A single tile can also be marked confidential; see \"Confidential tiles\".",
      settings: [
        "Exclude from screen capture, blur media: Expert menu → System & data → Privacy",
        "Mark a tile confidential, vault lock timeout: Expert menu → System & data → Privacy",
      ],
    },
    {
      section: "system",
      title: "Confidential tiles",
      whatItIs: "A confidential tile keeps its text encrypted with a vault password that only you know.",
      howTo: [
        "Turn on confidential tiles in the expert menu.",
        "Tick \"Confidential\" in a tile's editor and save.",
        "The first time, set a vault password and confirm that a lost password cannot be recovered.",
        "Later, enter the password once per session to view or copy those tiles.",
      ],
      whatHappens:
        "The password is never stored. Forgetting it loses those tiles for good, there is no recovery. After the lock timeout the tiles lock again. An export that contains confidential tiles asks whether to keep them encrypted or leave them out. They are never written to the sync folder and never get an editor draft. Resetting settings keeps the vault, so the tiles stay readable.",
      settings: ["Confidential tiles, lock after how many minutes: Expert menu → System & data → Privacy"],
      tip: "Write the vault password down somewhere safe before marking anything confidential.",
    },
    {
      section: "system",
      title: "Updates & security",
      whatItIs: "An optional update check looks for a newer version and only talks to the update server.",
      howTo: [
        "Answer Yes when the app asks on first launch whether it may check for updates, to be notified of a new version.",
        "Switch off Auto update in Settings to stop automatic checks; while it is on, the list beside it sets how often.",
        "Accept an offered update to download and install it with one click.",
      ],
      whatHappens:
        "On first launch the app asks whether it may check for updates; until you answer, it never goes online. The update check and, if that runtime is missing, a first-run download of the Microsoft WebView2 installer are the only network traffic the app starts on its own. A downloaded update is verified against a published checksum before it runs: a missing or mismatching checksum blocks the install instead of running an unverified file. Everything else waits for you. ffmpeg, used for video export and conversion, ships with an installed build, so it never touches the network for it. A portable build offers a one-time download only when a video feature needs it, never automatically and never in the background. Voice input sends audio to Microsoft's online speech recognition, only after you agree. A language pack for text recognition or voice input comes from Windows Update, only when you start the install. After an update, a small notice links to what changed. The About window, next to \"Check for updates\" in Settings, shows the version, the full history of changes and every bundled open-source component with its license.",
      settings: ["Auto update and how often it checks: Settings"],
    },
    {
      section: "system",
      title: "Command line",
      whatItIs: "The program file also works as a command line tool, for scripts and quick jobs without opening the window.",
      howTo: [
        "Open a command prompt and run the program file with --help to see every command.",
        "List, read, copy, add, edit, delete and restore tiles, manage tags and views, search, export, import, and make or restore backups.",
        "Add --json for machine-readable output, and --yes to any command that changes something.",
      ],
      whatHappens:
        "If the app is already running, the command goes to it and its answer is printed; otherwise the command runs on its own without a window. Only one copy ever touches the data at a time. Without --yes a changing command only says what it would do. Folder locations, the vault, the backup password and recording copies from other apps cannot be changed from here, only in the app.",
      settings: [],
    },
    {
      section: "system",
      title: "Demo mode",
      whatItIs: "Demo mode swaps the whole window to a sample library, so a demonstration or a screenshot never shows anything real.",
      howTo: [
        "Turn on demo mode in the expert menu; the app restarts into the sample library.",
        "Show or try anything; a marker stays visible the whole time.",
        "Turn it off to go back; the app asks first and restarts into your own data.",
      ],
      whatHappens:
        "The sample library is built fresh each time and covers most features: tiles with and without a picture, a tile with rotation, a placeholder, tags, a favorite, two views, a folder, an attachment and a video. Your own data is never opened or changed while it runs. Anything changed during the demo is discarded when it ends.",
      settings: ["Demo mode: Expert menu → System & data → Troubleshooting"],
    },
    {
      section: "data",
      title: "Where your data lives",
      whatItIs:
        "Everything lives in one local SQLite database plus its small companion file.",
      howTo: [
        "Installed: the data folder is under AppData, shown and changeable in the expert menu.",
        "Portable: the data folder sits next to the exe, so it all fits on a USB stick.",
      ],
      whatHappens:
        "Moving from portable to installed, or back, is detected automatically and offered as a one-time migration: existing data is never silently left behind. Pointing the data folder at a cloud-synced folder shows a one-time warning, and a small lock file then tracks which computer uses the database. Another computer that finds a fresh lock opens everything read-only and says so in a banner; a lock nobody refreshed for a while is taken over.",
      settings: ["Data folder: Expert menu → System & data → System", "Data folder lock: Expert menu → System & data → Troubleshooting"],
    },
    {
      section: "data",
      title: "Backup manager",
      whatItIs: "The backup manager creates encrypted, compressed snapshots of the entire library and settings.",
      howTo: [
        "Open the Backup manager from Settings.",
        "Turn on automatic backups and pick an interval, or create one manually.",
        "Save a backup to a file of your choice or import one from a file with the two buttons beside Create backup.",
        "Restore from the snapshot list, choosing data only, settings only, or both.",
      ],
      whatHappens:
        "Retention tiers (daily, weekly, monthly, yearly) thin out older snapshots automatically instead of keeping every one forever. A backup is either whole or absent; a failed one leaves nothing behind. The same window also has a storage breakdown of what is taking up disk space. Each snapshot can open its contents: counts, the newest change inside it, the version that made it, and every tile it holds, searchable and sortable, with a button to restore one tile on its own. One row per kind of kept data shows how much it holds and how long it is kept, jumps to its setting and can clean it up on the spot. The kinds are trash, copy history, copies from other apps, recordings, temporary exports and edit states.",
      settings: [
        "Interval, retention tiers, password, restore scope: Backup manager (its own window, opened from Settings)",
      ],
    },
    {
      section: "data",
      title: "Import & export",
      whatItIs: "Plain CSV, TXT or JSON export covers text alone; a full backup file covers everything.",
      howTo: [
        "Use Import or Export in Settings → Data.",
        "Pick a format from the dropdown next to the Export button (JSON keeps the most); a CSV or JSON import shows a name and duplicate status per item first.",
        "A full backup is restored from its own window.",
        "Confirm to actually import, or cancel and nothing changes.",
        "Import → \"From folder…\" in Settings turns a folder of .md or .txt files into tiles, one per file, after a preview with a checkbox per file.",
      ],
      whatHappens: "CSV and TXT round-trip text alone; JSON also carries the full rotation texts of a tile with rotation, tags, notes, hotkey, and color, none of which fit cleanly into the other two. Images, files, layout, and settings need the full backup format instead. Import decides CSV or JSON by the file's own content, never its extension. A folder import only reads: nothing is moved or deleted. The file name becomes the name, each sub-folder becomes a tag, and an unreadable file is grayed out with the reason.",
      settings: [
        "Switch import and export on or off: Expert menu → System & data → Data & maintenance",
        "Folder import depth and file limit: Expert menu → System & data → Data & maintenance",
      ],
    },
    {
      section: "data",
      title: "Markdown folder sync",
      whatItIs:
        "Folder sync mirrors every text tile as its own readable Markdown file in a chosen folder.",
      howTo: [
        "Turn it on and pick a folder in the expert menu.",
        "Edit a tile normally, or edit its Markdown file directly; both directions stay in sync.",
      ],
      whatHappens:
        "The Markdown files are plain text: anyone with access to the folder can read them, so keep sensitive tiles out of a shared or cloud folder. A confidential tile is never written there, and a file never changes one. A tile with an attached file, image or PDF syncs as a reference to its path, so another computer needs the same path; a pasted screenshot stays in the database only. Views and folders named in a file are created here if missing, and a view renamed on another computer is renamed here too. The file name is the tile's fixed id: renaming a file by hand makes a second tile. Deleting a file does not delete its tile, which is written again on the next round. Deleting a tile removes its file and moves the tile to the trash on every other computer using the folder. If both sides changed, one wins and the losing text is kept as a version of the tile or as a conflict copy beside the file. Files this app did not write are never changed or deleted, and a file over 8 MB is skipped. Picking another folder asks whether to delete the old one; only files this app wrote are removed, and nothing at all if anything unknown is in it. An unreachable folder pauses the sync after a few failed rounds.",
      settings: ["Markdown folder (beta), sync folder: Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "data",
      title: "Trash & restoring tiles",
      whatItIs: "A deleted tile moves to the trash first, not straight to permanent deletion.",
      howTo: [
        "Delete a tile as normal: it goes to the trash, and Undo brings it back.",
        "Open the trash from the expert menu's data group.",
        "Restore a tile from there, or let retention clear it automatically.",
      ],
      whatHappens:
        "Retention is set in days; zero means keep forever. Restoring places the tile in the active view's first free cell, not necessarily its original view or cell. It does not bring back a floating button, folder membership or hotkey it had before deletion; those need setting up again.",
      settings: ["Trash retention: Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "data",
      title: "Undo button",
      whatItIs: "Some actions outside the trash offer a one-step undo right on their confirmation message.",
      howTo: [
        "Delete a view, clear the copy history, delete a tag, or run a batch delete or tag change.",
        "Click Undo on the message that appears.",
      ],
      whatHappens:
        "Only the latest action can be undone; a newer one replaces it, and the button goes away with the message. With the undo time set to Off, deleting a view or merging tags asks first instead.",
      settings: ["Undo button, how long the message stays: Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "data",
      title: "Duplicate finder",
      whatItIs: "The duplicate finder flags tiles that are textually very similar without deleting anything.",
      howTo: [
        "Click \"Find duplicates\" in the library.",
        "Review each flagged pair side by side.",
        "Keep one, or mark the pair as not a duplicate to stop it being flagged again.",
      ],
      whatHappens:
        "Matching compares each tile's main text (or its name, if it has no text) by similarity, not by exact match; the threshold controls how close counts as close enough. A tile with rotation is only compared on its first text. The only way in is the \"Find duplicates\" button in the library; the expert menu only holds its on/off switch and the threshold.",
      settings: ["Duplicate finder, similarity threshold: Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "data",
      title: "Usage statistics",
      whatItIs: "A small window with the number of prompts, the copies of the last 7 days, the most used and the never used prompts.",
      howTo: [
        "Switch on \"Show statistics\" in the expert menu under \"Data & maintenance\".",
        "Open Settings and click \"Usage statistics\".",
        "Pick how many prompts the most used list shows: 5, 10 or 20.",
        "Click a never used prompt to find it in the library.",
      ],
      whatHappens:
        "Everything stays on this computer. The export button saves the full numbers, including daily counts and tool use, as CSV, TXT or JSON.",
      settings: [],
    },
    {
      section: "data",
      title: "Data maintenance",
      whatItIs: "Data maintenance gathers housekeeping tools in the expert menu.",
      howTo: [
        "Open Expert menu → System & data → Data & maintenance.",
        "From there, open the trash, export or import the settings alone, or open \"Recent messages\".",
      ],
      whatHappens:
        "\"Recent messages\" keeps a short list of error messages that flashed by, with time and place; copy or clear the list from its window. The settings alone can be saved to a file and loaded again. Before a reset or a delete-everything, the app can take a backup first.",
      settings: [
        "Trash retention, recent messages, settings export and import, undo button: Expert menu → System & data → Data & maintenance",
        "Take a backup before a reset or a delete-everything: Expert menu → System & data → Data & maintenance",
        "How much the recent messages keep: Expert menu → System & data → Troubleshooting",
      ],
    },
  ],
  de: [
    {
      section: "basics",
      title: "Erste Schritte",
      whatItIs: "Eine Kachel ist ein gespeicherter Eintrag im Raster; ein Klick darauf kopiert ihren Text in die Zwischenablage.",
      howTo: [
        "Doppelklick auf eine leere Rasterzelle oder Text ins Feld unten tippen und „Speichern“ drücken erstellt eine Kachel.",
        "Text eingeben oder einfügen, dann Speichern.",
        "Klick auf eine beliebige Kachel kopiert sie.",
      ],
      whatHappens:
        "Drei Beispielkacheln sind beim ersten Start bereits da: eine E-Mail mit Platzhaltern, ein Bild und eine Markdown-Vorlage. Ein Button über dem Raster entfernt sie mit einem Klick, sobald sie nicht mehr gebraucht werden.",
      settings: [],
    },
    {
      section: "basics",
      title: "Einstellungen",
      whatItIs: "Das Einstellungsfenster enthält die alltäglichen Entscheidungen; alles Optionale liegt im Expertenmenü, das sich im selben Fenster öffnet.",
      howTo: [
        "Auf das Zahnrad in der oberen Leiste klicken, um es zu öffnen.",
        "Aussehen: Design, Sprache, Größe der Oberfläche, Schriftart und Größe der Kacheln, Kurzinfos und die Favoriten-Ansicht.",
        "Verhalten: immer im Vordergrund, beim Schließen im Hintergrund weiterlaufen, alle Tastenkürzel, mit Windows starten und im Hintergrund starten.",
        "Daten: Import, Export, die Sicherungen und die Nutzungsstatistik.",
        "System: das Info-Fenster, die Update-Suche und automatische Updates.",
        "Eine portable Kopie beendet sich beim Schließen immer, statt im Hintergrund weiterzulaufen.",
      ],
      whatHappens:
        "Jede Änderung gilt sofort, ohne Speichern-Button. „Automatisch“ als Sprache folgt Windows und fällt auf Englisch zurück. Zurücksetzen stellt jede Einstellung auf die Vorgabe zurück und behält die Kacheln; Löschen entfernt alle Daten und fragt vorher nach.",
      settings: [],
    },
    {
      section: "basics",
      title: "Einführung",
      whatItIs: "Eine Einführung in vier Schritten, die direkt nach dem Erstellen der Beispiel-Kacheln einmal automatisch angezeigt wird.",
      howTo: [
        "Beim ersten Start: öffnet sich direkt nach dem Anlegen der Beispiel-Prompts automatisch.",
        "Später: über die Aktion „Einführung erneut zeigen“ im Expertenmenü erneut öffnen.",
        "←/→ oder die Punkte wechseln zwischen den Schritten, Enter fährt fort.",
        "ESC oder „Überspringen“ beendet die Einführung jederzeit; ESC zeigt dazu eine Meldung mit Rückgängig, die sie wieder öffnet.",
      ],
      whatHappens:
        "Im letzten Schritt lassen sich der Screenshot-Editor, die Farbauswahl oder die Texterkennung direkt aktivieren; dabei wird jeweils die ausgelieferte Standardeinstellung angezeigt. Gespeichert wird erst, wenn die Einführung tatsächlich abgeschlossen ist, und nur für Schalter, die wirklich geändert wurden. Ein beim ersten Start erscheinender Hinweis zum Datenordner oder zu WebView2 hat immer Vorrang; die Einführung wartet einfach, bis sie an der Reihe ist.",
      settings: ["Beim ersten Start automatisch öffnen: Expertenmenü → System & Daten → System"],
    },
    {
      section: "basics",
      title: "Das Raster",
      whatItIs: "Das Raster des Hauptfensters ist eine frei platzierbare Tafel für Kacheln, mit eigener Größe pro Ansicht.",
      howTo: [
        "Eine Kachel in eine beliebige leere Zelle ziehen.",
        "Eine Kachel auf einer besetzten Zelle loslassen, um beide zu tauschen.",
        "Eine Kachel auf einen Ansichts-Tab über dem Raster ziehen, um sie stattdessen in diese Ansicht zu verschieben.",
        "Beim Ziehen auf einen Ansichts-Tab Strg gedrückt halten, um die Kachel dorthin zu kopieren, statt zu verschieben, sodass sie an beiden Stellen bleibt.",
        "Die Rastergröße per Rechtsklick auf einen Ansichtsreiter oder eine freie Stelle im Raster unter Rastergröße ändern.",
      ],
      whatHappens:
        "Eine Kachel behält ihre Zelle, bis sie bewegt wird; das Raster ordnet sich nie selbst neu. Das Verkleinern löscht keine außerhalb liegenden Kacheln; sie bleiben zugewiesen und erscheinen beim Vergrößern wieder. Die Standardgröße von 20x20 ist nur ein Startwert; die Limit-Einstellung im Expertenmenü hebt sie auf bis zu 200x200 an.",
      settings: ["Schnelle Rastergrößen-Steuerung, Rasterrahmen, maximale Rastergröße: Expertenmenü → Funktionen → Ansichten und Schnellraster"],
    },
    {
      section: "basics",
      title: "Kacheln anlegen",
      whatItIs: "Über reinen Text hinaus kann eine Kachel eine angehängte Datei, einen Screenshot oder eine gepickte Bildschirmfarbe tragen.",
      howTo: [
        "Dateianhang: ein Klick auf die Kachel kopiert die Datei selbst, nicht den Text.",
        "Screenshot-Tool: einen Bereich oder ein ganzes Fenster direkt in eine neue Kachel erfassen.",
        "Farbpipette: einen beliebigen Bildschirmpixel picken; kopiert als Hex, RGB oder HSL.",
        "Ziehen und Ablegen: eine Datei oder ein Bild direkt auf das Raster ziehen.",
      ],
      whatHappens: "Die angehängte Datei, der Screenshot oder die Farbe wird zum Inhalt der Kachel; ein späteres Kopieren liefert diese Datei, dieses Bild oder diese Farbe, nie Text. Ein Anhang aus reinem Text, Markdown, JSON oder CSV zeigt seine ersten Zeilen direkt im Editor unter der Dateizeile, immer als reiner Text.",
      settings: ["Dateianhang, Kopie einer angehängten Datei behalten und mit der Kachel löschen, Textvorschau für Anhänge, Einfügen, Ziehen und Ablegen: Expertenmenü → Funktionen → Anhänge und Eingabe", "Screenshot-Tool: Expertenmenü → Aufnehmen → Screenshot", "Farbpipette: Expertenmenü → Aufnehmen → Farbpipette"],
    },
    {
      section: "basics",
      title: "Der Editor",
      whatItIs: "Der Editor ist im Kern ein einfaches Textfeld; einige Funktionen wie Markdown-Vorschau und private Notizen verleihen ihm Struktur und Versionen.",
      howTo: [
        "Editor einer Kachel öffnen: Doppelklick auf die Kachel oder Bearbeiten in ihrem Menü.",
        "Markdown-Vorschau, Syntax-Hervorhebung, Versionen oder private Notizen im Expertenmenü anpassen.",
      ],
      whatHappens:
        "Die Versionen speichern jede Änderung: Wiederherstellen holt eine frühere zurück, und der Button daneben behält diese Fassung stattdessen als neue Kachel. Die Markdown-Vorschau erscheint neben oder anstelle des Textfelds und ändert nie den gespeicherten Inhalt. Private Notizen werden nie mit dem Text kopiert oder exportiert. Der offene Text wird alle paar Sekunden als Entwurf gespeichert; schließt sich das Fenster oder die App vor dem Speichern, bietet der nächste Start ihn einmal wieder an. Eine vertrauliche Kachel bekommt nie einen Entwurf. „Formatiert kopieren“ legt zusätzlich die gerenderte Fassung in die Zwischenablage, sodass Word oder eine Mail-App die Formatierung zeigt. Wer eine neue Kachel speichert, die einer vorhandenen sehr ähnelt, sieht einen Hinweis, gespeichert wird trotzdem.",
      settings: ["Markdown-Vorschau, Syntax-Hervorhebung, Versionen, private Notizen: Expertenmenü → Funktionen → Editor", "Textfeld in dicktengleicher Schrift: Expertenmenü → Funktionen → Editor", "Formatiert kopieren, Live-Längenzähler, Verwerfen bestätigen, Entwurf automatisch sichern und wie oft: Expertenmenü → Funktionen → Editor", "Hinweis auf ähnliche Prompts beim Speichern: Expertenmenü → System & Daten → Daten & Wartung"],
    },
    {
      section: "basics",
      title: "Das Expertenmenü",
      whatItIs: "Das Expertenmenü hält jede optionale Funktion hinter einem eigenen Schalter mit ihren Parametern.",
      howTo: [
        "Einstellungen, dann das Expertenmenü öffnen.",
        "Einen Tab wählen, oder im Suchfeld tippen, um alle Tabs auf einmal zu durchsuchen.",
        "„Nur geänderte“ einschalten, um nur das zu sehen, was an den Standardwerten angepasst wurde.",
      ],
      whatHappens:
        "Eine standardmäßig deaktivierte Funktion trägt ein kleines Punkt-Badge an ihrer Beschriftung, mit einer erklärenden Kurzinfo. Zurücksetzen funktioniert auf drei Ebenen: das eigene Rückgängig-Symbol eines Werts, das Zurücksetzen-Symbol einer ganzen Gruppe in ihrer Überschrift, oder alles auf einmal über die allgemeine Zurücksetzen-Aktion. Die Suche findet auch Einstellungen außerhalb des Expertenmenüs, etwa im Einstellungsfenster oder im Fenster der Sicherungen, mit einem Hinweis, wo sie liegen; ein Klick öffnet diese Stelle.",
      settings: [],
    },
    {
      section: "basics",
      title: "Tastaturnavigation",
      whatItIs: "Pfeiltasten bewegen einen Fokusring über das Raster für rein tastaturbasiertes Kopieren.",
      howTo: [
        "Die Pfeiltasten bewegen den Fokusring, Enter oder Leertaste kopiert.",
        "Tab springt wie gewohnt weiterhin durch Buttons und Felder überall sonst in der App.",
      ],
      whatHappens: "Der Fokusring blendet sich kurz nach Fokusverlust des Fensters aus, damit er nicht auf einer Kachel verweilt, sobald eine andere App genutzt wird.",
      settings: ["Tastaturnavigation, Ausblend-Verzögerung: Expertenmenü → System & Daten → System"],
    },
    {
      section: "basics",
      title: "Häufige Fragen",
      whatItIs: "Dieser Abschnitt beantwortet die am häufigsten auftauchenden Fragen.",
      howTo: [
        "„Mein Screenshot ist leer“: der Bildschirmaufnahme-Ausschluss funktioniert wie vorgesehen; bei Bedarf im Expertenmenü ausschalten.",
        "„Die App startet nicht“: sie benötigt die Microsoft-WebView2-Laufzeit, vom Installer automatisch angeboten.",
        "„Ich finde eine Einstellung nicht“: das Suchfeld im Expertenmenü durchsucht alle Tabs auf einmal.",
        "„Ein Tastenkürzel bewirkt nichts“: eine andere App hält diese Kombination möglicherweise bereits; der Editor erklärt, warum.",
      ],
      settings: [],
    },
    {
      section: "features",
      title: "Kopieren",
      whatItIs: "Kopieren ist der Ablauf zwischen dem Klick auf eine Kachel und dem Eintreffen ihres Texts in der Zwischenablage.",
      howTo: [
        "Einmaliger Klick auf eine Kachel kopiert sie.",
        "Schneller zweiter Klick oder Enter fügt den Text zusätzlich ein, falls Auto-Einfügen aktiv ist.",
        "Strg-Ziehen einer Kachel auf ein anderes Fenster legt ihren Text direkt dort ab.",
      ],
      whatHappens:
        "Eine kurze Blase oder ein Aufblinken bestätigt das Kopieren. Verketten fasst mehrere nacheinander angeklickte Kacheln zu einem Eintrag in der Zwischenablage zusammen, statt dass jede die vorige überschreibt. Der Ketten-Button in der oberen Leiste startet eine Kette, der nächste Tipp beendet sie; „Kette hier starten“ im Rechtsklickmenü einer Textkachel beginnt eine mit dieser Kachel. Eine Kette bleibt beim Wechsel in eine andere App bestehen; Strg+V im App-Fenster beendet sie. Beim Verketten lässt sich ein Schritt in der Leiste über dem Raster ziehen, um ihn umzusortieren, oder per Rechtsklick entfernen. Ein roter Schritt braucht einen Platzhalter, den kein früherer Schritt füllt; die Kette läuft trotzdem. Verketten ist nicht Rotation: Rotation ändert, was eine einzelne Kachel von Kopie zu Kopie enthält (siehe Rotation).",
      settings: [
        "Verketten und der Kettenstart im Kachelmenü: Expertenmenü → Funktionen → Kette",
        "Was zwischen zwei verketteten Schritten steht: Expertenmenü → Funktionen → Kette",
        "Kettenmodus nach dem Kopieren eingeschaltet lassen: Expertenmenü → Funktionen → Kette",
        "Auto-Einfügen bei Doppelklick: Expertenmenü → Funktionen → Kopieren",
        "Eigene Tastenkürzel pro Kachel, Enter nach Auto-Einfügen, Drag-out, Fenster nach dem Kopieren ausblenden, Größe der Kopierblase: Expertenmenü → Funktionen → Kopieren",
        "Welche Taste eine Kachel herauszieht und ob diese Taste herauszieht oder sie verschiebt: Expertenmenü → Funktionen → Kopieren",
      ],
      sketches: ["flowToTarget", "chainSteps"],
    },
    {
      section: "features",
      title: "Platzhalter",
      whatItIs:
        "Platzhalter-Markup im Text einer Kachel öffnet ein Ausfüll-Popup, bevor der fertige Text die Zwischenablage erreicht.",
      howTo: [
        "{?PlatzhalterName} irgendwo in den Text eintippen.",
        "Oder unter dem Textfeld auf „Platzhalter einfügen“ klicken: das öffnet den Platzhalterbereich.",
        "Für einen sich selbst ausfüllenden Wert einen dynamischen Chip unter dem Textfeld anklicken statt zu tippen.",
        "Klick auf die Kachel zum Kopieren: das Ausfüll-Popup öffnet sich automatisch.",
        "Pro Platzhalter einen Wert eingeben, bei Auswahl-Platzhaltern einen wählen, dann bestätigen.",
      ],
      whatHappens:
        "Nur der ausgefüllte Text erreicht die Zwischenablage; das rohe {?...}-Markup wird nie kopiert. Ein dynamischer Platzhalter (Datum, Uhrzeit, Zwischenablage, UUID) wird mit einem @ davor geschrieben, {?@date}, und füllt sich ganz ohne Popup selbst aus. Das Popup kann sich den zuletzt eingetippten Wert eines Platzhalters merken und ihn wieder anbieten. Eine Liste fester Werte im Expertenmenü wird für einen gleichnamigen Platzhalter vorgeschlagen; sie wird immer nur vorgeschlagen, nie von selbst eingesetzt. Prompts mit der älteren Schreibweise {#{Name}#} werden automatisch umgeschrieben.",
      settings: ["Dynamische Platzhalter, Auswahl-Platzhalter, Auswahl merken, eingetippte Werte merken, Liste fester Werte, Datums- und Zeitformat: Expertenmenü → Funktionen → Platzhalter"],
      sketches: ["placeholderAnatomy"],
    },
    {
      section: "features",
      title: "Tastenkürzel",
      whatItIs:
        "Zwei unabhängige Arten von Tastenkürzeln: eine globale Kombination, die den schnellen Launcher öffnet, und eine Kombination pro Kachel.",
      howTo: [
        "Das globale Tastenkürzel in Einstellungen → Tastenkürzel, Zeile „Schnellstarter“, festlegen (Standard: Strg+Alt+P).",
        "Einer Kachel im Editor ein eigenes Tastenkürzel geben.",
        "Wählen, ob er nach dem Kopieren auch einfügen soll.",
        "„Tastenkürzel“ in den Einstellungen öffnet eine Liste.",
        "Oben stehen globale Tastenkürzel und die der Kacheln, durchsuchbar, darunter die Tasten jedes Fensters.",
        "Jede Zeile wird dort direkt umbelegt oder gelöscht.",
        "Ist eine Kombination bereits vergeben, erklärt das Feld den Grund, statt sie stillschweigend zu akzeptieren.",
      ],
      whatHappens:
        "Eine Kombination muss Strg oder Alt enthalten; Shift allein oder eine bloße Taste wird abgelehnt, damit normales Tippen nie verschluckt wird. Das App-Tastenkürzel gewinnt bei einem Konflikt immer; bei Kacheln behält die erste in der Liste eine geteilte Kombination. Der Import einer Sicherung mit kollidierendem Tastenkürzel verwirft das verlierende, statt den Import abzubrechen, und die Zusammenfassung nennt die Anzahl der verworfenen. Eine Kachel mit eigenem Tastenkürzel zeigt ein kleines Tastatursymbol.",
      settings: [
        "Globales Tastenkürzel, alle Tastenkürzel: Einstellungen → Tastenkürzel",
        "Eigene Tastenkürzel pro Kachel, Einfügen nach Kopieren: Expertenmenü → Funktionen → Kopieren",
        "Tastenkürzel-Abzeichen: Expertenmenü → Kacheln → Kacheln",
      ],
    },
    {
      section: "features",
      title: "Rotation",
      whatItIs: "Eine Kachel kann eine Liste von Texten statt nur einem tragen; jede Kopie liefert den nächsten Eintrag.",
      howTo: [
        "Im Editor einer Kachel „Rotation“ über dem Textbereich einschalten.",
        "Die Liste der Rotationstexte ersetzt dann das normale Textfeld.",
        "Mit „+ Rotationstext“ weitere Rotationstexte hinzufügen; Symbole daneben sortieren um oder entfernen sie.",
        "Reihenfolge (der Reihe nach) oder Mischen (zufällig, ohne Wiederholung) wählen.",
        "Die Kachel wie gewohnt kopieren: sie liefert immer den nächsten Eintrag, ohne zu fragen, welchen.",
      ],
      whatHappens:
        "Ein kleines Badge wie „2/5“ zählt auf einer Raster-Kachel, einem schwebenden Button und einem Mitglied eines Desktop-Ordners hoch; ein Raster-Ordner-Mitglied zeigt nur sein Dateityp-Symbol. Im Reihenfolge-Modus zeigt es die Position, im Misch-Modus zählt es die Ziehungen im aktuellen Durchlauf und beginnt wieder bei 1, sobald jeder Text einmal drankam. Der Zählerstand übersteht Neustarts und vollständige Sicherungen; ein CSV- oder TXT-Import startet die neue Kachel beim ersten Text. Wird die Rotation ausgeschaltet, bleiben alle Texte erhalten, genutzt wird nur der erste, bis sie wieder an ist. Mit „Beim Einfügen rotieren“ wechselt eine Kachel auch bei jedem echten Einfügen zum nächsten Text, nicht nur beim Kopieren.",
      settings: ["Rotation, Standard Reihenfolge oder Mischen, Badge, maximale Rotationstexte, beim Einfügen rotieren: Expertenmenü → Funktionen → Rotation. Der Schalter pro Kachel steht im Editor unter „Erweiterte Optionen“."],
    },
    {
      section: "features",
      title: "Schwebende Buttons",
      whatItIs: "Schwebende Buttons sind kleine, immer im Vordergrund liegende Kopier-Buttons außerhalb des Hauptfensters, über jeder anderen App.",
      howTo: [
        "Rechtsklick auf eine Kachel, dann „Schwebenden Button anzeigen“, um sie als schwebenden Button anzuheften.",
        "Den schwebenden Button ziehen, um ihn zu bewegen; Rechtsklick für Größe, Bearbeiten oder Entfernen.",
      ],
      whatHappens: "Positionen bleiben nach Neustarts erhalten. Ein Button, dessen gespeicherter Bildschirm nicht mehr angeschlossen ist, öffnet sich auf dem Hauptbildschirm, als geparkt markiert, und springt von selbst zurück, sobald dieser Bildschirm wieder da ist. Eine Aktion im Expertenmenü holt alle schwebenden Fenster auf einmal auf den Hauptbildschirm.",
      settings: ["Schwebende Buttons: Expertenmenü → Funktionen → Schwebende Buttons & Ordner", "Lange geparkte Buttons ausblenden, alle zurückholen: Expertenmenü → Funktionen → Schwebende Buttons & Ordner"],
    },
    {
      section: "features",
      title: "Ordner",
      whatItIs: "Dieselbe Geste gruppiert an zwei Stellen: ein Element auf ein gleichartiges ziehen, um es in einen Ordner aufzunehmen.",
      howTo: [
        "Einen schwebenden Button auf einen anderen ziehen, um einen Ordner auf dem Desktop zu bilden.",
        "Im Hauptfenster eine Kachel auf eine andere ziehen, um einen Ordner im Raster zu bilden.",
        "Ein Mitglied wieder herausziehen, um es freizugeben.",
      ],
      whatHappens:
        "Ein Ordner liegt dort, wo er angelegt wurde, im Raster oder auf dem Desktop. Ein Desktop-Ordner zeigt bis zu vier Vorschauen seiner Mitglieder und eine Anzahl; ein Raster-Ordner zeigt, wie viele Kacheln er enthält, und einen Farbpunkt pro Kachel. Das Herausziehen eines Mitglieds gibt es frei, und der Ordner löst sich auf, sobald nur noch eines übrig ist. Eine Kachel gehört immer nur zu einem Raster-Ordner gleichzeitig.",
      settings: ["Ordner: Expertenmenü → Funktionen → Schwebende Buttons & Ordner"],
      sketches: ["folderTypes"],
    },
    {
      section: "features",
      title: "Infobereich-Menü",
      whatItIs: "Das Rechtsklick-Menü des Infobereich-Symbols bietet schnellen Zugriff, ohne das Hauptfenster zu öffnen.",
      howTo: ["Rechtsklick auf das Infobereich-Symbol.", "Einen Eintrag aus „Zuletzt kopiert“ oder „Favoriten“ wählen, um ihn sofort zu kopieren."],
      whatHappens: "Das Menü kopiert direkt aus dem Infobereich; das Hauptfenster muss dafür nicht geöffnet werden. Das Menü erscheint als abgerundetes Fenster im Design der App; ausgeschaltet kommt das schlichte Windows-Menü. Tippen ins Suchfeld des Infobereich-Menüs öffnet den Schnellstarter mit dem Getippten.",
      settings: ["Infobereich-Menü im Windows-11-Stil, Suchfeld, Bereich für zuletzt Kopiertes, Bereich für Favoriten, Anzahl der Einträge: Expertenmenü → Funktionen → Infobereich-Menü"],
    },
    {
      section: "features",
      title: "Schneller Launcher",
      whatItIs:
        "Der schnelle Launcher ist ein kleines, immer im Vordergrund liegendes Popup, das beim Tippen Name und Text jeder Kachel durchsucht: ein einfacher Text-Treffer, kein unscharfer Abgleich. Ein Treffer im Namen steht über einem Treffer im Text, aber ein Tippfehler findet nichts.",
      howTo: ["Strg+Alt+P, oder das konfigurierte Tastenkürzel, von überall drücken.", "Tippen zum Suchen, dann Klick oder Enter auf einem Ergebnis zum Kopieren."],
      whatHappens: "Der Launcher schließt sich direkt nach dem Kopieren selbst, sodass er nie auf dem Bildschirm bleibt. Eine Zeile unten nennt die Tasten.",
      settings: [],
    },
    {
      section: "features",
      title: "Mehrere Ansichten",
      whatItIs:
        "Eine Ansicht ist eine Rasterseite mit eigener Größe und Anordnung.",
      howTo: [
        "Klick auf Plus neben den Ansichts-Tabs fügt eine Ansicht hinzu.",
        "Rechtsklick auf einen Tab zum Umbenennen, Größenändern, Umfärben oder Löschen.",
        "Einen Tab mit „weiter nach links“ oder „weiter nach rechts“ aus seinem Menü verschieben, oder ihn auf einen anderen Tab ziehen.",
      ],
      whatHappens: "Eine Kachel sitzt normalerweise in genau einer Ansicht (wird sie woanders abgelegt, wandert sie dorthin), außer „Prompt in mehreren Ansichten“ ist aktiv. Das Löschen einer Ansicht löscht nicht die dort platzierten Kacheln; eine auch anderswo gezeigte Kachel funktioniert dort weiter.",
      settings: ["Mehrere Ansichten, Kachel in mehreren Ansichten, höchste Zahl an Ansichten: Expertenmenü → Funktionen → Ansichten und Schnellraster"],
    },
    {
      section: "features",
      title: "Spracheingabe",
      id: "voiceDictation",
      whatItIs: "Der Mikrofon-Button {icon:mic} verwandelt Sprache in Text, über die Online-Spracherkennung von Microsoft.",
      howTo: [
        "Auf das Mikrofonsymbol neben der Eingabeleiste klicken, oder im Editor einer Kachel.",
        "Sprechen; der Text erscheint, während er erkannt wird.",
        "Erneut klicken, ESC drücken, oder speichern/schließen, um zu stoppen.",
      ],
      whatHappens:
        "Die Spracheingabe nutzt die Online-Spracherkennung von Microsoft, deine Sprachaufnahme geht also an Microsoft, während du sprichst. Sie startet erst, wenn du im Zustimmungsdialog einwilligst; dabei wird auch die Online-Spracherkennung in Windows eingeschaltet. Das Ausschalten von „Online-Erkennung erlauben“ zieht diese Zustimmung zurück. Gesprochene Wörter wie „Komma“ oder „neue Zeile“ werden zu Satzzeichen; „wörtlich“ vor einem Wort tippt es stattdessen. Ein fehlendes Sprachpaket öffnet den Sprachpaket-Dialog. Ein optionales globales Tastenkürzel startet und stoppt die Spracheingabe, ohne das Fenster zu öffnen. Der Text landet in der Zwischenablage, wird eingefügt oder geht in die Eingabeleiste; währenddessen zeigt die Kurzinfo des Infobereich-Symbols „Aufnahme läuft…“.",
      settings: ["Spracheingabe, Satzzeichen-Befehle, automatische Großschreibung, Stille-Auto-Stopp, Einfügeposition, globales Tastenkürzel: Expertenmenü → Funktionen → Spracheingabe"],
    },
    {
      section: "features",
      title: "Sprachpakete",
      whatItIs: "Texterkennung und Spracheingabe verwenden dieselben Windows-Sprachpakete und denselben Installationsdialog.",
      howTo: [
        "Eine Sprache im Dropdown für die Erkennungssprache auswählen; fehlt das Paket, öffnet sich der Installationsdialog automatisch.",
        "Zum Installieren eines Pakets sind Administratorrechte erforderlich: Windows fragt einmal über den eigenen Bestätigungsdialog danach.",
        "„Sprachen der Texterkennung verwalten…“ oder „Sprachen der Spracheingabe verwalten…“ unter dem Sprach-Dropdown im Expertenmenü öffnet diesen Dialog erneut.",
      ],
      whatHappens:
        "Bei manchen Windows-Versionen sind selbst zum Auflisten der installierten Pakete Administratorrechte erforderlich. In diesem Fall führt ein Button die Prüfung über dieselbe Administratorabfrage erneut aus. Eine Hintergrundüberwachung erkennt, wenn sich die Version eines installierten Pakets geändert hat oder ein Windows-Update es unbemerkt entfernt hat, und kennzeichnet es, ohne sofort zur Neuinstallation aufzufordern. Mit „Abbrechen“ im Hinweis lässt sich der Hinweis vorübergehend ausblenden; die Überwachung kann auch vollständig deaktiviert werden.",
      settings: [],
    },
    {
      section: "capture",
      title: "Screenshot-Tool",
      whatItIs: "Das Screenshot-Tool erfasst einen Teil oder den gesamten Bildschirm direkt in eine neue Kachel.",
      howTo: [
        "Auf das Kamerasymbol klicken.",
        "Einen Bereich ziehen oder ein Fenster anklicken, um es ganz zu erfassen.",
        "Der Screenshot-Editor öffnet sich: dort die Aufnahme als Datei oder Kachel speichern, oder ihn einfach schließen und einfügen.",
      ],
      whatHappens:
        "Die eigenen Fenster der App werden für den Moment der Aufnahme ausgeblendet, sodass sie nie im Bild landen. Das Ergebnis erreicht sofort die Zwischenablage; das Speichern als Kachel oder Datei ist ein separater, optionaler Schritt. Beim Speichern im Screenshots-Ordner wird nie eine vorhandene Datei ersetzt; die Aufnahme bekommt den nächsten freien Namen.",
      settings: ["Screenshot-Tool, Speicherort, Format und Qualität, Aufnahmeverzögerung: Expertenmenü → Aufnehmen → Screenshot", "Screenshots immer speichern: Expertenmenü → Aufnehmen → Screenshot"],
    },
    {
      section: "capture",
      title: "Screenshot-Editor",
      id: "screenshotEditor",
      whatItIs: "Öffnet sich direkt nach einer Aufnahme, bevor der Screenshot gespeichert wird: zum Zuschneiden, Markieren und Beschriften, bevor daraus eine Kachel wird.",
      howTo: [
        "Einen Screenshot aufnehmen; der Editor öffnet sich automatisch über der Aufnahme.",
        "Ein Werkzeug aus der Symbolleiste wählen: Zuschneiden, Stift, Marker, Linie, Pfeil, Rechteck, Ellipse, Text, Schrittnummer, Verpixeln, Fokusbereich oder Hand.",
        "Ziehe die Ränder oder Ecken des Rahmens, um zuzuschneiden.",
        "Rechtsklick auf ein Rechteck, eine Ellipse, Linie, einen Pfeil oder eine verpixelte Fläche dupliziert oder löscht es.",
        "Ein Fokusbereich lässt sich nur löschen.",
        "Pfeiltasten verschieben die gewählte Form, Strg+Pfeil ändert ihre Größe, Umschalt macht größere Schritte, ESC hebt die Auswahl auf.",
        "„Größe ändern“ ändert die echte Pixelgröße des Bildes; „Original zeigen“ (oder V) zeigt die unbearbeitete Aufnahme, solange gedrückt, ein Tippen hält sie fest.",
        "Mit Rückgängig/Wiederholen schrittweise durch die Bearbeitungen gehen.",
        "Änderungen landen sofort in der Zwischenablage: eine halbe Sekunde nach der letzten Änderung wird das bearbeitete Bild kopiert und lässt sich direkt einfügen.",
        "Die untere Leiste speichert den Screenshot als Datei im gewählten Format, macht ihn zum Button oder startet einen neuen Screenshot.",
        "„Schließen“ (auch ESC) übernimmt das Ergebnis; bei ungespeicherten Änderungen fragt der Editor vorher.",
      ],
      whatHappens:
        "Jede Bearbeitung ist ein Schritt in einer Rückgängig/Wiederholen-Liste, deren Höchstzahl separat einstellbar ist. Beim Verpixeln wird immer der ursprüngliche Screenshot verwendet, nie eine bereits bearbeitete Ebene; dadurch werden frühere Markierungen beim Unkenntlichmachen einer Stelle nicht verschmiert. Das Einrasten an Kanten richtet eine Form beim Ziehen am Bildrand, an der Mitte und an den anderen Formen aus, mit einer dünnen Hilfslinie; mit gehaltener Alt-Taste wird frei gezogen. Eine Textbeschriftung kann eine Hintergrundfläche oder einen Umriss bekommen, damit heller Text auch auf hellem Grund lesbar bleibt. Rückgängig deckt jeden Schritt ab, auch Duplizieren, Löschen, Größe ändern und eine Folge von Pfeiltasten-Schritten, die als ein Schritt zählt. Beim Hineinzoomen verschiebt das Hand-Werkzeug, die gehaltene Leertaste oder die mittlere Maustaste die Ansicht für pixelgenaues Arbeiten. Das Zuschneide-Werkzeug zeigt an jedem Anfasser einen Richtungspfeil und innerhalb des Ausschnitts einen Verschieben-Zeiger.",
      settings: ["Screenshot-Editor, Standardwerkzeug, Einrasten an Kanten, Hintergrund für Text, Standardfarbe: Expertenmenü → Aufnehmen → Screenshot-Editor. Der Editor merkt sich die zuletzt benutzte Größe jedes Werkzeugs. Er öffnet auch mit dem zuletzt benutzten Werkzeug, außer es ist ein festes Standardwerkzeug eingestellt.", "Mausrad in Zahlenfeldern: Expertenmenü → Aufnehmen → Beide Editoren"],
    },
    {
      section: "capture",
      title: "Datei im Editor öffnen",
      whatItIs: "Öffnet ein vorhandenes Bild oder Video in dem Editor, den sein Inhalt verlangt, statt einen Editor nur über eine frische Aufnahme zu erreichen.",
      howTo: [
        "Rechtsklick auf den Screenshot- {icon:screenshot} oder Aufnahme-Button {icon:record}, „Datei im Editor öffnen“ wählen und die Datei aussuchen.",
        "Oder per Rechtsklick auf eine Bild- oder Videokachel oder eine Bibliothekszeile Bild bearbeiten oder Video bearbeiten wählen.",
        "Wie gewohnt bearbeiten; beim Speichern wird nach dem Ziel gefragt.",
      ],
      whatHappens:
        "Der Inhalt der Datei entscheidet, welcher Editor sich öffnet, nicht ihre Endung: ein Video mit einer Bild-Endung öffnet trotzdem den Video-Editor. Der Editor arbeitet immer auf einer Kopie, das Original wird also nie verändert; der Speichern-Dialog startet im Ordner der Ausgangsdatei. Eine Datei über der eingestellten Größe wird abgelehnt, bevor sie geladen wird.",
      settings: ["Größte zu öffnende Datei: Expertenmenü → Aufnehmen → Beide Editoren"],
    },
    {
      section: "capture",
      title: "Bildschirmaufnahme",
      id: "screenRecording",
      whatItIs: "Nimmt ein Fenster, einen Monitor oder einen gezogenen Bereich als Video auf, neben dem Screenshot-Werkzeug.",
      howTo: [
        "Auf den Aufnahme-Button {icon:record} klicken.",
        "Ein Fenster anklicken, einen Bereich ziehen oder für den ganzen Bildschirm auf den freien Desktop klicken, in derselben Übersicht wie beim Screenshot-Werkzeug.",
        "Ein Countdown zählt optional herunter, bevor die Aufnahme wirklich startet.",
        "Eine kleine Bedienleiste zeigt Laufzeit und Dateigröße, zwei Ton-Stummschalt-Buttons sowie Pause/Fortsetzen, Stopp und Verwerfen.",
        "Zwei optionale globale Tastenkürzel und das Infobereich-Menü können die Aufnahme ebenfalls stoppen oder pausieren, solange sie läuft.",
        "Eine Fensteraufnahme über alle Bildschirme oder den aktuellen Bildschirm bekommt einen Button, der das Ziel wechselt, ohne neu zu beginnen.",
        "Stopp bietet Speichern, Speichern unter…, Kopieren, Bearbeiten oder Verwerfen an.",
        "Rechtsklick auf den Aufnahme-Button und „Aufnahmen verwalten…“ wählen, um ältere Aufnahmen zu exportieren oder zu löschen.",
      ],
      whatHappens:
        "Aufgenommen wird immer MP4 (H.264); die Qualitätsstufen Klein, Hoch und Verlustfrei tauschen Dateigröße gegen Bildschärfe. Ein Format-Feld kann das Ergebnis danach in WebM, AVI oder GIF umwandeln; GIF ist auf 30 Sekunden begrenzt und hat keinen Ton. Jedes Format außer MP4 sowie Verlustfrei braucht ffmpeg, das der Installer mitbringt; fehlt es, bietet die App den Download an. Eine fehlgeschlagene oder abgebrochene Umwandlung behält die Originalaufnahme. Systemton und Mikrofon landen in getrennten Spuren; die Mikrofonspur bleibt stumm, bis du sie im Video-Editor dazunimmst. Die beiden Ton-Buttons der Leiste schalten eine Quelle sofort stumm und wieder zu; ein abgezogenes Mikrofon beendet nur seine eigene Spur. Speichern und Kopieren nutzen den Aufnahmeordner; Bearbeiten öffnet den Clip im Video-Editor, und wer den Dialog danach ohne Speichern oder Kopieren schließt, verwirft ihn trotzdem. Die Leiste lässt sich überallhin ziehen, folgt dann für diese Aufnahme nicht mehr dem Fenster und ist nie Teil des Videos. Schließt sich das aufgenommene Fenster, stoppt die Aufnahme und behält, was sie hat. Eine nach einem Absturz oder erzwungenen Beenden liegengebliebene Aufnahme wird nach einer Schonfrist entfernt. Die Zeile des Aufnahmeordners im Expertenmenü zeigt Anzahl und Größe der Videos. „Aufräumen…“ listet Dateien älter als 7, 30 oder 90 Tage und löscht erst nach deiner Bestätigung; „Auswahl exportieren“ wandelt die angehakten Dateien in einem Rutsch um. Standardmäßig werden nicht verwendete Aufnahmen nach 90 Tagen gelöscht (0 behält sie für immer). Die optionale Tastenanzeige zeichnet Tastennamen und Zeiten auf, auch getippte Buchstaben, und lässt ein Passwortfeld nur aus, wenn Windows es meldet. Für Aufnahmen mit vertraulichen Eingaben „Nur Tastenkürzel“ einschalten.",
      settings: [
        "Bildschirmaufnahme, Countdown, Bitrate, Qualität, Format, Aufnahmeordner: Expertenmenü → Aufnehmen → Bildschirmaufnahme",
        "Mauszeiger, Tonquellen und -gerät, Stopp-/Pause-Tastenkürzel, Einträge im Infobereich-Menü, Tastenanzeige, Rahmen um einen aufgenommenen Bereich und seine Farbe: Expertenmenü → Aufnehmen → Während der Aufnahme",
        "Defekte Aufnahmen automatisch entfernen, jetzt aufräumen: Expertenmenü → System & Daten → Fehlersuche",
      ],
    },
    {
      section: "capture",
      title: "Kameraaufnahme",
      whatItIs: "Nimmt die Webcam in ein eigenes Video auf, allein oder neben einer Bildschirmaufnahme.",
      howTo: [
        "Die Kameraaufnahme im Expertenmenü einschalten.",
        "Rechtsklick auf den Aufnahme-Button {icon:record} und „Kamera aufnehmen“ wählen.",
        "Im eigenen Fenster die Vorschau prüfen, Tonquelle, Auflösung und Ordner wählen, dann Start.",
      ],
      whatHappens:
        "Das Kamerafenster oder sein Statuspunkt in der Aufnahmeleiste bleibt die ganze Zeit sichtbar, die Kamera läuft also nie unbemerkt. Die Kamera landet in einer eigenen Datei und wird nie in eine Bildschirmaufnahme eingebrannt. Während einer Bildschirmaufnahme gestartet, teilen beide einen Startpunkt, und der Video-Editor öffnet dann die Bildschirmaufnahme mit der Kamera als verschiebbarer Überlagerung darüber.",
      settings: ["Kameraaufnahme, Standardkamera, Auflösung, Bildrate: Expertenmenü → Aufnehmen → Kameraaufnahme"],
    },
    {
      section: "capture",
      title: "Video-Editor",
      whatItIs: "Ein kleiner Editor für eine fertige Aufnahme oder ein angehängtes Video, geöffnet aus dem Ergebnisdialog einer Aufnahme oder über das Rechtsklick-Menü eines Video-Prompts.",
      howTo: [
        "Bei einer frischen Aufnahme: im Ergebnisdialog auf Mehr, dann auf Video-Editor klicken.",
        "Bei einem bereits gespeicherten Video-Prompt: rechtsklicken und „Video bearbeiten“ wählen.",
        "Zuschneiden, Kürzen, Geschwindigkeit ändern, Ein- und Ausblenden sowie die Größe ändern, per eingegebenen Maßen, Prozentwert oder Schnellauswahl.",
        "Bild-, Standbild-, Text-, Verpixelungs- oder Fokusbereich-Overlays auf der Zeitleiste hinzufügen.",
        "Exportieren: bei reinem Kürzen greift ein verlustfreier Schnellpfad, sonst wird der ganze Clip einmal neu kodiert.",
      ],
      whatHappens:
        "Bis zum Export wird nichts verarbeitet: Die Live-Vorschau spielt die Bearbeitungen direkt ab. Filmstreifen und Wellenform geben eine zoombare Übersicht über den ganzen Clip für bildgenaue Navigation. Die Vorschau zoomt außerdem mit dem Mausrad bis 8× und lässt sich durch Ziehen verschieben, mit der mittleren Maustaste immer und mit der linken, sobald hineingezoomt ist. So lässt sich ein kleines Overlay bei einer großen Aufnahme genau platzieren. Das Exportziel ist ein Eingabefeld: den Pfad direkt eintippen, oder „Exportieren nach…“ wählen, um nur den Ordner zu wechseln und den Dateinamen zu behalten. Nach einem fertigen Export verschiebt „Speichern unter…“ das Video an einen anderen Ort. Ein Formatwechsel ändert nur die Endung. Der Button „Kopieren“ daneben verzichtet ganz auf einen Namen. Er exportiert in eine temporäre Kopie, die nach einer Stunde und beim nächsten Start wieder entfernt wird, und legt sie direkt in die Zwischenablage. Ein paar fertige Export-Voreinstellungen (unveränderte Qualität, kleiner für Chat, animiertes GIF) merken sich die letzte Wahl. Die Formatauswahl beim Export bietet MP4, WebM, GIF und AVI, jeweils mit einem kurzen Hinweis, wofür es gedacht ist. GIF hat nie Ton, warnt ab zehn Sekunden Länge und ist auf eine Höchstauflösung begrenzt, da es schnell groß wird. Rückgängig/Wiederholen deckt jeden Bearbeitungsschritt ab, begrenzt durch ein einstellbares Limit. „Video hinzufügen“ legt einen weiteren Clip als Überlagerung darüber, und die Überlagerung „Tasten“ macht aus den während einer Aufnahme gedrückten Tasten ein Tastaturbild. Rechtsklick auf die Zeile einer Überlagerung dupliziert oder löscht sie. Ein Zuschnitt oder eine Überlagerung rastet beim Ziehen an den Bildkanten, an der Mitte und an anderen Überlagerungen ein; mit gehaltener Alt-Taste wird frei gezogen. Eine neue Überlagerung startet mit der zuletzt für ihre Art benutzten Farbe, Stärke und Größe. Unfertige Schnitte und Überlagerungen werden gemerkt, sodass das erneute Öffnen derselben Aufnahme das Weitermachen anbietet. Der Name der Exportdatei hat ein eigenes Feld; eine vorhandene Datei wird nie ersetzt, der Export nimmt den nächsten freien Namen.",
      settings: ["Video-Editor, Einrasten an Kanten: Expertenmenü → Aufnehmen → Video-Editor", "Unfertige Video-Bearbeitungen merken, und wie viele Tage: Expertenmenü → Aufnehmen → Beide Editoren"],
      sketches: ["editorTimeline"],
    },
    {
      section: "capture",
      title: "Texterkennung (OCR)",
      id: "ocr",
      whatItIs: "Liest mit der geräteeigenen Erkennung von Windows Text aus einem Screenshot oder einer Bilddatei. Das läuft vollständig offline und nichts verlässt das Gerät.",
      howTo: [
        "Über die Eingabeleiste: ein Klick auf den OCR-Button {icon:ocr} erkennt Text in einem Screenshot, ein Rechtsklick darauf wählt eine vorhandene Bilddatei.",
        "Das Ergebnis prüfen und anschließend kopieren oder direkt einfügen.",
      ],
      whatHappens:
        "Die Erkennung läuft vollständig auf dem Gerät über die eigene Erkennungs-Engine von Windows. Der Ergebnisdialog kann den Text automatisch kopieren und getrennte Zeilen wieder zu Absätzen verbinden; beides lässt sich separat ein- und ausschalten. Fehlt ein Sprachpaket, öffnet sich derselbe Dialog wie bei der Spracheingabe; siehe „Sprachpakete“.",
      settings: ["Texterkennung, Ergebnis automatisch kopieren, Zeilen verbinden, Erkennungssprache: Expertenmenü → Aufnehmen → Texterkennung"],
    },
    {
      section: "capture",
      title: "Farbpipette & Auswahlverlauf",
      id: "colorPicker",
      whatItIs: "Die Farbpipette liest jeden Bildschirmpixel als Hex, RGB oder HSL aus.",
      howTo: [
        "Auf das Pipettensymbol in der Eingabeleiste klicken.",
        "Die Lupe zum gewünschten Pixel bewegen; scrollen oder Plus/Minus zum Zoomen nutzen.",
        "Klick kopiert die Farbe; Rechtsklick auf das Pipettensymbol öffnet die letzten paar Auswahlen erneut.",
      ],
      whatHappens:
        "Es wird nie automatisch etwas in einer Kachel gespeichert; eine Auswahl landet nur in der Zwischenablage. Der Auswahlverlauf hält die letzten paar Farben und übersteht einen Neustart, begrenzt durch eine eigene Einstellung.",
      settings: ["Farbpipette, Zoom, Format, Länge des Auswahlverlaufs: Expertenmenü → Aufnehmen → Farbpipette"],
      tip: "Farbformat (Hex, RGB oder HSL) im Hinweis wählen, der direkt nach der Auswahl erscheint.",
    },
    {
      section: "tiles",
      title: "Kachel-Verhalten & -Aussehen",
      whatItIs: "Diese Einstellungen regeln, wie eine Kachel auf die Maus reagiert und wie sie aussieht.",
      howTo: [
        "Rechtsklick auf eine Kachel für ihr Menü: Bearbeiten, Anheften, Verbergen, Löschen und mehr.",
        "Zwei schnelle Rechtsklicks auf eine Kachel öffnen direkt ihren Editor.",
        "Rechtsklick auf eine Kachel (oder Strg+Klick) wählt sie aus; solange du auswählst, zeigt jede Kachel ihr Kästchen.",
        "Eine Kachel auf eine beliebige Rasterzelle ziehen; auf einer besetzten Zelle loslassen, um zu tauschen.",
        "Hover-Lift, Schatten, Badges und ähnliche Optiken im Expertenmenü einschalten.",
        "Dort einen Kachelstil wählen: fett, Großbuchstaben, Monospace, kursiv, Namen mit Schatten, kompakt oder Verlauf.",
      ],
      whatHappens: "Das sind nur Interaktion und Optik; nichts davon ändert, was eine Kachel kopiert.",
      settings: [
        "Kachel-Menü, Drag-to-Reorder: Expertenmenü → Kacheln → Kacheln",
        "Abblenden bis zum Überfahren, Anheben beim Überfahren: Expertenmenü → Kacheln → Kacheln",
        "Typ-Badges, Schatten, abgerundete Ecken, Kachelstil (fett, Großbuchstaben, Monospace, kursiv, Namen mit Schatten, kompakt, Verlauf): Expertenmenü → Kacheln → Kachel-Aussehen",
      ],
    },
    {
      section: "tiles",
      title: "Medien",
      whatItIs: "Diese Einstellungen betreffen Kacheln, die ein Video oder Bild tragen.",
      howTo: [
        "Ein Video oder Bild an eine Kachel anhängen: Dateianhang, Screenshot, Einfügen oder Ziehen und Ablegen.",
        "Wiedergabe und privatsphärefreundliche Optiken im Expertenmenü anpassen.",
      ],
      whatHappens: "Video-Einstellungen (Autoplay, Stumm, Schleife, Lautstärke) gelten für jede Video-Kachel; Unschärfe ändert nur das Aussehen, nie den kopierten Inhalt. Bibliothek und Kopierverlauf zeigen außerdem die Länge eines Videos auf dem Vorschaubild sowie Auflösung und Dateigröße in der Kurzhilfe; das Hauptraster zeigt keins von beidem.",
      settings: ["Video-Steuerung, Autoplay und Schleife, Stumm: Expertenmenü → Kacheln → Medien", "Akzentrahmen und abgerundete Ecken um Medien: Expertenmenü → Kacheln → Medien", "Dauer/Auflösung/Größe in Bibliothek und Kopierverlauf: Expertenmenü → Kacheln → Medien", "Medienangaben bei älteren Anhängen nachtragen: Expertenmenü → System & Daten → Fehlersuche"],
    },
    {
      section: "tiles",
      title: "Favoriten",
      whatItIs: "Ein Stern markiert eine Kachel als Favorit für schnellen späteren Zugriff.",
      howTo: ["„Zu Favoriten hinzufügen“ im Rechtsklickmenü einer Kachel oder den Stern in der Bibliothek nutzen.", "Eine Favoriten-Kachel zeigt oben rechts einen kleinen Stern.", "Die eigene Favoriten-Ansicht öffnen, um nur markierte Kacheln zu sehen."],
      whatHappens: "Favoriten erhalten ihre eigene sortierbare Ansicht, unabhängig davon, wo die Kachel sonst liegt.",
      settings: ["Favoriten-Ansicht: Einstellungen. Eigene Sortierung: Expertenmenü → Kacheln → Favoriten"],
    },
    {
      section: "tiles",
      title: "Kurzinfos",
      whatItIs: "Das Überfahren einer Kachel mit der Maus zeigt ihren vollständigen Text, ihre Länge und eine eventuelle private Notiz.",
      howTo: ["Über eine Kachel fahren und die Verzögerung abwarten.", "Die Verzögerung im Expertenmenü anpassen, falls der Standard nicht passt."],
      whatHappens: "Die Kurzinfo blendet sich nach ihrem Timeout automatisch aus, selbst wenn die Maus stehen bleibt.",
      settings: ["Verzögerung vor Anzeige: Expertenmenü → Kacheln → Kurzinfos"],
    },
    {
      section: "appearance",
      title: "Aussehen & Designs",
      whatItIs: "Sieben integrierte Designs decken System, Hell, Dunkel und vier Akzent-Varianten ab: Programmierer, Neon-Violett, Ozean und Cyberpunk.",
      howTo: ["Ein Design in den Einstellungen wählen.", "Den Design-Editor, sobald aktiviert, öffnen, um Farben eines Designs anzupassen und ein eigenes zu speichern."],
      whatHappens: "\"System\" folgt live der Hell-/Dunkel-Einstellung des Betriebssystems. Ein benutzerdefiniertes Design wird separat gespeichert; das integrierte, auf dem es basierte, bleibt unangetastet.",
      settings: [
        "Design-Wahl: Einstellungen",
        "Design-Editor: Expertenmenü → Aussehen → Visuell",
        "Milchige Dialoge, versteckte Scrollbalken: Expertenmenü → Aussehen → Visuell",
      ],
      tip: "Ausgeschaltete benutzerdefinierte Designs entfernen auch den Eintrag „Eigenes Design…“ aus der Design-Auswahl.",
    },
    {
      section: "appearance",
      title: "Design-Editor",
      whatItIs: "Der Design-Editor passt die Farben eines beliebigen integrierten Designs an und speichert das Ergebnis als neues Design.",
      howTo: [
        "Den Design-Editor öffnen: Expertenmenü → Aussehen → Visuell.",
        "Benutzerdefinierte Designs müssen an sein.",
        "Ein Basis-Design wählen, Farben anpassen und unter neuem Namen speichern.",
      ],
      whatHappens: "Ein benutzerdefiniertes Design erscheint im regulären Design-Auswahl neben den integrierten; sein Löschen betrifft nicht das Design, auf dem es basierte.",
      settings: [],
    },
    {
      section: "appearance",
      title: "Skalierung & Zoom",
      whatItIs: "Unabhängige Größenregler passen verschiedene Teile des Fensters getrennt voneinander an.",
      howTo: ["Einstellungen öffnen und die Skalierungs-Regler finden.", "UI- oder Symbol-Skalierung unabhängig voneinander anpassen."],
      whatHappens: "Jeder Regler beeinflusst nur seinen eigenen Teil des Fensters: eine höhere Symbol-Skalierung vergrößert zum Beispiel keine Dialoge.",
      settings: ["Größe der Oberfläche: Einstellungen", "Symbolgröße: Expertenmenü → Aussehen → Visuell"],
    },
    {
      section: "library",
      title: "Bibliothek & Suche",
      whatItIs: "Die Bibliothek listet jede Kachel als durchsuchbare, filterbare Liste, anders als das Raster, das nur zeigt, was passt.",
      howTo: [
        "Klick auf den Bibliothek-Button {icon:library} in der Werkzeugleiste öffnet sie.",
        "Tippen zum Suchen, oder die Liste mit dem Typfilter eingrenzen.",
        "Klick auf eine Zeile zum Kopieren.",
        "Das Häkchen „Nach dem Kopieren schließen“ unten in der Bibliothek entscheidet, ob Kopieren die Bibliothek schließt, und merkt sich die Wahl.",
        "Das Menü zum Bearbeiten, auf das Raster Ziehen, Taggen oder Löschen nutzen.",
        "Eine Suche mit dem Stecknadel-Button im Feld anheften: Sie bleibt oben in den letzten Suchen, samt Tag-Filter und Sortierung.",
        "Der Knopf „Kacheln“ neben dem Sortieren wechselt zu großen Vorschaukacheln.",
        "Zwei Zeilen auswählen und in der Auswahlleiste „Vergleichen“ klicken, um beide nebeneinander mit markierten Unterschieden zu sehen.",
      ],
      whatHappens:
        "Die Suche ist fehlertolerant und sortiert, falls nicht deaktiviert, nach am häufigsten oder zuletzt Verwendetem. Im Vergleich lässt sich der Text einer Seite in die andere übernehmen. Die Kachelansicht verhält sich bei Klicks, Menüs und Mehrfachauswahl genau wie die Liste. Eine optionale Einstellung liest Text aus angehängten Bildern, damit die Suche ihn auch findet; ein Treffer nur dort trägt die Kennzeichnung „im Bild gefunden“. Dieser Text wird nie angezeigt, exportiert oder synchronisiert, und seine Länge ist begrenzt. Wird das Bild entfernt oder ersetzt, ist er sofort weg, und das Ausschalten der Einstellung entfernt ihn aus jeder Kachel, auch im Papierkorb.",
      settings: [
        "Unscharfe Suche, Typfilter, letzte Suchen: Expertenmenü → Bibliothek & Kopierverlauf → Suche",
        "Suchfeld beim Öffnen fokussieren: Expertenmenü → Bibliothek & Kopierverlauf → Suche",
        "Einträge je Suchliste (Bibliothek, Alles durchsuchen, Schnellstarter): Expertenmenü → Bibliothek & Kopierverlauf → Suche",
        "Clevere Sortierung, Mehrfachauswahl für Stapelaktionen, Bild- und Video-Vorschau in den Zeilen, Kachelansicht: Expertenmenü → Bibliothek & Kopierverlauf → Anzeige",
        "Text in angehängten Bildern für die Suche erfassen: Expertenmenü → Bibliothek & Kopierverlauf → Suche",
      ],
    },
    {
      section: "library",
      title: "Alles durchsuchen",
      whatItIs: "Ein Suchfeld für Kacheln, Kopierverlauf, Anhänge, Ansichten, Einstellungen und diese Hilfe, alles auf einmal.",
      howTo: [
        "Auf die Lupe in der Werkzeugleiste klicken oder Strg+K drücken.",
        "Mindestens zwei Zeichen tippen; die Treffer erscheinen in Gruppen.",
        "Mit den Pfeiltasten wählen und Enter drücken, oder einen Treffer anklicken.",
      ],
      whatHappens:
        "Enter tut, was zum Treffer passt: Es kopiert eine Kachel oder einen Eintrag im Kopierverlauf, öffnet einen Anhang, wechselt in eine Ansicht, öffnet und markiert eine Einstellung oder öffnet den Hilfeartikel. Jede Gruppe zeigt einige Treffer und ein „… weitere anzeigen“. Ist das Feld leer, werden die letzten Suchen angeboten. ESC, ein Klick daneben oder die Lupe schließt es wieder.",
      settings: ["Alles durchsuchen: Expertenmenü → Bibliothek & Kopierverlauf → Suche"],
    },
    {
      section: "library",
      title: "Tags",
      whatItIs: "Tags sind Freitext-Etiketten, die Kacheln zugewiesen werden, um die Bibliothek zu filtern.",
      howTo: [
        "Editor einer Kachel öffnen und einen Tag-Namen eintippen, um ihn hinzuzufügen, oder einen vorhandenen wählen.",
        "Die Tag-Filterleiste in der Bibliothek nutzen, um die Liste auf ein oder mehrere Tags einzugrenzen.",
        "Zum Archivieren im Menü einer Kachel „Archivieren“ wählen: sie bekommt den Archiv-Tag und verschwindet aus Raster und Liste, bis du sie wieder anzeigst.",
      ],
      whatHappens: "Umbenennen, Zusammenführen oder Löschen eines Tags in der Tag-Verwaltung wirkt überall dort, wo er genutzt wird, in einem Schritt. Das Archiv ist ein gewöhnlicher Tag mit reserviertem Namen: Eine archivierte Kachel taucht weiterhin in der Suche auf und bleibt in Exporten und Sicherungen, und „Aus dem Archiv holen“ im selben Menü holt sie zurück.",
      settings: ["Archiv-Tag: Expertenmenü → Bibliothek & Kopierverlauf → Anzeige"],
    },
    {
      section: "library",
      title: "Kopierverlauf",
      whatItIs: "Der Kopierverlauf protokolliert, was wann kopiert wurde, in der App und, wenn eingeschaltet, in anderen Apps.",
      howTo: [
        "Bibliothek öffnen und den Reiter Kopierverlauf wählen.",
        "Suchen oder nach Typ filtern, und solange Kopien aus anderen Apps aufgezeichnet werden, auch nach Quelle.",
        "Ein Zeitraum grenzt die zuletzt kopierten Einträge auf heute oder die letzten 7 oder 30 Tage ein.",
        "Eine einzelne Zeile löschen oder den gesamten Kopierverlauf leeren.",
      ],
      whatHappens: "Die gruppierte Ansicht zeigt eine Zeile pro Kachel (ihre letzte Kopie) statt jedes einzelnen Ereignisses. Älterer Kopierverlauf als die Aufbewahrungs-Einstellung wird automatisch gelöscht (bei einer neuen Installation 30 Tage, einstellbar, 0 behält alles). Der Kopierverlauf ist eine Liste; das Sortierfeld wechselt zwischen Zuletzt kopiert und Meistgenutzt (die 20 häufigsten).",
      settings: ["Gruppierte Ansicht, Aufbewahrungsdauer: Expertenmenü → Bibliothek & Kopierverlauf → Kopierverlauf", "Einen Eintrag überspringen, der die Kopie davor wiederholt: Expertenmenü → Bibliothek & Kopierverlauf → Kopierverlauf"],
    },
    {
      section: "library",
      title: "Kopien aus anderen Apps",
      whatItIs:
        "Eingeschaltet nimmt der Kopierverlauf auch Text auf, der irgendwo im System kopiert wurde, markiert als anderswo kopiert.",
      howTo: [
        "„Kopien aus anderen Apps aufnehmen“ im Expertenmenü einschalten.",
        "Text wie gewohnt irgendwo im System kopieren.",
        "Bibliothek → Kopierverlauf öffnen, den Herkunftsfilter auf „Anderswo“ stellen und nützliche Einträge als Kacheln speichern.",
      ],
      whatHappens:
        "Es wird nichts überwacht, bis die Funktion eingeschaltet ist. Kopien unter der Mindestlänge werden ignoriert, und die ältesten dieser Einträge fallen weg, sobald sie ihre Obergrenze überschreiten. Mit „Vermutete Passwörter überspringen“ landet eine Kopie, die wie ein Passwort aussieht, nie unter den Kopien aus anderen Apps. Als Passwort gilt eine Zeile ohne Leerzeichen mit gemischten Buchstaben, Ziffern und Zeichen; das ist eine Schätzung, die im Zweifel lieber überspringt. Beim Ausschalten fragt die App, ob das Gesammelte behalten oder gelöscht werden soll; wer die Frage schließt, behält es. Nach dem Leeren der Kopien aus anderen Apps bietet die Meldung einen Rückgängig-Button.",
      settings: ["Vermutete Passwörter überspringen, Mindestlänge, Obergrenze: Expertenmenü → Bibliothek & Kopierverlauf → Kopierverlauf"],
    },
    {
      section: "system",
      title: "System",
      whatItIs: "System-Einstellungen betreffen das Verhalten der ganzen App, nicht einer einzelnen Kachel.",
      howTo: ["Die System-Gruppe des Expertenmenüs für Tastaturnavigation, Daten-Ordner, Fortschritt am Taskleistensymbol und den Download der Video-Werkzeuge öffnen."],
      whatHappens:
        "Die Update-Suche unter Einstellungen → System installiert nie etwas, ohne vorher zu fragen. Ein Wechsel des Daten-Ordners verschiebt die bestehende Datenbank dorthin. Mehrere Instanzen zuzulassen braucht einen Neustart. Lange Aufgaben können das Taskleistensymbol als Fortschrittsbalken füllen und die Restzeit schätzen.",
      settings: [
        "Tastaturnavigation, Daten-Ordner, Fortschritt am Taskleistensymbol, Download der Video-Werkzeuge: Expertenmenü → System & Daten → System",
        "Mehrere Instanzen erlauben: Expertenmenü → System & Daten → Fehlersuche",
      ],
    },
    {
      section: "system",
      title: "Werkzeugleiste",
      whatItIs: "Die Werkzeugleiste bestimmt, welche Buttons die obere Leiste und die Eingabeleiste zeigen.",
      howTo: [
        "Rechtsklick auf die obere Leiste blendet ihre Buttons ein oder aus; die Funktionen selbst schaltet das Expertenmenü.",
        "Obere Leiste oder Eingabeleiste ausblenden: Rechtsklick auf das Zahnrad oder die obere Leiste.",
      ],
      whatHappens: "Das Ausblenden eines Buttons blendet ihn nur aus; die Funktion dahinter läuft weiter über ihr eigenes Tastenkürzel oder ihren Menüeintrag.",
      settings: [
        "Bibliotheksknopf und Doppelklick auf die Leisten-Buttons: Expertenmenü → System & Daten → Werkzeugleiste; Logo, Titel und Leisten-Buttons: Rechtsklick auf die obere Leiste.",
        "Schnelle Rastergrößen-Steuerung und sie erst beim Überfahren zeigen, mehrere Ansichten: Expertenmenü → Funktionen → Ansichten und Schnellraster",
      ],
    },
    {
      section: "system",
      title: "Datenschutz",
      whatItIs: "Diese Einstellungen halten den Inhalt dieses Fensters aus Screenshots, Screen-Shares und dem Blick von Passanten heraus.",
      howTo: [
        "Bei Bedarf Unschärfe für Medien im Expertenmenü aktivieren.",
      ],
      whatHappens:
        "Der Ausschluss von Bildschirmaufnahmen macht das Fenster für Screenshot- und Aufnahme-Apps unsichtbar, auch für die eigenen. Bleibt eine Aufnahme also leer, zuerst diese Einstellung prüfen. Eine einzelne Kachel kann außerdem als vertraulich markiert werden; siehe „Vertrauliche Kacheln“.",
      settings: [
        "Von Bildschirmaufnahmen ausschließen, Medien unscharf machen: Expertenmenü → System & Daten → Datenschutz",
        "Eine Kachel als vertraulich markieren, Sperr-Zeit des Tresors: Expertenmenü → System & Daten → Datenschutz",
      ],
    },
    {
      section: "system",
      title: "Vertrauliche Kacheln",
      whatItIs: "Eine vertrauliche Kachel hält ihren Text verschlüsselt, mit einem Tresor-Passwort, das nur der Besitzer kennt.",
      howTo: [
        "Vertrauliche Kacheln im Expertenmenü einschalten.",
        "Im Editor einer Kachel \"Vertraulich\" anhaken und speichern.",
        "Beim ersten Mal ein Tresor-Passwort festlegen und bestätigen, dass ein verlorenes Passwort nicht wiederherstellbar ist.",
        "Danach das Passwort einmal pro Sitzung eingeben, um diese Kacheln zu sehen oder zu kopieren.",
      ],
      whatHappens:
        "Das Passwort wird nie gespeichert. Wer es vergisst, verliert diese Kacheln endgültig, es gibt keine Wiederherstellung. Nach der Sperrzeit sind die Kacheln wieder gesperrt. Ein Export mit vertraulichen Kacheln fragt, ob sie verschlüsselt bleiben oder weggelassen werden. Sie werden nie in den Sync-Ordner geschrieben und bekommen nie einen Editor-Entwurf. Das Zurücksetzen der Einstellungen behält den Tresor, die Kacheln bleiben also lesbar.",
      settings: ["Vertrauliche Kacheln, Sperre nach wie vielen Minuten: Expertenmenü → System & Daten → Datenschutz"],
      tip: "Das Tresor-Passwort sicher notieren, bevor etwas als vertraulich markiert wird.",
    },
    {
      section: "system",
      title: "Updates & Sicherheit",
      whatItIs: "Eine optionale Prüfung sucht nach einer neueren Version und kommuniziert nur mit dem Update-Server.",
      howTo: [
        "Beim ersten Start auf die Frage nach Updates mit Ja antworten, für eine Benachrichtigung bei neuer Version.",
        "„Automatische Updates“ in den Einstellungen ausschalten, um automatische Prüfungen zu beenden.",
        "Solange der Schalter an ist, legt die Auswahl daneben fest, wie oft.",
        "Ein angebotenes Update annehmen, um es mit einem Klick herunterzuladen und zu installieren.",
      ],
      whatHappens:
        "Beim ersten Start fragt die App, ob es nach Updates suchen darf; bis zur Antwort geht es nie online. Die Update-Prüfung und, falls diese Laufzeit fehlt, ein Download des Microsoft-WebView2-Installers beim ersten Start sind der einzige Netzwerkverkehr, den die App von sich aus startet. Ein heruntergeladenes Update wird vor der Ausführung anhand einer veröffentlichten Prüfsumme verifiziert: eine fehlende oder falsche Prüfsumme blockiert die Installation, statt eine unbestätigte Datei auszuführen. Alles andere wartet auf dich. ffmpeg, für Videoexport und -umwandlung, bringt eine installierte Version mit und greift dafür nie auf das Netzwerk zu. Eine portable Version bietet einen einmaligen Download nur an, wenn eine Videofunktion es braucht, nie automatisch und nie im Hintergrund. Die Spracheingabe schickt Ton an die Online-Spracherkennung von Microsoft, erst nach deiner Zustimmung. Ein Sprachpaket für Texterkennung oder Spracheingabe kommt über Windows Update, nur wenn du die Installation startest. Nach einem Update verlinkt eine kleine Meldung auf die Änderungen. Das Fenster Über, neben „Updates suchen“ in den Einstellungen, zeigt die Version, die ganze Änderungsgeschichte und jeden mitgelieferten Open-Source-Bestandteil mit seiner Lizenz.",
      settings: ["Automatische Updates und wie oft sie prüfen: Einstellungen"],
    },
    {
      section: "system",
      title: "Befehlszeile",
      whatItIs: "Die Programmdatei funktioniert auch als Kommandozeilen-Werkzeug, für Skripte und schnelle Aufgaben, ohne das Fenster zu öffnen.",
      howTo: [
        "Eine Eingabeaufforderung öffnen und die Programmdatei mit --help starten, um jeden Befehl zu sehen.",
        "Kacheln auflisten, lesen, kopieren, anlegen, ändern, löschen und wiederherstellen.",
        "Tags und Ansichten verwalten, suchen, exportieren, importieren und Sicherungen anlegen oder wiederherstellen.",
        "--json für maschinenlesbare Ausgabe anhängen, und --yes an jeden Befehl, der etwas ändert.",
      ],
      whatHappens:
        "Läuft die App bereits, geht der Befehl an sie und ihre Antwort wird ausgegeben; sonst läuft der Befehl allein, ohne Fenster. Es greift immer nur eine Kopie auf die Daten zu. Ohne --yes sagt ein ändernder Befehl nur, was er täte. Ordnerorte, der Tresor, das Sicherungspasswort und das Aufnehmen von Kopien aus anderen Apps lassen sich hier nicht ändern, nur in der App.",
      settings: [],
    },
    {
      section: "system",
      title: "Vorführmodus",
      whatItIs:
        "Der Vorführmodus schaltet das ganze Fenster auf eine Beispiel-Bibliothek um, damit eine Vorführung oder ein Screenshot nie etwas Echtes zeigt.",
      howTo: [
        "Den Vorführmodus im Expertenmenü einschalten; die App startet neu in die Beispiel-Bibliothek.",
        "Alles zeigen oder ausprobieren; eine Markierung bleibt die ganze Zeit sichtbar.",
        "Zum Zurückkehren ausschalten; die App fragt zuerst nach und startet neu in die eigenen Daten.",
      ],
      whatHappens:
        "Die Beispiel-Bibliothek wird jedes Mal frisch erzeugt und deckt die meisten Funktionen ab. Sie enthält Kacheln mit und ohne Bild, eine Kachel mit Rotation, einen Platzhalter, Tags, einen Favoriten, zwei Ansichten, einen Ordner, einen Anhang und ein Video. Die eigenen Daten werden währenddessen nie geöffnet oder geändert. Was im Vorführmodus geändert wurde, wird beim Beenden verworfen.",
      settings: ["Vorführmodus: Expertenmenü → System & Daten → Fehlersuche"],
    },
    {
      section: "data",
      title: "Wo die Daten liegen",
      whatItIs:
        "Alles liegt in einer einzigen lokalen SQLite-Datenbank plus ihrer kleinen Begleitdatei.",
      howTo: [
        "Installiert: der Daten-Ordner liegt unter AppData, im Expertenmenü einsehbar und änderbar.",
        "Portabel: der Daten-Ordner liegt direkt neben der exe, sodass alles auf einen USB-Stick passt.",
      ],
      whatHappens:
        "Ein Wechsel von portabel zu installiert, oder zurück, wird automatisch erkannt und als einmalige Migration angeboten: bestehende Daten werden nie stillschweigend zurückgelassen. Zeigt der Daten-Ordner auf einen Cloud-synchronisierten Ordner, erscheint einmal eine Warnung, und eine kleine Sperrdatei hält dann fest, welcher PC die Datenbank nutzt. Ein anderer PC, der eine frische Sperre findet, öffnet alles nur lesend und zeigt das in einem Banner; eine Sperre, die länger niemand erneuert hat, wird übernommen.",
      settings: ["Daten-Ordner: Expertenmenü → System & Daten → System", "Sperre des Daten-Ordners: Expertenmenü → System & Daten → Fehlersuche"],
    },
    {
      section: "data",
      title: "Sicherungen",
      whatItIs: "Das Fenster „Sicherungen“ erstellt verschlüsselte, komprimierte Snapshots der gesamten Bibliothek und Einstellungen.",
      howTo: [
        "„Sicherungen“ in den Einstellungen öffnen.",
        "Automatische Sicherungen aktivieren und ein Intervall wählen, oder manuell eine erstellen.",
        "Mit den zwei Buttons neben „Sicherung erstellen“ eine Sicherung in eine Datei deiner Wahl speichern oder aus einer Datei importieren.",
        "Aus der Snapshot-Liste wiederherstellen und dabei nur Daten, nur Einstellungen oder beides wählen.",
      ],
      whatHappens:
        "Aufbewahrungsstufen (täglich, wöchentlich, monatlich, jährlich) dünnen ältere Snapshots automatisch aus, statt jeden für immer zu behalten. Eine Sicherung ist entweder vollständig oder gar nicht vorhanden; eine fehlgeschlagene hinterlässt nichts. Dasselbe Fenster enthält außerdem einen Speicherüberblick, was auf der Festplatte wie viel Platz belegt. Jeder Snapshot kann seinen Inhalt zeigen: Anzahlen, die jüngste Änderung darin, die Version, die ihn erstellt hat, und jede enthaltene Kachel. Die Liste ist durchsuchbar und sortierbar, und ein Button stellt eine einzelne Kachel wieder her. Je eine Zeile pro Art aufbewahrter Daten zeigt, wie viel sie enthält und wie lange sie bleibt, springt zu ihrer Einstellung und kann sie sofort aufräumen. Die Arten sind Papierkorb, Kopierverlauf, Kopien aus anderen Apps, Aufnahmen, temporäre Exporte und Bearbeitungsstände.",
      settings: [
        "Intervall, Aufbewahrungsstufen, Passwort, Wiederherstellungsumfang: Sicherungen (eigenes Fenster, geöffnet aus Einstellungen)",
      ],
    },
    {
      section: "data",
      title: "Import & Export",
      whatItIs: "Einfacher CSV-, TXT- oder JSON-Export deckt nur den Text ab; eine vollständige Sicherungsdatei deckt alles ab.",
      howTo: [
        "In den Einstellungen unter Daten auf Importieren oder Exportieren klicken.",
        "Ein Format aus der Auswahl neben dem Export-Button wählen (JSON behält am meisten); ein CSV- oder JSON-Import zeigt zuerst Name und Duplikat-Status je Eintrag.",
        "Eine vollständige Sicherung wird in ihrem eigenen Fenster wiederhergestellt.",
        "Zum tatsächlichen Import bestätigen, oder abbrechen, wodurch sich nichts ändert.",
        "„Importieren“ → „Aus Ordner…“ in den Einstellungen macht aus einem Ordner mit .md- oder .txt-Dateien Kacheln, eine pro Datei.",
        "Vorher zeigt eine Vorschau jede Datei mit einem Häkchen.",
      ],
      whatHappens: "CSV und TXT transportieren nur Text; JSON trägt zusätzlich die vollen Rotationstexte einer Kachel mit Rotation, Tags, Notiz, Tastenkürzel und Farbe, die in den anderen beiden nicht sauber Platz finden. Bilder, Dateien, Layout und Einstellungen benötigen weiterhin das vollständige Sicherungsformat. Der Import entscheidet anhand des Dateiinhalts zwischen CSV und JSON, nie anhand der Endung. Ein Ordner-Import liest nur: Nichts wird verschoben oder gelöscht. Der Dateiname wird zum Namen, jeder Unterordner zum Tag, und eine unlesbare Datei erscheint ausgegraut mit dem Grund.",
      settings: [
        "Import und Export ein- oder ausschalten: Expertenmenü → System & Daten → Daten & Wartung",
        "Tiefe und Dateigrenze des Ordner-Imports: Expertenmenü → System & Daten → Daten & Wartung",
      ],
    },
    {
      section: "data",
      title: "Markdown-Ordner-Sync",
      whatItIs:
        "Der Ordner-Sync spiegelt jede Text-Kachel als eigene lesbare Markdown-Datei in einem gewählten Ordner.",
      howTo: [
        "Im Expertenmenü einschalten und einen Ordner wählen.",
        "Eine Kachel normal bearbeiten, oder ihre Markdown-Datei direkt bearbeiten; beide Richtungen bleiben synchron.",
      ],
      whatHappens:
        "Die Markdown-Dateien sind Klartext: wer Zugriff auf den Ordner hat, kann sie lesen, also gehören sensible Kacheln nicht in einen geteilten oder Cloud-Ordner. Eine vertrauliche Kachel wird nie dorthin geschrieben, und eine Datei ändert nie eine. Eine Kachel mit angehängter Datei, Bild oder PDF wird als Verweis auf ihren Pfad synchronisiert, ein anderer PC braucht also denselben Pfad; ein eingefügter Screenshot bleibt nur in der Datenbank. In einer Datei genannte Ansichten und Ordner werden hier angelegt, falls sie fehlen, und eine auf einem anderen PC umbenannte Ansicht wird hier mit umbenannt. Der Dateiname ist die feste Kennung der Kachel: eine von Hand umbenannte Datei ergibt eine zweite Kachel. Eine gelöschte Datei löscht ihre Kachel nicht, sie wird im nächsten Durchlauf neu geschrieben. Eine gelöschte Kachel nimmt ihre Datei mit und landet auf jedem anderen PC mit demselben Ordner im Papierkorb. Wurden beide Seiten geändert, gewinnt eine, und der unterlegene Text bleibt als Version der Kachel oder als Konfliktkopie neben der Datei erhalten. Dateien, die diese App nicht geschrieben hat, werden nie geändert oder gelöscht, und eine Datei über 8 MB wird übersprungen. Ein Ordnerwechsel fragt, ob der alte Ordner gelöscht werden soll; entfernt wird nur, was diese App geschrieben hat, und gar nichts, wenn etwas Fremdes darin liegt. Ein nicht erreichbarer Ordner pausiert den Sync nach einigen Fehlversuchen.",
      settings: ["Markdown-Ordner (Beta), Sync-Ordner: Expertenmenü → System & Daten → Daten & Wartung"],
    },
    {
      section: "data",
      title: "Papierkorb & Kacheln wiederherstellen",
      whatItIs: "Eine gelöschte Kachel wandert zuerst in den Papierkorb, nicht direkt in die endgültige Löschung.",
      howTo: [
        "Eine Kachel wie gewohnt löschen: sie landet im Papierkorb, Rückgängig holt sie zurück.",
        "Den Papierkorb aus der Daten-Gruppe des Expertenmenüs öffnen.",
        "Eine Kachel dort wiederherstellen oder sie durch die Aufbewahrungsdauer automatisch löschen lassen.",
      ],
      whatHappens:
        "Die Aufbewahrungsdauer wird in Tagen festgelegt; null bedeutet für immer behalten. Das Wiederherstellen setzt die Kachel in die erste freie Zelle der aktiven Ansicht, nicht zwingend in die ursprüngliche Ansicht oder Zelle. Schwebender Button, Ordner-Mitgliedschaft und Hotkey von vorher kommen nicht zurück; die müssen neu eingerichtet werden.",
      settings: ["Papierkorb-Aufbewahrung: Expertenmenü → System & Daten → Daten & Wartung"],
    },
    {
      section: "data",
      title: "Rückgängig-Button",
      whatItIs: "Einige Aktionen außerhalb des Papierkorbs bieten direkt in ihrer Bestätigungsmeldung ein einstufiges Rückgängig.",
      howTo: [
        "Eine Ansicht löschen, den Kopierverlauf leeren, einen Tag löschen oder mehrere Einträge auf einmal löschen oder taggen.",
        "In der erscheinenden Meldung auf Rückgängig klicken.",
      ],
      whatHappens:
        "Nur die letzte Aktion lässt sich zurücknehmen; eine neuere ersetzt sie, und der Button verschwindet mit der Meldung. Steht die Rückgängig-Zeit auf Aus, fragen das Löschen einer Ansicht und das Zusammenführen von Tags vorher nach.",
      settings: ["Rückgängig-Button, wie lange die Meldung bleibt: Expertenmenü → System & Daten → Daten & Wartung"],
    },
    {
      section: "data",
      title: "Duplikat-Finder",
      whatItIs: "Der Duplikat-Finder markiert textlich sehr ähnliche Kacheln, ohne selbst etwas zu löschen.",
      howTo: [
        "In der Bibliothek auf „Duplikate finden“ klicken.",
        "Jedes markierte Paar Seite an Seite prüfen.",
        "Eine Kachel behalten, oder das Paar als kein Duplikat markieren, damit es nicht erneut markiert wird.",
      ],
      whatHappens:
        "Der Abgleich vergleicht den Haupttext jeder Kachel (oder ihren Namen, falls sie keinen Text hat) nach Ähnlichkeit, nicht nach exakter Übereinstimmung; der Schwellenwert steuert, wie nah als nah genug gilt. Eine Kachel mit Rotation wird nur nach ihrem ersten Text verglichen. Der einzige Zugang ist der Button „Duplikate finden“ in der Bibliothek; das Expertenmenü enthält nur den Schalter und die Schwelle.",
      settings: ["Duplikat-Finder, Ähnlichkeitsschwelle: Expertenmenü → System & Daten → Daten & Wartung"],
    },
    {
      section: "data",
      title: "Nutzungsstatistik",
      whatItIs: "Ein kleines Fenster mit der Zahl der Prompts, den Kopien der letzten 7 Tage, den meistgenutzten und den nie genutzten Prompts.",
      howTo: [
        "Im Expertenmenü unter „Daten & Wartung“ „Statistik anzeigen“ einschalten.",
        "Einstellungen öffnen und „Nutzungsstatistik“ anklicken.",
        "Wählen, wie viele Prompts die Liste der meistgenutzten zeigt: 5, 10 oder 20.",
        "Einen nie genutzten Prompt anklicken, um ihn in der Bibliothek zu finden.",
      ],
      whatHappens:
        "Alles bleibt auf diesem PC. Der Export-Button speichert die vollständigen Zahlen samt Tageswerten und Werkzeugnutzung als CSV, TXT oder JSON.",
      settings: [],
    },
    {
      section: "data",
      title: "Datenwartung",
      whatItIs: "Die Datenwartung bündelt Wartungswerkzeuge im Expertenmenü.",
      howTo: [
        "Im Expertenmenü unter System & Daten die Gruppe Daten & Wartung öffnen.",
        "Von dort den Papierkorb öffnen, nur die Einstellungen exportieren oder importieren, oder „Letzte Meldungen“ öffnen.",
      ],
      whatHappens:
        "„Letzte Meldungen“ hält eine kurze Liste der Fehlermeldungen fest, die nur kurz erschienen, mit Zeit und Ort; die Liste lässt sich in ihrem Fenster kopieren oder leeren. Die Einstellungen allein lassen sich in eine Datei speichern und wieder laden. Vor einem Zurücksetzen oder Alles-Löschen kann die App zuerst eine Sicherung anlegen.",
      settings: [
        "Papierkorb-Aufbewahrung, letzte Meldungen, Einstellungen exportieren und importieren, Rückgängig-Button: Expertenmenü → System & Daten → Daten & Wartung",
        "Vor einem Zurücksetzen oder Alles-Löschen eine Sicherung anlegen: Expertenmenü → System & Daten → Daten & Wartung",
        "Wie viel die letzten Meldungen behalten: Expertenmenü → System & Daten → Fehlersuche",
      ],
    },
  ],
};
