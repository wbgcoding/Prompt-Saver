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
sys.path.insert(0, str(REPO / "tests"))
from i18n_files import combined_source  # noqa: E402
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
    ("flag", "chainVarCheck", "expTabFeatures", "expGroupChain", 1),
    ("flag", "gridViewBorder", "expTabFeatures", "expGroupViews", 1),
    ("value", "maxViews", "expTabFeatures", "expGroupViews", 1),
    ("value", "gridMax", "expTabFeatures", "expGroupViews", 0),
    ("flag", "recSweepBroken", "expTabCapture", "expGroupRecWhile", 0),
    ("value", "recDiscardKeepH", "expTabCapture", "expGroupRecording", 1),
    ("select", "recordingRetentionDays", "expTabCapture", "expGroupRecording", 1),
    ("action", "rec-sweep-now", "expTabCapture", "expGroupRecWhile", 1),
    ("value", "videoClipTtlMin", "expTabCapture", "expGroupVideoEditor", 1),
    ("action", "ocr-index-backfill", "expTabLibrary", "expGroupLibSearch", 1),
    ("action", "float-recall", "expTabFeatures", "expGroupFolders", 1),
    ("dropdown", "dateStyle", "expTabFeatures", "expGroupVars", 1),
    ("dropdown", "timeStyle", "expTabFeatures", "expGroupVars", 1),
    ("varFixedList", "", "expTabFeatures", "expGroupVars", 2),
    ("value", "errorLogKeepKb", "expTabSystem", "expGroupData", 1),
    ("action", "error-log-open", "expTabSystem", "expGroupData", 1),
    ("color", "snipDefaultColor", "expTabCapture", "expGroupSnipEditor", 1),
    ("color", "veOvColor", "expTabCapture", "expGroupVideoEditor", 1),
    ("flag", "recOutline", "expTabCapture", "expGroupRecWhile", 0),
    ("color", "recOutlineColor", "expTabCapture", "expGroupRecWhile", 1),
    ("value", "cameraFramerate", "expTabCapture", "expGroupCamera", 1),
    ("dropdown", "chainSeparator", "expTabFeatures", "expGroupChain", 1),
    ("value", "searchListMax", "expTabLibrary", "expGroupLibSearch", 0),
    ("value", "veOvTextOutlinePx", "expTabCapture", "expGroupVideoEditor", 1),
    ("color", "veOvKeysActiveColor", "expTabCapture", "expGroupVideoEditor", 1),
    ("value", "veOvVideoVolume", "expTabCapture", "expGroupVideoEditor", 1),
]

# Rows that stay on screen with every gate off (they have no parent switch that
# is in the GATES list below).
STAYS = ["flag:chainPrompts", "value:gridMax", "value:searchListMax"]

# The parent switches the rows above hang off.
GATES = ["chainQuickStart", "multiView", "screenRecord", "recSweepBroken", "videoEditor",
         "ocrIndex", "floating", "promptVars", "dynVars", "varFixed", "errorLog", "screenshot", "snipEditor",
         "globalSearch", "recOutline", "cameraRecord"]

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


# bubbleMs: a value whose 0 is its off stop gates the rows that depend on it, like a flag.
VALUE_GATE_PROBE = """
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
    const found = await scan([{ kind: 'dropdown', key: 'copySize' }, { kind: 'dropdown', key: 'copyFont' }]);
    for (const id of ['dropdown:copySize', 'dropdown:copyFont']) {
      if (!!found[id] !== __WANT__) errs.push(id + ' is ' + (found[id] ? 'shown' : 'hidden') + ' with the bubble time __BUBBLE__');
    }
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""


# The expert dialog follows the window: wide window = wide dialog with groups side by side,
# narrow window = one column.
LAYOUT_PROBE = """
<script>
(async () => {""" + FIND + """
  const errs = [];
  try {
    await until(() => document.getElementById('gear') && document.getElementById('library-btn'));
    await sleep(400);
    document.getElementById('gear').click();
    await sleep(200);
    const langSel = document.getElementById('lang-select');
    langSel.value = 'de';
    langSel.dispatchEvent(new Event('change'));
    await sleep(300);
    document.getElementById('expert-open').click();
    await sleep(400);
    const tabEls = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
    if (tabEls.length !== 6) errs.push('expected six expert tabs, found ' + tabEls.length);
    if (new Set(tabEls.map((b) => Math.round(b.getBoundingClientRect().top))).size !== 1) {
      errs.push('the six expert tabs are not on one line');
    }
    const tabBarEl = document.querySelector('#expert-tabs-bar .expert-tabs');
    if (__WIDE__ && tabBarEl.scrollWidth > tabBarEl.clientWidth) errs.push('wide window: a tab is clipped');
    const modal = document.querySelector('#settings .modal');
    const groups = [...document.querySelectorAll('#expert-flags > .expert-group')];
    if (groups.length < 2) errs.push('fewer than two groups in the first tab');
    else {
      const lefts = new Set(groups.map((g) => Math.round(g.getBoundingClientRect().left)));
      const w = Math.round(modal.getBoundingClientRect().width);
      if (__WIDE__) {
        if (w < 1200) errs.push('wide window: dialog is ' + w + 'px, expected at least 1200');
        const l0 = Math.round(groups[0].getBoundingClientRect().left);
        const l1 = Math.round(groups[1].getBoundingClientRect().left);
        if (l0 === l1) errs.push('wide window: the first two groups share the left edge ' + l0);
      } else if (lefts.size !== 1) {
        errs.push('narrow window: groups have ' + lefts.size + ' different left edges');
      }
    }
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""


# Number rows read in seconds, words and megabytes, and a zero hour reads Never (German decimals).
VALUE_TEXT_PROBE = """
<script>
(async () => {""" + FIND + """
  const errs = [];
  try {
    await until(() => document.getElementById('gear') && document.getElementById('library-btn'));
    await sleep(400);
    document.getElementById('gear').click();
    await sleep(200);
    const langSel = document.getElementById('lang-select');
    langSel.value = 'de';
    langSel.dispatchEvent(new Event('change'));
    await sleep(300);
    document.getElementById('expert-open').click();
    await sleep(400);
    const want = {
      copyCooldownMs: '2 s',
      autoPasteDelayMs: '0,08 s',
      attachPreviewKb: '≈ 1.360 Wörter',
      errorLogKeepKb: '0,25 MB',
      floatParkCleanupH: 'Nie',
    };
    const got = {};
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click();
      await sleep(80);
      for (const key of Object.keys(want)) {
        const out = document.querySelector('#expert-flags [data-setting-row="' + key + '"] .value-out');
        if (out) got[key] = out.textContent.trim();
      }
    }
    for (const key of Object.keys(want)) {
      if (got[key] !== want[key]) errs.push(key + ' reads ' + JSON.stringify(got[key]) + ', expected ' + JSON.stringify(want[key]));
    }
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""


# ---- consumers: a non-default value reaches the code that uses it ----------------
# Probe JS avoids apostrophes and backslashes (the strings are plain Python).

FINISH = """
  const finish = (errs) => {
    errs.push(...(window.__SMOKE_ERRORS__ || []));
    const pre = document.createElement('pre');
    pre.id = 'SMOKE';
    pre.textContent = JSON.stringify(errs);
    document.body.appendChild(pre);
  };
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 80) && !fn(); i++) await sleep(50);
    return fn();
  };
"""

# hoverDim / hoverLift: one strength drives the dimming, one distance the lift, 0 is off.
HOVER_PROBE = """
<script>
(async () => {""" + FINISH + """
  const errs = [];
  try {
    await until(() => document.querySelectorAll('#grid .tile').length > 6);
    await sleep(400);
    const dimOn = document.body.classList.contains('hover-dim');
    const st = document.documentElement.style;
    const opacity = st.getPropertyValue('--hover-dim-opacity');
    const lift = st.getPropertyValue('--hover-lift');
    if (dimOn !== __DIMON__) errs.push('hover-dim class is ' + dimOn + ', expected __DIMON__');
    if (opacity !== '__OPACITY__') errs.push('dim opacity is ' + opacity + ', expected __OPACITY__');
    if (lift !== '__LIFT__') errs.push('hover lift is ' + lift + ', expected __LIFT__');
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""


# tileStyle: one choice for the tile text look; exactly the chosen style class is on the body.
STYLE_PROBE = """
<script>
(async () => {""" + FINISH + """
  const errs = [];
  try {
    await until(() => document.querySelectorAll('#grid .tile').length > 6);
    await sleep(400);
    const on = [...document.body.classList].filter((c) => c.startsWith('tile-style-'));
    if (on.join(',') !== '__ON__') errs.push('style classes are [' + on.join(',') + '], expected [__ON__]');
    const name = document.querySelector('#grid .tile .tile-name');
    const tt = name ? getComputedStyle(name).textTransform : '(no tile name)';
    if (tt !== '__TT__') errs.push('text-transform is ' + tt + ', expected __TT__');
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""


# chainSeparator: two static prompts chained, the copied text carries the chosen separator.
CHAIN_PROBE = """
<script>
(async () => {""" + FINISH + """
  const errs = [];
  try {
    await until(() => document.querySelectorAll('#grid .tile').length > 6);
    await sleep(400);
    const tile = (id) => [...document.querySelectorAll('#grid .tile')].find((e) => (e.dataset.id || '') === id);
    const pick = async (id) => {
      const el = tile(id);
      for (const type of ['pointerdown', 'pointerup', 'click']) {
        el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1 }));
      }
      await sleep(400);
    };
    let start = null;
    if (tile('p0')) {
      tile('p0').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 60, clientY: 60 }));
      const entry = document.querySelector('#ctx [data-act="chain"]');
      if (entry && !entry.classList.contains('hidden')) start = entry;
    }
    if (!start) { errs.push('the menu of p0 offers no Start chain'); }
    else {
      start.click();
      await sleep(300);
      await pick('p2');
      const want = 'Example prompt body number 1.' + __SEP__ + 'Example prompt body number 3.';
      if (window.__LAST_COPY__ !== want) errs.push('chain text is ' + JSON.stringify(window.__LAST_COPY__) + ', expected ' + JSON.stringify(want));
    }
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""

# searchListMax: the recent-search list keeps as many terms as the setting says.
GS_PROBE = """
<script>
(async () => {""" + FINISH + """
  const errs = [];
  try {
    await until(() => document.querySelectorAll('#grid .tile').length > 6);
    await sleep(400);
    const open = () => document.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }));
    const run = async (term) => {
      open();
      await sleep(150);
      const inp = document.getElementById('gs-input');
      inp.value = term;
      inp.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(200);
      inp.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      await sleep(150);
    };
    for (let i = 1; i <= __TERMS__; i++) await run('Prompt ' + i);
    open();
    await sleep(200);
    const names = [...document.querySelectorAll('#gs-results .gs-recent-row .gs-row-name')].map((r) => r.textContent);
    if (names.length !== __WANT__) errs.push('recent list holds ' + names.length + ' terms (' + names.join(',') + '), expected __WANT__');
    if (__WANT__ > 0 && names[0] !== 'Prompt __TERMS__') errs.push('newest recent term is ' + names[0] + ', expected Prompt __TERMS__');
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""

# searchListMax: the launcher lists as many rows as the setting says.
LAUNCHER_PROBE = """
<script>
(async () => {""" + FINISH + """
  const errs = [];
  try {
    await until(() => document.querySelectorAll('#results .launcher-row').length > 0);
    await sleep(300);
    const n = document.querySelectorAll('#results .launcher-row').length;
    if (n !== __WANT__) errs.push('launcher lists ' + n + ' rows, expected __WANT__');
    const search = document.getElementById('search');
    search.value = 'Prompt';
    search.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(100);
    const m = document.querySelectorAll('#results .launcher-row').length;
    if (m !== __WANT__) errs.push('launcher lists ' + m + ' matches, expected __WANT__');
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""

# The editor autosaves every 30 s; the probe only waits a few seconds, so the page
# gets a timer that runs ten times faster for exactly that one interval length.
FAST_AUTOSAVE = """<script>
(function () {
  const real = window.setInterval.bind(window);
  window.setInterval = (fn, ms, ...rest) => real(fn, ms === 30000 ? 3000 : ms, ...rest);
})();
</script>"""

# veOvTextOutlinePx / veOvKeysActiveColor / veOvVideoVolume: a new overlay starts from them.
# The editor autosaves its operations every few seconds; that record is what is read.
VIDEO_PROBE = """
<script>
(async () => {""" + FINISH + """
  const errs = [];
  try {
    await until(() => document.getElementById('ve-stage-msg').classList.contains('hidden') &&
      document.querySelectorAll('#ve-filmstrip .ve-thumb').length > 0, 300);
    await sleep(300);
    document.getElementById('ve-tab-overlays').click();
    await sleep(100);
    document.getElementById('ve-ov-add-text').click();
    await sleep(150);
    document.getElementById('ve-ov-add-keys').click();
    await sleep(150);
    document.getElementById('ve-add-video').click();
    await sleep(100);
    document.getElementById('ve-add-video-as-overlay').click();
    await sleep(6500);
    const saved = window.__EDIT_STATE_SAVED__;
    if (!saved) { errs.push('the editor never saved its operations'); }
    else {
      const ops = JSON.parse(saved.ops).ops.filter((o) => o.tool === 'overlay');
      const of = (kind) => ops.find((o) => o.kind === kind);
      const text = of('text'), keys = of('keys'), video = of('video');
      if (!text || !keys || !video) errs.push('missing overlay kinds: ' + ops.map((o) => o.kind).join(','));
      else {
        if (text.outlinePx !== __OUTLINE__) errs.push('text outlinePx is ' + text.outlinePx + ', expected __OUTLINE__');
        if (keys.activeColor !== '__ACTIVE__') errs.push('keys activeColor is ' + keys.activeColor + ', expected __ACTIVE__');
        if (video.volume !== __VOLUME__) errs.push('video volume is ' + video.volume + ', expected __VOLUME__');
      }
    }
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""

# recOutlineColor: the script the backend runs in the outline window (taken from the
# Rust source, so the two cannot drift apart) recolours the frame of recoutline.html.
OUTLINE_PROBE = """
<script>
(async () => {""" + FINISH + """
  const errs = [];
  try {
    await sleep(200);
    const frame = document.getElementById('frame');
    const got = getComputedStyle(frame).borderTopColor;
    if (got !== '__WANT__') errs.push('frame border is ' + got + ', expected __WANT__');
    const w = getComputedStyle(frame).borderTopWidth;
    if (w !== '2px') errs.push('frame border width is ' + w + ', expected 2px');
  } catch (e) { errs.push('probe threw: ' + (e && e.stack || e)); }
  finish(errs);
})();
</script>
"""


def outline_init_script(color):
    """The Rust rec_outline_init_script template with the colour filled in."""
    src = "\n".join(
        p.read_text(encoding="utf-8")
        for p in sorted((REPO / "src-tauri" / "src").glob("*.rs"))
        if p.name != "tests.rs"
    )
    m = re.search(r"fn rec_outline_init_script\(color: &str\) -> String \{\s*format!\(\s*\"(.*?)\"\s*\)\s*\}", src, re.S)
    if not m:
        raise RuntimeError("rec_outline_init_script not found in the Rust sources")
    return m.group(1).replace("{{", "{").replace("}}", "}").replace("{color}", color)


def hex_to_rgb(h):
    return "rgb(%d, %d, %d)" % (int(h[1:3], 16), int(h[3:5], 16), int(h[5:7], 16))


def consumer_problems(browser, tmp):
    out = []

    def check(label, name, probe, seed, **kw):
        for e in run(browser, tmp, name, probe, seed, **kw):
            out.append(f"[{label}] {e}")

    nl = "String.fromCharCode(10)"
    for texts, sep in (({}, nl + " + " + nl), ({"chainSeparator": "break"}, nl), ({"chainSeparator": "space"}, "' '"),
                       ({"chainSeparator": "none"}, "''")):
        label = "chainSeparator " + (texts.get("chainSeparator") or "default")
        check(label, "index.html", CHAIN_PROBE.replace("__SEP__", sep),
              {"ui_flags": {"chainQuickStart": True}, "ui_texts": texts})
    for values, terms, want in (({}, 9, 8), ({"searchListMax": 3}, 4, 3)):
        label = "searchListMax recent " + str(values.get("searchListMax", "default"))
        check(label, "index.html", GS_PROBE.replace("__TERMS__", str(terms)).replace("__WANT__", str(want)),
              {"ui_flags": {"globalSearch": True}, "ui_values": values})
    for values, want in (({}, 8), ({"searchListMax": 3}, 3), ({"searchListMax": 11}, 11)):
        label = "searchListMax launcher " + str(values.get("searchListMax", "default"))
        check(label, "launcher.html", LAUNCHER_PROBE.replace("__WANT__", str(want)), {"ui_values": values},
              size=(560, 420), budget=15000)
    for values, want in (({}, "true"), ({"bubbleMs": 0}, "false")):
        label = "bubbleMs gate " + str(values.get("bubbleMs", "default"))
        check(label, "index.html", VALUE_GATE_PROBE.replace("__WANT__", want).replace("__BUBBLE__", str(values.get("bubbleMs", "default"))),
              {"ui_values": values})
    for values, dim_on, opacity, lift in (({}, "false", "1", "0px"), ({"hoverDim": 50, "hoverLift": 3}, "true", "0.5", "3px")):
        label = "hover dim and lift " + ("default" if not values else "seeded")
        check(label, "index.html", HOVER_PROBE.replace("__DIMON__", dim_on).replace("__OPACITY__", opacity).replace("__LIFT__", lift),
              {"ui_values": values})
    for style, on, tt in (({}, "", "none"), ({"tileStyle": "uppercase"}, "tile-style-uppercase", "uppercase"),
                          ({"tileStyle": "bold"}, "tile-style-bold", "none")):
        label = "tile style " + (style.get("tileStyle") or "default")
        check(label, "index.html", STYLE_PROBE.replace("__ON__", on).replace("__TT__", tt), {"ui_texts": style})
    seeded = {"ui_values": {"veOvTextOutlinePx": 7, "veOvVideoVolume": 40},
              "ui_texts": {"veOvKeysActiveColor": "#ff00aa"}}
    plain = {"ui_values": {}}
    for label, seed, o, a, v in (("video overlays seeded", seeded, 7, "#ff00aa", 40),
                                 ("video overlays default", plain, 3, "#3fa7ff", 0)):
        probe = VIDEO_PROBE.replace("__OUTLINE__", str(o)).replace("__ACTIVE__", a).replace("__VOLUME__", str(v))
        check(label, "video.html", probe, seed, size=(1130, 860), budget=40000, head=FAST_AUTOSAVE)
    for color in ("#ff8800", "#2563eb"):
        head = "<script>" + outline_init_script(color) + "</script>"
        check("outline colour " + color, "recoutline.html", OUTLINE_PROBE.replace("__WANT__", hex_to_rgb(color)),
              {}, size=(300, 200), budget=5000, head=head)
    check("outline colour default", "recoutline.html", OUTLINE_PROBE.replace("__WANT__", hex_to_rgb("#2563eb")),
          {}, size=(300, 200), budget=5000)
    return out


def run(browser, tmp, name, probe, seed, size=(1280, 900), budget=60000, head=""):
    side = tmp / "ui" / name
    pre = "<script>window.__SEED_SETTINGS__ = " + json.dumps(seed) + ";</script>\n"
    side.write_text(
        (REPO / "ui" / name).read_text(encoding="utf-8")
        .replace("<script", pre + '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</head>", head + "</head>", 1)
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
    """Tab and group title key -> English text, read from the en block of the dictionary."""
    text = combined_source()
    en = re.search(r"\n  en: \{(.*?)\n  de: \{", text, re.S).group(1)
    return dict(re.findall(r'^    ((?:expTab|expGroup)\w+): "([^"]*)"', en, re.M))


def source_problems():
    """Help texts must not name a group that no longer exists."""
    out = []
    for rel in ("ui/help.js", "docs/help.md", "README.md", "ui/i18n/*"):
        text = combined_source() if rel == "ui/i18n/*" else (REPO / rel).read_text(encoding="utf-8")
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
        for label, wide, size in (("wide window", "true", (1600, 900)), ("narrow window", "false", (680, 460)), ("very narrow window", "false", (520, 460))):
            errs = run(browser, tmp, "index.html", LAYOUT_PROBE.replace("__WIDE__", wide), {}, size=size)
            problems += [f"[{label}] {e}" for e in errs]
        texts = {"ui_flags": {g: True for g in ("autoPaste", "fileAttach", "attachTextPreview", "errorLog", "floating")},
                 "ui_values": {"copyCooldownMs": 2000, "autoPasteDelayMs": 80, "attachPreviewKb": 8,
                               "errorLogKeepKb": 256, "floatParkCleanupH": 0}}
        errs = run(browser, tmp, "index.html", VALUE_TEXT_PROBE, texts)
        problems += [f"[value texts] {e}" for e in errs]
        problems += consumer_problems(browser, tmp)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    for p in problems:
        print("FAIL:", p)
    print("PASS" if not problems else f"{len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
