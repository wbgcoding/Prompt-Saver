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
"""
