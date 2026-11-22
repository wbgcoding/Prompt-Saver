"""Copy history probes: filters, clearing with and without the confirmation, the backup purge and the source switch."""
from .arprobe import make_ar_probe


def run(browser, tmp):
    ar_probe = make_ar_probe(browser, tmp)
    failed = 0
    failed += ar_probe('c3', 'copies from other apps: a failed clear offers no undo (storage panel and copy history)', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true } };</script>\n', r"""
    await bootReady();
    RESULTS.clip_inbox_clear = () => { throw 'delete-incomplete'; };
    const undoBtn = () => document.querySelector('#toast .toast-btn');
    // 1) the retention row of the backups dialog
    document.getElementById('gear').click(); await sleep(200);
    document.getElementById('settings-backup-btn').click(); await sleep(400);
    const row = document.querySelector('#retention-panel [data-setting-row="retention-clipInbox"]');
    const clean = row && row.querySelector('.retention-clean');
    if (!clean) { errs.push('564-c3: the cleanup button for copies from other apps was not found'); }
    else {
      window.__SMOKE_ARGS__.length = 0;
      clean.click(); await sleep(400);
      if (!callsFor('clip_inbox_clear').length) errs.push('564-c3: fixture: the clear was never attempted');
      if (undoBtn()) errs.push('564-c3: the storage panel offered Undo after the clear failed');
      const msg = document.getElementById('toast').textContent;
      if (/cleared/i.test(msg)) errs.push('564-c3: the storage panel reported a clear that failed: ' + msg);
    }
    // 2) the copy history itself, under the Elsewhere filter
    document.getElementById('backup-modal').classList.add('hidden');
    document.getElementById('toast').classList.remove('show');
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(400);
    const srcEl = document.getElementById('journal-source');
    if (!srcEl || srcEl.classList.contains('hidden')) { errs.push('564-c3: the source select is not shown in the fixture'); }
    else {
      srcEl.value = 'elsewhere'; srcEl.dispatchEvent(new Event('change')); await sleep(200);
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('journal-clear').click(); await sleep(300);
      const cmC3 = document.getElementById('confirm-modal');
      if (cmC3 && !cmC3.classList.contains('hidden')) { document.getElementById('confirm-ok').click(); await sleep(300); }
      if (!callsFor('clip_inbox_clear').length) errs.push('564-c3: fixture: the history clear was never attempted');
      if (undoBtn()) errs.push('564-c3: the copy history offered Undo after the clear failed');
    }
""")
    failed += ar_probe('m2', 'copy history: Clear follows the source filter, names both counts, stops on a failed clear and ignores an empty target', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true } };</script>\n', r"""
    await bootReady();
    const undoBtn = () => document.querySelector('#toast .toast-btn');
    const confirmOpen = () => !document.getElementById('confirm-modal').classList.contains('hidden');
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(400);
    // All = both stores: the confirm names both counts and cancelling clears nothing.
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('journal-clear').click(); await sleep(300);
    if (!confirmOpen()) errs.push('564-m2: Clear history under All asked nothing');
    else {
      const msg = document.getElementById('confirm-msg').textContent;
      if (!/\(4 entries\)/.test(msg) || !/\(2 entries\)/.test(msg)) errs.push('564-m2: the confirm text does not name both counts: ' + msg);
      document.getElementById('confirm-cancel').click(); await sleep(200);
    }
    if (callsFor('clear_copy_history').length || callsFor('clip_inbox_clear').length) errs.push('564-m2: a cancelled clear still deleted something');
    // A failed copy-log clear stops: nothing else is cleared, no undo.
    RESULTS.clear_copy_history = () => { throw 'boom'; };
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('journal-clear').click(); await sleep(300);
    if (confirmOpen()) { document.getElementById('confirm-ok').click(); await sleep(300); }
    if (!callsFor('clear_copy_history').length) errs.push('564-m2: fixture: the copy log clear was never attempted');
    if (callsFor('clip_inbox_clear').length) errs.push('564-m2: the stored entries were cleared although the copy log clear failed');
    if (undoBtn()) errs.push('564-m2: an Undo was offered after the clear failed');
    delete RESULTS.clear_copy_history;
    // An empty target does nothing: no dialog, no toast, no undo.
    RESULTS.clip_inbox_list = () => [];
    document.getElementById('toast').classList.remove('show', 'actionable');
    { const srcSel = document.getElementById('journal-source'); srcSel.value = 'elsewhere'; srcSel.dispatchEvent(new Event('change')); } await sleep(300);
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('journal-clear').click(); await sleep(300);
    // Only the purge question is asked (the backups may still hold earlier entries); cancelling does nothing.
    if (!confirmOpen()) errs.push('564-m2: clearing an empty store under Elsewhere did not ask the purge question');
    else {
      if (document.getElementById('confirm-cb-purgeBackups')) errs.push('564-m2: the purge-only dialog carries a box');
      document.getElementById('confirm-cancel').click(); await sleep(200);
    }
    if (callsFor('purge_clip_inbox_from_backups').length) errs.push('564-m2: cancelling the purge question still purged');
    if (callsFor('clip_inbox_clear').length) errs.push('564-m2: clearing an empty store still called clip_inbox_clear');
    if (undoBtn() || document.getElementById('toast').classList.contains('show')) errs.push('564-m2: clearing an empty store showed a toast');
""")
    failed += ar_probe('l3', 'copies from other apps: stored entries keep the history tab and the purge button while nothing records', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { copyHistory: false, clipWatcher: false } };</script>\n', r"""
    await bootReady();
    const tabsHidden = () => document.getElementById('library-tabs').classList.contains('hidden');
    document.getElementById('library-btn').click(); await sleep(500);
    if (tabsHidden()) errs.push('564-l3: the history tab is gone although entries from other apps are stored');
    else {
      document.getElementById('library-tab-history').click(); await sleep(400);
      if (document.querySelectorAll('#journal-body .clip-row').length !== 2) errs.push('564-l3: the stored entries are not listed');
    }
    // With nothing stored and nothing recording, the tab is gone again.
    document.getElementById('library-close').click(); await sleep(200);
    RESULTS.clip_inbox_list = () => [];
    document.getElementById('library-btn').click(); await sleep(500);
    if (!tabsHidden()) errs.push('564-l3: the history tab is offered with nothing to show and nothing recording');
""")
    failed += ar_probe('bx717', 'copy history filters: at 680 px they take two rows, the source is a select and only one All chip remains', 'index', '680x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true } };</script>\n', r"""
    await bootReady();
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(500);
    const first = document.querySelector('#journal-body .journal-item');
    const src = document.getElementById('journal-source');
    if (!first) errs.push('717: fixture: the history lists no entry');
    if (!src || src.tagName !== 'SELECT' || src.classList.contains('hidden')) errs.push('717: the source is not a visible select');
    else {
      const tops = ['journal-q', 'journal-source', 'journal-types'].map((id) => document.getElementById(id).getBoundingClientRect().top).sort((a, b) => a - b);
      let rows = 1;
      for (let i = 1; i < tops.length; i++) if (tops[i] - tops[i - 1] > 8) rows++;
      if (rows > 2) errs.push('717: the history filters take ' + rows + ' rows');
      if (first && document.getElementById('journal-types').getBoundingClientRect().top >= first.getBoundingClientRect().top) errs.push('717: the type chips are not above the first entry');
      if (src.options.length !== 3) errs.push('717: the source select has ' + src.options.length + ' options');
      window.__SMOKE_ARGS__ = [];
      src.value = 'elsewhere'; src.dispatchEvent(new Event('change')); await sleep(200);
      if (document.querySelector('#journal-body .journal-item') || !document.querySelector('#journal-body .clip-row')) errs.push('717: picking Elsewhere did not narrow the list to the stored entries');
    }
    // 718: only the type row keeps an All chip; the source All is a select option.
    const allChips = document.querySelectorAll('#library-panel-history button[data-i18n="filterAll"]').length;
    if (allChips !== 1) errs.push('718: the history shows ' + allChips + ' All chips instead of one');
""")
    failed += ar_probe('bx720', '17003: clearing the copy history offers the backup purge as an unticked box inside the dialog', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true } };</script>\n', r"""
    await bootReady();
    RESULTS.purge_clip_inbox_from_backups = () => [2, 0];
    const dlgOpen = () => !document.getElementById('confirm-modal').classList.contains('hidden');
    const purgeCalls = () => callsFor('purge_clip_inbox_from_backups').length;
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(500);
    if (document.getElementById('clip-purge-backups')) errs.push('17003: the header purge button is still there');
    const pickSource = async (v) => { const s = document.getElementById('journal-source'); s.value = v; s.dispatchEvent(new Event('change')); await sleep(200); };
    // Unticked: the clear runs, the backups are not touched.
    window.__SMOKE_ARGS__ = [];
    document.getElementById('journal-clear').click(); await sleep(300);
    let box = document.getElementById('confirm-cb-purgeBackups');
    if (!dlgOpen() || !box) errs.push('17003: the clear dialog has no purge box');
    else {
      if (box.checked) errs.push('17003: the purge box starts ticked');
      document.getElementById('confirm-ok').click(); await sleep(300);
      if (purgeCalls() !== 0) errs.push('17003: confirming with the box unticked purged the backups');
      if (!callsFor('clip_inbox_clear').length) errs.push('17003: the clear did not run');
    }
    // Ticked: the purge runs once.
    window.__resetClipInbox(); await window.__TAURI__.core.invoke('clip_inbox_list');
    document.getElementById('library-close').click(); await sleep(200);
    document.getElementById('library-btn').click(); await sleep(400);
    window.__SMOKE_ARGS__ = [];
    document.getElementById('journal-clear').click(); await sleep(300);
    box = document.getElementById('confirm-cb-purgeBackups');
    if (!dlgOpen() || !box) errs.push('17003: the second clear dialog has no purge box');
    else {
      box.checked = true; box.dispatchEvent(new Event('change'));
      document.getElementById('confirm-ok').click(); await sleep(400);
      if (purgeCalls() !== 1) errs.push('17003: ticking the box called the purge ' + purgeCalls() + ' times instead of once');
      if (!/2/.test(document.getElementById('toast').textContent)) errs.push('17003: the purge result is not reported');
    }
    // Only the copies from other apps, purged: nothing to undo.
    window.__resetClipInbox(); await window.__TAURI__.core.invoke('clip_inbox_list');
    document.getElementById('library-close').click(); await sleep(200);
    document.getElementById('library-btn').click(); await sleep(400);
    await pickSource('elsewhere');
    window.__SMOKE_ARGS__ = [];
    document.getElementById('toast').classList.remove('show', 'actionable');
    document.getElementById('journal-clear').click(); await sleep(300);
    box = document.getElementById('confirm-cb-purgeBackups');
    if (!box) errs.push('17003: the Elsewhere clear dialog has no purge box');
    else {
      box.checked = true; box.dispatchEvent(new Event('change'));
      document.getElementById('confirm-ok').click(); await sleep(400);
      if (purgeCalls() !== 1) errs.push('17003: the Elsewhere purge did not run once');
      if (document.querySelector('#toast .toast-btn')) errs.push('17003: an undo was offered for entries purged from the backups');
    }
    // The app filter does not cover the copies from other apps: no box.
    window.__resetClipInbox(); await window.__TAURI__.core.invoke('clip_inbox_list');
    document.getElementById('library-close').click(); await sleep(200);
    document.getElementById('library-btn').click(); await sleep(400);
    await pickSource('app');
    document.getElementById('journal-clear').click(); await sleep(300);
    if (document.getElementById('confirm-cb-purgeBackups')) errs.push('17003: the app-only clear dialog offers the purge box');
    document.getElementById('confirm-cancel').click(); await sleep(200);
""")
    failed += ar_probe('bx720off', '17003: with the clear confirmation off there is no dialog and no purge', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true, confirmClearHistory: false } };</script>\n', r"""
    await bootReady();
    RESULTS.purge_clip_inbox_from_backups = () => [2, 0];
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(500);
    window.__SMOKE_ARGS__ = [];
    document.getElementById('journal-clear').click(); await sleep(400);
    if (!document.getElementById('confirm-modal').classList.contains('hidden')) errs.push('17003: a dialog opened although the confirmation is off');
    if (!callsFor('clip_inbox_clear').length) errs.push('17003: the clear did not run without the dialog');
    if (callsFor('purge_clip_inbox_from_backups').length) errs.push('17003: the backups were purged without the dialog');
""")
    failed += ar_probe('bx720empty', '17003: with the history already empty the purge question alone is offered (confirm on)', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true } };</script>\n', r"""
    await bootReady();
    RESULTS.purge_clip_inbox_from_backups = () => [2, 0];
    RESULTS.clip_inbox_list = () => [];
    const dlgOpen = () => !document.getElementById('confirm-modal').classList.contains('hidden');
    const purgeCalls = () => callsFor('purge_clip_inbox_from_backups').length;
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(500);
    { const s = document.getElementById('journal-source'); s.value = 'elsewhere'; s.dispatchEvent(new Event('change')); } await sleep(300);
    window.__SMOKE_ARGS__ = [];
    document.getElementById('journal-clear').click(); await sleep(300);
    if (!dlgOpen()) errs.push('17003: an empty Elsewhere clear offered no purge question');
    else {
      if (!/already empty/.test(document.getElementById('confirm-msg').textContent)) errs.push('17003: the purge-only dialog reads ' + document.getElementById('confirm-msg').textContent);
      document.getElementById('confirm-cancel').click(); await sleep(300);
      if (purgeCalls() !== 0) errs.push('17003: cancelling the purge question purged');
      document.getElementById('journal-clear').click(); await sleep(300);
      document.getElementById('confirm-ok').click(); await sleep(400);
      if (purgeCalls() !== 1) errs.push('17003: confirming the purge question called the purge ' + purgeCalls() + ' times');
      if (callsFor('clip_inbox_clear').length || callsFor('clear_copy_history').length) errs.push('17003: the purge-only question cleared a store');
      if (!/2/.test(document.getElementById('toast').textContent)) errs.push('17003: the purge result is not reported');
    }
""")
    failed += ar_probe('bx720emptyoff', '17003: an empty history with the confirmation off does nothing at all', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true, confirmClearHistory: false } };</script>\n', r"""
    await bootReady();
    RESULTS.purge_clip_inbox_from_backups = () => [2, 0];
    RESULTS.clip_inbox_list = () => [];
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(500);
    { const s = document.getElementById('journal-source'); s.value = 'elsewhere'; s.dispatchEvent(new Event('change')); } await sleep(300);
    window.__SMOKE_ARGS__ = [];
    document.getElementById('journal-clear').click(); await sleep(400);
    if (!document.getElementById('confirm-modal').classList.contains('hidden')) errs.push('17003: a dialog opened although the confirmation is off');
    if (callsFor('purge_clip_inbox_from_backups').length) errs.push('17003: the backups were purged without a dialog');
""")
    failed += ar_probe('bx720tickoff', '17003: ticking the box, then switching the confirmation off, then clearing purges nothing', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true } };</script>\n', r"""
    await bootReady();
    RESULTS.purge_clip_inbox_from_backups = () => [2, 0];
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(500);
    document.getElementById('journal-clear').click(); await sleep(300);
    const box = document.getElementById('confirm-cb-purgeBackups');
    if (!box) errs.push('17003: fixture: the clear dialog has no purge box');
    else { box.checked = true; box.dispatchEvent(new Event('change')); }
    document.getElementById('confirm-cancel').click(); await sleep(300);
    document.getElementById('library-close').click(); await sleep(200);
    // Switch the confirmation off the way a user does, in the expert menu.
    document.getElementById('gear').click(); await sleep(200);
    document.getElementById('expert-open').click(); await sleep(300);
    let row = null;
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click(); await sleep(80);
      row = document.querySelector('#expert-flags [data-setting-row="confirmClearHistory"]');
      if (row) break;
    }
    if (!row) errs.push('17003: fixture: the confirmation switch was not found');
    else { const cb = row.querySelector('input[type=checkbox]'); cb.checked = false; cb.dispatchEvent(new Event('change', { bubbles: true })); await sleep(300); }
    document.getElementById('settings-close').click(); await sleep(300);
    document.getElementById('library-btn').click(); await sleep(400);
    window.__SMOKE_ARGS__ = [];
    document.getElementById('journal-clear').click(); await sleep(400);
    if (!document.getElementById('confirm-modal').classList.contains('hidden')) errs.push('17003: a dialog opened although the confirmation is off');
    if (!callsFor('clip_inbox_clear').length) errs.push('17003: fixture: the clear did not run');
    if (callsFor('purge_clip_inbox_from_backups').length) errs.push('17003: the earlier tick purged the backups with the confirmation off');
""")
    failed += ar_probe('bx720fails', '17003: a failed stored-entries clear skips the purge, a failure stays readable and offers the undo for what was cleared', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true } };</script>\n', r"""
    await bootReady();
    const dlgOpen = () => !document.getElementById('confirm-modal').classList.contains('hidden');
    const purgeCalls = () => callsFor('purge_clip_inbox_from_backups').length;
    const toastText = () => document.getElementById('toast').textContent;
    const undoBtn = () => document.querySelector('#toast .toast-btn');
    const tickAndConfirm = async () => {
      document.getElementById('journal-clear').click(); await sleep(300);
      const box = document.getElementById('confirm-cb-purgeBackups');
      if (!dlgOpen() || !box) { errs.push('17003: fixture: no dialog or box'); if (dlgOpen()) document.getElementById('confirm-cancel').click(); return; }
      box.checked = true; box.dispatchEvent(new Event('change'));
      document.getElementById('confirm-ok').click(); await sleep(600);
    };
    const reopen = async () => {
      window.__resetClipInbox(); await window.__TAURI__.core.invoke('clip_inbox_list');
      document.getElementById('library-close').click(); await sleep(200);
      document.getElementById('library-btn').click(); await sleep(400);
    };
    const pickSource = async (v) => { const s = document.getElementById('journal-source'); s.value = v; s.dispatchEvent(new Event('change')); await sleep(200); };
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(500);
    // The stored-entries clear fails: no purge, the error stays.
    RESULTS.purge_clip_inbox_from_backups = () => [2, 0];
    RESULTS.clip_inbox_clear = () => { throw 'delete-incomplete'; };
    window.__SMOKE_ARGS__ = [];
    await tickAndConfirm();
    if (!callsFor('clip_inbox_clear').length) errs.push('17003: fixture: the stored-entries clear was not attempted');
    if (purgeCalls()) errs.push('17003: the backups were purged although the stored-entries clear failed');
    if (!/could not be deleted/.test(toastText())) errs.push('17003: the failed clear is not what the toast shows: ' + toastText());
    // The copy log was already cleared: the error stays readable and the undo is offered for it.
    if (!undoBtn()) errs.push('17003: no undo was offered for the copy log cleared before the failure');
    else {
      undoBtn().click(); await sleep(400);
      if (!callsFor('restore_copy_history').length) errs.push('17003: the undo after the failed clear did not restore the copy log');
    }
    delete RESULTS.clip_inbox_clear;
    // The purge fails, filter Elsewhere and filter All: the error stays the last toast.
    await reopen();
    RESULTS.purge_clip_inbox_from_backups = () => { throw 'delete-incomplete'; };
    await pickSource('elsewhere');
    window.__SMOKE_ARGS__ = [];
    await tickAndConfirm();
    if (!purgeCalls()) errs.push('17003: fixture: the purge was not attempted');
    if (!/could not be deleted/.test(toastText())) errs.push('17003: Elsewhere: the failed purge is not what the toast shows: ' + toastText());
    if (undoBtn()) errs.push('17003: Elsewhere: an undo was offered although only stored entries were cleared');
    await reopen();
    await pickSource('all');
    window.__SMOKE_ARGS__ = [];
    await tickAndConfirm();
    if (!/could not be deleted/.test(toastText())) errs.push('17003: All: the failed purge is not what the toast shows: ' + toastText());
    if (!undoBtn()) errs.push('17003: All: no undo was offered for the copy log cleared before the failed purge');
""")
    failed += ar_probe('bx721', 'copy history: the other-apps switch carries a word that follows its state', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true } };</script>\n', r"""
    await bootReady();
    document.getElementById('library-btn').click(); await sleep(400);
    document.getElementById('library-tab-history').click(); await sleep(500);
    const tog = document.getElementById('clip-toggle');
    const word = () => tog.textContent.trim();
    if (!tog.classList.contains('head-toggle')) errs.push('721: the switch is not a head-toggle');
    if (word() !== 'Other apps: on') errs.push('721: with the watcher on the switch reads ' + JSON.stringify(word()));
    if (!tog.classList.contains('active')) errs.push('721: the switch is not marked active while on');
    tog.click(); await sleep(300);
    // Switching off asks once about the stored entries; keeping them is the way out.
    if (!document.getElementById('confirm-modal').classList.contains('hidden')) { document.getElementById('confirm-cancel').click(); await sleep(300); }
    if (word() !== 'Other apps: off') errs.push('721: after a click the switch reads ' + JSON.stringify(word()));
    if (tog.classList.contains('active')) errs.push('721: the switch is still active after switching off');
""")
    failed += ar_probe('bx722', 'copy history: today reads relative, other days keep the short date, the full stamp sits in the tooltip', 'index', '1000x780', '', r"""
    await bootReady();
    const core = await import('./core.js');
    const jr = await import('./journal.js');
    await core.setLang('de');
    const nowS = Math.floor(Date.now() / 1000);
    const fiveAgo = nowS - 300;
    if (new Date(fiveAgo * 1000).toDateString() !== new Date().toDateString()) { errs.push('722: skipped, the run straddles midnight'); }
    else {
      const rtf = new Intl.RelativeTimeFormat('de', { numeric: 'auto' });
      const want = rtf.format(-5, 'minute');
      if (jr.fmtWhen(fiveAgo) !== want) errs.push('722: five minutes ago reads ' + JSON.stringify(jr.fmtWhen(fiveAgo)) + ' instead of ' + JSON.stringify(want));
      if (!/5 Minuten/.test(jr.fmtWhen(fiveAgo))) errs.push('722: the German wording is not "vor 5 Minuten": ' + jr.fmtWhen(fiveAgo));
      const row = jr.journalClipRow({ id: 9, ts: fiveAgo, text: 'abc' });
      const timeEl = row.querySelector('.clip-time');
      if (timeEl.textContent !== want) errs.push('722: the elsewhere row shows ' + JSON.stringify(timeEl.textContent));
      if (timeEl.title !== new Date(fiveAgo * 1000).toLocaleString('de')) errs.push('722: the elsewhere row has no full-stamp tooltip: ' + JSON.stringify(timeEl.title));
    }
    const yesterday = nowS - 86400 - 3600;
    if (jr.fmtWhen(yesterday) !== jr.fmtCopyTime(yesterday)) errs.push('722: yesterday does not read like the short date and time');
    await core.setLang('en');
""")
    return failed
