// 97: in-app documentation. EN + DE only (explicit scope decision, see WORK.md) —
// every other language falls back to English. Kept out of i18n.js on purpose: that
// file's parity check (tests/i18n_check.js) requires every key in all 11 languages,
// which this content deliberately does not follow.
//
// Mirrored (structure, not word-for-word) in docs/help.md — keep both in sync when
// a category changes.

export const HELP_CONTENT = {
  en: [
    {
      section: "basics",
      title: "Getting started",
      whatItIs: "A tile is one saved prompt, placed on the grid; clicking it copies its text to the clipboard.",
      howTo: [
        "Double-click an empty grid cell, or the + button, to create one.",
        "Type or paste the text, then Save.",
        "Click any tile to copy it.",
      ],
      whatHappens:
        "Three example tiles (an email with placeholders, an image, and a Markdown template) ship on first launch — a button above the grid removes them with one click once you don't need them anymore.",
      settings: [],
    },
    {
      section: "basics",
      title: "Creating prompts",
      whatItIs: "Beyond plain text, a prompt can carry an attached file, a screenshot, or a picked screen colour.",
      howTo: [
        "File attach — clicking the tile copies the file itself, not text.",
        "Screenshot tool — snip a region or a whole window straight into a new prompt.",
        "Colour picker — pick any on-screen pixel, copies as hex/RGB/HSL.",
        "Drag & drop — drop a file or image straight onto the grid.",
      ],
      whatHappens: "The attached file, screenshot or colour becomes the prompt's content — copying it later copies that file, image or colour, not text.",
      settings: ["File attach, screenshot tool, colour picker, drag & drop — Expert menu → Features → Create prompts"],
      tip: "The colour picker (pipette) is off by default.",
    },
    {
      section: "basics",
      title: "Variables & placeholders",
      whatItIs:
        "Write {#{name}#} anywhere in a prompt's text — copying it opens a small popup asking for each placeholder's value before the filled-in text reaches the clipboard.",
      howTo: [
        "Type {#{PlaceholderName}#} inside the prompt text.",
        "Click the tile to copy — the fill-in popup opens automatically.",
        "Enter a value per placeholder (or pick one, for choice placeholders) and confirm.",
      ],
      whatHappens:
        "Only the filled-in text reaches the clipboard — the raw {#{...}#} markup is never copied. Dynamic placeholders (date/time/clipboard/uuid) fill themselves in with no popup at all.",
      settings: [
        "Dynamic placeholders, choice placeholders, remember choices, popup in its own window — Expert menu → Features → Placeholders",
      ],
    },
    {
      section: "basics",
      title: "The editor",
      whatItIs: "The prompt editor is a plain text box by default; a few opt-in features add structure and history to it.",
      howTo: [
        "Open a prompt's editor (double-click a tile, or Edit from its menu).",
        "Turn on Markdown preview, syntax highlighting, version history or private notes in the expert menu if you want them.",
      ],
      whatHappens:
        "Version history keeps every saved change so any earlier version can be restored. Markdown preview renders next to (or instead of) the text box without altering what's actually stored. Private notes are never copied or exported with the prompt text.",
      settings: ["Markdown preview, syntax highlighting, version history, private notes — Expert menu → Features → Editor"],
      tip: "Markdown preview and private notes both ship off.",
    },
    {
      section: "copying",
      title: "Copying",
      whatItIs: "What happens between clicking a tile and the text landing on the clipboard.",
      howTo: [
        "Click a tile once to copy.",
        "Click again quickly (or press Enter) to also paste into the app you were just in, if auto-paste is on.",
        "Ctrl-drag a tile onto another window to drop its text directly, no clipboard round-trip.",
      ],
      whatHappens:
        "A short confirmation bubble or flash confirms the copy. Chaining (clicking several tiles in a row) combines them into one clipboard entry instead of overwriting each other.",
      settings: ["Prompt chaining, per-prompt hotkeys, auto-paste, drag-out, close-on-copy — Expert menu → Features → Copying"],
    },
    {
      section: "alwaysOnTop",
      title: "Floating buttons & folders",
      whatItIs:
        "Floating buttons (pills) are small always-on-top copy buttons that sit outside the main window; grid folders group tiles inside the main grid the same way pill folders group pills.",
      howTo: [
        "Right-click a tile → \"Toggle floating button\" to pin it as a pill.",
        "Drag the pill to move it; right-click it for size, edit or remove.",
        "Drag one floating button onto another to merge them into a pill folder.",
        "On the main grid, drag one tile onto another to group them into a grid folder the same way — drag a member back out to free it.",
      ],
      whatHappens:
        "Positions survive app restarts. A folder (either kind) shows up to four member previews and a count; dragging a member out frees it, and the folder dissolves once one member is left. A tile can only ever belong to one grid folder at a time.",
      settings: ["Pill folders, grid folders — Expert menu → Features → Floating buttons & folders"],
      tip: "Both kinds of folder ship off.",
    },
    {
      section: "alwaysOnTop",
      title: "Tray",
      whatItIs: "The system tray icon's right-click menu offers quick access without opening the main window.",
      howTo: ["Right-click the tray icon.", "Pick a prompt from \"Recently copied\" or \"Favourites\" to copy it immediately."],
      whatHappens: "The menu copies straight from the tray — the main window never has to open for this.",
      settings: ["Recently copied section, favourites section, entry counts — Expert menu → Features → Tray menu"],
    },
    {
      section: "copying",
      title: "Clipboard watcher",
      whatItIs:
        "When switched on, everything you copy anywhere on the system (not just from this app) is logged into an inbox you can review and turn into prompts later.",
      howTo: [
        "Turn the watcher on in the expert menu.",
        "Copy text anywhere on your system as normal.",
        "Open the clipboard inbox to review captured entries and save the useful ones as prompts.",
      ],
      whatHappens:
        "Nothing is watched until you turn it on. Copies shorter than the minimum length are ignored, and the oldest entries drop once the inbox grows past its cap.",
      settings: ["Minimum length, inbox cap — Expert menu → Features → Clipboard watcher"],
      tip: "Off by default — nothing is watched until you turn it on.",
    },
    {
      section: "organising",
      title: "Tile behaviour & appearance",
      whatItIs: "How a tile reacts to the mouse, and what it looks like.",
      howTo: [
        "Right-click a tile for its menu (edit, pin, hide, delete, more).",
        "Drag a tile to any grid cell; drop on an occupied cell to swap.",
        "Turn on hover lift, shadows, gradients, badges and similar looks in the expert menu if you want them.",
      ],
      whatHappens: "These are interaction and looks only — none of them change what a tile copies.",
      settings: [
        "Tile menu, drag-to-reorder, hover lift — Expert menu → Buttons → Tiles & copying",
        "Type badges, shadows, gradients, rounded corners, bold/uppercase/monospace text — Expert menu → Buttons → Tile appearance",
      ],
    },
    {
      section: "look",
      title: "Tooltips",
      whatItIs: "Hovering a tile shows its full text, length, and (if present) its private note.",
      howTo: [
        "Hover a tile and wait for the delay to elapse.",
        "Adjust the delay, auto-hide timeout or width in the expert menu if the default doesn't fit.",
      ],
      whatHappens: "The tooltip auto-hides after its timeout even if the mouse stays put.",
      settings: ["Delay before showing, auto-hide timeout, tooltip width — Expert menu → Buttons → Tooltips"],
    },
    {
      section: "organising",
      title: "Favourites",
      whatItIs: "Star a prompt to mark it as a favourite.",
      howTo: ["Click the star on a tile, or use \"Favourite\" from its menu.", "Open the dedicated favourites view to see only starred prompts."],
      whatHappens: "Favourites get their own reorderable view, independent from wherever else the prompt lives.",
      settings: ["Favourites view, its own reordering — Expert menu → Buttons → Favorites"],
    },
    {
      section: "look",
      title: "Scaling & zoom",
      whatItIs: "Independent size sliders for different parts of the window, in case the defaults don't fit your screen or preference.",
      howTo: ["Open Settings and find the scaling sliders.", "Adjust overall UI, dialog, composer or icon scale independently."],
      whatHappens: "Each slider only affects its own part of the window — bumping icon scale, for example, does not resize dialogs.",
      settings: ["Overall UI scale, modal scale, composer scale, icon scale — Expert menu → Appearance → Sizes (%)"],
    },
    {
      section: "look",
      title: "Appearance & themes",
      whatItIs: "Eight built-in themes: system, light, dark, Programmer, Neon Violet, Ocean, Cyberpunk and Black & White.",
      howTo: ["Pick a theme from Settings.", "Open the theme editor (once enabled) to tweak any theme's colours and save your own."],
      whatHappens: "\"System\" follows the OS light/dark setting live. A custom theme is saved separately — the built-in it was based on stays untouched.",
      settings: ["Theme choice — Settings", "Theme editor — Expert menu → Appearance → Global colour palette"],
      tip: "Custom themes are an opt-in feature — the editor button appears once it's turned on.",
    },
    {
      section: "organising",
      title: "Library & search",
      whatItIs: "The library lists every prompt as a searchable, filterable list — the grid only shows what fits on screen, the library shows everything.",
      howTo: [
        "Click \"All prompts\" to open the library.",
        "Type to search, or use the type/colour filters to narrow the list.",
        "Click a row to copy, or use its menu to edit, drag onto the grid, tag or delete.",
      ],
      whatHappens: "Search is typo-tolerant (fuzzy) by default and, unless turned off, sorts by most-used or most-recently-used.",
      settings: ["Fuzzy search, smart sort, type/colour filters — Expert menu → Library & History → Library"],
    },
    {
      section: "organising",
      title: "Tags",
      whatItIs: "Free-form labels you assign to prompts; filter the library or the grid down to a set of tags.",
      howTo: [
        "Open a prompt's editor and type a tag name to add it (or pick an existing one).",
        "Use the tag filter row in the library to narrow the list to one or more tags.",
      ],
      whatHappens: "Renaming, recolouring or deleting a tag from the tag manager applies everywhere it's used, in one step.",
      settings: ["Tag filter mode — match ANY selected tag, or ALL of them — Expert menu → Library & History → Tags"],
    },
    {
      section: "copying",
      title: "Copy history",
      whatItIs: "A log of what you've copied and when — separate from the clipboard watcher, this only records copies made through this app.",
      howTo: [
        "Click \"Copy history\" to open the journal.",
        "Search, filter by type/colour, or star a favourite from the list.",
        "Delete a single row, or clear the whole history.",
      ],
      whatHappens: "Grouped view shows one row per prompt (its most recent copy) instead of every event. History older than the retention setting is dropped automatically.",
      settings: ["Grouped view, retention — Expert menu → Library & History → Copy history"],
    },
    {
      section: "organising",
      title: "Media",
      whatItIs: "Settings for prompts that carry a video or image.",
      howTo: [
        "Attach a video or image to a prompt (file attach, screenshot, paste, or drag & drop).",
        "Adjust playback and privacy-leaning looks in the expert menu.",
      ],
      whatHappens: "Video settings (autoplay, mute, loop, volume) apply to every video tile; grayscale/dim-until-hovered only changes how media looks, never what gets copied.",
      settings: ["Video controls, autoplay, mute, loop — Expert menu → Media", "Grayscale / dim until hovered — Expert menu → Media"],
    },
    {
      section: "look",
      title: "Toolbar",
      whatItIs: "Which buttons the header and composer bar show.",
      howTo: ["Right-click the header for a quick toggle list, or use the expert menu for the full set."],
      whatHappens: "Hiding a button only hides it — the feature behind it, if any, keeps working through its own hotkey or menu entry.",
      settings: ["Quick grid-size control, library/history/pin buttons, multiple views, view groups — Expert menu → System & data → Toolbar"],
    },
    {
      section: "system",
      title: "Privacy",
      whatItIs: "Options that keep this window's contents out of screenshots, screen shares, or a passer-by's glance.",
      howTo: ["Turn on screen-capture exclusion, tile/media blur, hidden tile names or dim-unhovered in the expert menu, as needed."],
      whatHappens:
        "Screen-capture exclusion makes the window invisible to screenshot and recording tools — including your own, so if a capture comes out empty, check this setting first.",
      settings: ["Exclude from screen capture, blur tiles/media, hide names, dim unhovered — Expert menu → System & data → Privacy"],
      tip: "Screen-capture exclusion ships OFF — turn it on if you want it.",
    },
    {
      section: "system",
      title: "System",
      whatItIs: "App-level behaviour that isn't about any one prompt.",
      howTo: ["Open the expert menu's System group for update checks, keyboard navigation, multiple instances and the data folder."],
      whatHappens:
        "Update checks are on by default and never install anything without asking first. Changing the data folder migrates your existing database there. Allowing multiple instances takes a restart to apply.",
      settings: ["Update checks, keyboard navigation, allow multiple instances, data folder — Expert menu → System & data → System"],
    },
    {
      section: "system",
      title: "Data maintenance",
      whatItIs: "Housekeeping tools reached from the expert menu's Data group.",
      howTo: ["Open the expert menu → System & data → Data & maintenance.", "Run the duplicate finder, open the trash, or import/export from there."],
      whatHappens:
        "A deleted prompt sits in the trash, with its own retention setting, before it's gone for good. The duplicate finder only flags near-identical prompts by text similarity — it never deletes anything on its own.",
      settings: ["Duplicate finder, trash retention, import/export — Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "system",
      title: "Standalone: Backup manager",
      whatItIs: "Encrypted, compressed snapshots of your entire prompt library and settings, on your own schedule.",
      howTo: [
        "Open the Backup manager from Settings.",
        "Turn on automatic backups and pick an interval, or create one manually.",
        "Restore from the snapshot list, choosing data only, settings only, or both.",
      ],
      whatHappens:
        "Retention tiers (daily/weekly/monthly/yearly) thin out older snapshots automatically instead of keeping every one forever. A backup is either whole or absent — a failed one leaves nothing behind. The same window also has a usage-statistics section (totals, a 14-day activity chart, most-used prompts), unrelated to the backups themselves.",
      settings: [
        "Interval, retention tiers, password, restore scope — Backup manager (its own window, opened from Settings)",
        "Usage statistics, how many prompts it lists — same window, below the backups",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Standalone: Quick launcher",
      whatItIs:
        "A small always-on-top popup (default hotkey Ctrl+Alt+P) with fuzzy search across every prompt's name and text — pick one and it copies immediately, no need to bring the main window forward.",
      howTo: ["Press Ctrl+Alt+P (or your configured hotkey) from anywhere.", "Type to search, then click or press Enter on a result to copy it."],
      whatHappens: "The launcher closes itself right after copying, so it never lingers on screen.",
      settings: [],
    },
    {
      section: "organising",
      title: "Standalone: Multiple views",
      whatItIs:
        "A view is one grid page with its own size and arrangement; a prompt can live in several views. View groups turn views into tabs instead of a dropdown.",
      howTo: [
        "Click + next to the view tabs to add a view.",
        "Right-click a tab to rename, resize, recolour or delete it.",
        "Drag one view tab onto another to merge them into a group.",
      ],
      whatHappens: "Deleting a view does not delete the prompts placed on it — a prompt also shown in another view keeps working there.",
      settings: [],
    },
    {
      section: "look",
      title: "Standalone: Theme editor",
      whatItIs: "Start from any built-in theme, change individual colours, and save the result as your own theme — the original stays untouched.",
      howTo: [
        "Open the theme editor (Expert menu → Appearance → Global colour palette, once custom themes are on).",
        "Pick a base theme, adjust colours, and save under a new name.",
      ],
      whatHappens: "A custom theme appears in the regular theme dropdown alongside the built-ins; deleting it does not affect the theme it was based on.",
      settings: [],
    },
    {
      section: "basics",
      title: "The grid",
      whatItIs: "The main window's grid is a free-placement board of prompt tiles — up to 20×20 cells by default, with its own layout per view.",
      howTo: [
        "Drag a tile to any empty cell.",
        "Drop a tile on an occupied cell to swap the two.",
        "Adjust the grid size with the quick columns/rows fields above it, or in a view's own settings.",
      ],
      whatHappens:
        "A tile keeps its cell until you move it — the grid never auto-arranges itself. Shrinking the grid does not delete tiles that fall outside the new size; they stay assigned and reappear if you grow it back. The 20×20 ceiling is only the default — the expert menu's limit setting raises it up to 200×200.",
      settings: ["Quick grid-size control, grid border — Expert menu → System & data → Toolbar", "Maximum grid size — Expert menu → Appearance → Limits & values"],
    },
    {
      section: "copying",
      title: "Hotkeys",
      whatItIs:
        "Two independent kinds of hotkey: one global key combination that opens the quick launcher, and a key combination per prompt that copies (and optionally pastes) that one prompt directly.",
      howTo: [
        "Set the global hotkey in Settings (default Ctrl+Alt+P).",
        "Give a prompt its own hotkey from its editor, and choose whether it should paste as well as copy.",
        "If a combination is already used, the field explains why — already used in this app, or held by another running app — instead of silently accepting it.",
      ],
      whatHappens:
        "A combination must include Ctrl or Alt; Shift alone, or a bare key, is refused so ordinary typing is never swallowed. The app hotkey always wins a conflict; among prompts, the first one in the list keeps a combination two prompts happen to share. Importing a backup that carries a colliding hotkey drops the losing one instead of failing the import, and the import summary says how many were dropped.",
      settings: ["Global hotkey — Settings", "Per-prompt hotkeys, paste-after-copy — Expert menu → Features → Copying"],
    },
    {
      section: "system",
      title: "Trash & restoring prompts",
      whatItIs: "A deleted prompt goes to the trash first, not straight to permanent deletion.",
      howTo: [
        "Delete a prompt as normal (a second click on the delete button confirms it).",
        "Open the trash from the expert menu's Data group.",
        "Restore a prompt from there, or let retention clear it automatically.",
      ],
      whatHappens:
        "Retention is set in days; 0 means keep forever. Restoring places the prompt in the active view's first free cell — not necessarily its original view or cell — and does not bring back a floating button, folder membership or hotkey it had before deletion; those need setting up again.",
      settings: ["Trash retention — Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "system",
      title: "Duplicate finder",
      whatItIs: "Flags prompts that are textually very similar, without deleting anything on its own.",
      howTo: [
        "Open the duplicate finder from the expert menu's Data group.",
        "Review each flagged pair side by side.",
        "Keep one, or mark the pair as not a duplicate to stop it being flagged again.",
      ],
      whatHappens:
        "Matching compares each prompt's main text (its name, if it has no text) by similarity, not an exact match — the threshold controls how close counts as close enough. A rotating prompt is only compared on its first entry.",
      settings: ["Duplicate finder, similarity threshold — Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "system",
      title: "Import & export",
      whatItIs: "Plain CSV/TXT export for the text alone, or a full backup file with everything — prompts, views, layouts, colours, styles and language.",
      howTo: [
        "Export or import from the expert menu's Data group.",
        "A CSV/TXT import shows a preview first — every incoming prompt's name and whether it would land as a duplicate — before anything is written. A full backup restores directly, from its own window.",
        "Confirm to actually import, or cancel and nothing changes.",
      ],
      whatHappens: "CSV/TXT only round-trips text; images, files, layout and settings need the full backup format instead.",
      settings: ["Import / export — Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "system",
      title: "Markdown folder sync (beta)",
      whatItIs:
        "An optional, off-by-default mirror that writes every text-only prompt as its own readable .md file in a folder you pick, alongside the encrypted database.",
      howTo: [
        "Turn it on and pick a folder in the expert menu.",
        "Edit a prompt normally, or edit its .md file directly — either direction syncs to the other.",
      ],
      whatHappens:
        "Unlike the database, the .md files are plain text — anyone with access to that folder can read the prompts, so avoid a synced or shared folder for anything sensitive. A prompt with an attached file, image or video is skipped; it never had a text-only .md form to sync.",
      settings: ["Prompt sync, sync folder — Expert menu → Features → Prompt sync"],
      tip: "This is a beta feature — keep the sync folder somewhere only you can read.",
    },
    {
      section: "basics",
      title: "Screenshot tool",
      whatItIs: "Captures part or all of your screen straight into a new copy button, without a separate screenshot app.",
      howTo: [
        "Click the camera icon.",
        "Drag a region, or click a window to capture it whole — even a normally-protected one like the Task Manager.",
        "In the confirmation dialog, save it as a button, save it as a file, or just leave it on the clipboard.",
      ],
      whatHappens:
        "The app's own windows are hidden for the moment of capture so they never end up in the shot. The result reaches the clipboard immediately; saving it as a button or file is a separate, optional step.",
      settings: ["Screenshot tool, save location, preview size — Expert menu → Features → Create prompts"],
    },
    {
      section: "basics",
      title: "Colour picker & pick history",
      whatItIs: "A pipette that freezes the screen, magnifies the pixel under your cursor, and copies it as hex, rgb or hsl.",
      howTo: [
        "Click the pipette icon (off by default — turn it on in the expert menu first).",
        "Move the loupe to the pixel you want; scroll or +/- to zoom.",
        "Click to copy the colour; right-click the pipette icon to reopen your last few picks.",
      ],
      whatHappens:
        "Nothing is ever saved to a prompt automatically — a pick only reaches the clipboard. The pick history holds your last few colours and survives a restart, capped by its own setting.",
      settings: ["Colour picker, zoom, format, pick-history length — Expert menu → Features → Create prompts"],
      tip: "The colour picker (pipette) is off by default.",
    },
    {
      section: "basics",
      title: "Rotating prompts",
      whatItIs: "A prompt can hold a list of texts instead of one — each copy serves the next entry instead of always the same text.",
      howTo: [
        "Turn rotating prompts on in the expert menu (off by default).",
        "In a prompt's editor, add more than one text entry.",
        "Choose order (cycles through in sequence) or shuffle (random, without repeating until every entry has been used once).",
        "Copy the tile as normal — it always serves the next entry, never asks which one.",
      ],
      whatHappens:
        "A small badge shows the position, like \"2/5\", on a grid tile, a floating pill, and a pill-folder member (a grid-folder member shows only its file-type icon, not this badge). The position survives restarts and full backups; a CSV/TXT import creates a new prompt with a fresh pointer, so only the list of texts and its order/shuffle mode carry over there, not the exact position. Turning rotation off on a prompt with several entries keeps them all — only the first one is used until you turn it back on.",
      settings: ["Rotating prompts, default order/shuffle, badge, max entries — Expert menu → Features → Create prompts"],
    },
    {
      section: "system",
      title: "The expert menu",
      whatItIs: "Every optional feature lives behind an on/off switch here, with its own parameters right underneath once it's on.",
      howTo: [
        "Open Settings → the expert menu.",
        "Pick a tab, or type in the search box to scan every tab at once.",
        "Turn on \"Only changed\" to see just what you've adjusted from the defaults.",
      ],
      whatHappens:
        "A feature that ships switched off carries a small dot badge on its label, with a tooltip explaining what it does. Reset works at three levels: one value's own undo icon, a whole group's reset icon in its header, or every setting at once from the general reset action.",
      settings: [],
    },
    {
      section: "system",
      title: "Keyboard-only use",
      whatItIs: "Arrow keys move a focus ring across the grid; Enter or Space copies whichever tile is focused.",
      howTo: [
        "Turn on keyboard navigation in the expert menu.",
        "Use the arrow keys to move the focus ring, Enter or Space to copy.",
        "Tab still moves through buttons and fields everywhere else in the app, as usual.",
      ],
      whatHappens: "The focus ring fades out a short while after the window loses focus, so it doesn't linger over a tile once you've moved on to another app.",
      settings: ["Keyboard navigation, fade delay — Expert menu → System & data → System"],
    },
    {
      section: "system",
      title: "Where your data lives",
      whatItIs:
        "Everything — prompts, views, settings — lives in one local SQLite database (plus its small companion WAL file); where that pair sits depends on whether you're running the portable exe or the installed version.",
      howTo: [
        "Installed: your data folder is under AppData, shown (and changeable) in the expert menu.",
        "Portable: the data folder sits right next to the exe, so the whole thing stays on a USB stick or synced folder as one unit.",
      ],
      whatHappens: "Moving from portable to installed (or back) is detected automatically and offered as a one-time migration — your existing data is never silently left behind.",
      settings: ["Data folder — Expert menu → System & data → System"],
    },
    {
      section: "system",
      title: "Updates & security",
      whatItIs: "An optional check for a newer version, on by default, that only ever talks to the update server — never anything else.",
      howTo: [
        "Leave update checks on (default) to be notified when a new version is out, or turn them off in the expert menu.",
        "Accept an offered update to download and install it with one click.",
      ],
      whatHappens:
        "The update check is the only network traffic the app makes on its own; the one exception is a first-run download of the Microsoft WebView2 installer if that runtime isn't already on your system. A downloaded app update is verified against a published checksum before it runs — a missing or mismatching checksum blocks the install instead of running an unverified file.",
      settings: ["Update checks — Expert menu → System & data → System"],
    },
    {
      section: "system",
      title: "Common questions",
      whatItIs: "Quick answers to the things that come up most.",
      howTo: [
        "\"My screenshot of this app came out empty\" — the privacy setting that excludes the window from screen capture is working as intended; turn it off in the expert menu if you don't want that.",
        "\"The app won't start\" — it needs the Microsoft WebView2 runtime; the installer offers it automatically if missing.",
        "\"I can't find a setting\" — check the expert menu's search box first; it scans every tab at once.",
        "\"A hotkey doesn't do anything\" — another app may already hold that combination; the editor explains this instead of accepting it silently.",
      ],
      settings: [],
    },
  ],
  de: [
    {
      section: "basics",
      title: "Erste Schritte",
      whatItIs: "Eine Kachel ist ein gespeicherter Prompt, der auf dem Raster platziert ist; ein Klick darauf kopiert seinen Text in die Zwischenablage.",
      howTo: [
        "Doppelklick auf eine leere Rasterzelle oder den +-Button erstellt eine Kachel.",
        "Tippe oder füge den Text ein, und klicke dann auf Speichern.",
        "Klick auf eine beliebige Kachel kopiert sie.",
      ],
      whatHappens:
        "Drei Beispiel-Kacheln (eine E-Mail mit Platzhaltern, ein Bild und eine Markdown-Vorlage) werden beim ersten Start mitgeliefert — ein Button über dem Raster entfernt sie mit einem Klick, sobald sie nicht mehr benötigt werden.",
      settings: [],
    },
    {
      section: "basics",
      title: "Prompts erstellen",
      whatItIs: "Über reinen Text hinaus kann ein Prompt eine angehängte Datei, einen Screenshot oder eine gepickte Bildschirmfarbe enthalten.",
      howTo: [
        "Dateianhang — ein Klick auf die Kachel kopiert die Datei selbst, nicht den Text.",
        "Screenshot-Tool — schneide einen Bereich oder ein ganzes Fenster direkt in einen neuen Prompt aus.",
        "Farbpipette — picke einen beliebigen Pixel auf dem Bildschirm, kopiert als Hex/RGB/HSL.",
        "Drag & Drop — ziehe eine Datei oder ein Bild direkt auf das Raster.",
      ],
      whatHappens: "Die angehängte Datei, der Screenshot oder die Farbe wird zum Inhalt des Prompts — das Kopieren kopiert später diese Datei, dieses Bild oder diese Farbe, nicht den Text.",
      settings: ["Dateianhang, Screenshot-Tool, Farbpipette, Drag & Drop — Experten-Menü → Funktionen → Prompts erstellen"],
      tip: "Die Farbpipette ist standardmäßig deaktiviert.",
    },
    {
      section: "basics",
      title: "Variablen & Platzhalter",
      whatItIs:
        "Schreibe {#{Name}#} irgendwo in den Text eines Prompts — beim Kopieren öffnet sich ein kleines Popup, das nach dem Wert für jeden Platzhalter fragt, bevor der ausgefüllte Text in die Zwischenablage gelangt.",
      howTo: [
        "Tippe {#{PlatzhalterName}#} in den Text des Prompts.",
        "Klick auf die Kachel zum Kopieren — das Popup zum Ausfüllen öffnet sich automatisch.",
        "Gib einen Wert pro Platzhalter ein (oder wähle einen bei Auswahl-Platzhaltern) und bestätige.",
      ],
      whatHappens:
        "Nur der ausgefüllte Text erreicht die Zwischenablage — das rohe {#{...}#}-Markup wird niemals kopiert. Dynamische Platzhalter (Datum/Uhrzeit/Zwischenablage/UUID) füllen sich von selbst ohne Popup aus.",
      settings: [
        "Dynamische Platzhalter, Auswahl-Platzhalter, Auswahl merken, Popup im eigenen Fenster — Experten-Menü → Funktionen → Variablen",
      ],
    },
    {
      section: "basics",
      title: "Der Editor",
      whatItIs: "Der Prompt-Editor ist standardmäßig ein einfaches Textfeld; einige optionale Funktionen verleihen ihm Struktur und einen Verlauf.",
      howTo: [
        "Öffne den Editor eines Prompts (Doppelklick auf eine Kachel oder Bearbeiten in ihrem Menü).",
        "Aktiviere Markdown-Vorschau, Syntax-Hervorhebung, Versionsverlauf oder private Notizen im Experten-Menü, falls gewünscht.",
      ],
      whatHappens:
        "Der Versionsverlauf speichert jede Änderung, sodass jede frühere Version wiederhergestellt werden kann. Die Markdown-Vorschau wird neben (oder anstelle) des Textfelds gerendert, ohne das zu verändern, was tatsächlich gespeichert wird. Private Notizen werden niemals kopiert oder mit dem Prompt-Text exportiert.",
      settings: ["Markdown-Vorschau, Syntax-Hervorhebung, Versionsverlauf, private Notizen — Experten-Menü → Funktionen → Editor"],
      tip: "Markdown-Vorschau und private Notizen sind standardmäßig deaktiviert.",
    },
    {
      section: "copying",
      title: "Kopieren",
      whatItIs: "Was passiert, zwischen dem Klick auf eine Kachel und dem Eintreffen des Textes in der Zwischenablage.",
      howTo: [
        "Einmaliger Klick auf eine Kachel kopiert sie.",
        "Schneller zweiter Klick (oder Enter) fügt den Text auch direkt in die zuletzt aktive App ein, falls Auto-Paste aktiviert ist.",
        "Strg-Ziehe eine Kachel auf ein anderes Fenster, um ihren Text direkt abzulegen, ohne Umweg über die Zwischenablage.",
      ],
      whatHappens:
        "Eine kurze Bestätigungsblase oder ein Aufblinken bestätigt den Kopiervorgang. Chaining (Klicken auf mehrere Kacheln hintereinander) fasst sie zu einem Eintrag in der Zwischenablage zusammen, anstatt sie gegenseitig zu überschreiben.",
      settings: ["Prompt-Chaining, eigene Hotkeys pro Prompt, Auto-Paste, Drag-out, Schließen-nach-Kopieren — Experten-Menü → Funktionen → Kopieren"],
    },
    {
      section: "alwaysOnTop",
      title: "Schwebende Buttons & Ordner",
      whatItIs:
        "Schwebende Buttons (Pillen) sind kleine Kopier-Buttons, die immer im Vordergrund sind und sich außerhalb des Hauptfensters befinden; Raster-Ordner gruppieren Kacheln im Hauptfenster auf dieselbe Weise, wie Pillen-Ordner Pillen gruppieren.",
      howTo: [
        "Rechtsklick auf eine Kachel → \"Schwebenden Button umschalten\", um sie als Pille anzuheften.",
        "Ziehe die Pille, um sie zu bewegen; Rechtsklick darauf für Größe, Bearbeiten oder Entfernen.",
        "Ziehe einen schwebenden Button auf einen anderen, um sie zu einem Pillen-Ordner zu verbinden.",
        "Ziehe im Hauptfenster eine Kachel auf eine andere, um sie auf dieselbe Weise in einem Raster-Ordner zu gruppieren — ziehe ein Mitglied wieder heraus, um es freizugeben.",
      ],
      whatHappens:
        "Positionen bleiben nach App-Neustarts erhalten. Ein Ordner (beider Arten) zeigt bis zu vier Vorschauen seiner Mitglieder und eine Anzahl; das Herausziehen eines Mitglieds gibt es frei, und der Ordner löst sich auf, sobald nur noch ein Mitglied übrig ist. Eine Kachel kann immer nur zu einem Raster-Ordner gleichzeitig gehören.",
      settings: ["Pillen-Ordner, Raster-Ordner — Experten-Menü → Funktionen → Schwebende Buttons & Ordner"],
      tip: "Beide Ordnerarten sind standardmäßig deaktiviert.",
    },
    {
      section: "alwaysOnTop",
      title: "Taskleistenmenü",
      whatItIs: "Das Rechtsklick-Menü des Taskleisten-Icons (System Tray) bietet schnellen Zugriff, ohne das Hauptfenster öffnen zu müssen.",
      howTo: ["Rechtsklick auf das Taskleisten-Icon.", "Wähle einen Prompt aus \"Zuletzt kopiert\" oder \"Favoriten\", um ihn sofort zu kopieren."],
      whatHappens: "Das Menü kopiert direkt aus der Taskleiste — das Hauptfenster muss dafür nicht geöffnet werden.",
      settings: ["Bereich für zuletzt Kopiertes, Bereich für Favoriten, Anzahl der Einträge — Experten-Menü → Funktionen → Taskleistenmenü"],
    },
    {
      section: "copying",
      title: "Zwischenablage-Überwachung",
      whatItIs:
        "Wenn aktiviert, wird alles, was irgendwo auf dem System kopiert wird (nicht nur aus dieser App), in einem Posteingang protokolliert. Diese Einträge lassen sich überprüfen und später in Prompts umwandeln.",
      howTo: [
        "Schalte die Überwachung im Experten-Menü ein.",
        "Kopiere ganz normal Text irgendwo auf dem System.",
        "Öffne den Posteingang der Zwischenablage, um die erfassten Einträge zu überprüfen und die nützlichen als Prompts zu speichern.",
      ],
      whatHappens:
        "Nichts wird überwacht, bis es eingeschaltet wird. Kopien, die kürzer als die Mindestlänge sind, werden ignoriert, und die ältesten Einträge fallen weg, sobald der Posteingang seine Kapazität überschreitet.",
      settings: ["Mindestlänge, Posteingang-Kapazität — Experten-Menü → Funktionen → Zwischenablage-Überwachung"],
      tip: "Standardmäßig aus — nichts wird überwacht, bis es eingeschaltet wird.",
    },
    {
      section: "organising",
      title: "Kachel-Verhalten & -Aussehen",
      whatItIs: "Wie eine Kachel auf die Maus reagiert und wie sie aussieht.",
      howTo: [
        "Rechtsklick auf eine Kachel für ihr Menü (Bearbeiten, Anheften, Verbergen, Löschen, Mehr).",
        "Ziehe eine Kachel auf eine beliebige Rasterzelle; lass sie auf einer besetzten Zelle los, um zu tauschen.",
        "Aktiviere Hover-Lift, Schatten, Farbverläufe, Badges und ähnliche Optiken im Experten-Menü, falls gewünscht.",
      ],
      whatHappens: "Das sind nur Interaktionen und Optik — keine davon ändert, was eine Kachel kopiert.",
      settings: [
        "Kachel-Menü, Drag-to-Reorder, Hover-Lift — Experten-Menü → Buttons → Kacheln & Kopieren",
        "Typ-Badges, Schatten, Farbverläufe, abgerundete Ecken, fetter/großgeschriebener/monospaced Text — Experten-Menü → Buttons → Kachel-Aussehen",
      ],
    },
    {
      section: "look",
      title: "Tooltips",
      whatItIs: "Das Überfahren einer Kachel mit der Maus zeigt ihren vollständigen Text, ihre Länge und (falls vorhanden) ihre private Notiz.",
      howTo: [
        "Fahre über eine Kachel und warte, bis die Verzögerung abgelaufen ist.",
        "Passe die Verzögerung, das automatische Ausblenden oder die Breite im Experten-Menü an, wenn die Standardwerte nicht passen.",
      ],
      whatHappens: "Der Tooltip wird nach dem Timeout automatisch ausgeblendet, selbst wenn die Maus stehen bleibt.",
      settings: ["Verzögerung vor Anzeige, automatisches Ausblenden, Tooltip-Breite — Experten-Menü → Buttons → Tooltips"],
    },
    {
      section: "organising",
      title: "Favoriten",
      whatItIs: "Versehe einen Prompt mit einem Stern, um ihn als Favorit zu markieren.",
      howTo: ["Klicke auf den Stern auf einer Kachel oder verwende \"Favorisieren\" in ihrem Menü.", "Öffne die dedizierte Favoriten-Ansicht, um nur markierte Prompts zu sehen."],
      whatHappens: "Favoriten erhalten ihre eigene sortierbare Ansicht, unabhängig davon, wo der Prompt sonst noch liegt.",
      settings: ["Favoriten-Ansicht, eigene Sortierung — Experten-Menü → Buttons → Favoriten"],
    },
    {
      section: "look",
      title: "Skalierung & Zoom",
      whatItIs: "Unabhängige Größenregler für verschiedene Teile des Fensters, falls die Standardwerte nicht zum Bildschirm oder den eigenen Vorlieben passen.",
      howTo: ["Öffne die Einstellungen und finde die Skalierungs-Regler.", "Passe die Skalierung der gesamten UI, der Dialoge, des Editors oder der Icons unabhängig voneinander an."],
      whatHappens: "Jeder Regler beeinflusst nur seinen eigenen Teil des Fensters — wenn beispielsweise die Icon-Skalierung hochgesetzt wird, werden Dialoge nicht vergrößert.",
      settings: ["Gesamte UI-Skalierung, Dialog-Skalierung, Editor-Skalierung, Icon-Skalierung — Experten-Menü → Aussehen → Größen (%)"],
    },
    {
      section: "look",
      title: "Aussehen & Themes",
      whatItIs: "Acht integrierte Themes: System, Hell, Dunkel, Programmer, Neon Violet, Ocean, Cyberpunk und Black & White.",
      howTo: ["Wähle ein Theme in den Einstellungen.", "Öffne den Theme-Editor (sobald aktiviert), um die Farben eines Themes anzupassen und ein eigenes zu speichern."],
      whatHappens: "\"System\" folgt live der Hell-/Dunkel-Einstellung des Betriebssystems. Ein benutzerdefiniertes Theme wird separat gespeichert — das integrierte, auf dem es basierte, bleibt unangetastet.",
      settings: ["Theme-Wahl — Einstellungen", "Theme-Editor — Experten-Menü → Aussehen → Globale Farbauswahl"],
      tip: "Benutzerdefinierte Themes sind eine optionale Funktion — der Editor-Button erscheint, sobald sie aktiviert ist.",
    },
    {
      section: "organising",
      title: "Bibliothek & Suche",
      whatItIs: "Die Bibliothek listet jeden Prompt als durchsuchbare, filterbare Liste auf — das Raster zeigt nur das, was auf den Bildschirm passt, die Bibliothek zeigt alles.",
      howTo: [
        "Klick auf \"Alle Prompts\" öffnet die Bibliothek.",
        "Tippe, um zu suchen, oder nutze die Typ-/Farb-Filter, um die Liste einzugrenzen.",
        "Klicke auf eine Zeile zum Kopieren oder nutze das Menü, um sie zu bearbeiten, auf das Raster zu ziehen, zu taggen oder zu löschen.",
      ],
      whatHappens: "Die Suche ist standardmäßig fehlertolerant (Fuzzy) und sortiert, falls nicht deaktiviert, nach den am häufigsten oder am kürzlichsten verwendeten.",
      settings: ["Fuzzy-Suche, clevere Sortierung, Typ-/Farb-Filter — Experten-Menü → Bibliothek & Verlauf → Bibliothek"],
    },
    {
      section: "organising",
      title: "Tags",
      whatItIs: "Freitext-Etiketten, die Prompts zugewiesen werden; filtere die Bibliothek oder das Raster auf eine Reihe von Tags herunter.",
      howTo: [
        "Öffne den Editor eines Prompts und tippe einen Tag-Namen, um ihn hinzuzufügen (oder wähle einen vorhandenen aus).",
        "Nutze die Tag-Filterleiste in der Bibliothek, um die Liste auf ein oder mehrere Tags einzugrenzen.",
      ],
      whatHappens: "Das Umbenennen, Umfärben oder Löschen eines Tags im Tag-Manager wird überall angewendet, wo er genutzt wird, in einem Schritt.",
      settings: ["Tag-Filter-Modus — finde JEDEN ausgewählten Tag, oder ALLE — Experten-Menü → Bibliothek & Verlauf → Tags"],
    },
    {
      section: "copying",
      title: "Kopierverlauf",
      whatItIs: "Ein Protokoll dessen, was wann kopiert wurde — getrennt von der Zwischenablage-Überwachung zeichnet dies nur Kopien auf, die über diese App gemacht wurden.",
      howTo: [
        "Klick auf \"Kopierverlauf\" öffnet das Journal.",
        "Suche, filtere nach Typ/Farbe, oder markiere einen Favoriten aus der Liste.",
        "Lösche eine einzelne Zeile oder leere den gesamten Verlauf.",
      ],
      whatHappens: "Die gruppierte Ansicht zeigt eine Zeile pro Prompt (seine letzte Kopie) anstatt jedes Ereignisses. Verlauf, der älter ist als die Aufbewahrungs-Einstellung, wird automatisch gelöscht.",
      settings: ["Gruppierte Ansicht, Aufbewahrungsdauer — Experten-Menü → Bibliothek & Verlauf → Kopierverlauf"],
    },
    {
      section: "organising",
      title: "Medien",
      whatItIs: "Einstellungen für Prompts, die ein Video oder Bild enthalten.",
      howTo: [
        "Hänge ein Video oder Bild an einen Prompt an (Dateianhang, Screenshot, Einfügen oder Drag & Drop).",
        "Passe Wiedergabe und privatsphärefreundliche Optiken im Experten-Menü an.",
      ],
      whatHappens: "Video-Einstellungen (Autoplay, Stumm, Schleife, Lautstärke) gelten für jede Video-Kachel; Graustufen/Abgedunkelt-bis-Hover ändert nur das Aussehen der Medien, niemals das, was kopiert wird.",
      settings: ["Video-Steuerung, Autoplay, Stumm, Schleife — Experten-Menü → Medien", "Graustufen / abgedunkelt bis gehovert — Experten-Menü → Medien"],
    },
    {
      section: "look",
      title: "Werkzeugleiste",
      whatItIs: "Welche Buttons der Kopfbereich und die Composer-Leiste anzeigen.",
      howTo: ["Rechtsklick auf den Kopfbereich für eine schnelle Umschaltliste, oder nutze das Experten-Menü für das komplette Set."],
      whatHappens: "Das Ausblenden eines Buttons blendet ihn nur aus — die dahinterliegende Funktion (falls vorhanden) funktioniert weiterhin über ihren eigenen Hotkey oder Menüeintrag.",
      settings: ["Schnelle Rastergrößen-Steuerung, Bibliothek/Verlauf/Anheft-Buttons, mehrere Ansichten, Ansichtsgruppen — Experten-Menü → System & Daten → Werkzeugleiste"],
    },
    {
      section: "system",
      title: "Datenschutz",
      whatItIs: "Optionen, die den Inhalt dieses Fensters aus Screenshots, Screen-Shares oder vor dem Blick von Passanten fernhalten.",
      howTo: ["Aktiviere im Experten-Menü bei Bedarf den Ausschluss von Bildschirmaufnahmen, Unschärfe für Kacheln/Medien, versteckte Kachelnamen oder abgedunkelte inaktive Elemente."],
      whatHappens:
        "Der Ausschluss von Bildschirmaufnahmen macht das Fenster für Screenshot- und Aufnahmeprogramme unsichtbar — auch für die eigenen. Wenn also eine Aufnahme leer bleibt, überprüfe zuerst diese Einstellung.",
      settings: ["Von Bildschirmaufnahmen ausschließen, Kacheln/Medien unscharf machen, Namen verbergen, Ungehoverte abdunkeln — Experten-Menü → System & Daten → Datenschutz"],
      tip: "Der Ausschluss von Bildschirmaufnahmen ist standardmäßig AUS — schalte ihn ein, falls gewünscht.",
    },
    {
      section: "system",
      title: "System",
      whatItIs: "App-weites Verhalten, das sich nicht auf einen einzelnen Prompt bezieht.",
      howTo: ["Öffne die System-Gruppe des Experten-Menüs für Update-Checks, Tastaturnavigation, mehrere Instanzen und den Daten-Ordner."],
      whatHappens:
        "Update-Checks sind standardmäßig aktiv und installieren niemals etwas, ohne vorher zu fragen. Wenn der Daten-Ordner geändert wird, wird die bestehende Datenbank dorthin migriert. Das Zulassen mehrerer Instanzen erfordert einen Neustart, um wirksam zu werden.",
      settings: ["Update-Checks, Tastaturnavigation, mehrere Instanzen erlauben, Daten-Ordner — Experten-Menü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Datenwartung",
      whatItIs: "Wartungswerkzeuge, die aus der Daten-Gruppe des Experten-Menüs erreichbar sind.",
      howTo: ["Öffne das Experten-Menü → System & Daten → Daten & Wartung.", "Führe den Duplikat-Finder aus, öffne den Papierkorb, oder importiere/exportiere von dort."],
      whatHappens:
        "Ein gelöschter Prompt liegt im Papierkorb mit seiner eigenen Aufbewahrungsdauer, bevor er endgültig weg ist. Der Duplikat-Finder markiert nur nahezu identische Prompts durch Textähnlichkeit — er löscht von sich aus niemals etwas.",
      settings: ["Duplikat-Finder, Papierkorb-Aufbewahrung, Import/Export — Experten-Menü → System & Daten → Daten & Wartung"],
    },
    {
      section: "system",
      title: "Eigenständig: Backup-Manager",
      whatItIs: "Verschlüsselte, komprimierte Snapshots der gesamten Prompt-Bibliothek und Einstellungen nach eigenem Zeitplan.",
      howTo: [
        "Öffne den Backup-Manager in den Einstellungen.",
        "Aktiviere automatische Backups und wähle ein Intervall, oder erstelle manuell eines.",
        "Stelle aus der Snapshot-Liste wieder her und wähle nur Daten, nur Einstellungen oder beides.",
      ],
      whatHappens:
        "Aufbewahrungsstufen (täglich/wöchentlich/monatlich/jährlich) dünnen ältere Snapshots automatisch aus, anstatt jeden für immer zu behalten. Ein Backup ist entweder ganz oder gar nicht vorhanden — ein fehlgeschlagenes hinterlässt nichts. Dasselbe Fenster enthält auch einen Bereich für Nutzungsstatistiken (Gesamtzahlen, ein 14-Tage-Aktivitätsdiagramm, meistgenutzte Prompts), unabhängig von den Backups selbst.",
      settings: [
        "Intervall, Aufbewahrungsstufen, Passwort, Wiederherstellungsumfang — Backup-Manager (eigenes Fenster, geöffnet aus Einstellungen)",
        "Nutzungsstatistiken, wie viele Prompts dort gelistet werden — selbes Fenster, unter den Backups",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Eigenständig: Schneller Launcher",
      whatItIs:
        "Ein kleines, immer im Vordergrund befindliches Popup (Standard-Hotkey Strg+Alt+P) mit Fuzzy-Suche über den Namen und Text jedes Prompts — wähle einen aus und er wird sofort kopiert, ohne das Hauptfenster in den Vordergrund holen zu müssen.",
      howTo: ["Drücke von überall Strg+Alt+P (oder den konfigurierten Hotkey).", "Tippe zur Suche, und klicke dann oder drücke Enter auf einem Ergebnis, um es zu kopieren."],
      whatHappens: "Der Launcher schließt sich direkt nach dem Kopieren selbst, sodass er nie auf dem Bildschirm verbleibt.",
      settings: [],
    },
    {
      section: "organising",
      title: "Eigenständig: Mehrere Ansichten",
      whatItIs:
        "Eine Ansicht ist eine Rasterseite mit eigener Größe und Anordnung; ein Prompt kann in mehreren Ansichten existieren. Ansichtsgruppen verwandeln Ansichten in Tabs statt eines Dropdowns.",
      howTo: [
        "Klick auf + neben den Ansichts-Tabs fügt eine Ansicht hinzu.",
        "Rechtsklick auf einen Tab, um ihn umzubenennen, in der Größe zu ändern, umzufärben oder zu löschen.",
        "Ziehe einen Ansichts-Tab auf einen anderen, um sie zu einer Gruppe zusammenzufassen.",
      ],
      whatHappens: "Das Löschen einer Ansicht löscht nicht die darauf platzierten Prompts — ein Prompt, der auch in einer anderen Ansicht angezeigt wird, funktioniert dort weiterhin.",
      settings: [],
    },
    {
      section: "look",
      title: "Eigenständig: Theme-Editor",
      whatItIs: "Beginne mit einem beliebigen integrierten Theme, ändere einzelne Farben und speichere das Ergebnis als eigenes Theme — das Original bleibt unangetastet.",
      howTo: [
        "Öffne den Theme-Editor (Experten-Menü → Aussehen → Globale Farbauswahl, sobald benutzerdefinierte Themes aktiviert sind).",
        "Wähle ein Basis-Theme, passe Farben an, und speichere es unter einem neuen Namen.",
      ],
      whatHappens: "Ein benutzerdefiniertes Theme erscheint im regulären Theme-Dropdown neben den integrierten; wenn es gelöscht wird, betrifft das nicht das Theme, auf dem es basierte.",
      settings: [],
    },
    {
      section: "basics",
      title: "Das Raster",
      whatItIs: "Das Raster des Hauptfensters ist eine frei platzierbare Tafel für Prompt-Kacheln — bis zu 20×20 Zellen standardmäßig, mit eigenem Layout pro Ansicht.",
      howTo: [
        "Ziehe eine Kachel in eine beliebige leere Zelle.",
        "Lass eine Kachel auf einer besetzten Zelle los, um beide zu tauschen.",
        "Passe die Rastergröße mit den Schnell-Feldern für Spalten/Zeilen darüber oder in den eigenen Einstellungen einer Ansicht an.",
      ],
      whatHappens:
        "Eine Kachel behält ihre Zelle, bis sie bewegt wird — das Raster ordnet sich niemals selbst neu. Das Verkleinern des Rasters löscht keine Kacheln, die außerhalb der neuen Größe fallen; sie bleiben zugewiesen und erscheinen wieder, wenn es wieder vergrößert wird. Die Obergrenze von 20×20 ist nur der Standardwert — die Limit-Einstellung im Experten-Menü hebt dies auf bis zu 200×200 an.",
      settings: ["Schnelle Rastergrößen-Steuerung, Rasterrahmen — Experten-Menü → System & Daten → Werkzeugleiste", "Maximale Rastergröße — Experten-Menü → Aussehen → Grenzwerte & Werte"],
    },
    {
      section: "copying",
      title: "Hotkeys",
      whatItIs:
        "Zwei unabhängige Arten von Hotkeys: eine globale Tastenkombination, die den schnellen Launcher öffnet, und eine Tastenkombination pro Prompt, die diesen einen Prompt direkt kopiert (und optional einfügt).",
      howTo: [
        "Stelle den globalen Hotkey in den Einstellungen ein (Standard Strg+Alt+P).",
        "Gib einem Prompt seinen eigenen Hotkey in seinem Editor und wähle, ob er nach dem Kopieren auch einfügen soll.",
        "Wenn eine Kombination bereits vergeben ist, erklärt das Feld den Grund — bereits in dieser App verwendet oder von einer anderen laufenden App belegt —, anstatt sie stillschweigend zu akzeptieren.",
      ],
      whatHappens:
        "Eine Kombination muss Strg oder Alt enthalten; Shift alleine oder eine bloße Taste wird abgelehnt, damit normales Tippen niemals verschluckt wird. Der App-Hotkey gewinnt bei einem Konflikt immer; bei Prompts behält der erste in der Liste eine Kombination, die zwei Prompts zufällig teilen. Der Import eines Backups, das einen kollidierenden Hotkey enthält, verwirft den verlierenden, anstatt den Import abzubrechen, und die Import-Zusammenfassung gibt an, wie viele verworfen wurden.",
      settings: ["Globaler Hotkey — Einstellungen", "Eigene Hotkeys pro Prompt, Einfügen nach Kopieren — Experten-Menü → Funktionen → Kopieren"],
    },
    {
      section: "system",
      title: "Papierkorb & Prompts wiederherstellen",
      whatItIs: "Ein gelöschter Prompt wandert zuerst in den Papierkorb, nicht direkt in die dauerhafte Löschung.",
      howTo: [
        "Lösche einen Prompt wie gewohnt (ein zweiter Klick auf den Löschen-Button bestätigt es).",
        "Öffne den Papierkorb aus der Daten-Gruppe des Experten-Menüs.",
        "Stelle dort einen Prompt wieder her oder lass ihn durch die Aufbewahrungsdauer automatisch löschen.",
      ],
      whatHappens:
        "Die Aufbewahrungsdauer wird in Tagen festgelegt; 0 bedeutet für immer behalten. Das Wiederherstellen platziert den Prompt in die erste freie Zelle der aktiven Ansicht — nicht zwangsläufig seine ursprüngliche Ansicht oder Zelle — und bringt keinen schwebenden Button, keine Ordner-Mitgliedschaft oder keinen Hotkey zurück, den er vor dem Löschen hatte; diese müssen neu eingerichtet werden.",
      settings: ["Papierkorb-Aufbewahrung — Experten-Menü → System & Daten → Daten & Wartung"],
    },
    {
      section: "system",
      title: "Duplikat-Finder",
      whatItIs: "Markiert Prompts, die textuell sehr ähnlich sind, ohne von sich aus etwas zu löschen.",
      howTo: [
        "Öffne den Duplikat-Finder aus der Daten-Gruppe des Experten-Menüs.",
        "Überprüfe jedes markierte Paar Seite an Seite.",
        "Behalte einen, oder markiere das Paar als kein Duplikat, damit es nicht noch einmal markiert wird.",
      ],
      whatHappens:
        "Der Abgleich vergleicht den Haupttext (oder den Namen, wenn er keinen Text hat) jedes Prompts nach Ähnlichkeit, nicht nach exakter Übereinstimmung — der Schwellenwert steuert, wie nah als nah genug gilt. Ein wechselnder Prompt wird nur nach seinem ersten Eintrag verglichen.",
      settings: ["Duplikat-Finder, Ähnlichkeitsschwelle — Experten-Menü → System & Daten → Daten & Wartung"],
    },
    {
      section: "system",
      title: "Import & Export",
      whatItIs: "Einfacher CSV/TXT-Export nur für den Text oder eine vollständige Backup-Datei mit allem — Prompts, Ansichten, Layouts, Farben, Stile und Sprache.",
      howTo: [
        "Exportiere oder importiere aus der Daten-Gruppe des Experten-Menüs.",
        "Ein CSV/TXT-Import zeigt zuerst eine Vorschau an — den Namen jedes eingehenden Prompts und ob er als Duplikat landen würde —, bevor etwas geschrieben wird. Ein vollständiges Backup wird direkt wiederhergestellt, aus seinem eigenen Fenster heraus.",
        "Bestätige, um tatsächlich zu importieren, oder brich ab und nichts ändert sich.",
      ],
      whatHappens: "CSV/TXT nimmt nur Text mit; Bilder, Dateien, Layout und Einstellungen benötigen stattdessen das vollständige Backup-Format.",
      settings: ["Import / Export — Experten-Menü → System & Daten → Daten & Wartung"],
    },
    {
      section: "system",
      title: "Markdown-Ordner-Sync (Beta)",
      whatItIs:
        "Ein optionaler, standardmäßig deaktivierter Spiegel, der jeden Nur-Text-Prompt als eigene lesbare .md-Datei in einen selbst gewählten Ordner schreibt, neben die verschlüsselte Datenbank.",
      howTo: [
        "Schalte es ein und wähle im Experten-Menü einen Ordner aus.",
        "Bearbeite einen Prompt normal oder bearbeite seine .md-Datei direkt — beide Richtungen synchronisieren sich zur anderen.",
      ],
      whatHappens:
        "Im Gegensatz zur Datenbank sind die .md-Dateien im Klartext — jeder mit Zugriff auf diesen Ordner kann die Prompts lesen. Vermeide also einen synchronisierten oder freigegebenen Ordner für sensible Daten. Ein Prompt mit einer angehängten Datei, einem Bild oder Video wird übersprungen; er hatte ohnehin nie eine Nur-Text-.md-Form zum Synchronisieren.",
      settings: ["Prompt-Synchronisierung, Synchronisierungsordner — Experten-Menü → Funktionen → Prompt-Synchronisierung"],
      tip: "Dies ist eine Beta-Funktion — bewahre den Synchronisierungsordner an einem sicheren Ort auf.",
    },
    {
      section: "basics",
      title: "Screenshot-Tool",
      whatItIs: "Erfasst einen Teil oder den gesamten Bildschirm direkt in einen neuen Kopier-Button, ohne eine separate Screenshot-App.",
      howTo: [
        "Klick auf das Kamera-Icon.",
        "Ziehe einen Bereich auf oder klicke auf ein Fenster, um es ganz zu erfassen — auch ein normalerweise geschütztes wie den Task-Manager.",
        "Im Bestätigungsdialog lässt es sich als Button speichern, als Datei speichern oder einfach in der Zwischenablage belassen.",
      ],
      whatHappens:
        "Die Fenster der App selbst werden für den Moment der Aufnahme ausgeblendet, sodass sie nie auf dem Bild landen. Das Ergebnis erreicht sofort die Zwischenablage; das Speichern als Button oder Datei ist ein separater, optionaler Schritt.",
      settings: ["Screenshot-Tool, Speicherort, Vorschaugröße — Experten-Menü → Funktionen → Prompts erstellen"],
    },
    {
      section: "basics",
      title: "Farbpipette & Auswahlverlauf",
      whatItIs: "Eine Pipette, die den Bildschirm einfriert, den Pixel unter dem Cursor vergrößert und ihn als Hex, RGB oder HSL kopiert.",
      howTo: [
        "Klicke auf das Pipetten-Icon (standardmäßig aus — schalte es zuerst im Experten-Menü ein).",
        "Bewege die Lupe zu dem gewünschten Pixel; scrolle oder nutze +/- zum Zoomen.",
        "Klicke, um die Farbe zu kopieren; Rechtsklick auf das Pipetten-Icon, um die letzten paar Auswahlen wieder zu öffnen.",
      ],
      whatHappens:
        "Es wird niemals automatisch etwas in einem Prompt gespeichert — eine Auswahl landet nur in der Zwischenablage. Der Auswahlverlauf speichert die letzten paar Farben und übersteht einen Neustart, begrenzt durch eine eigene Einstellung.",
      settings: ["Farbpipette, Zoom, Format, Länge des Auswahlverlaufs — Experten-Menü → Funktionen → Prompts erstellen"],
      tip: "Die Farbpipette ist standardmäßig deaktiviert.",
    },
    {
      section: "basics",
      title: "Wechselnde Prompts",
      whatItIs: "Ein Prompt kann eine Liste von Texten statt nur einem enthalten — jede Kopie serviert den nächsten Eintrag, anstatt immer denselben Text.",
      howTo: [
        "Schalte wechselnde Prompts im Experten-Menü ein (standardmäßig aus).",
        "Füge im Editor eines Prompts mehr als einen Texteintrag hinzu.",
        "Wähle Reihenfolge (durchläuft der Reihe nach) oder Mischen (zufällig, ohne Wiederholung, bis jeder Eintrag einmal genutzt wurde).",
        "Kopiere die Kachel wie gewohnt — sie serviert immer den nächsten Eintrag, es wird nie gefragt, welchen.",
      ],
      whatHappens:
        "Ein kleines Badge zeigt die Position, wie \"2/5\", auf einer Raster-Kachel, einer schwebenden Pille und einem Mitglied eines Pillen-Ordners (ein Raster-Ordner-Mitglied zeigt nur sein Dateityp-Icon, nicht dieses Badge). Die Position bleibt nach Neustarts und vollständigen Backups erhalten; ein CSV/TXT-Import erstellt einen neuen Prompt mit einem frischen Zeiger, sodass nur die Textliste und ihr Reihenfolge-/Mischen-Modus übernommen werden, nicht die genaue Position. Wird die Rotation bei einem Prompt mit mehreren Einträgen deaktiviert, bleiben alle erhalten — nur der erste wird verwendet, bis sie wieder aktiviert wird.",
      settings: ["Wechselnde Prompts, Standard Reihenfolge/Mischen, Badge, maximale Einträge — Experten-Menü → Funktionen → Prompts erstellen"],
    },
    {
      section: "system",
      title: "Das Experten-Menü",
      whatItIs: "Jede optionale Funktion befindet sich hier hinter einem Ein-/Aus-Schalter, mit ihren eigenen Parametern direkt darunter, sobald sie eingeschaltet ist.",
      howTo: [
        "Öffne Einstellungen → das Experten-Menü.",
        "Wähle einen Tab oder tippe in das Suchfeld, um alle Tabs auf einmal zu durchsuchen.",
        "Schalte \"Nur geänderte\" ein, um nur das zu sehen, was an den Standardwerten angepasst wurde.",
      ],
      whatHappens:
        "Eine Funktion, die standardmäßig deaktiviert ausgeliefert wird, trägt ein kleines Punkt-Badge an ihrer Beschriftung mit einem Tooltip, der erklärt, was sie bewirkt. Zurücksetzen funktioniert auf drei Ebenen: dem eigenen Rückgängig-Icon eines Wertes, dem Zurücksetzen-Icon einer ganzen Gruppe in ihrer Kopfzeile oder jeder Einstellung auf einmal über die generelle Zurücksetzen-Aktion.",
      settings: [],
    },
    {
      section: "system",
      title: "Tastaturnavigation",
      whatItIs: "Pfeiltasten bewegen einen Fokusring über das Raster; Enter oder Leertaste kopiert die Kachel, die gerade fokussiert ist.",
      howTo: [
        "Schalte die Tastaturnavigation im Experten-Menü ein.",
        "Verwende die Pfeiltasten, um den Fokusring zu bewegen, und Enter oder Leertaste, um zu kopieren.",
        "Tabulator springt weiterhin wie gewohnt durch Buttons und Felder überall sonst in der App.",
      ],
      whatHappens: "Der Fokusring blendet sich kurze Zeit aus, nachdem das Fenster den Fokus verliert, damit er nicht auf einer Kachel verweilt, sobald zu einer anderen App gewechselt wurde.",
      settings: ["Tastaturnavigation, Ausblend-Verzögerung — Experten-Menü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Wo die Daten liegen",
      whatItIs:
        "Alles — Prompts, Ansichten, Einstellungen — befindet sich in einer einzigen lokalen SQLite-Datenbank (plus ihrer kleinen begleitenden WAL-Datei); wo dieses Paar liegt, hängt davon ab, ob die portable exe oder die installierte Version verwendet wird.",
      howTo: [
        "Installiert: Der Daten-Ordner befindet sich unter AppData, im Experten-Menü angezeigt (und änderbar).",
        "Portabel: Der Daten-Ordner liegt direkt neben der exe, sodass das Ganze als eine Einheit auf einem USB-Stick oder in einem synchronisierten Ordner bleibt.",
      ],
      whatHappens: "Ein Wechsel von portabel zu installiert (oder zurück) wird automatisch erkannt und als einmalige Migration angeboten — bestehende Daten werden niemals stillschweigend zurückgelassen.",
      settings: ["Daten-Ordner — Experten-Menü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Updates & Sicherheit",
      whatItIs: "Eine optionale Prüfung auf eine neuere Version, standardmäßig aktiv, die immer nur mit dem Update-Server kommuniziert — niemals mit etwas anderem.",
      howTo: [
        "Lass Update-Checks eingeschaltet (Standard), um benachrichtigt zu werden, wenn eine neue Version verfügbar ist, oder schalte sie im Experten-Menü aus.",
        "Akzeptiere ein angebotenes Update, um es mit einem Klick herunterzuladen und zu installieren.",
      ],
      whatHappens:
        "Die Update-Prüfung ist der einzige Netzwerkverkehr, den die App von sich aus durchführt; die einzige Ausnahme ist ein Download des Microsoft WebView2-Installers beim ersten Start, falls diese Laufzeitumgebung noch nicht auf dem System vorhanden ist. Ein heruntergeladenes App-Update wird vor der Ausführung anhand einer veröffentlichten Prüfsumme verifiziert — eine fehlende oder nicht übereinstimmende Prüfsumme blockiert die Installation, anstatt eine unbestätigte Datei auszuführen.",
      settings: ["Update-Checks — Experten-Menü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Häufige Fragen",
      whatItIs: "Schnelle Antworten auf die Dinge, die am häufigsten aufkommen.",
      howTo: [
        "\"Mein Screenshot von dieser App ist leer\" — die Datenschutzeinstellung, die das Fenster aus Bildschirmaufnahmen ausschließt, funktioniert wie vorgesehen; schalte sie im Experten-Menü aus, falls das nicht gewünscht ist.",
        "\"Die App startet nicht\" — sie benötigt die Microsoft WebView2-Laufzeitumgebung; der Installer bietet sie automatisch an, falls sie fehlt.",
        "\"Ich finde eine Einstellung nicht\" — überprüfe zuerst das Suchfeld im Experten-Menü; es durchsucht alle Tabs auf einmal.",
        "\"Ein Hotkey bewirkt nichts\" — eine andere App hält diese Kombination möglicherweise bereits; der Editor erklärt dies, anstatt sie stillschweigend zu akzeptieren.",
      ],
      settings: [],
    },
  ],
};
