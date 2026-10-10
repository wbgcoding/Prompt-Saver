SECTION = """\
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
    // Collapsing must flip a class, not rebuild the list - rebuilding is what made
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
        if (kids[kids.length - 2].id !== 'settings-backup-btn' || kids[kids.length - 1].id !== 'settings-stats-btn') out.errors.push('settings: the backup manager and statistics buttons do not end the import/export row');
        const eb = document.getElementById('export-btn').getBoundingClientRect(), bb = document.getElementById('settings-backup-btn').getBoundingClientRect();
        if (!bb.width || !bb.height) out.errors.push('settings: the backup manager button has no size');
        else if (!(bb.left > eb.right - 1 || bb.top > eb.bottom - 1)) out.errors.push('settings: the backup manager button does not follow the export button');
      }
      // 726: the format choice sits right of the Export button, both right of Import folder.
      {
        const ifb = document.getElementById('import-folder-btn').getBoundingClientRect();
        const eb726 = document.getElementById('export-btn').getBoundingClientRect();
        const ef = document.getElementById('export-format');
        const efr = (ef.closest('.select-picker, .select-wrap') || ef).getBoundingClientRect();
        const efShown = efr.width ? efr : ef.getBoundingClientRect();
        if (!document.querySelector('#impexp-field .export-pair #export-btn')) out.errors.push('726: export-btn is not inside the export pair');
        if (!(efShown.left >= eb726.right - 1)) out.errors.push('726: the export format is not right of the Export button: ' + Math.round(efShown.left) + ' vs ' + Math.round(eb726.right));
        if (!(eb726.left >= ifb.right - 1 || eb726.top >= ifb.bottom - 1)) out.errors.push('726: the Export button is not after Import folder');
      }
      if (document.getElementById('export-all') || document.getElementById('import-all')) out.errors.push('settings: the old save/import backup buttons are still in the settings row');
      if (wasHidden) settingsEl.classList.add('hidden');
    }
    // 903: the usage statistics have their own dialog, opened from the settings. It
    // shows two figures, the most used prompts by name and the never used ones;
    // the backups dialog is not involved.
    {
      const sm = document.getElementById('stats-modal');
      const settingsOpen = !document.getElementById('settings').classList.contains('hidden');
      const waitFor = async (fn) => { for (let i = 0; i < 40 && !fn(); i++) await sleep(50); return fn(); };
      document.getElementById('settings-stats-btn').click();
      await sleep(250);
      if (sm.classList.contains('hidden')) {
        out.errors.push('903: the statistics dialog did not open from the settings');
      } else {
        layoutScan('statistics');
        mustReach('statistics', '#stats-modal .modal-head > button');
        if (!document.getElementById('backup-modal').classList.contains('hidden')) out.errors.push('903: the backups dialog is open next to the statistics');
        if (!document.getElementById('stats-panel').closest('#stats-modal')) out.errors.push('903: the statistics panel is not inside its own dialog');
        if (Number(getComputedStyle(sm).zIndex) <= Number(getComputedStyle(document.getElementById('settings')).zIndex)) {
          out.errors.push('903: the statistics dialog sits behind the settings it was opened from');
        }
        // 45: toggling the stats switch used to re-render into the inner panel from
        // the previous render instead of the real container, nesting one deeper each
        // time. Re-query fresh each click: a re-render replaces the whole subtree.
        for (let i = 0; i < 2; i++) {
          const sw = document.querySelector('#stats-panel .stats-head-row .switch');
          if (!sw) break;
          sw.click();
          await sleep(100);
        }
        if (!document.querySelector('#stats-panel .stats-head-row .switch')?.checked) out.errors.push('two stats-switch toggles did not end back on');
        const bodies = document.querySelectorAll('#stats-panel .stats-panel-body');
        if (bodies.length !== 1) out.errors.push('toggling stats twice left ' + bodies.length + ' stats-panel-body elements instead of 1');
        const cardEls = document.querySelectorAll('#stats-panel .stat-cards .stat-card');
        if (cardEls.length !== 2) out.errors.push('903: the statistics show ' + cardEls.length + ' cards instead of 2');
        const names = [...document.querySelectorAll('#stats-panel .stat-bar-name')].map((n) => n.textContent);
        if (names.join(',') !== 'Prompt 1,Prompt 2,Prompt 3') out.errors.push('903: the most used list does not show the prompt names: ' + names.join(','));
        if (document.querySelector('#stats-panel .stat-daily-chart, #stats-panel .stat-tool-views, #stats-panel .stat-overview-split')) {
          out.errors.push('903: the statistics still carry the chart, tool or overview blocks');
        }
        if (!document.querySelector('#stats-panel .stat-local-only')) out.errors.push('903: the statistics lose the stays-on-this-computer note');
        // 906: the list length is a choice of 5, 10 or 20.
        const topSel = document.getElementById('stats-topn');
        if (!topSel) {
          out.errors.push('906: the most used list has no length choice');
        } else {
          const opts = [...topSel.options].map((o) => o.value).join(',');
          if (opts !== '5,10,20') out.errors.push('906: the list length options are ' + opts + ' instead of 5,10,20');
          if (document.querySelector('#stats-panel .stats-topn-row')) out.errors.push('906: the old number field is still there');
          window.__SMOKE_ARGS__ = [];
          topSel.value = '20';
          topSel.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(150);
          const setCall = (window.__SMOKE_ARGS__ || []).find((c) => c[0] === 'set_ui_value' && c[1] && c[1].key === 'statsTopN');
          if (!setCall || setCall[1].value !== 20) out.errors.push('906: picking 20 did not store statsTopN 20');
          if (document.getElementById('stats-topn')?.value !== '20') out.errors.push('906: the list length choice did not keep 20');
        }
        // 407: the export button rides the same switch as the rest of the dashboard.
        if (!document.getElementById('stats-export-btn')) out.errors.push('407: the statistics export button is missing while statistics are on');
        const findToggle = () => document.querySelector('#stats-panel .stats-head-row .switch-field input');
        if (findToggle()) {
          findToggle().click();
          await waitFor(() => !document.getElementById('stats-export-btn'));
          if (document.getElementById('stats-export-btn')) out.errors.push('407: the statistics export button is still there with statistics off');
          if (document.querySelector('#stats-panel .stat-cards')) out.errors.push('903: the figures stay on screen with statistics off');
          const t2 = findToggle();
          if (t2) {
            t2.click(); // back on: statistics stays on for the rest of the walk
            await waitFor(() => !!document.getElementById('stats-export-btn'));
          }
          if (!document.getElementById('stats-export-btn')) out.errors.push('407: the statistics export button did not come back with statistics re-enabled');
        }
        // A never used prompt opens the library IN FRONT of the statistics dialog,
        // which stays open so it can be returned to.
        const cand = document.querySelector('#stats-panel .stat-cleanup-item');
        if (!cand) {
          out.errors.push('the statistics list no cleanup candidates');
        } else {
          cand.click();
          await sleep(300);
          const lib = document.getElementById('library');
          if (lib.classList.contains('hidden')) out.errors.push('a cleanup candidate does not open the library');
          if (sm.classList.contains('hidden')) out.errors.push('the statistics dialog was closed instead of stepped in front of');
          if (Number(getComputedStyle(lib).zIndex) <= Number(getComputedStyle(sm).zIndex)) {
            out.errors.push('the library opens behind the statistics dialog');
          }
          document.getElementById('library-close').click();
          await sleep(150);
          if (document.body.classList.contains('library-front')) out.errors.push('the raised-library state outlives the library');
          if (sm.classList.contains('hidden')) out.errors.push('there is no statistics dialog left to jump back to');
        }
        // Escape closes the statistics first, the settings underneath stay.
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await sleep(100);
        if (!sm.classList.contains('hidden')) out.errors.push('903: Escape did not close the statistics dialog');
        if (settingsOpen && document.getElementById('settings').classList.contains('hidden')) out.errors.push('903: Escape closed the settings along with the statistics');
      }
      if (!sm.classList.contains('hidden')) { document.getElementById('stats-close').click(); await sleep(100); }
    }
    document.getElementById('settings-backup-btn').click();
    await sleep(400);
    const bm = document.getElementById('backup-modal');
    document.querySelector('.backup-body').scrollTop = 0; // an earlier visit may have left it scrolled
    if (bm.classList.contains('hidden')) {
      out.errors.push('the backups dialog did not open');
    } else if (!document.getElementById('backup-panel').children.length) {
      out.errors.push('the backups dialog stayed empty');
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
      // Two-column settings and 8 diagnostic cards.
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
      // 65: opens the snapshot folder - must exist and never throw.
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
      // 414: the three sections appear in the fixed order backups, storage,
      // retention, and the storage section lists every row the backend sent
      // with a "measure again" button to force a fresh scan.
      {
        const sections = [...document.querySelectorAll('#backup-modal > .modal > .backup-body > div')].map((d) => d.id);
        if (sections.join(',') !== 'backup-panel,storage-panel,retention-panel') {
          out.errors.push('903: the sections are not in backups/storage/retention order (' + sections.join(',') + ')');
        }
        const rows = document.querySelectorAll('#storage-panel .storage-row');
        if (rows.length !== 9) {
          out.errors.push('414: the storage section does not list 9 rows (found ' + rows.length + ')');
        }
        if (!document.getElementById('storage-remeasure-btn')) {
          out.errors.push('414: the storage section has no "measure again" button');
        }
      }
      // A stored password shows as dots - as a placeholder, never as a value.
      const pw = document.querySelector('#backup-panel .backup-pw-wrap .modal-input');
      if (!pw || pw.type !== 'password' || pw.value !== '' || !/^•+$/.test(pw.placeholder)) {
        out.errors.push('a stored backup password is not shown as empty dots');
      }
      // 25: a plain number field replaces the light/normal/heavy preset tiles;
      // The "Keep daily, weekly, monthly and yearly backups" switch sits above it, greys the field out and
      // activates the tier grid below. The panel re-renders on every change, so
      // elements have to be looked up again after each one.
      const settle = async (fn) => { for (let i = 0; i < 60 && !fn(); i++) await sleep(50); return fn(); };
      const keepInput = () => document.querySelector('#backup-panel .backup-keep-row input[type=number]');
      const gfsSwitch = () => [...document.querySelectorAll('#backup-panel .switch-field')]
        .find((l) => l.querySelector('span')?.textContent.trim() === 'Keep daily, weekly, monthly and yearly backups')
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
      // two, the second - an "older build" stand-in - has none).
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
              if (li[1].querySelector('.backup-name-row-chars').textContent !== '\u2014') {
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
    // must ask first - it puts new backups back on the built-in key.
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
    // P64/85: restoring a managed backup goes through maybeBackupBeforeWipe("delete")
    // - the same safety net used before delete-all, gated by the backupBeforeDelete
    // flag. That flag ships default ON (85), so the fixture's default state proves
    // restoreBackup actually wires the call through, cheap to verify without
    // walking the expert menu (which a later 130%-zoom layout check turned out to
    // be sensitive to - not worth touching under this pass's time budget).
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
        // 70: both checkboxes default to checked - confirming as-is restores "all".
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
        document.getElementById('confirm-cb-settings').click(); // uncheck - data only
        await click('confirm-ok');
        await sleep(200);
        const dataArgs = (window.__SMOKE_ARGS__ || []).find(c => c[0] === 'restore_backup');
        if (!dataArgs || dataArgs[1].scope !== 'data') {
          out.errors.push('unchecking settings must restore scope "data", got ' + (dataArgs && dataArgs[1].scope));
        }
      }
    }
    // 70: unchecking both leaves nothing to restore - the confirm button must refuse.
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
"""
