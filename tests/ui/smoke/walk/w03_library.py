SECTION = """\
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
    // P18: a "-word" term excludes matches instead of narrowing to them -
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
          // 434: undo the batch tag-add - the toast's Undo button inverts it
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
          // 434: undo the batch tag-remove - inverted back to tag-add, then
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
      // 434: batch delete offers an Undo button too - restore_prompt per id
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
          delBatchBtn.click(); await sleep(250);
          if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
            out.errors.push('677: the library batch delete opened a dialog');
          }
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
      // 677: the library row's trash button acts on the first click, shows Undo
      // and no dialog; Undo sends the placement snapshot back.
      {
        const rowDel = document.querySelector('.lib-item[data-id="p8"] .lib-delete');
        if (!rowDel) {
          out.errors.push('677: the library row of p8 has no trash button');
        } else {
          window.__SMOKE_ARGS__ = [];
          rowDel.click();
          await sleep(300);
          if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
            out.errors.push('677: the library row trash button opened a dialog');
          }
          if (!window.__SMOKE_ARGS__.some(([cmd, args]) => cmd === 'delete_prompt' && args && args.id === 'p8')) {
            out.errors.push('677: the library row trash button did not delete on the first click');
          }
          const rowUndo = document.getElementById('toast').querySelector('.toast-btn');
          if (!rowUndo) {
            out.errors.push('677: the library row delete offered no undo toast');
          } else {
            window.__SMOKE_ARGS__ = [];
            rowUndo.click();
            await sleep(300);
            if (!window.__SMOKE_ARGS__.some(([cmd, args]) => cmd === 'restore_prompts_batch' && args && args.placements && args.placements.some((pl) => pl.id === 'p8'))) {
              out.errors.push('677: undoing the library row delete did not restore p8 with its placement');
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
"""
