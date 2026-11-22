// Media manager window: the app's own recordings or screenshots as a grid of tiles with
// posters, sort and filters, a multi-selection and a "recently discarded" section.
//
// The folder is never named here. Every command takes the kind and bare file names, and the
// backend resolves the folder from the settings and checks each name.
import { copyTextChecked, fmtBytes, fmtNum, orderedInvoke, resolveLang } from "./floatShared.js";
import { applyFloatTheme } from "./floattheme.js";
import { APP_NAME } from "./brand.js";
import { errMessageLogged } from "./errors.js";
import { LANGS, fullTexts, loadLang } from "./i18n.js";
import { insertWinButtons, wireWinButtons } from "./winButtons.js";
import { attachSelectPicker } from "./selectpop.js";
import { placePopoverAt } from "./popover.js";
import { confirmDialog, initDialogs } from "./dialog.js";
import { createPreview } from "./managerPreview.js";

// Frameless window: the title shows in the Alt-Tab switcher, so it comes from the brand.
document.title = APP_NAME;

const { convertFileSrc } = window.__TAURI__.core;
const invoke = orderedInvoke;
const { listen } = window.__TAURI__.event;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;
const appWin = getCurrentWebviewWindow();
const $ = (id) => document.getElementById(id);

loadLang("en");

// ---------------------------------------------------------------- constants

const KINDS = ["recordings", "screenshots"];
const MB = 1024 * 1024;
const DAY_MS = 24 * 60 * 60 * 1000;
const PROBE_CHUNK = 24;
const REFOCUS_RELIST_MS = 2000;
const UNDO_TOAST_FALLBACK_MS = 8000;
const TOAST_PLAIN_MS = 3500;
// Formats a webview can show itself when ffmpeg is missing for a poster.
const VIDEO_TAG_EXTS = new Set(["mp4", "webm"]);
const IMG_TAG_EXTS = new Set(["gif", "webp", "apng", "png", "jpg", "jpeg", "bmp", "tiff"]);

const SORTS_COMMON = ["newest", "oldest", "largest", "smallest", "name"];
const SORTS_VIDEO = ["newest", "oldest", "largest", "smallest", "longest", "shortest", "name"];
const SORT_KEYS = {
  newest: "mmSortNewest", oldest: "mmSortOldest", largest: "mmSortLargest", smallest: "mmSortSmallest",
  longest: "mmSortLongest", shortest: "mmSortShortest", name: "mmSortName",
};
const AGE_KEYS = { today: "mmAgeToday", week: "mmAgeWeek", month: "mmAgeMonth", older30: "mmAgeOlder30", older90: "mmAgeOlder90" };
const SIZE_KEYS = { small: "mmSizeSmall", mid: "mmSizeMid", large: "mmSizeLarge" };
const DUR_KEYS = { short: "mmDurShort", mid: "mmDurMid", long: "mmDurLong" };

const THEME_VARS = ["bg", "surface", "surface-2", "fg", "muted", "border", "accent", "accent-fg", "danger"];
const HEX_RE = /^#[0-9a-fA-F]{6}$/;

// The three constants below are fixed drawings written here; they are the only markup this page
// ever sets as HTML. File names, app names and every other text go in through textContent.
const FILM_SVG = '<svg viewBox="0 0 24 24" width="34" height="34" aria-hidden="true"><rect x="3" y="5" width="18" height="14" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"/><path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M10 9.5v5l4.2-2.5Z"/></svg>';
const IMAGE_SVG = '<svg viewBox="0 0 24 24" width="34" height="34" aria-hidden="true"><rect x="3" y="5" width="18" height="14" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"/><circle cx="9" cy="10" r="1.6" fill="none" stroke="currentColor" stroke-width="1.6"/><path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="m4 17 5-4.5 3.5 3L15 13l5 4.5"/></svg>';
const TOAST_ICON = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>';

// ---------------------------------------------------------------- i18n

let TX = null;
let LANG = "en";
let textsSeq = 0;

function t(key) {
  return TX ? TX(key) : key;
}
const fill = (key, vars) => Object.entries(vars).reduce((s, [k, v]) => s.replace(`{${k}}`, () => String(v)), t(key));

async function loadTexts(settings) {
  const lang = resolveLang(settings?.language, LANGS);
  const seq = ++textsSeq;
  const { tx } = await fullTexts(lang);
  if (seq !== textsSeq) return;
  LANG = lang;
  TX = tx;
  applyI18n();
}

function applyI18n() {
  if (!TX) return;
  document.documentElement.lang = LANG;
  document.querySelectorAll("[data-i18n]").forEach((el) => { el.textContent = t(el.dataset.i18n); });
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => { el.placeholder = t(el.dataset.i18nPh); });
  document.querySelectorAll("[data-i18n-title]").forEach((el) => { el.title = t(el.dataset.i18nTitle); });
  document.querySelectorAll("[data-i18n-aria]").forEach((el) => { el.setAttribute("aria-label", t(el.dataset.i18nAria)); });
  if (syncWinButtons) syncWinButtons();
  buildFilterOptions();
  renderAll();
  if (preview) preview.sync();
}

// ---------------------------------------------------------------- theme

function applyTheme(theme, vars) {
  applyFloatTheme(theme, vars);
  const style = document.documentElement.style;
  for (const key of THEME_VARS) style.removeProperty(`--${key}`);
  for (const [key, value] of Object.entries(vars || {})) {
    if (THEME_VARS.includes(key) && HEX_RE.test(value)) style.setProperty(`--${key}`, value);
  }
}

// ---------------------------------------------------------------- state

let settings = null;
let kind = "recordings";
let files = [];                 // MediaFile rows of the open kind, as the backend listed them
let view = [];                  // the filtered, sorted subset the grid shows
const probes = new Map();       // "kind|name|mtime|size" -> { duration_ms, width, height }
const posterUrls = new Map();   // "kind|name|mtime" -> asset URL, or "" when none could be made
let selected = new Set();
let focusName = "";
let anchorName = "";            // the shift-click range starts here
let epoch = 0;                  // bumped on every kind switch and relist: late answers are dropped
let lastListAt = 0;
let discarded = [];
let discOpen = false;
let ctxNames = [];
let syncWinButtons = null;
let toastTimer = 0;
let noPosterNote = false;

const DEFAULT_VIEW = () => ({ sort: "newest", format: "", age: "", size: "", dur: "" });
let viewPrefs = DEFAULT_VIEW();
let query = "";

const viewKey = (k) => `mm-view-${k}`;
function loadViewPrefs(k) {
  let saved = {};
  try { saved = JSON.parse(localStorage.getItem(viewKey(k)) || "{}") || {}; } catch (_) { saved = {}; }
  const v = { ...DEFAULT_VIEW() };
  for (const key of Object.keys(v)) if (typeof saved[key] === "string") v[key] = saved[key];
  if (!sortsFor(k).includes(v.sort)) v.sort = "newest";
  return v;
}
function saveViewPrefs() {
  try { localStorage.setItem(viewKey(kind), JSON.stringify(viewPrefs)); } catch (_) {}
}
const sortsFor = (k) => (k === "recordings" ? SORTS_VIDEO : SORTS_COMMON);

const fileKey = (f) => `${kind}|${f.name}|${f.modified_ms}|${f.size}`;
const posterKey = (f) => `${kind}|${f.name}|${f.modified_ms}`;
const probeOf = (f) => probes.get(fileKey(f));
const byName = (name) => files.find((f) => f.name === name);

// ---------------------------------------------------------------- formatting

function fmtDate(ms) {
  try {
    return new Intl.DateTimeFormat(LANG, { dateStyle: "medium", timeStyle: "short" }).format(new Date(ms));
  } catch (_) {
    return new Date(ms).toLocaleString();
  }
}
function fmtDuration(ms) {
  const total = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const two = (n) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${m}:${two(s)}`;
}
function totalSize(list) {
  return list.reduce((sum, f) => sum + f.size, 0);
}
// Line 1 of a tile: date and time. Line 2: size, plus the resolution once the probe has it.
function metaLines(f) {
  const detail = [fmtBytes(f.size, LANG)];
  const p = probeOf(f);
  if (p && p.width > 0 && p.height > 0) detail.push(`${p.width}×${p.height}`);
  return [fmtDate(f.modified_ms), detail.join(" · ")];
}
function metaText(f) {
  return metaLines(f).join(" · ");
}
// Rewrites a tile's meta element as two single-line rows, each with its full text as tooltip.
function paintMeta(el, f) {
  const lines = metaLines(f);
  while (el.children.length < lines.length) {
    const row = document.createElement("div");
    row.className = "mm-meta-line";
    el.appendChild(row);
  }
  lines.forEach((text, i) => {
    el.children[i].textContent = text;
    el.children[i].title = text;
  });
}

// ---------------------------------------------------------------- toast

function toast(msg, action = null, duration = TOAST_PLAIN_MS) {
  const el = $("mm-toast");
  clearTimeout(toastTimer);
  el.textContent = "";
  el.classList.toggle("actionable", !!action);
  const ico = document.createElement("span");
  ico.className = "toast-ico";
  ico.innerHTML = TOAST_ICON;
  const label = document.createElement("span");
  label.className = "toast-msg";
  label.textContent = msg;
  el.append(ico, label);
  if (action) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "toast-btn";
    btn.textContent = action.label;
    btn.addEventListener("click", () => { hideToast(); action.onClick(); });
    el.appendChild(btn);
  }
  void el.offsetWidth;
  el.classList.add("show");
  toastTimer = setTimeout(hideToast, duration);
}
function hideToast() {
  clearTimeout(toastTimer);
  const el = $("mm-toast");
  el.classList.remove("show", "actionable");
}
const errToast = (err) => toast(errMessageLogged(err, t, "manager"));

initDialogs({ t, toast: (msg) => toast(msg) });

// ---------------------------------------------------------------- filter and sort

function buildFilterOptions() {
  const setOptions = (sel, entries, current) => {
    sel.textContent = "";
    for (const [value, label] of entries) {
      const o = document.createElement("option");
      o.value = value;
      o.textContent = label;
      sel.appendChild(o);
    }
    sel.value = entries.some(([v]) => v === current) ? current : entries[0][0];
  };
  const any = ["", t("mmFilterAny")];
  setOptions($("mm-sort"), sortsFor(kind).map((k) => [k, t(SORT_KEYS[k])]), viewPrefs.sort);
  const exts = [...new Set(files.map((f) => f.ext))].sort();
  setOptions($("mm-format"), [any, ...exts.map((e) => [e, e.toUpperCase()])], viewPrefs.format);
  setOptions($("mm-age"), [any, ...Object.entries(AGE_KEYS).map(([k, key]) => [k, t(key)])], viewPrefs.age);
  setOptions($("mm-size"), [any, ...Object.entries(SIZE_KEYS).map(([k, key]) => [k, t(key)])], viewPrefs.size);
  setOptions($("mm-dur"), [any, ...Object.entries(DUR_KEYS).map(([k, key]) => [k, t(key)])], viewPrefs.dur);
  $("mm-f-dur").classList.toggle("hidden", kind !== "recordings");
}

function startOfToday() {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

function matchesAge(f, age, now) {
  const age_ms = now - f.modified_ms;
  switch (age) {
    case "today": return f.modified_ms >= startOfToday();
    case "week": return age_ms <= 7 * DAY_MS;
    case "month": return age_ms <= 30 * DAY_MS;
    case "older30": return age_ms > 30 * DAY_MS;
    case "older90": return age_ms > 90 * DAY_MS;
    default: return true;
  }
}
function matchesSize(f, size) {
  switch (size) {
    case "small": return f.size < 10 * MB;
    case "mid": return f.size >= 10 * MB && f.size <= 100 * MB;
    case "large": return f.size > 100 * MB;
    default: return true;
  }
}
function matchesDuration(f, dur) {
  if (!dur) return true;
  const p = probeOf(f);
  if (!p || !(p.duration_ms > 0)) return false; // not probed yet, or no length known
  const s = p.duration_ms / 1000;
  if (dur === "short") return s < 60;
  if (dur === "mid") return s >= 60 && s <= 600;
  return s > 600;
}

function sortCompare(sort) {
  const dur = (f) => probeOf(f)?.duration_ms || 0;
  switch (sort) {
    case "oldest": return (a, b) => a.modified_ms - b.modified_ms || a.name.localeCompare(b.name);
    case "largest": return (a, b) => b.size - a.size || a.name.localeCompare(b.name);
    case "smallest": return (a, b) => a.size - b.size || a.name.localeCompare(b.name);
    case "longest": return (a, b) => dur(b) - dur(a) || b.modified_ms - a.modified_ms;
    case "shortest": return (a, b) => {
      // A file with no known length goes last, not first.
      const da = dur(a) || Infinity;
      const db = dur(b) || Infinity;
      return da === db ? b.modified_ms - a.modified_ms : da - db;
    };
    case "name": return (a, b) => a.name.localeCompare(b.name, LANG, { numeric: true, sensitivity: "base" });
    default: return (a, b) => b.modified_ms - a.modified_ms || a.name.localeCompare(b.name);
  }
}

function computeView() {
  const now = Date.now();
  const q = query.trim().toLowerCase();
  view = files
    .filter((f) => (!q || f.name.toLowerCase().includes(q))
      && (!viewPrefs.format || f.ext === viewPrefs.format)
      && matchesAge(f, viewPrefs.age, now)
      && matchesSize(f, viewPrefs.size)
      && (kind !== "recordings" || matchesDuration(f, viewPrefs.dur)))
    .sort(sortCompare(viewPrefs.sort));
}

// ---------------------------------------------------------------- rendering

const filtersActive = () => !!(query.trim() || viewPrefs.format || viewPrefs.age || viewPrefs.size || viewPrefs.dur);

function renderAll() {
  const titleKey = kind === "recordings" ? "mmTitleRecs" : "mmTitleShots";
  $("mm-title").textContent = t(titleKey);
  for (const b of document.querySelectorAll("#mm-kind button")) {
    const on = b.dataset.kind === kind;
    b.classList.toggle("active", on);
    b.setAttribute("aria-selected", String(on));
  }
  $("mm-f-format").classList.toggle("hidden", files.length === 0 && !viewPrefs.format);
  computeView();
  renderGrid();
  renderFooter();
  renderDiscarded();
  $("mm-note").classList.toggle("hidden", !(kind === "recordings" && noPosterNote));
}

function renderEmpty() {
  const empty = $("mm-empty");
  const shotsOff = kind === "screenshots" && files.length === 0 && settings?.ui_flags?.screenshotSave !== true;
  const none = view.length === 0;
  empty.classList.toggle("hidden", !none);
  $("mm-grid").classList.toggle("hidden", none);
  if (!none) return;
  if (shotsOff) $("mm-empty-text").textContent = t("mmShotsSaveOff");
  else $("mm-empty-text").textContent = t(files.length === 0 ? "mmEmpty" : "mmEmptyFilter");
  $("mm-empty-btn").classList.toggle("hidden", !shotsOff);
}

let posterObserver = null;
function ensureObserver() {
  if (posterObserver) posterObserver.disconnect();
  posterObserver = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      posterObserver.unobserve(e.target);
      const f = byName(e.target.dataset.name);
      if (f) requestPoster(f, e.target);
    }
  }, { root: $("mm-scroll"), rootMargin: "240px" });
}

function renderGrid() {
  const grid = $("mm-grid");
  grid.textContent = "";
  ensureObserver();
  renderEmpty();
  if (!view.some((f) => f.name === focusName)) focusName = view[0]?.name || "";
  const frag = document.createDocumentFragment();
  for (const f of view) frag.appendChild(buildTile(f));
  grid.appendChild(frag);
  for (const tile of grid.children) posterObserver.observe(tile);
}

function buildTile(f) {
  const tile = document.createElement("div");
  tile.className = "mm-tile";
  tile.dataset.name = f.name;
  tile.setAttribute("role", "option");
  tile.tabIndex = f.name === focusName ? 0 : -1;
  tile.setAttribute("aria-selected", String(selected.has(f.name)));
  tile.classList.toggle("selected", selected.has(f.name));

  const thumb = document.createElement("div");
  thumb.className = "mm-thumb";
  const cached = posterUrls.get(posterKey(f));
  if (cached) setThumbImage(thumb, cached);
  else if (cached === "") setThumbFallback(thumb, f);
  else thumb.innerHTML = kind === "recordings" ? FILM_SVG : IMAGE_SVG;
  thumb.classList.toggle("mm-thumb-empty", !cached);

  const dur = document.createElement("span");
  dur.className = "mm-dur hidden";
  thumb.appendChild(dur);

  const check = document.createElement("label");
  check.className = "mm-check";
  const box = document.createElement("input");
  box.type = "checkbox";
  box.tabIndex = -1;
  box.checked = selected.has(f.name);
  box.setAttribute("aria-label", f.name);
  check.appendChild(box);
  check.addEventListener("click", (e) => e.stopPropagation());
  box.addEventListener("change", () => { toggleSelected(f.name, box.checked); });
  thumb.appendChild(check);

  const name = document.createElement("div");
  name.className = "mm-name";
  name.textContent = f.name;
  name.title = f.name;
  const meta = document.createElement("div");
  meta.className = "mm-meta";
  paintMeta(meta, f);
  tile.append(thumb, name, meta);
  if (f.app) {
    const app = document.createElement("div");
    app.className = "mm-app";
    app.textContent = f.app;
    app.title = f.app;
    tile.appendChild(app);
  }
  paintDuration(tile, f);
  return tile;
}

function paintDuration(tile, f) {
  const p = probeOf(f);
  const badge = tile.querySelector(".mm-dur");
  if (!badge) return;
  const on = kind === "recordings" && p && p.duration_ms > 0;
  badge.classList.toggle("hidden", !on);
  if (on) badge.textContent = fmtDuration(p.duration_ms);
}

function tileOf(name) {
  for (const el of $("mm-grid").children) if (el.dataset.name === name) return el;
  return null;
}

function setThumbImage(thumb, url) {
  const old = thumb.querySelector(".mm-img");
  if (old) old.remove();
  thumb.querySelector("svg")?.remove();
  const img = document.createElement("img");
  img.className = "mm-img";
  img.alt = "";
  img.draggable = false;
  img.decoding = "async";
  img.src = url;
  img.addEventListener("error", () => { img.remove(); thumb.classList.add("mm-thumb-empty"); thumb.insertAdjacentHTML("afterbegin", kind === "recordings" ? FILM_SVG : IMAGE_SVG); });
  thumb.prepend(img);
  thumb.classList.remove("mm-thumb-empty");
}

// No poster could be made (no ffmpeg, or a format it cannot read): show what the webview
// can draw by itself, else a film glyph. Only the recording folder is in the asset scope; the
// screenshot folder is not (and stays out on purpose), so a screenshot whose poster failed gets
// the glyph instead of an asset URL that would be denied.
function setThumbFallback(thumb, f) {
  thumb.querySelector("svg")?.remove();
  const src = kind === "recordings" ? convertFileSrc(f.path) : "";
  if (kind === "recordings" && VIDEO_TAG_EXTS.has(f.ext)) {
    const v = document.createElement("video");
    v.className = "mm-img";
    v.muted = true;
    v.preload = "metadata";
    v.src = `${src}#t=0.1`;
    thumb.prepend(v);
    thumb.classList.remove("mm-thumb-empty");
  } else if (kind === "recordings" && IMG_TAG_EXTS.has(f.ext)) {
    const img = document.createElement("img");
    img.className = "mm-img";
    img.alt = "";
    img.loading = "lazy";
    img.draggable = false;
    img.src = src;
    thumb.prepend(img);
    thumb.classList.remove("mm-thumb-empty");
  } else {
    thumb.insertAdjacentHTML("afterbegin", kind === "recordings" ? FILM_SVG : IMAGE_SVG);
  }
}

async function requestPoster(f, tile) {
  const key = posterKey(f);
  const mine = epoch;
  let url = posterUrls.get(key);
  if (url === undefined) {
    let path = null;
    try { path = await invoke("media_poster", { kind, name: f.name }); } catch (_) { path = null; }
    if (mine !== epoch) return;
    url = path ? convertFileSrc(path) : "";
    posterUrls.set(key, url);
    if (!path && kind === "recordings" && !noPosterNote) {
      noPosterNote = true;
      $("mm-note").classList.remove("hidden");
    }
  }
  const el = tile.isConnected ? tile : tileOf(f.name);
  const thumb = el?.querySelector(".mm-thumb");
  if (!thumb) return;
  if (url) setThumbImage(thumb, url);
  else setThumbFallback(thumb, f);
}

function renderFooter() {
  const sel = files.filter((f) => selected.has(f.name));
  // The footer text is the selection while there is one, the folder total otherwise.
  $("mm-sel").textContent = sel.length
    ? fill("mmSelected", { n: sel.length, size: fmtBytes(totalSize(sel), LANG) })
    : fill("mmCount", { n: files.length, size: fmtBytes(totalSize(files), LANG) });
  $("mm-del").disabled = sel.length === 0;
  $("mm-export").disabled = sel.length === 0;
  $("mm-export").classList.toggle("hidden", kind !== "recordings");
  $("mm-all").disabled = view.length === 0 || view.every((f) => selected.has(f.name));
  $("mm-none").disabled = sel.length === 0;
  $("mm-grid").classList.toggle("has-selection", sel.length > 0);
}

// ---------------------------------------------------------------- selection and focus

function paintSelection() {
  for (const tile of $("mm-grid").children) {
    const on = selected.has(tile.dataset.name);
    tile.classList.toggle("selected", on);
    tile.setAttribute("aria-selected", String(on));
    const box = tile.querySelector("input");
    if (box) box.checked = on;
  }
  renderFooter();
}

function toggleSelected(name, on) {
  if (on) selected.add(name);
  else selected.delete(name);
  anchorName = name;
  paintSelection();
}

function selectRangeTo(name) {
  const names = view.map((f) => f.name);
  const a = names.indexOf(anchorName || focusName);
  const b = names.indexOf(name);
  if (b < 0) return;
  const [from, to] = a < 0 ? [b, b] : [Math.min(a, b), Math.max(a, b)];
  for (let i = from; i <= to; i++) selected.add(names[i]);
  paintSelection();
}

function clearSelection() {
  selected = new Set();
  paintSelection();
}

function setFocus(name, scroll = true) {
  focusName = name;
  for (const tile of $("mm-grid").children) tile.tabIndex = tile.dataset.name === name ? 0 : -1;
  const tile = tileOf(name);
  if (tile) {
    tile.focus({ preventScroll: !scroll });
    if (scroll) tile.scrollIntoView({ block: "nearest" });
  }
}

function gridColumns() {
  const tiles = [...$("mm-grid").children];
  if (!tiles.length) return 1;
  const top = tiles[0].offsetTop;
  const n = tiles.findIndex((el) => el.offsetTop !== top);
  return n < 0 ? tiles.length : n;
}

function moveFocus(dx, dy) {
  if (!view.length) return;
  const names = view.map((f) => f.name);
  const at = Math.max(0, names.indexOf(focusName));
  const cols = gridColumns();
  let next = at + dx + dy * cols;
  if (dy !== 0 && (next < 0 || next >= names.length)) next = dy < 0 ? at : at; // a row edge stays put
  next = Math.max(0, Math.min(names.length - 1, next));
  setFocus(names[next]);
}

// ---------------------------------------------------------------- loading

async function loadList({ keepSelection = false } = {}) {
  const mine = ++epoch;
  let list;
  try {
    list = await invoke("list_media", { kind });
  } catch (err) {
    if (mine === epoch) { files = []; renderAll(); errToast(err); }
    return;
  }
  if (mine !== epoch) return;
  files = list;
  lastListAt = Date.now();
  const names = new Set(files.map((f) => f.name));
  selected = keepSelection ? new Set([...selected].filter((n) => names.has(n))) : new Set();
  buildFilterOptions();
  renderAll();
  preview.sync();
  probeMissing(mine);
  loadDiscarded(mine);
}

async function probeMissing(mine) {
  const todo = files.filter((f) => !probes.has(fileKey(f))).map((f) => f.name);
  for (let i = 0; i < todo.length; i += PROBE_CHUNK) {
    if (mine !== epoch) return;
    const chunk = todo.slice(i, i + PROBE_CHUNK);
    let out;
    try { out = await invoke("media_probe", { kind, names: chunk }); } catch (_) { out = []; }
    if (mine !== epoch) return;
    for (const p of out) {
      const f = byName(p.name);
      if (f) probes.set(fileKey(f), p);
    }
    applyProbes(chunk);
  }
  // The sort and the length filter may depend on lengths that just arrived.
  if (mine === epoch && (viewPrefs.sort === "longest" || viewPrefs.sort === "shortest" || viewPrefs.dur)) renderAll();
}

function applyProbes(names) {
  for (const name of names) {
    const f = byName(name);
    const tile = f && tileOf(name);
    if (!tile) continue;
    paintMeta(tile.querySelector(".mm-meta"), f);
    paintDuration(tile, f);
  }
}

async function loadDiscarded(mine = epoch) {
  let list;
  try { list = await invoke("media_list_discarded", { kind }); } catch (_) { list = []; }
  if (mine !== epoch) return;
  discarded = list;
  renderDiscarded();
}

function setKind(next) {
  if (!KINDS.includes(next)) return;
  if (next === kind && files.length) return;
  kind = next;
  viewPrefs = loadViewPrefs(kind);
  query = "";
  $("mm-search").value = "";
  selected = new Set();
  focusName = "";
  noPosterNote = false;
  files = [];
  discarded = [];
  hideToast();
  closeCtx();
  preview.close();
  buildFilterOptions();
  renderAll();
  loadList();
}

// ---------------------------------------------------------------- discarded section

function renderDiscarded() {
  const box = $("mm-disc");
  box.classList.toggle("hidden", discarded.length === 0);
  if (!discarded.length) return;
  $("mm-disc-label").textContent = `${t("videoDiscardedLabel")} (${discarded.length})`;
  $("mm-disc-head").setAttribute("aria-expanded", String(discOpen));
  box.classList.toggle("open", discOpen);
  const list = $("mm-disc-list");
  list.classList.toggle("hidden", !discOpen);
  list.textContent = "";
  if (!discOpen) return;
  for (const d of discarded) {
    const li = document.createElement("li");
    const name = document.createElement("span");
    name.className = "mm-disc-name";
    name.textContent = d.name;
    name.title = d.name;
    const size = document.createElement("span");
    size.className = "mm-disc-size";
    size.textContent = fmtBytes(d.size, LANG);
    const restore = document.createElement("button");
    restore.type = "button";
    restore.className = "ghost-btn";
    restore.textContent = t("videoDiscardedRestore");
    restore.addEventListener("click", () => restoreNames([d.name], true));
    const del = document.createElement("button");
    del.type = "button";
    del.className = "ghost-btn mm-disc-del";
    del.textContent = t("videoDiscardedDeleteNow");
    del.addEventListener("click", () => deleteNow(d.name));
    li.append(name, size, restore, del);
    list.appendChild(li);
  }
}

async function restoreNames(names, announce) {
  let ok = 0;
  for (const name of names) {
    try { if (await invoke("media_restore", { kind, name })) ok++; } catch (_) { /* counted below */ }
  }
  await loadList({ keepSelection: true });
  if (ok < names.length) toast(t("mmRestoreFailed"));
  else if (announce) toast(t("mmRestored"));
}

async function deleteNow(name) {
  const sure = await confirmDialog({ title: t("videoDiscardedDeleteNow"), message: t("mmDeleteNowAsk"), confirmLabel: t("videoDiscardedDeleteNow") });
  if (!sure) return;
  let ok = false;
  try { ok = await invoke("media_delete_now", { kind, name }); } catch (_) { ok = false; }
  if (!ok) toast(t("mmDeleteFailed"));
  loadDiscarded();
}

// ---------------------------------------------------------------- actions

// Names a discard is moving right now: a double click or a held Delete key must not send them
// again, or the second call moves nothing and its failure notice replaces the undo notice.
const discarding = new Set();

async function discardNames(names) {
  names = names.filter((n) => byName(n) && !discarding.has(n));
  if (!names.length) return;
  const asked = kind;
  let moved;
  for (const n of names) discarding.add(n);
  try {
    try {
      [moved] = await invoke("media_discard", { kind, names });
    } catch (err) {
      errToast(err);
      return;
    }
    if (asked !== kind) return;
    for (const n of names) selected.delete(n);
    await loadList({ keepSelection: true });
  } finally {
    for (const n of names) discarding.delete(n);
  }
  if (moved < names.length) toast(t("mmDeleteFailed"));
  if (moved <= 0) return;
  const undoMs = Number(settings?.ui_values?.undoToastMs ?? UNDO_TOAST_FALLBACK_MS);
  const msg = fill("mmDeletedUndo", { n: moved });
  if (undoMs > 0) toast(msg, { label: t("undo"), onClick: () => restoreNames(names, false) }, undoMs);
  else toast(msg);
}

function currentTargets() {
  if (selected.size) return view.filter((f) => selected.has(f.name)).map((f) => f.name);
  return focusName ? [focusName] : [];
}

async function openInEditor(name) {
  try { await invoke("media_open_in_main", { kind, name }); } catch (err) { errToast(err); }
}
async function play(name) {
  try { await invoke("media_play", { kind, name }); } catch (err) { errToast(err); }
}
async function reveal(name) {
  try { await invoke("media_reveal", { kind, name }); } catch (err) { errToast(err); }
}
async function copyFile(name) {
  try {
    const ok = await invoke("media_copy_file", { kind, name });
    toast(ok ? t("copied") : t("mmCopyFailed"));
  } catch (err) { errToast(err); }
}
async function copyImage(name) {
  try { await invoke("media_copy_image", { name }); toast(t("copied")); } catch (err) { errToast(err); }
}
async function copyPath(name) {
  const f = byName(name);
  if (!f) return;
  try { await copyTextChecked(invoke, f.path); toast(t("copied")); } catch (err) { errToast(err); }
}
async function exportNames(names) {
  const list = names.filter((n) => byName(n));
  if (!list.length) return;
  try { await invoke("media_export_in_main", { names: list }); } catch (err) { errToast(err); }
}

// ---------------------------------------------------------------- large view

const preview = createPreview({
  $, t, fill, invoke, convertFileSrc,
  kind: () => kind,
  view: () => view,
  byName,
  metaText,
  posterUrl: (f) => posterUrls.get(posterKey(f)) || "",
  onClose: (name) => { if (byName(name)) setFocus(name); },
  actions: {
    edit: openInEditor,
    copy: (name) => (kind === "screenshots" ? copyImage(name) : copyFile(name)),
    player: play,
    exportOne: (name) => exportNames([name]),
    reveal,
    discard: (name) => discardNames([name]),
  },
});

// ---------------------------------------------------------------- context menu

function closeCtx() {
  $("mm-ctx").classList.add("hidden");
  ctxNames = [];
}

function openCtx(name, x, y) {
  const f = byName(name);
  if (!f) return;
  const multi = selected.has(name) && selected.size > 1;
  ctxNames = multi ? currentTargets() : [name];
  const menu = $("mm-ctx");
  menu.textContent = "";
  const add = (label, run, cls = "") => {
    const b = document.createElement("button");
    b.type = "button";
    b.setAttribute("role", "menuitem");
    if (cls) b.className = cls;
    b.textContent = label;
    b.addEventListener("click", () => { closeCtx(); run(); });
    menu.appendChild(b);
  };
  const sep = () => { const s = document.createElement("div"); s.className = "ctx-sep"; menu.appendChild(s); };
  if (!multi) {
    add(t("mmView"), () => preview.open(name));
    add(t("mmOpenEditor"), () => openInEditor(name));
    if (kind === "recordings") add(t("mmOpenPlayer"), () => play(name));
    add(t("mmShowInFolder"), () => reveal(name));
    if (kind === "screenshots") add(t("mmCopyImage"), () => copyImage(name));
    add(t("mmCopyFile"), () => copyFile(name));
    add(t("mmCopyPath"), () => copyPath(name));
  }
  if (kind === "recordings") add(t("videoCleanupExportSelected"), () => exportNames(ctxNames));
  sep();
  const doomed = ctxNames.slice();
  add(t("mmDelete"), () => discardNames(doomed), "danger");
  placePopoverAt(menu, x, y);
  menu.querySelector("button")?.focus();
}

$("mm-ctx").addEventListener("keydown", (e) => {
  const items = [...$("mm-ctx").querySelectorAll("button")];
  const at = items.indexOf(document.activeElement);
  if (e.key === "ArrowDown") { e.preventDefault(); items[(at + 1) % items.length]?.focus(); }
  else if (e.key === "ArrowUp") { e.preventDefault(); items[(at - 1 + items.length) % items.length]?.focus(); }
});
document.addEventListener("pointerdown", (e) => {
  if (!$("mm-ctx").classList.contains("hidden") && !$("mm-ctx").contains(e.target)) closeCtx();
});
window.addEventListener("blur", closeCtx);

// ---------------------------------------------------------------- events

const grid = $("mm-grid");

grid.addEventListener("click", (e) => {
  const tile = e.target.closest(".mm-tile");
  if (!tile) return;
  const name = tile.dataset.name;
  anchorName = anchorName || name;
  setFocus(name, false);
  if (e.shiftKey) { selectRangeTo(name); return; }
  if (e.ctrlKey || e.metaKey) { toggleSelected(name, !selected.has(name)); return; }
  // A click shows the file large in this window; editing is one button away there.
  preview.open(name);
});

grid.addEventListener("contextmenu", (e) => {
  const tile = e.target.closest(".mm-tile");
  e.preventDefault();
  if (!tile) return;
  const name = tile.dataset.name;
  if (!selected.has(name)) { selected = new Set(); paintSelection(); }
  setFocus(name, false);
  openCtx(name, e.clientX, e.clientY);
});

document.addEventListener("keydown", (e) => {
  const inField = e.target instanceof HTMLElement && e.target.closest("input[type=search], input[type=text], select, textarea");
  if (e.key === "Escape" && !$("mm-ctx").classList.contains("hidden")) { closeCtx(); e.preventDefault(); return; }
  if (preview.isOpen()) {
    if (document.querySelector(".confirm-overlay, .modal:not(.hidden)")) return;
    if (preview.onKey(e)) e.preventDefault();
    return;
  }
  if (e.key === "Escape") {
    if (inField && e.target.id === "mm-search" && e.target.value) {
      e.target.value = "";
      query = "";
      renderAll();
      e.preventDefault();
      return;
    }
    if (document.querySelector(".num-pop:not(.hidden), .confirm-overlay, .modal:not(.hidden)")) return;
    if (selected.size) { clearSelection(); e.preventDefault(); return; }
    appWin.close().catch(() => {});
    return;
  }
  if (inField) return;
  if (document.querySelector(".modal:not(.hidden), .confirm-overlay")) return;
  if (e.key === "ArrowRight") { e.preventDefault(); moveFocus(1, 0); }
  else if (e.key === "ArrowLeft") { e.preventDefault(); moveFocus(-1, 0); }
  else if (e.key === "ArrowDown") { e.preventDefault(); moveFocus(0, 1); }
  else if (e.key === "ArrowUp") { e.preventDefault(); moveFocus(0, -1); }
  else if (e.key === "Home") { e.preventDefault(); if (view[0]) setFocus(view[0].name); }
  else if (e.key === "End") { e.preventDefault(); if (view.length) setFocus(view[view.length - 1].name); }
  else if (e.key === "Enter") {
    if (e.target instanceof HTMLButtonElement && !e.target.classList.contains("mm-tile")) return;
    if (focusName) { e.preventDefault(); preview.open(focusName); }
  } else if (e.key === " ") {
    if (e.target instanceof HTMLButtonElement || !focusName) return;
    e.preventDefault();
    toggleSelected(focusName, !selected.has(focusName));
  } else if (e.key === "Delete") {
    if (e.target instanceof HTMLButtonElement && !e.target.classList.contains("mm-tile")) return;
    const names = currentTargets();
    if (names.length) { e.preventDefault(); discardNames(names); }
  } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
    e.preventDefault();
    selected = new Set(view.map((f) => f.name));
    paintSelection();
  }
});

$("mm-search").addEventListener("input", (e) => { query = e.target.value; renderAll(); });
function onViewChange() {
  viewPrefs = { sort: $("mm-sort").value, format: $("mm-format").value, age: $("mm-age").value, size: $("mm-size").value, dur: $("mm-dur").value };
  saveViewPrefs();
  renderAll();
}
for (const id of ["mm-sort", "mm-format", "mm-age", "mm-size", "mm-dur"]) {
  attachSelectPicker($(id));
  $(id).addEventListener("change", onViewChange);
}
$("mm-refresh").addEventListener("click", () => { posterUrls.clear(); probes.clear(); noPosterNote = false; loadList({ keepSelection: true }); });
$("mm-all").addEventListener("click", () => { selected = new Set(view.map((f) => f.name)); paintSelection(); });
$("mm-none").addEventListener("click", clearSelection);
$("mm-del").addEventListener("click", () => discardNames(currentTargets().filter((n) => selected.has(n))));
$("mm-export").addEventListener("click", () => exportNames([...selected]));
$("mm-close").addEventListener("click", () => { appWin.close().catch(() => {}); });
$("mm-disc-head").addEventListener("click", () => { discOpen = !discOpen; renderDiscarded(); });
$("mm-empty-btn").addEventListener("click", () => { invoke("media_open_setting", { key: "screenshotSave" }).catch(errToast); });
for (const b of document.querySelectorAll("#mm-kind button")) {
  b.addEventListener("click", () => {
    if (b.dataset.kind === kind) return;
    // Through the backend: it also drops the poster jobs queued for the grid being left.
    invoke("open_media_manager", { kind: b.dataset.kind }).catch(() => setKind(b.dataset.kind));
  });
}

syncWinButtons = wireWinButtons({ win: appWin, ...insertWinButtons($("mm-close"), "mm"), translate: t });

// A window that regains focus shows what changed in the folder meanwhile.
window.addEventListener("focus", () => {
  if (Date.now() - lastListAt > REFOCUS_RELIST_MS && TX) loadList({ keepSelection: true });
});

listen("manager-kind", (e) => setKind(String(e.payload)));
listen("theme-changed", (e) => applyTheme(e.payload?.theme, e.payload?.vars));
listen("language-changed", () => { invoke("get_settings").then((s) => { settings = s; return loadTexts(s); }).catch(() => {}); });
listen("settings-changed", () => { invoke("get_settings").then((s) => { settings = s; renderEmpty(); }).catch(() => {}); });

// ---------------------------------------------------------------- boot

(async () => {
  const initial = new URLSearchParams(location.search).get("kind");
  kind = KINDS.includes(initial) ? initial : "recordings";
  viewPrefs = loadViewPrefs(kind);
  try {
    const [theme, vars, s] = await Promise.all([invoke("current_theme"), invoke("theme_vars"), invoke("get_settings")]);
    settings = s;
    applyTheme(theme, vars);
    await loadTexts(s);
  } catch (err) {
    errToast(err);
  }
  buildFilterOptions();
  await loadList();
  setFocus(focusName, false);
})();
