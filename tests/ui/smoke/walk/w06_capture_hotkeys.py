SECTION = """\
    // F26: the colour picker is an entry of the capture menu, and a picked colour reports
    // back as a toast tinted with that colour.
    const captureBtnEl = document.getElementById('capture-btn');
    // The fixture reports a running recording by default, and the capture button
    // follows it on its poll: report none until the 506 probe asks for one.
    window.__REC_IDLE__ = true;
    document.dispatchEvent(new Event('visibilitychange'));
    await sleep(150);
    const captureEntries = () => {
      const m = document.querySelector('.tab-menu:not(.hidden)');
      return m ? [...m.querySelectorAll('button[role=menuitem]')] : [];
    };
    const captureEntry = async (text) => {
      captureBtnEl.click();
      await sleep(120);
      return captureEntries().find((b) => b.textContent.trim() === text) || null;
    };
    if (!captureBtnEl) out.errors.push('the capture button is missing');
    else {
      if (getComputedStyle(captureBtnEl).display === 'none') out.errors.push('the capture button stays hidden although the features are on');
      const pickEntry = await captureEntry('Pick a color from the screen');
      if (!pickEntry) out.errors.push('F26: the capture menu has no colour picker entry');
      else {
        pickEntry.click();
        await sleep(150);
        if (!(window.__SMOKE_CALLS__ || []).includes('open_picker')) out.errors.push('F26: the picker entry did not open the picker');
      }
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
    // 19: the "Recent colors" entry reaches the pick history without opening the
    // overlay - the fixture backend always returns a fixed two-colour list.
    if (captureBtnEl) {
      const recentEntry = await captureEntry('Recent colors');
      if (!recentEntry) out.errors.push('19: the capture menu has no Recent colors entry');
      else recentEntry.click();
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
      const recentAgain = await captureEntry('Recent colors');
      if (recentAgain) recentAgain.click();
      await sleep(120);
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
      await sleep(120);
      if (!document.querySelector('.pick-history-pop').classList.contains('hidden')) {
        out.errors.push('19: clicking outside the pick-history popover did not close it');
      }
    }
    // 417: the capture menu opens an existing file in whichever editor its content
    // calls for, not only through a fresh capture or recording.
    if (!captureBtnEl) {
      out.errors.push('417: the capture button is missing');
    } else {
      const capTip = captureBtnEl.title || '';
      if (!/^Capture: /.test(capTip)) out.errors.push('417: the capture button tooltip does not name its tools: ' + capTip);
      const capAria = captureBtnEl.getAttribute('aria-label') || '';
      if (capAria !== 'Capture') out.errors.push('417: the capture button has no plain accessible name: ' + capAria);
      if (captureBtnEl.getAttribute('aria-haspopup') !== 'menu') out.errors.push('417: the capture button does not announce its menu');
      const openEntry = await captureEntry('Open a file in the editor');
      if (!openEntry) {
        out.errors.push('417: the capture menu has no open-in-editor entry');
      } else {
        // Picking a VIDEO through this entry must open the video editor:
        // content decides, never which entry was used.
        window.__SMOKE_CALLS__ = [];
        window.__SMOKE_ARGS__ = [];
        openEntry.click();
        await sleep(200);
        if (!window.__SMOKE_CALLS__.includes('pick_media_file_path')) {
          out.errors.push('417: choosing the menu entry did not offer a file picker');
        }
        if (!window.__SMOKE_CALLS__.includes('open_file_in_editor')) {
          out.errors.push('417: choosing the menu entry did not ask the backend what the file is');
        }
        const openCall = (window.__SMOKE_ARGS__ || []).find((c) => c[0] === 'open_video_editor');
        if (!openCall) {
          out.errors.push('417: picking a video through the capture menu did not open the video editor');
        } else {
          if (openCall[1].srcPath !== 'C:\\\\Temp\\\\ps-smoke-openedit-copy.mp4') {
            out.errors.push('417: the video editor opened on the wrong file: ' + openCall[1].srcPath);
          }
          if (openCall[1].originDir !== 'C:\\\\Pictures') {
            out.errors.push('417: the video editor did not receive the origin folder');
          }
        }
      }
      // An outside click closes the menu, same as every other popover.
      captureBtnEl.click();
      await sleep(120);
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
      await sleep(120);
      if (captureEntries().length) out.errors.push('417: clicking outside the capture menu did not close it');
    }
    // 703: with every capture tool on and the camera off the menu lists exactly the
    // seven entries in order; while a recording runs a click stops it instead of
    // opening the menu (506 below drives the recording state).
    if (captureBtnEl) {
      captureBtnEl.click();
      await sleep(120);
      const labels703 = captureEntries().map((b) => b.textContent.trim());
      const want703 = ['Screenshot', 'Record screen', 'Pick a color from the screen', 'Recent colors',
        'Text from a screenshot (OCR)', 'Text from an image file (OCR)', 'Open a file in the editor'];
      if (labels703.join('|') !== want703.join('|')) {
        out.errors.push('703: the capture menu reads ' + JSON.stringify(labels703));
      }
      if (captureEntries().some((b) => !b.querySelector('svg'))) out.errors.push('703: a capture menu entry has no icon');
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
      await sleep(120);
    }
    // 506: a capture-button click while a recording is already running must stop
    // it instead of trying to open a second one -- recording_status's own
    // fixture always reports an active recording, so forcing one resync
    // through the same visibilitychange path the real app uses (rather than
    // waiting out the real 5-second poll) is enough to reach that state.
    window.__SMOKE_CALLS__ = [];
    window.__REC_IDLE__ = false;
    document.dispatchEvent(new Event('visibilitychange'));
    await sleep(150);
    if (!captureBtnEl.classList.contains('is-recording') && !captureBtnEl.classList.contains('is-paused')) {
      out.errors.push('506: capture button never picked up the active-recording state from recording_status');
    } else {
      if (captureBtnEl.title !== 'Stop recording') out.errors.push('703: the capture button tooltip while recording is ' + captureBtnEl.title);
      window.__SMOKE_CALLS__ = [];
      captureBtnEl.click();
      await sleep(150);
      if (!window.__SMOKE_CALLS__.includes('stop_recording')) {
        out.errors.push('506: clicking the capture button during an active recording did not call stop_recording');
      }
      if (window.__SMOKE_CALLS__.includes('open_recorder')) {
        out.errors.push('506: clicking the capture button during an active recording also called open_recorder');
      }
      if (captureEntries().length) out.errors.push('703: a click during a recording opened the menu instead of stopping it');
    }
    // Back to "no recording" so the later probes find the capture button idle.
    window.__REC_IDLE__ = true;
    document.dispatchEvent(new Event('visibilitychange'));
    await sleep(150);
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
