"""Regression test: the edit dialog stays usable on a small window.

Renders the real edit-modal markup with the real ui/styles/main.css in headless
Chromium and asserts that, with the version history expanded, the list keeps a
usable height and the Save row is reachable by scrolling.

The dialog is a flex column: any item with its own overflow gets an automatic
minimum size of ZERO, which is what used to squeeze the open history down to a
sliver instead of letting the modal scroll. This test locks that behaviour in.

Everything is measured three times over: in English and in the two wordiest UI
languages, with the labels taken straight from ui/i18n/. A row that fits an
English word can still burst once it reads "Restaurează" or "Przywróć".

Run:  python tests/layout/modal_scroll_test.py
Exit: 0 = all sizes pass, 1 = regression.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from html import escape
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
STYLES = REPO / "ui" / "styles"

# A row is only as tight as its longest label, so every measurement runs three
# times: English, Polish and Romanian, the two wordiest of the eleven UI
# languages. The texts are read out of ui/i18n/ instead of being copied here,
# so a retranslation cannot leave this test measuring words the app dropped.
LANGS = ["en", "pl", "ro"]
sys.path.insert(0, str(REPO / "tests"))
from i18n_files import combined_source  # noqa: E402

_I18N = combined_source()


def strings(lang: str) -> dict:
    """Label texts of one UI language, with English filling any gap."""
    def block(code: str) -> dict:
        body = re.search(r"\n  %s: \{\n(.*?)\n  \},\n" % code, _I18N, re.S).group(1)
        out = {}
        for key, raw in re.findall(r'^    ([A-Za-z0-9_]+): (".*?")(?:,|$)', body, re.M):
            try:
                out[key] = json.loads(raw)
            except ValueError:
                pass  # a value built from pieces: no plain label, not needed here
        return out
    return {**block("en"), **block(lang)}

# Viewport sizes to check, smallest last. The last one is the app's own minimum
# window size, so nothing here is smaller than the user can actually make it.
_WIN = json.loads((REPO / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))["app"]["windows"][0]
SIZES = [(820, 600), (720, 500), (_WIN["minWidth"], _WIN["minHeight"])]
MIN_HISTORY_H = 100  # px the expanded list must keep

# Headless Chrome will not make its window narrower than ~500px, so the page is
# measured inside an iframe of the exact size and the wrapper republishes the
# result (--dump-dom only prints the top document).
WRAPPER = """<!doctype html><meta charset="utf-8">
<style>html,body{margin:0}iframe{border:0;display:block}</style>
<iframe id="f" src="%s" style="width:%dpx;height:%dpx"></iframe><pre id="RESULT"></pre>
<script>
setInterval(() => {
  try {
    const p = f.contentDocument.getElementById('RESULT');
    const own = document.getElementById('RESULT');
    if (p && p.textContent && !own.textContent) own.textContent = p.textContent;
  } catch (e) {}
}, 100);
</script>"""

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
  // No direct child of the dialog may sit on top of the one before it. Expanding a
  // collapsible must push the rows below it down, never overlap them.
  const overlaps = [];
  const name = (e) => e.id || e.className || e.tagName.toLowerCase();
  // `display: contents` boxes (the Markdown wrapper around the textarea) are NOT rows
  // of their own: their children are. Walking element.children alone dropped the
  // textarea out of the scan entirely, so it could grow over the colour row unseen.
  const boxes = (el) => [...el.children].flatMap((k) =>
    getComputedStyle(k).display === 'contents' ? boxes(k) : [k]);
  for (const parent of [m, ...m.querySelectorAll('*')]) {
    const st = getComputedStyle(parent);
    if (st.display !== 'block' && st.display !== 'flex') continue;
    const row = st.display === 'flex' && st.flexDirection.startsWith('row');
    const kids = boxes(parent).filter(e => e.getClientRects().length &&
      getComputedStyle(e).position === 'static');
    if (row) {
      // A wrapping row reflows by design; only a nowrap row can push an item out.
      if (st.flexWrap !== 'nowrap') continue;
      // A nowrap row must fit its own content box: an item pushed past the right
      // edge is drawn on top of whatever is there.
      const pr = parent.getBoundingClientRect();
      for (const k of kids) {
        const r = k.getBoundingClientRect();
        if (r.right > pr.right + 1) {
          overlaps.push(name(parent) + ' overflows by ' + Math.round(r.right - pr.right) + 'px at ' + name(k));
        }
      }
      continue;
    }
    for (let i = 1; i < kids.length; i++) {
      const a = kids[i - 1].getBoundingClientRect(), b = kids[i].getBoundingClientRect();
      if (b.top < a.bottom - 1 && b.height > 0 && a.height > 0) {
        overlaps.push(name(kids[i - 1]) + ' / ' + name(kids[i]) + ' by ' + Math.round(a.bottom - b.top) + 'px');
      }
    }
  }
  const out = {
    viewport: innerWidth + 'x' + innerHeight,
    histBodyH: Math.round(hr.height),
    histContentH: hb.scrollHeight,
    saveReachable: sr.top >= top - 1 && sr.bottom <= bottom + 1,
    overlaps,
  };
  out.historyUsable = out.histBodyH >= %d;
  out.PASS = out.saveReachable && out.historyUsable && !overlaps.length;
  document.getElementById('RESULT').textContent = JSON.stringify(out);
}
// In a small headless frame Chromium stops producing frames once the page is quiet,
// so a bare rAF chain can stall before the second callback. Dirty an unused custom
// property to keep frames coming; it touches no geometry.
const tick = setInterval(() => {
  document.documentElement.style.setProperty('--tick', String(Date.now()));
  document.body.getBoundingClientRect();
}, 30);
requestAnimationFrame(() => requestAnimationFrame(() => {
  clearInterval(tick);
  document.documentElement.style.removeProperty('--tick');
  measure();
}));
""" % MIN_HISTORY_H


def build_page(out_dir: Path, n_rows: int = 8, name: str = "modal.html", vars_open: bool = False,
               history_open: bool = True, lang: str = "en") -> Path:
    if not (out_dir / "styles").exists():
        shutil.copytree(STYLES, out_dir / "styles")

    txt = strings(lang)
    # Every label comes from the language under test; the prompt text, names and
    # dates are user content and stay as they are.
    def L(key: str) -> str:
        return escape(txt[key])

    rows = "\n".join(
        f'<div class="hist-row"><div class="hist-head">'
        f'<span class="hist-time">25.07.2026, 1{i}:04</span>'
        f'<span class="hist-preview">Older revision {i}: review the code.</span>'
        f'<button type="button" class="hist-expand"></button>'
        f'<button type="button" class="ghost-btn hist-restore">{L("historyRestore")}</button>'
        f'</div><div class="hist-full hidden">Older revision {i}</div></div>'
        for i in range(n_rows)
    )
    swatches = '<button type="button" class="swatch"></button>' * 10
    # A media prompt opens the preview block above the text. It is the tallest
    # optional row of the dialog and the one that used to be drawn over the colour
    # row when the window got small. Inline SVG: the harness never touches the net.
    preview_src = (
        "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' "
        "width='320' height='200'><rect width='320' height='200' fill='%23888'/></svg>"
    )
    # The F12/F13 variable helper sits above the textarea and wraps on a narrow
    # dialog, so it has to count against the height the history is left with.
    var_chips = "".join(
        f'<button type="button" class="vars-chip">{n}</button>'
        for n in ("date", "time", "datetime", "weekday", "clipboard", "uuid")
    )
    # Tag chips wrap over several lines on a narrow dialog; the harness carries the
    # feature class so that extra row counts against the available height.
    tag_chips = "".join(
        f'<span class="tag-chip"><button type="button" class="tag-dot"></button>'
        f'<span class="tag-chip-label">Tag {i}</span>'
        f'<button type="button" class="tag-x">&times;</button></span>'
        for i in range(6)
    )

    # The .clear-wrap / .clear-wrap-ta spans mirror what wireClear() injects at
    # runtime -- the textarea wrapper is the actual flex item, so the harness is
    # only faithful with it in place. The notes field is not collapsible: it sits
    # right above the colour row and always takes its space; font and size live in
    # the collapsed advanced options and take no room here.
    html = f"""<!doctype html><html lang="{lang}"><head><meta charset="utf-8">
<link rel="stylesheet" href="styles/main.css"><style>body{{margin:0}}</style></head><body class="flag-tags">
<div id="modal" class="overlay tall{' history-open' if history_open else ''}" role="dialog" aria-modal="true"><div class="modal">
  <div class="modal-titlebar"><h2 class="modal-title">{L('editModalTitle')}</h2>
    <button class="icon-btn modal-x" type="button">X</button></div>
  <div class="modal-name-row"><span class="clear-wrap">
    <input id="modal-name" class="modal-input" type="text" value="Code Review">
    <button type="button" class="clear-btn show">x</button></span>
    <button type="button" id="modal-fav" class="modal-fav-star">&#9733;</button></div>
  <div id="modal-img-wrap" class="modal-img-wrap">
    <div class="modal-media-stage">
      <img id="modal-img" class="modal-preview-img" alt="" src="{preview_src}">
      <div id="modal-caption-preview" class="tile-caption modal-caption-preview">Caption</div></div>
    <div class="modal-display-row">
      <button type="button" class="ghost-btn">{L('showImageBtn')}</button>
      <button type="button" class="ghost-btn">{L('showTextBtn')}</button>
      <button type="button" class="ghost-btn">{L('replaceMedia')}</button>
      <button type="button" class="ghost-btn danger-btn">{L('removeImage')}</button></div>
    <div class="caption-row">
      <span class="clear-wrap"><input id="modal-caption" class="modal-input" type="text" value="On the tile">
        <button type="button" class="clear-btn show">x</button></span>
      <select id="modal-caption-size" class="modal-input caption-size"><option>{L('styleDefault')}</option></select></div></div>
  <div id="modal-text-bar" class="modal-collapse modal-text-bar{' open' if vars_open else ''}">
    <button type="button" id="modal-vars-head" class="modal-collapse-head" aria-expanded="{'true' if vars_open else 'false'}">
      <span>{L('varsSection')}</span></button>
    <div id="modal-vars-body" class="modal-collapse-body{'' if vars_open else ' hidden'}">
      <p id="modal-vars-hint" class="modal-hint modal-vars-hint">{L('varsHint')}</p>
      <div id="modal-vars-new" class="vars-new">
        <span class="clear-wrap"><input id="modal-var-name" class="modal-input vars-new-name" type="text" value="Tone">
          <button type="button" class="clear-btn show">x</button></span>
        <span class="clear-wrap"><input id="modal-var-opts" class="modal-input vars-new-opts" type="text" value="formal,casual">
          <button type="button" class="clear-btn show">x</button></span>
        <button type="button" id="modal-var-add" class="ghost-btn">{L('varsInsert')}</button></div>
      <p id="modal-vars-choice-hint" class="modal-hint">{L('varsChoiceHint')}</p>
      <div id="modal-vars-chips" class="vars-chips">{var_chips}</div>
      <p id="modal-vars-dyn-hint" class="modal-hint">{L('varsDynHint')}</p></div></div>
  <div id="modal-text-wrap" class="modal-text-wrap">
    <span class="clear-wrap clear-wrap-ta">
      <textarea id="modal-text" class="modal-input modal-text" rows="6">Review the following code.</textarea>
      <button type="button" class="clear-btn show">x</button></span>
    <div id="modal-md-preview" class="md-preview hidden"></div></div>
  <div id="modal-length" class="modal-length">26 {L('lenChars')}</div>
  <div id="modal-notes"><label for="modal-notes-text" class="modal-field-label">{L('notesLabel')}</label><textarea id="modal-notes-text" class="modal-input modal-notes-text" rows="1"></textarea></div>
  <div id="color-row" class="swatches">{swatches}</div>
  <div id="modal-tags" class="modal-tags"><span class="modal-tags-label">{L('tagsLabel')}</span>
    <div id="modal-tag-chips" class="tag-chips">{tag_chips}</div>
    <input id="modal-tag-input" class="modal-input tag-input" type="text"></div>
  <div id="modal-hotkey" class="modal-collapse">
    <button type="button" id="modal-hotkey-head" class="modal-collapse-head" aria-expanded="false">
      <span>{L('promptHotkeyTitle')}</span>
      <span class="modal-collapse-side"><span id="modal-hotkey-badge" class="modal-collapse-badge">Ctrl + Shift + 9</span></span></button>
    <div id="modal-hotkey-body" class="modal-collapse-body hidden">
      <div class="hotkey-row"><span class="clear-wrap hotkey-clear-wrap"><input id="modal-hotkey-input" class="modal-input" type="text" readonly>
        <button type="button" id="modal-hotkey-clear" class="clear-btn" aria-label="{L('hotkeyRemove')}"></button></span></div></div></div>
  <div id="modal-history" class="modal-collapse{' open' if history_open else ''}">
    <button type="button" id="modal-history-head" class="modal-collapse-head" aria-expanded="{'true' if history_open else 'false'}">
      <span>{L('historyTitle')}</span></button>
    <div id="modal-history-body" class="modal-collapse-body{'' if history_open else ' hidden'}">{rows}</div></div>
  <div class="modal-actions"><button class="text-danger-btn">{L('delete')}</button>
    <button class="ghost-btn">{L('cancel')}</button>
    <button id="modal-confirm" class="primary-btn">{L('save')}</button></div>
</div></div>
<pre id="RESULT"></pre>
<script>{MEASURE}</script></body></html>"""
    page = out_dir / name
    page.write_text(html, encoding="utf-8")
    return page


# 80: two-column colour grid in the theme editor: a narrow window falls back to one
# column (auto-fit), a wide one gets two, and Save must stay reachable either way.
THEME_SIZES = [(520, 600), (900, 600)]
MEASURE_THEME = """
function measure() {
  const m = document.querySelector('#theme-modal .modal');
  const list = document.getElementById('theme-edit-vars');
  const save = document.getElementById('theme-edit-save');
  m.scrollTop = m.scrollHeight;
  const mr = m.getBoundingClientRect();
  const sr = save.getBoundingClientRect();
  const top = Math.max(mr.top, 0), bottom = Math.min(mr.bottom, innerHeight);
  const columns = getComputedStyle(list).gridTemplateColumns.trim().split(/\\s+/).length;
  const out = {
    viewport: innerWidth + 'x' + innerHeight,
    columns,
    saveReachable: sr.top >= top - 1 && sr.bottom <= bottom + 1,
  };
  document.getElementById('RESULT').textContent = JSON.stringify(out);
}
const tick = setInterval(() => {
  document.documentElement.style.setProperty('--tick', String(Date.now()));
  document.body.getBoundingClientRect();
}, 30);
requestAnimationFrame(() => requestAnimationFrame(() => {
  clearInterval(tick);
  document.documentElement.style.removeProperty('--tick');
  measure();
}));
"""


def build_theme_page(out_dir: Path, name: str = "theme.html", n_groups: int = 4, rows_per_group: int = 5,
                     lang: str = "en") -> Path:
    if not (out_dir / "styles").exists():
        shutil.copytree(STYLES, out_dir / "styles")
    txt = strings(lang)

    def L(key: str) -> str:
        return escape(txt[key])

    # The colour rows carry the real token names, cycled to fill the grid; their
    # length is what a narrow column has to cope with.
    groups = ["themeGroupBg", "themeGroupSurface", "themeGroupText", "themeGroupAccent"]
    names = ["themeVarBg", "themeVarSurface", "themeVarSurface2", "themeVarFg", "themeVarMuted",
             "themeVarAccent", "themeVarAccentFg", "themeVarBorder", "themeVarDanger"]
    rows = "\n".join(
        f'<div class="theme-var-group">{L(groups[g % len(groups)])}</div>' + "\n".join(
            f'<div class="theme-var-row">'
            f'<span class="theme-var-name">{L(names[(g * rows_per_group + i) % len(names)])}</span>'
            f'<button type="button" class="theme-swatch"></button>'
            f'<input type="text" class="modal-input theme-hex" value="#336699">'
            f'<button type="button" class="icon-btn theme-var-reset" disabled></button></div>'
            for i in range(rows_per_group)
        )
        for g in range(n_groups)
    )
    html = f"""<!doctype html><html lang="{lang}"><head><meta charset="utf-8">
<link rel="stylesheet" href="styles/main.css"><style>body{{margin:0}}</style></head><body>
<div id="theme-modal" class="overlay" role="dialog" aria-modal="true"><div class="modal modal-narrow">
  <div class="modal-head"><h2 class="modal-title">{L('themeEditor')}</h2>
    <button class="icon-btn" type="button">X</button></div>
  <div class="theme-edit-body">
    <div class="theme-edit-top">
      <label class="field"><span>{L('themeName')}</span><input class="modal-input" type="text" value="My theme"></label>
      <label class="field"><span>{L('themeBase')}</span><select class="modal-input"><option>{L('themeDark')}</option></select></label>
    </div>
    <div id="theme-edit-vars" class="theme-var-list">{rows}</div>
  </div>
  <div class="modal-actions">
    <button class="ghost-btn danger-btn" type="button">{L('delete')}</button>
    <button class="ghost-btn" type="button">{L('themeRevert')}</button>
    <button class="ghost-btn" type="button">{L('cancel')}</button>
    <button id="theme-edit-save" class="primary-btn" type="button">{L('save')}</button>
  </div>
</div></div>
<pre id="RESULT"></pre>
<script>{MEASURE_THEME}</script></body></html>"""
    page = out_dir / name
    page.write_text(html, encoding="utf-8")
    return page


_run = 0


def measure(browser, tmp, page, size):
    # An isolated profile is required or Chrome hands the URL to a running
    # instance and prints nothing, and a FRESH one per run, or a still-closing
    # instance holds the lock and the next run comes back empty.
    global _run
    _run += 1
    w, h = size
    wrapper = tmp / f"wrap-{_run}.html"
    wrapper.write_text(WRAPPER % (page.name, w, h), encoding="utf-8")
    proc = subprocess.run(
        [browser, "--headless=new", "--disable-gpu", "--no-first-run",
         f"--user-data-dir={tmp / ('profile-%d' % _run)}", "--allow-file-access-from-files",
         f"--window-size={max(w + 30, 520)},{h + 140}",
         "--virtual-time-budget=6000", "--dump-dom", wrapper.as_uri()],
        # Without an explicit codec Windows decodes the dumped DOM as cp1252 and
        # the reader thread dies on the first non-Latin-1 byte.
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120,
    )
    m = re.search(r'<pre id="RESULT">(\{.*?\})</pre>', proc.stdout, re.S)
    return json.loads(m.group(1)) if m else None


def main() -> int:
    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        # CI's runner image always ships Chrome (see ci.yml); a missing browser there
        # means this job silently stopped covering anything, which must fail loudly
        # instead of passing green with zero coverage.
        if os.environ.get("CI"):
            print("FAIL: no Chromium browser found on a CI runner")
            return 1
        print("SKIP: no Chromium browser found")
        return 0

    tmp = Path(tempfile.mkdtemp(prefix="ps-layout-"))
    try:
        failed = 0
        for lang in LANGS:
            page = build_page(tmp, name=f"modal-{lang}.html", lang=lang)
            # Same dialog with the variables helper expanded: opening it must push the
            # rows below it down, not land on top of them.
            page_vars = build_page(tmp, name=f"modal-vars-{lang}.html", vars_open=True, lang=lang)
            # Same again with the version history COLLAPSED: the normal state of the
            # editor. Only that branch leaves the textarea's flex wrapper without a floor,
            # so the field could shrink past its own min-height and draw over the colour
            # row. The history-open variant above never reproduced it.
            page_vars_hist_closed = build_page(
                tmp, name=f"modal-vars-nohist-{lang}.html", vars_open=True, history_open=False, lang=lang
            )
            for label, pg in (("vars open", page_vars), ("vars open, history closed", page_vars_hist_closed)):
                for size in SIZES:
                    r = measure(browser, tmp, pg, size)
                    if r is None:
                        print(f"FAIL [{lang}] win={size[0]}x{size[1]} {label}: no measurement")
                        failed += 1
                    elif r["overlaps"]:
                        print(f"FAIL [{lang}] win={size[0]}x{size[1]} {label}: overlapping rows: {r['overlaps']}")
                        failed += 1
                    else:
                        print(f"ok   [{lang}] win={size[0]}x{size[1]} {label}: no overlapping rows")
            for size in SIZES:
                r = measure(browser, tmp, page, size)
                if not r:
                    print(f"FAIL [{lang}] {size[0]}x{size[1]}: no measurement (browser produced no result)")
                    failed += 1
                    continue
                status = "ok  " if r["PASS"] else "FAIL"
                print(f"{status} [{lang}] win={size[0]}x{size[1]} vp={r['viewport']} history={r['histBodyH']}px "
                      f"saveReachable={r['saveReachable']}")
                if not r["PASS"]:
                    failed += 1

            # A short history must hug its content: a fixed floor left a dead band
            # under the last entry.
            short = build_page(tmp, n_rows=2, name=f"short-{lang}.html", lang=lang)
            r = measure(browser, tmp, short, SIZES[0])
            if not r:
                print(f"FAIL [{lang}] short list: no measurement")
                failed += 1
            else:
                slack = r["histBodyH"] - r["histContentH"]
                ok = slack <= 8  # padding only, no empty band
                print(f"{'ok  ' if ok else 'FAIL'} [{lang}] 2-entry history: box={r['histBodyH']}px "
                      f"content={r['histContentH']}px slack={slack}px")
                if not ok:
                    failed += 1

            # 80: theme editor colour grid: one column on a narrow window, two on a wide
            # one, Save reachable either way. The colour names are the widest thing in a
            # column, so the fallback has to hold in a wordy language as well.
            theme_page = build_theme_page(tmp, name=f"theme-{lang}.html", lang=lang)
            for size in THEME_SIZES:
                r = measure(browser, tmp, theme_page, size)
                if not r:
                    print(f"FAIL [{lang}] theme editor {size[0]}x{size[1]}: no measurement")
                    failed += 1
                    continue
                want_cols = 1 if size[0] < 700 else 2
                ok = r["columns"] == want_cols and r["saveReachable"]
                print(f"{'ok  ' if ok else 'FAIL'} [{lang}] theme editor win={size[0]}x{size[1]} "
                      f"columns={r['columns']} saveReachable={r['saveReachable']}")
                if not ok:
                    failed += 1

        print("PASS" if not failed else f"{failed} check(s) regressed")
        return 1 if failed else 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
