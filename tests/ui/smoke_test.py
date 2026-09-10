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

# The floor is the app's own minimum window size — read it, so this sweep follows
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
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const click = async (id) => { const e = document.getElementById(id); if (e) { e.click(); await sleep(200); } };
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
        '.tile-folder-pop, .view-group-pop, .toolbar-menu, .suggest-pop, .pick-history-pop')) {
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
      if (!/\\n/.test(msg)) out.errors.push('take-over dialog lost its line breaks');
      await click('confirm-cancel'); // "start empty"
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
        // 97: the button row is centered, so the FIRST button sits as far from the
        // modal's left edge as the LAST button sits from its right edge -- checking
        // .modal-actions itself would be wrong, that flex container always spans the
        // full inner width regardless of justify-content; only its children move.
        const aboutBox = aboutModal.querySelector('.modal');
        const firstBtn = document.getElementById('about-repo');
        const lastBtn = document.getElementById('about-ok');
        if (aboutBox && firstBtn && lastBtn) {
          const mb = aboutBox.getBoundingClientRect();
          const fb = firstBtn.getBoundingClientRect();
          const lb = lastBtn.getBoundingClientRect();
          const leftGap = fb.left - mb.left;
          const rightGap = mb.right - lb.right;
          if (Math.abs(leftGap - rightGap) > 1) {
            out.errors.push('97: About dialog buttons are not centered, left gap ' + Math.round(leftGap) + 'px vs right gap ' + Math.round(rightGap) + 'px');
          }
        }
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
          mergeBtn.click();
          await sleep(150);
          document.getElementById('confirm-input').value = 'MergeDstTmp';
          document.getElementById('confirm-ok').click(); // target picker
          await sleep(150);
          document.getElementById('confirm-ok').click(); // "N prompts affected" confirmation
          await sleep(250);
          if (rowFor('MergeSrcTmp')) out.errors.push('423: "MergeSrcTmp" is still listed after merging it into "MergeDstTmp"');
          const stillShown = [...document.querySelectorAll('#library-list .tag-chip')].some((c) => c.textContent === 'MergeSrcTmp');
          if (stillShown) out.errors.push('423: a library row still shows the merged-away tag');
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
          removeBtn.click();
          await sleep(150);
          document.getElementById('confirm-input').value = 'Smoke422';
          document.getElementById('confirm-ok').click();
          await sleep(150);
          const rows2 = [...document.querySelectorAll('.lib-item')].filter((el) => selectedIds.includes(el.dataset.id));
          if (rows2.some((el) => el.textContent.includes('Smoke422'))) {
            out.errors.push('422: removing the tag left it showing on a row');
          }
        }
      }
      document.getElementById('lib-batch-none').click();
      await click('library-select-btn'); // back out of select mode
    }
    await click('library-close');
    await click('journal-btn');
    layoutScan('copy history');
    mustReach('copy history', '#journal .modal-head-actions > button');
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
    await click('journal-close');
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
        textTile.querySelector('.tile-chain-start').click();
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
          for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
          await sleep(100);
        }
      }
    }
    await click('clip-btn');
    await click('clip-close');
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
      // Switch off (real click, so the expert-menu wiring is proven too): no lines
      // at all, although the very same data is still sitting in settings.last_used.
      const expertRow428 = async (label) => {
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
      const timeTipRow = await expertRow428('Last copied/edited in the tooltip');
      if (!timeTipRow) {
        out.errors.push('428: the tooltip-times option is missing from the expert menu');
      } else {
        timeTipRow.querySelector('input.switch').click();
        await sleep(200);
        const t3 = byId(copyId);
        const times3 = (t3 || {}).dataset ? t3.dataset.tipTimes || '' : '';
        if (times3) out.errors.push('428: turning the switch off still leaves tooltip time lines: "' + times3 + '"');
        const timeTipRowOff = await expertRow428('Last copied/edited in the tooltip');
        if (timeTipRowOff) timeTipRowOff.querySelector('input.switch').click();
        await sleep(200);
      }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // 1: the empty-grid hint stays out of the way once prompts exist.
    if (!document.getElementById('grid-empty').classList.contains('hidden')) {
      out.errors.push('the empty-grid hint shows although the grid has prompts');
    }
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
      // 425: a text default in the prompt itself prefills the field on first
      // ask (Text=Hello); a variable with no default stays empty (Language).
      if (varsOpen) {
        const prefill = document.querySelectorAll('#vars-fields input[type=text]');
        if (prefill[0]?.value !== 'Hello') out.errors.push('425: the text default did not prefill the field: ' + prefill[0]?.value);
        if (prefill[1]?.value) out.errors.push('425: a variable with no default was prefilled anyway: ' + prefill[1].value);
      }
      // 37: "Clear all" empties every text field in one click, without touching a
      // choice dropdown (there is none here, but the handler must not throw either).
      if (varsOpen) {
        const varsTexts = document.querySelectorAll('#vars-fields input[type=text]');
        for (const el of varsTexts) el.value = 'filled';
        await click('vars-clear-all');
        for (const el of varsTexts) {
          if (el.value !== '') out.errors.push('"clear all" left a variable field non-empty: ' + el.value);
        }
      }
      await click('vars-ok');
      await sleep(250);
      if ((window.__SMOKE_CALLS__ || []).includes('paste_into_previous')) {
        out.errors.push('a single click on a variable prompt pasted into the previous app');
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
    const varsHidden = () => document.getElementById('vars-modal').classList.contains('hidden');
    // F12: a prompt whose placeholders all resolve themselves must never ask, and
    // the copied text must carry no leftover {#{ }#} fence.
    window.__LAST_COPY__ = undefined;
    if (!await clickTile('p3')) {
      out.errors.push('the dynamic-placeholder prompt is missing from the grid');
    } else {
      if (!varsHidden()) { out.errors.push('an automatic placeholder still opened the fill-in dialog'); await click('vars-close'); }
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
    // F13: a mixed prompt asks only for the manual variable and offers the choice
    // as a themed dropdown; the pick lands in the copied text.
    window.__LAST_COPY__ = undefined;
    if (!await clickTile('p5')) {
      out.errors.push('the choice-variable prompt is missing from the grid');
    } else if (varsHidden()) {
      out.errors.push('the mixed variable prompt did not open the fill-in dialog');
    } else {
      const sel = document.querySelector('#vars-fields select.vars-select');
      const texts = document.querySelectorAll('#vars-fields input[type=text]');
      if (!sel) {
        out.errors.push('the choice variable is not rendered as a dropdown');
      } else {
        if ([...sel.options].map(o => o.value).join(',') !== 'formal,casual,witty') {
          out.errors.push('the choice dropdown lists the wrong options');
        }
        // 425: the token's own default (tone=casual) preselects that entry
        // instead of the list's first one.
        if (sel.value !== 'casual') out.errors.push('425: the choice dropdown did not preselect the text default: ' + sel.value);
        sel.value = 'witty';
      }
      if (texts.length !== 1) out.errors.push('the mixed dialog shows ' + texts.length + ' text fields instead of 1');
      if (texts[0]) texts[0].value = 'Rust';
      await click('vars-ok');
      await sleep(250);
      const copied = window.__LAST_COPY__ || '';
      if (copied !== 'Write a witty note about Rust.') out.errors.push('the filled text is wrong: ' + copied);
    }
    // The chosen option must come back preselected on the next copy.
    if (await clickTile('p5')) {
      const sel2 = document.querySelector('#vars-fields select.vars-select');
      if (!sel2 || sel2.value !== 'witty') out.errors.push('the last choice was not remembered');
      // P36: a remembered pick says so, since a choice variable has no checkbox.
      if (sel2 && !sel2.title.trim()) out.errors.push('the remembered choice has no "from last time" hint');
      await click('vars-close');
      await sleep(200);
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
    if (await clickTile('p1')) {
      const texts = document.querySelectorAll('#vars-fields input[type=text]');
      for (const f of texts) f.value = 'x';
      await click('vars-ok');
      await sleep(250);
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
        document.body.classList.remove('flag-promptNotes');
        await sleep(30);
        if (getComputedStyle(nRow).display !== 'none') {
          out.errors.push('promptNotes off does not hide the notes row');
        }
        document.body.classList.add('flag-promptNotes');
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
          const allPopup = document.getElementById('hotkeys-modal');
          if (allPopup.classList.contains('hidden')) {
            out.errors.push('430: the show-all button did not open the hotkeys list');
          }
          document.getElementById('hotkeys-modal-close').click();
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
        const popup = document.getElementById('hotkeys-modal');
        if (popup.classList.contains('hidden')) {
          out.errors.push('the "All hotkeys" popup did not open');
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
        document.getElementById('hotkeys-modal-close').click();
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
    if (!document.getElementById('vars-modal').classList.contains('hidden')) {
      out.errors.push('the hotkey fill-in opened the in-window dialog as well');
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

    // P92: the first close-to-tray shows a one-time hint before the window actually
    // hides — dismissing it must still hide the window.
    {
      const win = window.__TAURI__.window.getCurrentWindow();
      let hidden = false;
      const origHide = win.hide;
      win.hide = async () => { hidden = true; };
      window.__FIRE__('first-tray-hide');
      await sleep(200);
      if (document.getElementById('confirm-modal').classList.contains('hidden')) {
        out.errors.push('the first-tray-hide hint did not show a dialog');
      }
      if (hidden) out.errors.push('the window hid before the tray hint was dismissed');
      document.getElementById('confirm-ok').click();
      await sleep(200);
      if (!hidden) out.errors.push('dismissing the tray hint did not hide the window');
      win.hide = origHide;
    }

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
          const pop = document.querySelector('.num-pop');
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
            if (!document.querySelector('.num-pop').classList.contains('hidden')) {
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
        await click('grid-batch-delete');
        await sleep(200);
        if (gridTile('p4')) out.errors.push('the second press did not delete the tile');
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
            const popRows = () => [...document.querySelectorAll('.views-pop .view-group-item')];
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
      // (p1 asks for placeholders, so a copy would show up as the fill-in dialog).
      calls().length = 0;
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, clientX: 20, clientY: 20 }));
      await sleep(200);
      if (!document.getElementById('vars-modal').classList.contains('hidden')) {
        out.errors.push('a drag-out also started a copy');
        await click('vars-close');
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
    // Duplicates: its own dialog, reachable only from the expert menu, and the last
    // window of the main surface that no scan had ever opened. Its group (147) lives
    // in the "System & data" tab, not whatever tab was last active.
    document.getElementById('settings').classList.remove('hidden');
    await click('expert-open');
    const sysTab = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')].find(b => /system/i.test(b.textContent));
    if (sysTab) { sysTab.click(); await sleep(200); }
    const dupBtn = [...document.querySelectorAll('#expert-flags button')]
      .find(b => /duplicat|duplik|dupli/i.test(b.textContent));
    if (!dupBtn) {
      out.errors.push('the expert menu has no way to open the duplicates dialog');
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
        if (document.querySelectorAll('#dupes-body .dup-col').length !== 2) {
          out.errors.push('a duplicate group does not show both of its entries');
        }
        layoutScan('duplicates');
        mustReach('duplicates', '#dupes-body .dup-keep, #dupes-modal .modal-head-actions > button');
        await click('dupes-close');
        await sleep(200);
      }
    }
    await click('expert-back');
    document.getElementById('settings').classList.add('hidden');
    await sleep(150);
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
    if (!await expertRow('View groups')) {
      out.errors.push('the view-group toggle is missing from the expert menu');
    } else {
      if (!rowFor('Rows in the group list')) {
        out.errors.push('the group-list row count is missing although view groups are on');
      }
      await flip('View groups');
      if (rowFor('Rows in the group list')) {
        out.errors.push('the group-list row count survived switching view groups off');
      }
      if (!rowFor('View groups')) out.errors.push('switching view groups off also hid its own toggle');
      await flip('View groups');
      if (!rowFor('Rows in the group list')) out.errors.push('the group-list row count did not come back');
      // Transitive: the parent of the parent takes the whole branch with it.
      await flip('Multiple views');
      for (const gone of ['View groups', 'Rows in the group list', 'View tab border width']) {
        if (rowFor(gone)) out.errors.push('"' + gone + '" survived switching multiple views off');
      }
      await flip('Multiple views');
      if (!rowFor('View groups')) out.errors.push('the view-group toggle did not come back');
    }
    // The same for a sub-toggle: the preview owns the highlighter and its size.
    if (!await expertRow('Markdown preview')) {
      out.errors.push('the Markdown preview toggle is missing from the expert menu');
    } else {
      if (!rowFor('Syntax colours in code blocks')) {
        out.errors.push('the highlighter toggle is missing although the preview is on');
      }
      await flip('Markdown preview');
      for (const gone of ['Syntax colours in code blocks', 'Detect code without fences', 'Code text size']) {
        if (rowFor(gone)) out.errors.push('"' + gone + '" survived switching the preview off');
      }
      await flip('Markdown preview');
      if (!rowFor('Detect code without fences')) out.errors.push('the code detection toggle did not come back');
    }
    const firstTab2 = document.querySelector('#expert-tabs-bar .expert-tab');
    if (firstTab2) { firstTab2.click(); await sleep(150); }
    await click('expert-back');
    document.getElementById('settings').classList.add('hidden');
    // F9 view groups: a group is one folder tab, its popover lists the members,
    // picking one marks the group active and names the view on the tab, and taking
    // a view back out gives it its own tab again.
    const viewTabs = () => [...document.querySelectorAll('#views .view-btn:not(.view-add-top)')];
    const groupTab = () => viewTabs().find(b => b.dataset.groupId);
    if (!groupTab()) {
      out.errors.push('the view group has no folder tab');
    } else if (viewTabs().length !== 3) {
      out.errors.push('the tab bar shows ' + viewTabs().length + ' tabs instead of 3');
    } else {
      groupTab().click();
      await sleep(250);
      const rows = [...document.querySelectorAll('.view-group-pop .view-group-item')];
      if (rows.length !== 2) {
        out.errors.push('the group popover lists ' + rows.length + ' views instead of 2');
      } else {
        rows[1].click();
        await sleep(450);
        const gt = groupTab();
        if (!gt || !gt.classList.contains('active')) {
          out.errors.push('a view picked from the group did not mark the group tab active');
        } else if (!gt.querySelector('.view-group-current')) {
          out.errors.push('the group tab does not name the active view');
        }
        groupTab().click();
        await sleep(250);
        const outBtn = document.querySelector('.view-group-pop .view-group-item .view-group-out');
        if (!outBtn) {
          out.errors.push('a group row has no button to leave the group');
        } else {
          outBtn.click();
          await sleep(450);
          if (viewTabs().length !== 4) {
            out.errors.push('leaving the group did not give the view its own tab back');
          }
        }
      }
      const gt2 = groupTab();
      if (gt2) {
        gt2.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
        await sleep(200);
        const menu = document.querySelector('.tab-menu:not(.hidden)');
        const items = menu ? menu.querySelectorAll('button').length : 0;
        if (items !== 3) out.errors.push('the group menu has ' + items + ' entries instead of 3');
        document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
        await sleep(150);
      }
      // The gesture the feature is built on: one tab dropped on another merges them,
      // and a folder tab is a drag source in its own right (dropped on a folder the
      // two become one). Dragging a tab must not paint a text selection either.
      const tabCentre = (el) => { const r = el.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
      const tabPtr = (type, x, y, el) => (el || window).dispatchEvent(new PointerEvent(type, {
        bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 3, clientX: x, clientY: y }));
      const dragTab = async (from, to) => {
        // .views scrolls horizontally when squeezed (single-line header mode still
        // leaves it a variable share of the row) — a real user scrolls the strip
        // into view before dragging across it too.
        to.scrollIntoView({ block: 'nearest', inline: 'nearest' });
        await sleep(50);
        const [ax, ay] = tabCentre(from), [bx, by] = tabCentre(to);
        tabPtr('pointerdown', ax, ay, from);
        tabPtr('pointermove', ax + 8, ay);
        await sleep(80);
        tabPtr('pointermove', bx, by);
        await sleep(80);
        const marked = !!document.querySelector('.tab-drop');
        tabPtr('pointerup', bx, by);
        await sleep(350);
        return marked;
      };
      if (viewTabs().length && getComputedStyle(viewTabs()[0]).userSelect !== 'none') {
        out.errors.push('a view tab still selects its text while being dragged');
      }
      // The strip stays hit-testable at every window size the app allows, so the
      // gesture is testable everywhere — no width may skip this.
      const loose = viewTabs().filter(b => b.dataset.viewId);
      if (loose.length < 2) {
        out.errors.push('not enough loose view tabs for the merge check');
      } else {
        for (const tb of loose) {
          const r = tb.getBoundingClientRect();
          const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
          if (!hit || hit.closest('.view-btn') !== tb) {
            out.errors.push('a view tab cannot be dropped on: nothing hit-tests to "' +
              tb.textContent.trim().slice(0, 12) + '"');
          }
        }
        calls().length = 0;
        if (!await dragTab(loose[0], loose[1])) out.errors.push('a view tab dropped on another marked no target');
        if (document.getElementById('confirm-modal').classList.contains('hidden')) {
          out.errors.push('merging two view tabs did not ask for a group name');
        } else {
          await click('confirm-ok');
          await sleep(250);
          if (!calls().includes('save_view_group')) out.errors.push('merging two view tabs saved no group');
          if (viewTabs().some(b => b.dataset.viewId === loose[0].dataset.viewId)) {
            out.errors.push('a merged view still has its own tab');
          }
        }
        const groups = viewTabs().filter(b => b.dataset.groupId);
        if (groups.length !== 2) {
          out.errors.push('expected two folder tabs before the folder merge, got ' + groups.length);
        } else {
          calls().length = 0;
          if (!await dragTab(groups[0], groups[1])) out.errors.push('a folder tab cannot be dragged onto another tab');
          if (!calls().includes('save_view_group')) out.errors.push('dropping a folder on a folder merged nothing');
          const left = viewTabs().filter(b => b.dataset.groupId).length;
          if (left !== 1) out.errors.push(left + ' folder tabs left after merging two into one');
        }
      }
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
      const otherSw = rowByLabel2('Close library after copying')?.querySelector('input.switch');
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
            const otherAfter = rowByLabel2('Close library after copying')?.querySelector('input.switch');
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
              const backSw = rowByLabel2('Close library after copying')?.querySelector('input.switch');
              if (backSw && backSw.checked === otherWasChecked) { backSw.click(); await sleep(120); }
            }
          }
        }
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
        } else if (!advBody.contains(rotRow)) {
          out.errors.push('the rotating-texts switch is not under Advanced options (33)');
        } else if (!advBody.classList.contains('hidden')) {
          out.errors.push('advanced options does not start collapsed');
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
      layoutScan('backups');
      mustReach('backups', '#backup-modal .modal-head-actions > button');
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
        const findToggle = () => document.querySelector('#stats-panel .stats-head-row .switch-field input');
        const findRow = () => document.querySelector('#stats-panel .stats-topn-row');
        const waitFor = async (fn) => { for (let i = 0; i < 40 && !fn(); i++) await sleep(50); return fn(); };
        if (findToggle()) {
          findToggle().click();
          await waitFor(() => !findRow()); // re-render dropped the row while off
          const t2 = findToggle(); // 104: a fresh node -- the click above rebuilt the panel
          if (t2) {
            t2.click(); // back on: statistics stays on for the rest of the walk
            await waitFor(() => !!findRow());
          }
          checkTopN('statistics re-enabled');
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
    await click('backup-close');
    // Restoring a password-protected backup: the prompt must appear, reject a wrong
    // password with a new message, then accept the right one and confirm the result.
    const until = async (fn) => { for (let i = 0; i < 40 && !fn(); i++) await sleep(100); return fn(); };
    const pwShown = () => !document.getElementById('confirm-input-row').classList.contains('hidden');
    const scopeShown = () => document.getElementById('confirm-cb-data') && !document.getElementById('confirm-modal').classList.contains('hidden');
    document.getElementById('import-all').click();
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
    // on, defaults to on, and its length slider only matters while it is.
    {
      const valueRow431 = (label) => [...document.querySelectorAll('#expert-flags .value-field')]
        .find(r => ((r.querySelector('.value-head span') || {}).textContent || '').includes(label));
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const watcherRow = await expertRow('Clipboard inbox');
      if (!watcherRow) {
        out.errors.push('431: the clipboard inbox option is missing from the expert menu');
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
          if (!valueRow431('Password guess minimum length')) {
            out.errors.push('431: the minimum-length slider is missing although password-skip is on');
          }
          skipRow.querySelector('input.switch').click();
          await sleep(200);
          if (valueRow431('Password guess minimum length')) {
            out.errors.push('431: the minimum-length slider survived turning password-skip off');
          }
          // Re-fetch: password-skip gates the slider's visibility, so its own switch
          // rebuilt the tab and the earlier `skipRow` element is now detached.
          const skipRowBack = await expertRow('Skip likely passwords');
          if (skipRowBack) skipRowBack.querySelector('input.switch').click();
          await sleep(200);
          if (!valueRow431('Password guess minimum length')) {
            out.errors.push('431: the minimum-length slider did not come back with password-skip');
          }
        }
        // Restore the shipped default (off) so later checks see it unchanged.
        const watcherRowBack = await expertRow('Clipboard inbox');
        if (watcherRowBack) watcherRowBack.querySelector('input.switch').click();
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
        const seToolbar = document.querySelector('.snip-edit-toolbar');
        const SE_TOOLBAR_CEIL = 96;
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
          // needs Ben's own hands, same as moving it (see WORK.md 179).
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
      // 342: scaling the image itself (not just the view). A filled rect is
      // drawn before the scale so the test can check that the SAME drawn
      // content survives at the proportionally scaled position afterward --
      // a real 400x300 stub, not a 1x1 one, or the check would be as vacuous
      // as the 363/365 lesson already found once (see WORK-IMPL 342/365).
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
      // 365: the fullscreen media viewer (lightbox) keeps the point under the
      // cursor fixed while wheel-zooming instead of always zooming to its own
      // centre. Opened through a real prompt-editor preview click; the image's
      // own top-left corner (as far from its centre as a point gets) is the
      // known, trackable coordinate.
      {
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
      for (const theme of BUILTIN_THEMES) {
        document.documentElement.setAttribute('data-theme', theme);
        window.__FIRE__('snip-captured', { data_url: TINY_PNG2, path: '', name: 'test.png' });
        await sleep(150);
        layoutScan('screenshot editor in theme ' + theme);
        document.getElementById('snip-edit-cancel').click();
        await sleep(100);
      }
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
      document.getElementById('rec-modal-saveas').click();
      await sleep(30);
      if (!window.__SMOKE_CALLS__.includes('save_recording_as')) {
        out.errors.push('rec-modal: Save as did not call save_recording_as');
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

      // 384: the "window size" canvas default's reactive notice -- appears
      // BEFORE the result menu, names the reported second, its confirm
      // button writes recCanvasMode to "all", and its checkbox turns off
      // recCropNotice so a later occurrence stays silent.
      const confirmModal = document.getElementById('confirm-modal');
      window.__SMOKE_ARGS__ = [];
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw5-cropnotice.mp4', resize_crop_at_ms: 4200 });
      await sleep(30);
      if (confirmModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: resize-crop notice did not appear for resize_crop_at_ms');
      }
      if (!document.getElementById('confirm-msg').textContent.includes('4')) {
        out.errors.push('rec-modal: resize-crop notice message did not mention the reported second');
      }
      if (!recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: the result menu must not open before the resize-crop notice is dismissed');
      }
      document.getElementById('confirm-cb-dontShowAgain').checked = true;
      document.getElementById('confirm-ok').click();
      await sleep(30);
      if (!confirmModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: resize-crop notice did not close on confirm');
      }
      if (recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: the result menu did not open once the resize-crop notice was dismissed');
      }
      const canvasCall = window.__SMOKE_ARGS__.find(([cmd, args]) => cmd === 'set_ui_text' && args && args.key === 'recCanvasMode');
      if (!canvasCall || canvasCall[1].value !== 'all') {
        out.errors.push('rec-modal: confirming the resize-crop notice did not set recCanvasMode to all');
      }
      const noticeFlagCall = window.__SMOKE_ARGS__.find(([cmd, args]) => cmd === 'set_ui_flag' && args && args.key === 'recCropNotice');
      if (!noticeFlagCall || noticeFlagCall[1].enabled !== false) {
        out.errors.push('rec-modal: checking "do not show again" did not turn off recCropNotice');
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);

      // A later occurrence must stay silent once recCropNotice is off, but
      // the result menu must still open normally.
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw6-cropnotice-off.mp4', resize_crop_at_ms: 1000 });
      await sleep(30);
      if (!confirmModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: resize-crop notice must stay off once "do not show again" was checked');
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

      const langsRow = rowByLabel('Sprachen verwalten…');
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
        // than jumping back to the front -- Ben reproduced this landing at the
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
      document.getElementById('tour-open-btn').click();
      await sleep(150);
      // The height-per-step comparison below is sensitive to font-metric settling;
      // wait for webfonts to finish loading so it does not race one.
      try { await document.fonts.ready; } catch (e) {}
      await sleep(50);
      const tourEl = document.getElementById('tour-modal');
      if (!tourEl || tourEl.classList.contains('hidden')) {
        out.errors.push('Tour did not open from the settings-dialog button');
      } else {
        const isRawKey = (s) => /^(flag|val|tip|exp|stat|backup|help|tour)[A-Z][A-Za-z0-9]+$/.test((s || '').trim());
        const body = document.getElementById('tour-body');
        const heights = [];
        for (let step = 0; step < 5; step++) {
          const title = document.getElementById('tour-title').textContent.trim();
          if (!title || isRawKey(title)) out.errors.push('Tour step ' + step + ': heading missing or raw key (' + title + ')');
          if (step < 4) {
            const art = body.querySelector('.tour-art svg');
            if (!art) out.errors.push('Tour step ' + step + ': no drawing');
            const lines = [...body.querySelectorAll('.tour-text p')].map((p) => p.textContent.trim());
            if (!lines.length || lines.some((l) => !l || isRawKey(l))) {
              out.errors.push('Tour step ' + step + ': missing text line or raw key (' + JSON.stringify(lines) + ')');
            }
            if (lines.length > 3) out.errors.push('Tour step ' + step + ': more than three text lines');
          } else {
            const rows = [...document.querySelectorAll('.tour-opt-row')];
            if (rows.length !== 4) out.errors.push('Tour step 5: expected 4 option rows, found ' + rows.length);
            const wantOn = { screenshot: true, colorPicker: true, ocr: true, voice: true };
            for (const row of rows) {
              const cb = row.querySelector('input.switch');
              const note = row.querySelector('.tour-opt-default').textContent.trim();
              if (isRawKey(note)) out.errors.push('Tour step 5: default-note renders a raw key (' + note + ')');
              const desc = row.querySelector('.tour-opt-desc').textContent.trim();
              if (!desc || isRawKey(desc)) out.errors.push('Tour step 5: option description missing or raw key');
            }
            const boxes = rows.map((r) => r.querySelector('input.switch').checked);
            if (JSON.stringify(boxes) !== JSON.stringify([true, true, true, true])) {
              out.errors.push('Tour step 5: default switch states are ' + JSON.stringify(boxes) + ', expected [true,true,true,true] (shipsOn defaults)');
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
            if (JSON.stringify(restored) !== JSON.stringify([true, true, true, true])) {
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
        // Background click asks first (point 193) -- Continue leaves the tour open.
        const confirmModal = document.getElementById('confirm-modal');
        tourEl.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
        await sleep(80);
        if (confirmModal.classList.contains('hidden')) out.errors.push('Tour: background click did not open the skip-confirm dialog');
        if (tourEl.classList.contains('hidden')) out.errors.push('Tour: background click closed the tour before confirming');
        // .click() bypasses hit-testing, so it would pass even if the confirm
        // dialog rendered visually BEHIND the tour -- check the actual stacking
        // order (point 185, track 5) so a real mouse click would land on it too.
        const tourZ = parseInt(getComputedStyle(tourEl).zIndex, 10) || 0;
        const confirmZ = parseInt(getComputedStyle(confirmModal).zIndex, 10) || 0;
        if (confirmZ <= tourZ) {
          out.errors.push('Tour: skip-confirm dialog z-index (' + confirmZ + ') does not sit above the tour (' + tourZ + ')');
        }
        document.getElementById('confirm-cancel').click();
        await sleep(80);
        if (!confirmModal.classList.contains('hidden')) out.errors.push('Tour: Continue did not close the skip-confirm dialog');
        if (tourEl.classList.contains('hidden')) out.errors.push('Tour: Continue on the skip-confirm dialog closed the tour');
        // Escape follows the same path, and must not also fall through to tourKeydown's
        // own Escape handling while the confirm dialog sits on top of the tour. This is
        // also the block's one and only close -- a second open/close cycle in the same
        // probe run has been observed to stall the headless browser's virtual clock, so
        // the top-right Skip button (which stays deliberately direct, no confirmation)
        // is covered by inspection instead of a second live cycle here.
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await sleep(80);
        if (confirmModal.classList.contains('hidden')) out.errors.push('Tour: Escape did not open the skip-confirm dialog');
        document.getElementById('confirm-ok').click();
        await sleep(80);
        if (!confirmModal.classList.contains('hidden')) out.errors.push('Tour: confirming skip did not close the skip-confirm dialog');
        if (!tourEl.classList.contains('hidden')) out.errors.push('Tour: confirming skip did not close the tour');
        if (!window.__SMOKE_CALLS__.includes('set_ui_flag')) out.errors.push('Tour: confirming skip did not write tourSeen');
      }
      document.getElementById('settings').classList.add('hidden');
      await sleep(50);
    }
    // Point 419: shrinking a view below its member count must show an overhang
    // row instead of losing tiles, and growing it back must restore everyone -
    // exactly the arrangement it had before, membership included.
    {
      // The view-group merge test above (F9) leaves "home" folded into a group tab
      // with no standalone .view-btn of its own, so fall back to picking it from
      // that group's popover when the plain tab lookup comes up empty.
      const homeTab419 = document.querySelector('.view-btn[data-view-id="home"]');
      if (homeTab419) {
        homeTab419.click();
        await sleep(150);
      } else {
        document.querySelector('#views .view-btn[data-group-id]')?.click();
        await sleep(200);
        const homeRow = document.querySelector('.view-group-pop .view-group-item[data-view-id="home"]');
        if (homeRow) { homeRow.click(); await sleep(150); }
        else out.errors.push('Overflow: could not find "home" anywhere to make it active');
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


# F25: the folder window is the only place its UI is observable — the closed icon
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
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await sleep(30);
    if (window.__TRAY_ACTIVATED__ !== 'tray-copy-p1') {
      errs.push('Enter did not activate the highlighted row: ' + window.__TRAY_ACTIVATED__);
    }
    // Same "one action ever" guard the launcher and vars windows use: a stray
    // Escape after a real activation must never ALSO fire tray_window_close.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await sleep(30);
    if (window.__TRAY_CLOSED__) errs.push('closing fired even though an activation already had');
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
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
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
    ];
    for (const [btnId, liveClass] of overlayKinds) {
      document.getElementById(btnId).click();
      await sleep(30);
      if (!liveLayer.querySelector('.' + liveClass)) {
        errs.push(btnId + ' did not produce a .' + liveClass + ' element in the live overlay preview');
      }
    }
    // Every overlay just defaulted to the clip's [0, ve-duration] range --
    // seeking to the exact start must still keep them all visible (inclusive
    // boundary), proving the gate really reads the range rather than always
    // showing everything regardless of the playhead.
    document.getElementById('ve-tab-edit').click();
    await sleep(30);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home' }));
    await sleep(30);
    if (liveLayer.children.length !== overlayKinds.length) {
      errs.push('live overlay preview lost overlays at the exact start of their range (playhead 0)');
    }
    document.getElementById('ve-tab-overlays').click();
    await sleep(30);

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
    document.getElementById('ve-export-choose-file').click();
    await sleep(80);
    if (!destInput.value.startsWith('C:\\\\Videos')) {
      errs.push('"Save as..." did not adopt the picked file path');
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
    const formatSel = document.getElementById('ve-export-format');
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

    // ---- 387: the remaining-time estimate stays hidden below etaMinPct (10%
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
      errs.push(`387: remaining-time text showed before etaMinPct, got ${JSON.stringify(exportEtaEl.textContent)}`);
    }
    await sleep(120); // let this export resolve so a second one can start clean

    // ---- 387: past etaMinPct, a slow enough rate shows an "about ..." text ----
    // startExport() itself feeds an initial 0% sample the instant it starts, so
    // the very first real tick below is measured against that -- a real 3s gap
    // keeps that first rate low enough for the average to clear etaMinSecs.
    // export_video needs to still be pending 3s in, hence the longer delay.
    window.__EXPORT_DELAY_MS__ = 3500;
    exportStartBtn.click();
    await sleep(3000);
    window.__FIRE__('recording-convert-progress', { ms: 60000 }); // 10% after ~3s -> roughly 3.3%/s
    await sleep(10);
    if (!/about/i.test(exportEtaEl.textContent)) {
      errs.push(`387: remaining-time text did not appear past etaMinPct with a slow rate, got ${JSON.stringify(exportEtaEl.textContent)}`);
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


# A DPAPI failure makes the backend write the store in plaintext instead of
# refusing the save. That used to be reported in the error log and nowhere else.
# The same page is booted twice — once with the backend reporting a sealed store,
# once unsealed — and the run asserts BOTH directions: the warning appears when it
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
      document.getElementById('confirm-ok').click();
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


def unescape(s):
    for a, b in (("&quot;", '"'), ("&lt;", "<"), ("&gt;", ">"), ("&amp;", "&")):
        s = s.replace(a, b)
    return s


def main():
    browser = next((p for p in CHROME_CANDIDATES if Path(p).exists()), None)
    if not browser:
        # CI's runner image always ships Chrome (see ci.yml) — a missing browser there
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
                 # out mid-run on the smallest window and produced no result at all —
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
                 # editor-scale check added its own draw/resize/undo round trip.
                 # The window has to hold the iframe plus Chrome's own frame.
                 f"--window-size={max(w + 30, 520)},{h + 140}",
                 "--virtual-time-budget=154000", "--dump-dom",
                 wrapper.as_uri()],
                # The dumped DOM carries all 20 languages; without an explicit
                # codec Windows decodes it as cp1252 and the reader thread dies,
                # leaving stdout empty.
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=240)
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

        # 221: a pass in the longest-running UI language (Italian — averages ~42
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
                 # other) added its own round of tab walks and switch toggles.
                 "--virtual-time-budget=134000", "--dump-dom",
                 wrapper.as_uri()],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=240)
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

        # Each secondary window carries the only coverage its own UI can get — the
        # pill drag/merge, the colour picker and the whole folder window live here
        # and nowhere else.
        for name, size, probe, budget in (
                ("floating.html", (240, 60), FLOAT_PROBE, 15000),
                ("snip.html", (1280, 800), PICK_PROBE, 20000),
                ("folder.html", (520, 420), FOLDER_PROBE, 15000),
                ("vars.html", (340, 250), VARS_PROBE, 15000),
                ("launcher.html", (560, 420), LAUNCHER_PROBE, 15000),
                ("tray.html", (240, 360), TRAY_PROBE, 15000),
                ("recbar.html", (320, 56), RECBAR_PROBE, 15000)):
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

        # The store-sealing warning, both ways round. Its own boot each time: the
        # dialog only ever opens during startup, so it cannot be reached by walking
        # an already-running page. Wide enough that the settings dialog is not also
        # fighting for room — the small-window layout is the main sweep's job.
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

        print("PASS" if not failed else f"{failed} problem(s)")
        return 1 if failed else 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
