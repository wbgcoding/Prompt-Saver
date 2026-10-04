SECTION = """\
    // 96: the short "Latest X.Y.Z" status keeps update-actions no taller than
    // before, with the longest of the 11 translations (Romanian) -- and exactly
    // as tall as the button row below it once the window has real room (740px+;
    // below that the About/Check-for-updates button labels themselves already
    // do not fit beside a third item, independent of what that item says).
    {
      if (document.getElementById('settings').classList.contains('hidden')) await click('gear');
      const ua = document.querySelector('.update-actions');
      const btnRow = document.querySelector('.settings-btn-row');
      const uv = document.getElementById('update-version');
      if (ua && btnRow && uv) {
        const prevText = uv.textContent;
        uv.textContent = 'Cea mai recenta 2.7.0';
        const uaH = ua.getBoundingClientRect().height;
        uv.textContent = 'Neueste Version bereits installiert';
        const uaHOld = ua.getBoundingClientRect().height;
        const rowH = btnRow.getBoundingClientRect().height;
        if (uaH > uaHOld + 1) {
          out.errors.push('the short status is taller than the old long sentence was: ' + Math.round(uaH) + ' vs ' + Math.round(uaHOld));
        }
        if (window.innerWidth >= 740 && uaH > rowH + 1) {
          out.errors.push('update-actions is taller than settings-btn-row at ' + window.innerWidth + 'px: ' + Math.round(uaH) + ' vs ' + Math.round(rowH));
        }
        // 174: the version label sits centred in its own column, same width as
        // the About/Check-for-updates buttons beside it (only meaningful
        // unwrapped -- at 740px+ the three sit on one row per the check above).
        const aboutBtn = document.getElementById('about-btn');
        if (window.innerWidth >= 740 && aboutBtn) {
          const uvRect = uv.getBoundingClientRect();
          const abRect = aboutBtn.getBoundingClientRect();
          if (Math.abs(uvRect.width - abRect.width) > 2) {
            out.errors.push('update-version column width does not match about-btn: ' + Math.round(uvRect.width) + ' vs ' + Math.round(abRect.width));
          }
          const cs = getComputedStyle(uv);
          if (cs.textAlign !== 'center') {
            out.errors.push('update-version is not centre-aligned: ' + cs.textAlign);
          }
        }
        uv.textContent = prevText;
      } else {
        out.errors.push('update-actions, settings-btn-row or update-version missing from the settings panel');
      }
    }
    // 108: reset-settings and delete-data carry their full labels. In English
    // they stay on one line; a long translation (German "Einstellungen
    // zurücksetzen", Dutch "Alle gegevens verwijderen") may wrap to two lines.
    // The flex-row default stretch means a single wrapped button grows the WHOLE
    // row, so compare against the row's own single-line reference height rather
    // than siblings.
    {
      if (document.getElementById('settings-main').classList.contains('hidden')) {
        document.getElementById('expert-open').click();
        await sleep(50);
      }
      const reset = document.getElementById('reset-settings');
      const del = document.getElementById('delete-data');
      const help = document.getElementById('help-open');
      if (!reset || !del || !help) {
        out.errors.push('reset-settings, delete-data or help-open missing from settings-btn-row');
      } else if (help.getBoundingClientRect().height < 10) {
        out.errors.push('settings-btn-row is not visible at ' + window.innerWidth + 'px');
      } else {
        const prevReset = reset.textContent;
        const prevDel = del.textContent;
        const rowH = () => Math.round(help.getBoundingClientRect().height);
        reset.textContent = 'Reset';
        del.textContent = 'Delete';
        const baseH = rowH();
        // The full English labels stay on one line.
        reset.textContent = 'Reset settings';
        del.textContent = 'Delete all data';
        if (rowH() > baseH + 1) {
          out.errors.push('settings-btn-row wraps with the English labels at ' + window.innerWidth + 'px: ' + baseH + ' -> ' + rowH());
        }
        // Long translations may wrap to two lines, but all three buttons grow
        // together, never beyond two lines, and no label spills sideways.
        reset.textContent = 'Einstellungen zurücksetzen';
        del.textContent = 'Alle gegevens verwijderen';
        const longH = rowH();
        const hs = [reset, del, help].map((b) => Math.round(b.getBoundingClientRect().height));
        const spill = [reset, del].some((b) => b.scrollWidth > b.clientWidth + 1);
        reset.textContent = prevReset;
        del.textContent = prevDel;
        if (longH > baseH * 2) {
          out.errors.push('settings-btn-row grows past two lines with long labels at ' + window.innerWidth + 'px: ' + baseH + ' -> ' + longH);
        }
        if (new Set(hs).size > 1) {
          out.errors.push('settings-btn-row buttons differ in height with long labels at ' + window.innerWidth + 'px: ' + hs.join('/'));
        }
        if (spill) {
          out.errors.push('a settings-btn-row label overflows its button at ' + window.innerWidth + 'px');
        }
      }
    }
    // 109: hide-top (left of Minimize) and hide-bottom (left of Save) share the
    // same left/right edges -- both bars right-align on the same window edge and
    // win-controls/Save share --winctl-w. Also: hiding both toggles (barToggles
    // off) must not leave a gap where they used to sit. Only checked >= 700px:
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
      const ht = document.getElementById('hide-top');
      const hb = document.getElementById('hide-bottom');
      const winMin = document.getElementById('win-min');
      const winControls = document.querySelector('.win-controls');
      const saveBtn = document.getElementById('save');
      if (!ht || !hb || !winMin || !winControls || !saveBtn) {
        out.errors.push('hide-top, hide-bottom, win-min, win-controls or save missing');
      } else if (window.innerWidth >= 700) {
        const htR = ht.getBoundingClientRect();
        const hbR = hb.getBoundingClientRect();
        if (Math.abs(htR.right - hbR.right) > 1 || Math.abs(htR.left - hbR.left) > 1) {
          out.errors.push('hide-top/hide-bottom are not aligned at ' + window.innerWidth + 'px: left ' +
            Math.round(htR.left) + '/' + Math.round(hbR.left) + ', right ' + Math.round(htR.right) + '/' + Math.round(hbR.right));
        }
        if (htR.right > winMin.getBoundingClientRect().left + 1) {
          out.errors.push('hide-top is not left of win-min at ' + window.innerWidth + 'px');
        }
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
    // 426: a double-click on any of the four bar buttons folds/unfolds both
    // bars together while barToggleBoth stays on; a single click is untouched
    // either way. A real double-click fires click, click, dblclick -- click()
    // exercises the existing single-bar toggle, the synthetic dblclick layers
    // the extra behaviour on top, same as a genuine double-click would.
    {
      if (!document.getElementById('settings').classList.contains('hidden')) {
        await click('settings-close');
        await sleep(50);
      }
      const dbl = async (id) => {
        await click(id);
        document.getElementById(id).dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        await sleep(100);
      };
      const bothHidden = () => document.body.classList.contains('no-header') && document.body.classList.contains('no-composer');
      const bothShown = () => !document.body.classList.contains('no-header') && !document.body.classList.contains('no-composer');
      await dbl('hide-top');
      if (!bothHidden()) out.errors.push('426: double-clicking hide-top did not fold both bars');
      await dbl('show-bottom');
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
        await dbl('hide-top');
        if (document.body.classList.contains('no-composer')) {
          out.errors.push('426: double-click still folded the other bar with the option off');
        }
        if (!document.body.classList.contains('no-header')) {
          out.errors.push('426: the plain single click stopped working with the option off');
        }
        // Undo the single click, then put the option back for the rest of the run.
        await click('show-top');
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
    // 275: about-btn/update-btn/update-version share a fixed one-third-width
    // column each -- the longest translation in each of the 11 languages must
    // not overflow its own box, at the two smallest viewports the suite runs.
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
    await click('theme-edit-open');
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
