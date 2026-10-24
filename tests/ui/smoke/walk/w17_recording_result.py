SECTION = """\
    // 241: the recording result dialog (#rec-modal) -- mirrors the screenshot
    // editor's own probes above, plus the conversion step that only the recording flow has.
    {
      const langSel3 = document.getElementById('lang-select');
      langSel3.value = 'en';
      langSel3.dispatchEvent(new Event('change'));
      await sleep(100);
      const recModal = document.getElementById('rec-modal');
      const recPreview = document.getElementById('rec-preview');
      const recStatus = document.getElementById('rec-modal-status');
      // Drive the real recFormat control (expert menu) rather than poking
      // internal state -- found structurally (an <option value="webm">) so it
      // works whatever tab/group it currently renders in or whatever language
      // is active.
      await click('expert-open');
      const findDropdown = (optionValue) => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('select.expert-select')]
            .find((s) => [...s.options].some((o) => o.value === optionValue));
          if (hit) return hit;
        }
        return null;
      };
      const recFormatSel = findDropdown('webm');
      if (!recFormatSel) {
        out.errors.push('rec-modal: could not find the recFormat expert dropdown');
      } else {
        recFormatSel.value = 'webm';
        recFormatSel.dispatchEvent(new Event('change'));
      }
      await sleep(50);
      await click('expert-back');
      await sleep(50);

      // Conversion path (recFormat != mp4): opens straight into a "Converting"
      // status instead of a preview, updates from a progress event, then shows
      // the converted file once convert_recording resolves.
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw.mp4' });
      await sleep(30);
      if (recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: did not open on recording-captured');
      }
      if (recPreview.classList.contains('hidden') === false) {
        out.errors.push('rec-modal: preview must stay hidden while converting');
      }
      if (!/Converting/.test(recStatus.textContent)) {
        out.errors.push('rec-modal: status did not show the converting message, got ' + JSON.stringify(recStatus.textContent));
      }
      window.__FIRE__('recording-convert-progress', { ms: 3200 });
      await sleep(30);
      if (!recStatus.textContent.includes('3s')) {
        out.errors.push('rec-modal: converting status did not update from a progress event, got ' + JSON.stringify(recStatus.textContent));
      }
      await sleep(150); // let the stubbed convert_recording resolve (80ms)
      if (recPreview.classList.contains('hidden')) {
        out.errors.push('rec-modal: preview did not appear once conversion finished');
      }
      if (!recPreview.src.includes('.webm')) {
        out.errors.push('rec-modal: preview src does not point at the converted file, got ' + JSON.stringify(recPreview.src));
      }
      if (recStatus.textContent.trim() !== '') {
        out.errors.push('rec-modal: converting status must clear once done, got ' + JSON.stringify(recStatus.textContent));
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);
      if (!recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: close button did not hide the dialog');
      }

      // ffmpeg missing: the specific error convert_recording can return before
      // ever starting a process -- must name ffmpeg, not a generic failure, and
      // must still leave the raw file choosable (never dead-ends the dialog).
      window.__FIRE__('recording-captured', { path: 'C:/fake/needs-ffmpeg-MISSING.mp4' });
      await sleep(150);
      if (!/ffmpeg/i.test(recStatus.textContent)) {
        out.errors.push('rec-modal: a missing-ffmpeg failure must say so, got ' + JSON.stringify(recStatus.textContent));
      }
      if (recPreview.classList.contains('hidden')) {
        out.errors.push('rec-modal: a failed conversion must fall back to previewing the raw recording');
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);

      // 399: a real classified ffmpeg failure here too -- translated headline,
      // raw ffmpeg text only under the closed "Technical details" row, kept
      // recording still previewable.
      const recErrDetails = document.getElementById('rec-modal-error-details');
      const recErrRaw = document.getElementById('rec-modal-error-raw');
      window.__FIRE__('recording-captured', { path: 'C:/fake/needs-ffmpeg-FAILCLASSIFY.mp4' });
      await sleep(150);
      if (recStatus.textContent.includes('No space left on device') || recStatus.textContent.trim().startsWith('{')) {
        out.errors.push('rec-modal: a classified failure showed the raw/JSON text verbatim, got ' + JSON.stringify(recStatus.textContent));
      }
      if (recErrDetails.classList.contains('hidden') || recErrDetails.open) {
        out.errors.push('rec-modal: "Technical details" must appear closed, not hidden and not pre-opened');
      }
      if (!recErrRaw.textContent.includes('No space left on device')) {
        out.errors.push('rec-modal: "Technical details" did not carry the raw ffmpeg text, got ' + JSON.stringify(recErrRaw.textContent));
      }
      if (recPreview.classList.contains('hidden')) {
        out.errors.push('rec-modal: a failed conversion must fall back to previewing the raw recording');
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);

      // Reset recFormat back to mp4 -- everything below is the no-conversion path.
      if (recFormatSel) {
        await click('expert-open');
        recFormatSel.value = 'mp4';
        recFormatSel.dispatchEvent(new Event('change'));
        await sleep(30);
        await click('expert-back');
        await sleep(30);
      }

      // No-conversion path: opens straight into a playable preview.
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw2.mp4' });
      await sleep(30);
      if (recModal.classList.contains('hidden') || recPreview.classList.contains('hidden')) {
        out.errors.push('rec-modal: the no-conversion path must show the preview immediately');
      }

      // Discard removes the file and closes the dialog.
      window.__SMOKE_CALLS__ = [];
      document.getElementById('rec-modal-discard').click();
      await sleep(30);
      if (!window.__SMOKE_CALLS__.includes('discard_recording_file')) {
        out.errors.push('rec-modal: Discard did not call discard_recording_file');
      }
      if (!recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: Discard did not close the dialog');
      }

      // Save as copies out without closing the dialog (same contract as the
      // screenshot editor's own Save as).
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw3.mp4' });
      await sleep(30);
      window.__SMOKE_CALLS__ = [];
      // 674: the dialog shows Discard, More, Save, Attach; the rarer actions sit in
      // the menu behind More, which opens, closes on Escape without closing the dialog,
      // and closes itself when an item is picked.
      {
        const actionIds = [...document.querySelectorAll('#rec-modal .modal-actions button')].map((b) => b.id).join(',');
        if (actionIds !== 'rec-modal-discard,rec-modal-more,rec-modal-save,rec-modal-yes') out.errors.push('674: the recording dialog actions are ' + actionIds);
        const morePop = document.getElementById('rec-modal-more-pop');
        const moreBtn = document.getElementById('rec-modal-more');
        if (!morePop.classList.contains('hidden')) out.errors.push('674: the More menu is open before it was asked for');
        moreBtn.click();
        await sleep(60);
        if (morePop.classList.contains('hidden') || moreBtn.getAttribute('aria-expanded') !== 'true') out.errors.push('674: More did not open its menu');
        const itemIds = [...morePop.querySelectorAll('button:not(.hidden)')].map((b) => b.id).join(',');
        if (itemIds !== 'rec-modal-edit,rec-modal-saveas,rec-modal-copy,rec-modal-folder-change') out.errors.push('674: the More menu holds ' + itemIds);
        const pr = morePop.getBoundingClientRect();
        if (pr.left < 0 || pr.top < 0 || pr.right > window.innerWidth + 1 || pr.bottom > window.innerHeight + 1) out.errors.push('674: the More menu leaves the window');
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
        await sleep(60);
        if (!morePop.classList.contains('hidden')) out.errors.push('674: Escape did not close the More menu');
        if (recModal.classList.contains('hidden')) out.errors.push('674: Escape closed the whole dialog instead of only the More menu');
        moreBtn.click();
        await sleep(60);
      }
      document.getElementById('rec-modal-saveas').click();
      await sleep(30);
      if (!window.__SMOKE_CALLS__.includes('save_recording_as')) {
        out.errors.push('rec-modal: Save as did not call save_recording_as');
      }
      if (!document.getElementById('rec-modal-more-pop').classList.contains('hidden')) {
        out.errors.push('674: picking a menu item did not close the More menu');
      }
      if (recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: Save as must not close the dialog');
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);

      // Attach reuses the normal file-attach flow (openFileCreate) -- the main
      // edit modal must open with the recording as its video preview, and
      // #rec-modal itself must close first.
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw4.mp4' });
      await sleep(30);
      document.getElementById('rec-modal-yes').click();
      await sleep(50);
      if (!recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: Attach did not close the recording dialog');
      }
      const editModal = document.getElementById('modal');
      const editVideo = document.getElementById('modal-video');
      if (editModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: Attach did not open the edit modal');
      } else if (editVideo.classList.contains('hidden') || !editVideo.src) {
        out.errors.push('rec-modal: Attach did not show the recording as a video preview');
      }
      document.getElementById('modal-close-x').click();
      await sleep(30);

      // The "window size" canvas default's reactive notice is a toast: it names the
      // reported second, its button writes recCanvasMode to "all", the result menu
      // opens without waiting for it, and the expert flag recCropNotice silences it.
      const confirmModal = document.getElementById('confirm-modal');
      const cropToast = document.getElementById('toast');
      if (!confirmModal.classList.contains('hidden')) {
        // closing the edit modal above left its discard prompt open
        document.getElementById('confirm-ok').click();
        await sleep(30);
      }
      window.__SMOKE_ARGS__ = [];
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw5-cropnotice.mp4', resize_crop_at_ms: 4200 });
      await sleep(30);
      if (!confirmModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: resize-crop notice must be a toast, not a dialog');
      }
      const cropBtn = cropToast.querySelector('.toast-btn');
      if (!cropBtn) {
        out.errors.push('rec-modal: resize-crop toast did not appear for resize_crop_at_ms');
      } else {
        if (!cropToast.textContent.includes('4')) {
          out.errors.push('rec-modal: resize-crop toast did not mention the reported second');
        }
        cropBtn.click();
        await sleep(30);
        const canvasCall = window.__SMOKE_ARGS__.find(([cmd, args]) => cmd === 'set_ui_text' && args && args.key === 'recCanvasMode');
        if (!canvasCall || canvasCall[1].value !== 'all') {
          out.errors.push('rec-modal: the resize-crop toast button did not set recCanvasMode to all');
        }
      }
      if (recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: the result menu did not open next to the resize-crop toast');
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);

      // With recCropNotice off the toast stays away, the result menu still opens.
      await click('expert-open');
      let cropRow = null;
      for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
        tb.click();
        cropRow = [...document.querySelectorAll('#expert-flags .switch-field')]
          .find(r => (r.querySelector('span') || {}).textContent === 'Notice when a recording gets cut off');
        if (cropRow) break;
      }
      if (!cropRow) {
        out.errors.push('rec-modal: the recCropNotice flag row was not found in the expert menu');
      } else {
        const cropCb = cropRow.querySelector('input.switch');
        cropCb.checked = false;
        cropCb.dispatchEvent(new Event('change'));
        await sleep(150);
      }
      await click('expert-back');
      await sleep(50);
      document.getElementById('toast').textContent = '';
      window.__FIRE__('recording-captured', { path: 'C:/fake/raw6-cropnotice-off.mp4', resize_crop_at_ms: 1000 });
      await sleep(30);
      if (cropToast.querySelector('.toast-btn')) {
        out.errors.push('rec-modal: resize-crop toast must stay away while recCropNotice is off');
      }
      if (recModal.classList.contains('hidden')) {
        out.errors.push('rec-modal: the result menu must still open once the notice is suppressed');
      }
      document.getElementById('rec-modal-close').click();
      await sleep(30);

      langSel3.value = 'de';
      langSel3.dispatchEvent(new Event('change'));
      await sleep(100);
    }
    // OCR ships on (flag "ocr"). Gate check first (both entry points visible,
    // and gone once the flag is switched off), then switch it back on via the
    // real expert-menu checkbox and exercise the menu plus the result dialog.
    {
      const ocrBtn = document.getElementById('ocr-btn');
      const snipModalOcrBtn = document.getElementById('snip-edit-ocr');
      const hiddenNow = (el) => !el || getComputedStyle(el).display === 'none';
      if (hiddenNow(ocrBtn) || hiddenNow(snipModalOcrBtn)) {
        out.errors.push('OCR entry points are hidden while the ocr flag is on');
      }
      const rowByLabel = (text) => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('#expert-flags .switch-field')]
            .find(r => (r.querySelector('span') || {}).textContent === text);
          if (hit) return hit;
        }
        return null;
      };
      const toggleOcrFlag = async (on) => {
        await click('expert-open');
        const row = rowByLabel('Text erkennen (OCR)');
        if (!row) { out.errors.push('OCR flag row not found in the expert menu'); return; }
        const cb = row.querySelector('input.switch');
        cb.checked = on;
        cb.dispatchEvent(new Event('change'));
        await sleep(150);
        await click('expert-back');
        await sleep(50);
      };
      if (!ocrBtn.title.trim() || /^[a-z]+[A-Z]/.test(ocrBtn.title)) {
        out.errors.push('OCR composer button has no real title (raw i18n key or empty)');
      }
      ocrBtn.click();
      await sleep(100);
      const menu = document.querySelector('.tab-menu:not(.hidden)');
      const menuItems = menu ? [...menu.querySelectorAll('button[role=menuitem]')] : [];
      if (menuItems.length !== 2) {
        out.errors.push('OCR menu: expected 2 entries, found ' + menuItems.length);
      } else if (menuItems[1].textContent.trim() !== 'Screenshot') {
        // The screenshot entry belongs at the bottom of the menu.
        out.errors.push('OCR menu: last entry is ' + JSON.stringify(menuItems[1].textContent.trim()) + ', expected the screenshot one');
      }
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      await sleep(50);
      const injected = '<b>bold</b> and a tag look-alike </p>';
      window.__FIRE__('ocr-captured', { text: injected, lang: 'en' });
      await sleep(150);
      const ocrModal = document.getElementById('ocr-modal');
      if (!ocrModal || ocrModal.classList.contains('hidden')) {
        out.errors.push('OCR result dialog did not open on ocr-captured');
      } else {
        const resultBox = document.getElementById('ocr-result-text');
        if (resultBox.value !== injected) {
          out.errors.push('OCR result textarea does not hold the recognized text verbatim');
        }
        if (document.querySelector('#ocr-modal b')) {
          out.errors.push('OCR result rendered markup instead of literal text');
        }
        layoutScan('OCR result dialog');
        document.getElementById('ocr-modal-close').click();
        await sleep(100);
        if (!ocrModal.classList.contains('hidden')) {
          out.errors.push('OCR result dialog did not close');
        }
      }
      // 509: an image whose recognized lines are all empty must show a clear
      // toast instead of opening the result dialog with nothing in it.
      {
        const toastEl509 = document.getElementById('toast');
        window.__FIRE__('ocr-captured', { text: '\\n\\n', lang: 'en' });
        await sleep(150);
        if (!ocrModal.classList.contains('hidden')) {
          out.errors.push('509: an all-blank OCR result opened the result dialog instead of a toast');
        }
        if (!toastEl509.classList.contains('show') || toastEl509.classList.contains('hidden')) {
          out.errors.push('509: no toast appeared for an all-blank OCR result');
        } else {
          const manageBtn = toastEl509.querySelector('.toast-btn');
          if (!manageBtn || !manageBtn.textContent.trim()) {
            out.errors.push('509: the no-text-found toast is missing its manage-languages button');
          }
          const msgEl = toastEl509.querySelector('.toast-msg');
          if (!msgEl || !msgEl.textContent.trim()) {
            out.errors.push('509: the no-text-found toast has no message text');
          }
        }
        toastEl509.classList.remove('show'); // don't linger into later blocks
        await sleep(250);
      }
      await toggleOcrFlag(false);
      if (!hiddenNow(ocrBtn) || !hiddenNow(snipModalOcrBtn)) {
        out.errors.push('OCR entry points stayed visible after switching the flag off');
      }
      // Back to the shipped state -- later probes (the tour's step 5) read the
      // live flags and would otherwise see a value this probe left behind.
      await toggleOcrFlag(true);
      if (hiddenNow(ocrBtn) || hiddenNow(snipModalOcrBtn)) {
        out.errors.push('OCR entry points stayed hidden after switching the flag back on');
      }
    }
    // OCR language-pack management dialog: list renders, size sits inside the
    // download button, buttons are named, no overflow at 520px.
    {
      const rowByLabel = (text) => {
        for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
          tb.click();
          const hit = [...document.querySelectorAll('#expert-flags .switch-field, #expert-flags .expert-action')]
            .find((r) => {
              const span = r.tagName === 'BUTTON' ? r : r.querySelector('span');
              return span && span.textContent === text;
            });
          if (hit) return hit;
        }
        return null;
      };
      const toggleOcrFlag = async (on) => {
        await click('expert-open');
        const row = rowByLabel('Text erkennen (OCR)');
        if (!row) { out.errors.push('OCR flag row not found for the langs-modal probe'); return; }
        const cb = row.querySelector('input.switch');
        cb.checked = on;
        cb.dispatchEvent(new Event('change'));
        await sleep(150);
      };
      await toggleOcrFlag(true);

      const FAKE_CAPS = [
        { name: 'Language.OCR~~~de-DE~0.0.1.0', tag: 'de-DE', label: 'German', state: 'Installed', version: '0.0.1.0', download_size: null, install_size: 45000000 },
        { name: 'Language.OCR~~~fr-FR~0.0.1.0', tag: 'fr-FR', label: 'French', state: 'NotPresent', version: '0.0.1.0', download_size: 13000000, install_size: null },
        { name: 'Language.OCR~~~ja-JP~0.0.1.0', tag: 'ja-JP', label: 'Japanese', state: 'NotPresent', version: '0.0.1.0', download_size: null, install_size: null },
      ];
      // main.js captures `invoke` once at module load (const { invoke } = ...),
      // so overriding window.__TAURI__.core.invoke this late would have no
      // effect -- tauri_stub.js's own ocr_capabilities entry reads this global
      // fresh on every call instead.
      window.__OCR_CAPS_TEST__ = FAKE_CAPS;

      // toggleOcrFlag just left the OCR tab active, so search only the current
      // tab instead of restarting the tab walk from the first one.
      const langsRow = [...document.querySelectorAll('#expert-flags .expert-action')]
        .find((r) => (r.tagName === 'BUTTON' ? r : r.querySelector('span'))?.textContent === 'Sprachen der Texterkennung verwalten…');
      if (!langsRow) {
        out.errors.push('OCR langs-modal action button not found in the expert menu');
      } else {
        langsRow.click();
        await sleep(150);
        const modal = document.getElementById('ocr-langs-modal');
        if (!modal || modal.classList.contains('hidden')) {
          out.errors.push('OCR langs modal did not open from the expert action button');
        } else {
          const rows = [...document.querySelectorAll('#ocr-langs-body .ocr-lang-row')];
          if (rows.length !== FAKE_CAPS.length) {
            out.errors.push('OCR langs modal: expected ' + FAKE_CAPS.length + ' rows, found ' + rows.length);
          }
          // 237: the language name is the most prominent thing in the row, size
          // is its own right-aligned column (MB or a dash, never a guess), state
          // reads as a chip instead of being buried in a shared meta line.
          rows.forEach((row, i) => {
            const cap = FAKE_CAPS[i];
            const nameEl = row.querySelector('.lang-row-name');
            if (!nameEl || nameEl.textContent !== cap.label) {
              out.errors.push('OCR langs modal: row ' + i + ' name is not "' + cap.label + '"');
            }
            const sizeEl = row.querySelector('.lang-row-size');
            const expectSize = (cap.state === 'Installed' ? cap.install_size : cap.download_size) == null ? '\u2013' : 'MB';
            if (!sizeEl || !sizeEl.textContent.includes(expectSize)) {
              out.errors.push('OCR langs modal: row ' + i + ' size column does not show "' + expectSize + '"');
            }
            const chip = row.querySelector('.lang-row-chip');
            if (!chip || !chip.textContent.trim()) {
              out.errors.push('OCR langs modal: row ' + i + ' has no state chip');
            } else if (cap.state !== 'Installed') {
              // 230: a not-yet-installed row used to show the raw Rust-internal
              // state string ("NotPresent") verbatim instead of a translated one.
              // main.js is a module script (its own scope, t() unreachable from
              // here) and this suite runs under whichever UI language the test
              // machine resolves to (not necessarily English), so this cannot
              // compare against one hardcoded translated string -- it checks the
              // one language-independent property the bug actually broke: the
              // chip must not be the raw backend enum value verbatim.
              if (chip.textContent === cap.state) {
                out.errors.push('OCR langs modal: row ' + i + ' chip shows the raw backend state "' + cap.state + '" instead of a translated string');
              }
            }
          });
          const buttons = [...document.querySelectorAll('#ocr-langs-body .ghost-btn')];
          if (buttons.length !== 2) {
            out.errors.push('OCR langs modal: expected 2 download buttons, found ' + buttons.length);
          }
          for (const b of buttons) {
            if (!b.textContent.trim()) out.errors.push('OCR langs modal: a download button has no label');
          }
          if (!buttons.some((b) => /12[.,]4 MB/.test(b.textContent))) {
            out.errors.push('OCR langs modal: download button does not show the size in its own label');
          }
          if (!buttons.some((b) => b.textContent.includes('\u2013'))) {
            out.errors.push('OCR langs modal: a missing size did not render as a dash');
          }
          layoutScan('OCR langs modal');
          document.getElementById('ocr-langs-close-x').click();
          await sleep(100);
          if (!modal.classList.contains('hidden')) out.errors.push('OCR langs modal did not close');
        }
      }
      // 230: a second click on a language install button during the gap between
      // confirming the elevate dialog and the busy flag being set must not start
      // a second concurrent elevated install -- installPending[name] is now set
      // synchronously right after the confirm resolves, before invoke() ever
      // starts. The gate holds the elevated install() call itself pending (the
      // real one blocks for as long as the user takes on the UAC prompt, easily
      // seconds), so a genuinely separate click landing during that whole window
      // must find an already-disabled button, not just a narrow instant after
      // the confirm.
      if (langsRow) {
        langsRow.click();
        await sleep(150);
        const rows3 = [...document.querySelectorAll('#ocr-langs-body .ocr-lang-row')];
        const notInstalledRow = rows3.find((r) => !r.querySelector('.lang-row-chip.installed'));
        const installBtn = notInstalledRow && notInstalledRow.querySelector('.ghost-btn');
        const rowLabel = notInstalledRow && notInstalledRow.querySelector('.lang-row-name')?.textContent;
        if (!installBtn) {
          out.errors.push('230: no install button on a not-installed row for the double-click test');
        } else {
          window.__SMOKE_CALLS__ = [];
          window.__OCR_INSTALL_GATE__ = [];
          installBtn.click();
          await sleep(100);
          const okBtn2 = document.getElementById('confirm-ok');
          if (!okBtn2 || okBtn2.offsetParent === null) {
            out.errors.push('230: the elevate confirm dialog did not open on clicking install');
          } else {
            okBtn2.click();
            await sleep(50);
            if ((window.__OCR_INSTALL_GATE__ || []).length !== 1) {
              out.errors.push('230: expected exactly 1 gated ocr_install_language call, got ' + (window.__OCR_INSTALL_GATE__ || []).length);
            }
            const rebuiltRow = [...document.querySelectorAll('#ocr-langs-body .ocr-lang-row')]
              .find((r) => r.querySelector('.lang-row-name')?.textContent === rowLabel);
            const rebuiltBtn = rebuiltRow && rebuiltRow.querySelector('.ghost-btn');
            if (!rebuiltBtn || !rebuiltBtn.disabled) {
              out.errors.push('230: the install button did not disable itself while the elevated install call is still pending');
            }
            // A no-op on a disabled button -- proves the guard rather than just
            // documenting it, matching a real second click while Windows still
            // has not answered the first elevated install.
            rebuiltBtn && rebuiltBtn.click();
            await sleep(50);
            // Let the gated install call finally resolve so the dialog does not
            // leave a stuck poller behind for later probes.
            for (const g of window.__OCR_INSTALL_GATE__ || []) g.resolve(null);
            await sleep(50);
            const installCalls = (window.__SMOKE_CALLS__ || []).filter((c) => c === 'ocr_install_language');
            if (installCalls.length !== 1) {
              out.errors.push('230: expected exactly 1 ocr_install_language call after a rapid second click, got ' + installCalls.length);
            }
          }
        }
        window.__OCR_INSTALL_GATE__ = null;
        // The install above counts as still running across the close, and the
        // reopened dialog polls its result again: keep that result pending so the
        // elevated-listing check below is not interrupted by a finished install's
        // reload.
        RESULTS.ocr_install_result = () => null;
        document.getElementById('ocr-langs-close-x').click();
        await sleep(100);
      }
      // Windows refusing the read-only listing: an honest note plus a button that
      // re-runs the very same query elevated -- without it no language can be
      // added at all, because the list IS the install allowlist.
      window.__OCR_CAPS_ELEV__ = true;
      window.__OCR_CAPS_ELEV_DONE__ = false;
      if (langsRow) {
        langsRow.click();
        await sleep(150);
        const modal = document.getElementById('ocr-langs-modal');
        const elevBtn = [...document.querySelectorAll('#ocr-langs-body .ghost-btn')][0];
        if (!elevBtn || !elevBtn.textContent.trim()) {
          out.errors.push('OCR langs modal: no elevation button when Windows refuses the listing');
        } else {
          // 177: the raw backend reason rides along, shown collapsed.
          const details = document.querySelector('#ocr-langs-body .lang-error-details-text');
          if (!details || !details.textContent.includes('DISM')) {
            out.errors.push('OCR langs modal: the raw elevation-error text is missing from the details disclosure');
          }
          layoutScan('OCR langs modal (needs elevation)');
          elevBtn.click();
          await sleep(100);
          const okBtn = document.getElementById('confirm-ok');
          if (!okBtn || okBtn.offsetParent === null) {
            out.errors.push('OCR langs modal: elevation button did not explain itself first');
          } else {
            okBtn.click();
            await sleep(3000); // the poller re-asks every 2 s
            const rows2 = [...document.querySelectorAll('#ocr-langs-body .ocr-lang-row')];
            if (rows2.length !== FAKE_CAPS.length) {
              out.errors.push('OCR langs modal: elevated listing produced ' + rows2.length + ' rows, expected ' + FAKE_CAPS.length);
            }
          }
        }
        document.getElementById('ocr-langs-close-x').click();
        await sleep(100);
        if (modal && !modal.classList.contains('hidden')) out.errors.push('OCR langs modal did not close after the elevated listing');
      }
      window.__OCR_CAPS_ELEV__ = false;
      window.__OCR_CAPS_TEST__ = null;
      await toggleOcrFlag(true); // leave the flag in its shipped state
    }
    document.getElementById('settings').classList.add('hidden');
    await sleep(150);
"""
