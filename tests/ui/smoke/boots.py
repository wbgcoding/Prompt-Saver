"""Single-boot suite: one fresh boot of a page per probe, for the checks that only show up during
startup or need their own seeded store."""
import json
import re

from .harness import REPO, unescape, run_chrome


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
      // Containers only - a lone <path> inside an icon is empty by nature.
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


# 390/455: the automatic first-run open of the tour, which the main sweep above never
# exercises (the fixture ships tourSeen: true so the automatic open never interferes with
# any other check). Its own boot, same reasoning as SEAL_PROBE: the automatic open only
# ever happens once, right at startup. window.__TOUR_FIRSTRUN_MODE__ picks Finish or Skip:
# both must close the tour and write nothing but tourSeen -- the tour changes no setting.
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
      if (dots.length !== 4) errs.push(mode + ': expected 4 tour steps, found ' + dots.length);
      if (document.querySelector('.tour-opt-row, .tour-setup-row, .tour-restore-btn')) {
        errs.push(mode + ': the tour still renders a settings row');
      }
      dots[dots.length - 1].click(); // the last drawing page
      await sleep(150);
      window.__SMOKE_ARGS__ = [];
      if (mode === 'finish') {
        document.getElementById('tour-next').click(); // last step's button is Finish
      } else {
        document.getElementById('tour-skip').click(); // top-right Skip, no confirmation
      }
      await sleep(200);
      if (visible(tourEl)) errs.push(mode + ': the tour did not close');
      const calls = window.__SMOKE_ARGS__ || [];
      const tourSeenCall = calls.find((c) => c[0] === 'set_ui_flag' && c[1] && c[1].key === 'tourSeen');
      if (!tourSeenCall || tourSeenCall[1].enabled !== true) {
        errs.push(mode + ': tourSeen was not written on close');
      }
      const others = calls.filter((c) => c !== tourSeenCall && /^set_(ui_flag|autostart|auto_update|minimize_on_close)$/.test(c[0]));
      if (others.length) errs.push(mode + ': closing the tour wrote more than tourSeen: ' + JSON.stringify(others.map((c) => c[0])));
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

          // ---- Ablauf: the idle timeout drops the key; nothing tells this window, so
          // the relock timer (status remaining_secs + 1 s) must bring the bar back ----
          if (freshTile) {
            await sleep(5200);
            const after = document.querySelector('.tile[data-id=\\"psecret\\"]');
            if (!after || !after.classList.contains('tile-locked')) {
              errs.push('the tile stayed unlocked after the vault timeout ran out');
            }
            if (bodyHasLeaked()) errs.push('the real text stayed in the DOM after the vault timeout');
            if (!(window.__SMOKE_CALLS__ || []).includes('secret_vault_status')) {
              errs.push('the relock timer never asked the vault for its remaining time');
            }
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


# With the Undo time Off nothing can bring back a deleted view or a merged tag, so
# both ask first (a prompt delete goes to the trash and stays one click). Its own
# boot: window.__UNDO_OFF_TEST__ (see tauri_stub.js) starts the store with
# undoToastMs at 0.
UNDO_OFF_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--undo-off-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  const click = async (id) => { const e = document.getElementById(id); if (e) { e.click(); await sleep(200); } };
  const confirmOpen = () => !document.getElementById('confirm-modal').classList.contains('hidden');
  const undoShown = () => {
    const toast = document.getElementById('toast');
    return toast.classList.contains('show') && !!toast.querySelector('.toast-btn');
  };
  const menuItem = (text) => [...document.querySelectorAll('.tab-menu button[role="menuitem"]')]
    .find(el => el.textContent.trim() === text);
  const copyTab = () => [...document.querySelectorAll('#views .view-btn:not(.view-add-top)')]
    .find(b => (b.querySelector('span') || {}).textContent === 'Home (copy)');
  try {
    for (let i = 0; i < 80 && !document.querySelector('.view-btn[data-view-id="home"]'); i++) await sleep(100);
    // The first-run take-over question is still open: a later question waits behind it, so answer it.
    for (let i = 0; i < 30 && !confirmOpen(); i++) await sleep(100);
    if (confirmOpen()) await click('confirm-cancel');
    // View delete.
    const home = document.querySelector('.view-btn[data-view-id="home"]');
    home.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }));
    await sleep(150);
    menuItem('Duplicate').click();
    await sleep(300);
    if (!copyTab()) errs.push('the duplicate view for the delete check was not created');
    else {
      copyTab().dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }));
      await sleep(150);
      menuItem('Edit view').click();
      await sleep(200);
      await click('view-modal-delete');
      if (!confirmOpen()) errs.push('with Undo off, deleting a view did not ask first');
      if (!copyTab()) errs.push('with Undo off, a view was deleted before the question was answered');
      await click('confirm-cancel');
      if (!copyTab()) errs.push('cancelling the question still deleted the view');
      await click('view-modal-delete');
      await click('confirm-ok');
      await sleep(200);
      if (copyTab()) errs.push('confirming the question did not delete the view');
      if (undoShown()) errs.push('with Undo off, the delete toast still carries an Undo button');
    }
    // Tag merge: two disposable tags on one prompt, merged from the tag manager.
    await click('library-btn');
    await click('library-select-btn');
    document.querySelector('#library-list .lib-item[data-id="p1"]')?.click();
    await sleep(150);
    const add = document.getElementById('lib-batch-tag-add');
    for (const name of ['UoSrcTmp', 'UoDstTmp']) {
      add.click();
      await sleep(150);
      document.getElementById('confirm-input').value = name;
      document.getElementById('confirm-ok').click();
      await sleep(150);
    }
    document.getElementById('lib-batch-none').click();
    await click('library-select-btn');
    const mgr = [...document.querySelectorAll('#library-tagbar .tag-action')].pop();
    if (mgr) { mgr.click(); await sleep(250); }
    const rowFor = (name) => [...document.querySelectorAll('#tag-modal-list .tag-manage-row')]
      .find(r => (r.querySelector('.tag-manage-name') || {}).textContent === name);
    const pickTarget = async () => {
      rowFor('UoSrcTmp').querySelectorAll('.ghost-btn')[1].click();
      await sleep(150);
      document.getElementById('confirm-input').value = 'UoDstTmp';
      document.getElementById('confirm-ok').click();
      await sleep(250);
    };
    if (!rowFor('UoSrcTmp') || !rowFor('UoDstTmp')) errs.push('the disposable tags for the merge check are not listed');
    else {
      await pickTarget();
      if (!confirmOpen()) errs.push('with Undo off, merging tags did not ask first');
      if (!rowFor('UoSrcTmp')) errs.push('with Undo off, tags were merged before the question was answered');
      await click('confirm-cancel');
      if (!rowFor('UoSrcTmp')) errs.push('cancelling the question still merged the tags');
      await pickTarget();
      await click('confirm-ok');
      await sleep(250);
      if (rowFor('UoSrcTmp')) errs.push('confirming the question did not merge the tags');
      if (undoShown()) errs.push('with Undo off, the merge toast still carries an Undo button');
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


# 838: the first-launch update question. A fresh profile (stub flag
# __UPDATE_CONSENT_TEST__) is asked before anything else; "No" stores a refusal,
# Escape stores nothing so the question comes back on the next launch. Its own
# boot per answer because the question only opens during startup.
UPDATE_CONSENT_PROBE = """
<script>
(async () => {
  // 838:
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const mode = window.__UPDATE_CONSENT_MODE__;
  const beat = setInterval(() => {
    document.documentElement.style.setProperty('--uc-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  const visible = (el) => !!el && !el.classList.contains('hidden');
  const consentCalls = () => (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'set_update_consent');
  const checkCalls = () => (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'run_update_check');
  try {
    const cm = document.getElementById('confirm-modal');
    // Earlier startup dialogs (the data takeover offer) are closed with Cancel
    // until the update question itself is on screen.
    for (let i = 0; i < 100; i++) {
      if (visible(cm)) {
        if (document.getElementById('confirm-title').textContent.trim() === 'Check for updates?') break;
        document.getElementById('confirm-cancel').click();
        await sleep(300);
      } else {
        await sleep(100);
      }
    }
    if (!visible(cm)) {
      errs.push('838: ' + mode + ': an unanswered profile was not asked about updates');
    } else {
      const title = document.getElementById('confirm-title').textContent.trim();
      if (!title || /updConsent/.test(title)) errs.push('838: ' + mode + ': the question title is missing or a raw key: ' + title);
      const msg = document.getElementById('confirm-msg').textContent;
      if (/updConsent/.test(msg) || msg.indexOf('{app}') >= 0 || !msg.trim()) errs.push('838: ' + mode + ': the question text is missing or unfilled: ' + msg);
      const alt = document.getElementById('confirm-alt');
      if (!visible(alt) || alt.textContent.trim() !== 'No') errs.push('838: ' + mode + ': the No button is missing: ' + (alt && alt.textContent) + ' title=' + title);
      if (consentCalls().length) errs.push('838: ' + mode + ': an answer was stored before the user chose');
      if (mode === 'no') {
        alt.click();
      } else if (mode === 'yes') {
        document.getElementById('confirm-ok').click();
      } else {
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      }
      await sleep(400);
      const calls = consentCalls();
      if (mode === 'no') {
        if (calls.length !== 1 || !calls[0][1] || calls[0][1].enabled !== false) {
          errs.push('838: no: expected one set_update_consent {enabled:false}, got ' + JSON.stringify(calls));
        }
      } else if (mode === 'yes') {
        if (calls.length !== 1 || !calls[0][1] || calls[0][1].enabled !== true) {
          errs.push('838: yes: expected one set_update_consent {enabled:true}, got ' + JSON.stringify(calls));
        }
      } else if (calls.length) {
        errs.push('838: escape: Escape stored an answer: ' + JSON.stringify(calls));
      }
      if (visible(cm)) errs.push('838: ' + mode + ': the question stayed open after the answer');
      // Yes starts the first check right away, exactly once; No and Escape never go online.
      const nChecks = checkCalls().length;
      if (mode === 'yes' && nChecks !== 1) errs.push('838: yes: expected exactly one run_update_check, got ' + nChecks);
      if (mode !== 'yes' && nChecks) errs.push('838: ' + mode + ': an update check ran without a yes: ' + nChecks);
    }
  } catch (e) { errs.push('838: probe: ' + e.message); }
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
# reopened current-version-only from the About dialog, and its GitHub button.
WHATS_NEW_PROBE = """
<script>
window.__WHATS_NEW_FIXTURE__ = {
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
        if (document.getElementById('update-modal-github').classList.contains('hidden')) {
          errs.push('404: the notes view of the update dialog has no GitHub button');
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
        const githubBtn = document.getElementById('update-modal-github');
        if (githubBtn.textContent.trim() !== 'View on GitHub') {
          errs.push('404: the whats-new GitHub button reads ' + githubBtn.textContent.trim());
        }
        const repoCalls = () => (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'open_repo' && c[1] && c[1].page === 'releases').length;
        const repoBefore = repoCalls();
        githubBtn.click();
        await sleep(150);
        if (repoCalls() !== repoBefore + 1) {
          errs.push('404: the View on GitHub button did not open the releases page');
        }
        if (!(notes.textContent || '').includes('Current version only')) {
          errs.push('404: the View on GitHub button replaced the shipped list');
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
    // privacy-link: About links to the privacy page through the project-page command.
    const privacyBtn = document.getElementById('about-privacy');
    if (!privacyBtn) errs.push('privacy-link: the About dialog has no privacy button');
    else {
      privacyBtn.click();
      await sleep(80);
      const opened = (window.__SMOKE_ARGS__ || []).filter(([c]) => c === 'open_repo').at(-1);
      if (!opened || !opened[1] || opened[1].page !== 'privacy') {
        errs.push('privacy-link: the privacy button did not open the privacy page: ' + JSON.stringify(opened));
      }
    }
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


# 475: the extra-video-source limit is a fixed ten; the probe fills the list
# through the stub and proves the eleventh source is refused with the limit toast.
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
    // Every wait below polls the real DOM state instead of sleeping a fixed
    // guess -- a fixed sleep here was too short under load (many probes
    // running at once), flaking this whole check.
    let pickPop = null;
    for (let n = 1; n <= 10; n++) {
      document.getElementById('ve-add-video').click();
      await until(() => {
        pickPop = document.getElementById('ve-add-video-pop');
        return pickPop && !pickPop.classList.contains('hidden') && pickPop.querySelectorAll('button').length === 2;
      }, 100);
      if (!pickPop || pickPop.classList.contains('hidden') || pickPop.querySelectorAll('button').length !== 2) {
        errs.push('475: extra source number ' + n + ' was blocked below the limit of ten');
        break;
      }
      pickPop.querySelectorAll('button')[1].click(); // as its own section
      let posButtons = [];
      await until(() => {
        posButtons = pickPop.querySelectorAll('button');
        return !pickPop.classList.contains('hidden') && posButtons.length === 3;
      }, 100);
      if (pickPop.classList.contains('hidden') || posButtons.length !== 3) {
        errs.push('475: choosing as its own section did not open the position choice for source ' + n);
        break;
      }
      posButtons[1].click(); // at the end
      await until(() => document.querySelectorAll('#ve-clip-list .ve-op-row').length === n, 100);
      if (document.querySelectorAll('#ve-clip-list .ve-op-row').length !== n) {
        errs.push('475: extra source number ' + n + ' was not inserted');
        break;
      }
    }
    const afterFirst = document.querySelectorAll('#ve-clip-list .ve-op-row').length;
    if (afterFirst !== 10) {
      errs.push('475: ten extra sources were not inserted, found ' + afterFirst + ' clip row(s)');
    }

    document.getElementById('ve-add-video').click();
    const toastEl = document.getElementById('ve-toast');
    await until(() => !toastEl.classList.contains('hidden'), 100);
    pickPop = document.getElementById('ve-add-video-pop');
    const popOpened = pickPop && !pickPop.classList.contains('hidden') && pickPop.querySelectorAll('button').length;
    if (popOpened) {
      errs.push('475: an eleventh extra source was still allowed once the limit of ten was reached');
    }
    if (toastEl.classList.contains('hidden') || !toastEl.textContent || !toastEl.textContent.includes('10')) {
      errs.push('475: the limit toast did not report the limit of ten, got: ' + toastEl.textContent);
    }
    if (/^ve[A-Z][A-Za-z]+$/.test((toastEl.textContent || '').trim())) {
      errs.push('475: the limit toast rendered a raw i18n key: ' + toastEl.textContent);
    }
    const afterSecond = document.querySelectorAll('#ve-clip-list .ve-op-row').length;
    if (afterSecond !== 10) {
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
    inputEl.value = 'Say hi to {?topic}.';
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


UPDATE_INTERVAL_PROBE = """
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

    // 646: the interval beside the Auto update switch is stored on its own, the switch
    // alone turns checks off, and nothing here asks the backend to check for an update.
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
        if ([...sel.options].some((o) => o.value === '0')) errs.push('646: the interval list still offers Never');
        sel.value = '12';
        sel.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(150);
        if (!sw.checked) errs.push('646: choosing an interval switched Auto update off');
        if (!argsOf('set_ui_value').some((c) => c[1].key === 'updateCheckH' && c[1].value === 12)) errs.push('646: the interval was not stored');
        sw.checked = false;
        sw.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(100);
        if (!argsOf('set_auto_update').some((c) => c[1].enabled === false)) errs.push('646: switching Auto update off did not store it');
        sw.checked = true;
        sw.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(100);
        if (sel.value !== '12') errs.push('646: switching Auto update on should show the stored interval, got ' + sel.value);
        if (checks() !== checksBefore) errs.push('646: changing the update setting made a check_update call');
      }
    }
  } catch (e) { errs.push('646 probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


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


def run(browser, tmp):
    failed = 0
    # Undo time Off: a deleted view and a merged tag ask first. Own boot, the
    # store starts with undoToastMs at 0.
    side = tmp / "ui" / "undo-off.html"
    side.write_text(
        (REPO / "ui" / "index.html").read_text(encoding="utf-8")
        .replace("<script",
                 '<script>window.__UNDO_OFF_TEST__ = true;</script>\n'
                 '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", UNDO_OFF_PROBE + "</body>"),
        encoding="utf-8")
    proc = run_chrome(browser, tmp / 'profile-undo-off', "1000,780", 45000, side.as_uri(), 180)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
    print("-- Undo off: view delete and tag merge ask first")
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
    proc = run_chrome(browser, tmp / 'profile-drag-view', "1000,780", 45000, side.as_uri(), 180)
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
        proc = run_chrome(browser, tmp / ('profile-seal-%s' % tag), "1000,780", 45000, side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- store sealing ({tag} store)")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

    # 390/455: the first-run tour, both ways out (Finish and Skip write nothing but tourSeen).
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
        proc = run_chrome(browser, tmp / ('profile-tour-firstrun-%s' % mode), "1000,780", 45000, side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- first-run tour closes without writing settings ({mode})")
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
                 '<script>window.__SECRET_TEST__ = true; window.__SECRET_PRECONFIGURED__ = true;'
                 ' window.__SECRET_EXPIRES_MS__ = 3000;</script>\n'
                 '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", SECRET_PROBE + "</body>"),
        encoding="utf-8")
    proc = run_chrome(browser, tmp / 'profile-secret', "1000,780", 45000, side.as_uri(), 180)
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
        proc = run_chrome(browser, tmp / ('profile-writefail-%s' % tag), "1000,780", 45000, side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- settings write ({tag})")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

    # 838: the first-launch update question, one boot per answer (No, Yes, Escape).
    for mode in ("no", "yes", "escape"):
        side = tmp / "ui" / f"update-consent-{mode}.html"
        side.write_text(
            (REPO / "ui" / "index.html").read_text(encoding="utf-8")
            .replace("<script",
                     f'<script>window.__UPDATE_CONSENT_TEST__ = true; window.__UPDATE_CONSENT_MODE__ = "{mode}";</script>\n'
                     '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", UPDATE_CONSENT_PROBE + "</body>"),
            encoding="utf-8")
        proc = run_chrome(browser, tmp / ('profile-update-consent-%s' % mode), "1000,780", 45000, side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- update consent question ({mode})")
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
        proc = run_chrome(browser, tmp / ('profile-ddlock-%s' % tag), "1000,780", 45000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-folder-wording', "1000,780", 45000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-pill-wording', "520,420", 15000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-tile-hotkey-badge', "1000,780", 45000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-demo-mode', "1000,780", 45000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-cap-needs-admin', "1000,780", 45000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-whats-new', "1000,780", 45000, side.as_uri(), 180)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
    print("-- whats new: toast, window, About entry, GitHub button")
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
    proc = run_chrome(browser, tmp / 'profile-licenses', "1000,780", 45000, side.as_uri(), 180)
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
        proc = run_chrome(browser, tmp / ('profile-parked-%s' % name), f"{w},{h}", 15000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-float-recall', "1000,780", 45000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-extra-clips-max', "1130,860", 30000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-retention-panel', "1000,780", 45000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-editor-draft', "1000,780", 45000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-var-fixed', "1000,780", 45000, side.as_uri(), 180)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
    print("-- fixed placeholder values: list editor and fill-in suggestion")
    for e in errs:
        print(f"   FAIL: {e}")
    failed += len(errs)
    if not errs:
        print("   ok")

    # 646: the Auto update switch and its interval are one setting; Never is
    # the switch off and nothing here asks the backend to check for an update.
    side = tmp / "ui" / "update-interval.html"
    side.write_text(
        (REPO / "ui" / "index.html").read_text(encoding="utf-8")
        .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", UPDATE_INTERVAL_PROBE + "</body>"),
        encoding="utf-8")
    proc = run_chrome(browser, tmp / 'profile-update-interval', "1000,780", 45000, side.as_uri(), 180)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
    print("-- auto update switch and interval")
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
    proc = run_chrome(browser, tmp / 'profile-folder-dragout', "520,420", 15000, side.as_uri(), 180)
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
    proc = run_chrome(browser, tmp / 'profile-float-scale-race', "240,60", 15000, side.as_uri(), 180)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
    print("-- pill window: closing the menu during a size change")
    for e in errs:
        print(f"   FAIL: {e}")
    failed += len(errs)
    if not errs:
        print("   ok")
    return failed
