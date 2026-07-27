## Prompt Saver 2.7.0

Thirteen new features: tags, custom themes, folders for view tabs and for
floating buttons, self-filling and choice variables, a markdown preview with
syntax colors, private notes, per-prompt hotkeys, drag-out into other apps, a
tray quick-copy menu and a screen color picker. Everything new is an expert-menu
switch, most of it off until you turn it on.

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
- **Markdown preview** — an eye button renders the prompt with headings, lists, quotes, emphasis and code blocks, toggled or side by side. Code is syntax-colored for JavaScript, TypeScript, Python, HTML, CSS, JSON, SQL, Bash and Rust.

### Copy from anywhere

- **Per-prompt hotkeys** — give a prompt its own global key combination that copies it from any app, even with Prompt Saver in the tray, and optionally pastes it straight away.
- **Tray quick menu** — the tray menu can list your recent and favorite prompts; one click copies without opening the window.
- **Drag-out** — hold Ctrl and drag a tile, library row or floating button into another app to drop its text there.
- **Floating-button folders** — drag one floating button onto another and they merge into a folder showing up to four previews and a count. Click to open it in place, drag a member out to free it, and it dissolves when one is left.

### Make it yours

- **Custom themes** — build your own on top of any of the 16 built-ins: a color input per themable value, grouped and previewed live, saved into the theme dropdown. The floating buttons follow along.
- **Color picker** — a pipette beside the screenshot button freezes the screen and shows a magnified pixel grid with a live readout. Click to copy the color as hex, rgb or hsl; nothing is ever saved.

The complete list is in the [changelog](../../blob/master/CHANGELOG.md).
