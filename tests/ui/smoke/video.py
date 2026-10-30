"""Video editor suite: the two-size boots, the restore-dialog pair, the scale presets, the
superseded restore and the seeded expert values."""
import json
import re

from .harness import REPO, SIDE_SCAN, unescape, run_chrome


# The same editor booted in German (the longest labels) at the two narrow sizes, for the
# layout probes that depend on label length.
VIDEO_DE_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 60) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    await until(() => {
      const opsHint = document.querySelector('#ve-ops-list p');
      return document.getElementById('ve-stage-msg').classList.contains('hidden') &&
        document.querySelectorAll('#ve-filmstrip .ve-thumb').length > 0 &&
        !!opsHint && opsHint.textContent && opsHint.textContent !== 'veOpsEmpty';
    }, 300);
    if (document.getElementById('ve-tab-edit').textContent.trim() === 'Edit') {
      errs.push('the German boot still shows English labels');
    }
    // The volume sliders' screen-reader value is set before the texts load; it must be
    // redone once they arrive instead of keeping the raw key.
    for (const id of ['ve-vol-sys', 've-vol-mic']) {
      const vt = document.getElementById(id).getAttribute('aria-valuetext') || '';
      if (!/Prozent/.test(vt)) errs.push(id + ' aria-valuetext is not translated: ' + vt);
    }
    // 934: the tool row groups its buttons so none wraps alone, Fit is an icon
    // and Export frame sits in the transport row beside the frame number.
    {
      const tools = document.querySelector('.ve-tools');
      const groups = [...tools.querySelectorAll('.ve-tool-group')];
      if (groups.length !== 3) errs.push('934: expected 3 tool groups, found ' + groups.length);
      // Side by side the tool row only has the width left of the side panel and may wrap
      // between its groups (checked below); in the stacked layout it has the full width.
      if (window.innerWidth >= 880 && window.innerWidth <= 900 && tools.getBoundingClientRect().height > 46) {
        errs.push('934: the tool row wraps onto a second line (' + tools.getBoundingClientRect().height.toFixed(0) + 'px)');
      }
      for (const g of groups) {
        const mids = [...g.children].map((c) => c.getBoundingClientRect()).filter((r) => r.height > 0).map((r) => r.top + r.height / 2);
        if (Math.max(...mids) - Math.min(...mids) > 2) errs.push('934: a tool group breaks across lines');
      }
      if (!document.querySelector('.ve-transport #ve-still')) errs.push('934: Export frame is not in the transport row');
      const fit = document.getElementById('ve-zoom-fit');
      if (!fit.classList.contains('icon-btn') || fit.textContent.trim() !== '') errs.push('934: Fit is not an icon-only button');
      document.getElementById('ve-zoom-in').click();
      await sleep(60);
      if (document.getElementById('ve-zoom-val').textContent === '100%') errs.push('934: zooming in did not change the zoom value');
      fit.click();
      await sleep(60);
      if (document.getElementById('ve-zoom-val').textContent !== '100%') {
        errs.push('934: Fit did not return the zoom to 100% (' + document.getElementById('ve-zoom-val').textContent + ')');
      }
    }
    // 936: the clip start and end fields are compact and sit on one line.
    {
      const a = document.getElementById('ve-trim-start-val').getBoundingClientRect();
      const b = document.getElementById('ve-trim-end-val').getBoundingClientRect();
      if (a.width > 160) errs.push('936: the clip start field is ' + a.width.toFixed(0) + 'px wide');
      if (Math.abs(a.top - b.top) > 2) errs.push('936: the clip start and end fields are not on one line');
    }
    // 994-de: no page or main-area scroll at the side-by-side minimum in German either.
    if (window.innerWidth > 900) {
      const doc = document.documentElement;
      const main = document.querySelector('.ve-main');
      if (doc.scrollHeight !== window.innerHeight) errs.push('994: the German page scrolls (' + doc.scrollHeight + ' vs ' + window.innerHeight + ')');
      if (main.scrollHeight !== main.clientHeight) errs.push('994: the German main area scrolls (' + main.scrollHeight + ' vs ' + main.clientHeight + ')');
    }
    if (window.innerWidth > 920) errs.push('the German boot did not get its exact window size: ' + window.innerWidth);
    // 940: once the theme colour transitions are settled, the editor shows its
    // real theme at the narrow size (no faded surface): the body takes the theme
    // background and an inactive tab keeps a readable text contrast. A headless
    // boot freezes those transitions part way, so they are switched off first.
    {
      const noFade = document.createElement('style');
      noFade.textContent = '*, *::before, *::after { transition: none !important; animation: none !important; }';
      document.head.appendChild(noFade);
      await sleep(50);
      const nums = (c) => c.match(/[0-9.]+/g).map(Number);
      const lum = (rgb) => { const v = rgb.map((x) => { x /= 255; return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4); }); return 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]; };
      const over = (fg, bg) => { const a = fg.length > 3 ? fg[3] : 1; return [0, 1, 2].map((i) => fg[i] * a + bg[i] * (1 - a)); };
      const probeEl = document.createElement('div');
      probeEl.style.background = 'var(--bg)';
      document.body.appendChild(probeEl);
      const token = getComputedStyle(probeEl).backgroundColor;
      probeEl.remove();
      const bodyBg = getComputedStyle(document.body).backgroundColor;
      if (bodyBg !== token) errs.push('940: the body background ' + bodyBg + ' is not the theme background ' + token);
      const tab = document.getElementById('ve-tab-overlays');
      const tcs = getComputedStyle(tab);
      const bg = over(nums(tcs.backgroundColor), nums(bodyBg));
      const text = over(nums(tcs.color), bg);
      const hi = Math.max(lum(bg), lum(text)), lo = Math.min(lum(bg), lum(text));
      const ratio = (hi + 0.05) / (lo + 0.05);
      if (ratio < 4.5) errs.push('940: the inactive Overlays tab text has a contrast of only ' + ratio.toFixed(2));
    }
    // DE-PROBES
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# The video editor: the recording is PULLED at boot (video_editor_source), not
# pushed, so the probe only has to wait -- tauri_stub.js's own fixture is a
# 10-minute 1080p30 clip with both a system and a mic track, real enough that a
# populated filmstrip and both waveforms are what "boot finished" looks like.
VIDEO_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));""" + SIDE_SCAN + """
  const rawKeyCheck = (where) => {
    for (const el of document.querySelectorAll('body *')) {
      if (el.children.length) continue;
      const txt = (el.textContent || '').trim();
      if (/^ve[A-Z][A-Za-z]+$/.test(txt)) errs.push(where + ': a raw i18n key rendered: ' + txt);
    }
  };
  const hitTestable = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width < 4 || r.height < 4) return false;
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return !!hit && hit.closest('button') === el;
  };
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 60) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    // The recording itself resolves through invoke calls (fast, deterministic),
    // but the localized text comes from a lazily imported module -- a real
    // async fetch that a flat sleep can race ahead of under the virtual clock
    // (same reasoning FLOAT_PROBE calls out for the pill window) -- and the
    // waveform draw sits behind its own scheduleWaves() timeout on top of that.
    // One combined poll for everything "boot finished" means, rather than a
    // fixed delay that only happens to be long enough some of the time.
    await until(() => {
      const opsHint = document.querySelector('#ve-ops-list p');
      return document.getElementById('ve-stage-msg').classList.contains('hidden') &&
        document.querySelectorAll('#ve-filmstrip .ve-thumb').length > 0 &&
        document.getElementById('ve-wave-sys').width > 10 &&
        document.getElementById('ve-wave-mic').width > 10 &&
        !!opsHint && opsHint.textContent && opsHint.textContent !== 'veOpsEmpty';
    }, 300);
    // ---- the pulled recording loaded into a populated editor ----
    if (!document.getElementById('ve-stage-msg').classList.contains('hidden')) {
      errs.push('the stage never left its loading state');
    }
    if (document.getElementById('ve-duration').textContent !== '10:00.000') {
      errs.push('duration did not render from probe_recording: ' + document.getElementById('ve-duration').textContent);
    }
    if (!document.querySelectorAll('#ve-filmstrip .ve-thumb').length) {
      errs.push('no filmstrip thumbnails rendered from generate_filmstrip');
    }
    // Both canvases carry a hardcoded placeholder width of 10 in the markup, so
    // "still 10" is what an un-rendered waveform looks like, not zero.
    if (document.getElementById('ve-wave-sys').width <= 10) {
      errs.push('the system waveform canvas was never redrawn from generate_waveform_peaks');
    }
    if (document.getElementById('ve-wave-mic').classList.contains('hidden')) {
      errs.push('the mic waveform track stayed hidden although the fixture reports a mic track');
    } else if (document.getElementById('ve-wave-mic').width <= 10) {
      errs.push('the mic waveform canvas was never redrawn from generate_waveform_peaks');
    }
    if (!document.getElementById('ve-ffmpeg-warn').classList.contains('hidden')) {
      errs.push('the ffmpeg-missing banner showed although probe_recording succeeded');
    }
    // ---- 337: the mic track starts muted, the system track does not ----
    if (!document.getElementById('ve-mute-mic').classList.contains('ve-muted-btn')) {
      errs.push('the mic track must start muted in the video editor');
    }
    if (document.getElementById('ve-mute-sys').classList.contains('ve-muted-btn')) {
      errs.push('the system track must not start muted in the video editor');
    }
    // 939: each volume slider carries a short label, and a muted track shows
    // a struck-through icon (so muted and missing no longer look alike).
    {
      const { tx } = await (await import('./i18n.js')).fullTexts('en');
      const labels = [...document.querySelectorAll('.ve-audio-label')].map((l) => l.textContent);
      if (labels.length !== 2 || labels[0] !== tx('veAudioSys') || labels[1] !== tx('veAudioMic')) {
        errs.push('939: the volume labels read ' + JSON.stringify(labels) + ' instead of the system and mic names');
      }
      const shown = (id) => { const e = document.querySelector('#' + id + ' .ve-mute-slash'); return !!e && getComputedStyle(e).display !== 'none'; };
      if (!shown('ve-mute-mic')) errs.push('939: the muted mic shows no strike-through');
      if (shown('ve-mute-sys')) errs.push('939: the live system track shows a strike-through');
      document.getElementById('ve-mute-sys').click();
      await sleep(30);
      if (!shown('ve-mute-sys')) errs.push('939: muting the system track shows no strike-through');
      document.getElementById('ve-mute-sys').click();
      await sleep(30);
      if (shown('ve-mute-sys')) errs.push('939: unmuting the system track left the strike-through');
    }
    document.getElementById('ve-mute-mic').click();
    await sleep(30);
    if (document.getElementById('ve-mute-mic').classList.contains('ve-muted-btn')) {
      errs.push('clicking the mic mute button must mix the mic track back in');
    }
    {
      const shortcutsBtn = document.getElementById('ve-shortcuts');
      const shortcutsModal = document.getElementById('shortcuts-modal');
      if (!shortcutsBtn || !hitTestable(shortcutsBtn)) {
        errs.push('video editor: shortcuts overview button is missing, hidden, or not clickable');
      } else {
        shortcutsBtn.click();
        await sleep(50);
        if (shortcutsModal.classList.contains('hidden')) {
          errs.push('video editor: shortcuts overview did not open');
        } else {
          const keysTab = document.querySelector('#ve-help-nav .ve-help-tab[data-help="keys"]');
          if (keysTab) keysTab.click();
          await sleep(50);
          const rows = shortcutsModal.querySelectorAll('.shortcuts-row');
          if (rows.length === 0) errs.push('video editor: shortcuts overview opened with no rows');
          document.getElementById('shortcuts-modal-close').click();
          await sleep(50);
          if (!shortcutsModal.classList.contains('hidden')) {
            errs.push('video editor: shortcuts overview did not close');
          }
        }
      }
    }
    rawKeyCheck('edit tab');
    sideScan(errs, 'edit tab');

    // ---- numberField's stepper aria-labels used to be set once at
    // construction and never revisited, so a live language switch left them
    // stuck in whatever language was current at boot ----
    // (see numberField's decreaseAriaKey/increaseAriaKey/presetsAriaKey in
    // ui/floatShared.js, wired up here and in ui/main.js so applyI18n()'s
    // existing generic [data-i18n-aria] refresh loop covers them too).
    const cropDecBtn = document.getElementById('ve-crop-x').parentElement.querySelector('.numfield-dec');
    if (cropDecBtn.getAttribute('aria-label') !== 'Decrease value') {
      errs.push('the crop-X stepper did not start with the English aria-label: ' + cropDecBtn.getAttribute('aria-label'));
    }
    if (cropDecBtn.dataset.i18nAria !== 'numFieldDecrease') {
      errs.push('the crop-X stepper has no data-i18n-aria, so its aria-label would go stale on a language switch: ' + cropDecBtn.dataset.i18nAria);
    }
    const speedListBtn = document.getElementById('ve-speed-custom').parentElement.querySelector('.numfield-list');
    if (!speedListBtn || speedListBtn.dataset.i18nAria !== 'numFieldPresets') {
      errs.push('the speed presets stepper has no data-i18n-aria: ' + (speedListBtn && speedListBtn.dataset.i18nAria));
    }

    // ---- the timeline never clips vertically (horizontal scroll is expected) ----
    const timeline = document.getElementById('ve-timeline');
    const track = document.getElementById('ve-track');
    if (track.getBoundingClientRect().height > timeline.getBoundingClientRect().height + 1) {
      errs.push('the timeline track is taller than its own scroller and gets clipped');
    }

    // ---- every tab and toolbar control stays reachable ----
    const controls = [...document.querySelectorAll(
      '.ve-transport button, .ve-tools button, .ve-tabs button, .ve-titlebar button, ' +
      '.ve-audio-group button, .ve-ops-head button')];
    // In the narrow layout the controls scroll under the sticky preview by
    // design, so each one is scrolled into view before it is hit-tested.
    const untestable = [];
    for (const b of controls) {
      b.scrollIntoView({ block: 'nearest' });
      await sleep(30);
      if (!hitTestable(b)) untestable.push(b);
    }
    if (untestable.length) {
      errs.push(untestable.length + ' toolbar control(s) cannot be hit-tested: ' +
        untestable.map((b) => b.id || b.className).join(', '));
    }
    document.querySelector('.ve-main').scrollTop = 0;

    // 933: the narrow layout keeps the preview at 40 % of the height, sticky
    // on top while the controls scroll under it, and the side panel (with the
    // undo/redo row) fits the room below the preview.
    {
      const stage = document.getElementById('ve-stage');
      const stageH = stage.getBoundingClientRect().height;
      if (stageH < 0.4 * window.innerHeight - 1) {
        errs.push('933: the preview is only ' + stageH.toFixed(0) + 'px of a ' + window.innerHeight + 'px window');
      }
      if (window.innerWidth <= 900) {
        const main = document.querySelector('.ve-main');
        main.scrollTop = 1e6;
        await sleep(60);
        const dTop = Math.abs(stage.getBoundingClientRect().top - main.getBoundingClientRect().top);
        if (dTop > 1) errs.push('933: the preview did not stay on top while the controls scrolled (off by ' + dTop.toFixed(1) + 'px)');
        const undo = document.getElementById('ve-undo');
        if (!hitTestable(undo)) errs.push('933: undo is not reachable when the narrow layout is scrolled to the end');
        main.scrollTop = 0;
        await sleep(30);
      }
    }

    // 994: at the side-by-side minimum (920x600) neither the page nor the main area scrolls.
    if (window.innerWidth > 900 && window.innerWidth <= 940) {
      const doc = document.documentElement;
      if (doc.scrollHeight !== window.innerHeight) {
        errs.push('994: the page scrolls at ' + window.innerWidth + 'x' + window.innerHeight + ' (' + doc.scrollHeight + 'px content)');
      }
      const main = document.querySelector('.ve-main');
      if (main.scrollHeight !== main.clientHeight) {
        errs.push('994: the main area scrolls at ' + window.innerWidth + 'x' + window.innerHeight + ' (' + main.scrollHeight + ' vs ' + main.clientHeight + ')');
      }
    }

    // 937: the timeline shows the whole 10-minute recording when it opens
    // (the track no longer gets a floor that makes it wider than the view),
    // and the ruler labels stay apart.
    {
      const tl = document.getElementById('ve-timeline');
      const tr = document.getElementById('ve-track');
      if (tr.getBoundingClientRect().width > tl.clientWidth + 1) {
        errs.push('937: the track is ' + tr.getBoundingClientRect().width.toFixed(0) + 'px in a ' + tl.clientWidth + 'px timeline');
      }
      if (document.getElementById('ve-zoom-val').textContent !== '100%') {
        errs.push('937: the zoom label does not read 100% on open: ' + document.getElementById('ve-zoom-val').textContent);
      }
      const ticks = [...document.querySelectorAll('#ve-ruler .ve-tick')];
      if (ticks.length >= 2) {
        const gap = ticks[1].getBoundingClientRect().left - ticks[0].getBoundingClientRect().left;
        if (gap < 60) errs.push('937: neighbouring ruler labels are only ' + gap.toFixed(0) + 'px apart');
      } else {
        errs.push('937: the ruler shows fewer than two labels');
      }
    }

    // 938: the frame counter reads the word and a 1-based number, compared
    // against the translated string so the probe holds in any boot language.
    {
      const { tx } = await (await import('./i18n.js')).fullTexts('en');
      const frameText = () => document.getElementById('ve-frame').textContent;
      const want = (n) => tx('veFrameNo').replace('{n}', String(n));
      if (frameText() !== want(1)) errs.push('938: the frame counter at the start reads "' + frameText() + '" instead of "' + want(1) + '"');
      document.getElementById('ve-next-frame').click();
      await sleep(60);
      if (frameText() !== want(2)) errs.push('938: one frame on, the counter reads "' + frameText() + '" instead of "' + want(2) + '"');
      document.getElementById('ve-prev-frame').click();
      await sleep(60);
    }

    // 936-wide: the clip start and end fields share a line in every layout,
    // also in the 320 px side panel of the wide layout.
    {
      const a = document.getElementById('ve-trim-start-val').getBoundingClientRect();
      const b = document.getElementById('ve-trim-end-val').getBoundingClientRect();
      if (Math.abs(a.top - b.top) > 2) errs.push('936-wide: the clip start and end fields are on two lines at ' + window.innerWidth + 'px');
    }

    // ---- tabs actually switch panels ----
    document.getElementById('ve-tab-overlays').click();
    await sleep(80);
    if (document.getElementById('ve-panel-overlays').classList.contains('hidden')) {
      errs.push('the Overlays tab did not reveal its panel');
    }
    if (!document.getElementById('ve-panel-edit').classList.contains('hidden')) {
      errs.push('the Edit panel stayed visible under the Overlays tab');
    }

    // ---- 334: one of each overlay kind actually renders in the live-preview
    // layer, not just the edit-handle rectangle ----
    const liveLayer = document.getElementById('ve-overlay-live');
    if (liveLayer.children.length) {
      errs.push('the live overlay preview shows something before any overlay was added');
    }
    const overlayKinds = [
      ['ve-ov-add-image', 've-live-image'],
      ['ve-ov-add-frame', 've-live-frame'],
      ['ve-ov-add-text', 've-live-text'],
      ['ve-ov-add-pixelate', 've-live-pixelate'],
      ['ve-ov-add-spotlight', 've-live-spotlight'],
      ['ve-ov-add-keys', 've-live-keys'],
    ];
    for (const [btnId, liveClass] of overlayKinds) {
      document.getElementById(btnId).click();
      await sleep(30);
      if (!liveLayer.querySelector('.' + liveClass)) {
        errs.push(btnId + ' did not produce a .' + liveClass + ' element in the live overlay preview');
      }
    }
    // 526: an inserted "video" overlay (a picture-in-picture clip, added
    // through "Add a video" -> "As an overlay" rather than one of the plain
    // ADD AN OVERLAY buttons above) used to have no live-preview branch at
    // all in buildLiveOverlayEl -- it rendered fine in export but never
    // showed anything in the editor while working on it.
    document.getElementById('ve-add-video').click();
    await sleep(30);
    document.getElementById('ve-add-video-as-overlay').click();
    await sleep(30);
    if (!liveLayer.querySelector('.ve-live-video')) {
      errs.push('a "video" overlay did not produce a .ve-live-video element in the live overlay preview');
    }
    // Every overlay just defaulted to the clip's [0, ve-duration] range --
    // seeking to the exact start must still keep them all visible (inclusive
    // boundary), proving the gate really reads the range rather than always
    // showing everything regardless of the playhead.
    document.getElementById('ve-tab-edit').click();
    await sleep(30);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home' }));
    await sleep(30);
    if (liveLayer.children.length !== overlayKinds.length + 1) {
      errs.push('live overlay preview lost overlays at the exact start of their range (playhead 0)');
    }
    document.getElementById('ve-tab-overlays').click();
    await sleep(30);

    // ---- 524: the spotlight dim must be real painted geometry, not a huge
    // box-shadow spread some GPUs drop on real hardware. Select the spotlight
    // row so its edit outline (sized to op.rect, same transform the live
    // preview uses) gives a ground-truth clear rect. The live layer has
    // pointer-events: none (it is never interactive), so elementFromPoint
    // cannot see it -- instead read the actual painted shape back from the
    // computed clip-path: the dim element must cover far more than the clear
    // rect, and the punched-out hole in its clip-path must line up with that
    // same rect. A plain box-shadow spread sets no clip-path and keeps the
    // element itself sized to op.rect, so this fails on the old code.
    {
      const spotlightRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-spotlight');
      const rowsNow = [...document.querySelectorAll('#ve-ov-list .ve-ov-row')];
      const spotRow = rowsNow[spotlightRowIdx];
      if (!spotRow) {
        errs.push('524: could not find the spotlight overlay row to select it');
      } else {
        spotRow.click();
        await sleep(30);
        const clearEl = document.querySelector('.ve-rect-spotlight');
        const liveEl = document.querySelector('.ve-live-spotlight');
        if (!clearEl || !liveEl) {
          errs.push('524: spotlight edit outline or live preview element is missing');
        } else {
          const clear = clearEl.getBoundingClientRect();
          const live = liveEl.getBoundingClientRect();
          const bg = getComputedStyle(liveEl).backgroundColor;
          const parts = bg.match(/rgba?\\(([^)]+)\\)/);
          const nums = parts ? parts[1].split(',').map((s) => parseFloat(s.trim())) : [];
          const alpha = nums.length > 3 ? nums[3] : (bg === 'transparent' ? 0 : 1);
          if (Math.abs(alpha - 0.6) > 0.05) {
            errs.push('524: spotlight live dim background alpha is ' + alpha + ', expected about 0.6 (op.dim)');
          }
          if (live.width < clear.width + 20 || live.height < clear.height + 20) {
            errs.push('524: the spotlight dim element is not larger than its own clear rect (no dim area outside it)');
          }
          const clipPath = getComputedStyle(liveEl).clipPath;
          if (!clipPath || clipPath === 'none') {
            errs.push('524: the spotlight dim has no clip-path, so it cannot punch a clear hole in real geometry');
          } else {
            const pts = [...clipPath.matchAll(/([\\d.]+)px\\s+([\\d.]+)px/g)].map((m) => [parseFloat(m[1]), parseFloat(m[2])]);
            const hole = pts.slice(5, 9);
            if (hole.length < 4) {
              errs.push('524: the spotlight clip-path does not carry a separate hole shape: ' + clipPath);
            } else {
              const xs = hole.map((p) => p[0]), ys = hole.map((p) => p[1]);
              const holeLeft = live.left + Math.min(...xs), holeTop = live.top + Math.min(...ys);
              const holeW = Math.max(...xs) - Math.min(...xs), holeH = Math.max(...ys) - Math.min(...ys);
              const near = (a, b) => Math.abs(a - b) <= 3;
              if (!near(holeLeft, clear.left) || !near(holeTop, clear.top) || !near(holeW, clear.width) || !near(holeH, clear.height)) {
                errs.push('524: the clip-path hole (' + [holeLeft, holeTop, holeW, holeH].map((n) => n.toFixed(1)) +
                  ') does not line up with the clear rect (' + [clear.left, clear.top, clear.width, clear.height].map((n) => n.toFixed(1)) + ')');
              }
            }
          }
        }
      }
    }

    rawKeyCheck('overlays tab');
    sideScan(errs, 'overlays tab');

    // ---- 354: text can be selected in the side panels, not on the drag surfaces ----
    const selectableIds = ['ve-export-dest', 've-trim-start-val'];
    for (const id of selectableIds) {
      const style = getComputedStyle(document.getElementById(id));
      if (style.userSelect === 'none') {
        errs.push('#' + id + ' cannot be text-selected (user-select: none)');
      }
    }
    if (getComputedStyle(document.querySelector('.ve-titlebar')).userSelect !== 'none') {
      errs.push('the titlebar drag surface must stay non-selectable');
    }

    // ---- 355/356: select the text overlay, check it draws once and that a
    // value commit does not steal focus away from the field being edited ----
    const ovRows = [...document.querySelectorAll('#ve-ov-list .ve-ov-row')];
    const textRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-text');
    if (!ovRows[textRowIdx]) {
      errs.push('could not find the text overlay row in the overlay list');
    } else {
      ovRows[textRowIdx].click();
      await sleep(30);
      const textField = document.querySelector('#ve-ov-editor [data-ov-field="text"]');
      if (!textField) {
        errs.push('the text overlay editor has no tracked text-value field');
      } else {
        const before = textField.value;
        const withText = [...document.querySelectorAll('#ve-stage *')].filter((el) => el.textContent === before && el.children.length === 0);
        if (withText.length !== 1) {
          errs.push('expected exactly one element showing the overlay text in #ve-stage, found ' + withText.length);
        }
        textField.focus();
        textField.value = before + '!';
        // Set the caret AFTER the value assignment -- assigning .value itself
        // always snaps the caret to the end, so this reflects where a real
        // typist's cursor sits right before the commit this probe simulates.
        textField.setSelectionRange(2, 2);
        textField.dispatchEvent(new Event('input'));
        textField.dispatchEvent(new Event('change'));
        await sleep(30);
        if (document.activeElement !== textField) {
          errs.push('356: committing a text overlay value moved focus away from the field being edited');
        } else if (textField.selectionStart !== 2) {
          errs.push('356: committing a text overlay value moved the caret in the still-focused field');
        }
      }
      // the currently selected op is text, not spotlight/pixelate -- select the
      // spotlight row to check its rect-tip hint carries no duplicate tooltip
      const spotRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-spotlight');
      if (ovRows[spotRowIdx]) {
        ovRows[spotRowIdx].click();
        await sleep(30);
        const hint = document.querySelector('#ve-ov-editor .modal-hint');
        if (hint && hint.hasAttribute('title')) {
          errs.push('355: the pixelate/spotlight hint still repeats itself as a tooltip');
        }
      }
    }

    // ---- 463: the keys overlay offers all four presets plus a free
    // selection, and switching to the free selection reveals its picker ----
    {
      const keysRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-keys');
      if (!ovRows[keysRowIdx]) {
        errs.push('could not find the keys overlay row in the overlay list');
      } else {
        ovRows[keysRowIdx].click();
        await sleep(30);
        const presetSelect = document.querySelector('#ve-ov-editor select');
        const values = presetSelect ? [...presetSelect.options].map((o) => o.value) : [];
        const expected = ['pressed', 'half', 'full', 'game', 'custom'];
        if (!presetSelect || expected.some((v) => !values.includes(v))) {
          errs.push('463: the keys overlay preset dropdown is missing one of ' + expected.join(', ') + ', got ' + values.join(', '));
        } else {
          presetSelect.value = 'custom';
          presetSelect.dispatchEvent(new Event('change'));
          await sleep(30);
          const picker = document.querySelector('#ve-ov-editor .ve-keys-picker');
          if (!picker || !picker.querySelector('input[type="checkbox"]')) {
            errs.push('463: choosing the custom preset did not reveal the free-selection key picker');
          }
        }
      }
    }

    // ---- 356: dragging an overlay's rectangle updates its X/Y fields in
    // place while a different field in the same panel keeps focus ----
    {
      const imgRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-image');
      if (ovRows[imgRowIdx]) {
        ovRows[imgRowIdx].click();
        await sleep(30);
        const opacityField = document.querySelector('#ve-ov-editor [data-ov-field="opacity"]');
        const xField = document.querySelector('#ve-ov-editor [data-ov-field="rect.x"]');
        const rectEl = document.querySelector('#ve-rects .ve-rect-image');
        if (opacityField && xField && rectEl) {
          opacityField.focus();
          const beforeX = xField.value;
          const box = rectEl.getBoundingClientRect();
          const cx = box.left + box.width / 2, cy = box.top + box.height / 2;
          rectEl.dispatchEvent(new PointerEvent('pointerdown', { clientX: cx, clientY: cy, pointerId: 1, bubbles: true }));
          document.getElementById('ve-rects').dispatchEvent(new PointerEvent('pointermove', { clientX: cx + 20, clientY: cy + 20, pointerId: 1, bubbles: true }));
          document.getElementById('ve-rects').dispatchEvent(new PointerEvent('pointerup', { clientX: cx + 20, clientY: cy + 20, pointerId: 1, bubbles: true }));
          await sleep(30);
          if (document.activeElement !== opacityField) {
            errs.push('356: dragging an overlay rectangle moved focus away from an unrelated field in the same panel');
          }
          if (xField.value === beforeX) {
            errs.push('356: dragging an overlay rectangle did not refresh its own X field');
          }
        } else {
          errs.push('356: could not find the image overlay rect/fields to test the drag path');
        }
      }
    }

    // 370: the overlay colour row's custom swatch opens the same shared
    // colour popup the main window uses, and picking a hex both updates the
    // live preview and shows on the swatch itself.
    {
      ovRows[textRowIdx].click();
      await sleep(30);
      const customSwatch = document.querySelector('#ve-ov-editor .ve-swatches .swatch.custom');
      if (!customSwatch) {
        errs.push('370: no custom-colour swatch in the overlay colour row');
      } else {
        customSwatch.click();
        await sleep(30);
        const pop = document.querySelector('.color-pop');
        if (!pop || pop.classList.contains('hidden')) {
          errs.push('370: clicking the custom swatch did not open the shared colour popup');
        } else {
          const hexInput = pop.querySelector('.cp-hex');
          const liveText = document.querySelector('#ve-overlay-live .ve-live-text');
          hexInput.value = '#3fa796';
          hexInput.dispatchEvent(new Event('change', { bubbles: true }));
          await sleep(30);
          if (!liveText) {
            errs.push('370: no .ve-live-text element to check the colour change against');
          } else if (liveText.style.color !== 'rgb(63, 167, 150)') {
            errs.push('370: picking a custom colour did not update the live overlay preview, got ' + liveText.style.color);
          }
          const freshSwatch = document.querySelector('#ve-ov-editor .ve-swatches .swatch.custom');
          if (!freshSwatch || !freshSwatch.classList.contains('sel') || freshSwatch.style.background !== 'rgb(63, 167, 150)') {
            errs.push('370: the custom swatch does not show the picked colour after the row refreshes');
          }
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
          await sleep(30);
          if (!pop.classList.contains('hidden')) {
            errs.push('370: Escape did not close the shared colour popup');
          }
          if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
            errs.push('370: Escape while the colour popup was open also triggered the close-window confirm dialog');
          }
        }
      }
      // Restore the image overlay selection the later 364/365 checks below
      // depend on -- this block borrowed the text overlay for its own probe.
      const imgRowIdxAgain = overlayKinds.findIndex(([id]) => id === 've-ov-add-image');
      if (ovRows[imgRowIdxAgain]) {
        ovRows[imgRowIdxAgain].click();
        await sleep(30);
      }
    }

    // 364: the shared mouse-only number field, exercised on the still-selected
    // image overlay's opacity control (step 5, presets 25/50/75/100).
    {
      const opacityInput = document.querySelector('#ve-ov-editor [data-ov-field="opacity"]');
      const numfield = opacityInput && opacityInput.closest('.numfield');
      if (!numfield) {
        errs.push('364: could not find the opacity number field to test it');
      } else {
        const inc = numfield.querySelector('.numfield-inc');
        const dec = numfield.querySelector('.numfield-dec');
        const list = numfield.querySelector('.numfield-list');
        // Seed a mid-range value first -- the overlay's default opacity can
        // already sit at this field's max, which would leave + disabled and
        // make the click/hold steps below silent no-ops.
        opacityInput.value = '50';
        opacityInput.dispatchEvent(new Event('change', { bubbles: true }));
        await sleep(10);
        // Click: one pointerdown+up steps by exactly `step` (5).
        const beforeClick = Number(opacityInput.value);
        inc.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 21 }));
        await sleep(10);
        inc.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 21 }));
        await sleep(10);
        if (Number(opacityInput.value) !== beforeClick + 5) {
          errs.push('364: clicking + did not step the opacity field by exactly 5');
        }
        // Hold: still down past the 400ms/60ms repeat threshold must step more
        // than once (using - this time, so it also cannot get stuck at max=100).
        const beforeHold = Number(opacityInput.value);
        dec.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 22 }));
        await sleep(550);
        dec.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 22 }));
        await sleep(10);
        if (Number(opacityInput.value) > beforeHold - 10) {
          errs.push('364: holding - did not repeat-step the opacity field past a single step');
        }
        // Wheel: one wheel tick steps by exactly `step` (5), and must not
        // scroll the panel underneath it.
        const beforeWheel = Number(opacityInput.value);
        opacityInput.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, deltaY: -100 }));
        await sleep(30);
        if (Number(opacityInput.value) !== beforeWheel + 5) {
          errs.push('364: wheel over the opacity field did not step by exactly 5');
        }
        // Presets: opening the list and picking 100 sets the field to it and
        // closes the popover again.
        if (!list) {
          errs.push('364: the opacity field has no presets button');
        } else {
          list.click();
          await sleep(30);
          const pop = document.querySelector('.numfield-pop');
          const target = pop && [...pop.querySelectorAll('button')].find((b) => b.textContent === '100');
          if (!target) {
            errs.push('364: the opacity presets popover did not offer 100');
          } else {
            target.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 23 }));
            await sleep(30);
            if (Number(opacityInput.value) !== 100) {
              errs.push('364: picking the 100 preset did not set the opacity field to it');
            }
            if (document.querySelector('.numfield-pop')) {
              errs.push('364: picking a preset did not close the popover');
            }
          }
        }
        // At its max (just set to 100 above), + must now be visibly dead.
        if (!inc.disabled) {
          errs.push('364: the + button is not disabled once the opacity field is at its max');
        }
      }
      // The overlay editor column must not scroll sideways at this window's
      // minimum width just because a numfield now lives in it.
      const ovEditor = document.getElementById('ve-ov-editor');
      if (ovEditor && ovEditor.scrollWidth > ovEditor.clientWidth + 1) {
        errs.push('364: a number field overflows the overlay editor column at this window width');
      }
    }

    // 365: wheel-zooming the stage keeps the point under the cursor fixed
    // instead of always zooming to the stage centre. The image overlay's
    // rectangle from the 356 drag above sits off-centre by now -- its own
    // top-left corner is a known, trackable point to zoom onto.
    {
      const rectEl = document.querySelector('#ve-rects .ve-rect-image');
      if (!rectEl) {
        errs.push('365: could not find the image overlay rectangle to test the zoom anchor');
      } else {
        const boxBefore = rectEl.getBoundingClientRect();
        const cx = boxBefore.left, cy = boxBefore.top;
        document.getElementById('ve-stage').dispatchEvent(
          new WheelEvent('wheel', { bubbles: true, cancelable: true, clientX: cx, clientY: cy, deltaY: -100 }));
        await sleep(30);
        // renderRects() rebuilds the rectangle elements on every zoom/pan
        // change, so the original rectEl reference is now detached -- re-query.
        const rectElAfter = document.querySelector('#ve-rects .ve-rect-image');
        const boxAfter = rectElAfter ? rectElAfter.getBoundingClientRect() : { left: NaN, top: NaN };
        const moved = Math.hypot(boxAfter.left - cx, boxAfter.top - cy);
        if (!rectElAfter) {
          errs.push('365: the image overlay rectangle disappeared after zooming the stage');
        } else if (moved >= 2) {
          errs.push('365: zooming the video stage with the wheel did not keep the point under the cursor fixed (moved ' + moved.toFixed(1) + 'px)');
        }
        document.getElementById('ve-preview-zoom-reset')?.click();
        await sleep(30);
      }
    }

    // ---- 393: dragging an overlay near the frame centre snaps it exactly
    // onto that centre, and Alt held during the drag suppresses the snap.
    // Runs after 365 above (rather than right after 356, where it would
    // otherwise sit) because 365 depends on the image overlay still sitting
    // exactly where the 356 drag left it, off-centre, as its own known point
    // to zoom onto -- this block moves it around a great deal, so it runs
    // once that dependency is no longer needed. ----
    {
      const imgRowIdxSnap = overlayKinds.findIndex(([id]) => id === 've-ov-add-image');
      if (ovRows[imgRowIdxSnap]) {
        // The row is still the one selected from 356/364/365 above; a row
        // click toggles selection off, so only click if it is not open.
        const ovEditorBox = document.getElementById('ve-ov-editor');
        const imageAlreadyOpen = !ovEditorBox.classList.contains('hidden') && !!ovEditorBox.querySelector('.ve-path');
        if (!imageAlreadyOpen) {
          ovRows[imgRowIdxSnap].click();
          await sleep(30);
        }
        const xField = document.querySelector('#ve-ov-editor [data-ov-field="rect.x"]');
        const yField = document.querySelector('#ve-ov-editor [data-ov-field="rect.y"]');
        const wField = document.querySelector('#ve-ov-editor [data-ov-field="rect.w"]');
        const hField = document.querySelector('#ve-ov-editor [data-ov-field="rect.h"]');
        if (!xField || !yField || !wField || !hField || !document.querySelector('#ve-rects .ve-rect-image')) {
          errs.push('393: could not find the image overlay rect/fields to test snapping');
        } else {
          const expectX = 960 - Number(wField.value) / 2;
          const expectY = 540 - Number(hField.value) / 2;
          // Moves the overlay by a source-pixel delta, converting it to a
          // client-space drag distance from the rect element's OWN current
          // size against its known source width -- robust to however the
          // video is actually laid out inside the stage, rather than
          // recomputing that layout by hand. Reuses pointerId 1, same as the
          // 356 drag above: a fresh custom id is never recognised as an
          // active pointer by setPointerCapture, only the mouse's own is.
          // renderRects() also rebuilds '#ve-rects' from scratch on every
          // drag, so the rect element is re-queried fresh each call rather
          // than reused from an earlier, by-then-detached one.
          const dragBy = (dxSource, dySource, altKey) => {
            const el = document.querySelector('#ve-rects .ve-rect-image');
            const box = el.getBoundingClientRect();
            const scale = box.width / Number(wField.value);
            const from = { x: box.left + box.width / 2, y: box.top + box.height / 2 };
            const to = { x: from.x + dxSource * scale, y: from.y + dySource * scale };
            el.dispatchEvent(new PointerEvent('pointerdown', { clientX: from.x, clientY: from.y, pointerId: 1, bubbles: true }));
            document.getElementById('ve-rects').dispatchEvent(new PointerEvent('pointermove', { clientX: to.x, clientY: to.y, pointerId: 1, altKey, bubbles: true }));
            document.getElementById('ve-rects').dispatchEvent(new PointerEvent('pointerup', { clientX: to.x, clientY: to.y, pointerId: 1, altKey, bubbles: true }));
          };
          // Move it well away from the centre first, Alt held so this setup
          // move does not get pulled onto a snap line itself.
          dragBy(-300, 200, true);
          await sleep(30);
          if (Number(xField.value) === expectX && Number(yField.value) === expectY) {
            errs.push('393: the Alt-held setup drag left the overlay exactly on the frame centre, cannot prove the next drag snaps it there');
          }
          // Aim to within a source pixel or two of the exact centre -- not
          // exactly on it -- and expect the snap to land it exactly there.
          // The fixed 1920x1080 fixture puts the frame centre at source
          // (960, 540); the tolerance is always well above a couple of
          // source pixels here since the preview is always shown well below
          // its native size.
          let curX = Number(xField.value) + Number(wField.value) / 2;
          let curY = Number(yField.value) + Number(hField.value) / 2;
          dragBy(960 + 1 - curX, 540 - 1 - curY, false);
          await sleep(30);
          if (Number(xField.value) !== expectX || Number(yField.value) !== expectY) {
            errs.push('393: dragging near the frame centre did not snap onto it exactly, got ' +
              xField.value + ',' + yField.value + ' expected ' + expectX + ',' + expectY);
          }
          // The same near-centre approach with Alt held must not snap. The
          // overlay is already sitting exactly on the centre from the drag
          // above, so this nudges it 4 source pixels off -- an even number,
          // so the drag's own even-pixel rounding cannot coincidentally land
          // back on the same value the snap itself would have produced.
          curX = Number(xField.value) + Number(wField.value) / 2;
          curY = Number(yField.value) + Number(hField.value) / 2;
          dragBy(960 + 4 - curX, 540 - 4 - curY, true);
          await sleep(30);
          if (Number(xField.value) === expectX && Number(yField.value) === expectY) {
            errs.push('393: holding Alt during the drag did not suppress the centre snap');
          }
        }
      }
    }

    // ---- 383: the video editor's other half of the zoom-key and
    // click-to-select-handles parity check, screenshot editor side in
    // smoke_test.py's own '383:' block above ----
    {
      const zoomValVe = document.getElementById('ve-preview-zoom-val');
      if (!zoomValVe) {
        errs.push('383: video editor preview has no zoom readout to check');
      } else {
        const readVePct = () => Number(zoomValVe.textContent.replace('%', ''));
        const startPct = readVePct(); // reset to 100 by the 365 check just above
        document.dispatchEvent(new KeyboardEvent('keydown', { key: '+', ctrlKey: true, bubbles: true }));
        await sleep(30);
        const inPct = readVePct();
        if (inPct <= startPct) {
          errs.push('383: Ctrl+Plus did not zoom the video editor preview in (was ' + startPct + '%, now ' + inPct + '%)');
        }
        document.dispatchEvent(new KeyboardEvent('keydown', { key: '-', ctrlKey: true, bubbles: true }));
        await sleep(30);
        document.dispatchEvent(new KeyboardEvent('keydown', { key: '-', ctrlKey: true, bubbles: true }));
        await sleep(30);
        const outPct = readVePct();
        if (outPct >= inPct) {
          errs.push('383: Ctrl+Minus did not zoom the video editor preview out (was ' + inPct + '%, now ' + outPct + '%)');
        }
        document.dispatchEvent(new KeyboardEvent('keydown', { key: '0', ctrlKey: true, bubbles: true }));
        await sleep(30);
        const fitPctVe = readVePct();
        if (fitPctVe !== startPct) {
          errs.push('383: Ctrl+0 did not reset the video editor preview to its starting ' + startPct + '% (got ' + fitPctVe + '%)');
        }
      }

      // 473: zoomed in, every corner of the picture can be panned up to the
      // middle of the stage instead of stopping at the stage border.
      if (zoomValVe) {
        const readPct473 = () => Number(zoomValVe.textContent.replace('%', ''));
        const stageVe = document.getElementById('ve-stage');
        for (let i = 0; i < 20 && readPct473() < 400; i++) {
          document.dispatchEvent(new KeyboardEvent('keydown', { key: '+', ctrlKey: true, bubbles: true }));
          await sleep(20);
        }
        const v = document.getElementById('ve-video');
        if (readPct473() <= 100 || !v || !v.videoWidth) {
          // No decoded video in this harness (or the zoom keys never reached
          // above 100%): nothing to measure a picture box against, so this
          // check is skipped rather than faked.
        } else {
          const pan = (dx, dy) => {
            stageVe.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: 0, clientY: 0 }));
            stageVe.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, button: 0, pointerId: 1, clientX: dx, clientY: dy }));
            stageVe.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, pointerId: 1, clientX: dx, clientY: dy }));
          };
          const pictureBox = () => {
            const videoRect = v.getBoundingClientRect();
            const stageRect = stageVe.getBoundingClientRect();
            const zoom = videoRect.width / stageRect.width;
            const s = Math.min(stageRect.width / v.videoWidth, stageRect.height / v.videoHeight);
            const picW = v.videoWidth * s * zoom, picH = v.videoHeight * s * zoom;
            const vcx = videoRect.left + videoRect.width / 2, vcy = videoRect.top + videoRect.height / 2;
            return { left: vcx - picW / 2, top: vcy - picH / 2, right: vcx + picW / 2, bottom: vcy + picH / 2, stageRect };
          };
          pan(5000, 5000);
          await sleep(30);
          {
            const { left, top, stageRect } = pictureBox();
            const scx = stageRect.left + stageRect.width / 2, scy = stageRect.top + stageRect.height / 2;
            if (Math.abs(left - scx) > 2) errs.push('473: the picture left edge cannot be panned to the middle of the stage');
            if (Math.abs(top - scy) > 2) errs.push('473: the picture top edge cannot be panned to the middle of the stage');
          }
          pan(-5000, -5000);
          await sleep(30);
          {
            const { right, bottom, stageRect } = pictureBox();
            const scx = stageRect.left + stageRect.width / 2, scy = stageRect.top + stageRect.height / 2;
            if (Math.abs(right - scx) > 2) errs.push('473: the picture right edge cannot be panned to the middle of the stage');
            if (Math.abs(bottom - scy) > 2) errs.push('473: the picture bottom edge cannot be panned to the middle of the stage');
          }
          document.getElementById('ve-preview-zoom-reset')?.click();
          await sleep(30);
          if (v.style.transform !== '') {
            errs.push('473: resetting the preview zoom left a stale transform on the video element');
          }
        }
      }

      // The video editor selects an overlay from its own list rather than by
      // clicking its canvas rectangle -- renderRects() only ever draws the
      // rect and handles of the overlay already selected, so there is
      // nothing on the canvas to click before that. Selecting the pixelate
      // overlay (the same rectangular-region tool the screenshot editor
      // unified the wording for) must still show the same 8 handles.
      // Earlier edits above (355/356: the text and image overlays each got a
      // field or drag commit) replace-and-reappend the edited op, so the list
      // no longer sits in its original add order -- find the pixelate row by
      // its own label rather than assuming overlayKinds' add-time position.
      const ovRows383 = [...document.querySelectorAll('#ve-ov-list .ve-ov-row')];
      const pixelRow = ovRows383.find((r) => r.textContent.startsWith('Pixelate'));
      if (!pixelRow) {
        errs.push('383: could not find the pixelate overlay row to select for the handle count');
      } else {
        pixelRow.click();
        await sleep(30);
        const veHandles = document.querySelectorAll('#ve-rects .ve-rect-pixelate .ve-rect-handle');
        if (veHandles.length !== 8) {
          errs.push('383: selecting the pixelate overlay showed ' + veHandles.length + ' handles, expected 8');
        }
      }
    }

    // ---- 395: with an overlay selected, arrow keys move it (by numFieldStep,
    // default 1 -- see tests/ui/tauri_stub.js), Ctrl+Arrow resizes it instead,
    // comma/period take over frame-stepping, and Escape gives the arrow keys
    // back to the playhead. The pixelate overlay is still selected from the
    // 383 check just above -- being a rect-based overlay its x/y/w/h must
    // stay on even numbers (see snapRect() in video.js), so each press here
    // moves it by 2 (the nearest even number to the default step of 1), not
    // by the step itself; a text overlay has no such rect and moves by the
    // exact step, which is covered by the screenshot editor's own 395 check. ----
    {
      const xField395 = document.querySelector('#ve-ov-editor [data-ov-field="rect.x"]');
      const wField395 = document.querySelector('#ve-ov-editor [data-ov-field="rect.w"]');
      const hintEl = document.getElementById('ve-nudge-hint');
      if (!xField395 || !wField395 || !hintEl) {
        errs.push('395: could not find the selected overlay rect fields or the nudge hint element');
      } else {
        if (hintEl.classList.contains('hidden')) {
          errs.push('395: the nudge hint is not shown while an overlay is selected');
        }
        if (hintEl.offsetHeight !== 0) {
          errs.push('395: the nudge hint takes layout height, so showing it resizes the preview');
        }
        const beforeX = Number(xField395.value);
        for (let i = 0; i < 4; i++) {
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
          await sleep(20);
        }
        const afterX = Number(document.querySelector('#ve-ov-editor [data-ov-field="rect.x"]').value);
        if (afterX !== beforeX + 8) {
          errs.push('395: four ArrowRight presses moved the selected overlay by ' + (afterX - beforeX) + ', expected exactly 8 (2 per press, rounded up to stay even)');
        }
        const beforeW = Number(document.querySelector('#ve-ov-editor [data-ov-field="rect.w"]').value);
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', ctrlKey: true, bubbles: true, cancelable: true }));
        await sleep(20);
        const afterW = Number(document.querySelector('#ve-ov-editor [data-ov-field="rect.w"]').value);
        if (afterW <= beforeW) {
          errs.push('395: Ctrl+ArrowRight did not grow the selected overlay instead of moving it (was ' + beforeW + ', now ' + afterW + ')');
        }
        const beforeFrame = document.getElementById('ve-frame').textContent;
        document.dispatchEvent(new KeyboardEvent('keydown', { key: '.', bubbles: true, cancelable: true }));
        await sleep(30);
        const afterFrame = document.getElementById('ve-frame').textContent;
        if (afterFrame === beforeFrame) {
          errs.push('395: period did not step a frame forward while an overlay is selected');
        }
        // Escape deselects and hands the arrow keys back to frame-stepping,
        // rather than also popping the close-window confirm dialog.
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
        await sleep(30);
        if (!document.getElementById('confirm-modal').classList.contains('hidden')) {
          errs.push('395: Escape while an overlay was selected also opened the close-window confirm dialog');
        }
        if (!hintEl.classList.contains('hidden')) {
          errs.push('395: the nudge hint stayed visible after Escape deselected the overlay');
        }
        const frameAfterDeselect = document.getElementById('ve-frame').textContent;
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
        await sleep(30);
        const frameAfterArrow = document.getElementById('ve-frame').textContent;
        if (frameAfterArrow === frameAfterDeselect) {
          errs.push('395: ArrowRight did not move the playhead once nothing was selected');
        }
      }
    }

    // ---- 398: the show-original button (and V) hide the overlay and edit-
    // handle layers so the live preview compares against the untouched source
    // (fades and the export scale target never render live in the first
    // place, so hiding these two is the whole of it here) -- a quick click
    // locks it on until clicked again, holding it only peeks for as long as
    // it stays held, and a badge marks either state. ----
    {
      const origBtn398 = document.getElementById('ve-original');
      const origBadge398 = document.getElementById('ve-original-badge');
      const overlayLive398 = document.getElementById('ve-overlay-live');
      const rects398 = document.getElementById('ve-rects');
      if (!origBtn398 || !origBadge398 || !overlayLive398 || !rects398) {
        errs.push('398: the video editor is missing the show-original button, badge or overlay/rects layers');
      } else {
        if (overlayLive398.classList.contains('hidden') || rects398.classList.contains('hidden')) {
          errs.push('398: the overlay or rects layer starts hidden before the show-original check even runs');
        }
        origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 4 }));
        origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 4 }));
        await sleep(30);
        if (!overlayLive398.classList.contains('hidden') || !rects398.classList.contains('hidden')) {
          errs.push('398: a quick click on the show-original button did not hide the overlay and rects layers');
        }
        if (origBadge398.classList.contains('hidden')) errs.push('398: the original badge did not appear once the original was locked on');
        if (!origBtn398.classList.contains('active')) errs.push('398: the show-original button did not switch to its active state while locked on');
        origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 4 }));
        origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 4 }));
        await sleep(30);
        if (overlayLive398.classList.contains('hidden') || rects398.classList.contains('hidden')) {
          errs.push('398: a second click did not lock the original back off');
        }
        if (!origBadge398.classList.contains('hidden')) errs.push('398: the original badge stayed after the second click turned it off');
        origBtn398.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 4 }));
        await sleep(450);
        if (!overlayLive398.classList.contains('hidden')) errs.push('398: holding the show-original button down did not hide the overlay layer');
        origBtn398.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 4 }));
        await sleep(30);
        if (overlayLive398.classList.contains('hidden')) errs.push('398: releasing a held show-original press did not restore the overlay layer');
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'v', code: 'KeyV', bubbles: true }));
        await sleep(450);
        if (!overlayLive398.classList.contains('hidden')) errs.push('398: holding V did not hide the overlay layer');
        document.dispatchEvent(new KeyboardEvent('keyup', { key: 'v', code: 'KeyV', bubbles: true }));
        await sleep(30);
        if (overlayLive398.classList.contains('hidden')) errs.push('398: releasing V did not restore the overlay layer');
      }
    }

    // Both 358 probes below depend on tick()'s own requestAnimationFrame loop
    // actually firing across several real sleeps -- headless Chromium stops
    // producing frames once the page is quiet (see the heartbeat comment
    // earlier in this file), which would otherwise starve tick() of any
    // chance to run during the waits below. Same fix: keep something dirty.
    const beat358 = setInterval(() => {
      document.documentElement.style.setProperty('--smoke-tick-358', String(Date.now()));
      document.body.getBoundingClientRect();
    }, 20);

    // ---- 358: the live-preview element for an overlay is REUSED across
    // renders, not rebuilt (a fresh <img>/<canvas> every frame was the wear
    // pattern behind "the preview works at first and then stops") ----
    {
      const img0 = document.querySelector('#ve-overlay-live .ve-live-image');
      if (!img0) {
        errs.push('358: no live image element to check for node reuse');
      } else {
        img0._probeMarker = 'kept';
        for (let i = 0; i < 5; i++) {
          document.dispatchEvent(new KeyboardEvent('keydown', { key: i % 2 ? 'Home' : 'End', bubbles: true }));
          await sleep(30);
        }
        const img1 = document.querySelector('#ve-overlay-live .ve-live-image');
        if (img1 !== img0 || img1._probeMarker !== 'kept') {
          errs.push('358: the live image overlay element was rebuilt instead of reused across renders');
        }
        const allImages = document.querySelectorAll('#ve-overlay-live .ve-live-image');
        if (allImages.length !== 1) {
          errs.push('358: expected exactly one live image element, found ' + allImages.length);
        }
      }
    }

    // ---- 358: a throw inside tick() must be caught once (toasted) and must
    // not stop the frame loop from correcting the playhead on a later tick ----
    {
      const waitFor358 = async (fn) => { for (let i = 0; i < 20 && !fn(); i++) await sleep(50); return fn(); };
      if (document.activeElement) document.activeElement.blur();
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }));
      await waitFor358(() => document.getElementById('ve-time').textContent === '10:00.000');
      if (document.getElementById('ve-time').textContent !== '10:00.000') {
        errs.push('358: setup: End did not seek to the end of the clip');
      }
      const videoElD = document.getElementById('ve-video');
      const playheadEl = document.getElementById('ve-playhead');
      const realId = playheadEl.id;
      playheadEl.id = 've-playhead-broken-for-test';
      // The stub video mirrors any currentTime assignment back immediately, so
      // going through seek() (which sets VE.playhead to the same value at the
      // same time) never leaves tick() anything to correct. Desyncing
      // currentTime directly, bypassing seek(), is what actually forces the
      // next tick() into its mismatch-correction branch -- the one that
      // touches the (now-broken) #ve-playhead node and should throw.
      videoElD.currentTime = 0;
      const toastEl = document.getElementById('ve-toast');
      await waitFor358(() => !toastEl.classList.contains('hidden') && !!toastEl.textContent);
      if (toastEl.classList.contains('hidden') || !toastEl.textContent || toastEl.textContent === 'veTickError') {
        errs.push('358: a failing tick() did not report itself once via the toast');
      }
      playheadEl.id = realId;
      // Desync again so a later, now-healthy tick has something to correct.
      videoElD.currentTime = 5;
      await waitFor358(() => document.getElementById('ve-time').textContent === '00:05.000');
      if (document.getElementById('ve-time').textContent !== '00:05.000') {
        errs.push('358: the frame loop did not keep correcting the playhead on a later, healthy tick');
      }
    }
    clearInterval(beat358);

    // ---- 359: a replacing op's own _prev undo chain must be capped at
    // undoMax links, not grow forever with every re-edit of the same overlay
    // (editing one overlay 500 times used to keep 500 linked copies alive) ----
    {
      const frameRowIdx = overlayKinds.findIndex(([id]) => id === 've-ov-add-frame');
      let opacityField = null;
      if (ovRows[frameRowIdx]) {
        ovRows[frameRowIdx].click();
        await sleep(30);
        opacityField = document.querySelector('#ve-ov-editor [data-ov-field="opacity"]');
      }
      if (!opacityField) {
        errs.push('359: could not find the frame overlay opacity field to drive 500 commits through');
      } else {
        for (let i = 0; i < 500; i++) {
          opacityField.value = String(10 + (i % 80));
          opacityField.dispatchEvent(new Event('change'));
        }
        await sleep(30);
        const rowsBefore = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
        document.activeElement && document.activeElement.blur();
        // undoMax defaults to 40 (UNDO_MAX_DEFAULT, unset by this fixture): the
        // frame overlay's chain must be exhausted at exactly that many undos,
        // never sooner and never later than the SINGLE row it owns disappearing.
        for (let i = 0; i < 40; i++) {
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', code: 'KeyZ', ctrlKey: true, bubbles: true }));
        }
        await sleep(30);
        const rowsAfter = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
        if (rowsAfter !== rowsBefore - 1) {
          errs.push('359: the _prev undo chain was not capped at undoMax -- expected ' + (rowsBefore - 1) +
            ' op row(s) after 40 undos into 500 commits, found ' + rowsAfter);
        }
      }
    }

    // ---- 392: duplicate an overlay from its row context menu; undo removes
    // only the duplicate, nothing else in the list ----
    {
      const rowsDup = [...document.querySelectorAll('#ve-ov-list .ve-ov-row')];
      const pixelRowDup = rowsDup.find((r) => r.textContent.startsWith('Pixelate'));
      if (!pixelRowDup) {
        errs.push('392: could not find the pixelate overlay row to duplicate');
      } else {
        const beforeCount = rowsDup.length;
        const box = pixelRowDup.getBoundingClientRect();
        pixelRowDup.dispatchEvent(new MouseEvent('contextmenu', {
          bubbles: true, clientX: box.left + 4, clientY: box.top + 4,
        }));
        await sleep(30);
        const ctxMenu = document.querySelector('.ctx');
        const ctxBtns = ctxMenu ? [...ctxMenu.querySelectorAll('button')] : [];
        if (!ctxMenu || ctxMenu.classList.contains('hidden') || ctxBtns.length !== 2) {
          errs.push('392: right-clicking an overlay row did not open a two-action context menu');
        } else {
          ctxBtns[0].click();
          await sleep(30);
          const afterCount = document.querySelectorAll('#ve-ov-list .ve-ov-row').length;
          if (afterCount !== beforeCount + 1) {
            errs.push('392: duplicating an overlay produced ' + afterCount + ' rows, expected ' + (beforeCount + 1));
          }
          if (document.activeElement) document.activeElement.blur();
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', code: 'KeyZ', ctrlKey: true, bubbles: true }));
          await sleep(30);
          const undoCount = document.querySelectorAll('#ve-ov-list .ve-ov-row').length;
          if (undoCount !== beforeCount) {
            errs.push('392: undo after duplicating left ' + undoCount + ' row(s), expected exactly ' + beforeCount +
              ' (undo must remove only the duplicate)');
          }
        }
      }
    }

    // ---- 416: "Add a video" as a section clip updates the timeline scale,
    // and an unreadable extra source is rejected before it reaches the list ----
    {
      const beforeText = document.getElementById('ve-cut-count').textContent;
      document.getElementById('ve-add-video').click();
      await sleep(30);
      const pickPop = document.getElementById('ve-add-video-pop');
      if (!pickPop || pickPop.classList.contains('hidden') || pickPop.querySelectorAll('button').length !== 2) {
        errs.push('416: "Add a video" did not open the overlay/section choice');
      } else {
        pickPop.querySelectorAll('button')[1].click(); // "As its own section"
        await sleep(30);
        const posButtons = pickPop.querySelectorAll('button');
        if (pickPop.classList.contains('hidden') || posButtons.length !== 3) {
          errs.push('416: choosing "as its own section" did not open the start/end/current-position choice');
        } else {
          posButtons[1].click(); // "At the end"
          await sleep(30);
          const clipList = document.getElementById('ve-clip-list');
          const clipRow = clipList ? clipList.querySelector('.ve-op-row') : null;
          if (!clipRow || clipList.classList.contains('hidden')) {
            errs.push('416: the inserted section did not appear in the clip list');
          } else if (!/ps-smoke-badge/.test(clipRow.textContent)) {
            errs.push('416: the clip row did not show the name of the inserted file: ' + clipRow.textContent);
          }
          const afterText = document.getElementById('ve-cut-count').textContent;
          if (afterText === beforeText) {
            errs.push('416: the total-length readout did not change once a section was inserted (timeline scale not updated)');
          }
          // Clean up so later probes see the plain single-source state again.
          const del = clipRow ? clipRow.querySelector('.ve-op-del') : null;
          if (del) { del.click(); await sleep(30); }
        }
      }
    }
    {
      window.__EXTRA_VIDEO_FAIL__ = true;
      const beforeCount = document.querySelectorAll('#ve-clip-list .ve-op-row').length;
      document.getElementById('ve-add-video').click();
      await sleep(30);
      window.__EXTRA_VIDEO_FAIL__ = false;
      const toastEl = document.getElementById('ve-toast');
      const pickPop = document.getElementById('ve-add-video-pop');
      const popOpened = pickPop && !pickPop.classList.contains('hidden') && pickPop.querySelectorAll('button').length;
      if (popOpened) {
        errs.push('416: an unreadable extra video source still opened the overlay/section choice');
      }
      if (!toastEl || toastEl.classList.contains('hidden') || toastEl.textContent.includes('ffErrUnreadable')) {
        errs.push('416: an unreadable extra video source was not rejected with plain-language wording, got: ' +
          (toastEl ? toastEl.textContent : '(no toast)'));
      }
      const afterCount = document.querySelectorAll('#ve-clip-list .ve-op-row').length;
      if (afterCount !== beforeCount) {
        errs.push('416: an unreadable extra video source was added to the clip list anyway');
      }
    }

    // ---- 834: an error string the map does not know shows the generic text and
    // writes exactly one log_error entry carrying the raw text ----
    {
      window.__EXTRA_VIDEO_FAIL_RAW__ = 'weird backend failure 834';
      const before = (window.__SMOKE_ARGS__ || []).length;
      document.getElementById('ve-add-video').click();
      await sleep(60);
      window.__EXTRA_VIDEO_FAIL_RAW__ = null;
      const logged = (window.__SMOKE_ARGS__ || []).slice(before).filter(([c, a]) => c === 'log_error');
      if (logged.length !== 1) {
        errs.push('834: an unmapped backend error wrote ' + logged.length + ' log_error entries, expected 1');
      } else if (!logged[0][1] || logged[0][1].raw !== 'weird backend failure 834') {
        errs.push('834: the log_error entry lost the raw text: ' + JSON.stringify(logged[0][1]));
      }
      const toastEl = document.getElementById('ve-toast');
      if (!toastEl || toastEl.textContent.includes('weird backend failure')) {
        errs.push('834: the raw backend text reached the person');
      }
    }

    // ---- 392: the last colour picked for a text overlay is remembered as the
    // starting colour for the next text overlay ----
    {
      const rowsRem = [...document.querySelectorAll('#ve-ov-list .ve-ov-row')];
      const textRowRem = rowsRem.find((r) => r.textContent.startsWith('Text'));
      if (!textRowRem) {
        errs.push('392: could not find the text overlay row to test the remembered colour');
      } else {
        textRowRem.click();
        await sleep(30);
        const pick = document.querySelector('#ve-ov-editor .swatch[title="#ef4444"]');
        if (!pick) {
          errs.push('392: no red swatch to pick for the remembered-colour check');
        } else {
          pick.click();
          await sleep(30);
          const afterPick = await window.__TAURI__.core.invoke('get_settings');
          if ((afterPick.ui_texts || {}).veOvLastTextColor !== '#ef4444') {
            errs.push('392: picking a text overlay colour did not remember it as veOvLastTextColor');
          }
          // The new overlay is already selected by addOverlay() itself -- clicking
          // its row again would TOGGLE the selection back off (ve-ov-row's own
          // click handler flips VE.selOv when it already matches).
          document.getElementById('ve-ov-add-text').click();
          await sleep(30);
          const selSwatch = document.querySelector('#ve-ov-editor .swatch.sel');
          if (!selSwatch || selSwatch.title !== '#ef4444') {
            errs.push('392: a new text overlay did not start with the remembered colour, got ' +
              (selSwatch && selSwatch.title));
          }
        }
      }
    }

    // Export opens from the title bar button and from its own tab; the Export tab is the selected
    // one then, the start button has the focus, and the Edit tab leads back.
    document.getElementById('ve-export-open').click();
    await sleep(80);
    if (document.getElementById('ve-panel-export').classList.contains('hidden')) {
      errs.push('the export button did not reveal the export panel');
    }
    if (document.getElementById('ve-tab-export').getAttribute('aria-selected') !== 'true'
        || [...document.querySelectorAll('.ve-tab')].filter((t) => t.getAttribute('aria-selected') === 'true').length !== 1) {
      errs.push('987: the Export tab is not the one selected tab while the export panel is shown');
    }
    if (document.activeElement.id !== 've-export-start') {
      errs.push('935: the export start button did not take the focus: ' + document.activeElement.id);
    }
    if (!document.querySelector('.ve-tab[data-panel="ve-panel-export"]')) errs.push('987: the Export tab is missing');
    {
      const tabEdit = document.getElementById('ve-tab-edit');
      tabEdit.click();
      await sleep(60);
      if (document.getElementById('ve-panel-edit').classList.contains('hidden')) errs.push('935: the Edit tab did not lead back from the export panel');
      document.getElementById('ve-export-open').click();
      await sleep(60);
    }
    rawKeyCheck('export tab');
    sideScan(errs, 'export tab');

    // ---- 366: free width/height/percent scale fields, lock-aspect,
    // snap-to-even, the fixed-height anti-upscale guard, the Export tab's
    // read-only summary + "Change..." button, and undo ----
    document.getElementById('ve-tab-edit').click();
    await sleep(80);
    {
      const wField = document.getElementById('ve-scale-w');
      const hField = document.getElementById('ve-scale-h');
      const lockBox = document.getElementById('ve-scale-lock');
      const opsBefore = document.querySelectorAll('#ve-ops-list .ve-op-row').length;

      if (!lockBox.checked) errs.push('366: aspect lock is not on by default');
      // 1280x720 on the 1920x1080 fixture, lock on -- height must follow.
      wField.value = '1280';
      wField.dispatchEvent(new Event('change'));
      await sleep(30);
      if (wField.value !== '1280' || hField.value !== '720') {
        errs.push('366: locked width change did not carry height to 720, got ' + wField.value + 'x' + hField.value);
      }
      const rowsAfterScale = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
      if (rowsAfterScale !== opsBefore + 1) {
        errs.push('366: setting a scale did not add exactly one op row');
      }
      const scaleRow = [...document.querySelectorAll('#ve-ops-list .ve-op-row')].pop();
      if (!scaleRow || !scaleRow.textContent.includes('1280') || !scaleRow.textContent.includes('720')) {
        errs.push('366: the op row does not show the 1280x720 target: ' + (scaleRow && scaleRow.textContent));
      }

      // Unlock: a width change must leave the height field alone.
      lockBox.checked = false;
      lockBox.dispatchEvent(new Event('change'));
      await sleep(30);
      wField.value = '800';
      wField.dispatchEvent(new Event('change'));
      await sleep(30);
      if (hField.value !== '720') {
        errs.push('366: an unlocked width change moved the height field, got ' + hField.value);
      }

      // An odd width always snaps to an even number.
      wField.value = '801';
      wField.dispatchEvent(new Event('change'));
      await sleep(30);
      if (Number(wField.value) % 2 !== 0) {
        errs.push('366: an odd width was not snapped to an even number, got ' + wField.value);
      }

      // Re-lock, then the 25% preset must set both numbers and mark itself selected.
      lockBox.checked = true;
      lockBox.dispatchEvent(new Event('change'));
      await sleep(30);
      const presetBtns = [...document.querySelectorAll('#ve-scale-presets button')];
      const pct25 = presetBtns.find((b) => b.dataset.key === '25');
      if (!pct25) {
        errs.push('366: no 25% preset button found');
      } else {
        pct25.click();
        await sleep(30);
        if (wField.value !== '480' || hField.value !== '270') {
          errs.push('366: the 25% preset did not set 480x270, got ' + wField.value + 'x' + hField.value);
        }
        if (!pct25.classList.contains('sel')) {
          errs.push('366: the 25% preset button is not marked selected after being picked');
        }
      }

      // The Export tab mirrors the same target as a read-only summary (no Change button) and
      // shows the resulting size as plain text.
      document.getElementById('ve-export-open').click();
      await sleep(80);
      const summary = document.getElementById('ve-export-scale-summary').textContent;
      if (summary.trim() !== '480 × 270 px') {
        errs.push('366: the export-tab scale summary is not the scaled size 480 x 270: ' + summary);
      }
      if (document.getElementById('ve-export-scale-edit')) {
        errs.push('366: the export tab still has a Change button next to the scale summary');
      }

      // A fixed-height preset at or above the source size leaves the frame
      // untouched instead of upscaling it (the source here is 1080p).
      const rowsBeforeGuard = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
      const preset1080 = presetBtns.find((b) => b.dataset.key === '1080');
      if (!preset1080) {
        errs.push('366: no 1080p preset button found');
      } else {
        preset1080.click();
        await sleep(30);
        const rowsAfterGuard = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
        if (rowsAfterGuard !== rowsBeforeGuard - 1) {
          errs.push('366: picking the 1080p preset on a 1080p source did not clear the scale op (anti-upscale guard)');
        }
        if (wField.value !== '1920' || hField.value !== '1080') {
          errs.push('366: after the guard clears the scale, the fields do not show the source size, got ' + wField.value + 'x' + hField.value);
        }
      }

      // Undo restores the original resolution.
      wField.value = '1280';
      wField.dispatchEvent(new Event('change'));
      await sleep(30);
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', code: 'KeyZ', ctrlKey: true, bubbles: true }));
      await sleep(30);
      if (wField.value !== '1920' || hField.value !== '1080') {
        errs.push('366: undo did not restore the original resolution, got ' + wField.value + 'x' + hField.value);
      }
    }

    // ---- 335: the export destination input + its two picker buttons ----
    const destInput = document.getElementById('ve-export-dest');
    if (!destInput.value) {
      errs.push('the export destination field starts empty');
    }
    destInput.value = 'C:\\\\Custom\\\\my-clip.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);
    if (destInput.value !== 'C:\\\\Custom\\\\my-clip.mp4') {
      errs.push('typing a destination path directly did not stick as an override');
    }
    const pickedFile = destInput.value;
    document.getElementById('ve-export-choose-folder').click();
    await sleep(80);
    if (destInput.value === pickedFile) {
      errs.push('"Export to..." did not change the destination after picking a folder');
    }
    if (!destInput.value.startsWith('C:\\\\Videos\\\\Exports')) {
      errs.push('"Export to..." did not build the path from the picked folder');
    }
    // An externally opened file suggests its own folder, but a folder picked here wins.
    const { VE } = await import('./video.js');
    const { updateExportNote } = await import('./videoExport.js');
    VE.originDir = 'C:\\\\Elsewhere';
    updateExportNote();
    if (!destInput.value.startsWith('C:\\\\Elsewhere')) {
      errs.push('an externally opened file does not suggest its own folder');
    }
    document.getElementById('ve-export-choose-folder').click();
    await sleep(80);
    if (!destInput.value.startsWith('C:\\\\Videos\\\\Exports')) {
      errs.push('"Export to..." lost against the folder of an externally opened file: ' + destInput.value);
    }

    // ---- the plain "File name" field, separate from the full path above --
    // a valid custom name flows straight into the export call, an invalid
    // one shows an inline hint and disables Export, but leaves Copy to
    // clipboard alone since that one never reads this field ----
    const nameInput = document.getElementById('ve-export-name');
    const nameHint = document.getElementById('ve-export-name-hint');
    nameInput.value = 'Demo 1';
    nameInput.dispatchEvent(new Event('input'));
    await sleep(30);
    if (!destInput.value.endsWith('Demo 1.mp4')) {
      errs.push('typing "Demo 1" into the file-name field did not update the export destination: ' + destInput.value);
    }
    if (!nameHint.classList.contains('hidden')) {
      errs.push('a valid file name still shows the invalid-name hint');
    }
    window.__SMOKE_ARGS__ = [];
    document.getElementById('ve-export-start').click();
    await sleep(150);
    const nameExportCall = (window.__SMOKE_ARGS__ || []).find(([cmd]) => cmd === 'export_video');
    if (!nameExportCall || !String(nameExportCall[1].req.dest_path).endsWith('Demo 1.mp4')) {
      errs.push('export_video was not called with the typed file name');
    }
    nameInput.value = 'a/b';
    nameInput.dispatchEvent(new Event('input'));
    await sleep(30);
    if (nameHint.classList.contains('hidden')) {
      errs.push('an invalid file name ("a/b") does not show the inline hint');
    }
    if (!nameInput.classList.contains('invalid')) {
      errs.push('an invalid file name does not mark the field itself invalid');
    }
    if (!document.getElementById('ve-export-start').disabled) {
      errs.push('an invalid file name does not disable Export');
    }
    // Copy to clipboard writes to its own throwaway temp name and never
    // reads this field, so an invalid name here must not block it.
    if (document.getElementById('ve-export-clip').disabled) {
      errs.push('an invalid file name wrongly disables Copy to clipboard');
    }
    // Leave the field valid again so the probe steps below are not blocked
    // by this one's own leftover invalid state.
    nameInput.value = 'ps-smoke-recording-edit';
    nameInput.dispatchEvent(new Event('input'));
    await sleep(30);

    // ---- 503: a destination the backend does not recognize (e.g. a file
    // opened from outside the app) opens the export-folder picker and
    // retries once instead of failing outright ----
    window.__EXPORT_DEST_REFUSED_ONCE__ = false;
    window.__SMOKE_CALLS__ = [];
    destInput.value = 'C:\\\\Outside\\\\REFUSEDEST-clip.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);
    window.__PICK_FOLDER__ = 'C:\\\\Retry\\\\Folder';
    document.getElementById('ve-export-start').click();
    await sleep(250);
    if (!window.__SMOKE_CALLS__.includes('pick_export_folder')) {
      errs.push('503: a refused export destination did not open the export-folder picker');
    }
    const retryStatus = document.getElementById('ve-export-status').textContent;
    if (!retryStatus.includes('C:\\\\Retry\\\\Folder')) {
      errs.push(`503: the retried export did not land in the picked folder, status reads ${JSON.stringify(retryStatus)}`);
    }
    if (document.getElementById('ve-export-saveas').classList.contains('hidden')) {
      errs.push('503: the retried export did not complete successfully');
    }
    // The one control that acts on a finished export reads "Save as..." (it moves
    // the file) with a tooltip, and the old pre-export file picker is gone: the name
    // field plus "Export to..." cover it. The row reads like the screenshot editor's:
    // Cancel, Save as, Copy, then the primary Export; Cancel is always there.
    {
      const moveBtn = document.getElementById('ve-export-saveas');
      const moveText = moveBtn.textContent.trim();
      if (moveText !== 'Save as…') errs.push('547: the post-export button reads ' + JSON.stringify(moveText) + ', expected Save as...');
      if (!moveBtn.title || !moveBtn.title.includes('exported')) errs.push('547: the Save as button has no tooltip: ' + JSON.stringify(moveBtn.title));
      if (document.getElementById('ve-export-choose-file')) errs.push('547: the pre-export Save as button still exists');
      const saveAsLike = [...document.querySelectorAll('#ve-panel-export button')].filter((b) => /Save as|Speichern unter/.test(b.textContent) && b.id !== 've-export-saveas');
      if (saveAsLike.length) errs.push('547: a Save as button is still in the export panel: ' + saveAsLike.map((b) => b.id).join(','));
      const rowIds = [...document.getElementById('ve-export-cancel').parentElement.querySelectorAll('button')].map((b) => b.id).join(',');
      if (rowIds !== 've-export-cancel,ve-export-saveas,ve-export-clip,ve-export-start') errs.push('673: the export row order is ' + rowIds);
      if (document.getElementById('ve-export-cancel').classList.contains('hidden')) errs.push('673: Cancel is hidden although no export runs');
      if (document.getElementById('ve-export-clip').textContent.trim() !== 'Copy') errs.push('673: the clipboard button reads ' + JSON.stringify(document.getElementById('ve-export-clip').textContent.trim()));
    }
    window.__PICK_FOLDER__ = null;

    // ---- 503: cancelling that picker is a plain no-op -- no export, no error ----
    window.__EXPORT_DEST_REFUSED_ONCE__ = false;
    destInput.value = 'C:\\\\Outside\\\\REFUSEDEST-cancel.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);
    window.__PICK_FOLDER__ = null; // cancel
    window.__SMOKE_CALLS__ = [];
    document.getElementById('ve-export-start').click();
    await sleep(250);
    if (!window.__SMOKE_CALLS__.includes('pick_export_folder')) {
      errs.push('503: cancelling the retry picker was never even offered');
    }
    if (document.getElementById('ve-export-status').textContent.toLowerCase().includes('not recognized')) {
      errs.push('503: a cancelled retry showed the raw backend refusal instead of staying silent');
    }
    destInput.value = 'C:\\\\Custom\\\\my-clip.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);

    const formatSel = document.getElementById('ve-export-format');
    // 508: the export format/quality selects, themed across every built-in
    // theme -- a bare select used to fall back to the plain OS look.
    {
      const priorTheme = document.documentElement.getAttribute('data-theme');
      const BUILTIN_THEMES508 = ['light', 'dark', 'programmer', 'neon', 'cyberpunk', 'ocean'];
      const swatch = document.createElement('div');
      swatch.style.cssText = 'position:absolute;left:-9999px;top:-9999px;background-color:var(--surface)';
      document.body.appendChild(swatch);
      const qSel508 = document.getElementById('ve-export-quality');
      for (const theme of BUILTIN_THEMES508) {
        document.documentElement.setAttribute('data-theme', theme);
        const surfaceRgb = getComputedStyle(swatch).backgroundColor;
        for (const sel of [formatSel, qSel508]) {
          if (!sel) continue;
          const label = sel.id;
          const cs = getComputedStyle(sel);
          if (parseFloat(cs.borderTopWidth) <= 0) errs.push('508: ' + label + ' has no border in theme ' + theme);
          if (cs.backgroundColor === 'rgba(0, 0, 0, 0)' || cs.backgroundColor === 'transparent') {
            errs.push('508: ' + label + ' has a transparent background in theme ' + theme);
          }
          // A selected option shows the accent colour (option:checked); pick
          // an unselected one to check the plain surface colour instead.
          const unselected = [...sel.options].find((o) => !o.selected);
          if (unselected) {
            const optBg = getComputedStyle(unselected).backgroundColor;
            if (optBg !== surfaceRgb) {
              errs.push('508: ' + label + ' option background is ' + optBg + ', expected ' + surfaceRgb + ' in theme ' + theme);
            }
          }
        }
      }
      swatch.remove();
      if (priorTheme) document.documentElement.setAttribute('data-theme', priorTheme);
      else document.documentElement.removeAttribute('data-theme');
    }
    const otherOpt = formatSel && [...formatSel.options].find((o) => o.value !== formatSel.value);
    if (otherOpt) {
      const beforeFormatChange = destInput.value;
      formatSel.value = otherOpt.value;
      formatSel.dispatchEvent(new Event('change'));
      await sleep(30);
      if (destInput.value === beforeFormatChange) {
        errs.push('changing the export format did not clear the manual destination override');
      }
    }

    // ---- 367: the avi export format -- extension follows, audio controls
    // and the quality dropdown are disabled exactly for the silent one (gif) ----
    const newFormats = ['avi'];
    const formatExpectedExt = {};
    const silentFormats = new Set(['gif']);
    const optionValues = formatSel ? [...formatSel.options].map((o) => o.value) : [];
    for (const fmt of newFormats) {
      if (!optionValues.includes(fmt)) errs.push(`367: export format select is missing an option for "${fmt}"`);
    }
    const muteMic = document.getElementById('ve-mute-mic');
    const volMic = document.getElementById('ve-vol-mic');
    const muteSys = document.getElementById('ve-mute-sys');
    const volSys = document.getElementById('ve-vol-sys');
    const qualitySel = document.getElementById('ve-export-quality');
    for (const fmt of [...newFormats, 'gif', 'mp4']) {
      if (!optionValues.includes(fmt)) continue;
      formatSel.value = fmt;
      formatSel.dispatchEvent(new Event('change'));
      await sleep(30);
      const wantExt = formatExpectedExt[fmt] || fmt;
      if (!destInput.value.endsWith(`.${wantExt}`)) {
        errs.push(`367: picking "${fmt}" left the destination as "${destInput.value}" instead of ending in .${wantExt}`);
      }
      const wantSilent = silentFormats.has(fmt);
      if (qualitySel.disabled !== wantSilent) {
        errs.push(`367: quality dropdown disabled=${qualitySel.disabled} for "${fmt}", expected ${wantSilent}`);
      }
      for (const [name, el] of [['mute-mic', muteMic], ['vol-mic', volMic], ['mute-sys', muteSys], ['vol-sys', volSys]]) {
        if (wantSilent && !el.disabled) {
          errs.push(`367: ${name} stayed enabled for the silent format "${fmt}"`);
        }
      }
    }
    // Leaving the destination on the last silent format's ".gif" name
    // would break the next real test step below -- put it back on mp4.
    formatSel.value = 'mp4';
    formatSel.dispatchEvent(new Event('change'));
    await sleep(30);

    // ---- 412: the export format note changes with the format, names the
    // silent formats' missing audio, and carries a size estimate ----
    const exportNoteEl = document.getElementById('ve-export-note');
    if (optionValues.includes('avi')) {
      formatSel.value = 'avi';
      formatSel.dispatchEvent(new Event('change'));
      await sleep(30);
      const aviNote = exportNoteEl.textContent;
      formatSel.value = 'mp4';
      formatSel.dispatchEvent(new Event('change'));
      await sleep(30);
      const mp4Note = exportNoteEl.textContent;
      if (aviNote === mp4Note) {
        errs.push('412: the export note did not change when the format changed');
      }
      if (!/\\d/.test(mp4Note)) {
        errs.push(`412: the export note carries no size figure for mp4, got ${JSON.stringify(mp4Note)}`);
      }
    }
    for (const fmt of ['gif']) {
      if (!optionValues.includes(fmt)) continue;
      formatSel.value = fmt;
      formatSel.dispatchEvent(new Event('change'));
      await sleep(30);
      const note = exportNoteEl.textContent;
      if (!note.includes('carries no sound')) {
        errs.push(`412: the export note is missing the no-audio hint for "${fmt}", got ${JSON.stringify(note)}`);
      }
      if (!/\\d/.test(note)) {
        errs.push(`412: the export note carries no size figure for "${fmt}", got ${JSON.stringify(note)}`);
      }
    }
    formatSel.value = 'mp4';
    formatSel.dispatchEvent(new Event('change'));
    await sleep(30);

    // ---- 387: the remaining-time estimate stays hidden below the minimum percent (10%
    // by default), regardless of how fast progress is moving ----
    const exportEtaEl = document.getElementById('ve-export-eta');
    const exportStartBtn = document.getElementById('ve-export-start');
    // probe_recording's fixture duration is 10 real minutes (600000ms) and
    // nothing before this point trims or re-speeds the clip, so a payload's
    // percentage is exactly ms/600000 -- no need to derive it indirectly.
    exportStartBtn.click();
    await sleep(10);
    window.__FIRE__('recording-convert-progress', { job: 'export', ms: 18000 }); // 3%
    await sleep(15);
    window.__FIRE__('recording-convert-progress', { job: 'export', ms: 30000 }); // 5%, fast rate but still under 10%
    await sleep(10);
    if (exportEtaEl.textContent.trim() !== '') {
      errs.push(`387: remaining-time text showed before the minimum percent, got ${JSON.stringify(exportEtaEl.textContent)}`);
    }
    await sleep(120); // let this export resolve so a second one can start clean

    // ---- 387: past the minimum percent, a slow enough rate shows an "about ..." text ----
    // startExport() itself feeds an initial 0% sample the instant it starts, so
    // the very first real tick below is measured against that -- a real 3s gap
    // keeps that first rate low enough for the average to clear the minimum seconds.
    // export_video needs to still be pending 3s in, hence the longer delay.
    window.__EXPORT_DELAY_MS__ = 3500;
    exportStartBtn.click();
    await sleep(3000);
    window.__FIRE__('recording-convert-progress', { job: 'export', ms: 60000 }); // 10% after ~3s -> roughly 3.3%/s
    await sleep(10);
    // A conversion's progress (no job) must not move the export bar.
    window.__FIRE__('recording-convert-progress', { ms: 300000 });
    await sleep(10);
    if (Math.round(document.getElementById('ve-export-progress').value) !== 10) {
      errs.push(`F10: a conversion progress event moved the export bar to ${document.getElementById('ve-export-progress').value}`);
    }
    if (!/about/i.test(exportEtaEl.textContent)) {
      errs.push(`387: remaining-time text did not appear past the minimum percent with a slow rate, got ${JSON.stringify(exportEtaEl.textContent)}`);
    }
    await sleep(700);
    window.__EXPORT_DELAY_MS__ = undefined;

    // ---- 399: a real classified ffmpeg failure never shows its raw log text
    // as the headline -- the status line gets the translated key, the raw
    // text only appears, closed by default, under "Technical details" ----
    const errDetails = document.getElementById('ve-export-error-details');
    const errRaw = document.getElementById('ve-export-error-raw');
    const rawNeedle = 'No space left on device';
    destInput.value = 'C:\\\\FAILCLASSIFY\\\\clip.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);
    exportStartBtn.click();
    await sleep(120);
    const statusText = document.getElementById('ve-export-status').textContent;
    if (statusText.includes(rawNeedle) || statusText.trim().startsWith('{')) {
      errs.push(`399: export failure status showed the raw/JSON text verbatim, got ${JSON.stringify(statusText)}`);
    }
    if (!statusText.trim()) {
      errs.push('399: export failure left the status line empty');
    }
    if (errDetails.classList.contains('hidden') || errDetails.open) {
      errs.push('399: "Technical details" must appear closed, not hidden and not pre-opened');
    }
    if (!errRaw.textContent.includes(rawNeedle)) {
      errs.push(`399: "Technical details" did not carry the raw ffmpeg text, got ${JSON.stringify(errRaw.textContent)}`);
    }
    // A later successful export must clear both the status wording concern is
    // moot for and the stale details row -- nothing about a past failure
    // should linger once a new export actually starts.
    destInput.value = 'C:\\\\Custom\\\\my-clip.mp4';
    destInput.dispatchEvent(new Event('change'));
    await sleep(30);
    exportStartBtn.click();
    await sleep(120);
    if (!errDetails.classList.contains('hidden')) {
      errs.push('399: a later successful export left the previous failure details row visible');
    }

    // ---- 336: "Copy to clipboard" runs its own export into a throwaway temp
    // file and lands that file on the clipboard, without touching destInput ----
    window.__CLIP_EXPORT_COPIED__ = undefined;
    // The clipboard file is a temp file that gets swept: it never counts as saving the edits.
    const clipVE = (await import('./video.js')).VE;
    clipVE.dirty = true;
    clipVE.editStateKeepOn = true;
    window.__EDIT_STATE_DISCARDED__ = undefined;
    document.getElementById('ve-export-clip').click();
    await sleep(150);
    if (!clipVE.dirty) errs.push('F-clip: a clipboard export cleared the unsaved-edits mark');
    if (window.__EDIT_STATE_DISCARDED__) errs.push('F-clip: a clipboard export dropped the remembered edit state');
    if (!window.__CLIP_EXPORT_COPIED__) {
      errs.push('"Copy to clipboard" never reached copy_file_to_clipboard');
    } else if (!window.__CLIP_EXPORT_COPIED__.includes('clipexport')) {
      errs.push('"Copy to clipboard" copied something other than its own throwaway temp file');
    }

    document.getElementById('ve-tab-edit').click();
    await sleep(80);

    // ---- a gated control actually reacts when its switch flips ----
    const cropFields = document.getElementById('ve-crop-fields');
    const cropOn = document.getElementById('ve-crop-on');
    const dimmedBefore = cropFields.classList.contains('ve-off');
    cropOn.click();
    await sleep(80);
    if (cropFields.classList.contains('ve-off') === dimmedBefore) {
      errs.push('switching crop on did not change the crop fields enabled state');
    }
    const cropInput = cropFields.querySelector('input');
    if (cropInput && cropInput.disabled !== !cropOn.checked) {
      errs.push('a crop field disabled state does not track the crop switch');
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  errs.push(...(window.__SMOKE_ERRORS__ || []));
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# 385: the remembered-unfinished-edit restore dialog, both answers. A saved
# edit_state fixture (two ops: trim + fade, matching the exact shapes
# loadRecording's own trim/fade handlers push) is planted before boot, so
# checkEditState() finds it on the very first load and opens the dialog --
# same "own boot each time" reasoning as SEAL_PROBE, since the dialog only
# ever opens during the initial loadRecording() pass.
EDIT_STATE_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const action = window.__EDIT_STATE_RESTORE_ACTION__;
  const which = action + ': ';
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    const overlay = document.getElementById('confirm-modal');
    // The dialog only opens once loadRecording's own probe/mux/texts-ready
    // chain has fully settled -- generous relative to the default 5s used
    // elsewhere in this file (still well inside the page's 30s virtual-time
    // budget), since a shorter budget here measurably flaked under load.
    const shown = await until(() => overlay && !overlay.classList.contains('hidden'), 300);
    if (!shown) {
      errs.push(which + 'the restore dialog never opened although a saved edit state was planted');
    } else {
      const title = document.getElementById('confirm-title').textContent.trim();
      const msg = document.getElementById('confirm-msg').textContent.trim();
      if (!title || /^editStateRestore/.test(title)) errs.push(which + 'the dialog title is empty or a raw i18n key: ' + title);
      if (!msg.includes('(2)')) errs.push(which + 'the dialog message does not name the saved op count: ' + msg);
      const okBtn = document.getElementById('confirm-ok');
      const cancelBtn = document.getElementById('confirm-cancel');
      const altBtn = document.getElementById('confirm-alt');
      if (!okBtn.textContent.trim() || !cancelBtn.textContent.trim() || altBtn.classList.contains('hidden') || !altBtn.textContent.trim()) {
        errs.push(which + 'the dialog buttons carry no label');
      }
      // "discard" is the explicit start-fresh button; "later" is a click on the dimmed background,
      // which must keep the saved state (a stray click beside the dialog is not an answer).
      if (action === 'continue') okBtn.click();
      else if (action === 'discard') document.getElementById('confirm-alt').click();
      else overlay.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      await sleep(200);
      if (!overlay.classList.contains('hidden')) errs.push(which + 'the dialog did not close after the answer');
      const opRows = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
      if (action === 'continue') {
        if (opRows !== 2) errs.push(which + 'continuing did not restore the saved ops onto the timeline: ' + opRows + ' rows');
        if (window.__EDIT_STATE_DISCARDED__) errs.push(which + 'continuing must not also discard the saved edit state');
      } else if (action === 'later') {
        if (opRows !== 0) errs.push(which + 'dismissing left restored ops on the timeline: ' + opRows + ' rows');
        if (window.__EDIT_STATE_DISCARDED__) errs.push(which + 'a dismissed restore question discarded the saved edit state');
      } else {
        if (opRows !== 0) errs.push(which + 'starting fresh left restored ops on the timeline: ' + opRows + ' rows');
        const d = window.__EDIT_STATE_DISCARDED__;
        if (!d || d.kind !== 'video' || !d.target) errs.push(which + 'starting fresh did not tell the backend to discard the saved edit state');
      }
    }
  } catch (e) { errs.push(which + 'probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# 873: closing the editor and choosing Discard must not write the discarded ops as the
# recording's remembered edit state (and must drop one an earlier autosave left behind);
# answering Keep editing instead leaves the remembered state exactly as it is.
CLOSE_STATE_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => { for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50); return fn(); };
  const mode = window.__CLOSE_MODE__;
  const which = mode + ': ';
  const overlay = document.getElementById('confirm-modal');
  const shown = () => !overlay.classList.contains('hidden');
  try {
    if (mode === 'kept') {
      // The planted edit state is offered at load: continue with it, which leaves the editor clean.
      await until(shown, 300);
      document.getElementById('confirm-ok').click();
    }
    await until(() => document.getElementById('ve-stage-msg').classList.contains('hidden') &&
      document.querySelectorAll('#ve-filmstrip .ve-thumb').length > 0, 300);
    await sleep(300);
    if (mode === 'discard') {
      const fade = document.getElementById('ve-fade-on');
      fade.checked = true;
      fade.dispatchEvent(new Event('change', { bubbles: true }));
      await sleep(100);
      if (!document.querySelectorAll('#ve-ops-list .ve-op-row').length) errs.push(which + 'fixture: the fade left no operation');
    }
    window.__EDIT_STATE_SAVED__ = undefined;
    window.__EDIT_STATE_DISCARDED__ = undefined;
    document.getElementById('ve-close').click();
    await until(shown, 40);
    if (!shown()) errs.push(which + 'closing with unsaved edits did not ask');
    else document.getElementById(mode === 'discard' ? 'confirm-ok' : 'confirm-cancel').click();
    await sleep(500);
    const saved = window.__EDIT_STATE_SAVED__;
    const dropped = window.__EDIT_STATE_DISCARDED__;
    if (mode === 'discard') {
      if (saved) errs.push(which + 'the discarded edits were saved as the remembered edit state: ' + String(saved.ops).slice(0, 80));
      if (!dropped || dropped.kind !== 'video' || !dropped.target) errs.push(which + 'the remembered edit state was not cleared');
    } else {
      if (saved) errs.push(which + 'Keep editing saved the edit state on the way');
      if (dropped) errs.push(which + 'Keep editing cleared the remembered edit state');
    }
  } catch (e) { errs.push(which + 'probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# 874: a recording with a mic sidecar is muxed BEFORE the video element opens the file (Windows
# refuses to replace a file another handle holds open), so the element has no source until the
# mux and the second probe are done.
MIC_MUX_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => { for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50); return fn(); };
  try {
    const v = document.getElementById('ve-video');
    let early = 0, samples = 0;
    const timer = setInterval(() => {
      samples++;
      if (!window.__MUX_DONE__ && v.getAttribute('src')) early++;
    }, 10);
    await until(() => window.__MUX_DONE__ && v.getAttribute('src'), 200);
    clearInterval(timer);
    if (!window.__MUX_DONE__) errs.push('874: the mic track was never muxed');
    if (!v.getAttribute('src')) errs.push('874: the video element never got its source after the mux');
    if (early) errs.push('874: the video element had a source while the mux was still running (' + early + ' of ' + samples + ' samples)');
    await until(() => document.getElementById('ve-stage-msg').classList.contains('hidden'), 100);
    if (!document.getElementById('ve-stage-msg').classList.contains('hidden')) errs.push('874: the stage never left its loading state');
  } catch (e) { errs.push('874: probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# A 1280x720 source (a small region recording): the fixed-height presets may
# enlarge. 1080p sets a 1920x1080 scale op and shows the "scaled up" hint, the
# export filter scales with lanczos; 720p equals the source height, sets no op,
# selects "keep original" and shows no hint.
SCALE_UP_PROBE = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    const cur = document.getElementById('ve-scale-current');
    const loaded = await until(() => /1280/.test(cur.textContent) && document.querySelectorAll('#ve-scale-presets button').length > 3, 300);
    if (!loaded) errs.push('the 1280x720 source never loaded: ' + cur.textContent);
    else {
      const btn = (k) => [...document.querySelectorAll('#ve-scale-presets button')].find((b) => b.dataset.key === k);
      const hint = document.getElementById('ve-scale-uphint');
      const wField = document.getElementById('ve-scale-w');
      const hField = document.getElementById('ve-scale-h');
      if (!hint) errs.push('there is no scale-up hint element');
      else {
        if (!hint.classList.contains('hidden')) errs.push('the hint shows before any preset is picked');
        btn('1080').click();
        await sleep(60);
        if (wField.value !== '1920' || hField.value !== '1080') errs.push('1080p on a 720 source did not set 1920x1080, got ' + wField.value + 'x' + hField.value);
        if (hint.classList.contains('hidden')) errs.push('no scale-up hint while 1080p is selected on a 720 source');
        if (!btn('1080').classList.contains('sel')) errs.push('the 1080p button is not marked selected');
        if (!hint.textContent.trim() || /^veScaleUpHint/.test(hint.textContent.trim())) errs.push('the hint carries no text: ' + hint.textContent);
        document.getElementById('ve-export-start').click();
        await sleep(300);
        const call = (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'export_video').pop();
        const chain = call && call[1] && call[1].req ? String(call[1].req.filter_chain) : '';
        if (!/scale=1920:1080:flags=lanczos/.test(chain)) errs.push('the export filter does not enlarge with lanczos: ' + chain);
        btn('720').click();
        await sleep(60);
        if (!hint.classList.contains('hidden')) errs.push('the hint shows for the preset equal to the source height');
        if (!btn('').classList.contains('sel')) errs.push('the source-height preset did not select keep original');
        if (document.querySelectorAll('#ve-ops-list .ve-op-row').length !== 0) errs.push('the source-height preset left a scale op behind');
        btn('480').click();
        await sleep(60);
        if (!hint.classList.contains('hidden')) errs.push('the hint shows for a smaller preset');
        if (wField.value !== '854' || hField.value !== '480') errs.push('480p on a 720 source did not set 854x480, got ' + wField.value + 'x' + hField.value);
      }
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


# A second recording arriving (the "video-editor-load" event of a reused window)
# while the restore dialog of the first is still open: the first load must
# settle instead of waiting forever on a dialog nobody can answer any more, and
# the new source must get its own working dialog. The first load finishing is
# seen through the render loop the boot code starts only after it: a
# requestAnimationFrame(tick) call planted before the module runs.
EDIT_STATE_SUPERSEDE_PROBE = """
<script>
window.__TICK_STARTED__ = false;
const realRaf = window.requestAnimationFrame.bind(window);
window.requestAnimationFrame = (fn) => {
  if (fn && fn.name === 'tick') window.__TICK_STARTED__ = true;
  return realRaf(fn);
};
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50);
    return fn();
  };
  try {
    const overlay = document.getElementById('confirm-modal');
    const shown = await until(() => overlay && !overlay.classList.contains('hidden'), 300);
    if (!shown) errs.push('the first restore dialog never opened');
    else {
      window.__FIRE__('video-editor-load', { path: 'C:\\\\Recordings\\\\ps-second-recording.mp4', origin_dir: '' });
      // Long enough for the second load to get through its probe and reopen
      // the dialog for its own saved state.
      await sleep(1500);
      if (!window.__TICK_STARTED__) errs.push('the first load never settled after a second recording superseded its restore dialog');
      if (window.__EDIT_STATE_DISCARDED__) errs.push('a superseded dialog must not discard the saved edit state');
      const reopened = await until(() => !overlay.classList.contains('hidden'), 100);
      if (!reopened) errs.push('the second recording got no restore dialog of its own');
      else {
        document.getElementById('confirm-ok').click();
        await sleep(200);
        const rows = document.querySelectorAll('#ve-ops-list .ve-op-row').length;
        if (rows !== 2) errs.push('answering the second dialog did not restore the saved ops: ' + rows + ' rows');
        if (!overlay.classList.contains('hidden')) errs.push('the second dialog did not close after the answer');
      }
    }
  } catch (e) { errs.push('probe: ' + e.message); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""


def run(browser, tmp):
    failed = 0
    # The video editor keeps its own window config (1100x720 default, 920x600
    # minimum -- both well above the ~500px floor the generic loop above never
    # has to think about) rather than the single fixed size every other
    # secondary window uses, so it gets its own multi-size run instead of a slot
    # in that tuple list: the file is injected once, then run at each
    # --window-size, same as the sealed/unsealed index.html pair below.
    video_side = tmp / "ui" / "video.html"
    video_side.write_text(
        video_side.read_text(encoding="utf-8")
        .replace("<script", '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", VIDEO_PROBE + "</body>"),
        encoding="utf-8")
    # 760x520 is below the window minimum on purpose: it is the one stacked-layout run, standing
    # for a scaled UI where the same CSS width is reached in a bigger window.
    for w, h in ((1100, 720), (920, 600), (760, 520)):
        # Same "headless Chrome eats part of the requested window" offset
        # the main sweep already compensates for (see WRAPPER above) --
        # without it the requested size never actually delivers that innerWidth/
        # innerHeight to the page, and the min-size run silently tests a
        # bigger window than the one this feature ships with.
        proc = run_chrome(browser, tmp / ('profile-video-%dx%d' % (w, h)), f"{w + 30},{h + 140}", 30000, video_side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- video.html {w}x{h}")
        for e in errs:
            print(f"   FAIL JS error: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

    # The same page in German at the minimum size (label length decides the wrapping).
    video_de = tmp / "ui" / "video-de.html"
    video_de.write_text(
        (REPO / "ui" / "video.html").read_text(encoding="utf-8")
        .replace("<script",
                 '<script>window.__SEED_SETTINGS__ = { language: "de" };</script>\n'
                 '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", VIDEO_DE_PROBE + "</body>"),
        encoding="utf-8")
    for w, h in ((920, 600),):
        # 16 and 95 px are what headless Chrome keeps for its own frame, so the page sees exactly w x h.
        proc = run_chrome(browser, tmp / ('profile-video-de-%dx%d' % (w, h)), f"{w + 16},{h + 95}", 30000, video_de.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- video.html German {w}x{h}")
        for e in errs:
            print(f"   FAIL JS error: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

    # 385: the remembered-unfinished-edit restore dialog, both answers --
    # its own boot each time, same reasoning as the seal/write-fail pairs below.
    edit_state_fixture = json.dumps(json.dumps({
        "v": 1,
        "ops": [
            {"tool": "trim", "startMs": 0, "endMs": 2000},
            {"tool": "fade", "inMs": 100, "outMs": 200},
        ],
    }))
    for action in ("continue", "discard", "later"):
        side = tmp / "ui" / f"video-restore-{action}.html"
        side.write_text(
            (REPO / "ui" / "video.html").read_text(encoding="utf-8")
            .replace("<script",
                     f'<script>window.__EDIT_STATE_FIXTURE__ = {edit_state_fixture};'
                     f'window.__EDIT_STATE_RESTORE_ACTION__ = "{action}";</script>\n'
                     '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", EDIT_STATE_PROBE + "</body>"),
            encoding="utf-8")
        proc = run_chrome(browser, tmp / ('profile-video-restore-%s' % action), "1130,860", 30000, side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- video.html restore dialog ({action})")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

    # 873: closing with Discard, and answering Keep editing with restored operations.
    for mode in ("discard", "kept"):
        side = tmp / "ui" / f"video-close-{mode}.html"
        fixture = f'window.__EDIT_STATE_FIXTURE__ = {edit_state_fixture};' if mode == "kept" else ""
        side.write_text(
            (REPO / "ui" / "video.html").read_text(encoding="utf-8")
            .replace("<script",
                     f'<script>{fixture}window.__CLOSE_MODE__ = "{mode}";</script>\n'
                     '<script src="tauri_stub.js"></script>\n<script', 1)
            .replace("</body>", CLOSE_STATE_PROBE + "</body>"),
            encoding="utf-8")
        proc = run_chrome(browser, tmp / ('profile-video-close-%s' % mode), "1130,860", 30000, side.as_uri(), 180)
        m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
        errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
        print(f"-- video.html close ({mode})")
        for e in errs:
            print(f"   FAIL: {e}")
        failed += len(errs)
        if not errs:
            print("   ok")

    # 874: the mux of a mic sidecar finishes before the video element opens the file.
    side = tmp / "ui" / "video-mic-mux.html"
    side.write_text(
        (REPO / "ui" / "video.html").read_text(encoding="utf-8")
        .replace("<script",
                 '<script>window.__MIC_SIDECAR__ = true; window.__MUX_MS__ = 600;</script>\n'
                 '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", MIC_MUX_PROBE + "</body>"),
        encoding="utf-8")
    proc = run_chrome(browser, tmp / 'profile-video-mic-mux', "1130,860", 30000, side.as_uri(), 180)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
    print("-- video.html mic sidecar mux order")
    for e in errs:
        print(f"   FAIL: {e}")
    failed += len(errs)
    if not errs:
        print("   ok")

    # A 1280x720 source: the fixed-height presets may enlarge.
    side = tmp / "ui" / "video-scale-up.html"
    side.write_text(
        (REPO / "ui" / "video.html").read_text(encoding="utf-8")
        .replace("<script",
                 '<script>window.__PROBE_W__ = 1280; window.__PROBE_H__ = 720;</script>\n'
                 '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", SCALE_UP_PROBE + "</body>"),
        encoding="utf-8")
    proc = run_chrome(browser, tmp / 'profile-video-scale-up', "1130,860", 30000, side.as_uri(), 180)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
    print("-- video.html scale presets on a 720p source")
    for e in errs:
        print(f"   FAIL: {e}")
    failed += len(errs)
    if not errs:
        print("   ok")

    # A second recording arriving while the first one's restore dialog is open.
    side = tmp / "ui" / "video-restore-superseded.html"
    side.write_text(
        (REPO / "ui" / "video.html").read_text(encoding="utf-8")
        .replace("<script",
                 f'<script>window.__EDIT_STATE_FIXTURE__ = {edit_state_fixture};</script>\n'
                 '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", EDIT_STATE_SUPERSEDE_PROBE + "</body>"),
        encoding="utf-8")
    proc = run_chrome(browser, tmp / 'profile-video-restore-superseded', "1130,860", 30000, side.as_uri(), 180)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
    print("-- video.html restore dialog (superseded by a second recording)")
    for e in errs:
        print(f"   FAIL: {e}")
    failed += len(errs)
    if not errs:
        print("   ok")
    return failed


def run_expert(browser, tmp):
    failed = 0
    # Video editor values the editor remembers (last export choice, last overlay values)
    # next to the fixed defaults that no setting changes any more.
    # The stub is seeded before video.html loads, so the boot-time settings
    # read is what gets exercised (the backend never sends a live settings
    # event to this window). Its own boot, same reasoning as the runs above.
    video_expert_probe = """
<script>
(async () => {
  const errs = [];
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const until = async (fn, tries) => {
    for (let i = 0; i < (tries || 100) && !fn(); i++) await sleep(50);
    return fn();
  };
  const fieldOf = (label) => {
    const row = [...document.querySelectorAll('#ve-ov-editor label.field')]
      .find((l) => l.firstElementChild && l.firstElementChild.textContent === label);
    return row ? row.querySelector('input,select') : null;
  };
  const exportCalls = () => (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'export_video');
  const overlayRows = () => document.querySelectorAll('#ve-ov-list .ve-ov-row').length;
  try {
    await until(() => document.getElementById('ve-stage-msg').classList.contains('hidden') &&
      document.querySelectorAll('#ve-filmstrip .ve-thumb').length > 0, 300);
    await until(() => document.getElementById('ve-scale-w').value === '1280', 100);

    // The editor opens on the Edit tab; format, quality and scale preset open on the remembered export choice.
    if (!document.getElementById('ve-tab-edit').classList.contains('active')) {
      errs.push('537: the editor did not open on the Edit tab');
    }
    if (document.getElementById('ve-export-format').value !== 'webm') {
      errs.push('537: the remembered export format webm was ignored, got ' + document.getElementById('ve-export-format').value);
    }
    if (document.getElementById('ve-export-quality').value !== 'small') {
      errs.push('537: the remembered export quality small was ignored, got ' + document.getElementById('ve-export-quality').value);
    }
    const sw = document.getElementById('ve-scale-w').value, sh = document.getElementById('ve-scale-h').value;
    if (sw !== '1280' || sh !== '720') {
      errs.push('537: the remembered export scale 720 was ignored, got ' + sw + 'x' + sh);
    }
    // The initial fade fields hold the fixed 500 ms.
    if (document.getElementById('ve-fade-in').value !== '500' || document.getElementById('ve-fade-out').value !== '500') {
      errs.push('537: the fade fields did not start at 500 ms, got ' + document.getElementById('ve-fade-in').value + '/' + document.getElementById('ve-fade-out').value);
    }

    // Export state: bitrate of the small tier scaled to 720p of a 1080p source
    // (4000 * 720 / 1080), then the high tier (12000 * 720 / 1080), with the fade.
    document.getElementById('ve-fade-on').click();
    await sleep(80);
    document.getElementById('ve-export-start').click();
    await until(() => exportCalls().length > 0, 100);
    let last = exportCalls().pop();
    let req = last && last[1] && last[1].req;
    if (!req) errs.push('537: no export request was made');
    else {
      if (req.format !== 'webm') errs.push('537: the export format was ' + req.format + ', expected the remembered webm');
      if (req.video_kbps !== 2667) errs.push('537: the small-tier bitrate was ' + req.video_kbps + ', expected 4000 scaled to 2667');
      const chain = String(req.filter_chain);
      if (!/fade=t=in:st=0:d=0[.]500/.test(chain)) errs.push('537: the export fade-in did not use the fixed 500 ms: ' + chain);
    }
    await until(() => !document.getElementById('ve-export-start').disabled, 100);
    const before = exportCalls().length;
    const q = document.getElementById('ve-export-quality');
    q.value = 'high';
    q.dispatchEvent(new Event('change', { bubbles: true }));
    await sleep(80);
    document.getElementById('ve-export-start').click();
    await until(() => exportCalls().length > before, 100);
    last = exportCalls().pop();
    req = last && last[1] && last[1].req;
    if (!req || req.video_kbps !== 8000) {
      errs.push('537: the high-tier bitrate was ' + (req && req.video_kbps) + ', expected 12000 scaled to 8000');
    }

    // 742: "Add to a tile after saving" opens on its remembered choice, files the finished export
    // as a tile, tells a full grid, and stays quiet once switched off.
    const tileBox = document.getElementById('ve-export-tile');
    const addTileCalls = () => (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'video_editor_add_tile');
    const toastText = () => document.getElementById('ve-toast').textContent;
    const exportOnce = async () => {
      const n = exportCalls().length;
      document.getElementById('ve-export-start').click();
      await until(() => exportCalls().length > n, 100);
      await until(() => !document.getElementById('ve-export-start').disabled, 100);
      await sleep(120);
    };
    if (!tileBox || !tileBox.checked) errs.push('742: the remembered tile checkbox was not on');
    const tilesBefore = addTileCalls().length;
    await exportOnce();
    const added = addTileCalls().slice(tilesBefore);
    if (added.length !== 1 || !added[0][1] || !/[.]webm$/.test(String(added[0][1].path))) {
      errs.push('742: the export did not file one tile for the exported file: ' + JSON.stringify(added));
    }
    if (toastText() !== 'Added to a tile.') errs.push('742: the tile toast was ' + toastText());
    window.__TILE_FULL__ = true;
    await exportOnce();
    if (toastText() !== 'Grid is full. Drag onto a tile to swap') errs.push('742: a full grid was not told, toast: ' + toastText());
    window.__TILE_FULL__ = false;
    tileBox.checked = false;
    tileBox.dispatchEvent(new Event('change', { bubbles: true }));
    await sleep(80);
    const saved = (window.__SMOKE_ARGS__ || []).filter((c) => c[0] === 'set_ui_value' && c[1].key === 'veExportTile').pop();
    if (!saved || saved[1].value !== 0) errs.push('742: switching the tile box off was not remembered: ' + JSON.stringify(saved));
    const callsBefore = addTileCalls().length;
    await exportOnce();
    if (addTileCalls().length !== callsBefore) errs.push('742: a tile was filed with the box off');

    // 743: the quality options read Small, High, Lossless with High still the default.
    {
      const q = document.getElementById('ve-export-quality');
      const order = [...q.options].map((o) => o.value).join(',');
      if (order !== 'small,high,lossless') errs.push('743: the quality order is ' + order);
    }
    // 744: a cancelled export hides the progress row and keeps the message.
    {
      const destField = document.getElementById('ve-export-dest');
      destField.value = 'C:/Custom/CANCELLED-clip.mp4';
      destField.dispatchEvent(new Event('change', { bubbles: true }));
      await sleep(40);
      const n = exportCalls().length;
      document.getElementById('ve-export-start').click();
      await until(() => exportCalls().length > n, 100);
      await until(() => !document.getElementById('ve-export-start').disabled, 100);
      await sleep(80);
      if (!document.getElementById('ve-export-progress-row').classList.contains('hidden')) errs.push('744: the progress row stayed visible after a cancel');
      if (document.getElementById('ve-export-status').textContent !== 'Export cancelled.') errs.push('744: the cancel message was ' + document.getElementById('ve-export-status').textContent);
    }
    // 745: a typed ".mp4" on an mp4 export is not doubled, in any letter case.
    {
      const f = document.getElementById('ve-export-format');
      f.value = 'mp4';
      f.dispatchEvent(new Event('change', { bubbles: true }));
      const nm = document.getElementById('ve-export-name');
      nm.value = 'clip.MP4';
      nm.dispatchEvent(new Event('input', { bubbles: true }));
      nm.dispatchEvent(new Event('change', { bubbles: true }));
      await sleep(40);
      if (nm.value !== 'clip') errs.push('745: the name field kept the extension: ' + nm.value);
      const n = exportCalls().length;
      document.getElementById('ve-export-start').click();
      await until(() => exportCalls().length > n, 100);
      const req2 = exportCalls().pop()[1].req;
      if (!/[\\/]clip[.]mp4$/.test(String(req2.dest_path))) errs.push('745: the export path was ' + req2.dest_path);
      await until(() => !document.getElementById('ve-export-start').disabled, 100);
    }

    // A new overlay of each kind starts from the remembered values, or the fixed defaults.
    document.getElementById('ve-tab-overlays').click();
    await sleep(60);
    document.getElementById('ve-ov-add-pixelate').click();
    await until(() => overlayRows() === 1, 60);
    const block = fieldOf('Block size (px)');
    if (!block || block.value !== '24') errs.push('537: a new pixelate overlay did not use the remembered block size 24, got ' + (block && block.value));
    const rectX = fieldOf('X');
    const x0 = rectX ? Number(rectX.value) : NaN;
    // Duplicate through the row context menu: shifted by the fixed offset.
    document.querySelector('#ve-ov-list .ve-ov-row').dispatchEvent(
      new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 200, clientY: 200 }));
    await sleep(60);
    const dupBtn = [...document.querySelectorAll('body > .ctx[role=menu] button')].find((b) => !b.classList.contains('danger'));
    if (!dupBtn) errs.push('537: the overlay context menu had no duplicate button');
    else {
      // The menu is still open while the overlay is edited and nudged: the copy follows the
      // overlay as it is when Duplicate is pressed, and the menu swallows the arrow keys.
      rectX.value = String(x0 + 30);
      rectX.dispatchEvent(new Event('change', { bubbles: true }));
      await sleep(40);
      const x1 = Number(fieldOf('X').value);
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', code: 'ArrowRight', bubbles: true, cancelable: true }));
      await sleep(40);
      if (Number(fieldOf('X').value) !== x1) errs.push('F-ctx: an arrow key moved the overlay while its menu was open');
      dupBtn.click();
      await until(() => overlayRows() === 2, 60);
      const dx = fieldOf('X') ? Number(fieldOf('X').value) - x0 : NaN;
      if (dx !== 46) errs.push('537: the duplicated overlay moved by ' + dx + ' px, expected 46 (edited 30 + offset 16)');
    }
    document.getElementById('ve-ov-add-spotlight').click();
    await until(() => overlayRows() === 3, 60);
    const dim = fieldOf('Darkening (%)');
    if (!dim || dim.value !== '70') errs.push('537: a new spotlight did not use the remembered darkening 70, got ' + (dim && dim.value));
    document.getElementById('ve-ov-add-text').click();
    await until(() => overlayRows() === 4, 60);
    const font = fieldOf('Font');
    if (!font || font.value !== 'segoeui') errs.push('537: a new text overlay did not start with the fixed font segoeui, got ' + (font && font.value));
    const sel = document.querySelector('#ve-ov-editor .swatch.sel');
    if (!sel || String(sel.title).toLowerCase() !== '#ef4444') errs.push('537: a new text overlay did not use the remembered colour #ef4444, got ' + (sel && sel.title));
    document.getElementById('ve-ov-add-image').click();
    await until(() => overlayRows() === 5, 60);
    const opac = fieldOf('Opacity (%)');
    if (!opac || opac.value !== '40') errs.push('537: a new image overlay did not use the remembered opacity 40, got ' + (opac && opac.value));
  } catch (e) { errs.push('537: probe threw: ' + e); }
  const pre = document.createElement('pre');
  pre.id = 'SMOKE';
  pre.textContent = JSON.stringify(errs);
  document.body.appendChild(pre);
})();
</script>
"""
    seed = (
        '<script>window.__SEED_UI_VALUES__ = {"veExportTile": 1, "veOvLastPixelateBlock": 24, "veOvLastSpotlightDim": 70, '
        '"veOvLastImageOpacity": 40};'
        'window.__SEED_UI_TEXTS__ = {"videoExportFormat": "webm", "videoExportQuality": "small", '
        '"videoExportScale": "720", "veOvLastTextColor": "#ef4444"};</script>\n')
    side = tmp / "ui" / "video-expert-seeded.html"
    side.write_text(
        (REPO / "ui" / "video.html").read_text(encoding="utf-8")
        .replace("<script", seed + '<script src="tauri_stub.js"></script>\n<script', 1)
        .replace("</body>", video_expert_probe + "</body>"),
        encoding="utf-8")
    proc = run_chrome(browser, tmp / 'profile-video-expert-seeded', "1130,860", 30000, side.as_uri(), 180)
    m = re.search(r'<pre id="SMOKE">(\[.*?\])</pre>', proc.stdout, re.S)
    errs = json.loads(unescape(m.group(1))) if m else ["no probe result"]
    print("-- video.html remembered values and fixed defaults")
    for e in errs:
        print(f"   FAIL: {e}")
    failed += len(errs)
    if not errs:
        print("   ok")
    return failed
