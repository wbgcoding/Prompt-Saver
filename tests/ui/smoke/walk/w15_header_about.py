SECTION = """\
    // 724: the settings page is split into titled sections, in this order, with the
    // English titles (the last one holds the two danger buttons).
    {
      if (document.getElementById('settings').classList.contains('hidden')) await click('gear');
      if (document.getElementById('settings-main').classList.contains('hidden')) {
        await click('expert-back');
        await sleep(50);
      }
      const titles724 = [...document.querySelectorAll('#settings-main .settings-sec-title')]
        .filter((el) => el.getBoundingClientRect().height > 0).map((el) => el.textContent.trim());
      const want724 = ['Appearance', 'Behavior', 'Capture', 'Data', 'System', 'Reset and delete'];
      if (titles724.join('|') !== want724.join('|')) {
        out.errors.push('724: the visible settings section titles are "' + titles724.join('|') + '", expected "' + want724.join('|') + '"');
      }
    }
    // 729: help is the question-mark button in the settings head, left of the close
    // button, visible on the settings page and on the expert page; the expert menu
    // button sits alone in its row at normal button size.
    {
      if (document.getElementById('settings').classList.contains('hidden')) await click('gear');
      if (document.getElementById('settings-main').classList.contains('hidden')) {
        await click('expert-back');
        await sleep(50);
      }
      const help729 = document.getElementById('help-open');
      const head729 = document.querySelector('#settings .modal-head');
      const close729 = document.getElementById('settings-close');
      if (!help729 || !head729 || !head729.contains(help729)) {
        out.errors.push('729: help-open is not inside the settings head');
      } else {
        const hr = help729.getBoundingClientRect();
        if (!hr.width || !hr.height) out.errors.push('729: help-open is not visible on the settings page');
        if (!(hr.right <= close729.getBoundingClientRect().left + 1)) out.errors.push('729: help-open is not left of the close button');
        if (!help729.getAttribute('aria-label') || !help729.title) out.errors.push('729: help-open has no accessible name or tooltip');
        const eo729 = document.getElementById('expert-open');
        const row729 = eo729.parentElement;
        if (row729.querySelectorAll('button').length !== 1) out.errors.push('729: the expert menu button shares its row');
        if (eo729.getBoundingClientRect().width > 260) out.errors.push('729: the expert menu button is wider than a normal button: ' + Math.round(eo729.getBoundingClientRect().width));
        await click('expert-open');
        const hr2 = help729.getBoundingClientRect();
        if (!hr2.width || !hr2.height) out.errors.push('729: help-open is not visible on the expert page');
        await click('expert-back');
      }
    }
    // 730: the screenshot switch carries the short label and keeps it on one line.
    {
      if (document.getElementById('settings').classList.contains('hidden')) await click('gear');
      if (document.getElementById('settings-main').classList.contains('hidden')) {
        await click('expert-back');
        await sleep(50);
      }
      const span730 = document.querySelector('#opt-screenshot-folder').closest('label').querySelector('span');
      if (span730.textContent.trim() !== 'Always save screenshots') out.errors.push('730: the screenshot switch reads "' + span730.textContent.trim() + '"');
      const range730 = document.createRange();
      range730.selectNodeContents(span730);
      if (range730.getClientRects().length !== 1) out.errors.push('730: the screenshot switch label wraps at ' + window.innerWidth + 'px');
    }
    // 96: the short "Latest X.Y.Z" status keeps the first update line no taller than
    // before, with the longest of the 11 translations (Romanian).
    // 727: the System section is one line: version status, About, the check button, the
    // auto-update switch and the interval, in English and German at every size.
    {
      if (document.getElementById('settings').classList.contains('hidden')) await click('gear');
      if (document.getElementById('settings-main').classList.contains('hidden')) {
        await click('expert-back');
        await sleep(50);
      }
      const uv = document.getElementById('update-version');
      const aboutBtn = document.getElementById('about-btn');
      const ub = document.getElementById('update-btn');
      const ui727 = document.getElementById('opt-update-interval');
      const line1 = uv && uv.closest('.update-line');
      if (line1 && aboutBtn && ub && ui727) {
        const prevText = uv.textContent;
        uv.textContent = 'Cea mai recenta 2.7.0';
        const lineH = line1.getBoundingClientRect().height;
        uv.textContent = 'Neueste Version bereits installiert';
        const lineHOld = line1.getBoundingClientRect().height;
        if (lineH > lineHOld + 1) {
          out.errors.push('the short status is taller than the old long sentence was: ' + Math.round(lineH) + ' vs ' + Math.round(lineHOld));
        }
        uv.textContent = 'Version 2.7.0';
        const kids727 = [...line1.children];
        if (kids727.length !== 5 || !line1.contains(ub) || !line1.contains(ui727)) {
          out.errors.push('727: the System line does not hold all five controls (' + kids727.length + ')');
        }
        const oneRow727 = (label) => {
          // Below the minimum window size the line may wrap; it never cuts a text.
          const mids = kids727.map((k) => { const r = k.getBoundingClientRect(); return r.top + r.height / 2; });
          if (window.innerWidth >= 680 && Math.max(...mids) - Math.min(...mids) > 4) out.errors.push('727: the System line wraps in ' + label + ' at ' + window.innerWidth + 'px');
          const cut = [uv, aboutBtn, ub].filter((el) => el.scrollWidth > el.clientWidth + 1);
          if (cut.length) out.errors.push('727: ' + cut.map((el) => el.id).join(', ') + ' cut in ' + label + ' at ' + window.innerWidth + 'px');
        };
        oneRow727('the page language');
        const prev727 = [aboutBtn.textContent, ub.textContent, line1.querySelector('.switch-field > span').textContent];
        aboutBtn.textContent = 'Über';
        ub.textContent = 'Nach Updates suchen';
        line1.querySelector('.switch-field > span').textContent = 'Auto-Update';
        uv.textContent = 'Update v2.10.0';
        oneRow727('German');
        [aboutBtn.textContent, ub.textContent, line1.querySelector('.switch-field > span').textContent] = prev727;
        const rect = (el) => el.getBoundingClientRect();
        if (window.innerWidth >= 680 && !(rect(aboutBtn).left > rect(uv).left && rect(ub).left > rect(aboutBtn).left && rect(ui727).left > rect(ub).left)) {
          out.errors.push('727: the System line is not in the order version, About, check, switch, interval');
        }
        if (!ui727.getAttribute('aria-label')) out.errors.push('727: the interval select has no accessible name');
        if (document.querySelector('.update-row, .update-actions')) out.errors.push('727: the old update grid markup is still there');
        uv.textContent = prevText;
      } else {
        out.errors.push('update-line, about-btn, update-btn or opt-update-interval missing from the settings panel');
      }
    }
    // 108: reset-settings and delete-data carry their full labels. In English
    // they stay on one line; a long translation (German "Einstellungen
    // zurücksetzen", Dutch "Alle gegevens verwijderen") may wrap to two lines.
    // The flex-row default stretch means a single wrapped button grows the WHOLE
    // row, so compare against the row's own single-line reference height rather
    // than siblings. 725: both sit in the danger section, red at rest.
    {
      if (document.getElementById('settings-main').classList.contains('hidden')) {
        document.getElementById('expert-open').click();
        await sleep(50);
      }
      const reset = document.getElementById('reset-settings');
      const del = document.getElementById('delete-data');
      const dangerSec = document.querySelector('.settings-sec-danger');
      if (!reset || !del || !dangerSec) {
        out.errors.push('reset-settings, delete-data or the danger section missing from the settings panel');
      } else if (!dangerSec.contains(reset) || !dangerSec.contains(del)) {
        out.errors.push('725: reset-settings and delete-data are not inside the danger section');
      } else if (reset.getBoundingClientRect().height < 10) {
        out.errors.push('the danger section is not visible at ' + window.innerWidth + 'px');
      } else {
        const prevReset = reset.textContent;
        const prevDel = del.textContent;
        const rowH = () => Math.round(reset.getBoundingClientRect().height);
        reset.textContent = 'Reset';
        del.textContent = 'Delete';
        const baseH = rowH();
        // The full English labels stay on one line.
        reset.textContent = 'Reset settings';
        del.textContent = 'Delete all data';
        if (rowH() > baseH + 1) {
          out.errors.push('the danger buttons wrap with the English labels at ' + window.innerWidth + 'px: ' + baseH + ' -> ' + rowH());
        }
        // Long translations may wrap to two lines, but both buttons grow
        // together, never beyond two lines, and no label spills sideways.
        reset.textContent = 'Einstellungen zurücksetzen';
        del.textContent = 'Alle gegevens verwijderen';
        const longH = rowH();
        const hs = [reset, del].map((b) => Math.round(b.getBoundingClientRect().height));
        const spill = [reset, del].some((b) => b.scrollWidth > b.clientWidth + 1);
        reset.textContent = prevReset;
        del.textContent = prevDel;
        if (longH > baseH * 2) {
          out.errors.push('the danger buttons grow past two lines with long labels at ' + window.innerWidth + 'px: ' + baseH + ' -> ' + longH);
        }
        if (new Set(hs).size > 1) {
          out.errors.push('the danger buttons differ in height with long labels at ' + window.innerWidth + 'px: ' + hs.join('/'));
        }
        if (spill) {
          out.errors.push('a danger button label overflows its button at ' + window.innerWidth + 'px');
        }
        // 725: red at rest, and the last button of the page.
        const probe725 = document.createElement('span');
        probe725.style.color = 'var(--danger)';
        document.body.appendChild(probe725);
        const danger725 = getComputedStyle(probe725).color;
        probe725.remove();
        if (getComputedStyle(del).color !== danger725) {
          out.errors.push('725: delete-data is not red at rest: ' + getComputedStyle(del).color + ' vs ' + danger725);
        }
        if (getComputedStyle(reset).color !== danger725) {
          out.errors.push('725: reset-settings is not red at rest: ' + getComputedStyle(reset).color + ' vs ' + danger725);
        }
        const btns725 = document.querySelectorAll('#settings-main button');
        if (btns725[btns725.length - 1] !== del) out.errors.push('725: delete-data is not the last button of the settings page');
        const titles725 = [...document.querySelectorAll('#settings-main .settings-sec-title')]
          .filter((el) => el.getBoundingClientRect().height > 0).map((el) => el.textContent.trim());
        if (titles725.length !== 6 || titles725[5] !== 'Reset and delete') {
          out.errors.push('725: the visible section titles are "' + titles725.join('|') + '", expected six ending in Reset and delete');
        }
      }
    }
    // 109: hide-bottom sits left of Save, and win-controls/Save share --winctl-w.
    // Also: hiding the bar toggle (barToggles off) must not leave a gap where it
    // used to sit. Only checked >= 700px:
    // below the header's own ~690px wrap breakpoint, header-actions becomes the
    // SOLE item on its wrapped line and `.header`'s justify-content:space-between
    // has no second item left to space against, so it falls back to packing left
    // instead of right -- a pre-existing quirk outside this point's scope, and
    // exactly why the spec only asks for 900px/1280px here.
    {
      if (!document.getElementById('settings').classList.contains('hidden')) {
        await click('settings-close');
        await sleep(50);
      }
      const hb = document.getElementById('hide-bottom');
      const winControls = document.querySelector('.win-controls');
      const saveBtn = document.getElementById('save');
      if (document.getElementById('hide-top')) out.errors.push('700: the top bar still has a hide button');
      // 704: each bar button names its own bar in the tooltip and the accessible name.
      for (const [id704, want704] of [['hide-bottom', 'Hide input bar'], ['show-bottom', 'Show input bar'], ['show-top', 'Show top bar']]) {
        const el704 = document.getElementById(id704);
        if (!el704) { out.errors.push('704: ' + id704 + ' is missing'); continue; }
        if (el704.getAttribute('aria-label') !== want704) out.errors.push('704: ' + id704 + ' has the accessible name "' + el704.getAttribute('aria-label') + '"');
        if (!(el704.title || '').startsWith(want704)) out.errors.push('704: ' + id704 + ' has the tooltip "' + el704.title + '"');
      }
      // 705: with an empty composer the disabled Save button is plain grey (the
      // surface colour), not a faded accent.
      if (saveBtn) {
        const colorOf705 = (v) => {
          const d = document.createElement('div');
          d.style.background = 'var(' + v + ')';
          document.body.appendChild(d);
          const c = getComputedStyle(d).backgroundColor;
          d.remove();
          return c;
        };
        if (!saveBtn.disabled) out.errors.push('705: the save button is enabled with an empty composer');
        else {
          const bg705 = getComputedStyle(saveBtn).backgroundColor;
          if (bg705 !== colorOf705('--surface-2')) out.errors.push('705: the disabled save button is ' + bg705 + ', not the surface colour');
          if (bg705 === colorOf705('--accent-strong')) out.errors.push('705: the disabled save button still wears the accent colour');
          if (getComputedStyle(saveBtn).opacity !== '1') out.errors.push('705: the disabled save button is faded instead of grey');
        }
      }
      if (!hb || !winControls || !saveBtn) {
        out.errors.push('hide-bottom, win-controls or save missing');
      } else if (window.innerWidth >= 700) {
        const hbR = hb.getBoundingClientRect();
        if (hbR.right > saveBtn.getBoundingClientRect().left + 1) {
          out.errors.push('hide-bottom is not left of save at ' + window.innerWidth + 'px');
        }
        const wcRightBefore = Math.round(winControls.getBoundingClientRect().right);
        const saveRightBefore = Math.round(saveBtn.getBoundingClientRect().right);
        document.body.classList.add('noflag-barToggles');
        const wcRightAfter = Math.round(winControls.getBoundingClientRect().right);
        const saveRightAfter = Math.round(saveBtn.getBoundingClientRect().right);
        document.body.classList.remove('noflag-barToggles');
        if (Math.abs(wcRightAfter - wcRightBefore) > 1 || Math.abs(saveRightAfter - saveRightBefore) > 1) {
          out.errors.push('hiding the bar-toggle buttons shifts win-controls/save at ' + window.innerWidth + 'px');
        }
      }
    }
    // 700 / 701: the top bar keeps five buttons with the chain button, the tag filter sits after
    // the view tabs, hiding the bar lives in the right-click menu,
    // and the header dividers group content from window controls.
    {
      if (!document.getElementById('settings').classList.contains('hidden')) {
        await click('settings-close');
        await sleep(50);
      }
      const shown = (el) => getComputedStyle(el).display !== 'none';
      const ids = () => [...document.querySelectorAll('.header-actions > button')].filter(shown).map((b) => b.id);
      const got = ids().join(',');
      // Search is hidden below 760px by design.
      const want = window.innerWidth <= 760 ? 'library-btn,chain-btn,pin-top,gear' : 'library-btn,global-search-btn,chain-btn,pin-top,gear';
      if (got !== want) out.errors.push('700: the visible top bar buttons are ' + got);
      const tagBtn = document.getElementById('grid-tag-btn');
      const viewsNav = document.getElementById('views');
      if (!tagBtn || !tagBtn.parentElement.classList.contains('brand') || tagBtn.previousElementSibling !== viewsNav) {
        out.errors.push('700: the tag filter button is not right after the view tabs inside the brand area');
      }
      const chainBtn = document.getElementById('chain-btn');
      if (!shown(chainBtn)) out.errors.push('700: the chain button is hidden while no chain runs');
      // 701: one divider between content and window buttons without the grid size fields.
      const sepInfo = () => [...document.querySelectorAll('.header-actions > .header-sep')].filter(shown).map((s) => Math.round(s.getBoundingClientRect().left));
      const xOf = (id) => document.getElementById(id).getBoundingClientRect().left;
      const hadQuick = document.body.classList.contains('flag-quickGrid');
      document.body.classList.remove('flag-quickGrid');
      const one = sepInfo();
      if (one.length !== 1) {
        out.errors.push('701: expected exactly one header divider without the grid size fields, got ' + one.length);
      } else if (!(one[0] > xOf(shown(document.getElementById('global-search-btn')) ? 'global-search-btn' : 'library-btn') && one[0] < xOf('pin-top'))) {
        out.errors.push('701: the header divider is not between search and pin: ' + one[0]);
      }
      document.body.classList.add('flag-quickGrid');
      if (sepInfo().length !== 2) out.errors.push('701: expected two header dividers with the grid size fields, got ' + sepInfo().length);
      if (!hadQuick) document.body.classList.remove('flag-quickGrid');
      // Hide top bar: right-click on the gear, then the menu row.
      const tbMenu = document.querySelector('.toolbar-menu');
      document.getElementById('gear').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 300, clientY: 40 }));
      await sleep(150);
      const hideRow = tbMenu && [...tbMenu.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Hide top bar');
      if (!tbMenu || tbMenu.classList.contains('hidden')) {
        out.errors.push('700: right-clicking the gear did not open the toolbar menu');
      } else if (!hideRow) {
        out.errors.push('700: the toolbar menu has no Hide top bar row');
      } else {
        hideRow.click();
        await sleep(150);
        if (!document.body.classList.contains('no-header')) out.errors.push('700: Hide top bar did not hide the top bar');
        if (!tbMenu.classList.contains('hidden')) out.errors.push('700: the toolbar menu stayed open after Hide top bar');
        await click('show-top');
      }
      document.querySelector('.brand').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 120, clientY: 40 }));
      await sleep(150);
      if (!tbMenu || tbMenu.classList.contains('hidden')) out.errors.push('700: right-clicking the top bar did not open the toolbar menu');
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      await sleep(100);
    }
    // 426: a double-click on any of the three bar buttons folds/unfolds both
    // bars together while barToggleBoth stays on; a single click is untouched
    // either way. The second click of a real double-click lands where the first
    // button was and no longer on the button, so dbl() clicks the button at its
    // centre and then the element found at the same point.
    {
      if (!document.getElementById('settings').classList.contains('hidden')) {
        await click('settings-close');
        await sleep(50);
      }
      const clickAt = (el, x, y) => el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, clientX: x, clientY: y, view: window }));
      const dbl = async (id, twice) => {
        const el = document.getElementById(id);
        const r = el.getBoundingClientRect();
        const x = r.left + r.width / 2, y = r.top + r.height / 2;
        clickAt(el, x, y);
        await sleep(60);
        if (twice) clickAt(document.elementFromPoint(x, y) || document.body, x, y);
        await sleep(600);
      };
      const bothHidden = () => document.body.classList.contains('no-header') && document.body.classList.contains('no-composer');
      const bothShown = () => !document.body.classList.contains('no-header') && !document.body.classList.contains('no-composer');
      await dbl('hide-bottom', true);
      if (!bothHidden()) out.errors.push('426: double-clicking hide-bottom did not fold both bars');
      await dbl('show-bottom', true);
      if (!bothShown()) out.errors.push('426: double-clicking show-bottom did not unfold both bars');
      // Switch barToggleBoth off through its real expert switch, same idiom as
      // the prompt-hotkey-paste option above.
      const expertRow426 = async (label) => {
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
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const bothRow = await expertRow426('folds both bars');
      if (!bothRow) {
        out.errors.push('426: the double-click-folds-both option is missing from the expert menu');
      } else {
        bothRow.querySelector('input.switch').click();
        await sleep(200);
        await click('expert-back');
        document.getElementById('settings').classList.add('hidden');
        await dbl('hide-bottom', false);
        if (document.body.classList.contains('no-header')) {
          out.errors.push('426: double-click still folded the other bar with the option off');
        }
        if (!document.body.classList.contains('no-composer')) {
          out.errors.push('426: the plain single click stopped working with the option off');
        }
        // Undo the single click, then put the option back for the rest of the run.
        await click('show-bottom');
        document.getElementById('settings').classList.remove('hidden');
        await click('expert-open');
        const backRow = await expertRow426('folds both bars');
        if (backRow) { backRow.querySelector('input.switch').click(); await sleep(200); }
        const firstTab426 = document.querySelector('#expert-tabs-bar .expert-tab');
        if (firstTab426) { firstTab426.click(); await sleep(200); }
        await click('expert-back');
        document.getElementById('settings').classList.add('hidden');
      }
    }
    // 822: the second click of a double-click is recognised by position and time, not
    // by a dblclick event. Both clicks within 500 ms and 8 px fold both bars; the same
    // two clicks 700 ms apart, or 40 px apart, leave the top bar alone.
    {
      const clickAt = (el, x, y) => el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, clientX: x, clientY: y, view: window }));
      const reset = async () => {
        if (document.body.classList.contains('no-composer')) clickAt(document.getElementById('show-bottom'), 300, 300);
        await sleep(100);
        if (document.body.classList.contains('no-header')) clickAt(document.getElementById('show-top'), 300, 300);
        await sleep(700);
      };
      const pair = async (gap, dx) => {
        await reset();
        const hb = document.getElementById('hide-bottom');
        const r = hb.getBoundingClientRect();
        const x = r.left + r.width / 2, y = r.top + r.height / 2;
        clickAt(hb, x, y);
        await sleep(gap);
        const second = document.elementFromPoint(x + dx, y) || document.body;
        let reached = 0;
        const seen = () => { reached++; };
        window.addEventListener('pointerdown', seen);
        second.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, clientX: x + dx, clientY: y }));
        window.removeEventListener('pointerdown', seen);
        clickAt(second, x + dx, y);
        await sleep(60);
        return [document.body.classList.contains('no-header'), document.body.classList.contains('no-composer'), reached];
      };
      const quick = await pair(60, 0);
      if (!(quick[0] && quick[1])) out.errors.push('822: two quick clicks on hide-bottom did not fold both bars: ' + quick);
      if (quick[2] !== 0) out.errors.push('822: the press of the second click still reached the page under the pointer');
      const pressed = await pair(60, 40);
      if (pressed[2] !== 1) out.errors.push('822: a press 40 px away was held back');
      const slow = await pair(700, 0);
      if (slow[0] || !slow[1]) out.errors.push('822: two clicks 700 ms apart changed more than the composer: ' + slow);
      const apart = await pair(60, 40);
      if (apart[0] || !apart[1]) out.errors.push('822: a second click 40 px away changed more than the composer: ' + apart);
      await reset();
      if (document.body.classList.contains('no-header') || document.body.classList.contains('no-composer')) {
        out.errors.push('822: the bars did not come back after the checks');
      }
    }
    // 427: a synthesised "copy" sound plays on every successful copy -- exactly
    // once, only while the switch is on, never at volume 0. A stand-in
    // AudioContext counts real oscillator starts; four different prompt ids
    // keep the per-prompt copy cooldown out of the way without touching it.
    {
      window.__BEEP_COUNT__ = 0;
      window.AudioContext = window.webkitAudioContext = function () {
        return {
          currentTime: 0,
          createOscillator: () => ({ frequency: { value: 0 }, connect() { return this; }, start() { window.__BEEP_COUNT__++; }, stop() {} }),
          createGain: () => ({ gain: { setValueAtTime() {}, linearRampToValueAtTime() {} }, connect() { return this; } }),
          destination: {},
        };
      };
      const expertRow427 = async (label) => {
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
      const valueRow427 = () => [...document.querySelectorAll('#expert-flags .value-field')]
        .find(r => ((r.querySelector('.value-head span') || {}).textContent || '').includes('Copy sound volume'));
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const soundRow = await expertRow427('Sound on copy');
      if (!soundRow) {
        out.errors.push('427: the copy-sound option is missing from the expert menu');
      } else {
        soundRow.querySelector('input.switch').click();
        await sleep(200);
        window.__FIRE__('prompt-hotkey', 'p0');
        await sleep(300);
        if (window.__BEEP_COUNT__ !== 1) out.errors.push('427: turning copySound on did not play a beep on copy: ' + window.__BEEP_COUNT__);
        window.__FIRE__('prompt-hotkey', 'p2');
        await sleep(300);
        if (window.__BEEP_COUNT__ !== 2) out.errors.push('427: a second copy did not play a second beep: ' + window.__BEEP_COUNT__);
        // Volume 0 must play nothing.
        const volRow = valueRow427();
        if (!volRow) {
          out.errors.push('427: the copy-sound-volume slider is missing from the expert menu');
        } else {
          const slider = volRow.querySelector('input.value-range');
          slider.value = 0;
          slider.dispatchEvent(new Event('input', { bubbles: true }));
          slider.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(200);
        }
        window.__FIRE__('prompt-hotkey', 'p4');
        await sleep(300);
        if (window.__BEEP_COUNT__ !== 2) out.errors.push('427: volume 0 still played a beep: ' + window.__BEEP_COUNT__);
        // Switching the option off must play nothing either, even back at a normal volume.
        if (volRow) {
          const slider = volRow.querySelector('input.value-range');
          slider.value = 30;
          slider.dispatchEvent(new Event('input', { bubbles: true }));
          slider.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(200);
        }
        // Re-fetch the row: copySound gates copySoundVolume's visibility, so its own
        // switch is a GATING_FLAGS member and toggling it just rebuilt the whole
        // tab -- the earlier `soundRow` element is now detached from the document.
        const soundRowOff = await expertRow427('Sound on copy');
        if (soundRowOff) soundRowOff.querySelector('input.switch').click();
        await sleep(200);
        window.__FIRE__('prompt-hotkey', 'p6');
        await sleep(300);
        if (window.__BEEP_COUNT__ !== 2) out.errors.push('427: copySound off still played a beep: ' + window.__BEEP_COUNT__);
        const firstTab427 = document.querySelector('#expert-tabs-bar .expert-tab');
        if (firstTab427) { firstTab427.click(); await sleep(200); }
      }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // 431: the password-skip switch only matters once the clipboard inbox itself is
    // on and defaults to on; its former length slider is a fixed value now.
    {
      const valueRow431 = (label) => [...document.querySelectorAll('#expert-flags .value-field')]
        .find(r => ((r.querySelector('.value-head span') || {}).textContent || '').includes(label));
      document.getElementById('settings').classList.remove('hidden');
      await click('expert-open');
      const watcherRow = await expertRow('Record copies from other apps');
      if (!watcherRow) {
        out.errors.push('431: the record-copies option is missing from the expert menu');
      } else {
        watcherRow.querySelector('input.switch').click();
        await sleep(200);
        const skipRow = await expertRow('Skip likely passwords');
        if (!skipRow) {
          out.errors.push('431: the password-skip switch does not appear once the inbox is on');
        } else {
          if (!skipRow.querySelector('input.switch').checked) {
            out.errors.push('431: the password-skip switch is not on by default');
          }
          if (valueRow431('Password guess minimum length')) {
            out.errors.push('431: the minimum-length slider is still in the expert menu');
          }
        }
        // Switching the inbox off with items still collected asks once whether to
        // keep them (the fixture's inbox holds one item). Every answer is probed:
        // keep, delete and Escape; the last leaves the switch off, the shipped default.
        const confirmBox431 = document.getElementById('confirm-modal');
        const clipCalls431 = (cmd) => (window.__SMOKE_ARGS__ || []).filter(([c]) => c === cmd).length;
        const switchWatcher431 = async (answer) => {
          const row = await expertRow('Record copies from other apps');
          const sw = row && row.querySelector('input.switch');
          if (!sw) { out.errors.push('431: the record-copies switch vanished'); return false; }
          window.__SMOKE_ARGS__ = [];
          sw.click();
          await sleep(300);
          if (sw.checked) return true; // switched on: nothing to answer
          if (confirmBox431.classList.contains('hidden')) {
            out.errors.push('431: switching the inbox off with items did not ask what to do with them');
            return false;
          }
          if (answer === 'keep') document.getElementById('confirm-cancel').click();
          else if (answer === 'delete') document.getElementById('confirm-ok').click();
          else document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
          await sleep(300);
          if (!confirmBox431.classList.contains('hidden')) {
            out.errors.push('431: the inbox keep/delete question stayed open after the answer ' + answer);
            confirmBox431.classList.add('hidden');
          }
          const cleared = clipCalls431('clip_inbox_clear');
          if (answer === 'delete' && cleared !== 1) out.errors.push('431: deleting the collected items called clip_inbox_clear ' + cleared + ' times');
          if (answer !== 'delete' && cleared !== 0) out.errors.push('431: answer ' + answer + ' still cleared the inbox');
          return true;
        };
        await switchWatcher431('keep');
        await switchWatcher431('on');
        await switchWatcher431('delete');
        await switchWatcher431('on');
        await switchWatcher431('escape');
        const watcherEnd431 = await expertRow('Record copies from other apps');
        if (watcherEnd431 && watcherEnd431.querySelector('input.switch').checked) {
          out.errors.push('431: the inbox switch did not end up off');
        }
        await sleep(200);
        const firstTab431 = document.querySelector('#expert-tabs-bar .expert-tab');
        if (firstTab431) { firstTab431.click(); await sleep(150); }
      }
      await click('expert-back');
      document.getElementById('settings').classList.add('hidden');
    }
    // 108: Save must fit --winctl-w in every language -- the longest translations
    // ("Speichern", "Speichern" DE at 9 chars is the longest of the 11) must not
    // overflow the fixed-width button.
    {
      const saveBtn = document.getElementById('save');
      if (saveBtn) {
        const prev = saveBtn.textContent;
        for (const word of ['Save', 'Speichern', 'Guardar', 'Salva', 'Salvar', 'Zapisz', 'Opslaan', 'Kaydet', 'Salveaza', '保存']) {
          saveBtn.textContent = word;
          if (saveBtn.scrollWidth > saveBtn.clientWidth + 1) {
            out.errors.push('save button overflows with "' + word + '" at ' + window.innerWidth + 'px');
          }
        }
        saveBtn.textContent = prev;
      }
    }
    // 275: the longest translation of About, Check for updates and the version
    // status in each of the 11 languages must not overflow its own box, at the
    // two smallest viewports the suite runs.
    if (window.innerWidth <= 740) {
      const aboutBtn = document.getElementById('about-btn');
      const updateBtn = document.getElementById('update-btn');
      const updateVersion = document.getElementById('update-version');
      if (!aboutBtn || !updateBtn || !updateVersion) {
        out.errors.push('about-btn, update-btn or update-version missing at ' + window.innerWidth + 'px');
      } else {
        const fits = (el) => el.scrollHeight <= el.clientHeight + 1 && el.scrollWidth <= el.clientWidth + 1;
        const prevAbout = aboutBtn.textContent;
        for (const word of ['About', 'Über', 'Acerca de', 'Informazioni', 'Sobre', 'O programie', '关于', 'バージョン情報', 'Over', 'Hakkında', 'Despre']) {
          aboutBtn.textContent = word;
          if (!fits(aboutBtn)) out.errors.push('about-btn overflows with "' + word + '" at ' + window.innerWidth + 'px');
        }
        aboutBtn.textContent = prevAbout;
        const prevUpdate = updateBtn.textContent;
        for (const word of ['Check for updates', 'Update suchen', 'Buscar actualizaciones', 'Cerca aggiornamenti',
          'Verificar atualizações', 'Sprawdź aktualizacje', '检查更新', 'アップデートを確認', 'Controleren op updates',
          'Güncellemeleri denetle', 'Caută actualizări']) {
          updateBtn.textContent = word;
          if (!fits(updateBtn)) out.errors.push('update-btn overflows with "' + word + '" at ' + window.innerWidth + 'px');
        }
        updateBtn.textContent = prevUpdate;
        const prevVersion = updateVersion.textContent;
        for (const word of ['Version 2.7.0', 'Version 2.7.0', 'Versión 2.7.0', 'Versione 2.7.0', 'Versão 2.7.0',
          'Wersja 2.7.0', '版本 2.7.0', 'バージョン 2.7.0', 'Versie 2.7.0', 'Sürüm 2.7.0', 'Versiunea 2.7.0']) {
          updateVersion.textContent = word;
          if (updateVersion.scrollHeight > updateVersion.clientHeight + 1) {
            out.errors.push('update-version wraps to a second line with "' + word + '" at ' + window.innerWidth + 'px');
          }
        }
        updateVersion.textContent = prevVersion;
      }
    }
    // The changelog popup renders the release Markdown instead of dumping it, and
    // grows with the text rather than clipping it.
    await click('update-btn');
    await click('update-version');
    const notes = document.getElementById('update-modal-notes');
    if (document.getElementById('update-modal').classList.contains('hidden')) {
      out.errors.push('the changelog popup did not open');
    } else {
      if (!notes.querySelector('h3') || !notes.querySelector('li') || !notes.querySelector('table')) {
        out.errors.push('the changelog is not rendered as markup');
      }
      if (/[#*`|]/.test(notes.textContent)) {
        out.errors.push('raw Markdown left in the changelog: ' + notes.textContent.slice(0, 60));
      }
      if (notes.textContent.includes('9.9.9')) out.errors.push('the changelog repeats the version from its own title');
      if (notes.scrollHeight > notes.clientHeight + 1 && getComputedStyle(notes).overflowY !== 'auto') {
        out.errors.push('long release notes cannot be scrolled');
      }
      await click('update-modal-cancel');
    }
    // The appearance scales are user-tunable, so the layout has to survive them being
    // raised - the same guards once more with modal/icon/composer zoom at 130%.
    const zoomed = document.documentElement.style;
    document.getElementById('settings').classList.add('hidden');
    for (const v of ['--modal-zoom', '--icon-zoom', '--composer-zoom', '--primary-zoom']) {
      zoomed.setProperty(v, '1.3');
    }
    await sleep(400);
    layoutScan('main window at 130% scale');
    mustReach('main window at 130% scale', '.win-btn, .composer > button');
    await click('library-btn');
    layoutScan('library at 130% scale');
    await click('library-close');
    document.getElementById('settings').classList.remove('hidden');
    await sleep(250);
    layoutScan('settings at 130% scale');
    // Clipped away by a dialog that cannot scroll counts as unreachable: elementFromPoint
    // respects the clip, and scrollIntoView has nothing to scroll.
    mustReach('settings at 130% scale', '#settings-main button, #settings-main select');
    // 516: the redesigned About dialog at 130% scale.
    document.getElementById('about-btn').click();
    await sleep(150);
    if (window.__ABOUT_FITS_CHECK__) window.__ABOUT_FITS_CHECK__('130% scale');
    document.getElementById('about-ok').click();
    await sleep(120);
    await click('expert-open');
    layoutScan('expert menu at 130% scale');
    // Indentation is the only thing that says which setting belongs to which
    // toggle. Every nested row has to actually step in, one level per depth.
    for (const el of document.querySelectorAll('#expert-flags .expert-sub')) {
      const depth = Number(el.style.getPropertyValue('--expert-depth') || 0);
      const inset = parseFloat(getComputedStyle(el).marginLeft) || 0;
      if (!(depth > 0)) out.layout.push('expert menu: a nested row carries no depth');
      else if (inset < depth * 10) {
        out.layout.push('expert menu: a depth-' + depth + ' row is indented ' +
          Math.round(inset) + 'px, so it does not read as nested');
      }
    }
    await click('expert-back');
    document.getElementById('settings').classList.add('hidden');
    for (const v of ['--modal-zoom', '--icon-zoom', '--composer-zoom', '--primary-zoom']) {
      zoomed.removeProperty(v);
    }
    await sleep(300);
    // Leave the theme editor open so the scans below measure its real layout.
    document.getElementById('settings').classList.remove('hidden');
    {
      const themeSel914 = document.getElementById('theme-select');
      themeSel914.value = '__theme-editor';
      themeSel914.dispatchEvent(new Event('change', { bubbles: true }));
      await sleep(250);
    }
    // Every dropdown and swatch in the theme editor is anchored low in the window -
    // exactly where a popover used to open past the bottom edge. Open the LAST of
    // each kind, which is the worst case, and let the popover scan judge it.
    const lastVisible = (sel) => [...document.querySelectorAll(sel)]
      .filter(e => e.getClientRects().length).pop();
    for (const [sel, what] of [['#theme-modal select', 'theme editor dropdown'],
                               ['#theme-modal .swatch, #theme-modal .color-dot', 'theme editor colour']]) {
      const el = lastVisible(sel);
      if (!el) continue;
      el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      el.click();
      await sleep(250);
      layoutScan(what);
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      await sleep(150);
    }
    await click('theme-modal-close');
"""
