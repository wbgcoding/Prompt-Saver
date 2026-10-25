# Contributing

Thanks for helping to make Prompt Saver better. Bug reports and ideas are welcome, and so are
pull requests.

## Before you start

- **Bugs and ideas:** open an issue with the matching template first, so we can agree on the
  approach before you spend time on code.
- **Security problems:** never in a public issue, see [SECURITY.md](SECURITY.md).

## Build and test

You need Windows 10 or 11, Node.js, the Rust MSVC toolchain at the `rust-version` in
`src-tauri/Cargo.toml`, and `src-tauri/pdfium.dll` from
[pdfium-binaries](https://github.com/bblanchon/pdfium-binaries).

```bat
npm ci
npm run dev
cd src-tauri && cargo test --lib
```

All test suites and their commands are listed in `tests/README.md`. `build.bat` builds the
portable exe and the installer into `dist\`.

## Guidelines

- **Offline by design.** No new network access, no telemetry, no cloud sync.
- **Shared modules first.** Popovers, dialogs, themes and translations each have one shared module;
  reuse it instead of writing a second one.
- **Tests with every change.** New behaviour comes with a test; a bug fix comes with a test that
  failed before the fix. `tests/source_hygiene.py` must stay green.
- **Every language.** New user-facing text goes into every file under `ui/i18n/`.
- **One topic per pull request,** with a short description of what changed and how you tested it.
  Screenshots help for anything visible.

## License

Prompt Saver is not open source in the usual sense; see [LICENSE](LICENSE). By contributing you
agree that your contribution is published under the same terms.
