# Third-Party Notices

Prompt Saver is built with open-source components. This file acknowledges them and
their licenses. All bundled third-party code is distributed under permissive
open-source licenses (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, Unicode, BSL-1.0,
Unlicense/CC0) that allow redistribution in a binary with attribution, with one
exception noted below: ffmpeg is GPL-3.0, run as a separate subprocess rather than
linked into the app.

## Bundled binaries

- **PDFium** — © The PDFium Authors, licensed under **BSD-3-Clause**. Compiled into
  the executable and unpacked to a temporary folder when a PDF preview is first
  rendered. Source and full license text:
  https://github.com/bblanchon/pdfium-binaries
- **SQLite** (bundled via the `rusqlite` crate) — in the **public domain**.
  https://www.sqlite.org/copyright.html
- **ffmpeg** — © the FFmpeg developers, licensed under **GPL-3.0-or-later** (the
  gyan.dev "essentials" build, chosen because it includes the libx264/libx265
  encoders MP4/H.264 export needs; the LGPL alternative has no H.264 encoder at
  all). ffmpeg runs as a separate, independently invoked subprocess — never
  linked into this executable — so its own license governs it, not the app's
  code. The installer ships `ffmpeg.exe` directly; the portable build fetches it
  on first need, only with explicit consent, from the pinned release
  https://github.com/GyanD/codexffmpeg/releases/tag/9.0.1. Source and full
  license text: https://github.com/GyanD/codexffmpeg and
  https://www.gnu.org/licenses/gpl-3.0.html

## Runtime (not bundled)

- **Microsoft Edge WebView2 Runtime** — provided by Microsoft under the Microsoft
  Software License Terms. Prompt Saver uses the runtime already installed on Windows
  (or offers Microsoft's official installer); it is not redistributed by this project.

## Rust crates

The application links ~660 Rust crates (direct and transitive). Direct dependencies:

`tauri`, `tauri-plugin-single-instance`, `tauri-plugin-global-shortcut`, `tokio`,
`serde`, `serde_json`, `arboard`, `clipboard-win`, `rfd`, `sys-locale`, `image`,
`ureq`, `sha2`, `aes-gcm`, `argon2`, `rand`, `flate2`, `xcap`, `rayon`,
`jpeg-encoder`, `rusqlite`, `pdfium-render`, `zip`, `winreg`, `windows`,
`windows-capture`.

Aggregate license breakdown of the full dependency tree (660 packages, from
`cargo metadata`):

| Count | License (SPDX) |
|------:|----------------|
| 305 | MIT OR Apache-2.0 |
| 151 | MIT |
| 59 | Apache-2.0 OR MIT |
| 27 | MIT/Apache-2.0 |
| 18 | Unicode-3.0 |
| 17 | Zlib OR Apache-2.0 OR MIT |
| 10 | Unlicense OR MIT |
| 7 | BSD-3-Clause |
| 6 | Apache-2.0/MIT · MIT OR Apache-2.0 OR Zlib |
| 5 | Apache-2.0 · ISC · MPL-2.0 · Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| 3 | BSD-2-Clause |
| 2 | BSL-1.0 · CDLA-Permissive-2.0 · Unlicense/MIT · MIT OR Apache-2.0 OR LGPL-2.1-or-later · various BSD/Apache/MIT combinations |
| 1 | 0BSD · Zlib · NCSA, IJG and CC0-1.0 combinations · other permissive combinations |

Every license above is permissive or weak-copyleft (MPL-2.0 is file-level and no MPL
files are modified; the two crates offering LGPL-2.1-or-later as an alternative are
used under their MIT/Apache-2.0 option). The complete per-crate copyright notices and full license texts
are available from each crate's page on https://crates.io and its source repository.

A machine-generated bundle containing every dependency's full license text can be
produced with [`cargo-about`](https://github.com/EmbarkStudios/cargo-about)
(`cargo about generate about.hbs`) if an exhaustive `NOTICES` file is required.
