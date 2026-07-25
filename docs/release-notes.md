## Prompt Saver 2.6.2

Backups get their own window, their own encryption and a lot less disk space. The
expert menu was re-sorted from scratch, the portable version is now a single file,
and a long list of rough edges is gone.

Coming from 2.4.1? This build also brings everything from 2.5.0: a custom title bar,
version history per prompt, batch editing and favorites in the library, a duplicate
finder, fuzzy search, smart sorting, auto-paste and the clipboard inbox.

### Downloads

| File | Use it when |
|---|---|
| `Prompt.Saver_2.6.2_x64-setup.exe` | Installer — choose **all users** or **current user only**; upgrades replace the previous version automatically, your data is kept |
| `prompt-saver.exe` | Portable standalone version — one file, no installation, keeps its data next to itself |

Requires the Microsoft WebView2 runtime (preinstalled on Windows 11 and current Windows 10). If it is missing, the app offers the official Microsoft installer on first start.

### Highlights

- **Backups window** — the Backups button in the settings opens a page of its own: interval and retention, diagnostics, the restore list and the usage statistics.
- **Encrypted, compressed backups** — exports are AES-256-GCM encrypted and take a fraction of the space they used to. An optional password protects new backups; older backups keep whatever protected them and still restore.
- **Single-file portable version** — the PDF preview library is built into the exe, so nothing has to travel next to it. Starting it beside an existing installation offers to take those prompts along.
- **Installer runs through in one pass** — choosing "for all users" no longer makes it restart itself halfway through.
- **Expert menu re-sorted** — clearer groups, and 77 settings whose name alone does not explain them now carry a tooltip, in all 20 languages.
- **Split reset buttons** — delete the data or reset the settings, not both at once, optionally with a safety backup first.

### Fixed

- The version history in the editor is reachable on small windows and only as tall as its contents.
- No more flicker when changing a setting, taking a backup or collapsing a category.
- Resetting the settings no longer discards the backup password.
- Prompts with variables follow the same copy/paste rule as every other prompt.
- The window buttons no longer show tooltips; close no longer turns red on hover.

The complete list is in the [changelog](../../blob/master/CHANGELOG.md).
