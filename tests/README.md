# Test suites

Backend: `cargo test --lib`. JS syntax: `node --check <file>`. Everything below runs
from the project root. The browser-driven suites skip cleanly when no Chrome/Edge is
installed. Run the whole list after any UI change.

`cargo test` and `cargo clippy` create `src-tauri/target/debug/` (~4 GB). Delete it as
soon as the run has been evaluated; only `target/release/` ships.

## The suites

| Command | What it guards |
| --- | --- |
| `node tests/i18n_check.js` | Key parity across the 11 languages, substitution tokens matching `en`, keys used in `ui/` that do not exist, unreferenced strings. |
| `node tests/terms_check.js` | A brand's fork vocabulary covers every form of the word, per language, once it opts that language in. |
| `node tests/voice_check.js` | `applyVoiceCommands()`: punctuation attaches to the previous word with no leading space, auto-capitalization, spoken command words resolve per language. |
| `node tests/search_check.js` | `parseSearchQuery()`: `tag:`/`var:` search terms (and their negated forms) are pulled out of a query correctly, everything else is left untouched. |
| `python tests/source_hygiene.py` | No tracked source file outside `tests/` carries a work-item number, an AI-tool name or an internal planning-doc filename in a comment. |
| `python tests/flag_polarity.py` | Every `flag-<key>` / `noflag-<key>` class in a stylesheet or in markup belongs to the registry its prefix implies. |
| `python tests/tooltip_check.py` | No icon-only control without a tooltip or an accessible name, in any window. |
| `python tests/expert_menu_check.py` | Every expert tab entry exists once, no `gate:` on an unknown flag, nothing unreachable, every `DEPENDENCIES` pair really carries the parent's gate in the parent's own group; every `EXPERT_NOTICES` entry is declared, shown by exactly one group, has both its strings, and never doubles as a flag key. |
| `python tests/layout/modal_scroll_test.py` | Real edit-modal markup plus `main.css` in headless Chromium at five small viewports down to the app minimum: the expanded version history keeps a usable height and Save stays reachable. Also the theme editor (one column at 520 px, two at 900 px, Save reachable in both). |
| `python tests/ui/smoke_test.py` | Boots the real `index.html` against `tests/ui/tauri_stub.js` and walks every window (see below). |
| `python tests/perf_hot_paths.py` | Times four hot paths at 1000, 5000 and 10000 prompts, plus the video editor once (see below). Needs a Chromium-family browser (Chrome or Edge) installed. |

### `source_hygiene.py` in detail
Covers line **and** block comments (`/* */`, `<!-- -->`), the spelled-out forms
("point 194", "Item 2", "Runde F", "Phase B"), ranges (`90-94:`), decision dates
(`(21.08.)`) and a comment that is nothing but a number (`// 43`). String literals are
blanked first, so a URL in code is never read as a comment. Known-good exceptions live
in its `EXCEPTIONS` list with a reason each. This is a permanent gate, not a one-time
cleanup.

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
