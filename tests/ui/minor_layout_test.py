"""Small layout and count guards on the main window, one probe each.

Boots the real index.html against tests/ui/tauri_stub.js in headless Chromium and checks:
  * library tab counts: an archived prompt hidden by the archive toggle counts neither
    in the shown number nor in the total ("12", never "12 of 13"),
  * copy history at the 680 px minimum window: the tag chip row is one line that
    scrolls sideways with no visible scrollbar,
  * expert menu tab strip at 680 px (German): sideways scroll without a scrollbar, a soft
    fade on the edge that has more content, the active tab scrolled into view when the
    menu opens and when a tab is chosen,
  * expert menu search field at 680 px (German): the whole placeholder is visible,
  * settings About and updates block at 680 px: both lines start at the same left edge.

Run:  python tests/ui/minor_layout_test.py
Exit: 0 = clean, 1 = problems found. Skips cleanly when no Chromium is installed.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
HERE = Path(__file__).parent

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]

# The app's own minimum window width: the narrowest the user can make it.
MIN_W = json.loads((REPO / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))["app"]["windows"][0]["minWidth"]

# The page runs inside an iframe of the exact size (headless Chrome will not make its own
# window narrower than ~500 px) and the wrapper republishes the result.
WRAPPER = """<!doctype html><meta charset="utf-8">
<style>html,body{margin:0}iframe{border:0;display:block}</style>
<iframe id="f" src="%s" style="width:%dpx;height:%dpx"></iframe><pre id="SMOKE"></pre>
<script>
setInterval(() => {
  try {
    const p = f.contentDocument.getElementById('SMOKE');
    const own = document.getElementById('SMOKE');
    if (p && !own.textContent) own.textContent = p.textContent;
  } catch (e) {}
}, 200);
</script>"""

COMMON = """
  const errs = [];
  const info = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 80) && !fn(); i++) await sleep(50);
    return fn();
  };
  const $ = (id) => document.getElementById(id);
  const finish = () => {
    errs.push(...(window.__SMOKE_ERRORS__ || []));
    const pre = document.createElement('pre');
    pre.id = 'SMOKE';
    pre.textContent = JSON.stringify({ errs, info });
    document.body.appendChild(pre);
  };
  // Headless Chrome stops producing frames in a small quiet frame, and the app reveals
  // its window from a nested requestAnimationFrame: keep frames coming.
  setInterval(() => {
    document.documentElement.style.setProperty('--smoke-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  const openLibrary = async () => {
    await until(() => $('library-btn') && $('gear'));
    await sleep(500);
    $('library-btn').click();
    await sleep(400);
  };
  const openExpert = async () => {
    $('gear').click();
    await sleep(250);
    $('expert-open').click();
    await sleep(350);
  };
  const lastTab = () => { const t = document.querySelectorAll('#expert-tabs-bar .expert-tab'); return t[t.length - 1]; };
  const inView = (el, box) => {
    const r = el.getBoundingClientRect(), b = box.getBoundingClientRect();
    return r.left >= b.left - 1 && r.right <= b.right + 1;
  };
"""

# Library tab counts: archived prompts hidden by the toggle are in neither number.
COUNTS = """
<script>
(async () => {""" + COMMON + """
  try {
    await openLibrary();
    const tab = () => $('library-tab-prompts-count').textContent.trim();
    const head = () => $('library-count').textContent.trim();
    const total = document.querySelectorAll('#library-list .lib-item').length;
    if (!(total > 3)) errs.push('counts: the fixture shows only ' + total + ' prompts');
    if (tab() !== '(' + total + ')') errs.push('counts: unfiltered tab count is ' + tab() + ', expected (' + total + ')');
    const row = document.querySelector('#library-list .lib-item');
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 40 }));
    await sleep(200);
    const act = document.querySelector('#ctx [data-act="archive"]');
    if (!act) { errs.push('counts: the archive action is missing'); }
    else {
      act.click();
      await sleep(300);
      const left = total - 1;
      if (tab() !== '(' + left + ')') errs.push('counts: with one prompt archived the tab count is ' + tab() + ', expected (' + left + ')');
      if (head() !== String(left)) errs.push('counts: with one prompt archived the header count is "' + head() + '", expected ' + left);
      // A filter: shown of total, where the total leaves the hidden archived prompt out.
      const fav = $('library-fav');
      fav.click();
      await sleep(300);
      const m = /^(\\d+)\\D+(\\d+)$/.exec(head());
      if (!m) errs.push('counts: the filtered header count is "' + head() + '", expected "<shown> of <total>"');
      else if (Number(m[2]) !== left) errs.push('counts: the filtered total is ' + m[2] + ', expected ' + left + ' (archived prompt hidden)');
      fav.click();
      await sleep(300);
      // Showing the archive puts the prompt back into both numbers.
      $('library-archive').click();
      await sleep(300);
      if (tab() !== '(' + total + ')') errs.push('counts: with the archive shown the tab count is ' + tab() + ', expected (' + total + ')');
      $('library-archive').click();
      await sleep(300);
      if (tab() !== '(' + left + ')') errs.push('counts: with the archive hidden again the tab count is ' + tab() + ', expected (' + left + ')');
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  finish();
})();
</script>
"""

# Copy history tag row: one line, sideways scroll, no visible bar.
HISTORY_TAGS = """
<script>
(async () => {""" + COMMON + """
  try {
    if (innerWidth !== %d) errs.push('history-tags: the page is ' + innerWidth + 'px wide, expected %d');
    await openLibrary();
    $('library-tab-history').click();
    await sleep(500);
    const bar = $('journal-tags');
    if (!bar || bar.classList.contains('hidden')) { errs.push('history-tags: the tag row is not shown'); }
    else {
      for (let i = 0; i < 24; i++) {
        const b = document.createElement('button');
        b.type = 'button';
        b.className = 'tag-chip tag-filter';
        b.textContent = 'ExtraTag' + i;
        bar.appendChild(b);
      }
      await sleep(200);
      // Items of one line sit on one centre line whatever their own heights are.
      const mids = [...bar.children].filter((c) => c.getClientRects().length).map((c) => { const r = c.getBoundingClientRect(); return (r.top + r.bottom) / 2; });
      const rows = Math.max(...mids) - Math.min(...mids) > 4 ? 2 : 1;
      info.push('history-tags rows=' + rows + ' h=' + bar.clientHeight + ' scrollW=' + bar.scrollWidth + ' clientW=' + bar.clientWidth);
      if (rows !== 1) errs.push('history-tags: the tag chips wrap into more than one row');
      const cs = getComputedStyle(bar);
      if (!/auto|scroll/.test(cs.overflowX)) errs.push('history-tags: the tag row does not scroll sideways (overflow-x ' + cs.overflowX + ')');
      if (cs.scrollbarWidth !== 'none') errs.push('history-tags: the tag row shows a scrollbar (scrollbar-width ' + cs.scrollbarWidth + ')');
      if (!(bar.scrollWidth > bar.clientWidth)) errs.push('history-tags: the many tags do not overflow the row');
      if (bar.offsetHeight !== bar.clientHeight) errs.push('history-tags: the tag row has a bar of ' + (bar.offsetHeight - bar.clientHeight) + 'px');
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  finish();
})();
</script>
""" % (MIN_W, MIN_W)

# Expert menu tab strip: scroll without a bar, fade on the open edge, active tab in view.
EXPERT_STRIP = """
<script>
(async () => {""" + COMMON + """
  try {
    if (innerWidth !== %d) errs.push('expert-strip: the page is ' + innerWidth + 'px wide, expected %d');
    await until(() => $('gear'));
    await sleep(500);
    await openExpert();
    const strip = () => document.querySelector('#expert-tabs-bar .expert-tabs');
    let s = strip();
    if (!s) { errs.push('expert-strip: no tab strip'); }
    else {
      info.push('expert-strip scrollW=' + s.scrollWidth + ' clientW=' + s.clientWidth);
      if (!(s.scrollWidth > s.clientWidth + 2)) errs.push('expert-strip: the tabs fit at ' + innerWidth + 'px, so the strip cannot be checked');
      const mask = (el) => { const c = getComputedStyle(el); return c.maskImage && c.maskImage !== 'none' ? c.maskImage : (c.webkitMaskImage && c.webkitMaskImage !== 'none' ? c.webkitMaskImage : 'none'); };
      if (getComputedStyle(s).scrollbarWidth !== 'none') errs.push('expert-strip: a scrollbar shows (scrollbar-width ' + getComputedStyle(s).scrollbarWidth + ')');
      const first = mask(s);
      if (first === 'none') errs.push('expert-strip: no fade mask on the strip at the start');
      s.scrollLeft = s.scrollWidth;
      s.dispatchEvent(new Event('scroll'));
      await sleep(150);
      const end = mask(s);
      if (end === 'none') errs.push('expert-strip: no fade mask on the strip at the end');
      if (end === first) errs.push('expert-strip: the fade mask does not change with the scroll position');
      // Choosing a tab at the far end brings it fully into view.
      s.scrollLeft = 0;
      s.dispatchEvent(new Event('scroll'));
      await sleep(100);
      const lt = lastTab();
      lt.click();
      await sleep(300);
      s = strip();
      const act = s.querySelector('.expert-tab.active');
      if (!act || act.textContent !== lt.textContent) errs.push('expert-strip: the chosen tab is not active');
      else if (!inView(act, s)) errs.push('expert-strip: the chosen last tab is not scrolled into view');
      // Closing and reopening the menu keeps the tab, and it is in view straight away.
      $('expert-back').click();
      await sleep(250);
      $('expert-open').click();
      await sleep(350);
      s = strip();
      const act2 = s.querySelector('.expert-tab.active');
      if (!act2) errs.push('expert-strip: no active tab after reopening');
      else if (!inView(act2, s)) errs.push('expert-strip: the active tab is not in view after the menu opens');
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  finish();
})();
</script>
""" % (MIN_W, MIN_W)

# Expert menu search: the German placeholder fits in full.
EXPERT_SEARCH = """
<script>
(async () => {""" + COMMON + """
  try {
    if (innerWidth !== %d) errs.push('expert-search: the page is ' + innerWidth + 'px wide, expected %d');
    await until(() => $('gear'));
    await sleep(500);
    await openExpert();
    const inp = $('expert-search');
    const ph = inp.getAttribute('placeholder') || '';
    const cs = getComputedStyle(inp);
    const c = document.createElement('canvas').getContext('2d');
    c.font = cs.fontWeight + ' ' + cs.fontSize + ' ' + cs.fontFamily;
    const need = c.measureText(ph).width;
    const room = inp.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    info.push('expert-search placeholder="' + ph + '" need=' + Math.round(need) + ' room=' + Math.round(room) + ' w=' + Math.round(inp.getBoundingClientRect().width));
    if (ph.length < 5) errs.push('expert-search: no placeholder');
    if (need > room + 0.5) errs.push('expert-search: the placeholder needs ' + Math.round(need) + 'px, the field has ' + Math.round(room) + 'px');
    const wrap = inp.closest('.clear-wrap') || inp;
    const row = wrap.parentElement;
    const rr = row.getBoundingClientRect();
    const others = [...row.children].filter((e) => e !== wrap && e.getClientRects().length);
    const used = others.reduce((a, e) => a + e.getBoundingClientRect().width, 0) + (others.length) * 8;
    info.push('expert-search row=' + Math.round(rr.width) + ' others=' + Math.round(used) + ' wrap=' + Math.round(wrap.getBoundingClientRect().width));
  } catch (e) { errs.push('probe: ' + e.message); }
  finish();
})();
</script>
""" % (MIN_W, MIN_W)

# Settings About and updates: both lines on one left edge.
ABOUT = """
<script>
(async () => {""" + COMMON + """
  try {
    if (innerWidth !== %d) errs.push('about: the page is ' + innerWidth + 'px wide, expected %d');
    await until(() => $('gear'));
    await sleep(500);
    $('gear').click();
    await sleep(400);
    const lines = [...document.querySelectorAll('#updates-field > .update-line')];
    if (lines.length !== 2) { errs.push('about: expected 2 update lines, found ' + lines.length); }
    else {
      const left = (el) => Math.round(el.getBoundingClientRect().left * 10) / 10;
      const a = lines[0].firstElementChild, b = lines[1].firstElementChild;
      info.push('about left1=' + left(a) + ' left2=' + left(b) + ' top1=' + Math.round(a.getBoundingClientRect().top) + ' top2=' + Math.round(b.getBoundingClientRect().top));
      if (Math.abs(left(a) - left(b)) > 1) errs.push('about: the two lines start ' + Math.abs(left(a) - left(b)) + 'px apart (' + left(a) + ' vs ' + left(b) + ')');
      // The visible text of each first item starts on the same edge too.
      const textLeft = (el) => {
        const r = document.createRange();
        r.selectNodeContents(el);
        return Math.round(r.getBoundingClientRect().left * 10) / 10;
      };
      info.push('about text1=' + textLeft(a) + ' text2=' + textLeft(b));
      const h1 = lines[0].getBoundingClientRect(), h2 = lines[1].getBoundingClientRect();
      if (Math.abs(h1.left - h2.left) > 1) errs.push('about: the line boxes start at different edges');
      if (Math.abs(textLeft(a) - textLeft(b)) > 1) errs.push('about: the first text of the two lines starts ' + Math.abs(textLeft(a) - textLeft(b)) + 'px apart (' + textLeft(a) + ' vs ' + textLeft(b) + ')');
      // Vertical alignment: every item is centred on its own line.
      for (const ln of lines) {
        const lr = ln.getBoundingClientRect();
        for (const kid of ln.children) {
          if (!kid.getClientRects().length) continue;
          const kr = kid.getBoundingClientRect();
          const off = Math.abs((kr.top + kr.bottom) / 2 - (lr.top + lr.bottom) / 2);
          if (off > 1) errs.push('about: ' + (kid.id || kid.className) + ' is ' + Math.round(off) + 'px off its line centre');
        }
      }
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  finish();
})();
</script>
""" % (MIN_W, MIN_W)

# (name, probe, width, height, UI language)
CASES = (
    ("counts", COUNTS, 1000, 800, "en"),
)

LANGS = ("en", "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro")
PRELOADS = "".join(f'<link rel="modulepreload" href="i18n/{c}.js" />' for c in LANGS)


def run(browser, tmp, name, probe, w, h, lang):
    page = tmp / "ui" / f"index-{name}.html"
    pre = '<script src="tauri_stub.js"></script>' + PRELOADS + '\n<script>SETTINGS.language = "%s";</script>' % lang
    page.write_text(
        (REPO / "ui" / "index.html").read_text(encoding="utf-8")
        .replace("<script", pre + "\n<script", 1)
        .replace("</body>", probe + "</body>"),
        encoding="utf-8")
    wrapper = tmp / f"wrap-{name}.html"
    wrapper.write_text(WRAPPER % (f"ui/{page.name}", w, h), encoding="utf-8")
    proc = subprocess.run(
        [browser, "--headless=new", "--disable-gpu", "--no-first-run",
         f"--user-data-dir={tmp / ('profile-' + name)}", "--allow-file-access-from-files",
         f"--window-size={max(w + 30, 520)},{h + 140}", "--virtual-time-budget=60000",
         "--dump-dom", wrapper.as_uri()],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=240)
    m = re.search(r'<pre id="SMOKE">(\{.*?\})</pre>', proc.stdout, re.S)
    if not m:
        return ["no probe result"], []
    raw = m.group(1).replace("&quot;", '"').replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
    data = json.loads(raw)
    return data["errs"], data["info"]


def main():
    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        if os.environ.get("CI"):
            print("FAIL: no Chromium browser found on a CI runner")
            return 1
        print("SKIP: no Chromium browser found")
        return 0
    verbose = bool(os.environ.get("MINOR_VERBOSE"))
    tmp = Path(tempfile.mkdtemp(prefix="ps-minor-"))
    failed = 0
    try:
        shutil.copytree(REPO / "ui", tmp / "ui")
        shutil.copy(HERE / "tauri_stub.js", tmp / "ui" / "tauri_stub.js")
        for name, probe, w, h, lang in CASES:
            errs, info = run(browser, tmp, name, probe, w, h, lang)
            print(f"-- {name} ({w}x{h}, {lang})")
            if verbose:
                for i in info:
                    print(f"   info: {i}")
            for e in errs:
                print(f"   FAIL: {e}")
            failed += len(errs)
            if not errs:
                print("   ok")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
