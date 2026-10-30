"""Side-window suite: floating pill, snip, folder, vars, launcher, tray, recorder bar and camera windows,
each booted on its own with a probe injected."""
import json
import re
import shutil

from .harness import REPO, SIDE_SCAN, unescape, run_chrome


# F25 + F22: the pill window is the only place these two are observable. Both used to
# fail for the same kind of reason -- a drag that never reports its end, and settings
# that were read once when the window opened and never again.
FLOAT_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));""" + SIDE_SCAN + """
  const calls = () => (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []);
  const args = () => (window.__SMOKE_ARGS__ = window.__SMOKE_ARGS__ || []);
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(50); return fn(); };
  const pill = document.getElementById('pill');
  const down = (opts) => pill.dispatchEvent(new MouseEvent('mousedown', Object.assign(
    { bubbles: true, cancelable: true, button: 0, buttons: 1 }, opts)));
  const move = (opts) => window.dispatchEvent(new MouseEvent('mousemove', Object.assign(
    { bubbles: true, button: 0, buttons: 1 }, opts)));
  const up = () => window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, buttons: 0 }));
  try {
    // applyPrefs ends by sizing the window, so that call is the signal that the
    // settings are in place. Deliberately NOT the localized labels: those wait on a
    // lazily imported module, and behaviour must never be gated on it.
    await until(() => calls().includes('resize_float_pill'));
    if (!calls().includes('resize_float_pill')) errs.push('the pill never applied its settings');
    sideScan(errs, 'pill');
    // A headless window reports no inner size, so give the ring a known one.
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 240 });
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 160 });
    // and a quiet headless frame fires no animation frame, so run the handler on a timer.
    window.requestAnimationFrame = (fn) => setTimeout(fn, 0);
    window.dispatchEvent(new Event('resize'));
    await sleep(200);
    { const rz = parseFloat(getComputedStyle(document.getElementById('resize')).getPropertyValue('--rz'));
      if (Math.abs(rz - 29) > 0.5) errs.push('the pill resize ring is ' + rz + 'px, expected 29px'); }
    // The OS move loop startDragging() hands the window to CONSUMES the button
    // release, so a dragged pill never sees `mouseup`. The merge test therefore has
    // to run off the first later event that reports no button held.
    // Edge resize. Grabbing the ring starts a backend-driven drag (float_resize_start), which
    // reads the real cursor and does all geometry in physical pixels, so the page sends only
    // the grabbed edge. The tracking itself runs in the backend and is a VM check.
    args().length = 0;
    const cat = document.getElementById('rz-catch');
    // The faked 240x160 window: the middle third of its right side is the right edge.
    const cx = 236, cy = 80;
    cat.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, buttons: 1,
      clientX: cx, clientY: cy, screenX: cx, screenY: cy }));
    const starts = args().filter((c) => c[0] === 'float_resize_start').map((c) => c[1]);
    if (!pill.classList.contains('resizing')) {
      errs.push('grabbing the resize ring did not start a resize');
    } else if (!starts.length) {
      errs.push('grabbing the resize ring did not start the backend resize');
    } else {
      const a = starts[0];
      if (!(a.dirX === 1 && a.dirY === 0)) errs.push('the right edge grab sent direction ' + a.dirX + ',' + a.dirY);
      if (a.pill !== null) errs.push('a closed menu must not send a pill box');
    }
    up();
    await sleep(50);
    if (pill.classList.contains('resizing')) errs.push('the resize did not end on mouseup');

    // 41: folders must default ON even with folders ABSENT from ui_flags
    // (a fresh install) -- the stub otherwise ships it pre-set true, which cannot
    // tell an inverted default-off check apart from the correct default-on one
    // (both read true). undefined is the one value where they disagree.
    await window.__TAURI__.core.invoke('set_ui_flag', { key: 'folders', enabled: undefined });
    window.__FIRE__('settings-changed');
    await sleep(50);
    calls().length = 0;
    down({ screenX: 100, screenY: 100 });
    move({ screenX: 260, screenY: 180 });
    await until(() => pill.classList.contains('dragging'));
    if (!pill.classList.contains('dragging')) errs.push('a pill press-and-move did not start a window drag');
    if (calls().includes('float_folder_drop')) errs.push('the merge test ran while the pill was still being dragged');
    move({ screenX: 262, screenY: 182, buttons: 0 }); // the release, noticed late
    await until(() => calls().includes('float_folder_drop'));
    if (!calls().includes('float_folder_drop')) {
      errs.push('a finished pill drag never asked the backend to merge');
    }
    if (pill.classList.contains('dragging')) errs.push('the pill stayed marked as dragging');
    // A pill reads the expert settings when it opens, so a feature switched on later
    // only reaches it through `settings-changed` -- without that, drag-out (and the
    // merge above) stay dead on every pill already on screen.
    down({ screenX: 400, screenY: 400, ctrlKey: true });
    if (pill.draggable) errs.push('a pill armed a drag-out although the feature is off');
    up();
    await sleep(150);
    // Flip it exactly the way the main window does, then deliver the broadcast.
    await window.__TAURI__.core.invoke('set_ui_flag', { key: 'dragOut', enabled: true });
    window.__FIRE__('settings-changed');
    await until(() => { down({ screenX: 400, screenY: 400, ctrlKey: true }); const on = pill.draggable; up(); return on; });
    down({ screenX: 400, screenY: 400, ctrlKey: true });
    if (!pill.draggable) errs.push('switching drag-out on never reached a pill that was already open');
    else {
      const ev = new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: new DataTransfer() });
      pill.dispatchEvent(ev);
      if (!ev.dataTransfer.getData('text/plain')) errs.push('a pill drag-out carried no text');
    }
    up();
    // 43: "hide for now" - a distinct action from Remove, must never touch the
    // permanent toggle_floating command.
    calls().length = 0;
    document.getElementById('menu-hide').click();
    await until(() => calls().includes('hide_float_for_now'));
    if (!calls().includes('hide_float_for_now')) errs.push('menu-hide did not call hide_float_for_now');
    const hideArgs = args().find((c) => c[0] === 'hide_float_for_now');
    if (!hideArgs || hideArgs[1].id !== 'p0') errs.push('menu-hide sent the wrong id: ' + JSON.stringify(hideArgs));
    if (calls().includes('toggle_floating')) errs.push('menu-hide also called the permanent toggle_floating');

    // 427: a synthesised "copy" sound plays through the shared beep helper --
    // exactly once per successful copy, only while the switch is on, never at
    // volume 0. A stand-in AudioContext counts real oscillator starts.
    window.__BEEP_COUNT__ = 0;
    window.AudioContext = window.webkitAudioContext = function () {
      return {
        currentTime: 0,
        createOscillator: () => ({ frequency: { value: 0 }, connect() { return this; }, start() { window.__BEEP_COUNT__++; }, stop() {} }),
        createGain: () => ({ gain: { setValueAtTime() {}, linearRampToValueAtTime() {} }, connect() { return this; } }),
        destination: {},
      };
    };
    // Auto-paste and the copy cooldown belong to a different feature -- both would
    // otherwise swallow the rapid, scripted clicks this test needs.
    await window.__TAURI__.core.invoke('set_ui_flag', { key: 'autoPaste', enabled: false });
    await window.__TAURI__.core.invoke('set_ui_value', { key: 'copyCooldownMs', value: 0 });
    await window.__TAURI__.core.invoke('set_ui_flag', { key: 'copySound', enabled: true });
    window.__FIRE__('settings-changed');
    await sleep(50);
    down({ screenX: 100, screenY: 100 }); up();
    await until(() => window.__BEEP_COUNT__ === 1);
    if (window.__BEEP_COUNT__ !== 1) errs.push('427: copySound on did not play exactly one beep on copy: ' + window.__BEEP_COUNT__);
    down({ screenX: 100, screenY: 100 }); up();
    await until(() => window.__BEEP_COUNT__ === 2);
    if (window.__BEEP_COUNT__ !== 2) errs.push('427: a second copy did not play a second beep: ' + window.__BEEP_COUNT__);
    // Volume 0 must play nothing.
    await window.__TAURI__.core.invoke('set_ui_value', { key: 'copySoundVolume', value: 0 });
    window.__FIRE__('settings-changed');
    await sleep(50);
    down({ screenX: 100, screenY: 100 }); up();
    await sleep(150);
    if (window.__BEEP_COUNT__ !== 2) errs.push('427: volume 0 still played a beep: ' + window.__BEEP_COUNT__);
    // Switching the option off must play nothing either, even at a normal volume.
    await window.__TAURI__.core.invoke('set_ui_value', { key: 'copySoundVolume', value: 30 });
    await window.__TAURI__.core.invoke('set_ui_flag', { key: 'copySound', enabled: false });
    window.__FIRE__('settings-changed');
    await sleep(50);
    down({ screenX: 100, screenY: 100 }); up();
    await sleep(150);
    if (window.__BEEP_COUNT__ !== 2) errs.push('427: copySound off still played a beep: ' + window.__BEEP_COUNT__);
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# 851: a pill whose prompt carries a placeholder hands the copy to the main window
# (event "prompt-tray-copy") instead of copying the raw text itself; a plain pill still
# calls copy_prompt. The page names its prompt through __FLOAT_PROMPT_ID__.
FLOAT_VARS_PROBE = """
<script>
(async () => {
  // 851:
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const calls = () => (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []);
  const args = () => (window.__SMOKE_ARGS__ = window.__SMOKE_ARGS__ || []);
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(50); return fn(); };
  const id = window.__FLOAT_PROMPT_ID__;
  const pill = document.getElementById('pill');
  try {
    await until(() => calls().includes('resize_float_pill') && calls().includes('get_prompt'));
    await sleep(150);
    pill.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0, buttons: 1, screenX: 100, screenY: 100 }));
    window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, buttons: 0 }));
    await sleep(300);
    const emits = (window.__SMOKE_EMITS__ || []).filter((e) => e[0] === 'prompt-tray-copy');
    const copies = args().filter((c) => c[0] === 'copy_prompt');
    if (id === 'p1') {
      if (emits.length !== 1 || emits[0][1] !== 'p1') errs.push('851: a placeholder pill did not hand the copy to the main window: ' + JSON.stringify(window.__SMOKE_EMITS__));
      if (copies.length) errs.push('851: a placeholder pill copied the raw text itself: ' + JSON.stringify(copies));
    } else {
      if (copies.length !== 1 || copies[0][1].id !== id) errs.push('851: a plain pill did not call copy_prompt once: ' + JSON.stringify(copies));
      if (emits.length) errs.push('851: a plain pill handed its copy to the main window: ' + JSON.stringify(emits));
    }
  } catch (e) { errs.push('851: probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# F26: the colour picker runs inside the snip overlay, so this is the only place it
# is observable. A tiny frozen desktop of known pixels is handed to the overlay, then
# the loupe readout is checked against it in all three notations, plus the arrow-key
# nudge, the edge clamp, the click payload and Esc.
PICK_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(50); return fn(); };
  // 8x4 frozen desktop; row 0 is a known colour run, later rows rotate through it.
  const W = 8, H = 4;
  const COLORS = [[26,43,60],[255,0,0],[128,128,128],[255,128,0],[0,0,255],[0,255,0],[1,2,3],[255,255,255]];
  const rgb = new Uint8Array(W * H * 3);
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
    const c = COLORS[(x + y) % COLORS.length], i = (y * W + x) * 3;
    rgb[i] = c[0]; rgb[i + 1] = c[1]; rgb[i + 2] = c[2];
  }
  // The overlay background must decode, so paint the same pixels into a data URL.
  const cv = document.createElement('canvas');
  cv.width = W; cv.height = H;
  const ctx = cv.getContext('2d');
  const img = ctx.createImageData(W, H);
  for (let p = 0; p < W * H; p++) {
    img.data[p * 4] = rgb[p * 3]; img.data[p * 4 + 1] = rgb[p * 3 + 1];
    img.data[p * 4 + 2] = rgb[p * 3 + 2]; img.data[p * 4 + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
  const url = cv.toDataURL('image/png');

  let format = 'hex';
  const seen = [];
  const base = window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke = async (cmd, args) => {
    seen.push([cmd, args]);
    if (cmd === 'snip_should_begin') return true;
    if (cmd === 'snip_background') {
      return { src: url, is_file: false, width: W, height: H, pick: true, pick_zoom: 6, pick_format: format };
    }
    // The overlay caches ONE patch around the cursor instead of the whole desktop;
    // this mirrors the backend crop, including its refusal of a rect that runs off.
    if (cmd === 'pick_tile') {
      const { x, y, width, height } = args;
      if (x < 0 || y < 0 || x + width > W || y + height > H) return new Uint8Array(0).buffer;
      const out = new Uint8Array(width * height * 3);
      for (let r = 0; r < height; r++) for (let c = 0; c < width; c++) {
        const si = ((y + r) * W + (x + c)) * 3, di = (r * width + c) * 3;
        out[di] = rgb[si]; out[di + 1] = rgb[si + 1]; out[di + 2] = rgb[si + 2];
      }
      return out.buffer;
    }
    if (cmd === 'pick_color') return '#000000';
    return base(cmd, args);
  };
  const called = (cmd) => seen.some((c) => c[0] === cmd);
  const main = () => document.getElementById('pick-main').textContent;
  const alt = () => document.getElementById('pick-alt').textContent;
  const bg = () => document.getElementById('snip-bg').getBoundingClientRect();
  // Aim at the middle of image pixel (px, py) - the same ratio mapping the overlay
  // uses, which is what keeps it exact on a mixed-DPI desktop.
  const moveTo = (px, py) => {
    const r = bg();
    window.dispatchEvent(new MouseEvent('mousemove', { bubbles: true,
      clientX: r.left + (px + 0.5) * r.width / W, clientY: r.top + (py + 0.5) * r.height / H }));
  };
  const key = (k) => window.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true }));
  const session = async (fmt) => {
    format = fmt;
    window.__FIRE__('snip-begin');
    await until(() => document.body.classList.contains('pick'));
    await sleep(120);
  };
  const expect = async (want, what) => {
    const ok = await until(() => main() === want);
    if (!ok) errs.push(what + ': readout is "' + main() + '" instead of "' + want + '"');
  };

  try {
    await until(() => called('pick_tile'));
    if (!called('pick_tile')) errs.push('the picker never fetched a frozen patch');
    // Opening the picker must not pull the whole desktop across - that transfer is
    // what made it slow to start.
    if (called('pick_frame')) errs.push('the picker still transfers the whole frozen desktop');
    if (!document.body.classList.contains('pick')) errs.push('the overlay did not enter pick mode');
    moveTo(1, 0);
    await expect('#FF0000', 'hex');
    if (alt() !== 'rgb(255,0,0)') errs.push('the second readout line is "' + alt() + '" instead of rgb(255,0,0)');
    const loupe = document.getElementById('pick-loupe');
    if (loupe.classList.contains('hidden')) errs.push('the loupe stayed hidden');
    if (document.getElementById('pick-dot').classList.contains('hidden')) errs.push('the crosshair stayed hidden');
    const grid = document.getElementById('pick-grid');
    if (grid.width !== 15 * 6) errs.push('the loupe grid is ' + grid.width + 'px wide, not 15 cells at 6x');
    // The magnified grid must be centred on the picked pixel, not offset by a cell:
    // read the cells back and compare them with the frame they came from.
    const cell = (gx, gy) => {
      const d = grid.getContext('2d').getImageData(gx * 6 + 3, gy * 6 + 3, 1, 1).data;
      return [d[0], d[1], d[2]].join();
    };
    if (cell(7, 7) !== '255,0,0') errs.push('the centre cell of the loupe is ' + cell(7, 7) + ', not the picked pixel');
    if (cell(8, 7) !== '128,128,128') errs.push('the cell right of centre is ' + cell(8, 7) + ', not the next pixel');
    if (cell(6, 7) !== '26,43,60') errs.push('the cell left of centre is ' + cell(6, 7) + ', not the previous pixel');
    if (cell(7, 8) !== '128,128,128') errs.push('the cell below centre is ' + cell(7, 8) + ', not the pixel one row down');
    if (cell(7, 6) !== '0,0,0') errs.push('a cell above the desktop is ' + cell(7, 6) + ' instead of black');
    if (getComputedStyle(document.body).cursor !== 'none') errs.push('the OS cursor is still shown while picking');
    // The masks must be collapsed: a dimmed desktop would not match the colour read.
    if (document.getElementById('snip-mask-top').getBoundingClientRect().height > 0) {
      errs.push('the picker dimmed the desktop it is sampling');
    }
    // Arrow keys nudge exactly one pixel, and stop at the edge instead of wrapping.
    key('ArrowRight');
    await expect('#808080', 'one pixel right');
    key('ArrowLeft'); key('ArrowLeft');
    await expect('#1A2B3C', 'two pixels left');
    key('ArrowLeft');
    await expect('#1A2B3C', 'clamped at the left edge');
    key('ArrowUp');
    await expect('#1A2B3C', 'clamped at the top edge');
    // The hint has to name the controls, zoom included - the wheel is invisible
    // otherwise and the expert slider is not where anyone looks mid-pick.
    const hintTxt = document.getElementById('snip-hint').textContent;
    if (!hintTxt.includes('+/-')) errs.push('the picker hint never mentions zoom: "' + hintTxt + '"');

    // A shorter or longer value must NEVER resize the loupe - it used to jitter as
    // the sampled pixel changed. Measured at one zoom, across all three notations.
    const geo = [];
    const box = () => {
      const r = document.getElementById('pick-loupe').getBoundingClientRect();
      return Math.round(r.width) + 'x' + Math.round(r.height);
    };
    const measure = async (px, want, tag) => { moveTo(px, 0); await expect(want, tag); geo.push([tag, box()]); };
    await measure(6, '#010203', 'hex, dark pixel');
    await measure(7, '#FFFFFF', 'hex, white pixel');

    await session('rgb');
    moveTo(1, 0);
    await expect('rgb(255,0,0)', 'rgb notation');
    if (alt() !== '#FF0000') errs.push('rgb mode shows "' + alt() + '" instead of the hex line');
    await measure(6, 'rgb(1,2,3)', 'rgb, shortest value');
    await measure(7, 'rgb(255,255,255)', 'rgb, longest value');

    await session('hsl');
    moveTo(1, 0);
    await expect('hsl(0,100%,50%)', 'hsl notation');
    moveTo(2, 0);
    await expect('hsl(0,0%,50%)', 'hsl of pure grey');
    moveTo(0, 0);
    await expect('hsl(210,40%,17%)', 'hsl with a real hue');
    await measure(7, 'hsl(0,0%,100%)', 'hsl, short value');
    await measure(0, 'hsl(210,40%,17%)', 'hsl, long value');
    const sizes = [...new Set(geo.map((g) => g[1]))];
    if (sizes.length !== 1) errs.push('the loupe resizes with the value: ' + JSON.stringify(geo));

    // A click copies the pixel under the crosshair, by image coordinate.
    moveTo(3, 0);
    await expect('hsl(30,100%,50%)', 'before the click');
    window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 }));
    await until(() => called('pick_color'));
    const call = seen.filter((c) => c[0] === 'pick_color').pop();
    if (!call) errs.push('the click never copied a colour');
    else if (call[1].x !== 3 || call[1].y !== 0) {
      errs.push('the click copied pixel ' + call[1].x + ',' + call[1].y + ' instead of 3,0');
    }
    // Nothing about a pick may reach the prompts or the copy history.
    for (const forbidden of ['add_prompt', 'record_copy', 'copy_text', 'copy_prompt']) {
      if (called(forbidden)) errs.push('picking a colour called ' + forbidden);
    }

    // Zoom is reachable from the overlay itself: wheel, or +/- for anyone without one.
    // It starts at the setting and is clamped to the same 4-16x range as the slider.
    await session('hex');
    moveTo(1, 0);
    await expect('#FF0000', 'zoom session');
    const zoom = () => document.getElementById('pick-grid').width / 15;
    const atZoom = async (want, what) => {
      if (!await until(() => zoom() === want)) errs.push(what + ': ' + zoom() + 'x instead of ' + want + 'x');
    };
    await atZoom(6, 'the overlay did not start at the loupeZoom setting');
    const wheel = (dy, n) => {
      for (let i = 0; i < n; i++) {
        window.dispatchEvent(new WheelEvent('wheel', { deltaY: dy, bubbles: true, cancelable: true }));
      }
    };
    wheel(-100, 2);
    await atZoom(8, 'two wheel notches up');
    wheel(100, 3);
    await atZoom(5, 'three wheel notches down');
    wheel(100, 20);
    await atZoom(4, 'the wheel ran past the 4x floor');
    wheel(-100, 30);
    await atZoom(16, 'the wheel ran past the 16x ceiling');
    key('-'); key('-');
    await atZoom(14, 'the - key');
    key('+');
    await atZoom(15, 'the + key');
    // The zoom the user stops on is written back to the expert setting.
    const savedZoom = () => seen.filter((c) => c[0] === 'set_ui_value' && c[1] && c[1].key === 'loupeZoom').pop();
    if (!await until(() => savedZoom())) errs.push('the picker never persisted the zoom it ended on');
    else if (savedZoom()[1].value !== 15) {
      errs.push('the persisted zoom is ' + savedZoom()[1].value + ' instead of 15');
    }

    // A fresh session where the pointer never moves: a click must still take the
    // pixel under the cursor, not the seed the loupe opened on (desktop centre).
    await session('hex');
    await sleep(150); // let the seed's own patch land before the click
    window.dispatchEvent(new MouseEvent('mouseup', {
      bubbles: true, button: 0, clientX: bg().left + 3.5 * bg().width / W, clientY: bg().top + 0.5 * bg().height / H,
    }));
    await until(() => seen.filter((c) => c[0] === 'pick_color').length >= 2);
    const call2 = seen.filter((c) => c[0] === 'pick_color').pop();
    if (call2[1].x !== 3 || call2[1].y !== 0) {
      errs.push('a click with no prior pointer move copied ' + call2[1].x + ',' + call2[1].y + ' instead of the pixel under it (3,0)');
    }

    key('Escape');
    await until(() => called('snip_cancel'));
    if (!called('snip_cancel')) errs.push('Esc did not cancel the picker');
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# 501: a real drag over a highlighted window must still crop to the dragged rect,
# never fall back to capturing the whole window or the whole stitched canvas.
# Runs once at 1x and once at a simulated 150 percent display scale (the ratio
# mapping toImg uses is meant to be scale independent, so both must resolve the
# same rect). Shares snip.html with PICK_PROBE above: waits for its own #SMOKE
# element instead of creating a second one, then merges its findings into it.
REGION_PROBE = """
<script>
(() => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(50); return fn(); };
  // PICK_PROBE runs its own long walk on this same page first and only creates
  // #SMOKE at its very end; a plain re-assignment of window.__TAURI__.core.invoke
  // done only after that walk finishes would arrive too late, because snip.js
  // reads the invoke property exactly once, when its deferred module evaluates
  // (right after the document finishes parsing) -- long before either probe
  // script has finished its own async work. The last SYNCHRONOUS assignment
  // before that point wins, so this installs one stable dispatcher right now,
  // synchronously, and simply forwards to whatever was there before (PICK_PROBE
  // own wrapper) until this probe own session is actually ready to run.
  const priorInvoke = window.__TAURI__.core.invoke;
  let seen = null; // non-null only while a region session below is live
  let fullW = 0, fullH = 0;
  const cv = document.createElement("canvas");
  cv.width = 8; cv.height = 4;
  cv.getContext("2d").fillStyle = "#336699";
  cv.getContext("2d").fillRect(0, 0, 8, 4);
  const url = cv.toDataURL("image/png");
  window.__TAURI__.core.invoke = async (cmd, args) => {
    if (!seen) return priorInvoke(cmd, args);
    seen.push([cmd, args]);
    if (cmd === "snip_should_begin") return true;
    if (cmd === "snip_background") {
      return { src: url, is_file: false, width: fullW, height: fullH, pick: false, record: false };
    }
    // One maximized window covering the whole stitched canvas, topmost --
    // exactly what a real desktop hands back when a window fills the screen
    // under the drag.
    if (cmd === "snip_windows") return [{ id: 42, x: 0, y: 0, width: fullW, height: fullH }];
    if (cmd === "capture_region" || cmd === "capture_window") return null;
    return priorInvoke(cmd, args);
  };

  const bg = () => document.getElementById("snip-bg").getBoundingClientRect();
  const called = (cmd) => seen.some((c) => c[0] === cmd);

  const runSession = async (w, h, tag) => {
    fullW = w; fullH = h; seen = [];
    window.__FIRE__("snip-begin");
    await until(() => called("snip_background"));
    await sleep(200); // let snip_windows resolve too

    const r = bg();
    const sx = r.width / fullW, sy = r.height / fullH;
    const wantX = 200, wantY = 200, wantW = 800, wantH = 600;
    const cA = { x: r.left + wantX * sx, y: r.top + wantY * sy };
    const cMid = { x: r.left + 600 * sx, y: r.top + 500 * sy };
    const cB = { x: r.left + (wantX + wantW) * sx, y: r.top + (wantY + wantH) * sy };
    window.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0, clientX: cA.x, clientY: cA.y }));
    window.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: cMid.x, clientY: cMid.y }));
    window.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: cB.x, clientY: cB.y }));
    window.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, button: 0, clientX: cB.x, clientY: cB.y }));
    await until(() => called("capture_region") || called("capture_window"));
    const call = seen.filter((c) => c[0] === "capture_region" || c[0] === "capture_window").pop();
    if (!call) {
      errs.push(tag + ": no capture command was invoked after the drag over a window");
    } else if (call[0] !== "capture_region") {
      errs.push(tag + ": drag over a maximized window invoked " + call[0] + " instead of capture_region");
    } else {
      const a = call[1];
      const tol = 3;
      if (Math.abs(a.x - wantX) > tol || Math.abs(a.y - wantY) > tol ||
          Math.abs(a.width - wantW) > tol || Math.abs(a.height - wantH) > tol) {
        errs.push(tag + ": capture_region got x=" + a.x + " y=" + a.y + " width=" + a.width + " height=" + a.height +
          " instead of roughly x=" + wantX + " y=" + wantY + " width=" + wantW + " height=" + wantH);
      }
    }
  };

  const untilLong = async (fn) => { for (let i = 0; i < 500 && !fn(); i++) await sleep(50); return fn(); };
  (async () => {
    // Wait for PICK_PROBE own #SMOKE before this dispatcher starts intercepting
    // (seen stays null, so every call above still forwards to priorInvoke), so
    // its long walk keeps seeing exactly the responses it expects.
    await untilLong(() => document.getElementById("SMOKE"));
    try {
      await runSession(1280, 800, "1x scale");
      await runSession(1920, 1200, "150 percent scale");
    } catch (e) { errs.push("region probe: " + e.message); }
    seen = null; // stop intercepting, back to pure pass-through

    const prior = document.getElementById("SMOKE");
    if (prior) {
      let merged = errs;
      try { merged = JSON.parse(prior.textContent).concat(errs); } catch (e) { /* keep just this run own errs */ }
      prior.textContent = JSON.stringify(merged);
    } else {
      const pre = document.createElement("pre");
      pre.id = "SMOKE";
      pre.textContent = JSON.stringify(errs);
      document.body.appendChild(pre);
    }
  })();
})();
</script>
"""


# F25: the folder window is the only place its UI is observable: the closed icon
# (previews + count + label), the grid the window grows into, and Esc collapsing it.
FOLDER_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));""" + SIDE_SCAN + """
  const calls = () => window.__SMOKE_CALLS__ || [];
  const until = async (fn) => { for (let i = 0; i < 80 && !fn(); i++) await sleep(100); return fn(); };
  try {
    // The window loads its state and its translations asynchronously. Wait on the
    // label, which renderIcon fills in the same pass as the previews.
    const ready = await until(() => document.getElementById('fname').textContent !== '');
    if (!ready) errs.push('the folder window never rendered its state');
    const faces = document.querySelectorAll('#preview .folder-face').length;
    if (faces !== 4) errs.push('the closed folder shows ' + faces + ' previews instead of 4');
    // No count badge anywhere on a folder: the member count lives in the tooltip.
    if (document.getElementById('badge') || document.querySelector('.folder-badge')) {
      errs.push('the folder icon still carries a count badge');
    }
    // With the badge gone the tooltip is the only place the folder says what is in
    // it, so it has to name the folder AND its members.
    const iconEl = document.getElementById('icon');
    const tip = (iconEl && iconEl.title) || '';
    if (!tip.includes('Drafts') || !tip.includes('Prompt 1')) {
      errs.push('the folder tooltip does not describe its contents: ' + tip);
    }
    const name = document.getElementById('fname');
    if (name.classList.contains('hidden') || name.textContent !== 'Drafts') {
      errs.push('the folder label is missing: ' + name.textContent);
    }
    sideScan(errs, 'folder, closed');
    // Free resize from ANY edge, exactly like a pill: nothing is drawn for it, so the
    // ring is what this proves. It must change the size and must NOT open the folder
    // (the icon's own press is the open gesture), and the centre must stay free -
    // rz-hole covers it, or dragging the middle would resize instead of moving.
    const grip = document.getElementById('rz-catch');
    const hole = document.getElementById('rz-hole');
    if (!grip || !hole) {
      errs.push('the folder has no resize ring');
    } else {
      const ring = getComputedStyle(document.getElementById('resize')).getPropertyValue('--rz').trim();
      if (!ring || parseFloat(ring) < 8) errs.push('the resize ring has no usable thickness: ' + ring);
      const before = getComputedStyle(document.body).getPropertyValue('--folder-box').trim();
      grip.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, screenX: 100, screenY: 100 }));
      window.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, buttons: 1, screenX: 148, screenY: 148 }));
      await sleep(60);
      const during = getComputedStyle(document.body).getPropertyValue('--folder-box').trim();
      if (during === before) errs.push('dragging the resize grip did not change the folder size (' + before + ')');
      // A mouse fires far more often than the icon can usefully be redrawn, and a
      // redraw rebuilds every preview face. A burst of moves inside one frame has to
      // collapse into a single repaint - the icon is watched through the ring width,
      // which only renderIcon ever writes.
      let paints = 0;
      const obs = new MutationObserver((recs) => { paints += recs.length; });
      obs.observe(document.body, { attributes: true, attributeFilter: ['style'] });
      for (let i = 1; i <= 50; i++) {
        window.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, buttons: 1, screenX: 148 + i * 2, screenY: 148 + i * 2 }));
      }
      await sleep(60);
      obs.disconnect();
      if (paints > 4) errs.push('50 resize moves repainted the folder ' + paints + ' times');
      if (paints < 1) errs.push('50 resize moves never repainted the folder at all');
      window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 }));
      await sleep(60);
      // Skipping repaints is only allowed while the grab lasts: what stays on screen
      // afterwards has to be the size that was stored, not the last one drawn.
      const sent = (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'set_float_folder_scale').pop();
      const settled = getComputedStyle(document.body).getPropertyValue('--folder-box').trim();
      if (!sent) errs.push('the resize never reached the backend');
      else if (settled !== Math.round(96 * sent[1].scale) + 'px') {
        errs.push('the released folder kept a stale size: ' + settled + ' for scale ' + sent[1].scale);
      }
      if (!document.getElementById('pop').classList.contains('hidden')) {
        errs.push('resizing the folder opened it as well');
      }
    }
    // Opening: press and release on the icon, the gesture a click really is here.
    const icon = document.getElementById('icon');
    icon.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
    window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 }));
    const pop = document.getElementById('pop');
    await until(() => !pop.classList.contains('hidden'));
    if (pop.classList.contains('hidden')) errs.push('clicking the folder did not open it');
    if (!calls().includes('expand_float_folder')) errs.push('opening the folder never resized its window');
    if (!document.body.classList.contains('pop-open')) errs.push('the open folder is not marked open');
    const tiles = [...document.querySelectorAll('#grid .folder-member')];
    if (tiles.length !== 5) errs.push('the open folder shows ' + tiles.length + ' members instead of 5');
    sideScan(errs, 'folder, open');
    { const ex = (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'expand_float_folder').pop();
      const gs = getComputedStyle(document.getElementById('grid'));
      // Measured before the tile and popover spacing became fixed values: at this
      // folder scale the popover gap was 15.25px, the grid padding 25px, the tile gap 20px.
      if (!ex) errs.push('the open folder never asked for its popover gap');
      else if (Math.abs(ex[1].gap - 15.25) > 0.5) errs.push('the folder popover gap is ' + ex[1].gap + 'px, expected 15.25px');
      if (Math.abs(parseFloat(gs.paddingTop) - 25) > 0.5) errs.push('the folder grid padding is ' + gs.paddingTop + ', expected 25px');
      if (Math.abs(parseFloat(gs.rowGap) - 20) > 0.5) errs.push('the folder tile gap is ' + gs.rowGap + ', expected 20px'); }
    if (!tiles.length) {
      errs.push('the open folder rendered no members at all');
    } else {
      if (!tiles[0].title) errs.push('a member tile carries no tooltip');
      if (!tiles[0].querySelector('.member-ok')) errs.push('a member tile has no copy feedback');
      if (!tiles[0].querySelector('.member-face')) errs.push('a member tile renders no face');
      // A member click copies through the same path a pill uses.
      tiles[0].dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
      window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, clientX: 10, clientY: 10 }));
      await until(() => calls().includes('record_copy'));
      if (!calls().includes('copy_prompt')) errs.push('clicking a member did not copy it');
      if (!calls().includes('record_copy')) errs.push('a copy from a folder was not recorded');

      // 427: same beep helper, same "exactly one per copy, only while on, never
      // at volume 0" contract as a plain pill. clickMember() re-queries the tile
      // by id on every call rather than reusing `tiles[0]` -- each settings-changed
      // fire below reaches folder.js's own listener, which reloads state and (the
      // grid being open) calls renderGrid() again, detaching the old tile node; a
      // dispatchEvent on a detached node never bubbles to the delegated listener,
      // so a stale reference would silently swallow every click here. Outlast the
      // 400ms auto-paste double-click window too -- the member click just above
      // leaves lastClick[id] fresh, and clicking again inside that window pastes
      // instead of copying.
      const memberId = tiles[0].dataset.id;
      await sleep(450);
      window.__BEEP_COUNT__ = 0;
      window.AudioContext = window.webkitAudioContext = function () {
        return {
          currentTime: 0,
          createOscillator: () => ({ frequency: { value: 0 }, connect() { return this; }, start() { window.__BEEP_COUNT__++; }, stop() {} }),
          createGain: () => ({ gain: { setValueAtTime() {}, linearRampToValueAtTime() {} }, connect() { return this; } }),
          destination: {},
        };
      };
      const clickMember = () => {
        const t = document.querySelector(`#grid .folder-member[data-id="${CSS.escape(memberId)}"]`);
        if (!t) { errs.push('427: the member tile disappeared mid-test'); return; }
        t.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
        window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, clientX: 10, clientY: 10 }));
      };
      await window.__TAURI__.core.invoke('set_ui_flag', { key: 'autoPaste', enabled: false });
      await window.__TAURI__.core.invoke('set_ui_value', { key: 'copyCooldownMs', value: 0 });
      await window.__TAURI__.core.invoke('set_ui_flag', { key: 'copySound', enabled: true });
      window.__FIRE__('settings-changed');
      await sleep(50);
      clickMember();
      await until(() => window.__BEEP_COUNT__ === 1);
      if (window.__BEEP_COUNT__ !== 1) errs.push('427: copySound on did not play a beep on a folder-member copy: ' + window.__BEEP_COUNT__);
      await window.__TAURI__.core.invoke('set_ui_value', { key: 'copySoundVolume', value: 0 });
      window.__FIRE__('settings-changed');
      await sleep(50);
      clickMember();
      await sleep(150);
      if (window.__BEEP_COUNT__ !== 1) errs.push('427: volume 0 still played a beep in a folder: ' + window.__BEEP_COUNT__);
      await window.__TAURI__.core.invoke('set_ui_value', { key: 'copySoundVolume', value: 30 });
      await window.__TAURI__.core.invoke('set_ui_flag', { key: 'copySound', enabled: false });
      window.__FIRE__('settings-changed');
      await sleep(50);
      clickMember();
      await sleep(150);
      if (window.__BEEP_COUNT__ !== 1) errs.push('427: copySound off still played a beep in a folder: ' + window.__BEEP_COUNT__);
    }
    // 851-folder: a member whose prompt carries a placeholder hands the copy to the main
    // window; no raw-text copy_prompt for it. (p1 is such a member, p0 stayed plain above.)
    { const t1 = document.querySelector('#grid .folder-member[data-id="p1"]');
      if (!t1) {
        errs.push('851-folder: the placeholder member is missing');
      } else {
        const before = (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'copy_prompt' && c[1].id === 'p1').length;
        window.__SMOKE_EMITS__ = [];
        t1.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
        window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, clientX: 10, clientY: 10 }));
        await sleep(300);
        const after = (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'copy_prompt' && c[1].id === 'p1').length;
        const em = (window.__SMOKE_EMITS__ || []).filter((e) => e[0] === 'prompt-tray-copy');
        if (after !== before) errs.push('851-folder: a placeholder member copied the raw text itself');
        if (em.length !== 1 || em[0][1] !== 'p1') errs.push('851-folder: a placeholder member did not hand the copy to the main window: ' + JSON.stringify(window.__SMOKE_EMITS__));
      } }
    // 43: "hide for now" - a distinct action from Dissolve, must never touch the
    // permanent dissolve_float_folder command.
    window.__SMOKE_CALLS__.length = 0;
    document.getElementById('menu-hide').click();
    await until(() => calls().includes('hide_float_for_now'));
    if (!calls().includes('hide_float_for_now')) errs.push('menu-hide did not call hide_float_for_now');
    const hideArgs = (window.__SMOKE_ARGS__ || []).find((c) => c[0] === 'hide_float_for_now');
    if (!hideArgs || hideArgs[1].id !== 'f1') errs.push('menu-hide sent the wrong id: ' + JSON.stringify(hideArgs));
    if (calls().includes('dissolve_float_folder')) errs.push('menu-hide also called the permanent dissolve_float_folder');
    // Esc collapses it again and shrinks the window back to the icon.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await until(() => pop.classList.contains('hidden'));
    if (!pop.classList.contains('hidden')) errs.push('Esc did not collapse the folder');
    if (document.body.classList.contains('pop-open')) errs.push('the collapsed folder is still marked open');
    if (!calls().includes('collapse_float_folder')) errs.push('collapsing never shrank the window back');
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# The standalone fill-in window: it renders the rows the main window handed over,
# answers with what was typed, and treats Esc / a click outside as a cancel.
VARS_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    await sleep(300);
    const rows = [...document.querySelectorAll('#fields .vars-row')];
    if (rows.length !== 2) errs.push('the window rendered ' + rows.length + ' rows instead of 2');
    if (!document.getElementById('title').textContent.trim()) errs.push('the window shows no title');
    if (!document.getElementById('ok').textContent.trim()) errs.push('the confirm button has no label');
    if (!document.getElementById('cancel').textContent.trim()) errs.push('the cancel button has no label');
    const text = rows[0] && rows[0].querySelector('input[type=text]');
    if (!text) errs.push('the free-text row has no input');
    else if (text.value !== 'roadmap') errs.push('the remembered value was not filled in: ' + text.value);
    if (!rows[0] || !rows[0].querySelector('input[type=checkbox]')) {
      errs.push('the free-text row has no remember switch');
    }
    // The window asks to be exactly as tall as its form: a request smaller than the
    // content leaves a scrollbar on a form that would fit on screen.
    const fits = (window.__SMOKE_ARGS__ || []).filter(([c]) => c === 'var_window_fit');
    const fields = document.getElementById('fields');
    // Measured independently of vars.js: the list at its natural height, unclipped.
    const boxH = fields.clientHeight;
    fields.style.flex = 'none';
    fields.style.overflow = 'visible';
    const needed = window.innerHeight - boxH + fields.offsetHeight;
    fields.style.flex = '';
    fields.style.overflow = '';
    if (!fits.length) errs.push('the window never asked to fit its content');
    else if (!(fits[fits.length - 1][1].height >= needed)) {
      errs.push('the fit request ' + fits[fits.length - 1][1].height + ' is shorter than the form (' + needed + ')');
    }
    // The choice row must NOT be a native <select> - its OS popup would blur (and
    // therefore cancel) this window.
    if (rows[1] && rows[1].querySelector('select')) errs.push('a choice row uses a native select');
    const choice = rows[1] && rows[1].querySelector('.vars-choice-btn');
    const list = rows[1] && rows[1].querySelector('.vars-choice-list');
    if (!choice || !list) {
      errs.push('the choice row has no in-window dropdown');
    } else {
      if (choice.textContent !== 'casual') errs.push('the choice row did not preselect its value');
      choice.click();
      await sleep(120);
      if (list.classList.contains('hidden')) errs.push('the choice dropdown did not open');
      const items = [...list.querySelectorAll('.vars-choice-item')];
      if (items.map(i => i.textContent).join(',') !== 'formal,casual,witty') {
        errs.push('the choice dropdown lists the wrong options');
      }
      items[2] && items[2].click();
      await sleep(120);
      if (!list.classList.contains('hidden')) errs.push('picking an option left the dropdown open');
      if (choice.textContent !== 'witty') errs.push('the pick did not land in the button');
      // 58: Enter landing on a choice control (the button, or a focused item in an
      // open list) must not be hijacked into an immediate whole-window confirm --
      // it belongs to that control alone (open the list / pick the item).
      choice.click();
      await sleep(120);
      window.__VAR_RESULT__ = undefined;
      const firstItem = list.querySelector('.vars-choice-item');
      (firstItem || choice).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      await sleep(80);
      if (window.__VAR_RESULT__ !== undefined) {
        errs.push('Enter on a choice control confirmed the whole window instead of staying local to it');
      }
      if (!list.classList.contains('hidden')) choice.click(); // close it again for what follows
      await sleep(80);
    }
    if (text) text.value = 'Rust';
    // Nothing may have been sent before the user confirms.
    if (window.__VAR_RESULT__ !== undefined) errs.push('the window answered before it was confirmed');
    document.getElementById('ok').click();
    await sleep(200);
    let answer = null;
    try { answer = JSON.parse(window.__VAR_RESULT__ || 'null'); } catch (e) { answer = null; }
    if (!answer) errs.push('confirming sent no answer');
    else {
      if (answer.values.join('|') !== 'Rust|witty') errs.push('the answer carries ' + answer.values.join('|'));
      if (answer.remember[0] !== true) errs.push('the remember switch was not reported');
    }
    // One answer only: Esc after a confirm must not send a cancel on top of it.
    window.__VAR_RESULT__ = 'SENTINEL';
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    window.dispatchEvent(new Event('blur'));
    await sleep(150);
    if (window.__VAR_RESULT__ !== 'SENTINEL') errs.push('the window answered a second time');
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


LAUNCHER_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    await sleep(500);
    const search = document.getElementById('search');
    if (!search.placeholder.trim()) errs.push('the search field has no placeholder');
    const rows = () => [...document.querySelectorAll('#results .launcher-row')];
    if (!rows().length) errs.push('the empty-query default list shows nothing');
    if (rows().length > 8) errs.push('the default list is not capped at 8');
    // 944-dg: a footer names the launcher's keys, built from the shortcut table,
    // and it sits fully inside the card at the launcher's own size.
    {
      const keys = document.getElementById('keys');
      const kbds = keys ? [...keys.querySelectorAll('kbd')].map((k) => k.textContent) : [];
      const arrows = String.fromCharCode(0x2191, 0x2193);
      if (kbds.join('|') !== 'Enter|' + arrows + '|Esc') errs.push('944-dg: the footer keys read ' + JSON.stringify(kbds));
      if (keys) {
        const kr = keys.getBoundingClientRect();
        const cr = document.getElementById('card').getBoundingClientRect();
        if (kr.height <= 0 || kr.left < cr.left || kr.right > cr.right || kr.bottom > cr.bottom + 0.5) errs.push('944-dg: the footer is not fully inside the card');
        const items = [...keys.children];
        if (items.length !== 3 || items.some((s) => s.textContent.trim().length <= s.querySelector('kbd').textContent.length)) errs.push('944-dg: a footer key has no word next to it');
      }
    }
    // A query nothing matches shows the empty-state text, not a stale row.
    search.value = 'zzzznomatch';
    search.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(50);
    if (rows().length) errs.push('a query with no matches still shows rows');
    const empty = document.getElementById('empty');
    if (empty.classList.contains('hidden') || !empty.textContent.trim()) {
      errs.push('the no-matches state has no visible message');
    }
    // p1's TEXT (not its name) carries the only "Translate" in the fixture, so a
    // hit here proves the body is searched too, not just the name.
    search.value = 'Translate';
    search.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(50);
    if (rows().length !== 1) errs.push('searching "Translate" found ' + rows().length + ' rows instead of 1');
    else if (rows()[0].dataset.id !== 'p1') errs.push('the wrong prompt matched: ' + rows()[0].dataset.id);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await sleep(50);
    if (window.__LAUNCHER_SELECTED__ !== 'p1') {
      errs.push('Enter did not select the highlighted row: ' + window.__LAUNCHER_SELECTED__);
    }
    // "One action ever" - same guard vars.js uses (see its own SENTINEL check): a
    // stray Escape after a real selection must never ALSO fire launcher_close.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await sleep(50);
    if (window.__LAUNCHER_CLOSED__) errs.push('closing fired even though a selection already had');
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# The tray filter row opens the launcher with its text: the stub's launcher_items
# carries a prefill, which must land in the search field with the caret at the end.
LAUNCHER_PREFILL_PROBE = """
<script>
window.__LAUNCHER_PREFILL__ = 'Translate';
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    await sleep(500);
    const search = document.getElementById('search');
    if (search.value !== 'Translate') errs.push('the prefill did not reach the search field: ' + search.value);
    const rows = [...document.querySelectorAll('#results .launcher-row')];
    if (rows.length !== 1 || rows[0].dataset.id !== 'p1') errs.push('the prefilled query did not filter the list: ' + rows.length + ' rows');
    if (search.selectionStart !== 9 || search.selectionEnd !== 9) errs.push('the caret is not at the end of the prefill');
    // Characters the tray passes on after this window is up replace the starting query.
    window.__FIRE__('launcher-prefill', 'Translate it');
    await sleep(60);
    if (search.value !== 'Translate it') errs.push('a late prefill did not reach the search field: ' + search.value);
    if (search.selectionStart !== 12) errs.push('the caret is not at the end after a late prefill');
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# A prompt written in Markdown shows its words, not its marks, in the launcher's
# one-line preview.
LAUNCHER_MD_STUB = """
<script>
const mdInvoke = window.__TAURI__.core.invoke;
window.__TAURI__.core.invoke = async (cmd, args) => {
  const r = await mdInvoke(cmd, args);
  if (cmd === 'launcher_items') {
    const nl = String.fromCharCode(10);
    r.prompts = [{ id: 'pmd', name: 'Markdown prompt', text: ['# Heading one', '## Heading two with **bold** text'].join(nl), color: '' }].concat(r.prompts);
  }
  return r;
};
</script>
"""


LAUNCHER_MD_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    await sleep(500);
    const search = document.getElementById('search');
    search.value = 'Markdown prompt';
    search.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(60);
    // 943-dg: the preview line carries no Markdown marks.
    const prev = document.querySelector('#results .launcher-row[data-id="pmd"] .launcher-row-preview');
    if (!prev) errs.push('943-dg: the Markdown prompt has no preview line');
    else {
      if (/#|[*][*]/.test(prev.textContent)) errs.push('943-dg: the preview still shows Markdown marks: ' + prev.textContent);
      if (prev.textContent !== 'Heading one Heading two with bold text') errs.push('943-dg: unexpected preview text: ' + prev.textContent);
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# The custom Windows-11-style tray menu: rendering, keyboard nav and
# activation. The Rust-side content (favorites/recent selection, ordering, caps)
# has its own unit tests -- this only proves the window draws what it is given
# and responds to the keyboard the same way the launcher and vars windows do.
TRAY_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    await sleep(300);
    const rows = () => [...document.querySelectorAll('#card .tray-menu-row')];
    if (rows().length !== 5) errs.push('expected 5 rows (3 entries + show + quit), found ' + rows().length);
    const heads = [...document.querySelectorAll('#card .tray-menu-head')];
    if (heads.length !== 2) errs.push('expected 2 section headers, found ' + heads.length);
    const seps = [...document.querySelectorAll('#card .tray-menu-sep')];
    if (seps.length !== 3) errs.push('expected 3 separators (2 section separators plus the one above Quit), found ' + seps.length);
    // 945-dg: Open and Quit are set apart, Quit last behind its own line, both with
    // a plain glyph; the content is not clipped by the window.
    {
      const all = rows();
      const last = all[all.length - 1];
      if (!last || last.dataset.id !== 'quit') errs.push('945-dg: the last row is not Quit');
      else if (!last.querySelector('.tray-menu-action-icon')) errs.push('945-dg: Quit has no action icon');
      const lastSep = seps[seps.length - 1];
      const before = lastSep && lastSep.previousElementSibling;
      if (!before || before.dataset.id !== 'show') errs.push('945-dg: the row above the last separator is not Open');
      else if (!before.querySelector('.tray-menu-action-icon')) errs.push('945-dg: Open has no action icon');
      if (lastSep && lastSep.nextElementSibling !== last) errs.push('945-dg: Quit does not follow the last separator directly');
      const card = document.getElementById('card');
      if (card.scrollHeight > card.clientHeight + 1) errs.push('945-dg: the tray content is clipped (' + card.scrollHeight + ' > ' + card.clientHeight + ')');
    }
    if (!rows()[0].classList.contains('active')) errs.push('the first row is not highlighted on open');
    // Every entry row (not show/quit) carries a colored icon chip, and the one
    // entry with its own tile color must use it instead of the kind's tint.
    const icons = rows().slice(0, 3).map((r) => r.querySelector('.tray-menu-icon'));
    if (icons.some((i) => !i)) errs.push('an entry row has no icon chip');
    else if (!/59,\\s*130,\\s*246/.test(icons[1].style.background)) {
      errs.push('the entry with a custom color did not use it: ' + icons[1].style.background);
    }
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    await sleep(30);
    if (!rows()[1].classList.contains('active')) errs.push('ArrowDown did not move the highlight to row 1');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
    await sleep(30);
    if (!rows()[0].classList.contains('active')) errs.push('ArrowUp did not move the highlight back to row 0');
    // Enter on a fixed row still goes through the plain activation.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await sleep(30);
    if (window.__TRAY_ACTIVATED__ !== 'tray-copy-p1') {
      errs.push('Enter on a fixed row did not go through tray_window_activate: ' + window.__TRAY_ACTIVATED__);
    }
    // Same "one action ever" guard the launcher and vars windows use: a stray
    // Escape after a real activation must never ALSO fire tray_window_close.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await sleep(30);
    if (window.__TRAY_CLOSED__) errs.push('closing fired even though an activation already had');
    // The filter row at the very top hands the first keystroke to the quick
    // launcher: what was typed travels along, the tray list itself stays as it was.
    // Characters typed before the launcher has focus are passed on once, not lost,
    // and never start a second hand-off.
    const field = document.getElementById('tray-search');
    if (!field || field.hidden) errs.push('the search field is missing or hidden while traySearch is on');
    field.value = 'alpha';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    field.value = 'alphabet';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(80);
    if (window.__TRAY_LAUNCHER_QUERY__ !== 'alpha') {
      errs.push('typing in the tray filter did not hand the text to the launcher: ' + window.__TRAY_LAUNCHER_QUERY__);
    }
    if (window.__TRAY_LAUNCHER_CALLS__ !== 1) errs.push('typing twice started ' + window.__TRAY_LAUNCHER_CALLS__ + ' hand-offs instead of 1');
    // The text is held back while the tray still owns the focus, and passed on in one
    // piece the moment the launcher takes the focus (the tray blurs).
    if (window.__TRAY_LAUNCHER_PREFILL__ !== undefined) {
      errs.push('the tray passed its text on before the launcher took the focus: ' + window.__TRAY_LAUNCHER_PREFILL__);
    }
    window.dispatchEvent(new Event('blur'));
    await sleep(60);
    if (window.__TRAY_LAUNCHER_PREFILL__ !== 'alphabet') {
      errs.push('the characters typed during the hand-off were not passed on: ' + window.__TRAY_LAUNCHER_PREFILL__);
    }
    if (rows().length !== 5) errs.push('typing changed the tray rows, found ' + rows().length);
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


RECBAR_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  // A small quiet headless frame stops producing rendering frames once
  // nothing keeps invalidating it, and a ResizeObserver's notifications ride
  // that same rendering step -- dirtying an unused custom property keeps
  // frames coming without touching any real geometry (same trick the main
  // window's own probe wrapper uses for its startup rAF).
  const heartbeat = setInterval(() => {
    document.documentElement.style.setProperty('--smoke-tick', String(Date.now()));
    document.body.getBoundingClientRect();
  }, 40);
  try {
    await sleep(300);
    const timeEl = document.getElementById('time');
    if (timeEl.textContent !== '01:05') errs.push('time did not render from recording_status: ' + timeEl.textContent);
    const sizeEl = document.getElementById('size');
    if (!/MB/.test(sizeEl.textContent)) errs.push('size did not render: ' + sizeEl.textContent);
    if (sizeEl.classList.contains('rec-size-warn')) errs.push('size was highlighted well below the ceiling');
    const until = async (fn, tries) => {
      for (let i = 0; i < (tries || 40) && !fn(); i++) await sleep(50);
      return fn();
    };
    // 505: recbar.css now sizes the bar to its own content instead of a fixed
    // 320x56 window, so stop-btn/cancel-btn must never fall outside the width
    // fit_rec_bar was actually sent -- checked once both audio icons (always
    // shown by applyAudio) and the hotkey warning (shown here since the
    // fixture leaves stop_hotkey_ok/pause_hotkey_ok undefined) have settled,
    // the widest this harness can reach (the disk warning needs the i18n
    // module, which never finishes loading here, same limitation the eta
    // tooltip check above already lives with).
    const fitCalls = () => (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'fit_rec_bar');
    const hotkeyWarnEl = document.getElementById('hotkey-warn');
    if (!await until(() => fitCalls().length > 0)) errs.push('fit_rec_bar was never called');
    else if (!await until(() => !hotkeyWarnEl.classList.contains('hidden'))) errs.push('hotkey warning never showed for the undefined stop/pause hotkey state');
    else {
      await sleep(100);
      const lastFit = fitCalls()[fitCalls().length - 1][1];
      const barLeft = bar.getBoundingClientRect().left;
      const stopRight = document.getElementById('stop-btn').getBoundingClientRect().right - barLeft;
      const cancelRight = document.getElementById('cancel-btn').getBoundingClientRect().right - barLeft;
      if (stopRight > lastFit.width) errs.push('stop-btn falls outside the fitted width: ' + stopRight + ' > ' + lastFit.width);
      if (cancelRight > lastFit.width) errs.push('cancel-btn falls outside the fitted width: ' + cancelRight + ' > ' + lastFit.width);
    }
    // 940-dg: Discard sits apart from the other actions, which stay close together,
    // and the split button is a scissors icon.
    {
      const rc = (id) => document.getElementById(id).getBoundingClientRect();
      const gapShown = !document.getElementById('continue-gap').classList.contains('hidden');
      const actionsRight = gapShown ? rc('continue-gap').right : rc('continue-btn').right;
      const apart = rc('cancel-btn').left - actionsRight;
      if (apart < 30) errs.push('940-dg: Discard is only ' + apart.toFixed(1) + 'px from the other actions');
      const tight = rc('stop-btn').left - rc('pause-btn').right;
      if (tight > 12) errs.push('940-dg: Pause and Stop drifted apart (' + tight.toFixed(1) + 'px)');
      if (document.querySelectorAll('#bar .rec-sep').length !== 1) errs.push('940-dg: the separator before Discard is missing');
      if (document.querySelectorAll('#continue-btn svg circle').length !== 2) errs.push('940-dg: the split button has no scissors icon');
    }
    // 941-dg: the hotkey triangle carries a visible word and the bar still fits it;
    // the hidden disk warning leaves no box behind.
    {
      const hotText = document.querySelector('#hotkey-warn .rec-warn-text');
      const diskText = document.querySelector('#disk-warn .rec-warn-text');
      const lastFit = fitCalls()[fitCalls().length - 1];
      if (!hotText) errs.push('941-dg: the hotkey warning has no text');
      else {
        const hr = hotText.getBoundingClientRect();
        if (hr.width <= 20) errs.push('941-dg: the hotkey warning text is only ' + hr.width.toFixed(1) + 'px wide');
        if (lastFit && hr.right - bar.getBoundingClientRect().left > lastFit[1].width) errs.push('941-dg: the hotkey warning text falls outside the fitted width');
      }
      if (!diskText) errs.push('941-dg: the disk warning has no text');
      else if (diskText.getClientRects().length !== 0) errs.push('941-dg: the hidden disk warning text still has a box');
    }
    // Under the show threshold: the readout should go blank on the next poll.
    window.__REC_STATUS_OVERRIDE__ = { bytes: 50 * 1024 * 1024, show_size: false, size_eta_secs: null };
    if (!await until(() => sizeEl.textContent === '')) errs.push('size stayed visible under the show threshold: ' + sizeEl.textContent);
    // Past three quarters of the configured ceiling: highlighted. The eta
    // tooltip text itself needs the i18n module, which this stripped-down
    // harness never finishes loading (no real navigation, so the dynamic
    // import never settles) -- rec_bar_size_state's own ETA math has its own
    // Rust unit tests, so this only checks what this harness can see.
    window.__REC_STATUS_OVERRIDE__ = { bytes: 7500 * 1024 * 1024, show_size: true, size_eta_secs: 45 };
    if (!await until(() => /MB/.test(sizeEl.textContent) && sizeEl.classList.contains('rec-size-warn'))) errs.push('size did not render/highlight near the ceiling: ' + sizeEl.textContent);
    window.__REC_STATUS_OVERRIDE__ = null;
    await sleep(600);
    // Target-switch button: disabled by the default fixture (not a window
    // recording on a placement canvas), enabled once the status says so,
    // and a pick routed back through recorder-target-picked with forSwitch
    // reaches switch_recording_target rather than start_recording.
    const switchBtn = document.getElementById('switch-btn');
    if (!switchBtn.disabled) errs.push('switch button was not disabled by default');
    window.__REC_STATUS_OVERRIDE__ = { target_switch_enabled: true };
    if (!await until(() => !switchBtn.disabled)) errs.push('switch button never enabled once the status allowed it');
    switchBtn.click();
    await sleep(30);
    if (!window.__SMOKE_CALLS__.includes('open_recorder_switch')) errs.push('switch button did not open the picker');
    window.__FIRE__('recorder-target-picked', { kind: 'window', id: 4242, forSwitch: true });
    await sleep(30);
    const switchCall = (window.__SMOKE_ARGS__ || []).find((c) => c[0] === 'switch_recording_target');
    if (!switchCall) errs.push('a forSwitch pick did not call switch_recording_target');
    else if (switchCall[1].targetId !== 4242) errs.push('switch_recording_target got the wrong target id: ' + JSON.stringify(switchCall[1]));
    window.__REC_STATUS_OVERRIDE__ = null;
    await sleep(600);
    // "Finish and continue": disabled inside the first five seconds
    // (elapsed_ms mirrors the backend's own started_at_ms guard), enabled
    // past it, and a click reaches the backend command and shows its gap.
    const continueBtn = document.getElementById('continue-btn');
    const continueGapEl = document.getElementById('continue-gap');
    window.__REC_STATUS_OVERRIDE__ = { elapsed_ms: 2000 };
    if (!await until(() => continueBtn.disabled)) errs.push('continue button was not disabled in the first five seconds');
    window.__REC_STATUS_OVERRIDE__ = { elapsed_ms: 65000 };
    if (!await until(() => !continueBtn.disabled)) errs.push('continue button never enabled past the five-second lockout');
    continueBtn.click();
    await sleep(30);
    if (!window.__REC_CONTINUED__) errs.push('continue button did not call finish_and_continue_recording');
    if (continueGapEl.classList.contains('hidden')) errs.push('continue gap badge did not show after a successful continue');
    window.__REC_STATUS_OVERRIDE__ = null;
    await sleep(600);
    document.getElementById('pause-btn').click();
    await sleep(30);
    if (!window.__REC_PAUSED__) errs.push('pause button did not call pause_recording');
    document.getElementById('pause-btn').click();
    await sleep(30);
    if (window.__REC_PAUSED__ !== false) errs.push('pause button did not call resume_recording on the second click');
    document.getElementById('stop-btn').click();
    await sleep(30);
    if (!window.__REC_STOPPED__) errs.push('stop button did not call stop_recording');
    if (!window.__REC_BAR_HIDDEN__) errs.push('stop did not hide the bar afterwards');
  } catch (e) { errs.push('probe: ' + e.message); }
  clearInterval(heartbeat);
  document.documentElement.style.removeProperty('--smoke-tick');
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# Stubs getUserMedia to reject deterministically -- this host's own headless
# Chrome actually resolves a stream (a virtual device the CI image ships),
# so the "no camera" acceptance check has to force the rejection itself
# rather than lean on real hardware absence, which a stub is the documented
# way to do. This inline script runs synchronously during parsing, BEFORE
# camera.js's own module script (deferred by the HTML spec like every
# type="module" tag), so the override is in place before camera.js ever
# calls getUserMedia.
CAMERA_STUB = """
<script>
if (!navigator.mediaDevices) { navigator.mediaDevices = {}; }
navigator.mediaDevices.getUserMedia = () => Promise.reject(new DOMException('no camera', 'NotFoundError'));
navigator.mediaDevices.enumerateDevices = () => Promise.resolve([]);
</script>
"""


# The window opening at all and the message replacing the preview instead of
# a crash is the whole test -- CAMERA_STUB above forces the one path a real
# device-less machine would also take.
CAMERA_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 60) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    const noDeviceEl = document.getElementById('cam-no-device');
    const startBtn = document.getElementById('cam-start-btn');
    // The page opens the camera only once the backend says the permission is given.
    await until(() => startBtn.textContent.trim());
    window.__FIRE__('camera-permission-ready', null);
    const ok = await until(() => !noDeviceEl.classList.contains('hidden'));
    if (!ok) errs.push('no-camera message never appeared for a device-less getUserMedia rejection');
    else if (!noDeviceEl.textContent.trim()) errs.push('no-camera message element is empty');
    if (!startBtn.disabled) errs.push('start button stayed enabled with no camera available');
    // The window itself must still be a normal, usable window around the
    // message -- the titlebar, close button and the three control rows.
    if (!document.getElementById('cam-close')) errs.push('close button is missing');
    if (!document.getElementById('cam-audio-select')) errs.push('audio source control is missing');
    if (!document.getElementById('cam-res-select')) errs.push('resolution control is missing');
    if (!document.getElementById('cam-folder-path')) errs.push('target folder control is missing');
    // 508: cam-res-select carries real options regardless of device
    // availability; audit its themed look across every built-in theme.
    {
      const priorTheme = document.documentElement.getAttribute('data-theme');
      const BUILTIN_THEMES = ['light', 'dark', 'programmer', 'neon', 'cyberpunk', 'ocean'];
      const swatch = document.createElement('div');
      swatch.style.cssText = 'position:absolute;left:-9999px;top:-9999px;background-color:var(--surface)';
      document.body.appendChild(swatch);
      const resSel = document.getElementById('cam-res-select');
      for (const theme of BUILTIN_THEMES) {
        document.documentElement.setAttribute('data-theme', theme);
        if (resSel) {
          const cs = getComputedStyle(resSel);
          if (parseFloat(cs.borderTopWidth) <= 0) errs.push('508: cam-res-select has no border in theme ' + theme);
          if (cs.backgroundColor === 'rgba(0, 0, 0, 0)' || cs.backgroundColor === 'transparent') {
            errs.push('508: cam-res-select has a transparent background in theme ' + theme);
          }
          const surfaceRgb = getComputedStyle(swatch).backgroundColor;
          // A selected option shows the accent colour (option:checked); pick
          // an unselected one to check the plain surface colour instead.
          const unselected = [...resSel.options].find((o) => !o.selected);
          if (unselected) {
            const optBg = getComputedStyle(unselected).backgroundColor;
            if (optBg !== surfaceRgb) {
              errs.push('508: cam-res-select option background is ' + optBg + ', expected ' + surfaceRgb + ' in theme ' + theme);
            }
          }
        }
      }
      swatch.remove();
      if (priorTheme) document.documentElement.setAttribute('data-theme', priorTheme);
      else document.documentElement.removeAttribute('data-theme');
    }
    // 942-dg: the folder row shows the path (also as its title) and the folder
    // button is an icon with an accessible name.
    {
      const pathEl = document.getElementById('cam-folder-path');
      const folderBtn = document.getElementById('cam-folder-btn');
      await until(() => pathEl.textContent.trim() !== '' && folderBtn.getAttribute('aria-label'));
      if (pathEl.textContent !== 'C:/fake/Videos/Recordings') errs.push('942-dg: the folder path reads "' + pathEl.textContent + '"');
      if (pathEl.title !== pathEl.textContent) errs.push('942-dg: the folder path has no matching title');
      if (!(folderBtn.getAttribute('aria-label') || '').trim()) errs.push('942-dg: the folder button has no accessible name');
      if (!folderBtn.querySelector('svg')) errs.push('942-dg: the folder button has no icon');
    }
    errs.push(...(window.__SMOKE_ERRORS__ || []));
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# Second boot of the camera window: the default recording folder lookup fails
# (the stub reads window.__CAM_DIR_FAIL__) while a resolution is saved. The
# other saved settings must still load.
CAMERA_DIRFAIL_STUB = """
<script>
window.__CAM_DIR_FAIL__ = true;
const dirFailInvoke = window.__TAURI__.core.invoke;
window.__TAURI__.core.invoke = async (cmd, args) => {
  const r = await dirFailInvoke(cmd, args);
  if (cmd === "get_settings") r.ui_texts = Object.assign({}, r.ui_texts, { cameraResolution: "1080" });
  return r;
};
</script>
"""


CAMERA_DIRFAIL_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  try {
    const res = document.getElementById('cam-res-select');
    for (let i = 0; i < 60 && res.value !== '1080'; i++) await sleep(50);
    // 942-dg-b: a failing default folder lookup no longer drops the other settings.
    if (res.value !== '1080') errs.push('942-dg: the saved resolution was lost when the default folder lookup failed (' + res.value + ')');
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# The recording paths of the camera window: a resolving fake stream, a fake
# MediaRecorder and a scripted backend (every camera_* command is answered
# here and logged in __CAM.calls), so start failures, chunk-write failures and
# the device retry can be driven deterministically. Runs against its own copy
# of camera.html (camera_rec.html) so the plain no-camera run above stays
# untouched.
CAMERA_REC_STUB = """
<script type="module" src="i18n.js"></script>
<script type="module" src="i18n/en.js"></script>
<script type="module" src="floatShared.js"></script>
<script>
window.__CAM = { gum: [], calls: [], order: [], gumFail: null, ctorThrow: false, startThrow: false,
  n: 0, appendFail: function () { return false; }, elapsedDelay: 0, startDelay: 0, device: "dev-1" };
__CAM.gumFail = (c) => (c.video.deviceId ? new DOMException("gone", "NotFoundError") : null);
if (!navigator.mediaDevices) { navigator.mediaDevices = {}; }
navigator.mediaDevices.getUserMedia = (c) => {
  __CAM.gum.push(c);
  const err = __CAM.gumFail && __CAM.gumFail(c);
  if (err) return Promise.reject(err);
  return Promise.resolve(new MediaStream());
};
navigator.mediaDevices.enumerateDevices = () => Promise.resolve([]);
class FakeRec {
  static isTypeSupported() { return true; }
  constructor() {
    if (__CAM.ctorThrow) throw new Error("ctor");
    this.state = "inactive";
    window.__REC = this;
  }
  start() {
    if (__CAM.startThrow) throw new Error("start");
    this.state = "recording";
    setTimeout(() => { __CAM.startedAt = Date.now(); if (this.onstart) this.onstart(); }, __CAM.startDelay);
  }
  stop() { this.state = "inactive"; if (this.onstop) this.onstop(); }
  emit(text) { this.ondataavailable({ data: new Blob([text]) }); }
}
window.MediaRecorder = FakeRec;
const origInvoke = window.__TAURI__.core.invoke;
window.__TAURI__.core.invoke = async (cmd, args) => {
  __CAM.calls.push([cmd, args]);
  if (cmd === "camera_begin_recording") return { path: "C:/r/rec-cam-1.webm" };
  if (cmd === "camera_append_chunk") {
    const i = ++__CAM.n;
    await new Promise((r) => setTimeout(r, i * 20));
    __CAM.order.push("append-done-" + i);
    if (__CAM.appendFail(i)) throw new Error("disk full");
    return null;
  }
  if (cmd === "camera_screen_elapsed_ms") {
    await new Promise((r) => setTimeout(r, __CAM.elapsedDelay));
    return 4242;
  }
  if (cmd === "camera_finish_recording") { __CAM.order.push("finish"); return { path: args.path }; }
  if (cmd === "camera_mark_finishing" || cmd === "camera_discard_recording" || cmd === "camera_abandon_recording") return null;
  if (cmd === "open_video_editor") return null;
  if (cmd === "get_settings") {
    const s = await origInvoke(cmd, args);
    return Object.assign({}, s, { ui_texts: Object.assign({}, s.ui_texts || {}, { cameraDevice: __CAM.device }) });
  }
  return origInvoke(cmd, args);
};
</script>
"""


CAMERA_REC_PROBE = """
<script>
(async () => {
  const errs = [];
  let step = "boot";
  const finish = () => {
    if (document.getElementById("SMOKE")) return;
    const pre = document.createElement("pre");
    pre.id = "SMOKE";
    pre.textContent = JSON.stringify(errs);
    document.body.appendChild(pre);
  };
  setTimeout(() => { errs.push("probe stalled at step " + step); finish(); }, 25000);
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50);
    return fn();
  };
  const count = (cmd) => __CAM.calls.filter(c => c[0] === cmd).length;
  const startBtn = document.getElementById("cam-start-btn");
  const statusText = document.getElementById("cam-status-text");
  const recBadge = document.getElementById("cam-rec-badge");
  const pauseBtn = document.getElementById("cam-pause-btn");
  const stopBtn = document.getElementById("cam-stop-btn");
  const idleUi = (label) => {
    if (startBtn.disabled) errs.push(label + ": Start stayed disabled");
    if (startBtn.classList.contains("hidden")) errs.push(label + ": Start is hidden");
    if (!recBadge.classList.contains("hidden")) errs.push(label + ": rec badge still visible");
    if (!pauseBtn.classList.contains("hidden")) errs.push(label + ": pause still visible");
    if (!stopBtn.classList.contains("hidden")) errs.push(label + ": stop still visible");
  };
  try {
    step = "ready";
    // Settings (and with them the saved camera id) load before the first
    // preview request, the way the permission-ready event orders it for real.
    await until(() => startBtn.textContent.trim());
    window.__FIRE__("camera-permission-ready", null);
    if (!(await until(() => !startBtn.disabled))) errs.push("Start never became enabled with a resolving stream");
    // The saved camera is gone: exactly one retry with the default camera.
    step = "deviceRetry";
    if (__CAM.gum.length !== 2) errs.push("device retry: getUserMedia calls " + __CAM.gum.length);
    else {
      if (!__CAM.gum[0].video.deviceId) errs.push("device retry: first call had no exact device");
      if (__CAM.gum[1].video.deviceId) errs.push("device retry: second call still had a device");
    }
    // A permission denial is not a stale device: no retry, Start stays off.
    step = "denied";
    __CAM.gum.length = 0;
    __CAM.gumFail = () => new DOMException("no", "NotAllowedError");
    document.getElementById("cam-res-select").dispatchEvent(new Event("change"));
    await until(() => startBtn.disabled);
    await sleep(100);
    if (__CAM.gum.length !== 1) errs.push("denied: getUserMedia calls " + __CAM.gum.length);
    if (!startBtn.disabled) errs.push("denied: Start stayed enabled");
    __CAM.gumFail = (c) => (c.video.deviceId ? new DOMException("gone", "NotFoundError") : null);
    document.getElementById("cam-res-select").dispatchEvent(new Event("change"));
    if (!(await until(() => !startBtn.disabled))) errs.push("Start never came back after the denial cleared");
    // Constructor throws after the backend already reserved the take.
    for (const mode of ["ctorThrow", "startThrow"]) {
      step = mode;
      __CAM[mode] = true;
      __CAM.calls.length = 0;
      startBtn.click();
      await until(() => count("camera_discard_recording") > 0 && !startBtn.disabled);
      await sleep(100);
      if (count("camera_begin_recording") !== 1) errs.push(mode + ": begin count " + count("camera_begin_recording"));
      if (count("camera_discard_recording") !== 1) errs.push(mode + ": discard count " + count("camera_discard_recording"));
      const d = __CAM.calls.find(c => c[0] === "camera_discard_recording");
      if (d && d[1].path !== "C:/r/rec-cam-1.webm") errs.push(mode + ": discarded the wrong path");
      if (statusText.textContent.trim()) errs.push(mode + ": status text left over: " + statusText.textContent);
      idleUi(mode);
      __CAM[mode] = false;
    }
    // A rejected middle append: every append is still drained, the take is
    // not finished or opened, the session is released once and never deleted.
    step = "appendFail";
    __CAM.calls.length = 0;
    __CAM.order.length = 0;
    __CAM.n = 0;
    __CAM.appendFail = (i) => i === 2;
    startBtn.click();
    if (!(await until(() => window.__REC && window.__REC.state === "recording"))) errs.push("recorder never started");
    window.__REC.emit("a");
    window.__REC.emit("b");
    window.__REC.emit("c");
    window.__REC.stop();
    await until(() => statusText.textContent.trim() && !startBtn.disabled, 200);
    await sleep(100);
    if (statusText.textContent !== "Part of the recording could not be saved. The raw file was kept: C:/r/rec-cam-1.webm") errs.push("failure message missing, status: " + statusText.textContent);
    if (__CAM.order.filter(o => o.indexOf("append-done") === 0).length !== 3) errs.push("not every append was drained: " + __CAM.order.join());
    if (count("camera_finish_recording") !== 0) errs.push("a take with a failed chunk was finished");
    if (count("open_video_editor") !== 0) errs.push("the editor opened for a damaged take");
    if (count("camera_abandon_recording") !== 1) errs.push("abandon count " + count("camera_abandon_recording"));
    if (count("camera_discard_recording") !== 0) errs.push("the raw file was discarded");
    idleUi("appendFail");
    if (document.getElementById("cam-res-select").disabled) errs.push("appendFail: resolution select stayed disabled");
    // The failure lands while the take is still recording: the session is
    // released at once, so a later stop never marks it finishing (a window
    // closed in between could otherwise write a sidecar for a corrupt take).
    step = "failBeforeStop";
    __CAM.calls.length = 0;
    __CAM.order.length = 0;
    __CAM.n = 0;
    __CAM.appendFail = (i) => i === 1;
    startBtn.click();
    await until(() => window.__REC && window.__REC.state === "recording");
    window.__REC.emit("a");
    window.__REC.emit("b");
    await until(() => count("camera_abandon_recording") > 0);
    if (count("camera_abandon_recording") !== 1) errs.push("failBeforeStop: abandon not called when the append failed");
    if (count("camera_mark_finishing") !== 0) errs.push("failBeforeStop: marked finishing too early");
    await sleep(200);
    window.__REC.stop();
    await until(() => !startBtn.disabled, 200);
    await sleep(100);
    if (count("camera_mark_finishing") !== 0) errs.push("failBeforeStop: a released take was marked finishing");
    if (count("camera_abandon_recording") !== 1) errs.push("failBeforeStop: abandon count " + count("camera_abandon_recording"));
    if (count("camera_finish_recording") !== 0 || count("camera_discard_recording") !== 0) errs.push("failBeforeStop: finished or discarded");
    if (!statusText.textContent.includes("C:/r/rec-cam-1.webm")) errs.push("failBeforeStop: message lacks the file path");
    // A clean take waits for its last append before finishing, and the
    // camera start plus screen offset are stamped at the recorder start event.
    step = "success";
    __CAM.calls.length = 0;
    __CAM.order.length = 0;
    __CAM.n = 0;
    __CAM.appendFail = () => false;
    __CAM.startDelay = 300;
    __CAM.elapsedDelay = 100;
    const t0 = Date.now();
    startBtn.click();
    await until(() => window.__REC && window.__REC.state === "recording" && __CAM.startedAt > t0 - 1);
    await sleep(400);
    window.__REC.emit("a");
    window.__REC.emit("b");
    window.__REC.stop();
    // While the finish is in flight neither Start nor the selects may be live.
    if (!startBtn.disabled) errs.push("Start enabled while the finish is in flight");
    if (!document.getElementById("cam-res-select").disabled || !document.getElementById("cam-audio-select").disabled) errs.push("selects enabled while the finish is in flight");
    await until(() => count("open_video_editor") > 0, 200);
    await sleep(300);
    if (startBtn.disabled) errs.push("Start not re-enabled after the finish");
    if (document.getElementById("cam-res-select").disabled) errs.push("selects not re-enabled after the finish");
    {
      const marks = __CAM.calls.filter(c => c[0] === "camera_mark_finishing");
      if (!marks.length || marks[marks.length - 1][1].screenElapsedMs !== 4242) errs.push("mark finishing lacks the screen offset");
    }
    if (count("camera_finish_recording") !== 1) errs.push("finish count " + count("camera_finish_recording"));
    if (count("camera_abandon_recording") !== 0) errs.push("a good take was abandoned");
    if (__CAM.order.join() !== "append-done-1,append-done-2,finish") errs.push("finish did not wait for the last append: " + __CAM.order.join());
    if (count("open_video_editor") !== 1) errs.push("editor open count " + count("open_video_editor"));
    if (statusText.textContent.trim()) errs.push("status text after a good take: " + statusText.textContent);
    // The start stamp follows the recorder start event (300 ms after the
    // click here), and the screen offset comes from a read taken there.
    const fin = __CAM.calls.find(c => c[0] === "camera_finish_recording");
    if (fin) {
      if (fin[1].camStartMs < t0 + 250) errs.push("camera start stamped before the recorder started: " + (fin[1].camStartMs - t0));
      if (fin[1].screenElapsedMs !== 4242) errs.push("screen offset not carried: " + fin[1].screenElapsedMs);
    }
  } catch (e) {
    errs.push("probe crashed: " + e);
  }
  finish();
})();
</script>
"""


def run(browser, tmp):
    failed = 0
    # Each secondary window carries the only coverage its own UI can get: the
    # pill drag/merge, the colour picker and the whole folder window live here
    # and nowhere else.
    for name, size, probe, budget in (
            ("floating.html", (240, 60), FLOAT_PROBE, 15000),
            ("floating_vars.html", (240, 60), FLOAT_VARS_PROBE, 15000),
            ("floating_plain.html", (240, 60), FLOAT_VARS_PROBE, 15000),
            ("snip.html", (1280, 800), PICK_PROBE + REGION_PROBE, 28000),
            ("folder.html", (520, 420), FOLDER_PROBE, 15000),
            ("vars.html", (340, 250), VARS_PROBE, 15000),
            ("launcher.html", (560, 420), LAUNCHER_PROBE, 15000),
            ("launcher_prefill.html", (560, 420), LAUNCHER_PREFILL_PROBE, 15000),
            ("launcher_md.html", (560, 420), LAUNCHER_MD_STUB + LAUNCHER_MD_PROBE, 15000),
            ("tray.html", (240, 370), TRAY_PROBE, 15000),  # 271 px of content (the backend sizes the window the same way) plus the headless frame
            ("recbar.html", (320, 56), RECBAR_PROBE, 15000),
            ("camera.html", (420, 620), CAMERA_STUB + CAMERA_PROBE, 15000),
            ("camera_dirfail.html", (420, 620), CAMERA_STUB + CAMERA_DIRFAIL_STUB + CAMERA_DIRFAIL_PROBE, 15000),
            ("camera_rec.html", (420, 620), CAMERA_REC_STUB + CAMERA_REC_PROBE, 30000)):
        if name in ("camera_rec.html", "camera_dirfail.html"):
            shutil.copyfile(REPO / "ui" / "camera.html", tmp / "ui" / name)
        if name in ("floating_vars.html", "floating_plain.html"):
            pid = "p1" if name == "floating_vars.html" else "p0"
            (tmp / "ui" / name).write_text(
                (REPO / "ui" / "floating.html").read_text(encoding="utf-8")
                .replace("<script", '<script>window.__FLOAT_PROMPT_ID__ = "%s";</script>\n<script' % pid, 1),
                encoding="utf-8")
        if name in ("launcher_prefill.html", "launcher_md.html"):
            shutil.copyfile(REPO / "ui" / "launcher.html", tmp / "ui" / name)
        side = tmp / "ui" / name
        side.write_text(
            side.read_text(encoding="utf-8")
            .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", probe + "</body>"),
            encoding="utf-8")
        proc = run_chrome(browser, tmp / ('profile-%s' % name), f"{size[0]},{size[1]}", budget, side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- {name}")
        for e in errs:
            print(f"   FAIL JS error: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")
    return failed
