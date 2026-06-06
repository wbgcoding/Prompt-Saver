## Prompt Saver 1.6.1

### Downloads

| File | Use it when |
|---|---|
| `Prompt.Saver_1.6.1_x64-setup.exe` | Installer — choose **all users** (admin) or **current user only**, with desktop / start menu shortcuts and "run after install" (all pre-selected, all optional) |
| `prompt-saver.exe` | Portable standalone version — one file, no installation |

Requires the Microsoft WebView2 runtime (preinstalled on Windows 11 and current Windows 10). If it is missing, the app offers the official Microsoft installer on first start.

### Fixed
- Installing an update via the notification or the settings no longer fails — the silent installer starts reliably and the app restarts itself
- The update check runs right after launch (was: 30 seconds later)
- No more white flash at the window edges while resizing
- "You're up to date" in the settings makes way for the version number after a few seconds

Full history: see [CHANGELOG.md](../../blob/master/CHANGELOG.md)
