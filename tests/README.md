# Test suites

Backend: `cargo test --lib`. JS syntax: `node --check <file>`. Everything below runs
from the project root. The browser-driven suites skip cleanly when no Chrome/Edge is
installed. Run the whole list after any UI change.

`cargo test` and `cargo clippy` create `src-tauri/target/debug/` (~4 GB). Delete it as
soon as the run has been evaluated; only `target/release/` ships.

The Rust test process runs in a sandbox (`src-tauri/src/test_env.rs`): `APPDATA`, `LOCALAPPDATA`
and the temp folder point at a per-pid folder that is removed on exit, so a run never touches
the real data folder or error log and two runs at once never share scratch files. Tests that
sweep the temp folder take their own root (`sweep_test_root`).

## The suites

| Command | What it guards |
| --- | --- |
| `node tests/i18n_check.js` | Key parity across the 11 languages, substitution tokens matching `en`, keys used in `ui/` that do not exist, unreferenced strings, the same key and token parity for the compact strings in `ui/i18n/float.js`, and one language file per code in `LANGS`. Every window loads texts through `compactTexts` or `fullTexts` from `ui/i18n.js`; the tray is fed by the backend. Every static `data-i18n` fallback text in `ui/*.html` matches its `en` string (keys containing `{app}` excepted). |
| `node tests/terms_check.js` | A brand's fork vocabulary covers every form of the word, per language, once it opts that language in. |
| `node tests/voice_check.js` | `applyVoiceCommands()`: punctuation attaches to the previous word with no leading space, auto-capitalization, spoken command words resolve per language. |
| `node tests/search_check.js` | `parseSearchQuery()`: `tag:`/`var:` search terms (and their negated forms) are pulled out of a query correctly, everything else is left untouched. |
| `node tests/varpreview_check.js` | `varPreview()` (`ui/floatShared.js`): a placeholder in a preview reads `[name]` (name before an equals sign or a bar, a reserved name without its slash fence), plain text and an empty fence stay as typed, and the pattern copy equals `VAR_RE` in `ui/promptVars.js`. |
| `node tests/help_paths_check.js` | Every `Expert menu → <tab> → <group>` / `Expertenmenü → ...` path quoted in `ui/help.js` (EN and DE) names a real tab and a real group of that tab, with the label the menu shows in that language (tabs and groups read from `EXPERT_TABS` in `ui/expertConfig.js` and the strings in `ui/i18n/*.js`). A group heading with no string in `ui/i18n/*.js` yet uses its agreed wording from the test's `PENDING` list until the string exists. |
| `node tests/eta_check.js` | `makeEtaEstimator()`/`formatEta()`: nothing shows before `minPct`/`minSecs`, a mid-run pause does not skew the average rate, and the seconds/minutes/over-an-hour wording tiers land where expected. |
| `node tests/export_name_check.js` | `exportNameIsValid()` (`ui/exportName.js`): a plain or dotted name and a bare `..` inside a longer name pass, while empty/blank, a lone `.`/`..`, a Windows-reserved character or control character, a trailing dot, and a reserved device name (with or without a following segment) are refused. |
| `node tests/ink_check.js` | `applyFloatTheme()`'s readable-ink rule: the black/white split follows `autoInk`/`inkThreshold` from a `theme-changed` event and from one `ink_prefs()` fetch at load, not the old fixed 0.179 cutoff. |
| `node tests/video_overlay_time_check.js` | A video overlay's live preview clip time (`ui/video.js`, the "video" branch of `buildLiveOverlayEl`) goes through `outMs()` the same way the export's `videoOverlayEnable()` does, so a cut or a speed change before the overlay does not desync the preview from what ffmpeg actually renders. Runs the real extracted source of the helpers involved rather than a reimplementation. |
| `node tests/video_ops_check.js` | Video editor export decisions and filter graph (`ui/video.js`), run on the real extracted source against a fake editor state: a plain stream copy is never chosen while an inserted clip exists, an inserted clip keeps its own sound, a video overlay's end cap is worked out on the output clock, and editing an overlay keeps its stacking order. |
| `node tests/video_overlay_style_check.js` | `.ve-live-video` in `ui/styles/video.css` uses `object-fit: contain` (matching the export filter's `force_original_aspect_ratio=decrease` + black pad), not `fill`, while `.ve-live-image` keeps `fill` since its own export filter has no aspect-preserving step. |
| `node tests/video_export_request_check.js` | The whole-millisecond fields of the video export request (`total_ms`, and the `durationMs` of the size estimate and the disk-space check) stay integers when a tempo change makes the output length fractional; a float would be refused by the backend and abort the export. Runs the real extracted source of `startExport`'s length line and the timeline helpers. |
| `node tests/snip_crop_check.js` | `snipEditResizeCropRect()` (screenshot editor crop box): a shift-drag of a top corner keeps the aspect ratio and the opposite edges where they are, including the flipped case. Runs the real extracted source. |
| `node tests/blur_grace_check.js` | Every window script with a `BLUR_GRACE_MS` blur handler (launcher, placeholder window, tray) also has the `else setTimeout(` re-check branch, so a blur during the show hand-off is deferred instead of dropped. |
| `node tests/ui/recbar_reset_check.js` | The recording bar page (`ui/recbar.js`, real file in a node vm, scripted backend) outlives a recording because its window is only hidden: after Stop, the `rec-bar-reset` event sent by `show_rec_bar` re-arms the buttons, the timer and the poll for the second and third recording. |
| `node tests/ui/recbar_lang_check.js` | The recording bar (`ui/recbar.js`, a hidden-never-closed window) follows a UI language switch: the real file runs in a node vm against a scripted backend, a `language-changed` event makes its button titles and aria-labels (stop, pause, audio, target switch) the new language straight away, and back again. |
| `node tests/ui/camera_lang_check.js` | The camera window (`ui/camera.js`, open while the language can be switched in the main window) follows a `language-changed` event: labels, start button, button names, the silent-take option and the no-camera notice are the new language straight away. Real file in a node vm against a scripted backend. |
| `node tests/ui/tray_launcher_handoff_check.js` | The tray filter row to quick launcher hand-off (`ui/tray.js`, `ui/launcher.js`, real files in node vms against a model of the focus and the backend): keys typed every 7 to 200 ms while the launcher page is still loading arrive exactly once and in order, the launcher puts the passed-on text in front of keys it received itself, the field is capped at 200 characters, and the tray window closes afterwards. `HANDOFF_MODEL=old` replays the earlier backend that focused the launcher before its page existed. The default model also reads the Rust sources (`src-tauri/src/*.rs`) and `launcher.html`: a hand-off build that still passes `.focused(true)` makes the hidden launcher steal the focus from the tray, and a launcher field without `maxlength` is not capped for typed keys. |
| `node tests/snip_ctx_layer_check.js` | The screenshot editor's right-click shape menu (`.ctx.snip-ctx-pop`) has a z-index above the editor overlay and `ui/snipEditor.js` gives the menu that class. |
| `node tests/ui/snip_pick_check.js` | Pure helpers of the screenshot overlay (`ui/snip.js`), real extracted source: `pickWindow` hits the top window's frame (a title bar point never falls through to the window behind), `snipErrorText` maps a backend error to the overlay text the same way for the region drag and the window click. |
| `node tests/ui/snip_lang_check.js` | The screenshot overlay (`ui/snip.js`, a hidden-never-closed window) follows a UI language switch: the real file runs in a node vm against a scripted backend, a `language-changed` event between two sessions makes the second session's hint the new language. |
| `node tests/ui/lang_install_poll_check.js` | `pollLangInstall()` / `closeLangsModal()` (`ui/dictation.js`, real extracted source, fake timers): an `install_result` answer that lands after the language dialog was closed (and optionally reopened) is still handled: row no longer busy, capability list refreshed, no poller left armed. |
| `node tests/ui/store_synced_check.js` | The `store-synced` listener (`ui/main.js`, real extracted source): the main window refetches the state and redraws the grid, and redraws the library too when it is open, so a prompt the folder sync imported is never shown stale. |
| `python tests/source_hygiene.py` | No tracked source file outside `tests/` carries a work-item number, an AI-tool name or an internal planning-doc filename in a comment. |
| `python tests/module_guard.py` | The main-window modules listed in `tests/ui_sources.json`: no module assigns to an imported name, every name another module defines is imported where it is used, nothing imports `main.js`, the kernel has no import cycle, start-up hand-overs match, no unused import. Self-tests its own rules first. |
| `python tests/no_emdash.py` | No em dash, en dash, horizontal bar or figure dash in `ui/i18n/*.js`, `ui/help.js`, `docs/help.md`, `README.md` or any `ui/*.js`/`ui/*.html` file's visible text (comments excluded), no ' -- ' inside a Rust or UI string literal, and no dash character in `src-tauri/installer.nsi` or `src-tauri/nsis-languages/*.nsh`. Known-good exceptions live in its `EXCEPTIONS` list with a reason each. |
| `python tests/flag_polarity.py` | Every `flag-<key>` / `noflag-<key>` class in a stylesheet or in markup belongs to the registry its prefix implies. |
| `python tests/tooltip_check.py` | No icon-only control without a tooltip or an accessible name, in any window. |
| `python tests/a11y_check.py` | Over all nine windows: no icon-only control without a name, every value-bearing control (timeline, trim handles, volume sliders, background-jobs percent) wired to `aria-valuenow`, exactly one live region per window, every popover/menu shown via `classList.toggle("hidden", ...)` also trapped by `trapTabWithin(...)`, every window's stylesheets covered by a `prefers-reduced-motion` block. A manual Windows Narrator pass over the timeline and the video export is still needed on top of this. |
| `python tests/shortcut_check.py` | Every `ui/shortcuts.js` action resolves to a real i18n key, no key combo is bound twice in one scope, every `data-shortcut` button carries `data-i18n-title` so it actually shows the key, every `withShortcut()` call (static or dynamic) names a real action, and every non-alt action is shown by at least one button (list-only windows and the Ctrl+Tab view switching are exempt). |
| `python tests/tour_check.py` | Every first-run tour `TOUR_FEATURES` row (`ui/tourContent.js`) points at a flag that actually exists in the flag registry and a text key that actually exists in `ui/i18n/en.js`. |
| `node tests/camera_css_check.js` | The camera window's copied tokens and control rules match main.css, and every variable it uses is defined. |
| `python tests/contrast_check.py` | WCAG text, border and focus-ring contrast across every theme and window, accounting for a dark text-shadow halo where one covers the text. |
| `python tests/expert_menu_check.py` | Every expert tab entry exists once, no `gate:` on an unknown flag, nothing unreachable, every `DEPENDENCIES` pair really carries the parent's gate in the parent's own group; every `EXPERT_NOTICES` entry is declared, shown by exactly one group, has both its strings, and never doubles as a flag key. |
| `node tests/remote_tls_switch_check.js` | The certificate lifetime switch: expert entry with its gate and tip, strings in all 11 languages, kept out of bulk writes, and the toggle saves nothing on cancel and restores the switch on a failed save. |
| `python tests/settings_directory_check.py` | Every `SETTINGS_DIRECTORY` entry (the expert search's cross-area hits: normal settings, toolbar, library, backups) points at a real element and a real i18n key; every real setting control in `#settings-main` is either in the directory or explicitly whitelisted as not a setting. |
| `python tests/automation_doc_check.py` | `docs/automation.md`'s generated command table matches `docs/automation-manifest.json` (itself checked against `command_manifest()` in `src-tauri/src/cli.rs` by a `cargo test --lib` case) -- catches a command added to the CLI without the doc being regenerated via `tests/gen_automation_doc.py`. |
| `python tests/layout/modal_scroll_test.py` | Real edit-modal markup plus `main.css` in headless Chromium at five small viewports down to the app minimum: the expanded version history keeps a usable height and Save stays reachable. Also the theme editor (one column at 520 px, two at 900 px, Save reachable in both). |
| `python tests/ui/expert_rows_test.py` | Boots index.html and, for a table of expert rows, reads the tab, the group title and the `--expert-depth` each one renders with (chain block, recording cleanup, video-editor and library actions, views limits, colour rows, the fixed-value list); a second boot with the parent switches off asserts the dependent rows are gone. Also greps the help texts for group names that no longer exist. |
| `python tests/ui/select_picker_test.py` | Boots index, video and camera windows, opens the surfaces that build selects on demand (expert tabs, statistics export, screenshot editor text tool, video overlay editor) and asserts every `<select>` has the themed picker attached (`data-select-picker`) and an 8 px radius; drives the keyboard path and the disabled-select case. |
| `python tests/ui/smoke_test.py` | Boots the real `index.html` against `tests/ui/tauri_stub.js` and walks every window (see below). |
| `python tests/perf_hot_paths.py` | Times four hot paths at 1000, 5000 and 10000 prompts, plus the video editor once (see below). Needs a Chromium-family browser (Chrome or Edge) installed. |

Tests read the dictionaries through `tests/i18n_load.js` / `tests/i18n_files.py`, never by opening a language file directly.

### `source_hygiene.py` in detail
Covers line **and** block comments (`/* */`, `<!-- -->`), the spelled-out forms
("point 194", "Item 2", "Runde F", "Phase B"), ranges (`90-94:`), decision dates
(`(21.08.)`) and a comment that is nothing but a number (`// 43`). String literals are
blanked first, so a URL in code is never read as a comment. Known-good exceptions live
in its `EXCEPTIONS` list with a reason each. This is a permanent gate, not a one-time
cleanup.

### `no_emdash.py` in detail
Strips `//` and `/* */` / `<!-- -->` comments before scanning a `.js`/`.html` file (a
comment is out of scope on purpose, cleaned up separately later), then flags any of the
four dash characters left in the visible text: string literals, markup, and every line of
`docs/help.md` and `README.md`. A plain ASCII hyphen (e.g. a `00:12-00:34` time range) is
not one of the four and never trips it.

### `flag_polarity.py` in detail
A default-off feature gated on `noflag-` (or the reverse) parses fine, passes every
other test, and can never fire; only this comparison against `FLAG_LABELS` /
`OPT_FLAG_LABELS` tells the two apart. `--unused` also lists registry keys no stylesheet
mentions - information, not a defect.

### `smoke_test.py` in detail
Walks library, history, tile menu and every expert tab at four viewports plus a 130 %
pass, then `floating.html`, `snip.html`, `folder.html`, `vars.html`, `launcher.html`,
`tray.html`. Fails on any JS error, horizontal overflow, or a string rendered as its raw
i18n key. Layout guards on top: nothing clipped away by a box that cannot scroll to it,
no scroll region under 40 px, every tab and dialog action hit-testable. It also proves
that the Markdown preview renders real nodes and shows injected markup literally, and
that flipping a parent toggle hides its dependent rows without reopening the menu. Two
extra boots of `index.html` (`SEAL_PROBE`, stub flag `window.__STORE_SEALED__`) cover the
store-sealing warning both ways: shown once with its permanent settings row when the
store is unencrypted, and completely absent - with no empty gap left behind - when it is
not.

Layout: `tests/ui/smoke_test.py` is the entry (browser lookup, temp dir, suite order, the probe id
check); the suites live in `tests/ui/smoke/`: `harness.py` (paths, window sizes, the one Chrome
call helper `run_chrome`, the manifest recorder), `main_walk.py` (size walk and Italian pass; its
probe is assembled from `walk/w01..w18`, one fragment per surface), `side_windows.py`, `video.py`
(also the seeded expert boot), `boots.py` (single boots) and `findings.py` (the generated findings
block). `python tests/ui/smoke_test.py --only <suite>` runs one suite (repeatable; `main_walk`,
`side_windows`, `video`, `boots`, `video_expert`, `findings`); without it everything runs.

Probe rule: a probe block starts with `// <id>:` and every failure it pushes starts with `<id>:`;
an id is used by one block only (`<item>` or `<item>-<suffix>`). The entry fails a run that reuses an
id beyond `LEGACY_DUP_IDS`, the frozen list of ids reused before this rule. Cite a probe as
`probe <id> (smoke/<file>)`, never by line.

Refactor proof: `SMOKE_MANIFEST=<file>` writes one line per Chrome call (masked args, timeout,
hash of the staged temp tree); with `SMOKE_MANIFEST_DRY=1` no browser starts and the run
finishes in seconds. Two manifests from the same code are byte-equal.

### `perf_hot_paths.py` in detail
Boots `index.html` against `tauri_stub.js` with the fixture overridden to N synthetic
prompts (10% with an image) across two views, and times four hot paths from the real
DOM's perspective (click to fully rendered): opening the library (all unfiltered rows),
typing a search query, switching views (a full grid re-render), opening the expert menu.
Runs the four paths three times, at N = 1000, 5000 and 10000, each stage against its own
target (1000 keeps the original 400-800 ms, 5000 gets 3x, 10000 gets 6x), and prints one
table covering every stage at the end instead of stopping at the first miss - a real
regression at 10000 must never hide behind 1000 and 5000 passing. All four measured
10-120 ms at 1000 entries on the 30.08. baseline. Set `PERF_QUICK=1` to run only the
1000-entry stage while developing. Separately times the video editor opening a 10-minute
1080p30 recording once (unrelated to prompt count, so not repeated per stage). Needs a
Chromium-family browser (Chrome or Edge); skips cleanly if neither is found.

## Main-window sources
The main window is several modules under `ui/`. Tests that scan its source text never open
`ui/main.js` directly: they read the list `main_window_modules` in `tests/ui_sources.json`
as one text, the files joined with a newline in list order. Helpers: `tests/ui_sources.py`
(`main_window_files()`, `main_window_source(rev=None)`, a git revision reads the files as
they were there) and `tests/ui_sources.js` (`mainWindowModules`, `mainWindowFiles()`,
`mainWindowSource()`). The Rust tests read the same json at run time. A new module of the
main window is appended to the json in the commit that creates it.

## Moving code
`python tools/move_check.py [--base REV] [--max N]` proves that a staged change under `ui/`
only moved code: every removed line must reappear as an added line (whitespace, a leading
`export ` and import lines ignored). Stage the change first (`git add`), run it, and pass
`--max` the number of lines the item deliberately rewrites; every unmatched removed line is
printed with its old line number.

## Harness constraints (all three browser suites)
- `smoke_test.py` and `modal_scroll_test.py` load the page in an IFRAME of the exact
  viewport size, because headless Chrome refuses windows narrower than ~500 px, and they
  keep a heartbeat dirtying an unused custom property, because a small quiet frame stops
  producing frames and the app reveals its window from a nested `requestAnimationFrame`.
  `perf_hot_paths.py` loads `index.html` directly in a normal-sized window instead, since
  its own window is never below that floor.
- Timers run on Chrome's virtual clock in all three suites, so a probe must never wait on
  anything needing a real fetch (a dynamic `import()`, for example). Synchronous
  CPU-bound work (DOM building, layout) still costs real wall-clock time under virtual
  time, only idle and timer waits get fast-forwarded - which is what makes
  `perf_hot_paths.py`'s numbers meaningful rather than fabricated.
- A `</scr` + `ipt>` in a probe string must be split or it ends the probe's own script
  tag.
- Every `PROBE` / `*_PROBE` block is a Python triple-quoted (non-raw) string: a
  JS-intended `\'` is consumed by PYTHON's own escape processing before the browser ever
  sees it, leaving an unescaped quote that breaks the surrounding JS string literal and
  silently kills the WHOLE script tag. It reports as "no probe result" with no JS error,
  because the syntax error prevents the script from ever starting. Write JS string
  literals in these probes without apostrophes or contractions rather than escaping one.
