SECTION = """\
    // 32: rotating-prompts flag gates its default-mode dropdown, max-entries slider
    // and badge sub-flag - same contract every other gated expert row follows.
    {
      const rowByLabel5 = (text, sel) => {
        const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
        const find = () => [...document.querySelectorAll('#expert-flags ' + sel)]
          .find(r => ((r.querySelector('.value-head span, span') || {}).textContent || '').includes(text)) || null;
        if (!tabs.length) return find();
        for (const tb of tabs) { tb.click(); const hit = find(); if (hit) return hit; }
        return null;
      };
      const rotSw = rowByLabel5('Rotation', '.switch-field')?.querySelector('input.switch');
      if (!rotSw) {
        out.errors.push('the rotating-prompts toggle is missing from the expert menu');
      } else {
        // 33: default flipped ON (was off at 32's original ship).
        if (!rotSw.checked) out.errors.push('rotating prompts is not on by default (33)');
        if (!rowByLabel5('starts in', '.expert-dropdown-row')) {
          out.errors.push('the rotation-mode dropdown does not show while rotating prompts is on by default');
        }
        if (!rowByLabel5('Max. rotation texts per', '.value-field')) {
          out.errors.push('the max-entries slider does not show while rotating prompts is on by default');
        }
        const badgeSw = rowByLabel5('Show the next rotation text', '.switch-field')?.querySelector('input.switch');
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
        if (rowByLabel5('Max. rotation texts per', '.value-field')) {
          out.errors.push('the max-entries slider still shows after turning rotating prompts off');
        }
        if (rowByLabel5('Show the next rotation text', '.switch-field')) {
          out.errors.push('the badge sub-flag still shows after turning rotating prompts off');
        }
        // Restore the default so later blocks (and a real re-run of this suite) see
        // the same starting state.
        const backSw = rowByLabel5('Rotation', '.switch-field')?.querySelector('input.switch');
        if (backSw) { backSw.checked = true; backSw.dispatchEvent(new Event('change', { bubbles: true })); }
        await sleep(120);
      }
    }
    // 32: editor UI - the rotation switch converts the textarea into a list, "+
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
      const rotSw2 = rowByLabel6('Rotation', '.switch-field')?.querySelector('input.switch');
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
          // Markdown pane may keep a box while the variants list is the editor -
          // both used to survive and left a tall empty gap above the colour row,
          // the pane still showing entry 1 as a stray field.
          // 456: scoped to modal-text's own wrapper, not the first .clear-wrap-ta
          // in the dialog - the always-visible notes field now sits earlier in the
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
          // toggle (90-94) flush right of the mode control - not on a row of its own.
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
            // 67: the centering check above overwrote entry 1 with throwaway text -
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
              // copy_prompt - copyResolved's own path for grid/library/journal).
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
                // window after a real commit - simulated the same way every other
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
      const rotSwOff = rowByLabel6('Rotation', '.switch-field')?.querySelector('input.switch');
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
      const rotSw3 = rowByLabel6('Rotation', '.switch-field')?.querySelector('input.switch');
      if (rotSw3 && !rotSw3.checked) { rotSw3.checked = true; rotSw3.dispatchEvent(new Event('change', { bubbles: true })); }
      await sleep(120);
      document.getElementById('settings-close')?.click();
      await sleep(120);
    }
    // 32-E: library search finds a hit inside ANY entry of a rotating prompt, not
    // just entry 1 - "Entry two" (the "Rotate Test" prompt second entry, created
    // above) exists nowhere else in the fixture.
    // NOTE: no apostrophes inside JS string literals below - this whole PROBE is a
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
"""
