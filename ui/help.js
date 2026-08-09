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
      overview:
        "Eine Kachel ist ein gespeicherter Prompt. Doppelklick auf eine leere Rasterzelle (oder der +-Knopf) legt eine an, ein Klick auf eine Kachel kopiert sie. Beim ersten Start sind drei Beispielkacheln vorhanden (eine E-Mail mit Platzhaltern, ein Bild, eine Markdown-Vorlage) — ein Knopf über dem Raster entfernt sie mit einem Klick, sobald sie nicht mehr gebraucht werden.",
      settings: [],
    },
    {
      section: "basics",
      title: "Prompts anlegen",
      overview: "Neben reinem Text kann ein Prompt eine angehängte Datei, einen Screenshot oder eine per Pipette abgelesene Bildschirmfarbe tragen.",
      settings: [
        "Datei-Anhang — ein Klick auf die Kachel kopiert die Datei selbst, keinen Text",
        "Screenshot-Werkzeug — ein Bereich oder ein ganzes Fenster wird direkt zu einem neuen Prompt",
        "Farbpipette — beliebigen Bildschirmpunkt ablesen, Kopie als Hex/RGB/HSL",
        "Ziehen & Ablegen — eine Datei oder ein Bild direkt aufs Raster ziehen",
      ],
      tip: "Die Farbpipette ist standardmäßig aus.",
    },
    {
      section: "basics",
      title: "Variablen & Platzhalter",
      overview:
        "Schreib {#{Name}#} irgendwo in den Prompt-Text — beim Kopieren öffnet sich ein kleines Popup, das nach jedem Platzhalterwert fragt, bevor der ausgefüllte Text in der Zwischenablage landet.",
      settings: [
        "Dynamische Platzhalter — {#{date}#}, {#{time}#}, {#{clipboard}#} füllen sich selbst, kein Popup",
        "Auswahl-Platzhalter — ein Dropdown mit vorgegebenen Werten statt Freitext",
        "Auswahl merken — das Popup füllt die letzte Antwort je Prompt vor",
        "Popup im eigenen Fenster — nützlich bei schwebenden Buttons, wenn das Hauptfenster verborgen ist",
      ],
    },
    {
      section: "basics",
      title: "Der Editor",
      overview: "Der Prompt-Editor ist standardmäßig ein reines Textfeld; ein paar optionale Funktionen geben ihm Struktur und Verlauf.",
      settings: [
        "Markdown-Vorschau — zeigt den Text als formatiertes Markdown neben (oder statt) dem Feld",
        "Syntax-Hervorhebung — Codeblöcke in der Markdown-Vorschau bekommen Farbe",
        "Versionsverlauf — jede inhaltliche Änderung bleibt erhalten, jede frühere Version wiederherstellbar",
        "Private Notizen — ein nur für dich sichtbares Notizfeld, nie mitkopiert oder exportiert",
      ],
      tip: "Markdown-Vorschau und private Notizen sind beide standardmäßig aus.",
    },
    {
      section: "copying",
      title: "Kopieren",
      overview: "Was zwischen dem Klick auf eine Kachel und der Zwischenablage passiert.",
      settings: [
        "Verketten — mehrere Kacheln nacheinander anklicken, sie werden zu einer Kopie zusammengefasst",
        "Prompt-Hotkeys — einen bestimmten Prompt von überall kopieren, ohne das Programm zu öffnen",
        "Herausziehen — eine Kachel auf ein anderes Fenster ziehen fügt direkt ein, ohne Umweg über die Zwischenablage",
        "Automatisches Einfügen — nach dem Kopieren wechselt das Programm zum letzten Fenster und fügt selbst ein",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Schwebende Buttons & Ordner",
      overview: "Schwebende Buttons sind kleine, immer-oben-Kopierknöpfe außerhalb des Hauptfensters, über jedem beliebigen Programm.",
      settings: [
        "Pillen-Ordner — mehrere schwebende Buttons zu einer aufklappbaren Gruppe zusammenfassen",
        "Raster-Ordner — eine Kachel im Hauptraster auf eine andere ziehen, gruppiert sie genauso",
      ],
      tip: "Beide Ordnerarten sind standardmäßig aus.",
    },
    {
      section: "alwaysOnTop",
      title: "Tray",
      overview: "Das Rechtsklick-Menü des Symbols in der Taskleiste bietet schnellen Zugriff, ohne das Hauptfenster zu öffnen.",
      settings: [
        "Zuletzt kopiert — die letzten Kopien, direkt erneut kopierbar",
        "Favoriten — markierte Prompts, immer an derselben Stelle im Menü",
      ],
    },
    {
      section: "copying",
      title: "Zwischenablage-Wächter",
      overview: "Eingeschaltet wird alles, was irgendwo im System kopiert wird (nicht nur aus diesem Programm), in einem Eingang protokolliert, den man später durchsehen und zu Prompts machen kann.",
      settings: [
        "Mindestlänge — kürzere Kopien werden ignoriert (blendet einzelne Zeichen aus)",
        "Eingang-Obergrenze — älteste Einträge fallen raus, sobald der Eingang darüber wächst",
      ],
      tip: "Standardmäßig aus — nichts wird beobachtet, bis es eingeschaltet wird.",
    },
    {
      section: "organising",
      title: "Kachelverhalten & Aussehen",
      overview: "Wie eine Kachel auf die Maus reagiert und wie sie aussieht.",
      settings: [
        "Kachelmenü / Ziehen zum Umsortieren — Rechtsklick-Aktionen und freies Umordnen",
        "Hover-Anhebung — eine Kachel hebt sich leicht unter dem Zeiger",
        "Typ-Abzeichen, Schatten, Verläufe, abgerundete Ecken, fett/Großbuchstaben/Monospace — reine Optik",
      ],
    },
    {
      section: "look",
      title: "Tooltips",
      overview: "Hovern über eine Kachel zeigt ihren vollen Text, die Länge und (falls vorhanden) ihre private Notiz.",
      settings: ["Verzögerung bis zum Anzeigen", "Automatisches Ausblenden", "Tooltip-Breite"],
    },
    {
      section: "organising",
      title: "Favoriten",
      overview: "Ein Prompt lässt sich als Favorit markieren (Stern).",
      settings: ["Favoriten-Ansicht — eine eigene Ansicht nur mit markierten Prompts, eigenständig sortierbar"],
    },
    {
      section: "look",
      title: "Skalierung & Zoom",
      overview: "Unabhängige Größenregler für verschiedene Teile des Fensters, falls die Standardgrößen nicht zu Bildschirm oder Geschmack passen.",
      settings: ["Gesamte UI-Größe", "Dialog-Größe", "Composer-Größe", "Icon-Größe"],
    },
    {
      section: "look",
      title: "Aussehen & Themes",
      overview: "Acht mitgelieferte Themes: System, hell, dunkel, Programmierer, Neon-Violett, Ozean, Cyberpunk und Schwarzweiß.",
      settings: ["Theme-Editor — Farben jedes Themes anpassen und als eigenes Theme speichern"],
      tip: "Eigene Themes sind eine optionale Funktion — der Editor-Knopf erscheint erst, wenn sie eingeschaltet ist.",
    },
    {
      section: "organising",
      title: "Bibliothek & Suche",
      overview: "Die Bibliothek listet jeden Prompt als durchsuch- und filterbare Liste — das Raster zeigt nur, was auf den Bildschirm passt, die Bibliothek zeigt alles.",
      settings: [
        "Unscharfe Suche — findet Treffer auch bei einem Tippfehler",
        "Intelligente Sortierung — nach Nutzungshäufigkeit oder letzter Nutzung",
        "Typ-/Farbfilter — die Liste vor der Suche eingrenzen",
      ],
    },
    {
      section: "organising",
      title: "Tags",
      overview: "Frei vergebene Schlagwörter für Prompts; Bibliothek oder Raster lassen sich auf bestimmte Tags eingrenzen.",
      settings: ["Tag-Filtermodus — trifft EIN ausgewählter Tag, oder müssen ALLE zutreffen"],
    },
    {
      section: "copying",
      title: "Kopierverlauf",
      overview: "Ein Protokoll, was wann kopiert wurde — getrennt vom Zwischenablage-Wächter, erfasst nur Kopien aus diesem Programm.",
      settings: [
        "Gruppierte Ansicht — eine Zeile je Prompt (die letzte Kopie) statt jedes Ereignisses",
        "Aufbewahrung — Verlauf älter als N Tage automatisch löschen",
        "Einzeleintrag löschen — eine einzelne Verlaufszeile entfernen, ohne alles zu leeren",
        "Nutzungsstatistik — Summen, ein 14-Tage-Diagramm, meistgenutzte Prompts",
      ],
    },
    {
      section: "organising",
      title: "Medien",
      overview: "Einstellungen für Prompts mit Video oder Bild.",
      settings: ["Video-Steuerung, Autoplay, Stumm, Wiederholung", "Graustufen / Abdunkeln bis Hover — datenschutzfreundliche Optik für Medienkacheln"],
    },
    {
      section: "look",
      title: "Symbolleiste",
      overview: "Welche Knöpfe Kopfzeile und Composer-Leiste zeigen.",
      settings: ["Schnellraster-Größenregler", "Bibliothek-/Verlauf-/Anheften-Knöpfe", "Mehrfachansichten + Ansichtsgruppen (Reiter fürs Raster)"],
    },
    {
      section: "system",
      title: "Datenschutz",
      overview: "Einstellungen, die den Fensterinhalt aus Screenshots, Bildschirmfreigaben oder fremden Blicken heraushalten.",
      settings: [
        "Von Bildschirmaufnahme ausschließen — das Fenster ist für Screenshot-/Aufnahme-Werkzeuge unsichtbar",
        "Kacheln/Medien bis Hover weichzeichnen",
        "Kachelnamen verbergen",
        "Nicht gehoverte Kacheln abdunkeln",
      ],
      tip: "Der Bildschirmaufnahme-Ausschluss ist standardmäßig AUS — bei Bedarf einschalten.",
    },
    {
      section: "system",
      title: "System",
      overview: "Programmweites Verhalten, das keinen einzelnen Prompt betrifft.",
      settings: [
        "Update-Prüfung — standardmäßig an, installiert nie ungefragt etwas",
        "Tastatur-Navigation — Pfeiltasten bewegen einen Fokusring über das Raster",
        "Mehrere Instanzen erlauben — normalerweise holt ein zweiter Start nur das erste Fenster nach vorn",
        "Datenordner — wo Prompts und Einstellungen liegen; änderbar, portabel-freundlich",
      ],
    },
    {
      section: "system",
      title: "Datenpflege",
      overview: "Aufräum-Werkzeuge, erreichbar über die Datengruppe im Expertenmenü.",
      settings: [
        "Duplikatfinder — markiert nahezu identische Prompts anhand von Textähnlichkeit",
        "Papierkorb — ein gelöschter Prompt liegt hier, bevor er endgültig verschwindet, eigene Aufbewahrungsfrist",
        "Import/Export — CSV/TXT-Export, vollständiger Backup-Export/-Import",
      ],
    },
    {
      section: "system",
      title: "Eigenständig: Backup-Manager",
      overview: "Verschlüsselte, komprimierte Momentaufnahmen der gesamten Prompt-Bibliothek und Einstellungen, nach eigenem Zeitplan.",
      settings: [
        "Automatische Backups in festem Intervall",
        "Intelligente Aufbewahrungsstufen (täglich/wöchentlich/monatlich/jährlich, GFS-artig)",
        "Wiederherstellungs-Umfang — nur Daten, nur Einstellungen, oder beides",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Eigenständig: Schnellstarter",
      overview: "Ein kleines, immer-oben-Popup (Standard-Hotkey Strg+Alt+P) mit unscharfer Suche über Name und Text jedes Prompts — Auswahl kopiert sofort, ohne das Hauptfenster hervorzuholen.",
      settings: [],
    },
    {
      section: "organising",
      title: "Eigenständig: Mehrfachansichten",
      overview: "Eine Ansicht ist eine Rasterseite mit eigener Größe und Anordnung; ein Prompt kann in mehreren Ansichten liegen. Ansichtsgruppen machen aus Ansichten Reiter statt eines Dropdowns.",
      settings: [],
    },
    {
      section: "look",
      title: "Eigenständig: Theme-Editor",
      overview: "Von einem mitgelieferten Theme ausgehen, einzelne Farben ändern und das Ergebnis als eigenes Theme speichern — das Original bleibt unangetastet.",
      settings: [],
    },
  ],
};
