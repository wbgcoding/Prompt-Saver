SECTION = """\
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

    // 11: grid bulk selection - checkbox click must never fall through to the
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
        // (.swatch, not .lib-color) - skip "none" and the free-pick swatch.
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
        // 21: the delete acts on the first press: no arming, no dialog, and the
        // toast carries an Undo button.
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
        // Re-select for the delete test below - the custom-color commit
        // above clears the selection same as every other batch action.
        gridTile('p4').querySelector('.tile-select').click();
        await sleep(120);
        const delBtn = document.getElementById('grid-batch-delete');
        window.__SMOKE_ARGS__ = [];
        await click('grid-batch-delete');
        await sleep(200);
        if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
          out.errors.push('the batch delete still opens a confirm dialog');
        }
        if (delBtn.classList.contains('confirm')) out.errors.push('the batch delete button armed instead of deleting');
        if (gridTile('p4')) out.errors.push('one press did not delete the tile');
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
      }
    }
    // 677: a tile's context menu and the editor's Delete button also act on the
    // first click and offer Undo, with no dialog.
    {
      const delTile = [...document.querySelectorAll('#grid .tile')].find((e) => e.dataset.id && e.querySelector(':scope > .tile-select'));
      const delId = delTile ? delTile.dataset.id : '';
      const confirmBox = document.getElementById('confirm-modal');
      const toastBtn = () => document.getElementById('toast').querySelector('.toast-btn');
      if (!delTile) {
        out.errors.push('677: no prompt tile is on the grid for the context menu delete check');
      } else {
        delTile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 60, clientY: 60 }));
        await sleep(150);
        const ctxDel = document.querySelector('#ctx [data-act="delete"]');
        if (!ctxDel) {
          out.errors.push('677: the tile context menu has no Delete entry');
        } else {
          window.__SMOKE_ARGS__ = [];
          ctxDel.click();
          await sleep(300);
          if (!confirmBox.classList.contains('hidden')) out.errors.push('677: the context menu delete opened a dialog');
          if (!window.__SMOKE_ARGS__.some(([cmd, args]) => cmd === 'delete_prompt' && args && args.id === delId)) {
            out.errors.push('677: the context menu delete did not delete on the first click');
          }
          const ctxUndo = toastBtn();
          if (!ctxUndo) {
            out.errors.push('677: the context menu delete offered no undo toast');
          } else {
            window.__SMOKE_ARGS__ = [];
            ctxUndo.click();
            await sleep(300);
            if (!window.__SMOKE_ARGS__.some(([cmd]) => cmd === 'restore_prompts_batch')) {
              out.errors.push('677: undoing the context menu delete sent no restore');
            }
          }
        }
        const delTile2 = [...document.querySelectorAll('#grid .tile')].find((e) => e.dataset.id === delId);
        if (delTile2) {
          delTile2.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 60, clientY: 60 }));
          await sleep(150);
          const ctxEdit = document.querySelector('#ctx [data-act="edit"]');
          if (ctxEdit) ctxEdit.click();
          await sleep(300);
          const editorDel = document.getElementById('modal-delete');
          if (!editorDel || editorDel.classList.contains('hidden')) {
            out.errors.push('677: the editor has no Delete button for an existing prompt');
          } else {
            window.__SMOKE_ARGS__ = [];
            editorDel.click();
            await sleep(300);
            if (!confirmBox.classList.contains('hidden')) out.errors.push('677: the editor delete opened a dialog');
            if (!window.__SMOKE_ARGS__.some(([cmd, args]) => cmd === 'delete_prompt' && args && args.id === delId)) {
              out.errors.push('677: the editor delete did not delete on the first click');
            }
            if (!document.getElementById('modal').classList.contains('hidden')) {
              out.errors.push('677: the editor stayed open after deleting');
            }
            const edUndo = toastBtn();
            if (!edUndo) out.errors.push('677: the editor delete offered no undo toast');
            else { edUndo.click(); await sleep(300); }
          }
        }
      }
    }
    // 698: the tile menu offers Copy, Favorite, Select and Start chain, groups its
    // entries behind separators, gives each an icon and ends on a red Delete.
    {
      const ctxTile = [...document.querySelectorAll('#grid .tile')].find((e) => e.dataset.id === 'p0');
      const ctxVisible = (b) => !!b && !b.classList.contains('hidden') && getComputedStyle(b).display !== 'none';
      const openTileMenu = async () => {
        ctxTile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 60, clientY: 60 }));
        await sleep(150);
      };
      if (!ctxTile) {
        out.errors.push('698: no tile p0 on the grid for the tile menu check');
      } else {
        await openTileMenu();
        for (const act of ['copy', 'favorite', 'select', 'chain']) {
          if (!ctxVisible(document.querySelector('#ctx [data-act="' + act + '"]'))) {
            out.errors.push('698: the tile menu shows no ' + act + ' entry');
          }
        }
        // p0 carries a hotkey and is a favorite: the hotkey shows beside Copy, the star entry offers to remove it.
        const kbdText = (document.querySelector('#ctx [data-act="copy"] .ctx-kbd') || {}).textContent || '';
        if (!kbdText.includes('Shift') || !kbdText.includes('9')) out.errors.push('698: Copy shows no hotkey, got: ' + kbdText);
        const favText = (document.querySelector('#ctx [data-act="favorite"] .ctx-label') || {}).textContent || '';
        if (favText !== 'Remove from favorites') out.errors.push('698: the favorite entry reads: ' + favText);
        const seps = [...document.querySelectorAll('#ctx .ctx-sep')].filter((s) => getComputedStyle(s).display !== 'none');
        if (seps.length < 3) out.errors.push('698: the tile menu has ' + seps.length + ' visible separators, expected at least 3');
        const shown = [...document.querySelectorAll('#ctx > button')].filter(ctxVisible);
        const lastBtn = shown[shown.length - 1];
        if (!lastBtn || lastBtn.dataset.act !== 'delete') {
          out.errors.push('698: the last tile menu entry is ' + (lastBtn ? lastBtn.dataset.act : 'none') + ', expected delete');
        } else {
          const ref = document.createElement('span');
          ref.style.color = 'var(--danger)';
          document.body.appendChild(ref);
          const want = getComputedStyle(ref).color;
          ref.remove();
          if (getComputedStyle(lastBtn).color !== want) out.errors.push('698: Delete is not red at rest: ' + getComputedStyle(lastBtn).color + ' vs ' + want);
        }
        for (const b of shown) {
          if (b.dataset.act === 'rotateAutoMenu') continue;
          if (!b.querySelector('.menu-icon svg')) out.errors.push('698: the tile menu entry ' + b.dataset.act + ' has no icon');
          if (!b.querySelector('.ctx-label')) out.errors.push('698: the tile menu entry ' + b.dataset.act + ' has no label span');
        }
        window.__SMOKE_ARGS__ = [];
        document.querySelector('#ctx [data-act="copy"]').click();
        await sleep(300);
        const copies = window.__SMOKE_ARGS__.filter(([cmd]) => cmd === 'copy_prompt').length;
        if (copies !== 1) out.errors.push('698: the Copy entry called copy_prompt ' + copies + ' times, expected 1');
        if (!document.getElementById('ctx').classList.contains('hidden')) out.errors.push('698: the menu stayed open after Copy');
        for (const want of [false, true]) {
          await openTileMenu();
          window.__SMOKE_ARGS__ = [];
          document.querySelector('#ctx [data-act="favorite"]').click();
          await sleep(250);
          const favCalls = window.__SMOKE_ARGS__.filter(([cmd, args]) => cmd === 'set_favorite' && args && args.id === 'p0' && args.favorite === want);
          if (favCalls.length !== 1) out.errors.push('698: the favorite entry did not send set_favorite ' + want);
        }
        await openTileMenu();
        document.querySelector('#ctx [data-act="select"]').click();
        await sleep(200);
        if (!document.body.classList.contains('grid-selecting')) out.errors.push('698: the Select entry did not start selecting');
        await openTileMenu();
        document.querySelector('#ctx [data-act="select"]').click();
        await sleep(200);
        if (document.body.classList.contains('grid-selecting')) out.errors.push('698: the Select entry did not unselect the selected tile');
        // Start chain begins a chain with this tile; leave it again right away.
        await openTileMenu();
        document.querySelector('#ctx [data-act="chain"]').click();
        await sleep(300);
        if (!document.body.classList.contains('chain-mode')) out.errors.push('698: the Start chain entry did not start a chain');
        for (let k = 0; k < 3 && document.body.classList.contains('chain-mode'); k++) {
          document.getElementById('chain-btn').click();
          await sleep(200);
        }
        if (document.body.classList.contains('chain-mode')) out.errors.push('698: the chain did not end through the chain button');
      }
    }
    // 699: a hovered tile shows only its menu button; the checkbox and the chain button are
    // gone from the hover. Ctrl+click without moving selects, it never copies.
    {
      const hoverRules = [];
      const walkRules = (rules) => {
        for (const r of rules) {
          const sel = r.selectorText || '';
          if (sel.includes('tile-chain-start') || sel.includes('.tile:hover .tile-select') || sel.includes('.tile:hover .tile-type')) hoverRules.push(sel);
          if (r.cssRules) walkRules(r.cssRules);
        }
      };
      for (const sh of document.styleSheets) { try { walkRules(sh.cssRules); } catch (e) {} }
      if (hoverRules.length) out.errors.push('699: stylesheet still has hover or chain-start rules: ' + hoverRules.join(' | '));
      const selTile = gridTile('p0');
      const selBox = selTile && selTile.querySelector(':scope > .tile-select');
      if (!selBox) {
        out.errors.push('699: tile p0 has no select box');
      } else {
        selBox.style.transition = 'none'; // read the settled value, not a fade in progress
        const cs = getComputedStyle(selBox);
        if (cs.opacity !== '0' || cs.pointerEvents !== 'none') out.errors.push('699: the idle select box is visible or catches clicks: ' + cs.opacity + ' ' + cs.pointerEvents);
        window.__SMOKE_ARGS__ = [];
        press(selTile, { ctrlKey: true });
        window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, ctrlKey: true, clientX: 20, clientY: 20 }));
        await sleep(300);
        if (!document.body.classList.contains('grid-selecting')) out.errors.push('699: Ctrl+click did not select the tile');
        if (!selTile.classList.contains('grid-selected')) out.errors.push('699: Ctrl+click did not mark the tile selected');
        if (window.__SMOKE_ARGS__.some(([cmd]) => cmd === 'copy_prompt')) out.errors.push('699: Ctrl+click copied the prompt');
        const cs2 = getComputedStyle(selBox);
        if (cs2.opacity !== '1' || cs2.pointerEvents === 'none') out.errors.push('699: the select box is not shown while selecting: ' + cs2.opacity + ' ' + cs2.pointerEvents);
        press(selTile, { ctrlKey: true });
        window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, ctrlKey: true, clientX: 20, clientY: 20 }));
        await sleep(300);
        if (document.body.classList.contains('grid-selecting')) out.errors.push('699: a second Ctrl+click did not unselect the tile');
      }
    }
    // 91: trash - the dialog opened from the expert menu's Data group. Restore,
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
    // 51: prompt in multiple views - off by default. On exposes "Also show in…" on
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
                  workRowAfter.click(); // toggle back off - leaves the fixture clean
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
    // cache would keep offering "Show" for a prompt that is already pinned - and the
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
    // 24: grid tag filter - the header toolbar button opens a popover of tag chips;
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
            const t0 = gridTile('p0'); // ["Work"] only - must dim
            const t1 = gridTile('p1'); // ["Draft", "Work"] - must stay full
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
"""
