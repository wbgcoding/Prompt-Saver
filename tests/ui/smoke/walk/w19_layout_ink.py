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
"""
