SECTION = """\
    // 112: walk the same views the 130% pass just covered once per wordy language.
    // Polish, Romanian and German write the longest labels of the eleven, and a
    // clipped label or an overlapping box is exactly what layoutScan already
    // catches -- this just points it at real long strings instead of a synthetic
    // scale. German comes last on purpose: the OCR checks below find their expert
    // rows by German label, so that is the language the page has to be left in.
    document.getElementById('settings-expert').classList.add('hidden');
    document.getElementById('settings-main').classList.remove('hidden');
    document.getElementById('settings').classList.remove('hidden');
    const langSel = document.getElementById('lang-select');
    for (const lang of ['pl', 'ro', 'de']) {
      langSel.value = lang;
      langSel.dispatchEvent(new Event('change'));
      await sleep(300);
      if (document.documentElement.lang !== lang) {
        out.errors.push('lang-select change did not switch the active language to ' + lang +
          ' (still ' + document.documentElement.lang + ')');
      }
      layoutScan('main window in ' + lang);
      mustReach('main window in ' + lang, '.win-btn, .composer > button');
      await click('library-btn');
      layoutScan('library in ' + lang);
      // A string built once in script and never refreshed stays in the old
      // language after a live switch: the library's manage-tags button, the
      // maximize button title and the clear-field buttons.
      {
        const wantTag = document.querySelector('#tag-modal [data-i18n="tagManager"]').textContent;
        const tagBtns = [...document.querySelectorAll('#library-tagbar .tag-action')];
        const manageBtn = tagBtns[tagBtns.length - 1];
        if (!manageBtn) out.errors.push('lang switch: no manage-tags button in the library tag bar in ' + lang);
        else if (manageBtn.textContent !== wantTag) {
          out.errors.push('lang switch: manage-tags button reads "' + manageBtn.textContent + '" in ' + lang + ', expected "' + wantTag + '"');
        }
        const winMax = document.getElementById('win-max');
        const wantMax = document.getElementById('win-max').getAttribute('aria-label');
        if (document.body.classList.contains('win-maximized') === false && winMax.title !== wantMax) {
          out.errors.push('lang switch: maximize button title is "' + winMax.title + '" in ' + lang + ', expected "' + wantMax + '"');
        }
        const clearLabels = new Set([...document.querySelectorAll('.clear-btn')].map((b) => b.getAttribute('aria-label')));
        if (clearLabels.size !== 1) {
          out.errors.push('lang switch: clear-field buttons carry ' + clearLabels.size + ' different labels in ' + lang + ': ' + [...clearLabels].join(' | '));
        } else if (window.__PREV_CLEAR_LABEL__ === [...clearLabels][0]) {
          out.errors.push('lang switch: clear-field label did not change on the switch to ' + lang);
        }
        window.__PREV_CLEAR_LABEL__ = [...clearLabels][0];
        // The version status lines carry a live value and are redrawn from
        // their remembered key, so both read the same in the new language.
        const uvEl = document.getElementById('update-version'), avEl = document.getElementById('about-version');
        if (avEl.textContent && !uvEl.classList.contains('update-avail') && uvEl.textContent !== avEl.textContent) {
          out.errors.push('lang switch: update status line reads "' + uvEl.textContent + '" but About reads "' + avEl.textContent + '" in ' + lang);
        }
        if (!avEl.textContent) out.errors.push('lang switch: About version line is empty in ' + lang);
        if (window.__PREV_VER__ !== undefined && window.__PREV_VER__ === avEl.textContent) {
          out.errors.push('lang switch: About version line did not change on the switch to ' + lang);
        }
        window.__PREV_VER__ = avEl.textContent;
      }
      // 706: the composer placeholder is short and names no shortcut; the accessible name and
      // tooltip carry it. The editor field keeps the plain one.
      {
        const inp706 = document.getElementById('input');
        const ph706 = inp706.placeholder;
        const aria706 = inp706.getAttribute('aria-label') || '';
        const edPh706 = document.getElementById('modal-text').placeholder;
        if (ph706.includes('{keys}') || ph706.includes('Enter')) out.errors.push('706: the composer placeholder in ' + lang + ' reads "' + ph706 + '"');
        if (!/(Ctrl|Strg)\\+Enter/.test(aria706) || aria706.includes('{keys}')) out.errors.push('706: the composer accessible name in ' + lang + ' reads "' + aria706 + '"');
        if (inp706.title !== aria706) out.errors.push('706: the composer tooltip differs from its accessible name in ' + lang);
        if (edPh706.includes('Enter')) out.errors.push('706: the editor text field placeholder in ' + lang + ' names a shortcut: ' + edPh706);
        if (lang === 'de') {
          if (ph706 !== 'Neuer Prompt…') out.errors.push('706: the German composer placeholder reads "' + ph706 + '"');
          if (edPh706 !== 'Prompt eingeben…') out.errors.push('706: the German editor placeholder reads "' + edPh706 + '"');
        }
        // ph253: an empty composer stays one line in a small window.
        if (window.innerWidth <= 700 && !inp706.value && inp706.getBoundingClientRect().height > 44) {
          out.errors.push('ph253: the empty composer is ' + Math.round(inp706.getBoundingClientRect().height) + 'px tall at ' + window.innerWidth + 'px width in ' + lang);
        }
      }
      await click('library-close');
      layoutScan('settings in ' + lang);
      mustReach('settings in ' + lang, '#settings-main button, #settings-main select');
      // 521: the shortened German label keeps the folder-import button's own
      // text on one line -- a flex row with align-items stretch equalizes
      // every button's box height regardless, so the box height alone (the
      // first thing tried here) cannot tell a wrapped label from a plain one;
      // only the text's own line count can.
      if (lang === 'de') {
        const folderBtn521 = document.getElementById('import-folder-btn');
        if (!folderBtn521) {
          out.errors.push('521: could not find the folder-import button in de');
        } else {
          const label521 = [...folderBtn521.childNodes].find((n) => n.nodeType === 3 && n.textContent.trim());
          if (!label521) {
            out.errors.push('521: the folder-import button has no text label to measure in de');
          } else {
            const range521 = document.createRange();
            range521.selectNodeContents(label521);
            const tops521 = new Set([...range521.getClientRects()].map((r) => Math.round(r.top)));
            if (tops521.size > 1) {
              out.errors.push('521: the folder-import button label wraps to ' + tops521.size + ' lines in de');
            }
          }
        }
      }
      // 516: the redesigned About dialog in the longest-label languages.
      document.getElementById('about-btn').click();
      await sleep(150);
      if (window.__ABOUT_FITS_CHECK__) window.__ABOUT_FITS_CHECK__(lang);
      document.getElementById('about-ok').click();
      await sleep(120);
      await click('expert-open');
      layoutScan('expert menu in ' + lang);
      await click('expert-back');
    }
    // 117/118/119: screenshot editor -- window.__FIRE__ is the same hook every other
    // probe uses to exercise a backend-started flow (see tauri_stub.js), here firing
    // the snip-captured event finalize_capture would normally send.
    {
      const TINY_PNG = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
      window.__FIRE__('snip-captured', { data_url: TINY_PNG, path: '', name: 'test.png' });
      await sleep(300);
      const edit = document.getElementById('snip-edit');
      if (!edit || edit.classList.contains('hidden')) {
        out.errors.push('screenshot editor did not open on snip-captured');
      } else {
        const toolBtns = [...document.querySelectorAll('#snip-edit-tools button[data-tool]')];
        if (toolBtns.length !== 12) { // 284 added the Hand (pan) tool
          out.errors.push('screenshot editor: expected 12 tool buttons, found ' + toolBtns.length);
        }
        // 178: the toolbar must stay a couple of rows tall no matter which tool
        // is active -- a full inline swatch grid used to collapse into one tall
        // column here and blow the toolbar height up. This is the check that
        // would have caught that bug; a generic layoutScan alone did not.
        // 507 raised the ceiling from 96 to 200: the context row now wraps
        // its fields onto a second (and for the busiest tool, a third) line
        // instead of scrolling them sideways, so two or three lines of real
        // controls is expected height, not a regression. A collapsed swatch
        // grid is still caught well below 200 -- that failure mode stacks a
        // whole column of swatches, not two or three short field rows.
        const seToolbar = document.querySelector('.snip-edit-toolbar');
        const SE_TOOLBAR_CEIL = 200;
        for (const b of toolBtns) {
          b.click();
          await sleep(30);
          if (b.getAttribute('aria-pressed') !== 'true') {
            out.errors.push('screenshot editor: ' + b.dataset.tool + ' did not mark aria-pressed after click');
          }
          const pressedCount = toolBtns.filter((x) => x.getAttribute('aria-pressed') === 'true').length;
          if (pressedCount !== 1) {
            out.errors.push('screenshot editor: ' + pressedCount + ' tools show aria-pressed at once (expected exactly 1)');
          }
          if (seToolbar) {
            const h = seToolbar.getBoundingClientRect().height;
            if (h > SE_TOOLBAR_CEIL) {
              out.errors.push('screenshot editor toolbar is ' + Math.round(h) + 'px tall with the ' +
                b.dataset.tool + ' tool active, over the ' + SE_TOOLBAR_CEIL + 'px ceiling');
            }
          }
        }
        const penBtn = toolBtns.find((b) => b.dataset.tool === 'pen');
        if (penBtn) { penBtn.click(); await sleep(30); }
        // The pen tool has a color field: a single trigger button opening a
        // popover, not an inline grid, is what keeps the toolbar row short.
        const colorTrigger = document.querySelector('.snip-edit-color-trigger');
        if (!colorTrigger) {
          out.errors.push('screenshot editor: pen tool has no color trigger button');
        } else {
          colorTrigger.click();
          await sleep(30);
          const pop = document.querySelector('.snip-color-pop');
          if (!pop || pop.classList.contains('hidden')) {
            out.errors.push('screenshot editor: the color trigger did not open the palette popover');
          } else {
            if (seToolbar && seToolbar.getBoundingClientRect().height > SE_TOOLBAR_CEIL) {
              out.errors.push('screenshot editor toolbar grew past ' + SE_TOOLBAR_CEIL + 'px with the color popover open');
            }
            const popSwatches = pop.querySelectorAll('.swatch');
            if (popSwatches.length < 3) {
              out.errors.push('screenshot editor: the color popover has only ' + popSwatches.length + ' swatches');
            }
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
            await sleep(30);
            if (!pop.classList.contains('hidden')) {
              out.errors.push('screenshot editor: Escape did not close the color popover');
            }
            if (document.getElementById('snip-edit').classList.contains('hidden')) {
              out.errors.push('screenshot editor: Escape closed the whole editor instead of just the color popover');
            }
            document.querySelector('.snip-edit-color-trigger').click();
            await sleep(30);
            const pick = [...document.querySelectorAll('.snip-color-pop .swatch')]
              .find((s) => !s.classList.contains('none') && !s.classList.contains('custom'));
            if (pick) {
              pick.click();
              await sleep(30);
              const chip = document.querySelector('.snip-edit-color-chip');
              if (!chip || !chip.style.background) {
                out.errors.push('screenshot editor: picking a swatch did not color the trigger chip');
              }
              if (!document.querySelector('.snip-color-pop').classList.contains('hidden')) {
                out.errors.push('screenshot editor: picking a swatch did not close the color popover');
              }
            }
          }
        }
        layoutScan('screenshot editor');
        const live = document.getElementById('snip-edit-live');
        const undoBtn = document.getElementById('snip-edit-undo');
        if (live && undoBtn) {
          const r = live.getBoundingClientRect();
          const x = r.left + r.width / 2, y = r.top + r.height / 2;
          live.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: x, clientY: y }));
          live.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: x + 5, clientY: y + 5 }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: x + 5, clientY: y + 5 }));
          await sleep(100);
          if (undoBtn.disabled) out.errors.push('screenshot editor: undo stayed disabled after a drawn stroke');
          undoBtn.click();
          await sleep(100);
          if (!undoBtn.disabled) out.errors.push('screenshot editor: undo stayed enabled after undoing the only stroke');
        }
        {
          const shortcutsBtn = document.getElementById('snip-edit-shortcuts');
          const shortcutsModal = document.getElementById('shortcuts-modal');
          if (!shortcutsBtn || !onScreen(shortcutsBtn) || !canHit(shortcutsBtn)) {
            out.errors.push('screenshot editor: shortcuts overview button is missing, hidden, or not clickable');
          } else {
            shortcutsBtn.click();
            await sleep(50);
            if (shortcutsModal.classList.contains('hidden')) {
              out.errors.push('screenshot editor: shortcuts overview did not open');
            } else {
              const rows = shortcutsModal.querySelectorAll('.shortcuts-row');
              if (rows.length === 0) out.errors.push('screenshot editor: shortcuts overview opened with no rows');
              document.getElementById('shortcuts-modal-close').click();
              await sleep(50);
              if (!shortcutsModal.classList.contains('hidden')) {
                out.errors.push('screenshot editor: shortcuts overview did not close');
              }
            }
            // 519: the opener button can be gone by the time the overlay closes
            // (removed from the page while the overlay was still open) --
            // closeShortcutsModal must not throw trying to focus it, and focus
            // must land somewhere still in the document instead of nowhere.
            shortcutsBtn.click();
            await sleep(50);
            shortcutsBtn.remove();
            let threw = false;
            try { document.getElementById('shortcuts-modal-close').click(); } catch (e) { threw = true; }
            await sleep(50);
            if (threw) out.errors.push('519: closing the shortcuts overview threw with its opener removed from the page');
            if (!document.activeElement || !document.activeElement.isConnected) {
              out.errors.push('519: focus was left on a detached element after closing the shortcuts overview');
            }
          }
        }
        // 179: the overlay's own titlebar (move) and resize grip (resize) -- the
        // overlay covers the whole frameless main window, so without these there
        // is no way to move or resize it while it is open.
        const titlebar = document.querySelector('.snip-edit-titlebar');
        if (!titlebar || !onScreen(titlebar) || titlebar.getAttribute('data-tauri-drag-region') === null) {
          out.errors.push('screenshot editor: titlebar is missing or not a drag region');
        }
        const closeInBar = titlebar && titlebar.querySelector('#snip-edit-close');
        if (!closeInBar || closeInBar.getAttribute('data-tauri-drag-region') !== 'false') {
          out.errors.push('screenshot editor: close button must opt out of the titlebar drag region');
        }
        const grip = document.getElementById('snip-edit-resize-grip');
        if (!grip || !onScreen(grip) || !canHit(grip)) {
          out.errors.push('screenshot editor: resize grip is missing, hidden, or not clickable');
        } else {
          // A stub-only check: it proves the handler runs against the same
          // window.__TAURI__.window.ResizeDirection shape the real Tauri v2 API
          // exposes, not that the real WebView2 window actually resizes -- that
          // needs a real window and a person at the mouse, same as moving it.
          grip.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1 }));
          await sleep(30);
        }
        // 196: Save as reuses the same command the screenshot result dialog's own
        // Save as button calls -- must not close the editor (Copy/Cancel/Apply do).
        const saveAsBtn = document.getElementById('snip-edit-saveas');
        if (!saveAsBtn || !onScreen(saveAsBtn) || !canHit(saveAsBtn)) {
          out.errors.push('screenshot editor: Save as button is missing, hidden, or not clickable');
        } else {
          window.__SMOKE_CALLS__ = [];
          saveAsBtn.click();
          await sleep(150);
          if (!(window.__SMOKE_CALLS__ || []).includes('save_screenshot_as')) {
            out.errors.push('screenshot editor: Save as did not call save_screenshot_as');
          }
          if (document.getElementById('snip-edit').classList.contains('hidden')) {
            out.errors.push('screenshot editor: Save as closed the editor');
          }
        }
        // 181: Tab/Shift+Tab must wrap inside the overlay instead of running the
        // browser default, which would reach the grid/toolbar hidden underneath
        // (aria-modal is only a hint for assistive tech, not an actual trap).
        {
          const overlay = document.getElementById('snip-edit');
          const focusables = [...overlay.querySelectorAll('button, input, [tabindex]:not([tabindex="-1"]), select, textarea, a[href]')]
            .filter((el) => !el.disabled && el.offsetParent !== null);
          if (focusables.length > 1) {
            const first = focusables[0], last = focusables[focusables.length - 1];
            last.focus();
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));
            if (document.activeElement !== first) {
              out.errors.push('screenshot editor: Tab from the last control did not wrap to the first');
            }
            first.focus();
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true }));
            if (document.activeElement !== last) {
              out.errors.push('screenshot editor: Shift+Tab from the first control did not wrap to the last');
            }
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
        if (!document.getElementById('snip-edit').classList.contains('hidden')) {
          out.errors.push('screenshot editor did not close on Cancel');
        }
      }
      // 363: an image that fails to decode (truncated data, a broken picked
      // file) must not leave the editor silently never opening -- it shows an
      // error toast and leaves things as if the capture had never happened.
      {
        const BROKEN_PNG = 'data:image/png;base64,bm90LWEtcmVhbC1pbWFnZQ==';
        document.getElementById('toast').classList.add('hidden');
        window.__FIRE__('snip-captured', { data_url: BROKEN_PNG, path: '', name: 'broken.png' });
        await sleep(300);
        if (!document.getElementById('snip-edit').classList.contains('hidden')) {
          out.errors.push('363: a broken screenshot left the editor open instead of showing an error');
        }
        const brokenToast = document.getElementById('toast');
        if (brokenToast.classList.contains('hidden') || !brokenToast.textContent) {
          out.errors.push('363: a broken screenshot did not show an error toast');
        }
      }
      // 363: the marker tool's offscreen canvas must be reused across redraws
      // instead of reallocated every time -- cost used to scale with stroke
      // count times image resolution, since every redraw (every new op, every
      // undo/redo, every nudge, every tool switch) reallocated a full-size
      // canvas per marker stroke it painted.
      {
        const TINY_PNG2 = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
        window.__FIRE__('snip-captured', { data_url: TINY_PNG2, path: '', name: 'marker.png' });
        await sleep(300);
        const markerBtn = document.querySelector('#snip-edit-tools button[data-tool="marker"]');
        const penBtn2 = document.querySelector('#snip-edit-tools button[data-tool="pen"]');
        const live2 = document.getElementById('snip-edit-live');
        if (markerBtn && penBtn2 && live2) {
          markerBtn.click();
          await sleep(30);
          for (let i = 0; i < 3; i++) {
            const x = 10 + i * 5, y = 10 + i * 5;
            live2.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: x, clientY: y }));
            live2.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: x + 5, clientY: y + 5 }));
            window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: x + 5, clientY: y + 5 }));
            await sleep(30);
          }
          // The marker path's offscreen scratch canvas is never attached to the
          // document (it exists only to be drawn from, then discarded or
          // reused), so counting document.getElementsByTagName('canvas') can
          // never see it either way -- count document.createElement('canvas')
          // calls instead, which is the actual allocation this fix removes.
          let canvasCreations = 0;
          const origCreateElement = document.createElement.bind(document);
          document.createElement = (tag, ...rest) => {
            if (String(tag).toLowerCase() === 'canvas') canvasCreations++;
            return origCreateElement(tag, ...rest);
          };
          try {
            const baseline = canvasCreations;
            for (let i = 0; i < 10; i++) {
              (i % 2 === 0 ? penBtn2 : markerBtn).click();
              await sleep(15);
            }
            if (canvasCreations > baseline) {
              out.errors.push('363: document.createElement("canvas") ran ' + (canvasCreations - baseline) +
                ' more time(s) across 10 redraws of 3 marker strokes -- the scratch canvas is being reallocated');
            }
          } finally {
            document.createElement = origCreateElement;
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // The shared mouse-only number field, exercised on the text tool's
      // text-size control (min 10, max 96, step 1, presets 12/16/24/32/48/64/96).
      {
        const TINY_PNG3 = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
        window.__FIRE__('snip-captured', { data_url: TINY_PNG3, path: '', name: 'numfield.png' });
        await sleep(300);
        const textBtn = document.querySelector('#snip-edit-tools button[data-tool="text"]');
        if (!textBtn) {
          out.errors.push('the text tool button was not found while testing the text-size number field');
        } else {
          textBtn.click();
          await sleep(30);
          const numfield = document.querySelector('#snip-edit-context .numfield');
          if (!numfield) {
            out.errors.push('the text-size number field was not found after selecting the text tool');
          } else {
            const input = numfield.querySelector('input');
            const inc = numfield.querySelector('.numfield-inc');
            const dec = numfield.querySelector('.numfield-dec');
            const list = numfield.querySelector('.numfield-list');
            const beforeClick = Number(input.value);
            inc.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 31 }));
            await sleep(10);
            inc.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 31 }));
            await sleep(10);
            const afterClick = Number(input.value);
            if (afterClick !== beforeClick + 1) {
              out.errors.push('clicking + did not step the text-size field by exactly 1');
            }
            inc.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 32 }));
            await sleep(480);
            inc.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 32 }));
            await sleep(10);
            const afterHold = Number(input.value);
            if (!(afterHold > afterClick + 1)) {
              out.errors.push('holding + did not repeat-step the text-size field past a single click');
            }
            input.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, deltaY: -100 }));
            await sleep(30);
            if (Number(input.value) !== afterHold + 1) {
              out.errors.push('scrolling up on the text-size field did not step it by exactly 1');
            }
            list.click();
            await sleep(30);
            const pop = document.querySelector('.numfield-pop');
            const target = pop && [...pop.querySelectorAll('button')].find((b) => b.textContent === '32');
            if (!target) {
              out.errors.push('the text-size presets popover did not offer 32');
            } else {
              target.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 33 }));
              await sleep(30);
              if (Number(input.value) !== 32) {
                out.errors.push('picking 32 from the text-size presets popover did not set the field to 32');
              }
            }
            input.value = input.min;
            input.dispatchEvent(new Event('change', { bubbles: true }));
            await sleep(10);
            if (!dec.disabled) {
              out.errors.push('the - button is not disabled once the text-size field is at its minimum');
            }
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // 394: the text tool's background is now three choices (none, plate,
      // outline) instead of a plain switch. A previously stored
      // snipTextPlate: false (see the stub's initial ui_flags) must arrive as
      // "none" on its own, and picking Outline must reveal its own width field.
      {
        const dropdownRowByLabel = (text) => {
          for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
            tb.click();
            const hit = [...document.querySelectorAll('#expert-flags .expert-dropdown-row')]
              .find((r) => (r.querySelector('.value-head span') || {}).textContent === text);
            if (hit) return hit;
          }
          return null;
        };
        document.getElementById('gear').click();
        await sleep(200);
        document.getElementById('expert-open').click();
        await sleep(300);
        // The suite is left running in German from the 112 pass above onward
        // (see its own comment), so the row is found by its German label.
        const row = dropdownRowByLabel('Texthintergrund');
        const sel = row && row.querySelector('select');
        if (!sel) {
          out.errors.push('394: the text background dropdown is missing from the expert menu');
        } else {
          const values = [...sel.options].map((o) => o.value);
          if (values.join(',') !== 'none,plate,outline') {
            out.errors.push('394: the text background dropdown does not offer exactly none/plate/outline, got ' + values.join(','));
          }
          if (sel.value !== 'none') {
            out.errors.push('394: a stored snipTextPlate:false did not migrate to "none", got "' + sel.value + '"');
          }
          sel.value = 'outline';
          sel.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(120);
          const wrote = [...(window.__SMOKE_ARGS__ || [])]
            .some(([c, a]) => c === 'set_ui_text' && a.key === 'snipTextBack' && a.value === 'outline');
          if (!wrote) out.errors.push('394: picking Outline in the expert menu did not persist snipTextBack');
        }
        document.getElementById('settings-close').click();
        await sleep(200);
        const TINY_PNG4 = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
        window.__FIRE__('snip-captured', { data_url: TINY_PNG4, path: '', name: 'textback.png' });
        await sleep(300);
        const textBtn2 = document.querySelector('#snip-edit-tools button[data-tool="text"]');
        if (!textBtn2) {
          out.errors.push('394: the text tool button was not found while testing the per-shape background field');
        } else {
          textBtn2.click();
          await sleep(30);
          let shapeSel = document.querySelector('#snip-edit-context select');
          if (!shapeSel) {
            out.errors.push('394: the per-shape text background select was not found');
          } else {
            const values2 = [...shapeSel.options].map((o) => o.value);
            if (values2.join(',') !== 'none,plate,outline') {
              out.errors.push('394: the per-shape text background select does not offer exactly none/plate/outline, got ' + values2.join(','));
            }
            shapeSel.value = 'none';
            shapeSel.dispatchEvent(new Event('change', { bubbles: true }));
            await sleep(30);
            if (document.querySelectorAll('#snip-edit-context .numfield').length !== 1) {
              out.errors.push('394: an outline-width field is shown while the background is set to None');
            }
            // The panel was just rebuilt from scratch (innerHTML = ""), so the
            // old select node is detached -- re-query the live one.
            shapeSel = document.querySelector('#snip-edit-context select');
            shapeSel.value = 'outline';
            shapeSel.dispatchEvent(new Event('change', { bubbles: true }));
            await sleep(30);
            const numfields = document.querySelectorAll('#snip-edit-context .numfield input');
            if (numfields.length !== 2 || Number(numfields[1].value) !== 3) {
              out.errors.push('394: choosing Outline in the per-shape panel did not reveal a width field defaulting to 3px');
            }
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // 342: scaling the image itself (not just the view). A filled rect is
      // drawn before the scale so the test can check that the SAME drawn
      // content survives at the proportionally scaled position afterward --
      // a real 400x300 stub, not a 1x1 one, or the check would be as vacuous
      // as the scaling and zoom lesson already found once.
      {
        const whiteCanvas = document.createElement('canvas');
        whiteCanvas.width = 400;
        whiteCanvas.height = 300;
        whiteCanvas.getContext('2d').fillStyle = '#ffffff';
        whiteCanvas.getContext('2d').fillRect(0, 0, 400, 300);
        window.__FIRE__('snip-captured', { data_url: whiteCanvas.toDataURL('image/png'), path: '', name: 'scale.png' });
        await sleep(300);
        const rectBtn = document.querySelector('#snip-edit-tools button[data-tool="rect"]');
        const scaleBtn = document.getElementById('snip-edit-scale');
        if (!rectBtn || !scaleBtn) {
          out.errors.push('342: the rect tool or scale button was not found in the screenshot editor toolbar');
        } else {
          rectBtn.click();
          await sleep(30);
          const filledBox = document.querySelector('#snip-edit-context .snip-edit-toggle input');
          if (filledBox) { filledBox.checked = true; filledBox.dispatchEvent(new Event('change', { bubbles: true })); }
          await sleep(20);
          const baseCv = document.getElementById('snip-edit-base');
          const liveCv = document.getElementById('snip-edit-live');
          const rect0 = baseCv.getBoundingClientRect();
          const zoom0 = rect0.width / 400;
          const toClient = (ix, iy) => ({ x: rect0.left + ix * zoom0, y: rect0.top + iy * zoom0 });
          const p1 = toClient(50, 50), p2 = toClient(150, 150);
          liveCv.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: p1.x, clientY: p1.y }));
          liveCv.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: p2.x, clientY: p2.y }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: p2.x, clientY: p2.y }));
          await sleep(60);
          const isWhite = (data) => data[0] > 240 && data[1] > 240 && data[2] > 240;
          const beforePixel = baseCv.getContext('2d').getImageData(100, 100, 1, 1).data;
          if (isWhite(beforePixel)) {
            out.errors.push('342: the rect drawn before scaling did not leave any mark at its own (100,100) center');
          }
          scaleBtn.click();
          await sleep(30);
          const pop = document.querySelector('.snip-scale-pop');
          if (!pop) {
            out.errors.push('342: the scale button did not open the resize popover');
          } else {
            const widthInput = pop.querySelectorAll('.numfield input')[0];
            widthInput.value = '200';
            widthInput.dispatchEvent(new Event('change', { bubbles: true }));
            await sleep(20);
            const heightInput = pop.querySelectorAll('.numfield input')[1];
            if (Number(heightInput.value) !== 150) {
              out.errors.push('342: locked aspect ratio did not follow width 200 down to height 150 (got ' + heightInput.value + ')');
            }
            pop.querySelector('.snip-scale-actions .primary-btn').click();
            await sleep(60);
            if (baseCv.width !== 200 || baseCv.height !== 150) {
              out.errors.push('342: scaling to 200x150 did not change the canvas backing store (got ' + baseCv.width + 'x' + baseCv.height + ')');
            }
            const afterPixel = baseCv.getContext('2d').getImageData(50, 50, 1, 1).data;
            if (isWhite(afterPixel)) {
              out.errors.push('342: the rect drawn before scaling is gone from its proportionally scaled (50,50) position after a 2x downscale');
            }
            document.getElementById('snip-edit-undo').click();
            await sleep(60);
            if (baseCv.width !== 400 || baseCv.height !== 300) {
              out.errors.push('342: undo did not restore the exact pre-scale 400x300 size (got ' + baseCv.width + 'x' + baseCv.height + ')');
            }
            const undonePixel = baseCv.getContext('2d').getImageData(100, 100, 1, 1).data;
            if (isWhite(undonePixel)) {
              out.errors.push('342: undo restored the pre-scale size but lost the rect that was drawn on it');
            }
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // 383: the screenshot and video editors zoom on the same three keys and
      // select an existing rectangle with the same handle count -- this half
      // checks the screenshot editor, VIDEO_PROBE below checks the other.
      {
        const parityCanvas = document.createElement('canvas');
        parityCanvas.width = 400; parityCanvas.height = 300;
        parityCanvas.getContext('2d').fillStyle = '#ffffff';
        parityCanvas.getContext('2d').fillRect(0, 0, 400, 300);
        window.__FIRE__('snip-captured', { data_url: parityCanvas.toDataURL('image/png'), path: '', name: 'parity.png' });
        await sleep(300);
        const zoomVal = document.getElementById('snip-edit-zoom-val');
        const rectBtn2 = document.querySelector('#snip-edit-tools button[data-tool="rect"]');
        if (!zoomVal || !rectBtn2) {
          out.errors.push('383: screenshot editor is missing the zoom readout or the rect tool');
        } else {
          const readPct = () => Number(zoomVal.textContent.replace('%', ''));
          const fitPct = readPct();
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '+', ctrlKey: true, bubbles: true }));
          await sleep(30);
          const zoomedInPct = readPct();
          if (zoomedInPct <= fitPct) {
            out.errors.push('383: Ctrl+Plus did not zoom the screenshot editor in (was ' + fitPct + '%, now ' + zoomedInPct + '%)');
          }
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '-', ctrlKey: true, bubbles: true }));
          await sleep(30);
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '-', ctrlKey: true, bubbles: true }));
          await sleep(30);
          const zoomedOutPct = readPct();
          if (zoomedOutPct >= zoomedInPct) {
            out.errors.push('383: Ctrl+Minus did not zoom the screenshot editor out (was ' + zoomedInPct + '%, now ' + zoomedOutPct + '%)');
          }
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '0', ctrlKey: true, bubbles: true }));
          await sleep(30);
          const fitAgainPct = readPct();
          if (fitAgainPct !== fitPct) {
            out.errors.push('383: Ctrl+0 did not return the screenshot editor to its ' + fitPct + '% fit (got ' + fitAgainPct + '%)');
          }

          rectBtn2.click();
          await sleep(30);
          const baseCv2 = document.getElementById('snip-edit-base');
          const liveCv2 = document.getElementById('snip-edit-live');
          const rect2 = baseCv2.getBoundingClientRect();
          const zoom2 = rect2.width / 400;
          const toClient2 = (ix, iy) => ({ x: rect2.left + ix * zoom2, y: rect2.top + iy * zoom2 });
          const q1 = toClient2(50, 50), q2 = toClient2(150, 150);
          liveCv2.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: q1.x, clientY: q1.y }));
          liveCv2.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: q2.x, clientY: q2.y }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: q2.x, clientY: q2.y }));
          await sleep(60);
          // Selection handles are painted white on the LIVE canvas at fixed
          // image-space points around the rect's border -- nothing is there
          // until a click actually selects the shape (drawing one does not).
          const liveCtx2 = liveCv2.getContext('2d');
          const handlePoints = [[50, 50], [100, 50], [150, 50], [50, 100], [150, 100], [50, 150], [100, 150], [150, 150]];
          const countPaintedHandles = () => handlePoints.filter(([hx, hy]) => liveCtx2.getImageData(hx, hy, 1, 1).data[3] > 200).length;
          if (countPaintedHandles() !== 0) {
            out.errors.push('383: the freshly drawn rect already shows selection handles before it was clicked');
          }
          const borderPt = toClient2(50, 100); // left-edge midpoint, on the rect outline
          liveCv2.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: borderPt.x, clientY: borderPt.y }));
          window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: borderPt.x, clientY: borderPt.y }));
          await sleep(60);
          const seHandleCount = countPaintedHandles();
          if (seHandleCount !== 8) {
            out.errors.push('383: clicking the rect selected it with ' + seHandleCount + ' handles, expected 8');
          }

          // ---- 395: with the same rect still selected, arrow keys move it
          // (by numFieldStep, default 1 -- see tests/ui/tauri_stub.js),
          // Ctrl+Arrow resizes it from its top-left corner, and Escape
          // deselects instead of closing the whole editor -- there is no
          // frame-stepping to hand the arrow keys back to here, unlike the
          // video editor's own 395 check. ----
          const hint395 = document.getElementById('snip-edit-nudge-hint');
          if (!hint395 || hint395.classList.contains('hidden')) {
            out.errors.push('395: the nudge hint is not shown while a shape is selected');
          }
          // Handles are 8x8px squares (see snipEditPaintShapeHandles), too wide
          // to tell a 4px move apart from noise with a single pixel sample --
          // this instead finds the painted band's own centre within a window
          // wide enough to hold it at every position it visits here but far
          // enough from the neighbouring handles (50px apart) not to catch one
          // of those instead.
          const bandCentreX = (xMin, xMax, y) => {
            let lo = null, hi = null;
            for (let sx = xMin; sx <= xMax; sx++) {
              if (liveCtx2.getImageData(sx, y, 1, 1).data[3] > 200) { if (lo === null) lo = sx; hi = sx; }
            }
            return lo === null ? null : (lo + hi) / 2;
          };
          const bandCentreY = (x, yMin, yMax) => {
            let lo = null, hi = null;
            for (let sy = yMin; sy <= yMax; sy++) {
              if (liveCtx2.getImageData(x, sy, 1, 1).data[3] > 200) { if (lo === null) lo = sy; hi = sy; }
            }
            return lo === null ? null : (lo + hi) / 2;
          };
          const nwBefore = bandCentreX(30, 70, 50);
          const seBefore = bandCentreX(130, 170, 150);
          for (let i = 0; i < 4; i++) {
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
            await sleep(20);
          }
          const nwAfterMove = bandCentreX(30, 70, 50);
          const seAfterMove = bandCentreX(130, 170, 150);
          if (nwAfterMove !== nwBefore + 4 || seAfterMove !== seBefore + 4) {
            out.errors.push('395: four ArrowRight presses did not move the shape by exactly 4 pixels (corner at ' +
              nwBefore + ' -> ' + nwAfterMove + ')');
          }
          // Ctrl+ArrowRight grows the width from the still-fixed top-left corner
          // -- the bottom-right corner's row does not move yet, only its column.
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', ctrlKey: true, bubbles: true, cancelable: true }));
          await sleep(20);
          const nwAfterWidthGrow = bandCentreX(30, 70, 50);
          const seAfterWidthGrow = bandCentreX(130, 175, 150);
          if (nwAfterWidthGrow !== nwAfterMove || seAfterWidthGrow !== seAfterMove + 1) {
            out.errors.push('395: Ctrl+ArrowRight did not grow the shape from its top-left corner instead of moving it');
          }
          // Ctrl+ArrowDown then grows the height the same way -- its row
          // shifts down by one, its column stays exactly where the width
          // growth above just left it. Row centres (not single pixels) again,
          // since an 8px-tall handle square hides a 1px shift from a plain
          // presence check at either row.
          const seRowBeforeHeightGrow = bandCentreY(155, 140, 165);
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', ctrlKey: true, bubbles: true, cancelable: true }));
          await sleep(20);
          const seRowAfterHeightGrow = bandCentreY(155, 140, 165);
          const seColAfterHeightGrow = bandCentreX(130, 175, 151);
          if (seRowAfterHeightGrow !== seRowBeforeHeightGrow + 1 || seColAfterHeightGrow !== seAfterWidthGrow) {
            out.errors.push('395: Ctrl+ArrowDown did not grow the shape height from its fixed top-left corner');
          }
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
          await sleep(60);
          if (document.getElementById('snip-edit').classList.contains('hidden')) {
            out.errors.push('395: Escape while a shape was selected closed the whole editor instead of just deselecting it');
          }
          if (bandCentreX(30, 70, 50) !== null || bandCentreY(155, 140, 165) !== null) {
            out.errors.push('395: Escape did not remove the selection handles');
          }
          if (!hint395.classList.contains('hidden')) {
            out.errors.push('395: the nudge hint stayed visible after Escape deselected the shape');
          }

          // ---- 398: the show-original button (and the V key) compare against
          // the untouched capture -- a quick click locks it on until clicked
          // again, holding it down only peeks for as long as it stays held,
          // and a badge marks either state so it is never mistaken for the
          // edit. Drawn fresh at (200,50)-(300,150) rather than reusing the
          // rect above, which the 395 checks just moved and resized -- its
          // left edge midpoint would no longer be a known coordinate. Undo
          // staying enabled throughout proves the operation list itself was
          // never touched by any of this. ----
          const origBtn398 = document.getElementById('snip-edit-original');
          const origBadge398 = document.getElementById('snip-edit-original-badge');
          const undoBtn398 = document.getElementById('snip-edit-undo');
          if (!origBtn398 || !origBadge398) {
            out.errors.push('398: the screenshot editor is missing the show-original button or badge');
          } else {
            const q1b398 = toClient2(200, 50), q2b398 = toClient2(300, 150);
            liveCv2.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: q1b398.x, clientY: q1b398.y }));
            liveCv2.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: q2b398.x, clientY: q2b398.y }));
            window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: q2b398.x, clientY: q2b398.y }));
            await sleep(60);
            const baseCtx398 = document.getElementById('snip-edit-base').getContext('2d');
            const isWhite398 = (x, y) => { const d = baseCtx398.getImageData(x, y, 1, 1).data; return d[0] > 250 && d[1] > 250 && d[2] > 250; };
            const undoWasDisabled398 = undoBtn398.disabled;
            if (isWhite398(200, 100)) {
              out.errors.push('398: the freshly drawn rect border is not visible before the show-original check even runs');
            }
            origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 2 }));
            origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 2 }));
            await sleep(30);
            if (!isWhite398(200, 100)) out.errors.push('398: a quick click on the show-original button did not reveal the untouched capture');
            if (origBadge398.classList.contains('hidden')) out.errors.push('398: the original badge did not appear once the original was locked on');
            if (!origBtn398.classList.contains('active')) out.errors.push('398: the show-original button did not switch to its active state while locked on');
            origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 2 }));
            origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 2 }));
            await sleep(30);
            if (isWhite398(200, 100)) out.errors.push('398: a second click did not lock the original back off');
            if (!origBadge398.classList.contains('hidden')) out.errors.push('398: the original badge stayed after the second click turned it off');
            origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 2 }));
            await sleep(450);
            if (!isWhite398(200, 100)) out.errors.push('398: holding the show-original button down did not reveal the untouched capture');
            origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 2 }));
            await sleep(30);
            if (isWhite398(200, 100)) out.errors.push('398: releasing a held show-original press did not restore the edit');
            if (!origBadge398.classList.contains('hidden')) out.errors.push('398: the original badge stayed visible after releasing a held press');
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'v', bubbles: true }));
            await sleep(450);
            if (!isWhite398(200, 100)) out.errors.push('398: holding V did not reveal the untouched capture');
            document.dispatchEvent(new KeyboardEvent('keyup', { key: 'v', bubbles: true }));
            await sleep(30);
            if (isWhite398(200, 100)) out.errors.push('398: releasing V did not restore the edit');
            if (undoBtn398.disabled !== undoWasDisabled398) out.errors.push('398: the show-original round trip changed the operation list (undo availability changed)');
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // 470: dragging a selected rect (or one of its resize handles), and
      // dragging a line endpoint, used to be silent no-ops -- clicking to
      // select already worked on pointerdown, but the drag itself had no
      // matching case in pointermove or pointerup.
      {
        const canvas470 = document.createElement('canvas');
        canvas470.width = 400; canvas470.height = 300;
        canvas470.getContext('2d').fillStyle = '#ffffff';
        canvas470.getContext('2d').fillRect(0, 0, 400, 300);
        window.__FIRE__('snip-captured', { data_url: canvas470.toDataURL('image/png'), path: '', name: 'drag470.png' });
        await sleep(300);
        const rectBtn470 = document.querySelector('#snip-edit-tools button[data-tool="rect"]');
        const lineBtn470 = document.querySelector('#snip-edit-tools button[data-tool="line"]');
        const baseCv470 = document.getElementById('snip-edit-base');
        const liveCv470 = document.getElementById('snip-edit-live');
        if (!rectBtn470 || !lineBtn470 || !baseCv470 || !liveCv470) {
          out.errors.push('470: the screenshot editor is missing the rect tool, the line tool or a canvas');
        } else {
          // Reads the canvas position fresh on every call rather than caching
          // it once -- the tool options panel changes width per tool, which
          // shifts the canvas, so a cached rect drifts out of date as soon as
          // a different tool gets picked.
          const toClient470 = (ix, iy) => {
            const r = baseCv470.getBoundingClientRect();
            const zoom = r.width / 400;
            return { clientX: r.left + ix * zoom, clientY: r.top + iy * zoom };
          };
          // altKey turns off the editor snap-to-edges feature for the whole
          // drag, so every coordinate below lands exactly where sent instead
          // of drifting toward a nearby edge or centre.
          const down470 = (p) => liveCv470.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, altKey: true, ...p }));
          const move470 = (p) => liveCv470.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, altKey: true, ...p }));
          const up470 = (p) => window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, altKey: true, ...p }));
          const drag470 = async (ix1, iy1, ix2, iy2) => {
            down470(toClient470(ix1, iy1));
            move470(toClient470(ix2, iy2));
            up470(toClient470(ix2, iy2));
            await sleep(60);
          };
          const click470 = async (ix, iy) => {
            down470(toClient470(ix, iy));
            up470(toClient470(ix, iy));
            await sleep(60);
          };
          const baseCtx470 = baseCv470.getContext('2d');
          const hasInk470 = (x, y) => {
            const d = baseCtx470.getImageData(x, y, 1, 1).data;
            return !(d[0] > 250 && d[1] > 250 && d[2] > 250);
          };

          // ---- drag a selected rect by its body ----
          // Every ink check below samples 2px inward from the shape's own
          // corner or endpoint, never the exact boundary pixel -- a stroked
          // canvas edge anti-aliases its outermost pixel down to a partial
          // tint that a strict whiteness check can misread as blank.
          rectBtn470.click();
          await sleep(30);
          await drag470(40, 40, 140, 140); // draw the rect
          await click470(40, 60); // left border, away from any handle -- selects it
          if (!hasInk470(42, 42)) out.errors.push('470: the drawn rect shows no ink near its own corner before the move drag runs');
          await drag470(40, 60, 90, 110); // grab the same border point, drag it by (50,50)
          if (hasInk470(42, 42)) out.errors.push('470: dragging a selected rect left ink behind near its old corner (40,40)');
          if (!hasInk470(92, 92)) out.errors.push('470: dragging a selected rect by (50,50) did not move its corner near (90,90)');

          // ---- resize the same rect from its bottom-right handle ----
          if (!hasInk470(186, 186)) out.errors.push('470: the moved rect shows no ink near its own corner before the resize drag runs');
          await drag470(190, 190, 230, 230); // the se handle sits at the moved corner
          if (hasInk470(186, 186)) out.errors.push('470: resizing a selected rect from its se handle left ink behind near the old corner (190,190)');
          if (!hasInk470(226, 226)) out.errors.push('470: resizing a selected rect from its se handle did not grow it near (230,230)');

          // ---- drag one endpoint of a selected line ----
          lineBtn470.click();
          await sleep(30);
          await drag470(250, 50, 350, 100); // draw the line
          await click470(300, 75); // midpoint, away from either endpoint handle -- selects it
          if (!hasInk470(340, 95)) out.errors.push('470: the drawn line shows no ink near its own endpoint before the endpoint drag runs');
          await drag470(350, 100, 390, 140); // grab the far endpoint and pull it further out
          if (hasInk470(340, 95)) out.errors.push('470: dragging a line endpoint left ink behind near its old position (350,100)');
          if (!hasInk470(380, 134)) out.errors.push('470: dragging a line endpoint to (390,140) put no line near there');
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
      // 365: the fullscreen media viewer (lightbox) keeps the point under the
      // cursor fixed while wheel-zooming instead of always zooming to its own
      // centre. Opened through a real prompt-editor preview click; the image's
      // own top-left corner (as far from its centre as a point gets) is the
      // known, trackable coordinate.
      {
        document.querySelector('.view-btn[data-view-id="archive"]')?.click();
        await sleep(250);
        const freeCell = [...document.querySelectorAll('#grid .cell')].find((c) => !c.firstElementChild);
        if (!freeCell) {
          out.errors.push('365: no empty grid cell available to open the media viewer through');
        } else {
          freeCell.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
          await sleep(200);
          const modalImg = document.getElementById('modal-img');
          const modalImgWrap = document.getElementById('modal-img-wrap');
          if (!modalImg || !modalImgWrap) {
            out.errors.push('365: the prompt editor has no media preview element to open the viewer from');
          } else {
            // A real size, not the file's usual 1x1 fixture -- the viewer's
            // <img> has no CSS floor, so a 1x1 source would render at 1x1 CSS
            // pixel and make any zoom-anchor error too small to ever measure.
            const bigCanvas365 = document.createElement('canvas');
            bigCanvas365.width = 400; bigCanvas365.height = 300;
            const ctx365 = bigCanvas365.getContext('2d');
            ctx365.fillStyle = '#4488cc'; ctx365.fillRect(0, 0, 400, 300);
            modalImg.src = bigCanvas365.toDataURL('image/png');
            modalImgWrap.classList.remove('hidden');
            modalImg.click();
            await sleep(200);
            const lb = document.getElementById('lightbox');
            if (!lb || lb.classList.contains('hidden')) {
              out.errors.push('365: clicking the media preview did not open the fullscreen viewer');
            } else {
              const lbImg = document.getElementById('lightbox-img');
              const boxBefore = lbImg.getBoundingClientRect();
              const cx = boxBefore.left, cy = boxBefore.top;
              document.getElementById('lightbox-stage').dispatchEvent(
                new WheelEvent('wheel', { bubbles: true, cancelable: true, clientX: cx, clientY: cy, deltaY: -100 }));
              await sleep(30);
              const boxAfter = lbImg.getBoundingClientRect();
              const moved = Math.hypot(boxAfter.left - cx, boxAfter.top - cy);
              if (moved >= 2) {
                out.errors.push('365: zooming the media viewer with the wheel did not keep the point under the cursor fixed (moved ' + moved.toFixed(1) + 'px)');
              }
              document.getElementById('lightbox-close').click();
              await sleep(150);
            }
          }
          document.getElementById('modal-close-x').click();
          await sleep(150);
        }
      }
      // 181: a properly sized capture (not the 1x1 fixture above) to check the
      // text tool near the right edge -- an unstyled <input> does not shrink to
      // fit inside the stage, so placing it purely from the click point can push
      // it past the visible canvas area. Bigger than any tested viewport so
      // snipEditFit() always shrinks it to fill the wrap -- the stage's right
      // edge then lands at (or past) the wrap's own right edge at every size,
      // instead of leaving slack that would hide an overflowing input.
      const bigCanvas = document.createElement('canvas');
      bigCanvas.width = 1600; bigCanvas.height = 900;
      bigCanvas.getContext('2d').fillRect(0, 0, 1600, 900);
      window.__FIRE__('snip-captured', { data_url: bigCanvas.toDataURL('image/png'), path: '', name: 'big.png' });
      await sleep(300);
      if (!document.getElementById('snip-edit').classList.contains('hidden')) {
        const textBtn = document.querySelector('#snip-edit-tools button[data-tool="text"]');
        const wrap = document.getElementById('snip-edit-canvas-wrap');
        const liveCv = document.getElementById('snip-edit-live');
        textBtn.click();
        await sleep(30);
        const sr = liveCv.getBoundingClientRect();
        const rightEdge = { clientX: sr.left + sr.width - 3, clientY: sr.top + sr.height / 2 };
        liveCv.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, ...rightEdge }));
        await sleep(50);
        const ti = document.getElementById('snip-edit-text-input');
        if (ti.classList.contains('hidden')) {
          out.errors.push('screenshot editor: text tool did not open the input near the right edge');
        } else {
          const wr = wrap.getBoundingClientRect();
          const tr = ti.getBoundingClientRect();
          if (tr.right > wr.right + 1) {
            out.errors.push('screenshot editor: text input near the right edge overflows the canvas area by ' +
              Math.round(tr.right - wr.right) + 'px');
          }
        }
        // 472: pan room reaches every corner and edge, and the wheel zoom
        // anchors on the picture point under the cursor.
        ti.blur();
        await sleep(30);
        if (document.activeElement === ti) {
          out.errors.push('472: the text input kept focus after blur, so the editor key handler would ignore shortcuts');
        }
        {
          const zoomInBtn = document.getElementById('snip-edit-zoom-in');
          const zoomVal2 = document.getElementById('snip-edit-zoom-val');
          let lastPct = parseInt(zoomVal2.textContent, 10);
          for (let i = 0; i < 12; i++) {
            zoomInBtn.click();
            await sleep(20);
            const pct = parseInt(zoomVal2.textContent, 10);
            if (pct <= lastPct) break;
            lastPct = pct;
          }
          const stage = document.querySelector('.snip-edit-stage');
          wrap.scrollLeft = 0; wrap.scrollTop = 0;
          let stageRect = stage.getBoundingClientRect();
          let wrapRect = wrap.getBoundingClientRect();
          if (stageRect.left - wrapRect.left < wrap.clientWidth / 2 - 2) {
            out.errors.push('472: the left edge of the picture cannot be panned to the middle of the view');
          }
          if (stageRect.top - wrapRect.top < wrap.clientHeight / 2 - 2) {
            out.errors.push('472: the top edge of the picture cannot be panned to the middle of the view');
          }
          wrap.scrollLeft = wrap.scrollWidth; wrap.scrollTop = wrap.scrollHeight;
          stageRect = stage.getBoundingClientRect();
          wrapRect = wrap.getBoundingClientRect();
          if (wrapRect.left + wrap.clientWidth - stageRect.right < wrap.clientWidth / 2 - 2) {
            out.errors.push('472: the right edge of the picture cannot be panned to the middle of the view');
          }
          if (wrapRect.top + wrap.clientHeight - stageRect.bottom < wrap.clientHeight / 2 - 2) {
            out.errors.push('472: the bottom edge of the picture cannot be panned to the middle of the view');
          }
          wrap.scrollLeft = (wrap.scrollWidth - wrap.clientWidth) / 2;
          wrap.scrollTop = (wrap.scrollHeight - wrap.clientHeight) / 2;
          stageRect = stage.getBoundingClientRect();
          wrapRect = wrap.getBoundingClientRect();
          const px = wrapRect.left + wrap.clientWidth / 2 + 40;
          const py = wrapRect.top + wrap.clientHeight / 2 + 30;
          const scale = stageRect.width / 1600;
          const picX = (px - stageRect.left) / scale;
          const picY = (py - stageRect.top) / scale;
          wrap.dispatchEvent(new WheelEvent('wheel', { deltaY: 100, clientX: px, clientY: py, bubbles: true, cancelable: true }));
          await sleep(30);
          const stageRect2 = stage.getBoundingClientRect();
          const scale2 = stageRect2.width / 1600;
          const picX2 = (px - stageRect2.left) / scale2;
          const picY2 = (py - stageRect2.top) / scale2;
          if (Math.abs(picX2 - picX) > 1 || Math.abs(picY2 - picY) > 1) {
            out.errors.push('472: zooming with the wheel did not keep the picture point under the cursor');
          }
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '0', ctrlKey: true, bubbles: true }));
          await sleep(30);
          const stageRect3 = stage.getBoundingClientRect();
          const wrapRect3 = wrap.getBoundingClientRect();
          const wrapCx = wrapRect3.left + wrap.clientWidth / 2;
          const wrapCy = wrapRect3.top + wrap.clientHeight / 2;
          const stageCx = stageRect3.left + stageRect3.width / 2;
          const stageCy = stageRect3.top + stageRect3.height / 2;
          if (Math.abs(stageCx - wrapCx) > 1 || Math.abs(stageCy - wrapCy) > 1) {
            out.errors.push('472: Ctrl+0 did not fit and centre the picture in the view');
          }
        }
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await sleep(30);
        document.getElementById('snip-edit-cancel').click();
        await sleep(200);
      }
    }
    // 181 track 10: all seven built-in themes, and the toolbar in languages
    // with markedly longer tool labels than English. Sets data-theme directly
    // rather than driving the settings theme picker (a separate custom widget,
    // already covered by its own tests) -- this is only about the editor
    // rendering sanely once a theme is active, not about picking one.
    {
      const TINY_PNG2 = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
      const priorTheme = document.documentElement.getAttribute('data-theme');
      const BUILTIN_THEMES = ['light', 'dark', 'programmer', 'neon', 'cyberpunk', 'ocean'];
      const swatch508 = document.createElement('div');
      swatch508.style.cssText = 'position:absolute;left:-9999px;top:-9999px;background-color:var(--surface)';
      document.body.appendChild(swatch508);
      for (const theme of BUILTIN_THEMES) {
        document.documentElement.setAttribute('data-theme', theme);
        window.__FIRE__('snip-captured', { data_url: TINY_PNG2, path: '', name: 'test.png' });
        await sleep(150);
        layoutScan('screenshot editor in theme ' + theme);
        // 508: the text tool's shape-back select is the one bare select the
        // screenshot editor owns; assert it picked up the themed look too.
        const textToolBtn = document.querySelector('#snip-edit-tools button[data-tool="text"]');
        if (textToolBtn) {
          textToolBtn.click();
          const seSel = document.querySelector('#snip-edit-context select.snip-edit-select');
          if (!seSel) {
            out.errors.push('508: the screenshot editor text-back select is missing in theme ' + theme);
          } else {
            const cs = getComputedStyle(seSel);
            if (parseFloat(cs.borderTopWidth) <= 0) out.errors.push('508: snip editor select has no border in theme ' + theme);
            if (cs.backgroundColor === 'rgba(0, 0, 0, 0)' || cs.backgroundColor === 'transparent') {
              out.errors.push('508: snip editor select has a transparent background in theme ' + theme);
            }
            const surfaceRgb508 = getComputedStyle(swatch508).backgroundColor;
            // A selected option shows the accent colour (option:checked); pick
            // an unselected one to check the plain surface colour instead.
            const unselected = [...seSel.options].find((o) => !o.selected);
            if (unselected) {
              const optBg = getComputedStyle(unselected).backgroundColor;
              if (optBg !== surfaceRgb508) {
                out.errors.push('508: snip editor select option background is ' + optBg + ', expected ' + surfaceRgb508 + ' in theme ' + theme);
              }
            }
          }
          // 507: with the text tool's full field set showing, the context
          // row must wrap onto a second line rather than scroll sideways.
          const ctxEl = document.querySelector('.snip-edit-context');
          if (ctxEl && ctxEl.scrollWidth > ctxEl.clientWidth) {
            out.errors.push('507: snip-edit-context scrolls horizontally with the text tool at ' + innerWidth + 'px, theme ' + theme);
          }
        }
        const pixelateToolBtn = document.querySelector('#snip-edit-tools button[data-tool="pixelate"]');
        if (pixelateToolBtn) {
          pixelateToolBtn.click();
          const ctxEl2 = document.querySelector('.snip-edit-context');
          if (ctxEl2 && ctxEl2.scrollWidth > ctxEl2.clientWidth) {
            out.errors.push('507: snip-edit-context scrolls horizontally with the pixelate tool at ' + innerWidth + 'px, theme ' + theme);
          }
        }
        document.getElementById('snip-edit-cancel').click();
        await sleep(100);
      }
      swatch508.remove();
      if (priorTheme) document.documentElement.setAttribute('data-theme', priorTheme);
      else document.documentElement.removeAttribute('data-theme');
      // The page is left in German after the earlier lang loop (later OCR
      // checks depend on that) -- covers EN/PL/RO/JA here, restores de after.
      const langSel2 = document.getElementById('lang-select');
      for (const lang of ['en', 'pl', 'ro', 'ja']) {
        langSel2.value = lang;
        langSel2.dispatchEvent(new Event('change'));
        await sleep(150);
        window.__FIRE__('snip-captured', { data_url: TINY_PNG2, path: '', name: 'test.png' });
        await sleep(150);
        const toolBtns2 = [...document.querySelectorAll('#snip-edit-tools button[data-tool]')];
        if (toolBtns2.length !== 12) { // 284 added the Hand (pan) tool
          out.errors.push('screenshot editor in ' + lang + ': expected 12 tool buttons, found ' + toolBtns2.length);
        }
        layoutScan('screenshot editor in ' + lang);
        document.getElementById('snip-edit-cancel').click();
        await sleep(100);
      }
      langSel2.value = 'de';
      langSel2.dispatchEvent(new Event('change'));
      await sleep(150);
    }
"""
