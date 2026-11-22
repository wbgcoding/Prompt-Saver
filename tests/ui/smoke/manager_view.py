"""Media manager large view: a click on a tile shows the file big in the same window, with
stepping, the edit and copy actions, delete that moves on, and the way back to the grid."""
from .arprobe import make_ar_probe

# Waits for the manager grid of the kind asked for.
BOOT = r"""
    const tiles = () => document.querySelectorAll('#mm-grid .mm-tile');
    const kindReady = async (k) => {
      await until(() => tiles().length > 0, 200);
      if (k === 'screenshots') window.__FIRE__('manager-kind', 'screenshots');
      await until(() => tiles().length > 0 && tiles()[0].dataset.name.startsWith(k === 'screenshots' ? 'Screenshot' : 'Recording'), 200);
      await sleep(150);
    };
    const viewOpen = () => !document.getElementById('mm-view').classList.contains('hidden');
    const name = () => document.getElementById('mm-view-name').textContent;
    const key = (k, target) => (target || document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }));
"""


def run(browser, tmp):
    ar_probe = make_ar_probe(browser, tmp)
    failed = 0
    # A click on a screenshot opens the large view in the window, with its picture, position and actions.
    failed += ar_probe('bmv01', 'manager: a click on a screenshot shows it large with its actions', 'manager', '880x600', '', BOOT + r"""
    await kindReady('screenshots');
    const second = tiles()[1].dataset.name;
    tiles()[1].click();
    await until(() => viewOpen(), 40);
    if (!viewOpen()) { errs.push('bmv01: the large view did not open'); return; }
    if (name() !== second) errs.push('bmv01: shows ' + name() + ' instead of ' + second);
    await until(() => document.querySelector('#mm-view-stage img'), 40);
    const img = document.querySelector('#mm-view-stage img');
    if (!img || !img.src.startsWith('data:')) errs.push('bmv01: no picture from media_preview_image in the stage');
    if (!callsFor('media_preview_image').some((a) => a.name === second)) errs.push('bmv01: media_preview_image was not asked for ' + second);
    if (!document.getElementById('mm-view-pos').textContent.includes('2')) errs.push('bmv01: position text is ' + document.getElementById('mm-view-pos').textContent);
    if (document.querySelector('.mm-toolbar').offsetParent || document.querySelector('.mm-scroll').offsetParent) errs.push('bmv01: the grid is still visible behind the large view');
    if (!document.getElementById('mm-view-player').classList.contains('hidden')) errs.push('bmv01: a screenshot offers the player button');
    if (callsFor('media_open_in_main').length) errs.push('bmv01: a plain click opened the editor');
    document.getElementById('mm-view-copy').click();
    await sleep(100);
    if (!callsFor('media_copy_image').some((a) => a.name === second)) errs.push('bmv01: Copy did not copy the picture');
    document.getElementById('mm-view-edit').click();
    await sleep(100);
    if (!callsFor('media_open_in_main').some((a) => a.name === second && a.kind === 'screenshots')) errs.push('bmv01: Edit did not open the screenshot in the editor');
    """)
    # Arrow keys step through the files, Escape goes back to the grid with the shown file focused.
    failed += ar_probe('bmv02', 'manager: arrows step through the large view, Escape returns to the grid', 'manager', '880x600', '', BOOT + r"""
    await kindReady('screenshots');
    const names = [...tiles()].map((t) => t.dataset.name);
    tiles()[0].click();
    await until(() => viewOpen(), 40);
    if (!document.getElementById('mm-view-prev').disabled) errs.push('bmv02: Previous is enabled on the first file');
    key('ArrowRight', document.body); await sleep(50);
    key('ArrowRight', document.body); await sleep(50);
    if (name() !== names[2]) errs.push('bmv02: two steps right show ' + name() + ', expected ' + names[2]);
    key('ArrowLeft', document.body); await sleep(50);
    if (name() !== names[1]) errs.push('bmv02: one step back shows ' + name());
    key('Escape', document.body); await sleep(100);
    if (viewOpen()) errs.push('bmv02: Escape did not close the large view');
    if (!document.querySelector('.mm-scroll').offsetParent) errs.push('bmv02: the grid did not come back');
    if (document.activeElement?.dataset?.name !== names[1]) errs.push('bmv02: focus is not on the last shown file');
    if (document.getElementById('mm-view-stage').childElementCount) errs.push('bmv02: the stage still holds content after closing');
    """)
    # A recording in a format the webview plays gets a video element; leaving the view drops its source.
    failed += ar_probe('bmv03', 'manager: a recording plays in the large view and lets go of its file on close', 'manager', '880x600', '', BOOT + r"""
    await kindReady('recordings');
    const mp4 = [...tiles()].find((t) => t.dataset.name.endsWith('.mp4'));
    const mkv = [...tiles()].find((t) => t.dataset.name.endsWith('.mkv'));
    mp4.click();
    await until(() => document.querySelector('#mm-view-stage video'), 40);
    const v = document.querySelector('#mm-view-stage video');
    if (!v) { errs.push('bmv03: no video element for an mp4'); return; }
    if (!v.controls) errs.push('bmv03: the video has no controls');
    if (document.getElementById('mm-view-player').classList.contains('hidden')) errs.push('bmv03: no player button for a recording');
    document.getElementById('mm-view-back').click();
    await sleep(100);
    if (v.getAttribute('src')) errs.push('bmv03: the video kept its source after closing');
    mkv.click();
    await until(() => document.querySelector('#mm-view-stage .mm-view-msg'), 40);
    if (!document.querySelector('#mm-view-stage .mm-view-msg button')) errs.push('bmv03: an mkv shows no way to open it in the player');
    """)
    # Delete in the large view moves the file away and shows the next one; the last one closes the view.
    failed += ar_probe('bmv04', 'manager: Delete in the large view moves on to the next file', 'manager', '880x600', '', BOOT + r"""
    await kindReady('screenshots');
    const names = [...tiles()].map((t) => t.dataset.name);
    const base = await window.__TAURI__.core.invoke('list_media', { kind: 'screenshots' });
    tiles()[0].click();
    await until(() => viewOpen(), 40);
    window.__MM_FILES__ = base.filter((f) => f.name !== names[0]);
    document.getElementById('mm-view-del').click();
    await until(() => name() === names[1], 60);
    if (!callsFor('media_discard').some((a) => (a.names || []).includes(names[0]))) errs.push('bmv04: Delete did not discard ' + names[0]);
    if (!viewOpen()) errs.push('bmv04: the view closed instead of moving on');
    if (name() !== names[1]) errs.push('bmv04: after Delete it shows ' + name() + ', expected ' + names[1]);
    window.__MM_FILES__ = [];
    document.getElementById('mm-view-del').click();
    await until(() => !viewOpen(), 60);
    if (viewOpen()) errs.push('bmv04: deleting the last file left the view open');
    """)
    # Opening a recording, and stepping to another one, starts playback.
    failed += ar_probe('bmv05', 'manager: a recording starts playing when opened and when stepped to', 'manager', '880x600', '', BOOT + r"""
    let plays = 0;
    HTMLMediaElement.prototype.play = function () { plays++; return Promise.resolve(); };
    await kindReady('recordings');
    const mp4s = [...tiles()].filter((t) => t.dataset.name.endsWith('.mp4'));
    mp4s[0].click();
    await until(() => document.querySelector('#mm-view-stage video'), 40);
    if (plays < 1) errs.push('bmv05: opening an mp4 did not call play()');
    const before = plays;
    document.getElementById('mm-view-next').click();
    await sleep(150);
    const nextName = document.getElementById('mm-view-name').textContent;
    if (!nextName.endsWith('.mp4')) errs.push('bmv05: the next file is ' + nextName + ', not a second mp4');
    if (plays < before + 1) errs.push('bmv05: stepping to a second mp4 did not call play()');
    """)
    # Wheel zooms the picture around the cursor, drag pans it, double click and 0 restore it, stepping resets it.
    failed += ar_probe('bmv06', 'manager: the large view zooms with the wheel, pans, and resets on double click and on stepping', 'manager', '880x600', '', BOOT + r"""
    await kindReady('screenshots');
    tiles()[0].click();
    await until(() => document.querySelector('#mm-view-stage img'), 40);
    const stage = document.getElementById('mm-view-stage');
    const img = () => document.querySelector('#mm-view-stage img');
    const scaleOf = (el) => { const m = /scale\(([0-9.]+)\)/.exec(el.style.transform || ''); return m ? parseFloat(m[1]) : 1; };
    const shift = (el) => { const m = /translate\((-?[0-9.]+)px, (-?[0-9.]+)px\)/.exec(el.style.transform || ''); return m ? [parseFloat(m[1]), parseFloat(m[2])] : [0, 0]; };
    const r = img().getBoundingClientRect();
    const cx = r.left + r.width * 0.75, cy = r.top + r.height * 0.5;
    const wheel = (dy) => img().dispatchEvent(new WheelEvent('wheel', { deltaY: dy, clientX: cx, clientY: cy, bubbles: true, cancelable: true }));
    if (scaleOf(img()) !== 1) errs.push('bmv06: the picture starts zoomed');
    wheel(-300);
    const z1 = scaleOf(img());
    if (!(z1 > 1.2)) errs.push('bmv06: wheel up did not zoom in, scale ' + z1);
    if (!stage.classList.contains('zoomed')) errs.push('bmv06: the stage is not marked zoomed (grab cursor)');
    wheel(-100000);
    if (scaleOf(img()) > 8.0001) errs.push('bmv06: zoom went past 8x: ' + scaleOf(img()));
    if (scaleOf(img()) < 7.9) errs.push('bmv06: zoom did not reach 8x: ' + scaleOf(img()));
    wheel(100);
    if (!(scaleOf(img()) < 8)) errs.push('bmv06: wheel down did not zoom out');
    const before = shift(img());
    const pointer = (type, x, y) => img().dispatchEvent(new PointerEvent(type, { pointerId: 7, button: 0, clientX: x, clientY: y, bubbles: true, cancelable: true }));
    pointer('pointerdown', cx, cy);
    pointer('pointermove', cx - 40, cy - 30);
    pointer('pointermove', cx - 60, cy - 50);
    // A wheel turn in the middle of the drag: the next move goes on from the new zoom.
    wheel(100);
    const mid = shift(img()), midScale = scaleOf(img());
    pointer('pointermove', cx - 64, cy - 50);
    const next = shift(img());
    if (scaleOf(img()) !== midScale) errs.push('bmv06: moving after a wheel turn mid-drag changed the zoom');
    if (Math.abs(next[0] - mid[0]) > 6 || Math.abs(next[1] - mid[1]) > 2) errs.push('bmv06: the picture jumped after a wheel turn mid-drag: ' + mid + ' -> ' + next);
    pointer('pointerup', cx - 64, cy - 50);
    const after = shift(img());
    if (before[0] === after[0] && before[1] === after[1]) errs.push('bmv06: dragging did not move the zoomed picture');
    if (stage.classList.contains('panning')) errs.push('bmv06: the stage still shows the panning cursor after the drag');
    img().dispatchEvent(new MouseEvent('dblclick', { clientX: cx, clientY: cy, bubbles: true, cancelable: true }));
    if (scaleOf(img()) !== 1 || img().style.transform) errs.push('bmv06: double click did not restore fit, transform ' + img().style.transform);
    img().dispatchEvent(new MouseEvent('dblclick', { clientX: cx, clientY: cy, bubbles: true, cancelable: true }));
    if (!(scaleOf(img()) >= 2)) errs.push('bmv06: double click at fit did not zoom in');
    key('0', document.body);
    if (scaleOf(img()) !== 1) errs.push('bmv06: key 0 did not restore fit');
    key('+', document.body);
    if (!(scaleOf(img()) > 1)) errs.push('bmv06: key + did not zoom in');
    key('-', document.body);
    if (scaleOf(img()) !== 1) errs.push('bmv06: key - did not zoom back out to fit');
    wheel(-300);
    if (!(scaleOf(img()) > 1)) errs.push('bmv06: could not zoom again before stepping');
    const here = name();
    key('ArrowRight', document.body);
    await until(() => name() !== here && document.querySelector('#mm-view-stage img'), 60);
    if (scaleOf(img()) !== 1 || img().style.transform) errs.push('bmv06: stepping kept the zoom, transform ' + img().style.transform);
    if (stage.classList.contains('zoomed')) errs.push('bmv06: stepping kept the zoomed cursor');
    wheel(-300);
    key('Escape', document.body);
    await sleep(100);
    tiles()[0].click();
    await until(() => document.querySelector('#mm-view-stage img'), 40);
    if (scaleOf(img()) !== 1) errs.push('bmv06: reopening the view kept the zoom');
    """)
    # A tile shows its date and its size and resolution as two single-line rows with the full text as tooltip.
    failed += ar_probe('bmv07', 'manager: a tile shows two fixed meta lines, single line with a tooltip', 'manager', '880x600', '', BOOT + r"""
    await kindReady('recordings');
    await sleep(600);
    const tile = tiles()[0];
    const rows = tile.querySelectorAll('.mm-meta > .mm-meta-line');
    if (rows.length !== 2) { errs.push('bmv07: expected 2 meta lines, found ' + rows.length); return; }
    for (const row of rows) {
      const cs = getComputedStyle(row);
      if (cs.whiteSpace !== 'nowrap' || cs.textOverflow !== 'ellipsis') errs.push('bmv07: a meta line can wrap: ' + cs.whiteSpace + ' ' + cs.textOverflow);
      if (!row.title || row.title !== row.textContent) errs.push('bmv07: a meta line has no tooltip with its full text');
      if (row.getBoundingClientRect().height > 20) errs.push('bmv07: a meta line is taller than one line');
    }
    if (/\d+\s?[x×]\s?\d+/.test(rows[0].textContent)) errs.push('bmv07: the resolution sits on the date line');
    const shot = [...tiles()].find((t) => /\d+×\d+/.test(t.querySelector('.mm-meta').textContent));
    if (shot && !/\d+×\d+/.test(shot.querySelectorAll('.mm-meta-line')[1].textContent)) errs.push('bmv07: the resolution is not on line 2');
    """)
    # A zoomed video hides its native controls (they would scale off the stage) and shows them again at fit.
    failed += ar_probe('bmv08', 'manager: a zoomed video hides its controls and gets them back at fit', 'manager', '880x600', '', BOOT + r"""
    await kindReady('recordings');
    const mp4 = [...tiles()].find((t) => t.dataset.name.endsWith('.mp4'));
    mp4.click();
    await until(() => document.querySelector('#mm-view-stage video'), 40);
    const vid = () => document.querySelector('#mm-view-stage video');
    const box = () => vid().getBoundingClientRect();
    const cx = box().left + box().width / 2, cy = box().top + 40;
    const wheel = (dy, y) => vid().dispatchEvent(new WheelEvent('wheel', { deltaY: dy, clientX: cx, clientY: y, bubbles: true, cancelable: true }));
    if (!vid().controls) errs.push('bmv08: a video at fit has no controls');
    wheel(-300, box().bottom - 10);
    if (vid().style.transform) errs.push('bmv08: the wheel over the control bar zoomed the video');
    wheel(-300, cy);
    if (!/scale\(/.test(vid().style.transform || '')) errs.push('bmv08: the wheel did not zoom the video');
    if (vid().controls) errs.push('bmv08: a zoomed video still shows its controls');
    key('0', document.body);
    if (vid().style.transform) errs.push('bmv08: key 0 did not restore fit');
    if (!vid().controls) errs.push('bmv08: the controls did not come back at fit');
    wheel(-300, cy);
    if (vid().controls) errs.push('bmv08: controls shown after zooming again');
    vid().dispatchEvent(new MouseEvent('dblclick', { clientX: cx, clientY: cy, bubbles: true, cancelable: true }));
    if (!vid().controls) errs.push('bmv08: double click back to fit left the controls off');
    wheel(-300, cy);
    document.getElementById('mm-view-next').click();
    if (!vid() || !vid().controls) errs.push('bmv08: the next video has no controls after stepping from a zoomed one');
    """)
    return failed
