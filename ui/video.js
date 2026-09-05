// Video editor window. One recording at a time: preview on top, a two-track
// timeline (filmstrip + waveform per audio track) below it, and a side panel
// holding the edit controls, the overlay list and the export settings.
//
// Nothing is ever applied to the file while editing. Every change is an entry in
// an ordered operation list (VE.ops), exactly like the screenshot editor's own
// list, and the export compiles that list into ONE ffmpeg filter graph. That is
// what makes undo/redo cheap, the preview honest and the export a single pass.
import { resolveLang, ffmpegDownloadPct } from "./floatShared.js";
import { applyFloatTheme } from "./floattheme.js";
import { APP_NAME } from "./brand.js";

// Frameless window: the title is invisible in the app but shows in the Alt-Tab
// switcher, so it comes from the brand rather than a hardcoded English word.
document.title = APP_NAME;

const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

const appWin = getCurrentWebviewWindow();
const $ = (id) => document.getElementById(id);

// ---------------------------------------------------------------- constants

// Progress while exporting arrives on the same event the recording converter
// already uses -- one pipeline, one event name.
const EXPORT_PROGRESS_EVENT = "recording-convert-progress";

const SPEED_STEPS = [0.25, 0.5, 1, 1.5, 2, 4];
const ZOOM_MIN_PX_PER_S = 2;      // whole clip visible, however long it is
const ZOOM_MAX_PX_PER_S = 600;    // ~1.7ms per pixel: far past frame accuracy
const MAX_TRACK_PX = 12000;       // canvas/DOM ceiling for the timeline strip
const THUMB_PX = 96;              // target width of one filmstrip thumbnail
const WAVE_BUCKET_MAX = 2000;
const UNDO_MAX_DEFAULT = 40;      // expert value videoEditUndoMax
const MIN_CLIP_MS = 100;          // shortest trim/cut range that still means something
const RECT_MIN_PX = 16;           // smallest draggable rectangle, in source pixels
// Colour choices offered for text and frame overlays. Same palette the rest of
// the app starts from; the editor deliberately offers no second colour-picker
// implementation of its own.
const OVERLAY_COLORS = ["#ffffff", "#000000", "#ef4444", "#f59e0b", "#22c55e", "#3b82f6", "#a855f7", "#ec4899"];
const FONT_FILES = [
  ["segoeui", "segoeui.ttf"],
  ["arial", "arial.ttf"],
  ["arialbd", "arialbd.ttf"],
  ["consola", "consola.ttf"],
  ["georgia", "georgia.ttf"],
];
// The live preview draws text with the browser's own font shaping
// (drawtext's fontfile is only reachable by ffmpeg at export time), so each
// id gets the closest real CSS family instead.
const FONT_CSS = {
  segoeui: '"Segoe UI", sans-serif', arial: "Arial, sans-serif", arialbd: "Arial, sans-serif",
  consola: "Consolas, monospace", georgia: "Georgia, serif",
};
// This was hardcoded per-entry as "C:/Windows/Fonts/..." -- broke every
// text-overlay export on a system where Windows itself lives on another
// drive. Resolved once from the backend (which reads SystemRoot/windir);
// this fallback only covers the sliver of time before that resolves.
let fontsDir = "C:/Windows/Fonts";
invoke("windows_fonts_dir").then((d) => { if (d) fontsDir = d; }).catch(() => {});
const EXPORT_KBPS = { high: 12000, small: 4000, lossless: null };

// ---------------------------------------------------------------- state

const VE = {
  src: "",
  probe: null,          // RecordingProbe, or a locally guessed one without ffmpeg
  ffmpeg: true,         // false once a call reported "ffmpeg-missing"
  dur: 0,               // source duration in ms
  fps: 30,
  w: 0,
  h: 0,
  ops: [],
  redo: [],
  floor: 0,             // undo may not walk back past this index
  undoMax: UNDO_MAX_DEFAULT,
  sel: null,            // { a, b } selection on the source timeline, in ms
  selOv: null,          // id of the overlay currently being edited
  zoom: 20,             // timeline pixels per second
  playhead: 0,          // ms on the source timeline
  drag: null,           // active pointer gesture on the preview or the timeline
  // The mic track starts muted in the editor/export (the recording
  // itself is untouched, still full) -- one click on ve-mute-mic mixes it in.
  audio: { sys: { mute: false, vol: 100 }, mic: { mute: true, vol: 100 } },
  hasSys: true,
  hasMic: false,
  loadGen: 0,           // bumped on every loadRecording() call; a stale call's
                         // own late-arriving results check this before applying
  strip: [],            // filmstrip thumbnail paths
  stripCount: 0,
  peaks: { 0: null, 1: null },
  exporting: false,
  dirty: false,         // ops changed since the last successful export
  lastExport: "",       // path of the file the last export produced
  destOverride: null,   // a full path picked via "Save as…" or typed by hand
  destDir: "",          // remembered folder from "Export to…", persisted across videos
  // Spatial zoom/pan of the preview picture itself, distinct from `zoom`
  // above (the timeline's pixels-per-second). 1 = fit-to-stage, the only level
  // that existed before; panPx is a plain CSS-pixel offset, independent of
  // zoom, applied on top of the video's own fit-to-stage scaling.
  previewZoom: 1,
  panPx: { x: 0, y: 0 },
};

let TXT_READY = false;
let I18N_DICT = null;
let LANG = "en";
let applyTermsFn = (s) => s;

// ---------------------------------------------------------------- i18n

// Same contract as everywhere else: {app} stands for the product name, and a
// missing key falls back to English, then to the key itself.
function t(key) {
  if (!I18N_DICT) return key;
  const raw = I18N_DICT[LANG]?.[key] ?? I18N_DICT.en?.[key] ?? key;
  return applyTermsFn(String(raw).replaceAll("{app}", APP_NAME), LANG);
}

function applyI18n() {
  if (!TXT_READY) return;
  document.documentElement.lang = LANG;
  document.querySelectorAll("[data-i18n]").forEach((el) => { el.textContent = t(el.dataset.i18n); });
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => { el.placeholder = t(el.dataset.i18nPh); });
  document.querySelectorAll("[data-i18n-title]").forEach((el) => { el.title = t(el.dataset.i18nTitle); });
  document.querySelectorAll("[data-i18n-aria]").forEach((el) => { el.setAttribute("aria-label", t(el.dataset.i18nAria)); });
  renderOps();
  renderOverlayList();
  renderOverlayEditor();
  renderSpeedSteps();
  updateCutCount();
  updateExportNote();
  updatePlayButton();
  updateAudioLabels();
}

async function loadTexts(settings) {
  const i18n = await import("./i18n.js");
  const terms = await import("./terms.js");
  applyTermsFn = terms.applyTerms;
  I18N_DICT = i18n.I18N;
  LANG = resolveLang(settings?.language, i18n.LANGS);
  TXT_READY = true;
  applyI18n();
}

// ---------------------------------------------------------------- theme

// The window uses main.css's token set, so a custom theme has to reach THOSE
// variables. applyFloatTheme sets the data-theme attribute every window needs;
// the token mapping below is the main-window half of the same job.
const THEME_VARS = ["bg", "surface", "surface-2", "fg", "muted", "border", "accent", "accent-fg", "danger"];
const HEX_RE = /^#[0-9a-fA-F]{6}$/;

function applyTheme(theme, vars) {
  applyFloatTheme(theme, vars);
  const style = document.documentElement.style;
  for (const key of THEME_VARS) style.removeProperty(`--${key}`);
  for (const [key, value] of Object.entries(vars || {})) {
    if (THEME_VARS.includes(key) && HEX_RE.test(value)) style.setProperty(`--${key}`, value);
  }
}

// ---------------------------------------------------------------- helpers

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
const even = (v) => Math.max(0, Math.round(v / 2) * 2);
const pad = (n, w) => String(n).padStart(w, "0");

function fmtTime(ms) {
  const v = Math.max(0, Math.round(ms));
  const m = Math.floor(v / 60000);
  const s = Math.floor((v % 60000) / 1000);
  return `${pad(m, 2)}:${pad(s, 2)}.${pad(v % 1000, 3)}`;
}

// Accepts "mm:ss.mmm", "ss.mmm" and a bare number of seconds — whichever the
// user happens to type. Anything unreadable returns null so the caller can keep
// the previous value instead of jumping to zero.
function parseTime(text) {
  const s = String(text || "").trim().replace(",", ".");
  if (!s) return null;
  const m = s.match(/^(?:(\d+):)?(\d+(?:\.\d+)?)$/);
  if (!m) return null;
  const mins = m[1] ? Number(m[1]) : 0;
  const secs = Number(m[2]);
  if (!Number.isFinite(mins) || !Number.isFinite(secs)) return null;
  return Math.round(mins * 60000 + secs * 1000);
}

const frameOf = (ms) => Math.round((ms / 1000) * VE.fps);
const frameMs = () => 1000 / (VE.fps || 30);

let toastTimer = 0;
function toast(message) {
  const el = $("ve-toast");
  el.textContent = message;
  el.classList.remove("hidden");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.add("hidden"), 2600);
}

// ---------------------------------------------------------------- confirm dialog

// The main window's confirmDialog lives in main.js and is not reachable from a
// separate window, so this one carries the same markup, classes and behaviour.
let confirmState = null;

function veConfirm(title, message, okLabel) {
  return new Promise((resolve) => {
    const overlay = $("ve-confirm");
    confirmState = { resolve, prev: document.activeElement };
    $("ve-confirm-title").textContent = title;
    $("ve-confirm-msg").textContent = message;
    $("ve-confirm-ok").textContent = okLabel || t("veDiscard");
    $("ve-confirm-cancel").textContent = t("cancel");
    overlay.classList.remove("hidden");
    $("ve-confirm-ok").focus();
  });
}

function closeConfirm(answer) {
  if (!confirmState) return;
  const { resolve, prev } = confirmState;
  confirmState = null;
  $("ve-confirm").classList.add("hidden");
  // Focus goes back where it was: a dialog that dumps focus on the body leaves
  // the next Tab starting from nowhere.
  if (prev && prev.isConnected && typeof prev.focus === "function") prev.focus();
  resolve(answer);
}

// Keeps Tab inside the open dialog instead of letting it walk the editor behind it.
function trapTabWithin(container, e) {
  const list = [...container.querySelectorAll('button, input, [tabindex]:not([tabindex="-1"]), select, textarea, a[href]')]
    .filter((el) => !el.disabled && el.offsetParent !== null);
  if (!list.length) return;
  const first = list[0], last = list[list.length - 1];
  if (!container.contains(document.activeElement)) { e.preventDefault(); first.focus(); return; }
  if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
  else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
}

$("ve-confirm-ok").addEventListener("click", () => closeConfirm(true));
$("ve-confirm-cancel").addEventListener("click", () => closeConfirm(false));

// ---------------------------------------------------------------- operation list

// VE.ops IS the render list, in order. Once the undo window is exceeded the
// floor advances instead of the oldest entry being dropped: it stays in effect,
// undo simply cannot walk back past it. Dropping it would silently un-apply an
// operation the user can still see.
function pushOp(op) {
  VE.ops.push(op);
  if (VE.ops.length - VE.floor > VE.undoMax) VE.floor = VE.ops.length - VE.undoMax;
  VE.redo.length = 0;
  afterOpsChanged();
}

// Ops that may exist only once (trim, crop, speed, fade, scale) and an overlay
// being re-edited replace their predecessor rather than stacking.
//
// The replaced value is kept on the new op as `_prev` instead of just
// being dropped -- otherwise a second crop/spotlight drag silently erased the
// first drag from history, and one undo afterward removed the operation
// entirely instead of stepping back to it. See undo()/redo() for the other
// half: they walk this chain one link at a time rather than treating a
// replacing op as an ordinary push/pop.
function pushReplacing(match, op) {
  const before = VE.ops.length;
  const prev = VE.ops.find(match) || null;
  VE.ops = VE.ops.filter((o) => !match(o));
  // The floor is a count-based boundary into VE.ops; removing entries shifts
  // every later index down, so it has to shrink by the same amount or it would
  // re-lock operations that were still undoable.
  VE.floor = Math.max(0, VE.floor - (before - VE.ops.length));
  pushOp(prev ? { ...op, _prev: prev } : op);
}

function removeOps(match) {
  const before = VE.ops.length;
  VE.ops = VE.ops.filter((o) => !match(o));
  if (VE.ops.length === before) return;
  VE.floor = Math.max(0, VE.floor - (before - VE.ops.length));
  afterOpsChanged();
}

function undo() {
  if (VE.ops.length <= VE.floor) return;
  const idx = VE.ops.length - 1;
  const cur = VE.ops[idx];
  if (cur._prev) {
    // This slot was a replacement (pushReplacing) -- step it back to the
    // value it replaced rather than removing the row outright, and keep the
    // full entry (chain intact) on the redo stack so redo can restore it.
    VE.ops[idx] = cur._prev;
    VE.redo.push(cur);
  } else {
    VE.redo.push(VE.ops.pop());
  }
  afterOpsChanged();
}

function redo() {
  if (!VE.redo.length) return;
  const next = VE.redo.pop();
  if (next._prev) {
    // The matching undo left this slot holding next._prev -- put the newer
    // value back in the same slot instead of appending a second row.
    VE.ops[VE.ops.length - 1] = next;
  } else {
    VE.ops.push(next);
  }
  afterOpsChanged();
}

function afterOpsChanged() {
  VE.dirty = true;
  $("ve-undo").disabled = VE.ops.length <= VE.floor;
  $("ve-redo").disabled = !VE.redo.length;
  if (VE.selOv && !VE.ops.some((o) => o.id === VE.selOv)) VE.selOv = null;
  renderOps();
  renderOverlayList();
  renderOverlayEditor();
  renderEditFields();
  renderRegions();
  renderRects();
  updateCutCount();
  updateExportNote();
  applyPreviewSpeed();
}

const opOf = (tool) => VE.ops.filter((o) => o.tool === tool).pop() || null;
const opsOf = (tool) => VE.ops.filter((o) => o.tool === tool);

function opLabel(op) {
  switch (op.tool) {
    case "trim": return `${t("veOpTrim")} ${fmtTime(op.startMs)} – ${fmtTime(op.endMs)}`;
    case "cut": return `${t("veOpCut")} ${fmtTime(op.startMs)} – ${fmtTime(op.endMs)}`;
    case "crop": return `${t("veOpCrop")} ${op.rect.w}×${op.rect.h}`;
    case "speed": return `${t("veOpSpeed")} ${op.rate}×`;
    case "fade": return `${t("veOpFade")} ${op.inMs}/${op.outMs} ms`;
    case "scale": return `${t("veOpScale")} ${op.height}p`;
    case "overlay": return `${t("veOpOverlay")}: ${t(overlayLabelKey(op.kind))}`;
    default: return op.tool;
  }
}

const overlayLabelKey = (kind) => ({
  image: "veOvImage", frame: "veOvFrame", text: "veOvText",
  pixelate: "veOvPixelate", spotlight: "veOvSpotlight",
}[kind] || "veOpOverlay");

function renderOps() {
  const list = $("ve-ops-list");
  list.innerHTML = "";
  if (!VE.ops.length) {
    const empty = document.createElement("p");
    empty.className = "modal-hint";
    empty.textContent = t("veOpsEmpty");
    list.appendChild(empty);
    return;
  }
  VE.ops.forEach((op, index) => {
    const row = document.createElement("div");
    row.className = "ve-op-row" + (index < VE.floor ? " ve-op-locked" : "");
    const label = document.createElement("span");
    label.className = "ve-op-label";
    label.textContent = opLabel(op);
    row.appendChild(label);
    const del = document.createElement("button");
    del.type = "button";
    del.className = "icon-btn ve-op-del";
    del.innerHTML = '<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="currentColor" d="M6.4 5 12 10.6 17.6 5 19 6.4 13.4 12 19 17.6 17.6 19 12 13.4 6.4 19 5 17.6 10.6 12 5 6.4Z"/></svg>';
    del.title = t("veOpRemove");
    del.setAttribute("aria-label", `${t("veOpRemove")}: ${opLabel(op)}`);
    del.addEventListener("click", () => removeOps((o) => o === op));
    row.appendChild(del);
    if (op.tool === "overlay") {
      row.classList.add("ve-op-clickable");
      label.addEventListener("click", () => { VE.selOv = op.id; showPanel("ve-panel-overlays"); renderOverlayList(); renderOverlayEditor(); renderRects(); });
    }
    list.appendChild(row);
  });
}

// ---------------------------------------------------------------- timeline maths

// The kept parts of the source, in order: the trim range minus every cut.
function keptSegments() {
  const trim = opOf("trim");
  const a = trim ? clamp(trim.startMs, 0, VE.dur) : 0;
  const b = trim ? clamp(trim.endMs, a, VE.dur) : VE.dur;
  const cuts = opsOf("cut")
    .map((c) => ({ a: clamp(Math.min(c.startMs, c.endMs), a, b), b: clamp(Math.max(c.startMs, c.endMs), a, b) }))
    .filter((c) => c.b - c.a > 0)
    .sort((x, y) => x.a - y.a);
  const segments = [];
  let cursor = a;
  for (const cut of cuts) {
    if (cut.a > cursor) segments.push({ a: cursor, b: cut.a });
    cursor = Math.max(cursor, cut.b);
  }
  if (b > cursor) segments.push({ a: cursor, b });
  return segments.length ? segments : [{ a, b: a }];
}

const speedRate = () => {
  const op = opOf("speed");
  return op ? clamp(op.rate, 0.1, 10) : 1;
};

// Source milliseconds -> milliseconds in the exported clip. Overlay time ranges
// are picked on the source timeline (that is what the user sees) but ffmpeg's
// `enable` expressions run on the output timeline, so every range goes through
// this on the way into the filter graph.
function outMs(srcMs) {
  let acc = 0;
  for (const seg of keptSegments()) {
    if (srcMs <= seg.a) break;
    acc += Math.min(srcMs, seg.b) - seg.a;
    if (srcMs < seg.b) break;
  }
  return acc / speedRate();
}

const totalOutMs = () => keptSegments().reduce((sum, s) => sum + (s.b - s.a), 0) / speedRate();

// Playback preview: a position inside a cut region (or outside the trim range)
// jumps to the next kept moment, so the preview plays exactly what export produces.
function nextKept(srcMs) {
  const segments = keptSegments();
  for (const seg of segments) {
    if (srcMs < seg.a) return seg.a;
    if (srcMs < seg.b) return srcMs;
  }
  return segments.length ? segments[segments.length - 1].b : 0;
}

// ---------------------------------------------------------------- timeline rendering

const trackWidth = () => clamp((VE.dur / 1000) * VE.zoom, 40, MAX_TRACK_PX);
const msToPx = (ms) => (VE.dur ? (ms / VE.dur) * trackWidth() : 0);
const pxToMs = (px) => (trackWidth() ? (px / trackWidth()) * VE.dur : 0);

function fitZoom() {
  const view = $("ve-timeline").clientWidth || 600;
  const seconds = Math.max(0.5, VE.dur / 1000);
  return clamp(view / seconds, ZOOM_MIN_PX_PER_S, ZOOM_MAX_PX_PER_S);
}

function setZoom(next, anchorMs) {
  const view = $("ve-timeline");
  const anchor = anchorMs == null ? VE.playhead : anchorMs;
  const before = msToPx(anchor) - view.scrollLeft;
  VE.zoom = clamp(next, ZOOM_MIN_PX_PER_S, ZOOM_MAX_PX_PER_S);
  layoutTrack();
  // Zoom is anchored: whatever was under the playhead stays under it, instead of
  // the view sliding away from the frame the user is working on.
  view.scrollLeft = clamp(msToPx(anchor) - before, 0, Math.max(0, trackWidth() - view.clientWidth));
  $("ve-zoom-val").textContent = `${Math.round((VE.zoom / Math.max(1, fitZoom())) * 100)}%`;
  scheduleStrip();
  scheduleWaves();
}

function layoutTrack() {
  const width = trackWidth();
  $("ve-track").style.width = `${width}px`;
  renderRuler();
  renderStrip();
  renderRegions();
  renderPlayhead();
}

// Tick spacing is picked so labels never collide: the smallest step from the
// list that is at least ~70px wide at the current zoom.
const TICK_STEPS_MS = [100, 250, 500, 1000, 2000, 5000, 10000, 15000, 30000, 60000, 120000, 300000, 600000];
function renderRuler() {
  const ruler = $("ve-ruler");
  ruler.innerHTML = "";
  if (!VE.dur) return;
  const step = TICK_STEPS_MS.find((s) => msToPx(s) >= 70) || TICK_STEPS_MS[TICK_STEPS_MS.length - 1];
  for (let ms = 0; ms <= VE.dur; ms += step) {
    const tick = document.createElement("span");
    tick.className = "ve-tick";
    tick.style.left = `${msToPx(ms)}px`;
    tick.textContent = fmtTime(ms).slice(0, step < 1000 ? 9 : 5);
    ruler.appendChild(tick);
  }
}

function renderStrip() {
  const strip = $("ve-filmstrip");
  strip.innerHTML = "";
  if (!VE.strip.length) {
    strip.classList.add("ve-filmstrip-empty");
    return;
  }
  strip.classList.remove("ve-filmstrip-empty");
  const width = trackWidth() / VE.strip.length;
  VE.strip.forEach((path, index) => {
    const img = document.createElement("img");
    img.className = "ve-thumb";
    img.alt = "";
    img.draggable = false;
    img.src = convertFileSrc(path);
    img.style.left = `${index * width}px`;
    img.style.width = `${width}px`;
    strip.appendChild(img);
  });
}

function renderRegions() {
  const trim = opOf("trim");
  const a = trim ? trim.startMs : 0;
  const b = trim ? trim.endMs : VE.dur;
  $("ve-dim-a").style.width = `${msToPx(a)}px`;
  $("ve-dim-b").style.left = `${msToPx(b)}px`;
  $("ve-dim-b").style.width = `${Math.max(0, trackWidth() - msToPx(b))}px`;
  $("ve-trim-a").style.left = `${msToPx(a)}px`;
  $("ve-trim-b").style.left = `${msToPx(b)}px`;

  const cuts = $("ve-cuts");
  cuts.innerHTML = "";
  for (const cut of opsOf("cut")) {
    const el = document.createElement("div");
    el.className = "ve-cut";
    el.style.left = `${msToPx(Math.min(cut.startMs, cut.endMs))}px`;
    el.style.width = `${Math.max(2, msToPx(Math.abs(cut.endMs - cut.startMs)))}px`;
    el.title = `${t("veOpCut")} ${fmtTime(cut.startMs)} – ${fmtTime(cut.endMs)}`;
    cuts.appendChild(el);
  }

  const bars = $("ve-ovbars");
  bars.innerHTML = "";
  for (const op of opsOf("overlay")) {
    const el = document.createElement("div");
    el.className = "ve-ovbar" + (op.id === VE.selOv ? " sel" : "");
    el.style.left = `${msToPx(op.startMs)}px`;
    el.style.width = `${Math.max(3, msToPx(Math.max(0, op.endMs - op.startMs)))}px`;
    el.title = `${t(overlayLabelKey(op.kind))} ${fmtTime(op.startMs)} – ${fmtTime(op.endMs)}`;
    bars.appendChild(el);
  }

  const sel = $("ve-sel");
  if (VE.sel) {
    sel.classList.remove("hidden");
    sel.style.left = `${msToPx(Math.min(VE.sel.a, VE.sel.b))}px`;
    sel.style.width = `${Math.max(2, msToPx(Math.abs(VE.sel.b - VE.sel.a)))}px`;
  } else {
    sel.classList.add("hidden");
  }
  // renderRegions() is the one function every VE.sel mutation already
  // calls (mark in/out, a timeline drag, clearing it) -- syncing the overlay
  // editor's "Use selection" button here, cheaply, instead of only from the
  // clear-selection handler, means a freshly made selection enables it
  // immediately even while that editor panel is already open, without
  // rebuilding the whole panel on every pointermove of a selection drag.
  document.querySelectorAll(".ve-use-sel").forEach((btn) => { btn.disabled = !VE.sel; });
}

function renderPlayhead() {
  $("ve-playhead").style.left = `${msToPx(VE.playhead)}px`;
  $("ve-time").textContent = fmtTime(VE.playhead);
  $("ve-frame").textContent = `#${frameOf(VE.playhead)}`;
}

// ---------------------------------------------------------------- filmstrip + waveform

let stripTimer = 0;
function scheduleStrip() {
  clearTimeout(stripTimer);
  stripTimer = setTimeout(loadStrip, 350);
}

async function loadStrip() {
  if (!VE.ffmpeg || !VE.src || !VE.dur) return;
  const want = clamp(Math.round(trackWidth() / THUMB_PX), 8, 160);
  // Only ever ask for more, and only when it is a real step up: regenerating a
  // filmstrip is the most expensive call this window makes.
  if (VE.stripCount && want <= VE.stripCount * 1.6) return;
  VE.stripCount = want;
  // This call can still be in flight when the user switches source
  // again -- without checking the generation on return, a slow filmstrip for
  // the OLD video could land after the new one's own (faster) call and
  // silently paint the wrong recording's thumbnails.
  const myGen = VE.loadGen;
  try {
    const paths = await invoke("generate_filmstrip", { path: VE.src, count: want });
    if (VE.loadGen !== myGen) return;
    if (Array.isArray(paths) && paths.length) {
      VE.strip = paths;
      renderStrip();
    }
  } catch (err) {
    if (VE.loadGen === myGen) noteFfmpeg(err);
  }
}

let waveTimer = 0;
function scheduleWaves() {
  clearTimeout(waveTimer);
  waveTimer = setTimeout(() => { loadWave(0); if (VE.hasMic) loadWave(1); }, 300);
}

async function loadWave(track) {
  if (!VE.ffmpeg || !VE.src) return;
  const buckets = clamp(Math.round(trackWidth()), 100, WAVE_BUCKET_MAX);
  const cached = VE.peaks[track];
  if (cached && cached.buckets === buckets) { drawWave(track); return; }
  // Same stale-response race as loadStrip() above, for the waveform.
  const myGen = VE.loadGen;
  try {
    const peaks = await invoke("generate_waveform_peaks", { path: VE.src, track, buckets });
    if (VE.loadGen !== myGen || !Array.isArray(peaks)) return;
    VE.peaks[track] = { buckets, values: peaks };
    drawWave(track);
  } catch (err) {
    if (VE.loadGen === myGen) noteFfmpeg(err);
  }
}

function drawWave(track) {
  const canvas = track === 0 ? $("ve-wave-sys") : $("ve-wave-mic");
  const data = VE.peaks[track];
  const width = Math.round(trackWidth());
  canvas.width = Math.max(1, width);
  canvas.height = 34;
  canvas.style.width = `${width}px`;
  const ctx = canvas.getContext("2d");
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  if (!data || !data.values.length) return;
  const style = getComputedStyle(document.documentElement);
  ctx.fillStyle = (style.getPropertyValue(track === 0 ? "--accent" : "--muted") || "#3b82f6").trim();
  const mid = canvas.height / 2;
  for (let x = 0; x < canvas.width; x++) {
    const value = data.values[Math.min(data.values.length - 1, Math.floor((x / canvas.width) * data.values.length))];
    const h = Math.max(1, Math.abs(Number(value) || 0) * mid);
    ctx.fillRect(x, mid - h, 1, h * 2);
  }
}

// ---------------------------------------------------------------- preview rectangles

// The video is letterboxed inside its element, so every rectangle is placed
// against the painted picture, not the element box. One helper, used by both the
// drawing and the hit-testing side, keeps the two from ever disagreeing.
function videoBox() {
  const stage = $("ve-stage").getBoundingClientRect();
  const box = $("ve-video").getBoundingClientRect();
  const w = VE.w || 16, h = VE.h || 9;
  const scale = Math.min(box.width / w, box.height / h) || 1;
  const pw = w * scale, ph = h * scale;
  return {
    left: box.left - stage.left + (box.width - pw) / 2,
    top: box.top - stage.top + (box.height - ph) / 2,
    clientLeft: box.left + (box.width - pw) / 2,
    clientTop: box.top + (box.height - ph) / 2,
    scale,
  };
}

const toSource = (clientX, clientY) => {
  const vb = videoBox();
  return { x: (clientX - vb.clientLeft) / vb.scale, y: (clientY - vb.clientTop) / vb.scale };
};

// ---------------------------------------------------------------- preview zoom/pan
//
// Zooming alone never let you land on an exact spot -- this adds the missing
// half, a plain CSS transform on the <video> element. videoBox()/toSource()
// above need no changes for it: both already read the video element's LIVE
// bounding rect, which reflects any transform automatically, so a crop/overlay
// rectangle stays correctly placed and draggable at any zoom/pan without a
// second coordinate system. 1 = fit-to-stage, the only level that existed
// before and the floor here too -- shrinking further would just add more
// letterboxing, never show more of the picture.
const PREVIEW_ZOOM_MIN = 1;
const PREVIEW_ZOOM_MAX = 8;

// The video's own painted size at zoom 1, straight from the stage + source
// aspect ratio -- kept independent of the live (already zoomed) bounding rect
// so panning can be clamped without having to divide that rect back down.
function fitBoxSize() {
  const stage = $("ve-stage").getBoundingClientRect();
  const w = VE.w || 16, h = VE.h || 9;
  const scale = Math.min(stage.width / w, stage.height / h) || 1;
  return { w: w * scale, h: h * scale, stageW: stage.width, stageH: stage.height };
}

// Keeps the picture from being dragged fully out of view: at most half the
// zoomed overhang in either axis, zero once back at zoom 1.
function clampPan(pan, zoom) {
  const { w, h, stageW, stageH } = fitBoxSize();
  const maxX = Math.max(0, (w * zoom - stageW) / 2);
  const maxY = Math.max(0, (h * zoom - stageH) / 2);
  return { x: clamp(pan.x, -maxX, maxX), y: clamp(pan.y, -maxY, maxY) };
}

function applyPreviewTransform() {
  const zoomed = VE.previewZoom !== 1 || VE.panPx.x || VE.panPx.y;
  $("ve-video").style.transform = zoomed ? `translate(${VE.panPx.x}px, ${VE.panPx.y}px) scale(${VE.previewZoom})` : "";
  $("ve-stage").classList.toggle("ve-zoomed", VE.previewZoom !== 1);
  const pill = $("ve-preview-zoom");
  if (pill) {
    pill.classList.toggle("hidden", VE.previewZoom === 1);
    $("ve-preview-zoom-val").textContent = `${Math.round(VE.previewZoom * 100)}%`;
  }
  renderRects(); // rectangle overlays are positioned from the same live rect
}

function setPreviewZoom(z) {
  VE.previewZoom = clamp(z, PREVIEW_ZOOM_MIN, PREVIEW_ZOOM_MAX);
  VE.panPx = clampPan(VE.panPx, VE.previewZoom);
  applyPreviewTransform();
}

function resetPreviewZoom() {
  VE.previewZoom = 1;
  VE.panPx = { x: 0, y: 0 };
  applyPreviewTransform();
}

// Plain wheel, no modifier: the stage never scrolls on its own, so the
// gesture is free to mean zoom (unlike the timeline, which needs Ctrl+wheel
// because a plain wheel already scrolls it sideways).
$("ve-stage").addEventListener("wheel", (e) => {
  if (!VE.w) return;
  e.preventDefault();
  setPreviewZoom(VE.previewZoom * (e.deltaY < 0 ? 1.2 : 1 / 1.2));
}, { passive: false });

// Pan: middle-click always (matches the screenshot editor), plain left-drag
// only once actually zoomed in -- at zoom 1 a left-drag on empty stage space
// was always a no-op and stays one, rather than arming a grab cursor that
// then visibly does nothing. Space is already Play/Pause in this window
// (unlike the screenshot editor), so it is deliberately not a pan modifier
// here. A click that lands on a crop/overlay rectangle is left alone entirely
// -- $("ve-rects")'s own pointerdown owns that drag.
$("ve-stage").addEventListener("pointerdown", (e) => {
  if (!VE.w || (e.button !== 0 && e.button !== 1)) return;
  // A second mouse button pressed while a rect/trim/select/scrub drag is
  // already in progress fires another pointerdown for the same pointer --
  // without this it would steal capture and silently overwrite that drag.
  if (VE.drag) return;
  if (e.target.closest(".ve-rect")) return;
  if (e.target.closest("#ve-preview-zoom")) return; // the zoom pill/reset button sits inside the stage
  if (VE.previewZoom <= 1) return; // nothing to pan at 1:1, whichever button
  e.preventDefault();
  $("ve-stage").setPointerCapture(e.pointerId);
  VE.drag = { kind: "pan", x: e.clientX, y: e.clientY, start: { ...VE.panPx } };
  $("ve-stage").classList.add("ve-panning");
});
$("ve-stage").addEventListener("pointermove", (e) => {
  const drag = VE.drag;
  if (!drag || drag.kind !== "pan") return;
  VE.panPx = clampPan({ x: drag.start.x + (e.clientX - drag.x), y: drag.start.y + (e.clientY - drag.y) }, VE.previewZoom);
  applyPreviewTransform();
});
function endPreviewPan() {
  if (VE.drag && VE.drag.kind === "pan") VE.drag = null;
  $("ve-stage").classList.remove("ve-panning");
}
$("ve-stage").addEventListener("pointerup", endPreviewPan);
$("ve-stage").addEventListener("pointercancel", endPreviewPan);
$("ve-preview-zoom-reset").addEventListener("click", resetPreviewZoom);

// Every rectangle is snapped to even numbers while it is being dragged, not at
// export time — so the numbers on screen are already the numbers ffmpeg gets.
function snapRect(rect) {
  // Clamping an already-even width/height against an ODD source
  // dimension can land back on an odd number (e.g. even(641)=642, clamped
  // down to the odd ceiling 641) -- ffmpeg's yuv420p output needs both to
  // stay even, so the ceiling itself must already be even before clamping.
  const wMax = Math.max(2, VE.w - (VE.w % 2));
  const hMax = Math.max(2, VE.h - (VE.h % 2));
  const w = clamp(even(rect.w), 2, wMax);
  const h = clamp(even(rect.h), 2, hMax);
  return {
    x: clamp(even(rect.x), 0, Math.max(0, VE.w - w)),
    y: clamp(even(rect.y), 0, Math.max(0, VE.h - h)),
    w, h,
  };
}

const HANDLES = ["nw", "n", "ne", "w", "e", "sw", "s", "se"];

function rectFor(op) {
  if (op.tool === "crop") return op.rect;
  if (op.tool === "overlay" && op.rect) return op.rect;
  return null;
}

// The visible rectangles: the crop box plus whichever overlay is being edited.
function renderRects() {
  const layer = $("ve-rects");
  layer.innerHTML = "";
  const vb = videoBox();
  const items = [];
  const crop = opOf("crop");
  if (crop) items.push({ op: crop, kind: "crop" });
  const selected = VE.ops.find((o) => o.id && o.id === VE.selOv);
  if (selected && rectFor(selected)) items.push({ op: selected, kind: selected.kind });

  for (const item of items) {
    const rect = rectFor(item.op);
    const el = document.createElement("div");
    el.className = `ve-rect ve-rect-${item.kind}`;
    el.dataset.op = item.op.id || "crop";
    el.style.left = `${vb.left + rect.x * vb.scale}px`;
    el.style.top = `${vb.top + rect.y * vb.scale}px`;
    el.style.width = `${rect.w * vb.scale}px`;
    el.style.height = `${rect.h * vb.scale}px`;
    const size = document.createElement("span");
    size.className = "ve-rect-size";
    size.textContent = `${rect.w}×${rect.h}`;
    el.appendChild(size);
    for (const name of HANDLES) {
      const handle = document.createElement("span");
      handle.className = `ve-rect-handle ve-h-${name}`;
      handle.dataset.handle = name;
      el.appendChild(handle);
    }
    layer.appendChild(el);
  }

  // A text overlay has a point, not a rectangle — it gets a draggable label chip
  // showing roughly where the line will start.
  if (selected && selected.kind === "text") {
    const chip = document.createElement("div");
    chip.className = "ve-rect ve-rect-text ve-text-chip";
    chip.dataset.op = selected.id;
    chip.style.left = `${vb.left + selected.x * vb.scale}px`;
    chip.style.top = `${vb.top + selected.y * vb.scale}px`;
    chip.style.font = `${Math.max(8, selected.fontPx * vb.scale)}px "Segoe UI", system-ui, sans-serif`;
    chip.style.color = selected.color;
    chip.textContent = selected.text || t("veOvText");
    layer.appendChild(chip);
  }

  renderOverlayLive(vb);
}

// Every overlay actually active at the current playhead, drawn as close
// to its real exported look as the browser can manage -- not just an edit
// outline. Overlay time ranges are picked on the SOURCE timeline (same as
// VE.playhead here), so no outMs() conversion is needed like buildVideoChain()
// needs for ffmpeg's output-timeline `enable` clauses.
function renderOverlayLive(vb) {
  const layer = $("ve-overlay-live");
  layer.innerHTML = "";
  if (!VE.dur) return;
  const box = vb || videoBox();
  const now = VE.playhead;
  for (const op of opsOf("overlay")) {
    if (now < op.startMs || now > op.endMs) continue;
    const el = buildLiveOverlayEl(op, box);
    if (el) layer.appendChild(el);
  }
}

function buildLiveOverlayEl(op, vb) {
  const rectStyle = (el, rect) => {
    el.style.left = `${vb.left + rect.x * vb.scale}px`;
    el.style.top = `${vb.top + rect.y * vb.scale}px`;
    el.style.width = `${rect.w * vb.scale}px`;
    el.style.height = `${rect.h * vb.scale}px`;
  };
  if (op.kind === "image") {
    const img = document.createElement("img");
    img.className = "ve-live ve-live-image";
    img.src = convertFileSrc(op.path);
    rectStyle(img, op.rect);
    img.style.opacity = String(clamp(op.opacity, 5, 100) / 100);
    return img;
  }
  if (op.kind === "frame") {
    const el = document.createElement("div");
    el.className = "ve-live ve-live-frame";
    rectStyle(el, op.rect);
    const px = Math.max(1, op.thickness * vb.scale);
    el.style.opacity = String(clamp(op.opacity, 0, 100) / 100);
    el.style.borderWidth = `${px}px`;
    el.style.borderColor = op.color;
    if (op.style === "double") {
      // Approximates the second, inset drawbox with an outline outside the
      // border instead of a truly separate inner box -- close enough for a
      // preview whose job is "roughly where and how thick", not pixel parity.
      el.style.outline = `${px}px solid ${op.color}`;
      el.style.outlineOffset = `${op.thickness * vb.scale}px`;
    } else if (op.style === "soft") {
      el.style.boxShadow = `0 0 ${px * 2}px ${px}px ${op.color}`;
    }
    return el;
  }
  if (op.kind === "text") {
    const el = document.createElement("div");
    el.className = "ve-live ve-live-text";
    el.style.left = `${vb.left + op.x * vb.scale}px`;
    el.style.top = `${vb.top + op.y * vb.scale}px`;
    el.style.fontSize = `${Math.max(1, op.fontPx * vb.scale)}px`;
    el.style.fontFamily = FONT_CSS[op.font] || FONT_CSS.segoeui;
    el.style.fontWeight = op.font === "arialbd" ? "bold" : "normal";
    el.style.color = op.color;
    if (op.plate !== false) {
      el.style.background = "rgba(0, 0, 0, 0.5)";
      el.style.padding = `${Math.max(4, Math.round(op.fontPx / 4)) * vb.scale}px`;
    }
    el.textContent = op.text;
    return el;
  }
  if (op.kind === "pixelate") {
    const w = Math.max(1, Math.round(op.rect.w * vb.scale)), h = Math.max(1, Math.round(op.rect.h * vb.scale));
    const canvas = document.createElement("canvas");
    canvas.className = "ve-live ve-live-pixelate";
    canvas.width = w;
    canvas.height = h;
    rectStyle(canvas, op.rect);
    // Down-sample the live video frame to the real block grid, then blow it
    // back up with smoothing off -- the same two-step mosaic buildVideoChain()
    // asks ffmpeg for, just done in a canvas instead of a filter graph.
    const bw = Math.max(1, Math.round(op.rect.w / op.blockPx)), bh = Math.max(1, Math.round(op.rect.h / op.blockPx));
    try {
      const tiny = document.createElement("canvas");
      tiny.width = bw;
      tiny.height = bh;
      tiny.getContext("2d").drawImage(videoEl, op.rect.x, op.rect.y, op.rect.w, op.rect.h, 0, 0, bw, bh);
      const ctx = canvas.getContext("2d");
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(tiny, 0, 0, bw, bh, 0, 0, w, h);
    } catch {
      // A frame that is not yet decoded (seek in flight) -- skip this tick,
      // the next one draws once the video element has something to sample.
    }
    return canvas;
  }
  if (op.kind === "spotlight") {
    const el = document.createElement("div");
    el.className = "ve-live ve-live-spotlight";
    rectStyle(el, op.rect);
    // Same "hole punched in an oversized shadow" trick .ve-rect-spotlight
    // already uses for its edit outline, just at the overlay's own dim level.
    el.style.boxShadow = `0 0 0 9999px rgba(0, 0, 0, ${clamp(op.dim, 0, 1).toFixed(2)})`;
    return el;
  }
  return null;
}

function resizeRect(orig, handle, point, keepRatio) {
  let x1 = orig.x, y1 = orig.y, x2 = orig.x + orig.w, y2 = orig.y + orig.h;
  if (handle.includes("n")) y1 = point.y;
  if (handle.includes("s")) y2 = point.y;
  if (handle.includes("w")) x1 = point.x;
  if (handle.includes("e")) x2 = point.x;
  const next = { x: Math.min(x1, x2), y: Math.min(y1, y2), w: Math.abs(x2 - x1), h: Math.abs(y2 - y1) };
  if (keepRatio && orig.w && orig.h) {
    const ratio = orig.w / orig.h;
    if (handle === "n" || handle === "s") next.w = next.h * ratio;
    else next.h = next.w / ratio;
  }
  return next;
}

$("ve-rects").addEventListener("pointerdown", (e) => {
  const rectEl = e.target.closest(".ve-rect");
  if (!rectEl) return;
  const id = rectEl.dataset.op;
  const op = id === "crop" ? opOf("crop") : VE.ops.find((o) => o.id === id);
  if (!op) return;
  e.preventDefault();
  $("ve-rects").setPointerCapture(e.pointerId);
  const point = toSource(e.clientX, e.clientY);
  const handle = e.target.dataset.handle || null;
  VE.drag = {
    kind: "rect",
    op,
    handle,
    start: point,
    orig: op.rect ? { ...op.rect } : { x: op.x, y: op.y, w: 0, h: 0 },
    live: op.rect ? { ...op.rect } : null,
    livePoint: op.rect ? null : { x: op.x, y: op.y },
  };
});

$("ve-rects").addEventListener("pointermove", (e) => {
  const drag = VE.drag;
  if (!drag || drag.kind !== "rect") return;
  const point = toSource(e.clientX, e.clientY);
  if (drag.live) {
    if (drag.handle) {
      drag.live = snapRect(resizeRect(drag.orig, drag.handle, point, e.shiftKey));
    } else {
      drag.live = snapRect({
        x: drag.orig.x + (point.x - drag.start.x),
        y: drag.orig.y + (point.y - drag.start.y),
        w: drag.orig.w, h: drag.orig.h,
      });
    }
    drag.op.rect = drag.live; // live preview only; the op is re-pushed on release
  } else {
    drag.livePoint = {
      x: clamp(Math.round(drag.orig.x + (point.x - drag.start.x)), 0, VE.w),
      y: clamp(Math.round(drag.orig.y + (point.y - drag.start.y)), 0, VE.h),
    };
    drag.op.x = drag.livePoint.x;
    drag.op.y = drag.livePoint.y;
  }
  renderRects();
});

const rectsEqual = (a, b) => !!a && !!b && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;

function endRectDrag() {
  const drag = VE.drag;
  VE.drag = null;
  if (!drag || drag.kind !== "rect") return;
  const op = drag.op;
  const raw = drag.live && drag.live.w >= RECT_MIN_PX && drag.live.h >= RECT_MIN_PX ? drag.live : drag.orig;
  if (op.tool === "crop") {
    const rect = snapRect(raw);
    // A click-release with no actual movement (or one that snaps right
    // back to where it started) must not count as an edit -- it used to
    // still push a fresh undo step and wipe the redo stack for nothing.
    if (rectsEqual(rect, drag.orig)) { op.rect = rect; renderRects(); return; }
    pushReplacing((o) => o.tool === "crop", { tool: "crop", rect });
  } else {
    if (rectsEqual(raw, drag.orig)) { op.rect = raw; renderRects(); return; }
    pushReplacing((o) => o.id === op.id, { ...op, rect: raw });
  }
}

$("ve-rects").addEventListener("pointerup", endRectDrag);
$("ve-rects").addEventListener("pointercancel", endRectDrag);

// ---------------------------------------------------------------- playback

const videoEl = $("ve-video");

function updatePlayButton() {
  const playing = !videoEl.paused && !videoEl.ended;
  $("ve-play-icon").classList.toggle("hidden", playing);
  $("ve-pause-icon").classList.toggle("hidden", !playing);
  const label = t(playing ? "vePause" : "vePlay");
  $("ve-play").title = label;
  $("ve-play").setAttribute("aria-label", label);
}

function applyPreviewSpeed() {
  const rate = speedRate();
  const op = opOf("speed");
  // The browser's own limits are narrower than ffmpeg's; the preview shows what
  // it can and the export is unaffected either way.
  videoEl.playbackRate = clamp(rate, 0.0625, 16);
  if ("preservesPitch" in videoEl) videoEl.preservesPitch = op ? op.pitch !== false : true;
  const sys = VE.audio.sys;
  videoEl.muted = sys.mute;
  videoEl.volume = clamp(sys.vol / 100, 0, 1);
}

function seek(ms) {
  const target = clamp(ms, 0, VE.dur);
  VE.playhead = target;
  videoEl.currentTime = target / 1000;
  renderPlayhead();
  renderOverlayLive();
  keepPlayheadVisible();
}

function keepPlayheadVisible() {
  const view = $("ve-timeline");
  const x = msToPx(VE.playhead);
  const left = view.scrollLeft;
  const right = left + view.clientWidth;
  if (x < left + 20) view.scrollLeft = Math.max(0, x - 40);
  else if (x > right - 20) view.scrollLeft = Math.max(0, x - view.clientWidth + 60);
}

// Cut regions are skipped seamlessly during playback, so the preview is the
// finished clip and not the raw capture with markers drawn on it. A frame loop
// rather than the video's own timeupdate event: that fires roughly four times a
// second, which is long enough to see a cut region play before it is skipped.
function tick() {
  if (VE.dur) {
    const segments = keptSegments();
    const last = segments[segments.length - 1];
    const now = videoEl.currentTime * 1000;
    if (!videoEl.paused) {
      if (now >= last.b - 1) {
        videoEl.pause();
        seek(last.b);
      } else {
        const kept = nextKept(now);
        if (kept > now + 1) videoEl.currentTime = kept / 1000;
      }
    }
    if (Math.abs(now - VE.playhead) > 0.5) {
      VE.playhead = now;
      renderPlayhead();
      renderOverlayLive();
      if (!videoEl.paused) keepPlayheadVisible();
    }
  }
  requestAnimationFrame(tick);
}

function togglePlay() {
  if (videoEl.paused) {
    const segments = keptSegments();
    const last = segments[segments.length - 1];
    // Play again from the start of the kept range once the end was reached,
    // instead of sitting on the last frame doing nothing.
    if (VE.playhead >= last.b - 1) seek(segments[0].a);
    videoEl.play().catch(() => {});
  } else {
    videoEl.pause();
  }
  updatePlayButton();
}

videoEl.addEventListener("play", updatePlayButton);
videoEl.addEventListener("pause", updatePlayButton);

// ---------------------------------------------------------------- timeline input

const timelineEl = $("ve-timeline");
const regionsEl = $("ve-regions");

function timelineMs(clientX) {
  const rect = $("ve-track").getBoundingClientRect();
  return clamp(pxToMs(clientX - rect.left), 0, VE.dur);
}

regionsEl.addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  const handle = e.target.closest(".ve-handle");
  regionsEl.setPointerCapture(e.pointerId);
  const ms = timelineMs(e.clientX);
  if (handle) {
    const trim = opOf("trim") || { startMs: 0, endMs: VE.dur };
    VE.drag = { kind: "trim", edge: handle.id === "ve-trim-a" ? "a" : "b", live: { startMs: trim.startMs, endMs: trim.endMs } };
    return;
  }
  if (e.shiftKey) {
    VE.drag = { kind: "select", anchor: ms };
    VE.sel = { a: ms, b: ms };
    renderRegions();
    return;
  }
  VE.drag = { kind: "scrub" };
  seek(ms);
});

regionsEl.addEventListener("pointermove", (e) => {
  const drag = VE.drag;
  if (!drag) return;
  const ms = timelineMs(e.clientX);
  if (drag.kind === "scrub") { seek(ms); return; }
  if (drag.kind === "select") {
    VE.sel = { a: Math.min(drag.anchor, ms), b: Math.max(drag.anchor, ms) };
    renderRegions();
    return;
  }
  if (drag.kind === "trim") {
    if (drag.edge === "a") drag.live.startMs = clamp(ms, 0, drag.live.endMs - MIN_CLIP_MS);
    else drag.live.endMs = clamp(ms, drag.live.startMs + MIN_CLIP_MS, VE.dur);
    $("ve-trim-a").style.left = `${msToPx(drag.live.startMs)}px`;
    $("ve-trim-b").style.left = `${msToPx(drag.live.endMs)}px`;
    $("ve-dim-a").style.width = `${msToPx(drag.live.startMs)}px`;
    $("ve-dim-b").style.left = `${msToPx(drag.live.endMs)}px`;
    $("ve-dim-b").style.width = `${Math.max(0, trackWidth() - msToPx(drag.live.endMs))}px`;
  }
});

function endTimelineDrag() {
  const drag = VE.drag;
  if (!drag) return;
  VE.drag = null;
  if (drag.kind === "trim") setTrim(drag.live.startMs, drag.live.endMs);
}

regionsEl.addEventListener("pointerup", endTimelineDrag);
regionsEl.addEventListener("pointercancel", endTimelineDrag);

// Ctrl+wheel zooms around the playhead; a plain wheel scrolls the timeline
// sideways, which is the only direction it can go.
timelineEl.addEventListener("wheel", (e) => {
  if (e.ctrlKey) {
    e.preventDefault();
    setZoom(VE.zoom * (e.deltaY < 0 ? 1.25 : 0.8), VE.playhead);
  } else if (e.deltaY && !e.shiftKey) {
    e.preventDefault();
    timelineEl.scrollLeft += e.deltaY;
  }
}, { passive: false });

// ---------------------------------------------------------------- edit panel

function setTrim(startMs, endMs) {
  const a = clamp(Math.min(startMs, endMs), 0, VE.dur);
  const b = clamp(Math.max(startMs, endMs), a + MIN_CLIP_MS, VE.dur);
  if (a <= 0 && b >= VE.dur) { removeOps((o) => o.tool === "trim"); return; }
  pushReplacing((o) => o.tool === "trim", { tool: "trim", startMs: a, endMs: b });
}

function renderEditFields() {
  const trim = opOf("trim");
  const a = trim ? trim.startMs : 0;
  const b = trim ? trim.endMs : VE.dur;
  if (document.activeElement !== $("ve-trim-start-val")) $("ve-trim-start-val").value = fmtTime(a);
  if (document.activeElement !== $("ve-trim-end-val")) $("ve-trim-end-val").value = fmtTime(b);

  const crop = opOf("crop");
  $("ve-crop-on").checked = !!crop;
  $("ve-crop-fields").classList.toggle("ve-off", !crop);
  for (const input of $("ve-crop-fields").querySelectorAll("input, button")) input.disabled = !crop;
  const rect = crop ? crop.rect : { x: 0, y: 0, w: VE.w, h: VE.h };
  if (document.activeElement?.closest?.("#ve-crop-fields") == null) {
    $("ve-crop-x").value = rect.x;
    $("ve-crop-y").value = rect.y;
    $("ve-crop-w").value = rect.w;
    $("ve-crop-h").value = rect.h;
  }

  const speed = opOf("speed");
  const rate = speed ? speed.rate : 1;
  for (const btn of $("ve-speed-steps").querySelectorAll("button")) {
    btn.classList.toggle("sel", Math.abs(Number(btn.dataset.rate) - rate) < 0.001);
  }
  if (document.activeElement !== $("ve-speed-custom")) $("ve-speed-custom").value = rate;
  $("ve-speed-pitch").checked = speed ? speed.pitch !== false : true;

  const fade = opOf("fade");
  $("ve-fade-on").checked = !!fade;
  $("ve-fade-fields").classList.toggle("ve-off", !fade);
  for (const input of $("ve-fade-fields").querySelectorAll("input")) input.disabled = !fade;
  if (fade) {
    if (document.activeElement !== $("ve-fade-in")) $("ve-fade-in").value = fade.inMs;
    if (document.activeElement !== $("ve-fade-out")) $("ve-fade-out").value = fade.outMs;
  }

  const scale = opOf("scale");
  $("ve-scale").value = scale ? String(scale.height) : "";
}

function renderSpeedSteps() {
  const wrap = $("ve-speed-steps");
  wrap.innerHTML = "";
  for (const rate of SPEED_STEPS) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "ghost-btn ve-step";
    btn.dataset.rate = String(rate);
    btn.textContent = `${rate}×`;
    btn.title = `${t("veSpeedGroup")} ${rate}×`;
    btn.addEventListener("click", () => setSpeed(rate));
    wrap.appendChild(btn);
  }
  renderEditFields();
}

function setSpeed(rate) {
  const value = clamp(Number(rate) || 1, 0.1, 10);
  const pitch = $("ve-speed-pitch").checked;
  if (Math.abs(value - 1) < 0.001) { removeOps((o) => o.tool === "speed"); return; }
  pushReplacing((o) => o.tool === "speed", { tool: "speed", rate: Number(value.toFixed(3)), pitch });
}

function updateCutCount() {
  const cuts = opsOf("cut");
  $("ve-cut-count").textContent = cuts.length
    ? t("veCutCount").replace("{n}", String(cuts.length)).replace("{d}", fmtTime(totalOutMs()))
    : t("veCutNone").replace("{d}", fmtTime(totalOutMs()));
  $("ve-cut-clear").disabled = !cuts.length;
}

// ---------------------------------------------------------------- overlays

let overlaySeq = 0;
const newId = () => `ov${++overlaySeq}-${Date.now().toString(36)}`;

function defaultRange() {
  if (VE.sel) return { startMs: Math.min(VE.sel.a, VE.sel.b), endMs: Math.max(VE.sel.a, VE.sel.b) };
  const trim = opOf("trim");
  return { startMs: trim ? trim.startMs : 0, endMs: trim ? trim.endMs : VE.dur };
}

function centeredRect(fraction) {
  const w = even(VE.w * fraction), h = even(VE.h * fraction);
  return snapRect({ x: (VE.w - w) / 2, y: (VE.h - h) / 2, w, h });
}

async function addOverlay(kind) {
  const range = defaultRange();
  const base = { tool: "overlay", id: newId(), kind, ...range };
  let op = null;
  if (kind === "image") {
    const path = await invoke("pick_file_path").catch(() => null);
    if (!path) return;
    // An apostrophe in the path has no escape ffmpeg's filter parser accepts, so
    // it is refused here with a reason instead of failing at export time.
    if (path.includes("'")) { toast(t("veOvImageQuote")); return; }
    const rect = centeredRect(0.3);
    op = { ...base, path, rect, opacity: VE.ovImageOpacityDefault ?? 100 };
  } else if (kind === "frame") {
    op = { ...base, rect: snapRect({ x: 0, y: 0, w: VE.w, h: VE.h }), color: "#ffffff", thickness: Math.max(2, even(VE.h * 0.01)), style: "solid", opacity: 100 };
  } else if (kind === "text") {
    op = { ...base, text: t("veOvTextDefault"), x: even(VE.w * 0.08), y: even(VE.h * 0.08), fontPx: Math.max(12, Math.round(VE.h / 16)), font: FONT_FILES[0][0], color: "#ffffff", plate: true };
  } else if (kind === "pixelate") {
    op = { ...base, rect: centeredRect(0.35), blockPx: VE.ovPixelateBlockDefault ?? 16 };
  } else if (kind === "spotlight") {
    op = { ...base, rect: centeredRect(0.45), dim: VE.ovSpotlightDimDefault ?? 0.6 };
  }
  if (!op) return;
  VE.selOv = op.id;
  pushOp(op);
  showPanel("ve-panel-overlays");
}

function renderOverlayList() {
  const list = $("ve-ov-list");
  list.innerHTML = "";
  const overlays = opsOf("overlay");
  $("ve-ov-empty").classList.toggle("hidden", overlays.length > 0);
  for (const op of overlays) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "ve-ov-row" + (op.id === VE.selOv ? " sel" : "");
    row.textContent = `${t(overlayLabelKey(op.kind))} · ${fmtTime(op.startMs)} – ${fmtTime(op.endMs)}`;
    row.addEventListener("click", () => {
      VE.selOv = VE.selOv === op.id ? null : op.id;
      renderOverlayList();
      renderOverlayEditor();
      renderRects();
      renderRegions();
    });
    list.appendChild(row);
  }
}

// Small builders so every overlay field looks like the rest of the app's fields
// instead of each panel inventing its own row shape.
function fieldRow(labelText, control) {
  const label = document.createElement("label");
  label.className = "field";
  const span = document.createElement("span");
  span.textContent = labelText;
  label.append(span, control);
  return label;
}

function numberInput(value, min, max, step, onChange) {
  const input = document.createElement("input");
  input.type = "number";
  input.className = "modal-input";
  input.min = String(min);
  input.max = String(max);
  input.step = String(step);
  input.value = String(value);
  input.addEventListener("change", () => {
    const next = clamp(Number(input.value), min, max);
    input.value = String(next);
    onChange(next);
  });
  return input;
}

function textInput(value, onChange) {
  const input = document.createElement("input");
  input.type = "text";
  input.className = "modal-input";
  input.value = value;
  input.addEventListener("change", () => onChange(input.value));
  return input;
}

function selectInput(options, value, onChange) {
  const select = document.createElement("select");
  select.className = "modal-input";
  for (const [val, label] of options) {
    const option = document.createElement("option");
    option.value = val;
    option.textContent = label;
    select.appendChild(option);
  }
  select.value = value;
  select.addEventListener("change", () => onChange(select.value));
  return select;
}

function colorRow(current, onPick) {
  const wrap = document.createElement("div");
  wrap.className = "swatches ve-swatches";
  for (const hex of OVERLAY_COLORS) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "swatch" + (hex.toLowerCase() === String(current).toLowerCase() ? " sel" : "");
    btn.style.background = hex;
    btn.title = hex;
    btn.setAttribute("aria-label", hex);
    btn.addEventListener("click", () => onPick(hex));
    wrap.appendChild(btn);
  }
  return wrap;
}

function commitOverlay(op, changes) {
  pushReplacing((o) => o.id === op.id, { ...op, ...changes });
}

function renderOverlayEditor() {
  const box = $("ve-ov-editor");
  box.innerHTML = "";
  const op = VE.ops.find((o) => o.id === VE.selOv);
  if (!op) { box.classList.add("hidden"); return; }
  box.classList.remove("hidden");

  const head = document.createElement("h3");
  head.className = "ve-h";
  head.textContent = t(overlayLabelKey(op.kind));
  box.appendChild(head);

  // Pixelate and spotlight cover one fixed rectangle for one time range. A target
  // that moves needs a second overlay with its own range and position.
  if (op.kind === "pixelate" || op.kind === "spotlight") {
    const note = document.createElement("p");
    note.className = "modal-hint";
    note.textContent = t("veOvRectTip");
    note.title = t("veOvRectTip");
    box.appendChild(note);
  }

  const range = document.createElement("div");
  range.className = "ve-row2";
  const startInput = textInput(fmtTime(op.startMs), (v) => {
    const ms = parseTime(v);
    if (ms != null) commitOverlay(op, { startMs: clamp(ms, 0, op.endMs) });
    else renderOverlayEditor();
  });
  const endInput = textInput(fmtTime(op.endMs), (v) => {
    const ms = parseTime(v);
    if (ms != null) commitOverlay(op, { endMs: clamp(ms, op.startMs, VE.dur) });
    else renderOverlayEditor();
  });
  range.append(fieldRow(t("veOvFrom"), startInput), fieldRow(t("veOvTo"), endInput));
  box.appendChild(range);

  const useSel = document.createElement("button");
  useSel.type = "button";
  useSel.className = "ghost-btn ve-wide ve-use-sel";
  useSel.textContent = t("veOvUseSelection");
  useSel.disabled = !VE.sel;
  useSel.addEventListener("click", () => {
    if (!VE.sel) return;
    commitOverlay(op, { startMs: Math.min(VE.sel.a, VE.sel.b), endMs: Math.max(VE.sel.a, VE.sel.b) });
  });
  box.appendChild(useSel);

  if (op.kind === "image") {
    const pathLine = document.createElement("p");
    pathLine.className = "ve-path";
    pathLine.textContent = op.path;
    pathLine.title = op.path;
    box.appendChild(pathLine);
    const change = document.createElement("button");
    change.type = "button";
    change.className = "ghost-btn ve-wide";
    change.textContent = t("pathChange");
    change.addEventListener("click", async () => {
      const path = await invoke("pick_file_path").catch(() => null);
      if (!path) return;
      if (path.includes("'")) { toast(t("veOvImageQuote")); return; }
      commitOverlay(op, { path });
    });
    box.appendChild(change);
    box.appendChild(fieldRow(t("veOvOpacity"), numberInput(op.opacity, 5, 100, 5, (v) => commitOverlay(op, { opacity: v }))));
  }

  if (op.kind === "frame") {
    box.appendChild(fieldRow(t("veOvThickness"), numberInput(op.thickness, 1, Math.max(4, Math.round(VE.h / 4)), 1, (v) => commitOverlay(op, { thickness: v }))));
    box.appendChild(fieldRow(t("veOvStyle"), selectInput(
      [["solid", t("veOvStyleSolid")], ["double", t("veOvStyleDouble")], ["soft", t("veOvStyleSoft")]],
      op.style, (v) => commitOverlay(op, { style: v }))));
    box.appendChild(fieldRow(t("veOvColor"), colorRow(op.color, (hex) => commitOverlay(op, { color: hex }))));
    box.appendChild(fieldRow(t("veOvOpacity"), numberInput(op.opacity, 5, 100, 5, (v) => commitOverlay(op, { opacity: v }))));
  }

  if (op.kind === "text") {
    box.appendChild(fieldRow(t("veOvTextValue"), textInput(op.text, (v) => commitOverlay(op, { text: v }))));
    box.appendChild(fieldRow(t("veOvFont"), selectInput(FONT_FILES.map(([id]) => [id, id]), op.font, (v) => commitOverlay(op, { font: v }))));
    box.appendChild(fieldRow(t("veOvFontSize"), numberInput(op.fontPx, 8, Math.max(16, VE.h), 1, (v) => commitOverlay(op, { fontPx: v }))));
    const pos = document.createElement("div");
    pos.className = "ve-row2";
    pos.append(
      fieldRow(t("veCropX"), numberInput(op.x, 0, VE.w, 1, (v) => commitOverlay(op, { x: v }))),
      fieldRow(t("veCropY"), numberInput(op.y, 0, VE.h, 1, (v) => commitOverlay(op, { y: v }))));
    box.appendChild(pos);
    box.appendChild(fieldRow(t("veOvColor"), colorRow(op.color, (hex) => commitOverlay(op, { color: hex }))));
    const plate = document.createElement("label");
    plate.className = "field switch-field ve-switch-row";
    const plateLabel = document.createElement("span");
    plateLabel.textContent = t("veOvPlate");
    const plateBox = document.createElement("input");
    plateBox.type = "checkbox";
    plateBox.className = "switch";
    plateBox.checked = op.plate !== false;
    plateBox.addEventListener("change", () => commitOverlay(op, { plate: plateBox.checked }));
    plate.append(plateLabel, plateBox);
    box.appendChild(plate);
  }

  if (op.kind === "pixelate") {
    box.appendChild(fieldRow(t("veOvBlock"), numberInput(op.blockPx, 4, 80, 2, (v) => commitOverlay(op, { blockPx: v }))));
  }

  if (op.kind === "spotlight") {
    box.appendChild(fieldRow(t("veOvDim"), numberInput(Math.round(op.dim * 100), 10, 95, 5, (v) => commitOverlay(op, { dim: v / 100 }))));
  }

  if (op.rect) {
    const grid = document.createElement("div");
    grid.className = "ve-row4";
    const setRect = (key) => (v) => commitOverlay(op, { rect: snapRect({ ...op.rect, [key]: v }) });
    grid.append(
      fieldRow(t("veCropX"), numberInput(op.rect.x, 0, VE.w, 2, setRect("x"))),
      fieldRow(t("veCropY"), numberInput(op.rect.y, 0, VE.h, 2, setRect("y"))),
      fieldRow(t("veCropW"), numberInput(op.rect.w, 2, VE.w, 2, setRect("w"))),
      fieldRow(t("veCropH"), numberInput(op.rect.h, 2, VE.h, 2, setRect("h"))));
    box.appendChild(grid);
  }

  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "ghost-btn ve-wide";
  remove.textContent = t("veOvRemove");
  remove.addEventListener("click", () => { const id = op.id; VE.selOv = null; removeOps((o) => o.id === id); });
  box.appendChild(remove);
}

// ---------------------------------------------------------------- filter graph

// A path inside a filter option passes two parsing levels, and only one spelling
// survives both: wrapped in single quotes with exactly ONE backslash before the
// drive colon. Checked against the shipped ffmpeg build — unquoted, unescaped and
// double-escaped all fail with "Invalid argument". The caller supplies the quotes.
function ffPath(path) {
  return String(path).replace(/\\/g, "/").replace(/:/g, "\\:");
}

// The same two levels for drawtext's own text, plus one thing the parser simply
// cannot express: an ASCII apostrophe is silently dropped by every escape form
// ffmpeg accepts (the documented quote-escape-requote spelling renders "its", not
// "it's"). It becomes the typographic apostrophe instead, which is the character
// a caption wants anyway, rather than disappearing from the finished video.
// Backslashes are doubled before colons are escaped, or the escape's own
// backslash would be doubled in turn.
function ffText(text) {
  return String(text)
    .replace(/\r?\n/g, " ")
    .replace(/'/g, "’")
    .replace(/\\/g, "\\\\")
    .replace(/:/g, "\\:");
}

const secs = (ms) => (ms / 1000).toFixed(3);
const hexToFf = (hex, opacity) => `0x${String(hex).replace("#", "")}@${(clamp(opacity ?? 100, 0, 100) / 100).toFixed(2)}`;

// A stretch of source that survives trim + cuts, expressed on the OUTPUT
// timeline, which is what `enable=between(t,…)` reads.
function enableExpr(op) {
  const a = outMs(clamp(op.startMs, 0, VE.dur)) / 1000;
  const b = outMs(clamp(op.endMs, 0, VE.dur)) / 1000;
  if (b <= a) return null;
  return `enable='between(t,${a.toFixed(3)},${b.toFixed(3)})'`;
}

// atempo only accepts 0.5–2.0 per stage, so a larger factor is split across
// several stages that multiply back to the requested rate.
function atempoChain(rate) {
  const stages = [];
  let remaining = rate;
  while (remaining > 2.0001) { stages.push(2); remaining /= 2; }
  while (remaining < 0.4999) { stages.push(0.5); remaining *= 2; }
  stages.push(Number(remaining.toFixed(6)));
  return stages.map((s) => `atempo=${s}`).join(",");
}

const exportFormat = () => $("ve-export-format").value || "mp4";
const exportQuality = () => $("ve-export-quality").value || "high";
const exportDownscale = () => $("ve-export-downscale").value || "";

// The one place the operation list becomes ffmpeg. Produces a complete graph
// consuming [0:v] and ending on [vout]; the audio half is built separately below
// and ends on [aout], so the backend joins the two with ";" into a single
// -filter_complex and maps [vout]/[aout].
function buildVideoChain() {
  const segments = keptSegments();
  const crop = opOf("crop");
  const speed = opOf("speed");
  const fade = opOf("fade");
  const scaleOp = opOf("scale");
  const downscale = exportDownscale() ? Number(exportDownscale()) : (scaleOp ? scaleOp.height : 0);
  const overlays = opsOf("overlay");
  const gif = exportFormat() === "gif";
  const trimmed = segments.length > 1 || segments[0].a > 0 || segments[0].b < VE.dur;
  if (!trimmed && !crop && !speed && !fade && !downscale && !overlays.length && !gif) return null;

  const parts = [];
  let counter = 0;
  const label = () => `v${counter++}`;
  let cur = label();

  // fps-normalize first: a screen capture is variable frame rate, and trimming
  // or concatenating a VFR stream drifts against its own audio.
  const fps = VE.fps > 0 ? Number(VE.fps.toFixed(3)) : 30;
  if (segments.length > 1) {
    const splits = segments.map(() => label());
    parts.push(`[0:v]fps=${fps},split=${segments.length}${splits.map((l) => `[${l}]`).join("")}`);
    const trims = segments.map((seg, i) => {
      const out = label();
      parts.push(`[${splits[i]}]trim=start=${secs(seg.a)}:end=${secs(seg.b)},setpts=PTS-STARTPTS[${out}]`);
      return out;
    });
    parts.push(`${trims.map((l) => `[${l}]`).join("")}concat=n=${segments.length}:v=1:a=0[${cur}]`);
  } else {
    parts.push(`[0:v]fps=${fps},trim=start=${secs(segments[0].a)}:end=${secs(segments[0].b)},setpts=PTS-STARTPTS[${cur}]`);
  }

  const chain = (filter) => {
    const out = label();
    parts.push(`[${cur}]${filter}[${out}]`);
    cur = out;
  };

  // Speed comes before the overlays because their `enable` ranges are already
  // expressed on the sped-up output timeline. The crop comes AFTER them: every
  // overlay position is a coordinate in the SOURCE frame, which is the frame the
  // preview shows them on — cropping first would shift each one by the crop
  // offset and push anything near an edge out of the picture entirely.
  if (speed && Math.abs(speed.rate - 1) > 0.001) chain(`setpts=PTS/${speed.rate}`);

  for (const op of overlays) {
    const enable = enableExpr(op);
    if (!enable) continue;
    if (op.kind === "image") {
      const src = label(), scaled = label(), out = label();
      parts.push(`movie='${ffPath(op.path)}'[${src}]`);
      parts.push(`[${src}]scale=${op.rect.w}:${op.rect.h},format=rgba,colorchannelmixer=aa=${(clamp(op.opacity, 5, 100) / 100).toFixed(2)}[${scaled}]`);
      parts.push(`[${cur}][${scaled}]overlay=${op.rect.x}:${op.rect.y}:${enable}[${out}]`);
      cur = out;
    } else if (op.kind === "frame") {
      const color = hexToFf(op.color, op.opacity);
      chain(`drawbox=x=${op.rect.x}:y=${op.rect.y}:w=${op.rect.w}:h=${op.rect.h}:color=${color}:t=${op.thickness}:${enable}`);
      if (op.style === "double") {
        const gap = op.thickness * 2;
        chain(`drawbox=x=${op.rect.x + gap}:y=${op.rect.y + gap}:w=${Math.max(2, op.rect.w - 2 * gap)}:h=${Math.max(2, op.rect.h - 2 * gap)}:color=${color}:t=${op.thickness}:${enable}`);
      } else if (op.style === "soft") {
        chain(`drawbox=x=${op.rect.x}:y=${op.rect.y}:w=${op.rect.w}:h=${op.rect.h}:color=${hexToFf(op.color, (op.opacity || 100) * 0.4)}:t=${op.thickness * 3}:${enable}`);
      }
    } else if (op.kind === "text") {
      const font = FONT_FILES.find(([id]) => id === op.font) || FONT_FILES[0];
      const plate = op.plate !== false ? `:box=1:boxcolor=black@0.5:boxborderw=${Math.max(4, Math.round(op.fontPx / 4))}` : "";
      // expansion=none keeps a caption containing % or {} literal instead of
      // running it through drawtext's own text expansion.
      chain(`drawtext=fontfile='${ffPath(`${fontsDir}/${font[1]}`)}':x=${op.x}:y=${op.y}:fontsize=${op.fontPx}:fontcolor=${hexToFf(op.color, 100)}:expansion=none${plate}:${enable}:text='${ffText(op.text)}'`);
    } else if (op.kind === "pixelate") {
      // Sampled down and blown back up with nearest-neighbour: a real mosaic,
      // not a blur that a sharpening pass could partly undo.
      const a = label(), b = label(), small = label(), out = label();
      parts.push(`[${cur}]split[${a}][${b}]`);
      parts.push(`[${b}]crop=${op.rect.w}:${op.rect.h}:${op.rect.x}:${op.rect.y},scale=${Math.max(1, Math.round(op.rect.w / op.blockPx))}:${Math.max(1, Math.round(op.rect.h / op.blockPx))},scale=${op.rect.w}:${op.rect.h}:flags=neighbor[${small}]`);
      parts.push(`[${a}][${small}]overlay=${op.rect.x}:${op.rect.y}:${enable}[${out}]`);
      cur = out;
    } else if (op.kind === "spotlight") {
      const a = label(), b = label(), clear = label(), dark = label(), out = label();
      parts.push(`[${cur}]split[${a}][${b}]`);
      parts.push(`[${b}]crop=${op.rect.w}:${op.rect.h}:${op.rect.x}:${op.rect.y}[${clear}]`);
      parts.push(`[${a}]drawbox=x=0:y=0:w=iw:h=ih:color=black@${op.dim.toFixed(2)}:t=fill:${enable}[${dark}]`);
      parts.push(`[${dark}][${clear}]overlay=${op.rect.x}:${op.rect.y}:${enable}[${out}]`);
      cur = out;
    }
  }

  if (crop) chain(`crop=${crop.rect.w}:${crop.rect.h}:${crop.rect.x}:${crop.rect.y}`);

  if (fade) {
    const total = totalOutMs() / 1000;
    if (fade.inMs > 0) chain(`fade=t=in:st=0:d=${(fade.inMs / 1000).toFixed(3)}`);
    if (fade.outMs > 0) chain(`fade=t=out:st=${Math.max(0, total - fade.outMs / 1000).toFixed(3)}:d=${(fade.outMs / 1000).toFixed(3)}`);
  }

  // A target at or above the source's own height must leave the
  // frame untouched rather than upscale it -- picking e.g. 1080p for a 720p
  // clip is "don't shrink it", not "blow it up and waste bits doing so".
  if (downscale && (!VE.h || downscale < VE.h)) chain(`scale=-2:${downscale}`);

  if (gif) {
    // Palette generation and use in the SAME graph — two ffmpeg runs would mean
    // an intermediate file for nothing.
    const g0 = label(), g1 = label(), pal = label();
    parts.push(`[${cur}]fps=15,split[${g0}][${g1}]`);
    parts.push(`[${g0}]palettegen=stats_mode=diff[${pal}]`);
    parts.push(`[${g1}][${pal}]paletteuse=dither=bayer[vout]`);
    return parts.join(";");
  }

  parts.push(`[${cur}]null[vout]`);
  return parts.join(";");
}

// Audio half of the same graph: per track trim/cut/concat, volume, speed and
// fades, mixed down to one stream on [aout].
function buildAudioChain() {
  if (exportFormat() === "gif") return null;
  const tracks = [];
  if (VE.hasSys) tracks.push({ index: 0, spec: "0:a:0", state: VE.audio.sys });
  if (VE.hasMic) tracks.push({ index: 1, spec: "0:a:1", state: VE.audio.mic });
  if (!tracks.length) return null;

  const segments = keptSegments();
  const speed = opOf("speed");
  const fade = opOf("fade");
  const rate = speed ? speed.rate : 1;
  const trimmed = segments.length > 1 || segments[0].a > 0 || segments[0].b < VE.dur;
  const touched = trimmed || Math.abs(rate - 1) > 0.001 || !!fade
    || tracks.some((tr) => tr.state.mute || tr.state.vol !== 100) || tracks.length > 1;
  if (!touched) return null;

  const parts = [];
  let counter = 0;
  const label = () => `a${counter++}`;
  const outs = [];

  for (const track of tracks) {
    let cur = label();
    if (segments.length > 1) {
      const splits = segments.map(() => label());
      parts.push(`[${track.spec}]asplit=${segments.length}${splits.map((l) => `[${l}]`).join("")}`);
      const trims = segments.map((seg, i) => {
        const out = label();
        parts.push(`[${splits[i]}]atrim=start=${secs(seg.a)}:end=${secs(seg.b)},asetpts=PTS-STARTPTS[${out}]`);
        return out;
      });
      parts.push(`${trims.map((l) => `[${l}]`).join("")}concat=n=${segments.length}:v=0:a=1[${cur}]`);
    } else {
      parts.push(`[${track.spec}]atrim=start=${secs(segments[0].a)}:end=${secs(segments[0].b)},asetpts=PTS-STARTPTS[${cur}]`);
    }
    const chain = (filter) => {
      const out = label();
      parts.push(`[${cur}]${filter}[${out}]`);
      cur = out;
    };
    const volume = track.state.mute ? 0 : clamp(track.state.vol, 0, 200) / 100;
    chain(`volume=${volume.toFixed(2)}`);
    if (Math.abs(rate - 1) > 0.001) {
      // Keeping the pitch is atempo's job; letting it move is what a plain
      // sample-rate change does, which is the "chipmunk" result some clips want.
      chain(speed && speed.pitch === false ? `asetrate=48000*${rate},aresample=48000` : atempoChain(rate));
    }
    if (fade) {
      const total = totalOutMs() / 1000;
      if (fade.inMs > 0) chain(`afade=t=in:st=0:d=${(fade.inMs / 1000).toFixed(3)}`);
      if (fade.outMs > 0) chain(`afade=t=out:st=${Math.max(0, total - fade.outMs / 1000).toFixed(3)}:d=${(fade.outMs / 1000).toFixed(3)}`);
    }
    outs.push(cur);
  }

  if (outs.length > 1) parts.push(`${outs.map((l) => `[${l}]`).join("")}amix=inputs=${outs.length}:normalize=0[aout]`);
  else parts.push(`[${outs[0]}]anull[aout]`);
  return parts.join(";");
}

// A pure trim with nothing else touched can be copied instead of re-encoded —
// instant, and bit-identical to the source.
function isLosslessCopy() {
  if (exportFormat() !== "mp4") return false;
  if (exportQuality() === "lossless") return false; // an explicit re-encode was asked for
  // A source whose own streams are not already mp4-safe (h264/hevc video,
  // aac-or-silent audio -- e.g. a webm/VP9 file dragged in from elsewhere)
  // fails ffmpeg's "-c copy" outright instead of producing a trimmed mp4.
  if (VE.probe && VE.probe.stream_copy_safe === false) return false;
  if (exportDownscale()) return false;
  if (opsOf("cut").length) return false;
  if (opOf("crop") || opOf("speed") || opOf("fade") || opOf("scale") || opsOf("overlay").length) return false;
  const { sys, mic } = VE.audio;
  return !sys.mute && sys.vol === 100 && !mic.mute && mic.vol === 100;
}

// ---------------------------------------------------------------- export

// Three ways to land on a destination, checked in order -- a full path
// the user typed or picked via "Save as…" wins outright; otherwise a
// remembered "Export to…" folder keeps the auto filename but moves it there;
// otherwise the original stem-based guess next to the source file.
function destPath() {
  const format = exportFormat();
  const cut = VE.src.replace(/[/\\]+$/, "");
  const dot = cut.lastIndexOf(".");
  const stem = dot > cut.lastIndexOf("\\") && dot > cut.lastIndexOf("/") ? cut.slice(0, dot) : cut;
  const auto = `${stem}-edit.${format}`;
  if (VE.destOverride) return VE.destOverride;
  if (VE.destDir) {
    const base = auto.replace(/^.*[\\/]/, "");
    return `${VE.destDir.replace(/[\\/]+$/, "")}/${base}`;
  }
  return auto;
}

function updateExportNote() {
  const format = exportFormat();
  const notes = [];
  if (format === "gif") notes.push(t("veGifNote"));
  if (isLosslessCopy()) notes.push(t("veCopyNote"));
  notes.push(t("veOutLength").replace("{d}", fmtTime(totalOutMs())));
  $("ve-export-note").textContent = notes.join(" ");
  // Never overwrite what the user is actively typing into the field itself.
  if (document.activeElement !== $("ve-export-dest")) $("ve-export-dest").value = destPath();
  $("ve-export-dest").title = destPath();
  $("ve-export-quality").disabled = format === "gif";
}

function setProgress(pct) {
  const row = $("ve-export-progress-row");
  row.classList.remove("hidden");
  $("ve-export-progress").value = clamp(pct, 0, 100);
  $("ve-export-pct").textContent = `${Math.round(clamp(pct, 0, 100))}%`;
}

// toClipboard runs the very same export into a throwaway temp file
// instead of the normal destination, then puts that file on the clipboard for
// pasting elsewhere -- see clip_export_path/sweep_clip_export_temp on the
// Rust side for where and when that temp file goes away.
async function startExport(toClipboard) {
  if (VE.exporting) return;
  if (!VE.ffmpeg) { toast(t("recFfmpegMissingHint")); return; }
  const total = totalOutMs();
  if (total < 1) { toast(t("veNothingToExport")); return; }
  const lossless = isLosslessCopy();
  const format = exportFormat();
  const dest = toClipboard
    ? await invoke("clip_export_path", { ext: format }).catch((err) => { toast(String(err)); return null; })
    : destPath();
  if (!dest) return;
  const request = {
    src_path: VE.src,
    dest_path: dest,
    filter_chain: lossless ? null : buildVideoChain(),
    audio_map: lossless ? null : buildAudioChain(),
    lossless_copy: lossless,
    // Distinct from lossless_copy -- this is the "Lossless" QUALITY tier,
    // which forces a real re-encode (see isLosslessCopy above) that must
    // itself stay lossless (-crf 0) rather than falling back to an ordinary
    // bitrate target with nothing to map "lossless" to.
    lossless_quality: exportQuality() === "lossless",
    format,
    video_kbps: lossless ? null : videoKbps(),
    // Only meaningful on the stream-copy path, where there is no filter graph
    // that could carry the trim itself.
    trim_start_ms: lossless ? Math.round(keptSegments()[0].a) : null,
    trim_end_ms: lossless ? Math.round(keptSegments().slice(-1)[0].b) : null,
  };
  VE.exporting = true;
  $("ve-export-start").disabled = true;
  $("ve-export-clip").disabled = true;
  $("ve-export-cancel").classList.remove("hidden");
  $("ve-export-saveas").classList.add("hidden");
  $("ve-export-status").textContent = t("veExporting");
  setProgress(0);
  try {
    await invoke("export_video", { req: request });
    VE.dirty = false;
    setProgress(100);
    if (toClipboard) {
      const clipOk = await invoke("copy_file_to_clipboard", { path: dest }).catch(() => false);
      $("ve-export-status").textContent = clipOk ? t("veClipExportDone") : t("veExportDone").replace("{path}", dest);
      toast(clipOk ? t("veClipExportDone") : t("veExportDone").replace("{path}", dest));
    } else {
      VE.lastExport = dest;
      $("ve-export-status").textContent = t("veExportDone").replace("{path}", dest);
      $("ve-export-saveas").classList.remove("hidden");
      toast(t("veExportDone").replace("{path}", dest));
    }
  } catch (err) {
    const message = String(err);
    noteFfmpeg(err);
    $("ve-export-status").textContent = message.includes("cancel") ? t("veExportCancelled") : message;
  } finally {
    VE.exporting = false;
    $("ve-export-start").disabled = false;
    $("ve-export-clip").disabled = false;
    $("ve-export-cancel").classList.add("hidden");
  }
}

function videoKbps() {
  const base = EXPORT_KBPS[exportQuality()];
  if (base == null) return null;
  const target = exportDownscale() ? Number(exportDownscale()) : 0;
  if (!target || !VE.h || target >= VE.h) return base;
  // Fewer pixels need fewer bits; the floor keeps a 480p clip from going muddy.
  return Math.max(800, Math.round(base * (target / VE.h)));
}

// ---------------------------------------------------------------- ffmpeg presence

function noteFfmpeg(err) {
  if (!String(err).includes("ffmpeg-missing")) return;
  VE.ffmpeg = false;
  $("ve-ffmpeg-warn").classList.remove("hidden");
  $("ve-export-start").disabled = true;
  $("ve-export-clip").disabled = true;
  $("ve-still").disabled = true;
}

$("ve-ffmpeg-get").addEventListener("click", async (e) => {
  const btn = e.currentTarget;
  btn.disabled = true;
  const unlisten = await listen("ffmpeg-download-progress", (ev) => {
    const pct = ffmpegDownloadPct(ev.payload);
    btn.textContent = t("ffmpegDownloading").replace("{pct}", String(pct));
  });
  try {
    await invoke("download_ffmpeg");
    VE.ffmpeg = true;
    $("ve-ffmpeg-warn").classList.add("hidden");
    $("ve-export-start").disabled = false;
    $("ve-export-clip").disabled = false;
    $("ve-still").disabled = false;
    toast(t("ffmpegDownloadDone"));
    await loadRecording(VE.src);
  } catch (err) {
    toast(String(err));
  } finally {
    btn.disabled = false;
    btn.textContent = t("ffmpegOfferConfirm");
    unlisten();
  }
});

// ---------------------------------------------------------------- panels + wiring

function showPanel(id) {
  for (const panel of document.querySelectorAll(".ve-panel")) panel.classList.toggle("hidden", panel.id !== id);
  for (const tab of document.querySelectorAll(".ve-tab")) {
    const active = tab.dataset.panel === id;
    tab.classList.toggle("active", active);
    tab.setAttribute("aria-selected", active ? "true" : "false");
  }
}

for (const tab of document.querySelectorAll(".ve-tab")) {
  tab.addEventListener("click", () => showPanel(tab.dataset.panel));
}

$("ve-close").addEventListener("click", () => closeWindow());
$("ve-undo").addEventListener("click", undo);
$("ve-redo").addEventListener("click", redo);
$("ve-play").addEventListener("click", togglePlay);
$("ve-to-start").addEventListener("click", () => seek(keptSegments()[0].a));
$("ve-to-end").addEventListener("click", () => seek(keptSegments().slice(-1)[0].b));
// Stepping by a fixed frameMs() from an arbitrary (scrubbed) playhead
// position drifts further from the real frame grid every press instead of
// landing on it -- snap to the actual previous/next frame boundary instead.
// The small epsilon absorbs float error from a non-integer frame duration
// (e.g. 29.97fps) so a playhead already sitting exactly on a boundary still
// advances by one whole frame rather than getting stuck restating it.
function stepFrame(dir) {
  const fm = frameMs();
  const eps = fm / 100;
  const idx = dir > 0 ? Math.floor((VE.playhead + eps) / fm) + 1 : Math.ceil((VE.playhead - eps) / fm) - 1;
  seek(Math.max(0, idx * fm));
}
$("ve-prev-frame").addEventListener("click", () => stepFrame(-1));
$("ve-next-frame").addEventListener("click", () => stepFrame(1));

$("ve-mark-in").addEventListener("click", () => {
  const b = VE.sel && VE.sel.b > VE.playhead ? VE.sel.b : VE.dur;
  VE.sel = { a: VE.playhead, b };
  renderRegions();
});
$("ve-mark-out").addEventListener("click", () => {
  const a = VE.sel && VE.sel.a < VE.playhead ? VE.sel.a : 0;
  VE.sel = { a, b: VE.playhead };
  renderRegions();
});
$("ve-sel-clear").addEventListener("click", () => { VE.sel = null; renderRegions(); renderOverlayEditor(); });
$("ve-trim-set").addEventListener("click", () => {
  if (!VE.sel) { toast(t("veNeedSelection")); return; }
  setTrim(VE.sel.a, VE.sel.b);
});
$("ve-cut-add").addEventListener("click", () => {
  if (!VE.sel || Math.abs(VE.sel.b - VE.sel.a) < MIN_CLIP_MS) { toast(t("veNeedSelection")); return; }
  pushOp({ tool: "cut", startMs: Math.min(VE.sel.a, VE.sel.b), endMs: Math.max(VE.sel.a, VE.sel.b) });
  VE.sel = null;
});
$("ve-cut-clear").addEventListener("click", () => removeOps((o) => o.tool === "cut"));

$("ve-still").addEventListener("click", async () => {
  try {
    const saved = await invoke("extract_still_frame", { path: VE.src, atMs: Math.round(VE.playhead) });
    toast(saved ? t("veStillSaved").replace("{path}", String(saved)) : t("veStillSaved").replace("{path}", ""));
  } catch (err) {
    noteFfmpeg(err);
    toast(String(err));
  }
});

$("ve-zoom-in").addEventListener("click", () => setZoom(VE.zoom * 1.4));
$("ve-zoom-out").addEventListener("click", () => setZoom(VE.zoom / 1.4));
$("ve-zoom-fit").addEventListener("click", () => setZoom(fitZoom()));

$("ve-trim-start-val").addEventListener("change", () => {
  const ms = parseTime($("ve-trim-start-val").value);
  const trim = opOf("trim");
  if (ms == null) { renderEditFields(); return; }
  setTrim(ms, trim ? trim.endMs : VE.dur);
});
$("ve-trim-end-val").addEventListener("change", () => {
  const ms = parseTime($("ve-trim-end-val").value);
  const trim = opOf("trim");
  if (ms == null) { renderEditFields(); return; }
  setTrim(trim ? trim.startMs : 0, ms);
});

$("ve-crop-on").addEventListener("change", () => {
  if ($("ve-crop-on").checked) pushReplacing((o) => o.tool === "crop", { tool: "crop", rect: centeredRect(0.8) });
  else removeOps((o) => o.tool === "crop");
});
$("ve-crop-reset").addEventListener("click", () => {
  pushReplacing((o) => o.tool === "crop", { tool: "crop", rect: snapRect({ x: 0, y: 0, w: VE.w, h: VE.h }) });
});
for (const id of ["ve-crop-x", "ve-crop-y", "ve-crop-w", "ve-crop-h"]) {
  $(id).addEventListener("change", () => {
    const rect = snapRect({
      x: Number($("ve-crop-x").value) || 0,
      y: Number($("ve-crop-y").value) || 0,
      w: Number($("ve-crop-w").value) || VE.w,
      h: Number($("ve-crop-h").value) || VE.h,
    });
    pushReplacing((o) => o.tool === "crop", { tool: "crop", rect });
  });
}

$("ve-speed-custom").addEventListener("change", () => setSpeed(Number($("ve-speed-custom").value)));
$("ve-speed-pitch").addEventListener("change", () => {
  const speed = opOf("speed");
  if (speed) pushReplacing((o) => o.tool === "speed", { ...speed, pitch: $("ve-speed-pitch").checked });
});

$("ve-fade-on").addEventListener("change", () => {
  if ($("ve-fade-on").checked) {
    const fadeDef = VE.fadeMsDefault || 500;
    pushReplacing((o) => o.tool === "fade", {
      tool: "fade",
      inMs: clamp(Number($("ve-fade-in").value) || fadeDef, 0, 10000),
      outMs: clamp(Number($("ve-fade-out").value) || fadeDef, 0, 10000),
    });
  } else {
    removeOps((o) => o.tool === "fade");
  }
});
for (const id of ["ve-fade-in", "ve-fade-out"]) {
  $(id).addEventListener("change", () => {
    if (!opOf("fade")) return;
    pushReplacing((o) => o.tool === "fade", {
      tool: "fade",
      inMs: clamp(Number($("ve-fade-in").value) || 0, 0, 10000),
      outMs: clamp(Number($("ve-fade-out").value) || 0, 0, 10000),
    });
  });
}

// The edit tab's own resize control and the export tab's downscale
// dropdown are two faces of the one same setting -- only setting a control's
// .value from code, as each side's handler used to do to the OTHER control,
// never fires that control's own "change" event, so only whichever one the
// user actually touched ever updated the real "scale" op. Picking "Original"
// in the export tab only ever reset the dropdown's own display; the stale
// scale op from an earlier edit-tab resize stayed in VE.ops and still applied.
function setScaleOp(value) {
  if (!value) removeOps((o) => o.tool === "scale");
  else pushReplacing((o) => o.tool === "scale", { tool: "scale", height: Number(value) });
}

$("ve-scale").addEventListener("change", () => {
  const value = $("ve-scale").value;
  setScaleOp(value);
  $("ve-export-downscale").value = value;
  persistExportSettings();
  updateExportNote();
});

$("ve-ov-add-image").addEventListener("click", () => addOverlay("image"));
$("ve-ov-add-frame").addEventListener("click", () => addOverlay("frame"));
$("ve-ov-add-text").addEventListener("click", () => addOverlay("text"));
$("ve-ov-add-pixelate").addEventListener("click", () => addOverlay("pixelate"));
$("ve-ov-add-spotlight").addEventListener("click", () => addOverlay("spotlight"));

// A recording without a microphone track leaves those two controls switched off
// and says why, rather than offering a slider that could not change anything.
function updateAudioLabels() {
  for (const [id, key] of [["ve-mute-mic", "veMuteMic"], ["ve-vol-mic", "veVolumeMic"]]) {
    const el = $(id);
    el.disabled = !VE.hasMic;
    const label = VE.hasMic ? t(key) : t("veNoMicTrack");
    el.title = label;
    el.setAttribute("aria-label", label);
  }
  for (const [id, key] of [["ve-mute-sys", "veMuteSystem"], ["ve-vol-sys", "veVolumeSystem"]]) {
    const el = $(id);
    el.disabled = !VE.hasSys;
    const label = VE.hasSys ? t(key) : t("veNoAudioTrack");
    el.title = label;
    el.setAttribute("aria-label", label);
  }
}

// Reflects VE.audio in the two mute buttons/sliders -- needed on every load
// since resetForNewSource() changes the state without any click happening to
// trigger audioControl()'s own listener.
function syncAudioButtons() {
  $("ve-mute-sys").classList.toggle("ve-muted-btn", VE.audio.sys.mute);
  $("ve-mute-mic").classList.toggle("ve-muted-btn", VE.audio.mic.mute);
  $("ve-vol-sys").value = VE.audio.sys.vol;
  $("ve-vol-mic").value = VE.audio.mic.vol;
}

function audioControl(kind, muteId, volId) {
  $(muteId).addEventListener("click", () => {
    VE.audio[kind].mute = !VE.audio[kind].mute;
    $(muteId).classList.toggle("ve-muted-btn", VE.audio[kind].mute);
    applyPreviewSpeed();
    updateExportNote();
  });
  $(volId).addEventListener("input", () => {
    VE.audio[kind].vol = Number($(volId).value);
    applyPreviewSpeed();
    updateExportNote();
  });
}
audioControl("sys", "ve-mute-sys", "ve-vol-sys");
audioControl("mic", "ve-mute-mic", "ve-vol-mic");

for (const id of ["ve-export-format", "ve-export-quality", "ve-export-downscale"]) {
  $(id).addEventListener("change", () => {
    if (id === "ve-export-downscale") {
      const value = $("ve-export-downscale").value;
      $("ve-scale").value = value;
      setScaleOp(value);
    }
    // A full manual destination names its own extension -- picking a
    // different container format makes that extension wrong, so the format
    // dropdown is the one control that drops back to the auto-named file
    // (still inside a remembered "Export to…" folder, if any).
    if (id === "ve-export-format") VE.destOverride = null;
    persistExportSettings();
    updateExportNote();
  });
}

// The target field is a real input now -- typing a full path directly
// is itself a manual override, same as picking one via the dialog below.
$("ve-export-dest").addEventListener("change", () => {
  const value = $("ve-export-dest").value.trim();
  VE.destOverride = value || null;
  updateExportNote();
});
$("ve-export-choose-file").addEventListener("click", async () => {
  const defaultName = destPath().replace(/^.*[\\/]/, "");
  const picked = await invoke("pick_export_dest", { defaultName }).catch(() => null);
  if (!picked) return;
  VE.destOverride = picked;
  updateExportNote();
});
$("ve-export-choose-folder").addEventListener("click", async () => {
  const folder = await invoke("pick_folder").catch(() => null);
  if (!folder) return;
  VE.destDir = folder;
  VE.destOverride = null; // the auto filename moves into the new folder
  persistExportSettings();
  updateExportNote();
});

function applyPreset(format, quality, downscale) {
  $("ve-export-format").value = format;
  $("ve-export-quality").value = quality;
  $("ve-export-downscale").value = downscale;
  $("ve-scale").value = downscale;
  setScaleOp(downscale);
  persistExportSettings();
  updateExportNote();
}
// Presets only fill the manual controls — there is no second export path.
$("ve-preset-same").addEventListener("click", () => applyPreset("mp4", "high", ""));
$("ve-preset-small").addEventListener("click", () => applyPreset("mp4", "small", "720"));
$("ve-preset-gif").addEventListener("click", () => applyPreset("gif", "small", "480"));

$("ve-export-start").addEventListener("click", () => startExport(false));
$("ve-export-clip").addEventListener("click", () => startExport(true));
$("ve-export-cancel").addEventListener("click", () => invoke("cancel_export").catch(() => {}));
$("ve-export-saveas").addEventListener("click", async () => {
  if (!VE.lastExport) return;
  const moved = await invoke("save_recording_as", { srcPath: VE.lastExport }).catch((err) => { toast(String(err)); return null; });
  if (moved) {
    VE.lastExport = moved;
    $("ve-export-status").textContent = t("veExportDone").replace("{path}", moved);
  }
});

function persistExportSettings() {
  invoke("set_ui_text", { key: "videoExportFormat", value: exportFormat() }).catch(() => {});
  invoke("set_ui_text", { key: "videoExportQuality", value: exportQuality() }).catch(() => {});
  invoke("set_ui_text", { key: "videoExportDownscale", value: exportDownscale() }).catch(() => {});
  invoke("set_ui_text", { key: "videoExportDir", value: VE.destDir }).catch(() => {});
}

// ---------------------------------------------------------------- keyboard

async function closeWindow() {
  if (VE.exporting) {
    const stop = await veConfirm(t("veCloseTitle"), t("veCloseExporting"), t("veDiscard"));
    if (!stop) return;
    invoke("cancel_export").catch(() => {});
  } else if (VE.ops.length && VE.dirty) {
    const ok = await veConfirm(t("veCloseTitle"), t("veCloseUnsaved"), t("veDiscard"));
    if (!ok) return;
  }
  appWin.close().catch(() => {});
}

const TYPING = new Set(["INPUT", "TEXTAREA", "SELECT"]);

document.addEventListener("keydown", (e) => {
  if (confirmState) {
    if (e.key === "Escape") { e.preventDefault(); closeConfirm(false); }
    else if (e.key === "Tab") trapTabWithin($("ve-confirm"), e);
    else if (e.key === "Enter") { e.preventDefault(); closeConfirm(true); }
    return;
  }
  const typing = TYPING.has(document.activeElement?.tagName);
  if (e.key === "Escape") { e.preventDefault(); closeWindow(); return; }
  if (typing) return;

  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
    e.preventDefault();
    if (e.shiftKey) redo(); else undo();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "y") { e.preventDefault(); redo(); return; }
  if (e.ctrlKey || e.metaKey || e.altKey) return;

  switch (e.key) {
    case " ": e.preventDefault(); togglePlay(); break;
    case "ArrowLeft": e.preventDefault(); if (e.shiftKey) seek(VE.playhead - 1000); else stepFrame(-1); break;
    case "ArrowRight": e.preventDefault(); if (e.shiftKey) seek(VE.playhead + 1000); else stepFrame(1); break;
    case "Home": e.preventDefault(); seek(0); break;
    case "End": e.preventDefault(); seek(VE.dur); break;
    case "i": case "I": e.preventDefault(); $("ve-mark-in").click(); break;
    case "o": case "O": e.preventDefault(); $("ve-mark-out").click(); break;
    case "c": case "C": e.preventDefault(); $("ve-cut-add").click(); break;
    case "+": case "=": e.preventDefault(); setZoom(VE.zoom * 1.4); break;
    case "-": e.preventDefault(); setZoom(VE.zoom / 1.4); break;
    default: break;
  }
});

// ---------------------------------------------------------------- loading

function resetForNewSource() {
  VE.ops = [];
  VE.redo = [];
  VE.floor = 0;
  VE.sel = null;
  VE.selOv = null;
  VE.strip = [];
  VE.stripCount = 0;
  VE.peaks = { 0: null, 1: null };
  VE.playhead = 0;
  VE.dirty = false;
  VE.lastExport = "";
  // A full manual "Save as…" path names THIS source's file, so a newly
  // loaded video starts fresh with it; the remembered destination FOLDER
  // (VE.destDir) is a standing preference and survives across videos.
  VE.destOverride = null;
  // Mic starts muted on every newly loaded video, same reasoning as the
  // module-level default above.
  VE.audio = { sys: { mute: false, vol: 100 }, mic: { mute: true, vol: 100 } };
  syncAudioButtons();
  // Probe/geometry were only ever set by applyProbe() on success, never
  // cleared here -- if probing the NEW source then fails (no ffmpeg, a
  // corrupt file, ...), applyProbe(null) skips its own assignment entirely
  // and every one of these silently kept the PREVIOUS video's values: wrong
  // crop bounds, wrong duration/frame math, wrong audio-track availability,
  // all attributed to a source they no longer describe.
  VE.probe = null;
  VE.dur = 0;
  VE.fps = 30;
  VE.w = 0;
  VE.h = 0;
  VE.hasSys = true;
  VE.hasMic = false;
  // A leftover zoom/pan from the previous video would otherwise apply to a
  // brand-new source of a different resolution/aspect ratio, showing the
  // wrong region of the picture (or none of it) as soon as it loads.
  VE.previewZoom = 1;
  VE.panPx = { x: 0, y: 0 };
  applyPreviewTransform();
  $("ve-export-status").textContent = "";
  $("ve-export-progress-row").classList.add("hidden");
  $("ve-export-saveas").classList.add("hidden");
}

async function loadRecording(path) {
  if (!path) return;
  // Two "Edit video" clicks in quick succession (this recording, then
  // another before the first has finished probing/muxing) had no way to tell
  // a stale call's own late result apart from the current one -- whichever
  // async chain happened to finish LAST won, even if it was the older, no
  // longer relevant one. Every await below re-checks this before touching
  // any shared state.
  const myGen = ++VE.loadGen;
  VE.src = path;
  resetForNewSource();
  $("ve-file").textContent = path.split(/[\\/]/).pop() || "";
  $("ve-file").title = path;
  $("ve-stage-msg").classList.remove("hidden");
  videoEl.src = convertFileSrc(path);
  videoEl.load();

  let probe = null;
  try {
    probe = await invoke("probe_recording", { path });
  } catch (err) {
    noteFfmpeg(err);
  }
  if (VE.loadGen !== myGen) return;

  // A mic recording arrives as a sidecar WAV next to the mp4; the editor
  // consolidates the two the first time it opens that recording, so from then on
  // there is one file with two audio tracks. Failing here is not fatal — only the
  // mic controls stay off.
  if (probe && probe.mic_sidecar) {
    try {
      await invoke("mux_mic_track", { videoPath: path, sidecarPath: probe.mic_sidecar });
      if (VE.loadGen !== myGen) return;
      probe = await invoke("probe_recording", { path });
      if (VE.loadGen !== myGen) return;
      videoEl.load();
    } catch (err) {
      noteFfmpeg(err);
    }
  }
  if (VE.loadGen !== myGen) return;

  applyProbe(probe);
  $("ve-stage-msg").classList.add("hidden");
  setZoom(fitZoom());
  afterOpsChanged();
  loadStrip();
  scheduleWaves();
}

function applyProbe(probe) {
  VE.probe = probe;
  if (probe) {
    VE.dur = Number(probe.duration_ms) || 0;
    VE.fps = Number(probe.fps) > 0 ? Number(probe.fps) : 30;
    VE.w = Number(probe.width) || 0;
    VE.h = Number(probe.height) || 0;
    VE.hasMic = !!probe.has_mic_track;
    VE.hasSys = probe.has_audio !== false;
  }
  // Without ffmpeg the clip still plays, so the element itself supplies what it
  // knows; the frame counter then follows a nominal rate rather than the real one.
  if (!VE.dur && Number.isFinite(videoEl.duration)) VE.dur = Math.round(videoEl.duration * 1000);
  if (!VE.w) VE.w = videoEl.videoWidth || 1920;
  if (!VE.h) VE.h = videoEl.videoHeight || 1080;

  $("ve-duration").textContent = fmtTime(VE.dur);
  $("ve-wave-mic").classList.toggle("hidden", !VE.hasMic);
  updateAudioLabels();
  $("ve-crop-w").max = String(VE.w);
  $("ve-crop-h").max = String(VE.h);
  layoutTrack();
  renderRects();
}

videoEl.addEventListener("loadedmetadata", () => {
  if (!VE.dur) applyProbe(VE.probe);
  renderRects();
});

window.addEventListener("resize", () => {
  // A pan saved against the old stage size can land outside the new one once
  // the window shrinks (fitBoxSize/clampPan both read the stage's current,
  // live size) -- re-clamp before the next render instead of leaving the
  // picture stuck partly or wholly outside the visible stage.
  if (VE.previewZoom !== 1) { VE.panPx = clampPan(VE.panPx, VE.previewZoom); applyPreviewTransform(); }
  renderRects();
  layoutTrack();
});

// ---------------------------------------------------------------- backend events

listen("theme-changed", (e) => { applyTheme(e.payload?.theme, e.payload?.vars); drawWave(0); if (VE.hasMic) drawWave(1); });
listen("language-changed", () => { invoke("get_settings").then((s) => loadTexts(s)).catch(() => {}); });
listen("settings-changed", () => { invoke("get_settings").then(applySettings).catch(() => {}); });
// Opening a second recording reuses this window instead of stacking another one.
listen("video-editor-load", async (e) => {
  const path = typeof e.payload === "string" ? e.payload : e.payload?.path;
  if (!path || path === VE.src) return;
  // Switching source (clicking "Edit video" on another tile while this
  // window is already open) used to jump straight to loadRecording(), silently
  // discarding whatever was unsaved -- the same case closeWindow() already
  // guards, reusing its exact confirm.
  if (VE.ops.length && VE.dirty) {
    const ok = await veConfirm(t("veCloseTitle"), t("veCloseUnsaved"), t("veDiscard"));
    if (!ok) return;
  }
  loadRecording(path);
});
listen(EXPORT_PROGRESS_EVENT, (e) => {
  if (!VE.exporting) return;
  const ms = Number(e.payload?.ms ?? e.payload ?? 0);
  const total = totalOutMs();
  if (total > 0) setProgress((ms / total) * 100);
});

function applySettings(settings) {
  const undoMax = Number(settings?.ui_values?.videoEditUndoMax);
  VE.undoMax = Number.isFinite(undoMax) && undoMax > 0 ? Math.round(undoMax) : UNDO_MAX_DEFAULT;
  const values = settings?.ui_values || {};
  const texts = settings?.ui_texts || {};
  // The user's own last export choice always wins; only a field that was never
  // touched (no persisted videoExport* value yet) opens on the expert default.
  $("ve-export-format").value = texts.videoExportFormat || texts.veDefaultFormat || "mp4";
  $("ve-export-quality").value = texts.videoExportQuality || texts.veDefaultQuality || "high";
  const downscaleDef = texts.videoExportDownscale != null ? texts.videoExportDownscale : (texts.veDefaultDownscale || "");
  $("ve-export-downscale").value = downscaleDef;
  $("ve-scale").value = downscaleDef;
  VE.destDir = texts.videoExportDir || "";
  const kbpsHigh = Number(values.veExportKbpsHigh);
  const kbpsSmall = Number(values.veExportKbpsSmall);
  EXPORT_KBPS.high = Number.isFinite(kbpsHigh) && kbpsHigh > 0 ? kbpsHigh : 12000;
  EXPORT_KBPS.small = Number.isFinite(kbpsSmall) && kbpsSmall > 0 ? kbpsSmall : 4000;
  const fadeMs = Number(values.veFadeMs);
  VE.fadeMsDefault = Number.isFinite(fadeMs) && fadeMs > 0 ? Math.round(fadeMs) : 500;
  // Only while fade isn't already applied -- once it is, these fields hold the
  // user's own per-export values and a stray settings-changed elsewhere (any
  // setting, anywhere) must never overwrite what they just typed.
  if (!opOf("fade")) {
    $("ve-fade-in").value = VE.fadeMsDefault;
    $("ve-fade-out").value = VE.fadeMsDefault;
  }
  const pixelateBlock = Number(values.veOvPixelateBlock);
  VE.ovPixelateBlockDefault = Number.isFinite(pixelateBlock) && pixelateBlock > 0 ? Math.round(pixelateBlock) : 16;
  const spotlightDim = Number(values.veOvSpotlightDim);
  VE.ovSpotlightDimDefault = Number.isFinite(spotlightDim) && spotlightDim > 0 ? spotlightDim / 100 : 0.6;
  const imageOpacity = Number(values.veOvImageOpacity);
  VE.ovImageOpacityDefault = Number.isFinite(imageOpacity) && imageOpacity > 0 ? Math.round(imageOpacity) : 100;
  updateExportNote();
}

// ---------------------------------------------------------------- boot

(async () => {
  try {
    applyTheme(await invoke("current_theme"), await invoke("theme_vars"));
  } catch (_) {}
  let settings = null;
  try {
    settings = await invoke("get_settings");
    applySettings(settings);
  } catch (_) {}
  loadTexts(settings).catch(() => {});
  renderSpeedSteps();
  // The recording is PULLED, not pushed: a push would race the webview's first
  // frame, which is why the placeholder window does the same. The
  // "video-editor-load" event covers the other case — opening a second recording
  // into a window that is already up.
  let path = "";
  try {
    path = await invoke("video_editor_source");
  } catch (_) {
    // No source command yet, or none set: wait for the event instead of showing
    // an error for what is a normal startup order.
  }
  if (path) await loadRecording(path);
  else $("ve-stage-msg").textContent = t("veNoRecording");
  requestAnimationFrame(tick);
})();
