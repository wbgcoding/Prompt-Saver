SECTION = """\
    // The portable take-over dialog opens on startup: it must be the themed modal
    // with an icon and its own two labels, not a bare system box.
    const cm = document.getElementById('confirm-modal');
    // It opens once show_main_window resolves, which is not a fixed delay.
    for (let i = 0; i < 60 && cm.classList.contains('hidden'); i++) await sleep(100);
    if (cm.classList.contains('hidden')) {
      // Only a real failure if the app got as far as asking the backend; headless
      // rAF occasionally stalls before that, which is a harness hiccup, not a bug.
      if ((window.__SMOKE_CALLS__ || []).includes('takeover_offer')) {
        out.errors.push('the take-over dialog did not open');
      } else {
        out.tabs.push('(take-over check skipped: startup stalled)');
      }
    } else {
      const icon = document.getElementById('confirm-icon');
      if (icon.classList.contains('hidden') || !icon.querySelector('svg')) {
        out.errors.push('take-over dialog has no icon');
      }
      const msg = document.getElementById('confirm-msg').textContent;
      if (!msg.includes('Example-Store')) out.errors.push('take-over dialog does not name the folder');
      if (!msg.includes('7')) out.errors.push('take-over dialog does not name the prompt count');
      if (!/\\n/.test(msg)) out.errors.push('take-over dialog lost its line breaks');
      // 754: the sentence about the skip button names the real button label.
      const skipLabel754 = document.getElementById('confirm-cancel').textContent.trim();
      if (msg.includes('{skip}') || !skipLabel754 || !msg.includes(skipLabel754)) {
        out.errors.push('754: the take-over text does not name the skip button (' + JSON.stringify(skipLabel754) + ')');
      }
      // 449/755: a "do not ask again" box, unticked by default, so the question
      // comes back unless the person actively opts in to settling it.
      const rememberCb = document.getElementById('confirm-cb-remember');
      if (!rememberCb) {
        out.errors.push('take-over dialog has no "do not ask again" checkbox');
      } else if (rememberCb.checked) {
        out.errors.push('755: the "do not ask again" checkbox is checked by default');
      }
      window.__SMOKE_ARGS__ = [];
      await click('confirm-cancel'); // "start empty"
      const applyArgs = (window.__SMOKE_ARGS__ || []).find(c => c[0] === 'takeover_apply');
      if (!applyArgs || applyArgs[1].adopt !== false || applyArgs[1].remember !== false) {
        out.errors.push('755: starting empty with the box untouched must call takeover_apply with adopt false, remember false, got: ' + JSON.stringify(applyArgs && applyArgs[1]));
      }
    }
    clearInterval(heartbeat);
    document.documentElement.style.removeProperty('--smoke-tick');
    layoutScan('main window');
    // 2: seeding is checked unconditionally on every launch (the backend itself
    // decides whether the library was actually empty) - must have been called once
    // by now regardless of the fixture's non-empty PROMPTS array.
    if (!(window.__SMOKE_CALLS__ || []).includes('seed_example_prompts')) {
      out.errors.push('seed_example_prompts was not called on startup');
    }
    // 182: the take-over question must be answered BEFORE seeding runs, or a fresh
    // portable install could seed the examples first and hide a real store behind
    // an already-non-empty library (takeover_offer bails out once prompts exist).
    {
      const calls = window.__SMOKE_CALLS__ || [];
      const iTakeover = calls.indexOf('takeover_offer');
      const iSeed = calls.indexOf('seed_example_prompts');
      if (iTakeover === -1 || iSeed === -1 || iTakeover > iSeed) {
        out.errors.push('takeover_offer must be called before seed_example_prompts, got order: ' + calls.join(','));
      }
    }
    // The fixture library is never empty, so the banner must stay hidden - but the
    // dismiss button still has to exist and be wired correctly for when it isn't.
    {
      const banner = document.getElementById('examples-banner');
      if (!banner || !banner.classList.contains('hidden')) {
        out.errors.push('the examples banner is visible over a non-empty library');
      }
      window.__SMOKE_CALLS__ = [];
      document.getElementById('examples-dismiss').click();
      await sleep(150);
      if (!window.__SMOKE_CALLS__.includes('dismiss_example_prompts')) {
        out.errors.push('the "remove examples" button does not call dismiss_example_prompts');
      }
      // 421: no seeded example ids left after dismiss, so the next render must not
      // spend an extra round trip on the live pending count -- it already knows to
      // stay hidden.
      if (window.__SMOKE_CALLS__.includes('example_prompts_pending')) {
        out.errors.push('421: the examples banner fetched the pending count with no examples seeded');
      }
    }
    // 20: help is opened from a button INSIDE settings, so settings stays open
    // behind it - both used to share the same base z-index, so help rendered
    // under the very dialog it was opened from.
    {
      document.getElementById('gear').click();
      await sleep(120);
      document.getElementById('help-open').click();
      await sleep(150);
      const helpModal = document.querySelector('#help .modal');
      const r = helpModal.getBoundingClientRect();
      const topEl = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      if (!helpModal.contains(topEl)) out.errors.push('20: help dialog does not stack above settings');
      document.getElementById('help-close').click();
      await sleep(120);
      document.getElementById('settings').classList.add('hidden');
    }
    // 76: About dialog - version fills in, no raw i18n key, closes again.
    {
      document.getElementById('gear').click();
      await sleep(120);
      document.getElementById('about-btn').click();
      await sleep(120);
      const aboutModal = document.getElementById('about-modal');
      if (aboutModal.classList.contains('hidden')) {
        out.errors.push('76: the About dialog did not open');
      } else {
        layoutScan('about dialog');
        const verText = document.getElementById('about-version').textContent.trim();
        if (!verText) out.errors.push('76: the About dialog version is empty');
        for (const el of aboutModal.querySelectorAll('*')) {
          const txt = (el.children.length ? '' : el.textContent || '').trim();
          if (/^about[A-Z][A-Za-z]+$/.test(txt)) out.errors.push('76: About dialog shows a raw i18n key: ' + txt);
        }
        // 516: the three link buttons form a full-width column, so the Close
        // button below them is the one centered on its own; the FIRST link
        // button sits as far from the modal's left edge as the Close button's
        // right edge sits from the modal's right edge -- checking .modal-actions
        // itself would be wrong, that flex container always spans the full inner
        // width regardless of justify-content; only its child moves.
        const aboutBox = aboutModal.querySelector('.modal');
        const firstBtn = document.getElementById('about-whats-new');
        const closeBtn = document.getElementById('about-ok');
        // Skipped once the box actually scrolls: a shown scrollbar eats into
        // the content width on the right only, which shifts the centered
        // button without the row itself being off center.
        if (aboutBox && firstBtn && closeBtn && aboutBox.scrollHeight <= aboutBox.clientHeight + 1) {
          const mb = aboutBox.getBoundingClientRect();
          const fb = firstBtn.getBoundingClientRect();
          const cb = closeBtn.getBoundingClientRect();
          const leftGap = fb.left - mb.left;
          const rightGap = mb.right - cb.right;
          if (Math.abs(leftGap - rightGap) > 1) {
            out.errors.push('516: About dialog Close button is not centered, left gap ' + Math.round(leftGap) + 'px vs right gap ' + Math.round(rightGap) + 'px');
          }
        }
        // 516: every link button fits its own text (no horizontal overflow),
        // the box itself never grows wider than its column (a tall box just
        // scrolls, same as the version-history and theme-editor dialogs), and
        // Close stays reachable by scrolling to it -- in the current language
        // and at the current window size (this probe runs at every SIZES
        // entry, including the app minimum, and once more at 130% scale
        // further down, and once in each of pl/ro/de below).
        const aboutFitsCheck = (label) => {
          for (const btn of aboutModal.querySelectorAll('.about-link-btn')) {
            if (btn.scrollWidth > btn.clientWidth + 1) {
              out.errors.push('516: an About link button overflows (' + label + '): ' + btn.textContent.trim());
            }
          }
          if (aboutBox.scrollWidth > aboutBox.clientWidth + 1) {
            out.errors.push('516: the About dialog is wider than its own box (' + label + ')');
          }
          const closeBtn2 = document.getElementById('about-ok');
          closeBtn2.scrollIntoView({ block: 'nearest' });
          const cr = closeBtn2.getBoundingClientRect();
          const hit = document.elementFromPoint(cr.left + cr.width / 2, cr.top + cr.height / 2);
          if (!closeBtn2.contains(hit)) {
            out.errors.push('516: the About dialog Close button is not reachable (' + label + ')');
          }
        };
        aboutFitsCheck('default');
        window.__ABOUT_FITS_CHECK__ = aboutFitsCheck;
        document.getElementById('about-ok').click();
        await sleep(120);
        if (!aboutModal.classList.contains('hidden')) out.errors.push('76: the About dialog did not close');
      }
      document.getElementById('settings').classList.add('hidden');
    }
    // 230: closing the settings panel through its own close button returns
    // keyboard focus to whatever had it before the panel opened (settingsOpener,
    // set in the gear buttons own click handler) instead of dropping it to
    // document.body.
    {
      const gearBtn = document.getElementById('gear');
      gearBtn.focus();
      gearBtn.click();
      await sleep(120);
      if (document.getElementById('settings').classList.contains('hidden')) {
        out.errors.push('230: the settings panel did not open from the gear button');
      } else {
        // Move focus away from the gear button first (as it would move once the
        // user interacts with anything inside the open panel) -- .click() alone
        // never moves focus, so without this the gear button would trivially
        // still be focused regardless of whether hideSettings() restores it.
        const closeBtn = document.getElementById('settings-close');
        closeBtn.focus();
        closeBtn.click();
        await sleep(120);
        if (document.activeElement !== gearBtn) {
          const got = document.activeElement && (document.activeElement.id || document.activeElement.tagName);
          out.errors.push('230: closing settings did not return focus to the gear button, landed on ' + got);
        }
      }
    }
    // 97/21: in-app documentation - every tab reaches the page, none is empty,
    // search narrows the list, a tip callout shows up somewhere.
    {
      document.getElementById('help-open').click();
      await sleep(150);
      const helpEl = document.getElementById('help');
      if (helpEl.classList.contains('hidden')) {
        out.errors.push('the help menu did not open');
      } else {
        layoutScan('help menu');
        mustReach('help menu', '#help-close, #help-search');
        const groups = () => [...document.querySelectorAll('#help-body .help-group')];
        const rawKeyCheck = (where) => {
          for (const el of document.querySelectorAll('#help-tabs-bar *, #help-body *')) {
            const txt = (el.children.length ? '' : el.textContent || '').trim();
            if (/^help[A-Z][A-Za-z]+$/.test(txt)) out.errors.push(where + ' shows a raw i18n key: ' + txt);
          }
        };
        // 21: one tab renders at a time now - click through every one, summing what
        // each shows instead of expecting them all stacked in a single render.
        const tabs = [...document.querySelectorAll('#help-tabs-bar .expert-tab')];
        if (tabs.length < 2) out.errors.push('the help menu shows too few section tabs (' + tabs.length + ')');
        let totalGroups = 0;
        let sawTip = false;
        let sawSketch = false;
        let sawIcon = false;
        const seenTitles = new Set();
        for (let i = 0; i < tabs.length; i++) {
          document.querySelectorAll('#help-tabs-bar .expert-tab')[i].click();
          await sleep(120);
          layoutScan('help menu tab ' + i);
          rawKeyCheck('help menu tab ' + i);
          const here = groups();
          if (!here.length) out.errors.push('help tab ' + i + ' shows no categories');
          totalGroups += here.length;
          const empties = here.filter(g => !g.querySelector('.help-overview').textContent.trim());
          if (empties.length) out.errors.push(empties.length + ' help category has an empty overview (tab ' + i + ')');
          if (document.querySelector('#help-body .help-tip')) sawTip = true;
          if (document.querySelector('#help-body .help-sketch svg')) sawSketch = true;
          // 500: a {icon:key} placeholder in ui/help.js is swapped for the real
          // button's own svg at render time, never left as raw text, never empty.
          const icons = [...document.querySelectorAll('#help-body .help-icon')];
          if (icons.length) {
            sawIcon = true;
            for (const ic of icons) {
              const svg = ic.querySelector('svg');
              if (!svg || !svg.innerHTML.trim()) {
                out.errors.push('500: a help icon placeholder has no real button svg (tab ' + i + ')');
              }
            }
          }
          if (document.getElementById('help-body').textContent.includes('{icon:')) {
            out.errors.push('500: a raw {icon: placeholder is visible in the help text (tab ' + i + ')');
          }
          for (const g of here) {
            const t = g.querySelector('.help-group-title-text');
            if (t) seenTitles.add(t.textContent.trim());
          }
        }
        // 186: the four new help sections actually render, each on its own tab pass.
        for (const title of ['Introduction tour', 'Screenshot editor', 'Text recognition (OCR)', 'Language packs']) {
          if (!seenTitles.has(title)) out.errors.push('186: help category "' + title + '" never rendered');
        }
        if (totalGroups < 15) {
          out.errors.push('the help menu shows too few categories across all tabs (' + totalGroups + ')');
        }
        if (!sawTip) out.errors.push('no help category shows a tip callout');
        if (!sawSketch) out.errors.push('no help category shows a diagram');
        if (!sawIcon) out.errors.push('500: no help category shows a real button icon inline');
        // 500: every icon a placeholder can point at is a button that actually
        // exists in the program, not a name help.js made up.
        for (const id of ['snip-btn', 'record-btn', 'ocr-btn', 'voice-btn', 'library-btn', 'theme-edit-open']) {
          if (!document.getElementById(id)) out.errors.push('500: help icon source button #' + id + ' does not exist');
        }
        const search = document.getElementById('help-search');
        // Multi-word query whose words are not literally adjacent anywhere - only the
        // term-AND-logic search (73) finds it, a plain includes() search finds nothing.
        search.value = 'grid tags';
        search.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(200);
        const filtered = groups();
        if (!filtered.length || filtered.length >= totalGroups) {
          out.errors.push('searching the help menu did not narrow the category list');
        }
        const hitsEl = document.querySelector('#help-body .help-hits');
        if (!hitsEl || !/[0-9]/.test(hitsEl.textContent)) out.errors.push('the help search shows no hit counter');
        if (filtered.length > 1 && !filtered[0].classList.contains('collapsed')) {
          out.errors.push('help search results with more than one hit are not collapsed by default');
        }
        search.value = 'zzz_no_such_help_topic_zzz';
        search.dispatchEvent(new Event('input', { bubbles: true }));
        await sleep(200);
        if (groups().length) out.errors.push('a nonsense help search still shows categories');
        if (!document.querySelector('#help-body .hint')) out.errors.push('an empty help search shows no "no results" hint');
        search.value = '';
        search.dispatchEvent(new Event('input', { bubbles: true }));
        document.getElementById('help-close').click();
        await sleep(150);
        if (!document.getElementById('help').classList.contains('hidden')) {
          out.errors.push('the help menu did not close');
        }
      }
    }
    // Window controls and composer buttons are the ones a squeezed header used to
    // carry off the right edge, where no pointer could ever follow them.
    mustReach('main window', '.win-btn, .header-actions > button, .composer > button');
"""
