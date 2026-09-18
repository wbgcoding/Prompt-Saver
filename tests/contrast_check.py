#!/usr/bin/env python3
"""Contrast sweep: every rendered text-on-surface pair, every focus ring and
every control border, in every shipped colour world, on every one of the
app's nine windows.

Boots the real UI (real markup, real CSS, the same stubbed Tauri backend the
UI smoke test uses) in headless Chromium, then switches `data-theme` live --
the app's own theme mechanism is a pure CSS-variable swap, so one boot per
window covers all seven shipped themes without a reload. The colour editor
gets three additionally generated palettes (very light, very dark, and one
deliberately bad) on top of the seven, since its inline `--var` overrides are
a separate code path from `data-theme` alone.

Reported thresholds (WCAG AA): 4.5 for normal text, 3.0 for text at 24px or
more (or bold at 19px or more), 3.0 for focus rings and control borders.
Every finding names the colour world, the window, the element and the ratio.
A transparent background is resolved by compositing up through its ancestors
to the page's own canvas -- skipping that step is exactly what would make the
check compare a colour against white and report nonsense.

Run:  python tests/contrast_check.py
Exit: 0 = every real pair clears its threshold, 1 = a genuine finding.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
HERE = Path(__file__).parent

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]

# The seven shipped colour worlds (ui/main.js BUILTIN_THEMES) plus, on the
# main window only, three generated palettes exercising the colour editor's
# own inline-override path (applyThemeVars in ui/main.js).
BUILTIN_THEMES = ["light", "dark", "programmer", "neon", "cyberpunk", "mono", "ocean"]
GENERATED_PALETTES = {
    "generated-very-light": {
        "bg": "#ffffff", "surface": "#f5f5f5", "surface-2": "#ececec",
        "fg": "#111111", "muted": "#5a5a5a", "border": "#cfcfcf",
        "accent": "#1d4ed8", "accent-fg": "#ffffff", "danger": "#b91c1c",
    },
    "generated-very-dark": {
        "bg": "#0a0a0a", "surface": "#161616", "surface-2": "#202020",
        "fg": "#f2f2f2", "muted": "#a8a8a8", "border": "#333333",
        "accent": "#60a5fa", "accent-fg": "#0a0a0a", "danger": "#f87171",
    },
    # Deliberately unreadable (near-white text on white, a yellow accent with
    # a white label) so a clean run can prove the check actually trips.
    "generated-deliberately-bad": {
        "bg": "#ffffff", "surface": "#fdfdfd", "surface-2": "#fbfbfb",
        "fg": "#e4e4e4", "muted": "#eeeeee", "border": "#f6f6f6",
        "accent": "#ffff00", "accent-fg": "#ffffff", "danger": "#ffb3b3",
    },
}
BAD_PALETTE = "generated-deliberately-bad"

WINDOWS = [
    "index.html", "floating.html", "snip.html", "folder.html", "vars.html",
    "launcher.html", "tray.html", "recbar.html", "video.html",
]

NORMAL_MIN = 4.5
LARGE_MIN = 3.0
CONTROL_MIN = 3.0


class Exception_:  # noqa: N801 -- avoid shadowing the builtin `Exception`
    """One deliberate, explained finding. Content-matched (not line-pinned),
    same reasoning as tests/source_hygiene.py's own EXCEPTIONS list. A reason
    is mandatory -- the constructor refuses to build one without it, so an
    exception without a reason cannot exist rather than merely being flagged."""

    __slots__ = ("theme", "window", "match", "reason")

    def __init__(self, theme, window, match, reason):
        if not reason or not reason.strip():
            raise ValueError("a contrast exception needs a reason")
        self.theme, self.window, self.match, self.reason = theme, window, match, reason


# No known, deliberate exceptions today -- everything the sweep finds gets
# fixed in the same round instead (see the module docstring).
EXCEPTIONS = []


def is_excepted(theme, window, finding):
    return any(
        exc.theme in (theme, "*") and exc.window in (window, "*") and exc.match(finding)
        for exc in EXCEPTIONS
    )


# JS injected into every window page. THEMES/PALETTES/thresholds are spliced
# in from Python so the two sides never drift apart.
MEASURE_JS = """
<script>
window.__CONTRAST_THEMES__ = %(themes_json)s;
window.__CONTRAST_PALETTES__ = %(palettes_json)s;
window.__CONTRAST_SCENES__ = %(scenes_json)s;
window.__CONTRAST_RING_TOKENS__ = %(ring_tokens_json)s;
(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  // A plain timer, deliberately not requestAnimationFrame: a small or
  // unfocused headless window can stop producing frames entirely, which
  // would stall a rAF chain forever but still fires a setTimeout.
  const frame = () => sleep(60);
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--contrast-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 30);

  const parseColor = (s) => {
    const m = /rgba?\\(([\\d.]+),\\s*([\\d.]+),\\s*([\\d.]+)(?:,\\s*([\\d.]+))?\\)/.exec(s || '');
    if (!m) return null;
    return [parseFloat(m[1]), parseFloat(m[2]), parseFloat(m[3]), m[4] === undefined ? 1 : parseFloat(m[4])];
  };
  const relLum = ([r, g, b]) => {
    const lin = (c) => { c /= 255; return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4); };
    return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
  };
  const ratio = (a, b) => {
    const la = relLum(a), lb = relLum(b);
    const hi = Math.max(la, lb), lo = Math.min(la, lb);
    return (hi + 0.05) / (lo + 0.05);
  };
  // Transparent backgrounds resolved up to the first covering ancestor by
  // compositing every layer from the root down onto a white canvas -- the
  // only alternative is comparing straight against white and reporting
  // nonsense for anything that is not opaque on its own layer.
  const effectiveBg = (el) => {
    const chain = [];
    for (let n = el; n; n = n.parentElement) chain.unshift(n);
    let canvas = [255, 255, 255];
    for (const n of chain) {
      const bg = parseColor(getComputedStyle(n).backgroundColor);
      if (!bg || bg[3] <= 0) continue;
      const [r, g, b, a] = bg;
      canvas = [r * a + canvas[0] * (1 - a), g * a + canvas[1] * (1 - a), b * a + canvas[2] * (1 - a)];
    }
    return canvas;
  };
  // Occlusion-aware: a stacked dialog can leave an element with a real box
  // and display !== none while something else -- another open dialog, its
  // own backdrop -- is what is actually painted at that spot. Checking only
  // display/visibility/opacity (as a naive isVisible would) reports every
  // element still in the layout, including whatever a second dialog opened
  // in the same pass is currently covering; elementFromPoint is what a real
  // pointer -- and a real user's eye -- would actually land on.
  const isVisible = (el) => {
    const st = getComputedStyle(el);
    if (st.display === 'none' || st.visibility === 'hidden' || parseFloat(st.opacity) === 0) return false;
    // Opacity compounds through ancestors but getComputedStyle only ever
    // reports an element's OWN local value -- an .overlay stuck on its
    // opening `animation: fade-in` keyframe (a real headless-Chromium
    // quirk, not a product defect: the compositor-driven animation can
    // fail to progress past frame zero without a focused, painting window)
    // makes every element inside it invisible while each of THEM still
    // individually reports opacity: 1. elementFromPoint does not catch
    // this either -- opacity never affects hit-testing, unlike display or
    // visibility, so a fully transparent element still receives the hit.
    for (let n = el.parentElement; n; n = n.parentElement) {
      if (parseFloat(getComputedStyle(n).opacity) === 0) return false;
    }
    const r = el.getBoundingClientRect();
    if (!r.width || !r.height) return false;
    const x = Math.min(Math.max(r.left + r.width / 2, 0), innerWidth - 1);
    const y = Math.min(Math.max(r.top + r.height / 2, 0), innerHeight - 1);
    if (r.right < 0 || r.bottom < 0 || r.left > innerWidth || r.top > innerHeight) return false;
    const hit = document.elementFromPoint(x, y);
    if (!hit) return false;
    return hit === el || el.contains(hit) || hit.contains(el);
  };
  const hasDirectText = (el) => {
    for (const n of el.childNodes) if (n.nodeType === 3 && n.textContent.trim()) return true;
    return false;
  };
  const describe = (el) => {
    let d = el.tagName.toLowerCase();
    if (el.id) d += '#' + el.id;
    else if (typeof el.className === 'string' && el.className.trim()) {
      d += '.' + el.className.trim().split(/\\s+/).slice(0, 2).join('.');
    }
    const txt = (el.textContent || '').trim().replace(/\\s+/g, ' ').slice(0, 24);
    return txt ? d + ' \\"' + txt + '\\"' : d;
  };
  const round2 = (n) => Math.round(n * 100) / 100;

  // Border contrast is only meaningful where the border IS the control's
  // boundary (a form field's box). A button or icon control's affordance is
  // its background/icon, not a resting-state hairline, so a decorative 1px
  // separator there is not the WCAG 1.4.11 case this threshold targets --
  // including those produced thousands of findings against boxes never
  // meant to read as a hard edge on their own.
  const CONTROL_SEL = 'input:not([type="hidden"]), select, textarea';

  // WCAG's own text (1.4.3) and non-text (1.4.11) contrast criteria both
  // exempt inactive/disabled components -- reduced contrast is how a
  // disabled control is conventionally shown to BE disabled.
  const isDisabled = (el) => !!(el.disabled || el.closest('[disabled], [aria-disabled="true"]'));

  function scan() {
    const findings = [];
    for (const el of document.querySelectorAll('*')) {
      if (!hasDirectText(el) || !isVisible(el) || isDisabled(el)) continue;
      const st = getComputedStyle(el);
      const fg = parseColor(st.color);
      if (!fg) continue;
      // Composited through the element itself, not just its parent: a
      // button with its own opaque background (an active tab, a checked
      // switch, a primary button) paints that background UNDER its own
      // text, so the parent's colour behind it is no longer what the text
      // actually sits on. effectiveBg already walks the chain down to and
      // including whichever element it is handed.
      const bg = effectiveBg(el);
      const px = parseFloat(st.fontSize) || 16;
      const weight = parseInt(st.fontWeight, 10) || 400;
      const large = px >= 24 || (weight >= 700 && px >= 19);
      const min = large ? %(large_min)s : %(normal_min)s;
      const r = ratio(fg, bg);
      if (r < min) findings.push({ kind: 'text', element: describe(el), ratio: round2(r), min });
    }
    for (const el of document.querySelectorAll(CONTROL_SEL)) {
      if (!isVisible(el) || isDisabled(el)) continue;
      const bg = effectiveBg(el.parentElement || el);
      const st = getComputedStyle(el);
      if (st.borderStyle !== 'none' && parseFloat(st.borderTopWidth) > 0) {
        const bc = parseColor(st.borderTopColor);
        if (bc && bc[3] > 0) {
          const r = ratio(bc, bg);
          if (r < %(control_min)s) findings.push({ kind: 'border', element: describe(el), ratio: round2(r), min: %(control_min)s });
        }
      }
    }
    // Every :focus-visible rule in the shipped CSS draws its ring from one of
    // two theme tokens (var(--accent), var(--fg)) -- checking the tokens
    // against the surfaces a ring can sit on covers every ring in the app
    // without needing a real keyboard-focus event, which headless Chromium
    // will not raise from script (a programmatic .focus() never satisfies
    // :focus-visible, so reading getComputedStyle(el).outlineColor after it
    // only ever measures the browser's own unthemed default outline).
    const root = getComputedStyle(document.documentElement);
    const tokenColor = (name) => {
      const v = root.getPropertyValue('--' + name).trim();
      const probe = document.createElement('span');
      probe.style.color = v;
      document.body.appendChild(probe);
      const c = parseColor(getComputedStyle(probe).color);
      probe.remove();
      return c;
    };
    const surfaces = ['bg', 'surface', 'surface-2'].map((n) => ({ name: n, rgb: tokenColor(n) }));
    for (const ringToken of window.__CONTRAST_RING_TOKENS__ || []) {
      const ring = tokenColor(ringToken);
      if (!ring) continue;
      let worst = null;
      for (const s of surfaces) {
        if (!s.rgb) continue;
        const r = ratio(ring, s.rgb);
        if (!worst || r < worst.ratio) worst = { ratio: r, against: s.name };
      }
      if (worst && worst.ratio < %(control_min)s) {
        findings.push({
          kind: 'focus-ring', element: 'var(--' + ringToken + ') ring against --' + worst.against,
          ratio: round2(worst.ratio), min: %(control_min)s,
        });
      }
    }
    return findings;
  }

  const results = {};
  try {
    // The portable take-over dialog can open on startup once show_main_window
    // resolves (not a fixed delay) -- same wait the UI smoke test uses, so a
    // sweep never measures colours through it fading in or leaves it open to
    // stack with every dialog a later scene opens on purpose.
    const cm = document.getElementById('confirm-modal');
    if (cm) {
      for (let i = 0; i < 30 && cm.classList.contains('hidden'); i++) await sleep(100);
      if (!cm.classList.contains('hidden')) {
        const cancel = document.getElementById('confirm-cancel');
        if (cancel) cancel.click();
        await sleep(200);
      }
    }

    // Dialog coverage comes from the real click-driven scenes below (the
    // real app boots against the stub backend, same as the UI smoke test),
    // never from a blanket ".hidden" strip: forcing every dialog open at
    // once stacks their backdrops on top of the base page and on each
    // other, which is not a state a user ever sees and which trips the
    // app's own "dim the inert backdrop behind an open dialog" styling --
    // a real, intentional de-emphasis, not a contrast defect.
    const applyBuiltin = (theme) => {
      document.documentElement.setAttribute('data-theme', theme);
      const style = document.documentElement.style;
      for (const k of ['bg', 'surface', 'surface-2', 'fg', 'muted', 'border', 'accent', 'accent-fg', 'danger']) {
        style.removeProperty('--' + k);
      }
    };
    const applyPalette = (vars) => {
      applyBuiltin('light');
      const style = document.documentElement.style;
      for (const [k, v] of Object.entries(vars)) style.setProperty('--' + k, v);
    };

    // Force every CSS animation (the shared `.overlay { animation: fade-in }`
    // among them) straight to its end state -- a headless, unfocused window
    // can otherwise leave a just-opened dialog's compositor-driven opacity
    // animation parked on its own opening keyframe indefinitely, which is a
    // rendering-pipeline quirk of this harness, not a real user ever sees.
    const settleAnimations = () => {
      try { document.getAnimations().forEach((a) => a.finish()); } catch (e) {}
    };
    // A window with no confirm-modal to wait out (every satellite window
    // besides the main one) would otherwise start scanning the instant the
    // DOM parses -- before its own boot script has populated labels or
    // applied its own theme, which is a mid-boot flash no real user ever
    // sees, not a colour defect.
    await sleep(400);
    for (const scene of window.__CONTRAST_SCENES__ || ['default']) {
      if (scene !== 'default') {
        try {
          const el = document.querySelector(scene);
          if (el) el.click();
          await sleep(120);
          settleAnimations();
          await sleep(120);
        } catch (e) { /* scene not present in this window -- skip it */ }
      }
      for (const theme of window.__CONTRAST_THEMES__) {
        applyBuiltin(theme);
        // Every themed surface carries its own `transition: background
        // var(--transition), color var(--transition)`, so switching the
        // token set starts a real CSS transition rather than an instant
        // swap -- scanning right away would read the FIRST theme in the
        // loop mid-fade from whatever state came before it. Finishing
        // whatever the attribute change just started (a CSS transition is
        // itself an Animation) makes the swap instant instead of waiting
        // out the fade on every single theme.
        settleAnimations();
        await frame();
        const key = theme + (scene === 'default' ? '' : ' (' + scene + ')');
        results[key] = (results[key] || []).concat(scan());
      }
      for (const [name, vars] of Object.entries(window.__CONTRAST_PALETTES__ || {})) {
        applyPalette(vars);
        settleAnimations();
        await frame();
        const key = name + (scene === 'default' ? '' : ' (' + scene + ')');
        results[key] = (results[key] || []).concat(scan());
      }
    }
  } catch (e) {
    results['__error__'] = [{ kind: 'probe', element: String(e && e.stack || e), ratio: 0, min: 0 }];
  }
  clearInterval(beat);
  const pre = document.createElement('pre');
  pre.id = 'RESULT';
  pre.textContent = JSON.stringify(results);
  document.body.appendChild(pre);
})();
</script>
"""


def unescape(s):
    for a, b in (("&quot;", '"'), ("&lt;", "<"), ("&gt;", ">"), ("&amp;", "&")):
        s = s.replace(a, b)
    return s


# Per-window extra scenes to open before sweeping -- CSS selectors clicked in
# order, one sweep per scene, so dialog and expert-tab content gets its own
# pass instead of staying unmeasured behind a click nothing here triggers.
# Only the main window carries enough distinct dialogs to be worth it; the
# eight satellite windows show their whole content on boot already.
SCENES = {
    # Scenes run in order and state carries over (a real click-driven walk,
    # not an independent boot each time) -- each dialog this list opens is
    # closed again before the next one, so no two dialogs a user would not
    # see together ever end up stacked at once.
    "index.html": [
        "default",
        "#gear",
        "#about-btn",
        "#about-ok",
        "#expert-open",
        '#expert-tabs-bar .expert-tab:nth-child(1)',
        '#expert-tabs-bar .expert-tab:nth-child(2)',
        '#expert-tabs-bar .expert-tab:nth-child(3)',
        '#expert-tabs-bar .expert-tab:nth-child(4)',
        '#expert-tabs-bar .expert-tab:nth-child(5)',
        '#expert-tabs-bar .expert-tab:nth-child(6)',
    ],
}
# Palettes only apply to the main window -- the colour editor lives there.
PALETTE_WINDOWS = {"index.html"}

# Only the windows that load main.css alongside their own stylesheet share
# its --accent/--fg/--bg/--surface token names (ui/video.html links both);
# the eight small satellite windows carry their own separate token set
# (ui/floattheme.js) and define no `:focus-visible` rule of their own today,
# so there is nothing for this check to read there.
RING_TOKEN_WINDOWS = {"index.html": ["accent", "fg"], "video.html": ["accent", "fg"]}


def run_window(browser, tmp, name):
    themes = BUILTIN_THEMES
    palettes = GENERATED_PALETTES if name in PALETTE_WINDOWS else {}
    scenes = SCENES.get(name, ["default"])
    js = MEASURE_JS % {
        "themes_json": json.dumps(themes),
        "palettes_json": json.dumps(palettes),
        "scenes_json": json.dumps(scenes),
        "ring_tokens_json": json.dumps(RING_TOKEN_WINDOWS.get(name, [])),
        "normal_min": NORMAL_MIN,
        "large_min": LARGE_MIN,
        "control_min": CONTROL_MIN,
    }
    side = tmp / "ui" / f"contrast-{name}"
    html = (REPO / "ui" / name).read_text(encoding="utf-8")
    html = html.replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
    html = html.replace("</body>", js + "</body>")
    side.write_text(html, encoding="utf-8")
    proc = subprocess.run(
        [browser, "--headless=new", "--disable-gpu", "--no-first-run",
         f"--user-data-dir={tmp / ('profile-' + name)}", "--allow-file-access-from-files",
         "--window-size=1400,900", "--virtual-time-budget=60000",
         "--dump-dom", side.as_uri()],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
        timeout=180)
    m = re.search(r'<pre id="RESULT">(\{.*?\})</pre>', proc.stdout, re.S)
    if not m:
        return {"__no-result__": [{"kind": "probe", "element": "no probe result", "ratio": 0, "min": 0}]}
    return json.loads(unescape(m.group(1)))


def main():
    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        if os.environ.get("CI"):
            print("FAIL: no Chromium browser found on a CI runner")
            return 1
        print("SKIP: no Chromium browser found")
        return 0

    for exc in EXCEPTIONS:
        pass  # construction above already refused any exception without a reason

    tmp = Path(tempfile.mkdtemp(prefix="ps-contrast-"))
    failed = 0
    bad_palette_triggered = False
    try:
        shutil.copytree(REPO / "ui", tmp / "ui")
        shutil.copy(REPO / "tests" / "ui" / "tauri_stub.js", tmp / "ui" / "tauri_stub.js")

        for window in WINDOWS:
            results = run_window(browser, tmp, window)
            window_findings = 0
            for theme_key, findings in results.items():
                shown = []
                for f in findings:
                    if is_excepted(theme_key, window, f):
                        continue
                    shown.append(f)
                if theme_key.startswith(BAD_PALETTE) and findings:
                    bad_palette_triggered = True
                if theme_key.startswith("generated-"):
                    # Every generated palette (the two clean ones and the
                    # deliberately bad one) is a self-test of the sweep
                    # itself, never a shipped colour world -- printed for
                    # visibility when it fires, but never part of the gate.
                    for f in shown:
                        print(f'self-test [{theme_key}] {window}: {f["kind"]} "{f["element"]}" '
                              f'ratio {f["ratio"]} below {f["min"]}')
                    continue
                for f in shown:
                    print(f'FAIL  [{theme_key}] {window}: {f["kind"]} "{f["element"]}" '
                          f'ratio {f["ratio"]} below {f["min"]}')
                    window_findings += 1
            failed += window_findings
            if not window_findings:
                print(f"ok    {window}: every theme clears its threshold")

        if not bad_palette_triggered:
            print("FAIL  self-test: the deliberately bad generated palette raised no finding "
                  "-- the sweep cannot be trusted to catch a real regression")
            failed += 1

        print("PASS" if not failed else f"{failed} problem(s)")
        return 1 if failed else 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
