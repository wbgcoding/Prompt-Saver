SECTION = """\
    // F17: the search also looks inside the private notes. "Moonshot" lives only in
    // one prompt's note, so a hit can only have come from there.
    const q = document.getElementById('library-q');
    if (!q) {
      out.errors.push('the library has no search field');
    } else {
      q.value = 'moonshot';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const hits = document.querySelectorAll('#library-list .lib-item').length;
      if (hits !== 1) out.errors.push('searching a note matched ' + hits + ' prompts instead of 1');
      q.value = '';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(250);
    }
    // P18: a "-word" term excludes matches instead of narrowing to them -
    // "work" alone should hit more prompts than "work -draft".
    if (q) {
      q.value = 'work';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const withDraft = document.querySelectorAll('#library-list .lib-item').length;
      q.value = 'work -draft';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const withoutDraft = document.querySelectorAll('#library-list .lib-item').length;
      if (!(withoutDraft < withDraft)) {
        out.errors.push('"-draft" did not exclude anything: ' + withDraft + ' -> ' + withoutDraft);
      }
      q.value = '';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(250);
    }
    // 424: tag: and var: are hard filters -- typing one narrows the library
    // to exactly the prompts carrying that tag or placeholder, nothing scored.
    if (q) {
      // Earlier steps above created a couple of untagged test prompts, so the
      // total row count here is not the fixture's original 12 -- the
      // "everyone without Work" count is checked relative to it instead.
      const totalRows = document.querySelectorAll('#library-list .lib-item').length;
      q.value = 'tag:work';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const tagHits = document.querySelectorAll('#library-list .lib-item').length;
      if (tagHits !== 8) out.errors.push('424: "tag:work" matched ' + tagHits + ' prompts instead of 8');
      q.value = '-tag:work';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const tagExcludeHits = document.querySelectorAll('#library-list .lib-item').length;
      if (tagExcludeHits !== totalRows - tagHits) {
        out.errors.push('424: "tag:work" and "-tag:work" do not add up to the full list: ' + tagHits + ' + ' + tagExcludeHits + ' != ' + totalRows);
      }
      q.value = 'var:Text';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(300);
      const varHits = document.querySelectorAll('#library-list .lib-item').length;
      if (varHits !== 1) out.errors.push('424: "var:Text" matched ' + varHits + ' prompts instead of 1');
      q.value = '';
      q.dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(250);
      // 709: a placeholder reads as [name] in a library row, a reserved one too.
      const toneRow = document.querySelector('#library-list .lib-item[data-id="p5"] .lib-text');
      if (!toneRow || !toneRow.textContent.includes('[tone]') || toneRow.textContent.includes('{#{')) {
        out.errors.push('709: the library row of a prompt with a choice variable does not show [tone]: ' + (toneRow ? toneRow.textContent : 'row missing'));
      }
      const dynRow = document.querySelector('#library-list .lib-item[data-id="p3"] .lib-text');
      if (!dynRow || !dynRow.textContent.includes('[date]') || dynRow.textContent.includes('{#{')) {
        out.errors.push('709: the library row of a prompt with reserved names does not show [date]: ' + (dynRow ? dynRow.textContent : 'row missing'));
      }
    }
    // 710: "Close after copy" in the library head is a checkbox that follows the flag.
    {
      const closeBox = document.getElementById('library-close-toggle');
      const flagNow = async () => (await window.__TAURI__.core.invoke('get_settings')).ui_flags.closeAfterCopy !== false;
      if (!closeBox || closeBox.type !== 'checkbox') {
        out.errors.push('710: the library close-after-copy control is not a checkbox');
      } else {
        const before = await flagNow();
        if (closeBox.checked !== before) out.errors.push('710: the checkbox does not show the flag: ' + closeBox.checked + ' vs ' + before);
        closeBox.click();
        await sleep(250);
        if ((await flagNow()) === before || closeBox.checked === before) {
          out.errors.push('710: ticking the checkbox did not flip the flag (box ' + closeBox.checked + ', flag ' + (await flagNow()) + ')');
        }
        closeBox.click();
        await sleep(250);
        if ((await flagNow()) !== before || closeBox.checked !== before) {
          out.errors.push('710: ticking the checkbox again did not restore the flag');
        }
      }
    }
    // lay267: sort, tiles and select sit in the search row, "Close after copy" in a foot
    // under the list, "Find duplicates" in the title line.
    {
      const topOf267 = (el) => el.getBoundingClientRect().top;
      const q267 = document.getElementById('library-q');
      for (const id of ['library-sort', 'library-select-btn', 'library-view-toggle']) {
        const el = document.getElementById(id);
        if (!el || !el.closest('#library-search .lib-toolbar')) {
          out.errors.push('lay267: #' + id + ' is not inside the search row');
        } else if (id !== 'library-view-toggle' || getComputedStyle(el).display !== 'none') {
          if (Math.abs(topOf267(el) - topOf267(q267)) > 4) {
            out.errors.push('lay267: #' + id + ' does not share the top of the search field (' + topOf267(el) + ' vs ' + topOf267(q267) + ')');
          }
        }
      }
      const closeBox267 = document.getElementById('library-close-toggle');
      const list267 = document.getElementById('library-list');
      if (!closeBox267 || !closeBox267.closest('#library-foot')) {
        out.errors.push('lay267: the close-after-copy box is not inside the foot');
      } else if (closeBox267.getBoundingClientRect().top < list267.getBoundingClientRect().bottom - 1) {
        out.errors.push('lay267: the close-after-copy box is not below the list');
      }
      const dup267 = document.getElementById('library-dupes');
      const tabs267 = document.getElementById('library-tabs');
      if (Math.abs(dup267.getBoundingClientRect().top - tabs267.getBoundingClientRect().top) > 4) {
        out.errors.push('lay267: Find duplicates is not on the title line (' + dup267.getBoundingClientRect().top + ' vs ' + tabs267.getBoundingClientRect().top + ')');
      }
      // Every filter off, prompts present: the search row stays, the filter rows go.
      const coreL267 = await import('./core.js');
      const libMod267 = await import('./library.js');
      const flags267 = coreL267.settings.ui_flags;
      coreL267.settings.ui_flags = { ...(flags267 || {}), libraryTypeFilter: false, colorFilter: false, favorites: false, tags: false };
      libMod267.renderLibrary();
      await sleep(40);
      const search267 = document.getElementById('library-search');
      const tb267 = document.querySelector('#library-search .lib-toolbar');
      if (search267.classList.contains('hidden') || !tb267 || tb267.getBoundingClientRect().height < 10) {
        out.errors.push('lay267: with every filter off the search row is gone');
      }
      if (!document.getElementById('library-filter-rows').classList.contains('hidden')) {
        out.errors.push('lay267: with every filter off the filter rows are still shown');
      }
      coreL267.settings.ui_flags = flags267;
      libMod267.renderLibrary();
      await sleep(40);
      if (document.getElementById('library-filter-rows').classList.contains('hidden')) {
        out.errors.push('lay267: the filter rows did not come back with the flags');
      }
    }
    // 711: the colour filter offers only the colours the prompts really use, and
    // hides itself below two of them.
    {
      const invoke711 = window.__TAURI__.core.invoke;
      const state711 = await invoke711('get_state');
      const saved711 = state711.prompts.map((p) => [p.id, p.color]);
      const paint711 = async (colors) => {
        for (const [i, [id]] of saved711.entries()) await invoke711('update_prompt', { id, color: colors[i % colors.length] });
        window.__FIRE__('store-synced', null);
        await sleep(450);
      };
      const wrap711 = () => document.getElementById('library-colors');
      await paint711(['#ef4444', '#3b82f6']);
      const twoColours = wrap711().querySelectorAll('.lib-color').length;
      if (twoColours !== 3 || wrap711().classList.contains('hidden')) {
        out.errors.push('711: two used colours should give 3 filter chips (all + 2), got ' + twoColours);
      }
      await paint711(['#ef4444']);
      if (!wrap711().classList.contains('hidden')) out.errors.push('711: the colour filter stays visible with a single used colour');
      for (const [id, color] of saved711) await invoke711('update_prompt', { id, color });
      window.__FIRE__('store-synced', null);
      await sleep(450);
      const allColours = wrap711().querySelectorAll('.lib-color').length;
      if (allColours < 4 || wrap711().classList.contains('hidden')) {
        out.errors.push('711: the colour filter did not come back after the colours were restored: ' + allColours);
      }
    }
    // 712: the archive chip carries a word, not the file-cabinet emoji.
    {
      const archiveChip = document.getElementById('library-archive');
      const chipText = archiveChip ? archiveChip.textContent.trim() : '';
      if (chipText.length < 2 || chipText.charCodeAt(0) >= 0xD800) { // a word, not the emoji (a surrogate pair)
        out.errors.push('712: the archive chip does not show a word: "' + chipText + '"');
      }
    }
    // 714: the Prompts tab carries the count, the header spans only keep it for readers.
    {
      const promptsTab = document.getElementById('library-tab-prompts');
      const headCount = document.getElementById('library-count');
      const nPrompts = headCount ? headCount.textContent : '';
      const tabText = promptsTab ? promptsTab.textContent.trim() : '';
      if (!nPrompts || !tabText.endsWith('(' + nPrompts + ')')) {
        out.errors.push('714: the Prompts tab does not end with the count (' + nPrompts + '): "' + tabText + '"');
      }
      if (!headCount.classList.contains('hidden')) out.errors.push('714: the count in the library header is still visible');
    }
    // A library row with an image attachment opens the same actions menu as any
    // other row: edit, edit image, duplicate ... delete, not just "open in editor".
    {
      // Give the fixture row p7 a picture as its attached file for the length of
      // this check (the backend answers, a sync event makes the window refetch).
      const imgPath = 'C:\\\\Pictures\\\\ps-smoke-row.png';
      await window.__TAURI__.core.invoke('update_prompt', { id: 'p7', filePath: imgPath });
      window.__FIRE__('store-synced', null);
      await sleep(400);
      const imgRow = document.querySelector('#library-list .lib-item[data-id="p7"]');
      if (!imgRow) {
        out.errors.push('library image row: the fixture row p7 is missing');
      } else {
        // 713: a text row (colour dot) and a picture row start their title at the same x.
        const dot713 = document.querySelector('#library-list .lib-item > .dot');
        const textRow713 = dot713 && dot713.parentElement;
        const nameOf713 = (row) => row && row.querySelector('.lib-name');
        if (!nameOf713(textRow713) || !nameOf713(imgRow)) {
          out.errors.push('713: the rows to compare are missing');
        } else if (!imgRow.querySelector(':scope > .lib-thumb') || !textRow713.querySelector(':scope > .dot')) {
          out.errors.push('713: the picture row has no thumbnail or the text row no dot, so the alignment proves nothing');
        } else {
          const dx713 = Math.abs(nameOf713(textRow713).getBoundingClientRect().left - nameOf713(imgRow).getBoundingClientRect().left);
          if (dx713 > 1) out.errors.push('713: the title of a text row is ' + dx713.toFixed(1) + ' px off the title of a picture row');
        }
        imgRow.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 40, clientY: 40 }));
        await sleep(150);
        const ctxMenu = document.getElementById('ctx');
        const shown = (act) => { const b = ctxMenu.querySelector('[data-act="' + act + '"]'); return !!b && !b.classList.contains('hidden'); };
        if (ctxMenu.classList.contains('hidden')) out.errors.push('library image row: the right-click did not open the actions menu');
        else {
          for (const act of ['edit', 'editImage', 'duplicate', 'delete']) {
            if (!shown(act)) out.errors.push('library image row: the menu has no ' + act + ' entry');
          }
          if (shown('editVideo')) out.errors.push('library image row: the menu offers Edit video for a picture');
          if (document.querySelector('.open-editor-pop:not(.hidden)')) out.errors.push('library image row: the old open-in-editor popup opened instead of the menu');
          window.__SMOKE_ARGS__ = [];
          ctxMenu.querySelector('[data-act="editImage"]').click();
          await sleep(250);
          const openCall = (window.__SMOKE_ARGS__ || []).find((c) => c[0] === 'open_file_in_editor');
          if (!openCall || openCall[1].path !== imgPath) out.errors.push('library image row: Edit image did not open the attached file in the editor: ' + JSON.stringify(openCall));
        }
        document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
        await sleep(100);
      }
      await window.__TAURI__.core.invoke('update_prompt', { id: 'p7', filePath: '' });
      window.__FIRE__('store-synced', null);
      await sleep(400);
    }
    // 435: archiving a prompt hides it from the browsing library without
    // deleting it. The show-archive toggle brings it back, and an active
    // search query still finds it either way -- "Prompt 12" is unused by any
    // other check, so this block owns it start to finish and restores it.
    {
      const row435 = document.querySelector('#library-list .lib-item[data-id="p11"]');
      if (!row435) {
        out.errors.push('435: "Prompt 12" is missing from the library before the archive check');
      } else {
        const countBefore435 = document.getElementById('library-count').textContent;
        row435.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 40 }));
        await sleep(150);
        const archiveBtn435 = document.querySelector('#ctx [data-act="archive"]');
        if (!archiveBtn435 || archiveBtn435.classList.contains('hidden')) {
          out.errors.push('435: the archive action is missing from the library row context menu');
        } else {
          archiveBtn435.click();
          await sleep(200);
          if (document.querySelector('#library-list .lib-item[data-id="p11"]')) {
            out.errors.push('435: an archived prompt is still shown in the library list');
          }
          const countAfter435 = document.getElementById('library-count').textContent;
          const nBefore435 = parseInt(countBefore435, 10);
          const nAfter435 = parseInt(countAfter435, 10);
          if (!(nAfter435 < nBefore435)) {
            out.errors.push('435: the library header count did not drop after archiving a prompt: ' + countBefore435 + ' -> ' + countAfter435);
          }
          // The toggle above the list brings archived prompts back into view.
          document.getElementById('library-archive').click();
          await sleep(200);
          if (!document.querySelector('#library-list .lib-item[data-id="p11"]')) {
            out.errors.push('435: the show-archived toggle did not bring the archived prompt back');
          }
          document.getElementById('library-archive').click();
          await sleep(200);
          if (document.querySelector('#library-list .lib-item[data-id="p11"]')) {
            out.errors.push('435: turning the show-archived toggle back off did not hide it again');
          }
          // A search query still finds an archived prompt with the toggle off.
          q.value = 'Prompt 12';
          q.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(300);
          const row435b = document.querySelector('#library-list .lib-item[data-id="p11"]');
          if (!row435b) {
            out.errors.push('435: searching did not find an archived prompt');
          } else {
            // Unarchive it again, while search still shows it, so later checks
            // see the fixture's original state.
            row435b.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 40 }));
            await sleep(150);
            const unarchiveBtn435 = document.querySelector('#ctx [data-act="archive"]');
            if (unarchiveBtn435) { unarchiveBtn435.click(); await sleep(200); }
          }
          q.value = '';
          q.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(250);
          if (!document.querySelector('#library-list .lib-item[data-id="p11"]')) {
            out.errors.push('435: unarchiving did not bring "Prompt 12" back into the normal list');
          }
        }
      }
    }
    // 268: the save-search button is a bookmark drawing, not a text glyph.
    {
      const save268 = document.getElementById('library-search-save');
      if (!save268 || !save268.querySelector('svg')) out.errors.push('268: the save-search button has no drawing');
      else if (save268.textContent.trim() !== '') out.errors.push('268: the save-search button still carries text: ' + save268.textContent.trim());
    }
    // tag269: tags in a library row are flat, non-clickable marks beside the name, so a
    // tagged row is no taller than an untagged one with the same text.
    {
      const rows269 = [...document.querySelectorAll('#library-list .lib-item')];
      if (document.querySelector('#library-list .tag-chip')) out.errors.push('tag269: a library row still carries a tag chip');
      const tagged269 = rows269.filter((r) => r.querySelector('.lib-name-row .tag-mark'));
      if (!tagged269.length) out.errors.push('tag269: no row shows a tag mark');
      else {
        const first269 = tagged269[0];
        const marks269 = [...first269.querySelectorAll('.lib-name-row .tag-mark')];
        if (marks269.length > 4) out.errors.push('tag269: a row shows ' + marks269.length + ' marks, at most 4 expected');
        if (marks269.some((x) => getComputedStyle(x.parentElement).pointerEvents !== 'none' || getComputedStyle(x).pointerEvents !== 'none')) {
          out.errors.push('tag269: a tag mark takes clicks');
        }
        const nameTop269 = first269.querySelector('.lib-name').getBoundingClientRect().top;
        const markTop269 = marks269[0].getBoundingClientRect().top;
        if (Math.abs(markTop269 - nameTop269) > 6) out.errors.push('tag269: the first mark is not on the name line (' + markTop269 + ' vs ' + nameTop269 + ')');
        const media269 = (r) => !!r.querySelector('img, video, canvas');
        const textH269 = (r) => Math.round((r.querySelector('.lib-text') || { getBoundingClientRect: () => ({ height: -1 }) }).getBoundingClientRect().height);
        const plain269 = (r) => !r.querySelector('.lib-rotate-tag, .lib-image-hit-tag') && !media269(r);
        const pair269 = tagged269.filter(plain269).map((a) => [a, rows269.find((b) => !b.querySelector('.tag-mark') && plain269(b) && textH269(b) === textH269(a))]).find((x) => x[1]);
        if (!pair269) out.errors.push('tag269: no tagged and untagged text row of equal text height to compare');
        else {
          const hA = pair269[0].getBoundingClientRect().height;
          const hB = pair269[1].getBoundingClientRect().height;
          if (Math.abs(hA - hB) > 1) out.errors.push('tag269: a tagged row is ' + hA + ' px tall, an untagged one ' + hB + ' px');
        }
      }
    }
    // 436: saved searches keep a query, its active tag filters and the current
    // sort together as one chip. Saving, applying, the cap, renaming and
    // deleting are all self-contained here and end with the list empty again.
    {
      const saveBtn436 = document.getElementById('library-search-save');
      const firstTagChip436 = document.querySelector('#library-tagbar .tag-chip.tag-filter');
      if (!saveBtn436 || !firstTagChip436) {
        out.errors.push('436: the search-save button or a tag filter chip is missing');
      } else {
        // Every tag toggle rebuilds #library-tagbar from scratch, so a chip
        // reference from before that rebuild is stale -- always look it back
        // up by its name instead of holding onto the DOM node.
        const tagName436 = firstTagChip436.textContent;
        const findTagChip436 = () => [...document.querySelectorAll('#library-tagbar .tag-chip.tag-filter')].find((c) => c.textContent === tagName436);
        q.value = 'zzsaved436';
        q.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(250);
        findTagChip436().click();
        await sleep(150);
        const sortSel436 = document.getElementById('library-sort');
        const sortBefore436 = sortSel436.value;
        const altSort436 = [...sortSel436.options].map((o) => o.value).find((v) => v !== sortBefore436);
        if (altSort436) {
          sortSel436.value = altSort436;
          sortSel436.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(150);
        }
        saveBtn436.click();
        await sleep(200);
        document.getElementById('confirm-input').value = 'SmokeSaved1';
        document.getElementById('confirm-ok').click();
        await sleep(250);
        const chipsOf436 = () => [...document.querySelectorAll('#library-saved-searches .lib-saved-search-chip')];
        if (!chipsOf436().some((c) => c.textContent === 'SmokeSaved1')) {
          out.errors.push('436: saving a search did not add a chip for it');
        }
        // Forget the search, then bring it back with one click.
        q.value = '';
        q.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(200);
        findTagChip436().click(); // deselect
        await sleep(150);
        if (altSort436) { sortSel436.value = sortBefore436; sortSel436.dispatchEvent(new Event('change', { bubbles: true })); await sleep(150); }
        const savedChip436 = chipsOf436().find((c) => c.textContent === 'SmokeSaved1');
        savedChip436?.click();
        await sleep(250);
        if (q.value !== 'zzsaved436') out.errors.push('436: clicking a saved search did not restore the query text');
        if (!findTagChip436()?.classList.contains('active')) out.errors.push('436: clicking a saved search did not restore its tag filter');
        if (altSort436 && sortSel436.value !== altSort436) out.errors.push('436: clicking a saved search did not restore its sort');
        // Leave the tag filter the way this block found it, so later checks see
        // the full, unfiltered library list again.
        findTagChip436()?.click();
        await sleep(150);
        // Persists across closing and reopening the library.
        await click('library-close');
        await sleep(150);
        await click('library-btn');
        await sleep(250);
        if (!chipsOf436().some((c) => c.textContent === 'SmokeSaved1')) {
          out.errors.push('436: a saved search did not survive closing and reopening the library');
        }
        // The cap: 4 more reach the default limit of 5, a 6th is refused and
        // names the oldest one instead of silently dropping it.
        for (let i = 2; i <= 5; i++) {
          saveBtn436.click();
          await sleep(200);
          document.getElementById('confirm-input').value = 'SmokeSaved' + i;
          document.getElementById('confirm-ok').click();
          await sleep(200);
        }
        if (chipsOf436().length !== 5) {
          out.errors.push('436: expected exactly 5 saved searches at the cap, found ' + chipsOf436().length);
        }
        const toastBefore436 = document.getElementById('toast').textContent;
        saveBtn436.click();
        await sleep(200);
        const dialogOpen436 = !document.getElementById('confirm-modal').classList.contains('hidden');
        if (dialogOpen436) {
          out.errors.push('436: saving past the cap opened a dialog instead of being refused up front');
          document.getElementById('confirm-cancel').click();
          await sleep(150);
        } else {
          const toastAfter436 = document.getElementById('toast').textContent;
          if (toastAfter436 === toastBefore436 || !toastAfter436.includes('SmokeSaved1')) {
            out.errors.push('436: the at-cap message did not name the oldest saved search');
          }
        }
        if (chipsOf436().length !== 5) out.errors.push('436: a refused save past the cap changed the saved search count');
        // Rename, then delete, through the same right-click dialog.
        const renameTarget436 = chipsOf436().find((c) => c.textContent === 'SmokeSaved2');
        if (!renameTarget436) {
          out.errors.push('436: "SmokeSaved2" is missing before the rename check');
        } else {
          renameTarget436.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 20, clientY: 20 }));
          await sleep(150);
          document.getElementById('confirm-input').value = 'SmokeRenamed436';
          document.getElementById('confirm-ok').click();
          await sleep(250);
          if (!chipsOf436().some((c) => c.textContent === 'SmokeRenamed436')) {
            out.errors.push('436: renaming a saved search did not change its chip label');
          }
          if (chipsOf436().some((c) => c.textContent === 'SmokeSaved2')) {
            out.errors.push('436: the old name is still shown after renaming a saved search');
          }
        }
        // Clean up every chip this block created, through its own delete path,
        // so later checks see an empty saved-search row again.
        let guard436 = 0;
        while (chipsOf436().length && guard436 < 10) {
          const c = chipsOf436()[0];
          c.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 20, clientY: 20 }));
          await sleep(150);
          document.getElementById('confirm-alt').click();
          await sleep(200);
          guard436++;
        }
        if (chipsOf436().length) out.errors.push('436: deleting saved searches did not empty the list again');
      }
    }
    // 437: the list/tiles toggle draws the very same rows differently -- a
    // selection survives the switch either way, and neither layout scrolls
    // the window horizontally.
    {
      const viewToggle437 = document.getElementById('library-view-toggle');
      const list437 = document.getElementById('library-list');
      if (!viewToggle437 || !list437) {
        out.errors.push('437: the library list/tiles toggle is missing');
      } else {
        await click('library-select-btn');
        document.getElementById('lib-batch-all').click();
        await sleep(150);
        const selectedBefore437 = document.querySelectorAll('#library-list .lib-item.lib-selected').length;
        if (!selectedBefore437) out.errors.push('437: "select all" selected nothing before the tiles check');
        viewToggle437.click();
        await sleep(300);
        if (!list437.classList.contains('lib-tiles-view')) {
          out.errors.push('437: clicking the view toggle did not switch to tiles');
        }
        const selectedInTiles437 = document.querySelectorAll('#library-list .lib-item.lib-selected').length;
        if (selectedInTiles437 !== selectedBefore437) {
          out.errors.push('437: switching to tiles changed the selection (' + selectedBefore437 + ' -> ' + selectedInTiles437 + ')');
        }
        layoutScan('library tiles');
        viewToggle437.click();
        await sleep(300);
        if (list437.classList.contains('lib-tiles-view')) {
          out.errors.push('437: clicking the view toggle again did not switch back to the list');
        }
        const selectedBack437 = document.querySelectorAll('#library-list .lib-item.lib-selected').length;
        if (selectedBack437 !== selectedBefore437) {
          out.errors.push('437: switching back to the list changed the selection (' + selectedBefore437 + ' -> ' + selectedBack437 + ')');
        }
        document.getElementById('lib-batch-none').click();
        await sleep(150);
        await click('library-select-btn'); // exit select mode
      }
    }
    // P25: the 2 most-used tags across the fixture (Work, Draft) are offered as
    // one-click chips on an untagged prompt, and vanish once actually added.
    {
      const row = document.querySelector('#library-list .lib-item[data-id="p2"]');
      const editBtn = row && row.querySelector('.lib-edit');
      if (!editBtn) {
        out.errors.push('library row p2 has no edit icon for the quickadd check');
      } else {
        editBtn.click();
        await sleep(250);
        // 28: tags now live inside the "Advanced options" collapsible.
        document.getElementById('modal-advanced-head').click();
        await sleep(200);
        const qa = document.getElementById('modal-tag-quickadd');
        const chips = qa ? [...qa.querySelectorAll('.tag-chip')] : [];
        const names = chips.map(c => (c.querySelector('.tag-chip-label') || {}).textContent);
        if (!names.includes('Work') || !names.includes('Draft')) {
          out.errors.push('the tag quick-add row is missing an expected tag: ' + names.join(','));
        }
        const workChip = chips.find(c => (c.querySelector('.tag-chip-label') || {}).textContent === 'Work');
        if (workChip) {
          workChip.click();
          await sleep(150);
          const assigned = [...document.querySelectorAll('#modal-tag-chips .tag-chip-label')].map(s => s.textContent);
          if (!assigned.includes('Work')) out.errors.push('clicking a quick-add tag did not add it to the prompt');
          const stillOffered = [...document.querySelectorAll('#modal-tag-quickadd .tag-chip-label')].map(s => s.textContent);
          if (stillOffered.includes('Work')) out.errors.push('an already-added tag is still offered in quick-add');
        }
        // The tag makes the editor dirty, so closing asks first: discard, or the
        // pending question and the open editor would leak into the next checks.
        document.getElementById('modal-close-x').click();
        await sleep(200);
        const discardAsk = document.getElementById('confirm-modal');
        if (discardAsk && !discardAsk.classList.contains('hidden')) {
          document.getElementById('confirm-ok').click();
          await sleep(200);
        }
      }
    }
    // 339: library batch bar's export control -- same select+button pattern
    // as the settings page, now also offering JSON, gated on a real selection.
    {
      await click('library-select-btn');
      document.getElementById('lib-batch-all').click();
      await sleep(150);
      const fmtSel = document.getElementById('lib-batch-format');
      const expBtn = document.getElementById('lib-batch-export');
      if (!fmtSel || !expBtn) {
        out.errors.push('the library batch bar export control (format select + button) is missing');
      } else {
        const optValues = [...fmtSel.options].map(o => o.value);
        if (!['csv', 'txt', 'json'].every(v => optValues.includes(v))) {
          out.errors.push('the library batch export select is missing csv/txt/json, has: ' + optValues.join(','));
        }
        window.__EXPORT_CALL__ = null;
        fmtSel.value = 'txt';
        fmtSel.dispatchEvent(new Event('change', { bubbles: true }));
        expBtn.click();
        await sleep(200);
        if (!window.__EXPORT_CALL__ || window.__EXPORT_CALL__.format !== 'txt') {
          out.errors.push('the library batch export did not call export_prompts with format "txt"');
        }
        if (!Array.isArray(window.__EXPORT_CALL__.ids) || !window.__EXPORT_CALL__.ids.length) {
          out.errors.push('the library batch export did not pass the selected ids');
        }
      }
      // 422: batch tag add/remove, still on the same "select all" from above.
      {
        const addBtn = document.getElementById('lib-batch-tag-add');
        const removeBtn = document.getElementById('lib-batch-tag-remove');
        if (!addBtn || addBtn.classList.contains('hidden') || !removeBtn || removeBtn.classList.contains('hidden')) {
          out.errors.push('422: the batch tag add/remove buttons are missing or hidden with tags enabled');
        } else {
          const selectedIds = [...document.querySelectorAll('.lib-item.lib-selected')].map((el) => el.dataset.id);
          addBtn.click();
          await sleep(150);
          document.getElementById('confirm-input').value = 'Smoke422';
          document.getElementById('confirm-ok').click();
          await sleep(150);
          const rows = [...document.querySelectorAll('.lib-item')].filter((el) => selectedIds.includes(el.dataset.id));
          if (!rows.length || !rows.every((el) => el.textContent.includes('Smoke422'))) {
            out.errors.push('422: adding a tag to the selection did not show it on every selected row');
          }
          // 434: undo the batch tag-add - the toast's Undo button inverts it
          // through the same batch_prompts command, tag-remove this time.
          const addUndoToast = document.getElementById('toast');
          const addUndoBtn = addUndoToast.querySelector('.toast-btn');
          if (!addUndoToast.classList.contains('actionable') || !addUndoBtn) {
            out.errors.push('434: a batch tag-add did not offer an undo toast');
          } else {
            addUndoBtn.click();
            await sleep(200);
            const rowsAfterUndo = [...document.querySelectorAll('.lib-item')].filter((el) => selectedIds.includes(el.dataset.id));
            if (rowsAfterUndo.some((el) => el.textContent.includes('Smoke422'))) {
              out.errors.push('434: undoing a batch tag-add left the tag showing on a row');
            }
          }
          // Add it back so the tag-remove half below has something to remove.
          addBtn.click();
          await sleep(150);
          document.getElementById('confirm-input').value = 'Smoke422';
          document.getElementById('confirm-ok').click();
          await sleep(150);
          removeBtn.click();
          await sleep(150);
          document.getElementById('confirm-input').value = 'Smoke422';
          document.getElementById('confirm-ok').click();
          await sleep(150);
          const rows2 = [...document.querySelectorAll('.lib-item')].filter((el) => selectedIds.includes(el.dataset.id));
          if (rows2.some((el) => el.textContent.includes('Smoke422'))) {
            out.errors.push('422: removing the tag left it showing on a row');
          }
          // 434: undo the batch tag-remove - inverted back to tag-add, then
          // removed again for real (no undo) so the fixture ends up clean.
          const rmUndoToast = document.getElementById('toast');
          const rmUndoBtn = rmUndoToast.querySelector('.toast-btn');
          if (!rmUndoToast.classList.contains('actionable') || !rmUndoBtn) {
            out.errors.push('434: a batch tag-remove did not offer an undo toast');
          } else {
            rmUndoBtn.click();
            await sleep(200);
            const rowsRestored = [...document.querySelectorAll('.lib-item')].filter((el) => selectedIds.includes(el.dataset.id));
            if (!rowsRestored.length || !rowsRestored.every((el) => el.textContent.includes('Smoke422'))) {
              out.errors.push('434: undoing a batch tag-remove did not bring the tag back');
            }
            removeBtn.click();
            await sleep(150);
            document.getElementById('confirm-input').value = 'Smoke422';
            document.getElementById('confirm-ok').click();
            await sleep(150);
            const rowsFinal = [...document.querySelectorAll('.lib-item')].filter((el) => selectedIds.includes(el.dataset.id));
            if (rowsFinal.some((el) => el.textContent.includes('Smoke422'))) {
              out.errors.push('434: the fixture still carries "Smoke422" after the batch tag undo checks');
            }
          }
        }
      }
      // 434: batch delete offers an Undo button too - restore_prompt per id
      // through the same trash-restore path a single trash row uses. Narrowed
      // to one fixture prompt (p8, otherwise unreferenced past this point) so
      // this does not disturb the rest of the fixture library.
      {
        document.getElementById('lib-batch-none').click();
        await sleep(100);
        const p8Row = document.querySelector('.lib-item[data-id="p8"]');
        if (!p8Row) {
          out.errors.push('434: fixture prompt p8 is missing for the batch-delete-undo check');
        } else {
          p8Row.click();
          await sleep(100);
          const delBatchBtn = document.getElementById('lib-batch-delete');
          delBatchBtn.click(); await sleep(250);
          if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
            out.errors.push('677: the library batch delete opened a dialog');
          }
          if (document.querySelector('.lib-item[data-id="p8"]')) {
            out.errors.push('434: batch delete did not remove p8 from the library list');
          }
          const delUndoToast = document.getElementById('toast');
          const delUndoBtn = delUndoToast.querySelector('.toast-btn');
          if (!delUndoToast.classList.contains('actionable') || !delUndoBtn) {
            out.errors.push('434: a batch delete did not offer an undo toast');
          } else {
            window.__SMOKE_ARGS__ = [];
            delUndoBtn.click();
            await sleep(300);
            if (!window.__SMOKE_ARGS__.some(([cmd, args]) => cmd === 'restore_prompts_batch' && args && Array.isArray(args.placements) && args.placements.some((pl) => pl.id === 'p8'))) {
              out.errors.push('434: undoing a batch delete did not restore p8 with its placement snapshot');
            }
            if (!document.querySelector('.lib-item[data-id="p8"]')) {
              out.errors.push('434: undoing a batch delete did not bring p8 back to the library list');
            }
          }
        }
      }
      // 677: the library row's trash button acts on the first click, shows Undo
      // and no dialog; Undo sends the placement snapshot back.
      {
        const rowDel = document.querySelector('.lib-item[data-id="p8"] .lib-delete');
        if (!rowDel) {
          out.errors.push('677: the library row of p8 has no trash button');
        } else {
          window.__SMOKE_ARGS__ = [];
          rowDel.click();
          await sleep(300);
          if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
            out.errors.push('677: the library row trash button opened a dialog');
          }
          if (!window.__SMOKE_ARGS__.some(([cmd, args]) => cmd === 'delete_prompt' && args && args.id === 'p8')) {
            out.errors.push('677: the library row trash button did not delete on the first click');
          }
          const rowUndo = document.getElementById('toast').querySelector('.toast-btn');
          if (!rowUndo) {
            out.errors.push('677: the library row delete offered no undo toast');
          } else {
            window.__SMOKE_ARGS__ = [];
            rowUndo.click();
            await sleep(300);
            if (!window.__SMOKE_ARGS__.some(([cmd, args]) => cmd === 'restore_prompts_batch' && args && args.placements && args.placements.some((pl) => pl.id === 'p8'))) {
              out.errors.push('677: undoing the library row delete did not restore p8 with its placement');
            }
          }
        }
      }
      document.getElementById('lib-batch-none').click();
      await click('library-select-btn'); // back out of select mode
    }
    await click('library-close');
    await click('library-btn');
    await click('library-tab-history');
    layoutScan('copy history');
    mustReach('copy history', '#library .modal-head-actions > button');
"""
