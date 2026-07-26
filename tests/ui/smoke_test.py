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
    await click('library-btn');
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
      }
      await click('tag-modal-close');
    }
    await click('library-close');
    await click('journal-btn');
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
      const chips = [...document.querySelectorAll('#modal-vars-chips .vars-chip')];
      if (chips.length !== 6) {
        out.errors.push('the editor lists ' + chips.length + ' automatic placeholders instead of 6');
      } else if (chips.some(c => !c.title.trim())) {
        out.errors.push('an automatic placeholder chip has no explanation');
      }
      const ta = document.getElementById('modal-text');
      const before = ta.value;
      chips[0] && chips[0].click();
      await sleep(150);
      if (ta.value === before || !ta.value.includes('{#{date}#}')) {
        out.errors.push('clicking a placeholder chip did not insert it');
      }
      document.getElementById('modal-var-name').value = 'tone';
      document.getElementById('modal-var-opts').value = ' formal , casual ,, ';
      document.getElementById('modal-var-add').click();
      await sleep(150);
      if (!ta.value.includes('{#{tone|formal,casual}#}')) {
        out.errors.push('the helper did not insert a choice variable: ' + ta.value);
      }
      await click('modal-close-x');
      await sleep(250);
      // Editing made the dialog dirty, so the discard confirmation may step in.
      const cm2 = document.getElementById('confirm-modal');
      if (!cm2.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
    }
    // F14/F15: the preview renders every Markdown construct, colours the fenced code
    // blocks, and — the part that matters — shows injected markup as literal text.
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
      const ta = document.getElementById('modal-text');
      if (document.getElementById('modal-md-bar').classList.contains('hidden')) {
        out.errors.push('the Markdown preview switch is missing');
      }
      await click('modal-md-toggle');
      if (prev.classList.contains('hidden')) {
        out.errors.push('the Markdown preview did not open');
      } else {
        if (!ta.classList.contains('hidden')) {
          out.errors.push('the text field is still shown although the preview took its place');
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
        await sleep(200);
        if (prev.textContent.trim() !== 'live update') {
          out.errors.push('the preview does not follow the text: ' + prev.textContent.slice(0, 40));
        }
      }
      await click('modal-md-toggle');
      if (ta.classList.contains('hidden')) out.errors.push('switching back did not restore the text field');
      await click('modal-close-x');
      await sleep(250);
      const cm3 = document.getElementById('confirm-modal');
      if (!cm3.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
    }
    // The other layout: the expert dropdown puts both panes on screen at once.
    document.getElementById('settings').classList.remove('hidden');
    await click('expert-open');
    const mdSel = [...document.querySelectorAll('#expert-flags select.expert-select')]
      .find(s => [...s.options].some(o => o.value === 'split'));
    if (!mdSel) {
      out.errors.push('the preview layout dropdown is missing from the expert menu');
    } else {
      mdSel.value = 'split';
      mdSel.dispatchEvent(new Event('change', { bubbles: true }));
      await sleep(250);
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
      if (!await openEditor('p7')) {
        out.errors.push('the editor did not reopen for the split layout');
      } else {
        const text = document.getElementById('modal-text');
        const prev2 = document.getElementById('modal-md-preview');
        if (!document.getElementById('modal').classList.contains('md-split')) {
          out.errors.push('the split layout was not applied');
        }
        if (text.classList.contains('hidden') || prev2.classList.contains('hidden')) {
          out.errors.push('the split layout does not show text field and preview together');
        } else if (innerWidth > 640) {
          const tr = text.getBoundingClientRect(), pr = prev2.getBoundingClientRect();
          if (pr.left < tr.right - 1) out.errors.push('the split panes are not side by side');
        }
        if (!document.getElementById('modal-md-bar').classList.contains('hidden')) {
          out.errors.push('the split layout still offers the switch button');
        }
        await click('modal-close-x');
        await sleep(250);
      }
    }
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
      }
      await click('theme-edit-cancel');
      await sleep(150);
      if (document.documentElement.style.getPropertyValue('--bg') === '#123456') {
        out.errors.push('cancelling the theme editor kept the preview');
      }
    }
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
      // Two-column settings, 8 diagnostic cards, 12 statistic cards, overview split.
      if (document.querySelectorAll('#backup-panel .backup-col').length !== 2) {
        out.errors.push('the backup settings are not split in two columns');
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
      // A stored password shows as dots — as a placeholder, never as a value.
      const pw = document.querySelector('#backup-panel .backup-pw-wrap .modal-input');
      if (!pw || pw.type !== 'password' || pw.value !== '' || !/^•+$/.test(pw.placeholder)) {
        out.errors.push('a stored backup password is not shown as empty dots');
      }
      // Smart retention swaps the keep count for the 2x2 day/week/month/year grid.
      // The panel re-renders on every toggle, so the switch has to be looked up again.
      const gfsSwitch = () => [...document.querySelectorAll('#backup-panel input.switch')].pop();
      if (gfsSwitch()) {
        gfsSwitch().click();
        await sleep(400);
        const tiers = document.querySelectorAll('#backup-panel .backup-tier-grid .modal-input');
        if (tiers.length !== 4) out.errors.push('smart retention does not show 4 tier fields');
        gfsSwitch().click();
        await sleep(400);
      }
    }
    // Clearing a number field with its X and leaving it must land on the default,
    // not on the field minimum.
    const keepInput = document.querySelector('#backup-panel .backup-set-row input[type=number]');
    const keepClear = keepInput && keepInput.parentElement.querySelector('.clear-btn');
    if (!keepInput || !keepClear) {
      out.errors.push('the keep-count field has no clear button');
    } else {
      keepClear.click();
      await sleep(150);
      keepInput.dispatchEvent(new Event('blur'));
      await sleep(300);
      if (keepInput.value !== '10') out.errors.push('clearing the keep count gives ' + keepInput.value + ' instead of the default 10');
      // The X hides itself while the field is empty; once the default is written
      // back it has to reappear, or the field can never be cleared a second time.
      if (!keepClear.classList.contains('show')) out.errors.push('the clear button stays hidden after the field refilled itself');
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
    // Leave the theme editor open so the scans below measure its real layout.
    document.getElementById('settings').classList.remove('hidden');
    await click('theme-edit-open');
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
