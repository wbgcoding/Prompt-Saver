// In-app documentation. EN + DE only (deliberate scope decision):
// every other language falls back to English. Kept out of i18n.js on purpose: that
// file's parity check (tests/i18n_check.js) requires every key in all 11 languages,
// which this content deliberately does not follow.
//
// Mirrored (structure, not word-for-word) in docs/help.md: keep both in sync when
// a category changes.
//
// `id` is an optional stable key, present on the handful of articles something
// outside this file needs to jump straight to (the first-run tour's help buttons,
// so far) -- unlike `title`, it never changes with a rewrite and stays the same in
// both languages. Add one only when a new caller needs an anchor.

export const HELP_CONTENT = {
  en: [
    {
      section: "basics",
      title: "Getting started",
      whatItIs: "A tile is a saved item on the grid; clicking it copies its text to the clipboard.",
      howTo: [
        "Double-click an empty grid cell, or the plus button, to create one.",
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
        "Click the gear button in the header to open it.",
        "Pick the language, the theme, and the text size and font of the tiles.",
        "Choose whether the window stays on top, starts with Windows, starts in the background, and goes to the background instead of quitting when closed (on by default for a new install; a portable copy always quits).",
        "Set the global hotkey, auto-paste on double-click, and whether screenshots always go to the screenshots folder.",
      ],
      whatHappens:
        "Every change applies at once, with no save button. \"Automatic\" as the language follows Windows and falls back to English. Reset puts every setting back to its default and keeps the tiles; Delete removes all data and asks first.",
      settings: [],
    },
    {
      section: "basics",
      title: "Introduction tour",
      whatItIs: "A five-step walkthrough of the basics, shown once automatically right after the example tiles are created.",
      howTo: [
        "First launch: opens on its own, right after the example prompts are seeded.",
        "Later: reopen it from the settings dialog, or the expert menu's \"Reopen introduction\" action.",
        "←/→ or the dots move between steps, Enter continues, Escape or \"Skip\" leaves any time; Escape shows an Undo notice that reopens the tour.",
      ],
      whatHappens:
        "The last step offers to switch on the screenshot editor, color picker, or text recognition right there, showing each one's shipped default; nothing is written until the tour is actually finished, and only the switches actually changed. A first-launch data-folder or WebView2 prompt always takes priority; the tour just waits for its turn.",
      settings: ["Open automatically on first launch: Expert menu → System & data → Help"],
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
      tip: "Pick a color format (hex, RGB, or HSL) from the toast that appears right after a pick.",
    },
    {
      section: "basics",
      title: "Placeholders",
      whatItIs:
        "Placeholder markup inside a tile's text opens a fill-in popup before the finished text reaches the clipboard.",
      howTo: [
        "Type {#{PlaceholderName}#} inside the text.",
        "For a self-filling value, click a dynamic-placeholder chip under the text box instead of typing it.",
        "Click the tile to copy: the fill-in popup opens automatically.",
        "Enter a value per placeholder, or pick one for choice placeholders, and confirm.",
      ],
      whatHappens:
        "Only the filled-in text reaches the clipboard; the raw {#{...}#} markup is never copied. A dynamic placeholder (date, time, clipboard, UUID) uses its own syntax, {#{/date\\}#}, and fills itself in with no popup at all. The popup can remember the value last typed for a placeholder and offer it again. A list of fixed values, kept in the expert menu, is suggested for a placeholder of the same name; it is only ever suggested, never filled in on its own.",
      settings: ["Dynamic placeholders, choice placeholders, remember choices, remember typed values, fixed values list, date and time style: Expert menu → Features → Placeholders"],
      sketches: ["placeholderAnatomy"],
    },
    {
      section: "basics",
      title: "The editor",
      whatItIs: "The editor is a plain text box at heart; a few features (Markdown preview and private notes are on by default, switchable in the expert menu) add structure and versions to it.",
      howTo: [
        "Open a tile's editor: double-click the tile, or Edit from its menu.",
        "Adjust Markdown preview, syntax highlighting, versions, or private notes in the expert menu.",
      ],
      whatHappens:
        "Versions keep every saved change so an earlier version can be restored. Markdown preview renders next to, or instead of, the text box without altering what is actually stored. Private notes are never copied or exported with the text. The open text is saved as a draft every few seconds; if the window or the app closes before saving, the next start offers the draft back once. A confidential tile never gets a draft. Saving a new tile that closely matches an existing one shows a quiet hint instead of blocking the save. For a text with Markdown in it, \"Copy formatted\" in the editor and the tile menu puts the rendered version on the clipboard as well, so Word or a mail program shows the formatting. In the list of versions, a second button next to Restore keeps the chosen version as a new tile and leaves the current one untouched.",
      settings: ["Markdown preview, syntax highlighting, versions, private notes: Expert menu → Features → Editor", "Monospace text box: Expert menu → Features → Editor", "Formatted copy, live length counter, word-by-word comparison of versions, confirm discarding changes, draft autosave and its interval: Expert menu → Features → Editor", "Similar-prompt hint on save: Expert menu → System & data → Data & maintenance"],
      tip: "Markdown preview and private notes both ship on.",
    },
    {
      section: "copying",
      title: "Copying",
      whatItIs: "Copying is the sequence between clicking a tile and its text landing on the clipboard.",
      howTo: [
        "Click a tile once to copy.",
        "If auto-paste is on, click again quickly or press Enter to also paste.",
        "Ctrl-drag a tile onto another window to drop its text directly, with no clipboard round-trip.",
      ],
      whatHappens:
        "A short confirmation bubble or flash confirms the copy. Chaining, clicking several tiles in a row, combines them into one clipboard entry instead of overwriting each other; Start chain here: right-click a text tile to start a chain with that tile first. A chain stays when you switch to another program. A running chain shows the chain button in the top bar; click it to end the chain. Pasting with Ctrl+V in the app window ends it too. While chaining, drag a step in the strip above the grid to reorder it, or right-click it to remove it. Chaining is not the same as rotation: chaining combines several different tiles, rotation changes what a single tile holds from copy to copy (see Rotation). With the double-click latch on, chain mode stays on after copying, for building several chains in a row. The strip can also mark a step red when it needs a placeholder that no earlier step fills; the chain still runs.",
      settings: ["Chaining, and starting a chain from the tile menu: Expert menu → Features → Chain", "What goes between two chained steps: Expert menu → Features → Chain", "Keep chain mode on after copying, show what each chain step needs: Expert menu → Features → Chain", "Auto-paste on double-click: Settings", "Per-tile hotkeys, press Enter after auto-paste, drag-out, hide the window after copying: Expert menu → Features → Copying", "Which key drags a tile out, and whether that key drags out or moves it: Expert menu → Features → Copying"],
      sketches: ["flowToTarget", "chainSteps"],
    },
    {
      section: "alwaysOnTop",
      title: "Floating buttons",
      whatItIs: "Floating buttons are small always-on-top copy buttons that sit outside the main window, over whatever app is in use.",
      howTo: [
        "Right-click a tile, then \"Toggle floating button\", to pin it as a floating button.",
        "Drag the floating button to move it; right-click it for size, edit, or remove.",
      ],
      whatHappens: "Positions survive app restarts. A button whose saved screen is no longer connected opens on the main screen, marked as parked, and jumps back on its own once that screen returns. An action in the expert menu brings every floating window to the main screen at once.",
      settings: ["Floating buttons: Expert menu → Features → Floating buttons & button folders", "Park buttons whose screen is gone, hide long-parked ones, bring all back: Expert menu → Features → Floating buttons & button folders"],
    },
    {
      section: "alwaysOnTop",
      title: "Folders",
      whatItIs: "The same gesture groups things in two places: drop one item onto another of its own kind to gather it into a folder.",
      howTo: [
        "Drag one floating button onto another to make a button folder.",
        "On the main grid, drag one tile onto another to make a tile folder.",
        "Drag a member back out to free it.",
      ],
      whatHappens:
        "A button folder or tile folder shows up to four member previews and a count, dragging a member out frees it, and it dissolves once one member is left; a tile belongs to only one tile folder at a time.",
      settings: ["Button folders, tile folders: Expert menu → Features → Floating buttons & button folders"],
      tip: "Both kinds are on out of the box; switch any of them off in the expert menu if items are never grouped that way.",
      sketches: ["folderTypes"],
    },
    {
      section: "alwaysOnTop",
      title: "Tray",
      whatItIs: "The tray icon's right-click menu offers quick access without opening the main window.",
      howTo: ["Right-click the tray icon.", "Pick an item from \"Recently copied\" or \"Favorites\" to copy it immediately."],
      whatHappens: "The menu copies straight from the tray; the main window never has to open for this. The menu is drawn as a rounded window in the app's own theme; switch that off for the plain Windows menu. Typing in the tray menu's search field opens the quick launcher with what you typed.",
      settings: ["Windows 11-style menu, search field, recently copied section, favorites section, entry counts, label length: Expert menu → Features → Tray menu"],
    },
    {
      section: "copying",
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
      tip: "Off by default: nothing is watched until it is turned on.",
    },
    {
      section: "organising",
      title: "Tile behavior & appearance",
      whatItIs: "These settings control how a tile reacts to the mouse and what it looks like.",
      howTo: [
        "Right-click a tile for its menu: edit, pin, hide, delete, and more.",
        "Right-click a tile (or Ctrl+click) to select it; while selecting, every tile shows its checkbox.",
        "Drag a tile to any grid cell; drop it on an occupied cell to swap.",
        "Turn on hover lift, shadows, badges, and similar looks in the expert menu, and pick a tile style (bold, uppercase, monospace, italic, shadowed names, compact, gradient).",
      ],
      whatHappens: "These are interaction and looks only; none of them change what a tile copies.",
      settings: [
        "Tile menu, drag-to-reorder: Expert menu → Tiles → Tiles",
        "Dim until hovered, lift on hover: Expert menu → Tiles → Tiles",
        "Type badges, shadows, rounded corners, tile style (bold, uppercase, monospace, italic, shadowed names, compact, gradient): Expert menu → Tiles → Tile appearance",
      ],
    },
    {
      section: "look",
      title: "Tooltips",
      whatItIs: "Hovering a tile shows its full text, length, and any private note.",
      howTo: [
        "Hover a tile and wait for the delay to elapse.",
        "Adjust delay or width in the expert menu if needed.",
      ],
      whatHappens: "The tooltip auto-hides after its timeout even if the mouse stays put.",
      settings: ["Delay before showing, tooltip width: Expert menu → Tiles → Tooltips"],
    },
    {
      section: "organising",
      title: "Favorites",
      whatItIs: "A star marks a tile as a favorite for quick access later.",
      howTo: ["Click the star on a tile, or use \"Favorite\" from its menu.", "Open the dedicated favorites view to see only starred tiles."],
      whatHappens: "Favorites get their own reorderable view, independent of wherever else the tile lives.",
      settings: ["Favorites view: Settings. Its own reordering: Expert menu → Tiles → Favorites"],
    },
    {
      section: "look",
      title: "Scaling & zoom",
      whatItIs: "Independent size sliders adjust different parts of the window separately.",
      howTo: ["Open Settings and find the scaling sliders.", "Adjust overall UI, dialog, composer, or icon scale independently."],
      whatHappens: "Each slider only affects its own part of the window: raising icon scale, for example, does not resize dialogs.",
      settings: ["Interface size and icon size: Settings", "Dialog, composer and main-button scale: Expert menu → Appearance → Sizes (%)"],
    },
    {
      section: "look",
      title: "Appearance & themes",
      whatItIs: "Eight built-in themes cover system, light, dark, and five accent styles.",
      howTo: ["Pick a theme from Settings.", "Open the theme editor, once enabled, to adjust any theme's colors and save a custom one."],
      whatHappens: "\"System\" follows the OS light or dark setting live. A custom theme is saved separately; the built-in it was based on stays untouched.",
      settings: ["Theme choice: Settings", "Theme editor: Expert menu → Appearance → Visual", "Accent-colored header, frosted dialogs, hidden or accent-colored scrollbars, smooth scrolling: Expert menu → Appearance → Visual"],
      tip: "Custom themes are on out of the box; switching them off in the expert menu also hides the editor button {icon:themeEditor}.",
    },
    {
      section: "organising",
      title: "Library & search",
      whatItIs: "The library lists every tile as a searchable, filterable list, unlike the grid, which shows only what fits.",
      howTo: [
        "Click the library button {icon:library} in the toolbar to open it.",
        "Type to search, or use the type and color filters to narrow the list.",
        "Click a row to copy.",
        "The Close after copy button in the library header decides whether copying closes the library; it remembers your choice.",
        "Use its menu to edit, drag onto the grid, tag, or delete.",
        "Keep a search with its tag filters and sort order as a chip above the field; one click brings it back.",
        "The list or tiles toggle next to the type filter switches to large preview tiles.",
        "Pick two rows and click \"Compare\" in the selection bar to see both side by side with the differences marked.",
      ],
      whatHappens: "Search is typo-tolerant by default and, unless turned off, sorts by most-used or most-recently-used. In the comparison, either side's text can be taken over into the other. Tiles view behaves exactly like the list for clicks, menus and multi-select.",
      settings: ["Fuzzy search, type and color filters, saved searches: Expert menu → Library & copy history → Search", "Focus the search field on opening: Expert menu → Library & copy history → Search", "Entries per search list (library, Search everything, quick launcher): Expert menu → Library & copy history → Search", "Smart sort, multi-select for batch actions, image and video previews in the rows, tiles view: Expert menu → Library & copy history → Display"],
    },
    {
      section: "organising",
      title: "Search everything",
      whatItIs: "One search box for tiles, copy history, attachments, views, settings and this help, all at once.",
      howTo: [
        "Click the magnifier in the toolbar, or press Ctrl+K.",
        "Type at least two characters; hits appear in groups.",
        "Move with the arrow keys and press Enter, or click a hit.",
      ],
      whatHappens:
        "Enter does what fits the hit: it copies a tile or copy history entry, opens an attachment, switches to a view, opens and highlights a setting, or opens the help article. Each group shows a few hits and a \"show more\". With the box empty, recent searches are offered. Escape, a click outside, or the magnifier again closes it.",
      settings: ["Search everything: Expert menu → Library & copy history → Search"],
    },
    {
      section: "organising",
      title: "Tags",
      whatItIs: "Tags are free-form labels assigned to tiles, used to filter the library or the grid.",
      howTo: [
        "Type a tag name in a tile's editor to add it, or pick an existing one.",
        "Use the tag filter row in the library to narrow the list.",
      ],
      whatHappens: "Renaming, recoloring, or deleting a tag from the tag manager applies everywhere it is used, in one step.",
      settings: ["Tag filter mode (match any selected tag, or all of them): Expert menu → Library & copy history → Tags"],
    },
    {
      section: "organising",
      title: "Archive",
      whatItIs: "The archive sets a tile aside without deleting it.",
      howTo: [
        "Turn the archive on in the expert menu.",
        "Choose \"Archive\" from a tile's menu, or archive several at once from the library's selection bar.",
        "Use the toggle above the library list to show archived tiles again.",
      ],
      whatHappens:
        "An archived tile leaves the grid and the library list, but search still finds it, and exports and backups keep it like any other tile. \"Unarchive\" in the same menu brings it back.",
      settings: ["Archive: Expert menu → Library & copy history → Display"],
      tip: "Off by default.",
    },
    {
      section: "copying",
      title: "Copy history",
      whatItIs: "Copy history logs what was copied and when, in the app and, if switched on, in other apps.",
      howTo: [
        "Open the Library and choose the Copy history tab.",
        "Search, filter by type or color, or star a favorite from the list.",
        "Delete a single row, or clear the whole copy history.",
      ],
      whatHappens: "Grouped view shows one row per tile (its most recent copy) instead of every event. Copy history older than the retention setting is dropped automatically (30 days for a new install, adjustable, 0 keeps everything). The copy history is one list; the sort field switches between Recently copied and Most used (top 20).",
      settings: ["Grouped view, retention: Expert menu → Library & copy history → Copy history", "Skip an entry that repeats the copy before it: Expert menu → Library & copy history → Copy history"],
    },
    {
      section: "organising",
      title: "Media",
      whatItIs: "These settings control tiles that carry a video or an image.",
      howTo: [
        "Attach a video or image to a tile: file attach, paste, or drag and drop.",
        "Adjust playback and privacy-leaning looks in the expert menu.",
      ],
      whatHappens: "Video settings (autoplay, mute, loop, volume) apply to every video tile; grayscale or dimming only changes how media looks, never what gets copied. The library and copy history also show a video's length on its thumbnail, and its resolution and file size in the tooltip; the main grid never shows either.",
      settings: ["Video controls, autoplay and loop (off by default), mute: Expert menu → Tiles → Media", "Grayscale: Expert menu → Tiles → Media", "Accent border and rounded corners around media: Expert menu → Tiles → Media", "Duration/resolution/size in the library and copy history: Expert menu → Tiles → Media", "Fill in media info for older attachments: Expert menu → Tiles → Media"],
    },
    {
      section: "look",
      title: "Toolbar",
      whatItIs: "The toolbar setting controls which buttons the header and composer bar show.",
      howTo: ["Right-click the header to show or hide its buttons; features themselves are switched in the expert menu.", "Hide the top bar: right-click the gear or the top bar."],
      whatHappens: "Hiding a button only hides it; the feature behind it, if any, keeps working through its own hotkey or menu entry.",
      settings: ["Library button and double-click on the bar buttons: Expert menu → System & data → Toolbar; logo, title and bar buttons: right-click the top bar.", "Quick grid-size control and showing it on hover only, multiple views: Expert menu → Features → Views and quick grid"],
    },
    {
      section: "system",
      title: "Privacy",
      whatItIs: "These settings keep this window's contents out of screenshots, screen shares, and passers-by.",
      howTo: [
        "Screen-capture exclusion is on from the start.",
        "Turn on tile or media blur or hidden tile names in the expert menu, as needed.",
      ],
      whatHappens:
        "Screen-capture exclusion makes the window invisible to screenshot and recording tools, including its own, so if a capture comes out empty, check this setting first. An optional, off-by-default setting reads text out of an attached picture so the library search can find it too; a hit found only there carries a \"found in image\" badge, so it never looks like a random match. That recognized text stays out of the app otherwise: it is never shown, never exported, never synchronized, and its length is capped. Removing or replacing the picture clears it right away, and switching the setting off removes it from every tile, the trash included. A single tile can also be marked confidential; see \"Confidential tiles\" below.",
      settings: ["Exclude from screen capture, blur tiles or media, hide names: Expert menu → System & data → Privacy", "Index text inside attached pictures for search, image text length limit: Expert menu → Library & copy history → Search", "Mark a tile confidential, vault lock timeout: Expert menu → System & data → Privacy"],
      tip: "Screen-capture exclusion ships on: turn it off for normal screenshots or recordings of this window.",
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
      title: "System",
      whatItIs: "System settings cover app-level behavior that is not about any single tile.",
      howTo: ["Open the expert menu's System group for updates, navigation, instances, and the data folder."],
      whatHappens:
        "Update checks are on by default and never install anything without asking first. Changing the data folder migrates the existing database there. Allowing multiple instances takes a restart to apply. Pointing the data folder at a cloud-synced folder shows a one-time warning. A small lock file then tracks which computer uses the database: a second computer that finds a fresh lock from another one opens everything read-only and says so in a banner, while a lock nobody refreshed for a while is taken over. Long jobs can fill the taskbar icon as a progress bar and show an estimate of the time left.",
      settings: ["Keyboard navigation, allow multiple instances, data folder, data folder lock, taskbar progress, remaining time, video tools download: Expert menu → System & data → System"],
    },
    {
      section: "system",
      title: "Data maintenance",
      whatItIs: "Data maintenance gathers housekeeping tools reached from the expert menu's data group.",
      howTo: ["Open the expert menu → System & data → Data & maintenance.", "Open the trash or import or export from there. The duplicate finder has its own \"Find duplicates\" button in the library."],
      whatHappens:
        "A deleted tile sits in the trash, with its own retention setting, before it is gone for good. The duplicate finder only flags near-identical tiles by text similarity; it never deletes anything on its own. \"Recent messages\" keeps a short list of error messages that flashed by in a toast, with time and place; copy or clear the list from its window. Settings alone can be saved to a file and loaded again from the same group.",
      settings: ["Duplicate finder, trash retention, import and export: Expert menu → System & data → Data & maintenance", "Take a backup before a reset or a delete-everything: Expert menu → System & data → Data & maintenance", "Recent messages and how much it keeps, settings export and import, undo button: Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "system",
      title: "Standalone: Backup manager",
      whatItIs: "The backup manager creates encrypted, compressed snapshots of the entire library and settings.",
      howTo: [
        "Open the Backup manager from Settings.",
        "Turn on automatic backups and pick an interval, or create one manually.",
        "Save a backup to a file of your choice or import one from a file with the two buttons beside Create backup.",
        "Restore from the snapshot list, choosing data only, settings only, or both.",
      ],
      whatHappens:
        "Retention tiers (daily, weekly, monthly, yearly) thin out older snapshots automatically instead of keeping every one forever. A backup is either whole or absent; a failed one leaves nothing behind. The same window also has a usage-statistics section (totals, a 14-day activity chart, most-used tiles, most-used tools with a list of the ones never touched), unrelated to the backups themselves, plus a storage breakdown of what is taking up disk space. Each snapshot can open its contents: counts, the newest change inside it, the version that made it, and every tile it holds, searchable and sortable, with a button to restore one tile on its own. The statistics can be saved as CSV, TXT or JSON. One row per kind of kept data (trash, copy history, copies from other apps, recordings, temporary exports, edit states) shows how much it holds and how long it is kept, jumps to its setting and can clean it up on the spot.",
      settings: [
        "Interval, retention tiers, password, restore scope: Backup manager (its own window, opened from Settings)",
        "Usage statistics, how many tiles it lists: same window, below the backups",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Standalone: Quick launcher",
      whatItIs:
        "The quick launcher is a small always-on-top popup that searches every tile's name and text as you type: a plain substring match, not a fuzzy one, so a name hit ranks above a text hit but a typo will not find a match.",
      howTo: ["Press Ctrl+Alt+P, or the configured hotkey, from anywhere.", "Type to search, then click or press Enter on a result to copy it."],
      whatHappens: "The launcher closes itself right after copying, so it never lingers on screen.",
      settings: [],
    },
    {
      section: "organising",
      title: "Standalone: Multiple views",
      whatItIs:
        "A view is one grid page with its own size and arrangement.",
      howTo: [
        "Click plus next to the view tabs to add a view.",
        "Right-click a tab to rename, resize, recolor, or delete it.",
        "Move a tab with \"move further left\" or \"move further right\" in its menu, or drag it onto another tab.",
      ],
      whatHappens: "A tile normally sits in exactly one view (adding it elsewhere moves it there) unless \"Tile in several views\" is on, which ships off. Deleting a view does not delete the tiles placed on it; a tile also shown in another view keeps working there.",
      settings: ["Multiple views, tile in several views, maximum views: Expert menu → Features → Views and quick grid"],
    },
    {
      section: "look",
      title: "Standalone: Theme editor",
      whatItIs: "The theme editor adjusts any built-in theme's colors and saves the result as a new theme.",
      howTo: [
        "Open the theme editor: Expert menu → Appearance → Visual.",
        "Custom themes must be on, which they are by default.",
        "Pick a base theme, adjust colors, and save under a new name.",
      ],
      whatHappens: "A custom theme appears in the regular theme dropdown alongside the built-ins; deleting it does not affect the theme it was based on.",
      settings: [],
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
      section: "copying",
      title: "Hotkeys",
      whatItIs:
        "Two independent hotkey types exist: one global combination that opens the quick launcher, and one combination per tile.",
      howTo: [
        "Set the global hotkey in Settings (default Ctrl+Alt+P).",
        "Give a tile its own hotkey from its editor.",
        "Choose whether it should paste as well as copy.",
        "\"All shortcuts\", next to the global hotkey field, opens one list: global and per-tile hotkeys on top, searchable, then the keys of every window.",
        "Each row rebinds or clears on the spot.",
        "If a combination is already used, the field explains why instead of accepting it silently.",
      ],
      whatHappens:
        "A combination must include Ctrl or Alt; Shift alone, or a bare key, is refused so ordinary typing is never swallowed. The app hotkey always wins a conflict; among tiles, the first one in the list keeps a combination two tiles happen to share. Importing a backup that carries a colliding hotkey drops the losing one instead of failing the import, and the import summary says how many were dropped. A tile with its own hotkey shows a small keyboard symbol on it.",
      settings: [
        "Global hotkey, all shortcuts: Settings",
        "Per-tile hotkeys, paste-after-copy: Expert menu → Features → Copying",
        "Hotkey badge, badge opacity: Expert menu → Tiles → Tiles",
      ],
    },
    {
      section: "system",
      title: "Trash & restoring tiles",
      whatItIs: "A deleted tile moves to the trash first, not straight to permanent deletion.",
      howTo: [
        "Delete a tile as normal: it goes to the trash, and Undo brings it back.",
        "Open the trash from the expert menu's data group.",
        "Restore a tile from there, or let retention clear it automatically.",
      ],
      whatHappens:
        "Retention is set in days; zero means keep forever. Restoring places the tile in the active view's first free cell, not necessarily its original view or cell, and does not bring back a floating button, folder membership, or hotkey it had before deletion; those need setting up again.",
      settings: ["Trash retention: Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "system",
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
      section: "system",
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
      section: "system",
      title: "Import & export",
      whatItIs: "Plain CSV, TXT or JSON export covers text alone; a full backup file covers everything.",
      howTo: [
        "Export or import from the expert menu's data group.",
        "Pick a format from the dropdown next to the Export button; a CSV or JSON import shows a name and duplicate status per item first.",
        "A full backup restores directly, from its own window.",
        "Confirm to actually import, or cancel and nothing changes.",
        "\"Import folder\" in Settings turns a folder of .md or .txt files into tiles, one per file, after a preview with a checkbox per file.",
      ],
      whatHappens: "CSV and TXT round-trip text alone; JSON also carries the full rotation texts of a tile with rotation, tags, notes, hotkey, and color, none of which fit cleanly into the other two. Images, files, layout, and settings need the full backup format instead. Import decides CSV or JSON by the file's own content, never its extension. A folder import only reads: nothing is moved or deleted. The file name becomes the name, each sub-folder becomes a tag, and an unreadable file is grayed out with the reason.",
      settings: ["Import and export: Expert menu → System & data → Data & maintenance", "Folder import depth and file limit: Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "system",
      title: "Markdown folder sync (beta)",
      whatItIs:
        "Folder sync mirrors every text tile as its own readable Markdown file in a chosen folder.",
      howTo: [
        "Turn it on and pick a folder in the expert menu.",
        "Edit a tile normally, or edit its Markdown file directly; both directions stay in sync.",
      ],
      whatHappens:
        "Unlike the database, the Markdown files are plain text: anyone with access to that folder can read the content, so a synced or shared folder should be avoided for anything sensitive. A tile with an attached file, image, or PDF syncs as a reference: the header carries the path and the PDF page, never the file itself, so another machine needs the same path for the button to work there. A screenshot pasted straight into a tile has no path at all and stays in the database only. A confidential tile is never written to the folder, and a file never changes a confidential tile. A file whose tile sits in the trash does not bring it back as a second tile. A view or folder named in an incoming file that does not exist yet on this machine is created automatically, matched by name. Deleting a file in the folder does not delete the tile (it is written again on the next round); deleting the tile in the app does take its file with it. The file name is the tile's permanent id, so renaming a file by hand produces a second tile instead of a renamed one. If a tile and its file changed at the same time, the file wins that round and the replaced version is kept as a version of the tile; the other way round the app wins the file and the file's text is kept beside it as a conflict copy. Files this app did not write, or has never seen before, are never changed or deleted, and a file past 8 MB is skipped whole rather than cut short. Picking a different folder asks whether the old one should be deleted, and that delete only ever removes the files this app wrote itself, never a sub-folder, stopping entirely if anything unknown is in there. An unreachable folder pauses the sync after a few failed rounds instead of retrying forever.",
      settings: ["Folder sync, sync folder, poll interval, pause after failed rounds: Expert menu → Features → Prompt sync"],
      tip: "This is a beta feature: keep the sync folder somewhere only trusted eyes can read.",
    },
    {
      section: "basics",
      title: "Screenshot tool",
      whatItIs: "The screenshot tool captures part or all of the screen into a new tile.",
      howTo: [
        "Click the Capture button left of the input field and choose \"Screenshot\".",
        "Drag a region, or click a window to capture it whole, including protected ones.",
        "In the confirmation dialog, save it as a tile, a file, or leave it on the clipboard.",
      ],
      whatHappens:
        "The app's own windows are hidden for the moment of capture so they never end up in the shot. The result reaches the clipboard immediately; saving it as a tile or file is a separate, optional step. Saving into the screenshots folder never replaces an existing file; it takes the next free name.",
      settings: ["Screenshot tool, save location, format and quality, preview size, capture delay: Expert menu → Capture → Screenshot", "Always save screenshots: Settings"],
    },
    {
      section: "basics",
      title: "Screenshot editor",
      id: "screenshotEditor",
      whatItIs: "Opens right after a capture, before the shot is saved: crop, mark up, and annotate it before it becomes a tile.",
      howTo: [
        "Take a screenshot; the editor opens automatically over the capture.",
        "Pick a tool from the toolbar: crop, pen, marker, line, arrow, rectangle, ellipse, text, step number, pixelate, focus area, or hand.",
        "Right-click a rectangle, ellipse, line, arrow or pixelated area to duplicate or delete it; a focus area can only be deleted.",
        "Arrow keys move the selected shape, Ctrl+arrow resizes it, Shift takes bigger steps, Escape deselects.",
        "\"Resize\" changes the image's real pixel size; \"Show original\" (or V) shows the untouched capture while held, a tap locks it.",
        "Undo/redo steps back through edits; save when done.",
      ],
      whatHappens:
        "Every edit is one step in an undo/redo list capped by its own setting; pixelate always samples the original screenshot, never an already-edited layer, so blurring a spot never smears earlier marks. Snap-to-edges lines a shape up with the image border, its center and the other shapes while dragging, with a thin guide line; hold Alt to drag freely. A text label can get a background plate or an outline, so light text stays readable on a light background. Undo covers every step, including a duplicate, a delete, a resize and a run of arrow-key moves, which counts as one step. Once zoomed in, the hand tool (or holding Space, or the middle mouse button) drags the view around for pixel-precise work, and the crop tool shows a resize cursor at each handle and a move cursor inside the current selection. Switching the editor off (below) skips straight to the confirmation dialog instead.",
      settings: ["Screenshot editor, default tool, snap to edges, text background, default color, undo limit: Expert menu → Capture → Screenshot editor. The editor remembers the last size you used for each tool.", "Mouse wheel on number fields: Expert menu → Capture → Both editors"],
    },
    {
      section: "basics",
      title: "Open a file in the editor",
      whatItIs: "Opens an existing picture or video in whichever editor its content calls for, instead of only ever reaching an editor through a fresh capture or recording.",
      howTo: [
        "Click the Capture button left of the input field, choose \"Open a file in the editor\" and pick the file.",
        "Or right-click an image or video tile or library row and choose Edit image or Edit video.",
        "Edit as usual; saving asks where to put the result.",
      ],
      whatHappens:
        "The file's content decides which editor opens, not its extension: a video saved with a picture extension still opens the video editor. The editor always works on a copy, so the original file is never changed; the save dialog starts in the original file's own folder. A file past the configured size limit is rejected before it loads.",
      settings: ["Largest file to open: Expert menu → Capture → Both editors"],
    },
    {
      section: "basics",
      title: "Screen recording",
      id: "screenRecording",
      whatItIs: "Records a window, a monitor, or a dragged region as a video, next to the screenshot tool.",
      howTo: [
        "Click the Capture button left of the input field and choose \"Record screen\" {icon:record}; pick a window, drag a region, or click empty desktop for the whole screen. It is the same overlay as the screenshot tool.",
        "An optional countdown toast counts down before capture actually starts.",
        "A small floating control bar shows elapsed time and file size, two audio mute buttons, and offers Pause/Resume, Stop and Discard; two optional global hotkeys and the tray menu can stop or pause/resume it too, live only while it is actually running. A window recording whose recording area is All screens or Current screen also gets a switch-target button, to swap what it is capturing without starting over.",
        "Stopping offers Save, Save as…, Copy, Edit or Discard.",
      ],
      whatHappens:
        "MP4 (H.264) is always what gets written while recording; three quality steps (Small/High/Lossless) trade file size for detail, and a format setting can turn the result into WebM, AVI, MOV, MKV, animated WebP or APNG afterward (the last three carry no audio and are capped at 30 seconds). Anything other than plain MP4, or the Lossless step, needs ffmpeg (a separate download, offered inline when missing); a failed or cancelled conversion always falls back to the untouched original recording. System audio and microphone audio both record on by default, each into its own track: the microphone track is captured but not mixed in audibly until you bring it in yourself from the video editor; unplugging the microphone mid-recording keeps picture and system audio going and just marks that track as lost. The control bar's two audio buttons can mute either source instantly while recording without dropping its track: a muted source keeps recording silence and can be unmuted again in the same recording. Save and Copy share one recording folder; Edit opens the clip in the video editor without deciding its fate, so closing the dialog afterward without saving or copying still discards the file. Dragging the bar itself (not its buttons) moves it anywhere, including onto another monitor, and stops it following the recorded window for the rest of that one recording; the next recording starts following again, and the bar is otherwise never part of the recording. If the recorded window closes on its own, the recording stops automatically and keeps what was captured so far. A recording that never reached its own stop path (a crash, a forced shutdown) is removed automatically once it clears a grace period, so a just-stopped recording that is still finalising is never touched. The recording folder's row in the expert menu shows how many videos are in it and how much space they use, with a \"Clean up…\" button that lists files older than 7, 30 or 90 days and the space they would free before deleting anything; nothing is removed without that confirmation. An optional keys overlay records which keys are pressed while a recording runs, as key identifiers and timestamps only, never any text; it shows every key by default, including typed text, and skips a password field when Windows reports it as such, though this is not guaranteed. A \"Shortcuts only\" switch restricts capture to combinations with Ctrl, Alt or Win plus Enter, Tab, Esc, Backspace, Delete, Insert, arrows, Home, End, Page Up, Page Down and F keys, for a recording with confidential input. The same cleanup dialog has a checkbox per file: \"Export selected\" converts every checked recording in one go, to a chosen format, quality and folder, and reports how many finished. A new install deletes unused recordings after 90 days (expert menu, 0 = keep forever).",
      settings: ["Screen recording, countdown, bitrate, quality, format, recording folder and cleanup: Expert menu → Capture → Screen recording", "Cursor, audio sources and device, stop/pause hotkeys, tray entries, auto-remove broken recordings, keys overlay, frame around a recorded region and its color: Expert menu → Capture → While recording"],
    },
    {
      section: "basics",
      title: "Camera recording",
      whatItIs: "Records the webcam into its own video, alone or next to a screen recording.",
      howTo: [
        "Turn on camera recording in the expert menu.",
        "Click the Capture button left of the input field and choose \"Record camera\" {icon:record}.",
        "In its window, check the live preview, pick the audio source, resolution and folder, then Start.",
      ],
      whatHappens:
        "The camera window, or its status dot in the recording bar, stays visible the whole time, so the camera never runs unnoticed. The camera goes into its own file and is never burned into a screen recording. Started during a screen recording, both share one start point, and the video editor then opens the screen recording with the camera laid on top as an overlay that can still be moved.",
      settings: ["Camera recording, default camera, resolution, frame rate: Expert menu → Capture → Camera recording"],
      tip: "Off by default.",
    },
    {
      section: "basics",
      title: "Video editor",
      whatItIs: "A small editor for a finished recording or any attached video, opened from the recording's own result dialog or a video prompt's right-click menu.",
      howTo: [
        "From a fresh recording: click More, then Video editor, in the result dialog.",
        "From any video already saved as a prompt: right-click it and choose \"Edit video\".",
        "Trim, crop, change speed, fade in/out, resize (typed dimensions, a percentage, or a quick preset), and add image, freeze-frame, text, pixelate or focus area overlays on the timeline.",
        "Export: a lossless fast path applies when only trimming; otherwise the whole clip re-encodes once.",
      ],
      whatHappens:
        "Nothing is processed until you export: the live preview plays your edits directly. A filmstrip and waveform give a zoomable overview of the whole clip for frame-exact navigation. The preview itself also zooms with the scroll wheel (up to 8×) and pans by dragging (the middle mouse button always, the left button once zoomed in) for placing a small overlay precisely on a large recording. The export destination is an editable field: type a path directly, or use \"Export to…\" to keep the file name and change only the folder; once an export has finished, \"Save as…\" moves that finished video to another place. Switching format only rewrites the extension. \"Export and copy\" skips naming entirely, exporting into a self-cleaning temporary copy (removed after an hour, and again on the next launch) and placing the finished file straight on the clipboard. A few ready-made export presets (unchanged quality, a smaller size for chat, an animated GIF) remember your last choice. Besides MP4, WebM and GIF, the export format list also offers AVI, MOV, MKV, animated WebP and APNG, each with a short note on what it's for; GIF, WebP and APNG never carry audio, warn once the clip runs past ten seconds, and are capped to a maximum resolution since all three grow large fast. Undo/redo covers every editing step, capped by an adjustable limit. \"Add a video\" lays another clip on top as an overlay, and the Keys overlay turns the keys pressed during a recording into a keyboard picture. Right-click an overlay's row to duplicate or delete it. Dragging a crop box or overlay snaps to the frame edges, its center and other overlays; hold Alt to drag freely. A new overlay starts from the color, thickness and size last used for its kind. Unfinished cuts and overlays are remembered, so reopening the same recording offers to continue. The export file name has its own field; an existing file is never replaced, the export takes the next free name.",
      settings: ["Video editor, undo limit, remember overlay values, snap to edges: Expert menu → Capture → Video editor", "Remember unfinished video edits, and for how many days: Expert menu → Capture → Both editors"],
      sketches: ["editorTimeline"],
    },
    {
      section: "basics",
      title: "Text recognition (OCR)",
      id: "ocr",
      whatItIs: "Pulls text out of a screenshot or an image file using Windows' own on-device recognizer. It runs fully offline and nothing leaves the machine.",
      howTo: [
        "From the composer: the Capture button left of the input field {icon:ocr} offers text from a screenshot or from an existing image file.",
        "From a screenshot already taken: the snip dialog's \"Recognize text\" button.",
        "Review the result, then copy it or insert it directly.",
      ],
      whatHappens:
        "Recognition runs entirely on the device through Windows' own engine. The result dialog can auto-copy the text and join broken lines back into paragraphs, both switchable. A missing language pack opens the same dialog voice dictation uses; see \"Language packs\" below.",
      settings: ["Text recognition, auto-copy the result, join lines, enlarge small images, recognition language: Expert menu → Capture → Text recognition"],
    },
    {
      section: "basics",
      title: "Voice dictation",
      id: "voiceDictation",
      whatItIs: "The microphone button {icon:mic} turns speech into text, recognized entirely on-device.",
      howTo: [
        "Click the microphone icon next to the composer, or in a tile's editor.",
        "Speak; the text appears as it is recognized.",
        "Click the icon again, press Escape, or save/close to stop.",
      ],
      whatHappens:
        "Recognition runs locally through Windows' own speech engine; nothing leaves the machine unless online recognition is explicitly allowed. Spoken punctuation words (\"comma\", \"new line\", …) turn into real punctuation; say \"literal\" before a word to type it instead of triggering it as a command. A missing language pack opens the language dialog directly, shared with text recognition (OCR). An optional global hotkey starts and stops a session without opening the window at all, delivering the text to the clipboard, pasted, or straight into the composer; the tray icon's tooltip shows \"recording\" for as long as it runs.",
      settings: ["Voice dictation, punctuation commands, auto-capitalize, silence auto-stop, insert position, global hotkey: Expert menu → Features → Dictation"],
    },
    {
      section: "basics",
      title: "Language packs",
      whatItIs: "Text recognition and voice dictation share Windows' own language packs and the same install dialog.",
      howTo: [
        "Pick a language from the recognition-language dropdown; a missing pack opens the install dialog automatically.",
        "Installing a pack needs administrator rights: Windows asks for them once, through its own confirmation prompt.",
        "\"Manage text recognition languages…\" (or \"Manage dictation languages…\" for dictation) at the bottom of the language dropdown reopens the same dialog any time.",
      ],
      whatHappens:
        "On some Windows builds even listing what is installed needs administrator rights; if so, a button re-runs that check through the same elevation prompt. A background watcher notices when an installed pack's version changed or a Windows update silently removed it, and flags it without asking to reinstall right away; dismiss it for a while with \"remind me later\", or turn the watcher off entirely.",
      settings: ["Auto-pick the recognition language, background watcher for missing or changed packs: Expert menu → Capture → Text recognition, and again under Expert menu → Features → Dictation"],
    },
    {
      section: "basics",
      title: "Color picker & pick history",
      id: "colorPicker",
      whatItIs: "The color picker is a pipette that reads any on-screen pixel as hex, RGB, or HSL.",
      howTo: [
        "Click the Capture button left of the input field and choose \"Pick a color from the screen\".",
        "Move the loupe to the pixel wanted; scroll or use plus/minus to zoom.",
        "Click to copy the color; choose \"Recent colors\" in the same menu to reopen the last few picks.",
      ],
      whatHappens:
        "Nothing is ever saved to a tile automatically; a pick only reaches the clipboard. The pick history holds the last few colors and survives a restart, capped by its own setting.",
      settings: ["Color picker, zoom, format, pick-history length: Expert menu → Capture → Color picker"],
      tip: "Pick a color format (hex, RGB, or HSL) from the toast that appears right after a pick.",
    },
    {
      section: "basics",
      title: "Rotation",
      whatItIs: "A tile can hold a list of texts instead of one; each copy serves the next rotation text.",
      howTo: [
        "Rotation is on by default; turn it off in the expert menu if unused.",
        "In a tile's editor, turn on \"Rotation\" above the text area.",
        "The list of rotation texts then replaces the plain text field.",
        "Add more rotation texts with \"+ Rotation text\"; icons beside each one reorder or remove it.",
        "Choose order (sequential) or shuffle (random, no repeats until every rotation text is used).",
        "Copy the tile as normal: it always serves the next rotation text, never asks which one.",
      ],
      whatHappens:
        "A small badge counts up on a grid tile, a floating button, and a button folder member, like \"2/5\"; a tile-folder member shows only its file-type icon, not this badge. In order mode it is the absolute position; in shuffle mode it counts draws within the current cycle and restarts at 1 once every entry has been shown once, so it never jumps to an arbitrary position. The count survives restarts and full backups; a CSV/TXT import creates a new tile with a fresh pointer, so only the list of texts and its order or shuffle mode carry over, not the exact position. Turning rotation off on a tile with several entries keeps them all; only the first one is used until it is turned back on. The optional \"Rotate on paste\" moves a tile with rotation to its next text on every real paste, not only when it is copied.",
      settings: ["Rotation (on by default), default order or shuffle, badge, maximum rotation texts, rotate on paste: Expert menu → Features → Rotation. The per-tile switch itself sits above the entry area in the editor."],
    },
    {
      section: "system",
      title: "The expert menu",
      whatItIs: "The expert menu holds every optional feature behind its own switch and parameters.",
      howTo: [
        "Open Settings, then the expert menu.",
        "Pick a tab, or type in the search box to scan every tab at once.",
        "Turn on \"Only changed\" to see just what has been adjusted from the defaults.",
      ],
      whatHappens:
        "A feature that ships switched off carries a small dot badge on its label, with a tooltip explaining what it does. Reset works at three levels: one value's own undo icon, a whole group's reset icon in its header, or every setting at once from the general reset action. A profile first shows how many settings would change and is never remembered as a mode; All on leaves out anything destructive or anything that sends data out. The search also finds settings outside the expert menu, such as the settings window or the backup manager, marked with where they live; a click opens that place.",
      settings: ["Tip line, highlighted search hits and diagrams in this help window: Expert menu → System & data → Help"],
    },
    {
      section: "system",
      title: "Keyboard-only use",
      whatItIs: "Arrow keys move a focus ring across the grid for keyboard-only copying.",
      howTo: [
        "Keyboard navigation is on out of the box; turn it off in Expert menu → System & data.",
        "Use the arrow keys to move the focus ring, Enter or Space to copy.",
        "Tab still moves through buttons and fields everywhere else in the app, as usual.",
      ],
      whatHappens: "The focus ring fades out a short while after the window loses focus, so it does not linger over a tile once another app is in use.",
      settings: ["Keyboard navigation, fade delay: Expert menu → System & data → System"],
    },
    {
      section: "system",
      title: "Where your data lives",
      whatItIs:
        "Everything lives in one local SQLite database plus its small companion file.",
      howTo: [
        "Installed: the data folder is under AppData, shown and changeable in the expert menu.",
        "Portable: the data folder sits next to the exe, so it all fits on a USB stick.",
      ],
      whatHappens: "Moving from portable to installed, or back, is detected automatically and offered as a one-time migration: existing data is never silently left behind.",
      settings: ["Data folder: Expert menu → System & data → System"],
    },
    {
      section: "system",
      title: "Remote access & links",
      whatItIs: "Two optional ways for other programs to reach the app: links that open or copy a tile, and a password-protected receiver for commands.",
      howTo: [
        "Turn on promptsaver links in the expert menu to let a link open or copy a tile.",
        "Turn on remote access to let another program on this computer send commands; copy the password shown once.",
        "To allow other devices, turn on \"Allow from the local network\" and list the allowed address ranges.",
        "Open \"Certificate…\" to see the fingerprint, save the certificate for the other device, or make a new one.",
        "For iPhone, iPad or Mac, turn on \"Short certificate for Apple devices (825 days)\"; switching asks first and then makes a new certificate.",
      ],
      whatHappens:
        "Both are off by default. A link can only open or copy, never change or delete, and the first open and the first copy each ask once. The receiver never starts without a password, and the app keeps it in a form it cannot read back; ten wrong tries lock that caller out for ten minutes. Requests are size and rate limited and logged without the password. A banner with the port and an off button stays on screen while it runs. Without the network switch it only listens on this computer; with it, the connection is encrypted and every device has to trust the saved certificate. A new certificate replaces the old one at once, so every device needs the new file; make one after this computer's network address changed. Resetting settings switches remote access and links off. The certificate is valid for 10 years, or for 825 days with the short setting, which Apple devices require; devices then need the new certificate about every two years. An expired certificate is renewed when the receiver starts, with a message.",
      settings: ["Remote access, network access, address ranges, port, requests per minute, password, certificate, links: Expert menu → System & data → Remote access"],
      tip: "Anyone in the allowed ranges who has the password can read and change tiles: keep the ranges narrow.",
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
        "If the app is already running, the command goes to it and its answer is printed; otherwise the command runs on its own without a window. Only one copy ever touches the data at a time. Without --yes a changing command only says what it would do. Folder locations, the vault, the backup password and every remote access setting, recording copies from other apps and the link switch cannot be changed from here, only in the app.",
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
      settings: ["Demo mode: Expert menu → System & data → System"],
    },
    {
      section: "system",
      title: "Updates & security",
      whatItIs: "An optional update check looks for a newer version and only talks to the update server.",
      howTo: [
        "Leave update checks on, the default, to be notified of a new version.",
        "Pick Never beside Auto update in Settings to stop automatic checks.",
        "Accept an offered update to download and install it with one click.",
      ],
      whatHappens:
        "The update check, together with a first-run download of the Microsoft WebView2 installer if that runtime is missing, is one of the only two kinds of network traffic the app makes on its own. A downloaded update is verified against a published checksum before it runs: a missing or mismatching checksum blocks the install instead of running an unverified file. The third and last kind is ffmpeg, used for video export/conversion: an installed build ships it and never touches the network for it, while a portable build offers a one-time download only when a video feature actually needs it and only with your explicit consent, never automatically, never in the background. After an update, a small notice links to what changed. The About window, next to \"Check for updates\" in Settings, shows the version, the full history of changes and every bundled open-source component with its license.",
      settings: ["Auto update and how often it checks: Settings"],
    },
    {
      section: "system",
      title: "Common questions",
      whatItIs: "This section answers the questions that come up most often.",
      howTo: [
        "\"My screenshot came out empty\": screen-capture exclusion is working as intended; switch on \"Allow screenshots\" in the expert menu if unwanted.",
        "\"The app will not start\": it needs the Microsoft WebView2 runtime, offered automatically by the installer.",
        "\"I cannot find a setting\": the expert menu's search box scans every tab at once.",
        "\"A hotkey does not do anything\": another app may already hold that combination; the editor explains why.",
      ],
      settings: [],
    },
  ],
  de: [
    {
      section: "basics",
      title: "Erste Schritte",
      whatItIs: "Eine Kachel ist ein gespeicherter Eintrag im Raster; ein Klick darauf kopiert ihren Text in die Zwischenablage.",
      howTo: [
        "Doppelklick auf eine leere Rasterzelle oder den Plus-Button erstellt eine Kachel.",
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
        "Auf das Zahnrad in der Kopfzeile klicken, um es zu öffnen.",
        "Sprache, Theme sowie Textgröße und Schriftart der Kacheln wählen.",
        "Festlegen, ob das Fenster im Vordergrund bleibt, mit Windows startet, im Hintergrund startet und sich beim Schließen in den Hintergrund legt, statt zu beenden (bei einer neuen Installation ab Werk an; eine portable Kopie beendet sich immer).",
        "Den globalen Hotkey, Auto-Einfügen bei Doppelklick und das Speichern von Screenshots im Screenshots-Ordner einstellen.",
      ],
      whatHappens:
        "Jede Änderung gilt sofort, ohne Speichern-Button. \"Automatisch\" als Sprache folgt Windows und fällt auf Englisch zurück. Zurücksetzen stellt jede Einstellung auf die Vorgabe zurück und behält die Kacheln; Löschen entfernt alle Daten und fragt vorher nach.",
      settings: [],
    },
    {
      section: "basics",
      title: "Einführung",
      whatItIs: "Eine Einführung in fünf Schritten, die direkt nach dem Erstellen der Beispiel-Kacheln einmal automatisch angezeigt wird.",
      howTo: [
        "Beim ersten Start: öffnet sich direkt nach dem Anlegen der Beispiel-Prompts automatisch.",
        "Später: über den Einstellungsdialog oder die Aktion „Einführung erneut öffnen“ im Expertenmenü erneut öffnen.",
        "←/→ oder die Punkte wechseln zwischen den Schritten, Enter fährt fort, Escape oder „Überspringen“ beendet die Einführung jederzeit; Escape zeigt eine Meldung mit Rückgängig, die die Einführung wieder öffnet.",
      ],
      whatHappens:
        "Im letzten Schritt lassen sich der Screenshot-Editor, die Farbauswahl oder die Texterkennung direkt aktivieren; dabei wird jeweils die ausgelieferte Standardeinstellung angezeigt. Gespeichert wird erst, wenn die Einführung tatsächlich abgeschlossen ist, und nur für Schalter, die wirklich geändert wurden. Ein beim ersten Start erscheinender Hinweis zum Datenordner oder zu WebView2 hat immer Vorrang; die Einführung wartet einfach, bis sie an der Reihe ist.",
      settings: ["Beim ersten Start automatisch öffnen: Expertenmenü → System & Daten → Hilfe"],
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
      tip: "Farbformat (Hex, RGB oder HSL) im Hinweis wählen, der direkt nach der Auswahl erscheint.",
    },
    {
      section: "basics",
      title: "Platzhalter",
      whatItIs:
        "Platzhalter-Markup im Text einer Kachel öffnet ein Ausfüll-Popup, bevor der fertige Text die Zwischenablage erreicht.",
      howTo: [
        "{#{PlatzhalterName}#} irgendwo in den Text eintippen.",
        "Für einen sich selbst ausfüllenden Wert einen dynamischen Chip unter dem Textfeld anklicken statt zu tippen.",
        "Klick auf die Kachel zum Kopieren: das Ausfüll-Popup öffnet sich automatisch.",
        "Pro Platzhalter einen Wert eingeben, bei Auswahl-Platzhaltern einen wählen, dann bestätigen.",
      ],
      whatHappens:
        "Nur der ausgefüllte Text erreicht die Zwischenablage; das rohe {#{...}#}-Markup wird nie kopiert. Ein dynamischer Platzhalter (Datum, Uhrzeit, Zwischenablage, UUID) nutzt eine eigene Schreibweise, {#{/date\\}#}, und füllt sich ganz ohne Popup selbst aus. Das Popup kann sich den zuletzt eingetippten Wert eines Platzhalters merken und ihn wieder anbieten. Eine Liste fester Werte im Expertenmenü wird für einen gleichnamigen Platzhalter vorgeschlagen; sie wird immer nur vorgeschlagen, nie von selbst eingesetzt.",
      settings: ["Dynamische Platzhalter, Auswahl-Platzhalter, Auswahl merken, eingetippte Werte merken, Liste fester Werte, Datums- und Zeitformat: Expertenmenü → Funktionen → Platzhalter"],
      sketches: ["placeholderAnatomy"],
    },
    {
      section: "basics",
      title: "Der Editor",
      whatItIs: "Der Editor ist im Kern ein einfaches Textfeld; einige Funktionen (Markdown-Vorschau und private Notizen sind ab Werk an, im Expertenmenü abschaltbar) verleihen ihm Struktur und Versionen.",
      howTo: [
        "Editor einer Kachel öffnen: Doppelklick auf die Kachel oder Bearbeiten in ihrem Menü.",
        "Markdown-Vorschau, Syntax-Hervorhebung, Versionen oder private Notizen im Expertenmenü anpassen.",
      ],
      whatHappens:
        "Die Versionen speichern jede Änderung, sodass eine frühere wiederhergestellt werden kann. Die Markdown-Vorschau erscheint neben oder anstelle des Textfelds, ohne den gespeicherten Inhalt zu verändern. Private Notizen werden nie mit dem Text kopiert oder exportiert. Der offene Text wird alle paar Sekunden als Entwurf gesichert; schließt sich das Fenster oder die App vor dem Speichern, bietet der nächste Start den Entwurf einmal wieder an. Eine vertrauliche Kachel bekommt nie einen Entwurf. Wer eine neue Kachel speichert, die einer vorhandenen sehr ähnelt, sieht einen leisen Hinweis, das Speichern wird nicht aufgehalten. Bei einem Text mit Markdown legt \"Formatiert kopieren\" im Editor und im Kachel-Menü zusätzlich die gerenderte Fassung in die Zwischenablage, sodass Word oder ein Mailprogramm die Formatierung zeigt. In der Liste der Versionen behält ein zweiter Button neben Wiederherstellen die gewählte Fassung als neue Kachel und lässt die aktuelle unverändert.",
      settings: ["Markdown-Vorschau, Syntax-Hervorhebung, Versionen, private Notizen: Expertenmenü → Funktionen → Editor", "Textfeld in dicktengleicher Schrift: Expertenmenü → Funktionen → Editor", "Formatiert kopieren, Live-Längenzähler, wortweiser Vergleich der Versionen, Verwerfen bestätigen, Entwurf automatisch sichern und wie oft: Expertenmenü → Funktionen → Editor", "Hinweis auf ähnliche Prompts beim Speichern: Expertenmenü → System & Daten → Daten & Wartung"],
      tip: "Markdown-Vorschau und private Notizen sind ab Werk aktiviert.",
    },
    {
      section: "copying",
      title: "Kopieren",
      whatItIs: "Kopieren ist der Ablauf zwischen dem Klick auf eine Kachel und dem Eintreffen ihres Texts in der Zwischenablage.",
      howTo: [
        "Einmaliger Klick auf eine Kachel kopiert sie.",
        "Schneller zweiter Klick oder Enter fügt den Text zusätzlich ein, falls Auto-Paste aktiv ist.",
        "Strg-Ziehen einer Kachel auf ein anderes Fenster legt ihren Text direkt dort ab.",
      ],
      whatHappens:
        "Eine kurze Bestätigungsblase oder ein Aufblinken bestätigt den Kopiervorgang. Verketten, mehrere Kacheln hintereinander anklicken, fasst sie zu einem Eintrag in der Zwischenablage zusammen, statt sie zu überschreiben; Kette hier starten: Rechtsklick auf eine Textkachel beginnt direkt eine Kette mit dieser Kachel. Eine Kette bleibt beim Wechsel in ein anderes Programm bestehen. Solange eine Kette läuft, steht der Kettenknopf oben; ein Klick beendet sie. Auch Strg+V im App-Fenster beendet sie. Beim Verketten lässt sich ein Schritt in der Leiste über dem Raster ziehen, um ihn umzusortieren, oder per Rechtsklick entfernen. Verketten ist nicht dasselbe wie Rotation: Verketten fasst mehrere verschiedene Kacheln zusammen, Rotation ändert, was eine einzelne Kachel von Kopie zu Kopie enthält (siehe Rotation). Ist die Doppelklick-Dauer-an-Funktion eingeschaltet, bleibt der Kettenmodus nach dem Kopieren an, um mehrere Ketten nacheinander zu bauen. Der Streifen kann außerdem einen Schritt rot markieren, wenn er einen Platzhalter braucht, den kein früherer Schritt füllt; die Kette läuft trotzdem.",
      settings: ["Verketten und der Kettenstart im Kachelmenü: Expertenmenü → Funktionen → Kette", "Was zwischen zwei verketteten Schritten steht: Expertenmenü → Funktionen → Kette", "Kettenmodus nach dem Kopieren eingeschaltet lassen, zeigen, was jeder Kettenschritt braucht: Expertenmenü → Funktionen → Kette", "Auto-Einfügen bei Doppelklick: Einstellungen", "Eigene Hotkeys pro Kachel, Enter nach Auto-Einfügen, Drag-out, Fenster nach dem Kopieren ausblenden: Expertenmenü → Funktionen → Kopieren", "Welche Taste eine Kachel herauszieht und ob diese Taste herauszieht oder sie verschiebt: Expertenmenü → Funktionen → Kopieren"],
      sketches: ["flowToTarget", "chainSteps"],
    },
    {
      section: "alwaysOnTop",
      title: "Schwebende Buttons",
      whatItIs: "Schwebende Buttons sind kleine, immer im Vordergrund liegende Kopier-Buttons außerhalb des Hauptfensters, über jeder anderen App.",
      howTo: [
        "Rechtsklick auf eine Kachel, dann \"Schwebenden Button umschalten\", um sie als schwebenden Button anzuheften.",
        "Den schwebenden Button ziehen, um ihn zu bewegen; Rechtsklick für Größe, Bearbeiten oder Entfernen.",
      ],
      whatHappens: "Positionen bleiben nach Neustarts erhalten. Ein Button, dessen gespeicherter Bildschirm nicht mehr angeschlossen ist, öffnet sich auf dem Hauptbildschirm, als geparkt markiert, und springt von selbst zurück, sobald dieser Bildschirm wieder da ist. Eine Aktion im Expertenmenü holt alle schwebenden Fenster auf einmal auf den Hauptbildschirm.",
      settings: ["Schwebende Buttons: Expertenmenü → Funktionen → Schwebende Buttons & Button-Ordner", "Buttons parken, wenn ihr Bildschirm fehlt, lange geparkte ausblenden, alle zurückholen: Expertenmenü → Funktionen → Schwebende Buttons & Button-Ordner"],
    },
    {
      section: "alwaysOnTop",
      title: "Ordner",
      whatItIs: "Dieselbe Geste gruppiert an zwei Stellen: ein Element auf ein gleichartiges ziehen, um es in einen Ordner aufzunehmen.",
      howTo: [
        "Einen schwebenden Button auf einen anderen ziehen, um einen Button-Ordner zu bilden.",
        "Im Hauptfenster eine Kachel auf eine andere ziehen, um einen Kachelordner zu bilden.",
        "Ein Mitglied wieder herausziehen, um es freizugeben.",
      ],
      whatHappens:
        "Ein Button-Ordner oder ein Kachelordner zeigt bis zu vier Vorschauen seiner Mitglieder und eine Anzahl; das Herausziehen eines Mitglieds gibt es frei, und der Ordner löst sich auf, sobald nur noch eines übrig ist. Eine Kachel gehört immer nur zu einem Kachelordner gleichzeitig.",
      settings: ["Button-Ordner, Kachelordner: Expertenmenü → Funktionen → Schwebende Buttons & Button-Ordner"],
      tip: "Beide Arten sind ab Werk an; jede einzeln im Expertenmenü abschaltbar, wenn Elemente so nie gruppiert werden sollen.",
      sketches: ["folderTypes"],
    },
    {
      section: "alwaysOnTop",
      title: "Infobereich-Menü",
      whatItIs: "Das Rechtsklick-Menü des Infobereich-Symbols bietet schnellen Zugriff, ohne das Hauptfenster zu öffnen.",
      howTo: ["Rechtsklick auf das Infobereich-Symbol.", "Einen Eintrag aus \"Zuletzt kopiert\" oder \"Favoriten\" wählen, um ihn sofort zu kopieren."],
      whatHappens: "Das Menü kopiert direkt aus dem Infobereich; das Hauptfenster muss dafür nicht geöffnet werden. Das Menü erscheint als abgerundetes Fenster im Theme der App; ausgeschaltet kommt das schlichte Windows-Menü. Tippen ins Suchfeld des Infobereich-Menüs öffnet den Schnellstarter mit dem Getippten.",
      settings: ["Infobereich-Menü im Windows-11-Stil, Suchfeld, Bereich für zuletzt Kopiertes, Bereich für Favoriten, Anzahl der Einträge, Länge der Beschriftung: Expertenmenü → Funktionen → Infobereich-Menü"],
    },
    {
      section: "copying",
      title: "Kopien aus anderen Apps",
      whatItIs:
        "Eingeschaltet nimmt der Kopierverlauf auch Text auf, der irgendwo im System kopiert wurde, markiert als anderswo kopiert.",
      howTo: [
        "„Kopien aus anderen Apps aufnehmen“ im Expertenmenü einschalten.",
        "Text wie gewohnt irgendwo im System kopieren.",
        "Bibliothek → Kopierverlauf öffnen, den Herkunftsfilter auf „Anderswo“ stellen und nützliche Einträge als Kacheln speichern.",
      ],
      whatHappens:
        "Es wird nichts überwacht, bis die Funktion eingeschaltet ist. Kopien unter der Mindestlänge werden ignoriert, und die ältesten dieser Einträge fallen weg, sobald sie ihre Obergrenze überschreiten. Mit \"Vermutete Passwörter überspringen\" landet eine Kopie, die wie ein Passwort aussieht (eine Zeile, keine Leerzeichen, gemischte Buchstaben, Ziffern und Zeichen), nie im Eingang; das ist eine Schätzung, die im Zweifel lieber überspringt. Beim Ausschalten fragt die App, ob das Gesammelte behalten oder gelöscht werden soll; wer die Frage schließt, behält es. Nach dem Leeren des Eingangs bietet die Meldung einen Rückgängig-Button.",
      settings: ["Vermutete Passwörter überspringen, Mindestlänge, Obergrenze: Expertenmenü → Bibliothek & Kopierverlauf → Kopierverlauf"],
      tip: "Standardmäßig aus: es wird nichts überwacht, bis die Funktion eingeschaltet ist.",
    },
    {
      section: "organising",
      title: "Kachel-Verhalten & -Aussehen",
      whatItIs: "Diese Einstellungen regeln, wie eine Kachel auf die Maus reagiert und wie sie aussieht.",
      howTo: [
        "Rechtsklick auf eine Kachel für ihr Menü: Bearbeiten, Anheften, Verbergen, Löschen und mehr.",
        "Rechtsklick auf eine Kachel (oder Strg+Klick) wählt sie aus; solange du auswählst, zeigt jede Kachel ihr Kästchen.",
        "Eine Kachel auf eine beliebige Rasterzelle ziehen; auf einer besetzten Zelle loslassen, um zu tauschen.",
        "Hover-Lift, Schatten, Badges und ähnliche Optiken im Expertenmenü aktivieren und einen Kachelstil wählen (fett, Großbuchstaben, Monospace, kursiv, Namen mit Schatten, kompakt, Verlauf).",
      ],
      whatHappens: "Das sind nur Interaktion und Optik; nichts davon ändert, was eine Kachel kopiert.",
      settings: [
        "Kachel-Menü, Drag-to-Reorder: Expertenmenü → Kacheln → Kacheln",
        "Abblenden bis zum Überfahren, Anheben beim Überfahren: Expertenmenü → Kacheln → Kacheln",
        "Typ-Badges, Schatten, abgerundete Ecken, Kachelstil (fett, Großbuchstaben, Monospace, kursiv, Namen mit Schatten, kompakt, Verlauf): Expertenmenü → Kacheln → Kachel-Aussehen",
      ],
    },
    {
      section: "look",
      title: "Tooltips",
      whatItIs: "Das Überfahren einer Kachel mit der Maus zeigt ihren vollständigen Text, ihre Länge und eine eventuelle private Notiz.",
      howTo: [
        "Über eine Kachel fahren und die Verzögerung abwarten.",
        "Verzögerung oder Breite im Expertenmenü anpassen, falls der Standard nicht passt.",
      ],
      whatHappens: "Der Tooltip blendet sich nach seinem Timeout automatisch aus, selbst wenn die Maus stehen bleibt.",
      settings: ["Verzögerung vor Anzeige, Tooltip-Breite: Expertenmenü → Kacheln → Tooltips"],
    },
    {
      section: "organising",
      title: "Favoriten",
      whatItIs: "Ein Stern markiert eine Kachel als Favorit für schnellen späteren Zugriff.",
      howTo: ["Auf den Stern einer Kachel klicken, oder \"Favorisieren\" in ihrem Menü nutzen.", "Die eigene Favoriten-Ansicht öffnen, um nur markierte Kacheln zu sehen."],
      whatHappens: "Favoriten erhalten ihre eigene sortierbare Ansicht, unabhängig davon, wo die Kachel sonst liegt.",
      settings: ["Favoriten-Ansicht: Einstellungen. Eigene Sortierung: Expertenmenü → Kacheln → Favoriten"],
    },
    {
      section: "look",
      title: "Skalierung & Zoom",
      whatItIs: "Unabhängige Größenregler passen verschiedene Teile des Fensters getrennt voneinander an.",
      howTo: ["Einstellungen öffnen und die Skalierungs-Regler finden.", "UI-, Dialog-, Editor- oder Icon-Skalierung unabhängig voneinander anpassen."],
      whatHappens: "Jeder Regler beeinflusst nur seinen eigenen Teil des Fensters: eine höhere Icon-Skalierung vergrößert zum Beispiel keine Dialoge.",
      settings: ["Größe der Oberfläche und Icon-Größe: Einstellungen", "Dialog-, Editor- und Hauptknopf-Skalierung: Expertenmenü → Aussehen → Größen (%)"],
    },
    {
      section: "look",
      title: "Aussehen & Themes",
      whatItIs: "Acht integrierte Themes decken System, Hell, Dunkel und fünf Akzent-Varianten ab.",
      howTo: ["Ein Theme in den Einstellungen wählen.", "Den Theme-Editor, sobald aktiviert, öffnen, um Farben eines Themes anzupassen und ein eigenes zu speichern."],
      whatHappens: "\"System\" folgt live der Hell-/Dunkel-Einstellung des Betriebssystems. Ein benutzerdefiniertes Theme wird separat gespeichert; das integrierte, auf dem es basierte, bleibt unangetastet.",
      settings: ["Theme-Wahl: Einstellungen", "Theme-Editor: Expertenmenü → Aussehen → Visuell", "Kopfzeile in Akzentfarbe, milchige Dialoge, versteckte oder eingefärbte Scrollbalken, weiches Scrollen: Expertenmenü → Aussehen → Visuell"],
      tip: "Benutzerdefinierte Themes sind ab Werk an; werden sie im Expertenmenü ausgeschaltet, verschwindet auch der Editor-Button {icon:themeEditor}.",
    },
    {
      section: "organising",
      title: "Bibliothek & Suche",
      whatItIs: "Die Bibliothek listet jede Kachel als durchsuchbare, filterbare Liste, anders als das Raster, das nur zeigt, was passt.",
      howTo: [
        "Klick auf den Bibliothek-Button {icon:library} in der Werkzeugleiste öffnet sie.",
        "Tippen zum Suchen, oder Typ- und Farb-Filter zum Eingrenzen der Liste nutzen.",
        "Klick auf eine Zeile zum Kopieren.",
        "Der Knopf „Nach dem Kopieren schließen“ im Kopf der Bibliothek entscheidet, ob Kopieren die Bibliothek schließt, und merkt sich die Wahl.",
        "Das Menü zum Bearbeiten, auf das Raster Ziehen, Taggen oder Löschen nutzen.",
        "Eine Suche mit Tag-Filtern und Sortierung als Chip über dem Feld behalten; ein Klick holt sie zurück.",
        "Der Umschalter Liste oder Kacheln neben dem Typ-Filter wechselt zu großen Vorschaukacheln.",
        "Zwei Zeilen auswählen und in der Auswahlleiste \"Vergleichen\" klicken, um beide nebeneinander mit markierten Unterschieden zu sehen.",
      ],
      whatHappens: "Die Suche ist standardmäßig fehlertolerant und sortiert, falls nicht deaktiviert, nach am häufigsten oder zuletzt Verwendetem. Im Vergleich lässt sich der Text einer Seite in die andere übernehmen. Die Kachelansicht verhält sich bei Klicks, Menüs und Mehrfachauswahl genau wie die Liste.",
      settings: ["Fuzzy-Suche, Typ- und Farb-Filter, gespeicherte Suchen: Expertenmenü → Bibliothek & Kopierverlauf → Suche", "Suchfeld beim Öffnen fokussieren: Expertenmenü → Bibliothek & Kopierverlauf → Suche", "Einträge je Suchliste (Bibliothek, Alles durchsuchen, Schnellstarter): Expertenmenü → Bibliothek & Kopierverlauf → Suche", "Clevere Sortierung, Mehrfachauswahl für Stapelaktionen, Bild- und Video-Vorschau in den Zeilen, Kachelansicht: Expertenmenü → Bibliothek & Kopierverlauf → Anzeige"],
    },
    {
      section: "organising",
      title: "Alles durchsuchen",
      whatItIs: "Ein Suchfeld für Kacheln, Kopierverlauf, Anhänge, Ansichten, Einstellungen und diese Hilfe, alles auf einmal.",
      howTo: [
        "Auf die Lupe in der Werkzeugleiste klicken oder Strg+K drücken.",
        "Mindestens zwei Zeichen tippen; die Treffer erscheinen in Gruppen.",
        "Mit den Pfeiltasten wählen und Enter drücken, oder einen Treffer anklicken.",
      ],
      whatHappens:
        "Enter tut, was zum Treffer passt: Es kopiert eine Kachel oder einen Eintrag im Kopierverlauf, öffnet einen Anhang, wechselt in eine Ansicht, öffnet und markiert eine Einstellung oder öffnet den Hilfeartikel. Jede Gruppe zeigt einige Treffer und ein \"Mehr anzeigen\". Ist das Feld leer, werden die letzten Suchen angeboten. Escape, ein Klick daneben oder die Lupe schließt es wieder.",
      settings: ["Alles durchsuchen: Expertenmenü → Bibliothek & Kopierverlauf → Suche"],
    },
    {
      section: "organising",
      title: "Tags",
      whatItIs: "Tags sind Freitext-Etiketten, die Kacheln zugewiesen werden, um Bibliothek oder Raster zu filtern.",
      howTo: [
        "Editor einer Kachel öffnen und einen Tag-Namen eintippen, um ihn hinzuzufügen, oder einen vorhandenen wählen.",
        "Die Tag-Filterleiste in der Bibliothek nutzen, um die Liste auf ein oder mehrere Tags einzugrenzen.",
      ],
      whatHappens: "Umbenennen, Umfärben oder Löschen eines Tags im Tag-Manager wirkt überall dort, wo er genutzt wird, in einem Schritt.",
      settings: ["Tag-Filter-Modus (irgendeinen der ausgewählten Tags treffen oder alle): Expertenmenü → Bibliothek & Kopierverlauf → Tags"],
    },
    {
      section: "organising",
      title: "Archiv",
      whatItIs: "Das Archiv legt eine Kachel beiseite, ohne sie zu löschen.",
      howTo: [
        "Das Archiv im Expertenmenü einschalten.",
        "\"Ins Archiv\" im Menü einer Kachel wählen, oder mehrere auf einmal über die Auswahlleiste der Bibliothek archivieren.",
        "Mit dem Schalter über der Bibliotheksliste archivierte Kacheln wieder anzeigen.",
      ],
      whatHappens:
        "Eine archivierte Kachel verschwindet aus dem Raster und der Bibliotheksliste, die Suche findet sie aber weiterhin, und Exporte und Backups behalten sie wie jede andere Kachel. \"Aus dem Archiv\" im selben Menü holt sie zurück.",
      settings: ["Archiv: Expertenmenü → Bibliothek & Kopierverlauf → Anzeige"],
      tip: "Ab Werk aus.",
    },
    {
      section: "copying",
      title: "Kopierverlauf",
      whatItIs: "Der Kopierverlauf protokolliert, was wann kopiert wurde, in der App und, wenn eingeschaltet, in anderen Apps.",
      howTo: [
        "Bibliothek öffnen und den Reiter Kopierverlauf wählen.",
        "Suchen, nach Typ oder Farbe filtern, oder einen Favoriten aus der Liste markieren.",
        "Eine einzelne Zeile löschen oder den gesamten Kopierverlauf leeren.",
      ],
      whatHappens: "Die gruppierte Ansicht zeigt eine Zeile pro Kachel (ihre letzte Kopie) statt jedes einzelnen Ereignisses. Älterer Kopierverlauf als die Aufbewahrungs-Einstellung wird automatisch gelöscht (bei einer neuen Installation 30 Tage, einstellbar, 0 behält alles). Der Kopierverlauf ist eine Liste; das Sortierfeld wechselt zwischen Zuletzt kopiert und Meistgenutzt (die 20 häufigsten).",
      settings: ["Gruppierte Ansicht, Aufbewahrungsdauer: Expertenmenü → Bibliothek & Kopierverlauf → Kopierverlauf", "Einen Eintrag überspringen, der die Kopie davor wiederholt: Expertenmenü → Bibliothek & Kopierverlauf → Kopierverlauf"],
    },
    {
      section: "organising",
      title: "Medien",
      whatItIs: "Diese Einstellungen betreffen Kacheln, die ein Video oder Bild tragen.",
      howTo: [
        "Ein Video oder Bild an eine Kachel anhängen: Dateianhang, Screenshot, Einfügen oder Ziehen und Ablegen.",
        "Wiedergabe und privatsphärefreundliche Optiken im Expertenmenü anpassen.",
      ],
      whatHappens: "Video-Einstellungen (Autoplay, Stumm, Schleife, Lautstärke) gelten für jede Video-Kachel; Graustufen oder Abgedunkelt-bis-Hover ändert nur das Aussehen, nie den kopierten Inhalt. Bibliothek und Kopierverlauf zeigen außerdem die Länge eines Videos auf dem Vorschaubild sowie Auflösung und Dateigröße in der Kurzhilfe; das Hauptraster zeigt keins von beidem.",
      settings: ["Video-Steuerung, Autoplay und Schleife (ab Werk aus), Stumm: Expertenmenü → Kacheln → Medien", "Graustufen: Expertenmenü → Kacheln → Medien", "Akzentrahmen und abgerundete Ecken um Medien: Expertenmenü → Kacheln → Medien", "Dauer/Auflösung/Größe in Bibliothek und Kopierverlauf: Expertenmenü → Kacheln → Medien", "Medienangaben bei älteren Anhängen nachtragen: Expertenmenü → Kacheln → Medien"],
    },
    {
      section: "look",
      title: "Werkzeugleiste",
      whatItIs: "Die Werkzeugleiste bestimmt, welche Buttons der Kopfbereich und die Composer-Leiste zeigen.",
      howTo: ["Rechtsklick auf den Kopfbereich blendet seine Knöpfe ein oder aus; die Funktionen selbst schaltet das Expertenmenü.", "Obere Leiste ausblenden: Rechtsklick auf Zahnrad oder Kopfzeile."],
      whatHappens: "Das Ausblenden eines Buttons blendet ihn nur aus; die dahinterliegende Funktion funktioniert weiterhin über ihren eigenen Hotkey oder Menüeintrag.",
      settings: ["Bibliotheksknopf und Doppelklick auf die Leisten-Buttons: Expertenmenü → System & Daten → Werkzeugleiste; Logo, Titel und Leisten-Buttons: Rechtsklick auf die Kopfzeile.", "Schnelle Rastergrößen-Steuerung und sie erst beim Hovern zeigen, mehrere Ansichten: Expertenmenü → Funktionen → Ansichten und Schnellraster"],
    },
    {
      section: "system",
      title: "Datenschutz",
      whatItIs: "Diese Einstellungen halten den Inhalt dieses Fensters aus Screenshots, Screen-Shares und dem Blick von Passanten heraus.",
      howTo: [
        "Der Ausschluss von Bildschirmaufnahmen ist von Anfang an aktiv.",
        "Bei Bedarf Unschärfe für Kacheln oder Medien, versteckte Namen im Expertenmenü aktivieren.",
      ],
      whatHappens:
        "Der Ausschluss von Bildschirmaufnahmen macht das Fenster für Screenshot- und Aufnahmeprogramme unsichtbar, auch für die eigenen. Bleibt eine Aufnahme also leer, zuerst diese Einstellung prüfen. Eine optionale, standardmäßig ausgeschaltete Einstellung liest Text aus einem angehängten Bild, damit die Bibliothekssuche ihn auch findet; ein Treffer nur dort trägt die Kennzeichnung \"im Bild gefunden\", damit er nie wie ein zufälliger Treffer wirkt. Dieser erkannte Text bleibt sonst außerhalb der App: nirgends angezeigt, nie exportiert, nie synchronisiert, und in der Länge begrenzt. Wird das Bild entfernt oder ersetzt, ist der Text sofort weg, und das Ausschalten der Einstellung entfernt ihn aus jeder Kachel, auch im Papierkorb. Eine einzelne Kachel kann außerdem als vertraulich markiert werden; siehe \"Vertrauliche Kacheln\" weiter unten.",
      settings: ["Von Bildschirmaufnahmen ausschließen, Kacheln oder Medien unscharf machen, Namen verbergen: Expertenmenü → System & Daten → Datenschutz", "Text in angehängten Bildern für die Suche erfassen, Bildtext-Grenze: Expertenmenü → Bibliothek & Kopierverlauf → Suche", "Eine Kachel als vertraulich markieren, Sperr-Zeit des Tresors: Expertenmenü → System & Daten → Datenschutz"],
      tip: "Der Ausschluss von Bildschirmaufnahmen ist standardmäßig an: für normale Screenshots oder Aufnahmen dieses Fensters ausschalten.",
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
      title: "System",
      whatItIs: "System-Einstellungen betreffen app-weites Verhalten, das nicht eine einzelne Kachel betrifft.",
      howTo: ["Die System-Gruppe des Expertenmenüs für Update-Checks, Tastaturnavigation, mehrere Instanzen und den Daten-Ordner öffnen."],
      whatHappens:
        "Update-Checks sind standardmäßig aktiv und installieren nie etwas, ohne vorher zu fragen. Ein Wechsel des Daten-Ordners migriert die bestehende Datenbank dorthin. Mehrere Instanzen zuzulassen erfordert einen Neustart. Zeigt der Daten-Ordner auf einen Cloud-synchronisierten Ordner, erscheint einmal eine Warnung. Eine kleine Sperrdatei hält dann fest, welcher Rechner die Datenbank nutzt: Ein zweiter Rechner, der eine frische Sperre eines anderen findet, öffnet alles nur lesend und sagt das mit einem Banner; eine Sperre, die länger niemand erneuert hat, wird übernommen. Lange Aufgaben können das Taskleistensymbol als Fortschrittsbalken füllen und die Restzeit schätzen.",
      settings: ["Tastaturnavigation, mehrere Instanzen erlauben, Daten-Ordner, Sperre des Daten-Ordners, Fortschritt am Taskleistensymbol, Restzeit, Download der Video-Werkzeuge: Expertenmenü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Datenwartung",
      whatItIs: "Die Datenwartung bündelt Wartungswerkzeuge aus der Daten-Gruppe des Expertenmenüs.",
      howTo: ["Im Expertenmenü unter System & Daten die Gruppe Daten & Wartung öffnen.", "Den Papierkorb öffnen oder von dort importieren oder exportieren. Der Duplikat-Finder hat seinen eigenen Button „Duplikate finden“ in der Bibliothek."],
      whatHappens:
        "Eine gelöschte Kachel liegt zunächst im Papierkorb mit eigener Aufbewahrungsdauer, bevor sie endgültig verschwindet. Der Duplikat-Finder markiert nur textlich fast identische Kacheln; er löscht von sich aus nichts. \"Letzte Meldungen\" hält eine kurze Liste der Fehlermeldungen fest, die nur kurz als Hinweis erschienen, mit Zeit und Ort; die Liste lässt sich in ihrem Fenster kopieren oder leeren. Die Einstellungen allein lassen sich in derselben Gruppe in eine Datei speichern und wieder laden.",
      settings: ["Duplikat-Finder, Papierkorb-Aufbewahrung, Import und Export: Expertenmenü → System & Daten → Daten & Wartung", "Vor einem Zurücksetzen oder Alles-Löschen ein Backup anlegen: Expertenmenü → System & Daten → Daten & Wartung", "Letzte Meldungen und wie viel davon bleibt, Einstellungen exportieren und importieren, Rückgängig-Button: Expertenmenü → System & Daten → Daten & Wartung"],
    },
    {
      section: "system",
      title: "Eigenständig: Backup-Verwaltung",
      whatItIs: "Die Backup-Verwaltung erstellt verschlüsselte, komprimierte Snapshots der gesamten Bibliothek und Einstellungen.",
      howTo: [
        "Die Backup-Verwaltung in den Einstellungen öffnen.",
        "Automatische Backups aktivieren und ein Intervall wählen, oder manuell eines erstellen.",
        "Mit den zwei Knöpfen neben Backup erstellen ein Backup in eine Datei deiner Wahl speichern oder aus einer Datei importieren.",
        "Aus der Snapshot-Liste wiederherstellen und dabei nur Daten, nur Einstellungen oder beides wählen.",
      ],
      whatHappens:
        "Aufbewahrungsstufen (täglich, wöchentlich, monatlich, jährlich) dünnen ältere Snapshots automatisch aus, statt jeden für immer zu behalten. Ein Backup ist entweder vollständig oder gar nicht vorhanden; ein fehlgeschlagenes hinterlässt nichts. Dasselbe Fenster enthält außerdem eine Nutzungsstatistik (Gesamtzahlen, ein 14-Tage-Diagramm, meistgenutzte Kacheln, meistgenutzte Werkzeuge mit einer Liste der nie benutzten), unabhängig von den Backups selbst, dazu einen Speicherüberblick, was auf der Festplatte wie viel Platz belegt. Jeder Snapshot kann seinen Inhalt zeigen: Anzahlen, die jüngste Änderung darin, die Version, die ihn erstellt hat, und jede enthaltene Kachel, durchsuchbar und sortierbar, mit einem Button, der eine einzelne Kachel wiederherstellt. Die Statistik lässt sich als CSV, TXT oder JSON speichern. Je eine Zeile pro Art aufbewahrter Daten (Papierkorb, Kopierverlauf, Kopien aus anderen Apps, Aufnahmen, temporäre Exporte, Bearbeitungsstände) zeigt, wie viel sie enthält und wie lange sie bleibt, springt zu ihrer Einstellung und kann sie sofort aufräumen.",
      settings: [
        "Intervall, Aufbewahrungsstufen, Passwort, Wiederherstellungsumfang: Backup-Verwaltung (eigenes Fenster, geöffnet aus Einstellungen)",
        "Nutzungsstatistiken, wie viele Kacheln dort gelistet werden: selbes Fenster, unter den Backups",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Eigenständig: Schneller Launcher",
      whatItIs:
        "Der schnelle Launcher ist ein kleines, immer im Vordergrund liegendes Popup, das beim Tippen Name und Text jeder Kachel durchsucht: ein einfacher Text-Treffer, kein unscharfer Abgleich. Ein Treffer im Namen steht über einem Treffer im Text, aber ein Tippfehler findet nichts.",
      howTo: ["Strg+Alt+P, oder den konfigurierten Hotkey, von überall drücken.", "Tippen zum Suchen, dann Klick oder Enter auf einem Ergebnis zum Kopieren."],
      whatHappens: "Der Launcher schließt sich direkt nach dem Kopieren selbst, sodass er nie auf dem Bildschirm bleibt.",
      settings: [],
    },
    {
      section: "organising",
      title: "Eigenständig: Mehrere Ansichten",
      whatItIs:
        "Eine Ansicht ist eine Rasterseite mit eigener Größe und Anordnung.",
      howTo: [
        "Klick auf Plus neben den Ansichts-Tabs fügt eine Ansicht hinzu.",
        "Rechtsklick auf einen Tab zum Umbenennen, Größenändern, Umfärben oder Löschen.",
        "Einen Tab mit \"weiter nach links\" oder \"weiter nach rechts\" aus seinem Menü verschieben, oder ihn auf einen anderen Tab ziehen.",
      ],
      whatHappens: "Eine Kachel sitzt normalerweise in genau einer Ansicht (wird sie woanders abgelegt, wandert sie dorthin), außer \"Kachel in mehreren Ansichten\" ist aktiv, was ab Werk aus ist. Das Löschen einer Ansicht löscht nicht die dort platzierten Kacheln; eine auch anderswo gezeigte Kachel funktioniert dort weiter.",
      settings: ["Mehrere Ansichten, Kachel in mehreren Ansichten, höchste Zahl an Ansichten: Expertenmenü → Funktionen → Ansichten und Schnellraster"],
    },
    {
      section: "look",
      title: "Eigenständig: Theme-Editor",
      whatItIs: "Der Theme-Editor passt die Farben eines beliebigen integrierten Themes an und speichert das Ergebnis als neues Theme.",
      howTo: [
        "Den Theme-Editor öffnen: Expertenmenü → Aussehen → Visuell.",
        "Benutzerdefinierte Themes müssen an sein, was ab Werk der Fall ist.",
        "Ein Basis-Theme wählen, Farben anpassen und unter neuem Namen speichern.",
      ],
      whatHappens: "Ein benutzerdefiniertes Theme erscheint im regulären Theme-Dropdown neben den integrierten; sein Löschen betrifft nicht das Theme, auf dem es basierte.",
      settings: [],
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
      section: "copying",
      title: "Hotkeys",
      whatItIs:
        "Zwei unabhängige Hotkey-Arten: eine globale Kombination, die den schnellen Launcher öffnet, und eine Kombination pro Kachel.",
      howTo: [
        "Den globalen Hotkey in den Einstellungen festlegen (Standard: Strg+Alt+P).",
        "Einer Kachel im Editor einen eigenen Hotkey geben.",
        "Wählen, ob er nach dem Kopieren auch einfügen soll.",
        "\"Alle Tastenkürzel\" neben dem globalen Feld öffnet eine Liste: oben globale Hotkeys und Hotkeys der Kacheln, durchsuchbar, darunter die Tasten jedes Fensters.",
        "Jede Zeile wird dort direkt umbelegt oder gelöscht.",
        "Ist eine Kombination bereits vergeben, erklärt das Feld den Grund, statt sie stillschweigend zu akzeptieren.",
      ],
      whatHappens:
        "Eine Kombination muss Strg oder Alt enthalten; Shift allein oder eine bloße Taste wird abgelehnt, damit normales Tippen nie verschluckt wird. Der App-Hotkey gewinnt bei einem Konflikt immer; bei Kacheln behält die erste in der Liste eine geteilte Kombination. Der Import eines Backups mit kollidierendem Hotkey verwirft den verlierenden, statt den Import abzubrechen, und die Zusammenfassung nennt die Anzahl der verworfenen. Eine Kachel mit eigenem Hotkey zeigt ein kleines Tastatursymbol.",
      settings: [
        "Globaler Hotkey, alle Tastenkürzel: Einstellungen",
        "Eigene Hotkeys pro Kachel, Einfügen nach Kopieren: Expertenmenü → Funktionen → Kopieren",
        "Hotkey-Abzeichen, Deckkraft des Abzeichens: Expertenmenü → Kacheln → Kacheln",
      ],
    },
    {
      section: "system",
      title: "Papierkorb & Kacheln wiederherstellen",
      whatItIs: "Eine gelöschte Kachel wandert zuerst in den Papierkorb, nicht direkt in die endgültige Löschung.",
      howTo: [
        "Eine Kachel wie gewohnt löschen: sie landet im Papierkorb, Rückgängig holt sie zurück.",
        "Den Papierkorb aus der Daten-Gruppe des Expertenmenüs öffnen.",
        "Eine Kachel dort wiederherstellen oder sie durch die Aufbewahrungsdauer automatisch löschen lassen.",
      ],
      whatHappens:
        "Die Aufbewahrungsdauer wird in Tagen festgelegt; null bedeutet für immer behalten. Das Wiederherstellen platziert die Kachel in die erste freie Zelle der aktiven Ansicht, nicht zwingend die ursprüngliche Ansicht oder Zelle, und bringt weder schwebenden Button noch Ordner-Mitgliedschaft oder Hotkey von vorher zurück; die müssen neu eingerichtet werden.",
      settings: ["Papierkorb-Aufbewahrung: Expertenmenü → System & Daten → Daten & Wartung"],
    },
    {
      section: "system",
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
      section: "system",
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
      section: "system",
      title: "Import & Export",
      whatItIs: "Einfacher CSV-, TXT- oder JSON-Export deckt nur den Text ab; eine vollständige Backup-Datei deckt alles ab.",
      howTo: [
        "Aus der Daten-Gruppe des Expertenmenüs exportieren oder importieren.",
        "Ein Format aus der Auswahl neben dem Export-Button wählen; ein CSV- oder JSON-Import zeigt zuerst Name und Duplikat-Status je Eintrag.",
        "Ein vollständiges Backup stellt direkt wieder her, aus seinem eigenen Fenster.",
        "Zum tatsächlichen Import bestätigen, oder abbrechen, wodurch sich nichts ändert.",
        "\"Ordner importieren\" in den Einstellungen macht aus einem Ordner mit .md- oder .txt-Dateien Kacheln, eine pro Datei, nach einer Vorschau mit einem Häkchen pro Datei.",
      ],
      whatHappens: "CSV und TXT transportieren nur Text; JSON trägt zusätzlich die vollen Rotationstexte einer Kachel mit Rotation, Tags, Notiz, Tastenkürzel und Farbe, die in den anderen beiden nicht sauber Platz finden. Bilder, Dateien, Layout und Einstellungen benötigen weiterhin das vollständige Backup-Format. Der Import entscheidet anhand des Dateiinhalts zwischen CSV und JSON, nie anhand der Endung. Ein Ordner-Import liest nur: Nichts wird verschoben oder gelöscht. Der Dateiname wird zum Namen, jeder Unterordner zum Tag, und eine unlesbare Datei erscheint ausgegraut mit dem Grund.",
      settings: ["Import und Export: Expertenmenü → System & Daten → Daten & Wartung", "Tiefe und Dateigrenze des Ordner-Imports: Expertenmenü → System & Daten → Daten & Wartung"],
    },
    {
      section: "system",
      title: "Markdown-Ordner-Sync (Beta)",
      whatItIs:
        "Der Ordner-Sync spiegelt jede Text-Kachel als eigene lesbare Markdown-Datei in einem gewählten Ordner.",
      howTo: [
        "Im Expertenmenü einschalten und einen Ordner wählen.",
        "Eine Kachel normal bearbeiten, oder ihre Markdown-Datei direkt bearbeiten; beide Richtungen bleiben synchron.",
      ],
      whatHappens:
        "Anders als die Datenbank sind die Markdown-Dateien Klartext: wer Zugriff auf diesen Ordner hat, kann den Inhalt lesen; ein synchronisierter oder geteilter Ordner sollte deshalb für Sensibles vermieden werden. Eine Kachel mit angehängter Datei, Bild oder PDF wird als Verweis synchronisiert: Der Kopf trägt Pfad und PDF-Seite, nie die Datei selbst; der andere Rechner braucht also denselben Pfad, damit der Button dort funktioniert. Ein direkt eingefügter Screenshot hat gar keinen Pfad und bleibt nur in der Datenbank. Eine vertrauliche Kachel wird nie in den Ordner geschrieben, und eine Datei ändert nie eine vertrauliche Kachel. Eine Datei, deren Kachel im Papierkorb liegt, holt sie nicht als zweite Kachel zurück. Eine in einer eingehenden Datei genannte Ansicht oder ein Ordner, den es hier noch nicht gibt, wird automatisch angelegt, per Namensabgleich. Eine im Ordner gelöschte Datei löscht die Kachel nicht (sie wird im nächsten Durchlauf neu geschrieben); wer die Kachel in der App löscht, löscht auch ihre Datei. Der Dateiname ist die feste Kennung der Kachel, von Hand umbenennen erzeugt deshalb eine zweite Kachel statt einer umbenannten. Wurden Kachel und Datei gleichzeitig geändert, gewinnt in diesem Durchlauf die Datei, und die ersetzte Fassung bleibt als Version der Kachel erhalten; umgekehrt gewinnt die App die Datei, und deren Text bleibt als Konfliktkopie daneben liegen. Dateien, die diese App nicht geschrieben oder noch nie gesehen hat, werden nie geändert oder gelöscht, und eine Datei über 8 MB wird ganz übersprungen statt abgeschnitten. Ein Ordnerwechsel fragt, ob der alte Ordner gelöscht werden soll; gelöscht wird dabei nur, was diese App selbst geschrieben hat, nie ein Unterordner, und gar nichts, wenn etwas Fremdes darin liegt. Ein nicht erreichbarer Ordner pausiert die Synchronisierung nach einigen Fehlversuchen, statt endlos weiterzuversuchen.",
      settings: ["Ordner-Sync, Synchronisierungsordner, Abfragetakt, Pause nach Fehlversuchen: Expertenmenü → Funktionen → Prompt-Synchronisierung"],
      tip: "Dies ist eine Beta-Funktion: den Synchronisierungsordner nur dort ablegen, wo niemand Unbefugtes mitliest.",
    },
    {
      section: "basics",
      title: "Screenshot-Tool",
      whatItIs: "Das Screenshot-Tool erfasst einen Teil oder den gesamten Bildschirm direkt in eine neue Kachel.",
      howTo: [
        "Auf den Knopf „Aufnehmen“ links vom Eingabefeld klicken und „Screenshot“ wählen.",
        "Einen Bereich ziehen oder ein Fenster anklicken, um es ganz zu erfassen, auch geschützte.",
        "Im Bestätigungsdialog als Kachel speichern, als Datei speichern, oder einfach in der Zwischenablage belassen.",
      ],
      whatHappens:
        "Die eigenen Fenster der App werden für den Moment der Aufnahme ausgeblendet, sodass sie nie im Bild landen. Das Ergebnis erreicht sofort die Zwischenablage; das Speichern als Kachel oder Datei ist ein separater, optionaler Schritt. Beim Speichern im Screenshots-Ordner wird nie eine vorhandene Datei ersetzt; die Aufnahme bekommt den nächsten freien Namen.",
      settings: ["Screenshot-Tool, Speicherort, Format und Qualität, Vorschaugröße, Aufnahmeverzögerung: Expertenmenü → Aufnehmen → Screenshot", "Screenshots immer speichern: Einstellungen"],
    },
    {
      section: "basics",
      title: "Screenshot-Editor",
      id: "screenshotEditor",
      whatItIs: "Öffnet sich direkt nach einer Aufnahme, bevor der Screenshot gespeichert wird: zum Zuschneiden, Markieren und Beschriften, bevor daraus eine Kachel wird.",
      howTo: [
        "Einen Screenshot aufnehmen; der Editor öffnet sich automatisch über der Aufnahme.",
        "Ein Werkzeug aus der Symbolleiste wählen: Zuschneiden, Stift, Marker, Linie, Pfeil, Rechteck, Ellipse, Text, Schrittnummer, Verpixeln, Fokusbereich oder Hand.",
        "Rechtsklick auf ein Rechteck, eine Ellipse, Linie, einen Pfeil oder eine verpixelte Fläche, um es zu duplizieren oder zu löschen; ein Fokusbereich lässt sich nur löschen.",
        "Pfeiltasten verschieben die gewählte Form, Strg+Pfeil ändert ihre Größe, Umschalt macht größere Schritte, Escape hebt die Auswahl auf.",
        "\"Größe ändern\" ändert die echte Pixelgröße des Bildes; \"Original zeigen\" (oder V) zeigt die unbearbeitete Aufnahme, solange gedrückt, ein Tippen hält sie fest.",
        "Mit Rückgängig/Wiederholen schrittweise durch die Bearbeitungen gehen; anschließend speichern.",
      ],
      whatHappens:
        "Jede Bearbeitung ist ein Schritt in einer Rückgängig/Wiederholen-Liste, deren Höchstzahl separat einstellbar ist. Beim Verpixeln wird immer der ursprüngliche Screenshot verwendet, nie eine bereits bearbeitete Ebene; dadurch werden frühere Markierungen beim Unkenntlichmachen einer Stelle nicht verschmiert. Das Einrasten an Kanten richtet eine Form beim Ziehen am Bildrand, an der Mitte und an den anderen Formen aus, mit einer dünnen Hilfslinie; mit gehaltener Alt-Taste wird frei gezogen. Eine Textbeschriftung kann eine Hintergrundfläche oder einen Umriss bekommen, damit heller Text auch auf hellem Grund lesbar bleibt. Rückgängig deckt jeden Schritt ab, auch Duplizieren, Löschen, Größe ändern und eine Folge von Pfeiltasten-Schritten, die als ein Schritt zählt. Beim Hineinzoomen verschiebt das Hand-Werkzeug (oder gehaltene Leertaste bzw. die mittlere Maustaste) die Ansicht für pixelgenaues Arbeiten, und das Zuschneide-Werkzeug zeigt an jedem Anfasser einen passenden Richtungspfeil sowie innerhalb des Ausschnitts einen Verschieben-Zeiger. Ist der Editor unten deaktiviert, erscheint stattdessen direkt der Bestätigungsdialog.",
      settings: ["Screenshot-Editor, Standardwerkzeug, Einrasten an Kanten, Hintergrund für Text, Standardfarbe, Rückgängig-Limit: Expertenmenü → Aufnehmen → Screenshot-Editor. Der Editor merkt sich die zuletzt benutzte Größe jedes Werkzeugs.", "Mausrad in Zahlenfeldern: Expertenmenü → Aufnehmen → Beide Editoren"],
    },
    {
      section: "basics",
      title: "Datei im Editor öffnen",
      whatItIs: "Öffnet ein vorhandenes Bild oder Video in dem Editor, den sein Inhalt verlangt, statt einen Editor nur über eine frische Aufnahme zu erreichen.",
      howTo: [
        "Auf den Knopf „Aufnehmen“ links vom Eingabefeld klicken, \"Datei im Editor öffnen\" wählen und die Datei aussuchen.",
        "Oder per Rechtsklick auf eine Bild- oder Videokachel bzw. Bibliothekszeile Bild bearbeiten oder Video bearbeiten wählen.",
        "Wie gewohnt bearbeiten; beim Speichern wird nach dem Ziel gefragt.",
      ],
      whatHappens:
        "Der Inhalt der Datei entscheidet, welcher Editor sich öffnet, nicht ihre Endung: ein Video mit einer Bild-Endung öffnet trotzdem den Video-Editor. Der Editor arbeitet immer auf einer Kopie, das Original wird also nie verändert; der Speichern-Dialog startet im Ordner der Ausgangsdatei. Eine Datei über der eingestellten Größe wird abgelehnt, bevor sie geladen wird.",
      settings: ["Größte zu öffnende Datei: Expertenmenü → Aufnehmen → Beide Editoren"],
    },
    {
      section: "basics",
      title: "Bildschirmaufnahme",
      id: "screenRecording",
      whatItIs: "Nimmt ein Fenster, einen Monitor oder einen gezogenen Bereich als Video auf, neben dem Screenshot-Werkzeug.",
      howTo: [
        "Auf den Knopf „Aufnehmen“ links vom Eingabefeld klicken und \"Bildschirm aufnehmen\" {icon:record} wählen; ein Fenster anklicken, einen Bereich ziehen oder auf den freien Desktop klicken für den ganzen Bildschirm. Das ist dieselbe Übersicht wie beim Screenshot-Werkzeug.",
        "Ein Countdown zählt optional herunter, bevor die Aufnahme wirklich startet.",
        "Eine kleine Bedienleiste zeigt Laufzeit und Dateigröße, zwei Ton-Stummschalt-Buttons und bietet Pause/Fortsetzen, Stopp und Verwerfen; zwei optionale globale Hotkeys sowie das Tray-Menü können sie ebenfalls stoppen oder pausieren/fortsetzen, aber nur, solange die Aufnahme wirklich läuft. Eine Fensteraufnahme mit der Leinwand für alle Bildschirme oder den aktuellen Bildschirm bekommt zusätzlich einen Button zum Wechseln des Aufnahmeziels, ohne von vorn zu beginnen.",
        "Stopp bietet Speichern, Speichern unter…, Kopieren, Bearbeiten oder Verwerfen an.",
      ],
      whatHappens:
        "Aufgenommen wird immer MP4 (H.264); drei Qualitätsstufen (Klein/Hoch/Verlustfrei) tauschen Dateigröße gegen Bildschärfe, ein Format-Feld kann das Ergebnis danach in WebM, AVI, MOV, MKV, animiertes WebP oder APNG umwandeln (die letzten drei ohne Ton und auf 30 Sekunden begrenzt). Alles außer reinem MP4 sowie die Stufe Verlustfrei braucht ffmpeg (ein separater Download, direkt angeboten, wenn es fehlt); eine fehlgeschlagene oder abgebrochene Umwandlung fällt immer auf die unangetastete Originalaufnahme zurück. Systemton und Mikrofon werden beide standardmäßig aufgenommen, je als eigene Spur: die Mikrofonspur wird zwar aufgezeichnet, aber erst hörbar, wenn man sie später selbst im Video-Editor dazumischt; zieht man das Mikrofon während der Aufnahme ab, laufen Bild und Systemton weiter, nur diese Spur gilt als verloren. Die beiden Ton-Knöpfe in der Bedienleiste schalten Systemton oder Mikrofon sofort stumm, ohne die Spur zu beenden: eine stumme Quelle nimmt weiter Stille auf und lässt sich in derselben Aufnahme wieder zuschalten. Speichern und Kopieren teilen sich einen Aufnahme-Ordner; Bearbeiten öffnet den Clip im Video-Editor, ohne über sein Schicksal zu entscheiden; schließt man den Dialog danach, ohne gespeichert oder kopiert zu haben, wird die Datei trotzdem verworfen. Zieht man die Bedienleiste selbst (nicht ihre Knöpfe), folgt sie für den Rest dieser einen Aufnahme nicht mehr dem Aufnahmefenster, sondern bleibt frei beweglich stehen, auch auf einem anderen Monitor; die nächste Aufnahme folgt wieder von vorn, und die Leiste ist ansonsten nie Teil der Aufnahme. Schließt sich das aufgenommene Fenster von selbst, stoppt die Aufnahme automatisch und behält, was bis dahin aufgenommen wurde. Eine Aufnahme, die nie ordnungsgemäß gestoppt wurde (Absturz, erzwungenes Beenden), wird automatisch entfernt, sobald sie eine Schonfrist überschreitet; eine gerade gestoppte Aufnahme, die noch abgeschlossen wird, bleibt dabei unangetastet. Die Aufnahme-Ordner-Zeile im Expertenmenü zeigt Anzahl und Größe der enthaltenen Videos, mit einem „Aufräumen…“-Button, der Dateien älter als 7, 30 oder 90 Tage samt der frei werdenden Größe zur Bestätigung auflistet, bevor irgendetwas gelöscht wird. Eine optionale Tastenanzeige zeichnet auf, welche Tasten während einer Aufnahme gedrückt werden, nur als Tastenkennungen mit Zeitstempeln, nie als Textinhalt; sie zeigt ab Werk jede Taste, auch getippten Text, und lässt ein Passwortfeld aus, wenn Windows es als solches meldet, ohne dass das garantiert ist. Ein Schalter „Nur Tastenkürzel“ beschränkt die Aufzeichnung auf Kombinationen mit Strg, Alt oder Win sowie Enter, Tab, Esc, Rücktaste, Entf, Einfg, Pfeile, Pos1, Ende, Bild auf, Bild ab und F-Tasten, für Aufnahmen mit vertraulichen Eingaben. Derselbe Aufräum-Dialog hat ein Häkchen pro Datei: \"Auswahl exportieren\" wandelt alle angehakten Aufnahmen in einem Rutsch in Format, Qualität und Ordner der Wahl um und meldet, wie viele fertig wurden. Eine neue Installation löscht nicht verwendete Aufnahmen nach 90 Tagen (Expertenmenü, 0 = für immer).",
      settings: ["Bildschirmaufnahme, Countdown, Bitrate, Qualität, Format, Aufnahme-Ordner: Expertenmenü → Aufnehmen → Bildschirmaufnahme", "Mauszeiger, Tonquellen und -gerät, Stopp-/Pause-Hotkeys, Tray-Einträge, defekte Aufnahmen automatisch entfernen, Tastenanzeige, Rahmen um einen aufgenommenen Bereich und seine Farbe: Expertenmenü → Aufnehmen → Während der Aufnahme"],
    },
    {
      section: "basics",
      title: "Kameraaufnahme",
      whatItIs: "Nimmt die Webcam in ein eigenes Video auf, allein oder neben einer Bildschirmaufnahme.",
      howTo: [
        "Die Kameraaufnahme im Expertenmenü einschalten.",
        "Auf den Knopf „Aufnehmen“ links vom Eingabefeld klicken und \"Kamera aufnehmen\" {icon:record} wählen.",
        "Im eigenen Fenster die Vorschau prüfen, Tonquelle, Auflösung und Ordner wählen, dann Start.",
      ],
      whatHappens:
        "Das Kamerafenster oder sein Statuspunkt in der Aufnahmeleiste bleibt die ganze Zeit sichtbar, die Kamera läuft also nie unbemerkt. Die Kamera landet in einer eigenen Datei und wird nie in eine Bildschirmaufnahme eingebrannt. Während einer Bildschirmaufnahme gestartet, teilen beide einen Startpunkt, und der Video-Editor öffnet dann die Bildschirmaufnahme mit der Kamera als verschiebbarer Überlagerung darüber.",
      settings: ["Kameraaufnahme, Standardkamera, Auflösung, Bildrate: Expertenmenü → Aufnehmen → Kameraaufnahme"],
      tip: "Ab Werk aus.",
    },
    {
      section: "basics",
      title: "Video-Editor",
      whatItIs: "Ein kleiner Editor für eine fertige Aufnahme oder ein angehängtes Video, geöffnet aus dem Ergebnisdialog einer Aufnahme oder über das Rechtsklick-Menü eines Video-Prompts.",
      howTo: [
        "Bei einer frischen Aufnahme: im Ergebnisdialog auf Mehr, dann auf Video-Editor klicken.",
        "Bei einem bereits gespeicherten Video-Prompt: rechtsklicken und „Video bearbeiten“ wählen.",
        "Zuschneiden, Kürzen, Geschwindigkeit ändern, Ein-/Ausblenden, Größe ändern (eingegebene Maße, ein Prozentwert oder eine Schnellauswahl) sowie Bild-, Standbild-, Text-, Verpixelungs- oder Fokusbereich-Overlays auf der Zeitleiste hinzufügen.",
        "Exportieren: bei reinem Kürzen greift ein verlustfreier Schnellpfad, sonst wird der ganze Clip einmal neu kodiert.",
      ],
      whatHappens:
        "Bis zum Export wird nichts verarbeitet: Die Live-Vorschau spielt die Bearbeitungen direkt ab. Filmstreifen und Wellenform geben eine zoombare Übersicht über den ganzen Clip für bildgenaue Navigation. Die Vorschau selbst zoomt außerdem mit dem Mausrad (bis 8×) und lässt sich durch Ziehen verschieben (mit der mittleren Maustaste immer, mit der linken, sobald hineingezoomt ist), um ein kleines Overlay bei einer großen Aufnahme genau zu platzieren. Das Exportziel ist ein Eingabefeld: den Pfad direkt eintippen, oder „Exportieren nach…“, um nur den Ordner zu wechseln und den Dateinamen zu behalten; nach einem fertigen Export verschiebt „Speichern unter…“ das fertige Video an einen anderen Ort. Ein Formatwechsel ändert nur die Endung. „Exportieren und kopieren“ verzichtet ganz auf einen Namen und exportiert in eine selbst aufräumende temporäre Kopie (nach einer Stunde entfernt, und erneut beim nächsten Start), die direkt in der Zwischenablage landet. Ein paar fertige Export-Voreinstellungen (unveränderte Qualität, kleiner für Chat, animiertes GIF) merken sich die letzte Wahl. Neben MP4, WebM und GIF bietet die Formatauswahl beim Export auch AVI, MOV, MKV, animiertes WebP und APNG, jeweils mit einem kurzen Hinweis, wofür es gedacht ist; GIF, WebP und APNG haben nie Ton, warnen ab zehn Sekunden Länge und sind auf eine Höchstauflösung begrenzt, da alle drei schnell groß werden. Rückgängig/Wiederholen deckt jeden Bearbeitungsschritt ab, begrenzt durch ein einstellbares Limit. \"Video hinzufügen\" legt einen weiteren Clip als Überlagerung darüber, und die Überlagerung \\\"Tasten\\\" macht aus den während einer Aufnahme gedrückten Tasten ein Tastaturbild. Rechtsklick auf die Zeile einer Überlagerung dupliziert oder löscht sie. Ein Zuschnitt oder eine Überlagerung rastet beim Ziehen an den Bildkanten, an der Mitte und an anderen Überlagerungen ein; mit gehaltener Alt-Taste wird frei gezogen. Eine neue Überlagerung startet mit der zuletzt für ihre Art benutzten Farbe, Stärke und Größe. Unfertige Schnitte und Überlagerungen werden gemerkt, sodass das erneute Öffnen derselben Aufnahme das Weitermachen anbietet. Der Name der Exportdatei hat ein eigenes Feld; eine vorhandene Datei wird nie ersetzt, der Export nimmt den nächsten freien Namen.",
      settings: ["Video-Editor, Rückgängig-Limit, Überlagerungswerte merken, Einrasten an Kanten: Expertenmenü → Aufnehmen → Video-Editor", "Unfertige Video-Bearbeitungen merken, und wie viele Tage: Expertenmenü → Aufnehmen → Beide Editoren"],
      sketches: ["editorTimeline"],
    },
    {
      section: "basics",
      title: "Texterkennung (OCR)",
      id: "ocr",
      whatItIs: "Liest mit der geräteeigenen Erkennung von Windows Text aus einem Screenshot oder einer Bilddatei. Das läuft vollständig offline und nichts verlässt das Gerät.",
      howTo: [
        "Über die Eingabeleiste: der Knopf „Aufnehmen“ links vom Eingabefeld {icon:ocr} bietet Text aus einem Screenshot oder aus einer vorhandenen Bilddatei an.",
        "Bei einem bereits aufgenommenen Screenshot: über den Button „Text erkennen“ im Aufnahme-Dialog.",
        "Das Ergebnis prüfen und anschließend kopieren oder direkt einfügen.",
      ],
      whatHappens:
        "Die Erkennung läuft vollständig auf dem Gerät über die eigene Erkennungs-Engine von Windows. Der Ergebnisdialog kann den Text automatisch kopieren und getrennte Zeilen wieder zu Absätzen verbinden; beides lässt sich separat ein- und ausschalten. Fehlt ein Sprachpaket, öffnet sich derselbe Dialog wie bei der Spracheingabe; siehe „Sprachpakete“ unten.",
      settings: ["Texterkennung, Ergebnis automatisch kopieren, Zeilen verbinden, kleine Bilder vergrößern, Erkennungssprache: Expertenmenü → Aufnehmen → Texterkennung"],
    },
    {
      section: "basics",
      title: "Spracheingabe",
      id: "voiceDictation",
      whatItIs: "Der Mikrofon-Button {icon:mic} verwandelt Sprache in Text. Die Erkennung läuft komplett auf dem Gerät.",
      howTo: [
        "Auf das Mikrofon-Icon neben der Eingabeleiste klicken, oder im Editor einer Kachel.",
        "Sprechen; der Text erscheint, während er erkannt wird.",
        "Erneut klicken, Escape drücken, oder speichern/schließen, um zu stoppen.",
      ],
      whatHappens:
        "Die Erkennung läuft lokal über die eigene Spracherkennung von Windows: es verlässt nichts das Gerät, außer die Online-Erkennung wird ausdrücklich erlaubt. Gesprochene Satzzeichen-Wörter (\"Komma\", \"neue Zeile\", …) werden zu echten Satzzeichen; \"wörtlich\" vor einem Wort tippt es statt es als Befehl auszulösen. Ein fehlendes Sprachpaket öffnet direkt den Sprachpaket-Dialog, gemeinsam mit der Texterkennung (OCR). Ein optionaler globaler Kurzbefehl startet und stoppt eine Aufnahme, ohne das Fenster zu öffnen. Der Text landet in der Zwischenablage, wird eingefügt oder geht direkt in die Eingabeleiste; das Taskleisten-Symbol zeigt währenddessen \"Nimmt auf …\" im Tooltip.",
      settings: ["Spracheingabe, Satzzeichen-Befehle, automatische Großschreibung, Stille-Auto-Stopp, Einfügeposition, globaler Hotkey: Expertenmenü → Funktionen → Diktat"],
    },
    {
      section: "basics",
      title: "Sprachpakete",
      whatItIs: "Texterkennung und Spracheingabe verwenden dieselben Windows-Sprachpakete und denselben Installationsdialog.",
      howTo: [
        "Eine Sprache im Dropdown für die Erkennungssprache auswählen; fehlt das Paket, öffnet sich der Installationsdialog automatisch.",
        "Zum Installieren eines Pakets sind Administratorrechte erforderlich: Windows fragt einmal über den eigenen Bestätigungsdialog danach.",
        "„Sprachen der Texterkennung verwalten…“ (bei der Spracheingabe „Diktatsprachen verwalten…“) am unteren Rand des Sprach-Dropdowns öffnet denselben Dialog jederzeit erneut.",
      ],
      whatHappens:
        "Bei manchen Windows-Versionen sind selbst zum Auflisten der installierten Pakete Administratorrechte erforderlich. In diesem Fall führt ein Button die Prüfung über dieselbe Administratorabfrage erneut aus. Eine Hintergrundüberwachung erkennt, wenn sich die Version eines installierten Pakets geändert hat oder ein Windows-Update es unbemerkt entfernt hat, und kennzeichnet es, ohne sofort zur Neuinstallation aufzufordern. Mit „Später erinnern“ lässt sich der Hinweis vorübergehend ausblenden; die Überwachung kann auch vollständig deaktiviert werden.",
      settings: ["Erkennungssprache automatisch wählen, Hintergrundüberwachung für fehlende oder geänderte Pakete: Expertenmenü → Aufnehmen → Texterkennung, und noch einmal unter Expertenmenü → Funktionen → Diktat"],
    },
    {
      section: "basics",
      title: "Farbpipette & Auswahlverlauf",
      id: "colorPicker",
      whatItIs: "Die Farbpipette liest jeden Bildschirmpixel als Hex, RGB oder HSL aus.",
      howTo: [
        "Auf den Knopf „Aufnehmen“ links vom Eingabefeld klicken und \"Farbe vom Bildschirm aufnehmen\" wählen.",
        "Die Lupe zum gewünschten Pixel bewegen; scrollen oder Plus/Minus zum Zoomen nutzen.",
        "Klick kopiert die Farbe; im selben Menü öffnet \"Letzte Farben\" die letzten paar Auswahlen erneut.",
      ],
      whatHappens:
        "Es wird nie automatisch etwas in einer Kachel gespeichert; eine Auswahl landet nur in der Zwischenablage. Der Auswahlverlauf hält die letzten paar Farben und übersteht einen Neustart, begrenzt durch eine eigene Einstellung.",
      settings: ["Farbpipette, Zoom, Format, Länge des Auswahlverlaufs: Expertenmenü → Aufnehmen → Farbpipette"],
      tip: "Farbformat (Hex, RGB oder HSL) im Hinweis wählen, der direkt nach der Auswahl erscheint.",
    },
    {
      section: "basics",
      title: "Rotation",
      whatItIs: "Eine Kachel kann eine Liste von Texten statt nur einem tragen; jede Kopie liefert den nächsten Eintrag.",
      howTo: [
        "Die Rotation ist standardmäßig an; die Funktion im Expertenmenü ausschalten, falls nie genutzt.",
        "Im Editor einer Kachel \"Rotation\" über dem Textbereich einschalten.",
        "Die Liste der Rotationstexte ersetzt dann das normale Textfeld.",
        "Mit \"+ Rotationstext\" weitere Rotationstexte hinzufügen; Icons daneben sortieren um oder entfernen sie.",
        "Reihenfolge (der Reihe nach) oder Mischen (zufällig, ohne Wiederholung) wählen.",
        "Die Kachel wie gewohnt kopieren: sie liefert immer den nächsten Eintrag, ohne zu fragen, welchen.",
      ],
      whatHappens:
        "Ein kleines Badge zählt auf einer Raster-Kachel, einem schwebenden Button und einem Mitglied eines Button-Ordners hoch, wie \"2/5\"; ein Kachelordner-Mitglied zeigt nur sein Dateityp-Icon, nicht dieses Badge. Im Reihenfolge-Modus ist das die absolute Position; im Zufalls-Modus zählt es die Ziehungen im aktuellen Durchlauf und beginnt wieder bei 1, sobald jeder Eintrag einmal drankam, statt auf eine beliebige Position zu springen. Der Zählerstand übersteht Neustarts und vollständige Backups; ein CSV/TXT-Import erstellt eine neue Kachel mit frischem Zeiger, sodass nur die Textliste und ihr Reihenfolge- oder Misch-Modus übernommen werden, nicht die genaue Position. Wird die Rotation bei einer Kachel mit mehreren Einträgen ausgeschaltet, bleiben alle erhalten; nur der erste wird genutzt, bis sie wieder aktiviert wird. Das optionale \"Beim Einfügen rotieren\" bringt eine Kachel mit Rotation bei jedem echten Einfügen zum nächsten Text, nicht nur beim Kopieren.",
      settings: ["Rotation (standardmäßig an), Standard Reihenfolge oder Mischen, Badge, maximale Rotationstexte, beim Einfügen rotieren: Expertenmenü → Funktionen → Rotation. Der Schalter pro Kachel sitzt im Editor über dem Eintragsbereich."],
    },
    {
      section: "system",
      title: "Das Expertenmenü",
      whatItIs: "Das Expertenmenü hält jede optionale Funktion hinter einem eigenen Schalter mit ihren Parametern.",
      howTo: [
        "Einstellungen, dann das Expertenmenü öffnen.",
        "Einen Tab wählen, oder im Suchfeld tippen, um alle Tabs auf einmal zu durchsuchen.",
        "\"Nur geänderte\" einschalten, um nur das zu sehen, was an den Standardwerten angepasst wurde.",
      ],
      whatHappens:
        "Eine standardmäßig deaktivierte Funktion trägt ein kleines Punkt-Badge an ihrer Beschriftung, mit einem erklärenden Tooltip. Zurücksetzen funktioniert auf drei Ebenen: das eigene Rückgängig-Icon eines Werts, das Zurücksetzen-Icon einer ganzen Gruppe in ihrer Kopfzeile, oder alles auf einmal über die allgemeine Zurücksetzen-Aktion. Ein Profil zeigt zuerst, wie viele Einstellungen sich ändern würden, und bleibt nie als Modus gemerkt; Alles an lässt alles Zerstörerische und alles, was Daten nach außen schickt, aus. Die Suche findet auch Einstellungen außerhalb des Expertenmenüs, etwa im Einstellungsfenster oder im Fenster der Backups, mit einem Hinweis, wo sie liegen; ein Klick öffnet diese Stelle.",
      settings: ["Tipp-Zeile, hervorgehobene Suchtreffer und Skizzen in diesem Hilfefenster: Expertenmenü → System & Daten → Hilfe"],
    },
    {
      section: "system",
      title: "Tastaturnavigation",
      whatItIs: "Pfeiltasten bewegen einen Fokusring über das Raster für rein tastaturbasiertes Kopieren.",
      howTo: [
        "Die Tastaturnavigation ist ab Werk an; abschaltbar in Expertenmenü → System & Daten.",
        "Die Pfeiltasten bewegen den Fokusring, Enter oder Leertaste kopiert.",
        "Tab springt wie gewohnt weiterhin durch Buttons und Felder überall sonst in der App.",
      ],
      whatHappens: "Der Fokusring blendet sich kurz nach Fokusverlust des Fensters aus, damit er nicht auf einer Kachel verweilt, sobald eine andere App genutzt wird.",
      settings: ["Tastaturnavigation, Ausblend-Verzögerung: Expertenmenü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Wo die Daten liegen",
      whatItIs:
        "Alles liegt in einer einzigen lokalen SQLite-Datenbank plus ihrer kleinen Begleitdatei.",
      howTo: [
        "Installiert: der Daten-Ordner liegt unter AppData, im Expertenmenü einsehbar und änderbar.",
        "Portabel: der Daten-Ordner liegt direkt neben der exe, sodass alles auf einen USB-Stick passt.",
      ],
      whatHappens: "Ein Wechsel von portabel zu installiert, oder zurück, wird automatisch erkannt und als einmalige Migration angeboten: bestehende Daten werden nie stillschweigend zurückgelassen.",
      settings: ["Daten-Ordner: Expertenmenü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Fernzugriff & Links",
      whatItIs: "Zwei optionale Wege, über die andere Programme die App erreichen: Links, die eine Kachel öffnen oder kopieren, und ein passwortgeschützter Empfänger für Befehle.",
      howTo: [
        "promptsaver-Links im Expertenmenü einschalten, damit ein Link eine Kachel öffnen oder kopieren kann.",
        "Den Fernzugriff einschalten, damit ein anderes Programm auf diesem Rechner Befehle senden kann; das einmal angezeigte Passwort kopieren.",
        "Für andere Geräte \"Aus dem lokalen Netzwerk zulassen\" einschalten und die erlaubten Adressbereiche eintragen.",
        "\"Zertifikat…\" öffnen, um den Fingerabdruck zu sehen, das Zertifikat für das andere Gerät zu speichern oder ein neues zu erzeugen.",
        "Für iPhone, iPad oder Mac \"Kurzes Zertifikat für Apple-Geräte (825 Tage)\" einschalten; das Umschalten fragt zuerst nach und erzeugt dann ein neues Zertifikat.",
      ],
      whatHappens:
        "Beides ist ab Werk aus. Ein Link kann nur öffnen oder kopieren, nie ändern oder löschen, und das erste Öffnen und das erste Kopieren fragen je einmal nach. Der Empfänger startet nie ohne Passwort, und die App speichert es nur in einer Form, aus der es nicht wieder lesbar wird; zehn falsche Versuche sperren den Aufrufer zehn Minuten. Anfragen sind in Größe und Anzahl begrenzt und werden ohne das Passwort protokolliert. Solange er läuft, bleibt ein Banner mit Port und Ausschalter sichtbar. Ohne den Netzwerk-Schalter lauscht er nur auf diesem Rechner; mit ihm ist die Verbindung verschlüsselt, und jedes Gerät muss dem gespeicherten Zertifikat vertrauen. Ein neues Zertifikat ersetzt das alte sofort, jedes Gerät braucht dann die neue Datei; nach einer geänderten Netzwerkadresse dieses Rechners ein neues erzeugen. Das Zurücksetzen der Einstellungen schaltet Fernzugriff und Links aus. Das Zertifikat gilt 10 Jahre, mit der kurzen Einstellung 825 Tage, was Apple-Geräte verlangen; Geräte brauchen dann etwa alle zwei Jahre das neue Zertifikat. Ein abgelaufenes Zertifikat wird beim Start des Empfängers erneuert, mit einer Meldung.",
      settings: ["Fernzugriff, Netzwerkzugriff, Adressbereiche, Port, Anfragen pro Minute, Passwort, Zertifikat, Links: Expertenmenü → System & Daten → Fernzugriff"],
      tip: "Jeder in den erlaubten Bereichen, der das Passwort kennt, kann Kacheln lesen und ändern: die Bereiche eng halten.",
    },
    {
      section: "system",
      title: "Befehlszeile",
      whatItIs: "Die Programmdatei funktioniert auch als Kommandozeilen-Werkzeug, für Skripte und schnelle Aufgaben, ohne das Fenster zu öffnen.",
      howTo: [
        "Eine Eingabeaufforderung öffnen und die Programmdatei mit --help starten, um jeden Befehl zu sehen.",
        "Kacheln auflisten, lesen, kopieren, anlegen, ändern, löschen und wiederherstellen, Tags und Ansichten verwalten, suchen, exportieren, importieren und Backups anlegen oder wiederherstellen.",
        "--json für maschinenlesbare Ausgabe anhängen, und --yes an jeden Befehl, der etwas ändert.",
      ],
      whatHappens:
        "Läuft die App bereits, geht der Befehl an sie und ihre Antwort wird ausgegeben; sonst läuft der Befehl allein, ohne Fenster. Es greift immer nur eine Kopie auf die Daten zu. Ohne --yes sagt ein ändernder Befehl nur, was er täte. Ordnerorte, der Tresor, das Backup-Passwort und jede Fernzugriff-Einstellung, das Aufnehmen von Kopien aus anderen Apps und der Link-Schalter lassen sich hier nicht ändern, nur in der App.",
      settings: [],
    },
    {
      section: "system",
      title: "Vorführung",
      whatItIs: "Die Vorführung schaltet das ganze Fenster auf eine Beispiel-Bibliothek um, damit eine Vorführung oder ein Screenshot nie etwas Echtes zeigt.",
      howTo: [
        "Die Vorführung im Expertenmenü einschalten; die App startet neu in die Beispiel-Bibliothek.",
        "Alles zeigen oder ausprobieren; eine Markierung bleibt die ganze Zeit sichtbar.",
        "Zum Zurückkehren ausschalten; die App fragt zuerst nach und startet neu in die eigenen Daten.",
      ],
      whatHappens:
        "Die Beispiel-Bibliothek wird jedes Mal frisch erzeugt und deckt die meisten Funktionen ab: Kacheln mit und ohne Bild, eine Kachel mit Rotation, einen Platzhalter, Tags, einen Favoriten, zwei Ansichten, einen Ordner, einen Anhang und ein Video. Die eigenen Daten werden währenddessen nie geöffnet oder geändert. Was während der Vorführung geändert wurde, wird beim Beenden verworfen.",
      settings: ["Vorführung: Expertenmenü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Updates & Sicherheit",
      whatItIs: "Eine optionale Prüfung sucht nach einer neueren Version und kommuniziert nur mit dem Update-Server.",
      howTo: [
        "Update-Checks eingeschaltet lassen, der Standard, für eine Benachrichtigung bei neuer Version.",
        "Im Einstellungsfenster neben Auto-Update Nie wählen, um automatische Prüfungen zu beenden.",
        "Ein angebotenes Update annehmen, um es mit einem Klick herunterzuladen und zu installieren.",
      ],
      whatHappens:
        "Die Update-Prüfung ist zusammen mit einem Download des Microsoft-WebView2-Installers beim ersten Start, falls diese Laufzeit fehlt, eine von nur zwei Arten von Netzwerkverkehr, den die App von sich aus macht. Ein heruntergeladenes Update wird vor der Ausführung anhand einer veröffentlichten Prüfsumme verifiziert: eine fehlende oder falsche Prüfsumme blockiert die Installation, statt eine unbestätigte Datei auszuführen. Die dritte und letzte Art ist ffmpeg, für Videoexport/-konvertierung: Eine installierte Version bringt es mit und greift dafür nie auf das Netzwerk zu, eine portable Version bietet einen einmaligen Download nur an, wenn eine Videofunktion es tatsächlich braucht, und nur mit ausdrücklicher Zustimmung, nie automatisch, nie im Hintergrund. Nach einem Update verlinkt eine kleine Meldung auf die Änderungen. Das Fenster Über, neben \"Update suchen\" in den Einstellungen, zeigt die Version, die ganze Änderungsgeschichte und jeden mitgelieferten Open-Source-Bestandteil mit seiner Lizenz.",
      settings: ["Auto-Update und wie oft es prüft: Einstellungen"],
    },
    {
      section: "system",
      title: "Häufige Fragen",
      whatItIs: "Dieser Abschnitt beantwortet die am häufigsten auftauchenden Fragen.",
      howTo: [
        "\"Mein Screenshot ist leer\": der Bildschirmaufnahme-Ausschluss funktioniert wie vorgesehen; bei Bedarf im Expertenmenü ausschalten.",
        "\"Die App startet nicht\": sie benötigt die Microsoft-WebView2-Laufzeit, vom Installer automatisch angeboten.",
        "\"Ich finde eine Einstellung nicht\": das Suchfeld im Expertenmenü durchsucht alle Tabs auf einmal.",
        "\"Ein Hotkey bewirkt nichts\": eine andere App hält diese Kombination möglicherweise bereits; der Editor erklärt, warum.",
      ],
      settings: [],
    },
  ],
};
