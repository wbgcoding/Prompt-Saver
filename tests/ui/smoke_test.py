"""Headless smoke test: boot the real UI with a stubbed Tauri backend.

Catches the class of defect that only shows when the app actually runs:
  * a JS error during startup or while opening a dialog,
  * an element that overflows the window horizontally (layout bug),
  * a control parked outside the window, covered, or crushed to no height,
  * a string that renders as its raw i18n key (missing translation).

It walks the library, the copy history, the tile menu and every expert tab, down
to the smallest window the app itself allows, and once more with the appearance
scales raised. Two extra boots cover the store-sealing warning in both
directions, since it only ever appears during startup. Skips cleanly when no
Chromium browser is installed.

Run:  python tests/ui/smoke_test.py
Exit: 0 = clean, 1 = problems found.
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

# The floor is the app's own minimum window size: read it, so this sweep follows
# tauri.conf.json instead of drifting away from it.
_WIN = json.loads((REPO / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))["app"]["windows"][0]
MIN_W, MIN_H = _WIN["minWidth"], _WIN["minHeight"]
SIZES = [(1280, 800), (1366, 768), (900, 650), (620, 500), (MIN_W + 60, MIN_H + 40), (MIN_W, MIN_H)]

# Headless Chrome refuses to make its window narrower than ~500px, so the page is
# loaded in an iframe of the exact size instead; the wrapper republishes the probe
# result because --dump-dom only prints the top document.
WRAPPER = """<!doctype html><meta charset="utf-8">
<style>html,body{margin:0}iframe{border:0;display:block}</style>
<iframe id="f" src="ui/index.html" style="width:%dpx;height:%dpx"></iframe><pre id="SMOKE"></pre>
<script>
setInterval(() => {
  try {
    const p = f.contentDocument.getElementById('SMOKE');
    const own = document.getElementById('SMOKE');
    if (p && !own.textContent) own.textContent = p.textContent;
  } catch (e) {}
}, 200);
</script>"""

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]

PROBE = """
<script>
(async () => {
  const out = { errors: [], overflow: [], tabs: [], rawKeys: [], layout: [] };
  // 434: an uncaught exception anywhere below used to kill the whole probe
  // silently ("no probe result", nothing runs) -- wrapping the walk means a
  // real regression shows up as a reported error instead of a bare timeout.
  try {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const click = async (id) => { const e = document.getElementById(id); if (e) { e.click(); await sleep(200); } };
  // Placeholders are only ever asked in the standalone window: the stub keeps the
  // payload of open_var_window, and the window answers with a vars-filled event.
  const varReq = () => { try { return JSON.parse(window.__VAR_PAYLOAD__ || 'null'); } catch (e) { return null; } };
  const varAnswer = async (values) => {
    window.__FIRE__('vars-filled', JSON.stringify({ values, remember: values.map(() => false) }));
    await sleep(250);
    window.__VAR_PAYLOAD__ = undefined;
  };
  const varCancel = async () => { window.__FIRE__('vars-filled', null); await sleep(200); window.__VAR_PAYLOAD__ = undefined; };
  // The header padding stays at the former default of the removed setting (measured
  // before the setting went: 10px).
  { const hp = parseFloat(getComputedStyle(document.querySelector('.header')).paddingTop);
    if (Math.abs(hp - 10) > 0.5) out.errors.push('the header padding is ' + hp + 'px, expected 10px'); }
  // A name or body that looks like markup must render as literal text everywhere
  // it shows up (grid tile, library row), never as a real element.
  const XSS_NAME = 'Injected <img src=x onerror=alert(1)> name';
  const XSS_TEXT = 'body <script>alert(2)<' + '/script> tail';

  // ---- layout guards -------------------------------------------------------
  // Everything below answers one question: on a small window, can the user still
  // reach and read what is on screen?
  const onScreen = (el) => {
    const s = getComputedStyle(el);
    return s.display !== 'none' && s.visibility !== 'hidden' && s.pointerEvents !== 'none' &&
      Number(s.opacity) > 0.05 && el.getClientRects().length;
  };
  const sig = (el) => (el.id ? '#' + el.id : '') +
    (typeof el.className === 'string' && el.className.trim()
      ? '.' + el.className.trim().split(/\\s+/)[0] : el.tagName);
  // Content parked past the right edge of a SIDEWAYS-scrollable container (the view
  // tab strip) is reachable by scrolling. Behind an overflow-x:hidden ancestor it is
  // gone for good, so that still counts as an escape.
  // Only the NEAREST ancestor that controls horizontal overflow decides — an outer
  // scroller cannot reveal what an inner overflow-x:hidden box already cut off.
  const inScroller = (el) => {
    // Every SVG element computes overflow-x: hidden by the UA stylesheet, whether
    // or not its author ever clipped anything -- an icon <path> would otherwise
    // always "find" that default on its own <svg> parent before ever reaching a
    // real scroll container further up, permanently defeating this check for
    // every icon button (178 uncovered this on the screenshot editor toolbar,
    // whose icon buttons sit in a genuinely horizontally-scrollable row).
    // SVG's own viewport clipping is a distinct rendering concept from a CSS
    // overflow-driven scrollbar, so the walk starts above the whole SVG subtree.
    let start = el;
    while (start && start.namespaceURI === 'http://www.w3.org/2000/svg') start = start.parentElement;
    for (let p = start ? start.parentElement : null; p; p = p.parentElement) {
      const ox = getComputedStyle(p).overflowX;
      if (ox === 'auto' || ox === 'scroll') return true;
      if (ox === 'hidden' || ox === 'clip') return false;
    }
    return false;
  };
  // Hit-testable: a real pointer can land on it. Scrolling to it first is allowed;
  // sitting outside the window, or under something else, is not.
  const canHit = (el) => {
    try { el.scrollIntoView({ block: 'nearest', inline: 'nearest' }); } catch (e) {}
    const r = el.getBoundingClientRect();
    if (!r.width || !r.height) return false;
    const x = r.left + r.width / 2, y = r.top + r.height / 2;
    if (x < 0 || y < 0 || x > innerWidth || y > innerHeight) return false;
    const hit = document.elementFromPoint(x, y);
    if (!hit) return false;
    if (hit === el || el.contains(hit) || hit.contains(el)) return true;
    // Another dialog stacked on top is doing its job, not covering this one by accident.
    const theirs = hit.closest('.overlay');
    return !!theirs && theirs !== el.closest('.overlay');
  };
  const layoutScan = (where) => {
    const add = (m) => { const s = where + ': ' + m; if (!out.layout.includes(s)) out.layout.push(s); };
    if (document.documentElement.scrollWidth > innerWidth + 1) {
      add('the page scrolls horizontally (' + document.documentElement.scrollWidth + ' > ' + innerWidth + ')');
    }
    // A dialog holding more than it can show must be able to scroll to the rest —
    // on a short window the inner lists keep a height floor, so the dialog is where
    // the overflow ends up.
    for (const box of document.querySelectorAll('.overlay:not(.hidden) > .modal')) {
      const oy = getComputedStyle(box).overflowY;
      if (box.scrollHeight > box.clientHeight + 2 && oy !== 'auto' && oy !== 'scroll') {
        add(sig(box) + ' holds ' + box.scrollHeight + 'px of content in ' +
          box.clientHeight + 'px and cannot scroll to it');
      }
    }
    // Every popover goes through one placement rule and must land fully inside the
    // window — vertically too. A dropdown opening past the bottom edge is the theme
    // editor / value picker defect, and it is invisible to the horizontal scan.
    for (const el of document.querySelectorAll(
        '.num-pop, .color-pop, .tag-suggest, .tag-pop, .mini-menu, .ctx-menu, ' +
        '.tile-folder-pop, .toolbar-menu, .suggest-pop, .pick-history-pop')) {
      if (!onScreen(el)) continue;
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;
      if (r.bottom > innerHeight + 1 || r.top < -1 || r.right > innerWidth + 1 || r.left < -1) {
        add(sig(el) + ' opens outside the window (' + Math.round(r.left) + ',' + Math.round(r.top) +
          '..' + Math.round(r.right) + ',' + Math.round(r.bottom) + ' of ' + innerWidth + 'x' + innerHeight + ')');
      }
    }
    for (const el of document.querySelectorAll('body *')) {
      if (!onScreen(el)) continue;
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;
      if ((r.right > innerWidth + 1 || r.left < -1) && !inScroller(el)) {
        add(sig(el) + ' is outside the window (x ' + Math.round(r.left) + '..' +
          Math.round(r.right) + ' of ' + innerWidth + ')');
      }
      // Cut off sideways by a box that cannot scroll to it: the control is gone, not
      // merely off-view. Only interactive elements — labels legitimately ellipsise.
      if (el.matches('button, input, select, textarea')) {
        for (let p = el.parentElement; p; p = p.parentElement) {
          const ox = getComputedStyle(p).overflowX;
          if (ox === 'auto' || ox === 'scroll') break;
          if (ox === 'hidden' || ox === 'clip') {
            const pr = p.getBoundingClientRect();
            if (r.right > pr.right + 1 || r.left < pr.left - 1) {
              add(sig(el) + ' is clipped away by ' + sig(p));
            }
            break;
          }
        }
      }
      // A flex item with its own overflow bottoms out at zero height, which turns a
      // list into an unusable sliver instead of letting its dialog scroll.
      const s = getComputedStyle(el);
      if (/auto|scroll/.test(s.overflowY) && el.scrollHeight > 40 &&
          el.clientHeight > 0 && el.clientHeight < 40) {
        add(sig(el) + ' is crushed to ' + el.clientHeight + 'px around ' +
          el.scrollHeight + 'px of content');
      }
    }
    // Rows of an open dialog are stacked, never layered: a row squeezed below its
    // content is drawn OVER the row beneath it (the colour swatches used to end up
    // under the media block on a small window). Only in-flow children count.
    for (const box of document.querySelectorAll('.overlay:not(.hidden) .modal, .overlay:not(.hidden) .modal *')) {
      if (!(box instanceof HTMLElement)) continue; // an <svg> lays its own shapes out
      const st = getComputedStyle(box);
      if (st.display !== 'block' && st.display !== 'flex') continue;
      if (st.display === 'flex' && st.flexDirection.startsWith('row')) continue;
      const kids = [...box.children].filter((e) => e instanceof HTMLElement &&
        e.getClientRects().length && getComputedStyle(e).position === 'static');
      for (let i = 1; i < kids.length; i++) {
        const a = kids[i - 1].getBoundingClientRect(), b = kids[i].getBoundingClientRect();
        if (a.height > 0 && b.height > 0 && b.top < a.bottom - 1) {
          add(sig(kids[i - 1]) + ' and ' + sig(kids[i]) + ' overlap by ' +
            Math.round(a.bottom - b.top) + 'px');
        }
      }
    }
  };
  // A collapsible row is its own overflow box, so a flex parent can squeeze it below
  // its head and leave a 6px sliver where a clickable row belongs.
  const collapseRows = (where) => {
    for (const row of document.querySelectorAll(where === 'editor' ? '#modal .modal-collapse' : '.modal-collapse')) {
      const head = row.querySelector('.modal-collapse-head');
      if (!head || !onScreen(row)) continue;
      const h = row.getBoundingClientRect().height;
      if (h + 1 < head.scrollHeight) {
        out.layout.push('prompt editor: a collapsible row is squeezed to ' + Math.round(h) +
          'px around a ' + head.scrollHeight + 'px head');
      }
    }
  };
  const mustReach = (where, sel) => {
    for (const el of document.querySelectorAll(sel)) {
      if (onScreen(el) && !canHit(el)) {
        out.layout.push(where + ': "' + (el.id || el.textContent || sel).trim().slice(0, 20) + '" cannot be clicked');
      }
    }
  };

  // Startup reveals the window from a nested requestAnimationFrame. In a small
  // headless frame Chromium stops producing frames once the page is quiet, so that
  // second callback never runs and the walk below would start from a dead page.
  // Dirtying an unused custom property keeps frames coming; it touches no geometry.
  const heartbeat = setInterval(() => {
    document.documentElement.style.setProperty('--smoke-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
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
      if (!msg.includes('Example-Store')) out.errors.push('take-over dialog does not name the folder');
      if (!msg.includes('7')) out.errors.push('take-over dialog does not name the prompt count');
      if (!/\\n/.test(msg)) out.errors.push('take-over dialog lost its line breaks');
      // 449: a "do not ask again" box, checked by default, so an answer is
      // remembered unless the person actively opts out of that memory.
      const rememberCb = document.getElementById('confirm-cb-remember');
      if (!rememberCb) {
        out.errors.push('take-over dialog has no "do not ask again" checkbox');
      } else if (!rememberCb.checked) {
        out.errors.push('the "do not ask again" checkbox is not checked by default');
      }
      window.__SMOKE_ARGS__ = [];
      await click('confirm-cancel'); // "start empty"
      const applyArgs = (window.__SMOKE_ARGS__ || []).find(c => c[0] === 'takeover_apply');
      if (!applyArgs || applyArgs[1].adopt !== false || applyArgs[1].remember !== true) {
        out.errors.push('starting empty with the box still checked must call takeover_apply with adopt false, remember true, got: ' + JSON.stringify(applyArgs && applyArgs[1]));
      }
    }
    clearInterval(heartbeat);
    document.documentElement.style.removeProperty('--smoke-tick');
    layoutScan('main window');
    // 2: seeding is checked unconditionally on every launch (the backend itself
    // decides whether the library was actually empty) — must have been called once
    // by now regardless of the fixture's non-empty PROMPTS array.
    if (!(window.__SMOKE_CALLS__ || []).includes('seed_example_prompts')) {
      out.errors.push('seed_example_prompts was not called on startup');
    }
    // 182: the take-over question must be answered BEFORE seeding runs, or a fresh
    // portable install could seed the examples first and hide a real store behind
    // an already-non-empty library (takeover_offer bails out once prompts exist).
    {
      const calls = window.__SMOKE_CALLS__ || [];
      const iTakeover = calls.indexOf('takeover_offer');
      const iSeed = calls.indexOf('seed_example_prompts');
      if (iTakeover === -1 || iSeed === -1 || iTakeover > iSeed) {
        out.errors.push('takeover_offer must be called before seed_example_prompts, got order: ' + calls.join(','));
      }
    }
    // The fixture library is never empty, so the banner must stay hidden — but the
    // dismiss button still has to exist and be wired correctly for when it isn't.
    {
      const banner = document.getElementById('examples-banner');
      if (!banner || !banner.classList.contains('hidden')) {
        out.errors.push('the examples banner is visible over a non-empty library');
      }
      window.__SMOKE_CALLS__ = [];
      document.getElementById('examples-dismiss').click();
      await sleep(150);
      if (!window.__SMOKE_CALLS__.includes('dismiss_example_prompts')) {
        out.errors.push('the "remove examples" button does not call dismiss_example_prompts');
      }
      // 421: no seeded example ids left after dismiss, so the next render must not
      // spend an extra round trip on the live pending count -- it already knows to
      // stay hidden.
      if (window.__SMOKE_CALLS__.includes('example_prompts_pending')) {
        out.errors.push('421: the examples banner fetched the pending count with no examples seeded');
      }
    }
    // 20: help is opened from a button INSIDE settings, so settings stays open
    // behind it — both used to share the same base z-index, so help rendered
    // under the very dialog it was opened from.
    {
      document.getElementById('gear').click();
      await sleep(120);
      document.getElementById('help-open').click();
      await sleep(150);
      const helpModal = document.querySelector('#help .modal');
      const r = helpModal.getBoundingClientRect();
      const topEl = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      if (!helpModal.contains(topEl)) out.errors.push('20: help dialog does not stack above settings');
      document.getElementById('help-close').click();
      await sleep(120);
      document.getElementById('settings').classList.add('hidden');
    }
    // 76: About dialog — version fills in, no raw i18n key, closes again.
    {
      document.getElementById('gear').click();
      await sleep(120);
      document.getElementById('about-btn').click();
      await sleep(120);
      const aboutModal = document.getElementById('about-modal');
      if (aboutModal.classList.contains('hidden')) {
        out.errors.push('76: the About dialog did not open');
      } else {
        layoutScan('about dialog');
        const verText = document.getElementById('about-version').textContent.trim();
        if (!verText) out.errors.push('76: the About dialog version is empty');
        for (const el of aboutModal.querySelectorAll('*')) {
          const txt = (el.children.length ? '' : el.textContent || '').trim();
          if (/^about[A-Z][A-Za-z]+$/.test(txt)) out.errors.push('76: About dialog shows a raw i18n key: ' + txt);
        }
        // 516: the three link buttons form a full-width column, so the Close
        // button below them is the one centered on its own; the FIRST link
        // button sits as far from the modal's left edge as the Close button's
        // right edge sits from the modal's right edge -- checking .modal-actions
        // itself would be wrong, that flex container always spans the full inner
        // width regardless of justify-content; only its child moves.
        const aboutBox = aboutModal.querySelector('.modal');
        const firstBtn = document.getElementById('about-whats-new');
        const closeBtn = document.getElementById('about-ok');
        // Skipped once the box actually scrolls: a shown scrollbar eats into
        // the content width on the right only, which shifts the centered
        // button without the row itself being off center.
        if (aboutBox && firstBtn && closeBtn && aboutBox.scrollHeight <= aboutBox.clientHeight + 1) {
          const mb = aboutBox.getBoundingClientRect();
          const fb = firstBtn.getBoundingClientRect();
          const cb = closeBtn.getBoundingClientRect();
          const leftGap = fb.left - mb.left;
          const rightGap = mb.right - cb.right;
          if (Math.abs(leftGap - rightGap) > 1) {
            out.errors.push('516: About dialog Close button is not centered, left gap ' + Math.round(leftGap) + 'px vs right gap ' + Math.round(rightGap) + 'px');
          }
        }
        // 516: every link button fits its own text (no horizontal overflow),
        // the box itself never grows wider than its column (a tall box just
        // scrolls, same as the version-history and theme-editor dialogs), and
        // Close stays reachable by scrolling to it -- in the current language
        // and at the current window size (this probe runs at every SIZES
        // entry, including the app minimum, and once more at 130% scale
        // further down, and once in each of pl/ro/de below).
        const aboutFitsCheck = (label) => {
          for (const btn of aboutModal.querySelectorAll('.about-link-btn')) {
            if (btn.scrollWidth > btn.clientWidth + 1) {
              out.errors.push('516: an About link button overflows (' + label + '): ' + btn.textContent.trim());
            }
          }
          if (aboutBox.scrollWidth > aboutBox.clientWidth + 1) {
            out.errors.push('516: the About dialog is wider than its own box (' + label + ')');
          }
          const closeBtn2 = document.getElementById('about-ok');
          closeBtn2.scrollIntoView({ block: 'nearest' });
          const cr = closeBtn2.getBoundingClientRect();
          const hit = document.elementFromPoint(cr.left + cr.width / 2, cr.top + cr.height / 2);
          if (!closeBtn2.contains(hit)) {
            out.errors.push('516: the About dialog Close button is not reachable (' + label + ')');
          }
        };
        aboutFitsCheck('default');
        window.__ABOUT_FITS_CHECK__ = aboutFitsCheck;
        document.getElementById('about-ok').click();
        await sleep(120);
        if (!aboutModal.classList.contains('hidden')) out.errors.push('76: the About dialog did not close');
      }
      document.getElementById('settings').classList.add('hidden');
    }
    // 230: closing the settings panel through its own close button returns
    // keyboard focus to whatever had it before the panel opened (settingsOpener,
    // set in the gear buttons own click handler) instead of dropping it to
    // document.body.
    {
      const gearBtn = document.getElementById('gear');
      gearBtn.focus();
      gearBtn.click();
      await sleep(120);
      if (document.getElementById('settings').classList.contains('hidden')) {
        out.errors.push('230: the settings panel did not open from the gear button');
      } else {
        // Move focus away from the gear button first (as it would move once the
        // user interacts with anything inside the open panel) -- .click() alone
        // never moves focus, so without this the gear button would trivially
        // still be focused regardless of whether hideSettings() restores it.
        const closeBtn = document.getElementById('settings-close');
        closeBtn.focus();
        closeBtn.click();
        await sleep(120);
        if (document.activeElement !== gearBtn) {
          const got = document.activeElement && (document.activeElement.id || document.activeElement.tagName);
          out.errors.push('230: closing settings did not return focus to the gear button, landed on ' + got);
        }
      }
    }
    // 97/21: in-app documentation — every tab reaches the page, none is empty,
    // search narrows the list, a tip callout shows up somewhere.
    {
      document.getElementById('help-open').click();
      await sleep(150);
      const helpEl = document.getElementById('help');
      if (helpEl.classList.contains('hidden')) {
        out.errors.push('the help menu did not open');
      } else {
        layoutScan('help menu');
        mustReach('help menu', '#help-close, #help-search');
        const groups = () => [...document.querySelectorAll('#help-body .help-group')];
        const rawKeyCheck = (where) => {
          for (const el of document.querySelectorAll('#help-tabs-bar *, #help-body *')) {
            const txt = (el.children.length ? '' : el.textContent || '').trim();
            if (/^help[A-Z][A-Za-z]+$/.test(txt)) out.errors.push(where + ' shows a raw i18n key: ' + txt);
          }
        };
        // 21: one tab renders at a time now — click through every one, summing what
        // each shows instead of expecting them all stacked in a single render.
        const tabs = [...document.querySelectorAll('#help-tabs-bar .expert-tab')];
        if (tabs.length < 2) out.errors.push('the help menu shows too few section tabs (' + tabs.length + ')');
        let totalGroups = 0;
        let sawTip = false;
        let sawSketch = false;
        let sawIcon = false;
        const seenTitles = new Set();
        for (let i = 0; i < tabs.length; i++) {
          document.querySelectorAll('#help-tabs-bar .expert-tab')[i].click();
          await sleep(120);
          layoutScan('help menu tab ' + i);
          rawKeyCheck('help menu tab ' + i);
          const here = groups();
          if (!here.length) out.errors.push('help tab ' + i + ' shows no categories');
          totalGroups += here.length;
          const empties = here.filter(g => !g.querySelector('.help-overview').textContent.trim());
          if (empties.length) out.errors.push(empties.length + ' help category has an empty overview (tab ' + i + ')');
          if (document.querySelector('#help-body .help-tip')) sawTip = true;
          if (document.querySelector('#help-body .help-sketch svg')) sawSketch = true;
          // 500: a {icon:key} placeholder in ui/help.js is swapped for the real
          // button's own svg at render time, never left as raw text, never empty.
          const icons = [...document.querySelectorAll('#help-body .help-icon')];
          if (icons.length) {
            sawIcon = true;
            for (const ic of icons) {
              const svg = ic.querySelector('svg');
              if (!svg || !svg.innerHTML.trim()) {
                out.errors.push('500: a help icon placeholder has no real button svg (tab ' + i + ')');
              }
            }
          }
          if (document.getElementById('help-body').textContent.includes('{icon:')) {
            out.errors.push('500: a raw {icon: placeholder is visible in the help text (tab ' + i + ')');
          }
          for (const g of here) {
            const t = g.querySelector('.help-group-title-text');
            if (t) seenTitles.add(t.textContent.trim());
          }
        }
        // 186: the four new help sections actually render, each on its own tab pass.
        for (const title of ['Introduction tour', 'Screenshot editor', 'Text recognition (OCR)', 'Language packs']) {
          if (!seenTitles.has(title)) out.errors.push('186: help category "' + title + '" never rendered');
        }
        if (totalGroups < 15) {
          out.errors.push('the help menu shows too few categories across all tabs (' + totalGroups + ')');
        }
        if (!sawTip) out.errors.push('no help category shows a tip callout');
        if (!sawSketch) out.errors.push('no help category shows a diagram');
        if (!sawIcon) out.errors.push('500: no help category shows a real button icon inline');
        // 500: every icon a placeholder can point at is a button that actually
        // exists in the program, not a name help.js made up.
        for (const id of ['snip-btn', 'record-btn', 'ocr-btn', 'voice-btn', 'library-btn', 'theme-edit-open']) {
          if (!document.getElementById(id)) out.errors.push('500: help icon source button #' + id + ' does not exist');
        }
        const search = document.getElementById('help-search');
        // Multi-word query whose words are not literally adjacent anywhere — only the
        // term-AND-logic search (73) finds it, a plain includes() search finds nothing.
        search.value = 'grid tags';
        search.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(200);
        const filtered = groups();
        if (!filtered.length || filtered.length >= totalGroups) {
          out.errors.push('searching the help menu did not narrow the category list');
        }
        const hitsEl = document.querySelector('#help-body .help-hits');
        if (!hitsEl || !/[0-9]/.test(hitsEl.textContent)) out.errors.push('the help search shows no hit counter');
        if (filtered.length > 1 && !filtered[0].classList.contains('collapsed')) {
          out.errors.push('help search results with more than one hit are not collapsed by default');
        }
        search.value = 'zzz_no_such_help_topic_zzz';
        search.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(200);
        if (groups().length) out.errors.push('a nonsense help search still shows categories');
        if (!document.querySelector('#help-body .hint')) out.errors.push('an empty help search shows no "no results" hint');
        search.value = '';
        search.dispatchEvent(new Event('input', { bubbles: true }));
        document.getElementById('help-close').click();
        await sleep(150);
        if (!document.getElementById('help').classList.contains('hidden')) {
          out.errors.push('the help menu did not close');
        }
      }
    }
    // Window controls and composer buttons are the ones a squeezed header used to
    // carry off the right edge, where no pointer could ever follow them.
    mustReach('main window', '.win-btn, .header-actions > button, .composer > button');
    // P20: the library preview's line count is an expert value (default 2),
    // not a hardcoded CSS number.
    if (getComputedStyle(document.documentElement).getPropertyValue('--lib-preview-lines').trim() !== '2') {
      out.errors.push('the library preview line-count CSS var is not applied');
    }
    // P32: font/size sit right under the name row, not down by the prompt text —
    // kept, just moved. A pure DOM-order check, no need to open the editor.
    {
      const nameRow = document.querySelector('#modal .modal-name-row');
      const styleRow = document.getElementById('modal-style-row');
      const textWrap = document.getElementById('modal-text-wrap');
      if (!nameRow || !styleRow || !textWrap) {
        out.errors.push('the editor is missing the name row, style row or text wrap');
      } else {
        const pos = nameRow.compareDocumentPosition(styleRow);
        if (!(pos & Node.DOCUMENT_POSITION_FOLLOWING)) {
          out.errors.push('the font/size row does not come after the name row');
        }
        const pos2 = styleRow.compareDocumentPosition(textWrap);
        if (!(pos2 & Node.DOCUMENT_POSITION_FOLLOWING)) {
          out.errors.push('the font/size row is not above the prompt text anymore');
        }
      }
    }
    // The tab strip is the piece the header used to crush: below ~700px it kept a
    // ~45px sliver of a 244px strip, and every tab after the first became untouchable.
    const strip = document.getElementById('views');
    if (strip && !strip.classList.contains('hidden')) {
      const stripTabs = [...strip.querySelectorAll('.view-btn')];
      const widest = Math.max(0, ...stripTabs.map(t => t.getBoundingClientRect().width));
      if (strip.clientWidth < Math.min(widest, 90)) {
        out.layout.push('main window: the view tab strip is only ' + strip.clientWidth +
          'px wide (one tab is ' + Math.round(widest) + 'px)');
      }
      const missed = stripTabs.filter(t => !canHit(t));
      if (missed.length) {
        out.layout.push('main window: ' + missed.length + ' of ' + stripTabs.length +
          ' view tabs cannot be clicked');
      }
      strip.scrollLeft = 0;
    }
    // F27: the grid folder draws its own cell, hides its members as tiles and opens
    // a popover holding them.
    const fTile = document.querySelector('.tile.folder-tile');
    if (!fTile) {
      out.errors.push('the grid folder has no tile');
    } else {
      if (document.querySelector('.tile[data-id="p4"]')) {
        out.errors.push('a prompt inside the folder is still a tile of its own');
      }
      // No member-count badge on a folder tile: the preview identifies it, a number
      // on top of it was noise.
      if (fTile.querySelector('.folder-count')) out.errors.push('the folder tile still carries a count badge');
      const anchor = fTile.getBoundingClientRect();
      fTile.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: 5, clientY: 5 }));
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, clientX: 5, clientY: 5 }));
      await sleep(200);
      const pop = document.querySelector('.tile-folder-pop:not(.hidden)');
      const members = pop ? pop.querySelectorAll('.folder-member-tile').length : 0;
      if (members !== 5) out.errors.push('the open folder shows ' + members + ' of 5 entries');
      // 30: member cells now size from tileFolderCols + the popover width (bigger
      // than the old fixed 84x56 box) and no member's name text overflows its cell.
      if (pop) {
        const cellRect = pop.querySelector('.folder-member-tile')?.getBoundingClientRect();
        if (!cellRect || cellRect.width <= 84) {
          out.errors.push('30: folder member cells did not grow past the old 84px width (' + (cellRect && cellRect.width) + ')');
        }
        // Compare rendered rects, not scrollHeight/clientHeight: the auto-fit
        // text-box-trim on .tile-name.fit intentionally reports a layout height
        // smaller than its line box (optical centering) without actually
        // clipping any glyph, so that pairing is not a reliable overflow signal.
        const overflowing = [...pop.querySelectorAll('.folder-member-tile')].filter((btn) => {
          const nameEl = btn.querySelector('.tile-name');
          if (!nameEl || getComputedStyle(nameEl).display === 'none') return false;
          const b = btn.getBoundingClientRect(), n = nameEl.getBoundingClientRect();
          return n.left < b.left - 1 || n.right > b.right + 1 || n.top < b.top - 1 || n.bottom > b.bottom + 1;
        });
        if (overflowing.length) out.errors.push('30: ' + overflowing.length + ' folder member name(s) overflow their cell');
      }
      // A popover taller than the room below must shrink and scroll, NOT jump to the
      // top of the window and cover the tile it was opened from.
      if (pop) {
        const r = pop.getBoundingClientRect();
        const clear = r.top >= anchor.bottom - 1 || r.bottom <= anchor.top + 1;
        if (!clear) {
          out.errors.push('the folder popover covers the tile it belongs to (' +
            Math.round(r.top) + '..' + Math.round(r.bottom) + ' vs tile ' +
            Math.round(anchor.top) + '..' + Math.round(anchor.bottom) + ')');
        }
      }
      layoutScan('grid folder');
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
      await sleep(120);
      // 70: dragging in the colour picker fires on every pointer-move frame — must
      // paint just this one folder tile in place, not rebuild the whole grid.
      fTile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(150);
      const colorBtn = document.querySelectorAll('.tab-menu button')[1];
      if (!colorBtn) {
        out.errors.push('70: the grid folder context menu has no colour option');
      } else {
        colorBtn.click();
        await sleep(150);
        const pick = document.querySelector('.color-pop:not(.hidden)');
        const sv = pick ? pick.querySelector('.cp-sv') : null;
        if (!pick || !sv) {
          out.errors.push('70: the folder colour picker did not open');
        } else {
          const r = sv.getBoundingClientRect();
          sv.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 1, clientX: r.left + 5, clientY: r.top + 5 }));
          sv.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: r.left + 25, clientY: r.top + 25 }));
          await sleep(30);
          if (document.querySelector('.tile.folder-tile') !== fTile) {
            out.errors.push('70: painting a folder colour rebuilt the whole grid instead of the one tile');
          }
          if (!fTile.style.background) {
            out.errors.push('70: the folder tile did not pick up the live colour preview');
          }
        }
        document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
        await sleep(120);
      }
      // Dissolving must not just remove the folder tile — every member has to land
      // back on the grid as its own tile, not vanish into the library.
      fTile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(150);
      const dissolveBtn = document.querySelector('.tab-menu button.danger');
      if (!dissolveBtn) {
        out.errors.push('the grid folder context menu has no dissolve option');
      } else {
        dissolveBtn.click();
        await sleep(150);
        const cmDissolve = document.getElementById('confirm-modal');
        if (cmDissolve && !cmDissolve.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
        if (document.querySelector('.tile.folder-tile')) {
          out.errors.push('the folder tile survived dissolving it');
        }
        for (const mid of ['p4', 'p6', 'p8', 'p10', 'p11']) {
          if (!document.querySelector('.tile[data-id="' + mid + '"]')) {
            out.errors.push('dissolving the folder did not bring back ' + mid + ' as a tile');
          }
        }
      }
    }
    // P50: right-clicking a view tab's "Duplicate" entry creates a new, empty view
    // with the same name (+ suffix), color and grid size — cleaned up right after
    // so it doesn't throw off the view-count checks further down.
    {
      const homeTab = document.querySelector('.view-btn[data-view-id="home"]');
      if (!homeTab) {
        out.errors.push('the "home" view tab is missing for the duplicate-view check');
      } else {
        const tabCountBefore = document.querySelectorAll('#views .view-btn:not(.view-add-top)').length;
        const menuItem = (text) => [...document.querySelectorAll('.tab-menu button[role="menuitem"]')]
          .find(el => el.textContent.trim() === text);
        homeTab.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }));
        await sleep(150);
        const dupItem = menuItem('Duplicate');
        if (!dupItem) {
          out.errors.push('the view tab context menu has no Duplicate entry');
          // English-only lookup above: under another language this never matches,
          // so the menu has to be closed by hand (its own dismiss listens for a
          // pointerdown outside it) or it leaks into later layout checks, sitting
          // wherever it was opened.
          document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
          await sleep(100);
        } else {
          dupItem.click();
          await sleep(300);
          const tabsAfter = [...document.querySelectorAll('#views .view-btn:not(.view-add-top)')];
          if (tabsAfter.length !== tabCountBefore + 1) {
            out.errors.push('duplicating a view did not add exactly one tab');
          } else {
            const newTab = tabsAfter.find(b => (b.querySelector('span') || {}).textContent === 'Home (copy)');
            if (!newTab) {
              out.errors.push('the duplicated view is not named "Home (copy)"');
            } else {
              // Clean up: delete the duplicate (2-click arm+confirm) so later view-count
              // assertions see the same tab bar they expect.
              newTab.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }));
              await sleep(150);
              const editItem = menuItem('Edit view');
              if (editItem) editItem.click();
              await sleep(200);
              const delBtn = document.getElementById('view-modal-delete');
              if (delBtn && !delBtn.classList.contains('hidden')) {
                delBtn.click(); await sleep(50);
                delBtn.click(); await sleep(300);
              }
              // 434: the deleted view offers an Undo button — click it and prove the
              // view (name, member) really comes back, not just a bare empty tab.
              const viewUndoToast = document.getElementById('toast');
              const viewUndoBtn = viewUndoToast.querySelector('.toast-btn');
              if (!viewUndoToast.classList.contains('actionable') || !viewUndoBtn) {
                out.errors.push('434: deleting a view did not offer an undo toast');
              } else {
                viewUndoBtn.click();
                await sleep(300);
                const tabsRestored = [...document.querySelectorAll('#views .view-btn:not(.view-add-top)')];
                if (tabsRestored.length !== tabCountBefore + 1) {
                  out.errors.push('434: undoing a deleted view did not bring the tab back');
                } else if (!tabsRestored.some(b => (b.querySelector('span') || {}).textContent === 'Home (copy)')) {
                  out.errors.push('434: the restored view lost its name');
                }
                if (viewUndoToast.classList.contains('show')) {
                  out.errors.push('434: the view-undo toast is still showing after being clicked');
                }
                // Delete it again for real (no undo this time) so the tab bar ends
                // this check exactly where later view-count assertions expect it.
                const restoredTab = tabsRestored.find(b => (b.querySelector('span') || {}).textContent === 'Home (copy)');
                if (restoredTab) {
                  restoredTab.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }));
                  await sleep(150);
                  const editItem2 = menuItem('Edit view');
                  if (editItem2) editItem2.click();
                  await sleep(200);
                  const delBtn2 = document.getElementById('view-modal-delete');
                  if (delBtn2 && !delBtn2.classList.contains('hidden')) {
                    delBtn2.click(); await sleep(50);
                    delBtn2.click(); await sleep(300);
                  }
                }
              }
              const tabsFinal = document.querySelectorAll('#views .view-btn:not(.view-add-top)').length;
              if (tabsFinal !== tabCountBefore) {
                out.errors.push('cleanup after the duplicate-view check left an extra tab behind');
              }
            }
          }
        }
      }
    }
    // 9: double-click an empty cell creates a prompt right there instead of typing
    // below and dragging it over. Done on "work" (an untouched, empty view) so it
    // never competes with the folder math above for a free cell.
    const workTab = document.querySelector('.view-btn[data-view-id="work"]');
    if (!workTab) {
      out.errors.push('the "work" view tab is missing for the double-click-create check');
    } else {
      workTab.click();
      await sleep(200);
      // P53: the grid keeps a CSS var of the active view's color, ready for the
      // (default-off) grid-border option even while the flag itself is off.
      if (getComputedStyle(document.documentElement).getPropertyValue('--active-view-color').trim() !== '#ef4444') {
        out.errors.push('the active-view-color CSS var did not follow the switch to "work"');
      }
      const cell00 = document.querySelector('#grid .cell[data-col="0"][data-row="0"]');
      if (!cell00 || cell00.firstElementChild) {
        out.errors.push('expected an empty cell on "work" to double-click-create into');
      } else {
        cell00.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(200);
        const modalEl = document.getElementById('modal');
        if (!modalEl || modalEl.classList.contains('hidden')) {
          out.errors.push('double-clicking an empty cell did not open the create dialog');
        } else {
          document.getElementById('modal-name').value = 'Grid Click Prompt';
          document.getElementById('modal-text').value = 'Created by double-click.';
          document.getElementById('modal-confirm').click();
          await sleep(250);
          const placed = document.querySelector('#grid .cell[data-col="0"][data-row="0"] .tile');
          if (!placed) {
            out.errors.push('double-click-create did not land the new prompt on the clicked cell');
          } else if ((placed.querySelector('.tile-name') || {}).textContent !== 'Grid Click Prompt') {
            out.errors.push('the tile on the double-clicked cell is not the prompt just created');
          }
          // P89: creating a prompt runs the same near-duplicate check import does.
          await sleep(200);
          const toastEl2 = document.getElementById('toast');
          if (!(toastEl2.textContent || '').toLowerCase().includes('duplicate')) {
            out.errors.push('creating a prompt did not warn about possible duplicates: ' + toastEl2.textContent);
          }
        }
      }
      // 34: a {#{ }#} placeholder with no name inside warns before the save goes
      // through, and "save anyway" still lets it through.
      const cell10 = document.querySelector('#grid .cell[data-col="1"][data-row="0"]');
      if (!cell10 || cell10.firstElementChild) {
        out.errors.push('expected a second empty cell on "work" for the empty-variable check');
      } else {
        cell10.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(200);
        document.getElementById('modal-name').value = 'Empty Var Prompt';
        document.getElementById('modal-text').value = 'Say {#{ }#} please.';
        document.getElementById('modal-confirm').click();
        await sleep(200);
        const cm = document.getElementById('confirm-modal');
        if (!cm || cm.classList.contains('hidden')) {
          out.errors.push('saving a prompt with an unnamed {#{ }#} variable did not warn');
        } else {
          if (document.getElementById('confirm-title').textContent !== 'Empty variable') {
            out.errors.push('the empty-variable warning has the wrong title');
          }
          document.getElementById('confirm-cancel').click(); // back to editing, not saved
          await sleep(150);
          if (document.getElementById('modal').classList.contains('hidden')) {
            out.errors.push('cancelling the empty-variable warning closed the editor instead of returning to it');
          } else {
            document.getElementById('modal-confirm').click(); // through again
            await sleep(200);
            document.getElementById('confirm-ok').click(); // save anyway
            await sleep(250);
            if (!document.querySelector('#grid .cell[data-col="1"][data-row="0"] .tile')) {
              out.errors.push('"save anyway" did not actually save the prompt');
            }
          }
        }
      }
      // A name or body that looks like markup must render as literal text, never
      // as a real element -- the one place a raw HTML string could ever reach the
      // page (the Markdown preview) escapes up front; every other renderer here
      // uses textContent, so this proves the tile path stays that way too.
      const xssCell = document.querySelector('#grid .cell[data-col="2"][data-row="0"]');
      if (!xssCell || xssCell.firstElementChild) {
        out.errors.push('expected a third empty cell on "work" for the injected-markup check');
      } else {
        xssCell.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(200);
        document.getElementById('modal-name').value = XSS_NAME;
        document.getElementById('modal-text').value = XSS_TEXT;
        document.getElementById('modal-confirm').click();
        await sleep(250);
        const xssTile = document.querySelector('#grid .cell[data-col="2"][data-row="0"] .tile');
        if (!xssTile) {
          out.errors.push('the injected-markup prompt was not placed on the grid');
        } else {
          const tileName = xssTile.querySelector('.tile-name');
          if (!tileName || tileName.textContent !== XSS_NAME) {
            out.errors.push('the grid tile does not show injected markup as literal text');
          } else if (tileName.querySelector('img') || tileName.querySelector('script')) {
            out.errors.push('the grid tile turned injected markup into a real element');
          }
        }
      }
      const homeTab = document.querySelector('.view-btn[data-view-id="home"]');
      if (homeTab) { homeTab.click(); await sleep(200); }
    }
    await click('library-btn');
    layoutScan('library');
    mustReach('library', '#library .modal-head-actions > button');
    {
      const xssRow = [...document.querySelectorAll('#library-list .lib-item')]
        .find((r) => (r.querySelector('.lib-name') || {}).textContent === XSS_NAME);
      if (!xssRow) {
        out.errors.push('the injected-markup prompt is missing from the library list');
      } else if (xssRow.querySelector('.lib-name img') || xssRow.querySelector('.lib-name script')) {
        out.errors.push('the library row turned injected markup into a real element');
      }
    }
    // 17: "last edited" sort — from the last_edited_map fixture (p3 newest, then
    // p1, then p9; everything else — including the favorited p0/p5/p10, which
    // float to the top independently of this sort — has no version row and
    // sorts to the end).
    {
      const sortSel = document.getElementById('library-sort');
      if (!sortSel || ![...sortSel.options].some(o => o.value === 'lastEdited')) {
        out.errors.push('the library sort dropdown has no "last edited" option');
      } else {
        sortSel.value = 'lastEdited';
        sortSel.dispatchEvent(new Event('change'));
        await sleep(250);
        const order = [...document.querySelectorAll('#library-list .lib-item')].map(el => el.dataset.id);
        const idx = (id) => order.indexOf(id);
        if (idx('p3') < 0 || idx('p1') < 0 || idx('p9') < 0) {
          out.errors.push('"last edited" sort is missing a fixture prompt: ' + order.join(','));
        } else if (!(idx('p3') < idx('p1') && idx('p1') < idx('p9'))) {
          out.errors.push('"last edited" sort order is wrong: ' + order.join(','));
        } else if (idx('p2') >= 0 && idx('p2') < idx('p9')) {
          out.errors.push('a prompt with no edit history sorted before one that has it');
        }
        sortSel.value = 'default';
        sortSel.dispatchEvent(new Event('change'));
        await sleep(200);
      }
    }
    // 423 setup: 2 disposable tags for the merge check below, added to a single
    // prompt (not the fixture's shared "Draft"/"Work") so later assertions that
    // depend on those two staying exactly as the fixture made them still hold.
    {
      await click('library-select-btn');
      document.querySelector('#library-list .lib-item[data-id="p1"]')?.click();
      await sleep(150);
      const setupAdd = document.getElementById('lib-batch-tag-add');
      if (!setupAdd) {
        out.errors.push('423: the batch tag add button is missing for merge-test setup');
      } else {
        setupAdd.click();
        await sleep(150);
        document.getElementById('confirm-input').value = 'MergeSrcTmp';
        document.getElementById('confirm-ok').click();
        await sleep(150);
        setupAdd.click();
        await sleep(150);
        document.getElementById('confirm-input').value = 'MergeDstTmp';
        document.getElementById('confirm-ok').click();
        await sleep(150);
      }
      document.getElementById('lib-batch-none').click();
      await click('library-select-btn'); // back out of select mode
    }
    // Tags: the filter row lists them, a chip narrows the list, the manager opens.
    const chips = [...document.querySelectorAll('#library-tagbar .tag-filter')];
    if (chips.length < 2) {
      out.errors.push('the library tag filter row is empty');
    } else {
      const before = document.querySelectorAll('#library-list .lib-item').length;
      chips[0].click();
      await sleep(250);
      const after = document.querySelectorAll('#library-list .lib-item').length;
      if (after >= before) out.errors.push('a tag chip did not narrow the library list');
      const active = document.querySelector('#library-tagbar .tag-filter.active');
      if (!active) out.errors.push('the selected tag chip is not marked active');
      else { active.click(); await sleep(200); }
      const mgr = [...document.querySelectorAll('#library-tagbar .tag-action')].pop();
      if (mgr) { mgr.click(); await sleep(250); }
      const tm = document.getElementById('tag-modal');
      if (tm.classList.contains('hidden')) {
        out.errors.push('the tag manager did not open');
      } else if (!document.querySelectorAll('#tag-modal-list .tag-manage-row').length) {
        out.errors.push('the tag manager listed no tags');
      } else {
        // P27: "Colorize" gives every UNcolored tag a palette color, in order —
        // an already-colored tag ("Work", fixture blue) must stay untouched.
        const rowFor = (name) => [...document.querySelectorAll('#tag-modal-list .tag-manage-row')]
          .find(r => (r.querySelector('.tag-manage-name') || {}).textContent === name);
        const workDot = rowFor('Work')?.querySelector('.tag-dot');
        const workBefore = workDot?.style.getPropertyValue('--chip-color');
        document.getElementById('tag-colorize')?.click();
        await sleep(300);
        const workAfter = rowFor('Work')?.querySelector('.tag-dot')?.style.getPropertyValue('--chip-color');
        if (workAfter !== workBefore) out.errors.push('colorize touched a tag that already had a color');
        const draftDot = rowFor('Draft')?.querySelector('.tag-dot');
        if (!draftDot || !draftDot.classList.contains('tag-colored')) {
          out.errors.push('colorize did not give the uncolored tag a color');
        }
        // 423: merge "MergeSrcTmp" into "MergeDstTmp" (the setup above) via the
        // row's Merge button (order: Rename, Merge, Delete) -- disposable tags,
        // never the fixture's own "Draft"/"Work" that other checks rely on.
        const srcRow = rowFor('MergeSrcTmp');
        const mergeBtn = srcRow?.querySelectorAll('.ghost-btn')[1];
        if (!srcRow || !mergeBtn) {
          out.errors.push('423: the tag manager has no Merge button for "MergeSrcTmp"');
        } else {
          const runMerge = async () => {
            rowFor('MergeSrcTmp').querySelectorAll('.ghost-btn')[1].click();
            await sleep(150);
            document.getElementById('confirm-input').value = 'MergeDstTmp';
            document.getElementById('confirm-ok').click(); // target picker: the only dialog
            await sleep(250);
          };
          await runMerge();
          if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
            out.errors.push('676: a second confirmation dialog opened after picking the merge target');
          }
          if (rowFor('MergeSrcTmp')) out.errors.push('423: "MergeSrcTmp" is still listed after merging it into "MergeDstTmp"');
          // The merge ran at once and its toast carries an Undo that brings the
          // source tag back; then merge again so the later checks start as before.
          const mergeUndoBtn = document.getElementById('toast').querySelector('.toast-btn');
          if (!mergeUndoBtn) {
            out.errors.push('676: merging two tags did not offer an undo toast');
          } else {
            mergeUndoBtn.click();
            await sleep(400);
            if (!rowFor('MergeSrcTmp')) out.errors.push('676: undoing a tag merge did not bring the source tag back');
            if (!rowFor('MergeDstTmp')) out.errors.push('676: undoing a tag merge lost the target tag');
            await runMerge();
            if (rowFor('MergeSrcTmp')) out.errors.push('676: merging again after an undo left the source tag listed');
          }
          const stillShown = [...document.querySelectorAll('#library-list .tag-chip')].some((c) => c.textContent === 'MergeSrcTmp');
          if (stillShown) out.errors.push('423: a library row still shows the merged-away tag');
        }
        // 434: deleting a tag offers an Undo button, and clicking it re-adds the
        // tag text to every prompt that had it. Uses the same disposable
        // "MergeDstTmp" tag the merge check above just produced.
        const dstRow = rowFor('MergeDstTmp');
        const delBtn = dstRow?.querySelectorAll('.ghost-btn')[2];
        if (!dstRow || !delBtn) {
          out.errors.push('434: the tag manager has no Delete button for "MergeDstTmp"');
        } else {
          delBtn.click();
          await sleep(150);
          document.getElementById('confirm-ok').click();
          await sleep(250);
          if (rowFor('MergeDstTmp')) out.errors.push('434: "MergeDstTmp" is still listed after deleting it');
          const tagUndoToast = document.getElementById('toast');
          const tagUndoBtn = tagUndoToast.querySelector('.toast-btn');
          if (!tagUndoToast.classList.contains('actionable') || !tagUndoBtn) {
            out.errors.push('434: deleting a tag did not offer an undo toast');
          } else {
            tagUndoBtn.click();
            await sleep(300);
            if (!rowFor('MergeDstTmp')) out.errors.push('434: undoing a deleted tag did not bring it back');
            const chipBack = [...document.querySelectorAll('#library-list .tag-chip')].some((c) => c.textContent === 'MergeDstTmp');
            if (!chipBack) out.errors.push('434: undoing a deleted tag did not restore it on the prompt row');
            if (tagUndoToast.classList.contains('show')) {
              out.errors.push('434: the tag-undo toast is still showing after being clicked');
            }
            // Delete it again for real (no undo) so the tag list ends this
            // check the way it started, minus the disposable merge source.
            const dstRow2 = rowFor('MergeDstTmp');
            const delBtn2 = dstRow2?.querySelectorAll('.ghost-btn')[2];
            if (delBtn2) {
              delBtn2.click();
              await sleep(150);
              document.getElementById('confirm-ok').click();
              await sleep(250);
            }
          }
        }
      }
      await click('tag-modal-close');
    }
    // F17: the search also looks inside the private notes. "Moonshot" lives only in
    // one prompt's note, so a hit can only have come from there.
    const q = document.getElementById('library-q');
    if (!q) {
      out.errors.push('the library has no search field');
    } else {
      q.value = 'moonshot';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const hits = document.querySelectorAll('#library-list .lib-item').length;
      if (hits !== 1) out.errors.push('searching a note matched ' + hits + ' prompts instead of 1');
      q.value = '';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(250);
    }
    // P18: a "-word" term excludes matches instead of narrowing to them —
    // "work" alone should hit more prompts than "work -draft".
    if (q) {
      q.value = 'work';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const withDraft = document.querySelectorAll('#library-list .lib-item').length;
      q.value = 'work -draft';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const withoutDraft = document.querySelectorAll('#library-list .lib-item').length;
      if (!(withoutDraft < withDraft)) {
        out.errors.push('"-draft" did not exclude anything: ' + withDraft + ' -> ' + withoutDraft);
      }
      q.value = '';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(250);
    }
    // 424: tag: and var: are hard filters -- typing one narrows the library
    // to exactly the prompts carrying that tag or placeholder, nothing scored.
    if (q) {
      // Earlier steps above created a couple of untagged test prompts, so the
      // total row count here is not the fixture's original 12 -- the
      // "everyone without Work" count is checked relative to it instead.
      const totalRows = document.querySelectorAll('#library-list .lib-item').length;
      q.value = 'tag:work';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const tagHits = document.querySelectorAll('#library-list .lib-item').length;
      if (tagHits !== 8) out.errors.push('424: "tag:work" matched ' + tagHits + ' prompts instead of 8');
      q.value = '-tag:work';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const tagExcludeHits = document.querySelectorAll('#library-list .lib-item').length;
      if (tagExcludeHits !== totalRows - tagHits) {
        out.errors.push('424: "tag:work" and "-tag:work" do not add up to the full list: ' + tagHits + ' + ' + tagExcludeHits + ' != ' + totalRows);
      }
      q.value = 'var:Text';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const varHits = document.querySelectorAll('#library-list .lib-item').length;
      if (varHits !== 1) out.errors.push('424: "var:Text" matched ' + varHits + ' prompts instead of 1');
      q.value = '';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(250);
    }
    // A library row with an image attachment opens the same actions menu as any
    // other row: edit, edit image, duplicate ... delete, not just "open in editor".
    {
      // Give the fixture row p7 a picture as its attached file for the length of
      // this check (the backend answers, a sync event makes the window refetch).
      const imgPath = 'C:\\\\Pictures\\\\ps-smoke-row.png';
      await window.__TAURI__.core.invoke('update_prompt', { id: 'p7', filePath: imgPath });
      window.__FIRE__('store-synced', null);
      await sleep(400);
      const imgRow = document.querySelector('#library-list .lib-item[data-id="p7"]');
      if (!imgRow) {
        out.errors.push('library image row: the fixture row p7 is missing');
      } else {
        imgRow.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 40, clientY: 40 }));
        await sleep(150);
        const ctxMenu = document.getElementById('ctx');
        const shown = (act) => { const b = ctxMenu.querySelector('[data-act="' + act + '"]'); return !!b && !b.classList.contains('hidden'); };
        if (ctxMenu.classList.contains('hidden')) out.errors.push('library image row: the right-click did not open the actions menu');
        else {
          for (const act of ['edit', 'editImage', 'duplicate', 'delete']) {
            if (!shown(act)) out.errors.push('library image row: the menu has no ' + act + ' entry');
          }
          if (shown('editVideo')) out.errors.push('library image row: the menu offers Edit video for a picture');
          if (document.querySelector('.open-editor-pop:not(.hidden)')) out.errors.push('library image row: the old open-in-editor popup opened instead of the menu');
          window.__SMOKE_ARGS__ = [];
          ctxMenu.querySelector('[data-act="editImage"]').click();
          await sleep(250);
          const openCall = (window.__SMOKE_ARGS__ || []).find((c) => c[0] === 'open_file_in_editor');
          if (!openCall || openCall[1].path !== imgPath) out.errors.push('library image row: Edit image did not open the attached file in the editor: ' + JSON.stringify(openCall));
        }
        document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
        await sleep(100);
      }
      await window.__TAURI__.core.invoke('update_prompt', { id: 'p7', filePath: '' });
      window.__FIRE__('store-synced', null);
      await sleep(400);
    }
    // 435: archiving a prompt hides it from the browsing library without
    // deleting it. The show-archive toggle brings it back, and an active
    // search query still finds it either way -- "Prompt 12" is unused by any
    // other check, so this block owns it start to finish and restores it.
    {
      const row435 = document.querySelector('#library-list .lib-item[data-id="p11"]');
      if (!row435) {
        out.errors.push('435: "Prompt 12" is missing from the library before the archive check');
      } else {
        const countBefore435 = document.getElementById('library-count').textContent;
        row435.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 40 }));
        await sleep(150);
        const archiveBtn435 = document.querySelector('#ctx [data-act="archive"]');
        if (!archiveBtn435 || archiveBtn435.classList.contains('hidden')) {
          out.errors.push('435: the archive action is missing from the library row context menu');
        } else {
          archiveBtn435.click();
          await sleep(200);
          if (document.querySelector('#library-list .lib-item[data-id="p11"]')) {
            out.errors.push('435: an archived prompt is still shown in the library list');
          }
          const countAfter435 = document.getElementById('library-count').textContent;
          const nBefore435 = parseInt(countBefore435, 10);
          const nAfter435 = parseInt(countAfter435, 10);
          if (!(nAfter435 < nBefore435)) {
            out.errors.push('435: the library header count did not drop after archiving a prompt: ' + countBefore435 + ' -> ' + countAfter435);
          }
          // The toggle above the list brings archived prompts back into view.
          document.getElementById('library-archive').click();
          await sleep(200);
          if (!document.querySelector('#library-list .lib-item[data-id="p11"]')) {
            out.errors.push('435: the show-archived toggle did not bring the archived prompt back');
          }
          document.getElementById('library-archive').click();
          await sleep(200);
          if (document.querySelector('#library-list .lib-item[data-id="p11"]')) {
            out.errors.push('435: turning the show-archived toggle back off did not hide it again');
          }
          // A search query still finds an archived prompt with the toggle off.
          q.value = 'Prompt 12';
          q.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(300);
          const row435b = document.querySelector('#library-list .lib-item[data-id="p11"]');
          if (!row435b) {
            out.errors.push('435: searching did not find an archived prompt');
          } else {
            // Unarchive it again, while search still shows it, so later checks
            // see the fixture's original state.
            row435b.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 40 }));
            await sleep(150);
            const unarchiveBtn435 = document.querySelector('#ctx [data-act="archive"]');
            if (unarchiveBtn435) { unarchiveBtn435.click(); await sleep(200); }
          }
          q.value = '';
          q.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(250);
          if (!document.querySelector('#library-list .lib-item[data-id="p11"]')) {
            out.errors.push('435: unarchiving did not bring "Prompt 12" back into the normal list');
          }
        }
      }
    }
    // 436: saved searches keep a query, its active tag filters and the current
    // sort together as one chip. Saving, applying, the cap, renaming and
    // deleting are all self-contained here and end with the list empty again.
    {
      const saveBtn436 = document.getElementById('library-search-save');
      const firstTagChip436 = document.querySelector('#library-tagbar .tag-chip.tag-filter');
      if (!saveBtn436 || !firstTagChip436) {
        out.errors.push('436: the search-save button or a tag filter chip is missing');
      } else {
        // Every tag toggle rebuilds #library-tagbar from scratch, so a chip
        // reference from before that rebuild is stale -- always look it back
        // up by its name instead of holding onto the DOM node.
        const tagName436 = firstTagChip436.textContent;
        const findTagChip436 = () => [...document.querySelectorAll('#library-tagbar .tag-chip.tag-filter')].find((c) => c.textContent === tagName436);
        q.value = 'zzsaved436';
        q.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(250);
        findTagChip436().click();
        await sleep(150);
        const sortSel436 = document.getElementById('library-sort');
        const sortBefore436 = sortSel436.value;
        const altSort436 = [...sortSel436.options].map((o) => o.value).find((v) => v !== sortBefore436);
        if (altSort436) {
          sortSel436.value = altSort436;
          sortSel436.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(150);
        }
        saveBtn436.click();
        await sleep(200);
        document.getElementById('confirm-input').value = 'SmokeSaved1';
        document.getElementById('confirm-ok').click();
        await sleep(250);
        const chipsOf436 = () => [...document.querySelectorAll('#library-saved-searches .lib-saved-search-chip')];
        if (!chipsOf436().some((c) => c.textContent === 'SmokeSaved1')) {
          out.errors.push('436: saving a search did not add a chip for it');
        }
        // Forget the search, then bring it back with one click.
        q.value = '';
        q.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(200);
        findTagChip436().click(); // deselect
        await sleep(150);
        if (altSort436) { sortSel436.value = sortBefore436; sortSel436.dispatchEvent(new Event('change', { bubbles: true })); await sleep(150); }
        const savedChip436 = chipsOf436().find((c) => c.textContent === 'SmokeSaved1');
        savedChip436?.click();
        await sleep(250);
        if (q.value !== 'zzsaved436') out.errors.push('436: clicking a saved search did not restore the query text');
        if (!findTagChip436()?.classList.contains('active')) out.errors.push('436: clicking a saved search did not restore its tag filter');
        if (altSort436 && sortSel436.value !== altSort436) out.errors.push('436: clicking a saved search did not restore its sort');
        // Leave the tag filter the way this block found it, so later checks see
        // the full, unfiltered library list again.
        findTagChip436()?.click();
        await sleep(150);
        // Persists across closing and reopening the library.
        await click('library-close');
        await sleep(150);
        await click('library-btn');
        await sleep(250);
        if (!chipsOf436().some((c) => c.textContent === 'SmokeSaved1')) {
          out.errors.push('436: a saved search did not survive closing and reopening the library');
        }
        // The cap: 4 more reach the default limit of 5, a 6th is refused and
        // names the oldest one instead of silently dropping it.
        for (let i = 2; i <= 5; i++) {
          saveBtn436.click();
          await sleep(200);
          document.getElementById('confirm-input').value = 'SmokeSaved' + i;
          document.getElementById('confirm-ok').click();
          await sleep(200);
        }
        if (chipsOf436().length !== 5) {
          out.errors.push('436: expected exactly 5 saved searches at the cap, found ' + chipsOf436().length);
        }
        const toastBefore436 = document.getElementById('toast').textContent;
        saveBtn436.click();
        await sleep(200);
        const dialogOpen436 = !document.getElementById('confirm-modal').classList.contains('hidden');
        if (dialogOpen436) {
          out.errors.push('436: saving past the cap opened a dialog instead of being refused up front');
          document.getElementById('confirm-cancel').click();
          await sleep(150);
        } else {
          const toastAfter436 = document.getElementById('toast').textContent;
          if (toastAfter436 === toastBefore436 || !toastAfter436.includes('SmokeSaved1')) {
            out.errors.push('436: the at-cap message did not name the oldest saved search');
          }
        }
        if (chipsOf436().length !== 5) out.errors.push('436: a refused save past the cap changed the saved search count');
        // Rename, then delete, through the same right-click dialog.
        const renameTarget436 = chipsOf436().find((c) => c.textContent === 'SmokeSaved2');
        if (!renameTarget436) {
          out.errors.push('436: "SmokeSaved2" is missing before the rename check');
        } else {
          renameTarget436.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 20, clientY: 20 }));
          await sleep(150);
          document.getElementById('confirm-input').value = 'SmokeRenamed436';
          document.getElementById('confirm-ok').click();
          await sleep(250);
          if (!chipsOf436().some((c) => c.textContent === 'SmokeRenamed436')) {
            out.errors.push('436: renaming a saved search did not change its chip label');
          }
          if (chipsOf436().some((c) => c.textContent === 'SmokeSaved2')) {
            out.errors.push('436: the old name is still shown after renaming a saved search');
          }
        }
        // Clean up every chip this block created, through its own delete path,
        // so later checks see an empty saved-search row again.
        let guard436 = 0;
        while (chipsOf436().length && guard436 < 10) {
          const c = chipsOf436()[0];
          c.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 20, clientY: 20 }));
          await sleep(150);
          document.getElementById('confirm-alt').click();
          await sleep(200);
          guard436++;
        }
        if (chipsOf436().length) out.errors.push('436: deleting saved searches did not empty the list again');
      }
    }
    // 437: the list/tiles toggle draws the very same rows differently -- a
    // selection survives the switch either way, and neither layout scrolls
    // the window horizontally.
    {
      const viewToggle437 = document.getElementById('library-view-toggle');
      const list437 = document.getElementById('library-list');
      if (!viewToggle437 || !list437) {
        out.errors.push('437: the library list/tiles toggle is missing');
      } else {
        await click('library-select-btn');
        document.getElementById('lib-batch-all').click();
        await sleep(150);
        const selectedBefore437 = document.querySelectorAll('#library-list .lib-item.lib-selected').length;
        if (!selectedBefore437) out.errors.push('437: "select all" selected nothing before the tiles check');
        viewToggle437.click();
        await sleep(300);
        if (!list437.classList.contains('lib-tiles-view')) {
          out.errors.push('437: clicking the view toggle did not switch to tiles');
        }
        const selectedInTiles437 = document.querySelectorAll('#library-list .lib-item.lib-selected').length;
        if (selectedInTiles437 !== selectedBefore437) {
          out.errors.push('437: switching to tiles changed the selection (' + selectedBefore437 + ' -> ' + selectedInTiles437 + ')');
        }
        layoutScan('library tiles');
        viewToggle437.click();
        await sleep(300);
        if (list437.classList.contains('lib-tiles-view')) {
          out.errors.push('437: clicking the view toggle again did not switch back to the list');
        }
        const selectedBack437 = document.querySelectorAll('#library-list .lib-item.lib-selected').length;
        if (selectedBack437 !== selectedBefore437) {
          out.errors.push('437: switching back to the list changed the selection (' + selectedBefore437 + ' -> ' + selectedBack437 + ')');
        }
        document.getElementById('lib-batch-none').click();
        await sleep(150);
        await click('library-select-btn'); // exit select mode
      }
    }
    // P25: the 2 most-used tags across the fixture (Work, Draft) are offered as
    // one-click chips on an untagged prompt, and vanish once actually added.
    {
      const row = document.querySelector('#library-list .lib-item[data-id="p2"]');
      const editBtn = row && row.querySelector('.lib-edit');
      if (!editBtn) {
        out.errors.push('library row p2 has no edit icon for the quickadd check');
      } else {
        editBtn.click();
        await sleep(250);
        // 28: tags now live inside the "Advanced options" collapsible.
        document.getElementById('modal-advanced-head').click();
        await sleep(200);
        const qa = document.getElementById('modal-tag-quickadd');
        const chips = qa ? [...qa.querySelectorAll('.tag-chip')] : [];
        const names = chips.map(c => (c.querySelector('.tag-chip-label') || {}).textContent);
        if (!names.includes('Work') || !names.includes('Draft')) {
          out.errors.push('the tag quick-add row is missing an expected tag: ' + names.join(','));
        }
        const workChip = chips.find(c => (c.querySelector('.tag-chip-label') || {}).textContent === 'Work');
        if (workChip) {
          workChip.click();
          await sleep(150);
          const assigned = [...document.querySelectorAll('#modal-tag-chips .tag-chip-label')].map(s => s.textContent);
          if (!assigned.includes('Work')) out.errors.push('clicking a quick-add tag did not add it to the prompt');
          const stillOffered = [...document.querySelectorAll('#modal-tag-quickadd .tag-chip-label')].map(s => s.textContent);
          if (stillOffered.includes('Work')) out.errors.push('an already-added tag is still offered in quick-add');
        }
        document.getElementById('modal-close-x').click();
        await sleep(200);
      }
    }
    // 339: library batch bar's export control -- same select+button pattern
    // as the settings page, now also offering JSON, gated on a real selection.
    {
      await click('library-select-btn');
      document.getElementById('lib-batch-all').click();
      await sleep(150);
      const fmtSel = document.getElementById('lib-batch-format');
      const expBtn = document.getElementById('lib-batch-export');
      if (!fmtSel || !expBtn) {
        out.errors.push('the library batch bar export control (format select + button) is missing');
      } else {
        const optValues = [...fmtSel.options].map(o => o.value);
        if (!['csv', 'txt', 'json'].every(v => optValues.includes(v))) {
          out.errors.push('the library batch export select is missing csv/txt/json, has: ' + optValues.join(','));
        }
        window.__EXPORT_CALL__ = null;
        fmtSel.value = 'txt';
        fmtSel.dispatchEvent(new Event('change', { bubbles: true }));
        expBtn.click();
        await sleep(200);
        if (!window.__EXPORT_CALL__ || window.__EXPORT_CALL__.format !== 'txt') {
          out.errors.push('the library batch export did not call export_prompts with format "txt"');
        }
        if (!Array.isArray(window.__EXPORT_CALL__.ids) || !window.__EXPORT_CALL__.ids.length) {
          out.errors.push('the library batch export did not pass the selected ids');
        }
      }
      // 422: batch tag add/remove, still on the same "select all" from above.
      {
        const addBtn = document.getElementById('lib-batch-tag-add');
        const removeBtn = document.getElementById('lib-batch-tag-remove');
        if (!addBtn || addBtn.classList.contains('hidden') || !removeBtn || removeBtn.classList.contains('hidden')) {
          out.errors.push('422: the batch tag add/remove buttons are missing or hidden with tags enabled');
        } else {
          const selectedIds = [...document.querySelectorAll('.lib-item.lib-selected')].map((el) => el.dataset.id);
          addBtn.click();
          await sleep(150);
          document.getElementById('confirm-input').value = 'Smoke422';
          document.getElementById('confirm-ok').click();
          await sleep(150);
          const rows = [...document.querySelectorAll('.lib-item')].filter((el) => selectedIds.includes(el.dataset.id));
          if (!rows.length || !rows.every((el) => el.textContent.includes('Smoke422'))) {
            out.errors.push('422: adding a tag to the selection did not show it on every selected row');
          }
          // 434: undo the batch tag-add — the toast's Undo button inverts it
          // through the same batch_prompts command, tag-remove this time.
          const addUndoToast = document.getElementById('toast');
          const addUndoBtn = addUndoToast.querySelector('.toast-btn');
          if (!addUndoToast.classList.contains('actionable') || !addUndoBtn) {
            out.errors.push('434: a batch tag-add did not offer an undo toast');
          } else {
            addUndoBtn.click();
            await sleep(200);
            const rowsAfterUndo = [...document.querySelectorAll('.lib-item')].filter((el) => selectedIds.includes(el.dataset.id));
            if (rowsAfterUndo.some((el) => el.textContent.includes('Smoke422'))) {
              out.errors.push('434: undoing a batch tag-add left the tag showing on a row');
            }
          }
          // Add it back so the tag-remove half below has something to remove.
          addBtn.click();
          await sleep(150);
          document.getElementById('confirm-input').value = 'Smoke422';
          document.getElementById('confirm-ok').click();
          await sleep(150);
          removeBtn.click();
          await sleep(150);
          document.getElementById('confirm-input').value = 'Smoke422';
          document.getElementById('confirm-ok').click();
          await sleep(150);
          const rows2 = [...document.querySelectorAll('.lib-item')].filter((el) => selectedIds.includes(el.dataset.id));
          if (rows2.some((el) => el.textContent.includes('Smoke422'))) {
            out.errors.push('422: removing the tag left it showing on a row');
          }
          // 434: undo the batch tag-remove — inverted back to tag-add, then
          // removed again for real (no undo) so the fixture ends up clean.
          const rmUndoToast = document.getElementById('toast');
          const rmUndoBtn = rmUndoToast.querySelector('.toast-btn');
          if (!rmUndoToast.classList.contains('actionable') || !rmUndoBtn) {
            out.errors.push('434: a batch tag-remove did not offer an undo toast');
          } else {
            rmUndoBtn.click();
            await sleep(200);
            const rowsRestored = [...document.querySelectorAll('.lib-item')].filter((el) => selectedIds.includes(el.dataset.id));
            if (!rowsRestored.length || !rowsRestored.every((el) => el.textContent.includes('Smoke422'))) {
              out.errors.push('434: undoing a batch tag-remove did not bring the tag back');
            }
            removeBtn.click();
            await sleep(150);
            document.getElementById('confirm-input').value = 'Smoke422';
            document.getElementById('confirm-ok').click();
            await sleep(150);
            const rowsFinal = [...document.querySelectorAll('.lib-item')].filter((el) => selectedIds.includes(el.dataset.id));
            if (rowsFinal.some((el) => el.textContent.includes('Smoke422'))) {
              out.errors.push('434: the fixture still carries "Smoke422" after the batch tag undo checks');
            }
          }
        }
      }
      // 434: batch delete offers an Undo button too — restore_prompt per id
      // through the same trash-restore path a single trash row uses. Narrowed
      // to one fixture prompt (p8, otherwise unreferenced past this point) so
      // this does not disturb the rest of the fixture library.
      {
        document.getElementById('lib-batch-none').click();
        await sleep(100);
        const p8Row = document.querySelector('.lib-item[data-id="p8"]');
        if (!p8Row) {
          out.errors.push('434: fixture prompt p8 is missing for the batch-delete-undo check');
        } else {
          p8Row.click();
          await sleep(100);
          const delBatchBtn = document.getElementById('lib-batch-delete');
          delBatchBtn.click(); await sleep(50);
          delBatchBtn.click(); await sleep(250);
          if (document.querySelector('.lib-item[data-id="p8"]')) {
            out.errors.push('434: batch delete did not remove p8 from the library list');
          }
          const delUndoToast = document.getElementById('toast');
          const delUndoBtn = delUndoToast.querySelector('.toast-btn');
          if (!delUndoToast.classList.contains('actionable') || !delUndoBtn) {
            out.errors.push('434: a batch delete did not offer an undo toast');
          } else {
            window.__SMOKE_ARGS__ = [];
            delUndoBtn.click();
            await sleep(300);
            if (!window.__SMOKE_ARGS__.some(([cmd, args]) => cmd === 'restore_prompts_batch' && args && Array.isArray(args.placements) && args.placements.some((pl) => pl.id === 'p8'))) {
              out.errors.push('434: undoing a batch delete did not restore p8 with its placement snapshot');
            }
            if (!document.querySelector('.lib-item[data-id="p8"]')) {
              out.errors.push('434: undoing a batch delete did not bring p8 back to the library list');
            }
          }
        }
      }
      document.getElementById('lib-batch-none').click();
      await click('library-select-btn'); // back out of select mode
    }
    await click('library-close');
    await click('library-btn');
    await click('library-tab-history');
    layoutScan('copy history');
    mustReach('copy history', '#library .modal-head-actions > button');
    // P55: an edit icon on each history row opens the editor without copying.
    {
      const jRow = document.querySelector('#journal-body .journal-item');
      const jEdit = jRow && jRow.querySelector('.lib-edit');
      if (!jEdit) {
        out.errors.push('a history row has no edit icon');
      } else {
        window.__SMOKE_CALLS__ = [];
        jEdit.click();
        await sleep(250);
        const modalEl = document.getElementById('modal');
        if (!modalEl || modalEl.classList.contains('hidden')) {
          out.errors.push('the edit icon in the history did not open the editor');
        } else if (window.__SMOKE_CALLS__.includes('copy_text') || window.__SMOKE_CALLS__.includes('copy_prompt')) {
          out.errors.push('clicking the history edit icon also copied the prompt');
        }
        document.getElementById('modal-close-x').click();
        await sleep(200);
      }
    }
    // 59: a delete icon on each "recently copied" row removes that one entry —
    // not every copy of that prompt.
    {
      const jRow = document.querySelector('#journal-body .journal-item');
      const jDel = jRow && jRow.querySelector('.journal-delete');
      if (!jDel) {
        out.errors.push('a history row has no delete icon');
      } else {
        window.__SMOKE_ARGS__ = [];
        jDel.click();
        await sleep(150);
        const cm = document.getElementById('confirm-modal');
        if (!cm || cm.classList.contains('hidden')) {
          out.errors.push('deleting a history entry did not ask for confirmation');
        } else {
          document.getElementById('confirm-ok').click();
          await sleep(250);
          const delCall = window.__SMOKE_ARGS__.find(([cmd]) => cmd === 'delete_copy_entry');
          if (!delCall || typeof delCall[1].rowid !== 'number') {
            out.errors.push('deleting a history entry did not call delete_copy_entry with a rowid');
          }
          if (!window.__SMOKE_ARGS__.some(([cmd]) => cmd === 'recent_copies')) {
            out.errors.push('deleting a history entry did not refetch the journal');
          }
        }
      }
    }
    // 434: clearing the whole copy history offers an Undo button, and clicking
    // it replays the row-level log (through restore_copy_history) plus the two
    // counter maps -- not just an empty confirmation.
    {
      const clearBtn = document.getElementById('journal-clear');
      if (!clearBtn) {
        out.errors.push('434: the journal has no clear-history button');
      } else {
        window.__SMOKE_ARGS__ = [];
        clearBtn.click();
        await sleep(150);
        const cm = document.getElementById('confirm-modal');
        if (cm && !cm.classList.contains('hidden')) {
          document.getElementById('confirm-ok').click();
          await sleep(200);
        }
        if (!window.__SMOKE_ARGS__.some(([cmd]) => cmd === 'clear_copy_history')) {
          out.errors.push('434: clearing the history did not call clear_copy_history');
        }
        const histUndoToast = document.getElementById('toast');
        const histUndoBtn = histUndoToast.querySelector('.toast-btn');
        if (!histUndoToast.classList.contains('actionable') || !histUndoBtn) {
          out.errors.push('434: clearing the copy history did not offer an undo toast');
        } else {
          window.__SMOKE_ARGS__ = [];
          histUndoBtn.click();
          await sleep(250);
          const restoreCall = window.__SMOKE_ARGS__.find(([cmd]) => cmd === 'restore_copy_history');
          if (!restoreCall || !Array.isArray(restoreCall[1].entries) || !restoreCall[1].entries.length) {
            out.errors.push('434: undoing a cleared history did not replay any rows');
          }
          if (histUndoToast.classList.contains('show')) {
            out.errors.push('434: the history-undo toast is still showing after being clicked');
          }
        }
      }
    }
    // Copies from other apps are part of the copy history: source chips, a list of
    // exactly the stored entries under the Elsewhere filter, a per-entry dismiss and a
    // clear that follows the filter (only the stored entries, with its own Undo).
    {
      const srcBar = document.getElementById('journal-source');
      const srcBtn = (k) => srcBar && srcBar.querySelector('[data-source="' + k + '"]');
      const clipRows = () => [...document.querySelectorAll('#journal-body .clip-row')];
      if (!srcBar || srcBar.classList.contains('hidden')) {
        out.errors.push('434: the source chips are hidden although copies from other apps are stored');
      } else {
        if (document.querySelectorAll('#journal-body .journal-item').length === 0 || clipRows().length !== 2) {
          out.errors.push('434: the unfiltered history does not mix app copies and the two stored entries (' +
            document.querySelectorAll('#journal-body .journal-item').length + ' app rows, ' + clipRows().length + ' elsewhere rows)');
        }
        srcBtn('elsewhere').click();
        await sleep(150);
        if (clipRows().length !== 2) out.errors.push('434: the Elsewhere filter lists ' + clipRows().length + ' rows instead of 2');
        if (document.querySelector('#journal-body .journal-item')) out.errors.push('434: the Elsewhere filter still lists copies made in the app');
        // Dismiss removes one entry.
        window.__SMOKE_ARGS__ = [];
        const dismissBtn = clipRows()[0]?.querySelector('.clip-actions button:last-child');
        if (!dismissBtn) {
          out.errors.push('434: an entry copied elsewhere has no dismiss button');
        } else {
          dismissBtn.click();
          await sleep(250);
          if (!window.__SMOKE_ARGS__.some(([cmd]) => cmd === 'clip_inbox_dismiss')) out.errors.push('434: dismissing did not call clip_inbox_dismiss');
          if (clipRows().length !== 1) out.errors.push('434: dismissing left ' + clipRows().length + ' rows instead of 1');
        }
        // Clearing under the Elsewhere filter touches only the stored entries and offers
        // an Undo that replays them through restore_clip_inbox.
        window.__SMOKE_ARGS__ = [];
        document.getElementById('journal-clear').click();
        await sleep(150);
        const cmClip = document.getElementById('confirm-modal');
        if (cmClip && !cmClip.classList.contains('hidden')) {
          document.getElementById('confirm-ok').click();
          await sleep(200);
        }
        if (!window.__SMOKE_ARGS__.some(([cmd]) => cmd === 'clip_inbox_clear')) out.errors.push('434: clearing under Elsewhere did not call clip_inbox_clear');
        if (window.__SMOKE_ARGS__.some(([cmd]) => cmd === 'clear_copy_history')) out.errors.push('434: clearing under Elsewhere also cleared the copy log');
        const clipUndoToast = document.getElementById('toast');
        const clipUndoBtn = clipUndoToast.querySelector('.toast-btn');
        if (!clipUndoToast.classList.contains('actionable') || !clipUndoBtn) {
          out.errors.push('434: clearing the copies from other apps did not offer an undo toast');
        } else {
          window.__SMOKE_ARGS__ = [];
          clipUndoBtn.click();
          await sleep(250);
          if (!window.__SMOKE_ARGS__.some(([cmd]) => cmd === 'restore_clip_inbox')) {
            out.errors.push('434: undoing the clear did not call restore_clip_inbox');
          }
        }
        // The app filter hides the stored entries again; All brings them back.
        srcBtn('app').click();
        await sleep(150);
        if (clipRows().length) out.errors.push('434: the app filter still lists entries copied elsewhere');
        srcBtn('all').click();
        await sleep(150);
      }
      window.__resetClipInbox();
      await window.__TAURI__.core.invoke('clip_inbox_list');
    }
    // 105: the copy-history tag filter -- same picking logic as the library's own
    // tag bar (tagMatches/tagFilterMode), narrows both journal columns.
    {
      const tagBar = document.getElementById('journal-tags');
      if (!tagBar || tagBar.classList.contains('hidden')) {
        out.errors.push('105: the journal tag filter bar is missing or hidden');
      } else {
        const findChip = () => [...document.querySelectorAll('#journal-tags .tag-chip')].find((c) => c.textContent.trim() === 'Work');
        const countRecent = () => document.querySelectorAll('#journal-body .journal-col-recent .journal-item').length;
        if (!findChip()) {
          out.errors.push('105: the journal tag filter has no Work chip');
        } else {
          const before = countRecent();
          findChip().click();
          await sleep(150);
          const activeChip = findChip();
          if (!activeChip || !activeChip.classList.contains('active')) {
            out.errors.push('105: clicking a journal tag chip did not mark it active');
          }
          const after = countRecent();
          if (after >= before) {
            out.errors.push('105: filtering the journal by tag Work did not narrow the list (' + before + ' vs ' + after + ')');
          }
          findChip().click(); // deselect again
          await sleep(150);
          const clearedChip = findChip();
          if (clearedChip && clearedChip.classList.contains('active')) {
            out.errors.push('105: clicking the journal tag chip again did not deselect it');
          }
          if (countRecent() !== before) {
            out.errors.push('105: clearing the journal tag filter did not restore the full list');
          }
        }
      }
    }
    // 198: the journal header now matches the library's -- a count in the head,
    // and the tag row sits BELOW the filter row instead of squeezed into it.
    {
      const jCount = document.getElementById('journal-count');
      if (!jCount || !jCount.textContent.trim()) {
        out.errors.push('198: the journal header is missing its entry count');
      }
      const filtersBox = document.querySelector('#journal-search .journal-filters').getBoundingClientRect();
      const tagsBox = document.getElementById('journal-tags').getBoundingClientRect();
      if (tagsBox.top < filtersBox.bottom) {
        out.errors.push('198: the journal tag row overlaps the filter row instead of sitting below it');
      }
    }
    // the library reopens on the tab used last, and what follows expects the prompt list
    await click('library-tab-prompts');
    await click('library-close');
    // Search everything: the toolbar icon and Ctrl+K both open the overlay, all
    // six source groups can each turn up a hit, arrow keys move the highlight
    // across group boundaries, and Enter runs the right action per source.
    {
      const gsInput = document.getElementById('gs-input');
      const gsResults = document.getElementById('gs-results');
      const groupLabelBefore = (row) => {
        let el = row.previousElementSibling;
        while (el && !el.classList.contains('gs-group-head')) el = el.previousElementSibling;
        return el ? el.textContent : '';
      };
      // The group-head carries a stable data-group marker (see gsGroupHead in
      // main.js) -- a query can hit more than one group at once (e.g. a file
      // path substring also matching the prompt body scorer), so picking a
      // result by group identity rather than by "whatever is highlighted
      // first" is what actually exercises each source's own action.
      const firstRowOfGroup = (key) => {
        const head = gsResults.querySelector(`.gs-group-head[data-group="${key}"]`);
        return head ? head.nextElementSibling : null;
      };
      // Ctrl+K always works, at every window size; the icon itself drops below
      // 760px (see main.css) since the header has no spare width left there --
      // that specific trade-off is checked once below, separately from the
      // reopens the rest of this block needs.
      const openGS = () => document.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }));

      if (window.innerWidth >= 760) {
        document.getElementById('global-search-btn').click();
        await sleep(150);
        if (document.getElementById('global-search').classList.contains('hidden')) {
          out.errors.push('the search-everything icon did not open the overlay');
        }
        layoutScan('search everything');
        gsInput.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await sleep(100);
        if (!document.getElementById('global-search').classList.contains('hidden')) {
          out.errors.push('Escape did not close the search-everything overlay');
        }
      }
      // Opens via Ctrl+K.
      openGS();
      await sleep(150);
      if (document.getElementById('global-search').classList.contains('hidden')) {
        out.errors.push('Ctrl+K did not open the search-everything overlay');
      }
      layoutScan('search everything');

      // A query hitting a name shared by a prompt and its own copy-history entry
      // (tauri_stub.js logs the first four prompts as recently copied).
      gsInput.value = 'Prompt 1';
      gsInput.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(200);
      const rowsAfterPrompt1 = [...gsResults.querySelectorAll('.gs-row')];
      if (!rowsAfterPrompt1.length) out.errors.push('searching a prompt name in search-everything found nothing');
      const groupHeads1 = [...gsResults.querySelectorAll('.gs-group-head')].map((h) => h.textContent);
      if (!groupHeads1.length) out.errors.push('search-everything results show no group headings');
      // Arrow keys must cross from one group into the next, not just within one.
      if (rowsAfterPrompt1.length > 1) {
        const firstGroup = groupLabelBefore(rowsAfterPrompt1[0]);
        let crossed = false;
        for (let i = 0; i < rowsAfterPrompt1.length; i++) {
          gsInput.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
          const active = gsResults.querySelector('.gs-row.active');
          if (active && groupLabelBefore(active) !== firstGroup) { crossed = true; break; }
        }
        if (!crossed) out.errors.push('arrow keys in search-everything never crossed a group boundary');
      }
      // Enter on the highlighted (first) prompt result copies it and closes the overlay.
      window.__SMOKE_CALLS__ = [];
      gsInput.value = 'Prompt 1';
      gsInput.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(200);
      gsInput.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      await sleep(150);
      if (!document.getElementById('global-search').classList.contains('hidden')) {
        out.errors.push('Enter in search-everything did not close the overlay');
      }
      if (!window.__SMOKE_CALLS__.some((c) => c === 'copy_prompt' || c === 'copy_text')) {
        out.errors.push('Enter on a search-everything prompt result did not copy it');
      }

      // Attachment group: a query matching the seeded file_path (index 9/11).
      openGS();
      await sleep(150);
      gsInput.value = 'brief';
      gsInput.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(200);
      const attachRow = firstRowOfGroup('attachments');
      if (!attachRow) out.errors.push('searching an attachment file name in search-everything found nothing in the attachments group');
      else {
        window.__SMOKE_CALLS__ = [];
        attachRow.click();
        await sleep(150);
        if (!document.getElementById('modal') || document.getElementById('modal').classList.contains('hidden')) {
          out.errors.push('picking a search-everything attachment result did not open the edit dialog');
        }
        document.getElementById('modal-close-x')?.click();
        await sleep(150);
      }

      // Views group: a query matching the seeded "Work" view.
      openGS();
      await sleep(150);
      gsInput.value = 'Work';
      gsInput.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(200);
      const viewRow = firstRowOfGroup('views');
      if (!viewRow) out.errors.push('searching a view name in search-everything found nothing in the views group');
      else {
        window.__SMOKE_ARGS__ = [];
        viewRow.click();
        await sleep(200);
        if (!window.__SMOKE_ARGS__.some(([cmd, a]) => cmd === 'set_active_view' && a.id === 'work')) {
          out.errors.push('picking a search-everything view result did not switch to it');
        }
      }
      // Back to the Home view so every later grid-based check in this probe
      // still finds its fixture prompts.
      document.querySelector('.view-btn[data-view-id="home"]')?.click();
      await sleep(150);

      // Settings group: a query matching the theme setting outside the expert menu.
      openGS();
      await sleep(150);
      gsInput.value = 'theme';
      gsInput.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(200);
      const settingsRow = firstRowOfGroup('settings');
      if (!settingsRow) out.errors.push('searching a settings label in search-everything found nothing in the settings group');
      else {
        settingsRow.click();
        await sleep(200);
        if (document.getElementById('settings').classList.contains('hidden')) {
          out.errors.push('picking a search-everything settings result did not open settings');
        }
        if (!document.querySelector('.settings-search-hit')) {
          out.errors.push('picking a search-everything settings result did not flash the matching row');
        }
        document.getElementById('settings').classList.add('hidden');
      }

      // Help group: a query matching a help article that exists in English.
      openGS();
      await sleep(150);
      gsInput.value = 'tile';
      gsInput.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(200);
      const helpRow = firstRowOfGroup('help');
      if (!helpRow) out.errors.push('searching a help topic in search-everything found nothing in the help group');
      else {
        helpRow.click();
        await sleep(200);
        if (document.getElementById('help').classList.contains('hidden')) {
          out.errors.push('picking a search-everything help result did not open the help dialog');
        }
        document.getElementById('help-close').click();
        await sleep(150);
      }

      // Clicking the backdrop closes it, same as every other dialog here.
      openGS();
      await sleep(150);
      document.getElementById('global-search').dispatchEvent(new Event('pointerdown', { bubbles: true }));
      await sleep(150);
      if (!document.getElementById('global-search').classList.contains('hidden')) {
        out.errors.push('clicking outside the search-everything card did not close it');
      }
    }
    // 528: the search-everything head is a titlebar row (title left, close
    // right) above a full-width input, and each recent term can be deleted
    // on its own or all at once, keyboard reachable, focus surviving a delete.
    {
      const openGS528 = () => document.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }));
      openGS528();
      await sleep(150);
      const gsInput528 = document.getElementById('gs-input');
      const head = document.querySelector('#global-search .gs-head');
      const titlebar = head.querySelector('.modal-titlebar');
      const title = titlebar.querySelector('.modal-title');
      const closeBtn = document.getElementById('gs-close');
      const titleR = title.getBoundingClientRect();
      const closeR = closeBtn.getBoundingClientRect();
      const titlebarR = titlebar.getBoundingClientRect();
      if (!(titleR.left < closeR.left)) {
        out.errors.push('528: search-everything title is not left of the close button');
      }
      // Both sit in the titlebar row, so both must fit inside its own bounds
      // vertically -- a raw top-to-top compare is thrown off by the title text
      // and the button icon having different natural heights.
      if (closeR.top < titlebarR.top - 1 || closeR.bottom > titlebarR.bottom + 1 ||
          titleR.top < titlebarR.top - 1 || titleR.bottom > titlebarR.bottom + 1) {
        out.errors.push('528: search-everything title and close button are not on the same row');
      }
      const inputR = gsInput528.getBoundingClientRect();
      if (Math.abs(inputR.width - titlebarR.width) > 1) {
        out.errors.push('528: search-everything input width does not match the head content width');
      }
      if (inputR.top < titlebarR.bottom - 1) {
        out.errors.push('528: search-everything input does not sit below the title row');
      }
      document.getElementById('gs-close').click();
      await sleep(100);

      // Build a real recent list through the same path a person would (type a
      // term, press Enter): the stub backend has no memory of its own, so
      // there is nothing to seed directly from outside the module.
      const runTerm = async (term) => {
        openGS528();
        await sleep(150);
        const inp = document.getElementById('gs-input');
        inp.value = term;
        inp.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(200);
        inp.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
        await sleep(150);
        // A prompt with placeholders leaves its window up: close it, or it would
        // swallow the next copy of any prompt.
        if (window.__VAR_PAYLOAD__) await varCancel();
      };
      await runTerm('Prompt 1');
      await runTerm('Prompt 2');
      await runTerm('Prompt 3');

      openGS528();
      await sleep(150);
      // Earlier steps in this same probe already remembered a term or two of
      // their own, so the list can hold more than the three just added here --
      // what matters is that the three newest are on top, in reverse order.
      let recentRows = [...document.querySelectorAll('#gs-results .gs-recent-row')];
      const recentNames = recentRows.map((r) => r.querySelector('.gs-row-name').textContent);
      if (recentNames.slice(0, 3).join(',') !== 'Prompt 3,Prompt 2,Prompt 1') {
        out.errors.push('528: the recent list top three are not the newest terms in order, got ' + recentNames.join(','));
      }
      const clearAllBtn = document.querySelector('#gs-results .gs-clear-all');
      if (!clearAllBtn) out.errors.push('528: recent list has no clear-all button');

      // Tab reachability: the delete button is a real <button>, so it takes
      // focus in normal tab order without a special tabindex trick.
      const firstDel = recentRows[0] && recentRows[0].querySelector('.gs-recent-del');
      if (!firstDel) {
        out.errors.push('528: a recent row has no delete button');
      } else {
        firstDel.focus();
        if (document.activeElement !== firstDel) {
          out.errors.push('528: the recent-term delete button cannot receive keyboard focus');
        }
        // Deleting one term shrinks the list by one and keeps focus on a
        // delete button, so several terms can be cleared with Tab + Enter.
        const beforeCount = recentRows.length;
        firstDel.click();
        await sleep(100);
        let list528;
        try { list528 = JSON.parse(SETTINGS.ui_texts.globalSearchRecent || '[]'); } catch (e) { list528 = null; }
        if (!Array.isArray(list528) || list528.length !== beforeCount - 1) {
          out.errors.push('528: deleting one recent term did not shrink globalSearchRecent by one');
        }
        recentRows = [...document.querySelectorAll('#gs-results .gs-recent-row')];
        if (recentRows.length !== beforeCount - 1) {
          out.errors.push('528: deleting one recent term did not update the DOM');
        }
        if (!document.activeElement || !document.activeElement.classList.contains('gs-recent-del')) {
          out.errors.push('528: deleting a recent term did not keep focus on a delete button');
        }
      }

      // Clear all empties both the stub setting and the DOM.
      const clearBtn = document.querySelector('#gs-results .gs-clear-all');
      if (clearBtn) clearBtn.click();
      await sleep(100);
      let list528b;
      try { list528b = JSON.parse(SETTINGS.ui_texts.globalSearchRecent || '[]'); } catch (e) { list528b = null; }
      if (!Array.isArray(list528b) || list528b.length !== 0) {
        out.errors.push('528: clear all did not empty globalSearchRecent in the stub settings');
      }
      if (document.querySelectorAll('#gs-results .gs-recent-row').length !== 0) {
        out.errors.push('528: clear all did not empty the recent list in the DOM');
      }
      layoutScan('search everything recent list');
      document.getElementById('gs-close').click();
      await sleep(100);
    }
    // 439: recent-messages window -- reachable from the expert menu, shows an
    // error toast's own text, "copy all" puts it on the clipboard, "clear"
    // empties the list and the empty state says so.
    {
      window.__SMOKE_CALLS__ = [];
      window.__SMOKE_ARGS__ = [];
      const probeMsg = 'smoke-test probe message 439';
      // errToast itself is module-scoped and unreachable from here; invoke the
      // same command it calls (log_error) so the window/copy/clear probes below
      // exercise the real path an error toast feeds.
      await window.__TAURI__.core.invoke('log_error', { text: probeMsg, origin: 'smoke-test', raw: null });
      await sleep(150);
      document.getElementById('settings').classList.remove('hidden');
      await sleep(50);
      const findErrorLogBtn = () => document.querySelector('.expert-action[data-action="error-log-open"]');
      await click('expert-open');
      let errorLogBtn = findErrorLogBtn();
      for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
        if (errorLogBtn) break;
        tb.click();
        await sleep(150);
        errorLogBtn = findErrorLogBtn();
      }
      if (!errorLogBtn) {
        out.errors.push('439: error-log-open expert action not found in any tab');
      } else {
        errorLogBtn.click();
        await sleep(150);
        const modalEl = document.getElementById('error-log-modal');
        if (!modalEl || modalEl.classList.contains('hidden')) {
          out.errors.push('439: recent messages did not open from the expert-menu action');
        } else {
          mustReach('recent messages', '#error-log-modal .modal-head-actions > button');
          const body = document.getElementById('error-log-body');
          if (!body || !body.textContent.includes(probeMsg)) {
            out.errors.push('439: recent messages does not show the error just shown in a toast');
          }
          document.getElementById('error-log-copy').click();
          await sleep(150);
          const copyCall = window.__SMOKE_ARGS__.find(([cmd]) => cmd === 'copy_text');
          if (!copyCall || !copyCall[1].text.includes(probeMsg)) {
            out.errors.push('439: copy all did not put the shown message on the clipboard');
          }
          document.getElementById('error-log-clear').click();
          await sleep(150);
          if (!window.__SMOKE_CALLS__.includes('clear_error_log')) {
            out.errors.push('439: clear did not call clear_error_log');
          }
          if (!body.textContent.trim()) {
            out.errors.push('439: recent messages shows no text once cleared');
          } else if (body.textContent.includes(probeMsg)) {
            out.errors.push('439: the cleared message is still shown');
          }
        }
        document.getElementById('error-log-close').click();
        await sleep(100);
      }
      document.getElementById('settings').classList.add('hidden');
      await sleep(50);
    }
    const tile = document.querySelector('#grid .tile');
    if (tile) { tile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true })); await sleep(150); }
    document.body.click(); // dismiss the context menu
    await click('fav-view-btn');
    await click('fav-view-btn');
    // 199: a per-tile chain-start button next to the selection checkbox.
    {
      const textTile = [...document.querySelectorAll('#grid .tile')].find(
        (el) => el.querySelector('.tile-chain-start')
      );
      const imgTile = document.querySelector('#grid .tile[data-id="p9"]'); // fixture: file_path set, not a text prompt
      if (!textTile) {
        out.errors.push('199: no tile shows the chain-start button');
      } else if (imgTile && imgTile.querySelector('.tile-chain-start')) {
        out.errors.push('199: an image tile shows a chain-start button');
      } else {
        const csBtn = textTile.querySelector('.tile-chain-start');
        const csSvg = csBtn.querySelector('svg');
        const btnRect = csBtn.getBoundingClientRect();
        const svgRect = csSvg.getBoundingClientRect();
        const btnCx = btnRect.left + btnRect.width / 2, btnCy = btnRect.top + btnRect.height / 2;
        const svgCx = svgRect.left + svgRect.width / 2, svgCy = svgRect.top + svgRect.height / 2;
        if (svgRect.width < 15 || svgRect.height < 15) {
          out.errors.push('199: the chain-start glyph does not fill its tile button');
        }
        if (Math.abs(svgCx - btnCx) > 0.5 || Math.abs(svgCy - btnCy) > 0.5) {
          out.errors.push('199: the chain-start glyph is not centred in its tile button');
        }
        csBtn.click();
        await sleep(150);
        if (!document.body.classList.contains('chain-mode')) {
          out.errors.push('199: clicking a tile chain-start button did not enter chain mode');
        }
        if (!textTile.classList.contains('chain-picked')) {
          out.errors.push('199: the starting tile was not marked picked');
        }
        const badge = textTile.querySelector('.tile-chain-badge');
        if (!badge || badge.textContent.trim() !== '1') {
          out.errors.push('199: the starting tile has no "1" chain badge');
        }
        const otherStart = [...document.querySelectorAll('.tile-chain-start')]
          .find((el) => getComputedStyle(el).pointerEvents !== 'none');
        if (otherStart) {
          out.errors.push('199: a chain-start button stayed clickable while a chain is running');
        }
        // chainLock ships on, so a single click latches rather than exits (off ->
        // on -> latched -> off) -- cycle until it is actually gone, not just once.
        for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
        await sleep(150);
      }
    }
    // 275: every badge a tile can show AT ONCE must not visually overlap --
    // select checkbox, chain-start (or chain-badge once picked), rotate badge
    // and the actions menu. Not favorite/hotkey -- neither renders as a tile
    // badge in this app (only in the library row and the editor), so there
    // is nothing there that could ever overlap. Select/chain-start/the menu
    // button only reveal on :hover, which page script cannot trigger in a
    // real browser (CSS :hover tracks actual pointer state, not synthetic
    // events) -- forcing their opacity measures the exact geometry a real
    // hover would produce, without pretending hover itself was tested.
    {
      // Belt and braces: a probe upstream is only ever supposed to leave chain
      // mode off, but a state-leak there must not silently invalidate this one.
      for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
      const rotTile = document.querySelector('#grid .tile[data-id="p10"]'); // fixture: has variants + no file/copy_image
      const checkOverlap = (label, selectors) => {
        const els = selectors.map((sel) => rotTile.querySelector(sel)).filter(Boolean);
        const restore = els.map((el) => [el, el.style.opacity]);
        els.forEach((el) => { el.style.opacity = "1"; });
        for (let i = 0; i < els.length; i++) {
          for (let j = i + 1; j < els.length; j++) {
            const a = els[i].getBoundingClientRect();
            const b = els[j].getBoundingClientRect();
            const ix = Math.max(0, Math.min(a.right, b.right) - Math.max(a.left, b.left));
            const iy = Math.max(0, Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top));
            if (ix * iy > 4) {
              out.errors.push('275: ' + label + ' badges overlap at ' + window.innerWidth + 'px: ' +
                els[i].className + ' / ' + els[j].className);
            }
          }
        }
        restore.forEach(([el, val]) => { el.style.opacity = val; });
      };
      if (!rotTile) {
        out.errors.push('275: fixture tile p10 (rotating, chain-capable) missing from the grid');
      } else {
        checkOverlap('idle', ['.tile-select', '.tile-chain-start', '.tile-rotate-badge', '.tile-menu']);
        const startBtn = rotTile.querySelector('.tile-chain-start');
        if (startBtn) {
          startBtn.click();
          await sleep(150);
          checkOverlap('picked mid-chain', ['.tile-chain-badge', '.tile-rotate-badge', '.tile-menu']);
          // 491: the corner belongs to the order badge while a chain runs -- the
          // menu button and the select box stay hidden on hover too, on both the
          // picked tile and any other one, unless multi-select is on.
          const otherTile = [...document.querySelectorAll('#grid .tile')].find((el) => el !== rotTile);
          for (const [label, tile] of [['picked', rotTile], ['other', otherTile]]) {
            if (!tile) continue;
            for (const name of ['tile-menu', 'tile-select']) {
              const el = tile.querySelector('.' + name);
              if (!el) continue;
              const cs = getComputedStyle(el);
              if (cs.pointerEvents !== 'none' || cs.opacity !== '0') {
                out.errors.push('491: the ' + name + ' stayed reachable on the ' + label + ' tile during a chain');
              }
            }
          }
          for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
          await sleep(100);
          const afterMenu = rotTile.querySelector('.tile-menu');
          if (afterMenu && getComputedStyle(afterMenu).pointerEvents !== 'auto') {
            out.errors.push('491: the tile menu stayed unreachable after the chain ended');
          }
        }
      }
    }
    // 141: get_state carries has_image + image_rev, never the bytes. A tile fetches
    // the picture it needs through prompt_image and then keeps it, so the many
    // re-renders the walk above already caused must have cost exactly ONE call.
    {
      const state = await window.__TAURI__.core.invoke('get_state');
      const leaked = (state.prompts || []).filter(p => 'image' in p).map(p => p.id);
      if (leaked.length) {
        out.errors.push('get_state ships image bytes again for ' + leaked.join(', '));
      }
      const withImg = (state.prompts || []).find(p => p.has_image);
      if (!withImg) {
        out.errors.push('no fixture prompt has has_image, so the image path went untested');
      } else {
        const shot = () => {
          const el = [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === withImg.id);
          return el && el.querySelector('img.tile-img');
        };
        for (let i = 0; i < 40 && !shot(); i++) await sleep(100);
        const img = shot();
        if (!img) {
          out.errors.push('the image tile never got its picture from prompt_image');
        } else if (img.getAttribute('src') !== window.__STUB_IMAGE__) {
          out.errors.push('the image tile is showing something other than the fetched image');
        }
        const calls = window.__PROMPT_IMAGE_CALLS__;
        if (calls !== 1) {
          out.errors.push('prompt_image ran ' + calls + ' times for one image, expected 1');
        }
        // Draw the same tiles once more: the second render has to come out of the
        // cache, not off another round trip.
        await click('library-btn');
        await click('library-close');
        if (window.__PROMPT_IMAGE_CALLS__ !== calls) {
          out.errors.push('re-rendering an image prompt fetched it again');
        }
        if (!shot()) out.errors.push('the image tile lost its picture on a re-render');
      }
    }
    // F17: the note rides in the tile tooltip as its own dimmed section, and only
    // for a prompt that actually has one.
    const byId = (id) => [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
    const noteTile = byId('p0'), plainTile = byId('p2');
    if (!noteTile || !plainTile) {
      out.errors.push('the tiles for the note check are missing from the grid');
    } else {
      if (!noteTile.dataset.tipNotes) out.errors.push('a prompt with a note carries no tooltip note');
      if (plainTile.dataset.tipNotes) out.errors.push('a prompt without a note carries a tooltip note anyway');
      const noteSection = () => document.querySelector('.tile-tip .tile-tip-notes');
      const hover = async (el, x) => {
        el.dispatchEvent(new PointerEvent('pointerover', { bubbles: true, clientX: x, clientY: 40 }));
        await sleep(700); // longer than the tooltip show-delay
      };
      await hover(noteTile, 30);
      const shown = noteSection();
      if (!shown || !shown.textContent.includes('Moonshot')) {
        out.errors.push('the tooltip shows no note: ' + (shown ? shown.textContent : 'section missing'));
      } else if (getComputedStyle(shown).display === 'none') {
        out.errors.push('the note section stays hidden although a note is set');
      }
      await hover(plainTile, 60);
      const empty = noteSection();
      if (empty && empty.textContent.trim()) {
        out.errors.push('the tooltip shows a note for a prompt that has none');
      }
      // 6/14: the length readout and the {#{variable}#} highlight, on a prompt that
      // actually has placeholders (p1: "Translate {#{Text=Hello}#} into {#{Language}#}.").
      const varHoverTile = byId('p1');
      if (varHoverTile) {
        await hover(varHoverTile, 90);
        const lenText = (document.querySelector('.tile-tip .tile-tip-len') || {}).textContent || '';
        if (!/^[0-9][0-9,.]* chars/.test(lenText) || !/~[0-9][0-9,.]* tokens$/.test(lenText)) {
          out.errors.push('the tile tooltip length readout looks wrong: "' + lenText + '"');
        }
        const varSpans = [...document.querySelectorAll('.tile-tip .tile-tip-body .var-token')];
        const varTexts = varSpans.map((s) => s.textContent);
        if (varTexts.length !== 2 || !varTexts.includes('{#{Text=Hello}#}') || !varTexts.includes('{#{Language}#}')) {
          out.errors.push('the tile tooltip does not highlight both variables: ' + JSON.stringify(varTexts));
        }
      }
      // B1: the rotate line is shared between the rich tile tip and the plain
      // setting/icon tip. It used to survive the switch, so the last hovered
      // tile-s next-entry text stayed glued under every plain tooltip.
      const rotLine = () => document.querySelector('.tile-tip .tile-tip-rotate');
      noteTile.dataset.tipRotate = 'Rotating, next entry: probe';
      await hover(noteTile, 30);
      const withRot = rotLine();
      if (!withRot || !withRot.textContent.includes('probe')) {
        out.errors.push('the tile tooltip does not show the rotate line at all');
      }
      const plainTarget = document.createElement('div');
      plainTarget.dataset.tip = 'A plain setting tooltip';
      plainTarget.style.cssText = 'position:fixed;left:180px;top:30px;width:40px;height:20px';
      document.body.appendChild(plainTarget);
      await hover(plainTarget, 200);
      const plainName = document.querySelector('.tile-tip .tile-tip-name');
      if (!plainName || !plainName.textContent.includes('plain setting')) {
        out.errors.push('the plain tooltip never showed, so the rotate-line check proves nothing');
      } else {
        const leaked = rotLine();
        if (leaked && leaked.textContent.trim()) {
          out.errors.push('a plain tooltip carries the rotate line: "' + leaked.textContent + '"');
        }
      }
      plainTarget.remove();
      delete noteTile.dataset.tipRotate;
      document.body.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
      await sleep(200);
    }
    // 428: last-copied/last-edited lines in the tile tooltip. p1 carries canned
    // timestamps for both (tauri_stub's SETTINGS.last_used and last_edited_map);
    // p2 has neither. main.js is an ES module, so its `settings`/`renderGrid`
    // are not reachable from this classic probe script -- the data has to be
    // baked into the stub's initial state instead of poked in live.
    {
      const copyId = 'p1';
      const t1 = byId(copyId);
      const times1 = (t1 || {}).dataset ? t1.dataset.tipTimes || '' : '';
      if (!times1.includes('Last copied') || !times1.includes('Last edited')) {
        out.errors.push('428: a prompt with both timestamps is missing one of the tooltip lines: "' + times1 + '"');
      }
      const expected = new Date(1700000000 * 1000).toLocaleString('en', { dateStyle: 'short', timeStyle: 'short' });
      if (!times1.includes(expected)) {
        out.errors.push('428: the copy time is not a localized date string: "' + times1 + '"');
      }
      const t2 = byId('p2');
      const times2 = (t2 || {}).dataset ? t2.dataset.tipTimes || '' : '';
      if (times2) out.errors.push('428: a prompt with no history data carries tooltip time lines anyway: "' + times2 + '"');
    }
    // 1: the empty-grid hint stays out of the way once prompts exist.
    if (!document.getElementById('grid-empty').classList.contains('hidden')) {
      out.errors.push('the empty-grid hint shows although the grid has prompts');
    }
    // Copying the prompt with placeholders must open the placeholder window and,
    // once it answers, finish the copy -- and a plain click must NOT paste into the
    // previous app (that is what the double-click / Enter shortcut is for).
    const tiles = [...document.querySelectorAll('#grid .tile')];
    const varTile = tiles.find(el => (el.dataset.id || '') === 'p1');
    if (varTile) {
      window.__VAR_PAYLOAD__ = undefined;
      // The grid listens for pointer events (it tracks drags), so .click() alone
      // never reaches the handler -- dispatch the real sequence.
      for (const type of ['pointerdown', 'pointerup', 'click']) {
        varTile.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1 }));
      }
      await sleep(400);
      const varsReq1 = varReq();
      if (!varsReq1) out.errors.push('clicking a variable prompt did not open the placeholder window');
      // 425: a text default in the prompt itself prefills the field on first
      // ask (Text=Hello); a variable with no default stays empty (Language).
      if (varsReq1) {
        if (varsReq1.rows[0]?.value !== 'Hello') out.errors.push('425: the text default did not prefill the field: ' + varsReq1.rows[0]?.value);
        if (varsReq1.rows[1]?.value) out.errors.push('425: a variable with no default was prefilled anyway: ' + varsReq1.rows[1].value);
        if (document.getElementById('vars-modal') || document.getElementById('vars-fields')) out.errors.push('the main window still carries an in-window placeholder dialog');
        await varAnswer(['Hello', 'Dutch']);
      }
      if ((window.__SMOKE_CALLS__ || []).includes('paste_into_previous')) {
        out.errors.push('a single click on a variable prompt pasted into the previous app');
      }
      // A second press while the window is still up is the second half of a double
      // click: it opens no second question and turns the pending copy into a paste.
      await sleep(2400); // past the per-prompt copy cooldown (2000ms)
      window.__VAR_PAYLOAD__ = undefined;
      (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []).length = 0;
      for (let press = 0; press < 2; press++) {
        for (const type of ['pointerdown', 'pointerup', 'click']) {
          varTile.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1 }));
        }
        await sleep(200);
      }
      const dblOpens = window.__SMOKE_CALLS__.filter(c => c === 'open_var_window').length;
      if (dblOpens !== 1) out.errors.push('a double click on a variable prompt opened the placeholder window ' + dblOpens + ' times');
      await varAnswer(['Hello', 'Dutch']);
      if (!window.__SMOKE_CALLS__.includes('paste_into_previous')) {
        out.errors.push('a double click on a variable prompt did not paste after the window was filled');
      }
    }
    // The chain button latches (off -> on -> infinite), so the two clicks above left
    // it armed; a picked tile would go into the chain instead of being copied.
    for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
    if (document.body.classList.contains('chain-mode')) out.errors.push('chain mode could not be switched off');
    // F12/F13: clicking a tile, but through the same real pointer sequence.
    const clickTile = async (id) => {
      const el = [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
      if (!el) return false;
      for (const type of ['pointerdown', 'pointerup', 'click']) {
        el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1 }));
      }
      await sleep(400);
      return true;
    };
    const varsHidden = () => !varReq();
    // F12: a prompt whose placeholders all resolve themselves must never ask, and
    // the copied text must carry no leftover {#{ }#} fence.
    window.__LAST_COPY__ = undefined;
    window.__VAR_PAYLOAD__ = undefined;
    if (!await clickTile('p3')) {
      out.errors.push('the dynamic-placeholder prompt is missing from the grid');
    } else {
      if (!varsHidden()) { out.errors.push('an automatic placeholder still opened the placeholder window'); await varCancel(); }
      const copied = window.__LAST_COPY__ || '';
      if (/\\{#\\{/.test(copied)) out.errors.push('an automatic placeholder was left unresolved: ' + copied);
      if (!copied.includes('CLIPTEXT')) out.errors.push('the clipboard placeholder did not pick up the clipboard text');
      if (!/Meeting on \\S/.test(copied)) out.errors.push('the date placeholder produced nothing');
      // F12: every calendar/id placeholder has to produce a real value of its own.
      const dynChecks = [
        ['week', /week \\d{1,2} of \\S/],
        ['year', new RegExp(' ' + new Date().getFullYear() + '\\\\)')],
        ['yesterday/tomorrow', /yesterday \\S+, tomorrow \\S+/],
        ['timestamp', /stamp \\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}[+-]\\d{2}:\\d{2}/],
        ['epoch', /epoch 1\\d{9}/],
        ['username', /by tester/],
        ['random', /ref [0-9a-z]{8}/],
        ['counter', /no 1[ :]/],
      ];
      for (const [name, re] of dynChecks) {
        if (!re.test(copied)) out.errors.push('the ' + name + ' placeholder produced nothing usable: ' + copied);
      }
      // A second copy must move the counter on — that is the whole point of it.
      window.__LAST_COPY__ = undefined;
      await sleep(2400); // past the per-prompt copy cooldown (2000ms)
      if (await clickTile('p3') && !/no 2[ :]/.test(window.__LAST_COPY__ || '')) {
        out.errors.push('the counter placeholder did not count up: ' + window.__LAST_COPY__);
      }
    }
    // 445: the chain overview strip -- appears once a chain has a pick, marks a
    // step red when it needs a placeholder nothing earlier in the chain fills,
    // recomputes after a drag reorder, and disappears with its own switch off.
    {
      const findGridTile = (id) => [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
      const findExpertRow = async (label) => {
        const hit = () => [...document.querySelectorAll('#expert-flags .switch-field')]
          .find(r => ((r.querySelector('span') || {}).textContent || '').includes(label));
        if (hit()) return hit();
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          await sleep(200);
          if (hit()) return hit();
        }
        return null;
      };
      for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
      // 510: the grid's own top position with the chain off, to compare against
      // once the chain overview strip has come and gone below.
      const gridTopOff = document.getElementById('grid').getBoundingClientRect().top;
      const p0Tile = findGridTile('p0');
      const startBtn = p0Tile && p0Tile.querySelector('.tile-chain-start');
      if (!startBtn) {
        out.errors.push('445: fixture tile p0 has no chain-start button to begin the chain with');
      } else {
        startBtn.click();
        await sleep(150);
        // p3: dynamic placeholders only -- self-filling, no dialog, no ask badges.
        if (!await clickTile('p3')) out.errors.push('445: could not pick p3 into the chain');
        // p1: two manual placeholders -- opens the placeholder window, nothing fills them yet.
        window.__VAR_PAYLOAD__ = undefined;
        if (!await clickTile('p1')) {
          out.errors.push('445: could not pick p1 into the chain');
        } else {
          const chainReq = varReq();
          await varAnswer((chainReq ? chainReq.rows : []).map(() => 'x'));
        }
        const bar = document.getElementById('chain-overview');
        if (!bar || bar.classList.contains('hidden')) {
          out.errors.push('445: the chain overview strip did not appear for a running chain');
        } else {
          const ovTitle = bar.querySelector('.chain-ov-title');
          if (ovTitle && ovTitle.getBoundingClientRect().top - bar.getBoundingClientRect().top > 3) {
            out.errors.push('445: the chain overview keeps extra space above its title');
          }
          // 510: total gap from the grid area's own top edge (its padding box,
          // not the strip's) down to the title, so the padding the strip adds
          // on top of the grid area's own padding is counted too.
          if (ovTitle) {
            const gridWrapTop = document.querySelector('.grid-wrap').getBoundingClientRect().top;
            const gap510 = ovTitle.getBoundingClientRect().top - gridWrapTop;
            if (gap510 > 4) {
              out.errors.push('510: the gap above the chain overview title is ' + Math.round(gap510) + 'px, over the 4px target');
            }
          }
          const firstTile = document.querySelector('#grid .tile');
          if (firstTile && bar.getBoundingClientRect().bottom > firstTile.getBoundingClientRect().top) {
            out.errors.push('445: the chain overview overlaps the grid below it');
          }
          let rows = [...bar.querySelectorAll('.chain-ov-row')];
          if (rows.length !== 3) {
            out.errors.push('445: the overview shows ' + rows.length + ' rows instead of 3');
          } else {
            const [row0, row1, row2] = rows;
            if (row0.classList.contains('chain-ov-missing') || row1.classList.contains('chain-ov-missing')) {
              out.errors.push('445: a step with no unmet placeholder was marked red');
            }
            if (!row2.classList.contains('chain-ov-missing')) {
              out.errors.push('445: the step needing Text/Language was not marked red');
            }
            if (!row2.querySelector('.chain-ov-missing-note')) {
              out.errors.push('445: the red row names no missing placeholder');
            }
            if (!row1.querySelector('.chain-ov-badge.chain-ov-fills')) {
              out.errors.push('445: the reserved-name step shows no fills badge');
            }
            if (!row2.querySelector('.chain-ov-badge.chain-ov-needs')) {
              out.errors.push('445: the manual-placeholder step shows no needs badge');
            }
            // Drag the red (3rd) row onto the 1st row and drop -- the strip must
            // recompute in place, not leave the red mark stuck at the old position.
            const pt445 = (type, el, x, y) => el.dispatchEvent(new PointerEvent(type, {
              bubbles: true, cancelable: true, pointerId: 5, isPrimary: true, button: 0,
              buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y,
            }));
            const rr2 = row2.getBoundingClientRect();
            const rr0 = row0.getBoundingClientRect();
            const sx = rr2.left + rr2.width / 2, sy = rr2.top + rr2.height / 2;
            const ex = rr0.left + rr0.width * 0.25, ey = rr0.top + rr0.height / 2;
            pt445('pointerdown', row2, sx, sy);
            pt445('pointermove', row2, sx + 14, sy);
            pt445('pointermove', row2, ex, ey);
            pt445('pointerup', row2, ex, ey);
            await sleep(150);
            rows = [...bar.querySelectorAll('.chain-ov-row')];
            if (!rows[0] || !rows[0].classList.contains('chain-ov-missing')) {
              out.errors.push('445: dragging the red row to the front did not recompute the strip');
            }
          }
        }
        // The switch turns the whole strip off, even mid-chain.
        document.getElementById('settings').classList.remove('hidden');
        await click('expert-open');
        const varCheckRow = await findExpertRow('Show what each chain step needs');
        if (!varCheckRow) {
          out.errors.push('445: the chain overview switch is missing from the expert menu');
        } else {
          const sw = varCheckRow.querySelector('input.switch');
          sw.click();
          await sleep(200);
          if (getComputedStyle(bar).display !== 'none') {
            out.errors.push('445: turning the switch off left the strip visible');
          }
          sw.click();
          await sleep(200);
          if (getComputedStyle(bar).display === 'none') {
            out.errors.push('445: turning the switch back on did not restore the strip');
          }
        }
        await click('expert-back');
        document.getElementById('settings').classList.add('hidden');
      }
      for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
      await sleep(150);
      // 510: with the chain off again, the grid must sit exactly where it did
      // before the strip ever appeared.
      const gridTopBack = document.getElementById('grid').getBoundingClientRect().top;
      if (Math.abs(gridTopBack - gridTopOff) > 1) {
        out.errors.push('510: the grid top moved from ' + Math.round(gridTopOff) + 'px to ' +
          Math.round(gridTopBack) + 'px after the chain overview strip was shown and hidden again');
      }
    }
    // 555: the chain overview rows reorder by pointer drag and drop a step on
    // right-click; the clipboard text follows both.
    {
      const findGridTile = (id) => [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
      const startBtn = findGridTile('p0')?.querySelector('.tile-chain-start');
      const ovRows = () => [...document.querySelectorAll('#chain-overview .chain-ov-row')];
      const ovNames = () => ovRows().map((r) => r.querySelector('.chain-ov-name').textContent);
      const clipParts = () => String(window.__LAST_COPY__ || '').split('\\n\\n');
      const ptr = (type, el, x, y) => el.dispatchEvent(new PointerEvent(type, {
        bubbles: true, cancelable: true, pointerId: 7, isPrimary: true, button: 0,
        buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y,
      }));
      if (!startBtn) {
        out.errors.push('555: fixture tile p0 has no chain-start button');
      } else {
        startBtn.click();
        await sleep(150);
        if (!await clickTile('p3')) out.errors.push('555: could not pick p3 into the chain');
        if (!await clickTile('p2')) out.errors.push('555: could not pick p2 into the chain');
        await sleep(150);
        const rows0 = ovRows();
        if (rows0.length !== 3) {
          out.errors.push('555: expected three chain steps, got ' + rows0.length);
        } else {
          const names0 = ovNames();
          const parts0 = clipParts();
          if (parts0.length !== 3) out.errors.push('555: the chain clipboard text does not have three parts: ' + parts0.length);
          if (!rows0[0].title) out.errors.push('555: a chain step has no tooltip');
          const r3 = rows0[2].getBoundingClientRect();
          const r1 = rows0[0].getBoundingClientRect();
          const x3 = r3.left + r3.width / 2, y3 = r3.top + r3.height / 2;
          ptr('pointerdown', rows0[2], x3, y3);
          ptr('pointermove', rows0[2], x3 + 3, y3);
          if (rows0[2].classList.contains('dragging')) out.errors.push('555: a move under the threshold already started the drag');
          const lx = r1.left + r1.width * 0.25, ly = r1.top + r1.height / 2;
          ptr('pointermove', rows0[2], x3 + 14, y3);
          ptr('pointermove', rows0[2], lx, ly);
          if (!rows0[2].classList.contains('dragging')) out.errors.push('555: the dragged step has no dragging class');
          if (!rows0[0].classList.contains('drag-over')) out.errors.push('555: the step under the pointer shows no drag-over mark');
          ptr('pointerup', rows0[2], lx, ly);
          await sleep(200);
          const names1 = ovNames();
          const want1 = [names0[2], names0[0], names0[1]];
          if (names1.join('|') !== want1.join('|')) out.errors.push('555: dragging step 3 onto the left half of step 1 gave ' + names1.join('|') + ', wanted ' + want1.join('|'));
          const parts1 = clipParts();
          const wantParts1 = [parts0[2], parts0[0], parts0[1]];
          if (parts1.join('|') !== wantParts1.join('|')) out.errors.push('555: the clipboard text did not follow the new order');
          if (ovRows().some((r) => r.classList.contains('dragging') || r.classList.contains('drag-over'))) out.errors.push('555: drag marks stayed after the drop');
          // Right-click on step 2 removes it and the clipboard follows.
          const row2 = ovRows()[1];
          const cm = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2 });
          row2.dispatchEvent(cm);
          await sleep(200);
          if (!cm.defaultPrevented) out.errors.push('555: right-click on a step did not prevent the native menu');
          if (!document.getElementById('ctx').classList.contains('hidden')) out.errors.push('555: right-click on a step opened the tile menu');
          const names2 = ovNames();
          if (names2.join('|') !== [names1[0], names1[2]].join('|')) out.errors.push('555: right-click did not remove exactly step 2: ' + names2.join('|'));
          if (clipParts().join('|') !== [parts1[0], parts1[2]].join('|')) out.errors.push('555: the clipboard did not lose the removed step');
          if (!document.body.classList.contains('chain-mode')) out.errors.push('555: removing a step ended the chain');
        }
      }
      for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
      await sleep(150);
    }
    // 554: switching to another program (blur, then focus on return) never ends
    // or clears a chain, locked or not; that includes the blur/focus pair a
    // window move causes. Only the chain button, Escape or Ctrl+V in this window end it.
    {
      const findGridTile = (id) => [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
      const p0Tile = findGridTile('p0');
      const startBtn = p0Tile && p0Tile.querySelector('.tile-chain-start');
      const pickedIds = () => ['p0', 'p3'].filter((id) => findGridTile(id)?.classList.contains('chain-picked'));
      const awayAndBack = async () => {
        window.dispatchEvent(new Event('blur'));
        await sleep(60);
        window.dispatchEvent(new Event('focus'));
        await sleep(60);
      };
      if (!startBtn) {
        out.errors.push('fixture tile p0 has no chain-start button to begin a chain with');
      } else {
        startBtn.click();
        await sleep(150);
        if (!await clickTile('p3')) out.errors.push('could not pick p3 into the chain');
        if (!document.body.classList.contains('chain-mode') || pickedIds().length !== 2) {
          out.errors.push('554: the chain did not start with two picks, cannot test the app switch');
        } else {
          await awayAndBack();
          if (!document.body.classList.contains('chain-mode')) out.errors.push('554: blur then focus ended an unlocked chain');
          if (pickedIds().length !== 2) out.errors.push('554: blur then focus dropped the picks of an unlocked chain');
          const header = document.querySelector('[data-tauri-drag-region]');
          if (header) {
            header.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
            await awayAndBack();
            if (!document.body.classList.contains('chain-mode') || pickedIds().length !== 2) {
              out.errors.push('554: a window move (drag-region mousedown, blur, focus) hurt the chain');
            }
            window.dispatchEvent(new Event('mouseup'));
          }
          await click('chain-btn'); // on -> latched
          await sleep(60);
          await awayAndBack();
          if (!document.body.classList.contains('chain-mode')) out.errors.push('554: blur then focus ended a locked chain');
          if (pickedIds().length !== 2) out.errors.push('554: blur then focus cleared the picks of a locked chain');
        }
      }
      for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
      await sleep(150);
    }
    // F13: a mixed prompt asks only for the manual variable and offers the choice
    // as a themed dropdown; the pick lands in the copied text.
    window.__LAST_COPY__ = undefined;
    window.__VAR_PAYLOAD__ = undefined;
    if (!await clickTile('p5')) {
      out.errors.push('the choice-variable prompt is missing from the grid');
    } else if (varsHidden()) {
      out.errors.push('the mixed variable prompt did not open the placeholder window');
    } else {
      const mixedRows = varReq().rows;
      const sel = mixedRows.find(r => r.options.length);
      const texts = mixedRows.filter(r => !r.options.length);
      if (!sel) {
        out.errors.push('the choice variable is not offered as a list of options');
      } else {
        if (sel.options.join(',') !== 'formal,casual,witty') {
          out.errors.push('the choice lists the wrong options');
        }
        // 425: the token's own default (tone=casual) preselects that entry
        // instead of the list's first one.
        if (sel.value !== 'casual') out.errors.push('425: the choice did not preselect the text default: ' + sel.value);
      }
      if (texts.length !== 1) out.errors.push('the mixed window shows ' + texts.length + ' text fields instead of 1');
      await varAnswer(mixedRows.map(r => r.options.length ? 'witty' : 'Rust'));
      const copied = window.__LAST_COPY__ || '';
      if (copied !== 'Write a witty note about Rust.') out.errors.push('the filled text is wrong: ' + copied);
    }
    // The chosen option must come back preselected on the next copy.
    window.__VAR_PAYLOAD__ = undefined;
    if (await clickTile('p5')) {
      const sel2 = (varReq() || { rows: [] }).rows.find(r => r.options.length);
      if (!sel2 || sel2.value !== 'witty') out.errors.push('the last choice was not remembered');
      await varCancel();
    }
    // Copying a rotating prompt with Auto on has to hand its id to copy_text —
    // that id is the only thing that lets the backend open an auto-advance
    // session, and without it the Auto switch had no effect in this window at all.
    window.__LAST_COPY__ = undefined;
    window.__LAST_COPY_ID__ = undefined;
    if (!await clickTile('p10')) {
      out.errors.push('the rotating prompt is missing from the grid');
    } else if (window.__LAST_COPY_ID__ !== 'p10') {
      out.errors.push('copying the Auto rotating prompt passed no id: ' + window.__LAST_COPY_ID__);
    }
    // A prompt whose entries carry placeholders must NOT arm one: the session
    // renders later entries from their raw text, so the next paste would deliver
    // the placeholder instead of the answer.
    window.__LAST_COPY_ID__ = undefined;
    window.__VAR_PAYLOAD__ = undefined;
    if (await clickTile('p1')) {
      await varAnswer((varReq() || { rows: [] }).rows.map(() => 'x'));
      if (window.__LAST_COPY_ID__ !== undefined) {
        out.errors.push('a placeholder prompt armed an auto-advance session: ' + window.__LAST_COPY_ID__);
      }
    }
    // The editor helper offers the reserved names as chips and writes a token.
    const editTile = [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === 'p0');
    if (editTile) {
      editTile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(200);
      const edit = document.querySelector('#ctx [data-act="edit"]');
      if (edit) { edit.click(); await sleep(400); }
    }
    if (document.getElementById('modal').classList.contains('hidden')) {
      out.tabs.push('(variable helper check skipped: the editor did not open)');
    } else {
      layoutScan('prompt editor');
      collapseRows('editor');
      mustReach('prompt editor', '#modal .modal-x, #modal .modal-actions button');
      // The variable helper is a reference, so it opens collapsed and costs no height.
      const varsBody = document.getElementById('modal-vars-body');
      const varsHead = document.getElementById('modal-vars-head');
      if (!varsBody || !varsHead) {
        out.errors.push('the variable helper has no collapsible section');
      } else {
        if (!varsBody.classList.contains('hidden')) {
          out.errors.push('the variable helper did not open collapsed');
        }
        varsHead.click();
        await sleep(150);
        if (varsBody.classList.contains('hidden')) {
          out.errors.push('the variable helper did not expand when its head was clicked');
        }
        // Expanding it must PUSH the rows below it down, never land on them. The
        // textarea sits inside a display:contents wrapper, so the generic row scan
        // cannot see it — compare it with the colour row directly.
        const taField = document.getElementById('modal-text');
        const colorRow = document.getElementById('color-row');
        const field = taField && taField.getClientRects().length
          ? taField
          : document.getElementById('modal-md-wrap');
        if (field && colorRow) {
          const a = field.getBoundingClientRect(), b = colorRow.getBoundingClientRect();
          if (a.height && b.height && b.top < a.bottom - 1) {
            const wrap = field.closest('.clear-wrap-ta') || field.parentElement;
            const w = wrap.getBoundingClientRect();
            const cs = getComputedStyle(field), cw = getComputedStyle(wrap);
            out.errors.push('the expanded variable helper left the text field over the colour row by ' +
              Math.round(a.bottom - b.top) + 'px [modal=' + document.getElementById('modal').className +
              ' field=' + Math.round(a.height) + '/' + cs.minHeight + '/' + cs.flex +
              ' wrap=' + wrap.className + ':' + Math.round(w.height) + '/' + cw.minHeight + '/' + cw.flex + ']');
          }
        }
        layoutScan('prompt editor, variables open');
      }
      const chips = [...document.querySelectorAll('#modal-vars-chips .vars-chip')];
      if (chips.length !== 16) {
        out.errors.push('the editor lists ' + chips.length + ' automatic placeholders instead of 16');
      } else if (chips.some(c => !c.title.trim())) {
        out.errors.push('an automatic placeholder chip has no explanation');
      }
      // Every placeholder chip looks the same: singling /clipboard\\ out read as
      // "this one is selected", which it never is.
      const clipChip = chips.find(c => c.textContent === '/clipboard\\\\');
      if (!clipChip) out.errors.push('the clipboard placeholder chip is missing');
      if (chips.some(c => c.classList.contains('vars-chip-highlight'))) {
        out.errors.push('a placeholder chip is still highlighted');
      }
      const ta = document.getElementById('modal-text');
      const before = ta.value;
      chips[0] && chips[0].click();
      await sleep(150);
      // A reserved name is written with its slash fence, so the plain word stays
      // available as a variable of the user's own.
      if (ta.value === before || !ta.value.includes('{#{/date\\\\}#}')) {
        out.errors.push('clicking a placeholder chip did not insert it: ' + ta.value);
      }
      // P33: a chip inserts at the CURSOR, not appended to the end of the text.
      ta.value = 'Say AAA then BBB.';
      ta.focus();
      ta.setSelectionRange(8, 8); // right after "Say AAA "
      chips[1] && chips[1].click();
      await sleep(150);
      if (!ta.value.startsWith('Say AAA {#{') || !ta.value.includes('then BBB.')) {
        out.errors.push('a chip did not insert at the cursor position: ' + ta.value);
      }
      document.getElementById('modal-var-name').value = 'tone';
      document.getElementById('modal-var-opts').value = ' formal , casual ,, ';
      document.getElementById('modal-var-add').click();
      await sleep(150);
      if (!ta.value.includes('{#{tone|formal,casual}#}')) {
        out.errors.push('the helper did not insert a choice variable: ' + ta.value);
      }
      // 456: the notes field sits directly under the name row, full width, no
      // heading, always visible (no more collapse) and holds the stored note.
      const nRow = document.getElementById('modal-notes');
      const nText = document.getElementById('modal-notes-text');
      const nameRow = document.querySelector('.modal-name-row');
      if (!nRow || getComputedStyle(nRow).display === 'none') {
        out.errors.push('the editor has no notes row');
      } else if (nameRow.nextElementSibling !== nRow) {
        out.errors.push('the notes field is not right under the name row');
      } else if (document.getElementById('modal-notes-head')) {
        out.errors.push('the notes field still has a collapse header');
      } else if (!nText.value.includes('Moonshot')) {
        out.errors.push('the editor did not load the stored note');
      } else {
        // P35: the note field reads as a different kind of box, not a second
        // prompt field — its background must not match the main text area's.
        const notesBg = getComputedStyle(nText).backgroundColor;
        const promptBg = getComputedStyle(document.getElementById('modal-text')).backgroundColor;
        if (notesBg === promptBg) out.errors.push('the notes field looks identical to the prompt field');
        // 456: with the feature off, the gate hides the wrapper itself (not just
        // the textarea), so no empty gap is left where the field used to be.
        document.body.classList.add('noflag-promptNotes');
        await sleep(30);
        if (getComputedStyle(nRow).display !== 'none') {
          out.errors.push('promptNotes off does not hide the notes row');
        }
        document.body.classList.remove('noflag-promptNotes');
        await sleep(30);
      }
      // F20: the hotkey row opens collapsed (28: as part of "Advanced options" now),
      // shows the stored accelerator, refuses a combination that is already taken,
      // accepts a free one and can be cleared.
      const hRow = document.getElementById('modal-hotkey');
      const hBody = document.getElementById('modal-advanced-body');
      const hField = document.getElementById('modal-hotkey-input');
      const hErr = document.getElementById('modal-hotkey-err');
      const press = async (opts) => {
        hField.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, opts)));
        await sleep(150);
      };
      if (!hRow || getComputedStyle(hRow).display === 'none') {
        out.errors.push('the editor has no hotkey row');
      } else if (!hBody.classList.contains('hidden')) {
        out.errors.push('advanced options does not start collapsed');
      } else {
        document.getElementById('modal-advanced-head').click();
        await sleep(250);
        if (hBody.classList.contains('hidden')) out.errors.push('advanced options did not expand');
        if (!/9$/.test(hField.value) || !/\\+/.test(hField.value)) {
          out.errors.push('the stored hotkey is not shown readably: ' + hField.value);
        }
        const badge = document.getElementById('modal-hotkey-badge');
        if (badge.classList.contains('hidden') || !badge.textContent.trim()) {
          out.errors.push('the collapsed hotkey row shows no binding');
        }
        // The app hotkey is Ctrl+Shift+P — a prompt must not be able to take it.
        const before = hField.value;
        await press({ key: 'P', ctrlKey: true, shiftKey: true });
        if (hErr.classList.contains('hidden') || !hErr.textContent.trim()) {
          out.errors.push('a taken combination produced no inline error');
        }
        if (hField.value !== before) out.errors.push('a rejected combination was captured anyway');
        // A modifier-less press is refused too, with a reason.
        await press({ key: 'K' });
        if (hErr.classList.contains('hidden')) out.errors.push('a modifier-less key produced no inline error');
        // A universal system shortcut is refused by the backend, never captured.
        const beforeSys = hField.value;
        await press({ key: 'c', ctrlKey: true });
        if (hErr.classList.contains('hidden') || !hErr.textContent.trim()) {
          out.errors.push('Ctrl+C produced no inline error');
        }
        if (hField.value !== beforeSys) out.errors.push('a system shortcut was captured anyway: ' + hField.value);
        // 430: whatever the reason, a button under the message opens the full list
        // instead of leaving the user to guess what already holds the combination.
        const showAll = document.getElementById('modal-hotkey-showall');
        if (!showAll || showAll.classList.contains('hidden')) {
          out.errors.push('430: no "show all hotkeys" button next to a hotkey error');
        } else {
          showAll.click();
          await sleep(200);
          const allPopup = document.getElementById('shortcuts-modal');
          if (allPopup.classList.contains('hidden') || document.getElementById('shortcuts-global').classList.contains('hidden')) {
            out.errors.push('430: the show-all button did not open the hotkeys list');
          }
          document.getElementById('shortcuts-modal-close').click();
          await sleep(150);
        }
        // A combination another running app owns is refused the same way.
        await press({ key: 'B', ctrlKey: true, altKey: true });
        if (hErr.classList.contains('hidden')) out.errors.push('a combination owned by another app produced no inline error');
        if (hField.value !== beforeSys) out.errors.push('a conflicting combination was captured anyway: ' + hField.value);
        // A free combination is captured and clears the error.
        await press({ key: 'J', ctrlKey: true, altKey: true });
        if (!hErr.classList.contains('hidden')) out.errors.push('the inline error survived a valid capture');
        if (!/J$/.test(hField.value)) out.errors.push('a free combination was not captured: ' + hField.value);
        // F20: the copy-vs-paste choice belongs to THIS hotkey, and its default
        // entry has to spell out what it inherits.
        const actRow = document.getElementById('modal-hotkey-action-row');
        const actSel = document.getElementById('modal-hotkey-action');
        if (!actRow || actRow.classList.contains('hidden')) {
          out.errors.push('a bound hotkey shows no copy/paste choice of its own');
        } else {
          const def = actSel.querySelector('option[value=""]');
          if (!def || !/\\(.+\\)/.test(def.textContent || '')) {
            out.errors.push('the default entry does not name the inherited mode: ' + (def && def.textContent));
          }
          if ([...actSel.options].map(o => o.value).join(',') !== ',copy,paste') {
            out.errors.push('the hotkey action dropdown offers the wrong entries');
          }
        }
        document.getElementById('modal-hotkey-clear').click();
        await sleep(150);
        if (hField.value) out.errors.push('the clear button left a hotkey behind: ' + hField.value);
        if (actRow && !actRow.classList.contains('hidden')) {
          out.errors.push('the copy/paste choice stayed visible without a binding');
        }
      }
      // Both collapsibles are open now — the state that used to squeeze them to a
      // 6px sliver and push Save out of reach on a short window.
      layoutScan('prompt editor, rows expanded');
      collapseRows('editor');
      mustReach('prompt editor, rows expanded', '#modal .modal-x, #modal .modal-actions button');
      await click('modal-close-x');
      await sleep(250);
      // Editing made the dialog dirty, so the discard confirmation may step in.
      const cm2 = document.getElementById('confirm-modal');
      if (!cm2.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
    }
    // Item 1: "All hotkeys" popup — global + every per-prompt hotkey in one list,
    // each row live-rebindable with the same collision/free-combo behaviour the
    // editor's own field already has. Prompt 1 (p0) starts with a bound hotkey in
    // the fixture; every mutating step below restores it before the block ends, so
    // later blocks that fire a 'prompt-hotkey' event for p0/p1/p2 see it unchanged.
    {
      await click('gear');
      const listBtn = document.getElementById('hotkey-list-btn');
      if (!listBtn) {
        out.errors.push('the "All hotkeys" button is missing next to the global hotkey field');
      } else {
        listBtn.click();
        await sleep(150);
        const popup = document.getElementById('shortcuts-modal');
        if (popup.classList.contains('hidden') || document.getElementById('shortcuts-global').classList.contains('hidden')) {
          out.errors.push('the "All shortcuts" dialog did not open with the global hotkeys on top');
        } else {
          const rowByName = (name) => [...document.querySelectorAll('.hotkeys-row-name')]
            .find((el) => el.textContent === name)?.closest('.hotkeys-row');
          const globalRow = () => document.querySelector('.hotkeys-row-global')?.closest('.hotkeys-row');
          const globalInput = () => globalRow()?.querySelector('.modal-input');
          const p1Input = () => rowByName('Prompt 1')?.querySelector('.modal-input');
          if (!globalRow() || !/P$/.test(globalInput().value)) {
            out.errors.push('the popup does not list the global hotkey');
          }
          if (!rowByName('Prompt 1') || !/9$/.test(p1Input().value)) {
            out.errors.push('the popup does not list Prompt 1 with its stored hotkey');
          }
          if (rowByName('Prompt 4')) out.errors.push('the popup lists a prompt with no hotkey bound');
          // 343: the recording stop/pause hotkeys moved out of Settings and into the
          // expert menu's own capture row, but still show up here too, same as every
          // other hotkey in the app.
          const recStopInput = rowByName('Stop-recording hotkey')?.querySelector('.modal-input');
          const recPauseInput = rowByName('Pause/resume-recording hotkey')?.querySelector('.modal-input');
          if (!recStopInput || !/8$/.test(recStopInput.value)) {
            out.errors.push('the popup does not list the stop-recording hotkey with its stored value');
          }
          if (!recPauseInput || !/7$/.test(recPauseInput.value)) {
            out.errors.push('the popup does not list the pause-recording hotkey with its stored value');
          }
          if (document.querySelectorAll('.hotkeys-row').length !== 4) {
            out.errors.push('expected 4 rows in the "All hotkeys" popup (global, one prompt, two recording), got ' +
              document.querySelectorAll('.hotkeys-row').length);
          }
          // 672: the same dialog lists the keys of every window, one titled section each,
          // including view switching in the main window.
          const secTitles = [...document.querySelectorAll('#shortcuts-modal-list .shortcuts-section-title')].map((h) => h.textContent);
          const wantTitles = ['Main window', 'Quick launcher', 'Tray menu', 'Placeholder window', 'Floating buttons and folders', 'Screenshot selection', 'Recording bar', 'Camera'];
          if (secTitles.join('|') !== wantTitles.join('|')) {
            out.errors.push('672: the shortcut overview sections are: ' + secTitles.join(', '));
          }
          if (![...document.querySelectorAll('#shortcuts-modal-list .shortcuts-row-keys')].some((k) => k.textContent === 'Ctrl+Tab')) {
            out.errors.push('672: the shortcut overview does not list Ctrl+Tab');
          }
          // Search filters by name — a prompt-name query hides the global row.
          const search = document.getElementById('hotkeys-search');
          search.value = 'Prompt 1';
          search.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(80);
          if (!rowByName('Prompt 1')) out.errors.push('the search hid a matching row');
          if (globalRow()) out.errors.push('the search did not filter out the non-matching global row');
          search.value = '';
          search.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(80);
          const pressRow = async (input, opts) => {
            input.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, opts)));
            await sleep(150);
          };
          // A combination owned by another running app is refused, unchanged.
          const beforeP1 = p1Input().value;
          await pressRow(p1Input(), { key: 'B', ctrlKey: true, altKey: true });
          if (p1Input().value !== beforeP1) out.errors.push('the popup captured a combination owned by another app');
          // A free combination is captured and actually persisted (update_prompt).
          window.__SMOKE_ARGS__ = [];
          await pressRow(p1Input(), { key: 'K', ctrlKey: true, altKey: true });
          if (!/K$/.test(p1Input().value)) out.errors.push('the popup did not capture a free combination: ' + p1Input().value);
          const upd = (window.__SMOKE_ARGS__ || []).find(([cmd]) => cmd === 'update_prompt');
          if (!upd || !/\\+K$/i.test(upd[1].hotkey || '')) {
            out.errors.push('rebinding a popup row did not persist through update_prompt');
          }
          // Restore Prompt 1's original hotkey so later blocks see it unchanged.
          await pressRow(p1Input(), { key: '9', ctrlKey: true, shiftKey: true });
          if (!/9$/.test(p1Input().value)) out.errors.push('restoring Prompt 1s original hotkey failed: ' + p1Input().value);
          // The global row clears (and restores) the same way — it never disappears
          // from the list even with no binding, unlike a per-prompt row.
          const clearBtn = globalRow().querySelector('.ghost-btn');
          clearBtn.click();
          await sleep(150);
          if (globalInput().value) out.errors.push('clearing the global row from the popup left a value behind');
          if (!globalRow()) out.errors.push('the global row disappeared after being cleared');
          await pressRow(globalInput(), { key: 'P', ctrlKey: true, shiftKey: true });
          if (!/P$/.test(globalInput().value)) out.errors.push('restoring the global hotkey failed: ' + globalInput().value);
        }
        document.getElementById('shortcuts-modal-close').click();
        await sleep(120);
      }
      document.getElementById('settings-close')?.click();
      await sleep(120);
    }
    // F26: the pipette is in the composer bar, and a picked colour reports back as a
    // toast tinted with that colour.
    const pickBtn = document.getElementById('picker-btn');
    if (!pickBtn) out.errors.push('the colour picker button is missing');
    else {
      if (getComputedStyle(pickBtn).display === 'none') out.errors.push('the colour picker button stays hidden although the feature is on');
      pickBtn.click();
      await sleep(150);
      if (!(window.__SMOKE_CALLS__ || []).includes('open_picker')) out.errors.push('the pipette did not open the picker');
    }
    window.__FIRE__('color-picked', { text: '#1A2B3C', hex: '#1A2B3C' });
    await sleep(150);
    const toastEl = document.getElementById('toast');
    if (!toastEl.classList.contains('tinted')) out.errors.push('the picked colour did not tint its toast');
    if (!(toastEl.textContent || '').includes('#1A2B3C')) {
      out.errors.push('the picker toast reads "' + toastEl.textContent + '"');
    }
    if (toastEl.style.getPropertyValue('--toast-tint') !== '#1A2B3C') {
      out.errors.push('the picker toast carries no colour');
    }
    // 19: right-click on the pipette reaches its history without opening the
    // overlay — the fixture backend always returns a fixed two-colour list.
    if (pickBtn) {
      pickBtn.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(150);
      const pop = document.querySelector('.pick-history-pop');
      const rows = pop ? [...pop.querySelectorAll('.pick-history-row')] : [];
      if (rows.length !== 2) {
        out.errors.push('19: the pick-history popover shows ' + rows.length + ' entries instead of 2');
      } else {
        if (!rows[0].querySelector('.pick-history-hex').textContent.includes('AABBCC')) {
          out.errors.push('19: the pick-history popover does not show the expected hex value');
        }
        if (!rows[0].querySelector('.pick-history-swatch')) out.errors.push('19: a pick-history row has no colour swatch');
        rows[0].click();
        await sleep(150);
        if (window.__LAST_COPY__ !== undefined && window.__SMOKE_CALLS__.includes('copy_pick_history_entry') === false) {
          out.errors.push('19: clicking a pick-history entry did not call copy_pick_history_entry');
        }
        if (!pop.classList.contains('hidden')) out.errors.push('19: the pick-history popover stayed open after picking an entry');
      }
      // An outside click must close it too, same as every other popover.
      pickBtn.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(120);
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
      await sleep(120);
      if (!document.querySelector('.pick-history-pop').classList.contains('hidden')) {
        out.errors.push('19: clicking outside the pick-history popover did not close it');
      }
    }
    // 417: right-click either composer button for a quiet menu to open an
    // existing file in whichever editor its content calls for, not only
    // through a fresh capture or recording.
    const snipBtnEl = document.getElementById('snip-btn');
    const recordBtnEl = document.getElementById('record-btn');
    if (!snipBtnEl || !recordBtnEl) {
      out.errors.push('417: the screenshot or record button is missing');
    } else {
      // Every composer button with a right-click function names it in parentheses
      // in its tooltip; the accessible name of the first two stays the plain name.
      for (const id of ['snip-btn', 'record-btn', 'picker-btn', 'file-btn']) {
        const btnEl = document.getElementById(id) || {};
        // A running recording swaps the record tooltip for its stop text.
        if (id === 'record-btn' && btnEl.classList && (btnEl.classList.contains('is-recording') || btnEl.classList.contains('is-paused'))) {
          if (btnEl.dataset.i18nTitle !== 'recordBtnTip') out.errors.push('the record button idle tooltip is not the one with the right-click hint');
          continue;
        }
        const tip = btnEl.title || '';
        if (!/[(（].+[:：].+[)）]/.test(tip)) out.errors.push('composer button ' + id + ' tooltip does not name its right-click function: ' + tip);
      }
      for (const b of [snipBtnEl, recordBtnEl]) {
        const al = b.getAttribute('aria-label') || '';
        if (!al || /[(（]/.test(al)) out.errors.push('composer button ' + b.id + ' has no plain accessible name: ' + al);
      }
      recordBtnEl.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(150);
      let openEditPop = document.querySelector('.open-editor-pop');
      if (!openEditPop || openEditPop.classList.contains('hidden')) {
        out.errors.push('417: right-clicking the record button did not open the open-in-editor menu');
      }
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
      await sleep(120);
      if (openEditPop && !openEditPop.classList.contains('hidden')) {
        out.errors.push('417: clicking outside the open-in-editor menu did not close it');
      }
      // Picking a VIDEO through the screenshot (image) button must still open
      // the video editor: content decides, never which button was used.
      snipBtnEl.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(150);
      openEditPop = document.querySelector('.open-editor-pop');
      if (!openEditPop || openEditPop.classList.contains('hidden')) {
        out.errors.push('417: right-clicking the screenshot button did not open the open-in-editor menu');
      } else if (!(openEditPop.querySelector('button').textContent || '').trim()) {
        out.errors.push('417: the open-in-editor menu entry has no label');
      } else {
        window.__SMOKE_CALLS__ = [];
        window.__SMOKE_ARGS__ = [];
        openEditPop.querySelector('button').click();
        await sleep(200);
        if (!window.__SMOKE_CALLS__.includes('pick_media_file_path')) {
          out.errors.push('417: choosing the menu entry did not offer a file picker');
        }
        if (!window.__SMOKE_CALLS__.includes('open_file_in_editor')) {
          out.errors.push('417: choosing the menu entry did not ask the backend what the file is');
        }
        const openCall = (window.__SMOKE_ARGS__ || []).find((c) => c[0] === 'open_video_editor');
        if (!openCall) {
          out.errors.push('417: picking a video through the screenshot button did not open the video editor');
        } else {
          if (openCall[1].srcPath !== 'C:\\\\Temp\\\\ps-smoke-openedit-copy.mp4') {
            out.errors.push('417: the video editor opened on the wrong file: ' + openCall[1].srcPath);
          }
          if (openCall[1].originDir !== 'C:\\\\Pictures') {
            out.errors.push('417: the video editor did not receive the origin folder');
          }
        }
      }
    }
    // 506: a record-btn click while a recording is already running must stop
    // it instead of trying to open a second one -- recording_status's own
    // fixture always reports an active recording, so forcing one resync
    // through the same visibilitychange path the real app uses (rather than
    // waiting out the real 5-second poll) is enough to reach that state.
    window.__SMOKE_CALLS__ = [];
    document.dispatchEvent(new Event('visibilitychange'));
    await sleep(150);
    if (!recordBtnEl.classList.contains('is-recording') && !recordBtnEl.classList.contains('is-paused')) {
      out.errors.push('506: record button never picked up the active-recording state from recording_status');
    } else {
      window.__SMOKE_CALLS__ = [];
      recordBtnEl.click();
      await sleep(150);
      if (!window.__SMOKE_CALLS__.includes('stop_recording')) {
        out.errors.push('506: clicking the record button during an active recording did not call stop_recording');
      }
      if (window.__SMOKE_CALLS__.includes('open_recorder')) {
        out.errors.push('506: clicking the record button during an active recording also called open_recorder');
      }
    }
    // P60: an automatic backup toasts the same way a manual one already did —
    // the backend has no other call site to toast from, so it fires this itself.
    window.__FIRE__('backup-done');
    await sleep(150);
    if (!(toastEl.textContent || '').includes('Backup created')) {
      out.errors.push('an automatic backup gave no "Backup created" toast: ' + toastEl.textContent);
    }

    // F20/98: pressing a prompt hotkey runs the same activation a tile does — the
    // copy is recorded, and whether it also pastes follows the expert option alone
    // (default on since 98).
    const calls = () => (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []);
    calls().length = 0;
    window.__FIRE__('prompt-hotkey', 'p0');
    await sleep(400);
    if (!calls().includes('copy_prompt') && !calls().includes('copy_text')) {
      out.errors.push('a prompt hotkey did not copy the prompt');
    }
    if (!calls().includes('record_copy')) out.errors.push('a prompt hotkey did not record the copy');
    if (!calls().includes('paste_into_previous')) {
      out.errors.push('a prompt hotkey did not paste although the paste option defaults on');
    }
    // F12: a hotkey copy of a prompt with MANUAL placeholders asks in the small
    // standalone window — the main window must not be pulled forward for it.
    calls().length = 0;
    window.__VAR_PAYLOAD__ = undefined;
    window.__LAST_COPY__ = undefined;
    window.__FIRE__('prompt-hotkey', 'p1');
    await sleep(400);
    if (!calls().includes('open_var_window')) {
      out.errors.push('a hotkey copy with placeholders did not open the standalone window');
    }
    if (calls().includes('focus_main_window')) {
      out.errors.push('the hotkey fill-in pulled the main window forward');
    }
    {
      let req = null;
      try { req = JSON.parse(window.__VAR_PAYLOAD__ || 'null'); } catch (e) { req = null; }
      if (!req || !Array.isArray(req.rows) || req.rows.length !== 2) {
        out.errors.push('the standalone window was handed the wrong rows');
      } else if (req.rows.map(r => r.name).join(',') !== 'Text,Language') {
        out.errors.push('the standalone rows are ' + req.rows.map(r => r.name).join(','));
      } else if (!req.title || !req.ok) {
        out.errors.push('the standalone window was handed no labels');
      } else {
        // 425: same prefill rule as the in-window dialog -- the standalone
        // window is handed the resolved value, not just the raw token.
        if (req.rows[0].value !== 'Hello') out.errors.push('425: the standalone window got the wrong default: ' + req.rows[0].value);
        if (req.rows[1].value) out.errors.push('425: the standalone window prefilled a variable with no default: ' + req.rows[1].value);
      }
      // Answer the way the small window does, then the copy has to finish here.
      window.__FIRE__('vars-filled', JSON.stringify({ values: ['Hello', 'Dutch'], remember: [false, false] }));
      await sleep(400);
      if (window.__LAST_COPY__ !== 'Translate Hello into Dutch.') {
        out.errors.push('the standalone answer did not finish the copy: ' + window.__LAST_COPY__);
      }
      if (!calls().includes('record_copy')) out.errors.push('the standalone copy was not recorded');
    }
    // Cancelling it copies nothing at all.
    calls().length = 0;
    window.__LAST_COPY__ = undefined;
    await sleep(2400); // past the per-prompt copy cooldown (2000ms)
    window.__FIRE__('prompt-hotkey', 'p1');
    await sleep(300);
    window.__FIRE__('vars-filled', null);
    await sleep(300);
    if (window.__LAST_COPY__ !== undefined) out.errors.push('a cancelled fill-in copied anyway');

    // Turn the paste option on through its real expert switch, then fire again (a
    // different prompt, so the per-prompt copy cooldown cannot swallow it).
    // The menu only renders the active tab, so walk the tabs until the row shows up.
    const expertRow = async (label) => {
      const hit = () => [...document.querySelectorAll('#expert-flags .switch-field')]
        .find(r => ((r.querySelector('span') || {}).textContent || '').includes(label));
      if (hit()) return hit();
      for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
        tb.click();
        await sleep(200);
        if (hit()) return hit();
      }
      return null;
    };
    document.getElementById('settings').classList.remove('hidden');
    await click('expert-open');
    const pasteRow = await expertRow('hotkey also pastes');
    if (!pasteRow) {
      out.errors.push('the prompt-hotkey paste option is missing from the expert menu');
    } else {
      // 98: default on -- one click turns it off.
      pasteRow.querySelector('input.switch').click();
      await sleep(250);
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
      calls().length = 0;
      window.__FIRE__('prompt-hotkey', 'p2');
      await sleep(500);
      if (calls().includes('paste_into_previous')) {
        out.errors.push('the prompt hotkey pasted although the option was just switched off');
      }
      // Put it back so the rest of the run sees the shipped default.
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const back = await expertRow('hotkey also pastes');
      if (back) { back.querySelector('input.switch').click(); await sleep(200); }
      // The walk above left another tab active; later checks expect the first one.
      const firstTab = document.querySelector('#expert-tabs-bar .expert-tab');
      if (firstTab) { firstTab.click(); await sleep(200); }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // 511/551: the caution hint is the first child of the expert page, red and
    // centered in every mode; the quick-profiles row follows it.
    {
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const expertBox = document.getElementById('settings-expert');
      const hint = document.getElementById('expert-hint');
      const expertPage = document.getElementById('settings-expert');
      const hintOk = (label) => {
        if (!hint || expertPage.firstElementChild !== hint) {
          out.errors.push('551: the caution hint is not the first child of the expert page (' + label + ')');
          return;
        }
        const dangerProbe = document.createElement('span');
        dangerProbe.style.color = 'var(--danger)';
        document.body.appendChild(dangerProbe);
        const dangerColor = getComputedStyle(dangerProbe).color;
        dangerProbe.remove();
        if (getComputedStyle(hint).color !== dangerColor) {
          out.errors.push('551: the caution hint is not in the danger colour (' + label + '): ' + getComputedStyle(hint).color + ' vs ' + dangerColor);
        }
        const hr = hint.getBoundingClientRect();
        const pr = expertPage.getBoundingClientRect();
        const content = [...hint.children].map((c) => c.getBoundingClientRect());
        const cx = (Math.min(...content.map((r) => r.left)) + Math.max(...content.map((r) => r.right))) / 2;
        const px = pr.left + pr.width / 2;
        if (hr.width < 1 || Math.abs(cx - px) > 2) {
          out.errors.push('551: the caution hint is not centered (' + label + '): content x ' + cx + ' vs page x ' + px);
        }
        if (hint.matches('button, a, input, select, [tabindex]') || hint.hasAttribute('tabindex')) {
          out.errors.push('551: the caution hint is focusable, it should be plain text');
        }
      };
      hintOk('browse');
      const topRow551 = expertBox.querySelector('.expert-top-row');
      if (!topRow551 || hint.nextElementSibling !== topRow551) {
        out.errors.push('511: the expert menu top row (presets plus reset) does not follow the hint');
      } else if (!topRow551.querySelector('.expert-presets') || topRow551.querySelector('.expert-presets') !== topRow551.firstElementChild) {
        out.errors.push('511: the quick-profiles group is not the first thing in the top row');
      }
      const expertSearch551 = document.getElementById('expert-search');
      expertSearch551.value = 'shadow';
      expertSearch551.dispatchEvent(new Event('input'));
      await sleep(250);
      hintOk('while searching');
      expertSearch551.value = '';
      expertSearch551.dispatchEvent(new Event('input'));
      await sleep(250);
      const firstTab0 = document.querySelector('#expert-tabs-bar .expert-tab');
      if (firstTab0) { firstTab0.click(); await sleep(100); }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // 513: the drawing-color row -- a real swatch (not a bare default button),
    // sized and bordered like the app's other color triggers, with the hex
    // value shown as text, at about the same row height as a dropdown row.
    {
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const search = document.getElementById('expert-search');
      search.value = 'Screenshot editor';
      search.dispatchEvent(new Event('input'));
      await sleep(250);
      const colorRow513 = document.querySelector('#expert-flags [data-setting-row="snipDefaultColor"]');
      const toolRow513 = document.querySelector('#expert-flags [data-setting-row="snipEditDefaultTool"]');
      if (!colorRow513) {
        out.errors.push('513: the default drawing color row is missing (search did not surface it)');
      } else {
        const swatch = colorRow513.querySelector('.color-swatch');
        if (!swatch) {
          out.errors.push('513: the color row has no .color-swatch element');
        } else {
          const cs = getComputedStyle(swatch);
          if (Math.round(parseFloat(cs.width)) !== 28) {
            out.errors.push('513: the color swatch is ' + cs.width + ' wide, expected 28px');
          }
          if (parseFloat(cs.borderTopWidth) <= 0) {
            out.errors.push('513: the color swatch has no border');
          }
          if (!colorRow513.querySelector('.value-out')) {
            out.errors.push('513: the color row shows no hex value text beside the swatch');
          }
        }
        if (toolRow513) {
          const h1 = Math.round(colorRow513.getBoundingClientRect().height);
          const h2 = Math.round(toolRow513.getBoundingClientRect().height);
          if (Math.abs(h1 - h2) > 4) {
            out.errors.push('513: the color row is ' + h1 + 'px tall vs a dropdown row at ' + h2 + 'px, over the 4px tolerance');
          }
        }
      }
      search.value = '';
      search.dispatchEvent(new Event('input'));
      await sleep(150);
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // 514: the remote-access port is a typed number field, not a slider --
    // an out-of-range value shows the inline hint and commits nothing, a
    // valid one commits and clears the hint.
    {
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const search514 = document.getElementById('expert-search');
      const findFlagRow = (label) => [...document.querySelectorAll('#expert-flags .switch-field')]
        .find((r) => ((r.querySelector('span') || {}).textContent || '').includes(label));
      search514.value = 'Remote access (local API)';
      search514.dispatchEvent(new Event('input'));
      await sleep(250);
      const remoteFlagRow = findFlagRow('Remote access (local API)');
      const remoteSw = remoteFlagRow && remoteFlagRow.querySelector('input.switch');
      if (!remoteSw) {
        out.errors.push('514: could not find the remote-access switch to enable it for the port test');
      } else {
        if (!remoteSw.checked) { remoteSw.click(); await sleep(250); }
        search514.value = 'Remote access port';
        search514.dispatchEvent(new Event('input'));
        await sleep(250);
        const portRow = document.querySelector('#expert-flags [data-setting-row="remotePort"]');
        if (!portRow) {
          out.errors.push('514: the remote-access port row is missing');
        } else {
          if (portRow.querySelector('input[type=range]')) {
            out.errors.push('514: the port row still shows a slider');
          }
          const input514 = portRow.querySelector('.expert-num');
          const hint514 = portRow.querySelector('.expert-row-hint');
          if (!input514 || !hint514) {
            out.errors.push('514: the port row has no text input or hint element');
          } else {
            const portCommits = () => (window.__SMOKE_ARGS__ || [])
              .filter(([cmd, args]) => cmd === 'set_ui_value' && args && args.key === 'remotePort')
              .map(([, args]) => args.value);
            const before514 = portCommits().length;
            input514.value = '80';
            input514.dispatchEvent(new Event('change'));
            await sleep(150);
            if (portCommits().length > before514) {
              out.errors.push('514: an out-of-range port (80) was committed');
            }
            if (hint514.classList.contains('hidden')) {
              out.errors.push('514: no invalid-port hint shown for an out-of-range value');
            }
            input514.value = '51900';
            input514.dispatchEvent(new Event('change'));
            await sleep(150);
            const after514 = portCommits();
            if (after514[after514.length - 1] !== 51900) {
              out.errors.push('514: a valid port (51900) was not committed, got ' + after514[after514.length - 1]);
            }
            if (!hint514.classList.contains('hidden')) {
              out.errors.push('514: the invalid-port hint stayed visible after a valid value');
            }
          }
        }
        // 515: the rate-limit slider has one extra stop past its real max,
        // showing the unlimited label and storing 0.
        search514.value = 'Remote access requests per minute';
        search514.dispatchEvent(new Event('input'));
        await sleep(250);
        const rateRow = document.querySelector('#expert-flags [data-setting-row="remoteRateLimit"]');
        if (!rateRow) {
          out.errors.push('515: the remote rate-limit row is missing');
        } else {
          const rateSlider = rateRow.querySelector('input[type=range]');
          const rateOut = rateRow.querySelector('.value-out');
          if (!rateSlider || !rateOut) {
            out.errors.push('515: the rate-limit row has no slider or value readout');
          } else {
            const rateCommits = () => (window.__SMOKE_ARGS__ || [])
              .filter(([cmd, args]) => cmd === 'set_ui_value' && args && args.key === 'remoteRateLimit')
              .map(([, args]) => args.value);
            const belowMax = String(Number(rateSlider.max) - Number(rateSlider.step)) + '/min';
            rateSlider.value = String(Number(rateSlider.max) - Number(rateSlider.step));
            rateSlider.dispatchEvent(new Event('input'));
            const readoutBelowMax = rateOut.textContent;
            if (readoutBelowMax !== belowMax) {
              out.errors.push('515: the slider one stop below max reads "' + readoutBelowMax + '" instead of "' + belowMax + '"');
            }
            rateSlider.value = rateSlider.max;
            rateSlider.dispatchEvent(new Event('input'));
            if (rateOut.textContent === (rateSlider.max + '/min') || rateOut.textContent === readoutBelowMax) {
              out.errors.push('515: the top slider stop does not show a distinct unlimited label: ' + rateOut.textContent);
            }
            rateSlider.dispatchEvent(new Event('change'));
            await sleep(150);
            const rc = rateCommits();
            if (rc[rc.length - 1] !== 0) {
              out.errors.push('515: the unlimited slider stop did not commit 0, got ' + rc[rc.length - 1]);
            }
          }
        }
        // Put remoteApi back off so the rest of the run sees the shipped default.
        search514.value = 'Remote access (local API)';
        search514.dispatchEvent(new Event('input'));
        await sleep(200);
        const remoteFlagRowBack = findFlagRow('Remote access (local API)');
        const remoteSwBack = remoteFlagRowBack && remoteFlagRowBack.querySelector('input.switch');
        if (remoteSwBack && remoteSwBack.checked) { remoteSwBack.click(); await sleep(200); }
      }
      // 512: the dark halo behind white text is a live setting on the blue
      // sample tile (p0, always white text under the shipped threshold) -
      // default on at the current look, switch off clears it, a lower
      // strength lightens it.
      const inkShadowOf512 = () => {
        const t = [...document.querySelectorAll('#grid .tile')].find((e) => (e.dataset.id || '') === 'p0');
        return t ? t.style.getPropertyValue('--ink-shadow') : null;
      };
      const todayHalo512 = '0 0 2px rgba(0, 0, 0, 0.75), 0 1px 3px rgba(0, 0, 0, 0.62)';
      const startHalo512 = inkShadowOf512();
      if (startHalo512 === null) {
        out.errors.push('512: could not find the blue sample tile (p0) to check the ink halo');
      } else if (startHalo512 !== todayHalo512) {
        out.errors.push('512: the default ink halo does not match the shipped look, got ' + startHalo512);
      } else {
        search514.value = 'Dark shadow behind white text';
        search514.dispatchEvent(new Event('input'));
        await sleep(200);
        const findHaloSw512 = () => {
          const row = findFlagRow('Dark shadow behind white text');
          return row && row.querySelector('input.switch');
        };
        if (!findHaloSw512()) {
          out.errors.push('512: could not find the ink halo switch');
        } else {
          // inkHalo gates the strength row, so toggling it re-renders the
          // search results -- re-find the switch after every click instead
          // of holding onto a row the re-render just replaced.
          findHaloSw512().click();
          await sleep(200);
          if (inkShadowOf512() !== 'none') {
            out.errors.push('512: switching the ink halo off did not clear the tile shadow, got ' + inkShadowOf512());
          }
          const haloSwBack = findHaloSw512();
          if (!haloSwBack) {
            out.errors.push('512: the ink halo switch disappeared after switching it off');
          } else {
            haloSwBack.click();
            await sleep(200);
            if (inkShadowOf512() !== todayHalo512) {
              out.errors.push('512: switching the ink halo back on did not restore the shipped look, got ' + inkShadowOf512());
            }
          }
          search514.value = 'Shadow strength';
          search514.dispatchEvent(new Event('input'));
          await sleep(200);
          const strengthRow512 = document.querySelector('#expert-flags [data-setting-row="inkHaloStrength"]');
          const strengthSlider512 = strengthRow512 && strengthRow512.querySelector('input[type=range]');
          if (!strengthSlider512) {
            out.errors.push('512: could not find the shadow strength slider');
          } else {
            strengthSlider512.value = '40';
            strengthSlider512.dispatchEvent(new Event('input'));
            strengthSlider512.dispatchEvent(new Event('change'));
            await sleep(200);
            const lowerHalo512 = '0 0 2px rgba(0, 0, 0, 0.4), 0 1px 3px rgba(0, 0, 0, 0.33)';
            if (inkShadowOf512() !== lowerHalo512) {
              out.errors.push('512: a lower shadow strength (40) did not lighten the tile halo, got ' + inkShadowOf512());
            }
            // Put the strength back to the shipped default for whatever runs after.
            strengthSlider512.value = '75';
            strengthSlider512.dispatchEvent(new Event('input'));
            strengthSlider512.dispatchEvent(new Event('change'));
            await sleep(150);
          }
        }
      }
      search514.value = '';
      search514.dispatchEvent(new Event('input'));
      await sleep(150);
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // F22: a modifier-held drag hands the prompt to the OS; a plain drag has to stay
    // the tile reorder it always was, and the switch has to take the whole thing away.
    const gridTile = (id) => [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
    const press = (el, opts) => el.dispatchEvent(new PointerEvent('pointerdown', Object.assign(
      { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1, clientX: 20, clientY: 20 }, opts)));
    // Cancel, not release: a pointerup on a tile would copy and pollute the call log.
    const abort = () => window.dispatchEvent(new PointerEvent('pointercancel', { bubbles: true, pointerId: 1 }));
    const fireDragStart = (el) => {
      const ev = new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: new DataTransfer() });
      el.dispatchEvent(ev);
      return ev;
    };
    const setExpertFlag = async (label, on) => {
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const row = await expertRow(label);
      const sw = row && row.querySelector('input.switch');
      if (sw && sw.checked !== on) { sw.click(); await sleep(250); }
      const tab0 = document.querySelector('#expert-tabs-bar .expert-tab');
      if (tab0) { tab0.click(); await sleep(150); }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
      return !!row;
    };
    // Changing the Markdown-sync folder must ASK what happens to the old one, with
    // three real choices, and hand the answer to the backend as an explicit
    // instruction. Cancel must leave the folder exactly where it was.
    try {
      const openExpert = async () => {
        document.getElementById('settings').classList.remove('hidden');
        await click('expert-open');
      };
      const closeExpert = async () => {
        const tab0 = document.querySelector('#expert-tabs-bar .expert-tab');
        if (tab0) { tab0.click(); await sleep(150); }
        await click('expert-back');
        document.getElementById('settings').classList.add('hidden');
      };
      const syncPathRow = async () => {
        const hit = () => [...document.querySelectorAll('#expert-flags .path-field')]
          .find(r => ((r.querySelector('span') || {}).textContent || '').includes('Sync folder'));
        if (hit()) return hit();
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          await sleep(200);
          if (hit()) return hit();
        }
        return null;
      };
      // The switch warns about plain text first; accept it, that is not this test.
      await openExpert();
      const syncSwitch = await expertRow('Sync prompts as Markdown');
      if (!syncSwitch) {
        out.errors.push('the Markdown-sync switch is missing from the expert menu');
      } else {
        const sw = syncSwitch.querySelector('input.switch');
        if (!sw.checked) {
          sw.click();
          await sleep(200);
          const warn = document.getElementById('confirm-modal');
          if (!warn.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
        }
        const row = await syncPathRow();
        if (!row) {
          out.errors.push('the sync folder row does not appear once Markdown sync is on');
        } else {
          if (!row.querySelector('.path-sync-stats')) {
            out.errors.push('the sync folder row shows no status line');
          }
          // The picker is stubbed, so each round can name its own target folder;
          // the row is looked up again every time because the menu redraws itself
          // after a change.
          const askFor = async (folder) => {
            window.__PICK_FOLDER__ = folder;
            const fresh = await syncPathRow();
            const btn = fresh && fresh.querySelector('.path-btns button');
            if (!btn) return null;
            btn.click();
            await sleep(300);
            return document.getElementById('confirm-modal');
          };
          const answer = async (folder, buttonId) => {
            window.__SYNC_DIR_CALL__ = null;
            const cm = await askFor(folder);
            if (!cm) {
              out.errors.push('the sync folder row lost its change button');
              return null;
            }
            if (cm.classList.contains('hidden')) {
              out.errors.push('changing the sync folder did not ask about the old folder');
              return null;
            }
            const alt = document.getElementById('confirm-alt');
            const cancel = document.getElementById('confirm-cancel');
            const ok = document.getElementById('confirm-ok');
            for (const b of [alt, cancel, ok]) {
              if (b.classList.contains('hidden') || !b.textContent.trim()) {
                out.errors.push('the old-folder question is missing one of its three buttons: ' + b.id);
              }
            }
            if (!cm.textContent.includes('3')) {
              out.errors.push('the old-folder question does not say how many files are in there');
            }
            await click(buttonId);
            await sleep(300);
            return window.__SYNC_DIR_CALL__;
          };
          const deleted = await answer('D:/SyncA', 'confirm-ok');
          if (!deleted || deleted.dir !== 'D:/SyncA' || deleted.deleteOld !== true) {
            out.errors.push('choosing to delete the old folder did not reach the backend as such: ' + JSON.stringify(deleted));
          }
          const kept = await answer('D:/SyncB', 'confirm-alt');
          if (!kept || kept.dir !== 'D:/SyncB' || kept.deleteOld !== false) {
            out.errors.push('choosing to keep the old folder did not reach the backend as such: ' + JSON.stringify(kept));
          }
          const cancelled = await answer('D:/SyncC', 'confirm-cancel');
          if (cancelled) {
            out.errors.push('cancelling the old-folder question still switched the folder: ' + JSON.stringify(cancelled));
          }
          const shown = ((await syncPathRow()).querySelector('.path-out') || {}).textContent || '';
          if (!shown.includes('SyncB')) {
            out.errors.push('after cancelling, the folder shown is not the last one actually chosen: ' + shown);
          }
          // ---- 503: the backend refusing the switch outright (never actually
          // picked through the dialog) must not leave the new folder showing,
          // and must say so instead of the unrelated "old folder kept" text ----
          document.getElementById('toast').classList.add('hidden');
          window.__SYNC_DIR_REFUSE__ = true;
          window.__SYNC_DIR_CALL__ = null;
          const refusedCm = await askFor('D:/SyncRefused');
          if (refusedCm && !refusedCm.classList.contains('hidden')) {
            await click('confirm-ok');
            await sleep(300);
          }
          window.__SYNC_DIR_REFUSE__ = false;
          if (window.__SYNC_DIR_CALL__) {
            out.errors.push('503: a refused sync-dir switch still reached the backend as a success');
          }
          const keptModal = document.getElementById('confirm-modal');
          if (keptModal && !keptModal.classList.contains('hidden')) {
            out.errors.push('503: a refused switch showed the old-folder-kept dialog meant for the delete-failure case');
            keptModal.classList.add('hidden');
          }
          const refusedToast = document.getElementById('toast');
          if (refusedToast.classList.contains('hidden') || !refusedToast.textContent.includes('The folder was not applied')) {
            out.errors.push(`503: the refusal did not show its own message, toast reads ${JSON.stringify(refusedToast.textContent)}`);
          }
          const shownAfterRefusal = ((await syncPathRow()).querySelector('.path-out') || {}).textContent || '';
          if (!shownAfterRefusal.includes('SyncB') || shownAfterRefusal.includes('SyncRefused')) {
            out.errors.push('503: after a refusal the row must keep showing the folder still actually in effect: ' + shownAfterRefusal);
          }
        }
        // Back to the shipped default for everything after this.
        const off = await expertRow('Sync prompts as Markdown');
        const offSw = off && off.querySelector('input.switch');
        if (offSw && offSw.checked) { offSw.click(); await sleep(200); }
        window.__PICK_FOLDER__ = null;
      }
      await closeExpert();
    } catch (e) {
      // Whatever went wrong here, the rest of the run must still find the app in
      // the state it expects: no dialog on screen, settings closed.
      out.errors.push('the sync folder walk broke off: ' + (e && (e.stack || e.message) || e));
      document.getElementById('confirm-modal').classList.add('hidden');
      document.getElementById('settings').classList.add('hidden');
    }

    // ---- 503: the "Shortcuts only" keys-overlay tooltip names every key the
    // recorder actually keeps, not just the original shorter list ----
    try {
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      // The sub-switch only shows while the keys overlay itself is on, and that
      // ships off: switch it on through its own row first.
      const findKeysRow = () => [...document.querySelectorAll('#expert-flags .switch-field')]
        .find(r => ((r.querySelector('span') || {}).textContent || '').includes('Show pressed keys'));
      let keysRow = findKeysRow();
      for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
        if (keysRow) break;
        tb.click();
        await sleep(150);
        keysRow = findKeysRow();
      }
      const keysBox = keysRow && keysRow.querySelector('input.switch');
      if (keysBox && !keysBox.checked) {
        keysBox.checked = true;
        keysBox.dispatchEvent(new Event('change'));
        await sleep(250);
      }
      const findRow = () => [...document.querySelectorAll('#expert-flags .switch-field')]
        .find(r => ((r.querySelector('span') || {}).textContent || '').includes('Shortcuts only'));
      let shortcutsRow = findRow();
      for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
        if (shortcutsRow) break;
        tb.click();
        await sleep(150);
        shortcutsRow = findRow();
      }
      if (!shortcutsRow) {
        out.errors.push('503: the "Shortcuts only" keys-overlay row is missing from the expert menu');
      } else {
        const tip = shortcutsRow.title || '';
        for (const word of ['Backspace', 'Delete', 'Insert', 'Home', 'End', 'Page Up', 'Page Down']) {
          if (!tip.includes(word)) {
            out.errors.push(`503: the "Shortcuts only" tooltip is missing "${word}", got ${JSON.stringify(tip)}`);
          }
        }
      }
      const tab0b = document.querySelector('#expert-tabs-bar .expert-tab');
      if (tab0b) { tab0b.click(); await sleep(100); }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    } catch (e) {
      out.errors.push('503: the shortcuts-only tooltip check broke off: ' + (e && (e.stack || e.message) || e));
      document.getElementById('settings').classList.add('hidden');
    }

    // 11: grid bulk selection — checkbox click must never fall through to the
    // tile's own copy/drag handling (same .tile-select guard as .tile-menu).
    {
      const calls = () => (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []);
      const t4 = gridTile('p4'), t5 = gridTile('p5');
      if (!t4 || !t5) {
        out.errors.push('the grid multi-select test needs p4 and p5 on the grid');
      } else {
        calls().length = 0;
        t4.querySelector('.tile-select').click();
        await sleep(120);
        const batchBar = document.getElementById('grid-batch-bar');
        const batchCount = document.getElementById('grid-batch-count');
        if (!t4.classList.contains('grid-selected')) out.errors.push('clicking the checkbox did not select the tile');
        if (!document.body.classList.contains('grid-selecting')) out.errors.push('selecting a tile did not enter grid-selecting mode');
        if (batchBar.classList.contains('hidden')) out.errors.push('the grid batch bar stayed hidden with a tile selected');
        if (!batchCount.textContent.trim().startsWith('1')) {
          out.errors.push('the batch count reads "' + batchCount.textContent + '" instead of 1');
        }
        if (calls().includes('copy_prompt') || calls().includes('copy_text')) {
          out.errors.push('the checkbox click also copied the prompt');
        }
        t5.querySelector('.tile-select').click();
        await sleep(120);
        if (!batchCount.textContent.trim().startsWith('2')) out.errors.push('selecting a second tile did not update the count to 2');
        // 26: with a third tile selected the composer folds away and the grid
        // must not clip its own bottom row under the batch bar / banner above it.
        {
          const composer = document.querySelector('.composer');
          const composerWasVisible = composer && getComputedStyle(composer).display !== 'none';
          const t6 = gridTile('p6');
          t6?.querySelector('.tile-select').click();
          await sleep(150);
          if (!batchCount.textContent.trim().startsWith('3')) out.errors.push('selecting a third tile did not update the count to 3');
          if (composer && getComputedStyle(composer).display !== 'none') {
            out.errors.push('26: the composer did not fold away while a grid selection is active');
          }
          const wrapRect = document.querySelector('.grid-wrap').getBoundingClientRect();
          const overflowing = [...document.querySelectorAll('#grid .tile')]
            .filter((tile) => tile.getBoundingClientRect().bottom > wrapRect.bottom + 1);
          if (overflowing.length) {
            out.errors.push('26: ' + overflowing.length + ' tile(s) run past the bottom of .grid-wrap while selecting');
          }
          t6?.querySelector('.tile-select').click(); // deselect again, back to 2
          await sleep(150);
          if (composerWasVisible && composer && getComputedStyle(composer).display === 'none') {
            out.errors.push('26: the composer did not come back once the selection dropped below 3');
          }
        }
        // Recolor both, then confirm the change actually reached the tiles. 28:
        // the batch bar now uses the same free-color swatches as the editor
        // (.swatch, not .lib-color) — skip "none" and the free-pick swatch.
        const swatch = document.querySelector('#grid-batch-colors .swatch:not(.none):not(.custom)');
        if (!swatch) {
          out.errors.push('the grid batch bar has no colour swatches');
        } else {
          // Already normalised to rgb() by the browser, same as the tile's own style.
          const wantColor = swatch.style.background;
          swatch.click();
          await sleep(150);
          if (document.body.classList.contains('grid-selecting')) out.errors.push('recolouring did not clear the selection');
          if (!batchBar.classList.contains('hidden')) out.errors.push('the batch bar stayed open after recolouring');
          const t4After = gridTile('p4');
          if (!t4After || t4After.style.background !== wantColor) {
            out.errors.push('the recolour did not reach the tile: ' + (t4After && t4After.style.background));
          }
        }
        // 21: the delete confirms on the button — one press arms it (red, count
        // named), the second really deletes. No dialog is involved any more.
        gridTile('p4').querySelector('.tile-select').click();
        await sleep(120);
        // 27: the "move to view" select opens the same themed popup as every other
        // select instead of the native OS dropdown list, and matches its ghost-btn
        // neighbours in size.
        {
          const viewSel = document.getElementById('grid-batch-view');
          const neighbour = document.getElementById('grid-batch-move');
          const vr = viewSel.getBoundingClientRect(), nr = neighbour.getBoundingClientRect();
          if (Math.abs(vr.height - nr.height) > 1) {
            out.errors.push('27: the view select (' + vr.height + 'px) does not match its neighbours (' + nr.height + 'px)');
          }
          viewSel.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
          await sleep(120);
          const pop = document.querySelector('.select-pop');
          if (!pop || pop.classList.contains('hidden')) {
            out.errors.push('27: clicking the view select did not open the themed popup');
          } else {
            const rows = [...pop.querySelectorAll('button')];
            if (rows.length !== viewSel.options.length) {
              out.errors.push('27: the view popup lists ' + rows.length + ' entries, the select has ' + viewSel.options.length);
            }
            const wantId = viewSel.options[viewSel.options.length - 1].value;
            rows[rows.length - 1].dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
            await sleep(120);
            if (viewSel.value !== wantId) out.errors.push('27: picking a popup entry did not update the select value');
            if (!document.querySelector('.select-pop').classList.contains('hidden')) {
              out.errors.push('27: the view popup stayed open after picking an entry');
            }
          }
        }
        // 28: the free-color swatch opens the same picker as the editor and
        // applies a custom hex to every selected tile.
        {
          const customSwatch = document.querySelector('#grid-batch-colors .swatch.custom');
          if (!customSwatch) {
            out.errors.push('28: the grid batch bar has no free-color swatch');
          } else {
            customSwatch.click();
            await sleep(120);
            const pop = document.querySelector('.color-pop');
            if (!pop || pop.classList.contains('hidden')) {
              out.errors.push('28: the free-color swatch did not open the color picker');
            } else {
              const hexInput = pop.querySelector('.cp-hex');
              const probe = document.createElement('div');
              probe.style.background = '#336699';
              const wantColor = probe.style.background;
              hexInput.value = '336699';
              hexInput.dispatchEvent(new Event('change', { bubbles: true }));
              // The picker debounces its backend commit (drag frames must not
              // spam a write each), so give it longer than that window.
              await sleep(400);
              const t4Custom = gridTile('p4');
              if (!t4Custom || t4Custom.style.background !== wantColor) {
                out.errors.push('28: picking a custom hex did not reach the tile: ' + (t4Custom && t4Custom.style.background));
              }
              if (document.body.classList.contains('grid-selecting')) {
                out.errors.push('28: picking a custom hex did not clear the selection');
              }
            }
          }
        }
        // The grid's selection bar follows the library's action order, and the
        // shared tag action runs through the stub for every selected tile.
        {
          gridTile('p4').querySelector('.tile-select').click();
          gridTile('p5').querySelector('.tile-select').click();
          await sleep(150);
          const order = ['grid-batch-all', 'grid-batch-none', 'grid-batch-fav', 'grid-batch-unfav', 'grid-batch-tag-add',
            'grid-batch-tag-remove', 'grid-batch-archive', 'grid-batch-move', 'grid-batch-export', 'grid-batch-delete'];
          const seen = [...batchBar.querySelectorAll('button')]
            .filter((b) => b.id && order.includes(b.id) && getComputedStyle(b).display !== 'none')
            .map((b) => b.id);
          const want = order.filter((id) => seen.includes(id));
          if (seen.join() !== want.join()) out.errors.push('the grid batch bar is out of order: ' + seen.join(' '));
          for (const id of ['grid-batch-all', 'grid-batch-none', 'grid-batch-move', 'grid-batch-export', 'grid-batch-delete']) {
            if (!seen.includes(id)) out.errors.push('the grid batch bar lacks ' + id);
          }
          window.__SMOKE_ARGS__ = [];
          await click('grid-batch-tag-add');
          await sleep(150);
          document.getElementById('confirm-input').value = 'Smoke666';
          document.getElementById('confirm-ok').click();
          await sleep(250);
          const tagCall = window.__SMOKE_ARGS__.find(([cmd, args]) => cmd === 'batch_prompts' && args && args.action === 'tag-add');
          if (!tagCall || tagCall[1].tag !== 'Smoke666' || !tagCall[1].ids.includes('p4') || !tagCall[1].ids.includes('p5')) {
            out.errors.push('the grid Add tag did not send the tag for both selected tiles');
          }
          if (document.body.classList.contains('grid-selecting')) out.errors.push('the grid selection stayed after Add tag');
        }
        // Re-select for the delete-arm test below — the custom-color commit
        // above clears the selection same as every other batch action.
        gridTile('p4').querySelector('.tile-select').click();
        await sleep(120);
        const delBtn = document.getElementById('grid-batch-delete');
        await click('grid-batch-delete');
        await sleep(150);
        if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
          out.errors.push('the batch delete still opens a confirm dialog');
        }
        if (!delBtn.classList.contains('confirm') || !delBtn.textContent.includes('1')) {
          out.errors.push('the first press did not arm the delete button: ' + delBtn.textContent);
        }
        // 29: the warning triangle must sit on the label's vertical centre, not
        // its text baseline.
        {
          const svgEl = delBtn.querySelector('svg');
          const svgRect = svgEl.getBoundingClientRect();
          const textNode = Array.from(delBtn.childNodes).find((n) => n.nodeType === 3 && n.textContent.trim());
          const range = document.createRange();
          range.selectNodeContents(textNode);
          const textRect = range.getBoundingClientRect();
          const diff = Math.abs((svgRect.top + svgRect.height / 2) - (textRect.top + textRect.height / 2));
          if (diff >= 1) out.errors.push('29: warning triangle is off the label centre by ' + diff.toFixed(2) + 'px');
        }
        if (!gridTile('p4')) out.errors.push('the first press already deleted the tile');
        window.__SMOKE_ARGS__ = [];
        await click('grid-batch-delete');
        await sleep(200);
        if (gridTile('p4')) out.errors.push('the second press did not delete the tile');
        // The placement snapshot is taken before the delete, and the undo hands it back.
        {
          const names = window.__SMOKE_ARGS__.map(([cmd]) => cmd);
          const snapAt = names.indexOf('placement_snapshot');
          const delAt = names.indexOf('batch_prompts');
          if (snapAt < 0 || delAt < 0 || snapAt > delAt) {
            out.errors.push('the grid batch delete did not snapshot the placements before deleting: ' + names.join(','));
          }
          const undoBtn = document.getElementById('toast').querySelector('.toast-btn');
          if (!undoBtn) {
            out.errors.push('the grid batch delete offered no undo toast');
          } else {
            window.__SMOKE_ARGS__ = [];
            undoBtn.click();
            await sleep(300);
            if (!window.__SMOKE_ARGS__.some(([cmd, args]) => cmd === 'restore_prompts_batch' && args && args.placements && args.placements[0] && args.placements[0].id === 'p4')) {
              out.errors.push('undoing the grid batch delete did not send the placement snapshot');
            }
          }
        }
        if (delBtn.classList.contains('confirm')) out.errors.push('the delete button stayed armed after deleting');
      }
    }
    // 91: trash — the dialog opened from the expert menu's Data group. Restore,
    // single purge and "Clear all" (empty_trash) each get their own target so all
    // three new commands are exercised, not just the happy path of one.
    {
      const expertAction = async (label) => {
        const hit = () => [...document.querySelectorAll('.expert-action')].find(b => b.textContent === label);
        if (hit()) return hit();
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          await sleep(200);
          if (hit()) return hit();
        }
        return null;
      };
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const trashBtn = await expertAction('Trash');
      if (!trashBtn) {
        out.errors.push('the Trash action is missing from the expert menu');
      } else {
        trashBtn.click();
        await sleep(200);
        const trashModal = document.getElementById('trash-modal');
        const trashBody = document.getElementById('trash-body');
        const rows = () => [...trashBody.querySelectorAll('.clip-row')];
        if (trashModal.classList.contains('hidden')) {
          out.errors.push('clicking the Trash action did not open the trash dialog');
        } else if (rows().length !== 3) {
          out.errors.push('the trash dialog shows ' + rows().length + ' rows instead of 3');
        } else {
          const t1Row = rows().find(r => r.textContent.includes('Deleted idea'));
          if (!t1Row) {
            out.errors.push('the trash dialog is missing "Deleted idea"');
          } else {
            t1Row.querySelectorAll('button')[0].click(); // Restore
            await sleep(200);
            if (rows().length !== 2) out.errors.push('restoring left ' + rows().length + ' rows instead of 2');
          }
          const t2Row = rows().find(r => r.textContent.includes('Second trashed prompt'));
          if (!t2Row) {
            out.errors.push('the trash dialog is missing the anonymous trashed prompt');
          } else {
            t2Row.querySelectorAll('button')[1].click(); // Delete forever
            await sleep(150);
            const cm = document.getElementById('confirm-modal');
            if (cm.classList.contains('hidden')) {
              out.errors.push('purging a single trash item skipped the confirm dialog');
            } else {
              await click('confirm-ok');
              await sleep(200);
              if (rows().length !== 1) out.errors.push('purging left ' + rows().length + ' rows instead of 1');
            }
          }
          // 230: .click() alone does not move keyboard focus the way a real
          // pointer click does -- focus explicitly so confirmDialog captures a
          // real opener to test its focus-restore behaviour against below.
          document.getElementById('trash-clear').focus();
          await click('trash-clear');
          await sleep(150);
          const cm2 = document.getElementById('confirm-modal');
          if (cm2.classList.contains('hidden')) {
            out.errors.push('emptying the trash skipped the confirm dialog');
          } else {
            const clearBtnBeforeConfirm = document.getElementById('trash-clear');
            await click('confirm-ok');
            await sleep(200);
            if (!trashBody.textContent.includes('Trash is empty')) {
              out.errors.push('emptying the trash left the dialog without the empty-state message');
            }
            // 230: confirmDialog restores focus to whatever opened it (here, the
            // "Clear all" button that called emptyTrashAll -> confirmDialog)
            // instead of dropping it to document.body once the dialog resolves.
            if (document.activeElement !== clearBtnBeforeConfirm) {
              const got = document.activeElement && (document.activeElement.id || document.activeElement.tagName);
              out.errors.push('230: confirming the empty-trash dialog did not return focus to the button that opened it, landed on ' + got);
            }
          }
        }
        // 230: the trash modal is one of the ~20 dialogs newly covered by the
        // global Tab-trap dispatcher this round (it had no trap of its own before).
        // Tab pressed on the LAST focusable element must wrap back to the FIRST
        // instead of leaving focus stuck (the previous, untrapped behaviour) or
        // escaping into the dimmed page behind the dialog.
        {
          // The Tab-trap dispatcher traps whatever overlay is topmost, in a fixed
          // cascade -- an earlier, unrelated test (28: the free-color swatch) left
          // the colour-pop popover open (typing a hex never closes it) and it sits
          // ahead of the trash modal in that cascade, which would otherwise trap
          // Tab there instead. Clean test-owned leftover state before asserting.
          document.querySelectorAll('.color-pop:not(.hidden)').forEach((el) => el.classList.add('hidden'));
          const trashCloseBtn = document.getElementById('trash-close');
          const trashClearBtn = document.getElementById('trash-clear');
          if (trashModal.classList.contains('hidden') || !trashCloseBtn || !trashClearBtn) {
            out.errors.push('230: the trash modal is not in a state to test the Tab trap');
          } else {
            trashCloseBtn.focus();
            if (document.activeElement !== trashCloseBtn) {
              out.errors.push('230: could not focus the trash modal close button to test the Tab trap');
            } else {
              trashCloseBtn.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));
              if (document.activeElement !== trashClearBtn) {
                const ae = document.activeElement;
                const got = ae ? (ae.outerHTML || '').slice(0, 200) : String(ae);
                out.errors.push('230: Tab from the last focusable element in the trash modal did not wrap to the first, landed on ' + got);
              }
            }
          }
        }
        document.getElementById('trash-close').click();
        await sleep(100);
      }
      const firstTab = document.querySelector('#expert-tabs-bar .expert-tab');
      if (firstTab) { firstTab.click(); await sleep(150); }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // 51: prompt in multiple views — off by default. On exposes "Also show in…" on
    // the tile menu, which places/unplaces the SAME prompt (shared data, no copy)
    // in another view via addView/removeView.
    {
      const gotFlag = await setExpertFlag('in multiple views', true);
      if (!gotFlag) {
        out.errors.push('the "Prompt in multiple views" expert toggle is missing');
      } else {
        const t0 = gridTile('p0');
        if (!t0) {
          out.errors.push('the multi-view test needs p0 on the grid');
        } else {
          t0.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
          await sleep(150);
          const viewsBtn = document.querySelector('#ctx [data-act="views"]');
          if (!viewsBtn || viewsBtn.classList.contains('hidden')) {
            out.errors.push('the "Also show in…" tile action did not appear with the flag on');
          } else {
            viewsBtn.click();
            await sleep(150);
            const popRows = () => [...document.querySelectorAll('.views-pop .view-row')];
            if (popRows().length !== 3) {
              out.errors.push('the "also show in" popover shows ' + popRows().length + ' rows instead of 3');
            } else {
              const workRow = popRows().find(r => r.textContent.includes('Work'));
              if (!workRow || workRow.classList.contains('on')) {
                out.errors.push('p0 shows as already placed in Work before toggling it on');
              } else {
                window.__SMOKE_ARGS__ = [];
                workRow.click();
                await sleep(150);
                const addCall = (window.__SMOKE_ARGS__ || []).find(c => c[0] === 'batch_prompts');
                if (!addCall || addCall[1].action !== 'addView' || addCall[1].viewId !== 'work') {
                  out.errors.push('toggling a view row did not call batch_prompts with addView/work');
                }
                const workRowAfter = popRows().find(r => r.textContent.includes('Work'));
                if (!workRowAfter || !workRowAfter.classList.contains('on')) {
                  out.errors.push('the popover did not show Work as placed after adding it');
                } else {
                  window.__SMOKE_ARGS__ = [];
                  workRowAfter.click(); // toggle back off — leaves the fixture clean
                  await sleep(150);
                  const removeCall = (window.__SMOKE_ARGS__ || []).find(c => c[0] === 'batch_prompts');
                  if (!removeCall || removeCall[1].action !== 'removeView') {
                    out.errors.push('toggling the row back off did not call batch_prompts with removeView');
                  }
                }
              }
            }
            document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
            await sleep(100);
          }
        }
      }
      // Off again: the tile action must disappear, restoring the "one placement" guarantee.
      const restored = await setExpertFlag('in multiple views', false);
      if (restored) {
        const t0b = gridTile('p0');
        if (t0b) {
          t0b.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
          await sleep(150);
          const viewsBtnOff = document.querySelector('#ctx [data-act="views"]');
          if (!viewsBtnOff || !viewsBtnOff.classList.contains('hidden')) {
            out.errors.push('the "Also show in…" tile action stayed visible with the flag off');
          }
          document.body.click();
          await sleep(100);
        }
      }
    }
    // Pinning a floating button from the tile menu must rename the entry right away.
    // Nothing refetches the app state after that click, so a label built from a stale
    // cache would keep offering "Show" for a prompt that is already pinned — and the
    // next click would then do the opposite of what the entry says.
    {
      const openPin = () => {
        const tile = gridTile('p0');
        if (!tile) return null;
        tile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
        return document.querySelector('#ctx [data-act="pin"]');
      };
      const pin = openPin();
      await sleep(150);
      if (!pin || pin.classList.contains('hidden')) {
        out.errors.push('the floating-button action is missing from the tile menu');
      } else if (pin.textContent.trim() !== 'Show floating button') {
        out.errors.push('an unpinned prompt offers "' + pin.textContent.trim() + '" instead of pinning');
      } else {
        window.__SMOKE_CALLS__ = [];
        pin.click();
        await sleep(200);
        const refetched = (window.__SMOKE_CALLS__ || []).some(c => c === 'get_state' || c === 'get_settings');
        const pinned = openPin();
        await sleep(150);
        if (!pinned || pinned.textContent.trim() !== 'Remove floating button') {
          out.errors.push('after pinning, the tile menu still offers "' +
            (pinned ? pinned.textContent.trim() : 'nothing') + '"');
        }
        if (refetched) {
          out.errors.push('pinning refetched the app state, so this check cannot prove the label came from the answer');
        }
        // Unpin again, so the rest of the run starts from the untouched fixture.
        const back = document.querySelector('#ctx [data-act="pin"]');
        if (back) { back.click(); await sleep(200); }
        const off = openPin();
        await sleep(150);
        if (off && off.textContent.trim() !== 'Show floating button') {
          out.errors.push('after unpinning, the tile menu still offers "' + off.textContent.trim() + '"');
        }
        document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
        await sleep(100);
      }
    }
    // 24: grid tag filter — the header toolbar button opens a popover of tag chips;
    // picking one dims (never hides) tiles that do not match, so the grid geometry
    // and drag targets stay exactly as they were.
    {
      const tagBtn = document.getElementById('grid-tag-btn');
      if (!tagBtn || getComputedStyle(tagBtn).display === 'none') {
        out.errors.push('the grid tag filter button is missing');
      } else {
        tagBtn.click();
        await sleep(200);
        const pop = document.querySelector('.grid-tag-pop');
        const chips = () => pop ? [...pop.querySelectorAll('.tag-chip.tag-filter')] : [];
        if (!pop || pop.classList.contains('hidden') || chips().length < 2) {
          out.errors.push('the grid tag popover shows ' + chips().length + ' chips instead of at least 2');
        } else {
          // 81: chips line up in a fixed-column grid instead of free-wrapping.
          const tracks = getComputedStyle(pop).gridTemplateColumns.trim().split(/[ ]+/);
          if (tracks.length < 2) out.errors.push('81: the tag popover is not laid out in at least two columns');
          const draftChip = chips().find(c => (c.querySelector('.tag-chip-label') || {}).textContent === 'Draft');
          if (!draftChip) {
            out.errors.push('the grid tag popover is missing the "Draft" chip');
          } else {
            draftChip.click();
            await sleep(200);
            if (!tagBtn.classList.contains('active')) out.errors.push('picking a tag did not mark the toolbar button active');
            const t0 = gridTile('p0'); // ["Work"] only — must dim
            const t1 = gridTile('p1'); // ["Draft", "Work"] — must stay full
            if (!t0 || !t0.classList.contains('tile-dimmed')) out.errors.push('a non-matching tile (p0) was not dimmed');
            if (!t1 || t1.classList.contains('tile-dimmed')) out.errors.push('a matching tile (p1) was dimmed');
            if (t0 && getComputedStyle(t0).display === 'none') out.errors.push('a dimmed tile was hidden instead of dimmed');
            // Clear filter restores every tile and the button's active state.
            const clearBtn = [...pop.querySelectorAll('.tag-action')].find(b => b.textContent === 'Clear filter');
            if (clearBtn) {
              // Full width minus the popover's own left+right padding (10px each).
              const pr = pop.getBoundingClientRect(), cr = clearBtn.getBoundingClientRect();
              if (cr.width < pr.width - 30) {
                out.errors.push('81: Clear filter does not span the full popover width');
              }
            }
            clearBtn?.click();
            await sleep(200);
            if (tagBtn.classList.contains('active')) out.errors.push('clearing the filter left the toolbar button active');
            const t0After = gridTile('p0');
            if (t0After && t0After.classList.contains('tile-dimmed')) out.errors.push('clearing the filter left a tile dimmed');
          }
        }
        document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
        await sleep(100);
      }
    }
    // 88: import preview — shows new vs. duplicate before anything is imported.
    // Cancel only here (not confirm): confirming would push fake prompts into
    // the shared fixture and could throw off every check after this one in the
    // same probe run.
    {
      const importBtn = document.getElementById('import-btn');
      if (!importBtn) {
        out.errors.push('the settings page has no import button');
      } else {
        importBtn.click();
        await sleep(300);
        const modal = document.getElementById('import-preview-modal');
        if (!modal || modal.classList.contains('hidden')) {
          out.errors.push('the import preview did not open');
        } else {
          const rows = [...document.querySelectorAll('#import-preview-list .import-preview-row')];
          if (rows.length !== 3) out.errors.push('the import preview lists ' + rows.length + ' rows, expected 3');
          const dupRows = rows.filter(r => r.classList.contains('dup'));
          if (dupRows.length !== 1) out.errors.push('the import preview marks ' + dupRows.length + ' rows as duplicate, expected 1');
          if (dupRows.length && !dupRows[0].querySelector('.import-preview-badge')) {
            out.errors.push('a duplicate row has no badge');
          }
          const confirmBtn = document.getElementById('import-preview-confirm');
          if (confirmBtn.disabled) out.errors.push('the import preview confirm button is disabled although 2 items are new');
          document.getElementById('import-preview-cancel').click();
          await sleep(200);
          if (!modal.classList.contains('hidden')) out.errors.push('canceling the import preview did not close it');
        }
      }
    }
    // 446: import from a folder -- tags/size/a checkbox per row, an unreadable
    // file grayed out with its reason, and the "off by default" checkbox is
    // never touched here (cancel only, same reasoning as the import preview above).
    {
      const folderBtn = document.getElementById('import-folder-btn');
      if (!folderBtn) {
        out.errors.push('the settings page has no import-from-folder button');
      } else {
        folderBtn.click();
        await sleep(300);
        const modal = document.getElementById('import-folder-modal');
        if (!modal || modal.classList.contains('hidden')) {
          out.errors.push('the folder import preview did not open');
        } else {
          // 521: the dialog gets a real title instead of Windows' generic chrome.
          const folderCalls521 = (window.__SMOKE_ARGS__ || [])
            .filter(([cmd]) => cmd === 'preview_import_folder');
          const lastCall521 = folderCalls521[folderCalls521.length - 1];
          if (!lastCall521 || !lastCall521[1] || !lastCall521[1].title) {
            out.errors.push('521: preview_import_folder was not called with a title');
          }
          const rows = [...document.querySelectorAll('#import-folder-list .import-preview-row')];
          if (rows.length !== 3) out.errors.push('the folder import preview lists ' + rows.length + ' rows, expected 3');
          const badRows = rows.filter(r => r.classList.contains('dup'));
          if (badRows.length !== 1) out.errors.push('the folder import preview marks ' + badRows.length + ' rows unreadable, expected 1');
          if (badRows.length && badRows[0].querySelector('input[type=checkbox]')) {
            out.errors.push('an unreadable folder-import row still has a checkbox');
          }
          if (badRows.length && !badRows[0].querySelector('.import-preview-badge')) {
            out.errors.push('an unreadable folder-import row has no reason badge');
          }
          const goodRows = rows.filter(r => !r.classList.contains('dup'));
          const checked = goodRows.filter(r => (r.querySelector('input[type=checkbox]') || {}).checked);
          if (checked.length !== goodRows.length) out.errors.push('a readable folder-import row does not start checked');
          const tagged = goodRows.find(r => (r.querySelector('.import-preview-name') || {}).textContent === 'Meeting');
          if (!tagged || !tagged.querySelector('.import-preview-badge')) {
            out.errors.push('a folder-import row from a subfolder shows no tag badge');
          }
          document.getElementById('import-folder-select-none').click();
          await sleep(100);
          const confirmBtn = document.getElementById('import-folder-confirm');
          if (!confirmBtn.disabled) out.errors.push('the folder import confirm button stays enabled with none selected');
          document.getElementById('import-folder-select-all').click();
          await sleep(100);
          if (confirmBtn.disabled) out.errors.push('the folder import confirm button stays disabled after selecting all');
          document.getElementById('import-folder-cancel').click();
          await sleep(200);
          if (!modal.classList.contains('hidden')) out.errors.push('canceling the folder import did not close it');
        }
      }
    }
    // 339: settings export control -- one "Export" button plus a CSV/TXT/JSON
    // format select (replaces the old two hard-coded CSV/TXT buttons).
    {
      const fmtSel = document.getElementById('export-format');
      const expBtn = document.getElementById('export-btn');
      if (!fmtSel || !expBtn) {
        out.errors.push('the settings export control (format select + button) is missing');
      } else {
        const optValues = [...fmtSel.options].map(o => o.value);
        if (!['csv', 'txt', 'json'].every(v => optValues.includes(v))) {
          out.errors.push('the export format select is missing csv/txt/json, has: ' + optValues.join(','));
        }
        // 371: the format select must precede the Export button (library's
        // batch row has it in this order already) -- neither element sets its
        // own tabindex, so document order is also tab order here.
        const posFmtExp = fmtSel.compareDocumentPosition(expBtn);
        if (!(posFmtExp & Node.DOCUMENT_POSITION_FOLLOWING)) {
          out.errors.push('the export format select does not precede the Export button');
        }
        if (fmtSel.hasAttribute('tabindex') || expBtn.hasAttribute('tabindex')) {
          out.errors.push('an explicit tabindex on the format select or Export button would override document-order tabbing');
        }
        window.__EXPORT_CALL__ = null;
        fmtSel.value = 'json';
        fmtSel.dispatchEvent(new Event('change', { bubbles: true }));
        expBtn.click();
        await sleep(200);
        if (!window.__EXPORT_CALL__ || window.__EXPORT_CALL__.format !== 'json') {
          out.errors.push('choosing JSON and clicking Export did not call export_prompts with format "json"');
        }
      }
    }
    // 340: recordingDir shows the folder's video count/size plus a cleanup
    // button; the button opens a modal that previews what an age tier would
    // delete and only deletes after Delete is clicked -- Cancel must leave
    // the files alone.
    {
      const pathFieldRow = async (label) => {
        const hit = () => [...document.querySelectorAll('.path-field')]
          .find(r => ((r.querySelector('.value-head span') || {}).textContent || '') === label);
        if (hit()) return hit();
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          await sleep(200);
          if (hit()) return hit();
        }
        return null;
      };
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const row = await pathFieldRow('Recording folder');
      if (!row) {
        out.errors.push('the recording-folder path row is missing from the expert menu');
      } else {
        await sleep(150); // refreshVideoFolderInfo() resolves and re-renders
        const statsEl = row.querySelector('.path-video-stats span');
        if (!statsEl || statsEl.textContent !== '2 files, 5.0 MB') {
          out.errors.push('the recording folder stats line reads "' + (statsEl && statsEl.textContent) + '"');
        }
        const cleanBtn = row.querySelector('.path-video-stats button');
        if (!cleanBtn || cleanBtn.disabled) {
          out.errors.push('the video cleanup button is missing or disabled although the folder has files');
        } else {
          cleanBtn.click();
          await sleep(200);
          const modal = document.getElementById('video-cleanup-modal');
          const ageSel = document.getElementById('video-cleanup-age');
          const summary = document.getElementById('video-cleanup-summary');
          const list = document.getElementById('video-cleanup-list');
          if (!modal || modal.classList.contains('hidden')) {
            out.errors.push('the video cleanup button did not open its confirm modal');
          } else {
            // 389: with nothing discarded yet, the trash section stays hidden --
            // window.__DISCARDED_FIXTURE__ is unset at this point in the probe.
            const discardedSection = document.getElementById('video-cleanup-discarded-section');
            if (!discardedSection || !discardedSection.classList.contains('hidden')) {
              out.errors.push('the discard-trash section shows although nothing was discarded');
            }
            if (ageSel.value !== '30') out.errors.push('the video cleanup age select does not default to 30 days');
            const rows = [...list.querySelectorAll('.import-preview-row')];
            if (rows.length !== 1) {
              out.errors.push('the video cleanup preview lists ' + rows.length + ' rows, expected 1');
            } else {
              if ((rows[0].querySelector('.import-preview-name') || {}).textContent !== 'old-recording.mp4') {
                out.errors.push('the video cleanup preview row has the wrong file name');
              }
              if ((rows[0].querySelector('.import-preview-badge') || {}).textContent !== '3.0 MB') {
                out.errors.push('the video cleanup preview row has the wrong size badge');
              }
            }
            if (!(summary.textContent || '').includes('3.0 MB')) {
              out.errors.push('the video cleanup summary reads "' + summary.textContent + '"');
            }
            // A checkbox per row plus Select all/none feeds "Export
            // selected", which opens its own small dialog for format,
            // quality and destination, then calls the batch command with
            // resolved source and destination paths.
            {
              const exportBtn = document.getElementById('video-cleanup-export');
              if (exportBtn.disabled !== true) out.errors.push('the export-selected button starts enabled although nothing is checked');
              document.getElementById('video-cleanup-select-all').click();
              if (exportBtn.disabled) out.errors.push('Select all did not enable the export-selected button');
              document.getElementById('video-cleanup-select-none').click();
              if (!exportBtn.disabled) out.errors.push('Select none did not disable the export-selected button');
              document.getElementById('video-cleanup-select-all').click();
              window.__PICK_FOLDER__ = 'C:\\\\Export\\\\Out';
              window.__BATCH_EXPORT_CALLED__ = null;
              exportBtn.click();
              await sleep(200);
              const bModal = document.getElementById('batch-export-modal');
              if (!bModal || bModal.classList.contains('hidden')) {
                out.errors.push('Export selected did not open the batch-export dialog');
              } else {
                const bSummary = document.getElementById('batch-export-summary');
                if (!(bSummary.textContent || '').includes('1')) {
                  out.errors.push('the batch export summary does not mention the 1 selected recording');
                }
                document.getElementById('batch-export-choose-dest').click();
                await sleep(150);
                const destOut = document.getElementById('batch-export-dest-out');
                if ((destOut.textContent || '') !== 'C:\\\\Export\\\\Out') {
                  out.errors.push('choosing a destination folder did not show its path');
                }
                document.getElementById('batch-export-start').click();
                await sleep(200);
                if (!window.__BATCH_EXPORT_CALLED__) {
                  out.errors.push('Start did not call export_recordings_batch');
                } else {
                  const items = window.__BATCH_EXPORT_CALLED__.items || [];
                  if (items.length !== 1 || items[0].dest_path !== 'C:\\\\Export\\\\Out\\\\old-recording.mp4') {
                    out.errors.push('export_recordings_batch got the wrong item paths: ' + JSON.stringify(items));
                  }
                }
                if (!bModal.classList.contains('hidden')) out.errors.push('the batch-export dialog did not close after Start');
                if (modal.classList.contains('hidden')) out.errors.push('the video-cleanup modal did not reopen after a batch export');
              }
              window.__PICK_FOLDER__ = null;
            }
            // Changing the age re-runs the preview with the new value.
            window.__SMOKE_ARGS__ = [];
            ageSel.value = '7';
            ageSel.dispatchEvent(new Event('change', { bubbles: true }));
            await sleep(200);
            const previewCalls = window.__SMOKE_ARGS__.filter(([c]) => c === 'video_folder_cleanup_preview');
            if (!previewCalls.length || previewCalls[previewCalls.length - 1][1].ageDays !== 7) {
              out.errors.push('changing the cleanup age did not re-run the preview with the new value');
            }
            // Cancel must close without deleting anything.
            window.__VIDEO_CLEANUP_CALLED__ = null;
            document.getElementById('video-cleanup-cancel').click();
            await sleep(200);
            if (!modal.classList.contains('hidden')) out.errors.push('canceling the video cleanup did not close the modal');
            if (window.__VIDEO_CLEANUP_CALLED__) out.errors.push('canceling the video cleanup deleted files anyway');
            // Reopen (resets to the 30-day default) and confirm: this time it
            // must actually delete and report back.
            cleanBtn.click();
            await sleep(200);
            document.getElementById('video-cleanup-confirm').click();
            await sleep(200);
            if (!modal.classList.contains('hidden')) out.errors.push('confirming the video cleanup did not close the modal');
            if (!window.__VIDEO_CLEANUP_CALLED__ || window.__VIDEO_CLEANUP_CALLED__.ageDays !== 30) {
              out.errors.push('confirming the video cleanup did not call video_folder_cleanup with the shown age');
            }
            const toastEl3 = document.getElementById('toast');
            if (!(toastEl3.textContent || '').includes('1 files deleted')) {
              out.errors.push('the video cleanup done toast reads "' + toastEl3.textContent + '"');
            }
            // 389: now with one discarded recording waiting -- the trash section
            // must appear, list it, and its Restore button must call the right command.
            window.__DISCARDED_FIXTURE__ = [{ name: 'rec-discarded.mp4', size: 1048576 }];
            cleanBtn.click();
            await sleep(200);
            if (discardedSection.classList.contains('hidden')) {
              out.errors.push('the discard-trash section stays hidden although a recording was discarded');
            } else {
              const dRows = [...document.getElementById('video-cleanup-discarded-list').querySelectorAll('.import-preview-row')];
              if (dRows.length !== 1) {
                out.errors.push('the discard-trash list shows ' + dRows.length + ' rows, expected 1');
              } else {
                if ((dRows[0].querySelector('.import-preview-name') || {}).textContent !== 'rec-discarded.mp4') {
                  out.errors.push('the discard-trash row has the wrong file name');
                }
                const restoreBtn = [...dRows[0].querySelectorAll('.import-preview-actions button')][0];
                window.__SMOKE_ARGS__ = [];
                restoreBtn.click();
                await sleep(200);
                const restoreCall = window.__SMOKE_ARGS__.find(([c]) => c === 'restore_discarded_recording');
                if (!restoreCall || restoreCall[1].name !== 'rec-discarded.mp4') {
                  out.errors.push('clicking Restore on a discarded recording did not call restore_discarded_recording with its name');
                }
              }
            }
            window.__DISCARDED_FIXTURE__ = null;
            document.getElementById('video-cleanup-close').click();
            await sleep(150);
          }
        }
      }
      const tab0b = document.querySelector('#expert-tabs-bar .expert-tab');
      if (tab0b) { tab0b.click(); await sleep(150); }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    const DRAG_OUT = 'into another app';
    const offTile = gridTile('p1');
    if (offTile) {
      // Ships off: the modifier must not arm anything.
      press(offTile, { ctrlKey: true });
      if (offTile.draggable) out.errors.push('a tile is drag-out armed although the feature is off');
      abort();
      await sleep(100);
    }
    if (!await setExpertFlag(DRAG_OUT, true)) {
      out.errors.push('the drag-out option is missing from the expert menu');
    } else {
      // Plain drag: unchanged. The ghost proves the reorder engaged; dropping it on a
      // free cell has to save the new layout.
      calls().length = 0;
      const tile = gridTile('p1');
      const origCell = tile.closest('.cell');
      const origCol = origCell && origCell.dataset.col, origRow = origCell && origCell.dataset.row;
      press(tile, {});
      if (tile.draggable) out.errors.push('a plain press armed a drag-out');
      window.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX: 300, clientY: 200 }));
      await sleep(150);
      if (!document.querySelector('.drag-ghost')) out.errors.push('a plain drag no longer starts the tile reorder');
      const cell = [...document.querySelectorAll('#grid .cell')].find(c => {
        const b = c.getBoundingClientRect();
        return b.width && b.right < innerWidth && b.bottom < innerHeight;
      });
      const b = cell && cell.getBoundingClientRect();
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true,
        clientX: b ? b.left + b.width / 2 : 300, clientY: b ? b.top + b.height / 2 : 200 }));
      await sleep(350);
      if (document.querySelector('.drag-ghost')) out.errors.push('the drag ghost outlived the drag');
      if (cell && !calls().includes('set_layout')) {
        out.errors.push('a plain drag did not save a new tile position');
      }
      // P8: the same reorder must offer an undo toast. Click it and prove the tile
      // really lands back on its original cell (not just that a class flips), then
      // leave the toast the way its own click handler leaves it (hideToast()) — no
      // stray classList surgery, and nothing lingers into later checks.
      if (cell && origCell) {
        const undoToast = document.getElementById('toast');
        const undoBtn = undoToast.querySelector('.toast-btn');
        if (!undoToast.classList.contains('actionable') || !undoBtn) {
          out.errors.push('a plain drag did not offer an undo toast');
        } else {
          undoBtn.click();
          await sleep(300);
          const backCell = gridTile('p1')?.closest('.cell');
          if (!backCell || backCell.dataset.col !== origCol || backCell.dataset.row !== origRow) {
            out.errors.push('undo did not put the tile back on its original cell');
          }
          if (undoToast.classList.contains('show')) out.errors.push('the undo toast is still showing after being clicked');
        }
      }
      // Modifier drag: armed, and the payload is the raw text — placeholders included,
      // because filling them needs a dialog no drag can open.
      const varTile2 = gridTile('p1');
      press(varTile2, { ctrlKey: true });
      if (!varTile2.draggable) out.errors.push('the modifier did not arm the drag-out');
      const ev = fireDragStart(varTile2);
      const dropped = ev.dataTransfer.getData('text/plain');
      if (dropped !== 'Translate {#{Text=Hello}#} into {#{Language}#}.') {
        out.errors.push('the dragged text is wrong: ' + dropped);
      }
      if (document.querySelector('.drag-ghost')) out.errors.push('a drag-out left a reorder ghost behind');
      // Once the OS owns the gesture a stray pointerup must not ALSO copy the prompt
      // (p1 asks for placeholders, so a copy would show up as the placeholder window).
      calls().length = 0;
      window.__VAR_PAYLOAD__ = undefined;
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, clientX: 20, clientY: 20 }));
      await sleep(200);
      if (varReq()) {
        out.errors.push('a drag-out also started a copy');
        await varCancel();
      }
      if (calls().includes('copy_prompt')) out.errors.push('a drag-out also copied the prompt');
      varTile2.dispatchEvent(new DragEvent('dragend', { bubbles: true }));
      await sleep(100);
      if (varTile2.draggable) out.errors.push('the tile stayed armed after the drag ended');
      // A file-backed prompt drops its path plus a URI for targets that read one.
      const fileTile = gridTile('p9');
      if (!fileTile) {
        out.errors.push('the file-backed prompt is missing from the grid');
      } else {
        press(fileTile, { ctrlKey: true });
        const fev = fireDragStart(fileTile);
        if (fev.dataTransfer.getData('text/plain') !== 'C:\\\\Notes\\\\brief.txt') {
          out.errors.push('a file prompt did not drop its path: ' + fev.dataTransfer.getData('text/plain'));
        }
        if (fev.dataTransfer.getData('text/uri-list') !== 'file:///C:/Notes/brief.txt') {
          out.errors.push('a file prompt dropped no file URI: ' + fev.dataTransfer.getData('text/uri-list'));
        }
        fileTile.dispatchEvent(new DragEvent('dragend', { bubbles: true }));
      }
      // The library rows are drag sources too.
      await click('library-btn');
      const libRow = document.querySelector('#library-list .lib-item');
      if (!libRow) {
        out.errors.push('the library has no row for the drag-out check');
      } else {
        press(libRow, { ctrlKey: true });
        if (!libRow.draggable) out.errors.push('a library row cannot be dragged out');
        if (!fireDragStart(libRow).dataTransfer.getData('text/plain')) {
          out.errors.push('a library row dropped no text');
        }
        libRow.dispatchEvent(new DragEvent('dragend', { bubbles: true }));
      }
      await click('library-close');
      // Switching it off takes the whole behaviour with it — including on a tile that
      // is already wired up, which is the only way a stale drag could still fire.
      await setExpertFlag(DRAG_OUT, false);
      press(varTile2, { ctrlKey: true });
      if (varTile2.draggable) out.errors.push('the modifier still arms a drag-out after switching it off');
      if (!fireDragStart(varTile2).defaultPrevented) {
        out.errors.push('a drag-out was not refused after switching the feature off');
      }
      abort();
      await sleep(100);
    }
    // F14/F15: the preview renders every Markdown construct, colours the fenced code
    // blocks, and — the part that matters — shows injected markup as literal text.
    // Ships off now, so the test has to switch it on itself.
    if (!await setExpertFlag('Markdown preview', true)) {
      out.errors.push('the Markdown preview option is missing from the expert menu');
    }
    const openEditor = async (id) => {
      const el = [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
      if (!el) return false;
      el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(200);
      const edit = document.querySelector('#ctx [data-act="edit"]');
      if (!edit) return false;
      edit.click();
      await sleep(400);
      return !document.getElementById('modal').classList.contains('hidden');
    };
    if (!await openEditor('p7')) {
      out.errors.push('the Markdown prompt did not open in the editor');
    } else {
      const prev = document.getElementById('modal-md-preview');
      const pwrap = document.getElementById('modal-md-wrap');
      const ta = document.getElementById('modal-text');
      // With the feature on the prompt opens RENDERED — no switch, no empty pane.
      if (!prev.innerHTML.trim()) {
        out.errors.push('the preview pane is empty before it is opened');
      }
      if (pwrap.classList.contains('hidden')) {
        out.errors.push('the prompt does not open rendered');
      } else {
        if (!ta.classList.contains('hidden')) {
          out.errors.push('the text field is still shown although the preview took its place');
        }
        // An empty or invisible pane is exactly the "preview shows nothing" bug.
        const box = prev.getBoundingClientRect();
        if (getComputedStyle(prev).display === 'none' || box.width < 1 || box.height < 1) {
          out.errors.push('the preview pane has no visible box: ' + Math.round(box.width) + 'x' + Math.round(box.height));
        }
        if (prev.children.length < 5) {
          out.errors.push('the preview rendered ' + prev.children.length + ' nodes — it is not showing the prompt');
        }
        for (const pair of [['h1', 'heading 1'], ['h2', 'heading 2'], ['strong', 'bold'],
                            ['em', 'italic'], ['code.md-inline-code', 'inline code'],
                            ['ul li', 'bullet list'], ['ol li', 'numbered list'],
                            ['blockquote', 'quote'], ['hr', 'rule'],
                            ['pre.md-code', 'code block'], ['.md-link', 'link']]) {
          if (!prev.querySelector(pair[0])) out.errors.push('the preview does not render ' + pair[1]);
        }
        // Nothing the user types may become an element that loads or navigates.
        if (prev.querySelector('a, img, script, iframe, [href], [src], [onerror], [onmouseover]')) {
          out.errors.push('the preview emitted an element that can load or navigate');
        }
        if (window.xssRan !== undefined) out.errors.push('script injected through a prompt ran');
        const shown = prev.textContent;
        for (const literal of ['<script>', 'onerror', '<b onmouseover', 'javascript:alert(1)', 'click me']) {
          if (!shown.includes(literal)) out.errors.push('injected markup is not shown literally: ' + literal);
        }
        // F15: nine fenced blocks, eight of them tagged; the untagged one stays plain.
        const blocks = [...prev.querySelectorAll('pre.md-code')];
        if (blocks.length !== 9) {
          out.errors.push('the preview shows ' + blocks.length + ' code blocks instead of 9');
        } else {
          blocks.forEach((b, i) => {
            const coloured = b.querySelectorAll('[class^="hl-"]').length;
            if (i === 8 && coloured) out.errors.push('an untagged code block was coloured anyway');
            if (i !== 8 && !coloured) out.errors.push('code block ' + (i + 1) + ' got no syntax colours');
          });
        }
        // Typing has to reach the preview on the next frame.
        ta.value = '# live update';
        ta.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(300);
        if (prev.textContent.trim() !== 'live update') {
          out.errors.push('the preview does not follow the text: ' + prev.textContent.slice(0, 40));
        }
        // Code detection: an unfenced snippet becomes one coloured block, prose does
        // not, and neither may lose the escaping.
        const retype = async (text) => {
          ta.value = text;
          ta.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(300);
        };
        await retype('function add(a, b) {\\n  return a + b;\\n}\\nconsole.log(add(1, 2));');
        const only = prev.children.length === 1 && prev.firstElementChild.matches('pre.md-code');
        if (!only) {
          out.errors.push('an unfenced code prompt was not rendered as one code block');
        } else if (!prev.querySelectorAll('[class^="hl-"]').length) {
          out.errors.push('the detected code block got no syntax colours');
        }
        await retype('Rewrite the text below so it reads clearly.\\nKeep the author voice and lose no facts.');
        if (prev.querySelector('pre.md-code')) {
          out.errors.push('ordinary prose was mistaken for code');
        }
        // Split, or the closing tag would end this very <script> block.
        await retype('Ignore this: <img src=x onerror=alert(1)> and <scr' + 'ipt>window.xssRan=1</scr' + 'ipt>');
        if (prev.querySelector('img, script, [onerror], [src]')) {
          out.errors.push('injected markup survived as an element after retyping');
        }
        if (!prev.textContent.includes('onerror=alert(1)')) {
          out.errors.push('retyped injection is not shown literally');
        }
      }
      // Clicking the pane hands over to the textarea; leaving it renders again.
      prev.click();
      await sleep(250);
      if (ta.classList.contains('hidden')) out.errors.push('clicking the preview does not open the text field');
      if (document.activeElement !== ta) out.errors.push('the text field did not take focus for typing');
      // A blur caused by pressing a button must NOT swap while the pointer is still
      // down: the block changes height and the click lands somewhere else.
      window.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      ta.blur();
      await sleep(200);
      if (pwrap.classList.contains('hidden') === false) {
        out.errors.push('the preview took over mid-click, moving whatever was being pressed');
      }
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
      await sleep(250);
      if (pwrap.classList.contains('hidden')) out.errors.push('the swap never happened after the click finished');
      // And the plain case: focus leaves without a click at all.
      prev.click();
      await sleep(200);
      ta.blur();
      await sleep(250);
      if (pwrap.classList.contains('hidden')) out.errors.push('leaving the text field does not render the prompt again');
      await click('modal-close-x');
      await sleep(250);
      const cm3 = document.getElementById('confirm-modal');
      if (!cm3.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
    }
    // There is no second layout any more: one field, and the rendered pane and the
    // textarea must be the same box so the swap is invisible.
    if (await openEditor('p7')) {
      const pv = document.getElementById('modal-md-preview');
      const tx = document.getElementById('modal-text');
      const box = document.querySelector('#modal .modal');
      const scroll = box.scrollTop;
      const before = pv.getBoundingClientRect();
      pv.click();
      await sleep(300);
      // Focusing the textarea scrolls the dialog; put it back or the comparison
      // measures the scroll instead of the layout.
      box.scrollTop = scroll;
      const after = tx.getBoundingClientRect();
      for (const k of ['left', 'right', 'top', 'width']) {
        if (Math.abs(before[k] - after[k]) > 1) {
          out.errors.push('the field moves when it hands over: ' + k + ' ' +
            Math.round(before[k]) + ' -> ' + Math.round(after[k]));
        }
      }
      const a = getComputedStyle(pv), b = getComputedStyle(tx);
      for (const prop of ['fontSize', 'lineHeight', 'padding', 'borderRadius', 'backgroundColor']) {
        if (a[prop] !== b[prop]) {
          out.errors.push('rendered pane and text field differ in ' + prop + ': ' + a[prop] + ' vs ' + b[prop]);
        }
      }
      await click('modal-close-x');
      await sleep(250);
      const cm4 = document.getElementById('confirm-modal');
      if (!cm4.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
    }
    // 10: the quick-grid header control hides until you point at the header — pure
    // CSS :hover, so a synthetic event can't prove the reveal itself; this only
    // proves the flag actually hides/unhides the control (the hover is checked by eye).
    const qgEl = document.getElementById('quick-grid');
    // The header columns/rows fields are an opt-in: hidden on a fresh profile,
    // while the grid size stays reachable from two right-click menus.
    if (getComputedStyle(qgEl).display !== 'none') out.errors.push('the quick-grid fields show on a fresh profile');
    {
      const openMenuAt = (el) => {
        const r = el.getBoundingClientRect();
        el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 6, clientY: r.top + 6 }));
      };
      const gridMenuItem = () => [...document.querySelectorAll('.tab-menu:not(.hidden) button')].find(b => b.textContent.trim().endsWith('…'));
      const checkGridSizeEntry = async (what, el) => {
        if (!el) { out.errors.push('grid size menu: no ' + what + ' to right-click'); return; }
        openMenuAt(el);
        await sleep(150);
        const it = gridMenuItem();
        if (!it) { out.errors.push('grid size menu: right-click on ' + what + ' offers no Grid size entry'); document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); return; }
        it.click();
        await sleep(200);
        const vm = document.getElementById('view-modal');
        if (vm.classList.contains('hidden')) out.errors.push('grid size menu: ' + what + ' did not open the view dialog');
        else if (document.activeElement !== document.getElementById('view-modal-cols')) out.errors.push('grid size menu: ' + what + ' did not focus the columns field');
        await click('view-modal-close');
        await sleep(150);
      };
      await checkGridSizeEntry('a view tab', document.querySelector('.view-btn[data-view-id]'));
      await checkGridSizeEntry('an empty grid cell', [...document.querySelectorAll('#grid .cell')].find(c => !c.firstElementChild));
    }
    if (!await setExpertFlag('Quick grid size', true)) out.errors.push('the quick-grid switch is missing from the expert menu');
    else if (getComputedStyle(qgEl).display === 'none') out.errors.push('the quick-grid fields stay hidden after switching them on');
    if (!await setExpertFlag('Quick grid: hover to reveal', true)) {
      out.errors.push('the quick-grid hover-only option is missing from the expert menu');
    } else {
      const csOn = getComputedStyle(qgEl);
      if (csOn.opacity !== '0') out.errors.push('the quick-grid control is not hidden with hover-only on: opacity ' + csOn.opacity);
      if (csOn.pointerEvents === 'auto') out.errors.push('the hidden quick-grid control still accepts clicks');
      await setExpertFlag('Quick grid: hover to reveal', false);
      const csOff = getComputedStyle(qgEl);
      if (csOff.opacity !== '1') out.errors.push('the quick-grid control stays hidden after switching hover-only back off');
    }
    // P31: version history shows a word diff against whatever this snapshot
    // turned into next, not just the raw old text.
    if (!await openEditor('p6')) {
      out.errors.push('the version-history prompt did not open in the editor');
    } else {
      document.getElementById('modal-advanced-head').click();
      await sleep(250);
      const rows = [...document.querySelectorAll('#modal-history-body .hist-row')];
      if (rows.length !== 2) {
        out.errors.push('the history panel shows ' + rows.length + ' versions instead of 2');
      } else {
        // 125: each row's full diff is now built lazily on first expand -- open
        // every row before reading its .hist-full content.
        for (const r of rows) r.querySelector('.hist-expand')?.click();
        // Newest row (id 2): "draft"->"prompt" against the live current text.
        const newRowText = rows[0].querySelector('.hist-full').textContent;
        if (!newRowText.includes('draft') || !newRowText.includes('prompt')) {
          out.errors.push('the newest version diff is missing the expected words: ' + newRowText);
        }
        const newAdd = rows[0].querySelector('.diff-add'), newDel = rows[0].querySelector('.diff-del');
        if (!newAdd || newAdd.textContent !== 'prompt') out.errors.push('the newest row does not mark "prompt" as added');
        if (!newDel || newDel.textContent !== 'draft') out.errors.push('the newest row does not mark "draft" as removed');
        // Oldest row (id 1): "text"->"body" against the next-newer snapshot.
        const oldAdd = rows[1].querySelector('.diff-add'), oldDel = rows[1].querySelector('.diff-del');
        if (!oldAdd || oldAdd.textContent !== 'body') out.errors.push('the oldest row does not mark "body" as added');
        if (!oldDel || oldDel.textContent !== 'text') out.errors.push('the oldest row does not mark "text" as removed');
      }
      await click('modal-close-x');
      await sleep(200);
      const cmHist = document.getElementById('confirm-modal');
      if (!cmHist.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // 408: "Save as new entry" leaves the original prompt alone and adds the
    // snapshot's content as its own new entry instead, closing the editor and
    // showing it selected so the user sees what happened.
    if (!await openEditor('p6')) {
      out.errors.push('408: the version-history prompt did not open in the editor');
    } else {
      document.getElementById('modal-advanced-head').click();
      await sleep(250);
      const rows408 = [...document.querySelectorAll('#modal-history-body .hist-row')];
      if (rows408.length !== 2) {
        out.errors.push('408: the history panel shows ' + rows408.length + ' versions instead of 2');
      } else {
        const restoreNewBtn = rows408[0].querySelector('.hist-restore-new');
        if (!restoreNewBtn) {
          out.errors.push('408: the newest version row has no "save as new entry" button');
        } else {
          restoreNewBtn.click();
          await sleep(400);
          if (!document.getElementById('modal').classList.contains('hidden')) {
            out.errors.push('408: the editor stayed open after saving the version as a new entry');
          }
          const toastEl408 = document.getElementById('toast');
          if ((toastEl408.textContent || '').trim() !== 'New entry created') {
            out.errors.push('408: no "new entry created" toast after saving the version as new: ' + toastEl408.textContent);
          }
          await click('library-btn');
          const expectedName = 'Prompt 7 (version from';
          const hit = [...document.querySelectorAll('#library-list .lib-name')].some(e => e.textContent.startsWith(expectedName));
          if (!hit) {
            out.errors.push('408: no new library entry named like "' + expectedName + '…" after saving the version as new');
          }
          await click('library-close');
        }
      }
    }
    // 79: version history for a rotating prompt shows every entry, numbered, each
    // with its own diff, an entry-count chip on the row, and a name-change line
    // wherever the snapshot's name differs from what it became next.
    if (!await openEditor('p10')) {
      out.errors.push('79: the rotating prompt did not open in the editor');
    } else {
      document.getElementById('modal-advanced-head').click();
      await sleep(250);
      layoutScan('rotating prompt history');
      const rows10 = [...document.querySelectorAll('#modal-history-body .hist-row')];
      if (rows10.length !== 2) {
        out.errors.push('79: the rotating prompt history shows ' + rows10.length + ' versions instead of 2');
      } else {
        // 125: each row's full diff is now built lazily on first expand.
        for (const r of rows10) r.querySelector('.hist-expand')?.click();
        const chip = rows10[0].querySelector('.hist-entries');
        if (!chip || !chip.textContent.includes('2')) {
          out.errors.push('79: the newest rotating-prompt row is missing its entry-count chip');
        }
        const blocks = rows10[0].querySelectorAll('.hist-entry-block');
        if (blocks.length < 2) out.errors.push('79: the rotating prompt diff does not show a numbered block per entry');
        if (!rows10[1].querySelector('.hist-name-change')) {
          out.errors.push('79: a renamed version does not show the old to new name line');
        }
      }
      await click('modal-close-x');
      await sleep(200);
      const cmHist10 = document.getElementById('confirm-modal');
      if (!cmHist10.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // 79: a file/image prompt has no text field but still gets version history —
    // only a name change to show, never an empty box.
    if (!await openEditor('p9')) {
      out.errors.push('79: the file prompt did not open in the editor');
    } else {
      document.getElementById('modal-advanced-head').click();
      await sleep(200);
      if (document.getElementById('modal-history').classList.contains('hidden')) {
        out.errors.push('79: version history is hidden for a file prompt');
      }
      await click('modal-close-x');
      await sleep(200);
      const cmHist9 = document.getElementById('confirm-modal');
      if (!cmHist9.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // 429: a prompt with both a stored preview and a real attachment gets a line
    // explaining the difference, spelled out and not a raw i18n key.
    if (!await openEditor('p11')) {
      out.errors.push('429: the preview+attachment prompt did not open in the editor');
    } else {
      const hint = document.getElementById('modal-preview-attach-hint');
      const fileIcon = document.getElementById('modal-file-icon');
      if (!hint || hint.classList.contains('hidden')) {
        out.errors.push('429: the preview/attachment hint is not shown when both exist');
      } else if (!hint.textContent.trim() || hint.textContent.trim() === 'previewAttachHint') {
        out.errors.push('429: the preview/attachment hint shows no text or a raw key');
      }
      if (!fileIcon || !fileIcon.title || fileIcon.title === 'previewAttachHint') {
        out.errors.push('429: the attachment icon has no matching tooltip text');
      } else if (hint && fileIcon.title !== hint.textContent.trim()) {
        out.errors.push('429: the attachment icon tooltip does not match the editor hint text');
      }
      await click('modal-close-x');
      await sleep(200);
      const cmHist11 = document.getElementById('confirm-modal');
      if (!cmHist11.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // 429: the same prompt without a file attachment must NOT show the hint — it is
    // only for the case where the two things could actually be confused.
    if (!await openEditor('p7')) {
      out.errors.push('429: the plain-text prompt did not open in the editor');
    } else {
      const hint7 = document.getElementById('modal-preview-attach-hint');
      if (!hint7 || !hint7.classList.contains('hidden')) {
        out.errors.push('429: the preview/attachment hint shows for a prompt with no attachment');
      }
      await click('modal-close-x');
      await sleep(200);
      const cmHist7b = document.getElementById('confirm-modal');
      if (!cmHist7b.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // 466: a text attachment (txt/md/json/csv) previews its start right in the
    // editor, and injected markup in it must show up literally, never rendered.
    window.__ATTACH_TEXT_FIXTURES__ = window.__ATTACH_TEXT_FIXTURES__ || {};
    window.__ATTACH_TEXT_FIXTURES__['C:\\\\Notes\\\\brief.txt'] = { text: XSS_TEXT, truncated: false, readable: true };
    if (!await openEditor('p11')) {
      out.errors.push('466: the text-attachment prompt did not open in the editor');
    } else {
      await sleep(150);
      const box = document.getElementById('modal-text-attach-preview');
      const content = document.getElementById('modal-text-attach-content');
      if (!box || box.classList.contains('hidden')) {
        out.errors.push('466: the attachment text preview did not show for a .txt attachment');
      } else if (!content || content.textContent !== XSS_TEXT) {
        out.errors.push('466: the attachment preview text does not match the file content');
      } else if (content.querySelector('img') || content.querySelector('script')) {
        out.errors.push('466: injected markup in a text attachment was rendered as an element');
      }
      await click('modal-close-x');
      await sleep(200);
      const cmAttach = document.getElementById('confirm-modal');
      if (!cmAttach.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // Duplicates: its own dialog, reachable from the library header, and the last
    // window of the main surface that no scan had ever opened.
    await click('library-btn');
    await sleep(150);
    const dupBtn = document.getElementById('library-dupes');
    if (!dupBtn) {
      out.errors.push('the library header has no way to open the duplicates dialog');
    } else {
      dupBtn.click();
      await sleep(500);
      const dm = document.getElementById('dupes-modal');
      if (dm.classList.contains('hidden')) {
        out.errors.push('the duplicates dialog did not open');
      } else {
        if (!document.querySelectorAll('#dupes-body .dup-group').length) {
          out.errors.push('the duplicates dialog shows no group for a reported duplicate');
        }
        layoutScan('duplicates');
        mustReach('duplicates', '#dupes-body .dup-group button, #dupes-modal .modal-head-actions > button');
        // The group is now just a summary card -- its own "Compare" button opens
        // the SAME view the library's/tile's free compare uses, this time with
        // the finder's own "keep one side" actions instead of "take left/right".
        const groupCompareBtn = document.querySelector('#dupes-body .dup-group .dup-actions button');
        if (!groupCompareBtn) {
          out.errors.push('a duplicate group has no way to open the compare view');
        } else {
          groupCompareBtn.click();
          await sleep(300);
          const cmpModal = document.getElementById('compare-modal');
          if (cmpModal.classList.contains('hidden')) {
            out.errors.push('the duplicate group did not open the compare view');
          } else {
            if (document.querySelectorAll('#compare-body .dup-col').length !== 2) {
              out.errors.push('the compare view does not show both entries of the duplicate pair');
            }
            // A found pair shares the same word-diff engine as the free compare
            // view -- its two columns must highlight what differs, not just show
            // plain text side by side.
            if (!document.querySelector('#compare-body .diff-add, #compare-body .diff-del')) {
              out.errors.push('a duplicate pair does not highlight its word differences');
            }
            if ([...document.querySelectorAll('#compare-body .dup-col')].some((col) => col.querySelectorAll('.dup-keep').length !== 1)) {
              out.errors.push('the compare view opened from the duplicate finder is missing a per-column "keep this one" action');
            }
            if (document.querySelectorAll('#compare-actions .dup-keep').length !== 0) {
              out.errors.push('the compare view opened from the duplicate finder still has "keep one side" actions in the action bar');
            }
            layoutScan('duplicates compare view');
            mustReach('duplicates compare view', '#compare-actions button, #compare-body .dup-keep');
            if (!(Number(getComputedStyle(cmpModal).zIndex) > Number(getComputedStyle(dm).zIndex))) {
              out.errors.push('the compare view opens behind the duplicates dialog');
            }
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
            await sleep(200);
            if (!cmpModal.classList.contains('hidden') || dm.classList.contains('hidden')) {
              out.errors.push('Escape does not close the compare view first, over the duplicates dialog');
            }
          }
          if (!cmpModal.classList.contains('hidden')) await click('compare-close');
          await sleep(200);
        }
        await click('dupes-close');
        await sleep(200);
      }
    }
    // The save-time similar-prompt hint (its own switch, sharing similarThreshold
    // with the duplicate finder above -- same System & data group). Off first:
    // even with a fixture hit ready, saving must show nothing of the kind. Edits
    // an EXISTING library entry rather than creating a new one -- the grid may
    // be full of tiles the earlier parts of this sweep already placed, but the
    // save path (and the code that fires the hint) is identical either way.
    const findSimHintBox = () => {
      const row = [...document.querySelectorAll('#expert-flags label.switch-field')]
        .find((r) => r.querySelector('span')?.textContent === 'Similar-prompt hint on save');
      return row ? row.querySelector('input[type="checkbox"]') : null;
    };
    const editViaLibrary = async (id, text) => {
      await click('library-btn');
      await sleep(150);
      const editBtn = document.querySelector(`#library-list .lib-item[data-id="${id}"] .lib-edit`);
      if (!editBtn) { out.errors.push('442: the library has no edit icon for ' + id); return false; }
      editBtn.click();
      await sleep(200);
      document.getElementById('modal-text').value = text;
      document.getElementById('modal-confirm').click();
      await sleep(400);
      return true;
    };
    document.getElementById('settings').classList.remove('hidden');
    await click('expert-open');
    let simTab = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')].find(b => /system/i.test(b.textContent));
    if (simTab) { simTab.click(); await sleep(200); }
    const simBoxOff = findSimHintBox();
    if (!simBoxOff) {
      out.errors.push('442: the expert menu has no similar-prompt-hint switch');
    } else {
      if (!simBoxOff.checked) out.errors.push('442: similarHint does not ship on');
      simBoxOff.click(); // off
      await sleep(150);
    }
    await click('expert-back');
    document.getElementById('settings').classList.add('hidden');
    await sleep(150);
    window.__SIMILAR_HINT_FIXTURE__ = { id: 'p1', name: 'Prompt 2' };
    if (await editViaLibrary('p2', 'Similar hint off test edit.')) {
      if ([...document.querySelectorAll('#toast .toast-chip')].some((c) => c.textContent === 'Open')) {
        out.errors.push('442: the hint appeared while similarHint is off');
      }
    }
    // Back on, then the hint itself: message, Compare, and (on a second save)
    // Open, since a chip click closes the toast that carried it.
    document.getElementById('settings').classList.remove('hidden');
    await click('expert-open');
    simTab = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')].find(b => /system/i.test(b.textContent));
    if (simTab) { simTab.click(); await sleep(200); }
    findSimHintBox()?.click(); // on
    await sleep(150);
    await click('expert-back');
    document.getElementById('settings').classList.add('hidden');
    await sleep(150);
    if (await editViaLibrary('p3', 'Similar hint on test edit, compare action.')) {
      const toastEl = document.getElementById('toast');
      const chips = [...toastEl.querySelectorAll('.toast-chip')];
      if (toastEl.classList.contains('hidden') || chips.length !== 2) {
        out.errors.push('442: the similar-prompt hint did not appear with its two actions');
      } else {
        if (!toastEl.textContent.includes('Prompt 2')) out.errors.push('442: the hint does not name the similar prompt');
        const compareChip = chips.find((c) => c.textContent === 'Compare');
        if (!compareChip) {
          out.errors.push('442: the hint has no Compare action');
        } else {
          compareChip.click();
          await sleep(300);
          if (document.getElementById('compare-modal').classList.contains('hidden')) {
            out.errors.push('442: Compare on the hint did not open the compare view');
          }
          await click('compare-close');
          await sleep(200);
        }
      }
    }
    if (await editViaLibrary('p4', 'Similar hint on test edit, open action.')) {
      const openChip = [...document.querySelectorAll('#toast .toast-chip')].find((c) => c.textContent === 'Open');
      if (!openChip) {
        out.errors.push('442: the hint has no Open action on its second appearance');
      } else {
        openChip.click();
        await sleep(300);
        if (document.getElementById('modal').classList.contains('hidden') || document.getElementById('modal-name').value !== 'Prompt 2') {
          out.errors.push('442: Open on the hint did not open the similar prompt for editing');
        }
        await click('modal-close-x');
        await sleep(200);
        const cmAfterOpen = document.getElementById('confirm-modal');
        if (!cmAfterOpen.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
      }
    }
    delete window.__SIMILAR_HINT_FIXTURE__;
    // Compare view detached from the duplicate finder: the library's own
    // selection reaches it too, for two entries the finder never flagged.
    await click('library-btn');
    // The duplicate finder also has its own shortcut in the library's own
    // header now, next to sort — reachable without the expert menu or the
    // settings dialog, and correctly gated and layered like the other two.
    const libDupesBtn = document.getElementById('library-dupes');
    if (!libDupesBtn || libDupesBtn.offsetParent === null) {
      out.errors.push('the library header has no visible duplicate finder shortcut');
    } else {
      libDupesBtn.click();
      await sleep(300);
      const libDupesModal = document.getElementById('dupes-modal');
      if (libDupesModal.classList.contains('hidden')) {
        out.errors.push('the library header shortcut did not open the duplicate finder');
      }
      // 552: Rescan shows a busy ring for at least a second even when the scan
      // answers at once, ignores a second click meanwhile, and never changes width.
      {
        const rescan552 = document.getElementById('dupes-rescan');
        const scanCalls552 = () => (window.__SMOKE_CALLS__ || []).filter((c) => c === 'find_duplicates').length;
        await sleep(300);
        const widthBefore552 = rescan552.getBoundingClientRect().width;
        const callsBefore552 = scanCalls552();
        rescan552.click();
        await sleep(50);
        rescan552.click();
        await sleep(100);
        const widthDuring552 = rescan552.getBoundingClientRect().width;
        await sleep(750);
        if (!rescan552.classList.contains('is-busy')) out.errors.push('552: the Rescan button is not busy 900 ms after the click');
        if (rescan552.getAttribute('aria-busy') !== 'true') out.errors.push('552: the busy Rescan button has no aria-busy');
        if (!rescan552.querySelector('.btn-spinner')) out.errors.push('552: the Rescan button has no spinner element');
        await sleep(500);
        if (rescan552.classList.contains('is-busy')) out.errors.push('552: the Rescan button is still busy 1400 ms after the click');
        if (rescan552.hasAttribute('aria-busy')) out.errors.push('552: aria-busy stayed on after the scan');
        if (scanCalls552() - callsBefore552 !== 1) out.errors.push('552: a second click during the busy state started another scan (' + (scanCalls552() - callsBefore552) + ' calls)');
        if (Math.abs(widthBefore552 - widthDuring552) > 1) out.errors.push('552: the Rescan button changed width while busy: ' + widthBefore552 + ' to ' + widthDuring552);
        if (!document.querySelectorAll('#dupes-body .dup-group').length) out.errors.push('552: no result list after the rescan');
      }
      await click('dupes-close');
      await sleep(150);
    }
    document.body.classList.add('library-front');
    libDupesBtn.click();
    await sleep(300);
    if (!(Number(getComputedStyle(document.getElementById('dupes-modal')).zIndex) > Number(getComputedStyle(document.getElementById('library')).zIndex))) {
      out.errors.push('the duplicate finder opens behind the raised library');
    }
    await click('dupes-close');
    document.body.classList.remove('library-front');
    await sleep(150);
    document.body.classList.add('noflag-dupFinder');
    if (libDupesBtn.offsetParent !== null) {
      out.errors.push('the library header duplicate finder shortcut ignores its dupFinder gate');
    }
    document.body.classList.remove('noflag-dupFinder');
    document.getElementById('library-select-btn').click();
    await sleep(150);
    document.querySelector('#library-list .lib-item[data-id="p0"]')?.click();
    document.querySelector('#library-list .lib-item[data-id="p2"]')?.click();
    await sleep(150);
    const cmpBtn = document.getElementById('lib-batch-compare');
    if (!cmpBtn || cmpBtn.disabled) {
      out.errors.push('the library compare button is not enabled with exactly two entries selected');
    } else {
      cmpBtn.click();
      await sleep(300);
      const cmpModal = document.getElementById('compare-modal');
      if (cmpModal.classList.contains('hidden')) {
        out.errors.push('the compare view did not open for two selected entries');
      } else {
        const cmpCols = document.querySelectorAll('#compare-body .compare-col');
        if (cmpCols.length !== 2) {
          out.errors.push('the compare view does not show both entries: ' + cmpCols.length + ' columns');
        }
        if (!document.querySelector('#compare-body .diff-add, #compare-body .diff-del')) {
          out.errors.push('the compare view does not highlight the word differences between the two entries');
        }
        layoutScan('compare view');
        mustReach('compare view', '#compare-actions button');
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await sleep(200);
        if (!cmpModal.classList.contains('hidden') || document.getElementById('library').classList.contains('hidden')) {
          out.errors.push('Escape does not close the compare view detached from the library first');
        }
        cmpBtn.click();
        await sleep(300);
        // Position, not text, so this still finds the button under the Italian
        // pass below (takeLeft, takeRight, close, in that fixed append order).
        const takeRightBtn = document.querySelectorAll('#compare-actions button')[1];
        if (!takeRightBtn) {
          out.errors.push('the compare view has no "take right" action');
        } else {
          const leftId = 'p0', rightId = 'p2';
          const leftText = PROMPTS.find(p => p.id === leftId).text;
          takeRightBtn.click();
          await sleep(300);
          if (!document.getElementById('compare-modal').classList.contains('hidden')) {
            out.errors.push('the compare view stayed open after taking a side');
          }
          // "Take right" pushes the LEFT entry's content onto the right one.
          if (PROMPTS.find(p => p.id === rightId).text !== leftText) {
            out.errors.push('"take right" did not overwrite the right entry with the left one\\'s text');
          }
        }
      }
    }
    document.getElementById('library-select-btn').classList.contains('active') && document.getElementById('library-select-btn').click();
    await click('library-close');
    // The colour picker must fit the window at ANY size: it may neither scroll (the
    // knob would be out of reach) nor spill (the hex field gets cut off).
    if (await openEditor('p1')) {
      // 66: every colour swatch is icon-only (a coloured square, no text) and
      // needs a real name for screen readers, not just a visible colour.
      const noneSwatch = document.querySelector('#color-row .swatch.none');
      const custom = document.querySelector('#color-row .swatch.custom');
      if (!noneSwatch || !noneSwatch.title) out.errors.push('66: the none-colour swatch has no accessible name');
      if (!custom) {
        out.errors.push('the editor colour row has no custom-colour swatch');
      } else if (!custom.title) {
        out.errors.push('66: the custom-colour swatch has no accessible name');
      }
      if (custom) {
        custom.click();
        await sleep(300);
        const cp = document.querySelector('.color-pop:not(.hidden)');
        if (!cp) out.errors.push('the custom colour swatch opened no picker');
        else {
          const r = cp.getBoundingClientRect();
          if (r.top < -1 || r.bottom > innerHeight + 1 || r.left < -1 || r.right > innerWidth + 1) {
            out.errors.push('the colour picker does not fit the window (' +
              Math.round(r.top) + '..' + Math.round(r.bottom) + ' of ' + innerHeight + ')');
          }
          if (cp.scrollHeight > cp.clientHeight + 2) {
            out.errors.push('the colour picker scrolls instead of fitting');
          }
        }
        document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
        await sleep(150);
      }
      await click('modal-close-x');
      await sleep(250);
      const cm5 = document.getElementById('confirm-modal');
      if (!cm5.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
    } else {
      out.errors.push('the editor did not open for the colour-picker check');
    }
    // Gating: a row whose feature is off must disappear the moment that toggle is
    // flipped — no reopening the menu — and a whole branch goes with its parent.
    const rowFor = (label) => [...document.querySelectorAll('#expert-flags .field')]
      .find(r => ((r.querySelector('span') || {}).textContent || '').trim().startsWith(label));
    const flip = async (label) => {
      const row = rowFor(label);
      if (!row) return false;
      row.querySelector('input.switch').click();
      await sleep(300);
      return true;
    };
    document.getElementById('settings').classList.remove('hidden');
    await click('expert-open');
    if (!await expertRow('Multiple views')) {
      out.errors.push('the multiple-views toggle is missing from the expert menu');
    } else {
      // The parent of a branch takes its children with it and gives them back.
      for (const kept of ['Prompt in multiple views', 'View tab border width']) {
        if (!rowFor(kept)) out.errors.push('"' + kept + '" is missing although multiple views are on');
      }
      await flip('Multiple views');
      for (const gone of ['Prompt in multiple views', 'View tab border width']) {
        if (rowFor(gone)) out.errors.push('"' + gone + '" survived switching multiple views off');
      }
      if (!rowFor('Multiple views')) out.errors.push('switching multiple views off also hid its own toggle');
      await flip('Multiple views');
      for (const back of ['Prompt in multiple views', 'View tab border width']) {
        if (!rowFor(back)) out.errors.push('"' + back + '" did not come back');
      }
    }
    // The same for a sub-toggle: the preview owns the highlighter and its size.
    if (!await expertRow('Markdown preview')) {
      out.errors.push('the Markdown preview toggle is missing from the expert menu');
    } else {
      if (!rowFor('Syntax colors in code blocks')) {
        out.errors.push('the highlighter toggle is missing although the preview is on');
      }
      await flip('Markdown preview');
      for (const gone of ['Syntax colors in code blocks', 'Detect code without fences', 'Code text size']) {
        if (rowFor(gone)) out.errors.push('"' + gone + '" survived switching the preview off');
      }
      await flip('Markdown preview');
      if (!rowFor('Detect code without fences')) out.errors.push('the code detection toggle did not come back');
    }
    const firstTab2 = document.querySelector('#expert-tabs-bar .expert-tab');
    if (firstTab2) { firstTab2.click(); await sleep(150); }
    await click('expert-back');
    document.getElementById('settings').classList.add('hidden');
    // The tab bar: one plain tab per view, a right-click menu with the move entries,
    // and a drag that only ever reorders.
    const viewTabs = () => [...document.querySelectorAll('#views .view-btn:not(.view-add-top)')];
    const tabIds = () => viewTabs().map(b => b.dataset.viewId);
    if (viewTabs().length !== 4) {
      out.errors.push('the tab bar shows ' + viewTabs().length + ' tabs instead of 4');
    } else {
      if (viewTabs().some(b => !b.dataset.viewId)) out.errors.push('a tab is not a plain view tab');
      if (document.querySelector('#views .view-group-btn')) out.errors.push('a folder tab is still drawn for views');
      const startOrder = tabIds();
      const tabCentre = (el) => { const r = el.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
      const tabPtr = (type, x, y, el) => (el || window).dispatchEvent(new PointerEvent(type, {
        bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 3, clientX: x, clientY: y }));
      if (getComputedStyle(viewTabs()[0]).userSelect !== 'none') {
        out.errors.push('a view tab still selects its text while being dragged');
      }
      // The strip stays hit-testable at every window size the app allows, so the
      // gesture is testable everywhere: no width may skip this.
      for (const tb of viewTabs()) {
        // The strip scrolls when squeezed, so a user brings a tab into view first.
        tb.scrollIntoView({ block: 'nearest', inline: 'nearest' });
        await sleep(40);
        const [cx, cy] = tabCentre(tb);
        const hit = document.elementFromPoint(cx, cy);
        if (!hit || hit.closest('.view-btn') !== tb) {
          out.errors.push('a view tab cannot be dropped on: nothing hit-tests to "' +
            tb.textContent.trim().slice(0, 12) + '"');
        }
      }
      // Context menu: grid size, edit, duplicate, move left, move right (no group entry).
      viewTabs()[0].dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(200);
      const menuBtns = [...document.querySelectorAll('.tab-menu:not(.hidden) button')];
      if (menuBtns.length !== 5) {
        out.errors.push('a tab menu has ' + menuBtns.length + ' entries instead of 5');
      } else {
        if (!menuBtns[3].disabled) out.errors.push('move-left is not disabled on the first tab');
        if (menuBtns[4].disabled) out.errors.push('move-right is disabled although another tab follows');
        menuBtns[4].click();
        await sleep(250);
        if (tabIds()[1] !== startOrder[0]) out.errors.push('move-right on the first tab did not move it one place over');
        // Move it back so the order below matches what the rest of the run expects.
        viewTabs()[1].dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
        await sleep(200);
        [...document.querySelectorAll('.tab-menu:not(.hidden) button')][3]?.click();
        await sleep(250);
        if (tabIds().join() !== startOrder.join()) out.errors.push('move-left did not restore the tab order');
      }
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      await sleep(150);
      // A drag onto the right half of another tab reorders, marks the edge and
      // never opens a naming dialog or groups anything.
      calls().length = 0;
      const dragFrom = viewTabs()[0];
      const dragTo = viewTabs()[2];
      dragTo.scrollIntoView({ block: 'nearest', inline: 'nearest' });
      await sleep(50);
      const [ax, ay] = tabCentre(dragFrom);
      const tr = dragTo.getBoundingClientRect();
      tabPtr('pointerdown', ax, ay, dragFrom);
      tabPtr('pointermove', ax + 8, ay);
      await sleep(80);
      tabPtr('pointermove', tr.right - 3, tr.top + tr.height / 2);
      await sleep(80);
      if (!document.querySelector('.tab-drop-edge.tab-drop-after')) out.errors.push('dragging onto a tab shows no reorder mark');
      if (document.querySelector('.tab-drop:not(.tab-drop-edge)')) out.errors.push('dragging onto a tab shows a merge mark');
      tabPtr('pointerup', tr.right - 3, tr.top + tr.height / 2);
      await sleep(350);
      if (!calls().includes('reorder_views')) out.errors.push('dropping on a tab did not reorder the tab bar');
      if (viewTabs().length !== 4) out.errors.push('a tab drag changed the number of tabs');
      const dropped = tabIds();
      if (dropped[2] !== startOrder[0] || dropped.slice().sort().join() !== startOrder.slice().sort().join()) {
        out.errors.push('the dragged tab did not land after its target (' + dropped.join(',') + ')');
      }
      if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
        out.errors.push('a tab drag opened a dialog');
        await click('confirm-cancel');
      }
      // Put the order back for the probes that follow.
      {
        const back = viewTabs().find(b => b.dataset.viewId === startOrder[0]);
        const first = viewTabs()[0];
        first.scrollIntoView({ block: 'nearest', inline: 'nearest' });
        await sleep(50);
        const [bx, by] = tabCentre(back);
        const fr = first.getBoundingClientRect();
        tabPtr('pointerdown', bx, by, back);
        tabPtr('pointermove', bx - 8, by);
        await sleep(80);
        tabPtr('pointermove', fr.left + 3, fr.top + fr.height / 2);
        await sleep(80);
        tabPtr('pointerup', fr.left + 3, fr.top + fr.height / 2);
        await sleep(350);
        if (tabIds().join() !== startOrder.join()) out.errors.push('dragging a tab back did not restore the tab order');
      }
      // The probes that follow want a view with free cells under them.
      viewTabs().find(b => b.dataset.viewId === 'archive')?.click();
      await sleep(300);
    }
    document.getElementById('settings').classList.remove('hidden');
    // Custom themes: the stored one is offered in the theme list, the editor opens
    // with a colour row per themable token, and editing previews on the spot.
    const themeSel = document.getElementById('theme-select');
    if (![...themeSel.options].some(o => o.value === 'custom:ct1')) {
      out.errors.push('the stored custom theme is missing from the theme list');
    }
    // 188: every built-in theme, at the app's own smallest window, must not clip
    // or overlap anything -- only worth the extra round trips at the tightest
    // size, so this skips at the other three viewports this same probe also runs at.
    // Repeatedly switching this select has been observed to stall the headless
    // browser's virtual clock (the same class of harness limitation documented
    // at 193 and 186) -- one switch to the widest-risk theme (monospace font,
    // the one most likely to push text past a tight boundary) stands in for
    // cycling all eight.
    // 188: the widest layout risk a theme can add is a font-family change
    // (Programmer switches the whole UI to monospace, which is wider per
    // character and the one real way a theme swap could newly clip text) --
    // checked by setting the same data-theme attribute applyTheme() itself
    // sets, not by driving the live select: repeatedly switching that control
    // has been observed to stall the headless browser's virtual clock (the
    // same class of harness limitation documented at 193 and 186), and a raw
    // attribute set proves the identical CSS effect without touching it.
    if (innerWidth <= 700) {
      const startTheme = document.documentElement.getAttribute('data-theme');
      document.documentElement.setAttribute('data-theme', 'programmer');
      layoutScan('theme programmer');
      document.documentElement.setAttribute('data-theme', startTheme || 'system');
    }
    // 277: each built-in theme bakes its own --select-chevron (a data-URI SVG
    // cannot hold a var(), so the stroke colour is substituted per theme
    // instead). Checked here, right after the settings dialog is shown, so
    // #lang-select is a real visible <select> and not one buried inside an
    // already-closed modal elsewhere in this probe.
    {
      const priorTheme2 = document.documentElement.getAttribute('data-theme');
      const BUILTIN_THEMES2 = ['light', 'dark', 'programmer', 'neon', 'cyberpunk', 'mono', 'ocean'];
      const chevronByTheme = {};
      for (const theme of BUILTIN_THEMES2) {
        document.documentElement.setAttribute('data-theme', theme);
        chevronByTheme[theme] = getComputedStyle(document.getElementById('lang-select')).backgroundImage;
      }
      if (priorTheme2) document.documentElement.setAttribute('data-theme', priorTheme2);
      else document.documentElement.removeAttribute('data-theme');
      const vals = Object.values(chevronByTheme);
      for (const [theme, v] of Object.entries(chevronByTheme)) {
        if (!v || v === 'none') out.errors.push('select chevron in theme ' + theme + ': no background-image at all');
        if (/888888/.test(v)) out.errors.push('select chevron in theme ' + theme + ': still the hardcoded grey');
      }
      if (new Set(vals).size !== BUILTIN_THEMES2.length) {
        out.errors.push('select chevron: expected ' + BUILTIN_THEMES2.length + ' distinct colours across built-in themes, got ' + new Set(vals).size);
      }
    }
    // 508: every select in the main window carries a real border and a
    // themed, non-transparent background, and its option list uses the
    // theme surface colour, in every built-in theme. A bare select used to
    // fall back to the plain OS look because the global select rule left
    // border and background unset.
    {
      const priorTheme3 = document.documentElement.getAttribute('data-theme');
      const SELECT_THEMES = ['light', 'dark', 'programmer', 'neon', 'cyberpunk', 'mono', 'ocean'];
      const mainSelects = [...document.querySelectorAll('select')].filter((s) => s.options.length > 0);
      if (!mainSelects.length) out.errors.push('508: no populated select found in the main window to audit');
      const swatch = document.createElement('div');
      swatch.style.cssText = 'position:absolute;left:-9999px;top:-9999px;background-color:var(--surface)';
      document.body.appendChild(swatch);
      for (const theme of SELECT_THEMES) {
        document.documentElement.setAttribute('data-theme', theme);
        const surfaceRgb = getComputedStyle(swatch).backgroundColor;
        for (const sel of mainSelects) {
          const label = sel.id || sel.className || 'unnamed select';
          const cs = getComputedStyle(sel);
          if (parseFloat(cs.borderTopWidth) <= 0) {
            out.errors.push('508: ' + label + ' has no border in theme ' + theme);
          }
          // batch-view-select is deliberately transparent (it sits inline with
          // ghost-btn neighbours and its native list is replaced by a themed
          // popup, see the comment above its rule) -- not the bare-select
          // defect this point is about.
          if (!sel.classList.contains('batch-view-select')) {
            if (cs.backgroundColor === 'rgba(0, 0, 0, 0)' || cs.backgroundColor === 'transparent') {
              out.errors.push('508: ' + label + ' has a transparent background in theme ' + theme);
            }
          }
          // A selected option shows the accent colour (option:checked); pick
          // an unselected one to check the plain surface colour instead.
          const unselected = [...sel.options].find((o) => !o.selected);
          if (unselected) {
            const optBg = getComputedStyle(unselected).backgroundColor;
            if (optBg !== surfaceRgb) {
              out.errors.push('508: ' + label + ' option background is ' + optBg + ', expected the theme surface ' + surfaceRgb + ' in theme ' + theme);
            }
          }
        }
      }
      swatch.remove();
      if (priorTheme3) document.documentElement.setAttribute('data-theme', priorTheme3);
      else document.documentElement.removeAttribute('data-theme');
    }
    await click('theme-edit-open');
    const tmodal = document.getElementById('theme-modal');
    if (tmodal.classList.contains('hidden')) {
      out.errors.push('the theme editor did not open');
    } else {
      const rows = [...document.querySelectorAll('#theme-edit-vars .theme-var-row')];
      if (rows.length !== 9) out.errors.push('the theme editor shows ' + rows.length + ' colour rows instead of 9');
      if (document.querySelectorAll('#theme-edit-vars .theme-var-group').length !== 4) {
        out.errors.push('the theme colours are not split into four groups');
      }
      const hex = rows[0] && rows[0].querySelector('.theme-hex');
      if (!hex) {
        out.errors.push('a theme colour row has no hex field');
      } else {
        const original = hex.value;
        // A valid value applies live; an invalid one is rejected and rolled back.
        hex.value = '#123456';
        hex.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(120);
        if (getComputedStyle(document.documentElement).getPropertyValue('--bg').trim() !== '#123456') {
          out.errors.push('editing a colour does not preview live');
        }
        hex.value = 'nonsense';
        hex.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(120);
        if (hex.value !== '#123456') out.errors.push('an invalid hex value was not rejected');
        // D75 part 2: "Revert changes" undoes everything since the editor opened —
        // WITHOUT closing it, the one thing that actually sets it apart from Cancel.
        await click('theme-edit-revert');
        await sleep(120);
        if (tmodal.classList.contains('hidden')) {
          out.errors.push('reverting the theme draft closed the editor');
        }
        // renderThemeEditor() rebuilt the rows, so the old references are stale.
        const revertedRows = [...document.querySelectorAll('#theme-edit-vars .theme-var-row')];
        const revertedHex = revertedRows[0] && revertedRows[0].querySelector('.theme-hex');
        if (!revertedHex || revertedHex.value !== original) {
          out.errors.push('reverting did not restore the original colour: ' + (revertedHex && revertedHex.value));
        }
        if (getComputedStyle(document.documentElement).getPropertyValue('--bg').trim() === '#123456') {
          out.errors.push('reverting did not undo the live preview');
        }
      }
      // 277: a custom theme's own --muted must re-derive the select chevron too,
      // not just apply directly like the other colour tokens (a data-URI SVG
      // cannot hold a var(), so this is its own derivation path). Re-queries the
      // rows fresh -- the earlier revert rebuilt them, so `rows` above is stale.
      {
        const freshRows = [...document.querySelectorAll('#theme-edit-vars .theme-var-row')];
        const mutedHex = freshRows[5] && freshRows[5].querySelector('.theme-hex');
        if (!mutedHex) {
          out.errors.push('the muted colour row is missing');
        } else {
          mutedHex.value = '#334455';
          mutedHex.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(120);
          const chevron = getComputedStyle(document.documentElement).getPropertyValue('--select-chevron');
          if (!/334455/i.test(chevron)) {
            out.errors.push('a custom theme muted colour does not re-derive the select chevron: ' + chevron);
          }
          await click('theme-edit-revert');
          await sleep(120);
        }
      }
      // 76: switch to the stored theme (ct1) so it has a real id — the editor
      // opened on a fresh, unsaved draft by default, which correctly has none.
      const targetSel = document.getElementById('theme-edit-target');
      targetSel.value = 'ct1';
      targetSel.dispatchEvent(new Event('change', { bubbles: true }));
      await sleep(120);
      const exportBtn = document.getElementById('theme-edit-export');
      if (!exportBtn || exportBtn.classList.contains('hidden')) {
        out.errors.push('the export button is missing/hidden for a saved theme');
      } else {
        exportBtn.click();
        await sleep(120);
        const call = [...(window.__SMOKE_ARGS__ || [])].reverse().find(([c]) => c === 'export_custom_theme');
        if (!call || call[1].id !== 'ct1') out.errors.push('export did not pass the current theme id');
      }
      document.getElementById('theme-edit-import').click();
      await sleep(150);
      if (document.getElementById('theme-edit-target').value !== 'ct2') {
        out.errors.push('importing a theme did not open it in the editor');
      }
      if (document.getElementById('theme-edit-name').value !== 'Imported theme') {
        out.errors.push('the imported theme did not fill the editor with its own values');
      }
      if (exportBtn && exportBtn.classList.contains('hidden')) {
        out.errors.push('the export button stayed hidden after importing a (now saved) theme');
      }
      if (![...themeSel.options].some(o => o.value === 'custom:ct2')) {
        out.errors.push('the imported theme is missing from the theme list');
      }
      await click('theme-edit-cancel');
      await sleep(150);
      if (document.documentElement.style.getPropertyValue('--bg') === '#123456') {
        out.errors.push('cancelling the theme editor kept the preview');
      }
    }
    layoutScan('settings');
    mustReach('settings', '#settings .modal-x, #settings-main > .field > button');
    await click('expert-open');
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click();
      await sleep(200);
      out.tabs.push(tb.textContent.trim() + ':' +
        document.querySelectorAll('#expert-flags .expert-group').length);
      layoutScan('expert tab ' + tb.textContent.trim());
      // 101: paired two-up, a dropdown row stacks its label above the field (a
      // 240px column has no room for label-beside-field) -- checked wherever the
      // grid actually produces a pair, which needs no fixed pixel width of its own.
      for (const row of document.querySelectorAll('.expert-cols > .expert-dropdown-row')) {
        const label = row.querySelector('.value-head');
        const field = row.querySelector('.expert-select');
        if (!label || !field) continue;
        const lb = label.getBoundingClientRect();
        const fb = field.getBoundingClientRect();
        if (fb.top < lb.bottom - 1) {
          out.errors.push('101: paired dropdown row label is not above its field in ' + tb.textContent.trim());
        }
      }
    }
    mustReach('expert menu', '#expert-tabs-bar .expert-tab');
    // P69: a default-off feature gets a small dot badge instead of a "(Default
    // off)" text suffix crowding the label — find the rows across all tabs.
    {
      const rowByLabel = (text) => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('#expert-flags .switch-field')]
            .find(r => (r.querySelector('span') || {}).textContent === text);
          if (hit) return hit;
        }
        return null;
      };
      const offRow = rowByLabel('Always show grid lines');
      const onRow = rowByLabel('Media captions');
      const badge = offRow && offRow.querySelector('.default-off-badge');
      if (!badge || !badge.title.trim()) {
        out.errors.push('a default-off flag row has no badge (or no tooltip)');
      }
      if (offRow && offRow.querySelector('span').textContent.includes('Default off')) {
        out.errors.push('the default-off suffix is still text on the label');
      }
      if (onRow && onRow.querySelector('.default-off-badge')) {
        out.errors.push('a default-ON flag row wrongly carries the badge');
      }
      // 172: the badge must sit vertically centred on the same line as the toggle.
      const toggle = offRow && offRow.querySelector('.switch');
      if (badge && toggle) {
        const bMid = badge.getBoundingClientRect().top + badge.getBoundingClientRect().height / 2;
        const tMid = toggle.getBoundingClientRect().top + toggle.getBoundingClientRect().height / 2;
        if (Math.abs(bMid - tMid) > 1.5) {
          out.errors.push('default-off badge is not vertically centred with the switch (off by ' + Math.abs(bMid - tMid).toFixed(1) + 'px)');
        }
      }
    }
    // 372: expGroupCreate was split into expTabCapture's finer groups — a moved
    // setting only changed its group, never its key, so a value set before the
    // split must still read back correctly after it (and survive a re-render).
    {
      const dropdownRowByLabel = (text) => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('#expert-flags .expert-dropdown-row')]
            .find(r => (r.querySelector('.value-head span') || {}).textContent === text);
          if (hit) return hit;
        }
        return null;
      };
      const row = dropdownRowByLabel('Color notation'); // pickerFormat, moved into expGroupPicker
      const sel = row && row.querySelector('select');
      if (!sel) {
        out.errors.push('372: the colour picker format dropdown is missing from the Capture tab');
      } else {
        sel.value = 'rgb';
        sel.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(120);
        const wrote = [...(window.__SMOKE_ARGS__ || [])]
          .some(([c, a]) => c === 'set_ui_text' && a.key === 'pickerFormat' && a.value === 'rgb');
        if (!wrote) out.errors.push('372: changing the colour picker format did not persist');
        const row2 = dropdownRowByLabel('Color notation'); // re-finds it after switching tabs away and back
        const sel2 = row2 && row2.querySelector('select');
        if (!sel2 || sel2.value !== 'rgb') {
          out.errors.push('372: the colour picker format did not survive a re-render after the group split');
        }
        if (sel2) {
          sel2.value = 'hex';
          sel2.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(120);
        }
      }
    }
    // 373: closeOnCopy was renamed to hideWindowOnCopy -- a rename
    // silently breaks if the CSS class the flag drives on <body> is missed,
    // so flip each switch and check the class right on the body element.
    {
      const flagRowByLabel = (text) => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('#expert-flags .switch-field')]
            .find(r => (r.querySelector('span') || {}).textContent === text);
          if (hit) return hit;
        }
        return null;
      };
      const checkRenamedFlag = async (label, cls) => {
        const row = flagRowByLabel(label);
        const cb = row && row.querySelector('input.switch');
        if (!cb) { out.errors.push(`373: no switch found for "${label}"`); return; }
        const was = cb.checked;
        cb.checked = !was;
        cb.dispatchEvent(new Event('change'));
        await sleep(80);
        if (document.body.classList.contains(cls) !== !was) {
          out.errors.push(`373: body class "${cls}" did not flip with "${label}"`);
        }
        cb.checked = was; // restore, later probes assume the shipped default
        cb.dispatchEvent(new Event('change'));
        await sleep(80);
      };
      await checkRenamedFlag('Hide window after copy', 'flag-hideWindowOnCopy');
    }
    // P68: the expert-menu search also matches a word that lives only in a
    // setting's tooltip, not its label.
    {
      const search = document.getElementById('expert-search');
      if (!search) {
        out.errors.push('the expert menu has no search field');
      } else {
        search.value = 'arranging'; // only in tipGridLines, not in the "Always show grid lines" label
        search.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(200);
        const found = [...document.querySelectorAll('#expert-flags .switch-field span')]
          .some(s => s.textContent === 'Always show grid lines');
        if (!found) out.errors.push('the expert search did not match a tooltip-only word');
        search.value = 'zzz_no_such_setting_zzz';
        search.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(200);
        if (document.querySelectorAll('#expert-flags .expert-group').length) {
          out.errors.push('a nonsense expert search still shows groups');
        }
        search.value = '';
        search.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(200);
      }
    }
    // 403: the expert-menu search also finds a hit outside the expert menu --
    // a normal-settings-page row here -- and clicking it leaves the expert
    // page, reveals the right dialog and flashes the right row.
    {
      const search = document.getElementById('expert-search');
      // The background-start switch only exists while autostart is on: hidden
      // with it off, never offered by the search, shown again once it is on.
      {
        const autostartEl = document.getElementById('opt-autostart');
        const startminRow = document.getElementById('opt-startmin').closest('.field');
        const startminHit = () => [...document.querySelectorAll('.expert-foreign-row')]
          .find(r => (r.querySelector('span') || {}).textContent === 'Start in the background on autostart');
        const typeSearch = async (q) => { search.value = q; search.dispatchEvent(new Event('input', { bubbles: true })); await sleep(200); };
        if (autostartEl.checked) out.errors.push('659: autostart is on in the fresh fixture');
        if (!startminRow.classList.contains('hidden')) out.errors.push('659: the background-start row shows while autostart is off');
        await typeSearch('start in the background');
        if (startminHit()) out.errors.push('659: the search offers the hidden background-start row');
        autostartEl.checked = true;
        autostartEl.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(200);
        if (startminRow.classList.contains('hidden')) out.errors.push('659: the background-start row stays hidden after autostart is switched on');
        await typeSearch('start in the background');
        if (!startminHit()) out.errors.push('659: the search misses the background-start row while autostart is on');
        autostartEl.checked = false;
        autostartEl.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(200);
        if (!startminRow.classList.contains('hidden')) out.errors.push('659: the background-start row stays visible after autostart is switched off');
      }
      search.value = 'auto-paste';
      search.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(200);
      const foreignRow = [...document.querySelectorAll('.expert-foreign-row')]
        .find(r => (r.querySelector('span') || {}).textContent === 'Auto-paste on double-click');
      if (!foreignRow) {
        out.errors.push('403: search for a normal-settings-only word found no foreign hit');
      } else {
        if (!foreignRow.querySelector('.expert-foreign-badge')) {
          out.errors.push('403: a foreign hit carries no area badge');
        }
        foreignRow.click();
        await sleep(250);
        if (!document.getElementById('settings-expert').classList.contains('hidden')) {
          out.errors.push('403: clicking a foreign hit did not leave the expert page');
        }
        if (document.getElementById('settings-main').classList.contains('hidden')) {
          out.errors.push('403: clicking a foreign hit did not reveal the normal settings page');
        }
        if (!document.getElementById('opt-autopaste').closest('.field').classList.contains('settings-search-hit')) {
          out.errors.push('403: clicking a foreign hit did not flash the target row');
        }
      }
      // Back into the expert menu for the rest of this walk.
      document.getElementById('expert-open').click();
      await sleep(150);
      // A hit that lives in the backup manager -- a dialog of its own, not the
      // settings page -- must close Settings entirely and open that dialog.
      const search2 = document.getElementById('expert-search');
      search2.value = 'smart retention';
      search2.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(200);
      const backupRow = [...document.querySelectorAll('.expert-foreign-row')]
        .find(r => (r.querySelector('span') || {}).textContent === 'Smart retention');
      if (!backupRow) {
        out.errors.push('403: search for a backup-only word found no foreign hit');
      } else {
        backupRow.click();
        await sleep(400); // renderBackupPanel() awaits a real invoke round-trip
        if (!document.getElementById('settings').classList.contains('hidden')) {
          out.errors.push('403: clicking a backup hit left the settings dialog open');
        }
        if (document.getElementById('backup-modal').classList.contains('hidden')) {
          out.errors.push('403: clicking a backup hit did not open the backup manager');
        }
        const gfsTarget = document.querySelector('#backup-panel [data-setting-row="backup-gfs"]');
        if (!gfsTarget || !gfsTarget.classList.contains('settings-search-hit')) {
          out.errors.push('403: clicking a backup hit did not flash the target row');
        }
        document.getElementById('backup-modal').classList.add('hidden'); // close it, back to settings
      }
      document.getElementById('gear').click();
      await sleep(150);
      document.getElementById('expert-open').click();
      await sleep(150);
      const search3 = document.getElementById('expert-search');
      search3.value = 'zzz_totally_unmatched_zzz';
      search3.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(200);
      const hint = document.querySelector('#expert-flags .hint');
      if (!hint || !hint.textContent) {
        out.errors.push('403: a search with no hits anywhere shows no explanatory message');
      }
      search3.value = '';
      search3.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(150);
    }
    // 343: the recording stop/pause hotkeys moved out of Settings and into the
    // expert menu itself, as a real capture row (not a jump-to-settings button).
    {
      if (document.getElementById('rec-stop-hotkey-row') || document.getElementById('rec-pause-hotkey-row')) {
        out.errors.push('343: the recording hotkey rows are still in the Settings dialog');
      }
      const rowByLabel = (text) => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('#expert-flags .hotkey-field')]
            .find(r => (r.querySelector('.value-head span') || {}).textContent === text);
          if (hit) return hit;
        }
        return null;
      };
      const stopRow = rowByLabel('Stop-recording hotkey');
      const pauseRow = rowByLabel('Pause/resume-recording hotkey');
      if (!stopRow || !pauseRow) {
        out.errors.push('343: the expert menu has no capture row for one or both recording hotkeys');
      } else {
        const input = stopRow.querySelector('.hotkey-row .modal-input');
        const clearBtn = stopRow.querySelector('.hotkey-row .ghost-btn');
        if (!input || !/8$/.test(input.value)) {
          out.errors.push('343: the stop-recording row does not show its stored hotkey');
        }
        if (!clearBtn) out.errors.push('343: the stop-recording row has no Clear button');
        // Turning its own flag off must drop the row, same as every other gated setting.
        // Re-queried fresh after each click rather than reused: flipping the flag
        // rebuilds the whole group, so a reference taken before the click would be
        // detached from the document by the time this reads or clicks it again.
        const switchRowByLabel = (text) => {
          for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
            tb.click();
            const hit = [...document.querySelectorAll('#expert-flags .switch-field')]
              .find(r => (r.querySelector('span') || {}).textContent === text);
            if (hit) return hit;
          }
          return null;
        };
        const stopFlagRow = switchRowByLabel('Stop-recording hotkey');
        if (!stopFlagRow) {
          out.errors.push('343: no flag toggle found for the stop-recording hotkey');
        } else {
          stopFlagRow.querySelector('.switch').click();
          await sleep(80);
          if (rowByLabel('Stop-recording hotkey')) {
            out.errors.push('343: the stop-recording capture row stayed visible after its own flag was switched off');
          }
          switchRowByLabel('Stop-recording hotkey')?.querySelector('.switch').click(); // restore
          await sleep(80);
        }
      }
    }
    // 67: "only changed" filter — a flag at its default is hidden, flipping it in
    // makes the row (re)appear, flipping it back to default hides it again.
    {
      const onlyChanged = document.getElementById('expert-only-changed');
      // Unlike the badge-lookup above, this also has to work while "only changed" is
      // checked — the tab bar hides then (every tab is scanned in one flat pass, see
      // renderExpert's globalView), so there are no `.expert-tab` buttons to click.
      const rowByLabel = (text) => {
        const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
        const find = () => [...document.querySelectorAll('#expert-flags .switch-field')]
          .find(r => ((r.querySelector('span') || {}).textContent || '').includes(text)) || null;
        if (!tabs.length) return find();
        for (const tb of tabs) {
          tb.click();
          const hit = find();
          if (hit) return hit;
        }
        return null;
      };
      if (!onlyChanged) {
        out.errors.push('the expert menu has no "only changed" checkbox');
      } else {
        // Not asserting an empty filtered view here: by this point in the suite
        // other blocks have already toggled several real settings away from their
        // defaults, so "nothing changed yet" would be a false assumption. The
        // self-contained check below (flip one flag, watch it appear, flip it back,
        // watch it vanish) proves the filter without needing a clean slate.
        onlyChanged.checked = false;
        onlyChanged.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(150);
        // "Always show grid lines" ships off — flip it on, off is its default.
        const row = rowByLabel('Always show grid lines');
        const sw = row && row.querySelector('input.switch');
        if (!sw) {
          out.errors.push('could not find a flag row to drive the "only changed" test');
        } else {
          sw.click();
          await sleep(150);
          onlyChanged.checked = true;
          onlyChanged.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(200);
          const changedRow = rowByLabel('Always show grid lines');
          if (!changedRow) {
            out.errors.push('"only changed" did not surface a flag flipped away from its default');
          }
          // Flip it straight back to its default — must vanish from the filtered view.
          const swAgain = rowByLabel('Always show grid lines')?.querySelector('input.switch');
          if (swAgain) swAgain.click();
          await sleep(150);
          if (rowByLabel('Always show grid lines')) {
            out.errors.push('"only changed" kept showing a flag restored to its default');
          }
          onlyChanged.checked = false;
          onlyChanged.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(150);
        }
      }
    }
    // 71: per-category reset — resets only the settings that belong to the group
    // whose button was clicked, a flag changed in a DIFFERENT group must survive.
    {
      const rowByLabel2 = (text) => {
        const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
        const find = () => [...document.querySelectorAll('#expert-flags .switch-field')]
          .find(r => ((r.querySelector('span') || {}).textContent || '').includes(text)) || null;
        if (!tabs.length) return find();
        for (const tb of tabs) { tb.click(); const hit = find(); if (hit) return hit; }
        return null;
      };
      // Two flags in the SAME group (Tiles > Tile look) + one in a different group
      // (Library) entirely, to prove the reset button stays scoped to its own group.
      // Grid + caption are looked up from ONE tab visit (rowByLabel2 for each
      // separately would switch tabs in between and click a detached, already
      // replaced checkbox for the second one).
      rowByLabel2('Always show grid lines'); // lands on the Tiles tab
      const gridSw = [...document.querySelectorAll('#expert-flags .switch-field')]
        .find(r => (r.querySelector('span') || {}).textContent === 'Always show grid lines')
        ?.querySelector('input.switch');
      const captionSw = [...document.querySelectorAll('#expert-flags .switch-field')]
        .find(r => (r.querySelector('span') || {}).textContent === 'Media captions')
        ?.querySelector('input.switch');
      if (!gridSw || !captionSw) {
        out.errors.push('could not find the same-group rows needed for the "reset category" test');
      }
      gridSw && gridSw.click(); await sleep(120);
      captionSw && captionSw.click(); await sleep(120);
      const otherSw = rowByLabel2('Smart sort (usage-based)')?.querySelector('input.switch');
      if (!gridSw || !captionSw || !otherSw) {
        out.errors.push('could not find the rows needed for the "reset category" test');
      } else {
        otherSw.click(); await sleep(120);
        const otherWasChecked = otherSw.checked;
        const gridRowNow = rowByLabel2('Always show grid lines');
        const groupSec = gridRowNow?.closest('.expert-group');
        const resetBtn = groupSec && groupSec.querySelector('.expert-group-reset');
        if (!resetBtn) {
          out.errors.push('the "reset category" button did not appear for a changed group');
        } else {
          resetBtn.click();
          await sleep(150);
          const cm = document.getElementById('confirm-modal');
          if (cm.classList.contains('hidden')) {
            out.errors.push('"reset category" did not ask for confirmation');
          } else {
            document.getElementById('confirm-ok').click();
            await sleep(300);
            const gridAfter = rowByLabel2('Always show grid lines')?.querySelector('input.switch');
            const captionAfter = rowByLabel2('Media captions')?.querySelector('input.switch');
            const otherAfter = rowByLabel2('Smart sort (usage-based)')?.querySelector('input.switch');
            if (!gridAfter || gridAfter.checked) {
              out.errors.push('"reset category" left a flag in the reset category still non-default');
            }
            if (!captionAfter || !captionAfter.checked) {
              out.errors.push('"reset category" left the other changed flag in the same category non-default');
            }
            if (!otherAfter || otherAfter.checked !== otherWasChecked) {
              out.errors.push('"reset category" touched a setting outside its own category');
            }
            // Restore the cross-group control flag so later blocks see the default state.
            if (otherAfter && otherAfter.checked !== undefined) {
              const backSw = rowByLabel2('Smart sort (usage-based)')?.querySelector('input.switch');
              if (backSw && backSw.checked === otherWasChecked) { backSw.click(); await sleep(120); }
            }
          }
        }
      }
    }
    // 405: a group reset must also clear a row currently hidden behind its own
    // gate -- otherwise a changed value survives, invisible, behind a
    // switched-off toggle.
    {
      const rowByLabel5 = (text) => {
        const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
        const find = () => [...document.querySelectorAll('#expert-flags .switch-field')]
          .find(r => ((r.querySelector('span') || {}).textContent || '') === text) || null;
        if (!tabs.length) return find();
        for (const tb of tabs) { tb.click(); const hit = find(); if (hit) return hit; }
        return null;
      };
      let historySw = rowByLabel5('Prompt version history')?.querySelector('input.switch');
      if (!historySw) {
        out.errors.push('could not find "Prompt version history" for the gated-hidden-row reset test');
      } else {
        if (!historySw.checked) { historySw.click(); await sleep(120); } // reveal the gated child
        const diffSw = rowByLabel5('Word-by-word comparison in history')?.querySelector('input.switch');
        if (!diffSw) {
          out.errors.push('could not find the gated "Word-by-word comparison in history" row');
        } else {
          if (diffSw.checked) { diffSw.click(); await sleep(120); } // move it away from its default (on)
          // Hide it again by switching its own parent back off.
          rowByLabel5('Prompt version history')?.querySelector('input.switch')?.click();
          await sleep(120);
          if (rowByLabel5('Word-by-word comparison in history')) {
            out.errors.push('405 setup: the gated row is still visible, cannot test the hidden-row case');
          } else {
            const groupSec = rowByLabel5('Prompt version history')?.closest('.expert-group');
            const resetBtn = groupSec && groupSec.querySelector('.expert-group-reset');
            if (!resetBtn) {
              out.errors.push('405: the group reset button did not appear although a row hidden behind its own gate differs from its default');
            } else {
              // This group also holds "Markdown preview", deliberately switched
              // on by an earlier block (F14/F15) and still relied on by a much
              // later one (the voice-dictation session test) -- a real group
              // reset resets the WHOLE group, so record it here and put it back
              // afterwards rather than letting this probe silently undo another
              // block's setup.
              const mdWasOn = rowByLabel5('Markdown preview')?.querySelector('input.switch')?.checked;
              resetBtn.click();
              await sleep(150);
              document.getElementById('confirm-ok').click();
              await sleep(300);
              const historySwAfter = rowByLabel5('Prompt version history')?.querySelector('input.switch');
              if (!historySwAfter || !historySwAfter.checked) {
                out.errors.push('405: group reset did not restore the parent flag to its own default');
              }
              const diffSwAfter = rowByLabel5('Word-by-word comparison in history')?.querySelector('input.switch');
              if (!diffSwAfter || !diffSwAfter.checked) {
                out.errors.push('405: group reset left a row that was hidden behind its own gate still changed from its default');
              }
              if (mdWasOn) {
                const mdSwAfter = rowByLabel5('Markdown preview')?.querySelector('input.switch');
                if (mdSwAfter && !mdSwAfter.checked) { mdSwAfter.click(); await sleep(120); }
              }
            }
          }
        }
      }
    }
    // 405: the group reset button must never appear for "Data & system"
    // (expGroupData) -- it sits right next to one-click destructive actions
    // (emergency purge, delete all data), so it is ruled out entirely,
    // regardless of whether a real setting in that group differs from default.
    {
      const rowByLabel6 = (text) => {
        const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
        const find = () => [...document.querySelectorAll('#expert-flags .switch-field')]
          .find(r => ((r.querySelector('span') || {}).textContent || '') === text) || null;
        if (!tabs.length) return find();
        for (const tb of tabs) { tb.click(); const hit = find(); if (hit) return hit; }
        return null;
      };
      const dupSw = rowByLabel6('Duplicate finder')?.querySelector('input.switch');
      if (!dupSw) {
        out.errors.push('could not find "Duplicate finder" for the expGroupData no-reset-button test');
      } else {
        dupSw.click();
        await sleep(120);
        const groupSec = rowByLabel6('Duplicate finder')?.closest('.expert-group');
        if (groupSec && groupSec.querySelector('.expert-group-reset')) {
          out.errors.push('405: "Data & system" shows a group reset button, which must never appear there');
        }
        const dupSw2 = rowByLabel6('Duplicate finder')?.querySelector('input.switch');
        if (dupSw2) { dupSw2.click(); await sleep(120); } // restore
      }
    }
    // 31: per-value reset — a row's own undo icon appears only once IT changes,
    // resets just that row, then disappears again.
    {
      const rowByLabel4 = (text) => {
        const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
        const find = () => [...document.querySelectorAll('#expert-flags .switch-field')]
          .find(r => ((r.querySelector('span') || {}).textContent || '').includes(text)) || null;
        if (!tabs.length) return find();
        for (const tb of tabs) { tb.click(); const hit = find(); if (hit) return hit; }
        return null;
      };
      const row = rowByLabel4('Always show grid lines');
      const sw = row && row.querySelector('input.switch');
      if (!sw) {
        out.errors.push('could not find a flag row for the per-value reset test');
      } else {
        const shown = (r) => { const b = r && r.querySelector('.expert-row-reset'); return b && !b.classList.contains('hidden') ? b : null; };
        if (shown(row)) {
          out.errors.push('per-value reset icon showed on an already-default row');
        }
        const before = sw.checked;
        // 173: the icon must flip WITHOUT a menu rebuild -- checked on the SAME row
        // node (no re-query) right after the plain toggle, before any reset click
        // (a reset click itself still goes through resetOneExpertValue's own
        // renderExpert(), which is a separate, already-working code path).
        sw.click();
        await sleep(150);
        const resetBtn = shown(row);
        if (!resetBtn) {
          out.errors.push('per-value reset icon did not appear live on a changed row (no rebuild)');
        } else {
          resetBtn.click();
          await sleep(200);
          const rowAfter = rowByLabel4('Always show grid lines');
          const swAfter = rowAfter && rowAfter.querySelector('input.switch');
          if (!swAfter || swAfter.checked !== before) {
            out.errors.push('per-value reset did not restore the row to its default');
          }
          if (shown(rowAfter)) {
            out.errors.push('per-value reset icon stayed visible after resetting');
          }
        }
      }
    }
    // Prompt-folders-as-Markdown: the sync-folder path row is gated on the feature
    // flag, same contract every other gated expert row already follows. Checked via
    // the backend call each click makes, not the live DOM — cross-tab lookups here
    // proved to hit the same detached-checkbox harness quirk seen in 71's test.
    {
      const rowByLabel3 = (text, sel) => {
        const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
        const find = () => [...document.querySelectorAll('#expert-flags ' + sel)]
          .find(r => ((r.querySelector('span') || {}).textContent || '').includes(text)) || null;
        if (!tabs.length) return find();
        for (const tb of tabs) { tb.click(); const hit = find(); if (hit) return hit; }
        return null;
      };
      const syncRow = rowByLabel3('as Markdown files', '.switch-field');
      const syncSw = syncRow && syncRow.querySelector('input.switch');
      if (!syncSw) {
        out.errors.push('the prompt-sync toggle is missing from the expert menu');
      } else {
        if (syncSw.checked) out.errors.push('prompt sync is not off by default');
        if (rowByLabel3('Sync folder', '.path-field')) {
          out.errors.push('the sync-folder path shows while prompt sync is off');
        }
        window.__SMOKE_ARGS__ = [];
        syncSw.checked = true;
        syncSw.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(150);
        // 17: turning sync on now warns about the plaintext .md files first, via the
        // shared confirm modal, before set_ui_flag is ever called.
        const syncCm = document.getElementById('confirm-modal');
        if (syncCm.classList.contains('hidden')) {
          out.errors.push('turning on prompt sync did not show the plaintext warning');
        } else {
          if (!/plain text|Klartext/i.test(document.getElementById('confirm-msg').textContent)) {
            out.errors.push('the prompt-sync confirm dialog does not mention plain text');
          }
          document.getElementById('confirm-ok').click();
          await sleep(150);
        }
        const flagCall = window.__SMOKE_ARGS__.find(([cmd, args]) => cmd === 'set_ui_flag' && args && args.key === 'promptSync');
        if (!flagCall || flagCall[1].enabled !== true) {
          out.errors.push('turning on prompt sync did not call set_ui_flag(promptSync, true)');
        }
        if (!rowByLabel3('Sync folder', '.path-field')) {
          out.errors.push('the sync-folder path did not appear once prompt sync was turned on');
        }
        // Restore the default so later blocks (and a real re-run of this suite)
        // see the same starting state.
        const backSw = rowByLabel3('as Markdown files', '.switch-field')?.querySelector('input.switch');
        if (backSw) { backSw.checked = false; backSw.dispatchEvent(new Event('change', { bubbles: true })); }
        await sleep(120);
      }
    }
    // 32: rotating-prompts flag gates its default-mode dropdown, max-entries slider
    // and badge sub-flag — same contract every other gated expert row follows.
    {
      const rowByLabel5 = (text, sel) => {
        const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
        const find = () => [...document.querySelectorAll('#expert-flags ' + sel)]
          .find(r => ((r.querySelector('.value-head span, span') || {}).textContent || '').includes(text)) || null;
        if (!tabs.length) return find();
        for (const tb of tabs) { tb.click(); const hit = find(); if (hit) return hit; }
        return null;
      };
      const rotSw = rowByLabel5('Rotating', '.switch-field')?.querySelector('input.switch');
      if (!rotSw) {
        out.errors.push('the rotating-prompts toggle is missing from the expert menu');
      } else {
        // 33: default flipped ON (was off at 32's original ship).
        if (!rotSw.checked) out.errors.push('rotating prompts is not on by default (33)');
        if (!rowByLabel5('starts in', '.expert-dropdown-row')) {
          out.errors.push('the rotation-mode dropdown does not show while rotating prompts is on by default');
        }
        if (!rowByLabel5('Max. entries per', '.value-field')) {
          out.errors.push('the max-entries slider does not show while rotating prompts is on by default');
        }
        const badgeSw = rowByLabel5('Show the next-entry badge', '.switch-field')?.querySelector('input.switch');
        if (!badgeSw) {
          out.errors.push('the badge sub-flag does not show while rotating prompts is on by default');
        } else if (!badgeSw.checked) {
          out.errors.push('the next-entry badge is not on by default');
        }
        // Turn it off and confirm the gate hides its dependents too (same contract
        // every other gated expert row follows), then restore the default-on state.
        window.__SMOKE_ARGS__ = [];
        rotSw.checked = false;
        rotSw.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(150);
        const flagCall = window.__SMOKE_ARGS__.find(([cmd, args]) => cmd === 'set_ui_flag' && args && args.key === 'rotatingPrompts');
        if (!flagCall || flagCall[1].enabled !== false) {
          out.errors.push('turning off rotating prompts did not call set_ui_flag(rotatingPrompts, false)');
        }
        if (rowByLabel5('starts in', '.expert-dropdown-row')) {
          out.errors.push('the rotation-mode dropdown still shows after turning rotating prompts off');
        }
        if (rowByLabel5('Max. entries per', '.value-field')) {
          out.errors.push('the max-entries slider still shows after turning rotating prompts off');
        }
        if (rowByLabel5('Show the next-entry badge', '.switch-field')) {
          out.errors.push('the badge sub-flag still shows after turning rotating prompts off');
        }
        // Restore the default so later blocks (and a real re-run of this suite) see
        // the same starting state.
        const backSw = rowByLabel5('Rotating', '.switch-field')?.querySelector('input.switch');
        if (backSw) { backSw.checked = true; backSw.dispatchEvent(new Event('change', { bubbles: true })); }
        await sleep(120);
      }
    }
    // 32: editor UI — the rotation switch converts the textarea into a list, "+
    // Entry" adds a row, and saving calls add_prompt with the split payload.
    {
      const rowByLabel6 = (text, sel) => {
        const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
        const find = () => [...document.querySelectorAll('#expert-flags ' + sel)]
          .find(r => ((r.querySelector('span') || {}).textContent || '').includes(text)) || null;
        if (!tabs.length) return find();
        for (const tb of tabs) { tb.click(); const hit = find(); if (hit) return hit; }
        return null;
      };
      await click('gear');
      await click('expert-open');
      const rotSw2 = rowByLabel6('Rotating', '.switch-field')?.querySelector('input.switch');
      if (rotSw2 && !rotSw2.checked) { rotSw2.checked = true; rotSw2.dispatchEvent(new Event('change', { bubbles: true })); }
      await sleep(150);
      document.getElementById('settings-close')?.click();
      await sleep(150);
      const homeTab2 = document.querySelector('.view-btn[data-view-id="home"]');
      if (homeTab2) { homeTab2.click(); await sleep(150); }
      const freeCell = [...document.querySelectorAll('#grid .cell')].find(c => !c.firstElementChild);
      if (!freeCell) {
        out.errors.push('no free grid cell to test the rotating-prompts editor with');
      } else {
        freeCell.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(200);
        const rotRow = document.getElementById('modal-rotate-toggle-row');
        const advBody = document.getElementById('modal-advanced-body');
        if (!rotRow || rotRow.classList.contains('hidden')) {
          out.errors.push('the rotating-texts switch did not appear in the editor with the flag on');
        } else if (advBody.contains(rotRow)) {
          out.errors.push('the rotating-texts switch is still inside Advanced options');
        } else if (rotRow.nextElementSibling !== document.getElementById('modal-rotate-list')) {
          out.errors.push('the rotating-texts switch is not the element right above the entry list');
        } else if (!advBody.classList.contains('hidden')) {
          out.errors.push('advanced options does not start collapsed');
        } else if (!(rotRow.getBoundingClientRect().height > 0)) {
          out.errors.push('the rotating-texts switch has no height with Advanced collapsed');
        } else if (rotRow.getBoundingClientRect().bottom > document.getElementById('modal-rotate-list').getBoundingClientRect().top + 1 && !document.getElementById('modal-rotate-list').classList.contains('hidden')) {
          out.errors.push('the rotating-texts switch sits below the entry list');
        } else {
          document.getElementById('modal-advanced-head').click();
          await sleep(150);
          if (advBody.classList.contains('hidden')) out.errors.push('advanced options did not expand');
          document.getElementById('modal-name').value = 'Rotate Test';
          document.getElementById('modal-text').value = 'Entry one';
          const rotSwitch = document.getElementById('modal-rotate-switch');
          rotSwitch.checked = true;
          rotSwitch.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(120);
          if (!document.getElementById('modal-text').classList.contains('hidden')) {
            out.errors.push('the plain textarea stayed visible after turning rotation on');
          }
          {
            // Measured with rotation on: the switch row ends where the entry list begins.
            const rl = document.getElementById('modal-rotate-list').getBoundingClientRect();
            const sr = rotRow.getBoundingClientRect();
            if (!(rl.height > 0) || sr.bottom > rl.top + 1) out.errors.push('the rotating-texts switch is not above the entry list: switch bottom ' + Math.round(sr.bottom) + ', list top ' + Math.round(rl.top));
          }
          // B4: neither the clear-X wrapper around the textarea nor the rendered
          // Markdown pane may keep a box while the variants list is the editor —
          // both used to survive and left a tall empty gap above the colour row,
          // the pane still showing entry 1 as a stray field.
          // 456: scoped to modal-text's own wrapper, not the first .clear-wrap-ta
          // in the dialog — the always-visible notes field now sits earlier in the
          // DOM and wears that class too, but never hides during rotation.
          const taWrap = document.getElementById('modal-text').closest('.clear-wrap-ta');
          if (taWrap && taWrap.getBoundingClientRect().height > 0) {
            out.errors.push('the textarea wrapper still takes ' + Math.round(taWrap.getBoundingClientRect().height) + 'px while rotation is on');
          }
          const mdWrap = document.getElementById('modal-md-wrap');
          if (mdWrap && mdWrap.getBoundingClientRect().height > 0) {
            out.errors.push('the Markdown pane is still shown while rotation is on');
          }
          const ctrlRow = document.querySelector('.modal-rotate-controls-row');
          const colorRow = document.getElementById('color-row');
          if (ctrlRow && colorRow) {
            const gap = colorRow.getBoundingClientRect().top - ctrlRow.getBoundingClientRect().bottom;
            if (gap > 40) out.errors.push('gap between the entry controls and the colours is ' + Math.round(gap) + 'px');
          }
          // 33: the length readout belongs to the plain text box and must be gone
          // while the variants list stands in for it.
          const lenBox = document.getElementById('modal-length');
          if (lenBox && lenBox.getBoundingClientRect().height > 0) {
            out.errors.push('the chars/words readout is still shown while rotation is on');
          }
          // 33/90-94: the order/random control sits in the "+ Entry" row, pushed
          // right, with the reset ("from the start") to its left and the Auto
          // toggle (90-94) flush right of the mode control — not on a row of its own.
          const addBtn = document.getElementById('modal-rotate-add');
          const modeSel = document.getElementById('modal-rotate-mode');
          const resetBtn = document.getElementById('modal-rotate-reset');
          const autoBtn = document.getElementById('modal-rotate-auto');
          if (addBtn && modeSel && ctrlRow) {
            const a = addBtn.getBoundingClientRect(), m = modeSel.getBoundingClientRect();
            const r = ctrlRow.getBoundingClientRect();
            if (Math.abs(a.top - m.top) > 6) out.errors.push('the mode control is not on the "+ Entry" row');
            if (m.left <= a.right) out.errors.push('the mode control is not right of "+ Entry"');
            if (resetBtn && !resetBtn.classList.contains('hidden')) {
              const rs = resetBtn.getBoundingClientRect();
              if (rs.right > m.left + 1) out.errors.push('"from the start" is not left of the mode control');
            }
            if (autoBtn) {
              const au = autoBtn.getBoundingClientRect();
              if (au.left < m.right - 1) out.errors.push('the Auto toggle is not right of the mode control');
              if (r.right - au.right > 12) out.errors.push('the Auto toggle is not flush right in its row');
            } else if (r.right - m.right > 12) {
              out.errors.push('the mode control is not flush right in its row');
            }
          }
          const list = document.getElementById('modal-rotate-list');
          if (!list || list.classList.contains('hidden')) {
            out.errors.push('the rotate entry list did not appear after turning rotation on');
          }
          let rows = document.querySelectorAll('.rotate-entry-row');
          if (rows.length !== 1 || rows[0].querySelector('.rotate-entry-input').value !== 'Entry one') {
            out.errors.push('turning rotation on did not seed entry 1 with the existing text');
          }
          document.getElementById('modal-rotate-add').click();
          await sleep(80);
          rows = document.querySelectorAll('.rotate-entry-row');
          if (rows.length !== 2) {
            out.errors.push('"+ Entry" did not add a second entry row');
          } else {
            // 33: on a row whose text box has grown past one line, the number on the
            // left and the icons on the right must sit at the row-s vertical middle,
            // not stuck to its top edge.
            const tall = rows[0].querySelector('.rotate-entry-input');
            if (tall) {
              // String.fromCharCode(10), not an escape: Python eats a backslash-n in
              // this probe string before the browser ever sees it.
              tall.value = ['line one', 'line two', 'line three', 'line four'].join(String.fromCharCode(10));
              tall.dispatchEvent(new Event('input', { bubbles: true }));
              await sleep(120);
            }
            const rowBox = rows[0].getBoundingClientRect();
            for (const sel of ['.rotate-entry-num', '.rotate-entry-btns']) {
              const el = rows[0].querySelector(sel);
              if (!el) { out.errors.push('the entry row is missing ' + sel); continue; }
              const b = el.getBoundingClientRect();
              if (b.height >= rowBox.height - 2) continue; // fills the row, nothing to centre
              const off = (b.top + b.height / 2) - (rowBox.top + rowBox.height / 2);
              if (Math.abs(off) > 3) {
                out.errors.push(sel + ' is ' + Math.round(off) + 'px off the row centre');
              }
            }
            // 67: the centering check above overwrote entry 1 with throwaway text —
            // put it back before the save below, or the payload check further down
            // fails on content this probe itself corrupted.
            if (tall) {
              tall.value = 'Entry one';
              tall.dispatchEvent(new Event('input', { bubbles: true }));
            }
            const ta2 = rows[1].querySelector('.rotate-entry-input');
            ta2.value = 'Entry two';
            ta2.dispatchEvent(new Event('input', { bubbles: true }));
            // 94: Auto-advance toggle -- visible and clickable with two plain entries,
            // aria-pressed flips on click, stays enabled for a placeholder entry (90
            // explicitly allows them), and disables itself live the moment the entry
            // count drops below two, without needing the editor reopened (92-D fix).
            const autoBtn2 = document.getElementById('modal-rotate-auto');
            if (!autoBtn2) {
              out.errors.push('the Auto toggle is missing from the rotate editor');
            } else {
              if (autoBtn2.disabled) out.errors.push('the Auto toggle is disabled with two plain entries present');
              if (autoBtn2.getAttribute('aria-pressed') !== 'false') out.errors.push('the Auto toggle starts pressed');
              autoBtn2.click();
              if (autoBtn2.getAttribute('aria-pressed') !== 'true') out.errors.push('the Auto toggle did not flip to pressed on click');
              if (!autoBtn2.classList.contains('active')) out.errors.push('the Auto toggle did not gain the active class on click');
              ta2.value = 'Entry two {#{name}#}';
              ta2.dispatchEvent(new Event('input', { bubbles: true }));
              if (autoBtn2.disabled) out.errors.push('the Auto toggle disabled itself for a placeholder entry');
              ta2.value = 'Entry two';
              ta2.dispatchEvent(new Event('input', { bubbles: true }));
              const delBtn2 = rows[1].querySelector('.rotate-entry-del');
              delBtn2.click();
              if (!autoBtn2.disabled) out.errors.push('the Auto toggle stayed enabled after dropping to a single entry');
              document.getElementById('modal-rotate-add').click();
              const rowsAfterAuto = document.querySelectorAll('.rotate-entry-row');
              const ta2b = rowsAfterAuto[1].querySelector('.rotate-entry-input');
              ta2b.value = 'Entry two';
              ta2b.dispatchEvent(new Event('input', { bubbles: true }));
              if (autoBtn2.disabled) out.errors.push('the Auto toggle stayed disabled once a second entry came back');
              if (autoBtn2.getAttribute('aria-pressed') !== 'true') autoBtn2.click();
            }
            const modeSel = document.getElementById('modal-rotate-mode');
            modeSel.value = 'random';
            modeSel.dispatchEvent(new Event('change', { bubbles: true }));
            window.__SMOKE_ARGS__ = [];
            document.getElementById('modal-confirm').click();
            await sleep(250);
            const addCall = (window.__SMOKE_ARGS__ || []).find(([cmd]) => cmd === 'add_prompt');
            if (!addCall) {
              out.errors.push('saving a rotating prompt did not call add_prompt');
            } else {
              const a = addCall[1];
              if (a.text !== 'Entry one' || JSON.stringify(a.variants) !== JSON.stringify(['Entry two']) || a.rotateMode !== 'random' || a.rotateAuto !== true) {
                out.errors.push('add_prompt got the wrong rotating payload: ' + JSON.stringify({ text: a.text, variants: a.variants, rotateMode: a.rotateMode, rotateAuto: a.rotateAuto }));
              }
              // 32-D: the tile this created shows a live "next entry" badge, and a
              // real click copies the CURRENT entry (peek→copy_text→commit, not
              // copy_prompt — copyResolved's own path for grid/library/journal).
              await sleep(150); // grid re-render + primeRotateState's own peek
              const newTile = [...document.querySelectorAll('#grid .tile-name')]
                .find(n => n.textContent === 'Rotate Test')?.closest('.tile');
              if (!newTile) {
                out.errors.push('the newly created rotating prompt has no grid tile');
              } else {
                const newId = newTile.dataset.id;
                const badge0 = newTile.querySelector('.tile-rotate-badge');
                if (!badge0 || badge0.textContent !== '1/2') {
                  out.errors.push('the rotate badge shows "' + (badge0 && badge0.textContent) + '" instead of "1/2" on a fresh 2-entry prompt');
                }
                window.__LAST_COPY__ = undefined;
                await clickTile(newId);
                if (window.__LAST_COPY__ !== 'Entry one') {
                  out.errors.push('clicking the rotating tile copied "' + window.__LAST_COPY__ + '" instead of entry 1');
                }
                // The backend advances the pointer and pushes rotation-changed to every
                // window after a real commit — simulated the same way every other
                // push-event probe in this suite delivers a backend event.
                window.__FIRE__('rotation-changed', { id: newId, index: 1, pos: 2, total: 2, text: 'Entry two' });
                await sleep(50);
                const badge1 = newTile.querySelector('.tile-rotate-badge');
                if (!badge1 || badge1.textContent !== '2/2') {
                  out.errors.push('the rotate badge did not advance to "2/2" after rotation-changed');
                }
                if (!newTile.dataset.tipRotate || !newTile.dataset.tipRotate.includes('Entry two')) {
                  out.errors.push('the tile tooltip does not mention the new next entry after rotation-changed');
                }
              }
            }
          }
        }
      }
      // 47: rotateBadge is a sub-toggle of rotatingPrompts -- switching the PARENT
      // off must hide the badge too, not just leave it keyed to its own toggle.
      await click('gear');
      await click('expert-open');
      const rotSwOff = rowByLabel6('Rotating', '.switch-field')?.querySelector('input.switch');
      if (rotSwOff && rotSwOff.checked) { rotSwOff.checked = false; rotSwOff.dispatchEvent(new Event('change', { bubbles: true })); }
      await sleep(150);
      document.getElementById('settings-close')?.click();
      await sleep(150);
      const homeTabOff = document.querySelector('.view-btn[data-view-id="home"]');
      if (homeTabOff) { homeTabOff.click(); await sleep(150); }
      const rotTileOff = [...document.querySelectorAll('#grid .tile-name')]
        .find(n => n.textContent === 'Rotate Test')?.closest('.tile');
      if (rotTileOff && rotTileOff.querySelector('.tile-rotate-badge')) {
        out.errors.push('the rotate badge is still shown with rotatingPrompts switched off');
      }
      // Restore the flag to its default (on, since 33) for later blocks / a re-run.
      await click('gear');
      await click('expert-open');
      const rotSw3 = rowByLabel6('Rotating', '.switch-field')?.querySelector('input.switch');
      if (rotSw3 && !rotSw3.checked) { rotSw3.checked = true; rotSw3.dispatchEvent(new Event('change', { bubbles: true })); }
      await sleep(120);
      document.getElementById('settings-close')?.click();
      await sleep(120);
    }
    // 32-E: library search finds a hit inside ANY entry of a rotating prompt, not
    // just entry 1 — "Entry two" (the "Rotate Test" prompt second entry, created
    // above) exists nowhere else in the fixture.
    // NOTE: no apostrophes inside JS string literals below — this whole PROBE is a
    // Python triple-quoted (non-raw) string, so a JS-intended \' is consumed by
    // PYTHON's own escape processing before the browser ever sees it, leaving an
    // unescaped quote that breaks the surrounding JS string and kills the WHOLE
    // script tag with a syntax error (silent "no probe result", nothing runs).
    {
      await click('library-btn');
      const q2 = document.getElementById('library-q');
      if (!q2) {
        out.errors.push('the library has no search field (32-E rotating-entry check)');
      } else {
        q2.value = 'entry two';
        q2.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(300);
        const rows = [...document.querySelectorAll('#library-list .lib-item')];
        const nameEl = rows[0] && rows[0].querySelector('.lib-name');
        if (rows.length !== 1 || !nameEl || !nameEl.textContent.includes('Rotate Test')) {
          out.errors.push('searching a rotating prompt second entry found ' + rows.length + ' rows instead of exactly Rotate Test');
        } else {
          const tagEl = rows[0].querySelector('.lib-rotate-tag');
          if (!tagEl || tagEl.textContent !== '2/2') {
            out.errors.push('the library hit did not tag which entry matched: got ' + (tagEl && tagEl.textContent));
          }
          const textEl = rows[0].querySelector('.lib-text');
          if (!textEl || !textEl.textContent.includes('Entry two')) {
            out.errors.push('the library row preview did not switch to the matched entry');
          }
        }
        q2.value = '';
        q2.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(200);
      }
      await click('library-close');
    }
    // 230: rotateReqs guards a stale rotation_peek reply from stomping a NEWER
    // rotation-changed push that arrived while the peek was still in flight.
    // Create a brand-new rotating prompt so its FIRST-EVER peek is still
    // unresolved (primeRotateState only ever peeks once per id), gate that peek
    // open, deliver a newer push for the same id, THEN resolve the stale peek
    // with older data -- the tile must keep the pushed state, not fall back to
    // the late reply.
    {
      // Home is full by now; the roomy Archive view hosts the new prompt.
      document.querySelector('.view-btn[data-view-id="archive"]')?.click();
      await sleep(250);
      const freeCellRace = [...document.querySelectorAll('#grid .cell')].find(c => !c.firstElementChild);
      if (!freeCellRace) {
        out.errors.push('230: no free grid cell to test the stale rotation-peek race with');
      } else {
        freeCellRace.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(200);
        document.getElementById('modal-advanced-head').click();
        await sleep(150);
        document.getElementById('modal-name').value = 'Rotate Race';
        document.getElementById('modal-text').value = 'Race entry one';
        const rotSwitchRace = document.getElementById('modal-rotate-switch');
        rotSwitchRace.checked = true;
        rotSwitchRace.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(120);
        document.getElementById('modal-rotate-add').click();
        await sleep(80);
        const raceRows = document.querySelectorAll('.rotate-entry-row');
        if (raceRows.length !== 2) {
          out.errors.push('230: setting up the stale-peek race prompt did not add a second entry row');
        } else {
          const ta2r = raceRows[1].querySelector('.rotate-entry-input');
          ta2r.value = 'Race entry two';
          ta2r.dispatchEvent(new Event('input', { bubbles: true }));
          // Gate the NEXT rotation_peek call so it stays pending until this probe
          // resolves it -- the fresh tile about to render fires exactly one (its
          // first-ever primeRotateState peek, right after renderGrid() below).
          window.__ROTATION_PEEK_GATE__ = [];
          document.getElementById('modal-confirm').click();
          await sleep(250);
          const raceTile = [...document.querySelectorAll('#grid .tile-name')]
            .find(n => n.textContent === 'Rotate Race')?.closest('.tile');
          if (!raceTile) {
            out.errors.push('230: the stale-peek race prompt has no grid tile');
          } else {
            const raceId = raceTile.dataset.id;
            const gate = window.__ROTATION_PEEK_GATE__ || [];
            if (gate.length !== 1) {
              out.errors.push('230: expected exactly 1 gated rotation_peek call for the new tile, got ' + gate.length);
            } else {
              // A newer push -- as if another window already copied this prompt
              // and advanced it -- lands WHILE the initial peek is still pending.
              window.__FIRE__('rotation-changed', { id: raceId, index: 1, pos: 2, total: 2, text: 'Pushed entry two' });
              await sleep(50);
              // THEN the stale peek finally answers, with OLDER data -- the
              // rotateReqs generation guard must drop this reply, not let it
              // overwrite the fresher pushed state above.
              gate[0].resolve({ index: 0, pos: 1, total: 2, text: 'Stale entry one' });
              await sleep(80);
              const badgeRace = raceTile.querySelector('.tile-rotate-badge');
              if (!badgeRace || badgeRace.textContent !== '2/2') {
                out.errors.push('230: a stale rotation_peek reply overwrote the newer rotation-changed badge: got "' +
                  (badgeRace && badgeRace.textContent) + '" instead of "2/2"');
              }
              const tip = raceTile.dataset.tipRotate || '';
              if (!tip.includes('Pushed entry two')) {
                out.errors.push('230: a stale rotation_peek reply overwrote the newer rotation-changed tooltip');
              }
              if (tip.includes('Stale entry one')) {
                out.errors.push('230: the tile tooltip shows the stale peek text instead of the pushed rotation-changed text');
              }
            }
          }
        }
        window.__ROTATION_PEEK_GATE__ = null;
      }
    }
    // 230: per-row icon-only actions (library add/favorite/edit/delete, journal
    // edit/delete) are keyboard-activatable -- makeKeyboardActivatable wires
    // Enter/Space to the same click the element already handles.
    {
      await click('library-btn');
      const favSpan = document.querySelector('#library-list .lib-item .lib-fav');
      if (!favSpan) {
        out.errors.push('230: no favorite-star action found in the library to test keyboard activation');
      } else {
        // Favorites float to the top on toggle (renderLibrary re-sorts), so the
        // row must be re-found by its stable id afterward, not by list position.
        const rowId = favSpan.closest('.lib-item').dataset.id;
        const wasActive = favSpan.classList.contains('active');
        window.__SMOKE_CALLS__ = [];
        favSpan.focus();
        if (document.activeElement !== favSpan) {
          out.errors.push('230: the library favorite-star action is not focusable');
        } else {
          favSpan.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
          await sleep(200);
          if (!(window.__SMOKE_CALLS__ || []).includes('set_favorite')) {
            out.errors.push('230: pressing Enter on the library favorite-star action did not call set_favorite');
          }
          const rowAfter = document.querySelector('.lib-item[data-id="' + CSS.escape(rowId) + '"]');
          const favSpan2 = rowAfter && rowAfter.querySelector('.lib-fav');
          if (!favSpan2 || favSpan2.classList.contains('active') === wasActive) {
            out.errors.push('230: pressing Enter on the library favorite-star action did not toggle its state');
          }
        }
      }
      await click('library-close');
    }
    // 81: colour-pick toast shows all three notations as separate copy chips.
    {
      window.__SMOKE_ARGS__ = window.__SMOKE_ARGS__ || [];
      const before = window.__SMOKE_ARGS__.length;
      window.__FIRE__('color-picked', { text: '#1A2B3C', hex: '#1A2B3C', rgb: 'rgb(26,43,60)', hsl: 'hsl(210,40%,17%)' });
      await sleep(150);
      const toastEl2 = document.getElementById('toast');
      const chips = [...toastEl2.querySelectorAll('.toast-chip')];
      if (chips.length !== 3) {
        out.errors.push('the colour-pick toast does not show all three notations (found ' + chips.length + ')');
      } else {
        const labels = chips.map(c => c.textContent);
        if (!labels.includes('#1A2B3C') || !labels.includes('rgb(26,43,60)') || !labels.includes('hsl(210,40%,17%)')) {
          out.errors.push('the colour-pick toast chips show the wrong values: ' + labels.join('|'));
        }
        const hexChipBefore = chips.find(c => c.textContent === '#1A2B3C');
        if (!hexChipBefore.classList.contains('is-current')) {
          out.errors.push('the colour-pick toast did not mark the initially-copied chip as current');
        }
        const rgbChip = chips.find(c => c.textContent === 'rgb(26,43,60)');
        rgbChip.click();
        await sleep(120);
        const copyCall = window.__SMOKE_ARGS__.slice(before)
          .find(([cmd, args]) => cmd === 'copy_text' && args && args.text === 'rgb(26,43,60)');
        if (!copyCall) {
          out.errors.push('clicking a colour-toast chip did not copy that exact notation');
        }
        if (!toastEl2.classList.contains('show')) {
          out.errors.push('clicking a colour-toast chip closed the toast (should stay open for more clicks)');
        }
        // 169: clicking a different chip must move the "current" mark onto it.
        if (!rgbChip.classList.contains('is-current') || hexChipBefore.classList.contains('is-current')) {
          out.errors.push('clicking rgb chip did not move is-current from hex to rgb');
        }
        if (rgbChip.getAttribute('aria-pressed') !== 'true' || hexChipBefore.getAttribute('aria-pressed') !== 'false') {
          out.errors.push('chip aria-pressed did not follow the current pick');
        }
      }
      toastEl2.classList.remove('show'); // don't linger into later blocks
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
    // The settings keep one import/export row with the backup manager button at its
    // end; the file-based backup buttons are gone from here. Measured live.
    {
      const settingsEl = document.getElementById('settings');
      const wasHidden = settingsEl.classList.contains('hidden');
      settingsEl.classList.remove('hidden');
      await sleep(150);
      const field = document.getElementById('impexp-field');
      const rows = field ? field.querySelectorAll('.export-row') : [];
      if (rows.length !== 1) out.errors.push('settings: expected one import/export row, found ' + rows.length);
      else {
        const kids = [...rows[0].children];
        if (kids[kids.length - 1].id !== 'settings-backup-btn') out.errors.push('settings: the backup manager button is not last in the import/export row');
        const eb = document.getElementById('export-btn').getBoundingClientRect(), bb = document.getElementById('settings-backup-btn').getBoundingClientRect();
        if (!bb.width || !bb.height) out.errors.push('settings: the backup manager button has no size');
        else if (!(bb.left > eb.right - 1 || bb.top > eb.bottom - 1)) out.errors.push('settings: the backup manager button does not follow the export button');
      }
      if (document.getElementById('export-all') || document.getElementById('import-all')) out.errors.push('settings: the old save/import backup buttons are still in the settings row');
      if (wasHidden) settingsEl.classList.add('hidden');
    }
    document.getElementById('settings-backup-btn').click();
    await sleep(400);
    const bm = document.getElementById('backup-modal');
    if (bm.classList.contains('hidden')) {
      out.errors.push('the backups dialog did not open');
    } else if (!document.getElementById('backup-panel').children.length) {
      out.errors.push('the backups dialog stayed empty');
    } else if (!document.getElementById('stats-panel').children.length) {
      out.errors.push('the backups dialog has no usage statistics');
    } else if (!document.getElementById('storage-panel').children.length) {
      // 414: usage, storage and backups are three separate sections of the
      // same window, in that order.
      out.errors.push('414: the backups dialog has no storage section');
    } else if (getComputedStyle(bm).zIndex <= getComputedStyle(document.getElementById('settings')).zIndex) {
      out.errors.push('the backups dialog sits behind the settings it was opened from');
    } else {
      layoutScan('backups');
      mustReach('backups', '#backup-modal .modal-head-actions > button');
      // The manager's left column holds Create backup, Open in Explorer, Save
      // backup, Import backup, stacked in that order inside the column.
      {
        const col = document.querySelector('#backup-panel .backup-col');
        const kids = col ? [...col.children].filter(c => c.tagName === 'BUTTON') : [];
        if (kids.length < 4 || !kids[0].classList.contains('backup-now-btn') || kids[2].id !== 'backup-save-file' || kids[3].id !== 'backup-import-file') {
          out.errors.push('backup manager: the left column does not start with the four backup buttons in order: ' + kids.map(k => k.id || k.className).join(','));
        } else {
          const cr = col.getBoundingClientRect();
          let prevBottom = -1;
          for (const k of kids.slice(0, 4)) {
            const r = k.getBoundingClientRect();
            if (!r.width || !r.height) out.errors.push('backup manager: a backup button has no size: ' + (k.id || k.className));
            if (r.left < cr.left - 1 || r.right > cr.right + 1) out.errors.push('backup manager: a backup button sticks out of its column: ' + (k.id || k.className));
            if (r.top < prevBottom - 1) out.errors.push('backup manager: the backup buttons overlap: ' + (k.id || k.className));
            prevBottom = r.bottom;
          }
        }
      }
      // P63: the consequence of forgetting the backup password is spelled out
      // right where the field is, not just in a toast after saving it.
      const pwHint = document.querySelector('#backup-panel .backup-pw-hint');
      if (!pwHint || !pwHint.textContent.trim()) {
        out.errors.push('the backup password field has no forget-consequence warning');
      }
      // The top-list length field used to run past its own row.
      const row = document.querySelector('#stats-panel .stats-topn-row');
      const fld = row && row.querySelector('.modal-input');
      if (fld && fld.getBoundingClientRect().right > row.getBoundingClientRect().right + 1) {
        out.errors.push('the top-list length field overflows its row');
      }
      // 45: toggling the stats switch used to re-render into the inner panel from
      // the previous render instead of the real container, nesting one deeper each
      // time. Re-query fresh each click -- a re-render replaces the whole subtree,
      // so a reference from before the first click is stale for the second.
      {
        for (let i = 0; i < 2; i++) {
          const sw = document.querySelector('#stats-panel .stats-head-row .switch');
          if (!sw) break;
          sw.click();
          await sleep(150);
        }
        const stillOn = document.querySelector('#stats-panel .stats-head-row .switch')?.checked;
        if (!stillOn) out.errors.push('two stats-switch toggles did not end back on');
        const heads = document.querySelectorAll('#stats-panel .stats-panel-body');
        if (heads.length !== 1) {
          out.errors.push('toggling stats twice left ' + heads.length + ' stats-panel-body elements instead of 1');
        }
      }
      // Two-column settings, 8 diagnostic cards, 12 statistic cards, overview split.
      if (document.querySelectorAll('#backup-panel .backup-col').length !== 2) {
        out.errors.push('the backup settings are not split in two columns');
      }
      // 46: a typed 0 in a min-0 number field (a GFS retention tier, "none") used to
      // silently reset to the default via Number(raw) || def, since 0 is falsy.
      {
        const tierInput = document.querySelector('#backup-panel .backup-tier-grid input');
        if (tierInput) {
          tierInput.value = '0';
          tierInput.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(80);
          if (tierInput.value !== '0') {
            out.errors.push('typing 0 in a min-0 number field reset to ' + tierInput.value + ' instead of staying 0');
          }
        }
      }
      // 65: opens the snapshot folder — must exist and never throw.
      const openFolderBtn = [...document.querySelectorAll('#backup-panel .ghost-btn')]
        .find(b => b.textContent === 'Open in Explorer');
      if (!openFolderBtn) {
        out.errors.push('the "open backups folder" button is missing');
      } else {
        openFolderBtn.click();
        await sleep(120);
        if (!(window.__SMOKE_CALLS__ || []).includes('open_backups_folder')) {
          out.errors.push('the "open backups folder" button did not call the backend');
        }
        // 102: "Create backup" used to shrink to its own text while "Open in
        // Explorer" (a plain ghost-btn) took the full column width.
        const nowBtn = document.querySelector('#backup-panel .backup-now-btn');
        if (!nowBtn) {
          out.errors.push('the "create backup" button is missing');
        } else if (Math.abs(nowBtn.getBoundingClientRect().width - openFolderBtn.getBoundingClientRect().width) > 1) {
          out.errors.push('102: create-backup width ' + Math.round(nowBtn.getBoundingClientRect().width) +
            'px does not match open-folder width ' + Math.round(openFolderBtn.getBoundingClientRect().width) + 'px');
        }
      }
      if (document.querySelectorAll('#backup-panel .stat-cards .stat-card').length !== 8) {
        out.errors.push('the backup diagnostics do not show 8 cards');
      }
      if (document.querySelectorAll('#stats-panel .stat-cards .stat-card').length !== 12) {
        out.errors.push('the usage statistics do not show 12 cards');
      }
      // 104: measure before guessing -- the top-list-length field sits in a 2-col
      // grid row (.stats-head-row) right under the section's own collapsible header
      // button; check it against every neighbour it could plausibly overlap
      // (the header above, the toggle beside it, the cards below), stays permanently
      // as an overlap guard for the whole statistics header. Checked at every tested
      // width, with the statistics section open (it must be, to reach the row at
      // all) both before and after a round trip through hiding/showing the row via
      // the record-statistics toggle.
      {
        const overlaps = (a, b) => a.left < b.right - 1 && b.left < a.right - 1 && a.top < b.bottom - 1 && b.top < a.bottom - 1;
        const checkTopN = (label) => {
          const row = document.querySelector('#stats-panel .stats-topn-row');
          const header = document.querySelector('#stats-panel > .stat-section');
          const toggle = document.querySelector('#stats-panel .stats-head-row .switch-field');
          const cards = document.querySelector('#stats-panel .stat-cards');
          if (!row) { out.errors.push('104 (' + label + '): the top-list-length row is missing'); return; }
          const rb = row.getBoundingClientRect();
          for (const [name, el] of [['header', header], ['toggle', toggle], ['cards', cards]]) {
            if (el && overlaps(rb, el.getBoundingClientRect())) {
              out.errors.push('104 (' + label + '): the top-list-length row overlaps the ' + name);
            }
          }
        };
        checkTopN('statistics on');
        // 407: the "export statistics" button rides the same on/off switch as
        // the rest of the dashboard: present while statistics are recorded,
        // gone the moment they are switched off (an export of nothing would
        // just be a header row).
        if (!document.getElementById('stats-export-btn')) {
          out.errors.push('407: the statistics export button is missing while statistics are on');
        }
        const findToggle = () => document.querySelector('#stats-panel .stats-head-row .switch-field input');
        const findRow = () => document.querySelector('#stats-panel .stats-topn-row');
        const waitFor = async (fn) => { for (let i = 0; i < 40 && !fn(); i++) await sleep(50); return fn(); };
        if (findToggle()) {
          findToggle().click();
          await waitFor(() => !findRow()); // re-render dropped the row while off
          if (document.getElementById('stats-export-btn')) {
            out.errors.push('407: the statistics export button is still there with statistics off');
          }
          // 414: storage is not usage tracking and must not share its switch --
          // the storage section stays exactly as it was while only the usage
          // section re-renders itself off.
          if (!document.getElementById('storage-panel').children.length) {
            out.errors.push('414: the storage section disappeared when statistics were switched off');
          }
          const t2 = findToggle(); // 104: a fresh node -- the click above rebuilt the panel
          if (t2) {
            t2.click(); // back on: statistics stays on for the rest of the walk
            await waitFor(() => !!findRow());
          }
          checkTopN('statistics re-enabled');
          if (!document.getElementById('stats-export-btn')) {
            out.errors.push('407: the statistics export button did not come back with statistics re-enabled');
          }
        }
      }
      // 414: the three sections appear in the fixed order usage, storage,
      // backups, and the storage section lists every row the backend sent
      // with a "measure again" button to force a fresh scan.
      {
        const sections = [...document.querySelectorAll('#backup-modal > .modal > .backup-body > div')].map((d) => d.id);
        const order = sections.filter((id) => ['stats-panel', 'storage-panel', 'backup-panel'].includes(id));
        if (order.join(',') !== 'stats-panel,storage-panel,backup-panel') {
          out.errors.push('414: the three sections are not in usage/storage/backups order (' + order.join(',') + ')');
        }
        const rows = document.querySelectorAll('#storage-panel .storage-row');
        if (rows.length !== 9) {
          out.errors.push('414: the storage section does not list 9 rows (found ' + rows.length + ')');
        }
        if (!document.getElementById('storage-remeasure-btn')) {
          out.errors.push('414: the storage section has no "measure again" button');
        }
      }
      // 415: most-used tools rubric -- one row per counted tool, a short
      // "never used" list, and the sentence saying none of it leaves the
      // computer, matching the mocked usage_stats fixture (2 tools, 2 never used).
      {
        const toolRows = document.querySelectorAll('#stats-panel .stat-tool-views .stat-view-row');
        if (toolRows.length !== 2) {
          out.errors.push('415: the most-used tools list does not show 2 rows (found ' + toolRows.length + ')');
        }
        const neverUsed = document.querySelectorAll('#stats-panel .stat-tool-never');
        if (neverUsed.length !== 2) {
          out.errors.push('415: the tools never-used list does not show 2 entries (found ' + neverUsed.length + ')');
        }
        if (!document.querySelector('#stats-panel .stat-local-only')) {
          out.errors.push('415: the "stays on this computer" note is missing from the tools rubric');
        }
      }
      if (!document.querySelector('#stats-panel .stat-overview-split')) {
        out.errors.push('the overview is not split from the per-view list');
      }
      // 58: the last-14-days chart — one bar per day, zero days marked distinctly.
      {
        const bars = [...document.querySelectorAll('#stats-panel .stat-daily-bar')];
        if (bars.length !== 14) {
          out.errors.push('the daily-copies chart does not show 14 bars (found ' + bars.length + ')');
        } else {
          if (!bars.some((b) => b.classList.contains('zero'))) {
            out.errors.push('the daily-copies chart never marks a zero day');
          }
          if (bars.some((b) => b.classList.contains('zero') && parseFloat(b.style.height) > 3)) {
            out.errors.push('a zero-count day has a non-baseline bar height');
          }
          const busiest = bars.reduce((a, b) => parseFloat(b.style.height) > parseFloat(a.style.height) ? b : a);
          if (!busiest.title.includes(': 6')) {
            out.errors.push('the tallest bar does not report the known busiest day count (6)');
          }
        }
      }
      // A stored password shows as dots — as a placeholder, never as a value.
      const pw = document.querySelector('#backup-panel .backup-pw-wrap .modal-input');
      if (!pw || pw.type !== 'password' || pw.value !== '' || !/^•+$/.test(pw.placeholder)) {
        out.errors.push('a stored backup password is not shown as empty dots');
      }
      // 25: a plain number field replaces the light/normal/heavy preset tiles;
      // "Smart retention" is a switch above it that greys the field out and
      // activates the tier grid below. The panel re-renders on every change, so
      // elements have to be looked up again after each one.
      const settle = async (fn) => { for (let i = 0; i < 60 && !fn(); i++) await sleep(50); return fn(); };
      const keepInput = () => document.querySelector('#backup-panel .backup-keep-row input[type=number]');
      const gfsSwitch = () => [...document.querySelectorAll('#backup-panel .switch-field')]
        .find((l) => l.querySelector('span')?.textContent.trim() === 'Smart retention')
        ?.querySelector('input[type=checkbox]');
      const tierNode = () => document.querySelector('#backup-panel .backup-tier-grid');
      if (!keepInput() || !gfsSwitch()) {
        out.errors.push('the backup panel is missing the retention field or the smart-retention switch');
      } else {
        if (keepInput().value !== '10') out.errors.push('backupKeep does not default to 10: ' + keepInput().value);
        if (keepInput().disabled) out.errors.push('the retention field is disabled while smart retention is off');
        if (!tierNode().classList.contains('is-parked')) out.errors.push('the tier grid is not parked while smart retention is off');
        keepInput().value = '15';
        keepInput().dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(200);
        if (keepInput().value !== '15') out.errors.push('the retention field did not accept 15');
        let seenNode = tierNode();
        gfsSwitch().click();
        await settle(() => tierNode() !== seenNode);
        if (!await settle(() => !tierNode()?.classList.contains('is-parked'))) {
          out.errors.push('turning on smart retention did not unpark the tier grid');
        }
        if (document.querySelectorAll('#backup-panel .backup-tier-grid .modal-input').length !== 4) {
          out.errors.push('smart retention does not show 4 tier fields');
        }
        if (!keepInput().disabled || !keepInput().closest('.backup-keep-row').classList.contains('is-disabled')) {
          out.errors.push('the retention field did not grey out while smart retention is on');
        }
        seenNode = tierNode();
        gfsSwitch().click();
        await settle(() => tierNode() !== seenNode);
        if (!await settle(() => tierNode()?.classList.contains('is-parked'))) {
          out.errors.push('turning off smart retention did not park the tier grid again');
        }
        if (keepInput().disabled) out.errors.push('the retention field stayed disabled after turning smart retention off');
        if (keepInput().value !== '15') out.errors.push('the retention value was lost while smart retention was on: ' + keepInput().value);
        // 99: a row that differs from its default (15 vs 10) gets a reset icon that
        // restores it and then disappears again -- reused from the main expert menu.
        const keepReset = () => {
          const b = keepInput()?.closest('.backup-keep-row')?.querySelector('.expert-row-reset');
          return b && !b.classList.contains('hidden') ? b : null;
        };
        if (!keepReset()) {
          out.errors.push('backupKeep at 15 (default 10) shows no reset button');
        } else {
          keepReset().click();
          if (!await settle(() => keepInput()?.value === '10')) {
            out.errors.push('clicking the backupKeep reset button did not restore 10: ' + keepInput()?.value);
          }
          if (keepReset()) out.errors.push('the backupKeep reset button is still visible after resetting to default');
        }
      }
      // 61: the count column doubles as a click-to-expand toggle for the backup's
      // prompt names, when the backend supplied any (the fixture's first entry has
      // two, the second — an "older build" stand-in — has none).
      const rows = () => [...document.querySelectorAll('#backup-panel .backup-row')];
      const withNames = rows()[0];
      const withoutNames = rows()[1];
      if (!withNames || !withoutNames) {
        out.errors.push('the backup list does not show both fixture rows');
      } else {
        const toggle = withNames.querySelector('.backup-count-toggle');
        if (!toggle) {
          out.errors.push('a backup with names has no clickable count toggle');
        } else {
          const box = withNames.nextElementSibling;
          if (!box || !box.classList.contains('backup-names-list') || !box.classList.contains('hidden')) {
            out.errors.push('the names box is not collapsed by default');
          } else {
            toggle.click();
            await sleep(150);
            if (box.classList.contains('hidden')) out.errors.push('clicking the count did not reveal the names');
            // 103: real li rows (name / type chip / char count), not one comma string.
            const li = [...box.querySelectorAll('li.backup-name-row')];
            if (li.length !== 2) {
              out.errors.push('the expanded list does not show 2 real li rows (found ' + li.length + ')');
            } else {
              const text = box.textContent;
              if (!text.includes('Draft reply') || !text.includes('Meeting notes')) {
                out.errors.push('the expanded list is missing a fixture name: ' + text);
              }
              if (!li[0].querySelector('.backup-name-row-kind') || !li[0].querySelector('.backup-name-row-chars')) {
                out.errors.push('a backup name row is missing its type chip or char count');
              }
              if (li[1].querySelector('.backup-name-row-chars').textContent !== '—') {
                out.errors.push('a zero-char (legacy-cache) row does not show the placeholder dash');
              }
            }
            if (box.scrollWidth > box.clientWidth + 1) {
              out.errors.push('the expanded names list overflows horizontally');
            }
            toggle.click();
            await sleep(150);
            if (!box.classList.contains('hidden')) out.errors.push('clicking the count again did not collapse the names');
          }
        }
        if (withoutNames.querySelector('.backup-count-toggle')) {
          out.errors.push('a backup with no names still shows a count toggle');
        }
      }
    }
    // The X is positioned against its wrapper, so a field narrower than the wrapper
    // pushes it outside the box it belongs to.
    for (const w of document.querySelectorAll('#backup-modal .clear-wrap')) {
      const inp = w.querySelector('input');
      if (!inp) continue;
      const ir = inp.getBoundingClientRect(), br = w.querySelector('.clear-btn').getBoundingClientRect();
      if (ir.width && br.width && (br.right > ir.right || br.left < ir.left)) {
        out.errors.push('the clear button sits outside its field (' + (inp.name || inp.type) + ')');
      }
    }
    // Save on an empty field must do nothing at all, and removing a stored password
    // must ask first — it puts new backups back on the built-in key.
    const pwSave = document.querySelector('#backup-panel .backup-pw-save');
    const pwRemove = document.querySelector('#backup-panel .backup-pw-remove');
    const confirmEl = document.getElementById('confirm-modal');
    if (!pwSave || !pwRemove) {
      out.errors.push('backup password row is missing its save or remove button');
    } else {
      if (pwRemove.classList.contains('hidden')) out.errors.push('no remove button although a password is set');
      // A confirm dialog left over from the take-over check would fake a hit below.
      confirmEl.classList.add('hidden');
      pwSave.click();
      await sleep(300);
      if (!confirmEl.classList.contains('hidden')) {
        out.errors.push('Save on an empty password field starts a destructive action');
        document.getElementById('confirm-cancel').click();
        await sleep(200);
      }
      pwRemove.click();
      await sleep(300);
      if (confirmEl.classList.contains('hidden')) {
        out.errors.push('removing the backup password does not ask for confirmation');
      } else {
        document.getElementById('confirm-cancel').click();
        await sleep(200);
      }
    }
    // A cleanup candidate opens the library IN FRONT of the backups dialog, which
    // stays open so it can be returned to.
    const cand = document.querySelector('#stats-panel .stat-cleanup-item');
    if (!cand) {
      out.errors.push('the statistics list no cleanup candidates');
    } else {
      cand.click();
      await sleep(500);
      const lib = document.getElementById('library');
      if (lib.classList.contains('hidden')) out.errors.push('a cleanup candidate does not open the library');
      if (bm.classList.contains('hidden')) out.errors.push('the backups dialog was closed instead of stepped in front of');
      if (Number(getComputedStyle(lib).zIndex) <= Number(getComputedStyle(bm).zIndex)) {
        out.errors.push('the library opens behind the backups dialog');
      }
      document.getElementById('library-close').click();
      await sleep(300);
      if (document.body.classList.contains('library-front')) out.errors.push('the raised-library state outlives the library');
      if (bm.classList.contains('hidden')) out.errors.push('there is no backups dialog left to jump back to');
    }
    // P64/85: restoring a managed backup goes through maybeBackupBeforeWipe("delete")
    // — the same safety net used before delete-all, gated by the backupBeforeDelete
    // flag. That flag ships default ON (85), so the fixture's default state proves
    // restoreBackup actually wires the call through, cheap to verify without
    // walking the expert menu (which a later 130%-zoom layout check turned out to
    // be sensitive to — not worth touching under this pass's time budget).
    {
      const calls = () => (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []);
      calls().length = 0;
      const restoreBtn = [...document.querySelectorAll('#backup-panel button')]
        .find(b => b.textContent.trim() === 'Restore');
      if (!restoreBtn) {
        out.errors.push('the backup list has no Restore button');
      } else {
        restoreBtn.click();
        await sleep(200);
        // 70: both checkboxes default to checked — confirming as-is restores "all".
        const dataCb = document.getElementById('confirm-cb-data');
        const settingsCb = document.getElementById('confirm-cb-settings');
        if (!dataCb || !settingsCb) {
          out.errors.push('the restore confirm dialog has no data/settings checkboxes');
        } else if (!dataCb.checked || !settingsCb.checked) {
          out.errors.push('the restore checkboxes are not both checked by default');
        }
        await click('confirm-ok'); // the restore confirmation itself
        await sleep(200);
        if (!calls().includes('restore_backup')) out.errors.push('restoring did not call restore_backup at all');
        const allArgs = (window.__SMOKE_ARGS__ || []).find(c => c[0] === 'restore_backup');
        if (!allArgs || allArgs[1].scope !== 'all') {
          out.errors.push('both checkboxes checked must restore scope "all", got ' + (allArgs && allArgs[1].scope));
        }
        if (!calls().includes('create_backup')) {
          out.errors.push('no safety backup ran although backupBeforeDelete is on by default');
        }
      }
    }
    // 70: unchecking one checkbox switches to a selective restore scope.
    {
      const restoreBtn = [...document.querySelectorAll('#backup-panel button')]
        .find(b => b.textContent.trim() === 'Restore');
      if (restoreBtn) {
        window.__SMOKE_ARGS__ = [];
        restoreBtn.click();
        await sleep(200);
        document.getElementById('confirm-cb-settings').click(); // uncheck — data only
        await click('confirm-ok');
        await sleep(200);
        const dataArgs = (window.__SMOKE_ARGS__ || []).find(c => c[0] === 'restore_backup');
        if (!dataArgs || dataArgs[1].scope !== 'data') {
          out.errors.push('unchecking settings must restore scope "data", got ' + (dataArgs && dataArgs[1].scope));
        }
      }
    }
    // 70: unchecking both leaves nothing to restore — the confirm button must refuse.
    {
      const restoreBtn = [...document.querySelectorAll('#backup-panel button')]
        .find(b => b.textContent.trim() === 'Restore');
      if (restoreBtn) {
        restoreBtn.click();
        await sleep(200);
        document.getElementById('confirm-cb-data').click();
        document.getElementById('confirm-cb-settings').click();
        await sleep(100);
        if (!document.getElementById('confirm-ok').disabled) {
          out.errors.push('unchecking both restore checkboxes must disable the confirm button');
        }
        document.getElementById('confirm-cancel').click();
        await sleep(150);
      }
    }
    // 448: a snapshot's content viewer shows real counts, its own list can be
    // searched and sorted, and restoring from inside it reuses restoreBackup()
    // rather than a second path -- same command, same confirm dialog.
    {
      const settleContent = async (fn) => { for (let i = 0; i < 60 && !fn(); i++) await sleep(50); return fn(); };
      const row0 = document.querySelectorAll('#backup-panel .backup-row')[0];
      const contentBox = document.querySelectorAll('#backup-panel .backup-content')[0];
      const viewBtn = row0 && [...row0.querySelectorAll('button')].find(b => b.textContent.trim() === 'View content');
      if (!row0 || !contentBox || !viewBtn) {
        out.errors.push('the backup row has no View content button or content box');
      } else {
        if (!contentBox.classList.contains('hidden')) {
          out.errors.push('a backup content box starts open instead of collapsed');
        }
        viewBtn.click();
        const loaded = await settleContent(() => !contentBox.classList.contains('hidden') && contentBox.querySelector('.stat-card'));
        if (!loaded) {
          out.errors.push('clicking View content never revealed the stat cards');
        } else {
          const cardText = [...contentBox.querySelectorAll('.stat-card')].map(c => c.textContent).join(' | ');
          if (contentBox.querySelectorAll('.stat-card').length !== 6) {
            out.errors.push('the content viewer does not show all six stat cards: ' + cardText);
          }
          if (!cardText.includes('3') || !cardText.includes('2.6.0') || !cardText.includes('Yes')) {
            out.errors.push('the stat cards do not reflect the fixture data: ' + cardText);
          }
          const nameOf = (r) => r.querySelector('.backup-content-cell-name').textContent;
          const dataRows = () => [...contentBox.querySelectorAll('.backup-content-rows .backup-content-row')];
          if (dataRows().length !== 3) {
            out.errors.push('the content table does not list all three fixture entries: ' + dataRows().map(nameOf).join(', '));
          }
          const search = contentBox.querySelector('.backup-content-search');
          search.value = 'apple';
          search.dispatchEvent(new Event('input'));
          await sleep(100);
          if (dataRows().length !== 1 || nameOf(dataRows()[0]) !== 'Apple pie recipe') {
            out.errors.push('searching apple did not narrow the list to the one matching entry: ' + dataRows().map(nameOf).join(', '));
          }
          search.value = '';
          search.dispatchEvent(new Event('input'));
          await sleep(100);
          const nameHead = contentBox.querySelector('.backup-content-head .backup-content-cell-name');
          if (!nameHead) {
            out.errors.push('the content table has no clickable name column header');
          } else {
            // The table opens already sorted by name ascending, so the header
            // shows as sorted before it is ever clicked.
            let names = dataRows().map(nameOf);
            if (names[0] !== 'Apple pie recipe' || !nameHead.classList.contains('sorted')) {
              out.errors.push('the content table does not open sorted by name ascending: ' + names.join(', '));
            }
            nameHead.click(); // same column again -- flips to descending
            await sleep(100);
            names = dataRows().map(nameOf);
            if (names[0] !== 'Meeting notes' || !nameHead.classList.contains('sorted-desc')) {
              out.errors.push('sorting by name descending does not put Meeting notes first: ' + names.join(', '));
            }
            nameHead.click(); // back to ascending
            await sleep(100);
            names = dataRows().map(nameOf);
            if (names[0] !== 'Apple pie recipe' || nameHead.classList.contains('sorted-desc')) {
              out.errors.push('clicking name a second time did not return to ascending: ' + names.join(', '));
            }
          }
          const dateHead = contentBox.querySelector('.backup-content-head .backup-content-cell-date');
          if (dateHead) {
            dateHead.click(); // new column -- starts ascending
            await sleep(100);
            let names = dataRows().map(nameOf);
            if (names[names.length - 1] !== 'Meeting notes') {
              out.errors.push('sorting by date ascending does not put the never-edited entry last: ' + names.join(', '));
            }
            dateHead.click(); // same column again -- flips to descending
            await sleep(100);
            names = dataRows().map(nameOf);
            if (names[names.length - 1] !== 'Meeting notes') {
              out.errors.push('sorting by date descending does not put the never-edited entry last: ' + names.join(', '));
            }
          }
          search.value = 'no such prompt exists anywhere';
          search.dispatchEvent(new Event('input'));
          await sleep(100);
          const emptyHint = contentBox.querySelector('.backup-content-empty');
          if (dataRows().length !== 0 || !emptyHint || emptyHint.classList.contains('hidden')) {
            out.errors.push('a search with no matches does not show the empty-list hint');
          }
          search.value = '';
          search.dispatchEvent(new Event('input'));
          await sleep(100);
          window.__SMOKE_ARGS__ = [];
          const contentRestoreBtn = contentBox.querySelector('.backup-content-toolbar button');
          if (!contentRestoreBtn) {
            out.errors.push('the content viewer toolbar has no restore button');
          } else {
            contentRestoreBtn.click();
            await sleep(200);
            if (document.getElementById('confirm-modal').classList.contains('hidden')) {
              out.errors.push('restoring from the content viewer does not open the restore confirm dialog');
            } else {
              await click('confirm-ok');
              await sleep(200);
              const args = (window.__SMOKE_ARGS__ || []).find(c => c[0] === 'restore_backup');
              if (!args || args[1].name !== '2026-07-25_18-30-00') {
                out.errors.push('restoring from the content viewer targeted the wrong backup: ' + (args && args[1].name));
              }
            }
          }
        }
      }
    }
    await click('backup-close');
    // Restoring a password-protected backup: the prompt must appear, reject a wrong
    // password with a new message, then accept the right one and confirm the result.
    const until = async (fn) => { for (let i = 0; i < 40 && !fn(); i++) await sleep(100); return fn(); };
    const pwShown = () => !document.getElementById('confirm-input-row').classList.contains('hidden');
    const scopeShown = () => document.getElementById('confirm-cb-data') && !document.getElementById('confirm-modal').classList.contains('hidden');
    document.getElementById('settings-backup-btn').click();
    await sleep(400);
    document.getElementById('backup-import-file').click();
    // 107: a scope-pick dialog (data/settings checkboxes, both preselected) now
    // comes first -- confirm it before the password prompt can appear.
    if (!await until(scopeShown)) {
      out.errors.push('no scope-pick dialog before a full import');
    } else {
      await click('confirm-ok');
    }
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
      // F20: hotkeys the restore had to give up are named, not silently dropped.
      else if (!/hotkey/i.test(document.getElementById('confirm-msg').textContent)) {
        out.errors.push('the import summary does not mention the dropped hotkeys');
      }
      await click('confirm-ok');
    }
    if (!document.getElementById('backup-modal').classList.contains('hidden')) { await click('backup-close'); await sleep(150); }
    // 96: the short "Latest X.Y.Z" status keeps update-actions no taller than
    // before, with the longest of the 11 translations (Romanian) -- and exactly
    // as tall as the button row below it once the window has real room (740px+;
    // below that the About/Check-for-updates button labels themselves already
    // do not fit beside a third item, independent of what that item says).
    {
      if (document.getElementById('settings').classList.contains('hidden')) await click('gear');
      const ua = document.querySelector('.update-actions');
      const btnRow = document.querySelector('.settings-btn-row');
      const uv = document.getElementById('update-version');
      if (ua && btnRow && uv) {
        const prevText = uv.textContent;
        uv.textContent = 'Cea mai recenta 2.7.0';
        const uaH = ua.getBoundingClientRect().height;
        uv.textContent = 'Neueste Version bereits installiert';
        const uaHOld = ua.getBoundingClientRect().height;
        const rowH = btnRow.getBoundingClientRect().height;
        if (uaH > uaHOld + 1) {
          out.errors.push('the short status is taller than the old long sentence was: ' + Math.round(uaH) + ' vs ' + Math.round(uaHOld));
        }
        if (window.innerWidth >= 740 && uaH > rowH + 1) {
          out.errors.push('update-actions is taller than settings-btn-row at ' + window.innerWidth + 'px: ' + Math.round(uaH) + ' vs ' + Math.round(rowH));
        }
        // 174: the version label sits centred in its own column, same width as
        // the About/Check-for-updates buttons beside it (only meaningful
        // unwrapped -- at 740px+ the three sit on one row per the check above).
        const aboutBtn = document.getElementById('about-btn');
        if (window.innerWidth >= 740 && aboutBtn) {
          const uvRect = uv.getBoundingClientRect();
          const abRect = aboutBtn.getBoundingClientRect();
          if (Math.abs(uvRect.width - abRect.width) > 2) {
            out.errors.push('update-version column width does not match about-btn: ' + Math.round(uvRect.width) + ' vs ' + Math.round(abRect.width));
          }
          const cs = getComputedStyle(uv);
          if (cs.textAlign !== 'center') {
            out.errors.push('update-version is not centre-aligned: ' + cs.textAlign);
          }
        }
        uv.textContent = prevText;
      } else {
        out.errors.push('update-actions, settings-btn-row or update-version missing from the settings panel');
      }
    }
    // 108: neither reset-settings nor delete-data wraps to a second line, even
    // with the longest of the 11 translations' one-word labels (German
    // "Zuruecksetzen", Dutch "Verwijderen") -- flex-row default stretch means a
    // single wrapped button grows the WHOLE row, so compare against the row's
    // own single-line reference height rather than siblings.
    {
      if (document.getElementById('settings-main').classList.contains('hidden')) {
        document.getElementById('expert-open').click();
        await sleep(50);
      }
      const reset = document.getElementById('reset-settings');
      const del = document.getElementById('delete-data');
      const help = document.getElementById('help-open');
      if (!reset || !del || !help) {
        out.errors.push('reset-settings, delete-data or help-open missing from settings-btn-row');
      } else if (help.getBoundingClientRect().height < 10) {
        out.errors.push('settings-btn-row is not visible at ' + window.innerWidth + 'px');
      } else {
        const baseH = Math.round(help.getBoundingClientRect().height);
        const prevReset = reset.textContent;
        const prevDel = del.textContent;
        reset.textContent = 'Zuruecksetzen';
        del.textContent = 'Verwijderen';
        const longH = Math.round(help.getBoundingClientRect().height);
        reset.textContent = prevReset;
        del.textContent = prevDel;
        if (longH > baseH + 1) {
          out.errors.push('settings-btn-row grows with the longest one-word translation at ' + window.innerWidth + 'px: ' + baseH + ' -> ' + longH);
        }
      }
    }
    // 109: hide-top (left of Minimize) and hide-bottom (left of Save) share the
    // same left/right edges -- both bars right-align on the same window edge and
    // win-controls/Save share --winctl-w. Also: hiding both toggles (barToggles
    // off) must not leave a gap where they used to sit. Only checked >= 700px:
    // below the header's own ~690px wrap breakpoint, header-actions becomes the
    // SOLE item on its wrapped line and `.header`'s justify-content:space-between
    // has no second item left to space against, so it falls back to packing left
    // instead of right -- a pre-existing quirk outside this point's scope, and
    // exactly why the spec only asks for 900px/1280px here.
    {
      if (!document.getElementById('settings').classList.contains('hidden')) {
        await click('settings-close');
        await sleep(50);
      }
      const ht = document.getElementById('hide-top');
      const hb = document.getElementById('hide-bottom');
      const winMin = document.getElementById('win-min');
      const winControls = document.querySelector('.win-controls');
      const saveBtn = document.getElementById('save');
      if (!ht || !hb || !winMin || !winControls || !saveBtn) {
        out.errors.push('hide-top, hide-bottom, win-min, win-controls or save missing');
      } else if (window.innerWidth >= 700) {
        const htR = ht.getBoundingClientRect();
        const hbR = hb.getBoundingClientRect();
        if (Math.abs(htR.right - hbR.right) > 1 || Math.abs(htR.left - hbR.left) > 1) {
          out.errors.push('hide-top/hide-bottom are not aligned at ' + window.innerWidth + 'px: left ' +
            Math.round(htR.left) + '/' + Math.round(hbR.left) + ', right ' + Math.round(htR.right) + '/' + Math.round(hbR.right));
        }
        if (htR.right > winMin.getBoundingClientRect().left + 1) {
          out.errors.push('hide-top is not left of win-min at ' + window.innerWidth + 'px');
        }
        if (hbR.right > saveBtn.getBoundingClientRect().left + 1) {
          out.errors.push('hide-bottom is not left of save at ' + window.innerWidth + 'px');
        }
        const wcRightBefore = Math.round(winControls.getBoundingClientRect().right);
        const saveRightBefore = Math.round(saveBtn.getBoundingClientRect().right);
        document.body.classList.add('noflag-barToggles');
        const wcRightAfter = Math.round(winControls.getBoundingClientRect().right);
        const saveRightAfter = Math.round(saveBtn.getBoundingClientRect().right);
        document.body.classList.remove('noflag-barToggles');
        if (Math.abs(wcRightAfter - wcRightBefore) > 1 || Math.abs(saveRightAfter - saveRightBefore) > 1) {
          out.errors.push('hiding the bar-toggle buttons shifts win-controls/save at ' + window.innerWidth + 'px');
        }
      }
    }
    // 426: a double-click on any of the four bar buttons folds/unfolds both
    // bars together while barToggleBoth stays on; a single click is untouched
    // either way. A real double-click fires click, click, dblclick -- click()
    // exercises the existing single-bar toggle, the synthetic dblclick layers
    // the extra behaviour on top, same as a genuine double-click would.
    {
      if (!document.getElementById('settings').classList.contains('hidden')) {
        await click('settings-close');
        await sleep(50);
      }
      const dbl = async (id) => {
        await click(id);
        document.getElementById(id).dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(100);
      };
      const bothHidden = () => document.body.classList.contains('no-header') && document.body.classList.contains('no-composer');
      const bothShown = () => !document.body.classList.contains('no-header') && !document.body.classList.contains('no-composer');
      await dbl('hide-top');
      if (!bothHidden()) out.errors.push('426: double-clicking hide-top did not fold both bars');
      await dbl('show-bottom');
      if (!bothShown()) out.errors.push('426: double-clicking show-bottom did not unfold both bars');
      // Switch barToggleBoth off through its real expert switch, same idiom as
      // the prompt-hotkey-paste option above.
      const expertRow426 = async (label) => {
        const hit = () => [...document.querySelectorAll('#expert-flags .switch-field')]
          .find(r => ((r.querySelector('span') || {}).textContent || '').includes(label));
        if (hit()) return hit();
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          await sleep(200);
          if (hit()) return hit();
        }
        return null;
      };
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const bothRow = await expertRow426('folds both bars');
      if (!bothRow) {
        out.errors.push('426: the double-click-folds-both option is missing from the expert menu');
      } else {
        bothRow.querySelector('input.switch').click();
        await sleep(200);
        await click('expert-back');
        document.getElementById('settings').classList.add('hidden');
        await dbl('hide-top');
        if (document.body.classList.contains('no-composer')) {
          out.errors.push('426: double-click still folded the other bar with the option off');
        }
        if (!document.body.classList.contains('no-header')) {
          out.errors.push('426: the plain single click stopped working with the option off');
        }
        // Undo the single click, then put the option back for the rest of the run.
        await click('show-top');
        document.getElementById('settings').classList.remove('hidden');
        await click('expert-open');
        const backRow = await expertRow426('folds both bars');
        if (backRow) { backRow.querySelector('input.switch').click(); await sleep(200); }
        const firstTab426 = document.querySelector('#expert-tabs-bar .expert-tab');
        if (firstTab426) { firstTab426.click(); await sleep(200); }
        await click('expert-back');
        document.getElementById('settings').classList.add('hidden');
      }
    }
    // 427: a synthesised "copy" sound plays on every successful copy -- exactly
    // once, only while the switch is on, never at volume 0. A stand-in
    // AudioContext counts real oscillator starts; four different prompt ids
    // keep the per-prompt copy cooldown out of the way without touching it.
    {
      window.__BEEP_COUNT__ = 0;
      window.AudioContext = window.webkitAudioContext = function () {
        return {
          currentTime: 0,
          createOscillator: () => ({ frequency: { value: 0 }, connect() { return this; }, start() { window.__BEEP_COUNT__++; }, stop() {} }),
          createGain: () => ({ gain: { setValueAtTime() {}, linearRampToValueAtTime() {} }, connect() { return this; } }),
          destination: {},
        };
      };
      const expertRow427 = async (label) => {
        const hit = () => [...document.querySelectorAll('#expert-flags .switch-field')]
          .find(r => ((r.querySelector('span') || {}).textContent || '').includes(label));
        if (hit()) return hit();
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          await sleep(200);
          if (hit()) return hit();
        }
        return null;
      };
      const valueRow427 = () => [...document.querySelectorAll('#expert-flags .value-field')]
        .find(r => ((r.querySelector('.value-head span') || {}).textContent || '').includes('Copy sound volume'));
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const soundRow = await expertRow427('Sound on copy');
      if (!soundRow) {
        out.errors.push('427: the copy-sound option is missing from the expert menu');
      } else {
        soundRow.querySelector('input.switch').click();
        await sleep(200);
        window.__FIRE__('prompt-hotkey', 'p0');
        await sleep(300);
        if (window.__BEEP_COUNT__ !== 1) out.errors.push('427: turning copySound on did not play a beep on copy: ' + window.__BEEP_COUNT__);
        window.__FIRE__('prompt-hotkey', 'p2');
        await sleep(300);
        if (window.__BEEP_COUNT__ !== 2) out.errors.push('427: a second copy did not play a second beep: ' + window.__BEEP_COUNT__);
        // Volume 0 must play nothing.
        const volRow = valueRow427();
        if (!volRow) {
          out.errors.push('427: the copy-sound-volume slider is missing from the expert menu');
        } else {
          const slider = volRow.querySelector('input.value-range');
          slider.value = 0;
          slider.dispatchEvent(new Event('input', { bubbles: true }));
          slider.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(200);
        }
        window.__FIRE__('prompt-hotkey', 'p4');
        await sleep(300);
        if (window.__BEEP_COUNT__ !== 2) out.errors.push('427: volume 0 still played a beep: ' + window.__BEEP_COUNT__);
        // Switching the option off must play nothing either, even back at a normal volume.
        if (volRow) {
          const slider = volRow.querySelector('input.value-range');
          slider.value = 30;
          slider.dispatchEvent(new Event('input', { bubbles: true }));
          slider.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(200);
        }
        // Re-fetch the row: copySound gates copySoundVolume's visibility, so its own
        // switch is a GATING_FLAGS member and toggling it just rebuilt the whole
        // tab -- the earlier `soundRow` element is now detached from the document.
        const soundRowOff = await expertRow427('Sound on copy');
        if (soundRowOff) soundRowOff.querySelector('input.switch').click();
        await sleep(200);
        window.__FIRE__('prompt-hotkey', 'p6');
        await sleep(300);
        if (window.__BEEP_COUNT__ !== 2) out.errors.push('427: copySound off still played a beep: ' + window.__BEEP_COUNT__);
        const firstTab427 = document.querySelector('#expert-tabs-bar .expert-tab');
        if (firstTab427) { firstTab427.click(); await sleep(200); }
      }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // 431: the password-skip switch only matters once the clipboard inbox itself is
    // on and defaults to on; its former length slider is a fixed value now.
    {
      const valueRow431 = (label) => [...document.querySelectorAll('#expert-flags .value-field')]
        .find(r => ((r.querySelector('.value-head span') || {}).textContent || '').includes(label));
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const watcherRow = await expertRow('Record copies from other apps');
      if (!watcherRow) {
        out.errors.push('431: the record-copies option is missing from the expert menu');
      } else {
        watcherRow.querySelector('input.switch').click();
        await sleep(200);
        const skipRow = await expertRow('Skip likely passwords');
        if (!skipRow) {
          out.errors.push('431: the password-skip switch does not appear once the inbox is on');
        } else {
          if (!skipRow.querySelector('input.switch').checked) {
            out.errors.push('431: the password-skip switch is not on by default');
          }
          if (valueRow431('Password guess minimum length')) {
            out.errors.push('431: the minimum-length slider is still in the expert menu');
          }
        }
        // Switching the inbox off with items still collected asks once whether to
        // keep them (the fixture's inbox holds one item). Every answer is probed:
        // keep, delete and Escape; the last leaves the switch off, the shipped default.
        const confirmBox431 = document.getElementById('confirm-modal');
        const clipCalls431 = (cmd) => (window.__SMOKE_ARGS__ || []).filter(([c]) => c === cmd).length;
        const switchWatcher431 = async (answer) => {
          const row = await expertRow('Record copies from other apps');
          const sw = row && row.querySelector('input.switch');
          if (!sw) { out.errors.push('431: the record-copies switch vanished'); return false; }
          window.__SMOKE_ARGS__ = [];
          sw.click();
          await sleep(300);
          if (sw.checked) return true; // switched on: nothing to answer
          if (confirmBox431.classList.contains('hidden')) {
            out.errors.push('431: switching the inbox off with items did not ask what to do with them');
            return false;
          }
          if (answer === 'keep') document.getElementById('confirm-cancel').click();
          else if (answer === 'delete') document.getElementById('confirm-ok').click();
          else document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
          await sleep(300);
          if (!confirmBox431.classList.contains('hidden')) {
            out.errors.push('431: the inbox keep/delete question stayed open after the answer ' + answer);
            confirmBox431.classList.add('hidden');
          }
          const cleared = clipCalls431('clip_inbox_clear');
          if (answer === 'delete' && cleared !== 1) out.errors.push('431: deleting the collected items called clip_inbox_clear ' + cleared + ' times');
          if (answer !== 'delete' && cleared !== 0) out.errors.push('431: answer ' + answer + ' still cleared the inbox');
          return true;
        };
        await switchWatcher431('keep');
        await switchWatcher431('on');
        await switchWatcher431('delete');
        await switchWatcher431('on');
        await switchWatcher431('escape');
        const watcherEnd431 = await expertRow('Record copies from other apps');
        if (watcherEnd431 && watcherEnd431.querySelector('input.switch').checked) {
          out.errors.push('431: the inbox switch did not end up off');
        }
        await sleep(200);
        const firstTab431 = document.querySelector('#expert-tabs-bar .expert-tab');
        if (firstTab431) { firstTab431.click(); await sleep(150); }
      }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // 108: Save must fit --winctl-w in every language -- the longest translations
    // ("Speichern", "Speichern" DE at 9 chars is the longest of the 11) must not
    // overflow the fixed-width button.
    {
      const saveBtn = document.getElementById('save');
      if (saveBtn) {
        const prev = saveBtn.textContent;
        for (const word of ['Save', 'Speichern', 'Guardar', 'Salva', 'Salvar', 'Zapisz', 'Opslaan', 'Kaydet', 'Salveaza', '保存']) {
          saveBtn.textContent = word;
          if (saveBtn.scrollWidth > saveBtn.clientWidth + 1) {
            out.errors.push('save button overflows with "' + word + '" at ' + window.innerWidth + 'px');
          }
        }
        saveBtn.textContent = prev;
      }
    }
    // 275: about-btn/update-btn/update-version share a fixed one-third-width
    // column each -- the longest translation in each of the 11 languages must
    // not overflow its own box, at the two smallest viewports the suite runs.
    if (window.innerWidth <= 740) {
      const aboutBtn = document.getElementById('about-btn');
      const updateBtn = document.getElementById('update-btn');
      const updateVersion = document.getElementById('update-version');
      if (!aboutBtn || !updateBtn || !updateVersion) {
        out.errors.push('about-btn, update-btn or update-version missing at ' + window.innerWidth + 'px');
      } else {
        const fits = (el) => el.scrollHeight <= el.clientHeight + 1 && el.scrollWidth <= el.clientWidth + 1;
        const prevAbout = aboutBtn.textContent;
        for (const word of ['About', 'Über', 'Acerca de', 'Informazioni', 'Sobre', 'O programie', '关于', 'バージョン情報', 'Over', 'Hakkında', 'Despre']) {
          aboutBtn.textContent = word;
          if (!fits(aboutBtn)) out.errors.push('about-btn overflows with "' + word + '" at ' + window.innerWidth + 'px');
        }
        aboutBtn.textContent = prevAbout;
        const prevUpdate = updateBtn.textContent;
        for (const word of ['Check for updates', 'Update suchen', 'Buscar actualizaciones', 'Cerca aggiornamenti',
          'Verificar atualizações', 'Sprawdź aktualizacje', '检查更新', 'アップデートを確認', 'Controleren op updates',
          'Güncellemeleri denetle', 'Caută actualizări']) {
          updateBtn.textContent = word;
          if (!fits(updateBtn)) out.errors.push('update-btn overflows with "' + word + '" at ' + window.innerWidth + 'px');
        }
        updateBtn.textContent = prevUpdate;
        const prevVersion = updateVersion.textContent;
        for (const word of ['Version 2.7.0', 'Version 2.7.0', 'Versión 2.7.0', 'Versione 2.7.0', 'Versão 2.7.0',
          'Wersja 2.7.0', '版本 2.7.0', 'バージョン 2.7.0', 'Versie 2.7.0', 'Sürüm 2.7.0', 'Versiunea 2.7.0']) {
          updateVersion.textContent = word;
          if (updateVersion.scrollHeight > updateVersion.clientHeight + 1) {
            out.errors.push('update-version wraps to a second line with "' + word + '" at ' + window.innerWidth + 'px');
          }
        }
        updateVersion.textContent = prevVersion;
      }
    }
    // The changelog popup renders the release Markdown instead of dumping it, and
    // grows with the text rather than clipping it.
    await click('update-btn');
    await click('update-version');
    const notes = document.getElementById('update-modal-notes');
    if (document.getElementById('update-modal').classList.contains('hidden')) {
      out.errors.push('the changelog popup did not open');
    } else {
      if (!notes.querySelector('h3') || !notes.querySelector('li') || !notes.querySelector('table')) {
        out.errors.push('the changelog is not rendered as markup');
      }
      if (/[#*`|]/.test(notes.textContent)) {
        out.errors.push('raw Markdown left in the changelog: ' + notes.textContent.slice(0, 60));
      }
      if (notes.textContent.includes('9.9.9')) out.errors.push('the changelog repeats the version from its own title');
      if (notes.scrollHeight > notes.clientHeight + 1 && getComputedStyle(notes).overflowY !== 'auto') {
        out.errors.push('long release notes cannot be scrolled');
      }
      await click('update-modal-cancel');
    }
    // The appearance scales are user-tunable, so the layout has to survive them being
    // raised — the same guards once more with modal/icon/composer zoom at 130%.
    const zoomed = document.documentElement.style;
    document.getElementById('settings').classList.add('hidden');
    for (const v of ['--modal-zoom', '--icon-zoom', '--composer-zoom', '--primary-zoom']) {
      zoomed.setProperty(v, '1.3');
    }
    await sleep(400);
    layoutScan('main window at 130% scale');
    mustReach('main window at 130% scale', '.win-btn, .composer > button');
    await click('library-btn');
    layoutScan('library at 130% scale');
    await click('library-close');
    document.getElementById('settings').classList.remove('hidden');
    await sleep(250);
    layoutScan('settings at 130% scale');
    // Clipped away by a dialog that cannot scroll counts as unreachable: elementFromPoint
    // respects the clip, and scrollIntoView has nothing to scroll.
    mustReach('settings at 130% scale', '#settings-main button, #settings-main select');
    // 516: the redesigned About dialog at 130% scale.
    document.getElementById('about-btn').click();
    await sleep(150);
    if (window.__ABOUT_FITS_CHECK__) window.__ABOUT_FITS_CHECK__('130% scale');
    document.getElementById('about-ok').click();
    await sleep(120);
    await click('expert-open');
    layoutScan('expert menu at 130% scale');
    // Indentation is the only thing that says which setting belongs to which
    // toggle. Every nested row has to actually step in, one level per depth.
    for (const el of document.querySelectorAll('#expert-flags .expert-sub')) {
      const depth = Number(el.style.getPropertyValue('--expert-depth') || 0);
      const inset = parseFloat(getComputedStyle(el).marginLeft) || 0;
      if (!(depth > 0)) out.layout.push('expert menu: a nested row carries no depth');
      else if (inset < depth * 10) {
        out.layout.push('expert menu: a depth-' + depth + ' row is indented ' +
          Math.round(inset) + 'px, so it does not read as nested');
      }
    }
    await click('expert-back');
    document.getElementById('settings').classList.add('hidden');
    for (const v of ['--modal-zoom', '--icon-zoom', '--composer-zoom', '--primary-zoom']) {
      zoomed.removeProperty(v);
    }
    await sleep(300);
    // Leave the theme editor open so the scans below measure its real layout.
    document.getElementById('settings').classList.remove('hidden');
    await click('theme-edit-open');
    // Every dropdown and swatch in the theme editor is anchored low in the window —
    // exactly where a popover used to open past the bottom edge. Open the LAST of
    // each kind, which is the worst case, and let the popover scan judge it.
    const lastVisible = (sel) => [...document.querySelectorAll(sel)]
      .filter(e => e.getClientRects().length).pop();
    for (const [sel, what] of [['#theme-modal select', 'theme editor dropdown'],
                               ['#theme-modal .swatch, #theme-modal .color-dot', 'theme editor colour']]) {
      const el = lastVisible(sel);
      if (!el) continue;
      el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      el.click();
      await sleep(250);
      layoutScan(what);
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      await sleep(150);
    }
    await click('theme-modal-close');
    // 112: walk the same views the 130% pass just covered once per wordy language.
    // Polish, Romanian and German write the longest labels of the eleven, and a
    // clipped label or an overlapping box is exactly what layoutScan already
    // catches -- this just points it at real long strings instead of a synthetic
    // scale. German comes last on purpose: the OCR checks below find their expert
    // rows by German label, so that is the language the page has to be left in.
    document.getElementById('settings-expert').classList.add('hidden');
    document.getElementById('settings-main').classList.remove('hidden');
    document.getElementById('settings').classList.remove('hidden');
    const langSel = document.getElementById('lang-select');
    for (const lang of ['pl', 'ro', 'de']) {
      langSel.value = lang;
      langSel.dispatchEvent(new Event('change'));
      await sleep(300);
      if (document.documentElement.lang !== lang) {
        out.errors.push('lang-select change did not switch the active language to ' + lang +
          ' (still ' + document.documentElement.lang + ')');
      }
      layoutScan('main window in ' + lang);
      mustReach('main window in ' + lang, '.win-btn, .composer > button');
      await click('library-btn');
      layoutScan('library in ' + lang);
      await click('library-close');
      layoutScan('settings in ' + lang);
      mustReach('settings in ' + lang, '#settings-main button, #settings-main select');
      // 521: the shortened German label keeps the folder-import button's own
      // text on one line -- a flex row with align-items stretch equalizes
      // every button's box height regardless, so the box height alone (the
      // first thing tried here) cannot tell a wrapped label from a plain one;
      // only the text's own line count can.
      if (lang === 'de') {
        const folderBtn521 = document.getElementById('import-folder-btn');
        if (!folderBtn521) {
          out.errors.push('521: could not find the folder-import button in de');
        } else {
          const label521 = [...folderBtn521.childNodes].find((n) => n.nodeType === 3 && n.textContent.trim());
          if (!label521) {
            out.errors.push('521: the folder-import button has no text label to measure in de');
          } else {
            const range521 = document.createRange();
            range521.selectNodeContents(label521);
            const tops521 = new Set([...range521.getClientRects()].map((r) => Math.round(r.top)));
            if (tops521.size > 1) {
              out.errors.push('521: the folder-import button label wraps to ' + tops521.size + ' lines in de');
            }
          }
        }
      }
      // 516: the redesigned About dialog in the longest-label languages.
      document.getElementById('about-btn').click();
      await sleep(150);
      if (window.__ABOUT_FITS_CHECK__) window.__ABOUT_FITS_CHECK__(lang);
      document.getElementById('about-ok').click();
      await sleep(120);
      await click('expert-open');
      layoutScan('expert menu in ' + lang);
      await click('expert-back');
    }
    // 117/118/119: screenshot editor -- window.__FIRE__ is the same hook every other
    // probe uses to exercise a backend-started flow (see tauri_stub.js), here firing
    // the snip-captured event finalize_capture would normally send.
    {
      const TINY_PNG = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
      window.__FIRE__('snip-captured', { data_url: TINY_PNG, path: '', name: 'test.png' });
      await sleep(300);
      const edit = document.getElementById('snip-edit');
      if (!edit || edit.classList.contains('hidden')) {
        out.errors.push('screenshot editor did not open on snip-captured');
      } else {
        const toolBtns = [...document.querySelectorAll('#snip-edit-tools button[data-tool]')];
        if (toolBtns.length !== 12) { // 284 added the Hand (pan) tool
          out.errors.push('screenshot editor: expected 12 tool buttons, found ' + toolBtns.length);
        }
        // 178: the toolbar must stay a couple of rows tall no matter which tool
        // is active -- a full inline swatch grid used to collapse into one tall
        // column here and blow the toolbar height up. This is the check that
        // would have caught that bug; a generic layoutScan alone did not.
        // 507 raised the ceiling from 96 to 200: the context row now wraps
        // its fields onto a second (and for the busiest tool, a third) line
        // instead of scrolling them sideways, so two or three lines of real
        // controls is expected height, not a regression. A collapsed swatch
        // grid is still caught well below 200 -- that failure mode stacks a
        // whole column of swatches, not two or three short field rows.
        const seToolbar = document.querySelector('.snip-edit-toolbar');
        const SE_TOOLBAR_CEIL = 200;
        for (const b of toolBtns) {
          b.click();
          await sleep(30);
          if (b.getAttribute('aria-pressed') !== 'true') {
            out.errors.push('screenshot editor: ' + b.dataset.tool + ' did not mark aria-pressed after click');
          }
          const pressedCount = toolBtns.filter((x) => x.getAttribute('aria-pressed') === 'true').length;
          if (pressedCount !== 1) {
            out.errors.push('screenshot editor: ' + pressedCount + ' tools show aria-pressed at once (expected exactly 1)');
          }
          if (seToolbar) {
            const h = seToolbar.getBoundingClientRect().height;
            if (h > SE_TOOLBAR_CEIL) {
              out.errors.push('screenshot editor toolbar is ' + Math.round(h) + 'px tall with the ' +
                b.dataset.tool + ' tool active, over the ' + SE_TOOLBAR_CEIL + 'px ceiling');
            }
          }
        }
        const penBtn = toolBtns.find((b) => b.dataset.tool === 'pen');
        if (penBtn) { penBtn.click(); await sleep(30); }
        // The pen tool has a color field: a single trigger button opening a
        // popover, not an inline grid, is what keeps the toolbar row short.
        const colorTrigger = document.querySelector('.snip-edit-color-trigger');
        if (!colorTrigger) {
          out.errors.push('screenshot editor: pen tool has no color trigger button');
        } else {
          colorTrigger.click();
          await sleep(30);
          const pop = document.querySelector('.snip-color-pop');
          if (!pop || pop.classList.contains('hidden')) {
            out.errors.push('screenshot editor: the color trigger did not open the palette popover');
          } else {
            if (seToolbar && seToolbar.getBoundingClientRect().height > SE_TOOLBAR_CEIL) {
              out.errors.push('screenshot editor toolbar grew past ' + SE_TOOLBAR_CEIL + 'px with the color popover open');
            }
            const popSwatches = pop.querySelectorAll('.swatch');
            if (popSwatches.length < 3) {
              out.errors.push('screenshot editor: the color popover has only ' + popSwatches.length + ' swatches');
            }
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
            await sleep(30);
            if (!pop.classList.contains('hidden')) {
              out.errors.push('screenshot editor: Escape did not close the color popover');
            }
            if (document.getElementById('snip-edit').classList.contains('hidden')) {
              out.errors.push('screenshot editor: Escape closed the whole editor instead of just the color popover');
            }
            document.querySelector('.snip-edit-color-trigger').click();
            await sleep(30);
            const pick = [...document.querySelectorAll('.snip-color-pop .swatch')]
              .find((s) => !s.classList.contains('none') && !s.classList.contains('custom'));
            if (pick) {
              pick.click();
              await sleep(30);
              const chip = document.querySelector('.snip-edit-color-chip');
              if (!chip || !chip.style.background) {
                out.errors.push('screenshot editor: picking a swatch did not color the trigger chip');
              }
              if (!document.querySelector('.snip-color-pop').classList.contains('hidden')) {
                out.errors.push('screenshot editor: picking a swatch did not close the color popover');
              }
            }
          }
        }
        layoutScan('screenshot editor');
        const live = document.getElementById('snip-edit-live');
        const undoBtn = document.getElementById('snip-edit-undo');
        if (live && undoBtn) {
          const r = live.getBoundingClientRect();
          const x = r.left + r.width / 2, y = r.top + r.height / 2;
          live.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: x, clientY: y }));
          live.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: x + 5, clientY: y + 5 }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: x + 5, clientY: y + 5 }));
          await sleep(100);
          if (undoBtn.disabled) out.errors.push('screenshot editor: undo stayed disabled after a drawn stroke');
          undoBtn.click();
          await sleep(100);
          if (!undoBtn.disabled) out.errors.push('screenshot editor: undo stayed enabled after undoing the only stroke');
        }
        {
          const shortcutsBtn = document.getElementById('snip-edit-shortcuts');
          const shortcutsModal = document.getElementById('shortcuts-modal');
          if (!shortcutsBtn || !onScreen(shortcutsBtn) || !canHit(shortcutsBtn)) {
            out.errors.push('screenshot editor: shortcuts overview button is missing, hidden, or not clickable');
          } else {
            shortcutsBtn.click();
            await sleep(50);
            if (shortcutsModal.classList.contains('hidden')) {
              out.errors.push('screenshot editor: shortcuts overview did not open');
            } else {
              const rows = shortcutsModal.querySelectorAll('.shortcuts-row');
              if (rows.length === 0) out.errors.push('screenshot editor: shortcuts overview opened with no rows');
              document.getElementById('shortcuts-modal-close').click();
              await sleep(50);
              if (!shortcutsModal.classList.contains('hidden')) {
                out.errors.push('screenshot editor: shortcuts overview did not close');
              }
            }
            // 519: the opener button can be gone by the time the overlay closes
            // (removed from the page while the overlay was still open) --
            // closeShortcutsModal must not throw trying to focus it, and focus
            // must land somewhere still in the document instead of nowhere.
            shortcutsBtn.click();
            await sleep(50);
            shortcutsBtn.remove();
            let threw = false;
            try { document.getElementById('shortcuts-modal-close').click(); } catch (e) { threw = true; }
            await sleep(50);
            if (threw) out.errors.push('519: closing the shortcuts overview threw with its opener removed from the page');
            if (!document.activeElement || !document.activeElement.isConnected) {
              out.errors.push('519: focus was left on a detached element after closing the shortcuts overview');
            }
          }
        }
        // 179: the overlay's own titlebar (move) and resize grip (resize) -- the
        // overlay covers the whole frameless main window, so without these there
        // is no way to move or resize it while it is open.
        const titlebar = document.querySelector('.snip-edit-titlebar');
        if (!titlebar || !onScreen(titlebar) || titlebar.getAttribute('data-tauri-drag-region') === null) {
          out.errors.push('screenshot editor: titlebar is missing or not a drag region');
        }
        const closeInBar = titlebar && titlebar.querySelector('#snip-edit-close');
        if (!closeInBar || closeInBar.getAttribute('data-tauri-drag-region') !== 'false') {
          out.errors.push('screenshot editor: close button must opt out of the titlebar drag region');
        }
        const grip = document.getElementById('snip-edit-resize-grip');
        if (!grip || !onScreen(grip) || !canHit(grip)) {
          out.errors.push('screenshot editor: resize grip is missing, hidden, or not clickable');
        } else {
          // A stub-only check: it proves the handler runs against the same
          // window.__TAURI__.window.ResizeDirection shape the real Tauri v2 API
          // exposes, not that the real WebView2 window actually resizes -- that
          // needs a real window and a person at the mouse, same as moving it.
          grip.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1 }));
          await sleep(30);
        }
        // 196: Save as reuses the same command the screenshot result dialog's own
        // Save as button calls -- must not close the editor (Copy/Cancel/Apply do).
        const saveAsBtn = document.getElementById('snip-edit-saveas');
        if (!saveAsBtn || !onScreen(saveAsBtn) || !canHit(saveAsBtn)) {
          out.errors.push('screenshot editor: Save as button is missing, hidden, or not clickable');
        } else {
          window.__SMOKE_CALLS__ = [];
          saveAsBtn.click();
          await sleep(150);
          if (!(window.__SMOKE_CALLS__ || []).includes('save_screenshot_as')) {
            out.errors.push('screenshot editor: Save as did not call save_screenshot_as');
          }
          if (document.getElementById('snip-edit').classList.contains('hidden')) {
            out.errors.push('screenshot editor: Save as closed the editor');
          }
        }
        // 181: Tab/Shift+Tab must wrap inside the overlay instead of running the
        // browser default, which would reach the grid/toolbar hidden underneath
        // (aria-modal is only a hint for assistive tech, not an actual trap).
        {
          const overlay = document.getElementById('snip-edit');
          const focusables = [...overlay.querySelectorAll('button, input, [tabindex]:not([tabindex="-1"]), select, textarea, a[href]')]
            .filter((el) => !el.disabled && el.offsetParent !== null);
          if (focusables.length > 1) {
            const first = focusables[0], last = focusables[focusables.length - 1];
            last.focus();
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));
            if (document.activeElement !== first) {
              out.errors.push('screenshot editor: Tab from the last control did not wrap to the first');
            }
            first.focus();
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true }));
            if (document.activeElement !== last) {
              out.errors.push('screenshot editor: Shift+Tab from the first control did not wrap to the last');
            }
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
        if (!document.getElementById('snip-edit').classList.contains('hidden')) {
          out.errors.push('screenshot editor did not close on Cancel');
        }
      }
      // 363: an image that fails to decode (truncated data, a broken picked
      // file) must not leave the editor silently never opening -- it shows an
      // error toast and leaves things as if the capture had never happened.
      {
        const BROKEN_PNG = 'data:image/png;base64,bm90LWEtcmVhbC1pbWFnZQ==';
        document.getElementById('toast').classList.add('hidden');
        window.__FIRE__('snip-captured', { data_url: BROKEN_PNG, path: '', name: 'broken.png' });
        await sleep(300);
        if (!document.getElementById('snip-edit').classList.contains('hidden')) {
          out.errors.push('363: a broken screenshot left the editor open instead of showing an error');
        }
        const brokenToast = document.getElementById('toast');
        if (brokenToast.classList.contains('hidden') || !brokenToast.textContent) {
          out.errors.push('363: a broken screenshot did not show an error toast');
        }
      }
      // 363: the marker tool's offscreen canvas must be reused across redraws
      // instead of reallocated every time -- cost used to scale with stroke
      // count times image resolution, since every redraw (every new op, every
      // undo/redo, every nudge, every tool switch) reallocated a full-size
      // canvas per marker stroke it painted.
      {
        const TINY_PNG2 = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
        window.__FIRE__('snip-captured', { data_url: TINY_PNG2, path: '', name: 'marker.png' });
        await sleep(300);
        const markerBtn = document.querySelector('#snip-edit-tools button[data-tool="marker"]');
        const penBtn2 = document.querySelector('#snip-edit-tools button[data-tool="pen"]');
        const live2 = document.getElementById('snip-edit-live');
        if (markerBtn && penBtn2 && live2) {
          markerBtn.click();
          await sleep(30);
          for (let i = 0; i < 3; i++) {
            const x = 10 + i * 5, y = 10 + i * 5;
            live2.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: x, clientY: y }));
            live2.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: x + 5, clientY: y + 5 }));
            window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: x + 5, clientY: y + 5 }));
            await sleep(30);
          }
          // The marker path's offscreen scratch canvas is never attached to the
          // document (it exists only to be drawn from, then discarded or
          // reused), so counting document.getElementsByTagName('canvas') can
          // never see it either way -- count document.createElement('canvas')
          // calls instead, which is the actual allocation this fix removes.
          let canvasCreations = 0;
          const origCreateElement = document.createElement.bind(document);
          document.createElement = (tag, ...rest) => {
            if (String(tag).toLowerCase() === 'canvas') canvasCreations++;
            return origCreateElement(tag, ...rest);
          };
          try {
            const baseline = canvasCreations;
            for (let i = 0; i < 10; i++) {
              (i % 2 === 0 ? penBtn2 : markerBtn).click();
              await sleep(15);
            }
            if (canvasCreations > baseline) {
              out.errors.push('363: document.createElement("canvas") ran ' + (canvasCreations - baseline) +
                ' more time(s) across 10 redraws of 3 marker strokes -- the scratch canvas is being reallocated');
            }
          } finally {
            document.createElement = origCreateElement;
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // The shared mouse-only number field, exercised on the text tool's
      // text-size control (min 10, max 96, step 1, presets 12/16/24/32/48/64/96).
      {
        const TINY_PNG3 = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
        window.__FIRE__('snip-captured', { data_url: TINY_PNG3, path: '', name: 'numfield.png' });
        await sleep(300);
        const textBtn = document.querySelector('#snip-edit-tools button[data-tool="text"]');
        if (!textBtn) {
          out.errors.push('the text tool button was not found while testing the text-size number field');
        } else {
          textBtn.click();
          await sleep(30);
          const numfield = document.querySelector('#snip-edit-context .numfield');
          if (!numfield) {
            out.errors.push('the text-size number field was not found after selecting the text tool');
          } else {
            const input = numfield.querySelector('input');
            const inc = numfield.querySelector('.numfield-inc');
            const dec = numfield.querySelector('.numfield-dec');
            const list = numfield.querySelector('.numfield-list');
            const beforeClick = Number(input.value);
            inc.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 31 }));
            await sleep(10);
            inc.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 31 }));
            await sleep(10);
            const afterClick = Number(input.value);
            if (afterClick !== beforeClick + 1) {
              out.errors.push('clicking + did not step the text-size field by exactly 1');
            }
            inc.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 32 }));
            await sleep(480);
            inc.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 32 }));
            await sleep(10);
            const afterHold = Number(input.value);
            if (!(afterHold > afterClick + 1)) {
              out.errors.push('holding + did not repeat-step the text-size field past a single click');
            }
            input.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, deltaY: -100 }));
            await sleep(30);
            if (Number(input.value) !== afterHold + 1) {
              out.errors.push('scrolling up on the text-size field did not step it by exactly 1');
            }
            list.click();
            await sleep(30);
            const pop = document.querySelector('.numfield-pop');
            const target = pop && [...pop.querySelectorAll('button')].find((b) => b.textContent === '32');
            if (!target) {
              out.errors.push('the text-size presets popover did not offer 32');
            } else {
              target.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 33 }));
              await sleep(30);
              if (Number(input.value) !== 32) {
                out.errors.push('picking 32 from the text-size presets popover did not set the field to 32');
              }
            }
            input.value = input.min;
            input.dispatchEvent(new Event('change', { bubbles: true }));
            await sleep(10);
            if (!dec.disabled) {
              out.errors.push('the - button is not disabled once the text-size field is at its minimum');
            }
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // 394: the text tool's background is now three choices (none, plate,
      // outline) instead of a plain switch. A previously stored
      // snipTextPlate: false (see the stub's initial ui_flags) must arrive as
      // "none" on its own, and picking Outline must reveal its own width field.
      {
        const dropdownRowByLabel = (text) => {
          for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
            tb.click();
            const hit = [...document.querySelectorAll('#expert-flags .expert-dropdown-row')]
              .find((r) => (r.querySelector('.value-head span') || {}).textContent === text);
            if (hit) return hit;
          }
          return null;
        };
        document.getElementById('gear').click();
        await sleep(200);
        document.getElementById('expert-open').click();
        await sleep(300);
        // The suite is left running in German from the 112 pass above onward
        // (see its own comment), so the row is found by its German label.
        const row = dropdownRowByLabel('Texthintergrund');
        const sel = row && row.querySelector('select');
        if (!sel) {
          out.errors.push('394: the text background dropdown is missing from the expert menu');
        } else {
          const values = [...sel.options].map((o) => o.value);
          if (values.join(',') !== 'none,plate,outline') {
            out.errors.push('394: the text background dropdown does not offer exactly none/plate/outline, got ' + values.join(','));
          }
          if (sel.value !== 'none') {
            out.errors.push('394: a stored snipTextPlate:false did not migrate to "none", got "' + sel.value + '"');
          }
          sel.value = 'outline';
          sel.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(120);
          const wrote = [...(window.__SMOKE_ARGS__ || [])]
            .some(([c, a]) => c === 'set_ui_text' && a.key === 'snipTextBack' && a.value === 'outline');
          if (!wrote) out.errors.push('394: picking Outline in the expert menu did not persist snipTextBack');
        }
        document.getElementById('settings-close').click();
        await sleep(200);
        const TINY_PNG4 = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
        window.__FIRE__('snip-captured', { data_url: TINY_PNG4, path: '', name: 'textback.png' });
        await sleep(300);
        const textBtn2 = document.querySelector('#snip-edit-tools button[data-tool="text"]');
        if (!textBtn2) {
          out.errors.push('394: the text tool button was not found while testing the per-shape background field');
        } else {
          textBtn2.click();
          await sleep(30);
          let shapeSel = document.querySelector('#snip-edit-context select');
          if (!shapeSel) {
            out.errors.push('394: the per-shape text background select was not found');
          } else {
            const values2 = [...shapeSel.options].map((o) => o.value);
            if (values2.join(',') !== 'none,plate,outline') {
              out.errors.push('394: the per-shape text background select does not offer exactly none/plate/outline, got ' + values2.join(','));
            }
            shapeSel.value = 'none';
            shapeSel.dispatchEvent(new Event('change', { bubbles: true }));
            await sleep(30);
            if (document.querySelectorAll('#snip-edit-context .numfield').length !== 1) {
              out.errors.push('394: an outline-width field is shown while the background is set to None');
            }
            // The panel was just rebuilt from scratch (innerHTML = ""), so the
            // old select node is detached -- re-query the live one.
            shapeSel = document.querySelector('#snip-edit-context select');
            shapeSel.value = 'outline';
            shapeSel.dispatchEvent(new Event('change', { bubbles: true }));
            await sleep(30);
            const numfields = document.querySelectorAll('#snip-edit-context .numfield input');
            if (numfields.length !== 2 || Number(numfields[1].value) !== 3) {
              out.errors.push('394: choosing Outline in the per-shape panel did not reveal a width field defaulting to 3px');
            }
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // 342: scaling the image itself (not just the view). A filled rect is
      // drawn before the scale so the test can check that the SAME drawn
      // content survives at the proportionally scaled position afterward --
      // a real 400x300 stub, not a 1x1 one, or the check would be as vacuous
      // as the scaling and zoom lesson already found once.
      {
        const whiteCanvas = document.createElement('canvas');
        whiteCanvas.width = 400;
        whiteCanvas.height = 300;
        whiteCanvas.getContext('2d').fillStyle = '#ffffff';
        whiteCanvas.getContext('2d').fillRect(0, 0, 400, 300);
        window.__FIRE__('snip-captured', { data_url: whiteCanvas.toDataURL('image/png'), path: '', name: 'scale.png' });
        await sleep(300);
        const rectBtn = document.querySelector('#snip-edit-tools button[data-tool="rect"]');
        const scaleBtn = document.getElementById('snip-edit-scale');
        if (!rectBtn || !scaleBtn) {
          out.errors.push('342: the rect tool or scale button was not found in the screenshot editor toolbar');
        } else {
          rectBtn.click();
          await sleep(30);
          const filledBox = document.querySelector('#snip-edit-context .snip-edit-toggle input');
          if (filledBox) { filledBox.checked = true; filledBox.dispatchEvent(new Event('change', { bubbles: true })); }
          await sleep(20);
          const baseCv = document.getElementById('snip-edit-base');
          const liveCv = document.getElementById('snip-edit-live');
          const rect0 = baseCv.getBoundingClientRect();
          const zoom0 = rect0.width / 400;
          const toClient = (ix, iy) => ({ x: rect0.left + ix * zoom0, y: rect0.top + iy * zoom0 });
          const p1 = toClient(50, 50), p2 = toClient(150, 150);
          liveCv.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: p1.x, clientY: p1.y }));
          liveCv.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: p2.x, clientY: p2.y }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: p2.x, clientY: p2.y }));
          await sleep(60);
          const isWhite = (data) => data[0] > 240 && data[1] > 240 && data[2] > 240;
          const beforePixel = baseCv.getContext('2d').getImageData(100, 100, 1, 1).data;
          if (isWhite(beforePixel)) {
            out.errors.push('342: the rect drawn before scaling did not leave any mark at its own (100,100) center');
          }
          scaleBtn.click();
          await sleep(30);
          const pop = document.querySelector('.snip-scale-pop');
          if (!pop) {
            out.errors.push('342: the scale button did not open the resize popover');
          } else {
            const widthInput = pop.querySelectorAll('.numfield input')[0];
            widthInput.value = '200';
            widthInput.dispatchEvent(new Event('change', { bubbles: true }));
            await sleep(20);
            const heightInput = pop.querySelectorAll('.numfield input')[1];
            if (Number(heightInput.value) !== 150) {
              out.errors.push('342: locked aspect ratio did not follow width 200 down to height 150 (got ' + heightInput.value + ')');
            }
            pop.querySelector('.snip-scale-actions .primary-btn').click();
            await sleep(60);
            if (baseCv.width !== 200 || baseCv.height !== 150) {
              out.errors.push('342: scaling to 200x150 did not change the canvas backing store (got ' + baseCv.width + 'x' + baseCv.height + ')');
            }
            const afterPixel = baseCv.getContext('2d').getImageData(50, 50, 1, 1).data;
            if (isWhite(afterPixel)) {
              out.errors.push('342: the rect drawn before scaling is gone from its proportionally scaled (50,50) position after a 2x downscale');
            }
            document.getElementById('snip-edit-undo').click();
            await sleep(60);
            if (baseCv.width !== 400 || baseCv.height !== 300) {
              out.errors.push('342: undo did not restore the exact pre-scale 400x300 size (got ' + baseCv.width + 'x' + baseCv.height + ')');
            }
            const undonePixel = baseCv.getContext('2d').getImageData(100, 100, 1, 1).data;
            if (isWhite(undonePixel)) {
              out.errors.push('342: undo restored the pre-scale size but lost the rect that was drawn on it');
            }
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // 383: the screenshot and video editors zoom on the same three keys and
      // select an existing rectangle with the same handle count -- this half
      // checks the screenshot editor, VIDEO_PROBE below checks the other.
      {
        const parityCanvas = document.createElement('canvas');
        parityCanvas.width = 400; parityCanvas.height = 300;
        parityCanvas.getContext('2d').fillStyle = '#ffffff';
        parityCanvas.getContext('2d').fillRect(0, 0, 400, 300);
        window.__FIRE__('snip-captured', { data_url: parityCanvas.toDataURL('image/png'), path: '', name: 'parity.png' });
        await sleep(300);
        const zoomVal = document.getElementById('snip-edit-zoom-val');
        const rectBtn2 = document.querySelector('#snip-edit-tools button[data-tool="rect"]');
        if (!zoomVal || !rectBtn2) {
          out.errors.push('383: screenshot editor is missing the zoom readout or the rect tool');
        } else {
          const readPct = () => Number(zoomVal.textContent.replace('%', ''));
          const fitPct = readPct();
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '+', ctrlKey: true, bubbles: true }));
          await sleep(30);
          const zoomedInPct = readPct();
          if (zoomedInPct <= fitPct) {
            out.errors.push('383: Ctrl+Plus did not zoom the screenshot editor in (was ' + fitPct + '%, now ' + zoomedInPct + '%)');
          }
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '-', ctrlKey: true, bubbles: true }));
          await sleep(30);
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '-', ctrlKey: true, bubbles: true }));
          await sleep(30);
          const zoomedOutPct = readPct();
          if (zoomedOutPct >= zoomedInPct) {
            out.errors.push('383: Ctrl+Minus did not zoom the screenshot editor out (was ' + zoomedInPct + '%, now ' + zoomedOutPct + '%)');
          }
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '0', ctrlKey: true, bubbles: true }));
          await sleep(30);
          const fitAgainPct = readPct();
          if (fitAgainPct !== fitPct) {
            out.errors.push('383: Ctrl+0 did not return the screenshot editor to its ' + fitPct + '% fit (got ' + fitAgainPct + '%)');
          }

          rectBtn2.click();
          await sleep(30);
          const baseCv2 = document.getElementById('snip-edit-base');
          const liveCv2 = document.getElementById('snip-edit-live');
          const rect2 = baseCv2.getBoundingClientRect();
          const zoom2 = rect2.width / 400;
          const toClient2 = (ix, iy) => ({ x: rect2.left + ix * zoom2, y: rect2.top + iy * zoom2 });
          const q1 = toClient2(50, 50), q2 = toClient2(150, 150);
          liveCv2.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: q1.x, clientY: q1.y }));
          liveCv2.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: q2.x, clientY: q2.y }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: q2.x, clientY: q2.y }));
          await sleep(60);
          // Selection handles are painted white on the LIVE canvas at fixed
          // image-space points around the rect's border -- nothing is there
          // until a click actually selects the shape (drawing one does not).
          const liveCtx2 = liveCv2.getContext('2d');
          const handlePoints = [[50, 50], [100, 50], [150, 50], [50, 100], [150, 100], [50, 150], [100, 150], [150, 150]];
          const countPaintedHandles = () => handlePoints.filter(([hx, hy]) => liveCtx2.getImageData(hx, hy, 1, 1).data[3] > 200).length;
          if (countPaintedHandles() !== 0) {
            out.errors.push('383: the freshly drawn rect already shows selection handles before it was clicked');
          }
          const borderPt = toClient2(50, 100); // left-edge midpoint, on the rect outline
          liveCv2.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: borderPt.x, clientY: borderPt.y }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: borderPt.x, clientY: borderPt.y }));
          await sleep(60);
          const seHandleCount = countPaintedHandles();
          if (seHandleCount !== 8) {
            out.errors.push('383: clicking the rect selected it with ' + seHandleCount + ' handles, expected 8');
          }

          // ---- 395: with the same rect still selected, arrow keys move it
          // (by numFieldStep, default 1 -- see tests/ui/tauri_stub.js),
          // Ctrl+Arrow resizes it from its top-left corner, and Escape
          // deselects instead of closing the whole editor -- there is no
          // frame-stepping to hand the arrow keys back to here, unlike the
          // video editor's own 395 check. ----
          const hint395 = document.getElementById('snip-edit-nudge-hint');
          if (!hint395 || hint395.classList.contains('hidden')) {
            out.errors.push('395: the nudge hint is not shown while a shape is selected');
          }
          // Handles are 8x8px squares (see snipEditPaintShapeHandles), too wide
          // to tell a 4px move apart from noise with a single pixel sample --
          // this instead finds the painted band's own centre within a window
          // wide enough to hold it at every position it visits here but far
          // enough from the neighbouring handles (50px apart) not to catch one
          // of those instead.
          const bandCentreX = (xMin, xMax, y) => {
            let lo = null, hi = null;
            for (let sx = xMin; sx <= xMax; sx++) {
              if (liveCtx2.getImageData(sx, y, 1, 1).data[3] > 200) { if (lo === null) lo = sx; hi = sx; }
            }
            return lo === null ? null : (lo + hi) / 2;
          };
          const bandCentreY = (x, yMin, yMax) => {
            let lo = null, hi = null;
            for (let sy = yMin; sy <= yMax; sy++) {
              if (liveCtx2.getImageData(x, sy, 1, 1).data[3] > 200) { if (lo === null) lo = sy; hi = sy; }
            }
            return lo === null ? null : (lo + hi) / 2;
          };
          const nwBefore = bandCentreX(30, 70, 50);
          const seBefore = bandCentreX(130, 170, 150);
          for (let i = 0; i < 4; i++) {
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
            await sleep(20);
          }
          const nwAfterMove = bandCentreX(30, 70, 50);
          const seAfterMove = bandCentreX(130, 170, 150);
          if (nwAfterMove !== nwBefore + 4 || seAfterMove !== seBefore + 4) {
            out.errors.push('395: four ArrowRight presses did not move the shape by exactly 4 pixels (corner at ' +
              nwBefore + ' -> ' + nwAfterMove + ')');
          }
          // Ctrl+ArrowRight grows the width from the still-fixed top-left corner
          // -- the bottom-right corner's row does not move yet, only its column.
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', ctrlKey: true, bubbles: true, cancelable: true }));
          await sleep(20);
          const nwAfterWidthGrow = bandCentreX(30, 70, 50);
          const seAfterWidthGrow = bandCentreX(130, 175, 150);
          if (nwAfterWidthGrow !== nwAfterMove || seAfterWidthGrow !== seAfterMove + 1) {
            out.errors.push('395: Ctrl+ArrowRight did not grow the shape from its top-left corner instead of moving it');
          }
          // Ctrl+ArrowDown then grows the height the same way -- its row
          // shifts down by one, its column stays exactly where the width
          // growth above just left it. Row centres (not single pixels) again,
          // since an 8px-tall handle square hides a 1px shift from a plain
          // presence check at either row.
          const seRowBeforeHeightGrow = bandCentreY(155, 140, 165);
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', ctrlKey: true, bubbles: true, cancelable: true }));
          await sleep(20);
          const seRowAfterHeightGrow = bandCentreY(155, 140, 165);
          const seColAfterHeightGrow = bandCentreX(130, 175, 151);
          if (seRowAfterHeightGrow !== seRowBeforeHeightGrow + 1 || seColAfterHeightGrow !== seAfterWidthGrow) {
            out.errors.push('395: Ctrl+ArrowDown did not grow the shape height from its fixed top-left corner');
          }
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
          await sleep(60);
          if (document.getElementById('snip-edit').classList.contains('hidden')) {
            out.errors.push('395: Escape while a shape was selected closed the whole editor instead of just deselecting it');
          }
          if (bandCentreX(30, 70, 50) !== null || bandCentreY(155, 140, 165) !== null) {
            out.errors.push('395: Escape did not remove the selection handles');
          }
          if (!hint395.classList.contains('hidden')) {
            out.errors.push('395: the nudge hint stayed visible after Escape deselected the shape');
          }

          // ---- 398: the show-original button (and the V key) compare against
          // the untouched capture -- a quick click locks it on until clicked
          // again, holding it down only peeks for as long as it stays held,
          // and a badge marks either state so it is never mistaken for the
          // edit. Drawn fresh at (200,50)-(300,150) rather than reusing the
          // rect above, which the 395 checks just moved and resized -- its
          // left edge midpoint would no longer be a known coordinate. Undo
          // staying enabled throughout proves the operation list itself was
          // never touched by any of this. ----
          const origBtn398 = document.getElementById('snip-edit-original');
          const origBadge398 = document.getElementById('snip-edit-original-badge');
          const undoBtn398 = document.getElementById('snip-edit-undo');
          if (!origBtn398 || !origBadge398) {
            out.errors.push('398: the screenshot editor is missing the show-original button or badge');
          } else {
            const q1b398 = toClient2(200, 50), q2b398 = toClient2(300, 150);
            liveCv2.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: q1b398.x, clientY: q1b398.y }));
            liveCv2.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: q2b398.x, clientY: q2b398.y }));
            window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: q2b398.x, clientY: q2b398.y }));
            await sleep(60);
            const baseCtx398 = document.getElementById('snip-edit-base').getContext('2d');
            const isWhite398 = (x, y) => { const d = baseCtx398.getImageData(x, y, 1, 1).data; return d[0] > 250 && d[1] > 250 && d[2] > 250; };
            const undoWasDisabled398 = undoBtn398.disabled;
            if (isWhite398(200, 100)) {
              out.errors.push('398: the freshly drawn rect border is not visible before the show-original check even runs');
            }
            origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 2 }));
            origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 2 }));
            await sleep(30);
            if (!isWhite398(200, 100)) out.errors.push('398: a quick click on the show-original button did not reveal the untouched capture');
            if (origBadge398.classList.contains('hidden')) out.errors.push('398: the original badge did not appear once the original was locked on');
            if (!origBtn398.classList.contains('active')) out.errors.push('398: the show-original button did not switch to its active state while locked on');
            origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 2 }));
            origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 2 }));
            await sleep(30);
            if (isWhite398(200, 100)) out.errors.push('398: a second click did not lock the original back off');
            if (!origBadge398.classList.contains('hidden')) out.errors.push('398: the original badge stayed after the second click turned it off');
            origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 2 }));
            await sleep(450);
            if (!isWhite398(200, 100)) out.errors.push('398: holding the show-original button down did not reveal the untouched capture');
            origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 2 }));
            await sleep(30);
            if (isWhite398(200, 100)) out.errors.push('398: releasing a held show-original press did not restore the edit');
            if (!origBadge398.classList.contains('hidden')) out.errors.push('398: the original badge stayed visible after releasing a held press');
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'v', bubbles: true }));
            await sleep(450);
            if (!isWhite398(200, 100)) out.errors.push('398: holding V did not reveal the untouched capture');
            document.dispatchEvent(new KeyboardEvent('keyup', { key: 'v', bubbles: true }));
            await sleep(30);
            if (isWhite398(200, 100)) out.errors.push('398: releasing V did not restore the edit');
            if (undoBtn398.disabled !== undoWasDisabled398) out.errors.push('398: the show-original round trip changed the operation list (undo availability changed)');
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // 470: dragging a selected rect (or one of its resize handles), and
      // dragging a line endpoint, used to be silent no-ops -- clicking to
      // select already worked on pointerdown, but the drag itself had no
      // matching case in pointermove or pointerup.
      {
        const canvas470 = document.createElement('canvas');
        canvas470.width = 400; canvas470.height = 300;
        canvas470.getContext('2d').fillStyle = '#ffffff';
        canvas470.getContext('2d').fillRect(0, 0, 400, 300);
        window.__FIRE__('snip-captured', { data_url: canvas470.toDataURL('image/png'), path: '', name: 'drag470.png' });
        await sleep(300);
        const rectBtn470 = document.querySelector('#snip-edit-tools button[data-tool="rect"]');
        const lineBtn470 = document.querySelector('#snip-edit-tools button[data-tool="line"]');
        const baseCv470 = document.getElementById('snip-edit-base');
        const liveCv470 = document.getElementById('snip-edit-live');
        if (!rectBtn470 || !lineBtn470 || !baseCv470 || !liveCv470) {
          out.errors.push('470: the screenshot editor is missing the rect tool, the line tool or a canvas');
        } else {
          // Reads the canvas position fresh on every call rather than caching
          // it once -- the tool options panel changes width per tool, which
          // shifts the canvas, so a cached rect drifts out of date as soon as
          // a different tool gets picked.
          const toClient470 = (ix, iy) => {
            const r = baseCv470.getBoundingClientRect();
            const zoom = r.width / 400;
            return { clientX: r.left + ix * zoom, clientY: r.top + iy * zoom };
          };
          // altKey turns off the editor snap-to-edges feature for the whole
          // drag, so every coordinate below lands exactly where sent instead
          // of drifting toward a nearby edge or centre.
          const down470 = (p) => liveCv470.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, altKey: true, ...p }));
          const move470 = (p) => liveCv470.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, altKey: true, ...p }));
          const up470 = (p) => window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, altKey: true, ...p }));
          const drag470 = async (ix1, iy1, ix2, iy2) => {
            down470(toClient470(ix1, iy1));
            move470(toClient470(ix2, iy2));
            up470(toClient470(ix2, iy2));
            await sleep(60);
          };
          const click470 = async (ix, iy) => {
            down470(toClient470(ix, iy));
            up470(toClient470(ix, iy));
            await sleep(60);
          };
          const baseCtx470 = baseCv470.getContext('2d');
          const hasInk470 = (x, y) => {
            const d = baseCtx470.getImageData(x, y, 1, 1).data;
            return !(d[0] > 250 && d[1] > 250 && d[2] > 250);
          };

          // ---- drag a selected rect by its body ----
          // Every ink check below samples 2px inward from the shape's own
          // corner or endpoint, never the exact boundary pixel -- a stroked
          // canvas edge anti-aliases its outermost pixel down to a partial
          // tint that a strict whiteness check can misread as blank.
          rectBtn470.click();
          await sleep(30);
          await drag470(40, 40, 140, 140); // draw the rect
          await click470(40, 60); // left border, away from any handle -- selects it
          if (!hasInk470(42, 42)) out.errors.push('470: the drawn rect shows no ink near its own corner before the move drag runs');
          await drag470(40, 60, 90, 110); // grab the same border point, drag it by (50,50)
          if (hasInk470(42, 42)) out.errors.push('470: dragging a selected rect left ink behind near its old corner (40,40)');
          if (!hasInk470(92, 92)) out.errors.push('470: dragging a selected rect by (50,50) did not move its corner near (90,90)');

          // ---- resize the same rect from its bottom-right handle ----
          if (!hasInk470(186, 186)) out.errors.push('470: the moved rect shows no ink near its own corner before the resize drag runs');
          await drag470(190, 190, 230, 230); // the se handle sits at the moved corner
          if (hasInk470(186, 186)) out.errors.push('470: resizing a selected rect from its se handle left ink behind near the old corner (190,190)');
          if (!hasInk470(226, 226)) out.errors.push('470: resizing a selected rect from its se handle did not grow it near (230,230)');

          // ---- drag one endpoint of a selected line ----
          lineBtn470.click();
          await sleep(30);
          await drag470(250, 50, 350, 100); // draw the line
          await click470(300, 75); // midpoint, away from either endpoint handle -- selects it
          if (!hasInk470(340, 95)) out.errors.push('470: the drawn line shows no ink near its own endpoint before the endpoint drag runs');
          await drag470(350, 100, 390, 140); // grab the far endpoint and pull it further out
          if (hasInk470(340, 95)) out.errors.push('470: dragging a line endpoint left ink behind near its old position (350,100)');
          if (!hasInk470(380, 134)) out.errors.push('470: dragging a line endpoint to (390,140) put no line near there');
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // 365: the fullscreen media viewer (lightbox) keeps the point under the
      // cursor fixed while wheel-zooming instead of always zooming to its own
      // centre. Opened through a real prompt-editor preview click; the image's
      // own top-left corner (as far from its centre as a point gets) is the
      // known, trackable coordinate.
      {
        document.querySelector('.view-btn[data-view-id="archive"]')?.click();
        await sleep(250);
        const freeCell = [...document.querySelectorAll('#grid .cell')].find((c) => !c.firstElementChild);
        if (!freeCell) {
          out.errors.push('365: no empty grid cell available to open the media viewer through');
        } else {
          freeCell.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
          await sleep(200);
          const modalImg = document.getElementById('modal-img');
          const modalImgWrap = document.getElementById('modal-img-wrap');
          if (!modalImg || !modalImgWrap) {
            out.errors.push('365: the prompt editor has no media preview element to open the viewer from');
          } else {
            // A real size, not the file's usual 1x1 fixture -- the viewer's
            // <img> has no CSS floor, so a 1x1 source would render at 1x1 CSS
            // pixel and make any zoom-anchor error too small to ever measure.
            const bigCanvas365 = document.createElement('canvas');
            bigCanvas365.width = 400; bigCanvas365.height = 300;
            const ctx365 = bigCanvas365.getContext('2d');
            ctx365.fillStyle = '#4488cc'; ctx365.fillRect(0, 0, 400, 300);
            modalImg.src = bigCanvas365.toDataURL('image/png');
            modalImgWrap.classList.remove('hidden');
            modalImg.click();
            await sleep(200);
            const lb = document.getElementById('lightbox');
            if (!lb || lb.classList.contains('hidden')) {
              out.errors.push('365: clicking the media preview did not open the fullscreen viewer');
            } else {
              const lbImg = document.getElementById('lightbox-img');
              const boxBefore = lbImg.getBoundingClientRect();
              const cx = boxBefore.left, cy = boxBefore.top;
              document.getElementById('lightbox-stage').dispatchEvent(
                new WheelEvent('wheel', { bubbles: true, cancelable: true, clientX: cx, clientY: cy, deltaY: -100 }));
              await sleep(30);
              const boxAfter = lbImg.getBoundingClientRect();
              const moved = Math.hypot(boxAfter.left - cx, boxAfter.top - cy);
              if (moved >= 2) {
                out.errors.push('365: zooming the media viewer with the wheel did not keep the point under the cursor fixed (moved ' + moved.toFixed(1) + 'px)');
              }
              document.getElementById('lightbox-close').click();
              await sleep(150);
            }
          }
          document.getElementById('modal-close-x').click();
          await sleep(150);
        }
      }
      // 181: a properly sized capture (not the 1x1 fixture above) to check the
      // text tool near the right edge -- an unstyled <input> does not shrink to
      // fit inside the stage, so placing it purely from the click point can push
      // it past the visible canvas area. Bigger than any tested viewport so
      // snipEditFit() always shrinks it to fill the wrap -- the stage's right
      // edge then lands at (or past) the wrap's own right edge at every size,
      // instead of leaving slack that would hide an overflowing input.
      const bigCanvas = document.createElement('canvas');
      bigCanvas.width = 1600; bigCanvas.height = 900;
      bigCanvas.getContext('2d').fillRect(0, 0, 1600, 900);
      window.__FIRE__('snip-captured', { data_url: bigCanvas.toDataURL('image/png'), path: '', name: 'big.png' });
      await sleep(300);
      if (!document.getElementById('snip-edit').classList.contains('hidden')) {
        const textBtn = document.querySelector('#snip-edit-tools button[data-tool="text"]');
        const wrap = document.getElementById('snip-edit-canvas-wrap');
        const liveCv = document.getElementById('snip-edit-live');
        textBtn.click();
        await sleep(30);
        const sr = liveCv.getBoundingClientRect();
        const rightEdge = { clientX: sr.left + sr.width - 3, clientY: sr.top + sr.height / 2 };
        liveCv.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, ...rightEdge }));
        await sleep(50);
        const ti = document.getElementById('snip-edit-text-input');
        if (ti.classList.contains('hidden')) {
          out.errors.push('screenshot editor: text tool did not open the input near the right edge');
        } else {
          const wr = wrap.getBoundingClientRect();
          const tr = ti.getBoundingClientRect();
          if (tr.right > wr.right + 1) {
            out.errors.push('screenshot editor: text input near the right edge overflows the canvas area by ' +
              Math.round(tr.right - wr.right) + 'px');
          }
        }
        // 472: pan room reaches every corner and edge, and the wheel zoom
        // anchors on the picture point under the cursor.
        ti.blur();
        await sleep(30);
        if (document.activeElement === ti) {
          out.errors.push('472: the text input kept focus after blur, so the editor key handler would ignore shortcuts');
        }
        {
          const zoomInBtn = document.getElementById('snip-edit-zoom-in');
          const zoomVal2 = document.getElementById('snip-edit-zoom-val');
          let lastPct = parseInt(zoomVal2.textContent, 10);
          for (let i = 0; i < 12; i++) {
            zoomInBtn.click();
            await sleep(20);
            const pct = parseInt(zoomVal2.textContent, 10);
            if (pct <= lastPct) break;
            lastPct = pct;
          }
          const stage = document.querySelector('.snip-edit-stage');
          wrap.scrollLeft = 0; wrap.scrollTop = 0;
          let stageRect = stage.getBoundingClientRect();
          let wrapRect = wrap.getBoundingClientRect();
          if (stageRect.left - wrapRect.left < wrap.clientWidth / 2 - 2) {
            out.errors.push('472: the left edge of the picture cannot be panned to the middle of the view');
          }
          if (stageRect.top - wrapRect.top < wrap.clientHeight / 2 - 2) {
            out.errors.push('472: the top edge of the picture cannot be panned to the middle of the view');
          }
          wrap.scrollLeft = wrap.scrollWidth; wrap.scrollTop = wrap.scrollHeight;
          stageRect = stage.getBoundingClientRect();
          wrapRect = wrap.getBoundingClientRect();
          if (wrapRect.left + wrap.clientWidth - stageRect.right < wrap.clientWidth / 2 - 2) {
            out.errors.push('472: the right edge of the picture cannot be panned to the middle of the view');
          }
          if (wrapRect.top + wrap.clientHeight - stageRect.bottom < wrap.clientHeight / 2 - 2) {
            out.errors.push('472: the bottom edge of the picture cannot be panned to the middle of the view');
          }
          wrap.scrollLeft = (wrap.scrollWidth - wrap.clientWidth) / 2;
          wrap.scrollTop = (wrap.scrollHeight - wrap.clientHeight) / 2;
          stageRect = stage.getBoundingClientRect();
          wrapRect = wrap.getBoundingClientRect();
          const px = wrapRect.left + wrap.clientWidth / 2 + 40;
          const py = wrapRect.top + wrap.clientHeight / 2 + 30;
          const scale = stageRect.width / 1600;
          const picX = (px - stageRect.left) / scale;
          const picY = (py - stageRect.top) / scale;
          wrap.dispatchEvent(new WheelEvent('wheel', { deltaY: 100, clientX: px, clientY: py, bubbles: true, cancelable: true }));
          await sleep(30);
          const stageRect2 = stage.getBoundingClientRect();
          const scale2 = stageRect2.width / 1600;
          const picX2 = (px - stageRect2.left) / scale2;
          const picY2 = (py - stageRect2.top) / scale2;
          if (Math.abs(picX2 - picX) > 1 || Math.abs(picY2 - picY) > 1) {
            out.errors.push('472: zooming with the wheel did not keep the picture point under the cursor');
          }
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '0', ctrlKey: true, bubbles: true }));
          await sleep(30);
          const stageRect3 = stage.getBoundingClientRect();
          const wrapRect3 = wrap.getBoundingClientRect();
          const wrapCx = wrapRect3.left + wrap.clientWidth / 2;
          const wrapCy = wrapRect3.top + wrap.clientHeight / 2;
          const stageCx = stageRect3.left + stageRect3.width / 2;
          const stageCy = stageRect3.top + stageRect3.height / 2;
          if (Math.abs(stageCx - wrapCx) > 1 || Math.abs(stageCy - wrapCy) > 1) {
            out.errors.push('472: Ctrl+0 did not fit and centre the picture in the view');
          }
        }
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await sleep(30);
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
    }
    // 181 track 10: all seven built-in themes, and the toolbar in languages
    // with markedly longer tool labels than English. Sets data-theme directly
    // rather than driving the settings theme picker (a separate custom widget,
    // already covered by its own tests) -- this is only about the editor
    // rendering sanely once a theme is active, not about picking one.
    {
      const TINY_PNG2 = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
      const priorTheme = document.documentElement.getAttribute('data-theme');
      const BUILTIN_THEMES = ['light', 'dark', 'programmer', 'neon', 'cyberpunk', 'mono', 'ocean'];
      const swatch508 = document.createElement('div');
      swatch508.style.cssText = 'position:absolute;left:-9999px;top:-9999px;background-color:var(--surface)';
      document.body.appendChild(swatch508);
      for (const theme of BUILTIN_THEMES) {
        document.documentElement.setAttribute('data-theme', theme);
        window.__FIRE__('snip-captured', { data_url: TINY_PNG2, path: '', name: 'test.png' });
        await sleep(150);
        layoutScan('screenshot editor in theme ' + theme);
        // 508: the text tool's shape-back select is the one bare select the
        // screenshot editor owns; assert it picked up the themed look too.
        const textToolBtn = document.querySelector('#snip-edit-tools button[data-tool="text"]');
        if (textToolBtn) {
          textToolBtn.click();
          const seSel = document.querySelector('#snip-edit-context select.snip-edit-select');
          if (!seSel) {
            out.errors.push('508: the screenshot editor text-back select is missing in theme ' + theme);
          } else {
            const cs = getComputedStyle(seSel);
            if (parseFloat(cs.borderTopWidth) <= 0) out.errors.push('508: snip editor select has no border in theme ' + theme);
            if (cs.backgroundColor === 'rgba(0, 0, 0, 0)' || cs.backgroundColor === 'transparent') {
              out.errors.push('508: snip editor select has a transparent background in theme ' + theme);
            }
            const surfaceRgb508 = getComputedStyle(swatch508).backgroundColor;
            // A selected option shows the accent colour (option:checked); pick
            // an unselected one to check the plain surface colour instead.
            const unselected = [...seSel.options].find((o) => !o.selected);
            if (unselected) {
              const optBg = getComputedStyle(unselected).backgroundColor;
              if (optBg !== surfaceRgb508) {
                out.errors.push('508: snip editor select option background is ' + optBg + ', expected ' + surfaceRgb508 + ' in theme ' + theme);
              }
            }
          }
          // 507: with the text tool's full field set showing, the context
          // row must wrap onto a second line rather than scroll sideways.
          const ctxEl = document.querySelector('.snip-edit-context');
          if (ctxEl && ctxEl.scrollWidth > ctxEl.clientWidth) {
            out.errors.push('507: snip-edit-context scrolls horizontally with the text tool at ' + innerWidth + 'px, theme ' + theme);
          }
        }
        const pixelateToolBtn = document.querySelector('#snip-edit-tools button[data-tool="pixelate"]');
        if (pixelateToolBtn) {
          pixelateToolBtn.click();
          const ctxEl2 = document.querySelector('.snip-edit-context');
          if (ctxEl2 && ctxEl2.scrollWidth > ctxEl2.clientWidth) {
            out.errors.push('507: snip-edit-context scrolls horizontally with the pixelate tool at ' + innerWidth + 'px, theme ' + theme);
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(100);
      }
      swatch508.remove();
      if (priorTheme) document.documentElement.setAttribute('data-theme', priorTheme);
      else document.documentElement.removeAttribute('data-theme');
      // The page is left in German after the earlier lang loop (later OCR
      // checks depend on that) -- covers EN/PL/RO/JA here, restores de after.
      const langSel2 = document.getElementById('lang-select');
      for (const lang of ['en', 'pl', 'ro', 'ja']) {
        langSel2.value = lang;
        langSel2.dispatchEvent(new Event('change'));
        await sleep(150);
        window.__FIRE__('snip-captured', { data_url: TINY_PNG2, path: '', name: 'test.png' });
        await sleep(150);
        const toolBtns2 = [...document.querySelectorAll('#snip-edit-tools button[data-tool]')];
        if (toolBtns2.length !== 12) { // 284 added the Hand (pan) tool
          out.errors.push('screenshot editor in ' + lang + ': expected 12 tool buttons, found ' + toolBtns2.length);
        }
        layoutScan('screenshot editor in ' + lang);
        document.getElementById('snip-edit-cancel').click();
        await sleep(100);
      }
      langSel2.value = 'de';
      langSel2.dispatchEvent(new Event('change'));
      await sleep(150);
    }
    // 241: the recording result dialog (#rec-modal) -- mirrors #snip-modal's own
    // probes above, plus the conversion step that only the recording flow has.
    {
      const langSel3 = document.getElementById('lang-select');
      langSel3.value = 'en';
      langSel3.dispatchEvent(new Event('change'));
      await sleep(100);
      const recModal = document.getElementById('rec-modal');
      const recPreview = document.getElementById('rec-preview');
      const recStatus = document.getElementById('rec-modal-status');
      // Drive the real recFormat control (expert menu) rather than poking
      // internal state -- found structurally (an <option value="webm">) so it
      // works whatever tab/group it currently renders in or whatever language
      // is active.
      await click('expert-open');
      const findDropdown = (optionValue) => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('select.expert-select')]
            .find((s) => [...s.options].some((o) => o.value === optionValue));
          if (hit) return hit;
        }
        return null;
      };
      const recFormatSel = findDropdown('webm');
      if (!recFormatSel) {
        out.errors.push('rec-modal: could not find the recFormat expert dropdown');
      } else {
        recFormatSel.value = 'webm';
        recFormatSel.dispatchEvent(new Event('change'));
      }
      await sleep(50);
      await click('expert-back');
      await sleep(50);

      // Conversion path (recFormat != mp4): opens straight into a "Converting"
      // status instead of a preview, updates from a progress event, then shows
      // the converted file once convert_recording resolves.
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw.mp4' });
      await sleep(30);
      if (recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: did not open on recording-captured');
      }
      if (recPreview.classList.contains('hidden') === false) {
        out.errors.push('rec-modal: preview must stay hidden while converting');
      }
      if (!/Converting/.test(recStatus.textContent)) {
        out.errors.push('rec-modal: status did not show the converting message, got ' + JSON.stringify(recStatus.textContent));
      }
      window.__FIRE__('recording-convert-progress', { ms: 3200 });
      await sleep(30);
      if (!recStatus.textContent.includes('3s')) {
        out.errors.push('rec-modal: converting status did not update from a progress event, got ' + JSON.stringify(recStatus.textContent));
      }
      await sleep(150); // let the stubbed convert_recording resolve (80ms)
      if (recPreview.classList.contains('hidden')) {
        out.errors.push('rec-modal: preview did not appear once conversion finished');
      }
      if (!recPreview.src.includes('.webm')) {
        out.errors.push('rec-modal: preview src does not point at the converted file, got ' + JSON.stringify(recPreview.src));
      }
      if (recStatus.textContent.trim() !== '') {
        out.errors.push('rec-modal: converting status must clear once done, got ' + JSON.stringify(recStatus.textContent));
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);
      if (!recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: close button did not hide the dialog');
      }

      // ffmpeg missing: the specific error convert_recording can return before
      // ever starting a process -- must name ffmpeg, not a generic failure, and
      // must still leave the raw file choosable (never dead-ends the dialog).
      window.__FIRE__('recording-captured', { path: 'C:/fake/needs-ffmpeg-MISSING.mp4' });
      await sleep(150);
      if (!/ffmpeg/i.test(recStatus.textContent)) {
        out.errors.push('rec-modal: a missing-ffmpeg failure must say so, got ' + JSON.stringify(recStatus.textContent));
      }
      if (recPreview.classList.contains('hidden')) {
        out.errors.push('rec-modal: a failed conversion must fall back to previewing the raw recording');
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);

      // 399: a real classified ffmpeg failure here too -- translated headline,
      // raw ffmpeg text only under the closed "Technical details" row, kept
      // recording still previewable.
      const recErrDetails = document.getElementById('rec-modal-error-details');
      const recErrRaw = document.getElementById('rec-modal-error-raw');
      window.__FIRE__('recording-captured', { path: 'C:/fake/needs-ffmpeg-FAILCLASSIFY.mp4' });
      await sleep(150);
      if (recStatus.textContent.includes('No space left on device') || recStatus.textContent.trim().startsWith('{')) {
        out.errors.push('rec-modal: a classified failure showed the raw/JSON text verbatim, got ' + JSON.stringify(recStatus.textContent));
      }
      if (recErrDetails.classList.contains('hidden') || recErrDetails.open) {
        out.errors.push('rec-modal: "Technical details" must appear closed, not hidden and not pre-opened');
      }
      if (!recErrRaw.textContent.includes('No space left on device')) {
        out.errors.push('rec-modal: "Technical details" did not carry the raw ffmpeg text, got ' + JSON.stringify(recErrRaw.textContent));
      }
      if (recPreview.classList.contains('hidden')) {
        out.errors.push('rec-modal: a failed conversion must fall back to previewing the raw recording');
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);

      // Reset recFormat back to mp4 -- everything below is the no-conversion path.
      if (recFormatSel) {
        await click('expert-open');
        recFormatSel.value = 'mp4';
        recFormatSel.dispatchEvent(new Event('change'));
        await sleep(30);
        await click('expert-back');
        await sleep(30);
      }

      // No-conversion path: opens straight into a playable preview.
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw2.mp4' });
      await sleep(30);
      if (recModal.classList.contains('hidden') || recPreview.classList.contains('hidden')) {
        out.errors.push('rec-modal: the no-conversion path must show the preview immediately');
      }

      // Discard removes the file and closes the dialog.
      window.__SMOKE_CALLS__ = [];
      document.getElementById('rec-modal-discard').click();
      await sleep(30);
      if (!window.__SMOKE_CALLS__.includes('discard_recording_file')) {
        out.errors.push('rec-modal: Discard did not call discard_recording_file');
      }
      if (!recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: Discard did not close the dialog');
      }

      // Save as copies out without closing the dialog (same contract as the
      // screenshot editor's own Save as).
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw3.mp4' });
      await sleep(30);
      window.__SMOKE_CALLS__ = [];
      // 674: the dialog shows Discard, More, Save, Attach; the rarer actions sit in
      // the menu behind More, which opens, closes on Escape without closing the dialog,
      // and closes itself when an item is picked.
      {
        const actionIds = [...document.querySelectorAll('#rec-modal .modal-actions button')].map((b) => b.id).join(',');
        if (actionIds !== 'rec-modal-discard,rec-modal-more,rec-modal-save,rec-modal-yes') out.errors.push('674: the recording dialog actions are ' + actionIds);
        const morePop = document.getElementById('rec-modal-more-pop');
        const moreBtn = document.getElementById('rec-modal-more');
        if (!morePop.classList.contains('hidden')) out.errors.push('674: the More menu is open before it was asked for');
        moreBtn.click();
        await sleep(60);
        if (morePop.classList.contains('hidden') || moreBtn.getAttribute('aria-expanded') !== 'true') out.errors.push('674: More did not open its menu');
        const itemIds = [...morePop.querySelectorAll('button:not(.hidden)')].map((b) => b.id).join(',');
        if (itemIds !== 'rec-modal-edit,rec-modal-saveas,rec-modal-copy,rec-modal-folder-change') out.errors.push('674: the More menu holds ' + itemIds);
        const pr = morePop.getBoundingClientRect();
        if (pr.left < 0 || pr.top < 0 || pr.right > window.innerWidth + 1 || pr.bottom > window.innerHeight + 1) out.errors.push('674: the More menu leaves the window');
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
        await sleep(60);
        if (!morePop.classList.contains('hidden')) out.errors.push('674: Escape did not close the More menu');
        if (recModal.classList.contains('hidden')) out.errors.push('674: Escape closed the whole dialog instead of only the More menu');
        moreBtn.click();
        await sleep(60);
      }
      document.getElementById('rec-modal-saveas').click();
      await sleep(30);
      if (!window.__SMOKE_CALLS__.includes('save_recording_as')) {
        out.errors.push('rec-modal: Save as did not call save_recording_as');
      }
      if (!document.getElementById('rec-modal-more-pop').classList.contains('hidden')) {
        out.errors.push('674: picking a menu item did not close the More menu');
      }
      if (recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: Save as must not close the dialog');
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);

      // Attach reuses the normal file-attach flow (openFileCreate) -- the main
      // edit modal must open with the recording as its video preview, and
      // #rec-modal itself must close first.
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw4.mp4' });
      await sleep(30);
      document.getElementById('rec-modal-yes').click();
      await sleep(50);
      if (!recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: Attach did not close the recording dialog');
      }
      const editModal = document.getElementById('modal');
      const editVideo = document.getElementById('modal-video');
      if (editModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: Attach did not open the edit modal');
      } else if (editVideo.classList.contains('hidden') || !editVideo.src) {
        out.errors.push('rec-modal: Attach did not show the recording as a video preview');
      }
      document.getElementById('modal-close-x').click();
      await sleep(30);

      // The "window size" canvas default's reactive notice is a toast: it names the
      // reported second, its button writes recCanvasMode to "all", the result menu
      // opens without waiting for it, and the expert flag recCropNotice silences it.
      const confirmModal = document.getElementById('confirm-modal');
      const cropToast = document.getElementById('toast');
      if (!confirmModal.classList.contains('hidden')) {
        // closing the edit modal above left its discard prompt open
        document.getElementById('confirm-ok').click();
        await sleep(30);
      }
      window.__SMOKE_ARGS__ = [];
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw5-cropnotice.mp4', resize_crop_at_ms: 4200 });
      await sleep(30);
      if (!confirmModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: resize-crop notice must be a toast, not a dialog');
      }
      const cropBtn = cropToast.querySelector('.toast-btn');
      if (!cropBtn) {
        out.errors.push('rec-modal: resize-crop toast did not appear for resize_crop_at_ms');
      } else {
        if (!cropToast.textContent.includes('4')) {
          out.errors.push('rec-modal: resize-crop toast did not mention the reported second');
        }
        cropBtn.click();
        await sleep(30);
        const canvasCall = window.__SMOKE_ARGS__.find(([cmd, args]) => cmd === 'set_ui_text' && args && args.key === 'recCanvasMode');
        if (!canvasCall || canvasCall[1].value !== 'all') {
          out.errors.push('rec-modal: the resize-crop toast button did not set recCanvasMode to all');
        }
      }
      if (recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: the result menu did not open next to the resize-crop toast');
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);

      // With recCropNotice off the toast stays away, the result menu still opens.
      await click('expert-open');
      let cropRow = null;
      for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
        tb.click();
        cropRow = [...document.querySelectorAll('#expert-flags .switch-field')]
          .find(r => (r.querySelector('span') || {}).textContent === 'Notice when a recording gets cut off');
        if (cropRow) break;
      }
      if (!cropRow) {
        out.errors.push('rec-modal: the recCropNotice flag row was not found in the expert menu');
      } else {
        const cropCb = cropRow.querySelector('input.switch');
        cropCb.checked = false;
        cropCb.dispatchEvent(new Event('change'));
        await sleep(150);
      }
      await click('expert-back');
      await sleep(50);
      document.getElementById('toast').textContent = '';
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw6-cropnotice-off.mp4', resize_crop_at_ms: 1000 });
      await sleep(30);
      if (cropToast.querySelector('.toast-btn')) {
        out.errors.push('rec-modal: resize-crop toast must stay away while recCropNotice is off');
      }
      if (recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: the result menu must still open once the notice is suppressed');
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);

      langSel3.value = 'de';
      langSel3.dispatchEvent(new Event('change'));
      await sleep(100);
    }
    // OCR ships on (flag "ocr"). Gate check first (both entry points visible,
    // and gone once the flag is switched off), then switch it back on via the
    // real expert-menu checkbox and exercise the menu plus the result dialog.
    {
      const ocrBtn = document.getElementById('ocr-btn');
      const snipModalOcrBtn = document.getElementById('snip-modal-ocr');
      const hiddenNow = (el) => !el || getComputedStyle(el).display === 'none';
      if (hiddenNow(ocrBtn) || hiddenNow(snipModalOcrBtn)) {
        out.errors.push('OCR entry points are hidden while the ocr flag is on');
      }
      const rowByLabel = (text) => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('#expert-flags .switch-field')]
            .find(r => (r.querySelector('span') || {}).textContent === text);
          if (hit) return hit;
        }
        return null;
      };
      const toggleOcrFlag = async (on) => {
        await click('expert-open');
        const row = rowByLabel('Text erkennen (OCR)');
        if (!row) { out.errors.push('OCR flag row not found in the expert menu'); return; }
        const cb = row.querySelector('input.switch');
        cb.checked = on;
        cb.dispatchEvent(new Event('change'));
        await sleep(150);
        await click('expert-back');
        await sleep(50);
      };
      if (!ocrBtn.title.trim() || /^[a-z]+[A-Z]/.test(ocrBtn.title)) {
        out.errors.push('OCR composer button has no real title (raw i18n key or empty)');
      }
      ocrBtn.click();
      await sleep(100);
      const menu = document.querySelector('.tab-menu:not(.hidden)');
      const menuItems = menu ? [...menu.querySelectorAll('button[role=menuitem]')] : [];
      if (menuItems.length !== 2) {
        out.errors.push('OCR menu: expected 2 entries, found ' + menuItems.length);
      } else if (menuItems[1].textContent.trim() !== 'Screenshot') {
        // The screenshot entry belongs at the bottom of the menu.
        out.errors.push('OCR menu: last entry is ' + JSON.stringify(menuItems[1].textContent.trim()) + ', expected the screenshot one');
      }
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      await sleep(50);
      const injected = '<b>bold</b> and a tag look-alike </p>';
      window.__FIRE__('ocr-captured', { text: injected, lang: 'en' });
      await sleep(150);
      const ocrModal = document.getElementById('ocr-modal');
      if (!ocrModal || ocrModal.classList.contains('hidden')) {
        out.errors.push('OCR result dialog did not open on ocr-captured');
      } else {
        const resultBox = document.getElementById('ocr-result-text');
        if (resultBox.value !== injected) {
          out.errors.push('OCR result textarea does not hold the recognized text verbatim');
        }
        if (document.querySelector('#ocr-modal b')) {
          out.errors.push('OCR result rendered markup instead of literal text');
        }
        layoutScan('OCR result dialog');
        document.getElementById('ocr-modal-close').click();
        await sleep(100);
        if (!ocrModal.classList.contains('hidden')) {
          out.errors.push('OCR result dialog did not close');
        }
      }
      // 509: an image whose recognized lines are all empty must show a clear
      // toast instead of opening the result dialog with nothing in it.
      {
        const toastEl509 = document.getElementById('toast');
        window.__FIRE__('ocr-captured', { text: '\\n\\n', lang: 'en' });
        await sleep(150);
        if (!ocrModal.classList.contains('hidden')) {
          out.errors.push('509: an all-blank OCR result opened the result dialog instead of a toast');
        }
        if (!toastEl509.classList.contains('show') || toastEl509.classList.contains('hidden')) {
          out.errors.push('509: no toast appeared for an all-blank OCR result');
        } else {
          const manageBtn = toastEl509.querySelector('.toast-btn');
          if (!manageBtn || !manageBtn.textContent.trim()) {
            out.errors.push('509: the no-text-found toast is missing its manage-languages button');
          }
          const msgEl = toastEl509.querySelector('.toast-msg');
          if (!msgEl || !msgEl.textContent.trim()) {
            out.errors.push('509: the no-text-found toast has no message text');
          }
        }
        toastEl509.classList.remove('show'); // don't linger into later blocks
        await sleep(250);
      }
      await toggleOcrFlag(false);
      if (!hiddenNow(ocrBtn) || !hiddenNow(snipModalOcrBtn)) {
        out.errors.push('OCR entry points stayed visible after switching the flag off');
      }
      // Back to the shipped state -- later probes (the tour's step 5) read the
      // live flags and would otherwise see a value this probe left behind.
      await toggleOcrFlag(true);
      if (hiddenNow(ocrBtn) || hiddenNow(snipModalOcrBtn)) {
        out.errors.push('OCR entry points stayed hidden after switching the flag back on');
      }
    }
    // OCR language-pack management dialog: list renders, size sits inside the
    // download button, buttons are named, no overflow at 520px.
    {
      const rowByLabel = (text) => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('#expert-flags .switch-field, #expert-flags .expert-action')]
            .find((r) => {
              const span = r.tagName === 'BUTTON' ? r : r.querySelector('span');
              return span && span.textContent === text;
            });
          if (hit) return hit;
        }
        return null;
      };
      const toggleOcrFlag = async (on) => {
        await click('expert-open');
        const row = rowByLabel('Text erkennen (OCR)');
        if (!row) { out.errors.push('OCR flag row not found for the langs-modal probe'); return; }
        const cb = row.querySelector('input.switch');
        cb.checked = on;
        cb.dispatchEvent(new Event('change'));
        await sleep(150);
      };
      await toggleOcrFlag(true);

      const FAKE_CAPS = [
        { name: 'Language.OCR~~~de-DE~0.0.1.0', tag: 'de-DE', label: 'German', state: 'Installed', version: '0.0.1.0', download_size: null, install_size: 45000000 },
        { name: 'Language.OCR~~~fr-FR~0.0.1.0', tag: 'fr-FR', label: 'French', state: 'NotPresent', version: '0.0.1.0', download_size: 13000000, install_size: null },
        { name: 'Language.OCR~~~ja-JP~0.0.1.0', tag: 'ja-JP', label: 'Japanese', state: 'NotPresent', version: '0.0.1.0', download_size: null, install_size: null },
      ];
      // main.js captures `invoke` once at module load (const { invoke } = ...),
      // so overriding window.__TAURI__.core.invoke this late would have no
      // effect -- tauri_stub.js's own ocr_capabilities entry reads this global
      // fresh on every call instead.
      window.__OCR_CAPS_TEST__ = FAKE_CAPS;

      // 372: ocrLangsManage and voiceLangsManage share the exact same text
      // ("Manage languages…") -- harmless while both actions sat in one shared
      // group, but rowByLabel's cross-tab search now finds Voice's copy first
      // (its tab comes before the new Capture tab). toggleOcrFlag just left the
      // OCR tab active, so search only the current tab instead of restarting
      // the tab walk from the first one.
      const langsRow = [...document.querySelectorAll('#expert-flags .expert-action')]
        .find((r) => (r.tagName === 'BUTTON' ? r : r.querySelector('span'))?.textContent === 'Sprachen verwalten…');
      if (!langsRow) {
        out.errors.push('OCR langs-modal action button not found in the expert menu');
      } else {
        langsRow.click();
        await sleep(150);
        const modal = document.getElementById('ocr-langs-modal');
        if (!modal || modal.classList.contains('hidden')) {
          out.errors.push('OCR langs modal did not open from the expert action button');
        } else {
          const rows = [...document.querySelectorAll('#ocr-langs-body .ocr-lang-row')];
          if (rows.length !== FAKE_CAPS.length) {
            out.errors.push('OCR langs modal: expected ' + FAKE_CAPS.length + ' rows, found ' + rows.length);
          }
          // 237: the language name is the most prominent thing in the row, size
          // is its own right-aligned column (MB or a dash, never a guess), state
          // reads as a chip instead of being buried in a shared meta line.
          rows.forEach((row, i) => {
            const cap = FAKE_CAPS[i];
            const nameEl = row.querySelector('.lang-row-name');
            if (!nameEl || nameEl.textContent !== cap.label) {
              out.errors.push('OCR langs modal: row ' + i + ' name is not "' + cap.label + '"');
            }
            const sizeEl = row.querySelector('.lang-row-size');
            const expectSize = (cap.state === 'Installed' ? cap.install_size : cap.download_size) == null ? '—' : 'MB';
            if (!sizeEl || !sizeEl.textContent.includes(expectSize)) {
              out.errors.push('OCR langs modal: row ' + i + ' size column does not show "' + expectSize + '"');
            }
            const chip = row.querySelector('.lang-row-chip');
            if (!chip || !chip.textContent.trim()) {
              out.errors.push('OCR langs modal: row ' + i + ' has no state chip');
            } else if (cap.state !== 'Installed') {
              // 230: a not-yet-installed row used to show the raw Rust-internal
              // state string ("NotPresent") verbatim instead of a translated one.
              // main.js is a module script (its own scope, t() unreachable from
              // here) and this suite runs under whichever UI language the test
              // machine resolves to (not necessarily English), so this cannot
              // compare against one hardcoded translated string -- it checks the
              // one language-independent property the bug actually broke: the
              // chip must not be the raw backend enum value verbatim.
              if (chip.textContent === cap.state) {
                out.errors.push('OCR langs modal: row ' + i + ' chip shows the raw backend state "' + cap.state + '" instead of a translated string');
              }
            }
          });
          const buttons = [...document.querySelectorAll('#ocr-langs-body .ghost-btn')];
          if (buttons.length !== 2) {
            out.errors.push('OCR langs modal: expected 2 download buttons, found ' + buttons.length);
          }
          for (const b of buttons) {
            if (!b.textContent.trim()) out.errors.push('OCR langs modal: a download button has no label');
          }
          if (!buttons.some((b) => b.textContent.includes('12.4 MB'))) {
            out.errors.push('OCR langs modal: download button does not show the size in its own label');
          }
          if (!buttons.some((b) => b.textContent.includes('—'))) {
            out.errors.push('OCR langs modal: a missing size did not render as a dash');
          }
          layoutScan('OCR langs modal');
          document.getElementById('ocr-langs-close-x').click();
          await sleep(100);
          if (!modal.classList.contains('hidden')) out.errors.push('OCR langs modal did not close');
        }
      }
      // 230: a second click on a language install button during the gap between
      // confirming the elevate dialog and the busy flag being set must not start
      // a second concurrent elevated install -- installPending[name] is now set
      // synchronously right after the confirm resolves, before invoke() ever
      // starts. The gate holds the elevated install() call itself pending (the
      // real one blocks for as long as the user takes on the UAC prompt, easily
      // seconds), so a genuinely separate click landing during that whole window
      // must find an already-disabled button, not just a narrow instant after
      // the confirm.
      if (langsRow) {
        langsRow.click();
        await sleep(150);
        const rows3 = [...document.querySelectorAll('#ocr-langs-body .ocr-lang-row')];
        const notInstalledRow = rows3.find((r) => !r.querySelector('.lang-row-chip.installed'));
        const installBtn = notInstalledRow && notInstalledRow.querySelector('.ghost-btn');
        const rowLabel = notInstalledRow && notInstalledRow.querySelector('.lang-row-name')?.textContent;
        if (!installBtn) {
          out.errors.push('230: no install button on a not-installed row for the double-click test');
        } else {
          window.__SMOKE_CALLS__ = [];
          window.__OCR_INSTALL_GATE__ = [];
          installBtn.click();
          await sleep(100);
          const okBtn2 = document.getElementById('confirm-ok');
          if (!okBtn2 || okBtn2.offsetParent === null) {
            out.errors.push('230: the elevate confirm dialog did not open on clicking install');
          } else {
            okBtn2.click();
            await sleep(50);
            if ((window.__OCR_INSTALL_GATE__ || []).length !== 1) {
              out.errors.push('230: expected exactly 1 gated ocr_install_language call, got ' + (window.__OCR_INSTALL_GATE__ || []).length);
            }
            const rebuiltRow = [...document.querySelectorAll('#ocr-langs-body .ocr-lang-row')]
              .find((r) => r.querySelector('.lang-row-name')?.textContent === rowLabel);
            const rebuiltBtn = rebuiltRow && rebuiltRow.querySelector('.ghost-btn');
            if (!rebuiltBtn || !rebuiltBtn.disabled) {
              out.errors.push('230: the install button did not disable itself while the elevated install call is still pending');
            }
            // A no-op on a disabled button -- proves the guard rather than just
            // documenting it, matching a real second click while Windows still
            // has not answered the first elevated install.
            rebuiltBtn && rebuiltBtn.click();
            await sleep(50);
            // Let the gated install call finally resolve so the dialog does not
            // leave a stuck poller behind for later probes.
            for (const g of window.__OCR_INSTALL_GATE__ || []) g.resolve(null);
            await sleep(50);
            const installCalls = (window.__SMOKE_CALLS__ || []).filter((c) => c === 'ocr_install_language');
            if (installCalls.length !== 1) {
              out.errors.push('230: expected exactly 1 ocr_install_language call after a rapid second click, got ' + installCalls.length);
            }
          }
        }
        window.__OCR_INSTALL_GATE__ = null;
        // The install above counts as still running across the close, and the
        // reopened dialog polls its result again: keep that result pending so the
        // elevated-listing check below is not interrupted by a finished install's
        // reload.
        RESULTS.ocr_install_result = () => null;
        document.getElementById('ocr-langs-close-x').click();
        await sleep(100);
      }
      // Windows refusing the read-only listing: an honest note plus a button that
      // re-runs the very same query elevated -- without it no language can be
      // added at all, because the list IS the install allowlist.
      window.__OCR_CAPS_ELEV__ = true;
      window.__OCR_CAPS_ELEV_DONE__ = false;
      if (langsRow) {
        langsRow.click();
        await sleep(150);
        const modal = document.getElementById('ocr-langs-modal');
        const elevBtn = [...document.querySelectorAll('#ocr-langs-body .ghost-btn')][0];
        if (!elevBtn || !elevBtn.textContent.trim()) {
          out.errors.push('OCR langs modal: no elevation button when Windows refuses the listing');
        } else {
          // 177: the raw backend reason rides along, shown collapsed.
          const details = document.querySelector('#ocr-langs-body .lang-error-details-text');
          if (!details || !details.textContent.includes('DISM')) {
            out.errors.push('OCR langs modal: the raw elevation-error text is missing from the details disclosure');
          }
          layoutScan('OCR langs modal (needs elevation)');
          elevBtn.click();
          await sleep(100);
          const okBtn = document.getElementById('confirm-ok');
          if (!okBtn || okBtn.offsetParent === null) {
            out.errors.push('OCR langs modal: elevation button did not explain itself first');
          } else {
            okBtn.click();
            await sleep(3000); // the poller re-asks every 2 s
            const rows2 = [...document.querySelectorAll('#ocr-langs-body .ocr-lang-row')];
            if (rows2.length !== FAKE_CAPS.length) {
              out.errors.push('OCR langs modal: elevated listing produced ' + rows2.length + ' rows, expected ' + FAKE_CAPS.length);
            }
          }
        }
        document.getElementById('ocr-langs-close-x').click();
        await sleep(100);
        if (modal && !modal.classList.contains('hidden')) out.errors.push('OCR langs modal did not close after the elevated listing');
      }
      window.__OCR_CAPS_ELEV__ = false;
      window.__OCR_CAPS_TEST__ = null;
      await toggleOcrFlag(true); // leave the flag in its shipped state
    }
    document.getElementById('settings').classList.add('hidden');
    await sleep(150);
    // 127: voice dictation -- button visible by default (ships on), a click
    // arms it, the first event resolves the arming state, a voice-text event
    // lands in the composer input, a second click plus the backend's own
    // voice-stopped in reply ends the session and clears every visible state.
    {
      const voiceHiddenNow = (el) => !el || getComputedStyle(el).display === 'none';
      const voiceBtn = document.getElementById('voice-btn');
      if (!voiceBtn || voiceHiddenNow(voiceBtn)) {
        out.errors.push('Voice button not visible even though voice ships on');
      } else {
        if (!voiceBtn.title.trim() || /^[a-z]+[A-Z]/.test(voiceBtn.title)) {
          out.errors.push('Voice composer button has no real title (raw i18n key or empty)');
        }
        const voiceInput = document.getElementById('input');
        voiceInput.value = '';
        voiceBtn.click();
        await sleep(50);
        if (voiceBtn.getAttribute('aria-pressed') !== 'false') {
          out.errors.push('Voice button aria-pressed should stay false while only arming');
        }
        // 170: arming lights the button up, it must not dim while waiting.
        if (!voiceBtn.classList.contains('is-arming') || parseFloat(getComputedStyle(voiceBtn).opacity) < 1) {
          out.errors.push('Voice button dims while arming instead of lighting up (opacity ' + getComputedStyle(voiceBtn).opacity + ')');
        }
        window.__FIRE__('voice-text', { id: 1, token: 1, text: 'hello world' });
        await sleep(50);
        if (!voiceBtn.classList.contains('is-listening') || voiceBtn.getAttribute('aria-pressed') !== 'true') {
          out.errors.push('Voice button did not switch to is-listening after the first event');
        }
        if (voiceInput.value.trim() !== 'hello world') {
          out.errors.push('Voice text did not land in the composer input (' + JSON.stringify(voiceInput.value) + ')');
        }
        voiceBtn.click();
        window.__FIRE__('voice-stopped', { id: 1, token: 1, reason: 'manual' });
        await sleep(50);
        if (voiceBtn.classList.contains('is-listening') || voiceBtn.classList.contains('is-arming')) {
          out.errors.push('Voice button state did not clear after voice-stopped');
        }
        if (voiceBtn.getAttribute('aria-pressed') !== 'false') {
          out.errors.push('Voice button aria-pressed did not reset to false after stopping');
        }
        // 171: a second dictation round, with nothing else touching the field
        // in between, must continue from where the first one left off rather
        // than jumping back to the front -- in the real app this landed at the
        // start roughly every third time.
        voiceBtn.click();
        await sleep(50);
        const selStartBeforeSecond = voiceInput.selectionStart;
        window.__FIRE__('voice-text', { id: 1, token: 2, text: 'second round' });
        await sleep(50);
        voiceBtn.click();
        window.__FIRE__('voice-stopped', { id: 1, token: 2, reason: 'manual' });
        await sleep(50);
        if (!voiceInput.value.trim().endsWith('second round')) {
          out.errors.push('Second dictation landed at the front instead of the caret (selectionStart read before it was ' + selStartBeforeSecond + ', value is now ' + JSON.stringify(voiceInput.value) + ')');
        }
        layoutScan('Composer with voice button');
      }
    }
    // 171: a late voice-text can still arrive after its own session already
    // reported voice-stopped (the WinRT ResultGenerated/Completed handlers are
    // not ordered against each other) -- it must be delivered to that session's
    // own target instead of being dropped or, worse, picked up by whatever
    // session starts next. Uses the edit dialog's field as target 2 so the two
    // sessions have genuinely different targets, matching the real "clicked a
    // different field and started dictating again" scenario.
    {
      const voiceInput = document.getElementById('input');
      const voiceBtn = document.getElementById('voice-btn');
      voiceInput.value = '';
      voiceBtn.click(); // session token 3
      await sleep(50);
      window.__FIRE__('voice-stopped', { id: 1, token: 3, reason: 'manual' });
      await sleep(50);
      const tile = document.querySelector('#grid .tile');
      let modalVoiceBtn = null, modalText = null;
      if (tile) {
        tile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
        await sleep(200);
        const edit = document.querySelector('#ctx [data-act="edit"]');
        if (edit) { edit.click(); await sleep(400); }
      }
      if (!tile || document.getElementById('modal').classList.contains('hidden')) {
        out.errors.push('could not open the editor for the late-voice-text probe');
      } else {
        modalVoiceBtn = document.getElementById('modal-voice-btn');
        modalText = document.getElementById('modal-text');
        // With Markdown preview on (switched on earlier in this run) a prompt
        // opens rendered by default -- swap to the raw textarea, same click the
        // user would make, before dictating into it.
        const mdPreviewEl = document.getElementById('modal-md-preview');
        if (modalText.classList.contains('hidden') && mdPreviewEl && !mdPreviewEl.classList.contains('hidden')) {
          mdPreviewEl.click();
          await sleep(100);
        }
        modalText.value = '';
        modalVoiceBtn.click(); // session token 4
        await sleep(50);
        // The late text for the FIRST (already-stopped) session arrives while the
        // second is still arming -- must land in the first target, not the second.
        window.__FIRE__('voice-text', { id: 1, token: 3, text: 'LATE' });
        await sleep(50);
        if (!voiceInput.value.trim().endsWith('LATE')) {
          out.errors.push('Late voice-text for an already-stopped session was not delivered to its own target');
        }
        if (modalText.value.includes('LATE')) {
          out.errors.push('Late voice-text for a stopped session leaked into the new session target');
        }
        window.__FIRE__('voice-text', { id: 2, token: 4, text: 'real' });
        await sleep(50);
        modalVoiceBtn.click();
        window.__FIRE__('voice-stopped', { id: 2, token: 4, reason: 'manual' });
        await sleep(50);
        if (!modalText.value.trim().endsWith('real')) {
          out.errors.push('Second session text did not land in its own target (' + JSON.stringify(modalText.value) + ')');
        }
        if (modalVoiceBtn.classList.contains('is-listening') || modalVoiceBtn.classList.contains('is-arming')) {
          out.errors.push('Voice button did not return to idle after the second session stopped');
        }
        // Grace window (3s) has passed -- a further late event for the long-gone
        // first session must now be dropped entirely, not appended anywhere.
        await sleep(3100);
        window.__FIRE__('voice-text', { id: 1, token: 3, text: 'TOO-LATE' });
        await sleep(50);
        if (voiceInput.value.includes('TOO-LATE') || modalText.value.includes('TOO-LATE')) {
          out.errors.push('A voice-text arriving after the late-delivery grace window was not dropped');
        }
        document.getElementById('modal-close-x').click();
        await sleep(80);
        // The editor's own content changed (the voice text above), so closing it
        // opens the shared unsaved-changes confirm dialog -- discard, same as a
        // real user closing without saving, so it does not linger open into the
        // Tour block right after this one and corrupt its dialog-stack checks.
        const closeConfirm = document.getElementById('confirm-modal');
        if (closeConfirm && !closeConfirm.classList.contains('hidden')) {
          document.getElementById('confirm-ok').click();
          await sleep(80);
        }
      }
    }
    // First-run tour -- all five steps reachable, each with a heading and at
    // least one text line, no raw i18n key, no overflow, height barely moves
    // between steps (measured once per open), step 5's four switches match
    // shipsOn() and Restore defaults restores them, Skip/Finish both close the dialog.
    {
      document.getElementById('settings').classList.remove('hidden');
      await sleep(50);
      // No always-available settings button any more -- reach the tour through
      // its one remaining real entry point, the gated expert-menu action, same
      // as an actual user would (data-action survives translation/tab order).
      const findTourBtn = () => document.querySelector('.expert-action[data-action="tour-open"]');
      await click('expert-open');
      let tourBtn = findTourBtn();
      for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
        if (tourBtn) break;
        tb.click();
        await sleep(150);
        tourBtn = findTourBtn();
      }
      if (!tourBtn) {
        out.errors.push('tour-open expert action not found in any tab');
      } else {
        tourBtn.click();
      }
      await sleep(150);
      // The height-per-step comparison below is sensitive to font-metric settling;
      // wait for webfonts to finish loading so it does not race one.
      try { await document.fonts.ready; } catch (e) {}
      await sleep(50);
      const tourEl = document.getElementById('tour-modal');
      if (!tourEl || tourEl.classList.contains('hidden')) {
        out.errors.push('Tour did not open from the expert-menu action');
      } else {
        const isRawKey = (s) => /^(flag|val|tip|exp|stat|backup|help|tour)[A-Z][A-Za-z0-9]+$/.test((s || '').trim());
        const body = document.getElementById('tour-body');
        const heights = [];
        for (let step = 0; step < 5; step++) {
          const title = document.getElementById('tour-title').textContent.trim();
          if (!title || isRawKey(title)) out.errors.push('Tour step ' + step + ': heading missing or raw key (' + title + ')');
          if (step < 4) {
            const art = body.querySelector('.tour-art svg');
            if (!art) {
              out.errors.push('Tour step ' + step + ': no drawing');
            } else {
              // 344: every step's art must actually render (not a collapsed/empty
              // SVG) and highlight the one thing the step is about, via the shared
              // var(--accent) convention every tour drawing follows.
              const box = art.getBoundingClientRect();
              if (box.width < 4 || box.height < 4) {
                out.errors.push('Tour step ' + step + ': the drawing renders at zero size (' + box.width + 'x' + box.height + ')');
              }
              if (!art.querySelector('[fill*="--accent"], [stroke*="--accent"]')) {
                out.errors.push('Tour step ' + step + ': the drawing has no var(--accent)-highlighted element');
              }
            }
            const lines = [...body.querySelectorAll('.tour-text p')].map((p) => p.textContent.trim());
            if (!lines.length || lines.some((l) => !l || isRawKey(l))) {
              out.errors.push('Tour step ' + step + ': missing text line or raw key (' + JSON.stringify(lines) + ')');
            }
            if (lines.length > 3) out.errors.push('Tour step ' + step + ': more than three text lines');
          } else {
            const rows = [...document.querySelectorAll('.tour-opt-row')];
            if (rows.length !== 5) out.errors.push('Tour step 5: expected 5 feature rows, found ' + rows.length);
            for (const row of rows) {
              const cb = row.querySelector('input.switch');
              const note = row.querySelector('.tour-opt-default').textContent.trim();
              if (isRawKey(note)) out.errors.push('Tour step 5: default-note renders a raw key (' + note + ')');
              const desc = row.querySelector('.tour-opt-desc').textContent.trim();
              if (!desc || isRawKey(desc)) out.errors.push('Tour step 5: option description missing or raw key');
              // 390: every feature row carries its own help button, with a real
              // (translated) accessible name -- the button's own click behaviour
              // (close the tour, open help at the row's article) is covered by
              // inspection rather than a second live open/close cycle in this
              // probe, same reasoning as the Skip button below.
              const helpBtn = row.querySelector('.tour-help-btn');
              if (!helpBtn) {
                out.errors.push('Tour step 5: feature row has no help button');
              } else {
                const aria = (helpBtn.getAttribute('aria-label') || '').trim();
                if (!aria || isRawKey(aria)) out.errors.push('Tour step 5: help button has no real accessible name');
              }
            }
            const boxes = rows.map((r) => r.querySelector('input.switch').checked);
            if (JSON.stringify(boxes) !== JSON.stringify([true, true, true, true, true])) {
              out.errors.push('Tour step 5: default switch states are ' + JSON.stringify(boxes) + ', expected all-true (shipsOn defaults)');
            }
            // Flip every switch, then Restore defaults should bring them all back.
            for (const row of rows) {
              const cb = row.querySelector('input.switch');
              cb.checked = !cb.checked;
              cb.dispatchEvent(new Event('change'));
            }
            await sleep(50);
            document.querySelector('.tour-restore-btn').click();
            await sleep(50);
            const restored = [...document.querySelectorAll('.tour-opt-row')].map((r) => r.querySelector('input.switch').checked);
            if (JSON.stringify(restored) !== JSON.stringify([true, true, true, true, true])) {
              out.errors.push('Tour step 5: Restore defaults left switches at ' + JSON.stringify(restored));
            }
          }
          void body.offsetHeight; // force layout before measuring, no stale reflow
          heights.push(body.getBoundingClientRect().height);
          layoutScan('Tour step ' + step);
          if (step < 4) document.getElementById('tour-next').click();
          await sleep(150);
        }
        // The measured min-height's job is to stop a SHORT step from looking tiny
        // next to a long one -- it floors every step to the same height. It is not
        // meant to force a longer step's wrapped text to occupy zero extra room; one
        // line-height of headroom absorbs that and
        // sub-pixel wrap jitter between runs, while still catching the case that
        // matters: the variable genuinely not wired up (a many-line gap).
        const spread = Math.max(...heights) - Math.min(...heights);
        if (spread > 20) out.errors.push('Tour content area height varies by ' + spread + 'px across steps (measured min-height not holding): ' + JSON.stringify(heights));
        const dots = [...document.querySelectorAll('.tour-dot')];
        if (dots.length !== 5) out.errors.push('Tour: expected 5 step dots, found ' + dots.length);
        // Back should be visible and clickable everywhere but step 0.
        if (document.getElementById('tour-back').classList.contains('hidden')) {
          out.errors.push('Tour: Back is hidden on the last step');
        }
        dots[0].click();
        await sleep(80);
        if (!document.getElementById('tour-back').classList.contains('hidden')) {
          out.errors.push('Tour: Back is visible on step 0');
        }
        // Dot navigation must support a direct jump (not just sequential Next/Back),
        // and the last step's button swaps to a distinct, translated Finish label
        // instead of reusing Next (point 185, track 4).
        dots[4].click();
        await sleep(80);
        if (document.getElementById('tour-back').classList.contains('hidden')) {
          out.errors.push('Tour: Back is hidden after jumping straight to the last step via its dot');
        }
        const finishLabel = document.getElementById('tour-next').textContent.trim();
        const backLabel = document.getElementById('tour-back').textContent.trim();
        if (!finishLabel || isRawKey(finishLabel) || finishLabel === backLabel) {
          out.errors.push('Tour: last-step button does not show a distinct Finish label (' + finishLabel + ')');
        }
        dots[0].click();
        await sleep(80);
        // Keyboard focus trap (point 185, track 3): Tab from the last focusable
        // control must wrap to the first, and Shift+Tab from the first must wrap
        // to the last -- the same trapTabWithin() helper the screenshot editor
        // uses (point 181), reused here instead of a second implementation.
        {
          const focusables = [...tourEl.querySelectorAll('button, input, [tabindex]:not([tabindex="-1"]), select, textarea, a[href]')]
            .filter((el) => !el.disabled && el.offsetParent !== null);
          if (focusables.length > 1) {
            const first = focusables[0], last = focusables[focusables.length - 1];
            last.focus();
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));
            if (document.activeElement !== first) {
              out.errors.push('Tour: Tab from the last control did not wrap to the first');
            }
            first.focus();
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true }));
            if (document.activeElement !== last) {
              out.errors.push('Tour: Shift+Tab from the first control did not wrap to the last');
            }
          }
        }
        // A click on the backdrop does nothing: no dialog, the tour stays open.
        const confirmModal = document.getElementById('confirm-modal');
        tourEl.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
        await sleep(80);
        if (!confirmModal.classList.contains('hidden')) out.errors.push('Tour: a background click opened a dialog');
        if (tourEl.classList.contains('hidden')) out.errors.push('Tour: a background click closed the tour');
        // Escape skips at once and shows a toast with Undo. This is the block's one
        // and only close -- a second open/close cycle in the same probe run has been
        // observed to stall the headless browser's virtual clock, so the Undo click
        // and the top-right Skip button are covered by inspection instead.
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await sleep(120);
        if (!confirmModal.classList.contains('hidden')) out.errors.push('Tour: Escape opened a confirm dialog');
        if (!tourEl.classList.contains('hidden')) out.errors.push('Tour: Escape did not close the tour');
        if (!window.__SMOKE_CALLS__.includes('set_ui_flag')) out.errors.push('Tour: Escape did not write tourSeen');
        const tourSkipToast = document.getElementById('toast');
        if (!/Tour skipped|Einf.hrung .bersprungen/.test(tourSkipToast.textContent) || !tourSkipToast.querySelector('.toast-btn')) {
          out.errors.push('Tour: Escape did not show the "Tour skipped" toast with an Undo button');
        }
      }
      document.getElementById('settings').classList.add('hidden');
      await sleep(50);
    }
    // Point 419: shrinking a view below its member count must show an overhang
    // row instead of losing tiles, and growing it back must restore everyone -
    // exactly the arrangement it had before, membership included.
    {
      const homeTab419 = document.querySelector('.view-btn[data-view-id="home"]');
      if (homeTab419) {
        homeTab419.click();
        await sleep(150);
      } else {
        out.errors.push('Overflow: could not find the "home" tab to make it active');
      }
      const before419 = await window.__TAURI__.core.invoke('get_state');
      const homeBefore419 = before419.settings.views.find(v => v.id === 'home');
      if (!homeBefore419) {
        out.errors.push('Overflow: the "home" view is missing from get_state');
      } else {
        const setGrid419 = async (cols, rows) => {
          const c = document.getElementById('qg-cols');
          const r = document.getElementById('qg-rows');
          c.focus(); c.value = String(cols); c.dispatchEvent(new Event('input', { bubbles: true })); c.blur();
          await sleep(120);
          r.focus(); r.value = String(rows); r.dispatchEvent(new Event('input', { bubbles: true })); r.blur();
          await sleep(120);
        };
        await setGrid419(2, 2);
        const overflowRow = document.getElementById('grid-overflow');
        if (!overflowRow || overflowRow.classList.contains('hidden')) {
          out.errors.push('Overflow: shrinking "home" to 2x2 did not show the overhang row for members that no longer fit');
        } else if (!/[0-9]/.test(document.getElementById('grid-overflow-text').textContent)) {
          out.errors.push('Overflow: the overhang row has no visible count');
        } else {
          // One slot for every notice: the overhang row alone shows no counter; with
          // the remote access sign active as well, only that one (higher priority)
          // shows, the counter reads +1 more, and a click swaps the two. Measured
          // live: the front notice and the counter share one row above the grid.
          const stackEl = document.getElementById('banner-stack');
          const moreEl = document.getElementById('banner-more');
          const shown = () => [...stackEl.children].filter(c => c !== moreEl && getComputedStyle(c).display !== 'none');
          if (shown().length !== 1 || shown()[0] !== overflowRow) out.errors.push('banner stack: the overhang row alone is not the one visible notice');
          if (getComputedStyle(moreEl).display !== 'none') out.errors.push('banner stack: a counter shows with a single notice');
          window.__FIRE__('remote-api-status', { running: true, port: 4711 });
          await sleep(200);
          const remoteEl = document.getElementById('remote-api-banner');
          if (shown().length !== 1 || shown()[0] !== remoteEl) out.errors.push('banner stack: with two notices the remote access sign is not the one shown');
          if (getComputedStyle(moreEl).display === 'none' || !moreEl.textContent.includes('1') || moreEl.textContent.includes('{n}')) out.errors.push('banner stack: the counter does not read +1 more: ' + moreEl.textContent);
          if (!moreEl.getAttribute('aria-label')) out.errors.push('banner stack: the counter has no accessible name');
          {
            const rb = shown()[0].getBoundingClientRect(), mb = moreEl.getBoundingClientRect(), gb = document.getElementById('grid').getBoundingClientRect();
            if (!(rb.height > 0 && mb.height > 0)) out.errors.push('banner stack: the front notice or the counter has no height');
            if (mb.left < rb.right - 1) out.errors.push('banner stack: the counter does not sit beside the notice (' + Math.round(mb.left) + ' vs ' + Math.round(rb.right) + ')');
            if (mb.top > rb.bottom || mb.bottom < rb.top) out.errors.push('banner stack: the counter is not on the notice row');
            if (gb.top < rb.bottom - 1) out.errors.push('banner stack: the grid starts above the notice slot');
            if (stackEl.getBoundingClientRect().height > rb.height + 12) out.errors.push('banner stack: the slot is taller than one notice: ' + stackEl.getBoundingClientRect().height + ' vs ' + rb.height);
          }
          moreEl.click();
          await sleep(150);
          if (shown().length !== 1 || shown()[0] !== overflowRow) out.errors.push('banner stack: clicking the counter did not show the next notice');
          window.__FIRE__('remote-api-status', { running: false });
          await sleep(200);
          if (shown().length !== 1 || shown()[0] !== overflowRow || getComputedStyle(moreEl).display !== 'none') out.errors.push('banner stack: after the other notice ended the counter stays or the notice is gone');
        }
        const growBtn419 = document.getElementById('grid-overflow-grow');
        if (!growBtn419) {
          out.errors.push('Overflow: the "enlarge the grid" button is missing');
        } else {
          growBtn419.click();
          await sleep(200);
          if (!document.getElementById('grid-overflow').classList.contains('hidden')) {
            out.errors.push('Overflow: "enlarge the grid" did not bring every member back');
          }
          const after419 = await window.__TAURI__.core.invoke('get_state');
          const homeAfter419 = after419.settings.views.find(v => v.id === 'home');
          if (!homeAfter419 || homeAfter419.members.length !== homeBefore419.members.length) {
            out.errors.push('Overflow: growing the grid back lost view membership');
          }
        }
      }
    }
    {
      // A video attachment's duration/resolution/size, measured once and stored
      // on the fixture prompt (p2), shows a duration badge on its library
      // thumbnail and a resolution/size line in its tooltip -- and shows neither
      // on the same entry's main grid tile, which stays exactly as before.
      await click('library-btn');
      const mmRow = document.querySelector('.lib-item[data-id="p2"]');
      if (!mmRow) {
        out.errors.push('MediaMeta: library row for the video fixture (p2) is missing');
      } else {
        const badge = mmRow.querySelector('.lib-thumb-duration');
        if (!badge || badge.textContent.trim() !== '1:15') {
          out.errors.push('MediaMeta: library duration badge missing or wrong (' + (badge && badge.textContent) + ')');
        }
        if (mmRow.dataset.tipMedia !== '1280×720 · 5.0 MB') {
          out.errors.push('MediaMeta: library tooltip is missing the resolution/size line (' + mmRow.dataset.tipMedia + ')');
        }
      }
      await click('library-close');
      const mmTile = document.querySelector('.tile[data-id="p2"]');
      if (mmTile && mmTile.querySelector('.lib-thumb-duration')) {
        out.errors.push('MediaMeta: the main grid tile must never show the duration badge');
      }
    }
    {
      // Search can look inside a picture's recognized text, but only while
      // the switch is on, and only as the very last resort. "lighthouse" is
      // in no prompt's name or body, so a hit can only have come from the
      // stubbed search_image_text (fixture id p4), and while the switch is
      // still off (the shipped default) the same query must match nothing at
      // all, exactly as it would have before this feature existed.
      // An earlier probe leaves the UI language switched to German; this one
      // reads English labels, so it switches back first.
      const langSelOcr = document.getElementById('lang-select');
      if (langSelOcr && langSelOcr.value !== 'en') {
        langSelOcr.value = 'en';
        langSelOcr.dispatchEvent(new Event('change'));
        await sleep(100);
      }
      await click('library-btn');
      const oq = document.getElementById('library-q');
      oq.value = 'lighthouse';
      oq.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const offHits = document.querySelectorAll('#library-list .lib-item').length;
      if (offHits !== 0) {
        out.errors.push('OcrIndex: "lighthouse" matched ' + offHits + ' prompts with the switch off (default), expected 0');
      }
      await click('library-close');
      const ocrRow = () => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('#expert-flags .switch-field')]
            .find(r => ((r.querySelector('span') || {}).textContent || '').includes('Index text inside attached pictures for search'));
          if (hit) return hit;
        }
        return null;
      };
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const ocrSwitch = ocrRow();
      if (!ocrSwitch) {
        out.errors.push('OcrIndex: the setting is missing from the expert menu');
      } else {
        ocrSwitch.querySelector('input.switch').click();
      }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
      await click('library-btn');
      const oq2 = document.getElementById('library-q');
      oq2.value = 'lighthouse';
      oq2.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const onRow = document.querySelector('.lib-item[data-id="p4"]');
      if (!onRow) {
        out.errors.push('OcrIndex: "lighthouse" found no row for the image-text fixture (p4) with the switch on');
      } else {
        const tag = onRow.querySelector('.lib-image-hit-tag');
        if (!tag || !tag.textContent.trim()) {
          out.errors.push('OcrIndex: the library row has no "found in image" badge');
        }
      }
      {
        // One query change with the switch on must redraw the list once, not
        // once for the text match and again when the picture lookup answers.
        const libList = document.getElementById('library-list');
        const innerDesc = Object.getOwnPropertyDescriptor(Element.prototype, 'innerHTML');
        let redraws = 0;
        Object.defineProperty(libList, 'innerHTML', {
          configurable: true,
          get() { return innerDesc.get.call(this); },
          set(v) { if (v === '') redraws++; innerDesc.set.call(this, v); },
        });
        const oq3 = document.getElementById('library-q');
        oq3.value = 'lighthouse ';
        oq3.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(400);
        delete libList.innerHTML;
        if (redraws !== 1) {
          out.errors.push('OcrIndex: one search change redrew the library ' + redraws + ' times, expected 1');
        }
      }
      await click('library-close');
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
    if (/^(flag|val|tip|exp|stat|backup|help)[A-Z][A-Za-z]+$/.test(txt)) out.rawKeys.push(txt);
  }
  out.errors.push(...(window.__SMOKE_ERRORS__ || []));
  } catch (e) {
    out.errors.push('UNCAUGHT: ' + (e && e.message) + (e && e.stack ? ' | ' + String(e.stack).split(String.fromCharCode(10)).slice(0, 3).join(' <- ') : ''));
  }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(out);
  document.body.appendChild(pre);
})();
</script>
"""


# The secondary windows are small by nature, so they get the same "nothing escapes the
# window" rule as the main one. Their own scroll containers are allowed to clip.
SIDE_SCAN = """
  const sideScan = (errs, where) => {
    for (const el of document.querySelectorAll('body *')) {
      const s = getComputedStyle(el);
      if (s.display === 'none' || s.visibility === 'hidden' || s.position === 'fixed' ||
          !el.getClientRects().length) continue;
      let clipped = false;
      for (let p = el.parentElement; p; p = p.parentElement) {
        const ps = getComputedStyle(p);
        if (ps.overflowX !== 'visible' || ps.overflowY !== 'visible') { clipped = true; break; }
      }
      if (clipped) continue;
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;
      if (r.right > innerWidth + 1 || r.left < -1) {
        errs.push(where + ': ' + (el.id || el.className || el.tagName) + ' is outside the window (x ' +
          Math.round(r.left) + '..' + Math.round(r.right) + ' of ' + innerWidth + ')');
      }
    }
  };
"""

# F25 + F22: the pill window is the only place these two are observable. Both used to
# fail for the same kind of reason -- a drag that never reports its end, and settings
# that were read once when the window opened and never again.
FLOAT_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));""" + SIDE_SCAN + """
  const calls = () => (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []);
  const args = () => (window.__SMOKE_ARGS__ = window.__SMOKE_ARGS__ || []);
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(50); return fn(); };
  const pill = document.getElementById('pill');
  const down = (opts) => pill.dispatchEvent(new MouseEvent('mousedown', Object.assign(
    { bubbles: true, cancelable: true, button: 0, buttons: 1 }, opts)));
  const move = (opts) => window.dispatchEvent(new MouseEvent('mousemove', Object.assign(
    { bubbles: true, button: 0, buttons: 1 }, opts)));
  const up = () => window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, buttons: 0 }));
  try {
    // applyPrefs ends by sizing the window, so that call is the signal that the
    // settings are in place. Deliberately NOT the localized labels: those wait on a
    // lazily imported module, and behaviour must never be gated on it.
    await until(() => calls().includes('resize_float_pill'));
    if (!calls().includes('resize_float_pill')) errs.push('the pill never applied its settings');
    sideScan(errs, 'pill');
    // A headless window reports no inner size, so give the ring a known one.
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 240 });
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 160 });
    // and a quiet headless frame fires no animation frame, so run the handler on a timer.
    window.requestAnimationFrame = (fn) => setTimeout(fn, 0);
    window.dispatchEvent(new Event('resize'));
    await sleep(200);
    { const rz = parseFloat(getComputedStyle(document.getElementById('resize')).getPropertyValue('--rz'));
      if (Math.abs(rz - 29) > 0.5) errs.push('the pill resize ring is ' + rz + 'px, expected 29px'); }
    // The OS move loop startDragging() hands the window to CONSUMES the button
    // release, so a dragged pill never sees `mouseup`. The merge test therefore has
    // to run off the first later event that reports no button held.
    // Edge resize. Grabbing the ring snaps the window to the pill box straight away,
    // and THAT call is what this checks: it must carry the exact box, with no monitor
    // clamp — clamping this path moved the window out from under the cursor on every
    // frame near a screen edge. The frames that follow are rAF-driven and a quiet
    // headless frame produces none, so the tracking itself is a manual check.
    args().length = 0;
    const cat = document.getElementById('rz-catch');
    const before = pill.getBoundingClientRect();
    const cx = Math.round(before.right - 2), cy = Math.round(before.top + before.height / 2);
    cat.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, buttons: 1,
      clientX: cx, clientY: cy, screenX: cx, screenY: cy }));
    const bounds = args().filter((c) => c[0] === 'set_float_bounds').map((c) => c[1]);
    if (!pill.classList.contains('resizing')) {
      errs.push('grabbing the resize ring did not start a resize');
    } else if (!bounds.length) {
      errs.push('grabbing the resize ring set no bounds');
    } else {
      if (bounds.some((b) => b.clamp)) errs.push('a live resize asked for the monitor clamp');
      const b = bounds[bounds.length - 1];
      if (!(b.width > 0 && b.height > 0)) errs.push('the resize sent an empty box');
    }
    up();
    await sleep(50);
    if (pill.classList.contains('resizing')) errs.push('the resize did not end on mouseup');

    // 41: pillFolders must default ON even with pillFolders ABSENT from ui_flags
    // (a fresh install) -- the stub otherwise ships it pre-set true, which cannot
    // tell an inverted default-off check apart from the correct default-on one
    // (both read true). undefined is the one value where they disagree.
    await window.__TAURI__.core.invoke('set_ui_flag', { key: 'pillFolders', enabled: undefined });
    window.__FIRE__('settings-changed');
    await sleep(50);
    calls().length = 0;
    down({ screenX: 100, screenY: 100 });
    move({ screenX: 260, screenY: 180 });
    await until(() => pill.classList.contains('dragging'));
    if (!pill.classList.contains('dragging')) errs.push('a pill press-and-move did not start a window drag');
    if (calls().includes('float_folder_drop')) errs.push('the merge test ran while the pill was still being dragged');
    move({ screenX: 262, screenY: 182, buttons: 0 }); // the release, noticed late
    await until(() => calls().includes('float_folder_drop'));
    if (!calls().includes('float_folder_drop')) {
      errs.push('a finished pill drag never asked the backend to merge');
    }
    if (pill.classList.contains('dragging')) errs.push('the pill stayed marked as dragging');
    // A pill reads the expert settings when it opens, so a feature switched on later
    // only reaches it through `settings-changed` -- without that, drag-out (and the
    // merge above) stay dead on every pill already on screen.
    down({ screenX: 400, screenY: 400, ctrlKey: true });
    if (pill.draggable) errs.push('a pill armed a drag-out although the feature is off');
    up();
    await sleep(150);
    // Flip it exactly the way the main window does, then deliver the broadcast.
    await window.__TAURI__.core.invoke('set_ui_flag', { key: 'dragOut', enabled: true });
    window.__FIRE__('settings-changed');
    await until(() => { down({ screenX: 400, screenY: 400, ctrlKey: true }); const on = pill.draggable; up(); return on; });
    down({ screenX: 400, screenY: 400, ctrlKey: true });
    if (!pill.draggable) errs.push('switching drag-out on never reached a pill that was already open');
    else {
      const ev = new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: new DataTransfer() });
      pill.dispatchEvent(ev);
      if (!ev.dataTransfer.getData('text/plain')) errs.push('a pill drag-out carried no text');
    }
    up();
    // 43: "hide for now" — a distinct action from Remove, must never touch the
    // permanent toggle_floating command.
    calls().length = 0;
    document.getElementById('menu-hide').click();
    await until(() => calls().includes('hide_float_for_now'));
    if (!calls().includes('hide_float_for_now')) errs.push('menu-hide did not call hide_float_for_now');
    const hideArgs = args().find((c) => c[0] === 'hide_float_for_now');
    if (!hideArgs || hideArgs[1].id !== 'p0') errs.push('menu-hide sent the wrong id: ' + JSON.stringify(hideArgs));
    if (calls().includes('toggle_floating')) errs.push('menu-hide also called the permanent toggle_floating');

    // 427: a synthesised "copy" sound plays through the shared beep helper --
    // exactly once per successful copy, only while the switch is on, never at
    // volume 0. A stand-in AudioContext counts real oscillator starts.
    window.__BEEP_COUNT__ = 0;
    window.AudioContext = window.webkitAudioContext = function () {
      return {
        currentTime: 0,
        createOscillator: () => ({ frequency: { value: 0 }, connect() { return this; }, start() { window.__BEEP_COUNT__++; }, stop() {} }),
        createGain: () => ({ gain: { setValueAtTime() {}, linearRampToValueAtTime() {} }, connect() { return this; } }),
        destination: {},
      };
    };
    // Auto-paste and the copy cooldown belong to a different feature -- both would
    // otherwise swallow the rapid, scripted clicks this test needs.
    await window.__TAURI__.core.invoke('set_ui_flag', { key: 'autoPaste', enabled: false });
    await window.__TAURI__.core.invoke('set_ui_value', { key: 'copyCooldownMs', value: 0 });
    await window.__TAURI__.core.invoke('set_ui_flag', { key: 'copySound', enabled: true });
    window.__FIRE__('settings-changed');
    await sleep(50);
    down({ screenX: 100, screenY: 100 }); up();
    await until(() => window.__BEEP_COUNT__ === 1);
    if (window.__BEEP_COUNT__ !== 1) errs.push('427: copySound on did not play exactly one beep on copy: ' + window.__BEEP_COUNT__);
    down({ screenX: 100, screenY: 100 }); up();
    await until(() => window.__BEEP_COUNT__ === 2);
    if (window.__BEEP_COUNT__ !== 2) errs.push('427: a second copy did not play a second beep: ' + window.__BEEP_COUNT__);
    // Volume 0 must play nothing.
    await window.__TAURI__.core.invoke('set_ui_value', { key: 'copySoundVolume', value: 0 });
    window.__FIRE__('settings-changed');
    await sleep(50);
    down({ screenX: 100, screenY: 100 }); up();
    await sleep(150);
    if (window.__BEEP_COUNT__ !== 2) errs.push('427: volume 0 still played a beep: ' + window.__BEEP_COUNT__);
    // Switching the option off must play nothing either, even at a normal volume.
    await window.__TAURI__.core.invoke('set_ui_value', { key: 'copySoundVolume', value: 30 });
    await window.__TAURI__.core.invoke('set_ui_flag', { key: 'copySound', enabled: false });
    window.__FIRE__('settings-changed');
    await sleep(50);
    down({ screenX: 100, screenY: 100 }); up();
    await sleep(150);
    if (window.__BEEP_COUNT__ !== 2) errs.push('427: copySound off still played a beep: ' + window.__BEEP_COUNT__);
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# F26: the colour picker runs inside the snip overlay, so this is the only place it
# is observable. A tiny frozen desktop of known pixels is handed to the overlay, then
# the loupe readout is checked against it in all three notations, plus the arrow-key
# nudge, the edge clamp, the click payload and Esc.
PICK_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(50); return fn(); };
  // 8x4 frozen desktop; row 0 is a known colour run, later rows rotate through it.
  const W = 8, H = 4;
  const COLORS = [[26,43,60],[255,0,0],[128,128,128],[255,128,0],[0,0,255],[0,255,0],[1,2,3],[255,255,255]];
  const rgb = new Uint8Array(W * H * 3);
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
    const c = COLORS[(x + y) % COLORS.length], i = (y * W + x) * 3;
    rgb[i] = c[0]; rgb[i + 1] = c[1]; rgb[i + 2] = c[2];
  }
  // The overlay background must decode, so paint the same pixels into a data URL.
  const cv = document.createElement('canvas');
  cv.width = W; cv.height = H;
  const ctx = cv.getContext('2d');
  const img = ctx.createImageData(W, H);
  for (let p = 0; p < W * H; p++) {
    img.data[p * 4] = rgb[p * 3]; img.data[p * 4 + 1] = rgb[p * 3 + 1];
    img.data[p * 4 + 2] = rgb[p * 3 + 2]; img.data[p * 4 + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
  const url = cv.toDataURL('image/png');

  let format = 'hex';
  const seen = [];
  const base = window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke = async (cmd, args) => {
    seen.push([cmd, args]);
    if (cmd === 'snip_should_begin') return true;
    if (cmd === 'snip_background') {
      return { src: url, is_file: false, width: W, height: H, pick: true, pick_zoom: 6, pick_format: format };
    }
    // The overlay caches ONE patch around the cursor instead of the whole desktop;
    // this mirrors the backend crop, including its refusal of a rect that runs off.
    if (cmd === 'pick_tile') {
      const { x, y, width, height } = args;
      if (x < 0 || y < 0 || x + width > W || y + height > H) return new Uint8Array(0).buffer;
      const out = new Uint8Array(width * height * 3);
      for (let r = 0; r < height; r++) for (let c = 0; c < width; c++) {
        const si = ((y + r) * W + (x + c)) * 3, di = (r * width + c) * 3;
        out[di] = rgb[si]; out[di + 1] = rgb[si + 1]; out[di + 2] = rgb[si + 2];
      }
      return out.buffer;
    }
    if (cmd === 'pick_color') return '#000000';
    return base(cmd, args);
  };
  const called = (cmd) => seen.some((c) => c[0] === cmd);
  const main = () => document.getElementById('pick-main').textContent;
  const alt = () => document.getElementById('pick-alt').textContent;
  const bg = () => document.getElementById('snip-bg').getBoundingClientRect();
  // Aim at the middle of image pixel (px, py) — the same ratio mapping the overlay
  // uses, which is what keeps it exact on a mixed-DPI desktop.
  const moveTo = (px, py) => {
    const r = bg();
    window.dispatchEvent(new MouseEvent('mousemove', { bubbles: true,
      clientX: r.left + (px + 0.5) * r.width / W, clientY: r.top + (py + 0.5) * r.height / H }));
  };
  const key = (k) => window.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true }));
  const session = async (fmt) => {
    format = fmt;
    window.__FIRE__('snip-begin');
    await until(() => document.body.classList.contains('pick'));
    await sleep(120);
  };
  const expect = async (want, what) => {
    const ok = await until(() => main() === want);
    if (!ok) errs.push(what + ': readout is "' + main() + '" instead of "' + want + '"');
  };

  try {
    await until(() => called('pick_tile'));
    if (!called('pick_tile')) errs.push('the picker never fetched a frozen patch');
    // Opening the picker must not pull the whole desktop across — that transfer is
    // what made it slow to start.
    if (called('pick_frame')) errs.push('the picker still transfers the whole frozen desktop');
    if (!document.body.classList.contains('pick')) errs.push('the overlay did not enter pick mode');
    moveTo(1, 0);
    await expect('#FF0000', 'hex');
    if (alt() !== 'rgb(255,0,0)') errs.push('the second readout line is "' + alt() + '" instead of rgb(255,0,0)');
    const loupe = document.getElementById('pick-loupe');
    if (loupe.classList.contains('hidden')) errs.push('the loupe stayed hidden');
    if (document.getElementById('pick-dot').classList.contains('hidden')) errs.push('the crosshair stayed hidden');
    const grid = document.getElementById('pick-grid');
    if (grid.width !== 15 * 6) errs.push('the loupe grid is ' + grid.width + 'px wide, not 15 cells at 6x');
    // The magnified grid must be centred on the picked pixel, not offset by a cell:
    // read the cells back and compare them with the frame they came from.
    const cell = (gx, gy) => {
      const d = grid.getContext('2d').getImageData(gx * 6 + 3, gy * 6 + 3, 1, 1).data;
      return [d[0], d[1], d[2]].join();
    };
    if (cell(7, 7) !== '255,0,0') errs.push('the centre cell of the loupe is ' + cell(7, 7) + ', not the picked pixel');
    if (cell(8, 7) !== '128,128,128') errs.push('the cell right of centre is ' + cell(8, 7) + ', not the next pixel');
    if (cell(6, 7) !== '26,43,60') errs.push('the cell left of centre is ' + cell(6, 7) + ', not the previous pixel');
    if (cell(7, 8) !== '128,128,128') errs.push('the cell below centre is ' + cell(7, 8) + ', not the pixel one row down');
    if (cell(7, 6) !== '0,0,0') errs.push('a cell above the desktop is ' + cell(7, 6) + ' instead of black');
    if (getComputedStyle(document.body).cursor !== 'none') errs.push('the OS cursor is still shown while picking');
    // The masks must be collapsed: a dimmed desktop would not match the colour read.
    if (document.getElementById('snip-mask-top').getBoundingClientRect().height > 0) {
      errs.push('the picker dimmed the desktop it is sampling');
    }
    // Arrow keys nudge exactly one pixel, and stop at the edge instead of wrapping.
    key('ArrowRight');
    await expect('#808080', 'one pixel right');
    key('ArrowLeft'); key('ArrowLeft');
    await expect('#1A2B3C', 'two pixels left');
    key('ArrowLeft');
    await expect('#1A2B3C', 'clamped at the left edge');
    key('ArrowUp');
    await expect('#1A2B3C', 'clamped at the top edge');
    // The hint has to name the controls, zoom included — the wheel is invisible
    // otherwise and the expert slider is not where anyone looks mid-pick.
    const hintTxt = document.getElementById('snip-hint').textContent;
    if (!hintTxt.includes('+/-')) errs.push('the picker hint never mentions zoom: "' + hintTxt + '"');

    // A shorter or longer value must NEVER resize the loupe — it used to jitter as
    // the sampled pixel changed. Measured at one zoom, across all three notations.
    const geo = [];
    const box = () => {
      const r = document.getElementById('pick-loupe').getBoundingClientRect();
      return Math.round(r.width) + 'x' + Math.round(r.height);
    };
    const measure = async (px, want, tag) => { moveTo(px, 0); await expect(want, tag); geo.push([tag, box()]); };
    await measure(6, '#010203', 'hex, dark pixel');
    await measure(7, '#FFFFFF', 'hex, white pixel');

    await session('rgb');
    moveTo(1, 0);
    await expect('rgb(255,0,0)', 'rgb notation');
    if (alt() !== '#FF0000') errs.push('rgb mode shows "' + alt() + '" instead of the hex line');
    await measure(6, 'rgb(1,2,3)', 'rgb, shortest value');
    await measure(7, 'rgb(255,255,255)', 'rgb, longest value');

    await session('hsl');
    moveTo(1, 0);
    await expect('hsl(0,100%,50%)', 'hsl notation');
    moveTo(2, 0);
    await expect('hsl(0,0%,50%)', 'hsl of pure grey');
    moveTo(0, 0);
    await expect('hsl(210,40%,17%)', 'hsl with a real hue');
    await measure(7, 'hsl(0,0%,100%)', 'hsl, short value');
    await measure(0, 'hsl(210,40%,17%)', 'hsl, long value');
    const sizes = [...new Set(geo.map((g) => g[1]))];
    if (sizes.length !== 1) errs.push('the loupe resizes with the value: ' + JSON.stringify(geo));

    // A click copies the pixel under the crosshair, by image coordinate.
    moveTo(3, 0);
    await expect('hsl(30,100%,50%)', 'before the click');
    window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 }));
    await until(() => called('pick_color'));
    const call = seen.filter((c) => c[0] === 'pick_color').pop();
    if (!call) errs.push('the click never copied a colour');
    else if (call[1].x !== 3 || call[1].y !== 0) {
      errs.push('the click copied pixel ' + call[1].x + ',' + call[1].y + ' instead of 3,0');
    }
    // Nothing about a pick may reach the prompts or the copy history.
    for (const forbidden of ['add_prompt', 'record_copy', 'copy_text', 'copy_prompt']) {
      if (called(forbidden)) errs.push('picking a colour called ' + forbidden);
    }

    // Zoom is reachable from the overlay itself: wheel, or +/- for anyone without one.
    // It starts at the setting and is clamped to the same 4-16x range as the slider.
    await session('hex');
    moveTo(1, 0);
    await expect('#FF0000', 'zoom session');
    const zoom = () => document.getElementById('pick-grid').width / 15;
    const atZoom = async (want, what) => {
      if (!await until(() => zoom() === want)) errs.push(what + ': ' + zoom() + 'x instead of ' + want + 'x');
    };
    await atZoom(6, 'the overlay did not start at the loupeZoom setting');
    const wheel = (dy, n) => {
      for (let i = 0; i < n; i++) {
        window.dispatchEvent(new WheelEvent('wheel', { deltaY: dy, bubbles: true, cancelable: true }));
      }
    };
    wheel(-100, 2);
    await atZoom(8, 'two wheel notches up');
    wheel(100, 3);
    await atZoom(5, 'three wheel notches down');
    wheel(100, 20);
    await atZoom(4, 'the wheel ran past the 4x floor');
    wheel(-100, 30);
    await atZoom(16, 'the wheel ran past the 16x ceiling');
    key('-'); key('-');
    await atZoom(14, 'the - key');
    key('+');
    await atZoom(15, 'the + key');
    // The zoom the user stops on is written back to the expert setting.
    const savedZoom = () => seen.filter((c) => c[0] === 'set_ui_value' && c[1] && c[1].key === 'loupeZoom').pop();
    if (!await until(() => savedZoom())) errs.push('the picker never persisted the zoom it ended on');
    else if (savedZoom()[1].value !== 15) {
      errs.push('the persisted zoom is ' + savedZoom()[1].value + ' instead of 15');
    }

    // A fresh session where the pointer never moves: a click must still take the
    // pixel under the cursor, not the seed the loupe opened on (desktop centre).
    await session('hex');
    await sleep(150); // let the seed's own patch land before the click
    window.dispatchEvent(new MouseEvent('mouseup', {
      bubbles: true, button: 0, clientX: bg().left + 3.5 * bg().width / W, clientY: bg().top + 0.5 * bg().height / H,
    }));
    await until(() => seen.filter((c) => c[0] === 'pick_color').length >= 2);
    const call2 = seen.filter((c) => c[0] === 'pick_color').pop();
    if (call2[1].x !== 3 || call2[1].y !== 0) {
      errs.push('a click with no prior pointer move copied ' + call2[1].x + ',' + call2[1].y + ' instead of the pixel under it (3,0)');
    }

    key('Escape');
    await until(() => called('snip_cancel'));
    if (!called('snip_cancel')) errs.push('Esc did not cancel the picker');
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# 501: a real drag over a highlighted window must still crop to the dragged rect,
# never fall back to capturing the whole window or the whole stitched canvas.
# Runs once at 1x and once at a simulated 150 percent display scale (the ratio
# mapping toImg uses is meant to be scale independent, so both must resolve the
# same rect). Shares snip.html with PICK_PROBE above: waits for its own #SMOKE
# element instead of creating a second one, then merges its findings into it.
REGION_PROBE = """
<script>
(() => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(50); return fn(); };
  // PICK_PROBE runs its own long walk on this same page first and only creates
  // #SMOKE at its very end; a plain re-assignment of window.__TAURI__.core.invoke
  // done only after that walk finishes would arrive too late, because snip.js
  // reads the invoke property exactly once, when its deferred module evaluates
  // (right after the document finishes parsing) -- long before either probe
  // script has finished its own async work. The last SYNCHRONOUS assignment
  // before that point wins, so this installs one stable dispatcher right now,
  // synchronously, and simply forwards to whatever was there before (PICK_PROBE
  // own wrapper) until this probe own session is actually ready to run.
  const priorInvoke = window.__TAURI__.core.invoke;
  let seen = null; // non-null only while a region session below is live
  let fullW = 0, fullH = 0;
  const cv = document.createElement("canvas");
  cv.width = 8; cv.height = 4;
  cv.getContext("2d").fillStyle = "#336699";
  cv.getContext("2d").fillRect(0, 0, 8, 4);
  const url = cv.toDataURL("image/png");
  window.__TAURI__.core.invoke = async (cmd, args) => {
    if (!seen) return priorInvoke(cmd, args);
    seen.push([cmd, args]);
    if (cmd === "snip_should_begin") return true;
    if (cmd === "snip_background") {
      return { src: url, is_file: false, width: fullW, height: fullH, pick: false, record: false };
    }
    // One maximized window covering the whole stitched canvas, topmost --
    // exactly what a real desktop hands back when a window fills the screen
    // under the drag.
    if (cmd === "snip_windows") return [{ id: 42, x: 0, y: 0, width: fullW, height: fullH }];
    if (cmd === "capture_region" || cmd === "capture_window") return null;
    return priorInvoke(cmd, args);
  };

  const bg = () => document.getElementById("snip-bg").getBoundingClientRect();
  const called = (cmd) => seen.some((c) => c[0] === cmd);

  const runSession = async (w, h, tag) => {
    fullW = w; fullH = h; seen = [];
    window.__FIRE__("snip-begin");
    await until(() => called("snip_background"));
    await sleep(200); // let snip_windows resolve too

    const r = bg();
    const sx = r.width / fullW, sy = r.height / fullH;
    const wantX = 200, wantY = 200, wantW = 800, wantH = 600;
    const cA = { x: r.left + wantX * sx, y: r.top + wantY * sy };
    const cMid = { x: r.left + 600 * sx, y: r.top + 500 * sy };
    const cB = { x: r.left + (wantX + wantW) * sx, y: r.top + (wantY + wantH) * sy };
    window.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0, clientX: cA.x, clientY: cA.y }));
    window.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: cMid.x, clientY: cMid.y }));
    window.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: cB.x, clientY: cB.y }));
    window.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, button: 0, clientX: cB.x, clientY: cB.y }));
    await until(() => called("capture_region") || called("capture_window"));
    const call = seen.filter((c) => c[0] === "capture_region" || c[0] === "capture_window").pop();
    if (!call) {
      errs.push(tag + ": no capture command was invoked after the drag over a window");
    } else if (call[0] !== "capture_region") {
      errs.push(tag + ": drag over a maximized window invoked " + call[0] + " instead of capture_region");
    } else {
      const a = call[1];
      const tol = 3;
      if (Math.abs(a.x - wantX) > tol || Math.abs(a.y - wantY) > tol ||
          Math.abs(a.width - wantW) > tol || Math.abs(a.height - wantH) > tol) {
        errs.push(tag + ": capture_region got x=" + a.x + " y=" + a.y + " width=" + a.width + " height=" + a.height +
          " instead of roughly x=" + wantX + " y=" + wantY + " width=" + wantW + " height=" + wantH);
      }
    }
  };

  const untilLong = async (fn) => { for (let i = 0; i < 500 && !fn(); i++) await sleep(50); return fn(); };
  (async () => {
    // Wait for PICK_PROBE own #SMOKE before this dispatcher starts intercepting
    // (seen stays null, so every call above still forwards to priorInvoke), so
    // its long walk keeps seeing exactly the responses it expects.
    await untilLong(() => document.getElementById("SMOKE"));
    try {
      await runSession(1280, 800, "1x scale");
      await runSession(1920, 1200, "150 percent scale");
    } catch (e) { errs.push("region probe: " + e.message); }
    seen = null; // stop intercepting, back to pure pass-through

    const prior = document.getElementById("SMOKE");
    if (prior) {
      let merged = errs;
      try { merged = JSON.parse(prior.textContent).concat(errs); } catch (e) { /* keep just this run own errs */ }
      prior.textContent = JSON.stringify(merged);
    } else {
      const pre = document.createElement("pre");
      pre.id = "SMOKE";
      pre.textContent = JSON.stringify(errs);
      document.body.appendChild(pre);
    }
  })();
})();
</script>
"""


# F25: the folder window is the only place its UI is observable: the closed icon
# (previews + count + label), the grid the window grows into, and Esc collapsing it.
FOLDER_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));""" + SIDE_SCAN + """
  const calls = () => window.__SMOKE_CALLS__ || [];
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(100); return fn(); };
  try {
    // The window loads its state and its translations asynchronously. Wait on the
    // label, which renderIcon fills in the same pass as the previews.
    const ready = await until(() => document.getElementById('fname').textContent !== '');
    if (!ready) errs.push('the folder window never rendered its state');
    const faces = document.querySelectorAll('#preview .folder-face').length;
    if (faces !== 4) errs.push('the closed folder shows ' + faces + ' previews instead of 4');
    // No count badge anywhere on a folder: the member count lives in the tooltip.
    if (document.getElementById('badge') || document.querySelector('.folder-badge')) {
      errs.push('the folder icon still carries a count badge');
    }
    // With the badge gone the tooltip is the only place the folder says what is in
    // it, so it has to name the folder AND its members.
    const iconEl = document.getElementById('icon');
    const tip = (iconEl && iconEl.title) || '';
    if (!tip.includes('Drafts') || !tip.includes('Prompt 1')) {
      errs.push('the folder tooltip does not describe its contents: ' + tip);
    }
    const name = document.getElementById('fname');
    if (name.classList.contains('hidden') || name.textContent !== 'Drafts') {
      errs.push('the folder label is missing: ' + name.textContent);
    }
    sideScan(errs, 'folder, closed');
    // Free resize from ANY edge, exactly like a pill: nothing is drawn for it, so the
    // ring is what this proves. It must change the size and must NOT open the folder
    // (the icon's own press is the open gesture), and the centre must stay free —
    // rz-hole covers it, or dragging the middle would resize instead of moving.
    const grip = document.getElementById('rz-catch');
    const hole = document.getElementById('rz-hole');
    if (!grip || !hole) {
      errs.push('the folder has no resize ring');
    } else {
      const ring = getComputedStyle(document.getElementById('resize')).getPropertyValue('--rz').trim();
      if (!ring || parseFloat(ring) < 8) errs.push('the resize ring has no usable thickness: ' + ring);
      const before = getComputedStyle(document.body).getPropertyValue('--folder-box').trim();
      grip.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, screenX: 100, screenY: 100 }));
      window.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, buttons: 1, screenX: 148, screenY: 148 }));
      await sleep(60);
      const during = getComputedStyle(document.body).getPropertyValue('--folder-box').trim();
      if (during === before) errs.push('dragging the resize grip did not change the folder size (' + before + ')');
      // A mouse fires far more often than the icon can usefully be redrawn, and a
      // redraw rebuilds every preview face. A burst of moves inside one frame has to
      // collapse into a single repaint — the icon is watched through the ring width,
      // which only renderIcon ever writes.
      let paints = 0;
      const obs = new MutationObserver((recs) => { paints += recs.length; });
      obs.observe(document.body, { attributes: true, attributeFilter: ['style'] });
      for (let i = 1; i <= 50; i++) {
        window.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, buttons: 1, screenX: 148 + i * 2, screenY: 148 + i * 2 }));
      }
      await sleep(60);
      obs.disconnect();
      if (paints > 4) errs.push('50 resize moves repainted the folder ' + paints + ' times');
      if (paints < 1) errs.push('50 resize moves never repainted the folder at all');
      window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 }));
      await sleep(60);
      // Skipping repaints is only allowed while the grab lasts: what stays on screen
      // afterwards has to be the size that was stored, not the last one drawn.
      const sent = (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'set_float_folder_scale').pop();
      const settled = getComputedStyle(document.body).getPropertyValue('--folder-box').trim();
      if (!sent) errs.push('the resize never reached the backend');
      else if (settled !== Math.round(96 * sent[1].scale) + 'px') {
        errs.push('the released folder kept a stale size: ' + settled + ' for scale ' + sent[1].scale);
      }
      if (!document.getElementById('pop').classList.contains('hidden')) {
        errs.push('resizing the folder opened it as well');
      }
    }
    // Opening: press and release on the icon, the gesture a click really is here.
    const icon = document.getElementById('icon');
    icon.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
    window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 }));
    const pop = document.getElementById('pop');
    await until(() => !pop.classList.contains('hidden'));
    if (pop.classList.contains('hidden')) errs.push('clicking the folder did not open it');
    if (!calls().includes('expand_float_folder')) errs.push('opening the folder never resized its window');
    if (!document.body.classList.contains('pop-open')) errs.push('the open folder is not marked open');
    const tiles = [...document.querySelectorAll('#grid .folder-member')];
    if (tiles.length !== 5) errs.push('the open folder shows ' + tiles.length + ' members instead of 5');
    sideScan(errs, 'folder, open');
    { const ex = (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'expand_float_folder').pop();
      const gs = getComputedStyle(document.getElementById('grid'));
      // Measured before the tile and popover spacing became fixed values: at this
      // folder scale the popover gap was 15.25px, the grid padding 25px, the tile gap 20px.
      if (!ex) errs.push('the open folder never asked for its popover gap');
      else if (Math.abs(ex[1].gap - 15.25) > 0.5) errs.push('the folder popover gap is ' + ex[1].gap + 'px, expected 15.25px');
      if (Math.abs(parseFloat(gs.paddingTop) - 25) > 0.5) errs.push('the folder grid padding is ' + gs.paddingTop + ', expected 25px');
      if (Math.abs(parseFloat(gs.rowGap) - 20) > 0.5) errs.push('the folder tile gap is ' + gs.rowGap + ', expected 20px'); }
    if (!tiles.length) {
      errs.push('the open folder rendered no members at all');
    } else {
      if (!tiles[0].title) errs.push('a member tile carries no tooltip');
      if (!tiles[0].querySelector('.member-ok')) errs.push('a member tile has no copy feedback');
      if (!tiles[0].querySelector('.member-face')) errs.push('a member tile renders no face');
      // A member click copies through the same path a pill uses.
      tiles[0].dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
      window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, clientX: 10, clientY: 10 }));
      await until(() => calls().includes('record_copy'));
      if (!calls().includes('copy_prompt')) errs.push('clicking a member did not copy it');
      if (!calls().includes('record_copy')) errs.push('a copy from a folder was not recorded');

      // 427: same beep helper, same "exactly one per copy, only while on, never
      // at volume 0" contract as a plain pill. clickMember() re-queries the tile
      // by id on every call rather than reusing `tiles[0]` -- each settings-changed
      // fire below reaches folder.js's own listener, which reloads state and (the
      // grid being open) calls renderGrid() again, detaching the old tile node; a
      // dispatchEvent on a detached node never bubbles to the delegated listener,
      // so a stale reference would silently swallow every click here. Outlast the
      // 400ms auto-paste double-click window too -- the member click just above
      // leaves lastClick[id] fresh, and clicking again inside that window pastes
      // instead of copying.
      const memberId = tiles[0].dataset.id;
      await sleep(450);
      window.__BEEP_COUNT__ = 0;
      window.AudioContext = window.webkitAudioContext = function () {
        return {
          currentTime: 0,
          createOscillator: () => ({ frequency: { value: 0 }, connect() { return this; }, start() { window.__BEEP_COUNT__++; }, stop() {} }),
          createGain: () => ({ gain: { setValueAtTime() {}, linearRampToValueAtTime() {} }, connect() { return this; } }),
          destination: {},
        };
      };
      const clickMember = () => {
        const t = document.querySelector(`#grid .folder-member[data-id="${CSS.escape(memberId)}"]`);
        if (!t) { errs.push('427: the member tile disappeared mid-test'); return; }
        t.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
        window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, clientX: 10, clientY: 10 }));
      };
      await window.__TAURI__.core.invoke('set_ui_flag', { key: 'autoPaste', enabled: false });
      await window.__TAURI__.core.invoke('set_ui_value', { key: 'copyCooldownMs', value: 0 });
      await window.__TAURI__.core.invoke('set_ui_flag', { key: 'copySound', enabled: true });
      window.__FIRE__('settings-changed');
      await sleep(50);
      clickMember();
      await until(() => window.__BEEP_COUNT__ === 1);
      if (window.__BEEP_COUNT__ !== 1) errs.push('427: copySound on did not play a beep on a folder-member copy: ' + window.__BEEP_COUNT__);
      await window.__TAURI__.core.invoke('set_ui_value', { key: 'copySoundVolume', value: 0 });
      window.__FIRE__('settings-changed');
      await sleep(50);
      clickMember();
      await sleep(150);
      if (window.__BEEP_COUNT__ !== 1) errs.push('427: volume 0 still played a beep in a folder: ' + window.__BEEP_COUNT__);
      await window.__TAURI__.core.invoke('set_ui_value', { key: 'copySoundVolume', value: 30 });
      await window.__TAURI__.core.invoke('set_ui_flag', { key: 'copySound', enabled: false });
      window.__FIRE__('settings-changed');
      await sleep(50);
      clickMember();
      await sleep(150);
      if (window.__BEEP_COUNT__ !== 1) errs.push('427: copySound off still played a beep in a folder: ' + window.__BEEP_COUNT__);
    }
    // 43: "hide for now" — a distinct action from Dissolve, must never touch the
    // permanent dissolve_float_folder command.
    window.__SMOKE_CALLS__.length = 0;
    document.getElementById('menu-hide').click();
    await until(() => calls().includes('hide_float_for_now'));
    if (!calls().includes('hide_float_for_now')) errs.push('menu-hide did not call hide_float_for_now');
    const hideArgs = (window.__SMOKE_ARGS__ || []).find((c) => c[0] === 'hide_float_for_now');
    if (!hideArgs || hideArgs[1].id !== 'f1') errs.push('menu-hide sent the wrong id: ' + JSON.stringify(hideArgs));
    if (calls().includes('dissolve_float_folder')) errs.push('menu-hide also called the permanent dissolve_float_folder');
    // Esc collapses it again and shrinks the window back to the icon.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await until(() => pop.classList.contains('hidden'));
    if (!pop.classList.contains('hidden')) errs.push('Esc did not collapse the folder');
    if (document.body.classList.contains('pop-open')) errs.push('the collapsed folder is still marked open');
    if (!calls().includes('collapse_float_folder')) errs.push('collapsing never shrank the window back');
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# The standalone fill-in window: it renders the rows the main window handed over,
# answers with what was typed, and treats Esc / a click outside as a cancel.
VARS_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    await sleep(300);
    const rows = [...document.querySelectorAll('#fields .vars-row')];
    if (rows.length !== 2) errs.push('the window rendered ' + rows.length + ' rows instead of 2');
    if (!document.getElementById('title').textContent.trim()) errs.push('the window shows no title');
    if (!document.getElementById('ok').textContent.trim()) errs.push('the confirm button has no label');
    if (!document.getElementById('cancel').textContent.trim()) errs.push('the cancel button has no label');
    const text = rows[0] && rows[0].querySelector('input[type=text]');
    if (!text) errs.push('the free-text row has no input');
    else if (text.value !== 'roadmap') errs.push('the remembered value was not filled in: ' + text.value);
    if (!rows[0] || !rows[0].querySelector('input[type=checkbox]')) {
      errs.push('the free-text row has no remember switch');
    }
    // The choice row must NOT be a native <select> — its OS popup would blur (and
    // therefore cancel) this window.
    if (rows[1] && rows[1].querySelector('select')) errs.push('a choice row uses a native select');
    const choice = rows[1] && rows[1].querySelector('.vars-choice-btn');
    const list = rows[1] && rows[1].querySelector('.vars-choice-list');
    if (!choice || !list) {
      errs.push('the choice row has no in-window dropdown');
    } else {
      if (choice.textContent !== 'casual') errs.push('the choice row did not preselect its value');
      choice.click();
      await sleep(120);
      if (list.classList.contains('hidden')) errs.push('the choice dropdown did not open');
      const items = [...list.querySelectorAll('.vars-choice-item')];
      if (items.map(i => i.textContent).join(',') !== 'formal,casual,witty') {
        errs.push('the choice dropdown lists the wrong options');
      }
      items[2] && items[2].click();
      await sleep(120);
      if (!list.classList.contains('hidden')) errs.push('picking an option left the dropdown open');
      if (choice.textContent !== 'witty') errs.push('the pick did not land in the button');
      // 58: Enter landing on a choice control (the button, or a focused item in an
      // open list) must not be hijacked into an immediate whole-window confirm --
      // it belongs to that control alone (open the list / pick the item).
      choice.click();
      await sleep(120);
      window.__VAR_RESULT__ = undefined;
      const firstItem = list.querySelector('.vars-choice-item');
      (firstItem || choice).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      await sleep(80);
      if (window.__VAR_RESULT__ !== undefined) {
        errs.push('Enter on a choice control confirmed the whole window instead of staying local to it');
      }
      if (!list.classList.contains('hidden')) choice.click(); // close it again for what follows
      await sleep(80);
    }
    if (text) text.value = 'Rust';
    // Nothing may have been sent before the user confirms.
    if (window.__VAR_RESULT__ !== undefined) errs.push('the window answered before it was confirmed');
    document.getElementById('ok').click();
    await sleep(200);
    let answer = null;
    try { answer = JSON.parse(window.__VAR_RESULT__ || 'null'); } catch (e) { answer = null; }
    if (!answer) errs.push('confirming sent no answer');
    else {
      if (answer.values.join('|') !== 'Rust|witty') errs.push('the answer carries ' + answer.values.join('|'));
      if (answer.remember[0] !== true) errs.push('the remember switch was not reported');
    }
    // One answer only: Esc after a confirm must not send a cancel on top of it.
    window.__VAR_RESULT__ = 'SENTINEL';
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    window.dispatchEvent(new Event('blur'));
    await sleep(150);
    if (window.__VAR_RESULT__ !== 'SENTINEL') errs.push('the window answered a second time');
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


LAUNCHER_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    await sleep(500);
    const search = document.getElementById('search');
    if (!search.placeholder.trim()) errs.push('the search field has no placeholder');
    const rows = () => [...document.querySelectorAll('#results .launcher-row')];
    if (!rows().length) errs.push('the empty-query default list shows nothing');
    if (rows().length > 8) errs.push('the default list is not capped at 8');
    // A query nothing matches shows the empty-state text, not a stale row.
    search.value = 'zzzznomatch';
    search.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(50);
    if (rows().length) errs.push('a query with no matches still shows rows');
    const empty = document.getElementById('empty');
    if (empty.classList.contains('hidden') || !empty.textContent.trim()) {
      errs.push('the no-matches state has no visible message');
    }
    // p1's TEXT (not its name) carries the only "Translate" in the fixture, so a
    // hit here proves the body is searched too, not just the name.
    search.value = 'Translate';
    search.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(50);
    if (rows().length !== 1) errs.push('searching "Translate" found ' + rows().length + ' rows instead of 1');
    else if (rows()[0].dataset.id !== 'p1') errs.push('the wrong prompt matched: ' + rows()[0].dataset.id);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await sleep(50);
    if (window.__LAUNCHER_SELECTED__ !== 'p1') {
      errs.push('Enter did not select the highlighted row: ' + window.__LAUNCHER_SELECTED__);
    }
    // "One action ever" — same guard vars.js uses (see its own SENTINEL check): a
    // stray Escape after a real selection must never ALSO fire launcher_close.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await sleep(50);
    if (window.__LAUNCHER_CLOSED__) errs.push('closing fired even though a selection already had');
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# The tray filter row opens the launcher with its text: the stub's launcher_items
# carries a prefill, which must land in the search field with the caret at the end.
LAUNCHER_PREFILL_PROBE = """
<script>
window.__LAUNCHER_PREFILL__ = 'Translate';
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    await sleep(500);
    const search = document.getElementById('search');
    if (search.value !== 'Translate') errs.push('the prefill did not reach the search field: ' + search.value);
    const rows = [...document.querySelectorAll('#results .launcher-row')];
    if (rows.length !== 1 || rows[0].dataset.id !== 'p1') errs.push('the prefilled query did not filter the list: ' + rows.length + ' rows');
    if (search.selectionStart !== 9 || search.selectionEnd !== 9) errs.push('the caret is not at the end of the prefill');
    // Characters the tray passes on after this window is up replace the starting query.
    window.__FIRE__('launcher-prefill', 'Translate it');
    await sleep(60);
    if (search.value !== 'Translate it') errs.push('a late prefill did not reach the search field: ' + search.value);
    if (search.selectionStart !== 12) errs.push('the caret is not at the end after a late prefill');
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# The custom Windows-11-style tray menu (point 236): rendering, keyboard nav and
# activation. The Rust-side content (favorites/recent selection, ordering, caps)
# has its own unit tests -- this only proves the window draws what it is given
# and responds to the keyboard the same way the launcher and vars windows do.
TRAY_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    await sleep(300);
    const rows = () => [...document.querySelectorAll('#card .tray-menu-row')];
    if (rows().length !== 5) errs.push('expected 5 rows (3 entries + show + quit), found ' + rows().length);
    const heads = [...document.querySelectorAll('#card .tray-menu-head')];
    if (heads.length !== 2) errs.push('expected 2 section headers, found ' + heads.length);
    const seps = [...document.querySelectorAll('#card .tray-menu-sep')];
    if (seps.length !== 2) errs.push('expected 2 separators (one per section), found ' + seps.length);
    if (!rows()[0].classList.contains('active')) errs.push('the first row is not highlighted on open');
    // Every entry row (not show/quit) carries a colored icon chip, and the one
    // entry with its own tile color must use it instead of the kind's tint.
    const icons = rows().slice(0, 3).map((r) => r.querySelector('.tray-menu-icon'));
    if (icons.some((i) => !i)) errs.push('an entry row has no icon chip');
    else if (!/255,\\s*0,\\s*170/.test(icons[1].style.background)) {
      errs.push('the entry with a custom color did not use it: ' + icons[1].style.background);
    }
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    await sleep(30);
    if (!rows()[1].classList.contains('active')) errs.push('ArrowDown did not move the highlight to row 1');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
    await sleep(30);
    if (!rows()[0].classList.contains('active')) errs.push('ArrowUp did not move the highlight back to row 0');
    // Enter on a fixed row still goes through the plain activation.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await sleep(30);
    if (window.__TRAY_ACTIVATED__ !== 'tray-copy-p1') {
      errs.push('Enter on a fixed row did not go through tray_window_activate: ' + window.__TRAY_ACTIVATED__);
    }
    // Same "one action ever" guard the launcher and vars windows use: a stray
    // Escape after a real activation must never ALSO fire tray_window_close.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await sleep(30);
    if (window.__TRAY_CLOSED__) errs.push('closing fired even though an activation already had');
    // The filter row at the very top hands the first keystroke to the quick
    // launcher: what was typed travels along, the tray list itself stays as it was.
    // Characters typed before the launcher has focus are passed on once, not lost,
    // and never start a second hand-off.
    const field = document.getElementById('tray-search');
    if (!field || field.hidden) errs.push('the search field is missing or hidden while traySearch is on');
    field.value = 'alpha';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    field.value = 'alphabet';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(80);
    if (window.__TRAY_LAUNCHER_QUERY__ !== 'alpha') {
      errs.push('typing in the tray filter did not hand the text to the launcher: ' + window.__TRAY_LAUNCHER_QUERY__);
    }
    if (window.__TRAY_LAUNCHER_CALLS__ !== 1) errs.push('typing twice started ' + window.__TRAY_LAUNCHER_CALLS__ + ' hand-offs instead of 1');
    if (window.__TRAY_LAUNCHER_PREFILL__ !== 'alphabet') {
      errs.push('the characters typed during the hand-off were not passed on: ' + window.__TRAY_LAUNCHER_PREFILL__);
    }
    if (rows().length !== 5) errs.push('typing changed the tray rows, found ' + rows().length);
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


RECBAR_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  // A small quiet headless frame stops producing rendering frames once
  // nothing keeps invalidating it, and a ResizeObserver's notifications ride
  // that same rendering step -- dirtying an unused custom property keeps
  // frames coming without touching any real geometry (same trick the main
  // window's own probe wrapper uses for its startup rAF).
  const heartbeat = setInterval(() => {
    document.documentElement.style.setProperty('--smoke-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  try {
    await sleep(300);
    const timeEl = document.getElementById('time');
    if (timeEl.textContent !== '01:05') errs.push('time did not render from recording_status: ' + timeEl.textContent);
    const sizeEl = document.getElementById('size');
    if (!/MB/.test(sizeEl.textContent)) errs.push('size did not render: ' + sizeEl.textContent);
    if (sizeEl.classList.contains('rec-size-warn')) errs.push('size was highlighted well below the ceiling');
    const until = async (fn, tries) => {
      for (let i = 0; i < (tries || 40) && !fn(); i++) await sleep(50);
      return fn();
    };
    // 505: recbar.css now sizes the bar to its own content instead of a fixed
    // 320x56 window, so stop-btn/cancel-btn must never fall outside the width
    // fit_rec_bar was actually sent -- checked once both audio icons (always
    // shown by applyAudio) and the hotkey warning (shown here since the
    // fixture leaves stop_hotkey_ok/pause_hotkey_ok undefined) have settled,
    // the widest this harness can reach (the disk warning needs the i18n
    // module, which never finishes loading here, same limitation the eta
    // tooltip check above already lives with).
    const fitCalls = () => (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'fit_rec_bar');
    const hotkeyWarnEl = document.getElementById('hotkey-warn');
    if (!await until(() => fitCalls().length > 0)) errs.push('fit_rec_bar was never called');
    else if (!await until(() => !hotkeyWarnEl.classList.contains('hidden'))) errs.push('hotkey warning never showed for the undefined stop/pause hotkey state');
    else {
      await sleep(100);
      const lastFit = fitCalls()[fitCalls().length - 1][1];
      const barLeft = bar.getBoundingClientRect().left;
      const stopRight = document.getElementById('stop-btn').getBoundingClientRect().right - barLeft;
      const cancelRight = document.getElementById('cancel-btn').getBoundingClientRect().right - barLeft;
      if (stopRight > lastFit.width) errs.push('stop-btn falls outside the fitted width: ' + stopRight + ' > ' + lastFit.width);
      if (cancelRight > lastFit.width) errs.push('cancel-btn falls outside the fitted width: ' + cancelRight + ' > ' + lastFit.width);
    }
    // Under the show threshold: the readout should go blank on the next poll.
    window.__REC_STATUS_OVERRIDE__ = { bytes: 50 * 1024 * 1024, show_size: false, size_eta_secs: null };
    if (!await until(() => sizeEl.textContent === '')) errs.push('size stayed visible under the show threshold: ' + sizeEl.textContent);
    // Past three quarters of the configured ceiling: highlighted. The eta
    // tooltip text itself needs the i18n module, which this stripped-down
    // harness never finishes loading (no real navigation, so the dynamic
    // import never settles) -- rec_bar_size_state's own ETA math has its own
    // Rust unit tests, so this only checks what this harness can see.
    window.__REC_STATUS_OVERRIDE__ = { bytes: 7500 * 1024 * 1024, show_size: true, size_eta_secs: 45 };
    if (!await until(() => /MB/.test(sizeEl.textContent) && sizeEl.classList.contains('rec-size-warn'))) errs.push('size did not render/highlight near the ceiling: ' + sizeEl.textContent);
    window.__REC_STATUS_OVERRIDE__ = null;
    await sleep(600);
    // Target-switch button: disabled by the default fixture (not a window
    // recording on a placement canvas), enabled once the status says so,
    // and a pick routed back through recorder-target-picked with forSwitch
    // reaches switch_recording_target rather than start_recording.
    const switchBtn = document.getElementById('switch-btn');
    if (!switchBtn.disabled) errs.push('switch button was not disabled by default');
    window.__REC_STATUS_OVERRIDE__ = { target_switch_enabled: true };
    if (!await until(() => !switchBtn.disabled)) errs.push('switch button never enabled once the status allowed it');
    switchBtn.click();
    await sleep(30);
    if (!window.__SMOKE_CALLS__.includes('open_recorder_switch')) errs.push('switch button did not open the picker');
    window.__FIRE__('recorder-target-picked', { kind: 'window', id: 4242, forSwitch: true });
    await sleep(30);
    const switchCall = (window.__SMOKE_ARGS__ || []).find((c) => c[0] === 'switch_recording_target');
    if (!switchCall) errs.push('a forSwitch pick did not call switch_recording_target');
    else if (switchCall[1].targetId !== 4242) errs.push('switch_recording_target got the wrong target id: ' + JSON.stringify(switchCall[1]));
    window.__REC_STATUS_OVERRIDE__ = null;
    await sleep(600);
    // "Finish and continue": disabled inside the first five seconds
    // (elapsed_ms mirrors the backend's own started_at_ms guard), enabled
    // past it, and a click reaches the backend command and shows its gap.
    const continueBtn = document.getElementById('continue-btn');
    const continueGapEl = document.getElementById('continue-gap');
    window.__REC_STATUS_OVERRIDE__ = { elapsed_ms: 2000 };
    if (!await until(() => continueBtn.disabled)) errs.push('continue button was not disabled in the first five seconds');
    window.__REC_STATUS_OVERRIDE__ = { elapsed_ms: 65000 };
    if (!await until(() => !continueBtn.disabled)) errs.push('continue button never enabled past the five-second lockout');
    continueBtn.click();
    await sleep(30);
    if (!window.__REC_CONTINUED__) errs.push('continue button did not call finish_and_continue_recording');
    if (continueGapEl.classList.contains('hidden')) errs.push('continue gap badge did not show after a successful continue');
    window.__REC_STATUS_OVERRIDE__ = null;
    await sleep(600);
    document.getElementById('pause-btn').click();
    await sleep(30);
    if (!window.__REC_PAUSED__) errs.push('pause button did not call pause_recording');
    document.getElementById('pause-btn').click();
    await sleep(30);
    if (window.__REC_PAUSED__ !== false) errs.push('pause button did not call resume_recording on the second click');
    document.getElementById('stop-btn').click();
    await sleep(30);
    if (!window.__REC_STOPPED__) errs.push('stop button did not call stop_recording');
    if (!window.__REC_BAR_HIDDEN__) errs.push('stop did not hide the bar afterwards');
  } catch (e) { errs.push('probe: ' + e.message); }
  clearInterval(heartbeat);
  document.documentElement.style.removeProperty('--smoke-tick');
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# Stubs getUserMedia to reject deterministically -- this host's own headless
# Chrome actually resolves a stream (a virtual device the CI image ships),
# so the "no camera" acceptance check has to force the rejection itself
# rather than lean on real hardware absence, which a stub is the documented
# way to do. This inline script runs synchronously during parsing, BEFORE
# camera.js's own module script (deferred by the HTML spec like every
# type="module" tag), so the override is in place before camera.js ever
# calls getUserMedia.
CAMERA_STUB = """
<script>
if (!navigator.mediaDevices) { navigator.mediaDevices = {}; }
navigator.mediaDevices.getUserMedia = () => Promise.reject(new DOMException('no camera', 'NotFoundError'));
navigator.mediaDevices.enumerateDevices = () => Promise.resolve([]);
</script>
"""

# The window opening at all and the message replacing the preview instead of
# a crash is the whole test -- CAMERA_STUB above forces the one path a real
# device-less machine would also take.
CAMERA_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 60) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    const noDeviceEl = document.getElementById('cam-no-device');
    const startBtn = document.getElementById('cam-start-btn');
    const ok = await until(() => !noDeviceEl.classList.contains('hidden'));
    if (!ok) errs.push('no-camera message never appeared for a device-less getUserMedia rejection');
    else if (!noDeviceEl.textContent.trim()) errs.push('no-camera message element is empty');
    if (!startBtn.disabled) errs.push('start button stayed enabled with no camera available');
    // The window itself must still be a normal, usable window around the
    // message -- the titlebar, close button and the three control rows.
    if (!document.getElementById('cam-close')) errs.push('close button is missing');
    if (!document.getElementById('cam-audio-select')) errs.push('audio source control is missing');
    if (!document.getElementById('cam-res-select')) errs.push('resolution control is missing');
    if (!document.getElementById('cam-folder-path')) errs.push('target folder control is missing');
    // 508: cam-res-select carries real options regardless of device
    // availability; audit its themed look across every built-in theme.
    {
      const priorTheme = document.documentElement.getAttribute('data-theme');
      const BUILTIN_THEMES = ['light', 'dark', 'programmer', 'neon', 'cyberpunk', 'mono', 'ocean'];
      const swatch = document.createElement('div');
      swatch.style.cssText = 'position:absolute;left:-9999px;top:-9999px;background-color:var(--surface)';
      document.body.appendChild(swatch);
      const resSel = document.getElementById('cam-res-select');
      for (const theme of BUILTIN_THEMES) {
        document.documentElement.setAttribute('data-theme', theme);
        if (resSel) {
          const cs = getComputedStyle(resSel);
          if (parseFloat(cs.borderTopWidth) <= 0) errs.push('508: cam-res-select has no border in theme ' + theme);
          if (cs.backgroundColor === 'rgba(0, 0, 0, 0)' || cs.backgroundColor === 'transparent') {
            errs.push('508: cam-res-select has a transparent background in theme ' + theme);
          }
          const surfaceRgb = getComputedStyle(swatch).backgroundColor;
          // A selected option shows the accent colour (option:checked); pick
          // an unselected one to check the plain surface colour instead.
          const unselected = [...resSel.options].find((o) => !o.selected);
          if (unselected) {
            const optBg = getComputedStyle(unselected).backgroundColor;
            if (optBg !== surfaceRgb) {
              errs.push('508: cam-res-select option background is ' + optBg + ', expected ' + surfaceRgb + ' in theme ' + theme);
            }
          }
        }
      }
      swatch.remove();
      if (priorTheme) document.documentElement.setAttribute('data-theme', priorTheme);
      else document.documentElement.removeAttribute('data-theme');
    }
    errs.push(...(window.__SMOKE_ERRORS__ || []));
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# The recording paths of the camera window: a resolving fake stream, a fake
# MediaRecorder and a scripted backend (every camera_* command is answered
# here and logged in __CAM.calls), so start failures, chunk-write failures and
# the device retry can be driven deterministically. Runs against its own copy
# of camera.html (camera_rec.html) so the plain no-camera run above stays
# untouched.
CAMERA_REC_STUB = """
<script type="module" src="i18n.js"></script>
<script type="module" src="floatShared.js"></script>
<script>
window.__CAM = { gum: [], calls: [], order: [], gumFail: null, ctorThrow: false, startThrow: false,
  n: 0, appendFail: function () { return false; }, elapsedDelay: 0, startDelay: 0, device: "dev-1" };
__CAM.gumFail = (c) => (c.video.deviceId ? new DOMException("gone", "NotFoundError") : null);
if (!navigator.mediaDevices) { navigator.mediaDevices = {}; }
navigator.mediaDevices.getUserMedia = (c) => {
  __CAM.gum.push(c);
  const err = __CAM.gumFail && __CAM.gumFail(c);
  if (err) return Promise.reject(err);
  return Promise.resolve(new MediaStream());
};
navigator.mediaDevices.enumerateDevices = () => Promise.resolve([]);
class FakeRec {
  static isTypeSupported() { return true; }
  constructor() {
    if (__CAM.ctorThrow) throw new Error("ctor");
    this.state = "inactive";
    window.__REC = this;
  }
  start() {
    if (__CAM.startThrow) throw new Error("start");
    this.state = "recording";
    setTimeout(() => { __CAM.startedAt = Date.now(); if (this.onstart) this.onstart(); }, __CAM.startDelay);
  }
  stop() { this.state = "inactive"; if (this.onstop) this.onstop(); }
  emit(text) { this.ondataavailable({ data: new Blob([text]) }); }
}
window.MediaRecorder = FakeRec;
const origInvoke = window.__TAURI__.core.invoke;
window.__TAURI__.core.invoke = async (cmd, args) => {
  __CAM.calls.push([cmd, args]);
  if (cmd === "camera_begin_recording") return { path: "C:/r/rec-cam-1.webm" };
  if (cmd === "camera_append_chunk") {
    const i = ++__CAM.n;
    await new Promise((r) => setTimeout(r, i * 20));
    __CAM.order.push("append-done-" + i);
    if (__CAM.appendFail(i)) throw new Error("disk full");
    return null;
  }
  if (cmd === "camera_screen_elapsed_ms") {
    await new Promise((r) => setTimeout(r, __CAM.elapsedDelay));
    return 4242;
  }
  if (cmd === "camera_finish_recording") { __CAM.order.push("finish"); return { path: args.path }; }
  if (cmd === "camera_mark_finishing" || cmd === "camera_discard_recording" || cmd === "camera_abandon_recording") return null;
  if (cmd === "open_video_editor") return null;
  if (cmd === "get_settings") {
    const s = await origInvoke(cmd, args);
    return Object.assign({}, s, { ui_texts: Object.assign({}, s.ui_texts || {}, { cameraDevice: __CAM.device }) });
  }
  return origInvoke(cmd, args);
};
</script>
"""

CAMERA_REC_PROBE = """
<script>
(async () => {
  const errs = [];
  let step = "boot";
  const finish = () => {
    if (document.getElementById("SMOKE")) return;
    const pre = document.createElement("pre");
    pre.id = "SMOKE";
    pre.textContent = JSON.stringify(errs);
    document.body.appendChild(pre);
  };
  setTimeout(() => { errs.push("probe stalled at step " + step); finish(); }, 25000);
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50);
    return fn();
  };
  const count = (cmd) => __CAM.calls.filter(c => c[0] === cmd).length;
  const startBtn = document.getElementById("cam-start-btn");
  const statusText = document.getElementById("cam-status-text");
  const recBadge = document.getElementById("cam-rec-badge");
  const pauseBtn = document.getElementById("cam-pause-btn");
  const stopBtn = document.getElementById("cam-stop-btn");
  const idleUi = (label) => {
    if (startBtn.disabled) errs.push(label + ": Start stayed disabled");
    if (startBtn.classList.contains("hidden")) errs.push(label + ": Start is hidden");
    if (!recBadge.classList.contains("hidden")) errs.push(label + ": rec badge still visible");
    if (!pauseBtn.classList.contains("hidden")) errs.push(label + ": pause still visible");
    if (!stopBtn.classList.contains("hidden")) errs.push(label + ": stop still visible");
  };
  try {
    step = "ready";
    // Settings (and with them the saved camera id) load before the first
    // preview request, the way the permission-ready event orders it for real.
    await until(() => startBtn.textContent.trim());
    window.__FIRE__("camera-permission-ready", null);
    if (!(await until(() => !startBtn.disabled))) errs.push("Start never became enabled with a resolving stream");
    // The saved camera is gone: exactly one retry with the default camera.
    step = "deviceRetry";
    if (__CAM.gum.length !== 2) errs.push("device retry: getUserMedia calls " + __CAM.gum.length);
    else {
      if (!__CAM.gum[0].video.deviceId) errs.push("device retry: first call had no exact device");
      if (__CAM.gum[1].video.deviceId) errs.push("device retry: second call still had a device");
    }
    // A permission denial is not a stale device: no retry, Start stays off.
    step = "denied";
    __CAM.gum.length = 0;
    __CAM.gumFail = () => new DOMException("no", "NotAllowedError");
    document.getElementById("cam-res-select").dispatchEvent(new Event("change"));
    await until(() => startBtn.disabled);
    await sleep(100);
    if (__CAM.gum.length !== 1) errs.push("denied: getUserMedia calls " + __CAM.gum.length);
    if (!startBtn.disabled) errs.push("denied: Start stayed enabled");
    __CAM.gumFail = (c) => (c.video.deviceId ? new DOMException("gone", "NotFoundError") : null);
    document.getElementById("cam-res-select").dispatchEvent(new Event("change"));
    if (!(await until(() => !startBtn.disabled))) errs.push("Start never came back after the denial cleared");
    // Constructor throws after the backend already reserved the take.
    for (const mode of ["ctorThrow", "startThrow"]) {
      step = mode;
      __CAM[mode] = true;
      __CAM.calls.length = 0;
      startBtn.click();
      await until(() => count("camera_discard_recording") > 0 && !startBtn.disabled);
      await sleep(100);
      if (count("camera_begin_recording") !== 1) errs.push(mode + ": begin count " + count("camera_begin_recording"));
      if (count("camera_discard_recording") !== 1) errs.push(mode + ": discard count " + count("camera_discard_recording"));
      const d = __CAM.calls.find(c => c[0] === "camera_discard_recording");
      if (d && d[1].path !== "C:/r/rec-cam-1.webm") errs.push(mode + ": discarded the wrong path");
      if (statusText.textContent.trim()) errs.push(mode + ": status text left over: " + statusText.textContent);
      idleUi(mode);
      __CAM[mode] = false;
    }
    // A rejected middle append: every append is still drained, the take is
    // not finished or opened, the session is released once and never deleted.
    step = "appendFail";
    __CAM.calls.length = 0;
    __CAM.order.length = 0;
    __CAM.n = 0;
    __CAM.appendFail = (i) => i === 2;
    startBtn.click();
    if (!(await until(() => window.__REC && window.__REC.state === "recording"))) errs.push("recorder never started");
    window.__REC.emit("a");
    window.__REC.emit("b");
    window.__REC.emit("c");
    window.__REC.stop();
    await until(() => statusText.textContent.trim() && !startBtn.disabled, 200);
    await sleep(100);
    if (statusText.textContent !== "Part of the recording could not be saved. The raw file was kept: C:/r/rec-cam-1.webm") errs.push("failure message missing, status: " + statusText.textContent);
    if (__CAM.order.filter(o => o.indexOf("append-done") === 0).length !== 3) errs.push("not every append was drained: " + __CAM.order.join());
    if (count("camera_finish_recording") !== 0) errs.push("a take with a failed chunk was finished");
    if (count("open_video_editor") !== 0) errs.push("the editor opened for a damaged take");
    if (count("camera_abandon_recording") !== 1) errs.push("abandon count " + count("camera_abandon_recording"));
    if (count("camera_discard_recording") !== 0) errs.push("the raw file was discarded");
    idleUi("appendFail");
    if (document.getElementById("cam-res-select").disabled) errs.push("appendFail: resolution select stayed disabled");
    // The failure lands while the take is still recording: the session is
    // released at once, so a later stop never marks it finishing (a window
    // closed in between could otherwise write a sidecar for a corrupt take).
    step = "failBeforeStop";
    __CAM.calls.length = 0;
    __CAM.order.length = 0;
    __CAM.n = 0;
    __CAM.appendFail = (i) => i === 1;
    startBtn.click();
    await until(() => window.__REC && window.__REC.state === "recording");
    window.__REC.emit("a");
    window.__REC.emit("b");
    await until(() => count("camera_abandon_recording") > 0);
    if (count("camera_abandon_recording") !== 1) errs.push("failBeforeStop: abandon not called when the append failed");
    if (count("camera_mark_finishing") !== 0) errs.push("failBeforeStop: marked finishing too early");
    await sleep(200);
    window.__REC.stop();
    await until(() => !startBtn.disabled, 200);
    await sleep(100);
    if (count("camera_mark_finishing") !== 0) errs.push("failBeforeStop: a released take was marked finishing");
    if (count("camera_abandon_recording") !== 1) errs.push("failBeforeStop: abandon count " + count("camera_abandon_recording"));
    if (count("camera_finish_recording") !== 0 || count("camera_discard_recording") !== 0) errs.push("failBeforeStop: finished or discarded");
    if (!statusText.textContent.includes("C:/r/rec-cam-1.webm")) errs.push("failBeforeStop: message lacks the file path");
    // A clean take waits for its last append before finishing, and the
    // camera start plus screen offset are stamped at the recorder start event.
    step = "success";
    __CAM.calls.length = 0;
    __CAM.order.length = 0;
    __CAM.n = 0;
    __CAM.appendFail = () => false;
    __CAM.startDelay = 300;
    __CAM.elapsedDelay = 100;
    const t0 = Date.now();
    startBtn.click();
    await until(() => window.__REC && window.__REC.state === "recording" && __CAM.startedAt > t0 - 1);
    await sleep(400);
    window.__REC.emit("a");
    window.__REC.emit("b");
    window.__REC.stop();
    // While the finish is in flight neither Start nor the selects may be live.
    if (!startBtn.disabled) errs.push("Start enabled while the finish is in flight");
    if (!document.getElementById("cam-res-select").disabled || !document.getElementById("cam-audio-select").disabled) errs.push("selects enabled while the finish is in flight");
    await until(() => count("open_video_editor") > 0, 200);
    await sleep(300);
    if (startBtn.disabled) errs.push("Start not re-enabled after the finish");
    if (document.getElementById("cam-res-select").disabled) errs.push("selects not re-enabled after the finish");
    {
      const marks = __CAM.calls.filter(c => c[0] === "camera_mark_finishing");
      if (!marks.length || marks[marks.length - 1][1].screenElapsedMs !== 4242) errs.push("mark finishing lacks the screen offset");
    }
    if (count("camera_finish_recording") !== 1) errs.push("finish count " + count("camera_finish_recording"));
    if (count("camera_abandon_recording") !== 0) errs.push("a good take was abandoned");
    if (__CAM.order.join() !== "append-done-1,append-done-2,finish") errs.push("finish did not wait for the last append: " + __CAM.order.join());
    if (count("open_video_editor") !== 1) errs.push("editor open count " + count("open_video_editor"));
    if (statusText.textContent.trim()) errs.push("status text after a good take: " + statusText.textContent);
    // The start stamp follows the recorder start event (300 ms after the
    // click here), and the screen offset comes from a read taken there.
    const fin = __CAM.calls.find(c => c[0] === "camera_finish_recording");
    if (fin) {
      if (fin[1].camStartMs < t0 + 250) errs.push("camera start stamped before the recorder started: " + (fin[1].camStartMs - t0));
      if (fin[1].screenElapsedMs !== 4242) errs.push("screen offset not carried: " + fin[1].screenElapsedMs);
    }
  } catch (e) {
    errs.push("probe crashed: " + e);
  }
  finish();
})();
</script>
"""


# The video editor: the recording is PULLED at boot (video_editor_source), not
# pushed, so the probe only has to wait -- tauri_stub.js's own fixture is a
# 10-minute 1080p30 clip with both a system and a mic track, real enough that a
# populated filmstrip and both waveforms are what "boot finished" looks like.
VIDEO_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));""" + SIDE_SCAN + """
  const rawKeyCheck = (where) => {
    for (const el of document.querySelectorAll('body *')) {
      if (el.children.length) continue;
      const txt = (el.textContent || '').trim();
      if (/^ve[A-Z][A-Za-z]+$/.test(txt)) errs.push(where + ': a raw i18n key rendered: ' + txt);
    }
  };
  const hitTestable = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width < 4 || r.height < 4) return false;
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return !!hit && hit.closest('button') === el;
  };
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 60) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    // The recording itself resolves through invoke calls (fast, deterministic),
    // but the localized text comes from a lazily imported module -- a real
    // async fetch that a flat sleep can race ahead of under the virtual clock
    // (same reasoning FLOAT_PROBE calls out for the pill window) -- and the
    // waveform draw sits behind its own scheduleWaves() timeout on top of that.
    // One combined poll for everything "boot finished" means, rather than a
    // fixed delay that only happens to be long enough some of the time.
    await until(() => {
      const opsHint = document.querySelector('#ve-ops-list p');
      return document.getElementById('ve-stage-msg').classList.contains('hidden') &&
        document.querySelectorAll('#ve-filmstrip .ve-thumb').length > 0 &&
        document.getElementById('ve-wave-sys').width > 10 &&
        document.getElementById('ve-wave-mic').width > 10 &&
        !!opsHint && opsHint.textContent && opsHint.textContent !== 'veOpsEmpty';
    }, 300);
    // ---- the pulled recording loaded into a populated editor ----
    if (!document.getElementById('ve-stage-msg').classList.contains('hidden')) {
      errs.push('the stage never left its loading state');
    }
    if (document.getElementById('ve-duration').textContent !== '10:00.000') {
      errs.push('duration did not render from probe_recording: ' + document.getElementById('ve-duration').textContent);
    }
    if (!document.querySelectorAll('#ve-filmstrip .ve-thumb').length) {
      errs.push('no filmstrip thumbnails rendered from generate_filmstrip');
    }
    // Both canvases carry a hardcoded placeholder width of 10 in the markup, so
    // "still 10" is what an un-rendered waveform looks like, not zero.
    if (document.getElementById('ve-wave-sys').width <= 10) {
      errs.push('the system waveform canvas was never redrawn from generate_waveform_peaks');
    }
    if (document.getElementById('ve-wave-mic').classList.contains('hidden')) {
      errs.push('the mic waveform track stayed hidden although the fixture reports a mic track');
    } else if (document.getElementById('ve-wave-mic').width <= 10) {
      errs.push('the mic waveform canvas was never redrawn from generate_waveform_peaks');
    }
    if (!document.getElementById('ve-ffmpeg-warn').classList.contains('hidden')) {
      errs.push('the ffmpeg-missing banner showed although probe_recording succeeded');
    }
    // ---- 337: the mic track starts muted, the system track does not ----
    if (!document.getElementById('ve-mute-mic').classList.contains('ve-muted-btn')) {
      errs.push('the mic track must start muted in the video editor');
    }
    if (document.getElementById('ve-mute-sys').classList.contains('ve-muted-btn')) {
      errs.push('the system track must not start muted in the video editor');
    }
    document.getElementById('ve-mute-mic').click();
    await sleep(30);
    if (document.getElementById('ve-mute-mic').classList.contains('ve-muted-btn')) {
      errs.push('clicking the mic mute button must mix the mic track back in');
    }
    {
      const shortcutsBtn = document.getElementById('ve-shortcuts');
      const shortcutsModal = document.getElementById('shortcuts-modal');
      if (!shortcutsBtn || !hitTestable(shortcutsBtn)) {
        errs.push('video editor: shortcuts overview button is missing, hidden, or not clickable');
      } else {
        shortcutsBtn.click();
        await sleep(50);
        if (shortcutsModal.classList.contains('hidden')) {
          errs.push('video editor: shortcuts overview did not open');
        } else {
          const rows = shortcutsModal.querySelectorAll('.shortcuts-row');
          if (rows.length === 0) errs.push('video editor: shortcuts overview opened with no rows');
          document.getElementById('shortcuts-modal-close').click();
          await sleep(50);
          if (!shortcutsModal.classList.contains('hidden')) {
            errs.push('video editor: shortcuts overview did not close');
          }
        }
      }
    }
    rawKeyCheck('edit tab');
    sideScan(errs, 'edit tab');

    // ---- numberField's stepper aria-labels used to be set once at
    // construction and never revisited, so a live language switch left them
    // stuck in whatever language was current at boot ----
    // (see numberField's decreaseAriaKey/increaseAriaKey/presetsAriaKey in
    // ui/floatShared.js, wired up here and in ui/main.js so applyI18n()'s
    // existing generic [data-i18n-aria] refresh loop covers them too).
    const cropDecBtn = document.getElementById('ve-crop-x').parentElement.querySelector('.numfield-dec');
    if (cropDecBtn.getAttribute('aria-label') !== 'Decrease value') {
      errs.push('the crop-X stepper did not start with the English aria-label: ' + cropDecBtn.getAttribute('aria-label'));
    }
    if (cropDecBtn.dataset.i18nAria !== 'numFieldDecrease') {
      errs.push('the crop-X stepper has no data-i18n-aria, so its aria-label would go stale on a language switch: ' + cropDecBtn.dataset.i18nAria);
    }
    const speedListBtn = document.getElementById('ve-speed-custom').parentElement.querySelector('.numfield-list');
    if (!speedListBtn || speedListBtn.dataset.i18nAria !== 'numFieldPresets') {
      errs.push('the speed presets stepper has no data-i18n-aria: ' + (speedListBtn && speedListBtn.dataset.i18nAria));
    }

    // ---- the timeline never clips vertically (horizontal scroll is expected) ----
    const timeline = document.getElementById('ve-timeline');
    const track = document.getElementById('ve-track');
    if (track.getBoundingClientRect().height > timeline.getBoundingClientRect().height + 1) {
      errs.push('the timeline track is taller than its own scroller and gets clipped');
    }

    // ---- every tab and toolbar control stays reachable ----
    const controls = [...document.querySelectorAll(
      '.ve-transport button, .ve-tools button, .ve-tabs button, .ve-titlebar button, ' +
      '.ve-audio-group button, .ve-ops-head button')];
    const untestable = controls.filter((b) => !hitTestable(b));
    if (untestable.length) {
      errs.push(untestable.length + ' toolbar control(s) cannot be hit-tested: ' +
        untestable.map((b) => b.id || b.className).join(', '));
    }

    // ---- tabs actually switch panels ----
    document.getElementById('ve-tab-overlays').click();
    await sleep(80);
    if (document.getElementById('ve-panel-overlays').classList.contains('hidden')) {
      errs.push('the Overlays tab did not reveal its panel');
    }
    if (!document.getElementById('ve-panel-edit').classList.contains('hidden')) {
      errs.push('the Edit panel stayed visible under the Overlays tab');
    }

    // ---- 334: one of each overlay kind actually renders in the live-preview
    // layer, not just the edit-handle rectangle ----
    const liveLayer = document.getElementById('ve-overlay-live');
    if (liveLayer.children.length) {
      errs.push('the live overlay preview shows something before any overlay was added');
    }
    const overlayKinds = [
      ['ve-ov-add-image', 've-live-image'],
      ['ve-ov-add-frame', 've-live-frame'],
      ['ve-ov-add-text', 've-live-text'],
      ['ve-ov-add-pixelate', 've-live-pixelate'],
      ['ve-ov-add-spotlight', 've-live-spotlight'],
      ['ve-ov-add-keys', 've-live-keys'],
    ];
    for (const [btnId, liveClass] of overlayKinds) {
      document.getElementById(btnId).click();
      await sleep(30);
      if (!liveLayer.querySelector('.' + liveClass)) {
        errs.push(btnId + ' did not produce a .' + liveClass + ' element in the live overlay preview');
      }
    }
    // 526: an inserted "video" overlay (a picture-in-picture clip, added
    // through "Add a video" -> "As an overlay" rather than one of the plain
    // ADD AN OVERLAY buttons above) used to have no live-preview branch at
    // all in buildLiveOverlayEl -- it rendered fine in export but never
    // showed anything in the editor while working on it.
    document.getElementById('ve-add-video').click();
    await sleep(30);
    document.getElementById('ve-add-video-as-overlay').click();
    await sleep(30);
    if (!liveLayer.querySelector('.ve-live-video')) {
      errs.push('a "video" overlay did not produce a .ve-live-video element in the live overlay preview');
    }
    // Every overlay just defaulted to the clip's [0, ve-duration] range --
    // seeking to the exact start must still keep them all visible (inclusive
    // boundary), proving the gate really reads the range rather than always
    // showing everything regardless of the playhead.
    document.getElementById('ve-tab-edit').click();
    await sleep(30);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home' }));
    await sleep(30);
    if (liveLayer.children.length !== overlayKinds.length + 1) {
      errs.push('live overlay preview lost overlays at the exact start of their range (playhead 0)');
    }
    document.getElementById('ve-tab-overlays').click();
    await sleep(30);

    // ---- 524: the spotlight dim must be real painted geometry, not a huge
    // box-shadow spread some GPUs drop on real hardware. Select the spotlight
    // row so its edit outline (sized to op.rect, same transform the live
    // preview uses) gives a ground-truth clear rect. The live layer has
    // pointer-events: none (it is never interactive), so elementFromPoint
    // cannot see it -- instead read the actual painted shape back from the
    // computed clip-path: the dim element must cover far more than the clear
    // rect, and the punched-out hole in its clip-path must line up with that
    // same rect. A plain box-shadow spread sets no clip-path and keeps the
    // element itself sized to op.rect, so this fails on the old code.
    {
      const spotlightRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-spotlight');
      const rowsNow = [...document.querySelectorAll('#ve-ov-list .ve-ov-row')];
      const spotRow = rowsNow[spotlightRowIdx];
      if (!spotRow) {
        errs.push('524: could not find the spotlight overlay row to select it');
      } else {
        spotRow.click();
        await sleep(30);
        const clearEl = document.querySelector('.ve-rect-spotlight');
        const liveEl = document.querySelector('.ve-live-spotlight');
        if (!clearEl || !liveEl) {
          errs.push('524: spotlight edit outline or live preview element is missing');
        } else {
          const clear = clearEl.getBoundingClientRect();
          const live = liveEl.getBoundingClientRect();
          const bg = getComputedStyle(liveEl).backgroundColor;
          const parts = bg.match(/rgba?\\(([^)]+)\\)/);
          const nums = parts ? parts[1].split(',').map((s) => parseFloat(s.trim())) : [];
          const alpha = nums.length > 3 ? nums[3] : (bg === 'transparent' ? 0 : 1);
          if (Math.abs(alpha - 0.6) > 0.05) {
            errs.push('524: spotlight live dim background alpha is ' + alpha + ', expected about 0.6 (op.dim)');
          }
          if (live.width < clear.width + 20 || live.height < clear.height + 20) {
            errs.push('524: the spotlight dim element is not larger than its own clear rect (no dim area outside it)');
          }
          const clipPath = getComputedStyle(liveEl).clipPath;
          if (!clipPath || clipPath === 'none') {
            errs.push('524: the spotlight dim has no clip-path, so it cannot punch a clear hole in real geometry');
          } else {
            const pts = [...clipPath.matchAll(/([\\d.]+)px\\s+([\\d.]+)px/g)].map((m) => [parseFloat(m[1]), parseFloat(m[2])]);
            const hole = pts.slice(5, 9);
            if (hole.length < 4) {
              errs.push('524: the spotlight clip-path does not carry a separate hole shape: ' + clipPath);
            } else {
              const xs = hole.map((p) => p[0]), ys = hole.map((p) => p[1]);
              const holeLeft = live.left + Math.min(...xs), holeTop = live.top + Math.min(...ys);
              const holeW = Math.max(...xs) - Math.min(...xs), holeH = Math.max(...ys) - Math.min(...ys);
              const near = (a, b) => Math.abs(a - b) <= 3;
              if (!near(holeLeft, clear.left) || !near(holeTop, clear.top) || !near(holeW, clear.width) || !near(holeH, clear.height)) {
                errs.push('524: the clip-path hole (' + [holeLeft, holeTop, holeW, holeH].map((n) => n.toFixed(1)) +
                  ') does not line up with the clear rect (' + [clear.left, clear.top, clear.width, clear.height].map((n) => n.toFixed(1)) + ')');
              }
            }
          }
        }
      }
    }

    rawKeyCheck('overlays tab');
    sideScan(errs, 'overlays tab');

    // ---- 354: text can be selected in the side panels, not on the drag surfaces ----
    const selectableIds = ['ve-export-dest', 've-trim-start-val'];
    for (const id of selectableIds) {
      const style = getComputedStyle(document.getElementById(id));
      if (style.userSelect === 'none') {
        errs.push('#' + id + ' cannot be text-selected (user-select: none)');
      }
    }
    if (getComputedStyle(document.querySelector('.ve-titlebar')).userSelect !== 'none') {
      errs.push('the titlebar drag surface must stay non-selectable');
    }

    // ---- 355/356: select the text overlay, check it draws once and that a
    // value commit does not steal focus away from the field being edited ----
    const ovRows = [...document.querySelectorAll('#ve-ov-list .ve-ov-row')];
    const textRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-text');
    if (!ovRows[textRowIdx]) {
      errs.push('could not find the text overlay row in the overlay list');
    } else {
      ovRows[textRowIdx].click();
      await sleep(30);
      const textField = document.querySelector('#ve-ov-editor [data-ov-field="text"]');
      if (!textField) {
        errs.push('the text overlay editor has no tracked text-value field');
      } else {
        const before = textField.value;
        const withText = [...document.querySelectorAll('#ve-stage *')].filter((el) => el.textContent === before && el.children.length === 0);
        if (withText.length !== 1) {
          errs.push('expected exactly one element showing the overlay text in #ve-stage, found ' + withText.length);
        }
        textField.focus();
        textField.value = before + '!';
        // Set the caret AFTER the value assignment -- assigning .value itself
        // always snaps the caret to the end, so this reflects where a real
        // typist's cursor sits right before the commit this probe simulates.
        textField.setSelectionRange(2, 2);
        textField.dispatchEvent(new Event('input'));
        textField.dispatchEvent(new Event('change'));
        await sleep(30);
        if (document.activeElement !== textField) {
          errs.push('356: committing a text overlay value moved focus away from the field being edited');
        } else if (textField.selectionStart !== 2) {
          errs.push('356: committing a text overlay value moved the caret in the still-focused field');
        }
      }
      // the currently selected op is text, not spotlight/pixelate -- select the
      // spotlight row to check its rect-tip hint carries no duplicate tooltip
      const spotRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-spotlight');
      if (ovRows[spotRowIdx]) {
        ovRows[spotRowIdx].click();
        await sleep(30);
        const hint = document.querySelector('#ve-ov-editor .modal-hint');
        if (hint && hint.hasAttribute('title')) {
          errs.push('355: the pixelate/spotlight hint still repeats itself as a tooltip');
        }
      }
    }

    // ---- 463: the keys overlay offers all four presets plus a free
    // selection, and switching to the free selection reveals its picker ----
    {
      const keysRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-keys');
      if (!ovRows[keysRowIdx]) {
        errs.push('could not find the keys overlay row in the overlay list');
      } else {
        ovRows[keysRowIdx].click();
        await sleep(30);
        const presetSelect = document.querySelector('#ve-ov-editor select');
        const values = presetSelect ? [...presetSelect.options].map((o) => o.value) : [];
        const expected = ['pressed', 'half', 'full', 'game', 'custom'];
        if (!presetSelect || expected.some((v) => !values.includes(v))) {
          errs.push('463: the keys overlay preset dropdown is missing one of ' + expected.join(', ') + ', got ' + values.join(', '));
        } else {
          presetSelect.value = 'custom';
          presetSelect.dispatchEvent(new Event('change'));
          await sleep(30);
          const picker = document.querySelector('#ve-ov-editor .ve-keys-picker');
          if (!picker || !picker.querySelector('input[type="checkbox"]')) {
            errs.push('463: choosing the custom preset did not reveal the free-selection key picker');
          }
        }
      }
    }

    // ---- 356: dragging an overlay's rectangle updates its X/Y fields in
    // place while a different field in the same panel keeps focus ----
    {
      const imgRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-image');
      if (ovRows[imgRowIdx]) {
        ovRows[imgRowIdx].click();
        await sleep(30);
        const opacityField = document.querySelector('#ve-ov-editor [data-ov-field="opacity"]');
        const xField = document.querySelector('#ve-ov-editor [data-ov-field="rect.x"]');
        const rectEl = document.querySelector('#ve-rects .ve-rect-image');
        if (opacityField && xField && rectEl) {
          opacityField.focus();
          const beforeX = xField.value;
          const box = rectEl.getBoundingClientRect();
          const cx = box.left + box.width / 2, cy = box.top + box.height / 2;
          rectEl.dispatchEvent(new PointerEvent('pointerdown', { clientX: cx, clientY: cy, pointerId: 1, bubbles: true }));
          document.getElementById('ve-rects').dispatchEvent(new PointerEvent('pointermove', { clientX: cx + 20, clientY: cy + 20, pointerId: 1, bubbles: true }));
          document.getElementById('ve-rects').dispatchEvent(new PointerEvent('pointerup', { clientX: cx + 20, clientY: cy + 20, pointerId: 1, bubbles: true }));
          await sleep(30);
          if (document.activeElement !== opacityField) {
            errs.push('356: dragging an overlay rectangle moved focus away from an unrelated field in the same panel');
          }
          if (xField.value === beforeX) {
            errs.push('356: dragging an overlay rectangle did not refresh its own X field');
          }
        } else {
          errs.push('356: could not find the image overlay rect/fields to test the drag path');
        }
      }
    }

    // 370: the overlay colour row's custom swatch opens the same shared
    // colour popup the main window uses, and picking a hex both updates the
    // live preview and shows on the swatch itself.
    {
      ovRows[textRowIdx].click();
      await sleep(30);
      const customSwatch = document.querySelector('#ve-ov-editor .ve-swatches .swatch.custom');
      if (!customSwatch) {
        errs.push('370: no custom-colour swatch in the overlay colour row');
      } else {
        customSwatch.click();
        await sleep(30);
        const pop = document.querySelector('.color-pop');
        if (!pop || pop.classList.contains('hidden')) {
          errs.push('370: clicking the custom swatch did not open the shared colour popup');
        } else {
          const hexInput = pop.querySelector('.cp-hex');
          const liveText = document.querySelector('#ve-overlay-live .ve-live-text');
          hexInput.value = '#3fa796';
          hexInput.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(30);
          if (!liveText) {
            errs.push('370: no .ve-live-text element to check the colour change against');
          } else if (liveText.style.color !== 'rgb(63, 167, 150)') {
            errs.push('370: picking a custom colour did not update the live overlay preview, got ' + liveText.style.color);
          }
          const freshSwatch = document.querySelector('#ve-ov-editor .ve-swatches .swatch.custom');
          if (!freshSwatch || !freshSwatch.classList.contains('sel') || freshSwatch.style.background !== 'rgb(63, 167, 150)') {
            errs.push('370: the custom swatch does not show the picked colour after the row refreshes');
          }
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
          await sleep(30);
          if (!pop.classList.contains('hidden')) {
            errs.push('370: Escape did not close the shared colour popup');
          }
          if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
            errs.push('370: Escape while the colour popup was open also triggered the close-window confirm dialog');
          }
        }
      }
      // Restore the image overlay selection the later 364/365 checks below
      // depend on -- this block borrowed the text overlay for its own probe.
      const imgRowIdxAgain = overlayKinds.findIndex(([id]) => id === 've-ov-add-image');
      if (ovRows[imgRowIdxAgain]) {
        ovRows[imgRowIdxAgain].click();
        await sleep(30);
      }
    }

    // 364: the shared mouse-only number field, exercised on the still-selected
    // image overlay's opacity control (step 5, presets 25/50/75/100).
    {
      const opacityInput = document.querySelector('#ve-ov-editor [data-ov-field="opacity"]');
      const numfield = opacityInput && opacityInput.closest('.numfield');
      if (!numfield) {
        errs.push('364: could not find the opacity number field to test it');
      } else {
        const inc = numfield.querySelector('.numfield-inc');
        const dec = numfield.querySelector('.numfield-dec');
        const list = numfield.querySelector('.numfield-list');
        // Seed a mid-range value first -- the overlay's default opacity can
        // already sit at this field's max, which would leave + disabled and
        // make the click/hold steps below silent no-ops.
        opacityInput.value = '50';
        opacityInput.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(10);
        // Click: one pointerdown+up steps by exactly `step` (5).
        const beforeClick = Number(opacityInput.value);
        inc.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 21 }));
        await sleep(10);
        inc.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 21 }));
        await sleep(10);
        if (Number(opacityInput.value) !== beforeClick + 5) {
          errs.push('364: clicking + did not step the opacity field by exactly 5');
        }
        // Hold: still down past the 400ms/60ms repeat threshold must step more
        // than once (using - this time, so it also cannot get stuck at max=100).
        const beforeHold = Number(opacityInput.value);
        dec.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 22 }));
        await sleep(550);
        dec.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 22 }));
        await sleep(10);
        if (Number(opacityInput.value) > beforeHold - 10) {
          errs.push('364: holding - did not repeat-step the opacity field past a single step');
        }
        // Wheel: one wheel tick steps by exactly `step` (5), and must not
        // scroll the panel underneath it.
        const beforeWheel = Number(opacityInput.value);
        opacityInput.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, deltaY: -100 }));
        await sleep(30);
        if (Number(opacityInput.value) !== beforeWheel + 5) {
          errs.push('364: wheel over the opacity field did not step by exactly 5');
        }
        // Presets: opening the list and picking 100 sets the field to it and
        // closes the popover again.
        if (!list) {
          errs.push('364: the opacity field has no presets button');
        } else {
          list.click();
          await sleep(30);
          const pop = document.querySelector('.numfield-pop');
          const target = pop && [...pop.querySelectorAll('button')].find((b) => b.textContent === '100');
          if (!target) {
            errs.push('364: the opacity presets popover did not offer 100');
          } else {
            target.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 23 }));
            await sleep(30);
            if (Number(opacityInput.value) !== 100) {
              errs.push('364: picking the 100 preset did not set the opacity field to it');
            }
            if (document.querySelector('.numfield-pop')) {
              errs.push('364: picking a preset did not close the popover');
            }
          }
        }
        // At its max (just set to 100 above), + must now be visibly dead.
        if (!inc.disabled) {
          errs.push('364: the + button is not disabled once the opacity field is at its max');
        }
      }
      // The overlay editor column must not scroll sideways at this window's
      // minimum width just because a numfield now lives in it.
      const ovEditor = document.getElementById('ve-ov-editor');
      if (ovEditor && ovEditor.scrollWidth > ovEditor.clientWidth + 1) {
        errs.push('364: a number field overflows the overlay editor column at this window width');
      }
    }

    // 365: wheel-zooming the stage keeps the point under the cursor fixed
    // instead of always zooming to the stage centre. The image overlay's
    // rectangle from the 356 drag above sits off-centre by now -- its own
    // top-left corner is a known, trackable point to zoom onto.
    {
      const rectEl = document.querySelector('#ve-rects .ve-rect-image');
      if (!rectEl) {
        errs.push('365: could not find the image overlay rectangle to test the zoom anchor');
      } else {
        const boxBefore = rectEl.getBoundingClientRect();
        const cx = boxBefore.left, cy = boxBefore.top;
        document.getElementById('ve-stage').dispatchEvent(
          new WheelEvent('wheel', { bubbles: true, cancelable: true, clientX: cx, clientY: cy, deltaY: -100 }));
        await sleep(30);
        // renderRects() rebuilds the rectangle elements on every zoom/pan
        // change, so the original rectEl reference is now detached -- re-query.
        const rectElAfter = document.querySelector('#ve-rects .ve-rect-image');
        const boxAfter = rectElAfter ? rectElAfter.getBoundingClientRect() : { left: NaN, top: NaN };
        const moved = Math.hypot(boxAfter.left - cx, boxAfter.top - cy);
        if (!rectElAfter) {
          errs.push('365: the image overlay rectangle disappeared after zooming the stage');
        } else if (moved >= 2) {
          errs.push('365: zooming the video stage with the wheel did not keep the point under the cursor fixed (moved ' + moved.toFixed(1) + 'px)');
        }
        document.getElementById('ve-preview-zoom-reset')?.click();
        await sleep(30);
      }
    }

    // ---- 393: dragging an overlay near the frame centre snaps it exactly
    // onto that centre, and Alt held during the drag suppresses the snap.
    // Runs after 365 above (rather than right after 356, where it would
    // otherwise sit) because 365 depends on the image overlay still sitting
    // exactly where the 356 drag left it, off-centre, as its own known point
    // to zoom onto -- this block moves it around a great deal, so it runs
    // once that dependency is no longer needed. ----
    {
      const imgRowIdxSnap = overlayKinds.findIndex(([id]) => id === 've-ov-add-image');
      if (ovRows[imgRowIdxSnap]) {
        // The row is still the one selected from 356/364/365 above; a row
        // click toggles selection off, so only click if it is not open.
        const ovEditorBox = document.getElementById('ve-ov-editor');
        const imageAlreadyOpen = !ovEditorBox.classList.contains('hidden') && !!ovEditorBox.querySelector('.ve-path');
        if (!imageAlreadyOpen) {
          ovRows[imgRowIdxSnap].click();
          await sleep(30);
        }
        const xField = document.querySelector('#ve-ov-editor [data-ov-field="rect.x"]');
        const yField = document.querySelector('#ve-ov-editor [data-ov-field="rect.y"]');
        const wField = document.querySelector('#ve-ov-editor [data-ov-field="rect.w"]');
        const hField = document.querySelector('#ve-ov-editor [data-ov-field="rect.h"]');
        if (!xField || !yField || !wField || !hField || !document.querySelector('#ve-rects .ve-rect-image')) {
          errs.push('393: could not find the image overlay rect/fields to test snapping');
        } else {
          const expectX = 960 - Number(wField.value) / 2;
          const expectY = 540 - Number(hField.value) / 2;
          // Moves the overlay by a source-pixel delta, converting it to a
          // client-space drag distance from the rect element's OWN current
          // size against its known source width -- robust to however the
          // video is actually laid out inside the stage, rather than
          // recomputing that layout by hand. Reuses pointerId 1, same as the
          // 356 drag above: a fresh custom id is never recognised as an
          // active pointer by setPointerCapture, only the mouse's own is.
          // renderRects() also rebuilds '#ve-rects' from scratch on every
          // drag, so the rect element is re-queried fresh each call rather
          // than reused from an earlier, by-then-detached one.
          const dragBy = (dxSource, dySource, altKey) => {
            const el = document.querySelector('#ve-rects .ve-rect-image');
            const box = el.getBoundingClientRect();
            const scale = box.width / Number(wField.value);
            const from = { x: box.left + box.width / 2, y: box.top + box.height / 2 };
            const to = { x: from.x + dxSource * scale, y: from.y + dySource * scale };
            el.dispatchEvent(new PointerEvent('pointerdown', { clientX: from.x, clientY: from.y, pointerId: 1, bubbles: true }));
            document.getElementById('ve-rects').dispatchEvent(new PointerEvent('pointermove', { clientX: to.x, clientY: to.y, pointerId: 1, altKey, bubbles: true }));
            document.getElementById('ve-rects').dispatchEvent(new PointerEvent('pointerup', { clientX: to.x, clientY: to.y, pointerId: 1, altKey, bubbles: true }));
          };
          // Move it well away from the centre first, Alt held so this setup
          // move does not get pulled onto a snap line itself.
          dragBy(-300, 200, true);
          await sleep(30);
          if (Number(xField.value) === expectX && Number(yField.value) === expectY) {
            errs.push('393: the Alt-held setup drag left the overlay exactly on the frame centre, cannot prove the next drag snaps it there');
          }
          // Aim to within a source pixel or two of the exact centre -- not
          // exactly on it -- and expect the snap to land it exactly there.
          // The fixed 1920x1080 fixture puts the frame centre at source
          // (960, 540); the tolerance is always well above a couple of
          // source pixels here since the preview is always shown well below
          // its native size.
          let curX = Number(xField.value) + Number(wField.value) / 2;
          let curY = Number(yField.value) + Number(hField.value) / 2;
          dragBy(960 + 1 - curX, 540 - 1 - curY, false);
          await sleep(30);
          if (Number(xField.value) !== expectX || Number(yField.value) !== expectY) {
            errs.push('393: dragging near the frame centre did not snap onto it exactly, got ' +
              xField.value + ',' + yField.value + ' expected ' + expectX + ',' + expectY);
          }
          // The same near-centre approach with Alt held must not snap. The
          // overlay is already sitting exactly on the centre from the drag
          // above, so this nudges it 4 source pixels off -- an even number,
          // so the drag's own even-pixel rounding cannot coincidentally land
          // back on the same value the snap itself would have produced.
          curX = Number(xField.value) + Number(wField.value) / 2;
          curY = Number(yField.value) + Number(hField.value) / 2;
          dragBy(960 + 4 - curX, 540 - 4 - curY, true);
          await sleep(30);
          if (Number(xField.value) === expectX && Number(yField.value) === expectY) {
            errs.push('393: holding Alt during the drag did not suppress the centre snap');
          }
        }
      }
    }

    // ---- 383: the video editor's other half of the zoom-key and
    // click-to-select-handles parity check, screenshot editor side in
    // smoke_test.py's own '383:' block above ----
    {
      const zoomValVe = document.getElementById('ve-preview-zoom-val');
      if (!zoomValVe) {
        errs.push('383: video editor preview has no zoom readout to check');
      } else {
        const readVePct = () => Number(zoomValVe.textContent.replace('%', ''));
        const startPct = readVePct(); // reset to 100 by the 365 check just above
        document.dispatchEvent(new KeyboardEvent('keydown', { key: '+', ctrlKey: true, bubbles: true }));
        await sleep(30);
        const inPct = readVePct();
        if (inPct <= startPct) {
          errs.push('383: Ctrl+Plus did not zoom the video editor preview in (was ' + startPct + '%, now ' + inPct + '%)');
        }
        document.dispatchEvent(new KeyboardEvent('keydown', { key: '-', ctrlKey: true, bubbles: true }));
        await sleep(30);
        document.dispatchEvent(new KeyboardEvent('keydown', { key: '-', ctrlKey: true, bubbles: true }));
        await sleep(30);
        const outPct = readVePct();
        if (outPct >= inPct) {
          errs.push('383: Ctrl+Minus did not zoom the video editor preview out (was ' + inPct + '%, now ' + outPct + '%)');
        }
        document.dispatchEvent(new KeyboardEvent('keydown', { key: '0', ctrlKey: true, bubbles: true }));
        await sleep(30);
        const fitPctVe = readVePct();
        if (fitPctVe !== startPct) {
          errs.push('383: Ctrl+0 did not reset the video editor preview to its starting ' + startPct + '% (got ' + fitPctVe + '%)');
        }
      }

      // 473: zoomed in, every corner of the picture can be panned up to the
      // middle of the stage instead of stopping at the stage border.
      if (zoomValVe) {
        const readPct473 = () => Number(zoomValVe.textContent.replace('%', ''));
        const stageVe = document.getElementById('ve-stage');
        for (let i = 0; i < 20 && readPct473() < 400; i++) {
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '+', ctrlKey: true, bubbles: true }));
          await sleep(20);
        }
        const v = document.getElementById('ve-video');
        if (readPct473() <= 100 || !v || !v.videoWidth) {
          // No decoded video in this harness (or the zoom keys never reached
          // above 100%): nothing to measure a picture box against, so this
          // check is skipped rather than faked.
        } else {
          const pan = (dx, dy) => {
            stageVe.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: 0, clientY: 0 }));
            stageVe.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, button: 0, pointerId: 1, clientX: dx, clientY: dy }));
            stageVe.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, pointerId: 1, clientX: dx, clientY: dy }));
          };
          const pictureBox = () => {
            const videoRect = v.getBoundingClientRect();
            const stageRect = stageVe.getBoundingClientRect();
            const zoom = videoRect.width / stageRect.width;
            const s = Math.min(stageRect.width / v.videoWidth, stageRect.height / v.videoHeight);
            const picW = v.videoWidth * s * zoom, picH = v.videoHeight * s * zoom;
            const vcx = videoRect.left + videoRect.width / 2, vcy = videoRect.top + videoRect.height / 2;
            return { left: vcx - picW / 2, top: vcy - picH / 2, right: vcx + picW / 2, bottom: vcy + picH / 2, stageRect };
          };
          pan(5000, 5000);
          await sleep(30);
          {
            const { left, top, stageRect } = pictureBox();
            const scx = stageRect.left + stageRect.width / 2, scy = stageRect.top + stageRect.height / 2;
            if (Math.abs(left - scx) > 2) errs.push('473: the picture left edge cannot be panned to the middle of the stage');
            if (Math.abs(top - scy) > 2) errs.push('473: the picture top edge cannot be panned to the middle of the stage');
          }
          pan(-5000, -5000);
          await sleep(30);
          {
            const { right, bottom, stageRect } = pictureBox();
            const scx = stageRect.left + stageRect.width / 2, scy = stageRect.top + stageRect.height / 2;
            if (Math.abs(right - scx) > 2) errs.push('473: the picture right edge cannot be panned to the middle of the stage');
            if (Math.abs(bottom - scy) > 2) errs.push('473: the picture bottom edge cannot be panned to the middle of the stage');
          }
          document.getElementById('ve-preview-zoom-reset')?.click();
          await sleep(30);
          if (v.style.transform !== '') {
            errs.push('473: resetting the preview zoom left a stale transform on the video element');
          }
        }
      }

      // The video editor selects an overlay from its own list rather than by
      // clicking its canvas rectangle -- renderRects() only ever draws the
      // rect and handles of the overlay already selected, so there is
      // nothing on the canvas to click before that. Selecting the pixelate
      // overlay (the same rectangular-region tool the screenshot editor
      // unified the wording for) must still show the same 8 handles.
      // Earlier edits above (355/356: the text and image overlays each got a
      // field or drag commit) replace-and-reappend the edited op, so the list
      // no longer sits in its original add order -- find the pixelate row by
      // its own label rather than assuming overlayKinds' add-time position.
      const ovRows383 = [...document.querySelectorAll('#ve-ov-list .ve-ov-row')];
      const pixelRow = ovRows383.find((r) => r.textContent.startsWith('Pixelate'));
      if (!pixelRow) {
        errs.push('383: could not find the pixelate overlay row to select for the handle count');
      } else {
        pixelRow.click();
        await sleep(30);
        const veHandles = document.querySelectorAll('#ve-rects .ve-rect-pixelate .ve-rect-handle');
        if (veHandles.length !== 8) {
          errs.push('383: selecting the pixelate overlay showed ' + veHandles.length + ' handles, expected 8');
        }
      }
    }

    // ---- 395: with an overlay selected, arrow keys move it (by numFieldStep,
    // default 1 -- see tests/ui/tauri_stub.js), Ctrl+Arrow resizes it instead,
    // comma/period take over frame-stepping, and Escape gives the arrow keys
    // back to the playhead. The pixelate overlay is still selected from the
    // 383 check just above -- being a rect-based overlay its x/y/w/h must
    // stay on even numbers (see snapRect() in video.js), so each press here
    // moves it by 2 (the nearest even number to the default step of 1), not
    // by the step itself; a text overlay has no such rect and moves by the
    // exact step, which is covered by the screenshot editor's own 395 check. ----
    {
      const xField395 = document.querySelector('#ve-ov-editor [data-ov-field="rect.x"]');
      const wField395 = document.querySelector('#ve-ov-editor [data-ov-field="rect.w"]');
      const hintEl = document.getElementById('ve-nudge-hint');
      if (!xField395 || !wField395 || !hintEl) {
        errs.push('395: could not find the selected overlay rect fields or the nudge hint element');
      } else {
        if (hintEl.classList.contains('hidden')) {
          errs.push('395: the nudge hint is not shown while an overlay is selected');
        }
        const beforeX = Number(xField395.value);
        for (let i = 0; i < 4; i++) {
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
          await sleep(20);
        }
        const afterX = Number(document.querySelector('#ve-ov-editor [data-ov-field="rect.x"]').value);
        if (afterX !== beforeX + 8) {
          errs.push('395: four ArrowRight presses moved the selected overlay by ' + (afterX - beforeX) + ', expected exactly 8 (2 per press, rounded up to stay even)');
        }
        const beforeW = Number(document.querySelector('#ve-ov-editor [data-ov-field="rect.w"]').value);
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', ctrlKey: true, bubbles: true, cancelable: true }));
        await sleep(20);
        const afterW = Number(document.querySelector('#ve-ov-editor [data-ov-field="rect.w"]').value);
        if (afterW <= beforeW) {
          errs.push('395: Ctrl+ArrowRight did not grow the selected overlay instead of moving it (was ' + beforeW + ', now ' + afterW + ')');
        }
        const beforeFrame = document.getElementById('ve-frame').textContent;
        document.dispatchEvent(new KeyboardEvent('keydown', { key: '.', bubbles: true, cancelable: true }));
        await sleep(30);
        const afterFrame = document.getElementById('ve-frame').textContent;
        if (afterFrame === beforeFrame) {
          errs.push('395: period did not step a frame forward while an overlay is selected');
        }
        // Escape deselects and hands the arrow keys back to frame-stepping,
        // rather than also popping the close-window confirm dialog.
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
        await sleep(30);
        if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
          errs.push('395: Escape while an overlay was selected also opened the close-window confirm dialog');
        }
        if (!hintEl.classList.contains('hidden')) {
          errs.push('395: the nudge hint stayed visible after Escape deselected the overlay');
        }
        const frameAfterDeselect = document.getElementById('ve-frame').textContent;
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
        await sleep(30);
        const frameAfterArrow = document.getElementById('ve-frame').textContent;
        if (frameAfterArrow === frameAfterDeselect) {
          errs.push('395: ArrowRight did not move the playhead once nothing was selected');
        }
      }
    }

    // ---- 398: the show-original button (and V) hide the overlay and edit-
    // handle layers so the live preview compares against the untouched source
    // (fades and the export scale target never render live in the first
    // place, so hiding these two is the whole of it here) -- a quick click
    // locks it on until clicked again, holding it only peeks for as long as
    // it stays held, and a badge marks either state. ----
    {
      const origBtn398 = document.getElementById('ve-original');
      const origBadge398 = document.getElementById('ve-original-badge');
      const overlayLive398 = document.getElementById('ve-overlay-live');
      const rects398 = document.getElementById('ve-rects');
      if (!origBtn398 || !origBadge398 || !overlayLive398 || !rects398) {
        errs.push('398: the video editor is missing the show-original button, badge or overlay/rects layers');
      } else {
        if (overlayLive398.classList.contains('hidden') || rects398.classList.contains('hidden')) {
          errs.push('398: the overlay or rects layer starts hidden before the show-original check even runs');
        }
        origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 4 }));
        origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 4 }));
        await sleep(30);
        if (!overlayLive398.classList.contains('hidden') || !rects398.classList.contains('hidden')) {
          errs.push('398: a quick click on the show-original button did not hide the overlay and rects layers');
        }
        if (origBadge398.classList.contains('hidden')) errs.push('398: the original badge did not appear once the original was locked on');
        if (!origBtn398.classList.contains('active')) errs.push('398: the show-original button did not switch to its active state while locked on');
        origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 4 }));
        origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 4 }));
        await sleep(30);
        if (overlayLive398.classList.contains('hidden') || rects398.classList.contains('hidden')) {
          errs.push('398: a second click did not lock the original back off');
        }
        if (!origBadge398.classList.contains('hidden')) errs.push('398: the original badge stayed after the second click turned it off');
        origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 4 }));
        await sleep(450);
        if (!overlayLive398.classList.contains('hidden')) errs.push('398: holding the show-original button down did not hide the overlay layer');
        origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 4 }));
        await sleep(30);
        if (overlayLive398.classList.contains('hidden')) errs.push('398: releasing a held show-original press did not restore the overlay layer');
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'v', bubbles: true }));
        await sleep(450);
        if (!overlayLive398.classList.contains('hidden')) errs.push('398: holding V did not hide the overlay layer');
        document.dispatchEvent(new KeyboardEvent('keyup', { key: 'v', bubbles: true }));
        await sleep(30);
        if (overlayLive398.classList.contains('hidden')) errs.push('398: releasing V did not restore the overlay layer');
      }
    }

    // Both 358 probes below depend on tick()'s own requestAnimationFrame loop
    // actually firing across several real sleeps -- headless Chromium stops
    // producing frames once the page is quiet (see the heartbeat comment
    // earlier in this file), which would otherwise starve tick() of any
    // chance to run during the waits below. Same fix: keep something dirty.
    const beat358 = setInterval(() => {
      document.documentElement.style.setProperty('--smoke-tick-358', String(Date.now()));
      document.body.getBoundingClientRect();
    }, 20);

    // ---- 358: the live-preview element for an overlay is REUSED across
    // renders, not rebuilt (a fresh <img>/<canvas> every frame was the wear
    // pattern behind "the preview works at first and then stops") ----
    {
      const img0 = document.querySelector('#ve-overlay-live .ve-live-image');
      if (!img0) {
        errs.push('358: no live image element to check for node reuse');
      } else {
        img0._probeMarker = 'kept';
        for (let i = 0; i < 5; i++) {
          document.dispatchEvent(new KeyboardEvent('keydown', { key: i % 2 ? 'Home' : 'End', bubbles: true }));
          await sleep(30);
        }
        const img1 = document.querySelector('#ve-overlay-live .ve-live-image');
        if (img1 !== img0 || img1._probeMarker !== 'kept') {
          errs.push('358: the live image overlay element was rebuilt instead of reused across renders');
        }
        const allImages = document.querySelectorAll('#ve-overlay-live .ve-live-image');
        if (allImages.length !== 1) {
          errs.push('358: expected exactly one live image element, found ' + allImages.length);
        }
      }
    }

    // ---- 358: a throw inside tick() must be caught once (toasted) and must
    // not stop the frame loop from correcting the playhead on a later tick ----
    {
      const waitFor358 = async (fn) => { for (let i = 0; i < 20 && !fn(); i++) await sleep(50); return fn(); };
      if (document.activeElement) document.activeElement.blur();
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }));
      await waitFor358(() => document.getElementById('ve-time').textContent === '10:00.000');
      if (document.getElementById('ve-time').textContent !== '10:00.000') {
        errs.push('358: setup: End did not seek to the end of the clip');
      }
      const videoElD = document.getElementById('ve-video');
      const playheadEl = document.getElementById('ve-playhead');
      const realId = playheadEl.id;
      playheadEl.id = 've-playhead-broken-for-test';
      // The stub video mirrors any currentTime assignment back immediately, so
      // going through seek() (which sets VE.playhead to the same value at the
      // same time) never leaves tick() anything to correct. Desyncing
      // currentTime directly, bypassing seek(), is what actually forces the
      // next tick() into its mismatch-correction branch -- the one that
      // touches the (now-broken) #ve-playhead node and should throw.
      videoElD.currentTime = 0;
      const toastEl = document.getElementById('ve-toast');
      await waitFor358(() => !toastEl.classList.contains('hidden') && !!toastEl.textContent);
      if (toastEl.classList.contains('hidden') || !toastEl.textContent || toastEl.textContent === 'veTickError') {
        errs.push('358: a failing tick() did not report itself once via the toast');
      }
      playheadEl.id = realId;
      // Desync again so a later, now-healthy tick has something to correct.
      videoElD.currentTime = 5;
      await waitFor358(() => document.getElementById('ve-time').textContent === '00:05.000');
      if (document.getElementById('ve-time').textContent !== '00:05.000') {
        errs.push('358: the frame loop did not keep correcting the playhead on a later, healthy tick');
      }
    }
    clearInterval(beat358);

    // ---- 359: a replacing op's own _prev undo chain must be capped at
    // undoMax links, not grow forever with every re-edit of the same overlay
    // (editing one overlay 500 times used to keep 500 linked copies alive) ----
    {
      const frameRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-frame');
      let opacityField = null;
      if (ovRows[frameRowIdx]) {
        ovRows[frameRowIdx].click();
        await sleep(30);
        opacityField = document.querySelector('#ve-ov-editor [data-ov-field="opacity"]');
      }
      if (!opacityField) {
        errs.push('359: could not find the frame overlay opacity field to drive 500 commits through');
      } else {
        for (let i = 0; i < 500; i++) {
          opacityField.value = String(10 + (i % 80));
          opacityField.dispatchEvent(new Event('change'));
        }
        await sleep(30);
        const rowsBefore = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
        document.activeElement && document.activeElement.blur();
        // undoMax defaults to 40 (UNDO_MAX_DEFAULT, unset by this fixture): the
        // frame overlay's chain must be exhausted at exactly that many undos,
        // never sooner and never later than the SINGLE row it owns disappearing.
        for (let i = 0; i < 40; i++) {
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, bubbles: true }));
        }
        await sleep(30);
        const rowsAfter = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
        if (rowsAfter !== rowsBefore - 1) {
          errs.push('359: the _prev undo chain was not capped at undoMax -- expected ' + (rowsBefore - 1) +
            ' op row(s) after 40 undos into 500 commits, found ' + rowsAfter);
        }
      }
    }

    // ---- 392: duplicate an overlay from its row context menu; undo removes
    // only the duplicate, nothing else in the list ----
    {
      const rowsDup = [...document.querySelectorAll('#ve-ov-list .ve-ov-row')];
      const pixelRowDup = rowsDup.find((r) => r.textContent.startsWith('Pixelate'));
      if (!pixelRowDup) {
        errs.push('392: could not find the pixelate overlay row to duplicate');
      } else {
        const beforeCount = rowsDup.length;
        const box = pixelRowDup.getBoundingClientRect();
        pixelRowDup.dispatchEvent(new MouseEvent('contextmenu', {
          bubbles: true, clientX: box.left + 4, clientY: box.top + 4,
        }));
        await sleep(30);
        const ctxMenu = document.querySelector('.ctx');
        const ctxBtns = ctxMenu ? [...ctxMenu.querySelectorAll('button')] : [];
        if (!ctxMenu || ctxMenu.classList.contains('hidden') || ctxBtns.length !== 2) {
          errs.push('392: right-clicking an overlay row did not open a two-action context menu');
        } else {
          ctxBtns[0].click();
          await sleep(30);
          const afterCount = document.querySelectorAll('#ve-ov-list .ve-ov-row').length;
          if (afterCount !== beforeCount + 1) {
            errs.push('392: duplicating an overlay produced ' + afterCount + ' rows, expected ' + (beforeCount + 1));
          }
          if (document.activeElement) document.activeElement.blur();
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, bubbles: true }));
          await sleep(30);
          const undoCount = document.querySelectorAll('#ve-ov-list .ve-ov-row').length;
          if (undoCount !== beforeCount) {
            errs.push('392: undo after duplicating left ' + undoCount + ' row(s), expected exactly ' + beforeCount +
              ' (undo must remove only the duplicate)');
          }
        }
      }
    }

    // ---- 416: "Add a video" as a section clip updates the timeline scale,
    // and an unreadable extra source is rejected before it reaches the list ----
    {
      const beforeText = document.getElementById('ve-cut-count').textContent;
      document.getElementById('ve-add-video').click();
      await sleep(30);
      const pickPop = document.getElementById('ve-add-video-pop');
      if (!pickPop || pickPop.classList.contains('hidden') || pickPop.querySelectorAll('button').length !== 2) {
        errs.push('416: "Add a video" did not open the overlay/section choice');
      } else {
        pickPop.querySelectorAll('button')[1].click(); // "As its own section"
        await sleep(30);
        const posButtons = pickPop.querySelectorAll('button');
        if (pickPop.classList.contains('hidden') || posButtons.length !== 3) {
          errs.push('416: choosing "as its own section" did not open the start/end/current-position choice');
        } else {
          posButtons[1].click(); // "At the end"
          await sleep(30);
          const clipList = document.getElementById('ve-clip-list');
          const clipRow = clipList ? clipList.querySelector('.ve-op-row') : null;
          if (!clipRow || clipList.classList.contains('hidden')) {
            errs.push('416: the inserted section did not appear in the clip list');
          } else if (!/ps-smoke-badge/.test(clipRow.textContent)) {
            errs.push('416: the clip row did not show the name of the inserted file: ' + clipRow.textContent);
          }
          const afterText = document.getElementById('ve-cut-count').textContent;
          if (afterText === beforeText) {
            errs.push('416: the total-length readout did not change once a section was inserted (timeline scale not updated)');
          }
          // Clean up so later probes see the plain single-source state again.
          const del = clipRow ? clipRow.querySelector('.ve-op-del') : null;
          if (del) { del.click(); await sleep(30); }
        }
      }
    }
    {
      window.__EXTRA_VIDEO_FAIL__ = true;
      const beforeCount = document.querySelectorAll('#ve-clip-list .ve-op-row').length;
      document.getElementById('ve-add-video').click();
      await sleep(30);
      window.__EXTRA_VIDEO_FAIL__ = false;
      const toastEl = document.getElementById('ve-toast');
      const pickPop = document.getElementById('ve-add-video-pop');
      const popOpened = pickPop && !pickPop.classList.contains('hidden') && pickPop.querySelectorAll('button').length;
      if (popOpened) {
        errs.push('416: an unreadable extra video source still opened the overlay/section choice');
      }
      if (!toastEl || toastEl.classList.contains('hidden') || toastEl.textContent.includes('ffErrUnreadable')) {
        errs.push('416: an unreadable extra video source was not rejected with plain-language wording, got: ' +
          (toastEl ? toastEl.textContent : '(no toast)'));
      }
      const afterCount = document.querySelectorAll('#ve-clip-list .ve-op-row').length;
      if (afterCount !== beforeCount) {
        errs.push('416: an unreadable extra video source was added to the clip list anyway');
      }
    }

    // ---- 392: the last colour picked for a text overlay is remembered as the
    // starting colour for the next text overlay, and veOvRemember off falls
    // back to the fixed default again ----
    {
      const rowsRem = [...document.querySelectorAll('#ve-ov-list .ve-ov-row')];
      const textRowRem = rowsRem.find((r) => r.textContent.startsWith('Text'));
      if (!textRowRem) {
        errs.push('392: could not find the text overlay row to test the remembered colour');
      } else {
        textRowRem.click();
        await sleep(30);
        const pick = document.querySelector('#ve-ov-editor .swatch[title="#ef4444"]');
        if (!pick) {
          errs.push('392: no red swatch to pick for the remembered-colour check');
        } else {
          pick.click();
          await sleep(30);
          const afterPick = await window.__TAURI__.core.invoke('get_settings');
          if ((afterPick.ui_texts || {}).veOvLastTextColor !== '#ef4444') {
            errs.push('392: picking a text overlay colour did not remember it as veOvLastTextColor');
          }
          // The new overlay is already selected by addOverlay() itself -- clicking
          // its row again would TOGGLE the selection back off (ve-ov-row's own
          // click handler flips VE.selOv when it already matches).
          document.getElementById('ve-ov-add-text').click();
          await sleep(30);
          const selSwatch = document.querySelector('#ve-ov-editor .swatch.sel');
          if (!selSwatch || selSwatch.title !== '#ef4444') {
            errs.push('392: a new text overlay did not start with the remembered colour, got ' +
              (selSwatch && selSwatch.title));
          }
          await window.__TAURI__.core.invoke('set_ui_flag', { key: 'veOvRemember', enabled: false });
          window.__FIRE__('settings-changed');
          await sleep(50);
          document.getElementById('ve-ov-add-text').click();
          await sleep(30);
          const selSwatchOff = document.querySelector('#ve-ov-editor .swatch.sel');
          if (selSwatchOff && selSwatchOff.title === '#ef4444') {
            errs.push('392: veOvRemember off still started a new text overlay from the remembered colour');
          }
          await window.__TAURI__.core.invoke('set_ui_flag', { key: 'veOvRemember', enabled: true });
          window.__FIRE__('settings-changed');
          await sleep(30);
        }
      }
    }

    document.getElementById('ve-tab-export').click();
    await sleep(80);
    if (document.getElementById('ve-panel-export').classList.contains('hidden')) {
      errs.push('the Export tab did not reveal its panel');
    }
    rawKeyCheck('export tab');
    sideScan(errs, 'export tab');

    // ---- 366: free width/height/percent scale fields, lock-aspect,
    // snap-to-even, the fixed-height anti-upscale guard, the Export tab's
    // read-only summary + "Change..." button, and undo ----
    document.getElementById('ve-tab-edit').click();
    await sleep(80);
    {
      const wField = document.getElementById('ve-scale-w');
      const hField = document.getElementById('ve-scale-h');
      const lockBox = document.getElementById('ve-scale-lock');
      const opsBefore = document.querySelectorAll('#ve-ops-list .ve-op-row').length;

      if (!lockBox.checked) errs.push('366: aspect lock is not on by default');
      // 1280x720 on the 1920x1080 fixture, lock on -- height must follow.
      wField.value = '1280';
      wField.dispatchEvent(new Event('change'));
      await sleep(30);
      if (wField.value !== '1280' || hField.value !== '720') {
        errs.push('366: locked width change did not carry height to 720, got ' + wField.value + 'x' + hField.value);
      }
      const rowsAfterScale = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
      if (rowsAfterScale !== opsBefore + 1) {
        errs.push('366: setting a scale did not add exactly one op row');
      }
      const scaleRow = [...document.querySelectorAll('#ve-ops-list .ve-op-row')].pop();
      if (!scaleRow || !scaleRow.textContent.includes('1280') || !scaleRow.textContent.includes('720')) {
        errs.push('366: the op row does not show the 1280x720 target: ' + (scaleRow && scaleRow.textContent));
      }

      // Unlock: a width change must leave the height field alone.
      lockBox.checked = false;
      lockBox.dispatchEvent(new Event('change'));
      await sleep(30);
      wField.value = '800';
      wField.dispatchEvent(new Event('change'));
      await sleep(30);
      if (hField.value !== '720') {
        errs.push('366: an unlocked width change moved the height field, got ' + hField.value);
      }

      // An odd width always snaps to an even number.
      wField.value = '801';
      wField.dispatchEvent(new Event('change'));
      await sleep(30);
      if (Number(wField.value) % 2 !== 0) {
        errs.push('366: an odd width was not snapped to an even number, got ' + wField.value);
      }

      // Re-lock, then the 25% preset must set both numbers and mark itself selected.
      lockBox.checked = true;
      lockBox.dispatchEvent(new Event('change'));
      await sleep(30);
      const presetBtns = [...document.querySelectorAll('#ve-scale-presets button')];
      const pct25 = presetBtns.find((b) => b.dataset.key === '25');
      if (!pct25) {
        errs.push('366: no 25% preset button found');
      } else {
        pct25.click();
        await sleep(30);
        if (wField.value !== '480' || hField.value !== '270') {
          errs.push('366: the 25% preset did not set 480x270, got ' + wField.value + 'x' + hField.value);
        }
        if (!pct25.classList.contains('sel')) {
          errs.push('366: the 25% preset button is not marked selected after being picked');
        }
      }

      // The Export tab mirrors the same target as a read-only summary, and
      // its "Change..." button jumps back to the Edit tab.
      document.getElementById('ve-tab-export').click();
      await sleep(80);
      const summary = document.getElementById('ve-export-scale-summary').textContent;
      if (!summary.includes('480') || !summary.includes('270')) {
        errs.push('366: the export-tab scale summary does not show 480x270: ' + summary);
      }
      document.getElementById('ve-export-scale-edit').click();
      await sleep(80);
      if (document.getElementById('ve-panel-edit').classList.contains('hidden')) {
        errs.push('366: the export-tab "Change..." button did not jump back to the Edit tab');
      }

      // A fixed-height preset at or above the source size leaves the frame
      // untouched instead of upscaling it (the source here is 1080p).
      const rowsBeforeGuard = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
      const preset1080 = presetBtns.find((b) => b.dataset.key === '1080');
      if (!preset1080) {
        errs.push('366: no 1080p preset button found');
      } else {
        preset1080.click();
        await sleep(30);
        const rowsAfterGuard = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
        if (rowsAfterGuard !== rowsBeforeGuard - 1) {
          errs.push('366: picking the 1080p preset on a 1080p source did not clear the scale op (anti-upscale guard)');
        }
        if (wField.value !== '1920' || hField.value !== '1080') {
          errs.push('366: after the guard clears the scale, the fields do not show the source size, got ' + wField.value + 'x' + hField.value);
        }
      }

      // Undo restores the original resolution.
      wField.value = '1280';
      wField.dispatchEvent(new Event('change'));
      await sleep(30);
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, bubbles: true }));
      await sleep(30);
      if (wField.value !== '1920' || hField.value !== '1080') {
        errs.push('366: undo did not restore the original resolution, got ' + wField.value + 'x' + hField.value);
      }
    }

    // ---- 335: the export destination input + its two picker buttons ----
    const destInput = document.getElementById('ve-export-dest');
    if (!destInput.value) {
      errs.push('the export destination field starts empty');
    }
    destInput.value = 'C:\\\\Custom\\\\my-clip.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);
    if (destInput.value !== 'C:\\\\Custom\\\\my-clip.mp4') {
      errs.push('typing a destination path directly did not stick as an override');
    }
    const pickedFile = destInput.value;
    document.getElementById('ve-export-choose-folder').click();
    await sleep(80);
    if (destInput.value === pickedFile) {
      errs.push('"Export to..." did not change the destination after picking a folder');
    }
    if (!destInput.value.startsWith('C:\\\\Videos\\\\Exports')) {
      errs.push('"Export to..." did not build the path from the picked folder');
    }

    // ---- the plain "File name" field, separate from the full path above --
    // a valid custom name flows straight into the export call, an invalid
    // one shows an inline hint and disables Export, but leaves Copy to
    // clipboard alone since that one never reads this field ----
    const nameInput = document.getElementById('ve-export-name');
    const nameHint = document.getElementById('ve-export-name-hint');
    nameInput.value = 'Demo 1';
    nameInput.dispatchEvent(new Event('input'));
    await sleep(30);
    if (!destInput.value.endsWith('Demo 1.mp4')) {
      errs.push('typing "Demo 1" into the file-name field did not update the export destination: ' + destInput.value);
    }
    if (!nameHint.classList.contains('hidden')) {
      errs.push('a valid file name still shows the invalid-name hint');
    }
    window.__SMOKE_ARGS__ = [];
    document.getElementById('ve-export-start').click();
    await sleep(150);
    const nameExportCall = (window.__SMOKE_ARGS__ || []).find(([cmd]) => cmd === 'export_video');
    if (!nameExportCall || !String(nameExportCall[1].req.dest_path).endsWith('Demo 1.mp4')) {
      errs.push('export_video was not called with the typed file name');
    }
    nameInput.value = 'a/b';
    nameInput.dispatchEvent(new Event('input'));
    await sleep(30);
    if (nameHint.classList.contains('hidden')) {
      errs.push('an invalid file name ("a/b") does not show the inline hint');
    }
    if (!nameInput.classList.contains('invalid')) {
      errs.push('an invalid file name does not mark the field itself invalid');
    }
    if (!document.getElementById('ve-export-start').disabled) {
      errs.push('an invalid file name does not disable Export');
    }
    // Copy to clipboard writes to its own throwaway temp name and never
    // reads this field, so an invalid name here must not block it.
    if (document.getElementById('ve-export-clip').disabled) {
      errs.push('an invalid file name wrongly disables Copy to clipboard');
    }
    // Leave the field valid again so the probe steps below are not blocked
    // by this one's own leftover invalid state.
    nameInput.value = 'ps-smoke-recording-edit';
    nameInput.dispatchEvent(new Event('input'));
    await sleep(30);

    // ---- 503: a destination the backend does not recognize (e.g. a file
    // opened from outside the app) opens the export-folder picker and
    // retries once instead of failing outright ----
    window.__EXPORT_DEST_REFUSED_ONCE__ = false;
    window.__SMOKE_CALLS__ = [];
    destInput.value = 'C:\\\\Outside\\\\REFUSEDEST-clip.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);
    window.__PICK_FOLDER__ = 'C:\\\\Retry\\\\Folder';
    document.getElementById('ve-export-start').click();
    await sleep(250);
    if (!window.__SMOKE_CALLS__.includes('pick_export_folder')) {
      errs.push('503: a refused export destination did not open the export-folder picker');
    }
    const retryStatus = document.getElementById('ve-export-status').textContent;
    if (!retryStatus.includes('C:\\\\Retry\\\\Folder')) {
      errs.push(`503: the retried export did not land in the picked folder, status reads ${JSON.stringify(retryStatus)}`);
    }
    if (document.getElementById('ve-export-saveas').classList.contains('hidden')) {
      errs.push('503: the retried export did not complete successfully');
    }
    // The one control that acts on a finished export reads "Save as..." (it moves
    // the file) with a tooltip, and the old pre-export file picker is gone: the name
    // field plus "Export to..." cover it. The row reads like the screenshot editor's:
    // Cancel, Save as, Copy, then the primary Export; Cancel is always there.
    {
      const moveBtn = document.getElementById('ve-export-saveas');
      const moveText = moveBtn.textContent.trim();
      if (moveText !== 'Save as…') errs.push('547: the post-export button reads ' + JSON.stringify(moveText) + ', expected Save as...');
      if (!moveBtn.title || !moveBtn.title.includes('exported')) errs.push('547: the Save as button has no tooltip: ' + JSON.stringify(moveBtn.title));
      if (document.getElementById('ve-export-choose-file')) errs.push('547: the pre-export Save as button still exists');
      const saveAsLike = [...document.querySelectorAll('#ve-panel-export button')].filter((b) => /Save as|Speichern unter/.test(b.textContent) && b.id !== 've-export-saveas');
      if (saveAsLike.length) errs.push('547: a Save as button is still in the export panel: ' + saveAsLike.map((b) => b.id).join(','));
      const rowIds = [...document.getElementById('ve-export-cancel').parentElement.querySelectorAll('button')].map((b) => b.id).join(',');
      if (rowIds !== 've-export-cancel,ve-export-saveas,ve-export-clip,ve-export-start') errs.push('673: the export row order is ' + rowIds);
      if (document.getElementById('ve-export-cancel').classList.contains('hidden')) errs.push('673: Cancel is hidden although no export runs');
      if (document.getElementById('ve-export-clip').textContent.trim() !== 'Copy') errs.push('673: the clipboard button reads ' + JSON.stringify(document.getElementById('ve-export-clip').textContent.trim()));
    }
    window.__PICK_FOLDER__ = null;

    // ---- 503: cancelling that picker is a plain no-op -- no export, no error ----
    window.__EXPORT_DEST_REFUSED_ONCE__ = false;
    destInput.value = 'C:\\\\Outside\\\\REFUSEDEST-cancel.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);
    window.__PICK_FOLDER__ = null; // cancel
    window.__SMOKE_CALLS__ = [];
    document.getElementById('ve-export-start').click();
    await sleep(250);
    if (!window.__SMOKE_CALLS__.includes('pick_export_folder')) {
      errs.push('503: cancelling the retry picker was never even offered');
    }
    if (document.getElementById('ve-export-status').textContent.toLowerCase().includes('not recognized')) {
      errs.push('503: a cancelled retry showed the raw backend refusal instead of staying silent');
    }
    destInput.value = 'C:\\\\Custom\\\\my-clip.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);

    const formatSel = document.getElementById('ve-export-format');
    // 508: the export format/quality selects, themed across every built-in
    // theme -- a bare select used to fall back to the plain OS look.
    {
      const priorTheme = document.documentElement.getAttribute('data-theme');
      const BUILTIN_THEMES508 = ['light', 'dark', 'programmer', 'neon', 'cyberpunk', 'mono', 'ocean'];
      const swatch = document.createElement('div');
      swatch.style.cssText = 'position:absolute;left:-9999px;top:-9999px;background-color:var(--surface)';
      document.body.appendChild(swatch);
      const qSel508 = document.getElementById('ve-export-quality');
      for (const theme of BUILTIN_THEMES508) {
        document.documentElement.setAttribute('data-theme', theme);
        const surfaceRgb = getComputedStyle(swatch).backgroundColor;
        for (const sel of [formatSel, qSel508]) {
          if (!sel) continue;
          const label = sel.id;
          const cs = getComputedStyle(sel);
          if (parseFloat(cs.borderTopWidth) <= 0) errs.push('508: ' + label + ' has no border in theme ' + theme);
          if (cs.backgroundColor === 'rgba(0, 0, 0, 0)' || cs.backgroundColor === 'transparent') {
            errs.push('508: ' + label + ' has a transparent background in theme ' + theme);
          }
          // A selected option shows the accent colour (option:checked); pick
          // an unselected one to check the plain surface colour instead.
          const unselected = [...sel.options].find((o) => !o.selected);
          if (unselected) {
            const optBg = getComputedStyle(unselected).backgroundColor;
            if (optBg !== surfaceRgb) {
              errs.push('508: ' + label + ' option background is ' + optBg + ', expected ' + surfaceRgb + ' in theme ' + theme);
            }
          }
        }
      }
      swatch.remove();
      if (priorTheme) document.documentElement.setAttribute('data-theme', priorTheme);
      else document.documentElement.removeAttribute('data-theme');
    }
    const otherOpt = formatSel && [...formatSel.options].find((o) => o.value !== formatSel.value);
    if (otherOpt) {
      const beforeFormatChange = destInput.value;
      formatSel.value = otherOpt.value;
      formatSel.dispatchEvent(new Event('change'));
      await sleep(30);
      if (destInput.value === beforeFormatChange) {
        errs.push('changing the export format did not clear the manual destination override');
      }
    }

    // ---- 367: five new export formats -- extension follows, audio controls
    // and the quality dropdown are disabled exactly for the three silent ones ----
    const newFormats = ['avi', 'mov', 'mkv', 'webpanim', 'apng'];
    const formatExpectedExt = { webpanim: 'webp' };
    const silentFormats = new Set(['gif', 'webpanim', 'apng']);
    const optionValues = formatSel ? [...formatSel.options].map((o) => o.value) : [];
    for (const fmt of newFormats) {
      if (!optionValues.includes(fmt)) errs.push(`367: export format select is missing an option for "${fmt}"`);
    }
    const muteMic = document.getElementById('ve-mute-mic');
    const volMic = document.getElementById('ve-vol-mic');
    const muteSys = document.getElementById('ve-mute-sys');
    const volSys = document.getElementById('ve-vol-sys');
    const qualitySel = document.getElementById('ve-export-quality');
    for (const fmt of [...newFormats, 'gif', 'mp4']) {
      if (!optionValues.includes(fmt)) continue;
      formatSel.value = fmt;
      formatSel.dispatchEvent(new Event('change'));
      await sleep(30);
      const wantExt = formatExpectedExt[fmt] || fmt;
      if (!destInput.value.endsWith(`.${wantExt}`)) {
        errs.push(`367: picking "${fmt}" left the destination as "${destInput.value}" instead of ending in .${wantExt}`);
      }
      const wantSilent = silentFormats.has(fmt);
      if (qualitySel.disabled !== wantSilent) {
        errs.push(`367: quality dropdown disabled=${qualitySel.disabled} for "${fmt}", expected ${wantSilent}`);
      }
      for (const [name, el] of [['mute-mic', muteMic], ['vol-mic', volMic], ['mute-sys', muteSys], ['vol-sys', volSys]]) {
        if (wantSilent && !el.disabled) {
          errs.push(`367: ${name} stayed enabled for the silent format "${fmt}"`);
        }
      }
    }
    // Leaving the destination on the last silent format's ".webp"/".apng" name
    // would break the next real test step below -- put it back on mp4.
    formatSel.value = 'mp4';
    formatSel.dispatchEvent(new Event('change'));
    await sleep(30);

    // ---- 412: the export format note changes with the format, names the
    // silent formats' missing audio, and carries a size estimate ----
    const exportNoteEl = document.getElementById('ve-export-note');
    if (optionValues.includes('avi')) {
      formatSel.value = 'avi';
      formatSel.dispatchEvent(new Event('change'));
      await sleep(30);
      const aviNote = exportNoteEl.textContent;
      formatSel.value = 'mp4';
      formatSel.dispatchEvent(new Event('change'));
      await sleep(30);
      const mp4Note = exportNoteEl.textContent;
      if (aviNote === mp4Note) {
        errs.push('412: the export note did not change when the format changed');
      }
      if (!/\\d/.test(mp4Note)) {
        errs.push(`412: the export note carries no size figure for mp4, got ${JSON.stringify(mp4Note)}`);
      }
    }
    for (const fmt of ['gif', 'webpanim', 'apng']) {
      if (!optionValues.includes(fmt)) continue;
      formatSel.value = fmt;
      formatSel.dispatchEvent(new Event('change'));
      await sleep(30);
      const note = exportNoteEl.textContent;
      if (!note.includes('carries no sound')) {
        errs.push(`412: the export note is missing the no-audio hint for "${fmt}", got ${JSON.stringify(note)}`);
      }
      if (!/\\d/.test(note)) {
        errs.push(`412: the export note carries no size figure for "${fmt}", got ${JSON.stringify(note)}`);
      }
    }
    formatSel.value = 'mp4';
    formatSel.dispatchEvent(new Event('change'));
    await sleep(30);

    // ---- 387: the remaining-time estimate stays hidden below the minimum percent (10%
    // by default), regardless of how fast progress is moving ----
    const exportEtaEl = document.getElementById('ve-export-eta');
    const exportStartBtn = document.getElementById('ve-export-start');
    // probe_recording's fixture duration is 10 real minutes (600000ms) and
    // nothing before this point trims or re-speeds the clip, so a payload's
    // percentage is exactly ms/600000 -- no need to derive it indirectly.
    exportStartBtn.click();
    await sleep(10);
    window.__FIRE__('recording-convert-progress', { ms: 18000 }); // 3%
    await sleep(15);
    window.__FIRE__('recording-convert-progress', { ms: 30000 }); // 5%, fast rate but still under 10%
    await sleep(10);
    if (exportEtaEl.textContent.trim() !== '') {
      errs.push(`387: remaining-time text showed before the minimum percent, got ${JSON.stringify(exportEtaEl.textContent)}`);
    }
    await sleep(120); // let this export resolve so a second one can start clean

    // ---- 387: past the minimum percent, a slow enough rate shows an "about ..." text ----
    // startExport() itself feeds an initial 0% sample the instant it starts, so
    // the very first real tick below is measured against that -- a real 3s gap
    // keeps that first rate low enough for the average to clear the minimum seconds.
    // export_video needs to still be pending 3s in, hence the longer delay.
    window.__EXPORT_DELAY_MS__ = 3500;
    exportStartBtn.click();
    await sleep(3000);
    window.__FIRE__('recording-convert-progress', { ms: 60000 }); // 10% after ~3s -> roughly 3.3%/s
    await sleep(10);
    if (!/about/i.test(exportEtaEl.textContent)) {
      errs.push(`387: remaining-time text did not appear past the minimum percent with a slow rate, got ${JSON.stringify(exportEtaEl.textContent)}`);
    }
    await sleep(700);
    window.__EXPORT_DELAY_MS__ = undefined;

    // ---- 399: a real classified ffmpeg failure never shows its raw log text
    // as the headline -- the status line gets the translated key, the raw
    // text only appears, closed by default, under "Technical details" ----
    const errDetails = document.getElementById('ve-export-error-details');
    const errRaw = document.getElementById('ve-export-error-raw');
    const rawNeedle = 'No space left on device';
    destInput.value = 'C:\\\\FAILCLASSIFY\\\\clip.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);
    exportStartBtn.click();
    await sleep(120);
    const statusText = document.getElementById('ve-export-status').textContent;
    if (statusText.includes(rawNeedle) || statusText.trim().startsWith('{')) {
      errs.push(`399: export failure status showed the raw/JSON text verbatim, got ${JSON.stringify(statusText)}`);
    }
    if (!statusText.trim()) {
      errs.push('399: export failure left the status line empty');
    }
    if (errDetails.classList.contains('hidden') || errDetails.open) {
      errs.push('399: "Technical details" must appear closed, not hidden and not pre-opened');
    }
    if (!errRaw.textContent.includes(rawNeedle)) {
      errs.push(`399: "Technical details" did not carry the raw ffmpeg text, got ${JSON.stringify(errRaw.textContent)}`);
    }
    // A later successful export must clear both the status wording concern is
    // moot for and the stale details row -- nothing about a past failure
    // should linger once a new export actually starts.
    destInput.value = 'C:\\\\Custom\\\\my-clip.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);
    exportStartBtn.click();
    await sleep(120);
    if (!errDetails.classList.contains('hidden')) {
      errs.push('399: a later successful export left the previous failure details row visible');
    }

    // ---- 336: "Copy to clipboard" runs its own export into a throwaway temp
    // file and lands that file on the clipboard, without touching destInput ----
    window.__CLIP_EXPORT_COPIED__ = undefined;
    document.getElementById('ve-export-clip').click();
    await sleep(150);
    if (!window.__CLIP_EXPORT_COPIED__) {
      errs.push('"Copy to clipboard" never reached copy_file_to_clipboard');
    } else if (!window.__CLIP_EXPORT_COPIED__.includes('clipexport')) {
      errs.push('"Copy to clipboard" copied something other than its own throwaway temp file');
    }

    document.getElementById('ve-tab-edit').click();
    await sleep(80);

    // ---- a gated control actually reacts when its switch flips ----
    const cropFields = document.getElementById('ve-crop-fields');
    const cropOn = document.getElementById('ve-crop-on');
    const dimmedBefore = cropFields.classList.contains('ve-off');
    cropOn.click();
    await sleep(80);
    if (cropFields.classList.contains('ve-off') === dimmedBefore) {
      errs.push('switching crop on did not change the crop fields enabled state');
    }
    const cropInput = cropFields.querySelector('input');
    if (cropInput && cropInput.disabled !== !cropOn.checked) {
      errs.push('a crop field disabled state does not track the crop switch');
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# 385: the remembered-unfinished-edit restore dialog, both answers. A saved
# edit_state fixture (two ops: trim + fade, matching the exact shapes
# loadRecording's own trim/fade handlers push) is planted before boot, so
# checkEditState() finds it on the very first load and opens the dialog --
# same "own boot each time" reasoning as SEAL_PROBE, since the dialog only
# ever opens during the initial loadRecording() pass.
EDIT_STATE_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const action = window.__EDIT_STATE_RESTORE_ACTION__;
  const which = action + ': ';
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    const overlay = document.getElementById('confirm-modal');
    // The dialog only opens once loadRecording's own probe/mux/texts-ready
    // chain has fully settled -- generous relative to the default 5s used
    // elsewhere in this file (still well inside the page's 30s virtual-time
    // budget), since a shorter budget here measurably flaked under load.
    const shown = await until(() => overlay && !overlay.classList.contains('hidden'), 300);
    if (!shown) {
      errs.push(which + 'the restore dialog never opened although a saved edit state was planted');
    } else {
      const title = document.getElementById('confirm-title').textContent.trim();
      const msg = document.getElementById('confirm-msg').textContent.trim();
      if (!title || /^editStateRestore/.test(title)) errs.push(which + 'the dialog title is empty or a raw i18n key: ' + title);
      if (!msg.includes('(2)')) errs.push(which + 'the dialog message does not name the saved op count: ' + msg);
      const okBtn = document.getElementById('confirm-ok');
      const cancelBtn = document.getElementById('confirm-cancel');
      if (!okBtn.textContent.trim() || !cancelBtn.textContent.trim()) {
        errs.push(which + 'the dialog buttons carry no label');
      }
      (action === 'continue' ? okBtn : cancelBtn).click();
      await sleep(200);
      if (!overlay.classList.contains('hidden')) errs.push(which + 'the dialog did not close after the answer');
      const opRows = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
      if (action === 'continue') {
        if (opRows !== 2) errs.push(which + 'continuing did not restore the saved ops onto the timeline: ' + opRows + ' rows');
        if (window.__EDIT_STATE_DISCARDED__) errs.push(which + 'continuing must not also discard the saved edit state');
      } else {
        if (opRows !== 0) errs.push(which + 'starting fresh left restored ops on the timeline: ' + opRows + ' rows');
        const d = window.__EDIT_STATE_DISCARDED__;
        if (!d || d.kind !== 'video' || !d.target) errs.push(which + 'starting fresh did not tell the backend to discard the saved edit state');
      }
    }
  } catch (e) { errs.push(which + 'probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# A 1280x720 source (a small region recording): the fixed-height presets may
# enlarge. 1080p sets a 1920x1080 scale op and shows the "scaled up" hint, the
# export filter scales with lanczos; 720p equals the source height, sets no op,
# selects "keep original" and shows no hint.
SCALE_UP_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    const cur = document.getElementById('ve-scale-current');
    const loaded = await until(() => /1280/.test(cur.textContent) && document.querySelectorAll('#ve-scale-presets button').length > 3, 300);
    if (!loaded) errs.push('the 1280x720 source never loaded: ' + cur.textContent);
    else {
      const btn = (k) => [...document.querySelectorAll('#ve-scale-presets button')].find((b) => b.dataset.key === k);
      const hint = document.getElementById('ve-scale-uphint');
      const wField = document.getElementById('ve-scale-w');
      const hField = document.getElementById('ve-scale-h');
      if (!hint) errs.push('there is no scale-up hint element');
      else {
        if (!hint.classList.contains('hidden')) errs.push('the hint shows before any preset is picked');
        btn('1080').click();
        await sleep(60);
        if (wField.value !== '1920' || hField.value !== '1080') errs.push('1080p on a 720 source did not set 1920x1080, got ' + wField.value + 'x' + hField.value);
        if (hint.classList.contains('hidden')) errs.push('no scale-up hint while 1080p is selected on a 720 source');
        if (!btn('1080').classList.contains('sel')) errs.push('the 1080p button is not marked selected');
        if (!hint.textContent.trim() || /^veScaleUpHint/.test(hint.textContent.trim())) errs.push('the hint carries no text: ' + hint.textContent);
        document.getElementById('ve-export-start').click();
        await sleep(300);
        const call = (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'export_video').pop();
        const chain = call && call[1] && call[1].req ? String(call[1].req.filter_chain) : '';
        if (!/scale=1920:1080:flags=lanczos/.test(chain)) errs.push('the export filter does not enlarge with lanczos: ' + chain);
        btn('720').click();
        await sleep(60);
        if (!hint.classList.contains('hidden')) errs.push('the hint shows for the preset equal to the source height');
        if (!btn('').classList.contains('sel')) errs.push('the source-height preset did not select keep original');
        if (document.querySelectorAll('#ve-ops-list .ve-op-row').length !== 0) errs.push('the source-height preset left a scale op behind');
        btn('480').click();
        await sleep(60);
        if (!hint.classList.contains('hidden')) errs.push('the hint shows for a smaller preset');
        if (wField.value !== '854' || hField.value !== '480') errs.push('480p on a 720 source did not set 854x480, got ' + wField.value + 'x' + hField.value);
      }
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# A second recording arriving (the "video-editor-load" event of a reused window)
# while the restore dialog of the first is still open: the first load must
# settle instead of waiting forever on a dialog nobody can answer any more, and
# the new source must get its own working dialog. The first load finishing is
# seen through the render loop the boot code starts only after it: a
# requestAnimationFrame(tick) call planted before the module runs.
EDIT_STATE_SUPERSEDE_PROBE = """
<script>
window.__TICK_STARTED__ = false;
const realRaf = window.requestAnimationFrame.bind(window);
window.requestAnimationFrame = (fn) => {
  if (fn && fn.name === 'tick') window.__TICK_STARTED__ = true;
  return realRaf(fn);
};
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    const overlay = document.getElementById('confirm-modal');
    const shown = await until(() => overlay && !overlay.classList.contains('hidden'), 300);
    if (!shown) errs.push('the first restore dialog never opened');
    else {
      window.__FIRE__('video-editor-load', { path: 'C:\\\\Recordings\\\\ps-second-recording.mp4', origin_dir: '' });
      // Long enough for the second load to get through its probe and reopen
      // the dialog for its own saved state.
      await sleep(1500);
      if (!window.__TICK_STARTED__) errs.push('the first load never settled after a second recording superseded its restore dialog');
      if (window.__EDIT_STATE_DISCARDED__) errs.push('a superseded dialog must not discard the saved edit state');
      const reopened = await until(() => !overlay.classList.contains('hidden'), 100);
      if (!reopened) errs.push('the second recording got no restore dialog of its own');
      else {
        document.getElementById('confirm-ok').click();
        await sleep(200);
        const rows = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
        if (rows !== 2) errs.push('answering the second dialog did not restore the saved ops: ' + rows + ' rows');
        if (!overlay.classList.contains('hidden')) errs.push('the second dialog did not close after the answer');
      }
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# A DPAPI failure makes the backend write the store in plaintext instead of
# refusing the save. That used to be reported in the error log and nowhere else.
# The same page is booted twice: once with the backend reporting a sealed store,
# once unsealed, and the run asserts BOTH directions: the warning appears when it
# should, and leaves nothing behind (no leftover box, no empty gap in the settings
# page) when it should not.
SEAL_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const unsealed = window.__STORE_SEALED__ === false;
  const which = unsealed ? 'unsealed: ' : 'sealed: ';
  // Same reason the main probe keeps one: a quiet headless frame stops painting,
  // and the startup dialogs hang off a nested requestAnimationFrame.
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--seal-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  const visible = (el) => !!el && !el.classList.contains('hidden');
  try {
    const cm = document.getElementById('confirm-modal');
    // The portable take-over offer opens first on every launch (the stub always
    // offers one). Dismiss it, then watch for whatever the app shows next.
    for (let i = 0; i < 80 && !visible(cm); i++) await sleep(100);
    if (!visible(cm)) {
      errs.push(which + 'startup stalled before any dialog opened');
    } else {
      document.getElementById('confirm-cancel').click();
      await sleep(300);
    }
    // The seal warning is the one-button variant (infoDialog), which is what tells
    // it apart from the two-button take-over offer without reading any text.
    let warned = false;
    for (let i = 0; i < 60; i++) {
      if (visible(cm) && !visible(document.getElementById('confirm-cancel'))) { warned = true; break; }
      await sleep(50);
    }
    if (unsealed && !warned) errs.push(which + 'no warning was shown for an unencrypted store');
    if (!unsealed && warned) errs.push(which + 'a warning was shown although the store is sealed');
    if (warned) {
      const title = document.getElementById('confirm-title').textContent.trim();
      const msg = document.getElementById('confirm-msg').textContent.trim();
      const icon = document.getElementById('confirm-icon');
      if (!title) errs.push(which + 'the warning has no title');
      if (/^storeUnsealed/.test(title) || /^storeUnsealed/.test(msg)) {
        errs.push(which + 'the warning renders a raw i18n key: ' + title + ' / ' + msg);
      }
      // Two paragraphs: what happened, then what helps.
      if (!msg.includes('\\n\\n')) errs.push(which + 'the warning does not say what helps');
      if (!visible(icon) || !icon.querySelector('svg')) errs.push(which + 'the warning has no icon');
      // The way out: an encrypted manual backup does not depend on DPAPI at all,
      // so the dialog offers to make one right now instead of only explaining.
      const alt = document.getElementById('confirm-alt');
      if (!visible(alt)) {
        errs.push(which + 'the warning offers no way to back up now');
      } else {
        const altText = alt.textContent.trim();
        if (!altText || /storeUnsealed/.test(altText)) {
          errs.push(which + 'the backup button renders a raw i18n key: ' + altText);
        }
        alt.click();
        await sleep(300);
        if (!(window.__SMOKE_CALLS__ || []).includes('export_all')) {
          errs.push(which + 'the backup button did not start a backup');
        }
      }
      await sleep(400);
      // Once per session: it must not come straight back after being closed.
      if (visible(cm)) errs.push(which + 'the warning reopened after it was closed');
    }
    // ---- the permanent record in the settings dialog ------------------------
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('expert-open').click();
    await sleep(300);
    const tabs = [...document.querySelectorAll('.expert-tab')];
    if (!tabs.length) errs.push(which + 'the expert menu shows no tabs');
    tabs[tabs.length - 1].click(); // the system tab, which holds the privacy group
    await sleep(300);
    const box = document.getElementById('expert-flags');
    const notice = box.querySelector('.expert-notice[data-notice="storeUnsealed"]');
    if (unsealed) {
      if (!notice) {
        errs.push(which + 'the privacy group carries no permanent warning row');
      } else {
        const text = notice.textContent.trim();
        if (!text) errs.push(which + 'the warning row is empty');
        if (/storeUnsealed/.test(text)) errs.push(which + 'the warning row renders a raw i18n key: ' + text);
        notice.scrollIntoView({ block: 'nearest' });
        const r = notice.getBoundingClientRect();
        if (!r.width || !r.height) errs.push(which + 'the warning row takes up no space');
        // It reports what the data is doing right now, so it must not be
        // switchable off: no control of any kind inside it.
        if (notice.querySelector('input, button, select')) {
          errs.push(which + 'the warning row offers a control to silence it');
        }
      }
    } else {
      if (notice) errs.push(which + 'a warning row is shown although the store is sealed');
      // And no hole where it would have gone: no container in the expert list
      // that occupies vertical space while holding neither text nor children.
      // Containers only — a lone <path> inside an icon is empty by nature.
      for (const el of box.querySelectorAll('div, p, section, li, label, fieldset')) {
        if (el.children.length || el.textContent.trim()) continue;
        if (el.getBoundingClientRect().height > 4) {
          errs.push(which + 'an empty box takes up space in the settings page: ' +
            (el.className || el.tagName));
          break;
        }
      }
    }
  } catch (e) { errs.push(which + 'probe: ' + e.message); }
  clearInterval(beat);
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# 390/455: the first-run tour's draft-buffered write path (openTour({firstRun:
# true})), which the main sweep above never exercises -- there it is only ever
# reopened manually (firstRun: false, live writes), because the fixture ships
# tourSeen: true so the automatic open never interferes with any other check.
# Its own boot, same reasoning as SEAL_PROBE: the automatic open only ever
# happens once, right at startup. window.__TOUR_FIRSTRUN_MODE__ picks Finish or
# Skip so one probe body covers both directions of the buffering rule: Finish
# writes only what was actually touched, Skip writes nothing but tourSeen.
TOUR_FIRSTRUN_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const mode = window.__TOUR_FIRSTRUN_MODE__;
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--tour-fr-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  const visible = (el) => !!el && !el.classList.contains('hidden');
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && !visible(cm); i++) await sleep(100);
    if (visible(cm)) { document.getElementById('confirm-cancel').click(); await sleep(200); }
    const tourEl = document.getElementById('tour-modal');
    for (let i = 0; i < 80 && !visible(tourEl); i++) await sleep(100);
    if (!visible(tourEl)) {
      errs.push(mode + ': the tour did not open automatically with tourSeen unset');
    } else {
      const dots = [...document.querySelectorAll('.tour-dot')];
      dots[dots.length - 1].click(); // step 5, the settings step
      await sleep(150);
      window.__SMOKE_ARGS__ = [];
      // One feature switch, one setup switch -- both start on (shipsOn / the
      // fixture's autostart:false, so flip autostart on instead of a switch row).
      const featureRows = [...document.querySelectorAll('.tour-opt-row')];
      if (!featureRows.length) errs.push(mode + ': step 5 has no feature rows');
      const flippedFeatureCb = featureRows[0].querySelector('input.switch');
      const featureWasOn = flippedFeatureCb.checked;
      flippedFeatureCb.checked = !featureWasOn;
      flippedFeatureCb.dispatchEvent(new Event('change'));
      const setupRows = [...document.querySelectorAll('.tour-setup-row')].filter((r) => r.querySelector('input.switch'));
      if (!setupRows.length) errs.push(mode + ': step 5 has no setup switch rows');
      const setupCb = setupRows.length ? setupRows[0].querySelector('input.switch') : null;
      const setupWasOn = setupCb ? setupCb.checked : null;
      if (setupCb) {
        setupCb.checked = !setupWasOn;
        setupCb.dispatchEvent(new Event('change'));
      }
      // A draft must not write anything live while the tour is still open.
      if (window.__SMOKE_ARGS__.length) {
        errs.push(mode + ': flipping a switch during the draft-buffered open wrote immediately: ' +
          JSON.stringify(window.__SMOKE_ARGS__.map((c) => c[0])));
      }
      window.__SMOKE_ARGS__ = [];
      if (mode === 'finish') {
        document.getElementById('tour-next').click(); // last step's button is Finish
      } else {
        document.getElementById('tour-skip').click(); // top-right Skip, no confirmation
      }
      await sleep(200);
      if (visible(tourEl)) errs.push(mode + ': the tour did not close');
      const calls = window.__SMOKE_ARGS__ || [];
      const flagCalls = calls.filter((c) => c[0] === 'set_ui_flag');
      const tourSeenCall = flagCalls.find((c) => c[1] && c[1].key === 'tourSeen');
      if (!tourSeenCall || tourSeenCall[1].enabled !== true) {
        errs.push(mode + ': tourSeen was not written on close');
      }
      const otherFlagCalls = flagCalls.filter((c) => c[1] && c[1].key !== 'tourSeen');
      const autostartCalls = calls.filter((c) => c[0] === 'set_autostart');
      const autoUpdateCalls = calls.filter((c) => c[0] === 'set_auto_update');
      if (mode === 'finish') {
        if (!otherFlagCalls.length) {
          errs.push('finish: the flipped feature switch was not written');
        } else {
          const fc = otherFlagCalls[0][1];
          if (fc.enabled !== !featureWasOn) errs.push('finish: the feature flag was written with the wrong value');
        }
        if (otherFlagCalls.length > 1) errs.push('finish: an untouched feature flag was written too: ' + JSON.stringify(otherFlagCalls));
        if (!autostartCalls.length && !autoUpdateCalls.length) {
          errs.push('finish: the flipped setup switch was not written');
        }
      } else {
        if (otherFlagCalls.length) errs.push('skip: a feature flag was written even though the tour was skipped: ' + JSON.stringify(otherFlagCalls));
        if (autostartCalls.length || autoUpdateCalls.length) errs.push('skip: a setup switch was written even though the tour was skipped');
      }
    }
  } catch (e) { errs.push(mode + ': probe: ' + e.message); }
  clearInterval(beat);
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# 458: a confidential entry, both while locked and after unlocking it with the
# right password (plus a wrong one in between). Its own boot -- window.__SECRET_TEST__
# adds one confidential fixture (id "psecret") and turns the secretPrompts flag on,
# window.__SECRET_PRECONFIGURED__ makes the stub vault act already set up from an
# earlier session, so this probe exercises the plain unlock dialog rather than
# first-time setup (see tauri_stub.js).
SECRET_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--secret-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  const visible = (el) => !!el && !el.classList.contains('hidden');
  const clickTile = (el) => {
    const r = el.getBoundingClientRect();
    const x = r.left + r.width / 2, y = r.top + r.height / 2;
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: x, clientY: y }));
    window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, clientX: x, clientY: y }));
  };
  // Excludes every <script> (this probe's own source text literally names the
  // marker it is looking for) so the check only ever sees rendered content.
  const bodyHasLeaked = () => {
    const clone = document.body.cloneNode(true);
    clone.querySelectorAll('script').forEach((s) => s.remove());
    return clone.textContent.includes('REAL_TEXT_MARKER') || clone.innerHTML.includes('REAL_TEXT_MARKER');
  };
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && !visible(cm); i++) await sleep(100);
    if (visible(cm)) { document.getElementById('confirm-cancel').click(); await sleep(300); }

    // ---- Balken: a locked confidential tile shows a bar, never the real text ----
    const tile = document.querySelector('.tile[data-id=\\"psecret\\"]');
    if (!tile) {
      errs.push('the confidential prompt has no tile at all');
    } else {
      if (!tile.classList.contains('tile-locked')) errs.push('a locked confidential tile carries no tile-locked class');
      if (!tile.querySelector('.tile-lock-badge')) errs.push('a locked confidential tile shows no lock badge');
    }
    if (bodyHasLeaked()) {
      errs.push('the confidential prompt\\'s real text is present in the DOM while locked');
    }

    // ---- Sperre + Passwortfrage: clicking a locked tile asks for the password ----
    if (tile) {
      clickTile(tile);
      let asked = false;
      for (let i = 0; i < 40; i++) { if (visible(cm)) { asked = true; break; } await sleep(50); }
      if (!asked) {
        errs.push('clicking a locked confidential tile opened no password dialog');
      } else {
        const title = document.getElementById('confirm-title').textContent.trim();
        const msg = document.getElementById('confirm-msg').textContent.trim();
        const row = document.getElementById('confirm-input-row');
        const field = document.getElementById('confirm-input');
        if (!title || /^secretUnlock/.test(title)) errs.push('the unlock dialog title is empty or a raw i18n key: ' + title);
        if (!msg || /^secretUnlock/.test(msg)) errs.push('the unlock dialog message is empty or a raw i18n key: ' + msg);
        if (!visible(row)) errs.push('the unlock dialog has no password field');
        if (field.type !== 'password') errs.push('the unlock field is not masked: type=' + field.type);
        if (document.getElementById('confirm-checkbox-row') && visible(document.getElementById('confirm-checkbox-row'))) {
          errs.push('the plain unlock dialog also shows the first-time setup checkbox');
        }

        // A wrong password must fail cleanly: no copy, tile stays locked, no reopened
        // dialog forced on the user (a fresh click may ask again, this one may not).
        field.value = 'nope';
        document.getElementById('confirm-ok').click();
        await sleep(300);
        if (visible(cm)) errs.push('the dialog is still open after answering a wrong password');
        if (!tile.classList.contains('tile-locked')) errs.push('a wrong password unlocked the tile anyway');
        if (bodyHasLeaked()) {
          errs.push('a wrong password leaked the real text into the DOM');
        }
        if ((window.__SMOKE_CALLS__ || []).includes('copy_text')) {
          errs.push('a wrong password still let the entry reach the clipboard');
        }

        // The right password unlocks it and the click that started this now goes
        // through as a real copy.
        clickTile(tile);
        asked = false;
        for (let i = 0; i < 40; i++) { if (visible(cm)) { asked = true; break; } await sleep(50); }
        if (!asked) {
          errs.push('a second click on the still-locked tile did not ask again');
        } else {
          document.getElementById('confirm-input').value = 'right';
          document.getElementById('confirm-ok').click();
          await sleep(400);
          const freshTile = document.querySelector('.tile[data-id=\\"psecret\\"]');
          if (!freshTile || freshTile.classList.contains('tile-locked')) {
            errs.push('the right password did not unlock the tile');
          }
          if (window.__LAST_COPY__ !== 'REAL_TEXT_MARKER') {
            errs.push('unlocking did not copy the real text: ' + JSON.stringify(window.__LAST_COPY__));
          }
        }
      }
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  clearInterval(beat);
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# 420: dragging a grid tile onto a view tab. Its own boot, same reasoning as
# SEAL_PROBE above: the fixture's "work" view is shrunk to its own single cell
# (window.__DRAG_VIEW_TEST__, see tauri_stub.js) so one placement fills it,
# and the whole scenario needs a fixture shape the main sweep no longer has by
# the time it gets this far.
DRAG_VIEW_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--drag-view-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  const visible = (el) => !!el && !el.classList.contains('hidden');
  const callsFor = (cmd) => (window.__SMOKE_ARGS__ || []).filter(([c]) => c === cmd).map(([, a]) => a);
  const tabFor = (id) => document.querySelector('.view-btn[data-view-id="' + id + '"]');
  const tileEl = (id) => document.querySelector('#grid .tile[data-id="' + id + '"]');
  const selectTile = (id) => { const btn = tileEl(id) && tileEl(id).querySelector('.tile-select'); if (btn) btn.click(); };
  const centre = (el) => { const r = el.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
  const switchToTab = async (id) => {
    tabFor(id).click();
    await sleep(250);
  };
  const ptr = (type, x, y, el, extra) => (el || window).dispatchEvent(new PointerEvent(type, Object.assign({
    bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 6, clientX: x, clientY: y,
  }, extra || {})));
  // Presses a tile and drags it toward (tx, ty) -- past the reorder threshold,
  // then onto the target -- without releasing yet.
  const dragTileTo = async (tile, tx, ty) => {
    const [sx, sy] = centre(tile);
    ptr('pointerdown', sx, sy, tile);
    ptr('pointermove', sx + 8, sy - 8, tile);
    await sleep(80);
    ptr('pointermove', tx, ty, window);
    await sleep(80);
  };
  const endDrag = async (x, y, ctrl) => {
    ptr('pointerup', x, y, window, ctrl ? { ctrlKey: true } : {});
    await sleep(300);
  };
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && !visible(cm); i++) await sleep(100);
    if (visible(cm)) { document.getElementById('confirm-cancel').click(); await sleep(300); }

    const viewsBar = document.getElementById('views');
    if (!visible(viewsBar)) {
      errs.push('the view switcher is not visible for the drag-to-view check');
    } else if (!tabFor('home') || !tabFor('work') || !tabFor('notes') || !tabFor('archive')) {
      errs.push('the fixture does not show the expected home/work/notes/archive tabs');
    } else {
      // ---- fill "work" (shrunk to 1x1) so it counts as a full target ---------
      tabFor('work').click();
      await sleep(200);
      const workCell = document.querySelector('#grid .cell[data-col="0"][data-row="0"]');
      if (!workCell || workCell.firstElementChild) {
        errs.push('expected "work" to offer one empty cell to fill for the full-target check');
      } else {
        workCell.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(200);
        document.getElementById('modal-name').value = 'Work Filler Tile';
        document.getElementById('modal-text').value = 'Fills the only cell in work.';
        document.getElementById('modal-confirm').click();
        await sleep(300);
      }
      const fillerTile = document.querySelector('#grid .cell[data-col="0"][data-row="0"] .tile');
      if (!fillerTile) {
        errs.push('"work" has no tile after filling its only cell');
      } else {
        const fillerId = fillerTile.dataset.id;
        tabFor('home').click();
        await sleep(200);

        // ---- a full target refuses the whole drop, and says so -----------------
        const homeTileP0 = tileEl('p0');
        if (!homeTileP0) {
          errs.push('fixture tile "p0" is missing from "home" for the full-target check');
        } else {
          window.__SMOKE_ARGS__.length = 0;
          await dragTileTo(homeTileP0, ...centre(tabFor('work')));
          if (!tabFor('work').classList.contains('tab-drop')) {
            errs.push('dragging a tile over a view tab does not highlight it');
          }
          await endDrag(...centre(tabFor('work')));
          const fullCalls = callsFor('batch_prompts').filter(a => a && a.action === 'moveView' && a.viewId === 'work');
          if (!fullCalls.length) errs.push('dropping on a full view never asked the backend to move it there');
          const toastText = (document.getElementById('toast').textContent || '');
          if (!toastText.includes('Work') || !toastText.includes('1')) {
            errs.push('a full target view did not toast which view and how many cells are missing: "' + toastText + '"');
          }
          if (!tileEl('p0')) errs.push('a rejected move removed the tile from its own view anyway');
          tabFor('work').click();
          await sleep(200);
          if (tileEl('p0')) errs.push('a rejected move placed the tile in the full target view');
          if (!tileEl(fillerId)) errs.push('a rejected move disturbed the tile already filling the target');
          tabFor('home').click();
          await sleep(200);
        }
      }

      // ---- Ctrl copies: the source keeps the tile ----------------------------
      const homeTileP1 = tileEl('p1');
      if (!homeTileP1 || !tabFor('notes')) {
        errs.push('missing fixture tile or the notes tab for the copy check');
      } else {
        window.__SMOKE_ARGS__.length = 0;
        await dragTileTo(homeTileP1, ...centre(tabFor('notes')));
        if (!tabFor('notes').classList.contains('tab-drop')) {
          errs.push('dragging a tile over a view tab does not highlight it (copy check)');
        }
        const [nx, ny] = centre(tabFor('notes'));
        ptr('pointermove', nx, ny, window, { ctrlKey: true });
        await sleep(80);
        await endDrag(nx, ny, true); // Ctrl held: copy, not move
        const addCalls = callsFor('batch_prompts').filter(a => a && a.action === 'addView' && a.viewId === 'notes');
        if (!addCalls.some(a => (a.ids || []).includes('p1'))) {
          errs.push('Ctrl-dropping on a view tab did not copy the tile there');
        }
        if (!tileEl('p1')) errs.push('copying a tile removed it from the source view');
        await switchToTab('notes');
        if (!tileEl('p1')) errs.push('the copied tile is missing from the target view');
        tabFor('home').click();
        await sleep(200);
      }

      // ---- the same without Ctrl moves instead of copying --------------------
      const homeTileP2 = tileEl('p2');
      if (!homeTileP2 || !tabFor('archive')) {
        errs.push('missing fixture tile or the archive tab for the move check');
      } else {
        window.__SMOKE_ARGS__.length = 0;
        await dragTileTo(homeTileP2, ...centre(tabFor('archive')));
        const [ax, ay] = centre(tabFor('archive'));
        ptr('pointermove', ax, ay, window);
        await sleep(80);
        await endDrag(ax, ay);
        const moveCalls = callsFor('batch_prompts').filter(a => a && a.action === 'moveView' && a.viewId === 'archive');
        if (!moveCalls.some(a => a.fromViewId === 'home' && (a.ids || []).includes('p2'))) {
          errs.push('dropping on a view tab did not move the tile there');
        }
        if (tileEl('p2')) errs.push('moving a tile left it behind in the source view');
        await switchToTab('archive');
        if (!tileEl('p2')) errs.push('the moved tile is missing from the target view');
        tabFor('home').click();
        await sleep(200);
      }

      // ---- dropping on the view already on screen does nothing, no message ---
      const homeTileP3 = tileEl('p3');
      if (!homeTileP3) {
        errs.push('fixture tile "p3" is missing from "home" for the same-view check');
      } else {
        window.__SMOKE_ARGS__.length = 0;
        await dragTileTo(homeTileP3, ...centre(tabFor('home')));
        await endDrag(...centre(tabFor('home')));
        if (callsFor('batch_prompts').length) {
          errs.push('dropping a tile on its own view\\'s tab called the backend anyway');
        }
        if (!tileEl('p3')) errs.push('dropping a tile on its own view removed it');
      }

      // ---- a multi-select drag moves every selected tile together, not just --
      // ---- the one the drag started on; Ctrl copies the whole selection too --
      selectTile('p5');
      selectTile('p7');
      const homeTileP5 = tileEl('p5');
      if (!homeTileP5 || !tileEl('p7')) {
        errs.push('fixture tiles "p5"/"p7" are missing from "home" for the multi-select move check');
      } else {
        window.__SMOKE_ARGS__.length = 0;
        await dragTileTo(homeTileP5, ...centre(tabFor('notes')));
        const [nx2, ny2] = centre(tabFor('notes'));
        ptr('pointermove', nx2, ny2, window);
        await sleep(80);
        await endDrag(nx2, ny2);
        const moveIds = callsFor('batch_prompts')
          .filter(a => a && a.action === 'moveView' && a.viewId === 'notes')
          .flatMap(a => a.ids || []);
        if (!(moveIds.includes('p5') && moveIds.includes('p7'))) {
          errs.push('dragging one of two selected tiles did not move both of them: ' + JSON.stringify(moveIds));
        }
        if (tileEl('p5') || tileEl('p7')) {
          errs.push('a multi-select move left a selected tile behind in the source view');
        }
        await switchToTab('notes');
        if (!tileEl('p5') || !tileEl('p7')) {
          errs.push('a multi-select move did not place both selected tiles in the target view');
        }
        tabFor('home').click();
        await sleep(200);
      }
      selectTile('p9');
      selectTile('p3');
      const homeTileP9 = tileEl('p9');
      if (!homeTileP9 || !tileEl('p3')) {
        errs.push('fixture tiles "p9"/"p3" are missing from "home" for the multi-select copy check');
      } else {
        window.__SMOKE_ARGS__.length = 0;
        await dragTileTo(homeTileP9, ...centre(tabFor('archive')));
        const [ax2, ay2] = centre(tabFor('archive'));
        ptr('pointermove', ax2, ay2, window, { ctrlKey: true });
        await sleep(80);
        await endDrag(ax2, ay2, true);
        const copyIds = callsFor('batch_prompts')
          .filter(a => a && a.action === 'addView' && a.viewId === 'archive')
          .flatMap(a => a.ids || []);
        if (!(copyIds.includes('p9') && copyIds.includes('p3'))) {
          errs.push('Ctrl-dragging one of two selected tiles did not copy both of them: ' + JSON.stringify(copyIds));
        }
        if (!tileEl('p9') || !tileEl('p3')) {
          errs.push('a multi-select copy removed a selected tile from the source view');
        }
        await switchToTab('archive');
        if (!tileEl('p9') || !tileEl('p3')) {
          errs.push('a multi-select copy did not place both selected tiles in the target view');
        }
        tabFor('home').click();
        await sleep(200);
      }

      // ---- 479: a second view-tab drop attempted while the first is still open
      // must be refused outright -- letting it through is how a tile used to end
      // up placed in the first target AND stripped by the second's stale source.
      window.__SMOKE_ARGS__.length = 0;
      window.__BATCH_PROMPTS_GATE__ = [];
      const raceTileA = tileEl('p0');
      const raceTileB = tileEl('p3');
      if (!raceTileA || !raceTileB || !tabFor('notes') || !tabFor('archive')) {
        errs.push('missing fixture tile or view tab for the overlapping-drop check');
      } else {
        await dragTileTo(raceTileA, ...centre(tabFor('notes')));
        const [rnx, rny] = centre(tabFor('notes'));
        ptr('pointermove', rnx, rny, window);
        await sleep(80);
        await endDrag(rnx, rny);
        if (window.__BATCH_PROMPTS_GATE__.length !== 1) {
          errs.push('the first view-tab drop did not reach the backend as expected');
        }
        // The first call's own answer is still held open on purpose -- a
        // second drop attempted right now is exactly the race 479 closes.
        await dragTileTo(raceTileB, ...centre(tabFor('archive')));
        const [rax, ray] = centre(tabFor('archive'));
        ptr('pointermove', rax, ray, window);
        await sleep(80);
        await endDrag(rax, ray);
        if (window.__BATCH_PROMPTS_GATE__.length !== 1) {
          errs.push('a second view-tab drop reached the backend while the first was still open');
        }
        if (!tileEl('p3')) errs.push('a refused second drop moved the tile out of "home" anyway');
        window.__BATCH_PROMPTS_GATE__[0](); // let the first (only) call's answer land
        await sleep(300);
        if (tileEl('p0')) errs.push('the accepted first drop never moved the tile out of "home"');
        await switchToTab('notes');
        if (!tileEl('p0')) errs.push('the accepted first drop is missing from its target view');
        tabFor('home').click();
        await sleep(200);
        if (tileEl('p0')) errs.push('the moved tile ended up in two views at once');
      }

      // ---- 479: a stale response (dispatched before a later one, but answered
      // after it) must never overwrite what the later one already applied.
      window.__BATCH_PROMPTS_GATE__ = [];
      const staleTileA = tileEl('p9');
      const staleTileB = tileEl('p1');
      if (!staleTileA || !staleTileB || typeof window.__runViewMutation !== 'function') {
        errs.push('missing fixture tile or the exposed view-mutation core for the stale-response check');
      } else {
        const callA = window.__runViewMutation(['p9'], 'archive', 'home', false, window.__nextViewMutationSeq());
        const callB = window.__runViewMutation(['p1'], 'notes', 'home', false, window.__nextViewMutationSeq());
        if (window.__BATCH_PROMPTS_GATE__.length !== 2) {
          errs.push('the stale-response check did not reach the backend twice as expected');
        } else {
          window.__BATCH_PROMPTS_GATE__[1](); // the later call answers first
          await callB;
          if (tileEl('p1')) errs.push('the later view move did not remove the tile from "home"');
          window.__BATCH_PROMPTS_GATE__[0](); // the earlier call answers last -- must be discarded
          await callA;
          if (tileEl('p1')) {
            errs.push('a stale response brought a tile back into "home" after a newer move already removed it');
          }
        }
      }
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  clearInterval(beat);
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""



# 276: same idea as SEAL_PROBE just above, for the settings-write-failure notice.
# A failed disk write is backend-only, so the probe cannot trigger a real one --
# it only proves the two states the window itself can show: the startup dialog
# and the permanent settings-page row, driven by the settings_write_failed()
# poll the stub answers from window.__SETTINGS_WRITE_FAILED__.
WRITE_FAIL_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const failed = window.__SETTINGS_WRITE_FAILED__ === true;
  const which = failed ? 'failed: ' : 'ok: ';
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--wf-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  const visible = (el) => !!el && !el.classList.contains('hidden');
  try {
    const cm = document.getElementById('confirm-modal');
    // The portable take-over offer opens first on every launch (the stub always
    // offers one). Dismiss it, then watch for whatever the app shows next.
    for (let i = 0; i < 80 && !visible(cm); i++) await sleep(100);
    if (!visible(cm)) {
      errs.push(which + 'startup stalled before any dialog opened');
    } else {
      document.getElementById('confirm-cancel').click();
      await sleep(300);
    }
    // The write-failed dialog is the one-button variant (infoDialog), same tell
    // apart from the two-button take-over offer as the seal warning uses.
    let warned = false;
    for (let i = 0; i < 60; i++) {
      if (visible(cm) && !visible(document.getElementById('confirm-cancel'))) { warned = true; break; }
      await sleep(50);
    }
    if (failed && !warned) errs.push(which + 'no dialog was shown for a failed settings write');
    if (!failed && warned) errs.push(which + 'a dialog was shown although the write succeeded');
    if (warned) {
      const title = document.getElementById('confirm-title').textContent.trim();
      const msg = document.getElementById('confirm-msg').textContent.trim();
      const icon = document.getElementById('confirm-icon');
      if (!title) errs.push(which + 'the dialog has no title');
      if (/^settingsWriteFailed/.test(title) || /^settingsWriteFailed/.test(msg)) {
        errs.push(which + 'the dialog renders a raw i18n key: ' + title + ' / ' + msg);
      }
      if (!visible(icon) || !icon.querySelector('svg')) errs.push(which + 'the dialog has no icon');
      document.getElementById('confirm-ok').click();
      await sleep(400);
      // Once per session: it must not come straight back after being closed.
      if (visible(cm)) errs.push(which + 'the dialog reopened after it was closed');
    }
    // ---- the permanent record in the settings dialog ------------------------
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('expert-open').click();
    await sleep(300);
    const tabs = [...document.querySelectorAll('.expert-tab')];
    if (!tabs.length) errs.push(which + 'the expert menu shows no tabs');
    tabs[tabs.length - 1].click(); // the system tab, which holds the privacy group
    await sleep(300);
    const box = document.getElementById('expert-flags');
    const notice = box.querySelector('.expert-notice[data-notice="settingsWriteFailed"]');
    if (failed) {
      if (!notice) {
        errs.push(which + 'the privacy group carries no permanent warning row');
      } else {
        const text = notice.textContent.trim();
        if (!text) errs.push(which + 'the warning row is empty');
        if (/settingsWriteFailed/.test(text)) errs.push(which + 'the warning row renders a raw i18n key: ' + text);
        notice.scrollIntoView({ block: 'nearest' });
        const r = notice.getBoundingClientRect();
        if (!r.width || !r.height) errs.push(which + 'the warning row takes up no space');
        // It reports what the disk is doing right now, so it must not be
        // switchable off: no control of any kind inside it.
        if (notice.querySelector('input, button, select')) {
          errs.push(which + 'the warning row offers a control to silence it');
        }
      }
    } else {
      if (notice) errs.push(which + 'a warning row is shown although the write succeeded');
      // And no hole where it would have gone: no container in the expert list
      // that occupies vertical space while holding neither text nor children.
      for (const el of box.querySelectorAll('div, p, section, li, label, fieldset')) {
        if (el.children.length || el.textContent.trim()) continue;
        if (el.getBoundingClientRect().height > 4) {
          errs.push(which + 'an empty box takes up space in the settings page: ' +
            (el.className || el.tagName));
          break;
        }
      }
    }
  } catch (e) { errs.push(which + 'probe: ' + e.message); }
  clearInterval(beat);
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# The data-folder lock: while another machine's lock is fresh, the app opens
# read-only and says so in a persistent line in the window itself, not only in
# the expert menu -- a user who never opens the expert menu still has to see
# it. Same two-boot shape as the settings-write-failure pair above.
DATA_DIR_READONLY_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const readOnly = window.__DATA_DIR_READONLY__ === true;
  const which = readOnly ? 'read-only: ' : 'writable: ';
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--ddro-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  const visible = (el) => !!el && !el.classList.contains('hidden');
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && !visible(cm); i++) await sleep(100);
    if (visible(cm)) {
      document.getElementById('confirm-cancel').click();
      await sleep(300);
    }
    // ---- the persistent line in the window itself --------------------------
    for (let i = 0; i < 40; i++) {
      const b = document.getElementById('data-dir-readonly-banner');
      if (readOnly ? visible(b) : true) break;
      await sleep(50);
    }
    const banner = document.getElementById('data-dir-readonly-banner');
    if (readOnly) {
      if (!visible(banner)) {
        errs.push(which + 'no persistent banner shown although the folder is read-only');
      } else {
        const text = document.getElementById('data-dir-readonly-text').textContent.trim();
        if (!text) errs.push(which + 'the banner is empty');
        if (/dataDirReadOnly/.test(text)) errs.push(which + 'the banner renders a raw i18n key: ' + text);
        if (!text.includes('OTHER-PC')) errs.push(which + 'the banner does not name the other machine: ' + text);
      }
    } else if (visible(banner)) {
      errs.push(which + 'the banner is shown although this machine owns the lock');
    }
    // ---- the permanent record in the settings dialog ------------------------
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('expert-open').click();
    await sleep(300);
    const tabs = [...document.querySelectorAll('.expert-tab')];
    if (!tabs.length) errs.push(which + 'the expert menu shows no tabs');
    tabs[tabs.length - 1].click(); // the system tab, which holds this notice
    await sleep(300);
    const box = document.getElementById('expert-flags');
    const notice = box.querySelector('.expert-notice[data-notice="dataDirReadOnly"]');
    if (readOnly && !notice) {
      errs.push(which + 'the system group carries no read-only warning row');
    } else if (!readOnly && notice) {
      errs.push(which + 'a read-only warning row is shown although this machine owns the lock');
    }
  } catch (e) { errs.push(which + 'probe: ' + e.message); }
  clearInterval(beat);
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# 375: pill folders and grid folders are two separate mechanisms, but the
# "take it apart" menu entry must read and look the same in both. Its own
# boot, same reasoning as SEAL_PROBE above: the fixture already carries one
# grid folder by default, and nothing later in the main sweep needs it
# left standing.
FOLDER_WORDING_PROBE = """
<script>
(async () => {
  const errs = [];
  // Declared here, not inside the try below, so the final report line can
  // still read gridDissolve even if the try block threw partway through.
  let gridDissolve = null;
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--wording-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && cm.classList.contains('hidden'); i++) await sleep(100);
    if (!cm.classList.contains('hidden')) { document.getElementById('confirm-cancel').click(); await sleep(300); }

    const readDissolve = (menu) => {
      const btn = menu ? menu.querySelector('button.danger') : null;
      const icon = btn ? btn.querySelector('.menu-icon') : null;
      return btn ? { label: btn.textContent.trim(), icon: icon ? icon.innerHTML : null } : null;
    };

    const fTile = document.querySelector('.tile.folder-tile');
    if (!fTile) {
      errs.push('375: no grid folder in the fixture to compare');
    } else {
      fTile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(150);
      gridDissolve = readDissolve(document.querySelector('.tab-menu:not(.hidden)'));
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
      await sleep(120);
      if (!gridDissolve) errs.push('375: the grid folder context menu has no ungroup option');
    }
  } catch (e) { errs.push('375 probe: ' + e.message); }
  clearInterval(beat);
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  // gridDissolve travels out too -- 483 uses it as the pill folder's own
  // expectation, since the pill boots as a separate page and cannot read this
  // window's DOM directly.
  pre.textContent = JSON.stringify({ errs, gridDissolve });
  document.body.appendChild(pre);
})();
</script>
"""


# 483: the pill folder window's own "ungroup" entry, checked against the grid
# folder's reading that FOLDER_WORDING_PROBE captured above -- 375 asked for one
# language and one icon across all three folder kinds, but the wording probe
# above never looked at this window, so it could drift without any test
# noticing.
PILL_WORDING_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--pill-wording-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(100); return fn(); };
  try {
    const ready = await until(() => document.getElementById('fname').textContent !== '');
    if (!ready) errs.push('the folder window never rendered its state');
    // The label/icon are filled in by the window's own async translation load
    // (applyTexts), separate from the state fetch fname above waits on -- wait
    // for that to land before opening the menu, or the button reads empty.
    const textsReady = await until(() => !!document.getElementById('menu-dissolve').querySelector('.menu-label'));
    if (!textsReady) errs.push('the folder window never applied its own translations');
    document.getElementById('icon').dispatchEvent(
      new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    );
    await sleep(150);
    const btn = document.getElementById('menu-dissolve');
    const icon = btn ? btn.querySelector('.menu-icon') : null;
    const label = btn ? btn.querySelector('.menu-label') : null;
    if (!btn || !label || label.textContent.trim() === '') {
      errs.push('the pill folder has no ungroup entry in its context menu');
    }
    const pillDissolve = btn ? { label: label ? label.textContent.trim() : '', icon: icon ? icon.innerHTML : null } : null;
    const pre = document.createElement('pre');
    pre.id = 'SMOKE';
    pre.textContent = JSON.stringify({ errs, pillDissolve });
    document.body.appendChild(pre);
  } catch (e) {
    errs.push('483 pill wording probe: ' + e.message);
    const pre = document.createElement('pre');
    pre.id = 'SMOKE';
    pre.textContent = JSON.stringify({ errs, pillDissolve: null });
    document.body.appendChild(pre);
  }
  clearInterval(beat);
})();
</script>
"""


# 376: the small keyboard badge on a tile whose prompt carries its own hotkey.
# Its own boot, same reasoning as the probes above -- the fixture's p0 already
# carries a hotkey and nothing later in the main sweep needs it still bound.
TILE_HOTKEY_BADGE_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--hk-badge-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && cm.classList.contains('hidden'); i++) await sleep(100);
    if (!cm.classList.contains('hidden')) { document.getElementById('confirm-cancel').click(); await sleep(300); }

    const badgeCount = (id) => document.querySelectorAll('.tile[data-id="' + id + '"] .tile-hotkey-badge').length;

    const withHotkey = badgeCount('p0');
    if (withHotkey !== 1) errs.push('376: prompt p0 has a hotkey but shows ' + withHotkey + ' badge(s) instead of 1');
    const withoutHotkey = badgeCount('p1');
    if (withoutHotkey !== 0) errs.push('376: prompt p1 has no hotkey but shows ' + withoutHotkey + ' badge(s)');

    const badge = document.querySelector('.tile[data-id="p0"] .tile-hotkey-badge');
    const title = badge ? (badge.getAttribute('title') || '') : '';
    if (!title || title === 'tileHotkeyBadgeTitle' || !title.includes('9')) {
      errs.push('376: the hotkey badge title reads "' + title + '" instead of the formatted combination');
    }
    if (badge && badge.getAttribute('aria-label') !== title) {
      errs.push('376: the hotkey badge title and aria-label do not match');
    }
    if (badge && (badge.tabIndex >= 0 || badge.tagName === 'BUTTON' || badge.tagName === 'A')) {
      errs.push('376: the hotkey badge is focusable or clickable, it should be pure information');
    }

    // Switching the flag off must remove the badge live, not just after reload.
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('expert-open').click();
    await sleep(300);
    let row = null;
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click();
      await sleep(80);
      row = [...document.querySelectorAll('#expert-flags .switch-field')]
        .find(r => (r.querySelector('span') || {}).textContent === 'Hotkey badge');
      if (row) break;
    }
    const cb = row && row.querySelector('input.switch');
    if (!cb) {
      errs.push('376: no expert-menu switch found for the hotkey badge');
    } else {
      if (!cb.checked) errs.push('376: the hotkey badge flag does not ship on');
      cb.checked = false;
      cb.dispatchEvent(new Event('change'));
      await sleep(300);
      const left = badgeCount('p0');
      if (left !== 0) errs.push('376: switching the flag off left ' + left + ' badge(s) on the tile');
    }
  } catch (e) { errs.push('376 probe: ' + e.message); }
  clearInterval(beat);
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

# 459: demo mode -- window.__DEMO_TEST__ models booting straight into an
# already-active demo session (what a real restart into the sample library
# looks like from the window's side; the swap itself and the sample library's
# own content are Rust-side and covered there). Its own boot, same reasoning
# as the probes above.
DEMO_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && cm.classList.contains('hidden'); i++) await sleep(100);
    if (!cm.classList.contains('hidden')) { document.getElementById('confirm-cancel').click(); await sleep(300); }

    const banner = document.getElementById('demo-mode-banner');
    if (!banner || getComputedStyle(banner).display === 'none') {
      errs.push('459: the demo-mode marker is not visible while demo mode is on');
    } else if (!banner.textContent.trim()) {
      errs.push('459: the demo-mode marker has no text');
    }

    // Turning it off asks first -- cancelling must leave demo mode untouched
    // and the checkbox still checked.
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('expert-open').click();
    await sleep(300);
    let row = null;
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click();
      await sleep(80);
      row = [...document.querySelectorAll('#expert-flags .switch-field')]
        .find((r) => (r.querySelector('span') || {}).textContent === 'Demo mode');
      if (row) break;
    }
    const cb = row && row.querySelector('input.switch');
    if (!cb) {
      errs.push('459: no expert-menu switch found for demo mode');
    } else {
      if (!cb.checked) errs.push('459: the demo-mode switch does not show as on during a demo session');
      window.__SMOKE_CALLS__ = [];
      cb.checked = false;
      cb.dispatchEvent(new Event('change'));
      await sleep(200);
      if (document.getElementById('confirm-modal').classList.contains('hidden')) {
        errs.push('459: turning demo mode off did not ask for confirmation first');
      }
      document.getElementById('confirm-cancel').click();
      await sleep(150);
      if (!cb.checked) errs.push('459: cancelling the confirmation must leave the switch on');
      if (window.__SMOKE_CALLS__.includes('disable_demo_mode')) {
        errs.push('459: cancelling the confirmation must not call disable_demo_mode');
      }
      // Confirming calls the real switch-back command -- the actual data swap
      // and relaunch are Rust-side (see the cargo tests), not reachable here.
      cb.checked = false;
      cb.dispatchEvent(new Event('change'));
      await sleep(200);
      document.getElementById('confirm-ok').click();
      await sleep(200);
      if (!window.__SMOKE_CALLS__.includes('disable_demo_mode')) {
        errs.push('459: confirming did not call disable_demo_mode');
      }
    }
  } catch (e) { errs.push('459 probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

CAP_NEEDS_ADMIN_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--cap-admin-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && cm.classList.contains('hidden'); i++) await sleep(100);
    if (!cm.classList.contains('hidden')) { document.getElementById('confirm-cancel').click(); await sleep(300); }

    // A machine that refuses even the read-only listing shows the elevate
    // button first -- clicking it is what actually attempts the UAC prompt,
    // where the two distinct outcomes (378) diverge.
    window.__OCR_CAPS_ELEV__ = true;

    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('expert-open').click();
    await sleep(300);
    let openBtn = null;
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click();
      await sleep(80);
      openBtn = document.querySelector('.expert-action[data-action="ocr-langs-open"]');
      if (openBtn) break;
    }
    if (!openBtn) {
      errs.push('378: OCR "manage languages" action not found in the expert menu');
    } else {
      const runScenario = async (errorCode, expectRetry) => {
        window.__OCR_ELEV_ERROR__ = errorCode;
        openBtn.click();
        await sleep(150);
        const modal = document.getElementById('ocr-langs-modal');
        if (!modal || modal.classList.contains('hidden')) {
          errs.push('378 (' + errorCode + '): OCR langs modal did not open');
          return;
        }
        const elevBtn = document.querySelector('#ocr-langs-body .ghost-btn');
        if (!elevBtn) {
          errs.push('378 (' + errorCode + '): no elevate button on the initial "needs elevation" notice');
        } else {
          elevBtn.click();
          await sleep(100);
          const okBtn = document.getElementById('confirm-ok');
          if (!okBtn || okBtn.offsetParent === null) {
            errs.push('378 (' + errorCode + '): elevation button did not explain itself first');
          } else {
            okBtn.click();
            await sleep(200);
            const hint = document.querySelector('#ocr-langs-body .hint');
            if (!hint || !hint.textContent.trim()) {
              errs.push('378 (' + errorCode + '): no message shown after the elevation attempt failed');
            } else {
              const wantsText = expectRetry
                ? 'An administrator has to allow this. Ask someone with administrator rights to run the installation, then try again.'
                : 'Language pack installation is blocked on this computer.';
              if (!hint.textContent.includes(wantsText)) {
                errs.push('482 (' + errorCode + '): message does not match the decided wording, got: ' + hint.textContent);
              }
            }
            const retryBtn = document.querySelector('#ocr-langs-body .ghost-btn');
            if (expectRetry && !retryBtn) {
              errs.push('378 (' + errorCode + '): retry button missing for a transient failure');
            }
            if (!expectRetry && retryBtn) {
              errs.push('378 (' + errorCode + '): retry button shown even though this account can never elevate');
            }
          }
        }
        document.getElementById('ocr-langs-close-x').click();
        await sleep(100);
      };

      // The refused case: this account can never elevate -- no retry button.
      await runScenario('ocr-elevate-no-rights', false);
      // The transient case: a real declined/failed attempt -- retry stays.
      await runScenario('ocr-elevate-cancelled', true);
    }
    window.__OCR_CAPS_ELEV__ = false;
    window.__OCR_ELEV_ERROR__ = null;
  } catch (e) { errs.push('378 probe: ' + e.message); }
  clearInterval(beat);
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

# 411: the parked marker on a pill/folder whose saved screen is gone. Its own
# boot (window.__FLOAT_PARKED__ = true), same reasoning as the store-sealing
# and drag-view fixtures above: is_float_parked is only ever asked once at
# boot, a state the main sweep's default fixture never sets up.
PARKED_BADGE_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(50); return fn(); };
  try {
    // Not the badge's title: like every other pill/folder label, it waits on
    // the lazily imported translations, which never resolve under this
    // harness's virtual clock (see FLOAT_PROBE above) -- visibility alone,
    // set straight from the boot-time is_float_parked call, is what this checks.
    const badge = document.getElementById('parked-badge');
    await until(() => !badge.classList.contains('hidden'));
    if (badge.classList.contains('hidden')) {
      errs.push('411: the parked marker never appeared on a button whose screen is gone');
    }
  } catch (e) { errs.push('411 probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

# 411: the float-recall expert action reaches the backend. Its own boot (a
# plain index.html start is enough; the action does not depend on any pill or
# folder actually being parked, only on the floating feature being on, which
# ships on by default). expert-open's click handler only attaches once init()'s
# async settings fetch resolves; a fixed short wait can lose that race under
# load, so the open is retried instead of assumed.
FLOAT_RECALL_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const calls = () => (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []);
  try {
    document.getElementById('settings').classList.remove('hidden');
    await sleep(50);
    const findBtn = () => document.querySelector('.expert-action[data-action="float-recall"]');
    let opened = false;
    for (let i = 0; i < 40 && !opened; i++) {
      document.getElementById('expert-open').click();
      await sleep(100);
      opened = !document.getElementById('settings-expert').classList.contains('hidden');
    }
    if (!opened) errs.push('411: the expert menu never opened');
    let btn = findBtn();
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      if (btn) break;
      tb.click();
      await sleep(150);
      btn = findBtn();
    }
    if (!btn) {
      errs.push('411: float-recall expert action not found in any tab');
    } else {
      calls().length = 0;
      btn.click();
      await sleep(150);
      if (!calls().includes('float_recall')) errs.push('411: clicking the recall action never called float_recall');
    }
  } catch (e) { errs.push('411 probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

# 404: the "whats new" toast on startup, the window it opens (with an
# injected script tag that must render as text, never run), the same window
# reopened current-version-only from the About dialog, and its GitHub refresh.
WHATS_NEW_PROBE = """
<script>
window.__WHATS_NEW_FIXTURE__ = {
  since: "1.0.0",
  sections: [
    { version: "1.1.0", markdown: "## Prompt Saver 1.1.0\\n\\nFirst new thing." },
    { version: "1.2.0", markdown: "## Prompt Saver 1.2.0\\n\\n<script>window.__SMOKE_XSS__ = true;<" + "/script> Second new thing." },
  ],
};
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--whats-new-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  const waitFor = async (pred, tries) => {
    for (let i = 0; i < tries; i++) { if (pred()) return true; await sleep(100); }
    return pred();
  };
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && cm.classList.contains('hidden'); i++) await sleep(100);
    if (!cm.classList.contains('hidden')) { document.getElementById('confirm-cancel').click(); await sleep(300); }

    const toastEl = document.getElementById('toast');
    const toastUp = await waitFor(() => toastEl.classList.contains('show') && toastEl.classList.contains('actionable'), 60);
    if (!toastUp) {
      errs.push('404: no whats-new toast appeared on startup');
    } else {
      const btn = toastEl.querySelector('.toast-btn');
      if (!btn) errs.push('404: the whats-new toast has no action button');
      else btn.click();
      await sleep(150);
      const modal = document.getElementById('update-modal');
      if (!modal || modal.classList.contains('hidden')) {
        errs.push('404: clicking the toast did not open the whats-new window');
      } else {
        const notes = document.getElementById('update-modal-notes');
        if (!document.getElementById('update-modal-install').classList.contains('hidden')
            || !document.getElementById('update-modal-skip').classList.contains('hidden')) {
          errs.push('404: the notes view of the update dialog still offers install or skip');
        }
        if (document.getElementById('update-modal-refresh').classList.contains('hidden')) {
          errs.push('404: the notes view of the update dialog has no refresh button');
        }
        const text = notes.textContent || '';
        if (!text.includes('First new thing')) errs.push('404: the window is missing the older of the two new sections');
        if (!text.includes('Second new thing')) errs.push('404: the window is missing the newer of the two new sections');
        if (!text.includes('<script>')) {
          errs.push('404: an inserted script tag did not survive as literal text: ' + text);
        }
        if (window.__SMOKE_XSS__) errs.push('404: an inserted script tag in the list actually ran');
        document.getElementById('update-modal-cancel').click();
        await sleep(150);
        if (!modal.classList.contains('hidden')) errs.push('404: the whats-new window did not close');
      }
    }

    document.getElementById('gear').click();
    await sleep(150);
    document.getElementById('about-btn').click();
    await sleep(150);
    const aboutBtn = document.getElementById('about-whats-new');
    if (!aboutBtn) {
      errs.push('404: the About dialog has no whats-new button');
    } else {
      const aboutModal = document.getElementById('about-modal');
      aboutBtn.click();
      await sleep(150);
      const modal = document.getElementById('update-modal');
      const notes = document.getElementById('update-modal-notes');
      const text = (notes && notes.textContent) || '';
      if (!modal || modal.classList.contains('hidden')) {
        errs.push('404: the About dialog button did not open the whats-new window');
      } else {
        if (!aboutModal.classList.contains('hidden')) {
          errs.push('517: the About dialog stays open behind the whats-new window');
        }
        const r = modal.getBoundingClientRect();
        const topEl = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        if (!modal.contains(topEl)) {
          errs.push('517: the whats-new window is not the topmost element at its own centre point');
        }
        if (!text.includes('Current version only')) {
          errs.push('404: the About-triggered window does not show the current version own section');
        }
        if (text.includes('First new thing')) {
          errs.push('404: the About-triggered window shows the startup range instead of the current version only');
        }
        const refreshBtn = document.getElementById('update-modal-refresh');
        refreshBtn.click();
        await sleep(150);
        const refreshed = (notes && notes.textContent) || '';
        if (!refreshed.includes('Refreshed from GitHub')) {
          errs.push('404: the Update-from-GitHub button did not replace the list with the fetched one');
        }
        document.getElementById('update-modal-cancel').click();
        await sleep(150);
        if (aboutModal.classList.contains('hidden')) {
          errs.push('517: closing the whats-new window opened from About did not return to About');
        }
        // Escape takes the same way back as the Close button.
        aboutBtn.click();
        await sleep(150);
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await sleep(150);
        if (!modal.classList.contains('hidden')) errs.push('404: Escape did not close the whats-new window');
        if (aboutModal.classList.contains('hidden')) errs.push('517: Escape on the whats-new window did not return to About');
        document.getElementById('about-close').click();
        await sleep(150);
      }
    }
  } catch (e) { errs.push('404 probe: ' + e.message); }
  clearInterval(beat);
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

# 451: the About window's third-party license list -- section present, expanding
# an entry shows its license text, and no entry (including the note-only,
# text-less one) ever expands to a blank panel.
LICENSES_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    document.getElementById('gear').click();
    await sleep(150);
    document.getElementById('about-btn').click();
    await sleep(150);
    const openBtn = document.getElementById('about-licenses');
    if (!openBtn) {
      errs.push('451: the About dialog has no third-party licenses button');
    } else {
      openBtn.click();
      await sleep(150);
      const modal = document.getElementById('about-licenses-body');
      if (!modal || modal.classList.contains('hidden') || openBtn.getAttribute('aria-expanded') !== 'true') {
        errs.push('451: clicking the button did not expand the license list inside About');
      } else {
        const rows = Array.from(modal.querySelectorAll('.license-row'));
        if (rows.length !== 3) errs.push('451: expected 3 rows from the fixture, found ' + rows.length);
        for (const row of rows) {
          const toggle = row.querySelector('.license-row-toggle');
          const body = row.querySelector('.license-row-body');
          toggle.click();
          await sleep(50);
          if (body.classList.contains('hidden')) errs.push('451: expanding ' + toggle.textContent + ' did not reveal its panel');
          if (!body.textContent || !body.textContent.trim()) {
            errs.push('451: expanded panel for ' + toggle.textContent + ' is empty');
          }
        }
        const mitRow = rows.find((r) => r.textContent.includes('examplecrate'));
        if (!mitRow || !mitRow.querySelector('.license-row-text')) {
          errs.push('451: the MIT-resolved crate does not show its license text');
        }
        const sqliteRow = rows.find((r) => r.textContent.includes('SQLite'));
        if (!sqliteRow || !sqliteRow.querySelector('.license-row-body').textContent.includes('Public Domain')) {
          errs.push('451: the no-text public-domain entry does not fall back to showing its license label');
        }
        openBtn.click();
        await sleep(150);
        if (!modal.classList.contains('hidden') || openBtn.getAttribute('aria-expanded') !== 'false') {
          errs.push('451: clicking the licenses head again did not collapse the list');
        }
      }
    }
  } catch (e) { errs.push('451 probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

# 443: a crash draft found at the next start offers to restore it exactly
# once; declining discards it right away, and a later ordinary editor open
# must never ask about it again (the offer is a one-time boot check, not tied
# to openModal).
EDITOR_DRAFT_PROBE = """
<script>
window.__EDITOR_DRAFT_FIXTURE__ = {
  promptId: "neu",
  name: "",
  text: "Recovered body text that should show up in the preview.",
  variants: "[]",
  updatedAt: Math.floor(Date.now() / 1000) - 120,
};
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const calls = () => (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []);
  try {
    const cm = document.getElementById('confirm-modal');
    // Earlier boot dialogs (the store-takeover offer, an unsealed-store
    // warning) can show first -- dismiss whichever of those come up until the
    // recovery question itself, identified by its own title, appears.
    let shown = false;
    for (let i = 0; i < 60 && !shown; i++) {
      await sleep(300);
      if (cm.classList.contains('hidden')) continue;
      const title = document.getElementById('confirm-title').textContent;
      if (/restore/i.test(title)) { shown = true; break; }
      document.getElementById('confirm-cancel').click();
    }
    if (!shown) {
      errs.push('443: no recovery question appeared for a found draft');
    } else {
      const title = document.getElementById('confirm-title').textContent.trim();
      const msg = document.getElementById('confirm-msg').textContent.trim();
      if (!/restore/i.test(title)) errs.push('443: recovery dialog title does not read as a restore question: ' + title);
      if (!msg.includes('Recovered body text')) errs.push('443: recovery dialog does not preview the draft text: ' + msg);
      const loadCalls = calls().filter((c) => c === 'load_editor_draft').length;
      if (loadCalls !== 1) errs.push('443: load_editor_draft was called ' + loadCalls + ' times at boot, expected exactly once');

      document.getElementById('confirm-cancel').click(); // decline the restore
      await sleep(200);
      if (!cm.classList.contains('hidden')) errs.push('443: declining the recovery question left it open');
      if (!calls().includes('discard_editor_draft')) errs.push('443: declining the recovery question never discarded the draft');

      // An ordinary editor open right afterward must never ask again.
      const input = document.getElementById('input');
      const save = document.getElementById('save');
      input.value = 'A fresh prompt typed after declining the draft.';
      input.dispatchEvent(new Event('input', { bubbles: true }));
      save.click();
      await sleep(200);
      const modal = document.getElementById('modal');
      if (modal.classList.contains('hidden')) errs.push('443: the editor did not open for the fresh prompt');
      if (!cm.classList.contains('hidden')) errs.push('443: opening the editor normally re-asked the recovery question');
      document.getElementById('modal-close-x').click();
      await sleep(200);
      if (!cm.classList.contains('hidden')) {
        // The ordinary discard-confirmation for a still-open create modal --
        // unrelated to the recovery question, just closing it out cleanly.
        document.getElementById('confirm-ok').click();
        await sleep(150);
      }
    }
  } catch (e) { errs.push('443 probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

# 475: the extra-video-source limit used to be a fixed constant; it is now an
# expert value the smoke run can dial down and prove the "add a video" flow
# actually reads.
EXTRA_CLIPS_MAX_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 60) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    await until(() => {
      const opsHint = document.querySelector('#ve-ops-list p');
      return document.getElementById('ve-stage-msg').classList.contains('hidden') &&
        document.querySelectorAll('#ve-filmstrip .ve-thumb').length > 0 && !!opsHint;
    }, 300);
    document.getElementById('ve-tab-overlays').click();
    await sleep(80);

    const beforeClips = document.querySelectorAll('#ve-clip-list .ve-op-row').length;
    if (beforeClips !== 0) {
      errs.push('475: setup: the clip list was not empty before the limit test, found ' + beforeClips);
    }
    await window.__TAURI__.core.invoke('set_ui_value', { key: 'veExtraClipsMax', value: 1 });
    window.__FIRE__('settings-changed');

    // Every wait below polls the real DOM state instead of sleeping a fixed
    // guess -- a fixed sleep here was too short under load (many probes
    // running at once), flaking this whole check.
    document.getElementById('ve-add-video').click();
    let pickPop = null;
    await until(() => {
      pickPop = document.getElementById('ve-add-video-pop');
      return pickPop && !pickPop.classList.contains('hidden') && pickPop.querySelectorAll('button').length === 2;
    }, 100);
    if (!pickPop || pickPop.classList.contains('hidden') || pickPop.querySelectorAll('button').length !== 2) {
      errs.push('475: lowering the limit to one still blocked the first extra source');
    } else {
      pickPop.querySelectorAll('button')[1].click(); // as its own section
      let posButtons = [];
      await until(() => {
        posButtons = pickPop.querySelectorAll('button');
        return !pickPop.classList.contains('hidden') && posButtons.length === 3;
      }, 100);
      if (pickPop.classList.contains('hidden') || posButtons.length !== 3) {
        errs.push('475: choosing as its own section did not open the position choice');
      } else {
        posButtons[1].click(); // at the end
        await until(() => document.querySelectorAll('#ve-clip-list .ve-op-row').length === 1, 100);
      }
    }
    const afterFirst = document.querySelectorAll('#ve-clip-list .ve-op-row').length;
    if (afterFirst !== 1) {
      errs.push('475: the first extra source under a limit of one was not inserted, found ' + afterFirst + ' clip row(s)');
    }

    document.getElementById('ve-add-video').click();
    const toastEl = document.getElementById('ve-toast');
    await until(() => !toastEl.classList.contains('hidden'), 100);
    pickPop = document.getElementById('ve-add-video-pop');
    const popOpened = pickPop && !pickPop.classList.contains('hidden') && pickPop.querySelectorAll('button').length;
    if (popOpened) {
      errs.push('475: a second extra source was still allowed once the lowered limit of one was reached');
    }
    if (toastEl.classList.contains('hidden') || !toastEl.textContent || !toastEl.textContent.includes('1')) {
      errs.push('475: the limit toast did not report the lowered value of one, got: ' + toastEl.textContent);
    }
    if (/^ve[A-Z][A-Za-z]+$/.test((toastEl.textContent || '').trim())) {
      errs.push('475: the limit toast rendered a raw i18n key: ' + toastEl.textContent);
    }
    const afterSecond = document.querySelectorAll('#ve-clip-list .ve-op-row').length;
    if (afterSecond !== 1) {
      errs.push('475: a rejected extra source still changed the clip list, found ' + afterSecond + ' row(s)');
    }
  } catch (e) { errs.push('475 probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

# 413: the statistics/backups window's retention overview -- one row per
# kept-data category, in the fixed order the spec names, each with an amount,
# how long it is kept, a jump-to-setting button and a cleanup button.
RETENTION_PANEL_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const waitFor = async (pred, tries) => {
    for (let i = 0; i < tries; i++) { if (pred()) return true; await sleep(100); }
    return pred();
  };
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && cm.classList.contains('hidden'); i++) await sleep(100);
    if (!cm.classList.contains('hidden')) { document.getElementById('confirm-cancel').click(); await sleep(300); }

    document.getElementById('gear').click();
    await sleep(150);
    document.getElementById('settings-backup-btn').click();
    await sleep(300);
    if (document.getElementById('backup-modal').classList.contains('hidden')) {
      errs.push('413: the backups dialog did not open');
    }

    const order = ['trash', 'copyHistory', 'clipInbox', 'recordings', 'discardedRecordings', 'tempExports', 'editStates'];
    const rows = [...document.querySelectorAll('#retention-panel [data-setting-row^="retention-"]')];
    const gotOrder = rows.map((r) => r.dataset.settingRow.replace('retention-', ''));
    if (gotOrder.join(',') !== order.join(',')) {
      errs.push('413: the retention rows are not in the spec order, got: ' + gotOrder.join(','));
    }

    const trashRow = document.querySelector('#retention-panel [data-setting-row="retention-trash"]');
    const trashAmount = trashRow && trashRow.querySelector('.storage-size');
    if (!trashAmount || !trashAmount.textContent.includes('3')) {
      errs.push('413: the trash row amount does not show the stubbed count of three: ' + (trashAmount && trashAmount.textContent));
    }
    if (/^retention[A-Z][A-Za-z]+$/.test((trashAmount && trashAmount.textContent || '').trim())) {
      errs.push('413: the trash row amount rendered a raw i18n key: ' + trashAmount.textContent);
    }

    const discardedRow = document.querySelector('#retention-panel [data-setting-row="retention-discardedRecordings"]');
    const discardedCleanBtn = discardedRow && discardedRow.querySelectorAll('.import-preview-actions button')[1];
    if (!discardedCleanBtn || !discardedCleanBtn.disabled) {
      errs.push('413: the discarded-recordings cleanup button should be disabled when its count is zero');
    }

    // Jump-to-setting from the trash row must land on trashRetentionDays,
    // in the expert menu, flashed.
    const trashJumpBtn = trashRow.querySelectorAll('.import-preview-actions button')[0];
    trashJumpBtn.click();
    await sleep(250);
    if (document.getElementById('backup-modal').classList.contains('hidden') !== true) {
      errs.push('413: jumping to a setting did not close the backups dialog behind it');
    }
    if (document.getElementById('settings').classList.contains('hidden')) {
      errs.push('413: jumping to a setting did not open the settings dialog');
    }
    const flashedTarget = document.querySelector('#settings-expert [data-setting-row="trashRetentionDays"]');
    if (!flashedTarget) {
      errs.push('413: the jump target trashRetentionDays is not present in the expert menu DOM');
    } else if (!flashedTarget.classList.contains('settings-search-hit')) {
      errs.push('413: the jump target was not flashed');
    }

    // Back to the panel for the cleanup-button round trip.
    document.getElementById('gear').click();
    await sleep(150);
    document.getElementById('settings-backup-btn').click();
    await sleep(300);
    const trashRow2 = document.querySelector('#retention-panel [data-setting-row="retention-trash"]');
    const trashCleanBtn = trashRow2.querySelectorAll('.import-preview-actions button')[1];
    trashCleanBtn.click();
    await sleep(250);
    const toastEl = document.getElementById('toast');
    if (!toastEl.classList.contains('show') || !toastEl.textContent.includes('3')) {
      errs.push('413: the cleanup toast did not report the stubbed removed count of three: ' + toastEl.textContent);
    }
    if (/^retention[A-Z][A-Za-z]+$/.test((toastEl.textContent || '').trim())) {
      errs.push('413: the cleanup toast rendered a raw i18n key: ' + toastEl.textContent);
    }
  } catch (e) { errs.push('413 probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

# 444: fixed values for recurring placeholders -- the expert-menu list (add,
# edit shows in the row, delete), the empty-state hint, and the fill-in
# dialog's marker + placeholder-only suggestion (never silently inserted).
VAR_FIXED_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && cm.classList.contains('hidden'); i++) await sleep(100);
    if (!cm.classList.contains('hidden')) { document.getElementById('confirm-cancel').click(); await sleep(300); }

    document.getElementById('gear').click();
    await sleep(150);
    document.getElementById('expert-open').click();
    await sleep(200);

    const rowByLabel = async (label) => {
      const hit = () => [...document.querySelectorAll('#expert-flags .switch-field')]
        .find(r => ((r.querySelector('span') || {}).textContent || '').includes(label));
      if (hit()) return hit();
      for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
        tb.click();
        await sleep(200);
        if (hit()) return hit();
      }
      return null;
    };
    const flagRow = await rowByLabel('Suggest fixed values for placeholders');
    if (!flagRow) {
      errs.push('444: the fixed-value flag is missing from the expert menu');
    } else {
      flagRow.querySelector('input.switch').click();
      await sleep(150);
      // The flag is not a gate for anything else, so force a redraw the same
      // way switching tabs already does, instead of relying on one.
      const activeTab = document.querySelector('#expert-tabs-bar .expert-tab.active');
      if (activeTab) { activeTab.click(); await sleep(150); }

      let field = document.querySelector('.var-fixed-field');
      if (!field) errs.push('444: the fixed-value list did not appear once the flag was on');
      if (field && !field.querySelector('.var-fixed-empty')) {
        errs.push('444: the empty-state hint is missing before any fixed value exists');
      }

      if (field) {
        field.querySelector('.var-fixed-add-name').value = 'topic';
        field.querySelector('.var-fixed-add-value').value = 'coffee';
        field.querySelector('.var-fixed-add-btn').click();
        await sleep(150);
        field = document.querySelector('.var-fixed-field');
        const rows = field ? [...field.querySelectorAll('.var-fixed-row')] : [];
        if (rows.length !== 1) {
          errs.push('444: adding a fixed value did not produce exactly one row, found ' + rows.length);
        } else {
          const nameText = rows[0].querySelector('.var-fixed-name').textContent;
          if (nameText !== 'topic') errs.push('444: the added row shows the wrong name: ' + nameText);
          const valueInput = rows[0].querySelector('.var-fixed-value');
          if (!valueInput || valueInput.value !== 'coffee') {
            errs.push('444: the added row does not carry the expected value: ' + (valueInput && valueInput.value));
          }
        }
        if (field && field.querySelector('.var-fixed-empty')) {
          errs.push('444: the empty-state hint is still shown although a fixed value exists');
        }
        if (!(window.__SMOKE_CALLS__ || []).includes('set_ui_text')) {
          errs.push('444: adding a fixed value never persisted it');
        }

        if (rows.length === 1) {
          rows[0].querySelector('.var-fixed-del').click();
          await sleep(150);
          field = document.querySelector('.var-fixed-field');
          const rowsAfter = field ? field.querySelectorAll('.var-fixed-row').length : -1;
          if (rowsAfter !== 0) errs.push('444: deleting the fixed value did not remove its row, found ' + rowsAfter);
          if (!field || !field.querySelector('.var-fixed-empty')) {
            errs.push('444: the empty-state hint did not come back after the only fixed value was deleted');
          }
        }

        // Put one back for the fill-in dialog check below.
        field = document.querySelector('.var-fixed-field');
        if (field) {
          field.querySelector('.var-fixed-add-name').value = 'topic';
          field.querySelector('.var-fixed-add-value').value = 'coffee';
          field.querySelector('.var-fixed-add-btn').click();
          await sleep(150);
        }
      }
    }

    document.getElementById('expert-back').click();
    await sleep(100);
    document.getElementById('settings').classList.add('hidden');

    // A prompt with an unresolved placeholder must offer the fixed value as a
    // placeholder-only suggestion, marked, never inserted into the value.
    // The stub seeds the grid with its own fixture prompts, so the new one
    // has to be found by diffing tile ids, not by picking the first tile.
    const idsBefore = new Set([...document.querySelectorAll('#grid .tile')].map(el => el.dataset.id));
    const inputEl = document.getElementById('input');
    const saveBtn = document.getElementById('save');
    inputEl.value = 'Say hi to {#{topic}#}.';
    inputEl.dispatchEvent(new Event('input', { bubbles: true }));
    saveBtn.click();
    await sleep(200);
    const nameField = document.getElementById('modal-name');
    nameField.value = 'Fixed value test prompt';
    nameField.dispatchEvent(new Event('input', { bubbles: true }));
    document.getElementById('modal-confirm').click();
    await sleep(300);

    const tile = [...document.querySelectorAll('#grid .tile')].find(el => !idsBefore.has(el.dataset.id));
    if (!tile) {
      errs.push('444: the new prompt with a placeholder did not appear on the grid');
    } else {
      for (const type of ['pointerdown', 'pointerup', 'click']) {
        tile.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1 }));
      }
      await sleep(400);
      let fixedReq = null;
      try { fixedReq = JSON.parse(window.__VAR_PAYLOAD__ || 'null'); } catch (e) { fixedReq = null; }
      if (!fixedReq) {
        errs.push('444: copying a prompt with a placeholder did not open the placeholder window');
      } else {
        const row = fixedReq.rows[0];
        if (!row || row.options.length) {
          errs.push('444: the placeholder window has no text row for the placeholder');
        } else {
          if (row.value) errs.push('444: the fixed value was inserted instead of only suggested, got: ' + row.value);
          if (row.placeholder !== 'coffee') errs.push('444: the row placeholder does not carry the fixed value, got: ' + row.placeholder);
          if (!row.fixed) errs.push('444: the fixed-value marker flag is missing on the row');
        }
        window.__FIRE__('vars-filled', null);
        await sleep(150);
      }
    }
  } catch (e) { errs.push('444 probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

EXPERT_PRESET_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    const cm = document.getElementById('confirm-modal');
    for (let i = 0; i < 80 && cm.classList.contains('hidden'); i++) await sleep(100);
    if (!cm.classList.contains('hidden')) { document.getElementById('confirm-cancel').click(); await sleep(300); }

    document.getElementById('gear').click();
    await sleep(150);

    // 646: the Auto update switch and its interval are one setting. Never is the
    // switch off, any other interval turns it on, and nothing here asks the
    // backend to check for an update.
    {
      const sw = document.getElementById('opt-autoupdate');
      const sel = document.getElementById('opt-update-interval');
      const argsOf = (cmd) => (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === cmd);
      const checks = () => (window.__SMOKE_CALLS__ || []).filter((c) => c === 'check_update').length;
      if (!sel) {
        errs.push('646: the update interval select is missing from Settings');
      } else {
        const checksBefore = checks();
        if (!sw.checked || sel.value !== '24') errs.push('646: default should be on and daily, got ' + sw.checked + ' ' + sel.value);
        sel.value = '0';
        sel.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(150);
        if (sw.checked) errs.push('646: choosing Never left the Auto update switch on');
        if (!argsOf('set_auto_update').some((c) => c[1].enabled === false)) errs.push('646: choosing Never did not switch auto update off');
        sel.value = '12';
        sel.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(150);
        if (!sw.checked) errs.push('646: choosing an interval did not switch Auto update back on');
        if (!argsOf('set_ui_value').some((c) => c[1].key === 'updateCheckH' && c[1].value === 12)) errs.push('646: the interval was not stored');
        sw.checked = false;
        sw.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(100);
        if (sel.value !== '0') errs.push('646: switching Auto update off should show Never, got ' + sel.value);
        sw.checked = true;
        sw.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(100);
        if (sel.value !== '12') errs.push('646: switching Auto update on should show the stored interval, got ' + sel.value);
        if (checks() !== checksBefore) errs.push('646: changing the update setting made a check_update call');
      }
    }
    document.getElementById('expert-open').click();
    await sleep(200);

    const setFlagCalls = () => (window.__SMOKE_CALLS__ || []).filter((c) => c === 'set_ui_flag' || c === 'set_ui_value').length;
    // The last value a preset wrote for a key (the main script is a module, its own
    // state is out of reach, so the stubbed backend calls are the evidence).
    const lastValue = (key) => {
      const w = (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'set_ui_value' && c[1].key === key);
      return w.length ? w[w.length - 1][1].value : undefined;
    };
    const before = setFlagCalls();
    const quietBtn = document.getElementById('expert-preset-quiet');
    if (!quietBtn) {
      errs.push('447: the Quiet preset button is missing from the expert menu');
    } else {
      quietBtn.click();
      await sleep(200);
      if (cm.classList.contains('hidden')) {
        errs.push('447: applying a preset did not open the confirmation dialog');
      } else {
        const msg = document.getElementById('confirm-msg').textContent;
        const m = msg.match(/:\\s*(\\d+)/);
        const claimed = m ? parseInt(m[1], 10) : -1;
        if (claimed <= 0) errs.push('447: the confirmation dialog names no changed-setting count, got: ' + msg);
        document.getElementById('confirm-ok').click();
        await sleep(300);
        const actual = setFlagCalls() - before;
        if (lastValue('bubbleMs') !== 0 || lastValue('undoToastMs') !== 0) {
          errs.push('644: Quiet did not set the bubble and undo times to Off');
        }
        if (actual !== claimed) {
          errs.push('447: the dialog said ' + claimed + ' settings would change, but ' + actual + ' actually did');
        }
        for (const id of ['expert-preset-quiet', 'expert-preset-standard', 'expert-preset-full']) {
          const btn = document.getElementById(id);
          if (btn && (btn.classList.contains('active') || btn.getAttribute('aria-pressed') === 'true')) {
            errs.push('447: ' + id + ' stayed marked as the active profile after applying');
          }
        }
        // Nothing left to change: re-applying the same profile must report that
        // instead of reopening an empty confirmation dialog.
        quietBtn.click();
        await sleep(200);
        if (!cm.classList.contains('hidden')) {
          errs.push('447: re-applying an already-applied preset opened a confirmation dialog again');
          document.getElementById('confirm-cancel').click();
          await sleep(150);
        } else {
          const toastMsg = document.querySelector('#toast .toast-msg');
          if (!toastMsg || !toastMsg.textContent) {
            errs.push('447: re-applying an already-applied preset gave no feedback at all');
          }
        }
      }
    }
    // Standard puts the off-at-zero values back to their defaults.
    const stdBtn = document.getElementById('expert-preset-standard');
    if (stdBtn) {
      stdBtn.click();
      await sleep(200);
      if (!cm.classList.contains('hidden')) { document.getElementById('confirm-ok').click(); await sleep(300); }
      if (lastValue('bubbleMs') !== 950 || lastValue('undoToastMs') !== 8000) {
        errs.push('644: Standard did not restore the bubble and undo times');
      }
    }
  } catch (e) { errs.push('447 probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


def unescape(s):
    for a, b in (("&quot;", '"'), ("&lt;", "<"), ("&gt;", ">"), ("&amp;", "&")):
        s = s.replace(a, b)
    return s

# Folder window, drag-out on: a modifier-click that never moves is a plain copy,
# the same rule the pill states. No OS drag starts, so the click must not vanish.
FOLDER_DRAGOUT_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const calls = () => (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []);
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(100); return fn(); };
  try {
    await until(() => document.getElementById('fname').textContent !== '');
    const invoke = window.__TAURI__.core.invoke;
    await invoke('set_ui_flag', { key: 'autoPaste', enabled: false });
    await invoke('set_ui_value', { key: 'copyCooldownMs', value: 0 });
    await invoke('set_ui_flag', { key: 'dragOut', enabled: true });
    window.__FIRE__('settings-changed');
    await sleep(100);
    document.getElementById('icon').dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
    window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 }));
    const pop = document.getElementById('pop');
    await until(() => !pop.classList.contains('hidden'));
    const tile = document.querySelector('#grid .folder-member');
    if (!tile) {
      errs.push('the open folder rendered no members');
    } else {
      calls().length = 0;
      tile.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, buttons: 1, ctrlKey: true }));
      window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, buttons: 0 }));
      await until(() => calls().includes('copy_prompt'));
      if (!calls().includes('copy_prompt')) errs.push('a modifier-click without movement did not copy the member');
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# Pill window: closing the menu (Escape) while a size button's set_float_scale is
# still in flight must not leave an uncaught error behind. The probe wraps invoke
# before the window script captures it, so it can hold that one call back.
FLOAT_SCALE_RACE_PROBE = """
<script>
(() => {
  const real = window.__TAURI__.core.invoke;
  window.__SCALE_GATE__ = [];
  window.__TAURI__.core.invoke = async (cmd, args) => {
    const out = await real(cmd, args);
    if (cmd === 'set_float_scale' && window.__HOLD_SCALE__) {
      await new Promise((resolve) => window.__SCALE_GATE__.push(resolve));
    }
    return out;
  };
})();
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const calls = () => (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []);
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(50); return fn(); };
  try {
    await until(() => calls().includes('resize_float_pill'));
    window.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
    const menu = document.getElementById('menu');
    await until(() => !menu.classList.contains('hidden'));
    if (menu.classList.contains('hidden')) errs.push('the pill menu did not open');
    window.__HOLD_SCALE__ = true;
    document.querySelector('#menu [data-scale="2"]').click();
    await until(() => window.__SCALE_GATE__.length > 0);
    if (!window.__SCALE_GATE__.length) errs.push('the size button never called set_float_scale');
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await sleep(100);
    window.__HOLD_SCALE__ = false;
    window.__SCALE_GATE__.forEach((resolve) => resolve());
    await sleep(200);
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""



def main():
    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        # CI's runner image always ships Chrome (see ci.yml); a missing browser there
        # means the whole UI-smoke job silently stopped covering anything, which must
        # fail loudly instead of passing green with zero coverage.
        if os.environ.get("CI"):
            print("FAIL: no Chromium browser found on a CI runner")
            return 1
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
            wrapper = tmp / f"wrap-{size_tag}.html"
            wrapper.write_text(WRAPPER % (w, h), encoding="utf-8")
            # An isolated profile is required or Chrome hands the URL to a running
            # instance and prints nothing.
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-%s' % size_tag)}", "--allow-file-access-from-files",
                 # The probe walks more UI than it used to; 30s of virtual time ran
                 # out mid-run on the smallest window and produced no result at all;
                 # raised to 60s, then to 75s once the P10 quick-grid hover-only check
                 # (two more expert-menu round trips) pushed past that too, then to 95s
                 # once 91/51/70's regressions (trash dialog, multi-view popover, three
                 # restore-scope round trips) added their own expert-menu detours, then
                 # to 110s once 90-94's six new expert rows pushed past that too, then
                 # to 120s once the injected-markup grid/library check added its own
                 # create-and-verify round trip, then to 126s once 171's late-voice-text
                 # probe added its own editor round trip plus the 3s late-delivery grace
                 # window it has to sit through, then to 132s once the recording
                 # result dialog's probe added its own four expert-menu round trips,
                 # then to 138s once the tag-merge check added its own batch-select
                 # round trip (two disposable tags added before the merge itself),
                 # then to 146s once the copy-sound check added its own four
                 # copy-and-listen round trips plus a volume-slider detour,
                 # then to 149s once the number-field check added its own
                 # click/hold/wheel/preset/disabled-state round trip, then to
                 # 151s once the media-viewer zoom check added its own
                 # prompt-editor-to-lightbox round trip, then to 154s once the
                 # editor-scale check added its own draw/resize/undo round trip,
                 # then to 172s once the sync-folder check added three full
                 # question-and-answer rounds, each with its own tab walk, then
                 # to 220s once the undo-toast checks (434) added six of their
                 # own round trips (view delete, tag delete, copy history,
                 # clipboard inbox, batch delete, batch tag add/remove).
                 # The window has to hold the iframe plus Chrome's own frame.
                 f"--window-size={max(w + 30, 520)},{h + 140}",
                 "--virtual-time-budget=250000", "--dump-dom",
                 wrapper.as_uri()],
                # The dumped DOM carries all 20 languages; without an explicit
                # codec Windows decodes it as cp1252 and the reader thread dies,
                # leaving stdout empty.
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=270)
            m = re.search(r'<pre id="SMOKE">(\{.*?\})</pre>', proc.stdout, re.S)
            if not m:
                print(f"FAIL {w}x{h}: the UI produced no probe result")
                failed += 1
                continue
            r = json.loads(unescape(m.group(1)))
            print(f"-- {w}x{h} — expert tabs: {', '.join(r['tabs']) or 'none'}")
            for kind, items in (("JS error", r["errors"]),
                                ("overflow", sorted(set(r["overflow"]))),
                                ("layout", r.get("layout", [])),
                                ("untranslated", sorted(set(r["rawKeys"])))):
                for it in items:
                    print(f"   FAIL {kind}: {it}")
                failed += len(items)
            if not (r["errors"] or r["overflow"] or r["rawKeys"] or r.get("layout")):
                print("   ok")

        # 221: a pass in the longest-running UI language (Italian: averages ~42
        # chars/string across all 1231 keys, the longest of the 11 by total
        # character count) at the two tightest window sizes, where a long
        # translation is most likely to overflow, overlap or push a button
        # out of reach. Same page/probe as the English sweep above, only with
        # SETTINGS.language overridden before main.js ever runs.
        it_page = tmp / "ui" / "index-it.html"
        it_page.write_text(
            html.replace(
                '<script src="tauri_stub.js"></script>',
                '<script src="tauri_stub.js"></script>\n<script>SETTINGS.language = "it";</script>',
                1,
            ),
            encoding="utf-8",
        )
        for w, h in (SIZES[-1], SIZES[-2]):
            size_tag = f"it-{w}x{h}"
            wrapper = tmp / f"wrap-{size_tag}.html"
            wrapper.write_text(WRAPPER.replace("ui/index.html", "ui/index-it.html") % (w, h), encoding="utf-8")
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-%s' % size_tag)}", "--allow-file-access-from-files",
                 f"--window-size={max(w + 30, 520)},{h + 140}",
                 # Raised from 126s once the clipboard-secret expert-menu check
                 # (password-skip switch plus its length slider, each gated on the
                 # other) added its own round of tab walks and switch toggles, then
                 # from 134s once the Capture tab split (372) added a 7th expert
                 # tab -- lengthening every existing tab walk by one click -- plus
                 # its own group-survives-the-split probe, itself two full walks.
                 # Kept equal to the English sweep's own budget just above, since
                 # this runs the exact same probe script and had fallen behind it.
                 "--virtual-time-budget=250000", "--dump-dom",
                 wrapper.as_uri()],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=270)
            m = re.search(r'<pre id="SMOKE">(\{.*?\})</pre>', proc.stdout, re.S)
            if not m:
                print(f"FAIL it {w}x{h}: the UI produced no probe result")
                failed += 1
                continue
            r = json.loads(unescape(m.group(1)))
            print(f"-- it {w}x{h} — expert tabs: {', '.join(r['tabs']) or 'none'}")
            # Only the language-agnostic categories: overflow/layout/untranslated
            # are exactly what 221 asks for (truncated text, overlaps,
            # unreachable buttons). The probe's own "errors" array also carries
            # its behavioral assertions (exact English button labels, toast
            # text, dialog titles), which are EXPECTED to read differently in
            # Italian and are not a layout bug -- checked once already by the
            # English sweep above, so re-checking them here would only be noise.
            for kind, items in (("overflow", sorted(set(r["overflow"]))),
                                ("layout", r.get("layout", [])),
                                ("untranslated", sorted(set(r["rawKeys"])))):
                for it_ in items:
                    print(f"   FAIL {kind}: {it_}")
                failed += len(items)
            uncaught = [e for e in r["errors"] if e.startswith("error: ") or e.startswith("rejection: ")]
            for e in uncaught:
                print(f"   FAIL JS error: {e}")
            failed += len(uncaught)
            if not (uncaught or r["overflow"] or r["rawKeys"] or r.get("layout")):
                print("   ok")

        # Each secondary window carries the only coverage its own UI can get: the
        # pill drag/merge, the colour picker and the whole folder window live here
        # and nowhere else.
        for name, size, probe, budget in (
                ("floating.html", (240, 60), FLOAT_PROBE, 15000),
                ("snip.html", (1280, 800), PICK_PROBE + REGION_PROBE, 28000),
                ("folder.html", (520, 420), FOLDER_PROBE, 15000),
                ("vars.html", (340, 250), VARS_PROBE, 15000),
                ("launcher.html", (560, 420), LAUNCHER_PROBE, 15000),
                ("launcher_prefill.html", (560, 420), LAUNCHER_PREFILL_PROBE, 15000),
                ("tray.html", (240, 360), TRAY_PROBE, 15000),
                ("recbar.html", (320, 56), RECBAR_PROBE, 15000),
                ("camera.html", (420, 620), CAMERA_STUB + CAMERA_PROBE, 15000),
                ("camera_rec.html", (420, 620), CAMERA_REC_STUB + CAMERA_REC_PROBE, 30000)):
            if name == "camera_rec.html":
                shutil.copyfile(REPO / "ui" / "camera.html", tmp / "ui" / name)
            if name == "launcher_prefill.html":
                shutil.copyfile(REPO / "ui" / "launcher.html", tmp / "ui" / name)
            side = tmp / "ui" / name
            side.write_text(
                side.read_text(encoding="utf-8")
                .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
                .replace("</body>", probe + "</body>"),
                encoding="utf-8")
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-%s' % name)}", "--allow-file-access-from-files",
                 f"--window-size={size[0]},{size[1]}", f"--virtual-time-budget={budget}",
                 "--dump-dom", side.as_uri()],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=180)
            m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
            errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
            print(f"-- {name}")
            for e in errs:
                print(f"   FAIL JS error: {e}")
            failed += len(errs)
            if not errs:
                print("   ok")

        # The video editor keeps its own window config (1100x720 default, 760x520
        # minimum -- both well above the ~500px floor the generic loop above never
        # has to think about) rather than the single fixed size every other
        # secondary window uses, so it gets its own two-size run instead of a slot
        # in that tuple list: the file is injected once, then run twice with a
        # different --window-size, same as the sealed/unsealed index.html pair below.
        video_side = tmp / "ui" / "video.html"
        video_side.write_text(
            video_side.read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", VIDEO_PROBE + "</body>"),
            encoding="utf-8")
        for w, h in ((1100, 720), (760, 520)):
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-video-%dx%d' % (w, h))}", "--allow-file-access-from-files",
                 # Same "headless Chrome eats part of the requested window" offset
                 # the main sweep already compensates for (see WRAPPER above) --
                 # without it "760x520" never actually delivers that innerWidth/
                 # innerHeight to the page, and the min-size run silently tests a
                 # bigger window than the one this feature ships with.
                 f"--window-size={w + 30},{h + 140}", "--virtual-time-budget=30000",
                 "--dump-dom", video_side.as_uri()],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=180)
            m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
            errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
            print(f"-- video.html {w}x{h}")
            for e in errs:
                print(f"   FAIL JS error: {e}")
            failed += len(errs)
            if not errs:
                print("   ok")

        # 385: the remembered-unfinished-edit restore dialog, both answers --
        # its own boot each time, same reasoning as the seal/write-fail pairs below.
        edit_state_fixture = json.dumps(json.dumps({
            "v": 1,
            "ops": [
                {"tool": "trim", "startMs": 0, "endMs": 2000},
                {"tool": "fade", "inMs": 100, "outMs": 200},
            ],
        }))
        for action in ("continue", "discard"):
            side = tmp / "ui" / f"video-restore-{action}.html"
            side.write_text(
                (REPO / "ui" / "video.html").read_text(encoding="utf-8")
                .replace("<script",
                         f'<script>window.__EDIT_STATE_FIXTURE__ = {edit_state_fixture};'
                         f'window.__EDIT_STATE_RESTORE_ACTION__ = "{action}";</script>\n'
                         '<script src="tauri_stub.js"></script>\n<script', 1)
                .replace("</body>", EDIT_STATE_PROBE + "</body>"),
                encoding="utf-8")
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-video-restore-%s' % action)}", "--allow-file-access-from-files",
                 "--window-size=1130,860", "--virtual-time-budget=30000",
                 "--dump-dom", side.as_uri()],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=180)
            m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
            errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
            print(f"-- video.html restore dialog ({action})")
            for e in errs:
                print(f"   FAIL: {e}")
            failed += len(errs)
            if not errs:
                print("   ok")

        # A 1280x720 source: the fixed-height presets may enlarge.
        side = tmp / "ui" / "video-scale-up.html"
        side.write_text(
            (REPO / "ui" / "video.html").read_text(encoding="utf-8")
            .replace("<script",
                     '<script>window.__PROBE_W__ = 1280; window.__PROBE_H__ = 720;</script>\n'
                     '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", SCALE_UP_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-video-scale-up'}", "--allow-file-access-from-files",
             "--window-size=1130,860", "--virtual-time-budget=30000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- video.html scale presets on a 720p source")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # A second recording arriving while the first one's restore dialog is open.
        side = tmp / "ui" / "video-restore-superseded.html"
        side.write_text(
            (REPO / "ui" / "video.html").read_text(encoding="utf-8")
            .replace("<script",
                     f'<script>window.__EDIT_STATE_FIXTURE__ = {edit_state_fixture};</script>\n'
                     '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", EDIT_STATE_SUPERSEDE_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-video-restore-superseded'}", "--allow-file-access-from-files",
             "--window-size=1130,860", "--virtual-time-budget=30000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- video.html restore dialog (superseded by a second recording)")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 420: dragging a grid tile onto a view tab. Its own boot, same reasoning
        # as the restore-dialog and store-sealing runs below: window.__DRAG_VIEW_TEST__
        # shrinks "work" to its own single cell so the probe can fill and exhaust
        # it cheaply, a shape the main sweep above never sets up.
        side = tmp / "ui" / "drag-view.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script",
                     '<script>window.__DRAG_VIEW_TEST__ = true;</script>\n'
                     '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", DRAG_VIEW_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-drag-view'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- drag a tile onto a view tab")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # The store-sealing warning, both ways round. Its own boot each time: the
        # dialog only ever opens during startup, so it cannot be reached by walking
        # an already-running page. Wide enough that the settings dialog is not also
        # fighting for room; the small-window layout is the main sweep's job.
        for sealed in (False, True):
            tag = "sealed" if sealed else "unsealed"
            side = tmp / "ui" / f"seal-{tag}.html"
            side.write_text(
                (REPO / "ui" / "index.html").read_text(encoding="utf-8")
                .replace("<script",
                         f'<script>window.__STORE_SEALED__ = {str(sealed).lower()};</script>\n'
                         '<script src="tauri_stub.js"></script>\n<script', 1)
                .replace("</body>", SEAL_PROBE + "</body>"),
                encoding="utf-8")
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-seal-%s' % tag)}", "--allow-file-access-from-files",
                 "--window-size=1000,780", "--virtual-time-budget=45000",
                 "--dump-dom", side.as_uri()],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=180)
            m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
            errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
            print(f"-- store sealing ({tag} store)")
            for e in errs:
                print(f"   FAIL: {e}")
            failed += len(errs)
            if not errs:
                print("   ok")

        # 390/455: the first-run tour's draft-buffered write path, both ways round
        # (Finish writes only what was touched, Skip writes nothing but tourSeen).
        # Its own boot each time, same reasoning as the store-sealing pair above.
        for mode in ("finish", "skip"):
            side = tmp / "ui" / f"tour-firstrun-{mode}.html"
            side.write_text(
                (REPO / "ui" / "index.html").read_text(encoding="utf-8")
                .replace("<script",
                         f'<script>window.__TOUR_FIRSTRUN_TEST__ = true; window.__TOUR_FIRSTRUN_MODE__ = "{mode}";</script>\n'
                         '<script src="tauri_stub.js"></script>\n<script', 1)
                .replace("</body>", TOUR_FIRSTRUN_PROBE + "</body>"),
                encoding="utf-8")
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-tour-firstrun-%s' % mode)}", "--allow-file-access-from-files",
                 "--window-size=1000,780", "--virtual-time-budget=45000",
                 "--dump-dom", side.as_uri()],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=180)
            m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
            errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
            print(f"-- first-run tour draft buffering ({mode})")
            for e in errs:
                print(f"   FAIL: {e}")
            failed += len(errs)
            if not errs:
                print("   ok")

        # 458: a confidential entry, locked then unlocked -- its own boot, same
        # reasoning as the store-sealing pair above (the fixture needs its own
        # dedicated prompt and a vault state the main sweep never sets up).
        side = tmp / "ui" / "secret.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script",
                     '<script>window.__SECRET_TEST__ = true; window.__SECRET_PRECONFIGURED__ = true;</script>\n'
                     '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", SECRET_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-secret'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- confidential prompt: locked bar, password prompt, unlock")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 276: the settings-write-failure notice, both ways round -- same reasoning
        # as the store-sealing pair above (its own boot, since the startup dialog
        # only ever opens during startup).
        for wfailed in (True, False):
            tag = "failed" if wfailed else "ok"
            side = tmp / "ui" / f"writefail-{tag}.html"
            side.write_text(
                (REPO / "ui" / "index.html").read_text(encoding="utf-8")
                .replace("<script",
                         f'<script>window.__SETTINGS_WRITE_FAILED__ = {str(wfailed).lower()};</script>\n'
                         '<script src="tauri_stub.js"></script>\n<script', 1)
                .replace("</body>", WRITE_FAIL_PROBE + "</body>"),
                encoding="utf-8")
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-writefail-%s' % tag)}", "--allow-file-access-from-files",
                 "--window-size=1000,780", "--virtual-time-budget=45000",
                 "--dump-dom", side.as_uri()],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=180)
            m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
            errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
            print(f"-- settings write ({tag})")
            for e in errs:
                print(f"   FAIL: {e}")
            failed += len(errs)
            if not errs:
                print("   ok")

        # The data-folder lock: a fresh foreign lock opens the store read-only
        # and says so in a persistent line, same two-boot shape as the pair above.
        for ro in (True, False):
            tag = "read-only" if ro else "writable"
            side = tmp / "ui" / f"ddlock-{tag}.html"
            extra = (
                f'<script>window.__DATA_DIR_READONLY__ = {str(ro).lower()};'
                f'window.__DATA_DIR_OTHER_HOST__ = "OTHER-PC";</script>\n'
                '<script src="tauri_stub.js"></script>\n<script'
            )
            side.write_text(
                (REPO / "ui" / "index.html").read_text(encoding="utf-8")
                .replace("<script", extra, 1)
                .replace("</body>", DATA_DIR_READONLY_PROBE + "</body>"),
                encoding="utf-8")
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-ddlock-%s' % tag)}", "--allow-file-access-from-files",
                 "--window-size=1000,780", "--virtual-time-budget=45000",
                 "--dump-dom", side.as_uri()],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=180)
            m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
            errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
            print(f"-- data folder lock ({tag})")
            for e in errs:
                print(f"   FAIL: {e}")
            failed += len(errs)
            if not errs:
                print("   ok")

        # 375/483: the grid-folder and pill-folder "ungroup" menu
        # entries must all read and look the same -- two separate boots (the
        # pill is its own window, same reasoning as the FOLDER_PROBE run
        # above), then the pill's reading is checked against the grid
        # folder's here in Python since the two never share a page.
        side = tmp / "ui" / "folder-wording.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", FOLDER_WORDING_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-folder-wording'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\{.*?\})</pre>', proc.stdout, re.S)
        result = json.loads(unescape(m.group(1))) if m else {"errs": ["no probe result"], "gridDissolve": None}
        errs = list(result.get("errs") or [])
        grid_dissolve = result.get("gridDissolve")

        # Written as "folder.html" itself, not a differently-named copy: the
        # window reads its own filename off location.pathname to decide it is
        # the folder window (same as the FOLDER_PROBE boot above), so a
        # renamed file would load the wrong fixture.
        side = tmp / "ui" / "folder.html"
        side.write_text(
            (REPO / "ui" / "folder.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", PILL_WORDING_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-pill-wording'}", "--allow-file-access-from-files",
             "--window-size=520,420", "--virtual-time-budget=15000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\{.*?\})</pre>', proc.stdout, re.S)
        pill_result = json.loads(unescape(m.group(1))) if m else {"errs": ["no probe result"], "pillDissolve": None}
        errs.extend(pill_result.get("errs") or [])
        pill_dissolve = pill_result.get("pillDissolve")

        if grid_dissolve and pill_dissolve:
            if grid_dissolve.get("label") != pill_dissolve.get("label"):
                errs.append(
                    '483: the pill folder and grid folder ungroup entries read differently ("' +
                    str(pill_dissolve.get("label")) + '" vs "' + str(grid_dissolve.get("label")) + '")'
                )
            if not pill_dissolve.get("icon") or pill_dissolve.get("icon") != grid_dissolve.get("icon"):
                errs.append("483: the pill folder and grid folder ungroup entries do not share the same icon markup")

        print("-- folder ungroup wording")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 376: the tile hotkey badge, its wording and the live flag toggle.
        side = tmp / "ui" / "tile-hotkey-badge.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", TILE_HOTKEY_BADGE_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-tile-hotkey-badge'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- tile hotkey badge")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 459: demo mode's marker banner and its confirm-before-switch-back.
        side = tmp / "ui" / "demo-mode.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script",
                     '<script>window.__DEMO_TEST__ = true;</script>\n'
                     '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", DEMO_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-demo-mode'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- demo mode")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 378: an elevation attempt refused because the account can never
        # elevate gets its own message and no retry button; a real transient
        # refusal keeps its retry button.
        side = tmp / "ui" / "cap-needs-admin.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", CAP_NEEDS_ADMIN_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-cap-needs-admin'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- OCR/voice elevation: no admin rights vs transient failure")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 404: whats-new toast, window, About-dialog entry and GitHub refresh.
        side = tmp / "ui" / "whats-new.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", WHATS_NEW_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-whats-new'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- whats new: toast, window, About entry, GitHub refresh")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 451: third-party license list in the About window.
        side = tmp / "ui" / "licenses.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", LICENSES_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-licenses'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- third-party license list in the About window")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 411: the parked marker, once on the pill and once on the folder icon --
        # both windows read the same window.__FLOAT_PARKED__ fixture off the
        # stub, so one probe body covers both boots.
        for name, size in (("floating.html", (240, 60)), ("folder.html", (520, 420))):
            w, h = size
            side = tmp / "ui" / f"parked-{name}"
            side.write_text(
                (REPO / "ui" / name).read_text(encoding="utf-8")
                .replace("<script",
                         '<script>window.__FLOAT_PARKED__ = true;</script>\n'
                         '<script src="tauri_stub.js"></script>\n<script', 1)
                .replace("</body>", PARKED_BADGE_PROBE + "</body>"),
                encoding="utf-8")
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-parked-%s' % name)}", "--allow-file-access-from-files",
                 f"--window-size={w},{h}", "--virtual-time-budget=15000",
                 "--dump-dom", side.as_uri()],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=180)
            m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
            errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
            print(f"-- parked marker ({name})")
            for e in errs:
                print(f"   FAIL: {e}")
            failed += len(errs)
            if not errs:
                print("   ok")

        # 411: the float-recall expert action.
        side = tmp / "ui" / "float-recall.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", FLOAT_RECALL_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-float-recall'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- float-recall expert action")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 475: the video editor's extra-source limit reads its own expert value
        # instead of a fixed constant. Its own boot, same reasoning as the
        # restore-dialog and float-recall runs above.
        side = tmp / "ui" / "extra-clips-max.html"
        side.write_text(
            (REPO / "ui" / "video.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", EXTRA_CLIPS_MAX_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-extra-clips-max'}", "--allow-file-access-from-files",
             "--window-size=1130,860", "--virtual-time-budget=30000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- video editor extra-source limit as an expert value")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 413: the retention overview in the statistics/backups window -- row
        # order, amount wording, jump-to-setting and a cleanup button's round
        # trip. Its own boot, same reasoning as the other backups-dialog runs.
        side = tmp / "ui" / "retention-panel.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", RETENTION_PANEL_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-retention-panel'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- retention overview in the statistics/backups window")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 443: editor draft autosave -- a crash draft found at boot, the
        # one-time recovery question, declining it, and a later ordinary
        # editor open never re-asking.
        side = tmp / "ui" / "editor-draft.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", EDITOR_DRAFT_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-editor-draft'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- editor draft: crash recovery offered once, decline discards it")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 444: fixed values for recurring placeholders -- expert-menu list
        # (add/edit/delete) and the fill-in dialog's marker + suggestion.
        side = tmp / "ui" / "var-fixed.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", VAR_FIXED_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-var-fixed'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- fixed placeholder values: list editor and fill-in suggestion")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # 447: expert-menu quick profiles -- applying one asks first, the count
        # in that dialog matches the number of settings actually written, and
        # no profile stays marked as active once it is done.
        side = tmp / "ui" / "expert-preset.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", EXPERT_PRESET_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-expert-preset'}", "--allow-file-access-from-files",
             "--window-size=1000,780", "--virtual-time-budget=45000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- expert menu quick profiles: change count and no lasting marker")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

        # Folder window with drag-out on: a modifier-click without movement copies.
        side = tmp / "ui" / "folder-dragout.html"
        side.write_text(
            (REPO / "ui" / "folder.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", FOLDER_DRAGOUT_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-folder-dragout'}", "--allow-file-access-from-files",
             "--window-size=520,420", "--virtual-time-budget=15000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- folder window: modifier-click without movement copies")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")
        # Pill window: Escape during an in-flight size change throws nothing.
        side = tmp / "ui" / "floating-scale-race.html"
        side.write_text(
            (REPO / "ui" / "floating.html").read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", FLOAT_SCALE_RACE_PROBE + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-float-scale-race'}", "--allow-file-access-from-files",
             "--window-size=240,60", "--virtual-time-budget=15000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- pill window: closing the menu during a size change")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")
        # Video editor expert values that are NOT at their shipped default.
        # The stub is seeded before video.html loads, so the boot-time settings
        # read is what gets exercised (the backend never sends a live settings
        # event to this window). Its own boot, same reasoning as the runs above.
        video_expert_probe = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50);
    return fn();
  };
  const fieldOf = (label) => {
    const row = [...document.querySelectorAll('#ve-ov-editor label.field')]
      .find((l) => l.firstElementChild && l.firstElementChild.textContent === label);
    return row ? row.querySelector('input,select') : null;
  };
  const exportCalls = () => (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'export_video');
  const overlayRows = () => document.querySelectorAll('#ve-ov-list .ve-ov-row').length;
  try {
    await until(() => document.getElementById('ve-stage-msg').classList.contains('hidden') &&
      document.querySelectorAll('#ve-filmstrip .ve-thumb').length > 0, 300);
    await until(() => document.getElementById('ve-scale-w').value === '1280', 100);

    // Panel, export format, quality and scale preset open on the seeded values.
    if (!document.getElementById('ve-tab-export').classList.contains('active')) {
      errs.push('537: the seeded default panel (export) was not the open tab');
    }
    if (document.getElementById('ve-export-format').value !== 'webm') {
      errs.push('537: the seeded default format webm was ignored, got ' + document.getElementById('ve-export-format').value);
    }
    if (document.getElementById('ve-export-quality').value !== 'small') {
      errs.push('537: the seeded default quality small was ignored, got ' + document.getElementById('ve-export-quality').value);
    }
    const sw = document.getElementById('ve-scale-w').value, sh = document.getElementById('ve-scale-h').value;
    if (sw !== '1280' || sh !== '720') {
      errs.push('537: the seeded default scale 720 was ignored, got ' + sw + 'x' + sh);
    }
    // The initial fade fields.
    if (document.getElementById('ve-fade-in').value !== '900' || document.getElementById('ve-fade-out').value !== '900') {
      errs.push('537: the seeded fade length 900 was ignored, got ' + document.getElementById('ve-fade-in').value + '/' + document.getElementById('ve-fade-out').value);
    }

    // Export state: bitrate of the small tier scaled to 720p of a 1080p source
    // (3000 * 720 / 1080), then the high tier (9000 * 720 / 1080), with the fade.
    document.getElementById('ve-fade-on').click();
    await sleep(80);
    document.getElementById('ve-export-start').click();
    await until(() => exportCalls().length > 0, 100);
    let last = exportCalls().pop();
    let req = last && last[1] && last[1].req;
    if (!req) errs.push('537: no export request was made');
    else {
      if (req.format !== 'webm') errs.push('537: the export format was ' + req.format + ', expected the seeded webm');
      if (req.video_kbps !== 2000) errs.push('537: the small-tier bitrate was ' + req.video_kbps + ', expected the seeded 3000 scaled to 2000');
      const chain = String(req.filter_chain);
      if (!/fade=t=in:st=0:d=0[.]900/.test(chain)) errs.push('537: the export fade-in did not use the seeded 900 ms: ' + chain);
    }
    await until(() => !document.getElementById('ve-export-start').disabled, 100);
    const before = exportCalls().length;
    const q = document.getElementById('ve-export-quality');
    q.value = 'high';
    q.dispatchEvent(new Event('change', { bubbles: true }));
    await sleep(80);
    document.getElementById('ve-export-start').click();
    await until(() => exportCalls().length > before, 100);
    last = exportCalls().pop();
    req = last && last[1] && last[1].req;
    if (!req || req.video_kbps !== 6000) {
      errs.push('537: the high-tier bitrate was ' + (req && req.video_kbps) + ', expected the seeded 9000 scaled to 6000');
    }

    // A new overlay of each kind starts from the seeded values.
    document.getElementById('ve-tab-overlays').click();
    await sleep(60);
    document.getElementById('ve-ov-add-pixelate').click();
    await until(() => overlayRows() === 1, 60);
    const block = fieldOf('Block size (px)');
    if (!block || block.value !== '24') errs.push('537: a new pixelate overlay did not use the seeded block size 24, got ' + (block && block.value));
    const rectX = fieldOf('X');
    const x0 = rectX ? Number(rectX.value) : NaN;
    // Duplicate through the row context menu: shifted by the seeded offset.
    document.querySelector('#ve-ov-list .ve-ov-row').dispatchEvent(
      new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 200, clientY: 200 }));
    await sleep(60);
    const dupBtn = [...document.querySelectorAll('body > .ctx[role=menu] button')].find((b) => !b.classList.contains('danger'));
    if (!dupBtn) errs.push('537: the overlay context menu had no duplicate button');
    else {
      dupBtn.click();
      await until(() => overlayRows() === 2, 60);
      const dx = fieldOf('X') ? Number(fieldOf('X').value) - x0 : NaN;
      if (dx !== 40) errs.push('537: the duplicated overlay moved by ' + dx + ' px, expected the seeded 40');
    }
    document.getElementById('ve-ov-add-spotlight').click();
    await until(() => overlayRows() === 3, 60);
    const dim = fieldOf('Darkening (%)');
    if (!dim || dim.value !== '70') errs.push('537: a new spotlight did not use the seeded darkening 70, got ' + (dim && dim.value));
    document.getElementById('ve-ov-add-text').click();
    await until(() => overlayRows() === 4, 60);
    const font = fieldOf('Font');
    if (!font || font.value !== 'consola') errs.push('537: a new text overlay did not use the seeded font consola, got ' + (font && font.value));
    const sel = document.querySelector('#ve-ov-editor .swatch.sel');
    if (!sel || String(sel.title).toLowerCase() !== '#ef4444') errs.push('537: a new text overlay did not use the seeded colour #ef4444, got ' + (sel && sel.title));
    document.getElementById('ve-ov-add-image').click();
    await until(() => overlayRows() === 5, 60);
    const opac = fieldOf('Opacity (%)');
    if (!opac || opac.value !== '40') errs.push('537: a new image overlay did not use the seeded opacity 40, got ' + (opac && opac.value));
  } catch (e) { errs.push('537: probe threw: ' + e); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""
        seed = (
            '<script>window.__SEED_UI_VALUES__ = {"veExportKbpsHigh": 9000, "veExportKbpsSmall": 3000, '
            '"veFadeMs": 900, "veOvPixelateBlock": 24, "veOvSpotlightDim": 70, "veOvImageOpacity": 40, '
            '"veOvDupOffsetPx": 40};'
            'window.__SEED_UI_TEXTS__ = {"veDefaultFormat": "webm", "veDefaultQuality": "small", '
            '"veDefaultScale": "720", "veOvColor": "#ef4444", "veOvFont": "consola", '
            '"veDefaultPanel": "export"};</script>\n')
        side = tmp / "ui" / "video-expert-seeded.html"
        side.write_text(
            (REPO / "ui" / "video.html").read_text(encoding="utf-8")
            .replace("<script", seed + '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", video_expert_probe + "</body>"),
            encoding="utf-8")
        proc = subprocess.run(
            [browser, "--headless=new", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={tmp / 'profile-video-expert-seeded'}", "--allow-file-access-from-files",
             "--window-size=1130,860", "--virtual-time-budget=30000",
             "--dump-dom", side.as_uri()],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print("-- video.html seeded non-default expert values")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")
        # ---- main window findings: generated block begin ----
        ar_scaffold = r"""
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => { for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50); return fn(); };
  try {
%s
%s
  } catch (e) { errs.push('probe threw: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

        def ar_probe(tag, label, page, size, pre, body):
            side = tmp / "ui" / f"ar-{tag}.html"
            side.write_text(
                (REPO / "ui" / f"{page}.html").read_text(encoding="utf-8")
                .replace("<script", pre + '<script src="tauri_stub.js"></script>\n<script', 1)
                .replace("</body>", ar_scaffold % (ar_common, body) + "</body>"),
                encoding="utf-8")
            w, h = size.split("x")
            proc = subprocess.run(
                [browser, "--headless=new", "--disable-gpu", "--no-first-run",
                 f"--user-data-dir={tmp / ('profile-ar-' + tag)}", "--allow-file-access-from-files",
                 f"--window-size={w},{h}", "--virtual-time-budget=45000",
                 "--dump-dom", side.as_uri()],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=180)
            m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
            errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
            print(f"-- {label}")
            for e in errs:
                print(f"   FAIL: {e}")
            if not errs:
                print("   ok")
            return len(errs)

        ar_common = r"""
    const visible = (el) => !!el && !el.classList.contains('hidden');
    const callsFor = (cmd) => (window.__SMOKE_ARGS__ || []).filter(([c]) => c === cmd).map(([, a]) => a);
    const tileEl = (id) => document.querySelector('#grid .tile[data-id="' + id + '"]');
    const centre = (el) => { const r = el.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
    const ptr = (type, x, y, el, extra) => (el || window).dispatchEvent(new PointerEvent(type, Object.assign({
      bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 6, clientX: x, clientY: y,
    }, extra || {})));
    const dragTo = async (tile, tx, ty) => {
      const [sx, sy] = centre(tile);
      ptr('pointerdown', sx, sy, tile);
      ptr('pointermove', sx + 8, sy - 8, tile);
      await sleep(60);
      ptr('pointermove', tx, ty, window);
      await sleep(60);
    };
    const endDrag = async (x, y) => { ptr('pointerup', x, y, window); await sleep(300); };
    const bootReady = async () => {
      await until(() => document.querySelectorAll('#grid .tile').length > 0, 200);
      const cm = document.getElementById('confirm-modal');
      for (let i = 0; i < 60 && !visible(cm); i++) await sleep(50);
      if (visible(cm)) { document.getElementById('confirm-cancel').click(); await sleep(300); }
    };
    const cellEl = (c, r) => document.querySelector('#grid .cell[data-col="' + c + '"][data-row="' + r + '"]');
"""

        failed += ar_probe('f1', 'grid folder: a tile dropped on its cell sends the folder to the vacated cell', 'index', '1000x780', '', r"""
    await bootReady();
    const folderCell = () => { const c = document.querySelector('#grid .cell .folder-tile'); return c ? [Number(c.parentElement.dataset.col), Number(c.parentElement.dataset.row)] : null; };
    const tabFor = (id) => document.querySelector('.view-btn[data-view-id="' + id + '"]');
    const before = folderCell();
    if (!before || before[0] !== 0 || before[1] !== 1) errs.push('563-1: fixture folder not at [0,1], got ' + JSON.stringify(before));
    // p9 sits at [1,2]; the first free cell after the move would be [2,1], not the vacated one
    const srcCell = tileEl('p9').parentElement;
    const old = [Number(srcCell.dataset.col), Number(srcCell.dataset.row)];
    await dragTo(tileEl('p9'), ...centre(cellEl(0, 1)));
    await endDrag(...centre(cellEl(0, 1)));
    const p9At = tileEl('p9').parentElement;
    if (p9At.dataset.col !== '0' || p9At.dataset.row !== '1') errs.push('563-1: the drop did not land p9 on [0,1]');
    // a full render from the stored layout, as any later refresh does
    tabFor('work').click(); await sleep(250);
    tabFor('home').click(); await sleep(300);
    const after = folderCell();
    if (!after || after[0] !== old[0] || after[1] !== old[1]) {
      errs.push('563-1: dropping a tile on the folder cell should put the folder on the vacated cell ' + JSON.stringify(old) + ', got ' + JSON.stringify(after));
    }
""")
        failed += ar_probe('f2', 'grid undo toast: a tab switch before Undo leaves both views alone', 'index', '1000x780', '', r"""
    await bootReady();
    const tabFor = (id) => document.querySelector('.view-btn[data-view-id="' + id + '"]');
    const before = JSON.stringify(layoutRead());
    function layoutRead() {
      const o = {};
      document.querySelectorAll('#grid .cell .tile:not(.folder-tile)').forEach((t) => { o[t.dataset.id] = t.parentElement.dataset.col + ',' + t.parentElement.dataset.row; });
      return o;
    }
    // p9 (cell 1,2) moves to the empty cell 2,2 and the undo toast appears
    await dragTo(tileEl('p9'), ...centre(cellEl(2, 2)));
    await endDrag(...centre(cellEl(2, 2)));
    const movedTo = tileEl('p9') && tileEl('p9').parentElement;
    if (!movedTo || movedTo.dataset.col !== '2' || movedTo.dataset.row !== '2') errs.push('563-2: the move to 2,2 did not happen');
    const btn = document.querySelector('#toast .toast-btn');
    if (!btn) { errs.push('563-2: no undo toast after the move'); }
    else {
      tabFor('work').click(); await sleep(300);
      const work = JSON.stringify(layoutRead());
      window.__SMOKE_ARGS__.length = 0;
      btn.click(); await sleep(300);
      const removes = callsFor('batch_prompts').filter((a) => a && a.action === 'removeView');
      const layouts = callsFor('set_layout');
      if (removes.length) errs.push('563-2: Undo after a tab switch removed the prompt from another view: ' + JSON.stringify(removes));
      if (layouts.length) errs.push('563-2: Undo after a tab switch rewrote the layout of the view on screen');
      if (JSON.stringify(layoutRead()) !== work) errs.push('563-2: the view on screen changed by Undo');
      tabFor('home').click(); await sleep(300);
      const p9 = tileEl('p9') && tileEl('p9').parentElement;
      if (!p9 || p9.dataset.col !== '2' || p9.dataset.row !== '2') errs.push('563-2: the original view lost the moved tile position');
    }
""")
        failed += ar_probe('f4', 'favorites grid: switching to it clears the grid selection', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { favViewButton: true } };</script>\n', r"""
    await bootReady();
    const bar = document.getElementById('grid-batch-bar');
    tileEl('p1').querySelector('.tile-select').click();
    await sleep(150);
    if (bar.classList.contains('hidden')) errs.push('563-4: selecting a tile did not show the batch bar (fixture)');
    const fav = document.getElementById('fav-view-btn');
    if (fav.classList.contains('hidden')) errs.push('563-4: the favorites toggle is not shown (fixture)');
    fav.click();
    await sleep(250);
    if (!bar.classList.contains('hidden')) errs.push('563-4: the batch bar stayed visible after switching to the favorites grid');
    if (document.body.classList.contains('grid-selecting')) errs.push('563-4: the grid stayed in selection mode after switching to the favorites grid');
""")
        failed += ar_probe('f11', 'folder popover member: a click in chain mode picks the prompt for the chain', 'index', '1000x780', '', r"""
    await bootReady();
    const click = async (el) => { const [x, y] = centre(el); ptr('pointerdown', x, y, el); ptr('pointerup', x, y, window); await sleep(300); };
    document.getElementById('chain-btn').click();
    await sleep(200);
    await click(tileEl('p0'));
    const copies = () => callsFor('copy_text').map((a) => a && a.text);
    if (!copies().some((x) => /number 1[.]/.test(x || ''))) errs.push('563-11: fixture: the chain pick of p0 did not copy its text, got ' + JSON.stringify(copies()));
    await click(document.querySelector('#grid .folder-tile'));
    const member = document.querySelector('.folder-member-tile[data-id="p4"]');
    if (!member) { errs.push('563-11: the folder popover did not open'); }
    else {
      await click(member);
      const last = copies().pop() || '';
      if (!(/number 1[.]/.test(last) && /number 5[.]/.test(last))) errs.push('563-11: a member click in chain mode replaced the chain text instead of adding a step, clipboard now: ' + JSON.stringify(last));
    }
""")
        failed += ar_probe('f10', 'folder popover member: a fast second click pastes like the grid tile', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { autoPaste: true } };</script>\n', r"""
    await bootReady();
    const click = async (el) => { const [x, y] = centre(el); ptr('pointerdown', x, y, el); ptr('pointerup', x, y, window); await sleep(60); };
    await click(document.querySelector('#grid .folder-tile'));
    await sleep(200);
    const member = document.querySelector('.folder-member-tile[data-id="p4"]');
    if (!member) { errs.push('563-10: the folder popover did not open'); }
    else {
      window.__SMOKE_ARGS__.length = 0;
      await click(member);
      await click(member);
      await sleep(300);
      if (!callsFor('copy_prompt').length) errs.push('563-10: the first member click did not copy');
      if (!callsFor('paste_into_previous').length) errs.push('563-10: a fast second member click did not paste into the previous window');
    }
""")
        failed += ar_probe('f13', 'folder popover member: missing file banner like the grid tile', 'index', '1000x780', '', r"""
    await bootReady();
    const click = async (el) => { const [x, y] = centre(el); ptr('pointerdown', x, y, el); ptr('pointerup', x, y, window); await sleep(200); };
    window.__FIRE__('missing-files', ['p11']);
    await sleep(100);
    await click(document.querySelector('#grid .folder-tile'));
    const member = () => document.querySelector('.folder-member-tile[data-id="p11"]');
    if (!member()) { errs.push('563-13: the folder popover did not open'); }
    else {
      if (!member().classList.contains('file-missing')) errs.push('563-13: a member whose file is missing has no file-missing class');
      const banner = member().querySelector('.tile-error');
      if (!banner) errs.push('563-13: a member with an attached file has no missing-file banner element');
      else if (getComputedStyle(banner).display === 'none') errs.push('563-13: the missing-file banner of a member is not shown');
      window.__FIRE__('missing-files', []);
      await sleep(100);
      if (member().classList.contains('file-missing')) errs.push('563-13: the open popover kept file-missing after the file came back');
      window.__FIRE__('missing-files', ['p11']);
      await sleep(100);
      if (!member().classList.contains('file-missing')) errs.push('563-13: the open popover did not pick up a file going missing');
    }
""")
        failed += ar_probe('f14', 'folder popover member: per-prompt font, size and ink like the grid tile', 'index', '1000x780', '', r"""
    await bootReady();
    const click = async (el) => { const [x, y] = centre(el); ptr('pointerdown', x, y, el); ptr('pointerup', x, y, window); await sleep(200); };
    PROMPTS[4].font_size = 20;
    PROMPTS[4].font = 'mono';
    PROMPTS[6].font_size = 1;
    PROMPTS[8].color = '#fde047';
    const tabFor = (id) => document.querySelector('.view-btn[data-view-id="' + id + '"]');
    tabFor('work').click(); await sleep(250);
    tabFor('home').click(); await sleep(300);
    await click(document.querySelector('#grid .folder-tile'));
    const m = (id) => document.querySelector('.folder-member-tile[data-id="' + id + '"]');
    if (!m('p4')) { errs.push('563-14: the folder popover did not open'); }
    else {
      const l4 = m('p4').querySelector('.tile-name');
      if (l4.style.fontSize !== '20px') errs.push('563-14: a fixed-size prompt label has font size ' + JSON.stringify(l4.style.fontSize) + ', expected 20px');
      if (m('p4').dataset.fitMode !== 'fixed') errs.push('563-14: fitMode of a fixed-size member is ' + m('p4').dataset.fitMode);
      if (!l4.style.fontFamily) errs.push('563-14: the per-prompt font family is not applied to the member label');
      if (m('p6').dataset.fitMode !== 'auto') errs.push('563-14: fitMode of an auto-size member is ' + m('p6').dataset.fitMode);
      const ink = m('p8').style.color;
      if (ink !== 'rgb(0, 0, 0)' && ink !== '#000' && ink !== 'black') errs.push('563-14: a light tinted member has ink ' + JSON.stringify(ink) + ', expected a dark one');
    }
""")
        failed += ar_probe('g2', 'expert menu bulk actions never flip demo mode, remote access, network, sync or the URL scheme', 'index', '1000x780', '<script>window.__DEMO_TEST__ = true;</script>\n', r"""
    await bootReady();
    const flagCalls = () => callsFor('set_ui_flag');
    const DANGEROUS = ['demoMode', 'remoteApi', 'remoteNetworkFilter', 'urlScheme', 'promptSync'];
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('expert-open').click();
    await sleep(300);
    const cm = document.getElementById('confirm-modal');
    const preset = async (id) => {
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById(id).click();
      await sleep(250);
      if (!visible(cm)) { return false; }
      document.getElementById('confirm-ok').click();
      await sleep(400);
      return true;
    };
    // per-category reset: the System category holds the demo switch
    const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
    let reset = null;
    for (const tb of tabs) {
      tb.click(); await sleep(80);
      const grp = [...document.querySelectorAll('#expert-flags .expert-group')].find((g) => [...g.querySelectorAll('.switch-field span')].some((s) => s.textContent === 'Demo mode'));
      if (grp) { reset = grp.querySelector('.expert-group-reset'); break; }
    }
    window.__SMOKE_ARGS__.length = 0;
    if (reset) {
      reset.click(); await sleep(250);
      if (visible(cm)) { document.getElementById('confirm-ok').click(); await sleep(400); }
    }
    if (flagCalls().some((a) => a.key === 'demoMode')) errs.push('564-2: the category reset wrote demoMode');
    // per-row reset icon of the demo switch must not exist
    const rowBtn = [...document.querySelectorAll('#expert-flags .switch-field')].find((r) => (r.querySelector('span') || {}).textContent === 'Demo mode');
    const rowReset = rowBtn && rowBtn.querySelector('.expert-row-reset');
    if (rowReset && !rowReset.classList.contains('hidden')) errs.push('564-2: the demo switch offers a per-row reset');
    if (!(await preset('expert-preset-full'))) errs.push('564-2: All on showed no confirmation in the fixture');
    const afterFull = flagCalls().filter((a) => DANGEROUS.includes(a.key));
    if (afterFull.length) errs.push('564-2: All on wrote ' + JSON.stringify(afterFull));
    const multi = flagCalls().filter((a) => a.key === 'allowMultipleInstances');
    await preset('expert-preset-standard');
    const afterStd = flagCalls().filter((a) => DANGEROUS.includes(a.key) && a.enabled !== false);
    if (flagCalls().some((a) => a.key === 'demoMode')) errs.push('564-2: the Standard profile wrote demoMode during a demo session');
    if (afterStd.length) errs.push('564-2: the Standard profile switched something on: ' + JSON.stringify(afterStd));
    // reset all: the backend wipes the flag, so the marker is written back
    window.__SMOKE_ARGS__.length = 0;
    const rb = document.getElementById('expert-reset');
    rb.click(); await sleep(150);
    rb.click(); await sleep(500);
    const seq = (window.__SMOKE_ARGS__ || []).filter(([c, a]) => c === 'reset_expert' || (c === 'set_ui_flag' && a.key === 'demoMode'));
    if (!seq.length || seq[0][0] !== 'reset_expert') errs.push('564-2: reset all did not run reset_expert');
    const back = seq.find(([c, a]) => c === 'set_ui_flag' && a.key === 'demoMode');
    if (!back || back[1].enabled !== true) errs.push('564-2: reset all left a running demo session without its marker');
""")
        failed += ar_probe('g3', 'expert menu: a stored remote rate limit of 0 (unlimited) shows as unlimited, not 5/min', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { remoteApi: true }, ui_values: { remoteRateLimit: 0 } };</script>\n', r"""
    await bootReady();
    document.getElementById('gear').click(); await sleep(200);
    document.getElementById('expert-open').click(); await sleep(300);
    let row = null;
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click(); await sleep(80);
      row = document.querySelector('#expert-flags [data-setting-row="remoteRateLimit"]');
      if (row) break;
    }
    if (!row) { errs.push('564-3: the remote rate limit row was not found'); }
    else {
      const out = row.querySelector('.value-out').textContent;
      if (out !== 'Unlimited') errs.push('564-3: the stored 0 shows as ' + JSON.stringify(out) + ', expected Unlimited');
    }
""")
        failed += ar_probe('g4', 'expert menu: video folder numbers follow the folder, a late answer for an old folder is ignored', 'index', '1000x780', '', r"""
    await bootReady();
    const gates = [];
    RESULTS.video_folder_stats = (a) => new Promise((res) => gates.push({ dir: a.dir, res }));
    document.getElementById('gear').click(); await sleep(200);
    document.getElementById('expert-open').click(); await sleep(300);
    const statsEl = () => document.querySelector('.path-video-stats');
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click(); await sleep(80);
      if (statsEl()) break;
    }
    if (!statsEl()) { errs.push('564-4: the video folder row was not found'); }
    else {
      const line = () => statsEl().querySelector('span').textContent;
      const change = async (dir) => {
        window.__PICK_FOLDER__ = dir;
        statsEl().parentElement.querySelector('.path-btns button').click();
        await sleep(300);
      };
      if (gates.length < 1) errs.push('564-4: opening the row did not ask for the folder stats');
      gates[0].res([2, 2048]);
      await sleep(200);
      if (!/\b2\b/.test(line())) errs.push('564-4: fixture: first folder numbers missing: ' + line());
      await change('C:\Videos\B');
      if (/\b2\b/.test(line())) errs.push('564-4: the old folder numbers stayed on screen after picking another folder: ' + line());
      if (!statsEl().querySelector('button').disabled) errs.push('564-4: the cleanup button stayed enabled for a folder that is not measured yet');
      await change('C:\Videos\C');
      const byDir = (d) => gates.find((g) => g.dir === d);
      if (!byDir('C:\Videos\B') || !byDir('C:\Videos\C')) errs.push('564-4: fixture: both folder reads were not started');
      else {
        byDir('C:\Videos\C').res([7, 7168]);
        await sleep(200);
        byDir('C:\Videos\B').res([5, 5120]);
        await sleep(200);
        if (!/\b7\b/.test(line())) errs.push('564-4: a late answer for an older folder replaced the current numbers: ' + line());
      }
    }
""")
        failed += ar_probe('g6', 'expert menu: the sync readout is asked again each time the menu opens', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { promptSync: true } };</script>\n', r"""
    await bootReady();
    document.getElementById('gear').click(); await sleep(200);
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('expert-open').click(); await sleep(400);
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click(); await sleep(80);
      if (document.querySelector('.path-sync-stats')) break;
    }
    if (!document.querySelector('.path-sync-stats')) errs.push('564-6: the sync readout row was not found');
    const asked = () => callsFor('sync_status').length;
    const first = asked();
    if (first < 1) errs.push('564-6: opening the menu did not ask for the sync status');
    document.getElementById('expert-back').click(); await sleep(200);
    document.getElementById('expert-open').click(); await sleep(400);
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click(); await sleep(80);
      if (document.querySelector('.path-sync-stats')) break;
    }
    if (asked() <= first) errs.push('564-6: reopening the menu reused the first answer of the session (' + asked() + ' request(s))');
""")
        failed += ar_probe('g33', 'language dialog: a running install stays busy after the dialog is closed and reopened', 'index', '1000x780', '', r"""
    await bootReady();
    window.__OCR_CAPS_TEST__ = [
      { name: 'Language.OCR~~~ja-JP~0.0.1.0', tag: 'ja-JP', label: 'Japanese', state: 'NotPresent', version: '0.0.1.0', download_size: 1000000, install_size: null },
    ];
    RESULTS.ocr_install_result = () => null;
    document.getElementById('gear').click(); await sleep(200);
    document.getElementById('expert-open').click(); await sleep(300);
    let opener = null;
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click(); await sleep(80);
      opener = [...document.querySelectorAll('#expert-flags .expert-action')].find((r) => /languages/i.test(r.textContent) && r.closest('.expert-group') && /text recognition|ocr|image/i.test(r.closest('.expert-group').textContent));
      if (opener) break;
    }
    if (!opener) { errs.push('564-33: the language dialog opener was not found'); }
    else {
      const modal = document.getElementById('ocr-langs-modal');
      const btn = () => document.querySelector('#ocr-langs-body .ocr-lang-row .ghost-btn');
      opener.click(); await sleep(300);
      if (!btn()) { errs.push('564-33: no download button in the dialog'); }
      else {
        btn().click(); await sleep(200);
        document.getElementById('confirm-ok').click(); await sleep(400);
        if (!btn().disabled) errs.push('564-33: fixture: the row is not busy while the install runs');
        document.getElementById('ocr-langs-close-x').click(); await sleep(200);
        opener.click(); await sleep(400);
        if (!btn()) errs.push('564-33: no download button after reopening');
        else if (!btn().disabled) errs.push('564-33: after reopening the dialog the Download button is enabled while the install still runs');
        const started = callsFor('ocr_install_language').length;
        if (started !== 1) errs.push('564-33: expected one install start, got ' + started);
      }
    }
""")
        failed += ar_probe('g35', 'hotkey field: a refused capture leaves the global hotkeys off while the field has focus', 'index', '1000x780', '', r"""
    await bootReady();
    document.getElementById('gear').click(); await sleep(250);
    const input = document.getElementById('opt-hotkey');
    input.focus(); await sleep(150);
    const capture = () => callsFor('set_hotkey_capture');
    if (!capture().some((a) => a && a.active === true)) errs.push('564-35: fixture: focusing the field did not switch capture on');
    window.__SMOKE_ARGS__.length = 0;
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'b', ctrlKey: true, altKey: true, bubbles: true, cancelable: true }));
    await sleep(400);
    if (!callsFor('set_hotkey').length) errs.push('564-35: fixture: the keydown did not reach set_hotkey');
    if (document.activeElement !== input) errs.push('564-35: fixture: the field lost the focus');
    if (!capture().some((a) => a && a.active === true)) errs.push('564-35: after a refused combination the hotkeys were not taken off again while the field still has focus');
""")
        failed += ar_probe('m1', 'create dialog: saving with an empty text keeps the dialog and what was typed', 'index', '1000x780', '', r"""
    await bootReady();
    const empty = [...document.querySelectorAll('#grid .cell')].find((c) => !c.firstElementChild);
    if (!empty) { errs.push('564-m1: no empty cell in the fixture'); }
    else {
      empty.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })); await sleep(250);
      const modal = document.getElementById('modal');
      if (modal.classList.contains('hidden')) errs.push('564-m1: fixture: the create dialog did not open');
      document.getElementById('modal-name').value = 'Keep this name';
      document.getElementById('modal-text').value = '';
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('modal-confirm').click(); await sleep(300);
      if (modal.classList.contains('hidden')) errs.push('564-m1: saving with an empty text closed the dialog and dropped the typed name');
      else if (document.getElementById('modal-name').value !== 'Keep this name') errs.push('564-m1: the typed name was lost');
      if (callsFor('add_prompt').length) errs.push('564-m1: an empty prompt was saved');
      if (!modal.classList.contains('hidden') && document.activeElement !== document.getElementById('modal-text')) errs.push('564-m1: the text field was not focused for the missing text');
      if (document.getElementById('modal-confirm').disabled) errs.push('564-m1: Save stayed disabled after the refused save');
    }
""")
        failed += ar_probe('v2', 'dictation: a final text that lands late replaces the live partial instead of doubling it', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { voiceInterim: true } };</script>\n', r"""
    await bootReady();
    const btn = document.getElementById('voice-btn');
    const input = document.getElementById('input');
    input.value = '';
    btn.click(); await sleep(100);
    window.__FIRE__('voice-partial', { id: 1, token: 1, text: 'hello' });
    await sleep(50);
    if (input.value.trim() !== 'hello') errs.push('564-v2: fixture: the partial did not land, got ' + JSON.stringify(input.value));
    window.__FIRE__('voice-stopped', { id: 1, token: 1, reason: 'manual' });
    await sleep(50);
    window.__FIRE__('voice-text', { id: 1, token: 1, text: 'hello world' });
    await sleep(100);
    if (input.value.trim() !== 'hello world') errs.push('564-v2: the late final text should replace the partial, field holds ' + JSON.stringify(input.value));
""")
        failed += ar_probe('c3', 'copies from other apps: a failed clear offers no undo (storage panel and copy history)', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true } };</script>\n', r"""
    await bootReady();
    RESULTS.clip_inbox_clear = () => { throw 'delete-incomplete'; };
    const undoBtn = () => document.querySelector('#toast .toast-btn');
    // 1) the retention row of the backups dialog
    document.getElementById('gear').click(); await sleep(200);
    document.getElementById('settings-backup-btn').click(); await sleep(400);
    const row = document.querySelector('#retention-panel [data-setting-row="retention-clipInbox"]');
    const clean = row && row.querySelectorAll('.import-preview-actions button')[1];
    if (!clean) { errs.push('564-c3: the cleanup button for copies from other apps was not found'); }
    else {
      window.__SMOKE_ARGS__.length = 0;
      clean.click(); await sleep(400);
      if (!callsFor('clip_inbox_clear').length) errs.push('564-c3: fixture: the clear was never attempted');
      if (undoBtn()) errs.push('564-c3: the storage panel offered Undo after the clear failed');
      const msg = document.getElementById('toast').textContent;
      if (/cleared/i.test(msg)) errs.push('564-c3: the storage panel reported a clear that failed: ' + msg);
    }
    // 2) the copy history itself, under the Elsewhere filter
    document.getElementById('backup-modal').classList.add('hidden');
    document.getElementById('toast').classList.remove('show');
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(400);
    const srcEl = document.querySelector('#journal-source [data-source="elsewhere"]');
    if (!srcEl || document.getElementById('journal-source').classList.contains('hidden')) { errs.push('564-c3: the source chips are not shown in the fixture'); }
    else {
      srcEl.click(); await sleep(200);
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('journal-clear').click(); await sleep(300);
      const cmC3 = document.getElementById('confirm-modal');
      if (cmC3 && !cmC3.classList.contains('hidden')) { document.getElementById('confirm-ok').click(); await sleep(300); }
      if (!callsFor('clip_inbox_clear').length) errs.push('564-c3: fixture: the history clear was never attempted');
      if (undoBtn()) errs.push('564-c3: the copy history offered Undo after the clear failed');
    }
""")
        failed += ar_probe('m2', 'copy history: Clear follows the source filter, names both counts, stops on a failed clear and ignores an empty target', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true } };</script>\n', r"""
    await bootReady();
    const undoBtn = () => document.querySelector('#toast .toast-btn');
    const confirmOpen = () => !document.getElementById('confirm-modal').classList.contains('hidden');
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(400);
    // All = both stores: the confirm names both counts and cancelling clears nothing.
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('journal-clear').click(); await sleep(300);
    if (!confirmOpen()) errs.push('564-m2: Clear history under All asked nothing');
    else {
      const msg = document.getElementById('confirm-msg').textContent;
      if (!/\(4 entries\)/.test(msg) || !/\(2 entries\)/.test(msg)) errs.push('564-m2: the confirm text does not name both counts: ' + msg);
      document.getElementById('confirm-cancel').click(); await sleep(200);
    }
    if (callsFor('clear_copy_history').length || callsFor('clip_inbox_clear').length) errs.push('564-m2: a cancelled clear still deleted something');
    // A failed copy-log clear stops: nothing else is cleared, no undo.
    RESULTS.clear_copy_history = () => { throw 'boom'; };
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('journal-clear').click(); await sleep(300);
    if (confirmOpen()) { document.getElementById('confirm-ok').click(); await sleep(300); }
    if (!callsFor('clear_copy_history').length) errs.push('564-m2: fixture: the copy log clear was never attempted');
    if (callsFor('clip_inbox_clear').length) errs.push('564-m2: the stored entries were cleared although the copy log clear failed');
    if (undoBtn()) errs.push('564-m2: an Undo was offered after the clear failed');
    delete RESULTS.clear_copy_history;
    // An empty target does nothing: no dialog, no toast, no undo.
    RESULTS.clip_inbox_list = () => [];
    document.getElementById('toast').classList.remove('show', 'actionable');
    document.querySelector('#journal-source [data-source="elsewhere"]').click(); await sleep(300);
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('journal-clear').click(); await sleep(300);
    if (confirmOpen()) errs.push('564-m2: clearing an empty store opened a dialog');
    if (callsFor('clip_inbox_clear').length) errs.push('564-m2: clearing an empty store still called clip_inbox_clear');
    if (undoBtn() || document.getElementById('toast').classList.contains('show')) errs.push('564-m2: clearing an empty store showed a toast');
""")
        failed += ar_probe('l3', 'copies from other apps: stored entries keep the history tab and the purge button while nothing records', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { copyHistory: false, clipWatcher: false } };</script>\n', r"""
    await bootReady();
    const tabsHidden = () => document.getElementById('library-tabs').classList.contains('hidden');
    document.getElementById('library-btn').click(); await sleep(500);
    if (tabsHidden()) errs.push('564-l3: the history tab is gone although entries from other apps are stored');
    else {
      document.getElementById('library-tab-history').click(); await sleep(400);
      if (document.getElementById('clip-purge-backups').classList.contains('hidden')) errs.push('564-l3: the purge button is not reachable');
      if (document.querySelectorAll('#journal-body .clip-row').length !== 2) errs.push('564-l3: the stored entries are not listed');
    }
    // With nothing stored and nothing recording, the tab is gone again.
    document.getElementById('library-close').click(); await sleep(200);
    RESULTS.clip_inbox_list = () => [];
    document.getElementById('library-btn').click(); await sleep(500);
    if (!tabsHidden()) errs.push('564-l3: the history tab is offered with nothing to show and nothing recording');
""")
        failed += ar_probe('s1', 'screenshot editor: undoing a resize does not unlock operations behind the undo limit', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_values: { snipEditUndoMax: 10 } };</script>\n', r"""
    await bootReady();
    const cv = document.createElement('canvas'); cv.width = 200; cv.height = 120;
    const g = cv.getContext('2d'); g.fillStyle = '#88aacc'; g.fillRect(0, 0, 200, 120);
    window.__FIRE__('snip-captured', { data_url: cv.toDataURL('image/png'), path: '', name: 'undo.png' });
    await sleep(400);
    const edit = document.getElementById('snip-edit');
    if (edit.classList.contains('hidden')) { errs.push('564-s1: the editor did not open'); }
    else {
      document.querySelector('#snip-edit-tools button[data-tool="pen"]').click(); await sleep(50);
      const live = document.getElementById('snip-edit-live');
      const undo = document.getElementById('snip-edit-undo');
      const r = live.getBoundingClientRect();
      for (let i = 0; i < 12; i++) {
        const x = r.left + 10 + i * 3, y = r.top + 10 + i * 4;
        live.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: x, clientY: y }));
        live.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: x + 6, clientY: y + 6 }));
        window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: x + 6, clientY: y + 6 }));
        await sleep(20);
      }
      let plain = 0;
      // a plain count of how far undo walks with no resize, for comparison
      document.getElementById('snip-edit-scale').click(); await sleep(150);
      const pop = document.querySelector('.snip-scale-pop');
      const half = [...pop.querySelectorAll('.snip-scale-presets button')].find((b) => b.textContent === '50%');
      half.click(); await sleep(50);
      pop.querySelector('.primary-btn').click(); await sleep(250);
      let steps = 0;
      while (!undo.disabled && steps < 40) { undo.click(); steps++; await sleep(15); }
      // 12 strokes, limit 10: the scale itself plus the 10 newest strokes can be undone
      if (steps !== 11) errs.push('564-s1: undo walked ' + steps + ' steps after a resize, expected 11 (the resize and ten strokes)');
      const redo = document.getElementById('snip-edit-redo');
      let redos = 0;
      while (!redo.disabled && redos < 40) { redo.click(); redos++; await sleep(15); }
      if (redos !== 11) errs.push('564-s1: redo walked ' + redos + ' steps, expected 11');
      if (undo.disabled) errs.push('564-s1: Undo stayed disabled after redoing the resize');
    }
""")
        failed += ar_probe('d1', 'screenshot dialog: a failed delete keeps the saved path, switching back on saves no second copy', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { snipEditor: false, screenshotSave: true } };</script>\n', r"""
    await bootReady();
    RESULTS.delete_screenshot_file = () => false;
    RESULTS.save_screenshot_now = () => 'C:/shots/second.png';
    const cv = document.createElement('canvas'); cv.width = 40; cv.height = 30;
    window.__FIRE__('snip-captured', { data_url: cv.toDataURL('image/png'), path: 'C:/shots/first.png', name: 'first.png' });
    await sleep(300);
    const modal = document.getElementById('snip-modal');
    const box = document.getElementById('snip-save-folder');
    if (modal.classList.contains('hidden')) errs.push('564-d1: fixture: the result dialog did not open');
    if (!box.checked) errs.push('564-d1: fixture: the folder switch is not on');
    window.__SMOKE_ARGS__.length = 0;
    box.checked = false; box.dispatchEvent(new Event('change', { bubbles: true })); await sleep(200);
    if (!callsFor('delete_screenshot_file').length) errs.push('564-d1: fixture: switching off did not try to delete the file');
    box.checked = true; box.dispatchEvent(new Event('change', { bubbles: true })); await sleep(200);
    const saves = callsFor('save_screenshot_now').length;
    if (saves) errs.push('564-d1: switching back on saved a second copy (' + saves + ' save call) although the first file was never deleted');
""")
        ba_common = r"""
    const BAW = 200, BAH = 120, BGC = [136, 170, 204];
    const loadImg = (u) => new Promise((res) => { const i = new Image(); i.onload = () => res(i); i.src = u; });
    const pixelsOf = async (u) => {
      const i = await loadImg(u);
      const c = document.createElement('canvas'); c.width = i.width; c.height = i.height;
      const g = c.getContext('2d'); g.drawImage(i, 0, 0);
      return { w: i.width, h: i.height, px: (x, y) => Array.from(g.getImageData(x, y, 1, 1).data) };
    };
    const near = (p, c, tol) => [0, 1, 2].every((i) => Math.abs(p[i] - c[i]) <= (tol || 8));
    const captureFixture = () => {
      const cv = document.createElement('canvas'); cv.width = BAW; cv.height = BAH;
      const g = cv.getContext('2d'); g.fillStyle = '#88aacc'; g.fillRect(0, 0, BAW, BAH);
      return cv.toDataURL('image/png');
    };
    const baOpen = async (path) => {
      await bootReady();
      window.__FIRE__('snip-captured', { data_url: captureFixture(), path: path || '', name: 'ba.png' });
      await sleep(400);
      return !document.getElementById('snip-edit').classList.contains('hidden');
    };
    const baTool = async (n) => { document.querySelector('#snip-edit-tools button[data-tool="' + n + '"]').click(); await sleep(60); };
    const baPt = (x, y) => {
      const c = document.getElementById('snip-edit-base'), r = c.getBoundingClientRect();
      const z = r.width / c.width;
      return [r.left + x * z, r.top + y * z];
    };
    const baPtr = (type, x, y, target) => (target || window).dispatchEvent(new PointerEvent(type, {
      bubbles: true, cancelable: true, button: 0, pointerId: 1, isPrimary: true, clientX: x, clientY: y, altKey: true,
    }));
    const baDrag = async (x1, y1, x2, y2) => {
      const live = document.getElementById('snip-edit-live');
      // every position is worked out again after the previous event: a selection can
      // re-render the context bar and shift the canvas
      let a = baPt(x1, y1);
      baPtr('pointerdown', a[0], a[1], live);
      a = baPt((x1 + x2) / 2, (y1 + y2) / 2);
      baPtr('pointermove', a[0], a[1], live);
      a = baPt(x2, y2);
      baPtr('pointermove', a[0], a[1], live);
      baPtr('pointerup', a[0], a[1], window);
      await sleep(40);
    };
    const baClick = async (x, y) => {
      const live = document.getElementById('snip-edit-live');
      const a = baPt(x, y);
      baPtr('pointerdown', a[0], a[1], live);
      baPtr('pointerup', a[0], a[1], window);
      await sleep(40);
    };
    const baKey = (key, extra) => document.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ key, bubbles: true, cancelable: true }, extra || {})));
    const baExport = async () => {
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('snip-edit-copy').click();
      await sleep(200);
      const c = callsFor('copy_image_data');
      return c.length ? pixelsOf(c[c.length - 1].dataUrl) : null;
    };
    const baScale50 = async () => {
      document.getElementById('snip-edit-scale').click(); await sleep(150);
      const pop = document.querySelector('.snip-scale-pop');
      [...pop.querySelectorAll('.snip-scale-presets button')].find((b) => b.textContent === '50%').click();
      await sleep(50);
      pop.querySelector('.primary-btn').click(); await sleep(250);
    };
    const baUndo = async () => { document.getElementById('snip-edit-undo').click(); await sleep(60); };
    const baRedo = async () => { document.getElementById('snip-edit-redo').click(); await sleep(60); };
    const baCtxPop = () => [...document.body.children].find((e) => e.classList.contains('ctx') && !e.id && !e.classList.contains('hidden'));
    const baCtx = async (x, y) => {
      const a = baPt(x, y);
      document.getElementById('snip-edit-live').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: a[0], clientY: a[1] }));
      await sleep(60);
      return baCtxPop();
    };
"""
        ba_seed = '<script>window.__SEED_SETTINGS__ = { ui_flags: { screenshotSave: true } };</script>\n'
        failed += ar_probe('ba597a', 'screenshot editor: a failed save keeps the unedited file and says so', 'index', '1000x780', ba_seed, ba_common + r"""
    if (!(await baOpen('C:/shots/first.png'))) { errs.push('597: the editor did not open'); }
    else {
      RESULTS.save_screenshot_now = () => null;
      RESULTS.delete_screenshot_file = () => true;
      await baTool('pen'); await baDrag(20, 20, 90, 70);
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('toast').textContent = '';
      document.getElementById('snip-edit-apply').click(); await sleep(500);
      if (!callsFor('save_screenshot_now').length) errs.push('597: fixture: Apply did not try to save the edited shot');
      if (callsFor('delete_screenshot_file').length) errs.push('597: the unedited file was deleted although the edited shot was not saved');
      if (!/could not be saved/i.test(document.getElementById('toast').textContent)) errs.push('597: no error toast after the failed save, toast is ' + JSON.stringify(document.getElementById('toast').textContent));
      if (document.getElementById('snip-save-folder').checked) errs.push('597: the folder switch stays checked after the failed save');
      if (document.getElementById('snip-modal').classList.contains('hidden')) errs.push('597: fixture: the result dialog did not open');
    }
""")
        failed += ar_probe('ba597b', 'screenshot editor: a good save removes the old file only after the new one exists, never the new one itself', 'index', '1000x780', ba_seed, ba_common + r"""
    if (!(await baOpen('C:/shots/first.png'))) { errs.push('597: the editor did not open'); }
    else {
      RESULTS.save_screenshot_now = () => 'C:/shots/second.png';
      RESULTS.delete_screenshot_file = () => true;
      await baTool('pen'); await baDrag(20, 20, 90, 70);
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('snip-edit-apply').click(); await sleep(500);
      const order = (window.__SMOKE_ARGS__ || []).map(([c]) => c).filter((c) => c === 'save_screenshot_now' || c === 'delete_screenshot_file');
      if (order.join() !== 'save_screenshot_now,delete_screenshot_file') errs.push('597: expected save then delete, got ' + order.join());
      const del = callsFor('delete_screenshot_file')[0];
      if (!del || del.path !== 'C:/shots/first.png') errs.push('597: the wrong file was deleted: ' + JSON.stringify(del));
      if (!document.getElementById('snip-save-folder').checked) errs.push('597: the folder switch is off after a good save');
      // the same name comes back (same second): the old path is the new file, it must stay
      document.getElementById('snip-modal-close').click(); await sleep(200);
      window.__FIRE__('snip-captured', { data_url: captureFixture(), path: 'C:/shots/same.png', name: 'same.png' });
      await sleep(400);
      RESULTS.save_screenshot_now = () => 'C:/shots/same.png';
      await baTool('pen'); await baDrag(20, 20, 90, 70);
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('snip-edit-apply').click(); await sleep(500);
      if (callsFor('delete_screenshot_file').length) errs.push('597: the freshly saved file was deleted because it reused the old name');
    }
""")
        failed += ar_probe('ba597c', 'screenshot dialog: switching the folder save on with a failing save says so and puts the switch back off', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { snipEditor: false, screenshotSave: false } };</script>\n', ba_common + r"""
    await bootReady();
    RESULTS.save_screenshot_now = () => null;
    window.__FIRE__('snip-captured', { data_url: captureFixture(), path: '', name: 'x.png' });
    await sleep(300);
    const box = document.getElementById('snip-save-folder');
    if (document.getElementById('snip-modal').classList.contains('hidden')) errs.push('597: fixture: the result dialog did not open');
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('toast').textContent = '';
    box.checked = true; box.dispatchEvent(new Event('change', { bubbles: true })); await sleep(300);
    if (!callsFor('save_screenshot_now').length) errs.push('597: fixture: switching on did not try to save');
    if (box.checked) errs.push('597: the switch stays on although nothing was saved');
    if (!/could not be saved/i.test(document.getElementById('toast').textContent)) errs.push('597: no error toast, toast is ' + JSON.stringify(document.getElementById('toast').textContent));
    const last = callsFor('set_ui_flag').filter((a) => a && a.key === 'screenshotSave').pop();
    if (!last || last.enabled !== false) errs.push('597: the stored setting was left on, last write ' + JSON.stringify(last));
""")
        failed += ar_probe('ba594', 'screenshot editor: moving, resizing or deleting a finished shape is one undo step of its own', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('594: the editor did not open'); }
    else {
      // rows of the arrow (y 30) and of the rectangle border (y 75), sampled in a small band
      const hasInk = (img, x, y) => { for (let d = -3; d <= 3; d++) { if (!near(img.px(x, y + d), BGC, 30)) return true; } return false; };
      await baTool('arrow'); await baDrag(20, 30, 120, 30);
      await baTool('rect'); await baDrag(20, 75, 120, 105);
      await baTool('arrow');
      let img = await baExport();
      if (!img || !hasInk(img, 60, 30) || !hasInk(img, 60, 75)) errs.push('594: fixture: arrow and rectangle are not both drawn');
      // drag the arrow down by 20
      await baDrag(60, 30, 60, 50);
      img = await baExport();
      if (!hasInk(img, 60, 50) || hasInk(img, 60, 22)) errs.push('594: fixture: the drag did not move the arrow');
      await baUndo();
      img = await baExport();
      if (!hasInk(img, 60, 30)) errs.push('594: undo did not bring the moved arrow back to its place');
      if (hasInk(img, 60, 50)) errs.push('594: the arrow is still at the moved place after undo');
      if (!hasInk(img, 60, 75)) errs.push('594: undo removed the rectangle instead of the move');
      await baRedo();
      img = await baExport();
      if (!hasInk(img, 60, 50) || hasInk(img, 60, 22)) errs.push('594: redo did not move the arrow again');
      if (!hasInk(img, 60, 75)) errs.push('594: redo lost the rectangle');
      // resize the rectangle by its corner handle, then undo
      await baTool('rect');
      await baClick(70, 75);
      await baDrag(120, 105, 160, 112);
      img = await baExport();
      if (!hasInk(img, 140, 112)) errs.push('594: fixture: the rectangle was not resized');
      await baUndo();
      img = await baExport();
      if (hasInk(img, 140, 112)) errs.push('594: undo left the rectangle at its enlarged size');
      if (!hasInk(img, 60, 50)) errs.push('594: undoing the resize also undid the earlier arrow move');
      // delete the rectangle through the right-click menu, then undo
      const pop = await baCtx(60, 75);
      if (!pop) { errs.push('594: fixture: no context menu over the rectangle'); }
      else {
        pop.querySelector('button.danger').click(); await sleep(80);
        img = await baExport();
        if (hasInk(img, 60, 75)) errs.push('594: fixture: Delete did not remove the rectangle');
        await baUndo();
        img = await baExport();
        if (!hasInk(img, 60, 75)) errs.push('594: undo did not bring the deleted rectangle back');
        await baRedo();
        img = await baExport();
        if (hasInk(img, 60, 75)) errs.push('594: redo did not delete the rectangle again');
      }
    }
""")
        failed += ar_probe('ba592a', 'screenshot editor: undoing a committed crop clears the crop box, export is the full image', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('592: the editor did not open'); }
    else {
      await baTool('crop'); await baDrag(60, 30, 140, 90);
      baKey('Enter'); await sleep(80);
      let img = await baExport();
      if (!img || img.w !== 80 || img.h !== 60) errs.push('592: fixture: the crop did not export 80x60, got ' + (img ? img.w + 'x' + img.h : 'nothing'));
      await baUndo();
      img = await baExport();
      if (!img || img.w !== BAW || img.h !== BAH) errs.push('592: after undo the export is ' + (img ? img.w + 'x' + img.h : 'nothing') + ', expected the full ' + BAW + 'x' + BAH);
      if (!document.getElementById('snip-edit-undo').disabled) errs.push('592: exporting pushed the undone crop back onto the undo stack');
      await baRedo();
      img = await baExport();
      if (!img || img.w !== 80 || img.h !== 60) errs.push('592: redo did not bring the crop back, export is ' + (img ? img.w + 'x' + img.h : 'nothing'));
    }
""")
        failed += ar_probe('ba592b', 'screenshot editor: a resize keeps the crop box and the exported crop in step', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('592: the editor did not open'); }
    else {
      await baTool('crop'); await baDrag(100, 60, 180, 110);
      await baScale50();
      const img = await baExport();
      if (!img || img.w !== 40 || img.h !== 25) errs.push('592: after a 50 percent resize the crop should export 40x25, got ' + (img ? img.w + 'x' + img.h : 'nothing'));
    }
""")
        failed += ar_probe('ba596', 'screenshot editor: a resize with the Original locked on keeps every annotation', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('596: the editor did not open'); }
    else {
      const hasInk = (img, x, y) => { for (let d = -3; d <= 3; d++) { if (!near(img.px(x, y + d), BGC, 30)) return true; } return false; };
      await baTool('arrow'); await baDrag(20, 30, 120, 30);
      const orig = document.getElementById('snip-edit-original');
      orig.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 1 }));
      document.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1 }));
      await sleep(80);
      if (!orig.classList.contains('active')) errs.push('596: fixture: a tap on Original did not lock it on');
      await baScale50();
      let img = await baExport();
      if (!img || img.w !== 100 || img.h !== 60) errs.push('596: fixture: the export is not 100x60 after the resize');
      else if (!hasInk(img, 50, 15)) errs.push('596: the arrow is gone after resizing while Original was locked on');
      await baUndo();
      img = await baExport();
      if (!img || img.w !== BAW || !hasInk(img, 100, 30)) errs.push('596: undoing the resize did not bring the arrow back at full size');
    }
""")
        failed += ar_probe('ba593', 'screenshot editor: a resize with a crop set does not bake the crop dimming into the image', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('593: the editor did not open'); }
    else {
      await baTool('crop'); await baDrag(60, 30, 140, 90);
      baKey('Enter'); await sleep(80);
      await baTool('arrow');
      await baScale50();
      await baTool('crop');
      baKey('Escape'); await sleep(80);
      const img = await baExport();
      if (!img || img.w !== 100 || img.h !== 60) errs.push('593: fixture: expected the full 100x60 image, got ' + (img ? img.w + 'x' + img.h : 'nothing'));
      else {
        for (const [x, y] of [[0, 0], [99, 59], [5, 50]]) {
          if (!near(img.px(x, y), BGC, 6)) errs.push('593: pixel ' + x + ',' + y + ' is ' + JSON.stringify(img.px(x, y)) + ', the capture is ' + JSON.stringify(BGC) + ' (the crop dimming was baked in)');
        }
      }
    }
""")
        failed += ar_probe('ba595', 'screenshot editor: step numbers go on counting after a resize', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('595: the editor did not open'); }
    else {
      await baTool('step');
      await baClick(30, 100); await baClick(60, 100); await baClick(90, 100);
      await baScale50();
      const drawn = [];
      const orig = CanvasRenderingContext2D.prototype.fillText;
      CanvasRenderingContext2D.prototype.fillText = function (text) { drawn.push(String(text)); return orig.apply(this, arguments); };
      await baClick(40, 50);
      CanvasRenderingContext2D.prototype.fillText = orig;
      if (drawn.join() !== '4') errs.push('595: the marker placed after the resize reads ' + JSON.stringify(drawn) + ', expected 4');
      // a second resize keeps the count as well
      await baScale50();
      drawn.length = 0;
      CanvasRenderingContext2D.prototype.fillText = function (text) { drawn.push(String(text)); return orig.apply(this, arguments); };
      await baClick(20, 25);
      CanvasRenderingContext2D.prototype.fillText = orig;
      if (drawn.join() !== '5') errs.push('595: after a second resize the next marker reads ' + JSON.stringify(drawn) + ', expected 5');
    }
""")
        failed += ar_probe('ba598', 'screenshot editor: a spotlight offers no Duplicate, so two never dim each other', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('598: the editor did not open'); }
    else {
      await baTool('spotlight'); await baDrag(40, 30, 120, 80);
      let img = await baExport();
      if (!img || near(img.px(5, 5), BGC, 8) || !near(img.px(42, 32), BGC, 8)) errs.push('598: fixture: the spotlight does not dim the outside and keep the inside clear');
      const pop = await baCtx(80, 55);
      if (!pop) { errs.push('598: fixture: no context menu over the spotlight'); }
      else {
        const shown = [...pop.querySelectorAll('button')].filter((b) => !b.classList.contains('hidden') && b.offsetParent !== null);
        if (shown.length !== 1 || !shown[0].classList.contains('danger')) errs.push('598: the spotlight menu offers ' + JSON.stringify(shown.map((b) => b.textContent)) + ', expected only Delete');
      }
      // a rectangle keeps both entries
      document.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      await baTool('rect'); await baDrag(20, 100, 60, 115);
      const pop2 = await baCtx(40, 100);
      const shown2 = pop2 ? [...pop2.querySelectorAll('button')].filter((b) => b.offsetParent !== null) : [];
      if (shown2.length !== 2) errs.push('598: fixture: a rectangle menu shows ' + shown2.length + ' entries, expected Duplicate and Delete');
    }
""")
        failed += ar_probe('ba599', 'screenshot editor: a capture that arrives while Apply saves keeps its own file and state', 'index', '1000x780', ba_seed, ba_common + r"""
    if (!(await baOpen('C:/shots/first.png'))) { errs.push('599: the editor did not open'); }
    else {
      let release = null;
      RESULTS.save_screenshot_now = () => new Promise((res) => { release = () => res('C:/shots/e1.png'); });
      RESULTS.delete_screenshot_file = () => true;
      await baTool('pen'); await baDrag(20, 20, 90, 70);
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('snip-edit-apply').click(); await sleep(150);
      // a hotkey capture lands while the re-encode is still running
      window.__FIRE__('snip-captured', { data_url: captureFixture(), path: 'C:/shots/new.png', name: 'new.png' });
      await sleep(400);
      if (!release) { errs.push('599: fixture: the save never started'); }
      else {
        release(); await sleep(300);
        const dels = callsFor('delete_screenshot_file').map((a) => a.path);
        if (dels.includes('C:/shots/new.png')) errs.push('599: the new capture file was deleted: ' + dels.join());
        if (!dels.includes('C:/shots/first.png')) errs.push('599: the replaced unedited file was not removed: ' + dels.join());
        if (!document.getElementById('snip-modal').classList.contains('hidden')) errs.push('599: the stale apply opened the result dialog over the new capture');
        // applying the new capture must delete its own file, never the first edit's file
        RESULTS.save_screenshot_now = () => 'C:/shots/e2.png';
        await baTool('pen'); await baDrag(20, 20, 90, 70);
        window.__SMOKE_ARGS__.length = 0;
        document.getElementById('snip-edit-apply').click(); await sleep(400);
        const dels2 = callsFor('delete_screenshot_file').map((a) => a.path);
        if (dels2.join() !== 'C:/shots/new.png') errs.push('599: applying the new capture deleted ' + JSON.stringify(dels2) + ', expected only its own file');
      }
    }
""")
        failed += ar_probe('ba600', 'screenshot editor: undoing a delete puts the shape back in its place even after a crop changed the list', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('600: the editor did not open'); }
    else {
      const baText = async (x, y, word) => {
        await baClick(x, y);
        const ti = document.getElementById('snip-edit-text-input');
        ti.value = word;
        ti.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
        await sleep(60);
      };
      await baTool('crop'); await baDrag(10, 10, 190, 110);
      baKey('Enter'); await sleep(80);
      await baTool('text');
      await baText(30, 90, 'AAA'); await baText(120, 90, 'BBB');
      const pop = await baCtx(32, 94);
      const del = pop && pop.querySelector('button.danger');
      if (!del) { errs.push('600: fixture: no delete entry over the first text'); }
      else {
        del.click(); await sleep(80);
        // discarding the crop drops it from the list, every index behind it moves
        await baTool('crop');
        baKey('Escape'); await sleep(80);
        const drawn = [];
        const orig = CanvasRenderingContext2D.prototype.fillText;
        CanvasRenderingContext2D.prototype.fillText = function (text) { drawn.push(String(text)); return orig.apply(this, arguments); };
        await baUndo();
        CanvasRenderingContext2D.prototype.fillText = orig;
        if (drawn.join() !== 'AAA,BBB') errs.push('600: the texts are drawn in the order ' + JSON.stringify(drawn) + ', the restored one must come first');
      }
    }
""")
        failed += ar_probe('ba601', 'screenshot editor: a run of arrow-key nudges is one undo step and redo keeps it', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('601: the editor did not open'); }
    else {
      const hasInk = (img, x, y) => { for (let d = -3; d <= 3; d++) { if (!near(img.px(x + d, y), BGC, 30)) return true; } return false; };
      await baTool('rect'); await baDrag(20, 75, 120, 105);
      let img = await baExport();
      if (!hasInk(img, 20, 90)) errs.push('601: fixture: the rectangle is not drawn');
      baKey('ArrowRight', { shiftKey: true }); baKey('ArrowRight', { shiftKey: true }); await sleep(60);
      img = await baExport();
      if (hasInk(img, 20, 90)) errs.push('601: fixture: the nudges did not move the rectangle');
      await baUndo();
      img = await baExport();
      if (!hasInk(img, 20, 90)) errs.push('601: undo did not take the nudges back');
      if (!hasInk(img, 60, 75)) errs.push('601: undo removed the rectangle, the nudges were more than one step');
      await baRedo();
      img = await baExport();
      if (hasInk(img, 20, 90)) errs.push('601: redo dropped the nudges');
      await baUndo(); await baUndo();
      img = await baExport();
      if (hasInk(img, 60, 75)) errs.push('601: the second undo did not remove the rectangle');
    }
""")
        failed += ar_probe('bl649', 'screenshot editor: a tool starts with the size it was last used at, and a change is saved', 'index', '1000x780', r"""<script>window.__SEED_SETTINGS__ = { ui_texts: { snipToolSizes: '{"pen":{"width":10},"marker":{"width":999},"rect":"junk"}' } };</script>
""", ba_common + r"""
    if (!(await baOpen(''))) { errs.push('649: the editor did not open'); }
    else {
      const widthOf = () => { const i = document.querySelector('#snip-edit-context .snip-edit-field input'); return i ? i.value : '(no field)'; };
      await baTool('pen');
      if (widthOf() !== '10') errs.push('649: pen starts at ' + widthOf() + ', the remembered size is 10');
      await baTool('marker');
      if (widthOf() !== '18') errs.push('649: marker with an out-of-range remembered size starts at ' + widthOf() + ', expected 18');
      await baTool('rect');
      if (widthOf() !== '4') errs.push('649: rect with a broken entry starts at ' + widthOf() + ', expected 4');
      await baTool('line');
      if (widthOf() !== '4') errs.push('649: line with nothing remembered starts at ' + widthOf() + ', expected 4');
      window.__SMOKE_ARGS__ = [];
      document.querySelector('#snip-edit-context .snip-edit-width-btn:last-child').click();
      await sleep(700);
      const saved = (window.__SMOKE_ARGS__ || []).filter(([cmd, a]) => cmd === 'set_ui_text' && a && a.key === 'snipToolSizes');
      const last = saved.length ? saved[saved.length - 1][1].value : '';
      let parsed = null;
      try { parsed = JSON.parse(last); } catch (e) { parsed = null; }
      if (!parsed || !parsed.line || parsed.line.width !== 8) errs.push('649: choosing the large width did not save line width 8: ' + last);
      else if (!parsed.pen || parsed.pen.width !== 10) errs.push('649: saving line dropped the remembered pen size: ' + last);
    }
""")
        failed += ar_probe('bl646', 'update interval: a stored 0 hours with Auto update on shows the shortest interval, never Never', 'index', '1000x780', r"""<script>window.__SEED_SETTINGS__ = { auto_update: true, ui_values: { updateCheckH: 0 } };</script>
""", r"""
    await bootReady();
    document.getElementById('gear').click();
    await sleep(200);
    const sw = document.getElementById('opt-autoupdate');
    const sel = document.getElementById('opt-update-interval');
    if (!sel) errs.push('646: the interval select is missing');
    else {
      if (!sw.checked) errs.push('646: Auto update should be on');
      if (sel.value === '0') errs.push('646: a stored 0 hours with Auto update on shows Never');
      if (sel.value !== '1') errs.push('646: a stored 0 hours should show the 1 hour the backend uses, got ' + sel.value);
    }
""")
        failed += ar_probe('bm654', 'usage statistics: a fresh install shows the switch off and no counts, switching it on stores true and shows them', 'index', '1000x780', r"""<script>window.__SEED_SETTINGS__ = { ui_flags: { usageStats: undefined } };</script>
""", r"""
    await bootReady();
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('settings-backup-btn').click();
    await sleep(600);
    const swi = () => document.querySelector('#stats-panel .stats-head-row .switch');
    if (!swi()) errs.push('654: the statistics switch is missing');
    else {
      if (swi().checked) errs.push('654: a fresh install shows the statistics switch on');
      if (document.querySelector('#stats-panel .stat-cards')) errs.push('654: statistics cards show while the switch is off');
      window.__SMOKE_ARGS__ = [];
      swi().click();
      await sleep(600);
      const calls = (window.__SMOKE_ARGS__ || []).filter(([cmd, a]) => cmd === 'set_ui_flag' && a && a.key === 'usageStats');
      if (!calls.length || calls[calls.length - 1][1].enabled !== true) errs.push('654: switching statistics on did not store usageStats true');
      if (!swi() || !swi().checked) errs.push('654: the switch is not on after switching it on');
      if (!document.querySelector('#stats-panel .stat-cards')) errs.push('654: no statistics cards after switching on');
    }
""")
        # ---- main window findings: generated block end ----
        print("PASS" if not failed else f"{failed} problem(s)")
        return 1 if failed else 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
