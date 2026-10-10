SECTION = """\
    // The filter row explains the dot: a visible legend that carries a badge.
    {
      const legend = document.querySelector('#settings-expert .expert-legend');
      const lr = legend && legend.getBoundingClientRect();
      if (!legend || !lr || lr.width < 1 || lr.height < 1) {
        out.errors.push('735: the default-off legend is not visible on the expert page');
      } else {
        if (!legend.querySelector('.default-off-badge')) out.errors.push('735: the legend has no dot badge');
        if (!legend.textContent.trim()) out.errors.push('735: the legend has no text');
      }
    }
    // P69: a default-off feature gets a small dot badge instead of a "(Default
    // off)" text suffix crowding the label - find the rows across all tabs.
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
    // 372: expGroupCreate was split into expTabCapture's finer groups - a moved
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
      search2.value = 'weekly, monthly';
      search2.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(200);
      const backupRow = [...document.querySelectorAll('.expert-foreign-row')]
        .find(r => (r.querySelector('span') || {}).textContent === 'Keep daily, weekly, monthly and yearly backups');
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
    // 728: the favorites view switch lives in Settings only; the expert menu keeps no
    // row for it, and the dependent rows follow it and the favorites feature.
    {
      if (document.getElementById('settings').classList.contains('hidden')) await click('gear');
      if (document.getElementById('settings-main').classList.contains('hidden')) await click('expert-back');
      const favSwitch728 = document.getElementById('opt-favview');
      const wasOn728 = favSwitch728.checked;
      if (!wasOn728) { favSwitch728.click(); await sleep(150); }
      await click('expert-open');
      const rowOf728 = (key) => document.querySelector('#settings-expert [data-setting-row="' + key + '"]');
      const search728 = document.getElementById('expert-search');
      const find728 = async (q) => {
        search728.value = q;
        search728.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(250);
      };
      await find728('Favorites');
      if (!rowOf728('favorites')) out.errors.push('728: the favorites feature switch is missing from the expert menu');
      if (rowOf728('favViewButton')) out.errors.push('728: the favorites view switch is still a row in the expert menu');
      if (!rowOf728('favStarSize') || !rowOf728('favMaxCols')) out.errors.push('728: the favorites value rows are missing with the feature and the view switch on');
      const favRow728 = rowOf728('favorites');
      const favInput728 = favRow728 && favRow728.querySelector('input.switch');
      if (favInput728) {
        favInput728.click();
        await sleep(250);
        await find728('Favorites');
        if (rowOf728('favStarSize') || rowOf728('favMaxCols')) out.errors.push('728: the favorites value rows stay with the feature off');
        const again728 = rowOf728('favorites');
        if (again728) { again728.querySelector('input.switch').click(); await sleep(250); }
      }
      await find728('Favorites view');
      const hits728 = [...document.querySelectorAll('#expert-flags span')].filter((el) => el.textContent.trim() === 'Favorites view');
      if (hits728.some((el) => !el.closest('.expert-foreign-row'))) out.errors.push('728: the expert search for Favorites view finds an expert row');
      if (!document.querySelector('.expert-foreign-row')) out.errors.push('728: the expert search for Favorites view has no Settings hit');
      await find728('');
      await click('expert-back');
      if (!wasOn728) { document.getElementById('opt-favview').click(); await sleep(150); }
    }
    // 343: the recording stop/pause hotkeys moved out of Settings and into the
    // expert menu itself, as a real capture row (not a jump-to-settings button).
    {
      if (document.getElementById('rec-stop-hotkey-row') || document.getElementById('rec-pause-hotkey-row')) {
        out.errors.push('343: the recording hotkey rows are still in the Settings dialog');
      }
      // The switch and its capture field read differently ("Stop recording with a
      // hotkey" over "Key combination"); the field is the row right under its switch.
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
      const fieldUnder = (sw) => {
        const n = sw && sw.nextElementSibling;
        return n && n.classList.contains('hotkey-field') ? n : null;
      };
      const stopRow = fieldUnder(switchRowByLabel('Stop recording with a hotkey'));
      const pauseRow = fieldUnder(switchRowByLabel('Pause recording with a hotkey'));
      if (!stopRow || !pauseRow) {
        out.errors.push('343: the expert menu has no capture row for one or both recording hotkeys');
      } else {
        const input = stopRow.querySelector('.hotkey-row .modal-input');
        const clearBtn = stopRow.querySelector('.hotkey-row .ghost-btn');
        if (!input || !/8$/.test(input.value)) {
          out.errors.push('343: the stop-recording row does not show its stored hotkey');
        }
        if (!clearBtn) out.errors.push('343: the stop-recording row has no Clear button');
        const stopFieldText = (stopRow.querySelector('.value-head span') || {}).textContent;
        if (stopFieldText !== 'Key combination' || input.getAttribute('aria-label') !== 'Key combination') {
          out.errors.push('343: the stop-recording field is not labelled Key combination, got ' + stopFieldText);
        }
        if (stopFieldText === (switchRowByLabel('Stop recording with a hotkey').querySelector('span') || {}).textContent) {
          out.errors.push('343: the stop-recording switch and its field carry the same text');
        }
        // Turning its own flag off must drop the row, same as every other gated setting.
        const stopFlagRow = switchRowByLabel('Stop recording with a hotkey');
        if (!stopFlagRow) {
          out.errors.push('343: no flag toggle found for the stop-recording hotkey');
        } else {
          stopFlagRow.querySelector('.switch').click();
          await sleep(80);
          if (fieldUnder(switchRowByLabel('Stop recording with a hotkey'))) {
            out.errors.push('343: the stop-recording capture row stayed visible after its own flag was switched off');
          }
          switchRowByLabel('Stop recording with a hotkey')?.querySelector('.switch').click(); // restore
          await sleep(80);
        }
      }
    }
    // 67: "only changed" filter - a flag at its default is hidden, flipping it in
    // makes the row (re)appear, flipping it back to default hides it again.
    {
      const onlyChanged = document.getElementById('expert-only-changed');
      // Unlike the badge-lookup above, this also has to work while "only changed" is
      // checked - the tab bar hides then (every tab is scanned in one flat pass, see
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
        // "Always show grid lines" ships off - flip it on, off is its default.
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
          // Flip it straight back to its default - must vanish from the filtered view.
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
    // 71: per-category reset - resets only the settings that belong to the group
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
      const otherSw = rowByLabel2('Sort by usage')?.querySelector('input.switch');
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
            const otherAfter = rowByLabel2('Sort by usage')?.querySelector('input.switch');
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
              const backSw = rowByLabel2('Sort by usage')?.querySelector('input.switch');
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
      let historySw = rowByLabel5('Prompt versions')?.querySelector('input.switch');
      if (!historySw) {
        out.errors.push('could not find "Prompt versions" for the gated-hidden-row reset test');
      } else {
        if (!historySw.checked) { historySw.click(); await sleep(120); } // reveal the gated child
        const diffSw = rowByLabel5('Word-by-word comparison of versions')?.querySelector('input.switch');
        if (!diffSw) {
          out.errors.push('could not find the gated "Word-by-word comparison of versions" row');
        } else {
          if (diffSw.checked) { diffSw.click(); await sleep(120); } // move it away from its default (on)
          // Hide it again by switching its own parent back off.
          rowByLabel5('Prompt versions')?.querySelector('input.switch')?.click();
          await sleep(120);
          if (rowByLabel5('Word-by-word comparison of versions')) {
            out.errors.push('405 setup: the gated row is still visible, cannot test the hidden-row case');
          } else {
            const groupSec = rowByLabel5('Prompt versions')?.closest('.expert-group');
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
              const historySwAfter = rowByLabel5('Prompt versions')?.querySelector('input.switch');
              if (!historySwAfter || !historySwAfter.checked) {
                out.errors.push('405: group reset did not restore the parent flag to its own default');
              }
              const diffSwAfter = rowByLabel5('Word-by-word comparison of versions')?.querySelector('input.switch');
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
    // 31: per-value reset - a row's own undo icon appears only once IT changes,
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
    // the backend call each click makes, not the live DOM - cross-tab lookups here
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
    // diag1: the Troubleshooting group starts folded. A folded list stored before the
    // group existed gets it added once (guarded by diagGroupSeeded); unfolding it later
    // is not undone by the next render.
    {
      const coreD = await import('./core.js');
      const diagSec = () => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('#expert-flags .expert-group')]
            .find((g) => (g.querySelector('.expert-group-title span') || {}).textContent === 'Troubleshooting');
          if (hit) return hit;
        }
        return null;
      };
      coreD.settings.ui_texts = { ...(coreD.settings.ui_texts || {}), expertCollapsed: JSON.stringify(['expGroupHelp']) };
      coreD.settings.ui_flags = { ...(coreD.settings.ui_flags || {}), diagGroupSeeded: false };
      let secD = diagSec();
      if (!secD) out.errors.push('diag1: the Troubleshooting group is not in the System tab');
      else {
        if (!secD.classList.contains('collapsed')) out.errors.push('diag1: the Troubleshooting group is open although the stored folded list lacked it');
        const listD = JSON.parse(coreD.settings.ui_texts.expertCollapsed);
        if (!listD.includes('expGroupDiagnostics') || !listD.includes('expGroupHelp')) out.errors.push('diag1: the stored folded list is ' + JSON.stringify(listD));
        if (!coreD.settings.ui_flags.diagGroupSeeded) out.errors.push('diag1: the seeded flag was not set');
        secD.querySelector('.expert-group-title').click();
        await sleep(100);
        secD = diagSec();
        if (secD && secD.classList.contains('collapsed')) out.errors.push('diag1: the group folded itself again after the user opened it');
      }
    }
"""
