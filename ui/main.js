// Main-window controller. Uses Tauri's global API (withGlobalTauri).
import { I18N, LANGS } from "./i18n.js";
import { FONTS, FONT_LABELS, fitText } from "./fonts.js";
import { IMAGE_EXT, mediaKind, buildMediaBar, applyVideoPrefs, setMediaDefaults } from "./media.js";

const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const gridEl = $("grid");
const inputEl = $("input");
const saveBtn = $("save");
const toastEl = $("toast");
const ctxEl = $("ctx");
const settingsEl = $("settings");
const libraryEl = $("library");
const viewsEl = $("views");
const modal = {
  root: $("modal"),
  title: $("modal-title"),
  name: $("modal-name"),
  text: $("modal-text"),
  cancel: $("modal-cancel"),
  confirm: $("modal-confirm"),
  delete: $("modal-delete"),
  imgWrap: $("modal-img-wrap"),
  img: $("modal-img"),
  video: $("modal-video"),
  caption: $("modal-caption"),
  captionSize: $("modal-caption-size"),
  captionPreview: $("modal-caption-preview"),
  showImage: $("modal-show-image"),
  showText: $("modal-show-text"),
  replaceImg: $("modal-replace-img"),
  removeImg: $("modal-remove-img"),
  addIcon: $("modal-add-icon"),
  fileWrap: $("modal-file-wrap"),
  fileName: $("modal-file-name"),
  replaceFile: $("modal-replace-file"),
  fontSel: $("modal-font"),
  sizeSel: $("modal-size"),
  textBar: $("modal-text-bar"),
  varsHint: $("modal-vars-hint"),
};

const DRAG_THRESHOLD = 5;
const INPUT_MAX = 160; // keep in sync with .input max-height
// Grid-dimension and view-count limits are expert values: val("gridMax") /
// val("maxViews"). The backend keeps a hard ceiling of 100 for both.
let PREVIEW_MAX = 220; // tooltip preview length (expert-tunable)
let BUBBLE_MS = 950; // "Copied!" bubble lifetime (expert-tunable)
let TOAST_MS = 1400; // toast lifetime for plain messages (expert-tunable)
const SIZE_MIN = 10; // text size range, steps of 2 (keep in sync with backend)
const SIZE_MAX = 40;
const SIZE_STEP = 2;
const FILE_POLL_MS = 5000; // missing-file watcher interval

const clampGrid = (n, fallback) =>
  Math.min(val("gridMax"), Math.max(1, Math.round(Number(n) || fallback)));

// Cached state (refreshed by renderGrid).
let prompts = [];
let settings = { theme: "system", views: [], active_view: "" };

let modalState = null;
let modalInitial = ""; // snapshot of the open modal's content (unsaved-change guard)
let ctxId = null;
let drag = null;
let libQuery = ""; // prompt library search text
let libType = "all"; // prompt library type filter
let libColor = "all"; // prompt library color filter
let toastTimer = null;
let deleteAllTimer = null;
let expertResetTimer = null;
let versionLabel = ""; // "v1.6.0", shown in the update status

// Surface unexpected errors as a toast instead of failing silently.
window.addEventListener("error", (e) => toast(String(e.message)));
window.addEventListener("unhandledrejection", (e) => toast(String(e.reason)));

// Right-click is disabled everywhere inside the app (tiles re-enable it).
window.addEventListener("contextmenu", (e) => e.preventDefault());

const DOTS =
  '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d="M12 8a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm0 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm0 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z"/></svg>';
const CROSS =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M6.4 5 12 10.6 17.6 5 19 6.4 13.4 12 19 17.6 17.6 19 12 13.4 6.4 19 5 17.6 10.6 12 5 6.4Z"/></svg>';
const GRID_PLUS =
  '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="currentColor" d="M4 4h7v7H4V4Zm9 0h7v7h-7V4ZM4 13h7v7H4v-7Zm12 0h2v3h3v2h-3v3h-2v-3h-3v-2h3v-3Z"/></svg>';
const PLUS_ICON =
  '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="currentColor" d="M11 5h2v6h6v2h-6v6h-2v-6H5v-2h6V5Z"/></svg>';
// Corner badges marking what a tile copies (attached file / image).
const ICON_FILE =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M21.2 11.2l-8.4 8.4a5.5 5.5 0 0 1-7.8-7.8l8.4-8.4a3.7 3.7 0 0 1 5.2 5.2l-8.4 8.4a1.9 1.9 0 0 1-2.6-2.6l7.7-7.7"/></svg>';
const ICON_IMAGE =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="currentColor" d="M21 3H3a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h18a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Zm0 16H3V5h18v14Zm-5.5-9a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0ZM8.5 14l2-2.5 2 2.5 2.5-3 3.5 5H5l3.5-4.5Z"/></svg>';
const ICON_VIDEO =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="currentColor" d="M15 8.5V6a1 1 0 0 0-1-1H3a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h11a1 1 0 0 0 1-1v-2.5l5 4v-15l-5 4ZM6.5 9.8 12 13l-5.5 3.2V9.8Z"/></svg>';
// Plain grey document marker for PDF attachments (replaces the paperclip).
const ICON_PDF =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="currentColor" d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6Zm0 2 4 4h-4V4ZM8 12h8v1.6H8V12Zm0 3.2h8v1.6H8v-1.6Z"/></svg>';
const ICON_EDIT =
  '<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M4 20h4L18.5 9.5a2 2 0 0 0-2.8-2.8L5 17.2V20Zm9.5-12.5 3 3"/></svg>';

let LANG = "en";
function resolveLang(pref) {
  const p = (pref && pref !== "auto" ? pref : (navigator.language || "en")).toLowerCase();
  return LANGS.find((code) => p.startsWith(code)) || "en";
}
const t = (key) => I18N[LANG][key] ?? I18N.en[key] ?? key;

function applyI18n() {
  document.documentElement.lang = LANG;
  document.querySelectorAll("[data-i18n]").forEach((el) => { el.textContent = t(el.dataset.i18n); });
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => { el.placeholder = t(el.dataset.i18nPh); });
  document.querySelectorAll("[data-i18n-title]").forEach((el) => { el.title = t(el.dataset.i18nTitle); });
  document.querySelectorAll("[data-i18n-aria]").forEach((el) => { el.setAttribute("aria-label", t(el.dataset.i18nAria)); });
}

// ---- Helpers ----
function applyTheme(theme) {
  document.documentElement.setAttribute("data-theme", theme);
}

// Tile color palette ("" = default surface), full spectrum, one modal row.
// Empty = no color; the rest are hue-sorted and fill two rows of ten with the
// "none" and custom swatches.
const COLORS = [
  "",
  "#ef4444", "#f97316", "#f59e0b", "#eab308", "#84cc16",
  "#22c55e", "#10b981", "#14b8a6", "#06b6d4", "#0ea5e9",
  "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#d946ef",
  "#ec4899", "#f43f5e", "#64748b",
];

function applyTileStyle() {
  const root = document.documentElement.style;
  root.setProperty("--tile-font", FONTS[settings.tile_font] || FONTS.system);
  // tile_size 0 = auto-fit (per-tile, handled after every grid render).
  root.setProperty("--tile-size", `${settings.tile_size || 15}px`);
  fitCache.clear(); // font metrics changed -> cached fit sizes are stale
}

// Auto-fit cache per (text, cell size); cleared on font changes.
const fitCache = new Map();
const FIT_QUANT = 8; // measurement-box bucket size (see fitAllTiles)

// All fitting is measured inside ONE off-screen ruler pinned to 0/0: the
// result can never depend on a tile's own (sub)pixel position, cell or DPI.
let ruler = null;
function getRuler() {
  if (!ruler) {
    ruler = document.createElement("span");
    ruler.className = "tile-name fit tile-ruler";
    document.body.appendChild(ruler);
  }
  return ruler;
}

// Largest font size where the wrapped text fits the shared measurement box;
// depends only on (text, font, maxW, maxH).
function fitTileText(tile, maxW, maxH) {
  if (tile.classList.contains("has-image") || tile.dataset.fitMode === "fixed") return;
  const name = tile.querySelector(".tile-name");
  if (!name) return;
  name.classList.add("fit");
  const key = `${name.textContent}|${name.style.fontFamily}|${maxW}x${maxH}`;
  let size = fitCache.get(key);
  if (size == null) {
    const r = getRuler();
    r.style.fontFamily = name.style.fontFamily;
    r.textContent = name.textContent;
    r.style.width = `${maxW}px`;
    // scrollWidth only exceeds maxW when a single unbreakable word overflows.
    const fits = (s) => {
      r.style.fontSize = `${s}px`;
      return r.scrollHeight <= maxH && r.scrollWidth <= maxW;
    };
    let lo = 8;
    let hi = Math.max(8, Math.min(96, maxH));
    while (lo < hi) {
      const mid = Math.ceil((lo + hi) / 2);
      if (fits(mid)) lo = mid;
      else hi = mid - 1;
    }
    size = lo;
    if (fitCache.size > 1000) fitCache.clear();
    fitCache.set(key, size);
  }
  name.style.fontSize = `${size}px`;
}

// Measurement box cached per grid state: as long as the window and grid
// layout are unchanged, every render uses the EXACT same box — moving tiles
// around can never change a fitted text size.
const fitBox = { key: "", maxW: 0, maxH: 0 };

function fitAllTiles() {
  const globalAuto = Number(settings.tile_size) === 0;
  const tiles = [...gridEl.querySelectorAll(".tile")].filter(
    (tile) =>
      (globalAuto || tile.dataset.fitMode === "auto") &&
      tile.dataset.fitMode !== "fixed" &&
      !tile.classList.contains("has-image")
  );
  if (!tiles.length) return;
  const key =
    `${gridEl.style.gridTemplateColumns}|${gridEl.style.gridTemplateRows}` +
    // Include the gap: it changes the cell size without changing the grid's own
    // size or template, so without it a gap change would reuse a stale fit box
    // (text kept its old size, leaving wrong vertical spacing).
    `|${gridEl.clientWidth}x${gridEl.clientHeight}|${getComputedStyle(gridEl).gap}`;
  if (fitBox.key !== key) {
    // Shared box = smallest grid CELL (cells exist for every slot) minus the
    // tile chrome from computed style — fractional-exact, no per-cell rounding.
    let cellW = Infinity;
    let cellH = Infinity;
    for (const cell of gridEl.children) {
      cellW = Math.min(cellW, cell.clientWidth);
      cellH = Math.min(cellH, cell.clientHeight);
    }
    if (!Number.isFinite(cellW)) return;
    const cs = getComputedStyle(tiles[0]);
    const chromeW =
      parseFloat(cs.borderLeftWidth) + parseFloat(cs.borderRightWidth) +
      parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
    const chromeH =
      parseFloat(cs.borderTopWidth) + parseFloat(cs.borderBottomWidth) +
      parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
    fitBox.key = key;
    // Quantize to an 8px bucket: DPI handoffs and monitor drags shift cell
    // rounding by ±1px — inside one bucket the fitted size cannot change,
    // and the slack guarantees the text never clips after such a shift.
    fitBox.maxW = Math.floor((cellW - chromeW) / FIT_QUANT) * FIT_QUANT;
    fitBox.maxH = Math.floor((cellH - chromeH - 2) / FIT_QUANT) * FIT_QUANT;
  }
  if (fitBox.maxW <= 0 || fitBox.maxH <= 0) return;
  for (const tile of tiles) fitTileText(tile, fitBox.maxW, fitBox.maxH);
}

// Re-fit on window resize (cells change size with the window).
let fitRaf = 0;
window.addEventListener("resize", () => {
  cancelAnimationFrame(fitRaf);
  fitRaf = requestAnimationFrame(fitAllTiles);
});

// Re-measure cells when the window moves to a monitor with a different scale
// factor. The fit cache is kept: sizes are in DPI-independent CSS px, and the
// quantized box leaves enough slack that ±1px rounding can never clip — so a
// cross-monitor drag can never change an already fitted text size.
function watchDpr() {
  matchMedia(`(resolution: ${devicePixelRatio}dppx)`).addEventListener(
    "change",
    () => {
      fitBox.key = ""; // cell rounding differs at the new DPI
      fitAllTiles();
      watchDpr();
    },
    { once: true }
  );
}
watchDpr();

function hideToast() {
  toastEl.classList.remove("show");
  setTimeout(() => toastEl.classList.add("hidden"), 200);
}

// Optional action: { label, onClick } adds a button and keeps the toast longer.
function toast(msg, action = null) {
  toastEl.textContent = msg;
  toastEl.classList.toggle("actionable", !!action);
  if (action) {
    const btn = document.createElement("button");
    btn.className = "toast-btn";
    btn.textContent = action.label;
    btn.addEventListener("click", () => {
      hideToast();
      action.onClick();
    });
    toastEl.appendChild(btn);
  }
  toastEl.classList.remove("hidden");
  void toastEl.offsetWidth;
  toastEl.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(hideToast, action ? 12000 : TOAST_MS);
}

function autoGrow(el) {
  el.style.height = "auto";
  const target = el.scrollHeight + 2;
  el.style.height = `${Math.min(target, INPUT_MAX)}px`;
  el.style.overflowY = target > INPUT_MAX ? "auto" : "hidden";
}

const cellKey = (c, r) => `${c},${r}`;

// ---- Grid-size value picker ----
// Scrollable popup under a grid-size input: lists every value with a
// visible scrollbar, highlights and centers the current selection.
const numPop = document.createElement("div");
numPop.className = "num-pop hidden";
document.body.appendChild(numPop);
let popInput = null;
let popApply = null;

function renderNumPop() {
  const v = clampGrid(popInput.value, 1);
  numPop.innerHTML = "";
  let selected = null;
  for (let i = 1; i <= val("gridMax"); i++) {
    const row = document.createElement("button");
    row.type = "button";
    row.textContent = i;
    if (i === v) {
      row.className = "sel";
      selected = row;
    }
    row.addEventListener("pointerdown", (e) => {
      e.preventDefault(); // keep the input focused
      popInput.value = i;
      closeNumPop(true);
    });
    numPop.appendChild(row);
  }
  if (selected) selected.scrollIntoView({ block: "center" });
}

function openNumPop(input, apply) {
  popInput = input;
  popApply = apply;
  const r = input.getBoundingClientRect();
  numPop.style.left = `${Math.min(r.left, window.innerWidth - 70)}px`;
  numPop.style.top = `${r.bottom + 4}px`;
  // Unhide first: centering the selection needs a laid-out list.
  numPop.classList.remove("hidden");
  renderNumPop();
}

function closeNumPop(apply) {
  if (!popInput) return;
  numPop.classList.add("hidden");
  const done = popApply;
  popInput = null;
  popApply = null;
  if (apply && done) done();
}

// Generic list popup for <select>-backed pickers (8 rows visible, scrollbar
// only when the list overflows).
let popOnPick = null;
let popAnchor = null; // select that opened the popup (click again = close)
let popSuppressOpen = false;

function openValuePop(anchor, items, current, onPick) {
  popInput = null;
  popApply = null;
  popOnPick = onPick;
  popAnchor = anchor;
  numPop.innerHTML = "";
  let selected = null;
  for (const item of items) {
    const row = document.createElement("button");
    row.type = "button";
    row.textContent = item.label;
    // Font entries preview themselves in their own typeface.
    if (item.font) row.style.fontFamily = item.font;
    if (String(item.value) === String(current)) {
      row.className = "sel";
      selected = row;
    }
    row.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      const pick = popOnPick;
      closeValuePop();
      pick(item.value);
    });
    numPop.appendChild(row);
  }
  // Width follows the widest entry, not the anchor.
  const r = anchor.getBoundingClientRect();
  numPop.style.left = `${Math.min(r.left, window.innerWidth - 120)}px`;
  numPop.style.top = `${r.bottom + 4}px`;
  numPop.classList.remove("hidden");
  if (selected) selected.scrollIntoView({ block: "center" });
}

function closeValuePop() {
  popOnPick = null;
  popAnchor = null;
  numPop.classList.add("hidden");
}

document.addEventListener("pointerdown", (e) => {
  if (!popOnPick || numPop.contains(e.target)) return;
  // Clicking the anchor of the open popup toggles it closed instead of
  // letting the following mousedown reopen it immediately.
  popSuppressOpen = !!popAnchor && popAnchor.contains(e.target);
  closeValuePop();
});

// Replace the native dropdown of a <select> with the scrollable popup.
function attachSelectPicker(sel) {
  sel.addEventListener("mousedown", (e) => {
    e.preventDefault();
    if (popSuppressOpen) { popSuppressOpen = false; return; }
    const items = [...sel.options].map((o) => ({
      value: o.value,
      label: o.textContent,
      font: o.style.fontFamily,
    }));
    openValuePop(sel, items, sel.value, (v) => {
      sel.value = v;
      sel.dispatchEvent(new Event("change"));
    });
  });
}

// Combo behaviour: typing stays possible, focus opens the picker,
// wheel / arrow keys step through the values, blur or Enter applies.
function attachGridPicker(input, apply) {
  input.addEventListener("focus", () => openNumPop(input, apply));
  // Re-open on click even when the input kept focus after a pick.
  // Apply a still-open sibling picker first (pointerdown fires before blur).
  input.addEventListener("pointerdown", () => {
    if (popInput === input) return;
    closeNumPop(true);
    openNumPop(input, apply);
  });
  input.addEventListener("input", () => { if (popInput === input) renderNumPop(); });
  input.addEventListener("blur", () => { if (popInput === input) closeNumPop(true); });
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") input.blur();
    else if (e.key === "Escape") closeNumPop(false);
  });
  input.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      input.value = clampGrid(Number(input.value) + (e.deltaY > 0 ? 1 : -1), 1);
      if (popInput === input) renderNumPop();
      else openNumPop(input, apply);
    },
    { passive: false }
  );
}

// ---- Custom color picker (theme-styled popup with SV field + hue bar) ----
const colorPop = document.createElement("div");
colorPop.className = "color-pop hidden";
colorPop.innerHTML =
  '<div class="cp-sv"><div class="cp-knob"></div></div>' +
  '<input class="cp-hue" type="range" min="0" max="360" step="1" aria-label="Hue" />' +
  '<div class="cp-row"><span class="cp-preview"></span><input class="cp-hex" type="text" maxlength="7" spellcheck="false" aria-label="Hex" /></div>';
document.body.appendChild(colorPop);
const cpSv = colorPop.querySelector(".cp-sv");
const cpKnob = colorPop.querySelector(".cp-knob");
const cpHue = colorPop.querySelector(".cp-hue");
const cpPreview = colorPop.querySelector(".cp-preview");
const cpHex = colorPop.querySelector(".cp-hex");
let cp = { h: 215, s: 0.85, v: 0.92 };
let cpOnPick = null;

function hsvToHex({ h, s, v }) {
  const f = (n) => {
    const k = (n + h / 60) % 6;
    const c = v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
    return Math.round(c * 255).toString(16).padStart(2, "0");
  };
  return `#${f(5)}${f(3)}${f(1)}`;
}

function hexToHsv(hex) {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex || "");
  if (!m) return { h: 215, s: 0.85, v: 0.92 };
  const n = parseInt(m[1], 16);
  const r = (n >> 16) / 255;
  const g = ((n >> 8) & 255) / 255;
  const b = (n & 255) / 255;
  const max = Math.max(r, g, b);
  const d = max - Math.min(r, g, b);
  let h = 0;
  if (d) {
    h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
    h = Math.round(h * 60);
    if (h < 0) h += 360;
  }
  return { h, s: max ? d / max : 0, v: max };
}

function cpRender(notify = true) {
  cpSv.style.background =
    `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, hsl(${cp.h}, 100%, 50%))`;
  cpKnob.style.left = `${cp.s * 100}%`;
  cpKnob.style.top = `${(1 - cp.v) * 100}%`;
  cpHue.value = cp.h;
  const hex = hsvToHex(cp);
  cpPreview.style.background = hex;
  if (document.activeElement !== cpHex) cpHex.value = hex;
  if (notify) cpOnPick?.(hex);
}

function openColorPop(anchor, current, onPick) {
  cp = hexToHsv(current);
  cpOnPick = null;
  cpRender(false); // show the current color without re-triggering the pick
  cpOnPick = onPick;
  const r = anchor.getBoundingClientRect();
  colorPop.classList.remove("hidden");
  const w = colorPop.offsetWidth;
  colorPop.style.left = `${Math.min(r.left, window.innerWidth - w - 8)}px`;
  colorPop.style.top = `${r.bottom + 6}px`;
}

function closeColorPop() {
  cpOnPick = null;
  colorPop.classList.add("hidden");
}

// Floating preset palette anchored to a swatch button (views editor rows).
const swatchPop = document.createElement("div");
swatchPop.className = "swatch-pop swatches hidden";
document.body.appendChild(swatchPop);
function openSwatchPop(anchor, selected, onPick) {
  buildSwatches(
    swatchPop,
    selected,
    (hex) => { onPick(hex); closeSwatchPop(); },
    (a, cur) => openColorPop(anchor, cur, (hex) => onPick(hex))
  );
  swatchPop.classList.remove("hidden");
  const r = anchor.getBoundingClientRect();
  const w = swatchPop.offsetWidth;
  swatchPop.style.left = `${Math.min(r.left, window.innerWidth - w - 8)}px`;
  swatchPop.style.top = `${r.bottom + 6}px`;
}
function closeSwatchPop() {
  swatchPop.classList.add("hidden");
}
document.addEventListener("pointerdown", (e) => {
  if (swatchPop.classList.contains("hidden")) return;
  if (!swatchPop.contains(e.target) && !e.target.closest(".view-color-dot")) closeSwatchPop();
});

function cpDrag(e) {
  const r = cpSv.getBoundingClientRect();
  cp.s = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  cp.v = Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height));
  cpRender();
}
cpSv.addEventListener("pointerdown", (e) => {
  cpSv.setPointerCapture(e.pointerId);
  cpDrag(e);
});
cpSv.addEventListener("pointermove", (e) => {
  if (cpSv.hasPointerCapture(e.pointerId)) cpDrag(e);
});
cpHue.addEventListener("input", () => {
  cp.h = Number(cpHue.value);
  cpRender();
});
cpHex.addEventListener("change", () => {
  if (/^#?[0-9a-f]{6}$/i.test(cpHex.value)) {
    cp = hexToHsv(cpHex.value);
    cpRender();
  }
});
document.addEventListener("pointerdown", (e) => {
  if (colorPop.classList.contains("hidden")) return;
  if (!colorPop.contains(e.target) && !e.target.closest(".swatch.custom")) closeColorPop();
});

// Two-step confirmation: first call arms the button and returns false,
// the second call (while armed) returns true.
function armButton(btn, confirmLabel) {
  if (btn.classList.contains("confirm")) return true;
  btn.classList.add("confirm");
  btn.textContent = confirmLabel;
  return false;
}
function disarmButton(btn, label) {
  btn.classList.remove("confirm");
  btn.textContent = label;
}

// ---- Collapsible bars (header / composer) ----
function applyBars() {
  const header = settings.show_header !== false;
  const composer = settings.show_composer !== false;
  document.body.classList.toggle("no-header", !header);
  document.body.classList.toggle("no-composer", !composer);
  $("show-top").classList.toggle("hidden", header);
  $("show-bottom").classList.toggle("hidden", composer);
  // The grid area changed size — re-fit the tile text.
  requestAnimationFrame(fitAllTiles);
}

function setBars(header, composer) {
  settings.show_header = header;
  settings.show_composer = composer;
  invoke("set_bars", { header, composer }).catch(() => {});
  applyBars();
}

// ---- Expert feature flags & values ----
// Every feature ships enabled; the expert menu lets the user switch single
// pieces off. A missing flag means enabled, so old configs keep everything on.
const FLAG_LABELS = {
  fileAttach: "flagFileAttach",
  screenshot: "flagScreenshot",
  multiView: "flagMultiView",
  quickGrid: "flagQuickGrid",
  floating: "flagFloating",
  barToggles: "flagBarToggles",
  tileMenu: "flagTileMenu",
  tileHover: "flagTileHover",
  tilePreview: "flagTilePreview",
  typeBadges: "flagTypeBadges",
  captions: "flagCaptions",
  copyBubble: "flagCopyBubble",
  videoControls: "flagVideoControls",
  videoAutoplay: "flagVideoAutoplay",
  videoMuted: "flagVideoMuted",
  videoLoop: "flagVideoLoop",
  animations: "flagAnimations",
  pinButton: "flagPinButton",
  pasteMedia: "flagPasteMedia",
  dragDrop: "flagDragDrop",
  promptVars: "flagPromptVars",
  copyHistory: "flagCopyHistory",
  historyTimestamps: "flagHistoryTimestamps",
  librarySearch: "flagLibrarySearch",
  colorFilter: "flagColorFilter",
  closeAfterCopy: "flagCloseAfterCopy",
  libraryCloseToggle: "flagLibraryCloseToggle",
  imagePreview: "flagImagePreview",
  confirmDiscard: "flagConfirmDiscard",
  showLogo: "flagShowLogo",
  showTitle: "flagShowTitle",
};
const ALL_FLAG_KEYS = Object.keys(FLAG_LABELS);

// Numeric value tweaks. `def` is the shipped default; a missing value uses it.
const EXPERT_VALUES = {
  videoVolume: { label: "valVideoVolume", min: 0, max: 100, step: 5, def: 100, unit: "%" },
  animSpeed: { label: "valAnimSpeed", min: 0, max: 400, step: 10, def: 150, unit: "ms" },
  gridGap: { label: "valGridGap", min: 0, max: 24, step: 1, def: 8, unit: "px" },
  bubbleMs: { label: "valBubbleMs", min: 300, max: 3000, step: 50, def: 950, unit: "ms" },
  toastMs: { label: "valToastMs", min: 800, max: 5000, step: 100, def: 1400, unit: "ms" },
  previewLen: { label: "valPreviewLen", min: 0, max: 500, step: 20, def: 220, unit: "" },
  viewBorder: { label: "valViewBorder", min: 1, max: 6, step: 1, def: 3, unit: "px" },
  // Floating-button opacity (%). Floored at 20% so the pill never vanishes.
  floatOpacity: { label: "valFloatOpacity", min: 20, max: 100, step: 5, def: 100, unit: "%" },
  // Appearance scales (percent, 100 = unchanged). Applied as CSS zoom.
  uiScale: { label: "valUiScale", min: 50, max: 200, step: 5, def: 100, unit: "%" },
  modalScale: { label: "valModalScale", min: 60, max: 200, step: 5, def: 100, unit: "%" },
  composerScale: { label: "valComposerScale", min: 60, max: 200, step: 5, def: 100, unit: "%" },
  iconScale: { label: "valIconScale", min: 60, max: 200, step: 5, def: 100, unit: "%" },
  primaryScale: { label: "valPrimaryScale", min: 60, max: 200, step: 5, def: 100, unit: "%" },
  // Limit overrides (defaults match the classic 20; backend ceiling is 100).
  maxViews: { label: "valMaxViews", min: 1, max: 100, step: 1, def: 20, unit: "" },
  gridMax: { label: "valGridMax", min: 1, max: 100, step: 1, def: 20, unit: "" },
  // Copy-history length (0 = keep none).
  historyMax: { label: "valHistoryMax", min: 0, max: 200, step: 10, def: 50, unit: "" },
};

// Preset-or-custom numeric settings (dropdown with a free-entry option).
const EXPERT_SELECTS = {
  historyDays: { label: "valHistoryDays", options: [1, 3, 7, 30], def: 7, unit: "d" },
};

// Dropdowns like the settings' text-size / font selects. copySize lives in
// ui_values (0 = auto-fit to the button), copyFont in ui_texts ("" = default).
const EXPERT_DROPDOWNS = {
  copySize: {
    label: "valCopySize",
    options: () => {
      const o = [["0", t("langAuto")]];
      for (let s = SIZE_MIN; s <= SIZE_MAX; s += SIZE_STEP) o.push([String(s), String(s)]);
      return o;
    },
    get: () => String(Number(settings.ui_values?.copySize) || 0),
    set: async (v) => {
      settings.ui_values = { ...(settings.ui_values || {}), copySize: Number(v) };
      try { await invoke("set_ui_value", { key: "copySize", value: Number(v) }); } catch (e) { toast(String(e)); }
    },
  },
  copyFont: {
    label: "valCopyFont",
    options: () => {
      const o = [["", t("styleDefault"), ""]];
      for (const [key, stack] of Object.entries(FONTS)) {
        o.push([key, FONT_LABELS[key] ?? t(key === "script" ? "fontScript" : "fontSystem"), stack]);
      }
      return o;
    },
    get: () => settings.ui_texts?.copyFont ?? "",
    set: async (v) => {
      settings.ui_texts = { ...(settings.ui_texts || {}), copyFont: v };
      try { await invoke("set_ui_text", { key: "copyFont", value: v }); } catch (e) { toast(String(e)); }
    },
  },
};

// Expert menu organised into tabs (it has grown large — tabs keep it tidy).
const EXPERT_TABS = [
  { title: "expTabFeatures", groups: [
    { title: "expGroupCreate", flags: ["fileAttach", "screenshot", "pasteMedia", "dragDrop", "promptVars"] },
    { title: "expGroupWorkspace", flags: ["multiView", "quickGrid", "floating", "pinButton", "barToggles", "showLogo", "showTitle"] },
    { title: "expGroupTiles", flags: ["tileMenu", "tileHover", "tilePreview", "typeBadges", "captions", "copyBubble"] },
    { title: "expGroupLibrary", flags: ["librarySearch", "colorFilter", "imagePreview", "closeAfterCopy", "libraryCloseToggle", "confirmDiscard"] },
  ] },
  { title: "expTabAppearance", groups: [
    { title: "expGroupScale", values: ["uiScale", "modalScale", "composerScale", "iconScale", "primaryScale"] },
    { title: "expGroupVisual", flags: ["animations"], values: ["animSpeed", "gridGap", "viewBorder", "floatOpacity"], dropdowns: ["copySize", "copyFont"] },
    { title: "expGroupLimits", values: ["maxViews", "gridMax", "previewLen", "bubbleMs", "toastMs"] },
  ] },
  { title: "expTabPrivacy", groups: [
    { title: "expGroupPrivacy", flags: ["copyHistory", "historyTimestamps"], values: ["historyMax"], selects: ["historyDays"] },
  ] },
  { title: "expTabMedia", groups: [
    { title: "expGroupMedia", flags: ["videoControls", "videoAutoplay", "videoMuted", "videoLoop"], values: ["videoVolume"] },
  ] },
];

const flag = (key) => settings.ui_flags?.[key] !== false;
const val = (key) => {
  const v = settings.ui_values?.[key];
  return Number.isFinite(v) ? v : EXPERT_VALUES[key].def;
};
const txt = (key) => settings.ui_texts?.[key] || "";

// Mirror each disabled flag onto a body class so CSS can hide the matching UI.
function applyFlags() {
  for (const key of ALL_FLAG_KEYS) {
    document.body.classList.toggle(`noflag-${key}`, !flag(key));
  }
  applyValues();
}

// Apply every numeric tweak to its live target (CSS vars + JS constants).
function applyValues() {
  const root = document.documentElement.style;
  root.setProperty("--transition", `${val("animSpeed")}ms cubic-bezier(0.4, 0, 0.2, 1)`);
  root.setProperty("--gap", `${val("gridGap")}px`);
  root.setProperty("--view-border", `${val("viewBorder")}px`);
  // Appearance scales (CSS zoom; 1 = unchanged).
  root.setProperty("--ui-zoom", val("uiScale") / 100);
  root.setProperty("--modal-zoom", val("modalScale") / 100);
  root.setProperty("--composer-zoom", val("composerScale") / 100);
  root.setProperty("--icon-zoom", val("iconScale") / 100);
  root.setProperty("--primary-zoom", val("primaryScale") / 100);
  BUBBLE_MS = val("bubbleMs");
  TOAST_MS = val("toastMs");
  PREVIEW_MAX = val("previewLen");
  setMediaDefaults({ volume: val("videoVolume"), muted: flag("videoMuted"), looped: flag("videoLoop") });
}

function flagRow(key) {
  const row = document.createElement("label");
  row.className = "field switch-field";
  const span = document.createElement("span");
  span.textContent = t(FLAG_LABELS[key]);
  const input = document.createElement("input");
  input.type = "checkbox";
  input.className = "switch";
  input.checked = flag(key);
  input.addEventListener("change", async () => {
    const enabled = input.checked;
    settings.ui_flags = { ...(settings.ui_flags || {}), [key]: enabled };
    try {
      await invoke("set_ui_flag", { key, enabled });
    } catch (err) {
      toast(String(err));
    }
    applyFlags();
    renderViews();
    await renderGrid(true);
  });
  row.append(span, input);
  return row;
}

function valueRow(key) {
  const cfg = EXPERT_VALUES[key];
  const fmt = (v) => `${v}${cfg.unit}`;
  const row = document.createElement("div");
  row.className = "field value-field";
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(cfg.label);
  const out = document.createElement("span");
  out.className = "value-out";
  out.textContent = fmt(val(key));
  head.append(span, out);
  const input = document.createElement("input");
  input.type = "range";
  input.className = "value-range";
  input.min = cfg.min;
  input.max = cfg.max;
  input.step = cfg.step;
  input.value = val(key);
  input.addEventListener("input", () => (out.textContent = fmt(Number(input.value))));
  input.addEventListener("change", async () => {
    const value = Number(input.value);
    settings.ui_values = { ...(settings.ui_values || {}), [key]: value };
    try {
      await invoke("set_ui_value", { key, value });
    } catch (err) {
      toast(String(err));
    }
    applyValues();
    await renderGrid(true);
  });
  row.append(head, input);
  return row;
}

// A preset dropdown (e.g. 1d/3d/7d/30d) plus a free-entry "custom" option.
function selectRow(key) {
  const cfg = EXPERT_SELECTS[key];
  const cur = Number.isFinite(settings.ui_values?.[key]) ? settings.ui_values[key] : cfg.def;
  const row = document.createElement("div");
  row.className = "field value-field";
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(cfg.label);
  head.appendChild(span);
  const sel = document.createElement("select");
  sel.className = "modal-input expert-select";
  for (const o of cfg.options) {
    const opt = document.createElement("option");
    opt.value = String(o);
    opt.textContent = `${o}${cfg.unit}`;
    sel.appendChild(opt);
  }
  const customOpt = document.createElement("option");
  customOpt.value = "custom";
  customOpt.textContent = t("valCustom");
  sel.appendChild(customOpt);
  const num = document.createElement("input");
  num.type = "number";
  num.min = 1;
  num.max = 3650;
  num.className = "modal-input expert-num";
  const isPreset = cfg.options.includes(cur);
  sel.value = isPreset ? String(cur) : "custom";
  num.value = cur;
  num.classList.toggle("hidden", isPreset);
  const persist = async (value) => {
    settings.ui_values = { ...(settings.ui_values || {}), [key]: value };
    try { await invoke("set_ui_value", { key, value }); } catch (err) { toast(String(err)); }
  };
  sel.addEventListener("change", () => {
    if (sel.value === "custom") {
      num.classList.remove("hidden");
      num.focus();
      persist(Math.max(1, Number(num.value) || cfg.def));
    } else {
      num.classList.add("hidden");
      persist(Number(sel.value));
    }
  });
  num.addEventListener("change", () => {
    const v = Math.max(1, Math.min(3650, Number(num.value) || cfg.def));
    num.value = v;
    persist(v);
  });
  row.append(head, sel, num);
  return row;
}

// A plain value/font dropdown (no custom entry), like the settings selects.
function dropdownRow(key) {
  const cfg = EXPERT_DROPDOWNS[key];
  const row = document.createElement("div");
  row.className = "field value-field";
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(cfg.label);
  head.appendChild(span);
  const sel = document.createElement("select");
  sel.className = "modal-input expert-select";
  for (const [value, lbl, stack] of cfg.options()) {
    const o = document.createElement("option");
    o.value = value;
    o.textContent = lbl;
    if (stack) o.style.fontFamily = stack;
    sel.appendChild(o);
  }
  sel.value = cfg.get();
  sel.addEventListener("change", () => cfg.set(sel.value));
  row.append(head, sel);
  return row;
}

let expertTab = 0;
function renderExpert() {
  const box = $("expert-flags");
  box.innerHTML = "";
  if (expertTab >= EXPERT_TABS.length) expertTab = 0;
  // Tab bar.
  const bar = document.createElement("div");
  bar.className = "expert-tabs";
  EXPERT_TABS.forEach((tab, i) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "expert-tab" + (i === expertTab ? " active" : "");
    b.textContent = t(tab.title);
    b.addEventListener("click", () => { expertTab = i; renderExpert(); });
    bar.appendChild(b);
  });
  box.appendChild(bar);
  // Active tab's groups.
  for (const group of EXPERT_TABS[expertTab].groups) {
    const sec = document.createElement("div");
    sec.className = "expert-group";
    const head = document.createElement("div");
    head.className = "expert-group-title";
    head.textContent = t(group.title);
    sec.appendChild(head);
    for (const key of group.flags || []) sec.appendChild(flagRow(key));
    for (const key of group.values || []) sec.appendChild(valueRow(key));
    for (const key of group.selects || []) sec.appendChild(selectRow(key));
    for (const key of group.dropdowns || []) sec.appendChild(dropdownRow(key));
    box.appendChild(sec);
  }
}

async function onResetExpert() {
  settings.ui_flags = {};
  settings.ui_values = {};
  settings.ui_texts = {};
  try {
    await invoke("reset_expert");
  } catch (err) {
    toast(String(err));
  }
  applyFlags();
  renderExpert();
  renderViews();
  await renderGrid(true);
  toast(t("expertResetDone"));
}

// Switch between the main settings page and the expert sub-page.
function showExpertPage(on) {
  $("settings-main").classList.toggle("hidden", on);
  $("settings-expert").classList.toggle("hidden", !on);
  $("expert-back").classList.toggle("hidden", !on);
  $("settings-title").textContent = on ? t("expertMenu") : t("settings");
  if (on) renderExpert();
}

// ---- View helpers ----
function activeView() {
  return settings.views.find((v) => v.id === settings.active_view) || settings.views[0];
}
const gridKeyOf = (v) => `${v.cols}x${v.rows}`;
const layoutOf = (v) => v.layouts?.[gridKeyOf(v)] || {};

// ---- Layout normalization (active view, current grid size) ----
function firstFree(occupied, cols, rows) {
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      if (!occupied.has(cellKey(c, r))) return [c, r];
    }
  }
  return null;
}

function normalizeLayout(view) {
  const { cols, rows } = view;
  const layout = { ...layoutOf(view) };
  const occupied = new Map();
  const ids = new Set(prompts.map((p) => p.id));
  let changed = false;

  for (const id of Object.keys(layout)) {
    if (!ids.has(id)) { delete layout[id]; changed = true; }
  }
  for (const p of prompts) {
    const cell = layout[p.id];
    if (cell && cell[0] < cols && cell[1] < rows && !occupied.has(cellKey(...cell))) {
      occupied.set(cellKey(...cell), p.id);
    } else if (cell) {
      delete layout[p.id];
      changed = true;
    }
  }
  // Unplaced prompts stay reachable via the library. No auto-fill: saved
  // per-grid-size arrangements must stay untouched.
  view.layouts[gridKeyOf(view)] = layout;
  return changed;
}

// ---- Render ----
// skipFetch: caller already updated the local state (drag/hide hot path).
async function renderGrid(skipFetch = false) {
  if (!skipFetch) {
    const s = await invoke("get_state");
    prompts = s.prompts;
    settings = s.settings;
  }
  const view = activeView();
  if (normalizeLayout(view)) {
    invoke("set_layout", { layout: layoutOf(view) }).catch(() => {});
  }

  const { cols, rows } = view;
  const layout = layoutOf(view);
  gridEl.style.gridTemplateColumns = `repeat(${cols}, 1fr)`;
  gridEl.style.gridTemplateRows = `repeat(${rows}, 1fr)`;
  $("qg-cols").value = cols;
  $("qg-rows").value = rows;
  $("qg-cols").max = val("gridMax");
  $("qg-rows").max = val("gridMax");

  const byCell = new Map();
  for (const p of prompts) {
    const cell = layout[p.id];
    if (cell) byCell.set(cellKey(...cell), p);
  }

  // Build off-DOM, attach once (1 reflow instead of cols*rows).
  const frag = document.createDocumentFragment();
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const cellEl = document.createElement("div");
      cellEl.className = "cell";
      cellEl.dataset.col = c;
      cellEl.dataset.row = r;
      const p = byCell.get(cellKey(c, r));
      if (p) cellEl.appendChild(buildTile(p));
      frag.appendChild(cellEl);
    }
  }
  gridEl.innerHTML = "";
  gridEl.appendChild(frag);

  pruneVideoCache();
  renderViews();
  fitAllTiles();
}

// Header view switcher: one button per view (switch on click, manage on
// right-click) plus a trailing "+" to add a view via the name popup.
function renderViews() {
  viewsEl.innerHTML = "";
  // Multi-view disabled (expert menu): no switcher, no add button.
  if (!flag("multiView")) {
    viewsEl.classList.add("hidden");
    return;
  }
  for (const v of settings.views) {
    const btn = document.createElement("button");
    btn.className = "view-btn" + (v.id === settings.active_view ? " active" : "");
    if (v.color) {
      btn.classList.add("colored");
      btn.style.setProperty("--view-color", v.color);
    }
    btn.textContent = v.name;
    btn.title = `${v.name}\n${t("renameOrDeleteHint")}`;
    btn.addEventListener("click", async () => {
      settings = await invoke("set_active_view", { id: v.id });
      await renderGrid(true); // prompts unchanged, settings already fresh
    });
    btn.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      openViewModal(v);
    });
    viewsEl.appendChild(btn);
  }
  if (settings.views.length < val("maxViews")) {
    const add = document.createElement("button");
    add.className = "view-btn view-add-top";
    add.innerHTML = PLUS_ICON;
    add.title = t("addView");
    add.setAttribute("aria-label", t("addView"));
    add.addEventListener("click", () => openViewModal(null));
    viewsEl.appendChild(add);
  }
  viewsEl.classList.toggle("hidden", viewsEl.childElementCount === 0);
}

// ---- View add / rename / delete popup ----
const viewModal = {
  root: $("view-modal"),
  title: $("view-modal-title"),
  name: $("view-modal-name"),
  colorRow: $("view-color-row"),
  confirm: $("view-modal-confirm"),
  cancel: $("view-modal-cancel"),
  delete: $("view-modal-delete"),
  close: $("view-modal-close"),
};
let viewModalId = null; // null = create, otherwise the view being edited
let viewModalColor = ""; // empty = default

function renderViewColor() {
  buildSwatches(
    viewModal.colorRow,
    viewModalColor,
    (hex) => { viewModalColor = hex; renderViewColor(); },
    (anchor, cur) => openColorPop(anchor, cur, (hex) => { viewModalColor = hex; renderViewColor(); })
  );
}

function openViewModal(view) {
  viewModalId = view ? view.id : null;
  viewModalColor = view ? (view.color || "") : "";
  // The add-view label carries a leading "+"; the popup title drops it.
  viewModal.title.textContent = view ? t("renameView") : t("addView").replace(/^\+\s*/, "");
  viewModal.name.value = view ? view.name : "";
  renderViewColor();
  // Delete only when editing and at least two views remain.
  const canDelete = !!view && settings.views.length > 1;
  viewModal.delete.classList.toggle("hidden", !canDelete);
  disarmButton(viewModal.delete, t("delete"));
  viewModal.root.classList.remove("hidden");
  viewModal.name.focus();
  viewModal.name.select();
}

function closeViewModal() {
  viewModal.root.classList.add("hidden");
  viewModalId = null;
}

async function confirmViewModal() {
  const name = viewModal.name.value.trim();
  if (!name) { viewModal.name.focus(); return; }
  try {
    if (viewModalId) {
      await invoke("rename_view", { id: viewModalId, name });
      settings = await invoke("set_view_color", { id: viewModalId, color: viewModalColor });
    } else {
      settings = await invoke("add_view", { name, color: viewModalColor });
    }
  } catch (err) {
    toast(String(err));
    return;
  }
  closeViewModal();
  renderViews();
  renderViewsEditor();
  await renderGrid(true);
}

function buildTile(p) {
  const tile = document.createElement("div");
  tile.className = "tile";
  tile.dataset.id = p.id;
  const raw = p.file_path || p.text;
  const preview = PREVIEW_MAX <= 0 ? "" : raw.length > PREVIEW_MAX ? `${raw.slice(0, PREVIEW_MAX)}…` : raw;
  tile.title = !flag("tilePreview")
    ? ""
    : preview
      ? `${p.name}\n\n${preview}\n\n${t("tileTooltip")}`
      : `${p.name}\n${t("tileTooltip")}`;

  // Media on the tile: stored still, or gif/video from the icon/file path.
  const kind = p.file_path ? mediaKind(p.file_path) : "";
  const iconKind = !p.image && p.icon_path ? mediaKind(p.icon_path) : "";
  const showPath = iconKind ? p.icon_path : p.file_path;
  const showKind = p.image ? "" : iconKind || kind;
  const pathVideo = p.show_image && !p.image && showKind === "video";
  const pathGif = p.show_image && !p.image && showKind === "gif";
  if (p.show_image && (p.image || pathVideo || pathGif)) {
    tile.classList.add("has-image");
    // The chosen color tints the border area around the image.
    if (p.color) {
      tile.style.background = p.color;
      tile.style.borderColor = p.color;
    }
    if (pathVideo) {
      tile.appendChild(getVideoWrap(p, convertFileSrc(showPath)));
      // The control bar only appears while the mouse is in the lower part.
      tile.addEventListener("mousemove", (e) => {
        const r = tile.getBoundingClientRect();
        const zone = Math.max(48, r.height * 0.35);
        // Stay visible while on the bar itself (volume popup reaches higher).
        tile.classList.toggle(
          "media-hover",
          e.clientY > r.bottom - zone || !!e.target.closest(".media-bar")
        );
      });
      tile.addEventListener("mouseleave", () => {
        if (!tile.querySelector(".media-sound.dragging")) {
          tile.classList.remove("media-hover");
        }
      });
    } else {
      const img = document.createElement("img");
      img.className = "tile-img";
      img.src = p.image || convertFileSrc(showPath);
      img.draggable = false;
      tile.appendChild(img);
    }
    // Optional caption overlay (0 = default, 1 = auto-scale, else fixed px).
    if (p.caption && flag("captions")) {
      const cap = document.createElement("span");
      cap.className = "tile-caption";
      cap.textContent = p.caption;
      if (p.caption_size === 1) cap.classList.add("auto");
      else if (p.caption_size > 1) cap.style.fontSize = `${p.caption_size}px`;
      tile.appendChild(cap);
    }
  } else if (p.color) {
    tile.classList.add("tinted");
    tile.style.background = p.color;
    tile.style.borderColor = p.color;
  }

  const name = document.createElement("span");
  name.className = "tile-name";
  name.textContent = p.name;
  // Per-tile style overrides (0 = follow settings, 1 = auto-fit, else fixed).
  if (p.font) name.style.fontFamily = FONTS[p.font] || "";
  if (p.font_size === 1) {
    tile.dataset.fitMode = "auto";
  } else if (p.font_size > 1) {
    name.style.fontSize = `${p.font_size}px`;
    tile.dataset.fitMode = "fixed";
  }
  tile.appendChild(name);

  // Subtle type badge in the top-left corner. It reflects what the button
  // COPIES (file/image/video) — a decorative media icon never changes it.
  const typeIcon = p.file_path
    ? (kind === "video" ? ICON_VIDEO : kind ? ICON_IMAGE : PDF_EXT.test(p.file_path) ? ICON_PDF : ICON_FILE)
    : p.copy_image ? ICON_IMAGE : "";
  if (typeIcon && flag("typeBadges")) {
    const badge = document.createElement("span");
    badge.className = "tile-type";
    badge.innerHTML = typeIcon;
    tile.appendChild(badge);
  }

  // The actions menu (⋮ button + right-click) can be switched off entirely.
  if (flag("tileMenu")) {
    const menuBtn = document.createElement("button");
    menuBtn.className = "tile-menu";
    menuBtn.innerHTML = DOTS;
    menuBtn.title = t("actions");
    menuBtn.addEventListener("pointerdown", (e) => e.stopPropagation());
    menuBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      const r = menuBtn.getBoundingClientRect();
      openCtx(p.id, r.left, r.bottom + 4);
    });
    tile.appendChild(menuBtn);
    tile.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      openCtx(p.id, e.clientX, e.clientY);
    });
  }

  tile.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || e.target.closest(".tile-menu")) return;
    drag = { id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: tile };
  });

  // Prompts with an attached file or media icon get a persistent error
  // banner while that file is missing.
  if (p.file_path || p.icon_path) {
    const err = document.createElement("span");
    err.className = "tile-error";
    err.textContent = t("fileMissing");
    tile.appendChild(err);
    if (missingFiles.has(p.id)) tile.classList.add("file-missing");
  }
  return tile;
}

// ---- Video tiles: YouTube-style hover bar ----
// Video tiles keep their DOM element across grid re-renders — rebuilding
// would restart playback. Reparenting the cached wrapper does not.
const videoCache = new Map(); // prompt id -> { src, wrap }

function getVideoWrap(p, src) {
  const cached = videoCache.get(p.id);
  if (cached && cached.src === src) return cached.wrap;
  const wrap = document.createElement("div");
  wrap.className = "tile-media";
  const video = document.createElement("video");
  video.className = "tile-video";
  video.src = src;
  video.muted = true;
  video.loop = true;
  video.autoplay = flag("videoAutoplay");
  video.playsInline = true;
  wrap.append(video);
  if (flag("videoControls")) {
    wrap.append(buildMediaBar(video, {
      onChange: (prefs) => invoke("set_video_prefs", { id: p.id, ...prefs }).catch(() => {}),
    }));
  }
  applyVideoPrefs(video, settings.video_prefs?.[p.id]);
  videoCache.set(p.id, { src, wrap });
  return wrap;
}

function pruneVideoCache() {
  for (const id of videoCache.keys()) {
    if (!prompts.some((p) => p.id === id)) videoCache.delete(id);
  }
}

// Looping tile videos pause while the window is hidden (tray/minimised)
// and resume afterwards — manual pauses stay paused.
document.addEventListener("visibilitychange", () => {
  document.querySelectorAll(".tile-video").forEach((v) => {
    if (document.hidden) {
      if (!v.paused) {
        v.pause();
        v.dataset.resume = "1";
      }
    } else if (v.dataset.resume) {
      delete v.dataset.resume;
      v.play().catch(() => {});
    }
  });
});

// ---- Missing-file watcher (every few seconds) ----
let missingFiles = new Set();

async function pollMissingFiles() {
  if (!prompts.some((p) => p.file_path || p.icon_path)) {
    missingFiles.clear();
    return;
  }
  try {
    missingFiles = new Set(await invoke("missing_files"));
    document.querySelectorAll(".tile").forEach((tile) => {
      tile.classList.toggle("file-missing", missingFiles.has(tile.dataset.id));
    });
  } catch {}
}

// ---- Pointer-based drag to any cell ----
function cellAt(x, y) {
  return document.elementFromPoint(x, y)?.closest(".cell") || null;
}

// Track the hovered cell instead of querying all cells on every mousemove.
let hoverCell = null;

function setHoverCell(cell) {
  if (cell === hoverCell) return;
  hoverCell?.classList.remove("drag-over");
  hoverCell = cell;
  hoverCell?.classList.add("drag-over");
}

function endDragVisuals() {
  document.body.classList.remove("drag-active");
  setHoverCell(null);
}

window.addEventListener("pointermove", (e) => {
  if (!drag) return;
  if (!drag.moved) {
    if (Math.abs(e.clientX - drag.startX) < DRAG_THRESHOLD &&
        Math.abs(e.clientY - drag.startY) < DRAG_THRESHOLD) return;
    drag.moved = true;
    drag.el.classList.add("dragging");
    document.body.classList.add("drag-active");
    // Dragging out of the library: hide the overlay so the grid is visible.
    if (drag.fromLibrary) libraryEl.classList.add("hidden");
    // Live ghost: a clone of the tile follows the cursor.
    const r = drag.el.getBoundingClientRect();
    drag.offX = drag.startX - r.left;
    drag.offY = drag.startY - r.top;
    drag.ghost = drag.el.cloneNode(true);
    drag.ghost.classList.add("drag-ghost");
    drag.ghost.classList.remove("dragging");
    drag.ghost.style.width = `${r.width}px`;
    drag.ghost.style.height = `${r.height}px`;
    // A cloned <video> would autoplay and decode while following the cursor —
    // that, not the move itself, makes video-tile drags stutter.
    drag.ghost.querySelectorAll("video").forEach((v) => {
      v.removeAttribute("autoplay");
      v.removeAttribute("src");
    });
    // Pause the original too for the duration of the drag.
    drag.el.querySelectorAll("video").forEach((v) => {
      if (!v.paused) {
        v.pause();
        v.dataset.resume = "1";
      }
    });
    document.body.appendChild(drag.ghost);
  }
  drag.ghost.style.transform =
    `translate(${e.clientX - drag.offX}px, ${e.clientY - drag.offY}px)`;
  setHoverCell(cellAt(e.clientX, e.clientY));
});

// Resume videos the drag paused (visibility pauses use the same marker).
function resumeDragVideos(el) {
  el.querySelectorAll("video").forEach((v) => {
    if (v.dataset.resume) {
      delete v.dataset.resume;
      v.play().catch(() => {});
    }
  });
}

window.addEventListener("pointerup", async (e) => {
  if (!drag) return;
  const { id, moved, el, ghost } = drag;
  drag = null;
  ghost?.remove();
  el.classList.remove("dragging");
  resumeDragVideos(el);
  endDragVisuals();

  if (!moved) {
    if (el.classList.contains("tile")) {
      const p = prompts.find((x) => x.id === id);
      if (p && flag("promptVars") && !p.copy_image && !p.file_path && extractVars(p.text).length) {
        await copyTextWithVars(p, el);
      } else if (await invoke("copy_prompt", { id }).catch((e) => { toast(String(e)); return false; })) {
        showCopied(el);
        recordCopy(id);
      }
    }
    return;
  }
  const cell = cellAt(e.clientX, e.clientY);
  if (!cell) return;
  await placeTile(id, Number(cell.dataset.col), Number(cell.dataset.row));
});

window.addEventListener("pointercancel", () => {
  if (drag) {
    drag.el.classList.remove("dragging");
    drag.ghost?.remove();
    resumeDragVideos(drag.el);
  }
  drag = null;
  endDragVisuals();
});

// Apply the expert copy-feedback font + size to a "Copied!" element. copySize 0
// (default) auto-fits the text to the element's button; otherwise a fixed px.
function styleCopyText(el, maxW, maxH, cap) {
  el.style.fontFamily = FONTS[txt("copyFont")] || "";
  const cs = Number(settings.ui_values?.copySize) || 0;
  if (cs > 0) el.style.fontSize = `${cs}px`;
  else fitText(el, maxW, maxH, cap);
}

// Flash + small "Copied!" bubble at the bottom of the tile.
function showCopied(tile) {
  tile.classList.add("copied");
  setTimeout(() => tile.classList.remove("copied"), 350);
  if (!flag("copyBubble")) return; // border flash stays; bubble is optional
  const pop = document.createElement("div");
  pop.className = "copy-pop";
  pop.textContent = t("copied");
  tile.appendChild(pop);
  styleCopyText(pop, tile.clientWidth * 0.8, tile.clientHeight * 0.45, 26);
  setTimeout(() => pop.remove(), BUBBLE_MS);
}

// Pure layout move: the tile element is re-parented as-is — its fitted text
// size cannot change and a playing video keeps running. Falls back to a full
// render when the tile isn't in the grid yet (placed from the library).
function moveTileDom(id, col, row) {
  const tile = gridEl.querySelector(`.tile[data-id="${CSS.escape(id)}"]`);
  const target = gridEl.querySelector(`.cell[data-col="${col}"][data-row="${row}"]`);
  if (!tile || !target) return false;
  const source = tile.parentElement;
  if (source === target) return true;
  const occupant = target.firstElementChild;
  if (occupant) source.appendChild(occupant); // swap
  target.appendChild(tile);
  return true;
}

// Place a tile at [col,row]; swaps with the occupant. Renders from local
// state immediately, persistence runs in the background.
async function placeTile(id, col, row) {
  const view = activeView();
  const layout = { ...layoutOf(view) };
  const old = layout[id];
  const occupant = Object.entries(layout).find(
    ([oid, c]) => oid !== id && c[0] === col && c[1] === row
  );
  if (occupant) {
    if (old) layout[occupant[0]] = old;
    else delete layout[occupant[0]];
  }
  layout[id] = [col, row];
  view.layouts[gridKeyOf(view)] = layout;
  invoke("set_layout", { layout }).catch((e) => toast(String(e)));
  if (!moveTileDom(id, col, row)) await renderGrid(true);
}

// ---- Context menu ----
function openCtx(id, x, y) {
  ctxId = id;
  // Reset an armed delete confirmation from a previous open.
  disarmButton(ctxEl.querySelector('[data-act="delete"]'), t("delete"));
  // Hide the floating-button action when that feature is switched off.
  ctxEl.querySelector('[data-act="pin"]').classList.toggle("hidden", !flag("floating"));
  ctxEl.classList.remove("hidden");
  const w = ctxEl.offsetWidth, h = ctxEl.offsetHeight;
  ctxEl.style.left = `${Math.min(x, window.innerWidth - w - 4)}px`;
  ctxEl.style.top = `${Math.min(y, window.innerHeight - h - 4)}px`;
}
function closeCtx() {
  ctxEl.classList.add("hidden");
  ctxId = null;
}

// ---- Modal ----
// Reusable palette: "no color" + free picker + presets, into any container.
// onPick(hex) for none/preset; onCustom(anchor, current) opens the free picker.
function buildSwatches(container, selected, onPick, onCustom) {
  container.innerHTML = "";
  const isCustom = !!selected && !COLORS.includes(selected);

  const mkSwatch = (cls, bg) => {
    const sw = document.createElement("button");
    sw.type = "button";
    sw.className = `swatch ${cls}`.trim();
    if (bg) sw.style.background = bg;
    container.appendChild(sw);
    return sw;
  };

  const none = mkSwatch("none" + (selected === "" ? " sel" : ""));
  none.addEventListener("click", () => onPick(""));

  const custom = mkSwatch("custom" + (isCustom ? " sel" : ""), isCustom ? selected : "");
  custom.addEventListener("click", () => onCustom(custom, isCustom ? selected : ""));

  for (const c of COLORS.slice(1)) {
    const sw = mkSwatch(c === selected ? "sel" : "", c);
    sw.addEventListener("click", () => onPick(c));
  }
}

function renderSwatches(selected) {
  buildSwatches(
    $("color-row"),
    selected,
    (hex) => { modalState.color = hex; renderSwatches(hex); },
    (anchor, cur) => openColorPop(anchor, cur, (hex) => {
      if (!modalState) return;
      modalState.color = hex;
      renderSwatches(hex);
    })
  );
}

// Live caption overlay on the modal media preview — mirrors the on-tile render
// (0 = default size, 1 = auto-scale, else fixed px) so the user sees it as-is.
function updateCaptionPreview() {
  const el = modal.captionPreview;
  if (!el) return;
  const text = modal.caption.value.trim();
  const size = Number(modal.captionSize.value) || 0;
  el.textContent = text;
  el.classList.toggle("hidden", !text);
  el.classList.toggle("auto", size === 1);
  el.style.fontSize = size > 1 ? `${size}px` : "";
}

// Serialized content of the open modal — compared on dismiss to detect edits.
function modalSnapshot() {
  if (!modalState) return "";
  return JSON.stringify([
    modal.name.value, modal.text.value, modal.caption.value, modal.captionSize.value,
    modal.fontSel.value, modal.sizeSel.value, modalState.color || "", modalState.image || "",
    !!modalState.showImage, modalState.filePath || "", modalState.iconPath || "",
  ]);
}

// ---- Prompt variables ({{placeholder}}) ----
const VAR_RE = /\{\{\s*([^}\n]{1,60}?)\s*\}\}/g;
function extractVars(text) {
  const seen = [];
  for (const m of (text || "").matchAll(VAR_RE)) {
    const k = m[1].trim();
    if (k && !seen.includes(k)) seen.push(k);
  }
  return seen;
}
const fillVars = (text, values) => text.replace(VAR_RE, (_, k) => values[k.trim()] ?? "");

// Fill-in dialog shown when copying a prompt that holds {{placeholders}}.
// Resolves a {name: value} map, or null when cancelled.
let varsCleanup = null;
function promptVarsDialog(vars) {
  const root = $("vars-modal");
  $("vars-title").textContent = t("varsTitle");
  const fields = $("vars-fields");
  fields.innerHTML = "";
  const inputs = vars.map((name) => {
    const label = document.createElement("label");
    label.className = "field";
    const span = document.createElement("span");
    span.textContent = name;
    const input = document.createElement("input");
    input.className = "modal-input";
    input.type = "text";
    input.dataset.var = name;
    label.append(span, input);
    fields.appendChild(label);
    return input;
  });
  $("vars-ok").textContent = t("varsCopy");
  $("vars-cancel").textContent = t("cancel");
  varsCleanup?.(null);
  root.classList.remove("hidden");
  inputs[0]?.focus();
  return new Promise((resolve) => {
    const ok = $("vars-ok");
    const cancel = $("vars-cancel");
    const collect = () => Object.fromEntries(inputs.map((i) => [i.dataset.var, i.value]));
    const done = (val) => {
      root.classList.add("hidden");
      ok.removeEventListener("click", onOk);
      cancel.removeEventListener("click", onCancel);
      root.removeEventListener("pointerdown", onBg);
      document.removeEventListener("keydown", onKey);
      varsCleanup = null;
      resolve(val);
    };
    const onOk = () => done(collect());
    const onCancel = () => done(null);
    const onBg = (e) => { if (e.target === root) done(null); };
    const onKey = (e) => {
      if (e.key === "Escape") done(null);
      else if (e.key === "Enter") done(collect());
    };
    varsCleanup = done;
    ok.addEventListener("click", onOk);
    cancel.addEventListener("click", onCancel);
    root.addEventListener("pointerdown", onBg);
    document.addEventListener("keydown", onKey);
  });
}

// Record a copy for the history/usage journal (backend honours the privacy flag).
const recordCopy = (id) => { invoke("record_copy", { id }).catch(() => {}); };

// Copy a text prompt after filling its placeholders.
async function copyTextWithVars(p, el) {
  const values = await promptVarsDialog(extractVars(p.text));
  if (!values) return;
  const ok = await invoke("copy_text", { text: fillVars(p.text, values) })
    .catch((e) => { toast(String(e)); return false; });
  if (ok) { showCopied(el); recordCopy(p.id); }
}

function openModal({ mode, id, name = "", text = "", color = "", image = "", showImage = false, copyImage = false, filePath = "", iconPath = "", caption = "", captionSize = 0, font = "", fontSize = 0, title }) {
  modalState = { mode, id, color, image, showImage, copyImage, filePath, iconPath };
  modal.title.textContent = title;
  modal.name.value = name;
  modal.text.value = text;
  modal.caption.value = caption;
  modal.captionSize.value = String(normSize(captionSize) || 0);
  updateCaptionPreview();
  // Per-tile style overrides, available when creating and editing.
  modal.fontSel.value = font;
  modal.sizeSel.value = String(normSize(fontSize));
  modal.delete.classList.toggle("hidden", mode !== "edit");
  disarmButton(modal.delete, t("delete"));
  renderSwatches(color);
  // Show the dialog before wiring the preview so the <video>/<img> decode and
  // lay out in a visible context (a hidden element never renders a first frame).
  modal.root.classList.remove("hidden");
  syncModalImageUi(mode);
  modalInitial = modalSnapshot();
  modal.name.focus();
  modal.name.select();
}

// Keep all image/file modal controls consistent with modalState.
function syncModalImageUi(mode) {
  const { image, showImage, copyImage, filePath, iconPath } = modalState;
  const kind = filePath ? mediaKind(filePath) : "";
  const iconKind = !image && iconPath ? mediaKind(iconPath) : "";
  // Any media file previews straight from its path (image/gif/video) — so an
  // image still shows even when the backend preview re-encode returned nothing.
  const fileMedia = !image && !iconKind && !!kind;
  const hasPreview = !!image || !!iconKind || fileMedia;
  // Image and file prompts have no text field — the name doubles as the copy text.
  // Plain text prompts expose it on create (review before saving) and edit.
  const textVisible = (mode === "edit" || mode === "create") && !copyImage && !filePath;
  // Full-height dialog only when editing a text prompt (max textarea space).
  modal.root.classList.toggle("tall", textVisible);
  modal.text.classList.toggle("hidden", !textVisible);
  modal.textBar.classList.toggle("hidden", !(textVisible && flag("promptVars")));
  modal.varsHint.classList.toggle("hidden", !(textVisible && flag("promptVars")));
  modal.name.placeholder = filePath
    ? t(kind === "video" ? "videoNamePh" : kind ? "imageNamePh" : "fileNamePh")
    : copyImage ? t("imageNamePh") : t("namePh");
  modal.imgWrap.classList.toggle("hidden", !hasPreview);
  modal.addIcon.classList.toggle("hidden", hasPreview);
  // "Replace media" always available next to "Remove media"; only the image
  // of a clipboard-image prompt cannot be removed (it IS the prompt).
  modal.removeImg.classList.toggle("hidden", copyImage);
  if (hasPreview) {
    const previewPath = iconKind ? iconPath : filePath;
    const isVideo = !image && mediaKind(previewPath) === "video";
    modal.img.classList.toggle("hidden", isVideo);
    modal.video.classList.toggle("hidden", !isVideo);
    if (isVideo) {
      modal.video.src = convertFileSrc(previewPath);
      modal.video.load();
      modal.video.play().catch(() => {}); // show a frame even if autoplay is held
    } else {
      // Prefer the re-encoded preview; fall back to the raw file, and if even
      // that fails to decode, ask the backend for a data URL once.
      modal.img.onerror = () => {
        modal.img.onerror = null;
        loadFilePreview(previewPath).then((d) => { if (d) modal.img.src = d; });
      };
      modal.img.src = image || convertFileSrc(previewPath);
      modal.video.removeAttribute("src");
    }
    modal.showImage.classList.toggle("active", showImage);
    modal.showText.classList.toggle("active", !showImage);
  } else {
    modal.video.removeAttribute("src");
  }
  modal.fileWrap.classList.toggle("hidden", !filePath);
  $("modal-file-hint").classList.toggle("hidden", !filePath);
  if (filePath) {
    $("modal-file-icon").innerHTML = PDF_EXT.test(filePath) ? ICON_PDF : ICON_FILE;
    modal.fileName.textContent = filePath.split(/[\\/]/).pop();
    modal.fileName.title = filePath;
  }
}

// One native dialog at a time: ignore further requests until it is closed
// (the dialog is also window-modal on the backend side).
let dialogBusy = false;
async function withDialog(fn) {
  if (dialogBusy) return null;
  dialogBusy = true;
  try {
    return await fn();
  } finally {
    dialogBusy = false;
  }
}

const PDF_EXT = /\.pdf$/i;
async function loadFilePreview(path) {
  // A PDF gets its first page rendered as the preview image.
  if (PDF_EXT.test(path)) {
    return (await invoke("pdf_preview", { path }).catch(() => "")) || "";
  }
  if (!IMAGE_EXT.test(path)) return "";
  return (await invoke("load_image_file", { path }).catch(() => "")) || "";
}

// Build a file-attach prompt from a path. Image, gif, video and PDF
// attachments behave like media prompts; a PDF shows its rendered first page
// but still copies the file on click.
async function openFileCreate(path) {
  const kind = mediaKind(path);
  const isPdf = PDF_EXT.test(path);
  const image = await loadFilePreview(path);
  openModal({
    mode: "file-create",
    title: t(kind === "video" ? "videoModalTitle" : kind || isPdf ? "imageModalTitle" : "fileModalTitle"),
    name: path.split(/[\\/]/).pop(),
    filePath: path,
    image,
    showImage: !!kind || (isPdf && !!image),
  });
}

// Left click: open the file dialog directly.
async function startFileCreate() {
  await withDialog(async () => {
    const path = await invoke("pick_file_path");
    if (path) await openFileCreate(path);
  });
}

// Right click: take a file from the clipboard (not text) — a copied file, or a
// copied image (screenshot) kept as a new file.
async function startFileFromClipboard() {
  await withDialog(async () => {
    const path = await invoke("get_clipboard_file_path").catch(() => "");
    if (path) { await openFileCreate(path); return; }
    const clipImg = await invoke("get_clipboard_image").catch(() => "");
    if (clipImg) {
      openModal({ mode: "image-create", title: t("imageModalTitle"), name: t("filterImage"), image: clipImg, showImage: true, copyImage: true });
      return;
    }
    toast(t("noClipboardFile"));
  });
}
function closeModal() {
  closeColorPop();
  modal.root.classList.add("hidden");
  modal.video.removeAttribute("src"); // stop a playing preview
  modalState = null;
  modalInitial = "";
}

// Themed yes/no dialog. Resolves true on confirm, false on cancel / dismiss.
let confirmCleanup = null;
function confirmDialog({ title, message, confirmLabel, cancelLabel }) {
  const root = $("confirm-modal");
  $("confirm-title").textContent = title;
  $("confirm-msg").textContent = message;
  const ok = $("confirm-ok");
  const cancel = $("confirm-cancel");
  ok.textContent = confirmLabel;
  cancel.textContent = cancelLabel || t("cancel");
  confirmCleanup?.(false); // resolve any stale dialog first
  root.classList.remove("hidden");
  return new Promise((resolve) => {
    const done = (val) => {
      root.classList.add("hidden");
      ok.removeEventListener("click", onOk);
      cancel.removeEventListener("click", onCancel);
      root.removeEventListener("pointerdown", onBg);
      confirmCleanup = null;
      resolve(val);
    };
    const onOk = () => done(true);
    const onCancel = () => done(false);
    const onBg = (e) => { if (e.target === root) done(false); };
    ok.addEventListener("click", onOk);
    cancel.addEventListener("click", onCancel);
    root.addEventListener("pointerdown", onBg);
    confirmCleanup = done;
    ok.focus();
  });
}

// Ask before dismissing the modal via the background. A new save dialog (text,
// image, file, screenshot) always holds unsaved work; an edit only when changed.
async function confirmDiscardIfDirty() {
  if (!flag("confirmDiscard")) return true;
  const isNew = modalState && modalState.mode !== "edit";
  if (!isNew && modalSnapshot() === modalInitial) return true;
  return confirmDialog({
    title: t("discardTitle"),
    message: t("discardMsg"),
    confirmLabel: t("discardConfirm"),
    cancelLabel: t("keepEditing"),
  });
}

function startCreate() {
  const text = inputEl.value.trim();
  if (!text) return;
  // Carry the composed text into the dialog so it can be reviewed/edited (and
  // previewed as Markdown) before saving.
  openModal({ mode: "create", text, title: t("nameModalTitle") });
}

async function confirmModal() {
  if (!modalState) return;
  const name = modal.name.value.trim();
  if (!name) { modal.name.focus(); return; }

  const color = modalState.color || "";
  const image = modalState.image || "";
  const filePath = modalState.filePath || "";
  const iconPath = modalState.iconPath || "";
  const caption = modal.caption.value.trim();
  const captionSize = Number(modal.captionSize.value) || 0;
  const font = modal.fontSel.value;
  const fontSize = Number(modal.sizeSel.value) || 0;
  // NOTE: Tauri expects camelCase keys for snake_case Rust args.
  // Path media (gif/video icon or attachment) shows without a stored image.
  const showImage =
    image || mediaKind(filePath) || mediaKind(iconPath) ? modalState.showImage : false;
  const copyImage = image ? modalState.copyImage : false;
  try {
    if (modalState.mode === "create") {
      const text = modal.text.value.trim();
      if (!text) { closeModal(); return; }
      await invoke("add_prompt", { name, text, color, image, showImage, copyImage, filePath, iconPath, caption, captionSize, font, fontSize });
      inputEl.value = "";
      autoGrow(inputEl);
      saveBtn.disabled = true;
    } else if (modalState.mode === "image-create" || modalState.mode === "file-create") {
      // The name doubles as the copy text when shown as text.
      await invoke("add_prompt", { name, text: name, color, image, showImage, copyImage, filePath, iconPath, caption, captionSize, font, fontSize });
    } else {
      const text = copyImage || filePath ? name : modal.text.value;
      await invoke("update_prompt", { id: modalState.id, name, text, color, image, showImage, copyImage, filePath, iconPath, caption, captionSize, font, fontSize });
    }
  } catch (err) {
    toast(String(err)); // keep the modal open so nothing typed is lost
    return;
  }
  closeModal();
  await renderGrid();
  pollMissingFiles(); // a replaced file clears the error immediately
  if (!libraryEl.classList.contains("hidden")) renderLibrary();
}

async function editPrompt(id) {
  const p = await invoke("get_prompt", { id });
  if (p) {
    const kind = p.file_path ? mediaKind(p.file_path) : "";
    openModal({
      mode: "edit",
      id,
      name: p.name,
      text: p.text,
      color: p.color || "",
      image: p.image || "",
      showImage: p.show_image || false,
      copyImage: p.copy_image || false,
      filePath: p.file_path || "",
      iconPath: p.icon_path || "",
      caption: p.caption || "",
      captionSize: p.caption_size || 0,
      font: p.font || "",
      fontSize: p.font_size || 0,
      title: p.file_path
        ? t(kind === "video" ? "videoEditTitle" : kind ? "imageEditTitle" : "fileEditTitle")
        : p.copy_image ? t("imageEditTitle") : t("editModalTitle"),
    });
  }
}

// ---- Prompt library (all prompts, click to edit) ----
// Classify a prompt for the library type filter / thumbnails.
function promptType(p) {
  if (PDF_EXT.test(p.file_path)) return "pdf";
  const kind = p.file_path ? mediaKind(p.file_path) : p.icon_path ? mediaKind(p.icon_path) : "";
  if (kind === "video") return "video";
  if (kind === "image" || kind === "gif") return "image";
  if (p.copy_image && p.image) return "image";
  if (p.file_path) return "file";
  return "text";
}

// Library display name: fall back to a type label ("Bild" etc.) so saved
// images/screenshots/files are never shown blank.
const LIB_TYPE_LABEL = { image: "filterImage", video: "filterVideo", pdf: "filterPdf", file: "filterFile" };
function libLabel(p) {
  if (p.name) return p.name;
  const key = LIB_TYPE_LABEL[promptType(p)];
  return key ? t(key) : p.text;
}

// Best preview source for a prompt's library thumbnail; "" = no image.
function thumbSrc(p) {
  if (p.image) return p.image;
  const path = mediaKind(p.file_path) === "image" || mediaKind(p.file_path) === "gif"
    ? p.file_path
    : mediaKind(p.icon_path) === "image" || mediaKind(p.icon_path) === "gif"
      ? p.icon_path
      : "";
  return path ? convertFileSrc(path) : "";
}

function setLibType(type) {
  libType = type;
  for (const b of $("library-types").querySelectorAll(".lib-type")) {
    b.classList.toggle("active", b.dataset.type === type);
  }
}

function libMatches(p) {
  if (libType !== "all" && promptType(p) !== libType) return false;
  if (libColor !== "all" && (p.color || "") !== libColor) return false;
  const q = libQuery.trim().toLowerCase();
  if (!q) return true;
  return [p.name, p.text, p.file_path].some((s) => (s || "").toLowerCase().includes(q));
}

// Build the library color-filter dots (once). "All" + every palette color.
function buildLibColors() {
  const wrap = $("library-colors");
  if (!wrap || wrap.childElementCount) return;
  const mk = (color, cls) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = `lib-color ${cls}`.trim();
    b.dataset.color = color;
    if (color && color !== "all") b.style.background = color;
    if (color === libColor) b.classList.add("active");
    return b;
  };
  wrap.appendChild(mk("all", "lib-color-all"));
  for (const c of COLORS) if (c) wrap.appendChild(mk(c, ""));
}
function setLibColor(color) {
  libColor = color;
  for (const b of $("library-colors").querySelectorAll(".lib-color")) {
    b.classList.toggle("active", b.dataset.color === color);
  }
}

function renderLibrary() {
  const list = $("library-list");
  list.innerHTML = "";
  $("library-search").classList.toggle("hidden", !prompts.length || !flag("librarySearch"));
  if (!prompts.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("libraryEmpty");
    list.appendChild(empty);
    return;
  }
  const items = flag("librarySearch") ? prompts.filter(libMatches) : prompts;
  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("libraryNoResults");
    list.appendChild(empty);
    return;
  }
  const placed = new Set(Object.keys(layoutOf(activeView())));
  for (const p of items) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "lib-item";
    row.title = t("copy");

    const body = document.createElement("span");
    body.className = "lib-body";
    const name = document.createElement("span");
    name.className = "lib-name";
    name.textContent = libLabel(p);
    const text = document.createElement("span");
    text.className = "lib-text";
    text.textContent = p.file_path || p.text;
    body.append(name, text);

    // Image/screenshot/PDF prompts get a thumbnail preview, others a color dot.
    const thumb = flag("imagePreview") ? thumbSrc(p) : "";
    if (thumb) {
      const img = document.createElement("img");
      img.className = "lib-thumb";
      img.src = thumb;
      img.draggable = false;
      row.append(img, body);
    } else {
      const dot = document.createElement("span");
      dot.className = "dot";
      if (p.color) dot.style.background = p.color;
      row.append(dot, body);
    }

    // Place on the current layout: drag the row onto the grid, or one click.
    row.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || e.target.closest(".lib-add, .lib-edit")) return;
      drag = { id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: row, fromLibrary: true };
    });
    // Edit icon (right): opens the edit dialog without copying.
    const edit = document.createElement("span");
    edit.className = "icon-btn lib-edit";
    edit.title = t("edit");
    edit.innerHTML = ICON_EDIT;
    edit.addEventListener("pointerdown", (e) => e.stopPropagation());
    edit.addEventListener("click", (e) => { e.stopPropagation(); editPrompt(p.id); });
    row.appendChild(edit);
    if (!placed.has(p.id)) {
      const add = document.createElement("span");
      add.className = "icon-btn lib-add";
      add.title = t("addToLayout");
      add.innerHTML = GRID_PLUS;
      add.addEventListener("pointerdown", (e) => e.stopPropagation());
      add.addEventListener("click", async (e) => {
        e.stopPropagation();
        const view = activeView();
        const occupied = new Map(
          Object.entries(layoutOf(view)).map(([id, c]) => [cellKey(...c), id])
        );
        const free = firstFree(occupied, view.cols, view.rows);
        if (!free) { toast(t("gridFull")); return; }
        await placeTile(p.id, free[0], free[1]);
        renderLibrary();
      });
      row.appendChild(add);
    }

    // Click copies the prompt; optionally closes the library afterwards.
    row.addEventListener("click", () => libraryCopy(p));
    list.appendChild(row);
  }
}

// Copy a prompt from the library (handles {{variables}}), then optionally close.
async function libraryCopy(p) {
  const useVars = flag("promptVars") && !p.copy_image && !p.file_path && extractVars(p.text).length;
  let ok;
  if (useVars) {
    const values = await promptVarsDialog(extractVars(p.text));
    if (!values) return;
    ok = await invoke("copy_text", { text: fillVars(p.text, values) }).catch(() => false);
  } else {
    ok = await invoke("copy_prompt", { id: p.id }).catch((e) => { toast(String(e)); return false; });
  }
  if (!ok) return;
  recordCopy(p.id);
  toast(t("copied"));
  if (flag("closeAfterCopy")) libraryEl.classList.add("hidden");
}

// Reflect the close-after-copy flag on the library header toggle.
function syncLibraryToggle() {
  const btn = $("library-close-toggle");
  const on = flag("closeAfterCopy");
  btn.classList.toggle("active", on);
  btn.setAttribute("aria-pressed", String(on));
  btn.classList.toggle("hidden", !flag("libraryCloseToggle"));
}

// ---- Copy history & usage journal ----
function journalRow(p, subtitle, onClick) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "lib-item";
  row.title = t("copy");
  const body = document.createElement("span");
  body.className = "lib-body";
  const name = document.createElement("span");
  name.className = "lib-name";
  name.textContent = libLabel(p);
  const text = document.createElement("span");
  text.className = "lib-text";
  text.textContent = subtitle;
  body.append(name, text);
  const thumb = flag("imagePreview") ? thumbSrc(p) : "";
  if (thumb) {
    const img = document.createElement("img");
    img.className = "lib-thumb";
    img.src = thumb;
    img.draggable = false;
    row.append(img, body);
  } else {
    const dot = document.createElement("span");
    dot.className = "dot";
    if (p.color) dot.style.background = p.color;
    row.append(dot, body);
  }
  row.addEventListener("click", onClick);
  return row;
}

// Format a unix-seconds copy timestamp for the history column.
const fmtCopyTime = (ts) => {
  if (!ts) return "";
  return new Date(ts * 1000).toLocaleString(LANG, { dateStyle: "short", timeStyle: "short" });
};

function renderJournal() {
  const body = $("journal-body");
  body.innerHTML = "";
  const byId = new Map(prompts.map((p) => [p.id, p]));
  const recent = (settings.copy_log || [])
    .map((e) => [byId.get(e.id), e.ts])
    .filter(([p]) => p);
  const used = Object.entries(settings.usage || {})
    .map(([id, n]) => [byId.get(id), n])
    .filter(([p]) => p)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 20);
  if (!recent.length && !used.length) {
    const empty = document.createElement("div");
    empty.className = "hint journal-empty";
    empty.textContent = t("journalEmpty");
    body.appendChild(empty);
    return;
  }
  const copyAgain = (p) => async () => {
    if (await invoke("copy_prompt", { id: p.id }).catch((e) => { toast(String(e)); return false; })) {
      recordCopy(p.id);
      toast(t("copied"));
      settings = await invoke("get_settings");
      renderJournal();
    }
  };
  // Two columns: left = recently copied (with timestamp), right = most used.
  const column = (titleKey, rows) => {
    const col = document.createElement("div");
    col.className = "journal-col";
    const head = document.createElement("div");
    head.className = "lib-section";
    head.textContent = t(titleKey);
    col.appendChild(head);
    if (!rows.length) {
      const e = document.createElement("div");
      e.className = "hint";
      e.textContent = t("journalEmpty");
      col.appendChild(e);
    } else {
      for (const [p, sub] of rows) col.appendChild(journalRow(p, sub, copyAgain(p)));
    }
    return col;
  };
  body.append(
    column("recentlyCopied", recent.map(([p, ts]) => [p, fmtCopyTime(ts)])),
    column("mostUsed", used.map(([p, n]) => [p, t("usedTimes").replace("{n}", n)])),
  );
}

async function openJournal() {
  settings = await invoke("get_settings");
  renderJournal();
  $("journal").classList.remove("hidden");
}


// ---- Settings: views editor ----
function renderViewsEditor() {
  const editor = $("views-editor");
  editor.innerHTML = "";
  for (const v of settings.views) {
    const row = document.createElement("div");
    row.className = "view-row";

    const input = document.createElement("input");
    input.className = "modal-input";
    input.type = "text";
    input.maxLength = 30;
    input.value = v.name;
    input.placeholder = t("viewNamePh");
    input.addEventListener("change", async () => {
      settings = await invoke("rename_view", { id: v.id, name: input.value });
      renderViews();
      renderViewsEditor();
    });
    row.appendChild(input);

    // Per-view grid size (columns × rows), applied on change.
    const grid = document.createElement("span");
    grid.className = "view-grid";
    const mkNum = (value) => {
      const n = document.createElement("input");
      n.className = "grid-mini";
      n.type = "number";
      n.min = 1;
      n.max = val("gridMax");
      n.value = value;
      return n;
    };
    const colsIn = mkNum(v.cols);
    const rowsIn = mkNum(v.rows);
    const applyGrid = async () => {
      const cols = clampGrid(colsIn.value, v.cols);
      const rows = clampGrid(rowsIn.value, v.rows);
      settings = await invoke("set_view_grid", { id: v.id, cols, rows });
      renderViewsEditor();
      if (v.id === settings.active_view) await renderGrid(true);
    };
    attachGridPicker(colsIn, applyGrid);
    attachGridPicker(rowsIn, applyGrid);
    const times = document.createElement("span");
    times.className = "times";
    times.textContent = "×";
    grid.append(colsIn, times, rowsIn);
    row.appendChild(grid);

    // Per-view tab color: a swatch dot opening the preset palette.
    const colorDot = document.createElement("button");
    colorDot.type = "button";
    colorDot.className = "swatch view-color-dot" + (v.color ? "" : " none");
    if (v.color) colorDot.style.background = v.color;
    colorDot.title = t("viewColor");
    colorDot.addEventListener("click", () => {
      openSwatchPop(colorDot, v.color || "", async (hex) => {
        settings = await invoke("set_view_color", { id: v.id, color: hex });
        renderViews();
        renderViewsEditor();
      });
    });
    row.appendChild(colorDot);

    if (settings.views.length > 1) {
      const del = document.createElement("button");
      del.className = "icon-btn";
      del.innerHTML = CROSS;
      del.title = t("delete");
      // Two-step confirm: first click arms (red), second click deletes.
      del.addEventListener("click", async () => {
        if (!del.classList.contains("confirm")) {
          del.classList.add("confirm");
          del.title = `${t("delete")}?`;
          setTimeout(() => {
            del.classList.remove("confirm");
            del.title = t("delete");
          }, 3000);
          return;
        }
        try {
          settings = await invoke("delete_view", { id: v.id });
          renderViewsEditor();
          await renderGrid(true);
        } catch (err) {
          toast(String(err));
        }
      });
      row.appendChild(del);
    }
    editor.appendChild(row);
  }
  // At the limit the add button disappears entirely.
  $("view-add").classList.toggle("hidden", settings.views.length >= val("maxViews"));
}

// ---- Settings actions ----
async function runExport(format) {
  try {
    await withDialog(() => invoke("export_prompts", { format }));
  } catch (err) {
    if (String(err) !== "canceled") toast(`${t("exportFailed")}: ${err}`);
  }
}

async function runImport() {
  try {
    const count = await withDialog(() => invoke("import_prompts"));
    if (count == null) return;
    await renderGrid(); // refreshes prompts AND settings
    // Imported preferences apply on the spot — no restart needed.
    LANG = resolveLang(settings.language);
    applyI18n();
    fillSizeSelects();
    fillFontSelects();
    $("lang-select").value = settings.language || "auto";
    $("theme-select").value = settings.theme || "system";
    $("tile-font").value = settings.tile_font || "system";
    $("tile-size").value = String(normSize(Number(settings.tile_size ?? 0)));
    $("opt-minimize").checked = settings.minimize_to_tray === true;
    $("opt-autoupdate").checked = settings.auto_update !== false;
    applyTheme(await invoke("current_theme"));
    applyTileStyle();
    applyBars();
    await renderGrid(true); // re-render with the new tile style
    renderViewsEditor();
    toast(`${count} ${t("imported")}`);
  } catch (err) {
    if (String(err) !== "canceled") toast(`${t("importFailed")}: ${err}`);
  }
}

async function deleteAll() {
  const btn = $("delete-all");
  if (!armButton(btn, t("deleteAllConfirm"))) {
    clearTimeout(deleteAllTimer);
    deleteAllTimer = setTimeout(() => disarmButton(btn, t("deleteAll")), 3000);
    return;
  }
  clearTimeout(deleteAllTimer);
  disarmButton(btn, t("deleteAll"));
  await invoke("delete_all_data");
  location.reload(); // full re-init: default theme, views, toggles, grid
}

// ---- Wire events ----
function bind() {
  inputEl.addEventListener("input", () => {
    autoGrow(inputEl);
    saveBtn.disabled = !inputEl.value.trim();
  });
  inputEl.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.key === "Enter") { e.preventDefault(); startCreate(); }
  });
  // Paste an image or file (Ctrl+V) to save it as a button; plain text pastes
  // normally. A broken/unreadable image is reported, never saved.
  inputEl.addEventListener("paste", async (e) => {
    if (!flag("pasteMedia")) return;
    const dt = e.clipboardData;
    if (!dt) return;
    // Only intercept when the clipboard carries a file/image payload; plain text
    // keeps pasting normally.
    const hasPayload = dt.files?.length > 0 || [...(dt.items || [])].some((it) => it.kind === "file");
    if (!hasPayload) return;
    e.preventDefault();
    await withDialog(async () => {
      // Pasted bitmaps (screenshots) arrive as a file item with an empty type,
      // so try the image clipboard first, then fall back to a real file path.
      const img = await invoke("get_clipboard_image").catch(() => "");
      if (img) {
        openModal({ mode: "image-create", title: t("imageModalTitle"), name: t("filterImage"), image: img, showImage: true, copyImage: true });
        return;
      }
      const path = await invoke("get_clipboard_file_path").catch(() => "");
      if (path) await openFileCreate(path);
      else toast(t("imagePasteFailed"));
    });
  });
  saveBtn.addEventListener("click", startCreate);

  modal.confirm.addEventListener("click", confirmModal);
  modal.cancel.addEventListener("click", closeModal);
  modal.name.addEventListener("keydown", (e) => { if (e.key === "Enter") confirmModal(); });

  // Image / Text display toggle (text mode shows the name on the tile).
  modal.showImage.addEventListener("click", () => {
    if (!modalState) return;
    modalState.showImage = true;
    syncModalImageUi(modalState.mode);
  });
  modal.showText.addEventListener("click", () => {
    if (!modalState) return;
    modalState.showImage = false;
    syncModalImageUi(modalState.mode);
  });
  // Shared media pick: image, gif or video. Stills are stored as a preview,
  // gif/video become the icon path (shown, never copied). An image keeps the
  // prompt's copy behaviour; a gif/video icon switches copying back to text.
  async function pickReplacementMedia() {
    const path = await withDialog(() => invoke("pick_file_path"));
    if (!path || !modalState) return false;
    const kind = mediaKind(path);
    if (!kind) {
      toast(t("unsupportedFile"));
      return false;
    }
    if (kind === "image") {
      const data = await loadFilePreview(path);
      if (!data) return false;
      modalState.image = data;
      modalState.iconPath = "";
    } else {
      modalState.image = "";
      modalState.iconPath = path;
      modalState.copyImage = false;
    }
    modalState.showImage = true;
    return true;
  }
  // "Replace media" (preview exists) and "Media as icon" (no media yet).
  modal.replaceImg.addEventListener("click", async () => {
    if (modalState && (await pickReplacementMedia())) syncModalImageUi(modalState.mode);
  });
  modal.addIcon.addEventListener("click", async () => {
    if (modalState && (await pickReplacementMedia())) syncModalImageUi(modalState.mode);
  });
  modal.removeImg.addEventListener("click", () => {
    if (!modalState) return;
    modalState.image = "";
    modalState.iconPath = "";
    modalState.showImage = false;
    syncModalImageUi(modalState.mode);
  });

  // Replacing the attached file re-classifies the prompt by file type:
  // stills/gifs/videos switch to the media layout, anything else to plain file.
  modal.replaceFile.addEventListener("click", async () => {
    if (!modalState) return;
    const path = await withDialog(() => invoke("pick_file_path"));
    if (!path || !modalState) return;
    modalState.filePath = path;
    const kind = mediaKind(path);
    modalState.image = await loadFilePreview(path);
    modalState.copyImage = false; // the file itself is what gets copied
    modalState.showImage = !!kind;
    syncModalImageUi(modalState.mode);
  });

  // Paperclip button in the composer bar (files, images, gifs, videos).
  $("file-btn").addEventListener("click", startFileCreate);
  $("file-btn").addEventListener("contextmenu", (e) => {
    e.preventDefault();
    startFileFromClipboard();
  });

  // Snipping tool: open the overlay; the backend notifies us with the crop.
  $("snip-btn").addEventListener("click", () => {
    invoke("open_snip").catch((err) => toast(String(err)));
  });
  const snipModal = $("snip-modal");
  let pendingSnip = null;
  const closeSnipModal = () => {
    snipModal.classList.add("hidden");
    pendingSnip = null;
  };
  listen("snip-captured", (e) => {
    pendingSnip = e.payload?.data_url || "";
    if (!pendingSnip) return;
    $("snip-preview").src = pendingSnip;
    snipModal.classList.remove("hidden");
  });
  $("snip-modal-yes").addEventListener("click", () => {
    const image = pendingSnip;
    closeSnipModal();
    if (!image) return;
    // The screenshot becomes a button that copies the image on click.
    openModal({
      mode: "image-create",
      title: t("imageModalTitle"),
      name: t("snipTitle"),
      image,
      showImage: true,
      copyImage: true,
    });
  });
  $("snip-modal-no").addEventListener("click", closeSnipModal);
  // Discard this shot and immediately start a new one.
  $("snip-modal-retry").addEventListener("click", () => {
    closeSnipModal();
    invoke("open_snip").catch((err) => toast(String(err)));
  });
  $("snip-modal-close").addEventListener("click", closeSnipModal);
  snipModal.addEventListener("pointerdown", (e) => {
    if (e.target === snipModal) closeSnipModal();
  });

  // Delete from the edit dialog, with the same two-step confirmation.
  modal.delete.addEventListener("click", async () => {
    if (!modalState || modalState.mode !== "edit") return;
    if (!armButton(modal.delete, t("deleteConfirm"))) return;
    const id = modalState.id;
    closeModal();
    await invoke("delete_prompt", { id });
    await renderGrid();
    if (!libraryEl.classList.contains("hidden")) renderLibrary();
  });
  modal.root.addEventListener("pointerdown", async (e) => {
    if (e.target !== modal.root) return;
    if (await confirmDiscardIfDirty()) closeModal();
  });

  $("gear").addEventListener("click", () => {
    showExpertPage(false); // always open on the main page
    renderViewsEditor();
    settingsEl.classList.remove("hidden");
  });
  $("expert-open").addEventListener("click", () => showExpertPage(true));
  $("expert-back").addEventListener("click", () => showExpertPage(false));
  // Two-step confirm: first click arms it red, second within 3s resets.
  $("expert-reset").addEventListener("click", (e) => {
    const btn = e.currentTarget;
    if (!armButton(btn, t("expertResetConfirm"))) {
      clearTimeout(expertResetTimer);
      expertResetTimer = setTimeout(() => disarmButton(btn, t("expertReset")), 3000);
      return;
    }
    clearTimeout(expertResetTimer);
    disarmButton(btn, t("expertReset"));
    onResetExpert();
  });

  // Quick grid-size control (top-right of the layout, active view).
  const applyQuickGrid = async () => {
    const view = activeView();
    const cols = clampGrid($("qg-cols").value, view.cols);
    const rows = clampGrid($("qg-rows").value, view.rows);
    if (cols === view.cols && rows === view.rows) return;
    settings = await invoke("set_view_grid", { id: view.id, cols, rows });
    await renderGrid(true);
  };
  attachGridPicker($("qg-cols"), applyQuickGrid);
  attachGridPicker($("qg-rows"), applyQuickGrid);

  // Show/hide the top and bottom bars.
  $("hide-top").addEventListener("click", () => setBars(false, settings.show_composer !== false));
  $("show-top").addEventListener("click", () => setBars(true, settings.show_composer !== false));
  $("hide-bottom").addEventListener("click", () => setBars(settings.show_header !== false, false));
  $("show-bottom").addEventListener("click", () => setBars(settings.show_header !== false, true));

  buildLibColors();
  $("library-btn").addEventListener("click", () => {
    libQuery = "";
    $("library-q").value = "";
    setLibType("all");
    setLibColor("all");
    syncLibraryToggle();
    renderLibrary();
    libraryEl.classList.remove("hidden");
    if (flag("librarySearch")) $("library-q").focus();
  });
  // "Close after copy" toggle in the library header (mirrors the expert flag).
  $("library-close-toggle").addEventListener("click", async () => {
    const enabled = !flag("closeAfterCopy");
    settings.ui_flags = { ...(settings.ui_flags || {}), closeAfterCopy: enabled };
    try { await invoke("set_ui_flag", { key: "closeAfterCopy", enabled }); } catch (err) { toast(String(err)); }
    syncLibraryToggle();
  });
  $("library-q").addEventListener("input", (e) => { libQuery = e.target.value; renderLibrary(); });
  $("library-types").addEventListener("click", (e) => {
    const btn = e.target.closest(".lib-type");
    if (btn) { setLibType(btn.dataset.type); renderLibrary(); }
  });
  $("library-colors").addEventListener("click", (e) => {
    const btn = e.target.closest(".lib-color");
    if (btn) { setLibColor(btn.dataset.color); renderLibrary(); }
  });
  $("library-close").addEventListener("click", () => libraryEl.classList.add("hidden"));
  libraryEl.addEventListener("pointerdown", (e) => {
    if (e.target === libraryEl) libraryEl.classList.add("hidden");
  });

  // Copy history & usage journal.
  const journalEl = $("journal");
  $("journal-btn").addEventListener("click", openJournal);
  $("journal-close").addEventListener("click", () => journalEl.classList.add("hidden"));
  journalEl.addEventListener("pointerdown", (e) => {
    if (e.target === journalEl) journalEl.classList.add("hidden");
  });
  $("journal-clear").addEventListener("click", async () => {
    await invoke("clear_copy_history").catch((e) => toast(String(e)));
    settings.copy_log = [];
    settings.usage = {};
    renderJournal();
  });
  $("settings-close").addEventListener("click", () => settingsEl.classList.add("hidden"));
  // pointerdown (not click): selecting text that ends outside an input must not close.
  settingsEl.addEventListener("pointerdown", (e) => { if (e.target === settingsEl) settingsEl.classList.add("hidden"); });

  $("view-add").addEventListener("click", () => openViewModal(null));

  // View add / rename / delete popup.
  viewModal.confirm.addEventListener("click", confirmViewModal);
  viewModal.name.addEventListener("keydown", (e) => {
    if (e.key === "Enter") confirmViewModal();
  });
  viewModal.cancel.addEventListener("click", closeViewModal);
  viewModal.close.addEventListener("click", closeViewModal);
  viewModal.root.addEventListener("pointerdown", (e) => {
    if (e.target === viewModal.root) closeViewModal();
  });
  // Two-step delete: first click arms (red), second click removes.
  viewModal.delete.addEventListener("click", async () => {
    if (!viewModal.delete.classList.contains("confirm")) {
      viewModal.delete.classList.add("confirm");
      viewModal.delete.textContent = `${t("delete")}?`;
      setTimeout(() => disarmButton(viewModal.delete, t("delete")), 3000);
      return;
    }
    try {
      settings = await invoke("delete_view", { id: viewModalId });
    } catch (err) {
      toast(String(err));
      return;
    }
    closeViewModal();
    renderViews();
    renderViewsEditor();
    await renderGrid(true);
  });

  $("opt-minimize").addEventListener("change", (e) => {
    invoke("set_minimize_on_close", { enabled: e.target.checked }).catch((err) => toast(String(err)));
  });
  $("opt-autostart").addEventListener("change", async (e) => {
    try {
      await invoke("set_autostart", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked;
      toast(String(err));
    }
  });
  $("opt-startmin").addEventListener("change", async (e) => {
    try {
      await invoke("set_start_minimized", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked;
      toast(String(err));
    }
  });
  // Always-on-top: the header pin and the settings switch stay in sync; the
  // pin tints itself (accent) while active.
  const setOnTop = (on) => {
    settings.always_on_top = on;
    $("opt-ontop").checked = on;
    $("pin-top").classList.toggle("active", on);
    invoke("set_always_on_top", { enabled: on }).catch((err) => toast(String(err)));
  };
  $("opt-ontop").addEventListener("change", (e) => setOnTop(e.target.checked));
  $("pin-top").addEventListener("click", () => setOnTop(!settings.always_on_top));

  $("import-btn").addEventListener("click", runImport);
  $("export-csv").addEventListener("click", () => runExport("csv"));
  $("export-txt").addEventListener("click", () => runExport("txt"));
  $("delete-all").addEventListener("click", deleteAll);

  const themeSelect = $("theme-select");
  themeSelect.addEventListener("change", async () => {
    applyTheme(await invoke("set_theme", { theme: themeSelect.value }));
  });

  // Language: persist, then re-render every translated string in place —
  // no restart, no reload.
  $("lang-select").addEventListener("change", async (e) => {
    await invoke("set_language", { lang: e.target.value });
    LANG = resolveLang(e.target.value);
    applyI18n();
    // Re-fill the JS-built selects, then restore their current values.
    fillSizeSelects();
    fillFontSelects();
    $("tile-font").value = settings.tile_font || "system";
    $("tile-size").value = String(normSize(Number(settings.tile_size ?? 0)));
    await renderGrid(); // fresh state: tooltips + a renamed default view
    renderViewsEditor();
    if (!libraryEl.classList.contains("hidden")) renderLibrary();
  });

  // Prompt-tile font + size.
  const tileStyleChanged = async () => {
    settings.tile_font = $("tile-font").value;
    settings.tile_size = Number($("tile-size").value); // 0 = auto-fit
    applyTileStyle();
    invoke("set_tile_style", { font: settings.tile_font, size: settings.tile_size }).catch(() => {});
    await renderGrid(true); // re-render so auto-fit (or fixed size) applies cleanly
  };
  $("tile-font").addEventListener("change", tileStyleChanged);
  $("tile-size").addEventListener("change", tileStyleChanged);
  // Every dropdown uses the same popup style (scrollbar only when needed).
  attachSelectPicker($("tile-size"));
  attachSelectPicker(modal.sizeSel);
  attachSelectPicker(modal.captionSize);
  modal.caption.addEventListener("input", updateCaptionPreview);
  modal.captionSize.addEventListener("change", updateCaptionPreview);
  attachSelectPicker($("tile-font"));
  attachSelectPicker(modal.fontSel);
  attachSelectPicker($("theme-select"));
  attachSelectPicker($("lang-select"));

  ctxEl.addEventListener("click", async (e) => {
    const btn = e.target.closest("button");
    const act = btn?.dataset.act;
    if (!act || !ctxId) return;
    const id = ctxId;
    if (act === "delete") {
      if (!armButton(btn, t("deleteConfirm"))) return;
      closeCtx();
      await invoke("delete_prompt", { id });
      await renderGrid();
      return;
    }
    closeCtx();
    if (act === "edit") {
      await editPrompt(id);
    } else if (act === "hide") {
      // Remove from the active view's grid; stays available in the library.
      const view = activeView();
      const layout = { ...layoutOf(view) };
      delete layout[id];
      view.layouts[gridKeyOf(view)] = layout;
      invoke("set_layout", { layout }).catch((e) => toast(String(e)));
      await renderGrid(true);
    } else if (act === "pin") {
      await invoke("toggle_floating", { id });
    }
  });

  document.addEventListener("pointerdown", (e) => {
    if (!ctxEl.classList.contains("hidden") && !ctxEl.contains(e.target)) closeCtx();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (confirmCleanup) confirmCleanup(false);
    else if (!colorPop.classList.contains("hidden")) closeColorPop();
    else if (!ctxEl.classList.contains("hidden")) closeCtx();
    else if (!$("update-modal").classList.contains("hidden")) $("update-modal").classList.add("hidden");
    else if (!$("snip-modal").classList.contains("hidden")) $("snip-modal").classList.add("hidden");
    else if (!viewModal.root.classList.contains("hidden")) closeViewModal();
    else if (!modal.root.classList.contains("hidden")) confirmDiscardIfDirty().then((ok) => { if (ok) closeModal(); });
    else if (!libraryEl.classList.contains("hidden")) libraryEl.classList.add("hidden");
    else if (!settingsEl.classList.contains("hidden")) settingsEl.classList.add("hidden");
  });

  // Updates: manual check in the settings + daily background notification.
  let updateInfo = null;
  let statusTimer = null;
  const updateBtn = $("update-btn");
  const updateStatus = $("update-status");
  // Temporary status message; falls back to the version label after 5s.
  const flashStatus = (txt) => {
    updateStatus.textContent = txt;
    clearTimeout(statusTimer);
    statusTimer = setTimeout(() => { updateStatus.textContent = versionLabel; }, 5000);
  };
  const offerUpdate = (info) => {
    updateInfo = info;
    updateBtn.textContent = t("installUpdate").replace("{v}", info.version);
  };

  // Changelog popup shown before any install (manual check or daily toast).
  const updateModal = {
    root: $("update-modal"),
    title: $("update-modal-title"),
    notes: $("update-modal-notes"),
    warn: $("update-modal-warn"),
    cancel: $("update-modal-cancel"),
    skip: $("update-modal-skip"),
    install: $("update-modal-install"),
    close: $("update-modal-close"),
  };
  const openUpdateModal = (info) => {
    offerUpdate(info);
    updateModal.title.textContent = t("updateAvailable").replace("{v}", info.version);
    updateModal.notes.textContent = info.notes?.trim() || t("noChangelog");
    updateModal.warn.classList.toggle("hidden", !info.skipped);
    updateModal.root.classList.remove("hidden");
  };
  const closeUpdateModal = () => updateModal.root.classList.add("hidden");

  updateBtn.addEventListener("click", async () => {
    if (updateInfo?.available) { openUpdateModal(updateInfo); return; }
    updateBtn.disabled = true;
    try {
      const info = await invoke("check_update");
      if (info.available) openUpdateModal(info);
      else flashStatus(t("upToDate"));
    } catch (err) {
      flashStatus(t("updateFailed"));
      toast(String(err));
    }
    updateBtn.disabled = false;
  });
  updateModal.install.addEventListener("click", async () => {
    updateModal.install.disabled = true;
    try {
      await invoke("install_update", { url: updateInfo.url }); // app exits
    } catch (err) {
      updateModal.install.disabled = false;
      toast(String(err));
    }
  });
  updateModal.skip.addEventListener("click", async () => {
    const v = updateInfo?.version;
    if (!v) return;
    try { await invoke("skip_version", { version: v }); } catch (err) { toast(String(err)); }
    updateInfo = null;
    updateBtn.textContent = t("checkUpdate");
    closeUpdateModal();
    toast(t("versionSkipped").replace("{v}", v));
  });
  updateModal.cancel.addEventListener("click", closeUpdateModal);
  updateModal.close.addEventListener("click", closeUpdateModal);
  updateModal.root.addEventListener("pointerdown", (e) => {
    if (e.target === updateModal.root) closeUpdateModal();
  });

  $("opt-autoupdate").addEventListener("change", (e) => {
    invoke("set_auto_update", { enabled: e.target.checked }).catch((err) => toast(String(err)));
  });
  listen("update-available", (e) => {
    offerUpdate(e.payload);
    toast(t("updateAvailable").replace("{v}", e.payload.version), {
      label: t("installNow"),
      onClick: () => openUpdateModal(e.payload),
    });
  });

  listen("theme-changed", (e) => applyTheme(e.payload));
  // "Edit prompt" chosen in a floating pill's right-click menu.
  listen("edit-prompt", (e) => editPrompt(String(e.payload)));
}

// Fill both text-size selects: special options + 10..40 in steps of 2.
function fillSizeSelects() {
  const fill = (sel, specials) => {
    sel.innerHTML = "";
    const add = (value, label) => {
      const o = document.createElement("option");
      o.value = value;
      o.textContent = label;
      sel.appendChild(o);
    };
    for (const [value, key] of specials) add(value, t(key));
    for (let s = SIZE_MIN; s <= SIZE_MAX; s += SIZE_STEP) add(String(s), s);
  };
  fill($("tile-size"), [["0", "langAuto"]]);
  fill(modal.sizeSel, [["0", "styleDefault"], ["1", "langAuto"]]);
  fill(modal.captionSize, [["0", "styleDefault"], ["1", "langAuto"]]);
}

// Fill both font selects from the shared catalog; every entry carries its own
// font family so the popup can preview it.
function fillFontSelects() {
  const fill = (sel, withDefault) => {
    sel.innerHTML = "";
    if (withDefault) {
      const o = document.createElement("option");
      o.value = "";
      o.textContent = t("styleDefault");
      sel.appendChild(o);
    }
    for (const [key, stack] of Object.entries(FONTS)) {
      const o = document.createElement("option");
      o.value = key;
      o.textContent =
        FONT_LABELS[key] ?? t(key === "script" ? "fontScript" : "fontSystem");
      o.style.fontFamily = stack;
      sel.appendChild(o);
    }
  };
  fill($("tile-font"), false);
  fill(modal.fontSel, true);
}

// Snap legacy sizes (13/15/18/22) onto the new 10..40 grid.
const normSize = (v) =>
  v <= 1 ? v : Math.min(SIZE_MAX, Math.max(SIZE_MIN, Math.round(v / 2) * 2));

// Drag & drop onto the window: a dropped file/image/PDF becomes a button right
// away (same dialog as the paperclip); dropped text fills the composer and opens
// the save dialog. Extends the existing clipboard/screenshot paths.
function setupDragDrop() {
  const wv = window.__TAURI__?.webview?.getCurrentWebview?.();
  wv?.onDragDropEvent?.((e) => {
    if (!flag("dragDrop")) return;
    const kind = e.payload?.type;
    document.body.classList.toggle("drag-over", kind === "enter" || kind === "over");
    if (kind === "drop") {
      document.body.classList.remove("drag-over");
      const path = e.payload?.paths?.[0];
      if (path) openFileCreate(path).catch((err) => toast(String(err)));
    }
  }).catch?.(() => {});
  // Text dropped from another app (no file paths) — native handler ignores it.
  window.addEventListener("dragover", (e) => e.preventDefault());
  window.addEventListener("drop", (e) => {
    if (!flag("dragDrop")) return;
    const text = e.dataTransfer?.getData("text");
    if (text && !(e.dataTransfer.files && e.dataTransfer.files.length)) {
      e.preventDefault();
      inputEl.value = text;
      autoGrow(inputEl);
      saveBtn.disabled = !text.trim();
      startCreate();
    }
  });
}

// ---- Init ----
async function init() {
  settings = await invoke("get_settings");
  LANG = resolveLang(settings.language);
  applyI18n();
  fillSizeSelects();
  fillFontSelects();
  applyTheme(await invoke("current_theme"));
  bind();
  setupDragDrop();
  applyFlags();
  applyBars();
  await renderGrid();
  $("theme-select").value = settings.theme;
  $("lang-select").value = settings.language || "auto";
  $("opt-minimize").checked = settings.minimize_to_tray === true;
  $("opt-autostart").checked = settings.autostart === true;
  $("opt-startmin").checked = settings.start_minimized === true;
  $("opt-ontop").checked = settings.always_on_top === true;
  $("pin-top").classList.toggle("active", settings.always_on_top === true);
  $("opt-autoupdate").checked = settings.auto_update !== false;
  $("tile-font").value = settings.tile_font || "system";
  $("tile-size").value = String(normSize(Number(settings.tile_size ?? 0)));
  invoke("app_version").then((v) => {
    versionLabel = `v${v}`;
    $("update-status").textContent = versionLabel;
  }).catch(() => {});
  applyTileStyle();
  autoGrow(inputEl);
  inputEl.focus();
  pollMissingFiles();
  setInterval(pollMissingFiles, FILE_POLL_MS);
  // Reveal the window only after the first fully fitted paint — the user
  // never sees the text sizing itself.
  requestAnimationFrame(() => {
    fitCache.clear();
    fitAllTiles();
    requestAnimationFrame(() => invoke("show_main_window").catch(() => {}));
  });
}

init();
