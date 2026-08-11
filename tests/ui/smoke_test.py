"""Headless smoke test: boot the real UI with a stubbed Tauri backend.

Catches the class of defect that only shows when the app actually runs:
  * a JS error during startup or while opening a dialog,
  * an element that overflows the window horizontally (layout bug),
  * a control parked outside the window, covered, or crushed to no height,
  * a string that renders as its raw i18n key (missing translation).

It walks the library, the copy history, the tile menu and every expert tab, down
to the smallest window the app itself allows, and once more with the appearance
scales raised. Skips cleanly when no Chromium browser is installed.

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
SIZES = [(1280, 800), (620, 500), (MIN_W + 60, MIN_H + 40), (MIN_W, MIN_H)]

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
    for (let p = el.parentElement; p; p = p.parentElement) {
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
        }
        if (totalGroups < 15) {
          out.errors.push('the help menu shows too few categories across all tabs (' + totalGroups + ')');
        }
        if (!sawTip) out.errors.push('no help category shows a tip callout');
        const search = document.getElementById('help-search');
        search.value = 'markdown';
        search.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(150);
        const filtered = groups();
        if (!filtered.length || filtered.length >= totalGroups) {
          out.errors.push('searching the help menu did not narrow the category list');
        }
        search.value = 'zzz_no_such_help_topic_zzz';
        search.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(150);
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
      const homeTab = document.querySelector('.view-btn[data-view-id="home"]');
      if (homeTab) { homeTab.click(); await sleep(200); }
    }
    await click('library-btn');
    layoutScan('library');
    mustReach('library', '#library .modal-head-actions > button');
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
      // actually has placeholders (p1: "Translate {#{Text}#} into {#{Language}#}.").
      const varHoverTile = byId('p1');
      if (varHoverTile) {
        await hover(varHoverTile, 90);
        const lenText = (document.querySelector('.tile-tip .tile-tip-len') || {}).textContent || '';
        if (!/^[0-9][0-9,.]* chars/.test(lenText) || !/~[0-9][0-9,.]* tokens$/.test(lenText)) {
          out.errors.push('the tile tooltip length readout looks wrong: "' + lenText + '"');
        }
        const varSpans = [...document.querySelectorAll('.tile-tip .tile-tip-body .var-token')];
        const varTexts = varSpans.map((s) => s.textContent);
        if (varTexts.length !== 2 || !varTexts.includes('{#{Text}#}') || !varTexts.includes('{#{Language}#}')) {
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
        if (sel.value !== 'formal') out.errors.push('the choice dropdown did not preselect the first option');
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
      // F17: the notes row opens collapsed, expands on click and holds the note.
      const nRow = document.getElementById('modal-notes');
      const nBody = document.getElementById('modal-notes-body');
      if (!nRow || getComputedStyle(nRow).display === 'none') {
        out.errors.push('the editor has no notes row');
      } else if (!nBody.classList.contains('hidden')) {
        out.errors.push('the notes row does not start collapsed');
      } else {
        document.getElementById('modal-notes-head').click();
        await sleep(250);
        if (nBody.classList.contains('hidden')) out.errors.push('the notes row did not expand');
        if (!document.getElementById('modal-notes-text').value.includes('Moonshot')) {
          out.errors.push('the editor did not load the stored note');
        }
        // P35: the note field reads as a different kind of box, not a second
        // prompt field — its background must not match the main text area's.
        const notesBg = getComputedStyle(document.getElementById('modal-notes-text')).backgroundColor;
        const promptBg = getComputedStyle(document.getElementById('modal-text')).backgroundColor;
        if (notesBg === promptBg) out.errors.push('the notes field looks identical to the prompt field');
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

    // F20: pressing a prompt hotkey runs the same activation a tile does — the copy
    // is recorded, and whether it also pastes follows the expert option alone.
    const calls = () => (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []);
    calls().length = 0;
    window.__FIRE__('prompt-hotkey', 'p0');
    await sleep(400);
    if (!calls().includes('copy_prompt') && !calls().includes('copy_text')) {
      out.errors.push('a prompt hotkey did not copy the prompt');
    }
    if (!calls().includes('record_copy')) out.errors.push('a prompt hotkey did not record the copy');
    if (calls().includes('paste_into_previous')) {
      out.errors.push('a prompt hotkey pasted although the paste option is off');
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
      pasteRow.querySelector('input.switch').click();
      await sleep(250);
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
      calls().length = 0;
      window.__FIRE__('prompt-hotkey', 'p2');
      await sleep(500);
      if (!calls().includes('paste_into_previous')) {
        out.errors.push('the prompt hotkey did not paste although the option is on');
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
          await click('trash-clear');
          await sleep(150);
          const cm2 = document.getElementById('confirm-modal');
          if (cm2.classList.contains('hidden')) {
            out.errors.push('emptying the trash skipped the confirm dialog');
          } else {
            await click('confirm-ok');
            await sleep(200);
            if (!trashBody.textContent.includes('Trash is empty')) {
              out.errors.push('emptying the trash left the dialog without the empty-state message');
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
      if (dropped !== 'Translate {#{Text}#} into {#{Language}#}.') {
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
    // proves the flag actually hides/unhides the control (Ben checks the hover by eye).
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
          } else if (!changedRow.title || !changedRow.title.includes('Changed:')) {
            // 66: the row's tooltip gets a "Changed: <date>" line once it differs
            // from its default — proves the backend actually stamped ui_changed and
            // the frontend read it back, not just that the filter itself works.
            out.errors.push('a row flipped away from its default has no "Changed:" tooltip line (66)');
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
        if (row.querySelector('.expert-row-reset')) {
          out.errors.push('per-value reset icon showed on an already-default row');
        }
        const before = sw.checked;
        sw.click();
        await sleep(150);
        const rowNow = rowByLabel4('Always show grid lines');
        const resetBtn = rowNow && rowNow.querySelector('.expert-row-reset');
        if (!resetBtn) {
          out.errors.push('per-value reset icon did not appear on a changed row');
        } else {
          resetBtn.click();
          await sleep(200);
          const rowAfter = rowByLabel4('Always show grid lines');
          const swAfter = rowAfter && rowAfter.querySelector('input.switch');
          if (!swAfter || swAfter.checked !== before) {
            out.errors.push('per-value reset did not restore the row to its default');
          }
          if (rowAfter && rowAfter.querySelector('.expert-row-reset')) {
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
          const taWrap = document.querySelector('#modal .clear-wrap-ta');
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
          // 33: the order/random control sits in the "+ Entry" row, pushed right,
          // with the reset ("from the start") to its left — not on a row of its own.
          const addBtn = document.getElementById('modal-rotate-add');
          const modeSel = document.getElementById('modal-rotate-mode');
          const resetBtn = document.getElementById('modal-rotate-reset');
          if (addBtn && modeSel && ctrlRow) {
            const a = addBtn.getBoundingClientRect(), m = modeSel.getBoundingClientRect();
            const r = ctrlRow.getBoundingClientRect();
            if (Math.abs(a.top - m.top) > 6) out.errors.push('the mode control is not on the "+ Entry" row');
            if (m.left <= a.right) out.errors.push('the mode control is not right of "+ Entry"');
            if (r.right - m.right > 12) out.errors.push('the mode control is not flush right in its row');
            if (resetBtn && !resetBtn.classList.contains('hidden')) {
              const rs = resetBtn.getBoundingClientRect();
              if (rs.right > m.left + 1) out.errors.push('"from the start" is not left of the mode control');
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
              // this probe string before the browser ever sees it (see CLAUDE.md).
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
              if (a.text !== 'Entry one' || JSON.stringify(a.variants) !== JSON.stringify(['Entry two']) || a.rotateMode !== 'random') {
                out.errors.push('add_prompt got the wrong rotating payload: ' + JSON.stringify({ text: a.text, variants: a.variants, rotateMode: a.rotateMode }));
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
      }
      if (document.querySelectorAll('#backup-panel .stat-cards .stat-card').length !== 8) {
        out.errors.push('the backup diagnostics do not show 8 cards');
      }
      if (document.querySelectorAll('#stats-panel .stat-cards .stat-card').length !== 12) {
        out.errors.push('the usage statistics do not show 12 cards');
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
          if (!box || !box.classList.contains('backup-names') || !box.classList.contains('hidden')) {
            out.errors.push('the names box is not collapsed by default');
          } else {
            toggle.click();
            await sleep(150);
            if (box.classList.contains('hidden')) out.errors.push('clicking the count did not reveal the names');
            if (!box.textContent.includes('Draft reply') || !box.textContent.includes('Meeting notes')) {
              out.errors.push('the expanded names box is missing a fixture name: ' + box.textContent);
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
    // P64: restoring a managed backup goes through maybeBackupBeforeWipe() — the
    // same safety net already used before reset/delete-all. That helper's own
    // gating (the default-off autoBackupBeforeWipe flag) is this app's
    // established, already-relied-on mechanism; here we only prove restoreBackup
    // actually calls it, using the fixture's default (off) state, so a nothing-
    // happens result is the correct one and cheap to verify without walking the
    // expert menu (which a later 130%-zoom layout check turned out to be
    // sensitive to — not worth touching under this pass's time budget).
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
        if (calls().includes('create_backup')) {
          out.errors.push('a safety backup ran although autoBackupBeforeWipe is off by default');
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
      // F20: hotkeys the restore had to give up are named, not silently dropped.
      else if (!/hotkey/i.test(document.getElementById('confirm-msg').textContent)) {
        out.errors.push('the import summary does not mention the dropped hotkeys');
      }
      await click('confirm-ok');
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
      window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 }));
      await sleep(60);
      if (!calls().includes('set_float_folder_scale')) errs.push('the resize never reached the backend');
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
                 # restore-scope round trips) added their own expert-menu detours.
                 # The window has to hold the iframe plus Chrome's own frame.
                 f"--window-size={max(w + 30, 520)},{h + 140}",
                 "--virtual-time-budget=95000", "--dump-dom",
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

        # Each secondary window carries the only coverage its own UI can get — the
        # pill drag/merge, the colour picker and the whole folder window live here
        # and nowhere else.
        for name, size, probe, budget in (
                ("floating.html", (240, 60), FLOAT_PROBE, 15000),
                ("snip.html", (1280, 800), PICK_PROBE, 20000),
                ("folder.html", (520, 420), FOLDER_PROBE, 15000),
                ("vars.html", (340, 250), VARS_PROBE, 15000),
                ("launcher.html", (560, 420), LAUNCHER_PROBE, 15000)):
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

        print("PASS" if not failed else f"{failed} problem(s)")
        return 1 if failed else 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
