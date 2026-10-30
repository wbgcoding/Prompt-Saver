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
    const press = (key, code) => document.dispatchEvent(new KeyboardEvent('keydown', { key, code, bubbles: true, cancelable: true }));
    mod.renderRegions();
    if (!$('ve-trim-set').disabled || !$('ve-cut-add').disabled) errs.push('4024: trim or cut is enabled without a selection');
    if ($('ve-trim-set').title !== 'Select a range first: drag on the timeline or use the selection buttons.') errs.push('4024: the disabled trim button has the tooltip "' + $('ve-trim-set').title + '"');
    if (!($('ve-mark-out').compareDocumentPosition($('ve-mark-in')) & Node.DOCUMENT_POSITION_FOLLOWING)) errs.push('4024: mark out does not come before mark in');
    press('c', 'KeyC');
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

    # 4028: a drag that leaves the timeline while zoomed scrolls it on its own.
    failed += vprobe('autoscroll', 'dragging past the timeline edge scrolls it', r"""
    // 4028: drag autoscroll.
    const $ = (id) => document.getElementById(id);
    for (let i = 0; i < 6 && $('ve-track').getBoundingClientRect().width <= $('ve-timeline').clientWidth * 1.5; i++) { $('ve-zoom-in').click(); await sleep(60); }
    const tl = $('ve-timeline');
    if (tl.scrollWidth <= tl.clientWidth) { errs.push('4028: setup did not zoom in, scrollWidth ' + tl.scrollWidth); return; }
    tl.scrollLeft = 0;
    const reg = $('ve-regions');
    const rr = reg.getBoundingClientRect(), tr = tl.getBoundingClientRect();
    const y = rr.top + 40;
    ptr('pointerdown', tr.left + 100, y, reg);
    ptr('pointermove', tr.left + 160, y, reg);
    ptr('pointermove', tr.right + 60, y, reg);
    const moved = await until(() => tl.scrollLeft > 20, 60);
    const mid = tl.scrollLeft;
    await sleep(200);
    const later = tl.scrollLeft;
    ptr('pointerup', tr.right + 60, y, reg);
    if (!moved) errs.push('4028: the timeline did not scroll while the pointer was past its right edge');
    if (later <= mid) errs.push('4028: the scroll did not continue while the pointer stayed outside (' + mid + ' -> ' + later + ')');
    const stop = tl.scrollLeft;
    await sleep(200);
    if (tl.scrollLeft !== stop) errs.push('4028: the timeline kept scrolling after the release');
    if (!VE.sel || VE.sel.b <= VE.sel.a) errs.push('4028: the drag past the edge gave no selection ' + JSON.stringify(VE.sel));
    """)

    # 4029: cut bands and overlay bars take clicks; their edges resize.
    failed += vprobe('barclick', 'overlay bars and cut bands select on click, edges resize', r"""
    // 4029: clickable bars.
    const { addOverlay } = await import('./videoOverlays.js');
    const $ = (id) => document.getElementById(id);
    await addOverlay('frame'); await addOverlay('pixelate'); await sleep(150);
    const ovs = () => VE.ops.filter((o) => o.tool === 'overlay');
    if (ovs().length !== 2) { errs.push('4029: setup gave ' + ovs().length + ' overlays'); return; }
    // Widen both so the bars are clearly clickable, then deselect.
    VE.selOv = null; mod.renderRegions();
    const bars = () => [...document.querySelectorAll('#ve-regions .ve-ovbar')];
    if (bars().length !== 2) { errs.push('4029: ' + bars().length + ' overlay bars'); return; }
    const first = ovs()[0];
    const bar = bars().find((b) => b.title.indexOf(mod.overlayName(first)) === 0) || bars()[0];
    const br = bar.getBoundingClientRect();
    const hit = document.elementFromPoint(br.left + br.width / 2, br.top + br.height / 2);
    if (!hit || !hit.closest('.ve-ovbar')) errs.push('4029: a click on the bar centre would not reach the bar (' + (hit && hit.className) + ')');
    const reg = $('ve-regions');
    ptr('pointerdown', br.left + br.width / 2, br.top + br.height / 2, bar);
    ptr('pointerup', br.left + br.width / 2, br.top + br.height / 2, reg);
    await sleep(120);
    const barOfSel = () => bars().filter((b) => b.classList.contains('sel'));
    if (!VE.selOv) errs.push('4029: pressing an overlay bar selected nothing');
    if (barOfSel().length !== 1) errs.push('4029: ' + barOfSel().length + ' bars show as selected');
    if (!document.querySelector('#ve-ov-list .ve-ov-row.sel')) errs.push('4029: the matching list row is not selected');
    if (VE.sel) errs.push('4029: a bar click started a range selection');
    // Resize through the left edge: the start moves, the end stays, undo restores.
    const id = VE.selOv;
    const op0 = ovs().find((o) => o.id === id);
    const b2 = barOfSel()[0];
    const edge = b2.querySelector('.ve-bar-edge-a');
    if (!edge) { errs.push('4029: the selected bar has no start edge handle'); return; }
    const er = edge.getBoundingClientRect();
    const ex = er.left + 2, ey = er.top + er.height / 2;
    const hit2 = document.elementFromPoint(ex, ey);
    if (!hit2 || !hit2.classList.contains('ve-bar-edge')) errs.push('4029: the edge handle is covered by ' + (hit2 && hit2.className));
    const pxPerMs = $('ve-track').getBoundingClientRect().width / VE.dur;
    const dx = Math.round(40 / pxPerMs / 1000) * 1000 * pxPerMs;
    ptr('pointerdown', ex, ey, edge);
    ptr('pointermove', ex + dx, ey, reg);
    ptr('pointerup', ex + dx, ey, reg);
    await sleep(120);
    const op1 = ovs().find((o) => o.id === id);
    if (!op1 || op1.endMs !== op0.endMs || op1.startMs === op0.startMs) errs.push('4029: the edge drag gave ' + JSON.stringify([op0.startMs, op0.endMs]) + ' -> ' + JSON.stringify(op1 && [op1.startMs, op1.endMs]));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', code: 'KeyZ', ctrlKey: true, bubbles: true, cancelable: true }));
    await sleep(120);
    const op2 = ovs().find((o) => o.id === id);
    if (!op2 || op2.startMs !== op0.startMs || op2.endMs !== op0.endMs) errs.push('4029: undo did not restore the overlay range');
    // A cut band selects its entry in the ops list.
    VE.sel = { a: 3000, b: 9000 }; mod.renderRegions(); $('ve-cut-add').click(); await sleep(100);
    const cut = document.querySelector('#ve-regions .ve-cut');
    if (!cut) { errs.push('4029: no cut band after adding a cut'); return; }
    VE.selOv = null; mod.renderRegions();
    const cr = document.querySelector('#ve-regions .ve-cut').getBoundingClientRect();
    const cutOp = VE.ops.find((o) => o.tool === 'cut');
    VE.playhead = 100;
    ptr('pointerdown', cr.left + cr.width / 2, cr.top + cr.height / 2, document.querySelector('#ve-regions .ve-cut'));
    ptr('pointerup', cr.left + cr.width / 2, cr.top + cr.height / 2, reg);
    await sleep(150);
    if (VE.sel) errs.push('4029: a cut band click started a range selection');
    if (Math.abs(VE.playhead - cutOp.startMs) > 50) errs.push('4029: a cut band click left the playhead at ' + VE.playhead + ' instead of the cut start ' + cutOp.startMs);
    """)

    # 4030: letter shortcuts follow the printed letter, the physical key on a non-Latin layout.
    failed += vprobe('keycode', 'shortcuts work on German and non-Latin layouts', r"""
    // 4030: letterCode dispatch.
    const $ = (id) => document.getElementById(id);
    if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
    VE.sel = null; mod.renderRegions();
    VE.playhead = 5000;
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ш', code: 'KeyI', bubbles: true, cancelable: true }));
    await sleep(60);
    if (!VE.sel) errs.push('4030: Mark in on a Cyrillic layout (code KeyI) did nothing');
    else if (VE.sel.a !== 5000) errs.push('4030: mark in set ' + JSON.stringify(VE.sel));
    VE.sel = { a: 2000, b: 4000 }; mod.renderRegions();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'с', code: 'KeyC', bubbles: true, cancelable: true }));
    await sleep(80);
    if (!VE.ops.some((o) => o.tool === 'cut')) errs.push('4030: Cut on a Cyrillic layout (code KeyC) did nothing');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'я', code: 'KeyZ', ctrlKey: true, bubbles: true, cancelable: true }));
    await sleep(80);
    if (VE.ops.some((o) => o.tool === 'cut')) errs.push('4030: Ctrl+Z on a Cyrillic layout (code KeyZ) did not undo');
    // A Latin layout goes by the printed letter: on a German keyboard the Z key sits where a
    // US keyboard has Y (code KeyY), and Ctrl+Z there must undo, not redo.
    VE.sel = { a: 2000, b: 4000 }; mod.renderRegions();
    $('ve-cut-add').click();
    await sleep(80);
    if (!VE.ops.some((o) => o.tool === 'cut')) errs.push('4030: no cut to undo on the German layout');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', code: 'KeyY', ctrlKey: true, bubbles: true, cancelable: true }));
    await sleep(80);
    if (VE.ops.some((o) => o.tool === 'cut')) errs.push('4030: Ctrl+Z on a German layout (code KeyY) did not undo');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'y', code: 'KeyZ', ctrlKey: true, bubbles: true, cancelable: true }));
    await sleep(80);
    if (!VE.ops.some((o) => o.tool === 'cut')) errs.push('4030: Ctrl+Y on a German layout (code KeyZ) did not redo');
    """)

    # 4031: an overlay committed without text goes away, the preview shows the exported apostrophe.
    failed += vprobe('emptytext', 'empty text overlay is removed, bar titles name the text', r"""
    // 4031: text overlay edge cases.
    const { addOverlay } = await import('./videoOverlays.js');
    const texts = () => VE.ops.filter((o) => o.tool === 'overlay' && o.kind === 'text');
    const field = () => [...document.querySelectorAll('#ve-ov-editor input.modal-input')].find((i) => i.value === (texts()[0] ? texts()[0].text : '\u0000'));
    const fire = (el, v) => { el.value = v; el.dispatchEvent(new Event('change', { bubbles: true })); };
    await addOverlay('text'); await sleep(150);
    if (texts().length !== 1) { errs.push('4031: setup gave ' + texts().length + ' text overlays'); return; }
    let f = field();
    if (!f) { errs.push('4031: the text field was not found'); return; }
    fire(f, '');
    await sleep(150);
    if (texts().length !== 0) errs.push('4031: a fresh text overlay committed empty is still there');
    if (document.querySelector('#ve-ov-list .ve-ov-item')) errs.push('4031: the list still shows a row after the empty commit');
    if (document.querySelector('#ve-regions .ve-ovbar')) errs.push('4031: the timeline still shows a bar after the empty commit');
    mod.undo(); await sleep(120);
    mod.undo(); await sleep(120);
    // Existing text: an empty commit restores the old text instead.
    await addOverlay('text'); await sleep(150);
    f = field();
    fire(f, "it's a very long overlay line"); await sleep(150);
    let op = texts()[0];
    if (!op || op.text !== "it's a very long overlay line") { errs.push('4031: the text edit gave ' + JSON.stringify(op && op.text)); return; }
    const bar = document.querySelector('#ve-regions .ve-ovbar');
    if (!bar || bar.title.indexOf("it's a very long ove") < 0 || bar.title.indexOf("it's a very long overl") >= 0) errs.push('4031: the bar title is "' + (bar && bar.title) + '"');
    const rowText = document.querySelector('#ve-ov-list .ve-ov-item').textContent;
    if (rowText.indexOf("it's a very long ove") < 0 || rowText.indexOf("very long overlay") >= 0) errs.push('4031: the list row reads "' + rowText + '"');
    const live = document.querySelector('.ve-live-text');
    if (live && live.textContent.indexOf('’') < 0) errs.push('4031: the preview text keeps the plain apostrophe: ' + live.textContent);
    if (!live) errs.push('4031: no live text element to check the apostrophe');
    f = field();
    fire(f, ''); await sleep(150);
    op = texts()[0];
    if (!op || op.text !== "it's a very long overlay line") errs.push('4031: an empty commit on an edited overlay changed it to ' + JSON.stringify(op && op.text));
    f = field();
    if (!f || f.value !== "it's a very long overlay line") errs.push('4031: the field did not get the old text back');
    """)

    # 4032: a picker that answers after another recording loaded adds nothing.
    failed += vprobe('stalepick', 'a late file picker answer is ignored after a reload', r"""
    // 4032: stale pick.
    const { addOverlay } = await import('./videoOverlays.js');
    const count = () => VE.ops.filter((o) => o.tool === 'overlay').length;
    window.__STUB_DELAY_MS__ = { pick_file_path: 300 };
    const p = addOverlay('image');
    await sleep(60);
    VE.loadGen += 1;
    await p;
    await sleep(100);
    if (count() !== 0) errs.push('4032: a stale image pick added ' + count() + ' overlay(s)');
    // The same pick without a reload in between still adds the image.
    const p2 = addOverlay('image');
    await p2;
    await sleep(150);
    if (count() !== 1) errs.push('4032: a current image pick added ' + count() + ' overlays');
    window.__STUB_DELAY_MS__ = {};
    """)

    # 4040: a saved default export scale is the starting state of every loaded source, not an edit.
    failed += vprobe('defscale', 'a saved default scale leaves the editor clean and applies to each source', r"""
    // 4040: default export scale.
    const scaleOps = () => VE.ops.filter((o) => o.tool === 'scale');
    if (scaleOps().length !== 1) errs.push('4040: the saved default scale was not applied (' + scaleOps().length + ' scale ops)');
    if (VE.dirty) errs.push('4040: applying the saved default marked the editor as edited');
    if (!document.getElementById('ve-undo').disabled) errs.push('4040: Undo is enabled for the saved default scale');
    const { loadRecording } = await import('./videoLoad.js');
    await loadRecording('C:/Temp/second.mp4', '', '');
    await sleep(300);
    if (scaleOps().length !== 1) errs.push('4040: a second recording got ' + scaleOps().length + ' scale ops, want the default');
    if (VE.dirty) errs.push('4040: the second recording starts out edited');
    """, pre='<script>window.__SEED_UI_TEXTS__ = {"videoExportScale": "720"};</script>\n')

    # 4041: a preset switches the format programmatically; a typed full destination follows its extension.
    failed += vprobe('presetext', 'a preset changes the extension of a typed destination', r"""
    // 4041: preset vs destination extension.
    const dest = document.getElementById('ve-export-dest');
    dest.value = 'D:/out/clip.mp4';
    dest.dispatchEvent(new Event('change', { bubbles: true }));
    document.getElementById('ve-preset-gif').click();
    await sleep(100);
    if (!/clip[.]gif$/.test(VE.destOverride || '')) errs.push('4041: after the GIF preset the destination is ' + VE.destOverride);
    document.getElementById('ve-preset-same').click();
    await sleep(100);
    if (!/clip[.]mp4$/.test(VE.destOverride || '')) errs.push('4041: after the same-quality preset the destination is ' + VE.destOverride);
    """)

    # 4042: switching fade off and on keeps a deliberate 0 on one side.
    failed += vprobe('fadezero', 'a fade side set to 0 stays 0 when fade is switched on again', r"""
    // 4042: fade zero.
    const on = document.getElementById('ve-fade-on');
    const fin = document.getElementById('ve-fade-in');
    const fout = document.getElementById('ve-fade-out');
    const fire = (el) => el.dispatchEvent(new Event('change', { bubbles: true }));
    on.checked = true; fire(on);
    fin.value = '0'; fire(fin);
    fout.value = '1000'; fire(fout);
    on.checked = false; fire(on);
    on.checked = true; fire(on);
    const fade = VE.ops.find((o) => o.tool === 'fade');
    if (!fade || fade.inMs !== 0 || fade.outMs !== 1000) errs.push('4042: fade came back as ' + JSON.stringify(fade));
    fin.value = ''; on.checked = false; fire(on); on.checked = true; fire(on);
    const again = VE.ops.find((o) => o.tool === 'fade');
    if (!again || again.inMs <= 0) errs.push('4042: an empty field should fall back to the default, got ' + JSON.stringify(again));
    """)

    # 4043: Escape in an edited field abandons the text without committing anything.
    failed += vprobe('escfield', 'Escape in an edited field adds no undo step and leaves the editor clean', r"""
    // 4043: Escape restore.
    const w = document.getElementById('ve-scale-w');
    w.value = '320';
    w.dispatchEvent(new Event('change', { bubbles: true }));
    if (!VE.ops.some((o) => o.tool === 'scale')) errs.push('4043: fixture: no scale op');
    VE.dirty = false;
    const before = JSON.stringify(VE.ops);
    const redoBefore = VE.redo.length;
    w.focus();
    const shown = w.value;
    w.value = '321';
    w.dispatchEvent(new Event('input', { bubbles: true }));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true, cancelable: true }));
    await sleep(100);
    if (w.value !== shown) errs.push('4043: the field kept the abandoned text: ' + w.value);
    if (JSON.stringify(VE.ops) !== before) errs.push('4043: Escape changed the operations: ' + JSON.stringify(VE.ops));
    if (VE.dirty) errs.push('4043: Escape marked the editor as edited');
    if (VE.redo.length !== redoBefore) errs.push('4043: Escape changed the redo stack');
    """)

    # 4044: after a checkbox was clicked, Ctrl+Z still undoes; Space on it stays the checkbox's own.
    failed += vprobe('boxkeys', 'editor shortcuts work while a checkbox has the focus', r"""
    // 4044: shortcuts with a checkbox focused.
    const w = document.getElementById('ve-scale-w');
    w.value = '320';
    w.dispatchEvent(new Event('change', { bubbles: true }));
    if (!VE.ops.some((o) => o.tool === 'scale')) errs.push('4044: fixture: no scale op');
    const box = document.getElementById('ve-crop-on');
    box.focus();
    if (document.activeElement !== box) errs.push('4044: fixture: the checkbox took no focus');
    const space = new KeyboardEvent('keydown', { key: ' ', code: 'Space', bubbles: true, cancelable: true });
    box.dispatchEvent(space);
    if (space.defaultPrevented) errs.push('4044: Space on a focused checkbox was taken from it');
    box.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', code: 'KeyZ', ctrlKey: true, bubbles: true, cancelable: true }));
    await sleep(60);
    if (VE.ops.some((o) => o.tool === 'scale')) errs.push('4044: Ctrl+Z did nothing with a checkbox focused');
    """)

    # The help dialog: F1 opens it, a section click swaps the text, the last section lists the registry.
    failed += vprobe('help', 'F1 opens the help, sections switch, shortcut list is complete', r"""
    const $ = (id) => document.getElementById(id);
    const { SHORTCUTS } = await import('./shortcuts.js');
    const modal = $('shortcuts-modal');
    if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
    if (!modal.classList.contains('hidden')) errs.push('help: the dialog starts open');
    if (!/\(F1\)$/.test($('ve-shortcuts').title)) errs.push('help: the button title does not end in (F1): ' + $('ve-shortcuts').title);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'F1', code: 'F1', bubbles: true, cancelable: true }));
    await sleep(60);
    if (modal.classList.contains('hidden')) errs.push('help: F1 did not open the dialog');
    const tabs = [...document.querySelectorAll('#ve-help-nav .ve-help-tab')];
    if (tabs.length !== 9) errs.push('help: ' + tabs.length + ' sections, want 9');
    if (tabs[tabs.length - 1].dataset.help !== 'keys') errs.push('help: the shortcut list is not the last section');
    const heading = () => { const h = document.querySelector('#shortcuts-modal-list .ve-help-h'); return h ? h.textContent : ''; };
    const first = heading();
    if (!first || /^veHelp/.test(first)) errs.push('help: first section has no heading: ' + first);
    if (document.querySelectorAll('#shortcuts-modal-list .ve-help-p').length < 2) errs.push('help: the first section shows fewer than two paragraphs');
    if (document.activeElement !== tabs[0]) errs.push('help: the first section button does not hold the focus');
    document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', code: 'ArrowDown', bubbles: true, cancelable: true }));
    await sleep(40);
    if (!tabs[1].classList.contains('active') || heading() === first) errs.push('help: ArrowDown did not move to the next section');
    tabs[4].click(); await sleep(40);
    if (tabs[4].getAttribute('aria-selected') !== 'true' || tabs[1].getAttribute('aria-selected') !== 'false') errs.push('help: aria-selected does not follow the click');
    if (!document.querySelector('#shortcuts-modal-list .ve-help-p')) errs.push('help: the overlays section has no text');
    tabs[8].click(); await sleep(40);
    const want = new Set(SHORTCUTS.filter((e) => e.scopes.includes('videoEditor') && !e.action.endsWith('Alt')).map((e) => e.descKey)).size;
    const rows = document.querySelectorAll('#shortcuts-modal-list .shortcuts-row');
    if (rows.length !== want || want < 15) errs.push('help: shortcut section has ' + rows.length + ' rows, registry has ' + want);
    const keysOf = [...rows].map((r) => r.querySelector('.shortcuts-row-keys').textContent);
    if (keysOf.indexOf('F1') < 0) errs.push('help: the list does not show F1');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'F1', code: 'F1', bubbles: true, cancelable: true }));
    await sleep(60);
    if (!modal.classList.contains('hidden')) errs.push('help: a second F1 did not close the dialog');
    $('ve-shortcuts').click(); await sleep(60);
    if (!tabs[0].classList.contains('active')) errs.push('help: reopening does not start at the first section');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true, cancelable: true }));
    await sleep(60);
    if (!modal.classList.contains('hidden')) errs.push('help: Escape did not close the dialog');
    """)
    return failed
