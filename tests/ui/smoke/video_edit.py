"""Video editor probes added with the editor rework: title bar, selection, overlays. Each probe has its own boot."""
import json
import re

from .harness import REPO, unescape, run_chrome


SCAFFOLD = r"""
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => { for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50); return fn(); };
  try {
    await until(() => {
      const hint = document.querySelector('#ve-ops-list p');
      return document.getElementById('ve-stage-msg').classList.contains('hidden') &&
        document.querySelectorAll('#ve-filmstrip .ve-thumb').length > 0 &&
        !!hint && hint.textContent && hint.textContent !== 'veOpsEmpty';
    }, 300);
    const mod = await import('./video.js');
    const { VE } = mod;
    const ptr = (type, x, y, el, extra) => (el || window).dispatchEvent(new PointerEvent(type, Object.assign({
      bubbles: true, cancelable: true, button: 0, isPrimary: true, pointerId: 7, clientX: x, clientY: y,
    }, extra || {})));
%s
  } catch (e) { errs.push('probe threw: ' + e.message + ' ' + (e.stack || '').split('\n')[1]); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


def run(browser, tmp):
    failed = 0

    def vprobe(tag, label, body, pre="", size="1130,860"):
        side = tmp / "ui" / f"ve-{tag}.html"
        side.write_text(
            (REPO / "ui" / "video.html").read_text(encoding="utf-8")
            .replace("<script", pre + '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", SCAFFOLD % body + "</body>"),
            encoding="utf-8")
        proc = run_chrome(browser, tmp / ("profile-ve-" + tag), size, 30000, side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- video.html {label}")
        for e in errs:
            print(f"   FAIL: {e}")
        if not errs:
            print("   ok")
        return len(errs)

    # The camera window's bar is a deep drag region too (its markup is static, nothing to boot).
    cam = (REPO / "ui" / "camera.html").read_text(encoding="utf-8")
    print("-- camera.html title bar drags everywhere")
    if 'class="cam-titlebar" data-tauri-drag-region="deep"' in cam:
        print("   ok")
    else:
        print('   FAIL: the camera title bar is not data-tauri-drag-region="deep"')
        failed += 1

    # 4021: a bare drag attribute moves only from the bar itself; the file name and label need "deep".
    failed += vprobe('bar', 'title bar is a deep drag region, buttons opt out', r"""
    // 4021: the video editor title bar.
    const bar = document.querySelector('.ve-titlebar');
    if (bar.getAttribute('data-tauri-drag-region') !== 'deep') errs.push('4021: the bar drag region is "' + bar.getAttribute('data-tauri-drag-region') + '", want deep');
    for (const b of bar.querySelectorAll('button')) {
      if (b.getAttribute('data-tauri-drag-region') !== 'false') errs.push('4021: ' + b.id + ' does not opt out of the drag region');
    }
    """)

    # 4022: minimize and maximize/restore sit between the shortcuts and close buttons.
    failed += vprobe('winbtn', 'minimize and maximize buttons follow the window state', r"""
    // 4022: window buttons in the video editor bar.
    const mn = document.getElementById('ve-minimize');
    const mx = document.getElementById('ve-maximize');
    const sh = document.getElementById('ve-shortcuts');
    const cl = document.getElementById('ve-close');
    if (!mn || !mx) { errs.push('4022: a window button is missing'); }
    else {
      for (const b of [mn, mx]) {
        if (b.getAttribute('data-tauri-drag-region') !== 'false') errs.push('4022: ' + b.id + ' does not opt out of the drag region');
        const sv = b.querySelector('svg');
        if (!sv || sv.getAttribute('width') !== '18' || sv.getAttribute('stroke-width') !== '1.5') errs.push('4022: ' + b.id + ' icon is not 18px with stroke 1.5');
        if (!b.title || !b.getAttribute('aria-label') || /^win[A-Z]/.test(b.title)) errs.push('4022: ' + b.id + ' has no label');
      }
      const r = (e) => e.getBoundingClientRect();
      if (!(r(sh).right <= r(mn).left + 1 && r(mn).right <= r(mx).left + 1 && r(mx).right <= r(cl).left + 1)) errs.push('4022: the order is not shortcuts, minimize, maximize, close');
      if (Math.abs(r(mn).height - r(cl).height) > 1) errs.push('4022: the minimize button is not as tall as the close button');
      const iconBefore = mx.innerHTML, titleBefore = mx.title;
      mx.click(); await sleep(150);
      if (mx.dataset.maximized !== '1' || mx.innerHTML === iconBefore) errs.push('4022: a click did not switch to the restore icon');
      if (mx.title === titleBefore) errs.push('4022: the title did not change to Restore');
      mx.click(); await sleep(150);
      if (mx.dataset.maximized !== '0' || mx.innerHTML !== iconBefore) errs.push('4022: a second click did not switch back');
    }
    """)

    # 4023: a plain drag on the region track selects, a click only moves the playhead, the ruler scrubs.
    failed += vprobe('drag', 'drag selects, click seeks, ruler scrubs', r"""
    // 4023: timeline pointer input.
    const reg = document.getElementById('ve-regions');
    const rr = reg.getBoundingClientRect();
    const X = (ms) => rr.left + (ms / VE.dur) * rr.width;
    const tol = (VE.dur / rr.width) * 1.5 + 1;
    const near = (a, b) => Math.abs(a - b) <= tol;
    const yBody = rr.top + 40, yRuler = rr.top + 6;
    // The stub clip is ten minutes long, so a few pixels are seconds: use ranges well past the 4 px slop.
    ptr('pointerdown', X(20000), yBody, reg); ptr('pointermove', X(120000), yBody, reg); ptr('pointerup', X(120000), yBody, reg);
    if (!VE.sel || !near(VE.sel.a, 20000) || !near(VE.sel.b, 120000)) errs.push('4023: a drag from 20 s to 120 s selected ' + JSON.stringify(VE.sel));
    VE.sel = null; mod.renderRegions();
    ptr('pointerdown', X(70000), yBody, reg); ptr('pointerup', X(70000), yBody, reg);
    await sleep(100);
    if (VE.sel) errs.push('4023: a click left a selection ' + JSON.stringify(VE.sel));
    if (!near(VE.playhead, 70000)) errs.push('4023: a click did not move the playhead (' + VE.playhead + ')');
    ptr('pointerdown', X(80000), yBody, reg); ptr('pointermove', X(80000) + 2, yBody, reg); ptr('pointerup', X(80000) + 2, yBody, reg);
    if (VE.sel) errs.push('4023: a 2 px wobble made a selection ' + JSON.stringify(VE.sel));
    ptr('pointerdown', X(30000), yRuler, reg); ptr('pointermove', X(40000), yRuler, reg); ptr('pointerup', X(40000), yRuler, reg);
    await sleep(100);
    if (!near(VE.playhead, 40000)) errs.push('4023: a drag on the ruler did not scrub (' + VE.playhead + ')');
    if (VE.sel) errs.push('4023: scrubbing on the ruler made a selection');
    // A selection shorter than the minimum is dropped on release: on a one-second clip a few pixels are milliseconds.
    const longDur = VE.dur;
    VE.dur = 1000; mod.renderRegions();
    const r2 = reg.getBoundingClientRect();
    ptr('pointerdown', r2.left + 100, yBody, reg); ptr('pointermove', r2.left + 106, yBody, reg); ptr('pointerup', r2.left + 106, yBody, reg);
    if (VE.sel) errs.push('4023: a selection shorter than the minimum survived the release ' + JSON.stringify(VE.sel));
    VE.dur = longDur; mod.renderRegions();
    if (!document.getElementById('ve-timeline').title) errs.push('4023: the timeline has no hint tooltip');
    """)

    # 4024: buttons follow the selection; trim and cut act on it; marks default to the trim range.
    failed += vprobe('selbtn', 'selection buttons: disabled without a range, trim and cut, merge', r"""
    // 4024: selection tools.
    const $ = (id) => document.getElementById(id);
    const cuts = () => VE.ops.filter((o) => o.tool === 'cut').map((o) => [o.startMs, o.endMs]);
    const trimOp = () => VE.ops.filter((o) => o.tool === 'trim').pop() || null;
    const press = (key) => document.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
    mod.renderRegions();
    if (!$('ve-trim-set').disabled || !$('ve-cut-add').disabled) errs.push('4024: trim or cut is enabled without a selection');
    if ($('ve-trim-set').title !== 'Select a range first: drag on the timeline or use the selection buttons.') errs.push('4024: the disabled trim button has the tooltip "' + $('ve-trim-set').title + '"');
    if (!($('ve-mark-out').compareDocumentPosition($('ve-mark-in')) & Node.DOCUMENT_POSITION_FOLLOWING)) errs.push('4024: mark out does not come before mark in');
    press('c');
    if ($('ve-toast').textContent !== 'Select a range on the timeline first.') errs.push('4024: C without a selection did not toast, got "' + $('ve-toast').textContent + '"');
    VE.sel = { a: 3000, b: 3000 }; mod.renderRegions();
    if (!$('ve-trim-set').disabled) errs.push('4024: a collapsed selection enables trim');
    VE.sel = { a: 2000, b: 5000 }; mod.renderRegions();
    if ($('ve-trim-set').disabled || $('ve-cut-add').disabled) errs.push('4024: a real selection leaves trim or cut disabled');
    if ($('ve-trim-set').title === 'Select a range first: drag on the timeline or use the selection buttons.') errs.push('4024: the enabled trim button keeps the need-selection tooltip');
    $('ve-trim-set').click();
    const tr = trimOp();
    if (!tr || tr.startMs !== 2000 || tr.endMs !== 5000) errs.push('4024: trim to selection gave ' + JSON.stringify(tr));
    if (VE.sel) errs.push('4024: trim kept the selection');
    VE.sel = { a: 3000, b: 4000 }; mod.renderRegions(); $('ve-cut-add').click();
    if (JSON.stringify(cuts()) !== '[[3000,4000]]') errs.push('4024: a valid cut gave ' + JSON.stringify(cuts()));
    VE.sel = { a: 3500, b: 4800 }; mod.renderRegions(); $('ve-cut-add').click();
    if (JSON.stringify(cuts()) !== '[[3000,4800]]') errs.push('4024: overlapping cuts did not merge: ' + JSON.stringify(cuts()));
    mod.undo();
    if (JSON.stringify(cuts()) !== '[[3000,4000]]') errs.push('4024: one undo did not restore the earlier cut: ' + JSON.stringify(cuts()));
    mod.redo();
    if (JSON.stringify(cuts()) !== '[[3000,4800]]') errs.push('4024: redo did not restore the merged cut: ' + JSON.stringify(cuts()));
    VE.sel = { a: 4500, b: 9000 }; mod.renderRegions(); $('ve-cut-add').click();
    if (JSON.stringify(cuts()) !== '[[3000,5000]]') errs.push('4024: a cut running past the trim end was not clamped: ' + JSON.stringify(cuts()));
    VE.sel = { a: 6000, b: 7000 }; mod.renderRegions(); $('ve-cut-add').click();
    if ($('ve-toast').textContent !== 'The selection is outside the trimmed part.') errs.push('4024: a cut outside the trim did not toast, got "' + $('ve-toast').textContent + '"');
    if (JSON.stringify(cuts()) !== '[[3000,5000]]') errs.push('4024: a cut outside the trim was accepted: ' + JSON.stringify(cuts()));
    """)

    # 4025: mark in/out fall back to the trim range, not the recording.
    failed += vprobe('marks', 'mark in and out default to the trim range', r"""
    // 4025: marks against a trimmed clip.
    const $ = (id) => document.getElementById(id);
    mod.pushOp({ tool: 'trim', startMs: 10000, endMs: 60000 });
    await sleep(100);
    mod.seek(30000); await sleep(150);
    $('ve-mark-out').click();
    if (!VE.sel || VE.sel.a !== 10000 || Math.abs(VE.sel.b - VE.playhead) > 1) errs.push('4025: mark out gave ' + JSON.stringify(VE.sel) + ' at ' + VE.playhead);
    VE.sel = null;
    mod.seek(40000); await sleep(150);
    $('ve-mark-in').click();
    if (!VE.sel || VE.sel.b !== 60000 || Math.abs(VE.sel.a - VE.playhead) > 1) errs.push('4025: mark in gave ' + JSON.stringify(VE.sel) + ' at ' + VE.playhead);
    """)

    # 4026: the keys preview shows W as the pressed sample key, never Esc, and says so.
    failed += vprobe('keyssample', 'keys preview highlights W and explains it', r"""
    // 4026: sample key of the keys preview.
    const { keysPreviewModel } = await import('./videoKeys.js');
    const rect = { x: 0, y: 0, w: 900, h: 300 };
    for (const preset of ['full', 'half', 'game']) {
      const active = keysPreviewModel({ preset, rect, keys: [] }).cells.filter((c) => c.active);
      if (active.length !== 1 || active[0].label !== 'W') errs.push('4026: ' + preset + ' layout highlights ' + JSON.stringify(active.map((c) => c.label)));
    }
    const custom = keysPreviewModel({ preset: 'custom', rect, keys: [0x41, 0x42] }).cells;
    if (!custom[0].active || custom[1].active) errs.push('4026: a custom pick without W should highlight its first key');
    const mixed = keysPreviewModel({ preset: 'custom', rect, keys: [0x41, 0x57] }).cells;
    if (mixed[0].active || !mixed[1].active) errs.push('4026: a custom pick with W should highlight W');
    const { addOverlay } = await import('./videoOverlays.js');
    VE.keySidecar = true;
    mod.pushOp({ tool: 'overlay', id: 'k1', kind: 'keys', startMs: 0, endMs: 5000, preset: 'full', keys: [], color: '#303030', activeColor: '#e0a000', opacity: 100, rect: { x: 0, y: 0, w: 400, h: 120 } });
    mod.selectOverlay('k1'); await sleep(150);
    const hints = [...document.querySelectorAll('#ve-ov-editor .modal-hint')].filter((h) => h.textContent === 'The highlighted key shows how a pressed key looks.');
    if (hints.length !== 1) errs.push('4026: the sample hint appears ' + hints.length + ' times in the keys editor');
    """)

    # 4027: each overlay row ends in a two-step remove cross; Delete removes the selected overlay.
    failed += vprobe('ovremove', 'overlay row remove cross arms, removes, undoes', r"""
    // 4027: overlay removal.
    const { addOverlay } = await import('./videoOverlays.js');
    const rowsOf = () => [...document.querySelectorAll('#ve-ov-list .ve-ov-item')];
    const count = () => VE.ops.filter((o) => o.tool === 'overlay').length;
    await addOverlay('frame'); await addOverlay('pixelate'); await sleep(150);
    if (count() !== 2 || rowsOf().length !== 2) { errs.push('4027: setup gave ' + count() + ' overlays and ' + rowsOf().length + ' rows'); return; }
    const del = () => rowsOf()[0].querySelector('.ve-ov-del');
    if (!del()) { errs.push('4027: the row has no remove cross'); return; }
    if (del().title !== 'Remove this overlay' || del().getAttribute('aria-label') !== 'Remove this overlay') errs.push('4027: cross label is "' + del().title + '"');
    del().click(); await sleep(50);
    if (!del().classList.contains('confirm') || del().textContent !== 'Remove?') errs.push('4027: the first click did not arm, text "' + del().textContent + '"');
    if (count() !== 2) errs.push('4027: the first click already removed an overlay');
    await sleep(3400);
    if (del().classList.contains('confirm') || del().textContent === 'Remove?') errs.push('4027: the armed cross did not expire');
    del().click(); await sleep(50); del().click(); await sleep(150);
    if (count() !== 1 || rowsOf().length !== 1) errs.push('4027: the second click left ' + count() + ' overlays');
    mod.undo(); await sleep(150);
    if (count() !== 2 || rowsOf().length !== 2) errs.push('4027: undo gave ' + count() + ' overlays');
    // Delete on a selected overlay removes it without arming.
    const id = VE.ops.filter((o) => o.tool === 'overlay')[0].id;
    mod.selectOverlay(id); await sleep(100);
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true, cancelable: true }));
    await sleep(150);
    if (count() !== 1 || VE.ops.some((o) => o.id === id)) errs.push('4027: Delete did not remove the selected overlay');
    mod.undo(); await sleep(100);
    if (count() !== 2) errs.push('4027: undo after Delete gave ' + count() + ' overlays');
    """)
    return failed
