SECTION = """\
    // 127: voice dictation -- button visible by default (ships on), a click
    // arms it, the first event resolves the arming state, a voice-text event
    // lands in the composer input, a second click plus the backend's own
    // voice-stopped in reply ends the session and clears every visible state.
    {
      const voiceHiddenNow = (el) => !el || getComputedStyle(el).display === 'none';
      const voiceBtn = document.getElementById('voice-btn');
      if (!voiceBtn || voiceHiddenNow(voiceBtn)) {
        out.errors.push('Voice button not visible even though voice ships on');
      } else {
        if (!voiceBtn.title.trim() || /^[a-z]+[A-Z]/.test(voiceBtn.title)) {
          out.errors.push('Voice composer button has no real title (raw i18n key or empty)');
        }
        const voiceInput = document.getElementById('input');
        voiceInput.value = '';
        voiceBtn.click();
        await sleep(50);
        if (voiceBtn.getAttribute('aria-pressed') !== 'false') {
          out.errors.push('Voice button aria-pressed should stay false while only arming');
        }
        // 170: arming lights the button up, it must not dim while waiting.
        if (!voiceBtn.classList.contains('is-arming') || parseFloat(getComputedStyle(voiceBtn).opacity) < 1) {
          out.errors.push('Voice button dims while arming instead of lighting up (opacity ' + getComputedStyle(voiceBtn).opacity + ')');
        }
        window.__FIRE__('voice-text', { id: 1, token: 1, text: 'hello world' });
        await sleep(50);
        if (!voiceBtn.classList.contains('is-listening') || voiceBtn.getAttribute('aria-pressed') !== 'true') {
          out.errors.push('Voice button did not switch to is-listening after the first event');
        }
        if (voiceInput.value.trim() !== 'hello world') {
          out.errors.push('Voice text did not land in the composer input (' + JSON.stringify(voiceInput.value) + ')');
        }
        voiceBtn.click();
        window.__FIRE__('voice-stopped', { id: 1, token: 1, reason: 'manual' });
        await sleep(50);
        if (voiceBtn.classList.contains('is-listening') || voiceBtn.classList.contains('is-arming')) {
          out.errors.push('Voice button state did not clear after voice-stopped');
        }
        if (voiceBtn.getAttribute('aria-pressed') !== 'false') {
          out.errors.push('Voice button aria-pressed did not reset to false after stopping');
        }
        // 171: a second dictation round, with nothing else touching the field
        // in between, must continue from where the first one left off rather
        // than jumping back to the front -- in the real app this landed at the
        // start roughly every third time.
        voiceBtn.click();
        await sleep(50);
        const selStartBeforeSecond = voiceInput.selectionStart;
        window.__FIRE__('voice-text', { id: 1, token: 2, text: 'second round' });
        await sleep(50);
        voiceBtn.click();
        window.__FIRE__('voice-stopped', { id: 1, token: 2, reason: 'manual' });
        await sleep(50);
        if (!voiceInput.value.trim().endsWith('second round')) {
          out.errors.push('Second dictation landed at the front instead of the caret (selectionStart read before it was ' + selStartBeforeSecond + ', value is now ' + JSON.stringify(voiceInput.value) + ')');
        }
        layoutScan('Composer with voice button');
      }
    }
    // 171: a late voice-text can still arrive after its own session already
    // reported voice-stopped (the WinRT ResultGenerated/Completed handlers are
    // not ordered against each other) -- it must be delivered to that session's
    // own target instead of being dropped or, worse, picked up by whatever
    // session starts next. Uses the edit dialog's field as target 2 so the two
    // sessions have genuinely different targets, matching the real "clicked a
    // different field and started dictating again" scenario.
    {
      const voiceInput = document.getElementById('input');
      const voiceBtn = document.getElementById('voice-btn');
      voiceInput.value = '';
      voiceBtn.click(); // session token 3
      await sleep(50);
      window.__FIRE__('voice-stopped', { id: 1, token: 3, reason: 'manual' });
      await sleep(50);
      const tile = document.querySelector('#grid .tile');
      let modalVoiceBtn = null, modalText = null;
      if (tile) {
        tile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
        await sleep(200);
        const edit = document.querySelector('#ctx [data-act="edit"]');
        if (edit) { edit.click(); await sleep(400); }
      }
      if (!tile || document.getElementById('modal').classList.contains('hidden')) {
        out.errors.push('could not open the editor for the late-voice-text probe');
      } else {
        modalVoiceBtn = document.getElementById('modal-voice-btn');
        modalText = document.getElementById('modal-text');
        // With Markdown preview on (switched on earlier in this run) a prompt
        // opens rendered by default -- swap to the raw textarea, same click the
        // user would make, before dictating into it.
        const mdPreviewEl = document.getElementById('modal-md-preview');
        if (modalText.classList.contains('hidden') && mdPreviewEl && !mdPreviewEl.classList.contains('hidden')) {
          mdPreviewEl.click();
          await sleep(100);
        }
        modalText.value = '';
        modalVoiceBtn.click(); // session token 4
        await sleep(50);
        // The late text for the FIRST (already-stopped) session arrives while the
        // second is still arming -- must land in the first target, not the second.
        window.__FIRE__('voice-text', { id: 1, token: 3, text: 'LATE' });
        await sleep(50);
        if (!voiceInput.value.trim().endsWith('LATE')) {
          out.errors.push('Late voice-text for an already-stopped session was not delivered to its own target');
        }
        if (modalText.value.includes('LATE')) {
          out.errors.push('Late voice-text for a stopped session leaked into the new session target');
        }
        window.__FIRE__('voice-text', { id: 2, token: 4, text: 'real' });
        await sleep(50);
        modalVoiceBtn.click();
        window.__FIRE__('voice-stopped', { id: 2, token: 4, reason: 'manual' });
        await sleep(50);
        if (!modalText.value.trim().endsWith('real')) {
          out.errors.push('Second session text did not land in its own target (' + JSON.stringify(modalText.value) + ')');
        }
        if (modalVoiceBtn.classList.contains('is-listening') || modalVoiceBtn.classList.contains('is-arming')) {
          out.errors.push('Voice button did not return to idle after the second session stopped');
        }
        // Grace window (3s) has passed -- a further late event for the long-gone
        // first session must now be dropped entirely, not appended anywhere.
        await sleep(3100);
        window.__FIRE__('voice-text', { id: 1, token: 3, text: 'TOO-LATE' });
        await sleep(50);
        if (voiceInput.value.includes('TOO-LATE') || modalText.value.includes('TOO-LATE')) {
          out.errors.push('A voice-text arriving after the late-delivery grace window was not dropped');
        }
        document.getElementById('modal-close-x').click();
        await sleep(80);
        // The editor's own content changed (the voice text above), so closing it
        // opens the shared unsaved-changes confirm dialog -- discard, same as a
        // real user closing without saving, so it does not linger open into the
        // Tour block right after this one and corrupt its dialog-stack checks.
        const closeConfirm = document.getElementById('confirm-modal');
        if (closeConfirm && !closeConfirm.classList.contains('hidden')) {
          document.getElementById('confirm-ok').click();
          await sleep(80);
        }
      }
    }
    // First-run tour -- all five steps reachable, each with a heading and at
    // least one text line, no raw i18n key, no overflow, height barely moves
    // between steps (measured once per open), step 5's four switches match
    // shipsOn() and Restore defaults restores them, Skip/Finish both close the dialog.
    {
      document.getElementById('settings').classList.remove('hidden');
      await sleep(50);
      // No always-available settings button any more -- reach the tour through
      // its one remaining real entry point, the gated expert-menu action, same
      // as an actual user would (data-action survives translation/tab order).
      const findTourBtn = () => document.querySelector('.expert-action[data-action="tour-open"]');
      await click('expert-open');
      let tourBtn = findTourBtn();
      for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
        if (tourBtn) break;
        tb.click();
        await sleep(150);
        tourBtn = findTourBtn();
      }
      if (!tourBtn) {
        out.errors.push('tour-open expert action not found in any tab');
      } else {
        tourBtn.click();
      }
      await sleep(150);
      // The height-per-step comparison below is sensitive to font-metric settling;
      // wait for webfonts to finish loading so it does not race one.
      try { await document.fonts.ready; } catch (e) {}
      await sleep(50);
      const tourEl = document.getElementById('tour-modal');
      if (!tourEl || tourEl.classList.contains('hidden')) {
        out.errors.push('Tour did not open from the expert-menu action');
      } else {
        const isRawKey = (s) => /^(flag|val|tip|exp|stat|backup|help|tour)[A-Z][A-Za-z0-9]+$/.test((s || '').trim());
        const body = document.getElementById('tour-body');
        const heights = [];
        for (let step = 0; step < 5; step++) {
          const title = document.getElementById('tour-title').textContent.trim();
          if (!title || isRawKey(title)) out.errors.push('Tour step ' + step + ': heading missing or raw key (' + title + ')');
          if (step < 4) {
            const art = body.querySelector('.tour-art svg');
            if (!art) {
              out.errors.push('Tour step ' + step + ': no drawing');
            } else {
              // 344: every step's art must actually render (not a collapsed/empty
              // SVG) and highlight the one thing the step is about, via the shared
              // var(--accent) convention every tour drawing follows.
              const box = art.getBoundingClientRect();
              if (box.width < 4 || box.height < 4) {
                out.errors.push('Tour step ' + step + ': the drawing renders at zero size (' + box.width + 'x' + box.height + ')');
              }
              if (!art.querySelector('[fill*="--accent"], [stroke*="--accent"]')) {
                out.errors.push('Tour step ' + step + ': the drawing has no var(--accent)-highlighted element');
              }
            }
            const lines = [...body.querySelectorAll('.tour-text p')].map((p) => p.textContent.trim());
            if (!lines.length || lines.some((l) => !l || isRawKey(l))) {
              out.errors.push('Tour step ' + step + ': missing text line or raw key (' + JSON.stringify(lines) + ')');
            }
            if (lines.length > 3) out.errors.push('Tour step ' + step + ': more than three text lines');
          } else {
            const rows = [...document.querySelectorAll('.tour-opt-row')];
            if (rows.length !== 5) out.errors.push('Tour step 5: expected 5 feature rows, found ' + rows.length);
            for (const row of rows) {
              const cb = row.querySelector('input.switch');
              const note = row.querySelector('.tour-opt-default').textContent.trim();
              if (isRawKey(note)) out.errors.push('Tour step 5: default-note renders a raw key (' + note + ')');
              const desc = row.querySelector('.tour-opt-desc').textContent.trim();
              if (!desc || isRawKey(desc)) out.errors.push('Tour step 5: option description missing or raw key');
              // 390: every feature row carries its own help button, with a real
              // (translated) accessible name -- the button's own click behaviour
              // (close the tour, open help at the row's article) is covered by
              // inspection rather than a second live open/close cycle in this
              // probe, same reasoning as the Skip button below.
              const helpBtn = row.querySelector('.tour-help-btn');
              if (!helpBtn) {
                out.errors.push('Tour step 5: feature row has no help button');
              } else {
                const aria = (helpBtn.getAttribute('aria-label') || '').trim();
                if (!aria || isRawKey(aria)) out.errors.push('Tour step 5: help button has no real accessible name');
              }
            }
            const boxes = rows.map((r) => r.querySelector('input.switch').checked);
            if (JSON.stringify(boxes) !== JSON.stringify([true, true, true, true, true])) {
              out.errors.push('Tour step 5: default switch states are ' + JSON.stringify(boxes) + ', expected all-true (shipsOn defaults)');
            }
            // Flip every switch, then Restore defaults should bring them all back.
            for (const row of rows) {
              const cb = row.querySelector('input.switch');
              cb.checked = !cb.checked;
              cb.dispatchEvent(new Event('change'));
            }
            await sleep(50);
            document.querySelector('.tour-restore-btn').click();
            await sleep(50);
            const restored = [...document.querySelectorAll('.tour-opt-row')].map((r) => r.querySelector('input.switch').checked);
            if (JSON.stringify(restored) !== JSON.stringify([true, true, true, true, true])) {
              out.errors.push('Tour step 5: Restore defaults left switches at ' + JSON.stringify(restored));
            }
          }
          void body.offsetHeight; // force layout before measuring, no stale reflow
          heights.push(body.getBoundingClientRect().height);
          layoutScan('Tour step ' + step);
          if (step < 4) document.getElementById('tour-next').click();
          await sleep(150);
        }
        // The measured min-height's job is to stop a SHORT step from looking tiny
        // next to a long one -- it floors every step to the same height. It is not
        // meant to force a longer step's wrapped text to occupy zero extra room; one
        // line-height of headroom absorbs that and
        // sub-pixel wrap jitter between runs, while still catching the case that
        // matters: the variable genuinely not wired up (a many-line gap).
        const spread = Math.max(...heights) - Math.min(...heights);
        if (spread > 20) out.errors.push('Tour content area height varies by ' + spread + 'px across steps (measured min-height not holding): ' + JSON.stringify(heights));
        const dots = [...document.querySelectorAll('.tour-dot')];
        if (dots.length !== 5) out.errors.push('Tour: expected 5 step dots, found ' + dots.length);
        // Back should be visible and clickable everywhere but step 0.
        if (document.getElementById('tour-back').classList.contains('hidden')) {
          out.errors.push('Tour: Back is hidden on the last step');
        }
        dots[0].click();
        await sleep(80);
        if (!document.getElementById('tour-back').classList.contains('hidden')) {
          out.errors.push('Tour: Back is visible on step 0');
        }
        // Dot navigation must support a direct jump (not just sequential Next/Back),
        // and the last step's button swaps to a distinct, translated Finish label
        // instead of reusing Next (point 185, track 4).
        dots[4].click();
        await sleep(80);
        if (document.getElementById('tour-back').classList.contains('hidden')) {
          out.errors.push('Tour: Back is hidden after jumping straight to the last step via its dot');
        }
        const finishLabel = document.getElementById('tour-next').textContent.trim();
        const backLabel = document.getElementById('tour-back').textContent.trim();
        if (!finishLabel || isRawKey(finishLabel) || finishLabel === backLabel) {
          out.errors.push('Tour: last-step button does not show a distinct Finish label (' + finishLabel + ')');
        }
        dots[0].click();
        await sleep(80);
        // Keyboard focus trap (point 185, track 3): Tab from the last focusable
        // control must wrap to the first, and Shift+Tab from the first must wrap
        // to the last -- the same trapTabWithin() helper the screenshot editor
        // uses (point 181), reused here instead of a second implementation.
        {
          const focusables = [...tourEl.querySelectorAll('button, input, [tabindex]:not([tabindex="-1"]), select, textarea, a[href]')]
            .filter((el) => !el.disabled && el.offsetParent !== null);
          if (focusables.length > 1) {
            const first = focusables[0], last = focusables[focusables.length - 1];
            last.focus();
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));
            if (document.activeElement !== first) {
              out.errors.push('Tour: Tab from the last control did not wrap to the first');
            }
            first.focus();
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true }));
            if (document.activeElement !== last) {
              out.errors.push('Tour: Shift+Tab from the first control did not wrap to the last');
            }
          }
        }
        // A click on the backdrop does nothing: no dialog, the tour stays open.
        const confirmModal = document.getElementById('confirm-modal');
        tourEl.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
        await sleep(80);
        if (!confirmModal.classList.contains('hidden')) out.errors.push('Tour: a background click opened a dialog');
        if (tourEl.classList.contains('hidden')) out.errors.push('Tour: a background click closed the tour');
        // Escape skips at once and shows a toast with Undo. This is the block's one
        // and only close -- a second open/close cycle in the same probe run has been
        // observed to stall the headless browser's virtual clock, so the Undo click
        // and the top-right Skip button are covered by inspection instead.
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await sleep(120);
        if (!confirmModal.classList.contains('hidden')) out.errors.push('Tour: Escape opened a confirm dialog');
        if (!tourEl.classList.contains('hidden')) out.errors.push('Tour: Escape did not close the tour');
        if (!window.__SMOKE_CALLS__.includes('set_ui_flag')) out.errors.push('Tour: Escape did not write tourSeen');
        const tourSkipToast = document.getElementById('toast');
        if (!/Tour skipped|Einf.hrung .bersprungen/.test(tourSkipToast.textContent) || !tourSkipToast.querySelector('.toast-btn')) {
          out.errors.push('Tour: Escape did not show the "Tour skipped" toast with an Undo button');
        }
      }
      document.getElementById('settings').classList.add('hidden');
      await sleep(50);
    }
    // Point 419: shrinking a view below its member count must show an overhang
    // row instead of losing tiles, and growing it back must restore everyone -
    // exactly the arrangement it had before, membership included.
    {
      const homeTab419 = document.querySelector('.view-btn[data-view-id="home"]');
      if (homeTab419) {
        homeTab419.click();
        await sleep(150);
      } else {
        out.errors.push('Overflow: could not find the "home" tab to make it active');
      }
      const before419 = await window.__TAURI__.core.invoke('get_state');
      const homeBefore419 = before419.settings.views.find(v => v.id === 'home');
      if (!homeBefore419) {
        out.errors.push('Overflow: the "home" view is missing from get_state');
      } else {
        const setGrid419 = async (cols, rows) => {
          const c = document.getElementById('qg-cols');
          const r = document.getElementById('qg-rows');
          c.focus(); c.value = String(cols); c.dispatchEvent(new Event('input', { bubbles: true })); c.blur();
          await sleep(120);
          r.focus(); r.value = String(rows); r.dispatchEvent(new Event('input', { bubbles: true })); r.blur();
          await sleep(120);
        };
        await setGrid419(2, 2);
        const overflowRow = document.getElementById('grid-overflow');
        if (!overflowRow || overflowRow.classList.contains('hidden')) {
          out.errors.push('Overflow: shrinking "home" to 2x2 did not show the overhang row for members that no longer fit');
        } else if (!/[0-9]/.test(document.getElementById('grid-overflow-text').textContent)) {
          out.errors.push('Overflow: the overhang row has no visible count');
        } else {
          // One slot for every notice: the overhang row alone shows no counter; with
          // the remote access sign active as well, only that one (higher priority)
          // shows, the counter reads +1 more, and a click swaps the two. Measured
          // live: the front notice and the counter share one row above the grid.
          const stackEl = document.getElementById('banner-stack');
          const moreEl = document.getElementById('banner-more');
          const shown = () => [...stackEl.children].filter(c => c !== moreEl && getComputedStyle(c).display !== 'none');
          if (shown().length !== 1 || shown()[0] !== overflowRow) out.errors.push('banner stack: the overhang row alone is not the one visible notice');
          if (getComputedStyle(moreEl).display !== 'none') out.errors.push('banner stack: a counter shows with a single notice');
          window.__FIRE__('remote-api-status', { running: true, port: 4711 });
          await sleep(200);
          const remoteEl = document.getElementById('remote-api-banner');
          if (shown().length !== 1 || shown()[0] !== remoteEl) out.errors.push('banner stack: with two notices the remote access sign is not the one shown');
          if (getComputedStyle(moreEl).display === 'none' || !moreEl.textContent.includes('1') || moreEl.textContent.includes('{n}')) out.errors.push('banner stack: the counter does not read +1 more: ' + moreEl.textContent);
          if (!moreEl.getAttribute('aria-label')) out.errors.push('banner stack: the counter has no accessible name');
          {
            const rb = shown()[0].getBoundingClientRect(), mb = moreEl.getBoundingClientRect(), gb = document.getElementById('grid').getBoundingClientRect();
            if (!(rb.height > 0 && mb.height > 0)) out.errors.push('banner stack: the front notice or the counter has no height');
            if (mb.left < rb.right - 1) out.errors.push('banner stack: the counter does not sit beside the notice (' + Math.round(mb.left) + ' vs ' + Math.round(rb.right) + ')');
            if (mb.top > rb.bottom || mb.bottom < rb.top) out.errors.push('banner stack: the counter is not on the notice row');
            if (gb.top < rb.bottom - 1) out.errors.push('banner stack: the grid starts above the notice slot');
            if (stackEl.getBoundingClientRect().height > rb.height + 12) out.errors.push('banner stack: the slot is taller than one notice: ' + stackEl.getBoundingClientRect().height + ' vs ' + rb.height);
          }
          moreEl.click();
          await sleep(150);
          if (shown().length !== 1 || shown()[0] !== overflowRow) out.errors.push('banner stack: clicking the counter did not show the next notice');
          window.__FIRE__('remote-api-status', { running: false });
          await sleep(200);
          if (shown().length !== 1 || shown()[0] !== overflowRow || getComputedStyle(moreEl).display !== 'none') out.errors.push('banner stack: after the other notice ended the counter stays or the notice is gone');
        }
        const growBtn419 = document.getElementById('grid-overflow-grow');
        if (!growBtn419) {
          out.errors.push('Overflow: the "enlarge the grid" button is missing');
        } else {
          growBtn419.click();
          await sleep(200);
          if (!document.getElementById('grid-overflow').classList.contains('hidden')) {
            out.errors.push('Overflow: "enlarge the grid" did not bring every member back');
          }
          const after419 = await window.__TAURI__.core.invoke('get_state');
          const homeAfter419 = after419.settings.views.find(v => v.id === 'home');
          if (!homeAfter419 || homeAfter419.members.length !== homeBefore419.members.length) {
            out.errors.push('Overflow: growing the grid back lost view membership');
          }
        }
      }
    }
    {
      // A video attachment's duration/resolution/size, measured once and stored
      // on the fixture prompt (p2), shows a duration badge on its library
      // thumbnail and a resolution/size line in its tooltip -- and shows neither
      // on the same entry's main grid tile, which stays exactly as before.
      await click('library-btn');
      const mmRow = document.querySelector('.lib-item[data-id="p2"]');
      if (!mmRow) {
        out.errors.push('MediaMeta: library row for the video fixture (p2) is missing');
      } else {
        const badge = mmRow.querySelector('.lib-thumb-duration');
        if (!badge || badge.textContent.trim() !== '1:15') {
          out.errors.push('MediaMeta: library duration badge missing or wrong (' + (badge && badge.textContent) + ')');
        }
        if (mmRow.dataset.tipMedia !== '1280×720 · 5.0 MB') {
          out.errors.push('MediaMeta: library tooltip is missing the resolution/size line (' + mmRow.dataset.tipMedia + ')');
        }
      }
      await click('library-close');
      const mmTile = document.querySelector('.tile[data-id="p2"]');
      if (mmTile && mmTile.querySelector('.lib-thumb-duration')) {
        out.errors.push('MediaMeta: the main grid tile must never show the duration badge');
      }
    }
    {
      // Search can look inside a picture's recognized text, but only while
      // the switch is on, and only as the very last resort. "lighthouse" is
      // in no prompt's name or body, so a hit can only have come from the
      // stubbed search_image_text (fixture id p4), and while the switch is
      // still off (the shipped default) the same query must match nothing at
      // all, exactly as it would have before this feature existed.
      // An earlier probe leaves the UI language switched to German; this one
      // reads English labels, so it switches back first.
      const langSelOcr = document.getElementById('lang-select');
      if (langSelOcr && langSelOcr.value !== 'en') {
        langSelOcr.value = 'en';
        langSelOcr.dispatchEvent(new Event('change'));
        await sleep(100);
      }
      await click('library-btn');
      const oq = document.getElementById('library-q');
      oq.value = 'lighthouse';
      oq.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const offHits = document.querySelectorAll('#library-list .lib-item').length;
      if (offHits !== 0) {
        out.errors.push('OcrIndex: "lighthouse" matched ' + offHits + ' prompts with the switch off (default), expected 0');
      }
      await click('library-close');
      const ocrRow = () => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('#expert-flags .switch-field')]
            .find(r => ((r.querySelector('span') || {}).textContent || '').includes('Index text inside attached pictures for search'));
          if (hit) return hit;
        }
        return null;
      };
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const ocrSwitch = ocrRow();
      if (!ocrSwitch) {
        out.errors.push('OcrIndex: the setting is missing from the expert menu');
      } else {
        ocrSwitch.querySelector('input.switch').click();
      }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
      await click('library-btn');
      const oq2 = document.getElementById('library-q');
      oq2.value = 'lighthouse';
      oq2.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const onRow = document.querySelector('.lib-item[data-id="p4"]');
      if (!onRow) {
        out.errors.push('OcrIndex: "lighthouse" found no row for the image-text fixture (p4) with the switch on');
      } else {
        const tag = onRow.querySelector('.lib-image-hit-tag');
        if (!tag || !tag.textContent.trim()) {
          out.errors.push('OcrIndex: the library row has no "found in image" badge');
        }
      }
      {
        // One query change with the switch on must redraw the list once, not
        // once for the text match and again when the picture lookup answers.
        const libList = document.getElementById('library-list');
        const innerDesc = Object.getOwnPropertyDescriptor(Element.prototype, 'innerHTML');
        let redraws = 0;
        Object.defineProperty(libList, 'innerHTML', {
          configurable: true,
          get() { return innerDesc.get.call(this); },
          set(v) { if (v === '') redraws++; innerDesc.set.call(this, v); },
        });
        const oq3 = document.getElementById('library-q');
        oq3.value = 'lighthouse ';
        oq3.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(400);
        delete libList.innerHTML;
        if (redraws !== 1) {
          out.errors.push('OcrIndex: one search change redrew the library ' + redraws + ' times, expected 1');
        }
      }
      await click('library-close');
    }
"""
