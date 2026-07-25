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
  closeX: $("modal-close-x"),
  confirm: $("modal-confirm"),
  delete: $("modal-delete"),
  fav: $("modal-fav"),
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
let PREVIEW_MAX = 600; // tooltip preview length (expert-tunable)
let BUBBLE_MS = 950; // "Copied!" bubble lifetime (expert-tunable)
let TOAST_MS = 1400; // toast lifetime for plain messages (expert-tunable)
const SIZE_MIN = 10; // text size range, steps of 2 (keep in sync with backend)
const SIZE_MAX = 40;
const SIZE_STEP = 2;
const FILE_POLL_MS = 5000; // missing-file watcher interval
const DISARM_MS = 3000; // confirm-button auto-disarm delay

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
let libFav = false; // prompt library "favorites only" filter
// F8 batch operations state.
let libSelectMode = false;
let libSelected = new Set();
let libLastId = null;          // anchor for shift-click range select
let libDisplayOrder = [];      // ids in current display order (for range select)
let gridDirty = false;         // batch ops changed prompts; rebuild grid on library close
let journalQuery = ""; // copy-history search text
let journalType = "all"; // copy-history type filter
let journalColor = "all"; // copy-history color filter
let toastTimer = null;
let deleteAllTimer = null;
let resetSettingsTimer = null;
let expertResetTimer = null;
let versionLabel = ""; // "Version 1.6.0", shown as the clickable releases link

// Hide the settings modal and reset the Updates status line back to the version
// number — unless a real update is pending (the "update available" state stays).
// So a transient "already newest version" message doesn't linger after reopening.
function hideSettings() {
  settingsEl.classList.add("hidden");
  const uv = $("update-version");
  if (uv && versionLabel && !uv.classList.contains("update-avail")) uv.textContent = versionLabel;
}

// Surface unexpected errors as a toast instead of failing silently.
window.addEventListener("error", (e) => toast(String(e.message)));
window.addEventListener("unhandledrejection", (e) => toast(String(e.reason)));

// Right-click is disabled everywhere inside the app (tiles re-enable it).
window.addEventListener("contextmenu", (e) => e.preventDefault());

const DOTS =
  '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d="M6 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM12 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM18 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z"/></svg>';
const CROSS =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M6.4 5 12 10.6 17.6 5 19 6.4 13.4 12 19 17.6 17.6 19 12 13.4 6.4 19 5 17.6 10.6 12 5 6.4Z"/></svg>';
const GRID_PLUS =
  '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="currentColor" d="M4 4h7v7H4V4Zm9 0h7v7h-7V4ZM4 13h7v7H4v-7Zm12 0h2v3h3v2h-3v3h-2v-3h-3v-2h3v-3Z"/></svg>';
const PLUS_ICON =
  '<svg viewBox="0 0 24 24" width="19" height="19" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" d="M12 5v14M5 12h14"/></svg>';
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
// "none" and custom swatches. The user can recolor the palette (expert menu);
// COLORS holds the live set, DEFAULT_COLORS the shipped fallback.
const DEFAULT_COLORS = [
  "",
  "#ef4444", "#f97316", "#f59e0b", "#eab308", "#84cc16",
  "#22c55e", "#10b981", "#14b8a6", "#06b6d4", "#0ea5e9",
  "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#d946ef",
  "#ec4899", "#f43f5e", "#64748b",
];
let COLORS = [...DEFAULT_COLORS];
const PALETTE_LEN = DEFAULT_COLORS.length - 1; // editable hues (index 0 is "none")
const HEX_RE = /^#[0-9a-fA-F]{6}$/;

// Load the custom palette from settings (ui_texts.palette = JSON hex array), or
// fall back to defaults. Invalid/partial data is ignored so colors never break.
function loadPalette() {
  let custom = null;
  try {
    const raw = settings.ui_texts?.palette;
    if (raw) {
      const arr = JSON.parse(raw);
      if (Array.isArray(arr) && arr.length === PALETTE_LEN && arr.every((c) => HEX_RE.test(c))) {
        custom = arr;
      }
    }
  } catch (_) { /* malformed -> defaults */ }
  COLORS = custom ? ["", ...custom] : [...DEFAULT_COLORS];
}

// Rebuild the filter dots (cached) after a palette change, and refresh any open list.
function refreshColorUI() {
  for (const [wrap, cur] of [[$("library-colors"), libColor], [$("journal-colors"), journalColor]]) {
    if (wrap) { wrap.innerHTML = ""; buildColorFilter(wrap, cur); }
  }
  if (!libraryEl.classList.contains("hidden")) renderLibrary();
  if (!$("journal").classList.contains("hidden")) renderJournal();
}

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

// Default toast icon (checkmark badge). Static, developer-controlled SVG.
const TOAST_ICON = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>';

// Optional action: { label, onClick } adds a button and keeps the toast longer.
// Every toast is now the same themed card (icon badge + message), matching the
// update notification, so nothing looks like the old flat pill.
function toast(msg, action = null) {
  toastEl.textContent = "";
  // Only actionable toasts capture clicks; plain ones stay click-through so they
  // never block the UI underneath. The card layout lives on the base .toast now.
  toastEl.classList.toggle("actionable", !!action);
  toastEl.classList.toggle("update", !!(action && action.variant === "update"));
  const ico = document.createElement("span");
  ico.className = "toast-ico";
  ico.innerHTML = (action && action.icon) ? action.icon : TOAST_ICON;
  toastEl.appendChild(ico);
  const label = document.createElement("span");
  label.className = "toast-msg";
  label.textContent = msg;
  toastEl.appendChild(label);
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

// UI scale now uses the WebView's NATIVE zoom (set from Rust, like Ctrl +/-), which
// keeps event coords and getBoundingClientRect in one coordinate space — so
// JS-positioned fixed popups need no compensation. Kept as a hook (always 1) so the
// existing `/ z` call sites stay no-ops instead of being ripped out everywhere.
const uiZoom = () => 1;

function openNumPop(input, apply) {
  popInput = input;
  popApply = apply;
  const r = input.getBoundingClientRect();
  const z = uiZoom();
  numPop.style.left = `${Math.min(r.left, window.innerWidth - 70) / z}px`;
  numPop.style.top = `${(r.bottom + 4) / z}px`;
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
  const z = uiZoom();
  numPop.style.left = `${Math.min(r.left, window.innerWidth - 120) / z}px`;
  numPop.style.top = `${(r.bottom + 4) / z}px`;
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
  '<input class="cp-hue" type="range" min="0" max="360" step="1" data-i18n-aria="colorHue" />' +
  '<div class="cp-row"><span class="cp-preview"></span><input class="cp-hex" type="text" maxlength="7" spellcheck="false" data-i18n-aria="colorHex" /></div>';
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
  const z = uiZoom();
  colorPop.style.left = `${Math.min(r.left, window.innerWidth - w - 8) / z}px`;
  colorPop.style.top = `${(r.bottom + 6) / z}px`;
}

function closeColorPop() {
  cpOnPick = null;
  colorPop.classList.add("hidden");
}

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
  iconTooltips: "flagIconTooltips",
  tooltipDelay: "flagTooltipDelay",
  tooltipTimeout: "flagTooltipTimeout",
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
  captureExclusion: "flagCaptureExclusion",
  copyHistory: "flagCopyHistory",
  historyTimestamps: "flagHistoryTimestamps",
  historyGrouped: "flagHistoryGrouped",
  librarySearch: "flagLibrarySearch",
  searchSuggest: "flagSearchSuggest",
  colorFilter: "flagColorFilter",
  closeAfterCopy: "flagCloseAfterCopy",
  libraryCloseToggle: "flagLibraryCloseToggle",
  imagePreview: "flagImagePreview",
  libraryVideoPreview: "flagLibraryVideoPreview",
  confirmDiscard: "flagConfirmDiscard",
  showLogo: "flagShowLogo",
  showTitle: "flagShowTitle",
  showLibrary: "library",   // top-bar library button visibility (toolbar menu)
  showJournal: "journal",   // top-bar copy-history button visibility (toolbar menu)
  chainPrompts: "flagChainPrompts",
  showUpdates: "flagShowUpdates",
  importExport: "flagImportExport",
  tileReorder: "flagTileReorder",
  libraryTypeFilter: "flagLibraryTypeFilter",
  journalSearch: "flagJournalSearch",
  journalTypeFilter: "flagJournalTypeFilter",
  journalColorFilter: "flagJournalColorFilter",
  journalClear: "flagJournalClear",
  journalMostUsed: "flagJournalMostUsed",
  journalRecent: "flagJournalRecent",
  journalGroupBtn: "flagJournalGroupBtn",
  journalRecentSort: "flagJournalRecentSort",
  journalUsedSort: "flagJournalUsedSort",
  confirmClearHistory: "flagConfirmClearHistory",
  chainLock: "flagChainLock",
  favorites: "flagFavorites",
  keyboardNav: "flagKeyboardNav",
  varDefaults: "flagVarDefaults",
  libraryColsToggle: "flagLibraryColsToggle",
  favViewButton: "flagFavViewButton",
  favViewReorder: "flagFavViewReorder",
  tileShadow: "flagTileShadow",
  tilePressScale: "flagTilePressScale",
  tileHoverLift: "flagTileHoverLift",
  copyFlash: "flagCopyFlash",
  headerSeparators: "flagHeaderSeparators",
  cleanupFiles: "flagCleanupFiles",
  // New features (default on).
  lengthCounter: "flagLengthCounter",   // F16
  smartSort: "flagSmartSort",           // F10
  fuzzySearch: "flagFuzzySearch",       // F18
  batchOps: "flagBatchOps",             // F8
  promptHistory: "flagPromptHistory",   // F7
  usageStats: "flagUsageStats",         // F11
  dupFinder: "flagDupFinder",           // F6
  dupImportCheck: "flagDupImportCheck", // F6
  autoBackup: "flagAutoBackup",         // F2
  autoPaste: "flagAutoPaste",           // F19 — normal setting now (default on)
  // Checks at launch, but only while the normal auto-update setting is on.
  checkUpdateOnStart: "flagCheckUpdateOnStart",
};
const ALL_FLAG_KEYS = Object.keys(FLAG_LABELS);

// Opt-in flags: OFF by default (the mirror of the flags above, which are ON by
// default). Enabled only when explicitly set true; wired via a `flag-<key>` body
// class that is present exactly when the option is on.
const OPT_FLAG_LABELS = {
  // Features extras
  storeFiles: "flagStoreFiles",
  gridLines: "flagGridLines",
  uppercaseTiles: "flagUppercaseTiles",
  boldTileNames: "flagBoldTileNames",
  monospaceTiles: "flagMonospaceTiles",
  italicTiles: "flagItalicTiles",
  tileTextShadow: "flagTileTextShadow",
  // Library/search extras
  searchAutofocus: "flagSearchAutofocus",
  // Appearance extras
  compactTiles: "flagCompactTiles",
  tileGradient: "flagTileGradient",
  accentHeader: "flagAccentHeader",
  frostedModals: "flagFrostedModals",
  hideScrollbars: "flagHideScrollbars",
  accentScrollbar: "flagAccentScrollbar",
  smoothScroll: "flagSmoothScroll",
  // Privacy extras
  blurTilesUntilHover: "flagBlurTilesUntilHover",
  blurMediaUntilHover: "flagBlurMediaUntilHover",
  hideTileNames: "flagHideTileNames",
  dimUnhovered: "flagDimUnhovered",
  // Media extras
  grayscaleMedia: "flagGrayscaleMedia",
  dimMedia: "flagDimMedia",
  mediaBorder: "flagMediaBorder",
  roundMedia: "flagRoundMedia",
  // New features (default off).
  autoPasteEnter: "flagAutoPasteEnter", // F19
  clipWatcher: "flagClipWatcher",       // F21
  backupGfs: "flagBackupGfs",           // F2 period-based retention
  // v2.6.0 additions (all default off → "(Standard Aus)").
  editorMonospace: "flagEditorMonospace",       // monospace editor textarea
  dimToolbar: "flagDimToolbar",                 // header dims until hovered
  closeOnCopy: "flagCloseOnCopy",               // hide window after copying a prompt
  dedupCopyLog: "flagDedupCopyLog",             // skip identical consecutive copies
  autoBackupBeforeWipe: "flagAutoBackupBeforeWipe", // backup before reset/delete
  // Keep the clipboard-inbox and copy-history buttons in the toolbar even while
  // their feature is switched off, so the feature can be paused, not hidden.
  pausedIcons: "flagPausedIcons",
};
const OPT_FLAG_KEYS = Object.keys(OPT_FLAG_LABELS);
const optFlag = (key) => settings.ui_flags?.[key] === true;
const isOptFlag = (key) => key in OPT_FLAG_LABELS;
// Current state + label key, whichever kind of flag this is.
const flagState = (key) => (isOptFlag(key) ? optFlag(key) : flag(key));
const flagLabelKey = (key) => OPT_FLAG_LABELS[key] || FLAG_LABELS[key];
// A value/select/dropdown may declare `gate: "flagKey"` (or "!flagKey" to invert)
// so its row hides while that feature is off — no dead sliders for disabled things.
const gateOpen = (gate) => {
  if (!gate) return true;
  const inv = gate[0] === "!";
  const on = flagState(inv ? gate.slice(1) : gate);
  return inv ? !on : on;
};

// Copy-history slider stops: coarser steps as the value grows (10s -> 100s ->
// 1000s -> 10000s -> 100000s) so the slider stays smooth all the way to 1,000,000.
const HISTORY_STOPS = (() => {
  const s = [];
  for (let v = 0; v <= 100; v += 10) s.push(v);
  for (let v = 200; v <= 1000; v += 100) s.push(v);
  for (let v = 2000; v <= 10000; v += 1000) s.push(v);
  for (let v = 20000; v <= 100000; v += 10000) s.push(v);
  for (let v = 200000; v <= 1000000; v += 100000) s.push(v);
  return s;
})();

// Numeric value tweaks. `def` is the shipped default; a missing value uses it.
const EXPERT_VALUES = {
  videoVolume: { label: "valVideoVolume", min: 0, max: 100, step: 5, def: 100, unit: "%", gate: "!videoMuted" },
  animSpeed: { label: "valAnimSpeed", min: 0, max: 1000, step: 10, def: 150, unit: "ms", gate: "animations" },
  gridGap: { label: "valGridGap", min: 0, max: 60, step: 1, def: 8, unit: "px" },
  gridPad: { label: "valGridPad", min: 0, max: 40, step: 1, def: 10, unit: "px" },
  modalRadius: { label: "valModalRadius", min: 0, max: 28, step: 1, def: 14, unit: "px" },
  overlayDim: { label: "valOverlayDim", min: 0, max: 80, step: 5, def: 40, unit: "%" },
  bubbleMs: { label: "valBubbleMs", min: 300, max: 6000, step: 50, def: 950, unit: "ms" },
  toastMs: { label: "valToastMs", min: 800, max: 10000, step: 100, def: 1400, unit: "ms" },
  previewLen: { label: "valPreviewLen", min: 0, max: 4000, step: 20, def: 600, unit: "" },
  viewBorder: { label: "valViewBorder", min: 1, max: 12, step: 1, def: 3, unit: "px", gate: "multiView" },
  // Floating-button opacity (%). Floored at 20% so the pill never vanishes.
  floatOpacity: { label: "valFloatOpacity", min: 20, max: 100, step: 5, def: 100, unit: "%", gate: "floating" },
  // Custom tile tooltip max width (it wraps + grows taller instead of widening).
  tooltipWidth: { label: "valTooltipWidth", min: 180, max: 640, step: 10, def: 340, unit: "px", gate: "tilePreview" },
  // How long the pointer must rest on an element before its tooltip appears, so a
  // casual sweep across the window shows nothing (paired with tooltipDelay).
  tooltipDelayMs: { label: "valTooltipDelay", min: 100, max: 2000, step: 50, def: 500, unit: "ms", gate: "tooltipDelay" },
  // How long the hover tooltip stays before auto-hiding (paired with tooltipTimeout).
  tooltipTimeoutMs: { label: "valTooltipTimeout", min: 2000, max: 60000, step: 1000, def: 15000, unit: "ms", gate: "tooltipTimeout" },
  // Appearance scales (percent, 100 = unchanged). Applied as CSS zoom.
  uiScale: { label: "valUiScale", min: 50, max: 300, step: 5, def: 100, unit: "%" },
  modalScale: { label: "valModalScale", min: 60, max: 300, step: 5, def: 100, unit: "%" },
  composerScale: { label: "valComposerScale", min: 60, max: 300, step: 5, def: 100, unit: "%" },
  iconScale: { label: "valIconScale", min: 50, max: 300, step: 5, def: 100, unit: "%" },
  primaryScale: { label: "valPrimaryScale", min: 60, max: 300, step: 5, def: 100, unit: "%" },
  // Max length for prompt names + captions (raise it for long labels).
  nameMaxLen: { label: "valNameMaxLen", min: 20, max: 1000, step: 10, def: 80, unit: "" },
  // Limit overrides (defaults match the classic 20; backend ceiling is 200).
  maxViews: { label: "valMaxViews", min: 1, max: 200, step: 1, def: 20, unit: "" },
  gridMax: { label: "valGridMax", min: 1, max: 200, step: 1, def: 20, unit: "" },
  // Copy-history length (0 = keep none).
  historyMax: { label: "valHistoryMax", def: 100, unit: "", stops: HISTORY_STOPS, gate: "copyHistory" },
  // Max copy-history rows the journal renders at once (UI cap, not storage).
  journalLimit: { label: "valJournalLimit", min: 50, max: 10000, step: 50, def: 100, unit: "", gate: "copyHistory" },
  // Block re-copying the SAME prompt within this window (0 = off). Never blocks others.
  copyCooldownMs: { label: "valCopyCooldown", min: 0, max: 10000, step: 100, def: 2000, unit: "ms" },
  // Header favorites-star size.
  favStarSize: { label: "valFavStarSize", min: 14, max: 48, step: 1, def: 22, unit: "px", gate: "favViewButton" },
  // Favorites grid: cap the column count (0 = auto-size to the count).
  favMaxCols: { label: "valFavMaxCols", min: 0, max: 20, step: 1, def: 0, unit: "", gate: "favViewButton" },
  // Prompt-list (library) column options: how many columns the header toggle offers.
  libraryMaxCols: { label: "valLibraryMaxCols", min: 1, max: 6, step: 1, def: 2, unit: "", gate: "libraryColsToggle" },
  // Prompt-list (library) dialog width.
  libraryWidth: { label: "valLibraryWidth", min: 520, max: 1400, step: 20, def: 900, unit: "px" },
  // How many recent search terms the suggestions dropdown keeps + shows.
  searchRecentMax: { label: "valSearchRecentMax", min: 1, max: 20, step: 1, def: 8, unit: "", gate: "searchSuggest" },
  // Tile geometry.
  tileRadius: { label: "valTileRadius", min: 0, max: 30, step: 1, def: 10, unit: "px" },
  tileBorderWidth: { label: "valTileBorderWidth", min: 0, max: 6, step: 1, def: 1, unit: "px" },
  // Tile hover-lift travel (paired with the tileHoverLift toggle).
  hoverLift: { label: "valHoverLift", min: 1, max: 12, step: 1, def: 1, unit: "px", gate: "tileHoverLift" },
  // Header vertical padding + screenshot-preview max height.
  headerPadY: { label: "valHeaderPadY", min: 2, max: 30, step: 1, def: 10, unit: "px" },
  snipPreviewVh: { label: "valSnipPreviewVh", min: 30, max: 92, step: 1, def: 74, unit: "%", gate: "screenshot" },
  // Scrollbar thickness across every dialog (thumb keeps its 3px inset border).
  scrollbarWidth: { label: "valScrollbarWidth", min: 8, max: 20, step: 1, def: 12, unit: "px" },
  // Frosted-glass dialog background blur (paired with the frostedModals toggle).
  frostedBlur: { label: "valFrostedBlur", min: 0, max: 20, step: 1, def: 4, unit: "px", gate: "frostedModals" },
  // Privacy blur strength (each paired with its "blur until hover" toggle).
  tileBlur: { label: "valTileBlur", min: 1, max: 16, step: 1, def: 4, unit: "px", gate: "blurTilesUntilHover" },
  mediaBlur: { label: "valMediaBlur", min: 1, max: 30, step: 1, def: 10, unit: "px", gate: "blurMediaUntilHover" },
  // Dim strength for the "dim unhovered / dim media" toggles (lower = dimmer).
  dimOpacity: { label: "valDimOpacity", min: 10, max: 90, step: 5, def: 45, unit: "%", gate: "dimUnhovered" },
  mediaDimOpacity: { label: "valMediaDim", min: 10, max: 95, step: 5, def: 80, unit: "%", gate: "dimMedia" },
  // Rounded-media corner radius (paired with the roundMedia toggle).
  mediaRadius: { label: "valMediaRadius", min: 0, max: 24, step: 1, def: 8, unit: "px", gate: "roundMedia" },
  // F7 prompt version history: versions kept per prompt.
  historyPerPrompt: { label: "valHistoryPerPrompt", min: 5, max: 100, step: 5, def: 20, unit: "", gate: "promptHistory" },
  // F11 usage statistics: how many rows the top-N lists show.
  statsTopN: { label: "valStatsTopN", min: 5, max: 100, step: 1, def: 10, unit: "", gate: "usageStats" },
  // F6 duplicate finder: similarity threshold.
  dupThreshold: { label: "valDupThreshold", min: 70, max: 100, step: 1, def: 90, unit: "%", gate: "dupFinder" },
  // F2 automatic backups: how many rotating snapshots to keep.
  backupKeep: { label: "valBackupKeep", min: 1, max: 50, step: 1, def: 10, unit: "", gate: "autoBackup" },
  // F2 GFS retention tiers (kept per distinct day / week / month).
  backupDaily: { label: "valBackupDaily", min: 0, max: 30, step: 1, def: 3, unit: "", gate: "backupGfs" },
  backupWeekly: { label: "valBackupWeekly", min: 0, max: 52, step: 1, def: 4, unit: "", gate: "backupGfs" },
  backupMonthly: { label: "valBackupMonthly", min: 0, max: 60, step: 1, def: 12, unit: "", gate: "backupGfs" },
  // F19 auto-paste: SetForeground→Ctrl+V delay + double-click window.
  autoPasteDelayMs: { label: "valAutoPasteDelay", min: 30, max: 300, step: 10, def: 80, unit: "ms", gate: "autoPaste" },
  dblClickMs: { label: "valDblClick", min: 200, max: 600, step: 20, def: 400, unit: "ms", gate: "autoPaste" },
  // F21 clipboard watcher: min length + inbox cap.
  clipMinChars: { label: "valClipMin", min: 1, max: 100, step: 1, def: 3, unit: "", gate: "clipWatcher" },
  clipInboxMax: { label: "valClipMax", min: 10, max: 200, step: 10, def: 50, unit: "", gate: "clipWatcher" },
  // Keyboard grid focus fades this long after the window loses focus.
  kbFocusFadeMs: { label: "valKbFocusFade", min: 500, max: 10000, step: 500, def: 2000, unit: "ms", gate: "keyboardNav" },
};

// Preset-or-custom numeric settings (dropdown with a free-entry option).
const EXPERT_SELECTS = {
  historyDays: { label: "valHistoryDays", options: [0, 7, 30, 90, 365], def: 0, unit: "d", zeroLabel: "retentionForever", gate: "copyHistory" },
  // F2 backup interval (hours) — a preset-or-custom select right under backupKeep.
  backupIntervalH: { label: "valBackupInterval", options: [6, 12, 24, 48, 168], def: 24, unit: "h", gate: "autoBackup" },
};

// Build a simple string-enum dropdown backed by ui_texts (default when unset).
// `opts` is [[value, i18nLabelKey], …]. Used by expert enum params.
function enumDropdown(key, labelKey, opts, def, gate) {
  return {
    label: labelKey,
    gate,
    options: () => opts.map(([v, lk]) => [v, t(lk)]),
    get: () => settings.ui_texts?.[key] || def,
    set: async (v) => {
      settings.ui_texts = { ...(settings.ui_texts || {}), [key]: v };
      try { await invoke("set_ui_text", { key, value: v }); } catch (e) { toast(String(e)); }
      applyValues();
    },
  };
}

// Dropdowns like the settings' text-size / font selects. copySize lives in
// ui_values (0 = auto-fit to the button), copyFont in ui_texts ("" = default).
const EXPERT_DROPDOWNS = {
  // F16 live counter unit selection.
  counterUnits: enumDropdown("counterUnits", "valCounterUnits",
    [["all", "optAll"], ["chars", "lenChars"], ["words", "lenWords"], ["tokens", "lenTokens"]], "all", "lengthCounter"),
  // F18 fuzzy-search typo tolerance.
  fuzzyMaxTypos: enumDropdown("fuzzyMaxTypos", "valFuzzyTypos",
    [["auto", "optAuto"], ["1", "opt1"], ["2", "opt2"]], "auto", "fuzzySearch"),
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
  { title: "expTabGeneral", groups: [
    { title: "expGroupCreate", flags: ["fileAttach", "screenshot", "pasteMedia", "dragDrop", "promptVars", "varDefaults", "lengthCounter", "promptHistory", "storeFiles", "editorMonospace"], values: ["snipPreviewVh", "historyPerPrompt"], dropdowns: ["counterUnits"], paths: ["screenshotDir"] },
    { title: "expGroupWorkspace", flags: ["floating", "keyboardNav", "headerSeparators", "dimToolbar"], values: ["floatOpacity", "kbFocusFadeMs"] },
    { title: "expGroupSystem", flags: ["showUpdates", "checkUpdateOnStart"] },
  ] },
  { title: "expTabTiles", groups: [
    // Split by what a setting actually does: acting on a tile, the hover tooltip,
    // and pure looks. One 26-entry blob made everything hard to find.
    { title: "expGroupTiles", flags: ["tileMenu", "tileReorder", "copyBubble", "copyFlash", "closeOnCopy", "chainPrompts", "chainLock", "autoPasteEnter"], values: ["copyCooldownMs", "autoPasteDelayMs", "dblClickMs"] },
    { title: "expGroupTooltips", flags: ["tilePreview", "iconTooltips", "tooltipDelay", "tooltipTimeout"], values: ["tooltipWidth", "tooltipDelayMs", "tooltipTimeoutMs"] },
    { title: "expGroupTileLook", flags: ["tileHover", "tileHoverLift", "tilePressScale", "tileShadow", "typeBadges", "captions", "compactTiles", "tileGradient", "gridLines", "uppercaseTiles", "boldTileNames", "monospaceTiles", "italicTiles", "tileTextShadow"], values: ["hoverLift", "tileRadius", "tileBorderWidth"] },
    { title: "expGroupFavorites", flags: ["favorites", "favViewButton", "favViewReorder"], values: ["favStarSize", "favMaxCols"] },
  ] },
  { title: "expTabAppearance", groups: [
    { title: "expGroupScale", values: ["uiScale", "modalScale", "composerScale", "iconScale", "primaryScale"] },
    { title: "expGroupVisual", flags: ["animations", "accentHeader", "hideScrollbars", "accentScrollbar", "smoothScroll", "frostedModals"], values: ["animSpeed", "gridGap", "gridPad", "headerPadY", "modalRadius", "overlayDim", "scrollbarWidth", "frostedBlur"], dropdowns: ["copySize", "copyFont"] },
    { title: "expGroupColors", palette: true },
    { title: "expGroupLimits", values: ["maxViews", "gridMax", "previewLen", "nameMaxLen", "bubbleMs", "toastMs"] },
  ] },
  { title: "expTabLibrary", groups: [
    { title: "expGroupLibrary", flags: ["librarySearch", "searchSuggest", "fuzzySearch", "smartSort", "batchOps", "libraryTypeFilter", "colorFilter", "searchAutofocus", "imagePreview", "libraryVideoPreview", "closeAfterCopy", "libraryCloseToggle", "libraryColsToggle", "confirmDiscard"], values: ["searchRecentMax", "libraryMaxCols", "libraryWidth"], dropdowns: ["fuzzyMaxTypos"] },
    // Everything that records or shows copy history lives with the history view.
    { title: "expGroupJournal", flags: ["copyHistory", "historyTimestamps", "historyGrouped", "dedupCopyLog", "journalSearch", "journalTypeFilter", "journalColorFilter", "journalRecent", "journalMostUsed", "journalGroupBtn", "journalRecentSort", "journalUsedSort", "journalClear", "confirmClearHistory"], values: ["historyMax", "journalLimit"], selects: ["historyDays"] },
    { title: "expGroupPrivacy", flags: ["captureExclusion", "blurTilesUntilHover", "blurMediaUntilHover", "hideTileNames", "dimUnhovered"], values: ["tileBlur", "mediaBlur", "dimOpacity"] },
  ] },
  { title: "expTabMedia", groups: [
    { title: "expGroupMedia", flags: ["videoControls", "videoAutoplay", "videoMuted", "videoLoop", "grayscaleMedia", "dimMedia", "mediaBorder", "roundMedia"], values: ["videoVolume", "mediaDimOpacity", "mediaRadius"] },
  ] },
  { title: "expTabTools", groups: [
    // Toolbar button visibility (mirrors the header right-click tool list).
    { title: "expGroupToolbar", flags: ["multiView", "quickGrid", "pinButton", "barToggles", "showLibrary", "showJournal", "showLogo", "showTitle", "pausedIcons"], values: ["viewBorder"] },
    { title: "expGroupData", flags: ["dupFinder", "dupImportCheck", "autoBackupBeforeWipe", "cleanupFiles", "importExport"], values: ["dupThreshold"], paths: ["dataDir"],
      actions: [{ id: "dupes-open", label: "dupTitle", gate: "dupFinder" }] },
    { title: "expGroupClipboard", flags: ["clipWatcher"], values: ["clipMinChars", "clipInboxMax"] },
  ] },
];

const flag = (key) => settings.ui_flags?.[key] !== false;
const val = (key) => {
  const cfg = EXPERT_VALUES[key] || EXPERT_SELECTS[key]; // selects live in ui_values too
  const raw = settings.ui_values?.[key];
  const v = Number.isFinite(raw) ? raw : cfg.def;
  // Clamp to the setting's own range so no stored/imported value can push a
  // slider-bounded setting out of bounds (grid geometry etc. stays always valid).
  if (cfg.stops) {
    return Math.min(cfg.stops[cfg.stops.length - 1], Math.max(cfg.stops[0], v));
  }
  if (Number.isFinite(cfg.min) && Number.isFinite(cfg.max)) {
    return Math.min(cfg.max, Math.max(cfg.min, v));
  }
  return v;
};
const txt = (key) => settings.ui_texts?.[key] || "";

// Mirror each disabled flag onto a body class so CSS can hide the matching UI.
function applyFlags() {
  for (const key of ALL_FLAG_KEYS) {
    document.body.classList.toggle(`noflag-${key}`, !flag(key));
  }
  // Opt-in (default-off) flags: a `flag-<key>` class appears only when enabled.
  for (const key of OPT_FLAG_KEYS) {
    document.body.classList.toggle(`flag-${key}`, optFlag(key));
  }
  if (chainMode && !flag("chainPrompts")) setChainMode(false); // killed in expert menu
  if (chainLock && !flag("chainLock")) { chainLock = false; $("chain-btn")?.classList.remove("chain-locked"); }
  applyValues();
  refreshClipInbox(); // F21: reflect the watcher toggle + badge count
}

// Apply every numeric tweak to its live target (CSS vars + JS constants).
let _lastUiZoom = null; // last native-zoom factor pushed to the backend (throttle)
function applyValues() {
  const root = document.documentElement.style;
  root.setProperty("--transition", `${val("animSpeed")}ms cubic-bezier(0.4, 0, 0.2, 1)`);
  root.setProperty("--gap", `${val("gridGap")}px`);
  root.setProperty("--grid-pad", `${val("gridPad")}px`);
  root.setProperty("--modal-radius", `${val("modalRadius")}px`);
  root.setProperty("--overlay-dim", String(val("overlayDim") / 100));
  root.setProperty("--view-border", `${val("viewBorder")}px`);
  // Appearance scales. UI scale drives the WebView's native zoom (robust); the rest
  // stay CSS `zoom` on their own elements (1 = unchanged). Only push the native zoom
  // when it actually changed — applyValues runs on every slider tick, and a redundant
  // IPC per tick would flood the backend while dragging an unrelated value.
  const uiZoomFactor = val("uiScale") / 100;
  if (uiZoomFactor !== _lastUiZoom) {
    _lastUiZoom = uiZoomFactor;
    invoke("set_ui_zoom", { factor: uiZoomFactor }).catch(() => {});
  }
  root.setProperty("--modal-zoom", val("modalScale") / 100);
  root.setProperty("--composer-zoom", val("composerScale") / 100);
  root.setProperty("--icon-zoom", val("iconScale") / 100);
  root.setProperty("--primary-zoom", val("primaryScale") / 100);
  // Sizes exposed to CSS (favorites star, chain badge, tile geometry, header, snip).
  root.setProperty("--fav-star-size", `${val("favStarSize")}px`);
  root.setProperty("--tile-radius", `${val("tileRadius")}px`);
  root.setProperty("--tile-border-w", `${val("tileBorderWidth")}px`);
  root.setProperty("--header-pad-y", `${val("headerPadY")}px`);
  root.setProperty("--snip-preview-vh", `${val("snipPreviewVh")}vh`);
  root.setProperty("--frosted-blur", `${val("frostedBlur")}px`);
  root.setProperty("--hover-lift", `${val("hoverLift")}px`);
  root.setProperty("--lib-width", `${val("libraryWidth")}px`);
  root.setProperty("--sb-width", `${val("scrollbarWidth")}px`);
  root.setProperty("--tooltip-width", `${val("tooltipWidth")}px`);
  root.setProperty("--blur-tiles", `${val("tileBlur")}px`);
  root.setProperty("--blur-media", `${val("mediaBlur")}px`);
  root.setProperty("--dim-unhovered", val("dimOpacity") / 100);
  root.setProperty("--dim-media", val("mediaDimOpacity") / 100);
  root.setProperty("--media-radius", `${val("mediaRadius")}px`);
  BUBBLE_MS = val("bubbleMs");
  TOAST_MS = val("toastMs");
  PREVIEW_MAX = val("previewLen");
  const nameMax = val("nameMaxLen");
  for (const id of ["modal-name", "modal-caption", "view-modal-name"]) {
    const el = $(id);
    if (el) el.maxLength = nameMax;
  }
  setMediaDefaults({ volume: val("videoVolume"), muted: flag("videoMuted"), looped: flag("videoLoop") });
}

// Expert settings whose name alone doesn't explain what they do get a themed
// tooltip (the hover engine picks up [title] and styles it). Key -> i18n tip key,
// convention tip<Key>. Self-explanatory settings are deliberately left out.
const EXPERT_TIPS = [
  // General / creating
  "storeFiles", "varDefaults", "lengthCounter", "promptHistory", "editorMonospace",
  "checkUpdateOnStart", "showUpdates", "importExport", "dimToolbar", "keyboardNav",
  "headerSeparators", "floating",
  // Tiles / copying
  "tilePreview", "tileMenu", "typeBadges", "copyBubble", "copyFlash", "tilePressScale",
  "tileReorder", "chainPrompts", "chainLock", "autoPasteEnter", "closeOnCopy",
  "tooltipDelay", "tooltipTimeout", "iconTooltips",
  // Appearance
  "frostedModals", "accentHeader", "accentScrollbar", "hideScrollbars", "smoothScroll",
  "compactTiles", "tileHoverLift", "gridLines", "pausedIcons",
  // Library / privacy
  "smartSort", "fuzzySearch", "batchOps", "searchSuggest", "searchAutofocus",
  "closeAfterCopy", "libraryCloseToggle", "libraryColsToggle", "confirmDiscard",
  "captureExclusion", "copyHistory", "historyTimestamps", "historyGrouped",
  "dedupCopyLog", "cleanupFiles", "dimUnhovered", "blurTilesUntilHover",
  "blurMediaUntilHover", "hideTileNames",
  // Tools / data
  "dupFinder", "dupImportCheck", "autoBackupBeforeWipe", "clipWatcher", "backupGfs",
  "usageStats",
  // Values that need a unit/behaviour explanation
  "copyCooldownMs", "dblClickMs", "autoPasteDelayMs", "kbFocusFadeMs", "snipPreviewVh",
  "historyPerPrompt", "dupThreshold", "clipMinChars", "clipInboxMax", "historyMax",
  "journalLimit", "previewLen", "nameMaxLen", "searchRecentMax", "maxViews", "gridMax",
  "viewBorder", "statsTopN",
];
const EXPERT_TIP_SET = new Set(EXPERT_TIPS);
// Attach the tooltip for `key` to a rendered expert row, if one is defined.
function withTip(row, key) {
  if (!EXPERT_TIP_SET.has(key)) return row;
  const tipKey = `tip${key[0].toUpperCase()}${key.slice(1)}`;
  const tip = t(tipKey);
  // t() falls back to the key name itself, so never show a raw "tipFoo" as a tooltip.
  if (tip && tip !== tipKey) row.title = tip; // hover engine themes any [title]
  return row;
}

function flagRow(key) {
  const row = document.createElement("label");
  row.className = "field switch-field";
  withTip(row, key);
  const span = document.createElement("span");
  span.textContent = t(flagLabelKey(key));
  // Features that ship OFF by default (opt-in flags) get a "(Standard Aus)" tag so it's
  // clear at a glance which settings are additions beyond the standard configuration.
  if (OPT_FLAG_KEYS.includes(key)) span.textContent += ` ${t("defaultOff")}`;
  const input = document.createElement("input");
  input.type = "checkbox";
  input.className = "switch";
  input.checked = flagState(key);
  input.addEventListener("change", async () => {
    const enabled = input.checked;
    settings.ui_flags = { ...(settings.ui_flags || {}), [key]: enabled };
    try {
      await invoke("set_ui_flag", { key, enabled });
    } catch (err) {
      toast(String(err));
    }
    applyFlags();
    if (GATING_FLAGS.has(key)) renderExpert(); // reveal/hide its parameter rows live
    renderViews();
    // Only these two flags change what a tile contains; the rest are body classes,
    // so a full grid rebuild (and its flash) is unnecessary.
    if (key === "tilePreview" || key === "captions") await renderGrid(true);
    else fitAllTiles();
  });
  row.append(span, input);
  return row;
}

// Almost every expert value only drives a CSS variable, so rebuilding the whole grid
// for it threw away and recreated every tile — visible as a flash through the
// translucent settings overlay. Only these actually change tile DOM.
const GRID_DOM_VALUES = new Set(["previewLen", "gridMax"]);
// Re-fit the tile text to the new box size without touching the DOM.
const refreshTiles = (key) => (GRID_DOM_VALUES.has(key) ? renderGrid(true) : (fitAllTiles(), undefined));

// Live slider preview: reflect the value visually while dragging (CSS vars + at most
// one refit per frame), then persist it on release.
let liveRenderQueued = false;
function previewValue(key, value) {
  settings.ui_values = { ...(settings.ui_values || {}), [key]: value };
  applyValues();
  if (liveRenderQueued) return;
  liveRenderQueued = true;
  requestAnimationFrame(() => { liveRenderQueued = false; refreshTiles(key); });
}
async function commitValue(key, value) {
  settings.ui_values = { ...(settings.ui_values || {}), [key]: value };
  try { await invoke("set_ui_value", { key, value }); } catch (err) { toast(String(err)); }
  applyValues();
  await refreshTiles(key);
}

function valueRow(key) {
  const cfg = EXPERT_VALUES[key];
  const fmt = (v) => `${v}${cfg.unit}`;
  const row = document.createElement("div");
  row.className = "field value-field";
  withTip(row, key);
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(cfg.label);
  const out = document.createElement("span");
  out.className = "value-out";
  const slider = document.createElement("input");
  slider.type = "range";
  slider.className = "value-range";
  // A `stops` list drives a stepped slider (e.g. history length up to a million,
  // coarser as it climbs); everything else is a plain min/max/step slider.
  const cur = cfg.stops
    ? () => cfg.stops[Number(slider.value)]
    : () => Number(slider.value);
  const show = cfg.stops ? (v) => String(v) : fmt;
  if (cfg.stops) {
    slider.min = 0;
    slider.max = cfg.stops.length - 1;
    slider.step = 1;
    const idx = cfg.stops.findIndex((s) => s >= val(key));
    slider.value = idx < 0 ? cfg.stops.length - 1 : idx;
  } else {
    slider.min = cfg.min;
    slider.max = cfg.max;
    slider.step = cfg.step;
    slider.value = val(key);
  }
  out.textContent = show(cur());
  slider.addEventListener("input", () => { out.textContent = show(cur()); previewValue(key, cur()); });
  slider.addEventListener("change", () => commitValue(key, cur()));
  head.append(span, out);
  row.append(head, slider);
  return row;
}

// A preset dropdown (e.g. 1d/3d/7d/30d) plus a free-entry "custom" option.
function selectRow(key) {
  const cfg = EXPERT_SELECTS[key];
  const cur = Number.isFinite(settings.ui_values?.[key]) ? settings.ui_values[key] : cfg.def;
  const row = document.createElement("div");
  row.className = "field value-field expert-dropdown-row"; // label left of the dropdown
  withTip(row, key);
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
    // A 0-stop can carry a special label (e.g. retention "∞ forever") instead of "0d".
    opt.textContent = o === 0 && cfg.zeroLabel ? t(cfg.zeroLabel) : `${o}${cfg.unit}`;
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
  row.className = "field value-field expert-dropdown-row"; // label left of the dropdown
  withTip(row, key);
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

// Folder-path options (expert): current path + a Change (folder picker) and,
// where applicable, a reset-to-default button.
let currentDataDir = "";
let defaultScreenshotDir = "";
const EXPERT_PATHS = {
  screenshotDir: {
    label: "valScreenshotDir",
    gate: "screenshot", // no folder picker when the screenshot tool is off
    get: () => settings.ui_texts?.screenshotDir || defaultScreenshotDir,
    change: async () => {
      const dir = await invoke("pick_folder").catch(() => null);
      if (!dir) return;
      settings.ui_texts = { ...(settings.ui_texts || {}), screenshotDir: dir };
      invoke("set_ui_text", { key: "screenshotDir", value: dir }).catch((e) => toast(String(e)));
      renderExpert();
    },
    onReset: () => {
      settings.ui_texts = { ...(settings.ui_texts || {}), screenshotDir: "" };
      invoke("set_ui_text", { key: "screenshotDir", value: "" }).catch((e) => toast(String(e)));
      renderExpert();
    },
  },
  dataDir: {
    label: "valDataDir",
    get: () => currentDataDir,
    change: async () => {
      const dir = await invoke("pick_folder").catch(() => null);
      if (!dir) return;
      let oldPath;
      try { oldPath = await invoke("set_data_dir", { dir }); } catch (e) { toast(String(e)); return; }
      currentDataDir = dir;
      renderExpert();
      // Offer to delete the old data (default = keep). Message notes the restart.
      if (oldPath && oldPath !== dir) {
        const del = await confirmDialog({
          title: t("valDataDir"),
          message: t("dataDirConfirm"),
          confirmLabel: t("delete"),
          cancelLabel: t("dataDirKeep"),
        });
        if (del) await invoke("delete_data_dir", { path: oldPath }).catch(() => {});
      }
    },
  },
};

function pathRow(key) {
  const cfg = EXPERT_PATHS[key];
  const row = document.createElement("div");
  row.className = "field value-field path-field";
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(cfg.label);
  const out = document.createElement("span");
  out.className = "value-out path-out";
  const cur = cfg.get() || t("pathDefault");
  out.textContent = cur;
  out.title = cur;
  head.append(span, out);
  const btns = document.createElement("div");
  btns.className = "path-btns";
  const change = document.createElement("button");
  change.type = "button";
  change.className = "ghost-btn";
  change.textContent = t("pathChange");
  change.addEventListener("click", cfg.change);
  btns.appendChild(change);
  if (cfg.onReset) {
    const reset = document.createElement("button");
    reset.type = "button";
    reset.className = "ghost-btn";
    reset.textContent = t("pathDefault");
    reset.addEventListener("click", cfg.onReset);
    btns.appendChild(reset);
  }
  row.append(head, btns);
  return row;
}

let expertTab = 0;
let expertQuery = ""; // expert-menu search filter
// Editable default-color palette: one native color picker per hue + reset.
function paletteRow() {
  const row = document.createElement("div");
  row.className = "field palette-field";
  const grid = document.createElement("div");
  grid.className = "palette-grid";
  const cur = COLORS.slice(1);
  const inputs = [];
  for (let i = 0; i < PALETTE_LEN; i++) {
    const c = document.createElement("input");
    c.type = "color";
    c.className = "palette-swatch";
    c.value = HEX_RE.test(cur[i]) ? cur[i] : DEFAULT_COLORS[i + 1];
    c.title = c.value;
    c.addEventListener("input", () => { c.title = c.value; });
    c.addEventListener("change", () => savePalette(inputs.map((x) => x.value)));
    inputs.push(c);
    grid.appendChild(c);
  }
  const reset = document.createElement("button");
  reset.type = "button";
  reset.className = "ghost-btn palette-reset";
  reset.textContent = t("paletteReset");
  reset.addEventListener("click", () => savePalette(null));
  row.append(grid, reset);
  return row;
}

// Persist the palette (null = reset to defaults), remap every saved prompt/view
// color from the old hue to the new one, and refresh every colour surface live.
async function savePalette(colors) {
  const oldHues = COLORS.slice(1);
  const value = colors ? JSON.stringify(colors) : "";
  settings.ui_texts = { ...(settings.ui_texts || {}), palette: value };
  try { await invoke("set_ui_text", { key: "palette", value }); } catch (e) { toast(String(e)); }
  loadPalette();
  const newHues = COLORS.slice(1);
  const pairs = [];
  for (let i = 0; i < PALETTE_LEN; i++) {
    if (oldHues[i] && newHues[i] && oldHues[i] !== newHues[i]) pairs.push([oldHues[i], newHues[i]]);
  }
  if (pairs.length) {
    try { await invoke("remap_colors", { pairs }); } catch (e) { toast(String(e)); }
    // Reload the caches so the grid/views reflect the remap without a restart.
    try {
      const s = await invoke("get_state");
      prompts = s.prompts;
      settings = s.settings;
      loadPalette();
    } catch (_) { /* keep going with the in-memory state */ }
  }
  refreshColorUI();
  renderViews();
  renderExpert();
  await renderGrid(true);
}

// Translation-key lookup per setting kind — used by the expert-menu search.
const EXPERT_LABEL_KEYS = {
  flags: (k) => flagLabelKey(k),
  values: (k) => EXPERT_VALUES[k].label,
  selects: (k) => EXPERT_SELECTS[k].label,
  dropdowns: (k) => EXPERT_DROPDOWNS[k].label,
  paths: (k) => EXPERT_PATHS[k].label,
};
// The def a given setting kind lives in — so its optional `gate` can be read.
const EXPERT_DEFS = { values: EXPERT_VALUES, selects: EXPERT_SELECTS, dropdowns: EXPERT_DROPDOWNS, paths: EXPERT_PATHS };
// Flags that own gated rows. Only those change the menu's shape when toggled, so
// only those need a re-render — the rest are body classes and re-rendering the
// whole tab for them just made the list flicker.
const GATING_FLAGS = new Set(
  [
    ...Object.values(EXPERT_DEFS).flatMap((defs) => Object.values(defs).map((d) => d.gate)),
    ...EXPERT_TABS.flatMap((tab) => tab.groups.flatMap((g) => (g.actions || []).map((a) => a.gate))),
  ].filter(Boolean).map((g) => g.replace(/^!/, ""))
);
// Expert-menu action buttons (id -> handler); populated by the features that own them.
const EXPERT_ACTIONS = {};
// A search matches if the term is in the active language OR in English — English
// is the lingua franca, so a non-native speaker's query still finds the setting.
function i18nHit(key, q) {
  return (I18N[LANG][key] || "").toLowerCase().includes(q) || (I18N.en[key] || "").toLowerCase().includes(q);
}

// The "off by default" extras groups start folded (kept tidy until wanted).
const DEFAULT_COLLAPSED_GROUPS = [];
// Collapsed expert categories, persisted as a JSON title list in ui_texts. Unset
// (first ever open) → everything expanded; after that the stored list wins.
function expertCollapsedSet() {
  const stored = settings.ui_texts?.expertCollapsed;
  if (stored == null) return new Set(DEFAULT_COLLAPSED_GROUPS);
  try { return new Set(JSON.parse(stored)); } catch (_) { return new Set(); }
}
// Collapsing is a pure CSS state, so flip the class on the section itself. Going
// through renderExpert() rebuilt the whole tab (and re-fetched the backup/stats
// panels), which flashed on every click.
function toggleExpertGroup(title, sec, fill) {
  const set = expertCollapsedSet();
  const collapsed = !set.has(title); // the state we are switching to
  collapsed ? set.add(title) : set.delete(title);
  const value = JSON.stringify([...set]);
  settings.ui_texts = { ...(settings.ui_texts || {}), expertCollapsed: value };
  invoke("set_ui_text", { key: "expertCollapsed", value }).catch(() => {});
  if (!sec) { renderExpert(); return; }
  sec.classList.toggle("collapsed", collapsed);
  if (!collapsed) fill?.(); // custom panels fill on first open
}

function renderExpert() {
  const box = $("expert-flags");
  const tabBar = $("expert-tabs-bar");
  const keepScroll = box.scrollTop; // survive live re-renders (e.g. a toggled gate)
  box.innerHTML = "";
  tabBar.innerHTML = "";
  if (expertTab >= EXPERT_TABS.length) expertTab = 0;
  const q = expertQuery.trim().toLowerCase();
  const searching = q.length > 0;
  // Tab bar lives in the fixed header (stays visible while the rows scroll); it is
  // hidden while searching, when every tab's groups are scanned at once.
  if (!searching) {
    const bar = document.createElement("div");
    bar.className = "expert-tabs";
    EXPERT_TABS.forEach((tab, i) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "expert-tab" + (i === expertTab ? " active" : "");
      b.textContent = t(tab.title);
      b.addEventListener("click", () => { expertTab = i; box.scrollTop = 0; renderExpert(); });
      bar.appendChild(b);
    });
    tabBar.appendChild(bar);
  }
  const tabs = searching ? EXPERT_TABS : [EXPERT_TABS[expertTab]];
  const collapsed = expertCollapsedSet();
  let shown = 0;
  for (const tab of tabs) {
    for (const group of tab.groups) {
      const groupHit = searching && i18nHit(group.title, q);
      const keep = (kind) => (group[kind] || []).filter((k) => {
        if (searching) return groupHit || i18nHit(EXPERT_LABEL_KEYS[kind](k), q);
        // Not searching: drop a row whose gating feature is currently off.
        return gateOpen(EXPERT_DEFS[kind]?.[k]?.gate);
      });
      const flags = keep("flags");
      const values = keep("values");
      const selects = keep("selects");
      const dropdowns = keep("dropdowns");
      const paths = keep("paths");
      const palette = group.palette && (!searching || groupHit || i18nHit("expGroupColors", q));
      const actions = (group.actions || []).filter((a) => searching ? (groupHit || i18nHit(a.label, q)) : gateOpen(a.gate));
      if (searching && !flags.length && !values.length && !selects.length && !dropdowns.length && !paths.length && !palette && !actions.length) continue;
      shown++;
      // Categories collapse on click (persisted); searching always shows them.
      const isCollapsed = !searching && collapsed.has(group.title);
      const sec = document.createElement("div");
      sec.className = "expert-group" + (isCollapsed ? " collapsed" : "");
      const head = document.createElement("div");
      head.className = "expert-group-title";
      head.textContent = t(group.title);
      if (!searching) {
        head.classList.add("collapsible");
        head.addEventListener("click", () => toggleExpertGroup(group.title, sec));
      }
      sec.appendChild(head);
      if (searching) {
        // Flat list while searching (grouping a match under a non-matching toggle
        // would hide it); adjacency only matters for the calm, browsable view.
        for (const key of flags) sec.appendChild(flagRow(key));
        for (const key of values) sec.appendChild(valueRow(key));
        for (const key of selects) sec.appendChild(selectRow(key));
      } else {
        // A param whose gate names a toggle in THIS group renders right beneath that
        // toggle (and vanishes with it), so a feature and its sub-settings stay together.
        const ownerIn = (kind, k) => {
          const g = EXPERT_DEFS[kind]?.[k]?.gate;
          return g && g[0] !== "!" && (group.flags || []).includes(g) ? g : null;
        };
        const owned = {};
        const orphan = { values: [], selects: [], paths: [] };
        for (const kind of ["values", "selects", "paths"]) {
          for (const k of { values, selects, paths }[kind]) {
            const o = ownerIn(kind, k);
            (o ? (owned[o] ||= []) : orphan[kind]).push([kind, k]);
          }
        }
        const ROW = { selects: selectRow, paths: pathRow, values: valueRow };
        const render = ([kind, k]) => sec.appendChild(ROW[kind](k));
        for (const key of flags) { sec.appendChild(flagRow(key)); (owned[key] || []).forEach(render); }
        orphan.values.forEach(render);
        orphan.selects.forEach(render);
        orphan.paths.forEach(render);
      }
      if (dropdowns.length) {
        const pair = document.createElement("div");
        pair.className = "dropdown-pair";
        for (const key of dropdowns) pair.appendChild(dropdownRow(key));
        sec.appendChild(pair);
      }
      if (searching) for (const key of paths) sec.appendChild(pathRow(key));
      if (palette) sec.appendChild(paletteRow());
      for (const a of actions) {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "ghost-btn expert-action";
        btn.textContent = t(a.label);
        btn.addEventListener("click", () => EXPERT_ACTIONS[a.id]?.());
        sec.appendChild(btn);
      }
      box.appendChild(sec);
    }
  }
  if (searching && !shown) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("libraryNoResults");
    box.appendChild(empty);
  }
  box.scrollTop = keepScroll;
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
  loadPalette();
  refreshColorUI();
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
  // Expert rows are single-line; keep the panel ~2 columns wide instead of the
  // wider 3-across settings width.
  settingsEl.classList.toggle("expert-active", on);
  $("settings-title").textContent = on ? t("expertMenu") : t("settings");
  if (on) renderExpert();
}

// ---- View helpers ----
function activeView() {
  return settings.views.find((v) => v.id === settings.active_view) || settings.views[0];
}
const gridKeyOf = (v) => `${v.cols}x${v.rows}`;
const layoutOf = (v) => v.layouts?.[gridKeyOf(v)] || {};

// ---- Favorites view: a live, auto-sized grid of every starred prompt.
// Enabled by the favView setting (off by default); the header star toggles it.
let favView = false; // currently showing the favorites grid
const favViewEnabled = () => settings.ui_flags?.favView === true;

// Favorites layout: prompt id -> [col,row]. Free placement (gaps allowed), saved
// in ui_texts; the grid grows to fit the count and to cover any hand-placed cell.
function favLayoutMap() {
  try { const m = JSON.parse(settings.ui_texts?.favLayout || "{}"); return m && typeof m === "object" ? m : {}; } catch (_) { return {}; }
}
function saveFavLayout(map) {
  const value = JSON.stringify(map);
  settings.ui_texts = { ...(settings.ui_texts || {}), favLayout: value };
  invoke("set_ui_text", { key: "favLayout", value }).catch(() => {});
}

// Star button visibility (setting + expert flag) + pressed state. In the favorites
// view the grid-size control stays visible (so the size is shown) but read-only —
// the favorites grid sizes itself.
function refreshFavViewUi() {
  const btn = $("fav-view-btn");
  if (btn) {
    btn.classList.toggle("hidden", !(favViewEnabled() && flag("favViewButton")));
    btn.classList.toggle("active", favView);
  }
  const qg = $("quick-grid");
  if (qg) qg.classList.toggle("qg-readonly", favView);
  $("qg-cols").readOnly = favView;
  $("qg-rows").readOnly = favView;
}

function setFavView(on) {
  favView = on && favViewEnabled();
  renderGrid(true);
}

// Favorites grid with FREE placement inside a grid that always fits the count:
// a favorite keeps the cell it is dragged to (gaps allowed), but the grid resizes
// the moment the count changes, and any button now outside it is re-fitted.
function renderFavGrid() {
  const favs = prompts.filter((p) => p.favorite);
  const n = favs.length;
  const ids = new Set(favs.map((p) => p.id));
  const layout = favLayoutMap();
  let changed = false;
  for (const id of Object.keys(layout)) if (!ids.has(id)) { delete layout[id]; changed = true; }

  // Size fits the count (landscape); an expert cap can pin the column count.
  let cols = Math.max(1, Math.ceil(n / Math.max(1, Math.floor(Math.sqrt(n)))));
  const capCols = val("favMaxCols");
  if (capCols > 0) cols = Math.min(cols, capCols);
  let rows = Math.max(1, Math.ceil(n / cols));
  while (cols * rows < n) rows++; // always room for every favorite

  // Placed cells outside the (re)computed grid are dropped and re-placed below,
  // so shrinking the grid re-fits any button that fell off it.
  const occupied = new Map();
  for (const [id, cell] of Object.entries(layout)) {
    if (cell[0] < cols && cell[1] < rows && !occupied.has(cellKey(cell[0], cell[1]))) {
      occupied.set(cellKey(cell[0], cell[1]), id);
    } else { delete layout[id]; changed = true; }
  }
  // Newly starred favorites (not yet placed) drop into the first free cell.
  for (const p of favs) {
    if (layout[p.id]) continue;
    const cell = firstFree(occupied, cols, rows);
    if (cell) { layout[p.id] = cell; occupied.set(cellKey(...cell), p.id); changed = true; }
  }
  if (changed) saveFavLayout(layout);

  const byCell = new Map();
  for (const p of favs) { const c = layout[p.id]; if (c) byCell.set(cellKey(c[0], c[1]), p); }
  gridEl.style.gridTemplateColumns = `repeat(${cols}, 1fr)`;
  gridEl.style.gridTemplateRows = `repeat(${rows}, 1fr)`;
  $("qg-cols").value = cols; // show (read-only) the auto-computed favorites size
  $("qg-rows").value = rows;
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
  if (!n) {
    const empty = document.createElement("div");
    empty.className = "fav-empty";
    empty.textContent = t("favViewEmpty");
    gridEl.appendChild(empty);
  }
  pruneVideoCache();
  renderViews();
  refreshFavViewUi();
  fitAllTiles();
}

// Move a favorite to the drop cell, swapping with any occupant. Dropping on an
// empty cell just moves it there and frees the old one — gaps are preserved.
async function placeFav(id, col, row) {
  if (!flag("favViewReorder")) return;
  const layout = favLayoutMap();
  const ids = new Set(prompts.filter((p) => p.favorite).map((p) => p.id));
  for (const k of Object.keys(layout)) if (!ids.has(k)) delete layout[k];
  const old = layout[id];
  const occupant = Object.entries(layout).find(([oid, c]) => oid !== id && c[0] === col && c[1] === row);
  if (occupant) { if (old) layout[occupant[0]] = old; else delete layout[occupant[0]]; }
  layout[id] = [col, row];
  saveFavLayout(layout);
  await renderGrid(true);
}

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
  hideTileTip(); // the hovered tile may be replaced by this render
  if (!skipFetch) {
    try {
      const s = await invoke("get_state");
      prompts = s.prompts;
      settings = s.settings;
    } catch (e) {
      // IPC hiccup: keep the last good state instead of throwing into the
      // global handler and leaving a half-rendered grid.
      toast(String((e && e.message) || e));
      if (!settings || !prompts) return;
    }
  }
  if (favView && !favViewEnabled()) favView = false; // setting switched off underneath us
  if (favView) { renderFavGrid(); return; }
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
  refreshFavViewUi();
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
    // While the favorites view is open, no normal view is the active one.
    btn.className = "view-btn" + (!favView && v.id === settings.active_view ? " active" : "");
    if (v.color) {
      btn.classList.add("colored");
      btn.style.setProperty("--view-color", v.color);
    }
    btn.textContent = v.name;
    btn.addEventListener("click", async () => {
      favView = false; // picking a normal view leaves the favorites grid
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
    add.setAttribute("aria-label", t("addView")); // aria only — no hover tooltip
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
  gridField: $("view-modal-grid-field"),
  cols: $("view-modal-cols"),
  rows: $("view-modal-rows"),
  confirm: $("view-modal-confirm"),
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
  viewModal.title.textContent = view ? t("editViewTitle") : t("addView").replace(/^\+\s*/, "");
  viewModal.name.value = view ? view.name : "";
  renderViewColor();
  // Grid size (columns × rows) is editable only for an existing view — a new one
  // starts at the default and can be resized here afterwards.
  viewModal.gridField.classList.toggle("hidden", !view);
  if (view) {
    const gm = val("gridMax");
    viewModal.cols.max = gm; viewModal.rows.max = gm;
    viewModal.cols.value = String(view.cols);
    viewModal.rows.value = String(view.rows);
  }
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
      await invoke("set_view_color", { id: viewModalId, color: viewModalColor });
      // Apply the grid size too (clamped to the expert gridMax).
      const view = settings.views.find((v) => v.id === viewModalId);
      const cols = clampGrid(viewModal.cols.value, view?.cols ?? 1);
      const rows = clampGrid(viewModal.rows.value, view?.rows ?? 1);
      settings = await invoke("set_view_grid", { id: viewModalId, cols, rows });
    } else {
      settings = await invoke("add_view", { name, color: viewModalColor });
    }
  } catch (err) {
    toast(String(err));
    return;
  }
  closeViewModal();
  renderViews();
  await renderGrid(true);
}

// ---- Prompt chaining ----
// The chain button turns several TEXT buttons into one clipboard copy: click it,
// then click text buttons in the order you want them; each shows its position (1,
// 2, 3 …) and the joined text (newline-separated) lands on the clipboard after
// every pick. It auto-ends after a paste (Ctrl+V while focused, or leaving the
// window to paste elsewhere) or when the button is clicked again. Only text
// prompts qualify — image/file/video buttons copy media, which can't be joined.
let chainMode = false;
let chainLock = false; // "always on" — survives window blur until turned off by hand
const chainSel = []; // selected prompt ids, in pick order
const chainTexts = new Map(); // id -> resolved copy text (variables already filled)

const isTextPrompt = (p) => !!p && !p.copy_image && !p.file_path;

// Chained prompts are joined with a blank line (two newlines) between them.
const CHAIN_SEP = "\n\n";

function chainText() {
  return chainSel.map((id) => chainTexts.get(id) ?? "").join(CHAIN_SEP);
}

function pushChainClipboard() {
  if (!chainSel.length) return;
  invoke("copy_text", { text: chainText() }).catch((e) => toast(String(e)));
}

// Reflect one tile's chain state (skip / picked + order badge) without a re-render.
function applyChainToTile(tile, p) {
  tile.classList.toggle("chain-skip", chainMode && !isTextPrompt(p));
  const idx = chainMode ? chainSel.indexOf(p.id) : -1;
  tile.classList.toggle("chain-picked", idx >= 0);
  tile.querySelector(".tile-chain-badge")?.remove();
  if (idx >= 0) {
    const badge = document.createElement("span");
    badge.className = "tile-chain-badge";
    badge.textContent = String(idx + 1);
    tile.appendChild(badge);
  }
}

function refreshChainUi() {
  for (const tile of gridEl.querySelectorAll(".tile")) {
    const p = prompts.find((x) => x.id === tile.dataset.id);
    if (p) applyChainToTile(tile, p);
  }
}

async function toggleChainPick(p) {
  if (!isTextPrompt(p)) return; // only text buttons chain
  const i = chainSel.indexOf(p.id);
  if (i >= 0) {
    chainSel.splice(i, 1); // deselect → the rest renumber
    chainTexts.delete(p.id);
  } else {
    // Resolve {#{variables}#} right when the button is picked into the chain.
    let text = p.text || p.name || "";
    if (flag("promptVars") && extractVars(p.text).length) {
      const values = await promptVarsDialog(extractVars(p.text));
      if (!values) return; // cancelled → leave it unselected
      text = fillVars(p.text, values);
    }
    chainSel.push(p.id);
    chainTexts.set(p.id, text);
  }
  refreshChainUi();
  pushChainClipboard();
}

function setChainMode(on) {
  if (on && !flag("chainPrompts")) return;
  chainMode = on;
  if (!on) { chainSel.length = 0; chainTexts.clear(); chainLock = false; }
  document.body.classList.toggle("chain-mode", on);
  const btn = $("chain-btn");
  if (btn) {
    btn.classList.toggle("active", on);
    btn.classList.toggle("chain-locked", chainLock);
    btn.setAttribute("aria-pressed", String(on));
  }
  refreshChainUi();
}

// Latch chaining "always on": a window switch normally resets the chain, but a
// locked chain survives until the user turns it off by hand. The button fills with
// the accent colour to mark the locked (infinite-repeat) state.
function setChainLock() {
  if (!flag("chainPrompts") || !flag("chainLock")) return;
  chainLock = true;
  setChainMode(true); // turning on never clears the current picks; refreshes the button state
}

// Each click advances one fixed step: off → on → latched (∞) → off. Deterministic
// order, no double-click timing to misfire. A plain toggle when latching is off.
function onChainClick() {
  if (!flag("chainLock")) { setChainMode(!chainMode); return; }
  if (!chainMode) setChainMode(true); // off → on
  else if (!chainLock) setChainLock(); // on → latched
  else setChainMode(false); // latched → off
}

// ---- Themed tooltips: ONE shared popup for every hover hint — rich tile previews
// AND the native `title` on any button/icon — so nothing falls back to the OS
// tooltip. A raw `title` is converted to `data-tip` on first hover (removed so the
// OS tip never fires); tiles carry `data-tip-name`/`data-tip-body` for the richer
// name + preview + hint layout.
let tipEl = null;
let tipCur = null;   // element the tooltip is currently anchored to
let tipTimer = null;   // auto-hide (AFK) timer once a tip is shown
let tipShowTimer = null; // hover show-delay timer before a tip appears
let tipPending = null;   // element whose tip is scheduled but not yet shown
let tipExpired = null; // element whose tooltip timed out; suppressed until another is hovered
let tipPtrX = 0, tipPtrY = 0; // last pointer position (a delayed tip places here)
function clearTipShow() {
  if (tipShowTimer) { clearTimeout(tipShowTimer); tipShowTimer = null; }
  tipPending = null;
}
function hideTileTip() {
  tipCur = null;
  clearTipShow();
  if (tipTimer) { clearTimeout(tipTimer); tipTimer = null; }
  tipEl?.classList.remove("show");
}
// A greyed clear-X inside a text field: brightens on hover, empties the field on
// click. Wraps the input so the button can sit at its right edge.
const CLEAR_SVG = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" d="M7 7l10 10M17 7 7 17"/></svg>';
// Dialog icons: a stack of records for the data take-over, a padlock for the
// password prompt.
const TAKEOVER_SVG = '<svg viewBox="0 0 24 24" aria-hidden="true"><ellipse cx="12" cy="5.5" rx="7.5" ry="3" fill="none" stroke="currentColor" stroke-width="1.8"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M4.5 5.5v13c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-13M4.5 12c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3"/></svg>';
const LOCK_SVG = '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="10" width="16" height="11" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.8"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M8 10V7a4 4 0 0 1 8 0v3"/><circle cx="12" cy="15.5" r="1.6" fill="currentColor"/></svg>';
// Reveal toggle for password fields (backup settings + the restore prompt).
const EYE_SVG ='<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M2 12s3.6-6 10-6 10 6 10 6-3.6 6-10 6-10-6-10-6Z"/><circle cx="12" cy="12" r="2.6" fill="none" stroke="currentColor" stroke-width="2"/></svg>';
function wireClear(el) {
  if (!el || el.dataset.clearWired || el.readOnly) return;
  const isTa = el.tagName === "TEXTAREA";
  if (!isTa) {
    const type = (el.getAttribute("type") || "text").toLowerCase();
    if (!["text", "search", "number", ""].includes(type)) return;
    if (el.classList.contains("grid-mini")) return; // too tiny for an X
  }
  el.dataset.clearWired = "1";
  const wrap = document.createElement("span");
  wrap.className = "clear-wrap" + (isTa ? " clear-wrap-ta" : "");
  el.parentNode.insertBefore(wrap, el);
  wrap.appendChild(el);
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "clear-btn";
  btn.tabIndex = -1;
  btn.setAttribute("aria-label", t("clearField"));
  btn.innerHTML = CLEAR_SVG;
  wrap.appendChild(btn);
  const upd = () => btn.classList.toggle("show", !!el.value);
  el.addEventListener("input", upd);
  btn.addEventListener("mousedown", (e) => e.preventDefault()); // keep focus
  btn.addEventListener("click", () => {
    el.value = "";
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.focus();
    upd();
  });
  upd();
}
// Wire every text/number input + textarea (idempotent) — call again after any render
// that creates new fields (expert menu, dialogs, backups panel).
function wireClearButtons(root = document) {
  const sel = "input:not([type=checkbox]):not([type=radio]):not([type=color]):not([type=range]), textarea";
  for (const el of root.querySelectorAll(sel)) wireClear(el);
}

function wireTooltips() {
  tipEl = document.createElement("div");
  tipEl.className = "tile-tip";
  tipEl.setAttribute("role", "tooltip");
  tipEl.innerHTML = '<div class="tile-tip-name"></div><div class="tile-tip-body"></div><div class="tile-tip-hint"></div>';
  document.body.appendChild(tipEl);
  const nameEl = tipEl.querySelector(".tile-tip-name");
  const bodyEl = tipEl.querySelector(".tile-tip-body");
  const hintEl = tipEl.querySelector(".tile-tip-hint");
  const OFF = 16, PAD = 12;
  const placeAt = (cx, cy) => {
    const r = tipEl.getBoundingClientRect();
    let x = cx + OFF;
    let y = cy + OFF;
    if (x + r.width + PAD > innerWidth) x = cx - OFF - r.width;
    if (y + r.height + PAD > innerHeight) y = cy - OFF - r.height;
    const z = uiZoom(); // fixed popup lives inside the zoomed body
    tipEl.style.left = `${Math.max(PAD, x) / z}px`;
    tipEl.style.top = `${Math.max(PAD, y) / z}px`;
  };
  // Actually reveal the tip for `el` (called immediately or after the show-delay).
  const showNow = (el) => {
    const rich = !!el.dataset.tipName;
    const name = el.dataset.tipName || el.dataset.tip || "";
    if (!name) return;
    tipExpired = null;
    tipCur = el;
    // Plain (icon/label) tooltips get a compact single-line style; tiles get the
    // richer name + preview + hint box.
    tipEl.classList.toggle("plain", !rich);
    if (rich) {
      nameEl.textContent = name;
      bodyEl.textContent = el.dataset.tipBody || "";
      hintEl.textContent = t("tileTooltip") + (flag("autoPaste") ? " · " + t("autoPasteHint") : "");
    } else {
      // Plain setting/icon tip: break "<lead sentence>. <details>" into two tidy
      // paragraphs (e.g. the "Expert menu: …" clause drops onto its own line).
      const sp = name.indexOf(". ");
      if (sp > 0 && sp < name.length - 2) {
        nameEl.textContent = name.slice(0, sp + 1);
        bodyEl.textContent = name.slice(sp + 2);
      } else {
        nameEl.textContent = name;
        bodyEl.textContent = "";
      }
      hintEl.textContent = "";
    }
    placeAt(tipPtrX, tipPtrY);
    tipEl.classList.add("show");
    // AFK cleanness: auto-hide after the timeout, then don't reshow on this same
    // element until the pointer visits a different one.
    if (flag("tooltipTimeout")) {
      tipTimer = setTimeout(() => { tipExpired = el; hideTileTip(); }, val("tooltipTimeoutMs"));
    }
  };
  document.addEventListener("pointerover", (e) => {
    if (drag) return; // no hover hints mid-drag
    if (!flag("tooltipsEnabled")) { if (tipCur || tipPending) hideTileTip(); return; } // master switch off
    const el = e.target.closest?.("[data-tip-name], [data-tip], [title]");
    // Minimize/maximize/close never get a hint — they are universally understood
    // and a tooltip right under the cursor there is only in the way.
    if (!el || el.closest(".win-controls")) { if (tipCur) hideTileTip(); clearTipShow(); return; }
    if (el.hasAttribute("title")) { el.dataset.tip = el.getAttribute("title"); el.removeAttribute("title"); }
    const rich = !!el.dataset.tipName;
    if (!rich && !flag("iconTooltips")) { hideTileTip(); return; } // icon hints off
    const name = el.dataset.tipName || el.dataset.tip || "";
    if (!name) { hideTileTip(); return; }
    if (el === tipCur) return;     // already showing on this element
    if (el === tipPending) return; // already scheduled for this element
    if (el === tipExpired) return; // timed out here — hover elsewhere to reset
    // New target: drop any pending/showing tip, then (re)arm the show-delay so
    // casually sweeping the pointer across the window shows nothing.
    clearTipShow();
    if (tipCur && tipCur !== el) hideTileTip();
    tipPtrX = e.clientX; tipPtrY = e.clientY;
    const delay = flag("tooltipDelay") ? val("tooltipDelayMs") : 0;
    if (delay <= 0) { showNow(el); return; }
    tipPending = el;
    tipShowTimer = setTimeout(() => {
      tipShowTimer = null;
      if (tipPending === el) { tipPending = null; showNow(el); }
    }, delay);
  });
  document.addEventListener("pointermove", (e) => {
    tipPtrX = e.clientX; tipPtrY = e.clientY;
    if (tipCur) placeAt(e.clientX, e.clientY);
  });
  document.addEventListener("pointerout", (e) => {
    if (tipPending && !tipPending.contains(e.relatedTarget)) clearTipShow();
    if (tipCur && !tipCur.contains(e.relatedTarget)) hideTileTip();
  });
  document.addEventListener("pointerdown", hideTileTip); // out of the way while clicking/dragging
}

// ---- Custom window controls (the native OS titlebar is disabled in
// tauri.conf.json, so minimize / maximize / close live in the header). close()
// fires the same CloseRequested event the native X did, so tray behaviour is kept.
async function wireWindowControls() {
  const win = window.__TAURI__?.window?.getCurrentWindow?.();
  if (!win) return;
  const syncMax = async () => {
    try {
      const max = await win.isMaximized();
      document.body.classList.toggle("win-maximized", max);
      const b = $("win-max");
      if (b) { b.title = t(max ? "winRestore" : "winMaximize"); delete b.dataset.tip; }
    } catch (_) {}
  };
  $("win-min")?.addEventListener("click", () => { win.minimize().catch(() => {}); });
  $("win-max")?.addEventListener("click", async () => {
    try { await win.toggleMaximize(); } catch (_) {}
    syncMax();
  });
  $("win-close")?.addEventListener("click", () => { win.close().catch(() => {}); });
  try { await win.onResized(syncMax); } catch (_) {}
  syncMax();
}

// Themed recent-searches dropdown for a search field, replacing the browser's
// unstyled native autofill. Terms live in ui_texts (DPAPI-encrypted like the rest),
// most-recent first, deduped, capped. Picking one re-runs the field's own search.
function wireSearchSuggest(input, storeKey) {
  if (!input) return;
  input.setAttribute("autocomplete", "off");
  const pop = document.createElement("div");
  pop.className = "search-suggest hidden";
  document.body.appendChild(pop);
  const load = () => {
    try { const a = JSON.parse(settings.ui_texts?.[storeKey] || "[]"); return Array.isArray(a) ? a : []; }
    catch { return []; }
  };
  const remember = (term) => {
    term = term.trim();
    if (term.length < 2 || !flag("searchSuggest")) return;
    const list = load().filter((x) => x.toLowerCase() !== term.toLowerCase());
    list.unshift(term);
    const value = JSON.stringify(list.slice(0, val("searchRecentMax")));
    settings.ui_texts = { ...(settings.ui_texts || {}), [storeKey]: value };
    invoke("set_ui_text", { key: storeKey, value }).catch(() => {});
  };
  const forget = (term) => {
    const value = JSON.stringify(load().filter((x) => x.toLowerCase() !== term.toLowerCase()));
    settings.ui_texts = { ...(settings.ui_texts || {}), [storeKey]: value };
    invoke("set_ui_text", { key: storeKey, value }).catch(() => {});
  };
  const hide = () => pop.classList.add("hidden");
  const show = () => {
    if (!flag("searchSuggest")) { hide(); return; }
    const q = input.value.trim().toLowerCase();
    const items = load().filter((x) => !q || x.toLowerCase().includes(q)).slice(0, val("searchRecentMax"));
    pop.innerHTML = "";
    if (!items.length) { hide(); return; }
    for (const term of items) {
      const row = document.createElement("div");
      row.className = "search-suggest-item";
      const label = document.createElement("span");
      label.className = "search-suggest-text";
      label.textContent = term;
      label.addEventListener("mousedown", (e) => {
        e.preventDefault(); // keep focus so blur-hide doesn't beat the click
        input.value = term;
        input.dispatchEvent(new Event("input", { bubbles: true }));
        remember(term);
        hide();
      });
      const del = document.createElement("button");
      del.type = "button";
      del.className = "search-suggest-del";
      del.textContent = "×";
      del.title = t("delete");
      del.setAttribute("aria-label", t("delete"));
      del.addEventListener("mousedown", (e) => {
        e.preventDefault();
        e.stopPropagation();
        forget(term);
        show(); // re-render the shortened list, keeping the popup open
      });
      row.append(label, del);
      pop.appendChild(row);
    }
    // Divide by the body zoom (uiScale): fixed popups live inside the zoomed body.
    const z = uiZoom();
    const r = input.getBoundingClientRect();
    // Size to the terms (short), not the full search field — a full-width bar over a
    // wide library search looked oversized. Clamp: at least 220px, at most the field.
    pop.style.left = `${r.left / z}px`;
    pop.style.top = `${(r.bottom + 4) / z}px`;
    pop.style.width = "";
    pop.style.minWidth = `${Math.min(220, r.width / z)}px`;
    pop.style.maxWidth = `${r.width / z}px`;
    pop.classList.remove("hidden");
  };
  // Open on an explicit click/type, NOT on the focus the dialog may grab on open.
  input.addEventListener("pointerdown", show);
  input.addEventListener("input", show);
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") { remember(input.value); hide(); } });
  input.addEventListener("blur", () => { remember(input.value); setTimeout(hide, 120); });
}

// ---- Fullscreen media viewer (lightbox): click a preview to zoom (wheel) + pan
// (drag). Stays inside the app window; X / Escape / backdrop-click close it.
let lbScale = 1, lbX = 0, lbY = 0, lbDrag = null;
function lbMedia() {
  return $("lightbox-img").classList.contains("hidden") ? $("lightbox-video") : $("lightbox-img");
}
function lbApply() {
  lbMedia().style.transform = `translate(${lbX}px, ${lbY}px) scale(${lbScale})`;
  $("lightbox-stage").classList.toggle("zoomed", lbScale > 1);
}
function openLightbox(src, isVideo) {
  if (!src) return;
  lbScale = 1; lbX = 0; lbY = 0;
  const img = $("lightbox-img"), vid = $("lightbox-video");
  img.classList.toggle("hidden", isVideo);
  vid.classList.toggle("hidden", !isVideo);
  if (isVideo) { vid.src = src; vid.play?.().catch(() => {}); }
  else { vid.pause?.(); vid.removeAttribute("src"); img.src = src; }
  lbApply();
  $("lightbox").classList.remove("hidden");
}
function closeLightbox() {
  if ($("lightbox").classList.contains("hidden")) return;
  $("lightbox").classList.add("hidden");
  const vid = $("lightbox-video");
  vid.pause?.(); vid.removeAttribute("src");
  $("lightbox-img").removeAttribute("src");
}
function wireLightbox() {
  const stage = $("lightbox-stage");
  $("lightbox-close").addEventListener("click", closeLightbox);
  // Backdrop click closes only when not zoomed (a zoomed click may be a pan).
  stage.addEventListener("click", (e) => { if (lbScale === 1 && e.target === stage) closeLightbox(); });
  stage.addEventListener("wheel", (e) => {
    e.preventDefault();
    lbScale = Math.min(8, Math.max(1, lbScale * (e.deltaY < 0 ? 1.15 : 1 / 1.15)));
    if (lbScale === 1) { lbX = 0; lbY = 0; }
    lbApply();
  }, { passive: false });
  stage.addEventListener("pointerdown", (e) => {
    if (lbScale <= 1 || e.target.closest(".lightbox-close")) return;
    lbDrag = { x: e.clientX, y: e.clientY };
    stage.classList.add("grabbing");
    stage.setPointerCapture?.(e.pointerId);
  });
  stage.addEventListener("pointermove", (e) => {
    if (!lbDrag) return;
    const z = uiZoom();
    lbX += (e.clientX - lbDrag.x) / z;
    lbY += (e.clientY - lbDrag.y) / z;
    lbDrag.x = e.clientX; lbDrag.y = e.clientY;
    lbApply();
  });
  const endDrag = () => { lbDrag = null; stage.classList.remove("grabbing"); };
  stage.addEventListener("pointerup", endDrag);
  stage.addEventListener("pointercancel", endDrag);
}

function buildTile(p) {
  const tile = document.createElement("div");
  tile.className = "tile";
  tile.dataset.id = p.id;
  const raw = p.file_path || p.text || "";
  const preview = PREVIEW_MAX <= 0 ? "" : raw.length > PREVIEW_MAX ? `${raw.slice(0, PREVIEW_MAX)}…` : raw;
  // Themed hover tooltip (rendered by wireTooltips' shared popup, not the native
  // title): stash name + preview on the tile; the popup wraps + grows tall so long
  // prompts read comfortably at a constrained width.
  if (flag("tilePreview")) {
    tile.dataset.tipName = p.name;
    if (preview && preview !== p.name) tile.dataset.tipBody = preview;
  }

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

  // The actions menu (⋯ button + right-click) can be switched off entirely.
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
  if (chainMode) applyChainToTile(tile, p); // keep badges/skip across re-renders
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
  // Ensure a freshly added video actually starts — the autoplay attribute alone
  // can be unreliable for dynamically created elements. Muted videos may play.
  if (flag("videoAutoplay")) video.play().catch(() => {});
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
  // No reorder while chaining (a click selects) or when reorder is switched off in
  // the expert menu — but always allow finishing a library-placed tile or a
  // favorites-view reorder (reordering favorites is intrinsic to that view).
  if (!drag.moved && (chainMode || (!flag("tileReorder") && !drag.fromLibrary && !favView))) return;
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
      if (chainMode) { if (p) await toggleChainPick(p); return; } // pick (asks vars), don't copy
      // A prompt with variables ALWAYS routes through the dialog — the double-click
      // paste shortcut would otherwise fire on the 2nd click and paste stale clipboard
      // without ever asking for the variables. A click only copies, never pastes.
      if (p && flag("promptVars") && !p.copy_image && !p.file_path && extractVars(p.text).length) {
        await copyTextWithVars(p, el);
        return;
      }
      if (autoPasteDouble(id)) { await autoPasteNow(el); return; } // F19: 2nd fast click pastes
      if (copyOnCooldown(id)) return; // same prompt copied moments ago
      if (await invoke("copy_prompt", { id }).catch((e) => { toast(String(e)); return false; })) {
        showCopied(el);
        recordCopy(id);
        maybeCloseOnCopy();
      }
    }
    return;
  }
  const cell = cellAt(e.clientX, e.clientY);
  if (!cell) return;
  if (favView) { await placeFav(id, Number(cell.dataset.col), Number(cell.dataset.row)); return; }
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
function showCopied(tile, label) {
  tile.classList.add("copied");
  setTimeout(() => tile.classList.remove("copied"), 350);
  if (!flag("copyBubble")) return; // border flash stays; bubble is optional
  const pop = document.createElement("div");
  pop.className = "copy-pop";
  // Match the fade animation to the (expert-tunable) bubble lifetime so it never
  // gets cut off early or lingers invisibly after the CSS fade ends.
  pop.style.animationDuration = `${BUBBLE_MS}ms`;
  pop.textContent = label || t("copied");
  tile.appendChild(pop);
  styleCopyText(pop, tile.clientWidth * 0.8, tile.clientHeight * 0.45, 26);
  setTimeout(() => pop.remove(), BUBBLE_MS);
}

// ---- F19 auto-paste: a fast second activation of the SAME prompt pastes the
// just-copied text into the last external window instead of re-copying. Enter on a
// focused tile copies + pastes in one press. All gated by the autoPaste opt-flag.
let lastActivate = { id: null, t: 0 };
function autoPasteDouble(id) {
  if (!flag("autoPaste")) return false;
  const now = performance.now();
  const dbl = lastActivate.id === id && (now - lastActivate.t) <= val("dblClickMs");
  lastActivate = dbl ? { id: null, t: 0 } : { id, t: now };
  return dbl;
}
async function autoPasteNow(el) {
  try {
    await invoke("paste_into_previous", { delayMs: Math.round(val("autoPasteDelayMs")), enter: optFlag("autoPasteEnter") });
    if (el) showCopied(el, t("pasted")); else toast(t("pasted"));
  } catch (e) {
    const m = String(e);
    toast(m === "no-target" ? t("pasteNoTarget") : m === "focus-failed" ? t("pasteFocusFail") : m);
  }
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
  // In the favorites view a prompt can't be hidden/removed — unstar it instead.
  ctxEl.querySelector('[data-act="hide"]').classList.toggle("hidden", favView);
  ctxEl.classList.remove("hidden");
  const w = ctxEl.offsetWidth, h = ctxEl.offsetHeight;
  const z = uiZoom();
  ctxEl.style.left = `${Math.min(x, window.innerWidth - w - 4) / z}px`;
  ctxEl.style.top = `${Math.min(y, window.innerHeight - h - 4) / z}px`;
}
function closeCtx() {
  ctxEl.classList.add("hidden");
  ctxId = null;
}

// ---- Right-click toolbar menu: quickly toggle the optional top-bar tools on/off
// (a shortcut into the expert flags that govern which header buttons are shown). ----
const TOOLBAR_TOOLS = ["multiView", "favViewButton", "quickGrid", "showLibrary", "showJournal", "clipWatcher", "chainPrompts", "pinButton", "barToggles", "showLogo", "showTitle"];
const TB_CHECK = '<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" d="M5 12.5l4.5 4.5L19 7"/></svg>';
// Little glyph shown beside each tool name so the menu reads at a glance.
const tbIcon = (d, fill) => `<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path ${fill ? 'fill="currentColor"' : 'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"'} d="${d}"/></svg>`;
// Raw wrapper for icons needing a per-path transform (e.g. the diagonal chain).
const tbWrap = (inner) => `<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">${inner}</svg>`;
// Tool-menu icons mirror the real toolbar buttons so the menu reads the same as
// the header (chain + pin stay diagonal; library/journal/clip match 1:1).
const TOOL_ICONS = {
  multiView: tbIcon("M9 4h11v11H9z M4 9h11v11H4z"), // two overlapping frames = multiple views
  favViewButton: tbIcon("M12 3.5l2.5 5.2 5.7.8-4.1 4 1 5.7L12 16.9l-5.1 2.6 1-5.7-4.1-4 5.7-.8z", true),
  quickGrid: tbIcon("M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z"),
  showLibrary: tbIcon("M4 6h2v2H4V6Zm4 0h12v2H8V6ZM4 11h2v2H4v-2Zm4 0h12v2H8v-2ZM4 16h2v2H4v-2Zm4 0h12v2H8v-2Z", true),
  showJournal: tbIcon("M12 8v4l3 2M21 12a9 9 0 1 1-2.6-6.3M21 4v4h-4"),
  clipWatcher: tbIcon("M9 4h6a1 1 0 0 1 1 1v1h1a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h1V5a1 1 0 0 1 1-1Zm0 2v1h6V6M9 4h6"),
  chainPrompts: tbWrap('<path transform="rotate(-45 12 12)" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M9 17H7A5 5 0 0 1 7 7h2M15 7h2a5 5 0 0 1 0 10h-2M8 12h8"/>'),
  pinButton: tbIcon("M16 3.2 20.8 8l-1.7 1.7-.9-.2-3.2 3.2.5 4.1-1.4 1L9.6 14l-5.4 5.4-1.2-1.2L8.4 13 4 8.6l1-1.4 4.1.5 3.2-3.2-.2-.9L16 3.2Z", true),
  barToggles: tbIcon("M6 9l6 6 6-6"),
  showLogo: tbIcon("M4 5h16v14H4zM7 15l3.5-3.5 3 3 2.5-2.5 2 2M8.6 10a1.4 1.4 0 1 1-2.8 0 1.4 1.4 0 0 1 2.8 0"),
  showTitle: tbIcon("M6 7V5h12v2M12 5v14M9 19h6"),
};
const toolbarMenu = document.createElement("div");
toolbarMenu.className = "ctx toolbar-menu hidden";
toolbarMenu.setAttribute("role", "menu");
document.body.appendChild(toolbarMenu);
const toolFlagOn = (key) => (OPT_FLAG_KEYS.includes(key) ? optFlag(key) : flag(key));
function buildToolbarMenu() {
  toolbarMenu.innerHTML = "";
  const head = document.createElement("div");
  head.className = "toolbar-menu-head";
  head.textContent = t("toolbarMenuTitle");
  toolbarMenu.appendChild(head);
  for (const key of TOOLBAR_TOOLS) {
    const on = toolFlagOn(key);
    const b = document.createElement("button");
    b.type = "button";
    b.className = "toolbar-item" + (on ? " on" : "");
    b.role = "menuitemcheckbox";
    b.setAttribute("aria-checked", String(on));
    const check = document.createElement("span");
    check.className = "tb-check";
    check.innerHTML = on ? TB_CHECK : "";
    const icon = document.createElement("span");
    icon.className = "tb-icon";
    icon.innerHTML = TOOL_ICONS[key] || "";
    const label = document.createElement("span");
    label.textContent = t(FLAG_LABELS[key] || OPT_FLAG_LABELS[key]);
    b.append(check, icon, label);
    b.addEventListener("click", async () => {
      const next = !toolFlagOn(key);
      settings.ui_flags = { ...(settings.ui_flags || {}), [key]: next };
      await invoke("set_ui_flag", { key, enabled: next }).catch((e) => toast(String(e)));
      // The ★ button only shows/works when the favorites-view feature is on, so
      // enabling it from the toolbar turns the feature on too (otherwise the menu
      // shows a checkmark but no star appears).
      if (key === "favViewButton" && next && !favViewEnabled()) {
        settings.ui_flags.favView = true;
        await invoke("set_ui_flag", { key: "favView", enabled: true }).catch((e) => toast(String(e)));
      }
      applyFlags();
      renderViews();
      refreshFavViewUi();
      buildToolbarMenu(); // reflect the new state, keep the menu open
    });
    toolbarMenu.appendChild(b);
  }
}
function openToolbarMenu(x, y) {
  buildToolbarMenu();
  toolbarMenu.classList.remove("hidden");
  const z = uiZoom();
  const w = toolbarMenu.offsetWidth, h = toolbarMenu.offsetHeight;
  toolbarMenu.style.left = `${Math.min(x, window.innerWidth - w - 6) / z}px`;
  toolbarMenu.style.top = `${Math.min(y + 4, window.innerHeight - h - 6) / z}px`;
}
function closeToolbarMenu() { toolbarMenu.classList.add("hidden"); }

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
    !!modalState.showImage, modalState.filePath || "", modalState.iconPath || "", modal.fav.classList.contains("active"),
  ]);
}

// ---- Prompt variables ({#{placeholder}#}) ----
// The {#{ ... }#} fence is deliberately uncommon so a literal "{{" pasted from a
// code block is never mistaken for a variable.
const VAR_RE = /\{#\{\s*([^}\n]{1,60}?)\s*\}#\}/g;
function extractVars(text) {
  const seen = [];
  for (const m of (text || "").matchAll(VAR_RE)) {
    const k = m[1].trim();
    if (k && !seen.includes(k)) seen.push(k);
  }
  return seen;
}
const fillVars = (text, values) => text.replace(VAR_RE, (_, k) => values[k.trim()] ?? "");

// Fill-in dialog shown when copying a prompt that holds {#{placeholders}#}.
// Resolves a {name: value} map, or null when cancelled.
let varsCleanup = null;
function promptVarsDialog(vars) {
  const root = $("vars-modal");
  $("vars-title").textContent = t("varsTitle");
  const fields = $("vars-fields");
  fields.innerHTML = "";
  // Remembered values are stored globally by variable name (expert: varDefaults).
  const useRemember = flag("varDefaults");
  let remembered = {};
  try { remembered = JSON.parse(settings.ui_texts?.varDefaults || "{}"); } catch (_) { /* ignore */ }
  const inputs = vars.map((name) => {
    const label = document.createElement("label");
    label.className = "field";
    const span = document.createElement("span");
    span.textContent = name;
    const input = document.createElement("input");
    input.className = "modal-input";
    input.type = "text";
    input.dataset.var = name;
    if (useRemember && name in remembered) input.value = remembered[name];
    label.append(span, input);
    if (useRemember) {
      const rem = document.createElement("label");
      rem.className = "vars-remember";
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.className = "switch";
      cb.checked = name in remembered;
      cb.dataset.rem = name;
      const rlbl = document.createElement("span");
      rlbl.textContent = t("rememberValue");
      rem.append(cb, rlbl);
      label.appendChild(rem);
    }
    fields.appendChild(label);
    return input;
  });
  wireClearButtons(fields); // clear-X on the variable inputs
  // Save/forget remembered values on confirm (never on cancel).
  const persistRemembered = () => {
    if (!useRemember) return;
    const obj = {};
    for (const i of inputs) {
      const cb = fields.querySelector(`input[data-rem="${CSS.escape(i.dataset.var)}"]`);
      if (cb?.checked) obj[i.dataset.var] = i.value;
    }
    const json = JSON.stringify(obj);
    if (json !== JSON.stringify(remembered)) {
      settings.ui_texts = { ...(settings.ui_texts || {}), varDefaults: json };
      invoke("set_ui_text", { key: "varDefaults", value: json }).catch(() => {});
    }
  };
  $("vars-ok").textContent = t("varsCopy");
  varsCleanup?.(null);
  root.classList.remove("hidden");
  inputs[0]?.focus();
  return new Promise((resolve) => {
    const ok = $("vars-ok");
    const cancel = $("vars-close");
    const collect = () => Object.fromEntries(inputs.map((i) => [i.dataset.var, i.value]));
    // The click/keypress that OPENED this dialog is still in flight: arm the
    // backdrop + Enter close handlers only on the next frame so that same event
    // can't immediately dismiss the dialog (it was flashing shut instantly).
    let armed = false;
    requestAnimationFrame(() => { armed = true; });
    const done = (val) => {
      root.classList.add("hidden");
      ok.removeEventListener("click", onOk);
      cancel.removeEventListener("click", onCancel);
      root.removeEventListener("pointerdown", onBg);
      document.removeEventListener("keydown", onKey);
      varsCleanup = null;
      resolve(val);
    };
    const onOk = () => { persistRemembered(); done(collect()); };
    const onCancel = () => done(null);
    const onBg = (e) => { if (armed && e.target === root) done(null); };
    const onKey = (e) => {
      if (e.key === "Escape") done(null);
      else if (e.key === "Enter" && armed) { persistRemembered(); done(collect()); }
    };
    varsCleanup = done;
    ok.addEventListener("click", onOk);
    cancel.addEventListener("click", onCancel);
    root.addEventListener("pointerdown", onBg);
    document.addEventListener("keydown", onKey);
  });
}

// Record a copy for the history/usage journal (backend honours the privacy flag).
const recordCopy = (id) => invoke("record_copy", { id }).catch(() => {});

// Per-prompt copy throttle: block re-copying the SAME prompt within the cooldown
// (expert value, 0 = off). Other prompts are never blocked, so a wrong pick can
// be corrected instantly. Returns true if this copy should be skipped.
const copyTimes = new Map();
const COPY_TIMES_CAP = 256; // sweep entries past the cooldown once the map grows
function copyOnCooldown(id) {
  const ms = val("copyCooldownMs");
  if (!ms) return false;
  const now = Date.now();
  const last = copyTimes.get(id);
  if (last && now - last < ms) return true;
  if (copyTimes.size >= COPY_TIMES_CAP) {
    for (const [k, t0] of copyTimes) if (now - t0 >= ms) copyTimes.delete(k);
  }
  copyTimes.set(id, now);
  return false;
}

// Build a Tauri accelerator ("CommandOrControl+Shift+P") from a keydown event.
// Returns null unless at least one modifier plus a real key was pressed.
function accelFromEvent(e) {
  if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return null;
  const mods = [];
  if (e.ctrlKey) mods.push("CommandOrControl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  if (!mods.length) return null;
  let key = e.key;
  if (key === " ") key = "Space";
  else if (key.startsWith("Arrow")) key = key.slice(5);
  else if (key.length === 1) key = key.toUpperCase();
  return [...mods, key].join("+");
}

// Turn a stored Tauri accelerator ("CommandOrControl+Shift+P") into a readable,
// localized label ("Strg + Umschalt + P"). Display only — the raw accelerator is
// what stays saved and registered.
const MOD_LABELS = {
  CommandOrControl: () => t("keyCtrl"), Control: () => t("keyCtrl"),
  Shift: () => t("keyShift"),
  Alt: () => "Alt", Option: () => "Alt",
  Super: () => "Win", Meta: () => "Win", Command: () => "Win", Cmd: () => "Win",
};
function prettyAccel(accel) {
  if (!accel) return "";
  return accel.split("+").map((p) => (MOD_LABELS[p] ? MOD_LABELS[p]() : p)).join(" + ");
}

// Copy a text prompt after filling its placeholders. `allowPaste` is only set by
// activations that mean "paste" (Enter on a focused tile); a plain click copies
// and nothing else, exactly like a prompt without variables.
async function copyTextWithVars(p, el, allowPaste = false) {
  // A double-click can never reach the tile a second time — the dialog already
  // covers it, and that second press used to land on the overlay and dismiss it.
  // Swallow it instead and treat it as the double-click it was: fill in the
  // variables, then paste.
  let quickSecond = false;
  const overlay = $("vars-modal");
  const onSecond = (e) => {
    if (e.target !== overlay) return; // a press inside the dialog is real input
    e.stopPropagation();
    quickSecond = true;
  };
  overlay.addEventListener("pointerdown", onSecond, true); // capture: before the dismiss
  const stopArming = setTimeout(() => overlay.removeEventListener("pointerdown", onSecond, true), val("dblClickMs"));
  const values = await promptVarsDialog(extractVars(p.text));
  clearTimeout(stopArming);
  overlay.removeEventListener("pointerdown", onSecond, true);
  if (!values) return;
  const ok = await invoke("copy_text", { text: fillVars(p.text, values) })
    .catch((e) => { toast(String(e)); return false; });
  if (ok) {
    showCopied(el);
    recordCopy(p.id);
    if ((allowPaste || quickSecond) && flag("autoPaste")) await autoPasteNow(el);
    maybeCloseOnCopy();
  }
}

// ---- Keyboard navigation: arrow keys move a focus ring across the grid,
// Enter/Space copies the focused tile (feature-gated by keyboardNav).
let kbFocus = null;
let kbFadeTimer = null;
function focusTile(id) {
  clearTimeout(kbFadeTimer);
  kbFocus = id;
  for (const el of gridEl.querySelectorAll(".tile")) el.classList.toggle("kb-focus", el.dataset.id === id);
  for (const el of gridEl.querySelectorAll(".tile.kb-leaving")) el.classList.remove("kb-leaving");
  gridEl.querySelector(`.tile[data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" });
}
// Clears the focus ring. `fade` smoothly transitions it out (used on window blur).
function clearKbFocus(fade) {
  const el = kbFocus ? gridEl.querySelector(`.tile[data-id="${CSS.escape(kbFocus)}"]`) : null;
  kbFocus = null;
  for (const t of gridEl.querySelectorAll(".tile.kb-focus")) t.classList.remove("kb-focus");
  if (fade && el) {
    el.classList.add("kb-leaving");
    setTimeout(() => el.classList.remove("kb-leaving"), 520);
  }
}
// The keyboard focus ring fades a short while after the window loses focus, so a
// left-behind highlight doesn't linger over other apps (delay is expert-tunable;
// the fade-out itself is a smooth 480ms CSS transition).
window.addEventListener("blur", () => {
  if (!kbFocus || !flag("keyboardNav")) return;
  clearTimeout(kbFadeTimer);
  kbFadeTimer = setTimeout(() => clearKbFocus(true), val("kbFocusFadeMs"));
});
window.addEventListener("focus", () => { clearTimeout(kbFadeTimer); });
function moveKbFocus(dc, dr) {
  // Use whichever layout the grid is currently showing — the favorites view lays out
  // from favLayoutMap(), not the active view, so arrow keys must follow that.
  const layout = favView ? favLayoutMap() : layoutOf(activeView());
  const cells = Object.entries(layout);
  if (!cells.length) return;
  if (!kbFocus || !layout[kbFocus]) { focusTile(cells[0][0]); return; }
  const [cc, cr] = layout[kbFocus];
  let best = null;
  let bestD = Infinity;
  for (const [id, [col, row]] of cells) {
    if (id === kbFocus) continue;
    const inDir = (dc > 0 && col > cc) || (dc < 0 && col < cc) || (dr > 0 && row > cr) || (dr < 0 && row < cr);
    if (!inDir) continue;
    // Prefer the same row/column: penalise the off-axis distance heavily.
    const d = dc ? Math.abs(col - cc) + Math.abs(row - cr) * 4 : Math.abs(row - cr) + Math.abs(col - cc) * 4;
    if (d < bestD) { bestD = d; best = id; }
  }
  if (best) focusTile(best);
}
async function activateTile(id) {
  const p = prompts.find((x) => x.id === id);
  if (!p) return;
  const el = gridEl.querySelector(`.tile[data-id="${CSS.escape(id)}"]`);
  if (chainMode) { await toggleChainPick(p); return; }
  if (copyOnCooldown(id)) return;
  if (flag("promptVars") && !p.copy_image && !p.file_path && extractVars(p.text).length) {
    await copyTextWithVars(p, el, true); // Enter means copy + paste
  } else if (await invoke("copy_prompt", { id }).catch((e) => { toast(String(e)); return false; })) {
    if (el) showCopied(el);
    recordCopy(id);
    if (flag("autoPaste")) await autoPasteNow(el); // F19: Enter = copy + paste
    maybeCloseOnCopy();
  }
}
// Expert opt: hide the window to the tray right after a prompt copy.
function maybeCloseOnCopy() {
  if (!optFlag("closeOnCopy")) return;
  try { window.__TAURI__?.window?.getCurrentWindow?.()?.hide?.().catch?.(() => {}); } catch { /* no-op */ }
}

function openModal({ mode, id, name = "", text = "", color = "", image = "", showImage = false, copyImage = false, filePath = "", iconPath = "", caption = "", captionSize = 0, font = "", fontSize = 0, favorite = false, title }) {
  modalState = { mode, id, color, image, showImage, copyImage, filePath, iconPath };
  modal.title.textContent = title;
  modal.fav.classList.toggle("active", !!favorite);
  modal.fav.setAttribute("aria-pressed", String(!!favorite));
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

// ---- F16: live length counter (chars / words / ~tokens) in the editor ----
let lenRaf = 0;
function updateLenCounter() {
  const el = $("modal-length");
  if (!el) return;
  const on = flag("lengthCounter") && !modal.text.classList.contains("hidden");
  el.classList.toggle("hidden", !on);
  if (!on) return;
  const s = modal.text.value;
  const chars = [...s].length; // code points, so emoji count as one
  const words = (s.trim().match(/\S+/g) || []).length;
  const tokens = Math.ceil(chars / 4); // rough heuristic, labeled with "~"
  const nf = (n) => n.toLocaleString(LANG);
  const units = txt("counterUnits") || "all";
  const parts = [];
  if (units === "all" || units === "chars") parts.push(`${nf(chars)} ${t("lenChars")}`);
  if (units === "all" || units === "words") parts.push(`${nf(words)} ${t("lenWords")}`);
  if (units === "all" || units === "tokens") parts.push(`~${nf(tokens)} ${t("lenTokens")}`);
  el.textContent = parts.join(" · ");
}
function scheduleLenCounter() {
  if (lenRaf) return;
  lenRaf = requestAnimationFrame(() => { lenRaf = 0; updateLenCounter(); });
}

// ---- F7 version history: render the collapsible list in the editor ----
async function loadHistory(id) {
  const body = $("modal-history-body");
  if (!body || !id) return;
  body.innerHTML = "";
  let list = [];
  try { list = await invoke("list_versions", { promptId: id }); } catch (_) {}
  if (!list.length) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t("historyEmpty");
    body.appendChild(e);
    return;
  }
  for (const v of list) {
    const row = document.createElement("div");
    row.className = "hist-row";
    const head = document.createElement("div");
    head.className = "hist-head";
    const time = document.createElement("span");
    time.className = "hist-time";
    time.textContent = new Date(v.ts * 1000).toLocaleString(LANG);
    const prev = document.createElement("span");
    prev.className = "hist-preview";
    prev.textContent = (v.name ? v.name + " — " : "") + (v.text || "").slice(0, 80);
    // Explicit expand chevron (collapsed by default) → reveals the COMPLETE text.
    const expand = document.createElement("button");
    expand.type = "button";
    expand.className = "hist-expand";
    expand.setAttribute("aria-expanded", "false");
    expand.innerHTML = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M6 9l6 6 6-6"/></svg>';
    const restore = document.createElement("button");
    restore.type = "button";
    restore.className = "ghost-btn hist-restore";
    restore.textContent = t("historyRestore");
    restore.addEventListener("click", async () => {
      try {
        const p = await invoke("restore_version", { versionId: v.id });
        if (p) {
          const idx = prompts.findIndex((x) => x.id === p.id);
          if (idx >= 0) prompts[idx] = p;
          modal.name.value = p.name;
          modal.text.value = p.text;
          updateLenCounter();
          toast(t("historyRestored"));
          await renderGrid(true);
          loadHistory(id); // the restore itself added a fresh snapshot
        }
      } catch (err) { toast(String(err)); }
    });
    head.append(time, prev, expand, restore);
    const full = document.createElement("div");
    full.className = "hist-full hidden";
    full.textContent = v.text || "";
    const toggle = () => {
      const open = !full.classList.toggle("hidden");
      expand.setAttribute("aria-expanded", String(open));
      row.classList.toggle("open", open);
    };
    expand.addEventListener("click", toggle);
    prev.addEventListener("click", toggle); // clicking the preview also expands
    row.append(head, full);
    body.appendChild(row);
  }
}

// ---- F11 usage statistics dashboard (opened from the expert menu) ----
function statSection(text) {
  const h = document.createElement("div");
  h.className = "stat-section";
  h.textContent = text;
  return h;
}
async function renderStatsPanel(body) {
  // Feature toggle + (when on) the top-N field share one row.
  const on = flag("usageStats");
  // Fetch BEFORE clearing — otherwise the panel paints empty during the await,
  // which read as a flash on every re-render.
  let s = null;
  if (on) {
    try { s = await invoke("usage_stats", { topN: Math.round(val("statsTopN")) }); }
    catch (e) { toast(String(e)); return; }
  }
  body.innerHTML = "";
  const headRow = document.createElement("div"); headRow.className = "stats-head-row";
  const sw = document.createElement("label"); sw.className = "field switch-field";
  const swl = document.createElement("span"); swl.textContent = t("flagUsageStats");
  const swi = document.createElement("input"); swi.type = "checkbox"; swi.className = "switch"; swi.checked = on;
  swi.addEventListener("change", async () => { await saveBackupSetting("flag", "usageStats", swi.checked); renderStatsPanel(body); });
  sw.append(swl, swi); headRow.appendChild(sw);
  if (on) {
    // Top-N param (how many prompts the "most used" chart lists), right of the toggle.
    const topRow = document.createElement("label"); topRow.className = "field backup-set-row stats-topn-row";
    const trl = document.createElement("span"); trl.textContent = t("valStatsTopN");
    const tri = document.createElement("input"); tri.type = "number"; tri.min = 5; tri.max = 100; tri.className = "modal-input"; tri.value = String(val("statsTopN"));
    tri.addEventListener("change", async () => { const v = Math.max(5, Math.min(100, Number(tri.value) || 10)); tri.value = String(v); await saveBackupSetting("value", "statsTopN", v); renderStatsPanel(body); });
    topRow.append(trl, tri); headRow.appendChild(topRow);
  }
  body.appendChild(headRow);
  if (!on) return;
  const nf = (n) => Number(n).toLocaleString(LANG);
  const cards = document.createElement("div");
  cards.className = "stat-cards";
  const card = (label, value) => {
    const c = document.createElement("div"); c.className = "stat-card";
    const v = document.createElement("div"); v.className = "stat-val"; v.textContent = value;
    const l = document.createElement("div"); l.className = "stat-label"; l.textContent = label;
    c.append(v, l); return c;
  };
  // Four rows of four, each row one topic: what you have, what is in it, how much
  // it gets used, and what that says about the collection.
  cards.append(
    // — Inventory —
    card(t("statTotalPrompts"), nf(s.total_prompts)),
    card(t("statViews"), nf((settings.views || []).length)),
    card(t("statFavorites"), nf(s.favorites)),
    card(t("statColored"), nf(s.colored)),
    // — Content —
    card(t("statWithMedia"), nf(prompts.filter((p) => p.image || p.file_path || p.copy_image).length)),
    card(t("statWithVars"), nf(prompts.filter((p) => extractVars(p.text || "").length > 0).length)),
    card(t("statChars"), nf(s.total_chars)),
    card(t("statAvgLen"), s.total_prompts ? nf(Math.round(s.total_chars / s.total_prompts)) : "—"),
    // — Copy activity —
    card(t("statTotalCopies"), nf(s.total_copies)),
    card(t("statCopies7"), s.history_on ? nf(s.copies7) : "—"),
    card(t("statCopies30"), s.history_on ? nf(s.copies30) : "—"),
    card(t("statPerDay"), s.history_on ? (s.copies30 / 30).toFixed(1) : "—"),
    // — What that says about the collection —
    card(t("statAvg"), s.used_count ? s.avg_copies.toFixed(1) : "—"),
    card(t("statTopCopies"), s.top && s.top.length ? nf(s.top[0].count) : "—"),
    card(t("statUsedShare"), s.total_prompts ? `${Math.round((s.used_count / s.total_prompts) * 100)} %` : "—"),
    card(t("statUnused"), nf(s.unused)),
  );
  body.appendChild(cards);
  // Info rows: last-used + longest prompt.
  const info = document.createElement("div");
  info.className = "stat-views";
  const infoRow = (label, value) => {
    const r = document.createElement("div"); r.className = "stat-view-row";
    const l = document.createElement("span"); l.textContent = label;
    const v = document.createElement("span"); v.className = "stat-view-count"; v.textContent = value;
    r.append(l, v); return r;
  };
  if (s.recent_ts) info.appendChild(infoRow(t("statRecent"), `${s.recent_name} · ${new Date(s.recent_ts * 1000).toLocaleDateString(LANG)}`));
  if (s.longest_chars) info.appendChild(infoRow(t("statLongest"), `${s.longest_name} · ${nf(s.longest_chars)} ${t("lenChars")}`));
  if (info.childElementCount) { body.appendChild(statSection(t("statOverview"))); body.appendChild(info); }
  // Type breakdown as proportional bars.
  if (s.types.length) {
    body.appendChild(statSection(t("statTypes")));
    const list = document.createElement("div"); list.className = "stat-bars";
    const typeLabel = { text: "statTypeText", image: "filterImage", video: "filterVideo", pdf: "filterPdf", file: "filterFile" };
    const maxT = Math.max(...s.types.map((x) => x.count), 1);
    for (const ty of s.types) {
      const row = document.createElement("div"); row.className = "stat-bar-row";
      const name = document.createElement("span"); name.className = "stat-bar-name"; name.textContent = t(typeLabel[ty.name] || ty.name);
      const track = document.createElement("div"); track.className = "stat-bar-track";
      const fill = document.createElement("div"); fill.className = "stat-bar-fill";
      fill.style.width = `${Math.max(4, (ty.count / maxT) * 100)}%`;
      track.appendChild(fill);
      const cnt = document.createElement("span"); cnt.className = "stat-bar-count"; cnt.textContent = nf(ty.count);
      row.append(name, track, cnt);
      list.appendChild(row);
    }
    body.appendChild(list);
  }
  if (s.top.length) {
    body.appendChild(statSection(t("statTop")));
    const list = document.createElement("div"); list.className = "stat-bars";
    const max = s.top[0].count || 1;
    for (const b of s.top) {
      const row = document.createElement("div"); row.className = "stat-bar-row";
      const name = document.createElement("span"); name.className = "stat-bar-name"; name.textContent = b.name;
      const track = document.createElement("div"); track.className = "stat-bar-track";
      const fill = document.createElement("div"); fill.className = "stat-bar-fill";
      fill.style.width = `${Math.max(4, (b.count / max) * 100)}%`;
      track.appendChild(fill);
      const cnt = document.createElement("span"); cnt.className = "stat-bar-count"; cnt.textContent = nf(b.count);
      row.append(name, track, cnt);
      list.appendChild(row);
    }
    body.appendChild(list);
  }
  if (s.never_used.length) {
    body.appendChild(statSection(t("statCleanup")));
    const list = document.createElement("div"); list.className = "stat-cleanup";
    for (const b of s.never_used) {
      const item = document.createElement("button");
      item.type = "button"; item.className = "stat-cleanup-item"; item.textContent = b.name;
      item.title = t("statJump");
      item.addEventListener("click", () => jumpToPrompt(b.name));
      list.appendChild(item);
    }
    body.appendChild(list);
  }
  if (s.per_view.length) {
    body.appendChild(statSection(t("statPerView")));
    const list = document.createElement("div"); list.className = "stat-views";
    for (const v of s.per_view) {
      const row = document.createElement("div"); row.className = "stat-view-row";
      const name = document.createElement("span"); name.textContent = v.name;
      const cnt = document.createElement("span"); cnt.className = "stat-view-count"; cnt.textContent = nf(v.count);
      row.append(name, cnt);
      list.appendChild(row);
    }
    body.appendChild(list);
  }
  wireClearButtons(body); // clear-X on the top-N field
}
function jumpToPrompt(name) {
  settingsEl.classList.add("hidden");
  $("library-btn").click(); // resets + opens the library
  libQuery = name || "";
  $("library-q").value = libQuery;
  renderLibrary();
}
// Inline expert panels (backups + statistics) — filled on demand by renderExpert.

// ---- F6 duplicate finder ----
async function openDupes() {
  $("dupes-modal").classList.remove("hidden");
  await scanDupes();
}
async function scanDupes() {
  const body = $("dupes-body");
  body.innerHTML = "";
  const loading = document.createElement("div");
  loading.className = "hint";
  loading.textContent = t("dupScanning");
  body.appendChild(loading);
  let groups;
  try { groups = await invoke("find_duplicates", { threshold: Math.round(val("dupThreshold")) }); }
  catch (e) { toast(String(e)); body.innerHTML = ""; return; }
  body.innerHTML = "";
  if (!groups.length) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t("dupNone");
    body.appendChild(e);
    return;
  }
  for (const g of groups) {
    const card = document.createElement("div");
    card.className = "dup-group";
    const head = document.createElement("div");
    head.className = "dup-head";
    const score = document.createElement("span");
    score.className = "dup-score";
    score.textContent = `${Math.round(g.score * 100)}%`;
    head.appendChild(score);
    card.appendChild(head);
    const cols = document.createElement("div");
    cols.className = "dup-cols";
    for (const m of g.members) {
      const col = document.createElement("div");
      col.className = "dup-col";
      const name = document.createElement("div");
      name.className = "dup-name";
      name.textContent = m.name;
      const meta = document.createElement("div");
      meta.className = "dup-meta";
      meta.textContent = `${t("dupUsage")}: ${m.usage.toLocaleString(LANG)}` + (m.views.length ? ` · ${m.views.join(", ")}` : "");
      const txt = document.createElement("div");
      txt.className = "dup-text";
      txt.textContent = m.text;
      const keep = document.createElement("button");
      keep.type = "button";
      keep.className = "ghost-btn dup-keep";
      keep.textContent = t("dupKeep");
      keep.addEventListener("click", () => keepOneDup(g, m.id));
      col.append(name, meta, txt, keep);
      cols.appendChild(col);
    }
    card.appendChild(cols);
    const actions = document.createElement("div");
    actions.className = "dup-actions";
    const ignore = document.createElement("button");
    ignore.type = "button";
    ignore.className = "ghost-btn";
    ignore.textContent = t("dupIgnore");
    ignore.addEventListener("click", async () => {
      await invoke("ignore_dups", { ids: g.members.map((m) => m.id) }).catch((e) => toast(String(e)));
      scanDupes();
    });
    actions.appendChild(ignore);
    card.appendChild(actions);
    body.appendChild(card);
  }
}
async function keepOneDup(group, keepId) {
  const remove = group.members.filter((m) => m.id !== keepId).map((m) => m.id);
  if (!remove.length) return;
  const ok = await confirmDialog({ title: t("dupKeep"), message: t("dupKeepConfirm").replace("{n}", String(remove.length)), confirmLabel: t("delete") });
  if (!ok) return;
  try {
    const gone = new Set(remove);
    settings = await invoke("batch_prompts", { ids: remove, action: "delete", color: null, favorite: null, viewId: null });
    prompts = prompts.filter((p) => !gone.has(p.id));
  } catch (e) { toast(String(e)); return; }
  renderViews();
  await renderGrid(true);
  scanDupes();
}
// F6 import hook: after an import, flag near-duplicates with a toast linking to the tool.
async function maybeDupImportToast() {
  if (!flag("dupFinder") || !flag("dupImportCheck")) return;
  try {
    const groups = await invoke("find_duplicates", { threshold: Math.round(val("dupThreshold")) });
    if (groups.length) toast(t("dupImportToast").replace("{n}", String(groups.length)));
  } catch (_) {}
}
EXPERT_ACTIONS["dupes-open"] = openDupes;

// ---- F2 backups & restore ----
function fmtBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
function prettyStamp(name) {
  const [d, tm] = String(name).split("_");
  return tm ? `${d} ${tm.replace(/-/g, ":")}` : d;
}
function parseStamp(name) {
  const [d, tm] = String(name).split("_");
  if (!d) return null;
  const dt = new Date(`${d}T${(tm || "00-00-00").replace(/-/g, ":")}`);
  return isNaN(dt.getTime()) ? null : dt;
}
async function saveBackupSetting(kind, key, value) {
  if (kind === "flag") {
    settings.ui_flags = { ...(settings.ui_flags || {}), [key]: value };
    await invoke("set_ui_flag", { key, enabled: value }).catch((e) => toast(String(e)));
    applyFlags();
  } else {
    settings.ui_values = { ...(settings.ui_values || {}), [key]: value };
    await invoke("set_ui_value", { key, value }).catch((e) => toast(String(e)));
    applyValues();
  }
}
// Backups live in their own dialog (opened from settings), not in the expert menu:
// the page carries settings, diagnostics and a restore list and needs the room.
function openBackups() {
  $("backup-modal").classList.remove("hidden");
  renderBackupPanel($("backup-panel"));
  renderStatsPanel($("stats-panel"));
}
function closeBackups() {
  $("backup-modal").classList.add("hidden");
}

// Remembered UI state (which panel sections are expanded). Kept in ui_flags so it
// survives a restart; the key is not in OPT_FLAG_LABELS, so it never shows up as an
// expert toggle and applyFlags() ignores it.
async function saveUiState(key, on) {
  settings.ui_flags = { ...(settings.ui_flags || {}), [key]: on };
  await invoke("set_ui_flag", { key, enabled: on }).catch(() => {});
}
async function renderBackupPanel(body) {
  // Fetch BEFORE clearing: emptying the panel and only then awaiting lets the browser
  // paint the empty state for a frame, which showed up as a visible flash on every
  // re-render (backup now, value change …).
  let backups;
  try { backups = await invoke("list_backups"); }
  catch (e) { toast(String(e)); return; }
  body.innerHTML = "";
  const nf = (n) => Number(n).toLocaleString(LANG);
  const card = (l, v, small) => {
    const c = document.createElement("div"); c.className = "stat-card";
    const vv = document.createElement("div"); vv.className = "stat-val" + (small ? " stat-val-sm" : ""); vv.textContent = v;
    const ll = document.createElement("div"); ll.className = "stat-label"; ll.textContent = l;
    c.append(vv, ll); return c;
  };
  // Date/time card: date on top, time below (no comma), a touch larger than the
  // plain small value so the timestamps read clearly.
  const dtCard = (l, d) => {
    const c = document.createElement("div"); c.className = "stat-card";
    const vv = document.createElement("div"); vv.className = "stat-val stat-dt";
    if (d) {
      const dd = document.createElement("div"); dd.className = "dt-date"; dd.textContent = d.toLocaleDateString(LANG);
      const tt = document.createElement("div"); tt.className = "dt-time"; tt.textContent = d.toLocaleTimeString(LANG);
      vv.append(dd, tt);
    } else { vv.textContent = "—"; }
    const ll = document.createElement("div"); ll.className = "stat-label"; ll.textContent = l;
    c.append(vv, ll); return c;
  };
  // Collapsible section head (chevron), toggles the given content element and
  // remembers the last state under `stateKey`.
  const collapseHead = (labelText, contentEl, stateKey) => {
    const open = optFlag(stateKey);
    const h = document.createElement("button");
    h.type = "button";
    h.className = "stat-section backup-list-head" + (open ? " open" : "");
    const s = document.createElement("span"); s.textContent = labelText; h.appendChild(s);
    h.insertAdjacentHTML("beforeend", '<svg class="collapse-chevron" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M6 9l6 6 6-6"/></svg>');
    contentEl.classList.toggle("hidden", !open);
    h.addEventListener("click", () => {
      const o = !contentEl.classList.toggle("hidden");
      h.classList.toggle("open", o);
      saveUiState(stateKey, o);
    });
    return h;
  };

  const fmtInterval = (h) => (h < 24 ? `${h} h` : `${h / 24} d`); // ≥24h shown in days
  const gfsSum = () => Math.max(1, Math.min(50, val("backupDaily") + val("backupWeekly") + val("backupMonthly")));
  const numRow = (labelKey, key, min, max, onSet) => {
    const l = document.createElement("label"); l.className = "field backup-set-row";
    const s = document.createElement("span"); s.textContent = t(labelKey);
    const i = document.createElement("input"); i.type = "number"; i.min = min; i.max = max; i.className = "modal-input"; i.value = String(val(key));
    i.addEventListener("change", async () => { const v = Math.max(min, Math.min(max, Number(i.value) || min)); i.value = String(v); await onSet(v); });
    l.append(s, i); return l;
  };

  // ---- Settings ---- (auto-backup always runs; the interval controls how often)
  body.appendChild(statSection(t("backupSettings")));
  const set = document.createElement("div"); set.className = "backup-settings";
  // "Backup now" + interval + (unless GFS) keep, all on one row — backup-now first.
  const row2 = document.createElement("div"); row2.className = "backup-two-col";
  const nowBtn = document.createElement("button"); nowBtn.type = "button"; nowBtn.className = "ghost-btn backup-now-btn"; nowBtn.textContent = t("backupNow");
  nowBtn.addEventListener("click", async () => {
    try { await invoke("create_backup", { keep: Math.round(val("backupKeep")) }); toast(t("backupDone")); }
    catch (e) { toast(String(e)); }
    await saveUiState("backupListOpen", true); // reveal the fresh backup right away
    renderBackupPanel(body);
  });
  row2.appendChild(nowBtn);
  const iv = document.createElement("label"); iv.className = "field backup-set-row backup-interval-row";
  const ivl = document.createElement("span"); ivl.textContent = t("valBackupInterval");
  const ivs = document.createElement("select"); ivs.className = "modal-input";
  for (const h of [6, 12, 24, 48, 168]) { const o = document.createElement("option"); o.value = String(h); o.textContent = fmtInterval(h); ivs.appendChild(o); }
  ivs.value = String(val("backupIntervalH"));
  ivs.addEventListener("change", async () => { await saveBackupSetting("value", "backupIntervalH", Number(ivs.value)); renderBackupPanel(body); });
  iv.append(ivl, ivs); row2.appendChild(iv);
  if (!optFlag("backupGfs")) {
    row2.appendChild(numRow("valBackupKeep", "backupKeep", 1, 50, (v) => saveBackupSetting("value", "backupKeep", v)));
  }
  set.appendChild(row2);
  // Period-based retention (GFS): keep N per day / week / month.
  const gsw = document.createElement("label"); gsw.className = "field switch-field";
  const gswl = document.createElement("span"); gswl.textContent = t("flagBackupGfs");
  const gswi = document.createElement("input"); gswi.type = "checkbox"; gswi.className = "switch"; gswi.checked = optFlag("backupGfs");
  gswi.addEventListener("change", async () => {
    await saveBackupSetting("flag", "backupGfs", gswi.checked);
    if (gswi.checked) await saveBackupSetting("value", "backupKeep", gfsSum()); // keep = sum of tiers
    renderBackupPanel(body);
  });
  gsw.append(gswl, gswi); set.appendChild(gsw);
  if (optFlag("backupGfs")) {
    const tiers = document.createElement("div"); tiers.className = "backup-three-col";
    const onTier = (key) => async (v) => { await saveBackupSetting("value", key, v); await saveBackupSetting("value", "backupKeep", gfsSum()); };
    tiers.appendChild(numRow("valBackupDaily", "backupDaily", 0, 30, onTier("backupDaily")));
    tiers.appendChild(numRow("valBackupWeekly", "backupWeekly", 0, 52, onTier("backupWeekly")));
    tiers.appendChild(numRow("valBackupMonthly", "backupMonthly", 0, 60, onTier("backupMonthly")));
    set.appendChild(tiers);
  }
  // Optional backup password. Without one, backups use a fixed key and restore on any
  // PC; with one they need exactly this password. The stored password is never sent
  // back to the UI — the eye only reveals what was just typed in this session.
  const pwRow = document.createElement("div"); pwRow.className = "field backup-pw-row";
  const pwLabel = document.createElement("span"); pwLabel.textContent = t("backupPasswordLabel");
  const pwWrap = document.createElement("div"); pwWrap.className = "backup-pw-wrap";
  const pwIn = document.createElement("input");
  pwIn.type = "password"; pwIn.className = "modal-input"; pwIn.maxLength = 128;
  pwIn.autocomplete = "new-password";
  const hasPw = await invoke("has_backup_password").catch(() => false);
  pwIn.placeholder = hasPw ? t("backupPasswordSetPh") : t("backupPasswordPh");
  const eye = document.createElement("button");
  eye.type = "button"; eye.className = "icon-btn backup-pw-eye";
  eye.setAttribute("aria-label", t("showPassword"));
  eye.innerHTML = EYE_SVG;
  // Reveals what is typed right now. A stored password is never loaded back into
  // the field, so there is nothing secret to leak here.
  eye.addEventListener("click", () => {
    const show = pwIn.type === "password";
    pwIn.type = show ? "text" : "password";
    eye.setAttribute("aria-label", t(show ? "hidePassword" : "showPassword"));
  });
  // Saving happens ONLY through this button — leaving the field must never change
  // the password by accident.
  const save = document.createElement("button");
  save.type = "button";
  save.className = "ghost-btn backup-pw-save";
  save.textContent = t("save");
  save.addEventListener("click", async () => {
    const pw = pwIn.value;
    try {
      await invoke("set_backup_password", { password: pw });
      toast(pw ? t("backupPasswordWarn") : t("backupPasswordCleared"));
      pwIn.value = "";
      pwIn.type = "password";
      pwIn.placeholder = pw ? t("backupPasswordSetPh") : t("backupPasswordPh");
    } catch (e) { toast(String(e)); }
  });
  pwWrap.append(pwIn, eye);
  pwRow.append(pwLabel, pwWrap, save);
  set.appendChild(pwRow);
  body.appendChild(set);

  // ---- Diagnostics (collapsible) ----
  const total = backups.reduce((s, b) => s + b.size, 0);
  const newest = backups.length ? parseStamp(backups[0].name) : null;
  const oldest = backups.length ? parseStamp(backups[backups.length - 1].name) : null;
  const sizes = backups.map((b) => b.size);
  const cards = document.createElement("div"); cards.className = "stat-cards";
  const recent = backups.filter((b) => {
    const d = parseStamp(b.name);
    return d && Date.now() - d.getTime() <= 7 * 86400000;
  }).length;
  // Row 1 answers "what do I have and from when", row 2 "when is the next one and
  // how much space does this cost".
  cards.append(
    card(t("backupCount"), nf(backups.length)),
    card(t("backupThisWeek"), nf(recent)),
    card(t("backupSpan"), newest && oldest
      ? `${nf(Math.max(0, Math.round((newest - oldest) / 86400000)))} d`
      : "—"),
    dtCard(t("backupOldest"), oldest),
    dtCard(t("backupNewest"), newest),
    dtCard(t("backupNext"), newest ? new Date(newest.getTime() + val("backupIntervalH") * 3600000) : null),
    card(t("backupTotalSize"), fmtBytes(total)),
    card(t("backupAvgSize"), backups.length ? fmtBytes(Math.round(total / backups.length)) : "—"),
    card(t("backupLargest"), backups.length ? fmtBytes(Math.max(...sizes)) : "—"),
    card(t("backupSmallest"), backups.length ? fmtBytes(Math.min(...sizes)) : "—"),
  );
  body.append(collapseHead(t("backupDiag"), cards, "backupDiagOpen"), cards);

  // ---- List (opened right after a manual backup, otherwise last known state) ----
  const list = document.createElement("div"); list.className = "backups-list";
  const listHead = collapseHead(`${t("backupList")} (${backups.length})`, list, "backupListOpen");
  if (!backups.length) {
    const e = document.createElement("div"); e.className = "hint"; e.textContent = t("backupsEmpty");
    list.appendChild(e);
  } else {
    for (const b of backups) {
      const row = document.createElement("div"); row.className = "backup-row";
      const name = document.createElement("span"); name.className = "backup-name"; name.textContent = prettyStamp(b.name);
      const size = document.createElement("span"); size.className = "backup-size"; size.textContent = fmtBytes(b.size);
      const restore = document.createElement("button"); restore.type = "button"; restore.className = "ghost-btn"; restore.textContent = t("backupRestore");
      restore.addEventListener("click", () => restoreBackup(b.name));
      const del = document.createElement("button"); del.type = "button"; del.className = "ghost-btn danger-btn"; del.textContent = t("delete");
      del.addEventListener("click", async () => {
        if (!armButton(del, `${t("delete")}?`)) { setTimeout(() => disarmButton(del, t("delete")), DISARM_MS); return; }
        await invoke("delete_backup", { name: b.name }).catch((e) => toast(String(e)));
        await saveUiState("backupListOpen", true); // stay in the list after delete
        renderBackupPanel(body);
      });
      row.append(name, size, restore, del);
      list.appendChild(row);
    }
  }
  body.append(listHead, list);
  wireClearButtons(body); // clear-X on the number fields
}
async function restoreBackup(name) {
  const ok = await confirmDialog({ title: t("backupRestore"), message: t("backupRestoreConfirm"), confirmLabel: t("backupRestore") });
  if (!ok) return;
  try { await invoke("restore_backup", { name, keep: Math.round(val("backupKeep")) }); }
  catch (e) { toast(String(e)); } // on success the app relaunches
}

// ---- F21 clipboard inbox ----
let clipCount = 0; // number of collected clipboard items currently waiting in the inbox
function updateClipBadge() {
  const btn = $("clip-btn");
  if (!btn) return;
  // The icon stays while the watcher runs — or always, if the user asked to keep
  // the paused buttons around (expert: pausedIcons).
  btn.classList.toggle("hidden", !optFlag("clipWatcher") && !optFlag("pausedIcons"));
  const tgl = $("clip-toggle");
  if (tgl) tgl.classList.toggle("active", optFlag("clipWatcher"));
  const badge = $("clip-badge");
  badge.classList.toggle("hidden", clipCount <= 0); // only the number hides at 0
  badge.textContent = clipCount > 99 ? "99+" : String(clipCount);
}
async function refreshClipInbox() {
  if (!optFlag("clipWatcher")) { clipCount = 0; updateClipBadge(); return; }
  let items = [];
  try { items = await invoke("clip_inbox_list"); } catch (_) {}
  clipCount = items.length; // the badge shows how many collected items are waiting
  updateClipBadge();
  if (!$("clip-modal").classList.contains("hidden")) renderClipList(items);
}
function renderClipList(items) {
  const list = $("clip-list");
  list.innerHTML = "";
  if (!items.length) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t("clipEmpty");
    list.appendChild(e);
    return;
  }
  for (const it of items) {
    const row = document.createElement("div");
    row.className = "clip-row";
    const time = document.createElement("span");
    time.className = "clip-time";
    time.textContent = new Date(it.ts * 1000).toLocaleString(LANG);
    const prev = document.createElement("div");
    prev.className = "clip-preview";
    prev.textContent = it.text;
    const actions = document.createElement("div");
    actions.className = "clip-actions";
    const save = document.createElement("button");
    save.type = "button"; save.className = "ghost-btn"; save.textContent = t("clipSave");
    save.addEventListener("click", () => {
      $("clip-modal").classList.add("hidden");
      openModal({ mode: "create", text: it.text, title: t("nameModalTitle") });
    });
    const copy = document.createElement("button");
    copy.type = "button"; copy.className = "ghost-btn"; copy.textContent = t("clipCopy");
    copy.addEventListener("click", () => { invoke("copy_text", { text: it.text }).then(() => toast(t("copied"))).catch((e) => toast(String(e))); });
    const dismiss = document.createElement("button");
    dismiss.type = "button"; dismiss.className = "ghost-btn"; dismiss.textContent = t("clipDismiss");
    dismiss.addEventListener("click", async () => { await invoke("clip_inbox_dismiss", { id: it.id }).catch((e) => toast(String(e))); refreshClipInbox(); });
    actions.append(save, copy, dismiss);
    row.append(time, prev, actions);
    list.appendChild(row);
  }
}
async function openClipInbox() {
  let items = [];
  try { items = await invoke("clip_inbox_list"); } catch (_) {}
  clipCount = items.length; // badge stays until items are dismissed/cleared
  updateClipBadge();
  renderClipList(items);
  $("clip-modal").classList.remove("hidden");
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
  updateLenCounter();
  // F7: version history only for existing text prompts; reset to collapsed on open.
  const histOn = mode === "edit" && textVisible && flag("promptHistory");
  $("modal-history").classList.toggle("hidden", !histOn);
  if (histOn) {
    $("modal-history-body").classList.add("hidden");
    $("modal-history-body").innerHTML = "";
    $("modal-history-head").setAttribute("aria-expanded", "false");
    $("modal-history").classList.remove("open");
    modal.root.classList.remove("history-open");
  }
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
  const fileHint = $("modal-file-hint");
  fileHint.classList.toggle("hidden", !filePath);
  fileHint.textContent = t(optFlag("storeFiles") ? "fileHintStored" : "fileHint");
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
// `icon` puts an SVG above the title, `input` turns it into a one-field prompt
// (then it resolves to the entered string, or null when dismissed).
let confirmCleanup = null;
function confirmDialog({ title, message, confirmLabel, cancelLabel, icon, input, danger = true }) {
  const root = $("confirm-modal");
  $("confirm-title").textContent = title;
  $("confirm-msg").textContent = message;
  const iconEl = $("confirm-icon");
  iconEl.innerHTML = icon || "";
  iconEl.classList.toggle("hidden", !icon);
  const row = $("confirm-input-row");
  const field = $("confirm-input");
  const eye = $("confirm-input-eye");
  row.classList.toggle("hidden", !input);
  const ok = $("confirm-ok");
  const cancel = $("confirm-cancel");
  ok.classList.toggle("danger-confirm", danger);
  ok.textContent = confirmLabel;
  cancel.textContent = cancelLabel || t("cancel");
  if (input) {
    field.value = "";
    field.type = "password";
    field.placeholder = input.placeholder || "";
    eye.innerHTML = EYE_SVG;
  }
  confirmCleanup?.(false); // resolve any stale dialog first
  root.classList.remove("hidden");
  return new Promise((resolve) => {
    const done = (val) => {
      root.classList.add("hidden");
      ok.removeEventListener("click", onOk);
      cancel.removeEventListener("click", onCancel);
      root.removeEventListener("pointerdown", onBg);
      field.removeEventListener("keydown", onKey);
      eye.removeEventListener("click", onEye);
      confirmCleanup = null;
      resolve(val);
    };
    const finish = (okPressed) => done(input ? (okPressed ? field.value : null) : okPressed);
    const onOk = () => finish(true);
    const onCancel = () => finish(false);
    const onBg = (e) => { if (e.target === root) finish(false); };
    const onKey = (e) => { if (e.key === "Enter") { e.preventDefault(); finish(true); } };
    const onEye = () => { field.type = field.type === "password" ? "text" : "password"; };
    ok.addEventListener("click", onOk);
    cancel.addEventListener("click", onCancel);
    root.addEventListener("pointerdown", onBg);
    field.addEventListener("keydown", onKey);
    eye.addEventListener("click", onEye);
    confirmCleanup = done;
    (input ? field : ok).focus();
  });
}

// Same dialog with nothing to decide: one button, used to report a result.
async function infoDialog(title, message, icon) {
  const cancel = $("confirm-cancel");
  cancel.classList.add("hidden");
  try {
    await confirmDialog({ title, message, confirmLabel: t("close"), icon, danger: false });
  } finally {
    cancel.classList.remove("hidden");
  }
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
  const favorite = modal.fav.classList.contains("active");
  // NOTE: Tauri expects camelCase keys for snake_case Rust args.
  // Path media (gif/video icon or attachment) shows without a stored image.
  const showImage =
    image || mediaKind(filePath) || mediaKind(iconPath) ? modalState.showImage : false;
  const copyImage = image ? modalState.copyImage : false;
  try {
    if (modalState.mode === "create") {
      const text = modal.text.value.trim();
      if (!text) { closeModal(); return; }
      await invoke("add_prompt", { name, text, color, image, showImage, copyImage, filePath, iconPath, caption, captionSize, font, fontSize, favorite });
      inputEl.value = "";
      autoGrow(inputEl);
      saveBtn.disabled = true;
    } else if (modalState.mode === "image-create" || modalState.mode === "file-create") {
      // The name doubles as the copy text when shown as text.
      await invoke("add_prompt", { name, text: name, color, image, showImage, copyImage, filePath, iconPath, caption, captionSize, font, fontSize, favorite });
    } else {
      const text = copyImage || filePath ? name : modal.text.value;
      await invoke("update_prompt", { id: modalState.id, name, text, color, image, showImage, copyImage, filePath, iconPath, caption, captionSize, font, fontSize, favorite });
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
      favorite: p.favorite || false,
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

function videoSrc(p) {
  if (mediaKind(p.file_path) === "video") return p.file_path;
  if (mediaKind(p.icon_path) === "video") return p.icon_path;
  return "";
}

// Only the videos currently scrolled into view play — keeps a long list cheap.
// Library and journal each get their own observer so opening one panel never
// pauses the other's still-mounted video thumbnails.
let _libObs = null;
let _journalObs = null;
let _activeVidObs = null; // observer the current render attaches new videos to
function makeVideoObserver() {
  return new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (e.isIntersecting) e.target.play().catch(() => {});
      else e.target.pause();
    }
  }, { threshold: 0.1 });
}

function thumbDot(p) {
  const dot = document.createElement("span");
  dot.className = "dot";
  if (p.color) dot.style.background = p.color;
  return dot;
}

// Shared row preview for library + journal: a looping muted inline video for
// video prompts, an image for image/gif/stored-image prompts, else a color dot.
function thumbEl(p) {
  if (!flag("imagePreview")) return thumbDot(p);
  const vsrc = flag("libraryVideoPreview") ? videoSrc(p) : "";
  if (vsrc) {
    const v = document.createElement("video");
    v.className = "lib-thumb";
    v.muted = true;
    v.loop = true;
    v.playsInline = true;
    v.preload = "metadata";
    v.draggable = false;
    v.src = convertFileSrc(vsrc);
    if (_activeVidObs) _activeVidObs.observe(v);
    v.addEventListener("click", (e) => { e.stopPropagation(); openLightbox(v.src, true); });
    return v;
  }
  const src = thumbSrc(p);
  if (src) {
    const img = document.createElement("img");
    img.className = "lib-thumb";
    img.src = src;
    img.draggable = false;
    img.addEventListener("click", (e) => { e.stopPropagation(); openLightbox(img.src, false); });
    return img;
  }
  return thumbDot(p);
}

function setLibType(type) {
  libType = type;
  setActiveFilter($("library-types"), ".lib-type", "type", type);
}

// ---- F18 fuzzy search: tiered scoring (exact > prefix > fuzzy) over the same
// fields the plain search used. Fuzzy only kicks in when no substring hit exists,
// so the common case short-circuits and stays cheap on 1000+ prompts.
function fuzzyMaxTyposFor(term) {
  const raw = settings.ui_texts?.fuzzyMaxTypos || "auto";
  if (raw === "1") return 1;
  if (raw === "2") return 2;
  return term.length <= 5 ? 1 : 2; // auto
}
// Bounded Levenshtein: returns max+1 as soon as every cell exceeds max (early out).
function editDistWithin(a, b, max) {
  const la = a.length, lb = b.length;
  if (Math.abs(la - lb) > max) return max + 1;
  let prev = Array.from({ length: lb + 1 }, (_, j) => j);
  for (let i = 1; i <= la; i++) {
    const cur = new Array(lb + 1);
    cur[0] = i;
    let rowMin = i;
    const ca = a.charCodeAt(i - 1);
    for (let j = 1; j <= lb; j++) {
      const cost = ca === b.charCodeAt(j - 1) ? 0 : 1;
      cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost);
      if (cur[j] < rowMin) rowMin = cur[j];
    }
    if (rowMin > max) return max + 1;
    prev = cur;
  }
  return prev[lb];
}
// Weighted match score for the active query. score 0 = no match; fuzzy = true when
// only the typo-tolerant tier matched (rendered dimmed).
// Per-render memo: libSearchScore is called for every prompt in the filter, TWICE
// per comparison in the sort, and once more per row for the fuzzy class. Fuzzy mode
// runs bounded edit-distance over every token, so recomputing 3×/keystroke lagged.
// Cache is cleared at the top of each renderLibrary (see below).
let libScoreCache = new Map();
function libSearchScore(p) {
  const hit = libScoreCache.get(p.id);
  if (hit) return hit;
  const res = computeLibSearchScore(p);
  libScoreCache.set(p.id, res);
  return res;
}
function computeLibSearchScore(p) {
  const q = libQuery.trim().toLowerCase();
  if (!q) return { score: 1, fuzzy: false };
  const fields = [[(p.name || "").toLowerCase(), 3], [(p.text || "").toLowerCase(), 1], [(p.file_path || "").toLowerCase(), 1]];
  let best = 0;
  for (const [val, w] of fields) {
    if (val && val.includes(q)) { const s = (val.startsWith(q) ? 120 : 100) * w; if (s > best) best = s; }
  }
  if (best) return { score: best, fuzzy: false };
  if (!flag("fuzzySearch")) return { score: 0, fuzzy: false };
  // Tokenize on any non-alphanumeric so "code," / "code." / code blocks split into
  // clean words (scripts without spaces, e.g. CJK, stay one token — substring wins).
  const qWords = fuzzyTokens(q);
  if (!qWords.length) return { score: 0, fuzzy: false };
  for (const [val, w] of fields) {
    if (!val) continue;
    const words = fuzzyTokens(val);
    let matched = 0;
    for (const qw of qWords) {
      const max = fuzzyMaxTyposFor(qw);
      if (words.some((vw) => fuzzyWordHit(qw, vw, max))) matched++;
    }
    if (matched === qWords.length) { const s = 40 * w; if (s > best) best = s; }
  }
  return { score: best, fuzzy: best > 0 };
}
// Split into alphanumeric tokens (Unicode-aware).
function fuzzyTokens(s) {
  return s.split(/[^\p{L}\p{N}]+/u).filter(Boolean);
}
// A query word matches a field word by substring, whole-word edit distance, or —
// for longer field words — edit distance against its leading prefix ("cde"→"code",
// "cde"→"codebase").
function fuzzyWordHit(qw, vw, max) {
  if (vw.includes(qw)) return true;
  if (editDistWithin(qw, vw, max) <= max) return true;
  if (vw.length > qw.length + max) return editDistWithin(qw, vw.slice(0, qw.length + max), max) <= max;
  return false;
}

function libMatches(p) {
  if (libFav && !p.favorite) return false;
  if (libType !== "all" && promptType(p) !== libType) return false;
  if (libColor !== "all" && (p.color || "") !== libColor) return false;
  return libSearchScore(p).score > 0;
}

// Color-filter dots (once per wrap). "All" + every palette color. Shared by the
// library and the copy-history journal.
function buildColorFilter(wrap, current) {
  if (!wrap || wrap.childElementCount) return; // built once; refreshColorUI clears first
  const mk = (color, cls) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = `lib-color ${cls}`.trim();
    b.dataset.color = color;
    if (color && color !== "all") b.style.background = color;
    if (color === current) b.classList.add("active");
    return b;
  };
  wrap.appendChild(mk("all", "lib-color-all"));
  for (const c of COLORS) if (c) wrap.appendChild(mk(c, ""));
}
function setActiveFilter(wrap, selector, attr, value) {
  for (const b of wrap.querySelectorAll(selector)) {
    b.classList.toggle("active", b.dataset[attr] === value);
  }
}
function setLibColor(color) {
  libColor = color;
  setActiveFilter($("library-colors"), ".lib-color", "color", color);
}

// F10 smart sort: order the library list. The usage-based options gate on smartSort.
function libSortMode() {
  const raw = settings.ui_texts?.librarySort || "default";
  if ((raw === "mostUsed" || raw === "recentUsed") && !flag("smartSort")) return "default";
  return raw;
}
function sortLibraryItems(items) {
  const mode = libSortMode();
  const q = libQuery.trim();
  const nameOf = (p) => libLabel(p).toLowerCase();
  const byName = (a, b) => nameOf(a).localeCompare(nameOf(b), LANG);
  if (mode === "default") {
    // Default + active query = relevance order (exact/prefix above fuzzy).
    if (q) items.sort((a, b) => (libSearchScore(b).score - libSearchScore(a).score) || byName(a, b));
    return;
  }
  if (mode === "name") { items.sort(byName); return; }
  // Usage/recency desc; unused (0) fall last, ties resolve by name — all stable.
  if (mode === "mostUsed") {
    items.sort((a, b) => ((settings.usage?.[b.id] || 0) - (settings.usage?.[a.id] || 0)) || byName(a, b));
  } else if (mode === "recentUsed") {
    items.sort((a, b) => ((settings.last_used?.[b.id] || 0) - (settings.last_used?.[a.id] || 0)) || byName(a, b));
  }
}
function fillLibrarySort() {
  const sel = $("library-sort");
  if (!sel) return;
  const opts = [["default", "sortDefault"], ["name", "sortName"]];
  if (flag("smartSort")) opts.push(["mostUsed", "sortMostUsed"], ["recentUsed", "sortRecentUsed"]);
  sel.innerHTML = "";
  for (const [v, k] of opts) {
    const o = document.createElement("option");
    o.value = v; o.textContent = t(k);
    sel.appendChild(o);
  }
  sel.value = libSortMode();
}

// Close the library and flush any deferred grid rebuild from batch edits.
function hideLibrary() {
  libraryEl.classList.add("hidden");
  if (gridDirty) { gridDirty = false; renderGrid(true); }
}

function renderLibrary() {
  const list = $("library-list");
  list.innerHTML = "";
  libScoreCache.clear(); // fresh scores each render (query / prompt content may have changed)
  fillLibrarySort();
  syncLibraryCols();
  if (_libObs) _libObs.disconnect();
  _libObs = makeVideoObserver();
  _activeVidObs = _libObs;
  const anyLibFilter = flag("librarySearch") || flag("libraryTypeFilter") || flag("colorFilter") || flag("favorites");
  $("library-search").classList.toggle("hidden", !prompts.length || !anyLibFilter);
  if (!prompts.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("libraryEmpty");
    list.appendChild(empty);
    return;
  }
  // Hidden filters keep their default state, so this is inert when they are off.
  const items = prompts.filter(libMatches);
  sortLibraryItems(items); // F10 chosen sort (stable), before the favorites float
  // Favorites float to the top (stable otherwise) when the feature is on.
  if (flag("favorites")) items.sort((a, b) => (b.favorite ? 1 : 0) - (a.favorite ? 1 : 0));
  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("libraryNoResults");
    list.appendChild(empty);
    return;
  }
  const placed = new Set(Object.keys(layoutOf(activeView())));
  libDisplayOrder = items.map((p) => p.id); // F8: range-select uses display order
  for (const p of items) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "lib-item";
    row.dataset.id = p.id; // F8: in-place selection toggle finds the row by id
    // F18: dim rows that only matched via the typo-tolerant tier.
    if (libQuery.trim() && libSearchScore(p).fuzzy) row.classList.add("lib-fuzzy");
    // F8: selection state (the checkbox itself is a CSS ::before, shown while the
    // list carries the .selecting class — so toggling select mode never re-renders).
    if (libSelectMode && libSelected.has(p.id)) row.classList.add("lib-selected");
    row.title = libSelectMode ? "" : t("copy");

    const body = document.createElement("span");
    body.className = "lib-body";
    const name = document.createElement("span");
    name.className = "lib-name";
    name.textContent = libLabel(p);
    const text = document.createElement("span");
    text.className = "lib-text";
    text.textContent = p.file_path || p.text;
    body.append(name, text);

    // Per-type preview: video plays inline, image/gif as a thumbnail, else a dot.
    row.append(thumbEl(p), body);

    // Place on the current layout: drag the row onto the grid, or one click.
    row.addEventListener("pointerdown", (e) => {
      if (libSelectMode) return; // no drag while multi-selecting
      if (e.button !== 0 || e.target.closest(".lib-add, .lib-edit, .lib-fav")) return;
      drag = { id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: row, fromLibrary: true };
    });
    // Add-to-view icon (leftmost of the action group): place on the current layout.
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
    // Star (middle): mark/unmark this prompt as a favorite (feature-gated).
    if (flag("favorites")) {
      const fav = document.createElement("span");
      fav.className = "icon-btn lib-fav" + (p.favorite ? " active" : "");
      fav.textContent = "★";
      fav.title = t(p.favorite ? "favoriteRemove" : "favoriteAdd");
      fav.addEventListener("pointerdown", (e) => e.stopPropagation());
      fav.addEventListener("click", async (e) => {
        e.stopPropagation();
        p.favorite = !p.favorite;
        try { await invoke("set_favorite", { id: p.id, favorite: p.favorite }); } catch (err) { toast(String(err)); }
        renderLibrary();
        if (favView) renderGrid(true); // favorites grid updates live underneath
      });
      row.appendChild(fav);
    }
    // Edit icon (rightmost): opens the edit dialog without copying.
    const edit = document.createElement("span");
    edit.className = "icon-btn lib-edit";
    edit.title = t("edit");
    edit.innerHTML = ICON_EDIT;
    edit.addEventListener("pointerdown", (e) => e.stopPropagation());
    edit.addEventListener("click", (e) => { e.stopPropagation(); editPrompt(p.id); });
    row.appendChild(edit);

    // Click copies the prompt (or toggles selection in batch mode).
    row.addEventListener("click", (e) => {
      if (libSelectMode) { toggleLibSelect(p.id, e.shiftKey); return; }
      libraryCopy(p);
    });
    list.appendChild(row);
  }
}

// ---- F8 batch operations ----
function setLibSelectMode(on) {
  libSelectMode = on;
  if (!on) { libSelected.clear(); libLastId = null; }
  const btn = $("library-select-btn");
  btn?.classList.toggle("active", on);
  btn?.setAttribute("aria-pressed", String(on));
  $("lib-batch-bar").classList.toggle("hidden", !on);
  // Toggle the checkbox column via a class — no list rebuild (instant on 1000s).
  $("library-list").classList.toggle("selecting", on);
  if (on) { buildBatchColors(); fillBatchViews(); }
  applyLibSelectionDom();
  updateBatchBar();
}
function toggleLibSelect(id, shift) {
  if (shift && libLastId && libDisplayOrder.includes(libLastId) && libDisplayOrder.includes(id)) {
    const a = libDisplayOrder.indexOf(libLastId), b = libDisplayOrder.indexOf(id);
    const [lo, hi] = a < b ? [a, b] : [b, a];
    const add = !libSelected.has(id); // extend using the target's new state
    for (let i = lo; i <= hi; i++) { if (add) libSelected.add(libDisplayOrder[i]); else libSelected.delete(libDisplayOrder[i]); }
  } else {
    if (libSelected.has(id)) libSelected.delete(id); else libSelected.add(id);
  }
  libLastId = id;
  applyLibSelectionDom(); // in-place class toggle — no full re-render (fast on 1000s)
  updateBatchBar();
}
// Reflect the current selection onto the visible rows without rebuilding the list.
function applyLibSelectionDom() {
  for (const row of $("library-list").querySelectorAll(".lib-item")) {
    row.classList.toggle("lib-selected", libSelected.has(row.dataset.id));
  }
}
function updateBatchBar() {
  const n = libSelected.size;
  $("lib-batch-count").textContent = t("selectedCount").replace("{n}", n.toLocaleString(LANG));
  const bar = $("lib-batch-bar");
  bar.querySelectorAll("button, select").forEach((el) => {
    if (["lib-batch-all", "lib-batch-none"].includes(el.id)) return; // always usable
    el.disabled = n === 0;
  });
}
function buildBatchColors() {
  const wrap = $("lib-batch-colors");
  if (!wrap || wrap.childElementCount) return;
  const mk = (color, cls) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = `lib-color ${cls}`.trim();
    b.dataset.color = color;
    if (color && color !== "none") b.style.background = color;
    b.title = color === "none" ? t("colorNone") : color;
    return b;
  };
  wrap.appendChild(mk("none", "lib-color-all"));
  for (const c of COLORS) if (c) wrap.appendChild(mk(c, ""));
}
function fillBatchViews() {
  const sel = $("lib-batch-view");
  if (!sel) return;
  sel.innerHTML = "";
  for (const v of settings.views || []) {
    const o = document.createElement("option");
    o.value = v.id; o.textContent = v.name;
    sel.appendChild(o);
  }
  if (settings.active_view) sel.value = settings.active_view;
}
async function doBatch(action, extra = {}) {
  const ids = [...libSelected];
  if (!ids.length) return;
  const idset = new Set(ids);
  try {
    // Backend now returns ONLY settings (no prompt echo) — so a batch over
    // image-heavy prompts no longer re-serialises every base64 blob over IPC.
    settings = await invoke("batch_prompts", {
      ids, action,
      color: extra.color ?? null,
      favorite: extra.favorite ?? null,
      viewId: extra.viewId ?? null,
    });
  } catch (e) { toast(String(e)); return; }
  // Mirror the change onto the local prompt list ourselves.
  if (action === "delete") prompts = prompts.filter((p) => !idset.has(p.id));
  else if (action === "favorite") { for (const p of prompts) if (idset.has(p.id)) p.favorite = extra.favorite ?? true; }
  else if (action === "color") { for (const p of prompts) if (idset.has(p.id)) p.color = extra.color ?? ""; }
  // The grid sits hidden behind the library modal — rebuilding it (a full fit-text
  // pass) is deferred until the library closes.
  gridDirty = true;
  if (action === "delete") {
    // Remove the deleted rows in place instead of rebuilding the whole list.
    for (const id of ids) $("library-list").querySelector(`.lib-item[data-id="${CSS.escape(id)}"]`)?.remove();
    libDisplayOrder = libDisplayOrder.filter((x) => !idset.has(x));
    libSelected.clear(); libLastId = null;
  } else {
    // favorite/color/view can reorder or change row content → one rebuild.
    if (action === "addView" || action === "removeView") renderViews();
    renderLibrary();
  }
  updateBatchBar();
}
async function batchExport(format) {
  const ids = [...libSelected];
  if (!ids.length) return;
  try {
    const n = await invoke("export_prompts", { format, ids });
    toast(t("exportDone").replace("{n}", String(n)));
  } catch (e) {
    if (String(e) !== "canceled") toast(String(e));
  }
}

// Copy a prompt from the library (handles {{variables}}), then optionally close.
// Copy a prompt, asking for {{variables}} first when applicable. Returns false
// if the copy failed or the user cancelled the variable dialog. Shared by the
// library and the copy-history journal so both honour variable prompts.
async function copyResolved(p) {
  const useVars = flag("promptVars") && !p.copy_image && !p.file_path && extractVars(p.text).length;
  if (useVars) {
    const values = await promptVarsDialog(extractVars(p.text));
    if (!values) return false;
    return invoke("copy_text", { text: fillVars(p.text, values) }).catch(() => false);
  }
  return invoke("copy_prompt", { id: p.id }).catch((e) => { toast(String(e)); return false; });
}

async function libraryCopy(p) {
  const hasVars = flag("promptVars") && !p.copy_image && !p.file_path && extractVars(p.text).length > 0;
  // Variable prompts skip the double-click shortcut: they must reach the dialog
  // (copyResolved) so the variables get asked before copy + auto-paste.
  if (!hasVars && autoPasteDouble(p.id)) { await autoPasteNow(null); return; } // F19: 2nd fast click pastes
  if (copyOnCooldown(p.id)) return;
  if (!(await copyResolved(p))) return;
  recordCopy(p.id);
  toast(t("copied"));
  if (flag("closeAfterCopy")) hideLibrary();
}

// Reflect the close-after-copy flag on the library header toggle.
function syncLibraryToggle() {
  const btn = $("library-close-toggle");
  const on = flag("closeAfterCopy");
  btn.classList.toggle("active", on);
  btn.setAttribute("aria-pressed", String(on));
  btn.classList.toggle("hidden", !flag("libraryCloseToggle"));
}

// Library list columns: a user-chosen count (1 = one wide column by default), with
// the number of options set by the libraryMaxCols expert value.
const libCols = () => {
  const n = parseInt(settings.ui_texts?.libCols, 10) || 1;
  return Math.min(Math.max(1, n), val("libraryMaxCols"));
};
async function setLibCols(n) {
  const value = String(n);
  if ((settings.ui_texts?.libCols || "1") === value) return;
  settings.ui_texts = { ...(settings.ui_texts || {}), libCols: value };
  try { await invoke("set_ui_text", { key: "libCols", value }); } catch (err) { toast(String(err)); }
  syncLibraryCols();
}
// Rebuild the 1..max segmented toggle and apply the current column count.
function syncLibraryCols() {
  const max = val("libraryMaxCols");
  const cur = libCols();
  $("library-list").style.setProperty("--lib-cols", cur);
  const seg = $("lib-cols-seg");
  seg.innerHTML = "";
  for (let n = 1; n <= max; n++) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "seg-btn" + (n === cur ? " active" : "");
    b.textContent = String(n);
    b.setAttribute("aria-label", `${n} ${t("libraryColumns")}`);
    b.addEventListener("click", () => setLibCols(n));
    seg.appendChild(b);
  }
}

// ---- Copy history & usage journal ----

function journalRow(p, subtitle, onClick, subtitle2) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "lib-item";
  row.title = t("copy");
  const body = document.createElement("span");
  body.className = "lib-body";
  const name = document.createElement("span");
  name.className = "lib-name";
  name.textContent = libLabel(p);
  body.append(name);
  if (subtitle) {
    const text = document.createElement("span");
    text.className = "lib-text";
    text.textContent = subtitle;
    body.append(text);
  }
  if (subtitle2 && typeof subtitle2 === "object") {
    // Two-part sub-line: left text + right text on one row (e.g. time + count).
    const sub = document.createElement("span");
    sub.className = "lib-sub lib-subrow";
    const l = document.createElement("span");
    l.textContent = subtitle2.left || "";
    const r = document.createElement("span");
    r.className = "lib-sub-right";
    r.textContent = subtitle2.right || "";
    sub.append(l, r);
    body.append(sub);
  } else if (subtitle2) {
    const sub = document.createElement("span");
    sub.className = "lib-sub";
    sub.textContent = subtitle2;
    body.append(sub);
  }
  row.append(thumbEl(p), body);
  row.addEventListener("click", onClick);
  return row;
}

// Format a unix-seconds copy timestamp for the history column.
const fmtCopyTime = (ts) => {
  if (!ts) return "";
  return new Date(ts * 1000).toLocaleString(LANG, { dateStyle: "short", timeStyle: "short" });
};

function journalMatches(p) {
  if (journalType !== "all" && promptType(p) !== journalType) return false;
  if (journalColor !== "all" && (p.color || "") !== journalColor) return false;
  const q = journalQuery.trim().toLowerCase();
  if (!q) return true;
  return [p.name, p.text, p.file_path].some((s) => (s || "").toLowerCase().includes(q));
}

function setJournalType(type) {
  journalType = type;
  setActiveFilter($("journal-types"), ".lib-type", "type", type);
}
function setJournalColor(color) {
  journalColor = color;
  setActiveFilter($("journal-colors"), ".lib-color", "color", color);
}

let journalLog = []; // cached newest copy events; refetched only on open/group/copy
let journalRecentRev = false; // recently-copied sorted oldest-first
let journalUsedRev = false; // most-used sorted least-first

// Refetch the copy log (the query changes: open, grouped toggle, after a copy).
async function loadJournal() {
  const grouped = flag("historyGrouped");
  journalLog = await invoke("recent_copies", { limit: val("journalLimit"), grouped }).catch(() => []);
  renderJournal();
}

// Render from the cached log. Filters run client-side, so typing in the search
// box re-renders instantly without an IPC roundtrip (and without a render race).
function renderJournal() {
  const body = $("journal-body");
  if (_journalObs) _journalObs.disconnect();
  _journalObs = makeVideoObserver();
  _activeVidObs = _journalObs;
  const byId = new Map(prompts.map((p) => [p.id, p]));
  const grouped = flag("historyGrouped");
  const showTime = flag("historyTimestamps");
  let recent = journalLog
    .map((e) => [byId.get(e.id), e.ts])
    .filter(([p]) => p && journalMatches(p));
  if (journalRecentRev) recent = recent.slice().reverse();
  // Most used: copy count + last-used timestamp, busiest first (or least-first).
  const lastUsed = settings.last_used || {};
  const used = Object.entries(settings.usage || {})
    .map(([id, n]) => [byId.get(id), n, lastUsed[id] || 0])
    .filter(([p]) => p && journalMatches(p))
    .sort((a, b) => (journalUsedRev ? a[1] - b[1] : b[1] - a[1]))
    .slice(0, 20);
  body.innerHTML = "";
  const copyAgain = (p) => async () => {
    if (copyOnCooldown(p.id)) return; // same prompt copied moments ago
    if (await copyResolved(p)) { // handles {{variables}} like the library does
      await recordCopy(p.id); // await: the log must refetch AFTER the copy is recorded
      toast(t("copied"));
      settings = await invoke("get_settings");
      await loadJournal();
    }
  };
  // `rows` items are [prompt, subtitle, subtitle2?]. `extra` is an optional
  // control rendered to the right of the column title.
  const column = (titleKey, rows, extra, cls) => {
    const col = document.createElement("div");
    col.className = "journal-col" + (cls ? " " + cls : "");
    const head = document.createElement("div");
    head.className = "lib-section journal-head";
    const title = document.createElement("span");
    title.textContent = t(titleKey);
    head.append(title);
    if (extra) head.append(extra);
    col.appendChild(head);
    if (!rows.length) {
      const e = document.createElement("div");
      e.className = "hint";
      e.textContent = t("journalEmpty");
      col.appendChild(e);
    } else {
      for (const [p, sub, sub2] of rows) col.appendChild(journalRow(p, sub, copyAgain(p), sub2));
    }
    return col;
  };
  // Reverse-sort arrow for a column title (▼ newest/most first, ▲ reversed).
  const sortBtn = (rev, onClick) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "journal-sort" + (rev ? " rev" : "");
    b.textContent = rev ? "▲" : "▼";
    b.title = t("sortToggle");
    b.setAttribute("aria-label", t("sortToggle"));
    b.addEventListener("click", onClick);
    return b;
  };
  const headExtras = (...nodes) => {
    const s = document.createElement("span");
    s.className = "journal-head-extras";
    for (const n of nodes) if (n) s.append(n);
    return s;
  };
  // Toggle beside the "recently copied" title: group repeats vs show every copy.
  let groupBtn = null;
  if (flag("journalGroupBtn")) {
    groupBtn = document.createElement("button");
    groupBtn.type = "button";
    groupBtn.className = "journal-toggle" + (grouped ? " active" : "");
    groupBtn.textContent = t("journalGrouped");
    groupBtn.title = t("journalGrouped");
    groupBtn.setAttribute("aria-pressed", String(grouped));
    groupBtn.addEventListener("click", async () => {
      const next = !grouped;
      settings.ui_flags = { ...(settings.ui_flags || {}), historyGrouped: next };
      try { await invoke("set_ui_flag", { key: "historyGrouped", enabled: next }); } catch (err) { toast(String(err)); }
      await loadJournal(); // grouping changes the query → refetch
    });
  }
  const recentSort = flag("journalRecentSort")
    ? sortBtn(journalRecentRev, () => { journalRecentRev = !journalRecentRev; renderJournal(); })
    : null;
  const usedSort = flag("journalUsedSort")
    ? sortBtn(journalUsedRev, () => { journalUsedRev = !journalUsedRev; renderJournal(); })
    : null;
  body.append(
    column("recentlyCopied", recent.map(([p, ts]) => [p, showTime ? fmtCopyTime(ts) : ""]), headExtras(groupBtn, recentSort), "journal-col-recent"),
    column("mostUsed", used.map(([p, n, ts]) => [
      p,
      "",
      // One sub-line: last-used time on the left, copy count on the right.
      { left: showTime && ts ? fmtCopyTime(ts) : "", right: t("usedTimes").replace("{n}", n) },
    ]), headExtras(usedSort), "journal-col-used"),
  );
}

async function openJournal() {
  settings = await invoke("get_settings");
  journalQuery = "";
  if ($("journal-q")) $("journal-q").value = "";
  setJournalType("all");
  setJournalColor("all");
  // Hide the whole filter bar when every filter is switched off (expert menu).
  const anyFilter = flag("journalSearch") || flag("journalTypeFilter") || flag("journalColorFilter");
  $("journal-search").classList.toggle("hidden", !anyFilter);
  await loadJournal();
  $("journal").classList.remove("hidden");
  if (flag("journalSearch") && optFlag("searchAutofocus")) $("journal-q").focus();
}


// ---- Settings: views editor ----
// Views are managed straight from the top view tabs: the "+" adds one, a
// right-click opens the view popup (rename, grid size, color, delete). The old
// settings-page views editor was removed.

// ---- Settings actions ----
async function runExport(format) {
  try {
    await withDialog(() => invoke("export_prompts", { format }));
  } catch (err) {
    if (String(err) !== "canceled") toast(t("exportFailed"));
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
    fillScaleSelect("ui-scale", "uiScale");
    fillScaleSelect("icon-scale", "iconScale");
    $("opt-minimize").checked = settings.minimize_to_tray === true;
    $("opt-screenshot-folder").checked = settings.ui_flags?.screenshotSave === true;
    $("opt-tooltips").checked = settings.ui_flags?.tooltipsEnabled !== false;
    $("opt-autopaste").checked = settings.ui_flags?.autoPaste !== false;
    $("opt-autoupdate").checked = settings.auto_update !== false;
    applyTheme(await invoke("current_theme"));
    applyTileStyle();
    applyBars();
    await renderGrid(true); // re-render with the new tile style
    toast(`${count} ${t("imported")}`);
    maybeDupImportToast(); // F6: flag near-duplicates the import may have introduced
  } catch (err) {
    if (String(err) !== "canceled") toast(t("importFailed"));
  }
}

// "Everything" backup: prompts + all settings in one JSON file.
async function runExportAll() {
  try {
    await withDialog(() => invoke("export_all"));
  } catch (err) {
    if (String(err) !== "canceled") toast(t("exportFailed"));
  }
}

async function runImportAll() {
  const path = await withDialog(() => invoke("pick_backup_file")).catch(() => null);
  if (!path) return;
  // The file stays picked while the password is retried, so a typo does not send
  // the user back through the file picker.
  let password = null;
  let asked = false;
  for (;;) {
    try {
      const res = await invoke("import_all", { path, password });
      await reloadEverything();
      await infoDialog(t("backupImport"), t("importAllDone").replace("{f}", res.name).replace("{n}", res.count));
      return;
    } catch (err) {
      const m = String(err);
      if (!/password/i.test(m)) {
        await infoDialog(t("importFailed"), m); // the reason, not just "failed"
        return;
      }
      password = await confirmDialog({
        title: t("backupPwTitle"),
        message: asked ? t("backupPwWrong") : t("backupPwBody"),
        confirmLabel: t("backupRestore"),
        cancelLabel: t("cancel"),
        icon: LOCK_SVG,
        input: { placeholder: t("backupPasswordLabel") },
        danger: false,
      });
      asked = true;
      if (password === null) return; // dismissed
    }
  }
}

// Portable start with an installed copy's store present: offer to adopt it. Runs
// after the window is up, so the dialog already has the theme and language.
async function offerTakeover() {
  const src = await invoke("takeover_offer").catch(() => null);
  if (!src) return;
  const adopt = await confirmDialog({
    title: t("takeoverTitle"),
    message: t("takeoverBody").replace("{path}", src),
    confirmLabel: t("takeoverAdopt"),
    cancelLabel: t("takeoverSkip"),
    icon: TAKEOVER_SVG,
    danger: false,
  });
  try {
    await invoke("takeover_apply", { adopt });
  } catch (e) {
    toast(String(e));
    return;
  }
  if (adopt) await reloadEverything();
}

// Delete all DATA (prompts, versions, copy history, clip inbox) — settings kept.
async function deleteData() {
  const btn = $("delete-data");
  if (!armButton(btn, t("deleteDataConfirm"))) {
    clearTimeout(deleteAllTimer);
    deleteAllTimer = setTimeout(() => disarmButton(btn, t("deleteData")), DISARM_MS);
    return;
  }
  clearTimeout(deleteAllTimer);
  disarmButton(btn, t("deleteData"));
  await maybeBackupBeforeWipe();
  await invoke("delete_all_data");
  await reloadEverything(); // in place: a page reload repaints the whole window
}

// Expert opt: take a safety backup before a destructive reset/delete.
async function maybeBackupBeforeWipe() {
  if (!optFlag("autoBackupBeforeWipe")) return;
  try { await invoke("create_backup", { keep: Math.round(val("backupKeep")) }); } catch { /* best-effort */ }
}

// Reset all SETTINGS to defaults (theme, views, toggles, expert values) — prompts kept.
async function resetSettings() {
  const btn = $("reset-settings");
  if (!armButton(btn, t("resetSettingsConfirm"))) {
    clearTimeout(resetSettingsTimer);
    resetSettingsTimer = setTimeout(() => disarmButton(btn, t("resetSettings")), DISARM_MS);
    return;
  }
  clearTimeout(resetSettingsTimer);
  disarmButton(btn, t("resetSettings"));
  await maybeBackupBeforeWipe();
  await invoke("reset_settings");
  await reloadEverything(); // in place: a page reload repaints the whole window
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
  modal.closeX.addEventListener("click", closeModal);
  modal.fav.addEventListener("click", () => {
    const on = modal.fav.classList.toggle("active");
    modal.fav.setAttribute("aria-pressed", String(on));
  });
  // Click the preview media → fullscreen zoom/pan viewer.
  modal.img.addEventListener("click", () => { if (modal.img.src) openLightbox(modal.img.src, false); });
  modal.video.addEventListener("click", () => { if (modal.video.src) openLightbox(modal.video.src, true); });
  modal.name.addEventListener("keydown", (e) => { if (e.key === "Enter") confirmModal(); });
  modal.text.addEventListener("input", scheduleLenCounter); // F16 live length readout
  // F7 version-history collapsible.
  $("modal-history-head").addEventListener("click", () => {
    const body = $("modal-history-body");
    const open = !body.classList.toggle("hidden");
    $("modal-history-head").setAttribute("aria-expanded", String(open));
    $("modal-history").classList.toggle("open", open);
    // In tall (text-edit) mode the textarea flex-grows and eats the height the open
    // history needs; history-open caps it (CSS) so the list + Save row stay reachable.
    modal.root.classList.toggle("history-open", open);
    if (open) {
      loadHistory(modalState?.id);
      // Scroll the modal's own scroll container to the bottom so the freshly opened
      // history (and the Save row below it) come fully into view on small windows.
      // scrollIntoView("nearest") only nudged the already-visible head, hence unreliable.
      const scroller = $("modal-history").closest(".modal");
      if (scroller) requestAnimationFrame(() => requestAnimationFrame(() => {
        scroller.scrollTo({ top: scroller.scrollHeight, behavior: "smooth" });
      }));
    }
  });

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

  // Chain mode: combine several text buttons into one clipboard copy. Single
  // click toggles it; a quick double-click latches it "always on".
  $("chain-btn").addEventListener("click", onChainClick);
  // A paste ends chain mode (unless latched). Ctrl/Cmd+V is only seen while we're
  // focused; leaving the window to paste elsewhere blurs it — both finalize (the
  // clipboard already holds the chain). Clicking the chain button is the manual off.
  document.addEventListener("keydown", (e) => {
    if (chainMode && !chainLock && (e.ctrlKey || e.metaKey) && (e.key === "v" || e.key === "V")) setChainMode(false);
  });
  // Arrow keys move a focus ring across grid tiles; Enter/Space copies.
  const NAV = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
  document.addEventListener("keydown", (e) => {
    if (!flag("keyboardNav")) return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.target.closest("input, textarea, [contenteditable]")) return;
    if (document.querySelector(".overlay:not(.hidden)")) return; // a dialog is open
    if (NAV[e.key]) { e.preventDefault(); moveKbFocus(...NAV[e.key]); }
    else if ((e.key === "Enter" || e.key === " ") && kbFocus) { e.preventDefault(); activateTile(kbFocus); }
  });
  // Hold Shift during keyboard navigation to arm chaining on the fly: it switches
  // on while Shift is down and off again when released — unless a prompt was picked
  // into the chain during the hold, in which case it latches "always on" so the
  // user can keep building the chain from there.
  let shiftChain = false;
  const kbBlocked = (e) => e.target.closest("input, textarea, [contenteditable]") || document.querySelector(".overlay:not(.hidden)");
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Shift" || e.repeat || shiftChain || chainMode) return;
    if (!flag("keyboardNav") || !flag("chainPrompts") || kbBlocked(e)) return;
    shiftChain = true;
    setChainMode(true);
  });
  document.addEventListener("keyup", (e) => {
    if (e.key !== "Shift" || !shiftChain) return;
    shiftChain = false;
    if (!chainSel.length) { setChainMode(false); return; }
    if (flag("chainLock")) setChainLock(); // picked something → keep the chain alive
  });
  // Keep the chain armed while a variable dialog is open — switching windows to
  // look something up must not abort the chain (it leaves the popup stranded). A
  // latched chain ignores blur entirely (that is the point of the lock).
  window.addEventListener("blur", () => { if (chainMode && chainSel.length && !varsCleanup && !chainLock) setChainMode(false); });
  // A latched chain survives the blur but starts fresh when we return: the
  // previous chain was already copied, so clear the picks (mode stays on).
  window.addEventListener("focus", () => { if (chainLock && chainSel.length) { chainSel.length = 0; chainTexts.clear(); refreshChainUi(); } });
  const snipModal = $("snip-modal");
  let pendingSnip = null;
  let pendingSnipPath = ""; // disk path if this shot was auto-saved to the folder
  let pendingSnipName = ""; // backend-suggested "Screenshot <app?> <timestamp>"
  const closeSnipModal = () => {
    snipModal.classList.add("hidden");
    pendingSnip = null;
    pendingSnipPath = "";
    pendingSnipName = "";
  };
  listen("snip-captured", (e) => {
    pendingSnip = e.payload?.data_url || "";
    if (!pendingSnip) return;
    pendingSnipPath = e.payload?.path || "";
    pendingSnipName = e.payload?.name || "";
    $("snip-preview").src = pendingSnip;
    $("snip-save-folder").checked = settings.ui_flags?.screenshotSave === true;
    snipModal.classList.remove("hidden");
  });
  // Folder toggle on the result dialog = the same setting as in the main settings.
  // On -> save this shot to the folder now; off -> delete the saved copy.
  $("snip-save-folder").addEventListener("change", async (ev) => {
    const on = ev.target.checked;
    settings.ui_flags = { ...(settings.ui_flags || {}), screenshotSave: on };
    invoke("set_ui_flag", { key: "screenshotSave", enabled: on }).catch((err) => toast(String(err)));
    const optBox = $("opt-screenshot-folder");
    if (optBox) optBox.checked = on;
    if (on) {
      if (!pendingSnipPath && pendingSnip) {
        pendingSnipPath = (await invoke("save_screenshot_now", { dataUrl: pendingSnip }).catch(() => "")) || "";
      }
    } else if (pendingSnipPath) {
      invoke("delete_screenshot_file", { path: pendingSnipPath }).catch(() => {});
      pendingSnipPath = "";
    }
  });
  $("snip-modal-saveas").addEventListener("click", async () => {
    if (!pendingSnip) return;
    await invoke("save_screenshot_as", { dataUrl: pendingSnip }).catch((err) => toast(String(err)));
  });
  $("snip-modal-yes").addEventListener("click", () => {
    const image = pendingSnip;
    const name = pendingSnipName || t("snipTitle");
    closeSnipModal();
    if (!image) return;
    // The screenshot becomes a button that copies the image on click.
    openModal({
      mode: "image-create",
      title: t("imageModalTitle"),
      name,
      image,
      showImage: true,
      copyImage: true,
    });
  });
  // Discard this shot and immediately start a new one.
  $("snip-modal-retry").addEventListener("click", () => {
    closeSnipModal();
    invoke("open_snip").catch((err) => toast(String(err)));
  });
  $("snip-modal-close").addEventListener("click", closeSnipModal);
  $("snip-preview").addEventListener("click", () => { const s = $("snip-preview").src; if (s) openLightbox(s, false); });
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
    settingsEl.classList.remove("hidden");
  });
  $("expert-open").addEventListener("click", () => {
    expertQuery = "";
    $("expert-search").value = "";
    showExpertPage(true);
  });
  $("expert-back").addEventListener("click", () => showExpertPage(false));
  $("expert-search").addEventListener("input", (e) => { expertQuery = e.target.value; renderExpert(); });
  // Two-step confirm: first click arms it red, second within 3s resets.
  $("expert-reset").addEventListener("click", (e) => {
    const btn = e.currentTarget;
    if (!armButton(btn, t("expertResetConfirm"))) {
      clearTimeout(expertResetTimer);
      expertResetTimer = setTimeout(() => disarmButton(btn, t("expertReset")), DISARM_MS);
      return;
    }
    clearTimeout(expertResetTimer);
    disarmButton(btn, t("expertReset"));
    onResetExpert();
  });

  // Quick grid-size control (top-right of the layout, active view).
  const applyQuickGrid = async () => {
    if (favView) return; // favorites grid sizes itself; the control is read-only there
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

  buildColorFilter($("library-colors"), libColor);
  $("library-btn").addEventListener("click", () => {
    libQuery = "";
    $("library-q").value = "";
    setLibType("all");
    setLibColor("all");
    libFav = false;
    $("library-fav").classList.remove("active");
    // Always open in normal (non-select) mode.
    libSelectMode = false; libSelected.clear(); libLastId = null;
    $("library-select-btn").classList.remove("active");
    $("library-select-btn").setAttribute("aria-pressed", "false");
    $("lib-batch-bar").classList.add("hidden");
    syncLibraryToggle();
    renderLibrary();
    libraryEl.classList.remove("hidden");
    if (flag("librarySearch") && optFlag("searchAutofocus")) $("library-q").focus();
  });
  // "Close after copy" toggle in the library header (mirrors the expert flag).
  $("library-close-toggle").addEventListener("click", async () => {
    const enabled = !flag("closeAfterCopy");
    settings.ui_flags = { ...(settings.ui_flags || {}), closeAfterCopy: enabled };
    try { await invoke("set_ui_flag", { key: "closeAfterCopy", enabled }); } catch (err) { toast(String(err)); }
    syncLibraryToggle();
  });
  $("library-q").addEventListener("input", (e) => { libQuery = e.target.value; renderLibrary(); });
  wireSearchSuggest($("library-q"), "recentSearchLib");
  $("library-sort").addEventListener("change", (e) => { // F10
    settings.ui_texts = { ...(settings.ui_texts || {}), librarySort: e.target.value };
    invoke("set_ui_text", { key: "librarySort", value: e.target.value }).catch((err) => toast(String(err)));
    renderLibrary();
  });
  $("library-types").addEventListener("click", (e) => {
    if (e.target.closest("#library-fav")) {
      libFav = !libFav;
      $("library-fav").classList.toggle("active", libFav);
      renderLibrary();
      return;
    }
    const btn = e.target.closest(".lib-type");
    if (btn) { setLibType(btn.dataset.type); renderLibrary(); }
  });
  $("library-colors").addEventListener("click", (e) => {
    const btn = e.target.closest(".lib-color");
    if (btn) { setLibColor(btn.dataset.color); renderLibrary(); }
  });
  // F8 batch operations wiring.
  $("library-select-btn").addEventListener("click", () => setLibSelectMode(!libSelectMode));
  $("lib-batch-all").addEventListener("click", () => { for (const id of libDisplayOrder) libSelected.add(id); applyLibSelectionDom(); updateBatchBar(); });
  $("lib-batch-none").addEventListener("click", () => { libSelected.clear(); libLastId = null; applyLibSelectionDom(); updateBatchBar(); });
  $("lib-batch-colors").addEventListener("click", (e) => {
    const b = e.target.closest(".lib-color");
    if (b) doBatch("color", { color: b.dataset.color === "none" ? "" : b.dataset.color });
  });
  $("lib-batch-fav").addEventListener("click", () => doBatch("favorite", { favorite: true }));
  $("lib-batch-unfav").addEventListener("click", () => doBatch("favorite", { favorite: false }));
  $("lib-batch-addview").addEventListener("click", () => doBatch("addView", { viewId: $("lib-batch-view").value }));
  $("lib-batch-removeview").addEventListener("click", () => doBatch("removeView", { viewId: $("lib-batch-view").value }));
  $("lib-batch-csv").addEventListener("click", () => batchExport("csv"));
  $("lib-batch-txt").addEventListener("click", () => batchExport("txt"));
  $("lib-batch-delete").addEventListener("click", async () => {
    const n = libSelected.size;
    if (!n) return;
    const ok = await confirmDialog({ title: t("delete"), message: t("batchDeleteConfirm").replace("{n}", String(n)), confirmLabel: t("delete") });
    if (ok) doBatch("delete");
  });
  // Esc leaves selection mode before it closes the library (capture beats the overlay).
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && libSelectMode && !libraryEl.classList.contains("hidden")) {
      e.stopPropagation();
      setLibSelectMode(false);
    }
  }, true);
  $("library-close").addEventListener("click", hideLibrary);
  libraryEl.addEventListener("pointerdown", (e) => {
    if (e.target === libraryEl) hideLibrary();
  });
  // F6 duplicate finder modal (backups + statistics are now inline expert panels).
  $("dupes-close").addEventListener("click", () => $("dupes-modal").classList.add("hidden"));
  $("dupes-modal").addEventListener("pointerdown", (e) => { if (e.target === $("dupes-modal")) $("dupes-modal").classList.add("hidden"); });
  $("dupes-rescan").addEventListener("click", () => scanDupes());
  // F21 clipboard inbox.
  $("clip-btn").addEventListener("click", openClipInbox);
  // Pause/resume collecting straight from the inbox — same switch as the expert one.
  $("clip-toggle").addEventListener("click", async () => {
    const on = !optFlag("clipWatcher");
    settings.ui_flags = { ...(settings.ui_flags || {}), clipWatcher: on };
    try { await invoke("set_ui_flag", { key: "clipWatcher", enabled: on }); }
    catch (e) { toast(String(e)); }
    applyFlags();
    await refreshClipInbox();
    updateClipBadge();
  });
  $("clip-close").addEventListener("click", () => $("clip-modal").classList.add("hidden"));
  $("clip-modal").addEventListener("pointerdown", (e) => { if (e.target === $("clip-modal")) $("clip-modal").classList.add("hidden"); });
  $("clip-clear").addEventListener("click", async () => { await invoke("clip_inbox_clear").catch((e) => toast(String(e))); refreshClipInbox(); });
  listen("clip-inbox-changed", () => refreshClipInbox());
  // Clicking the app logo opens the project's GitHub page (drag still moves the window).
  $("app-logo")?.addEventListener("click", () => invoke("open_repo").catch((e) => toast(String(e))));

  // Copy history & usage journal.
  const journalEl = $("journal");
  buildColorFilter($("journal-colors"), journalColor);
  $("journal-btn").addEventListener("click", openJournal);
  $("journal-close").addEventListener("click", () => journalEl.classList.add("hidden"));
  journalEl.addEventListener("pointerdown", (e) => {
    if (e.target === journalEl) journalEl.classList.add("hidden");
  });
  $("journal-q").addEventListener("input", (e) => { journalQuery = e.target.value; renderJournal(); });
  wireSearchSuggest($("journal-q"), "recentSearchJournal");
  $("journal-types").addEventListener("click", (e) => {
    const btn = e.target.closest(".lib-type");
    if (btn) { setJournalType(btn.dataset.type); renderJournal(); }
  });
  $("journal-colors").addEventListener("click", (e) => {
    const btn = e.target.closest(".lib-color");
    if (btn) { setJournalColor(btn.dataset.color); renderJournal(); }
  });
  $("journal-clear").addEventListener("click", async () => {
    if (flag("confirmClearHistory")) {
      const ok = await confirmDialog({
        title: t("clearHistory"),
        message: t("clearHistoryConfirm"),
        confirmLabel: t("clearHistory"),
      });
      if (!ok) return;
    }
    await invoke("clear_copy_history").catch((e) => toast(String(e)));
    settings.usage = {};
    settings.last_used = {};
    await loadJournal();
  });
  $("settings-close").addEventListener("click", hideSettings);
  // pointerdown (not click): selecting text that ends outside an input must not close.
  settingsEl.addEventListener("pointerdown", (e) => { if (e.target === settingsEl) hideSettings(); });

  // View add / rename / delete popup.
  viewModal.confirm.addEventListener("click", confirmViewModal);
  // Grid size uses the same dropdown number picker as the main layout; values are
  // applied on Save, so the pick callback is a no-op here.
  attachGridPicker(viewModal.cols, () => {});
  attachGridPicker(viewModal.rows, () => {});
  viewModal.name.addEventListener("keydown", (e) => {
    if (e.key === "Enter") confirmViewModal();
  });
  viewModal.close.addEventListener("click", closeViewModal);
  viewModal.root.addEventListener("pointerdown", (e) => {
    if (e.target === viewModal.root) closeViewModal();
  });
  // Two-step delete: first click arms (red), second click removes.
  viewModal.delete.addEventListener("click", async () => {
    if (!armButton(viewModal.delete, `${t("delete")}?`)) {
      setTimeout(() => disarmButton(viewModal.delete, t("delete")), DISARM_MS);
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
    await renderGrid(true);
  });

  $("opt-minimize").addEventListener("change", (e) => {
    invoke("set_minimize_on_close", { enabled: e.target.checked }).catch((err) => toast(String(err)));
  });
  $("opt-screenshot-folder").addEventListener("change", (e) => {
    settings.ui_flags = { ...(settings.ui_flags || {}), screenshotSave: e.target.checked };
    invoke("set_ui_flag", { key: "screenshotSave", enabled: e.target.checked }).catch((err) => toast(String(err)));
  });
  // Master tooltip switch (default on): off = no hover tooltips anywhere.
  $("opt-tooltips").addEventListener("change", (e) => {
    settings.ui_flags = { ...(settings.ui_flags || {}), tooltipsEnabled: e.target.checked };
    invoke("set_ui_flag", { key: "tooltipsEnabled", enabled: e.target.checked }).catch((err) => toast(String(err)));
  });
  // F19 auto-paste (default on): normal setting; enter-after stays in the expert menu.
  $("opt-autopaste").addEventListener("change", (e) => {
    settings.ui_flags = { ...(settings.ui_flags || {}), autoPaste: e.target.checked };
    invoke("set_ui_flag", { key: "autoPaste", enabled: e.target.checked }).catch((err) => toast(String(err)));
  });
  // F21 clipboard inbox is toggled from the expert menu + toolbar right-click only.
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
    $("pin-top").setAttribute("aria-pressed", String(on));
    invoke("set_always_on_top", { enabled: on }).catch((err) => toast(String(err)));
  };
  $("opt-ontop").addEventListener("change", (e) => setOnTop(e.target.checked));
  $("pin-top").addEventListener("click", () => setOnTop(!settings.always_on_top));

  // Favorites view: setting enables the header star; the star toggles the grid.
  $("opt-favview").addEventListener("change", (e) => {
    const on = e.target.checked;
    settings.ui_flags = { ...(settings.ui_flags || {}), favView: on };
    invoke("set_ui_flag", { key: "favView", enabled: on }).catch((err) => toast(String(err)));
    if (!on && favView) favView = false;
    refreshFavViewUi();
    renderGrid(true);
  });
  $("fav-view-btn").addEventListener("click", () => setFavView(!favView));

  // Global hotkey: click the field and press a key combo to capture it.
  $("opt-hotkey").addEventListener("keydown", async (e) => {
    if (e.key === "Tab" || e.key === "Escape") return; // let focus leave the field
    e.preventDefault();
    const accel = accelFromEvent(e);
    if (!accel) return; // a modifier + key is required
    try {
      await invoke("set_hotkey", { hotkey: accel });
      settings.hotkey = accel;
      $("opt-hotkey").value = prettyAccel(accel);
      toast(t("hotkeySet"));
    } catch (err) {
      $("opt-hotkey").value = prettyAccel(settings.hotkey);
      toast(t("hotkeyFail"));
    }
  });
  $("hotkey-clear").addEventListener("click", async () => {
    await invoke("set_hotkey", { hotkey: "" }).catch(() => {});
    settings.hotkey = "";
    $("opt-hotkey").value = "";
  });
  // The global hotkey summons the window + opens the quick launcher (library).
  listen("summon-launcher", () => { $("library-btn").click(); });

  $("import-btn").addEventListener("click", runImport);
  $("export-csv").addEventListener("click", () => runExport("csv"));
  $("export-txt").addEventListener("click", () => runExport("txt"));
  $("import-all").addEventListener("click", runImportAll);
  $("export-all").addEventListener("click", runExportAll);
  $("reset-settings").addEventListener("click", resetSettings);
  $("delete-data").addEventListener("click", deleteData);
  $("settings-backup-btn").addEventListener("click", openBackups);
  $("backup-close").addEventListener("click", closeBackups);
  $("backup-modal").addEventListener("pointerdown", (e) => {
    if (e.target === $("backup-modal")) closeBackups();
  });

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

  // UI size + icon size: percentage-preset selects mirroring the expert scales.
  fillScaleSelect("ui-scale", "uiScale");
  fillScaleSelect("icon-scale", "iconScale");
  const scaleChanged = (key) => async (e) => {
    const v = Number(e.target.value);
    settings.ui_values = { ...(settings.ui_values || {}), [key]: v };
    await invoke("set_ui_value", { key, value: v }).catch((err) => toast(String(err)));
    applyValues();
  };
  $("ui-scale").addEventListener("change", scaleChanged("uiScale"));
  $("icon-scale").addEventListener("change", scaleChanged("iconScale"));
  attachSelectPicker($("ui-scale"));
  attachSelectPicker($("icon-scale"));

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
    if (!toolbarMenu.classList.contains("hidden") && !toolbarMenu.contains(e.target)) closeToolbarMenu();
  });
  // Right-click anywhere in the header actions → toggle which top-bar tools show.
  document.querySelector(".header-actions")?.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    openToolbarMenu(e.clientX, e.clientY);
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (!toolbarMenu.classList.contains("hidden")) { closeToolbarMenu(); return; }
    if (!$("lightbox").classList.contains("hidden")) closeLightbox();
    else if (confirmCleanup) confirmCleanup(false);
    // The variable-fill dialog has its own keydown handler; consume Escape here so the
    // chain doesn't ALSO close the library/journal underneath it.
    else if (varsCleanup) { /* vars dialog closes itself */ }
    else if (!colorPop.classList.contains("hidden")) closeColorPop();
    else if (!ctxEl.classList.contains("hidden")) closeCtx();
    else if (!$("update-modal").classList.contains("hidden")) $("update-modal").classList.add("hidden");
    else if (!$("snip-modal").classList.contains("hidden")) $("snip-modal").classList.add("hidden");
    else if (!viewModal.root.classList.contains("hidden")) closeViewModal();
    else if (!modal.root.classList.contains("hidden")) confirmDiscardIfDirty().then((ok) => { if (ok) closeModal(); });
    else if (!$("clip-modal").classList.contains("hidden")) $("clip-modal").classList.add("hidden");
    else if (!$("dupes-modal").classList.contains("hidden")) $("dupes-modal").classList.add("hidden");
    else if (!$("backup-modal").classList.contains("hidden")) closeBackups();
    else if (!journalEl.classList.contains("hidden")) journalEl.classList.add("hidden");
    else if (!libraryEl.classList.contains("hidden")) hideLibrary();
    else if (!settingsEl.classList.contains("hidden")) hideSettings();
  });

  // Updates: manual check in the settings + daily background notification.
  let updateInfo = null;
  const updateBtn = $("update-btn");
  const updateVersion = $("update-version");
  // The status line (left of the button) is the version by default, and switches to
  // "Update to X available" once an update is found (manual OR auto). The button text
  // never changes — it always offers a fresh manual check.
  const offerUpdate = (info) => {
    updateInfo = info;
    updateVersion.textContent = t("updateAvailableBtn").replace("{v}", info.version);
    updateVersion.classList.add("update-avail");
  };
  // Clicking the status line: open the update dialog if one is pending, else the
  // GitHub releases page.
  updateVersion.addEventListener("click", () => {
    if (updateInfo?.available) openUpdateModal(updateInfo);
    else invoke("open_repo", { page: "releases" }).catch((e) => toast(String(e)));
  });

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
    updateBtn.disabled = true;
    try {
      const info = await invoke("check_update");
      if (info.available) offerUpdate(info);          // status → "Update to X available"
      else { updateVersion.textContent = t("upToDate"); updateVersion.classList.remove("update-avail"); }
    } catch (err) {
      toast(String(err));
    }
    updateBtn.disabled = false;
  });
  // Silently check for updates at launch and surface any in the status line. Only
  // when update checking is enabled at all — the normal setting is the master switch.
  if (flag("checkUpdateOnStart") && settings.auto_update !== false) {
    invoke("check_update").then((info) => { if (info?.available) offerUpdate(info); }).catch(() => {});
  }
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
    updateVersion.textContent = versionLabel;
    updateVersion.classList.remove("update-avail");
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
    const icon =
      '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 16V4"/><path d="M7 9l5-5 5 5"/><path d="M5 20h14"/></svg>';
    toast(t("updateAvailable").replace("{v}", e.payload.version), {
      label: t("installNow"),
      variant: "update",
      icon,
      onClick: () => openUpdateModal(e.payload),
    });
  });

  listen("theme-changed", (e) => applyTheme(e.payload));
  // "Edit prompt" chosen in a floating pill's right-click menu.
  listen("edit-prompt", (e) => editPrompt(String(e.payload)));
}

// Fill a UI/icon size select with percentage presets (plus the current value if it
// was fine-tuned in the expert menu), then select the current value.
const SCALE_PRESETS = [50, 75, 90, 100, 110, 125, 150];
function fillScaleSelect(id, key) {
  const sel = $(id);
  if (!sel) return;
  const cur = Math.round(val(key));
  sel.innerHTML = "";
  for (const p of [...new Set([...SCALE_PRESETS, cur])].sort((a, b) => a - b)) {
    const o = document.createElement("option");
    o.value = String(p);
    o.textContent = `${p} %`;
    sel.appendChild(o);
  }
  sel.value = String(cur);
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
  try {
    settings = await invoke("get_settings");
    loadPalette();
    LANG = resolveLang(settings.language);
    applyI18n();
    fillSizeSelects();
    fillFontSelects();
    applyTheme(await invoke("current_theme"));
    bind();
    wireTooltips();
    wireWindowControls();
    wireClearButtons();
    wireLightbox();
    setupDragDrop();
    applyFlags();
    applyBars();
    await renderGrid();
    syncSettingsControls();
    invoke("app_version").then((v) => {
      versionLabel = t("versionInstalled").replace("{v}", v);
      $("update-version").textContent = versionLabel;
    }).catch(() => {});
    invoke("current_data_dir").then((d) => { currentDataDir = d || ""; }).catch(() => {});
    invoke("default_screenshot_dir").then((d) => { defaultScreenshotDir = d || ""; }).catch(() => {});
    applyTileStyle();
    autoGrow(inputEl);
    inputEl.focus();
    pollMissingFiles();
    setInterval(pollMissingFiles, FILE_POLL_MS);
  } catch (e) {
    // Never strand the user with an invisible app: log, surface a toast if the
    // UI got far enough to have one, and still reveal the window below.
    console.error(e);
    try { toast(String((e && e.message) || e)); } catch (_) {}
  } finally {
    // Reveal only after the first fitted paint — the user never sees text sizing.
    requestAnimationFrame(() => {
      try { fitCache.clear(); fitAllTiles(); } catch (_) {}
      requestAnimationFrame(() => {
        // Ask about adopting an installed store only once the themed window is up.
        invoke("show_main_window").catch(() => {}).finally(() => offerTakeover());
      });
    });
  }
}

// Push the loaded settings into the settings-dialog controls.
function syncSettingsControls() {
    $("theme-select").value = settings.theme;
    $("lang-select").value = settings.language || "auto";
    $("opt-minimize").checked = settings.minimize_to_tray === true;
    $("opt-screenshot-folder").checked = settings.ui_flags?.screenshotSave === true;
    $("opt-tooltips").checked = settings.ui_flags?.tooltipsEnabled !== false;
    $("opt-autopaste").checked = settings.ui_flags?.autoPaste !== false;
    $("opt-autostart").checked = settings.autostart === true;
    $("opt-startmin").checked = settings.start_minimized === true;
    $("opt-ontop").checked = settings.always_on_top === true;
    $("opt-favview").checked = favViewEnabled();
    refreshFavViewUi();
    $("opt-hotkey").value = prettyAccel(settings.hotkey);
    $("pin-top").classList.toggle("active", settings.always_on_top === true);
    $("pin-top").setAttribute("aria-pressed", String(settings.always_on_top === true));
    $("opt-autoupdate").checked = settings.auto_update !== false;
    $("tile-font").value = settings.tile_font || "system";
    $("tile-size").value = String(normSize(Number(settings.tile_size ?? 0)));
    applyTileStyle();
}

// Re-read the store and re-apply everything a full import replaced (theme, language,
// flags, views, prompts, controls). Reloading the page would do the same but repaints
// the whole window, which reads as a flicker.
async function reloadEverything() {
  settings = await invoke("get_settings");
  loadPalette();
  LANG = resolveLang(settings.language);
  applyI18n();
  fillSizeSelects();
  fillFontSelects();
  applyTheme(await invoke("current_theme").catch(() => settings.theme));
  applyFlags();
  applyBars();
  await renderGrid(); // WITH the fetch: the prompts themselves were replaced too
  syncSettingsControls();
  wireClearButtons();
  fitCache.clear();
  fitAllTiles();
}

init();
