SECTION = """\
    // P20: the library preview's line count is an expert value (default 2),
    // not a hardcoded CSS number.
    if (getComputedStyle(document.documentElement).getPropertyValue('--lib-preview-lines').trim() !== '2') {
      out.errors.push('the library preview line-count CSS var is not applied');
    }
    // 707: an automatic tile text wraps and grows to the largest size that fits its
    // box, with no cap below the box: a short name fills the tile, a long one wraps
    // without overflowing. Driven through the real fitTileText on detached tiles.
    {
      let fit707 = null;
      try { fit707 = (await import('./tiles.js')).fitTileText; } catch (e) { out.errors.push('707: tiles.js not importable: ' + e); }
      if (fit707) {
        const host707 = document.createElement('div');
        host707.style.cssText = 'position:fixed;left:0;top:0;width:200px;visibility:hidden;';
        document.body.appendChild(host707);
        const lines707 = (el) => { const rg = document.createRange(); rg.selectNodeContents(el); return new Set([...rg.getClientRects()].map((q) => Math.round(q.top))).size; };
        for (const [label707, text707] of [['short', 'Prompt 2'], ['long', 'Write a friendly reply to the customer about the delayed order and offer a discount']]) {
          const tile707 = document.createElement('div');
          // No .tile class: its padding would narrow the name below the measured box.
          tile707.dataset.fitMode = 'auto';
          const nm707 = document.createElement('span');
          nm707.className = 'tile-name';
          nm707.textContent = text707;
          tile707.appendChild(nm707);
          host707.appendChild(tile707);
          fit707(tile707, 200, 96);
          const px707 = parseFloat(nm707.style.fontSize);
          if (nm707.scrollHeight > 96 || nm707.scrollWidth > 200) out.errors.push('707: the ' + label707 + ' name overflows its box (' + nm707.scrollWidth + 'x' + nm707.scrollHeight + ')');
          if (label707 === 'short' && !(px707 > 32)) out.errors.push('707: the short name stays small at ' + px707 + 'px instead of filling the tile');
          if (label707 === 'long' && lines707(nm707) < 2) out.errors.push('707: the long name did not wrap');
        }
        host707.remove();
      }
    }
    // P32: font/size are advanced options now: the style row sits inside the
    // collapsed Advanced body, after the prompt text. A pure DOM-order check.
    {
      const styleRow = document.getElementById('modal-style-row');
      const advBody = document.getElementById('modal-advanced-body');
      const textWrap = document.getElementById('modal-text-wrap');
      if (!styleRow || !advBody || !textWrap) {
        out.errors.push('the editor is missing the style row, advanced body or text wrap');
      } else {
        if (!advBody.contains(styleRow)) out.errors.push('the font/size row is not inside the advanced options');
        if (!(textWrap.compareDocumentPosition(styleRow) & Node.DOCUMENT_POSITION_FOLLOWING)) {
          out.errors.push('the font/size row is not below the prompt text');
        }
      }
    }
    // The tab strip is the piece the header used to crush: below ~700px it kept a
    // ~45px sliver of a 244px strip, and every tab after the first became untouchable.
    const strip = document.getElementById('views');
    if (strip && !strip.classList.contains('hidden')) {
      const stripTabs = [...strip.querySelectorAll('.view-btn')];
      const widest = Math.max(0, ...stripTabs.map(t => t.getBoundingClientRect().width));
      if (strip.clientWidth < Math.min(widest, 90)) {
        out.layout.push('main window: the view tab strip is only ' + strip.clientWidth +
          'px wide (one tab is ' + Math.round(widest) + 'px)');
      }
      const missed = stripTabs.filter(t => !canHit(t));
      if (missed.length) {
        out.layout.push('main window: ' + missed.length + ' of ' + stripTabs.length +
          ' view tabs cannot be clicked');
      }
      strip.scrollLeft = 0;
    }
    // fav249: a favorite tile carries one display-only star in its top-right quarter, a
    // plain tile none; the expert switch and the favorites master switch both remove it.
    {
      const coreF = await import('./core.js');
      const viewsF = await import('./views.js');
      const favOf = (id) => ((coreF.prompts.find((x) => x.id === id)) || {}).favorite === true;
      const tilesF = () => [...document.querySelectorAll('#grid .tile[data-id]:not(.folder-tile)')];
      const stars249 = () => document.querySelectorAll('#grid .tile-fav-badge').length;
      const favTile = tilesF().find((x) => favOf(x.dataset.id));
      const plainTile = tilesF().find((x) => !favOf(x.dataset.id));
      if (!favTile || !plainTile) out.errors.push('fav249: the grid has no favorite and plain tile to compare');
      else {
        const stars = favTile.querySelectorAll('.tile-fav-badge');
        if (stars.length !== 1) out.errors.push('fav249: a favorite tile has ' + stars.length + ' stars, expected 1');
        else {
          const tr = favTile.getBoundingClientRect();
          const sr = stars[0].getBoundingClientRect();
          if (sr.left + sr.width / 2 < tr.left + tr.width / 2 || sr.top + sr.height / 2 > tr.top + tr.height / 2) {
            out.errors.push('fav249: the star is not in the tile top-right quarter');
          }
          if (getComputedStyle(stars[0]).pointerEvents !== 'none') out.errors.push('fav249: the star takes clicks');
        }
        if (plainTile.querySelector('.tile-fav-badge')) out.errors.push('fav249: a plain tile has a star');
        const flags0 = coreF.settings.ui_flags;
        for (const off of ['tileFavBadge', 'favorites']) {
          coreF.settings.ui_flags = { ...(flags0 || {}), [off]: false };
          await viewsF.renderGrid(true);
          if (stars249() !== 0) out.errors.push('fav249: ' + off + ' off still shows ' + stars249() + ' stars');
        }
        coreF.settings.ui_flags = flags0;
        await viewsF.renderGrid(true);
        if (stars249() < 1) out.errors.push('fav249: the stars did not come back after the switches went on again');
      }
    }
    // fb257: the untinted folder tile has a visible frame in every built-in theme
    // (border against the page background, WCAG contrast of at least 1.45).
    {
      const themeMod = await import('./theme.js');
      const rootF = document.documentElement;
      const prevTheme = rootF.getAttribute('data-theme');
      const noMotion = document.createElement('style');
      noMotion.textContent = '* { transition: none !important; animation: none !important; }';
      document.head.appendChild(noMotion);
      const toRgb = (css) => {
        const m = /^color[(]srgb ([0-9.e-]+) ([0-9.e-]+) ([0-9.e-]+)/.exec(css);
        if (m) return [Number(m[1]), Number(m[2]), Number(m[3])];
        const n = /^rgba?[(]([0-9.]+)[, ]+([0-9.]+)[, ]+([0-9.]+)/.exec(css);
        return n ? [Number(n[1]) / 255, Number(n[2]) / 255, Number(n[3]) / 255] : null;
      };
      const lum = (c) => { const f = (v) => (v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)); return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]); };
      const ratio = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
      const folderF = document.querySelector('#grid .folder-tile:not(.tinted)');
      if (!folderF) out.errors.push('fb257: no untinted folder tile on the grid (fixture)');
      else {
        for (const themeId of themeMod.BUILTIN_THEMES) {
          rootF.setAttribute('data-theme', themeId);
          await sleep(60);
          const border = toRgb(getComputedStyle(folderF).borderTopColor);
          const page = toRgb(getComputedStyle(document.body).backgroundColor);
          if (!border || !page) { out.errors.push('fb257: ' + themeId + ': colour not readable: ' + getComputedStyle(folderF).borderTopColor + ' on ' + getComputedStyle(document.body).backgroundColor); continue; }
          const r257 = ratio(border, page);
          if (r257 < 1.45) out.errors.push('fb257: the folder frame in ' + themeId + ' has contrast ' + r257.toFixed(2) + ', want 1.45');
        }
      }
      if (prevTheme === null) rootF.removeAttribute('data-theme'); else rootF.setAttribute('data-theme', prevTheme);
      noMotion.remove();
    }
    // F27: the grid folder draws its own cell, hides its members as tiles and opens
    // a popover holding them.
    const fTile = document.querySelector('.tile.folder-tile');
    if (!fTile) {
      out.errors.push('the grid folder has no tile');
    } else {
      if (document.querySelector('.tile[data-id="p4"]')) {
        out.errors.push('a prompt inside the folder is still a tile of its own');
      }
      // 708: a folder is a normal card: the size of a neighbouring prompt tile, the
      // surface colour, a preview without a plate of its own and the title at the bottom.
      {
        const prompt708 = document.querySelector('.tile:not(.folder-tile)[data-id]');
        const fr708 = fTile.getBoundingClientRect();
        if (prompt708) {
          const pr708 = prompt708.getBoundingClientRect();
          if (Math.abs(fr708.width - pr708.width) > 1 || Math.abs(fr708.height - pr708.height) > 1) {
            out.errors.push('708: the folder tile is ' + Math.round(fr708.width) + 'x' + Math.round(fr708.height) + ', a prompt tile ' + Math.round(pr708.width) + 'x' + Math.round(pr708.height));
          }
        } else out.errors.push('708: no prompt tile next to the folder to compare with');
        const probe708 = document.createElement('div');
        probe708.style.background = 'var(--surface)';
        document.body.appendChild(probe708);
        const surface708 = getComputedStyle(probe708).backgroundColor;
        probe708.remove();
        if (!fTile.classList.contains('tinted') && getComputedStyle(fTile).backgroundColor !== surface708) {
          out.errors.push('708: the folder tile is ' + getComputedStyle(fTile).backgroundColor + ', not the surface colour ' + surface708);
        }
        const mini708 = fTile.querySelector('.folder-mini');
        const title708 = fTile.querySelector('.folder-title');
        if (mini708 && getComputedStyle(mini708).backgroundColor !== 'rgba(0, 0, 0, 0)') out.errors.push('708: the folder preview still has a plate of its own');
        if (mini708) {
          const mr708 = mini708.getBoundingClientRect();
          if (mr708.right > fr708.right + 1 || mr708.bottom > fr708.bottom + 1 || mr708.left < fr708.left - 1 || mr708.top < fr708.top - 1) out.errors.push('708: the folder preview leaves its tile');
        }
        if (mini708 && title708) {
          const tr708 = title708.getBoundingClientRect();
          if (tr708.top < mini708.getBoundingClientRect().bottom - 1) out.errors.push('708: the folder title does not sit below the preview');
          if (tr708.bottom > fr708.bottom + 1) out.errors.push('708: the folder title leaves its tile');
        }
      }
      const anchor = fTile.getBoundingClientRect();
      fTile.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: 5, clientY: 5 }));
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, clientX: 5, clientY: 5 }));
      await sleep(200);
      const pop = document.querySelector('.tile-folder-pop:not(.hidden)');
      const members = pop ? pop.querySelectorAll('.folder-member-tile').length : 0;
      if (members !== 5) out.errors.push('the open folder shows ' + members + ' of 5 entries');
      // 30: a member is as big as a grid tile (the folder cell) whenever three of
      // them fit the window side by side, and no member's name overflows its cell.
      if (pop) {
        // Layout sizes, not rects: the popover's pop-in animation scales it for a moment.
        const m30 = pop.querySelector('.folder-member-tile');
        const cellRect = m30 && { width: m30.offsetWidth, height: m30.offsetHeight };
        const fitsThree = 3 * anchor.width + 2 * 6 + 18 <= window.innerWidth * 0.96;
        if (!cellRect) {
          out.errors.push('30: the open folder has no member tile to measure');
        } else if (fitsThree && (Math.abs(cellRect.width - anchor.width) > 1.5 || Math.abs(cellRect.height - anchor.height) > 1.5)) {
          out.errors.push('30: a folder member is ' + Math.round(cellRect.width) + 'x' + Math.round(cellRect.height) + ', not the grid tile size ' + Math.round(anchor.width) + 'x' + Math.round(anchor.height));
        } else if (!fitsThree && 3 * cellRect.width > window.innerWidth) {
          out.errors.push('30: three folder members are wider than the window (' + Math.round(cellRect.width) + ' each)');
        }
        // Compare rendered rects, not scrollHeight/clientHeight: the auto-fit
        // text-box-trim on .tile-name.fit intentionally reports a layout height
        // smaller than its line box (optical centering) without actually
        // clipping any glyph, so that pairing is not a reliable overflow signal.
        const overflowing = [...pop.querySelectorAll('.folder-member-tile')].filter((btn) => {
          const nameEl = btn.querySelector('.tile-name');
          if (!nameEl || getComputedStyle(nameEl).display === 'none') return false;
          const b = btn.getBoundingClientRect(), n = nameEl.getBoundingClientRect();
          return n.left < b.left - 1 || n.right > b.right + 1 || n.top < b.top - 1 || n.bottom > b.bottom + 1;
        });
        if (overflowing.length) out.errors.push('30: ' + overflowing.length + ' folder member name(s) overflow their cell');
      }
      // The popover opens beside its tile when the rows fit there. Where they do not, it
      // may open over the tile (the grid is dimmed behind it) but then shows every row
      // inside the window without scrolling.
      if (pop) {
        const r = pop.getBoundingClientRect();
        const clear = r.top >= anchor.bottom - 1 || r.bottom <= anchor.top + 1 ||
          (pop.scrollHeight <= pop.clientHeight + 1 && r.top >= -1 && r.bottom <= window.innerHeight + 1);
        if (!clear) {
          out.errors.push('the folder popover covers the tile it belongs to (' +
            Math.round(r.top) + '..' + Math.round(r.bottom) + ' vs tile ' +
            Math.round(anchor.top) + '..' + Math.round(anchor.bottom) + ')');
        }
      }
      layoutScan('grid folder');
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
      await sleep(120);
      // 70: dragging in the colour picker fires on every pointer-move frame - must
      // paint just this one folder tile in place, not rebuild the whole grid.
      fTile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(150);
      const colorBtn = document.querySelectorAll('.tab-menu button')[1];
      if (!colorBtn) {
        out.errors.push('70: the grid folder context menu has no colour option');
      } else {
        colorBtn.click();
        await sleep(150);
        const pick = document.querySelector('.color-pop:not(.hidden)');
        const sv = pick ? pick.querySelector('.cp-sv') : null;
        if (!pick || !sv) {
          out.errors.push('70: the folder colour picker did not open');
        } else {
          const r = sv.getBoundingClientRect();
          sv.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 1, clientX: r.left + 5, clientY: r.top + 5 }));
          sv.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: r.left + 25, clientY: r.top + 25 }));
          await sleep(30);
          if (document.querySelector('.tile.folder-tile') !== fTile) {
            out.errors.push('70: painting a folder colour rebuilt the whole grid instead of the one tile');
          }
          if (!fTile.style.background) {
            out.errors.push('70: the folder tile did not pick up the live colour preview');
          }
        }
        document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
        await sleep(120);
      }
      // Dissolving must not just remove the folder tile - every member has to land
      // back on the grid as its own tile, not vanish into the library.
      fTile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(150);
      const dissolveBtn = document.querySelector('.tab-menu button.danger');
      if (!dissolveBtn) {
        out.errors.push('the grid folder context menu has no dissolve option');
      } else {
        dissolveBtn.click();
        await sleep(150);
        const cmDissolve = document.getElementById('confirm-modal');
        if (cmDissolve && !cmDissolve.classList.contains('hidden')) { await click('confirm-ok'); await sleep(250); }
        if (document.querySelector('.tile.folder-tile')) {
          out.errors.push('the folder tile survived dissolving it');
        }
        for (const mid of ['p4', 'p6', 'p8', 'p10', 'p11']) {
          if (!document.querySelector('.tile[data-id="' + mid + '"]')) {
            out.errors.push('dissolving the folder did not bring back ' + mid + ' as a tile');
          }
        }
      }
    }
    // P50: right-clicking a view tab's "Duplicate" entry creates a new, empty view
    // with the same name (+ suffix), color and grid size - cleaned up right after
    // so it doesn't throw off the view-count checks further down.
    {
      const homeTab = document.querySelector('.view-btn[data-view-id="home"]');
      if (!homeTab) {
        out.errors.push('the "home" view tab is missing for the duplicate-view check');
      } else {
        const tabCountBefore = document.querySelectorAll('#views .view-btn:not(.view-add-top)').length;
        const menuItem = (text) => [...document.querySelectorAll('.tab-menu button[role="menuitem"]')]
          .find(el => el.textContent.trim() === text);
        homeTab.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }));
        await sleep(150);
        const dupItem = menuItem('Duplicate');
        if (!dupItem) {
          out.errors.push('the view tab context menu has no Duplicate entry');
          // English-only lookup above: under another language this never matches,
          // so the menu has to be closed by hand (its own dismiss listens for a
          // pointerdown outside it) or it leaks into later layout checks, sitting
          // wherever it was opened.
          document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
          await sleep(100);
        } else {
          dupItem.click();
          await sleep(300);
          const tabsAfter = [...document.querySelectorAll('#views .view-btn:not(.view-add-top)')];
          if (tabsAfter.length !== tabCountBefore + 1) {
            out.errors.push('duplicating a view did not add exactly one tab');
          } else {
            const newTab = tabsAfter.find(b => (b.querySelector('span') || {}).textContent === 'Home (copy)');
            if (!newTab) {
              out.errors.push('the duplicated view is not named "Home (copy)"');
            } else {
              // Clean up: delete the duplicate (one click, undo toast) so later view-count
              // assertions see the same tab bar they expect.
              newTab.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }));
              await sleep(150);
              const editItem = menuItem('Edit view');
              if (editItem) editItem.click();
              await sleep(200);
              const delBtn = document.getElementById('view-modal-delete');
              if (delBtn && !delBtn.classList.contains('hidden')) {
                delBtn.click(); await sleep(300);
              }
              // 434: the deleted view offers an Undo button - click it and prove the
              // view (name, member) really comes back, not just a bare empty tab.
              const viewUndoToast = document.getElementById('toast');
              const viewUndoBtn = viewUndoToast.querySelector('.toast-btn');
              if (!viewUndoToast.classList.contains('actionable') || !viewUndoBtn) {
                out.errors.push('434: deleting a view did not offer an undo toast');
              } else {
                viewUndoBtn.click();
                await sleep(300);
                const tabsRestored = [...document.querySelectorAll('#views .view-btn:not(.view-add-top)')];
                if (tabsRestored.length !== tabCountBefore + 1) {
                  out.errors.push('434: undoing a deleted view did not bring the tab back');
                } else if (!tabsRestored.some(b => (b.querySelector('span') || {}).textContent === 'Home (copy)')) {
                  out.errors.push('434: the restored view lost its name');
                }
                if (viewUndoToast.classList.contains('show')) {
                  out.errors.push('434: the view-undo toast is still showing after being clicked');
                }
                // Delete it again for real (no undo this time) so the tab bar ends
                // this check exactly where later view-count assertions expect it.
                const restoredTab = tabsRestored.find(b => (b.querySelector('span') || {}).textContent === 'Home (copy)');
                if (restoredTab) {
                  restoredTab.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }));
                  await sleep(150);
                  const editItem2 = menuItem('Edit view');
                  if (editItem2) editItem2.click();
                  await sleep(200);
                  const delBtn2 = document.getElementById('view-modal-delete');
                  if (delBtn2 && !delBtn2.classList.contains('hidden')) {
                    delBtn2.click(); await sleep(300);
                  }
                }
              }
              const tabsFinal = document.querySelectorAll('#views .view-btn:not(.view-add-top)').length;
              if (tabsFinal !== tabCountBefore) {
                out.errors.push('cleanup after the duplicate-view check left an extra tab behind');
              }
            }
          }
        }
      }
    }
    // 9: double-click an empty cell creates a prompt right there instead of typing
    // below and dragging it over. Done on "work" (an untouched, empty view) so it
    // never competes with the folder math above for a free cell.
    const workTab = document.querySelector('.view-btn[data-view-id="work"]');
    if (!workTab) {
      out.errors.push('the "work" view tab is missing for the double-click-create check');
    } else {
      workTab.click();
      await sleep(200);
      // P53: the grid keeps a CSS var of the active view's color, ready for the
      // (default-off) grid-border option even while the flag itself is off.
      if (getComputedStyle(document.documentElement).getPropertyValue('--active-view-color').trim() !== '#ef4444') {
        out.errors.push('the active-view-color CSS var did not follow the switch to "work"');
      }
      const cell00 = document.querySelector('#grid .cell[data-col="0"][data-row="0"]');
      if (!cell00 || cell00.firstElementChild) {
        out.errors.push('expected an empty cell on "work" to double-click-create into');
      } else {
        cell00.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(200);
        const modalEl = document.getElementById('modal');
        if (!modalEl || modalEl.classList.contains('hidden')) {
          out.errors.push('double-clicking an empty cell did not open the create dialog');
        } else {
          document.getElementById('modal-name').value = 'Grid Click Prompt';
          document.getElementById('modal-text').value = 'Created by double-click.';
          document.getElementById('modal-confirm').click();
          await sleep(250);
          const placed = document.querySelector('#grid .cell[data-col="0"][data-row="0"] .tile');
          if (!placed) {
            out.errors.push('double-click-create did not land the new prompt on the clicked cell');
          } else if ((placed.querySelector('.tile-name') || {}).textContent !== 'Grid Click Prompt') {
            out.errors.push('the tile on the double-clicked cell is not the prompt just created');
          }
          // P89: creating a prompt runs the same near-duplicate check import does.
          await sleep(200);
          const toastEl2 = document.getElementById('toast');
          if (!(toastEl2.textContent || '').toLowerCase().includes('duplicate')) {
            out.errors.push('creating a prompt did not warn about possible duplicates: ' + toastEl2.textContent);
          }
        }
      }
      // 34: a {?} placeholder with no name inside warns before the save goes
      // through, and "save anyway" still lets it through.
      const cell10 = document.querySelector('#grid .cell[data-col="1"][data-row="0"]');
      if (!cell10 || cell10.firstElementChild) {
        out.errors.push('expected a second empty cell on "work" for the empty-variable check');
      } else {
        cell10.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(200);
        document.getElementById('modal-name').value = 'Empty Var Prompt';
        document.getElementById('modal-text').value = 'Say {?} please.';
        document.getElementById('modal-confirm').click();
        await sleep(200);
        const cm = document.getElementById('confirm-modal');
        if (!cm || cm.classList.contains('hidden')) {
          out.errors.push('saving a prompt with an unnamed {?} variable did not warn');
        } else {
          if (document.getElementById('confirm-title').textContent !== 'Empty placeholder') {
            out.errors.push('the empty-variable warning has the wrong title');
          }
          document.getElementById('confirm-cancel').click(); // back to editing, not saved
          await sleep(150);
          if (document.getElementById('modal').classList.contains('hidden')) {
            out.errors.push('cancelling the empty-variable warning closed the editor instead of returning to it');
          } else {
            document.getElementById('modal-confirm').click(); // through again
            await sleep(200);
            document.getElementById('confirm-ok').click(); // save anyway
            await sleep(250);
            if (!document.querySelector('#grid .cell[data-col="1"][data-row="0"] .tile')) {
              out.errors.push('"save anyway" did not actually save the prompt');
            }
          }
        }
      }
      // A name or body that looks like markup must render as literal text, never
      // as a real element -- the one place a raw HTML string could ever reach the
      // page (the Markdown preview) escapes up front; every other renderer here
      // uses textContent, so this proves the tile path stays that way too.
      const xssCell = document.querySelector('#grid .cell[data-col="2"][data-row="0"]');
      if (!xssCell || xssCell.firstElementChild) {
        out.errors.push('expected a third empty cell on "work" for the injected-markup check');
      } else {
        xssCell.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(200);
        document.getElementById('modal-name').value = XSS_NAME;
        document.getElementById('modal-text').value = XSS_TEXT;
        document.getElementById('modal-confirm').click();
        await sleep(250);
        const xssTile = document.querySelector('#grid .cell[data-col="2"][data-row="0"] .tile');
        if (!xssTile) {
          out.errors.push('the injected-markup prompt was not placed on the grid');
        } else {
          const tileName = xssTile.querySelector('.tile-name');
          if (!tileName || tileName.textContent !== XSS_NAME) {
            out.errors.push('the grid tile does not show injected markup as literal text');
          } else if (tileName.querySelector('img') || tileName.querySelector('script')) {
            out.errors.push('the grid tile turned injected markup into a real element');
          }
        }
      }
      const homeTab = document.querySelector('.view-btn[data-view-id="home"]');
      if (homeTab) { homeTab.click(); await sleep(200); }
    }
    await click('library-btn');
    layoutScan('library');
    mustReach('library', '#library .modal-head-actions > button');
    {
      const xssRow = [...document.querySelectorAll('#library-list .lib-item')]
        .find((r) => (r.querySelector('.lib-name') || {}).textContent === XSS_NAME);
      if (!xssRow) {
        out.errors.push('the injected-markup prompt is missing from the library list');
      } else if (xssRow.querySelector('.lib-name img') || xssRow.querySelector('.lib-name script')) {
        out.errors.push('the library row turned injected markup into a real element');
      }
    }
    // 17: "last edited" sort - from the last_edited_map fixture (p3 newest, then
    // p1, then p9; everything else - including the favorited p0/p5/p10, which
    // float to the top independently of this sort - has no version row and
    // sorts to the end).
    {
      const sortSel = document.getElementById('library-sort');
      if (!sortSel || ![...sortSel.options].some(o => o.value === 'lastEdited')) {
        out.errors.push('the library sort dropdown has no "last edited" option');
      } else {
        sortSel.value = 'lastEdited';
        sortSel.dispatchEvent(new Event('change'));
        await sleep(250);
        const order = [...document.querySelectorAll('#library-list .lib-item')].map(el => el.dataset.id);
        const idx = (id) => order.indexOf(id);
        if (idx('p3') < 0 || idx('p1') < 0 || idx('p9') < 0) {
          out.errors.push('"last edited" sort is missing a fixture prompt: ' + order.join(','));
        } else if (!(idx('p3') < idx('p1') && idx('p1') < idx('p9'))) {
          out.errors.push('"last edited" sort order is wrong: ' + order.join(','));
        } else if (idx('p2') >= 0 && idx('p2') < idx('p9')) {
          out.errors.push('a prompt with no edit history sorted before one that has it');
        }
        sortSel.value = 'default';
        sortSel.dispatchEvent(new Event('change'));
        await sleep(200);
      }
    }
    // 423 setup: 2 disposable tags for the merge check below, added to a single
    // prompt (not the fixture's shared "Draft"/"Work") so later assertions that
    // depend on those two staying exactly as the fixture made them still hold.
    {
      await click('library-select-btn');
      document.querySelector('#library-list .lib-item[data-id="p1"]')?.click();
      await sleep(150);
      const setupAdd = document.getElementById('lib-batch-tag-add');
      if (!setupAdd) {
        out.errors.push('423: the batch tag add button is missing for merge-test setup');
      } else {
        setupAdd.click();
        await sleep(150);
        document.getElementById('confirm-input').value = 'MergeSrcTmp';
        document.getElementById('confirm-ok').click();
        await sleep(150);
        setupAdd.click();
        await sleep(150);
        document.getElementById('confirm-input').value = 'MergeDstTmp';
        document.getElementById('confirm-ok').click();
        await sleep(150);
      }
      document.getElementById('lib-batch-none').click();
      await click('library-select-btn'); // back out of select mode
    }
    // Tags: the filter row lists them, a chip narrows the list, the manager opens.
    const chips = [...document.querySelectorAll('#library-tagbar .tag-filter')];
    if (chips.length < 2) {
      out.errors.push('the library tag filter row is empty');
    } else {
      const before = document.querySelectorAll('#library-list .lib-item').length;
      chips[0].click();
      await sleep(250);
      const after = document.querySelectorAll('#library-list .lib-item').length;
      if (after >= before) out.errors.push('a tag chip did not narrow the library list');
      const active = document.querySelector('#library-tagbar .tag-filter.active');
      if (!active) out.errors.push('the selected tag chip is not marked active');
      else { active.click(); await sleep(200); }
      const mgr = [...document.querySelectorAll('#library-tagbar .tag-action')].pop();
      if (mgr) { mgr.click(); await sleep(250); }
      const tm = document.getElementById('tag-modal');
      if (tm.classList.contains('hidden')) {
        out.errors.push('the tag manager did not open');
      } else if (!document.querySelectorAll('#tag-modal-list .tag-manage-row').length) {
        out.errors.push('the tag manager listed no tags');
      } else {
        // P27: "Colorize" gives every UNcolored tag a palette color, in order -
        // an already-colored tag ("Work", fixture blue) must stay untouched.
        const rowFor = (name) => [...document.querySelectorAll('#tag-modal-list .tag-manage-row')]
          .find(r => (r.querySelector('.tag-manage-name') || {}).textContent === name);
        const workDot = rowFor('Work')?.querySelector('.tag-dot');
        const workBefore = workDot?.style.getPropertyValue('--chip-color');
        document.getElementById('tag-colorize')?.click();
        await sleep(300);
        const workAfter = rowFor('Work')?.querySelector('.tag-dot')?.style.getPropertyValue('--chip-color');
        if (workAfter !== workBefore) out.errors.push('colorize touched a tag that already had a color');
        const draftDot = rowFor('Draft')?.querySelector('.tag-dot');
        if (!draftDot || !draftDot.classList.contains('tag-colored')) {
          out.errors.push('colorize did not give the uncolored tag a color');
        }
        // 423: merge "MergeSrcTmp" into "MergeDstTmp" (the setup above) via the
        // row's Merge button (order: Rename, Merge, Delete) -- disposable tags,
        // never the fixture's own "Draft"/"Work" that other checks rely on.
        const srcRow = rowFor('MergeSrcTmp');
        const mergeBtn = srcRow?.querySelectorAll('.ghost-btn')[1];
        if (!srcRow || !mergeBtn) {
          out.errors.push('423: the tag manager has no Merge button for "MergeSrcTmp"');
        } else {
          const runMerge = async () => {
            rowFor('MergeSrcTmp').querySelectorAll('.ghost-btn')[1].click();
            await sleep(150);
            document.getElementById('confirm-input').value = 'MergeDstTmp';
            document.getElementById('confirm-ok').click(); // target picker: the only dialog
            await sleep(250);
          };
          await runMerge();
          if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
            out.errors.push('676: a second confirmation dialog opened after picking the merge target');
          }
          if (rowFor('MergeSrcTmp')) out.errors.push('423: "MergeSrcTmp" is still listed after merging it into "MergeDstTmp"');
          // The merge ran at once and its toast carries an Undo that brings the
          // source tag back; then merge again so the later checks start as before.
          const mergeUndoBtn = document.getElementById('toast').querySelector('.toast-btn');
          if (!mergeUndoBtn) {
            out.errors.push('676: merging two tags did not offer an undo toast');
          } else {
            mergeUndoBtn.click();
            await sleep(400);
            if (!rowFor('MergeSrcTmp')) out.errors.push('676: undoing a tag merge did not bring the source tag back');
            if (!rowFor('MergeDstTmp')) out.errors.push('676: undoing a tag merge lost the target tag');
            await runMerge();
            if (rowFor('MergeSrcTmp')) out.errors.push('676: merging again after an undo left the source tag listed');
          }
          const stillShown = [...document.querySelectorAll('#library-list .tag-mark')].some((c) => c.textContent === 'MergeSrcTmp');
          if (stillShown) out.errors.push('423: a library row still shows the merged-away tag');
        }
        // 434: deleting a tag offers an Undo button, and clicking it re-adds the
        // tag text to every prompt that had it. Uses the same disposable
        // "MergeDstTmp" tag the merge check above just produced.
        const dstRow = rowFor('MergeDstTmp');
        const delBtn = dstRow?.querySelectorAll('.ghost-btn')[2];
        if (!dstRow || !delBtn) {
          out.errors.push('434: the tag manager has no Delete button for "MergeDstTmp"');
        } else {
          delBtn.click();
          await sleep(150);
          document.getElementById('confirm-ok').click();
          await sleep(250);
          if (rowFor('MergeDstTmp')) out.errors.push('434: "MergeDstTmp" is still listed after deleting it');
          const tagUndoToast = document.getElementById('toast');
          const tagUndoBtn = tagUndoToast.querySelector('.toast-btn');
          if (!tagUndoToast.classList.contains('actionable') || !tagUndoBtn) {
            out.errors.push('434: deleting a tag did not offer an undo toast');
          } else {
            tagUndoBtn.click();
            await sleep(300);
            if (!rowFor('MergeDstTmp')) out.errors.push('434: undoing a deleted tag did not bring it back');
            const chipBack = [...document.querySelectorAll('#library-list .tag-mark')].some((c) => c.textContent === 'MergeDstTmp');
            if (!chipBack) out.errors.push('434: undoing a deleted tag did not restore it on the prompt row');
            if (tagUndoToast.classList.contains('show')) {
              out.errors.push('434: the tag-undo toast is still showing after being clicked');
            }
            // Delete it again for real (no undo) so the tag list ends this
            // check the way it started, minus the disposable merge source.
            const dstRow2 = rowFor('MergeDstTmp');
            const delBtn2 = dstRow2?.querySelectorAll('.ghost-btn')[2];
            if (delBtn2) {
              delBtn2.click();
              await sleep(150);
              document.getElementById('confirm-ok').click();
              await sleep(250);
            }
          }
        }
      }
      await click('tag-modal-close');
    }
"""
