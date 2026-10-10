"""Every <select> in every window wears the themed, rounded list popup.

The closed control is styled globally, but a <select> that never went through
attachSelectPicker() (ui/selectpop.js) still opens the square native OS list.
This boots index.html, video.html and camera.html in headless Chromium, opens the
surfaces that build selects on demand (expert tabs, the statistics export, the
screenshot editor's text tool, the video editor's text and keys overlays), and
asserts that every <select> carries the marker attribute set by the picker and
has an 8 px border radius on all four corners. It also drives the keyboard path
(Enter / Space / Alt+Down open, arrows move, Escape closes, focus stays on the
select) and checks that a disabled select never opens.

Run:  python tests/ui/select_picker_test.py
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

# Shared by the three probes: the audit itself and the result hand-off.
COMMON = """
  const errs = [];
  const seen = new Set();
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 80) && !fn(); i++) await sleep(50);
    return fn();
  };
  const describe = (s) => (s.id ? '#' + s.id : s.tagName.toLowerCase() + '.' + (String(s.className).trim().split(/\\s+/).join('.') || '?'));
  // Runs after every surface is opened; a select is reported once, where it
  // was first seen.
  const audit = (where) => {
    for (const s of document.querySelectorAll('select')) {
      if (seen.has(s)) continue;
      seen.add(s);
      const cs = getComputedStyle(s);
      const radii = [cs.borderTopLeftRadius, cs.borderTopRightRadius, cs.borderBottomLeftRadius, cs.borderBottomRightRadius];
      if (!s.dataset.selectPicker) errs.push('no picker attached: ' + describe(s) + ' (' + where + ')');
      if (radii.some((r) => parseFloat(r) !== 8)) errs.push('radius ' + radii[0] + ' instead of 8px: ' + describe(s) + ' (' + where + ')');
    }
  };
  const key = (el, k, opts) => el.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ key: k, bubbles: true, cancelable: true }, opts || {})));
  const popOpen = () => document.querySelector('.select-pop:not(.hidden)');
  // Focusing a select can scroll its panel, and the scroll event reaches the page
  // a frame later (the list closes on a foreign scroll). Let the queued events
  // drain before a key press, so a slow machine cannot close the list under it.
  const frame = () => new Promise((r) => { requestAnimationFrame(() => r()); setTimeout(r, 120); });
  const settle = async () => { await frame(); await frame(); await sleep(100); };
  // Keyboard contract on one select: opens with Enter / Space / Alt+Down,
  // arrows move, Enter picks, Escape closes, focus stays on the select.
  const keyboardCheck = async (sel, label) => {
    if (sel.options.length < 3) { errs.push(label + ': need three options for the keyboard check'); return; }
    let changes = 0;
    sel.addEventListener('change', () => changes++);
    sel.selectedIndex = 0;
    sel.focus();
    await settle();
    for (const [k, o] of [['Enter', {}], [' ', {}], ['ArrowDown', { altKey: true }]]) {
      key(sel, k, o);
      await until(popOpen, 100);
      if (!popOpen()) { errs.push(label + ': ' + k + ' did not open the list'); continue; }
      key(sel, 'Escape');
      await until(() => !popOpen(), 100);
      if (popOpen()) errs.push(label + ': Escape did not close the list');
      if (document.activeElement !== sel) errs.push(label + ': focus left the select after Escape');
    }
    await settle();
    key(sel, 'Enter');
    await until(popOpen, 100);
    key(sel, 'ArrowDown');
    key(sel, 'ArrowDown');
    const lit = popOpen() && popOpen().querySelector('button.kb');
    if (!lit) errs.push(label + ': arrows did not highlight a row');
    key(sel, 'Enter');
    await until(() => !popOpen(), 100);
    if (popOpen()) errs.push(label + ': Enter did not close the list after picking');
    if (sel.selectedIndex !== 2) errs.push(label + ': two ArrowDown + Enter picked index ' + sel.selectedIndex + ' instead of 2');
    if (changes !== 1) errs.push(label + ': change fired ' + changes + ' times instead of once');
    if (document.activeElement !== sel) errs.push(label + ': focus left the select after picking');
    sel.selectedIndex = 0;
  };
  const finish = () => {
    errs.push(...(window.__SMOKE_ERRORS__ || []));
    const pre = document.createElement('pre');
    pre.id = 'SMOKE';
    pre.textContent = JSON.stringify(errs);
    document.body.appendChild(pre);
  };
"""

INDEX_PROBE = """
<script>
(async () => {""" + COMMON + """
  try {
    await until(() => document.getElementById('gear') && document.getElementById('library-btn'));
    await sleep(400);
    audit('boot');
    // Settings and the expert menu: every tab builds its own dropdown rows.
    document.getElementById('gear').click();
    await sleep(200);
    audit('settings');
    document.getElementById('expert-open').click();
    await sleep(300);
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click();
      await sleep(150);
      audit('expert tab ' + tb.textContent.trim());
    }
    document.getElementById('expert-back').click();
    await sleep(200);
    // Statistics export lives in the backup manager.
    document.getElementById('settings-backup-btn').click();
    await until(() => document.getElementById('stats-export-format'), 60);
    await sleep(200);
    if (!document.getElementById('stats-export-format')) errs.push('the statistics export select never appeared');
    audit('backup manager');
    document.getElementById('backup-modal').classList.add('hidden');
    document.getElementById('settings-close').click();
    await sleep(200);
    // Library: the sort select carries the keyboard check.
    document.getElementById('library-btn').click();
    await sleep(400);
    audit('library');
    await keyboardCheck(document.getElementById('library-sort'), 'library sort');
    // Screenshot editor: the text tool builds selects through a helper.
    const TINY_PNG = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
    window.__FIRE__('snip-captured', { data_url: TINY_PNG, path: '', name: 'test.png' });
    await until(() => { const e = document.getElementById('snip-edit'); return e && !e.classList.contains('hidden'); }, 60);
    await sleep(200);
    for (const tool of ['text', 'rect', 'arrow', 'pen']) {
      const b = document.querySelector('#snip-edit-tools button[data-tool="' + tool + '"]');
      if (b) { b.click(); await sleep(120); }
      audit('screenshot editor ' + tool);
      if (tool === 'text' && !document.querySelector('.snip-edit-select')) errs.push('the screenshot editor built no select for the text tool');
    }
    audit('final');
  } catch (e) { errs.push('probe: ' + e.message); }
  finish();
})();
</script>
"""

VIDEO_PROBE = """
<script>
(async () => {""" + COMMON + """
  try {
    await until(() => document.getElementById('ve-stage-msg').classList.contains('hidden') &&
      document.querySelectorAll('#ve-filmstrip .ve-thumb').length > 0, 300);
    await sleep(200);
    audit('boot');
    document.getElementById('ve-tab-overlays').click();
    await sleep(100);
    for (const id of ['ve-ov-add-text', 've-ov-add-keys']) {
      document.getElementById(id).click();
      await sleep(80);
      await sleep(150);
      // A new overlay selects itself; a row click would toggle it back off.
      if (document.getElementById('ve-ov-editor').classList.contains('hidden')) {
        const rows = [...document.querySelectorAll('#ve-ov-list .ve-ov-row')];
        if (rows.length) rows[rows.length - 1].click();
        await sleep(150);
      }
      audit('overlay panel ' + id);
      if (!document.querySelector('#ve-ov-editor select')) errs.push('the overlay editor built no select for ' + id);
    }
    document.getElementById('ve-export-open').click();
    await sleep(150);
    audit('export tab');
    await keyboardCheck(document.getElementById('ve-export-format'), 'video export format');
    audit('final');
  } catch (e) { errs.push('probe: ' + e.message); }
  finish();
})();
</script>
"""

CAMERA_STUB = """
<script>
if (!navigator.mediaDevices) { navigator.mediaDevices = {}; }
navigator.mediaDevices.getUserMedia = () => Promise.reject(new DOMException('no camera', 'NotFoundError'));
navigator.mediaDevices.enumerateDevices = () => Promise.resolve([]);
</script>
"""

CAMERA_PROBE = """
<script>
(async () => {""" + COMMON + """
  try {
    await until(() => !document.getElementById('cam-no-device').classList.contains('hidden'));
    await sleep(200);
    audit('boot');
    await keyboardCheck(document.getElementById('cam-res-select'), 'camera resolution');
    // A locked select (recording in flight) must stay shut.
    const res = document.getElementById('cam-res-select');
    res.disabled = true;
    res.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
    await settle();
    if (popOpen()) errs.push('a disabled select opened the list');
    key(res, 'Enter');
    await settle();
    if (popOpen()) errs.push('a disabled select opened the list on Enter');
    res.disabled = false;
    audit('final');
  } catch (e) { errs.push('probe: ' + e.message); }
  finish();
})();
</script>
"""


def run(browser, tmp, name, probe, size, budget, pre=""):
    side = tmp / "ui" / name
    side.write_text(
        (REPO / "ui" / name).read_text(encoding="utf-8")
        .replace("<script", '<script src="tauri_stub.js"></script>\n' + pre + "<script", 1)
        .replace("</body>", probe + "</body>"),
        encoding="utf-8")
    proc = subprocess.run(
        [browser, "--headless=new", "--disable-gpu", "--no-first-run",
         f"--user-data-dir={tmp / ('profile-' + name)}", "--allow-file-access-from-files",
         f"--window-size={size[0]},{size[1]}", f"--virtual-time-budget={budget}",
         "--dump-dom", side.as_uri()],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=240)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    if not m:
        return ["no probe result"]
    raw = m.group(1).replace("&quot;", '"').replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
    return json.loads(raw)


def main():
    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        if os.environ.get("CI"):
            print("FAIL: no Chromium browser found on a CI runner")
            return 1
        print("SKIP: no Chromium browser found")
        return 0
    tmp = Path(tempfile.mkdtemp(prefix="ps-selpop-"))
    failed = 0
    try:
        shutil.copytree(REPO / "ui", tmp / "ui")
        shutil.copy(HERE / "tauri_stub.js", tmp / "ui" / "tauri_stub.js")
        for name, probe, size, budget, pre in (
                ("index.html", INDEX_PROBE, (1280, 900), 60000, ""),
                ("video.html", VIDEO_PROBE, (1130, 860), 40000, ""),
                ("camera.html", CAMERA_PROBE, (420, 620), 20000, CAMERA_STUB)):
            errs = run(browser, tmp, name, probe, size, budget, pre)
            print(f"-- {name}")
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
