"""Findings suite: the main-window probes generated from the review findings, each with its own boot."""
import json
import re

from .harness import REPO, unescape, run_chrome


def run(browser, tmp):
    failed = 0
    # ---- main window findings: generated block begin ----
    ar_scaffold = r"""
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => { for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50); return fn(); };
  try {
%s
%s
  } catch (e) { errs.push('probe threw: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""

    def ar_probe(tag, label, page, size, pre, body):
        side = tmp / "ui" / f"ar-{tag}.html"
        side.write_text(
            (REPO / "ui" / f"{page}.html").read_text(encoding="utf-8")
            .replace("<script", pre + '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", ar_scaffold % (ar_common, body) + "</body>"),
            encoding="utf-8")
        w, h = size.split("x")
        proc = run_chrome(browser, tmp / ('profile-ar-' + tag), f"{w},{h}", 45000, side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- {label}")
        for e in errs:
            print(f"   FAIL: {e}")
        if not errs:
            print("   ok")
        return len(errs)

    ar_common = r"""
    const visible = (el) => !!el && !el.classList.contains('hidden');
    const callsFor = (cmd) => (window.__SMOKE_ARGS__ || []).filter(([c]) => c === cmd).map(([, a]) => a);
    const tileEl = (id) => document.querySelector('#grid .tile[data-id="' + id + '"]');
    const centre = (el) => { const r = el.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
    const ptr = (type, x, y, el, extra) => (el || window).dispatchEvent(new PointerEvent(type, Object.assign({
      bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 6, clientX: x, clientY: y,
    }, extra || {})));
    const dragTo = async (tile, tx, ty) => {
      const [sx, sy] = centre(tile);
      ptr('pointerdown', sx, sy, tile);
      ptr('pointermove', sx + 8, sy - 8, tile);
      await sleep(60);
      ptr('pointermove', tx, ty, window);
      await sleep(60);
    };
    const endDrag = async (x, y) => { ptr('pointerup', x, y, window); await sleep(300); };
    const bootReady = async () => {
      await until(() => document.querySelectorAll('#grid .tile').length > 0, 200);
      const cm = document.getElementById('confirm-modal');
      for (let i = 0; i < 60 && !visible(cm); i++) await sleep(50);
      if (visible(cm)) { document.getElementById('confirm-cancel').click(); await sleep(300); }
    };
    const cellEl = (c, r) => document.querySelector('#grid .cell[data-col="' + c + '"][data-row="' + r + '"]');
"""

    failed += ar_probe('f1', 'grid folder: a tile dropped on its cell sends the folder to the vacated cell', 'index', '1000x780', '', r"""
    await bootReady();
    const folderCell = () => { const c = document.querySelector('#grid .cell .folder-tile'); return c ? [Number(c.parentElement.dataset.col), Number(c.parentElement.dataset.row)] : null; };
    const tabFor = (id) => document.querySelector('.view-btn[data-view-id="' + id + '"]');
    const before = folderCell();
    if (!before || before[0] !== 0 || before[1] !== 1) errs.push('563-1: fixture folder not at [0,1], got ' + JSON.stringify(before));
    // p9 sits at [1,2]; the first free cell after the move would be [2,1], not the vacated one
    const srcCell = tileEl('p9').parentElement;
    const old = [Number(srcCell.dataset.col), Number(srcCell.dataset.row)];
    await dragTo(tileEl('p9'), ...centre(cellEl(0, 1)));
    await endDrag(...centre(cellEl(0, 1)));
    const p9At = tileEl('p9').parentElement;
    if (p9At.dataset.col !== '0' || p9At.dataset.row !== '1') errs.push('563-1: the drop did not land p9 on [0,1]');
    // a full render from the stored layout, as any later refresh does
    tabFor('work').click(); await sleep(250);
    tabFor('home').click(); await sleep(300);
    const after = folderCell();
    if (!after || after[0] !== old[0] || after[1] !== old[1]) {
      errs.push('563-1: dropping a tile on the folder cell should put the folder on the vacated cell ' + JSON.stringify(old) + ', got ' + JSON.stringify(after));
    }
""")
    failed += ar_probe('f2', 'grid undo toast: a tab switch before Undo leaves both views alone', 'index', '1000x780', '', r"""
    await bootReady();
    const tabFor = (id) => document.querySelector('.view-btn[data-view-id="' + id + '"]');
    const before = JSON.stringify(layoutRead());
    function layoutRead() {
      const o = {};
      document.querySelectorAll('#grid .cell .tile:not(.folder-tile)').forEach((t) => { o[t.dataset.id] = t.parentElement.dataset.col + ',' + t.parentElement.dataset.row; });
      return o;
    }
    // p9 (cell 1,2) moves to the empty cell 2,2 and the undo toast appears
    await dragTo(tileEl('p9'), ...centre(cellEl(2, 2)));
    await endDrag(...centre(cellEl(2, 2)));
    const movedTo = tileEl('p9') && tileEl('p9').parentElement;
    if (!movedTo || movedTo.dataset.col !== '2' || movedTo.dataset.row !== '2') errs.push('563-2: the move to 2,2 did not happen');
    const btn = document.querySelector('#toast .toast-btn');
    if (!btn) { errs.push('563-2: no undo toast after the move'); }
    else {
      tabFor('work').click(); await sleep(300);
      const work = JSON.stringify(layoutRead());
      window.__SMOKE_ARGS__.length = 0;
      btn.click(); await sleep(300);
      const removes = callsFor('batch_prompts').filter((a) => a && a.action === 'removeView');
      const layouts = callsFor('set_layout');
      if (removes.length) errs.push('563-2: Undo after a tab switch removed the prompt from another view: ' + JSON.stringify(removes));
      if (layouts.length) errs.push('563-2: Undo after a tab switch rewrote the layout of the view on screen');
      if (JSON.stringify(layoutRead()) !== work) errs.push('563-2: the view on screen changed by Undo');
      tabFor('home').click(); await sleep(300);
      const p9 = tileEl('p9') && tileEl('p9').parentElement;
      if (!p9 || p9.dataset.col !== '2' || p9.dataset.row !== '2') errs.push('563-2: the original view lost the moved tile position');
    }
""")
    failed += ar_probe('f4', 'favorites grid: switching to it clears the grid selection', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { favViewButton: true } };</script>\n', r"""
    await bootReady();
    const bar = document.getElementById('grid-batch-bar');
    tileEl('p1').querySelector('.tile-select').click();
    await sleep(150);
    if (bar.classList.contains('hidden')) errs.push('563-4: selecting a tile did not show the batch bar (fixture)');
    const fav = document.getElementById('fav-view-btn');
    if (fav.classList.contains('hidden')) errs.push('563-4: the favorites toggle is not shown (fixture)');
    fav.click();
    await sleep(250);
    if (!bar.classList.contains('hidden')) errs.push('563-4: the batch bar stayed visible after switching to the favorites grid');
    if (document.body.classList.contains('grid-selecting')) errs.push('563-4: the grid stayed in selection mode after switching to the favorites grid');
""")
    failed += ar_probe('f11', 'folder popover member: a click in chain mode picks the prompt for the chain', 'index', '1000x780', '', r"""
    await bootReady();
    const click = async (el) => { const [x, y] = centre(el); ptr('pointerdown', x, y, el); ptr('pointerup', x, y, window); await sleep(300); };
    // The chain starts from the tile menu of p0, which also picks p0 as the first step.
    tileEl('p0').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 60, clientY: 60 }));
    document.querySelector('#ctx [data-act="chain"]').click();
    await sleep(300);
    const copies = () => callsFor('copy_text').map((a) => a && a.text);
    if (!copies().some((x) => /number 1[.]/.test(x || ''))) errs.push('563-11: fixture: the chain pick of p0 did not copy its text, got ' + JSON.stringify(copies()));
    await click(document.querySelector('#grid .folder-tile'));
    const member = document.querySelector('.folder-member-tile[data-id="p4"]');
    if (!member) { errs.push('563-11: the folder popover did not open'); }
    else {
      await click(member);
      const last = copies().pop() || '';
      if (!(/number 1[.]/.test(last) && /number 5[.]/.test(last))) errs.push('563-11: a member click in chain mode replaced the chain text instead of adding a step, clipboard now: ' + JSON.stringify(last));
    }
""")
    failed += ar_probe('f10', 'folder popover member: a fast second click pastes like the grid tile', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { autoPaste: true } };</script>\n', r"""
    await bootReady();
    const click = async (el) => { const [x, y] = centre(el); ptr('pointerdown', x, y, el); ptr('pointerup', x, y, window); await sleep(60); };
    await click(document.querySelector('#grid .folder-tile'));
    await sleep(200);
    const member = document.querySelector('.folder-member-tile[data-id="p4"]');
    if (!member) { errs.push('563-10: the folder popover did not open'); }
    else {
      window.__SMOKE_ARGS__.length = 0;
      await click(member);
      await click(member);
      await sleep(300);
      if (!callsFor('copy_prompt').length) errs.push('563-10: the first member click did not copy');
      if (!callsFor('paste_into_previous').length) errs.push('563-10: a fast second member click did not paste into the previous window');
    }
""")
    failed += ar_probe('f13', 'folder popover member: missing file banner like the grid tile', 'index', '1000x780', '', r"""
    await bootReady();
    const click = async (el) => { const [x, y] = centre(el); ptr('pointerdown', x, y, el); ptr('pointerup', x, y, window); await sleep(200); };
    window.__FIRE__('missing-files', ['p11']);
    await sleep(100);
    await click(document.querySelector('#grid .folder-tile'));
    const member = () => document.querySelector('.folder-member-tile[data-id="p11"]');
    if (!member()) { errs.push('563-13: the folder popover did not open'); }
    else {
      if (!member().classList.contains('file-missing')) errs.push('563-13: a member whose file is missing has no file-missing class');
      const banner = member().querySelector('.tile-error');
      if (!banner) errs.push('563-13: a member with an attached file has no missing-file banner element');
      else if (getComputedStyle(banner).display === 'none') errs.push('563-13: the missing-file banner of a member is not shown');
      window.__FIRE__('missing-files', []);
      await sleep(100);
      if (member().classList.contains('file-missing')) errs.push('563-13: the open popover kept file-missing after the file came back');
      window.__FIRE__('missing-files', ['p11']);
      await sleep(100);
      if (!member().classList.contains('file-missing')) errs.push('563-13: the open popover did not pick up a file going missing');
    }
""")
    failed += ar_probe('f14', 'folder popover member: per-prompt font, size and ink like the grid tile', 'index', '1000x780', '', r"""
    await bootReady();
    const click = async (el) => { const [x, y] = centre(el); ptr('pointerdown', x, y, el); ptr('pointerup', x, y, window); await sleep(200); };
    PROMPTS[4].font_size = 20;
    PROMPTS[4].font = 'mono';
    PROMPTS[6].font_size = 1;
    PROMPTS[8].color = '#fde047';
    const tabFor = (id) => document.querySelector('.view-btn[data-view-id="' + id + '"]');
    tabFor('work').click(); await sleep(250);
    tabFor('home').click(); await sleep(300);
    await click(document.querySelector('#grid .folder-tile'));
    const m = (id) => document.querySelector('.folder-member-tile[data-id="' + id + '"]');
    if (!m('p4')) { errs.push('563-14: the folder popover did not open'); }
    else {
      const l4 = m('p4').querySelector('.tile-name');
      if (l4.style.fontSize !== '20px') errs.push('563-14: a fixed-size prompt label has font size ' + JSON.stringify(l4.style.fontSize) + ', expected 20px');
      if (m('p4').dataset.fitMode !== 'fixed') errs.push('563-14: fitMode of a fixed-size member is ' + m('p4').dataset.fitMode);
      if (!l4.style.fontFamily) errs.push('563-14: the per-prompt font family is not applied to the member label');
      if (m('p6').dataset.fitMode !== 'auto') errs.push('563-14: fitMode of an auto-size member is ' + m('p6').dataset.fitMode);
      const ink = m('p8').style.color;
      if (ink !== 'rgb(0, 0, 0)' && ink !== '#000' && ink !== 'black') errs.push('563-14: a light tinted member has ink ' + JSON.stringify(ink) + ', expected a dark one');
    }
""")
    failed += ar_probe('g2', 'expert menu bulk actions never flip demo mode, network or sync', 'index', '1000x780', '<script>window.__DEMO_TEST__ = true;</script>\n', r"""
    await bootReady();
    const flagCalls = () => callsFor('set_ui_flag');
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('expert-open').click();
    await sleep(300);
    const cm = document.getElementById('confirm-modal');
    // per-category reset: the System category holds the demo switch
    const tabs = [...document.querySelectorAll('#expert-tabs-bar .expert-tab')];
    let reset = null;
    for (const tb of tabs) {
      tb.click(); await sleep(80);
      const grp = [...document.querySelectorAll('#expert-flags .expert-group')].find((g) => [...g.querySelectorAll('.switch-field span')].some((s) => s.textContent === 'Demo mode'));
      if (grp) { reset = grp.querySelector('.expert-group-reset'); break; }
    }
    window.__SMOKE_ARGS__.length = 0;
    if (reset) {
      reset.click(); await sleep(250);
      if (visible(cm)) { document.getElementById('confirm-ok').click(); await sleep(400); }
    }
    if (flagCalls().some((a) => a.key === 'demoMode')) errs.push('564-2: the category reset wrote demoMode');
    // per-row reset icon of the demo switch must not exist
    const rowBtn = [...document.querySelectorAll('#expert-flags .switch-field')].find((r) => (r.querySelector('span') || {}).textContent === 'Demo mode');
    const rowReset = rowBtn && rowBtn.querySelector('.expert-row-reset');
    if (rowReset && !rowReset.classList.contains('hidden')) errs.push('564-2: the demo switch offers a per-row reset');
    // reset all: the backend wipes the flag, so the marker is written back
    window.__SMOKE_ARGS__.length = 0;
    const rb = document.querySelector('#expert-flags [data-action="expert-reset"]');
    rb.click(); await sleep(150);
    rb.click(); await sleep(500);
    const seq = (window.__SMOKE_ARGS__ || []).filter(([c, a]) => c === 'reset_expert' || (c === 'set_ui_flag' && a.key === 'demoMode'));
    if (!seq.length || seq[0][0] !== 'reset_expert') errs.push('564-2: reset all did not run reset_expert');
    const back = seq.find(([c, a]) => c === 'set_ui_flag' && a.key === 'demoMode');
    if (!back || back[1].enabled !== true) errs.push('564-2: reset all left a running demo session without its marker');
""")
    failed += ar_probe('g4', 'expert menu: video folder numbers follow the folder, a late answer for an old folder is ignored', 'index', '1000x780', '', r"""
    await bootReady();
    const gates = [];
    RESULTS.video_folder_stats = (a) => new Promise((res) => gates.push({ dir: a.dir, res }));
    document.getElementById('gear').click(); await sleep(200);
    document.getElementById('expert-open').click(); await sleep(300);
    const statsEl = () => document.querySelector('.path-video-stats');
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click(); await sleep(80);
      if (statsEl()) break;
    }
    if (!statsEl()) { errs.push('564-4: the video folder row was not found'); }
    else {
      const line = () => statsEl().querySelector('span').textContent;
      const change = async (dir) => {
        window.__PICK_FOLDER__ = dir;
        statsEl().parentElement.querySelector('.path-btns button').click();
        await sleep(300);
      };
      if (gates.length < 1) errs.push('564-4: opening the row did not ask for the folder stats');
      gates[0].res([2, 2048]);
      await sleep(200);
      if (!/\b2\b/.test(line())) errs.push('564-4: fixture: first folder numbers missing: ' + line());
      await change('C:\Videos\B');
      if (/\b2\b/.test(line())) errs.push('564-4: the old folder numbers stayed on screen after picking another folder: ' + line());
      if (!statsEl().querySelector('button').disabled) errs.push('564-4: the cleanup button stayed enabled for a folder that is not measured yet');
      await change('C:\Videos\C');
      const byDir = (d) => gates.find((g) => g.dir === d);
      if (!byDir('C:\Videos\B') || !byDir('C:\Videos\C')) errs.push('564-4: fixture: both folder reads were not started');
      else {
        byDir('C:\Videos\C').res([7, 7168]);
        await sleep(200);
        byDir('C:\Videos\B').res([5, 5120]);
        await sleep(200);
        if (!/\b7\b/.test(line())) errs.push('564-4: a late answer for an older folder replaced the current numbers: ' + line());
      }
    }
""")
    failed += ar_probe('g6', 'expert menu: the sync readout is asked again each time the menu opens', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { promptSync: true } };</script>\n', r"""
    await bootReady();
    document.getElementById('gear').click(); await sleep(200);
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('expert-open').click(); await sleep(400);
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click(); await sleep(80);
      if (document.querySelector('.path-sync-stats')) break;
    }
    if (!document.querySelector('.path-sync-stats')) errs.push('564-6: the sync readout row was not found');
    const asked = () => callsFor('sync_status').length;
    const first = asked();
    if (first < 1) errs.push('564-6: opening the menu did not ask for the sync status');
    document.getElementById('expert-back').click(); await sleep(200);
    document.getElementById('expert-open').click(); await sleep(400);
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click(); await sleep(80);
      if (document.querySelector('.path-sync-stats')) break;
    }
    if (asked() <= first) errs.push('564-6: reopening the menu reused the first answer of the session (' + asked() + ' request(s))');
""")
    failed += ar_probe('g33', 'language dialog: a running install stays busy after the dialog is closed and reopened', 'index', '1000x780', '', r"""
    await bootReady();
    window.__OCR_CAPS_TEST__ = [
      { name: 'Language.OCR~~~ja-JP~0.0.1.0', tag: 'ja-JP', label: 'Japanese', state: 'NotPresent', version: '0.0.1.0', download_size: 1000000, install_size: null },
    ];
    RESULTS.ocr_install_result = () => null;
    document.getElementById('gear').click(); await sleep(200);
    document.getElementById('expert-open').click(); await sleep(300);
    let opener = null;
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click(); await sleep(80);
      opener = [...document.querySelectorAll('#expert-flags .expert-action')].find((r) => /languages/i.test(r.textContent) && r.closest('.expert-group') && /text recognition|ocr|image/i.test(r.closest('.expert-group').textContent));
      if (opener) break;
    }
    if (!opener) { errs.push('564-33: the language dialog opener was not found'); }
    else {
      const modal = document.getElementById('ocr-langs-modal');
      const btn = () => document.querySelector('#ocr-langs-body .ocr-lang-row .ghost-btn');
      opener.click(); await sleep(300);
      if (!btn()) { errs.push('564-33: no download button in the dialog'); }
      else {
        btn().click(); await sleep(200);
        document.getElementById('confirm-ok').click(); await sleep(400);
        if (!btn().disabled) errs.push('564-33: fixture: the row is not busy while the install runs');
        document.getElementById('ocr-langs-close-x').click(); await sleep(200);
        opener.click(); await sleep(400);
        if (!btn()) errs.push('564-33: no download button after reopening');
        else if (!btn().disabled) errs.push('564-33: after reopening the dialog the Download button is enabled while the install still runs');
        const started = callsFor('ocr_install_language').length;
        if (started !== 1) errs.push('564-33: expected one install start, got ' + started);
      }
    }
""")
    failed += ar_probe('g35', 'hotkey field: a refused capture leaves the global hotkeys off while the field has focus', 'index', '1000x780', '', r"""
    await bootReady();
    document.getElementById('gear').click(); await sleep(250);
    const input = document.getElementById('opt-hotkey');
    input.focus(); await sleep(150);
    const capture = () => callsFor('set_hotkey_capture');
    if (!capture().some((a) => a && a.active === true)) errs.push('564-35: fixture: focusing the field did not switch capture on');
    window.__SMOKE_ARGS__.length = 0;
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'b', ctrlKey: true, altKey: true, bubbles: true, cancelable: true }));
    await sleep(400);
    if (!callsFor('set_hotkey').length) errs.push('564-35: fixture: the keydown did not reach set_hotkey');
    if (document.activeElement !== input) errs.push('564-35: fixture: the field lost the focus');
    if (!capture().some((a) => a && a.active === true)) errs.push('564-35: after a refused combination the hotkeys were not taken off again while the field still has focus');
""")
    failed += ar_probe('m1', 'create dialog: saving with an empty text keeps the dialog and what was typed', 'index', '1000x780', '', r"""
    await bootReady();
    const empty = [...document.querySelectorAll('#grid .cell')].find((c) => !c.firstElementChild);
    if (!empty) { errs.push('564-m1: no empty cell in the fixture'); }
    else {
      empty.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })); await sleep(250);
      const modal = document.getElementById('modal');
      if (modal.classList.contains('hidden')) errs.push('564-m1: fixture: the create dialog did not open');
      document.getElementById('modal-name').value = 'Keep this name';
      document.getElementById('modal-text').value = '';
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('modal-confirm').click(); await sleep(300);
      if (modal.classList.contains('hidden')) errs.push('564-m1: saving with an empty text closed the dialog and dropped the typed name');
      else if (document.getElementById('modal-name').value !== 'Keep this name') errs.push('564-m1: the typed name was lost');
      if (callsFor('add_prompt').length) errs.push('564-m1: an empty prompt was saved');
      if (!modal.classList.contains('hidden') && document.activeElement !== document.getElementById('modal-text')) errs.push('564-m1: the text field was not focused for the missing text');
      if (document.getElementById('modal-confirm').disabled) errs.push('564-m1: Save stayed disabled after the refused save');
    }
""")
    failed += ar_probe('v2', 'dictation: a final text that lands late replaces the live partial instead of doubling it', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { voiceInterim: true } };</script>\n', r"""
    await bootReady();
    const btn = document.getElementById('voice-btn');
    const input = document.getElementById('input');
    input.value = '';
    btn.click(); await sleep(100);
    window.__FIRE__('voice-partial', { id: 1, token: 1, text: 'hello' });
    await sleep(50);
    if (input.value.trim() !== 'hello') errs.push('564-v2: fixture: the partial did not land, got ' + JSON.stringify(input.value));
    window.__FIRE__('voice-stopped', { id: 1, token: 1, reason: 'manual' });
    await sleep(50);
    window.__FIRE__('voice-text', { id: 1, token: 1, text: 'hello world' });
    await sleep(100);
    if (input.value.trim() !== 'hello world') errs.push('564-v2: the late final text should replace the partial, field holds ' + JSON.stringify(input.value));
""")
    failed += ar_probe('c3', 'copies from other apps: a failed clear offers no undo (storage panel and copy history)', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { clipWatcher: true } };</script>\n', r"""
    await bootReady();
    RESULTS.clip_inbox_clear = () => { throw 'delete-incomplete'; };
    const undoBtn = () => document.querySelector('#toast .toast-btn');
    // 1) the retention row of the backups dialog
    document.getElementById('gear').click(); await sleep(200);
    document.getElementById('settings-backup-btn').click(); await sleep(400);
    const row = document.querySelector('#retention-panel [data-setting-row="retention-clipInbox"]');
    const clean = row && row.querySelectorAll('.import-preview-actions button')[1];
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
    failed += ar_probe('s1', 'screenshot editor: undoing a resize does not unlock operations behind the undo limit', 'index', '1000x780', '', r"""
    await bootReady();
    const cv = document.createElement('canvas'); cv.width = 200; cv.height = 120;
    const g = cv.getContext('2d'); g.fillStyle = '#88aacc'; g.fillRect(0, 0, 200, 120);
    window.__FIRE__('snip-captured', { data_url: cv.toDataURL('image/png'), path: '', name: 'undo.png' });
    await sleep(400);
    const edit = document.getElementById('snip-edit');
    if (edit.classList.contains('hidden')) { errs.push('564-s1: the editor did not open'); }
    else {
      document.querySelector('#snip-edit-tools button[data-tool="pen"]').click(); await sleep(50);
      const live = document.getElementById('snip-edit-live');
      const undo = document.getElementById('snip-edit-undo');
      const r = live.getBoundingClientRect();
      for (let i = 0; i < 52; i++) {
        const x = r.left + 10 + (i % 20) * 3, y = r.top + 10 + (i % 20) * 4;
        live.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: x, clientY: y }));
        live.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: x + 6, clientY: y + 6 }));
        window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: x + 6, clientY: y + 6 }));
        await sleep(20);
      }
      let plain = 0;
      // a plain count of how far undo walks with no resize, for comparison
      document.getElementById('snip-edit-scale').click(); await sleep(150);
      const pop = document.querySelector('.snip-scale-pop');
      const half = [...pop.querySelectorAll('.snip-scale-presets button')].find((b) => b.textContent === '50%');
      half.click(); await sleep(50);
      pop.querySelector('.primary-btn').click(); await sleep(250);
      let steps = 0;
      while (!undo.disabled && steps < 80) { undo.click(); steps++; await sleep(15); }
      // 52 strokes, limit 50: the scale itself plus the 50 newest strokes can be undone
      if (steps !== 51) errs.push('564-s1: undo walked ' + steps + ' steps after a resize, expected 51 (the resize and fifty strokes)');
      const redo = document.getElementById('snip-edit-redo');
      let redos = 0;
      while (!redo.disabled && redos < 80) { redo.click(); redos++; await sleep(15); }
      if (redos !== 51) errs.push('564-s1: redo walked ' + redos + ' steps, expected 51');
      if (undo.disabled) errs.push('564-s1: Undo stayed disabled after redoing the resize');
    }
""")
    failed += ar_probe('d1', 'screenshot dialog: a failed delete keeps the saved path, switching back on saves no second copy', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { snipEditor: false, screenshotSave: true } };</script>\n', r"""
    await bootReady();
    RESULTS.delete_screenshot_file = () => false;
    RESULTS.save_screenshot_now = () => 'C:/shots/second.png';
    const cv = document.createElement('canvas'); cv.width = 40; cv.height = 30;
    window.__FIRE__('snip-captured', { data_url: cv.toDataURL('image/png'), path: 'C:/shots/first.png', name: 'first.png' });
    await sleep(300);
    const modal = document.getElementById('snip-modal');
    const box = document.getElementById('snip-save-folder');
    if (modal.classList.contains('hidden')) errs.push('564-d1: fixture: the result dialog did not open');
    if (!box.checked) errs.push('564-d1: fixture: the folder switch is not on');
    window.__SMOKE_ARGS__.length = 0;
    box.checked = false; box.dispatchEvent(new Event('change', { bubbles: true })); await sleep(200);
    if (!callsFor('delete_screenshot_file').length) errs.push('564-d1: fixture: switching off did not try to delete the file');
    box.checked = true; box.dispatchEvent(new Event('change', { bubbles: true })); await sleep(200);
    const saves = callsFor('save_screenshot_now').length;
    if (saves) errs.push('564-d1: switching back on saved a second copy (' + saves + ' save call) although the first file was never deleted');
""")
    ba_common = r"""
    const BAW = 200, BAH = 120, BGC = [136, 170, 204];
    const loadImg = (u) => new Promise((res) => { const i = new Image(); i.onload = () => res(i); i.src = u; });
    const pixelsOf = async (u) => {
      const i = await loadImg(u);
      const c = document.createElement('canvas'); c.width = i.width; c.height = i.height;
      const g = c.getContext('2d'); g.drawImage(i, 0, 0);
      return { w: i.width, h: i.height, px: (x, y) => Array.from(g.getImageData(x, y, 1, 1).data) };
    };
    const near = (p, c, tol) => [0, 1, 2].every((i) => Math.abs(p[i] - c[i]) <= (tol || 8));
    const captureFixture = () => {
      const cv = document.createElement('canvas'); cv.width = BAW; cv.height = BAH;
      const g = cv.getContext('2d'); g.fillStyle = '#88aacc'; g.fillRect(0, 0, BAW, BAH);
      return cv.toDataURL('image/png');
    };
    const baOpen = async (path) => {
      await bootReady();
      window.__FIRE__('snip-captured', { data_url: captureFixture(), path: path || '', name: 'ba.png' });
      await sleep(400);
      return !document.getElementById('snip-edit').classList.contains('hidden');
    };
    const baTool = async (n) => { document.querySelector('#snip-edit-tools button[data-tool="' + n + '"]').click(); await sleep(60); };
    const baPt = (x, y) => {
      const c = document.getElementById('snip-edit-base'), r = c.getBoundingClientRect();
      const z = r.width / c.width;
      return [r.left + x * z, r.top + y * z];
    };
    const baPtr = (type, x, y, target) => (target || window).dispatchEvent(new PointerEvent(type, {
      bubbles: true, cancelable: true, button: 0, pointerId: 1, isPrimary: true, clientX: x, clientY: y, altKey: true,
    }));
    const baDrag = async (x1, y1, x2, y2) => {
      const live = document.getElementById('snip-edit-live');
      // every position is worked out again after the previous event: a selection can
      // re-render the context bar and shift the canvas
      let a = baPt(x1, y1);
      baPtr('pointerdown', a[0], a[1], live);
      a = baPt((x1 + x2) / 2, (y1 + y2) / 2);
      baPtr('pointermove', a[0], a[1], live);
      a = baPt(x2, y2);
      baPtr('pointermove', a[0], a[1], live);
      baPtr('pointerup', a[0], a[1], window);
      await sleep(40);
    };
    const baClick = async (x, y) => {
      const live = document.getElementById('snip-edit-live');
      const a = baPt(x, y);
      baPtr('pointerdown', a[0], a[1], live);
      baPtr('pointerup', a[0], a[1], window);
      await sleep(40);
    };
    // The crop box starts as the whole image; carving it down is dragging its four edges in.
    const baCropTo = async (x1, y1, x2, y2) => {
      let r = { x: 0, y: 0, w: BAW, h: BAH };
      await baDrag(r.x, r.y + r.h / 2, x1, r.y + r.h / 2);
      r = { x: x1, y: r.y, w: r.w - x1, h: r.h };
      await baDrag(r.x + r.w, r.y + r.h / 2, x2, r.y + r.h / 2);
      r = { x: r.x, y: r.y, w: x2 - r.x, h: r.h };
      await baDrag(r.x + r.w / 2, r.y, r.x + r.w / 2, y1);
      r = { x: r.x, y: y1, w: r.w, h: r.h - y1 };
      await baDrag(r.x + r.w / 2, r.y + r.h, r.x + r.w / 2, y2);
    };
    const baKey = (key, extra) => document.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ key, bubbles: true, cancelable: true }, extra || {})));
    const baExport = async () => {
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('snip-edit-copy').click();
      await sleep(200);
      const c = callsFor('copy_image_data');
      return c.length ? pixelsOf(c[c.length - 1].dataUrl) : null;
    };
    const baScale50 = async () => {
      document.getElementById('snip-edit-scale').click(); await sleep(150);
      const pop = document.querySelector('.snip-scale-pop');
      [...pop.querySelectorAll('.snip-scale-presets button')].find((b) => b.textContent === '50%').click();
      await sleep(50);
      pop.querySelector('.primary-btn').click(); await sleep(250);
    };
    const baUndo = async () => { document.getElementById('snip-edit-undo').click(); await sleep(60); };
    const baRedo = async () => { document.getElementById('snip-edit-redo').click(); await sleep(60); };
    const baCtxPop = () => [...document.body.children].find((e) => e.classList.contains('ctx') && !e.id && !e.classList.contains('hidden'));
    const baCtx = async (x, y) => {
      const a = baPt(x, y);
      document.getElementById('snip-edit-live').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: a[0], clientY: a[1] }));
      await sleep(60);
      return baCtxPop();
    };
"""
    ba_seed = '<script>window.__SEED_SETTINGS__ = { ui_flags: { screenshotSave: true } };</script>\n'
    failed += ar_probe('ba597a', 'screenshot editor: a failed save keeps the unedited file and says so', 'index', '1000x780', ba_seed, ba_common + r"""
    if (!(await baOpen('C:/shots/first.png'))) { errs.push('597: the editor did not open'); }
    else {
      RESULTS.save_screenshot_now = () => null;
      RESULTS.delete_screenshot_file = () => true;
      await baTool('pen'); await baDrag(20, 20, 90, 70);
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('toast').textContent = '';
      document.getElementById('snip-edit-apply').click(); await sleep(500);
      if (!callsFor('save_screenshot_now').length) errs.push('597: fixture: Apply did not try to save the edited shot');
      if (callsFor('delete_screenshot_file').length) errs.push('597: the unedited file was deleted although the edited shot was not saved');
      if (!/could not be saved/i.test(document.getElementById('toast').textContent)) errs.push('597: no error toast after the failed save, toast is ' + JSON.stringify(document.getElementById('toast').textContent));
      if (document.getElementById('snip-save-folder').checked) errs.push('597: the folder switch stays checked after the failed save');
      if (document.getElementById('snip-modal').classList.contains('hidden')) errs.push('597: fixture: the result dialog did not open');
    }
""")
    failed += ar_probe('ba597b', 'screenshot editor: a good save removes the old file only after the new one exists, never the new one itself', 'index', '1000x780', ba_seed, ba_common + r"""
    if (!(await baOpen('C:/shots/first.png'))) { errs.push('597: the editor did not open'); }
    else {
      RESULTS.save_screenshot_now = () => 'C:/shots/second.png';
      RESULTS.delete_screenshot_file = () => true;
      await baTool('pen'); await baDrag(20, 20, 90, 70);
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('snip-edit-apply').click(); await sleep(500);
      const order = (window.__SMOKE_ARGS__ || []).map(([c]) => c).filter((c) => c === 'save_screenshot_now' || c === 'delete_screenshot_file');
      if (order.join() !== 'save_screenshot_now,delete_screenshot_file') errs.push('597: expected save then delete, got ' + order.join());
      const del = callsFor('delete_screenshot_file')[0];
      if (!del || del.path !== 'C:/shots/first.png') errs.push('597: the wrong file was deleted: ' + JSON.stringify(del));
      if (!document.getElementById('snip-save-folder').checked) errs.push('597: the folder switch is off after a good save');
      // the same name comes back (same second): the old path is the new file, it must stay
      document.getElementById('snip-modal-close').click(); await sleep(200);
      window.__FIRE__('snip-captured', { data_url: captureFixture(), path: 'C:/shots/same.png', name: 'same.png' });
      await sleep(400);
      RESULTS.save_screenshot_now = () => 'C:/shots/same.png';
      await baTool('pen'); await baDrag(20, 20, 90, 70);
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('snip-edit-apply').click(); await sleep(500);
      if (callsFor('delete_screenshot_file').length) errs.push('597: the freshly saved file was deleted because it reused the old name');
    }
""")
    failed += ar_probe('ba597c', 'screenshot dialog: switching the folder save on with a failing save says so and puts the switch back off', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { snipEditor: false, screenshotSave: false } };</script>\n', ba_common + r"""
    await bootReady();
    RESULTS.save_screenshot_now = () => null;
    window.__FIRE__('snip-captured', { data_url: captureFixture(), path: '', name: 'x.png' });
    await sleep(300);
    const box = document.getElementById('snip-save-folder');
    if (document.getElementById('snip-modal').classList.contains('hidden')) errs.push('597: fixture: the result dialog did not open');
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('toast').textContent = '';
    box.checked = true; box.dispatchEvent(new Event('change', { bubbles: true })); await sleep(300);
    if (!callsFor('save_screenshot_now').length) errs.push('597: fixture: switching on did not try to save');
    if (box.checked) errs.push('597: the switch stays on although nothing was saved');
    if (!/could not be saved/i.test(document.getElementById('toast').textContent)) errs.push('597: no error toast, toast is ' + JSON.stringify(document.getElementById('toast').textContent));
    const last = callsFor('set_ui_flag').filter((a) => a && a.key === 'screenshotSave').pop();
    if (!last || last.enabled !== false) errs.push('597: the stored setting was left on, last write ' + JSON.stringify(last));
""")
    failed += ar_probe('ba594', 'screenshot editor: moving, resizing or deleting a finished shape is one undo step of its own', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('594: the editor did not open'); }
    else {
      // rows of the arrow (y 30) and of the rectangle border (y 75), sampled in a small band
      const hasInk = (img, x, y) => { for (let d = -3; d <= 3; d++) { if (!near(img.px(x, y + d), BGC, 30)) return true; } return false; };
      await baTool('arrow'); await baDrag(20, 30, 120, 30);
      await baTool('rect'); await baDrag(20, 75, 120, 105);
      await baTool('arrow');
      let img = await baExport();
      if (!img || !hasInk(img, 60, 30) || !hasInk(img, 60, 75)) errs.push('594: fixture: arrow and rectangle are not both drawn');
      // drag the arrow down by 20
      await baDrag(60, 30, 60, 50);
      img = await baExport();
      if (!hasInk(img, 60, 50) || hasInk(img, 60, 22)) errs.push('594: fixture: the drag did not move the arrow');
      await baUndo();
      img = await baExport();
      if (!hasInk(img, 60, 30)) errs.push('594: undo did not bring the moved arrow back to its place');
      if (hasInk(img, 60, 50)) errs.push('594: the arrow is still at the moved place after undo');
      if (!hasInk(img, 60, 75)) errs.push('594: undo removed the rectangle instead of the move');
      await baRedo();
      img = await baExport();
      if (!hasInk(img, 60, 50) || hasInk(img, 60, 22)) errs.push('594: redo did not move the arrow again');
      if (!hasInk(img, 60, 75)) errs.push('594: redo lost the rectangle');
      // resize the rectangle by its corner handle, then undo
      await baTool('rect');
      await baClick(70, 75);
      await baDrag(120, 105, 160, 112);
      img = await baExport();
      if (!hasInk(img, 140, 112)) errs.push('594: fixture: the rectangle was not resized');
      await baUndo();
      img = await baExport();
      if (hasInk(img, 140, 112)) errs.push('594: undo left the rectangle at its enlarged size');
      if (!hasInk(img, 60, 50)) errs.push('594: undoing the resize also undid the earlier arrow move');
      // delete the rectangle through the right-click menu, then undo
      const pop = await baCtx(60, 75);
      if (!pop) { errs.push('594: fixture: no context menu over the rectangle'); }
      else {
        pop.querySelector('button.danger').click(); await sleep(80);
        img = await baExport();
        if (hasInk(img, 60, 75)) errs.push('594: fixture: Delete did not remove the rectangle');
        await baUndo();
        img = await baExport();
        if (!hasInk(img, 60, 75)) errs.push('594: undo did not bring the deleted rectangle back');
        await baRedo();
        img = await baExport();
        if (hasInk(img, 60, 75)) errs.push('594: redo did not delete the rectangle again');
      }
    }
""")
    failed += ar_probe('ba592a', 'screenshot editor: undoing a committed crop clears the crop box, export is the full image', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('592: the editor did not open'); }
    else {
      await baTool('crop'); await baCropTo(60, 30, 140, 90);
      baKey('Enter'); await sleep(80);
      let img = await baExport();
      if (!img || img.w !== 80 || img.h !== 60) errs.push('592: fixture: the crop did not export 80x60, got ' + (img ? img.w + 'x' + img.h : 'nothing'));
      // each dragged edge is one crop step
      for (let i = 0; i < 4; i++) await baUndo();
      img = await baExport();
      if (!img || img.w !== BAW || img.h !== BAH) errs.push('592: after undo the export is ' + (img ? img.w + 'x' + img.h : 'nothing') + ', expected the full ' + BAW + 'x' + BAH);
      if (!document.getElementById('snip-edit-undo').disabled) errs.push('592: exporting pushed the undone crop back onto the undo stack');
      for (let i = 0; i < 4; i++) await baRedo();
      img = await baExport();
      if (!img || img.w !== 80 || img.h !== 60) errs.push('592: redo did not bring the crop back, export is ' + (img ? img.w + 'x' + img.h : 'nothing'));
    }
""")
    failed += ar_probe('ba592b', 'screenshot editor: a resize keeps the crop box and the exported crop in step', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('592: the editor did not open'); }
    else {
      await baTool('crop'); await baCropTo(100, 60, 180, 110);
      await baScale50();
      const img = await baExport();
      if (!img || img.w !== 40 || img.h !== 25) errs.push('592: after a 50 percent resize the crop should export 40x25, got ' + (img ? img.w + 'x' + img.h : 'nothing'));
    }
""")
    failed += ar_probe('ba596', 'screenshot editor: a resize with the Original locked on keeps every annotation', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('596: the editor did not open'); }
    else {
      const hasInk = (img, x, y) => { for (let d = -3; d <= 3; d++) { if (!near(img.px(x, y + d), BGC, 30)) return true; } return false; };
      await baTool('arrow'); await baDrag(20, 30, 120, 30);
      const orig = document.getElementById('snip-edit-original');
      orig.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 1 }));
      document.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1 }));
      await sleep(80);
      if (!orig.classList.contains('active')) errs.push('596: fixture: a tap on Original did not lock it on');
      await baScale50();
      let img = await baExport();
      if (!img || img.w !== 100 || img.h !== 60) errs.push('596: fixture: the export is not 100x60 after the resize');
      else if (!hasInk(img, 50, 15)) errs.push('596: the arrow is gone after resizing while Original was locked on');
      await baUndo();
      img = await baExport();
      if (!img || img.w !== BAW || !hasInk(img, 100, 30)) errs.push('596: undoing the resize did not bring the arrow back at full size');
    }
""")
    failed += ar_probe('ba593', 'screenshot editor: a resize with a crop set does not bake the crop dimming into the image', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('593: the editor did not open'); }
    else {
      await baTool('crop'); await baCropTo(60, 30, 140, 90);
      baKey('Enter'); await sleep(80);
      await baTool('arrow');
      await baScale50();
      await baTool('crop');
      baKey('Escape'); await sleep(80);
      const img = await baExport();
      if (!img || img.w !== 100 || img.h !== 60) errs.push('593: fixture: expected the full 100x60 image, got ' + (img ? img.w + 'x' + img.h : 'nothing'));
      else {
        for (const [x, y] of [[0, 0], [99, 59], [5, 50]]) {
          if (!near(img.px(x, y), BGC, 6)) errs.push('593: pixel ' + x + ',' + y + ' is ' + JSON.stringify(img.px(x, y)) + ', the capture is ' + JSON.stringify(BGC) + ' (the crop dimming was baked in)');
        }
      }
    }
""")
    failed += ar_probe('ba595', 'screenshot editor: step numbers go on counting after a resize', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('595: the editor did not open'); }
    else {
      await baTool('step');
      await baClick(30, 100); await baClick(60, 100); await baClick(90, 100);
      await baScale50();
      const drawn = [];
      const orig = CanvasRenderingContext2D.prototype.fillText;
      CanvasRenderingContext2D.prototype.fillText = function (text) { drawn.push(String(text)); return orig.apply(this, arguments); };
      await baClick(40, 50);
      CanvasRenderingContext2D.prototype.fillText = orig;
      if (drawn.join() !== '4') errs.push('595: the marker placed after the resize reads ' + JSON.stringify(drawn) + ', expected 4');
      // a second resize keeps the count as well
      await baScale50();
      drawn.length = 0;
      CanvasRenderingContext2D.prototype.fillText = function (text) { drawn.push(String(text)); return orig.apply(this, arguments); };
      await baClick(20, 25);
      CanvasRenderingContext2D.prototype.fillText = orig;
      if (drawn.join() !== '5') errs.push('595: after a second resize the next marker reads ' + JSON.stringify(drawn) + ', expected 5');
    }
""")
    failed += ar_probe('ba598', 'screenshot editor: a spotlight offers no Duplicate, so two never dim each other', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('598: the editor did not open'); }
    else {
      await baTool('spotlight'); await baDrag(40, 30, 120, 80);
      let img = await baExport();
      if (!img || near(img.px(5, 5), BGC, 8) || !near(img.px(42, 32), BGC, 8)) errs.push('598: fixture: the spotlight does not dim the outside and keep the inside clear');
      const pop = await baCtx(80, 55);
      if (!pop) { errs.push('598: fixture: no context menu over the spotlight'); }
      else {
        const shown = [...pop.querySelectorAll('button')].filter((b) => !b.classList.contains('hidden') && b.offsetParent !== null);
        if (shown.length !== 1 || !shown[0].classList.contains('danger')) errs.push('598: the spotlight menu offers ' + JSON.stringify(shown.map((b) => b.textContent)) + ', expected only Delete');
      }
      // a rectangle keeps both entries
      document.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      await baTool('rect'); await baDrag(20, 100, 60, 115);
      const pop2 = await baCtx(40, 100);
      const shown2 = pop2 ? [...pop2.querySelectorAll('button')].filter((b) => b.offsetParent !== null) : [];
      if (shown2.length !== 2) errs.push('598: fixture: a rectangle menu shows ' + shown2.length + ' entries, expected Duplicate and Delete');
    }
""")
    failed += ar_probe('ba599', 'screenshot editor: a capture that arrives while Apply saves keeps its own file and state', 'index', '1000x780', ba_seed, ba_common + r"""
    if (!(await baOpen('C:/shots/first.png'))) { errs.push('599: the editor did not open'); }
    else {
      let release = null;
      RESULTS.save_screenshot_now = () => new Promise((res) => { release = () => res('C:/shots/e1.png'); });
      RESULTS.delete_screenshot_file = () => true;
      await baTool('pen'); await baDrag(20, 20, 90, 70);
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('snip-edit-apply').click(); await sleep(150);
      // a hotkey capture lands while the re-encode is still running
      window.__FIRE__('snip-captured', { data_url: captureFixture(), path: 'C:/shots/new.png', name: 'new.png' });
      await sleep(400);
      if (!release) { errs.push('599: fixture: the save never started'); }
      else {
        release(); await sleep(300);
        const dels = callsFor('delete_screenshot_file').map((a) => a.path);
        if (dels.includes('C:/shots/new.png')) errs.push('599: the new capture file was deleted: ' + dels.join());
        if (!dels.includes('C:/shots/first.png')) errs.push('599: the replaced unedited file was not removed: ' + dels.join());
        if (!document.getElementById('snip-modal').classList.contains('hidden')) errs.push('599: the stale apply opened the result dialog over the new capture');
        // applying the new capture must delete its own file, never the first edit's file
        RESULTS.save_screenshot_now = () => 'C:/shots/e2.png';
        await baTool('pen'); await baDrag(20, 20, 90, 70);
        window.__SMOKE_ARGS__.length = 0;
        document.getElementById('snip-edit-apply').click(); await sleep(400);
        const dels2 = callsFor('delete_screenshot_file').map((a) => a.path);
        if (dels2.join() !== 'C:/shots/new.png') errs.push('599: applying the new capture deleted ' + JSON.stringify(dels2) + ', expected only its own file');
      }
    }
""")
    failed += ar_probe('ba600', 'screenshot editor: undoing a delete puts the shape back in its place even after a crop changed the list', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('600: the editor did not open'); }
    else {
      const baText = async (x, y, word) => {
        await baClick(x, y);
        const ti = document.getElementById('snip-edit-text-input');
        ti.value = word;
        ti.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
        await sleep(60);
      };
      await baTool('crop'); await baCropTo(10, 10, 190, 110);
      baKey('Enter'); await sleep(80);
      await baTool('text');
      await baText(30, 90, 'AAA'); await baText(120, 90, 'BBB');
      const pop = await baCtx(32, 94);
      const del = pop && pop.querySelector('button.danger');
      if (!del) { errs.push('600: fixture: no delete entry over the first text'); }
      else {
        del.click(); await sleep(80);
        // discarding the crop drops it from the list, every index behind it moves
        await baTool('crop');
        baKey('Escape'); await sleep(80);
        const drawn = [];
        const orig = CanvasRenderingContext2D.prototype.fillText;
        CanvasRenderingContext2D.prototype.fillText = function (text) { drawn.push(String(text)); return orig.apply(this, arguments); };
        await baUndo();
        CanvasRenderingContext2D.prototype.fillText = orig;
        if (drawn.join() !== 'AAA,BBB') errs.push('600: the texts are drawn in the order ' + JSON.stringify(drawn) + ', the restored one must come first');
      }
    }
""")
    failed += ar_probe('ba601', 'screenshot editor: a run of arrow-key nudges is one undo step and redo keeps it', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('601: the editor did not open'); }
    else {
      const hasInk = (img, x, y) => { for (let d = -3; d <= 3; d++) { if (!near(img.px(x + d, y), BGC, 30)) return true; } return false; };
      await baTool('rect'); await baDrag(20, 75, 120, 105);
      let img = await baExport();
      if (!hasInk(img, 20, 90)) errs.push('601: fixture: the rectangle is not drawn');
      baKey('ArrowRight', { shiftKey: true }); baKey('ArrowRight', { shiftKey: true }); await sleep(60);
      img = await baExport();
      if (hasInk(img, 20, 90)) errs.push('601: fixture: the nudges did not move the rectangle');
      await baUndo();
      img = await baExport();
      if (!hasInk(img, 20, 90)) errs.push('601: undo did not take the nudges back');
      if (!hasInk(img, 60, 75)) errs.push('601: undo removed the rectangle, the nudges were more than one step');
      await baRedo();
      img = await baExport();
      if (hasInk(img, 20, 90)) errs.push('601: redo dropped the nudges');
      await baUndo(); await baUndo();
      img = await baExport();
      if (hasInk(img, 60, 75)) errs.push('601: the second undo did not remove the rectangle');
    }
""")
    failed += ar_probe('bl649', 'screenshot editor: a tool starts with the size it was last used at, and a change is saved', 'index', '1000x780', r"""<script>window.__SEED_SETTINGS__ = { ui_texts: { snipToolSizes: '{"pen":{"width":10},"marker":{"width":999},"rect":"junk"}' } };</script>
""", ba_common + r"""
    if (!(await baOpen(''))) { errs.push('649: the editor did not open'); }
    else {
      const widthOf = () => { const i = document.querySelector('#snip-edit-context .snip-edit-field input'); return i ? i.value : '(no field)'; };
      await baTool('pen');
      if (widthOf() !== '10') errs.push('649: pen starts at ' + widthOf() + ', the remembered size is 10');
      await baTool('marker');
      if (widthOf() !== '18') errs.push('649: marker with an out-of-range remembered size starts at ' + widthOf() + ', expected 18');
      await baTool('rect');
      if (widthOf() !== '4') errs.push('649: rect with a broken entry starts at ' + widthOf() + ', expected 4');
      await baTool('line');
      if (widthOf() !== '4') errs.push('649: line with nothing remembered starts at ' + widthOf() + ', expected 4');
      window.__SMOKE_ARGS__ = [];
      document.querySelector('#snip-edit-context .snip-edit-width-btn:last-child').click();
      await sleep(700);
      const saved = (window.__SMOKE_ARGS__ || []).filter(([cmd, a]) => cmd === 'set_ui_text' && a && a.key === 'snipToolSizes');
      const last = saved.length ? saved[saved.length - 1][1].value : '';
      let parsed = null;
      try { parsed = JSON.parse(last); } catch (e) { parsed = null; }
      if (!parsed || !parsed.line || parsed.line.width !== 8) errs.push('649: choosing the large width did not save line width 8: ' + last);
      else if (!parsed.pen || parsed.pen.width !== 10) errs.push('649: saving line dropped the remembered pen size: ' + last);
    }
""")
    failed += ar_probe('bz978', 'screenshot editor: changing the pen size moves nothing, rebuilds nothing and keeps the focus', 'index', '1000x780', '', ba_common + r"""
    // 978: a size change updates the panel where it stands.
    if (!(await baOpen(''))) { errs.push('978: the editor did not open'); }
    else {
      await baTool('pen');
      const panel = document.getElementById('snip-edit-context');
      const field = () => panel.querySelector('.snip-edit-field input');
      const rectOf = (el) => { const r = el.getBoundingClientRect(); return [r.left, r.top, r.width, r.height].map((v) => Math.round(v * 10) / 10).join(',') };
      const before = { panel: rectOf(panel), input: field(), win: [window.innerWidth, window.innerHeight].join('x'), firstChild: panel.firstElementChild };
      const sizeAfter = [];
      const presets = [...panel.querySelectorAll('.snip-edit-width-btn')];
      for (let i = 0; i < 10; i++) {
        if (i % 2 === 0) {
          const b = presets[i % 3];
          b.focus(); b.click(); await sleep(30);
          if (document.activeElement !== b) errs.push('978: clicking a size button moved the focus (round ' + i + ')');
        } else {
          const f = field(); f.focus(); f.value = String(3 + i * 2);
          f.dispatchEvent(new Event('change', { bubbles: true })); await sleep(30);
          if (document.activeElement !== f) errs.push('978: typing a size moved the focus (round ' + i + ')');
        }
        if (field() !== before.input) errs.push('978: the width field was rebuilt in round ' + i);
        if (panel.firstElementChild !== before.firstChild) errs.push('978: the panel content was rebuilt in round ' + i);
        if (rectOf(panel) !== before.panel) errs.push('978: the panel rect changed in round ' + i + ': ' + before.panel + ' -> ' + rectOf(panel));
        sizeAfter.push(field().value);
      }
      if (window.innerWidth + 'x' + window.innerHeight !== before.win) errs.push('978: the window size changed');
      presets[2].click(); await sleep(30);
      if (field().value !== '14') errs.push('978: the large button left the field at ' + field().value + ', expected 14');
      if (!presets[2].classList.contains('sel') || presets[0].classList.contains('sel') || presets[1].classList.contains('sel')) errs.push('978: the selected size button does not follow the large button');
      const f = field(); f.value = '6'; f.dispatchEvent(new Event('change', { bubbles: true })); await sleep(30);
      if (!presets[1].classList.contains('sel') || presets[2].classList.contains('sel')) errs.push('978: typing the medium size does not select the medium button');
      f.value = '9'; f.dispatchEvent(new Event('change', { bubbles: true })); await sleep(30);
      if (presets.some((b) => b.classList.contains('sel'))) errs.push('978: a size between the presets leaves a button selected');
      if (getComputedStyle(panel).contain.indexOf('layout') < 0) errs.push('978: the panel has no layout containment: ' + getComputedStyle(panel).contain);
      if (panel.getBoundingClientRect().width < 240) errs.push('978: the panel is narrower than its minimum width');
    }
""")
    failed += ar_probe('bl646', 'update interval: a stored 0 hours with Auto update on shows the shortest interval, never Never', 'index', '1000x780', r"""<script>window.__SEED_SETTINGS__ = { auto_update: true, ui_values: { updateCheckH: 0 } };</script>
""", r"""
    await bootReady();
    document.getElementById('gear').click();
    await sleep(200);
    const sw = document.getElementById('opt-autoupdate');
    const sel = document.getElementById('opt-update-interval');
    if (!sel) errs.push('646: the interval select is missing');
    else {
      if (!sw.checked) errs.push('646: Auto update should be on');
      if (sel.value === '0') errs.push('646: a stored 0 hours with Auto update on shows Never');
      if (sel.value !== '1') errs.push('646: a stored 0 hours should show the 1 hour the backend uses, got ' + sel.value);
    }
""")
    failed += ar_probe('bm654', 'usage statistics: a fresh install shows the switch off and no counts, switching it on stores true and shows them', 'index', '1000x780', r"""<script>window.__SEED_SETTINGS__ = { ui_flags: { usageStats: undefined } };</script>
""", r"""
    await bootReady();
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('settings-stats-btn').click();
    await sleep(600);
    const swi = () => document.querySelector('#stats-panel .stats-head-row .switch');
    if (!swi()) errs.push('654: the statistics switch is missing');
    else {
      if (swi().checked) errs.push('654: a fresh install shows the statistics switch on');
      if (document.querySelector('#stats-panel .stat-cards')) errs.push('654: statistics cards show while the switch is off');
      window.__SMOKE_ARGS__ = [];
      swi().click();
      await sleep(600);
      const calls = (window.__SMOKE_ARGS__ || []).filter(([cmd, a]) => cmd === 'set_ui_flag' && a && a.key === 'usageStats');
      if (!calls.length || calls[calls.length - 1][1].enabled !== true) errs.push('654: switching statistics on did not store usageStats true');
      if (!swi() || !swi().checked) errs.push('654: the switch is not on after switching it on');
      if (!document.querySelector('#stats-panel .stat-cards')) errs.push('654: no statistics cards after switching on');
    }
""")
    failed += ar_probe('bm908', 'hotkey fields: a small X inside the field removes the shortcut, no Clear button', 'index', '880x600', '', r"""
    // 908: the X shows while a shortcut is set and removes only the shortcut.
    await bootReady();
    document.getElementById('gear').click();
    await sleep(300);
    const hk = document.getElementById('opt-hotkey');
    const x = document.getElementById('hotkey-clear');
    if (!x || !hk.parentElement.classList.contains('hotkey-clear-wrap') || x.parentElement !== hk.parentElement) {
      errs.push('908: the global hotkey X is not inside the field wrapper');
    } else {
      if (!hk.value) errs.push('908: fixture: the global hotkey field is empty');
      x.style.transition = 'none'; // headless frames stall, a running fade would read as a half value
      if (Number(getComputedStyle(x).opacity) <= 0) errs.push('908: the X is invisible while a shortcut is set');
      if (x.textContent.trim()) errs.push('908: the X carries text instead of an icon');
      window.__SMOKE_ARGS__ = [];
      x.click();
      await sleep(400);
      const call = callsFor('set_hotkey').pop();
      if (!call || call.hotkey !== '') errs.push('908: clicking the X did not store an empty hotkey');
      if (hk.value) errs.push('908: the field still holds a shortcut after the X');
      if (Number(getComputedStyle(x).opacity) !== 0) errs.push('908: the X stays visible after the shortcut is gone');
    }
    const mx = document.getElementById('modal-hotkey-clear');
    const mi = document.getElementById('modal-hotkey-input');
    if (!mx || mx.parentElement !== mi.parentElement || !mi.parentElement.classList.contains('hotkey-clear-wrap')) {
      errs.push('908: the editor hotkey X is not inside the field wrapper');
    } else {
      mx.style.transition = 'none';
      mi.value = '';
      if (Number(getComputedStyle(mx).opacity) !== 0) errs.push('908: the editor X shows on an empty field');
      mi.value = 'Ctrl + 9';
      if (Number(getComputedStyle(mx).opacity) <= 0) errs.push('908: the editor X is hidden while a shortcut is set');
    }
    if (document.querySelector('.hotkey-row button.ghost-btn[id$="-clear"]')) errs.push('908: a text Clear button is still in the markup');
""")
    failed += ar_probe('bm909', 'settings: switches in two columns (one below 440px), screenshot saving in its own Capture section', 'index', '880x600', '', r"""
    // 909: two switch columns (one when the dialog is narrower than 440px), the capture switch in a section of its own.
    await bootReady();
    document.getElementById('gear').click();
    await sleep(300);
    const grids = [...document.querySelectorAll('#settings-main .switch-grid')];
    if (!grids.length) errs.push('909: fixture: no switch grid in the settings');
    for (const g of grids) {
      const tracks = getComputedStyle(g).gridTemplateColumns.split(' ').filter(Boolean).length;
      const want = document.querySelector('#settings .modal').clientWidth < 440 ? 1 : 2;
      if (tracks !== want) errs.push('909: a switch grid has ' + tracks + ' columns, expected ' + want);
    }
    const cb = document.getElementById('opt-screenshot-folder');
    const sec = cb && cb.closest('section');
    if (!sec || sec.id !== 'settings-sec-capture') errs.push('909: the screenshot switch is not in the Capture section');
    else {
      const r = sec.getBoundingClientRect();
      if (!r.width || !r.height) errs.push('909: the Capture section is not visible');
      if (!sec.querySelector('.settings-sec-title').textContent.trim()) errs.push('909: the Capture section has no title');
    }
""")
    failed += ar_probe('bm910', 'settings: one Import button opens a menu with a file and a folder entry', 'index', '880x600', '', r"""
    // 910: the Import button opens a small menu instead of two buttons.
    await bootReady();
    document.getElementById('gear').click();
    await sleep(300);
    const ib = document.getElementById('import-btn');
    const pop = document.getElementById('import-menu-pop');
    if (document.getElementById('import-folder-btn').closest('#import-menu-pop') !== pop) errs.push('910: the folder entry is not in the menu');
    ib.click();
    await sleep(200);
    if (pop.classList.contains('hidden')) errs.push('910: the Import button did not open its menu');
    if (pop.querySelectorAll('[role=menuitem]').length !== 2) errs.push('910: the import menu does not hold 2 entries');
    if (ib.getAttribute('aria-expanded') !== 'true') errs.push('910: the Import button does not report the open menu');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await sleep(150);
    if (!pop.classList.contains('hidden')) errs.push('910: Escape did not close the import menu');
    if (document.getElementById('settings').classList.contains('hidden')) errs.push('910: Escape closed the settings along with the menu');
    ib.click();
    await sleep(200);
    window.__SMOKE_ARGS__ = [];
    document.getElementById('import-file-btn').click();
    await sleep(300);
    if (!callsFor('preview_import').length) errs.push('910: From file did not start the import preview');
    if (!pop.classList.contains('hidden')) errs.push('910: choosing an entry left the menu open');
""")
    failed += ar_probe('bm912', 'settings: the expert menu button has a normal size and closes the System line', 'index', '880x600', '', r"""
    // 912: the button is one short primary button at the right end of the System line.
    await bootReady();
    document.getElementById('gear').click();
    await sleep(300);
    const eo = document.getElementById('expert-open');
    const r = eo.getBoundingClientRect();
    if (!(r.width > 0)) errs.push('912: the expert menu button is not visible');
    if (r.width >= 260) errs.push('912: the expert menu button is ' + Math.round(r.width) + 'px wide, expected under 260');
    if (!eo.classList.contains('primary-btn')) errs.push('912: the expert menu button is not primary-styled');
    if (eo !== eo.parentElement.lastElementChild) errs.push('912: the expert menu button is not the last item of the System line');
    if (eo.scrollWidth > eo.clientWidth + 1) errs.push('912: the expert menu label is cut');
""")
    failed += ar_probe('bm913', 'settings: the update interval sits beside the Auto update switch and shows only while it is on', 'index', '880x600', '', r"""
    // 913: the interval list is hidden while Auto update is off and hugs the switch while on.
    await bootReady();
    document.getElementById('gear').click();
    await sleep(300);
    const sw = document.getElementById('opt-autoupdate');
    const sel = document.getElementById('opt-update-interval');
    const wrap = sel.closest('.update-interval');
    const isShown = () => wrap.getBoundingClientRect().width > 0 && !wrap.classList.contains('hidden');
    if (!sw.checked) errs.push('913: fixture: Auto update should start on');
    if (!isShown()) errs.push('913: the interval is not visible while Auto update is on');
    if ([...sel.options].some((o) => o.value === '0')) errs.push('913: the interval list still has a 0 entry');
    const lab = sw.closest('label').getBoundingClientRect();
    const gap = wrap.getBoundingClientRect().left - lab.right;
    if (gap < 0 || gap > 16) errs.push('913: the interval sits ' + Math.round(gap) + 'px from the switch, expected within 16');
    sw.checked = false;
    sw.dispatchEvent(new Event('change', { bubbles: true }));
    await sleep(200);
    if (!wrap.classList.contains('hidden')) errs.push('913: the interval stays visible while Auto update is off');
    sw.checked = true;
    sw.dispatchEvent(new Event('change', { bubbles: true }));
    await sleep(200);
    if (wrap.classList.contains('hidden')) errs.push('913: the interval stays hidden after switching Auto update on');
    const lab2 = sw.closest('label').getBoundingClientRect();
    const gap2 = wrap.getBoundingClientRect().left - lab2.right;
    if (gap2 < 0 || gap2 > 16) errs.push('913: after switching on, the interval sits ' + Math.round(gap2) + 'px from the switch');
""")
    failed += ar_probe('bm914', 'settings: the last theme entry opens the editor, keeps the theme and stores nothing', 'index', '880x600', '', r"""
    // 914: "Custom theme..." at the end of the theme list opens the editor.
    await bootReady();
    document.getElementById('gear').click();
    await sleep(300);
    const sel = document.getElementById('theme-select');
    if (document.querySelector('.theme-field button')) errs.push('914: a button is still beside the theme select');
    const last = sel.options[sel.options.length - 1];
    if (!last || last.value !== '__theme-editor') errs.push('914: the last theme option is not the editor entry');
    if (!last.textContent.trim()) errs.push('914: the editor entry has no text');
    const before = sel.value;
    window.__SMOKE_ARGS__ = [];
    sel.value = '__theme-editor';
    sel.dispatchEvent(new Event('change', { bubbles: true }));
    await sleep(400);
    if (document.getElementById('theme-modal').classList.contains('hidden')) errs.push('914: picking the entry did not open the theme editor');
    if (sel.value !== before) errs.push('914: the theme select shows "' + sel.value + '" after the entry, expected "' + before + '"');
    if (callsFor('set_theme').length) errs.push('914: picking the entry stored a theme');
""")
    failed += ar_probe('bm914b', 'settings: with custom themes off the theme list has no editor entry', 'index', '880x600', r"""<script>window.__SEED_SETTINGS__ = { ui_flags: { customThemes: false } };</script>
""", r"""
    await bootReady();
    document.getElementById('gear').click();
    await sleep(300);
    const sel = document.getElementById('theme-select');
    if ([...sel.options].some((o) => o.value === '__theme-editor')) errs.push('914: the editor entry shows although custom themes are off');
""")
    failed += ar_probe('bm916', 'icon size: gone from the settings page, a slider in the expert menu sets the icon zoom', 'index', '880x600', '', r"""
    // 916: the icon size lives in the expert menu, Appearance tab, group Sizes.
    await bootReady();
    document.getElementById('gear').click();
    await sleep(300);
    if ([...document.querySelectorAll('#settings-main select')].some((sl) => sl.id.startsWith('icon-'))) errs.push('916: an icon size select is still on the settings page');
    document.getElementById('expert-open').click();
    await sleep(300);
    let row = null;
    for (const tb of document.querySelectorAll('#expert-tabs-bar .expert-tab')) {
      tb.click();
      await sleep(120);
      row = document.querySelector('#expert-flags [data-setting-row="iconScale"]');
      if (row) break;
    }
    if (!row) errs.push('916: the icon size row is not in the expert menu');
    else {
      const slider = row.querySelector('input[type=range]');
      if (!slider) errs.push('916: the icon size row has no slider');
      else {
        slider.value = '150';
        slider.dispatchEvent(new Event('input', { bubbles: true }));
        slider.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(300);
        const z = document.documentElement.style.getPropertyValue('--icon-zoom');
        if (Number(z) !== 1.5) errs.push('916: --icon-zoom is "' + z + '" after moving the slider to 150, expected 1.5');
      }
    }
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
    # The statistics panel keeps its blocks apart: the switch row, the export row and the cards.
    stats_gap_body = r"""
    await bootReady();
    document.getElementById('gear').click();
    await sleep(200);
    document.getElementById('settings-stats-btn').click();
    await sleep(600);
    const head = document.querySelector('#stats-panel .stats-head-row');
    const exp = document.querySelector('#stats-panel .stats-export-row');
    const cards = document.querySelector('#stats-panel .stat-cards');
    if (!head || !exp || !cards) errs.push('823: a block of the statistics panel is missing (fixture)');
    else {
      const g1 = exp.getBoundingClientRect().top - head.getBoundingClientRect().bottom;
      const g2 = cards.getBoundingClientRect().top - exp.getBoundingClientRect().bottom;
      if (g1 < 10) errs.push('823: the switch row and the export row are ' + g1 + ' px apart');
      if (g2 < 10) errs.push('823: the export row and the cards are ' + g2 + ' px apart');
    }
"""
    for size_823 in ('880x600', '680x460'):
        failed += ar_probe('bs823-' + size_823, '823: statistics panel blocks keep a gap at ' + size_823, 'index', size_823, '<script>window.__SEED_SETTINGS__ = { ui_flags: { usageStats: true } };</script>\n', stats_gap_body)
    # The help dialog's close button sits at the same inset as the settings dialog's.
    help_close_body = r"""
    await bootReady();
    const inset = (id, close) => {
      const m = document.querySelector('#' + id + ' .modal').getBoundingClientRect();
      return Math.round(m.right - document.getElementById(close).getBoundingClientRect().right);
    };
    document.getElementById('gear').click();
    await sleep(300);
    const setIn = inset('settings', 'settings-close');
    document.getElementById('help-open').click();
    await sleep(400);
    const helpIn = inset('help', 'help-close');
    if (Math.abs(setIn - helpIn) > 1) errs.push('824: the help close button sits ' + helpIn + ' px from the dialog edge, the settings one ' + setIn);
"""
    for size_824 in ('880x600', '680x460'):
        failed += ar_probe('bh824-' + size_824, '824: help close button inset matches settings at ' + size_824, 'index', size_824, '', help_close_body)
    # The folder tile shows how many tiles it holds and one colour dot per tile.
    failed += ar_probe('bf250', '250: the folder tile shows the member count and one dot per tile', 'index', '880x600', '', r"""
    await bootReady();
    const gf = await import('./gridFolders.js');
    const real = document.querySelector('#grid .folder-tile');
    if (!real) errs.push('250: no folder tile on the grid (fixture)');
    else {
      const mini = real.querySelector('.folder-mini');
      const title = real.querySelector('.folder-title');
      const cs = getComputedStyle(real);
      const tr = real.getBoundingClientRect();
      const top = tr.top + parseFloat(cs.borderTopWidth) + parseFloat(cs.paddingTop);
      const mr = mini.getBoundingClientRect();
      const room = title.getBoundingClientRect().top - top;
      if (mr.height / room < 0.75) errs.push('250: the preview fills only ' + Math.round(mr.height / room * 100) + ' percent of the room above the caption');
      if (mini.querySelector('.folder-count').textContent !== '5') errs.push('250: five members should show the count 5');
    }
    const build = (n, w, h) => {
      const box = document.createElement('div');
      box.style.cssText = 'position:fixed;left:0;top:0;z-index:99999;width:' + w + 'px;height:' + h + 'px';
      const ids = ['p1', 'p2', 'p3', 'p5', 'p6', 'p7', 'p8', 'p9', 'p10'].slice(0, n);
      const tile = gf.buildFolderTile({ id: 'tfx', name: 'X', color: '', view_id: 'home', cell: [0, 0], members: ids });
      box.appendChild(tile);
      document.body.appendChild(box);
      const mini = tile.querySelector('.folder-mini');
      const res = {
        count: tile.querySelector('.folder-count').textContent,
        dots: tile.querySelectorAll('.folder-dot').length,
        parts: mini.children.length,
        label: mini.getAttribute('aria-label'),
        inside: mini.getBoundingClientRect().bottom <= tile.querySelector('.folder-title').getBoundingClientRect().top + 1 &&
          mini.getBoundingClientRect().top >= tile.getBoundingClientRect().top - 1,
      };
      box.remove();
      return res;
    };
    for (const n of [1, 2, 4, 6, 9]) {
      for (const [w, h] of [[207, 148], [260, 150]]) {
        const r = build(n, w, h);
        if (r.count !== String(n)) errs.push('250: ' + n + ' members show the count ' + r.count);
        if (r.dots !== Math.min(n, 8)) errs.push('250: ' + n + ' members show ' + r.dots + ' dots, want ' + Math.min(n, 8));
        if (r.parts !== 2) errs.push('250: the preview holds ' + r.parts + ' parts, want the count and the dots only');
        if (!r.label || r.label.indexOf(String(n)) < 0) errs.push('250: the preview label does not name the count ' + n + ': ' + r.label);
        if (!r.inside) errs.push('250: ' + n + ' members: the preview is not inside the tile above the title at ' + w + 'x' + h);
      }
    }
""")
    # The open grid folder dims the grid, has a title row with rename and close, and fits its rows.
    fpop_body = r"""
    await bootReady();
    const folder = document.querySelector('#grid .folder-tile');
    if (!folder) errs.push('251: no folder tile on the grid (fixture)');
    else {
      const openPop = async () => {
        ptr('pointerdown', 5, 5, folder);
        ptr('pointerup', 5, 5, window);
        await sleep(450);
      };
      const popEl = () => document.querySelector('.tile-folder-pop:not(.hidden)');
      const bd = () => document.querySelector('.tile-folder-backdrop');
      const shown = (el) => !!el && getComputedStyle(el).display !== 'none';
      await openPop();
      const pop = popEl();
      if (!pop) errs.push('251: the folder did not open');
      else {
        if (!shown(bd())) errs.push('251: the open folder has no visible backdrop');
        const pr = pop.getBoundingClientRect();
        const members = [...pop.querySelectorAll('.folder-member-tile')];
        if (members.length !== 5) errs.push('251: expected 5 members, found ' + members.length);
        for (const m of members) {
          const r = m.getBoundingClientRect();
          if (r.left < -1 || r.top < -1 || r.right > innerWidth + 1 || r.bottom > innerHeight + 1) errs.push('251: a member leaves the window: ' + JSON.stringify([r.left, r.top, r.right, r.bottom]));
          if (r.left < pr.left - 1 || r.top < pr.top - 1 || r.right > pr.right + 1 || r.bottom > pr.bottom + 1) errs.push('251: a member leaves the popover');
        }
        if (pop.scrollHeight > pop.clientHeight + 1) errs.push('251: the popover scrolls (' + pop.scrollHeight + ' > ' + pop.clientHeight + ')');
        const btns = [...pop.querySelectorAll('.tile-folder-pop-head button')];
        if (btns.length !== 2) errs.push('251: the title row has ' + btns.length + ' buttons, want rename and close');
        else {
          for (const b of btns) if (!(b.getAttribute('aria-label') || '').trim() || !(b.title || '').trim()) errs.push('251: a title row button has no accessible name');
          if (!pop.querySelector('.tile-folder-pop-title')) errs.push('251: no title element');
          btns[1].click();
          await sleep(150);
          if (popEl() || shown(bd())) errs.push('251: the close button left the popover or backdrop visible');
          await openPop();
          if (!popEl()) errs.push('251: the folder did not reopen');
          else {
            ptr('pointerdown', 5, 5, bd());
            await sleep(150);
            if (popEl() || shown(bd())) errs.push('251: a press on the backdrop left the popover or backdrop visible');
          }
        }
        await openPop();
        const member = popEl() && popEl().querySelector('.folder-member-tile[data-id="p6"]');
        const free = [...document.querySelectorAll('#grid .cell')].filter((c) => !c.querySelector('.tile')).map((c) => ({ c, r: c.getBoundingClientRect() }))
          .find((x) => x.r.left > 0 && x.r.top > 0 && x.r.right < innerWidth && x.r.bottom < innerHeight);
        if (!member || !free) errs.push('251: no member or free cell for the drag-out (fixture)');
        else {
          window.__SMOKE_ARGS__.length = 0;
          const [sx, sy] = centre(member);
          const [tx, ty] = centre(free.c);
          ptr('pointerdown', sx, sy, member);
          ptr('pointermove', sx + 8, sy - 8, member);
          await sleep(60);
          ptr('pointermove', tx, ty, window);
          await sleep(60);
          ptr('pointerup', tx, ty, window);
          await sleep(300);
          if (callsFor('take_out_of_tile_folder').length !== 1) errs.push('251: dragging a member over a free cell did not take it out of the folder');
        }
      }
    }
"""
    for size_251 in ('880x600', '680x460'):
        failed += ar_probe('fpop251-' + size_251, '251: the open grid folder is dimmed, titled and fits at ' + size_251, 'index', size_251, '', fpop_body)
    # The tile menu groups hiding and view placement under one "Views..." entry.
    views_menu_body = r"""
    await bootReady();
    const coreC = await import('./core.js');
    const menuAt = async (id) => {
      tileEl(id).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await sleep(150);
    };
    const shownRow = (act) => { const e = document.querySelector('#ctx [data-act="' + act + '"]'); return !!e && !e.classList.contains('hidden'); };
    const activeId = coreC.settings.active_view;
    await menuAt('p1');
    if (!shownRow('views')) errs.push('252: the tile menu has no Views entry with two or more views');
    if (shownRow('hide')) errs.push('252: the tile menu still shows a direct hide row next to Views');
    document.querySelector('#ctx [data-act="views"]').click();
    await sleep(200);
    const pop = document.querySelector('.views-pop:not(.hidden)');
    const first = pop && pop.firstElementChild;
    if (!first || first.getAttribute('role') !== 'menuitem') errs.push('252: the Views popover does not start with the hide row');
    else {
      window.__SMOKE_ARGS__.length = 0;
      first.click();
      await sleep(300);
      // Membership and cell leave together: one removeView for the active view (the
      // backend drops the layout entry too), never a layout-only write.
      const rm = callsFor('batch_prompts').filter((c) => c.action === 'removeView');
      if (rm.length !== 1 || rm[0].viewId !== activeId || (rm[0].ids || []).join() !== 'p1') errs.push('252: the first row did not remove p1 from the active view');
      if (callsFor('set_layout').length) errs.push('252: hiding wrote a layout only, the prompt would stay a view member');
      if (tileEl('p1')) errs.push('252: p1 is still on the grid after hiding it');
    }
    await menuAt('p2');
    document.querySelector('#ctx [data-act="views"]').click();
    await sleep(200);
    const moveRows = [...document.querySelectorAll('.views-pop .view-move-row')];
    if (!document.querySelector('.views-pop .views-pop-head')) errs.push('252: the Views popover has no heading');
    if (!moveRows.length) errs.push('252: with multiViewPrompt off the popover offers no move rows');
    else {
      window.__SMOKE_ARGS__.length = 0;
      moveRows[0].click();
      await sleep(300);
      const moves = callsFor('batch_prompts').filter((a) => a && a.action === 'moveView');
      if (moves.length !== 1 || moves[0].fromViewId !== activeId || moves[0].viewId === activeId) errs.push('252: a Move to row did not call batch_prompts moveView: ' + JSON.stringify(moves));
      if (tileEl('p2')) errs.push('252: p2 is still on the grid after moving it to another view');
    }
    coreC.settings.views = coreC.settings.views.filter((v) => v.id === activeId);
    await menuAt('p3');
    if (!shownRow('hide')) errs.push('252: with one view only the hide row must stay a direct entry');
    if (shownRow('views')) errs.push('252: with one view only the Views entry must be hidden');
"""
    failed += ar_probe('ctx252', '252: the tile menu groups hide and move under Views', 'index', '880x600', '', views_menu_body)
    # Screenshot editor probes share one opener: a plain canvas fired as a capture.
    snip_common = r"""
    await bootReady();
    const snipOpen = async (w, h, color) => {
      const cv = document.createElement('canvas'); cv.width = w; cv.height = h;
      const g = cv.getContext('2d'); g.fillStyle = color || '#ffffff'; g.fillRect(0, 0, w, h);
      window.__FIRE__('snip-captured', { data_url: cv.toDataURL('image/png'), path: '', name: 'x.png' });
      await until(() => !document.getElementById('snip-edit').classList.contains('hidden'), 100);
      await sleep(400);
    };
"""
    # The help button sits right beside the close button, not in the middle of the title bar.
    for size_828 in ('880x600', '680x460'):
        failed += ar_probe('bq828-' + size_828, '828: the editor help button sits beside the close button at ' + size_828, 'index', size_828, '', snip_common + r"""
    await snipOpen(300, 200);
    const help = document.getElementById('snip-edit-shortcuts').getBoundingClientRect();
    const close = document.getElementById('snip-edit-close').getBoundingClientRect();
    const gap = close.left - help.right;
    if (!(gap >= 0 && gap <= 8)) errs.push('828: the help button is ' + gap + ' px left of the close button, want 0 to 8');
""")
    # Tool options open in a panel under the tool button, not in the toolbar row.
    snip_panel_common = snip_common + r"""
    const panel = document.getElementById('snip-edit-context');
    const toolBtn = (n) => document.querySelector('#snip-edit-tools button[data-tool="' + n + '"]');
    const panelOpen = () => !panel.classList.contains('hidden');
    const esc = async () => {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
      await sleep(80);
    };
"""
    for size_829 in ('880x600', '680x460'):
        failed += ar_probe('bq829a-' + size_829, '829: the rect options open under the tool button and the toolbar stays one row at ' + size_829, 'index', size_829, '', snip_panel_common + r"""
    await snipOpen(300, 200);
    const bar = document.querySelector('.snip-edit-toolbar');
    if (bar.getBoundingClientRect().height > 52) errs.push('829: the toolbar is ' + bar.getBoundingClientRect().height + ' px tall before a tool is picked');
    toolBtn('rect').click(); await sleep(200);
    if (!panelOpen()) errs.push('829: picking rect did not open the options panel');
    else {
      const pr = panel.getBoundingClientRect(), br = toolBtn('rect').getBoundingClientRect();
      if (pr.top < br.bottom - 0.5) errs.push('829: the panel top ' + pr.top + ' sits above the tool button bottom ' + br.bottom);
      if (Math.abs(pr.left - br.left) > 40) errs.push('829: the panel left ' + pr.left + ' is ' + Math.abs(pr.left - br.left) + ' px from the tool button left ' + br.left);
      if (getComputedStyle(panel).position !== 'fixed') errs.push('829: the panel is ' + getComputedStyle(panel).position + ', not fixed');
      if (!panel.querySelector('.snip-edit-width-row')) errs.push('829: the panel holds no width field');
    }
    const h = bar.getBoundingClientRect().height;
    if (h > 52) errs.push('829: the toolbar is ' + h + ' px tall with the rect panel open, want at most 52');
    if (bar.scrollWidth > bar.clientWidth + 1) errs.push('829: the toolbar overflows sideways (' + bar.scrollWidth + ' > ' + bar.clientWidth + ')');
    for (const tool of ['hand', 'crop']) {
      toolBtn(tool).click(); await sleep(120);
      if (panelOpen()) errs.push('829: the ' + tool + ' tool opened the options panel');
    }
""")
    failed += ar_probe('bq829b', '829: canvas press, outside click and Escape close the options panel', 'index', '880x600', '', snip_panel_common + r"""
    await snipOpen(300, 200);
    toolBtn('rect').click(); await sleep(150);
    if (!panelOpen()) errs.push('829: the panel did not open');
    const live = document.getElementById('snip-edit-live');
    const lr = live.getBoundingClientRect();
    live.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, pointerId: 1, clientX: lr.left + 20, clientY: lr.top + 20 }));
    live.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: lr.left + 60, clientY: lr.top + 50 }));
    window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: lr.left + 60, clientY: lr.top + 50 }));
    await sleep(120);
    if (panelOpen()) errs.push('829: a press on the canvas left the panel open');
    if (document.getElementById('snip-edit-undo').disabled) errs.push('829: the press on the canvas did not start the stroke');
    // A click on the already-active tool opens it again; a press elsewhere closes it.
    toolBtn('rect').click(); await sleep(120);
    if (!panelOpen()) errs.push('829: a click on the active tool did not open the panel');
    document.querySelector('.snip-edit-titlebar-label').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, pointerId: 2 }));
    await sleep(80);
    if (panelOpen()) errs.push('829: an outside press left the panel open');
    // Picking another tool swaps the panel to that tool.
    toolBtn('rect').click(); await sleep(120);
    toolBtn('pixelate').click(); await sleep(150);
    if (!panelOpen() || !panel.querySelector('input')) errs.push('829: pixelate did not show its own panel');
    // Escape: first the panel, then the editor as before.
    await esc();
    if (panelOpen()) errs.push('829: the first Escape left the panel open');
    if (document.getElementById('snip-edit').classList.contains('hidden')) errs.push('829: the first Escape closed the whole editor');
    await esc();
    // The stroke drawn above is an edit: the second Escape asks before closing.
    if (document.getElementById('confirm-modal').classList.contains('hidden')) errs.push('829: the second Escape with an edit did not ask before closing');
    else document.getElementById('confirm-ok').click();
    await sleep(120);
    if (!document.getElementById('snip-edit').classList.contains('hidden')) errs.push('829: the second Escape did not close the editor');
""")
    failed += ar_probe('bq829c', '829: popups that belong to the panel do not close it', 'index', '880x600', '', snip_panel_common + r"""
    await snipOpen(300, 200);
    toolBtn('text').click(); await sleep(200);
    if (!panelOpen()) errs.push('829: the text tool did not open the panel');
    const sel = panel.querySelector('select.snip-edit-select');
    if (!sel) { errs.push('829: no background select in the text panel'); }
    else {
      const before = sel.value;
      sel.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
      await sleep(120);
      const rows = [...document.querySelectorAll('.select-pop:not(.hidden) button[role="option"]')];
      if (rows.length < 2) errs.push('829: the select list did not open');
      else {
        const pick = rows.find((r) => r.getAttribute('aria-selected') !== 'true');
        pick.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, pointerId: 3 }));
        await sleep(150);
        if (!panelOpen()) errs.push('829: choosing a background closed the panel');
        const after = panel.querySelector('select.snip-edit-select');
        if (!after || after.value === before) errs.push('829: the background choice did not stick');
      }
    }
    // The colour popover is part of the panel too.
    const trig = panel.querySelector('.snip-edit-color-trigger');
    if (!trig) { errs.push('829: no colour trigger in the text panel'); }
    else {
      trig.click(); await sleep(150);
      const cp = document.querySelector('.snip-color-pop:not(.hidden)');
      if (!cp) errs.push('829: the colour popover did not open');
      else {
        const sw = cp.querySelector('.swatch');
        sw.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, pointerId: 4 }));
        sw.click(); await sleep(150);
        if (!panelOpen()) errs.push('829: picking a colour closed the panel');
      }
    }
    // The number field preset list is part of the panel too.
    const lst = panel.querySelector('.numfield-list');
    if (!lst) { errs.push('829: no preset list button in the text panel'); }
    else {
      lst.click(); await sleep(120);
      const pb = document.querySelector('.numfield-pop button');
      if (!pb) errs.push('829: the preset list did not open');
      else {
        pb.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, pointerId: 5 }));
        await sleep(150);
        if (!panelOpen()) errs.push('829: choosing a preset closed the panel');
      }
    }
""")
    # The crop tool starts with a full-image box; the edges are dragged in, and each drag is an undoable crop.
    failed += ar_probe('bq830', '830: the crop box starts at the whole image and is dragged in by its edges', 'index', '880x600', '', snip_common + r"""
    const live = document.getElementById('snip-edit-live');
    const stage = document.getElementById('snip-edit-stage');
    const wrap = document.getElementById('snip-edit-canvas-wrap');
    const undo = document.getElementById('snip-edit-undo');
    const redo = document.getElementById('snip-edit-redo');
    const sleepMs = (ms) => new Promise((r) => setTimeout(r, ms));
    const alphaAt = (x, y) => live.getContext('2d').getImageData(x, y, 1, 1).data[3];
    const firePtr = (type, x, y, target) => (target || window).dispatchEvent(new PointerEvent(type, {
      bubbles: true, cancelable: true, button: 0, pointerId: 1, isPrimary: true, clientX: x, clientY: y,
    }));
    const dragEdge = async (side, fromImg, toImg) => {
      const sr = stage.getBoundingClientRect();
      const z = sr.width / live.width;
      const x0 = sr.left + fromImg * z + (side === 'right' ? -2 : 2), x1 = sr.left + toImg * z;
      const y0 = sr.top + sr.height / 2;
      firePtr('pointerdown', x0, y0, live);
      firePtr('pointermove', (x0 + x1) / 2, y0, live);
      firePtr('pointermove', x1, y0, live);
      firePtr('pointerup', x1, y0, window);
      await sleepMs(150);
    };
    const filled = () => {
      const sr = stage.getBoundingClientRect();
      const z = sr.width / live.width;
      return Math.min(Math.abs(1200 * z - wrap.clientWidth), Math.abs(800 * z - wrap.clientHeight));
    };
    const exportSize = async () => {
      window.__SMOKE_ARGS__.length = 0;
      document.getElementById('snip-edit-copy').click();
      await sleepMs(250);
      const c = callsFor('copy_image_data');
      if (!c.length) return null;
      return await new Promise((res) => { const i = new Image(); i.onload = () => res(i.width + 'x' + i.height); i.src = c[c.length - 1].dataUrl; });
    };
    await snipOpen(1400, 800);
    document.querySelector('#snip-edit-tools button[data-tool="crop"]').click(); await sleepMs(200);
    if (!undo.disabled) errs.push('830: the undo button is enabled before any crop');
    for (const [x, y] of [[1, 1], [live.width - 2, 1], [1, live.height - 2], [live.width - 2, live.height - 2]]) {
      if (alphaAt(x, y) === 0) errs.push('830: no handle is drawn at the image corner ' + x + ',' + y);
    }
    await dragEdge('right', 1400, 1200);
    if (undo.disabled) errs.push('830: dragging an edge in did not create an undoable crop');
    const f1 = filled();
    if (f1 > 2) errs.push('830: the crop box misses the view by ' + f1 + ' px after the drag');
    document.getElementById('snip-edit-undo').click(); await sleepMs(150);
    if (!undo.disabled) errs.push('830: undo left a crop behind');
    if (redo.disabled) errs.push('830: undo did not enable redo');
    if (alphaAt(1, 1) === 0 || alphaAt(live.width - 2, live.height - 2) === 0) errs.push('830: the full-image box is not back after undo');
    redo.click(); await sleepMs(150);
    if (undo.disabled) errs.push('830: redo did not bring the crop back');
    const size = await exportSize();
    if (size !== '1200x800') errs.push('830: the export is ' + size + ', expected 1200x800');
    // After an undo the full box is still draggable.
    document.getElementById('snip-edit-undo').click(); await sleepMs(150);
    await dragEdge('left', 0, 100);
    if (undo.disabled) errs.push('830: the full-image box could not be dragged after an undo');
    if (await exportSize() !== '1300x800') errs.push('830: dragging the left edge in by 100 did not export 1300x800');
    // Dragging back out to the image bounds removes the crop again.
    await dragEdge('left', 100, 0);
    if (await exportSize() !== '1400x800') errs.push('830: dragging the edge back out did not restore the full image');
    // A press outside the box and inside a full box does nothing.
    const before = undo.disabled;
    const sr = stage.getBoundingClientRect();
    firePtr('pointerdown', sr.left + sr.width / 2, sr.top + sr.height / 2, live);
    firePtr('pointermove', sr.left + sr.width / 2 + 40, sr.top + sr.height / 2 + 40, live);
    firePtr('pointerup', sr.left + sr.width / 2 + 40, sr.top + sr.height / 2 + 40, window);
    await sleepMs(100);
    if (undo.disabled !== before) errs.push('830: a press inside the full-image box changed the crop');
""")
    failed += ar_probe('bq830b', '830: Enter with the untouched full-image box applies the editor without a crop', 'index', '880x600', '', snip_common + r"""
    await snipOpen(1400, 800);
    document.querySelector('#snip-edit-tools button[data-tool="crop"]').click(); await sleep(200);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    await sleep(500);
    if (!document.getElementById('snip-edit').classList.contains('hidden')) errs.push('830: Enter with the full-image box did not apply the editor');
    const img = document.getElementById('snip-preview');
    if (img.naturalWidth !== 1400 || img.naturalHeight !== 800) errs.push('830: the applied picture is ' + img.naturalWidth + 'x' + img.naturalHeight + ', expected 1400x800');
""")
    failed += ar_probe('bq830c', '830: Apply after a dragged crop exports the cropped picture', 'index', '880x600', '', snip_common + r"""
    await snipOpen(1400, 800);
    document.querySelector('#snip-edit-tools button[data-tool="crop"]').click(); await sleep(200);
    const live = document.getElementById('snip-edit-live');
    const sr = live.getBoundingClientRect();
    const z = sr.width / live.width;
    const firePtr = (type, x, y, target) => (target || window).dispatchEvent(new PointerEvent(type, {
      bubbles: true, cancelable: true, button: 0, pointerId: 1, isPrimary: true, clientX: x, clientY: y,
    }));
    firePtr('pointerdown', sr.right - 2, sr.top + sr.height / 2, live);
    firePtr('pointermove', sr.right - 200 * z, sr.top + sr.height / 2, live);
    firePtr('pointerup', sr.right - 200 * z, sr.top + sr.height / 2, window);
    await sleep(150);
    document.getElementById('snip-edit-apply').click();
    await sleep(500);
    const img = document.getElementById('snip-preview');
    if (img.naturalWidth !== 1200 || img.naturalHeight !== 800) errs.push('830: the applied picture is ' + img.naturalWidth + 'x' + img.naturalHeight + ', expected 1200x800');
""")
    # 100 % is one image pixel per device pixel: at a device pixel ratio of 1.5 a 300x200 picture is 200x133 CSS pixels.
    failed += ar_probe('bq831', '831: 100 percent shows one image pixel per device pixel', 'index', '880x600', '', snip_common + r"""
    Object.defineProperty(window, 'devicePixelRatio', { get: () => 1.5, configurable: true });
    await snipOpen(300, 200, '#ffffff');
    const stage = document.getElementById('snip-edit-stage');
    const zoomVal = document.getElementById('snip-edit-zoom-val').textContent.trim();
    if (zoomVal !== '100%') errs.push('831: the zoom label reads ' + zoomVal + ', expected 100%');
    const sr = stage.getBoundingClientRect();
    if (Math.abs(sr.width - 200) > 0.6) errs.push('831: the stage is ' + sr.width + ' CSS px wide, expected 200 (300 / 1.5)');
    if (Math.abs(sr.height - 200 / 1.5) > 0.6) errs.push('831: the stage is ' + sr.height + ' CSS px tall, expected ' + (200 / 1.5));
    // A rect dragged from the stage top-left to its bottom-right corner covers the whole picture.
    document.querySelector('#snip-edit-tools button[data-tool="rect"]').click(); await sleep(150);
    const live = document.getElementById('snip-edit-live');
    const fire = (type, x, y, target) => (target || window).dispatchEvent(new PointerEvent(type, {
      bubbles: true, cancelable: true, button: 0, pointerId: 1, isPrimary: true, clientX: x, clientY: y,
    }));
    fire('pointerdown', sr.left, sr.top, live);
    fire('pointermove', (sr.left + sr.right) / 2, (sr.top + sr.bottom) / 2, live);
    fire('pointermove', sr.right, sr.bottom, live);
    fire('pointerup', sr.right, sr.bottom, window);
    await sleep(150);
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('snip-edit-copy').click();
    await sleep(300);
    const calls = callsFor('copy_image_data');
    if (!calls.length) { errs.push('831: the export did not run'); }
    else {
      const img = await new Promise((res) => { const i = new Image(); i.onload = () => res(i); i.src = calls[calls.length - 1].dataUrl; });
      if (img.width !== 300 || img.height !== 200) errs.push('831: the export is ' + img.width + 'x' + img.height + ', expected 300x200');
      const c = document.createElement('canvas'); c.width = img.width; c.height = img.height;
      const g = c.getContext('2d'); g.drawImage(img, 0, 0);
      const inked = (x, y) => { const d = g.getImageData(x, y, 1, 1).data; return d[0] < 250 || d[1] < 250 || d[2] < 250; };
      for (const [x, y] of [[1, 100], [298, 100], [150, 1], [150, 198]]) {
        if (!inked(x, y)) errs.push('831: the rect does not reach the picture edge at ' + x + ',' + y);
      }
    }
""")
    failed += ar_probe('bq831b', '831: magnified pixels stay sharp and the label keeps showing the zoom', 'index', '880x600', '', snip_common + r"""
    Object.defineProperty(window, 'devicePixelRatio', { get: () => 1.5, configurable: true });
    await snipOpen(300, 200, '#ffffff');
    const base = document.getElementById('snip-edit-base');
    if (getComputedStyle(base).imageRendering === 'pixelated') errs.push('831: the canvas is pixelated at 100 percent');
    document.getElementById('snip-edit-zoom-in').click(); await sleep(150);
    const label = document.getElementById('snip-edit-zoom-val').textContent.trim();
    if (label !== '120%') errs.push('831: one zoom step reads ' + label + ', expected 120%');
    if (getComputedStyle(base).imageRendering !== 'pixelated') errs.push('831: the base canvas is not pixelated above 100 percent');
    if (getComputedStyle(document.getElementById('snip-edit-live')).imageRendering !== 'pixelated') errs.push('831: the live canvas is not pixelated above 100 percent');
    const w = document.getElementById('snip-edit-stage').getBoundingClientRect().width;
    if (Math.abs(w - 300 * 1.2 / 1.5) > 0.6) errs.push('831: at 120 percent the stage is ' + w + ' CSS px wide, expected ' + (300 * 1.2 / 1.5));
""")
    # At the minimum width the whole toolbar, view controls included, fits one row without scrolling, in both languages.
    for lang_829 in ('en', 'de'):
        failed += ar_probe('bq829w-' + lang_829, '829: the toolbar fits one row at 680x460 (' + lang_829 + ')', 'index', '680x460', '<script>window.__SEED_SETTINGS__ = { language: "' + lang_829 + '" };</script>\n', snip_common + r"""
    await snipOpen(300, 200);
    const bar = document.querySelector('.snip-edit-toolbar');
    if (bar.scrollWidth > bar.clientWidth) errs.push('829: the toolbar is ' + bar.scrollWidth + ' px wide in a ' + bar.clientWidth + ' px bar');
    if (bar.getBoundingClientRect().height > 52) errs.push('829: the toolbar is ' + bar.getBoundingClientRect().height + ' px tall');
    const eye = document.getElementById('snip-edit-original').getBoundingClientRect();
    if (eye.right > bar.getBoundingClientRect().right + 0.5) errs.push('829: the eye button sticks out past the toolbar');
    for (const id of ['snip-edit-fit', 'snip-edit-scale']) {
      const b = document.getElementById(id);
      if (!b.getAttribute('aria-label')) errs.push('829: ' + id + ' has no accessible name');
      if (!b.title) errs.push('829: ' + id + ' has no title');
    }
""")
    # Undo and zoom live in the bottom bar; the toolbar stays one line and the bottom row stays one line in German.
    for size_932 in ('880x600', '680x460'):
        failed += ar_probe('bq932a-' + size_932, '932-de: undo and zoom sit in the bottom bar, both bars stay one line at ' + size_932, 'index', size_932, '<script>window.__SEED_SETTINGS__ = { language: "de" };</script>\n', snip_common + r"""
    // 932-de: the status group (undo, redo, zoom) moved from the toolbar into the bottom row.
    await snipOpen(300, 200);
    const bar = document.querySelector('.snip-edit-toolbar');
    const actions = document.querySelector('.snip-edit-actions');
    const checkBars = (when) => {
      const bh = bar.getBoundingClientRect().height;
      if (bh > 52) errs.push('932-de: the toolbar is ' + bh + ' px tall ' + when + ', want at most 52');
      if (bar.scrollWidth > bar.clientWidth + 1) errs.push('932-de: the toolbar overflows sideways ' + when + ' (' + bar.scrollWidth + ' > ' + bar.clientWidth + ')');
      const ah = actions.getBoundingClientRect().height;
      if (ah > 56) errs.push('932-de: the bottom row is ' + ah + ' px tall ' + when + ', want one line (at most 56)');
    };
    checkBars('on open');
    const ar = actions.getBoundingClientRect();
    for (const id of ['snip-edit-undo', 'snip-edit-redo', 'snip-edit-zoom-out', 'snip-edit-zoom-in', 'snip-edit-fit']) {
      const r = document.getElementById(id).getBoundingClientRect();
      if (r.left < ar.left - 0.5 || r.right > ar.right + 0.5 || r.top < ar.top - 0.5 || r.bottom > ar.bottom + 0.5) {
        errs.push('932-de: ' + id + ' lies outside the bottom row');
      }
    }
    const undo = document.getElementById('snip-edit-undo');
    if (!undo.disabled) errs.push('932-de: undo is enabled on open');
    const op = getComputedStyle(undo).opacity;
    if (Math.abs(Number(op) - 0.35) > 0.01) errs.push('932-de: the disabled undo button has opacity ' + op + ', want 0.35');
    const eye = document.getElementById('snip-edit-original');
    const tap = async () => {
      const [x, y] = centre(eye);
      ptr('pointerdown', x, y, eye);
      ptr('pointerup', x, y, eye);
      await sleep(150);
    };
    await tap();
    if (document.getElementById('snip-edit-original-badge').classList.contains('hidden')) errs.push('932-de: a tap on the eye did not show the badge');
    checkBars('with the original badge shown');
    await tap();
    if (!document.getElementById('snip-edit-original-badge').classList.contains('hidden')) errs.push('932-de: a second tap on the eye did not release the badge');
""")
    # A long translation may wrap the bottom row but nothing may leave the window.
    for lang_932 in ('it', 'pl'):
        failed += ar_probe('bq932b-' + lang_932, '932-de: the bottom row stays inside the window at 680x460 (' + lang_932 + ')', 'index', '680x460', '<script>window.__SEED_SETTINGS__ = { language: "' + lang_932 + '" };</script>\n', snip_common + r"""
    await snipOpen(300, 200);
    const actions = document.querySelector('.snip-edit-actions');
    for (const el of actions.querySelectorAll('button, .snip-edit-status, .snip-edit-zoom-val')) {
      const r = el.getBoundingClientRect();
      if (r.width === 0) continue;
      if (r.left < -0.5 || r.right > window.innerWidth + 0.5) errs.push('932-de: ' + (el.id || el.className) + ' leaves the window (' + Math.round(r.left) + ' to ' + Math.round(r.right) + ' of ' + window.innerWidth + ')');
    }
""")
    # ---- main window findings: generated block end ----
    return failed
