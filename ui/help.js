// In-app documentation. EN + DE only (deliberate scope decision) —
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
      whatItIs: "A tile is a saved item on the grid; clicking it copies its text to the clipboard.",
      howTo: [
        "Double-click an empty grid cell, or the plus button, to create one.",
        "Type or paste the text, then save.",
        "Click any tile to copy it.",
      ],
      whatHappens:
        "Three sample tiles — an email with placeholders, an image, and a Markdown template — ship on first launch; remove them with one click above the grid once they are no longer needed.",
      settings: [],
    },
    {
      section: "basics",
      title: "Introduction tour",
      whatItIs: "A five-step walkthrough of the basics, shown once automatically right after the example tiles are created.",
      howTo: [
        "First launch — opens on its own, right after the example prompts are seeded.",
        "Later — reopen it from the settings dialog, or the expert menu's \"Reopen introduction\" action.",
        "←/→ or the dots move between steps, Enter continues, Escape or \"Skip\" leaves any time — both ask to confirm first.",
      ],
      whatHappens:
        "The last step offers to switch on the screenshot editor, colour picker, or text recognition right there, showing each one's shipped default; nothing is written until the tour is actually finished, and only the switches actually changed. A first-launch data-folder or WebView2 prompt always takes priority — the tour just waits for its turn.",
      settings: ["Open automatically on first launch — Expert menu → System & data → Help"],
    },
    {
      section: "basics",
      title: "Creating tiles",
      whatItIs: "Beyond plain text, a tile can carry an attached file, a screenshot, or a picked screen colour.",
      howTo: [
        "File attach — clicking the tile copies the file itself, not text.",
        "Screenshot tool — capture a region or a whole window into a new tile.",
        "Colour picker — pick any on-screen pixel; copies as hex, RGB, or HSL.",
        "Drag and drop — drop a file or image onto the grid.",
      ],
      whatHappens: "The attached file, screenshot, or colour becomes the tile's content — a later copy delivers that file, image, or colour, never text.",
      settings: ["File attach, screenshot tool, colour picker, drag and drop — Expert menu → Features → Create prompts", "Keep a copy of an attached file, and delete that copy with its tile — Expert menu → Features → Create prompts"],
      tip: "Pick a colour format — hex, RGB, or HSL — from the toast that appears right after a pick.",
    },
    {
      section: "basics",
      title: "Variables & placeholders",
      whatItIs:
        "Placeholder markup inside a tile's text opens a fill-in popup before the finished text reaches the clipboard.",
      howTo: [
        "Type {#{PlaceholderName}#} inside the text.",
        "For a self-filling value, click a dynamic-placeholder chip under the text box instead of typing it.",
        "Click the tile to copy — the fill-in popup opens automatically.",
        "Enter a value per placeholder, or pick one for choice placeholders, and confirm.",
      ],
      whatHappens:
        "Only the filled-in text reaches the clipboard — the raw {#{...}#} markup is never copied. A dynamic placeholder (date, time, clipboard, UUID) uses its own syntax, {#{/date\\}#}, and fills itself in with no popup at all.",
      settings: [
        "Dynamic placeholders, choice placeholders, remember choices, popup in its own window — Expert menu → Features → Placeholders",
      ],
    },
    {
      section: "basics",
      title: "The editor",
      whatItIs: "The editor is a plain text box by default; a few opt-in features add structure and history to it.",
      howTo: [
        "Open a tile's editor — double-click the tile, or Edit from its menu.",
        "Turn on Markdown preview, syntax highlighting, version history, or private notes in the expert menu.",
      ],
      whatHappens:
        "Version history keeps every saved change so an earlier version can be restored. Markdown preview renders next to, or instead of, the text box without altering what is actually stored. Private notes are never copied or exported with the text.",
      settings: ["Markdown preview, syntax highlighting, version history, private notes — Expert menu → Features → Editor", "Monospace text box — Expert menu → Features → Editor"],
      tip: "Markdown preview and private notes both ship off.",
    },
    {
      section: "copying",
      title: "Copying",
      whatItIs: "Copying is the sequence between clicking a tile and its text landing on the clipboard.",
      howTo: [
        "Click a tile once to copy.",
        "Click again quickly, or press Enter, to also paste — if auto-paste is on.",
        "Ctrl-drag a tile onto another window to drop its text directly, with no clipboard round-trip.",
      ],
      whatHappens:
        "A short confirmation bubble or flash confirms the copy. Chaining, clicking several tiles in a row, combines them into one clipboard entry instead of overwriting each other; hovering a text tile also shows a small chain icon next to its selection checkbox that starts a chain with that tile first. Chaining is not the same as rotation: chaining combines several different tiles, rotation changes what a single tile holds from copy to copy (see Rotating tiles).",
      settings: ["Chaining, per-tile hotkeys, auto-paste, drag-out, close-on-copy — Expert menu → Features → Copying", "Which key drags a tile out, and whether that key drags out or moves it — Expert menu → Features → Copying"],
    },
    {
      section: "alwaysOnTop",
      title: "Floating buttons",
      whatItIs: "Floating buttons are small always-on-top copy buttons that sit outside the main window, over whatever app is in use.",
      howTo: [
        "Right-click a tile, then \"Toggle floating button\", to pin it as a pill.",
        "Drag the pill to move it; right-click it for size, edit, or remove.",
      ],
      whatHappens: "Positions survive app restarts.",
      settings: ["Floating buttons — Expert menu → Features → Floating buttons & folders"],
    },
    {
      section: "alwaysOnTop",
      title: "Folders",
      whatItIs: "The same gesture groups things in three places: drop one item onto another of its own kind to gather it into a folder.",
      howTo: [
        "Drag one view tab onto another to make a group tab.",
        "Drag one floating button onto another to make a pill folder.",
        "On the main grid, drag one tile onto another to make a grid folder.",
        "Drag a member back out to free it.",
      ],
      whatHappens:
        "A pill or grid folder shows up to four member previews and a count, dragging a member out frees it, and it dissolves once one member is left; a tile belongs to only one grid folder at a time. A group tab instead opens the list of its views when clicked.",
      settings: ["Pill folders, grid folders — Expert menu → Features → Floating buttons & folders", "View groups — Expert menu → Features → Views and quick grid"],
      tip: "All three kinds are on out of the box; switch any of them off in the expert menu if items are never grouped that way.",
    },
    {
      section: "alwaysOnTop",
      title: "Tray",
      whatItIs: "The tray icon's right-click menu offers quick access without opening the main window.",
      howTo: ["Right-click the tray icon.", "Pick an item from \"Recently copied\" or \"Favourites\" to copy it immediately."],
      whatHappens: "The menu copies straight from the tray — the main window never has to open for this.",
      settings: ["Recently copied section, favourites section, entry counts — Expert menu → Features → Tray menu"],
    },
    {
      section: "copying",
      title: "Clipboard watcher",
      whatItIs: "The clipboard watcher logs everything copied anywhere on the system into a reviewable inbox.",
      howTo: [
        "Turn the watcher on in the expert menu.",
        "Copy text anywhere on the system as normal.",
        "Open the clipboard inbox to review captured entries and save the useful ones as tiles.",
      ],
      whatHappens:
        "Nothing is watched until it is turned on. Copies shorter than the minimum length are ignored, and the oldest entries drop once the inbox grows past its cap.",
      settings: ["Minimum length, inbox cap — Expert menu → Features → Clipboard watcher"],
      tip: "Off by default — nothing is watched until it is turned on.",
    },
    {
      section: "organising",
      title: "Tile behaviour & appearance",
      whatItIs: "These settings control how a tile reacts to the mouse and what it looks like.",
      howTo: [
        "Right-click a tile for its menu — edit, pin, hide, delete, and more.",
        "Drag a tile to any grid cell; drop it on an occupied cell to swap.",
        "Turn on hover lift, shadows, gradients, badges, and similar looks in the expert menu.",
      ],
      whatHappens: "These are interaction and looks only — none of them change what a tile copies.",
      settings: [
        "Tile menu, drag-to-reorder, hover lift — Expert menu → Buttons → Tiles & copying",
        "Type badges, shadows, gradients, rounded corners, bold, uppercase, or monospace text — Expert menu → Buttons → Tile appearance",
      ],
    },
    {
      section: "look",
      title: "Tooltips",
      whatItIs: "Hovering a tile shows its full text, length, and any private note.",
      howTo: [
        "Hover a tile and wait for the delay to elapse.",
        "Adjust delay, auto-hide timeout, or width in the expert menu if needed.",
      ],
      whatHappens: "The tooltip auto-hides after its timeout even if the mouse stays put.",
      settings: ["Delay before showing, auto-hide timeout, tooltip width — Expert menu → Buttons → Tooltips"],
    },
    {
      section: "organising",
      title: "Favourites",
      whatItIs: "A star marks a tile as a favourite for quick access later.",
      howTo: ["Click the star on a tile, or use \"Favourite\" from its menu.", "Open the dedicated favourites view to see only starred tiles."],
      whatHappens: "Favourites get their own reorderable view, independent of wherever else the tile lives.",
      settings: ["Favourites view, its own reordering — Expert menu → Buttons → Favorites"],
    },
    {
      section: "look",
      title: "Scaling & zoom",
      whatItIs: "Independent size sliders adjust different parts of the window separately.",
      howTo: ["Open Settings and find the scaling sliders.", "Adjust overall UI, dialog, composer, or icon scale independently."],
      whatHappens: "Each slider only affects its own part of the window — raising icon scale, for example, does not resize dialogs.",
      settings: ["Overall UI scale, modal scale, composer scale, icon scale — Expert menu → Appearance → Sizes (%)"],
    },
    {
      section: "look",
      title: "Appearance & themes",
      whatItIs: "Eight built-in themes cover system, light, dark, and five accent styles.",
      howTo: ["Pick a theme from Settings.", "Open the theme editor, once enabled, to adjust any theme's colours and save a custom one."],
      whatHappens: "\"System\" follows the OS light or dark setting live. A custom theme is saved separately — the built-in it was based on stays untouched.",
      settings: ["Theme choice — Settings", "Theme editor — Expert menu → Appearance → Global colour palette", "Accent-coloured header, frosted dialogs, hidden or accent-coloured scrollbars, smooth scrolling — Expert menu → Appearance → Visual"],
      tip: "Custom themes are on out of the box; switching them off in the expert menu also hides the editor button.",
    },
    {
      section: "organising",
      title: "Library & search",
      whatItIs: "The library lists every tile as a searchable, filterable list, unlike the grid, which shows only what fits.",
      howTo: [
        "Click the library button in the toolbar to open it.",
        "Type to search, or use the type and colour filters to narrow the list.",
        "Click a row to copy.",
        "Use its menu to edit, drag onto the grid, tag, or delete.",
      ],
      whatHappens: "Search is typo-tolerant by default and, unless turned off, sorts by most-used or most-recently-used.",
      settings: ["Fuzzy search, smart sort, type and colour filters — Expert menu → Library & History → Library", "Focus the search field on opening, multi-select for batch actions, image and video previews in the rows — Expert menu → Library & History → Library"],
    },
    {
      section: "organising",
      title: "Tags",
      whatItIs: "Tags are free-form labels assigned to tiles, used to filter the library or the grid.",
      howTo: [
        "Type a tag name in a tile's editor to add it, or pick an existing one.",
        "Use the tag filter row in the library to narrow the list.",
      ],
      whatHappens: "Renaming, recolouring, or deleting a tag from the tag manager applies everywhere it is used, in one step.",
      settings: ["Tag filter mode — match any selected tag, or all of them — Expert menu → Library & History → Tags"],
    },
    {
      section: "copying",
      title: "Copy history",
      whatItIs: "Copy history logs what was copied and when, separate from the clipboard watcher.",
      howTo: [
        "Click \"Copy history\" to open the journal.",
        "Search, filter by type or colour, or star a favourite from the list.",
        "Delete a single row, or clear the whole history.",
      ],
      whatHappens: "Grouped view shows one row per tile — its most recent copy — instead of every event. History older than the retention setting is dropped automatically.",
      settings: ["Grouped view, retention — Expert menu → Library & History → Copy history", "Skip an entry that repeats the copy before it — Expert menu → Library & History → Copy history"],
    },
    {
      section: "organising",
      title: "Media",
      whatItIs: "These settings control tiles that carry a video or an image.",
      howTo: [
        "Attach a video or image to a tile — file attach, paste, or drag and drop.",
        "Adjust playback and privacy-leaning looks in the expert menu.",
      ],
      whatHappens: "Video settings — autoplay, mute, loop, volume — apply to every video tile; grayscale or dim-until-hovered only changes how media looks, never what gets copied. The library and copy history also show a video's length on its thumbnail, and its resolution and file size in the tooltip; the main grid never shows either.",
      settings: ["Video controls, autoplay, mute, loop — Expert menu → Media → Media", "Grayscale, or dim until hovered — Expert menu → Media → Media", "Accent border and rounded corners around media — Expert menu → Media → Media", "Duration/resolution/size in the library and copy history: Expert menu → Media → Media", "Fill in media info for older attachments: Expert menu → Media → Media"],
    },
    {
      section: "look",
      title: "Toolbar",
      whatItIs: "The toolbar setting controls which buttons the header and composer bar show.",
      howTo: ["Right-click the header for a quick toggle list, or use the expert menu for more."],
      whatHappens: "Hiding a button only hides it — the feature behind it, if any, keeps working through its own hotkey or menu entry.",
      settings: ["Quick grid-size control, library and history buttons, pin button, multiple views, view groups — Expert menu → System & data → Toolbar", "Reveal the quick grid-size control on hover only, dim the whole bar until hovered, keep buttons of switched-off features visible — Expert menu → System & data → Toolbar"],
    },
    {
      section: "system",
      title: "Privacy",
      whatItIs: "These settings keep this window's contents out of screenshots, screen shares, and passers-by.",
      howTo: [
        "Screen-capture exclusion is on from the start.",
        "Turn on tile or media blur, hidden tile names, or dim-unhovered in the expert menu, as needed.",
      ],
      whatHappens:
        "Screen-capture exclusion makes the window invisible to screenshot and recording tools, including its own — so if a capture comes out empty, check this setting first. An optional, off-by-default setting reads text out of an attached picture so the library search can find it too; a hit found only there carries a \"found in image\" badge, so it never looks like a random match. That recognized text stays out of the app otherwise: it is never shown, never exported, never synchronized, and its length is capped. Removing or replacing the picture clears it right away.",
      settings: ["Exclude from screen capture, blur tiles or media, hide names, dim unhovered tiles — Expert menu → System & data → Privacy", "Index text inside attached pictures for search, image text length limit: Expert menu → Library & History → Library"],
      tip: "Screen-capture exclusion ships on — turn it off for normal screenshots or recordings of this window.",
    },
    {
      section: "system",
      title: "System",
      whatItIs: "System settings cover app-level behaviour that is not about any single tile.",
      howTo: ["Open the expert menu's System group for updates, navigation, instances, and the data folder."],
      whatHappens:
        "Update checks are on by default and never install anything without asking first. Changing the data folder migrates the existing database there. Allowing multiple instances takes a restart to apply.",
      settings: ["Update checks, keyboard navigation, allow multiple instances, data folder — Expert menu → System & data → System"],
    },
    {
      section: "system",
      title: "Data maintenance",
      whatItIs: "Data maintenance gathers housekeeping tools reached from the expert menu's data group.",
      howTo: ["Open the expert menu → System & data → Data & maintenance.", "Run the duplicate finder, open the trash, or import or export from there."],
      whatHappens:
        "A deleted tile sits in the trash, with its own retention setting, before it is gone for good. The duplicate finder only flags near-identical tiles by text similarity — it never deletes anything on its own.",
      settings: ["Duplicate finder, trash retention, import and export — Expert menu → System & data → Data & maintenance", "Take a backup before a reset or a delete-everything — Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "system",
      title: "Standalone: Backup manager",
      whatItIs: "The backup manager creates encrypted, compressed snapshots of the entire library and settings.",
      howTo: [
        "Open the Backup manager from Settings.",
        "Turn on automatic backups and pick an interval, or create one manually.",
        "Restore from the snapshot list, choosing data only, settings only, or both.",
      ],
      whatHappens:
        "Retention tiers — daily, weekly, monthly, yearly — thin out older snapshots automatically instead of keeping every one forever. A backup is either whole or absent; a failed one leaves nothing behind. The same window also has a usage-statistics section — totals, a 14-day activity chart, most-used tiles, most-used tools with a list of the ones never touched — unrelated to the backups themselves, plus a storage breakdown of what is taking up disk space.",
      settings: [
        "Interval, retention tiers, password, restore scope — Backup manager (its own window, opened from Settings)",
        "Usage statistics, how many tiles it lists — same window, below the backups",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Standalone: Quick launcher",
      whatItIs:
        "The quick launcher is a small always-on-top popup that searches every tile's name and text as you type — a plain substring match, not a fuzzy one, so a name hit ranks above a text hit but a typo will not find a match.",
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
        "Right-click a tab to rename, resize, recolour, or delete it.",
        "Drag one view tab onto another to merge them into a group.",
      ],
      whatHappens: "A tile normally sits in exactly one view — adding it elsewhere moves it there — unless \"Tile in several views\" is on, which ships off. Deleting a view does not delete the tiles placed on it; a tile also shown in another view keeps working there.",
      settings: ["Multiple views, view groups, tile in several views — Expert menu → System & data → Toolbar"],
    },
    {
      section: "look",
      title: "Standalone: Theme editor",
      whatItIs: "The theme editor adjusts any built-in theme's colours and saves the result as a new theme.",
      howTo: [
        "Open the theme editor — Expert menu → Appearance → Global colour palette.",
        "Custom themes must be on, which they are by default.",
        "Pick a base theme, adjust colours, and save under a new name.",
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
        "Hover over a collapsed view group so it opens and the drop can target a view inside.",
        "Adjust the grid size with the quick columns and rows fields above it.",
      ],
      whatHappens:
        "A tile keeps its cell until moved — the grid never rearranges itself. Shrinking the grid does not delete tiles that fall outside the new size; they stay assigned and reappear once it grows back. The 20x20 default is only a starting point — the expert menu's limit setting raises it up to 200x200.",
      settings: ["Quick grid-size control, grid border — Expert menu → System & data → Toolbar", "Maximum grid size — Expert menu → Appearance → Limits & values"],
    },
    {
      section: "copying",
      title: "Hotkeys",
      whatItIs:
        "Two independent hotkey types exist: one global combination that opens the quick launcher, and one combination per tile.",
      howTo: [
        "Set the global hotkey in Settings — default Ctrl+Alt+P.",
        "Give a tile its own hotkey from its editor.",
        "Choose whether it should paste as well as copy.",
        "\"All hotkeys\", next to the global hotkey field, opens every hotkey in one searchable list.",
        "Each row rebinds or clears on the spot.",
        "If a combination is already used, the field explains why instead of accepting it silently.",
      ],
      whatHappens:
        "A combination must include Ctrl or Alt; Shift alone, or a bare key, is refused so ordinary typing is never swallowed. The app hotkey always wins a conflict; among tiles, the first one in the list keeps a combination two tiles happen to share. Importing a backup that carries a colliding hotkey drops the losing one instead of failing the import, and the import summary says how many were dropped. A tile with its own hotkey shows a small keyboard symbol on it.",
      settings: [
        "Global hotkey, all hotkeys — Settings",
        "Per-tile hotkeys, paste-after-copy — Expert menu → Features → Copying",
        "Hotkey badge, badge opacity — Expert menu → Buttons → Tiles & copying",
      ],
    },
    {
      section: "system",
      title: "Trash & restoring tiles",
      whatItIs: "A deleted tile moves to the trash first, not straight to permanent deletion.",
      howTo: [
        "Delete a tile as normal — a second click on the delete button confirms it.",
        "Open the trash from the expert menu's data group.",
        "Restore a tile from there, or let retention clear it automatically.",
      ],
      whatHappens:
        "Retention is set in days; zero means keep forever. Restoring places the tile in the active view's first free cell — not necessarily its original view or cell — and does not bring back a floating button, folder membership, or hotkey it had before deletion; those need setting up again.",
      settings: ["Trash retention — Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "system",
      title: "Duplicate finder",
      whatItIs: "The duplicate finder flags tiles that are textually very similar without deleting anything.",
      howTo: [
        "Open the duplicate finder from the expert menu's data group.",
        "Review each flagged pair side by side.",
        "Keep one, or mark the pair as not a duplicate to stop it being flagged again.",
      ],
      whatHappens:
        "Matching compares each tile's main text — its name, if it has no text — by similarity, not an exact match; the threshold controls how close counts as close enough. A rotating tile is only compared on its first entry. A shortcut in the settings dialog opens it directly too, next to import and export, so it does not take the expert menu to find.",
      settings: ["Duplicate finder, similarity threshold — Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "system",
      title: "Import & export",
      whatItIs: "Plain CSV/TXT export covers text alone; a full backup file covers everything.",
      howTo: [
        "Export or import from the expert menu's data group.",
        "A CSV/TXT import shows a name and duplicate status per item first.",
        "A full backup restores directly, from its own window.",
        "Confirm to actually import, or cancel and nothing changes.",
      ],
      whatHappens: "CSV/TXT only round-trips text; images, files, layout, and settings need the full backup format instead.",
      settings: ["Import and export — Expert menu → System & data → Data & maintenance"],
    },
    {
      section: "system",
      title: "Markdown folder sync (beta)",
      whatItIs:
        "Folder sync mirrors every text tile as its own readable Markdown file in a chosen folder.",
      howTo: [
        "Turn it on and pick a folder in the expert menu.",
        "Edit a tile normally, or edit its Markdown file directly — both directions stay in sync.",
      ],
      whatHappens:
        "Unlike the database, the Markdown files are plain text — anyone with access to that folder can read the content, so a synced or shared folder should be avoided for anything sensitive. A tile with an attached file, image, or PDF syncs as a reference: the header carries the path and the PDF page, never the file itself, so another machine needs the same path for the button to work there. A screenshot pasted straight into a tile has no path at all and stays in the database only. A view or folder named in an incoming file that does not exist yet on this machine is created automatically, matched by name. Deleting a file in the folder does not delete the tile (it is written again on the next round); deleting the tile in the app does take its file with it. The file name is the tile's permanent id, so renaming a file by hand produces a second tile instead of a renamed one. If a tile and its file changed at the same time, the file wins that round and the replaced version is kept in the tile's version history; the other way round the app wins the file and the file's text is kept beside it as a conflict copy. Files this app did not write are never changed or deleted, and a file past 8 MB is skipped whole rather than cut short. Picking a different folder asks whether the old one should be deleted, and that delete only ever removes the files this app wrote itself, never a sub-folder, stopping entirely if anything unknown is in there. An unreachable folder pauses the sync after a few failed rounds instead of retrying forever.",
      settings: ["Folder sync, sync folder, poll interval, pause after failed rounds — Expert menu → Features → Prompt sync"],
      tip: "This is a beta feature — keep the sync folder somewhere only trusted eyes can read.",
    },
    {
      section: "basics",
      title: "Screenshot tool",
      whatItIs: "The screenshot tool captures part or all of the screen into a new tile.",
      howTo: [
        "Click the camera icon.",
        "Drag a region, or click a window to capture it whole, including protected ones.",
        "In the confirmation dialog, save it as a tile, a file, or leave it on the clipboard.",
      ],
      whatHappens:
        "The app's own windows are hidden for the moment of capture so they never end up in the shot. The result reaches the clipboard immediately; saving it as a tile or file is a separate, optional step.",
      settings: ["Screenshot tool, save location, preview size — Expert menu → Features → Create prompts"],
    },
    {
      section: "basics",
      title: "Screenshot editor",
      whatItIs: "Opens right after a capture, before the shot is saved — crop, mark up, and annotate it before it becomes a tile.",
      howTo: [
        "Take a screenshot; the editor opens automatically over the capture.",
        "Pick a tool from the toolbar: crop, pen, marker, line, arrow, rectangle, ellipse, text, step number, pixelate, spotlight, or hand.",
        "Undo/redo steps back through edits; save when done.",
      ],
      whatHappens:
        "Every edit is one step in an undo/redo list capped by its own setting; pixelate always samples the original screenshot, never an already-edited layer, so blurring a spot never smears earlier marks. Snap-to-edges lines shapes up with the image border and with each other; a text label can get its own background plate for readability over busy backgrounds. Once zoomed in, the hand tool (or holding Space, or the middle mouse button) drags the view around for pixel-precise work, and the crop tool shows a resize cursor at each handle and a move cursor inside the current selection. Switching the editor off (below) skips straight to the confirmation dialog instead.",
      settings: ["Screenshot editor, default tool, snap-to-edges, text-label background, pen/marker/text size, pixelate block size, spotlight dimming, undo limit — Expert menu → Features → Create prompts"],
    },
    {
      section: "basics",
      title: "Open a file in the editor",
      whatItIs: "Opens an existing picture or video in whichever editor its content calls for, instead of only ever reaching an editor through a fresh capture or recording.",
      howTo: [
        "Right-click the screenshot or record button, or an image/video attachment in the library.",
        "Choose \"Open a file in the editor\" and pick the file.",
        "Edit as usual; saving asks where to put the result.",
      ],
      whatHappens:
        "The file's content decides which editor opens, not its extension — a video saved with a picture extension still opens the video editor. The editor always works on a copy, so the original file is never changed; the save dialog starts in the original file's own folder. A file past the configured size limit is rejected before it loads.",
      settings: ["Largest file to open — Expert menu → Capture → Both editors"],
    },
    {
      section: "basics",
      title: "Screen recording",
      whatItIs: "Records a window, a monitor, or a dragged region as a video, next to the screenshot tool.",
      howTo: [
        "Click the record button; pick a window, drag a region, or click empty desktop for the whole screen — same overlay as the screenshot tool.",
        "An optional countdown toast counts down before capture actually starts.",
        "A small floating control bar shows elapsed time and file size, and offers Pause/Resume, Stop and Discard; two optional global hotkeys and the tray menu can stop or pause/resume it too, live only while it is actually running. A window recording using the every-display or current-display canvas also gets a switch-target button, to swap what it is capturing without starting over.",
        "Stopping offers Save, Save as…, Copy, Edit or Discard.",
      ],
      whatHappens:
        "MP4 (H.264) is always what gets written while recording; three quality steps (Small/High/Lossless) trade file size for detail, and a format setting can turn the result into WebM, AVI, MOV, MKV, animated WebP or APNG afterward — the last three carry no audio and are capped at 30 seconds. Anything other than plain MP4, or the Lossless step, needs ffmpeg (a separate download, offered inline when missing) — a failed or cancelled conversion always falls back to the untouched original recording. System audio and microphone audio both record on by default, each into its own track — the microphone track is captured but not mixed in audibly until you bring it in yourself from the video editor; unplugging the microphone mid-recording keeps picture and system audio going and just marks that track as lost. Save and Copy share one recording folder; Edit opens the clip in the video editor without deciding its fate, so closing the dialog afterward without saving or copying still discards the file. The control bar follows a moved recorded window and is itself never part of the recording. If the recorded window closes on its own, the recording stops automatically and keeps what was captured so far. A recording that never reached its own stop path (a crash, a forced shutdown) is removed automatically once it clears a grace period, so a just-stopped recording that is still finalising is never touched.",
      settings: ["Screen recording, cursor, countdown, audio sources and device, bitrate, quality, format, recording folder, stop/pause hotkeys, tray entries, auto-remove broken recordings — Expert menu → Features → Create prompts"],
    },
    {
      section: "basics",
      title: "Video editor",
      whatItIs: "A small editor for a finished recording or any attached video, opened from the recording's own result dialog or a video prompt's right-click menu.",
      howTo: [
        "From a fresh recording — click Edit in the result dialog.",
        "From any video already saved as a prompt — right-click it and choose \"Edit video\".",
        "Trim, crop, change speed, fade in/out, resize (typed dimensions, a percentage, or a quick preset), and add image, freeze-frame, text, pixelate or spotlight overlays on the timeline.",
        "Export — a lossless fast path applies when only trimming, otherwise the whole clip re-encodes once.",
      ],
      whatHappens:
        "Nothing is processed until you export: the live preview plays your edits directly. A filmstrip and waveform give a zoomable overview of the whole clip for frame-exact navigation. The preview itself also zooms with the scroll wheel (up to 8×) and pans by dragging — the middle mouse button always, the left button once zoomed in — for placing a small overlay precisely on a large recording. A few ready-made export presets (unchanged quality, a smaller size for chat, an animated GIF) remember your last choice. Besides MP4, WebM and GIF, the export format list also offers AVI, MOV, MKV, animated WebP and APNG, each with a short note on what it's for; GIF, WebP and APNG never carry audio, warn once the clip runs past ten seconds, and are capped to a maximum resolution since all three grow large fast. Undo/redo covers every editing step, capped by an adjustable limit.",
      settings: ["Video editor, undo limit — Expert menu → Features → Create prompts"],
    },
    {
      section: "basics",
      title: "Text recognition (OCR)",
      whatItIs: "Pulls text out of a screenshot or an image file using Windows' own on-device recognizer — fully offline, nothing leaves the machine.",
      howTo: [
        "From the composer — the OCR button offers a screenshot or an existing image file.",
        "From a screenshot already taken — the snip dialog's \"Recognize text\" button.",
        "Review the result, then copy it or insert it directly.",
      ],
      whatHappens:
        "Recognition runs entirely on the device through Windows' own engine. The result dialog can auto-copy the text and join broken lines back into paragraphs, both switchable. A missing language pack opens the same dialog voice dictation uses — see \"Language packs\" below.",
      settings: ["Text recognition, auto-copy the result, join lines, upscale small images, recognition language — Expert menu → Features → Create prompts"],
    },
    {
      section: "basics",
      title: "Voice dictation",
      whatItIs: "The microphone button turns speech into text, recognised entirely on-device.",
      howTo: [
        "Click the microphone icon next to the composer, or in a tile's editor.",
        "Speak; the text appears as it is recognised.",
        "Click the icon again, press Escape, or save/close to stop.",
      ],
      whatHappens:
        "Recognition runs locally through Windows' own speech engine — nothing leaves the machine unless online recognition is explicitly allowed. Spoken punctuation words (\"comma\", \"new line\", …) turn into real punctuation; say \"literal\" before a word to type it instead of triggering it as a command. A missing language pack opens the language dialog directly, shared with text recognition (OCR). An optional global hotkey starts and stops a session without opening the window at all, delivering the text to the clipboard, pasted, or straight into the composer; the tray icon's tooltip shows \"recording\" for as long as it runs.",
      settings: ["Voice dictation, punctuation commands, auto-capitalise, silence auto-stop, insert position, global hotkey — Expert menu → Features → Create prompts"],
    },
    {
      section: "basics",
      title: "Language packs",
      whatItIs: "Text recognition and voice dictation share Windows' own language packs and the same install dialog.",
      howTo: [
        "Pick a language from the recognition-language dropdown; a missing pack opens the install dialog automatically.",
        "Installing a pack needs administrator rights — Windows asks for them once, through its own confirmation prompt.",
        "\"Manage languages…\" at the bottom of the language dropdown reopens the same dialog any time.",
      ],
      whatHappens:
        "On some Windows builds even listing what is installed needs administrator rights; if so, a button re-runs that check through the same elevation prompt. A background watcher notices when an installed pack's version changed or a Windows update silently removed it, and flags it without asking to reinstall right away — dismiss it for a while with \"remind me later\", or turn the watcher off entirely.",
      settings: ["Auto-pick the recognition language, background watcher for missing/changed packs — Expert menu → Features → Create prompts (one pair under Text recognition, one under Voice dictation)"],
    },
    {
      section: "basics",
      title: "Colour picker & pick history",
      whatItIs: "The colour picker is a pipette that reads any on-screen pixel as hex, RGB, or HSL.",
      howTo: [
        "Click the pipette icon in the composer bar.",
        "Move the loupe to the pixel wanted; scroll or use plus/minus to zoom.",
        "Click to copy the colour; right-click the pipette icon to reopen the last few picks.",
      ],
      whatHappens:
        "Nothing is ever saved to a tile automatically — a pick only reaches the clipboard. The pick history holds the last few colours and survives a restart, capped by its own setting.",
      settings: ["Colour picker, zoom, format, pick-history length — Expert menu → Features → Create prompts"],
      tip: "Pick a colour format — hex, RGB, or HSL — from the toast that appears right after a pick.",
    },
    {
      section: "basics",
      title: "Rotating tiles",
      whatItIs: "A tile can hold a list of texts instead of one; each copy serves the next entry.",
      howTo: [
        "Rotating tiles are on by default — turn them off in the expert menu if unused.",
        "In a tile's editor, open Advanced options and turn on \"Rotating texts\".",
        "The entry list then replaces the plain text field.",
        "Add more entries with \"+ Entry\"; icons beside each one reorder or remove it.",
        "Choose order (sequential) or shuffle (random, no repeats until every entry is used).",
        "Copy the tile as normal — it always serves the next entry, never asks which one.",
      ],
      whatHappens:
        "A small badge counts up on a grid tile, a floating pill, and a pill-folder member, like \"2/5\" — a grid-folder member shows only its file-type icon, not this badge. In order mode it is the absolute position; in shuffle mode it counts draws within the current cycle and restarts at 1 once every entry has been shown once, so it never jumps to an arbitrary position. The count survives restarts and full backups; a CSV/TXT import creates a new tile with a fresh pointer, so only the list of texts and its order or shuffle mode carry over, not the exact position. Turning rotation off on a tile with several entries keeps them all — only the first one is used until it is turned back on.",
      settings: ["Rotating tiles (on by default), default order or shuffle, badge, maximum entries — Expert menu → Features → Create prompts. The per-tile switch itself sits under Advanced options in the editor."],
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
        "A feature that ships switched off carries a small dot badge on its label, with a tooltip explaining what it does. Reset works at three levels: one value's own undo icon, a whole group's reset icon in its header, or every setting at once from the general reset action.",
      settings: [],
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
      settings: ["Keyboard navigation, fade delay — Expert menu → System & data → System"],
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
      whatHappens: "Moving from portable to installed, or back, is detected automatically and offered as a one-time migration — existing data is never silently left behind.",
      settings: ["Data folder — Expert menu → System & data → System"],
    },
    {
      section: "system",
      title: "Updates & security",
      whatItIs: "An optional update check looks for a newer version and only talks to the update server.",
      howTo: [
        "Leave update checks on, the default, to be notified of a new version.",
        "Turn them off in the expert menu if unwanted.",
        "Accept an offered update to download and install it with one click.",
      ],
      whatHappens:
        "The update check, together with a first-run download of the Microsoft WebView2 installer if that runtime is missing, is one of the only two kinds of network traffic the app makes on its own. A downloaded update is verified against a published checksum before it runs — a missing or mismatching checksum blocks the install instead of running an unverified file. The third and last kind is ffmpeg, used for video export/conversion: an installed build ships it and never touches the network for it, while a portable build offers a one-time download only when a video feature actually needs it and only with your explicit consent — never automatically, never in the background.",
      settings: ["Update checks — Expert menu → System & data → System"],
    },
    {
      section: "system",
      title: "Common questions",
      whatItIs: "This section answers the questions that come up most often.",
      howTo: [
        "\"My screenshot came out empty\" — screen-capture exclusion is working as intended; turn it off if unwanted.",
        "\"The app will not start\" — it needs the Microsoft WebView2 runtime, offered automatically by the installer.",
        "\"I cannot find a setting\" — the expert menu's search box scans every tab at once.",
        "\"A hotkey does not do anything\" — another app may already hold that combination; the editor explains why.",
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
        "Drei Beispielkacheln — eine E-Mail mit Platzhaltern, ein Bild und eine Markdown-Vorlage — sind beim ersten Start bereits da; ein Button über dem Raster entfernt sie mit einem Klick, sobald sie nicht mehr gebraucht werden.",
      settings: [],
    },
    {
      section: "basics",
      title: "Einführung",
      whatItIs: "Eine Einführung in fünf Schritten, die direkt nach dem Erstellen der Beispiel-Kacheln einmal automatisch angezeigt wird.",
      howTo: [
        "Beim ersten Start — öffnet sich direkt nach dem Anlegen der Beispiel-Prompts automatisch.",
        "Später — über den Einstellungsdialog oder die Aktion „Einführung erneut öffnen“ im Experten-Menü erneut öffnen.",
        "←/→ oder die Punkte wechseln zwischen den Schritten, Enter fährt fort, Escape oder „Überspringen“ beendet die Einführung jederzeit — beides erst nach Bestätigung.",
      ],
      whatHappens:
        "Im letzten Schritt lassen sich der Screenshot-Editor, die Farbauswahl oder die Texterkennung direkt aktivieren; dabei wird jeweils die ausgelieferte Standardeinstellung angezeigt. Gespeichert wird erst, wenn die Einführung tatsächlich abgeschlossen ist, und nur für Schalter, die wirklich geändert wurden. Ein beim ersten Start erscheinender Hinweis zum Datenordner oder zu WebView2 hat immer Vorrang — die Einführung wartet einfach, bis sie an der Reihe ist.",
      settings: ["Beim ersten Start automatisch öffnen — Experten-Menü → System & Daten → Hilfe"],
    },
    {
      section: "basics",
      title: "Kacheln anlegen",
      whatItIs: "Über reinen Text hinaus kann eine Kachel eine angehängte Datei, einen Screenshot oder eine gepickte Bildschirmfarbe tragen.",
      howTo: [
        "Dateianhang — ein Klick auf die Kachel kopiert die Datei selbst, nicht den Text.",
        "Screenshot-Tool — einen Bereich oder ein ganzes Fenster direkt in eine neue Kachel erfassen.",
        "Farbpipette — einen beliebigen Bildschirmpixel picken; kopiert als Hex, RGB oder HSL.",
        "Ziehen und Ablegen — eine Datei oder ein Bild direkt auf das Raster ziehen.",
      ],
      whatHappens: "Die angehängte Datei, der Screenshot oder die Farbe wird zum Inhalt der Kachel — ein späteres Kopieren liefert diese Datei, dieses Bild oder diese Farbe, nie Text.",
      settings: ["Dateianhang, Screenshot-Tool, Farbpipette, Ziehen und Ablegen — Experten-Menü → Funktionen → Prompts erstellen", "Kopie einer angehängten Datei behalten und diese Kopie mit der Kachel löschen — Experten-Menü → Funktionen → Prompts erstellen"],
      tip: "Farbformat — Hex, RGB oder HSL — im Hinweis wählen, der direkt nach der Auswahl erscheint.",
    },
    {
      section: "basics",
      title: "Variablen & Platzhalter",
      whatItIs:
        "Platzhalter-Markup im Text einer Kachel öffnet ein Ausfüll-Popup, bevor der fertige Text die Zwischenablage erreicht.",
      howTo: [
        "{#{PlatzhalterName}#} irgendwo in den Text eintippen.",
        "Für einen sich selbst ausfüllenden Wert einen dynamischen Chip unter dem Textfeld anklicken statt zu tippen.",
        "Klick auf die Kachel zum Kopieren — das Ausfüll-Popup öffnet sich automatisch.",
        "Pro Platzhalter einen Wert eingeben, bei Auswahl-Platzhaltern einen wählen, dann bestätigen.",
      ],
      whatHappens:
        "Nur der ausgefüllte Text erreicht die Zwischenablage — das rohe {#{...}#}-Markup wird nie kopiert. Ein dynamischer Platzhalter (Datum, Uhrzeit, Zwischenablage, UUID) nutzt eine eigene Schreibweise, {#{/date\\}#}, und füllt sich ganz ohne Popup selbst aus.",
      settings: [
        "Dynamische Platzhalter, Auswahl-Platzhalter, Auswahl merken, Popup im eigenen Fenster — Experten-Menü → Funktionen → Variablen",
      ],
    },
    {
      section: "basics",
      title: "Der Editor",
      whatItIs: "Der Editor ist standardmäßig ein einfaches Textfeld; einige optionale Funktionen verleihen ihm Struktur und einen Verlauf.",
      howTo: [
        "Editor einer Kachel öffnen — Doppelklick auf die Kachel oder Bearbeiten in ihrem Menü.",
        "Markdown-Vorschau, Syntax-Hervorhebung, Versionsverlauf oder private Notizen im Experten-Menü aktivieren.",
      ],
      whatHappens:
        "Der Versionsverlauf speichert jede Änderung, sodass eine frühere Version wiederhergestellt werden kann. Die Markdown-Vorschau erscheint neben oder anstelle des Textfelds, ohne den gespeicherten Inhalt zu verändern. Private Notizen werden nie mit dem Text kopiert oder exportiert.",
      settings: ["Markdown-Vorschau, Syntax-Hervorhebung, Versionsverlauf, private Notizen — Experten-Menü → Funktionen → Editor", "Textfeld in dicktengleicher Schrift — Experten-Menü → Funktionen → Editor"],
      tip: "Markdown-Vorschau und private Notizen sind standardmäßig deaktiviert.",
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
        "Eine kurze Bestätigungsblase oder ein Aufblinken bestätigt den Kopiervorgang. Verketten, mehrere Kacheln hintereinander anklicken, fasst sie zu einem Eintrag in der Zwischenablage zusammen, statt sie zu überschreiben; beim Überfahren einer Text-Kachel erscheint zusätzlich ein kleines Ketten-Symbol neben ihrer Auswahlbox, das direkt eine Kette mit dieser Kachel beginnt. Verketten ist nicht dasselbe wie Rotation: Verketten fasst mehrere verschiedene Kacheln zusammen, Rotation ändert, was eine einzelne Kachel von Kopie zu Kopie enthält (siehe Wechselnde Kacheln).",
      settings: ["Verketten, eigene Hotkeys pro Kachel, Auto-Paste, Drag-out, Schließen-nach-Kopieren — Experten-Menü → Funktionen → Kopieren", "Welche Taste eine Kachel herauszieht und ob diese Taste herauszieht oder sie verschiebt — Experten-Menü → Funktionen → Kopieren"],
    },
    {
      section: "alwaysOnTop",
      title: "Schwebende Buttons",
      whatItIs: "Schwebende Buttons sind kleine, immer im Vordergrund liegende Kopier-Buttons außerhalb des Hauptfensters, über jeder anderen App.",
      howTo: [
        "Rechtsklick auf eine Kachel, dann \"Schwebenden Button umschalten\", um sie als Pille anzuheften.",
        "Die Pille ziehen, um sie zu bewegen; Rechtsklick für Größe, Bearbeiten oder Entfernen.",
      ],
      whatHappens: "Positionen bleiben nach Neustarts erhalten.",
      settings: ["Schwebende Buttons — Experten-Menü → Funktionen → Schwebende Buttons & Ordner"],
    },
    {
      section: "alwaysOnTop",
      title: "Ordner",
      whatItIs: "Dieselbe Geste gruppiert an drei Stellen: ein Element auf ein gleichartiges ziehen, um es in einen Ordner aufzunehmen.",
      howTo: [
        "Einen Ansichts-Tab auf einen anderen ziehen, um einen Gruppen-Tab zu bilden.",
        "Einen schwebenden Button auf einen anderen ziehen, um einen Pillen-Ordner zu bilden.",
        "Im Hauptfenster eine Kachel auf eine andere ziehen, um einen Raster-Ordner zu bilden.",
        "Ein Mitglied wieder herausziehen, um es freizugeben.",
      ],
      whatHappens:
        "Ein Pillen- oder Raster-Ordner zeigt bis zu vier Vorschauen seiner Mitglieder und eine Anzahl; das Herausziehen eines Mitglieds gibt es frei, und der Ordner löst sich auf, sobald nur noch eines übrig ist. Eine Kachel gehört immer nur zu einem Raster-Ordner gleichzeitig. Ein Gruppen-Tab öffnet stattdessen beim Anklicken die Liste seiner Ansichten.",
      settings: ["Pillen-Ordner, Raster-Ordner — Experten-Menü → Funktionen → Schwebende Buttons & Ordner", "Ansichtsgruppen — Experten-Menü → Funktionen → Ansichten und Schnellraster"],
      tip: "Alle drei Arten sind ab Werk an; jede einzeln im Experten-Menü abschaltbar, wenn Elemente so nie gruppiert werden sollen.",
    },
    {
      section: "alwaysOnTop",
      title: "Taskleistenmenü",
      whatItIs: "Das Rechtsklick-Menü des Taskleisten-Icons bietet schnellen Zugriff, ohne das Hauptfenster zu öffnen.",
      howTo: ["Rechtsklick auf das Taskleisten-Icon.", "Einen Eintrag aus \"Zuletzt kopiert\" oder \"Favoriten\" wählen, um ihn sofort zu kopieren."],
      whatHappens: "Das Menü kopiert direkt aus der Taskleiste — das Hauptfenster muss dafür nicht geöffnet werden.",
      settings: ["Bereich für zuletzt Kopiertes, Bereich für Favoriten, Anzahl der Einträge — Experten-Menü → Funktionen → Taskleistenmenü"],
    },
    {
      section: "copying",
      title: "Zwischenablage-Überwachung",
      whatItIs:
        "Die Zwischenablage-Überwachung protokolliert alles, was irgendwo auf dem System kopiert wird, in einem einsehbaren Posteingang.",
      howTo: [
        "Die Überwachung im Experten-Menü einschalten.",
        "Text ganz normal irgendwo auf dem System kopieren.",
        "Den Posteingang öffnen, um erfasste Einträge zu prüfen und nützliche als Kacheln zu speichern.",
      ],
      whatHappens:
        "Es wird nichts überwacht, bis die Funktion eingeschaltet ist. Kopien unter der Mindestlänge werden ignoriert, und die ältesten Einträge fallen weg, sobald der Posteingang seine Kapazität überschreitet.",
      settings: ["Mindestlänge, Posteingang-Kapazität — Experten-Menü → Funktionen → Zwischenablage-Überwachung"],
      tip: "Standardmäßig aus — es wird nichts überwacht, bis die Funktion eingeschaltet ist.",
    },
    {
      section: "organising",
      title: "Kachel-Verhalten & -Aussehen",
      whatItIs: "Diese Einstellungen regeln, wie eine Kachel auf die Maus reagiert und wie sie aussieht.",
      howTo: [
        "Rechtsklick auf eine Kachel für ihr Menü — Bearbeiten, Anheften, Verbergen, Löschen und mehr.",
        "Eine Kachel auf eine beliebige Rasterzelle ziehen; auf einer besetzten Zelle loslassen, um zu tauschen.",
        "Hover-Lift, Schatten, Farbverläufe, Badges und ähnliche Optiken im Experten-Menü aktivieren.",
      ],
      whatHappens: "Das sind nur Interaktion und Optik — nichts davon ändert, was eine Kachel kopiert.",
      settings: [
        "Kachel-Menü, Drag-to-Reorder, Hover-Lift — Experten-Menü → Buttons → Kacheln & Kopieren",
        "Typ-Badges, Schatten, Farbverläufe, abgerundete Ecken, fetter, großgeschriebener oder dicktengleicher Text — Experten-Menü → Buttons → Kachel-Aussehen",
      ],
    },
    {
      section: "look",
      title: "Tooltips",
      whatItIs: "Das Überfahren einer Kachel mit der Maus zeigt ihren vollständigen Text, ihre Länge und eine eventuelle private Notiz.",
      howTo: [
        "Über eine Kachel fahren und die Verzögerung abwarten.",
        "Verzögerung, automatisches Ausblenden oder Breite im Experten-Menü anpassen, falls der Standard nicht passt.",
      ],
      whatHappens: "Der Tooltip blendet sich nach seinem Timeout automatisch aus, selbst wenn die Maus stehen bleibt.",
      settings: ["Verzögerung vor Anzeige, automatisches Ausblenden, Tooltip-Breite — Experten-Menü → Buttons → Tooltips"],
    },
    {
      section: "organising",
      title: "Favoriten",
      whatItIs: "Ein Stern markiert eine Kachel als Favorit für schnellen späteren Zugriff.",
      howTo: ["Auf den Stern einer Kachel klicken, oder \"Favorisieren\" in ihrem Menü nutzen.", "Die eigene Favoriten-Ansicht öffnen, um nur markierte Kacheln zu sehen."],
      whatHappens: "Favoriten erhalten ihre eigene sortierbare Ansicht, unabhängig davon, wo die Kachel sonst liegt.",
      settings: ["Favoriten-Ansicht, eigene Sortierung — Experten-Menü → Buttons → Favoriten"],
    },
    {
      section: "look",
      title: "Skalierung & Zoom",
      whatItIs: "Unabhängige Größenregler passen verschiedene Teile des Fensters getrennt voneinander an.",
      howTo: ["Einstellungen öffnen und die Skalierungs-Regler finden.", "UI-, Dialog-, Editor- oder Icon-Skalierung unabhängig voneinander anpassen."],
      whatHappens: "Jeder Regler beeinflusst nur seinen eigenen Teil des Fensters — eine höhere Icon-Skalierung vergrößert zum Beispiel keine Dialoge.",
      settings: ["Gesamte UI-Skalierung, Dialog-Skalierung, Editor-Skalierung, Icon-Skalierung — Experten-Menü → Aussehen → Größen (%)"],
    },
    {
      section: "look",
      title: "Aussehen & Themes",
      whatItIs: "Acht integrierte Themes decken System, Hell, Dunkel und fünf Akzent-Varianten ab.",
      howTo: ["Ein Theme in den Einstellungen wählen.", "Den Theme-Editor, sobald aktiviert, öffnen, um Farben eines Themes anzupassen und ein eigenes zu speichern."],
      whatHappens: "\"System\" folgt live der Hell-/Dunkel-Einstellung des Betriebssystems. Ein benutzerdefiniertes Theme wird separat gespeichert — das integrierte, auf dem es basierte, bleibt unangetastet.",
      settings: ["Theme-Wahl — Einstellungen", "Theme-Editor — Experten-Menü → Aussehen → Globale Farbauswahl", "Kopfzeile in Akzentfarbe, milchige Dialoge, versteckte oder eingefärbte Scrollbalken, weiches Scrollen — Experten-Menü → Aussehen → Visuell"],
      tip: "Benutzerdefinierte Themes sind ab Werk an; werden sie im Experten-Menü ausgeschaltet, verschwindet auch der Editor-Button.",
    },
    {
      section: "organising",
      title: "Bibliothek & Suche",
      whatItIs: "Die Bibliothek listet jede Kachel als durchsuchbare, filterbare Liste, anders als das Raster, das nur zeigt, was passt.",
      howTo: [
        "Klick auf den Bibliothek-Button in der Werkzeugleiste öffnet sie.",
        "Tippen zum Suchen, oder Typ- und Farb-Filter zum Eingrenzen der Liste nutzen.",
        "Klick auf eine Zeile zum Kopieren.",
        "Das Menü zum Bearbeiten, auf das Raster Ziehen, Taggen oder Löschen nutzen.",
      ],
      whatHappens: "Die Suche ist standardmäßig fehlertolerant und sortiert, falls nicht deaktiviert, nach am häufigsten oder zuletzt Verwendetem.",
      settings: ["Fuzzy-Suche, clevere Sortierung, Typ- und Farb-Filter — Experten-Menü → Bibliothek & Verlauf → Bibliothek", "Suchfeld beim Öffnen fokussieren, Mehrfachauswahl für Stapelaktionen, Bild- und Video-Vorschau in den Zeilen — Experten-Menü → Bibliothek & Verlauf → Bibliothek"],
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
      settings: ["Tag-Filter-Modus — jeden ausgewählten Tag treffen, oder alle — Experten-Menü → Bibliothek & Verlauf → Tags"],
    },
    {
      section: "copying",
      title: "Kopierverlauf",
      whatItIs: "Der Kopierverlauf protokolliert, was wann kopiert wurde, getrennt von der Zwischenablage-Überwachung.",
      howTo: [
        "Klick auf \"Kopierverlauf\" öffnet das Journal.",
        "Suchen, nach Typ oder Farbe filtern, oder einen Favoriten aus der Liste markieren.",
        "Eine einzelne Zeile löschen oder den gesamten Verlauf leeren.",
      ],
      whatHappens: "Die gruppierte Ansicht zeigt eine Zeile pro Kachel — ihre letzte Kopie — statt jedes einzelnen Ereignisses. Älterer Verlauf als die Aufbewahrungs-Einstellung wird automatisch gelöscht.",
      settings: ["Gruppierte Ansicht, Aufbewahrungsdauer — Experten-Menü → Bibliothek & Verlauf → Kopierverlauf", "Einen Eintrag überspringen, der die Kopie davor wiederholt — Experten-Menü → Bibliothek & Verlauf → Kopierverlauf"],
    },
    {
      section: "organising",
      title: "Medien",
      whatItIs: "Diese Einstellungen betreffen Kacheln, die ein Video oder Bild tragen.",
      howTo: [
        "Ein Video oder Bild an eine Kachel anhängen — Dateianhang, Screenshot, Einfügen oder Ziehen und Ablegen.",
        "Wiedergabe und privatsphärefreundliche Optiken im Experten-Menü anpassen.",
      ],
      whatHappens: "Video-Einstellungen — Autoplay, Stumm, Schleife, Lautstärke — gelten für jede Video-Kachel; Graustufen oder Abgedunkelt-bis-Hover ändert nur das Aussehen, nie den kopierten Inhalt. Bibliothek und Kopierverlauf zeigen außerdem die Länge eines Videos auf dem Vorschaubild sowie Auflösung und Dateigröße in der Kurzhilfe; das Hauptraster zeigt keins von beidem.",
      settings: ["Video-Steuerung, Autoplay, Stumm, Schleife — Experten-Menü → Medien → Medien", "Graustufen oder abgedunkelt bis gehovert — Experten-Menü → Medien → Medien", "Akzentrahmen und abgerundete Ecken um Medien — Experten-Menü → Medien → Medien", "Dauer/Auflösung/Größe in Bibliothek und Kopierverlauf: Experten-Menü → Medien → Medien", "Medienangaben bei älteren Anhängen nachtragen: Experten-Menü → Medien → Medien"],
    },
    {
      section: "look",
      title: "Werkzeugleiste",
      whatItIs: "Die Werkzeugleiste bestimmt, welche Buttons der Kopfbereich und die Composer-Leiste zeigen.",
      howTo: ["Rechtsklick auf den Kopfbereich für eine schnelle Umschaltliste, oder das Experten-Menü für das komplette Set nutzen."],
      whatHappens: "Das Ausblenden eines Buttons blendet ihn nur aus — die dahinterliegende Funktion funktioniert weiterhin über ihren eigenen Hotkey oder Menüeintrag.",
      settings: ["Schnelle Rastergrößen-Steuerung, Bibliothek- und Verlauf-Buttons, Anheft-Button, mehrere Ansichten, Ansichtsgruppen — Experten-Menü → System & Daten → Werkzeugleiste", "Rastergrößen-Steuerung erst beim Hovern zeigen, die ganze Leiste bis zum Hovern abdunkeln, Buttons abgeschalteter Funktionen sichtbar lassen — Experten-Menü → System & Daten → Werkzeugleiste"],
    },
    {
      section: "system",
      title: "Datenschutz",
      whatItIs: "Diese Einstellungen halten den Inhalt dieses Fensters aus Screenshots, Screen-Shares und dem Blick von Passanten heraus.",
      howTo: [
        "Der Ausschluss von Bildschirmaufnahmen ist von Anfang an aktiv.",
        "Bei Bedarf Unschärfe für Kacheln oder Medien, versteckte Namen oder abgedunkelte inaktive Elemente im Experten-Menü aktivieren.",
      ],
      whatHappens:
        "Der Ausschluss von Bildschirmaufnahmen macht das Fenster für Screenshot- und Aufnahmeprogramme unsichtbar, auch für die eigenen — bleibt eine Aufnahme also leer, zuerst diese Einstellung prüfen. Eine optionale, standardmäßig ausgeschaltete Einstellung liest Text aus einem angehängten Bild, damit die Bibliothekssuche ihn auch findet; ein Treffer nur dort trägt die Kennzeichnung \"im Bild gefunden\", damit er nie wie ein zufälliger Treffer wirkt. Dieser erkannte Text bleibt sonst außerhalb der App: nirgends angezeigt, nie exportiert, nie synchronisiert, und in der Länge begrenzt. Entfernen oder Ersetzen des Bilds löscht ihn sofort.",
      settings: ["Von Bildschirmaufnahmen ausschließen, Kacheln oder Medien unscharf machen, Namen verbergen, ungehoverte Kacheln abdunkeln — Experten-Menü → System & Daten → Datenschutz", "Text in angehängten Bildern für die Suche erfassen, Bildtext-Grenze: Experten-Menü → Bibliothek & Verlauf → Bibliothek"],
      tip: "Der Ausschluss von Bildschirmaufnahmen ist standardmäßig an — für normale Screenshots oder Aufnahmen dieses Fensters ausschalten.",
    },
    {
      section: "system",
      title: "System",
      whatItIs: "System-Einstellungen betreffen app-weites Verhalten, das nicht eine einzelne Kachel betrifft.",
      howTo: ["Die System-Gruppe des Experten-Menüs für Update-Checks, Tastaturnavigation, mehrere Instanzen und den Daten-Ordner öffnen."],
      whatHappens:
        "Update-Checks sind standardmäßig aktiv und installieren nie etwas, ohne vorher zu fragen. Ein Wechsel des Daten-Ordners migriert die bestehende Datenbank dorthin. Mehrere Instanzen zuzulassen erfordert einen Neustart.",
      settings: ["Update-Checks, Tastaturnavigation, mehrere Instanzen erlauben, Daten-Ordner — Experten-Menü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Datenwartung",
      whatItIs: "Die Datenwartung bündelt Wartungswerkzeuge aus der Daten-Gruppe des Experten-Menüs.",
      howTo: ["Experten-Menü → System & Daten → Daten & Wartung öffnen.", "Den Duplikat-Finder ausführen, den Papierkorb öffnen, oder von dort importieren oder exportieren."],
      whatHappens:
        "Eine gelöschte Kachel liegt zunächst im Papierkorb mit eigener Aufbewahrungsdauer, bevor sie endgültig verschwindet. Der Duplikat-Finder markiert nur textlich fast identische Kacheln — er löscht von sich aus nichts.",
      settings: ["Duplikat-Finder, Papierkorb-Aufbewahrung, Import und Export — Experten-Menü → System & Daten → Daten & Wartung", "Vor einem Zurücksetzen oder Alles-Löschen eine Sicherung anlegen — Experten-Menü → System & Daten → Daten & Wartung"],
    },
    {
      section: "system",
      title: "Eigenständig: Backup-Manager",
      whatItIs: "Der Backup-Manager erstellt verschlüsselte, komprimierte Snapshots der gesamten Bibliothek und Einstellungen.",
      howTo: [
        "Den Backup-Manager in den Einstellungen öffnen.",
        "Automatische Backups aktivieren und ein Intervall wählen, oder manuell eines erstellen.",
        "Aus der Snapshot-Liste wiederherstellen und dabei nur Daten, nur Einstellungen oder beides wählen.",
      ],
      whatHappens:
        "Aufbewahrungsstufen — täglich, wöchentlich, monatlich, jährlich — dünnen ältere Snapshots automatisch aus, statt jeden für immer zu behalten. Ein Backup ist entweder vollständig oder gar nicht vorhanden; ein fehlgeschlagenes hinterlässt nichts. Dasselbe Fenster enthält außerdem eine Nutzungsstatistik — Gesamtzahlen, ein 14-Tage-Diagramm, meistgenutzte Kacheln, meistgenutzte Werkzeuge mit einer Liste der nie benutzten — unabhängig von den Backups selbst, dazu einen Speicherüberblick, was auf der Festplatte wie viel Platz belegt.",
      settings: [
        "Intervall, Aufbewahrungsstufen, Passwort, Wiederherstellungsumfang — Backup-Manager (eigenes Fenster, geöffnet aus Einstellungen)",
        "Nutzungsstatistiken, wie viele Kacheln dort gelistet werden — selbes Fenster, unter den Backups",
      ],
    },
    {
      section: "alwaysOnTop",
      title: "Eigenständig: Schneller Launcher",
      whatItIs:
        "Der schnelle Launcher ist ein kleines, immer im Vordergrund liegendes Popup, das beim Tippen Name und Text jeder Kachel durchsucht — ein einfacher Text-Treffer, kein unscharfer Abgleich: Ein Treffer im Namen steht über einem Treffer im Text, aber ein Tippfehler findet nichts.",
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
        "Einen Ansichts-Tab auf einen anderen ziehen, um sie zu einer Gruppe zusammenzufassen.",
      ],
      whatHappens: "Eine Kachel sitzt normalerweise in genau einer Ansicht — wird sie woanders abgelegt, wandert sie dorthin — außer \"Kachel in mehreren Ansichten\" ist aktiv, was ab Werk aus ist. Das Löschen einer Ansicht löscht nicht die dort platzierten Kacheln; eine auch anderswo gezeigte Kachel funktioniert dort weiter.",
      settings: ["Mehrere Ansichten, Ansichtsgruppen, Kachel in mehreren Ansichten — Experten-Menü → System & Daten → Werkzeugleiste"],
    },
    {
      section: "look",
      title: "Eigenständig: Theme-Editor",
      whatItIs: "Der Theme-Editor passt die Farben eines beliebigen integrierten Themes an und speichert das Ergebnis als neues Theme.",
      howTo: [
        "Den Theme-Editor öffnen — Experten-Menü → Aussehen → Globale Farbauswahl.",
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
        "Über einer eingeklappten Ansichtsgruppe warten, bis sie sich öffnet, damit das Ablegen eine Ansicht darin treffen kann.",
        "Die Rastergröße mit den Schnell-Feldern für Spalten und Zeilen darüber anpassen.",
      ],
      whatHappens:
        "Eine Kachel behält ihre Zelle, bis sie bewegt wird — das Raster ordnet sich nie selbst neu. Das Verkleinern löscht keine außerhalb liegenden Kacheln; sie bleiben zugewiesen und erscheinen beim Vergrößern wieder. Die Standardgröße von 20x20 ist nur ein Startwert — die Limit-Einstellung im Experten-Menü hebt sie auf bis zu 200x200 an.",
      settings: ["Schnelle Rastergrößen-Steuerung, Rasterrahmen — Experten-Menü → System & Daten → Werkzeugleiste", "Maximale Rastergröße — Experten-Menü → Aussehen → Grenzwerte & Werte"],
    },
    {
      section: "copying",
      title: "Hotkeys",
      whatItIs:
        "Zwei unabhängige Hotkey-Arten: eine globale Kombination, die den schnellen Launcher öffnet, und eine Kombination pro Kachel.",
      howTo: [
        "Den globalen Hotkey in den Einstellungen festlegen — Standard Strg+Alt+P.",
        "Einer Kachel im Editor einen eigenen Hotkey geben.",
        "Wählen, ob er nach dem Kopieren auch einfügen soll.",
        "\"Alle Hotkeys\" neben dem globalen Feld öffnet jeden Hotkey in einer durchsuchbaren Liste.",
        "Jede Zeile wird dort direkt umbelegt oder gelöscht.",
        "Ist eine Kombination bereits vergeben, erklärt das Feld den Grund, statt sie stillschweigend zu akzeptieren.",
      ],
      whatHappens:
        "Eine Kombination muss Strg oder Alt enthalten; Shift allein oder eine bloße Taste wird abgelehnt, damit normales Tippen nie verschluckt wird. Der App-Hotkey gewinnt bei einem Konflikt immer; bei Kacheln behält die erste in der Liste eine geteilte Kombination. Der Import eines Backups mit kollidierendem Hotkey verwirft den verlierenden, statt den Import abzubrechen, und die Zusammenfassung nennt die Anzahl der verworfenen. Eine Kachel mit eigenem Hotkey zeigt ein kleines Tastatursymbol.",
      settings: [
        "Globaler Hotkey, alle Hotkeys — Einstellungen",
        "Eigene Hotkeys pro Kachel, Einfügen nach Kopieren — Experten-Menü → Funktionen → Kopieren",
        "Hotkey-Abzeichen, Deckkraft des Abzeichens — Experten-Menü → Buttons → Kacheln & Kopieren",
      ],
    },
    {
      section: "system",
      title: "Papierkorb & Kacheln wiederherstellen",
      whatItIs: "Eine gelöschte Kachel wandert zuerst in den Papierkorb, nicht direkt in die endgültige Löschung.",
      howTo: [
        "Eine Kachel wie gewohnt löschen — ein zweiter Klick auf den Löschen-Button bestätigt es.",
        "Den Papierkorb aus der Daten-Gruppe des Experten-Menüs öffnen.",
        "Eine Kachel dort wiederherstellen oder sie durch die Aufbewahrungsdauer automatisch löschen lassen.",
      ],
      whatHappens:
        "Die Aufbewahrungsdauer wird in Tagen festgelegt; null bedeutet für immer behalten. Das Wiederherstellen platziert die Kachel in die erste freie Zelle der aktiven Ansicht — nicht zwingend die ursprüngliche Ansicht oder Zelle — und bringt weder schwebenden Button noch Ordner-Mitgliedschaft oder Hotkey von vorher zurück; die müssen neu eingerichtet werden.",
      settings: ["Papierkorb-Aufbewahrung — Experten-Menü → System & Daten → Daten & Wartung"],
    },
    {
      section: "system",
      title: "Duplikat-Finder",
      whatItIs: "Der Duplikat-Finder markiert textlich sehr ähnliche Kacheln, ohne selbst etwas zu löschen.",
      howTo: [
        "Den Duplikat-Finder aus der Daten-Gruppe des Experten-Menüs öffnen.",
        "Jedes markierte Paar Seite an Seite prüfen.",
        "Eine Kachel behalten, oder das Paar als kein Duplikat markieren, damit es nicht erneut markiert wird.",
      ],
      whatHappens:
        "Der Abgleich vergleicht den Haupttext jeder Kachel — oder ihren Namen, falls sie keinen Text hat — nach Ähnlichkeit, nicht nach exakter Übereinstimmung; der Schwellenwert steuert, wie nah als nah genug gilt. Eine rotierende Kachel wird nur nach ihrem ersten Eintrag verglichen. Eine Abkürzung im Einstellungsdialog öffnet ihn auch direkt, neben Import und Export, sodass dafür nicht erst das Experten-Menü nötig ist.",
      settings: ["Duplikat-Finder, Ähnlichkeitsschwelle — Experten-Menü → System & Daten → Daten & Wartung"],
    },
    {
      section: "system",
      title: "Import & Export",
      whatItIs: "Einfacher CSV/TXT-Export deckt nur den Text ab; eine vollständige Backup-Datei deckt alles ab.",
      howTo: [
        "Aus der Daten-Gruppe des Experten-Menüs exportieren oder importieren.",
        "Ein CSV/TXT-Import zeigt zuerst Name und Duplikat-Status je Eintrag.",
        "Ein vollständiges Backup stellt direkt wieder her, aus seinem eigenen Fenster.",
        "Zum tatsächlichen Import bestätigen, oder abbrechen, wodurch sich nichts ändert.",
      ],
      whatHappens: "CSV/TXT transportiert nur Text; Bilder, Dateien, Layout und Einstellungen benötigen stattdessen das vollständige Backup-Format.",
      settings: ["Import und Export — Experten-Menü → System & Daten → Daten & Wartung"],
    },
    {
      section: "system",
      title: "Markdown-Ordner-Sync (Beta)",
      whatItIs:
        "Der Ordner-Sync spiegelt jede Text-Kachel als eigene lesbare Markdown-Datei in einem gewählten Ordner.",
      howTo: [
        "Im Experten-Menü einschalten und einen Ordner wählen.",
        "Eine Kachel normal bearbeiten, oder ihre Markdown-Datei direkt bearbeiten — beide Richtungen bleiben synchron.",
      ],
      whatHappens:
        "Anders als die Datenbank sind die Markdown-Dateien Klartext — wer Zugriff auf diesen Ordner hat, kann den Inhalt lesen; ein synchronisierter oder geteilter Ordner sollte deshalb für Sensibles vermieden werden. Eine Kachel mit angehängter Datei, Bild oder PDF wird als Verweis synchronisiert: Der Kopf trägt Pfad und PDF-Seite, nie die Datei selbst — der andere Rechner braucht also denselben Pfad, damit der Button dort funktioniert. Ein direkt eingefügter Screenshot hat gar keinen Pfad und bleibt nur in der Datenbank. Eine in einer eingehenden Datei genannte Ansicht oder ein Ordner, den es hier noch nicht gibt, wird automatisch angelegt, per Namensabgleich. Eine im Ordner gelöschte Datei löscht die Kachel nicht (sie wird im nächsten Durchlauf neu geschrieben); wer die Kachel in der App löscht, löscht auch ihre Datei. Der Dateiname ist die feste Kennung der Kachel, von Hand umbenennen erzeugt deshalb eine zweite Kachel statt einer umbenannten. Wurden Kachel und Datei gleichzeitig geändert, gewinnt in diesem Durchlauf die Datei, und die ersetzte Fassung bleibt in der Versionsgeschichte der Kachel; umgekehrt gewinnt die App die Datei, und deren Text bleibt als Konfliktkopie daneben liegen. Dateien, die diese App nicht geschrieben hat, werden nie geändert oder gelöscht, und eine Datei über 8 MB wird ganz übersprungen statt abgeschnitten. Ein Ordnerwechsel fragt, ob der alte Ordner gelöscht werden soll; gelöscht wird dabei nur, was diese App selbst geschrieben hat, nie ein Unterordner, und gar nichts, wenn etwas Fremdes darin liegt. Ein nicht erreichbarer Ordner pausiert die Synchronisierung nach einigen Fehlversuchen, statt endlos weiterzuversuchen.",
      settings: ["Ordner-Sync, Synchronisierungsordner, Abfragetakt, Pause nach Fehlversuchen — Experten-Menü → Funktionen → Prompt-Synchronisierung"],
      tip: "Dies ist eine Beta-Funktion — den Synchronisierungsordner nur dort ablegen, wo niemand Unbefugtes mitliest.",
    },
    {
      section: "basics",
      title: "Screenshot-Tool",
      whatItIs: "Das Screenshot-Tool erfasst einen Teil oder den gesamten Bildschirm direkt in eine neue Kachel.",
      howTo: [
        "Auf das Kamera-Icon klicken.",
        "Einen Bereich ziehen oder ein Fenster anklicken, um es ganz zu erfassen, auch geschützte.",
        "Im Bestätigungsdialog als Kachel speichern, als Datei speichern, oder einfach in der Zwischenablage belassen.",
      ],
      whatHappens:
        "Die eigenen Fenster der App werden für den Moment der Aufnahme ausgeblendet, sodass sie nie im Bild landen. Das Ergebnis erreicht sofort die Zwischenablage; das Speichern als Kachel oder Datei ist ein separater, optionaler Schritt.",
      settings: ["Screenshot-Tool, Speicherort, Vorschaugröße — Experten-Menü → Funktionen → Prompts erstellen"],
    },
    {
      section: "basics",
      title: "Screenshot-Editor",
      whatItIs: "Öffnet sich direkt nach einer Aufnahme, bevor der Screenshot gespeichert wird — zum Zuschneiden, Markieren und Beschriften, bevor daraus eine Kachel wird.",
      howTo: [
        "Einen Screenshot aufnehmen; der Editor öffnet sich automatisch über der Aufnahme.",
        "Ein Werkzeug aus der Symbolleiste wählen: Zuschneiden, Stift, Marker, Linie, Pfeil, Rechteck, Ellipse, Text, Schrittnummer, Verpixeln, Spotlight oder Hand.",
        "Mit Rückgängig/Wiederholen schrittweise durch die Bearbeitungen gehen; anschließend speichern.",
      ],
      whatHappens:
        "Jede Bearbeitung ist ein Schritt in einer Rückgängig/Wiederholen-Liste, deren Höchstzahl separat einstellbar ist. Beim Verpixeln wird immer der ursprüngliche Screenshot verwendet, nie eine bereits bearbeitete Ebene; dadurch werden frühere Markierungen beim Unkenntlichmachen einer Stelle nicht verschmiert. Das Einrasten an Kanten richtet Formen am Bildrand und aneinander aus. Textbeschriftungen können für bessere Lesbarkeit auf unruhigem Hintergrund eine eigene Hintergrundfläche erhalten. Beim Hineinzoomen verschiebt das Hand-Werkzeug (oder gehaltene Leertaste bzw. die mittlere Maustaste) die Ansicht für pixelgenaues Arbeiten, und das Zuschneide-Werkzeug zeigt an jedem Anfasser einen passenden Richtungspfeil sowie innerhalb des Ausschnitts einen Verschieben-Zeiger. Ist der Editor unten deaktiviert, erscheint stattdessen direkt der Bestätigungsdialog.",
      settings: ["Screenshot-Editor, Standardwerkzeug, Einrasten an Kanten, Hintergrund für Textbeschriftungen, Stift-/Marker-/Textgröße, Pixelblockgröße, Spotlight-Abdunklung, Rückgängig-Limit — Experten-Menü → Funktionen → Prompts erstellen"],
    },
    {
      section: "basics",
      title: "Datei im Editor öffnen",
      whatItIs: "Öffnet ein vorhandenes Bild oder Video in dem Editor, den sein Inhalt verlangt, statt einen Editor nur über eine frische Aufnahme zu erreichen.",
      howTo: [
        "Rechtsklick auf den Bildschirmfoto- oder Aufnahme-Knopf, oder auf einen Bild-/Videoanhang in der Bibliothek.",
        "\"Datei im Editor öffnen\" wählen und die Datei aussuchen.",
        "Wie gewohnt bearbeiten; beim Speichern wird nach dem Ziel gefragt.",
      ],
      whatHappens:
        "Der Inhalt der Datei entscheidet, welcher Editor sich öffnet, nicht ihre Endung — ein Video mit einer Bild-Endung öffnet trotzdem den Video-Editor. Der Editor arbeitet immer auf einer Kopie, das Original wird also nie verändert; der Speichern-Dialog startet im Ordner der Ausgangsdatei. Eine Datei über der eingestellten Größe wird abgelehnt, bevor sie geladen wird.",
      settings: ["Größte zu öffnende Datei — Experten-Menü → Aufnehmen → Beide Editoren"],
    },
    {
      section: "basics",
      title: "Bildschirmaufnahme",
      whatItIs: "Nimmt ein Fenster, einen Monitor oder einen gezogenen Bereich als Video auf, neben dem Screenshot-Werkzeug.",
      howTo: [
        "Auf den Aufnahme-Knopf klicken; ein Fenster anklicken, einen Bereich ziehen oder auf den freien Desktop klicken für den ganzen Bildschirm — dieselbe Übersicht wie beim Screenshot-Werkzeug.",
        "Ein Countdown zählt optional herunter, bevor die Aufnahme wirklich startet.",
        "Eine kleine Bedienleiste zeigt Laufzeit und Dateigröße und bietet Pause/Fortsetzen, Stopp und Verwerfen; zwei optionale globale Hotkeys sowie das Tray-Menü können sie ebenfalls stoppen oder pausieren/fortsetzen, aber nur, solange die Aufnahme wirklich läuft. Eine Fensteraufnahme mit der Leinwand für alle Bildschirme oder den aktuellen Bildschirm bekommt zusätzlich einen Knopf zum Wechseln des Aufnahmeziels, ohne von vorn zu beginnen.",
        "Stopp bietet Speichern, Speichern unter…, Kopieren, Bearbeiten oder Verwerfen an.",
      ],
      whatHappens:
        "Aufgenommen wird immer MP4 (H.264); drei Qualitätsstufen (Klein/Hoch/Verlustfrei) tauschen Dateigröße gegen Bildschärfe, ein Format-Feld kann das Ergebnis danach in WebM, AVI, MOV, MKV, animiertes WebP oder APNG umwandeln — die letzten drei ohne Ton und auf 30 Sekunden begrenzt. Alles außer reinem MP4 sowie die Stufe Verlustfrei braucht ffmpeg (ein separater Download, direkt angeboten, wenn es fehlt) — eine fehlgeschlagene oder abgebrochene Umwandlung fällt immer auf die unangetastete Originalaufnahme zurück. Systemton und Mikrofon werden beide standardmäßig aufgenommen, je als eigene Spur — die Mikrofonspur wird zwar aufgezeichnet, aber erst hörbar, wenn man sie später selbst im Video-Editor dazumischt; zieht man das Mikrofon während der Aufnahme ab, laufen Bild und Systemton weiter, nur diese Spur gilt als verloren. Speichern und Kopieren teilen sich einen Aufnahme-Ordner; Bearbeiten öffnet den Clip im Video-Editor, ohne über sein Schicksal zu entscheiden — schließt man den Dialog danach, ohne gespeichert oder kopiert zu haben, wird die Datei trotzdem verworfen. Die Bedienleiste folgt einem verschobenen Aufnahmefenster und ist selbst nie Teil der Aufnahme. Schließt sich das aufgenommene Fenster von selbst, stoppt die Aufnahme automatisch und behält, was bis dahin aufgenommen wurde. Eine Aufnahme, die nie ordnungsgemäß gestoppt wurde (Absturz, erzwungenes Beenden), wird automatisch entfernt, sobald sie eine Schonfrist überschreitet — eine gerade gestoppte Aufnahme, die noch abgeschlossen wird, bleibt dabei unangetastet.",
      settings: ["Bildschirmaufnahme, Mauszeiger, Countdown, Tonquellen und -gerät, Bitrate, Qualität, Format, Aufnahme-Ordner, Stopp-/Pause-Hotkeys, Tray-Einträge, defekte Aufnahmen automatisch entfernen — Experten-Menü → Funktionen → Prompts erstellen"],
    },
    {
      section: "basics",
      title: "Video-Editor",
      whatItIs: "Ein kleiner Editor für eine fertige Aufnahme oder ein angehängtes Video, geöffnet aus dem Ergebnisdialog einer Aufnahme oder über das Rechtsklick-Menü eines Video-Prompts.",
      howTo: [
        "Bei einer frischen Aufnahme — im Ergebnisdialog auf Bearbeiten klicken.",
        "Bei einem bereits gespeicherten Video-Prompt — rechtsklicken und „Video bearbeiten“ wählen.",
        "Zuschneiden, Kürzen, Geschwindigkeit ändern, Ein-/Ausblenden, Größe ändern (eingegebene Maße, ein Prozentwert oder eine Schnellauswahl) sowie Bild-, Standbild-, Text-, Verpixelungs- oder Spotlight-Overlays auf der Zeitleiste hinzufügen.",
        "Exportieren — bei reinem Kürzen greift ein verlustfreier Schnellpfad, sonst wird der ganze Clip einmal neu kodiert.",
      ],
      whatHappens:
        "Bis zum Export wird nichts verarbeitet: Die Live-Vorschau spielt die Bearbeitungen direkt ab. Filmstreifen und Wellenform geben eine zoombare Übersicht über den ganzen Clip für bildgenaue Navigation. Die Vorschau selbst zoomt außerdem mit dem Mausrad (bis 8×) und lässt sich durch Ziehen verschieben — mit der mittleren Maustaste immer, mit der linken sobald hineingezoomt ist —, um ein kleines Overlay bei einer großen Aufnahme genau zu platzieren. Ein paar fertige Export-Voreinstellungen (unveränderte Qualität, kleiner für Chat, animiertes GIF) merken sich die letzte Wahl. Neben MP4, WebM und GIF bietet die Formatauswahl beim Export auch AVI, MOV, MKV, animiertes WebP und APNG, jeweils mit einem kurzen Hinweis, wofür es gedacht ist; GIF, WebP und APNG haben nie Ton, warnen ab zehn Sekunden Länge und sind auf eine Höchstauflösung begrenzt, da alle drei schnell groß werden. Rückgängig/Wiederholen deckt jeden Bearbeitungsschritt ab, begrenzt durch ein einstellbares Limit.",
      settings: ["Video-Editor, Rückgängig-Limit — Experten-Menü → Funktionen → Prompts erstellen"],
    },
    {
      section: "basics",
      title: "Texterkennung (OCR)",
      whatItIs: "Liest mit der geräteeigenen Erkennung von Windows Text aus einem Screenshot oder einer Bilddatei — vollständig offline, nichts verlässt das Gerät.",
      howTo: [
        "Über die Eingabeleiste — der OCR-Button bietet einen Screenshot oder eine vorhandene Bilddatei an.",
        "Bei einem bereits aufgenommenen Screenshot — über den Button „Text erkennen“ im Aufnahme-Dialog.",
        "Das Ergebnis prüfen und anschließend kopieren oder direkt einfügen.",
      ],
      whatHappens:
        "Die Erkennung läuft vollständig auf dem Gerät über die eigene Erkennungs-Engine von Windows. Der Ergebnisdialog kann den Text automatisch kopieren und getrennte Zeilen wieder zu Absätzen verbinden; beides lässt sich separat ein- und ausschalten. Fehlt ein Sprachpaket, öffnet sich derselbe Dialog wie bei der Spracheingabe — siehe „Sprachpakete“ unten.",
      settings: ["Texterkennung, Ergebnis automatisch kopieren, Zeilen verbinden, kleine Bilder hochskalieren, Erkennungssprache — Experten-Menü → Funktionen → Prompts erstellen"],
    },
    {
      section: "basics",
      title: "Spracheingabe",
      whatItIs: "Der Mikrofon-Button verwandelt Sprache in Text — die Erkennung läuft komplett auf dem Gerät.",
      howTo: [
        "Auf das Mikrofon-Icon neben der Eingabeleiste klicken, oder im Editor einer Kachel.",
        "Sprechen; der Text erscheint, während er erkannt wird.",
        "Erneut klicken, Escape drücken, oder speichern/schließen, um zu stoppen.",
      ],
      whatHappens:
        "Die Erkennung läuft lokal über die eigene Spracherkennung von Windows — es verlässt nichts das Gerät, außer die Online-Erkennung wird ausdrücklich erlaubt. Gesprochene Satzzeichen-Wörter (\"Komma\", \"neue Zeile\", …) werden zu echten Satzzeichen; \"wörtlich\" vor einem Wort tippt es statt es als Befehl auszulösen. Ein fehlendes Sprachpaket öffnet direkt den Sprachpaket-Dialog, gemeinsam mit der Texterkennung (OCR). Ein optionaler globaler Kurzbefehl startet und stoppt eine Aufnahme, ohne das Fenster zu öffnen — der Text landet in der Zwischenablage, wird eingefügt, oder geht direkt in die Eingabeleiste; das Taskleisten-Symbol zeigt währenddessen \"Nimmt auf …\" im Tooltip.",
      settings: ["Spracheingabe, Satzzeichen-Befehle, automatische Großschreibung, Stille-Auto-Stopp, Einfügeposition, globaler Hotkey — Experten-Menü → Funktionen → Prompts erstellen"],
    },
    {
      section: "basics",
      title: "Sprachpakete",
      whatItIs: "Texterkennung und Spracheingabe verwenden dieselben Windows-Sprachpakete und denselben Installationsdialog.",
      howTo: [
        "Eine Sprache im Dropdown für die Erkennungssprache auswählen; fehlt das Paket, öffnet sich der Installationsdialog automatisch.",
        "Zum Installieren eines Pakets sind Administratorrechte erforderlich — Windows fragt einmal über den eigenen Bestätigungsdialog danach.",
        "„Sprachen verwalten…“ am unteren Rand des Sprach-Dropdowns öffnet denselben Dialog jederzeit erneut.",
      ],
      whatHappens:
        "Bei manchen Windows-Versionen sind selbst zum Auflisten der installierten Pakete Administratorrechte erforderlich. In diesem Fall führt ein Button die Prüfung über dieselbe Administratorabfrage erneut aus. Eine Hintergrundüberwachung erkennt, wenn sich die Version eines installierten Pakets geändert hat oder ein Windows-Update es unbemerkt entfernt hat, und kennzeichnet es, ohne sofort zur Neuinstallation aufzufordern. Mit „Später erinnern“ lässt sich der Hinweis vorübergehend ausblenden; die Überwachung kann auch vollständig deaktiviert werden.",
      settings: ["Erkennungssprache automatisch wählen, Hintergrundüberwachung für fehlende/geänderte Pakete — Experten-Menü → Funktionen → Prompts erstellen (ein Paar unter Texterkennung, eines unter Spracheingabe)"],
    },
    {
      section: "basics",
      title: "Farbpipette & Auswahlverlauf",
      whatItIs: "Die Farbpipette liest jeden Bildschirmpixel als Hex, RGB oder HSL aus.",
      howTo: [
        "Auf das Pipetten-Icon in der Eingabeleiste klicken.",
        "Die Lupe zum gewünschten Pixel bewegen; scrollen oder Plus/Minus zum Zoomen nutzen.",
        "Klick kopiert die Farbe; Rechtsklick auf das Pipetten-Icon öffnet die letzten paar Auswahlen erneut.",
      ],
      whatHappens:
        "Es wird nie automatisch etwas in einer Kachel gespeichert — eine Auswahl landet nur in der Zwischenablage. Der Auswahlverlauf hält die letzten paar Farben und übersteht einen Neustart, begrenzt durch eine eigene Einstellung.",
      settings: ["Farbpipette, Zoom, Format, Länge des Auswahlverlaufs — Experten-Menü → Funktionen → Prompts erstellen"],
      tip: "Farbformat — Hex, RGB oder HSL — im Hinweis wählen, der direkt nach der Auswahl erscheint.",
    },
    {
      section: "basics",
      title: "Wechselnde Kacheln",
      whatItIs: "Eine Kachel kann eine Liste von Texten statt nur einem tragen; jede Kopie liefert den nächsten Eintrag.",
      howTo: [
        "Wechselnde Kacheln sind standardmäßig an — die Funktion im Experten-Menü ausschalten, falls nie genutzt.",
        "Im Editor einer Kachel Erweiterte Optionen öffnen und \"Wechselnde Texte\" einschalten.",
        "Die Eintragsliste ersetzt dann das normale Textfeld.",
        "Mit \"+ Eintrag\" weitere Texteinträge hinzufügen; Icons daneben sortieren um oder entfernen sie.",
        "Reihenfolge (der Reihe nach) oder Mischen (zufällig, ohne Wiederholung) wählen.",
        "Die Kachel wie gewohnt kopieren — sie liefert immer den nächsten Eintrag, ohne zu fragen, welchen.",
      ],
      whatHappens:
        "Ein kleines Badge zählt auf einer Raster-Kachel, einer schwebenden Pille und einem Pillen-Ordner-Mitglied hoch, wie \"2/5\" — ein Raster-Ordner-Mitglied zeigt nur sein Dateityp-Icon, nicht dieses Badge. Im Reihenfolge-Modus ist das die absolute Position; im Zufalls-Modus zählt es die Ziehungen im aktuellen Durchlauf und beginnt wieder bei 1, sobald jeder Eintrag einmal drankam, statt auf eine beliebige Position zu springen. Der Zählerstand übersteht Neustarts und vollständige Backups; ein CSV/TXT-Import erstellt eine neue Kachel mit frischem Zeiger, sodass nur die Textliste und ihr Reihenfolge- oder Misch-Modus übernommen werden, nicht die genaue Position. Wird die Rotation bei einer Kachel mit mehreren Einträgen ausgeschaltet, bleiben alle erhalten — nur der erste wird genutzt, bis sie wieder aktiviert wird.",
      settings: ["Wechselnde Kacheln (standardmäßig an), Standard Reihenfolge oder Mischen, Badge, maximale Einträge — Experten-Menü → Funktionen → Prompts erstellen. Der Schalter pro Kachel sitzt im Editor unter Erweiterten Optionen."],
    },
    {
      section: "system",
      title: "Das Experten-Menü",
      whatItIs: "Das Experten-Menü hält jede optionale Funktion hinter einem eigenen Schalter mit ihren Parametern.",
      howTo: [
        "Einstellungen, dann das Experten-Menü öffnen.",
        "Einen Tab wählen, oder im Suchfeld tippen, um alle Tabs auf einmal zu durchsuchen.",
        "\"Nur geänderte\" einschalten, um nur das zu sehen, was an den Standardwerten angepasst wurde.",
      ],
      whatHappens:
        "Eine standardmäßig deaktivierte Funktion trägt ein kleines Punkt-Badge an ihrer Beschriftung, mit einem erklärenden Tooltip. Zurücksetzen funktioniert auf drei Ebenen: das eigene Rückgängig-Icon eines Werts, das Zurücksetzen-Icon einer ganzen Gruppe in ihrer Kopfzeile, oder alles auf einmal über die allgemeine Zurücksetzen-Aktion.",
      settings: [],
    },
    {
      section: "system",
      title: "Tastaturnavigation",
      whatItIs: "Pfeiltasten bewegen einen Fokusring über das Raster für rein tastaturbasiertes Kopieren.",
      howTo: [
        "Die Tastaturnavigation ist ab Werk an; abschaltbar in Experten-Menü → System & Daten.",
        "Die Pfeiltasten bewegen den Fokusring, Enter oder Leertaste kopiert.",
        "Tab springt wie gewohnt weiterhin durch Buttons und Felder überall sonst in der App.",
      ],
      whatHappens: "Der Fokusring blendet sich kurz nach Fokusverlust des Fensters aus, damit er nicht auf einer Kachel verweilt, sobald eine andere App genutzt wird.",
      settings: ["Tastaturnavigation, Ausblend-Verzögerung — Experten-Menü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Wo die Daten liegen",
      whatItIs:
        "Alles liegt in einer einzigen lokalen SQLite-Datenbank plus ihrer kleinen Begleitdatei.",
      howTo: [
        "Installiert: der Daten-Ordner liegt unter AppData, im Experten-Menü einsehbar und änderbar.",
        "Portabel: der Daten-Ordner liegt direkt neben der exe, sodass alles auf einen USB-Stick passt.",
      ],
      whatHappens: "Ein Wechsel von portabel zu installiert, oder zurück, wird automatisch erkannt und als einmalige Migration angeboten — bestehende Daten werden nie stillschweigend zurückgelassen.",
      settings: ["Daten-Ordner — Experten-Menü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Updates & Sicherheit",
      whatItIs: "Eine optionale Prüfung sucht nach einer neueren Version und kommuniziert nur mit dem Update-Server.",
      howTo: [
        "Update-Checks eingeschaltet lassen, der Standard, für eine Benachrichtigung bei neuer Version.",
        "Sie im Experten-Menü ausschalten, falls unerwünscht.",
        "Ein angebotenes Update annehmen, um es mit einem Klick herunterzuladen und zu installieren.",
      ],
      whatHappens:
        "Die Update-Prüfung ist zusammen mit einem Download des Microsoft-WebView2-Installers beim ersten Start, falls diese Laufzeit fehlt, eine von nur zwei Arten von Netzwerkverkehr, den die App von sich aus macht. Ein heruntergeladenes Update wird vor der Ausführung anhand einer veröffentlichten Prüfsumme verifiziert — eine fehlende oder falsche Prüfsumme blockiert die Installation, statt eine unbestätigte Datei auszuführen. Die dritte und letzte Art ist ffmpeg, für Videoexport/-konvertierung: Eine installierte Version bringt es mit und greift dafür nie auf das Netzwerk zu, eine portable Version bietet einen einmaligen Download nur an, wenn eine Videofunktion es tatsächlich braucht, und nur mit ausdrücklicher Zustimmung — nie automatisch, nie im Hintergrund.",
      settings: ["Update-Checks — Experten-Menü → System & Daten → System"],
    },
    {
      section: "system",
      title: "Häufige Fragen",
      whatItIs: "Dieser Abschnitt beantwortet die am häufigsten auftauchenden Fragen.",
      howTo: [
        "\"Mein Screenshot ist leer\" — der Bildschirmaufnahme-Ausschluss funktioniert wie vorgesehen; bei Bedarf im Experten-Menü ausschalten.",
        "\"Die App startet nicht\" — sie benötigt die Microsoft-WebView2-Laufzeit, vom Installer automatisch angeboten.",
        "\"Ich finde eine Einstellung nicht\" — das Suchfeld im Experten-Menü durchsucht alle Tabs auf einmal.",
        "\"Ein Hotkey bewirkt nichts\" — eine andere App hält diese Kombination möglicherweise bereits; der Editor erklärt, warum.",
      ],
      settings: [],
    },
  ],
};
