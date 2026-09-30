"""Where an expert row sits: its tab, its group and how far it is indented.

The menu contract (docs/DESIGN.md, "Expert menu") says a setting lives next to
the feature it tunes and steps in under the switch that unlocks it. This boots
index.html in headless Chromium against the stub backend, opens the expert menu
and, for every row in EXPECTED, finds it, reads its tab, its group title and its
`--expert-depth`, and compares them with the table. A second boot with the
parent switches off proves the same rows disappear with their switch. The help
texts are grepped for the group names that no longer exist.

Run:  python tests/ui/expert_rows_test.py
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

# kind, key, tab title key, group title key, depth (0 = unindented, n = at least n)
EXPECTED = [
    ("flag", "chainPrompts", "expTabFeatures", "expGroupChain", 0),
    ("flag", "chainLock", "expTabFeatures", "expGroupChain", 1),
    ("flag", "chainQuickStart", "expTabFeatures", "expGroupChain", 1),
    ("flag", "chainStartAlwaysVisible", "expTabFeatures", "expGroupChain", 2),
    ("flag", "chainVarCheck", "expTabFeatures", "expGroupChain", 1),
    ("flag", "gridViewBorder", "expTabFeatures", "expGroupViews", 1),
    ("value", "maxViews", "expTabFeatures", "expGroupViews", 1),
    ("value", "gridMax", "expTabFeatures", "expGroupViews", 0),
    ("flag", "recSweepBroken", "expTabCapture", "expGroupRecording", 1),
    ("value", "recBrokenGraceMin", "expTabCapture", "expGroupRecording", 2),
    ("value", "recSweepEveryMin", "expTabCapture", "expGroupRecording", 2),
    ("value", "recDiscardKeepH", "expTabCapture", "expGroupRecording", 1),
    ("select", "recordingRetentionDays", "expTabCapture", "expGroupRecording", 1),
    ("action", "rec-sweep-now", "expTabCapture", "expGroupRecording", 2),
    ("value", "videoClipTtlMin", "expTabCapture", "expGroupVideoEditor", 1),
    ("action", "ocr-index-backfill", "expTabLibrary", "expGroupLibrary", 1),
    ("action", "float-recall", "expTabFeatures", "expGroupFolders", 1),
    ("dropdown", "dateStyle", "expTabFeatures", "expGroupVars", 1),
    ("dropdown", "timeStyle", "expTabFeatures", "expGroupVars", 1),
    ("varFixedList", "", "expTabFeatures", "expGroupVars", 2),
    ("value", "errorLogKeepKb", "expTabSystem", "expGroupData", 1),
    ("action", "error-log-open", "expTabSystem", "expGroupData", 1),
    ("color", "snipDefaultColor", "expTabCapture", "expGroupSnipEditor", 1),
    ("color", "veOvColor", "expTabCapture", "expGroupVideoEditor", 1),
]

# Rows that stay on screen with every gate off (they have no parent switch that
# is in the GATES list below).
STAYS = ["flag:chainPrompts", "value:gridMax"]

# The parent switches the rows above hang off.
GATES = ["chainQuickStart", "multiView", "screenRecord", "recSweepBroken", "videoEditor",
         "ocrIndex", "floating", "promptVars", "dynVars", "varFixed", "errorLog", "screenshot", "snipEditor"]

FIND = """
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 80) && !fn(); i++) await sleep(50);
    return fn();
  };
  // Every tab in turn; returns one entry per row found, where it was found.
  const scan = async (wanted) => {
    const found = {};
    const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
    for (const tb of tabs) {
      tb.click();
      await sleep(120);
      for (const w of wanted) {
        const id = w.kind + ':' + w.key;
        let el = null;
        if (w.kind === 'flag') {
          el = document.querySelector('#expert-flags .switch-field[data-setting-row="' + w.key + '"]');
        } else if (w.kind === 'action') {
          el = document.querySelector('#expert-flags [data-action="' + w.key + '"]');
        } else if (w.kind === 'varFixedList') {
          el = document.querySelector('#expert-flags .var-fixed-field');
        } else {
          el = document.querySelector('#expert-flags [data-setting-row="' + w.key + '"]');
        }
        if (!el) continue;
        const holder = el.closest('.expert-sub');
        const depth = holder ? Number(holder.style.getPropertyValue('--expert-depth') || 0) : 0;
        const group = el.closest('.expert-group');
        const title = group && group.querySelector('.expert-group-title span');
        found[id] = { tab: tb.textContent.trim(), group: title ? title.textContent.trim() : '', depth: depth };
      }
    }
    return found;
  };
  const finish = (errs) => {
    errs.push(...(window.__SMOKE_ERRORS__ || []));
    const pre = document.createElement('pre');
    pre.id = 'SMOKE';
    pre.textContent = JSON.stringify(errs);
    document.body.appendChild(pre);
  };
"""

PROBE_ON = """
<script>
(async () => {""" + FIND + """
  const errs = [];
  try {
    await until(() => document.getElementById('gear') && document.getElementById('library-btn'));
    await sleep(400);
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('expert-open').click();
    await sleep(300);
    const expected = __EXPECTED__;
    const found = await scan(expected.map((e) => ({ kind: e[0], key: e[1] })));
    for (const [kind, key, tabKey, groupKey, depth] of expected) {
      const f = found[kind + ':' + key];
      const what = kind + ' ' + (key || '(list)');
      if (!f) { errs.push(what + ': not rendered with its parent switch on'); continue; }
      if (f.tab !== tabKey) errs.push(what + ': in tab ' + f.tab + ', expected ' + tabKey);
      if (f.group !== groupKey) errs.push(what + ': in group ' + f.group + ', expected ' + groupKey);
      if (depth === 0 ? f.depth !== 0 : f.depth < depth) {
        errs.push(what + ': depth ' + f.depth + ', expected ' + (depth === 0 ? '0' : 'at least ' + depth));
      }
    }
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""

PROBE_OFF = """
<script>
(async () => {""" + FIND + """
  const errs = [];
  try {
    await until(() => document.getElementById('gear') && document.getElementById('library-btn'));
    await sleep(400);
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('expert-open').click();
    await sleep(300);
    const expected = __EXPECTED__;
    const found = await scan(expected.map((e) => ({ kind: e[0], key: e[1] })));
    const stays = new Set(__STAYS__);
    for (const [kind, key] of expected) {
      const id = kind + ':' + key;
      if (found[id] && !stays.has(id)) errs.push(id + ': still rendered with its parent switch off');
    }
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""


def run(browser, tmp, name, probe, seed, size=(1280, 900), budget=60000):
    side = tmp / "ui" / name
    pre = "<script>window.__SEED_SETTINGS__ = " + json.dumps(seed) + ";</script>\n"
    side.write_text(
        (REPO / "ui" / name).read_text(encoding="utf-8")
        .replace("<script", pre + '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", probe + "</body>"),
        encoding="utf-8")
    proc = subprocess.run(
        [browser, "--headless=new", "--disable-gpu", "--no-first-run",
         f"--user-data-dir={tmp / ('profile-' + name + str(abs(hash(probe)) % 9999))}",
         "--allow-file-access-from-files",
         f"--window-size={size[0]},{size[1]}", f"--virtual-time-budget={budget}",
         "--dump-dom", side.as_uri()],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=240)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    if not m:
        return ["no probe result"]
    raw = m.group(1).replace("&quot;", '"').replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
    return json.loads(raw)


def english_titles():
    """Tab and group title key -> English text, read from the en block of i18n.js."""
    text = (REPO / "ui" / "i18n.js").read_text(encoding="utf-8")
    en = re.search(r"\n  en: \{(.*?)\n  de: \{", text, re.S).group(1)
    return dict(re.findall(r'^    ((?:expTab|expGroup)\w+): "([^"]*)"', en, re.M))


def source_problems():
    """Help texts must not name a group that no longer exists."""
    out = []
    for rel in ("ui/help.js", "docs/help.md", "README.md", "ui/i18n.js"):
        text = (REPO / rel).read_text(encoding="utf-8")
        for old in ("Tiles & copying", "Kacheln & Kopieren", "Tiles &amp; copying"):
            if old in text:
                out.append(f"{rel} still names the old group '{old}'")
    return out


def main():
    problems = source_problems()
    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        for p in problems:
            print("FAIL:", p)
        if os.environ.get("CI"):
            print("FAIL: no Chromium browser found on a CI runner")
            return 1
        print("SKIP browser part: no Chromium browser found")
        return 1 if problems else 0
    tmp = Path(tempfile.mkdtemp(prefix="ps-exprows-"))
    try:
        shutil.copytree(REPO / "ui", tmp / "ui")
        shutil.copy(HERE / "tauri_stub.js", tmp / "ui" / "tauri_stub.js")
        titles = english_titles()
        expected = json.dumps([[k, key, titles[tab], titles[grp], d] for k, key, tab, grp, d in EXPECTED])
        on = {"ui_flags": {g: True for g in GATES + ["chainPrompts"]}}
        off = {"ui_flags": {g: False for g in GATES + ["chainPrompts"]}}
        for label, probe, seed in (("switches on", PROBE_ON, on), ("switches off", PROBE_OFF, off)):
            probe = probe.replace("__EXPECTED__", expected).replace("__STAYS__", json.dumps(STAYS))
            errs = run(browser, tmp, "index.html", probe, seed)
            problems += [f"[{label}] {e}" for e in errs]
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    for p in problems:
        print("FAIL:", p)
    print("PASS" if not problems else f"{len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
