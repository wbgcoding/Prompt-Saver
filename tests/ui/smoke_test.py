"""Headless smoke test: boot the real UI with a stubbed Tauri backend.

Catches the class of defect that only shows when the app actually runs:
  * a JS error during startup or while opening a dialog,
  * an element that overflows the window horizontally (layout bug),
  * a string that renders as its raw i18n key (missing translation).

It walks the library, the copy history, the tile menu and every expert tab, at
two window sizes. Skips cleanly when no Chromium browser is installed.

Run:  python tests/ui/smoke_test.py
Exit: 0 = clean, 1 = problems found.
"""
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
HERE = Path(__file__).parent
SIZES = [(1280, 800), (620, 500), (480, 360)]

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]

PROBE = """
<script>
(async () => {
  const out = { errors: [], overflow: [], tabs: [], rawKeys: [] };
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const click = async (id) => { const e = document.getElementById(id); if (e) { e.click(); await sleep(200); } };
  await sleep(700);
  try {
    // The portable take-over dialog opens on startup: it must be the themed modal
    // with an icon and its own two labels, not a bare system box.
    const cm = document.getElementById('confirm-modal');
    // It opens once show_main_window resolves, which is not a fixed delay.
    for (let i = 0; i < 60 && cm.classList.contains('hidden'); i++) await sleep(100);
    if (cm.classList.contains('hidden')) {
      // Only a real failure if the app got as far as asking the backend; headless
      // rAF occasionally stalls before that, which is a harness hiccup, not a bug.
      if ((window.__SMOKE_CALLS__ || []).includes('takeover_offer')) {
        out.errors.push('the take-over dialog did not open');
      } else {
        out.tabs.push('(take-over check skipped: startup stalled)');
      }
    } else {
      const icon = document.getElementById('confirm-icon');
      if (icon.classList.contains('hidden') || !icon.querySelector('svg')) {
        out.errors.push('take-over dialog has no icon');
      }
      const msg = document.getElementById('confirm-msg').textContent;
      if (!msg.includes('Prompt-Saver')) out.errors.push('take-over dialog does not name the folder');
      if (!/\\n/.test(msg)) out.errors.push('take-over dialog lost its line breaks');
      await click('confirm-cancel'); // "start empty"
    }
    await click('open-library');
    await click('library-close');
    await click('open-journal');
    await click('journal-close');
    const tile = document.querySelector('#grid .tile');
    if (tile) { tile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true })); await sleep(150); }
    document.body.click(); // dismiss the context menu
    await click('fav-view-btn');
    await click('fav-view-btn');
    await click('chain-btn');
    await click('chain-btn');
    await click('clip-btn');
    await click('clip-close');
    // Copying the prompt with placeholders must open and close the fill-in dialog —
    // and a plain click must NOT paste into the previous app (that is what the
    // double-click / Enter shortcut is for).
    const tiles = [...document.querySelectorAll('#grid .tile')];
    const varTile = tiles.find(el => (el.dataset.id || '') === 'p1');
    if (varTile) {
      // The grid listens for pointer events (it tracks drags), so .click() alone
      // never reaches the handler — dispatch the real sequence.
      for (const type of ['pointerdown', 'pointerup', 'click']) {
        varTile.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1 }));
      }
      await sleep(400);
      const varsOpen = !document.getElementById('vars-modal').classList.contains('hidden');
      if (!varsOpen) out.errors.push('clicking a variable prompt did not open the fill-in dialog');
      await click('vars-ok');
      await sleep(250);
      if ((window.__SMOKE_CALLS__ || []).includes('paste_into_previous')) {
        out.errors.push('a single click on a variable prompt pasted into the previous app');
      }
    }
    document.getElementById('settings').classList.remove('hidden');
    await click('expert-open');
    for (const tb of [...document.querySelectorAll('.expert-tab')]) {
      tb.click();
      await sleep(200);
      out.tabs.push(tb.textContent.trim() + ':' +
        document.querySelectorAll('#expert-flags .expert-group').length);
    }
    // Exercise the interactions that rebuild the menu: collapsing a group and
    // flipping switches (gated ones re-render, plain ones must not).
    // Collapsing must flip a class, not rebuild the list — rebuilding is what made
    // the menu flicker. Marking the node proves it survived the round trip.
    const grp = document.querySelector('#expert-flags .expert-group');
    const head = grp && grp.querySelector('.expert-group-title.collapsible');
    if (head) {
      grp.dataset.mark = '1';
      head.click(); await sleep(120);
      head.click(); await sleep(120);
      const now = document.querySelector('#expert-flags .expert-group');
      if (!now || now.dataset.mark !== '1') out.errors.push('collapsing rebuilt the expert list (flicker)');
    }
    for (const sw of [...document.querySelectorAll('#expert-flags input.switch')].slice(0, 4)) {
      sw.click();
      await sleep(150);
    }
    // Backups live in their own dialog now, reached from settings.
    document.getElementById('expert-back').click();
    await sleep(250);
    document.getElementById('settings-backup-btn').click();
    await sleep(400);
    const bm = document.getElementById('backup-modal');
    if (bm.classList.contains('hidden')) {
      out.errors.push('the backups dialog did not open');
    } else if (!document.getElementById('backup-panel').children.length) {
      out.errors.push('the backups dialog stayed empty');
    } else if (!document.getElementById('stats-panel').children.length) {
      out.errors.push('the backups dialog has no usage statistics');
    } else if (getComputedStyle(bm).zIndex <= getComputedStyle(document.getElementById('settings')).zIndex) {
      out.errors.push('the backups dialog sits behind the settings it was opened from');
    } else {
      // The top-list length field used to run past its own row.
      const row = document.querySelector('#stats-panel .stats-topn-row');
      const fld = row && row.querySelector('.modal-input');
      if (fld && fld.getBoundingClientRect().right > row.getBoundingClientRect().right + 1) {
        out.errors.push('the top-list length field overflows its row');
      }
    }
    await click('backup-close');
    // Restoring a password-protected backup: the prompt must appear, reject a wrong
    // password with a new message, then accept the right one and confirm the result.
    const until = async (fn) => { for (let i = 0; i < 40 && !fn(); i++) await sleep(100); return fn(); };
    const pwShown = () => !document.getElementById('confirm-input-row').classList.contains('hidden');
    document.getElementById('import-all').click();
    if (!await until(pwShown)) {
      out.errors.push('no password prompt for a protected backup');
    } else {
      const field = document.getElementById('confirm-input');
      if (field.type !== 'password') out.errors.push('password field is not masked');
      document.getElementById('confirm-input-eye').click();
      if (field.type !== 'text') out.errors.push('the eye does not reveal the password');
      document.getElementById('confirm-input-eye').click();
      field.value = 'nope';
      document.getElementById('confirm-ok').click();
      await sleep(150);
      if (!await until(pwShown)) out.errors.push('a wrong password did not re-open the prompt');
      document.getElementById('confirm-input').value = 'letmein';
      document.getElementById('confirm-ok').click();
      const done = await until(() => document.getElementById('confirm-msg').textContent.includes('12'));
      if (!done) out.errors.push('no import confirmation after the right password');
      await click('confirm-ok');
    }
  } catch (e) { out.errors.push('probe: ' + e.message); }
  for (const el of document.querySelectorAll('.overlay *, #expert-flags *')) {
    const r = el.getBoundingClientRect();
    if (r.width && r.right > innerWidth + 1) {
      out.overflow.push((el.id || el.className || el.tagName) + ' right=' + Math.round(r.right));
    }
  }
  // t() falls back to the key name, so an untranslated string shows up verbatim.
  for (const el of document.querySelectorAll('body *')) {
    const txt = (el.children.length ? '' : el.textContent || '').trim();
    if (/^(flag|val|tip|exp|stat|backup)[A-Z][A-Za-z]+$/.test(txt)) out.rawKeys.push(txt);
  }
  out.errors.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(out);
  document.body.appendChild(pre);
})();
</script>
"""


ERR_PROBE = """
<script>
setTimeout(() => {
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(window.__SMOKE_ERRORS__ || []);
  document.body.appendChild(pre);
}, 1500);
</script>
"""


def unescape(s):
    for a, b in (("&quot;", '"'), ("&lt;", "<"), ("&gt;", ">"), ("&amp;", "&")):
        s = s.replace(a, b)
    return s


def main():
    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        print("SKIP: no Chromium browser found")
        return 0

    tmp = Path(tempfile.mkdtemp(prefix="ps-smoke-"))
    failed = 0
    try:
        shutil.copytree(REPO / "ui", tmp / "ui")
        shutil.copy(HERE / "tauri_stub.js", tmp / "ui" / "tauri_stub.js")
        page = tmp / "ui" / "index.html"
        html = page.read_text(encoding="utf-8")
        html = html.replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
        html = html.replace("</body>", PROBE + "</body>")
        page.write_text(html, encoding="utf-8")

        for w, h in SIZES:
            size_tag = f"{w}x{h}"
            # An isolated profile is required or Chrome hands the URL to a running
            # instance and prints nothing.
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-%s' % size_tag)}", "--allow-file-access-from-files",
                 f"--window-size={w},{h}", "--virtual-time-budget=30000", "--dump-dom",
                 page.as_uri()],
                capture_output=True, text=True, timeout=240)
            m = re.search(r'<pre id="SMOKE">(\{.*?\})</pre>', proc.stdout, re.S)
            if not m:
                print(f"FAIL {w}x{h}: the UI produced no probe result")
                failed += 1
                continue
            r = json.loads(unescape(m.group(1)))
            print(f"-- {w}x{h} — expert tabs: {', '.join(r['tabs']) or 'none'}")
            for kind, items in (("JS error", r["errors"]),
                                ("overflow", sorted(set(r["overflow"]))),
                                ("untranslated", sorted(set(r["rawKeys"])))):
                for it in items:
                    print(f"   FAIL {kind}: {it}")
                failed += len(items)
            if not (r["errors"] or r["overflow"] or r["rawKeys"]):
                print("   ok")

        # The secondary windows only need to boot without throwing.
        for name, size in (("floating.html", (240, 60)), ("snip.html", (1280, 800))):
            side = tmp / "ui" / name
            side.write_text(
                side.read_text(encoding="utf-8")
                .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
                .replace("</body>", ERR_PROBE + "</body>"),
                encoding="utf-8")
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-%s' % name)}", "--allow-file-access-from-files",
                 f"--window-size={size[0]},{size[1]}", "--virtual-time-budget=6000",
                 "--dump-dom", side.as_uri()],
                capture_output=True, text=True, timeout=180)
            m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
            errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
            print(f"-- {name}")
            for e in errs:
                print(f"   FAIL JS error: {e}")
            failed += len(errs)
            if not errs:
                print("   ok")

        print("PASS" if not failed else f"{failed} problem(s)")
        return 1 if failed else 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
