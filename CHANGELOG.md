# Changelog

All notable changes to **Prompt Saver**. Download the latest version from the
[releases page](../../releases/latest).

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
