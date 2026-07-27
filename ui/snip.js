// Snipping overlay (one window spanning the whole virtual desktop). The frozen
// capture stays fully sharp; four dim rectangles frame the selection. Drag a
// region across any monitors or click a window.
//
// Coordinates are mapped by RATIO against the displayed frozen image, never via
// the window scale factor — so the mapping is pixel-exact on any DPI mix.
const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const bg = document.getElementById("snip-bg");
const mask = {
  top: document.getElementById("snip-mask-top"),
  bottom: document.getElementById("snip-mask-bottom"),
  left: document.getElementById("snip-mask-left"),
  right: document.getElementById("snip-mask-right"),
};
const box = document.getElementById("snip-box");
const sizeLabel = document.getElementById("snip-size");
const hint = document.getElementById("snip-hint");
const loupe = document.getElementById("pick-loupe");
const grid = document.getElementById("pick-grid");
const dot = document.getElementById("pick-dot");
const swatch = document.getElementById("pick-swatch");
const readMain = document.getElementById("pick-main");
const readAlt = document.getElementById("pick-alt");
const readZoom = document.getElementById("pick-zoom");

const MIN_REGION = 5; // CSS px; below this a drag counts as a click
const WIN_RADIUS = 8; // rounded corners when highlighting a window
const IDX = 0; // single overlay
let start = null;
let busy = false;
let windows = []; // stitched-image physical px, topmost first
let FULL_W = 1, FULL_H = 1; // full stitched dimensions (display image may be smaller)

// ---- F26 colour picker state ----
// Same overlay, second mode: no selection, a loupe on the cursor instead.
const LOUPE_CELLS = 15; // odd, so one cell is exactly the picked pixel
const ZOOM_MIN = 4, ZOOM_MAX = 16; // same range as the loupeZoom expert slider
// The frozen desktop is sampled through ONE cached patch around the cursor, not as a
// whole. Handing the overlay the entire desktop meant a ~58 MB transfer on a
// multi-monitor setup before the picker could show anything; a patch is ~110 KB and
// arrives in a millisecond. It has to come from the frozen capture and not from the
// displayed background: that background is a JPEG, whose chroma is subsampled, so
// reading it back would not match a reference tool.
const TILE = 192;       // patch edge in physical px (backend caps this at 512)
const TILE_MARGIN = 48; // refetch once the cursor comes this close to the patch edge
let pickMode = false;
let pickZoom = 8;      // magnification (expert: loupeZoom)
let pickFmt = "hex";   // output notation (expert: pickerFormat)
let pickBusy = false;
let tile = null;       // { x, y, w, h, px } in stitched-image physical pixels
let tileBusy = false;
let loupeLive = false; // a real pixel has been painted at least once this session
let cur = null; // picked point, in stitched-image physical pixels

// Seed hint/error text synchronously from the browser locale (en/de are the
// defaults) so a German user never sees an English flash; the async i18n load
// below refines it for the user's actually-selected language.
const SEED = {
  en: {
    hint: "Drag a region · click a window · Esc to cancel",
    failed: "This window can't be captured. Try a region or another window.",
    pick: "Click to copy the color · scroll or +/- to zoom · arrow keys move one pixel · Esc to cancel",
  },
  de: {
    hint: "Bereich ziehen · Fenster anklicken · Esc zum Abbrechen",
    failed: "Dieses Fenster kann nicht aufgenommen werden. Versuche einen Bereich oder ein anderes Fenster.",
    pick: "Klicken kopiert die Farbe · Scrollen oder +/- zoomt · Pfeiltasten bewegen ein Pixel · Esc zum Abbrechen",
  },
};
const T = { ...((navigator.language || "en").toLowerCase().startsWith("de") ? SEED.de : SEED.en) };
hint.textContent = T.hint;

// The overlay WINDOW is reused across snips so its WebView2 stays warm (creating
// a fresh one was the slow part). beginSession() runs once on first load and
// again on every "snip-begin" the backend fires for a reused window.
let i18nLoaded = false;

function resetSession() {
  start = null;
  busy = false;
  windows = [];
  sizeLabel.textContent = "";
  hint.classList.remove("hidden", "error");
  hint.textContent = T.hint;
  pickMode = false;
  pickBusy = false;
  tile = null;
  tileBusy = false;
  loupeLive = false;
  cur = null;
  pendingMove = null;
  document.body.classList.remove("pick");
  loupe.classList.add("hidden");
  dot.classList.add("hidden");
  fullDim();
}

async function beginSession() {
  resetSession();
  const b = await invoke("snip_background", { index: IDX });
  if (!b) { invoke("snip_cancel"); return; }
  FULL_W = b.width || 1;
  FULL_H = b.height || 1;
  // Colour-pick mode: no selection chrome at all, and the frozen desktop is sampled
  // through a cached patch around the cursor (see `tile`).
  pickMode = !!b.pick;
  if (pickMode) {
    pickZoom = clampZoom(Number(b.pick_zoom) || 8);
    readZoom.textContent = `${pickZoom}×`;
    pickFmt = b.pick_format || "hex";
    document.body.classList.add("pick");
    hint.textContent = T.pick;
    noDim();
    // Start on the middle of the desktop so the first patch is already on its way
    // before the user moves; the very next pointer move corrects it.
    setCur(Math.floor(FULL_W / 2), Math.floor(FULL_H / 2));
  }
  // Reveal the overlay only once the frozen image has decoded, so it never
  // appears blank first. onerror presents too, so a rare decode failure can't
  // leave it stuck hidden; a timer is the final safety net in case a hidden
  // webview ever throttles the load event. present() is idempotent.
  let presented = false;
  let timer = 0;
  const present = () => {
    if (presented) return;
    presented = true;
    if (timer) clearTimeout(timer);
    invoke("snip_present").catch(() => {});
  };
  bg.onload = present;
  bg.onerror = present;
  bg.src = b.is_file ? convertFileSrc(b.src) : b.src;
  timer = setTimeout(present, 500);
  // Hover highlight loads in the background — it must not gate the overlay. The
  // picker has no window selection, so it never pays for the enumeration.
  if (!pickMode) {
    invoke("snip_windows", { index: IDX }).then((w) => { windows = w || []; }).catch(() => { windows = []; });
  }
  // Localized hint/error text: load once, then reuse for every later session.
  if (i18nLoaded) return;
  i18nLoaded = true;
  Promise.all([invoke("get_settings"), import("./i18n.js")])
    .then(([s, { I18N, LANGS }]) => {
      const pref = (s.language && s.language !== "auto"
        ? s.language
        : navigator.language || "en"
      ).toLowerCase();
      const lang = LANGS.find((c) => pref.startsWith(c)) || "en";
      const d = I18N[lang] || I18N.en;
      T.hint = d.snipHint ?? I18N.en.snipHint;
      T.failed = d.snipFailed ?? I18N.en.snipFailed ?? T.failed;
      T.pick = d.pickHint ?? I18N.en.pickHint ?? T.pick;
      if (!hint.classList.contains("error")) hint.textContent = pickMode ? T.pick : T.hint;
    })
    .catch(() => {});
}

// Begin now only if a snip is already in flight (cold-build path); otherwise stay
// idle — the window may be a startup pre-warm. Reused snips arrive via "snip-begin".
invoke("snip_should_begin").then((go) => { if (go) beginSession().catch(() => {}); }).catch(() => {});
listen("snip-begin", () => beginSession().catch(() => {}));

// ---- Ratio mapping between CSS (cursor) and stitched-image pixels ----
// The frozen image fills the overlay, which physically covers the desktop, so a
// fraction of the displayed image equals the same fraction of its pixels.
function imgScale() {
  const r = bg.getBoundingClientRect();
  // Map against the FULL stitched dimensions (the displayed JPEG may be
  // downscaled for speed), so crops are always full-resolution.
  return { r, sx: r.width / FULL_W, sy: r.height / FULL_H };
}
// CSS point -> stitched-image pixel.
function toImg(cssX, cssY) {
  const { r, sx, sy } = imgScale();
  return { x: (cssX - r.left) / sx, y: (cssY - r.top) / sy };
}
// Stitched-image rect -> CSS rect (for the highlight box).
function toCss(ix, iy, iw, ih) {
  const { r, sx, sy } = imgScale();
  return { x: r.left + ix * sx, y: r.top + iy * sy, w: iw * sx, h: ih * sy };
}

function showError() {
  hint.textContent = T.failed;
  hint.classList.add("error");
  hint.classList.remove("hidden");
  // Dimming while picking would stop the screen matching the colour being read.
  if (pickMode) noDim();
  else fullDim();
}

// Keep the selection [x,y,w,h] (CSS px) sharp by framing it with four dim
// rectangles; draw the border box (rounded by radius for window hover).
function place(el, left, top, width, height) {
  el.style.left = `${left}px`;
  el.style.top = `${top}px`;
  el.style.width = `${Math.max(0, width)}px`;
  el.style.height = `${Math.max(0, height)}px`;
}
function setRegion(x, y, w, h, radius) {
  const sw = window.innerWidth, sh = window.innerHeight;
  place(mask.top, 0, 0, sw, y);
  place(mask.bottom, 0, y + h, sw, sh - (y + h));
  place(mask.left, 0, y, x, h);
  place(mask.right, x + w, y, sw - (x + w), h);
  box.style.left = `${x}px`;
  box.style.top = `${y}px`;
  box.style.width = `${w}px`;
  box.style.height = `${h}px`;
  box.style.borderRadius = `${radius}px`;
  box.classList.remove("hidden");
}

// No selection: dim everything (top mask covers the whole screen).
function fullDim() {
  const sw = window.innerWidth, sh = window.innerHeight;
  place(mask.top, 0, 0, sw, sh);
  place(mask.bottom, 0, 0, 0, 0);
  place(mask.left, 0, 0, 0, 0);
  place(mask.right, 0, 0, 0, 0);
  box.classList.add("hidden");
}
// Picking: no dim at all, so the desktop on screen matches the colour being read.
function noDim() {
  for (const m of Object.values(mask)) place(m, 0, 0, 0, 0);
  box.classList.add("hidden");
}
fullDim();

// ---- F26 colour picker ----

// Raw command results arrive as an ArrayBuffer; accept the other shapes too rather
// than leaving the picker mysteriously dead if the transport ever changes.
function toBytes(buf) {
  if (!buf) return null;
  if (buf instanceof ArrayBuffer) return new Uint8Array(buf);
  if (ArrayBuffer.isView(buf)) return new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
  if (Array.isArray(buf)) return Uint8Array.from(buf);
  return null;
}

// The patch that should be cached for a cursor at (cx, cy): TILE square, centred,
// pushed fully inside the frozen desktop.
function tileRectFor(cx, cy) {
  const w = Math.min(TILE, FULL_W);
  const h = Math.min(TILE, FULL_H);
  return {
    x: Math.min(FULL_W - w, Math.max(0, cx - (w >> 1))),
    y: Math.min(FULL_H - h, Math.max(0, cy - (h >> 1))),
    w,
    h,
  };
}

// True once the cursor is close enough to an edge of the cached patch that the loupe
// could run past it. An edge that IS the desktop edge never counts — there is nothing
// further to fetch there, so a cursor parked in a corner must not refetch forever.
function tileStale() {
  if (!tile) return true;
  const near = (v, lo, span, full) =>
    (v - lo < TILE_MARGIN && lo > 0) || (lo + span - v <= TILE_MARGIN && lo + span < full);
  return near(cur.x, tile.x, tile.w, FULL_W) || near(cur.y, tile.y, tile.h, FULL_H);
}

// Keep the cached patch under the cursor. Fire-and-forget: the loupe repaints when
// the patch lands, so a fetch in flight never blocks a pointer move.
async function ensureTile() {
  if (tileBusy || !pickMode || !cur || !tileStale()) return;
  const r = tileRectFor(cur.x, cur.y);
  if (tile && tile.x === r.x && tile.y === r.y && tile.w === r.w && tile.h === r.h) return;
  tileBusy = true;
  let buf;
  try {
    buf = await invoke("pick_tile", { x: r.x, y: r.y, width: r.w, height: r.h });
  } catch {
    buf = null;
  }
  tileBusy = false;
  const bytes = toBytes(buf);
  if (!bytes || bytes.length < r.w * r.h * 3) return;
  tile = { ...r, px: bytes };
  drawLoupe();
}

// One pixel of the frozen desktop, in stitched physical px. Null when it is outside
// the desktop OR simply not in the cached patch yet — the picker must never report a
// neighbour, a clamped edge or a stale value as if it were the real pixel.
function pixelAt(x, y) {
  if (!tile || x < tile.x || y < tile.y || x >= tile.x + tile.w || y >= tile.y + tile.h) return null;
  const i = ((y - tile.y) * tile.w + (x - tile.x)) * 3;
  return [tile.px[i], tile.px[i + 1], tile.px[i + 2]];
}

const hex2 = (n) => n.toString(16).toUpperCase().padStart(2, "0");
const rgbToHex = (r, g, b) => `#${hex2(r)}${hex2(g)}${hex2(b)}`;

// Mirror of the backend's rgb_to_hsl (rounded ints; grey has no hue, red is 0°).
// The readout has to be identical to what the click will copy, and it cannot ask
// the backend per pointer move — so the same rounding lives on both sides.
function rgbToHsl(r, g, b) {
  const rf = r / 255, gf = g / 255, bf = b / 255;
  const max = Math.max(rf, gf, bf), min = Math.min(rf, gf, bf);
  const l = (max + min) / 2, d = max - min;
  if (d === 0) return [0, 0, Math.round(l * 100)];
  const s = d / (1 - Math.abs(2 * l - 1));
  let h;
  if (max === rf) h = ((gf - bf) / d) % 6;
  else if (max === gf) h = (bf - rf) / d + 2;
  else h = (rf - gf) / d + 4;
  return [((Math.round(h * 60) % 360) + 360) % 360, Math.round(s * 100), Math.round(l * 100)];
}

function formatColor(fmt, r, g, b) {
  if (fmt === "rgb") return `rgb(${r},${g},${b})`;
  if (fmt === "hsl") { const [h, s, l] = rgbToHsl(r, g, b); return `hsl(${h},${s}%,${l}%)`; }
  return rgbToHex(r, g, b);
}

// Move the picked point, clamped to the desktop (arrow keys must stop at the edge).
function clampCur(x, y) {
  cur = {
    x: Math.min(FULL_W - 1, Math.max(0, x)),
    y: Math.min(FULL_H - 1, Math.max(0, y)),
  };
}
function setCur(x, y) {
  clampCur(x, y);
  queueLoupe();
}

// At most one redraw per frame — pointer moves and held arrow keys both fire far
// faster than the screen refreshes. The timer is a backstop: this overlay is shown
// from a parked webview whose frame production can still be paused for a moment,
// and a loupe that never paints would leave the picker looking dead. Whichever
// fires first cancels the other, so rAF stays the normal path.
let loupeRaf = 0;
let loupeTimer = 0;
let pendingMove = null; // newest pointer position, applied when the redraw runs
function runLoupe() {
  if (loupeRaf) { cancelAnimationFrame(loupeRaf); loupeRaf = 0; }
  if (loupeTimer) { clearTimeout(loupeTimer); loupeTimer = 0; }
  if (pendingMove) {
    const p = toImg(pendingMove.x, pendingMove.y);
    pendingMove = null;
    clampCur(Math.floor(p.x), Math.floor(p.y));
  }
  drawLoupe();
}
function queueLoupe() {
  if (loupeRaf || loupeTimer) return;
  loupeRaf = requestAnimationFrame(runLoupe);
  loupeTimer = setTimeout(runLoupe, 100);
}

function drawLoupe() {
  if (!pickMode || !cur) return;
  ensureTile();
  // Nothing to read yet (first patch still in flight, or the cursor outran it): keep
  // the last painting rather than inventing a value, and repaint when the patch lands.
  const centre = pixelAt(cur.x, cur.y);
  if (!centre) {
    if (loupeLive) placeLoupe();
    return;
  }
  const z = pickZoom;
  const half = (LOUPE_CELLS - 1) / 2;
  const size = LOUPE_CELLS * z;
  if (grid.width !== size) { grid.width = size; grid.height = size; }
  const ctx = grid.getContext("2d");
  ctx.fillStyle = "#000"; // off-desktop cells stay black
  ctx.fillRect(0, 0, size, size);
  for (let gy = 0; gy < LOUPE_CELLS; gy++) {
    for (let gx = 0; gx < LOUPE_CELLS; gx++) {
      const c = pixelAt(cur.x + gx - half, cur.y + gy - half);
      if (!c) continue;
      ctx.fillStyle = `rgb(${c[0]},${c[1]},${c[2]})`;
      ctx.fillRect(gx * z, gy * z, z, z);
    }
  }
  // Cell separators, only once a cell is big enough for them to read as a grid.
  if (z >= 6) {
    ctx.lineWidth = 1;
    ctx.strokeStyle = "rgba(0, 0, 0, 0.16)";
    ctx.beginPath();
    for (let i = 1; i < LOUPE_CELLS; i++) {
      ctx.moveTo(i * z + 0.5, 0);
      ctx.lineTo(i * z + 0.5, size);
      ctx.moveTo(0, i * z + 0.5);
      ctx.lineTo(size, i * z + 0.5);
    }
    ctx.stroke();
  }
  // The picked pixel: a black ring around a white one, so the outline is visible
  // whatever colour sits under it.
  const o = half * z;
  ctx.lineWidth = 1;
  ctx.strokeStyle = "#000";
  ctx.strokeRect(o - 0.5, o - 0.5, z + 1, z + 1);
  ctx.strokeStyle = "#fff";
  ctx.strokeRect(o + 0.5, o + 0.5, z - 1, z - 1);

  const c = centre;
  const hex = rgbToHex(c[0], c[1], c[2]);
  swatch.style.background = hex;
  readMain.textContent = formatColor(pickFmt, c[0], c[1], c[2]);
  // Second line: hex is the one notation everyone recognises, so show it whenever
  // the chosen format is something else — and rgb() when hex is the format.
  readAlt.textContent = pickFmt === "hex" ? `rgb(${c[0]},${c[1]},${c[2]})` : hex;
  loupeLive = true;
  placeLoupe();
}

// ---- Zoom ----
// The magnification is an expert setting, but nobody looks there mid-pick — so the
// overlay owns it too: wheel, or + / - for anyone without one. The last value is
// written back to loupeZoom (debounced), so the next pick starts where this one ended
// and the expert slider stays the single source of truth.
const clampZoom = (z) => Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(z)));
let zoomSave = 0;
function saveZoom() {
  if (!zoomSave) return;
  clearTimeout(zoomSave);
  zoomSave = 0;
  invoke("set_ui_value", { key: "loupeZoom", value: pickZoom }).catch(() => {});
}
function setZoom(z) {
  const n = clampZoom(z);
  if (!pickMode || n === pickZoom) return;
  pickZoom = n;
  readZoom.textContent = `${n}×`;
  queueLoupe();
  if (zoomSave) clearTimeout(zoomSave);
  zoomSave = setTimeout(saveZoom, 400);
}
window.addEventListener(
  "wheel",
  (e) => {
    if (!pickMode || pickBusy) return;
    e.preventDefault();
    setZoom(pickZoom + (e.deltaY < 0 ? 1 : -1));
  },
  { passive: false }
);

// Park the loupe next to the picked point, flipping to the other side rather than
// running off the desktop.
function placeLoupe() {
  const p = toCss(cur.x, cur.y, 1, 1);
  loupe.classList.remove("hidden");
  dot.classList.remove("hidden");
  dot.style.left = `${p.x}px`;
  dot.style.top = `${p.y}px`;
  const GAP = 20;
  const w = loupe.offsetWidth, h = loupe.offsetHeight;
  let lx = p.x + GAP;
  let ly = p.y + GAP;
  if (lx + w > window.innerWidth - 6) lx = p.x - GAP - w;
  if (ly + h > window.innerHeight - 6) ly = p.y - GAP - h;
  loupe.style.left = `${Math.max(6, lx)}px`;
  loupe.style.top = `${Math.max(6, ly)}px`;
}

// Copy the picked colour. The backend re-samples the frozen image at this exact
// pixel and owns the clipboard write — nothing is stored as a prompt or in the
// copy history.
async function pickHere() {
  if (pickBusy || !cur) return;
  pickBusy = true;
  saveZoom(); // the overlay is about to close; a pending debounce would be lost
  try {
    await invoke("pick_color", { x: cur.x, y: cur.y });
  } catch {
    pickBusy = false;
    showError();
  }
}

// Topmost window under a CSS point; returns its id, CSS rect and image rect.
function windowAt(cssX, cssY) {
  const p = toImg(cssX, cssY);
  for (const w of windows) {
    if (p.x >= w.x && p.x < w.x + w.width && p.y >= w.y && p.y < w.y + w.height) {
      return { id: w.id, css: toCss(w.x, w.y, w.width, w.height), img: w };
    }
  }
  return null;
}

// Process at most once per frame — mousemove fires far faster than the screen
// refreshes, so coalescing keeps the overlay smooth on large desktops.
let lastMove = null, moveRaf = 0;
function onMove() {
  moveRaf = 0;
  const e = lastMove;
  if (!e || busy) return;
  if (start) {
    const x = Math.min(start.x, e.clientX);
    const y = Math.min(start.y, e.clientY);
    const w = Math.abs(e.clientX - start.x);
    const h = Math.abs(e.clientY - start.y);
    setRegion(x, y, w, h, 0);
    const a = toImg(x, y), b = toImg(x + w, y + h);
    sizeLabel.textContent = `${Math.round(b.x - a.x)} × ${Math.round(b.y - a.y)}`;
    return;
  }
  const win = windowAt(e.clientX, e.clientY);
  if (win) { sizeLabel.textContent = ""; setRegion(win.css.x, win.css.y, win.css.w, win.css.h, WIN_RADIUS); }
  else fullDim();
}
window.addEventListener("mousemove", (e) => {
  // Picking runs on the loupe's own scheduler, which redraws the readout too.
  if (pickMode) { pendingMove = { x: e.clientX, y: e.clientY }; queueLoupe(); return; }
  lastMove = e;
  if (!moveRaf) moveRaf = requestAnimationFrame(onMove);
});

window.addEventListener("mousedown", (e) => {
  if (e.button !== 0 || busy) return;
  if (pickMode) return; // picking has no drag; the release does the work
  hint.textContent = T.hint;
  hint.classList.remove("error");
  start = { x: e.clientX, y: e.clientY };
  hint.classList.add("hidden");
  sizeLabel.textContent = "";
  setRegion(e.clientX, e.clientY, 0, 0, 0);
});

window.addEventListener("mouseup", async (e) => {
  if (pickMode) {
    if (e.button !== 0) return;
    // `cur` already tracks the pointer; only seed it if the mouse never moved. A
    // click must not undo an arrow-key nudge made from the same spot.
    if (!cur) { const p = toImg(e.clientX, e.clientY); setCur(Math.floor(p.x), Math.floor(p.y)); }
    pickHere();
    return;
  }
  if (!start || busy) return;
  const s = start;
  start = null;
  const x = Math.min(s.x, e.clientX);
  const y = Math.min(s.y, e.clientY);
  const w = Math.abs(e.clientX - s.x);
  const h = Math.abs(e.clientY - s.y);
  // A click (no real drag) on a window captures that window's own content.
  if (w < MIN_REGION || h < MIN_REGION) {
    const win = windowAt(s.x, s.y);
    if (win) {
      busy = true;
      try {
        await invoke("capture_window", { id: win.id });
      } catch { busy = false; showError(); }
    } else {
      hint.classList.remove("hidden");
      fullDim();
    }
    return;
  }
  const a = toImg(x, y), b = toImg(x + w, y + h);
  busy = true;
  try {
    await invoke("capture_region", {
      index: IDX,
      x: Math.round(a.x),
      y: Math.round(a.y),
      width: Math.round(b.x - a.x),
      height: Math.round(b.y - a.y),
    });
  } catch {
    busy = false;
    showError();
  }
});

// Arrow keys nudge the picked pixel by exactly one PHYSICAL pixel — the precision
// the mouse cannot give on a scaled monitor. The OS cursor is hidden while picking,
// so the crosshair we draw is the only pointer and it follows the keys.
const NUDGE = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
const ZOOM_KEYS = { "+": 1, "=": 1, "-": -1, _: -1 };
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") { saveZoom(); invoke("snip_cancel"); return; }
  if (!pickMode || pickBusy) return;
  const d = NUDGE[e.key];
  const zk = ZOOM_KEYS[e.key];
  if (d && cur) {
    e.preventDefault();
    setCur(cur.x + d[0], cur.y + d[1]);
  } else if (zk) {
    e.preventDefault();
    setZoom(pickZoom + zk);
  } else if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    pickHere();
  }
});
window.addEventListener("contextmenu", (e) => e.preventDefault());
