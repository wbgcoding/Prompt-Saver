## Prompt Saver 1.4.0

### Downloads

| File | Use it when |
|---|---|
| `Prompt.Saver_1.4.0_x64-setup.exe` | Installer — choose **all users** (admin) or **current user only**, with desktop / start menu shortcuts and "run after install" (all pre-selected, all optional) |
| `prompt-saver.exe` | Portable standalone version — one file, no installation |

Requires the Microsoft WebView2 runtime (preinstalled on Windows 11 and current Windows 10). If it is missing, the app offers the official Microsoft installer on first start.

### Added
- **Image prompts**: the image button next to the input saves a picture from the clipboard (or via file dialog) — clicking the tile copies the image, ready to paste anywhere
- **Icon images**: any text prompt can show a picture on its tile instead of the name (the click still copies the text)
- Tiles and floating buttons display images edge to edge; the chosen color frames grid tiles, floating image buttons are borderless square boxes with much larger S / M / L sizes
- High-quality scaling (up to 1024 px, Lanczos) keeps images sharp

### Fixed
- The window appears only after the first fully sized layout — no more visible text resizing on startup
- The "Copied!" overlay on floating image buttons matches the visible image

Full history: see [CHANGELOG.md](../../blob/master/CHANGELOG.md)
