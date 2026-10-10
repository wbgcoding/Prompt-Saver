SECTION = """\
    // 88: import preview - shows new vs. duplicate before anything is imported.
    // Cancel only here (not confirm): confirming would push fake prompts into
    // the shared fixture and could throw off every check after this one in the
    // same probe run.
    {
      const importBtn = document.getElementById('import-btn');
      if (!importBtn) {
        out.errors.push('the settings page has no import button');
      } else {
        importBtn.click();
        await sleep(300);
        const modal = document.getElementById('import-preview-modal');
        if (!modal || modal.classList.contains('hidden')) {
          out.errors.push('the import preview did not open');
        } else {
          const rows = [...document.querySelectorAll('#import-preview-list .import-preview-row')];
          if (rows.length !== 3) out.errors.push('the import preview lists ' + rows.length + ' rows, expected 3');
          const dupRows = rows.filter(r => r.classList.contains('dup'));
          if (dupRows.length !== 1) out.errors.push('the import preview marks ' + dupRows.length + ' rows as duplicate, expected 1');
          if (dupRows.length && !dupRows[0].querySelector('.import-preview-badge')) {
            out.errors.push('a duplicate row has no badge');
          }
          const confirmBtn = document.getElementById('import-preview-confirm');
          if (confirmBtn.disabled) out.errors.push('the import preview confirm button is disabled although 2 items are new');
          document.getElementById('import-preview-cancel').click();
          await sleep(200);
          if (!modal.classList.contains('hidden')) out.errors.push('canceling the import preview did not close it');
        }
      }
    }
    // 446: import from a folder -- tags/size/a checkbox per row, an unreadable
    // file grayed out with its reason, and the "off by default" checkbox is
    // never touched here (cancel only, same reasoning as the import preview above).
    {
      const folderBtn = document.getElementById('import-folder-btn');
      if (!folderBtn) {
        out.errors.push('the settings page has no import-from-folder button');
      } else {
        folderBtn.click();
        await sleep(300);
        const modal = document.getElementById('import-folder-modal');
        if (!modal || modal.classList.contains('hidden')) {
          out.errors.push('the folder import preview did not open');
        } else {
          // 521: the dialog gets a real title instead of Windows' generic chrome.
          const folderCalls521 = (window.__SMOKE_ARGS__ || [])
            .filter(([cmd]) => cmd === 'preview_import_folder');
          const lastCall521 = folderCalls521[folderCalls521.length - 1];
          if (!lastCall521 || !lastCall521[1] || !lastCall521[1].title) {
            out.errors.push('521: preview_import_folder was not called with a title');
          }
          const rows = [...document.querySelectorAll('#import-folder-list .import-preview-row')];
          if (rows.length !== 3) out.errors.push('the folder import preview lists ' + rows.length + ' rows, expected 3');
          const badRows = rows.filter(r => r.classList.contains('dup'));
          if (badRows.length !== 1) out.errors.push('the folder import preview marks ' + badRows.length + ' rows unreadable, expected 1');
          if (badRows.length && badRows[0].querySelector('input[type=checkbox]')) {
            out.errors.push('an unreadable folder-import row still has a checkbox');
          }
          if (badRows.length && !badRows[0].querySelector('.import-preview-badge')) {
            out.errors.push('an unreadable folder-import row has no reason badge');
          }
          const goodRows = rows.filter(r => !r.classList.contains('dup'));
          const checked = goodRows.filter(r => (r.querySelector('input[type=checkbox]') || {}).checked);
          if (checked.length !== goodRows.length) out.errors.push('a readable folder-import row does not start checked');
          const tagged = goodRows.find(r => (r.querySelector('.import-preview-name') || {}).textContent === 'Meeting');
          if (!tagged || !tagged.querySelector('.import-preview-badge')) {
            out.errors.push('a folder-import row from a subfolder shows no tag badge');
          }
          document.getElementById('import-folder-select-none').click();
          await sleep(100);
          const confirmBtn = document.getElementById('import-folder-confirm');
          if (!confirmBtn.disabled) out.errors.push('the folder import confirm button stays enabled with none selected');
          document.getElementById('import-folder-select-all').click();
          await sleep(100);
          if (confirmBtn.disabled) out.errors.push('the folder import confirm button stays disabled after selecting all');
          document.getElementById('import-folder-cancel').click();
          await sleep(200);
          if (!modal.classList.contains('hidden')) out.errors.push('canceling the folder import did not close it');
        }
      }
    }
    // 339: settings export control -- one "Export" button plus a CSV/TXT/JSON
    // format select (replaces the old two hard-coded CSV/TXT buttons).
    {
      const fmtSel = document.getElementById('export-format');
      const expBtn = document.getElementById('export-btn');
      if (!fmtSel || !expBtn) {
        out.errors.push('the settings export control (format select + button) is missing');
      } else {
        const optValues = [...fmtSel.options].map(o => o.value);
        if (!['csv', 'txt', 'json'].every(v => optValues.includes(v))) {
          out.errors.push('the export format select is missing csv/txt/json, has: ' + optValues.join(','));
        }
        // 371, 726: the format select follows the Export button in the pair --
        // neither element sets its own tabindex, so document order is also tab order here.
        const posFmtExp = expBtn.compareDocumentPosition(fmtSel);
        if (!(posFmtExp & Node.DOCUMENT_POSITION_FOLLOWING)) {
          out.errors.push('the export format select does not follow the Export button');
        }
        if (fmtSel.hasAttribute('tabindex') || expBtn.hasAttribute('tabindex')) {
          out.errors.push('an explicit tabindex on the format select or Export button would override document-order tabbing');
        }
        window.__EXPORT_CALL__ = null;
        fmtSel.value = 'json';
        fmtSel.dispatchEvent(new Event('change', { bubbles: true }));
        expBtn.click();
        await sleep(200);
        if (!window.__EXPORT_CALL__ || window.__EXPORT_CALL__.format !== 'json') {
          out.errors.push('choosing JSON and clicking Export did not call export_prompts with format "json"');
        }
      }
    }
    // 340: recordingDir shows the folder's video count/size plus a cleanup
    // button; the button opens a modal that previews what an age tier would
    // delete and only deletes after Delete is clicked -- Cancel must leave
    // the files alone.
    {
      const pathFieldRow = async (label) => {
        const hit = () => [...document.querySelectorAll('.path-field')]
          .find(r => ((r.querySelector('.value-head span') || {}).textContent || '') === label);
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
      const row = await pathFieldRow('Recording folder');
      if (!row) {
        out.errors.push('the recording-folder path row is missing from the expert menu');
      } else {
        await sleep(150); // refreshVideoFolderInfo() resolves and re-renders
        const statsEl = row.querySelector('.path-video-stats span');
        if (!statsEl || statsEl.textContent !== '2 files, 5.0 MB') {
          out.errors.push('the recording folder stats line reads "' + (statsEl && statsEl.textContent) + '"');
        }
        const cleanBtn = row.querySelector('.path-video-stats button');
        if (!cleanBtn || cleanBtn.disabled) {
          out.errors.push('the video cleanup button is missing or disabled although the folder has files');
        } else {
          cleanBtn.click();
          await sleep(200);
          const modal = document.getElementById('video-cleanup-modal');
          const ageSel = document.getElementById('video-cleanup-age');
          const summary = document.getElementById('video-cleanup-summary');
          const list = document.getElementById('video-cleanup-list');
          if (!modal || modal.classList.contains('hidden')) {
            out.errors.push('the video cleanup button did not open its confirm modal');
          } else {
            // 389: with nothing discarded yet, the trash section stays hidden --
            // window.__DISCARDED_FIXTURE__ is unset at this point in the probe.
            const discardedSection = document.getElementById('video-cleanup-discarded-section');
            if (!discardedSection || !discardedSection.classList.contains('hidden')) {
              out.errors.push('the discard-trash section shows although nothing was discarded');
            }
            if (ageSel.value !== '30') out.errors.push('the video cleanup age select does not default to 30 days');
            const rows = [...list.querySelectorAll('.import-preview-row')];
            if (rows.length !== 1) {
              out.errors.push('the video cleanup preview lists ' + rows.length + ' rows, expected 1');
            } else {
              if ((rows[0].querySelector('.import-preview-name') || {}).textContent !== 'old-recording.mp4') {
                out.errors.push('the video cleanup preview row has the wrong file name');
              }
              if ((rows[0].querySelector('.import-preview-badge') || {}).textContent !== '3.0 MB') {
                out.errors.push('the video cleanup preview row has the wrong size badge');
              }
            }
            if (!(summary.textContent || '').includes('3.0 MB')) {
              out.errors.push('the video cleanup summary reads "' + summary.textContent + '"');
            }
            // A checkbox per row plus Select all/none feeds "Export
            // selected", which opens its own small dialog for format,
            // quality and destination, then calls the batch command with
            // resolved source and destination paths.
            {
              const exportBtn = document.getElementById('video-cleanup-export');
              if (exportBtn.disabled !== true) out.errors.push('the export-selected button starts enabled although nothing is checked');
              document.getElementById('video-cleanup-select-all').click();
              if (exportBtn.disabled) out.errors.push('Select all did not enable the export-selected button');
              document.getElementById('video-cleanup-select-none').click();
              if (!exportBtn.disabled) out.errors.push('Select none did not disable the export-selected button');
              document.getElementById('video-cleanup-select-all').click();
              window.__PICK_FOLDER__ = 'C:\\\\Export\\\\Out';
              window.__BATCH_EXPORT_CALLED__ = null;
              exportBtn.click();
              await sleep(200);
              const bModal = document.getElementById('batch-export-modal');
              if (!bModal || bModal.classList.contains('hidden')) {
                out.errors.push('Export selected did not open the batch-export dialog');
              } else {
                const bSummary = document.getElementById('batch-export-summary');
                if (!(bSummary.textContent || '').includes('1')) {
                  out.errors.push('the batch export summary does not mention the 1 selected recording');
                }
                document.getElementById('batch-export-choose-dest').click();
                await sleep(150);
                const destOut = document.getElementById('batch-export-dest-out');
                if ((destOut.textContent || '') !== 'C:\\\\Export\\\\Out') {
                  out.errors.push('choosing a destination folder did not show its path');
                }
                document.getElementById('batch-export-start').click();
                await sleep(200);
                if (!window.__BATCH_EXPORT_CALLED__) {
                  out.errors.push('Start did not call export_recordings_batch');
                } else {
                  const items = window.__BATCH_EXPORT_CALLED__.items || [];
                  if (items.length !== 1 || items[0].dest_path !== 'C:\\\\Export\\\\Out\\\\old-recording.mp4') {
                    out.errors.push('export_recordings_batch got the wrong item paths: ' + JSON.stringify(items));
                  }
                }
                if (!bModal.classList.contains('hidden')) out.errors.push('the batch-export dialog did not close after Start');
                if (modal.classList.contains('hidden')) out.errors.push('the video-cleanup modal did not reopen after a batch export');
              }
              window.__PICK_FOLDER__ = null;
            }
            // Changing the age re-runs the preview with the new value.
            window.__SMOKE_ARGS__ = [];
            ageSel.value = '7';
            ageSel.dispatchEvent(new Event('change', { bubbles: true }));
            await sleep(200);
            const previewCalls = window.__SMOKE_ARGS__.filter(([c]) => c === 'video_folder_cleanup_preview');
            if (!previewCalls.length || previewCalls[previewCalls.length - 1][1].ageDays !== 7) {
              out.errors.push('changing the cleanup age did not re-run the preview with the new value');
            }
            // Cancel must close without deleting anything.
            window.__VIDEO_CLEANUP_CALLED__ = null;
            document.getElementById('video-cleanup-cancel').click();
            await sleep(200);
            if (!modal.classList.contains('hidden')) out.errors.push('canceling the video cleanup did not close the modal');
            if (window.__VIDEO_CLEANUP_CALLED__) out.errors.push('canceling the video cleanup deleted files anyway');
            // Reopen (resets to the 30-day default) and confirm: this time it
            // must actually delete and report back.
            cleanBtn.click();
            await sleep(200);
            document.getElementById('video-cleanup-confirm').click();
            await sleep(200);
            if (!modal.classList.contains('hidden')) out.errors.push('confirming the video cleanup did not close the modal');
            if (!window.__VIDEO_CLEANUP_CALLED__ || window.__VIDEO_CLEANUP_CALLED__.ageDays !== 30) {
              out.errors.push('confirming the video cleanup did not call video_folder_cleanup with the shown age');
            }
            const toastEl3 = document.getElementById('toast');
            if (!(toastEl3.textContent || '').includes('1 files deleted')) {
              out.errors.push('the video cleanup done toast reads "' + toastEl3.textContent + '"');
            }
            // 389: now with one discarded recording waiting -- the trash section
            // must appear, list it, and its Restore button must call the right command.
            window.__DISCARDED_FIXTURE__ = [{ name: 'rec-discarded.mp4', size: 1048576 }];
            cleanBtn.click();
            await sleep(200);
            if (discardedSection.classList.contains('hidden')) {
              out.errors.push('the discard-trash section stays hidden although a recording was discarded');
            } else {
              const dRows = [...document.getElementById('video-cleanup-discarded-list').querySelectorAll('.import-preview-row')];
              if (dRows.length !== 1) {
                out.errors.push('the discard-trash list shows ' + dRows.length + ' rows, expected 1');
              } else {
                if ((dRows[0].querySelector('.import-preview-name') || {}).textContent !== 'rec-discarded.mp4') {
                  out.errors.push('the discard-trash row has the wrong file name');
                }
                const restoreBtn = [...dRows[0].querySelectorAll('.import-preview-actions button')][0];
                window.__SMOKE_ARGS__ = [];
                restoreBtn.click();
                await sleep(200);
                const restoreCall = window.__SMOKE_ARGS__.find(([c]) => c === 'restore_discarded_recording');
                if (!restoreCall || restoreCall[1].name !== 'rec-discarded.mp4') {
                  out.errors.push('clicking Restore on a discarded recording did not call restore_discarded_recording with its name');
                }
              }
            }
            window.__DISCARDED_FIXTURE__ = null;
            document.getElementById('video-cleanup-close').click();
            await sleep(150);
          }
        }
      }
      const tab0b = document.querySelector('#expert-tabs-bar .expert-tab');
      if (tab0b) { tab0b.click(); await sleep(150); }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    const DRAG_OUT = 'into another app';
    const offTile = gridTile('p1');
    if (offTile) {
      // Ships off: the modifier must not arm anything.
      press(offTile, { ctrlKey: true });
      if (offTile.draggable) out.errors.push('a tile is drag-out armed although the feature is off');
      abort();
      await sleep(100);
    }
    if (!await setExpertFlag(DRAG_OUT, true)) {
      out.errors.push('the drag-out option is missing from the expert menu');
    } else {
      // Plain drag: unchanged. The ghost proves the reorder engaged; dropping it on a
      // free cell has to save the new layout.
      calls().length = 0;
      const tile = gridTile('p1');
      const origCell = tile.closest('.cell');
      const origCol = origCell && origCell.dataset.col, origRow = origCell && origCell.dataset.row;
      press(tile, {});
      if (tile.draggable) out.errors.push('a plain press armed a drag-out');
      window.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX: 300, clientY: 200 }));
      await sleep(150);
      if (!document.querySelector('.drag-ghost')) out.errors.push('a plain drag no longer starts the tile reorder');
      const cell = [...document.querySelectorAll('#grid .cell')].find(c => {
        const b = c.getBoundingClientRect();
        return b.width && b.right < innerWidth && b.bottom < innerHeight;
      });
      const b = cell && cell.getBoundingClientRect();
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true,
        clientX: b ? b.left + b.width / 2 : 300, clientY: b ? b.top + b.height / 2 : 200 }));
      await sleep(350);
      if (document.querySelector('.drag-ghost')) out.errors.push('the drag ghost outlived the drag');
      if (cell && !calls().includes('set_layout')) {
        out.errors.push('a plain drag did not save a new tile position');
      }
      // P8: the same reorder must offer an undo toast. Click it and prove the tile
      // really lands back on its original cell (not just that a class flips), then
      // leave the toast the way its own click handler leaves it (hideToast()) - no
      // stray classList surgery, and nothing lingers into later checks.
      if (cell && origCell) {
        const undoToast = document.getElementById('toast');
        const undoBtn = undoToast.querySelector('.toast-btn');
        if (!undoToast.classList.contains('actionable') || !undoBtn) {
          out.errors.push('a plain drag did not offer an undo toast');
        } else {
          undoBtn.click();
          await sleep(300);
          const backCell = gridTile('p1')?.closest('.cell');
          if (!backCell || backCell.dataset.col !== origCol || backCell.dataset.row !== origRow) {
            out.errors.push('undo did not put the tile back on its original cell');
          }
          if (undoToast.classList.contains('show')) out.errors.push('the undo toast is still showing after being clicked');
        }
      }
      // Modifier drag: armed, and the payload is the raw text - placeholders included,
      // because filling them needs a dialog no drag can open.
      const varTile2 = gridTile('p1');
      press(varTile2, { ctrlKey: true });
      if (!varTile2.draggable) out.errors.push('the modifier did not arm the drag-out');
      const ev = fireDragStart(varTile2);
      const dropped = ev.dataTransfer.getData('text/plain');
      if (dropped !== 'Translate {?Text=Hello} into {?Language}.') {
        out.errors.push('the dragged text is wrong: ' + dropped);
      }
      if (document.querySelector('.drag-ghost')) out.errors.push('a drag-out left a reorder ghost behind');
      // Once the OS owns the gesture a stray pointerup must not ALSO copy the prompt
      // (p1 asks for placeholders, so a copy would show up as the placeholder window).
      calls().length = 0;
      window.__VAR_PAYLOAD__ = undefined;
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, clientX: 20, clientY: 20 }));
      await sleep(200);
      if (varReq()) {
        out.errors.push('a drag-out also started a copy');
        await varCancel();
      }
      if (calls().includes('copy_prompt')) out.errors.push('a drag-out also copied the prompt');
      varTile2.dispatchEvent(new DragEvent('dragend', { bubbles: true }));
      await sleep(100);
      if (varTile2.draggable) out.errors.push('the tile stayed armed after the drag ended');
      // A file-backed prompt drops its path plus a URI for targets that read one.
      const fileTile = gridTile('p9');
      if (!fileTile) {
        out.errors.push('the file-backed prompt is missing from the grid');
      } else {
        press(fileTile, { ctrlKey: true });
        const fev = fireDragStart(fileTile);
        if (fev.dataTransfer.getData('text/plain') !== 'C:\\\\Notes\\\\brief.txt') {
          out.errors.push('a file prompt did not drop its path: ' + fev.dataTransfer.getData('text/plain'));
        }
        if (fev.dataTransfer.getData('text/uri-list') !== 'file:///C:/Notes/brief.txt') {
          out.errors.push('a file prompt dropped no file URI: ' + fev.dataTransfer.getData('text/uri-list'));
        }
        fileTile.dispatchEvent(new DragEvent('dragend', { bubbles: true }));
      }
      // The library rows are drag sources too.
      await click('library-btn');
      const libRow = document.querySelector('#library-list .lib-item');
      if (!libRow) {
        out.errors.push('the library has no row for the drag-out check');
      } else {
        press(libRow, { ctrlKey: true });
        if (!libRow.draggable) out.errors.push('a library row cannot be dragged out');
        if (!fireDragStart(libRow).dataTransfer.getData('text/plain')) {
          out.errors.push('a library row dropped no text');
        }
        libRow.dispatchEvent(new DragEvent('dragend', { bubbles: true }));
      }
      await click('library-close');
      // Switching it off takes the whole behaviour with it - including on a tile that
      // is already wired up, which is the only way a stale drag could still fire.
      await setExpertFlag(DRAG_OUT, false);
      press(varTile2, { ctrlKey: true });
      if (varTile2.draggable) out.errors.push('the modifier still arms a drag-out after switching it off');
      if (!fireDragStart(varTile2).defaultPrevented) {
        out.errors.push('a drag-out was not refused after switching the feature off');
      }
      abort();
      await sleep(100);
    }
    // F14/F15: the preview renders every Markdown construct, colours the fenced code
    // blocks, and - the part that matters - shows injected markup as literal text.
    // Ships off now, so the test has to switch it on itself.
    if (!await setExpertFlag('Markdown preview', true)) {
      out.errors.push('the Markdown preview option is missing from the expert menu');
    }
    const openEditor = async (id) => {
      const el = [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
      if (!el) return false;
      el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(200);
      const edit = document.querySelector('#ctx [data-act="edit"]');
      if (!edit) return false;
      edit.click();
      await sleep(400);
      return !document.getElementById('modal').classList.contains('hidden');
    };
    if (!await openEditor('p7')) {
      out.errors.push('the Markdown prompt did not open in the editor');
    } else {
      const prev = document.getElementById('modal-md-preview');
      const pwrap = document.getElementById('modal-md-wrap');
      const ta = document.getElementById('modal-text');
      // With the feature on the prompt opens RENDERED - no switch, no empty pane.
      if (!prev.innerHTML.trim()) {
        out.errors.push('the preview pane is empty before it is opened');
      }
      if (pwrap.classList.contains('hidden')) {
        out.errors.push('the prompt does not open rendered');
      } else {
        if (!ta.classList.contains('hidden')) {
          out.errors.push('the text field is still shown although the preview took its place');
        }
        // An empty or invisible pane is exactly the "preview shows nothing" bug.
        const box = prev.getBoundingClientRect();
        if (getComputedStyle(prev).display === 'none' || box.width < 1 || box.height < 1) {
          out.errors.push('the preview pane has no visible box: ' + Math.round(box.width) + 'x' + Math.round(box.height));
        }
        if (prev.children.length < 5) {
          out.errors.push('the preview rendered ' + prev.children.length + ' nodes - it is not showing the prompt');
        }
        for (const pair of [['h1', 'heading 1'], ['h2', 'heading 2'], ['strong', 'bold'],
                            ['em', 'italic'], ['code.md-inline-code', 'inline code'],
                            ['ul li', 'bullet list'], ['ol li', 'numbered list'],
                            ['blockquote', 'quote'], ['hr', 'rule'],
                            ['pre.md-code', 'code block'], ['.md-link', 'link']]) {
          if (!prev.querySelector(pair[0])) out.errors.push('the preview does not render ' + pair[1]);
        }
        // Nothing the user types may become an element that loads or navigates.
        if (prev.querySelector('a, img, script, iframe, [href], [src], [onerror], [onmouseover]')) {
          out.errors.push('the preview emitted an element that can load or navigate');
        }
        if (window.xssRan !== undefined) out.errors.push('script injected through a prompt ran');
        const shown = prev.textContent;
        for (const literal of ['<script>', 'onerror', '<b onmouseover', 'javascript:alert(1)', 'click me']) {
          if (!shown.includes(literal)) out.errors.push('injected markup is not shown literally: ' + literal);
        }
        // F15: nine fenced blocks, eight of them tagged; the untagged one stays plain.
        const blocks = [...prev.querySelectorAll('pre.md-code')];
        if (blocks.length !== 9) {
          out.errors.push('the preview shows ' + blocks.length + ' code blocks instead of 9');
        } else {
          blocks.forEach((b, i) => {
            const coloured = b.querySelectorAll('[class^="hl-"]').length;
            if (i === 8 && coloured) out.errors.push('an untagged code block was coloured anyway');
            if (i !== 8 && !coloured) out.errors.push('code block ' + (i + 1) + ' got no syntax colours');
          });
        }
        // Typing has to reach the preview on the next frame.
        ta.value = '# live update';
        ta.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(300);
        if (prev.textContent.trim() !== 'live update') {
          out.errors.push('the preview does not follow the text: ' + prev.textContent.slice(0, 40));
        }
        // Code detection: an unfenced snippet becomes one coloured block, prose does
        // not, and neither may lose the escaping.
        const retype = async (text) => {
          ta.value = text;
          ta.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(300);
        };
        await retype('function add(a, b) {\\n  return a + b;\\n}\\nconsole.log(add(1, 2));');
        const only = prev.children.length === 1 && prev.firstElementChild.matches('pre.md-code');
        if (!only) {
          out.errors.push('an unfenced code prompt was not rendered as one code block');
        } else if (!prev.querySelectorAll('[class^="hl-"]').length) {
          out.errors.push('the detected code block got no syntax colours');
        }
        await retype('Rewrite the text below so it reads clearly.\\nKeep the author voice and lose no facts.');
        if (prev.querySelector('pre.md-code')) {
          out.errors.push('ordinary prose was mistaken for code');
        }
        // Split, or the closing tag would end this very <script> block.
        await retype('Ignore this: <img src=x onerror=alert(1)> and <scr' + 'ipt>window.xssRan=1</scr' + 'ipt>');
        if (prev.querySelector('img, script, [onerror], [src]')) {
          out.errors.push('injected markup survived as an element after retyping');
        }
        if (!prev.textContent.includes('onerror=alert(1)')) {
          out.errors.push('retyped injection is not shown literally');
        }
      }
      // Clicking the pane hands over to the textarea; leaving it renders again.
      prev.click();
      await sleep(250);
      if (ta.classList.contains('hidden')) out.errors.push('clicking the preview does not open the text field');
      if (document.activeElement !== ta) out.errors.push('the text field did not take focus for typing');
      // A blur caused by pressing a button must NOT swap while the pointer is still
      // down: the block changes height and the click lands somewhere else.
      window.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      ta.blur();
      await sleep(200);
      if (pwrap.classList.contains('hidden') === false) {
        out.errors.push('the preview took over mid-click, moving whatever was being pressed');
      }
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
      await sleep(250);
      if (pwrap.classList.contains('hidden')) out.errors.push('the swap never happened after the click finished');
      // And the plain case: focus leaves without a click at all.
      prev.click();
      await sleep(200);
      ta.blur();
      await sleep(250);
      if (pwrap.classList.contains('hidden')) out.errors.push('leaving the text field does not render the prompt again');
      await click('modal-close-x');
      await sleep(250);
      const cm3 = document.getElementById('confirm-modal');
      if (!cm3.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
    }
    // There is no second layout any more: one field, and the rendered pane and the
    // textarea must be the same box so the swap is invisible.
    if (await openEditor('p7')) {
      const pv = document.getElementById('modal-md-preview');
      const tx = document.getElementById('modal-text');
      const box = document.querySelector('#modal .modal');
      const scroll = box.scrollTop;
      const before = pv.getBoundingClientRect();
      pv.click();
      await sleep(300);
      // Focusing the textarea scrolls the dialog; put it back or the comparison
      // measures the scroll instead of the layout.
      box.scrollTop = scroll;
      const after = tx.getBoundingClientRect();
      for (const k of ['left', 'right', 'top', 'width']) {
        if (Math.abs(before[k] - after[k]) > 1) {
          out.errors.push('the field moves when it hands over: ' + k + ' ' +
            Math.round(before[k]) + ' -> ' + Math.round(after[k]));
        }
      }
      const a = getComputedStyle(pv), b = getComputedStyle(tx);
      for (const prop of ['fontSize', 'lineHeight', 'padding', 'borderRadius', 'backgroundColor']) {
        if (a[prop] !== b[prop]) {
          out.errors.push('rendered pane and text field differ in ' + prop + ': ' + a[prop] + ' vs ' + b[prop]);
        }
      }
      await click('modal-close-x');
      await sleep(250);
      const cm4 = document.getElementById('confirm-modal');
      if (!cm4.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
    }
"""
