# Prompt Saver

A tiny, fully offline Windows tool for storing your favorite prompts and copying them to the clipboard with a single click — from a customizable grid or from floating always-on-top buttons that work over any application.

![Prompt Saver](docs/screenshot.svg)

## Features

- **Prompt grid** – free placement on a per-view grid (1×1 up to 20×20, default 5×4), drag tiles anywhere, swap by dropping; layouts are remembered per grid size
- **One-click copy** – click a tile and the prompt text is in your clipboard ("Copied!" bubble confirms)
- **Multiple views** – up to 20 named pages, each with its own grid size and layout
- **Floating buttons** – pin any prompt as a frameless, transparent, always-on-top pill; click to copy from anywhere, drag to reposition, right-click for size / edit / remove; positions survive restarts
- **Per-prompt colors** – tint tiles and floating pills from a color palette
- **Auto-fit text** – tile text grows to fill the button (or pick a fixed size and one of 10 fonts)
- **10 languages** – auto-detected from the system (EN fallback): EN, DE, ES, FR, IT, PT, PL, RU, ZH, JA
- **Import / export** – CSV or TXT including views, layouts, colors and language; re-import restores everything
- **Runs in the background** – optional minimize-to-tray on close, autostart at login (optionally minimized)
- **Light / dark / system theme** with live OS detection
- **100% local** – data stored as JSON in `%APPDATA%`, no network, no telemetry
- **Tiny** – ~3 MB exe (Tauri / Rust), uses the Windows WebView2 runtime

## Download

From the [latest release](../../releases/latest):

| File | What it is |
|---|---|
| `Prompt.Saver_x64-setup.exe` | **Installer** – installs for the current user, with desktop / start menu shortcuts and "run after install" |
| `prompt-saver.exe` | **Portable** – single standalone exe, no installation |

Both need the Microsoft WebView2 runtime (preinstalled on Windows 11 and current Windows 10; the app offers the official installer automatically if it is missing).

## Usage

| Action | How |
|---|---|
| Save a prompt | Type it in the input line → **Save** → name + color (Ctrl+Enter works too) |
| Copy a prompt | Click its tile ("Copied!" bubble) |
| Move a tile | Drag it to any grid cell; drop on an occupied cell to swap |
| Edit / hide / pin / delete | Right-click a tile (or hover **⋮**) |
| Floating button | "Toggle floating button" in the tile menu; right-click the pill for options |
| Views | Buttons next to the title; manage them in the settings |
| Settings | Gear icon: theme, language, fonts, views & grid sizes, background behaviour, autostart, import/export, reset |

## Building from source

Requirements: [Node.js](https://nodejs.org), [Rust](https://rustup.rs) (MSVC toolchain).

```bat
build.bat
```

or manually:

```sh
npm install
npm run build              # portable exe + NSIS installer
# -> src-tauri/target/release/prompt-saver.exe
# -> src-tauri/target/release/bundle/nsis/Prompt Saver_<version>_x64-setup.exe
```

## Tech stack

- [Tauri 2](https://tauri.app) (Rust backend, WebView2 frontend)
- Vanilla HTML/CSS/JS — no bundler, no framework
- `arboard` (clipboard), `rfd` (native dialogs), `winreg` (autostart), `sys-locale` (language detection)

## License

MIT
