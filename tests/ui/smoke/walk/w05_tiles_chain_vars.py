SECTION = """\
    // Chain start lives in the tile menu: open it on the tile and hand back the entry when it is
    // offered (null otherwise, menu closed again), so a probe presses it like the user does.
    const menuChainStart = (tile) => {
      if (!tile) return null;
      tile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 60, clientY: 60 }));
      const b = document.querySelector('#ctx [data-act="chain"]');
      if (b && !b.classList.contains('hidden')) return b;
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      return null;
    };
    // 199: a text tile starts a chain from its menu; a file tile never offers it, and the tile
    // itself carries no chain-start button any more.
    {
      const textTile = document.querySelector('#grid .tile[data-id="p0"]');
      const imgTile = document.querySelector('#grid .tile[data-id="p9"]'); // fixture: file_path set, not a text prompt
      if (document.querySelector('.tile-chain-start')) {
        out.errors.push('199: a tile still carries a chain-start button');
      }
      if (!textTile) {
        out.errors.push('199: fixture tile p0 missing from the grid');
      } else if (imgTile && menuChainStart(imgTile)) {
        out.errors.push('199: the menu of an image tile offers Start chain');
      } else {
        const csBtn = menuChainStart(textTile);
        if (!csBtn) {
          out.errors.push('199: the menu of a text tile offers no Start chain');
        } else {
          csBtn.click();
          await sleep(150);
          if (!document.body.classList.contains('chain-mode')) {
            out.errors.push('199: Start chain in the tile menu did not enter chain mode');
          }
          if (!textTile.classList.contains('chain-picked')) {
            out.errors.push('199: the starting tile was not marked picked');
          }
          const badge = textTile.querySelector('.tile-chain-badge');
          if (!badge || badge.textContent.trim() !== '1') {
            out.errors.push('199: the starting tile has no "1" chain badge');
          }
          if (menuChainStart(textTile)) {
            out.errors.push('199: the tile menu offered Start chain while a chain is running');
          }
        }
        // A running chain ends on one click; the loop only guards a stray state.
        for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
        await sleep(150);
      }
    }
    // 275: every badge a tile can show AT ONCE must not visually overlap --
    // select checkbox, chain-badge once picked, rotate badge
    // and the actions menu. Not favorite/hotkey -- neither renders as a tile
    // badge in this app (only in the library row and the editor), so there
    // is nothing there that could ever overlap. Select/the menu
    // button only reveal on :hover, which page script cannot trigger in a
    // real browser (CSS :hover tracks actual pointer state, not synthetic
    // events) -- forcing their opacity measures the exact geometry a real
    // hover would produce, without pretending hover itself was tested.
    {
      // Belt and braces: a probe upstream is only ever supposed to leave chain
      // mode off, but a state-leak there must not silently invalidate this one.
      for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
      const rotTile = document.querySelector('#grid .tile[data-id="p10"]'); // fixture: has variants + no file/copy_image
      const checkOverlap = (label, selectors) => {
        const els = selectors.map((sel) => rotTile.querySelector(sel)).filter(Boolean);
        const restore = els.map((el) => [el, el.style.opacity]);
        els.forEach((el) => { el.style.opacity = "1"; });
        for (let i = 0; i < els.length; i++) {
          for (let j = i + 1; j < els.length; j++) {
            const a = els[i].getBoundingClientRect();
            const b = els[j].getBoundingClientRect();
            const ix = Math.max(0, Math.min(a.right, b.right) - Math.max(a.left, b.left));
            const iy = Math.max(0, Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top));
            if (ix * iy > 4) {
              out.errors.push('275: ' + label + ' badges overlap at ' + window.innerWidth + 'px: ' +
                els[i].className + ' / ' + els[j].className);
            }
          }
        }
        restore.forEach(([el, val]) => { el.style.opacity = val; });
      };
      if (!rotTile) {
        out.errors.push('275: fixture tile p10 (rotating, chain-capable) missing from the grid');
      } else {
        checkOverlap('idle', ['.tile-select', '.tile-rotate-badge']);
        const startBtn = menuChainStart(rotTile);
        if (startBtn) {
          startBtn.click();
          await sleep(150);
          checkOverlap('picked mid-chain', ['.tile-chain-badge', '.tile-rotate-badge']);
          // 491: the corner belongs to the order badge while a chain runs -- the
          // select box stays hidden on hover too, on both the
          // picked tile and any other one, unless multi-select is on.
          const otherTile = [...document.querySelectorAll('#grid .tile')].find((el) => el !== rotTile);
          for (const [label, tile] of [['picked', rotTile], ['other', otherTile]]) {
            if (!tile) continue;
            for (const name of ['tile-select']) {
              const el = tile.querySelector('.' + name);
              if (!el) continue;
              const cs = getComputedStyle(el);
              if (cs.pointerEvents !== 'none' || cs.opacity !== '0') {
                out.errors.push('491: the ' + name + ' stayed reachable on the ' + label + ' tile during a chain');
              }
            }
          }
          for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
          await sleep(100);
        }
      }
    }
    // 141: get_state carries has_image + image_rev, never the bytes. A tile fetches
    // the picture it needs through prompt_image and then keeps it, so the many
    // re-renders the walk above already caused must have cost exactly ONE call.
    {
      const state = await window.__TAURI__.core.invoke('get_state');
      const leaked = (state.prompts || []).filter(p => 'image' in p).map(p => p.id);
      if (leaked.length) {
        out.errors.push('get_state ships image bytes again for ' + leaked.join(', '));
      }
      const withImg = (state.prompts || []).find(p => p.has_image);
      if (!withImg) {
        out.errors.push('no fixture prompt has has_image, so the image path went untested');
      } else {
        const shot = () => {
          const el = [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === withImg.id);
          return el && el.querySelector('img.tile-img');
        };
        for (let i = 0; i < 40 && !shot(); i++) await sleep(100);
        const img = shot();
        if (!img) {
          out.errors.push('the image tile never got its picture from prompt_image');
        } else if (img.getAttribute('src') !== window.__STUB_IMAGE__) {
          out.errors.push('the image tile is showing something other than the fetched image');
        }
        const calls = window.__PROMPT_IMAGE_CALLS__;
        if (calls !== 1) {
          out.errors.push('prompt_image ran ' + calls + ' times for one image, expected 1');
        }
        // Draw the same tiles once more: the second render has to come out of the
        // cache, not off another round trip.
        await click('library-btn');
        await click('library-close');
        if (window.__PROMPT_IMAGE_CALLS__ !== calls) {
          out.errors.push('re-rendering an image prompt fetched it again');
        }
        if (!shot()) out.errors.push('the image tile lost its picture on a re-render');
      }
    }
    // F17: the note rides in the tile tooltip as its own dimmed section, and only
    // for a prompt that actually has one.
    const byId = (id) => [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
    const noteTile = byId('p0'), plainTile = byId('p2');
    if (!noteTile || !plainTile) {
      out.errors.push('the tiles for the note check are missing from the grid');
    } else {
      if (!noteTile.dataset.tipNotes) out.errors.push('a prompt with a note carries no tooltip note');
      if (plainTile.dataset.tipNotes) out.errors.push('a prompt without a note carries a tooltip note anyway');
      const noteSection = () => document.querySelector('.tile-tip .tile-tip-notes');
      const hover = async (el, x) => {
        el.dispatchEvent(new PointerEvent('pointerover', { bubbles: true, clientX: x, clientY: 40 }));
        await sleep(700); // longer than the tooltip show-delay
      };
      await hover(noteTile, 30);
      const shown = noteSection();
      if (!shown || !shown.textContent.includes('Moonshot')) {
        out.errors.push('the tooltip shows no note: ' + (shown ? shown.textContent : 'section missing'));
      } else if (getComputedStyle(shown).display === 'none') {
        out.errors.push('the note section stays hidden although a note is set');
      }
      await hover(plainTile, 60);
      const empty = noteSection();
      if (empty && empty.textContent.trim()) {
        out.errors.push('the tooltip shows a note for a prompt that has none');
      }
      // 6/14: the length readout and the {?variable} highlight, on a prompt that
      // actually has placeholders (p1: "Translate {?Text=Hello} into {?Language}.").
      const varHoverTile = byId('p1');
      if (varHoverTile) {
        await hover(varHoverTile, 90);
        const lenText = (document.querySelector('.tile-tip .tile-tip-len') || {}).textContent || '';
        if (!/^[0-9][0-9,.]* chars/.test(lenText) || !/~[0-9][0-9,.]* tokens$/.test(lenText)) {
          out.errors.push('the tile tooltip length readout looks wrong: "' + lenText + '"');
        }
        const varSpans = [...document.querySelectorAll('.tile-tip .tile-tip-body .var-token')];
        const varTexts = varSpans.map((s) => s.textContent);
        if (varTexts.length !== 2 || !varTexts.includes('[Text]') || !varTexts.includes('[Language]')) {
          out.errors.push('the tile tooltip does not highlight both variables: ' + JSON.stringify(varTexts));
        }
      }
      // B1: the rotate line is shared between the rich tile tip and the plain
      // setting/icon tip. It used to survive the switch, so the last hovered
      // tile-s next-entry text stayed glued under every plain tooltip.
      const rotLine = () => document.querySelector('.tile-tip .tile-tip-rotate');
      noteTile.dataset.tipRotate = 'Rotation, next text: probe';
      await hover(noteTile, 30);
      const withRot = rotLine();
      if (!withRot || !withRot.textContent.includes('probe')) {
        out.errors.push('the tile tooltip does not show the rotate line at all');
      }
      const plainTarget = document.createElement('div');
      plainTarget.dataset.tip = 'A plain setting tooltip';
      plainTarget.style.cssText = 'position:fixed;left:180px;top:30px;width:40px;height:20px';
      document.body.appendChild(plainTarget);
      await hover(plainTarget, 200);
      const plainName = document.querySelector('.tile-tip .tile-tip-name');
      if (!plainName || !plainName.textContent.includes('plain setting')) {
        out.errors.push('the plain tooltip never showed, so the rotate-line check proves nothing');
      } else {
        const leaked = rotLine();
        if (leaked && leaked.textContent.trim()) {
          out.errors.push('a plain tooltip carries the rotate line: "' + leaked.textContent + '"');
        }
      }
      plainTarget.remove();
      delete noteTile.dataset.tipRotate;
      document.body.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
      await sleep(200);
    }
    // 428: last-copied/last-edited lines in the tile tooltip. p1 carries canned
    // timestamps for both (tauri_stub's SETTINGS.last_used and last_edited_map);
    // p2 has neither. main.js is an ES module, so its `settings`/`renderGrid`
    // are not reachable from this classic probe script -- the data has to be
    // baked into the stub's initial state instead of poked in live.
    {
      const copyId = 'p1';
      const t1 = byId(copyId);
      const times1 = (t1 || {}).dataset ? t1.dataset.tipTimes || '' : '';
      if (!times1.includes('Last copied') || !times1.includes('Last edited')) {
        out.errors.push('428: a prompt with both timestamps is missing one of the tooltip lines: "' + times1 + '"');
      }
      const expected = new Date(1700000000 * 1000).toLocaleString('en', { dateStyle: 'short', timeStyle: 'short' });
      if (!times1.includes(expected)) {
        out.errors.push('428: the copy time is not a localized date string: "' + times1 + '"');
      }
      const t2 = byId('p2');
      const times2 = (t2 || {}).dataset ? t2.dataset.tipTimes || '' : '';
      if (times2) out.errors.push('428: a prompt with no history data carries tooltip time lines anyway: "' + times2 + '"');
    }
    // 1: the empty-grid hint stays out of the way once prompts exist.
    if (!document.getElementById('grid-empty').classList.contains('hidden')) {
      out.errors.push('the empty-grid hint shows although the grid has prompts');
    }
    // Copying the prompt with placeholders must open the placeholder window and,
    // once it answers, finish the copy -- and a plain click must NOT paste into the
    // previous app (that is what the double-click / Enter shortcut is for).
    const tiles = [...document.querySelectorAll('#grid .tile')];
    const varTile = tiles.find(el => (el.dataset.id || '') === 'p1');
    if (varTile) {
      window.__VAR_PAYLOAD__ = undefined;
      // The grid listens for pointer events (it tracks drags), so .click() alone
      // never reaches the handler -- dispatch the real sequence.
      for (const type of ['pointerdown', 'pointerup', 'click']) {
        varTile.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1 }));
      }
      await sleep(400);
      const varsReq1 = varReq();
      if (!varsReq1) out.errors.push('clicking a variable prompt did not open the placeholder window');
      // 425: a text default in the prompt itself prefills the field on first
      // ask (Text=Hello); a variable with no default stays empty (Language).
      if (varsReq1) {
        if (varsReq1.rows[0]?.value !== 'Hello') out.errors.push('425: the text default did not prefill the field: ' + varsReq1.rows[0]?.value);
        if (varsReq1.rows[1]?.value) out.errors.push('425: a variable with no default was prefilled anyway: ' + varsReq1.rows[1].value);
        if (document.getElementById('vars-modal') || document.getElementById('vars-fields')) out.errors.push('the main window still carries an in-window placeholder dialog');
        await varAnswer(['Hello', 'Dutch']);
      }
      if ((window.__SMOKE_CALLS__ || []).includes('paste_into_previous')) {
        out.errors.push('a single click on a variable prompt pasted into the previous app');
      }
      // A second press while the window is still up is the second half of a double
      // click: it opens no second question and turns the pending copy into a paste.
      await sleep(2400); // past the per-prompt copy cooldown (2000ms)
      window.__VAR_PAYLOAD__ = undefined;
      (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []).length = 0;
      for (let press = 0; press < 2; press++) {
        for (const type of ['pointerdown', 'pointerup', 'click']) {
          varTile.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1 }));
        }
        await sleep(200);
      }
      const dblOpens = window.__SMOKE_CALLS__.filter(c => c === 'open_var_window').length;
      if (dblOpens !== 1) out.errors.push('a double click on a variable prompt opened the placeholder window ' + dblOpens + ' times');
      await varAnswer(['Hello', 'Dutch']);
      if (!window.__SMOKE_CALLS__.includes('paste_into_previous')) {
        out.errors.push('a double click on a variable prompt did not paste after the window was filled');
      }
    }
    // The chain button latches (off -> on -> infinite), so the two clicks above left
    // it armed; a picked tile would go into the chain instead of being copied.
    for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
    if (document.body.classList.contains('chain-mode')) out.errors.push('chain mode could not be switched off');
    // F12/F13: clicking a tile, but through the same real pointer sequence.
    const clickTile = async (id) => {
      const el = [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
      if (!el) return false;
      for (const type of ['pointerdown', 'pointerup', 'click']) {
        el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1 }));
      }
      await sleep(400);
      return true;
    };
    const varsHidden = () => !varReq();
    // F12: a prompt whose placeholders all resolve themselves must never ask, and
    // the copied text must carry no leftover {?} fence.
    window.__LAST_COPY__ = undefined;
    window.__VAR_PAYLOAD__ = undefined;
    if (!await clickTile('p3')) {
      out.errors.push('the dynamic-placeholder prompt is missing from the grid');
    } else {
      if (!varsHidden()) { out.errors.push('an automatic placeholder still opened the placeholder window'); await varCancel(); }
      const copied = window.__LAST_COPY__ || '';
      if (/\\{#\\{/.test(copied)) out.errors.push('an automatic placeholder was left unresolved: ' + copied);
      if (!copied.includes('CLIPTEXT')) out.errors.push('the clipboard placeholder did not pick up the clipboard text');
      if (!/Meeting on \\S/.test(copied)) out.errors.push('the date placeholder produced nothing');
      // F12: every calendar/id placeholder has to produce a real value of its own.
      const dynChecks = [
        ['week', /week \\d{1,2} of \\S/],
        ['year', new RegExp(' ' + new Date().getFullYear() + '\\\\)')],
        ['yesterday/tomorrow', /yesterday \\S+, tomorrow \\S+/],
        ['timestamp', /stamp \\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}[+-]\\d{2}:\\d{2}/],
        ['epoch', /epoch 1\\d{9}/],
        ['username', /by tester/],
        ['random', /ref [0-9a-z]{8}/],
        ['counter', /no 1[ :]/],
      ];
      for (const [name, re] of dynChecks) {
        if (!re.test(copied)) out.errors.push('the ' + name + ' placeholder produced nothing usable: ' + copied);
      }
      // A second copy must move the counter on - that is the whole point of it.
      window.__LAST_COPY__ = undefined;
      await sleep(2400); // past the per-prompt copy cooldown (2000ms)
      if (await clickTile('p3') && !/no 2[ :]/.test(window.__LAST_COPY__ || '')) {
        out.errors.push('the counter placeholder did not count up: ' + window.__LAST_COPY__);
      }
    }
    // 445: the chain overview strip -- appears once a chain has a pick, marks a
    // step red when it needs a placeholder nothing earlier in the chain fills,
    // recomputes after a drag reorder, and disappears with its own switch off.
    {
      const findGridTile = (id) => [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
      const findExpertRow = async (label) => {
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
      for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
      // 510: the grid's own top position with the chain off, to compare against
      // once the chain overview strip has come and gone below.
      const gridTopOff = document.getElementById('grid').getBoundingClientRect().top;
      const p0Tile = findGridTile('p0');
      const startBtn = menuChainStart(p0Tile);
      if (!startBtn) {
        out.errors.push('445: the menu of fixture tile p0 offers no Start chain to begin the chain with');
      } else {
        startBtn.click();
        await sleep(150);
        // p3: dynamic placeholders only -- self-filling, no dialog, no ask badges.
        if (!await clickTile('p3')) out.errors.push('445: could not pick p3 into the chain');
        // p1: two manual placeholders -- opens the placeholder window, nothing fills them yet.
        window.__VAR_PAYLOAD__ = undefined;
        if (!await clickTile('p1')) {
          out.errors.push('445: could not pick p1 into the chain');
        } else {
          const chainReq = varReq();
          await varAnswer((chainReq ? chainReq.rows : []).map(() => 'x'));
        }
        const bar = document.getElementById('chain-overview');
        if (!bar || bar.classList.contains('hidden')) {
          out.errors.push('445: the chain overview strip did not appear for a running chain');
        } else {
          const ovTitle = bar.querySelector('.chain-ov-title');
          if (ovTitle && ovTitle.getBoundingClientRect().top - bar.getBoundingClientRect().top > 3) {
            out.errors.push('445: the chain overview keeps extra space above its title');
          }
          // 510: total gap from the grid area's own top edge (its padding box,
          // not the strip's) down to the title, so the padding the strip adds
          // on top of the grid area's own padding is counted too.
          if (ovTitle) {
            const gridWrapTop = document.querySelector('.grid-wrap').getBoundingClientRect().top;
            const gap510 = ovTitle.getBoundingClientRect().top - gridWrapTop;
            if (gap510 > 4) {
              out.errors.push('510: the gap above the chain overview title is ' + Math.round(gap510) + 'px, over the 4px target');
            }
          }
          const firstTile = document.querySelector('#grid .tile');
          if (firstTile && bar.getBoundingClientRect().bottom > firstTile.getBoundingClientRect().top) {
            out.errors.push('445: the chain overview overlaps the grid below it');
          }
          let rows = [...bar.querySelectorAll('.chain-ov-row')];
          if (rows.length !== 3) {
            out.errors.push('445: the overview shows ' + rows.length + ' rows instead of 3');
          } else {
            const [row0, row1, row2] = rows;
            if (row0.classList.contains('chain-ov-missing') || row1.classList.contains('chain-ov-missing')) {
              out.errors.push('445: a step with no unmet placeholder was marked red');
            }
            if (!row2.classList.contains('chain-ov-missing')) {
              out.errors.push('445: the step needing Text/Language was not marked red');
            }
            if (!row2.querySelector('.chain-ov-missing-note')) {
              out.errors.push('445: the red row names no missing placeholder');
            }
            if (!row1.querySelector('.chain-ov-badge.chain-ov-fills')) {
              out.errors.push('445: the reserved-name step shows no fills badge');
            }
            if (!row2.querySelector('.chain-ov-badge.chain-ov-needs')) {
              out.errors.push('445: the manual-placeholder step shows no needs badge');
            }
            // Drag the red (3rd) row onto the 1st row and drop -- the strip must
            // recompute in place, not leave the red mark stuck at the old position.
            const pt445 = (type, el, x, y) => el.dispatchEvent(new PointerEvent(type, {
              bubbles: true, cancelable: true, pointerId: 5, isPrimary: true, button: 0,
              buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y,
            }));
            const rr2 = row2.getBoundingClientRect();
            const rr0 = row0.getBoundingClientRect();
            const sx = rr2.left + rr2.width / 2, sy = rr2.top + rr2.height / 2;
            const ex = rr0.left + rr0.width * 0.25, ey = rr0.top + rr0.height / 2;
            pt445('pointerdown', row2, sx, sy);
            pt445('pointermove', row2, sx + 14, sy);
            pt445('pointermove', row2, ex, ey);
            pt445('pointerup', row2, ex, ey);
            await sleep(150);
            rows = [...bar.querySelectorAll('.chain-ov-row')];
            if (!rows[0] || !rows[0].classList.contains('chain-ov-missing')) {
              out.errors.push('445: dragging the red row to the front did not recompute the strip');
            }
          }
        }
      }
      for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
      await sleep(150);
      // 510: with the chain off again, the grid must sit exactly where it did
      // before the strip ever appeared.
      const gridTopBack = document.getElementById('grid').getBoundingClientRect().top;
      if (Math.abs(gridTopBack - gridTopOff) > 1) {
        out.errors.push('510: the grid top moved from ' + Math.round(gridTopOff) + 'px to ' +
          Math.round(gridTopBack) + 'px after the chain overview strip was shown and hidden again');
      }
    }
    // 555: the chain overview rows reorder by pointer drag and drop a step on
    // right-click; the clipboard text follows both.
    {
      const findGridTile = (id) => [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
      const startBtn = menuChainStart(findGridTile('p0'));
      const ovRows = () => [...document.querySelectorAll('#chain-overview .chain-ov-row')];
      const ovNames = () => ovRows().map((r) => r.querySelector('.chain-ov-name').textContent);
      const clipParts = () => String(window.__LAST_COPY__ || '').split('\\n\\n');
      const ptr = (type, el, x, y) => el.dispatchEvent(new PointerEvent(type, {
        bubbles: true, cancelable: true, pointerId: 7, isPrimary: true, button: 0,
        buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y,
      }));
      if (!startBtn) {
        out.errors.push('555: the menu of fixture tile p0 offers no Start chain');
      } else {
        startBtn.click();
        await sleep(150);
        if (!await clickTile('p3')) out.errors.push('555: could not pick p3 into the chain');
        if (!await clickTile('p2')) out.errors.push('555: could not pick p2 into the chain');
        await sleep(150);
        const rows0 = ovRows();
        if (rows0.length !== 3) {
          out.errors.push('555: expected three chain steps, got ' + rows0.length);
        } else {
          const names0 = ovNames();
          const parts0 = clipParts();
          if (parts0.length !== 3) out.errors.push('555: the chain clipboard text does not have three parts: ' + parts0.length);
          if (!rows0[0].title) out.errors.push('555: a chain step has no tooltip');
          const r3 = rows0[2].getBoundingClientRect();
          const r1 = rows0[0].getBoundingClientRect();
          const x3 = r3.left + r3.width / 2, y3 = r3.top + r3.height / 2;
          ptr('pointerdown', rows0[2], x3, y3);
          ptr('pointermove', rows0[2], x3 + 3, y3);
          if (rows0[2].classList.contains('dragging')) out.errors.push('555: a move under the threshold already started the drag');
          const lx = r1.left + r1.width * 0.25, ly = r1.top + r1.height / 2;
          ptr('pointermove', rows0[2], x3 + 14, y3);
          ptr('pointermove', rows0[2], lx, ly);
          if (!rows0[2].classList.contains('dragging')) out.errors.push('555: the dragged step has no dragging class');
          if (!rows0[0].classList.contains('drag-over')) out.errors.push('555: the step under the pointer shows no drag-over mark');
          ptr('pointerup', rows0[2], lx, ly);
          await sleep(200);
          const names1 = ovNames();
          const want1 = [names0[2], names0[0], names0[1]];
          if (names1.join('|') !== want1.join('|')) out.errors.push('555: dragging step 3 onto the left half of step 1 gave ' + names1.join('|') + ', wanted ' + want1.join('|'));
          const parts1 = clipParts();
          const wantParts1 = [parts0[2], parts0[0], parts0[1]];
          if (parts1.join('|') !== wantParts1.join('|')) out.errors.push('555: the clipboard text did not follow the new order');
          if (ovRows().some((r) => r.classList.contains('dragging') || r.classList.contains('drag-over'))) out.errors.push('555: drag marks stayed after the drop');
          // Right-click on step 2 removes it and the clipboard follows.
          const row2 = ovRows()[1];
          const cm = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2 });
          row2.dispatchEvent(cm);
          await sleep(200);
          if (!cm.defaultPrevented) out.errors.push('555: right-click on a step did not prevent the native menu');
          if (!document.getElementById('ctx').classList.contains('hidden')) out.errors.push('555: right-click on a step opened the tile menu');
          const names2 = ovNames();
          if (names2.join('|') !== [names1[0], names1[2]].join('|')) out.errors.push('555: right-click did not remove exactly step 2: ' + names2.join('|'));
          if (clipParts().join('|') !== [parts1[0], parts1[2]].join('|')) out.errors.push('555: the clipboard did not lose the removed step');
          if (!document.body.classList.contains('chain-mode')) out.errors.push('555: removing a step ended the chain');
        }
      }
      for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
      await sleep(150);
    }
    // 554: switching to another program (blur, then focus on return) never ends
    // or clears a chain, locked or not; that includes the blur/focus pair a
    // window move causes. Only the chain button, Escape or Ctrl+V in this window end it.
    {
      const findGridTile = (id) => [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === id);
      const p0Tile = findGridTile('p0');
      const startBtn = menuChainStart(p0Tile);
      const pickedIds = () => ['p0', 'p3'].filter((id) => findGridTile(id)?.classList.contains('chain-picked'));
      const awayAndBack = async () => {
        window.dispatchEvent(new Event('blur'));
        await sleep(60);
        window.dispatchEvent(new Event('focus'));
        await sleep(60);
      };
      if (!startBtn) {
        out.errors.push('the menu of fixture tile p0 offers no Start chain to begin a chain with');
      } else {
        startBtn.click();
        await sleep(150);
        if (!await clickTile('p3')) out.errors.push('could not pick p3 into the chain');
        if (!document.body.classList.contains('chain-mode') || pickedIds().length !== 2) {
          out.errors.push('554: the chain did not start with two picks, cannot test the app switch');
        } else {
          await awayAndBack();
          if (!document.body.classList.contains('chain-mode')) out.errors.push('554: blur then focus ended an unlocked chain');
          if (pickedIds().length !== 2) out.errors.push('554: blur then focus dropped the picks of an unlocked chain');
          const header = document.querySelector('[data-tauri-drag-region]');
          if (header) {
            header.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
            await awayAndBack();
            if (!document.body.classList.contains('chain-mode') || pickedIds().length !== 2) {
              out.errors.push('554: a window move (drag-region mousedown, blur, focus) hurt the chain');
            }
            window.dispatchEvent(new Event('mouseup'));
          }
          await click('chain-btn'); // a tap ends the running chain
          await sleep(60);
          if (document.body.classList.contains('chain-mode')) out.errors.push('one tap on the chain button did not end the chain');
          await click('chain-btn'); // one tap starts a latched chain
          await sleep(60);
          if (!document.getElementById('chain-btn').classList.contains('chain-locked')) out.errors.push('one tap on the chain button did not start a latched chain');
          if (!await clickTile('p0') || !await clickTile('p3')) out.errors.push('could not pick p0 and p3 into the latched chain');
          await awayAndBack();
          if (!document.body.classList.contains('chain-mode')) out.errors.push('554: blur then focus ended a locked chain');
          if (pickedIds().length !== 2) out.errors.push('554: blur then focus cleared the picks of a locked chain');
        }
      }
      for (let i = 0; i < 3 && document.body.classList.contains('chain-mode'); i++) await click('chain-btn');
      await sleep(150);
    }
    // F13: a mixed prompt asks only for the manual variable and offers the choice
    // as a themed dropdown; the pick lands in the copied text.
    window.__LAST_COPY__ = undefined;
    window.__VAR_PAYLOAD__ = undefined;
    if (!await clickTile('p5')) {
      out.errors.push('the choice-variable prompt is missing from the grid');
    } else if (varsHidden()) {
      out.errors.push('the mixed variable prompt did not open the placeholder window');
    } else {
      const mixedRows = varReq().rows;
      const sel = mixedRows.find(r => r.options.length);
      const texts = mixedRows.filter(r => !r.options.length);
      if (!sel) {
        out.errors.push('the choice variable is not offered as a list of options');
      } else {
        if (sel.options.join(',') !== 'formal,casual,witty') {
          out.errors.push('the choice lists the wrong options');
        }
        // 425: the token's own default (tone=casual) preselects that entry
        // instead of the list's first one.
        if (sel.value !== 'casual') out.errors.push('425: the choice did not preselect the text default: ' + sel.value);
      }
      if (texts.length !== 1) out.errors.push('the mixed window shows ' + texts.length + ' text fields instead of 1');
      await varAnswer(mixedRows.map(r => r.options.length ? 'witty' : 'Rust'));
      const copied = window.__LAST_COPY__ || '';
      if (copied !== 'Write a witty note about Rust.') out.errors.push('the filled text is wrong: ' + copied);
    }
    // The chosen option must come back preselected on the next copy.
    window.__VAR_PAYLOAD__ = undefined;
    await sleep(2400); // past the per-prompt copy cooldown, which now comes before the dialog
    if (await clickTile('p5')) {
      const sel2 = (varReq() || { rows: [] }).rows.find(r => r.options.length);
      if (!sel2 || sel2.value !== 'witty') out.errors.push('the last choice was not remembered');
      await varCancel();
    }
    // Copying a rotating prompt with Auto on has to hand its id to copy_text -
    // that id is the only thing that lets the backend open an auto-advance
    // session, and without it the Auto switch had no effect in this window at all.
    window.__LAST_COPY__ = undefined;
    window.__LAST_COPY_ID__ = undefined;
    if (!await clickTile('p10')) {
      out.errors.push('the rotating prompt is missing from the grid');
    } else if (window.__LAST_COPY_ID__ !== 'p10') {
      out.errors.push('copying the Auto rotating prompt passed no id: ' + window.__LAST_COPY_ID__);
    }
    // 832: two quick copies of a rotating prompt copy entries N and N+1 even when
    // the first commit reaches the backend late (they run on worker threads there).
    {
      const tileRot = [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === 'p10');
      if (tileRot) {
        // The copy cooldown is two seconds; the first commit lands after the second copy
        // has already asked for its entry.
        await sleep(2100); // the earlier copy of this tile is out of its cooldown
        window.__ROTATION_COMMIT_DELAY__ = 2600;
        const before = (window.__SMOKE_ARGS__ || []).length;
        for (let n = 0; n < 2; n++) {
          for (const type of ['pointerdown', 'pointerup', 'click']) {
            tileRot.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 1 }));
          }
          await sleep(2150);
        }
        await sleep(1500);
        window.__ROTATION_COMMIT_DELAY__ = 0;
        const copied = (window.__SMOKE_ARGS__ || []).slice(before).filter(([c]) => c === 'copy_text').map(([, a]) => a.text);
        if (copied.length !== 2) {
          out.errors.push('832: two quick copies of the rotating prompt made ' + copied.length + ' copies');
        } else if (copied[0] === copied[1]) {
          out.errors.push('832: two quick copies of the rotating prompt copied the same entry twice');
        }
      }
    }
    // A prompt whose entries carry placeholders must NOT arm one: the session
    // renders later entries from their raw text, so the next paste would deliver
    // the placeholder instead of the answer.
    window.__LAST_COPY_ID__ = undefined;
    window.__VAR_PAYLOAD__ = undefined;
    if (await clickTile('p1')) {
      await varAnswer((varReq() || { rows: [] }).rows.map(() => 'x'));
      if (window.__LAST_COPY_ID__ !== undefined) {
        out.errors.push('a placeholder prompt armed an auto-advance session: ' + window.__LAST_COPY_ID__);
      }
    }
    // The editor helper offers the reserved names as chips and writes a token.
    const editTile = [...document.querySelectorAll('#grid .tile')].find(e => (e.dataset.id || '') === 'p0');
    if (editTile) {
      editTile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(200);
      const edit = document.querySelector('#ctx [data-act="edit"]');
      if (edit) { edit.click(); await sleep(400); }
    }
    if (document.getElementById('modal').classList.contains('hidden')) {
      out.tabs.push('(variable helper check skipped: the editor did not open)');
    } else {
      layoutScan('prompt editor');
      collapseRows('editor');
      mustReach('prompt editor', '#modal .modal-x, #modal .modal-actions button');
      // 791: when the dialog scrolls, its sticky action row ends on the dialog's own
      // bottom edge, so no field ever shows below Save and Delete.
      {
        const box791 = document.querySelector('#modal .modal');
        const act791 = box791.querySelector(':scope > .modal-actions');
        if (box791.scrollHeight > box791.clientHeight + 2 && act791) {
          const gap791 = box791.getBoundingClientRect().bottom - act791.getBoundingClientRect().bottom;
          if (Math.abs(gap791) > 2) out.errors.push('791: the editor action row ends ' + Math.round(gap791) + 'px above the dialog edge at ' + window.innerWidth + 'x' + window.innerHeight);
        }
      }
      // 748: the prompt text is the first thing under the name row (the label of the
      // field may sit between), and at the app minimum size Save stays inside the
      // dialog without scrolling for a text prompt with a note and placeholders.
      {
        let el748 = document.querySelector('#modal .modal-name-row').nextElementSibling;
        let first748 = '';
        while (el748 && !first748) {
          if (el748.id === 'modal-text-wrap') first748 = 'ok';
          else if (el748.id !== 'modal-text-label' && el748.getClientRects().length > 0) first748 = el748.id || el748.className;
          el748 = el748.nextElementSibling;
        }
        if (first748 !== 'ok') out.errors.push('748: the prompt text does not follow the name row, ' + first748 + ' comes first');
        if (innerWidth <= 700 && innerHeight <= 470) {
          // Unscrolled: the earlier reach checks left the dialog scrolled to its end.
          document.querySelector('#modal .modal').scrollTop = 0;
          await sleep(50);
          const mr748 = document.querySelector('#modal .modal').getBoundingClientRect();
          const cr748 = document.getElementById('modal-confirm').getBoundingClientRect();
          if (cr748.bottom > mr748.bottom + 1 || cr748.top < mr748.top - 1) {
            out.errors.push('748: Save is outside the dialog at the minimum size: ' + Math.round(cr748.top) + '..' + Math.round(cr748.bottom) + ' of ' + Math.round(mr748.top) + '..' + Math.round(mr748.bottom));
          }
        }
      }
      // The variable helper is a reference, so it opens collapsed and costs no height.
      const varsBody = document.getElementById('modal-vars-body');
      const varsHead = document.getElementById('modal-vars-head');
      if (!varsBody || !varsHead) {
        out.errors.push('the variable helper has no collapsible section');
      } else {
        if (!varsBody.classList.contains('hidden')) {
          out.errors.push('the variable helper did not open collapsed');
        }
        // var263: a prompt without placeholders shows no bar, only a link in the foot row
        // under the text; the link reveals the bar, opens it and focuses the name field.
        const vBar263 = document.getElementById('modal-text-bar');
        const vLink263 = document.getElementById('modal-vars-link');
        if (!vBar263.classList.contains('hidden')) out.errors.push('var263: the placeholder bar shows for a prompt without placeholders');
        if (!vLink263 || vLink263.classList.contains('hidden') || getComputedStyle(vLink263).display === 'none' || !vLink263.closest('.modal-text-foot')) {
          out.errors.push('var263: no Insert placeholder link in the foot row of the prompt field');
        } else {
          vLink263.click();
          await sleep(150);
          if (vBar263.classList.contains('hidden')) out.errors.push('var263: the placeholder bar stays hidden after the link was clicked');
          if (!vLink263.classList.contains('hidden')) out.errors.push('var263: the link stays after the bar appeared');
          if (document.activeElement !== document.getElementById('modal-var-name')) out.errors.push('var263: the placeholder name field is not focused after the link');
        }
        if (varsBody.classList.contains('hidden')) {
          out.errors.push('the variable helper did not expand when its head was clicked');
        }
        // Expanding it must PUSH the rows below it down, never land on them. The
        // textarea sits inside a display:contents wrapper, so the generic row scan
        // cannot see it - compare it with the colour row directly.
        const taField = document.getElementById('modal-text');
        const colorRow = document.getElementById('color-row');
        const field = taField && taField.getClientRects().length
          ? taField
          : document.getElementById('modal-md-wrap');
        if (field && colorRow) {
          const a = field.getBoundingClientRect(), b = colorRow.getBoundingClientRect();
          if (a.height && b.height && b.top < a.bottom - 1) {
            const wrap = field.closest('.clear-wrap-ta') || field.parentElement;
            const w = wrap.getBoundingClientRect();
            const cs = getComputedStyle(field), cw = getComputedStyle(wrap);
            out.errors.push('the expanded variable helper left the text field over the colour row by ' +
              Math.round(a.bottom - b.top) + 'px [modal=' + document.getElementById('modal').className +
              ' field=' + Math.round(a.height) + '/' + cs.minHeight + '/' + cs.flex +
              ' wrap=' + wrap.className + ':' + Math.round(w.height) + '/' + cw.minHeight + '/' + cw.flex + ']');
          }
        }
        layoutScan('prompt editor, variables open');
      }
      const chips = [...document.querySelectorAll('#modal-vars-chips .vars-chip')];
      if (chips.length !== 16) {
        out.errors.push('the editor lists ' + chips.length + ' automatic placeholders instead of 16');
      } else if (chips.some(c => !c.title.trim())) {
        out.errors.push('an automatic placeholder chip has no explanation');
      }
      // Every placeholder chip looks the same: singling @clipboard out read as
      // "this one is selected", which it never is.
      const clipChip = chips.find(c => c.textContent === '@clipboard');
      if (!clipChip) out.errors.push('the clipboard placeholder chip is missing');
      if (chips.some(c => c.classList.contains('vars-chip-highlight'))) {
        out.errors.push('a placeholder chip is still highlighted');
      }
      const ta = document.getElementById('modal-text');
      const before = ta.value;
      chips[0] && chips[0].click();
      await sleep(150);
      // A reserved name is written with an @ in front, so the plain word stays
      // available as a variable of the user's own.
      if (ta.value === before || !ta.value.includes('{?@date}')) {
        out.errors.push('clicking a placeholder chip did not insert it: ' + ta.value);
      }
      // P33: a chip inserts at the CURSOR, not appended to the end of the text.
      ta.value = 'Say AAA then BBB.';
      ta.focus();
      ta.setSelectionRange(8, 8); // right after "Say AAA "
      chips[1] && chips[1].click();
      await sleep(150);
      if (!ta.value.startsWith('Say AAA {?') || !ta.value.includes('then BBB.')) {
        out.errors.push('a chip did not insert at the cursor position: ' + ta.value);
      }
      document.getElementById('modal-var-name').value = 'tone';
      document.getElementById('modal-var-opts').value = ' formal , casual ,, ';
      document.getElementById('modal-var-add').click();
      await sleep(150);
      if (!ta.value.includes('{?tone|formal,casual}')) {
        out.errors.push('the helper did not insert a choice variable: ' + ta.value);
      }
      // 456: the notes field sits directly above the colour row, full width, no
      // heading, always visible (no more collapse) and holds the stored note.
      const nRow = document.getElementById('modal-notes');
      const nText = document.getElementById('modal-notes-text');
      const nameRow = document.querySelector('.modal-name-row');
      if (!nRow || getComputedStyle(nRow).display === 'none') {
        out.errors.push('the editor has no notes row');
      } else if (nRow.nextElementSibling !== document.getElementById('color-row')) {
        out.errors.push('the notes field is not right above the colour row');
      } else if (document.getElementById('modal-notes-head')) {
        out.errors.push('the notes field still has a collapse header');
      } else if (!nText.value.includes('Moonshot')) {
        out.errors.push('the editor did not load the stored note');
      } else {
        // P35: the note field reads as a different kind of box, not a second
        // prompt field - its background must not match the main text area's.
        const notesBg = getComputedStyle(nText).backgroundColor;
        const promptBg = getComputedStyle(document.getElementById('modal-text')).backgroundColor;
        if (notesBg === promptBg) out.errors.push('the notes field looks identical to the prompt field');
        // The field carries a visible label inside its wrapper that points at the textarea.
        const nLabel = nRow.querySelector('label');
        if (!nLabel) out.errors.push('456: the notes field has no label');
        else {
          if (nLabel.getAttribute('for') !== 'modal-notes-text') out.errors.push('456: the notes label does not point at the notes field');
          if (nLabel.getBoundingClientRect().height < 4 || getComputedStyle(nLabel).visibility === 'hidden') out.errors.push('456: the notes label is not visible');
          if (nLabel.getBoundingClientRect().bottom > nText.getBoundingClientRect().top + 1) out.errors.push('456: the notes label is not above the notes field');
        }
        // 456: with the feature off, the gate hides the wrapper itself (not just
        // the textarea), so no empty gap is left where the field used to be.
        document.body.classList.add('noflag-promptNotes');
        await sleep(30);
        if (getComputedStyle(nRow).display !== 'none') {
          out.errors.push('promptNotes off does not hide the notes row');
        }
        document.body.classList.remove('noflag-promptNotes');
        await sleep(30);
      }
      // F20: the hotkey row opens collapsed (28: as part of "Advanced options" now),
      // shows the stored accelerator, refuses a combination that is already taken,
      // accepts a free one and can be cleared.
      const hRow = document.getElementById('modal-hotkey');
      const hBody = document.getElementById('modal-advanced-body');
      const hField = document.getElementById('modal-hotkey-input');
      const hErr = document.getElementById('modal-hotkey-err');
      const press = async (opts) => {
        hField.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, opts)));
        await sleep(150);
      };
      if (!hRow || getComputedStyle(hRow).display === 'none') {
        out.errors.push('the editor has no hotkey row');
      } else if (!hBody.classList.contains('hidden')) {
        out.errors.push('advanced options does not start collapsed');
      } else {
        document.getElementById('modal-advanced-head').click();
        await sleep(250);
        if (hBody.classList.contains('hidden')) out.errors.push('advanced options did not expand');
        if (!/9$/.test(hField.value) || !/\\+/.test(hField.value)) {
          out.errors.push('the stored hotkey is not shown readably: ' + hField.value);
        }
        const badge = document.getElementById('modal-hotkey-badge');
        if (badge.classList.contains('hidden') || !badge.textContent.trim()) {
          out.errors.push('the collapsed hotkey row shows no binding');
        }
        // The app hotkey is Ctrl+Shift+P - a prompt must not be able to take it.
        const before = hField.value;
        await press({ key: 'P', ctrlKey: true, shiftKey: true });
        if (hErr.classList.contains('hidden') || !hErr.textContent.trim()) {
          out.errors.push('a taken combination produced no inline error');
        }
        if (hField.value !== before) out.errors.push('a rejected combination was captured anyway');
        // A modifier-less press is refused too, with a reason.
        await press({ key: 'K' });
        if (hErr.classList.contains('hidden')) out.errors.push('a modifier-less key produced no inline error');
        // A universal system shortcut is refused by the backend, never captured.
        const beforeSys = hField.value;
        await press({ key: 'c', ctrlKey: true });
        if (hErr.classList.contains('hidden') || !hErr.textContent.trim()) {
          out.errors.push('Ctrl+C produced no inline error');
        }
        if (hField.value !== beforeSys) out.errors.push('a system shortcut was captured anyway: ' + hField.value);
        // 430: whatever the reason, a button under the message opens the full list
        // instead of leaving the user to guess what already holds the combination.
        const showAll = document.getElementById('modal-hotkey-showall');
        if (!showAll || showAll.classList.contains('hidden')) {
          out.errors.push('430: no "show all hotkeys" button next to a hotkey error');
        } else {
          showAll.click();
          await sleep(200);
          const allPopup = document.getElementById('shortcuts-modal');
          if (allPopup.classList.contains('hidden') || document.getElementById('shortcuts-global').classList.contains('hidden')) {
            out.errors.push('430: the show-all button did not open the hotkeys list');
          }
          document.getElementById('shortcuts-modal-close').click();
          await sleep(150);
        }
        // A combination another running app owns is refused the same way.
        await press({ key: 'B', ctrlKey: true, altKey: true });
        if (hErr.classList.contains('hidden')) out.errors.push('a combination owned by another app produced no inline error');
        if (hField.value !== beforeSys) out.errors.push('a conflicting combination was captured anyway: ' + hField.value);
        // A free combination is captured and clears the error.
        await press({ key: 'J', ctrlKey: true, altKey: true });
        if (!hErr.classList.contains('hidden')) out.errors.push('the inline error survived a valid capture');
        if (!/J$/.test(hField.value)) out.errors.push('a free combination was not captured: ' + hField.value);
        // F20: the copy-vs-paste choice belongs to THIS hotkey, and its default
        // entry has to spell out what it inherits.
        const actRow = document.getElementById('modal-hotkey-action-row');
        const actSel = document.getElementById('modal-hotkey-action');
        if (!actRow || actRow.classList.contains('hidden')) {
          out.errors.push('a bound hotkey shows no copy/paste choice of its own');
        } else {
          const def = actSel.querySelector('option[value=""]');
          if (!def || !/\\(.+\\)/.test(def.textContent || '')) {
            out.errors.push('the default entry does not name the inherited mode: ' + (def && def.textContent));
          }
          if ([...actSel.options].map(o => o.value).join(',') !== ',copy,paste') {
            out.errors.push('the hotkey action dropdown offers the wrong entries');
          }
        }
        document.getElementById('modal-hotkey-clear').click();
        await sleep(150);
        if (hField.value) out.errors.push('the clear button left a hotkey behind: ' + hField.value);
        if (actRow && !actRow.classList.contains('hidden')) {
          out.errors.push('the copy/paste choice stayed visible without a binding');
        }
      }
      // Both collapsibles are open now - the state that used to squeeze them to a
      // 6px sliver and push Save out of reach on a short window.
      layoutScan('prompt editor, rows expanded');
      collapseRows('editor');
      mustReach('prompt editor, rows expanded', '#modal .modal-x, #modal .modal-actions button');
      await click('modal-close-x');
      await sleep(250);
      // Editing made the dialog dirty, so the discard confirmation may step in.
      const cm2 = document.getElementById('confirm-modal');
      if (!cm2.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
    }
    // Item 1: "All hotkeys" popup - global + every per-prompt hotkey in one list,
    // each row live-rebindable with the same collision/free-combo behaviour the
    // editor's own field already has. Prompt 1 (p0) starts with a bound hotkey in
    // the fixture; every mutating step below restores it before the block ends, so
    // later blocks that fire a 'prompt-hotkey' event for p0/p1/p2 see it unchanged.
    {
      await click('gear');
      const listBtn = document.getElementById('hotkey-list-btn');
      if (!listBtn) {
        out.errors.push('the "All hotkeys" button is missing next to the global hotkey field');
      } else {
        listBtn.click();
        await sleep(150);
        const popup = document.getElementById('shortcuts-modal');
        if (popup.classList.contains('hidden') || document.getElementById('shortcuts-global').classList.contains('hidden')) {
          out.errors.push('the "All shortcuts" dialog did not open with the global hotkeys on top');
        } else {
          const rowByName = (name) => [...document.querySelectorAll('.hotkeys-row-name')]
            .find((el) => el.textContent === name)?.closest('.hotkeys-row');
          const globalRow = () => document.querySelector('.hotkeys-row-global')?.closest('.hotkeys-row');
          const globalInput = () => globalRow()?.querySelector('.modal-input');
          const p1Input = () => rowByName('Prompt 1')?.querySelector('.modal-input');
          if (!globalRow() || !/P$/.test(globalInput().value)) {
            out.errors.push('the popup does not list the global hotkey');
          }
          if (!rowByName('Prompt 1') || !/9$/.test(p1Input().value)) {
            out.errors.push('the popup does not list Prompt 1 with its stored hotkey');
          }
          if (rowByName('Prompt 4')) out.errors.push('the popup lists a prompt with no hotkey bound');
          // 343: the recording stop/pause hotkeys moved out of Settings and into the
          // expert menu's own capture row, but still show up here too, same as every
          // other hotkey in the app.
          const recStopInput = rowByName('Stop-recording hotkey')?.querySelector('.modal-input');
          const recPauseInput = rowByName('Pause/resume-recording hotkey')?.querySelector('.modal-input');
          if (!recStopInput || !/8$/.test(recStopInput.value)) {
            out.errors.push('the popup does not list the stop-recording hotkey with its stored value');
          }
          if (!recPauseInput || !/7$/.test(recPauseInput.value)) {
            out.errors.push('the popup does not list the pause-recording hotkey with its stored value');
          }
          if (document.querySelectorAll('.hotkeys-row').length !== 4) {
            out.errors.push('expected 4 rows in the "All hotkeys" popup (global, one prompt, two recording), got ' +
              document.querySelectorAll('.hotkeys-row').length);
          }
          // 672: the same dialog lists the keys of every window, one titled section each,
          // including view switching in the main window.
          const secTitles = [...document.querySelectorAll('#shortcuts-modal-list .shortcuts-section-title')].map((h) => h.textContent);
          const wantTitles = ['Main window', 'Quick launcher', 'Tray menu', 'Placeholder window', 'Floating buttons and folders', 'Screenshot selection', 'Recording bar', 'Camera'];
          if (secTitles.join('|') !== wantTitles.join('|')) {
            out.errors.push('672: the shortcut overview sections are: ' + secTitles.join(', '));
          }
          if (![...document.querySelectorAll('#shortcuts-modal-list .shortcuts-row-keys')].some((k) => k.textContent === 'Ctrl+Tab')) {
            out.errors.push('672: the shortcut overview does not list Ctrl+Tab');
          }
          // Search filters by name - a prompt-name query hides the global row.
          const search = document.getElementById('hotkeys-search');
          search.value = 'Prompt 1';
          search.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(80);
          if (!rowByName('Prompt 1')) out.errors.push('the search hid a matching row');
          if (globalRow()) out.errors.push('the search did not filter out the non-matching global row');
          search.value = '';
          search.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(80);
          const pressRow = async (input, opts) => {
            input.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, opts)));
            await sleep(150);
          };
          // A combination owned by another running app is refused, unchanged.
          const beforeP1 = p1Input().value;
          await pressRow(p1Input(), { key: 'B', ctrlKey: true, altKey: true });
          if (p1Input().value !== beforeP1) out.errors.push('the popup captured a combination owned by another app');
          // A free combination is captured and actually persisted (update_prompt).
          window.__SMOKE_ARGS__ = [];
          await pressRow(p1Input(), { key: 'K', ctrlKey: true, altKey: true });
          if (!/K$/.test(p1Input().value)) out.errors.push('the popup did not capture a free combination: ' + p1Input().value);
          const upd = (window.__SMOKE_ARGS__ || []).find(([cmd]) => cmd === 'update_prompt');
          if (!upd || !/\\+K$/i.test(upd[1].hotkey || '')) {
            out.errors.push('rebinding a popup row did not persist through update_prompt');
          }
          // Restore Prompt 1's original hotkey so later blocks see it unchanged.
          await pressRow(p1Input(), { key: '9', ctrlKey: true, shiftKey: true });
          if (!/9$/.test(p1Input().value)) out.errors.push('restoring Prompt 1s original hotkey failed: ' + p1Input().value);
          // The global row clears (and restores) the same way - it never disappears
          // from the list even with no binding, unlike a per-prompt row.
          const clearBtn = globalRow().querySelector('.clear-btn');
          clearBtn.click();
          await sleep(150);
          if (globalInput().value) out.errors.push('clearing the global row from the popup left a value behind');
          if (!globalRow()) out.errors.push('the global row disappeared after being cleared');
          await pressRow(globalInput(), { key: 'P', ctrlKey: true, shiftKey: true });
          if (!/P$/.test(globalInput().value)) out.errors.push('restoring the global hotkey failed: ' + globalInput().value);
        }
        document.getElementById('shortcuts-modal-close').click();
        await sleep(120);
      }
      document.getElementById('settings-close')?.click();
      await sleep(120);
    }
"""
