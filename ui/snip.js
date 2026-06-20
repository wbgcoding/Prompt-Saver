// Snipping overlay (one window spanning the whole virtual desktop). The frozen
// capture stays fully sharp; four dim rectangles frame the selection. Drag a
// region across any monitors or click a window.
//
// Coordinates are mapped by RATIO against the displayed frozen image, never via
// the window scale factor — so the mapping is pixel-exact on any DPI mix.
const { invoke, convertFileSrc } = window.__TAURI__.core;

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

const MIN_REGION = 5; // CSS px; below this a drag counts as a click
const WIN_RADIUS = 8; // rounded corners when highlighting a window
const IDX = 0; // single overlay
let start = null;
let busy = false;
let windows = []; // stitched-image physical px, topmost first
let FULL_W = 1, FULL_H = 1; // full stitched dimensions (display image may be smaller)

// Seed hint/error text synchronously from the browser locale (en/de are the
// defaults) so a German user never sees an English flash; the async i18n load
// below refines it for the user's actually-selected language.
const SEED = {
  en: {
    hint: "Drag a region · click a window · Esc to cancel",
    failed: "This window can't be captured. Try a region or another window.",
  },
  de: {
    hint: "Bereich ziehen · Fenster anklicken · Esc zum Abbrechen",
    failed: "Dieses Fenster kann nicht aufgenommen werden. Versuche einen Bereich oder ein anderes Fenster.",
  },
};
const T = { ...((navigator.language || "en").toLowerCase().startsWith("de") ? SEED.de : SEED.en) };
hint.textContent = T.hint;

(async () => {
  const b = await invoke("snip_background", { index: IDX });
  if (!b) { invoke("snip_cancel"); return; }
  FULL_W = b.width || 1;
  FULL_H = b.height || 1;
  bg.src = b.is_file ? convertFileSrc(b.src) : b.src;
  try { windows = await invoke("snip_windows", { index: IDX }); } catch { windows = []; }
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
      hint.textContent = T.hint;
    })
    .catch(() => { hint.textContent = T.hint; });
})();

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
  fullDim();
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
fullDim();

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
  lastMove = e;
  if (!moveRaf) moveRaf = requestAnimationFrame(onMove);
});

window.addEventListener("mousedown", (e) => {
  if (e.button !== 0 || busy) return;
  hint.textContent = T.hint;
  hint.classList.remove("error");
  start = { x: e.clientX, y: e.clientY };
  hint.classList.add("hidden");
  sizeLabel.textContent = "";
  setRegion(e.clientX, e.clientY, 0, 0, 0);
});

window.addEventListener("mouseup", async (e) => {
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

window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") invoke("snip_cancel");
});
window.addEventListener("contextmenu", (e) => e.preventDefault());
