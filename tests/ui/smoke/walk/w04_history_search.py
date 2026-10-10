SECTION = """\
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
    // 59: a delete icon on each "recently copied" row removes that one entry -
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
      const srcPick = (k) => { srcBar.value = k; srcBar.dispatchEvent(new Event('change')); };
      const clipRows = () => [...document.querySelectorAll('#journal-body .clip-row')];
      if (!srcBar || srcBar.classList.contains('hidden')) {
        out.errors.push('434: the source select is hidden although copies from other apps are stored');
      } else {
        // 900-label: the source select carries a visible label to its left; off, both are hidden.
        {
          const core277 = await import('./core.js');
          const jr277 = await import('./journal.js');
          const field277 = document.getElementById('journal-source-field');
          const lbl277 = field277 && field277.querySelector('.cols-label');
          if (!field277 || field277.classList.contains('hidden') || !lbl277) {
            out.errors.push('900-label: the source field with its label is missing or hidden while sources are on');
          } else {
            if (lbl277.textContent.trim() !== core277.t('filterSourceLabel')) out.errors.push('900-label: the source label reads ' + JSON.stringify(lbl277.textContent) + ' instead of ' + JSON.stringify(core277.t('filterSourceLabel')));
            const lr277 = lbl277.getBoundingClientRect();
            const sr277 = srcBar.getBoundingClientRect();
            if (!(lr277.right <= sr277.left + 1) || Math.abs((lr277.top + lr277.bottom) / 2 - (sr277.top + sr277.bottom) / 2) > 6) {
              out.errors.push('900-label: the source label does not sit left of the select on one line');
            }
          }
          jr277.syncJournalSourceBar(false);
          if (!srcBar.classList.contains('hidden') || !field277.classList.contains('hidden')) out.errors.push('900-label: with sources off the select or its label stays visible');
          jr277.syncJournalSourceBar(true);
          if (srcBar.classList.contains('hidden') || field277.classList.contains('hidden')) out.errors.push('900-label: with sources back on the select or its label stays hidden');
        }
        if (document.querySelectorAll('#journal-body .journal-item').length === 0 || clipRows().length !== 2) {
          out.errors.push('434: the unfiltered history does not mix app copies and the two stored entries (' +
            document.querySelectorAll('#journal-body .journal-item').length + ' app rows, ' + clipRows().length + ' elsewhere rows)');
        }
        srcPick('elsewhere');
        await sleep(150);
        if (clipRows().length !== 2) out.errors.push('434: the Elsewhere filter lists ' + clipRows().length + ' rows instead of 2');
        if (document.querySelector('#journal-body .journal-item')) out.errors.push('434: the Elsewhere filter still lists copies made in the app');
        // 280: the save control of an elsewhere entry says "As prompt"; the dismiss stays the last icon.
        {
          const core280 = await import('./core.js');
          const first280 = document.querySelector('#journal-body .clip-row .clip-actions > :first-child');
          if (!first280) out.errors.push('280: an elsewhere row has no save control');
          else {
            if (first280.textContent.trim() !== core280.t('clipSaveShort')) out.errors.push('280: the save control shows ' + JSON.stringify(first280.textContent.trim()) + ' instead of ' + JSON.stringify(core280.t('clipSaveShort')));
            if (first280.title !== core280.t('clipSave') && first280.getAttribute('aria-label') !== core280.t('clipSave')) out.errors.push('280: the save control lost its full title');
            const last280 = document.querySelector('#journal-body .clip-row .clip-actions .icon-btn:last-child');
            if (!last280 || last280 === first280) out.errors.push('280: the dismiss is no longer the last icon');
          }
        }
        // 723: an entry copied elsewhere has two icons and no text buttons; the row copies.
        {
          const r0 = clipRows()[0];
          if (!r0) {
            out.errors.push('723: no elsewhere row to inspect');
          } else {
            if (r0.querySelectorAll('.ghost-btn').length !== 0 || r0.querySelectorAll('.icon-btn').length !== 2) {
              out.errors.push('723: an elsewhere row has ' + r0.querySelectorAll('.ghost-btn').length + ' text buttons and ' + r0.querySelectorAll('.icon-btn').length + ' icons instead of 0 and 2');
            }
            if (r0.tabIndex !== 0) out.errors.push('723: an elsewhere row cannot be reached with the keyboard');
            for (const ic of r0.querySelectorAll('.icon-btn')) {
              if (!ic.title || !ic.getAttribute('aria-label')) out.errors.push('723: an elsewhere icon has no title or accessible name');
            }
            window.__SMOKE_ARGS__ = [];
            r0.click();
            await sleep(150);
            if (!window.__SMOKE_ARGS__.some(([cmd]) => cmd === 'copy_text')) out.errors.push('723: clicking an elsewhere row did not copy it');
          }
        }
        // Dismiss removes one entry.
        window.__SMOKE_ARGS__ = [];
        const dismissBtn = clipRows()[0]?.querySelector('.clip-actions .icon-btn:last-child');
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
        srcPick('app');
        await sleep(150);
        if (clipRows().length) out.errors.push('434: the app filter still lists entries copied elsewhere');
        srcPick('all');
        await sleep(150);
      }
      window.__resetClipInbox();
      await window.__TAURI__.core.invoke('clip_inbox_list');
    }
    // 198: the journal header carries a count in the head.
    {
      const jCount = document.getElementById('journal-count');
      if (!jCount || !jCount.textContent.trim()) {
        out.errors.push('198: the journal header is missing its entry count');
      }
    }
    // 274: the copy history has no prompt filters: no favourite chip, colour dots or tag row.
    {
      for (const id of ['journal-fav', 'journal-colors', 'journal-tags']) {
        if (document.getElementById(id)) out.errors.push('274: #' + id + ' still exists in the copy history');
      }
      if (document.querySelector('#journal-types .lib-fav-chip')) out.errors.push('274: the type filter still carries a favourite chip');
    }
    // row275: every history entry is one line; the preview and the full time ride in the tooltip.
    {
      const rows275 = [...document.querySelectorAll('#journal-body .journal-item, #journal-body .journal-clip')];
      if (!rows275.length) out.errors.push('row275: the history lists no entry');
      else {
        const hs275 = rows275.map((r) => r.getBoundingClientRect().height);
        if (Math.max(...hs275) > 48.5) out.errors.push('row275: a history row is ' + Math.max(...hs275) + ' px tall, at most 48 expected');
        if (Math.max(...hs275) - Math.min(...hs275) > 2) out.errors.push('row275: the history rows differ in height (' + Math.min(...hs275) + ' to ' + Math.max(...hs275) + ')');
        const prow275 = rows275.find((r) => r.classList.contains('journal-item'));
        if (!prow275) out.errors.push('row275: no prompt row in the history');
        else {
          if (!prow275.dataset.tipName) out.errors.push('row275: a prompt row has no tooltip name');
          if (prow275.querySelector('.lib-text')) out.errors.push('row275: a prompt row still shows a preview line');
          if (prow275.querySelector('.lib-tags')) out.errors.push('row275: a prompt row still shows tag chips');
          const when275 = prow275.querySelector('.journal-when');
          if (!when275) out.errors.push('row275: a prompt row has no time on the right');
          else if (prow275.getBoundingClientRect().right - when275.getBoundingClientRect().right > 90) {
            out.errors.push('row275: the time is not at the right edge (' + (prow275.getBoundingClientRect().right - when275.getBoundingClientRect().right) + ' px from it)');
          }
          const core275 = await import('./core.js');
          const vals275 = core275.settings.ui_values;
          core275.settings.ui_values = { ...(vals275 || {}), tooltipDelayMs: 0 };
          prow275.dispatchEvent(new PointerEvent('pointerover', { bubbles: true, clientX: 200, clientY: 200 }));
          await sleep(100);
          const tip275 = document.querySelector('.tile-tip.show');
          if (!tip275) out.errors.push('row275: hovering a history row showed no tooltip');
          else {
            const nm275 = tip275.querySelector('.tile-tip-name').textContent;
            const hint275 = tip275.querySelector('.tile-tip-hint').textContent;
            if (nm275 !== prow275.dataset.tipName) out.errors.push('row275: the tooltip names ' + JSON.stringify(nm275) + ' instead of the row');
            if (hint275 !== core275.t('journalRowHint')) out.errors.push('row275: the tooltip hint is ' + JSON.stringify(hint275));
          }
          prow275.dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body }));
          core275.settings.ui_values = vals275;
          await sleep(50);
        }
      }
    }
    // hd276: "Grouped" is a checkbox that follows the flag, and the order button names the order.
    {
      const core276 = await import('./core.js');
      const cb276 = document.getElementById('journal-grouped');
      const sortBtn276 = () => document.querySelector('#journal-body .journal-sort');
      const sortSel276 = document.getElementById('journal-sort');
      if (!cb276 || cb276.type !== 'checkbox' || !cb276.closest('label.head-check')) {
        out.errors.push('hd276: Grouped is not a checkbox inside a head-check label');
      } else {
        const flagOn276 = () => (core276.settings.ui_flags || {}).historyGrouped !== false;
        if (cb276.checked !== flagOn276()) out.errors.push('hd276: the Grouped box does not show the flag');
        const before276 = flagOn276();
        window.__SMOKE_ARGS__ = [];
        cb276.click();
        await sleep(300);
        if (!window.__SMOKE_ARGS__.some(([cmd, a]) => cmd === 'set_ui_flag' && a.key === 'historyGrouped' && a.enabled === !before276)) {
          out.errors.push('hd276: ticking Grouped did not store historyGrouped');
        }
        const cbAfter276 = document.getElementById('journal-grouped');
        if (!cbAfter276 || cbAfter276.checked !== !before276) out.errors.push('hd276: the Grouped box did not follow the click');
        if (cbAfter276) { cbAfter276.click(); await sleep(300); }
        if (flagOn276() !== before276) out.errors.push('hd276: Grouped did not come back to its first state');
      }
      if (!sortBtn276() || !sortBtn276().textContent.includes(core276.t('journalNewestFirst'))) {
        out.errors.push('hd276: the order button does not say ' + core276.t('journalNewestFirst') + ': ' + (sortBtn276() ? sortBtn276().textContent : 'missing'));
      } else {
        sortBtn276().click();
        await sleep(200);
        if (!sortBtn276() || !sortBtn276().textContent.includes(core276.t('journalOldestFirst'))) out.errors.push('hd276: after a click the order button does not say ' + core276.t('journalOldestFirst'));
        else { sortBtn276().click(); await sleep(200); }
        const headKids276 = [...document.querySelectorAll('#journal-body .journal-head > *, #journal-body .journal-head-extras > *')].map((e) => e.getBoundingClientRect().top);
        if (Math.max(...headKids276) - Math.min(...headKids276) > 4) out.errors.push('hd276: the history head is not one line (tops ' + headKids276.map(Math.round).join(', ') + ')');
      }
      if (sortSel276) {
        sortSel276.value = 'used';
        sortSel276.dispatchEvent(new Event('change'));
        await sleep(200);
        if (!sortBtn276() || !sortBtn276().textContent.includes(core276.t('journalMostFirst'))) out.errors.push('hd276: in most-used mode the order button does not say ' + core276.t('journalMostFirst'));
        const sel2 = document.getElementById('journal-sort');
        sel2.value = 'recent';
        sel2.dispatchEvent(new Event('change'));
        await sleep(200);
      }
    }
    // 279: a video whose thumbnail shows its duration needs no VIDEO chip; other types keep theirs.
    {
      const jr279 = await import('./journal.js');
      const videoRow279 = [...document.querySelectorAll('#journal-body .journal-item')].find((r) => r.querySelector('.lib-thumb-duration'));
      if (!videoRow279) out.errors.push('279: no history row with a duration badge to inspect');
      else if (videoRow279.querySelector('.journal-kind')) out.errors.push('279: a video row with a duration badge still shows the VIDEO chip');
      const imgRow279 = jr279.journalRow({ id: 'img279', name: 'Pic', file_path: 'C:/x/pic.png' }, null, () => {});
      if (!imgRow279.querySelector('.journal-kind')) out.errors.push('279: an image row lost its type chip');
      const bareVideo279 = jr279.journalRow({ id: 'vid279', name: 'Clip', file_path: 'C:/x/clip.mp4' }, null, () => {});
      if (!bareVideo279.querySelector('.journal-kind')) out.errors.push('279: a video without a duration lost its type chip');
    }
    // 946: the time range narrows "recently copied"; it steps aside in most-used mode and resets on reopen.
    {
      const core946 = await import('./core.js');
      const jr946 = await import('./journal.js');
      const range946 = document.getElementById('journal-range');
      if (!range946) out.errors.push('946: the history has no time range field');
      else {
        const nowS946 = Math.floor(Date.now() / 1000);
        window.__JOURNAL_LOG__ = [{ id: 'p0', ts: nowS946, rowid: 901 }, { id: 'p1', ts: nowS946 - 10 * 86400, rowid: 902 }];
        await jr946.loadJournal();
        await sleep(200);
        const rows946 = () => [...document.querySelectorAll('#journal-body .journal-item, #journal-body .journal-clip')];
        const names946 = () => rows946().map((r) => (r.querySelector('.lib-name') || {}).textContent || '');
        const pick946 = async (v) => { range946.value = v; range946.dispatchEvent(new Event('change')); await sleep(200); };
        const startRows946 = rows946().length;
        const oldName946 = (core946.prompts.find((x) => x.id === 'p1') || {}).name;
        if (!names946().includes(oldName946)) out.errors.push('946: the ten day old copy is missing before a range is chosen');
        await pick946('7');
        if (rows946().length !== 1) out.errors.push('946: a 7 day range lists ' + rows946().length + ' rows instead of 1 (from ' + startRows946 + ')');
        const count946 = document.getElementById('journal-count').textContent;
        const nums946 = count946.replace(/[^0-9]+/g, ' ').trim().split(' ');
        const want946 = core946.t('libraryCount').replace('{n}', String(rows946().length)).replace('{v}', nums946[nums946.length - 1]);
        if (nums946.length !== 2 || count946 !== want946) out.errors.push('946: the count does not read as n of m with a range: ' + JSON.stringify(count946));
        await pick946('today');
        if (names946().includes(oldName946)) out.errors.push('946: the ten day old copy still shows under Today');
        const sortSel946 = document.getElementById('journal-sort');
        sortSel946.value = 'used';
        sortSel946.dispatchEvent(new Event('change'));
        await sleep(200);
        if (!range946.classList.contains('hidden')) out.errors.push('946: the range field stays visible in most-used mode');
        const back946 = document.getElementById('journal-sort');
        back946.value = 'recent';
        back946.dispatchEvent(new Event('change'));
        await sleep(200);
        if (range946.classList.contains('hidden')) out.errors.push('946: the range field stays hidden in recently-copied mode');
        window.__JOURNAL_LOG__ = null;
        await click('library-close');
        await click('library-btn');
        await click('library-tab-history');
        await sleep(300);
        if (range946.value !== 'all') out.errors.push('946: the range did not reset on reopen: ' + range946.value);
      }
    }
    // 716: the copy history is one column; the sort field switches to "most used", the
    // pick is stored and survives closing and reopening the library.
    {
      const body = document.getElementById('journal-body');
      const sortSel = document.getElementById('journal-sort');
      if (!sortSel) {
        out.errors.push('716: the history has no sort field');
      } else {
        if (getComputedStyle(body).gridTemplateColumns.trim().split(' ').length !== 1) out.errors.push('716: the history body is not one column');
        if (body.querySelectorAll('.journal-col').length !== 1) out.errors.push('716: the history shows more than one list');
        window.__SMOKE_ARGS__ = [];
        sortSel.value = 'used';
        sortSel.dispatchEvent(new Event('change'));
        await sleep(200);
        if (!window.__SMOKE_ARGS__.some(([cmd, a]) => cmd === 'set_ui_text' && a.key === 'journalSort' && a.value === 'used')) out.errors.push('716: the sort pick was not stored');
        const usedCounts = () => [...document.querySelectorAll('#journal-body .journal-item')]
          .map((r) => parseInt((r.querySelector('.lib-sub-right')?.textContent || '').replace(/[^0-9]/g, ''), 10));
        let counts = usedCounts();
        if (!counts.length || counts.some((n) => Number.isNaN(n))) out.errors.push('716: most used lists no rows with a count');
        for (let i = 1; i < counts.length; i++) if (counts[i] > counts[i - 1]) out.errors.push('716: most used is not ordered by count');
        if (document.querySelector('#journal-body .journal-col-recent')) out.errors.push('716: most used still carries the recent column class');
        await click('library-close');
        document.getElementById('library-btn').click();
        await sleep(300);
        if (document.getElementById('library-panel-history').classList.contains('hidden')) await click('library-tab-history');
        await sleep(200);
        if (document.getElementById('journal-sort')?.value !== 'used') out.errors.push('716: the sort pick did not survive reopening the library');
        counts = usedCounts();
        if (!counts.length) out.errors.push('716: most used is empty after reopening');
        const back = document.getElementById('journal-sort');
        if (back) { back.value = 'recent'; back.dispatchEvent(new Event('change')); await sleep(200); }
        if (!document.querySelector('#journal-body .journal-col-recent')) out.errors.push('716: switching back to recent lost the recent column');
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
"""
