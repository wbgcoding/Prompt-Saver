SECTION = """\
    // 511/551: the caution hint is the first child of the expert page, red and
    // centered in every mode; the quick-profiles row follows it.
    {
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const expertBox = document.getElementById('settings-expert');
      const hint = document.getElementById('expert-hint');
      const expertPage = document.getElementById('settings-expert');
      const hintOk = (label) => {
        if (!hint || expertPage.firstElementChild !== hint) {
          out.errors.push('551: the caution hint is not the first child of the expert page (' + label + ')');
          return;
        }
        const dangerProbe = document.createElement('span');
        dangerProbe.style.color = 'var(--danger)';
        document.body.appendChild(dangerProbe);
        const dangerColor = getComputedStyle(dangerProbe).color;
        dangerProbe.remove();
        if (getComputedStyle(hint).color !== dangerColor) {
          out.errors.push('551: the caution hint is not in the danger colour (' + label + '): ' + getComputedStyle(hint).color + ' vs ' + dangerColor);
        }
        const hr = hint.getBoundingClientRect();
        const pr = expertPage.getBoundingClientRect();
        const content = [...hint.children].map((c) => c.getBoundingClientRect());
        const cx = (Math.min(...content.map((r) => r.left)) + Math.max(...content.map((r) => r.right))) / 2;
        const px = pr.left + pr.width / 2;
        if (hr.width < 1 || Math.abs(cx - px) > 2) {
          out.errors.push('551: the caution hint is not centered (' + label + '): content x ' + cx + ' vs page x ' + px);
        }
        if (hint.matches('button, a, input, select, [tabindex]') || hint.hasAttribute('tabindex')) {
          out.errors.push('551: the caution hint is focusable, it should be plain text');
        }
      };
      hintOk('browse');
      const topRow551 = expertBox.querySelector('.expert-top-row');
      if (!topRow551 || hint.nextElementSibling !== topRow551) {
        out.errors.push('511: the expert menu top row (presets plus reset) does not follow the hint');
      } else if (!topRow551.querySelector('.expert-presets') || topRow551.querySelector('.expert-presets') !== topRow551.firstElementChild) {
        out.errors.push('511: the quick-profiles group is not the first thing in the top row');
      }
      const expertSearch551 = document.getElementById('expert-search');
      expertSearch551.value = 'shadow';
      expertSearch551.dispatchEvent(new Event('input'));
      await sleep(250);
      hintOk('while searching');
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
    // 514: the remote-access port is a typed number field, not a slider --
    // an out-of-range value shows the inline hint and commits nothing, a
    // valid one commits and clears the hint.
    {
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const search514 = document.getElementById('expert-search');
      const findFlagRow = (label) => [...document.querySelectorAll('#expert-flags .switch-field')]
        .find((r) => ((r.querySelector('span') || {}).textContent || '').includes(label));
      search514.value = 'Remote access (local API)';
      search514.dispatchEvent(new Event('input'));
      await sleep(250);
      const remoteFlagRow = findFlagRow('Remote access (local API)');
      const remoteSw = remoteFlagRow && remoteFlagRow.querySelector('input.switch');
      if (!remoteSw) {
        out.errors.push('514: could not find the remote-access switch to enable it for the port test');
      } else {
        if (!remoteSw.checked) { remoteSw.click(); await sleep(250); }
        search514.value = 'Remote access port';
        search514.dispatchEvent(new Event('input'));
        await sleep(250);
        const portRow = document.querySelector('#expert-flags [data-setting-row="remotePort"]');
        if (!portRow) {
          out.errors.push('514: the remote-access port row is missing');
        } else {
          if (portRow.querySelector('input[type=range]')) {
            out.errors.push('514: the port row still shows a slider');
          }
          const input514 = portRow.querySelector('.expert-num');
          const hint514 = portRow.querySelector('.expert-row-hint');
          if (!input514 || !hint514) {
            out.errors.push('514: the port row has no text input or hint element');
          } else {
            const portCommits = () => (window.__SMOKE_ARGS__ || [])
              .filter(([cmd, args]) => cmd === 'set_ui_value' && args && args.key === 'remotePort')
              .map(([, args]) => args.value);
            const before514 = portCommits().length;
            input514.value = '80';
            input514.dispatchEvent(new Event('change'));
            await sleep(150);
            if (portCommits().length > before514) {
              out.errors.push('514: an out-of-range port (80) was committed');
            }
            if (hint514.classList.contains('hidden')) {
              out.errors.push('514: no invalid-port hint shown for an out-of-range value');
            }
            input514.value = '51900';
            input514.dispatchEvent(new Event('change'));
            await sleep(150);
            const after514 = portCommits();
            if (after514[after514.length - 1] !== 51900) {
              out.errors.push('514: a valid port (51900) was not committed, got ' + after514[after514.length - 1]);
            }
            if (!hint514.classList.contains('hidden')) {
              out.errors.push('514: the invalid-port hint stayed visible after a valid value');
            }
          }
        }
        // 515: the rate-limit slider has one extra stop past its real max,
        // showing the unlimited label and storing 0.
        search514.value = 'Remote access requests per minute';
        search514.dispatchEvent(new Event('input'));
        await sleep(250);
        const rateRow = document.querySelector('#expert-flags [data-setting-row="remoteRateLimit"]');
        if (!rateRow) {
          out.errors.push('515: the remote rate-limit row is missing');
        } else {
          const rateSlider = rateRow.querySelector('input[type=range]');
          const rateOut = rateRow.querySelector('.value-out');
          if (!rateSlider || !rateOut) {
            out.errors.push('515: the rate-limit row has no slider or value readout');
          } else {
            const rateCommits = () => (window.__SMOKE_ARGS__ || [])
              .filter(([cmd, args]) => cmd === 'set_ui_value' && args && args.key === 'remoteRateLimit')
              .map(([, args]) => args.value);
            const belowMax = String(Number(rateSlider.max) - Number(rateSlider.step)) + '/min';
            rateSlider.value = String(Number(rateSlider.max) - Number(rateSlider.step));
            rateSlider.dispatchEvent(new Event('input'));
            const readoutBelowMax = rateOut.textContent;
            if (readoutBelowMax !== belowMax) {
              out.errors.push('515: the slider one stop below max reads "' + readoutBelowMax + '" instead of "' + belowMax + '"');
            }
            rateSlider.value = rateSlider.max;
            rateSlider.dispatchEvent(new Event('input'));
            if (rateOut.textContent === (rateSlider.max + '/min') || rateOut.textContent === readoutBelowMax) {
              out.errors.push('515: the top slider stop does not show a distinct unlimited label: ' + rateOut.textContent);
            }
            rateSlider.dispatchEvent(new Event('change'));
            await sleep(150);
            const rc = rateCommits();
            if (rc[rc.length - 1] !== 0) {
              out.errors.push('515: the unlimited slider stop did not commit 0, got ' + rc[rc.length - 1]);
            }
          }
        }
        // Put remoteApi back off so the rest of the run sees the shipped default.
        search514.value = 'Remote access (local API)';
        search514.dispatchEvent(new Event('input'));
        await sleep(200);
        const remoteFlagRowBack = findFlagRow('Remote access (local API)');
        const remoteSwBack = remoteFlagRowBack && remoteFlagRowBack.querySelector('input.switch');
        if (remoteSwBack && remoteSwBack.checked) { remoteSwBack.click(); await sleep(200); }
      }
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
"""
