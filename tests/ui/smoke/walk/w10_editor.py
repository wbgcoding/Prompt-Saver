SECTION = """\
    // 10: the quick-grid header control hides until you point at the header - pure
    // CSS :hover, so a synthetic event can't prove the reveal itself; this only
    // proves the flag actually hides/unhides the control (the hover is checked by eye).
    const qgEl = document.getElementById('quick-grid');
    // The header columns/rows fields are an opt-in: hidden on a fresh profile,
    // while the grid size stays reachable from two right-click menus.
    if (getComputedStyle(qgEl).display !== 'none') out.errors.push('the quick-grid fields show on a fresh profile');
    {
      const openMenuAt = (el) => {
        const r = el.getBoundingClientRect();
        el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 6, clientY: r.top + 6 }));
      };
      const gridMenuItem = () => [...document.querySelectorAll('.tab-menu:not(.hidden) button')].find(b => b.textContent.trim().endsWith('…'));
      const checkGridSizeEntry = async (what, el) => {
        if (!el) { out.errors.push('grid size menu: no ' + what + ' to right-click'); return; }
        openMenuAt(el);
        await sleep(150);
        const it = gridMenuItem();
        if (!it) { out.errors.push('grid size menu: right-click on ' + what + ' offers no Grid size entry'); document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); return; }
        it.click();
        await sleep(200);
        const vm = document.getElementById('view-modal');
        if (vm.classList.contains('hidden')) out.errors.push('grid size menu: ' + what + ' did not open the view dialog');
        else if (document.activeElement !== document.getElementById('view-modal-cols')) out.errors.push('grid size menu: ' + what + ' did not focus the columns field');
        await click('view-modal-close');
        await sleep(150);
      };
      await checkGridSizeEntry('a view tab', document.querySelector('.view-btn[data-view-id]'));
      await checkGridSizeEntry('an empty grid cell', [...document.querySelectorAll('#grid .cell')].find(c => !c.firstElementChild));
    }
    if (!await setExpertFlag('Quick grid size', true)) out.errors.push('the quick-grid switch is missing from the expert menu');
    else if (getComputedStyle(qgEl).display === 'none') out.errors.push('the quick-grid fields stay hidden after switching them on');
    if (!await setExpertFlag('Quick grid: hover to reveal', true)) {
      out.errors.push('the quick-grid hover-only option is missing from the expert menu');
    } else {
      const csOn = getComputedStyle(qgEl);
      if (csOn.opacity !== '0') out.errors.push('the quick-grid control is not hidden with hover-only on: opacity ' + csOn.opacity);
      if (csOn.pointerEvents === 'auto') out.errors.push('the hidden quick-grid control still accepts clicks');
      await setExpertFlag('Quick grid: hover to reveal', false);
      const csOff = getComputedStyle(qgEl);
      if (csOff.opacity !== '1') out.errors.push('the quick-grid control stays hidden after switching hover-only back off');
    }
    // P31: version history shows a word diff against whatever this snapshot
    // turned into next, not just the raw old text.
    if (!await openEditor('p6')) {
      out.errors.push('the version-history prompt did not open in the editor');
    } else {
      document.getElementById('modal-advanced-head').click();
      await sleep(250);
      const rows = [...document.querySelectorAll('#modal-history-body .hist-row')];
      if (rows.length !== 2) {
        out.errors.push('the history panel shows ' + rows.length + ' versions instead of 2');
      } else {
        // 125: each row's full diff is now built lazily on first expand -- open
        // every row before reading its .hist-full content.
        for (const r of rows) r.querySelector('.hist-expand')?.click();
        // Newest row (id 2): "draft"->"prompt" against the live current text.
        const newRowText = rows[0].querySelector('.hist-full').textContent;
        if (!newRowText.includes('draft') || !newRowText.includes('prompt')) {
          out.errors.push('the newest version diff is missing the expected words: ' + newRowText);
        }
        const newAdd = rows[0].querySelector('.diff-add'), newDel = rows[0].querySelector('.diff-del');
        if (!newAdd || newAdd.textContent !== 'prompt') out.errors.push('the newest row does not mark "prompt" as added');
        if (!newDel || newDel.textContent !== 'draft') out.errors.push('the newest row does not mark "draft" as removed');
        // Oldest row (id 1): "text"->"body" against the next-newer snapshot.
        const oldAdd = rows[1].querySelector('.diff-add'), oldDel = rows[1].querySelector('.diff-del');
        if (!oldAdd || oldAdd.textContent !== 'body') out.errors.push('the oldest row does not mark "body" as added');
        if (!oldDel || oldDel.textContent !== 'text') out.errors.push('the oldest row does not mark "text" as removed');
      }
      await click('modal-close-x');
      await sleep(200);
      const cmHist = document.getElementById('confirm-modal');
      if (!cmHist.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // 408: "Save as new entry" leaves the original prompt alone and adds the
    // snapshot's content as its own new entry instead, closing the editor and
    // showing it selected so the user sees what happened.
    if (!await openEditor('p6')) {
      out.errors.push('408: the version-history prompt did not open in the editor');
    } else {
      document.getElementById('modal-advanced-head').click();
      await sleep(250);
      const rows408 = [...document.querySelectorAll('#modal-history-body .hist-row')];
      if (rows408.length !== 2) {
        out.errors.push('408: the history panel shows ' + rows408.length + ' versions instead of 2');
      } else {
        const restoreNewBtn = rows408[0].querySelector('.hist-restore-new');
        if (!restoreNewBtn) {
          out.errors.push('408: the newest version row has no "save as new entry" button');
        } else {
          restoreNewBtn.click();
          await sleep(400);
          if (!document.getElementById('modal').classList.contains('hidden')) {
            out.errors.push('408: the editor stayed open after saving the version as a new entry');
          }
          const toastEl408 = document.getElementById('toast');
          if ((toastEl408.textContent || '').trim() !== 'New entry created') {
            out.errors.push('408: no "new entry created" toast after saving the version as new: ' + toastEl408.textContent);
          }
          await click('library-btn');
          const expectedName = 'Prompt 7 (version from';
          const hit = [...document.querySelectorAll('#library-list .lib-name')].some(e => e.textContent.startsWith(expectedName));
          if (!hit) {
            out.errors.push('408: no new library entry named like "' + expectedName + '…" after saving the version as new');
          }
          await click('library-close');
        }
      }
    }
    // 79: version history for a rotating prompt shows every entry, numbered, each
    // with its own diff, an entry-count chip on the row, and a name-change line
    // wherever the snapshot's name differs from what it became next.
    if (!await openEditor('p10')) {
      out.errors.push('79: the rotating prompt did not open in the editor');
    } else {
      document.getElementById('modal-advanced-head').click();
      await sleep(250);
      layoutScan('rotating prompt history');
      const rows10 = [...document.querySelectorAll('#modal-history-body .hist-row')];
      if (rows10.length !== 2) {
        out.errors.push('79: the rotating prompt history shows ' + rows10.length + ' versions instead of 2');
      } else {
        // 125: each row's full diff is now built lazily on first expand.
        for (const r of rows10) r.querySelector('.hist-expand')?.click();
        const chip = rows10[0].querySelector('.hist-entries');
        if (!chip || !chip.textContent.includes('2')) {
          out.errors.push('79: the newest rotating-prompt row is missing its entry-count chip');
        }
        const blocks = rows10[0].querySelectorAll('.hist-entry-block');
        if (blocks.length < 2) out.errors.push('79: the rotating prompt diff does not show a numbered block per entry');
        if (!rows10[1].querySelector('.hist-name-change')) {
          out.errors.push('79: a renamed version does not show the old to new name line');
        }
      }
      await click('modal-close-x');
      await sleep(200);
      const cmHist10 = document.getElementById('confirm-modal');
      if (!cmHist10.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // 79: a file/image prompt has no text field but still gets version history -
    // only a name change to show, never an empty box.
    if (!await openEditor('p9')) {
      out.errors.push('79: the file prompt did not open in the editor');
    } else {
      document.getElementById('modal-advanced-head').click();
      await sleep(200);
      if (document.getElementById('modal-history').classList.contains('hidden')) {
        out.errors.push('79: version history is hidden for a file prompt');
      }
      await click('modal-close-x');
      await sleep(200);
      const cmHist9 = document.getElementById('confirm-modal');
      if (!cmHist9.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // 749: font and tile text size live in the Advanced options body, and the
    // Advanced head stays visible even with tags, hotkeys and history all off.
    {
      const flags749 = ['Tags', 'Hotkey per prompt', 'Prompt versions'];
      for (const f of flags749) await setExpertFlag(f, false);
      if (!await openEditor('p9')) {
        out.errors.push('749: the file prompt did not open in the editor');
      } else {
        const body749 = document.getElementById('modal-advanced-body');
        const font749 = document.getElementById('modal-font');
        if (!font749 || !body749.contains(font749)) out.errors.push('749: the font select is not inside the advanced options');
        if (document.getElementById('modal-advanced').classList.contains('hidden')) {
          out.errors.push('749: the advanced options head is hidden with tags, hotkeys and history off');
        }
        // 751: a file prompt has no text field, so no label over it either.
        if (document.getElementById('modal-text-label').getBoundingClientRect().height > 0) {
          out.errors.push('751: the prompt text label is shown for a file prompt');
        }
        await click('modal-close-x');
        await sleep(200);
        const cm749 = document.getElementById('confirm-modal');
        if (!cm749.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
      }
      for (const f of flags749) await setExpertFlag(f, true);
    }
    // 750: the mic sits on the counter row of a text prompt, to the right of the counter.
    {
      await setExpertFlag('Live length counter', true);
      if (!await openEditor('p6')) {
        out.errors.push('750: the text prompt did not open in the editor');
      } else {
        // 751-text: a text prompt shows the visible label over the prompt text; the
        // Markdown pane carries the same accessible name.
        const lab751 = document.getElementById('modal-text-label');
        if (!(lab751.getBoundingClientRect().height > 0) || !lab751.textContent.trim() || lab751.textContent.trim() === 'promptTextLabel') {
          out.errors.push('751-text: a text prompt has no prompt text label');
        }
        if (!document.getElementById('modal-md-preview').getAttribute('aria-label')) out.errors.push('751-text: the Markdown pane has no accessible name');
        const len750 = document.getElementById('modal-length').getBoundingClientRect();
        const mic750 = document.getElementById('modal-voice-btn').getBoundingClientRect();
        if (!(len750.height > 0) || !(mic750.height > 0)) {
          out.errors.push('750: the counter or the mic is not shown in the editor');
        } else {
          // Centred in one flex row: the box tops differ by half the height gap, so the
          // vertical centres are what must agree.
          const dy750 = Math.abs((mic750.top + mic750.bottom) / 2 - (len750.top + len750.bottom) / 2);
          if (dy750 > 6) out.errors.push('750: the mic is not on the counter row: centres ' + Math.round(dy750) + 'px apart');
          if (mic750.left < len750.right) out.errors.push('750: the mic is not right of the counter');
        }
        await click('modal-close-x');
        await sleep(200);
        const cm750 = document.getElementById('confirm-modal');
        if (!cm750.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
      }
      await setExpertFlag('Live length counter', false);
    }
    // 429: a prompt with both a stored preview and a real attachment gets a line
    // explaining the difference, spelled out and not a raw i18n key.
    if (!await openEditor('p11')) {
      out.errors.push('429: the preview+attachment prompt did not open in the editor');
    } else {
      const hint = document.getElementById('modal-preview-attach-hint');
      const fileIcon = document.getElementById('modal-file-icon');
      if (!hint || hint.classList.contains('hidden')) {
        out.errors.push('429: the preview/attachment hint is not shown when both exist');
      } else if (!hint.textContent.trim() || hint.textContent.trim() === 'previewAttachHint') {
        out.errors.push('429: the preview/attachment hint shows no text or a raw key');
      }
      if (!fileIcon || !fileIcon.title || fileIcon.title === 'previewAttachHint') {
        out.errors.push('429: the attachment icon has no matching tooltip text');
      } else if (hint && fileIcon.title !== hint.textContent.trim()) {
        out.errors.push('429: the attachment icon tooltip does not match the editor hint text');
      }
      await click('modal-close-x');
      await sleep(200);
      const cmHist11 = document.getElementById('confirm-modal');
      if (!cmHist11.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // 429: the same prompt without a file attachment must NOT show the hint - it is
    // only for the case where the two things could actually be confused.
    if (!await openEditor('p7')) {
      out.errors.push('429: the plain-text prompt did not open in the editor');
    } else {
      const hint7 = document.getElementById('modal-preview-attach-hint');
      if (!hint7 || !hint7.classList.contains('hidden')) {
        out.errors.push('429: the preview/attachment hint shows for a prompt with no attachment');
      }
      await click('modal-close-x');
      await sleep(200);
      const cmHist7b = document.getElementById('confirm-modal');
      if (!cmHist7b.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // 466: a text attachment (txt/md/json/csv) previews its start right in the
    // editor, and injected markup in it must show up literally, never rendered.
    window.__ATTACH_TEXT_FIXTURES__ = window.__ATTACH_TEXT_FIXTURES__ || {};
    window.__ATTACH_TEXT_FIXTURES__['C:\\\\Notes\\\\brief.txt'] = { text: XSS_TEXT, truncated: false, readable: true };
    if (!await openEditor('p11')) {
      out.errors.push('466: the text-attachment prompt did not open in the editor');
    } else {
      await sleep(150);
      const box = document.getElementById('modal-text-attach-preview');
      const content = document.getElementById('modal-text-attach-content');
      if (!box || box.classList.contains('hidden')) {
        out.errors.push('466: the attachment text preview did not show for a .txt attachment');
      } else if (!content || content.textContent !== XSS_TEXT) {
        out.errors.push('466: the attachment preview text does not match the file content');
      } else if (content.querySelector('img') || content.querySelector('script')) {
        out.errors.push('466: injected markup in a text attachment was rendered as an element');
      }
      await click('modal-close-x');
      await sleep(200);
      const cmAttach = document.getElementById('confirm-modal');
      if (!cmAttach.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
    }
    // Duplicates: its own dialog, reachable from the library header, and the last
    // window of the main surface that no scan had ever opened.
    await click('library-btn');
    await sleep(150);
    const dupBtn = document.getElementById('library-dupes');
    if (!dupBtn) {
      out.errors.push('the library header has no way to open the duplicates dialog');
    } else {
      dupBtn.click();
      await sleep(500);
      const dm = document.getElementById('dupes-modal');
      if (dm.classList.contains('hidden')) {
        out.errors.push('the duplicates dialog did not open');
      } else {
        if (!document.querySelectorAll('#dupes-body .dup-group').length) {
          out.errors.push('the duplicates dialog shows no group for a reported duplicate');
        }
        layoutScan('duplicates');
        mustReach('duplicates', '#dupes-body .dup-group button, #dupes-modal .modal-head-actions > button');
        // The group is now just a summary card -- its own "Compare" button opens
        // the SAME view the library's/tile's free compare uses, this time with
        // the finder's own "keep one side" actions instead of "take left/right".
        const groupCompareBtn = document.querySelector('#dupes-body .dup-group .dup-actions button');
        if (!groupCompareBtn) {
          out.errors.push('a duplicate group has no way to open the compare view');
        } else {
          groupCompareBtn.click();
          await sleep(300);
          const cmpModal = document.getElementById('compare-modal');
          if (cmpModal.classList.contains('hidden')) {
            out.errors.push('the duplicate group did not open the compare view');
          } else {
            if (document.querySelectorAll('#compare-body .dup-col').length !== 2) {
              out.errors.push('the compare view does not show both entries of the duplicate pair');
            }
            // A found pair shares the same word-diff engine as the free compare
            // view -- its two columns must highlight what differs, not just show
            // plain text side by side.
            if (!document.querySelector('#compare-body .diff-add, #compare-body .diff-del')) {
              out.errors.push('a duplicate pair does not highlight its word differences');
            }
            if ([...document.querySelectorAll('#compare-body .dup-col')].some((col) => col.querySelectorAll('.dup-keep').length !== 1)) {
              out.errors.push('the compare view opened from the duplicate finder is missing a per-column "keep this one" action');
            }
            if (document.querySelectorAll('#compare-actions .dup-keep').length !== 0) {
              out.errors.push('the compare view opened from the duplicate finder still has "keep one side" actions in the action bar');
            }
            layoutScan('duplicates compare view');
            mustReach('duplicates compare view', '#compare-actions button, #compare-body .dup-keep');
            if (!(Number(getComputedStyle(cmpModal).zIndex) > Number(getComputedStyle(dm).zIndex))) {
              out.errors.push('the compare view opens behind the duplicates dialog');
            }
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
            await sleep(200);
            if (!cmpModal.classList.contains('hidden') || dm.classList.contains('hidden')) {
              out.errors.push('Escape does not close the compare view first, over the duplicates dialog');
            }
          }
          if (!cmpModal.classList.contains('hidden')) await click('compare-close');
          await sleep(200);
        }
        await click('dupes-close');
        await sleep(200);
      }
    }
    // The save-time similar-prompt hint (its own switch, sharing similarThreshold
    // with the duplicate finder above -- same System & data group). Off first:
    // even with a fixture hit ready, saving must show nothing of the kind. Edits
    // an EXISTING library entry rather than creating a new one -- the grid may
    // be full of tiles the earlier parts of this sweep already placed, but the
    // save path (and the code that fires the hint) is identical either way.
    const findSimHintBox = () => {
      const row = [...document.querySelectorAll('#expert-flags label.switch-field')]
        .find((r) => r.querySelector('span')?.textContent === 'Similar-prompt hint on save');
      return row ? row.querySelector('input[type="checkbox"]') : null;
    };
    const editViaLibrary = async (id, text) => {
      await click('library-btn');
      await sleep(150);
      const editBtn = document.querySelector(`#library-list .lib-item[data-id="${id}"] .lib-edit`);
      if (!editBtn) { out.errors.push('442: the library has no edit icon for ' + id); return false; }
      editBtn.click();
      await sleep(200);
      document.getElementById('modal-text').value = text;
      document.getElementById('modal-confirm').click();
      await sleep(400);
      return true;
    };
    document.getElementById('settings').classList.remove('hidden');
    await click('expert-open');
    let simTab = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')].find(b => /system/i.test(b.textContent));
    if (simTab) { simTab.click(); await sleep(200); }
    const simBoxOff = findSimHintBox();
    if (!simBoxOff) {
      out.errors.push('442: the expert menu has no similar-prompt-hint switch');
    } else {
      if (!simBoxOff.checked) out.errors.push('442: similarHint does not ship on');
      simBoxOff.click(); // off
      await sleep(150);
    }
    await click('expert-back');
    document.getElementById('settings').classList.add('hidden');
    await sleep(150);
    window.__SIMILAR_HINT_FIXTURE__ = { id: 'p1', name: 'Prompt 2' };
    if (await editViaLibrary('p2', 'Similar hint off test edit.')) {
      if ([...document.querySelectorAll('#toast .toast-chip')].some((c) => c.textContent === 'Open')) {
        out.errors.push('442: the hint appeared while similarHint is off');
      }
    }
    // Back on, then the hint itself: message, Compare, and (on a second save)
    // Open, since a chip click closes the toast that carried it.
    document.getElementById('settings').classList.remove('hidden');
    await click('expert-open');
    simTab = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')].find(b => /system/i.test(b.textContent));
    if (simTab) { simTab.click(); await sleep(200); }
    findSimHintBox()?.click(); // on
    await sleep(150);
    await click('expert-back');
    document.getElementById('settings').classList.add('hidden');
    await sleep(150);
    if (await editViaLibrary('p3', 'Similar hint on test edit, compare action.')) {
      const toastEl = document.getElementById('toast');
      const chips = [...toastEl.querySelectorAll('.toast-chip')];
      if (toastEl.classList.contains('hidden') || chips.length !== 2) {
        out.errors.push('442: the similar-prompt hint did not appear with its two actions');
      } else {
        if (!toastEl.textContent.includes('Prompt 2')) out.errors.push('442: the hint does not name the similar prompt');
        const compareChip = chips.find((c) => c.textContent === 'Compare');
        if (!compareChip) {
          out.errors.push('442: the hint has no Compare action');
        } else {
          compareChip.click();
          await sleep(300);
          if (document.getElementById('compare-modal').classList.contains('hidden')) {
            out.errors.push('442: Compare on the hint did not open the compare view');
          }
          await click('compare-close');
          await sleep(200);
        }
      }
    }
    if (await editViaLibrary('p4', 'Similar hint on test edit, open action.')) {
      const openChip = [...document.querySelectorAll('#toast .toast-chip')].find((c) => c.textContent === 'Open');
      if (!openChip) {
        out.errors.push('442: the hint has no Open action on its second appearance');
      } else {
        openChip.click();
        await sleep(300);
        if (document.getElementById('modal').classList.contains('hidden') || document.getElementById('modal-name').value !== 'Prompt 2') {
          out.errors.push('442: Open on the hint did not open the similar prompt for editing');
        }
        await click('modal-close-x');
        await sleep(200);
        const cmAfterOpen = document.getElementById('confirm-modal');
        if (!cmAfterOpen.classList.contains('hidden')) { await click('confirm-ok'); await sleep(200); }
      }
    }
    delete window.__SIMILAR_HINT_FIXTURE__;
    // Compare view detached from the duplicate finder: the library's own
    // selection reaches it too, for two entries the finder never flagged.
    await click('library-btn');
    // The duplicate finder also has its own shortcut in the library's own
    // header now, next to sort - reachable without the expert menu or the
    // settings dialog, and correctly gated and layered like the other two.
    const libDupesBtn = document.getElementById('library-dupes');
    if (!libDupesBtn || libDupesBtn.offsetParent === null) {
      out.errors.push('the library header has no visible duplicate finder shortcut');
    } else {
      libDupesBtn.click();
      await sleep(300);
      const libDupesModal = document.getElementById('dupes-modal');
      if (libDupesModal.classList.contains('hidden')) {
        out.errors.push('the library header shortcut did not open the duplicate finder');
      }
      // 552: Rescan shows a busy ring for at least a second even when the scan
      // answers at once, ignores a second click meanwhile, and never changes width.
      {
        const rescan552 = document.getElementById('dupes-rescan');
        const scanCalls552 = () => (window.__SMOKE_CALLS__ || []).filter((c) => c === 'find_duplicates').length;
        await sleep(300);
        const widthBefore552 = rescan552.getBoundingClientRect().width;
        const callsBefore552 = scanCalls552();
        rescan552.click();
        await sleep(50);
        rescan552.click();
        await sleep(100);
        const widthDuring552 = rescan552.getBoundingClientRect().width;
        await sleep(750);
        if (!rescan552.classList.contains('is-busy')) out.errors.push('552: the Rescan button is not busy 900 ms after the click');
        if (rescan552.getAttribute('aria-busy') !== 'true') out.errors.push('552: the busy Rescan button has no aria-busy');
        if (!rescan552.querySelector('.btn-spinner')) out.errors.push('552: the Rescan button has no spinner element');
        await sleep(500);
        if (rescan552.classList.contains('is-busy')) out.errors.push('552: the Rescan button is still busy 1400 ms after the click');
        if (rescan552.hasAttribute('aria-busy')) out.errors.push('552: aria-busy stayed on after the scan');
        if (scanCalls552() - callsBefore552 !== 1) out.errors.push('552: a second click during the busy state started another scan (' + (scanCalls552() - callsBefore552) + ' calls)');
        if (Math.abs(widthBefore552 - widthDuring552) > 1) out.errors.push('552: the Rescan button changed width while busy: ' + widthBefore552 + ' to ' + widthDuring552);
        if (!document.querySelectorAll('#dupes-body .dup-group').length) out.errors.push('552: no result list after the rescan');
      }
      await click('dupes-close');
      await sleep(150);
    }
    document.body.classList.add('library-front');
    libDupesBtn.click();
    await sleep(300);
    if (!(Number(getComputedStyle(document.getElementById('dupes-modal')).zIndex) > Number(getComputedStyle(document.getElementById('library')).zIndex))) {
      out.errors.push('the duplicate finder opens behind the raised library');
    }
    await click('dupes-close');
    document.body.classList.remove('library-front');
    await sleep(150);
    document.body.classList.add('noflag-dupFinder');
    if (libDupesBtn.offsetParent !== null) {
      out.errors.push('the library header duplicate finder shortcut ignores its dupFinder gate');
    }
    document.body.classList.remove('noflag-dupFinder');
    document.getElementById('library-select-btn').click();
    await sleep(150);
    document.querySelector('#library-list .lib-item[data-id="p0"]')?.click();
    document.querySelector('#library-list .lib-item[data-id="p2"]')?.click();
    await sleep(150);
    const cmpBtn = document.getElementById('lib-batch-compare');
    if (!cmpBtn || cmpBtn.disabled) {
      out.errors.push('the library compare button is not enabled with exactly two entries selected');
    } else {
      cmpBtn.click();
      await sleep(300);
      const cmpModal = document.getElementById('compare-modal');
      if (cmpModal.classList.contains('hidden')) {
        out.errors.push('the compare view did not open for two selected entries');
      } else {
        const cmpCols = document.querySelectorAll('#compare-body .compare-col');
        if (cmpCols.length !== 2) {
          out.errors.push('the compare view does not show both entries: ' + cmpCols.length + ' columns');
        }
        if (!document.querySelector('#compare-body .diff-add, #compare-body .diff-del')) {
          out.errors.push('the compare view does not highlight the word differences between the two entries');
        }
        layoutScan('compare view');
        mustReach('compare view', '#compare-actions button');
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await sleep(200);
        if (!cmpModal.classList.contains('hidden') || document.getElementById('library').classList.contains('hidden')) {
          out.errors.push('Escape does not close the compare view detached from the library first');
        }
        cmpBtn.click();
        await sleep(300);
        // Position, not text, so this still finds the button under the Italian
        // pass below (takeLeft, takeRight, close, in that fixed append order).
        const takeRightBtn = document.querySelectorAll('#compare-actions button')[1];
        if (!takeRightBtn) {
          out.errors.push('the compare view has no "take right" action');
        } else {
          const leftId = 'p0', rightId = 'p2';
          const leftText = PROMPTS.find(p => p.id === leftId).text;
          takeRightBtn.click();
          await sleep(300);
          if (!document.getElementById('compare-modal').classList.contains('hidden')) {
            out.errors.push('the compare view stayed open after taking a side');
          }
          // "Take right" pushes the LEFT entry's content onto the right one.
          if (PROMPTS.find(p => p.id === rightId).text !== leftText) {
            out.errors.push('"take right" did not overwrite the right entry with the left one\\'s text');
          }
        }
      }
    }
    document.getElementById('library-select-btn').classList.contains('active') && document.getElementById('library-select-btn').click();
    await click('library-close');
    // The colour picker must fit the window at ANY size: it may neither scroll (the
    // knob would be out of reach) nor spill (the hex field gets cut off).
    if (await openEditor('p1')) {
      // 66: every colour swatch is icon-only (a coloured square, no text) and
      // needs a real name for screen readers, not just a visible colour.
      const noneSwatch = document.querySelector('#color-row .swatch.none');
      const custom = document.querySelector('#color-row .swatch.custom');
      if (!noneSwatch || !noneSwatch.title) out.errors.push('66: the none-colour swatch has no accessible name');
      if (!custom) {
        out.errors.push('the editor colour row has no custom-colour swatch');
      } else if (!custom.title) {
        out.errors.push('66: the custom-colour swatch has no accessible name');
      }
      if (custom) {
        custom.click();
        await sleep(300);
        const cp = document.querySelector('.color-pop:not(.hidden)');
        if (!cp) out.errors.push('the custom colour swatch opened no picker');
        else {
          const r = cp.getBoundingClientRect();
          if (r.top < -1 || r.bottom > innerHeight + 1 || r.left < -1 || r.right > innerWidth + 1) {
            out.errors.push('the colour picker does not fit the window (' +
              Math.round(r.top) + '..' + Math.round(r.bottom) + ' of ' + innerHeight + ')');
          }
          if (cp.scrollHeight > cp.clientHeight + 2) {
            out.errors.push('the colour picker scrolls instead of fitting');
          }
        }
        document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
        await sleep(150);
      }
      // 753: the colour row is one line before More, and More shows all 20 swatches.
      {
        const sw753 = [...document.querySelectorAll('#color-row .swatch')];
        const more753 = document.querySelector('#color-row .swatch.more');
        if (!more753) {
          out.errors.push('753: the colour row has no More colors button');
        } else {
          // offsetTop ignores the grow-on-select transform of the chosen swatch.
          const tops753 = new Set(sw753.map(e => e.offsetTop));
          if (tops753.size !== 1) out.errors.push('753: the short colour row wraps onto ' + tops753.size + ' lines');
          if (sw753.length > 12) out.errors.push('753: the short colour row has ' + sw753.length + ' swatches');
          more753.click();
          await sleep(150);
          const all753 = document.querySelectorAll('#color-row .swatch').length;
          if (all753 !== 20) out.errors.push('753: More colors shows ' + all753 + ' swatches instead of 20');
          if (document.querySelector('#color-row .swatch.more')) out.errors.push('753: the More button stays after expanding');
        }
      }
      await click('modal-close-x');
      await sleep(250);
      const cm5 = document.getElementById('confirm-modal');
      if (!cm5.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
    } else {
      out.errors.push('the editor did not open for the colour-picker check');
    }
    // foc259: an existing prompt without Markdown syntax opens with the text field
    // handed over and the caret at its end; a Markdown prompt opens rendered with the
    // caret at the end of the name; creating still selects the whole (empty) name.
    {
      const closeEd259 = async () => {
        await click('modal-close-x');
        await sleep(250);
        const cm259 = document.getElementById('confirm-modal');
        if (!cm259.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
      };
      if (await openEditor('p1')) {
        const ta259 = document.getElementById('modal-text');
        if (document.activeElement !== ta259) out.errors.push('foc259: a plain prompt does not open with the text field focused (' + (document.activeElement && document.activeElement.id) + ')');
        else if (ta259.selectionStart !== ta259.value.length || ta259.selectionEnd !== ta259.value.length) {
          out.errors.push('foc259: the caret is not at the end of the text (' + ta259.selectionStart + '..' + ta259.selectionEnd + ' of ' + ta259.value.length + ')');
        }
        await closeEd259();
      } else out.errors.push('foc259: the plain prompt did not open');
      if (await openEditor('p7')) {
        const nm259 = document.getElementById('modal-name');
        const wrap259 = document.getElementById('modal-md-wrap');
        if (wrap259.classList.contains('hidden')) out.errors.push('foc259: the Markdown prompt does not open rendered');
        if (document.activeElement !== nm259) out.errors.push('foc259: a Markdown prompt does not open with the name focused (' + (document.activeElement && document.activeElement.id) + ')');
        else if (nm259.selectionStart !== nm259.value.length || nm259.selectionEnd !== nm259.value.length) {
          out.errors.push('foc259: the caret is not at the end of the name (' + nm259.selectionStart + '..' + nm259.selectionEnd + ' of ' + nm259.value.length + ')');
        }
        await closeEd259();
      } else out.errors.push('foc259: the Markdown prompt did not open');
      const free259 = [...document.querySelectorAll('#grid .cell')].find(c => !c.firstElementChild);
      if (!free259) out.errors.push('foc259: no free cell to create a prompt in');
      else {
        free259.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(300);
        const nm259 = document.getElementById('modal-name');
        if (document.activeElement !== nm259) out.errors.push('foc259: a new prompt does not open with the name focused');
        else if (nm259.value.length && (nm259.selectionStart !== 0 || nm259.selectionEnd !== nm259.value.length)) {
          out.errors.push('foc259: the name of a new prompt is not fully selected');
        }
        await closeEd259();
      }
    }
    // act260: the editor footer reads Delete (red text, no frame), Cancel, Save; Cancel
    // takes the same path as the X (the discard question when edited); creating shows no Delete.
    {
      const vis260 = (el) => !!el && !el.classList.contains('hidden') && getComputedStyle(el).display !== 'none';
      const footer260 = () => [...document.querySelectorAll('#modal .modal-actions > button')].filter(vis260).map(b => b.id);
      if (await openEditor('p1')) {
        const order260 = footer260().join();
        if (order260 !== 'modal-delete,modal-cancel,modal-confirm') out.errors.push('act260: the edit footer order is ' + order260);
        const del260 = document.getElementById('modal-delete');
        const ref260 = document.createElement('i');
        ref260.style.color = 'var(--danger)';
        document.body.appendChild(ref260);
        const danger260 = getComputedStyle(ref260).color;
        ref260.remove();
        if (del260) {
          const cs260 = getComputedStyle(del260);
          if (cs260.color !== danger260) out.errors.push('act260: Delete is ' + cs260.color + ', not the danger colour ' + danger260);
          if (cs260.borderTopColor !== 'rgba(0, 0, 0, 0)') out.errors.push('act260: Delete has a visible border (' + cs260.borderTopColor + ')');
        }
        await click('modal-cancel');
        await sleep(300);
        if (!document.getElementById('confirm-modal').classList.contains('hidden')) out.errors.push('act260: Cancel without edits asked a question');
        if (!document.getElementById('modal').classList.contains('hidden')) out.errors.push('act260: Cancel without edits left the editor open');
      } else out.errors.push('act260: the editor did not open');
      if (await openEditor('p1')) {
        const nm260 = document.getElementById('modal-name');
        nm260.value = nm260.value + ' edited';
        nm260.dispatchEvent(new Event('input', { bubbles: true }));
        await click('modal-cancel');
        await sleep(300);
        const cm260 = document.getElementById('confirm-modal');
        if (cm260.classList.contains('hidden')) out.errors.push('act260: Cancel after typing did not ask about discarding');
        else { await click('confirm-ok'); await sleep(300); }
        if (!document.getElementById('modal').classList.contains('hidden')) out.errors.push('act260: the editor stayed open after confirming the discard');
      } else out.errors.push('act260: the editor did not reopen');
      const free260 = [...document.querySelectorAll('#grid .cell')].find(c => !c.firstElementChild);
      if (!free260) out.errors.push('act260: no free cell to create a prompt in');
      else {
        free260.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(300);
        const create260 = footer260().join();
        if (create260 !== 'modal-cancel,modal-confirm') out.errors.push('act260: the create footer is ' + create260);
        await click('modal-close-x');
        await sleep(250);
        if (!document.getElementById('confirm-modal').classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
      }
    }
    // ico261: the tile-picture well sits before the name for a prompt without media and
    // is gone for a prompt that has a picture.
    {
      const closeEd261 = async () => {
        await click('modal-close-x');
        await sleep(250);
        if (!document.getElementById('confirm-modal').classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
      };
      if (await openEditor('p1')) {
        const well = document.getElementById('modal-add-icon');
        const nm261 = document.getElementById('modal-name');
        if (!well || well.classList.contains('hidden') || getComputedStyle(well).display === 'none') out.errors.push('ico261: the tile-picture well is not shown for a prompt without media');
        else {
          if (well.getBoundingClientRect().right > nm261.getBoundingClientRect().left + 1) out.errors.push('ico261: the tile-picture well is not left of the name field');
          const aria261 = well.getAttribute('aria-label') || '';
          if (!aria261 || aria261 !== well.title) out.errors.push('ico261: the well has no matching accessible name (' + aria261 + ')');
          if (document.documentElement.lang === 'en' && aria261 !== 'Choose a tile picture') out.errors.push('ico261: the well is named "' + aria261 + '"');
          if (getComputedStyle(well).borderTopStyle !== 'dashed') out.errors.push('ico261: the well has no dashed frame');
        }
        await closeEd261();
      } else out.errors.push('ico261: the editor did not open for a text prompt');
      if (await openEditor('p11')) {
        const well = document.getElementById('modal-add-icon');
        if (well && !well.classList.contains('hidden') && getComputedStyle(well).display !== 'none') out.errors.push('ico261: the tile-picture well shows for a prompt that already has media');
        await closeEd261();
      } else out.errors.push('ico261: the editor did not open for the image prompt');
    }
    // fit262: the prompt field grows with its content up to the dialog height, then
    // scrolls inside itself; the name row stays reachable; the history cap still wins.
    {
      const closeEd262 = async () => {
        await click('modal-close-x');
        await sleep(250);
        if (!document.getElementById('confirm-modal').classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
      };
      if (await openEditor('p1')) {
        const ta262 = document.getElementById('modal-text');
        const box262 = document.querySelector('#modal .modal');
        const flag262 = () => document.getElementById('modal-md-wrap').classList.contains('hidden') ? 'wrap-hidden' : 'wrap-shown';
        const setText262 = async (n) => {
          ta262.value = Array.from({ length: n }, (_, i) => 'line ' + (i + 1)).join('\\n');
          ta262.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(200);
          return ta262.getBoundingClientRect().height;
        };
        if (document.activeElement !== ta262) { document.getElementById('modal-md-preview').click(); await sleep(150); }
        const h3 = await setText262(3);
        // A tall window leaves free height the field takes anyway (flex), so the upper
        // bounds only mean something where the dialog has no room to spare.
        const snug262 = innerHeight < 700;
        if (h3 < 110 || (snug262 && h3 > 140)) out.errors.push('fit262: a 3 line prompt field is ' + Math.round(h3) + 'px tall');
        const h60 = await setText262(60);
        if (h60 < 200) out.errors.push('fit262: a 60 line prompt field stays ' + Math.round(h60) + 'px tall (fit ' + box262.style.getPropertyValue('--ta-fit') + ', box ' + box262.clientHeight + ', active ' + (document.activeElement && document.activeElement.id) + ', hidden ' + ta262.classList.contains('hidden') + ', md ' + flag262() + ')');
        if (!(ta262.scrollHeight > ta262.clientHeight)) out.errors.push('fit262: a 60 line prompt field does not scroll inside itself');
        box262.scrollTop = 0;
        await sleep(50);
        if (document.querySelector('#modal .modal-name-row').getBoundingClientRect().top < box262.getBoundingClientRect().top - 1) out.errors.push('fit262: the name row is scrolled out of the dialog');
        const h1 = await setText262(1);
        if (snug262 && h1 > 140) out.errors.push('fit262: the field stays ' + Math.round(h1) + 'px tall after deleting back to 1 line');
        ta262.value = '';
        await closeEd262();
      } else out.errors.push('fit262: the editor did not open');
      if (await openEditor('p6')) {
        document.getElementById('modal-advanced-head').click();
        await sleep(300);
        const box262 = document.querySelector('#modal .modal');
        const field262 = document.getElementById('modal-md-wrap').classList.contains('hidden') ? document.getElementById('modal-text') : document.getElementById('modal-md-wrap');
        if (box262.style.getPropertyValue('--ta-fit')) out.errors.push('fit262: the fit stays set while the history is open');
        if (field262.getBoundingClientRect().height > innerHeight * 0.26 + 2) out.errors.push('fit262: the field is ' + Math.round(field262.getBoundingClientRect().height) + 'px tall with the history open');
        await closeEd262();
      } else out.errors.push('fit262: the history prompt did not open');
    }
    // var263-reopen: a prompt that already holds a placeholder opens with the bar and no
    // link; with the placeholder feature off neither is there.
    {
      const closeEd263 = async () => {
        await click('modal-close-x');
        await sleep(250);
        if (!document.getElementById('confirm-modal').classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
      };
      const shown263 = (id) => { const el = document.getElementById(id); return !!el && !el.classList.contains('hidden') && getComputedStyle(el).display !== 'none'; };
      if (await openEditor('p1')) {
        if (!shown263('modal-text-bar')) out.errors.push('var263-reopen: the bar is hidden for a prompt with a placeholder');
        if (shown263('modal-vars-link')) out.errors.push('var263-reopen: the link shows although the bar is there');
        await closeEd263();
      } else out.errors.push('var263-reopen: the placeholder prompt did not open');
      if (await setExpertFlag('Placeholders ({#{Name}#})', false)) {
        if (await openEditor('p0')) {
          if (shown263('modal-text-bar') || shown263('modal-vars-link')) out.errors.push('var263-reopen: the placeholder UI shows with the feature off');
          await closeEd263();
        } else out.errors.push('var263-reopen: the editor did not open with the feature off');
        await setExpertFlag('Placeholders ({#{Name}#})', true);
      } else out.errors.push('var263-reopen: the placeholder switch is missing from the expert menu');
    }
    // Gating: a row whose feature is off must disappear the moment that toggle is
    // flipped - no reopening the menu - and a whole branch goes with its parent.
    const rowFor = (label) => [...document.querySelectorAll('#expert-flags .field')]
      .find(r => ((r.querySelector('span') || {}).textContent || '').trim().startsWith(label));
    const flip = async (label) => {
      const row = rowFor(label);
      if (!row) return false;
      row.querySelector('input.switch').click();
      await sleep(300);
      return true;
    };
    document.getElementById('settings').classList.remove('hidden');
    await click('expert-open');
    if (!await expertRow('Multiple views')) {
      out.errors.push('the multiple-views toggle is missing from the expert menu');
    } else {
      // The parent of a branch takes its children with it and gives them back.
      for (const kept of ['Prompt in multiple views', 'View tab border width']) {
        if (!rowFor(kept)) out.errors.push('"' + kept + '" is missing although multiple views are on');
      }
      await flip('Multiple views');
      for (const gone of ['Prompt in multiple views', 'View tab border width']) {
        if (rowFor(gone)) out.errors.push('"' + gone + '" survived switching multiple views off');
      }
      if (!rowFor('Multiple views')) out.errors.push('switching multiple views off also hid its own toggle');
      await flip('Multiple views');
      for (const back of ['Prompt in multiple views', 'View tab border width']) {
        if (!rowFor(back)) out.errors.push('"' + back + '" did not come back');
      }
    }
    // The same for a sub-toggle: the preview owns the highlighter and its size.
    if (!await expertRow('Markdown preview')) {
      out.errors.push('the Markdown preview toggle is missing from the expert menu');
    } else {
      if (!rowFor('Syntax colors in code blocks')) {
        out.errors.push('the highlighter toggle is missing although the preview is on');
      }
      await flip('Markdown preview');
      for (const gone of ['Syntax colors in code blocks', 'Detect code without fences', 'Code text size']) {
        if (rowFor(gone)) out.errors.push('"' + gone + '" survived switching the preview off');
      }
      await flip('Markdown preview');
      if (!rowFor('Detect code without fences')) out.errors.push('the code detection toggle did not come back');
    }
    const firstTab2 = document.querySelector('#expert-tabs-bar .expert-tab');
    if (firstTab2) { firstTab2.click(); await sleep(150); }
    await click('expert-back');
    document.getElementById('settings').classList.add('hidden');
    // The tab bar: one plain tab per view, a right-click menu with the move entries,
    // and a drag that only ever reorders.
    const viewTabs = () => [...document.querySelectorAll('#views .view-btn:not(.view-add-top)')];
    const tabIds = () => viewTabs().map(b => b.dataset.viewId);
    if (viewTabs().length !== 4) {
      out.errors.push('the tab bar shows ' + viewTabs().length + ' tabs instead of 4');
    } else {
      if (viewTabs().some(b => !b.dataset.viewId)) out.errors.push('a tab is not a plain view tab');
      if (document.querySelector('#views .view-group-btn')) out.errors.push('a folder tab is still drawn for views');
      const startOrder = tabIds();
      const tabCentre = (el) => { const r = el.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
      const tabPtr = (type, x, y, el) => (el || window).dispatchEvent(new PointerEvent(type, {
        bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 3, clientX: x, clientY: y }));
      if (getComputedStyle(viewTabs()[0]).userSelect !== 'none') {
        out.errors.push('a view tab still selects its text while being dragged');
      }
      // The strip stays hit-testable at every window size the app allows, so the
      // gesture is testable everywhere: no width may skip this.
      for (const tb of viewTabs()) {
        // The strip scrolls when squeezed, so a user brings a tab into view first.
        tb.scrollIntoView({ block: 'nearest', inline: 'nearest' });
        await sleep(40);
        const [cx, cy] = tabCentre(tb);
        const hit = document.elementFromPoint(cx, cy);
        if (!hit || hit.closest('.view-btn') !== tb) {
          out.errors.push('a view tab cannot be dropped on: nothing hit-tests to "' +
            tb.textContent.trim().slice(0, 12) + '"');
        }
      }
      // Context menu: grid size, edit, duplicate, move left, move right (no group entry).
      viewTabs()[0].dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(200);
      const menuBtns = [...document.querySelectorAll('.tab-menu:not(.hidden) button')];
      if (menuBtns.length !== 5) {
        out.errors.push('a tab menu has ' + menuBtns.length + ' entries instead of 5');
      } else {
        if (!menuBtns[3].disabled) out.errors.push('move-left is not disabled on the first tab');
        if (menuBtns[4].disabled) out.errors.push('move-right is disabled although another tab follows');
        menuBtns[4].click();
        await sleep(250);
        if (tabIds()[1] !== startOrder[0]) out.errors.push('move-right on the first tab did not move it one place over');
        // Move it back so the order below matches what the rest of the run expects.
        viewTabs()[1].dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
        await sleep(200);
        [...document.querySelectorAll('.tab-menu:not(.hidden) button')][3]?.click();
        await sleep(250);
        if (tabIds().join() !== startOrder.join()) out.errors.push('move-left did not restore the tab order');
      }
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      await sleep(150);
      // A drag onto the right half of another tab reorders, marks the edge and
      // never opens a naming dialog or groups anything.
      calls().length = 0;
      const dragFrom = viewTabs()[0];
      const dragTo = viewTabs()[2];
      dragTo.scrollIntoView({ block: 'nearest', inline: 'nearest' });
      await sleep(50);
      const [ax, ay] = tabCentre(dragFrom);
      const tr = dragTo.getBoundingClientRect();
      tabPtr('pointerdown', ax, ay, dragFrom);
      tabPtr('pointermove', ax + 8, ay);
      await sleep(80);
      tabPtr('pointermove', tr.right - 3, tr.top + tr.height / 2);
      await sleep(80);
      if (!document.querySelector('.tab-drop-edge.tab-drop-after')) out.errors.push('dragging onto a tab shows no reorder mark');
      if (document.querySelector('.tab-drop:not(.tab-drop-edge)')) out.errors.push('dragging onto a tab shows a merge mark');
      tabPtr('pointerup', tr.right - 3, tr.top + tr.height / 2);
      await sleep(350);
      if (!calls().includes('reorder_views')) out.errors.push('dropping on a tab did not reorder the tab bar');
      if (viewTabs().length !== 4) out.errors.push('a tab drag changed the number of tabs');
      const dropped = tabIds();
      if (dropped[2] !== startOrder[0] || dropped.slice().sort().join() !== startOrder.slice().sort().join()) {
        out.errors.push('the dragged tab did not land after its target (' + dropped.join(',') + ')');
      }
      if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
        out.errors.push('a tab drag opened a dialog');
        await click('confirm-cancel');
      }
      // Put the order back for the probes that follow.
      {
        const back = viewTabs().find(b => b.dataset.viewId === startOrder[0]);
        const first = viewTabs()[0];
        first.scrollIntoView({ block: 'nearest', inline: 'nearest' });
        await sleep(50);
        const [bx, by] = tabCentre(back);
        const fr = first.getBoundingClientRect();
        tabPtr('pointerdown', bx, by, back);
        tabPtr('pointermove', bx - 8, by);
        await sleep(80);
        tabPtr('pointermove', fr.left + 3, fr.top + fr.height / 2);
        await sleep(80);
        tabPtr('pointerup', fr.left + 3, fr.top + fr.height / 2);
        await sleep(350);
        if (tabIds().join() !== startOrder.join()) out.errors.push('dragging a tab back did not restore the tab order');
      }
      // The probes that follow want a view with free cells under them.
      viewTabs().find(b => b.dataset.viewId === 'archive')?.click();
      await sleep(300);
    }
    document.getElementById('settings').classList.remove('hidden');
    // Custom themes: the stored one is offered in the theme list, the editor opens
    // with a colour row per themable token, and editing previews on the spot.
    const themeSel = document.getElementById('theme-select');
    if (![...themeSel.options].some(o => o.value === 'custom:ct1')) {
      out.errors.push('the stored custom theme is missing from the theme list');
    }
"""
