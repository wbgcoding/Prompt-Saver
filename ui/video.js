// Video editor window. One recording at a time: preview on top, a two-track
// timeline (filmstrip + waveform per audio track) below it, and a side panel
// holding the edit controls, the overlay list and the export settings.
//
// Nothing is ever applied to the file while editing. Every change is an entry in
// an ordered operation list (VE.ops), exactly like the screenshot editor's own
// list, and the export compiles that list into ONE ffmpeg filter graph. That is
// what makes undo/redo cheap, the preview honest and the export a single pass.
import { copyTextChecked, fillVideoFormatSelect, fmtSizeCompact, makeHoldOrTapToggle, normRecFormat, numberField, numPopClosedByEscape, orderedInvoke, resolveLang, runFfmpegDownload, trapTabWithin } from "./floatShared.js";
import { withShortcut as withShortcutRaw, rowsForScope as rowsForScopeRaw, buildShortcutRows, openShortcutsOverlay, closeShortcutsOverlay, letterCode } from "./shortcuts.js";
import { applyFloatTheme } from "./floattheme.js";
import { APP_NAME } from "./brand.js";
import { errMessageLogged } from "./errors.js";
import { LANGS, fullTexts, loadLang } from "./i18n.js";
import { createColorPop } from "./colorpop.js";
import { insertWinButtons, wireWinButtons } from "./winButtons.js";
import { attachSelectPicker } from "./selectpop.js";
import { confirmDialog, dismissConfirm, initDialogs } from "./dialog.js";
import { stripFormatExt } from "./exportName.js";
import { secs, exportFormat, exportQuality, effectiveSize } from "./videoGraph.js";
import { destPath, applyExportGate, updateExportNote, exportRun, setProgress, startExport, videoKbps, noteFfmpeg } from "./videoExport.js";
import { stopEditStateAutosave, armEditStateAutosave, saveEditStateNow, loadRecording, applyProbe } from "./videoLoad.js";
import { markedRange, centeredRect, addAlignedCameraOverlay, addOverlay, ovCtxPop, closeOvCtxPop, addVideoPop, closeAddVideoPop, startAddVideo, renderOverlayList, renderClipList, renderOverlayEditor } from "./videoOverlays.js";
import { videoBox, clampPan, applyPreviewTransform, setPreviewZoom, resetPreviewZoom, snapRect, nudgeSelectedOverlay, renderRects, renderOverlayLive, clearLiveOverlays } from "./videoPreview.js";

// Frameless window: the title is invisible in the app but shows in the Alt-Tab
// switcher, so it comes from the brand rather than a hardcoded English word.
document.title = APP_NAME;

export const { convertFileSrc } = window.__TAURI__.core;
export const invoke = orderedInvoke;
const { listen } = window.__TAURI__.event;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

const appWin = getCurrentWebviewWindow();
export const $ = (id) => document.getElementById(id);
// The shared confirm dialog; this window never offers a copy-to-clipboard prompt,
// so its toast hook stays empty.
initDialogs({ t, toast: () => {} });
// The shared custom-colour popup (see colorRow()).
export const { el: colorPop, open: openColorPop, close: closeColorPop } = createColorPop();

// ---------------------------------------------------------------- constants

// Export progress arrives on the same event the recording converter uses.
const EXPORT_PROGRESS_EVENT = "recording-convert-progress";

const SPEED_STEPS = [0.25, 0.5, 1, 1.5, 2, 4];
const ZOOM_MIN_PX_PER_S = 0.01;   // lets a fit show the whole clip, up to many hours, in any window width
const ZOOM_MAX_PX_PER_S = 600;    // ~1.7ms per pixel: far past frame accuracy
const MAX_TRACK_PX = 12000;       // canvas/DOM ceiling for the timeline strip
const THUMB_PX = 96;              // target width of one filmstrip thumbnail
const WAVE_BUCKET_MAX = 2000;
// Remaining time stays hidden below this percent and for this many seconds.
export const ETA_MIN_PCT = 10;
export const ETA_MIN_SECS = 20;
// Arrow-key overlay nudge step while Shift is held.
export const NUM_FIELD_BIG_STEP = 10;
const UNDO_MAX_DEFAULT = 40;      // undo steps kept
const MIN_CLIP_MS = 100;          // shortest trim/cut range that still means something
export const RECT_MIN_PX = 16;           // smallest draggable rectangle, in source pixels
// Colour choices offered for text and frame overlays; a custom colour goes through the shared
// popup (colorRow()'s "custom" swatch), never a second picker.
export const OVERLAY_COLORS = ["#ffffff", "#000000", "#ef4444", "#f59e0b", "#22c55e", "#3b82f6", "#a855f7", "#ec4899"];
export const FONT_FILES = [
  ["segoeui", "segoeui.ttf"],
  ["arial", "arial.ttf"],
  ["arialbd", "arialbd.ttf"],
  ["consola", "consola.ttf"],
  ["georgia", "georgia.ttf"],
];
// Font names are proper names and stay untranslated.
export const FONT_LABELS = {
  segoeui: "Segoe UI", arial: "Arial", arialbd: "Arial Bold", consola: "Consolas", georgia: "Georgia",
};
// The live preview draws text with the browser's own font shaping (drawtext's fontfile is only
// reachable by ffmpeg at export time), so each id gets the closest real CSS family.
export const FONT_CSS = {
  segoeui: '"Segoe UI", sans-serif', arial: "Arial, sans-serif", arialbd: "Arial, sans-serif",
  consola: "Consolas, monospace", georgia: "Georgia, serif",
};
// Windows fonts folder, resolved once from the backend (SystemRoot/windir) because Windows may
// live on another drive; this fallback only covers the time before that resolves.
export let fontsDir = "C:/Windows/Fonts";
invoke("windows_fonts_dir").then((d) => { if (d) fontsDir = d; }).catch(() => {});
export const EXPORT_KBPS = { high: 12000, small: 4000, lossless: null };

// GIF never carries sound. Every format key is also the file extension it writes.
export const SILENT_FORMATS = new Set(["gif"]);
export const formatHasAudio = (fmt) => !SILENT_FORMATS.has(fmt);
export const formatExt = (fmt) => fmt;
// A chosen destination path with only its extension swapped: the folder and file name stay.
export const withExtension = (path, ext) => `${String(path).replace(/\.[^\\/.]+$/, "")}.${ext}`;
// A length above which a GIF tends to balloon in size: shown as a warning, never enforced.
export const SILENT_FORMAT_LENGTH_WARN_MS = 10000;
// Enforced, unlike the length warning: these formats never encode above this on their long
// side (on top of any scale op); it only shrinks, never upscales.
export const MAX_SILENT_DIM = 1280;

// ---------------------------------------------------------------- state

// Populated once the boot IIFE fetches it; read by numberInput() and the wheel-toggle setup
// so both agree with applySettings() on the numFieldWheel flag.
export let settings = null;

// The largest number of extra video sources (overlays and sections combined)
// one project may add.
export const EXTRA_CLIPS_MAX = 10;

export const VE = {
  src: "",
  originDir: "", // set only for a copy of a file opened for editing from outside the app
  probe: null,          // RecordingProbe, or a locally guessed one without ffmpeg
  ffmpeg: true,         // false once a call reported "ffmpeg-missing"
  dur: 0,               // source duration in ms
  fps: 30,
  w: 0,
  h: 0,
  pendingScaleKey: null, // a scale preset waiting on VE.w/VE.h from applyProbe()
  scaleDefaultKey: "", // the saved default export scale, re-armed for every loaded source
  ops: [],
  redo: [],
  floor: 0,             // undo may not walk back past this index
  undoMax: UNDO_MAX_DEFAULT,
  numFieldStep: 1,      // arrow-key overlay nudge step
  sel: null,            // { a, b } selection on the source timeline, in ms
  selOv: null,          // id of the overlay currently being edited
  ovFields: {},         // key -> { input, getValue } for the open overlay editor,
                         // filled by renderOverlayEditor(); lets a value commit
                         // refresh the panel's OTHER fields without rebuilding it
  liveEls: new Map(),   // overlay id -> its live-preview DOM node, reused across
                         // frames instead of rebuilt (see renderOverlayLive)
  zoom: 20,             // timeline pixels per second
  playhead: 0,          // ms on the source timeline
  drag: null,           // active pointer gesture on the preview or the timeline
  // The mic track starts muted (the recording itself is untouched); one click on ve-mute-mic
  // mixes it in.
  audio: { sys: { mute: false, vol: 100 }, mic: { mute: true, vol: 100 } },
  hasSys: true,
  hasMic: false,
  // The recording's key-overlay sidecar (events/gaps/hold_ms), or null when none was captured;
  // loaded in loadRecording().
  keySidecar: null,
  // The source file's size/mtime as of the last successful load or save, stamped once per
  // loadRecording(). edit_state matches on this exact triple and it is never re-stat'd
  // mid-session, so a transient touch elsewhere cannot invalidate it.
  editStateSize: 0,
  editStateMtime: 0,
  loadGen: 0,           // bumped on every loadRecording() call; a stale call's
                         // own late-arriving results check this before applying
  strip: [],            // filmstrip thumbnail paths
  stripCount: 0,
  peaks: { 0: null, 1: null },
  exporting: false,
  dirty: false,         // ops changed since the last successful export
  rev: 0,               // counts every ops change; an export compares it with its start value
  lastExport: "",       // path of the file the last export produced
  destOverride: null,   // a full path picked via "Save as…" or typed by hand
  destDir: "",          // remembered folder from "Export to…", persisted across videos
  exportNameOverride: null, // a plain file name typed into ve-export-name
  // Spatial zoom/pan of the preview picture itself, distinct from `zoom` (the timeline's pixels
  // per second). 1 = fit-to-stage; panPx is a CSS-pixel offset applied on top of the fit scaling.
  previewZoom: 1,
  panPx: { x: 0, y: 0 },
  // Fixed editor defaults. applySettings() only overrides the overlay values
  // the editor remembers from the last use.
  zoomStepFactor: 1.2,
  zoomMax: 8,
  ovColorDefault: "#ffffff",
  ovTextColorDefault: "#ffffff",
  ovFrameColorDefault: "#ffffff",
  ovDupOffsetPx: 16,
  ovTextOutlineDefault: 3,
  ovKeysActiveColorDefault: "#3fa7ff",
  ovKeysOpacityDefault: 100,
  ovVideoVolumeDefault: 0,
  fadeMsDefault: 500,
  ovFontDefault: "segoeui",
  defaultPanel: "edit",
  ovPixelateBlockDefault: 16,
  ovSpotlightDimDefault: 0.6,
  ovImageOpacityDefault: 100,
  snapOn: true,
  snapTolPx: 6,
  guide: null,
};

export let TXT_READY = false;
let TX = null; // the text lookup, set once the language has loaded
export let LANG = "en";
// Resolves once loadTexts() has landed, so a modal built entirely at open time can wait for
// real strings instead of showing raw i18n keys.
export let textsReady = null;

// English is fetched at once as the fallback; the active language follows once
// settings arrive.
loadLang("en");

// ---------------------------------------------------------------- i18n

// Same contract as everywhere else: {app} stands for the product name, and a
// missing key falls back to English, then to the key itself.
export function t(key) {
  return TX ? TX(key) : key;
}

// Same binding trick core.js uses: shortcuts.js has no i18n state of its own,
// so every call here is closed over this window's own t()/LANG.
const withShortcut = (el, action) => withShortcutRaw(el, action, t, LANG);
const shortcutRows = (scope) => rowsForScopeRaw(scope, t, LANG);

// Re-reads the labels of the minimize and maximize buttons; set once the bar is wired.
let syncWinButtons = null;

function applyI18n() {
  if (!TXT_READY) return;
  document.documentElement.lang = LANG;
  document.querySelectorAll("[data-i18n]").forEach((el) => { el.textContent = t(el.dataset.i18n); });
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => { el.placeholder = t(el.dataset.i18nPh); });
  document.querySelectorAll("[data-i18n-title]").forEach((el) => {
    if (el.dataset.shortcut) withShortcut(el, el.dataset.shortcut);
    else el.title = t(el.dataset.i18nTitle);
  });
  document.querySelectorAll("[data-i18n-aria]").forEach((el) => { el.setAttribute("aria-label", t(el.dataset.i18nAria)); });
  renderOps();
  renderOverlayList();
  renderOverlayEditor();
  renderSpeedSteps();
  renderScalePresets();
  updateCutCount();
  updateExportNote();
  updatePlayButton();
  updateAudioLabels();
  updateVolumeValueText("ve-vol-sys");
  updateVolumeValueText("ve-vol-mic");
  renderPlayhead();
  if (syncWinButtons) syncWinButtons();
  syncSelectionButtons();
  renderHelp();
}

export async function loadTexts(settings) {
  const lang = resolveLang(settings?.language, LANGS);
  const { tx } = await fullTexts(lang);
  LANG = lang;
  TX = tx;
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

export const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
export const even = (v) => Math.max(0, Math.round(v / 2) * 2);
export const pad = (n, w) => String(n).padStart(w, "0");

export function fmtTime(ms) {
  const v = Math.max(0, Math.round(ms));
  const m = Math.floor(v / 60000);
  const s = Math.floor((v % 60000) / 1000);
  return `${pad(m, 2)}:${pad(s, 2)}.${pad(v % 1000, 3)}`;
}

// Shows or hides the collapsible "technical details" row under the export status line; an
// empty `raw` (a non-ffmpeg error, or success) hides it.
export function showExportErrorDetails(raw) {
  const box = $("ve-export-error-details");
  box.classList.toggle("hidden", !raw);
  box.open = false;
  $("ve-export-error-raw").textContent = raw || "";
}
$("ve-export-error-copy").addEventListener("click", () => {
  copyTextChecked(invoke, $("ve-export-error-raw").textContent).then(() => toast(t("copied"))).catch((err) => toast(errMessageLogged(err, t, "video")));
});

// Accepts "mm:ss.mmm", "ss.mmm" and a bare number of seconds: whichever the
// user happens to type. Anything unreadable returns null so the caller can keep
// the previous value instead of jumping to zero.
export function parseTime(text) {
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

// This window's toast stays longer than the main window's (the editor reports export
// and file problems here, which take longer to read); one zoom key press scales the
// timeline by this factor.
const VE_TOAST_MS = 2600;
const TIMELINE_ZOOM_STEP = 1.4;
let toastTimer = 0;
export function toast(message) {
  const el = $("ve-toast");
  el.textContent = message;
  el.classList.remove("hidden");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.add("hidden"), VE_TOAST_MS);
}

// The help dialog: a section list on the left, the text of the chosen section on the right.
// The last section is the shortcut overview, filled straight from SHORTCUTS.
const HELP_SECTIONS = ["nav", "sel", "cut", "edit", "ov", "audio", "export", "close", "keys"];
const HELP_KEYS = {
  nav: ["veHelpNavTitle", "veHelpNavBody"], sel: ["veHelpSelTitle", "veHelpSelBody"],
  cut: ["veHelpCutTitle", "veHelpCutBody"], edit: ["veHelpEditTitle", "veHelpEditBody"],
  ov: ["veHelpOvTitle", "veHelpOvBody"], audio: ["veHelpAudioTitle", "veHelpAudioBody"],
  export: ["veHelpExportTitle", "veHelpExportBody"], close: ["veHelpCloseTitle", "veHelpCloseBody"],
  keys: ["veHelpKeysTitle", "veHelpKeysBody"],
};
let helpSection = "nav";

function renderHelp() {
  const list = $("shortcuts-modal-list");
  if (!list) return;
  list.innerHTML = "";
  document.querySelectorAll("#ve-help-nav .ve-help-tab").forEach((b) => {
    const on = b.dataset.help === helpSection;
    b.classList.toggle("active", on);
    b.setAttribute("aria-selected", on ? "true" : "false");
    b.tabIndex = on ? 0 : -1;
  });
  const [titleKey, bodyKey] = HELP_KEYS[helpSection];
  const h = document.createElement("h3");
  h.className = "ve-help-h";
  h.textContent = t(titleKey);
  list.appendChild(h);
  for (const para of t(bodyKey).split("\n\n")) {
    const p = document.createElement("p");
    p.className = "ve-help-p";
    p.textContent = para;
    list.appendChild(p);
  }
  if (helpSection === "keys") {
    const rows = document.createElement("div");
    rows.className = "shortcuts-list ve-help-keys";
    buildShortcutRows(rows, shortcutRows("videoEditor"));
    list.appendChild(rows);
  }
  list.scrollTop = 0;
}
function setHelpSection(id, focusTab) {
  if (!HELP_KEYS[id]) return;
  helpSection = id;
  renderHelp();
  if (focusTab) document.querySelector(`#ve-help-nav .ve-help-tab[data-help="${id}"]`)?.focus();
}
function openShortcutsModal() {
  helpSection = "nav";
  renderHelp();
  openShortcutsOverlay($("shortcuts-modal"));
  document.querySelector("#ve-help-nav .ve-help-tab.active")?.focus();
}
function closeShortcutsModal() {
  closeShortcutsOverlay($("shortcuts-modal"));
}
// Arrow keys, Home and End move between the sections while a section button has the focus.
function helpNavKey(e) {
  if (!e.target.closest?.(".ve-help-tab")) return;
  const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
  let i = HELP_SECTIONS.indexOf(helpSection);
  if (step) i = (i + step + HELP_SECTIONS.length) % HELP_SECTIONS.length;
  else if (e.key === "Home") i = 0;
  else if (e.key === "End") i = HELP_SECTIONS.length - 1;
  else return;
  e.preventDefault();
  setHelpSection(HELP_SECTIONS[i], true);
}

// ---------------------------------------------------------------- operation list

// VE.ops IS the render list, in order. Once the undo window is exceeded the
// floor advances: entries below it can no longer be undone, but they stay in the
// list, because the preview, the export and the saved edit state read it (a trim
// made long ago must not vanish after enough later edits).
export function pushOp(op, opts) {
  // Every overlay gets a stacking number once, at creation. Edits re-push the
  // overlay at the end of VE.ops (see pushReplacing) but keep this number, so
  // export and preview stack overlays by it instead of by array position.
  if (op.tool === "overlay" && op.z === undefined) op.z = 1 + Math.max(0, ...VE.ops.map((o) => o.z || 0));
  VE.ops.push(op);
  if (VE.ops.length - VE.floor > VE.undoMax) VE.floor = VE.ops.length - VE.undoMax;
  VE.redo.length = 0;
  afterOpsChanged(opts);
}

// Ops that may exist only once (trim, crop, speed, fade, scale) and an overlay being re-edited
// replace their predecessor rather than stacking. The replaced value is kept on the new op as
// `_prev`, so a second crop/spotlight drag stays in history and one undo steps back to it
// instead of removing the operation. undo()/redo() walk this chain one link at a time.
export function pushReplacing(match, op, opts) {
  const prev = VE.ops.find(match) || null;
  // A replaced op below the floor was no longer undoable; stepping back to it later must not
  // make it undoable again (see undo()).
  const prevLocked = !!prev && VE.ops.indexOf(prev) < VE.floor;
  // The floor is a count-based boundary into VE.ops; removing entries below it
  // shifts every later index down, so it shrinks by that many (and only those) or
  // it would re-lock operations that were still undoable.
  const lockedGone = VE.ops.filter((o, i) => i < VE.floor && match(o)).length;
  VE.ops = VE.ops.filter((o) => !match(o));
  VE.floor = Math.max(0, VE.floor - lockedGone);
  if (prev && op.tool === "overlay" && op.z === undefined && prev.z !== undefined) op = { ...op, z: prev.z };
  const next = prev ? { ...op, _prev: prev, ...(prevLocked ? { _prevLocked: true } : {}) } : op;
  pushOp(next._prev ? capPrevChain(next, VE.undoMax) : next, opts);
}

// A replacing op's `_prev` chain grows one link per re-edit of the same operation and never
// touches VE.ops, so VE.undoMax does not bound it; cut it at the same depth.
function capPrevChain(op, max) {
  let node = op, depth = 1;
  while (node._prev && depth < max) { node = node._prev; depth++; }
  delete node._prev;
  return op;
}

// Overlays bottom to top. Legacy ops without a number sort first, in list order
// (the sort is stable).
export const overlayStack = () => opsOf("overlay").sort((a, b) => (a.z || 0) - (b.z || 0));

// Removing an operation is one undo step too. A "removal" marker is pushed after the entries
// are taken out and remembers each removed op and its slot, so undo restores them in place and
// redo removes them again. It is bookkeeping only: never listed, saved or exported.
export function removeOps(match) {
  const removed = [];
  VE.ops.forEach((o, at) => { if (o.tool !== "removal" && match(o)) removed.push({ op: o, at }); });
  if (!removed.length) return;
  VE.ops = VE.ops.filter((o) => !removed.some((r) => r.op === o));
  const floor = VE.floor;
  VE.floor = Math.max(0, VE.floor - removed.filter((r) => r.at < VE.floor).length);
  pushOp({ tool: "removal", removed, floor });
}

// Swaps the matching ops for one new op as a single undo step: the marker records what went and
// what came, so one undo brings every old op back and takes the new one out again.
export function replaceOps(match, op) {
  const removed = [];
  VE.ops.forEach((o, at) => { if (o.tool !== "removal" && match(o)) removed.push({ op: o, at }); });
  const floor = VE.floor;
  VE.ops = VE.ops.filter((o) => !removed.some((r) => r.op === o));
  VE.floor = Math.max(0, VE.floor - removed.filter((r) => r.at < VE.floor).length);
  VE.ops.push(op);
  pushOp({ tool: "removal", removed, floor, added: op });
}

function restoreRemoved(marker) {
  for (const { op, at } of marker.removed) VE.ops.splice(Math.min(at, VE.ops.length), 0, op);
}

// A drag that will commit an op on release (rect, resize, trim) must not have that op undone or
// deleted from under it, or the release pushes it back as a new entry and drops the redo stack.
const editDragActive = () => !!VE.drag && ["rect", "resize", "trim"].includes(VE.drag.kind);

export function undo() {
  if (editDragActive() || VE.ops.length <= VE.floor) return;
  const idx = VE.ops.length - 1;
  const cur = VE.ops[idx];
  if (cur.tool === "removal") {
    VE.ops.pop();
    if (cur.added) VE.ops = VE.ops.filter((o) => o !== cur.added);
    restoreRemoved(cur);
    // The removal lowered the floor for every locked op it took out; they are back, so is it.
    if (Number.isFinite(cur.floor)) VE.floor = Math.max(VE.floor, Math.min(cur.floor, VE.ops.length));
    VE.redo.push(cur);
  } else if (cur._prev) {
    // This slot was a replacement (pushReplacing): step it back to the
    // value it replaced rather than removing the row outright, and keep the
    // full entry (chain intact) on the redo stack so redo can restore it.
    VE.ops[idx] = cur._prev;
    // The restored op was locked when it was replaced: it stays, a second Undo stops at it.
    if (cur._prevLocked) VE.floor = Math.max(VE.floor, idx + 1);
    VE.redo.push(cur);
  } else {
    VE.redo.push(VE.ops.pop());
  }
  afterOpsChanged();
}

export function redo() {
  if (editDragActive() || !VE.redo.length) return;
  const next = VE.redo.pop();
  if (next.tool === "removal") {
    VE.floor = Math.max(0, VE.floor - next.removed.filter((r) => r.at < VE.floor).length);
    VE.ops = VE.ops.filter((o) => !next.removed.some((r) => r.op === o));
    if (next.added) VE.ops.push(next.added);
    VE.ops.push(next);
  } else if (next._prev) {
    // The matching undo left this slot holding next._prev: put the newer
    // value back in the same slot instead of appending a second row.
    VE.ops[VE.ops.length - 1] = next;
    if (next._prevLocked) VE.floor = Math.min(VE.floor, VE.ops.length - 1);
  } else {
    VE.ops.push(next);
  }
  afterOpsChanged();
}

// A value commit inside the overlay editor or a rect drag on the open overlay passes
// keepEditor: true: everything updates except the two panels (renderOverlayEditor,
// renderEditFields) that would rebuild the field being typed in. syncOverlayFields keeps that
// panel's other inputs current (see VE.ovFields).
export function afterOpsChanged(opts) {
  const keep = opts && opts.keepEditor;
  VE.dirty = true;
  VE.rev += 1;
  $("ve-undo").disabled = VE.ops.length <= VE.floor;
  $("ve-redo").disabled = !VE.redo.length;
  if (VE.selOv && !VE.ops.some((o) => o.id === VE.selOv)) VE.selOv = null;
  renderOps();
  renderOverlayList();
  renderClipList();
  if (keep) syncOverlayFields();
  else { renderOverlayEditor(); renderEditFields(); }
  renderRegions();
  renderRects();
  // The tick() loop only redraws the live overlay layer once the playhead
  // itself moves, so an edit made while paused (a colour, a position, an
  // opacity) would otherwise sit invisible until the next scrub or play.
  renderOverlayLive(videoBox());
  updateCutCount();
  updateScaleCurrent();
  updateExportNote();
  applyPreviewSpeed();
  updateNudgeHint();
}

// Shown only while an overlay is selected, because the arrow keys then nudge the overlay
// instead of stepping frames.
export function updateNudgeHint() {
  $("ve-nudge-hint").classList.toggle("hidden", !VE.selOv);
}

// Writes the current op's values into the overlay editor's inputs without touching the node
// the user may be focused on (registered in VE.ovFields by renderOverlayEditor()).
// Number fields whose text is typed but not committed keep their text across a refresh; any
// programmatic change (reset, handle drag, undo, redo, preset) rewrites every other field.
const uncommittedFields = new Set();
document.addEventListener("input", (e) => {
  if (e.target instanceof HTMLInputElement) uncommittedFields.add(e.target);
}, true);
document.addEventListener("change", (e) => uncommittedFields.delete(e.target), true);
document.addEventListener("focusout", (e) => uncommittedFields.delete(e.target), true);
function isUncommitted(el) {
  for (const n of uncommittedFields) if (n === el || (el.contains && el.contains(n))) return true;
  return false;
}

export function syncOverlayFields() {
  const fields = VE.ovFields;
  if (!fields) return;
  const op = VE.ops.find((o) => o.id === VE.selOv);
  if (!op) return;
  for (const key in fields) {
    const { input, getValue } = fields[key];
    // A plain <input> contains itself, which also covers `input` being the field itself rather
    // than a numberField() wrapper.
    if (isUncommitted(input)) continue;
    const value = getValue(op);
    if (value == null) continue;
    if (typeof input.setValue === "function") input.setValue(value);
    else input.value = value;
  }
}

export const opOf = (tool) => VE.ops.filter((o) => o.tool === tool).pop() || null;
export const opsOf = (tool) => VE.ops.filter((o) => o.tool === tool);

function opLabel(op) {
  switch (op.tool) {
    case "trim": return `${t("veOpTrim")} ${fmtTime(op.startMs)}-${fmtTime(op.endMs)}`;
    case "cut": return `${t("veOpCut")} ${fmtTime(op.startMs)}-${fmtTime(op.endMs)}`;
    case "crop": return `${t("veOpCrop")} ${op.rect.w}×${op.rect.h}`;
    case "speed": return `${t("veOpSpeed")} ${op.rate}×`;
    case "fade": return `${t("veOpFade")} ${op.inMs}/${op.outMs} ${t("veUnitMs")}`;
    case "scale": return `${t("veOpScale")} ${op.w}×${op.h}`;
    case "overlay": return `${t("veOpOverlay")}: ${overlayName(op)}`;
    default: return op.tool;
  }
}

// What an overlay is called in a list row or a bar tooltip: its kind, and for a text overlay the
// first 20 characters of the text, so several text overlays can be told apart.
export function overlayName(op) {
  const base = t(overlayLabelKey(op.kind));
  if (op.kind !== "text" || !op.text) return base;
  const chars = Array.from(String(op.text).replace(/\s+/g, " ").trim());
  if (!chars.length) return base;
  return `${base} "${chars.slice(0, 20).join("")}${chars.length > 20 ? "\u2026" : ""}"`;
}

export const overlayLabelKey = (kind) => ({
  image: "veOvImage", frame: "veOvFrame", text: "veOvText",
  pixelate: "veOvPixelate", spotlight: "veOvSpotlight", video: "veOvVideo",
  keys: "veOvKeys",
}[kind] || "veOpOverlay");

export function extraClipCount() {
  return VE.ops.filter((o) => o.tool === "clip" || (o.tool === "overlay" && o.kind === "video")).length;
}

export function deleteOp(op) {
  if (op.tool === "crop") changeCrop(() => removeOps((o) => o === op));
  else removeOps((o) => o === op);
}

// Selecting an op row: an overlay is selected and its editor opened, every op with a start
// time also moves the playhead there.
function selectOp(op) {
  if (op.tool === "overlay") selectOverlay(op.id);
  if (Number.isFinite(op.startMs)) seek(op.startMs);
}

// Makes an overlay the edited one and opens its editor; null leaves the selection. Used by the
// list rows and by clicks on the picture.
export function selectOverlay(id) {
  VE.selOv = id;
  if (id) showPanel("ve-panel-overlays");
  renderOverlayList(); renderOverlayEditor(); renderRects(); updateNudgeHint(); renderRegions();
}

function renderOps() {
  const list = $("ve-ops-list");
  list.innerHTML = "";
  // Section clips are a whole inserted file rather than a filter, and get their own list
  // (renderClipList) instead of a line here.
  const shown = VE.ops.filter((op) => op.tool !== "clip" && op.tool !== "removal");
  if (!shown.length) {
    const empty = document.createElement("p");
    empty.className = "modal-hint";
    empty.textContent = t("veOpsEmpty");
    list.appendChild(empty);
    return;
  }
  shown.forEach((op) => {
    const row = document.createElement("div");
    row.className = "ve-op-row";
    // A button, so Tab reaches every row: Enter or a click selects the op (an overlay opens its
    // editor) and moves the playhead to where it starts, Delete removes it.
    const label = document.createElement("button");
    label.type = "button";
    label.className = "ve-op-label";
    label.textContent = opLabel(op);
    label.addEventListener("click", () => selectOp(op));
    label.addEventListener("keydown", (e) => {
      if (e.key !== "Delete" && e.key !== "Backspace") return;
      e.preventDefault();
      deleteOp(op);
    });
    row.appendChild(label);
    const del = document.createElement("button");
    del.type = "button";
    del.className = "icon-btn ve-op-del";
    del.innerHTML = '<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="currentColor" d="M6.4 5 12 10.6 17.6 5 19 6.4 13.4 12 19 17.6 17.6 19 12 13.4 6.4 19 5 17.6 10.6 12 5 6.4Z"/></svg>';
    del.title = t("veOpRemove");
    del.setAttribute("aria-label", `${t("veOpRemove")}: ${opLabel(op)}`);
    del.addEventListener("click", () => deleteOp(op));
    row.appendChild(del);
    list.appendChild(row);
  });
}

// ---------------------------------------------------------------- timeline maths

// keptSegments()'s scratch storage, reused across calls instead of building a fresh array
// chain each time: tick() (the playback render loop) calls it at least once per frame, so the
// garbage would otherwise grow with playback time. Every caller reads the numbers back
// synchronously and never stores the array or a segment past its own call, so reuse never
// leaks a stale value.
const KEPT_CUTS_BUF = [];
const KEPT_SEGMENTS_BUF = [];

// The trim range held inside the recording: what the export uses and the timeline shows.
export function trimBounds() {
  const trim = opOf("trim");
  const a = trim ? clamp(trim.startMs, 0, VE.dur) : 0;
  const b = trim ? clamp(trim.endMs, a, VE.dur) : VE.dur;
  return { a, b };
}

// The kept parts of the source, in order: the trim range minus every cut.
export function keptSegments() {
  const { a, b } = trimBounds();
  KEPT_CUTS_BUF.length = 0;
  for (const c of opsOf("cut")) {
    const ca = clamp(Math.min(c.startMs, c.endMs), a, b);
    const cb = clamp(Math.max(c.startMs, c.endMs), a, b);
    if (cb - ca > 0) KEPT_CUTS_BUF.push({ a: ca, b: cb });
  }
  KEPT_CUTS_BUF.sort((x, y) => x.a - y.a);
  KEPT_SEGMENTS_BUF.length = 0;
  let cursor = a;
  for (const cut of KEPT_CUTS_BUF) {
    if (cut.a > cursor) KEPT_SEGMENTS_BUF.push({ a: cursor, b: cut.a });
    cursor = Math.max(cursor, cut.b);
  }
  if (b > cursor) KEPT_SEGMENTS_BUF.push({ a: cursor, b });
  if (!KEPT_SEGMENTS_BUF.length) KEPT_SEGMENTS_BUF.push({ a, b: a });
  return KEPT_SEGMENTS_BUF;
}

export const speedRate = () => {
  const op = opOf("speed");
  return op ? clamp(op.rate, 0.1, 10) : 1;
};

// The kept parts of the main source in viewing order, interleaved with every inserted video
// section (tool "clip"): one entry per piece of the export, `{ type: "main", a, b }` (source ms,
// a keptSegments() entry) or `{ type: "clip", op }` (a whole other file). Position "cut" (the
// current position) splits the kept segment containing it; its `atMs` is an output-timeline
// offset recorded at insertion (see clipAtMsFromPlayhead), so clips inserted at different
// moments keep their relative order as earlier edits change the source-timeline math.
export function timelinePlan() {
  const segs = keptSegments();
  const rate = speedRate();
  let cum = 0;
  const withCum = segs.map((s) => {
    const startCum = cum;
    cum += (s.b - s.a) / rate;
    return { a: s.a, b: s.b, startCum, endCum: cum };
  });
  const clips = opsOf("clip").filter((c) => !c.missing);
  const startClips = clips.filter((c) => c.position === "start");
  const endClips = clips.filter((c) => c.position === "end");
  const cutClips = clips.filter((c) => c.position === "cut").sort((x, y) => x.atMs - y.atMs);

  const plan = [];
  for (const c of startClips) plan.push({ type: "clip", op: c });
  let cutIdx = 0;
  for (const seg of withCum) {
    let a = seg.a;
    let startCum = seg.startCum;
    while (cutIdx < cutClips.length && cutClips[cutIdx].atMs >= startCum && cutClips[cutIdx].atMs <= seg.endCum) {
      const cut = cutClips[cutIdx];
      const splitSrc = clamp(a + (cut.atMs - startCum) * rate, a, seg.b);
      if (splitSrc > a) plan.push({ type: "main", a, b: splitSrc });
      plan.push({ type: "clip", op: cut });
      a = splitSrc;
      startCum = cut.atMs;
      cutIdx++;
    }
    if (seg.b > a) plan.push({ type: "main", a, b: seg.b });
  }
  // A position past the current total (the end was trimmed or cut away after the insert)
  // lands at the end instead of vanishing from the preview and the export; atMs stays as stored.
  for (; cutIdx < cutClips.length; cutIdx++) plan.push({ type: "clip", op: cutClips[cutIdx] });
  for (const c of endClips) plan.push({ type: "clip", op: c });
  return plan;
}

function planEntryOutMs(entry) {
  return entry.type === "main" ? (entry.b - entry.a) / speedRate() : entry.op.durationMs / speedRate();
}

// Source ms -> ms in the exported clip. Overlay ranges are picked on the source timeline but
// ffmpeg's `enable` expressions run on the output timeline, so every range goes through this
// on its way into the filter graph. The walk follows timelinePlan(), so the length of every
// section clip before the point counts (overlays are applied after the clips are concatenated).
// A point exactly on the start of a kept stretch lands after the clips in front of it when it
// opens a range and before them when it closes one (atEnd).
export function outMs(srcMs, atEnd) {
  const rate = speedRate();
  let acc = 0;
  let pending = 0;
  for (const entry of timelinePlan()) {
    if (entry.type === "clip") { pending += planEntryOutMs(entry); continue; }
    if (srcMs < entry.a || (atEnd && srcMs === entry.a)) break;
    acc += pending;
    pending = 0;
    acc += (Math.min(srcMs, entry.b) - entry.a) / rate;
    if (srcMs < entry.b) break;
  }
  return acc;
}

// Includes every inserted section clip's length, so the duration readout, fade end time and
// export progress total stay correct after a clip is inserted.
export const totalOutMs = () => timelinePlan().reduce((sum, e) => sum + planEntryOutMs(e), 0);

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

export function fitZoom() {
  const view = $("ve-timeline").clientWidth || 600;
  const seconds = Math.max(0.5, VE.dur / 1000);
  return clamp(view / seconds, ZOOM_MIN_PX_PER_S, ZOOM_MAX_PX_PER_S);
}

export function setZoom(next, anchorMs) {
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

export function layoutTrack() {
  const width = trackWidth();
  $("ve-track").style.width = `${width}px`;
  renderRuler();
  renderStrip();
  renderRegions();
  renderPlayhead();
}

// Tick spacing is picked so labels never collide: the smallest step from the
// list that is at least ~70px wide at the current zoom.
const TICK_STEPS_MS = [100, 250, 500, 1000, 2000, 5000, 10000, 15000, 30000, 60000, 120000, 300000, 600000, 1200000, 1800000, 3600000, 7200000];
// A ruler tick label: minutes and seconds, with the milliseconds only on a fine scale. Cut at the
// dot rather than at a fixed width, so a recording of 100 minutes or more keeps its full minutes.
export function rulerLabel(ms, withMs) {
  const label = fmtTime(ms);
  return withMs ? label : label.slice(0, label.indexOf("."));
}

function renderRuler() {
  const ruler = $("ve-ruler");
  ruler.innerHTML = "";
  if (!VE.dur) return;
  const step = TICK_STEPS_MS.find((s) => msToPx(s) >= 70) || TICK_STEPS_MS[TICK_STEPS_MS.length - 1];
  for (let ms = 0; ms <= VE.dur; ms += step) {
    const tick = document.createElement("span");
    tick.className = "ve-tick";
    tick.style.left = `${msToPx(ms)}px`;
    tick.textContent = rulerLabel(ms, step < 1000);
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

// The bar elements of the last render and the op each stands for: a press on a cut band or an
// overlay bar selects that op, a press on its edge resizes it.
const barOps = new WeakMap();
const barEls = new Map();
const BAR_EDGE_MIN_PX = 14;

function addBarEdges(el, widthPx) {
  if (widthPx < BAR_EDGE_MIN_PX) return;
  for (const edge of ["a", "b"]) {
    const grip = document.createElement("span");
    grip.className = `ve-bar-edge ve-bar-edge-${edge}`;
    grip.dataset.edge = edge;
    el.appendChild(grip);
  }
}

export function renderRegions() {
  const { a, b } = trimBounds();
  $("ve-dim-a").style.width = `${msToPx(a)}px`;
  $("ve-dim-b").style.left = `${msToPx(b)}px`;
  $("ve-dim-b").style.width = `${Math.max(0, trackWidth() - msToPx(b))}px`;
  $("ve-trim-a").style.left = `${msToPx(a)}px`;
  $("ve-trim-b").style.left = `${msToPx(b)}px`;
  setTimeValue($("ve-trim-a"), a, VE.dur);
  setTimeValue($("ve-trim-b"), b, VE.dur);

  const cuts = $("ve-cuts");
  cuts.innerHTML = "";
  barEls.clear();
  for (const cut of opsOf("cut")) {
    const el = document.createElement("div");
    el.className = "ve-cut";
    const width = Math.max(2, msToPx(Math.abs(cut.endMs - cut.startMs)));
    el.style.left = `${msToPx(Math.min(cut.startMs, cut.endMs))}px`;
    el.style.width = `${width}px`;
    el.title = `${t("veOpCut")} ${fmtTime(cut.startMs)}-${fmtTime(cut.endMs)}`;
    addBarEdges(el, width);
    barOps.set(el, cut);
    barEls.set(cut, el);
    cuts.appendChild(el);
  }

  const bars = $("ve-ovbars");
  bars.innerHTML = "";
  for (const op of opsOf("overlay")) {
    const el = document.createElement("div");
    el.className = "ve-ovbar" + (op.id === VE.selOv ? " sel" : "");
    const width = Math.max(3, msToPx(Math.max(0, op.endMs - op.startMs)));
    el.style.left = `${msToPx(op.startMs)}px`;
    el.style.width = `${width}px`;
    el.title = `${overlayName(op)} ${fmtTime(op.startMs)}-${fmtTime(op.endMs)}`;
    addBarEdges(el, width);
    barOps.set(el, op);
    barEls.set(op, el);
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
  // renderRegions() is called on every VE.sel mutation (mark in/out, timeline drag, clear), so
  // syncing the overlay editor's "Use selection" button here enables it immediately while that
  // panel is open, without rebuilding the panel on every pointermove.
  syncSelectionButtons();
}

// Trim, cut and "use selection" need a range that spans something. While there is none they
// are disabled and their tooltip says how to get one.
export function syncSelectionButtons() {
  const ready = !!markedRange();
  for (const btn of [$("ve-trim-set"), $("ve-cut-add"), ...document.querySelectorAll(".ve-use-sel")]) {
    btn.disabled = !ready;
    const own = btn.dataset.i18nTitle;
    if (!ready) btn.title = t("veNeedSelectionTip");
    else if (btn.dataset.shortcut) withShortcut(btn, btn.dataset.shortcut);
    else if (own) btn.title = t(own);
    else btn.removeAttribute("title");
  }
}

// The two trim handles are sliders: a screen reader gets the plain time it would otherwise have
// to infer from a bare pixel position. The values are milliseconds, so a short clip still moves.
function setTimeValue(el, ms, durMs) {
  el.setAttribute("aria-valuemin", "0");
  el.setAttribute("aria-valuemax", String(Math.round(durMs)));
  el.setAttribute("aria-valuenow", String(Math.round(ms)));
  el.setAttribute("aria-valuetext", t("a11yTimeOfTotal").replace("{time}", fmtTime(ms)).replace("{total}", fmtTime(durMs)));
}

function renderPlayhead() {
  $("ve-playhead").style.left = `${msToPx(VE.playhead)}px`;
  $("ve-time").textContent = fmtTime(VE.playhead);
  $("ve-frame").textContent = t("veFrameNo").replace("{n}", String(frameOf(VE.playhead) + 1));
}

// ---------------------------------------------------------------- filmstrip + waveform

let stripTimer = 0;
function scheduleStrip() {
  clearTimeout(stripTimer);
  stripTimer = setTimeout(loadStrip, 350);
}

export async function loadStrip() {
  if (!VE.ffmpeg || !VE.src || !VE.dur) return;
  const want = clamp(Math.round(trackWidth() / THUMB_PX), 8, 160);
  // Only ever ask for more, and only when it is a real step up: regenerating a
  // filmstrip is the most expensive call this window makes.
  if (VE.stripCount && want <= VE.stripCount * 1.6) return;
  // This call can still be in flight when the user switches source; the generation check on
  // return stops a slow filmstrip for the old video from painting over the new one's.
  const myGen = VE.loadGen;
  try {
    const paths = await invoke("generate_filmstrip", { path: VE.src, count: want });
    if (VE.loadGen !== myGen) return;
    if (Array.isArray(paths) && paths.length) {
      VE.strip = paths;
      VE.stripCount = want;
      renderStrip();
    }
  } catch (err) {
    // Left at zero, so the next resize or reload asks again instead of keeping an empty strip.
    if (VE.loadGen === myGen) { VE.stripCount = 0; noteFfmpeg(err); }
  }
}

let waveTimer = 0;
export function scheduleWaves() {
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

// Waveform colours are read once per theme, not on every redraw of a zoom or resize.
const waveColors = [null, null];
const WAVE_HEIGHT = 34;
const WAVE_MAX_PX = 16384;

function waveColor(track) {
  if (!waveColors[track]) {
    const style = getComputedStyle(document.documentElement);
    waveColors[track] = (style.getPropertyValue(track === 0 ? "--accent" : "--muted") || "#3b82f6").trim();
  }
  return waveColors[track];
}

function drawWave(track) {
  const canvas = track === 0 ? $("ve-wave-sys") : $("ve-wave-mic");
  const data = VE.peaks[track];
  const width = Math.max(1, Math.round(trackWidth()));
  // Backing store in device pixels so the waveform stays sharp on scaled displays; the CSS size
  // stays the layout size. A very long track scales below one so the canvas never passes the cap.
  const scale = Math.min(window.devicePixelRatio || 1, WAVE_MAX_PX / width);
  canvas.width = Math.round(width * scale);
  canvas.height = Math.round(WAVE_HEIGHT * scale);
  canvas.style.width = `${width}px`;
  canvas.style.height = `${WAVE_HEIGHT}px`;
  const ctx = canvas.getContext("2d");
  ctx.setTransform(scale, 0, 0, scale, 0, 0);
  ctx.clearRect(0, 0, width, WAVE_HEIGHT);
  if (!data || !data.values.length) return;
  ctx.fillStyle = waveColor(track);
  const mid = WAVE_HEIGHT / 2;
  for (let x = 0; x < width; x++) {
    const value = data.values[Math.min(data.values.length - 1, Math.floor((x / width) * data.values.length))];
    const h = Math.max(1, Math.abs(Number(value) || 0) * mid);
    ctx.fillRect(x, mid - h, 1, h * 2);
  }
}

// ---------------------------------------------------------------- playback

export const videoEl = $("ve-video");

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

export function seek(ms, vb) {
  const target = clamp(ms, 0, VE.dur);
  VE.playhead = target;
  videoEl.currentTime = target / 1000;
  renderPlayhead();
  renderOverlayLive(vb);
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

// A single throw must not kill this loop for good (the playhead, time display and live overlay
// would freeze). Reported once, never per frame; the loop always re-arms itself in the finally below.
let tickErrorReported = false;
function noteTickError(err) {
  if (tickErrorReported) return;
  tickErrorReported = true;
  console.error("video editor tick() failed", err);
  toast(t("veTickError"));
}

// Cut regions are skipped seamlessly during playback, so the preview is the finished clip. A
// frame loop rather than the video's timeupdate event, which fires about four times a second,
// long enough to see a cut region play before it is skipped.
export function tick() {
  try {
    if (VE.dur) {
      const now = videoEl.currentTime * 1000;
      const playing = !videoEl.paused;
      // Paused and already in step with the playhead: nothing to do, so an idle
      // editor costs no segment sort and no layout read per frame.
      if (playing || Math.abs(now - VE.playhead) > 0.5) {
        const vb = videoBox();
        if (playing) {
          const segments = keptSegments();
          const last = segments[segments.length - 1];
          if (now >= last.b - 1) {
            videoEl.pause();
            seek(last.b, vb);
          } else {
            const kept = nextKept(now);
            if (kept > now + 1) videoEl.currentTime = kept / 1000;
          }
        }
        if (Math.abs(now - VE.playhead) > 0.5) {
          VE.playhead = now;
          renderPlayhead();
          renderOverlayLive(vb);
          if (!videoEl.paused) keepPlayheadVisible();
        }
      }
    }
  } catch (err) {
    noteTickError(err);
  } finally {
    requestAnimationFrame(tick);
  }
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

// A drag that reaches past the visible timeline keeps scrolling it, faster the further out the
// pointer is, and re-applies the drag at the last pointer position after every step.
const AUTOSCROLL_MAX_PX = 40;
const AUTOSCROLL_TICK_MS = 16;
let dragClientX = 0;
let dragTimer = 0;

function dragAutoscroll() {
  dragTimer = 0;
  if (!VE.drag) return;
  const view = timelineEl.getBoundingClientRect();
  const over = dragClientX < view.left ? dragClientX - view.left : dragClientX > view.right ? dragClientX - view.right : 0;
  if (over) {
    const before = timelineEl.scrollLeft;
    const step = Math.sign(over) * clamp(Math.abs(over) / 4, 2, AUTOSCROLL_MAX_PX);
    timelineEl.scrollLeft = before + step;
    if (timelineEl.scrollLeft !== before) applyTimelineDrag(dragClientX);
  }
  dragTimer = setTimeout(dragAutoscroll, AUTOSCROLL_TICK_MS);
}

function startDragAutoscroll(clientX) {
  dragClientX = clientX;
  if (!dragTimer) dragTimer = setTimeout(dragAutoscroll, AUTOSCROLL_TICK_MS);
}

function stopDragAutoscroll() {
  if (dragTimer) clearTimeout(dragTimer);
  dragTimer = 0;
}

// A press on the ruler band scrubs; a press anywhere lower on the track starts a selection that
// only counts once the pointer moved a few pixels, so a plain click just sets the playhead.
const DRAG_SLOP_PX = 4;
regionsEl.addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  const handle = e.target.closest(".ve-handle");
  // A press the page did not start as a real pointer (a synthetic one) has nothing to capture.
  try { regionsEl.setPointerCapture(e.pointerId); } catch { /* the move and up events still reach the track */ }
  const ms = timelineMs(e.clientX);
  const trackLeft = $("ve-track").getBoundingClientRect().left;
  if (handle) {
    const trim = opOf("trim") || { startMs: 0, endMs: VE.dur };
    const edge = handle.id === "ve-trim-a" ? "a" : "b";
    // The handle keeps the spot where it was grabbed instead of jumping its centre to the pointer.
    const grabPx = e.clientX - (trackLeft + msToPx(edge === "a" ? trim.startMs : trim.endMs));
    VE.drag = { kind: "trim", edge, grabPx, live: { startMs: trim.startMs, endMs: trim.endMs } };
    startDragAutoscroll(e.clientX);
    return;
  }
  const barEl = e.target.closest(".ve-cut, .ve-ovbar");
  const barOp = barEl && barOps.get(barEl);
  if (barOp) {
    const grip = e.target.closest(".ve-bar-edge");
    if (grip) {
      if (barOp.tool === "overlay") selectOverlay(barOp.id);
      const a = Math.min(barOp.startMs, barOp.endMs), b = Math.max(barOp.startMs, barOp.endMs);
      const edge = grip.dataset.edge;
      const grabPx = e.clientX - (trackLeft + msToPx(edge === "a" ? a : b));
      VE.drag = { kind: "resize", op: barOp, edge, grabPx, live: { startMs: a, endMs: b } };
      startDragAutoscroll(e.clientX);
    } else {
      selectOp(barOp);
      VE.drag = { kind: "bar" };
    }
    return;
  }
  const rulerH = $("ve-ruler").offsetHeight || 18;
  if (e.clientY - $("ve-track").getBoundingClientRect().top < rulerH) {
    VE.drag = { kind: "scrub" };
    seek(ms);
    startDragAutoscroll(e.clientX);
    return;
  }
  VE.drag = { kind: "select", anchor: ms, startX: e.clientX, moved: false };
  startDragAutoscroll(e.clientX);
});

regionsEl.addEventListener("pointermove", (e) => {
  if (!VE.drag) return;
  dragClientX = e.clientX;
  applyTimelineDrag(e.clientX);
});

function applyTimelineDrag(clientX) {
  const drag = VE.drag;
  if (!drag) return;
  const ms = timelineMs(clientX);
  if (drag.kind === "scrub") { seek(ms); return; }
  if (drag.kind === "resize") {
    const grabbed = clamp(pxToMs(clientX - $("ve-track").getBoundingClientRect().left - drag.grabPx), 0, VE.dur);
    if (drag.edge === "a") drag.live.startMs = clamp(grabbed, 0, drag.live.endMs - MIN_CLIP_MS);
    else drag.live.endMs = clamp(grabbed, drag.live.startMs + MIN_CLIP_MS, VE.dur);
    const el = barEls.get(drag.op);
    if (el) {
      el.style.left = `${msToPx(drag.live.startMs)}px`;
      el.style.width = `${Math.max(2, msToPx(drag.live.endMs - drag.live.startMs))}px`;
    }
    return;
  }
  if (drag.kind === "select") {
    if (!drag.moved && Math.abs(clientX - drag.startX) < DRAG_SLOP_PX) return;
    drag.moved = true;
    VE.sel = { a: Math.min(drag.anchor, ms), b: Math.max(drag.anchor, ms) };
    renderRegions();
    return;
  }
  if (drag.kind === "trim") {
    const grabbed = clamp(pxToMs(clientX - $("ve-track").getBoundingClientRect().left - drag.grabPx), 0, VE.dur);
    if (drag.edge === "a") drag.live.startMs = clamp(grabbed, 0, drag.live.endMs - MIN_CLIP_MS);
    else drag.live.endMs = clamp(grabbed, drag.live.startMs + MIN_CLIP_MS, VE.dur);
    $("ve-trim-a").style.left = `${msToPx(drag.live.startMs)}px`;
    $("ve-trim-b").style.left = `${msToPx(drag.live.endMs)}px`;
    setTimeValue($("ve-trim-a"), drag.live.startMs, VE.dur);
    setTimeValue($("ve-trim-b"), drag.live.endMs, VE.dur);
    $("ve-dim-a").style.width = `${msToPx(drag.live.startMs)}px`;
    $("ve-dim-b").style.left = `${msToPx(drag.live.endMs)}px`;
    $("ve-dim-b").style.width = `${Math.max(0, trackWidth() - msToPx(drag.live.endMs))}px`;
  }
}

function endTimelineDrag() {
  const drag = VE.drag;
  if (!drag) return;
  VE.drag = null;
  stopDragAutoscroll();
  if (drag.kind === "resize") {
    const op = drag.op;
    const a = Math.min(op.startMs, op.endMs), b = Math.max(op.startMs, op.endMs);
    // A press on an edge that did not move it is no edit: no undo step, no dirty mark.
    if (drag.live.startMs === a && drag.live.endMs === b) return;
    const match = op.tool === "overlay" ? (o) => o.id === op.id : (o) => o === op;
    pushReplacing(match, { ...op, startMs: drag.live.startMs, endMs: drag.live.endMs }, { keepEditor: true });
    return;
  }
  if (drag.kind === "select") {
    if (!drag.moved) seek(drag.anchor);
    else if (VE.sel && VE.sel.b - VE.sel.a < MIN_CLIP_MS) { VE.sel = null; renderRegions(); }
    return;
  }
  if (drag.kind === "trim") {
    // A click on a handle that did not move it is no edit: no undo step, no dirty mark.
    const cur = opOf("trim");
    const a = cur ? cur.startMs : 0, b = cur ? cur.endMs : VE.dur;
    if (drag.live.startMs !== a || drag.live.endMs !== b) setTrim(drag.live.startMs, drag.live.endMs);
  }
}

regionsEl.addEventListener("pointerup", endTimelineDrag);
regionsEl.addEventListener("pointercancel", endTimelineDrag);

// Ctrl+wheel zooms around the playhead; a plain wheel scrolls the timeline
// sideways, which is the only direction it can go.
timelineEl.addEventListener("wheel", (e) => {
  if (e.ctrlKey) {
    e.preventDefault();
    // The zoom keeps the moment under the pointer where it is.
    setZoom(VE.zoom * (e.deltaY < 0 ? 1.25 : 0.8), timelineMs(e.clientX));
  } else if (e.deltaY && !e.shiftKey) {
    e.preventDefault();
    timelineEl.scrollLeft += e.deltaY;
  }
}, { passive: false });

// ---------------------------------------------------------------- edit panel

// Moves one trim edge by `step` and stops it a minimum clip away from the other one, so the
// opposite handle never gets pushed along.
function nudgeTrim(trim, edge, step) {
  if (edge === "a") return [clamp(trim.startMs + step, 0, trim.endMs - MIN_CLIP_MS), trim.endMs];
  return [trim.startMs, clamp(trim.endMs + step, trim.startMs + MIN_CLIP_MS, VE.dur)];
}

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
  if (!isUncommitted($("ve-trim-start-val"))) $("ve-trim-start-val").value = fmtTime(a);
  if (!isUncommitted($("ve-trim-end-val"))) $("ve-trim-end-val").value = fmtTime(b);

  const crop = opOf("crop");
  $("ve-crop-on").checked = !!crop;
  $("ve-crop-fields").classList.toggle("ve-off", !crop);
  for (const input of $("ve-crop-fields").querySelectorAll("input, button")) input.disabled = !crop;
  const rect = crop ? crop.rect : { x: 0, y: 0, w: VE.w, h: VE.h };
  if (!isUncommitted($("ve-crop-fields"))) {
    $("ve-crop-x").value = rect.x;
    $("ve-crop-y").value = rect.y;
    $("ve-crop-w").value = rect.w;
    $("ve-crop-h").value = rect.h;
    for (const id of ["ve-crop-x", "ve-crop-y", "ve-crop-w", "ve-crop-h"]) $(id).numFieldRefresh?.();
  }

  const speed = opOf("speed");
  const rate = speed ? speed.rate : 1;
  for (const btn of $("ve-speed-steps").querySelectorAll("button")) {
    btn.classList.toggle("sel", Math.abs(Number(btn.dataset.rate) - rate) < 0.001);
  }
  if (!isUncommitted($("ve-speed-custom"))) {
    $("ve-speed-custom").value = rate;
    $("ve-speed-custom").numFieldRefresh?.();
  }
  $("ve-speed-pitch").checked = speed ? speed.pitch !== false : true;

  const fade = opOf("fade");
  $("ve-fade-on").checked = !!fade;
  $("ve-fade-fields").classList.toggle("ve-off", !fade);
  for (const input of $("ve-fade-fields").querySelectorAll("input")) input.disabled = !fade;
  if (fade) {
    if (!isUncommitted($("ve-fade-in"))) {
      $("ve-fade-in").value = fade.inMs;
      $("ve-fade-in").numFieldRefresh?.();
    }
    if (!isUncommitted($("ve-fade-out"))) {
      $("ve-fade-out").value = fade.outMs;
      $("ve-fade-out").numFieldRefresh?.();
    }
  }
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

const SCALE_HEIGHT_PRESETS = [2160, 1440, 1080, 720, 480, 360];
const SCALE_PERCENT_PRESETS = [50, 25];
const AUDIO_KBPS_ESTIMATE = 128;

function renderScalePresets() {
  const wrap = $("ve-scale-presets");
  wrap.innerHTML = "";
  const addBtn = (key, label) => {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "ghost-btn ve-step";
    btn.dataset.key = key;
    btn.textContent = label;
    btn.addEventListener("click", () => {
      applyScalePresetKey(key);
      persistExportSettings();
      updateExportNote();
    });
    wrap.appendChild(btn);
  };
  addBtn("", t("veScaleOff"));
  for (const h of SCALE_HEIGHT_PRESETS) addBtn(String(h), t(`optVeScale${h}`));
  for (const pct of SCALE_PERCENT_PRESETS) addBtn(String(pct), t(`optVeScale${pct}pct`));
  updateScaleCurrent();
}

// The one place a scale op becomes literal pixels: a fixed-height preset gives its own size,
// smaller or larger than the source (a region recording is often below 720 px, and refusing to
// enlarge would make every preset "keep original"). Only a preset equal to the source height
// needs no op. A percent value applies as typed.
function scaleForHeightPreset(h) {
  const eff = effectiveSize();
  if (!eff.w || !eff.h || h === eff.h) return null;
  return { w: even(Math.round((eff.w * h) / eff.h)), h: even(h) };
}
function scaleForPercent(pct) {
  const eff = effectiveSize();
  if (!eff.w || !eff.h) return null;
  return { w: even(Math.round((eff.w * pct) / 100)), h: even(Math.round((eff.h * pct) / 100)) };
}

// Returns the preset key matching the current scale op exactly ("" for no
// op, a height/percent key), or null when it is a custom/free-typed value --
// used both to persist the last choice and to highlight the preset buttons.
function currentScalePresetKey() {
  const scale = opOf("scale");
  if (!scale) return "";
  for (const h of SCALE_HEIGHT_PRESETS) {
    const target = scaleForHeightPreset(h);
    if (target && target.w === scale.w && target.h === scale.h) return String(h);
  }
  for (const pct of SCALE_PERCENT_PRESETS) {
    const target = scaleForPercent(pct);
    if (target && target.w === scale.w && target.h === scale.h) return String(pct);
  }
  return null;
}

// The saved default scale is the starting state of a freshly loaded source, not an edit: it is
// locked below the undo floor and leaves the editor clean, so opening and closing a recording
// untouched asks nothing, stores no edit state and one Undo cannot silently drop it.
export function applyDefaultScale(key) {
  const pristine = !VE.dirty && !VE.ops.length;
  applyScalePresetKey(key);
  if (!pristine) return;
  VE.floor = VE.ops.length;
  VE.redo = [];
  VE.dirty = false;
  $("ve-undo").disabled = true;
  $("ve-redo").disabled = true;
}

export function applyScalePresetKey(key) {
  if (!key) { setScaleOp(null, null); return; }
  const keyNum = Number(key);
  if (SCALE_HEIGHT_PRESETS.includes(keyNum)) {
    const target = scaleForHeightPreset(keyNum);
    if (target) setScaleOp(target.w, target.h);
    else setScaleOp(null, null);
    return;
  }
  const pct = SCALE_PERCENT_PRESETS.includes(keyNum) ? scaleForPercent(keyNum) : null;
  if (pct) setScaleOp(pct.w, pct.h);
}

function updateScaleCurrent() {
  const scale = opOf("scale");
  const eff = effectiveSize();
  if (!isUncommitted($("ve-scale-w"))) { $("ve-scale-w").value = scale ? scale.w : eff.w || ""; $("ve-scale-w").numFieldRefresh?.(); }
  if (!isUncommitted($("ve-scale-h"))) { $("ve-scale-h").value = scale ? scale.h : eff.h || ""; $("ve-scale-h").numFieldRefresh?.(); }
  if (!isUncommitted($("ve-scale-pct"))) {
    const pct = scale && eff.w ? Math.round((scale.w / eff.w) * 100) : 100;
    $("ve-scale-pct").value = pct;
    $("ve-scale-pct").numFieldRefresh?.();
  }

  const line = $("ve-scale-current");
  if (!eff.w || !eff.h) {
    line.textContent = "";
  } else if (!scale) {
    line.textContent = t("veScaleCurrentOriginal").replace("{sw}", () => eff.w).replace("{sh}", () => eff.h);
  } else {
    const kbps = videoKbps();
    const durS = totalOutMs() / 1000;
    let text = kbps != null && durS > 0
      ? t("veScaleCurrent").replace("{size}", fmtSizeCompact(Math.round(((kbps + AUDIO_KBPS_ESTIMATE) * 1000 / 8) * durS), LANG))
      : t("veScaleCurrentNoSize");
    text = text.replace("{sw}", () => eff.w).replace("{sh}", () => eff.h).replace("{tw}", () => scale.w).replace("{th}", () => scale.h);
    if (scale.w > eff.w || scale.h > eff.h) text += " " + t("veScaleUpscaleNote");
    line.textContent = text;
  }

  const key = currentScalePresetKey();
  // Only a selected height preset that is taller than the source gets the hint.
  $("ve-scale-uphint").classList.toggle("hidden", !(scale && key !== null && SCALE_HEIGHT_PRESETS.includes(Number(key)) && scale.h > eff.h));
  for (const btn of $("ve-scale-presets").querySelectorAll("button")) {
    btn.classList.toggle("sel", key !== null && btn.dataset.key === key);
  }
  // The size the file will have: the chosen target, or the (cropped) source size when unchanged.
  const outSize = scale || eff;
  $("ve-export-scale-summary").textContent = t("veExportScaleSummary").replace("{w}", () => outSize.w).replace("{h}", () => outSize.h);
}

// ---------------------------------------------------------------- panels + wiring
$("ve-ffmpeg-get").addEventListener("click", (e) => {
  runFfmpegDownload(e.currentTarget, {
    t,
    onDone: async () => {
      VE.ffmpeg = true;
      $("ve-ffmpeg-warn").classList.add("hidden");
      applyExportGate();
      $("ve-still").disabled = false;
      toast(t("ffmpegDownloadDone"));
      await loadRecording(VE.src);
    },
    onError: (err) => toast(errMessageLogged(err, t, "video")),
  });
});

export function showPanel(id) {
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
// The window has no OS title bar, so the bar carries minimize and maximize/restore too.
syncWinButtons = wireWinButtons({ win: appWin, ...insertWinButtons($("ve-close"), "ve"), translate: t });
$("ve-shortcuts").addEventListener("click", () => openShortcutsModal());
$("ve-help-nav").addEventListener("click", (e) => { const b = e.target.closest(".ve-help-tab"); if (b) setHelpSection(b.dataset.help, false); });
$("shortcuts-modal-close").addEventListener("click", closeShortcutsModal);
$("shortcuts-modal").addEventListener("pointerdown", (e) => { if (e.target === $("shortcuts-modal")) closeShortcutsModal(); });
$("ve-undo").addEventListener("click", undo);
$("ve-redo").addEventListener("click", redo);
$("ve-play").addEventListener("click", togglePlay);
const keptStart = () => keptSegments()[0].a;
const keptEnd = () => keptSegments().slice(-1)[0].b;
$("ve-to-start").addEventListener("click", () => seek(keptStart()));
$("ve-to-end").addEventListener("click", () => seek(keptEnd()));
// Stepping by a fixed frameMs() from a scrubbed playhead drifts off the frame grid, so snap to
// the real previous/next frame boundary. The small epsilon absorbs float error from a
// non-integer frame duration (e.g. 29.97fps), so a playhead already on a boundary still
// advances a whole frame.
function stepFrame(dir) {
  const fm = frameMs();
  const eps = fm / 100;
  const idx = dir > 0 ? Math.floor((VE.playhead + eps) / fm) + 1 : Math.ceil((VE.playhead - eps) / fm) - 1;
  seek(Math.max(0, idx * fm));
}
$("ve-prev-frame").addEventListener("click", () => stepFrame(-1));
$("ve-next-frame").addEventListener("click", () => stepFrame(1));

// Marking only makes sense on the kept part: the toast says why a press did nothing.
function playheadInKept() {
  const { a, b } = trimBounds();
  if (VE.playhead >= a && VE.playhead <= b) return true;
  toast(t("veMarkOutside"));
  return false;
}
$("ve-mark-in").addEventListener("click", () => {
  if (!playheadInKept()) return;
  const b = VE.sel && VE.sel.b > VE.playhead ? VE.sel.b : trimBounds().b;
  VE.sel = { a: VE.playhead, b };
  renderRegions();
});
$("ve-mark-out").addEventListener("click", () => {
  if (!playheadInKept()) return;
  const a = VE.sel && VE.sel.a < VE.playhead ? VE.sel.a : trimBounds().a;
  VE.sel = { a, b: VE.playhead };
  renderRegions();
});
$("ve-sel-clear").addEventListener("click", () => { VE.sel = null; renderRegions(); renderOverlayEditor(); });
$("ve-trim-set").addEventListener("click", () => {
  const range = markedRange();
  if (!range) { toast(t("veNeedSelection")); return; }
  // Cleared before the push, like a cut: the band must not outlive the edit it produced.
  VE.sel = null;
  setTrim(range.startMs, range.endMs);
});
// Cuts the marked range out of the kept part. A range sticking out of the trim is clamped to it,
// one entirely outside says so, and a cut touching earlier cuts becomes one cut with them.
export function cutSelection() {
  const range = markedRange();
  if (!range) { toast(t("veNeedSelection")); return; }
  const bounds = trimBounds();
  let startMs = Math.max(range.startMs, bounds.a);
  let endMs = Math.min(range.endMs, bounds.b);
  if (endMs - startMs < MIN_CLIP_MS) { toast(t("veCutOutside")); return; }
  const touching = opsOf("cut").filter((c) => Math.min(c.startMs, c.endMs) <= endMs && Math.max(c.startMs, c.endMs) >= startMs);
  for (const c of touching) {
    startMs = Math.min(startMs, c.startMs, c.endMs);
    endMs = Math.max(endMs, c.startMs, c.endMs);
  }
  // Cleared before the push: pushOp redraws the timeline, and the band must not outlive the cut.
  VE.sel = null;
  if (touching.length) replaceOps((o) => touching.includes(o), { tool: "cut", startMs, endMs });
  else pushOp({ tool: "cut", startMs, endMs });
}
$("ve-cut-add").addEventListener("click", cutSelection);
$("ve-cut-clear").addEventListener("click", () => removeOps((o) => o.tool === "cut"));

$("ve-still").addEventListener("click", async () => {
  if ($("ve-still").disabled) return;
  $("ve-still").disabled = true;
  try {
    const saved = await invoke("extract_still_frame", { path: VE.src, atMs: Math.round(VE.playhead) });
    toast(saved ? t("veStillSaved").replace("{path}", () => String(saved)) : t("veStillNotSaved"));
  } catch (err) {
    noteFfmpeg(err);
    toast(errMessageLogged(err, t, "video"));
  } finally {
    // An export in progress keeps the button off on its own, so only a free editor re-enables it.
    $("ve-still").disabled = !!VE.exporting;
  }
});

$("ve-zoom-in").addEventListener("click", () => setZoom(VE.zoom * TIMELINE_ZOOM_STEP));
$("ve-zoom-out").addEventListener("click", () => setZoom(VE.zoom / TIMELINE_ZOOM_STEP));
$("ve-zoom-fit").addEventListener("click", () => setZoom(fitZoom()));

// Hold the button (or V) to peek at the source without overlays, fades or the scale target, or
// tap it to lock that view until pressed again. The current cut still applies; this only
// touches the live preview.
const veOriginalToggle = makeHoldOrTapToggle((shown) => {
  $("ve-overlay-live").classList.toggle("hidden", shown);
  $("ve-rects").classList.toggle("hidden", shown);
  $("ve-original").classList.toggle("active", shown);
  $("ve-original-badge").classList.toggle("hidden", !shown);
});
$("ve-original").addEventListener("pointerdown", () => veOriginalToggle.press());
document.addEventListener("pointerup", () => veOriginalToggle.release());
document.addEventListener("pointercancel", () => veOriginalToggle.release());
window.addEventListener("blur", () => veOriginalToggle.reset());

$("ve-trim-start-val").addEventListener("change", () => {
  const ms = parseTime($("ve-trim-start-val").value);
  const trim = opOf("trim");
  if (ms == null) { renderEditFields(); return; }
  const end = trim ? trim.endMs : VE.dur;
  setTrim(clamp(ms, 0, end - MIN_CLIP_MS), end);
});
$("ve-trim-end-val").addEventListener("change", () => {
  const ms = parseTime($("ve-trim-end-val").value);
  const trim = opOf("trim");
  if (ms == null) { renderEditFields(); return; }
  const start = trim ? trim.startMs : 0;
  setTrim(start, clamp(ms, start + MIN_CLIP_MS, VE.dur));
});

// Decorate the static crop/speed/fade fields with the shared mouse-only stepper before wiring
// their own change listeners, so its clamp runs first and the domain listener reads an
// already-clamped value. Each keeps its id, so existing $(id) lookups and listeners still work.
const numFieldDec = t("numFieldDecrease");
const numFieldInc = t("numFieldIncrease");
const numFieldList = t("numFieldPresets");
// Read on each wheel turn: `settings` is still null when these fields are built.
const numFieldWheelOn = () => settings?.ui_flags?.numFieldWheel !== false;
for (const id of ["ve-crop-x", "ve-crop-y", "ve-crop-w", "ve-crop-h"]) {
  numberField({
    input: $(id), bigStep: 20, decreaseLabel: numFieldDec, increaseLabel: numFieldInc,
    decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease", wheelEnabled: numFieldWheelOn,
  });
}
numberField({
  input: $("ve-speed-custom"), bigStep: 0.5, presets: [0.25, 0.5, 1, 1.5, 2, 4],
  decreaseLabel: numFieldDec, increaseLabel: numFieldInc, presetsLabel: numFieldList,
  decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease", presetsAriaKey: "numFieldPresets",
  wheelEnabled: numFieldWheelOn,
});
for (const id of ["ve-fade-in", "ve-fade-out"]) {
  numberField({
    input: $(id), bigStep: 500, presets: [250, 500, 1000, 2000],
    decreaseLabel: numFieldDec, increaseLabel: numFieldInc, presetsLabel: numFieldList,
    decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease", presetsAriaKey: "numFieldPresets",
    wheelEnabled: numFieldWheelOn,
  });
}
for (const id of ["ve-scale-w", "ve-scale-h"]) {
  numberField({
    input: $(id), bigStep: 20, decreaseLabel: numFieldDec, increaseLabel: numFieldInc,
    decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease", wheelEnabled: numFieldWheelOn,
  });
}
numberField({
  input: $("ve-scale-pct"), bigStep: 25, decreaseLabel: numFieldDec, increaseLabel: numFieldInc,
  decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease", wheelEnabled: numFieldWheelOn,
});

// Every crop edit goes through here: the scale op was worked out from the crop it was set
// against, so it is worked out again for the new crop. A scale that matches a preset keeps
// that preset; a free size with the ratio lock keeps its height and takes the new aspect; a
// free size without the lock is a deliberate distortion and stays. Each is its own undo step.
export function changeCrop(change) {
  const scale = opOf("scale");
  const key = scale ? currentScalePresetKey() : "";
  const before = effectiveSize();
  change();
  if (!scale || !opOf("scale")) return;
  const now = effectiveSize();
  if (before.w === now.w && before.h === now.h) return;
  if (key) applyScalePresetKey(key);
  else if (key === null && $("ve-scale-lock").checked && now.h) setScaleOp((scale.h * now.w) / now.h, scale.h);
}
export const pushCrop = (rect) => changeCrop(() => pushReplacing((o) => o.tool === "crop", { tool: "crop", rect }));

$("ve-crop-on").addEventListener("change", () => {
  if ($("ve-crop-on").checked) pushCrop(centeredRect(0.8));
  else changeCrop(() => removeOps((o) => o.tool === "crop"));
});
$("ve-crop-reset").addEventListener("click", () => {
  pushCrop(snapRect({ x: 0, y: 0, w: VE.w, h: VE.h }));
});
for (const id of ["ve-crop-x", "ve-crop-y", "ve-crop-w", "ve-crop-h"]) {
  $(id).addEventListener("change", () => {
    const rect = snapRect({
      x: Number($("ve-crop-x").value) || 0,
      y: Number($("ve-crop-y").value) || 0,
      w: Number($("ve-crop-w").value) || VE.w,
      h: Number($("ve-crop-h").value) || VE.h,
    });
    pushCrop(rect);
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
    // An empty or unreadable field means the default; a typed 0 is a deliberate "no fade on this side".
    const fadeField = (id) => {
      const raw = $(id).value.trim();
      return raw !== "" && Number.isFinite(Number(raw)) ? Number(raw) : fadeDef;
    };
    pushReplacing((o) => o.tool === "fade", {
      tool: "fade",
      inMs: clamp(fadeField("ve-fade-in"), 0, 10000),
      outMs: clamp(fadeField("ve-fade-out"), 0, 10000),
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

// The Edit tab's width/height/percent fields and the Export tab's read-only summary are two
// faces of the one "scale" op; this is the only place that writes it, with no size guard.
function setScaleOp(w, h) {
  if (w == null || h == null) { removeOps((o) => o.tool === "scale"); return; }
  const cw = even(clamp(Math.round(w), 16, 7680));
  const ch = even(clamp(Math.round(h), 16, 7680));
  // Back at the size the crop leaves (the source size without a crop) is no scaling at all,
  // so the entry goes away.
  const eff = effectiveSize();
  if (eff.w && eff.h && cw === even(eff.w) && ch === even(eff.h)) { removeOps((o) => o.tool === "scale"); return; }
  pushReplacing((o) => o.tool === "scale", { tool: "scale", w: cw, h: ch });
}

$("ve-scale-w").addEventListener("change", () => {
  const eff = effectiveSize();
  const w = Number($("ve-scale-w").value) || eff.w;
  const h = $("ve-scale-lock").checked && eff.w && eff.h ? Math.round((w * eff.h) / eff.w) : (opOf("scale") || { h: eff.h }).h;
  setScaleOp(w, h);
  persistExportSettings();
  updateExportNote();
});
$("ve-scale-h").addEventListener("change", () => {
  const eff = effectiveSize();
  const h = Number($("ve-scale-h").value) || eff.h;
  const w = $("ve-scale-lock").checked && eff.w && eff.h ? Math.round((h * eff.w) / eff.h) : (opOf("scale") || { w: eff.w }).w;
  setScaleOp(w, h);
  persistExportSettings();
  updateExportNote();
});
$("ve-scale-pct").addEventListener("change", () => {
  const pct = clamp(Number($("ve-scale-pct").value) || 100, 10, 400);
  const target = scaleForPercent(pct);
  if (target) { setScaleOp(target.w, target.h); persistExportSettings(); updateExportNote(); }
});
$("ve-scale-lock").addEventListener("change", () => {
  const eff = effectiveSize();
  if (!$("ve-scale-lock").checked || !eff.w || !eff.h) return;
  const scale = opOf("scale");
  if (!scale) return;
  // Re-locking snaps the current values back onto the aspect ratio of the cropped picture
  // immediately, keyed off the width so both fields agree afterward.
  setScaleOp(scale.w, Math.round((scale.w * eff.h) / eff.w));
  persistExportSettings();
});

$("ve-ov-add-image").addEventListener("click", () => addOverlay("image"));
$("ve-ov-add-frame").addEventListener("click", () => addOverlay("frame"));
$("ve-ov-add-text").addEventListener("click", () => addOverlay("text"));
$("ve-ov-add-pixelate").addEventListener("click", () => addOverlay("pixelate"));
$("ve-ov-add-spotlight").addEventListener("click", () => addOverlay("spotlight"));
$("ve-ov-add-keys").addEventListener("click", () => addOverlay("keys"));
$("ve-add-video").addEventListener("click", (e) => {
  const r = e.currentTarget.getBoundingClientRect();
  startAddVideo(r.left, r.bottom);
});

// A mute button's tooltip names what a click will do: Mute while the track plays, Unmute once it is
// muted. Its accessible name stays "Mute ..." and aria-pressed says whether it is on.
function muteKey(id, key, kind) {
  if (!id.startsWith("ve-mute-") || !VE.audio[kind].mute) return key;
  return key === "veMuteMic" ? "veUnmuteMic" : "veUnmuteSystem";
}

// A recording without a microphone track leaves those controls off and says why, instead of
// offering a slider that could change nothing. An export format that drops audio (gif) does
// the same to both tracks' controls, on top of any per-track reason.
export function updateAudioLabels() {
  const silent = SILENT_FORMATS.has(exportFormat());
  for (const [id, key] of [["ve-mute-mic", "veMuteMic"], ["ve-vol-mic", "veVolumeMic"]]) {
    const el = $(id);
    el.disabled = !VE.hasMic || silent;
    const reason = silent ? t("veNoAudioNote") : VE.hasMic ? "" : t("veNoMicTrack");
    // The accessible name stays fixed and aria-pressed carries the muted state; only the
    // tooltip says what the next click does.
    el.title = reason || t(muteKey(id, key, "mic"));
    el.setAttribute("aria-label", reason || t(key));
  }
  for (const [id, key] of [["ve-mute-sys", "veMuteSystem"], ["ve-vol-sys", "veVolumeSystem"]]) {
    const el = $(id);
    el.disabled = !VE.hasSys || silent;
    const reason = silent ? t("veNoAudioNote") : VE.hasSys ? "" : t("veNoAudioTrack");
    // The accessible name stays fixed and aria-pressed carries the muted state; only the
    // tooltip says what the next click does.
    el.title = reason || t(muteKey(id, key, "sys"));
    el.setAttribute("aria-label", reason || t(key));
  }
}

// Reflects VE.audio in the mute buttons/sliders; needed on every load because
// resetForNewSource() changes the state without a click triggering audioControl()'s listener.
function updateVolumeValueText(volId) {
  $(volId).setAttribute("aria-valuetext", t("a11yPctOnly").replace("{pct}", () => $(volId).value));
}

function syncMuteButton(kind, id) {
  $(id).classList.toggle("ve-muted-btn", VE.audio[kind].mute);
  $(id).setAttribute("aria-pressed", VE.audio[kind].mute ? "true" : "false");
}

function syncAudioButtons() {
  syncMuteButton("sys", "ve-mute-sys");
  syncMuteButton("mic", "ve-mute-mic");
  updateAudioLabels();
  $("ve-vol-sys").value = VE.audio.sys.vol;
  $("ve-vol-mic").value = VE.audio.mic.vol;
  updateVolumeValueText("ve-vol-sys");
  updateVolumeValueText("ve-vol-mic");
}

function audioControl(kind, muteId, volId) {
  $(muteId).addEventListener("click", () => {
    VE.audio[kind].mute = !VE.audio[kind].mute;
    syncMuteButton(kind, muteId);
    updateAudioLabels();
    applyPreviewSpeed();
    updateExportNote();
  });
  $(volId).addEventListener("input", () => {
    VE.audio[kind].vol = Number($(volId).value);
    updateVolumeValueText(volId);
    applyPreviewSpeed();
    updateExportNote();
  });
}
audioControl("sys", "ve-mute-sys", "ve-vol-sys");
audioControl("mic", "ve-mute-mic", "ve-vol-mic");

fillVideoFormatSelect($("ve-export-format"));
for (const id of ["ve-export-format", "ve-export-quality"]) {
  attachSelectPicker($(id));
  $(id).addEventListener("change", () => {
    // A full manual destination names its own extension, so a different container format makes
    // it wrong: only that extension follows the format, the folder and name the user chose stay.
    if (id === "ve-export-format" && VE.destOverride) VE.destOverride = withExtension(VE.destOverride, formatExt(exportFormat()));
    persistExportSettings();
    updateExportNote();
  });
}

// A plain file name, checked live as typed. It wins over the auto "<source stem>-edit" guess
// but stays under the full-path field below, like picking a folder (destPath() reads it through
// exportNameStem()). Typing a full path into that field still takes over completely.
$("ve-export-name").addEventListener("input", () => {
  VE.exportNameOverride = $("ve-export-name").value;
  VE.destOverride = null;
  $("ve-export-dest").value = destPath();
  $("ve-export-dest").title = destPath();
  applyExportGate();
});
// Leaving the field drops a typed ".<ext>" of the chosen format, so the shown name is the stem.
$("ve-export-name").addEventListener("change", () => {
  const stem = stripFormatExt($("ve-export-name").value, formatExt(exportFormat()));
  $("ve-export-name").value = stem;
  VE.exportNameOverride = stem;
  $("ve-export-dest").value = destPath();
  $("ve-export-dest").title = destPath();
  applyExportGate();
});
// The target field is an input: typing a full path is a manual override, like picking one via
// the dialog below.
$("ve-export-dest").addEventListener("change", () => {
  const value = $("ve-export-dest").value.trim();
  // A typed name with another extension would write one container under another's name.
  VE.destOverride = value ? withExtension(value, formatExt(exportFormat())) : null;
  $("ve-export-dest").value = destPath();
  updateExportNote();
});
$("ve-export-tile").addEventListener("change", persistExportSettings);

$("ve-export-choose-folder").addEventListener("click", async () => {
  const folder = await invoke("pick_export_folder").catch(() => null);
  if (!folder) return;
  VE.destDir = folder;
  VE.destOverride = null; // the auto filename moves into the new folder
  // A folder picked here beats the suggested folder of an externally opened file.
  VE.originDir = "";
  // The backend only accepts this folder from the setter call that follows its own picker in the
  // same flow, so it is sent here once rather than via persistExportSettings, which resends the
  // unchanged VE.destDir on every unrelated change and would be refused after the first.
  invoke("set_video_export_dir", { dir: folder }).catch(() => {});
  persistExportSettings();
  updateExportNote();
});

function applyPreset(format, quality, downscale) {
  $("ve-export-format").value = format;
  // Setting the value fires no change event, so the extension fix-up of the format select runs here.
  if (VE.destOverride) VE.destOverride = withExtension(VE.destOverride, formatExt(exportFormat()));
  $("ve-export-quality").value = quality;
  // These presets are about a smaller file: they only ever shrink, so a
  // source already at or below their height keeps its own size.
  const shrink = downscale && effectiveSize().h && Number(downscale) < effectiveSize().h ? downscale : "";
  applyScalePresetKey(shrink);
  persistExportSettings();
  updateExportNote();
}
// Presets only fill the manual controls; there is no second export path.
$("ve-preset-same").addEventListener("click", () => applyPreset("mp4", "high", ""));
$("ve-preset-small").addEventListener("click", () => applyPreset("mp4", "small", "720"));
$("ve-preset-gif").addEventListener("click", () => applyPreset("gif", "small", "480"));

$("ve-export-start").addEventListener("click", () => startExport(false));
$("ve-export-clip").addEventListener("click", () => startExport(true));
// The Stop button only shows while an export runs (syncExportButton), so a click always means stop.
function stopExport() {
  if (VE.exporting) invoke("cancel_export").catch(() => {});
}
$("ve-export-cancel").addEventListener("click", stopExport);
$("ve-export-saveas").addEventListener("click", async () => {
  if (!VE.lastExport) return;
  const moved = await invoke("save_recording_as", { srcPath: VE.lastExport }).catch((err) => { toast(String(err).includes("source-not-allowed") ? t("veMoveNotAllowed") : errMessageLogged(err, t, "video")); return null; });
  if (moved) {
    VE.lastExport = moved;
    $("ve-export-status").textContent = t("veExportDone").replace("{path}", () => moved);
  }
});

function persistExportSettings() {
  invoke("set_ui_text", { key: "videoExportFormat", value: exportFormat() }).catch(() => {});
  invoke("set_ui_text", { key: "videoExportQuality", value: exportQuality() }).catch(() => {});
  invoke("set_ui_value", { key: "veExportTile", value: $("ve-export-tile").checked ? 1 : 0 }).catch(() => {});
  // A custom free-typed size has no preset to remember; it falls back to "keep original" next session.
  invoke("set_ui_text", { key: "videoExportScale", value: currentScalePresetKey() || "" }).catch(() => {});
  // videoExportDir is not resent here; the choose-folder handler above is the only place VE.destDir changes.
}

// ---------------------------------------------------------------- keyboard

// The window may be minimized or behind another one when a close the page did not start (the
// taskbar entry, the app quitting) needs its question answered.
async function bringForward() {
  await appWin.unminimize().catch(() => {});
  await appWin.show().catch(() => {});
  await appWin.setFocus().catch(() => {});
}

// Every close path ends here: the titlebar button, Escape, and (through the backend's
// "ve-close-requested") Alt+F4, the taskbar entry and the app quitting. `quit` is true when the
// app itself is on its way out: a running export then needs no extra question (the app's own quit
// dialog covers running jobs), and an unsaved-edits question still applies. Edits count as
// unsaved until an export covers them, whether or not the autosave stored them.
let closeAsking = false;
async function closeWindow(opts) {
  const quit = !!(opts && opts.quit);
  if (closeAsking) return;
  closeAsking = true;
  try {
    await closeWindowAsked(quit);
  } finally {
    closeAsking = false;
  }
}

async function closeWindowAsked(quit) {
  let discarded = false;
  const keepOpen = () => { invoke("video_editor_close_ack", { state: "cancel" }).catch(() => {}); };
  if (VE.exporting && !quit) {
    await bringForward();
    // Three-way choice: keep it running in the background (the export is backend-driven and
    // survives this window closing), cancel it and close, or stay put. "Keep running" is the
    // default (Enter). With the edit state not kept, edits made after the export started would be
    // lost on close, so that is said too.
    const unsaved = VE.dirty && !VE.editStateKeepOn ? ` ${t("veCloseUnsaved")}` : "";
    const choice = await confirmDialog({
      title: t("veCloseTitle"),
      message: t("veCloseExporting") + unsaved,
      confirmLabel: t("bgKeepRunning"),
      cancelLabel: t("bgStayOpen"),
      altLabel: t("bgCancelClose"),
      danger: false,
    });
    if (choice === false) { keepOpen(); return; }
    if (choice === "alt") invoke("cancel_export").catch(() => {});
  } else if (VE.ops.length && VE.dirty) {
    await bringForward();
    const ok = await confirmDialog({ title: t("veCloseTitle"), message: t("veCloseUnsaved"), confirmLabel: t("veDiscard"), danger: true });
    if (!ok) { keepOpen(); return; }
    discarded = true;
  }
  if (discarded) {
    // The user threw these edits away: stop the autosave and forget what an earlier tick
    // may already have stored, or the next open would offer the discarded work back.
    stopEditStateAutosave();
    if (VE.src) await invoke("discard_edit_state", { kind: "video", target: VE.src }).catch(() => {});
  } else {
    // Best-effort: this saves the last few seconds the periodic autosave (armEditStateAutosave)
    // has not stored yet.
    await saveEditStateNow();
  }
  // The backend holds back every close until the page allows this one.
  await invoke("video_editor_close_ack", { state: "allow" }).catch(() => {});
  appWin.close().catch(() => {});
}


// Escape in a field gives back what the field held when it got focus (or when it last committed),
// so a half-typed file name or time is abandoned instead of taking the whole editor with it.
const NON_TEXT_INPUT = new Set(["checkbox", "radio", "range", "button", "submit", "color", "file"]);
const isTextField = (el) => !!el && (el.tagName === "TEXTAREA" || (el.tagName === "INPUT" && !NON_TEXT_INPUT.has(el.type)));
// Controls that use the plain letter, arrow and Space keys themselves. A checkbox or button does
// not, so the editor's shortcuts must keep working after one was clicked.
const KEY_INPUT_TYPES = new Set(["range", "radio"]);
const keepsOwnKeys = (el) => isTextField(el) || el?.tagName === "SELECT" || (el?.tagName === "INPUT" && KEY_INPUT_TYPES.has(el.type));
let fieldFocus = null;
document.addEventListener("focusin", (e) => { fieldFocus = isTextField(e.target) ? { el: e.target, value: e.target.value } : null; });
document.addEventListener("change", (e) => { if (fieldFocus && fieldFocus.el === e.target) fieldFocus.value = e.target.value; }, true);

// Escape peels off one layer at a time: the field being typed in, the selected overlay, the marked
// range, the Export tab, and only when nothing is left the window. Returns the layer it took.
function handleEscape() {
  const el = document.activeElement;
  if (isTextField(el)) {
    if (fieldFocus && fieldFocus.el === el && el.value !== fieldFocus.value) {
      el.value = fieldFocus.value;
      // Only "input": the abandoned text never committed, so a "change" would re-commit the old
      // value as an undo step that changes nothing, clear the redo stack and mark the editor edited.
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }
    el.blur();
    return "field";
  }
  // A selected overlay only gives up the arrow keys, never the window: closing here would throw
  // away every edit just because something is selected.
  if (VE.selOv) { VE.selOv = null; renderOverlayList(); renderOverlayEditor(); renderRects(); renderRegions(); updateNudgeHint(); return "overlay"; }
  if (VE.sel) { VE.sel = null; renderRegions(); renderOverlayEditor(); return "selection"; }
  // The Export tab is a step on top of the edit view: Escape goes back to it first. While an
  // export runs, Escape keeps its cancel-or-close meaning.
  if (!VE.exporting && !$("ve-panel-export").classList.contains("hidden")) { showPanel("ve-panel-edit"); return "panel"; }
  closeWindow();
  return "close";
}

document.addEventListener("keydown", (e) => {
  if (!$("confirm-modal").classList.contains("hidden")) {
    if (e.key === "Escape") { e.preventDefault(); dismissConfirm(); }
    else if (e.key === "Tab") trapTabWithin($("confirm-modal"), e);
    return;
  }
  if (!$("shortcuts-modal").classList.contains("hidden")) {
    if (e.key === "Escape" || e.code === "F1") { e.preventDefault(); closeShortcutsModal(); }
    else if (e.key === "Tab") trapTabWithin($("shortcuts-modal"), e);
    else helpNavKey(e);
    return;
  }
  if (!colorPop.classList.contains("hidden")) {
    if (e.key === "Escape") { e.preventDefault(); closeColorPop(); return; }
    if (e.key === "Tab") { trapTabWithin(colorPop, e); return; }
  }
  if (!ovCtxPop.classList.contains("hidden")) {
    if (e.key === "Escape") { e.preventDefault(); closeOvCtxPop(); return; }
    if (e.key === "Tab") { trapTabWithin(ovCtxPop, e); return; }
    // The menu is about one overlay; arrows and Delete must not edit it behind the menu.
    if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Delete", "Backspace"].includes(e.key)) { e.preventDefault(); return; }
  }
  if (!addVideoPop.classList.contains("hidden")) {
    if (e.key === "Escape") { e.preventDefault(); closeAddVideoPop(); return; }
    if (e.key === "Tab") { trapTabWithin(addVideoPop, e); return; }
  }
  if (e.code === "F1") { e.preventDefault(); openShortcutsModal(); return; }
  const typing = keepsOwnKeys(document.activeElement);
  if (e.key === "Escape") {
    e.preventDefault();
    if (numPopClosedByEscape(e)) return; // that Escape only dismissed a preset list
    handleEscape();
    return;
  }
  if (typing) return;

  // Letter shortcuts follow the letter printed on the key (e.key), so Ctrl+Z undoes on a
  // German keyboard too; letterCode() falls back to the physical key on a non-Latin layout.
  const letter = letterCode(e);
  if ((e.ctrlKey || e.metaKey) && letter === "KeyZ") {
    e.preventDefault();
    if (e.shiftKey) redo(); else undo();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && letter === "KeyY") { e.preventDefault(); redo(); return; }
  // Preview zoom: Ctrl+Plus/Minus/0, like the screenshot editor's canvas zoom; plain +/- stay the
  // timeline zoom below.
  if ((e.ctrlKey || e.metaKey) && (e.key === "+" || e.key === "=")) { e.preventDefault(); setPreviewZoom(VE.previewZoom * VE.zoomStepFactor); return; }
  if ((e.ctrlKey || e.metaKey) && e.key === "-") { e.preventDefault(); setPreviewZoom(VE.previewZoom / VE.zoomStepFactor); return; }
  if ((e.ctrlKey || e.metaKey) && e.key === "0") { e.preventDefault(); resetPreviewZoom(); return; }

  // A focused trim handle takes its own arrow keys instead of stepping the playhead, or the
  // handles would be reachable by Tab but not movable without a mouse.
  const focusedId = document.activeElement && document.activeElement.id;
  if (focusedId === "ve-trim-a" || focusedId === "ve-trim-b") {
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      const dir = e.key === "ArrowLeft" ? -1 : 1;
      const step = (e.shiftKey ? 1000 : frameMs()) * dir;
      const trim = opOf("trim") || { startMs: 0, endMs: VE.dur };
      const [startMs, endMs] = nudgeTrim(trim, focusedId === "ve-trim-a" ? "a" : "b", step);
      setTrim(startMs, endMs);
      return;
    }
  }

  // While an overlay is selected the arrow keys move/resize it instead of stepping frames;
  // frame-stepping moves to comma/period.
  const selectedOverlay = VE.selOv && VE.ops.find((o) => o.id === VE.selOv);
  if (selectedOverlay) {
    if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key)) {
      e.preventDefault();
      nudgeSelectedOverlay(selectedOverlay, e);
      return;
    }
    if (e.key === "," || e.key === ".") {
      e.preventDefault();
      stepFrame(e.key === "," ? -1 : 1);
      return;
    }
  }
  // Delete or Backspace removes the overlay selected on the picture (undoable). A key that a
  // focused row already took for itself (defaultPrevented) is not used twice.
  if (selectedOverlay && !editDragActive() && !e.defaultPrevented && !e.ctrlKey && !e.metaKey && !e.altKey && (e.key === "Delete" || e.key === "Backspace")) {
    e.preventDefault();
    VE.selOv = null;
    deleteOp(selectedOverlay);
    return;
  }
  if (e.ctrlKey || e.metaKey || e.altKey) return;

  switch (e.key) {
    case " ": {
      // A control focused from the keyboard keeps Space for its own activation; after a
      // mouse click the button has no focus ring, and Space still plays.
      const ae = document.activeElement;
      if (ae && ae.matches("button, a[href], [role='button'], summary") && ae.matches(":focus-visible")) break;
      if (ae && ae.matches("input[type=checkbox]")) break; // Space toggles the box
      e.preventDefault();
      togglePlay();
      break;
    }
    case "ArrowLeft": e.preventDefault(); if (e.shiftKey) seek(VE.playhead - 1000); else stepFrame(-1); break;
    case "ArrowRight": e.preventDefault(); if (e.shiftKey) seek(VE.playhead + 1000); else stepFrame(1); break;
    case "Home": e.preventDefault(); seek(e.shiftKey ? 0 : keptStart()); break;
    case "End": e.preventDefault(); seek(e.shiftKey ? VE.dur : keptEnd()); break;
    case "+": case "=": e.preventDefault(); setZoom(VE.zoom * TIMELINE_ZOOM_STEP); break;
    case "-": e.preventDefault(); setZoom(VE.zoom / TIMELINE_ZOOM_STEP); break;
    default: break;
  }
  switch (letterCode(e)) {
    case "KeyI": e.preventDefault(); $("ve-mark-in").click(); break;
    case "KeyO": e.preventDefault(); $("ve-mark-out").click(); break;
    case "KeyC": e.preventDefault(); cutSelection(); break;
    case "KeyV": if (!e.repeat) veOriginalToggle.press(); e.preventDefault(); break;
    default: break;
  }
});
document.addEventListener("keyup", (e) => {
  if (letterCode(e) === "KeyV") veOriginalToggle.release();
});

// ---------------------------------------------------------------- loading

export function resetForNewSource() {
  // A confirm still open from the previous source (the restore question) must not stay up over
  // the new one, and its waiting caller is released (it sees the load generation moved on and
  // does nothing).
  dismissConfirm();
  VE.ops = [];
  VE.redo = [];
  VE.floor = 0;
  VE.sel = null;
  VE.selOv = null;
  veOriginalToggle.reset();
  updateNudgeHint();
  // The live-preview nodes belong to the old source's overlays; leftovers would show a stale
  // image/text/pixelate sampled from a video that is no longer loaded.
  clearLiveOverlays($("ve-overlay-live"));
  VE.strip = [];
  VE.stripCount = 0;
  VE.peaks = { 0: null, 1: null };
  VE.playhead = 0;
  VE.dirty = false;
  // The saved default scale applies to every source, not only the first one the window loads.
  VE.pendingScaleKey = VE.scaleDefaultKey || null;
  VE.lastExport = "";
  // A full manual "Save as…" path (or typed file name) names this source's file, so a newly
  // loaded video starts fresh with both; the remembered destination folder (VE.destDir) is a
  // standing preference and survives across videos.
  VE.destOverride = null;
  VE.exportNameOverride = null;
  // Mic starts muted on every newly loaded video, like the module-level default.
  VE.audio = { sys: { mute: false, vol: 100 }, mic: { mute: true, vol: 100 } };
  syncAudioButtons();
  showPanel(`ve-panel-${VE.defaultPanel || "edit"}`);
  // Probe and geometry are only set by applyProbe() on success; if probing the new source fails
  // (no ffmpeg, a corrupt file), applyProbe(null) skips its assignment, so they are cleared here
  // or they would keep the previous video's values (wrong crop bounds, duration/frame math,
  // audio-track availability).
  VE.probe = null;
  VE.keySidecar = null;
  VE.dur = 0;
  VE.fps = 30;
  VE.w = 0;
  VE.h = 0;
  VE.hasSys = true;
  VE.hasMic = false;
  // A leftover zoom/pan from the previous video would apply to a new source of a different
  // resolution or aspect ratio and show the wrong region.
  VE.previewZoom = 1;
  VE.panPx = { x: 0, y: 0 };
  applyPreviewTransform();
  $("ve-export-status").textContent = "";
  $("ve-export-progress-row").classList.add("hidden");
  $("ve-export-saveas").classList.add("hidden");
}

videoEl.addEventListener("loadedmetadata", () => {
  if (!VE.dur) applyProbe(VE.probe);
  renderRects();
});

window.addEventListener("resize", () => {
  // A pan saved against the old stage size can land outside the new one once the window shrinks
  // (fitBoxSize/clampPan read the live stage size); re-clamp before the next render.
  if (VE.previewZoom !== 1) { VE.panPx = clampPan(VE.panPx, VE.previewZoom); applyPreviewTransform(); }
  renderRects();
  layoutTrack();
});

// ---------------------------------------------------------------- backend events
listen("theme-changed", (e) => { applyTheme(e.payload?.theme, e.payload?.vars); waveColors[0] = waveColors[1] = null; drawWave(0); if (VE.hasMic) drawWave(1); });
listen("language-changed", () => { invoke("get_settings").then((s) => loadTexts(s)).catch(() => {}); });
listen("settings-changed", () => { invoke("get_settings").then((s) => { settings = s; applySettings(s); }).catch(() => {}); });
// Opening a second recording reuses this window instead of stacking another one.
listen("video-editor-load", async (e) => {
  const path = typeof e.payload === "string" ? e.payload : e.payload?.path;
  const originDir = typeof e.payload === "object" ? e.payload?.origin_dir : "";
  const overlayPath = typeof e.payload === "object" ? e.payload?.overlay_path : "";
  // The opener waits for this answer: it only treats the switch as done once taken.
  const reqId = typeof e.payload === "object" ? e.payload?.request_id : null;
  const ack = (state) => { if (reqId != null) invoke("video_editor_load_ack", { requestId: reqId, state }).catch(() => {}); };
  if (!path || path === VE.src) {
    // The recording is already open; a camera recording opened for it afterwards only adds its
    // overlay.
    if (path && overlayPath && VE.dur > 0 && VE.w > 0) addAlignedCameraOverlay(overlayPath, VE.loadGen);
    ack(path ? "accepted" : "refused");
    return;
  }
  // Switching source ("Edit video" on another tile while this window is open) must not silently
  // discard unsaved edits: it guards the same case closeWindow() does, with its exact confirm.
  if (VE.ops.length && VE.dirty) {
    ack("asking");
    const askedSrc = VE.src;
    const ok = await confirmDialog({ title: t("veCloseTitle"), message: t("veCloseUnsaved"), confirmLabel: t("veDiscard"), danger: true });
    if (!ok) { ack("refused"); return; }
    // A second request queued behind the first confirm: once the first was accepted the old
    // edits are gone and another source is loaded, so there is nothing left to discard.
    if (VE.src === askedSrc && VE.ops.length && VE.dirty) {
      // Thrown away on purpose: stop the autosave for the old source and forget what an
      // earlier tick stored, or reopening that recording would offer the work back.
      stopEditStateAutosave();
      if (VE.src) await invoke("discard_edit_state", { kind: "video", target: VE.src }).catch(() => {});
    }
  }
  loadRecording(path, originDir, overlayPath);
  ack("accepted");
});
// A close the page did not start (Alt+F4, the taskbar entry, the app quitting) arrives here and
// takes the same path as the close button. The backend only holds closes back once told that this
// listener exists.
listen("ve-close-requested", (e) => { closeWindow({ quit: e.payload === true }); })
  .then(() => invoke("video_editor_close_ack", { state: "ready" }))
  .catch(() => {});
listen(EXPORT_PROGRESS_EVENT, (e) => {
  // A recording conversion in the main window reports on the same event; only the editor's own
  // export tags its ticks.
  if (e.payload?.job !== "export") return;
  if (!VE.exporting || !exportRun || exportRun.gen !== VE.loadGen) return;
  const ms = Number(e.payload.ms ?? 0);
  const total = exportRun.total;
  if (total > 0) setProgress((ms / total) * 100);
});

export function applySettings(settings) {
  const values = settings?.ui_values || {};
  VE.editStateKeepOn = settings?.ui_flags?.editStateKeep !== false;
  armEditStateAutosave();
  const texts = settings?.ui_texts || {};
  // The export dialog opens on the user's own last choice.
  $("ve-export-format").value = normRecFormat(texts.videoExportFormat);
  $("ve-export-quality").value = texts.videoExportQuality || "high";
  $("ve-export-tile").checked = values.veExportTile === 1;
  // VE.w/VE.h are still 0 the first time this runs (applyProbe() hasn't landed), and a scale
  // preset needs the source size to become pixels, so it is deferred to VE.pendingScaleKey and
  // consumed once they are known. A scale op the user (or an earlier default) already set is
  // left alone, like the fade-default guard below.
  const scaleDef = texts.videoExportScale ?? "";
  VE.scaleDefaultKey = scaleDef;
  if (!opOf("scale")) {
    if (VE.w && VE.h) applyDefaultScale(scaleDef);
    else VE.pendingScaleKey = scaleDef;
  }
  VE.destDir = texts.videoExportDir || "";
  // Only while fade isn't already applied: once it is, these fields hold the user's per-export
  // values, and a settings-changed event must never overwrite what they just typed.
  if (!opOf("fade")) {
    $("ve-fade-in").value = VE.fadeMsDefault;
    $("ve-fade-out").value = VE.fadeMsDefault;
    $("ve-fade-in").numFieldRefresh?.();
    $("ve-fade-out").numFieldRefresh?.();
  }
  // Shared with the screenshot editor's own snapping (see snap.js); the switch
  // to turn it off is per editor.
  VE.snapOn = settings?.ui_flags?.videoEditSnap !== false;
  // Each overlay kind starts from its own last-used colour/thickness/opacity/font
  // size instead of the fixed default above, kept in the ordinary settings store
  // (not the per-file edit state) because it belongs to the user, not the file.
  VE.ovTextColorDefault = HEX_RE.test(texts.veOvLastTextColor || "") ? texts.veOvLastTextColor : VE.ovColorDefault;
  VE.ovFrameColorDefault = HEX_RE.test(texts.veOvLastFrameColor || "") ? texts.veOvLastFrameColor : VE.ovColorDefault;
  // Kept as the raw remembered number (or null) and resolved against VE.h only when an overlay
  // is created, since VE.h is still 0 the first time this runs.
  const lastTextFontPx = Number(values.veOvLastTextFontPx);
  VE.ovLastTextFontPx = Number.isFinite(lastTextFontPx) && lastTextFontPx > 0 ? Math.round(lastTextFontPx) : null;
  const lastThickness = Number(values.veOvLastFrameThickness);
  VE.ovLastFrameThickness = Number.isFinite(lastThickness) && lastThickness > 0 ? Math.round(lastThickness) : null;
  const lastPixelateBlock = Number(values.veOvLastPixelateBlock);
  if (Number.isFinite(lastPixelateBlock) && lastPixelateBlock > 0) VE.ovPixelateBlockDefault = Math.round(lastPixelateBlock);
  const lastSpotlightDim = Number(values.veOvLastSpotlightDim);
  if (Number.isFinite(lastSpotlightDim) && lastSpotlightDim > 0) VE.ovSpotlightDimDefault = lastSpotlightDim / 100;
  const lastImageOpacity = Number(values.veOvLastImageOpacity);
  if (Number.isFinite(lastImageOpacity) && lastImageOpacity > 0) VE.ovImageOpacityDefault = Math.round(lastImageOpacity);
  VE.ovKeysColorDefault = HEX_RE.test(texts.veOvLastKeysColor || "") ? texts.veOvLastKeysColor : VE.ovColorDefault;
  if (HEX_RE.test(texts.veOvLastKeysActiveColor || "")) VE.ovKeysActiveColorDefault = texts.veOvLastKeysActiveColor;
  const lastKeysOpacity = Number(values.veOvLastKeysOpacity);
  if (Number.isFinite(lastKeysOpacity) && lastKeysOpacity > 0) VE.ovKeysOpacityDefault = Math.round(lastKeysOpacity);
  VE.ovFrameOpacityDefault = 100;
  const lastFrameOpacity = Number(values.veOvLastFrameOpacity);
  if (Number.isFinite(lastFrameOpacity) && lastFrameOpacity > 0) VE.ovFrameOpacityDefault = Math.round(lastFrameOpacity);
  updateExportNote();
}

// ---------------------------------------------------------------- boot

(async () => {
  try {
    applyTheme(await invoke("current_theme"), await invoke("theme_vars"));
  } catch (_) {}
  try {
    settings = await invoke("get_settings");
    applySettings(settings);
  } catch (_) {}
  textsReady = loadTexts(settings).catch(() => {});
  renderSpeedSteps();
  renderScalePresets();
  // The recording is pulled, not pushed (a push would race the webview's first frame, as for the
  // placeholder window). The "video-editor-load" event covers a second recording opened into a
  // window that is already up.
  let path = "";
  let originDir = "";
  let overlayPath = "";
  try {
    const source = await invoke("video_editor_source");
    path = source?.path || "";
    originDir = source?.origin_dir || "";
    overlayPath = source?.overlay_path || "";
  } catch (_) {
    // No source command yet, or none set: wait for the event instead of showing
    // an error for what is a normal startup order.
  }
  if (path) await loadRecording(path, originDir, overlayPath);
  else {
    // Wait for the texts (t() answers the raw key before they land) and point the label's
    // translation hook at the new message, or the next language pass would paint "Loading"
    // back over it.
    await textsReady;
    const msg = $("ve-stage-msg");
    msg.dataset.i18n = "veNoRecording";
    msg.textContent = t("veNoRecording");
  }
  requestAnimationFrame(tick);
})();
