## Prompt Saver 2.7.0

Fourteen new features: tags, custom themes, folders for view tabs and for
floating buttons, self-filling and choice variables, a markdown preview with
syntax colors, rotating prompts, private notes, per-prompt hotkeys, drag-out
into other apps, a tray quick-copy menu and a screen color picker. Everything
new is an expert-menu switch, most of it off until you turn it on.

### Downloads

| File | Use it when |
|---|---|
| `Prompt.Saver_2.7.0_x64-setup.exe` | Installer — choose **all users** or **current user only**; upgrades replace the previous version automatically, your data is kept |
| `prompt-saver.exe` | Portable standalone version — one file, no installation, keeps its data next to itself |

Requires the Microsoft WebView2 runtime (preinstalled on Windows 11 and current Windows 10). If it is missing, the app offers the official Microsoft installer on first start.

### Organise

- **Tags** — label prompts with free-form, colored tags. Type-ahead chips in the editor, a multi-select filter row in the library that matches all or any of the picked tags and stacks with the search, and a manager to rename, recolor or delete a tag everywhere at once.
- **View groups** — drag one view tab onto another and they become a folder tab with a popover listing its views. Rename, recolor or dissolve it from its context menu; Ctrl+Tab cycles through views in visual order.
- **Private notes** — a collapsible notes row per prompt for context you keep but never copy. Shown dimmed in the tile tooltip, found by the search, never on the clipboard.

### Write

- **Self-filling variables** — `date`, `time`, `datetime`, `weekday`, `clipboard` and `uuid` fill themselves in on copy with no dialog, formatted in the app language. A prompt that mixes them with your own variables only asks for yours.
- **Choice variables** — `{#{tone|formal,casual,witty}#}` turns into a dropdown in the copy dialog and remembers your last pick.
- **Markdown preview** — the editor shows the prompt rendered: headings, lists, quotes, emphasis and code blocks. One field, not two: the rendered view stands where the text field was and hands it back the moment you click into it, in the same box at the same size. Code is syntax-colored for JavaScript, TypeScript, Python, HTML, CSS, JSON, SQL, Bash and Rust.
- **Rotating prompts** — a text prompt can hold a list of texts instead of one; each copy serves the next entry, in order or shuffled, with a small badge on the tile showing the position. Entries travel with export/import, backups, Markdown-sync and version history, and the search finds a match inside any of them.
- The fill-in dialog gets a "Clear all" button, and saving a prompt with an empty `{#{ }#}` placeholder now warns first instead of leaving it stuck in the copied text.

### Copy from anywhere

- **Per-prompt hotkeys** — give a prompt its own global key combination that copies it from any app, even with Prompt Saver in the tray, and optionally pastes it straight away.
- **Tray quick menu** — the tray menu can list your recent and favorite prompts; one click copies without opening the window.
- **Drag-out** — hold Ctrl and drag a tile, library row or floating button into another app to drop its text there.
- **Floating-button folders** — drag one floating button onto another and they merge into a folder showing up to four previews and a count. Click to open it in place, drag a member out to free it, and it dissolves when one is left.

### Also in this release

- **Grid folders** — drop one tile onto another and both move into a folder in that cell; drag one out and it goes back to the layout.
- **Allow multiple instances** (expert menu, off by default) — run the app more than once at the same time, e.g. against two different data folders. Off keeps the familiar single-instance behaviour. Takes a restart to apply.
- The backups window can open its snapshot folder in Explorer directly.
- Dragging a button, tab or tile onto another shows what would happen: the target dims behind a folder mark in the accent colour — the same gesture in all three places.
- Dropdowns and menus flip above their anchor when there is no room below, and never open past the edge of the window.
- The copy history reads like the library — same rows, tags, a type chip and a quiet timestamp.
- The favourites section of the tray menu is on out of the box.
- The privacy switch that keeps the window out of screenshots is now called **Allow screenshots of Prompt Saver** and reads from your side. It ships off, so the window is still left out of screen captures — but if your screenshots come out empty, the switch that explains it is under privacy in the expert menu.
- A backup is either whole or absent: a failed one leaves nothing behind.
- "Duplicate" on a prompt now opens the create dialog prefilled with everything from the original, instead of a silent copy — change something first, or cancel and leave nothing behind.

### Make it yours

- **Custom themes** — build your own on top of any of the 16 built-ins: a color input per themable value, grouped and previewed live, saved into the theme dropdown. The floating buttons follow along.
- **Color picker** — a pipette beside the screenshot button freezes the screen and shows a magnified pixel grid with a live readout. Click to copy the color as hex, rgb or hsl; nothing is ever saved.

The complete list is in the [changelog](../../blob/master/CHANGELOG.md).
