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
    failed += ar_probe('g2', 'expert menu bulk actions never flip demo mode, remote access, network, sync or the URL scheme', 'index', '1000x780', '<script>window.__DEMO_TEST__ = true;</script>\n', r"""
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
    const rb = document.getElementById('expert-reset');
    rb.click(); await sleep(150);
    rb.click(); await sleep(500);
    const seq = (window.__SMOKE_ARGS__ || []).filter(([c, a]) => c === 'reset_expert' || (c === 'set_ui_flag' && a.key === 'demoMode'));
    if (!seq.length || seq[0][0] !== 'reset_expert') errs.push('564-2: reset all did not run reset_expert');
    const back = seq.find(([c, a]) => c === 'set_ui_flag' && a.key === 'demoMode');
    if (!back || back[1].enabled !== true) errs.push('564-2: reset all left a running demo session without its marker');
""")
    failed += ar_probe('g3', 'expert menu: a stored remote rate limit of 0 (unlimited) shows as unlimited, not 5/min', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_flags: { remoteApi: true }, ui_values: { remoteRateLimit: 0 } };</script>\n', r"""
    await bootReady();
    document.getElementById('gear').click(); await sleep(200);
    document.getElementById('expert-open').click(); await sleep(300);
    let row = null;
    for (const tb of [...document.querySelectorAll('#expert-tabs-bar .expert-tab')]) {
      tb.click(); await sleep(80);
      row = document.querySelector('#expert-flags [data-setting-row="remoteRateLimit"]');
      if (row) break;
    }
    if (!row) { errs.push('564-3: the remote rate limit row was not found'); }
    else {
      const out = row.querySelector('.value-out').textContent;
      if (out !== 'Unlimited') errs.push('564-3: the stored 0 shows as ' + JSON.stringify(out) + ', expected Unlimited');
    }
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
    const srcEl = document.querySelector('#journal-source [data-source="elsewhere"]');
    if (!srcEl || document.getElementById('journal-source').classList.contains('hidden')) { errs.push('564-c3: the source chips are not shown in the fixture'); }
    else {
      srcEl.click(); await sleep(200);
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
    document.querySelector('#journal-source [data-source="elsewhere"]').click(); await sleep(300);
    window.__SMOKE_ARGS__.length = 0;
    document.getElementById('journal-clear').click(); await sleep(300);
    if (confirmOpen()) errs.push('564-m2: clearing an empty store opened a dialog');
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
      if (document.getElementById('clip-purge-backups').classList.contains('hidden')) errs.push('564-l3: the purge button is not reachable');
      if (document.querySelectorAll('#journal-body .clip-row').length !== 2) errs.push('564-l3: the stored entries are not listed');
    }
    // With nothing stored and nothing recording, the tab is gone again.
    document.getElementById('library-close').click(); await sleep(200);
    RESULTS.clip_inbox_list = () => [];
    document.getElementById('library-btn').click(); await sleep(500);
    if (!tabsHidden()) errs.push('564-l3: the history tab is offered with nothing to show and nothing recording');
""")
    failed += ar_probe('s1', 'screenshot editor: undoing a resize does not unlock operations behind the undo limit', 'index', '1000x780', '<script>window.__SEED_SETTINGS__ = { ui_values: { snipEditUndoMax: 10 } };</script>\n', r"""
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
      for (let i = 0; i < 12; i++) {
        const x = r.left + 10 + i * 3, y = r.top + 10 + i * 4;
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
      while (!undo.disabled && steps < 40) { undo.click(); steps++; await sleep(15); }
      // 12 strokes, limit 10: the scale itself plus the 10 newest strokes can be undone
      if (steps !== 11) errs.push('564-s1: undo walked ' + steps + ' steps after a resize, expected 11 (the resize and ten strokes)');
      const redo = document.getElementById('snip-edit-redo');
      let redos = 0;
      while (!redo.disabled && redos < 40) { redo.click(); redos++; await sleep(15); }
      if (redos !== 11) errs.push('564-s1: redo walked ' + redos + ' steps, expected 11');
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
      await baTool('crop'); await baDrag(60, 30, 140, 90);
      baKey('Enter'); await sleep(80);
      let img = await baExport();
      if (!img || img.w !== 80 || img.h !== 60) errs.push('592: fixture: the crop did not export 80x60, got ' + (img ? img.w + 'x' + img.h : 'nothing'));
      await baUndo();
      img = await baExport();
      if (!img || img.w !== BAW || img.h !== BAH) errs.push('592: after undo the export is ' + (img ? img.w + 'x' + img.h : 'nothing') + ', expected the full ' + BAW + 'x' + BAH);
      if (!document.getElementById('snip-edit-undo').disabled) errs.push('592: exporting pushed the undone crop back onto the undo stack');
      await baRedo();
      img = await baExport();
      if (!img || img.w !== 80 || img.h !== 60) errs.push('592: redo did not bring the crop back, export is ' + (img ? img.w + 'x' + img.h : 'nothing'));
    }
""")
    failed += ar_probe('ba592b', 'screenshot editor: a resize keeps the crop box and the exported crop in step', 'index', '1000x780', '', ba_common + r"""
    if (!(await baOpen(''))) { errs.push('592: the editor did not open'); }
    else {
      await baTool('crop'); await baDrag(100, 60, 180, 110);
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
      await baTool('crop'); await baDrag(60, 30, 140, 90);
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
      await baTool('crop'); await baDrag(10, 10, 190, 110);
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
    document.getElementById('settings-backup-btn').click();
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
    # ---- main window findings: generated block end ----
    return failed
