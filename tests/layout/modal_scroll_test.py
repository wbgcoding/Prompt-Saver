"""Regression test: the edit dialog stays usable on a small window.

Renders the real edit-modal markup with the real ui/styles/main.css in headless
Chromium and asserts that, with the version history expanded, the list keeps a
usable height and the Save row is reachable by scrolling.

The dialog is a flex column: any item with its own overflow gets an automatic
minimum size of ZERO, which is what used to squeeze the open history down to a
sliver instead of letting the modal scroll. This test locks that behaviour in.

Run:  python tests/layout/modal_scroll_test.py
Exit: 0 = all sizes pass, 1 = regression.
"""
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
STYLES = REPO / "ui" / "styles"

# Window sizes to check, smallest last. The app allows resizing well below these.
SIZES = ["560,430", "520,360", "480,300"]
MIN_HISTORY_H = 100  # px the expanded list must keep

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]

MEASURE = """
function measure() {
  const m = document.querySelector('#modal .modal');
  const ov = document.getElementById('modal');
  const hb = document.getElementById('modal-history-body');
  const save = document.getElementById('modal-confirm');
  m.scrollTop = m.scrollHeight;
  ov.scrollTop = ov.scrollHeight;
  const mr = m.getBoundingClientRect();
  const sr = save.getBoundingClientRect();
  const hr = hb.getBoundingClientRect();
  const top = Math.max(mr.top, 0), bottom = Math.min(mr.bottom, innerHeight);
  const out = {
    viewport: innerWidth + 'x' + innerHeight,
    histBodyH: Math.round(hr.height),
    histContentH: hb.scrollHeight,
    saveReachable: sr.top >= top - 1 && sr.bottom <= bottom + 1,
  };
  out.historyUsable = out.histBodyH >= %d;
  out.PASS = out.saveReachable && out.historyUsable;
  document.getElementById('RESULT').textContent = JSON.stringify(out);
}
requestAnimationFrame(() => requestAnimationFrame(measure));
""" % MIN_HISTORY_H


def build_page(out_dir: Path, n_rows: int = 8, name: str = "modal.html") -> Path:
    if not (out_dir / "styles").exists():
        shutil.copytree(STYLES, out_dir / "styles")

    rows = "\n".join(
        f'<div class="hist-row"><div class="hist-head">'
        f'<span class="hist-time">25.07.2026, 1{i}:04</span>'
        f'<span class="hist-preview">Older revision {i}: review the code.</span>'
        f'<button type="button" class="hist-expand"></button>'
        f'<button type="button" class="ghost-btn hist-restore">Restore</button>'
        f'</div><div class="hist-full hidden">Older revision {i}</div></div>'
        for i in range(n_rows)
    )
    swatches = '<button type="button" class="swatch"></button>' * 10

    # The .clear-wrap / .clear-wrap-ta spans mirror what wireClear() injects at
    # runtime -- the textarea wrapper is the actual flex item, so the harness is
    # only faithful with it in place.
    html = f"""<!doctype html><html lang="en"><head><meta charset="utf-8">
<link rel="stylesheet" href="styles/main.css"><style>body{{margin:0}}</style></head><body>
<div id="modal" class="overlay tall history-open" role="dialog" aria-modal="true"><div class="modal">
  <div class="modal-titlebar"><h2 class="modal-title">Edit prompt</h2>
    <button class="icon-btn modal-x" type="button">X</button></div>
  <div class="modal-name-row"><span class="clear-wrap">
    <input id="modal-name" class="modal-input" type="text" value="Code Review">
    <button type="button" class="clear-btn show">x</button></span>
    <button type="button" id="modal-fav" class="modal-fav-star">&#9733;</button></div>
  <span class="clear-wrap clear-wrap-ta">
    <textarea id="modal-text" class="modal-input modal-text" rows="6">Review the following code.</textarea>
    <button type="button" class="clear-btn show">x</button></span>
  <div id="modal-length" class="modal-length">26 chars</div>
  <div id="modal-style-row" class="two-col">
    <label class="field"><span>Font</span><select class="modal-input"><option>Default</option></select></label>
    <label class="field"><span>Text size</span><select class="modal-input"><option>Auto</option></select></label></div>
  <div id="color-row" class="swatches">{swatches}</div>
  <div id="modal-history" class="modal-collapse open">
    <button type="button" id="modal-history-head" class="modal-collapse-head" aria-expanded="true">
      <span>Version history</span></button>
    <div id="modal-history-body" class="modal-collapse-body">{rows}</div></div>
  <div class="modal-actions"><button class="ghost-btn danger-btn">Delete</button>
    <button id="modal-confirm" class="primary-btn">Save</button></div>
</div></div>
<pre id="RESULT"></pre>
<script>{MEASURE}</script></body></html>"""
    page = out_dir / name
    page.write_text(html, encoding="utf-8")
    return page


_run = 0


def measure(browser, tmp, url, size):
    # An isolated profile is required or Chrome hands the URL to a running
    # instance and prints nothing — and a FRESH one per run, or a still-closing
    # instance holds the lock and the next run comes back empty.
    global _run
    _run += 1
    proc = subprocess.run(
        [browser, "--headless=new", "--disable-gpu", "--no-first-run",
         f"--user-data-dir={tmp / ('profile-%d' % _run)}", "--allow-file-access-from-files",
         f"--window-size={size}", "--virtual-time-budget=3000", "--dump-dom", url],
        capture_output=True, text=True, timeout=120,
    )
    m = re.search(r'<pre id="RESULT">(\{.*?\})</pre>', proc.stdout, re.S)
    return json.loads(m.group(1)) if m else None


def main() -> int:
    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        print("SKIP: no Chromium browser found")
        return 0

    tmp = Path(tempfile.mkdtemp(prefix="ps-layout-"))
    try:
        url = build_page(tmp).as_uri()
        failed = 0
        for size in SIZES:
            r = measure(browser, tmp, url, size)
            if not r:
                print(f"FAIL {size}: no measurement (browser produced no result)")
                failed += 1
                continue
            status = "ok  " if r["PASS"] else "FAIL"
            print(f"{status} win={size} vp={r['viewport']} history={r['histBodyH']}px "
                  f"saveReachable={r['saveReachable']}")
            if not r["PASS"]:
                failed += 1

        # A short history must hug its content: a fixed floor left a dead band
        # under the last entry.
        short = build_page(tmp, n_rows=2, name="short.html")
        r = measure(browser, tmp, short.as_uri(), SIZES[0])
        if not r:
            print("FAIL short list: no measurement")
            failed += 1
        else:
            slack = r["histBodyH"] - r["histContentH"]
            ok = slack <= 8  # padding only, no empty band
            print(f"{'ok  ' if ok else 'FAIL'} 2-entry history: box={r['histBodyH']}px "
                  f"content={r['histContentH']}px slack={slack}px")
            if not ok:
                failed += 1
        print("PASS" if not failed else f"{failed} check(s) regressed")
        return 1 if failed else 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
