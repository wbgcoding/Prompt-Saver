SECTION = """\
    // 188: every built-in theme, at the app's own smallest window, must not clip
    // or overlap anything -- only worth the extra round trips at the tightest
    // size, so this skips at the other three viewports this same probe also runs at.
    // Repeatedly switching this select has been observed to stall the headless
    // browser's virtual clock (the same class of harness limitation documented
    // at 193 and 186) -- one switch to the widest-risk theme (monospace font,
    // the one most likely to push text past a tight boundary) stands in for
    // cycling all eight.
    // 188: the widest layout risk a theme can add is a font-family change
    // (Programmer switches the whole UI to monospace, which is wider per
    // character and the one real way a theme swap could newly clip text) --
    // checked by setting the same data-theme attribute applyTheme() itself
    // sets, not by driving the live select: repeatedly switching that control
    // has been observed to stall the headless browser's virtual clock (the
    // same class of harness limitation documented at 193 and 186), and a raw
    // attribute set proves the identical CSS effect without touching it.
    if (innerWidth <= 700) {
      const startTheme = document.documentElement.getAttribute('data-theme');
      document.documentElement.setAttribute('data-theme', 'programmer');
      layoutScan('theme programmer');
      document.documentElement.setAttribute('data-theme', startTheme || 'system');
    }
    // 277: each built-in theme bakes its own --select-chevron (a data-URI SVG
    // cannot hold a var(), so the stroke colour is substituted per theme
    // instead). Checked here, right after the settings dialog is shown, so
    // #lang-select is a real visible <select> and not one buried inside an
    // already-closed modal elsewhere in this probe.
    {
      const priorTheme2 = document.documentElement.getAttribute('data-theme');
      const BUILTIN_THEMES2 = ['light', 'dark', 'programmer', 'neon', 'cyberpunk', 'ocean'];
      const chevronByTheme = {};
      for (const theme of BUILTIN_THEMES2) {
        document.documentElement.setAttribute('data-theme', theme);
        chevronByTheme[theme] = getComputedStyle(document.getElementById('lang-select')).backgroundImage;
      }
      if (priorTheme2) document.documentElement.setAttribute('data-theme', priorTheme2);
      else document.documentElement.removeAttribute('data-theme');
      const vals = Object.values(chevronByTheme);
      for (const [theme, v] of Object.entries(chevronByTheme)) {
        if (!v || v === 'none') out.errors.push('select chevron in theme ' + theme + ': no background-image at all');
        if (/888888/.test(v)) out.errors.push('select chevron in theme ' + theme + ': still the hardcoded grey');
      }
      if (new Set(vals).size !== BUILTIN_THEMES2.length) {
        out.errors.push('select chevron: expected ' + BUILTIN_THEMES2.length + ' distinct colours across built-in themes, got ' + new Set(vals).size);
      }
    }
    // 508: every select in the main window carries a real border and a
    // themed, non-transparent background, and its option list uses the
    // theme surface colour, in every built-in theme. A bare select used to
    // fall back to the plain OS look because the global select rule left
    // border and background unset.
    {
      const priorTheme3 = document.documentElement.getAttribute('data-theme');
      const SELECT_THEMES = ['light', 'dark', 'programmer', 'neon', 'cyberpunk', 'ocean'];
      const mainSelects = [...document.querySelectorAll('select')].filter((s) => s.options.length > 0);
      if (!mainSelects.length) out.errors.push('508: no populated select found in the main window to audit');
      const swatch = document.createElement('div');
      swatch.style.cssText = 'position:absolute;left:-9999px;top:-9999px;background-color:var(--surface)';
      document.body.appendChild(swatch);
      for (const theme of SELECT_THEMES) {
        document.documentElement.setAttribute('data-theme', theme);
        const surfaceRgb = getComputedStyle(swatch).backgroundColor;
        for (const sel of mainSelects) {
          const label = sel.id || sel.className || 'unnamed select';
          const cs = getComputedStyle(sel);
          if (parseFloat(cs.borderTopWidth) <= 0) {
            out.errors.push('508: ' + label + ' has no border in theme ' + theme);
          }
          // batch-view-select is deliberately transparent (it sits inline with
          // ghost-btn neighbours and its native list is replaced by a themed
          // popup, see the comment above its rule) -- not the bare-select
          // defect this point is about.
          if (!sel.classList.contains('batch-view-select')) {
            if (cs.backgroundColor === 'rgba(0, 0, 0, 0)' || cs.backgroundColor === 'transparent') {
              out.errors.push('508: ' + label + ' has a transparent background in theme ' + theme);
            }
          }
          // A selected option shows the accent colour (option:checked); pick
          // an unselected one to check the plain surface colour instead.
          const unselected = [...sel.options].find((o) => !o.selected);
          if (unselected) {
            const optBg = getComputedStyle(unselected).backgroundColor;
            if (optBg !== surfaceRgb) {
              out.errors.push('508: ' + label + ' option background is ' + optBg + ', expected the theme surface ' + surfaceRgb + ' in theme ' + theme);
            }
          }
        }
      }
      swatch.remove();
      if (priorTheme3) document.documentElement.setAttribute('data-theme', priorTheme3);
      else document.documentElement.removeAttribute('data-theme');
    }
    await click('theme-edit-open');
    const tmodal = document.getElementById('theme-modal');
    if (tmodal.classList.contains('hidden')) {
      out.errors.push('the theme editor did not open');
    } else {
      const rows = [...document.querySelectorAll('#theme-edit-vars .theme-var-row')];
      if (rows.length !== 9) out.errors.push('the theme editor shows ' + rows.length + ' colour rows instead of 9');
      if (document.querySelectorAll('#theme-edit-vars .theme-var-group').length !== 4) {
        out.errors.push('the theme colours are not split into four groups');
      }
      const hex = rows[0] && rows[0].querySelector('.theme-hex');
      if (!hex) {
        out.errors.push('a theme colour row has no hex field');
      } else {
        const original = hex.value;
        // A valid value applies live; an invalid one is rejected and rolled back.
        hex.value = '#123456';
        hex.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(120);
        if (getComputedStyle(document.documentElement).getPropertyValue('--bg').trim() !== '#123456') {
          out.errors.push('editing a colour does not preview live');
        }
        hex.value = 'nonsense';
        hex.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(120);
        if (hex.value !== '#123456') out.errors.push('an invalid hex value was not rejected');
        // D75 part 2: "Revert changes" undoes everything since the editor opened -
        // WITHOUT closing it, the one thing that actually sets it apart from Cancel.
        await click('theme-edit-revert');
        await sleep(120);
        if (tmodal.classList.contains('hidden')) {
          out.errors.push('reverting the theme draft closed the editor');
        }
        // renderThemeEditor() rebuilt the rows, so the old references are stale.
        const revertedRows = [...document.querySelectorAll('#theme-edit-vars .theme-var-row')];
        const revertedHex = revertedRows[0] && revertedRows[0].querySelector('.theme-hex');
        if (!revertedHex || revertedHex.value !== original) {
          out.errors.push('reverting did not restore the original colour: ' + (revertedHex && revertedHex.value));
        }
        if (getComputedStyle(document.documentElement).getPropertyValue('--bg').trim() === '#123456') {
          out.errors.push('reverting did not undo the live preview');
        }
      }
      // 277: a custom theme's own --muted must re-derive the select chevron too,
      // not just apply directly like the other colour tokens (a data-URI SVG
      // cannot hold a var(), so this is its own derivation path). Re-queries the
      // rows fresh -- the earlier revert rebuilt them, so `rows` above is stale.
      {
        const freshRows = [...document.querySelectorAll('#theme-edit-vars .theme-var-row')];
        const mutedHex = freshRows[5] && freshRows[5].querySelector('.theme-hex');
        if (!mutedHex) {
          out.errors.push('the muted colour row is missing');
        } else {
          mutedHex.value = '#334455';
          mutedHex.dispatchEvent(new Event('input', { bubbles: true }));
          await sleep(120);
          const chevron = getComputedStyle(document.documentElement).getPropertyValue('--select-chevron');
          if (!/334455/i.test(chevron)) {
            out.errors.push('a custom theme muted colour does not re-derive the select chevron: ' + chevron);
          }
          await click('theme-edit-revert');
          await sleep(120);
        }
      }
      // 76: switch to the stored theme (ct1) so it has a real id - the editor
      // opened on a fresh, unsaved draft by default, which correctly has none.
      const targetSel = document.getElementById('theme-edit-target');
      targetSel.value = 'ct1';
      targetSel.dispatchEvent(new Event('change', { bubbles: true }));
      await sleep(120);
      const exportBtn = document.getElementById('theme-edit-export');
      if (!exportBtn || exportBtn.classList.contains('hidden')) {
        out.errors.push('the export button is missing/hidden for a saved theme');
      } else {
        exportBtn.click();
        await sleep(120);
        const call = [...(window.__SMOKE_ARGS__ || [])].reverse().find(([c]) => c === 'export_custom_theme');
        if (!call || call[1].id !== 'ct1') out.errors.push('export did not pass the current theme id');
      }
      document.getElementById('theme-edit-import').click();
      await sleep(150);
      if (document.getElementById('theme-edit-target').value !== 'ct2') {
        out.errors.push('importing a theme did not open it in the editor');
      }
      if (document.getElementById('theme-edit-name').value !== 'Imported theme') {
        out.errors.push('the imported theme did not fill the editor with its own values');
      }
      if (exportBtn && exportBtn.classList.contains('hidden')) {
        out.errors.push('the export button stayed hidden after importing a (now saved) theme');
      }
      if (![...themeSel.options].some(o => o.value === 'custom:ct2')) {
        out.errors.push('the imported theme is missing from the theme list');
      }
      await click('theme-edit-cancel');
      await sleep(150);
      if (document.documentElement.style.getPropertyValue('--bg') === '#123456') {
        out.errors.push('cancelling the theme editor kept the preview');
      }
    }
    layoutScan('settings');
    mustReach('settings', '#settings .modal-x, #settings-main button');
    await click('expert-open');
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click();
      await sleep(200);
      out.tabs.push(tb.textContent.trim() + ':' +
        document.querySelectorAll('#expert-flags .expert-group').length);
      layoutScan('expert tab ' + tb.textContent.trim());
      // 101: paired two-up, a dropdown row stacks its label above the field (a
      // 240px column has no room for label-beside-field) -- checked wherever the
      // grid actually produces a pair, which needs no fixed pixel width of its own.
      for (const row of document.querySelectorAll('.expert-cols > .expert-dropdown-row')) {
        const label = row.querySelector('.value-head');
        const field = row.querySelector('.expert-select');
        if (!label || !field) continue;
        const lb = label.getBoundingClientRect();
        const fb = field.getBoundingClientRect();
        if (fb.top < lb.bottom - 1) {
          out.errors.push('101: paired dropdown row label is not above its field in ' + tb.textContent.trim());
        }
      }
    }
    mustReach('expert menu', '#expert-tabs-bar .expert-tab');
"""
