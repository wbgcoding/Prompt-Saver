SECTION = """\
    // 511: the filter row (search, only changed) is the first child of the expert page;
    // the reset button is the single button of the last group of the System tab (red, full
    // width, hint under it), found by search, and gone from the old footer.
    {
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const expertBox = document.getElementById('settings-expert');
      const filterRow551 = expertBox.querySelector('.expert-filter-row');
      if (!filterRow551 || expertBox.firstElementChild !== filterRow551) {
        out.errors.push('511: the filter row is not the first child of the expert page');
      } else if (filterRow551.querySelector('[data-action="expert-reset"]')) {
        out.errors.push('511: the reset button is still in the filter row');
      }
      if (document.getElementById('expert-end') || document.getElementById('expert-reset')) {
        out.errors.push('511: the old reset footer is still in the page');
      }
      const flagsBox551 = document.getElementById('expert-flags');
      const resetSel551 = '#expert-flags [data-action="expert-reset"]';
      let resetTabs551 = 0;
      for (const tab of document.querySelectorAll('#expert-tabs-bar .expert-tab')) {
        tab.click();
        await sleep(100);
        const rb = document.querySelector(resetSel551);
        if (!rb) continue;
        resetTabs551++;
        const grp = rb.closest('.expert-group');
        if (!grp || grp !== [...flagsBox551.querySelectorAll('.expert-group')].pop()) out.errors.push('511: the reset group is not the last group of its tab');
        if (!rb.classList.contains('danger-btn')) out.errors.push('511: the reset button is not danger-styled');
        if (!grp.querySelector('.hint') || !grp.querySelector('.hint').textContent) out.errors.push('511: the reset button has no hint line');
        if (rb.getBoundingClientRect().width < grp.getBoundingClientRect().width * 0.8) out.errors.push('511: the reset button is not full width');
      }
      if (resetTabs551 !== 1) out.errors.push('511: the reset button is on ' + resetTabs551 + ' tabs, expected exactly the System tab');
      if (expertBox.querySelector('.expert-top-row, .expert-presets')) {
        out.errors.push('511: the removed top row or quick profiles are still in the expert page');
      }
      const expertSearch551 = document.getElementById('expert-search');
      expertSearch551.value = 'reset expert';
      expertSearch551.dispatchEvent(new Event('input'));
      await sleep(250);
      if (!document.querySelector(resetSel551)) out.errors.push('511: the expert search does not find the reset button');
      expertSearch551.value = '';
      expertSearch551.dispatchEvent(new Event('input'));
      await sleep(250);
      const firstTab0 = document.querySelector('#expert-tabs-bar .expert-tab');
      if (firstTab0) { firstTab0.click(); await sleep(100); }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // 513: the drawing-color row -- a real swatch (not a bare default button),
    // sized and bordered like the app's other color triggers, with the hex
    // value shown as text, at about the same row height as a dropdown row.
    {
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const search = document.getElementById('expert-search');
      search.value = 'Screenshot editor';
      search.dispatchEvent(new Event('input'));
      await sleep(250);
      const colorRow513 = document.querySelector('#expert-flags [data-setting-row="snipDefaultColor"]');
      const toolRow513 = document.querySelector('#expert-flags [data-setting-row="snipEditDefaultTool"]');
      if (!colorRow513) {
        out.errors.push('513: the default drawing color row is missing (search did not surface it)');
      } else {
        const swatch = colorRow513.querySelector('.color-swatch');
        if (!swatch) {
          out.errors.push('513: the color row has no .color-swatch element');
        } else {
          const cs = getComputedStyle(swatch);
          if (Math.round(parseFloat(cs.width)) !== 28) {
            out.errors.push('513: the color swatch is ' + cs.width + ' wide, expected 28px');
          }
          if (parseFloat(cs.borderTopWidth) <= 0) {
            out.errors.push('513: the color swatch has no border');
          }
          if (!colorRow513.querySelector('.value-out')) {
            out.errors.push('513: the color row shows no hex value text beside the swatch');
          }
        }
        if (toolRow513) {
          const h1 = Math.round(colorRow513.getBoundingClientRect().height);
          const h2 = Math.round(toolRow513.getBoundingClientRect().height);
          if (Math.abs(h1 - h2) > 4) {
            out.errors.push('513: the color row is ' + h1 + 'px tall vs a dropdown row at ' + h2 + 'px, over the 4px tolerance');
          }
        }
      }
      search.value = '';
      search.dispatchEvent(new Event('input'));
      await sleep(150);
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // The expert search box is shared by the checks below.
    {
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const search514 = document.getElementById('expert-search');
      const findFlagRow = (label) => [...document.querySelectorAll('#expert-flags .switch-field')]
        .find((r) => ((r.querySelector('span') || {}).textContent || '').includes(label));
      // 512: the dark halo behind white text is a live setting on the blue
      // sample tile (p0, always white text under the shipped threshold) -
      // default on at the current look, switch off clears it, a lower
      // strength lightens it.
      const inkShadowOf512 = () => {
        const t = [...document.querySelectorAll('#grid .tile')].find((e) => (e.dataset.id || '') === 'p0');
        return t ? t.style.getPropertyValue('--ink-shadow') : null;
      };
      const todayHalo512 = '0 0 2px rgba(0, 0, 0, 0.75), 0 1px 3px rgba(0, 0, 0, 0.62)';
      const startHalo512 = inkShadowOf512();
      if (startHalo512 === null) {
        out.errors.push('512: could not find the blue sample tile (p0) to check the ink halo');
      } else if (startHalo512 !== todayHalo512) {
        out.errors.push('512: the default ink halo does not match the shipped look, got ' + startHalo512);
      } else {
        search514.value = 'Dark shadow behind white text';
        search514.dispatchEvent(new Event('input'));
        await sleep(200);
        const findHaloSw512 = () => {
          const row = findFlagRow('Dark shadow behind white text');
          return row && row.querySelector('input.switch');
        };
        if (!findHaloSw512()) {
          out.errors.push('512: could not find the ink halo switch');
        } else {
          // inkHalo gates the strength row, so toggling it re-renders the
          // search results -- re-find the switch after every click instead
          // of holding onto a row the re-render just replaced.
          findHaloSw512().click();
          await sleep(200);
          if (inkShadowOf512() !== 'none') {
            out.errors.push('512: switching the ink halo off did not clear the tile shadow, got ' + inkShadowOf512());
          }
          const haloSwBack = findHaloSw512();
          if (!haloSwBack) {
            out.errors.push('512: the ink halo switch disappeared after switching it off');
          } else {
            haloSwBack.click();
            await sleep(200);
            if (inkShadowOf512() !== todayHalo512) {
              out.errors.push('512: switching the ink halo back on did not restore the shipped look, got ' + inkShadowOf512());
            }
          }
          search514.value = 'Shadow strength';
          search514.dispatchEvent(new Event('input'));
          await sleep(200);
          const strengthRow512 = document.querySelector('#expert-flags [data-setting-row="inkHaloStrength"]');
          const strengthSlider512 = strengthRow512 && strengthRow512.querySelector('input[type=range]');
          if (!strengthSlider512) {
            out.errors.push('512: could not find the shadow strength slider');
          } else {
            strengthSlider512.value = '40';
            strengthSlider512.dispatchEvent(new Event('input'));
            strengthSlider512.dispatchEvent(new Event('change'));
            await sleep(200);
            const lowerHalo512 = '0 0 2px rgba(0, 0, 0, 0.4), 0 1px 3px rgba(0, 0, 0, 0.33)';
            if (inkShadowOf512() !== lowerHalo512) {
              out.errors.push('512: a lower shadow strength (40) did not lighten the tile halo, got ' + inkShadowOf512());
            }
            // Put the strength back to the shipped default for whatever runs after.
            strengthSlider512.value = '75';
            strengthSlider512.dispatchEvent(new Event('input'));
            strengthSlider512.dispatchEvent(new Event('change'));
            await sleep(150);
          }
        }
      }
      search514.value = '';
      search514.dispatchEvent(new Event('input'));
      await sleep(150);
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // 836: rendering the expert menu over and over must not leave one live resize
    // observer per render behind (the tab bar is rebuilt each time).
    {
      const NativeRO = window.ResizeObserver;
      const liveRO = new Set();
      window.ResizeObserver = class extends NativeRO {
        observe(el, o) { liveRO.add(this); return super.observe(el, o); }
        unobserve(el) { return super.unobserve(el); }
        disconnect() { liveRO.delete(this); return super.disconnect(); }
      };
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      for (let i = 0; i < 20; i++) {
        const tabsNow = document.querySelectorAll('#expert-tabs-bar .expert-tab');
        if (!tabsNow.length) break;
        tabsNow[i % tabsNow.length].click();
        await sleep(20);
      }
      await sleep(100);
      if (liveRO.size > 1) {
        out.errors.push('836: ' + liveRO.size + ' resize observers are still live after 20 expert menu renders');
      }
      window.ResizeObserver = NativeRO;
      const firstTab836 = document.querySelector('#expert-tabs-bar .expert-tab');
      if (firstTab836) { firstTab836.click(); await sleep(100); }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
"""
