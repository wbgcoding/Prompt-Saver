# Changelog

All notable changes to **Prompt Saver**. Download the latest version from the
[releases page](../../releases/latest).

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
