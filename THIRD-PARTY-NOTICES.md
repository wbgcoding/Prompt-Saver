# Third-Party Notices

Prompt Saver is built with open-source components. This file acknowledges them and
their licenses. All bundled third-party code is distributed under permissive
open-source licenses (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, Unicode, BSL-1.0,
Unlicense/CC0) that allow redistribution in a binary with attribution.

## Bundled binaries

- **PDFium**: © The PDFium Authors, licensed under **BSD-3-Clause**. Compiled into
  the executable and unpacked to a temporary folder when a PDF preview is first
  rendered. Source and full license text:
  https://github.com/bblanchon/pdfium-binaries
- **SQLite** (bundled via the `rusqlite` crate), in the **public domain**.
  https://www.sqlite.org/copyright.html

## Runtime (not bundled)

- **Microsoft Edge WebView2 Runtime**: provided by Microsoft under the Microsoft
  Software License Terms. Prompt Saver uses the runtime already installed on Windows
  (or offers Microsoft's official installer); it is not redistributed by this project.

## Rust crates

The application links ~640 Rust crates (direct and transitive). Direct dependencies:

`tauri`, `tauri-plugin-single-instance`, `tauri-plugin-global-shortcut`, `tokio`,
`serde`, `serde_json`, `arboard`, `clipboard-win`, `rfd`, `sys-locale`, `image`,
`ureq`, `sha2`, `aes-gcm`, `argon2`, `rand`, `flate2`, `xcap`, `rayon`,
`jpeg-encoder`, `rusqlite`, `pdfium-render`, `winreg`.

Aggregate license breakdown of the full dependency tree (644 packages, from
`cargo metadata`):

| Count | License (SPDX) |
|------:|----------------|
| 294 | MIT OR Apache-2.0 |
| 144 | MIT |
| 57 | Apache-2.0 OR MIT |
| 29 | MIT/Apache-2.0 |
| 18 | Unicode-3.0 |
| 17 | Zlib OR Apache-2.0 OR MIT |
| 15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| 7 | BSD-3-Clause |
| 6 | MIT OR Apache-2.0 OR Zlib · Apache-2.0/MIT |
| 5 | Unlicense OR MIT · MPL-2.0 |
| 4 | ISC · Apache-2.0 |
| 3 | BSD-2-Clause |
| 2 | CDLA-Permissive-2.0 · BSL-1.0 · Zlib · Unlicense/MIT · various BSD/Apache/MIT combinations |
| 1 | 0BSD · NCSA, IJG and CC0-1.0 combinations · other permissive combinations |

Every license above is permissive or weak-copyleft (MPL-2.0 is file-level and no MPL
files are modified; the two crates offering LGPL-2.1-or-later as an alternative are
used under their MIT/Apache-2.0 option). The complete per-crate copyright notices and full license texts
are available from each crate's page on https://crates.io and its source repository.

A machine-generated bundle containing every dependency's full license text can be
produced with [`cargo-about`](https://github.com/EmbarkStudios/cargo-about)
(`cargo about generate about.hbs`) if an exhaustive `NOTICES` file is required.
