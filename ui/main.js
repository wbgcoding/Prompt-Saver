// Main-window controller. Uses Tauri's global API (withGlobalTauri).
import { I18N, LANGS } from "./i18n.js";
import { FONTS, FONT_LABELS } from "./fonts.js";
import { IMAGE_EXT, mediaKind, buildMediaBar, applyVideoPrefs } from "./media.js";

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
};

const DRAG_THRESHOLD = 5;
const INPUT_MAX = 160; // keep in sync with .input max-height
const MAX_VIEWS = 20;
const GRID_MAX = 20; // keep in sync with backend GRID_MAX
const PREVIEW_MAX = 220; // tooltip preview length of the prompt text
const SIZE_MIN = 10; // text size range, steps of 2 (keep in sync with backend)
const SIZE_MAX = 40;
const SIZE_STEP = 2;
const FILE_POLL_MS = 5000; // missing-file watcher interval

const clampGrid = (n, fallback) =>
  Math.min(GRID_MAX, Math.max(1, Math.round(Number(n) || fallback)));

// Cached state (refreshed by renderGrid).
let prompts = [];
let settings = { theme: "system", views: [], active_view: "" };

let modalState = null;
let ctxId = null;
let drag = null;
let toastTimer = null;
let deleteAllTimer = null;
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
// Corner badges marking what a tile copies (attached file / image).
const ICON_FILE =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M21.2 11.2l-8.4 8.4a5.5 5.5 0 0 1-7.8-7.8l8.4-8.4a3.7 3.7 0 0 1 5.2 5.2l-8.4 8.4a1.9 1.9 0 0 1-2.6-2.6l7.7-7.7"/></svg>';
const ICON_IMAGE =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="currentColor" d="M21 3H3a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h18a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Zm0 16H3V5h18v14Zm-5.5-9a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0ZM8.5 14l2-2.5 2 2.5 2.5-3 3.5 5H5l3.5-4.5Z"/></svg>';
const ICON_VIDEO =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="currentColor" d="M15 8.5V6a1 1 0 0 0-1-1H3a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h11a1 1 0 0 0 1-1v-2.5l5 4v-15l-5 4ZM6.5 9.8 12 13l-5.5 3.2V9.8Z"/></svg>';

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
const COLORS = [
  "", "#ef4444", "#f97316", "#f59e0b", "#eab308", "#22c55e", "#14b8a6",
  "#06b6d4", "#3b82f6", "#6366f1", "#8b5cf6", "#ec4899", "#64748b",
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
    `|${gridEl.clientWidth}x${gridEl.clientHeight}`;
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
  toastTimer = setTimeout(hideToast, action ? 12000 : 1400);
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
  for (let i = 1; i <= GRID_MAX; i++) {
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

// Header view-switcher buttons (hidden while only one view exists).
function renderViews() {
  viewsEl.classList.toggle("hidden", settings.views.length <= 1);
  viewsEl.innerHTML = "";
  for (const v of settings.views) {
    const btn = document.createElement("button");
    btn.className = "view-btn" + (v.id === settings.active_view ? " active" : "");
    btn.textContent = v.name;
    btn.title = v.name;
    btn.addEventListener("click", async () => {
      settings = await invoke("set_active_view", { id: v.id });
      await renderGrid(true); // prompts unchanged, settings already fresh
    });
    viewsEl.appendChild(btn);
  }
}

function buildTile(p) {
  const tile = document.createElement("div");
  tile.className = "tile";
  tile.dataset.id = p.id;
  const raw = p.file_path || p.text;
  const preview = raw.length > PREVIEW_MAX ? `${raw.slice(0, PREVIEW_MAX)}…` : raw;
  tile.title = preview
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
    if (p.caption) {
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
    ? (kind === "video" ? ICON_VIDEO : kind ? ICON_IMAGE : ICON_FILE)
    : p.copy_image ? ICON_IMAGE : "";
  if (typeIcon) {
    const badge = document.createElement("span");
    badge.className = "tile-type";
    badge.innerHTML = typeIcon;
    tile.appendChild(badge);
  }

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

  tile.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || e.target.closest(".tile-menu")) return;
    drag = { id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: tile };
  });
  tile.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    openCtx(p.id, e.clientX, e.clientY);
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
  video.autoplay = true;
  video.playsInline = true;
  wrap.append(video, buildMediaBar(video, {
    onChange: (prefs) => invoke("set_video_prefs", { id: p.id, ...prefs }).catch(() => {}),
  }));
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
    if (el.classList.contains("tile") &&
        (await invoke("copy_prompt", { id }).catch((e) => { toast(String(e)); return false; }))) {
      showCopied(el);
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

// Flash + small "Copied!" bubble at the bottom of the tile.
function showCopied(tile) {
  tile.classList.add("copied");
  setTimeout(() => tile.classList.remove("copied"), 350);
  const pop = document.createElement("div");
  pop.className = "copy-pop";
  pop.textContent = t("copied");
  tile.appendChild(pop);
  setTimeout(() => pop.remove(), 950);
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
function renderSwatches(selected) {
  const row = $("color-row");
  row.innerHTML = "";
  const isCustom = !!selected && !COLORS.includes(selected);

  const mkSwatch = (cls, bg) => {
    const sw = document.createElement("button");
    sw.type = "button";
    sw.className = `swatch ${cls}`.trim();
    if (bg) sw.style.background = bg;
    row.appendChild(sw);
    return sw;
  };

  // "No color" first, then the free color picker, then the palette.
  const none = mkSwatch("none" + (selected === "" ? " sel" : ""));
  none.addEventListener("click", () => {
    modalState.color = "";
    renderSwatches("");
  });

  const custom = mkSwatch("custom" + (isCustom ? " sel" : ""), isCustom ? selected : "");
  custom.addEventListener("click", () => {
    openColorPop(custom, isCustom ? selected : "", (hex) => {
      if (!modalState) return;
      modalState.color = hex;
      renderSwatches(hex);
    });
  });

  for (const c of COLORS.slice(1)) {
    const sw = mkSwatch(c === selected ? "sel" : "", c);
    sw.addEventListener("click", () => {
      modalState.color = c;
      renderSwatches(c);
    });
  }
}

function openModal({ mode, id, name = "", text = "", color = "", image = "", showImage = false, copyImage = false, filePath = "", iconPath = "", caption = "", captionSize = 0, font = "", fontSize = 0, title }) {
  modalState = { mode, id, color, image, showImage, copyImage, filePath, iconPath };
  modal.title.textContent = title;
  modal.name.value = name;
  modal.text.value = text;
  modal.caption.value = caption;
  modal.captionSize.value = String(normSize(captionSize) || 0);
  // Per-tile style overrides, available when creating and editing.
  modal.fontSel.value = font;
  modal.sizeSel.value = String(normSize(fontSize));
  syncModalImageUi(mode);
  modal.delete.classList.toggle("hidden", mode !== "edit");
  disarmButton(modal.delete, t("delete"));
  renderSwatches(color);
  modal.root.classList.remove("hidden");
  modal.name.focus();
  modal.name.select();
}

// Keep all image/file modal controls consistent with modalState.
function syncModalImageUi(mode) {
  const { image, showImage, copyImage, filePath, iconPath } = modalState;
  const kind = filePath ? mediaKind(filePath) : "";
  const iconKind = !image && iconPath ? mediaKind(iconPath) : "";
  const fileMedia = !image && !iconKind && (kind === "gif" || kind === "video");
  const hasPreview = !!image || !!iconKind || fileMedia;
  // Image and file prompts have no text field — the name doubles as the copy text.
  modal.text.classList.toggle("hidden", mode !== "edit" || copyImage || !!filePath);
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
    } else {
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

async function loadFilePreview(path) {
  if (!IMAGE_EXT.test(path)) return "";
  return (await invoke("load_image_file", { path }).catch(() => "")) || "";
}

// Paperclip flow: clipboard file first, then a copied image (screenshots),
// then the file dialog. Media files switch the tile to the media layout.
async function startFileCreate() {
  await withDialog(async () => {
    let path = await invoke("get_clipboard_file_path");
    if (!path) {
      const clipImg = await invoke("get_clipboard_image");
      if (clipImg) {
        openModal({ mode: "image-create", title: t("imageModalTitle"), image: clipImg, showImage: true, copyImage: true });
        return;
      }
      path = await invoke("pick_file_path");
    }
    if (!path) return;
    const kind = mediaKind(path);
    const image = await loadFilePreview(path);
    openModal({
      mode: "file-create",
      // Image, gif and video attachments behave like media prompts.
      title: t(kind === "video" ? "videoModalTitle" : kind ? "imageModalTitle" : "fileModalTitle"),
      name: path.split(/[\\/]/).pop(),
      filePath: path,
      image,
      showImage: !!kind,
    });
  });
}
function closeModal() {
  closeColorPop();
  modal.root.classList.add("hidden");
  modal.video.removeAttribute("src"); // stop a playing preview
  modalState = null;
}

function startCreate() {
  if (!inputEl.value.trim()) return;
  openModal({ mode: "create", title: t("nameModalTitle") });
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
      const text = inputEl.value.trim();
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
function renderLibrary() {
  const list = $("library-list");
  list.innerHTML = "";
  if (!prompts.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("libraryEmpty");
    list.appendChild(empty);
    return;
  }
  const placed = new Set(Object.keys(layoutOf(activeView())));
  for (const p of prompts) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "lib-item";
    row.title = t("edit");

    const body = document.createElement("span");
    body.className = "lib-body";
    const name = document.createElement("span");
    name.className = "lib-name";
    name.textContent = p.name;
    const text = document.createElement("span");
    text.className = "lib-text";
    text.textContent = p.file_path || p.text;
    body.append(name, text);

    // Image prompts get a thumbnail, text prompts the color dot.
    if (p.show_image && p.image) {
      const thumb = document.createElement("img");
      thumb.className = "lib-thumb";
      thumb.src = p.image;
      thumb.draggable = false;
      row.append(thumb, body);
    } else {
      const dot = document.createElement("span");
      dot.className = "dot";
      if (p.color) dot.style.background = p.color;
      row.append(dot, body);
    }

    // Place on the current layout: drag the row onto the grid, or one click.
    row.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || e.target.closest(".lib-add")) return;
      drag = { id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: row, fromLibrary: true };
    });
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

    row.addEventListener("click", () => editPrompt(p.id));
    list.appendChild(row);
  }
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
      n.max = GRID_MAX;
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
  $("view-add").classList.toggle("hidden", settings.views.length >= MAX_VIEWS);
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
  modal.root.addEventListener("pointerdown", (e) => { if (e.target === modal.root) closeModal(); });

  $("gear").addEventListener("click", () => {
    renderViewsEditor();
    settingsEl.classList.remove("hidden");
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

  $("library-btn").addEventListener("click", () => {
    renderLibrary();
    libraryEl.classList.remove("hidden");
  });
  $("library-close").addEventListener("click", () => libraryEl.classList.add("hidden"));
  libraryEl.addEventListener("pointerdown", (e) => {
    if (e.target === libraryEl) libraryEl.classList.add("hidden");
  });
  $("settings-close").addEventListener("click", () => settingsEl.classList.add("hidden"));
  // pointerdown (not click): selecting text that ends outside an input must not close.
  settingsEl.addEventListener("pointerdown", (e) => { if (e.target === settingsEl) settingsEl.classList.add("hidden"); });

  $("view-add").addEventListener("click", async () => {
    try {
      settings = await invoke("add_view", { name: "" });
      renderViewsEditor();
      await renderGrid(true);
    } catch (err) {
      toast(String(err));
    }
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
    if (!colorPop.classList.contains("hidden")) closeColorPop();
    else if (!ctxEl.classList.contains("hidden")) closeCtx();
    else if (!modal.root.classList.contains("hidden")) closeModal();
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
  updateBtn.addEventListener("click", async () => {
    updateBtn.disabled = true;
    try {
      if (updateInfo?.available) {
        await invoke("install_update", { url: updateInfo.url }); // app exits
        return;
      }
      const info = await invoke("check_update");
      if (info.available) offerUpdate(info);
      else flashStatus(t("upToDate"));
    } catch (err) {
      flashStatus(t("updateFailed"));
      toast(String(err));
    }
    updateBtn.disabled = false;
  });
  $("opt-autoupdate").addEventListener("change", (e) => {
    invoke("set_auto_update", { enabled: e.target.checked }).catch((err) => toast(String(err)));
  });
  listen("update-available", (e) => {
    offerUpdate(e.payload);
    toast(t("updateAvailable").replace("{v}", e.payload.version), {
      label: t("installNow"),
      onClick: () =>
        invoke("install_update", { url: e.payload.url }).catch((err) =>
          toast(String(err))
        ),
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

// ---- Init ----
async function init() {
  settings = await invoke("get_settings");
  LANG = resolveLang(settings.language);
  applyI18n();
  fillSizeSelects();
  fillFontSelects();
  applyTheme(await invoke("current_theme"));
  bind();
  applyBars();
  await renderGrid();
  $("theme-select").value = settings.theme;
  $("lang-select").value = settings.language || "auto";
  $("opt-minimize").checked = settings.minimize_to_tray === true;
  $("opt-autostart").checked = settings.autostart === true;
  $("opt-startmin").checked = settings.start_minimized === true;
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
