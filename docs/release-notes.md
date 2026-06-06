## Prompt Saver 1.8.0

### Downloads

| File | Use it when |
|---|---|
| `Prompt.Saver_1.8.0_x64-setup.exe` | Installer — choose **all users** (admin) or **current user only**; upgrades replace the previous version automatically, your data is kept |
| `prompt-saver.exe` | Portable standalone version — one file, no installation |

Requires the Microsoft WebView2 runtime (preinstalled on Windows 11 and current Windows 10). If it is missing, the app offers the official Microsoft installer on first start.

### Added
- **Video player on floating buttons**: hover the lower edge of a video pill for play/pause, scrubber, time, loop toggle and sound — same controls as on grid tiles
- **Volume slider**: hovering the sound button opens a vertical slider (grid tiles and floating buttons)
- **Saved player state**: volume, mute and the loop / play-once choice are remembered per prompt and restored on the next start
- **Close button** (X) in the floating button's right-click menu
- The WebView2 setup dialog on first start now speaks all 20 languages

### Improved
- Images and videos fill the floating button **exactly** — every pixel visible, no cropping; S/M/L scales the whole button
- Dragging a video button or a video tile no longer stutters
- Tile text keeps its size when the window moves between monitors with different display scaling
- The floating button's right-click menu is more compact
- Screen reader labels follow the chosen language

### Fixed
- The image on a floating button could appear wrong or not at all
- A frozen video on a floating button now recovers by itself
- Saving a prompt shows an error message if it fails instead of failing silently

Full history: see [CHANGELOG.md](../../blob/master/CHANGELOG.md)
