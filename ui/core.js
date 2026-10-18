// Shared kernel of the main window: Tauri handles, DOM handles, language, toast and
// error helpers, flag/value readers, settings and prompts and the small shared state.
// Imports no feature module.

import { DRAG_THRESHOLD_DEFAULT } from "./copyDefaults.js";
import { hexLuminance, inkFor, orderedInvoke, resolveLang, serialInvoke } from "./floatShared.js";
import { LANGS, loadLang } from "./i18n.js";
import { applyTerms } from "./terms.js";
import { APP_NAME, FLAG_DEFAULTS } from "./brand.js";
import { errMessage } from "./errors.js";
import { rowsForScope as rowsForScopeRaw, sectionsForScopes as sectionsForScopesRaw, withShortcut as withShortcutRaw } from "./shortcuts.js";
import { confirmDialog } from "./dialog.js";
import { EXPERT_SELECTS, EXPERT_VALUES, FLAG_GATES, isOptFlag } from "./expertConfig.js";
import { createColorPop } from "./colorpop.js";
import EN from "./i18n/en.js";

export const { convertFileSrc } = window.__TAURI__.core;
// Commands whose order matters are chained per key (see serialInvoke); the rest run at once.
export const invoke = orderedInvoke;
export { serialInvoke };
export const { listen, emit } = window.__TAURI__.event;
// The screenshot editor's resize grip needs the main window's own resize-drag (the same v2
// API floating.js and folder.js use for startDragging()).
export const { getCurrentWindow, ResizeDirection } = window.__TAURI__.window;

export const $ = (id) => document.getElementById(id);
export const gridEl = $("grid");
export const inputEl = $("input");
export const saveBtn = $("save");
export const toastEl = $("toast");
export const ctxEl = $("ctx");
export const settingsEl = $("settings");
export const libraryEl = $("library");
export const viewsEl = $("views");
export const modal = {
  root: $("modal"),
  title: $("modal-title"),
  name: $("modal-name"),
  text: $("modal-text"),
  voiceRow: $("modal-voice-row"),
  voiceBtn: $("modal-voice-btn"),
  closeX: $("modal-close-x"),
  confirm: $("modal-confirm"),
  cancel: $("modal-cancel"),
  delete: $("modal-delete"),
  fav: $("modal-fav"),
  secret: $("modal-secret"),
  imgWrap: $("modal-img-wrap"),
  img: $("modal-img"),
  video: $("modal-video"),
  caption: $("modal-caption"),
  captionSize: $("modal-caption-size"),
  captionPreview: $("modal-caption-preview"),
  showImage: $("modal-show-image"),
  showText: $("modal-show-text"),
  copyFormattedBtn: $("modal-copy-formatted"),
  replaceImg: $("modal-replace-img"),
  removeImg: $("modal-remove-img"),
  addIcon: $("modal-add-icon"),
  fileWrap: $("modal-file-wrap"),
  fileName: $("modal-file-name"),
  pdfPage: $("modal-pdf-page"),
  replaceFile: $("modal-replace-file"),
  textAttachPreview: $("modal-text-attach-preview"),
  textAttachContent: $("modal-text-attach-content"),
  textAttachTruncated: $("modal-text-attach-truncated"),
  textAttachUnreadable: $("modal-text-attach-unreadable"),
  fontSel: $("modal-font"),
  sizeSel: $("modal-size"),
  textBar: $("modal-text-bar"),
  varsHead: $("modal-vars-head"),
  varsBody: $("modal-vars-body"),
  varsHint: $("modal-vars-hint"),
  mdPreview: $("modal-md-preview"),
  mdWrap: $("modal-md-wrap"),
  notesText: $("modal-notes-text"),
  advanced: $("modal-advanced"),
  advancedHead: $("modal-advanced-head"),
  advancedBody: $("modal-advanced-body"),
  hotkey: $("modal-hotkey"),
  hotkeyInput: $("modal-hotkey-input"),
  hotkeyBadge: $("modal-hotkey-badge"),
  rotateBadge: $("modal-rotate-badge"),
  hotkeyErr: $("modal-hotkey-err"),
  hotkeyAction: $("modal-hotkey-action"),
  hotkeyActionRow: $("modal-hotkey-action-row"),
  rotateToggleRow: $("modal-rotate-toggle-row"),
  rotateSwitch: $("modal-rotate-switch"),
  rotateList: $("modal-rotate-list"),
  rotateEntries: $("modal-rotate-entries"),
  rotateAdd: $("modal-rotate-add"),
  rotateMode: $("modal-rotate-mode"),
  rotateNext: $("modal-rotate-next"),
  rotateReset: $("modal-rotate-reset"),
  rotateAuto: $("modal-rotate-auto"),
};

export const DRAG_THRESHOLD = DRAG_THRESHOLD_DEFAULT;
export const INPUT_MAX = 160; // keep in sync with .input max-height
// Grid-dimension and view-count limits are expert values (val("gridMax") / val("maxViews")).
// The backend keeps a hard ceiling of 100 for both.
export const TOOLTIP_PREVIEW_CHARS = 600; // how much of a prompt a tile tooltip shows
export let BUBBLE_MS = 950; // "Copied!" bubble lifetime (expert-tunable)
export function assignBUBBLE_MS(v) { BUBBLE_MS = v; }
export const TOAST_MS = 1400; // toast lifetime for plain messages
export const SIZE_MIN = 10; // text size range, steps of 2 (keep in sync with backend)
export const SIZE_MAX = 40;
export const SIZE_STEP = 2;
export const DISARM_MS = 3000; // confirm-button auto-disarm delay

export const clampGrid = (n, fallback) =>
  Math.min(val("gridMax"), Math.max(1, Math.round(Number(n) || fallback)));

// Cached state (refreshed by renderGrid).
export let prompts = [];
export function assignPrompts(v) { prompts = v; }
export let settings = { theme: "system", views: [], active_view: "" };
export function assignSettings(v) { settings = v; }

export let modalState = null;
export function assignModalState(v) { modalState = v; }

export let drag = null;
export function assignDrag(v) { drag = v; }

export const gridTags = new Set(); // same idea, independent selection for the grid

// Single-slot undo for a destructive action outside the editors (which have their own undo
// stack). A new destructive action replaces whatever was pending; there is no history.
export let pendingUndo = null; // { restore }
// Grid bulk selection: its own Set, independent of the library's libSelected. Folder tiles
// are excluded: a folder stands for several prompts, and "select, then recolor/move/delete"
// has no single obvious meaning for the group.
export let gridSelected = new Set();

export let toastTimer = null;

export let appVersion = ""; // raw "1.6.0", for the About dialog
export function assignAppVersion(v) { appVersion = v; }

// Which edge of a sideways-scrolling strip still has content behind it: sets data-fade
// (start, end, both) for the soft edge fade in CSS.
export function syncEdgeFade(bar) {
  const more = bar.scrollWidth - bar.clientWidth > 1;
  const start = more && bar.scrollLeft > 1;
  const end = more && bar.scrollLeft < bar.scrollWidth - bar.clientWidth - 1;
  if (start && end) bar.dataset.fade = "both";
  else if (start) bar.dataset.fade = "start";
  else if (end) bar.dataset.fade = "end";
  else delete bar.dataset.fade;
}

// A status line that carries a live value ("Version 1.6.0", a port, a host) is remembered as
// key + token + value, so applyI18n can redraw it after a language switch.
export function setStatusText(el, key, token, value) {
  if (!el) return;
  el.dataset.statusKey = key;
  el.dataset.statusToken = token;
  el.dataset.statusValue = String(value);
  el.textContent = t(key).replace("{" + token + "}", () => String(value));
}

export const GRID_PLUS =
  '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="currentColor" d="M4 4h7v7H4V4Zm9 0h7v7h-7V4ZM4 13h7v7H4v-7Zm12 0h2v3h3v2h-3v3h-2v-3h-3v-2h3v-3Z"/></svg>';
export const PLUS_ICON =
  '<svg viewBox="0 0 24 24" width="19" height="19" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" d="M12 5v14M5 12h14"/></svg>';
// Corner badges marking what a tile copies (attached file / image).
export const ICON_FILE =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M21.2 11.2l-8.4 8.4a5.5 5.5 0 0 1-7.8-7.8l8.4-8.4a3.7 3.7 0 0 1 5.2 5.2l-8.4 8.4a1.9 1.9 0 0 1-2.6-2.6l7.7-7.7"/></svg>';
export const ICON_IMAGE =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="currentColor" d="M21 3H3a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h18a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Zm0 16H3V5h18v14Zm-5.5-9a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0ZM8.5 14l2-2.5 2 2.5 2.5-3 3.5 5H5l3.5-4.5Z"/></svg>';
export const ICON_VIDEO =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="currentColor" d="M15 8.5V6a1 1 0 0 0-1-1H3a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h11a1 1 0 0 0 1-1v-2.5l5 4v-15l-5 4ZM6.5 9.8 12 13l-5.5 3.2V9.8Z"/></svg>';
// Plain grey document marker for PDF attachments (replaces the paperclip).
export const ICON_PDF =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><path fill="currentColor" d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6Zm0 2 4 4h-4V4ZM8 12h8v1.6H8V12Zm0 3.2h8v1.6H8v-1.6Z"/></svg>';
export const ICON_EDIT =
  '<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M4 20h4L18.5 9.5a2 2 0 0 0-2.8-2.8L5 17.2V20Zm9.5-12.5 3 3"/></svg>';
export const ICON_ADD_SQUARE =
  '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="3" fill="none" stroke="currentColor" stroke-width="2"/><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" d="M12 8.5v7M8.5 12h7"/></svg>';
export const ICON_TRASH =
  '<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M4 7h16M9 7V5h6v2m-8 0 1 13h8l1-13M10 11v6M14 11v6"/></svg>';
// The glyph is shifted to bound [4,20]x[4.5,19.5] so it sits centered in the toast's icon
// badge.
export const ICON_UNDO =
  '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" d="M9 4.5 4 9.5l5 5M4 9.5h11a5 5 0 0 1 0 10h-4"/></svg>';
// Same triangle the expert menu's own caution line uses, so a warning looks like a
// warning wherever it appears.
export const ICON_WARN =
  '<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><path fill="currentColor" d="M12 3 1 21h22L12 3Zm0 5.5 6.6 11H5.4L12 8.5ZM11 11h2v5h-2v-5Zm0 6.5h2v2h-2v-2Z"/></svg>';
// Reorder a rotating prompt's entries (no drag & drop: YAGNI, and testable).
export const ICON_UP =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M6 15l6-6 6 6"/></svg>';
export const ICON_DOWN =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M6 9l6 6 6-6"/></svg>';
// A prompt's own hotkey, shown as a small always-visible tile badge.
export const ICON_KEYBOARD =
  '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"><rect x="2.5" y="6" width="19" height="12" rx="2" fill="none" stroke="currentColor" stroke-width="1.8"/><path fill="currentColor" d="M5.5 9h1.6v1.6H5.5V9Zm3.2 0h1.6v1.6H8.7V9Zm3.2 0h1.6v1.6h-1.6V9Zm3.2 0h1.6v1.6h-1.6V9Zm3.2 0h1.6v1.6h-1.6V9ZM5.5 12.2h1.6v1.6H5.5v-1.6Zm3.2 0h1.6v1.6H8.7v-1.6Zm3.2 0h1.6v1.6h-1.6v-1.6Zm3.2 0h1.6v1.6h-1.6v-1.6Zm3.2 0h1.6v1.6h-1.6v-1.6ZM5.5 15.4h11.6V17H5.5v-1.6Z"/></svg>';

// Icon markup handed out as ready-made nodes. Assigning the same SVG string to innerHTML
// re-parses it every time, which a list of hundreds of rows pays for once per icon per row;
// parsing once and copying is the same markup for a fraction of the work. Only worth it
// inside a long loop.
export const iconNodes = new Map();
export function putIcon(el, svg) {
  let node = iconNodes.get(svg);
  if (node === undefined) {
    const box = document.createElement("span");
    box.innerHTML = svg;
    node = box.firstElementChild;
    iconNodes.set(svg, node);
  }
  if (node) el.appendChild(node.cloneNode(true));
  else el.innerHTML = svg; // not a single element: fall back to the plain path
  return el;
}

// A per-row action icon has to be a <span>, not a <button>: it sits nested inside the row's
// own <button> (library/journal rows), and HTML forbids nesting buttons. This supplies the
// keyboard half a real button would give: joins the tab order and turns Enter/Space into the
// click the element already handles. Call once, right after wiring the element's own "click"
// listener.
export function makeKeyboardActivatable(el) {
  el.tabIndex = 0;
  el.setAttribute("role", "button");
  el.addEventListener("keydown", (e) => {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    e.stopPropagation();
    el.click();
  });
  return el;
}

export let LANG = "en";
export let DICT = EN;
// A language stored before it was retired has no option left, which would leave the select
// blank; show what the app falls back to instead.
export function setLangSelect() {
  const sel = document.getElementById("lang-select");
  if (!sel) return;
  sel.value = settings.language || "auto";
  if (!sel.value) sel.value = "auto";
}
export const pickLang = (pref) => resolveLang(pref, LANGS);
// Loads the dictionary of the chosen language before LANG flips, so a caller that awaits
// this never renders a half-switched state. English is primed, so it never waits.
export let langSeq = 0;
export async function setLang(pref) {
  const code = pickLang(pref), seq = ++langSeq;
  const dict = await loadLang(code);
  if (seq !== langSeq) return;
  LANG = code; DICT = dict;
}
// {app} stands for the product name in every translation, so no string spells it; callers
// never pass it.
export const t = (key) =>
  applyTerms(String(DICT[key] ?? EN[key] ?? key).replaceAll("{app}", APP_NAME), LANG);

// buildMediaBar (media.js) is shared with the floating pill, which has no synchronous access
// to t(), so it takes resolved strings, not keys.
export const mediaBarLabels = () => ({
  play: t("mediaPlay"), pause: t("mediaPause"),
  loopOn: t("mediaLoopOn"), loopOff: t("mediaLoopOff"),
  mute: t("mediaMute"), unmute: t("mediaUnmute"),
});

// The product name never appears in the markup: every [data-brand] element (the <title> and
// the header label) is filled from ui/brand.js. Runs at load, before the first backend round
// trip, so the header is never briefly blank.
export const applyBrand = () => {
  document.querySelectorAll("[data-brand]").forEach((el) => { el.textContent = APP_NAME; });
};

// shortcuts.js has no i18n state of its own; every call here binds it to this window's t()
// and LANG.
export const withShortcut = (el, action) => withShortcutRaw(el, action, t, LANG);
export const shortcutRows = (scope) => rowsForScopeRaw(scope, t, LANG);
export const shortcutSections = (scopes) => sectionsForScopesRaw(scopes, t, LANG);
// Every window that has keys of its own, in the order the overview lists them.
export const ALL_SHORTCUT_SCOPES = ["mainWindow", "library", "launcher", "tray", "vars", "floating", "snipOverlay", "recBar", "camera"];

// Tile color palette ("" = default surface), full spectrum, one modal row. Empty = no color;
// the rest are hue-sorted and fill two rows of ten with the "none" and custom swatches. The
// user can recolor the palette (expert menu); COLORS holds the live set, DEFAULT_COLORS the
// shipped fallback.
export const DEFAULT_COLORS = [
  "",
  "#ef4444", "#f97316", "#f59e0b", "#eab308", "#84cc16",
  "#22c55e", "#10b981", "#14b8a6", "#06b6d4", "#0ea5e9",
  "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#d946ef",
  "#ec4899", "#f43f5e", "#64748b",
];
export let COLORS = [...DEFAULT_COLORS];
export function assignCOLORS(v) { COLORS = v; }
export const PALETTE_LEN = DEFAULT_COLORS.length - 1; // editable hues (index 0 is "none")
// The house mark for "no value": a lone en dash.
export const EMPTY_VALUE = "\u2013";
export const HEX_RE = /^#[0-9a-fA-F]{6}$/;

// The pending "finish hiding" step of the last hideToast(). A toast shown inside that window
// (an Undo whose restore fails) must cancel it, or it hides the new one.
let toastHideTimer = null;
export function hideToast() {
  toastEl.classList.remove("show");
  clearTimeout(toastHideTimer);
  toastHideTimer = setTimeout(() => toastEl.classList.add("hidden"), 200);
}

// Default toast icon (checkmark badge). Static, developer-controlled SVG.
export const TOAST_ICON = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>';

// Relative luminance to the ink that stays readable on `hex`. With autoInk off the text is
// always white; on, black only above the expert-tunable inkThreshold (a relative luminance in
// percent). Its default sits just above the point where black and white reach the same
// contrast, so a colour that white text only reaches with a shadow gets black text instead.
export function readableInk(hex) {
  if (hexLuminance(hex) === null) return "#fff";
  if (!flag("autoInk")) return "#fff";
  return inkFor(hex, val("inkThreshold") / 100) === "#000000" ? "#000" : "#fff";
}

// White ink stays white up to the user's threshold (a deliberate look), but on a colour past
// the contrast crossover it gets a dark halo so it reads. The halo can be switched off
// (inkHalo) or dialed in strength (inkHaloStrength, percent); the second alpha keeps the same
// ratio to the first at any strength.
export const INK_HALO_RATIO = 0.7 / 0.85;
export function inkShadow(hex) {
  if (!flag("inkHalo")) return "none";
  const l = hexLuminance(hex);
  if (l === null || l <= 0.179 || readableInk(hex) !== "#fff") return "none";
  const a1 = val("inkHaloStrength") / 100;
  const a2 = Number((a1 * INK_HALO_RATIO).toFixed(2));
  return `0 0 2px rgba(0, 0, 0, ${Number(a1.toFixed(2))}), 0 1px 3px rgba(0, 0, 0, ${a2})`;
}

// Optional action: { label, onClick } adds a button and keeps the toast longer. `tint` (a
// #rrggbb) paints the icon badge in that colour: the colour picker uses it so the picked
// colour is visible next to its value. `chips` is a second, independent action slot: a row of
// small pill buttons (e.g. the three colour notations) instead of the single `.toast-btn`.
// `duration` overrides the computed auto-hide delay (ms) for this one call; the background
// language-update notices use their own fixed duration (OCR_LANG_NOTICE_MS).
// Shorter than the pill and folder windows' shared copied duration (900 ms): this toast chip
// flash is brief by design.
export const CHIP_COPIED_MS = 400;
// Fixed layout and timing values.
export const HEADER_PAD_Y = 10; // px, vertical padding of the header bar
export const TILE_FOLDER_ZONE = 55; // % of a tile that means "drop into a folder"
export const KB_FOCUS_FADE_MS = 2000; // delay before the keyboard focus ring fades
// How long the background language-pack update notices stay up (ms).
export const OCR_LANG_NOTICE_MS = 8000;
export const VOICE_LANG_NOTICE_MS = 8000;
// Arrow-key nudge step in the screenshot editor while Shift is held.
export const NUM_FIELD_BIG_STEP = 10;
export function toast(msg, action = null, tint = "", chips = null, duration = null) {
  clearTimeout(toastHideTimer);
  toastEl.textContent = "";
  const hasChips = !!(chips && chips.length);
  // Only actionable toasts capture clicks; plain ones stay click-through so they never block
  // the UI underneath.
  toastEl.classList.toggle("actionable", !!action || hasChips);
  toastEl.classList.toggle("update", !!(action && action.variant === "update"));
  toastEl.classList.toggle("tinted", !!tint);
  toastEl.classList.toggle("has-chips", hasChips); // wider card, chips inline with the message
  if (tint) {
    toastEl.style.setProperty("--toast-tint", tint);
    toastEl.style.setProperty("--toast-ink", readableInk(tint));
  }
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
  if (hasChips) {
    const row = document.createElement("span");
    row.className = "toast-chip-row";
    for (const c of chips) {
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = "toast-chip";
      chip.textContent = c.label;
      chip.classList.toggle("is-current", !!c.current); // the one on the clipboard
      chip.setAttribute("aria-pressed", String(!!c.current));
      // Copies again without closing the toast (comparing or re-picking a notation is the point).
      // The clicked chip becomes "current" too: it is now on the clipboard, and the row shows
      // exactly one such chip.
      chip.addEventListener("click", () => {
        c.onClick();
        for (const other of row.children) {
          other.classList.toggle("is-current", other === chip);
          other.setAttribute("aria-pressed", String(other === chip));
        }
        chip.classList.add("copied");
        setTimeout(() => chip.classList.remove("copied"), CHIP_COPIED_MS);
      });
      row.appendChild(chip);
    }
    toastEl.appendChild(row);
  }
  toastEl.classList.remove("hidden");
  void toastEl.offsetWidth;
  toastEl.classList.add("show");
  clearTimeout(toastTimer);
  // A toast carrying an action button or chips needs longer to read and click than a plain
  // message: the plain duration plus a fixed bonus (12000ms in all).
  toastTimer = setTimeout(hideToast, duration ?? ((action || hasChips) ? TOAST_MS + 10600 : TOAST_MS));
}

// Logs the error and returns the text the user is shown for it. A string with no translation
// is never shown raw: the user gets the generic "something went wrong" text and the raw
// string goes to the error log.
export function errText(err) {
  const msg = errMessage(err, t);
  logErrorLogEntry(msg, String(err));
  return msg;
}
export function errToast(err) {
  toast(errText(err));
}

// Best-effort label for which part of the app an error most likely belongs to, read from
// what is on screen rather than tagging each of errToast's many call sites.
export function currentErrorOrigin() {
  const openOverlay = document.querySelector(".overlay:not(.hidden)[id]");
  if (openOverlay) return openOverlay.id;
  if (!$("settings-expert").classList.contains("hidden")) return "settings-expert";
  return "main";
}

// Fire-and-forget: a failure to log the log itself must never show another error (recursing
// into errToast) or block the toast the user is waiting on.
export function logErrorLogEntry(text, raw) {
  invoke("log_error", { text, origin: currentErrorOrigin(), raw }).catch(() => {});
}

// Runs the pending undo, if any, and clears the slot first so a restore that
// itself triggers another destructive action cannot re-arm on top of itself.
export async function runPendingUndo() {
  const slot = pendingUndo;
  if (!slot) return;
  pendingUndo = null;
  try {
    await slot.restore();
  } catch (err) {
    errToast(err);
  }
}
// Shows `msg` with an Undo button wired to `restore`. With the Undo time Off it is a plain
// toast and nothing is armed (an off setting means nothing is kept around to undo).
export function undoToast(msg, restore) {
  if (val("undoToastMs") <= 0) { toast(msg); return; }
  pendingUndo = { restore };
  toast(msg, { label: t("undo"), onClick: runPendingUndo }, "", null, val("undoToastMs"));
}

// A failure that struck after part of an action already went through: the error text
// stays the toast message and the Undo button brings the finished part back.
export function errUndoToast(err, restore) {
  undoToast(errText(err), restore);
}

// For actions that cannot go to the trash and have nothing to bring them back but the Undo
// toast (a deleted view, a merged tag): with the Undo time Off nothing would be kept, so ask
// first. With Undo on, the toast is the safety net and this resolves true at once.
export async function confirmIfNoUndo(opts) {
  if (val("undoToastMs") > 0) return true;
  return !!(await confirmDialog(opts));
}

export function autoGrow(el) {
  el.style.height = "auto";
  const target = el.scrollHeight + 2;
  el.style.height = `${Math.min(target, INPUT_MAX)}px`;
  el.style.overflowY = target > INPUT_MAX ? "auto" : "hidden";
}

export const cellKey = (c, r) => `${c},${r}`;

// Two-step confirmation: first call arms the button and returns false,
// the second call (while armed) returns true.
export function armButton(btn, confirmLabel) {
  if (btn.classList.contains("confirm")) return true;
  btn.classList.add("confirm");
  btn.textContent = confirmLabel;
  return false;
}
export function disarmButton(btn, label) {
  btn.classList.remove("confirm");
  btn.textContent = label;
}

// A brand may ship a feature the other way round than this app does (see
// brands/<slug>.json "flagDefaults"). A stored value always wins; the default only decides
// what an untouched install starts with.
export const shipsOn = (key) => {
  const v = FLAG_DEFAULTS[key];
  return typeof v === "boolean" ? v : !isOptFlag(key);
};
export const optFlag = (key) => (settings.ui_flags?.[key] ?? shipsOn(key)) === true;
// Current state + label key, whichever kind of flag this is.
export const flagState = (key) => (isOptFlag(key) ? optFlag(key) : flag(key));
// A sub-toggle counts as off while its own parent is off, so everything hanging off it
// disappears with the whole branch instead of surviving as an orphan row.
export const gateOpen = (gate, seen) => {
  if (!gate) return true;
  const inv = gate[0] === "!";
  const key = inv ? gate.slice(1) : gate;
  let on = EXPERT_VALUES[key]?.offAtZero ? val(key) > 0 : flagState(key);
  const guard = seen || new Set();
  if (on && !guard.has(key)) {
    guard.add(key);
    on = gateOpen(FLAG_GATES[key], guard);
  }
  return inv ? !on : on;
};

export const flag = (key) => (settings.ui_flags?.[key] ?? shipsOn(key)) !== false;
export const val = (key) => {
  const cfg = EXPERT_VALUES[key] || EXPERT_SELECTS[key]; // selects live in ui_values too
  const raw = settings.ui_values?.[key];
  const v = Number.isFinite(raw) ? raw : cfg.def;
  // 0 is this setting's "unlimited" stop, below its slider minimum on purpose.
  if (cfg.topUnlimited && v === 0) return 0;
  // Clamp to the setting's own range so no stored or imported value can push a slider-bounded
  // setting out of bounds (grid geometry stays valid).
  if (cfg.stops) {
    return Math.min(cfg.stops[cfg.stops.length - 1], Math.max(cfg.stops[0], v));
  }
  if (Number.isFinite(cfg.min) && Number.isFinite(cfg.max)) {
    return Math.min(cfg.max, Math.max(cfg.min, v));
  }
  return v;
};
export const txt = (key) => settings.ui_texts?.[key] || "";

// Wraps a command that reports no real progress of its own (backup, import) with an
// indeterminate taskbar entry; without a percentage that is all that can honestly be shown.
export async function withTaskbarBusy(op, fn) {
  invoke("set_taskbar_progress", { op, state: "indeterminate" }).catch(() => {});
  try {
    return await fn();
  } finally {
    invoke("set_taskbar_progress", { op, state: "done" }).catch(() => {});
  }
}

// Expert-menu action buttons (id -> handler); populated by the features that own them.
export const EXPERT_ACTIONS = {};

// ---- Favorites view: a live, auto-sized grid of every starred prompt. ----
// Enabled by favViewButton (off by default); the header star toggles it.
export let favView = false; // currently showing the favorites grid
export function assignFavView(v) { favView = v; }

// A greyed clear-X inside a text field: brightens on hover, empties the field on click.
// Wraps the input so the button can sit at its right edge.
export const CLEAR_SVG = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" d="M7 7l10 10M17 7 7 17"/></svg>';
// Dialog icons: a stack of records for the data take-over, a padlock for the
// password prompt.
export const TAKEOVER_SVG = '<svg viewBox="0 0 24 24" aria-hidden="true"><ellipse cx="12" cy="5.5" rx="7.5" ry="3" fill="none" stroke="currentColor" stroke-width="1.8"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M4.5 5.5v13c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-13M4.5 12c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3"/></svg>';
export const LOCK_SVG = '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="10" width="16" height="11" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.8"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M8 10V7a4 4 0 0 1 8 0v3"/><circle cx="12" cy="15.5" r="1.6" fill="currentColor"/></svg>';

export const ROTATE_SHORT_MAX = 60; // tooltip preview length, independent of the notes cap

// Anything drawn while its picture was still loading tags itself with the id it is waiting
// for, and only those elements are rebuilt when the bytes land. Re-rendering the whole grid
// would tear down whatever else is going on (a live colour preview, a half-finished drag).
export const IMG_WAIT = "imgWait"; // dataset key: the prompt id this element is waiting for

// ---- Video tiles: YouTube-style hover bar ----
// Video tiles keep their DOM element across grid re-renders because rebuilding would restart
// playback; reparenting the cached wrapper does not.
export const videoCache = new Map(); // prompt id -> { src, wrap }

// Library and journal thumbnails are cached per panel so a search re-render (every keystroke)
// does not force the browser to re-decode the same base64 blob. Both panels can stay mounted
// at once (see the video IntersectionObserver comment), so a single shared cache would rip a
// thumb out of whichever panel is not rendering.
export const libThumbCache = new Map(); // prompt id -> { key, el }
export const journalThumbCache = new Map();
export function pruneThumbCache(cache) {
  if (!cache.size) return;
  const live = new Set(prompts.map((p) => p.id));
  for (const id of cache.keys()) {
    if (!live.has(id)) cache.delete(id);
  }
}

// Dragging a member out of the open popover.
export let memberDrag = null;
export function assignMemberDrag(v) { memberDrag = v; }

// ---- Arming a folder while a tile is dragged ----
// The outer band of a tile keeps the plain swap; the middle makes a folder, but only after a
// short dwell, so a quick swap never turns into one by accident.
export let folderArm = null;
export function assignFolderArm(v) { folderArm = v; }

export const TB_CHECK = '<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" d="M5 12.5l4.5 4.5L19 7"/></svg>';

// Search re-renders the whole result list, which for a large library means rebuilding every
// row (and re-decoding every thumbnail), far too much per keystroke. The field stays live;
// only the list waits for a short pause in typing: short enough to feel immediate, long
// enough that a fast typist renders once.
export const SEARCH_DEBOUNCE_MS = 120;
// ms may be a getter, so a caller can read a live value on every keystroke instead
// of one baked in when the debounced wrapper was created.
export function debounced(fn, ms = SEARCH_DEBOUNCE_MS) {
  let timer = 0;
  const run = () => {
    clearTimeout(timer);
    timer = setTimeout(fn, typeof ms === "function" ? ms() : ms);
  };
  run.cancel = () => clearTimeout(timer);
  return run;
}

// ---- Version history: render the collapsible list in the editor ----
export const HISTORY_PREVIEW_LEN = 80;

export const PDF_EXT = /\.pdf$/i;

// Which tab of the library is showing; the library reopens on the one used last.
export let libraryTab = "prompts"; // "prompts" | "history"
export function assignLibraryTab(v) { libraryTab = v; }

// ---- Custom color picker (theme-styled popup with SV field + hue bar) ----
// Shared with the video editor through colorpop.js.
export let colorPop, openColorPop, closeColorPop;
export function wireColorPop() {
  ({ el: colorPop, open: openColorPop, close: closeColorPop } = createColorPop());
}
