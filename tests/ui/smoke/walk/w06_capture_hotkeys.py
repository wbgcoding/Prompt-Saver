SECTION = """\
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
    // overlay - the fixture backend always returns a fixed two-colour list.
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
    // 417: right-click either composer button for a quiet menu to open an
    // existing file in whichever editor its content calls for, not only
    // through a fresh capture or recording.
    const snipBtnEl = document.getElementById('snip-btn');
    const recordBtnEl = document.getElementById('record-btn');
    if (!snipBtnEl || !recordBtnEl) {
      out.errors.push('417: the screenshot or record button is missing');
    } else {
      // Every composer button with a right-click function names it in parentheses
      // in its tooltip; the accessible name of the first two stays the plain name.
      for (const id of ['snip-btn', 'record-btn', 'picker-btn', 'file-btn']) {
        const btnEl = document.getElementById(id) || {};
        // A running recording swaps the record tooltip for its stop text.
        if (id === 'record-btn' && btnEl.classList && (btnEl.classList.contains('is-recording') || btnEl.classList.contains('is-paused'))) {
          if (btnEl.dataset.i18nTitle !== 'recordBtnTip') out.errors.push('the record button idle tooltip is not the one with the right-click hint');
          continue;
        }
        const tip = btnEl.title || '';
        if (!/[(（].+[:：].+[)）]/.test(tip)) out.errors.push('composer button ' + id + ' tooltip does not name its right-click function: ' + tip);
      }
      for (const b of [snipBtnEl, recordBtnEl]) {
        const al = b.getAttribute('aria-label') || '';
        if (!al || /[(（]/.test(al)) out.errors.push('composer button ' + b.id + ' has no plain accessible name: ' + al);
      }
      recordBtnEl.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(150);
      let openEditPop = document.querySelector('.open-editor-pop');
      if (!openEditPop || openEditPop.classList.contains('hidden')) {
        out.errors.push('417: right-clicking the record button did not open the open-in-editor menu');
      }
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
      await sleep(120);
      if (openEditPop && !openEditPop.classList.contains('hidden')) {
        out.errors.push('417: clicking outside the open-in-editor menu did not close it');
      }
      // Picking a VIDEO through the screenshot (image) button must still open
      // the video editor: content decides, never which button was used.
      snipBtnEl.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(150);
      openEditPop = document.querySelector('.open-editor-pop');
      if (!openEditPop || openEditPop.classList.contains('hidden')) {
        out.errors.push('417: right-clicking the screenshot button did not open the open-in-editor menu');
      } else if (!(openEditPop.querySelector('button').textContent || '').trim()) {
        out.errors.push('417: the open-in-editor menu entry has no label');
      } else {
        window.__SMOKE_CALLS__ = [];
        window.__SMOKE_ARGS__ = [];
        openEditPop.querySelector('button').click();
        await sleep(200);
        if (!window.__SMOKE_CALLS__.includes('pick_media_file_path')) {
          out.errors.push('417: choosing the menu entry did not offer a file picker');
        }
        if (!window.__SMOKE_CALLS__.includes('open_file_in_editor')) {
          out.errors.push('417: choosing the menu entry did not ask the backend what the file is');
        }
        const openCall = (window.__SMOKE_ARGS__ || []).find((c) => c[0] === 'open_video_editor');
        if (!openCall) {
          out.errors.push('417: picking a video through the screenshot button did not open the video editor');
        } else {
          if (openCall[1].srcPath !== 'C:\\\\Temp\\\\ps-smoke-openedit-copy.mp4') {
            out.errors.push('417: the video editor opened on the wrong file: ' + openCall[1].srcPath);
          }
          if (openCall[1].originDir !== 'C:\\\\Pictures') {
            out.errors.push('417: the video editor did not receive the origin folder');
          }
        }
      }
    }
    // rec254: with no recording session the main window starts idle, as the real backend
    // answers: one resync leaves the record button without a recording or paused ring.
    window.__REC_ACTIVE__ = undefined;
    document.dispatchEvent(new Event('visibilitychange'));
    await sleep(150);
    if (recordBtnEl.classList.contains('is-recording') || recordBtnEl.classList.contains('is-paused')) {
      out.errors.push('rec254: the record button shows a recording state with no recording session');
    }
    // 506: a record-btn click while a recording is already running must stop
    // it instead of trying to open a second one -- the stub reports an active
    // recording while __REC_ACTIVE__ is set, so forcing one resync
    // through the same visibilitychange path the real app uses (rather than
    // waiting out the real 5-second poll) is enough to reach that state.
    window.__REC_ACTIVE__ = true;
    window.__SMOKE_CALLS__ = [];
    document.dispatchEvent(new Event('visibilitychange'));
    await sleep(150);
    if (!recordBtnEl.classList.contains('is-recording') && !recordBtnEl.classList.contains('is-paused')) {
      out.errors.push('506: record button never picked up the active-recording state from recording_status');
    } else {
      window.__SMOKE_CALLS__ = [];
      recordBtnEl.click();
      await sleep(150);
      if (!window.__SMOKE_CALLS__.includes('stop_recording')) {
        out.errors.push('506: clicking the record button during an active recording did not call stop_recording');
      }
      if (window.__SMOKE_CALLS__.includes('open_recorder')) {
        out.errors.push('506: clicking the record button during an active recording also called open_recorder');
      }
    }
    window.__REC_ACTIVE__ = false;
    // 995: the backend announces every recording state change and the record button follows,
    // with no poll in between.
    {
      const state = () => (recordBtnEl.classList.contains('is-paused') ? 'paused' : recordBtnEl.classList.contains('is-recording') ? 'recording' : 'idle');
      for (const next of ['recording', 'paused', 'recording', 'idle']) {
        window.__FIRE__('recording-state', next);
        await sleep(60);
        if (state() !== next) out.errors.push('995: after recording-state ' + next + ' the record button reads ' + state());
      }
    }
    // 835: the bar failing to open after a good start must not reset a live
    // recording to idle. The countdown runs first, so wait for the bar call.
    window.__FIRE__('recording-cancelled');
    await sleep(50);
    window.__SHOW_REC_BAR_FAIL__ = true;
    window.__SMOKE_CALLS__ = [];
    window.__FIRE__('recorder-target-picked', { kind: 'monitor', monitor: 1 });
    for (let i = 0; i < 160 && !(window.__SMOKE_CALLS__ || []).includes('show_rec_bar'); i++) await sleep(50);
    await sleep(60);
    window.__SHOW_REC_BAR_FAIL__ = false;
    if (!(window.__SMOKE_CALLS__ || []).includes('show_rec_bar')) {
      out.errors.push('835: the recording start never reached the bar call');
    } else {
      if (!recordBtnEl.classList.contains('is-recording')) {
        out.errors.push('835: the record button went idle although the recording runs and only the bar failed');
      }
      if ((window.__SMOKE_CALLS__ || []).includes('hide_rec_bar')) {
        out.errors.push('835: a failed bar open hid the bar of a running recording');
      }
    }
    // P60: an automatic backup toasts the same way a manual one already did -
    // the backend has no other call site to toast from, so it fires this itself.
    window.__FIRE__('backup-done');
    await sleep(150);
    if (!(toastEl.textContent || '').includes('Backup created')) {
      out.errors.push('an automatic backup gave no "Backup created" toast: ' + toastEl.textContent);
    }

    // F20/98: pressing a prompt hotkey runs the same activation a tile does - the
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
    // standalone window - the main window must not be pulled forward for it.
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
    {
      let req = null;
      try { req = JSON.parse(window.__VAR_PAYLOAD__ || 'null'); } catch (e) { req = null; }
      if (!req || !Array.isArray(req.rows) || req.rows.length !== 2) {
        out.errors.push('the standalone window was handed the wrong rows');
      } else if (req.rows.map(r => r.name).join(',') !== 'Text,Language') {
        out.errors.push('the standalone rows are ' + req.rows.map(r => r.name).join(','));
      } else if (!req.lang) {
        out.errors.push('the standalone window was handed no language');
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
"""
