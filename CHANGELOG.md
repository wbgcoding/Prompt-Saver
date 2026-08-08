# Changelog

All notable changes to **Prompt Saver**. Download the latest version from the
[releases page](../../releases/latest).


## 2.7.0 (unreleased)

Eighteen new features: tags, custom themes, folders for view tabs and for
floating buttons, self-filling and choice variables, a markdown preview with
syntax colors, private notes, per-prompt hotkeys, drag-out into other apps, a
tray quick-copy menu, a screen color picker, a standalone quick launcher,
multi-select in the grid, a trash for deleted prompts, placing one prompt in
several views at once, and restoring a backup's data and settings separately.
Everything new is an expert-menu switch, most of it off until you turn it on.

### Added

- **Tags** — label prompts with free-form, colored tags. The editor gets a chip
  row with type-ahead over existing tags and a per-chip color popover; the
  library gets a filter row above the results (multi-select, switchable between
  matching all or any of the picked tags) that combines with search and the
  other filters, plus a small manager to rename, recolor or delete a tag
  everywhere at once, with a usage count per tag. Tags travel with TXT and CSV
  exports.
- **Custom themes** — build your own theme on top of any of the 16 built-in
  ones: a palette button beside the theme dropdown opens an editor with a color
  input per themable value, grouped by background, surfaces, text and accent,
  with a live preview while you edit. Saved themes appear in the dropdown under
  a separator, and the floating buttons follow them too.
- **View groups** — collect view tabs into a folder tab. Drag one tab onto
  another to group them; the group tab opens a popover listing its views, and
  its context menu renames, recolors or dissolves it. Ctrl+Tab cycles through
  views in visual order.
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
  in the editor.
- **Drag-out** — hold Ctrl and drag a tile, a library row or a floating button
  into any other app to drop its text there.
- **Tray quick menu** — the tray icon's menu can list your recently used and
  your favorite prompts; clicking one copies it without opening the window.
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
  without touching your current setup, or the other way around.

### Improved

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

### Fixed

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

### Changed

- The product name now lives in one generated file per layer instead of being
  spelled out across the code, the interface and the build scripts. Nothing about
  the app changes: it keeps the same data folder, autostart entry and update
  source, and a unit test pins every one of those values.

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
