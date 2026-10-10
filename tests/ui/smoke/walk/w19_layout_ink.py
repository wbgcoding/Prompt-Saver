SECTION = """\
    // 862: the tile menu opens fully inside the window at the smallest size, with its last entry
    // reachable by a real pointer (a menu that fits the window is moved up, not squeezed into a scroller).
    {
      const tile = document.querySelector('#grid .tile[data-id]');
      if (!tile) {
        out.errors.push('862: no tile in the grid to open a menu on');
      } else {
        const tr = tile.getBoundingClientRect();
        tile.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: tr.left + 30, clientY: tr.top + 30 }));
        await sleep(250);
        const menu = document.getElementById('ctx');
        const entries = [...menu.querySelectorAll('button')].filter((b) => !b.classList.contains('hidden') && b.getClientRects().length);
        const last = entries[entries.length - 1];
        if (menu.classList.contains('hidden') || !last) {
          out.errors.push('862: the tile menu did not open');
        } else {
          const lr = last.getBoundingClientRect();
          const hit = document.elementFromPoint(lr.left + lr.width / 2, lr.top + lr.height / 2);
          if (lr.bottom > innerHeight || lr.top < 0) {
            out.errors.push('862: the last tile menu entry lies outside the window (' + Math.round(lr.top) + '..' + Math.round(lr.bottom) + ' of ' + innerHeight + ')');
          } else if (!hit || !(hit === last || last.contains(hit))) {
            out.errors.push('862: the last tile menu entry is covered by ' + (hit && (hit.id || hit.className || hit.tagName)));
          }
        }
        document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
        await sleep(100);
      }
    }
    // 864: a light tile colour gets dark text, a darker one keeps white text with its shadow
    // (the default ink threshold, read from the rendered tile name).
    {
      const nameStyle = (rgb) => {
        for (const tile of document.querySelectorAll('#grid .tile.tinted')) {
          if (tile.style.background.replace(/\\s/g, '') === rgb) {
            const n = tile.querySelector('.tile-name');
            return n ? getComputedStyle(n) : null;
          }
        }
        return null;
      };
      const amber = nameStyle('rgb(245,158,11)');
      const blue = nameStyle('rgb(59,130,246)');
      if (!amber || !blue) {
        out.errors.push('864: the fixture has no amber or blue tile in the grid');
      } else {
        if (amber.color !== 'rgb(0, 0, 0)') out.errors.push('864: the name on an amber tile is ' + amber.color + ', expected black');
        if (blue.color !== 'rgb(255, 255, 255)') out.errors.push('864: the name on a blue tile is ' + blue.color + ', expected white');
        if (blue.textShadow === 'none') out.errors.push('864: white text on a blue tile lost its shadow');
      }
    }
    // 867: on the smallest window every long dialog scrolls with a scrollbar the user can see:
    // its scrolling element has overflow-y auto or scroll and a scrollbar of real width (the
    // expert menu switch that hides scrollbars is off).
    if (innerHeight <= 500 && !document.querySelector('.flag-hideScrollbars')) {
      const scrollers = () => {
        const found = [];
        for (const el of document.querySelectorAll('.overlay:not(.hidden) *')) {
          const oy = getComputedStyle(el).overflowY;
          if (el.scrollHeight > el.clientHeight + 2 && el.clientHeight > 40 && (oy === 'auto' || oy === 'scroll' || oy === 'hidden')) {
            found.push({ el, oy, bar: el.offsetWidth - el.clientWidth });
          }
        }
        return found;
      };
      const checkDialog = (name) => {
        const list = scrollers().filter((s) => s.oy !== 'hidden');
        if (!list.length) {
          out.errors.push('867: ' + name + ' has no scrolling element at ' + innerWidth + 'x' + innerHeight);
          return;
        }
        for (const s of list) {
          if (s.bar <= 0) out.errors.push('867: ' + name + ' scrolls in ' + sig(s.el) + ' without a visible scrollbar');
        }
      };
      const step = async (ms) => { await sleep(ms || 300); };
      document.getElementById('gear').click(); await step();
      document.getElementById('help-open').click(); await step(400);
      checkDialog('the help dialog');
      document.getElementById('help-close').click(); await step();
      document.getElementById('settings-backup-btn').click(); await step(500);
      checkDialog('the backup dialog');
      document.getElementById('backup-close').click(); await step();
      document.getElementById('settings-stats-btn').click(); await step(500);
      checkDialog('the statistics dialog');
      document.getElementById('stats-close').click(); await step();
      document.getElementById('expert-open').click(); await step(400);
      checkDialog('the expert menu');
      document.getElementById('settings-close').click(); await step();
      document.getElementById('library-btn').click(); await step(400);
      checkDialog('the library');
      document.getElementById('library-tab-history').click(); await step(400);
      checkDialog('the copy history');
      document.getElementById('library-close').click(); await step();
    }
"""
