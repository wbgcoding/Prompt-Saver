// Main-window controller. Uses Tauri's global API (withGlobalTauri).
import { APP_NAME, FLAG_DEFAULTS } from "./brand.js";
import { applyTerms } from "./terms.js";
import { I18N, LANGS } from "./i18n.js";
import { FONTS, FONT_LABELS, fitText } from "./fonts.js";
import { IMAGE_EXT, mediaKind, buildMediaBar, applyVideoPrefs, setMediaDefaults } from "./media.js";
import { COOLDOWN_MS_DEFAULT, AUTO_PASTE_DELAY_MS_DEFAULT, DBL_CLICK_MS_DEFAULT, DRAG_THRESHOLD_DEFAULT, COPY_SOUND_VOLUME_DEFAULT } from "./copyDefaults.js";
import { EXAMPLE_IMAGE_DATA_URL } from "./exampleAssets.js";
import { HELP_CONTENT } from "./help.js";
import { DRAG_OUT_MODS, fileUri, resolveLang, ffmpegDownloadPct, themeColorScheme, playCopySound, numberField, makeEtaEstimator, formatEta, parseFfmpegError, fmtBytes } from "./floatShared.js";
import { placePopover, placePopoverAt, setPopoverPad } from "./popover.js";
import { createColorPop } from "./colorpop.js";

const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen, emit } = window.__TAURI__.event;
// The screenshot editor's resize grip needs the main window's own resize-drag,
// the same documented v2 API floating.js/folder.js already use for startDragging().
const { getCurrentWindow, ResizeDirection } = window.__TAURI__.window;

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
  voiceRow: $("modal-voice-row"),
  voiceBtn: $("modal-voice-btn"),
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
  pdfPage: $("modal-pdf-page"),
  replaceFile: $("modal-replace-file"),
  fontSel: $("modal-font"),
  sizeSel: $("modal-size"),
  textBar: $("modal-text-bar"),
  varsHead: $("modal-vars-head"),
  varsBody: $("modal-vars-body"),
  varsHint: $("modal-vars-hint"),
  mdPreview: $("modal-md-preview"),
  mdWrap: $("modal-md-wrap"),
  notesText: $("modal-notes-text"),
  // Tags/hotkey/history share one collapsible now.
  advanced: $("modal-advanced"),
  advancedHead: $("modal-advanced-head"),
  advancedBody: $("modal-advanced-body"),
  hotkey: $("modal-hotkey"),
  hotkeyInput: $("modal-hotkey-input"),
  hotkeyBadge: $("modal-hotkey-badge"),
  hotkeyErr: $("modal-hotkey-err"),
  hotkeyAction: $("modal-hotkey-action"),
  hotkeyActionRow: $("modal-hotkey-action-row"),
  // Rotating prompts.
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

let DRAG_THRESHOLD = DRAG_THRESHOLD_DEFAULT; // expert-tunable
let INPUT_MAX = 160; // keep in sync with .input max-height, expert-tunable via composerMaxH
// Grid-dimension and view-count limits are expert values: val("gridMax") /
// val("maxViews"). The backend keeps a hard ceiling of 100 for both.
let PREVIEW_MAX = 600; // tooltip preview length (expert-tunable)
let BUBBLE_MS = 950; // "Copied!" bubble lifetime (expert-tunable)
let TOAST_MS = 1400; // toast lifetime for plain messages (expert-tunable)
const SIZE_MIN = 10; // text size range, steps of 2 (keep in sync with backend)
const SIZE_MAX = 40;
const SIZE_STEP = 2;
let DISARM_MS = 3000; // confirm-button auto-disarm delay, expert-tunable

const clampGrid = (n, fallback) =>
  Math.min(val("gridMax"), Math.max(1, Math.round(Number(n) || fallback)));

// Cached state (refreshed by renderGrid).
let prompts = [];
let settings = { theme: "system", views: [], active_view: "" };

let modalState = null;
// Bumped by openModal/closeModal. An async pick/preview started for one editor
// session (pickReplacementMedia, replaceFile) captures this before its awaits and
// checks it again after -- `modalState` alone is not enough, since closing and
// opening a DIFFERENT prompt leaves modalState just as truthy, letting a slow
// reply from the old prompt land in the new one's editor.
let modalSession = 0;
// Whatever had keyboard focus right before the editor opened, so closing
// it returns focus there instead of dropping it to document.body -- without
// this a keyboard user's next Tab press restarts from the top of the page.
let modalOpener = null;
let modalInitial = ""; // snapshot of the open modal's content (unsaved-change guard)
let modalSaving = false; // re-entry guard, a double-click/double-Enter must not fire two saves
let ctxId = null;
let drag = null;
let libQuery = ""; // prompt library search text
// Type filters are ADDITIVE and identical in both windows: an empty set means every
// type, each chip toggles its own, and the pick is stored per window.
const libTypes = new Set();
let libColor = "all"; // prompt library color filter
let libFav = false; // prompt library "favorites only" filter
const libTags = new Set(); // Selected tag filters, as lowercase tag keys
const gridTags = new Set(); // same idea, independent selection for the grid
// Batch operations state.
let libSelectMode = false;
let libSelected = new Set();
let libLastId = null;          // anchor for shift-click range select
let libDisplayOrder = [];      // ids in current display order (for range select)
let gridDirty = false;         // batch ops changed prompts; rebuild grid on library close
// Grid bulk selection — its own Set, independent of the library's libSelected.
// Folder tiles are excluded: a folder stands for several prompts, and "select,
// then recolor/move/delete" has no single obvious meaning for the group itself.
let gridSelected = new Set();
let journalQuery = ""; // copy-history search text
const journalTypes = new Set(); // additive, like the library's
let journalColor = "all"; // copy-history color filter
let journalFav = false; // copy-history "favorites only" filter
const journalTags = new Set(); // same idea as libTags/gridTags, own selection
let toastTimer = null;
let deleteAllTimer = null;
let resetSettingsTimer = null;
let expertResetTimer = null;
let versionLabel = ""; // "Version 1.6.0", shown as the clickable releases link
let appVersion = ""; // raw "1.6.0", for the About dialog

// Whatever had focus right before the settings/expert panel opened, so
// closing it returns focus there (same reasoning as modalOpener above).
let settingsOpener = null;

// Hide the settings modal and reset the Updates status line back to the version
// number — unless a real update is pending (the "update available" state stays).
// So a transient "already newest version" message doesn't linger after reopening.
function hideSettings() {
  settingsEl.classList.add("hidden");
  const uv = $("update-version");
  if (uv && versionLabel && !uv.classList.contains("update-avail")) uv.textContent = versionLabel;
  settingsOpener?.focus?.();
  settingsOpener = null;
}

// Surface unexpected errors as a toast instead of failing silently.
window.addEventListener("error", (e) => toast(String(e.message)));
window.addEventListener("unhandledrejection", (e) => toast(String(e.reason)));

// Right-click is disabled everywhere inside the app (tiles re-enable it).
window.addEventListener("contextmenu", (e) => e.preventDefault());

const DOTS =
  '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="currentColor" d="M6 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM12 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM18 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z"/></svg>';
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
const ICON_TRASH =
  '<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M4 7h16M9 7V5h6v2m-8 0 1 13h8l1-13M10 11v6M14 11v6"/></svg>';
// The original path (M8 7 3 12l5 5M3 12h11a5 5 0 0 1 0 10h-4) was not
// centered in its 24x24 viewBox (its true bounds sit low and to the left),
// which read as an off-center glyph in the toast's centered icon badge even
// though the badge itself centers correctly. Shifted to bound [4,20]x[4.5,19.5].
const ICON_UNDO =
  '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" d="M9 4.5 4 9.5l5 5M4 9.5h11a5 5 0 0 1 0 10h-4"/></svg>';
// Same triangle the expert menu's own caution line uses, so a warning looks like a
// warning wherever it appears.
const ICON_WARN =
  '<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><path fill="currentColor" d="M12 3 1 21h22L12 3Zm0 5.5 6.6 11H5.4L12 8.5ZM11 11h2v5h-2v-5Zm0 6.5h2v2h-2v-2Z"/></svg>';
// Reorder a rotating prompt's entries (no drag & drop — YAGNI, and testable).
const ICON_UP =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M6 15l6-6 6 6"/></svg>';
const ICON_DOWN =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M6 9l6 6 6-6"/></svg>';

// Icon markup handed out as ready-made nodes. Assigning the same SVG string to
// innerHTML re-parses it every time, which a list of hundreds of rows pays for
// once per icon per row; parsing each icon once and copying the result is the
// same markup for a fraction of the work. Only worth it inside a long loop —
// a one-off button is clearer with a plain innerHTML.
const iconNodes = new Map();
function putIcon(el, svg) {
  let node = iconNodes.get(svg);
  if (node === undefined) {
    const box = document.createElement("span");
    box.innerHTML = svg;
    node = box.firstElementChild;
    iconNodes.set(svg, node);
  }
  if (node) el.appendChild(node.cloneNode(true));
  else el.innerHTML = svg; // not a single element — fall back to the plain path
  return el;
}

// A per-row action icon has to be a <span>, not a real <button> — it sits
// nested inside the row's own <button> (library/journal rows), and HTML
// forbids nesting one button in another. This is the keyboard half a real
// button would give for free: joins the tab order and turns Enter/Space into
// the same click the element already handles. Call once, right after wiring
// the element's own "click" listener.
function makeKeyboardActivatable(el) {
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

let LANG = "en";
// A language that was stored before it was retired has no option left, which would
// leave the select blank; show what the app actually falls back to instead.
function setLangSelect() {
  const sel = document.getElementById("lang-select");
  if (!sel) return;
  sel.value = settings.language || "auto";
  if (!sel.value) sel.value = "auto";
}
const pickLang = (pref) => resolveLang(pref, LANGS);
// {app} stands for the product name in every translation, so no string spells it.
// Filling it here means callers never have to pass it.
const t = (key) =>
  applyTerms(String(I18N[LANG][key] ?? I18N.en[key] ?? key).replaceAll("{app}", APP_NAME), LANG);

// buildMediaBar (media.js) is shared with the floating pill, which has no
// synchronous access to t() — so it takes plain resolved strings, not keys.
const mediaBarLabels = () => ({
  play: t("mediaPlay"), pause: t("mediaPause"),
  loopOn: t("mediaLoopOn"), loopOff: t("mediaLoopOff"),
  mute: t("mediaMute"), unmute: t("mediaUnmute"),
});

// The product name never appears in the markup — every [data-brand] element (the
// <title> and the header label) is filled from ui/brand.js. Runs at load, before
// the first backend round trip, so the header is never briefly blank.
const applyBrand = () => {
  document.querySelectorAll("[data-brand]").forEach((el) => { el.textContent = APP_NAME; });
};
applyBrand();

function applyI18n() {
  document.documentElement.lang = LANG;
  applyBrand();
  document.querySelectorAll("[data-i18n]").forEach((el) => { el.textContent = t(el.dataset.i18n); });
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => { el.placeholder = t(el.dataset.i18nPh); });
  document.querySelectorAll("[data-i18n-title]").forEach((el) => { el.title = t(el.dataset.i18nTitle); });
  document.querySelectorAll("[data-i18n-aria]").forEach((el) => { el.setAttribute("aria-label", t(el.dataset.i18nAria)); });
  // Cached video tiles keep their media bar across a language switch — its
  // button labels are not markup, so data-i18n above never reaches them. Walk
  // videoCache itself, not just the live DOM: a tile from a view that is not
  // the active one is cached but detached, so querySelectorAll would miss it.
  for (const { wrap } of videoCache.values()) {
    wrap.querySelector(".media-bar")?.setLabels?.(mediaBarLabels());
  }
  updateExamplesBanner(); // its label carries a live count, not a plain data-i18n string
  applyBarToggleTips();
}

// ---- Helpers ----
// Custom themes: the colour tokens of the `:root` block in main.css that a
// custom theme may override. Everything else stays with the inherited base theme.
const THEME_VARS = ["bg", "surface", "surface-2", "fg", "muted", "border", "accent", "accent-fg", "danger"];
// Editor layout: which tokens belong together, in the order they are shown.
const THEME_VAR_GROUPS = [
  ["themeGroupBg", ["bg"]],
  ["themeGroupSurface", ["surface", "surface-2", "border"]],
  ["themeGroupText", ["fg", "muted"]],
  ["themeGroupAccent", ["accent", "accent-fg", "danger"]],
];
// The built-in themes a custom theme can inherit from ("system" is not one of
// them — it has no rules of its own). Order matches the settings dropdown.
const BUILTIN_THEMES = ["light", "dark", "programmer", "neon", "cyberpunk", "mono", "ocean"];

const customThemes = () => (flag("customThemes") ? settings.custom_themes || [] : []);
const customThemeId = (pref) => (String(pref || "").startsWith("custom:") ? String(pref).slice(7) : "");
const findCustomTheme = (pref) => customThemes().find((c) => c.id === customThemeId(pref)) || null;

// Overrides of the active theme, straight from the local settings — the backend
// sends the same set with every theme-changed event.
function activeThemeVars() {
  return findCustomTheme(settings.theme)?.vars || {};
}

// `theme` is always a built-in key (a custom theme resolves to its base first);
// `vars` are the colour tokens layered on top as inline styles.
function applyTheme(theme, vars) {
  if (theme) document.documentElement.setAttribute("data-theme", theme);
  // Lets a native control we do not style ourselves -- the video element's own
  // three-dot menu -- follow dark/light instead of always drawing light.
  document.documentElement.style.colorScheme = themeColorScheme(theme);
  applyThemeVars(vars ?? activeThemeVars());
}

// Inline overrides are the whole custom-theme mechanism, so always clear the full
// set first — switching to a built-in theme must leave nothing behind.
function applyThemeVars(vars) {
  const style = document.documentElement.style;
  for (const key of THEME_VARS) style.removeProperty(`--${key}`);
  style.removeProperty("--shadow");
  style.removeProperty("--select-chevron");
  for (const [key, value] of Object.entries(vars || {})) {
    if (THEME_VARS.includes(key) && HEX_RE.test(value)) style.setProperty(`--${key}`, value);
  }
  // The window glow is the base theme's, so a custom accent used to sit inside a
  // shadow of the wrong colour and never followed an edit. Derive it instead.
  const accent = vars?.accent;
  if (HEX_RE.test(accent || "")) {
    style.setProperty("--shadow", `0 8px 28px color-mix(in srgb, ${accent} 32%, transparent)`);
  }
  // Same reasoning as --shadow above: a data-URI SVG cannot hold a var(), so a
  // custom theme's own muted colour needs its own baked copy of the chevron
  // or every select keeps showing the base theme's arrow colour.
  const muted = vars?.muted;
  if (HEX_RE.test(muted || "")) {
    style.setProperty("--select-chevron", selectChevronVar(muted));
  }
}

// Which built-in theme a preference renders as. A custom theme keeps its base even
// while the feature is switched off, so the dropdown still shows something truthful.
function themeBase(pref) {
  const id = customThemeId(pref);
  if (!id) return pref;
  const raw = (settings.custom_themes || []).find((c) => c.id === id);
  return raw && BUILTIN_THEMES.includes(raw.base) ? raw.base : "system";
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
// Same data-URI SVG every built-in theme bakes its own copy of in main.css
// (each with that theme's --muted colour already substituted for the stroke,
// since a data URI cannot hold a var()) -- this is the one JS-side copy, used
// only to derive --select-chevron for a custom theme's runtime override.
// Keep in sync with the CSS copy if the SVG itself ever changes.
const selectChevronVar = (hex) =>
  `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%23${hex.slice(1)}' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M6 9l6 6 6-6'/%3E%3C/svg%3E")`;

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

// `boxOnly` = skip the per-tile work while the measurement box is unchanged. Used by
// the resize handler: dragging the window edge changes the pixel size on every frame,
// but the quantized box only moves every FIT_QUANT px — re-fitting every tile in
// between is what made the grid shimmer during a resize.
function fitAllTiles(boxOnly) {
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
  let boxMoved = false;
  if (fitBox.key !== key) {
    const prevW = fitBox.maxW;
    const prevH = fitBox.maxH;
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
    boxMoved = fitBox.maxW !== prevW || fitBox.maxH !== prevH;
  }
  if (boxOnly && !boxMoved) return;
  if (fitBox.maxW <= 0 || fitBox.maxH <= 0) return;
  for (const tile of tiles) fitTileText(tile, fitBox.maxW, fitBox.maxH);
}

// Re-fit on window resize (cells change size with the window).
let fitRaf = 0;
window.addEventListener("resize", () => {
  cancelAnimationFrame(fitRaf);
  fitRaf = requestAnimationFrame(() => fitAllTiles(true));
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

// Relative luminance -> the ink that stays readable on `hex` (WCAG's 0.179 split).
function readableInk(hex) {
  const m = /^#?([0-9a-f]{6})$/i.exec(String(hex || ""));
  if (!m) return "#fff";
  const n = parseInt(m[1], 16);
  const lin = (c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  const l = 0.2126 * lin(((n >> 16) & 255) / 255) + 0.7152 * lin(((n >> 8) & 255) / 255) + 0.0722 * lin((n & 255) / 255);
  return l > 0.179 ? "#000" : "#fff";
}

// Optional action: { label, onClick } adds a button and keeps the toast longer.
// `tint` (a #rrggbb) paints the icon badge in that colour — the colour picker uses it so the
// picked colour is visible next to its value.
// Every toast is now the same themed card (icon badge + message), matching the
// update notification, so nothing looks like the old flat pill.
// `chips` is a second, independent action slot — a row of small pill buttons
// (e.g. the three colour notations) instead of the single `.toast-btn`. Kept
// separate from `action` rather than folding it in, so every existing single-action
// caller (undo, update-available, …) is untouched.
// `duration` overrides the computed auto-hide delay (ms) for this one call --
// the background OCR-language-update notice uses its own ocrLangNoticeMs expert
// setting instead of the generic TOAST_MS/action-bonus math below. Every existing
// caller omits it and keeps the old behaviour.
// Named (not the pill/folder windows' shared COPIED_MS=900 -- this toast
// chip flash has always run shorter; unifying the value would be a behaviour change).
const CHIP_COPIED_MS = 400;
function toast(msg, action = null, tint = "", chips = null, duration = null) {
  toastEl.textContent = "";
  const hasChips = !!(chips && chips.length);
  // Only actionable toasts capture clicks; plain ones stay click-through so they
  // never block the UI underneath. The card layout lives on the base .toast now.
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
      // Copies again without closing the toast — comparing/re-picking a different
      // notation is the whole point, unlike the single-action button above. The
      // clicked chip becomes "current" too: it is now what sits on the clipboard,
      // and the row must show exactly one such chip at a time.
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
  // A toast carrying an action button or chips needs longer to read/click than a
  // plain message, but was a flat 12000ms regardless of the toastMs expert
  // setting -- longer than that setting's own 10000ms max. Scale it off the same
  // knob instead (same 12000ms result at the 1400ms default).
  toastTimer = setTimeout(hideToast, duration ?? ((action || hasChips) ? TOAST_MS + 10600 : TOAST_MS));
}

// The backend returns Err(String) with a plain English sentence; most call sites
// just did errToast(err), which shows raw English no matter the app language.
// Known error strings get mapped to a translated key here; anything not in the map
// (an unexpected error, or a plain error code like "canceled"/"save-failed" a
// caller already checks for itself) still falls back to the raw string so nothing
// is silently swallowed.
const ERROR_MSG_KEYS = {
  "cannot delete the last view": "errLastView",
  "theme name is empty": "errThemeNameEmpty",
  "theme name contains a control character": "errThemeNameControlChar",
  "group name is empty": "errGroupNameEmpty",
  "a group needs at least one view": "errGroupNeedsView",
  "tag name is empty": "errTagNameEmpty",
  "tag name contains a comma or a control character": "errTagNameInvalid",
  "installer too large / truncated": "errUpdaterTooLarge",
  "downloaded file is not a valid installer": "errUpdaterInvalidInstaller",
  "update hash mismatch — download rejected": "errUpdaterHashMismatch",
  "backup expands to more than 512 MB": "errBackupTooLarge",
  "store-too-large-for-backup": "errStoreTooLargeForBackup",
  "backup-schema-mismatch": "errBackupSchemaMismatch",
  "not in a folder": "errNotInFolder",
  "not a member of this folder": "errNotFolderMember",
  "color picker is off": "errColorPickerOff",
  "autostart is only supported on Windows": "errAutostartWindowsOnly",
  "delete-incomplete": "errDeleteIncomplete",
  "unsupported-export-format": "exportUnsupportedFormat",
  "recording-target-switch-not-supported": "recTargetSwitchUnsupported",
  "recording-target-switch-different-monitor": "recTargetSwitchDifferentMonitor",
  "recording-target-switch-invalid-window": "recTargetSwitchInvalidWindow",
  "recording-target-switch-timeout": "recTargetSwitchTimeout",
};
function errToast(err) {
  const s = String(err);
  toast(ERROR_MSG_KEYS[s] ? t(ERROR_MSG_KEYS[s]) : s);
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

// ---- Colour-pick history popover (right-click on #picker-btn) ----
const pickHistoryPop = document.createElement("div");
pickHistoryPop.className = "pick-history-pop hidden";
document.body.appendChild(pickHistoryPop);
function closePickHistoryPop() { pickHistoryPop.classList.add("hidden"); }
document.addEventListener("pointerdown", (e) => {
  if (pickHistoryPop.classList.contains("hidden")) return;
  if (!pickHistoryPop.contains(e.target) && !e.target.closest("#picker-btn")) {
    closePickHistoryPop();
  }
});

// ---- Background jobs popover (header button) --------------------------
// The registry lives entirely in Rust (see list_bg_ops/push_bg_op) and is
// shared with the taskbar icon -- this just displays it. Export runs in the
// video-editor window, convert and the ffmpeg download can run from this one,
// but every one of their progress events is broadcast app-wide, so this
// window learns about all three the same way regardless of which window
// actually started them.
const BG_OP_CANCEL_CMD = {
  export: "cancel_export",
  convert: "cancel_recording_convert",
  "ffmpeg-download": "cancel_ffmpeg_download",
};
function bgKindNoun(kind) {
  if (kind === "export") return t("bgKindExport");
  if (kind === "convert") return t("bgKindConvert");
  return t("bgKindDownload");
}
// The three progress strings ("Converting… {s}s (click to cancel)", etc.) all
// start with the same short present-tense phrase this popover needs -- reusing
// them here avoids a fourth near-duplicate translation for every language.
function bgOpLabel(kind) {
  const key = kind === "export" ? "veExporting" : kind === "convert" ? "recConverting" : "ffmpegDownloading";
  const s = t(key);
  const i = s.indexOf("{");
  return (i >= 0 ? s.slice(0, i) : s).trim();
}
const bgOpsPop = document.createElement("div");
bgOpsPop.className = "pick-history-pop hidden";
document.body.appendChild(bgOpsPop);
function closeBgOpsPop() { bgOpsPop.classList.add("hidden"); }
document.addEventListener("pointerdown", (e) => {
  if (bgOpsPop.classList.contains("hidden")) return;
  if (!bgOpsPop.contains(e.target) && !e.target.closest("#bg-ops-btn")) closeBgOpsPop();
});

function renderBgOpsPop(ops) {
  bgOpsPop.innerHTML = "";
  if (!ops.length) {
    const empty = document.createElement("div");
    empty.className = "pick-history-empty";
    empty.textContent = t("bgNone");
    bgOpsPop.appendChild(empty);
    return;
  }
  for (const op of ops) {
    const row = document.createElement("div");
    row.className = "bg-ops-row";
    const label = document.createElement("span");
    label.className = "bg-ops-label";
    label.textContent = bgOpLabel(op.kind);
    const pct = document.createElement("span");
    pct.className = "bg-ops-pct";
    pct.textContent = op.state === "error" ? "–" : op.state === "done" ? "100%" : (op.pct != null ? `${op.pct}%` : "…");
    row.append(label, pct);
    if (op.target) {
      const openBtn = document.createElement("button");
      openBtn.type = "button";
      openBtn.className = "ghost-btn bg-ops-action";
      openBtn.textContent = t("bgOpenFolder");
      openBtn.addEventListener("click", () => invoke("open_bg_op_folder", { path: op.target }).catch((err) => errToast(err)));
      row.appendChild(openBtn);
    }
    if (op.state === "running" || op.state === "indeterminate") {
      const cancelCmd = BG_OP_CANCEL_CMD[op.kind];
      if (cancelCmd) {
        const cancelBtn = document.createElement("button");
        cancelBtn.type = "button";
        cancelBtn.className = "ghost-btn bg-ops-action";
        cancelBtn.textContent = t("cancel");
        cancelBtn.addEventListener("click", () => invoke(cancelCmd).catch(() => {}));
        row.appendChild(cancelBtn);
      }
    }
    bgOpsPop.appendChild(row);
  }
}

// Keyed by "kind:started_at_ms" -- the registry itself is keyed by kind alone
// (only one job per kind at a time today), but that combination is what tells
// this toast-once check a NEW run of the same kind from one it already showed.
const bgOpsToasted = new Set();
let bgOpsFirstRun = true;

async function refreshBgOps() {
  let ops = [];
  try { ops = (await invoke("list_bg_ops")) || []; } catch { return; }
  const running = ops.filter((o) => o.state === "running" || o.state === "indeterminate");
  $("bg-ops-btn").classList.toggle("hidden", running.length === 0);
  $("bg-ops-badge").classList.toggle("hidden", running.length === 0);
  $("bg-ops-badge").textContent = String(running.length);
  if (!bgOpsPop.classList.contains("hidden")) renderBgOpsPop(ops);
  for (const op of ops) {
    if (op.state !== "done" && op.state !== "error") continue;
    const key = `${op.kind}:${op.started_at_ms}`;
    if (bgOpsToasted.has(key)) continue;
    bgOpsToasted.add(key);
    // First call after load just learns the current state -- a job finished
    // before this window ever asked about it does not need a toast for it.
    if (bgOpsFirstRun || op.state !== "done") continue;
    const action = op.target
      ? { label: t("bgOpenFolder"), onClick: () => invoke("open_bg_op_folder", { path: op.target }).catch((err) => errToast(err)) }
      : null;
    toast(t("bgDoneToast").replace("{what}", bgKindNoun(op.kind)), action);
  }
  bgOpsFirstRun = false;
}

$("bg-ops-btn").addEventListener("click", async () => {
  if (!bgOpsPop.classList.contains("hidden")) { closeBgOpsPop(); return; }
  let ops = [];
  try { ops = (await invoke("list_bg_ops")) || []; } catch { /* show empty rather than nothing */ }
  renderBgOpsPop(ops);
  bgOpsPop.classList.remove("hidden");
  placePopover(bgOpsPop, $("bg-ops-btn").getBoundingClientRect());
});

// push_bg_op (Rust) broadcasts this on every state change -- running tick,
// done, error -- regardless of which window (if any) started the job, so this
// popover stays live without a separate poll timer.
listen("bg-ops-changed", refreshBgOps);
refreshBgOps();

listen("bg-quit-requested", async (e) => {
  const n = Number(e.payload) || 0;
  const ok = await confirmDialog({
    title: t("bgTitle"),
    message: t("bgQuitRunning").replace("{n}", String(n)),
    confirmLabel: t("bgCancelClose"),
    danger: true,
  });
  if (ok) invoke("confirm_quit_with_running_ops").catch(() => {});
});

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

// placePopover/placePopoverAt now live in ui/popover.js -- the one placement
// rule for every popover, dropdown and context menu in THIS window. See that
// file for why floating.js/folder.js and media.js's volume popup stay separate.

function openNumPop(input, apply) {
  popInput = input;
  popApply = apply;
  // Fill first: the placement needs a laid-out list, and so does centering the
  // selected row.
  numPop.classList.remove("hidden");
  renderNumPop();
  placePopover(numPop, input.getBoundingClientRect());
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
    // A disabled option is a section separator, not a choice.
    if (item.disabled) {
      row.className = "sep";
      row.disabled = true;
      numPop.appendChild(row);
      continue;
    }
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
  // Width follows the widest entry, not the anchor — so the clamp has to run on the
  // real box, which is why it happens after the rows exist.
  placePopover(numPop, anchor.getBoundingClientRect());
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
      disabled: o.disabled,
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
// Shared with the video editor through colorpop.js, the one implementation
// both windows load.
const { el: colorPop, open: openColorPop, close: closeColorPop } = createColorPop();

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
// A delete confirms on the button itself instead of in a dialog — first press
// arms it (fully red, warning glyph, the count spelled out), a second press within
// DISARM_MS deletes. One place to look instead of two, and the pointer never has to
// travel to a dialog that just appeared under it.
const ARM_ICON = '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" d="M12 9v4M12 17h.01M10.3 4 2.6 17.5A1.8 1.8 0 0 0 4.2 20h15.6a1.8 1.8 0 0 0 1.6-2.5L13.7 4a1.9 1.9 0 0 0-3.4 0Z"/></svg>';
let armDeleteTimer = 0;
function armDelete(btn, armedLabel, run) {
  clearTimeout(armDeleteTimer);
  if (btn.classList.contains("confirm")) {
    disarmButton(btn, t("delete"));
    run();
    return;
  }
  btn.classList.add("confirm");
  btn.textContent = "";
  btn.insertAdjacentHTML("beforeend", ARM_ICON); // static, developer-controlled markup
  btn.insertAdjacentText("beforeend", armedLabel);
  armDeleteTimer = setTimeout(() => disarmButton(btn, t("delete")), DISARM_MS);
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
  requestAnimationFrame(() => fitAllTiles());
}

function setBars(header, composer) {
  settings.show_header = header;
  settings.show_composer = composer;
  invoke("set_bars", { header, composer }).catch(() => {});
  applyBars();
}

// Mentions the double-click shortcut in the four bar buttons' own tooltip
// while barToggleBoth is on, reusing the expert-menu row's own explanation
// text instead of a second translated sentence.
function applyBarToggleTips() {
  const hint = flag("barToggleBoth") ? ` ${t("tipBarToggleBoth")}` : "";
  for (const id of ["hide-top", "hide-bottom"]) $(id).title = t("hideBar") + hint;
  for (const id of ["show-top", "show-bottom"]) $(id).title = t("showBar") + hint;
}

// ---- Expert feature flags & values ----
// Every feature ships enabled; the expert menu lets the user switch single
// pieces off. A missing flag means enabled, so old configs keep everything on.
const FLAG_LABELS = {
  fileAttach: "flagFileAttach",
  screenshot: "flagScreenshot",
  snipEditor: "flagSnipEditor",
  snipEditSnap: "flagSnipEditSnap",
  snipTextPlate: "flagSnipTextPlate",
  multiView: "flagMultiView",
  quickGrid: "flagQuickGrid",
  floating: "flagFloating",
  barToggles: "flagBarToggles",
  barToggleBoth: "flagBarToggleBoth",
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
  // Windows taskbar icon progress during export, conversion, backup, import
  // and the ffmpeg download; and the shared "about how long is left" estimate
  // shown on every progress bar in the app.
  taskbarProgress: "flagTaskbarProgress",
  etaShow: "flagEtaShow",
  importExport: "flagImportExport",
  tileReorder: "flagTileReorder",
  libraryTypeFilter: "flagLibraryTypeFilter",
  journalSearch: "flagJournalSearch",
  journalTypeFilter: "flagJournalTypeFilter",
  journalColorFilter: "flagJournalColorFilter",
  journalTagFilter: "flagJournalTagFilter",
  journalClear: "flagJournalClear",
  journalMostUsed: "flagJournalMostUsed",
  journalRecent: "flagJournalRecent",
  journalGroupBtn: "flagJournalGroupBtn",
  journalRecentSort: "flagJournalRecentSort",
  journalUsedSort: "flagJournalUsedSort",
  confirmClearHistory: "flagConfirmClearHistory",
  chainLock: "flagChainLock",
  chainQuickStart: "flagChainQuickStart",
  favorites: "flagFavorites",
  keyboardNav: "flagKeyboardNav",
  varDefaults: "flagVarDefaults",
  favViewButton: "flagFavViewButton",
  favViewReorder: "flagFavViewReorder",
  tileShadow: "flagTileShadow",
  tilePressScale: "flagTilePressScale",
  tileHoverLift: "flagTileHoverLift",
  copyFlash: "flagCopyFlash",
  headerSeparators: "flagHeaderSeparators",
  cleanupFiles: "flagCleanupFiles",
  // A text prompt can hold a list of variants instead of one text; each
  // copy serves the next entry (in order, or shuffled without repeats). Default
  // on since 33 — shipped default off at 32's original release.
  rotatingPrompts: "flagRotatingPrompts",
  // Sub-toggle of rotatingPrompts, itself also default on.
  rotateBadge: "flagRotateBadge",
  // Sub-toggle of rotatingPrompts, default on.
  rotateAuto: "flagRotateAuto",
  // New features (default on).
  lengthCounter: "flagLengthCounter",
  smartSort: "flagSmartSort",
  fuzzySearch: "flagFuzzySearch",
  batchOps: "flagBatchOps",
  promptHistory: "flagPromptHistory",
  usageStats: "statsEnable",            // only ever rendered in the backups dialog
  dupFinder: "flagDupFinder",
  dupImportCheck: "flagDupImportCheck",
  autoBackup: "flagAutoBackup",
  autoPaste: "flagAutoPaste",           // a normal setting now (default on)
  dynVars: "flagDynVars",               // self-filling placeholder names
  choiceVars: "flagChoiceVars",         // {#{name|a,b}#} dropdown variables
  choiceRemember: "flagChoiceRemember", // reopen a choice on its last pick
  varPopupWindow: "flagVarPopupWindow", // ask for placeholders in a small window of their own
  codeHighlight: "flagCodeHighlight",   // syntax colours in the preview's code blocks
  codeAutoDetect: "flagCodeAutoDetect", // render an unfenced code prompt as code
  notesInTooltip: "flagNotesInTooltip", // show the private note in the tile tooltip
  tileTimeTip: "flagTileTimeTip",       // show last-copied/last-edited in the tile/library tooltip
  promptHotkeys: "flagPromptHotkeys",   // per-prompt global hotkeys
  // Sub-toggle of promptHotkeys, default on -- moved from
  // OPT_FLAG_LABELS; no CSS class keys on this flag, so the move is risk-free.
  promptHotkeyPaste: "flagPromptHotkeyPaste",
  // A custom, rounded/themed tray context menu instead of the native Win32
  // popup. Off falls back to the plain OS menu -- the escape hatch if the
  // custom window ever misbehaves on some setup.
  trayCustomMenu: "flagTrayCustomMenu",
  trayRecent: "flagTrayRecent",         // "recently used" section in the tray menu
  colorPicker: "flagColorPicker",       // pipette — pick a screen colour to the clipboard
  // Windows Media.Ocr text recognition (screenshot / image file -> editable text).
  // Ships ON; the button hides itself when Windows has no recognizer at all.
  ocr: "flagOcr",
  customThemes: "flagCustomThemes",     // theme editor + custom entries in the theme list
  tileFolders: "flagTileFolders",       // drop a tile into another to make a grid folder
  tags: "flagTags",                     // tags + library tag filter
  pillFolders: "flagPillFolders",       // merge floating buttons into folders
  trayFavs: "flagTrayFavs",             // favourites section in the tray menu
  // Checks at launch, but only while the normal auto-update setting is on.
  checkUpdateOnStart: "flagCheckUpdateOnStart",
  // Split from the old single autoBackupBeforeWipe so either case can be
  // switched off on its own. Both ship default on.
  backupBeforeReset: "flagBackupBeforeReset",
  backupBeforeDelete: "flagBackupBeforeDelete",
  // Help panel — the tip line under each article, and search-hit highlighting.
  helpTips: "flagHelpTips",
  helpHighlight: "flagHelpHighlight",
  // Word-level diff in the version-history expanded view.
  historyDiff: "flagHistoryDiff",
  // Sub-toggles of ocr (OPT_FLAG_LABELS, default off), themselves default on.
  ocrAutoCopy: "flagOcrAutoCopy",
  ocrJoinLines: "flagOcrJoinLines",
  // Sub-toggles of ocr, themselves default on.
  ocrAutoPickLang: "flagOcrAutoPickLang",
  ocrLangAutoUpdate: "flagOcrLangAutoUpdate",
  // Auto-opens the five-step first-run tour once, right after the example
  // prompts are seeded. Off only stops the AUTOMATIC open — the Help-menu entry
  // that reopens the tour manually stays available either way.
  firstRunTour: "flagFirstRunTour",
  // Voice dictation (Windows.Media.SpeechRecognition). Ships ON, like ocr; the
  // button hides itself when Windows reports no speech engine at all.
  voice: "flagVoice",
  voiceInterim: "flagVoiceInterim",
  voicePunct: "flagVoicePunct",
  voiceAutoCaps: "flagVoiceAutoCaps",
  voiceAutoStop: "flagVoiceAutoStop",
  voiceAutoPickLang: "flagVoiceAutoPickLang",
  voiceLangAutoUpdate: "flagVoiceLangAutoUpdate",
  // Screen recording. Ships ON, like ocr/voice.
  screenRecord: "flagScreenRecord",
  // Sub-toggle of screenRecord: whether the mouse pointer appears in the
  // recording. Ships ON.
  recCursor: "flagRecCursor",
  // Sub-toggle of screenRecord: system audio (WASAPI loopback) into the
  // recording's audio track. Ships ON.
  recAudioSystem: "flagRecAudioSystem",
  // Sub-toggle of screenRecord, microphone audio into its own track --
  // moved here from OPT_FLAG_LABELS, its shipped default flipping from off to
  // on (into a sidecar track only, muxed in audibly later from the video
  // editor -- see lib.rs's AudioConfig.mic comment). A stored user value from
  // before this change, or an explicit opt-out, still wins.
  recAudioMic: "flagRecAudioMic",
  // A running recording would otherwise have only the bar itself as a way
  // to stop or pause it. All three ship ON.
  // Sub-toggle of screenRecord: the one-time reactive notice offering to
  // switch the recording canvas to "every display" the first time a window
  // recording (still on its "window size" default) actually grows past its
  // starting size and starts losing picture at the edge. Ships ON.
  recCropNotice: "flagRecCropNotice",
  // The control bar's file-size readout, past recBarSizeFromMb, plus a
  // highlighted ETA once the recording nears recMaxMb. Ships ON.
  recBarSize: "flagRecBarSize",
  // The control bar's target-switch button: swaps a running window
  // recording's own source without restarting it. Ships ON.
  recTargetSwitch: "flagRecTargetSwitch",
  recStopHotkey: "flagRecStopHotkey",
  recPauseHotkey: "flagRecPauseHotkey",
  recTrayEntries: "flagRecTrayEntries",
  // Catches a crashed recording's unplayable leftover before it sits
  // there forever -- ships ON, like the rest of the recorder's own upkeep.
  recSweepBroken: "flagRecSweepBroken",
  // The video editor satellite window, reachable from a recording's
  // result modal and from any video prompt's context menu. Ships ON, like the
  // rest of the media-editing features (snipEditor, ocr, voice).
  videoEditor: "flagVideoEditor",
  // Only matters once clipWatcher (itself off by default) is on -- ships ON so a
  // guessed password never has to be manually turned away in the first place.
  clipSkipSecrets: "flagClipSkipSecrets",
};
const ALL_FLAG_KEYS = Object.keys(FLAG_LABELS);

// Opt-in flags: OFF by default (the mirror of the flags above, which are ON by
// default). Enabled only when explicitly set true; wired via a `flag-<key>` body
// class that is present exactly when the option is on.
const OPT_FLAG_LABELS = {
  // Features extras
  viewGroups: "flagViewGroups",                 // folder tabs for views
  // The tile's own chain-start icon otherwise only appears on hover -- kept as an
  // opt-in for whoever prefers it dimly visible on every tile all the time (04.09.
  // decision); the 05.09. decision reversed the shipped default back to hover-only.
  chainStartAlwaysVisible: "flagChainStartAlwaysVisible",
  gridViewBorder: "flagGridViewBorder",         // colored border around the grid in the view's color
  quickGridHoverOnly: "flagQuickGridHoverOnly", // quick-grid control shows only while the header is hovered
  // Named from the user's side: the window is kept out of screenshots, and this is
  // the switch that lets them through. Ships off, so the protection is the default
  // and whoever misses their screenshots finds the reason by its name.
  allowScreenshot: "flagAllowScreenshot",
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
  tilesHoverDim: "flagTilesHoverDim",
  // Media extras
  grayscaleMedia: "flagGrayscaleMedia",
  mediaHoverDim: "flagMediaHoverDim",
  mediaBorder: "flagMediaBorder",
  roundMedia: "flagRoundMedia",
  // New features (default off).
  mdPreview: "flagMdPreview",           // rendered Markdown preview in the editor
  autoPasteEnter: "flagAutoPasteEnter",
  clipWatcher: "flagClipWatcher",
  backupGfs: "flagBackupGfs",           // period-based retention
  // v2.6.0 additions (all default off → "(Standard Aus)").
  editorMonospace: "flagEditorMonospace",       // monospace editor textarea
  toolbarHoverDim: "flagToolbarHoverDim",        // header dims until hovered
  hideWindowOnCopy: "flagHideWindowOnCopy",     // hide window after copying a prompt
  dedupCopyLog: "flagDedupCopyLog",             // skip identical consecutive copies
  // Keep the clipboard-inbox and copy-history buttons in the toolbar even while
  // their feature is switched off, so the feature can be paused, not hidden.
  pausedIcons: "flagPausedIcons",
  promptNotes: "flagPromptNotes",               // private notes row in the editor
  dragOut: "flagDragOut",                       // drag a prompt into another app
  // Swaps the two gestures: off (shipped) = hold the key to drag out, plain drag
  // moves the tile; on = plain drag leaves the app and the key moves the tile.
  dragOutInvert: "flagDragOutInvert",
  // Off = one instance app-wide (a second launch focuses the first, as always). On =
  // the OS-level single-instance guard is skipped entirely at startup, so several
  // windows can run side by side. Read once at launch, before settings normally
  // load — toggling it needs a restart to take effect (flagRow() toasts that).
  allowMultipleInstances: "flagAllowMultipleInstances",
  // Off keeps the historical "one placement" guarantee (adding elsewhere moves
  // it there); on lets a prompt sit in several views at once, sharing the same data.
  multiViewPrompt: "flagMultiViewPrompt",
  // Prompt-folders-as-Markdown (beta — see the module note above sync_enabled() in
  // lib.rs): mirrors every text-only prompt as a .md file in a folder, editable by
  // hand or another tool.
  promptSync: "flagPromptSync",
  // Voice dictation's two default-off options: sending audio to Microsoft's
  // online recognizer needs its own consent, and a global hotkey
  // is a bigger commitment than the in-window button.
  voiceOnlineOk: "flagVoiceOnlineOk",
  voiceHotkey: "flagVoiceHotkey",
  // A short synthesised beep on every successful copy, mainly for the floating
  // button (easy to miss visually) and for screen-reader users.
  copySound: "flagCopySound",
};
const OPT_FLAG_KEYS = Object.keys(OPT_FLAG_LABELS);
const isOptFlag = (key) => key in OPT_FLAG_LABELS;
// A brand may ship a feature the other way round than this app does (see
// brands/<slug>.json "flagDefaults"). A stored value always wins, so the default
// only decides what an untouched install starts with.
const shipsOn = (key) => {
  const v = FLAG_DEFAULTS[key];
  return typeof v === "boolean" ? v : !isOptFlag(key);
};
const optFlag = (key) => (settings.ui_flags?.[key] ?? shipsOn(key)) === true;
// Current state + label key, whichever kind of flag this is.
const flagState = (key) => (isOptFlag(key) ? optFlag(key) : flag(key));
const flagLabelKey = (key) => OPT_FLAG_LABELS[key] || FLAG_LABELS[key];
// A setting may declare `gate: "flagKey"` (or "!flagKey" to invert) so its row hides
// while that feature is off — no dead controls for disabled things. Values, selects,
// dropdowns and paths carry it in their definition; a flag carries it in the group's
// `flags` list, written as { key, gate } instead of a bare key.
const flagEntry = (f) => (typeof f === "string" ? { key: f } : f);
const groupFlags = (group) => (group.flags || []).map(flagEntry);
// A sub-toggle counts as off while its own parent is off, so everything hanging off
// it disappears with the whole branch instead of surviving as an orphan row.
const gateOpen = (gate, seen) => {
  if (!gate) return true;
  const inv = gate[0] === "!";
  const key = inv ? gate.slice(1) : gate;
  let on = flagState(key);
  const guard = seen || new Set();
  if (on && !guard.has(key)) {
    guard.add(key);
    on = gateOpen(FLAG_GATES[key], guard);
  }
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
  bubbleMs: { label: "valBubbleMs", min: 300, max: 6000, step: 50, def: 950, unit: "ms", gate: "copyBubble" },
  toastMs: { label: "valToastMs", min: 800, max: 10000, step: 100, def: 1400, unit: "ms" },
  previewLen: { label: "valPreviewLen", min: 0, max: 4000, step: 20, def: 600, unit: "" },
  viewBorder: { label: "valViewBorder", min: 1, max: 12, step: 1, def: 3, unit: "px", gate: "multiView" },
  // Floating-button opacity (%). Floored at 20% so the pill never vanishes.
  floatOpacity: { label: "valFloatOpacity", min: 20, max: 100, step: 5, def: 100, unit: "%", gate: "floating" },
  // How close two floating buttons must be on drop to merge, and how wide the
  // grid of an opened folder is. Both mirrored by the backend (merge_radius/folder_cols).
  pillMergeRadius: { label: "valPillMergeRadius", min: 20, max: 80, step: 1, def: 40, unit: "px", gate: "pillFolders" },
  folderGridCols: { label: "valFolderGridCols", min: 2, max: 5, step: 1, def: 3, unit: "", gate: "pillFolders" },
  // Grid folders: how much of a tile counts as its middle, how long the pointer
  // has to rest there before the folder is offered, and how wide the open folder is.
  tileFolderZone: { label: "valTileFolderZone", min: 30, max: 80, step: 5, def: 55, unit: "%", gate: "tileFolders" },
  tileFolderHoverMs: { label: "valTileFolderHoverMs", min: 100, max: 800, step: 50, def: 300, unit: "ms", gate: "tileFolders" },
  tileFolderCols: { label: "valTileFolderCols", min: 2, max: 5, step: 1, def: 3, unit: "", gate: "tileFolders" },
  // Floating pill appearance -- font, minimum text-pill width, media-pill border,
  // resize-ring thickness and the gap to its own context menu.
  pillFontSize: { label: "valPillFontSize", min: 14, max: 40, step: 1, def: 26, unit: "px", gate: "floating" },
  pillTextMinW: { label: "valPillTextMinW", min: 100, max: 400, step: 10, def: 200, unit: "px", gate: "floating" },
  pillBorderW: { label: "valPillBorderW", min: 0, max: 12, step: 1, def: 5, unit: "px", gate: "floating" },
  pillRingFrac: { label: "valPillRingFrac", min: 5, max: 40, step: 1, def: 18, unit: "%", gate: "floating" },
  pillMenuGap: { label: "valPillMenuGap", min: 0, max: 20, step: 1, def: 6, unit: "px", gate: "floating" },
  // Pill folders: how much a pill grows for the merge-target hint, and the open
  // folder popover's own tile size / gap / padding.
  pillMergeGrow: { label: "valPillMergeGrow", min: 0, max: 50, step: 1, def: 12, unit: "%", gate: "pillFolders" },
  pillFolderTileSize: { label: "valPillFolderTileSize", min: 48, max: 120, step: 4, def: 72, unit: "px", gate: "pillFolders" },
  pillFolderTileGap: { label: "valPillFolderTileGap", min: 0, max: 24, step: 1, def: 8, unit: "px", gate: "pillFolders" },
  pillFolderPad: { label: "valPillFolderPad", min: 0, max: 24, step: 1, def: 10, unit: "px", gate: "pillFolders" },
  pillFolderPopGap: { label: "valPillFolderPopGap", min: 0, max: 20, step: 1, def: 6, unit: "px", gate: "pillFolders" },
  // Custom tile tooltip max width (it wraps + grows taller instead of widening).
  tooltipWidth: { label: "valTooltipWidth", min: 180, max: 640, step: 10, def: 340, unit: "px", gate: "tilePreview" },
  // How long the pointer must rest on an element before its tooltip appears, so a
  // casual sweep across the window shows nothing (paired with tooltipDelay).
  tooltipDelayMs: { label: "valTooltipDelay", min: 100, max: 2000, step: 50, def: 500, unit: "ms", gate: "tooltipDelay" },
  // How long the hover tooltip stays before auto-hiding (paired with tooltipTimeout).
  tooltipTimeoutMs: { label: "valTooltipTimeout", min: 2000, max: 60000, step: 1000, def: 15000, unit: "ms", gate: "tooltipTimeout" },
  // Appearance scales (percent, 100 = unchanged). Applied as CSS zoom.
  uiScale: { label: "valUiScale", min: 75, max: 300, step: 5, def: 100, unit: "%" },
  modalScale: { label: "valModalScale", min: 75, max: 300, step: 5, def: 100, unit: "%" },
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
  // Copy-history dialog width.
  journalWidth: { label: "valJournalWidth", min: 520, max: 1400, step: 20, def: 980, unit: "px", gate: "copyHistory" },
  // Block re-copying the SAME prompt within this window (0 = off). Never blocks others.
  copyCooldownMs: { label: "valCopyCooldown", min: 0, max: 10000, step: 100, def: COOLDOWN_MS_DEFAULT, unit: "ms" },
  copySoundVolume: { label: "valCopySoundVolume", min: 0, max: 100, step: 5, def: COPY_SOUND_VOLUME_DEFAULT, unit: "%", gate: "copySound" },
  // Header favorites-star size.
  favStarSize: { label: "valFavStarSize", min: 14, max: 48, step: 1, def: 22, unit: "px", gate: "favViewButton" },
  // Favorites grid: cap the column count (0 = auto-size to the count).
  favMaxCols: { label: "valFavMaxCols", min: 0, max: 20, step: 1, def: 0, unit: "", gate: "favViewButton" },
  // Prompt-list (library) column options: how many columns the header toggle offers.
  libraryCols: { label: "valLibraryCols", min: 1, max: 6, step: 1, def: 1, unit: "", gate: "librarySearch" },
  libPreviewLines: { label: "valLibPreviewLines", min: 1, max: 4, step: 1, def: 2, unit: "" },
  // Prompt-list (library) dialog width.
  libraryWidth: { label: "valLibraryWidth", min: 520, max: 1400, step: 20, def: 900, unit: "px" },
  // How many recent search terms the suggestions dropdown keeps + shows.
  searchRecentMax: { label: "valSearchRecentMax", min: 1, max: 20, step: 1, def: 8, unit: "", gate: "searchSuggest" },
  // How long the library search waits after the last keystroke before it re-filters.
  searchDebounceMs: { label: "valSearchDebounceMs", min: 0, max: 600, step: 10, def: 120, unit: "ms", gate: "librarySearch" },
  // Tile geometry.
  tileRadius: { label: "valTileRadius", min: 0, max: 30, step: 1, def: 10, unit: "px" },
  tileBorderWidth: { label: "valTileBorderWidth", min: 0, max: 6, step: 1, def: 1, unit: "px" },
  // Tile hover-lift travel (paired with the tileHoverLift toggle).
  hoverLift: { label: "valHoverLift", min: 1, max: 12, step: 1, def: 1, unit: "px", gate: "tileHoverLift" },
  // Drag-vs-click distance before a pointer-down turns into a drag (tiles, tabs, folders).
  dragThreshold: { label: "valDragThreshold", min: 2, max: 20, step: 1, def: DRAG_THRESHOLD_DEFAULT, unit: "px" },
  // Header vertical padding + screenshot-preview max height.
  headerPadY: { label: "valHeaderPadY", min: 2, max: 30, step: 1, def: 10, unit: "px" },
  snipPreviewVh: { label: "valSnipPreviewVh", min: 30, max: 92, step: 1, def: 74, unit: "%", gate: "screenshot" },
  // Screenshot JPEG quality + a delay before the capture (lets a menu stay open in shot).
  snipQuality: { label: "valSnipQuality", min: 50, max: 100, step: 5, def: 80, unit: "%", gate: "screenshot" },
  snipDelayMs: { label: "valSnipDelayMs", min: 0, max: 5000, step: 100, def: 0, unit: "ms", gate: "screenshot" },
  // The actual saved/copied/exported JPEG quality -- every save/copy path, edited
  // or not, shares this one value (see SNIP_SAVE_Q in lib.rs). Not the same
  // setting as snipQuality above, which only affects the live capture preview.
  snipSaveQuality: { label: "valSnipSaveQuality", min: 50, max: 100, step: 5, def: 90, unit: "%", gate: "screenshot" },
  // Screenshot-editor drawing defaults.
  snipPenPx: { label: "valSnipPenPx", min: 1, max: 40, step: 1, def: 6, unit: "px", gate: "snipEditor" },
  snipMarkerPx: { label: "valSnipMarkerPx", min: 4, max: 60, step: 2, def: 18, unit: "px", gate: "snipEditor" },
  snipTextPx: { label: "valSnipTextPx", min: 10, max: 96, step: 2, def: 24, unit: "px", gate: "snipEditor" },
  snipPixelBlock: { label: "valSnipPixelBlock", min: 4, max: 64, step: 2, def: 12, unit: "px", gate: "snipEditor" },
  snipSpotlightDim: { label: "valSnipSpotlightDim", min: 10, max: 90, step: 5, def: 55, unit: "%", gate: "snipEditor" },
  snipEditSnapTolerancePx: { label: "valSnipEditSnapTolerancePx", min: 2, max: 20, step: 1, def: 6, unit: "px", gate: "snipEditSnap" },
  snipEditUndoMax: { label: "valSnipEditUndoMax", min: 10, max: 200, step: 10, def: 50, unit: "", gate: "snipEditor" },
  videoEditUndoMax: { label: "valVideoEditUndoMax", min: 10, max: 200, step: 10, def: 50, unit: "", gate: "videoEditor" },
  // The H.264 bitrate each export-quality tier actually encodes at -- the
  // "small"/"high" dropdown just picks between these two.
  veExportKbpsHigh: { label: "valVeExportKbpsHigh", min: 4000, max: 30000, step: 500, def: 12000, unit: "kbps", gate: "videoEditor" },
  veExportKbpsSmall: { label: "valVeExportKbpsSmall", min: 500, max: 8000, step: 250, def: 4000, unit: "kbps", gate: "videoEditor" },
  // Seeds both the fade-in and fade-out fields the moment "Fade in and out" is
  // first switched on for an export -- each field stays freely adjustable
  // afterward, only the starting value comes from here.
  veFadeMs: { label: "valVeFadeMs", min: 100, max: 5000, step: 100, def: 500, unit: "ms", gate: "videoEditor" },
  veOvPixelateBlock: { label: "valVeOvPixelateBlock", min: 4, max: 64, step: 2, def: 16, unit: "px", gate: "videoEditor" },
  veOvSpotlightDim: { label: "valVeOvSpotlightDim", min: 10, max: 95, step: 5, def: 60, unit: "%", gate: "videoEditor" },
  veOvImageOpacity: { label: "valVeOvImageOpacity", min: 10, max: 100, step: 5, def: 100, unit: "%", gate: "videoEditor" },
  // A crop smaller than this (either side) is upscaled 2x before recognition --
  // the trick that makes small on-screen text readable to the OCR engine.
  ocrUpscaleUnderPx: { label: "valOcrUpscaleUnderPx", min: 100, max: 800, step: 50, def: 300, unit: "px", gate: "ocr" },
  // Delay between picking a recording target and the capture actually starting -- 0 disables it.
  recCountdownS: { label: "valRecCountdownS", min: 0, max: 10, step: 1, def: 3, unit: "s", gate: "screenRecord" },
  // The encoder always recorded at a fixed 30fps regardless of the source
  // monitor's real refresh rate until this was added -- a 60/144Hz screen can
  // now be captured at its own rate instead of being silently capped.
  recFps: { label: "valRecFps", min: 10, max: 60, step: 5, def: 30, unit: "fps", gate: "screenRecord" },
  recAudioKbps: { label: "valRecAudioKbps", min: 64, max: 320, step: 32, def: 192, unit: "kbps", gate: "screenRecord" },
  // Native H.264 bitrate. Has no effect once recQuality is "lossless" -- that
  // step is produced by a separate ffmpeg CRF-0 re-encode after the recording
  // stops, not by this slider (see the recQuality/recBitrateKbps tooltips).
  recBitrateKbps: { label: "valRecBitrateKbps", min: 1000, max: 50000, step: 500, def: 12000, unit: "kbps", gate: "screenRecord" },
  // How young a moov-less (broken) recording has to be to still get a pass
  // (the post-capture menu's stop might just still be finalising) and how often
  // the background sweep for it (and for merely-undecided recordings) runs.
  recBrokenGraceMin: { label: "valRecBrokenGraceMin", min: 1, max: 60, step: 1, def: 5, unit: "min", gate: "recSweepBroken" },
  recSweepEveryMin: { label: "valRecSweepEveryMin", min: 5, max: 1440, step: 5, def: 60, unit: "min", gate: "recSweepBroken" },
  // How long a video editor "Copy to clipboard" export's throwaway temp
  // file survives before the background sweep deletes it (also cleared at
  // the next app start regardless).
  videoClipTtlMin: { label: "valVideoClipTtl", min: 5, max: 240, step: 5, def: 60, unit: "min" },
  // How often the background thread checks installed language packs for an
  // update, how long the "update available" toast stays before it counts as a
  // silent no-op, and how long a click on its Cancel button postpones the next
  // check. All three only matter while ocrLangAutoUpdate itself is on.
  ocrLangCheckH: { label: "valOcrLangCheckH", min: 24, max: 720, step: 24, def: 168, unit: "h", gate: "ocrLangAutoUpdate" },
  ocrLangNoticeMs: { label: "valOcrLangNoticeMs", min: 2000, max: 20000, step: 500, def: 8000, unit: "ms", gate: "ocrLangAutoUpdate" },
  ocrLangSnoozeH: { label: "valOcrLangSnoozeH", min: 24, max: 720, step: 24, def: 168, unit: "h", gate: "ocrLangAutoUpdate" },
  voiceAutoStopSec: { label: "valVoiceAutoStopSec", min: 1, max: 60, step: 1, def: 10, unit: "s", gate: "voiceAutoStop" },
  voiceMaxMin: { label: "valVoiceMaxMin", min: 1, max: 60, step: 1, def: 10, unit: "min", gate: "voice" },
  voiceLangCacheH: { label: "valVoiceLangCacheH", min: 1, max: 168, step: 1, def: 24, unit: "h", gate: "voice" },
  voiceLangCheckH: { label: "valVoiceLangCheckH", min: 24, max: 720, step: 24, def: 168, unit: "h", gate: "voiceLangAutoUpdate" },
  voiceLangNoticeMs: { label: "valVoiceLangNoticeMs", min: 2000, max: 20000, step: 500, def: 8000, unit: "ms", gate: "voiceLangAutoUpdate" },
  voiceLangSnoozeH: { label: "valVoiceLangSnoozeH", min: 24, max: 720, step: 24, def: 168, unit: "h", gate: "voiceLangAutoUpdate" },
  // How long ocr_capabilities()'s own result is cached backend-side, so opening
  // the management dialog (or a manual "Check now") does not always wait on
  // PowerShell. Gated on "ocr" alone, not ocrLangAutoUpdate -- manual "Check now"
  // uses this cache too, even with automatic updates switched off.
  ocrLangCacheH: { label: "valOcrLangCacheH", min: 1, max: 168, step: 1, def: 24, unit: "h", gate: "ocr" },
  // Scrollbar thickness across every dialog (thumb keeps its 3px inset border).
  scrollbarWidth: { label: "valScrollbarWidth", min: 8, max: 20, step: 1, def: 12, unit: "px", gate: "!hideScrollbars" },
  // Frosted-glass dialog background blur (paired with the frostedModals toggle).
  frostedBlur: { label: "valFrostedBlur", min: 0, max: 20, step: 1, def: 4, unit: "px", gate: "frostedModals" },
  // Composer textarea auto-grow cap, popover edge padding, toast width, modal max height.
  composerMaxH: { label: "valComposerMaxH", min: 80, max: 400, step: 10, def: 160, unit: "px" },
  popoverPad: { label: "valPopoverPad", min: 2, max: 24, step: 1, def: 6, unit: "px" },
  toastWidth: { label: "valToastWidth", min: 260, max: 700, step: 20, def: 440, unit: "px" },
  modalMaxVh: { label: "valModalMaxVh", min: 60, max: 95, step: 1, def: 88, unit: "%" },
  // Privacy blur strength (each paired with its "blur until hover" toggle).
  tileBlur: { label: "valTileBlur", min: 1, max: 16, step: 1, def: 4, unit: "px", gate: "blurTilesUntilHover" },
  mediaBlur: { label: "valMediaBlur", min: 1, max: 30, step: 1, def: 10, unit: "px", gate: "blurMediaUntilHover" },
  // Dim strength for the "dim unhovered / dim media" toggles (lower = dimmer).
  tilesHoverOpacity: { label: "valTilesHoverOpacity", min: 10, max: 90, step: 5, def: 45, unit: "%", gate: "tilesHoverDim" },
  mediaHoverOpacity: { label: "valMediaHoverOpacity", min: 10, max: 95, step: 5, def: 80, unit: "%", gate: "mediaHoverDim" },
  // Rounded-media corner radius (paired with the roundMedia toggle).
  mediaRadius: { label: "valMediaRadius", min: 0, max: 24, step: 1, def: 8, unit: "px", gate: "roundMedia" },
  // Prompt version history: versions kept per prompt.
  historyPerPrompt: { label: "valHistoryPerPrompt", min: 5, max: 100, step: 5, def: 20, unit: "", gate: "promptHistory" },
  // Usage statistics: how many rows the top-N lists show.
  statsTopN: { label: "valStatsTopN", min: 5, max: 100, step: 1, def: 10, unit: "", gate: "usageStats" },
  // Duplicate finder: similarity threshold.
  dupThreshold: { label: "valDupThreshold", min: 70, max: 100, step: 1, def: 90, unit: "%", gate: "dupFinder" },
  // How long a confirm button (delete etc.) stays armed before it auto-resets.
  disarmMs: { label: "valDisarmMs", min: 1000, max: 10000, step: 500, def: 3000, unit: "ms" },
  // Automatic backups: how many rotating snapshots to keep.
  backupKeep: { label: "valBackupKeep", min: 1, max: 50, step: 1, def: 10, unit: "", gate: "autoBackup" },
  // How long the auto-backup job waits after launch before its first check --
  // long enough for the window and store to settle, short enough that a
  // backup due at launch does not sit for a full BACKUP_CHECK_SECS tick.
  backupStartDelayS: { label: "valBackupStartDelay", min: 0, max: 300, step: 5, def: 20, unit: "s", gate: "autoBackup" },
  // GFS retention tiers (kept per distinct day / week / month / year).
  backupDaily: { label: "valBackupDaily", min: 0, max: 30, step: 1, def: 3, unit: "", gate: "backupGfs" },
  backupWeekly: { label: "valBackupWeekly", min: 0, max: 52, step: 1, def: 4, unit: "", gate: "backupGfs" },
  backupMonthly: { label: "valBackupMonthly", min: 0, max: 60, step: 1, def: 6, unit: "", gate: "backupGfs" },
  backupYearly: { label: "valBackupYearly", min: 0, max: 20, step: 1, def: 1, unit: "", gate: "backupGfs" },
  // Auto-paste: SetForeground→Ctrl+V delay + double-click window.
  autoPasteDelayMs: { label: "valAutoPasteDelay", min: 30, max: 300, step: 10, def: AUTO_PASTE_DELAY_MS_DEFAULT, unit: "ms", gate: "autoPaste" },
  dblClickMs: { label: "valDblClick", min: 200, max: 600, step: 20, def: DBL_CLICK_MS_DEFAULT, unit: "ms", gate: "autoPaste" },
  // Clipboard watcher: min length + inbox cap.
  clipMinChars: { label: "valClipMin", min: 1, max: 100, step: 1, def: 3, unit: "", gate: "clipWatcher" },
  clipInboxMax: { label: "valClipMax", min: 10, max: 200, step: 10, def: 50, unit: "", gate: "clipWatcher" },
  clipSecretMinLen: { label: "valClipSecretMinLen", min: 4, max: 32, step: 1, def: 8, unit: "", gate: "clipSkipSecrets" },
  // Keyboard grid focus fades this long after the window loses focus.
  kbFocusFadeMs: { label: "valKbFocusFade", min: 500, max: 10000, step: 500, def: 2000, unit: "ms", gate: "keyboardNav" },
  // Tag chip text size (editor, library filter row, manager).
  tagChipPx: { label: "valTagChipPx", min: 10, max: 20, step: 1, def: 12, unit: "px", gate: "tags" },
  // Column count for the grid tag-filter popover.
  tagPopCols: { label: "valTagPopCols", min: 1, max: 3, step: 1, def: 2, unit: "", gate: "tags" },
  // How many custom themes may be stored (keeps the settings blob bounded).
  customThemeMax: { label: "valCustomThemeMax", min: 1, max: 20, step: 1, def: 8, unit: "", gate: "customThemes" },
  // How many views a group popover lists before it starts scrolling.
  viewGroupMaxRows: { label: "valViewGroupMaxRows", min: 4, max: 12, step: 1, def: 8, unit: "", gate: "viewGroups" },
  // How many entries each tray quick-copy section lists.
  trayRecentN: { label: "valTrayRecentN", min: 1, max: 10, step: 1, def: 5, unit: "", gate: "trayRecent" },
  trayFavN: { label: "valTrayFavN", min: 1, max: 10, step: 1, def: 10, unit: "", gate: "trayFavs" },
  // Character cap for a tray menu entry's name before it is cut with an ellipsis.
  // Shared by both sections, so it gates on trayRecent (the first/default one) --
  // no OR-gate exists, and gating a shared row on either lone sub-flag is an
  // approximation either way.
  trayLabelMax: { label: "valTrayLabelMax", min: 20, max: 80, step: 5, def: 40, unit: "", gate: "trayRecent" },
  // How often the Markdown-sync folder is re-scanned for outside edits.
  syncPollSecs: { label: "valSyncPollSecs", min: 2, max: 30, step: 1, def: 4, unit: "s", gate: "promptSync" },
  // Code text size inside the Markdown preview.
  hlFontPx: { label: "valHlFontPx", min: 10, max: 18, step: 1, def: 13, unit: "px", gate: "codeHighlight" },
  // Markdown preview + syntax-highlight size caps, past which they fall back to plain text.
  mdMaxChars: { label: "valMdMaxChars", min: 10000, max: 500000, step: 10000, def: 100000, unit: "", gate: "mdPreview" },
  hlMaxChars: { label: "valHlMaxChars", min: 2000, max: 100000, step: 1000, def: 20000, unit: "", gate: "codeHighlight" },
  // Word-diff cap in version history (500: minimum where a diff is still worth it).
  diffWordCap: { label: "valDiffWordCap", min: 500, max: 20000, step: 500, def: 4000, unit: "", gate: "historyDiff" },
  // Version-history preview line length.
  historyPreviewLen: { label: "valHistoryPreviewLen", min: 20, max: 300, step: 10, def: 80, unit: "", gate: "promptHistory" },
  // How much of a private note the tile tooltip shows (0 = the whole note).
  notesTipLen: { label: "valNotesTipLen", min: 0, max: 2000, step: 20, def: 300, unit: "", gate: "notesInTooltip" },
  // How far the picker's loupe magnifies the pixels around the cursor.
  loupeZoom: { label: "valLoupeZoom", min: 4, max: 16, step: 1, def: 4, unit: "×", gate: "colorPicker" },
  // Loupe grid size (odd numbers only — the centre cell must be the exact picked pixel).
  loupeCells: { label: "valLoupeCells", min: 7, max: 25, step: 2, def: 15, unit: "", gate: "colorPicker" },
  // How many recent picks the right-click popover (and the overlay strip) keep. 0 = off.
  pickHistoryN: { label: "valPickHistoryN", min: 0, max: 20, step: 1, def: 5, unit: "", gate: "colorPicker" },
  // Swatch diameter in the right-click colour-history popover.
  pickSwatchPx: { label: "valPickSwatchPx", min: 12, max: 40, step: 1, def: 22, unit: "px", gate: "colorPicker" },
  // Cap on how many variants one rotating prompt may hold.
  rotateMaxEntries: { label: "valRotateMaxEntries", min: 2, max: 50, step: 1, def: 20, unit: "", gate: "rotatingPrompts" },
  // Tile tooltip preview length for a rotating prompt's short text.
  rotateTipLen: { label: "valRotateTipLen", min: 0, max: 300, step: 10, def: 60, unit: "", gate: "rotatingPrompts" },
  // Auto-advance session tuning — mirrors the backend's auto_cfg() defaults.
  rotateAutoTimeoutMin: { label: "valRotateAutoTimeoutMin", min: 0, max: 480, step: 5, def: 60, unit: "min", zeroLabel: "valUnlimited", gate: "rotateAuto" },
  rotateAutoSettleMs: { label: "valRotateAutoSettleMs", min: 100, max: 2000, step: 50, def: 350, unit: "ms", gate: "rotateAuto" },
  rotateAutoCooldownMs: { label: "valRotateAutoCooldownMs", min: 0, max: 2000, step: 50, def: 250, unit: "ms", gate: "rotateAuto" },
  rotateAutoInputMs: { label: "valRotateAutoInputMs", min: 500, max: 10000, step: 250, def: 3000, unit: "ms", gate: "rotateAuto" },
  rotateAutoRearmMax: { label: "valRotateAutoRearmMax", min: 0, max: 50, step: 1, def: 12, unit: "", gate: "rotateAuto" },
  // A remaining-time estimate only shows once a progress bar has moved this
  // far, and only if what's left is still worth naming a number for.
  etaMinPct: { label: "valEtaMinPct", min: 0, max: 50, step: 1, def: 10, unit: "%", gate: "etaShow" },
  etaMinSecs: { label: "valEtaMinSecs", min: 0, max: 120, step: 5, def: 20, unit: "s", gate: "etaShow" },
  bgDoneKeepMin: { label: "bgDoneKeepMin", min: 1, max: 120, step: 1, def: 10, unit: "min" },
};

// Preset-or-custom numeric settings (dropdown with a free-entry option).
const EXPERT_SELECTS = {
  historyDays: { label: "valHistoryDays", options: [0, 7, 30, 90, 365], def: 0, unit: "d", zeroLabel: "retentionForever", gate: "copyHistory" },
  // Backup interval (hours) — a preset-or-custom select right under backupKeep.
  backupIntervalH: { label: "valBackupInterval", options: [6, 12, 24, 48, 168], def: 24, unit: "h", gate: "autoBackup" },
  // How long a deleted prompt sits in the trash before it is purged automatically.
  // No feature gate — trash is not optional, delete always goes through it.
  trashRetentionDays: { label: "valTrashRetentionDays", options: [0, 7, 30, 90, 365], def: 30, unit: "d", zeroLabel: "retentionForever" },
  // How often the background thread checks for a new release.
  updateCheckH: { label: "valUpdateCheckH", options: [6, 12, 24, 48, 168], def: 24, unit: "h", gate: "showUpdates" },
};

// Date/time width, shared by both style dropdowns. "iso" is the only entry
// that bypasses Intl and always writes the machine format.
const STYLE_OPTS = [["short", "optStyleShort"], ["medium", "optStyleMedium"], ["long", "optStyleLong"], ["iso", "optStyleIso"]];

// Build a simple string-enum dropdown backed by ui_texts (default when unset).
// `opts` is [[value, i18nLabelKey], …]. Used by expert enum params.
function enumDropdown(key, labelKey, opts, def, gate) {
  return {
    label: labelKey,
    gate,
    def, // "only changed" filter compares get() against this
    options: () => opts.map(([v, lk]) => [v, t(lk)]),
    get: () => settings.ui_texts?.[key] || def,
    set: async (v) => {
      settings.ui_texts = { ...(settings.ui_texts || {}), [key]: v };
      try { await invoke("set_ui_text", { key, value: v }); } catch (e) { errToast(e); }
      applyValues();
    },
  };
}

// recQuality's own suggested recBitrateKbps default, applied when the step is
// picked (see recQuality's set() below); documented in docs/help.md.
const REC_QUALITY_BITRATE = { small: 4000, high: 12000 };

// Dropdowns like the settings' text-size / font selects. copySize lives in
// ui_values (0 = auto-fit to the button), copyFont in ui_texts ("" = default).
const EXPERT_DROPDOWNS = {
  // Live counter unit selection.
  counterUnits: enumDropdown("counterUnits", "valCounterUnits",
    [["all", "optAll"], ["chars", "lenChars"], ["words", "lenWords"], ["tokens", "lenTokens"]], "all", "lengthCounter"),
  // Fuzzy-search typo tolerance.
  fuzzyMaxTypos: enumDropdown("fuzzyMaxTypos", "valFuzzyTypos",
    [["auto", "optAuto"], ["1", "opt1"], ["2", "opt2"]], "auto", "fuzzySearch"),
  // How several selected tag filters combine.
  tagFilterMode: enumDropdown("tagFilterMode", "valTagFilterMode",
    [["and", "optTagAnd"], ["or", "optTagOr"]], "and", "tags"),
  // How the self-filling date/time placeholders are written.
  dateStyle: enumDropdown("dateStyle", "valDateStyle", STYLE_OPTS, "short", "dynVars"),
  timeStyle: enumDropdown("timeStyle", "valTimeStyle", STYLE_OPTS, "short", "dynVars"),
  // Which key turns a press into a drag-out instead of the plain drag.
  dragOutModifier: enumDropdown("dragOutModifier", "valDragOutModifier",
    [["ctrl", "keyCtrl"], ["alt", "keyAlt"], ["shift", "keyShift"]], "ctrl", "dragOut"),
  // Mode a NEW rotating prompt starts with (each prompt can still switch its
  // own mode afterward in the editor).
  rotateDefaultMode: enumDropdown("rotateDefaultMode", "valRotateDefaultMode",
    [["order", "optRotateOrder"], ["random", "optRotateRandom"]], "order", "rotatingPrompts"),
  // The notation the picked colour is copied in.
  pickerFormat: enumDropdown("pickerFormat", "valPickerFormat",
    [["hex", "optFmtHex"], ["rgb", "optFmtRgb"], ["hsl", "optFmtHsl"]], "hex", "colorPicker"),
  // Which tool the screenshot editor opens with.
  snipEditDefaultTool: enumDropdown("snipEditDefaultTool", "valSnipEditDefaultTool",
    [["hand", "snipToolHand"], ["crop", "snipToolCrop"], ["pen", "snipToolPen"], ["marker", "snipToolMarker"],
     ["arrow", "snipToolArrow"], ["text", "snipToolText"], ["step", "snipToolStep"]], "hand", "snipEditor"),
  // Resampling quality for the editor's own scale action: smooth for photos,
  // nearest-neighbour ("low") to keep pixel art crisp instead of blurred.
  snipScaleFilter: enumDropdown("snipScaleFilter", "valSnipScaleFilter",
    [["high", "optSnipScaleFilterHigh"], ["low", "optSnipScaleFilterLow"]], "high", "snipEditor"),
  // Options are fetched once from ocr_languages() at startup and cached
  // (ocrLangsCache, near the OCR wiring below) -- "Automatic" is always first,
  // regardless of what the OS itself reports.
  ocrLang: {
    label: "valOcrLang",
    gate: "ocr",
    def: "auto",
    // Trailing "Manage languages..." entry opens #ocr-langs-modal instead of
    // ever being persisted as the chosen language -- see set() below.
    options: () => [["auto", t("langAuto")], ...ocrLangsCache.map((l) => [l.tag, l.label]), ["__manage__", t("ocrLangsManage")]],
    get: () => settings.ui_texts?.ocrLang || "auto",
    set: async (v) => {
      if (v === "__manage__") { openOcrLangsModal(); return; }
      settings.ui_texts = { ...(settings.ui_texts || {}), ocrLang: v };
      try { await invoke("set_ui_text", { key: "ocrLang", value: v }); } catch (e) { errToast(e); }
    },
  },
  // Which microphone to capture for recAudioMic. Options are fetched once
  // from list_mic_devices() at startup (micDevicesCache, near the recording
  // wiring below); empty string = the system default capture device.
  recMicDevice: {
    label: "valRecMicDevice",
    gate: "recAudioMic",
    def: "",
    options: () => [["", t("langAuto")], ...micDevicesCache.map((d) => [d.id, d.name])],
    get: () => settings.ui_texts?.recMicDevice || "",
    set: async (v) => {
      settings.ui_texts = { ...(settings.ui_texts || {}), recMicDevice: v };
      try { await invoke("set_ui_text", { key: "recMicDevice", value: v }); } catch (e) { errToast(e); }
    },
  },
  // Three quality steps, default "high". Picking a step re-suggests the
  // matching recBitrateKbps default (REC_QUALITY_BITRATE) so the two controls
  // stay in sync without forcing a second manual edit -- the slider still
  // accepts any later override. "Lossless" leaves the slider's value alone
  // since it has no effect on that step (a separate ffmpeg CRF-0 pass produces
  // it after the recording stops).
  recQuality: {
    label: "valRecQuality",
    gate: "screenRecord",
    def: "high",
    options: () => [
      ["small", t("optRecQualitySmall")], ["high", t("optRecQualityHigh")], ["lossless", t("optRecQualityLossless")],
    ],
    get: () => settings.ui_texts?.recQuality || "high",
    set: async (v) => {
      settings.ui_texts = { ...(settings.ui_texts || {}), recQuality: v };
      try { await invoke("set_ui_text", { key: "recQuality", value: v }); } catch (e) { errToast(e); }
      if (REC_QUALITY_BITRATE[v] != null) {
        settings.ui_values = { ...(settings.ui_values || {}), recBitrateKbps: REC_QUALITY_BITRATE[v] };
        try { await invoke("set_ui_value", { key: "recBitrateKbps", value: REC_QUALITY_BITRATE[v] }); } catch (e) { errToast(e); }
      }
      applyValues();
    },
  },
  // MP4 is always what the recorder itself writes; webm/gif are produced by
  // an ffmpeg conversion pass once the recording stops (see convert_recording
  // on the backend and the post-capture menu that calls it).
  recFormat: enumDropdown("recFormat", "valRecFormat",
    [["mp4", "optRecFormatMp4"], ["webm", "optRecFormatWebm"], ["gif", "optRecFormatGif"],
      ["avi", "optRecFormatAvi"], ["mov", "optRecFormatMov"], ["mkv", "optRecFormatMkv"],
      ["webpanim", "optRecFormatWebpanim"], ["apng", "optRecFormatApng"]], "mp4", "screenRecord"),
  // The video editor's export dialog opens with these eight every time --
  // each field stays freely changeable per export, only the opening
  // selection comes from here.
  veDefaultFormat: enumDropdown("veDefaultFormat", "valVeDefaultFormat",
    [["mp4", "optRecFormatMp4"], ["webm", "optRecFormatWebm"], ["gif", "optRecFormatGif"],
      ["avi", "optRecFormatAvi"], ["mov", "optRecFormatMov"], ["mkv", "optRecFormatMkv"],
      ["webpanim", "optRecFormatWebpanim"], ["apng", "optRecFormatApng"]], "mp4", "videoEditor"),
  veDefaultQuality: enumDropdown("veDefaultQuality", "valVeDefaultQuality",
    [["high", "veQualityHigh"], ["small", "veQualitySmall"], ["lossless", "veQualityLossless"]], "high", "videoEditor"),
  veDefaultScale: enumDropdown("veDefaultScale", "valVeDefaultScale",
    [["", "veScaleOff"], ["2160", "optVeScale2160"], ["1440", "optVeScale1440"], ["1080", "optVeScale1080"],
      ["720", "optVeScale720"], ["480", "optVeScale480"], ["360", "optVeScale360"],
      ["50", "optVeScale50pct"], ["25", "optVeScale25pct"]], "", "videoEditor"),
  // Applies to every screenshot: the snip tool's own save/save-as/save-to-folder
  // paths AND the video editor's "export still frame" (same setting, one format
  // for anything this app calls a screenshot). webp and bmp/tiff are always
  // lossless (see encode_screenshot in lib.rs); only jpeg has a quality of its
  // own, which is why snipQuality/snipSaveQuality below only show for jpeg.
  screenshotFormat: enumDropdown("screenshotFormat", "valScreenshotFormat",
    [["png", "optScreenshotFormatPng"], ["jpeg", "optScreenshotFormatJpeg"], ["webp", "optScreenshotFormatWebp"],
      ["bmp", "optScreenshotFormatBmp"], ["tiff", "optScreenshotFormatTiff"]], "png", "screenshot"),
  // Same pattern as ocrLang just above: fetched once from voice_languages()
  // at startup (voiceLangsCache), "Automatic" always first, a trailing entry
  // opens the shared language-pack dialog instead of ever being stored.
  voiceLang: {
    label: "valVoiceLang",
    gate: "voice",
    def: "auto",
    options: () => [["auto", t("langAuto")], ...voiceLangsCache.map((l) => [l.tag, l.label]), ["__manage__", t("voiceLangsManage")]],
    get: () => settings.ui_texts?.voiceLang || "auto",
    set: async (v) => {
      if (v === "__manage__") { openVoiceLangsModal(); return; }
      settings.ui_texts = { ...(settings.ui_texts || {}), voiceLang: v };
      try { await invoke("set_ui_text", { key: "voiceLang", value: v }); } catch (e) { errToast(e); }
    },
  },
  voiceInsert: enumDropdown("voiceInsert", "valVoiceInsert",
    [["caret", "voiceInsertCaret"], ["end", "voiceInsertEnd"]], "caret", "voice"),
  voiceHotkeyTarget: enumDropdown("voiceHotkeyTarget", "valVoiceHotkeyTarget",
    [["clipboard", "voiceHotkeyTargetClip"], ["paste", "voiceHotkeyTargetPaste"], ["input", "voiceHotkeyTargetInput"]], "clipboard", "voiceHotkey"),
  // Which edge the toast notifications rise from.
  toastPos: enumDropdown("toastPos", "valToastPos",
    [["bottom", "optToastBottom"], ["top", "optToastTop"]], "bottom"),
  // Tag manager / filter chip order.
  tagSort: enumDropdown("tagSort", "valTagSort",
    [["alpha", "optTagSortAlpha"], ["usage", "optTagSortUsage"]], "alpha", "tags"),
  copySize: {
    label: "valCopySize",
    // Only styles the "Copied!" bubble text -- pointless while that bubble is off.
    gate: "copyBubble",
    // get() always returns a String (see below) — a numeric def here made
    // isRowChanged's strict !== comparison permanently true ("0" !== 0).
    def: "0",
    options: () => {
      const o = [["0", t("langAuto")]];
      for (let s = SIZE_MIN; s <= SIZE_MAX; s += SIZE_STEP) o.push([String(s), String(s)]);
      return o;
    },
    get: () => String(Number(settings.ui_values?.copySize) || 0),
    set: async (v) => {
      settings.ui_values = { ...(settings.ui_values || {}), copySize: Number(v) };
      try { await invoke("set_ui_value", { key: "copySize", value: Number(v) }); } catch (e) { errToast(e); }
    },
  },
  copyFont: {
    label: "valCopyFont",
    gate: "copyBubble",
    def: "",
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
      try { await invoke("set_ui_text", { key: "copyFont", value: v }); } catch (e) { errToast(e); }
    },
  },
};

// Expert menu organised into tabs. The shape is deliberate: the first tab holds
// FEATURES — a toggle followed by everything that tunes it, so a switched-off
// feature costs exactly one line — while the remaining tabs hold the general
// values that belong to no single feature.
const EXPERT_TABS = [
  { title: "expTabFeatures", groups: [
    // Everything that puts new content into the app.
    { title: "expGroupInput", flags: ["fileAttach", { key: "storeFiles", gate: "fileAttach" }, { key: "cleanupFiles", gate: "storeFiles" }, "pasteMedia", "dragDrop"] },
    { title: "expGroupViews", flags: ["multiView", { key: "viewGroups", gate: "multiView" }, { key: "multiViewPrompt", gate: "multiView" }, "quickGrid", { key: "quickGridHoverOnly", gate: "quickGrid" }], values: ["viewBorder", "viewGroupMaxRows"] },
    // Placeholders, from the grammar to where the app asks for them.
    { title: "expGroupVars", flags: ["promptVars", { key: "varDefaults", gate: "promptVars" }, { key: "dynVars", gate: "promptVars" }, { key: "choiceVars", gate: "promptVars" }, { key: "choiceRemember", gate: "choiceVars" }, { key: "varPopupWindow", gate: "promptVars" }], dropdowns: ["dateStyle", "timeStyle"] },
    // The editor: what it shows beside the text box. Preview and highlighter belong together — the
    // highlighter only ever paints inside the preview.
    { title: "expGroupEditor", flags: ["mdPreview", { key: "codeHighlight", gate: "mdPreview" }, { key: "codeAutoDetect", gate: "mdPreview" }, "lengthCounter", "promptHistory", { key: "historyDiff", gate: "promptHistory" }, "promptNotes", { key: "notesInTooltip", gate: "promptNotes" }, "editorMonospace", "confirmDiscard"], values: ["hlFontPx", "historyPerPrompt", "notesTipLen", "mdMaxChars", "hlMaxChars", "diffWordCap", "historyPreviewLen"], dropdowns: ["counterUnits"] },
    // Everything that happens between a click and the clipboard.
    { title: "expGroupCopy", flags: ["chainPrompts", { key: "chainLock", gate: "chainPrompts" }, { key: "chainQuickStart", gate: "chainPrompts" }, { key: "chainStartAlwaysVisible", gate: "chainQuickStart" }, "promptHotkeys", { key: "promptHotkeyPaste", gate: "promptHotkeys" }, "dragOut", { key: "dragOutInvert", gate: "dragOut" }, { key: "autoPasteEnter", gate: "autoPaste" }, "hideWindowOnCopy", "copyBubble", "copyFlash", "copySound"], values: ["copyCooldownMs", "autoPasteDelayMs", "dblClickMs", "bubbleMs", "copySoundVolume"], dropdowns: ["dragOutModifier", "copySize", "copyFont"] },
    // Floating buttons and the two kinds of folder they can form.
    { title: "expGroupFolders", flags: ["floating", { key: "pillFolders", gate: "floating" }, "tileFolders"], values: ["floatOpacity", "pillFontSize", "pillTextMinW", "pillBorderW", "pillRingFrac", "pillMenuGap", "pillMergeRadius", "pillMergeGrow", "pillFolderTileSize", "pillFolderTileGap", "pillFolderPad", "pillFolderPopGap", "folderGridCols", "tileFolderZone", "tileFolderHoverMs", "tileFolderCols"] },
    // The quick-copy sections of the tray menu, each with its own entry count.
    { title: "expGroupTray", flags: ["trayCustomMenu", "trayRecent", "trayFavs"], values: ["trayRecentN", "trayFavN", "trayLabelMax"] },
    // The watcher that collects what you copy elsewhere.
    { title: "expGroupClipboard", flags: ["clipWatcher", { key: "clipSkipSecrets", gate: "clipWatcher" }], values: ["clipMinChars", "clipInboxMax", "clipSecretMinLen"] },
    // Prompt folders as Markdown (beta): mirrors text-only prompts as .md files.
    { title: "expGroupSync", flags: ["promptSync"], values: ["syncPollSecs"], paths: ["syncDir"] },
    // Auto-advancing through a rotation -- not a capture tool, stays in tab 1.
    { title: "expGroupRotation", flags: ["rotatingPrompts", { key: "rotateBadge", gate: "rotatingPrompts" }, { key: "rotateAuto", gate: "rotatingPrompts" }], values: ["rotateMaxEntries", "rotateTipLen", "rotateAutoTimeoutMin", "rotateAutoSettleMs", "rotateAutoCooldownMs", "rotateAutoInputMs", "rotateAutoRearmMax"], dropdowns: ["rotateDefaultMode"] },
    // Voice dictation -- not a capture tool either (it puts text into the composer,
    // the same way typing does), stays in tab 1.
    { title: "expGroupVoice", flags: ["voice", { key: "voiceInterim", gate: "voice" }, { key: "voicePunct", gate: "voice" }, { key: "voiceAutoCaps", gate: "voicePunct" }, { key: "voiceAutoStop", gate: "voice" }, { key: "voiceOnlineOk", gate: "voice" }, { key: "voiceAutoPickLang", gate: "voice" }, { key: "voiceLangAutoUpdate", gate: "voice" }, { key: "voiceHotkey", gate: "voice" }], values: ["voiceAutoStopSec", "voiceMaxMin", "voiceLangCacheH", "voiceLangCheckH", "voiceLangNoticeMs", "voiceLangSnoozeH"], dropdowns: ["voiceLang", "voiceInsert", "voiceHotkeyTarget"],
      actions: [{ id: "voice-langs-open", label: "voiceLangsManage", gate: "voice" }, { id: "voice-hotkey-set", label: "voiceHotkeySet", gate: "voiceHotkey" }] },
  ] },
  { title: "expTabCapture", groups: [
    // Everything below is a capture tool: it reads something off the screen or the
    // microphone rather than putting typed content into the app.
    { title: "expGroupScreenshot", flags: ["screenshot"], values: ["snipPreviewVh", "snipQuality", "snipSaveQuality", "snipDelayMs"], dropdowns: ["screenshotFormat"], paths: ["screenshotDir"] },
    { title: "expGroupSnipEditor", flags: [{ key: "snipEditor", gate: "screenshot" }, { key: "snipEditSnap", gate: "snipEditor" }, { key: "snipTextPlate", gate: "snipEditor" }], values: ["snipPenPx", "snipMarkerPx", "snipTextPx", "snipPixelBlock", "snipSpotlightDim", "snipEditSnapTolerancePx", "snipEditUndoMax"], dropdowns: ["snipEditDefaultTool", "snipScaleFilter"] },
    // Opens #ocr-langs-modal -- same gate as the rest of the feature.
    { title: "expGroupOcr", flags: ["ocr", { key: "ocrAutoCopy", gate: "ocr" }, { key: "ocrJoinLines", gate: "ocr" }, { key: "ocrAutoPickLang", gate: "ocr" }, { key: "ocrLangAutoUpdate", gate: "ocr" }], values: ["ocrUpscaleUnderPx", "ocrLangCacheH", "ocrLangCheckH", "ocrLangNoticeMs", "ocrLangSnoozeH"], dropdowns: ["ocrLang"],
      actions: [{ id: "ocr-langs-open", label: "ocrLangsManage", gate: "ocr" }] },
    { title: "expGroupPicker", flags: ["colorPicker"], values: ["loupeZoom", "loupeCells", "pickHistoryN", "pickSwatchPx"], dropdowns: ["pickerFormat"] },
    // The recording hotkeys render their own capture row directly (hotkeyRow())
    // instead of a button that jumps to a settings field -- see actionRow()'s
    // "hotkey" branch.
    { title: "expGroupRecording", flags: ["screenRecord", { key: "recCursor", gate: "screenRecord" }, { key: "recAudioSystem", gate: "screenRecord" }, { key: "recAudioMic", gate: "screenRecord" }, { key: "recStopHotkey", gate: "screenRecord" }, { key: "recPauseHotkey", gate: "screenRecord" }, { key: "recTrayEntries", gate: "screenRecord" }, { key: "recCropNotice", gate: "screenRecord" }, { key: "recBarSize", gate: "screenRecord" }, { key: "recTargetSwitch", gate: "screenRecord" }], values: ["recCountdownS", "recFps", "recAudioKbps", "recBitrateKbps"], dropdowns: ["recMicDevice", "recQuality", "recFormat"], paths: ["recordingDir"],
      actions: [{ id: "rec-stop-hotkey-row", type: "hotkey", hotkeyKey: "recStopHotkey", label: "recStopHotkeyLabel", gate: "recStopHotkey" }, { id: "rec-pause-hotkey-row", type: "hotkey", hotkeyKey: "recPauseHotkey", label: "recPauseHotkeyLabel", gate: "recPauseHotkey" }] },
    { title: "expGroupVideoEditor", flags: ["videoEditor"], values: ["videoEditUndoMax", "veExportKbpsHigh", "veExportKbpsSmall", "veFadeMs", "veOvPixelateBlock", "veOvSpotlightDim", "veOvImageOpacity"], dropdowns: ["veDefaultFormat", "veDefaultQuality", "veDefaultScale"] },
  ] },
  { title: "expTabTiles", groups: [
    // Split by what a setting actually does: acting on a tile, the hover tooltip,
    // and pure looks. One 26-entry blob made everything hard to find.
    { title: "expGroupTiles", flags: ["tileMenu", "tileReorder", "tileHover", { key: "tileHoverLift", gate: "tileHover" }, "tilePressScale"], values: ["hoverLift", "dragThreshold"] },
    { title: "expGroupTooltips", flags: ["tilePreview", "iconTooltips", "tooltipDelay", "tooltipTimeout", "tileTimeTip"], values: ["tooltipWidth", "tooltipDelayMs", "tooltipTimeoutMs"] },
    { title: "expGroupTileLook", flags: ["tileShadow", "typeBadges", "captions", "compactTiles", "tileGradient", "gridLines", "uppercaseTiles", "boldTileNames", "monospaceTiles", "italicTiles", "tileTextShadow"], values: ["tileRadius", "tileBorderWidth"] },
    { title: "expGroupFavorites", flags: ["favorites", { key: "favViewButton", gate: "favorites" }, { key: "favViewReorder", gate: "favViewButton" }], values: ["favStarSize", "favMaxCols"] },
  ] },
  { title: "expTabAppearance", groups: [
    { title: "expGroupScales", values: ["uiScale", "modalScale", "composerScale", "iconScale", "primaryScale"] },
    { title: "expGroupAppearanceDetails", flags: ["animations", "accentHeader", "hideScrollbars", { key: "accentScrollbar", gate: "!hideScrollbars" }, "smoothScroll", "frostedModals"], values: ["animSpeed", "gridGap", "gridPad", "headerPadY", "modalRadius", "overlayDim", "scrollbarWidth", "frostedBlur", "composerMaxH", "popoverPad", "toastWidth", "toastMs", "modalMaxVh"], dropdowns: ["toastPos"] },
    { title: "expGroupColors", flags: ["customThemes"], values: ["customThemeMax"], palette: true },
    { title: "expGroupLimits", values: ["maxViews", "gridMax", "previewLen", "nameMaxLen"] },
  ] },
  { title: "expTabLibrary", groups: [
    { title: "expGroupLibrary", flags: ["librarySearch", "searchSuggest", "fuzzySearch", "smartSort", "batchOps", "libraryTypeFilter", "colorFilter", "searchAutofocus", "imagePreview", "libraryVideoPreview", "closeAfterCopy", "libraryCloseToggle"], values: ["searchRecentMax", "libraryCols", "libraryWidth", "libPreviewLines", "searchDebounceMs"], dropdowns: ["fuzzyMaxTypos"] },
    { title: "expGroupTags", flags: ["tags"], values: ["tagChipPx", "tagPopCols"], dropdowns: ["tagFilterMode", "tagSort"] },
    // Everything that records or shows copy history lives with the history view.
    { title: "expGroupJournal", flags: ["copyHistory", "historyTimestamps", "historyGrouped", "dedupCopyLog", "journalSearch", "journalTypeFilter", "journalColorFilter", { key: "journalTagFilter", gate: "tags" }, "journalRecent", "journalMostUsed", "journalGroupBtn", "journalRecentSort", "journalUsedSort", "journalClear", "confirmClearHistory"], values: ["historyMax", "journalLimit", "journalWidth"], selects: ["historyDays"] },
  ] },
  { title: "expTabMedia", groups: [
    { title: "expGroupMedia", flags: ["videoControls", "videoAutoplay", "videoMuted", "videoLoop", "grayscaleMedia", "mediaHoverDim", "mediaBorder", "roundMedia"], values: ["videoVolume", "mediaHoverOpacity", "mediaRadius"] },
  ] },
  { title: "expTabSystem", groups: [
    // Toolbar button visibility (mirrors the header right-click tool list).
    { title: "expGroupToolbar", flags: ["gridViewBorder", "pinButton", "barToggles", { key: "barToggleBoth", gate: "barToggles" }, "showLibrary", "showJournal", "showLogo", "showTitle", "pausedIcons", "headerSeparators", "toolbarHoverDim"] },
    { title: "expGroupPrivacy", notices: ["storeUnsealed", "settingsLocked", "settingsWriteFailed"], flags: ["allowScreenshot", "blurTilesUntilHover", "blurMediaUntilHover", "hideTileNames", "tilesHoverDim"], values: ["tileBlur", "mediaBlur", "tilesHoverOpacity"] },
    { title: "expGroupSystem", flags: ["showUpdates", { key: "checkUpdateOnStart", gate: "showUpdates" }, "keyboardNav", "allowMultipleInstances", "taskbarProgress", "etaShow"], values: ["kbFocusFadeMs", "etaMinPct", "etaMinSecs", "bgDoneKeepMin"], selects: ["updateCheckH"], paths: ["dataDir"],
      actions: [{ id: "ffmpeg-download", label: "ffmpegDownloadAction" }] },
    { title: "expGroupHelp", flags: ["helpTips", "helpHighlight", "firstRunTour"],
      actions: [{ id: "tour-open", label: "tourOpenAction", gate: "firstRunTour" }] },
    { title: "expGroupData", flags: ["dupFinder", { key: "dupImportCheck", gate: "dupFinder" }, "backupBeforeReset", "backupBeforeDelete", "importExport", "recSweepBroken"], values: ["dupThreshold", "disarmMs", "recBrokenGraceMin", "recSweepEveryMin", "videoClipTtlMin"], selects: ["trashRetentionDays"],
      actions: [{ id: "dupes-open", label: "dupTitle", gate: "dupFinder" }, { id: "trash-open", label: "trashTitle" }, { id: "rec-sweep-now", label: "recSweepNowAction", gate: "recSweepBroken" }] },
  ] },
];

// Sub-toggle -> the feature it belongs to, collected from the group lists above so a
// gate is declared in exactly one place. gateOpen() walks it, which is what makes a
// whole branch (feature -> sub-toggle -> its sliders) vanish together.
const FLAG_GATES = Object.fromEntries(
  EXPERT_TABS.flatMap((tab) => tab.groups.flatMap(groupFlags))
    .filter((f) => f.gate)
    .map((f) => [f.key, f.gate])
);

const flag = (key) => (settings.ui_flags?.[key] ?? shipsOn(key)) !== false;
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

// Wraps a command that reports no real progress of its own (backup, import)
// with an indeterminate taskbar entry -- "something is happening" is all that
// can honestly be shown without a percentage to work from.
async function withTaskbarBusy(op, fn) {
  invoke("set_taskbar_progress", { op, state: "indeterminate" }).catch(() => {});
  try {
    return await fn();
  } finally {
    invoke("set_taskbar_progress", { op, state: "done" }).catch(() => {});
  }
}

// Mirror each disabled flag onto a body class so CSS can hide the matching UI.
function applyFlags() {
  for (const key of ALL_FLAG_KEYS) {
    document.body.classList.toggle(`noflag-${key}`, !flag(key));
  }
  // Opt-in (default-off) flags: a `flag-<key>` class appears only when enabled.
  for (const key of OPT_FLAG_KEYS) {
    document.body.classList.toggle(`flag-${key}`, optFlag(key));
  }
  // Not a setting -- a runtime fact from voice_support(). A Windows build
  // with no speech engine at all must hide the button regardless of the flag.
  document.body.classList.toggle("no-voice-engine", !voiceSupport.supported);
  refreshCustomThemes(); // The feature may have just been switched on or off
  // uppercaseTiles/boldTileNames/monospaceTiles/italicTiles change .tile-name
  // metrics via CSS but are plain flags (no dedicated apply* path), so a cached fit
  // size from before the toggle would otherwise linger until fitCache's own 1000-entry eviction.
  fitCache.clear();
  if (chainMode && !flag("chainPrompts")) setChainMode(false); // killed in expert menu
  if (chainLock && !flag("chainLock")) { chainLock = false; $("chain-btn")?.classList.remove("chain-locked"); }
  applyValues();
  // An open library must follow a flag flip (the tag row appears/disappears).
  if (prompts.length && !$("library")?.classList.contains("hidden")) renderLibrary();
  refreshClipInbox(); // Reflect the watcher toggle + badge count
  applyBarToggleTips();
}

// Apply every numeric tweak to its live target (CSS vars + JS constants).
let _lastUiZoom = null; // last native-zoom factor pushed to the backend (throttle)
function applyValues() {
  const root = document.documentElement.style;
  root.setProperty("--transition", `${val("animSpeed")}ms cubic-bezier(0.4, 0, 0.2, 1)`);
  root.setProperty("--transition-ms", `${val("animSpeed")}ms`); // bare duration for effects that scale it (e.g. a slower fade)
  root.setProperty("--gap", `${val("gridGap")}px`);
  root.setProperty("--lib-preview-lines", String(Math.round(val("libPreviewLines"))));
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
  root.setProperty("--dim-unhovered", val("tilesHoverOpacity") / 100);
  root.setProperty("--dim-media", val("mediaHoverOpacity") / 100);
  root.setProperty("--media-radius", `${val("mediaRadius")}px`);
  root.setProperty("--tag-chip-px", `${val("tagChipPx")}px`);
  root.setProperty("--hl-font-px", `${val("hlFontPx")}px`);
  root.setProperty("--view-group-rows", String(val("viewGroupMaxRows")));
  root.setProperty("--pick-swatch", `${val("pickSwatchPx")}px`);
  root.setProperty("--composer-max-h", `${val("composerMaxH")}px`);
  root.setProperty("--toast-width", `${val("toastWidth")}px`);
  root.setProperty("--modal-max-vh", `${val("modalMaxVh")}vh`);
  root.setProperty("--tag-pop-cols", String(Math.round(val("tagPopCols"))));
  root.setProperty("--journal-width", `${val("journalWidth")}px`);
  // Caps/lengths that were fixed constants, now expert-tunable.
  ROTATE_SHORT_MAX = Math.round(val("rotateTipLen"));
  MD_MAX_CHARS = Math.round(val("mdMaxChars"));
  HL_MAX_CHARS = Math.round(val("hlMaxChars"));
  DIFF_WORD_CAP = Math.round(val("diffWordCap"));
  HISTORY_PREVIEW_LEN = Math.round(val("historyPreviewLen"));
  DRAG_THRESHOLD = val("dragThreshold");
  INPUT_MAX = Math.round(val("composerMaxH"));
  setPopoverPad(Math.round(val("popoverPad")));
  DISARM_MS = Math.round(val("disarmMs"));
  document.body.classList.toggle("toast-top", settings.ui_texts?.toastPos === "top");
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
  "storeFiles", "varDefaults", "dynVars", "choiceVars", "choiceRemember", "varPopupWindow",
  "lengthCounter", "promptHistory", "historyDiff", "diffWordCap", "historyPreviewLen", "editorMonospace",
  "promptNotes", "notesInTooltip", "notesTipLen",
  "mdPreview", "mdMaxChars", "codeHighlight", "codeAutoDetect", "hlFontPx", "hlMaxChars",
  "colorPicker", "pickerFormat", "loupeZoom", "loupeCells", "pickHistoryN", "pickSwatchPx",
  "screenRecord", "recCursor", "recCountdownS", "recFps", "recAudioSystem", "recAudioMic", "recAudioKbps", "recMicDevice",
  "recQuality", "recBitrateKbps", "recFormat", "recordingDir",
  "recStopHotkey", "recPauseHotkey", "recTrayEntries", "recCropNotice", "recBarSize", "recTargetSwitch",
  "videoEditor", "videoEditUndoMax",
  "veDefaultFormat", "veDefaultQuality", "veDefaultScale", "veExportKbpsHigh", "veExportKbpsSmall",
  "veFadeMs", "veOvPixelateBlock", "veOvSpotlightDim", "veOvImageOpacity",
  "snipQuality", "snipSaveQuality", "snipDelayMs",
  "snipPenPx", "snipMarkerPx", "snipTextPx", "snipPixelBlock", "snipSpotlightDim",
  "snipEditUndoMax", "snipEditSnap", "snipEditSnapTolerancePx", "snipTextPlate", "snipEditDefaultTool", "snipScaleFilter",
  "ocr", "ocrUpscaleUnderPx", "ocrLang",
  "ocrAutoPickLang", "ocrLangAutoUpdate", "ocrLangCheckH", "ocrLangNoticeMs", "ocrLangSnoozeH", "ocrLangCacheH",
  "voice", "voiceInterim", "voicePunct", "voiceAutoCaps", "voiceAutoStop", "voiceAutoStopSec", "voiceMaxMin",
  "voiceLangCacheH", "voiceLang", "voiceInsert", "voiceOnlineOk", "voiceAutoPickLang", "voiceLangAutoUpdate",
  "voiceLangCheckH", "voiceLangNoticeMs", "voiceLangSnoozeH", "voiceHotkey", "voiceHotkeyTarget",
  "trayCustomMenu", "trayRecent", "trayFavs", "trayRecentN", "trayFavN", "trayLabelMax",
  "checkUpdateOnStart", "showUpdates", "updateCheckH", "importExport", "toolbarHoverDim", "keyboardNav", "allowMultipleInstances",
  "taskbarProgress", "etaShow",
  "headerSeparators", "floating",
  "pillFolders", "pillFontSize", "pillTextMinW", "pillBorderW", "pillRingFrac", "pillMenuGap",
  "pillMergeRadius", "pillMergeGrow", "pillFolderTileSize", "pillFolderTileGap", "pillFolderPad", "pillFolderPopGap",
  "folderGridCols",
  "tileFolders", "tileFolderZone", "tileFolderHoverMs", "tileFolderCols",
  "rotatingPrompts", "rotateDefaultMode", "rotateMaxEntries", "rotateBadge", "rotateTipLen",
  "rotateAuto", "rotateAutoTimeoutMin", "rotateAutoSettleMs", "rotateAutoCooldownMs", "rotateAutoInputMs", "rotateAutoRearmMax",
  "helpTips", "helpHighlight", "firstRunTour",
  // Tiles / copying
  "tilePreview", "tileMenu", "typeBadges", "copyBubble", "copyFlash", "tilePressScale", "chainPrompts", "chainLock", "chainQuickStart", "chainStartAlwaysVisible", "autoPasteEnter", "hideWindowOnCopy",
  "promptHotkeys", "promptHotkeyPaste", "dragOut", "dragOutInvert", "dragOutModifier",
  "tooltipDelay", "tooltipTimeout", "iconTooltips",
  // Appearance
  "hideScrollbars", "gridLines", "pausedIcons",
  "customThemes", "customThemeMax",
  "dragThreshold", "composerMaxH", "popoverPad", "toastWidth", "toastPos", "modalMaxVh",
  // Library / privacy
  "smartSort", "fuzzySearch", "batchOps", "searchSuggest", "searchAutofocus", "libraryCloseToggle", "searchDebounceMs",
  "tags", "tagChipPx", "tagFilterMode", "tagPopCols", "tagSort",
  "allowScreenshot", "copyHistory", "historyTimestamps", "historyGrouped", "journalWidth",
  "dedupCopyLog", "cleanupFiles", "blurTilesUntilHover",
  // Tools / data
  "dupFinder", "clipWatcher", "backupGfs", "backupBeforeReset", "backupBeforeDelete", "disarmMs",
  "recSweepBroken", "recBrokenGraceMin", "recSweepEveryMin", "videoClipTtlMin",
  "usageStats", "promptSync", "syncDir", "syncPollSecs", "clipSkipSecrets",
  // Values that need a unit/behaviour explanation
  "copyCooldownMs", "dblClickMs", "autoPasteDelayMs", "kbFocusFadeMs", "snipPreviewVh",
  "historyPerPrompt", "dupThreshold", "clipMinChars", "clipInboxMax", "clipSecretMinLen", "historyMax",
  "journalLimit", "previewLen", "nameMaxLen", "searchRecentMax", "maxViews", "gridMax",
  "viewBorder", "statsTopN", "viewGroups", "viewGroupMaxRows", "multiViewPrompt",
  "barToggles", "barToggleBoth", "quickGrid", "quickGridHoverOnly", "multiView", "journalGroupBtn", "journalRecentSort", "journalUsedSort", "libraryCols", "copySize", "copyFont", "showLibrary", "showJournal", "composerScale", "primaryScale",
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

// A small per-row undo icon, visible only while this exact row differs from its
// default — resets just this one key. Reuses the same isRowChanged() diff as 66/67
// and the same per-key commands resetExpertGroup() uses; no confirmation, since one
// click back is reversible.
// `after` lets a caller outside the main expert menu (the backup/statistics
// panels, their own top-level window) redraw itself instead of renderExpert().
// Always built (never null) and shown/hidden through a class, so a value that
// changes without a full menu rebuild (a dragged slider, a flipped switch)
// still gets its reset button live -- syncRowReset() below is what flips it.
function rowResetBtn(kind, key, after) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "expert-row-reset";
  btn.dataset.expertKind = kind;
  btn.dataset.expertKey = key;
  btn.classList.toggle("hidden", !isRowChanged(kind, key));
  const label = t("expertResetOne");
  btn.title = label;
  btn.setAttribute("aria-label", label);
  putIcon(btn, ICON_UNDO);
  btn.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    resetOneExpertValue(kind, key, after);
  });
  return btn;
}
// Re-checks one row's changed-from-default state and flips its already-built
// reset button in place -- no menu rebuild, so scroll position/focus survive.
// A no-op if that row isn't on screen right now (row list is filtered, or a
// renderExpert() elsewhere already replaced it), which is fine either way.
function syncRowReset(kind, key) {
  const btn = document.querySelector(`.expert-row-reset[data-expert-kind="${kind}"][data-expert-key="${key}"]`);
  if (btn) btn.classList.toggle("hidden", !isRowChanged(kind, key));
}
async function resetOneExpertValue(kind, key, after) {
  if (kind === "flags") {
    const enabled = shipsOn(key);
    settings.ui_flags = { ...(settings.ui_flags || {}), [key]: enabled };
    try { await invoke("set_ui_flag", { key, enabled }); } catch (err) { errToast(err); }
    applyFlags();
    if (TILE_DOM_FLAGS.has(key)) await renderGrid(true); else fitAllTiles();
  } else if (kind === "values" || kind === "selects") {
    const cfg = EXPERT_VALUES[key] || EXPERT_SELECTS[key];
    settings.ui_values = { ...(settings.ui_values || {}), [key]: cfg.def };
    try { await invoke("set_ui_value", { key, value: cfg.def }); } catch (err) { errToast(err); }
    applyValues();
    await refreshTiles(key);
  } else if (kind === "dropdowns") {
    const cfg = EXPERT_DROPDOWNS[key];
    await cfg.set(cfg.def);
  } else if (kind === "paths") {
    EXPERT_PATHS[key].onReset(); // already re-renders itself
    return;
  }
  if (after) after(); else renderExpert();
}

// Flags whose state changes the tile DOM itself (everything else is a body class).
// rotatingPrompts/rotateBadge gate a tile's own rotate-badge element -- without
// a full rebuild here, an already-open grid keeps showing (or hiding) it until some
// unrelated re-render happens to catch up.
const TILE_DOM_FLAGS = new Set(["tilePreview", "captions", "promptNotes", "notesInTooltip", "tileTimeTip", "rotatingPrompts", "rotateBadge"]);

function flagRow(key) {
  const row = document.createElement("label");
  row.className = "field switch-field";
  withTip(row, key);
  const span = document.createElement("span");
  span.textContent = t(flagLabelKey(key));
  row.append(span);
  // Features that ship OFF get a small badge (a sign, not text crowding the
  // label) so it's clear at a glance which settings are additions beyond the
  // standard configuration. Read from the shipped default, so a brand that turns
  // a feature off is labelled correctly too.
  if (!shipsOn(key)) {
    const badge = document.createElement("span");
    badge.className = "default-off-badge";
    badge.title = t("defaultOff");
    badge.setAttribute("role", "img");
    badge.setAttribute("aria-label", t("defaultOff"));
    row.append(badge);
  }
  const input = document.createElement("input");
  input.type = "checkbox";
  input.className = "switch";
  input.checked = flagState(key);
  input.addEventListener("change", async () => {
    const enabled = input.checked;
    // Turning sync on starts writing every text prompt as a plain-text .md file
    // next to the encrypted DB — say so before it happens, not just in the help doc.
    if (key === "promptSync" && enabled) {
      const ok = await confirmDialog({
        title: t(flagLabelKey(key)),
        message: t("syncPlaintextConfirm"),
        confirmLabel: t("syncPlaintextConfirmOk"),
      });
      if (!ok) { input.checked = false; return; }
    }
    settings.ui_flags = { ...(settings.ui_flags || {}), [key]: enabled };
    try {
      await invoke("set_ui_flag", { key, enabled });
    } catch (err) {
      // The backend refused to switch the feature on (today: prompt sync, whose
      // safety backup or first folder write failed). It left the flag off, so the switch has to follow —
      // a checkbox that shows a state the store does not have is worse than the error.
      const msg = String(err);
      if (msg.startsWith("sync-backup-failed")) toast(t("syncBackupFailed"));
      else if (msg.startsWith("sync-write-failed")) toast(t("syncWriteFailed"));
      else toast(msg);
      settings.ui_flags = { ...(settings.ui_flags || {}), [key]: !enabled };
      input.checked = !enabled;
    }
    // Read once at process start (before the builder exists) — flipping it live
    // cannot change which guard is already running.
    if (key === "allowMultipleInstances") toast(t("toastRestartNeeded"));
    applyFlags();
    // "only changed" has to recompute after EVERY flag (not just gating ones,
    // which is all a plain re-render used to care about) — the row set depends on
    // this flag's own value now, not only on what it gates.
    if (GATING_FLAGS.has(key) || expertOnlyChanged) renderExpert();
    else syncRowReset("flags", key); // reset icon must not wait for an unrelated rebuild
    renderViews();
    // Only these flags change what a tile contains; the rest are body classes,
    // so a full grid rebuild (and its flash) is unnecessary.
    if (TILE_DOM_FLAGS.has(key)) await renderGrid(true);
    else fitAllTiles();
    // The only TILE_DOM_FLAGS member that also changes a library row's tooltip.
    if (key === "tileTimeTip" && !libraryEl.classList.contains("hidden")) renderLibrary();
  });
  const resetBtn = rowResetBtn("flags", key);
  if (resetBtn) row.append(resetBtn);
  row.append(input);
  return row;
}

// Almost every expert value only drives a CSS variable, so rebuilding the whole grid
// for it threw away and recreated every tile — visible as a flash through the
// translucent settings overlay. Only these actually change tile DOM.
const GRID_DOM_VALUES = new Set(["previewLen", "gridMax", "notesTipLen"]);
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
  try { await invoke("set_ui_value", { key, value }); } catch (err) { errToast(err); }
  applyValues();
  await refreshTiles(key);
  // Fires once on release, not per drag frame — same "no flash mid-drag" reason
  // previewValue() never calls this, safe to rebuild the (possibly filtered) list here.
  if (expertOnlyChanged) renderExpert();
  else syncRowReset("values", key); // reset icon must not wait for an unrelated rebuild
}

function valueRow(key) {
  const cfg = EXPERT_VALUES[key];
  // A 0-stop can carry a special label (e.g. auto-advance "unlimited") instead of "0min".
  const fmt = (v) => v === 0 && cfg.zeroLabel ? t(cfg.zeroLabel) : `${v}${cfg.unit}`;
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
  // .value-head is `justify-content: space-between` over exactly two children —
  // a 3rd sibling would misplace, so the reset icon joins `out` in one flex group.
  const right = document.createElement("span");
  right.className = "value-head-right";
  right.append(out);
  const resetBtn = rowResetBtn("values", key);
  if (resetBtn) right.append(resetBtn);
  head.append(span, right);
  row.append(head, slider);
  // Both quality sliders are a JPEG-only concept -- every other screenshot
  // format is always lossless (see encode_screenshot in lib.rs), so a
  // quality percentage would apply to nothing. renderExpert() rebuilds this
  // row whenever screenshotFormat itself changes (dropdownRow's own change
  // handler), so this stays in sync without a dedicated listener of its own.
  if (key === "snipQuality" || key === "snipSaveQuality") {
    row.classList.toggle("hidden", txt("screenshotFormat") !== "jpeg");
  }
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
  const resetBtn = rowResetBtn("selects", key);
  if (resetBtn) head.appendChild(resetBtn);
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
  // The presets show their unit ("7d", "24h") right on the option — the free-entry
  // field cannot, since it holds only the number. Without this, a value typed here
  // looked unit-less, inviting the unit to be typed into the number field itself
  // ("7d"), which a number input silently discards with no error at all.
  const unit = document.createElement("span");
  unit.className = "expert-num-unit";
  unit.textContent = cfg.unit;
  const isPreset = cfg.options.includes(cur);
  sel.value = isPreset ? String(cur) : "custom";
  num.value = cur;
  num.classList.toggle("hidden", isPreset);
  unit.classList.toggle("hidden", isPreset);
  attachSelectPicker(sel); // themed popup instead of the native OS dropdown
  const persist = async (value) => {
    settings.ui_values = { ...(settings.ui_values || {}), [key]: value };
    try { await invoke("set_ui_value", { key, value }); } catch (err) { errToast(err); }
  };
  sel.addEventListener("change", () => {
    if (sel.value === "custom") {
      num.classList.remove("hidden");
      unit.classList.remove("hidden");
      num.focus();
      // No re-render here even under "only changed" — it would rebuild the row
      // (a fresh, unfocused num) right out from under the focus() above, right as
      // the user is about to type into it. The number field's own "change" below
      // covers the filter once a value is actually committed.
      persist(Math.max(1, Number(num.value) || cfg.def)).then(() => syncRowReset("selects", key));
    } else {
      num.classList.add("hidden");
      unit.classList.add("hidden");
      persist(Number(sel.value)).then(() => syncRowReset("selects", key));
      if (expertOnlyChanged) renderExpert();
    }
  });
  num.addEventListener("change", () => {
    const v = Math.max(1, Math.min(3650, Number(num.value) || cfg.def));
    num.value = v;
    persist(v).then(() => syncRowReset("selects", key));
    if (expertOnlyChanged) renderExpert();
  });
  row.append(head, sel, num, unit);
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
  const resetBtn = rowResetBtn("dropdowns", key);
  if (resetBtn) head.appendChild(resetBtn);
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
  sel.addEventListener("change", async () => {
    await cfg.set(sel.value);
    // A setter may decline the picked value instead of persisting it --
    // ocrLang's "Manage languages..." entry opens a dialog and leaves the actual
    // language untouched -- resync so the select never keeps showing a choice
    // that never took effect. A no-op for every other dropdown, whose get()
    // already reports back exactly what set() just wrote.
    sel.value = cfg.get();
    if (expertOnlyChanged) renderExpert();
    else syncRowReset("dropdowns", key); // reset icon must not wait for an unrelated rebuild
  });
  attachSelectPicker(sel); // themed popup instead of the native OS dropdown
  row.append(head, sel);
  if (key === "recQuality" || key === "recFormat") row.appendChild(recFfmpegHintEl(key, sel));
  return row;
}

// The reason line under recQuality/recFormat: the GIF format's fixed 30s cap
// (always shown, ffmpeg or not) and, when ffmpeg is missing, why the picked
// step/format cannot actually be produced plus an inline offer to fetch it --
// on the control itself, not only in the help (240's explicit requirement).
function recFfmpegHintEl(key, sel) {
  const hint = document.createElement("div");
  hint.className = "expert-row-hint";
  const update = () => {
    ensureFfmpegStatusChecked();
    const v = sel.value;
    const needsFfmpeg = key === "recQuality" ? v === "lossless" : v !== "mp4";
    const parts = [];
    if (key === "recFormat" && v === "gif") parts.push(t("recFormatGifHint"));
    if (key === "recQuality" && v === "lossless") parts.push(t("recQualityLosslessHint"));
    if (needsFfmpeg && ffmpegAvailable === false) parts.push(t("recFfmpegMissingHint"));
    hint.classList.toggle("hidden", parts.length === 0);
    hint.replaceChildren(document.createTextNode(parts.join(" ")));
    if (needsFfmpeg && ffmpegAvailable === false) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "ghost-btn expert-row-hint-btn";
      btn.textContent = t("ffmpegOfferConfirm");
      btn.addEventListener("click", () => EXPERT_ACTIONS["ffmpeg-download"]?.(btn));
      hint.appendChild(btn);
    }
  };
  update();
  sel.addEventListener("change", update);
  return hint;
}

// Folder-path options (expert): current path + a Change (folder picker) and,
// where applicable, a reset-to-default button.
let currentDataDir = "";
let defaultScreenshotDir = "";
let defaultSyncDir = "";
let defaultRecordingDir = "";
let ocrLangsCache = []; // [{tag, label}], fetched once at startup (init())
let micDevicesCache = []; // [{id, name}], fetched once at startup (init())
// Whether ffmpeg was found (see ffmpeg_status's own doc comment: never probed
// on app start, only lazily the first time a control that needs it renders --
// here, the first time the recQuality/recFormat expert rows are built).
// null = not checked yet.
let ffmpegAvailable = null; // null = unknown/unchecked, true/false = confirmed present/absent
let ffmpegChecking = false; // separate in-flight guard -- a slow probe must not render as "confirmed missing"
function ensureFfmpegStatusChecked() {
  if (ffmpegAvailable !== null || ffmpegChecking) return;
  ffmpegChecking = true;
  invoke("ffmpeg_status")
    .then((p) => { ffmpegAvailable = !!p; })
    // A failed probe (not "ffmpeg missing", the call itself erroring) leaves
    // this null rather than latching false, so the next render retries
    // instead of claiming "missing" for the rest of the session.
    .catch(() => { ffmpegAvailable = null; })
    .finally(() => { ffmpegChecking = false; renderExpert(); });
}
let voiceLangsCache = []; // [{tag, label}], fetched once at startup (init())
// voice_support()'s answer, optimistic before it resolves (this app is
// Windows-only, so an unsupported machine is the rare case, not the default).
let voiceSupport = { supported: true, engine: "", langs: [], needs_online: false };
// Whether the backend still manages to seal everything it writes. Read once at
// startup and pushed by the "store-unsealed" event if it changes mid-session;
// assumed true until the backend says otherwise, so a failed call never invents
// a warning that is not real.
let storeSealed = true;
// Whether the settings row on disk was present but unreadable at the last load
// (query error, decrypt failure, corrupt JSON) -- unlike storeSealed this never
// changes mid-session on its own, only through reset/import, both of which
// already reload the whole window, so refreshSettingsLocked() is called from
// there instead of a push event. Assumed false until the backend says otherwise.
let settingsLocked = false;
async function refreshSettingsLocked() {
  settingsLocked = (await invoke("settings_locked").catch(() => false)) === true;
  if (!$("settings-expert").classList.contains("hidden")) renderExpert();
}
// Whether the LAST settings write actually reached disk -- like storeSealed
// (not settingsLocked) this can flip at any time mid-session, so the live
// state comes from the existing "persist-failed" event rather than a poll.
let settingsWriteFailed = false;
const EXPERT_PATHS = {
  screenshotDir: {
    label: "valScreenshotDir",
    gate: "screenshot", // no folder picker when the screenshot tool is off
    def: () => defaultScreenshotDir,
    get: () => settings.ui_texts?.screenshotDir || defaultScreenshotDir,
    change: async () => {
      const dir = await invoke("pick_folder").catch(() => null);
      if (!dir) return;
      settings.ui_texts = { ...(settings.ui_texts || {}), screenshotDir: dir };
      invoke("set_ui_text", { key: "screenshotDir", value: dir }).catch((e) => errToast(e));
      renderExpert();
    },
    onReset: () => {
      settings.ui_texts = { ...(settings.ui_texts || {}), screenshotDir: "" };
      invoke("set_ui_text", { key: "screenshotDir", value: "" }).catch((e) => errToast(e));
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
      try { oldPath = await invoke("set_data_dir", { dir }); } catch (e) { errToast(e); return; }
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
  syncDir: {
    label: "valSyncDir",
    gate: "promptSync",
    def: () => defaultSyncDir,
    get: () => settings.ui_texts?.syncDir || defaultSyncDir,
    change: async () => {
      const dir = await invoke("pick_folder").catch(() => null);
      if (!dir) return;
      settings.ui_texts = { ...(settings.ui_texts || {}), syncDir: dir };
      invoke("set_ui_text", { key: "syncDir", value: dir }).catch((e) => errToast(e));
      renderExpert();
    },
    onReset: () => {
      settings.ui_texts = { ...(settings.ui_texts || {}), syncDir: "" };
      invoke("set_ui_text", { key: "syncDir", value: "" }).catch((e) => errToast(e));
      renderExpert();
    },
  },
  // Target for the recording result window's plain "Save"/"Copy" -- the
  // raw capture itself always stays in <data>/recordings until one of those
  // moves it here (or the window closes unsaved and it is deleted instead).
  recordingDir: {
    label: "valRecordingDir",
    gate: "screenRecord",
    def: () => defaultRecordingDir,
    get: () => settings.ui_texts?.recordingDir || defaultRecordingDir,
    change: async () => {
      const dir = await invoke("pick_folder").catch(() => null);
      if (!dir) return;
      settings.ui_texts = { ...(settings.ui_texts || {}), recordingDir: dir };
      invoke("set_ui_text", { key: "recordingDir", value: dir }).catch((e) => errToast(e));
      renderExpert();
    },
    onReset: () => {
      settings.ui_texts = { ...(settings.ui_texts || {}), recordingDir: "" };
      invoke("set_ui_text", { key: "recordingDir", value: "" }).catch((e) => errToast(e));
      renderExpert();
    },
    // The only path row that also shows how much space its folder holds, plus
    // a cleanup button right next to that -- pathRow() renders both only when
    // this is set, so every other path (screenshotDir, syncDir, dataDir) is
    // untouched.
    videoCleanup: true,
  },
};

// (folder path -> {count, bytes}) for the recordingDir video-folder readout, so
// switching the folder and switching back does not re-fetch what is already
// known. Cleared (falsy) means "not fetched yet for this path" -- pathRow()
// shows a loading placeholder until refreshVideoFolderInfo() resolves.
let videoFolderInfo = null;
let videoFolderInfoDir = null;
function refreshVideoFolderInfo(dir) {
  videoFolderInfoDir = dir;
  invoke("video_folder_stats", { dir })
    .then(([count, bytes]) => { videoFolderInfo = { count, bytes }; renderExpert(); })
    .catch(() => { videoFolderInfo = { count: 0, bytes: 0 }; renderExpert(); });
}

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
  const resetBtn = rowResetBtn("paths", key);
  if (resetBtn) btns.appendChild(resetBtn);
  row.append(head, btns);
  if (cfg.videoCleanup) {
    const dir = cfg.get();
    if (dir !== videoFolderInfoDir) refreshVideoFolderInfo(dir);
    const info = document.createElement("div");
    info.className = "path-video-stats";
    const text = document.createElement("span");
    text.textContent = videoFolderInfo
      ? t("videoFolderStatsLine").replace("{n}", videoFolderInfo.count).replace("{size}", fmtBytes(videoFolderInfo.bytes))
      : t("videoFolderStatsLoading");
    const cleanBtn = document.createElement("button");
    cleanBtn.type = "button";
    cleanBtn.className = "ghost-btn";
    cleanBtn.textContent = t("videoFolderCleanupBtn");
    cleanBtn.disabled = !videoFolderInfo || videoFolderInfo.count === 0;
    cleanBtn.addEventListener("click", () => openVideoCleanup(dir));
    info.append(text, cleanBtn);
    row.appendChild(info);
  }
  return withTip(row, key);
}

// The full "asks for an age, lists what that would delete, deletes only after
// confirmation" flow for the recordingDir cleanup button above. Preview and the
// real delete share the exact same backend scan (video_folder_cleanup_preview /
// video_folder_cleanup both call video_folder_stale with the same age), so what
// is shown here is exactly what gets removed -- never a second, drifting list.
async function openVideoCleanup(dir) {
  const modal = $("video-cleanup-modal");
  const ageSel = $("video-cleanup-age");
  const summary = $("video-cleanup-summary");
  const list = $("video-cleanup-list");
  const confirmBtn = $("video-cleanup-confirm");
  const refresh = async () => {
    const ageDays = Number(ageSel.value);
    const files = await invoke("video_folder_cleanup_preview", { dir, ageDays }).catch((e) => { errToast(e); return []; });
    const totalBytes = files.reduce((s, f) => s + f.size, 0);
    summary.textContent = files.length
      ? t("videoCleanupSummary").replace("{n}", files.length).replace("{size}", fmtBytes(totalBytes))
      : t("videoCleanupNone");
    confirmBtn.disabled = files.length === 0;
    list.innerHTML = "";
    for (const f of files) {
      const row = document.createElement("div");
      row.className = "import-preview-row";
      const name = document.createElement("span");
      name.className = "import-preview-name";
      name.textContent = f.name;
      const size = document.createElement("span");
      size.className = "import-preview-badge";
      size.textContent = fmtBytes(f.size);
      row.append(name, size);
      list.appendChild(row);
    }
  };
  // The trash section: a discarded recording sitting in this same
  // folder's "verworfen" subfolder, restorable or deletable one at a time --
  // independent of the age-based bulk cleanup above, so it refreshes on its
  // own after either action instead of closing the whole dialog.
  const discardedSection = $("video-cleanup-discarded-section");
  const discardedList = $("video-cleanup-discarded-list");
  const refreshDiscarded = async () => {
    const files = await invoke("list_discarded_recordings", { dir }).catch(() => []);
    discardedSection.classList.toggle("hidden", files.length === 0);
    discardedList.innerHTML = "";
    for (const f of files) {
      const row = document.createElement("div");
      row.className = "import-preview-row";
      const name = document.createElement("span");
      name.className = "import-preview-name";
      name.textContent = f.name;
      const size = document.createElement("span");
      size.className = "import-preview-badge";
      size.textContent = fmtBytes(f.size);
      const restoreBtn = document.createElement("button");
      restoreBtn.type = "button";
      restoreBtn.className = "ghost-btn";
      restoreBtn.textContent = t("videoDiscardedRestore");
      restoreBtn.addEventListener("click", async () => {
        const ok = await invoke("restore_discarded_recording", { dir, name: f.name }).catch(() => false);
        if (ok) { toast(t("videoDiscardedRestored")); refreshDiscarded(); refreshVideoFolderInfo(dir); }
        else toast(t("videoDiscardedRestoreFailed"));
      });
      const deleteBtn = document.createElement("button");
      deleteBtn.type = "button";
      deleteBtn.className = "ghost-btn danger-btn";
      deleteBtn.textContent = t("videoDiscardedDeleteNow");
      deleteBtn.addEventListener("click", async () => {
        const ok = await invoke("delete_discarded_recording_now", { dir, name: f.name }).catch(() => false);
        if (ok) { refreshDiscarded(); refreshVideoFolderInfo(dir); }
        else toast(t("videoDiscardedDeleteFailed"));
      });
      const actions = document.createElement("div");
      actions.className = "import-preview-actions";
      actions.append(restoreBtn, deleteBtn);
      row.append(name, size, actions);
      discardedList.appendChild(row);
    }
  };
  ageSel.value = "30";
  ageSel.onchange = refresh;
  await refresh();
  await refreshDiscarded();
  modal.classList.remove("hidden");
  const confirmed = await new Promise((resolve) => {
    const done = (v) => {
      modal.classList.add("hidden");
      ageSel.onchange = null;
      confirmBtn.removeEventListener("click", onOk);
      $("video-cleanup-cancel").removeEventListener("click", onCancel);
      $("video-cleanup-close").removeEventListener("click", onCancel);
      resolve(v);
    };
    const onOk = () => done(true);
    const onCancel = () => done(false);
    confirmBtn.addEventListener("click", onOk);
    $("video-cleanup-cancel").addEventListener("click", onCancel);
    $("video-cleanup-close").addEventListener("click", onCancel);
  });
  if (!confirmed) return;
  const ageDays = Number(ageSel.value);
  const deleted = await invoke("video_folder_cleanup", { dir, ageDays }).catch((e) => { errToast(e); return 0; });
  toast(t("videoFolderCleanupDone").replace("{n}", deleted));
  refreshVideoFolderInfo(dir);
}

let expertTab = 0;
let expertQuery = ""; // expert-menu search filter
let expertOnlyChanged = false; // hide every row still at its default
// The filter is a pill whose checkbox is visually hidden, so the "on" look is a
// class rather than :checked (:has() is avoided project-wide, see main.css:733).
const syncOnlyChangedChip = () =>
  $("expert-only-changed-chip")?.classList.toggle("active", expertOnlyChanged);
// Does this row differ from its shipped default? Actions and the colour palette
// have no single "default value" to diff against, so they are excluded up in
// renderExpert() instead of handled here.
function isRowChanged(kind, key) {
  if (kind === "flags") return flagState(key) !== shipsOn(key);
  if (kind === "values" || kind === "selects") {
    const cfg = EXPERT_VALUES[key] || EXPERT_SELECTS[key];
    return val(key) !== cfg.def;
  }
  if (kind === "dropdowns") {
    const cfg = EXPERT_DROPDOWNS[key];
    return cfg.def !== undefined && cfg.get() !== cfg.def;
  }
  if (kind === "paths") {
    // Only a path with a "reset to default" button (screenshotDir, syncDir) has a
    // default to diff against — dataDir is always "current", never changed from
    // anything. A stored value equal to the (computed) default is unchanged —
    // picking exactly the default folder must not mark the row as changed.
    const cfg = EXPERT_PATHS[key];
    if (!cfg.onReset || !cfg.def) return false;
    const stored = settings.ui_texts?.[key] || "";
    return !!stored && stored !== cfg.def();
  }
  return false;
}
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
  // Only show it while a hue actually differs from DEFAULT_COLORS -- same rule
  // as every other row's rowResetBtn(), the palette just has no single key to diff.
  const changed = cur.some((c, i) => (HEX_RE.test(c) ? c : DEFAULT_COLORS[i + 1]) !== DEFAULT_COLORS[i + 1]);
  if (changed) {
    const reset = document.createElement("button");
    reset.type = "button";
    reset.className = "ghost-btn palette-reset";
    reset.textContent = t("paletteReset");
    reset.addEventListener("click", () => savePalette(null));
    row.append(grid, reset);
  } else {
    row.append(grid);
  }
  return row;
}

// Persist the palette (null = reset to defaults), remap every saved prompt/view
// color from the old hue to the new one, and refresh every colour surface live.
async function savePalette(colors) {
  const oldHues = COLORS.slice(1);
  const value = colors ? JSON.stringify(colors) : "";
  settings.ui_texts = { ...(settings.ui_texts || {}), palette: value };
  try { await invoke("set_ui_text", { key: "palette", value }); } catch (e) { errToast(e); }
  loadPalette();
  const newHues = COLORS.slice(1);
  const pairs = [];
  for (let i = 0; i < PALETTE_LEN; i++) {
    if (oldHues[i] && newHues[i] && oldHues[i] !== newHues[i]) pairs.push([oldHues[i], newHues[i]]);
  }
  if (pairs.length) {
    try { await invoke("remap_colors", { pairs }); } catch (e) { toast(String(e) === "save-failed" ? t("persistFailed") : String(e)); }
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
    ...Object.values(FLAG_GATES),
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
// Expert-menu search also matches a setting's tooltip text, not just its
// label — a setting is often findable only by a word that lives in the tip.
function settingHit(settingKey, labelKey, q) {
  if (i18nHit(labelKey, q)) return true;
  if (!EXPERT_TIP_SET.has(settingKey)) return false;
  const tipKey = `tip${settingKey[0].toUpperCase()}${settingKey.slice(1)}`;
  return i18nHit(tipKey, q);
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

// ---- Expert-menu notices -------------------------------------------------
// A notice is not a setting: it reports a state the app is currently in, has
// nothing to switch, and appears only while that state holds. Notices render at
// the top of their group, above the settings, so the state is read before the
// controls it affects.
//
// There is deliberately NO switch to hide the unsealed-store notice. Everything
// else in the expert menu can be turned off; a warning that describes what is
// actually on disk right now cannot, because a warning nobody can see is worth
// nothing. This is the one exception to "every feature gets a toggle".
const EXPERT_NOTICES = {
  storeUnsealed: {
    when: () => !storeSealed,
    title: "storeUnsealedRow",
    body: "storeUnsealedRowNote",
  },
  settingsLocked: {
    when: () => settingsLocked,
    title: "settingsLockedRow",
    body: "settingsLockedRowNote",
  },
  settingsWriteFailed: {
    when: () => settingsWriteFailed,
    title: "settingsWriteFailedRow",
    body: "settingsWriteFailedRowNote",
  },
};

function noticeRow(id) {
  const def = EXPERT_NOTICES[id];
  const row = document.createElement("div");
  row.className = "expert-notice";
  row.dataset.notice = id;
  row.setAttribute("role", "status");
  const icon = document.createElement("span");
  icon.className = "expert-notice-icon";
  putIcon(icon, ICON_WARN);
  const text = document.createElement("div");
  const head = document.createElement("strong");
  head.textContent = t(def.title);
  const body = document.createElement("span");
  body.textContent = t(def.body);
  text.append(head, body);
  row.append(icon, text);
  return row;
}

function actionRow(a) {
  if (a.type === "hotkey") return hotkeyRow(a.hotkeyKey, a.label);
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "ghost-btn expert-action";
  btn.textContent = t(a.label);
  // Passing the button itself lets a long-running action (ffmpeg-download) show
  // progress in its own label and repurpose a second click as cancel — every
  // existing handler ignores the extra argument.
  btn.addEventListener("click", () => EXPERT_ACTIONS[a.id]?.(btn));
  return btn;
}

// A hotkey-capture row for the expert menu itself, not a button that jumps to a
// settings field -- click the input, press a combo, same as every other hotkey
// field in the app (bindHotkeyField). Rebuilt on every renderExpert() call, so
// the field is bound fresh each time rather than once at boot like the settings
// dialog's own fields.
const HOTKEY_ROW_DEFS = {
  recStopHotkey: { current: () => settings.rec_stop_hotkey, onCommit: commitRecStopHotkey },
  recPauseHotkey: { current: () => settings.rec_pause_hotkey, onCommit: commitRecPauseHotkey },
};
function hotkeyRow(key, labelKey) {
  const def = HOTKEY_ROW_DEFS[key];
  const row = document.createElement("div");
  // Label above the capture field, same stacked shape as pathRow -- the expert
  // menu's own column is narrower than the settings dialog .field-inline was
  // built for, and a single "label ... 460px field" line would overflow it.
  row.className = "field value-field hotkey-field";
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(labelKey);
  head.appendChild(span);
  const wrap = document.createElement("div");
  wrap.className = "hotkey-row";
  const input = document.createElement("input");
  input.className = "modal-input";
  input.type = "text";
  input.readOnly = true;
  input.placeholder = t("hotkeyPh");
  input.setAttribute("aria-label", t(labelKey));
  input.value = prettyAccel(def.current());
  const clear = document.createElement("button");
  clear.type = "button";
  clear.className = "ghost-btn";
  clear.textContent = t("hotkeyClear");
  clear.addEventListener("click", async () => {
    try { await def.onCommit(""); } catch (_) { /* Clear always leaves an empty field regardless */ }
    input.value = "";
  });
  wrap.append(input, clear);
  row.append(head, wrap);
  bindHotkeyField(input, { current: def.current, onCommit: def.onCommit });
  return withTip(row, key);
}

// Rows that carry a label plus one narrow control fit two to a line; a slider needs
// the full width for its track, and anything with children must stay a full row so
// its subtree reads as one block.
const COMPACT_KINDS = new Set(["dropdowns", "selects"]);
const isCompactRow = (n) => COMPACT_KINDS.has(n.kind) && !n.kids.length;

// Emit one level of the row tree: runs of compact rows are paired into a two-column
// grid, everything else is a full-width row followed by its own subtree.
function emitExpertRows(list, parent, actionById, depth) {
  const build = (n) => {
    const row = n.kind === "actions"
      ? actionRow(actionById.get(n.k))
      : { flags: flagRow, values: valueRow, selects: selectRow, dropdowns: dropdownRow, paths: pathRow }[n.kind](n.k);
    // Indent by depth: a setting unlocked by a setting that is itself unlocked sits
    // one step further in, so the chain of dependencies reads off the left edge.
    if (depth > 0) {
      row.classList.add("expert-sub");
      row.style.setProperty("--expert-depth", String(depth));
    }
    return row;
  };
  for (let i = 0; i < list.length; i++) {
    if (isCompactRow(list[i])) {
      let j = i;
      while (j < list.length && isCompactRow(list[j])) j++;
      if (j - i >= 2) {
        const cols = document.createElement("div");
        cols.className = "expert-cols";
        if (depth > 0) {
          cols.classList.add("expert-sub");
          cols.style.setProperty("--expert-depth", String(depth));
        }
        for (let x = i; x < j; x++) {
          const row = build(list[x]);
          row.classList.remove("expert-sub"); // the wrapper carries the indent
          row.style.removeProperty("--expert-depth");
          cols.appendChild(row);
        }
        parent.appendChild(cols);
        i = j - 1;
        continue;
      }
    }
    parent.appendChild(build(list[i]));
    if (list[i].kids.length) emitExpertRows(list[i].kids, parent, actionById, depth + 1);
  }
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
  // Like searching, "only changed" scans every tab at once — a per-tab filter
  // would defeat the point of a diff view across the whole expert menu.
  const globalView = searching || expertOnlyChanged;
  // Tab bar lives in the fixed header (stays visible while the rows scroll); it is
  // hidden while scanning every tab at once.
  if (!globalView) {
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
  const tabs = globalView ? EXPERT_TABS : [EXPERT_TABS[expertTab]];
  const collapsed = expertCollapsedSet();
  let shown = 0;
  for (const tab of tabs) {
    for (const group of tab.groups) {
      const groupHit = searching && i18nHit(group.title, q);
      // A row whose gating feature is off is dropped in every mode, search included:
      // a setting the user cannot act on has no business being on screen. "Only
      // changed" stacks on top: a row also needs to differ from its default.
      const keep = (kind) => (group[kind] || []).filter((k) => {
        if (!gateOpen(EXPERT_DEFS[kind]?.[k]?.gate)) return false;
        if (expertOnlyChanged && !isRowChanged(kind, k)) return false;
        return !searching || groupHit || settingHit(k, EXPERT_LABEL_KEYS[kind](k), q);
      });
      const flags = groupFlags(group)
        .filter((f) => gateOpen(f.gate) && (!expertOnlyChanged || isRowChanged("flags", f.key)))
        .filter((f) => !searching || groupHit || settingHit(f.key, flagLabelKey(f.key), q))
        .map((f) => f.key);
      const values = keep("values");
      const selects = keep("selects");
      const dropdowns = keep("dropdowns");
      const paths = keep("paths");
      // Neither has a single "default" to diff against (a palette colour would need
      // its own per-swatch compare; an action is a button, not a value) — "only
      // changed" drops both rather than showing them unconditionally.
      const palette = !expertOnlyChanged && group.palette && (!searching || groupHit || i18nHit("expGroupColors", q));
      const actions = expertOnlyChanged ? [] : (group.actions || []).filter((a) => gateOpen(a.gate) && (!searching || groupHit || i18nHit(a.label, q)));
      if (globalView && !flags.length && !values.length && !selects.length && !dropdowns.length && !paths.length && !palette && !actions.length) continue;
      shown++;
      // Categories collapse on click (persisted); scanning every tab always shows them.
      const isCollapsed = !globalView && collapsed.has(group.title);
      const sec = document.createElement("div");
      sec.className = "expert-group" + (isCollapsed ? " collapsed" : "");
      const head = document.createElement("div");
      head.className = "expert-group-title";
      const titleSpan = document.createElement("span");
      titleSpan.textContent = t(group.title);
      head.appendChild(titleSpan);
      if (!globalView) {
        head.classList.add("collapsible");
        head.addEventListener("click", () => toggleExpertGroup(group.title, sec));
      }
      // Per-category reset — only worth showing when there is something in this
      // group to reset. Reuses the same isRowChanged() diff as 66/67.
      const right = document.createElement("span");
      right.className = "expert-group-title-right";
      const changedHere = changedKeysInGroup(group);
      const changedCount = changedHere.flags.length + changedHere.values.length +
        changedHere.selects.length + changedHere.dropdowns.length + changedHere.paths.length +
        (changedHere.palette ? 1 : 0);
      if (changedCount) {
        const resetBtn = document.createElement("button");
        resetBtn.type = "button";
        resetBtn.className = "expert-group-reset";
        resetBtn.title = t("expertResetGroupBtn");
        resetBtn.setAttribute("aria-label", t("expertResetGroupBtn"));
        putIcon(resetBtn, ICON_UNDO);
        resetBtn.addEventListener("click", (e) => {
          e.stopPropagation(); // must not also toggle the collapse
          resetExpertGroup(group, changedHere, changedCount);
        });
        right.appendChild(resetBtn);
      }
      head.appendChild(right);
      sec.appendChild(head);
      // State first, settings after — and nothing at all while the state does not
      // hold, so a healthy install sees no placeholder where a notice would go.
      for (const id of group.notices || []) {
        if (EXPERT_NOTICES[id]?.when()) sec.appendChild(noticeRow(id));
      }
      const actionById = new Map(actions.map((a) => [a.id, a]));
      if (searching) {
        // Flat list while searching (grouping a match under a non-matching toggle
        // would hide it); adjacency only matters for the calm, browsable view.
        for (const key of flags) sec.appendChild(flagRow(key));
        for (const key of values) sec.appendChild(valueRow(key));
        for (const key of selects) sec.appendChild(selectRow(key));
        for (const key of dropdowns) sec.appendChild(dropdownRow(key));
        for (const key of paths) sec.appendChild(pathRow(key));
        for (const a of actions) sec.appendChild(actionRow(a));
      } else {
        // Every kind of row obeys the same contract: a setting whose gate names a
        // toggle in THIS group renders right beneath that toggle and vanishes with
        // it, nested as deep as the gates go. Dropdowns and actions used to sit in
        // a block of their own at the end, which is what tore features apart.
        const shownFlags = new Set(flags);
        const ownerOf = (kind, k) => {
          const g = kind === "flags" ? FLAG_GATES[k]
            : kind === "actions" ? actionById.get(k)?.gate
            : EXPERT_DEFS[kind]?.[k]?.gate;
          return g && g[0] !== "!" && g !== k && shownFlags.has(g) ? g : null;
        };
        const byFlag = new Map();
        const roots = [];
        const lists = { flags, values, selects, dropdowns, paths, actions: actions.map((a) => a.id) };
        // Flags first, so a parent always exists before anything hangs off it.
        for (const kind of ["flags", "values", "selects", "dropdowns", "paths", "actions"]) {
          for (const k of lists[kind]) {
            const n = { kind, k, kids: [] };
            if (kind === "flags") byFlag.set(k, n);
            const owner = ownerOf(kind, k);
            (owner && byFlag.has(owner) ? byFlag.get(owner).kids : roots).push(n);
          }
        }
        emitExpertRows(roots, sec, actionById, 0);
      }
      if (palette) sec.appendChild(paletteRow());
      box.appendChild(sec);
    }
  }
  if (globalView && !shown) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t(!searching && expertOnlyChanged ? "expertNothingChanged" : "libraryNoResults");
    box.appendChild(empty);
  }
  box.scrollTop = keepScroll;
}

// Which of this group's own rows currently differ from their default — reuses
// the same gate + isRowChanged() checks renderExpert()'s keep() applies, just
// standalone so the reset button can both decide whether to show itself and know
// exactly what to touch.
function changedKeysInGroup(group) {
  const out = { flags: [], values: [], selects: [], dropdowns: [], paths: [], palette: false };
  for (const f of groupFlags(group)) {
    if (gateOpen(f.gate) && isRowChanged("flags", f.key)) out.flags.push(f.key);
  }
  for (const kind of ["values", "selects", "dropdowns", "paths"]) {
    for (const k of group[kind] || []) {
      if (gateOpen(EXPERT_DEFS[kind]?.[k]?.gate) && isRowChanged(kind, k)) out[kind].push(k);
    }
  }
  // Closes the one category the per-kind loop above cannot see — the colour
  // palette has no single key, just PALETTE_LEN live hues to diff against defaults.
  if (group.palette) out.palette = COLORS.slice(1).some((c, i) => c !== DEFAULT_COLORS[i + 1]);
  return out;
}

// Reset only what THIS category owns, straight through the same per-key
// set_ui_flag/set_ui_value/set_ui_text commands a manual edit would use — the
// backend's own side effects (screensaver exclusion, tray, clipboard watcher, …)
// are already keyed on the setting being touched, so resetting an unrelated
// category can never drag them along; nothing extra to guard here.
async function resetExpertGroup(group, changed, count) {
  const ok = await confirmDialog({
    title: t(group.title),
    message: t("expertResetGroupConfirm").replace("{n}", String(count)),
    confirmLabel: t("expertReset"),
  });
  if (!ok) return;
  for (const key of changed.flags) {
    const enabled = shipsOn(key);
    settings.ui_flags = { ...(settings.ui_flags || {}), [key]: enabled };
    try { await invoke("set_ui_flag", { key, enabled }); } catch (err) { errToast(err); }
  }
  for (const key of [...changed.values, ...changed.selects]) {
    const cfg = EXPERT_VALUES[key] || EXPERT_SELECTS[key];
    settings.ui_values = { ...(settings.ui_values || {}), [key]: cfg.def };
    try { await invoke("set_ui_value", { key, value: cfg.def }); } catch (err) { errToast(err); }
  }
  for (const key of changed.dropdowns) {
    const cfg = EXPERT_DROPDOWNS[key];
    if (cfg.def === undefined) continue;
    await cfg.set(cfg.def);
  }
  for (const key of changed.paths) EXPERT_PATHS[key].onReset?.();
  if (changed.palette) await savePalette(null); // does its own remap + refresh
  // Batched once for the whole category instead of per key — same reasoning as
  // onResetExpert()'s own full-reset refresh below.
  applyFlags();
  applyValues();
  renderViews();
  await renderGrid(true);
  fitAllTiles();
  toast(t("expertResetGroupDone"));
  renderExpert();
}

async function onResetExpert() {
  settings.ui_flags = {};
  settings.ui_values = {};
  settings.ui_texts = {};
  try {
    await invoke("reset_expert");
  } catch (err) {
    errToast(err);
  }
  loadPalette();
  refreshColorUI();
  applyFlags();
  renderExpert();
  // ui_flags also holds NORMAL settings (auto-paste, tooltips, screenshot saving,
  // favourites view). Wiping it resets those too, so the settings page has to be
  // redrawn or its checkboxes keep showing the old, no longer stored state.
  syncSettingsControls();
  refreshFavViewUi();
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
// Every OTHER view currently holding this prompt — a client-side scan, no
// round trip, since every view's layout is already loaded in `settings`.
const viewsContaining = (id, exceptViewId) =>
  settings.views.filter((v) => v.id !== exceptViewId && Object.prototype.hasOwnProperty.call(layoutOf(v), id));

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
  $("grid-empty").classList.add("hidden"); // this view has its own empty message
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
  pruneThumbCache(libThumbCache);
  pruneThumbCache(journalThumbCache);
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
      await updateExamplesBanner(); // re-evaluate on every real fetch, not just at startup
    } catch (e) {
      // IPC hiccup: keep the last good state instead of throwing into the
      // global handler and leaving a half-rendered grid.
      errToast(e);
      if (!settings || !prompts) return;
    }
  }
  if (favView && !favViewEnabled()) favView = false; // setting switched off underneath us
  if (favView) { renderFavGrid(); return; }
  const view = activeView();
  // A colored border around the grid itself (expert opt, default off) —
  // reuses the view's own color, kept current on every render so it never
  // shows a stale color after switching views.
  document.documentElement.style.setProperty("--active-view-color", view.color || "transparent");
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

  // A prompt inside a grid folder is drawn by that folder, not by itself.
  const folders = foldersOfView(view.id);
  const inFolder = new Set(folders.flatMap((f) => f.members || []));
  const byCell = new Map();
  for (const p of prompts) {
    if (inFolder.has(p.id)) continue;
    const cell = layout[p.id];
    if (cell) byCell.set(cellKey(...cell), p);
  }
  // A folder whose cell fell outside a resized grid — or landed on a taken one —
  // moves to the first free cell, and that repair is written back.
  const folderAt = new Map();
  for (const f of folders) {
    const [fc, fr] = f.cell || [0, 0];
    let cell = [fc, fr];
    const free = (c) => c[0] < cols && c[1] < rows &&
      !byCell.has(cellKey(c[0], c[1])) && !folderAt.has(cellKey(c[0], c[1]));
    if (!free(cell)) {
      const spot = firstFree(new Map([...byCell, ...folderAt]), cols, rows);
      if (!spot) continue; // no room at all: it reappears when the grid grows again
      cell = spot;
      saveFolder(f, { cell });
    }
    folderAt.set(cellKey(cell[0], cell[1]), f);
  }

  // Build off-DOM, attach once (1 reflow instead of cols*rows).
  const frag = document.createDocumentFragment();
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const cellEl = document.createElement("div");
      cellEl.className = "cell";
      cellEl.dataset.col = c;
      cellEl.dataset.row = r;
      const f = folderAt.get(cellKey(c, r));
      const p = byCell.get(cellKey(c, r));
      if (f) cellEl.appendChild(buildFolderTile(f));
      else if (p) cellEl.appendChild(buildTile(p));
      frag.appendChild(cellEl);
    }
  }
  gridEl.innerHTML = "";
  gridEl.appendChild(frag);
  // First run, nothing anywhere yet — a plain grid explains nothing on its own.
  $("grid-empty").classList.toggle("hidden", prompts.length > 0);

  // Members that belong to this view but have no cell at this grid size (grid
  // shrunk under them) still belong here — shown as a visible overhang instead
  // of a silent loss.
  const seated = new Set([...byCell.values()].map((p) => p.id));
  const overflowCount = view.members.filter((id) => !seated.has(id) && !inFolder.has(id)).length;
  $("grid-overflow").classList.toggle("hidden", overflowCount === 0);
  if (overflowCount > 0) {
    $("grid-overflow-text").textContent = t("viewOverflowTitle").replace("{n}", String(overflowCount)) + " " + t("viewOverflowBody");
  }

  pruneVideoCache();
  pruneThumbCache(libThumbCache);
  pruneThumbCache(journalThumbCache);
  renderViews();
  refreshFavViewUi();
  fitAllTiles();
}

// Header view switcher: one button per view (switch on click, manage on
// right-click) plus a trailing "+" to add a view via the name popup. With view
// groups on, a group takes the place of its members as one folder tab.
function renderViews() {
  viewsEl.innerHTML = "";
  // Multi-view disabled (expert menu): no switcher, no add button.
  if (!flag("multiView")) {
    viewsEl.classList.add("hidden");
    return;
  }
  closeViewGroupPop();
  for (const entry of viewTabOrder()) {
    viewsEl.appendChild(entry.group ? groupTabEl(entry.group, entry.views) : viewTabEl(entry.view));
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

async function switchToView(id) {
  favView = false; // picking a normal view leaves the favorites grid
  if (gridSelected.size) clearGridSelection(); // a selection belongs to what's on screen
  try {
    settings = await invoke("set_active_view", { id });
  } catch (err) {
    errToast(err);
    return; // a rejection must not leave the click doing nothing silently
  }
  await renderGrid(true); // prompts unchanged, settings already fresh
}

function viewTabEl(v) {
  const btn = document.createElement("button");
  // While the favorites view is open, no normal view is the active one.
  btn.className = "view-btn" + (!favView && v.id === settings.active_view ? " active" : "");
  btn.dataset.viewId = v.id;
  if (v.color) {
    btn.classList.add("colored");
    btn.style.setProperty("--view-color", v.color);
  }
  // The name lives in its own span so the merge preview can shrink it back into the
  // tab the way a folder tab's spans already do.
  const label = document.createElement("span");
  label.textContent = v.name;
  btn.appendChild(label);
  btn.title = t("viewSwitchHint");
  btn.addEventListener("click", () => { if (!tabDragBlockClick) switchToView(v.id); });
  btn.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    const items = [
      { label: t("editViewTitle"), run: () => openViewModal(v) },
      { label: t("duplicate"), run: () => duplicateView(v) },
    ];
    if (groupsOn()) items.push({ label: t("viewGroupNew"), run: () => newViewGroup([v.id], v.name) });
    openMiniMenu(items, e.clientX, e.clientY);
  });
  makeTabDraggable(btn, v.id, false);
  return btn;
}

// ---- View groups: several views collapsed into one folder tab ----
// The feature is opt-in and rides on multi-view; with either off the stored groups
// stay untouched and every view renders as its own tab again.
const groupsOn = () => optFlag("viewGroups") && flag("multiView");
const viewGroupList = () => (groupsOn() ? settings.view_groups || [] : []);
const groupOfView = (id) => viewGroupList().find((g) => (g.view_ids || []).includes(id));
const viewById = (id) => settings.views.find((v) => v.id === id);
// Members in tab-bar order, not in the order they were added.
const groupViews = (g) => settings.views.filter((v) => (g.view_ids || []).includes(v.id));

// Tab bar in visual order: a group occupies the slot of its first member and
// swallows the rest; every ungrouped view keeps its own tab.
function viewTabOrder() {
  const out = [];
  const seen = new Set();
  for (const v of settings.views) {
    const g = groupOfView(v.id);
    if (!g) { out.push({ view: v }); continue; }
    if (seen.has(g.id)) continue;
    seen.add(g.id);
    out.push({ group: g, views: groupViews(g) });
  }
  return out;
}
// Every view in the order the tab bar shows it (group members inline) — the order
// keyboard switching walks.
const viewOrder = () => viewTabOrder().flatMap((e) => (e.group ? e.views : [e.view]));

const CHEVRON_DOWN = '<svg viewBox="0 0 24 24" width="12" height="12" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" d="M6 9.5l6 6 6-6"/></svg>';
const GROUP_OUT_ICON = '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M14 5h5v14h-5M12 12H4m0 0 3-3m-3 3 3 3"/></svg>';

function groupTabEl(g, views) {
  const current = favView ? null : views.find((v) => v.id === settings.active_view);
  const btn = document.createElement("button");
  btn.className = "view-btn view-group-btn" + (current ? " active" : "");
  btn.dataset.groupId = g.id;
  if (g.color) {
    btn.classList.add("colored");
    btn.style.setProperty("--view-color", g.color);
  }
  const name = document.createElement("span");
  name.textContent = g.name;
  btn.appendChild(name);
  // A collapsed folder still has to say which page is on screen.
  if (current) {
    const cur = document.createElement("span");
    cur.className = "view-group-current";
    cur.textContent = current.name;
    btn.appendChild(cur);
  }
  const chev = document.createElement("span");
  chev.className = "view-group-chev";
  chev.innerHTML = CHEVRON_DOWN;
  btn.appendChild(chev);
  btn.addEventListener("click", () => {
    if (tabDragBlockClick) return;
    if (viewGroupPopId === g.id) closeViewGroupPop();
    else openViewGroupPop(g, btn);
  });
  btn.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    openMiniMenu([
      { label: t("viewGroupRename"), run: () => renameViewGroup(g) },
      { label: t("viewGroupColor"), run: () => pickViewGroupColor(g, btn) },
      { label: t("viewGroupDissolve"), danger: true, run: () => dissolveViewGroup(g) },
    ], e.clientX, e.clientY);
  });
  // A folder tab is a drag source too: dropped on a loose tab it swallows that view,
  // dropped on another folder the two become one.
  makeTabDraggable(btn, null, false, g.id);
  return btn;
}

// Group popover: the member list. Click switches, the arrow button (or dragging a
// row out) takes a view back to the tab bar.
const viewGroupPop = document.createElement("div");
viewGroupPop.className = "ctx view-group-pop hidden";
viewGroupPop.setAttribute("role", "menu");
document.body.appendChild(viewGroupPop);
let viewGroupPopId = null;

function closeViewGroupPop() {
  viewGroupPop.classList.add("hidden");
  viewGroupPopId = null;
}

// Shared by openViewGroupPop() and renderViewsPop(): both build a checkbox + optional
// colour dot + name row for a view — a new column here only needs adding once.
function viewRowBase(v, on) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "view-group-item" + (on ? " on" : "");
  const check = document.createElement("span");
  check.className = "tb-check";
  check.innerHTML = on ? TB_CHECK : "";
  const label = document.createElement("span");
  label.className = "view-group-item-name";
  label.textContent = v.name;
  if (v.color) {
    const dot = document.createElement("span");
    dot.className = "view-group-dot";
    dot.style.background = v.color;
    label.prepend(dot);
  }
  row.append(check, label);
  return row;
}

function openViewGroupPop(g, anchor) {
  viewGroupPop.innerHTML = "";
  viewGroupPopId = g.id;
  for (const v of groupViews(g)) {
    const on = !favView && v.id === settings.active_view;
    const row = viewRowBase(v, on);
    row.dataset.viewId = v.id;
    row.setAttribute("role", "menuitem");
    const out = document.createElement("span");
    out.className = "view-group-out";
    out.innerHTML = GROUP_OUT_ICON;
    out.title = t("viewGroupRemove");
    row.appendChild(out);
    row.addEventListener("click", (e) => {
      if (tabDragBlockClick) return;
      if (e.target.closest(".view-group-out")) { moveViewToGroup(v.id, ""); return; }
      closeViewGroupPop();
      switchToView(v.id);
    });
    makeTabDraggable(row, v.id, true);
    viewGroupPop.appendChild(row);
  }
  placePopover(viewGroupPop, anchor.getBoundingClientRect());
}

// Small shared right-click menu for view tabs and group tabs.
const miniMenu = document.createElement("div");
miniMenu.className = "ctx tab-menu hidden";
miniMenu.setAttribute("role", "menu");
document.body.appendChild(miniMenu);
function closeMiniMenu() { miniMenu.classList.add("hidden"); }
function openMiniMenu(items, x, y) {
  miniMenu.innerHTML = "";
  for (const it of items) {
    const b = document.createElement("button");
    b.type = "button";
    b.setAttribute("role", "menuitem");
    if (it.danger) b.className = "danger";
    b.textContent = it.label;
    b.addEventListener("click", () => { closeMiniMenu(); it.run(); });
    miniMenu.appendChild(b);
  }
  placePopoverAt(miniMenu, x, y);
}

async function saveViewGroup(g, patch) {
  try {
    settings = await invoke("save_view_group", {
      id: g.id,
      name: patch.name ?? g.name,
      color: patch.color ?? g.color ?? "",
      viewIds: patch.viewIds ?? (g.view_ids || []),
    });
  } catch (e) {
    errToast(e);
    return false;
  }
  renderViews();
  return true;
}

async function newViewGroup(viewIds, suggested) {
  const name = await confirmDialog({
    title: t("viewGroupNewTitle"),
    message: t("viewGroupNewMsg"),
    confirmLabel: t("save"),
    input: { type: "text", value: suggested || t("viewGroupDefaultName"), placeholder: t("viewGroupNamePh") },
    danger: false,
  });
  if (!name || !name.trim()) return;
  try {
    settings = await invoke("save_view_group", { id: "", name: name.trim(), color: "", viewIds });
  } catch (e) {
    errToast(e);
    return;
  }
  renderViews();
}

async function renameViewGroup(g) {
  const name = await confirmDialog({
    title: t("viewGroupRename"),
    message: t("viewGroupNewMsg"),
    confirmLabel: t("save"),
    input: { type: "text", value: g.name, placeholder: t("viewGroupNamePh") },
    danger: false,
  });
  if (!name || !name.trim() || name.trim() === g.name) return;
  await saveViewGroup(g, { name: name.trim() });
}

// The picker fires on every pointer move, so paint the tab live and write the
// colour once the user has settled on one.
let groupColorTimer = null;
function pickViewGroupColor(g, anchor) {
  openColorPop(anchor, g.color || "", (hex) => {
    const live = (settings.view_groups || []).find((x) => x.id === g.id);
    if (!live) return;
    live.color = hex;
    renderViews();
    clearTimeout(groupColorTimer);
    // Re-read the group on write: the settings object was replaced in between.
    groupColorTimer = setTimeout(() => {
      const now = (settings.view_groups || []).find((x) => x.id === g.id);
      if (now) saveViewGroup(now, { color: hex });
    }, 300);
  });
}

async function dissolveViewGroup(g) {
  const ok = await confirmDialog({
    title: t("viewGroupDissolve"),
    message: t("viewGroupDissolveMsg").replace("{name}", g.name),
    confirmLabel: t("viewGroupDissolve"),
  });
  if (!ok) return;
  try {
    settings = await invoke("delete_view_group", { id: g.id });
  } catch (e) {
    errToast(e);
    return;
  }
  renderViews();
}

// groupId "" takes the view out of whatever group holds it. One command either
// way, so moving between groups never needs a remove+add round trip.
async function moveViewToGroup(viewId, groupId) {
  if (groupOfView(viewId)?.id === groupId || (!groupId && !groupOfView(viewId))) return;
  try {
    settings = await invoke("set_view_group_member", { groupId, viewId });
  } catch (e) {
    errToast(e);
    return;
  }
  closeViewGroupPop();
  renderViews();
}

// Dragging a tab: onto another loose tab makes a group of the two, onto a group
// tab joins it, out of the popover leaves the group. Pointer events rather than
// HTML5 drag-and-drop — the webview reserves that for file drops.
let tabDrag = null;
let tabDragBlockClick = false;

// `groupId` marks the source as a whole folder tab instead of a single view.
function makeTabDraggable(el, viewId, fromGroup, groupId) {
  el.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || !groupsOn()) return;
    tabDrag = { viewId, fromGroup, groupId, el, startX: e.clientX, startY: e.clientY, moved: false, target: null };
  });
}

// Pour one folder's views into another and let the emptied one dissolve — a single
// write, so the tab bar never shows the half-moved state.
async function mergeViewGroups(fromId, intoId) {
  const from = viewGroupList().find((g) => g.id === fromId);
  const into = viewGroupList().find((g) => g.id === intoId);
  if (!from || !into) return;
  await saveViewGroup(into, { viewIds: [...(into.view_ids || []), ...(from.view_ids || [])] });
}

const tabDropTarget = (x, y) => document.elementFromPoint(x, y)?.closest(".view-btn") || null;

function markTabDrop(el) {
  if (tabDrag.target === el) return;
  tabDrag.target?.classList.remove("tab-drop");
  tabDrag.target = el;
  el?.classList.add("tab-drop");
}

window.addEventListener("pointermove", (e) => {
  if (!tabDrag) return;
  if (!tabDrag.moved) {
    if (Math.abs(e.clientX - tabDrag.startX) < DRAG_THRESHOLD &&
        Math.abs(e.clientY - tabDrag.startY) < DRAG_THRESHOLD) return;
    tabDrag.moved = true;
    tabDragBlockClick = true; // the click that follows the drop must not switch views
    tabDrag.el.classList.add("tab-dragging");
  }
  const hit = tabDropTarget(e.clientX, e.clientY);
  markTabDrop(hit && hit !== tabDrag.el && !hit.classList.contains("view-add-top") ? hit : null);
});

window.addEventListener("pointerup", async (e) => {
  if (!tabDrag) return;
  const d = tabDrag;
  tabDrag = null;
  d.el.classList.remove("tab-dragging");
  d.target?.classList.remove("tab-drop");
  if (!d.moved) return;
  const target = d.target;
  if (d.groupId) {
    if (target?.dataset.groupId) await mergeViewGroups(d.groupId, target.dataset.groupId);
    else if (target?.dataset.viewId) await moveViewToGroup(target.dataset.viewId, d.groupId);
    return;
  }
  if (target?.dataset.groupId) {
    await moveViewToGroup(d.viewId, target.dataset.groupId);
  } else if (target?.dataset.viewId && target.dataset.viewId !== d.viewId) {
    const other = target.dataset.viewId;
    // Two loose tabs dropped on each other become a new group; dropping onto a tab
    // that already sits in a group would be ambiguous, so it joins that group.
    const g = groupOfView(other);
    if (g) await moveViewToGroup(d.viewId, g.id);
    else await newViewGroup([other, d.viewId], viewById(other)?.name);
  } else if (d.fromGroup && !viewGroupPop.contains(document.elementFromPoint(e.clientX, e.clientY))) {
    await moveViewToGroup(d.viewId, "");
  }
});

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
    errToast(err);
    return;
  }
  closeViewModal();
  renderViews();
  await renderGrid(true);
}

// A new, empty view with the same name/color/grid size as the source — its
// tile placements are not copied, since a prompt cannot yet sit in two views at
// once.
async function duplicateView(v) {
  try {
    settings = await invoke("add_view", { name: `${v.name} (${t("duplicateSuffix")})`, color: v.color || "" });
    const newId = settings.active_view; // add_view switches to the view it just created
    settings = await invoke("set_view_grid", { id: newId, cols: v.cols, rows: v.rows });
  } catch (err) {
    errToast(err);
    return;
  }
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

// `commitId` is the prompt just newly picked into the chain (if it rotates) —
// its pointer only advances once this actual clipboard write succeeds, never from
// the pick alone (a pick that never reaches the clipboard must burn nothing).
function pushChainClipboard(commitId) {
  if (!chainSel.length) return;
  invoke("copy_text", { text: chainText() })
    .then((ok) => { if (ok && commitId) invoke("rotation_commit", { id: commitId }).catch(() => {}); })
    .catch((e) => errToast(e));
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

// A rotating/variable prompt's pick goes through an await (peek, var fill)
// between reading chainSel and mutating it -- without this, a fast double
// click on the same tile passes the "already selected?" check twice, both
// branches push, and the tile that DID get de-picked on the next click still
// matches indexOf on the leftover duplicate, so it stays visibly picked with
// its text silently gone. One in-flight pick/unpick per id at a time.
const chainPicking = new Set();
async function toggleChainPick(p) {
  if (!isTextPrompt(p)) return; // only text buttons chain
  if (chainPicking.has(p.id)) return;
  chainPicking.add(p.id);
  try {
    const i = chainSel.indexOf(p.id);
    if (i >= 0) {
      chainSel.splice(i, 1); // deselect → the rest renumber
      chainTexts.delete(p.id);
      refreshChainUi();
      pushChainClipboard();
      return;
    }
    // Rotating prompt — peek the entry a copy would use (never consumes), resolve
    // {#{variables}#} on THAT text, right when the button is picked into the chain.
    let rawText = p.text || p.name || "";
    let peeked = false; // only a successful peek may be followed by a commit below —
                         // a failed peek already fell back to p.text (a stale/wrong
                         // entry to consume) and committing anyway would advance
                         // whatever the pointer's REAL position actually is.
    if ((p.variants || []).length) {
      const peek = await invoke("rotation_peek", { id: p.id }).catch(() => null);
      if (peek) { rawText = peek.text; peeked = true; }
    }
    let text = rawText;
    if (varTokens(p, rawText).length) {
      const filled = await resolveVars(rawText);
      if (filled === null) return; // cancelled → leave it unselected
      text = filled;
    }
    chainSel.push(p.id);
    chainTexts.set(p.id, text);
    refreshChainUi();
    pushChainClipboard(peeked ? p.id : null);
  } finally {
    chainPicking.delete(p.id);
  }
}

// The tile's own chain-start button — a second entry point into the SAME
// mechanism a click on #chain-btn followed by a first tile pick already
// uses, not a parallel one. chainLock stays untouched: starting from a tile
// behaves like a normal, unlatched start.
async function startChainWith(p) {
  if (!flag("chainPrompts") || !flag("chainQuickStart")) return;
  if (chainMode || !isTextPrompt(p)) return; // button is hidden either way -- belt and braces
  setChainMode(true);
  await toggleChainPick(p);
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
// A folder for the pill-folder rename dialog.
const FOLDER_SVG = '<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" d="M3 6.5A1.5 1.5 0 0 1 4.5 5h4.2l2 2.4h8.8A1.5 1.5 0 0 1 21 8.9v9.6a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5Z"/></svg>';
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
  tipEl.innerHTML = '<div class="tile-tip-name"></div><div class="tile-tip-body"></div><div class="tile-tip-len"></div><div class="tile-tip-times"></div><div class="tile-tip-notes"></div><div class="tile-tip-rotate"></div><div class="tile-tip-hint"></div>';
  document.body.appendChild(tipEl);
  const nameEl = tipEl.querySelector(".tile-tip-name");
  const bodyEl = tipEl.querySelector(".tile-tip-body");
  const lenEl = tipEl.querySelector(".tile-tip-len");
  const timesEl = tipEl.querySelector(".tile-tip-times");
  const notesEl = tipEl.querySelector(".tile-tip-notes");
  const rotateEl = tipEl.querySelector(".tile-tip-rotate");
  const hintEl = tipEl.querySelector(".tile-tip-hint");
  const OFF = 16, PAD = 12;
  const placeAt = (cx, cy) => {
    const r = tipEl.getBoundingClientRect();
    let x = cx + OFF;
    let y = cy + OFF;
    if (x + r.width + PAD > innerWidth) x = cx - OFF - r.width;
    if (y + r.height + PAD > innerHeight) y = cy - OFF - r.height;
    const z = uiZoom(); // fixed popup lives inside the zoomed body
    // The flip above only guards the edge it checked — near a corner, with a
    // wide tooltip (expert tooltipWidth), the flipped side can itself run off the
    // FAR edge. Clamp both bounds, same as placePopover does for every other popup.
    tipEl.style.left = `${Math.max(PAD, Math.min(x, innerWidth - r.width - PAD)) / z}px`;
    tipEl.style.top = `${Math.max(PAD, Math.min(y, innerHeight - r.height - PAD)) / z}px`;
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
    // Read regardless of rich/plain -- a library row's plain "Copy" tooltip
    // carries these same two lines, just without the rest of the rich box.
    timesEl.textContent = el.dataset.tipTimes || "";
    if (rich) {
      nameEl.textContent = name;
      fillVarHighlighted(bodyEl, el.dataset.tipBody || "");
      lenEl.textContent = el.dataset.tipLen || "";
      // textContent, so a note is shown as the literal text the user typed.
      notesEl.textContent = el.dataset.tipNotes || "";
      rotateEl.textContent = el.dataset.tipRotate || "";
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
      lenEl.textContent = "";
      notesEl.textContent = "";
      // The rotate line is shared with the rich tile tip — leaving it filled here
      // stuck the last hovered tile's "next entry" text under every plain tooltip.
      rotateEl.textContent = "";
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
    const r = input.getBoundingClientRect();
    // Size to the terms (short), not the full search field — a full-width bar over a
    // wide library search looked oversized. Clamp: at least 220px, at most the field.
    pop.style.width = "";
    pop.style.minWidth = `${Math.min(220, r.width)}px`;
    pop.style.maxWidth = `${r.width}px`;
    placePopover(pop, r);
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
// Screen point -> the media's own local CSS coordinates at the CURRENT scale
// removed again, so it stays comparable to lbX/lbY (also pre-scale units)
// regardless of zoom level. Reads the live, already-transformed bounding
// rect, same trick as the video editor's toSource().
function lbToLocal(clientX, clientY) {
  const rect = lbMedia().getBoundingClientRect();
  return { x: (clientX - rect.left) / lbScale, y: (clientY - rect.top) / lbScale };
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
    // Keep the point under the cursor fixed instead of always zooming to the
    // media's centre: read it before and after the scale change and slide
    // the pan by the difference, scaled back up -- the pan is a CSS translate
    // applied AFTER the scale in the same transform, so it moves the media in
    // already-scaled screen pixels, not the pre-scale ones lbToLocal reads in.
    const before = lbToLocal(e.clientX, e.clientY);
    lbScale = Math.min(8, Math.max(1, lbScale * (e.deltaY < 0 ? 1.15 : 1 / 1.15)));
    if (lbScale === 1) {
      lbX = 0; lbY = 0;
    } else {
      lbApply(); // commit the new scale so the rect below reflects it
      const after = lbToLocal(e.clientX, e.clientY);
      lbX += (after.x - before.x) * lbScale;
      lbY += (after.y - before.y) * lbScale;
    }
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

// ---- Private notes: context the user keeps next to a prompt. Read in the
// editor, the tile tooltip and the library search — never in any copy path.
const notesOn = () => optFlag("promptNotes");
// Cap what the tooltip shows so one long note can never fill the screen.
const clipNote = (s) => {
  const max = val("notesTipLen");
  return max > 0 && s.length > max ? `${s.slice(0, max)}…` : s;
};

// Client-side cache of each rotating prompt's "next entry" state (index/pos/
// total/text), kept in sync by the backend's rotation-changed event — fired by every
// copy path in every window after the pointer actually moves. Populated lazily the
// first time a tile for that id renders (primeRotateState), so a fresh render never
// has to await anything itself; the badge/tooltip just stay blank for one frame.
const rotateState = new Map(); // id -> { index, pos, total, text }
// Per-id request generation: a rotation-changed push (another window's copy)
// can land while this id's initial rotation_peek is still in flight. Without
// this, the peek's reply arrives after and overwrites the fresher pushed
// position with stale data (same shape as floating.js's rotateReq / folder.js's
// rotateReqs).
const rotateReqs = new Map(); // id -> latest request number
let ROTATE_SHORT_MAX = 60; // tooltip preview length, independent of the notes cap, expert-tunable

// `pos` (not `index + 1`) — in random mode `index` is the absolute entry that
// will be drawn next (jumps around), while `pos` is the 1-based draw number within
// the current shuffle cycle, counting up and restarting at the next reshuffle. In
// order mode the two coincide (the backend already returns pos = index + 1 there).
function rotateBadgeLabel(id) {
  const s = rotateState.get(id);
  return s ? `${s.pos}/${s.total}` : "";
}

// Pushes the cached state (however stale/empty) into one tile's badge + tooltip.
function applyRotateState(tile, id) {
  const badge = tile.querySelector(".tile-rotate-badge");
  if (badge) badge.textContent = rotateBadgeLabel(id);
  const s = rotateState.get(id);
  if (s && flag("tilePreview")) {
    // 0 = show the whole entry, same convention as notesTipLen/clipNote.
    const short = ROTATE_SHORT_MAX > 0 && s.text.length > ROTATE_SHORT_MAX
      ? `${s.text.slice(0, ROTATE_SHORT_MAX)}…` : s.text;
    tile.dataset.tipRotate = t("tileTipRotate").replace("{text}", short);
  }
}

// Ids with a live auto-advance session, kept by the backend's rotation-auto
// event (fired on arm/end from clipwatch — see mod clipwatch in lib.rs, 91).
const autoActiveIds = new Set();

function applyAutoBadge(tile, id) {
  const badge = tile.querySelector(".tile-rotate-badge");
  if (!badge) return;
  const active = autoActiveIds.has(id);
  badge.classList.toggle("is-auto", active);
  if (active) badge.title = t("rotateAutoActive");
  else badge.removeAttribute("title");
}

function primeRotateState(id) {
  if (rotateState.has(id)) return;
  const req = (rotateReqs.get(id) || 0) + 1;
  rotateReqs.set(id, req);
  invoke("rotation_peek", { id }).then((r) => {
    if (!r || rotateReqs.get(id) !== req) return;
    rotateState.set(id, r);
    document.querySelectorAll(`.tile[data-id="${CSS.escape(id)}"]`).forEach((tile) => applyRotateState(tile, id));
  }).catch(() => {});
}

// ---- Stored images, fetched one prompt at a time ----
// get_state hands the window `has_image` + `image_rev` instead of the picture: a
// stored image is a base64 PNG of up to a few hundred KB, and sending every one of
// them through IPC on each render was by far the most expensive thing the app did.
// A surface that wants to draw one asks here; until the bytes arrive the tile keeps
// its plain name/colour face, so nothing ever flashes an empty frame.
const imgCache = new Map(); // id -> data URL; insertion order = least recently used first
const imgCacheRev = new Map(); // id -> the image_rev the cached data URL belongs to
const imgPending = new Set(); // ids with a prompt_image call already in flight
// Several screens' worth of image tiles: enough that scrolling the library or
// hopping between views never refetches, low enough that the cache stays a few tens
// of MB even when every entry is a full-size screenshot.
const IMG_CACHE_MAX = 80;

// True for both shapes a prompt arrives in — the slim one from get_state and the
// full one from get_prompt/list_trash, which still carries its image inline.
function hasImage(p) {
  return !!p && (p.has_image === true || !!p.image);
}

// The prompt's stored image, or "" while it is still on its way (or absent).
function imageOf(p) {
  if (!p) return "";
  if (p.image) return p.image; // a full prompt brought its own
  if (!p.has_image) return ""; // nothing to ask for — never spend a call on it
  const id = p.id;
  const rev = p.image_rev || 0;
  if (imgCache.has(id)) {
    if (imgCacheRev.get(id) === rev) {
      const data = imgCache.get(id);
      imgCache.delete(id); // re-insert, so the eviction order stays oldest-access-first
      imgCache.set(id, data);
      return data;
    }
    imgCache.delete(id); // the picture changed under us
    imgCacheRev.delete(id);
  }
  if (!imgPending.has(id)) {
    imgPending.add(id);
    invoke("prompt_image", { id })
      .then((data) => {
        imgPending.delete(id);
        // An empty answer is cached too: without that, a prompt whose image went
        // away between the two calls would be asked for again on every render.
        imgCache.set(id, data || "");
        imgCacheRev.set(id, rev);
        while (imgCache.size > IMG_CACHE_MAX) {
          const oldest = imgCache.keys().next().value;
          imgCache.delete(oldest);
          imgCacheRev.delete(oldest);
        }
        if (data) scheduleImagePaint();
      })
      .catch(() => { imgPending.delete(id); });
  }
  return "";
}

// Anything drawn while its picture was still loading tags itself with the id it is
// waiting for, and only those elements are rebuilt when the bytes land. Re-rendering
// the whole grid instead would tear down whatever else is going on at that moment —
// a live colour preview, a half-finished drag — for a background arrival.
const IMG_WAIT = "imgWait"; // dataset key: the prompt id this element is waiting for

// One repaint for a whole batch of arrivals instead of one per image.
let imagePaintQueued = false;
function scheduleImagePaint() {
  if (imagePaintQueued) return;
  imagePaintQueued = true;
  setTimeout(() => {
    imagePaintQueued = false;
    if (drag) return; // a drag owns the tiles it moves; its end re-renders anyway
    for (const el of document.querySelectorAll(`[data-img-wait]`)) {
      const p = prompts.find((x) => x.id === el.dataset[IMG_WAIT]);
      if (!p || !imageOf(p)) continue;
      if (el.classList.contains("tile")) {
        el.replaceWith(buildTile(p));
      } else {
        // A library / copy-history row preview: its panel owns the element cache,
        // so drop the placeholder entry before asking for a fresh thumbnail.
        const cache = el.closest("#journal") ? journalThumbCache : libThumbCache;
        cache.delete(p.id);
        el.replaceWith(thumbEl(p, cache));
      }
    }
    fitAllTiles();
  }, 0);
}

function buildTile(p) {
  const tile = document.createElement("div");
  tile.className = "tile";
  tile.dataset.id = p.id;
  if (gridSelected.has(p.id)) tile.classList.add("grid-selected");
  // A tag filter dims what does not match instead of hiding it — the cell
  // stays occupied, so drag/reorder/grid geometry never has to treat it as empty.
  if (gridTags.size && !tagMatches(p, gridTags)) tile.classList.add("tile-dimmed");
  // Bulk-selection checkbox. Its own pointerdown/click stop propagation so a
  // press here can never arm the tile's reorder drag or start a copy.
  const selBtn = document.createElement("button");
  selBtn.type = "button";
  selBtn.className = "tile-select";
  selBtn.title = t("selectPrompt");
  selBtn.setAttribute("aria-label", t("selectPrompt"));
  selBtn.addEventListener("pointerdown", (e) => e.stopPropagation());
  selBtn.addEventListener("click", (e) => { e.stopPropagation(); toggleGridSelect(p.id); });
  tile.appendChild(selBtn);
  // A second small button right of the checkbox starts a chain with THIS
  // tile as entry 1 — only text prompts can ever be a chain link, so image/
  // file/video tiles never show it (a symbol there would be a false promise).
  // Also skipped outright with chaining off, not just left inert --
  // .noflag-chainPrompts/.noflag-chainQuickStart below back this up in CSS.
  if (isTextPrompt(p) && flag("chainPrompts") && flag("chainQuickStart")) {
    const chainBtn = document.createElement("button");
    chainBtn.type = "button";
    chainBtn.className = "tile-chain-start";
    chainBtn.title = t("startChainHere");
    chainBtn.setAttribute("aria-label", t("startChainHere"));
    chainBtn.innerHTML = '<svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true">'
      + '<path transform="rotate(-45 12 12)" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" d="M9 17H7A5 5 0 0 1 7 7h2M15 7h2a5 5 0 0 1 0 10h-2M8 12h1.6M14.4 12H16"/></svg>';
    chainBtn.addEventListener("pointerdown", (e) => e.stopPropagation());
    chainBtn.addEventListener("click", (e) => { e.stopPropagation(); startChainWith(p); });
    tile.appendChild(chainBtn);
  }
  const raw = p.file_path || p.text || "";
  const preview = PREVIEW_MAX <= 0 ? "" : raw.length > PREVIEW_MAX ? `${raw.slice(0, PREVIEW_MAX)}…` : raw;
  // Themed hover tooltip (rendered by wireTooltips' shared popup, not the native
  // title): stash name + preview on the tile; the popup wraps + grows tall so long
  // prompts read comfortably at a constrained width.
  if (flag("tilePreview")) {
    tile.dataset.tipName = p.name;
    if (preview && preview !== p.name) tile.dataset.tipBody = preview;
    // Two prompts of wildly different length look identical on the tile itself —
    // the same char/~token readout the editor already shows, reused here so there is
    // only one formula for "how long is this" in the whole app.
    if (p.text) {
      const chars = [...p.text].length;
      const tokens = Math.ceil(chars / 4);
      const nf = (n) => n.toLocaleString(LANG);
      tile.dataset.tipLen = `${nf(chars)} ${t("lenChars")} · ~${nf(tokens)} ${t("lenTokens")}`;
    }
    // The private note rides along as its own dimmed section (opt-out flag).
    const note = notesOn() && flag("notesInTooltip") ? (p.notes || "").trim() : "";
    if (note) tile.dataset.tipNotes = clipNote(note);
    const times = tileTimeLines(p.id);
    if (times) tile.dataset.tipTimes = times;
  }

  // Media on the tile: stored still, or gif/video from the icon/file path.
  const kind = p.file_path ? mediaKind(p.file_path) : "";
  // `has` (known from the prompt list) decides the LAYOUT, `stored` (the bytes,
  // which may still be loading) decides whether the image can be drawn yet — so a
  // tile waiting for its picture falls back to the name/colour face below instead
  // of showing an empty frame or, worse, an unrelated icon-path preview.
  const has = hasImage(p);
  const stored = has ? imageOf(p) : "";
  const iconKind = !has && p.icon_path ? mediaKind(p.icon_path) : "";
  const showPath = iconKind ? p.icon_path : p.file_path;
  const showKind = has ? "" : iconKind || kind;
  const pathVideo = p.show_image && !has && showKind === "video";
  const pathGif = p.show_image && !has && showKind === "gif";
  if (has && !stored) tile.dataset[IMG_WAIT] = p.id; // redrawn once the bytes land
  if (p.show_image && (stored || pathVideo || pathGif)) {
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
      img.src = stored || convertFileSrc(showPath);
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

  // Rotating prompt — "next entry" position pill, bottom-left (a rotating
  // prompt is always text-only, so that corner never competes with a media
  // caption/control bar). Text fills in from the cache once primed below.
  // rotateBadge is a sub-toggle of rotatingPrompts (gating rule: nothing
  // dependent stays visible/active with its parent off) — flag() alone ignores
  // the parent, gateOpen() walks the declared chain.
  const rotateUiOn = flag("rotateBadge") && gateOpen(FLAG_GATES.rotateBadge);
  if ((p.variants || []).length && rotateUiOn) {
    const rbadge = document.createElement("span");
    rbadge.className = "tile-rotate-badge";
    tile.appendChild(rbadge);
    applyAutoBadge(tile, p.id); // survives a re-render mid-session
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
    if (e.button !== 0 || e.target.closest(".tile-menu") || e.target.closest(".tile-select")) return;
    // With the modifier held the press becomes a drag-out; `out` keeps the
    // reorder machine out of it while a modifier-click still copies as before.
    const out = armDragOut(tile, e, () => prompts.find((x) => x.id === p.id));
    drag = { id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: tile, out };
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
  // Gated the same as the badge element above — an unconditional peek here was
  // firing one rotation_peek IPC call per rotating tile even with the badge (or its
  // whole parent feature) switched off.
  if ((p.variants || []).length && rotateUiOn) {
    primeRotateState(p.id); // async — patches this tile again once the peek lands
    applyRotateState(tile, p.id); // sync — reflects whatever is cached right now
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
      labels: mediaBarLabels(),
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

// Library/journal thumbnails used to get a fresh <img> with a reassigned
// base64 src on EVERY render (search re-renders the whole row list on every
// keystroke), forcing the browser to re-decode the same blob each time. One cache
// per panel — library and journal can both stay mounted at once (see the video
// IntersectionObserver comment above), so a single shared cache would rip a thumb
// out of whichever panel isn't rendering right now.
const libThumbCache = new Map(); // prompt id -> { key, el }
const journalThumbCache = new Map();
function pruneThumbCache(cache) {
  for (const id of cache.keys()) {
    if (!prompts.some((p) => p.id === id)) cache.delete(id);
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

// ---- Missing-file watcher ----
// The ambient every-few-seconds refresh is a backend-pushed "missing-files"
// event now (listened for below), not this window's own timer - see
// start_missing_files_poll in lib.rs. pollMissingFiles stays for the one-off
// "recheck right now" calls (initial load, right after an edit).
let missingFiles = new Set();

function applyMissingFiles(ids) {
  missingFiles = new Set(ids);
  document.querySelectorAll(".tile").forEach((tile) => {
    tile.classList.toggle("file-missing", missingFiles.has(tile.dataset.id));
  });
}

async function pollMissingFiles() {
  if (!prompts.some((p) => p.file_path || p.icon_path)) {
    missingFiles.clear();
    return;
  }
  try {
    applyMissingFiles(await invoke("missing_files"));
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

// ---- drag a prompt out into another app ----
// The webview's own drag-drop handling only replaces the drop TARGET (that is how a
// file dropped onto the window becomes a prompt); starting a drag is untouched, so a
// plain HTML5 dragstart is all it takes to hand the prompt to the OS.
// A modifier is required everywhere because every drag source already owns its plain
// drag — tiles reorder, library rows get placed on the grid, the pill moves its window.
const dragOutMod = () => DRAG_OUT_MODS[txt("dragOutModifier")] || "ctrlKey";
const dragOutWired = new WeakSet();

// A rotating prompt's peeked "next entry" text, prefetched on arm (see
// armDragOut) so dragOutText can stay synchronous — dataTransfer.setData only works
// inside the dragstart handler itself, too late to await a peek there.
const dragRotationPeek = new Map(); // prompt id -> peeked text

// What lands in the drop target. Placeholders stay as written: resolving them needs
// the fill-in dialog, and no dialog can open while the OS drag loop holds the pointer.
function dragOutText(p) {
  if (p.file_path) return p.file_path;
  if ((p.variants || []).length) {
    const peeked = dragRotationPeek.get(p.id);
    if (peeked != null) return peeked;
  }
  return p.text || "";
}

// Called on pointerdown. Returns true when this press should become a drag-out;
// the caller then leaves the plain drag alone.
function armDragOut(el, e, getPrompt) {
  // XOR: with dragOutInvert on, the plain drag is the one that leaves the app and
  // the modifier is what still moves the tile inside the grid.
  const on = optFlag("dragOut") && (e[dragOutMod()] === true) !== flag("dragOutInvert");
  el.draggable = on;
  if (on) {
    const p = getPrompt();
    // Peek is read-only — safe to fire on every arm, even one that never becomes a
    // real drag. Almost always resolved well before dragstart (a real drag needs
    // pointer motion past the threshold first).
    if (p && (p.variants || []).length) {
      invoke("rotation_peek", { id: p.id }).then((r) => { if (r) dragRotationPeek.set(p.id, r.text); }).catch(() => {});
    }
  }
  if (on && !dragOutWired.has(el)) {
    dragOutWired.add(el);
    el.addEventListener("dragstart", (ev) => startDragOut(ev, getPrompt()));
    el.addEventListener("dragend", (ev) => {
      el.draggable = false;
      const p = getPrompt();
      // Only an actually-accepted drop of a text we actually peeked may consume
      // an entry — a cancelled drag (dropped nowhere, or on a target that refused
      // it) reports "none", and a peek that never resolved already fell back to
      // p.text in dragOutText above (committing then would advance whatever the
      // pointer's REAL position is, not the entry that actually got dragged out).
      if (p && (p.variants || []).length) {
        if (ev.dataTransfer.dropEffect !== "none" && dragRotationPeek.has(p.id)) {
          invoke("rotation_commit", { id: p.id }).catch(() => {});
        }
        dragRotationPeek.delete(p.id);
      }
    });
  }
  return on;
}

function startDragOut(e, p) {
  const text = p ? dragOutText(p) : "";
  if (!optFlag("dragOut") || !text) { e.preventDefault(); return; }
  e.dataTransfer.effectAllowed = "copy";
  e.dataTransfer.setData("text/plain", text);
  if (p.file_path) e.dataTransfer.setData("text/uri-list", fileUri(p.file_path));
  // The OS owns the gesture from here — drop the pointer-drag bookkeeping so the
  // ghost/hover cell of a reorder can never be left behind.
  drag = null;
  endDragVisuals();
}

window.addEventListener("pointermove", (e) => {
  if (!drag || drag.out) return;
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
    // Dragging out of the library: hide the overlay so the grid is visible — and
    // anything the library was raised over, or the grid stays covered.
    if (drag.fromLibrary) {
      libraryEl.classList.add("hidden");
      dropLibraryFront(true);
    }
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
  armFolderDrop(e); // Hovering the middle of a tile offers a folder
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
  const { id, moved, el, ghost, folderId } = drag;
  drag = null;
  ghost?.remove();
  el.classList.remove("dragging");
  resumeDragVideos(el);
  endDragVisuals();
  // What an armed centre would swallow, decided before the arm is dropped.
  const armed = folderArm && folderArm.armed ? folderArm.el : null;
  disarmFolder();

  // A folder tile: a click opens it, a drag moves it to another cell.
  if (folderId) {
    const f = folderById(folderId);
    if (!f) return;
    if (!moved) { openTileFolderPop(f, el); return; }
    const cell = cellAt(e.clientX, e.clientY);
    if (!cell) return;
    const target = [Number(cell.dataset.col), Number(cell.dataset.row)];
    // Onto an occupied cell the folder simply stays put — a folder never swaps a
    // tile away, that is what dropping INTO it is for.
    if (cell.firstElementChild && cell.firstElementChild !== el) return;
    if (await saveFolder(f, { cell: target })) await renderGrid(true);
    return;
  }

  if (!moved) {
    if (el.classList.contains("tile")) {
      const p = prompts.find((x) => x.id === id);
      if (chainMode) { if (p) await toggleChainPick(p); return; } // pick (asks vars), don't copy
      // A prompt that still needs input ALWAYS routes through the dialog — the
      // double-click paste shortcut would otherwise fire on the 2nd click and paste
      // stale clipboard without ever asking. A click only copies, never pastes.
      // Check the entry rotation is actually about to copy, not always entry 0.
      if (p && dialogTokens(p, await peekRotatedText(p)).length) {
        await copyTextWithVars(p, el);
        return;
      }
      if (autoPasteDouble(id)) { await autoPasteNow(el); return; } // 2nd fast click pastes
      // A click swallowed by the cooldown used to give no feedback at all —
      // a quick shake says "that didn't do anything" instead of silence.
      if (copyOnCooldown(id)) {
        el.classList.add("cooldown-blocked");
        setTimeout(() => el.classList.remove("cooldown-blocked"), 250);
        return;
      }
      if (p && await copyResolved(p)) {
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
  if (armed) { await dropIntoFolder(id, armed, cell); return; }
  // Only a genuine reorder of an already-placed tile gets an undo offer — a
  // brand-new placement (drag in from the library, double-click-create) has
  // nothing to move back FROM, so prevCell stays undefined and the toast is skipped.
  const prevCell = layoutOf(activeView())[id];
  const destCol = Number(cell.dataset.col);
  const destRow = Number(cell.dataset.row);
  await placeTile(id, destCol, destRow);
  if (prevCell && (prevCell[0] !== destCol || prevCell[1] !== destRow)) {
    const [fromCol, fromRow] = prevCell;
    toast(t("tileMoved"), {
      label: t("undo"),
      icon: ICON_UNDO,
      onClick: () => placeTile(id, fromCol, fromRow),
    });
  }
});

window.addEventListener("pointercancel", () => {
  if (drag) {
    drag.el.classList.remove("dragging");
    drag.ghost?.remove();
    resumeDragVideos(drag.el);
  }
  drag = null;
  disarmFolder();
  if (memberDrag) {
    memberDrag.el.classList.remove("dragging");
    memberDrag.ghost?.remove();
  }
  memberDrag = null;
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

// Named (not the pill/folder windows' shared COPIED_MS=900 -- this tile flash
// has always run shorter; unifying the value would be a behaviour change).
const TILE_COPIED_MS = 350;
// Flash + small "Copied!" bubble at the bottom of the tile.
function showCopied(tile, label) {
  tile.classList.add("copied");
  setTimeout(() => tile.classList.remove("copied"), TILE_COPIED_MS);
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

// ---- Auto-paste: a fast second activation of the SAME prompt pastes the
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
  // Off by default — placing a prompt here drops it from every other view it
  // sits in first, keeping the historical "one placement" guarantee. On, a prompt
  // can sit in several views at once (same object, same data, everywhere).
  if (!optFlag("multiViewPrompt")) {
    for (const other of viewsContaining(id, view.id)) {
      delete layoutOf(other)[id];
      invoke("batch_prompts", { ids: [id], action: "removeView", color: null, favorite: null, viewId: other.id }).catch(() => {});
    }
  }
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
  invoke("set_layout", { layout }).catch((e) => errToast(e));
  if (!moveTileDom(id, col, row)) await renderGrid(true);
}

// Double-click an empty cell to create a prompt right there, instead of typing
// below and dragging it over. `.cell` has no children while nothing occupies it
// (renderGrid only ever appends a tile or a folder tile to a filled one).
gridEl.addEventListener("dblclick", (e) => {
  if (favView) return; // free-form favorites layout has its own placement rules
  const cell = e.target.closest(".cell");
  if (!cell || cell.firstElementChild) return;
  openModal({
    mode: "create",
    title: t("nameModalTitle"),
    targetCell: [Number(cell.dataset.col), Number(cell.dataset.row)],
  });
});

// ---- Grid folders: several prompts collapsed into one cell ----
// Ships ON. A member has no layout entry of its own while it sits in a folder, so
// switching the feature off simply shows every prompt as its own tile again — the
// folders stay stored and come back with it.
const tileFoldersOn = () => flag("tileFolders");
const tileFolderList = () => (tileFoldersOn() ? settings.tile_folders || [] : []);
const foldersOfView = (viewId) => tileFolderList().filter((f) => f.view_id === viewId);
const folderById = (id) => tileFolderList().find((f) => f.id === id);
const folderMembers = (f) =>
  (f.members || []).map((id) => prompts.find((p) => p.id === id)).filter(Boolean);
const folderTitle = (f) => f.name || t("tileFolderDefaultName");

// One write per change — name, colour, cell and members always travel together, so
// the grid never shows a half-moved folder.
async function saveFolder(f, patch) {
  try {
    settings = await invoke("save_tile_folder", {
      id: f.id || "",
      name: patch.name ?? f.name ?? "",
      color: patch.color ?? f.color ?? "",
      viewId: f.view_id,
      cell: patch.cell ?? f.cell ?? [0, 0],
      members: patch.members ?? f.members ?? [],
    });
  } catch (e) {
    errToast(e);
    return false;
  }
  return true;
}

// Two tiles dropped on each other become a folder on the target's cell.
async function newTileFolder(targetId, dropId, cell) {
  const name = await confirmDialog({
    title: t("tileFolderNew"),
    message: t("tileFolderNameMsg"),
    confirmLabel: t("save"),
    input: { type: "text", value: t("tileFolderDefaultName"), placeholder: t("tileFolderNamePh") },
    danger: false,
  });
  if (name === null) return; // cancelled: nothing moves, the tiles stay as they were
  const view = activeView();
  const ok = await saveFolder(
    { id: "", view_id: view.id },
    { name: String(name).trim(), cell, members: [targetId, dropId] }
  );
  if (ok) await renderGrid(true);
}

async function renameTileFolder(f) {
  const name = await confirmDialog({
    title: t("tileFolderRename"),
    message: t("tileFolderNameMsg"),
    confirmLabel: t("save"),
    input: { type: "text", value: f.name, placeholder: t("tileFolderNamePh") },
    danger: false,
  });
  if (name === null || String(name).trim() === f.name) return;
  if (await saveFolder(f, { name: String(name).trim() })) await renderGrid(true);
}

let folderColorTimer = null;
function pickTileFolderColor(f, anchor) {
  openColorPop(anchor, f.color || "", (hex) => {
    const live = folderById(f.id);
    if (!live) return;
    live.color = hex;
    // The free picker's SV field fires this on every drag frame — a full
    // renderGrid per frame would rebuild every tile in the view. Paint just
    // this one folder tile live, the same way buildFolderTile itself does.
    const tile = gridEl.querySelector(`.folder-tile[data-folder-id="${f.id}"]`);
    if (tile) {
      tile.classList.toggle("tinted", !!hex);
      tile.style.background = hex;
      tile.style.borderColor = hex;
    }
    clearTimeout(folderColorTimer);
    // Re-read the folder on write: the settings object was replaced in between.
    folderColorTimer = setTimeout(() => {
      const now = folderById(f.id);
      if (now) saveFolder(now, { color: hex });
    }, 300);
  });
}

async function dissolveTileFolder(f) {
  const ok = await confirmDialog({
    title: t("tileFolderDissolve"),
    message: t("tileFolderDissolveMsg").replace("{name}", folderTitle(f)),
    confirmLabel: t("tileFolderDissolve"),
  });
  if (!ok) return;
  try {
    settings = await invoke("delete_tile_folder", { id: f.id });
  } catch (e) {
    errToast(e);
    return;
  }
  closeTileFolderPop();
  await renderGrid(true);
}

// Take one member out and put it on `cell`. The second-to-last one leaving dissolves
// the folder — the backend's invariant, so there is no special case here.
async function takeOutOfFolder(id, cell) {
  try {
    settings = await invoke("take_out_of_tile_folder", { id, cell });
  } catch (e) {
    errToast(e);
    return;
  }
  closeTileFolderPop();
  await renderGrid(true);
}

// A folder cell: up to four member faces, the name and the count.
function buildFolderTile(f) {
  const tile = document.createElement("div");
  tile.className = "tile folder-tile";
  tile.dataset.folderId = f.id;
  if (f.color) {
    tile.classList.add("tinted");
    tile.style.background = f.color;
    tile.style.borderColor = f.color;
  }
  const mini = document.createElement("span");
  mini.className = "folder-mini";
  const members = folderMembers(f);
  for (const p of members.slice(0, 4)) {
    const face = document.createElement("span");
    face.className = "folder-mini-cell";
    if (p.color) face.style.background = p.color;
    const faceImg = imageOf(p);
    if (faceImg) {
      const img = document.createElement("img");
      img.src = faceImg;
      img.draggable = false;
      face.appendChild(img);
    } else {
      face.textContent = (p.name || "").slice(0, 2);
    }
    mini.appendChild(face);
  }
  tile.appendChild(mini);
  const name = document.createElement("span");
  // NOT .tile-name: that class is what the auto-fitter scales, and a folder label
  // blown up to prompt size is what made these tiles unreadable.
  name.className = "folder-title";
  name.textContent = folderTitle(f);
  tile.appendChild(name);
  if (flag("tilePreview")) {
    tile.dataset.tipName = folderTitle(f);
    tile.dataset.tipBody = members.map((p) => p.name).join(", ");
  }
  tile.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    openMiniMenu([
      { label: t("tileFolderRename"), run: () => renameTileFolder(f) },
      { label: t("tileFolderColor"), run: () => pickTileFolderColor(f, tile) },
      { label: t("tileFolderDissolve"), danger: true, run: () => dissolveTileFolder(f) },
    ], e.clientX, e.clientY);
  });
  // Same drag machine as a tile — only what a press means differs (open, not copy).
  tile.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    drag = { folderId: f.id, startX: e.clientX, startY: e.clientY, moved: false, el: tile };
  });
  return tile;
}

// ---- Folder popover: the members, opened by a click on the folder ----
const tileFolderPop = document.createElement("div");
tileFolderPop.className = "ctx tile-folder-pop hidden";
tileFolderPop.setAttribute("role", "menu");
document.body.appendChild(tileFolderPop);
let tileFolderPopId = null;

function closeTileFolderPop() {
  tileFolderPop.classList.add("hidden");
  tileFolderPopId = null;
}

function openTileFolderPop(f, anchor) {
  if (tileFolderPopId === f.id) { closeTileFolderPop(); return; }
  tileFolderPop.innerHTML = "";
  tileFolderPopId = f.id;
  const head = document.createElement("div");
  head.className = "tile-folder-pop-head";
  head.textContent = folderTitle(f);
  tileFolderPop.appendChild(head);
  const grid = document.createElement("div");
  grid.className = "tile-folder-grid";
  grid.style.gridTemplateColumns = `repeat(${val("tileFolderCols")}, 1fr)`;
  for (const p of folderMembers(f)) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "folder-member-tile";
    btn.dataset.id = p.id;
    btn.title = p.name;
    // Same content as a real grid tile (buildTile) — image/gif thumb, type
    // badge, tinted background — so a folder never shows less than the grid
    // underneath it. Interactive bits (select checkbox, tile menu, video player)
    // stay out: this button already has its own click/drag/context-menu handling.
    const kind = p.file_path ? mediaKind(p.file_path) : "";
    const has = hasImage(p);
    const stored = has ? imageOf(p) : "";
    const iconKind = !has && p.icon_path ? mediaKind(p.icon_path) : "";
    const showPath = iconKind ? p.icon_path : p.file_path;
    const showKind = has ? "" : iconKind || kind;
    const showable = p.show_image && (stored || showKind === "image" || showKind === "gif");
    if (showable) {
      btn.classList.add("has-image");
      if (p.color) { btn.style.background = p.color; btn.style.borderColor = p.color; }
      const img = document.createElement("img");
      img.className = "tile-img";
      img.src = stored || convertFileSrc(showPath);
      img.draggable = false;
      btn.appendChild(img);
    } else if (p.color) {
      btn.style.background = p.color;
      btn.style.borderColor = p.color;
    }
    const label = document.createElement("span");
    label.className = "tile-name";
    label.textContent = p.name;
    btn.appendChild(label);
    const typeIcon = p.file_path
      ? (kind === "video" ? ICON_VIDEO : kind ? ICON_IMAGE : PDF_EXT.test(p.file_path) ? ICON_PDF : ICON_FILE)
      : p.copy_image ? ICON_IMAGE : "";
    if (typeIcon && flag("typeBadges")) {
      const badge = document.createElement("span");
      badge.className = "tile-type";
      badge.innerHTML = typeIcon;
      btn.appendChild(badge);
    }
    btn.addEventListener("contextmenu", (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      openCtx(p.id, ev.clientX, ev.clientY);
    });
    // Press and pull onto a cell to take the prompt out of the folder again.
    btn.addEventListener("pointerdown", (ev) => {
      if (ev.button !== 0) return;
      memberDrag = { id: p.id, startX: ev.clientX, startY: ev.clientY, moved: false, el: btn };
    });
    grid.appendChild(btn);
  }
  tileFolderPop.appendChild(grid);
  placePopover(tileFolderPop, anchor.getBoundingClientRect());
  // Cells now size themselves from tileFolderCols + the popover width (CSS
  // grid + aspect-ratio), so the fit box is measured per tile instead of a
  // constant — 1px border + 4px padding each side, same auto-shrink as the grid.
  for (const btn of grid.children) {
    const r = btn.getBoundingClientRect();
    // Floor, not the raw fractional rect: scrollWidth/Height are integers, so a
    // fractional maxW like 76.65 can fail its own <= check against a rounded 77.
    fitTileText(btn, Math.floor(r.width - 10), Math.floor(r.height - 10));
  }
}

// Dragging a member out of the open popover.
let memberDrag = null;
window.addEventListener("pointermove", (e) => {
  if (!memberDrag) return;
  if (!memberDrag.moved) {
    if (Math.abs(e.clientX - memberDrag.startX) < DRAG_THRESHOLD &&
        Math.abs(e.clientY - memberDrag.startY) < DRAG_THRESHOLD) return;
    memberDrag.moved = true;
    memberDrag.el.classList.add("dragging");
    document.body.classList.add("drag-active");
    // A live ghost under the cursor, exactly as a tile drag has: pulling a member out
    // of the folder showed nothing moving before, so the gesture read as "not working".
    const r = memberDrag.el.getBoundingClientRect();
    memberDrag.offX = memberDrag.startX - r.left;
    memberDrag.offY = memberDrag.startY - r.top;
    const ghost = memberDrag.el.cloneNode(true);
    ghost.classList.add("drag-ghost");
    ghost.classList.remove("dragging");
    ghost.style.width = `${r.width}px`;
    ghost.style.height = `${r.height}px`;
    document.body.appendChild(ghost);
    memberDrag.ghost = ghost;
  }
  memberDrag.ghost.style.transform =
    `translate(${e.clientX - memberDrag.offX}px, ${e.clientY - memberDrag.offY}px)`;
  setHoverCell(cellAt(e.clientX, e.clientY));
});
window.addEventListener("pointerup", async (e) => {
  if (!memberDrag) return;
  const d = memberDrag;
  memberDrag = null;
  d.ghost?.remove();
  d.el.classList.remove("dragging");
  endDragVisuals();
  const p = prompts.find((x) => x.id === d.id);
  if (!d.moved) {
    // A plain click copies, exactly like the tile it stands for.
    if (!p) return;
    if (dialogTokens(p, await peekRotatedText(p)).length) { await copyTextWithVars(p, d.el); return; }
    if (copyOnCooldown(d.id)) return;
    if (await copyResolved(p)) {
      showCopied(d.el);
      recordCopy(d.id);
      maybeCloseOnCopy();
    }
    return;
  }
  const cell = cellAt(e.clientX, e.clientY);
  if (!cell) return;
  await takeOutOfFolder(d.id, [Number(cell.dataset.col), Number(cell.dataset.row)]);
});

// ---- Arming a folder while a tile is dragged ----
// The OUTER band of a tile keeps the plain swap; the middle makes a folder, but only
// after a short dwell, so a quick swap can never turn into one by accident.
let folderArm = null;
function disarmFolder() {
  if (!folderArm) return;
  clearTimeout(folderArm.timer);
  folderArm.el.classList.remove("folder-arm", "folder-arming");
  folderArm.el.style.removeProperty("--arm-ms");
  folderArm = null;
}
function armFolderDrop(e) {
  if (!tileFoldersOn() || favView || !drag || drag.folderId) return disarmFolder();
  const host = cellAt(e.clientX, e.clientY)?.firstElementChild;
  if (!host || host === drag.el) return disarmFolder();
  const r = host.getBoundingClientRect();
  const zone = Math.min(90, Math.max(10, val("tileFolderZone"))) / 100;
  const inside =
    Math.abs(e.clientX - (r.left + r.width / 2)) <= (r.width * zone) / 2 &&
    Math.abs(e.clientY - (r.top + r.height / 2)) <= (r.height * zone) / 2;
  if (!inside) return disarmFolder();
  if (folderArm && folderArm.el === host) return;
  disarmFolder();
  const arm = { el: host, armed: false, timer: 0 };
  const wait = Math.max(0, val("tileFolderHoverMs"));
  // Show the wait instead of making the user guess it: a ring fills over exactly the
  // dwell time, then the target flips to the armed scrim. Without it the folder
  // appeared out of nothing and a drag that was only passing through looked broken.
  host.style.setProperty("--arm-ms", `${wait}ms`);
  host.classList.add("folder-arming");
  arm.timer = setTimeout(() => {
    host.classList.remove("folder-arming");
    host.classList.add("folder-arm");
    arm.armed = true;
  }, wait);
  folderArm = arm;
}

// A drop in an armed centre: join that folder, or make one out of the two tiles.
async function dropIntoFolder(id, host, cell) {
  const f = host.dataset.folderId ? folderById(host.dataset.folderId) : null;
  if (f) {
    if (f.members.includes(id)) return;
    if (await saveFolder(f, { members: [...f.members, id] })) await renderGrid(true);
    return;
  }
  if (!host.dataset.id) return;
  await newTileFolder(host.dataset.id, id, [Number(cell.dataset.col), Number(cell.dataset.row)]);
}

// `settings.floating` maps every pinned prompt to its pill position, and the backend
// treats exactly that entry as "pinned". The grid only ever asks whether an entry
// exists, so after a pin/unpin the cached copy is brought in line straight away from
// what the backend reports — waiting for the next full state fetch would leave the
// context menu offering the opposite of what a click actually does. The placeholder
// position is replaced by the real one with that next fetch; it is never sent back.
function cacheFloatingState(id, pinned) {
  if (!settings.floating) settings.floating = {};
  if (pinned) settings.floating[id] = settings.floating[id] || { x: 0, y: 0 };
  else delete settings.floating[id];
}

// ---- Context menu ----
function openCtx(id, x, y) {
  ctxId = id;
  // Reset an armed delete confirmation from a previous open.
  disarmButton(ctxEl.querySelector('[data-act="delete"]'), t("delete"));
  // Hide the floating-button action when that feature is switched off; otherwise
  // name the actual effect of a click instead of a state-less "toggle".
  const pinBtn = ctxEl.querySelector('[data-act="pin"]');
  pinBtn.classList.toggle("hidden", !flag("floating"));
  const pinned = !!settings.floating?.[id];
  pinBtn.textContent = t(pinned ? "removeFloating" : "showFloating");
  // In the favorites view a prompt can't be hidden/removed — unstar it instead.
  ctxEl.querySelector('[data-act="hide"]').classList.toggle("hidden", favView);
  // Nothing to place this prompt into besides the view it is already on.
  ctxEl.querySelector('[data-act="views"]').classList.toggle("hidden", !optFlag("multiViewPrompt") || settings.views.length < 2);
  // Only visible when this prompt actually rotates and both auto gates are on —
  // grey-and-disabled would still be a control the user cannot act on.
  const rmenuBtn = ctxEl.querySelector('[data-act="rotateAutoMenu"]');
  const rp = prompts.find((x) => x.id === id);
  ctxEl.querySelector('[data-act="editVideo"]').classList.toggle("hidden", !(flag("videoEditor") && rp?.file_path && mediaKind(rp.file_path) === "video"));
  const rotateMenuEligible = !!rp && (rp.variants || []).length > 0 && flag("rotatingPrompts") && flag("rotateAuto");
  rmenuBtn.classList.toggle("hidden", !rotateMenuEligible);
  if (rotateMenuEligible) {
    const on = !!rp.rotate_auto;
    rmenuBtn.setAttribute("aria-checked", String(on));
    rmenuBtn.classList.toggle("checked", on);
    rmenuBtn.querySelector(".tb-check").innerHTML = on ? TB_CHECK : "";
  }
  placePopoverAt(ctxEl, x, y);
}
function closeCtx() {
  ctxEl.classList.add("hidden");
  ctxId = null;
}

// ---- "also show in" popover — place/unplace this prompt on other views ----
const viewsPop = document.createElement("div");
viewsPop.className = "ctx views-pop hidden";
viewsPop.setAttribute("role", "menu");
document.body.appendChild(viewsPop);
function closeViewsPop() {
  viewsPop.classList.add("hidden");
}
function renderViewsPop(id) {
  viewsPop.innerHTML = "";
  for (const v of settings.views) {
    if (v.id === settings.active_view) continue; // already here — that's why the menu was opened
    const on = Object.prototype.hasOwnProperty.call(layoutOf(v), id);
    const row = viewRowBase(v, on);
    row.setAttribute("role", "menuitemcheckbox");
    row.setAttribute("aria-checked", String(on));
    row.addEventListener("click", async () => {
      try {
        settings = await invoke("batch_prompts", { ids: [id], action: on ? "removeView" : "addView", color: null, favorite: null, viewId: v.id });
      } catch (e) { batchErrorToast(e); return; }
      renderViewsPop(id);
    });
    viewsPop.appendChild(row);
  }
}
function openViewsPop(id, rect) {
  renderViewsPop(id);
  placePopover(viewsPop, rect);
}

// ---- Grid tag filter — header toolbar popover, own Set from the library's ----
const gridTagPop = document.createElement("div");
gridTagPop.className = "ctx grid-tag-pop hidden";
gridTagPop.setAttribute("role", "menu");
document.body.appendChild(gridTagPop);
function closeGridTagPop() {
  gridTagPop.classList.add("hidden");
}
function updateGridTagBtn() {
  $("grid-tag-btn")?.classList.toggle("active", gridTags.size > 0);
}
function renderGridTagPop() {
  gridTagPop.innerHTML = "";
  const names = allTags();
  for (const name of names) {
    const chip = tagChipEl(name, "button");
    chip.classList.add("tag-filter");
    const on = gridTags.has(tagKey(name));
    chip.classList.toggle("active", on);
    chip.setAttribute("aria-pressed", String(on));
    chip.addEventListener("click", () => {
      const key = tagKey(name);
      if (gridTags.has(key)) gridTags.delete(key); else gridTags.add(key);
      renderGridTagPop();
      updateGridTagBtn();
      renderGrid(true);
    });
    gridTagPop.appendChild(chip);
  }
  if (gridTags.size) {
    const clear = document.createElement("button");
    clear.type = "button";
    clear.className = "tag-action";
    clear.textContent = t("tagClearFilter");
    clear.addEventListener("click", () => {
      gridTags.clear();
      renderGridTagPop();
      updateGridTagBtn();
      renderGrid(true);
    });
    gridTagPop.appendChild(clear);
  }
  if (!names.length && !gridTags.size) {
    const hint = document.createElement("div");
    hint.className = "hint";
    hint.textContent = t("tagFilterEmpty");
    gridTagPop.appendChild(hint);
  }
}
function openGridTagPop() {
  renderGridTagPop();
  placePopover(gridTagPop, $("grid-tag-btn").getBoundingClientRect());
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
      await invoke("set_ui_flag", { key, enabled: next }).catch((e) => errToast(e));
      // The ★ button only shows/works when the favorites-view feature is on, so
      // enabling it from the toolbar turns the feature on too (otherwise the menu
      // shows a checkmark but no star appears).
      if (key === "favViewButton" && next && !favViewEnabled()) {
        settings.ui_flags.favView = true;
        await invoke("set_ui_flag", { key: "favView", enabled: true }).catch((e) => errToast(e));
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
  placePopoverAt(toolbarMenu, x, y);
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
  none.title = t("colorNone");
  none.setAttribute("aria-label", t("colorNone"));
  none.addEventListener("click", () => onPick(""));

  const custom = mkSwatch("custom" + (isCustom ? " sel" : ""), isCustom ? selected : "");
  custom.title = t("colorCustom");
  custom.setAttribute("aria-label", t("colorCustom"));
  custom.addEventListener("click", () => onCustom(custom, isCustom ? selected : ""));

  for (const c of COLORS.slice(1)) {
    const sw = mkSwatch(c === selected ? "sel" : "", c);
    sw.title = c;
    sw.setAttribute("aria-label", c);
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
    modalState.tags || [], modal.notesText.value, modalState.hotkey || "", modalState.hotkeyPaste || "",
    // The entry list + mode are edited entirely in memory until Save.
    modalState.rotateEntries || [], modalState.rotateMode || "", !!modalState.rotateAuto,
    modalState.pdfPage || 0,
  ]);
}

// ---- Prompt variables ({#{placeholder}#}) ----
// The {#{ ... }#} fence is deliberately uncommon so a literal "{{" pasted from a
// code block is never mistaken for a variable. The 200-char body leaves room for
// a choice variable's option list, not just a name.
const VAR_RE = /\{#\{\s*([^}\n]{1,200}?)\s*\}#\}/g;
function extractVars(text) {
  const seen = [];
  for (const m of (text || "").matchAll(VAR_RE)) {
    const k = m[1].trim();
    if (k && !seen.includes(k)) seen.push(k);
  }
  return seen;
}
const fillVars = (text, values) => text.replace(VAR_RE, (_, k) => values[k.trim()] ?? "");
// A fence whose body is only whitespace ({#{ }#}) never becomes a real variable —
// extractVars() silently drops it — so it would sit in the copied text unfilled and
// unexplained. Caught before save instead of left a silent trap.
const hasEmptyVarToken = (text) =>
  [...String(text || "").matchAll(VAR_RE)].some((m) => !m[1].trim());

// A placeholder gets a visible accent wherever raw prompt text is previewed, so
// "does this prompt ask for anything" no longer surprises you only at copy time. DOM
// text nodes + a plain span, never innerHTML — a token is user-typed text.
function fillVarHighlighted(el, text) {
  el.textContent = "";
  let last = 0;
  for (const m of text.matchAll(VAR_RE)) {
    if (m.index > last) el.appendChild(document.createTextNode(text.slice(last, m.index)));
    const span = document.createElement("span");
    span.className = "var-token";
    span.textContent = m[0];
    el.appendChild(span);
    last = m.index + m[0].length;
  }
  if (last < text.length) el.appendChild(document.createTextNode(text.slice(last)));
}

// A token is `name`, `name=default`, `name|opt,opt,…` or `name=default|opt,opt,…`.
// Options are trimmed and empty ones dropped; a comma always starts the next
// option, so an option can never contain one. A default selects the matching
// entry in a choice list, or prefills a plain text variable when there is no
// list. A bar or an equals sign without a usable name before it stays an
// ordinary text variable — the whole token is then its name, exactly as
// before, and no default is extracted from it.
function parseVar(token) {
  const raw = String(token).trim();
  const bar = raw.indexOf("|");
  const namePart = bar < 0 ? raw : raw.slice(0, bar);
  const eq = namePart.indexOf("=");
  const rawName = eq < 0 ? namePart.trim() : namePart.slice(0, eq).trim();
  const rawDefault = eq < 0 ? "" : namePart.slice(eq + 1).trim();
  if (bar < 0 || !flag("choiceVars")) {
    return rawName ? { name: rawName, options: [], default: rawDefault } : { name: raw, options: [], default: "" };
  }
  const options = raw.slice(bar + 1).split(",").map((o) => o.trim()).filter(Boolean);
  return rawName && options.length
    ? { name: rawName, options, default: rawDefault }
    : { name: raw, options: [], default: "" };
}

// Reserved names that fill themselves in at copy time, no dialog. They win
// over a manual variable of the same name (documented in the editor's helper).
const DYN_VARS = [
  "date", "time", "datetime", "weekday", "yesterday", "tomorrow",
  "week", "month", "year", "timestamp", "epoch",
  "clipboard", "username", "uuid", "random", "counter",
];
const DYN_SET = new Set(DYN_VARS);
// A reserved name is WRITTEN with a slash fence: {#{/date\}#}. That keeps every plain
// word free for a variable of the user's own — "date" only fills itself in when it is
// spelled /date\. Old prompts are rewritten once by the store migration.
const DYN_OPEN = "/";
const DYN_CLOSE = "\\";
const dynToken = (name) => `${DYN_OPEN}${name}${DYN_CLOSE}`;
const dynName = (raw) =>
  raw.length > 2 && raw.startsWith(DYN_OPEN) && raw.endsWith(DYN_CLOSE) ? raw.slice(1, -1) : "";
const isDynVar = (name) => flag("dynVars") && DYN_SET.has(dynName(name));
// The app language is a bare ISO code, which is already a valid BCP-47 tag; only
// where the default region would be the wrong one is it spelled out. The app's
// Portuguese is European, so "pt" must not fall back to the Brazilian default.
const DYN_LOCALES = { pt: "pt-PT" };
const dynLocale = () => DYN_LOCALES[LANG] || LANG;
const pad2 = (n) => String(n).padStart(2, "0");
const intlFmt = (opts) => {
  try { return new Intl.DateTimeFormat(dynLocale(), opts); } catch (_) { return new Intl.DateTimeFormat("en", opts); }
};
function fmtDynDate(d, style) {
  if (style === "iso") return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
  return intlFmt({ dateStyle: style }).format(d);
}
function fmtDynTime(d, style) {
  if (style === "iso") return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
  return intlFmt({ timeStyle: style }).format(d);
}
// Same calendar day ± n days, keeping the local time of day (so a DST change never
// shifts the date by one).
function dayShift(d, days) {
  const out = new Date(d.getTime());
  out.setDate(out.getDate() + days);
  return out;
}
// ISO-8601 week number: the week holding the Thursday of the current week.
function isoWeek(d) {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  t.setUTCDate(t.getUTCDate() + 4 - (t.getUTCDay() || 7));
  const jan1 = Date.UTC(t.getUTCFullYear(), 0, 1);
  return Math.ceil(((t - jan1) / 86400000 + 1) / 7);
}
// ISO-8601 local timestamp with the UTC offset ("2026-07-27T14:03:22+02:00").
function isoStamp(d) {
  const off = -d.getTimezoneOffset();
  const sign = off < 0 ? "-" : "+";
  const abs = Math.abs(off);
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}` +
    `T${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}` +
    `${sign}${pad2(Math.floor(abs / 60))}:${pad2(abs % 60)}`;
}
// Short throwaway id — the same platform RNG as the UUID, just eight characters.
const shortId = () =>
  [...crypto.getRandomValues(new Uint8Array(6))].map((b) => b.toString(36).padStart(2, "0")).join("").slice(0, 8);
// {#{counter}#} counts copies, so its value has to survive a restart. Kept in the
// same ui_texts store the remembered variable values use.
const VAR_COUNTER_MAX = 1e9;
function nextVarCounter() {
  const now = Math.floor(Number(settings.ui_texts?.varCounter) || 0);
  const next = now >= 0 && now < VAR_COUNTER_MAX ? now + 1 : 1;
  settings.ui_texts = { ...(settings.ui_texts || {}), varCounter: String(next) };
  invoke("set_ui_text", { key: "varCounter", value: String(next) }).catch(() => {});
  return next;
}
// RFC 4122 v4, lowercase — built from the platform RNG, no dependency.
function uuidV4() {
  const b = crypto.getRandomValues(new Uint8Array(16));
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const h = [...b].map((x) => x.toString(16).padStart(2, "0")).join("");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}
// Clipboard text is foreign input pasted into the prompt: cap it so one runaway
// copy can never blow up the clipboard payload.
const CLIP_VAR_MAX = 100000;

// Resolve one reserved name. `ctx` carries the values that cost a round trip or a
// write, fetched once per copy: the clipboard, the account name, the counter.
function dynValue(name, now, ctx) {
  const ds = txt("dateStyle") || "short";
  const ts = txt("timeStyle") || "short";
  switch (name) {
    case "date": return fmtDynDate(now, ds);
    case "time": return fmtDynTime(now, ts);
    case "datetime": return `${fmtDynDate(now, ds)} ${fmtDynTime(now, ts)}`;
    case "weekday": return intlFmt({ weekday: "long" }).format(now);
    case "yesterday": return fmtDynDate(dayShift(now, -1), ds);
    case "tomorrow": return fmtDynDate(dayShift(now, 1), ds);
    case "week": return String(isoWeek(now));
    case "month": return intlFmt({ month: "long" }).format(now);
    case "year": return String(now.getFullYear());
    case "timestamp": return isoStamp(now);
    case "epoch": return String(Math.floor(now.getTime() / 1000));
    case "clipboard": return ctx.clip;
    case "username": return ctx.user;
    case "uuid": return uuidV4();
    case "random": return shortId();
    case "counter": return String(ctx.counter);
    default: return "";
  }
}

// The variable tokens of a prompt that are worth resolving at all (text prompts
// only — an image/file prompt copies its media, not its text).
// `text` overrides which string is scanned (the rotated entry actually about
// to be copied) — every existing caller keeps checking p.text unchanged.
const varTokens = (p, text) =>
  (flag("promptVars") && p && !p.copy_image && !p.file_path ? extractVars(text ?? p.text) : []);
// The subset the user still has to fill in by hand — the only reason to open a dialog.
// An override text (the peeked rotating entry) must flow through the same as
// varTokens takes one, or a placeholder living only in a later entry never gets
// noticed at all -- checking p.text (entry 0) unconditionally.
const dialogTokens = (p, text) => varTokens(p, text).filter((tok) => !isDynVar(parseVar(tok).name));
// True while any entry of a rotating prompt still carries a placeholder. An
// auto-advance session renders the following entries itself, from their raw text —
// so a prompt in this state must not open one, or the second paste would deliver
// the placeholder rather than what it stands for.
const anyEntryHasVars = (p) =>
  [p.text, ...(p.variants || [])].some((entry) => varTokens(p, entry).length > 0);

// The text a copy of `p` is actually about to use — the pointer's CURRENT entry
// for a rotating prompt, p.text otherwise. Read-only (never advances the pointer),
// so it is safe to call ahead of a decision (dialogTokens) as well as the copy
// itself. Every path that resolves p.text directly instead of this one silently
// mis-detects (and mis-copies) once rotation has moved past entry 0.
async function peekRotatedText(p) {
  if (!(p.variants || []).length) return p.text;
  const peek = await invoke("rotation_peek", { id: p.id }).catch(() => null);
  return peek ? peek.text : p.text;
}

// The one place every copy path goes through: reserved names resolve themselves,
// the rest are asked for once. Returns the finished text, null when the user
// cancelled — or false when `standalone` was asked for and the small window could
// not be opened, so the caller can fall back to the in-window dialog.
// `standalone` is set only by copies started from OUTSIDE the app (global hotkey,
// tray): everything inside the window uses the in-window dialog.
async function resolveVars(text, standalone = false) {
  const tokens = extractVars(text);
  if (!tokens.length) return text;
  const values = {};
  const ask = [];
  const dyn = [];
  for (const tok of tokens) {
    (isDynVar(parseVar(tok).name) ? dyn : ask).push(tok);
  }
  if (dyn.length) {
    const names = new Set(dyn.map((tok) => dynName(parseVar(tok).name)));
    const ctx = { clip: "", user: "", counter: 0 };
    if (names.has("clipboard")) {
      ctx.clip = String(await invoke("clipboard_text").catch(() => "") || "").slice(0, CLIP_VAR_MAX);
      if (!ctx.clip) toast(t("clipVarEmpty")); // image/file payload, or an empty clipboard
    }
    if (names.has("username")) ctx.user = String(await invoke("os_user_name").catch(() => "") || "");
    if (names.has("counter")) ctx.counter = nextVarCounter();
    const now = new Date();
    for (const tok of dyn) values[tok] = dynValue(dynName(parseVar(tok).name), now, ctx);
  }
  if (ask.length) {
    const filled = standalone ? await promptVarsWindow(ask) : await promptVarsDialog(ask);
    if (filled === false) return false;
    if (!filled) return null;
    Object.assign(values, filled);
  }
  return fillVars(text, values);
}

// ---- Editor variable helper ----
// One-line explanation per reserved name, shown as the chip's tooltip.
const DYN_TIPS = {
  date: "dynDate", time: "dynTime", datetime: "dynDatetime",
  weekday: "dynWeekday", yesterday: "dynYesterday", tomorrow: "dynTomorrow",
  week: "dynWeek", month: "dynMonth", year: "dynYear",
  timestamp: "dynTimestamp", epoch: "dynEpoch",
  clipboard: "dynClipboard", username: "dynUsername",
  uuid: "dynUuid", random: "dynRandom", counter: "dynCounter",
};
// A name or option must not carry the fence characters, or the token breaks.
const cleanVarPart = (s) => s.replace(/[|{}\n]/g, "").trim();

// Write a placeholder at the caret of the prompt textarea.
function insertVarToken(body) {
  const ta = modal.text;
  const start = ta.selectionStart ?? ta.value.length;
  const end = ta.selectionEnd ?? start;
  const token = `{#{${body}}#}`;
  ta.value = ta.value.slice(0, start) + token + ta.value.slice(end);
  ta.focus();
  ta.setSelectionRange(start + token.length, start + token.length);
  ta.dispatchEvent(new Event("input", { bubbles: true })); // length counter + clear-X
}

// Rebuild the helper for the current feature flags (chips only with promptVars,
// the options field only with choiceVars).
function renderVarHelper() {
  const chips = $("modal-vars-chips");
  const dyn = flag("dynVars");
  chips.classList.toggle("hidden", !dyn);
  $("modal-vars-dyn-hint").classList.toggle("hidden", !dyn);
  chips.innerHTML = "";
  if (dyn) {
    for (const name of DYN_VARS) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "vars-chip";
      b.textContent = dynToken(name);
      b.title = t(DYN_TIPS[name]); // hover engine themes any [title]
      b.addEventListener("click", () => insertVarToken(dynToken(name)));
      chips.appendChild(b);
    }
  }
  const choice = flag("choiceVars");
  $("modal-var-opts").classList.toggle("hidden", !choice);
  $("modal-vars-choice-hint").classList.toggle("hidden", !choice);
}

function insertHelperVar() {
  const nameEl = $("modal-var-name");
  const optsEl = $("modal-var-opts");
  const name = cleanVarPart(nameEl.value);
  if (!name) { nameEl.focus(); return; }
  const opts = flag("choiceVars")
    ? optsEl.value.split(",").map(cleanVarPart).filter(Boolean)
    : [];
  insertVarToken(opts.length ? `${name}|${opts.join(",")}` : name);
  nameEl.value = "";
  optsEl.value = "";
}
$("modal-var-add").addEventListener("click", insertHelperVar);
for (const id of ["modal-var-name", "modal-var-opts"]) {
  $(id).addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    e.preventDefault(); // Enter here inserts, it must not submit the editor
    insertHelperVar();
  });
}

// Values the user asked to remember, keyed by variable NAME (expert: varDefaults).
// Both fill-in surfaces read and write them through these two helpers.
function readRememberedVars() {
  try { return JSON.parse(settings.ui_texts?.varDefaults || "{}"); } catch (_) { return {}; }
}
function saveRememberedVars(next, prev) {
  const json = JSON.stringify(next);
  if (json === JSON.stringify(prev)) return;
  settings.ui_texts = { ...(settings.ui_texts || {}), varDefaults: json };
  invoke("set_ui_text", { key: "varDefaults", value: json }).catch(() => {});
}

// Fill-in dialog shown when copying a prompt that holds {#{placeholders}#}.
// `vars` are raw tokens; resolves a {token: value} map, or null when cancelled.
let varsCleanup = null;
function promptVarsDialog(vars) {
  const root = $("vars-modal");
  $("vars-title").textContent = t("varsTitle");
  const fields = $("vars-fields");
  fields.innerHTML = "";
  // Remembered values are stored globally by variable NAME (expert: varDefaults);
  // a choice variable keeps its last pick without a checkbox (expert: choiceRemember).
  const useRemember = flag("varDefaults");
  const keepChoice = flag("choiceRemember");
  const remembered = readRememberedVars();
  const rows = vars.map((token) => {
    const { name, options, default: def } = parseVar(token);
    const label = document.createElement("label");
    label.className = "field";
    const span = document.createElement("span");
    span.textContent = name; // textContent: a variable name is never markup
    let control;
    let cb = null;
    if (options.length) {
      control = document.createElement("select");
      control.className = "modal-input expert-select vars-select";
      for (const o of options) {
        const opt = document.createElement("option");
        opt.value = o;
        opt.textContent = o;
        control.appendChild(opt);
      }
      const last = keepChoice ? remembered[name] : null;
      const usedLast = options.includes(last);
      // Remembered pick wins over the text's own default -- it is the more
      // recent decision.
      control.value = usedLast ? last : (def && options.includes(def) ? def : options[0]);
      // Says outright that this pick came from last time, not just the checkbox
      // state (there is none for a choice variable — it remembers silently).
      if (usedLast) control.title = t("rememberedValueHint");
      // A single option is a fixed value: still shown, but nothing to choose.
      if (options.length === 1) control.disabled = true;
      else attachSelectPicker(control); // themed popup, 8 rows then scroll
      label.append(span, control);
    } else {
      control = document.createElement("input");
      control.className = "modal-input";
      control.type = "text";
      if (useRemember && name in remembered) {
        control.value = remembered[name];
        control.title = t("rememberedValueHint"); // Says so, not just a checked box
      } else if (def) {
        control.value = def;
      }
      label.append(span, control);
      if (useRemember) {
        const rem = document.createElement("label");
        rem.className = "vars-remember";
        cb = document.createElement("input");
        cb.type = "checkbox";
        cb.className = "switch";
        cb.checked = name in remembered;
        const rlbl = document.createElement("span");
        rlbl.textContent = t("rememberValue");
        rem.append(cb, rlbl);
        label.appendChild(rem);
      }
    }
    fields.appendChild(label);
    return { token, name, choice: options.length > 0, control, cb };
  });
  wireClearButtons(fields); // clear-X on the variable inputs
  // Save/forget remembered values on confirm (never on cancel). Entries for
  // variables outside this dialog are carried over untouched.
  const persistRemembered = () => {
    const obj = { ...remembered };
    for (const r of rows) {
      if (r.choice) {
        if (keepChoice) obj[r.name] = r.control.value;
      } else if (useRemember) {
        if (r.cb?.checked) obj[r.name] = r.control.value;
        else delete obj[r.name];
      }
    }
    saveRememberedVars(obj, remembered);
  };
  $("vars-ok").textContent = t("varsCopy");
  varsCleanup?.(null);
  root.classList.remove("hidden");
  rows[0]?.control.focus();
  return new Promise((resolve) => {
    const ok = $("vars-ok");
    const cancel = $("vars-close");
    const clearAll = $("vars-clear-all");
    const collect = () => Object.fromEntries(rows.map((r) => [r.token, r.control.value]));
    // Only plain text fields have an empty state worth clearing to — a choice
    // dropdown always needs a value, so it is left on whatever it already shows.
    const onClearAll = () => {
      for (const r of rows) if (!r.choice) r.control.value = "";
      rows.find((r) => !r.choice)?.control.focus();
    };
    // The click/keypress that OPENED this dialog is still in flight: arm the
    // backdrop + Enter close handlers only on the next frame so that same event
    // can't immediately dismiss the dialog (it was flashing shut instantly).
    // A timer, not a frame callback: this dialog can open while the window is still
    // hidden or tray-minimised, and a WebView2 window that paints no frames would
    // leave Enter and the backdrop dead for as long as it stays that way.
    let armed = false;
    setTimeout(() => { armed = true; }, 0);
    const done = (val) => {
      root.classList.add("hidden");
      ok.removeEventListener("click", onOk);
      cancel.removeEventListener("click", onCancel);
      clearAll.removeEventListener("click", onClearAll);
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
    clearAll.addEventListener("click", onClearAll);
    root.addEventListener("pointerdown", onBg);
    document.addEventListener("keydown", onKey);
  });
}

// Record a copy for the history/usage journal (backend honours the privacy flag).
// Every successful copy path ends here, which makes it the one place that may start
// the cooldown — see below.
const recordCopy = (id) => {
  markCopied(id);
  if (optFlag("copySound")) playCopySound(val("copySoundVolume"));
  return invoke("record_copy", { id }).catch(() => {});
};

// Per-prompt copy throttle: block re-copying the SAME prompt within the cooldown
// (expert value, 0 = off). Other prompts are never blocked, so a wrong pick can
// be corrected instantly. Returns true if this copy should be skipped.
// The check is pure: a copy that never happened — the placeholder dialog was
// cancelled — must not lock the prompt out for the whole cooldown.
const copyTimes = new Map();
const COPY_TIMES_CAP = 256; // sweep entries past the cooldown once the map grows
function copyOnCooldown(id) {
  const ms = val("copyCooldownMs");
  if (!ms) return false;
  const last = copyTimes.get(id);
  return !!last && Date.now() - last < ms;
}
// Start the cooldown for a prompt that really was copied.
function markCopied(id) {
  const ms = val("copyCooldownMs");
  if (!ms) return;
  const now = Date.now();
  if (copyTimes.size >= COPY_TIMES_CAP) {
    for (const [k, t0] of copyTimes) if (now - t0 >= ms) copyTimes.delete(k);
  }
  copyTimes.set(id, now);
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

// Copy a text prompt after filling its placeholders. `paste` is only set by
// activations that mean "paste" (Enter on a focused tile, a prompt hotkey with the
// paste option on); a plain click copies and nothing else, exactly like a prompt
// without variables.
async function copyTextWithVars(p, el, paste = false) {
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
  // Peek -> resolve -> copy -> commit, same order copyResolved uses -- this used
  // to always resolve p.text (entry 0) even once rotation had moved past it.
  const rotating = (p.variants || []).length > 0;
  const peek = rotating ? await invoke("rotation_peek", { id: p.id }).catch(() => null) : null;
  const rawText = peek ? peek.text : p.text;
  const text = await resolveVars(rawText);
  clearTimeout(stopArming);
  overlay.removeEventListener("pointerdown", onSecond, true);
  if (text === null) return;
  const ok = await invoke("copy_text", { text })
    .catch((e) => { errToast(e); return false; });
  if (ok) {
    showCopied(el);
    recordCopy(p.id);
    // A failed peek already fell back to p.text above -- only a successful peek may
    // be followed by a commit (copyResolved's own rule).
    if (rotating && peek) invoke("rotation_commit", { id: p.id }).catch(() => {});
    if (paste || (quickSecond && flag("autoPaste"))) await autoPasteNow(el);
    maybeCloseOnCopy();
  }
}

// ---- Standalone fill-in window (vars.html) ----
// Used ONLY by copies started outside the app (global prompt hotkey, tray quick
// copy): a small always-on-top window asks for the placeholders so the app the user
// is working in keeps the foreground. Everything the variables MEAN still lives
// here — that window is a form, it renders rows and hands the answers back.
let varsWindowPending = null;

async function promptVarsWindow(tokens) {
  const useRemember = flag("varDefaults");
  const keepChoice = flag("choiceRemember");
  const remembered = readRememberedVars();
  const rows = tokens.map((token) => {
    const { name, options, default: def } = parseVar(token);
    const last = remembered[name];
    return {
      token,
      name,
      options,
      // Remembered pick/value wins over the text's own default everywhere --
      // it is the more recent decision.
      value: options.length
        ? (keepChoice && options.includes(last) ? last : (def && options.includes(def) ? def : options[0]))
        : (useRemember && name in remembered ? String(last) : (def || "")),
      remember: !options.length && useRemember && name in remembered,
    };
  });
  const payload = {
    theme: document.documentElement.getAttribute("data-theme") || "",
    vars: activeThemeVars(),
    title: t("varsTitle"),
    ok: t("varsCopy"),
    cancel: t("cancel"),
    rememberLabel: t("rememberValue"),
    askRemember: useRemember,
    rows: rows.map(({ name, options, value, remember }) => ({ name, options, value, remember })),
  };
  // A second hotkey while a window is still open cancels the older copy — one
  // pending question at a time, or two answers would race for one clipboard.
  const stale = varsWindowPending;
  varsWindowPending = null;
  stale?.(null);
  const answer = await new Promise((resolve) => {
    varsWindowPending = resolve;
    invoke("open_var_window", { payload: JSON.stringify(payload), rows: rows.length })
      .catch(() => { varsWindowPending = null; resolve(false); });
  });
  if (answer === false) return false; // no window — the caller falls back
  if (!answer) return null;
  let parsed;
  try { parsed = JSON.parse(answer); } catch (_) { return null; }
  if (!Array.isArray(parsed.values) || parsed.values.length !== rows.length) return null;
  const next = { ...remembered };
  const filled = {};
  rows.forEach((r, i) => {
    const value = String(parsed.values[i] ?? "");
    filled[r.token] = value;
    if (r.options.length) {
      if (keepChoice) next[r.name] = value;
    } else if (useRemember) {
      if (parsed.remember?.[i]) next[r.name] = value;
      else delete next[r.name];
    }
  });
  saveRememberedVars(next, remembered);
  return filled;
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
// One activation path for "copy this prompt now": Enter on a focused tile and a
// prompt hotkey both land here, so usage recording, chain semantics and
// placeholder filling behave identically. `paste` decides whether the copy is also
// pushed into the last app — Enter follows the auto-paste setting, a hotkey follows
// its own option.
async function activateTile(id, paste = flag("autoPaste")) {
  const p = prompts.find((x) => x.id === id);
  if (!p) return;
  const el = gridEl.querySelector(`.tile[data-id="${CSS.escape(id)}"]`);
  if (chainMode) { await toggleChainPick(p); return; }
  if (copyOnCooldown(id)) return;
  if (dialogTokens(p, await peekRotatedText(p)).length) {
    await copyTextWithVars(p, el, paste);
  } else if (await copyResolved(p)) {
    if (el) showCopied(el);
    recordCopy(id);
    if (paste) await autoPasteNow(el);
    maybeCloseOnCopy();
  }
}

// A prompt was triggered from outside the grid — its global hotkey or a tray
// quick-copy entry. Placeholders that still need typing are asked for in a
// small window of their own, so the app the user is working in stays in front;
// everything else copies (and optionally pastes) without ever showing up.
async function onPromptHotkey(id, paste) {
  const p = prompts.find((x) => x.id === id);
  if (!p) return;
  if (dialogTokens(p, await peekRotatedText(p)).length) {
    // Chain mode is an in-window gesture — a hotkey then still goes through the grid.
    if (flag("varPopupWindow") && !chainMode) { await copyOutsideApp(p, paste); return; }
    await invoke("focus_main_window").catch(() => {}); // fallback: the in-window dialog
  }
  await activateTile(id, paste);
}

// The hotkey/tray copy of a prompt with manual placeholders: ask in the standalone
// window, then finish the copy exactly like every other path does. Falls back to the
// in-window dialog if the small window cannot be opened at all.
async function copyOutsideApp(p, paste) {
  if (copyOnCooldown(p.id)) return;
  // Same peek -> resolve -> copy -> commit order as copyTextWithVars/copyResolved.
  const rotating = (p.variants || []).length > 0;
  const peek = rotating ? await invoke("rotation_peek", { id: p.id }).catch(() => null) : null;
  const rawText = peek ? peek.text : p.text;
  const text = await resolveVars(rawText, true);
  if (text === null) return; // cancelled
  if (text === false) { // no standalone window available
    await invoke("focus_main_window").catch(() => {});
    await activateTile(p.id, paste);
    return;
  }
  if (await invoke("copy_text", { text }).catch((e) => { errToast(e); return false; })) {
    recordCopy(p.id);
    if (rotating && peek) invoke("rotation_commit", { id: p.id }).catch(() => {});
    if (paste) await autoPasteNow(null);
  }
}
// Expert opt: hide the window to the tray right after a prompt copy.
function maybeCloseOnCopy() {
  if (!optFlag("hideWindowOnCopy")) return;
  try { window.__TAURI__?.window?.getCurrentWindow?.()?.hide?.().catch?.(() => {}); } catch { /* no-op */ }
}

function openModal({ mode, id, name = "", text = "", color = "", image = "", showImage = false, copyImage = false, filePath = "", pdfPage = 0, iconPath = "", caption = "", captionSize = 0, font = "", fontSize = 0, favorite = false, tags = [], notes = "", hotkey = "", hotkeyPaste = "", variants = [], rotateMode = "", rotateAuto = false, title, targetCell }) {
  modalSession++;
  modalOpener = document.activeElement;
  modalState = {
    mode, id, color, image, showImage, copyImage, filePath, pdfPage, iconPath, tags: [...tags], hotkey, hotkeyPaste, targetCell,
    // What the editor opened with. Save compares against it and only sends an
    // image field when the user really changed the picture — a save must never be
    // able to clear a stored image just by not having it to hand.
    imageOpened: image,
    // The FULL entry list (entry 0 = what would otherwise be modal.text's value)
    // while rotation is active, empty while it is not.
    rotateEntries: variants.length ? [text, ...variants] : [],
    rotateMode,
    rotateAuto: !!rotateAuto,
  };
  modal.title.textContent = title;
  modal.fav.classList.toggle("active", !!favorite);
  modal.fav.setAttribute("aria-pressed", String(!!favorite));
  modal.name.value = name;
  modal.text.value = text;
  mdEditing = false; // every prompt opens rendered; a click on the pane starts editing
  mdLastText = null;
  modal.notesText.value = notes;
  modal.caption.value = caption;
  modal.captionSize.value = String(normSize(captionSize) || 0);
  updateCaptionPreview();
  // Per-tile style overrides, available when creating and editing.
  modal.fontSel.value = font;
  modal.sizeSel.value = String(normSize(fontSize));
  modal.delete.classList.toggle("hidden", mode !== "edit");
  disarmButton(modal.delete, t("delete"));
  renderSwatches(color);
  const tagInput = $("modal-tag-input");
  if (tagInput) tagInput.value = "";
  closeTagSuggest();
  renderTagChips();
  renderPromptHotkey();
  // Show the dialog before wiring the preview so the <video>/<img> decode and
  // lay out in a visible context (a hidden element never renders a first frame).
  modal.root.classList.remove("hidden");
  syncModalImageUi(mode);
  modalInitial = modalSnapshot();
  modal.name.focus();
  modal.name.select();
}

// ---- Per-prompt hotkey (editor row) ----
// The capture field mirrors the global-hotkey setting: click it, press a combo. The
// binding is stored with the prompt and only reaches the OS on save, so a cancelled
// edit changes nothing.
const promptHotkeysOn = () => flag("promptHotkeys");

// The backend answers a rejected accelerator with a code so the reason can be shown
// in the app language; anything else passes through unchanged. "save-failed" isn't a
// hotkey code at all (update_prompt's generic write failure) but reaches this same
// mapper from Item-1's popup rows, so it gets the same friendly text the editor's
// own save path already shows for it.
const HOTKEY_ERRORS = {
  "hotkey-invalid": "promptHotkeyInvalid",
  "hotkey-taken": "promptHotkeyTaken",
  "hotkey-system": "promptHotkeySystem",
  "hotkey-conflict": "promptHotkeyConflict",
};
const hotkeyMessage = (err) =>
  HOTKEY_ERRORS[err] ? t(HOTKEY_ERRORS[err]) : err === "save-failed" ? t("persistFailed") : t("hotkeyFail");

// Show an inline reason and drop the capture; empty text clears the message again.
// Whatever the reason, the full list of what is already bound is one click away
// through the same button -- naming the owner is not enough on its own when the
// fix is to open that other binding and change it.
function showHotkeyError(msg) {
  modal.hotkeyErr.textContent = msg || "";
  modal.hotkeyErr.classList.toggle("hidden", !msg);
  const showAll = $("modal-hotkey-showall");
  if (showAll) showAll.classList.toggle("hidden", !msg);
}

// What a hotkey without its own choice inherits — named on the row, so "Default"
// never leaves the user guessing which of the two it means.
const hotkeyPasteDefault = () => (flag("promptHotkeyPaste") ? t("promptHotkeyPaste") : t("promptHotkeyCopy"));

function renderPromptHotkey() {
  const accel = modalState?.hotkey || "";
  const pretty = prettyAccel(accel);
  modal.hotkeyInput.value = pretty;
  modal.hotkeyBadge.textContent = pretty;
  modal.hotkeyBadge.classList.toggle("hidden", !pretty);
  const sel = modal.hotkeyAction;
  if (sel) {
    sel.value = modalState?.hotkeyPaste || "";
    // The inherited mode is spelled out on the default entry, and the whole row is
    // pointless without a binding to act on.
    const def = sel.querySelector('option[value=""]');
    if (def) def.textContent = t("promptHotkeyActionDefault").replace("{mode}", hotkeyPasteDefault());
    modal.hotkeyActionRow.classList.toggle("hidden", !accel);
  }
  showHotkeyError("");
}

// Which combinations are already spoken for: the app hotkey and every OTHER prompt.
// Compared on the raw accelerator, normalized the way the backend does it.
const accelKey = (a) => String(a || "").trim().toLowerCase()
  .split("+").map((s) => s.trim()).filter(Boolean)
  .map((s) => (["commandorcontrol", "commandorctrl", "cmdorctrl", "cmdorcontrol", "control"].includes(s) ? "ctrl"
    : ["command", "cmd", "meta", "win"].includes(s) ? "super"
      : s === "option" ? "alt" : s))
  .sort().join("+");

// Who already holds this combination inside the app: the app hotkey, another
// prompt, or nobody. Naming the owner is the difference between "taken" and a
// message the user can act on. `excludeId` is the prompt (if any) THIS field
// belongs to, so its own current binding never reads back as "already taken".
function hotkeyOwner(accel, excludeId = null) {
  const key = accelKey(accel);
  if (!key) return null;
  if (accelKey(settings.hotkey) === key) return { app: true };
  if (accelKey(settings.voice_hotkey) === key) return { voice: true };
  if (accelKey(settings.rec_stop_hotkey) === key) return { recStop: true };
  if (accelKey(settings.rec_pause_hotkey) === key) return { recPause: true };
  const p = prompts.find((x) => x.id !== excludeId && accelKey(x.hotkey) === key);
  return p ? { name: p.name } : null;
}
const hotkeyTakenMsg = (owner) =>
  owner.app ? t("promptHotkeyTakenApp")
    : owner.voice ? t("promptHotkeyTakenVoice")
    : owner.recStop ? t("promptHotkeyTakenRecStop")
    : owner.recPause ? t("promptHotkeyTakenRecPause")
    : t("promptHotkeyTakenBy").replace("{name}", owner.name);

// Global hotkeys have to be off while a field is capturing: Windows hands a
// registered combination to the owning process, not to the focused window, so the
// key press would never reach the field — the prompt behind it would simply be
// copied, with no error to show for it.
let hotkeyCaptureOn = false;
function hotkeyCapture(on) {
  if (on === hotkeyCaptureOn) return;
  hotkeyCaptureOn = on;
  invoke("set_hotkey_capture", { active: on }).catch(() => {});
}
// Storing a hotkey re-arms the whole set, so a field that still has the focus has
// to take them back off straight away.
function reassertHotkeyCapture() {
  if (!hotkeyCaptureOn) return;
  hotkeyCaptureOn = false;
  hotkeyCapture(true);
}

// ONE reusable hotkey-recording field, wired onto `input` — click, press a
// combo. A client-side pre-check (hotkeyOwner) gives instant feedback; the BACKEND
// re-checks before anything is actually kept (only it knows the system blocklist
// and can ask Windows whether another app already owns the combination), so a
// refusal here always leaves the previous binding exactly as it was.
//   id      — the prompt this field belongs to (null for the global hotkey), or a
//             function returning it fresh each time, for a field reused across
//             different prompts (the singleton editor).
//   errEl   — element for an inline error message; omitted falls back to a toast.
//   current — () => the accel to restore the field to after a refused capture.
//   onCommit(accel) — persists (or, for the editor, only stages) the change; must
//             throw the backend's rejection code on failure, and returns the accel
//             actually stored (not always what was requested — see the note
//             on update_prompt's own late OS-conflict check). Returning `null`
//             aborts silently without touching the field or showing an error — the
//             editor uses this when the user moved to a different prompt while an
//             await was in flight, so a stale answer never overwrites what is by
//             then a completely different prompt's row.
function bindHotkeyField(input, { id = null, errEl = null, showAllEl = null, current, onCommit }) {
  const resolveId = () => (typeof id === "function" ? id() : id);
  const showErr = (msg) => {
    if (errEl) { errEl.textContent = msg || ""; errEl.classList.toggle("hidden", !msg); }
    else if (msg) toast(msg);
    if (showAllEl) showAllEl.classList.toggle("hidden", !msg);
  };
  // A commit is a round trip (check_hotkey, maybe update_prompt); a second keypress
  // landing before the first resolves could otherwise finish out of order and
  // clobber the newer request with the older one's result.
  let busy = false;
  input.addEventListener("focus", () => hotkeyCapture(true));
  input.addEventListener("blur", () => hotkeyCapture(false));
  input.addEventListener("keydown", async (e) => {
    if (e.key === "Tab" || e.key === "Escape") return; // let focus leave the field
    e.preventDefault();
    if (busy) return;
    const clearing = e.key === "Backspace" || e.key === "Delete";
    let accel = "";
    if (!clearing) {
      accel = accelFromEvent(e);
      if (!accel) { showErr(t("promptHotkeyInvalid")); return; }
      const owner = hotkeyOwner(accel, resolveId());
      if (owner) { showErr(hotkeyTakenMsg(owner)); return; }
    }
    showErr("");
    busy = true;
    try {
      const stored = await onCommit(accel);
      if (stored === null) return; // aborted silently, see onCommit's doc above
      input.value = prettyAccel(stored);
      reassertHotkeyCapture(); // the field still has the focus
    } catch (err) {
      input.value = prettyAccel(current());
      showErr(hotkeyMessage(String(err)));
    } finally {
      busy = false;
    }
  });
}

function setPromptHotkey(accel) {
  if (!modalState) return;
  modalState.hotkey = accel;
  if (!accel) modalState.hotkeyPaste = ""; // no binding, no action to choose
  renderPromptHotkey();
}

// ---- "All hotkeys" — global + every per-prompt hotkey, each row live ----

// The ONE place that actually persists the global hotkey — shared by the Settings
// panel's own field and the popup's global row, so they can never disagree.
async function commitGlobalHotkey(accel) {
  await invoke("set_hotkey", { hotkey: accel });
  settings.hotkey = accel;
  if (accel) toast(t("hotkeySet"));
  renderHotkeysList(); // a no-op while the popup is closed
  return accel;
}
// 131's counterpart, same shape -- its own field, gated behind the voiceHotkey
// expert flag, checked through the same set_voice_hotkey/check_hotkey backend
// path (hotkeyOwner above already treats it as a real collision).
async function commitVoiceHotkey(accel) {
  await invoke("set_voice_hotkey", { hotkey: accel });
  settings.voice_hotkey = accel;
  if (accel) toast(t("hotkeySet"));
  return accel;
}
// 278's own pair, same shape again -- only ever actually registered while a
// recording runs (see rearm_hotkeys on the backend), so a successful commit
// here can still show as blocked later through the bar's own warning icon.
async function commitRecStopHotkey(accel) {
  await invoke("set_rec_stop_hotkey", { hotkey: accel });
  settings.rec_stop_hotkey = accel;
  if (accel) toast(t("hotkeySet"));
  return accel;
}
async function commitRecPauseHotkey(accel) {
  await invoke("set_rec_pause_hotkey", { hotkey: accel });
  settings.rec_pause_hotkey = accel;
  if (accel) toast(t("hotkeySet"));
  return accel;
}

// Unlike the editor (which only stages a hotkey until Save), a popup row has no
// save step of its own — it writes straight through update_prompt with every OTHER
// field taken from the prompt's own in-memory copy, unchanged. check_hotkey runs
// first so a live OS conflict shows up immediately instead of only being caught by
// update_prompt's own late rearm (see the "the backend hands back what it
// really stored" note at the editor's save path).
async function commitPromptHotkeyDirect(id, accel) {
  const p = prompts.find((x) => x.id === id);
  if (!p) throw "save-failed";
  if (accel) await invoke("check_hotkey", { hotkey: accel, id });
  const saved = await invoke("update_prompt", {
    id: p.id, name: p.name, text: p.text, color: p.color,
    // No image field: the prompt list no longer carries the bytes, and a missing
    // field means "keep the stored picture" — exactly what this write wants.
    showImage: p.show_image, copyImage: p.copy_image,
    filePath: p.file_path, pdfPage: p.pdf_page, iconPath: p.icon_path,
    caption: p.caption, captionSize: p.caption_size, font: p.font, fontSize: p.font_size,
    favorite: p.favorite, tags: p.tags, notes: p.notes,
    hotkey: accel, hotkeyPaste: accel ? p.hotkey_paste : "",
    variants: p.variants || [], rotateMode: p.rotate_mode || "", rotateAuto: p.rotate_auto || false,
  });
  if (!saved) throw "save-failed";
  p.hotkey = saved.hotkey;
  p.hotkey_paste = saved.hotkey_paste;
  if (accel && !saved.hotkey) toast(t("promptHotkeyFail")); // OS refused between check and save
  renderHotkeysList();
  return saved.hotkey;
}

// Tile-menu checkmark entry — same "write straight through with everything
// else unchanged" shape as commitPromptHotkeyDirect. Turning it off also ends a
// session that happens to be running right now, instead of waiting for the next
// paste to notice the prompt no longer qualifies.
async function toggleRotateAutoDirect(id) {
  const p = prompts.find((x) => x.id === id);
  if (!p) return;
  const nextAuto = !p.rotate_auto;
  try {
    const saved = await invoke("update_prompt", {
      id: p.id, name: p.name, text: p.text, color: p.color,
      // Same as commitPromptHotkeyDirect: no image field = keep the stored one.
      showImage: p.show_image, copyImage: p.copy_image,
      filePath: p.file_path, pdfPage: p.pdf_page, iconPath: p.icon_path,
      caption: p.caption, captionSize: p.caption_size, font: p.font, fontSize: p.font_size,
      favorite: p.favorite, tags: p.tags, notes: p.notes,
      hotkey: p.hotkey || "", hotkeyPaste: p.hotkey_paste || "",
      variants: p.variants || [], rotateMode: p.rotate_mode || "", rotateAuto: nextAuto,
    });
    if (!saved) throw "save-failed";
    p.rotate_auto = saved.rotate_auto;
    if (!p.rotate_auto) await invoke("rotation_end_auto", { id }).catch(() => {});
  } catch (err) {
    toast(String(err) === "save-failed" ? t("persistFailed") : String(err));
  }
}

function openHotkeysModal() {
  $("hotkeys-search").value = "";
  $("hotkeys-modal").classList.remove("hidden");
  renderHotkeysList();
  $("hotkeys-search").focus();
}
function closeHotkeysModal() {
  $("hotkeys-modal").classList.add("hidden");
}

// One entry per row kind: how to read its current value and how to commit a
// new one. "prompt" rows carry their own id (used both for the lookup and to
// exclude themselves from hotkeyOwner's conflict check); the other three share
// the null-id bucket the same way their own dedicated fields always have.
const HOTKEYS_LIST_KINDS = {
  global: { current: () => settings.hotkey, onCommit: commitGlobalHotkey },
  prompt: {
    current: (id) => prompts.find((x) => x.id === id)?.hotkey || "",
    onCommit: (accel, id) => commitPromptHotkeyDirect(id, accel),
  },
  recStop: { current: () => settings.rec_stop_hotkey, onCommit: commitRecStopHotkey },
  recPause: { current: () => settings.rec_pause_hotkey, onCommit: commitRecPauseHotkey },
};
function renderHotkeysList() {
  const box = $("hotkeys-list");
  if (!box || $("hotkeys-modal").classList.contains("hidden")) return;
  const q = ($("hotkeys-search").value || "").trim().toLowerCase();
  const rows = [{ kind: "global", id: null, name: t("hotkeysGlobalRow"), accel: settings.hotkey }]
    .concat(promptHotkeysOn() ? prompts.filter((p) => p.hotkey).map((p) => ({ kind: "prompt", id: p.id, name: p.name, accel: p.hotkey })) : [])
    .concat(flag("screenRecord") && flag("recStopHotkey") ? [{ kind: "recStop", id: null, name: t("recStopHotkeyLabel"), accel: settings.rec_stop_hotkey }] : [])
    .concat(flag("screenRecord") && flag("recPauseHotkey") ? [{ kind: "recPause", id: null, name: t("recPauseHotkeyLabel"), accel: settings.rec_pause_hotkey }] : []);
  const hit = (r) => !q || r.name.toLowerCase().includes(q) || prettyAccel(r.accel).toLowerCase().includes(q);
  const filtered = rows.filter(hit);
  box.innerHTML = "";
  if (!filtered.length) {
    const empty = document.createElement("p");
    empty.className = "hotkeys-empty";
    empty.textContent = t("hotkeysNoMatch");
    box.appendChild(empty);
    return;
  }
  for (const r of filtered) {
    const kindDef = HOTKEYS_LIST_KINDS[r.kind];
    const row = document.createElement("div");
    row.className = "hotkeys-row";
    const name = document.createElement("span");
    name.className = "hotkeys-row-name" + (r.kind === "global" ? " hotkeys-row-global" : "");
    name.textContent = r.name;
    const input = document.createElement("input");
    input.className = "modal-input";
    input.type = "text";
    input.readOnly = true;
    input.placeholder = t("hotkeyPh");
    input.value = prettyAccel(r.accel);
    const clear = document.createElement("button");
    clear.type = "button";
    clear.className = "ghost-btn";
    clear.textContent = t("hotkeyClear");
    row.append(name, input, clear);
    box.appendChild(row);
    bindHotkeyField(input, {
      id: r.id,
      current: () => kindDef.current(r.id),
      onCommit: (accel) => kindDef.onCommit(accel, r.id),
    });
    clear.addEventListener("click", async () => {
      try { input.value = prettyAccel(await kindDef.onCommit("", r.id)); }
      catch (err) { toast(hotkeyMessage(String(err))); }
    });
  }
}

// ---- live length counter (chars / words / ~tokens) in the editor ----
let lenRaf = 0;
function updateLenCounter() {
  const el = $("modal-length");
  if (!el) return;
  // Still a text prompt while the Markdown preview stands in for the textarea; never
  // while rotation replaced the textarea with the variants list instead.
  const on = flag("lengthCounter") && !rotationUiOn() &&
    !(modal.text.classList.contains("hidden") && modal.mdPreview.classList.contains("hidden"));
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

// Search re-renders the whole result list, which for a large library means rebuilding
// every row (and re-decoding every image thumbnail) — far too much to do on each
// keystroke. The field itself stays live; only the list waits for a short pause in
// typing. Short enough to feel immediate, long enough that a fast typist renders once.
const SEARCH_DEBOUNCE_MS = 120;
// ms may be a getter, so a caller can stay live-tunable (library search reads
// the searchDebounceMs expert value on every keystroke instead of a value baked
// in when the debounced wrapper was created).
function debounced(fn, ms = SEARCH_DEBOUNCE_MS) {
  let timer = 0;
  return () => {
    clearTimeout(timer);
    timer = setTimeout(fn, typeof ms === "function" ? ms() : ms);
  };
}

// ---- Markdown preview + code highlighting (editor) ----
// The prompt body is untrusted text. It is HTML-escaped ONCE, up front, and every
// transform below only ever slices that escaped string, so nothing a user types can
// come out as markup. A link keeps its label as plain styled text and never becomes
// an <a>, which is why no href/src exists for a click to follow.
let MD_MAX_CHARS = 100000; // past this the preview is cut, so split-mode typing stays smooth, expert-tunable
// A single huge fence is rendered plain: an unterminated block comment makes the
// lazy scan quadratic, and nobody reads 20k characters of colour anyway.
let HL_MAX_CHARS = 20000; // expert-tunable
const MD_ESC_RE = /[&<>"']/g;
const MD_ESC_MAP = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
const mdEsc = (s) => String(s).replace(MD_ESC_RE, (c) => MD_ESC_MAP[c]);

// Token patterns. They run against the ESCAPED code, which is why quotes and
// angle brackets appear as &quot; / &#39; / &lt; here. Deliberately best-effort:
// anything the scanner does not recognise stays plain, so malformed code still renders.
const HL_DQ = String.raw`&quot;(?:\\.|&(?!quot;)|[^&\\\n])*&quot;`;
const HL_SQ = String.raw`&#39;(?:\\.|&(?!#39;)|[^&\\\n])*&#39;`;
const HL_BT = "`(?:\\\\.|[^`\\\\])*`";
const HL_BLOCK_C = String.raw`/\*[\s\S]*?(?:\*/|$)`;
const HL_NUM = String.raw`\b(?:0[xXbBoO][0-9a-fA-F_]+|\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?)\w*`;
const hlLine = (mark) => `${mark}[^\\n]*`;
const hlWords = (w) => String.raw`\b(?:${w})\b`;
const HL_CLASSES = ["cmt", "str", "num", "kw", "fn"];
const HL_RULES = {
  js: { rules: [
    ["cmt", `${hlLine("//")}|${HL_BLOCK_C}`],
    ["str", `${HL_BT}|${HL_DQ}|${HL_SQ}`],
    ["num", HL_NUM],
    ["kw", hlWords("const|let|var|function|return|if|else|for|while|do|switch|case|break|continue|default|class|extends|super|new|delete|typeof|instanceof|in|of|this|null|undefined|true|false|async|await|import|export|from|as|try|catch|finally|throw|yield|void|static|interface|type|enum|implements|public|private|protected|readonly|declare|namespace|number|string|boolean|any|unknown|never")],
    ["fn", String.raw`\b[A-Za-z_$][\w$]*(?=\s*\()`],
  ] },
  py: { rules: [
    ["cmt", hlLine("#")],
    ["str", `(?:&quot;){3}[\\s\\S]*?(?:(?:&quot;){3}|$)|(?:&#39;){3}[\\s\\S]*?(?:(?:&#39;){3}|$)|${HL_DQ}|${HL_SQ}`],
    ["num", HL_NUM],
    ["kw", hlWords("def|class|return|if|elif|else|for|while|break|continue|pass|import|from|as|with|try|except|finally|raise|lambda|yield|global|nonlocal|assert|del|in|is|not|and|or|None|True|False|self|async|await|match|case")],
    ["fn", String.raw`\b[A-Za-z_]\w*(?=\s*\()`],
  ] },
  html: { rules: [
    ["cmt", String.raw`&lt;!--[\s\S]*?(?:--&gt;|$)`],
    ["str", `${HL_DQ}|${HL_SQ}`],
    ["kw", String.raw`&lt;/?[A-Za-z][\w:.-]*|/?&gt;`],
    ["fn", String.raw`[A-Za-z_:][\w:.-]*(?=\s*=)`],
  ] },
  css: { rules: [
    ["cmt", HL_BLOCK_C],
    ["str", `${HL_DQ}|${HL_SQ}`],
    ["num", String.raw`#[0-9a-fA-F]{3,8}\b|-?\b\d*\.?\d+(?:px|em|rem|%|vh|vw|vmin|vmax|ms|s|deg|fr|ch|pt)?\b`],
    ["kw", String.raw`@[a-zA-Z-]+|[-a-zA-Z]+(?=\s*:)`],
    ["fn", String.raw`[.#]?[-\w]+(?=\s*[{,])|::?[a-z-]+`],
  ] },
  json: { rules: [
    // A string followed by a colon is a key; plain strings keep the string colour.
    ["kw", `${HL_DQ}(?=\\s*:)|${hlWords("true|false|null")}`],
    ["str", HL_DQ],
    ["num", String.raw`-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b`],
  ] },
  sql: { flags: "i", rules: [
    ["cmt", `${hlLine("--")}|${HL_BLOCK_C}`],
    ["str", `${HL_SQ}|${HL_DQ}`],
    ["num", String.raw`\b\d+(?:\.\d+)?\b`],
    ["kw", hlWords("select|from|where|insert|into|values|update|set|delete|create|table|alter|drop|index|view|join|inner|left|right|full|outer|cross|on|group|by|order|having|limit|offset|distinct|as|and|or|not|null|is|in|between|like|exists|union|all|case|when|then|else|end|primary|key|foreign|references|default|unique|constraint|asc|desc|with|returning")],
    ["fn", String.raw`\b[A-Za-z_]\w*(?=\s*\()`],
  ] },
  bash: { rules: [
    ["cmt", hlLine("#")],
    ["str", `${HL_DQ}|${HL_SQ}`],
    ["num", String.raw`\b\d+\b`],
    ["kw", hlWords("if|then|elif|else|fi|for|in|do|done|while|until|case|esac|function|return|local|export|readonly|set|unset|shift|source|echo|cd|exit|trap|declare|sudo")],
    ["fn", String.raw`\$\{[^}\n]*\}|\$[A-Za-z_]\w*|\$[0-9@*#?]`],
  ] },
  rust: { rules: [
    ["cmt", `${hlLine("//")}|${HL_BLOCK_C}`],
    // A lifetime ('a) has no closing quote, so it simply falls through to plain.
    ["str", `${HL_DQ}|` + String.raw`&#39;(?:\\.|[^&\\\n])&#39;`],
    ["num", String.raw`\b(?:0[xXbBoO][0-9a-fA-F_]+|\d[\d_]*(?:\.\d+)?)(?:[iuf](?:8|16|32|64|128|size))?\b`],
    ["kw", hlWords("fn|let|mut|const|static|struct|enum|impl|trait|pub|use|mod|match|if|else|for|while|loop|return|break|continue|as|where|in|ref|move|dyn|unsafe|async|await|type|crate|self|Self|super|true|false|Some|None|Ok|Err")],
    ["fn", String.raw`#!?\[[^\]\n]*\]|\b[a-z_]\w*(?=\s*\()|\b[A-Z]\w*\b`],
  ] },
};
// Fence tag -> ruleset. An unknown or missing tag renders as plain code.
const HL_ALIAS = {
  js: "js", javascript: "js", jsx: "js", mjs: "js", node: "js",
  ts: "js", typescript: "js", tsx: "js",
  py: "py", python: "py",
  html: "html", htm: "html", xml: "html", svg: "html", vue: "html",
  css: "css", scss: "css", sass: "css", less: "css",
  json: "json", jsonc: "json",
  sql: "sql", mysql: "sql", psql: "sql", postgres: "sql", sqlite: "sql",
  sh: "bash", bash: "bash", shell: "bash", zsh: "bash", console: "bash",
  rs: "rust", rust: "rust",
};
// One regex per language, compiled once — never per keystroke.
const HL_RE = Object.fromEntries(Object.entries(HL_RULES).map(([id, def]) => [
  id,
  new RegExp(def.rules.map(([cls, src]) => `(?<${cls}>${src})`).join("|"), `g${def.flags || ""}`),
]));

// Wrap recognised tokens of already-escaped code in themed spans.
function hlCode(escCode, lang) {
  const re = HL_RE[lang];
  if (!re) return escCode;
  re.lastIndex = 0;
  let out = "", last = 0, m;
  while ((m = re.exec(escCode))) {
    if (!m[0]) { re.lastIndex++; continue; } // never spin on a zero-length match
    out += escCode.slice(last, m.index);
    const cls = HL_CLASSES.find((c) => m.groups[c] !== undefined);
    out += `<span class="hl-${cls}">${m[0]}</span>`;
    last = re.lastIndex;
  }
  return out + escCode.slice(last);
}

const MD_FENCE = /^\s{0,3}(`{3,}|~{3,})\s*([A-Za-z0-9+#._-]*)\s*$/;
const MD_HEAD = /^\s{0,3}(#{1,6})\s+(.*)$/;
const MD_HR = /^\s{0,3}([-*_])(?:\s*\1){2,}\s*$/;
const MD_QUOTE = /^\s{0,3}&gt;\s?(.*)$/; // ">" is already escaped at this point
const MD_UL = /^\s{0,3}[-*+]\s+(.*)$/;
const MD_OL = /^\s{0,3}\d{1,9}[.)]\s+(.*)$/;
const MD_INLINE = /`([^`\n]+)`|\[([^\]\n]*)\]\([^)\n]*\)|\*\*([^*\n]+)\*\*|__([^_\n]+)__|\*([^*\n]+)\*|_([^_\n]+)_/g;

function mdInline(s) {
  MD_INLINE.lastIndex = 0;
  let out = "", last = 0, m;
  while ((m = MD_INLINE.exec(s))) {
    out += s.slice(last, m.index);
    if (m[1] !== undefined) out += `<code class="md-inline-code">${m[1]}</code>`;
    else if (m[2] !== undefined) out += `<span class="md-link">${m[2]}</span>`; // label only, no target
    else if (m[3] !== undefined || m[4] !== undefined) out += `<strong>${m[3] ?? m[4]}</strong>`;
    else out += `<em>${m[5] ?? m[6]}</em>`;
    last = MD_INLINE.lastIndex;
  }
  return out + s.slice(last);
}

// ---- Code detection: a prompt that is plainly source code should look like code ----
// Markdown rules would eat its punctuation (a leading "-", "#" or "*" turns into a
// list, a heading or italics), so such a prompt is rendered as one highlighted block.
// The heuristic keys on three things, all cheap:
//   1. a language cue — a construct only that language writes (fn/def/SELECT/<tag>),
//   2. shape — most non-empty lines end in code punctuation or are indented,
//   3. no prose — not a single line reads like a sentence in running text.
// A cue marked strong stands on its own (a shebang, a whole JSON document, an SQL
// statement in the usual upper case) and skips the shape test, which those write too
// few punctuated line ends to pass; every other cue needs the shape as well.
// Markdown structure (a list, a quote, a fence) vetoes the whole thing, and so does a
// single line: when in doubt this returns "" and the text stays ordinary Markdown.
const CODE_CUES = [
  ["json", /^\s*\{[\s\S]*"[^"\n]*"\s*:[\s\S]*\}\s*$|^\s*\[\s*[{["\d-][\s\S]*\]\s*$/, true],
  ["bash", /^\s*#!\/\S/, true],
  ["sql", /\bSELECT\b[\s\S]*\bFROM\b|\bINSERT\s+INTO\b|\bCREATE\s+TABLE\b|\bUPDATE\b[\s\S]*\bSET\b/, true],
  ["html", /<\/[a-z][\w-]*>|<[a-z][\w-]*(?:\s[^<>]*)?\/>/i, false],
  ["css", /(?:^|\n)[^\n{}]{1,80}\{[^}]*[\w-]+\s*:[^;}]+;/, false],
  ["py", /(?:^|\n)[ \t]*(?:def\s+\w+\s*\(|class\s+\w+\s*[:(]|from\s+[\w.]+\s+import\b|import\s+\w+\s*$)/m, false],
  ["rust", /\b(?:fn\s+\w+\s*[(<]|let\s+mut\s+\w+|impl\s+\w+|pub\s+(?:fn|struct|enum)\b|use\s+[\w:]+;)/, false],
  ["js", /\b(?:function\s*\w*\s*\(|=>\s*[{(]|(?:const|let|var)\s+\w+\s*=|console\.\w+\(|export\s+(?:default|const|function)\b)/, false],
  ["bash", /(?:^|\n)\s*(?:sudo|apt|npm|npx|git|curl|docker|chmod)\s+\S/, false],
];
// Markdown a code block would never contain. "#" is missing on purpose: it is a
// comment in half the languages here and would veto every commented snippet.
const MD_STRUCTURE = /^\s{0,3}(?:[-*+]\s|\d{1,9}[.)]\s|>\s|```|~~~|\|)/m;
// A line that carries code punctuation where prose never would.
const CODE_LINE = /[;{}([,:=]\s*$|^\s*(?:[}\])]|\/\/|\/\*|#|--|<\/|["'`])/;
// Five or more words ending in a full stop, with no code punctuation: running prose.
const PROSE_LINE = /^[^;{}=<>|]*\s\S+\s\S+\s\S+\s\S+[^;{}=<>|]*[.!?:]$/;

function detectCodeLang(text) {
  const s = text.trim();
  // The highlighter's own ceiling: past it a block renders plain anyway.
  if (!s || s.length > HL_MAX_CHARS || MD_STRUCTURE.test(s)) return "";
  const lines = s.split("\n").filter((l) => l.trim());
  if (lines.length < 2) return "";
  const cue = CODE_CUES.find(([, re]) => re.test(s));
  if (!cue) return "";
  if (!cue[2]) {
    const shaped = lines.filter((l) => CODE_LINE.test(l.trim()) || /^(?: {2,}|\t)/.test(l)).length;
    if (shaped * 2 < lines.length) return "";
  }
  if (lines.some((l) => PROSE_LINE.test(l.trim()))) return "";
  return cue[0];
}

function renderMarkdown(text) {
  // An unfenced code prompt is one block, not Markdown — see detectCodeLang.
  if (flag("codeAutoDetect")) {
    const lang = detectCodeLang(text);
    if (lang) {
      const esc = mdEsc(text.replace(/\r\n?/g, "\n"));
      return `<pre class="md-code"><code>${flag("codeHighlight") ? hlCode(esc, lang) : esc}</code></pre>`;
    }
  }
  const cut = text.length > MD_MAX_CHARS;
  const src = mdEsc((cut ? text.slice(0, MD_MAX_CHARS) : text).replace(/\r\n?/g, "\n"));
  const lines = src.split("\n");
  const out = [];
  let para = [], quote = [], list = "";
  // Prompt text lives on its line breaks, so a wrapped paragraph keeps them as <br>
  // instead of being reflowed into one run the way a prose renderer would.
  const flushPara = () => { if (para.length) { out.push(`<p>${para.map(mdInline).join("<br>")}</p>`); para = []; } };
  const flushQuote = () => { if (quote.length) { out.push(`<blockquote>${quote.map(mdInline).join("<br>")}</blockquote>`); quote = []; } };
  const flushList = () => { if (list) { out.push(`</${list}>`); list = ""; } };
  const flushAll = () => { flushPara(); flushQuote(); flushList(); };
  const highlight = flag("codeHighlight");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = MD_FENCE.exec(line);
    if (fence) {
      flushAll();
      const mark = fence[1][0];
      const lang = HL_ALIAS[fence[2].toLowerCase()] || "";
      const closes = (l) => { const s = l.trim(); return s.length >= 3 && !s.split("").some((c) => c !== mark); };
      const body = [];
      i++;
      while (i < lines.length && !closes(lines[i])) body.push(lines[i++]);
      const code = body.join("\n");
      const colour = lang && highlight && code.length <= HL_MAX_CHARS;
      out.push(`<pre class="md-code"><code>${colour ? hlCode(code, lang) : code}</code></pre>`);
      continue; // the loop's own i++ steps over the closing fence
    }
    if (!line.trim()) { flushAll(); continue; }
    const head = MD_HEAD.exec(line);
    if (head) { flushAll(); const n = head[1].length; out.push(`<h${n}>${mdInline(head[2])}</h${n}>`); continue; }
    if (MD_HR.test(line)) { flushAll(); out.push("<hr>"); continue; }
    const q = MD_QUOTE.exec(line);
    if (q) { flushPara(); flushList(); quote.push(q[1]); continue; }
    const ul = MD_UL.exec(line);
    const ol = ul ? null : MD_OL.exec(line);
    if (ul || ol) {
      flushPara(); flushQuote();
      const want = ul ? "ul" : "ol";
      if (list !== want) { flushList(); out.push(`<${want}>`); list = want; }
      out.push(`<li>${mdInline((ul || ol)[1])}</li>`);
      continue;
    }
    flushQuote(); flushList();
    para.push(line.trim());
  }
  flushAll();
  if (cut) out.push(`<p class="md-trunc">${mdEsc(t("mdTruncated"))}</p>`);
  return out.join("");
}

// Replace layout: the rendered pane stands in for the textarea until the user clicks
// into it. `mdEditing` is that moment — everywhere else the prompt reads as Markdown.
let mdEditing = false;
let mdTimer = 0;
let mdLastText = null;

function renderMdPreview(force) {
  const box = modal.mdPreview;
  if (!box) return;
  const s = modal.text.value;
  if (!force && s === mdLastText) return;
  mdLastText = s;
  // The only innerHTML in this feature, and it is fed a string assembled purely from
  // escaped slices plus literal tags — see renderMarkdown.
  box.innerHTML = renderMarkdown(s);
}
function scheduleMdPreview() {
  // Nothing to draw into while the pane is hidden — but mark it stale, so the next
  // time it is shown it is rebuilt from the current text instead of a stale render.
  if (modal.mdPreview.classList.contains("hidden")) { mdLastText = null; return; }
  clearTimeout(mdTimer);
  // A plain timer, deliberately not requestAnimationFrame: a window that paints no
  // frames (occluded, minimised to the tray, restored from it) never runs a rAF
  // callback, and the pane would then sit there empty.
  mdTimer = setTimeout(() => renderMdPreview(), 60);
}

// Bring toolbar, textarea and preview in line with the flags and the picked layout.
// When `rotating`, the variants list IS the editing surface, so neither the
// textarea nor the rendered pane may stay in flow. Both used to survive it — the
// clear-X wrapper as an empty 120px box (the gap above the colour row) and, with
// on, the preview still rendering entry 1, which read as a stray field.
function syncMdUi(textVisible, rotating = false) {
  const on = textVisible && !rotating && flag("mdPreview");
  if (!on) mdEditing = false;
  // ONE field: the rendered pane stands in for the textarea and hands it back on a
  // click. Two panes side by side was the second thing on screen that did the same
  // job — there is no layout option any more.
  modal.mdWrap.classList.toggle("hidden", !on || mdEditing);
  // Hide the WRAPPER, not just the textarea: the clear-X wrapper stays in flow with
  // an empty box and pushes the rendered pane down, so the field appeared to jump
  // 19px every time it handed over.
  const field = modal.text.closest(".clear-wrap-ta") || modal.text;
  if (textVisible) {
    const hide = rotating || (on && !mdEditing);
    field.classList.toggle("hidden", hide);
    modal.text.classList.toggle("hidden", hide);
    // Tied to `rotating` alone, not the full `hide` -- the mic still dictates
    // into modal.text's value while its rendered Markdown preview is what's on
    // screen, and keeping the row's own visibility stable across THAT swap is
    // what keeps the field from jumping vertically when it hands over.
    modal.voiceRow.classList.toggle("hidden", rotating);
  }
  // Fill the pane whenever the feature is on, visible or not: the first paint must
  // never depend on the pane already being on screen, or switching to it shows a
  // blank box until the next keystroke.
  clearTimeout(mdTimer);
  if (on) renderMdPreview(true);
}

// Word-level diff between two texts via a classic LCS backtrack — no
// dependency, the whole thing is a couple dozen lines. O(n*m) on word count, so
// a pathologically huge pasted text falls back to plain text (DIFF_WORD_CAP)
// rather than freezing the editor while it opens the history panel.
let DIFF_WORD_CAP = 4000; // expert-tunable
function diffWords(oldText, newText) {
  const tokenize = (s) => (s || "").split(/(\s+)/).filter((t) => t !== "");
  const a = tokenize(oldText);
  const b = tokenize(newText);
  const n = a.length, m = b.length;
  // Cap on the PRODUCT n*m (the actual DP cost), not each side independently --
  // a long old text diffed against a short new one (or vice versa) is cheap either way.
  if (n * m > DIFF_WORD_CAP * DIFF_WORD_CAP) return null;
  const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const out = [];
  let i = 0, j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) { out.push({ type: "same", text: a[i] }); i++; j++; }
    else if (dp[i + 1][j] >= dp[i][j + 1]) { out.push({ type: "del", text: a[i] }); i++; }
    else { out.push({ type: "add", text: b[j] }); j++; }
  }
  while (i < n) out.push({ type: "del", text: a[i++] });
  while (j < m) out.push({ type: "add", text: b[j++] });
  return out;
}
// Text nodes only — never innerHTML on prompt content, same rule the Markdown
// preview follows (a prompt's own text is fully user-controlled).
function renderDiffInto(el, tokens) {
  el.textContent = "";
  for (const t of tokens) {
    if (t.type === "same") {
      el.appendChild(document.createTextNode(t.text));
      continue;
    }
    const span = document.createElement("span");
    span.className = t.type === "add" ? "diff-add" : "diff-del";
    span.textContent = t.text;
    el.appendChild(span);
  }
}

// ---- Version history: render the collapsible list in the editor ----
let HISTORY_PREVIEW_LEN = 80; // expert-tunable
// A snapshot's full entry list — index 0 is the base text, 1..n are rotation variants.
const versionEntries = (v) => (v.variants?.length ? [v.text || "", ...v.variants] : [v.text || ""]);
// The editor's current, unsaved state — the "newer" side of the newest row's diff.
const liveEntries = () => (modalState?.rotateEntries?.length ? [...modalState.rotateEntries] : [modal.text.value || ""]);
async function loadHistory(id) {
  const body = $("modal-history-body");
  if (!body || !id) return;
  body.innerHTML = "";
  let list = [];
  try { list = await invoke("list_versions", { promptId: id }); } catch (_) {}
  // A slower response for a since-abandoned prompt must not land in the editor
  // after it has already moved on to a different one.
  if (modalState?.id !== id) return;
  if (!list.length) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t("historyEmpty");
    body.appendChild(e);
    return;
  }
  list.forEach((v, idx) => {
    // Diff against whatever came right after this snapshot — the live editor
    // state for the newest row, the next-newer snapshot for every other one. Each
    // side is the FULL entry list (rotation variants included), not just entry 1.
    const olderEntries = versionEntries(v);
    const newerEntries = idx === 0 ? liveEntries() : versionEntries(list[idx - 1]);
    const newerName = idx === 0 ? modal.name.value : list[idx - 1].name;
    const row = document.createElement("div");
    row.className = "hist-row";
    const head = document.createElement("div");
    head.className = "hist-head";
    const time = document.createElement("span");
    time.className = "hist-time";
    time.textContent = new Date(v.ts * 1000).toLocaleString(LANG);
    const prev = document.createElement("span");
    prev.className = "hist-preview";
    prev.textContent = (v.name ? v.name + " — " : "") + olderEntries[0].slice(0, HISTORY_PREVIEW_LEN);
    head.append(time, prev);
    if (olderEntries.length > 1) {
      const chip = document.createElement("span");
      chip.className = "hist-entries";
      chip.textContent = t("historyEntriesN").replace("{n}", String(olderEntries.length));
      head.appendChild(chip);
    }
    // Explicit expand chevron (collapsed by default) → reveals the COMPLETE text.
    const expand = document.createElement("button");
    expand.type = "button";
    expand.className = "hist-expand";
    expand.title = t("historyExpandToggle");
    expand.setAttribute("aria-expanded", "false");
    expand.innerHTML = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M6 9l6 6 6-6"/></svg>';
    const restore = document.createElement("button");
    restore.type = "button";
    restore.className = "ghost-btn hist-restore";
    restore.textContent = t("historyRestore");
    restore.addEventListener("click", async () => {
      try {
        const p = await invoke("restore_version", { versionId: v.id });
        // The editor may have moved on to a different (or no) prompt while this
        // invoke was in flight — never write a stale restore into the wrong modal.
        if (modalState?.id !== id) return;
        if (p) {
          const pIdx = prompts.findIndex((x) => x.id === p.id);
          if (pIdx >= 0) prompts[pIdx] = p;
          modal.name.value = p.name;
          modal.text.value = p.text;
          // The open editor must pick up the restored rotation entries too, or the
          // next save writes the still-open (stale) entries back over them.
          modalState.rotateEntries = (p.variants || []).length ? [p.text, ...p.variants] : [];
          modalState.rotateMode = p.rotate_mode || "";
          renderRotateUi();
          updateLenCounter();
          toast(t("historyRestored"));
          await renderGrid(true);
          loadHistory(id); // the restore itself added a fresh snapshot
        }
      } catch (err) {
        const s = String(err);
        toast(s === "save-failed" || s === "snapshot-failed" ? t("persistFailed") : s);
      }
    });
    head.append(expand, restore);
    const full = document.createElement("div");
    full.className = "hist-full hidden";
    // Building this (word-diffing every entry) is expensive at long prompts /
    // many rotation entries; each row starts collapsed, so defer it to first expand
    // and cache the result -- most rows are never opened at all.
    let fullBuilt = false;
    const buildFull = () => {
      if (fullBuilt) return;
      fullBuilt = true;
      if (v.name !== newerName) {
        const nameRow = document.createElement("div");
        nameRow.className = "hist-name-change";
        const label = document.createElement("span");
        label.className = "hist-name-label";
        label.textContent = t("historyNameChanged") + ": ";
        const oldSpan = document.createElement("span");
        oldSpan.className = "diff-del";
        oldSpan.textContent = v.name || "";
        const newSpan = document.createElement("span");
        newSpan.className = "diff-add";
        newSpan.textContent = newerName || "";
        nameRow.append(label, oldSpan, document.createTextNode(" → "), newSpan);
        full.appendChild(nameRow);
      }
      if (olderEntries.every((e) => !e) && newerEntries.every((e) => !e)) {
        const hint = document.createElement("div");
        hint.className = "hint";
        hint.textContent = t("historyNoText");
        full.appendChild(hint);
      } else {
        const maxLen = Math.max(olderEntries.length, newerEntries.length);
        for (let i = 0; i < maxLen; i++) {
          const hasOlder = i < olderEntries.length;
          const hasNewer = i < newerEntries.length;
          const block = document.createElement("div");
          if (maxLen > 1) {
            block.className = "hist-entry-block";
            const num = document.createElement("div");
            num.className = "hist-entry-num";
            num.textContent = String(i + 1);
            block.appendChild(num);
          }
          const content = document.createElement("div");
          content.className = "hist-entry-content";
          if (!hasNewer) {
            const span = document.createElement("span");
            span.className = "diff-del";
            span.textContent = olderEntries[i];
            content.appendChild(span);
          } else if (!hasOlder) {
            const span = document.createElement("span");
            span.className = "diff-add";
            span.textContent = newerEntries[i];
            content.appendChild(span);
          } else if (!flag("historyDiff")) {
            content.textContent = olderEntries[i]; // word diff switched off, show the plain old text
          } else {
            const tokens = diffWords(olderEntries[i], newerEntries[i]);
            if (tokens) renderDiffInto(content, tokens);
            else content.textContent = olderEntries[i]; // DIFF_WORD_CAP fallback: plain text
          }
          block.appendChild(content);
          full.appendChild(block);
        }
      }
    };
    const toggle = () => {
      const open = !full.classList.toggle("hidden");
      if (open) buildFull();
      expand.setAttribute("aria-expanded", String(open));
      row.classList.toggle("open", open);
    };
    expand.addEventListener("click", toggle);
    prev.addEventListener("click", toggle); // clicking the preview also expands
    row.append(head, full);
    body.appendChild(row);
  });
}

// ---- Usage statistics dashboard (opened from the expert menu) ----
function statSection(text) {
  const h = document.createElement("div");
  h.className = "stat-section";
  h.textContent = text;
  return h;
}
// Collapsible section head (chevron) shared by the backup and statistics panels: it
// folds `contentEl` away and remembers the state under `stateKey`.
function collapsibleHead(labelText, contentEl, stateKey) {
  const open = optFlag(stateKey);
  const h = document.createElement("button");
  h.type = "button";
  h.className = "stat-section backup-list-head" + (open ? " open" : "");
  const s = document.createElement("span");
  s.textContent = labelText;
  h.appendChild(s);
  h.insertAdjacentHTML("beforeend", '<svg class="collapse-chevron" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M6 9l6 6 6-6"/></svg>');
  contentEl.classList.toggle("hidden", !open);
  h.addEventListener("click", () => {
    const o = !contentEl.classList.toggle("hidden");
    h.classList.toggle("open", o);
    saveUiState(stateKey, o);
  });
  return h;
}
// Label + bounded number field, as used by the backup and statistics panels.
// Clearing the field means "back to the default" — it is committed on blur, so
// an empty field mid-typing is left alone.
// `after` redraws the caller's own panel once a reset restores the default,
// same job renderExpert() does for the main expert menu's rows.
function numRow(labelKey, key, min, max, onSet, extraClass, after) {
  const l = document.createElement("label");
  l.className = "field backup-set-row" + (extraClass ? " " + extraClass : "");
  const s = document.createElement("span"); s.textContent = t(labelKey);
  const i = document.createElement("input");
  i.type = "number"; i.min = min; i.max = max; i.className = "modal-input"; i.value = String(val(key));
  const def = EXPERT_VALUES[key]?.def ?? min;
  const commit = async (raw) => {
    // Number(raw) || def treated a typed 0 as falsy and silently fell back to
    // the default -- a GFS retention tier (min 0 = "none") could never be set to 0.
    const n = Number(raw);
    const v = raw === "" || !Number.isFinite(n) ? def : Math.max(min, Math.min(max, n));
    i.value = String(v);
    // The clear-X wipes the field programmatically, so only an `input` event
    // brings it back once the default has been written into the field again.
    i.dispatchEvent(new Event("input", { bubbles: true }));
    await onSet(v);
  };
  i.addEventListener("change", () => commit(i.value.trim()));
  i.addEventListener("blur", () => { if (i.value.trim() === "") commit(""); });
  const right = document.createElement("span"); right.className = "numrow-right";
  const resetBtn = rowResetBtn("values", key, after);
  if (resetBtn) right.append(resetBtn);
  right.append(i);
  l.append(s, right);
  return l;
}

// Shared with the backup list's per-prompt type chip -- one classifier's
// worth of i18n keys, not two.
const PROMPT_TYPE_LABELS = { text: "statTypeText", image: "filterImage", video: "filterVideo", pdf: "filterPdf", file: "filterFile" };

async function renderStatsPanel(body) {
  // Feature toggle + (when on) the top-N field share one row.
  const on = flag("usageStats");
  // Fetch BEFORE clearing — otherwise the panel paints empty during the await,
  // which read as a flash on every re-render.
  let s = null;
  if (on) {
    try { s = await invoke("usage_stats", { topN: Math.round(val("statsTopN")) }); }
    catch (e) { errToast(e); return; }
  }
  body.innerHTML = "";
  // ONE collapsible head for the whole section, its own switch included — folding it
  // away used to leave the "record statistics" row behind as an orphan under a
  // heading with nothing under it.
  const root = body;
  const panel = document.createElement("div");
  panel.className = "stats-panel-body";
  const head = collapsibleHead(t("statsSection"), panel, "statsPanelOpen");
  head.classList.add("backup-split"); // the rule that separates it from the backups above
  root.append(head, panel);
  body = panel;
  const headRow = document.createElement("div"); headRow.className = "stats-head-row";
  const sw = document.createElement("label"); sw.className = "field switch-field";
  const swl = document.createElement("span"); swl.textContent = t("statsEnable");
  const swi = document.createElement("input"); swi.type = "checkbox"; swi.className = "switch"; swi.checked = on;
  // Root, not body — body was reassigned to the INNER panel just above, and
  // re-rendering with it would build a new stats-panel-body inside the previous
  // one instead of replacing it, nesting one deeper on every toggle.
  swi.addEventListener("change", async () => { await saveBackupSetting("flag", "usageStats", swi.checked); renderStatsPanel(root); });
  sw.append(swl, swi); headRow.appendChild(withTip(sw, "usageStats"));
  if (on) {
    // Top-N param (how many prompts the "most used" chart lists), right of the toggle.
    headRow.appendChild(numRow("valStatsTopN", "statsTopN", 5, 100, async (v) => {
      await saveBackupSetting("value", "statsTopN", v);
      renderStatsPanel(root); // see the toggle handler above
    }, "stats-topn-row", () => renderStatsPanel(root)));
  }
  body.appendChild(headRow);
  if (!on) return;
  const nf = (n) => Number(n).toLocaleString(LANG);
  const nf1 = (n) => Number(n).toLocaleString(LANG, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  const cards = document.createElement("div");
  cards.className = "stat-cards";
  const card = (label, value) => {
    const c = document.createElement("div"); c.className = "stat-card";
    const v = document.createElement("div"); v.className = "stat-val"; v.textContent = value;
    const l = document.createElement("div"); l.className = "stat-label"; l.textContent = label;
    c.append(v, l); return c;
  };
  // Three rows of four: what the library holds, how long/heavily used the prompts
  // are, and the raw copy activity.
  cards.append(
    // — Inventory —
    card(t("statTotalPrompts"), nf(s.total_prompts)),
    card(t("statWithMedia"), nf(prompts.filter((p) => hasImage(p) || p.file_path || p.copy_image).length)),
    card(t("statWithVars"), nf(prompts.filter((p) => extractVars(p.text || "").length > 0).length)),
    card(t("statViews"), nf((settings.views || []).length)),
    // — Content + intensity —
    card(t("statTopCopies"), s.top && s.top.length ? nf(s.top[0].count) : "—"),
    card(t("statAvg"), s.used_count ? nf1(s.avg_copies) : "—"),
    card(t("statChars"), nf(s.total_chars)),
    card(t("statAvgLen"), s.total_prompts ? nf(Math.round(s.total_chars / s.total_prompts)) : "—"),
    // — Copy activity —
    card(t("statTotalCopies"), nf(s.total_copies)),
    card(t("statCopies7"), s.history_on ? nf(s.copies7) : "—"),
    card(t("statCopies30"), s.history_on ? nf(s.copies30) : "—"),
    card(t("statPerDay"), s.history_on ? nf1(s.copies30 / 30) : "—"),
  );
  body.appendChild(cards);
  // Last 14 days as a small bar chart — copies7/copies30 above are just two flat
  // totals, this fills the gap between them without a day/week toggle (deliberately
  // scoped down from "day or week" to daily only).
  if (s.history_on && s.daily && s.daily.length) {
    body.appendChild(statSection(t("statDaily")));
    const chart = document.createElement("div");
    chart.className = "stat-daily-chart";
    const maxD = Math.max(...s.daily.map((d) => d.count), 1);
    for (const d of s.daily) {
      const bar = document.createElement("span");
      bar.className = "stat-daily-bar" + (d.count === 0 ? " zero" : "");
      bar.style.height = `${Math.max(3, (d.count / maxD) * 100)}%`;
      const dateLabel = new Date(d.ts * 1000).toLocaleDateString(LANG, { weekday: "short", day: "numeric", month: "short" });
      bar.title = `${dateLabel}: ${nf(d.count)}`;
      chart.appendChild(bar);
    }
    body.appendChild(chart);
  }
  // Overview (2 columns wide) next to the per-view breakdown (1 column).
  const infoRow = (label, value) => {
    const r = document.createElement("div"); r.className = "stat-view-row";
    const l = document.createElement("span"); l.textContent = label;
    const v = document.createElement("span"); v.className = "stat-view-count"; v.textContent = value;
    r.append(l, v); return r;
  };
  const info = document.createElement("div");
  info.className = "stat-views";
  // Only what the cards above do NOT already show — the most-used count is a card
  // and a bar below it, so it stays out of here.
  if (s.recent_ts) info.appendChild(infoRow(t("statRecent"), `${s.recent_name} · ${new Date(s.recent_ts * 1000).toLocaleDateString(LANG)}`));
  // Longest and shortest: the name on the label line, its size on a line of its own —
  // one row could not hold name, characters and tokens without cutting one off.
  const sizeRow = (label, name, chars) => {
    info.appendChild(infoRow(label, name));
    const r = document.createElement("div");
    r.className = "stat-view-row stat-sub-row";
    const l = document.createElement("span");
    l.textContent = `${nf(chars)} ${t("lenChars")} · ~${nf(Math.ceil(chars / 4))} ${t("lenTokens")}`;
    r.appendChild(l);
    info.appendChild(r);
  };
  if (s.longest_chars) sizeRow(t("statLongest"), s.longest_name, s.longest_chars);
  const split = document.createElement("div"); split.className = "stat-overview-split";
  const ovCol = document.createElement("div"); ovCol.className = "stat-overview-main";
  ovCol.append(statSection(t("statOverview")), info);
  split.appendChild(ovCol);
  if (s.per_view.length) {
    const pvCol = document.createElement("div"); pvCol.className = "stat-overview-side";
    const pv = document.createElement("div"); pv.className = "stat-views";
    for (const v of s.per_view) {
      const row = document.createElement("div"); row.className = "stat-view-row";
      const name = document.createElement("span"); name.textContent = v.name;
      const cnt = document.createElement("span"); cnt.className = "stat-view-count"; cnt.textContent = nf(v.count);
      row.append(name, cnt);
      pv.appendChild(row);
    }
    pvCol.append(statSection(t("statPerView")), pv);
    split.appendChild(pvCol);
  } else {
    split.classList.add("no-side"); // nothing to put in the right third
  }
  body.appendChild(split);
  // Type breakdown as proportional bars.
  if (s.types.length) {
    body.appendChild(statSection(t("statTypes")));
    const list = document.createElement("div"); list.className = "stat-bars";
    const maxT = Math.max(...s.types.map((x) => x.count), 1);
    for (const ty of s.types) {
      const row = document.createElement("div"); row.className = "stat-bar-row";
      const name = document.createElement("span"); name.className = "stat-bar-name"; name.textContent = t(PROMPT_TYPE_LABELS[ty.name] || ty.name);
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
  wireClearButtons(root); // clear-X on the top-N field
}
function jumpToPrompt(name) {
  // The settings and the backups dialog stay open underneath: the library is only
  // raised above them, so closing it lands back on the statistics it was opened from.
  document.body.classList.add("library-front");
  $("library-btn").click(); // resets + opens the library
  libQuery = name || "";
  $("library-q").value = libQuery;
  renderLibrary();
}
// Inline expert panels (backups + statistics) — filled on demand by renderExpert.

// ---- Duplicate finder ----
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
  catch (e) { errToast(e); body.innerHTML = ""; return; }
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
      await invoke("ignore_dups", { ids: g.members.map((m) => m.id) }).catch((e) => errToast(e));
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
  } catch (e) { toast(String(e) === "save-failed" ? t("persistFailed") : String(e)); return; }
  renderViews();
  await renderGrid(true);
  scanDupes();
}
// Import hook: after an import, flag near-duplicates with a toast linking to the tool.
async function maybeDupImportToast() {
  if (!flag("dupFinder") || !flag("dupImportCheck")) return;
  try {
    const groups = await invoke("find_duplicates", { threshold: Math.round(val("dupThreshold")) });
    if (groups.length) toast(t("dupImportToast").replace("{n}", String(groups.length)));
  } catch (_) {}
}
EXPERT_ACTIONS["dupes-open"] = openDupes;

// ---- Trash ----
// Deleting a prompt (grid tile, library row, editor, multi-select) always moves it
// here first — delete_prompt/batch_prompts("delete") no longer hard-remove. This
// dialog is the only place it can come back from, or be purged for good.
let trashList = [];
async function openTrash() {
  $("trash-modal").classList.remove("hidden");
  try { trashList = await invoke("list_trash"); } catch (e) { errToast(e); trashList = []; }
  renderTrashList();
}
function renderTrashList() {
  const body = $("trash-body");
  body.innerHTML = "";
  if (!trashList.length) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t("trashEmpty");
    body.appendChild(e);
    return;
  }
  for (const p of trashList) {
    const row = document.createElement("div");
    row.className = "clip-row";
    const head = document.createElement("span");
    head.className = "clip-time";
    head.textContent = `${libLabel(p)} · ${new Date(p.trashed_at * 1000).toLocaleDateString(LANG)}`;
    const prev = document.createElement("div");
    prev.className = "clip-preview";
    prev.textContent = p.file_path || p.text || "";
    const actions = document.createElement("div");
    actions.className = "clip-actions";
    const restore = document.createElement("button");
    restore.type = "button"; restore.className = "ghost-btn"; restore.textContent = t("trashRestore");
    restore.addEventListener("click", () => restoreTrashItem(p.id));
    const purge = document.createElement("button");
    purge.type = "button"; purge.className = "ghost-btn"; purge.textContent = t("trashPurge");
    purge.addEventListener("click", () => purgeTrashItem(p.id, libLabel(p)));
    actions.append(restore, purge);
    row.append(head, prev, actions);
    body.appendChild(row);
  }
}
async function restoreTrashItem(id) {
  let st;
  try { st = await invoke("restore_prompt", { id }); } catch (e) { toast(String(e) === "save-failed" ? t("persistFailed") : String(e)); return; }
  prompts = st.prompts;
  settings = st.settings;
  trashList = trashList.filter((p) => p.id !== id);
  renderTrashList();
  renderViews();
  await renderGrid(true);
  if (!libraryEl.classList.contains("hidden")) renderLibrary();
}
async function purgeTrashItem(id, name) {
  const ok = await confirmDialog({ title: t("trashPurge"), message: `${t("trashPurgeConfirm")}\n\n${name}`, confirmLabel: t("trashPurge") });
  if (!ok) return;
  try { trashList = await invoke("purge_prompt", { id }); } catch (e) { errToast(e); return; }
  renderTrashList();
}
async function emptyTrashAll() {
  if (!trashList.length) return;
  const ok = await confirmDialog({ title: t("trashTitle"), message: t("trashEmptyConfirm").replace("{n}", String(trashList.length)), confirmLabel: t("trashPurge") });
  if (!ok) return;
  try { trashList = await invoke("empty_trash"); } catch (e) { errToast(e); return; }
  renderTrashList();
}
EXPERT_ACTIONS["trash-open"] = openTrash;
// Runs the same sweep on demand -- unlike the silent background sweep,
// an explicit click always gets an answer, even "nothing to clean up".
EXPERT_ACTIONS["rec-sweep-now"] = async () => {
  try {
    const [count, bytes] = await invoke("sweep_recordings_now");
    toast(count > 0 ? t("recSweepDone").replace("{n}", String(count)).replace("{v}", fmtBytes(bytes)) : t("recSweepNothing"));
  } catch (e) {
    errToast(e);
  }
};

// ---- OCR + voice language pack management, one shared dialog ----
// #ocr-langs-modal serves both OCR and voice dictation language packs -- same
// markup, same flow (Windows-reported capability list, elevated listing/install),
// just pointed at a different set of Tauri commands and settings keys. `kind`
// ("ocr" | "speech") selects which.
// *_capabilities() lists every capability Windows knows about (installed AND
// not) with size/version straight from Windows -- a different list from
// ocrLangsCache/voiceLangsCache above, which are only the installed set actually
// used to pick a recognition language.
const LANG_CMDS = {
  ocr: {
    capabilities: "ocr_capabilities", elevate: "ocr_capabilities_elevate",
    elevatedResult: "ocr_capabilities_elevated_result",
    install: "ocr_install_language", installResult: "ocr_install_result",
    languages: "ocr_languages",
  },
  speech: {
    capabilities: "speech_capabilities", elevate: "speech_capabilities_elevate",
    elevatedResult: "speech_capabilities_elevated_result",
    install: "speech_install_language", installResult: "speech_install_result",
    languages: "voice_languages",
  },
};
// Per-kind dialog state, kept independent so switching which dialog is open
// never loses the other's in-flight scan or install polls.
const LANG_DIALOG = {
  ocr: { cache: [], checked: 0, installTimers: {}, installPending: {}, installErrors: {}, elevTimer: null, elevRunning: false, elevLeft: 60 },
  speech: { cache: [], checked: 0, installTimers: {}, installPending: {}, installErrors: {}, elevTimer: null, elevRunning: false, elevLeft: 60 },
};
let langDialogKind = "ocr"; // which kind #ocr-langs-modal currently displays
function openLangsModal(kind) {
  langDialogKind = kind;
  $("ocr-langs-title").textContent = t(kind === "speech" ? "voiceLangsManage" : "ocrLangsManage");
  $("ocr-langs-hint").textContent = t(kind === "speech" ? "voiceLangsHint" : "ocrLangsHint");
  $("ocr-langs-modal").classList.remove("hidden");
  const st = LANG_DIALOG[kind];
  // A scan started on a previous open (or from the elevation retry button)
  // may still be running server-side -- resume polling it instead of the dialog
  // silently doing nothing until the user re-elevates from scratch.
  if (st.elevRunning) { renderLangsLoading(kind); pollElevatedCaps(kind); }
  else loadLangCaps(kind, false);
}
function closeLangsModal() {
  $("ocr-langs-modal").classList.add("hidden");
  const st = LANG_DIALOG[langDialogKind];
  // Installs and an elevated scan are OS processes and keep running regardless
  // -- only the polling loop (and the DOM it repaints) needs to stop while
  // nobody is looking. elevRunning itself is left set so reopening resumes.
  for (const id of Object.values(st.installTimers)) clearTimeout(id);
  st.installTimers = {};
  if (st.elevTimer) { clearTimeout(st.elevTimer); st.elevTimer = null; }
}
async function loadLangCaps(kind, force) {
  const st = LANG_DIALOG[kind];
  if (!st.cache.length || force) renderLangsLoading(kind);
  try {
    st.cache = await invoke(LANG_CMDS[kind].capabilities, { force: !!force });
    st.checked = Math.floor(Date.now() / 1000);
  } catch (e) {
    // Listing is supposed to need no elevated rights, but some Windows builds
    // require it even for this read-only query -- an honest inline reason
    // (plus a retry, 162) beats a blank dialog or a generic error toast. 177:
    // ANY failure here comes back this same way now (backend no longer tries
    // to classify by HResult/message text, which varies by machine/locale) --
    // "code|raw text" so the raw PowerShell/DISM reason can still be shown.
    const s = String(e);
    if (s.startsWith("ocr-caps-need-elevation")) {
      renderLangsRetry(kind, t("ocrLangsNeedElevation"), s.slice(s.indexOf("|") + 1));
    } else errToast(e);
    return;
  }
  renderLangsBody(kind);
}
// Runs the capability listing through the UAC prompt on machines that refuse it
// to a normal user. Same shape as an install: explain first, launch, then poll
// the result the elevated process writes.
async function startElevatedCapsScan(kind) {
  const ok = await confirmDialog({
    title: t("ocrElevateTitle"),
    message: t("ocrCapsElevateInfo"),
    confirmLabel: t("ocrLangsElevateBtn"),
    danger: false,
  });
  if (!ok) return;
  const st = LANG_DIALOG[kind];
  renderLangsLoading(kind);
  try {
    await invoke(LANG_CMDS[kind].elevate);
  } catch (e) {
    // A launch failure also gets the notice+retry pair, not a dead end.
    renderLangsRetry(kind, String(e) === "ocr-elevate-cancelled" ? t("ocrElevateCancelled") : String(e));
    return;
  }
  st.elevRunning = true;
  st.elevLeft = 60;
  pollElevatedCaps(kind);
}
// Polls the elevated process's result file a few seconds apart. Re-entrant: can
// be called again after the dialog was closed and reopened without losing
// the two-minute budget already spent (elevLeft lives on LANG_DIALOG, not in a
// closure local, so it survives that close/reopen).
function pollElevatedCaps(kind) {
  const st = LANG_DIALOG[kind];
  const tick = async () => {
    st.elevTimer = null;
    let res;
    try { res = await invoke(LANG_CMDS[kind].elevatedResult); }
    catch (e) { st.elevRunning = false; renderLangsRetry(kind, String(e)); return; }
    if (res == null) {
      if (--st.elevLeft <= 0) { st.elevRunning = false; renderLangsRetry(kind, t("ocrLangsNeedElevation")); return; }
      st.elevTimer = setTimeout(tick, 2000);
      return;
    }
    st.elevRunning = false;
    st.cache = res;
    st.checked = Math.floor(Date.now() / 1000);
    renderLangsBody(kind);
  };
  st.elevTimer = setTimeout(tick, 2000);
}
function renderLangsLoading(kind) {
  if (langDialogKind !== kind) return;
  const body = $("ocr-langs-body");
  body.innerHTML = "";
  const loading = document.createElement("div");
  loading.className = "hint";
  loading.textContent = t("ocrLangsScanning");
  body.appendChild(loading);
}
// Notice text and its retry button are always built together, in one
// place -- a failure (launch or the two-minute poll timeout alike) can no
// longer replace the body with text only and strand the user without a way
// to try again. `details` is the raw PowerShell/DISM text behind a
// translated reason like `message` -- shown collapsed so it never crowds out
// the actual instruction, but never silently dropped either.
function renderLangsRetry(kind, message, details) {
  if (langDialogKind !== kind) return;
  const body = $("ocr-langs-body");
  body.innerHTML = "";
  const note = document.createElement("div");
  note.className = "hint";
  note.textContent = message;
  body.appendChild(note);
  if (details && details.trim()) {
    const disc = document.createElement("details");
    disc.className = "lang-error-details";
    const summary = document.createElement("summary");
    summary.textContent = t("ocrLangsErrorDetails");
    disc.appendChild(summary);
    const pre = document.createElement("div");
    pre.className = "lang-error-details-text";
    pre.textContent = details.trim();
    disc.appendChild(pre);
    body.appendChild(disc);
  }
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "ghost-btn";
  btn.textContent = t("ocrLangsElevateBtn");
  btn.addEventListener("click", () => startElevatedCapsScan(kind));
  body.appendChild(btn);
}
function renderLangsBody(kind) {
  if (langDialogKind !== kind) return; // a stale async result for a dialog that moved on
  const st = LANG_DIALOG[kind];
  const body = $("ocr-langs-body");
  body.innerHTML = "";
  for (const cap of st.cache) body.appendChild(langRow(kind, cap));
  $("ocr-langs-checked").textContent = st.checked
    ? t("ocrLangsLastChecked").replace("{t}", fmtCopyTime(st.checked))
    : t("ocrLangsNeverChecked");
}
function langRow(kind, cap) {
  const st = LANG_DIALOG[kind];
  const row = document.createElement("div");
  row.className = "clip-row ocr-lang-row lang-row";
  const main = document.createElement("div");
  main.className = "lang-row-main";
  const name = document.createElement("div");
  name.className = "lang-row-name";
  name.textContent = cap.label;
  const sub = document.createElement("div");
  sub.className = "lang-row-sub";
  sub.textContent = `${cap.tag} · ${cap.version || "—"}`;
  main.append(name, sub);
  const installed = cap.state === "Installed";
  const size = installed ? cap.install_size : cap.download_size;
  const sizeEl = document.createElement("div");
  sizeEl.className = "lang-row-size";
  // Windows does not always report a size -- never guess one, show the dash instead.
  sizeEl.textContent = size == null ? "—" : fmtBytes(size);
  const chip = document.createElement("span");
  chip.className = installed ? "lang-row-chip installed" : "lang-row-chip";
  chip.textContent = installed ? t("ocrLangsInstalled") : t("ocrLangsNotInstalled");
  const actions = document.createElement("div");
  actions.className = "clip-actions";
  row.append(main, sizeEl, chip, actions);
  if (installed) {
    const check = document.createElement("span");
    check.className = "ocr-lang-check";
    check.textContent = "✓";
    check.title = t("ocrLangsInstalled");
    actions.appendChild(check);
    return row;
  }
  const note = st.installErrors[cap.name];
  if (note) {
    const hint = document.createElement("span");
    hint.className = "ocr-lang-note";
    // A string is the backend's own reason (a real install failure); `true` is
    // the calm "you declined the rights prompt" case, which gets its own text.
    hint.textContent = typeof note === "string" ? note : t("ocrElevateCancelled");
    actions.appendChild(hint);
  }
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "ghost-btn";
  const busy = !!st.installTimers[cap.name] || !!st.installPending[cap.name];
  btn.disabled = busy;
  // The size sits INSIDE the button label on purpose -- nobody should start a
  // download blind.
  btn.textContent = busy
    ? t("ocrLangsInstalling")
    : t("ocrLangsDownloadBtn").replace("{size}", cap.download_size == null ? "—" : fmtBytes(cap.download_size));
  btn.addEventListener("click", () => startLangInstall(kind, cap));
  actions.appendChild(btn);
  return row;
}
// Every path that can lead to an elevated install goes through here first:
// explain the language, its size and that Windows will ask for admin rights
// once, BEFORE that prompt ever appears. No exceptions.
async function startLangInstall(kind, cap) {
  const st = LANG_DIALOG[kind];
  const size = cap.download_size == null ? "—" : fmtBytes(cap.download_size);
  const ok = await confirmDialog({
    title: t("ocrElevateTitle"),
    message: t("ocrElevateInfo").replace("{lang}", cap.label).replace("{size}", size),
    confirmLabel: t("ocrElevateInstall"),
    danger: false,
  });
  if (!ok) return;
  delete st.installErrors[cap.name];
  // Marks the row busy for the WHOLE gap, including the elevated install()
  // call itself (which blocks until the user answers the UAC prompt) -- set
  // synchronously, before any await, so a second click on the still-enabled
  // button can't start a second concurrent elevated install for the same
  // language while the first one is still waiting on Windows.
  st.installPending[cap.name] = true;
  renderLangsBody(kind);
  try {
    await invoke(LANG_CMDS[kind].install, { name: cap.name });
  } catch (e) {
    delete st.installPending[cap.name];
    // A declined UAC prompt is not an error: a calm inline
    // note on the row, no error tone, the row stays "not installed".
    if (String(e) === "ocr-elevate-cancelled") st.installErrors[cap.name] = true;
    else errToast(e);
    // Every branch above just deleted installPending -- the row rendered disabled
    // ("Installing...") right before the await must be repainted regardless of
    // which branch ran, or the button stays stuck disabled until the dialog is
    // closed and reopened.
    renderLangsBody(kind);
    return;
  }
  delete st.installPending[cap.name];
  // Arm the poller (marks the row busy) BEFORE repainting, or the render still
  // reads installTimers[cap.name] as empty and shows an enabled Download button --
  // a second click then starts a second UAC-elevated install.
  pollLangInstall(kind, cap.name);
  renderLangsBody(kind);
}
// *_install_language() only resolves once the elevated process has LAUNCHED,
// not once it finished -- poll the result file ourselves, a few seconds apart,
// until it stops being null.
// A result file that never becomes readable (permanently corrupt, or wiped
// from under the install) used to look exactly like "still running" forever --
// polled every 3s with no way out. ~10 minutes is generous for even a large,
// slow-connection language pack, so hitting this is always a real stuck state.
const MAX_LANG_INSTALL_POLLS = 200;
function pollLangInstall(kind, name) {
  const st = LANG_DIALOG[kind];
  if (st.installTimers[name]) return;
  let attempts = 0;
  // setTimeout re-armed after each result, not setInterval -- the callback
  // awaits an invoke, and a reply slower than the 3s tick would otherwise overlap
  // itself (two install_result calls, or two clearInterval/repaint races).
  const tick = async () => {
    attempts++;
    let res;
    try { res = await invoke(LANG_CMDS[kind].installResult, { name }); } catch (_) { res = undefined; }
    if (res == null) {
      if (attempts >= MAX_LANG_INSTALL_POLLS) {
        delete st.installTimers[name];
        st.installErrors[name] = t("ocrInstallResultTimeout");
        renderLangsBody(kind);
        return;
      }
      st.installTimers[name] = setTimeout(tick, 3000); // still running (or a transient probe failure)
      return;
    }
    delete st.installTimers[name];
    if (res.ok) {
      // Clear both caches so the new pack is selectable immediately, no restart.
      invoke(LANG_CMDS[kind].languages).then((langs) => {
        if (kind === "speech") voiceLangsCache = langs || []; else ocrLangsCache = langs || [];
      }).catch(() => {});
      await loadLangCaps(kind, true); // force -- also drops the backend's own cache
      return;
    }
    // A real failure (e.g. Windows Home / policy blocked) -- the reason belongs
    // in the row, never a silent failure. The backend's own internal keys are
    // translated here; anything else is a raw Windows exception message, shown
    // exactly as Windows worded it since there is no way to pre-translate that.
    st.installErrors[name] = res.error === "install-result-unreadable" ? t("ocrInstallResultUnreadable") : (res.error || true);
    renderLangsBody(kind);
  };
  st.installTimers[name] = setTimeout(tick, 3000);
}
// Thin OCR-flavoured wrapper so the expert-action entry below keeps working
// unchanged -- the generalization lives entirely behind `kind`.
function openOcrLangsModal() { openLangsModal("ocr"); }
function openVoiceLangsModal() { openLangsModal("speech"); }
EXPERT_ACTIONS["ocr-langs-open"] = openOcrLangsModal;
EXPERT_ACTIONS["voice-langs-open"] = openVoiceLangsModal;
// "ocr-no-language"/"voice-no-language" always have the same fix: open this
// dialog. Used at every call site that would otherwise show them as a bare errToast().
function ocrErrToast(err) {
  if (String(err) === "ocr-no-language") {
    toast(t("ocrNoLanguageMsg"), { label: t("ocrLangsManage"), onClick: openOcrLangsModal });
    return;
  }
  errToast(err);
}

// ---- Voice dictation session control ----
let voiceSessionId = null;
let voiceToken = 0; // frontend-chosen, sent to voice_start and echoed on every event -- the only identity events are matched on
let voiceState = "idle"; // idle | arming | listening
let voiceTargetEl = null;
let voiceActiveBtn = null;
let voiceInterim = null; // {start, len} inside voiceTargetEl.value, or null
let voiceSuppressInput = false; // true while a programmatic .value write is in flight
let voiceResolvedLang = ""; // BCP-47 tag voice_start actually recognized with, for command post-processing
let voiceArmCancelled = false; // stopVoice() was called before voice_start's invoke() resolved
// The just-ended session's target, for one late-arriving voice-text that WinRT's
// unordered ResultGenerated/Completed handlers let land after voice-stopped
// already fired -- delivered instead of dropped, but only within the grace
// window and only to ITS OWN target, never adopted by whatever session (if any)
// has started in the meantime. Not an expert setting -- internal timing.
let voicePrev = null; // {token, el, caret, lang, hotkey, endedAt} | null
const VOICE_LATE_GRACE_MS = 3000;

// The two-letter code voice.js's command tables are keyed by, derived from
// the BCP-47 tag voice_start resolved to -- NOT the UI's own display
// language, which can easily differ from what is actually being recognized
// (e.g. a German UI with only an English speech pack installed). Falls back
// to the UI language before a session has resolved one at all (the hotkey
// path's first partial can in rare cases race ahead of it).
function voiceLangCode() {
  return (voiceResolvedLang || LANG).slice(0, 2).toLowerCase();
}
function voiceButtonsAll() { return [$("voice-btn"), modal.voiceBtn].filter(Boolean); }
function setVoiceBtnState(activeBtn, cls) {
  for (const b of voiceButtonsAll()) {
    b.classList.remove("is-arming", "is-listening");
    b.setAttribute("aria-pressed", "false");
    b.title = t("voiceBtnTitle");
  }
  if (!activeBtn || !cls) return;
  activeBtn.classList.add(cls);
  if (cls === "is-listening") {
    activeBtn.setAttribute("aria-pressed", "true");
    activeBtn.title = t("voiceStopTitle");
  }
}
// The button stays in its "arming" in-between state (so a fast double-click
// cannot start a second session) until the first backend event, of any kind,
// proves the session is actually alive.
function resolveVoiceArming() {
  if (voiceState === "arming") {
    voiceState = "listening";
    setVoiceBtnState(voiceActiveBtn, "is-listening");
  }
}
function endVoiceUi() {
  if (voiceTargetEl) {
    voicePrev = {
      token: voiceToken,
      el: voiceTargetEl,
      caret: voiceTargetEl.selectionEnd ?? null,
      lang: voiceResolvedLang,
      hotkey: voiceTargetEl === voiceHotkeyBuffer,
      endedAt: Date.now(),
    };
  }
  voiceState = "idle";
  voiceSessionId = null;
  voiceTargetEl = null;
  voiceInterim = null;
  voiceResolvedLang = "";
  setVoiceBtnState(null, null);
  voiceActiveBtn = null;
}
async function startVoice(target, btn) {
  if (voiceState !== "idle" || !target) return;
  voiceState = "arming";
  voiceTargetEl = target;
  voiceActiveBtn = btn;
  voiceInterim = null;
  voiceArmCancelled = false;
  voiceToken = (voiceToken % 1e9) + 1;
  setVoiceBtnState(btn, "is-arming");
  let res;
  try {
    res = await invoke("voice_start", {
      lang: txt("voiceLang") || "auto",
      autoStopSec: flag("voiceAutoStop") ? Math.round(val("voiceAutoStopSec")) : 0,
      maxSec: Math.round(val("voiceMaxMin")) * 60,
      interim: flag("voiceInterim"),
      token: voiceToken,
    });
  } catch (e) {
    endVoiceUi();
    routeVoiceError(e);
    return;
  }
  const { id, lang } = res;
  // A cancel that happened while this call was in flight (voice_start's own
  // spawn_blocking round trip is not instant) has to stop the session it
  // just now learned the id of -- stopVoice() itself had nothing to send.
  if (voiceArmCancelled) {
    voiceArmCancelled = false;
    invoke("voice_stop", { id }).catch(() => {});
    // Only reset the UI if no backend event has already adopted this session
    // (see voiceEventIsMine) -- if one has, its own listener owns the reset.
    if (voiceSessionId === null && voiceState === "arming") endVoiceUi();
    return;
  }
  if (voiceSessionId === null) voiceSessionId = id;
  voiceResolvedLang = lang || voiceResolvedLang;
}
async function stopVoice() {
  if (voiceState === "arming" && voiceSessionId === null) {
    // Nothing to send yet -- voice_start hasn't resolved with an id. Mark the
    // cancel and let startVoice() finish the job the moment it does.
    voiceArmCancelled = true;
    return;
  }
  if (!voiceSessionId) return;
  try { await invoke("voice_stop", { id: voiceSessionId }); } catch (_) {}
  // The UI resets from the voice-stopped event the backend always sends back,
  // not here -- the button never claims "off" before the session (and its
  // mic) has actually finished.
}
// True if a token-tagged backend event belongs to the session currently
// starting or running. voiceToken is chosen by the frontend and sent to
// voice_start itself, so it is known before any event can possibly arrive --
// unlike the backend-assigned id, it is never adopted from an event. The two
// WinRT handlers (ResultGenerated, Completed) are not ordered against each
// other, so a session that already reported voice-stopped can still deliver
// one more voice-text after that -- its token no longer matches here and it
// falls to deliverLateVoiceText instead of being silently misattributed to
// whatever session (if any) is now starting.
function voiceEventIsMine(payload) {
  return voiceState !== "idle" && !!payload && payload.token === voiceToken;
}
// A late voice-text for the session endVoiceUi just closed out (see voicePrev)
// -- delivered to ITS OWN target within VOICE_LATE_GRACE_MS instead of being
// dropped, and never touches voiceState/voiceSessionId/the buttons, since
// there is no live session left for this to belong to.
function deliverLateVoiceText(payload) {
  const prev = voicePrev;
  if (!prev || !payload || payload.token !== prev.token) return;
  if (Date.now() - prev.endedAt >= VOICE_LATE_GRACE_MS) return;
  voicePrev = null;
  const text = payload.text || "";
  if (!text) return;
  const processed = applyVoiceCommands(text, {
    punct: flag("voicePunct"),
    autoCaps: flag("voiceAutoCaps"),
    lang: (prev.lang || LANG).slice(0, 2).toLowerCase(),
  });
  if (prev.hotkey) { deliverVoiceHotkeyText(processed); return; }
  const el = prev.el;
  if (!el || el.classList.contains("hidden")) return;
  const atEnd = txt("voiceInsert") === "end";
  const at = atEnd ? el.value.length : (prev.caret ?? el.value.length);
  insertLateVoiceText(el, processed, at);
}
function wireVoiceButton(btn, targetFn) {
  if (!btn) return;
  btn.addEventListener("click", () => {
    if (voiceState !== "idle") { stopVoice(); return; }
    startVoice(targetFn(), btn);
  });
}
async function openVoiceOnlineConsent() {
  const ok = await confirmDialog({
    title: t("voiceOnlineTitle"),
    message: t("voiceOnlineInfo"),
    confirmLabel: t("voiceOnlineAllow"),
    danger: false,
  });
  if (!ok) return;
  try { await invoke("voice_enable_online_speech"); } catch (e) { errToast(e); return; }
  settings.ui_flags = { ...(settings.ui_flags || {}), voiceOnlineOk: true };
  applyFlags();
}
function routeVoiceError(err) {
  const code = String(err);
  // No error toast for this one -- the fix is always the same, so the first
  // click just opens the dialog that offers it (the OCR button does the same).
  if (code === "voice-no-language") { openVoiceLangsModal(); return; }
  // voice-privacy-off is the same missing consent voice-online-required already
  // guards against -- Windows' own registry value, so the same dialog fixes it.
  if (code === "voice-online-required" || code === "voice-privacy-off") { openVoiceOnlineConsent(); return; }
  if (code === "voice-denied") {
    toast(t("voiceDeniedMsg"), { label: t("voiceOpenPrivacy"), onClick: () => invoke("open_privacy_mic").catch(() => {}) });
    return;
  }
  const key = { "voice-busy": "voiceErrBusy", "voice-failed": "voiceErrFailed" }[code];
  errToast(key ? t(key) : code);
}
// Joins a new recognized chunk onto whatever the target already holds: one
// space, except right after a newline or an opening bracket, and never a
// second space next to one already there.
function voiceJoiner(before) {
  if (!before) return "";
  if (/[\n(]$/.test(before)) return "";
  if (/\s$/.test(before)) return "";
  return " ";
}
// interim: true for a live voice-partial update (replaced by the next update
// or the final result), false for the committed voice-text result. The marker
// (voiceInterim) is what makes a partial replaceable in place instead of
// piling up duplicate text on every update; tracked per the target element,
// discarded the moment the user types or moves the cursor themselves (see the
// discard listeners below), at which point it just stays as plain text.
function insertVoiceText(text, interim) {
  const el = voiceTargetEl;
  if (!el || el.classList.contains("hidden")) return; // its field disappeared mid-session
  const value = el.value;
  let start, replaceLen;
  if (voiceInterim) {
    start = voiceInterim.start;
    replaceLen = voiceInterim.len;
  } else {
    const atEnd = txt("voiceInsert") === "end";
    start = atEnd ? value.length : (el.selectionStart ?? value.length);
    replaceLen = 0;
  }
  const joiner = voiceInterim ? "" : voiceJoiner(value.slice(0, start));
  const newValue = value.slice(0, start) + joiner + text + value.slice(start + replaceLen);
  const newStart = start + joiner.length;
  voiceSuppressInput = true;
  el.value = newValue;
  el.selectionStart = el.selectionEnd = newStart + text.length;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  voiceSuppressInput = false;
  voiceInterim = interim ? { start: newStart, len: text.length } : null;
}
// Sibling of insertVoiceText for deliverLateVoiceText's one-shot delivery --
// deliberately does not touch voiceInterim/voiceSuppressInput, since there is
// no live session left for the discard-tracking those belong to.
function insertLateVoiceText(el, text, at) {
  const value = el.value;
  const joiner = voiceJoiner(value.slice(0, at));
  el.value = value.slice(0, at) + joiner + text + value.slice(at);
  el.selectionStart = el.selectionEnd = at + joiner.length + text.length;
  el.dispatchEvent(new Event("input", { bubbles: true }));
}
// The recognized text is still ours to lay claim to right up until the user
// visibly acts on the field themselves -- then the interim span is just text.
function voiceWatchDiscard(el) {
  if (!el) return;
  const discard = () => { if (voiceInterim && voiceTargetEl === el) voiceInterim = null; };
  el.addEventListener("input", () => { if (!voiceSuppressInput) discard(); });
  el.addEventListener("click", discard);
  el.addEventListener("keydown", (e) => {
    if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"].includes(e.key)) discard();
  });
}
// 131's global hotkey has no visible field to dictate into (the window is
// never brought forward), so it targets this detached textarea instead --
// never added to the DOM, but a real element, so it rides the exact same
// insertVoiceText/voiceJoiner pipeline as the two visible buttons rather than
// a second copy of it.
const voiceHotkeyBuffer = document.createElement("textarea");
async function deliverVoiceHotkeyText(text) {
  if (!text.trim()) return;
  const target = txt("voiceHotkeyTarget") || "clipboard";
  if (target === "input") {
    inputEl.value = inputEl.value ? `${inputEl.value}\n${text}` : text;
    autoGrow(inputEl);
    return;
  }
  // clipboard and paste both copy first -- paste then adds the same
  // paste-into-previous-app step a prompt hotkey already uses.
  try { await invoke("copy_text", { text }); } catch (e) { errToast(e); return; }
  if (target === "paste") await autoPasteNow(null);
  else toast(t("copied"));
}

// ---- Backups & restore ----
// A backup folder name as a readable local date + time ("29.07.2026 18:15" in German,
// and whatever the picked language writes elsewhere).
function prettyStamp(name) {
  const dt = parseStamp(name);
  if (dt) return `${dt.toLocaleDateString(LANG)} ${dt.toLocaleTimeString(LANG, { hour: "2-digit", minute: "2-digit" })}`;
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
    await invoke("set_ui_flag", { key, enabled: value }).catch((e) => errToast(e));
    applyFlags();
  } else {
    settings.ui_values = { ...(settings.ui_values || {}), [key]: value };
    await invoke("set_ui_value", { key, value }).catch((e) => errToast(e));
    applyValues();
  }
}
// Backups live in their own dialog (opened from settings), not in the expert menu:
// the page carries settings, diagnostics and a restore list and needs the room.
// What is typed into the backup-password field but not saved yet. The panel is
// rebuilt on every setting change, which would otherwise throw the input away
// mid-typing. Cleared on save and when the dialog closes.
let backupPwDraft = "";
function openBackups() {
  $("backup-modal").classList.remove("hidden");
  renderBackupPanel($("backup-panel"));
  renderStatsPanel($("stats-panel"));
}
function closeBackups() {
  $("backup-modal").classList.add("hidden");
  backupPwDraft = ""; // never keep an unsaved password around after the dialog is gone
}

// Remembered UI state (which panel sections are expanded). Kept in ui_flags so it
// survives a restart; the key is not in OPT_FLAG_LABELS, so it never shows up as an
// expert toggle and applyFlags() ignores it.
// The write failure is swallowed on purpose, everywhere this is called (including
// tourSeen): every key here is a one-off memory of what the user already saw or
// last had open, never data they typed. Losing a write just means the panel
// starts collapsed again, or the tour reopens once more, next launch -- an error
// toast at that exact moment (right as a tour finishes, or a panel closes) would
// be more disruptive than the failure it reports, for something that quietly
// self-corrects on its own.
async function saveUiState(key, on) {
  settings.ui_flags = { ...(settings.ui_flags || {}), [key]: on };
  await invoke("set_ui_flag", { key, enabled: on }).catch(() => {});
}
async function renderBackupPanel(body) {
  // Fetch BEFORE clearing: emptying the panel and only then awaiting lets the browser
  // paint the empty state for a frame, which showed up as a visible flash on every
  // re-render (backup now, value change …).
  let backups, hadPw;
  try { [backups, hadPw] = await Promise.all([invoke("list_backups"), invoke("has_backup_password").catch(() => false)]); }
  catch (e) { errToast(e); return; }
  body.innerHTML = "";
  const nf = (n) => Number(n).toLocaleString(LANG);
  const card = (l, v) => {
    const c = document.createElement("div"); c.className = "stat-card";
    const vv = document.createElement("div"); vv.className = "stat-val"; vv.textContent = v;
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
  const collapseHead = collapsibleHead;

  const fmtInterval = (h) => (h < 24 ? `${h} h` : `${h / 24} d`); // ≥24h shown in days

  const switchRow = (labelText, on, onChange, tipKey) => {
    const l = document.createElement("label"); l.className = "field switch-field";
    const s = document.createElement("span"); s.textContent = labelText;
    const i = document.createElement("input"); i.type = "checkbox"; i.className = "switch"; i.checked = on;
    i.addEventListener("change", () => onChange(i.checked));
    l.append(s, i);
    return tipKey ? withTip(l, tipKey) : l;
  };

  // ---- Settings ---- Left column: making a backup and when it happens by itself.
  // Right column: how many of them survive.
  body.appendChild(statSection(t("backupSettings")));
  const set = document.createElement("div"); set.className = "backup-settings backup-cols";
  const left = document.createElement("div"); left.className = "backup-col";
  const right = document.createElement("div"); right.className = "backup-col";
  set.append(left, right);

  const nowBtn = document.createElement("button"); nowBtn.type = "button"; nowBtn.className = "ghost-btn backup-now-btn"; nowBtn.textContent = t("backupNow");
  nowBtn.addEventListener("click", async () => {
    try { await withTaskbarBusy("backup", () => invoke("create_backup", { keep: Math.round(val("backupKeep")) })); toast(t("backupDone")); }
    catch (e) { errToast(e); }
    await saveUiState("backupListOpen", true); // reveal the fresh backup right away
    renderBackupPanel(body);
  });
  left.appendChild(nowBtn);
  const openFolderBtn = document.createElement("button");
  openFolderBtn.type = "button";
  openFolderBtn.className = "ghost-btn";
  openFolderBtn.textContent = t("backupOpenFolder");
  openFolderBtn.addEventListener("click", () => invoke("open_backups_folder").catch((e) => errToast(e)));
  left.appendChild(openFolderBtn);
  left.appendChild(switchRow(t("flagAutoBackup"), flag("autoBackup"), async (on) => {
    await saveBackupSetting("flag", "autoBackup", on);
    renderBackupPanel(body);
  }));
  // The interval only means something while automatic backups run.
  if (flag("autoBackup")) {
    const iv = document.createElement("label"); iv.className = "field backup-set-row backup-interval-row";
    const ivl = document.createElement("span"); ivl.textContent = t("valBackupInterval");
    const ivs = document.createElement("select"); ivs.className = "modal-input backup-interval";
    for (const h of [6, 12, 24, 48, 168]) { const o = document.createElement("option"); o.value = String(h); o.textContent = fmtInterval(h); ivs.appendChild(o); }
    ivs.value = String(val("backupIntervalH"));
    ivs.addEventListener("change", async () => { await saveBackupSetting("value", "backupIntervalH", Number(ivs.value)); renderBackupPanel(body); });
    attachSelectPicker(ivs); // themed popup instead of the native OS dropdown
    const ivRight = document.createElement("span"); ivRight.className = "numrow-right";
    const ivReset = rowResetBtn("selects", "backupIntervalH", () => renderBackupPanel(body));
    if (ivReset) ivRight.append(ivReset);
    ivRight.append(ivs);
    iv.append(ivl, ivRight); left.appendChild(iv);
    left.appendChild(numRow("valBackupStartDelay", "backupStartDelayS", 0, 300, async (v) => {
      await saveBackupSetting("value", "backupStartDelayS", v);
      renderBackupPanel(body);
    }, "backup-start-delay-row", () => renderBackupPanel(body)));
  }

  // Right column — retention. A plain number field replaces the three preset
  // buttons; "Smart retention" is a switch above it that turns the field grey
  // (backend ignores backupKeep while it is on) and activates the tiers below.
  const gfsOn = optFlag("backupGfs");
  right.appendChild(switchRow(t("flagBackupGfs"), gfsOn, async (on) => {
    await saveBackupSetting("flag", "backupGfs", on);
    renderBackupPanel(body);
  }, "tipBackupGfs"));
  const keepRow = numRow("valBackupKeep", "backupKeep", 1, 50, async (v) => {
    await saveBackupSetting("value", "backupKeep", v);
    renderBackupPanel(body); // without this the reset icon never appears live
  }, "backup-keep-row", () => renderBackupPanel(body));
  if (gfsOn) {
    keepRow.classList.add("is-disabled");
    keepRow.querySelector("input").disabled = true;
  }
  right.appendChild(keepRow);
  // Both retention states share ONE slot so picking a tile never moves the rows below
  // it. The tier grid is always BUILT and merely parked when smart retention is off:
  // a CSS min-height guessing its height was three pixels short of what the fields
  // really measure, and every font, UI scale and translation moved that target again.
  // Day / week beside each other, month / year underneath — four small fields. The
  // tiers replace the plain count entirely (the backend ignores it while smart
  // retention is on), so the count the user set stays untouched for when it is off.
  const slot = document.createElement("div"); slot.className = "backup-retention-slot";
  right.appendChild(slot);
  const tiers = document.createElement("div");
  tiers.className = "backup-tier-grid" + (gfsOn ? "" : " is-parked");
  // Re-renders like every other row in this panel, so a tier's reset icon
  // appears/disappears live instead of only after some unrelated change.
  const onTier = (key) => async (v) => { await saveBackupSetting("value", key, v); renderBackupPanel(body); };
  const rerenderPanel = () => renderBackupPanel(body);
  tiers.appendChild(numRow("valBackupDaily", "backupDaily", 0, 30, onTier("backupDaily"), null, rerenderPanel));
  tiers.appendChild(numRow("valBackupWeekly", "backupWeekly", 0, 52, onTier("backupWeekly"), null, rerenderPanel));
  tiers.appendChild(numRow("valBackupMonthly", "backupMonthly", 0, 60, onTier("backupMonthly"), null, rerenderPanel));
  tiers.appendChild(numRow("valBackupYearly", "backupYearly", 0, 20, onTier("backupYearly"), null, rerenderPanel));
  slot.appendChild(tiers);
  // Optional backup password. Without one, backups use a fixed key and restore on any
  // PC; with one they need exactly this password. The stored password is never sent
  // back to the UI — the eye only reveals what was just typed in this session.
  const pwRow = document.createElement("div"); pwRow.className = "field backup-pw-row";
  // Dots as the PLACEHOLDER, never as a value: they show that a password exists
  // without putting anything revealable — or accidentally saveable — in the field.
  const PW_DOTS = "••••••••";
  const pwLabel = document.createElement("span"); pwLabel.textContent = t("backupPasswordLabel");
  const pwWrap = document.createElement("div"); pwWrap.className = "backup-pw-wrap";
  const pwIn = document.createElement("input");
  pwIn.type = "password"; pwIn.className = "modal-input"; pwIn.maxLength = 128;
  pwIn.autocomplete = "new-password";
  let hasPw = hadPw; // placeholder + the remove button follow this (syncPwButtons)
  pwIn.value = backupPwDraft; // survives the re-render a setting change triggers
  pwIn.addEventListener("input", () => { backupPwDraft = pwIn.value; });
  const eye = document.createElement("button");
  eye.type = "button"; eye.className = "icon-btn backup-pw-eye";
  eye.title = t("showPassword");
  eye.setAttribute("aria-label", t("showPassword"));
  eye.innerHTML = EYE_SVG;
  // Reveals what is typed right now. A stored password is never loaded back into
  // the field, so there is nothing secret to leak here.
  eye.addEventListener("click", () => {
    const show = pwIn.type === "password";
    pwIn.type = show ? "text" : "password";
    eye.title = t(show ? "hidePassword" : "showPassword");
    eye.setAttribute("aria-label", t(show ? "hidePassword" : "showPassword"));
  });
  // Saving happens ONLY through this button — leaving the field must never change
  // the password by accident.
  const save = document.createElement("button");
  save.type = "button";
  save.className = "ghost-btn backup-pw-save";
  save.textContent = t("save");
  // Removing is its own button below — an empty field here saves nothing, so the
  // password can never be dropped by an absent-minded click on Save.
  const remove = document.createElement("button");
  const syncPwButtons = () => {
    pwIn.placeholder = hasPw ? PW_DOTS : t("backupPasswordPh");
    remove.classList.toggle("hidden", !hasPw);
  };
  save.addEventListener("click", async () => {
    const pw = pwIn.value;
    if (!pw) { toast(t("backupPasswordEmpty")); return; }
    try {
      await invoke("set_backup_password", { password: pw });
      toast(t("backupPasswordWarn"));
      hasPw = true;
      backupPwDraft = "";
      pwIn.value = "";
      pwIn.type = "password";
      syncPwButtons();
    } catch (e) { toast(String(e) === "save-failed" ? t("persistFailed") : String(e)); }
  });
  remove.type = "button";
  remove.className = "ghost-btn danger-btn backup-pw-remove";
  remove.textContent = t("backupPasswordRemove");
  remove.addEventListener("click", async () => {
    const ok = await confirmDialog({
      title: t("backupPasswordRemove"),
      message: t("backupPasswordClearConfirm"),
      confirmLabel: t("backupPasswordRemove"),
      icon: LOCK_SVG,
    });
    if (!ok) return;
    try {
      await invoke("set_backup_password", { password: "" });
      toast(t("backupPasswordCleared"));
      hasPw = false;
      backupPwDraft = "";
      pwIn.value = "";
      pwIn.type = "password";
      syncPwButtons();
    } catch (e) { toast(String(e) === "save-failed" ? t("persistFailed") : String(e)); }
  });
  syncPwButtons();
  pwWrap.append(pwIn, eye);
  // Field and both buttons stay on one line, so the row spans both columns —
  // neither column alone is wide enough for all three.
  const pwControls = document.createElement("div"); pwControls.className = "backup-pw-controls";
  pwControls.append(pwWrap, save, remove);
  pwRow.append(pwLabel, pwControls);
  set.appendChild(pwRow);
  // The consequence of forgetting it, spelled out where it's set — not just
  // in a toast after the fact, which is easy to miss.
  const pwWarn = document.createElement("p");
  pwWarn.className = "modal-hint backup-pw-hint";
  pwWarn.textContent = t("backupPasswordForgetWarn");
  set.appendChild(pwWarn);
  body.appendChild(set);

  // ---- Diagnostics (collapsible) ----
  const total = backups.reduce((s, b) => s + b.size, 0);
  const newest = backups.length ? parseStamp(backups[0].name) : null;
  const oldest = backups.length ? parseStamp(backups[backups.length - 1].name) : null;
  const sizes = backups.map((b) => b.size);
  const cards = document.createElement("div"); cards.className = "stat-cards";
  // Row 1: how many backups there are and what they cost in disk space.
  // Row 2: the time axis — how far back they reach and when the next one is due.
  cards.append(
    card(t("backupCount"), nf(backups.length)),
    card(t("backupTotalSize"), fmtBytes(total)),
    card(t("backupLargest"), backups.length ? fmtBytes(Math.max(...sizes)) : "—"),
    card(t("backupSmallest"), backups.length ? fmtBytes(Math.min(...sizes)) : "—"),
    card(t("backupSpan"), newest && oldest
      ? `${nf(Math.max(0, Math.round((newest - oldest) / 86400000)))} d`
      : "—"),
    dtCard(t("backupNext"), newest && flag("autoBackup") ? new Date(newest.getTime() + val("backupIntervalH") * 3600000) : null),
    dtCard(t("backupNewest"), newest),
    dtCard(t("backupOldest"), oldest),
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
      // What is actually in that snapshot — read from the snapshot itself, not guessed.
      const count = document.createElement("span"); count.className = "backup-count";
      count.textContent = b.prompts === null || b.prompts === undefined ? "—" : nf(b.prompts);
      count.title = t("backupPrompts");
      // Names are read straight from the snapshot (DPAPI-sealed exactly like
      // the live library — a local snapshot is never password-protected, only the
      // separate manual export-to-file has that option), so there is nothing to ask
      // for here. An empty list just means an older backup made before this existed.
      let namesBox = null;
      if (b.items && b.items.length) {
        namesBox = document.createElement("ul");
        namesBox.className = "backup-names-list hidden";
        for (const item of b.items) {
          const li = document.createElement("li"); li.className = "backup-name-row";
          const nm = document.createElement("span"); nm.className = "backup-name-row-name"; nm.textContent = item.name;
          const kind = document.createElement("span");
          kind.className = "backup-name-row-kind";
          kind.textContent = t(PROMPT_TYPE_LABELS[item.kind] || item.kind);
          const chars = document.createElement("span");
          chars.className = "backup-name-row-chars";
          chars.textContent = item.chars ? nf(item.chars) : "—";
          li.append(nm, kind, chars);
          namesBox.appendChild(li);
        }
        count.classList.add("backup-count-toggle");
        count.setAttribute("role", "button");
        count.tabIndex = 0;
        count.title = t("backupNamesToggle");
        const toggle = () => namesBox.classList.toggle("hidden");
        count.addEventListener("click", toggle);
        count.addEventListener("keydown", (e) => {
          if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggle(); }
        });
      }
      const size = document.createElement("span"); size.className = "backup-size"; size.textContent = fmtBytes(b.size);
      const restore = document.createElement("button"); restore.type = "button"; restore.className = "ghost-btn"; restore.textContent = t("backupRestore");
      restore.addEventListener("click", () => restoreBackup(b.name));
      const del = document.createElement("button"); del.type = "button"; del.className = "ghost-btn danger-btn"; del.textContent = t("delete");
      del.addEventListener("click", async () => {
        if (!armButton(del, `${t("delete")}?`)) { setTimeout(() => disarmButton(del, t("delete")), DISARM_MS); return; }
        await invoke("delete_backup", { name: b.name }).catch((e) => errToast(e));
        await saveUiState("backupListOpen", true); // stay in the list after delete
        renderBackupPanel(body);
      });
      row.append(name, count, size, restore, del);
      list.appendChild(row);
      if (namesBox) list.appendChild(namesBox);
    }
  }
  body.append(listHead, list);
  wireClearButtons(body); // clear-X on the number fields
}
async function restoreBackup(name) {
  // Both checked (the default) = the old full-file "all" restore; only one
  // checked = the new selective merge; the dialog itself refuses zero checked.
  const ok = await confirmDialog({
    title: t("backupRestore"), message: t("backupRestoreConfirm"), confirmLabel: t("backupRestore"),
    checkboxes: [
      { key: "data", label: t("backupRestoreData"), checked: true },
      { key: "settings", label: t("backupRestoreSettings"), checked: true },
    ],
  });
  if (!ok) return;
  const dataOn = $("confirm-cb-data").checked;
  const settingsOn = $("confirm-cb-settings").checked;
  const scope = dataOn && settingsOn ? "all" : dataOn ? "data" : "settings";
  // The same safety net as delete — restoring replaces the current
  // data just as permanently, so it follows the same "before delete" switch.
  await maybeBackupBeforeWipe("delete");
  try { await withTaskbarBusy("backup", () => invoke("restore_backup", { name, keep: Math.round(val("backupKeep")), scope })); }
  catch (e) { errToast(e); } // on success the app relaunches
}

// ---- Clipboard inbox ----
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
// The badge counts what the user has NOT looked at yet, so opening the inbox clears
// it. Inbox ids are row ids and only grow, which makes the highest one a cheap
// "seen up to here" marker that survives a restart.
const clipSeenId = () => Number(settings.ui_values?.clipSeenId) || 0;
const clipUnseen = (items) => items.filter((it) => Number(it.id) > clipSeenId()).length;
async function markClipSeen(items) {
  const top = items.reduce((m, it) => Math.max(m, Number(it.id) || 0), 0);
  if (top <= clipSeenId()) return;
  settings.ui_values = { ...(settings.ui_values || {}), clipSeenId: top };
  try { await invoke("set_ui_value", { key: "clipSeenId", value: top }); } catch (_) {}
}
async function refreshClipInbox() {
  if (!optFlag("clipWatcher")) { clipCount = 0; updateClipBadge(); return; }
  let items = [];
  try { items = await invoke("clip_inbox_list"); } catch (_) {}
  const open = !$("clip-modal").classList.contains("hidden");
  if (open) await markClipSeen(items); // arriving while the list is on screen counts as seen
  clipCount = clipUnseen(items);
  updateClipBadge();
  if (open) renderClipList(items);
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
    copy.addEventListener("click", () => { invoke("copy_text", { text: it.text }).then(() => toast(t("copied"))).catch((e) => errToast(e)); });
    const dismiss = document.createElement("button");
    dismiss.type = "button"; dismiss.className = "ghost-btn"; dismiss.textContent = t("clipDismiss");
    dismiss.addEventListener("click", async () => { await invoke("clip_inbox_dismiss", { id: it.id }).catch((e) => errToast(e)); refreshClipInbox(); });
    actions.append(save, copy, dismiss);
    row.append(time, prev, actions);
    list.appendChild(row);
  }
}
async function openClipInbox() {
  let items = [];
  try { items = await invoke("clip_inbox_list"); } catch (_) {}
  await markClipSeen(items); // everything on this list has now been seen
  clipCount = 0;
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
  // Only a text prompt, and only while the feature is on, ever offers rotation.
  modal.rotateToggleRow.classList.toggle("hidden", !textVisible || !flag("rotatingPrompts"));
  renderRotateUi();
  // rotationUiOn(), not rotationActive() — with the feature off but the prompt
  // still holding stored entries, the plain textarea is what must show (entry 0,
  // editable), or the editor is stuck showing a list with no way back to it (its
  // own toggle row is hidden too while the feature is off).
  const rotating = textVisible && rotationUiOn();
  modal.text.classList.toggle("hidden", !textVisible || rotating);
  modal.voiceRow.classList.toggle("hidden", !textVisible || rotating);
  const varsUi = textVisible && !rotating && flag("promptVars");
  modal.textBar.classList.toggle("hidden", !varsUi);
  modal.varsHint.classList.toggle("hidden", !varsUi);
  if (varsUi) renderVarHelper(); // chips + insert row follow their flags
  syncMdUi(textVisible, rotating); // preview toolbar + rendered pane
  updateLenCounter();
  // The variable helper is a reference, so it reopens collapsed too.
  modal.varsBody.classList.add("hidden");
  modal.varsHead.setAttribute("aria-expanded", "false");
  modal.textBar.classList.remove("open");
  // Tags/hotkey/history share one "Advanced options" collapsible, which always
  // reopens collapsed. The hotkey row exists for every prompt kind (a hotkey
  // copies whatever the prompt holds). Version history is offered for every existing
  // prompt kind — a snapshot also captures name-only changes on an image/file/colour
  // prompt, so it stays reachable there too. The outer section itself hides only if
  // none of the three apply.
  modal.hotkey.classList.toggle("hidden", !promptHotkeysOn());
  showHotkeyError("");
  const histOn = mode === "edit" && flag("promptHistory");
  $("modal-history").classList.toggle("hidden", !histOn);
  if (histOn) $("modal-history-body").innerHTML = "";
  modal.advanced.classList.toggle("hidden", !tagsOn() && !promptHotkeysOn() && !histOn);
  modal.advancedBody.classList.add("hidden");
  modal.advancedHead.setAttribute("aria-expanded", "false");
  modal.advanced.classList.remove("open");
  modal.root.classList.remove("history-open");
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
      const session = modalSession;
      modal.img.onerror = () => {
        modal.img.onerror = null;
        loadFilePreview(previewPath, modalState?.pdfPage || 0).then((d) => {
          if (d && session === modalSession) modal.img.src = d;
        });
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
  // The two only actually need telling apart when both a stored preview and a
  // real attachment exist side by side -- otherwise there is nothing to confuse.
  const previewAttachHint = $("modal-preview-attach-hint");
  previewAttachHint.classList.toggle("hidden", !(hasPreview && filePath));
  const fileIconEl = $("modal-file-icon");
  fileIconEl.title = t("previewAttachHint");
  if (filePath) {
    const isPdf = PDF_EXT.test(filePath);
    $("modal-file-icon").innerHTML = isPdf ? ICON_PDF : ICON_FILE;
    modal.fileName.textContent = filePath.split(/[\\/]/).pop();
    modal.fileName.title = filePath;
    // Shown 1-based (page 1, 2, ...) — pdfPage itself stays the 0-based index
    // pdf_preview expects, so the two never need converting anywhere else.
    modal.pdfPage.classList.toggle("hidden", !isPdf);
    if (isPdf) modal.pdfPage.value = String((modalState?.pdfPage || 0) + 1);
  } else {
    modal.pdfPage.classList.add("hidden");
  }
}

// ---- Rotating prompts (editor) ----
// While active, modalState.rotateEntries holds EVERY entry (index 0 = what would
// otherwise be modal.text's own value) — kept separate from modal.text so the two
// never fight over which one is "the" source of truth while the switch is on.
const rotationActive = () => (modalState?.rotateEntries || []).length > 0;
// rotationActive() alone answers "does this prompt have stored entries" (used
// by confirmModal to decide what to preserve on save); this answers "is the rotate
// list actually the thing shown right now" — false while the feature itself is off,
// even with entries still stored. DISPLAY decisions (which surface to show/hide)
// must use this one, or a prompt edited while the feature is off gets its rotate
// list shown with no way back to the textarea (its own toggle row is hidden too).
const rotationUiOn = () => rotationActive() && flag("rotatingPrompts");

function renderRotateUi() {
  if (!modalState) return;
  // UI-gated, same reasoning as syncModalImageUi -- the list must stay hidden
  // (in favour of the plain textarea) while the feature itself is off, even with
  // entries still stored on the prompt.
  const active = rotationUiOn();
  modal.rotateSwitch.checked = active;
  modal.rotateList.classList.toggle("hidden", !active);
  if (!active) return;
  renderRotateEntries();
  modal.rotateMode.value = modalState.rotateMode === "random" ? "random" : "order";
  refreshRotateNext();
  renderRotateAutoBtn();
}

// Mirrors the backend's auto_eligible minus the two feature gates —
// rotatingPrompts already gates this whole row (rotationUiOn), and rotateAuto
// (the feature switch, 93) is checked here so the button disables live if it
// gets flipped off with the editor open.
function rotateAutoEligibleHere() {
  const filled = (modalState.rotateEntries || []).filter((e) => (e || "").trim()).length;
  return flag("rotateAuto") && !modalState.image && !modalState.filePath && filled >= 2;
}

function renderRotateAutoBtn() {
  const eligible = rotateAutoEligibleHere();
  modal.rotateAuto.disabled = !eligible;
  const on = eligible && !!modalState.rotateAuto;
  modal.rotateAuto.classList.toggle("active", on);
  modal.rotateAuto.setAttribute("aria-pressed", String(on));
  const hasVars = (modalState.rotateEntries || []).some((entry) => extractVars(entry).length);
  modal.rotateAuto.title = !eligible ? t("rotateAutoUnavailable") : hasVars ? t("rotateAutoVarsHint") : t("rotateAutoTip");
}

function renderRotateEntries() {
  const list = modalState.rotateEntries || [];
  const max = Math.max(2, Math.round(val("rotateMaxEntries")) || 20);
  modal.rotateEntries.innerHTML = "";
  list.forEach((entry, i) => {
    const row = document.createElement("div");
    row.className = "rotate-entry-row";
    const num = document.createElement("span");
    num.className = "rotate-entry-num";
    num.textContent = String(i + 1);
    const ta = document.createElement("textarea");
    ta.className = "modal-input rotate-entry-input";
    ta.rows = 1;
    ta.value = entry;
    // Typing can flip an entry between blank/filled (eligibility) or add/remove
    // a placeholder (the button's hint tooltip) — both must stay live, not just on open.
    ta.addEventListener("input", () => { list[i] = ta.value; autoGrow(ta); renderRotateAutoBtn(); });
    const btns = document.createElement("div");
    btns.className = "rotate-entry-btns";
    const up = document.createElement("button");
    up.type = "button";
    up.className = "icon-btn rotate-entry-move";
    up.title = t("rotateMoveUp");
    up.setAttribute("aria-label", t("rotateMoveUp"));
    putIcon(up, ICON_UP);
    up.disabled = i === 0;
    up.addEventListener("click", () => { [list[i - 1], list[i]] = [list[i], list[i - 1]]; renderRotateEntries(); renderRotateAutoBtn(); });
    const down = document.createElement("button");
    down.type = "button";
    down.className = "icon-btn rotate-entry-move";
    down.title = t("rotateMoveDown");
    down.setAttribute("aria-label", t("rotateMoveDown"));
    putIcon(down, ICON_DOWN);
    down.disabled = i === list.length - 1;
    down.addEventListener("click", () => { [list[i + 1], list[i]] = [list[i], list[i + 1]]; renderRotateEntries(); renderRotateAutoBtn(); });
    const del = document.createElement("button");
    del.type = "button";
    del.className = "icon-btn rotate-entry-del";
    del.title = t("delete");
    del.setAttribute("aria-label", t("delete"));
    putIcon(del, ICON_TRASH);
    del.disabled = list.length <= 1;
    del.addEventListener("click", () => { list.splice(i, 1); renderRotateEntries(); renderRotateAutoBtn(); });
    btns.append(up, down, del);
    row.append(num, ta, btns);
    modal.rotateEntries.appendChild(row);
    requestAnimationFrame(() => autoGrow(ta));
  });
  modal.rotateAdd.disabled = list.length >= max;
}

// "Next: n/total" + "From the start" only mean anything for an ALREADY SAVED
// rotating prompt — a brand-new or not-yet-multi-entry one has no backend pointer
// to read yet.
async function refreshRotateNext() {
  const list = modalState.rotateEntries || [];
  const showPeek = modalState.mode === "edit" && !!modalState.id && list.length > 1;
  modal.rotateNext.classList.toggle("hidden", !showPeek);
  modal.rotateReset.classList.toggle("hidden", !showPeek);
  if (!showPeek) return;
  const id = modalState.id; // snapshot before the await, re-check after
  try {
    const peek = await invoke("rotation_peek", { id });
    if (peek && modalState.id === id) {
      modal.rotateNext.textContent = t("rotateNextLabel")
        .replace("{n}", String(peek.pos)).replace("{total}", String(peek.total));
    }
  } catch (_) { /* the peek is a convenience readout, never worth surfacing an error for */ }
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
// Page defaults to the first (0). The backend clamps against the real page
// count, so a stale number from a previously longer PDF at the same path still
// renders something instead of nothing.
async function loadFilePreview(path, page = 0) {
  if (PDF_EXT.test(path)) {
    return (await invoke("pdf_preview", { path, page }).catch(() => "")) || "";
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
  if (voiceState !== "idle" && voiceTargetEl === modal.text) stopVoice(); // its field is about to disappear
  closeColorPop();
  closeTagSuggest();
  closeTagPop();
  modal.root.classList.add("hidden");
  modal.video.removeAttribute("src"); // stop a playing preview
  modalState = null;
  modalInitial = "";
  // A tile the editor was opened from can be gone by the time it closes (the
  // prompt got deleted mid-edit, or a re-render replaced the DOM node) --
  // focusing a detached element is a harmless no-op, never an error.
  modalOpener?.focus?.();
  modalOpener = null;
}

// Themed yes/no dialog. Resolves true on confirm, false on cancel / dismiss.
// `icon` puts an SVG above the title, `input` turns it into a one-field prompt
// (then it resolves to the entered string, or null when dismissed).
let confirmCleanup = null;
// `checkboxes`: [{key, label, checked}] — rendered into #confirm-checkbox-row,
// read back by the caller via document.getElementById(`confirm-cb-${key}`) once
// the promise resolves true (the return value itself stays plain bool/string, so
// no existing `if (!ok) return` caller needs to change). Default requires at
// least one to stay checked (the confirm button would otherwise offer nothing
// to actually do) -- pass `checkboxesOptional: true` for a plain standalone
// toggle (e.g. "don't show this again") that must never block either button.
function confirmDialog({ title, message, confirmLabel, cancelLabel, icon, input, checkboxes, checkboxesOptional = false, danger = true, oneButton = false }) {
  // Restored once this dialog resolves, so a keyboard user's next Tab
  // continues from where they were instead of restarting from the page top.
  const opener = document.activeElement;
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
  // Part of THIS call's own render, not a side effect toggled by the caller
  // around await confirmDialog(...) — a second, unrelated dialog opening while
  // this one is still pending must never flip the wrong dialog's cancel button.
  cancel.classList.toggle("hidden", oneButton);
  ok.classList.toggle("danger-confirm", danger);
  ok.textContent = confirmLabel;
  cancel.textContent = cancelLabel || t("cancel");
  if (input) {
    // Defaults to a masked field; `type: "text"` makes it a plain one-line prompt
    // (renaming a tag), where the reveal eye has nothing to reveal.
    field.type = input.type || "password";
    field.value = input.value || "";
    field.placeholder = input.placeholder || "";
    eye.innerHTML = EYE_SVG;
    eye.classList.toggle("hidden", field.type !== "password");
    // Native browser completion from the given list (e.g. existing tag names) --
    // only wired when the caller actually passes one, so every other prompt keeps
    // its plain input.
    const datalist = $("confirm-input-datalist");
    datalist.innerHTML = "";
    for (const value of input.datalist || []) {
      const opt = document.createElement("option");
      opt.value = value;
      datalist.appendChild(opt);
    }
  }
  const cbRow = $("confirm-checkbox-row");
  cbRow.innerHTML = "";
  cbRow.classList.toggle("hidden", !checkboxes);
  const syncOk = () => { ok.disabled = !checkboxesOptional && !!checkboxes && !cbRow.querySelector("input:checked"); };
  if (checkboxes) {
    for (const cb of checkboxes) {
      const label = document.createElement("label");
      label.className = "confirm-checkbox";
      const box = document.createElement("input");
      box.type = "checkbox";
      box.id = `confirm-cb-${cb.key}`;
      box.checked = cb.checked !== false;
      box.addEventListener("change", syncOk);
      label.append(box, document.createTextNode(cb.label));
      cbRow.appendChild(label);
    }
  }
  // Resolve any stale dialog BEFORE syncOk(), not after -- its done() resets
  // ok.disabled = false unconditionally, which otherwise clobbered a fresh
  // checkbox dialog's correct disabled-with-nothing-checked state.
  confirmCleanup?.(false);
  syncOk();
  root.classList.remove("hidden");
  return new Promise((resolve) => {
    const done = (val) => {
      root.classList.add("hidden");
      ok.disabled = false;
      ok.removeEventListener("click", onOk);
      cancel.removeEventListener("click", onCancel);
      root.removeEventListener("pointerdown", onBg);
      field.removeEventListener("keydown", onKey);
      eye.removeEventListener("click", onEye);
      confirmCleanup = null;
      opener?.focus?.();
      resolve(val);
    };
    const finish = (okPressed) => done(input ? (okPressed ? field.value : null) : okPressed);
    const onOk = () => finish(true);
    const onCancel = () => finish(false);
    const onBg = (e) => { if (e.target === root) finish(false); };
    const onKey = (e) => { if (e.key === "Enter" && !ok.disabled) { e.preventDefault(); finish(true); } };
    const onEye = () => { field.type = field.type === "password" ? "text" : "password"; };
    ok.addEventListener("click", onOk);
    cancel.addEventListener("click", onCancel);
    root.addEventListener("pointerdown", onBg);
    field.addEventListener("keydown", onKey);
    eye.addEventListener("click", onEye);
    // Escape and the stale-dialog sweep both come through here, so they must map
    // their answer exactly like Cancel does — a prompt with an input field owes its
    // caller null, never false.
    confirmCleanup = finish;
    (input ? field : ok).focus();
    if (input && field.value) field.select();
  });
}

// Same dialog with nothing to decide: one button, used to report a result.
async function infoDialog(title, message, icon) {
  await confirmDialog({ title, message, confirmLabel: t("close"), icon, danger: false, oneButton: true });
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

// Same guard as the X button/backdrop/Escape, for an action (drag-drop) that can
// fire with no editor open at all -- confirmDiscardIfDirty relies on modalState
// being set, so it is only asked when the modal is actually visible.
function confirmDiscardOrProceed(run) {
  if (modal.root.classList.contains("hidden")) {
    run();
    return;
  }
  confirmDiscardIfDirty().then((ok) => { if (ok) run(); });
}

function startCreate() {
  const text = inputEl.value.trim();
  if (!text) return;
  // Carry the composed text into the dialog so it can be reviewed/edited (and
  // previewed as Markdown) before saving.
  openModal({ mode: "create", text, title: t("nameModalTitle") });
}

async function confirmModal() {
  if (!modalState || modalSaving) return;
  // Saving ends a dictation session targeting this dialog's text field --
  // one of the five stop paths, so the recognizer never keeps listening into a
  // field that just closed.
  if (voiceState !== "idle" && voiceTargetEl === modal.text) stopVoice();
  const name = modal.name.value.trim();
  if (!name) { modal.name.focus(); return; }
  // While rotating, the real text lives in rotateEntries, not modal.text.
  // rotationActive() (raw) — with the feature off but the prompt still holding
  // stored entries, they must still be preserved on save, just sourced
  // from wherever each one actually lives right now: entry 0 from the visible,
  // editable textarea (rotationUiOn() is false, so THAT is the surface the user just
  // edited), entries 1..n untouched from modalState (never shown, never editable
  // while the feature is off).
  const rotating = rotationUiOn();
  const hasVariants = rotationActive();
  const rawTexts = rotating
    ? modalState.rotateEntries
    : hasVariants
      ? [modal.text.value, ...modalState.rotateEntries.slice(1)]
      : [modal.text.value];
  // A {#{ }#} with no name inside never becomes a fillable variable — warn now,
  // while it is still one click away from being fixed, instead of after it is saved.
  if (rawTexts.some(hasEmptyVarToken)) {
    const proceed = await confirmDialog({
      title: t("emptyVarTitle"),
      message: t("emptyVarBody"),
      confirmLabel: t("emptyVarSaveAnyway"),
      danger: false,
    });
    if (!proceed) { if (!rotating) modal.text.focus(); return; }
  }
  // "empty entries are dropped on save" applies to entry 1 too — the first
  // surviving non-blank entry becomes the prompt's own text.
  const rotateFilled = (rotating || hasVariants) ? rawTexts.map((s) => s.trim()).filter(Boolean) : [];
  const variants = rotateFilled.slice(1);
  const rotateMode = variants.length ? (modalState.rotateMode === "random" ? "random" : "") : "";
  const rotateAuto = variants.length ? !!modalState.rotateAuto : false;

  const color = modalState.color || "";
  const image = modalState.image || "";
  const filePath = modalState.filePath || "";
  const iconPath = modalState.iconPath || "";
  const caption = modal.caption.value.trim();
  const captionSize = Number(modal.captionSize.value) || 0;
  const font = modal.fontSel.value;
  const fontSize = Number(modal.sizeSel.value) || 0;
  const favorite = modal.fav.classList.contains("active");
  // Sent even while the feature is off, so switching it off never drops stored
  // tags or notes.
  const tags = modalState.tags || [];
  const notes = modal.notesText.value;
  const hotkey = modalState.hotkey || "";
  const hotkeyPaste = hotkey ? (modalState.hotkeyPaste || "") : "";
  const pdfPage = modalState.pdfPage || 0;
  // Double-click on an empty cell opened this dialog for that exact spot — the
  // backend's own first-free-cell placement would otherwise ignore it.
  const targetCell = modalState.targetCell;
  // NOTE: Tauri expects camelCase keys for snake_case Rust args.
  // Path media (gif/video icon or attachment) shows without a stored image.
  const showImage =
    image || mediaKind(filePath) || mediaKind(iconPath) ? modalState.showImage : false;
  const copyImage = image ? modalState.copyImage : false;
  let saved = null;
  modalSaving = true;
  modal.confirm.disabled = true;
  try {
    if (modalState.mode === "create") {
      const text = (rotating || hasVariants) ? (rotateFilled[0] || "") : modal.text.value.trim();
      if (!text) { closeModal(); return; }
      saved = await invoke("add_prompt", { name, text, color, image, showImage, copyImage, filePath, pdfPage, iconPath, caption, captionSize, font, fontSize, favorite, tags, notes, hotkey, hotkeyPaste, variants, rotateMode, rotateAuto });
      inputEl.value = "";
      autoGrow(inputEl);
      saveBtn.disabled = true;
    } else if (modalState.mode === "image-create" || modalState.mode === "file-create") {
      // The name doubles as the copy text when shown as text.
      saved = await invoke("add_prompt", { name, text: name, color, image, showImage, copyImage, filePath, pdfPage, iconPath, caption, captionSize, font, fontSize, favorite, tags, notes, hotkey, hotkeyPaste, variants: [], rotateMode: "", rotateAuto: false });
    } else {
      const text = copyImage || filePath ? name : ((rotating || hasVariants) ? (rotateFilled[0] || "") : modal.text.value);
      const args = { id: modalState.id, name, text, color, showImage, copyImage, filePath, pdfPage, iconPath, caption, captionSize, font, fontSize, favorite, tags, notes, hotkey, hotkeyPaste, variants, rotateMode, rotateAuto };
      // The image field only travels when it actually changed — an untouched
      // picture is left to the backend, and "" only ever means the remove button.
      if (image !== (modalState.imageOpened || "")) args.image = image;
      saved = await invoke("update_prompt", args);
      // Save can turn rotateAuto off (or drop it below two entries) while a
      // session for THIS prompt happens to be armed — end it now instead of
      // waiting for the next paste to notice, same as the tile-menu toggle.
      // A no-op on the backend if no session is running for this id.
      if (!rotateAuto) await invoke("rotation_end_auto", { id: modalState.id }).catch(() => {});
    }
  } catch (err) {
    const errStr = String(err);
    // save-failed has no hotkey involved - route it through the same friendly
    // message the persist-failed event uses, instead of hotkeyMessage's fallback
    // of showing the raw error code verbatim.
    toast(errStr === "save-failed" ? t("persistFailed") : hotkeyMessage(errStr)); // keep the modal open so nothing typed is lost
    // A binding the backend refused was never stored, so the field must not keep
    // showing it: drop it and name the reason where it was captured.
    if (HOTKEY_ERRORS[errStr] && hotkey) {
      setPromptHotkey("");
      showHotkeyError(hotkeyMessage(errStr));
    }
    return;
  } finally {
    modalSaving = false;
    modal.confirm.disabled = false;
  }
  // The backend hands back what it really stored. An empty hotkey where one
  // was sent means the OS refused the combination between the check and the save.
  if (hotkey && saved && !saved.hotkey) toast(t("promptHotkeyFail"));
  // The same near-duplicate check the import path already runs, now also
  // after creating a prompt by hand — before closeModal() clears modalState.
  const wasCreate = modalState.mode === "create" || modalState.mode === "image-create" || modalState.mode === "file-create";
  const wasEdit = modalState.mode === "edit";
  // An edit can change entry count/text — drop the cached rotation state so the
  // grid re-render below primes it fresh instead of keeping a stale badge/tooltip
  // (primeRotateState only ever peeks once per id otherwise).
  if (saved) rotateState.delete(saved.id);
  closeModal();
  await renderGrid();
  if (saved && targetCell) await placeTile(saved.id, targetCell[0], targetCell[1]);
  pollMissingFiles(); // a replaced file clears the error immediately
  if (!libraryEl.classList.contains("hidden")) {
    // An edit MAY have just changed this prompt's "last edited" rank — the
    // library stayed open underneath the modal, so the usual refresh-on-open
    // (library-btn click) never fired for it.
    if (wasEdit && flag("promptHistory")) await refreshLastEdited();
    renderLibrary();
  }
  if (saved && wasCreate) maybeDupImportToast();
}

// Everything about a prompt except what identifies it and the hotkey (one accelerator,
// one prompt): opens the normal create dialog prefilled, so the user can review or
// cancel before anything is saved. Placement then follows the regular create path.
// A stored prompt as the editor's fields. Both "edit" and "duplicate" go through
// here so a newly added prompt field can never reach one and be forgotten in the
// other — each caller then overrides only what genuinely differs between them.
function promptToModal(p) {
  const kind = p.file_path ? mediaKind(p.file_path) : "";
  return {
    name: p.name,
    text: p.text,
    color: p.color || "",
    image: p.image || "",
    showImage: p.show_image || false,
    copyImage: p.copy_image || false,
    filePath: p.file_path || "",
    pdfPage: p.pdf_page || 0,
    iconPath: p.icon_path || "",
    caption: p.caption || "",
    captionSize: p.caption_size || 0,
    font: p.font || "",
    fontSize: p.font_size || 0,
    favorite: p.favorite || false,
    tags: p.tags || [],
    notes: p.notes || "",
    hotkey: p.hotkey || "",
    hotkeyPaste: p.hotkey_paste || "",
    variants: p.variants || [],
    rotateMode: p.rotate_mode || "",
    rotateAuto: p.rotate_auto || false,
    title: p.file_path
      ? t(kind === "video" ? "videoEditTitle" : kind ? "imageEditTitle" : "fileEditTitle")
      : p.copy_image ? t("imageEditTitle") : t("editModalTitle"),
  };
}

async function duplicatePrompt(id) {
  let p;
  try {
    p = await invoke("get_prompt", { id });
  } catch (err) {
    errToast(err);
    return;
  }
  if (!p) return;
  // A copy starts unstarred and unbound: one accelerator belongs to one prompt.
  openModal({
    ...promptToModal(p),
    mode: "create",
    name: `${p.name} (${t("duplicateSuffix")})`,
    favorite: false,
    hotkey: "",
    hotkeyPaste: "",
  });
}

async function editPrompt(id) {
  let p;
  try {
    p = await invoke("get_prompt", { id });
  } catch (err) {
    errToast(err);
    return;
  }
  if (p) {
    openModal({ ...promptToModal(p), mode: "edit", id });
  }
}

// ---- Tags: chips in the editor, filter row + manager in the library ----
// Names are compared case-insensitively; the backend keeps one canonical casing
// and stores chip colors under the lowercase key, so a recased tag keeps its color.
const tagsOn = () => flag("tags");
const tagKey = (name) => String(name ?? "").trim().toLowerCase();
const tagColor = (name) => settings.tag_colors?.[tagKey(name)] || "";
const TAG_MAX_LEN = 60; // keep in sync with MAX_TAG_LEN in lib.rs

// Every tag in the library, first-seen casing, sorted for a stable filter row.
// tagSort switches alphabetical (default) for usage-count order, ties broken
// alphabetically either way.
function allTags() {
  const seen = new Map();
  for (const p of prompts) {
    for (const name of p.tags || []) {
      const key = tagKey(name);
      if (key && !seen.has(key)) seen.set(key, name);
    }
  }
  const names = [...seen.values()];
  if (settings.ui_texts?.tagSort === "usage") {
    const counts = tagUsage();
    names.sort((a, b) => (counts.get(tagKey(b)) || 0) - (counts.get(tagKey(a)) || 0) || a.localeCompare(b, LANG));
  } else {
    names.sort((a, b) => a.localeCompare(b, LANG));
  }
  return names;
}

function tagUsage() {
  const counts = new Map();
  for (const p of prompts) {
    for (const key of new Set((p.tags || []).map(tagKey))) {
      counts.set(key, (counts.get(key) || 0) + 1);
    }
  }
  return counts;
}

// One chip. `as` picks the element so filter chips stay real buttons.
function tagChipEl(name, as = "span") {
  const chip = document.createElement(as);
  chip.className = "tag-chip";
  if (as === "button") chip.type = "button";
  const color = tagColor(name);
  if (color) {
    chip.style.setProperty("--chip-color", color);
    chip.classList.add("tag-colored");
  }
  const label = document.createElement("span");
  label.className = "tag-chip-label";
  label.textContent = name;
  chip.append(label);
  return chip;
}

// Shared floating layers: the type-ahead list and the per-chip palette.
const tagSuggest = document.createElement("div");
tagSuggest.className = "tag-suggest hidden";
document.body.appendChild(tagSuggest);
const tagPop = document.createElement("div");
tagPop.className = "tag-pop hidden";
tagPop.innerHTML = '<div class="swatches"></div>';
document.body.appendChild(tagPop);

const placePop = (el, anchor) => placePopover(el, anchor.getBoundingClientRect());
const closeTagSuggest = () => tagSuggest.classList.add("hidden");
const closeTagPop = () => tagPop.classList.add("hidden");
document.addEventListener("pointerdown", (e) => {
  if (!tagSuggest.classList.contains("hidden") &&
      !tagSuggest.contains(e.target) && e.target !== $("modal-tag-input") &&
      !e.target.closest?.(".tag-pick")) closeTagSuggest();
  if (!tagPop.classList.contains("hidden") &&
      !tagPop.contains(e.target) && !e.target.closest(".tag-dot") && !e.target.closest(".color-pop")) closeTagPop();
});

// Palette for one tag. Colors are global (settings), so this writes through at once.
function openTagColorPop(anchor, name, after) {
  const box = tagPop.querySelector(".swatches");
  const apply = async (hex) => {
    try {
      settings = await invoke("set_tag_color", { name, color: hex });
    } catch (err) { errToast(err); return; }
    after?.();
  };
  buildSwatches(box, tagColor(name), (hex) => { closeTagPop(); apply(hex); },
    (swatch, cur) => openColorPop(swatch, cur, apply));
  placePop(tagPop, anchor);
}

// ---- Editor: assigned chips + type-ahead input ----
function renderTagChips() {
  const box = $("modal-tag-chips");
  if (!box || !modalState) return;
  box.innerHTML = "";
  for (const name of modalState.tags || []) {
    const chip = tagChipEl(name);
    const dot = document.createElement("button");
    dot.type = "button";
    dot.className = "tag-dot";
    dot.title = t("tagColor");
    dot.addEventListener("click", (e) => {
      e.stopPropagation();
      openTagColorPop(dot, name, renderTagChips);
    });
    const del = document.createElement("button");
    del.type = "button";
    del.className = "tag-x";
    del.title = t("tagRemove");
    del.setAttribute("aria-label", t("tagRemove"));
    del.textContent = "×";
    del.addEventListener("click", () => {
      modalState.tags = modalState.tags.filter((n) => tagKey(n) !== tagKey(name));
      renderTagChips();
    });
    chip.prepend(dot);
    chip.append(del);
    box.appendChild(chip);
  }
  renderTagQuickAdd();
}

// The 3 most-used tags across the library, minus ones already on this
// prompt — click to add without typing.
function renderTagQuickAdd() {
  const box = $("modal-tag-quickadd");
  if (!box || !modalState) return;
  box.innerHTML = "";
  if (!tagsOn()) { box.classList.add("hidden"); return; }
  const counts = tagUsage();
  const assigned = new Set((modalState.tags || []).map(tagKey));
  const top = allTags()
    .filter((n) => !assigned.has(tagKey(n)))
    .sort((a, b) => (counts.get(tagKey(b)) || 0) - (counts.get(tagKey(a)) || 0))
    .slice(0, 3);
  box.classList.toggle("hidden", !top.length);
  if (!top.length) return;
  const label = document.createElement("span");
  label.className = "tag-quickadd-label";
  label.textContent = t("tagQuickAddLabel");
  box.appendChild(label);
  for (const name of top) {
    const chip = tagChipEl(name, "button");
    chip.addEventListener("click", () => { addTagToModal(name); renderTagChips(); });
    box.appendChild(chip);
  }
}

// Add a typed name to the open editor. Returns false when it is unusable, so the
// input can keep the text instead of silently swallowing it.
function addTagToModal(raw) {
  if (!modalState) return false;
  const name = String(raw || "").trim();
  if (!name || name.length > TAG_MAX_LEN || name.includes(",")) return false;
  const key = tagKey(name);
  if ((modalState.tags || []).some((n) => tagKey(n) === key)) return true; // already on
  // Join the casing the library already uses instead of forking a near-duplicate.
  const known = allTags().find((n) => tagKey(n) === key);
  modalState.tags = [...(modalState.tags || []), known || name];
  renderTagChips();
  return true;
}

// `all` ignores what is typed — that is what the chevron next to the field opens,
// so an existing tag can be picked without knowing its first letters.
function renderTagSuggest(all = false) {
  const input = $("modal-tag-input");
  if (!input || !modalState) return;
  const q = all ? "" : input.value.trim().toLowerCase();
  const taken = new Set((modalState.tags || []).map(tagKey));
  const hits = allTags()
    .filter((n) => !taken.has(tagKey(n)) && (!q || tagKey(n).includes(q)))
    .slice(0, all ? 40 : 8);
  // Typing with no match just closes — a dropdown that keeps saying "nothing" while
  // the user is still typing a NEW tag is in the way.
  if (!hits.length && !all) { closeTagSuggest(); return; }
  tagSuggest.innerHTML = "";
  if (!hits.length) {
    // Opened by the handle with nothing to offer. This belongs IN the dropdown —
    // a toast for "there is nothing here yet" reads like an error and is gone
    // before it is understood.
    const empty = document.createElement("div");
    empty.className = "tag-suggest-empty";
    empty.textContent = t(allTags().length ? "tagAllTaken" : "tagNone");
    tagSuggest.appendChild(empty);
  }
  for (const name of hits) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "tag-suggest-item";
    item.append(tagChipEl(name));
    item.addEventListener("click", () => {
      addTagToModal(name);
      input.value = "";
      closeTagSuggest();
      input.focus();
    });
    tagSuggest.appendChild(item);
  }
  // A dropdown belongs to its field, so it takes the field's width.
  tagSuggest.style.minWidth = `${Math.round(input.getBoundingClientRect().width)}px`;
  placePop(tagSuggest, input);
}

function wireTagInput() {
  const input = $("modal-tag-input");
  if (!input) return;
  const commit = () => {
    // A pasted "a, b, c" splits into chips rather than being rejected as one name.
    const parts = input.value.split(",");
    const rest = parts.filter((p) => !addTagToModal(p));
    input.value = rest.join(",").trim();
    if (input.value) toast(t("tagInvalid"));
    closeTagSuggest();
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); commit(); return; }
    if (e.key === "Escape" && !tagSuggest.classList.contains("hidden")) {
      e.stopPropagation(); // keep Escape from closing the whole editor
      closeTagSuggest();
      return;
    }
    if (e.key === "Backspace" && !input.value && modalState?.tags?.length) {
      modalState.tags = modalState.tags.slice(0, -1);
      renderTagChips();
    }
  });
  input.addEventListener("input", () => {
    if (input.value.includes(",")) { commit(); return; }
    renderTagSuggest();
  });
  input.addEventListener("focus", () => renderTagSuggest());
  input.addEventListener("blur", () => setTimeout(closeTagSuggest, 150)); // let a click land first
  const pick = $("modal-tag-pick");
  pick?.addEventListener("mousedown", (e) => e.preventDefault()); // keep the field focused
  pick?.addEventListener("click", () => {
    if (!tagSuggest.classList.contains("hidden")) { closeTagSuggest(); return; }
    input.value = "";
    renderTagSuggest(true);
    input.focus();
  });
}
wireTagInput();

// ---- Library: filter chips + manager ----
// The bar (and every chip's click listener) doesn't change while the user is
// just typing a search query — skip rebuilding it unless the tag set or the active
// filter actually changed since the last render.
let _lastTagFilterSig = null;
function renderTagFilter() {
  const bar = $("library-tagbar");
  if (!bar) return;
  if (!tagsOn()) {
    bar.innerHTML = "";
    _lastTagFilterSig = null;
    libTags.clear();
    closeTagManager();
    return;
  }
  const names = allTags();
  // A tag renamed or deleted elsewhere must not keep filtering the list.
  const live = new Set(names.map(tagKey));
  for (const key of [...libTags]) if (!live.has(key)) libTags.delete(key);
  const sig = names.join("") + "" + [...libTags].sort().join("");
  if (sig === _lastTagFilterSig) return;
  _lastTagFilterSig = sig;
  renderTagBar(bar, libTags, renderLibrary);
}

// Shared markup for both filter bars (library's #library-tagbar and the copy
// history's #journal-tags) -- label + chips + "manage tags" button.
// Only the container, the active set and what to re-render differ per caller.
// "Manage tags" stays reachable even before the first tag exists -- the bar
// used to disappear entirely with nothing to filter by, hiding the only way
// to find out tags exist at all.
function renderTagBar(bar, activeSet, rerender) {
  bar.innerHTML = "";
  const names = allTags();
  if (names.length) {
    // A bare row of chips reads as decoration; the label is what makes it a filter.
    const label = document.createElement("span");
    label.className = "lib-tagbar-label";
    label.textContent = t("tagFilterLabel");
    bar.appendChild(label);
    for (const name of names) {
      const chip = tagChipEl(name, "button");
      chip.classList.add("tag-filter");
      const on = activeSet.has(tagKey(name));
      chip.classList.toggle("active", on);
      chip.setAttribute("aria-pressed", String(on));
      chip.addEventListener("click", () => {
        const key = tagKey(name);
        if (activeSet.has(key)) activeSet.delete(key); else activeSet.add(key);
        rerender();
      });
      bar.appendChild(chip);
    }
    if (activeSet.size) {
      const clear = document.createElement("button");
      clear.type = "button";
      clear.className = "tag-action";
      clear.textContent = t("tagClearFilter");
      clear.addEventListener("click", () => { activeSet.clear(); rerender(); });
      bar.appendChild(clear);
    }
  }
  const manage = document.createElement("button");
  manage.type = "button";
  manage.className = "tag-action";
  manage.textContent = t("tagManager");
  manage.addEventListener("click", openTagManager);
  bar.appendChild(manage);
}

// Merging is just rename_tag with a target that already exists -- its own
// rename_tag_in dedupes per-prompt and its color rule (keep the target's color,
// only borrow the source's if the target has none) already matches what a merge
// needs. Shared by the explicit Merge button and a rename that collides with an
// existing tag, so both funnel through the same confirmation and backend call.
async function confirmAndMergeTag(from, into, refresh) {
  const n = tagUsage().get(tagKey(from)) || 0;
  const ok = await confirmDialog({
    title: t("tagMergeConfirmTitle"),
    message: t("tagMergeConfirmMsg").replace("{from}", from).replace("{into}", into).replace("{n}", String(n)),
    confirmLabel: t("tagMerge"),
  });
  if (!ok) return;
  try { settings = await invoke("rename_tag", { old: from, new: into }); }
  catch (err) { errToast(err); return; }
  await refresh();
}

function openTagManager() {
  $("tag-modal").classList.remove("hidden");
  renderTagManager();
}
function closeTagManager() {
  $("tag-modal").classList.add("hidden");
  closeTagPop();
}

function renderTagManager() {
  const list = $("tag-modal-list");
  if (!list) return;
  list.innerHTML = "";
  const counts = tagUsage();
  const names = allTags();
  if (!names.length) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("tagNone");
    list.appendChild(empty);
    return;
  }
  // One backend pass per action, then a full state refetch — a tag on hundreds of
  // prompts is rewritten in the store, never prompt by prompt from here.
  const refresh = async () => {
    await renderGrid();
    renderTagManager();
    renderLibrary();
  };
  for (const name of names) {
    const row = document.createElement("div");
    row.className = "tag-manage-row";
    const dot = document.createElement("button");
    dot.type = "button";
    dot.className = "tag-dot";
    dot.title = t("tagColor");
    const color = tagColor(name);
    if (color) dot.style.setProperty("--chip-color", color);
    dot.classList.toggle("tag-colored", !!color);
    dot.addEventListener("click", (e) => {
      e.stopPropagation();
      openTagColorPop(dot, name, refresh);
    });
    const label = document.createElement("span");
    label.className = "tag-manage-name";
    label.textContent = name;
    const count = document.createElement("span");
    count.className = "tag-manage-count";
    count.textContent = String(counts.get(tagKey(name)) || 0);
    count.title = t("tagUsageCount");
    const rename = document.createElement("button");
    rename.type = "button";
    rename.className = "ghost-btn";
    rename.textContent = t("tagRename");
    rename.addEventListener("click", async () => {
      const next = await confirmDialog({
        title: t("tagRenameTitle"),
        message: name,
        confirmLabel: t("save"),
        danger: false,
        input: { type: "text", value: name, placeholder: t("tagsPh") },
      });
      if (next == null || !next.trim()) return;
      const nextTrimmed = next.trim();
      if (tagKey(nextTrimmed) === tagKey(name)) return; // unchanged
      // Renaming onto an existing tag is really a merge -- name it that way and
      // show what it actually affects before it happens, same as the explicit
      // Merge button below.
      const existing = allTags().find((n2) => tagKey(n2) === tagKey(nextTrimmed));
      if (existing) { await confirmAndMergeTag(name, existing, refresh); return; }
      try { settings = await invoke("rename_tag", { old: name, new: nextTrimmed }); }
      catch (err) { errToast(err); return; }
      await refresh();
    });
    const merge = document.createElement("button");
    merge.type = "button";
    merge.className = "ghost-btn";
    merge.textContent = t("tagMerge");
    merge.addEventListener("click", async () => {
      const others = allTags().filter((n2) => tagKey(n2) !== tagKey(name));
      const target = await confirmDialog({
        title: t("tagMergeTargetTitle"),
        message: t("tagMergeTargetMsg").replace("{name}", name),
        confirmLabel: t("tagMerge"),
        danger: false,
        input: { type: "text", placeholder: t("tagsPh"), datalist: others },
      });
      const trimmed = (target || "").trim();
      if (!trimmed || tagKey(trimmed) === tagKey(name)) return;
      const existing = others.find((n2) => tagKey(n2) === tagKey(trimmed)) || trimmed;
      await confirmAndMergeTag(name, existing, refresh);
    });
    const del = document.createElement("button");
    del.type = "button";
    del.className = "ghost-btn danger-btn";
    del.textContent = t("delete");
    del.addEventListener("click", async () => {
      const ok = await confirmDialog({
        title: t("tagDeleteTitle"),
        message: t("tagDeleteMsg").replace("{name}", name),
        confirmLabel: t("delete"),
      });
      if (!ok) return;
      try { settings = await invoke("delete_tag", { name }); }
      catch (err) { errToast(err); return; }
      libTags.delete(tagKey(name));
      await refresh();
    });
    row.append(dot, label, count, rename, merge, del);
    list.appendChild(row);
  }
}

$("tag-modal-close")?.addEventListener("click", closeTagManager);
// One click assigns the palette to every tag that has no color yet, in the
// order the palette lists them — round-robin once past the end.
$("tag-colorize")?.addEventListener("click", async () => {
  const palette = COLORS.slice(1);
  if (!palette.length) return;
  const uncolored = allTags().filter((n) => !tagColor(n));
  if (!uncolored.length) return;
  const entries = uncolored.map((name, i) => ({ name, color: palette[i % palette.length] }));
  try { settings = await invoke("set_tag_colors", { entries }); }
  catch (err) { errToast(err); return; }
  renderTagManager();
  renderLibrary();
  await renderGrid();
});
$("tag-modal")?.addEventListener("pointerdown", (e) => {
  if (e.target === $("tag-modal")) closeTagManager();
});

// ---- Custom theme editor ----
const THEME_LABELS = {
  light: "themeLight", dark: "themeDark", programmer: "themeProgrammer", neon: "themeNeon",
  cyberpunk: "themeCyberpunk", mono: "themeMono", ocean: "themeOcean",
};
const THEME_VAR_LABELS = {
  bg: "themeVarBg", surface: "themeVarSurface", "surface-2": "themeVarSurface2",
  fg: "themeVarFg", muted: "themeVarMuted", border: "themeVarBorder",
  accent: "themeVarAccent", "accent-fg": "themeVarAccentFg", danger: "themeVarDanger",
};

// The theme tokens are authored as hex or rgba(), and a color picker needs hex.
// Alpha is dropped — only a value the user actually overrides is ever stored.
function cssColorToHex(value) {
  const v = String(value || "").trim();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(v);
  if (hex) {
    const d = hex[1];
    return `#${(d.length === 3 ? [...d].map((c) => c + c).join("") : d)}`.toLowerCase();
  }
  const rgb = /^rgba?\(([^)]+)\)$/i.exec(v);
  if (rgb) {
    const parts = rgb[1].split(/[\s,/]+/).filter(Boolean).slice(0, 3);
    if (parts.length === 3) {
      const to = (p) => {
        const n = p.endsWith("%") ? (parseFloat(p) * 255) / 100 : parseFloat(p);
        return Math.max(0, Math.min(255, Math.round(n || 0))).toString(16).padStart(2, "0");
      };
      return `#${parts.map(to).join("")}`;
    }
  }
  return "#808080"; // unreadable value: a neutral starting point beats a broken input
}

// The colours a base theme resolves to. Only :root carries the theme rules, so the
// probe runs on the document element itself and restores its state right away —
// no frame is painted in between, so nothing flickers.
function baseThemeColors(base) {
  const root = document.documentElement;
  const prevTheme = root.getAttribute("data-theme");
  const prevVars = THEME_VARS.map((key) => [key, root.style.getPropertyValue(`--${key}`)]);
  root.setAttribute("data-theme", base);
  for (const key of THEME_VARS) root.style.removeProperty(`--${key}`);
  const style = getComputedStyle(root);
  const out = {};
  for (const key of THEME_VARS) out[key] = cssColorToHex(style.getPropertyValue(`--${key}`));
  if (prevTheme != null) root.setAttribute("data-theme", prevTheme);
  for (const [key, value] of prevVars) if (value) root.style.setProperty(`--${key}`, value);
  return out;
}

// The theme being edited, and the state to return to when the editor is cancelled
// (it previews on the real window, so cancelling has to undo it).
let themeDraft = null;
let themeRestore = null;
// The draft as it looked the moment the editor opened, so "Revert changes"
// can undo everything done in THIS session without closing the editor — a
// separate snapshot from themeRestore, which is the real window's pre-edit
// theme (used only when the whole editor is cancelled).
let themeDraftStart = null;

function newThemeDraft() {
  const current = document.documentElement.getAttribute("data-theme");
  return {
    id: "",
    name: t("themeNewName"),
    base: BUILTIN_THEMES.includes(current) ? current : "light",
    vars: {},
  };
}

function draftFrom(theme) {
  return { id: theme.id, name: theme.name, base: theme.base, vars: { ...(theme.vars || {}) } };
}

// Live preview. Writing the variables costs a style recalc, which the browser
// already coalesces into the next frame — no grid rebuild, no IPC, so dragging a
// colour stays smooth without an extra throttle in front of it.
function previewThemeDraft() {
  if (!themeDraft) return;
  document.documentElement.setAttribute("data-theme", themeDraft.base);
  applyThemeVars(themeDraft.vars);
  // The pills and folders are their own windows and only ever hear the backend's
  // theme-changed event, which fires on a real switch — without this the preview
  // would stop at the main window's edge while they kept the old colours.
  broadcastTheme(themeDraft.base, themeDraft.vars);
}

// Same payload shape the backend emits, so the small windows need no new listener.
function broadcastTheme(theme, vars) {
  emit("theme-changed", { theme, vars: vars || {} }).catch(() => {});
}

function openThemeEditor() {
  if (!flag("customThemes")) return;
  themeRestore = {
    theme: document.documentElement.getAttribute("data-theme"),
    vars: { ...activeThemeVars() },
  };
  const active = findCustomTheme(settings.theme);
  themeDraft = active ? draftFrom(active) : newThemeDraft();
  themeDraftStart = draftFrom(themeDraft);
  $("theme-modal").classList.remove("hidden");
  renderThemeEditor();
  previewThemeDraft();
}

function closeThemeEditor(revert = true) {
  $("theme-modal").classList.add("hidden");
  closeColorPop();
  if (revert && themeRestore) {
    applyTheme(themeRestore.theme, themeRestore.vars);
    broadcastTheme(themeRestore.theme, themeRestore.vars); // undo the preview in the pills too
  }
  themeDraft = null;
  themeRestore = null;
  themeDraftStart = null;
}

// D75 part 2: undo every change made since the editor opened, without closing it.
function revertThemeDraft() {
  if (!themeDraftStart) return;
  themeDraft = draftFrom(themeDraftStart);
  renderThemeEditor();
  previewThemeDraft();
}

function renderThemeEditor() {
  if (!themeDraft) return;
  const target = $("theme-edit-target");
  target.innerHTML = "";
  const fresh = document.createElement("option");
  fresh.value = "";
  fresh.textContent = t("themeNew");
  target.appendChild(fresh);
  for (const theme of customThemes()) {
    const opt = document.createElement("option");
    opt.value = theme.id;
    opt.textContent = theme.name;
    target.appendChild(opt);
  }
  target.value = themeDraft.id;
  $("theme-edit-name").value = themeDraft.name;
  const baseSel = $("theme-edit-base");
  baseSel.innerHTML = "";
  for (const key of BUILTIN_THEMES) {
    const opt = document.createElement("option");
    opt.value = key;
    opt.textContent = t(THEME_LABELS[key]);
    baseSel.appendChild(opt);
  }
  baseSel.value = themeDraft.base;
  $("theme-edit-delete").classList.toggle("hidden", !themeDraft.id);
  $("theme-edit-export")?.classList.toggle("hidden", !themeDraft.id);
  renderThemeVars();
}

function renderThemeVars() {
  const box = $("theme-edit-vars");
  box.innerHTML = "";
  const defaults = baseThemeColors(themeDraft.base);
  for (const [groupKey, keys] of THEME_VAR_GROUPS) {
    const head = document.createElement("div");
    head.className = "theme-var-group";
    head.textContent = t(groupKey);
    box.appendChild(head);
    for (const key of keys) box.appendChild(themeVarRow(key, defaults[key]));
  }
}

function themeVarRow(key, fallback) {
  const row = document.createElement("div");
  row.className = "theme-var-row";
  const name = document.createElement("span");
  name.className = "theme-var-name";
  name.textContent = t(THEME_VAR_LABELS[key]);
  const swatch = document.createElement("button");
  swatch.type = "button";
  swatch.className = "theme-swatch";
  swatch.title = t("colorCustom");
  swatch.setAttribute("aria-label", t("colorCustom"));
  const hex = document.createElement("input");
  hex.type = "text";
  hex.className = "modal-input theme-hex";
  hex.maxLength = 7;
  hex.spellcheck = false;
  const reset = document.createElement("button");
  reset.type = "button";
  reset.className = "icon-btn theme-var-reset";
  reset.innerHTML = CLEAR_SVG;
  reset.title = t("themeVarReset");
  reset.setAttribute("aria-label", t("themeVarReset"));

  const paint = () => {
    const value = themeDraft.vars[key] || fallback;
    swatch.style.background = value;
    if (document.activeElement !== hex) hex.value = value;
    const overridden = !!themeDraft.vars[key];
    row.classList.toggle("overridden", overridden);
    reset.disabled = !overridden;
  };
  const setVar = (value) => {
    themeDraft.vars[key] = value;
    paint();
    previewThemeDraft();
  };
  const clearVar = () => {
    delete themeDraft.vars[key];
    hex.value = fallback;
    paint();
    previewThemeDraft();
  };

  swatch.addEventListener("click", () => openColorPop(swatch, themeDraft.vars[key] || fallback, setVar));
  // Typing applies as soon as the value is a complete colour; an unfinished or
  // invalid entry is rejected on blur and the last valid one comes back.
  hex.addEventListener("input", () => {
    hex.classList.remove("invalid");
    if (HEX_RE.test(hex.value.trim())) setVar(hex.value.trim().toLowerCase());
  });
  hex.addEventListener("change", () => {
    const raw = hex.value.trim();
    if (!raw) { clearVar(); return; }
    const value = raw[0] === "#" ? raw : `#${raw}`;
    if (HEX_RE.test(value)) { setVar(value.toLowerCase()); return; }
    hex.classList.add("invalid");
    setTimeout(() => hex.classList.remove("invalid"), 900);
    paint();
  });
  reset.addEventListener("click", clearVar);
  paint();
  row.append(name, swatch, hex, reset);
  return row;
}

async function saveThemeDraft() {
  if (!themeDraft) return;
  const name = $("theme-edit-name").value.trim();
  if (!name) {
    toast(t("themeNameRequired"));
    $("theme-edit-name").focus();
    return;
  }
  themeDraft.name = name;
  try {
    settings = await invoke("save_custom_theme", {
      id: themeDraft.id,
      name,
      base: themeDraft.base,
      vars: themeDraft.vars,
    });
  } catch (err) {
    errToast(err);
    return;
  }
  // A new theme is created active by the backend; editing an existing one switches
  // to it, so the live preview simply carries on instead of snapping away.
  const id = themeDraft.id || customThemeId(settings.theme);
  const pref = `custom:${id}`;
  if (settings.theme !== pref) {
    settings.theme = pref;
    await invoke("set_theme", { theme: pref }).catch((err) => errToast(err));
  }
  fillThemeSelect();
  applyTheme(themeBase(pref));
  closeThemeEditor(false);
}

async function deleteThemeDraft() {
  if (!themeDraft?.id) return;
  const ok = await confirmDialog({
    title: t("themeDeleteTitle"),
    message: t("themeDeleteMsg").replace("{name}", themeDraft.name),
    confirmLabel: t("delete"),
  });
  if (!ok) return;
  try {
    settings = await invoke("delete_custom_theme", { id: themeDraft.id });
  } catch (err) {
    errToast(err);
    return;
  }
  fillThemeSelect();
  // Deleting the active theme drops back to "system", which only the backend can
  // resolve to light or dark.
  applyTheme(await invoke("current_theme").catch(() => themeBase(settings.theme)));
  closeThemeEditor(false);
}

// Settings dropdown: the built-in themes, then the custom ones under a separator.
function fillThemeSelect() {
  const sel = $("theme-select");
  if (!sel) return;
  for (const opt of [...sel.querySelectorAll("option.custom-theme")]) opt.remove();
  const list = customThemes();
  if (list.length) {
    const sep = document.createElement("option");
    sep.className = "custom-theme";
    sep.disabled = true;
    sep.textContent = t("themeCustomGroup");
    sel.appendChild(sep);
  }
  for (const theme of list) {
    const opt = document.createElement("option");
    opt.className = "custom-theme";
    opt.value = `custom:${theme.id}`;
    opt.textContent = theme.name;
    sel.appendChild(opt);
  }
  sel.value = settings.theme || "system";
  // A parked custom theme (feature off) has no entry — show the base it renders as.
  if (!sel.value) sel.value = themeBase(settings.theme);
  // A theme that was dropped from the product leaves the select with no match at
  // all; fall back to the automatic one rather than showing an empty picker.
  if (!sel.value) sel.value = "system";
}

// Re-apply after the feature was toggled: switching it off parks every custom theme
// on its base theme without touching the stored data.
function refreshCustomThemes() {
  fillThemeSelect();
  const base = themeBase(settings.theme);
  if (customThemeId(settings.theme) && BUILTIN_THEMES.includes(base)) {
    document.documentElement.setAttribute("data-theme", base);
  }
  applyThemeVars(activeThemeVars());
}

$("theme-edit-open")?.addEventListener("click", openThemeEditor);
$("theme-modal-close")?.addEventListener("click", () => closeThemeEditor());
$("theme-edit-cancel")?.addEventListener("click", () => closeThemeEditor());
$("theme-edit-revert")?.addEventListener("click", revertThemeDraft);
// Export the currently selected SAVED theme (an unsaved draft has no id to
// look up on the backend, hence the button hides for one — see renderThemeEditor).
$("theme-edit-export")?.addEventListener("click", async () => {
  if (!themeDraft?.id) return;
  try {
    await invoke("export_custom_theme", { id: themeDraft.id });
  } catch (err) {
    if (err !== "canceled") errToast(err);
  }
});
// Import creates a new saved theme on the backend right away (the same validation
// as typing values in by hand), then opens it in the editor so it can be tweaked
// or just confirmed with Cancel/Save.
$("theme-edit-import")?.addEventListener("click", async () => {
  try {
    settings = await invoke("import_custom_theme");
  } catch (err) {
    if (err !== "canceled") errToast(err);
    return;
  }
  fillThemeSelect();
  const imported = findCustomTheme(settings.theme);
  themeDraft = imported ? draftFrom(imported) : newThemeDraft();
  themeDraftStart = draftFrom(themeDraft);
  renderThemeEditor();
  previewThemeDraft();
});
$("theme-modal")?.addEventListener("pointerdown", (e) => {
  if (e.target === $("theme-modal")) closeThemeEditor();
});
$("theme-edit-save")?.addEventListener("click", saveThemeDraft);
$("theme-edit-delete")?.addEventListener("click", deleteThemeDraft);
$("theme-edit-name")?.addEventListener("input", (e) => {
  if (themeDraft) themeDraft.name = e.target.value;
});
$("theme-edit-target")?.addEventListener("change", (e) => {
  const theme = customThemes().find((c) => c.id === e.target.value);
  themeDraft = theme ? draftFrom(theme) : newThemeDraft();
  renderThemeEditor();
  previewThemeDraft();
});
$("theme-edit-base")?.addEventListener("change", (e) => {
  if (!themeDraft) return;
  themeDraft.base = e.target.value;
  renderThemeVars(); // the un-overridden colours now come from another theme
  previewThemeDraft();
});

// ---- Prompt library (all prompts, click to edit) ----
// Classify a prompt for the library type filter / thumbnails.
function promptType(p) {
  if (PDF_EXT.test(p.file_path)) return "pdf";
  const kind = p.file_path ? mediaKind(p.file_path) : p.icon_path ? mediaKind(p.icon_path) : "";
  if (kind === "video") return "video";
  if (kind === "image" || kind === "gif") return "image";
  if (p.copy_image && hasImage(p)) return "image";
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
  const stored = imageOf(p);
  if (stored) return stored;
  // A stored image outranks any path preview — while it is still loading, show
  // nothing rather than briefly swapping in the attachment's own thumbnail.
  if (hasImage(p)) return "";
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
// `cache` reuses the element across re-renders of the SAME panel instead of
// re-decoding the same base64 blob on every keystroke — appending an already-
// mounted node just reparents it, so an id whose thumb type stays the same never
// gets a fresh <img>/<video>.
function thumbEl(p, cache) {
  if (!flag("imagePreview")) return thumbDot(p);
  const vsrc = flag("libraryVideoPreview") ? videoSrc(p) : "";
  const src = vsrc ? "" : thumbSrc(p);
  const key = vsrc ? `v:${vsrc}` : src ? `i:${src}` : "";
  if (!key) {
    const dot = thumbDot(p);
    // A stored image still on its way: the dot stands in and swaps itself out.
    if (!vsrc && hasImage(p)) dot.dataset[IMG_WAIT] = p.id;
    return dot;
  }
  const cached = cache.get(p.id);
  if (cached && cached.key === key) {
    // renderLibrary/renderJournal build a FRESH IntersectionObserver every
    // render (disconnecting the old one) -- a cache hit must re-observe with the
    // current one, or a video that survives a re-render (e.g. the search field)
    // stops being paused/started by scroll after the first refresh.
    if (vsrc && _activeVidObs) _activeVidObs.observe(cached.el);
    return cached.el;
  }
  let el;
  if (vsrc) {
    el = document.createElement("video");
    el.className = "lib-thumb";
    el.muted = true;
    el.loop = true;
    el.playsInline = true;
    el.preload = "metadata";
    el.draggable = false;
    el.src = convertFileSrc(vsrc);
    if (_activeVidObs) _activeVidObs.observe(el);
    el.addEventListener("click", (e) => { e.stopPropagation(); openLightbox(el.src, true); });
  } else {
    el = document.createElement("img");
    el.className = "lib-thumb";
    el.src = src;
    el.draggable = false;
    el.addEventListener("click", (e) => { e.stopPropagation(); openLightbox(el.src, false); });
  }
  cache.set(p.id, { key, el });
  return el;
}

// ---- Additive type filters, shared by the library and the copy history ----
// Nothing picked = every type. "All" clears the set, every other chip toggles itself,
// so PDF and File can be shown together. The pick is stored per window.
const typeFilterOf = (key) => new Set((settings.ui_texts?.[key] || "").split(",").filter(Boolean));

function paintTypeChips(box, set) {
  for (const btn of box.querySelectorAll(".lib-type[data-type]")) {
    const type = btn.dataset.type;
    btn.classList.toggle("active", type === "all" ? set.size === 0 : set.has(type));
  }
}

function toggleTypeFilter(set, type, box, key) {
  if (type === "all") set.clear();
  else if (!set.delete(type)) set.add(type);
  paintTypeChips(box, set);
  const value = [...set].join(",");
  settings.ui_texts = { ...(settings.ui_texts || {}), [key]: value };
  invoke("set_ui_text", { key, value }).catch(() => {});
}

// A prompt passes when no type is picked, or when its own type is among them.
const typeAllowed = (set, p) => set.size === 0 || set.has(promptType(p));

// ---- Fuzzy search: tiered scoring (exact > prefix > fuzzy) over the same
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
// Whether a prompt satisfies one tag:/var: condition from the search query.
// tag: matches a tag's own identity (tagKey), case-insensitive, not a
// substring -- the plain search already does substring matching over tag
// text. var: matches a placeholder's name, read the same way the variable
// dialog itself reads it (VAR_RE over the body, then the "|options" split).
function searchConditionMet(c, p) {
  let hit;
  if (c.prefix === "tag") {
    hit = (p.tags || []).some((name) => tagKey(name) === tagKey(c.value));
  } else {
    const names = [p.text, ...(p.variants || [])].flatMap((t) => extractVars(t || ""));
    hit = names.some((tok) => parseVar(tok).name.toLowerCase() === c.value);
  }
  return c.neg ? !hit : hit;
}
function computeLibSearchScore(p) {
  const rawInput = libQuery.trim().toLowerCase();
  if (!rawInput) return { score: 1, fuzzy: false, entryIndex: 0 };
  // tag:word and var:word (and their negated "-tag:"/"-var:" forms) are hard
  // conditions, pulled out before anything else runs -- a prompt that fails
  // one of them is out, whatever the rest of the query would otherwise have
  // scored it. parseSearchQuery leaves everything else untouched.
  const { rest, conditions } = parseSearchQuery(rawInput);
  if (conditions.length && !conditions.every((c) => searchConditionMet(c, p))) {
    return { score: 0, fuzzy: false, entryIndex: 0 };
  }
  const raw = rest;
  if (!raw) return { score: 1, fuzzy: false, entryIndex: 0 }; // only tag:/var: terms given
  // entryIndex tags which entry a body-field match came from (0 = p.text, the
  // library row's default preview; 1+ = variants[entryIndex-1]) — a rotating
  // prompt is findable by ANY entry, and the row can then show the one that
  // actually matched instead of always entry 1 regardless of why it appeared.
  const fields = [[(p.name || "").toLowerCase(), 3, -1], [(p.text || "").toLowerCase(), 1, 0], [(p.file_path || "").toLowerCase(), 1, -1]];
  (p.variants || []).forEach((v, i) => fields.push([(v || "").toLowerCase(), 1, i + 1]));
  // A tag is a deliberate label, so it ranks between the name and the body.
  if (tagsOn() && (p.tags || []).length) fields.push([p.tags.join(" ").toLowerCase(), 2, -1]);
  // A note is private context, so it matches but ranks below the prompt body.
  if (notesOn() && p.notes) fields.push([p.notes.toLowerCase(), 0.5, -1]);
  // A "-word" term excludes any prompt whose searchable text contains it,
  // checked across every field before anything else runs.
  const terms = raw.split(/\s+/).filter(Boolean);
  const isExclude = (t) => t.length > 1 && t[0] === "-";
  const excludes = terms.filter(isExclude).map((t) => t.slice(1));
  if (excludes.some((ex) => fields.some(([val]) => val.includes(ex)))) return { score: 0, fuzzy: false, entryIndex: 0 };
  const q = terms.filter((t) => !isExclude(t)).join(" ");
  if (!q) return { score: 1, fuzzy: false, entryIndex: 0 }; // exclusion-only query matches everything not excluded
  let best = 0, bestEntry = 0;
  for (const [val, w, entry] of fields) {
    if (val && val.includes(q)) {
      const s = (val.startsWith(q) ? 120 : 100) * w;
      if (s > best) { best = s; bestEntry = entry >= 0 ? entry : 0; }
    }
  }
  if (best) return { score: best, fuzzy: false, entryIndex: bestEntry };
  if (!flag("fuzzySearch")) return { score: 0, fuzzy: false, entryIndex: 0 };
  // Tokenize on any non-alphanumeric so "code," / "code." / code blocks split into
  // clean words (scripts without spaces, e.g. CJK, stay one token — substring wins).
  const qWords = fuzzyTokens(q);
  if (!qWords.length) return { score: 0, fuzzy: false, entryIndex: 0 };
  for (const [val, w, entry] of fields) {
    if (!val) continue;
    const words = fuzzyTokens(val);
    let matched = 0;
    for (const qw of qWords) {
      const max = fuzzyMaxTyposFor(qw);
      if (words.some((vw) => fuzzyWordHit(qw, vw, max))) matched++;
    }
    if (matched === qWords.length) {
      const s = 40 * w;
      if (s > best) { best = s; bestEntry = entry >= 0 ? entry : 0; }
    }
  }
  return { score: best, fuzzy: best > 0, entryIndex: bestEntry };
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
  if (!typeAllowed(libTypes, p)) return false;
  if (libColor !== "all" && (p.color || "") !== libColor) return false;
  if (!tagMatches(p)) return false;
  return libSearchScore(p).score > 0;
}

// Selected tag chips narrow the list, combined with search and the other
// filters. AND (default) needs every chip, OR any of them. The grid's own
// filter (gridTags) reuses this with its own Set instead of the library's.
function tagMatches(p, tagSet = libTags) {
  if (!tagsOn() || !tagSet.size) return true;
  const own = new Set((p.tags || []).map(tagKey));
  const wanted = [...tagSet];
  return (txt("tagFilterMode") || "and") === "or"
    ? wanted.some((k) => own.has(k))
    : wanted.every((k) => own.has(k));
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
    if (color && color !== "all") {
      b.style.background = color;
      b.title = color;
      b.setAttribute("aria-label", color);
    }
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

// "last edited" per prompt — from prompt_versions (a version row is only
// ever written on a real name/text change, see update_prompt), refetched
// whenever the library opens or an edit lands while it's already open. A
// prompt absent from the map has no version row (history off, or never
// content-edited since creation) and sorts to the end, same as an unused
// prompt already does under "most used"/"recently used" above.
let lastEdited = {};
async function refreshLastEdited() {
  if (!flag("promptHistory")) { lastEdited = {}; return; }
  try { lastEdited = await invoke("last_edited_map"); } catch { lastEdited = {}; }
}
// Smart sort: order the library list. The usage-based options gate on smartSort.
function libSortMode() {
  const raw = settings.ui_texts?.librarySort || "default";
  if ((raw === "mostUsed" || raw === "recentUsed") && !flag("smartSort")) return "default";
  if (raw === "lastEdited" && !flag("promptHistory")) return "default";
  return raw;
}
function sortLibraryItems(items) {
  const mode = libSortMode();
  const q = libQuery.trim();
  const nameOf = (p) => libLabel(p).toLowerCase();
  // One Collator for the whole sort instead of one per pairwise comparison —
  // localeCompare(str, locale) builds a fresh one internally on every call.
  const collator = new Intl.Collator(LANG);
  const byName = (a, b) => collator.compare(nameOf(a), nameOf(b));
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
  } else if (mode === "lastEdited") {
    items.sort((a, b) => ((lastEdited[b.id] || 0) - (lastEdited[a.id] || 0)) || byName(a, b));
  }
}
function fillLibrarySort() {
  const sel = $("library-sort");
  if (!sel) return;
  const opts = [["default", "sortDefault"], ["name", "sortName"]];
  if (flag("smartSort")) opts.push(["mostUsed", "sortMostUsed"], ["recentUsed", "sortRecentUsed"]);
  if (flag("promptHistory")) opts.push(["lastEdited", "sortLastEdited"]);
  sel.innerHTML = "";
  for (const [v, k] of opts) {
    const o = document.createElement("option");
    o.value = v; o.textContent = t(k);
    sel.appendChild(o);
  }
  sel.value = libSortMode();
}

// Close the library and flush any deferred grid rebuild from batch edits.
// The library was raised over the backups dialog by jumpToPrompt. Put it back down;
// `closeBehind` also dismisses the dialogs underneath (dragging onto the grid needs
// the grid). Returns true if the library was raised at all.
function dropLibraryFront(closeBehind) {
  if (!document.body.classList.contains("library-front")) return false;
  document.body.classList.remove("library-front");
  if (closeBehind) {
    closeBackups();
    settingsEl.classList.add("hidden");
  }
  return true;
}
function hideLibrary() {
  libraryEl.classList.add("hidden");
  // Prompts may have been edited or deleted while the library was up — the
  // statistics underneath would otherwise still show the old numbers.
  if (dropLibraryFront(false)) renderStatsPanel($("stats-panel"));
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
  renderTagFilter();
  const anyLibFilter = flag("librarySearch") || flag("libraryTypeFilter") || flag("colorFilter") || flag("favorites") || tagsOn();
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
  // How hard the filters are biting, in the header where the filters are.
  const count = $("library-count");
  if (count) {
    const all = prompts.length;
    count.textContent = items.length === all
      ? String(all)
      : t("libraryCount").replace("{n}", items.length).replace("{v}", all);
  }
  sortLibraryItems(items); // chosen sort (stable), before the favorites float
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
  // A folder member has no layout entry of its own (the folder draws it), so
  // without this it would show as addable — placing it would silently no-op since
  // renderGrid always skips ids listed in a folder's members.
  for (const f of foldersOfView(activeView().id)) for (const id of f.members || []) placed.add(id);
  libDisplayOrder = items.map((p) => p.id); // Range-select uses display order
  // Batch through a fragment -- appending straight into the live list forced
  // a reflow per row on a large library.
  const frag = document.createDocumentFragment();
  for (const p of items) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "lib-item";
    row.dataset.id = p.id; // In-place selection toggle finds the row by id
    // Dim rows that only matched via the typo-tolerant tier.
    if (libQuery.trim() && libSearchScore(p).fuzzy) row.classList.add("lib-fuzzy");
    // Selection state (the checkbox itself is a CSS ::before, shown while the
    // list carries the .selecting class — so toggling select mode never re-renders).
    if (libSelectMode && libSelected.has(p.id)) row.classList.add("lib-selected");
    row.title = libSelectMode ? "" : t("copy");
    const rowTimes = tileTimeLines(p.id);
    if (rowTimes) row.dataset.tipTimes = rowTimes;

    const body = document.createElement("span");
    body.className = "lib-body";
    const name = document.createElement("span");
    name.className = "lib-name";
    name.textContent = libLabel(p);
    // A search hit inside entry 2+ shows THAT entry (why the row matched at
    // all) instead of always entry 1, plus a small "n/total" tag naming it.
    const hitEntry = libQuery.trim() ? libSearchScore(p).entryIndex : 0;
    const hitVariant = hitEntry > 0 ? (p.variants || [])[hitEntry - 1] : null;
    // file_path still wins over a variant hit — a file/image prompt can never
    // actually HAVE variants (the editor gates rotation to text-only prompts), so
    // hitVariant is never non-null on a prompt that also has a file_path.
    const text = document.createElement("span");
    text.className = "lib-text";
    text.textContent = p.file_path || (hitVariant ?? p.text);
    body.appendChild(name);
    if (hitVariant != null) {
      const tag = document.createElement("span");
      tag.className = "lib-rotate-tag";
      tag.textContent = `${hitEntry + 1}/${1 + p.variants.length}`;
      body.appendChild(tag);
    }
    body.appendChild(text);
    if (tagsOn() && (p.tags || []).length) {
      const row2 = document.createElement("span");
      row2.className = "lib-tags";
      for (const tn of p.tags) row2.appendChild(tagChipEl(tn));
      body.appendChild(row2);
    }

    // Per-type preview: video plays inline, image/gif as a thumbnail, else a dot.
    row.append(thumbEl(p, libThumbCache), body);

    // Place on the current layout: drag the row onto the grid, or one click.
    row.addEventListener("pointerdown", (e) => {
      if (libSelectMode) return; // no drag while multi-selecting
      if (e.button !== 0 || e.target.closest(".lib-add, .lib-edit, .lib-fav, .lib-delete")) return;
      const out = armDragOut(row, e, () => prompts.find((x) => x.id === p.id));
      drag = { id: p.id, startX: e.clientX, startY: e.clientY, moved: false, el: row, fromLibrary: true, out };
    });
    // Add-to-view icon (leftmost of the action group): place on the current layout.
    if (!placed.has(p.id)) {
      const add = document.createElement("span");
      add.className = "icon-btn lib-add";
      add.title = t("addToLayout");
      putIcon(add, GRID_PLUS);
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
      makeKeyboardActivatable(add);
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
        try {
          await invoke("set_favorite", { id: p.id, favorite: p.favorite });
        } catch (err) {
          p.favorite = !p.favorite; // backend rolled back on a failed save — mirror it locally
          toast(String(err) === "save-failed" ? t("persistFailed") : String(err));
        }
        renderLibrary();
        if (favView) renderGrid(true); // favorites grid updates live underneath
      });
      makeKeyboardActivatable(fav);
      row.appendChild(fav);
    }
    // Edit icon (rightmost): opens the edit dialog without copying.
    const edit = document.createElement("span");
    edit.className = "icon-btn lib-edit";
    edit.title = t("edit");
    putIcon(edit, ICON_EDIT);
    edit.addEventListener("pointerdown", (e) => e.stopPropagation());
    edit.addEventListener("click", (e) => { e.stopPropagation(); editPrompt(p.id); });
    makeKeyboardActivatable(edit);
    row.appendChild(edit);

    // Delete (far right): the only destructive action in the row, so it asks first.
    const del = document.createElement("span");
    del.className = "icon-btn lib-delete";
    del.title = t("delete");
    putIcon(del, ICON_TRASH);
    del.addEventListener("pointerdown", (e) => e.stopPropagation());
    del.addEventListener("click", async (e) => {
      e.stopPropagation();
      const ok = await confirmDialog({
        title: t("delete"),
        message: `${t("deleteConfirm")}\n\n${p.name}`,
        confirmLabel: t("delete"),
      });
      if (!ok) return;
      try {
        await invoke("delete_prompt", { id: p.id });
      } catch (err) {
        toast(String(err) === "save-failed" ? t("persistFailed") : String(err));
        return;
      }
      prompts = prompts.filter((x) => x.id !== p.id);
      renderLibrary();
      await renderGrid(); // the tile (or its folder) is gone from the grid as well
    });
    makeKeyboardActivatable(del);
    row.appendChild(del);

    // Click copies the prompt (or toggles selection in batch mode).
    row.addEventListener("click", (e) => {
      if (libSelectMode) { toggleLibSelect(p.id, e.shiftKey); return; }
      libraryCopy(p);
    });
    frag.appendChild(row);
  }
  list.appendChild(frag);
}

// ---- Batch operations ----
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
  // Same feature gate as the rest of the tag UI (filter chips, manager button).
  const tagsEnabled = tagsOn();
  $("lib-batch-tag-add").classList.toggle("hidden", !tagsEnabled);
  $("lib-batch-tag-remove").classList.toggle("hidden", !tagsEnabled);
  bar.querySelectorAll("button, select").forEach((el) => {
    if (["lib-batch-all", "lib-batch-none"].includes(el.id)) return; // always usable
    el.disabled = n === 0;
  });
}
// Same free-color palette as the editor (buildSwatches + openColorPop)
// instead of "none" + presets only — no selection is ever marked here since a
// batch can hold prompts of different colors.
let batchColorDebounce = 0;
function buildBatchColors(wrapId = "lib-batch-colors") {
  const wrap = $(wrapId);
  if (!wrap) return;
  const run = wrapId === "grid-batch-colors"
    ? (color) => doGridBatch("color", { color })
    : (color) => doBatch("color", { color });
  // The free picker's SV field fires onPick on every drag frame (fine for the
  // editor's cheap local-state preview) — a batch write per frame would spam
  // the backend and clear the selection mid-drag, so settle before committing.
  const debouncedRun = (color) => {
    clearTimeout(batchColorDebounce);
    batchColorDebounce = setTimeout(() => run(color), 200);
  };
  buildSwatches(wrap, "", run, (anchor, cur) => openColorPop(anchor, cur, debouncedRun));
}
function fillBatchViews(selId = "lib-batch-view") {
  const sel = $(selId);
  if (!sel) return;
  sel.innerHTML = "";
  for (const v of settings.views || []) {
    const o = document.createElement("option");
    o.value = v.id; o.textContent = v.name;
    sel.appendChild(o);
  }
  if (settings.active_view) sel.value = settings.active_view;
}

// ---- Grid bulk selection — reuses the same batch_prompts backend, its own
// selection state and toolbar (folder tiles are excluded, see gridSelected). ----
function toggleGridSelect(id) {
  if (gridSelected.has(id)) gridSelected.delete(id); else gridSelected.add(id);
  const was = document.body.classList.contains("grid-selecting");
  const now = gridSelected.size > 0;
  document.body.classList.toggle("grid-selecting", now);
  applyGridSelectionDom();
  updateGridBatchBar();
  // The composer folds away only at the 0<->1 transition — refit tile text
  // to the grid's new height then, not on every further checkbox click.
  if (was !== now) fitAllTiles();
}
function applyGridSelectionDom() {
  for (const tile of gridEl.querySelectorAll(".tile")) {
    tile.classList.toggle("grid-selected", gridSelected.has(tile.dataset.id));
  }
}
function updateGridBatchBar() {
  const n = gridSelected.size;
  $("grid-batch-bar")?.classList.toggle("hidden", n === 0);
  const count = $("grid-batch-count");
  if (count) count.textContent = t("selectedCount").replace("{n}", n.toLocaleString(LANG));
  // buildBatchColors() no-ops once already built; fillBatchViews() harmlessly
  // resets the dropdown to the active view each time, which is the right default
  // for a selection that just grew.
  if (n > 0) { buildBatchColors("grid-batch-colors"); fillBatchViews("grid-batch-view"); }
}
function clearGridSelection() {
  const was = document.body.classList.contains("grid-selecting");
  gridSelected.clear();
  document.body.classList.remove("grid-selecting");
  applyGridSelectionDom();
  updateGridBatchBar();
  if (was) fitAllTiles(); // composer reappeared — refit tile text to the new height
}
// addView is all-or-nothing on the backend now — a full target view rejects
// the whole batch with this one error string instead of silently dropping ids.
function batchErrorToast(e) {
  toast(String(e).includes("view-full") ? t("gridFull") : String(e));
}
async function doGridBatch(action, extra = {}) {
  const ids = [...gridSelected];
  if (!ids.length) return;
  const idset = new Set(ids);
  try {
    settings = await invoke("batch_prompts", {
      ids, action,
      color: extra.color ?? null,
      favorite: extra.favorite ?? null,
      viewId: extra.viewId ?? null,
    });
  } catch (e) { batchErrorToast(e); return; }
  if (action === "delete") prompts = prompts.filter((p) => !idset.has(p.id));
  else if (action === "color") { for (const p of prompts) if (idset.has(p.id)) p.color = extra.color ?? ""; }
  clearGridSelection();
  await renderGrid(true);
}
// A real move (not just "also in"): drop the current view's placement first, then
// place into the target — matches the "a prompt lives in exactly one view"
// invariant everywhere else in the app relies on.
async function doGridMove(targetViewId) {
  const ids = [...gridSelected];
  if (!ids.length || !targetViewId) return;
  try {
    // One moveView call - both layouts change in the same backend settings
    // write, so a crash mid-call can no longer leave a tile placed in both views
    // (the old addView-then-removeView pair was two separate IPC round-trips).
    settings = await invoke("batch_prompts", { ids, action: "moveView", color: null, favorite: null, viewId: targetViewId, fromViewId: settings.active_view });
  } catch (e) { batchErrorToast(e); return; }
  clearGridSelection();
  await renderGrid(true);
}

// Returns whether the batch actually went through, so a caller showing its own
// "done" toast (delete, tag add/remove) never reports success over a failure
// batchErrorToast already surfaced.
async function doBatch(action, extra = {}) {
  const ids = [...libSelected];
  if (!ids.length) return false;
  const idset = new Set(ids);
  try {
    // Backend now returns ONLY settings (no prompt echo) — so a batch over
    // image-heavy prompts no longer re-serialises every base64 blob over IPC.
    settings = await invoke("batch_prompts", {
      ids, action,
      color: extra.color ?? null,
      favorite: extra.favorite ?? null,
      viewId: extra.viewId ?? null,
      tag: extra.tag ?? null,
    });
  } catch (e) { batchErrorToast(e); return false; }
  // Mirror the change onto the local prompt list ourselves.
  if (action === "delete") prompts = prompts.filter((p) => !idset.has(p.id));
  else if (action === "favorite") { for (const p of prompts) if (idset.has(p.id)) p.favorite = extra.favorite ?? true; }
  else if (action === "color") { for (const p of prompts) if (idset.has(p.id)) p.color = extra.color ?? ""; }
  else if (action === "tag-add") {
    const key = tagKey(extra.tag);
    for (const p of prompts) {
      if (idset.has(p.id) && !(p.tags || []).some((t) => tagKey(t) === key)) {
        p.tags = [...(p.tags || []), extra.tag];
      }
    }
  } else if (action === "tag-remove") {
    const key = tagKey(extra.tag);
    for (const p of prompts) if (idset.has(p.id)) p.tags = (p.tags || []).filter((t) => tagKey(t) !== key);
  }
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
  return true;
}
async function batchExport(format) {
  const ids = [...libSelected];
  if (!ids.length) return;
  try {
    const n = await invoke("export_prompts", { format, ids });
    toast(t("exportDone").replace("{n}", String(n)));
  } catch (e) {
    if (String(e) !== "canceled") errToast(e);
  }
}

// Copy a prompt, filling {#{variables}#} first when applicable — reserved names
// resolve silently, the rest go through the dialog. Returns false if the copy
// failed or the user cancelled. The single copy entry point for every path that
// does not run the dialog's own double-click handling (grid, library, journal).
async function copyResolved(p) {
  // Rotating prompt — peek the entry copy_prompt would use, resolve variables
  // on THAT text (fixes {#{...}#} tokens, if any, in it), then commit only once the
  // copy actually reaches the clipboard, so a cancelled variable dialog never
  // burns an entry.
  if ((p.variants || []).length) {
    const peek = await invoke("rotation_peek", { id: p.id }).catch(() => null);
    const rawText = peek ? peek.text : p.text;
    const text = varTokens(p, rawText).length ? await resolveVars(rawText) : rawText;
    if (text === null || text === false) return false;
    // Handing the id over is what lets the backend open an auto-advance session for
    // an eligible prompt — without it this path copied plain text and the Auto
    // switch did nothing at all here, while the floating buttons (which copy through
    // copy_prompt) honoured it.
    const armId = p.rotate_auto && !anyEntryHasVars(p) ? p.id : undefined;
    const ok = await invoke("copy_text", { text, id: armId }).catch(() => false);
    // A failed peek already fell back to p.text above — committing anyway would
    // advance whatever the pointer's REAL position is, not the entry we actually
    // just copied. Only a successful peek may be followed by a commit.
    if (ok && peek) invoke("rotation_commit", { id: p.id }).catch(() => {});
    return ok;
  }
  if (varTokens(p).length) {
    const text = await resolveVars(p.text);
    if (text === null) return false;
    return invoke("copy_text", { text }).catch(() => false);
  }
  return invoke("copy_prompt", { id: p.id }).catch((e) => { errToast(e); return false; });
}

// A blink says "something happened"; the character count says the whole prompt went
// to the clipboard, which is the doubt a long one leaves.
function toastCopied(p) {
  const n = (p?.text || "").length;
  toast(n > 0 ? t("copiedChars").replace("{n}", n.toLocaleString(LANG)) : t("copied"));
}

async function libraryCopy(p) {
  // Prompts that still need input skip the double-click shortcut: they must reach
  // the dialog (copyResolved) so the variables get asked before copy + auto-paste.
  if (!dialogTokens(p, await peekRotatedText(p)).length && autoPasteDouble(p.id)) { await autoPasteNow(null); return; } // 2nd fast click pastes
  if (copyOnCooldown(p.id)) return;
  if (!(await copyResolved(p))) return;
  recordCopy(p.id);
  toastCopied(p);
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

// Library list columns. ONE place decides it: the expert value. The segmented
// toggle in the library header was a third control for the same number, next to the
// flag that showed it and the value that capped it.
const libCols = () => Math.min(6, Math.max(1, Math.round(val("libraryCols"))));
function syncLibraryCols() {
  $("library-list").style.setProperty("--lib-cols", libCols());
}

// ---- Copy history & usage journal ----

// One history entry, built like a library row on purpose: same thumbnail, same name
// and text lines, same tag chips. What the history adds is the meta line — when it
// was copied on the left, how often on the right — and a chip naming the type for
// everything that is not plain text.
function journalRow(p, meta, onClick) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "lib-item journal-item";
  row.title = t("copy");
  const body = document.createElement("span");
  body.className = "lib-body";
  const head = document.createElement("span");
  head.className = "journal-name-row";
  const name = document.createElement("span");
  name.className = "lib-name";
  name.textContent = libLabel(p);
  head.append(name);
  const kind = promptType(p);
  if (kind !== "text") {
    const chip = document.createElement("span");
    chip.className = "journal-kind";
    chip.textContent = t(`filter${kind[0].toUpperCase()}${kind.slice(1)}`);
    head.append(chip);
  }
  body.append(head);
  const preview = p.file_path || p.text;
  if (preview) {
    const text = document.createElement("span");
    text.className = "lib-text";
    text.textContent = preview;
    body.append(text);
  }
  if (tagsOn() && (p.tags || []).length) {
    const tags = document.createElement("span");
    tags.className = "lib-tags";
    for (const tn of p.tags) tags.appendChild(tagChipEl(tn));
    body.append(tags);
  }
  if (meta && (meta.left || meta.right)) {
    const sub = document.createElement("span");
    sub.className = "lib-sub lib-subrow journal-meta";
    const l = document.createElement("span");
    l.textContent = meta.left || "";
    const r = document.createElement("span");
    r.className = "lib-sub-right";
    r.textContent = meta.right || "";
    sub.append(l, r);
    body.append(sub);
  }
  row.append(thumbEl(p, journalThumbCache), body);
  row.addEventListener("click", onClick);
  // Edit straight from the history — same icon/pattern as the library row.
  const edit = document.createElement("span");
  edit.className = "icon-btn lib-edit";
  edit.title = t("edit");
  putIcon(edit, ICON_EDIT);
  edit.addEventListener("pointerdown", (e) => e.stopPropagation());
  edit.addEventListener("click", (e) => { e.stopPropagation(); editPrompt(p.id); });
  makeKeyboardActivatable(edit);
  row.appendChild(edit);
  // Only the "recently copied" column carries a rowid — "most used" is a
  // usage-count aggregate with no single copy_log row a delete could target.
  if (meta && meta.rowid != null) {
    const del = document.createElement("span");
    del.className = "icon-btn journal-delete";
    del.title = t("delete");
    del.setAttribute("aria-label", t("delete"));
    putIcon(del, ICON_TRASH);
    del.addEventListener("pointerdown", (e) => e.stopPropagation());
    del.addEventListener("click", async (e) => {
      e.stopPropagation();
      const ok = await confirmDialog({
        title: t("delete"),
        message: t("journalEntryDeleteConfirm"),
        confirmLabel: t("delete"),
      });
      if (!ok) return;
      try { await invoke("delete_copy_entry", { rowid: meta.rowid }); } catch (err) { errToast(err); return; }
      await loadJournal(); // refetch — the entry (and only it) disappears
    });
    makeKeyboardActivatable(del);
    row.appendChild(del);
  }
  return row;
}

// Format a unix-seconds copy timestamp for the history column.
const fmtCopyTime = (ts) => {
  if (!ts) return "";
  return new Date(ts * 1000).toLocaleString(LANG, { dateStyle: "short", timeStyle: "short" });
};

// The "last copied" / "last edited" lines a tile or library row's tooltip
// shows, each hanging off the function that actually owns that data -- the copy
// timestamp off the same settings.last_used map smartSort reads, the edit
// timestamp off promptHistory's lastEdited map -- so a value missing because its
// own feature is off (or just never happened yet) drops only its own line.
function tileTimeLines(id) {
  if (!flag("tileTimeTip")) return "";
  const lines = [];
  const copied = settings.last_used?.[id];
  if (copied) lines.push(t("tipLastCopied").replace("{time}", fmtCopyTime(copied)));
  const edited = lastEdited[id];
  if (edited) lines.push(t("tipLastEdited").replace("{time}", fmtCopyTime(edited)));
  return lines.join("\n");
}

function journalMatches(p) {
  if (!typeAllowed(journalTypes, p)) return false;
  if (journalFav && flag("favorites") && !p.favorite) return false;
  if (journalColor !== "all" && (p.color || "") !== journalColor) return false;
  if (flag("journalTagFilter") && !tagMatches(p, journalTags)) return false;
  const q = journalQuery.trim().toLowerCase();
  if (!q) return true;
  return [p.name, p.text, p.file_path].some((s) => (s || "").toLowerCase().includes(q));
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

// Same bar as the library's (renderTagBar) -- only the extra
// journalTagFilter toggle and the hidden-until-enabled state are journal-only.
let _lastJournalTagFilterSig = null;
function renderJournalTagFilter() {
  const bar = $("journal-tags");
  if (!bar) return;
  if (!tagsOn() || !flag("journalTagFilter")) {
    bar.innerHTML = "";
    bar.classList.add("hidden");
    _lastJournalTagFilterSig = null;
    journalTags.clear();
    return;
  }
  const names = allTags();
  const live = new Set(names.map(tagKey));
  for (const key of [...journalTags]) if (!live.has(key)) journalTags.delete(key);
  // Same as the library's own bar: "manage tags" stays reachable even with
  // zero tags, so this no longer hides on an empty tag set.
  bar.classList.remove("hidden");
  const sig = names.join("") + "" + [...journalTags].sort().join("");
  if (sig === _lastJournalTagFilterSig) return;
  _lastJournalTagFilterSig = sig;
  renderTagBar(bar, journalTags, renderJournal);
}

// Render from the cached log. Filters run client-side, so typing in the search
// box re-renders instantly without an IPC roundtrip (and without a render race).
function renderJournal() {
  renderJournalTagFilter();
  const body = $("journal-body");
  if (_journalObs) _journalObs.disconnect();
  _journalObs = makeVideoObserver();
  _activeVidObs = _journalObs;
  const byId = new Map(prompts.map((p) => [p.id, p]));
  const grouped = flag("historyGrouped");
  const showTime = flag("historyTimestamps");
  let recent = journalLog
    .map((e) => [byId.get(e.id), e.ts, e.rowid])
    .filter(([p]) => p && journalMatches(p));
  if (journalRecentRev) recent = recent.slice().reverse();
  // How hard the filters are biting, in the header where the filters are —
  // same wording/element as the library, counted against entries with a
  // still-existing prompt (a log row for a deleted prompt is not "all").
  const jcount = $("journal-count");
  if (jcount) {
    const all = journalLog.filter((e) => byId.get(e.id)).length;
    jcount.textContent = recent.length === all
      ? String(all)
      : t("libraryCount").replace("{n}", recent.length).replace("{v}", all);
  }
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
      toastCopied(p);
      settings = await invoke("get_settings");
      await loadJournal();
    }
  };
  // `rows` items are [prompt, meta]. `extra` is an optional control rendered to the
  // right of the column title.
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
      for (const [p, meta] of rows) col.appendChild(journalRow(p, meta, copyAgain(p)));
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
      try { await invoke("set_ui_flag", { key: "historyGrouped", enabled: next }); } catch (err) { errToast(err); }
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
    column("recentlyCopied",
      // Rowid rides along so journalRow() can offer a per-entry delete —
      // "most used" (below) has no single copy_log row to point at, so it stays
      // without one.
      recent.map(([p, ts, rowid]) => [p, { left: showTime ? fmtCopyTime(ts) : "", rowid }]),
      headExtras(groupBtn, recentSort), "journal-col-recent"),
    // One meta line: last-used time on the left, copy count on the right.
    column("mostUsed", used.map(([p, n, ts]) => [
      p,
      { left: showTime && ts ? fmtCopyTime(ts) : "", right: t("usedTimes").replace("{n}", n) },
    ]), headExtras(usedSort), "journal-col-used"),
  );
}

// In-app documentation. EN + DE only (ui/help.js) — every other language
// falls back to English rather than showing untranslated placeholder text.
let helpQuery = "";
let helpTab = 0;
let helpCollapsed = new Set(); // in-memory only; resets each time the dialog opens

function helpCategories() {
  return HELP_CONTENT[LANG] || HELP_CONTENT.en;
}

// Section ids are stable keys (see ui/help.js), never translated text — the
// label comes from a helpSec-prefixed key (via t) so it follows the app's own
// 11 languages even though the article content itself stays EN/DE-only.
function helpSectionLabel(id) {
  return t(`helpSec${id.charAt(0).toUpperCase()}${id.slice(1)}`);
}

// Strips case and diacritics so "ordner"/"Übersicht" match "Ordner"/"ubersicht" alike.
const helpNorm = (s) => (s || "").toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "");

// Splits text into plain text nodes + <mark> spans around every term match. Never
// innerHTML — the search query is user input and must not be parsed as markup.
function markTerms(el, text, terms) {
  el.textContent = "";
  const norm = helpNorm(text);
  const ranges = [];
  for (const term of terms) {
    if (!term) continue;
    let from = 0;
    for (let idx; (idx = norm.indexOf(term, from)) !== -1; from = idx + term.length) {
      ranges.push([idx, idx + term.length]);
    }
  }
  if (!ranges.length) { el.appendChild(document.createTextNode(text)); return; }
  ranges.sort((a, b) => a[0] - b[0]);
  const merged = [ranges[0]];
  for (const r of ranges.slice(1)) {
    const last = merged[merged.length - 1];
    if (r[0] <= last[1]) last[1] = Math.max(last[1], r[1]);
    else merged.push(r);
  }
  let pos = 0;
  for (const [start, end] of merged) {
    if (start > pos) el.appendChild(document.createTextNode(text.slice(pos, start)));
    const mark = document.createElement("mark");
    mark.className = "help-mark";
    mark.textContent = text.slice(start, end);
    el.appendChild(mark);
    pos = end;
  }
  if (pos < text.length) el.appendChild(document.createTextNode(text.slice(pos)));
}

function renderHelp() {
  const box = $("help-body");
  const tabBar = $("help-tabs-bar");
  box.innerHTML = "";
  tabBar.innerHTML = "";
  const highlight = flag("helpHighlight");
  const terms = helpNorm(helpQuery).trim().split(/\s+/).filter(Boolean);
  const searching = terms.length > 0;
  const cats = helpCategories();
  const sections = [...new Set(cats.map((c) => c.section))];
  if (helpTab >= sections.length) helpTab = 0;
  // Tab bar lives in the fixed header (stays visible while the rows scroll); it is
  // hidden while searching across every section, same pattern as the expert menu.
  if (!searching && sections.length > 1) {
    const bar = document.createElement("div");
    bar.className = "expert-tabs";
    sections.forEach((id, i) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "expert-tab" + (i === helpTab ? " active" : "");
      b.textContent = helpSectionLabel(id);
      b.addEventListener("click", () => { helpTab = i; box.scrollTop = 0; renderHelp(); });
      bar.appendChild(b);
    });
    tabBar.appendChild(bar);
  }
  const activeSection = searching ? null : sections[helpTab];
  // Fields carry a search weight (title matters far more than a settings line); a
  // term must appear in SOME field for the article to count as a hit at all.
  let list;
  if (searching) {
    const wholeQuery = helpNorm(helpQuery).trim();
    const scored = [];
    for (const cat of cats) {
      const fields = [
        [cat.title, 10],
        [cat.whatItIs, 4],
        [(cat.howTo || []).join(" "), 3],
        [cat.whatHappens || "", 2],
        [cat.settings.join(" "), 2],
        [cat.tip || "", 1],
      ];
      let score = 0;
      let allTermsHit = true;
      for (const term of terms) {
        let termHit = false;
        for (const [text, weight] of fields) {
          if (helpNorm(text).includes(term)) { score += weight; termHit = true; }
        }
        if (!termHit) { allTermsHit = false; break; }
      }
      if (!allTermsHit) continue;
      if (helpNorm(cat.title).includes(wholeQuery)) score += 20;
      scored.push({ cat, score });
    }
    scored.sort((a, b) => b.score - a.score);
    list = scored.map((s) => s.cat);
  } else {
    list = cats.filter((cat) => cat.section === activeSection);
  }
  const shown = list.length;
  if (searching) {
    const hits = document.createElement("div");
    hits.className = "hint help-hits";
    hits.textContent = t("helpHits").replace("{n}", String(shown));
    box.appendChild(hits);
  }
  for (const cat of list) {
    // Reuses the expert menu's own group/collapse styling (arrow, spacing, hover) —
    // same visual language, zero new CSS for the mechanic itself.
    const collapsedNow = searching ? shown > 1 : helpCollapsed.has(cat.title);
    const sec = document.createElement("div");
    sec.className = "expert-group help-group" + (collapsedNow ? " collapsed" : "");
    const head = document.createElement("div");
    head.className = "expert-group-title collapsible";
    const titleSpan = document.createElement("span");
    titleSpan.className = "help-group-title-text";
    if (searching && highlight) markTerms(titleSpan, cat.title, terms);
    else titleSpan.textContent = cat.title;
    head.appendChild(titleSpan);
    if (searching) {
      const badge = document.createElement("span");
      badge.className = "help-sec-badge";
      badge.textContent = helpSectionLabel(cat.section);
      head.appendChild(badge);
    }
    head.addEventListener("click", () => {
      if (helpCollapsed.has(cat.title)) helpCollapsed.delete(cat.title);
      else helpCollapsed.add(cat.title);
      sec.classList.toggle("collapsed");
    });
    sec.appendChild(head);
    const body = document.createElement("div");
    body.className = "help-group-body";
    // 22-A: four parts per entry — what it is, how to do it, what happens, and the
    // settings that tune it. howTo/whatHappens are optional so a short standalone
    // entry (e.g. the quick launcher) doesn't have to force a step list.
    const ov = document.createElement("p");
    ov.className = "help-overview";
    if (searching && highlight) markTerms(ov, cat.whatItIs, terms);
    else ov.textContent = cat.whatItIs;
    body.appendChild(ov);
    if (cat.howTo && cat.howTo.length) {
      const ol = document.createElement("ol");
      ol.className = "help-steps";
      for (const step of cat.howTo) {
        const li = document.createElement("li");
        if (searching && highlight) markTerms(li, step, terms);
        else li.textContent = step;
        ol.appendChild(li);
      }
      body.appendChild(ol);
    }
    if (cat.whatHappens) {
      const wh = document.createElement("p");
      wh.className = "help-happens";
      wh.textContent = cat.whatHappens;
      body.appendChild(wh);
    }
    if (cat.settings.length) {
      const ul = document.createElement("ul");
      ul.className = "help-settings";
      for (const s of cat.settings) {
        const li = document.createElement("li");
        li.textContent = s;
        ul.appendChild(li);
      }
      body.appendChild(ul);
    }
    if (cat.tip && flag("helpTips")) {
      const tip = document.createElement("p");
      tip.className = "help-tip";
      const ico = document.createElement("span");
      ico.className = "help-tip-ico";
      ico.setAttribute("aria-hidden", "true");
      ico.innerHTML = '<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M8 1a5 5 0 0 0-3 9v1.5a1 1 0 0 0 1 1h4a1 1 0 0 0 1-1V10a5 5 0 0 0-3-9zm-1.5 13h3a.5.5 0 0 1 0 1h-3a.5.5 0 0 1 0-1z"/></svg>';
      const txt = document.createElement("span");
      txt.textContent = cat.tip;
      tip.appendChild(ico);
      tip.appendChild(txt);
      body.appendChild(tip);
    }
    sec.appendChild(body);
    box.appendChild(sec);
  }
  if (searching && !shown) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("helpNoResults");
    box.appendChild(empty);
  }
}

function openHelp() {
  helpQuery = "";
  $("help-search").value = "";
  helpCollapsed = new Set();
  renderHelp();
  $("help").classList.remove("hidden");
}

async function openJournal() {
  settings = await invoke("get_settings");
  journalQuery = "";
  if ($("journal-q")) $("journal-q").value = "";
  journalTypes.clear();
  for (const type of typeFilterOf("journalTypeFilterPick")) journalTypes.add(type);
  paintTypeChips($("journal-types"), journalTypes);
  journalFav = false;
  $("journal-fav").classList.remove("active");
  setJournalColor("all");
  // Hide the whole filter bar when every filter is switched off (expert menu).
  const anyFilter = flag("journalSearch") || flag("journalTypeFilter") || flag("journalColorFilter") || (flag("journalTagFilter") && tagsOn()) || flag("favorites");
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

// Shows what an import would do (new vs. skipped-as-duplicate) before it
// touches anything. Resolves the picked-file path if the user confirms, or
// null if they canceled anywhere along the way (no file picked, no items,
// or the preview itself dismissed).
async function runImportPreview() {
  let preview;
  try {
    preview = await withDialog(() => invoke("preview_import"));
  } catch (err) {
    if (String(err) !== "canceled") toast(t("importFailed"));
    return null;
  }
  if (!preview) return null;
  const newCount = preview.items.filter((i) => !i.duplicate).length;
  const dupCount = preview.items.length - newCount;
  $("import-preview-summary").textContent = t("importPreviewSummary")
    .replace("{new}", newCount).replace("{dup}", dupCount);
  const list = $("import-preview-list");
  list.innerHTML = "";
  for (const item of preview.items) {
    const row = document.createElement("div");
    row.className = "import-preview-row" + (item.duplicate ? " dup" : "");
    const name = document.createElement("span");
    name.className = "import-preview-name";
    name.textContent = item.name;
    row.appendChild(name);
    if (item.duplicate) {
      const badge = document.createElement("span");
      badge.className = "import-preview-badge";
      badge.textContent = t("importPreviewDuplicate");
      row.appendChild(badge);
    }
    list.appendChild(row);
  }
  const modal = $("import-preview-modal");
  const confirmBtn = $("import-preview-confirm");
  confirmBtn.disabled = newCount === 0;
  modal.classList.remove("hidden");
  const confirmed = await new Promise((resolve) => {
    const done = (v) => {
      modal.classList.add("hidden");
      confirmBtn.removeEventListener("click", onOk);
      $("import-preview-cancel").removeEventListener("click", onCancel);
      $("import-preview-close").removeEventListener("click", onCancel);
      resolve(v);
    };
    const onOk = () => done(true);
    const onCancel = () => done(false);
    confirmBtn.addEventListener("click", onOk);
    $("import-preview-cancel").addEventListener("click", onCancel);
    $("import-preview-close").addEventListener("click", onCancel);
  });
  return confirmed ? { path: preview.path, hash: preview.hash } : null;
}
async function runImport() {
  const picked = await runImportPreview();
  if (!picked) return;
  try {
    // No withDialog here — import_prompts re-reads the already-picked file, it
    // never opens a native dialog itself (that guard is preview_import's job).
    // hash is preview_import's digest of that same file -- import_prompts
    // refuses if the file changed underneath the preview.
    const count = await withTaskbarBusy("import", () => invoke("import_prompts", { path: picked.path, hash: picked.hash }));
    if (count == null) return;
    await renderGrid(); // refreshes prompts AND settings
    // Imported preferences apply on the spot — no restart needed.
    LANG = pickLang(settings.language);
    applyI18n();
    fillSizeSelects();
    fillFontSelects();
    setLangSelect();
    fillThemeSelect();
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
    toast(t("imported").replace("{n}", count.toLocaleString(LANG)));
    maybeDupImportToast(); // Flag near-duplicates the import may have introduced
  } catch (err) {
    if (String(err) === "import-changed") toast(t("importChanged"));
    else if (String(err) !== "canceled") toast(t("importFailed"));
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
  // Same two checkboxes as restoreBackup, both preselected -- the dialog
  // itself refuses zero checked.
  const ok = await confirmDialog({
    title: t("backupImport"), message: t("importAllConfirm"), confirmLabel: t("backupImport"),
    checkboxes: [
      { key: "data", label: t("backupRestoreData"), checked: true },
      { key: "settings", label: t("backupRestoreSettings"), checked: true },
    ],
  });
  if (!ok) return;
  const dataOn = $("confirm-cb-data").checked;
  const settingsOn = $("confirm-cb-settings").checked;
  const scope = dataOn && settingsOn ? "all" : dataOn ? "data" : "settings";
  // The file stays picked while the password is retried, so a typo does not send
  // the user back through the file picker.
  let password = null;
  let asked = false;
  for (;;) {
    try {
      const res = await withTaskbarBusy("import", () => invoke("import_all", { path, password, scope }));
      await reloadEverything();
      let msg = t("importAllDone").replace("{f}", res.name).replace("{n}", Number(res.count).toLocaleString(LANG));
      // A hotkey the restore could not keep is named, never silently dropped.
      if (res.dropped_hotkeys) msg += "\n" + t("importHotkeysDropped").replace("{n}", Number(res.dropped_hotkeys).toLocaleString(LANG));
      // The file's own backup password is a foreign-machine DPAPI blob and was
      // discarded — say so, or a following export silently ships unprotected.
      if (res.backup_pw_dropped) msg += "\n" + t("importBackupPwKept");
      await infoDialog(t("backupImport"), msg);
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
// Returns whether it actually showed a dialog — the first-run tour must not
// open on top of (or right after burying) this more urgent prompt; it retries next
// launch instead, since tourSeen was never written this time.
async function offerTakeover() {
  const src = await invoke("takeover_offer").catch(() => null);
  if (!src) return false;
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
    errToast(e);
    return true;
  }
  if (adopt) await reloadEverything();
  return true;
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
  await maybeBackupBeforeWipe("delete");
  try {
    await invoke("delete_all_data");
  } catch (e) {
    // The in-memory store is already cleared either way (see reloadEverything
    // below) -- this means some ON-DISK table could not be, so the user must
    // still be told rather than believe the wipe fully succeeded.
    errToast(e);
  }
  await reloadEverything(); // in place: a page reload repaints the whole window
}

// Expert opt (Standard-an, split so a user who only wants one of the two can
// switch the other off). Take a safety backup before a destructive reset/delete.
async function maybeBackupBeforeWipe(kind) {
  if (!flag(kind === "reset" ? "backupBeforeReset" : "backupBeforeDelete")) return;
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
  await maybeBackupBeforeWipe("reset");
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
    // Escape ends a dictation session targeting this field, instead of
    // falling through to whatever the global Escape handler would otherwise do.
    if (e.key === "Escape" && voiceState !== "idle" && voiceTargetEl === inputEl) {
      e.stopPropagation();
      stopVoice();
    }
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
  modal.closeX.addEventListener("click", () => confirmDiscardIfDirty().then((ok) => { if (ok) closeModal(); }));
  modal.fav.addEventListener("click", () => {
    const on = modal.fav.classList.toggle("active");
    modal.fav.setAttribute("aria-pressed", String(on));
  });
  // Rotating prompts.
  modal.rotateSwitch.addEventListener("change", async () => {
    if (modal.rotateSwitch.checked) {
      modalState.rotateEntries = [modal.text.value];
      // Only a prompt that never rotated before takes the shipped/expert default —
      // re-checking the box mid-edit must not clobber a mode already chosen.
      if (!modalState.rotateMode) {
        modalState.rotateMode = EXPERT_DROPDOWNS.rotateDefaultMode.get() === "random" ? "random" : "";
      }
    } else {
      const hasExtra = (modalState.rotateEntries || []).length > 1;
      if (hasExtra) {
        const ok = await confirmDialog({
          title: t("rotateToggle"),
          message: t("rotateDiscardConfirm"),
          confirmLabel: t("discardConfirm"),
          danger: true,
        });
        if (!ok) { modal.rotateSwitch.checked = true; return; }
      }
      modal.text.value = (modalState.rotateEntries || [])[0] ?? modal.text.value;
      modalState.rotateEntries = [];
    }
    renderRotateUi();
    // The switch row only exists for a text prompt, so textVisible is true here.
    // Flipping it has to move the same three surfaces syncModalImageUi does — only
    // hiding the textarea left the clear-X wrapper (an empty 120px box) and, with
    // on, the rendered pane still showing entry 1 sitting above the colours.
    const rotating = rotationActive();
    modal.text.classList.toggle("hidden", rotating);
    modal.voiceRow.classList.toggle("hidden", rotating);
    const varsUi = !rotating && flag("promptVars");
    modal.textBar.classList.toggle("hidden", !varsUi);
    modal.varsHint.classList.toggle("hidden", !varsUi);
    if (varsUi) renderVarHelper();
    syncMdUi(true, rotating);
    if (!modal.rotateSwitch.checked) modal.text.focus();
    // This must hide/show the readout on the SAME tick as the toggle — the
    // rAF-deferred scheduleLenCounter() left it visible for one frame, long enough
    // for a synchronous check right after the toggle to see it.
    updateLenCounter();
  });
  modal.rotateAdd.addEventListener("click", () => {
    const max = Math.max(2, Math.round(val("rotateMaxEntries")) || 20);
    if ((modalState.rotateEntries || []).length >= max) return;
    modalState.rotateEntries.push("");
    renderRotateEntries();
    renderRotateAutoBtn();
    const rows = modal.rotateEntries.querySelectorAll(".rotate-entry-input");
    rows[rows.length - 1]?.focus();
  });
  modal.rotateMode.addEventListener("change", () => {
    if (!modalState) return;
    modalState.rotateMode = modal.rotateMode.value === "random" ? "random" : "";
  });
  modal.rotateAuto.addEventListener("click", () => {
    if (modal.rotateAuto.disabled) return;
    modalState.rotateAuto = !modalState.rotateAuto;
    renderRotateAutoBtn();
  });
  modal.rotateReset.addEventListener("click", async () => {
    if (!modalState?.id) return;
    try {
      await invoke("rotation_reset", { id: modalState.id });
    } catch (err) {
      errToast(err);
      return;
    }
    toast(t("rotateResetDone"));
    refreshRotateNext();
  });
  // Click the preview media → fullscreen zoom/pan viewer.
  modal.img.addEventListener("click", () => { if (modal.img.src) openLightbox(modal.img.src, false); });
  modal.video.addEventListener("click", () => { if (modal.video.src) openLightbox(modal.video.src, true); });
  modal.name.addEventListener("keydown", (e) => { if (e.key === "Enter") confirmModal(); });
  modal.text.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && voiceState !== "idle" && voiceTargetEl === modal.text) {
      e.stopPropagation();
      stopVoice();
    }
  });
  modal.text.addEventListener("input", scheduleLenCounter); // live length readout
  modal.text.addEventListener("input", scheduleMdPreview);  // live preview (rAF-throttled)
  // Replace layout: the rendered pane IS the resting state, so it has to hand the
  // textarea over on a click and take back over once the user is done typing.
  const mdSwap = (editing) => {
    const from = editing ? modal.mdPreview : modal.text;
    const span = from.scrollHeight - from.clientHeight;
    const ratio = span > 0 ? from.scrollTop / span : 0;
    mdEditing = editing;
    syncMdUi(true);
    const to = editing ? modal.text : modal.mdPreview;
    if (editing) to.focus(); // never behind a frame callback — see scheduleMdPreview
    // Only the scroll position waits for layout; losing it costs nothing.
    requestAnimationFrame(() => {
      const s = to.scrollHeight - to.clientHeight;
      to.scrollTop = s > 0 ? Math.round(ratio * s) : 0;
    });
  };
  modal.mdPreview.addEventListener("click", () => { if (flag("mdPreview")) mdSwap(true); });
  modal.mdPreview.addEventListener("focus", () => { if (flag("mdPreview")) mdSwap(true); });
  // Swapping back changes the height of the block, so doing it on the blur that a
  // mousedown produces moves whatever the user is halfway through clicking — the
  // click then lands somewhere else, or nowhere. Wait for the pointer to come up.
  let swapAfterClick = false;
  let pointerIsDown = false;
  window.addEventListener("pointerdown", () => { pointerIsDown = true; }, true);
  window.addEventListener("pointerup", () => {
    pointerIsDown = false;
    if (!swapAfterClick) return;
    swapAfterClick = false;
    if (mdEditing && document.activeElement !== modal.text) mdSwap(false);
  }, true);
  modal.text.addEventListener("blur", () => {
    if (!mdEditing) return;
    if (pointerIsDown) { swapAfterClick = true; return; }
    mdSwap(false);
  });
  // Variable helper collapsible (same convention as the notes row).
  modal.varsHead.addEventListener("click", () => {
    const open = !modal.varsBody.classList.toggle("hidden");
    modal.varsHead.setAttribute("aria-expanded", String(open));
    modal.textBar.classList.toggle("open", open);
  });
  // Capture: the field is read-only, the key press itself is the input.
  bindHotkeyField(modal.hotkeyInput, {
    id: () => modalState?.id || null,
    errEl: modal.hotkeyErr,
    showAllEl: $("modal-hotkey-showall"),
    current: () => modalState?.hotkey || "",
    onCommit: async (accel) => {
      const before = modalState?.id;
      try {
        await invoke("check_hotkey", { hotkey: accel, id: modalState?.id || null });
      } catch (err) {
        if (modalState?.id !== before) return null; // moved on — nothing to show for it
        throw err;
      }
      if (modalState?.id !== before) return null; // moved on between the check and here
      setPromptHotkey(accel);
      return accel;
    },
  });
  $("modal-hotkey-clear").addEventListener("click", () => setPromptHotkey(""));
  $("modal-hotkey-showall").addEventListener("click", () => openHotkeysModal());
  modal.hotkeyAction.addEventListener("change", () => {
    if (modalState) modalState.hotkeyPaste = modal.hotkeyAction.value;
  });
  // Tags/hotkey/history collapsible, combined into one "Advanced options".
  modal.advancedHead.addEventListener("click", () => {
    const open = !modal.advancedBody.classList.toggle("hidden");
    modal.advancedHead.setAttribute("aria-expanded", String(open));
    modal.advanced.classList.toggle("open", open);
    // In tall (text-edit) mode the textarea flex-grows and eats the height the open
    // history needs; history-open caps it (CSS) so the list + Save row stay reachable.
    // Only relevant when history itself applies to this prompt (mode/type/flag).
    const histApplies = !$("modal-history").classList.contains("hidden");
    modal.root.classList.toggle("history-open", open && histApplies);
    if (open) {
      if (histApplies) loadHistory(modalState?.id);
      // Scroll the modal's own scroll container to the bottom so the freshly opened
      // section (and the Save row below it) come fully into view on small windows.
      // scrollIntoView("nearest") only nudged the already-visible head, hence unreliable.
      const scroller = modal.advanced.closest(".modal");
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
    const session = modalSession;
    const path = await withDialog(() => invoke("pick_file_path"));
    if (!path || !modalState || session !== modalSession) return false;
    const kind = mediaKind(path);
    if (!kind) {
      toast(t("unsupportedFile"));
      return false;
    }
    if (kind === "image") {
      const data = await loadFilePreview(path);
      if (!data || !modalState || session !== modalSession) return false;
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
    const session = modalSession;
    const path = await withDialog(() => invoke("pick_file_path"));
    if (!path || !modalState || session !== modalSession) return;
    modalState.filePath = path;
    modalState.pdfPage = 0; // a different file starts back at its own first page
    const kind = mediaKind(path);
    const data = await loadFilePreview(path);
    if (!modalState || session !== modalSession) return;
    modalState.image = data;
    modalState.copyImage = false; // the file itself is what gets copied
    modalState.showImage = !!kind;
    syncModalImageUi(modalState.mode);
  });

  // Re-requests the preview at the chosen page and saves it into pdf_page —
  // the field only ever shows for a PDF attachment (syncModalImageUi).
  // Request token -- a fast second page change must not let the slower
  // older reply land after the newer one and overwrite its already-fresher preview.
  let pdfPageReq = 0;
  modal.pdfPage.addEventListener("change", async () => {
    if (!modalState) return;
    const session = modalSession;
    const page = Math.max(0, (Number(modal.pdfPage.value) || 1) - 1);
    modalState.pdfPage = page;
    const req = ++pdfPageReq;
    const data = await loadFilePreview(modalState.filePath, page);
    if (req !== pdfPageReq || !modalState || session !== modalSession) return;
    if (data) { modalState.image = data; modal.img.src = data; }
  });

  // Paperclip button in the composer bar (files, images, gifs, videos).
  $("file-btn").addEventListener("click", startFileCreate);
  $("file-btn").addEventListener("contextmenu", (e) => {
    e.preventDefault();
    startFileFromClipboard();
  });

  // Snipping tool: open the overlay; the backend notifies us with the crop.
  $("snip-btn").addEventListener("click", () => {
    invoke("open_snip").catch((err) => errToast(err));
  });

  // Pipette: the same frozen-desktop overlay, in colour-pick mode. The result
  // is a clipboard value only — no prompt is created and nothing enters the history.
  $("picker-btn").addEventListener("click", () => {
    invoke("open_picker").catch((err) => errToast(err));
  });

  // Screen recording: the same frozen-desktop overlay, in record-target
  // mode. The backend resolves the click/drag into a window, monitor or region
  // and notifies us with "recorder-target-picked"; a short countdown (0 = none)
  // then the actual capture starts.
  $("record-btn").addEventListener("click", () => {
    invoke("open_recorder").catch((err) => errToast(err));
  });
  // Driven by the recording state, not by the click -- set on a
  // successful start, cleared on stop/cancel/auto-stop, and re-synced by a
  // slow poll so a missed event can never leave the button glowing forever.
  function setRecordBtnState(state) {
    const btn = $("record-btn");
    btn.classList.toggle("is-recording", state === "recording");
    btn.classList.toggle("is-paused", state === "paused");
  }
  async function syncRecordBtnFromStatus() {
    const status = await invoke("recording_status").catch(() => null);
    setRecordBtnState(!status ? "idle" : status.paused ? "paused" : "recording");
  }
  setInterval(syncRecordBtnFromStatus, 5000);
  listen("recording-cancelled", () => setRecordBtnState("idle"));
  let recCountdownTimer = 0;
  function cancelRecCountdown() {
    clearTimeout(recCountdownTimer);
    recCountdownTimer = 0;
  }
  listen("recorder-target-picked", async (e) => {
    const p = e.payload || {};
    // A pick made from the running recording's own bar, switching its target
    // instead of starting a new one -- recbar.js's own listener handles that
    // one, this window must not also try to start a second recording from it.
    if (p.forSwitch) return;
    const kind = p.kind;
    let targetId = 0, regionX = 0, regionY = 0, regionW = 0, regionH = 0;
    if (kind === "window") targetId = p.id;
    else if (kind === "monitor") targetId = p.monitor;
    else if (kind === "region") {
      targetId = p.monitor;
      regionX = p.x; regionY = p.y; regionW = p.width; regionH = p.height;
    } else return;

    const startNow = async () => {
      try {
        await invoke("start_recording", { targetKind: kind, targetId, regionX, regionY, regionW, regionH });
        setRecordBtnState("recording");
        await invoke("show_rec_bar", { targetKind: kind, id: targetId });
      } catch (err) { errToast(err); }
    };
    // Checked before the countdown even starts -- discovering low disk space
    // only after a 3-2-1 count would just waste the user's time on a "no" answer.
    const diskWarn = await invoke("check_recording_disk_space").catch(() => null);
    if (diskWarn) {
      const minutes = Math.max(1, Math.round(diskWarn.est_seconds / 60));
      const proceed = await confirmDialog({
        title: t("recDiskWarnTitle"),
        message: t("recDiskWarnMsg")
          .replace("{drive}", diskWarn.drive)
          .replace("{free}", fmtBytes(diskWarn.free_mb * 1024 * 1024))
          .replace("{minutes}", String(minutes)),
        confirmLabel: t("recDiskWarnRecordAnyway"),
      });
      if (!proceed) return;
    }
    cancelRecCountdown();
    let n = Math.round(val("recCountdownS"));
    if (n <= 0) { await startNow(); return; }
    const tick = () => {
      toast(t("recCountdownToast").replace("{n}", n));
      n -= 1;
      if (n < 0) { recCountdownTimer = 0; startNow(); return; }
      recCountdownTimer = setTimeout(tick, 1000);
    };
    tick();
  });

  // ---- Recording result: mirrors the #snip-modal flow above, plus an
  // optional conversion step (recFormat/recQuality != the raw mp4 capture).
  // "recording-captured" is emitted by stop_recording (and by the auto-stop
  // path when the recorded window itself closes) -- the rec-bar is its own
  // window, so this is how the main window learns a file is ready.
  const recModal = $("rec-modal");
  const recPreview = $("rec-preview");
  const recStatus = $("rec-modal-status");
  const recFolderOut = $("rec-modal-folder");
  const recFolderChangeBtn = $("rec-modal-folder-change");
  let pendingRecPath = "";
  let recGen = 0; // invalidated on every close/discard so a late progress tick or a
                   // finished conversion from a superseded recording is ignored
  let recProgressOff = null;
  // Same "closed by default, raw text only under Technical details" treatment
  // as the video editor's own export error row, for a recording conversion
  // that fails instead of exporting.
  function showRecErrorDetails(raw) {
    const box = $("rec-modal-error-details");
    box.classList.toggle("hidden", !raw);
    box.open = false;
    $("rec-modal-error-raw").textContent = raw || "";
  }
  $("rec-modal-error-copy").addEventListener("click", () => {
    invoke("copy_text", { text: $("rec-modal-error-raw").textContent }).then(() => toast(t("copied"))).catch(() => {});
  });
  // 280 (Ben, 04.09.): closing the result window without an explicit
  // Save/Save as.../Attach deletes the recording -- Discard, the X button,
  // the backdrop and Escape all end up in closeRecModal(). Copy alone does
  // NOT count as "saving" (it only relocates the file so the clipboard
  // reference survives), so a copy-then-close still removes the working file
  // -- recCopiedNotSaved is what gates the confirm-before-delete for that case.
  let recSavedOrAttached = false;
  let recCopiedNotSaved = false;
  // Elide a long path in the middle rather than at the end -- the last
  // segment (the actual folder name) is what tells destinations apart, so it
  // has to stay visible; the title attribute always carries the full path.
  function elideMiddle(s, max = 60) {
    if (s.length <= max) return s;
    const head = Math.ceil((max - 1) / 2);
    return s.slice(0, head) + "…" + s.slice(s.length - (max - 1 - head));
  }
  function renderRecFolder() {
    const dir = EXPERT_PATHS.recordingDir.get();
    recFolderOut.textContent = elideMiddle(dir);
    recFolderOut.title = dir;
  }
  // Once saved/attached, nothing that reaches this function may delete
  // the file anymore -- "discard" used to force deletion regardless, which is
  // exactly how clicking Discard right after a successful Save deleted the
  // file the user had just saved. Both a real Discard click and an
  // implicit close (X, backdrop, Escape) before any Save/Attach/Edit already
  // meant "throw this away", so the two no longer need separate handling.
  async function closeRecModal() {
    const path = pendingRecPath;
    const shouldDelete = !recSavedOrAttached;
    // Copy alone does not count as saving, but it already moved the file to
    // where the clipboard reference points -- closing without confirming
    // first would delete that file out from under the still-live clipboard
    // reference (280's own rule). A toast after the fact is too late; ask now,
    // before anything else changes, so declining leaves the modal untouched.
    if (shouldDelete && path && recCopiedNotSaved) {
      const ok = await confirmDialog({
        title: t("recDiscard"),
        message: t("recClosedAfterCopy"),
        confirmLabel: t("recDiscard"),
        cancelLabel: t("cancel"),
      });
      if (!ok) return;
    }
    recGen += 1;
    invoke("cancel_recording_convert").catch(() => {});
    invoke("cancel_recording_crop").catch(() => {});
    if (recProgressOff) { recProgressOff(); recProgressOff = null; }
    recModal.classList.add("hidden");
    recPreview.classList.add("hidden");
    recPreview.removeAttribute("src");
    recStatus.classList.remove("rec-status-cancellable");
    recStatus.onclick = null;
    pendingRecPath = "";
    recSavedOrAttached = false;
    recCopiedNotSaved = false;
    if (shouldDelete && path) {
      // A number back means it actually moved to "verworfen" (see
      // discard_recording_file) -- null/failure stays silent, same as before.
      const keepH = await invoke("discard_recording_file", { path }).catch(() => null);
      if (keepH != null) toast(t("recDiscardedToast").replace("{hours}", String(Math.round(keepH))));
    }
  }
  // needsCrop: the window recording's canvas was bigger than the window
  // ever used (369's multi-display canvas) -- trim the black margin first,
  // with the same visible/cancellable status line as the format conversion
  // below, before either showing the raw preview or starting that conversion.
  // A cancel or any failure here just keeps the untouched, letterboxed
  // recording -- never something worth an error toast.
  async function runRecCrop(rawPath, gen) {
    let secs = 0;
    const showCropping = () => { recStatus.textContent = t("recCropping").replace("{s}", String(secs)); };
    showCropping();
    recStatus.classList.add("rec-status-cancellable");
    recStatus.onclick = () => { if (gen === recGen) invoke("cancel_recording_crop").catch(() => {}); };
    recProgressOff = await listen("recording-crop-progress", (e) => {
      if (gen !== recGen) return;
      secs = Math.floor((e.payload?.ms || 0) / 1000);
      showCropping();
    });
    const myOff = recProgressOff;
    if (gen === recGen) {
      await invoke("crop_recording_to_used_area", { path: rawPath }).catch(() => {});
    }
    myOff();
    if (recProgressOff === myOff) recProgressOff = null;
    if (gen !== recGen) return;
    recStatus.classList.remove("rec-status-cancellable");
    recStatus.onclick = null;
    recStatus.textContent = "";
  }
  async function openRecModal(rawPath, needsCrop) {
    pendingRecPath = rawPath;
    recSavedOrAttached = false;
    recCopiedNotSaved = false;
    recGen += 1;
    const gen = recGen;
    renderRecFolder();
    $("rec-modal-edit").classList.toggle("hidden", !flag("videoEditor"));
    recModal.classList.remove("hidden");
    if (needsCrop) {
      recPreview.classList.add("hidden");
      showRecErrorDetails("");
      await runRecCrop(rawPath, gen);
      if (gen !== recGen) return;
    }
    const format = settings.ui_texts?.recFormat || "mp4";
    const lossless = (settings.ui_texts?.recQuality || "high") === "lossless";
    if (format === "mp4" && !lossless) {
      recPreview.classList.remove("hidden");
      recPreview.src = convertFileSrc(rawPath);
      recStatus.textContent = "";
      return;
    }
    recPreview.classList.add("hidden");
    showRecErrorDetails("");
    let secs = 0;
    const showConverting = () => { recStatus.textContent = t("recConverting").replace("{s}", String(secs)); };
    showConverting();
    recStatus.classList.add("rec-status-cancellable");
    recStatus.onclick = () => { if (gen === recGen) invoke("cancel_recording_convert").catch(() => {}); };
    recProgressOff = await listen("recording-convert-progress", (e) => {
      if (gen !== recGen) return;
      secs = Math.floor((e.payload?.ms || 0) / 1000);
      showConverting();
    });
    const myOff = recProgressOff;
    if (gen !== recGen) {
      // The modal was closed/discarded/attached while this await was in
      // flight -- do not start a conversion nobody will see, and do not race
      // whatever that close already did with this recording's file (discard
      // deletes it by the same path convert_recording would then also touch).
      myOff();
      if (recProgressOff === myOff) recProgressOff = null;
      return;
    }
    try {
      const outPath = await invoke("convert_recording", { srcPath: rawPath, format, lossless });
      if (gen !== recGen) return;
      pendingRecPath = outPath;
      recStatus.classList.remove("rec-status-cancellable");
      recStatus.onclick = null;
      recStatus.textContent = "";
      recPreview.classList.remove("hidden");
      recPreview.src = convertFileSrc(outPath);
    } catch (err) {
      if (gen !== recGen) return;
      recStatus.classList.remove("rec-status-cancellable");
      recStatus.onclick = null;
      // Cancelled by the user's own click above, ffmpeg missing, or ffmpeg
      // genuinely failed on this input -- either way the untouched raw
      // recording is still there to fall back to, and only a real failure
      // gets the "Technical details" row (never a cancel).
      const raw = String(err);
      if (raw.includes("cancel")) {
        recStatus.textContent = t("recConvertCancelled");
      } else if (raw.includes("ffmpeg-missing")) {
        recStatus.textContent = `${t("recFfmpegMissingHint")} ${t("recConvertFailedKeepRaw")}`;
      } else {
        const { message, details } = parseFfmpegError(err, t);
        recStatus.textContent = `${message} ${t("recConvertFailedKeepRaw")}`;
        showRecErrorDetails(details);
      }
      recPreview.classList.remove("hidden");
      recPreview.src = convertFileSrc(rawPath);
    } finally {
      // Unlisten this call's own listener, never whatever the global variable
      // currently points at -- a superseded (older) call's finally must not
      // tear down a newer, still-active openRecModal call's own listener.
      myOff();
      if (recProgressOff === myOff) recProgressOff = null;
    }
  }
  // The periodic sweep and "once a recording ends" both fire this live
  // (they always run after this listener exists) -- only when it removed
  // something, per "a toast... when the total is worth mentioning".
  listen("recording-sweep-done", (e) => {
    const { count, bytes } = e.payload || {};
    if (count > 0) toast(t("recSweepDone").replace("{n}", String(count)).replace("{v}", fmtBytes(bytes)));
  });
  // The control bar's own target-switch button lives in a separate
  // window with no toast of its own -- a failed switch always leaves the
  // previous source still recording, this is purely the "here is why" note.
  listen("recording-target-switch-failed", (e) => errToast(e.payload));
  listen("recording-captured", async (e) => {
    setRecordBtnState("idle"); // covers both a normal stop and the auto-stop path
    const path = e.payload?.path || "";
    // An encoder write failure (disk full, etc.) already stopped the feeder
    // itself before this event fired -- whatever got captured up to that
    // point is still a valid, playable file, just cut short unexpectedly.
    // "low-disk-space" is the one case with a message of its own: the
    // disk-space watch stopped the recording proactively, before the
    // encoder itself would have hit that same wall.
    if (e.payload?.encode_error === "low-disk-space") toast(t("recDiskStoppedMsg"));
    else if (e.payload?.encode_error) toast(t("recEncodeError"));
    // A window recording whose canvas had to fall back below the
    // "every display" default (too large for the encoder, or the user
    // picked a smaller mode) says so once, here -- the normal "every
    // display" case stays silent since that is simply the default working.
    if (e.payload?.canvas_note === "monitor") toast(t("recCanvasFallbackMonitor"));
    else if (e.payload?.canvas_note === "window") toast(t("recCanvasFallbackWindow"));
    // The "window size" canvas default (today's Vorgabe) just cut off part
    // of the picture because the window grew mid-recording -- offer, once
    // per occurrence and only while the notice is on, to switch the canvas
    // to every display so a resize like this stops losing content. Runs
    // before the result menu itself opens, never instead of it.
    const cropMs = e.payload?.resize_crop_at_ms;
    if (cropMs != null && flag("recCropNotice")) {
      const secs = Math.floor(cropMs / 1000);
      const switchNow = await confirmDialog({
        title: t("recCropNoticeTitle"),
        message: t("recCropNoticeMsg").replace("{s}", String(secs)),
        confirmLabel: t("recCropNoticeSwitch"),
        cancelLabel: t("recCropNoticeOk"),
        danger: false,
        checkboxes: [{ key: "dontShowAgain", label: t("recCropNoticeDontShowAgain"), checked: false }],
        checkboxesOptional: true,
      });
      if ($("confirm-cb-dontShowAgain").checked) {
        settings.ui_flags = { ...(settings.ui_flags || {}), recCropNotice: false };
        invoke("set_ui_flag", { key: "recCropNotice", enabled: false }).catch(() => {});
      }
      if (switchNow) {
        settings.ui_texts = { ...(settings.ui_texts || {}), recCanvasMode: "all" };
        invoke("set_ui_text", { key: "recCanvasMode", value: "all" }).catch(() => {});
      }
    }
    // "Finish and continue" split this recording into several files -- the
    // result dialog below still only ever opens the LAST one (opening every
    // part in sequence in the editor is future work), so at least name how
    // many earlier parts exist and were kept, rather than the split going
    // unmentioned.
    const partsCount = e.payload?.parts?.length || 0;
    if (partsCount > 0) toast(t("recContinuePartsSaved").replace("{n}", String(partsCount)));
    if (path) openRecModal(path, !!e.payload?.needs_crop);
  });
  $("rec-modal-close").addEventListener("click", () => closeRecModal());
  recModal.addEventListener("pointerdown", (e) => {
    if (e.target === recModal) closeRecModal();
  });
  $("rec-modal-discard").addEventListener("click", () => closeRecModal());
  // Opening the editor hands the raw file to another window that keeps working
  // with it -- treat this like Save/Attach so closing this modal afterward never
  // deletes a file the editor might still be reading from.
  $("rec-modal-edit").addEventListener("click", async () => {
    if (!pendingRecPath) return;
    try {
      await invoke("open_video_editor", { srcPath: pendingRecPath });
      recSavedOrAttached = true;
    } catch (err) {
      errToast(err);
    }
  });
  recFolderChangeBtn.addEventListener("click", async () => {
    await EXPERT_PATHS.recordingDir.change();
    renderRecFolder();
  });
  // Update the preview + pending path to a command's returned new location --
  // Save/Save as.../Copy all move the file, so a later action in the same
  // still-open window (Attach, another Copy) must act on where it ended up,
  // not on the now-stale original <data>/recordings path.
  function adoptRecPath(newPath) {
    if (!newPath || newPath === pendingRecPath) return;
    pendingRecPath = newPath;
    if (!recPreview.classList.contains("hidden")) recPreview.src = convertFileSrc(newPath);
  }
  let recSaveBusy = false;
  $("rec-modal-save").addEventListener("click", async () => {
    if (!pendingRecPath || recSaveBusy) return;
    recSaveBusy = true;
    try {
      const newPath = await invoke("save_recording_now", { srcPath: pendingRecPath });
      adoptRecPath(newPath);
      recSavedOrAttached = true;
      toast(t("recSaved"));
    } catch (err) {
      errToast(err);
    } finally {
      recSaveBusy = false;
    }
  });
  let recSaveAsBusy = false;
  $("rec-modal-saveas").addEventListener("click", async () => {
    if (!pendingRecPath || recSaveAsBusy) return;
    recSaveAsBusy = true;
    try {
      const newPath = await invoke("save_recording_as", { srcPath: pendingRecPath }).catch((err) => { errToast(err); return null; });
      if (newPath) {
        adoptRecPath(newPath);
        recSavedOrAttached = true;
      }
    } finally {
      recSaveAsBusy = false;
    }
  });
  let recCopyBusy = false;
  $("rec-modal-copy").addEventListener("click", async () => {
    if (!pendingRecPath || recCopyBusy) return;
    recCopyBusy = true;
    try {
      // The file is already relocated by the time the backend even
      // attempts the clipboard write, so the new path is adopted regardless
      // of whether that write succeeded -- otherwise a clipboard failure left
      // this window pointing at a path that no longer exists.
      const [newPath, clipOk] = await invoke("copy_recording_to_clipboard", { srcPath: pendingRecPath });
      adoptRecPath(newPath);
      if (clipOk) {
        recCopiedNotSaved = !recSavedOrAttached;
        toast(t("copied"));
      } else {
        errToast("clipboard write failed");
      }
    } catch (err) {
      errToast(err);
    } finally {
      recCopyBusy = false;
    }
  });
  $("rec-modal-yes").addEventListener("click", () => {
    const path = pendingRecPath;
    // Reuses the exact same path a picked/dropped video file already takes --
    // media.js's video detection and the editor's video preview need nothing new.
    // Attaching counts as "saving": mark it before closeRecModal() so the
    // close path does not delete the file out from under the newly opened editor.
    recSavedOrAttached = true;
    closeRecModal();
    if (path) openFileCreate(path);
  });

  // Right-click reaches the picker's history without opening the overlay —
  // it lives in the backend (survives a restart), this popover just displays it.
  function renderPickHistoryPop(hex) {
    pickHistoryPop.innerHTML = "";
    if (!hex.length) {
      const empty = document.createElement("div");
      empty.className = "pick-history-empty";
      empty.textContent = t("pickHistoryEmpty");
      pickHistoryPop.appendChild(empty);
    } else {
      for (const h of hex) {
        const row = document.createElement("button");
        row.type = "button";
        row.className = "pick-history-row";
        const sw = document.createElement("span");
        sw.className = "pick-history-swatch";
        sw.style.background = h;
        const label = document.createElement("span");
        label.className = "pick-history-hex";
        label.textContent = h;
        row.append(sw, label);
        row.addEventListener("click", async () => {
          closePickHistoryPop();
          try {
            const text = await invoke("copy_pick_history_entry", { hex: h });
            toast(t("colorCopied"), null, text);
          } catch (err) { errToast(err); }
        });
        pickHistoryPop.appendChild(row);
      }
    }
  }
  $("picker-btn").addEventListener("contextmenu", async (e) => {
    e.preventDefault();
    let hex = [];
    try { hex = (await invoke("pick_history")) || []; } catch { /* history stays empty */ }
    renderPickHistoryPop(hex);
    placePopover(pickHistoryPop, $("picker-btn").getBoundingClientRect());
  });
  listen("color-picked", async (e) => {
    const value = e.payload?.text || "";
    if (!value) return;
    // The picked notation is already on the clipboard (unchanged behaviour);
    // these three chips let a click re-copy any of the OTHER notations too.
    const chips = ["hex", "rgb", "hsl"]
      .map((k) => e.payload?.[k])
      .filter(Boolean)
      .map((v) => ({
        label: v,
        // The notation already on the clipboard is marked instead of being repeated
        // in the message — that duplication was what pushed the row onto a second line.
        current: v === value,
        onClick: () => invoke("copy_text", { text: v }).catch((err) => errToast(err)),
      }));
    toast(t("colorCopied"), null, e.payload?.hex || "", chips);
    // Live-update the recent-colours popover if the user left it open across a
    // pick, instead of it staying frozen at whatever it showed when it opened.
    if (!pickHistoryPop.classList.contains("hidden")) {
      try { renderPickHistoryPop((await invoke("pick_history")) || []); } catch { /* leave it as-is on a read hiccup */ }
    }
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
    if (e.target?.closest?.("input, textarea, [contenteditable]")) return;
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
  // ==== 117/118: screenshot editor ====
  // Opens right after a capture, before #snip-modal. Pure frontend: it receives a
  // data URL and hands back an edited one; everything downstream of #snip-modal is
  // untouched. Every knob below reads live from the expert menu.
  const SNIP_EDIT_PEN_PX = () => val("snipPenPx");
  const SNIP_EDIT_MARKER_PX = () => val("snipMarkerPx");
  const SNIP_EDIT_TEXT_PX = () => val("snipTextPx");
  const SNIP_EDIT_PIXELATE_PX = () => val("snipPixelBlock");
  const SNIP_EDIT_SPOTLIGHT_DIM = () => val("snipSpotlightDim") / 100;
  const SNIP_EDIT_UNDO_MAX = () => val("snipEditUndoMax");
  const SNIP_SCALE_FILTER = () => settings.ui_texts?.snipScaleFilter || "high";
  // Refuse a target this large outright rather than risk an out-of-memory
  // canvas allocation -- comfortably above any real screen resolution.
  const SNIP_SCALE_MAX_AREA = 80_000_000;
  const SNIP_EDIT_SNAP = () => flag("snipEditSnap");
  const SNIP_EDIT_TEXT_PLATE = () => flag("snipTextPlate");
  const SNIP_EDIT_DEFAULT_TOOL = () => txt("snipEditDefaultTool") || "hand";
  const SNIP_EDIT_SNAP_TOL = () => val("snipEditSnapTolerancePx") || 6; // px (screen)
  // Function, not a static object: pen/marker's middle preset tracks the live
  // expert-menu default, so a changed snipPenPx/snipMarkerPx shows up next open.
  const SNIP_WIDTH_PRESETS_OF = (tool) => ({
    pen: [2, SNIP_EDIT_PEN_PX(), 14],
    marker: [10, SNIP_EDIT_MARKER_PX(), 28],
    line: [2, 4, 8],
    arrow: [2, 4, 8],
    rect: [2, 4, 8],
    ellipse: [2, 4, 8],
    step: [4, 8, 14],
  }[tool] || [2, 4, 8]);
  const SNIP_TOOL_SVG = {
    crop: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M6 2v14a2 2 0 0 0 2 2h14M2 6h14a2 2 0 0 1 2 2v14"/></svg>',
    pen: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M4 20l1-4L15.5 5.5l3 3L8 19l-4 1Z"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M13.5 7l3 3"/></svg>',
    marker: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" stroke-linecap="round" d="M8 14l7-7 3 3-7 7-4 1 1-4Z"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M4 21h7"/></svg>',
    line: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><circle cx="5" cy="19" r="1.6" fill="currentColor"/><circle cx="19" cy="5" r="1.6" fill="currentColor"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M6.3 17.7 17.7 6.3"/></svg>',
    arrow: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M5 19 18 6M11 6h7v7"/></svg>',
    rect: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><rect x="4" y="6" width="16" height="12" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.8"/></svg>',
    ellipse: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><ellipse cx="12" cy="12" rx="8" ry="6" fill="none" stroke="currentColor" stroke-width="1.8"/></svg>',
    text: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M5 5.5h14M12 5.5v13"/></svg>',
    step: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.8"/><text x="12" y="15.5" font-size="9" text-anchor="middle" fill="currentColor" font-family="sans-serif">1</text></svg>',
    pixelate: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><g fill="currentColor"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7" opacity="0.5"/><rect x="3" y="14" width="7" height="7" opacity="0.5"/><rect x="14" y="14" width="7" height="7"/></g></svg>',
    spotlight: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><circle cx="12" cy="12" r="5" fill="none" stroke="currentColor" stroke-width="1.8"/><path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" opacity="0.6" d="M12 1.5v3M12 19.5v3M1.5 12h3M19.5 12h3"/></svg>',
    // The only way to reach panning used to be an undiscoverable middle-click
    // or holding Space -- a real toolbar entry for the exact same gesture.
    hand: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M8 13V6a1.5 1.5 0 0 1 3 0v5.5M11 11.2V4.5a1.5 1.5 0 0 1 3 0V11M14 11.3V6a1.5 1.5 0 0 1 3 0v7M17 12.5V9a1.5 1.5 0 0 1 3 0v6c0 3.6-2.2 6.5-6 6.5h-1c-2.7 0-4-.9-5.4-2.8L4.8 15c-.6-.9-.3-1.9.5-2.3.7-.4 1.5-.2 2.1.5L8 14"/></svg>',
  };
  const SNIP_EYEDROP_SVG = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M15 5l4 4-9.5 9.5H5v-4.5L14.5 5Z"/><path fill="none" stroke="currentColor" stroke-width="1.8" d="M13 7l4 4"/></svg>';
  const SNIP_TOOLS = [
    { id: "crop", key: "1", label: "snipToolCrop" },
    { id: "pen", key: "2", label: "snipToolPen" },
    { id: "marker", key: "3", label: "snipToolMarker" },
    { id: "line", key: "4", label: "snipToolLine" },
    { id: "arrow", key: "5", label: "snipToolArrow" },
    { id: "rect", key: "6", label: "snipToolRect" },
    { id: "ellipse", key: "7", label: "snipToolEllipse" },
    { id: "text", key: "8", label: "snipToolText" },
    { id: "step", key: "9", label: "snipToolStep" },
    { id: "pixelate", key: "0", label: "snipToolPixelate" },
    { id: "spotlight", key: "-", label: "snipToolSpotlight" },
    { id: "hand", key: "h", label: "snipToolHand" },
  ];

  const snipEdit = $("snip-edit");
  const snipEditToolsEl = $("snip-edit-tools");
  const snipEditContextEl = $("snip-edit-context");
  const snipEditWrap = $("snip-edit-canvas-wrap");
  const snipEditStage = $("snip-edit-stage");
  const snipEditBaseCv = $("snip-edit-base");
  const snipEditLiveCv = $("snip-edit-live");
  const snipEditTextInput = $("snip-edit-text-input");
  const snipEditMeasureEl = $("snip-edit-measure");
  const snipEditBaseCtx = snipEditBaseCv.getContext("2d");
  const snipEditLiveCtx = snipEditLiveCv.getContext("2d");

  // Drawing model: SE.ops is the ordered operation list ("base image, then every
  // operation in sequence"). SE.rawCanvas holds ONLY the untouched capture (native
  // size) so pixelate always samples the base, never already-drawn operations.
  const SE = {
    active: false, img: null, rawCanvas: null, w: 0, h: 0,
    ops: [], redo: [], floor: 0, perTool: {}, tool: SNIP_EDIT_DEFAULT_TOOL(),
    zoom: 1, drag: null, panState: null, spaceDown: false, showRaw: false,
    cropRect: null, lastNudged: null, eyedropArm: null, textEdit: null,
    onApply: null, onCancel: null,
  };

  function snipEditClamp(v, lo, hi) { return Math.min(hi, Math.max(lo, v)); }
  function snipEditDist(a, b) { return Math.hypot(b.x - a.x, b.y - a.y); }
  function snipEditNormRect(r) {
    if (r && "a" in r) {
      const x = Math.min(r.a.x, r.b.x), y = Math.min(r.a.y, r.b.y);
      return { x, y, w: Math.abs(r.b.x - r.a.x), h: Math.abs(r.b.y - r.a.y) };
    }
    return r;
  }
  // The one place image-pixel coordinates are computed. Canvas backing stores are
  // always the image's native size (never scaled by devicePixelRatio), and the CSS
  // display size is scaled independently by zoom -- so DPR cancels out here by
  // construction; clientX/clientY and getBoundingClientRect are both CSS pixels.
  function snipEditToImage(ev) {
    const r = snipEditBaseCv.getBoundingClientRect();
    return {
      x: snipEditClamp((ev.clientX - r.left) / SE.zoom, 0, SE.w),
      y: snipEditClamp((ev.clientY - r.top) / SE.zoom, 0, SE.h),
    };
  }
  function snipEditSnapPoint(pt) {
    if (!SNIP_EDIT_SNAP()) return pt;
    const tol = SNIP_EDIT_SNAP_TOL() / SE.zoom;
    let { x, y } = pt;
    for (const tx of [0, SE.w / 2, SE.w]) if (Math.abs(x - tx) <= tol) { x = tx; break; }
    for (const ty of [0, SE.h / 2, SE.h]) if (Math.abs(y - ty) <= tol) { y = ty; break; }
    return { x, y };
  }

  function snipEditToolState(tool) {
    if (!SE.perTool[tool]) {
      const widths = SNIP_WIDTH_PRESETS_OF(tool);
      SE.perTool[tool] = {
        color: COLORS[1] || "#ef4444",
        width: widths[1],
        filled: false,
        fontPx: SNIP_EDIT_TEXT_PX(),
        plate: SNIP_EDIT_TEXT_PLATE(),
        blockPx: SNIP_EDIT_PIXELATE_PX(),
        dim: SNIP_EDIT_SPOTLIGHT_DIM(),
      };
    }
    return SE.perTool[tool];
  }
  function snipEditContrastColor(hex) {
    const c = String(hex || "").replace("#", "");
    if (c.length !== 6) return "#fff";
    const r = parseInt(c.slice(0, 2), 16), g = parseInt(c.slice(2, 4), 16), b = parseInt(c.slice(4, 6), 16);
    return (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.6 ? "#111" : "#fff";
  }
  function snipEditGetPixelColor(pt) {
    const x = snipEditClamp(Math.round(pt.x), 0, SE.w - 1);
    const y = snipEditClamp(Math.round(pt.y), 0, SE.h - 1);
    const d = snipEditBaseCtx.getImageData(x, y, 1, 1).data;
    return "#" + [d[0], d[1], d[2]].map((v) => v.toString(16).padStart(2, "0")).join("");
  }

  // ---- per-tool drawing (shared by the finished layer and the live preview) ----
  function snipEditStrokeSmoothPath(ctx, points) {
    if (points.length < 2) return;
    ctx.beginPath();
    ctx.moveTo(points[0].x, points[0].y);
    for (let i = 1; i < points.length - 1; i++) {
      const mx = (points[i].x + points[i + 1].x) / 2;
      const my = (points[i].y + points[i + 1].y) / 2;
      ctx.quadraticCurveTo(points[i].x, points[i].y, mx, my);
    }
    const last = points[points.length - 1];
    ctx.lineTo(last.x, last.y);
    ctx.stroke();
  }
  function snipEditDrawPenOp(ctx, op) {
    ctx.save();
    ctx.strokeStyle = op.color;
    ctx.lineWidth = op.width;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    snipEditStrokeSmoothPath(ctx, op.points);
    ctx.restore();
  }
  // Collects the whole stroke on its own transparent layer first (so self-overlap
  // within one stroke never darkens further), then composites it once with alpha.
  // Reused across calls instead of a fresh full-resolution canvas per marker
  // stroke -- snipEditRedrawBase calls this once per marker op on EVERY redraw
  // (every new operation, undo/redo, nudge and tool switch), so cost used to
  // scale with marker count times image resolution.
  let snipEditMarkerTmp = null;
  function snipEditDrawMarkerOp(ctx, op) {
    const tmp = snipEditMarkerTmp || (snipEditMarkerTmp = document.createElement("canvas"));
    if (tmp.width !== SE.w) tmp.width = SE.w;
    if (tmp.height !== SE.h) tmp.height = SE.h;
    const tctx = tmp.getContext("2d");
    tctx.clearRect(0, 0, tmp.width, tmp.height);
    tctx.strokeStyle = op.color;
    tctx.lineWidth = op.width;
    tctx.lineCap = "round";
    tctx.lineJoin = "round";
    snipEditStrokeSmoothPath(tctx, op.points);
    ctx.save();
    ctx.globalAlpha = 0.5;
    ctx.globalCompositeOperation = "multiply";
    ctx.drawImage(tmp, 0, 0);
    ctx.restore();
  }
  function snipEditDrawLineOp(ctx, op) {
    ctx.save();
    ctx.strokeStyle = op.color;
    ctx.lineWidth = op.width;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(op.points[0].x, op.points[0].y);
    ctx.lineTo(op.points[1].x, op.points[1].y);
    ctx.stroke();
    ctx.restore();
  }
  // Used to draw the full shaft (round-capped) all the way to the tip and
  // then lay a short, fairly wide triangle on top -- the round cap regularly
  // poked out from under/beside that triangle, looking like a buggy double
  // point. Now the shaft stops exactly at the head's own back edge (so the
  // cap's bulge lands under the head's fill, never past its sides), and the
  // head itself is a longer, more slender point instead of a stubby wedge.
  function snipEditDrawArrowOp(ctx, op) {
    const [a, b] = op.points;
    const dist = Math.hypot(b.x - a.x, b.y - a.y);
    const angle = dist > 0 ? Math.atan2(b.y - a.y, b.x - a.x) : 0;
    // Capped at the actual drag distance -- otherwise a drag shorter than one
    // arrowhead put the shaft's "back" point PAST the start point, flipping
    // the shaft to point backwards during the first few pixels of every drag.
    const headLen = Math.min(7 + op.width * 3.2, dist); // tip-to-back distance along the shaft
    const headHalfAngle = Math.PI / 9;  // ~20 deg either side -- slender, not dumpy
    const backX = b.x - headLen * Math.cos(angle);
    const backY = b.y - headLen * Math.sin(angle);
    ctx.save();
    ctx.strokeStyle = op.color;
    ctx.lineWidth = op.width;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(backX, backY);
    ctx.stroke();
    ctx.restore();
    const headSide = headLen / Math.cos(headHalfAngle); // tip-to-back-corner distance
    ctx.save();
    ctx.fillStyle = op.color;
    ctx.beginPath();
    ctx.moveTo(b.x, b.y);
    ctx.lineTo(b.x - headSide * Math.cos(angle - headHalfAngle), b.y - headSide * Math.sin(angle - headHalfAngle));
    ctx.lineTo(b.x - headSide * Math.cos(angle + headHalfAngle), b.y - headSide * Math.sin(angle + headHalfAngle));
    ctx.closePath();
    ctx.fill();
    ctx.restore();
  }
  function snipEditDrawRectOp(ctx, op) {
    const r = op.rect;
    ctx.save();
    ctx.strokeStyle = op.color;
    ctx.fillStyle = op.color;
    ctx.lineWidth = op.width;
    if (op.filled) ctx.fillRect(r.x, r.y, r.w, r.h);
    else ctx.strokeRect(r.x + op.width / 2, r.y + op.width / 2, Math.max(0, r.w - op.width), Math.max(0, r.h - op.width));
    ctx.restore();
  }
  function snipEditDrawEllipseOp(ctx, op) {
    const r = op.rect;
    ctx.save();
    ctx.strokeStyle = op.color;
    ctx.fillStyle = op.color;
    ctx.lineWidth = op.width;
    ctx.beginPath();
    ctx.ellipse(r.x + r.w / 2, r.y + r.h / 2, Math.max(0, r.w / 2 - op.width / 2), Math.max(0, r.h / 2 - op.width / 2), 0, 0, Math.PI * 2);
    if (op.filled) ctx.fill(); else ctx.stroke();
    ctx.restore();
  }
  function snipEditDrawTextOp(ctx, op) {
    ctx.save();
    ctx.font = `${op.fontPx}px sans-serif`;
    ctx.textBaseline = "top";
    const w = ctx.measureText(op.text).width;
    const h = op.fontPx * 1.25;
    if (op.plate) {
      ctx.fillStyle = "rgba(0,0,0,0.45)";
      ctx.fillRect(op.point.x - 4, op.point.y - 2, w + 8, h + 4);
    }
    ctx.shadowColor = "rgba(0,0,0,0.6)";
    ctx.shadowBlur = 3;
    ctx.shadowOffsetX = 1;
    ctx.shadowOffsetY = 1;
    ctx.fillStyle = op.color;
    ctx.fillText(op.text, op.point.x, op.point.y);
    ctx.restore();
  }
  function snipEditMeasureTextWidth(op) {
    snipEditBaseCtx.save();
    snipEditBaseCtx.font = `${op.fontPx}px sans-serif`;
    const w = snipEditBaseCtx.measureText(op.text).width;
    snipEditBaseCtx.restore();
    return w;
  }
  function snipEditDrawStepOp(ctx, op, n) {
    const r = 10 + op.width;
    ctx.save();
    ctx.fillStyle = op.color;
    ctx.beginPath();
    ctx.arc(op.point.x, op.point.y, r, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = snipEditContrastColor(op.color);
    ctx.font = `${Math.round(r * 1.1)}px sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(String(n), op.point.x, op.point.y + 1);
    ctx.restore();
  }
  // Always samples SE.rawCanvas (the untouched capture) -- never already-drawn
  // operations -- so overlapping pixelation never compounds on itself.
  function snipEditDrawPixelate(ctx, rect, blockPx) {
    const { x, y, w, h } = snipEditNormRect(rect);
    if (w < 2 || h < 2) return;
    const bw = Math.max(1, Math.round(w / Math.max(2, blockPx)));
    const bh = Math.max(1, Math.round(h / Math.max(2, blockPx)));
    const tmp = document.createElement("canvas");
    tmp.width = bw;
    tmp.height = bh;
    const tctx = tmp.getContext("2d");
    tctx.imageSmoothingEnabled = true;
    tctx.drawImage(SE.rawCanvas, x, y, w, h, 0, 0, bw, bh);
    ctx.save();
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(tmp, 0, 0, bw, bh, x, y, w, h);
    ctx.restore();
  }
  function snipEditDrawSpotlight(ctx, rect, dim) {
    const { x, y, w, h } = snipEditNormRect(rect);
    ctx.save();
    ctx.fillStyle = `rgba(0,0,0,${dim})`;
    ctx.fillRect(0, 0, SE.w, y);
    ctx.fillRect(0, y + h, SE.w, SE.h - (y + h));
    ctx.fillRect(0, y, x, h);
    ctx.fillRect(x + w, y, SE.w - (x + w), h);
    ctx.restore();
  }
  function snipEditDrawCropDim(ctx, r) {
    ctx.save();
    ctx.fillStyle = "rgba(0,0,0,0.55)";
    ctx.fillRect(0, 0, SE.w, r.y);
    ctx.fillRect(0, r.y + r.h, SE.w, SE.h - (r.y + r.h));
    ctx.fillRect(0, r.y, r.x, r.h);
    ctx.fillRect(r.x + r.w, r.y, SE.w - (r.x + r.w), r.h);
    ctx.restore();
  }
  function snipEditDrawOp(ctx, op, stepN) {
    switch (op.tool) {
      case "pen": snipEditDrawPenOp(ctx, op); break;
      case "marker": snipEditDrawMarkerOp(ctx, op); break;
      case "line": snipEditDrawLineOp(ctx, op); break;
      case "arrow": snipEditDrawArrowOp(ctx, op); break;
      case "rect": snipEditDrawRectOp(ctx, op); break;
      case "ellipse": snipEditDrawEllipseOp(ctx, op); break;
      case "text": snipEditDrawTextOp(ctx, op); break;
      case "pixelate": snipEditDrawPixelate(ctx, op.rect, op.blockPx); break;
      case "spotlight": snipEditDrawSpotlight(ctx, op.rect, op.dim); break;
      case "step": snipEditDrawStepOp(ctx, op, stepN); break;
    }
  }

  // Full recomposite: base image, then every finished operation in sequence. Only
  // called on discrete events (undo/redo/crop-change/zoom-change/finalize) -- never
  // per mousemove, so a 4K capture never redraws its whole bitmap on a drag.
  function snipEditRedrawBase() {
    const ctx = snipEditBaseCtx;
    ctx.clearRect(0, 0, SE.w, SE.h);
    ctx.drawImage(SE.img, 0, 0, SE.w, SE.h);
    if (SE.showRaw) return; // "V" held: show the unedited base only
    let stepN = 0;
    for (const op of SE.ops) {
      if (op.tool === "crop") continue; // drawn as an overlay pass below
      if (op.tool === "scale") continue; // already baked into SE.img, see snipEditApplyScale
      if (op.tool === "step") stepN++;
      snipEditDrawOp(ctx, op, stepN);
    }
    const crop = SE.ops.find((o) => o.tool === "crop");
    if (crop && SE.tool !== "crop") snipEditDrawCropDim(ctx, crop.rect); // crop tool shows its own live overlay instead
  }
  function snipEditClearLive() { snipEditLiveCtx.clearRect(0, 0, SE.w, SE.h); }

  function snipEditUpdateUndoRedoButtons() {
    $("snip-edit-undo").disabled = SE.ops.length <= SE.floor;
    $("snip-edit-redo").disabled = !SE.redo.length;
  }
  function snipEditPushOp(op) {
    SE.ops.push(op);
    // SE.ops IS the render list -- never drop a rendered op. Once the undo
    // window is exceeded, advance the floor instead of shifting the oldest op out;
    // it stays drawn (snipEditRedrawBase still walks every op) but becomes
    // permanent, undo just can no longer walk back past it.
    if (SE.ops.length - SE.floor > SNIP_EDIT_UNDO_MAX()) SE.floor = SE.ops.length - SNIP_EDIT_UNDO_MAX();
    SE.redo.length = 0;
    SE.lastNudged = op;
    snipEditRedrawBase();
    snipEditUpdateUndoRedoButtons();
  }
  // Spotlight/crop: exactly one may exist -- a new one replaces the old rather than
  // stacking two dimming layers.
  function snipEditPushUniqueOp(tool, op) {
    const before = SE.ops.length;
    SE.ops = SE.ops.filter((o) => o.tool !== tool);
    // floor is a count-based boundary into SE.ops; removing an entry shifts every
    // later index down by one, so shrink the floor by the same amount to keep
    // pointing at the same logical (never re-lock an already-undoable op).
    SE.floor = Math.max(0, SE.floor - (before - SE.ops.length));
    snipEditPushOp(op);
  }
  // Halving-step resample: a single drawImage straight down to a much smaller
  // canvas blurs/aliases badly (the classic "shrunk screenshot text turns to
  // mush" problem), so downscaling by more than 2x repeatedly halves first and
  // only does the final, non-halved step at the end. Upscaling and the "low"
  // (nearest-neighbour, for pixel art) filter always run in one step.
  function snipEditScaleCanvas(src, targetW, targetH) {
    const filter = SNIP_SCALE_FILTER();
    let cur = src, curW = src.width, curH = src.height;
    if (filter === "high") {
      while (curW > targetW * 2 && curH > targetH * 2) {
        const nextW = Math.max(targetW, Math.round(curW / 2));
        const nextH = Math.max(targetH, Math.round(curH / 2));
        const step = document.createElement("canvas");
        step.width = nextW;
        step.height = nextH;
        const sctx = step.getContext("2d");
        sctx.imageSmoothingEnabled = true;
        sctx.imageSmoothingQuality = "high";
        sctx.drawImage(cur, 0, 0, curW, curH, 0, 0, nextW, nextH);
        cur = step;
        curW = nextW;
        curH = nextH;
      }
    }
    const out = document.createElement("canvas");
    out.width = targetW;
    out.height = targetH;
    const octx = out.getContext("2d");
    octx.imageSmoothingEnabled = filter !== "low";
    if (filter !== "low") octx.imageSmoothingQuality = "high";
    octx.drawImage(cur, 0, 0, curW, curH, 0, 0, targetW, targetH);
    return out;
  }
  // Scaling is an operation like any other tool, not a destructive edit: it
  // pushes a "scale" op so undo/redo work the normal way. Every other op stores
  // its coordinates in image pixels, so instead of trying to rescale each one
  // individually this bakes the base image and every op drawn before it into a
  // new canvas at the new size (a flatten step) -- see the "scale" skip in
  // snipEditRedrawBase and the special-cased undo/redo just below.
  function snipEditApplyScale(targetW, targetH) {
    targetW = Math.max(1, Math.round(targetW));
    targetH = Math.max(1, Math.round(targetH));
    if (targetW * targetH > SNIP_SCALE_MAX_AREA) { toast(t("snipScaleTooLarge")); return false; }
    if (SE.tool === "crop") snipEditCropCommit(); // same guard snipEditExportDataUrl uses for a drawn-but-uncommitted crop
    snipEditFinishTextIfOpen();
    const prevW = SE.w, prevH = SE.h, prevImg = SE.img, prevRawCanvas = SE.rawCanvas, prevOps = SE.ops.slice();
    const sx = targetW / SE.w, sy = targetH / SE.h;
    snipEditRedrawBase(); // make sure snipEditBaseCv reflects every op (but crop) before it gets baked in
    const baked = snipEditScaleCanvas(snipEditBaseCv, targetW, targetH);
    const rawBaked = snipEditScaleCanvas(SE.rawCanvas, targetW, targetH);
    // Crop is never rasterized (it only ever dims an overlay preview, see
    // snipEditRedrawBase) -- its rect is the one thing stored in image-pixel
    // coordinates that survives the bake, so it is rescaled by hand instead of
    // being baked away, or it would point at the wrong region afterward.
    const keptOps = SE.ops
      .filter((o) => o.tool === "crop")
      .map((o) => ({ ...o, rect: { x: o.rect.x * sx, y: o.rect.y * sy, w: o.rect.w * sx, h: o.rect.h * sy } }));
    SE.floor = Math.max(0, SE.floor - (prevOps.length - keptOps.length));
    const op = {
      tool: "scale", w: targetW, h: targetH, img: baked, rawCanvas: rawBaked, keptOps,
      prevW, prevH, prevImg, prevRawCanvas, prevOps,
    };
    SE.img = baked;
    SE.rawCanvas = rawBaked;
    SE.w = targetW;
    SE.h = targetH;
    SE.ops = keptOps;
    snipEditBaseCv.width = targetW;
    snipEditBaseCv.height = targetH;
    snipEditLiveCv.width = targetW;
    snipEditLiveCv.height = targetH;
    snipEditPushOp(op);
    snipEditFit();
    return true;
  }
  // Scale bakes the whole composite into a new SE.img/SE.rawCanvas at a new
  // SE.w/SE.h (see snipEditApplyScale) instead of being replayed like every
  // other op, so undo/redo can't just pop/push it -- the op itself carries the
  // before/after canvases and sizes needed to swap the working image back and
  // forth, same idea as SE.rawCanvas already keeping the untouched capture.
  function snipEditRestoreScaleState(w, h, img, rawCanvas, ops) {
    SE.w = w;
    SE.h = h;
    SE.img = img;
    SE.rawCanvas = rawCanvas;
    SE.ops = ops.slice();
    snipEditBaseCv.width = w;
    snipEditBaseCv.height = h;
    snipEditLiveCv.width = w;
    snipEditLiveCv.height = h;
    snipEditFit();
  }
  function snipEditUndo() {
    if (SE.ops.length <= SE.floor) return;
    const op = SE.ops.pop();
    SE.redo.push(op);
    if (op.tool === "scale") snipEditRestoreScaleState(op.prevW, op.prevH, op.prevImg, op.prevRawCanvas, op.prevOps);
    SE.lastNudged = null;
    snipEditRedrawBase();
    snipEditUpdateUndoRedoButtons();
  }
  function snipEditRedo() {
    if (!SE.redo.length) return;
    const op = SE.redo.pop();
    SE.ops.push(op);
    if (op.tool === "scale") snipEditRestoreScaleState(op.w, op.h, op.img, op.rawCanvas, [...op.keptOps, op]);
    SE.lastNudged = null;
    snipEditRedrawBase();
    snipEditUpdateUndoRedoButtons();
  }
  function snipEditNudge(op, key, amount) {
    const dx = key === "ArrowLeft" ? -amount : key === "ArrowRight" ? amount : 0;
    const dy = key === "ArrowUp" ? -amount : key === "ArrowDown" ? amount : 0;
    if (op.points) for (const p of op.points) { p.x += dx; p.y += dy; }
    if (op.point) { op.point.x += dx; op.point.y += dy; }
    if (op.rect) { op.rect.x += dx; op.rect.y += dy; }
    snipEditRedrawBase();
    // The crop tool's own live overlay (handles + dim) is drawn from SE.cropRect,
    // not from the committed op -- without this, nudging a just-committed crop
    // moves what gets exported while the on-screen handles stay put, so Apply/
    // Copy would silently produce an image cropped from a region never shown.
    if (op.tool === "crop" && op.rect) {
      SE.cropRect = { ...op.rect };
      snipEditDrawCropOverlayLive();
    }
  }

  // ---- crop: a rect operation, only applied to pixels at export time ----
  function snipEditCropHandlePoints(r) {
    return {
      nw: { x: r.x, y: r.y }, n: { x: r.x + r.w / 2, y: r.y }, ne: { x: r.x + r.w, y: r.y },
      w: { x: r.x, y: r.y + r.h / 2 }, e: { x: r.x + r.w, y: r.y + r.h / 2 },
      sw: { x: r.x, y: r.y + r.h }, s: { x: r.x + r.w / 2, y: r.y + r.h }, se: { x: r.x + r.w, y: r.y + r.h },
    };
  }
  function snipEditHitCropHandle(pt, r) {
    const tol = 10 / SE.zoom;
    for (const [name, p] of Object.entries(snipEditCropHandlePoints(r))) {
      if (Math.abs(pt.x - p.x) <= tol && Math.abs(pt.y - p.y) <= tol) return name;
    }
    return null;
  }
  // Same handle-to-cursor mapping the video editor already uses for its own
  // crop/overlay handles (video.css .ve-h-*): a directional resize cursor at
  // each corner/edge, a move cursor inside the rect, crosshair everywhere
  // else -- signalling that a click there starts a brand new crop instead of
  // dragging the existing one.
  const SNIP_CROP_HANDLE_CURSORS = {
    nw: "nwse-resize", n: "ns-resize", ne: "nesw-resize",
    w: "ew-resize", e: "ew-resize",
    sw: "nesw-resize", s: "ns-resize", se: "nwse-resize",
  };
  function snipEditCropCursorFor(pt) {
    const r = SE.cropRect;
    if (!r) return "crosshair";
    const handle = snipEditHitCropHandle(pt, r);
    if (handle) return SNIP_CROP_HANDLE_CURSORS[handle];
    if (pt.x >= r.x && pt.x <= r.x + r.w && pt.y >= r.y && pt.y <= r.y + r.h) return "move";
    return "crosshair";
  }
  function snipEditDrawCropOverlayLive() {
    snipEditClearLive();
    const r = SE.cropRect;
    if (!r) return;
    snipEditDrawCropDim(snipEditLiveCtx, r);
    const hs = 8;
    snipEditLiveCtx.save();
    snipEditLiveCtx.fillStyle = "#fff";
    snipEditLiveCtx.strokeStyle = "#2563eb";
    snipEditLiveCtx.lineWidth = 1.5;
    for (const p of Object.values(snipEditCropHandlePoints(r))) {
      snipEditLiveCtx.fillRect(p.x - hs / 2, p.y - hs / 2, hs, hs);
      snipEditLiveCtx.strokeRect(p.x - hs / 2, p.y - hs / 2, hs, hs);
    }
    snipEditLiveCtx.restore();
  }
  function snipEditCropPointerDown(pt) {
    const r = SE.cropRect;
    if (r) {
      const handle = snipEditHitCropHandle(pt, r);
      if (handle) { SE.drag = { kind: "crop-resize", handle, orig: { ...r } }; return; }
      if (pt.x >= r.x && pt.x <= r.x + r.w && pt.y >= r.y && pt.y <= r.y + r.h) {
        SE.drag = { kind: "crop-move", start: pt, orig: { ...r } };
        return;
      }
    }
    SE.drag = { kind: "crop-new", start: pt };
    SE.cropRect = { x: pt.x, y: pt.y, w: 0, h: 0 };
  }
  function snipEditResizeCropRect(orig, handle, pt, shift) {
    let x = orig.x, y = orig.y, x2 = orig.x + orig.w, y2 = orig.y + orig.h;
    if (handle.includes("n")) y = pt.y;
    if (handle.includes("s")) y2 = pt.y;
    if (handle.includes("w")) x = pt.x;
    if (handle.includes("e")) x2 = pt.x;
    const nr = { x: Math.min(x, x2), y: Math.min(y, y2), w: Math.abs(x2 - x), h: Math.abs(y2 - y) };
    if (shift && orig.w && orig.h) {
      const ratio = orig.w / orig.h;
      if (handle === "n" || handle === "s") nr.w = nr.h * ratio; else nr.h = nr.w / ratio;
    }
    return nr;
  }
  function snipEditCropPointerMove(pt, shift) {
    const d = SE.drag;
    if (d.kind === "crop-new") {
      let r = { x: Math.min(d.start.x, pt.x), y: Math.min(d.start.y, pt.y), w: Math.abs(pt.x - d.start.x), h: Math.abs(pt.y - d.start.y) };
      if (shift) { const s = Math.max(r.w, r.h); r = { ...r, w: s, h: s }; }
      SE.cropRect = r;
    } else if (d.kind === "crop-move") {
      const dx = pt.x - d.start.x, dy = pt.y - d.start.y;
      SE.cropRect = {
        x: snipEditClamp(d.orig.x + dx, 0, SE.w - d.orig.w),
        y: snipEditClamp(d.orig.y + dy, 0, SE.h - d.orig.h),
        w: d.orig.w, h: d.orig.h,
      };
    } else if (d.kind === "crop-resize") {
      SE.cropRect = snipEditResizeCropRect(d.orig, d.handle, pt, shift);
    }
    snipEditDrawCropOverlayLive();
  }
  function snipEditCropPointerUp() {
    if (SE.cropRect && (SE.cropRect.w < 8 || SE.cropRect.h < 8)) {
      const committed = SE.ops.find((o) => o.tool === "crop");
      SE.cropRect = committed ? { ...committed.rect } : null; // "smaller than 8x8 -> ignore"
    }
    snipEditDrawCropOverlayLive();
  }
  function snipEditCropCommit() {
    if (!SE.cropRect || SE.cropRect.w < 8 || SE.cropRect.h < 8) return;
    snipEditPushUniqueOp("crop", { tool: "crop", rect: { ...SE.cropRect } });
  }
  // Esc while the crop tool is active discards only the crop, not the whole editor.
  function snipEditCropDiscard() {
    SE.drag = null;
    SE.cropRect = null;
    const before = SE.ops.length;
    SE.ops = SE.ops.filter((o) => o.tool !== "crop");
    // Same floor adjustment as snipEditPushUniqueOp -- removing an entry shifts
    // every later index down by one; without this the undo/redo boundary and
    // button state can drift out of sync with the actual op list.
    SE.floor = Math.max(0, SE.floor - (before - SE.ops.length));
    snipEditUpdateUndoRedoButtons();
    snipEditClearLive();
    snipEditRedrawBase();
  }

  // ---- text: a real <input> positioned over the canvas, in stage-local (i.e.
  // already-zoomed) pixels, since the input shares the stage's containing block. ----
  function snipEditHitTestText(pt) {
    for (let i = SE.ops.length - 1; i >= 0; i--) {
      const op = SE.ops[i];
      if (op.tool !== "text") continue;
      const w = snipEditMeasureTextWidth(op);
      const h = op.fontPx * 1.3;
      if (pt.x >= op.point.x && pt.x <= op.point.x + w && pt.y >= op.point.y - 2 && pt.y <= op.point.y + h) return op;
    }
    return null;
  }
  function snipEditOpenTextInput(pt) {
    const ts = snipEditToolState("text");
    SE.textEdit = { point: pt };
    snipEditTextInput.value = "";
    snipEditTextInput.style.left = `${pt.x * SE.zoom}px`;
    snipEditTextInput.style.top = `${pt.y * SE.zoom}px`;
    snipEditTextInput.style.fontSize = `${ts.fontPx * SE.zoom}px`;
    snipEditTextInput.style.color = ts.color;
    snipEditTextInput.classList.remove("hidden");
    snipEditTextInput.focus();
  }
  function snipEditCommitTextInput() {
    if (!SE.textEdit) return;
    const { point } = SE.textEdit;
    SE.textEdit = null;
    const val = snipEditTextInput.value;
    snipEditTextInput.classList.add("hidden");
    if (!val.trim()) return; // empty input -> discard the operation
    const ts = snipEditToolState("text");
    snipEditPushOp({ tool: "text", point, text: val, color: ts.color, fontPx: ts.fontPx, plate: ts.plate });
  }
  function snipEditCancelTextInput() {
    SE.textEdit = null;
    snipEditTextInput.classList.add("hidden");
  }
  function snipEditFinishTextIfOpen() { if (SE.textEdit) snipEditCommitTextInput(); }
  function snipEditTextPointerDown(pt) {
    snipEditFinishTextIfOpen();
    const hit = snipEditHitTestText(pt);
    if (hit) { SE.drag = { kind: "text-move", op: hit, dx: pt.x - hit.point.x, dy: pt.y - hit.point.y }; return; }
    snipEditOpenTextInput(pt);
  }

  // ---- contextual (tool-specific) toolbar: only what the active tool understands ----
  function snipEditBuildWidthField(tool, ts) {
    // A plain div, not a <label>: the S/M/L buttons below sit before the
    // exact-value field, so a <label> here would forward a click on the
    // "Width" text to the first of THEM, not to the number field -- the
    // field gets its own aria-label instead (see the .setAttribute below).
    const wrap = document.createElement("div");
    wrap.className = "snip-edit-field";
    wrap.append(Object.assign(document.createElement("span"), { textContent: t("snipEditWidth") }));
    const row = document.createElement("div");
    row.className = "snip-edit-width-row";
    const presets = SNIP_WIDTH_PRESETS_OF(tool);
    // Three visibly different stroke weights read at a glance -- the S/M/L
    // letters said nothing about WHICH was thicker without reading the tooltip.
    const STROKE_ICON_W = [2.5, 5, 8];
    ["snipEditWidthS", "snipEditWidthM", "snipEditWidthL"].forEach((lab, i) => {
      const px = presets[i];
      const b = document.createElement("button");
      b.type = "button";
      b.className = "snip-edit-width-btn" + (ts.width === px ? " sel" : "");
      b.innerHTML = `<svg viewBox="0 0 20 12" width="20" height="12" aria-hidden="true"><line x1="2" y1="6" x2="18" y2="6" stroke="currentColor" stroke-width="${STROKE_ICON_W[i]}" stroke-linecap="round"/></svg>`;
      b.title = `${t(lab)} (${px}px)`;
      b.setAttribute("aria-label", `${t(lab)} (${px}px)`);
      b.addEventListener("click", () => { ts.width = px; snipEditRenderContext(); });
      row.appendChild(b);
    });
    wrap.appendChild(row);
    const control = numberField({
      value: ts.width, min: 1, max: 80, step: 1, presets: [1, 2, 4, 8, 16],
      onChange: (v) => { ts.width = v; snipEditRenderContext(); },
      decreaseLabel: t("numFieldDecrease"), increaseLabel: t("numFieldIncrease"), presetsLabel: t("numFieldPresets"),
      decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease", presetsAriaKey: "numFieldPresets",
    });
    const num = control.querySelector("input");
    num.title = t("snipEditWidthTip");
    num.setAttribute("aria-label", t("snipEditWidthTip"));
    wrap.appendChild(control);
    return wrap;
  }
  // Single trigger button (current color) opens a popover holding the full
  // palette + eyedropper, so the toolbar stays one row tall regardless of
  // palette size (a full inline grid there used to force it multi-row).
  const snipColorPop = document.createElement("div");
  snipColorPop.className = "snip-color-pop hidden";
  document.body.appendChild(snipColorPop);
  function closeSnipColorPop() {
    snipColorPop.classList.add("hidden");
  }
  function snipColorPopIsOpen() {
    return !snipColorPop.classList.contains("hidden");
  }
  function openSnipColorPop(anchor, tool, ts) {
    snipColorPop.innerHTML = "";
    const sw = document.createElement("div");
    sw.className = "swatches";
    buildSwatches(sw, ts.color,
      (hex) => { ts.color = hex; snipEditRenderContext(); closeSnipColorPop(); },
      (anchor2, cur) => openColorPop(anchor2, cur, (hex) => { ts.color = hex; snipEditRenderContext(); }));
    snipColorPop.appendChild(sw);
    snipColorPop.classList.remove("hidden");
    placePopover(snipColorPop, anchor.getBoundingClientRect(), 6);
  }
  document.addEventListener("pointerdown", (e) => {
    if (!snipColorPopIsOpen()) return;
    if (snipColorPop.contains(e.target)) return;
    if (!colorPop.classList.contains("hidden") && colorPop.contains(e.target)) return;
    if (e.target.closest(".snip-edit-color-trigger")) return;
    closeSnipColorPop();
  });
  function snipEditBuildColorField(tool, ts) {
    const wrap = document.createElement("div");
    wrap.className = "snip-edit-field";
    const trigger = document.createElement("button");
    trigger.type = "button";
    trigger.className = "snip-edit-color-trigger";
    trigger.title = t("snipEditColorTip");
    trigger.setAttribute("aria-label", t("snipEditColor"));
    const chip = document.createElement("span");
    chip.className = "snip-edit-color-chip" + (ts.color ? "" : " none");
    if (ts.color) chip.style.background = ts.color;
    trigger.appendChild(chip);
    trigger.addEventListener("click", () => openSnipColorPop(trigger, tool, ts));
    wrap.appendChild(trigger);
    const eye = document.createElement("button");
    eye.type = "button";
    eye.className = "icon-btn snip-edit-eyedrop" + (SE.eyedropArm === tool ? " active" : "");
    eye.title = t("snipEditEyedropTip");
    eye.setAttribute("aria-label", t("snipEditEyedrop"));
    eye.innerHTML = SNIP_EYEDROP_SVG;
    eye.addEventListener("click", () => { SE.eyedropArm = tool; snipEditRenderContext(); });
    wrap.appendChild(eye);
    return wrap;
  }
  function snipEditBuildToggle(labelText, checked, onChange) {
    const wrap = document.createElement("label");
    wrap.className = "snip-edit-toggle";
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = checked;
    cb.addEventListener("change", () => onChange(cb.checked));
    wrap.append(cb, Object.assign(document.createElement("span"), { textContent: labelText }));
    return wrap;
  }
  function snipEditBuildNumField(labelText, value, min, max, title, onChange, presets) {
    // A real <label> (not a plain div) so the input keeps an accessible name
    // from its own visible text -- see the numFieldDec/-Inc/-List buttons
    // below for why they need an explicit aria-label instead.
    const wrap = document.createElement("label");
    wrap.className = "snip-edit-field";
    wrap.append(Object.assign(document.createElement("span"), { textContent: labelText }));
    const control = numberField({
      value, min, max, step: 1, presets, onChange,
      decreaseLabel: t("numFieldDecrease"), increaseLabel: t("numFieldIncrease"), presetsLabel: t("numFieldPresets"),
      decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease", presetsAriaKey: "numFieldPresets",
    });
    control.querySelector("input").title = title;
    wrap.appendChild(control);
    return wrap;
  }
  function snipEditBuildDimField(ts) {
    const wrap = document.createElement("div");
    wrap.className = "snip-edit-field";
    wrap.append(Object.assign(document.createElement("span"), { textContent: t("snipEditDim") }));
    const range = document.createElement("input");
    range.type = "range";
    range.min = "10";
    range.max = "90";
    range.step = "5";
    range.value = String(Math.round(ts.dim * 100));
    range.title = t("snipEditDimTip");
    range.addEventListener("input", () => { ts.dim = Number(range.value) / 100; });
    wrap.appendChild(range);
    return wrap;
  }
  function snipEditRenderContext() {
    const box = snipEditContextEl;
    box.innerHTML = "";
    const tool = SE.tool;
    if (tool === "crop") return; // nothing this tool understands -> nothing shown
    const ts = snipEditToolState(tool);
    const hasWidth = ["pen", "marker", "line", "arrow", "rect", "ellipse", "step"].includes(tool);
    if (hasWidth) box.appendChild(snipEditBuildWidthField(tool, ts));
    if (hasWidth || tool === "text") box.appendChild(snipEditBuildColorField(tool, ts));
    if (tool === "rect" || tool === "ellipse") {
      box.appendChild(snipEditBuildToggle(t("snipEditFilled"), !!ts.filled, (v) => { ts.filled = v; }));
    }
    if (tool === "text") {
      box.appendChild(snipEditBuildNumField(t("snipEditTextSize"), ts.fontPx, 10, 96, t("snipEditTextSizeTip"), (v) => { ts.fontPx = v; }, [12, 16, 24, 32, 48, 64, 96]));
      box.appendChild(snipEditBuildToggle(t("snipEditTextPlate"), !!ts.plate, (v) => { ts.plate = v; }));
    }
    if (tool === "pixelate") {
      box.appendChild(snipEditBuildNumField(t("snipEditBlockSize"), ts.blockPx, 4, 64, t("snipEditBlockSizeTip"), (v) => { ts.blockPx = v; }, [8, 12, 16, 24, 32]));
    }
    if (tool === "spotlight") box.appendChild(snipEditBuildDimField(ts));
  }

  // ---- toolbar (left): one tool active at a time ----
  function snipEditBuildToolButtons() {
    snipEditToolsEl.innerHTML = "";
    for (const tdef of SNIP_TOOLS) {
      // Dividers: between the drawing tools (pen..step) and the image tools
      // (pixelate/spotlight) -- same grouping the Snipping Tool uses -- and
      // again before the view tool (hand), which does not draw anything.
      if (tdef.id === "pixelate" || tdef.id === "hand") {
        const sep = document.createElement("span");
        sep.className = "snip-edit-sep";
        sep.setAttribute("aria-hidden", "true");
        snipEditToolsEl.appendChild(sep);
      }
      const b = document.createElement("button");
      b.type = "button";
      b.className = "icon-btn";
      b.dataset.tool = tdef.id;
      b.title = `${t(tdef.label)} (${tdef.key})`;
      b.setAttribute("aria-label", t(tdef.label));
      b.setAttribute("aria-pressed", "false");
      b.innerHTML = SNIP_TOOL_SVG[tdef.id];
      b.addEventListener("click", () => snipEditSetTool(tdef.id));
      snipEditToolsEl.appendChild(b);
    }
  }
  function snipEditRenderToolButtons() {
    for (const b of snipEditToolsEl.children) {
      const on = b.dataset.tool === SE.tool;
      b.classList.toggle("active", on);
      b.setAttribute("aria-pressed", String(on));
    }
  }
  function snipEditSetTool(tool) {
    if (SE.drag) return;
    snipEditFinishTextIfOpen();
    SE.eyedropArm = null;
    closeSnipColorPop();
    SE.tool = tool;
    SE.lastNudged = null;
    snipEditClearLive();
    // Clear any crop-hover cursor left over from the previous tool -- an
    // inline style would otherwise outrank the panning class below and the
    // per-tool crosshair default.
    snipEditWrap.style.cursor = "";
    // A grab cursor as soon as the Hand tool is picked, not only once a
    // drag actually starts -- the same affordance Space-hold already gets.
    snipEditWrap.classList.toggle("panning", tool === "hand");
    if (tool === "crop") {
      const existing = SE.ops.find((o) => o.tool === "crop");
      SE.cropRect = existing ? { ...existing.rect } : null;
      snipEditDrawCropOverlayLive();
    }
    snipEditRenderToolButtons();
    snipEditRenderContext();
    snipEditRedrawBase(); // toggles the crop-dim pass on/off for the new tool
  }

  // ---- live previews for the currently-dragging tool (top canvas only) ----
  function snipEditDrawLivePenStroke() {
    snipEditClearLive();
    const ts = snipEditToolState(SE.tool);
    const op = { color: ts.color, width: ts.width, points: SE.drag.points };
    if (SE.tool === "marker") snipEditDrawMarkerOp(snipEditLiveCtx, op); else snipEditDrawPenOp(snipEditLiveCtx, op);
  }
  function snipEditDrawLiveLineArrow() {
    snipEditClearLive();
    const ts = snipEditToolState(SE.tool);
    const op = { color: ts.color, width: ts.width, points: [SE.drag.a, SE.drag.b] };
    if (SE.tool === "arrow") snipEditDrawArrowOp(snipEditLiveCtx, op); else snipEditDrawLineOp(snipEditLiveCtx, op);
  }
  function snipEditDrawLiveRectEllipse() {
    snipEditClearLive();
    const ts = snipEditToolState(SE.tool);
    const op = { color: ts.color, width: ts.width, rect: snipEditNormRect(SE.drag), filled: !!ts.filled };
    if (SE.tool === "ellipse") snipEditDrawEllipseOp(snipEditLiveCtx, op); else snipEditDrawRectOp(snipEditLiveCtx, op);
  }
  function snipEditDrawLivePixelatePreview() {
    snipEditClearLive();
    const ts = snipEditToolState("pixelate");
    snipEditDrawPixelate(snipEditLiveCtx, snipEditNormRect(SE.drag), ts.blockPx);
  }
  function snipEditDrawLiveSpotlightPreview() {
    snipEditClearLive();
    const ts = snipEditToolState("spotlight");
    snipEditDrawSpotlight(snipEditLiveCtx, snipEditNormRect(SE.drag), ts.dim);
  }

  function snipEditShowMeasure(e, rect) {
    const r = snipEditNormRect(rect);
    snipEditMeasureEl.textContent = `${Math.round(r.w)} × ${Math.round(r.h)}`;
    const wr = snipEditWrap.getBoundingClientRect();
    snipEditMeasureEl.style.left = `${e.clientX - wr.left + snipEditWrap.scrollLeft + 14}px`;
    snipEditMeasureEl.style.top = `${e.clientY - wr.top + snipEditWrap.scrollTop + 14}px`;
    snipEditMeasureEl.classList.remove("hidden");
  }
  function snipEditHideMeasure() { snipEditMeasureEl.classList.add("hidden"); }

  function snipEditApplyShiftConstraint(drag, pt) {
    if (drag.kind === "line" || drag.kind === "arrow") {
      const dx = pt.x - drag.a.x, dy = pt.y - drag.a.y;
      const dist = Math.hypot(dx, dy);
      const angle = Math.round(Math.atan2(dy, dx) / (Math.PI / 12)) * (Math.PI / 12); // 15deg steps
      return { x: drag.a.x + Math.cos(angle) * dist, y: drag.a.y + Math.sin(angle) * dist };
    }
    if (drag.kind === "rect" || drag.kind === "ellipse") {
      const dx = pt.x - drag.a.x, dy = pt.y - drag.a.y;
      const s = Math.max(Math.abs(dx), Math.abs(dy));
      return { x: drag.a.x + Math.sign(dx || 1) * s, y: drag.a.y + Math.sign(dy || 1) * s };
    }
    return pt;
  }

  // ---- pan (space+drag or middle mouse) ----
  function snipEditStartPan(e) {
    SE.panState = { x: e.clientX, y: e.clientY, sl: snipEditWrap.scrollLeft, st: snipEditWrap.scrollTop };
    // Without capture, dragging past the canvas edge (zoomed in beyond the
    // viewport) stops delivering pointermove once the cursor leaves it.
    snipEditLiveCv.setPointerCapture(e.pointerId);
    // A leftover crop-tool inline cursor otherwise outranks the grab/grabbing
    // cursor below for this whole drag (only the Space-key path cleared it).
    snipEditWrap.style.cursor = "";
    snipEditWrap.classList.add("panning");
    e.preventDefault();
  }
  function snipEditDoPan(e) {
    if (!SE.panState) return;
    snipEditWrap.scrollLeft = SE.panState.sl - (e.clientX - SE.panState.x);
    snipEditWrap.scrollTop = SE.panState.st - (e.clientY - SE.panState.y);
  }
  function snipEditEndPan() {
    SE.panState = null;
    if (!SE.spaceDown && SE.tool !== "hand") snipEditWrap.classList.remove("panning");
  }

  // ---- zoom: pure CSS scaling of the native-resolution canvas -- no redraw needed ----
  function snipEditSetZoom(z) {
    SE.zoom = snipEditClamp(z, 0.05, 8);
    snipEditStage.style.width = `${SE.w * SE.zoom}px`;
    snipEditStage.style.height = `${SE.h * SE.zoom}px`;
    $("snip-edit-zoom-val").textContent = `${Math.round(SE.zoom * 100)}%`;
  }
  function snipEditZoomBy(factor) { snipEditSetZoom(SE.zoom * factor); }
  function snipEditFit() {
    const availW = snipEditWrap.clientWidth - 24;
    const availH = snipEditWrap.clientHeight - 24;
    snipEditSetZoom(Math.min(1, availW / SE.w, availH / SE.h));
  }

  // ---- pointer wiring ----
  function snipEditOnPointerDown(e) {
    if (!SE.active) return;
    if (e.button === 1 || SE.spaceDown || (e.button === 0 && SE.tool === "hand")) { snipEditStartPan(e); return; }
    if (e.button !== 0) return;
    // The canvas itself is never a focus target, so the browser's default
    // mousedown action for it is to blur whatever is currently focused --
    // including the text-tool input this same handler is about to focus below,
    // since that default action runs AFTER this handler returns. Without this,
    // opening the text field looked like it immediately lost focus again.
    e.preventDefault();
    snipEditFinishTextIfOpen();
    const pt0 = snipEditToImage(e);
    if (SE.eyedropArm) {
      snipEditToolState(SE.eyedropArm).color = snipEditGetPixelColor(pt0);
      SE.eyedropArm = null;
      snipEditRenderContext();
      return;
    }
    snipEditLiveCv.setPointerCapture(e.pointerId);
    SE.lastNudged = null;
    const pt = e.altKey ? pt0 : snipEditSnapPoint(pt0);
    switch (SE.tool) {
      case "crop": snipEditCropPointerDown(pt); break;
      case "pen": case "marker": SE.drag = { kind: SE.tool, points: [pt] }; break;
      case "line": case "arrow": SE.drag = { kind: SE.tool, a: pt, b: pt }; break;
      case "rect": case "ellipse": case "pixelate": case "spotlight": SE.drag = { kind: SE.tool, a: pt, b: pt }; break;
      case "text": snipEditTextPointerDown(pt); break;
      case "step": snipEditPushOp({ tool: "step", point: pt, color: snipEditToolState("step").color, width: snipEditToolState("step").width }); break;
    }
  }
  function snipEditOnPointerMove(e) {
    if (!SE.active) return;
    if (SE.panState) { snipEditDoPan(e); return; }
    if (!SE.drag) {
      if (SE.tool === "crop") {
        // Space held means a pan is about to start (or the panning CSS class
        // is already showing its grab cursor) -- an inline crop cursor would
        // otherwise outrank that class and never let go/grabbing show.
        if (!SE.spaceDown) snipEditWrap.style.cursor = snipEditCropCursorFor(snipEditToImage(e));
        // The overlay is already on screen and unchanged by a plain hover
        // (its handles do not react to hover position) -- redrawing it here
        // was a full-image clear+refill on every mousemove for nothing.
      }
      return;
    }
    const pt0 = snipEditToImage(e);
    let pt = e.altKey ? pt0 : snipEditSnapPoint(pt0);
    if (e.shiftKey) pt = snipEditApplyShiftConstraint(SE.drag, pt);
    switch (SE.drag.kind) {
      case "pen": case "marker": SE.drag.points.push(pt); snipEditDrawLivePenStroke(); break;
      case "line": case "arrow": SE.drag.b = pt; snipEditDrawLiveLineArrow(); break;
      case "rect": case "ellipse": SE.drag.b = pt; snipEditDrawLiveRectEllipse(); snipEditShowMeasure(e, SE.drag); break;
      case "pixelate": SE.drag.b = pt; snipEditDrawLivePixelatePreview(); break;
      case "spotlight": SE.drag.b = pt; snipEditDrawLiveSpotlightPreview(); break;
      case "crop-new": case "crop-move": case "crop-resize":
        snipEditCropPointerMove(pt, e.shiftKey);
        snipEditShowMeasure(e, SE.cropRect);
        break;
      case "text-move": {
        const d = SE.drag;
        const newPoint = { x: pt.x - d.dx, y: pt.y - d.dy };
        snipEditClearLive();
        snipEditDrawTextOp(snipEditLiveCtx, { ...d.op, point: newPoint });
        d.newPoint = newPoint;
        break;
      }
    }
  }
  function snipEditOnPointerUp() {
    if (!SE.active) return;
    if (SE.panState) { snipEditEndPan(); return; }
    if (!SE.drag) return;
    const d = SE.drag;
    SE.drag = null;
    snipEditHideMeasure();
    snipEditClearLive();
    const ts = snipEditToolState(SE.tool);
    switch (d.kind) {
      case "pen": if (d.points.length >= 2) snipEditPushOp({ tool: "pen", points: d.points, color: ts.color, width: ts.width }); break;
      case "marker": if (d.points.length >= 2) snipEditPushOp({ tool: "marker", points: d.points, color: ts.color, width: ts.width }); break;
      case "line": case "arrow":
        if (snipEditDist(d.a, d.b) >= 2) snipEditPushOp({ tool: d.kind, points: [d.a, d.b], color: ts.color, width: ts.width });
        break;
      case "rect": case "ellipse": {
        const r = snipEditNormRect(d);
        if (r.w >= 4 && r.h >= 4) snipEditPushOp({ tool: d.kind, rect: r, color: ts.color, width: ts.width, filled: !!ts.filled });
        break;
      }
      case "pixelate": {
        const r = snipEditNormRect(d);
        if (r.w >= 4 && r.h >= 4) snipEditPushOp({ tool: "pixelate", rect: r, blockPx: ts.blockPx });
        break;
      }
      case "spotlight": {
        const r = snipEditNormRect(d);
        if (r.w >= 4 && r.h >= 4) snipEditPushUniqueOp("spotlight", { tool: "spotlight", rect: r, dim: ts.dim });
        break;
      }
      case "crop-new": case "crop-move": case "crop-resize": snipEditCropPointerUp(); break;
      case "text-move": if (d.newPoint) { d.op.point = d.newPoint; snipEditRedrawBase(); } break;
    }
  }
  snipEditLiveCv.addEventListener("pointerdown", snipEditOnPointerDown);
  snipEditLiveCv.addEventListener("pointermove", snipEditOnPointerMove);
  window.addEventListener("pointerup", snipEditOnPointerUp);
  // A cancelled pointer stream (e.g. the browser claiming a touch drag mid-
  // gesture) would otherwise leave SE.drag/SE.panState set forever, which
  // makes every tool button and shortcut a no-op for the rest of the session.
  window.addEventListener("pointercancel", snipEditOnPointerUp);
  // Alt-Tabbing away while Space/V is held never delivers the matching keyup
  // to this window -- without this, every later click would start a pan
  // (SE.spaceDown stuck true) or the raw/before image would stay stuck shown.
  window.addEventListener("blur", () => {
    if (SE.spaceDown) { SE.spaceDown = false; if (SE.tool !== "hand") snipEditWrap.classList.remove("panning"); }
    if (SE.showRaw) { SE.showRaw = false; snipEditRedrawBase(); }
  });
  snipEditWrap.addEventListener("wheel", (e) => {
    if (!SE.active) return;
    e.preventDefault();
    const rect = snipEditWrap.getBoundingClientRect();
    const cx = e.clientX - rect.left + snipEditWrap.scrollLeft;
    const cy = e.clientY - rect.top + snipEditWrap.scrollTop;
    const oldZoom = SE.zoom;
    const newZoom = snipEditClamp(oldZoom * (e.deltaY < 0 ? 1.1 : 1 / 1.1), 0.05, 8);
    const ratio = newZoom / oldZoom;
    snipEditSetZoom(newZoom);
    snipEditWrap.scrollLeft = cx * ratio - (e.clientX - rect.left);
    snipEditWrap.scrollTop = cy * ratio - (e.clientY - rect.top);
  }, { passive: false });

  snipEditTextInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); e.stopPropagation(); snipEditCommitTextInput(); }
    else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); snipEditCancelTextInput(); }
  });
  snipEditTextInput.addEventListener("blur", () => { if (SE.textEdit) snipEditCommitTextInput(); });

  // ---- keyboard: tool shortcuts, undo/redo, Enter/Esc, nudge, pan, before/after ----
  function snipEditKeydown(e) {
    if (snipEdit.classList.contains("hidden")) return;
    if (e.key === "Tab") { trapTabWithin(snipEdit, e); return; }
    if (e.key === "Escape") {
      e.preventDefault();
      if (snipColorPopIsOpen()) { closeSnipColorPop(); return; }
      if (SE.eyedropArm) { SE.eyedropArm = null; snipEditRenderContext(); return; }
      if (SE.tool === "crop" && (SE.drag || SE.cropRect)) { snipEditCropDiscard(); return; }
      snipEditCancel();
      return;
    }
    if (document.activeElement === snipEditTextInput) return; // input owns its own keys
    if (e.key === "Enter") {
      e.preventDefault();
      if (SE.tool === "crop" && SE.cropRect) { snipEditCropCommit(); return; }
      snipEditApply();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") { e.preventDefault(); snipEditUndo(); return; }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "y") { e.preventDefault(); snipEditRedo(); return; }
    if (e.key === " ") { SE.spaceDown = true; snipEditWrap.style.cursor = ""; snipEditWrap.classList.add("panning"); e.preventDefault(); return; }
    if (e.key.toLowerCase() === "v" && !e.repeat) { SE.showRaw = true; snipEditRedrawBase(); return; }
    if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key) && SE.lastNudged) {
      e.preventDefault();
      snipEditNudge(SE.lastNudged, e.key, e.shiftKey ? 10 : 1);
      return;
    }
    const tdef = SNIP_TOOLS.find((t) => t.key === e.key || (t.key === "-" && e.key === "ß"));
    if (tdef) { e.preventDefault(); snipEditSetTool(tdef.id); }
  }
  function snipEditKeyup(e) {
    if (snipEdit.classList.contains("hidden")) return;
    if (e.key === " ") { SE.spaceDown = false; if (SE.tool !== "hand") snipEditWrap.classList.remove("panning"); }
    if (e.key.toLowerCase() === "v") { SE.showRaw = false; snipEditRedrawBase(); }
  }
  document.addEventListener("keydown", snipEditKeydown);
  document.addEventListener("keyup", snipEditKeyup);

  // ---- open / cancel / apply / export ----
  function snipEditOpen(dataUrl, { onApply, onCancel }) {
    const img = new Image();
    // A capture that fails to decode (truncated data URL, a broken picked file)
    // must not leave the editor silently never opening -- fall back to the
    // same path a user cancel already takes, so nothing is left half-open.
    img.onerror = () => {
      toast(t("snipEditOpenFailed"));
      onCancel?.();
    };
    img.onload = () => {
      SE.img = img;
      SE.w = img.naturalWidth;
      SE.h = img.naturalHeight;
      SE.ops = [];
      SE.redo = [];
      SE.floor = 0;
      SE.perTool = {};
      SE.tool = SNIP_EDIT_DEFAULT_TOOL();
      SE.cropRect = null;
      SE.lastNudged = null;
      SE.eyedropArm = null;
      SE.showRaw = false;
      SE.drag = null;
      closeSnipColorPop();
      SE.onApply = onApply;
      SE.onCancel = onCancel;
      SE.active = true;
      snipEditWrap.style.cursor = ""; // clear a cursor left inline by the previous session

      snipEditBaseCv.width = SE.w;
      snipEditBaseCv.height = SE.h;
      snipEditLiveCv.width = SE.w;
      snipEditLiveCv.height = SE.h;
      SE.rawCanvas = document.createElement("canvas");
      SE.rawCanvas.width = SE.w;
      SE.rawCanvas.height = SE.h;
      SE.rawCanvas.getContext("2d").drawImage(img, 0, 0);

      snipEditBuildToolButtons();
      snipEditRenderToolButtons();
      snipEditRenderContext();
      snipEditRedrawBase();
      snipEditUpdateUndoRedoButtons();
      snipEdit.classList.remove("hidden");
      snipEditFit();
      snipEditWrap.scrollLeft = 0;
      snipEditWrap.scrollTop = 0;
    };
    img.src = dataUrl;
  }
  function snipEditClose() {
    SE.active = false;
    snipEdit.classList.add("hidden");
    SE.drag = null;
    SE.panState = null;
    SE.spaceDown = false;
    SE.eyedropArm = null;
    snipEditWrap.classList.remove("panning");
    snipEditWrap.style.cursor = "";
    snipEditCancelTextInput();
  }
  function snipEditCancel() {
    const cb = SE.onCancel;
    snipEditClose();
    cb?.();
  }
  // Applies the crop (if any) only now -- everything else stays undoable until export.
  // The output canvas is never larger than the source, so a 4K/panorama capture is
  // never upscaled, and this is a single synchronous pass, never a per-block loop.
  function snipEditExportDataUrl() {
    // A drawn crop rectangle otherwise only reaches SE.ops via an Enter keypress --
    // export must not require that hidden step, or a crop drawn (or redrawn over an
    // older one) and exported straight away silently exports uncropped/stale instead.
    // Scoped to the crop tool being active (same guard the Enter handler already
    // uses) so this never resurrects a crop the user has since undone while parked
    // on another tool -- SE.cropRect is only kept in sync with SE.ops while crop is
    // the active tool (see snipEditSetTool).
    if (SE.tool === "crop") snipEditCropCommit();
    SE.showRaw = false;
    snipEditRedrawBase();
    const crop = SE.ops.find((o) => o.tool === "crop");
    let sx = 0, sy = 0, sw = SE.w, sh = SE.h;
    if (crop) {
      sx = snipEditClamp(Math.round(crop.rect.x), 0, SE.w);
      sy = snipEditClamp(Math.round(crop.rect.y), 0, SE.h);
      sw = Math.min(SE.w - sx, Math.round(crop.rect.w));
      sh = Math.min(SE.h - sy, Math.round(crop.rect.h));
    }
    if (sw < 8 || sh < 8) { sx = 0; sy = 0; sw = SE.w; sh = SE.h; }
    const out = document.createElement("canvas");
    out.width = sw;
    out.height = sh;
    out.getContext("2d").drawImage(snipEditBaseCv, sx, sy, sw, sh, 0, 0, sw, sh);
    // Always lossless png regardless of the screenshotFormat setting -- the
    // setting's actual target format is applied once, in Rust, at the real
    // save step (save_screenshot_now/_as re-encode from this data URL), so an
    // edited screenshot never picks up a lossy detour on its way there.
    return out.toDataURL("image/png");
  }
  function snipEditApply() {
    snipEditFinishTextIfOpen();
    const dataUrl = snipEditExportDataUrl();
    const cb = SE.onApply;
    snipEditClose();
    cb?.(dataUrl);
  }
  $("snip-edit-cancel").addEventListener("click", snipEditCancel);
  $("snip-edit-close").addEventListener("click", snipEditCancel);
  $("snip-edit-apply").addEventListener("click", snipEditApply);
  $("snip-edit-copy").addEventListener("click", async () => {
    const dataUrl = snipEditExportDataUrl();
    await invoke("copy_image_data", { dataUrl }).catch((err) => errToast(err));
  });
  $("snip-edit-undo").addEventListener("click", snipEditUndo);
  $("snip-edit-redo").addEventListener("click", snipEditRedo);
  $("snip-edit-zoom-in").addEventListener("click", () => snipEditZoomBy(1.25));
  $("snip-edit-zoom-out").addEventListener("click", () => snipEditZoomBy(0.8));
  $("snip-edit-fit").addEventListener("click", snipEditFit);

  // ---- resize (scale the image itself, not the view -- see snipEditApplyScale) ----
  const snipScalePop = document.createElement("div");
  snipScalePop.className = "snip-scale-pop hidden";
  document.body.appendChild(snipScalePop);
  function closeSnipScalePop() { snipScalePop.classList.add("hidden"); }
  function snipScalePopIsOpen() { return !snipScalePop.classList.contains("hidden"); }
  function openSnipScalePop(anchor) {
    const origW = SE.w, origH = SE.h;
    let lockAspect = true;
    snipScalePop.innerHTML = "";

    const size = document.createElement("div");
    size.className = "snip-scale-current";
    size.textContent = t("snipScaleCurrentSize").replace("{w}", origW).replace("{h}", origH);
    snipScalePop.appendChild(size);

    const widthField = numberField({
      value: origW, min: 1, max: 20000, step: 1,
      decreaseLabel: t("numFieldDecrease"), increaseLabel: t("numFieldIncrease"),
      decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease",
      onChange: (w) => {
        if (lockAspect) heightField.setValue(Math.max(1, Math.round((origH * w) / origW)));
        percentField.setValue(Math.round((w / origW) * 100));
      },
    });
    const heightField = numberField({
      value: origH, min: 1, max: 20000, step: 1,
      decreaseLabel: t("numFieldDecrease"), increaseLabel: t("numFieldIncrease"),
      decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease",
      onChange: (h) => {
        if (lockAspect) widthField.setValue(Math.max(1, Math.round((origW * h) / origH)));
        percentField.setValue(Math.round((h / origH) * 100));
      },
    });
    // Presets and the percent field itself share this: they always set both
    // dimensions from the ORIGINAL size, regardless of the lock-aspect toggle
    // (a percentage is inherently aspect-preserving).
    const setByPercent = (p) => {
      percentField.setValue(p);
      widthField.setValue(Math.max(1, Math.round((origW * p) / 100)));
      heightField.setValue(Math.max(1, Math.round((origH * p) / 100)));
    };
    const percentField = numberField({
      value: 100, min: 1, max: 1000, step: 5,
      decreaseLabel: t("numFieldDecrease"), increaseLabel: t("numFieldIncrease"),
      decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease",
      onChange: (p) => {
        widthField.setValue(Math.max(1, Math.round((origW * p) / 100)));
        heightField.setValue(Math.max(1, Math.round((origH * p) / 100)));
      },
    });

    const wRow = document.createElement("label");
    wRow.className = "snip-scale-row";
    wRow.textContent = t("snipScaleWidth");
    wRow.appendChild(widthField);
    const hRow = document.createElement("label");
    hRow.className = "snip-scale-row";
    hRow.textContent = t("snipScaleHeight");
    hRow.appendChild(heightField);
    const pRow = document.createElement("label");
    pRow.className = "snip-scale-row";
    pRow.textContent = t("snipScalePercent");
    pRow.appendChild(percentField);
    snipScalePop.append(wRow, hRow, pRow);

    const lockRow = document.createElement("label");
    lockRow.className = "snip-scale-lock";
    const lockBox = document.createElement("input");
    lockBox.type = "checkbox";
    lockBox.checked = true;
    lockBox.addEventListener("change", () => { lockAspect = lockBox.checked; });
    lockRow.append(lockBox, document.createTextNode(t("snipScaleLockAspect")));
    snipScalePop.appendChild(lockRow);

    const presets = document.createElement("div");
    presets.className = "snip-scale-presets";
    [25, 50, 200].forEach((p) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = `${p}%`;
      b.addEventListener("click", () => setByPercent(p));
      presets.appendChild(b);
    });
    const fitBtn = document.createElement("button");
    fitBtn.type = "button";
    fitBtn.textContent = t("snipScalePresetFit1920");
    fitBtn.addEventListener("click", () => {
      widthField.setValue(1920);
      heightField.setValue(Math.max(1, Math.round((origH * 1920) / origW)));
      percentField.setValue(Math.round((1920 / origW) * 100));
    });
    presets.appendChild(fitBtn);
    snipScalePop.appendChild(presets);

    const actions = document.createElement("div");
    actions.className = "snip-scale-actions";
    const cancelBtn = document.createElement("button");
    cancelBtn.type = "button";
    cancelBtn.className = "ghost-btn";
    cancelBtn.textContent = t("cancel");
    cancelBtn.addEventListener("click", closeSnipScalePop);
    const applyBtn = document.createElement("button");
    applyBtn.type = "button";
    applyBtn.className = "primary-btn";
    applyBtn.textContent = t("apply");
    applyBtn.addEventListener("click", () => {
      const w = Number(widthField.querySelector("input").value);
      const h = Number(heightField.querySelector("input").value);
      if (snipEditApplyScale(w, h)) closeSnipScalePop();
    });
    actions.append(cancelBtn, applyBtn);
    snipScalePop.appendChild(actions);

    snipScalePop.classList.remove("hidden");
    placePopover(snipScalePop, anchor.getBoundingClientRect(), 6);
  }
  $("snip-edit-scale").addEventListener("click", (e) => {
    if (snipScalePopIsOpen()) { closeSnipScalePop(); return; }
    openSnipScalePop(e.currentTarget);
  });
  document.addEventListener("pointerdown", (e) => {
    if (!snipScalePopIsOpen()) return;
    if (snipScalePop.contains(e.target) || e.target.closest("#snip-edit-scale")) return;
    closeSnipScalePop();
  });
  $("snip-edit-saveas").addEventListener("click", async () => {
    const dataUrl = snipEditExportDataUrl();
    try {
      const path = await invoke("save_screenshot_as", { dataUrl });
      if (path) toast(t("snipEditSavedTo").replace("{path}", path));
    } catch (e) { errToast(e); }
  });
  // SouthEast-only resize handle. The move side of the titlebar needs no JS
  // at all -- data-tauri-drag-region alone drags the window (same as the main
  // window's own header) -- but there is no HTML-attribute equivalent for
  // resizing, so this is the one spot that calls into the window API directly.
  $("snip-edit-resize-grip").addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    getCurrentWindow().startResizeDragging(ResizeDirection.SouthEast).catch(() => {});
  });

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
    if (!flag("snipEditor")) {
      // Off -> the pre-117/118 flow, unedited shot straight into the result dialog.
      $("snip-preview").src = pendingSnip;
      $("snip-save-folder").checked = settings.ui_flags?.screenshotSave === true;
      $("snip-modal-saveas-format").value = txt("screenshotFormat") || "png";
      snipModal.classList.remove("hidden");
      return;
    }
    // The editor sits between the capture and the result dialog.
    snipEditOpen(pendingSnip, {
      onApply: async (editedDataUrl) => {
        pendingSnip = editedDataUrl;
        // finalize_capture already put the UNEDITED image on the clipboard (and,
        // optionally, on disk) -- redo both side effects on the edited result.
        invoke("copy_image_data", { dataUrl: editedDataUrl }).catch((err) => errToast(err));
        if (settings.ui_flags?.screenshotSave === true) {
          if (pendingSnipPath) await invoke("delete_screenshot_file", { path: pendingSnipPath }).catch(() => {});
          pendingSnipPath = (await invoke("save_screenshot_now", { dataUrl: editedDataUrl }).catch(() => "")) || "";
        }
        $("snip-preview").src = pendingSnip;
        $("snip-save-folder").checked = settings.ui_flags?.screenshotSave === true;
        $("snip-modal-saveas-format").value = txt("screenshotFormat") || "png";
        snipModal.classList.remove("hidden");
      },
      // Cancel behaves like closing the result dialog today: the unedited shot
      // already on the clipboard/in the folder stays as-is, nothing more happens.
      onCancel: () => {
        pendingSnip = null;
        pendingSnipPath = "";
        pendingSnipName = "";
      },
    });
  });
  // Folder toggle on the result dialog = the same setting as in the main settings.
  // On -> save this shot to the folder now; off -> delete the saved copy.
  $("snip-save-folder").addEventListener("change", async (ev) => {
    const on = ev.target.checked;
    settings.ui_flags = { ...(settings.ui_flags || {}), screenshotSave: on };
    invoke("set_ui_flag", { key: "screenshotSave", enabled: on }).catch((err) => errToast(err));
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
    const format = $("snip-modal-saveas-format").value || undefined;
    await invoke("save_screenshot_as", { dataUrl: pendingSnip, format }).catch((err) => errToast(err));
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
    invoke("open_snip").catch((err) => errToast(err));
  });
  $("snip-modal-close").addEventListener("click", closeSnipModal);
  $("snip-preview").addEventListener("click", () => { const s = $("snip-preview").src; if (s) openLightbox(s, false); });
  snipModal.addEventListener("pointerdown", (e) => {
    if (e.target === snipModal) closeSnipModal();
  });

  // ---- OCR (text recognition, on by default -- flag "ocr") ----
  const ocrModal = $("ocr-modal");
  const ocrSettings = () => ({
    upscaleUnderPx: Math.round(val("ocrUpscaleUnderPx")),
    joinLines: flag("ocrJoinLines"),
    lang: txt("ocrLang") || "auto",
    autoPickLang: flag("ocrAutoPickLang"),
  });
  const closeOcrModal = () => ocrModal.classList.add("hidden");
  // Shared by all three entry points (snip-modal, composer screenshot, composer
  // file) and the ocr-captured event below -- one place decides "empty -> toast
  // only" and "auto-copy -> copy now + footer note instead of a toast".
  function showOcrResult(result) {
    const text = result?.text || "";
    if (!text) { toast(t("ocrNoTextFound")); return; }
    const autoCopy = flag("ocrAutoCopy");
    if (autoCopy) invoke("copy_text", { text }).catch(() => {});
    $("ocr-result-text").value = text;
    $("ocr-result-meta").textContent = `${result.lang || ""} · ${text.length}`;
    $("ocr-autocopy-note").classList.toggle("hidden", !autoCopy);
    ocrModal.classList.remove("hidden");
  }
  // (a) "Recognize text" in #snip-modal -- runs on the capture already shown there.
  // pendingSnip only ever holds a data URL at this point (it may not be saved to
  // disk yet, see the folder-toggle handler above), hence ocr_image_data.
  $("snip-modal-ocr").addEventListener("click", async () => {
    if (!pendingSnip) return;
    try {
      showOcrResult(await invoke("ocr_image_data", { dataUrl: pendingSnip, ...ocrSettings() }));
    } catch (err) { ocrErrToast(err); }
  });
  // (b) standalone composer button -- small menu (Screenshot / From image file).
  $("ocr-btn").addEventListener("click", () => {
    const r = $("ocr-btn").getBoundingClientRect();
    openMiniMenu([
      {
        label: t("ocrMenuFromFile"),
        run: async () => {
          const path = await withDialog(() => invoke("pick_file_path"));
          if (!path) return;
          try {
            showOcrResult(await invoke("ocr_image", { path, ...ocrSettings() }));
          } catch (err) { ocrErrToast(err); }
        },
      },
      {
        label: t("ocrMenuScreenshot"),
        run: async () => {
          // Arms the NEXT capture for OCR instead of a normal screenshot save; the
          // backend resets this the moment the session ends (capture or cancel).
          await invoke("set_snip_mode", { mode: "ocr" }).catch(() => {});
          invoke("open_snip").catch((err) => errToast(err));
        },
      },
    ], r.left, r.bottom + 4);
  });
  // Screenshot-into-OCR result, emitted by capture_region's OCR branch -- a
  // separate event from "snip-captured" so the normal screenshot flow above is
  // completely unaffected by OCR mode being armed.
  listen("ocr-captured", (e) => showOcrResult(e.payload));
  // Background thread found one or more installed language packs that need
  // updating (a version change, or Windows silently dropped one after a feature
  // update). Purely a notice -- the backend never auto-installs on its own, so a
  // toast the user ignores until it auto-hides is a complete no-op, not a missed
  // action. Cancel snoozes the next automatic check by ocrLangSnoozeH.
  listen("ocr-lang-update-due", (e) => {
    const due = e.payload || [];
    if (!due.length) return;
    const msg = due.length === 1
      ? t("ocrLangUpdateOne").replace("{lang}", due[0].label)
      : t("ocrLangUpdateMany").replace("{n}", String(due.length));
    toast(
      msg,
      { label: t("cancel"), onClick: () => invoke("ocr_lang_snooze").catch(() => {}) },
      "", null,
      Math.round(val("ocrLangNoticeMs")),
    );
  });
  // Same background-watcher notice, for voice language packs.
  listen("voice-lang-update-due", (e) => {
    const due = e.payload || [];
    if (!due.length) return;
    const msg = due.length === 1
      ? t("ocrLangUpdateOne").replace("{lang}", due[0].label)
      : t("ocrLangUpdateMany").replace("{n}", String(due.length));
    toast(
      msg,
      { label: t("cancel"), onClick: () => invoke("voice_lang_snooze").catch(() => {}) },
      "", null,
      Math.round(val("voiceLangNoticeMs")),
    );
  });
  // Voice dictation: the two buttons (composer, edit dialog) share one wiring
  // helper and one event pipeline -- see the section above for all of it.
  wireVoiceButton($("voice-btn"), () => inputEl);
  wireVoiceButton(modal.voiceBtn, () => modal.text);
  voiceWatchDiscard(inputEl);
  voiceWatchDiscard(modal.text);
  listen("voice-partial", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) return; // a late partial is ignored, not delivered
    resolveVoiceArming();
    insertVoiceText(payload.text || "", true);
  });
  listen("voice-text", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) { deliverLateVoiceText(payload); return; }
    resolveVoiceArming();
    const processed = applyVoiceCommands(payload.text || "", { punct: flag("voicePunct"), autoCaps: flag("voiceAutoCaps"), lang: voiceLangCode() });
    insertVoiceText(processed, false);
  });
  listen("voice-stopped", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) return;
    const wasHotkey = voiceTargetEl === voiceHotkeyBuffer;
    const finalText = wasHotkey ? voiceHotkeyBuffer.value : "";
    endVoiceUi();
    if (wasHotkey) deliverVoiceHotkeyText(finalText);
  });
  listen("voice-error", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) return;
    endVoiceUi();
    routeVoiceError(payload.code);
  });
  // The global hotkey toggled a session ON (a toggle OFF is handled entirely
  // on the Rust side, no event needed for that direction).
  listen("voice-hotkey-toggle", () => {
    if (voiceState !== "idle") return;
    voiceHotkeyBuffer.value = "";
    startVoice(voiceHotkeyBuffer, null);
  });
  // Management dialog chrome.
  $("ocr-langs-close-x").addEventListener("click", closeLangsModal);
  $("ocr-langs-modal").addEventListener("pointerdown", (e) => { if (e.target === $("ocr-langs-modal")) closeLangsModal(); });
  $("ocr-langs-recheck").addEventListener("click", () => loadLangCaps(langDialogKind, true));
  $("ocr-modal-copy").addEventListener("click", () => {
    const text = $("ocr-result-text").value;
    invoke("copy_text", { text }).then(() => toast(t("copied"))).catch((err) => errToast(err));
  });
  $("ocr-modal-create").addEventListener("click", () => {
    const text = $("ocr-result-text").value;
    closeOcrModal();
    openModal({ mode: "create", text, title: t("nameModalTitle") });
  });
  $("ocr-modal-close").addEventListener("click", closeOcrModal);
  $("ocr-modal-close-x").addEventListener("click", closeOcrModal);
  ocrModal.addEventListener("pointerdown", (e) => {
    if (e.target === ocrModal) closeOcrModal();
  });

  // Delete from the edit dialog, with the same two-step confirmation.
  modal.delete.addEventListener("click", async () => {
    if (!modalState || modalState.mode !== "edit") return;
    if (!armButton(modal.delete, t("deleteConfirm"))) return;
    const id = modalState.id;
    try {
      await invoke("delete_prompt", { id });
    } catch (err) {
      toast(String(err) === "save-failed" ? t("persistFailed") : String(err)); // keep the editor open so the user can retry
      return;
    }
    closeModal();
    await renderGrid();
    if (!libraryEl.classList.contains("hidden")) renderLibrary();
  });
  modal.root.addEventListener("pointerdown", async (e) => {
    if (e.target !== modal.root) return;
    if (await confirmDiscardIfDirty()) closeModal();
  });

  $("gear").addEventListener("click", () => {
    settingsOpener = document.activeElement;
    showExpertPage(false); // always open on the main page
    settingsEl.classList.remove("hidden");
  });
  $("expert-open").addEventListener("click", () => {
    expertQuery = "";
    $("expert-search").value = "";
    expertOnlyChanged = false;
    $("expert-only-changed").checked = false;
    syncOnlyChangedChip();
    showExpertPage(true);
  });
  $("expert-back").addEventListener("click", () => showExpertPage(false));
  const renderExpertDebounced = debounced(renderExpert, () => val("searchDebounceMs"));
  $("expert-search").addEventListener("input", (e) => { expertQuery = e.target.value; renderExpertDebounced(); });
  $("expert-only-changed").addEventListener("change", (e) => {
    expertOnlyChanged = e.target.checked;
    syncOnlyChangedChip();
    renderExpert();
  });
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

  // Overhang row: grow the grid one step at a time, capped at
  // gridMax, until the view has enough cells for every member (or it hits the
  // cap - the row then just stays, since nothing was lost either way).
  $("grid-overflow-grow").addEventListener("click", async () => {
    if (favView) return;
    const view = activeView();
    const gm = val("gridMax");
    let cols = view.cols;
    let rows = view.rows;
    while (cols * rows < view.members.length && (cols < gm || rows < gm)) {
      if (cols <= rows && cols < gm) cols++;
      else if (rows < gm) rows++;
      else break;
    }
    if (cols === view.cols && rows === view.rows) return;
    settings = await invoke("set_view_grid", { id: view.id, cols, rows });
    await renderGrid(true);
  });

  // Show/hide the top and bottom bars.
  $("hide-top").addEventListener("click", () => setBars(false, settings.show_composer !== false));
  $("show-top").addEventListener("click", () => setBars(true, settings.show_composer !== false));
  $("hide-bottom").addEventListener("click", () => setBars(settings.show_header !== false, false));
  $("show-bottom").addEventListener("click", () => setBars(settings.show_header !== false, true));

  // A double-click on any one of the four buttons folds or unfolds both bars
  // together, target state taken from the clicked button; the single click
  // that already fired on the way to the double-click is left as it landed.
  const bindBarToggleDbl = (id, target) => $(id).addEventListener("dblclick", () => {
    if (flag("barToggleBoth")) setBars(target, target);
  });
  bindBarToggleDbl("hide-top", false);
  bindBarToggleDbl("show-top", true);
  bindBarToggleDbl("hide-bottom", false);
  bindBarToggleDbl("show-bottom", true);

  buildColorFilter($("library-colors"), libColor);
  $("library-btn").addEventListener("click", async () => {
    await refreshLastEdited();
    libQuery = "";
    $("library-q").value = "";
    // The type pick is remembered per window; search, colour and favourites are not.
    libTypes.clear();
    for (const type of typeFilterOf("libTypeFilter")) libTypes.add(type);
    paintTypeChips($("library-types"), libTypes);
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
    try { await invoke("set_ui_flag", { key: "closeAfterCopy", enabled }); } catch (err) { errToast(err); }
    syncLibraryToggle();
  });
  const renderLibraryDebounced = debounced(renderLibrary, () => val("searchDebounceMs"));
  $("library-q").addEventListener("input", (e) => { libQuery = e.target.value; renderLibraryDebounced(); });
  wireSearchSuggest($("library-q"), "recentSearchLib");
  $("library-sort").addEventListener("change", (e) => {
    settings.ui_texts = { ...(settings.ui_texts || {}), librarySort: e.target.value };
    invoke("set_ui_text", { key: "librarySort", value: e.target.value }).catch((err) => errToast(err));
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
    if (btn) { toggleTypeFilter(libTypes, btn.dataset.type, $("library-types"), "libTypeFilter"); renderLibrary(); }
  });
  $("library-colors").addEventListener("click", (e) => {
    const btn = e.target.closest(".lib-color");
    if (btn) { setLibColor(btn.dataset.color); renderLibrary(); }
  });
  // Batch operations wiring.
  $("library-select-btn").addEventListener("click", () => setLibSelectMode(!libSelectMode));
  $("lib-batch-all").addEventListener("click", () => { for (const id of libDisplayOrder) libSelected.add(id); applyLibSelectionDom(); updateBatchBar(); });
  $("lib-batch-none").addEventListener("click", () => { libSelected.clear(); libLastId = null; applyLibSelectionDom(); updateBatchBar(); });
  $("lib-batch-fav").addEventListener("click", () => doBatch("favorite", { favorite: true }));
  $("lib-batch-unfav").addEventListener("click", () => doBatch("favorite", { favorite: false }));
  $("lib-batch-addview").addEventListener("click", () => doBatch("addView", { viewId: $("lib-batch-view").value }));
  $("lib-batch-removeview").addEventListener("click", () => doBatch("removeView", { viewId: $("lib-batch-view").value }));
  $("lib-batch-tag-add").addEventListener("click", async () => {
    const n = libSelected.size;
    if (!n) return;
    const raw = await confirmDialog({
      title: t("batchTagAdd"),
      message: t("batchTagAddMsg").replace("{n}", String(n)),
      confirmLabel: t("batchTagAdd"),
      input: { type: "text", placeholder: t("tagsPh"), datalist: allTags() },
      danger: false,
    });
    const tag = (raw || "").trim();
    if (!tag) return;
    const key = tagKey(tag);
    const changed = prompts.filter((p) => libSelected.has(p.id) && !(p.tags || []).some((x) => tagKey(x) === key)).length;
    if (await doBatch("tag-add", { tag })) toast(t("batchTagAddDone").replace("{n}", String(changed)));
  });
  $("lib-batch-tag-remove").addEventListener("click", async () => {
    const n = libSelected.size;
    if (!n) return;
    const raw = await confirmDialog({
      title: t("batchTagRemove"),
      message: t("batchTagRemoveMsg").replace("{n}", String(n)),
      confirmLabel: t("batchTagRemove"),
      input: { type: "text", placeholder: t("tagsPh"), datalist: allTags() },
      danger: false,
    });
    const tag = (raw || "").trim();
    if (!tag) return;
    const key = tagKey(tag);
    const changed = prompts.filter((p) => libSelected.has(p.id) && (p.tags || []).some((x) => tagKey(x) === key)).length;
    if (await doBatch("tag-remove", { tag })) toast(t("batchTagRemoveDone").replace("{n}", String(changed)));
  });
  $("lib-batch-export").addEventListener("click", () => batchExport($("lib-batch-format").value));
  $("lib-batch-delete").addEventListener("click", (e) => {
    const n = libSelected.size;
    if (!n) return;
    armDelete(e.currentTarget, t("batchDeleteConfirm").replace("{n}", String(n)), async () => {
      if (await doBatch("delete")) toast(t("batchDeleteDone").replace("{n}", String(n)));
    });
  });
  // Esc leaves selection mode before it closes the library (capture beats the overlay).
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && libSelectMode && !libraryEl.classList.contains("hidden")) {
      e.stopPropagation();
      setLibSelectMode(false);
    }
  }, true);
  // Grid batch toolbar wiring — same shape as the library's, its own state.
  $("grid-batch-none").addEventListener("click", clearGridSelection);
  $("grid-batch-move").addEventListener("click", () => doGridMove($("grid-batch-view").value));
  $("grid-batch-delete").addEventListener("click", (e) => {
    const n = gridSelected.size;
    if (!n) return;
    armDelete(e.currentTarget, t("batchDeleteConfirm").replace("{n}", String(n)), async () => {
      await doGridBatch("delete");
      toast(t("batchDeleteDone").replace("{n}", String(n)));
    });
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && gridSelected.size) {
      e.stopPropagation();
      clearGridSelection();
    }
  }, true);
  // "gone with one click" per spec — no confirm dialog, unlike a real delete
  // (these are throwaway onboarding content, not the user's own data).
  $("examples-dismiss").addEventListener("click", async () => {
    try { await invoke("dismiss_example_prompts"); } catch (err) { errToast(err); return; }
    await renderGrid();
  });
  $("library-close").addEventListener("click", hideLibrary);
  libraryEl.addEventListener("pointerdown", (e) => {
    if (e.target === libraryEl) hideLibrary();
  });
  // Duplicate finder modal (backups + statistics are now inline expert panels).
  $("dupes-close").addEventListener("click", () => $("dupes-modal").classList.add("hidden"));
  $("dupes-modal").addEventListener("pointerdown", (e) => { if (e.target === $("dupes-modal")) $("dupes-modal").classList.add("hidden"); });
  $("dupes-rescan").addEventListener("click", () => scanDupes());
  $("trash-close").addEventListener("click", () => $("trash-modal").classList.add("hidden"));
  $("trash-modal").addEventListener("pointerdown", (e) => { if (e.target === $("trash-modal")) $("trash-modal").classList.add("hidden"); });
  $("trash-clear").addEventListener("click", () => emptyTrashAll());
  // Clipboard inbox.
  $("clip-btn").addEventListener("click", openClipInbox);
  // Pause/resume collecting straight from the inbox — same switch as the expert one.
  $("clip-toggle").addEventListener("click", async () => {
    const on = !optFlag("clipWatcher");
    settings.ui_flags = { ...(settings.ui_flags || {}), clipWatcher: on };
    try { await invoke("set_ui_flag", { key: "clipWatcher", enabled: on }); }
    catch (e) { errToast(e); }
    applyFlags();
    await refreshClipInbox();
    updateClipBadge();
  });
  $("clip-close").addEventListener("click", () => $("clip-modal").classList.add("hidden"));
  $("clip-modal").addEventListener("pointerdown", (e) => { if (e.target === $("clip-modal")) $("clip-modal").classList.add("hidden"); });
  $("clip-clear").addEventListener("click", async () => { await invoke("clip_inbox_clear").catch((e) => errToast(e)); refreshClipInbox(); });
  // Emergency wipe: clears the live inbox AND scrubs clip_inbox rows out of every
  // rotating backup snapshot. Irreversible and touches every backup, so it asks
  // twice — the second dialog spells out that it cannot be undone.
  $("clip-purge-backups").addEventListener("click", async () => {
    const step1 = await confirmDialog({
      title: t("clipPurgeBackupsTitle"),
      message: t("clipPurgeBackupsBody1"),
      confirmLabel: t("clipPurgeBackupsConfirm"),
      cancelLabel: t("cancel"),
      danger: true,
    });
    if (!step1) return;
    const step2 = await confirmDialog({
      title: t("clipPurgeBackupsTitle"),
      message: t("clipPurgeBackupsBody2"),
      confirmLabel: t("clipPurgeBackupsConfirm2"),
      cancelLabel: t("cancel"),
      danger: true,
    });
    if (!step2) return;
    await invoke("clip_inbox_clear").catch((e) => errToast(e));
    const result = await invoke("purge_clip_inbox_from_backups").catch((e) => { errToast(e); return null; });
    refreshClipInbox();
    if (result !== null) {
      const [purged, failed] = result;
      let msg = t("clipPurgeBackupsDone").replace("{n}", String(purged));
      // Some snapshot could not be opened or its scrub could not be confirmed
      // written back -- an emergency wipe reporting silent success there would
      // leave the user believing they are clean when they may not be.
      if (failed > 0) msg += ` — ${t("clipPurgeBackupsFailed").replace("{n}", String(failed))}`;
      toast(msg);
    }
  });
  listen("clip-inbox-changed", () => refreshClipInbox());
  // Any window's copy of a rotating prompt lands here — refresh every tile of
  // that id (usually one, but "prompt in multiple views" can place several).
  listen("rotation-changed", (e) => {
    const { id, index, pos, total, text } = e.payload || {};
    if (!id) return;
    // Invalidates any in-flight primeRotateState peek for this id, so its reply
    // can no longer overwrite the fresher position this push just delivered.
    rotateReqs.set(id, (rotateReqs.get(id) || 0) + 1);
    rotateState.set(id, { index, pos, total, text });
    document.querySelectorAll(`.tile[data-id="${CSS.escape(id)}"]`).forEach((tile) => applyRotateState(tile, id));
  });
  // Auto-advance session start/end — drives the badge's is-auto accent and,
  // on start only, a toast (no toast on end — that would just be noise).
  listen("rotation-auto", (e) => {
    const { id, active } = e.payload || {};
    if (!id) return;
    if (active) autoActiveIds.add(id); else autoActiveIds.delete(id);
    document.querySelectorAll(`.tile[data-id="${CSS.escape(id)}"]`).forEach((tile) => applyAutoBadge(tile, id));
    if (active && flag("copyBubble")) toast(t("rotateAutoStarted"));
  });
  listen("rotation-auto-blocked", () => {
    toast(t("rotateAutoBlocked"));
  });
  // Clicking the app logo opens the project's GitHub page (drag still moves the window).
  $("app-logo")?.addEventListener("click", () => invoke("open_repo").catch((e) => errToast(e)));

  // About dialog — version + project page link.
  const aboutEl = $("about-modal");
  const closeAbout = () => aboutEl.classList.add("hidden");
  $("about-btn").addEventListener("click", () => aboutEl.classList.remove("hidden"));
  $("about-close").addEventListener("click", closeAbout);
  $("about-ok").addEventListener("click", closeAbout);
  $("about-repo").addEventListener("click", () => invoke("open_repo").catch((e) => errToast(e)));
  aboutEl.addEventListener("pointerdown", (e) => {
    if (e.target === aboutEl) closeAbout();
  });

  // Reopen the five-step tour (Help-menu style entry, always available
  // regardless of the firstRunTour toggle — that flag only gates the automatic
  // open). Step 5 writes live from here, see openTour()'s firstRun param.
  $("tour-open-btn").addEventListener("click", () => openTour({ firstRun: false }));
  $("tour-skip").addEventListener("click", tourSkip);
  $("tour-back").addEventListener("click", () => tourGoto(tourStep - 1));
  $("tour-next").addEventListener("click", () => (tourStep === TOUR_LAST_STEP ? tourFinish() : tourGoto(tourStep + 1)));
  const tourEl = $("tour-modal");
  tourEl.addEventListener("pointerdown", (e) => { if (e.target === tourEl) tourAskSkip(); });

  // In-app documentation.
  const helpEl = $("help");
  $("help-open").addEventListener("click", openHelp);
  $("help-close").addEventListener("click", () => helpEl.classList.add("hidden"));
  helpEl.addEventListener("pointerdown", (e) => {
    if (e.target === helpEl) helpEl.classList.add("hidden");
  });
  const renderHelpDebounced = debounced(renderHelp);
  $("help-search").addEventListener("input", (e) => { helpQuery = e.target.value; renderHelpDebounced(); });

  // Copy history & usage journal.
  const journalEl = $("journal");
  buildColorFilter($("journal-colors"), journalColor);
  $("journal-btn").addEventListener("click", openJournal);
  $("journal-close").addEventListener("click", () => journalEl.classList.add("hidden"));
  journalEl.addEventListener("pointerdown", (e) => {
    if (e.target === journalEl) journalEl.classList.add("hidden");
  });
  const renderJournalDebounced = debounced(renderJournal);
  $("journal-q").addEventListener("input", (e) => { journalQuery = e.target.value; renderJournalDebounced(); });
  wireSearchSuggest($("journal-q"), "recentSearchJournal");
  $("journal-types").addEventListener("click", (e) => {
    if (e.target.closest("#journal-fav")) {
      journalFav = !journalFav;
      $("journal-fav").classList.toggle("active", journalFav);
      renderJournal();
      return;
    }
    const btn = e.target.closest(".lib-type");
    if (btn) { toggleTypeFilter(journalTypes, btn.dataset.type, $("journal-types"), "journalTypeFilterPick"); renderJournal(); }
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
    await invoke("clear_copy_history").catch((e) => errToast(e));
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
      errToast(err);
      return;
    }
    closeViewModal();
    renderViews();
    await renderGrid(true);
  });

  $("opt-minimize").addEventListener("change", (e) => {
    invoke("set_minimize_on_close", { enabled: e.target.checked }).catch((err) => errToast(err));
  });
  $("opt-screenshot-folder").addEventListener("change", (e) => {
    settings.ui_flags = { ...(settings.ui_flags || {}), screenshotSave: e.target.checked };
    invoke("set_ui_flag", { key: "screenshotSave", enabled: e.target.checked }).catch((err) => errToast(err));
  });
  // Master tooltip switch (default on): off = no hover tooltips anywhere.
  $("opt-tooltips").addEventListener("change", (e) => {
    settings.ui_flags = { ...(settings.ui_flags || {}), tooltipsEnabled: e.target.checked };
    invoke("set_ui_flag", { key: "tooltipsEnabled", enabled: e.target.checked }).catch((err) => errToast(err));
  });
  // Auto-paste (default on): normal setting; enter-after stays in the expert menu.
  $("opt-autopaste").addEventListener("change", (e) => {
    settings.ui_flags = { ...(settings.ui_flags || {}), autoPaste: e.target.checked };
    invoke("set_ui_flag", { key: "autoPaste", enabled: e.target.checked }).catch((err) => errToast(err));
  });
  // Clipboard inbox is toggled from the expert menu + toolbar right-click only.
  $("opt-autostart").addEventListener("change", async (e) => {
    try {
      await invoke("set_autostart", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked;
      errToast(err);
    }
  });
  $("opt-startmin").addEventListener("change", async (e) => {
    try {
      await invoke("set_start_minimized", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked;
      errToast(err);
    }
  });
  // Always-on-top: the header pin and the settings switch stay in sync; the
  // pin tints itself (accent) while active.
  const setOnTop = (on) => {
    settings.always_on_top = on;
    $("opt-ontop").checked = on;
    $("pin-top").classList.toggle("active", on);
    $("pin-top").setAttribute("aria-pressed", String(on));
    invoke("set_always_on_top", { enabled: on }).catch((err) => errToast(err));
  };
  $("opt-ontop").addEventListener("change", (e) => setOnTop(e.target.checked));
  $("pin-top").addEventListener("click", () => setOnTop(!settings.always_on_top));
  // Grid tag filter toolbar button.
  $("grid-tag-btn").addEventListener("click", () => {
    if (!gridTagPop.classList.contains("hidden")) { closeGridTagPop(); return; }
    openGridTagPop();
  });

  // Favorites view: setting enables the header star; the star toggles the grid.
  $("opt-favview").addEventListener("change", (e) => {
    const on = e.target.checked;
    settings.ui_flags = { ...(settings.ui_flags || {}), favView: on };
    invoke("set_ui_flag", { key: "favView", enabled: on }).catch((err) => errToast(err));
    if (!on && favView) favView = false;
    refreshFavViewUi();
    renderGrid(true);
  });
  $("fav-view-btn").addEventListener("click", () => setFavView(!favView));

  // Global hotkey: click the field and press a key combo to capture it.
  bindHotkeyField($("opt-hotkey"), {
    current: () => settings.hotkey,
    onCommit: commitGlobalHotkey,
  });
  $("hotkey-clear").addEventListener("click", async () => {
    try { await commitGlobalHotkey(""); } catch (_) { /* Clear always leaves an empty field regardless */ }
    $("opt-hotkey").value = "";
  });
  $("hotkey-list-btn").addEventListener("click", openHotkeysModal);
  bindHotkeyField($("opt-voice-hotkey"), {
    current: () => settings.voice_hotkey,
    onCommit: commitVoiceHotkey,
  });
  $("voice-hotkey-clear").addEventListener("click", async () => {
    try { await commitVoiceHotkey(""); } catch (_) { /* Clear always leaves an empty field regardless */ }
    $("opt-voice-hotkey").value = "";
  });
  EXPERT_ACTIONS["voice-hotkey-set"] = () => {
    settingsOpener = document.activeElement;
    showExpertPage(false);
    settingsEl.classList.remove("hidden");
    requestAnimationFrame(() => {
      $("opt-voice-hotkey").scrollIntoView({ block: "center" });
      $("opt-voice-hotkey").focus();
    });
  };
  $("hotkeys-modal-close").addEventListener("click", closeHotkeysModal);
  $("hotkeys-modal").addEventListener("pointerdown", (e) => {
    if (e.target === $("hotkeys-modal")) closeHotkeysModal();
  });
  $("hotkeys-search").addEventListener("input", renderHotkeysList);

  $("import-btn").addEventListener("click", runImport);
  $("export-btn").addEventListener("click", () => runExport($("export-format").value));
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
    settings.theme = themeSelect.value; // applyTheme reads the custom overrides from it
    applyTheme(await invoke("set_theme", { theme: themeSelect.value }));
  });

  // Language: persist, then re-render every translated string in place —
  // no restart, no reload.
  $("lang-select").addEventListener("change", async (e) => {
    await invoke("set_language", { lang: e.target.value });
    LANG = pickLang(e.target.value);
    applyI18n();
    // Re-fill the JS-built selects, then restore their current values.
    fillSizeSelects();
    fillFontSelects();
    fillThemeSelect(); // the custom-theme separator is a translated label too
    $("tile-font").value = settings.tile_font || "system";
    $("tile-size").value = String(normSize(Number(settings.tile_size ?? 0)));
    await renderGrid(); // fresh state: tooltips + a renamed default view
    if (!libraryEl.classList.contains("hidden")) renderLibrary();
    if (!$("journal").classList.contains("hidden")) renderJournal();
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
  attachSelectPicker(modal.hotkeyAction);
  attachSelectPicker(modal.rotateMode);
  modal.caption.addEventListener("input", updateCaptionPreview);
  modal.captionSize.addEventListener("change", updateCaptionPreview);
  attachSelectPicker($("tile-font"));
  attachSelectPicker(modal.fontSel);
  attachSelectPicker($("theme-select"));
  attachSelectPicker($("lang-select"));
  attachSelectPicker($("theme-edit-target"));
  attachSelectPicker($("theme-edit-base"));
  attachSelectPicker($("grid-batch-view")); // themed popup, options refreshed by fillBatchViews()
  attachSelectPicker($("lib-batch-view"));
  attachSelectPicker($("lib-batch-format"));
  attachSelectPicker($("export-format"));
  attachSelectPicker($("video-cleanup-age"));

  // UI size + icon size: percentage-preset selects mirroring the expert scales.
  fillScaleSelect("ui-scale", "uiScale");
  fillScaleSelect("icon-scale", "iconScale");
  const scaleChanged = (key) => async (e) => {
    const v = Number(e.target.value);
    settings.ui_values = { ...(settings.ui_values || {}), [key]: v };
    await invoke("set_ui_value", { key, value: v }).catch((err) => errToast(err));
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
      try {
        await invoke("delete_prompt", { id });
      } catch (err) {
        toast(String(err) === "save-failed" ? t("persistFailed") : String(err));
        return;
      }
      await renderGrid();
      return;
    }
    if (act === "views") {
      const rect = ctxEl.getBoundingClientRect();
      closeCtx();
      openViewsPop(id, rect);
      return;
    }
    closeCtx();
    if (act === "edit") {
      await editPrompt(id);
    } else if (act === "editVideo") {
      const p = prompts.find((x) => x.id === id);
      if (p?.file_path) await invoke("open_video_editor", { srcPath: p.file_path }).catch((e) => errToast(e));
    } else if (act === "duplicate") {
      await duplicatePrompt(id);
    } else if (act === "hide") {
      // Remove from the active view's grid; stays available in the library.
      const view = activeView();
      const layout = { ...layoutOf(view) };
      delete layout[id];
      view.layouts[gridKeyOf(view)] = layout;
      invoke("set_layout", { layout }).catch((e) => errToast(e));
      await renderGrid(true);
    } else if (act === "pin") {
      // The command answers with the state it actually left the prompt in — take that
      // instead of assuming the click flipped the label that was just on screen.
      await invoke("toggle_floating", { id })
        .then((pinned) => cacheFloatingState(id, pinned))
        .catch((e) => errToast(e));
    } else if (act === "rotateAutoMenu") {
      await toggleRotateAutoDirect(id);
    }
  });

  document.addEventListener("pointerdown", (e) => {
    tabDragBlockClick = false; // a fresh press ends the suppression a drop armed
    if (!ctxEl.classList.contains("hidden") && !ctxEl.contains(e.target)) closeCtx();
    if (!toolbarMenu.classList.contains("hidden") && !toolbarMenu.contains(e.target)) closeToolbarMenu();
    if (!miniMenu.classList.contains("hidden") && !miniMenu.contains(e.target)) closeMiniMenu();
    // The group tab itself toggles the popover, so let its own click through.
    if (!viewGroupPop.classList.contains("hidden") && !viewGroupPop.contains(e.target) &&
        !e.target.closest(".view-group-btn")) closeViewGroupPop();
    // The folder tile toggles its own popover, so let its press through.
    if (!tileFolderPop.classList.contains("hidden") && !tileFolderPop.contains(e.target) &&
        !e.target.closest(".folder-tile")) closeTileFolderPop();
    // Opened from the (already-closed) tile context menu, so nothing else needs excluding.
    if (!viewsPop.classList.contains("hidden") && !viewsPop.contains(e.target)) closeViewsPop();
    // The toolbar button toggles its own popover, so let its own press through.
    if (!gridTagPop.classList.contains("hidden") && !gridTagPop.contains(e.target) &&
        !e.target.closest("#grid-tag-btn")) closeGridTagPop();
  });
  // Ctrl+Tab walks the views in the order the tab bar shows them, so a group's
  // members are reached in place instead of being skipped.
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Tab" || !e.ctrlKey || e.altKey) return;
    if (!flag("multiView") || settings.views.length < 2) return;
    if (document.querySelector(".overlay:not(.hidden)")) return; // a dialog is open
    e.preventDefault();
    const order = viewOrder();
    const at = order.findIndex((v) => v.id === settings.active_view);
    const next = order[((at < 0 ? 0 : at) + (e.shiftKey ? -1 : 1) + order.length) % order.length];
    if (next && next.id !== settings.active_view) switchToView(next.id);
  });
  // Right-click anywhere in the header actions → toggle which top-bar tools show.
  document.querySelector(".header-actions")?.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    openToolbarMenu(e.clientX, e.clientY);
  });
  // Keeps Tab from leaking past whichever overlay is currently topmost,
  // into the (visually dimmed) page behind it. Mirrors the Escape dispatcher's
  // own "what's on top wins" cascade below one for one, reusing the same
  // container references, so a new overlay only needs adding in one place to
  // stay consistent between the two. snipEdit and the first-run tour own their
  // own Tab handling already (trapTabWithin call sites in snipEditKeydown/
  // tourKeydown) and are skipped here to avoid trapping twice.
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Tab" || e.ctrlKey) return; // Ctrl+Tab is the view-switcher above
    if (!snipEdit.classList.contains("hidden")) return;
    // tourKeydown owns Tab while the tour is the topmost thing on screen, but it
    // bows out the moment a confirm dialog (tourAskSkip) opens on top of it -- and
    // that dialog's own trap below is never reached if this bails unconditionally
    // just because the tour is still open underneath. Only defer to the tour when
    // nothing is layered above it.
    if (!$("tour-modal").classList.contains("hidden") && $("confirm-modal").classList.contains("hidden")) return;
    if (!toolbarMenu.classList.contains("hidden")) { trapTabWithin(toolbarMenu, e); return; }
    if (!miniMenu.classList.contains("hidden")) { trapTabWithin(miniMenu, e); return; }
    if (!viewGroupPop.classList.contains("hidden")) { trapTabWithin(viewGroupPop, e); return; }
    if (!tileFolderPop.classList.contains("hidden")) { trapTabWithin(tileFolderPop, e); return; }
    if (!viewsPop.classList.contains("hidden")) { trapTabWithin(viewsPop, e); return; }
    if (!gridTagPop.classList.contains("hidden")) { trapTabWithin(gridTagPop, e); return; }
    if (!$("lightbox").classList.contains("hidden")) { trapTabWithin($("lightbox"), e); return; }
    if (!$("confirm-modal").classList.contains("hidden")) { trapTabWithin($("confirm-modal"), e); return; }
    if (!colorPop.classList.contains("hidden")) { trapTabWithin(colorPop, e); return; }
    if (!tagPop.classList.contains("hidden")) { trapTabWithin(tagPop, e); return; }
    if (!$("tag-modal").classList.contains("hidden")) { trapTabWithin($("tag-modal"), e); return; }
    if (!ctxEl.classList.contains("hidden")) { trapTabWithin(ctxEl, e); return; }
    if (!$("update-modal").classList.contains("hidden")) { trapTabWithin($("update-modal"), e); return; }
    if (!$("about-modal").classList.contains("hidden")) { trapTabWithin($("about-modal"), e); return; }
    if (!$("snip-modal").classList.contains("hidden")) { trapTabWithin($("snip-modal"), e); return; }
    if (!$("rec-modal").classList.contains("hidden")) { trapTabWithin($("rec-modal"), e); return; }
    if (!viewModal.root.classList.contains("hidden")) { trapTabWithin(viewModal.root, e); return; }
    if (!modal.root.classList.contains("hidden")) { trapTabWithin(modal.root, e); return; }
    if (!$("clip-modal").classList.contains("hidden")) { trapTabWithin($("clip-modal"), e); return; }
    if (!$("dupes-modal").classList.contains("hidden")) { trapTabWithin($("dupes-modal"), e); return; }
    if (!$("trash-modal").classList.contains("hidden")) { trapTabWithin($("trash-modal"), e); return; }
    if (document.body.classList.contains("library-front") && !libraryEl.classList.contains("hidden")) { trapTabWithin(libraryEl, e); return; }
    if (!$("backup-modal").classList.contains("hidden")) { trapTabWithin($("backup-modal"), e); return; }
    if (!journalEl.classList.contains("hidden")) { trapTabWithin(journalEl, e); return; }
    if (!libraryEl.classList.contains("hidden")) { trapTabWithin(libraryEl, e); return; }
    if (!helpEl.classList.contains("hidden")) { trapTabWithin(helpEl, e); return; }
    if (!settingsEl.classList.contains("hidden")) { trapTabWithin(settingsEl, e); return; }
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (!snipEdit.classList.contains("hidden")) return; // the editor owns Escape while open
    if (!toolbarMenu.classList.contains("hidden")) { closeToolbarMenu(); return; }
    if (!miniMenu.classList.contains("hidden")) { closeMiniMenu(); return; }
    if (!viewGroupPop.classList.contains("hidden")) { closeViewGroupPop(); return; }
    if (!tileFolderPop.classList.contains("hidden")) { closeTileFolderPop(); return; }
    if (!viewsPop.classList.contains("hidden")) { closeViewsPop(); return; }
    if (!gridTagPop.classList.contains("hidden")) { closeGridTagPop(); return; }
    if (!$("lightbox").classList.contains("hidden")) closeLightbox();
    else if (confirmCleanup) confirmCleanup(false);
    // The variable-fill dialog has its own keydown handler; consume Escape here so the
    // chain doesn't ALSO close the library/journal underneath it.
    else if (varsCleanup) { /* vars dialog closes itself */ }
    else if (!colorPop.classList.contains("hidden")) closeColorPop();
    else if (!tagPop.classList.contains("hidden")) closeTagPop();
    else if (!$("tag-modal").classList.contains("hidden")) closeTagManager();
    else if (!ctxEl.classList.contains("hidden")) closeCtx();
    else if (!$("update-modal").classList.contains("hidden")) $("update-modal").classList.add("hidden");
    else if (!$("about-modal").classList.contains("hidden")) $("about-modal").classList.add("hidden");
    else if (!$("snip-modal").classList.contains("hidden")) $("snip-modal").classList.add("hidden");
    else if (!$("rec-modal").classList.contains("hidden")) closeRecModal();
    else if (!viewModal.root.classList.contains("hidden")) closeViewModal();
    else if (!modal.root.classList.contains("hidden")) confirmDiscardIfDirty().then((ok) => { if (ok) closeModal(); });
    else if (!$("clip-modal").classList.contains("hidden")) $("clip-modal").classList.add("hidden");
    else if (!$("dupes-modal").classList.contains("hidden")) $("dupes-modal").classList.add("hidden");
    else if (!$("trash-modal").classList.contains("hidden")) $("trash-modal").classList.add("hidden");
    // A library raised over the backups dialog is the topmost thing — it closes first.
    else if (document.body.classList.contains("library-front") && !libraryEl.classList.contains("hidden")) hideLibrary();
    else if (!$("backup-modal").classList.contains("hidden")) closeBackups();
    else if (!journalEl.classList.contains("hidden")) journalEl.classList.add("hidden");
    else if (!libraryEl.classList.contains("hidden")) hideLibrary();
    // Help opens over the settings panel (from expGroupHelp) — close it before the
    // settings underneath, same pattern as library-front/journal above.
    else if (!helpEl.classList.contains("hidden")) helpEl.classList.add("hidden");
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
    // Undo a previous "up to date" check's tooltip override, if any.
    updateVersion.dataset.i18nTitle = "openReleases";
    updateVersion.title = t("openReleases");
  };
  // Clicking the status line: open the update dialog if one is pending, else the
  // GitHub releases page.
  updateVersion.addEventListener("click", () => {
    if (updateInfo?.available) openUpdateModal(updateInfo);
    else invoke("open_repo", { page: "releases" }).catch((e) => errToast(e));
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
  // The release body arrives as Markdown. Render the part release notes actually use
  // — headings, bullets, tables, bold and code — instead of dumping the raw source
  // with its "###" and "**" into a monospace box. Everything is built node by node:
  // the text comes from the network and must never reach innerHTML.
  const renderNotes = (box, text, version) => {
    box.textContent = "";
    const inline = (target, s) => {
      // A link has nowhere to go inside a dialog, so only its label survives.
      const re = /\*\*(.+?)\*\*|`([^`]+?)`|\[(.+?)\]\([^)]*\)/g;
      let last = 0, m;
      while ((m = re.exec(s))) {
        if (m.index > last) target.append(s.slice(last, m.index));
        if (m[3] !== undefined) target.append(m[3]);
        else {
          const el = document.createElement(m[1] !== undefined ? "strong" : "code");
          el.textContent = m[1] ?? m[2];
          target.append(el);
        }
        last = re.lastIndex;
      }
      target.append(s.slice(last));
    };
    let list = null, table = null, para = null;
    for (const raw of text.replace(/\r/g, "").split("\n")) {
      const line = raw.trim();
      if (!line) { list = table = para = null; continue; }
      const head = /^#{1,6}\s+(.*)$/.exec(line);
      const item = /^[-*+]\s+(.*)$/.exec(line);
      const row = line.startsWith("|") && line.endsWith("|");
      if (!item) list = null;
      if (!row) table = null;
      if (head || item || row) para = null;
      if (head) {
        // The dialog title already names the version, so its heading would only repeat it.
        if (version && head[1].includes(version)) continue;
        const h = document.createElement("h3");
        h.textContent = head[1];
        box.append(h);
      } else if (item) {
        if (!list) { list = document.createElement("ul"); box.append(list); }
        const li = document.createElement("li");
        inline(li, item[1]);
        list.append(li);
      } else if (row) {
        const cells = line.slice(1, -1).split("|").map((c) => c.trim());
        if (cells.every((c) => /^:?-{2,}:?$/.test(c))) continue; // the |---|---| divider
        if (!table) { table = document.createElement("table"); box.append(table); }
        const tr = document.createElement("tr");
        for (const c of cells) {
          const cell = document.createElement(table.rows.length ? "td" : "th");
          inline(cell, c);
          tr.append(cell);
        }
        table.append(tr);
      } else {
        // Markdown wraps a paragraph over several lines; keep it as one.
        if (para) para.append(" ");
        else { para = document.createElement("p"); box.append(para); }
        inline(para, line);
      }
    }
  };

  const openUpdateModal = (info) => {
    offerUpdate(info);
    updateModal.title.textContent = t("updateAvailable").replace("{v}", info.version);
    renderNotes(updateModal.notes, info.notes?.trim() || t("noChangelog"), info.version || "");
    updateModal.warn.classList.toggle("hidden", !info.skipped);
    updateModal.root.classList.remove("hidden");
  };
  const closeUpdateModal = () => updateModal.root.classList.add("hidden");

  updateBtn.addEventListener("click", async () => {
    updateBtn.disabled = true;
    try {
      const info = await invoke("check_update");
      if (info.available) offerUpdate(info);          // status → "Update to X available"
      else {
        // The short form keeps this single-line at the same width the long
        // sentence used to wrap at; the full sentence survives as a tooltip.
        // dataset.i18nTitle (not just .title) keeps it correct across a later
        // language switch too, the same as every other data-i18n-title element.
        updateVersion.textContent = t("upToDate").replace("{v}", appVersion);
        updateVersion.dataset.i18nTitle = "upToDateTip";
        updateVersion.title = t("upToDateTip");
        updateVersion.classList.remove("update-avail");
      }
    } catch (err) {
      errToast(err);
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
      errToast(err);
    }
  });
  updateModal.skip.addEventListener("click", async () => {
    const v = updateInfo?.version;
    if (!v) return;
    try { await invoke("skip_version", { version: v }); } catch (err) { errToast(err); }
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
    invoke("set_auto_update", { enabled: e.target.checked }).catch((err) => errToast(err));
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

  // ---------- ffmpeg ----------
  // Fetched on demand only — never on app start, never in the background.
  // Discovery (next to the exe → data folder → PATH) runs entirely on the Rust
  // side and touches no network; this only ever runs when the user explicitly
  // clicks the expert menu's action. Any future video feature that shells out
  // to ffmpeg should call the same three commands (ffmpeg_status /
  // download_ffmpeg / cancel_ffmpeg_download) from wherever it first needs
  // ffmpeg and finds it missing.
  let ffmpegBusy = false;
  EXPERT_ACTIONS["ffmpeg-download"] = async (btn) => {
    if (ffmpegBusy) {
      invoke("cancel_ffmpeg_download").catch(() => {});
      return;
    }
    const found = await invoke("ffmpeg_status").catch(() => null);
    ffmpegAvailable = !!found;
    if (found) {
      await infoDialog(t("ffmpegFoundTitle"), t("ffmpegFoundMsg").replace("{path}", found));
      renderExpert();
      return;
    }
    const ok = await confirmDialog({
      title: t("ffmpegOfferTitle"),
      message: t("ffmpegOfferMsg"),
      confirmLabel: t("ffmpegOfferConfirm"),
      cancelLabel: t("cancel"),
      danger: false,
    });
    if (!ok) return;
    ffmpegBusy = true;
    const label = btn.textContent;
    const origTitle = btn.title;
    btn.title = t("ffmpegCancelHint");
    const dlEta = makeEtaEstimator();
    const unlisten = await listen("ffmpeg-download-progress", (e) => {
      const pct = ffmpegDownloadPct(e.payload);
      dlEta.feed(pct);
      const eta = flag("etaShow") ? formatEta(dlEta.estimate(val("etaMinPct"), val("etaMinSecs")), t) : "";
      btn.textContent = t("ffmpegDownloading").replace("{pct}", String(pct)) + (eta ? ` (${eta})` : "");
    });
    try {
      await invoke("download_ffmpeg");
      ffmpegAvailable = true;
      toast(t("ffmpegDownloadDone"));
      renderExpert();
    } catch (err) {
      if (String(err) !== "cancelled") errToast(err);
    } finally {
      unlisten();
      ffmpegBusy = false;
      btn.textContent = label;
      btn.title = origTitle || "";
    }
  };

  listen("theme-changed", (e) => applyTheme(e.payload?.theme, e.payload?.vars));
  // "Edit prompt" chosen in a floating pill's right-click menu.
  listen("edit-prompt", (e) => editPrompt(String(e.payload)));
  // A prompt's global hotkey was pressed — run the same activation a tile does.
  // Whether it also pastes is the hotkey's OWN setting, falling back to the global one.
  listen("prompt-hotkey", (e) => {
    const id = String(e.payload);
    const mode = prompts.find((p) => p.id === id)?.hotkey_paste || "";
    onPromptHotkey(id, mode ? mode === "paste" : flag("promptHotkeyPaste"));
  });
  // A quick-copy entry in the tray menu — same activation, never auto-paste.
  listen("prompt-tray-copy", (e) => onPromptHotkey(String(e.payload), false));
  // The very first close-to-tray — backend held the window open, we explain
  // the tray menu, then hide it ourselves once the hint is dismissed.
  listen("first-tray-hide", async () => {
    await infoDialog(t("trayHintTitle"), t("trayHintMsg"));
    try { await window.__TAURI__?.window?.getCurrentWindow?.()?.hide?.(); } catch { /* no-op */ }
  });
  // The automatic backup has no frontend call site of its own to toast
  // from — the backend says so once the snapshot is really on disk.
  listen("backup-done", () => toast(t("backupDone")));
  // Same shape, the failure side -- an automatic backup that silently never ran
  // again would otherwise leave the user unprotected with nothing telling them.
  listen("backup-failed", () => toast(t("autoBackupFailed")));
  // A pill can unpin ITSELF (its own "Remove" action) without this window ever
  // being told -- the tile context menu's pin/unpin label then kept reading a
  // stale settings.floating and could re-open a pill the label said to remove.
  listen("floating-pin-changed", (e) => cacheFloatingState(e.payload.id, e.payload.pinned));
  // A copy recorded from a pill, the folder window or the tray never touched
  // this window's own settings.usage/last_used mirror -- "most used"/"recently
  // used" library sort silently froze at whatever it read at the last full
  // get_state. Only worth a re-render while the library is actually open.
  listen("usage-changed", (e) => {
    if (!settings.usage) settings.usage = {};
    if (!settings.last_used) settings.last_used = {};
    settings.usage[e.payload.id] = e.payload.usage;
    if (e.payload.lastUsed) settings.last_used[e.payload.id] = e.payload.lastUsed;
    else delete settings.last_used[e.payload.id];
    if (!libraryEl.classList.contains("hidden")) renderLibrary();
  });
  // save_prompts/save_trash used to fail silently to the user when both the DB
  // and the JSON fallback failed (disk full, permissions). One toast per event —
  // a stuck disk would otherwise spam this on every edit. A failed SETTINGS
  // write gets the sticky treatment instead of the toast: it is a state, not a
  // one-off event -- every setting changed for the rest of this session keeps
  // failing the same way and is lost on restart, so it stays visible in the
  // privacy group (like storeUnsealed) rather than something a missed toast
  // could hide.
  listen("persist-failed", (e) => {
    if (e.payload === "settings") {
      settingsWriteFailed = true;
      warnSettingsWriteFailed();
      if (!$("settings-expert").classList.contains("hidden")) renderExpert();
    } else {
      toast(t("persistFailed"));
    }
  });
  // The disk problem that caused the above went away and a later settings write
  // reached it again -- the notice does not need to survive that, so it comes
  // straight back down (the dialog itself still stays told-once for the session).
  listen("settings-write-recovered", () => {
    settingsWriteFailed = false;
    if (!$("settings-expert").classList.contains("hidden")) renderExpert();
  });
  // Sealing broke while the app was already running. The startup question is long
  // over by then, so the backend pushes it instead of this window polling for it.
  listen("store-unsealed", () => {
    storeSealed = false;
    warnStoreUnsealed();
    // An expert menu that happens to be open must not keep claiming everything is
    // fine until it is reopened.
    if (!$("settings-expert").classList.contains("hidden")) renderExpert();
  });
  // Pushed by the backend every few seconds instead of this window polling
  // missing_files on its own timer.
  listen("missing-files", (e) => applyMissingFiles(e.payload || []));
  // The standalone fill-in window answered (null = cancelled or closed).
  listen("vars-filled", (e) => {
    const resolve = varsWindowPending;
    varsWindowPending = null;
    resolve?.(e.payload ?? null);
  });
  // "Rename" in a pill folder's menu. The folder window hosts no themed dialog,
  // so the name is asked for here and written back from here.
  listen("folder-rename", async (e) => {
    const id = String(e.payload?.id || "");
    if (!id) return;
    const name = await confirmDialog({
      title: t("folderRenameTitle"),
      message: t("folderRenameMsg"),
      confirmLabel: t("save"),
      icon: FOLDER_SVG,
      danger: false,
      input: { type: "text", value: String(e.payload?.name || "") },
    });
    if (name === null) return;
    invoke("rename_float_folder", { id, name }).catch((err) => errToast(err));
  });
}

// Fill a UI/icon size select with percentage presets (plus the current value if it
// was fine-tuned in the expert menu), then select the current value.
const SCALE_PRESETS = [75, 90, 100, 110, 125, 150];
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
      if (!path) return;
      // A drop replaces whatever the editor is showing with a brand-new create
      // dialog -- if it is already open with unsaved changes, ask first, same as
      // the X button/backdrop/Escape.
      confirmDiscardOrProceed(() => openFileCreate(path).catch((err) => errToast(err)));
    }
  }).catch?.(() => {});
  // Text dropped from another app (no file paths) — native handler ignores it.
  window.addEventListener("dragover", (e) => e.preventDefault());
  window.addEventListener("drop", (e) => {
    if (!flag("dragDrop")) return;
    const text = e.dataTransfer?.getData("text");
    if (text && !(e.dataTransfer.files && e.dataTransfer.files.length)) {
      e.preventDefault();
      confirmDiscardOrProceed(() => {
        inputEl.value = text;
        autoGrow(inputEl);
        saveBtn.disabled = !text.trim();
        startCreate();
      });
    }
  });
}

// ---- First-run tour ----
// Five fixed steps in one shared frame (#tour-modal), content swapped per step.
// Steps 1-4 are a small hand-drawn illustration + up to three lines of text; step 5
// is the "optional features" settings table (TOUR_OPTIONS below). Opened two ways:
//  - Automatically once, right after the example prompts are seeded on a genuinely
//    fresh install (maybeOpenFirstRunTour, called from init()'s reveal()). Step 5
//    buffers its switches in a local draft and only writes what differs from the
//    shipped default, on Finish. Skip writes nothing but tourSeen.
//  - Manually via the settings dialog's "tour-open-btn" (always available) or the
//    expert menu's gated "tour-open" action (EXPERT_ACTIONS). Step 5 then writes
//    live, exactly like a normal expert-menu flag row — nothing is buffered or reset
//    just from reopening it this way.

// Tiny hand-drawn illustrations, ~110x110, currentColor + var(--accent) on the one
// interactive element each drawing highlights (never a raster asset, matches the
// toolbar icon convention just at a bigger size).
const TOUR_SVG_WELCOME = `<svg viewBox="0 0 100 100" width="110" height="110" aria-hidden="true">
  <rect x="8" y="14" width="84" height="72" rx="6" fill="none" stroke="currentColor" stroke-width="3"/>
  <line x1="8" y1="30" x2="92" y2="30" stroke="currentColor" stroke-width="3"/>
  <circle cx="16" cy="22" r="2" fill="currentColor"/>
  <circle cx="24" cy="22" r="2" fill="currentColor"/>
  <circle cx="32" cy="22" r="2" fill="currentColor"/>
  <rect x="16" y="38" width="20" height="16" rx="3" fill="none" stroke="currentColor" stroke-width="2.5"/>
  <rect x="40" y="38" width="20" height="16" rx="3" fill="var(--accent)" stroke="var(--accent)" stroke-width="2.5"/>
  <rect x="64" y="38" width="20" height="16" rx="3" fill="none" stroke="currentColor" stroke-width="2.5"/>
  <rect x="16" y="60" width="20" height="16" rx="3" fill="none" stroke="currentColor" stroke-width="2.5"/>
  <rect x="40" y="60" width="20" height="16" rx="3" fill="none" stroke="currentColor" stroke-width="2.5"/>
  <rect x="64" y="60" width="20" height="16" rx="3" fill="none" stroke="currentColor" stroke-width="2.5"/>
  <line x1="46" y1="43" x2="54" y2="43" stroke="var(--accent-fg)" stroke-width="1.5" stroke-linecap="round"/>
  <line x1="46" y1="48" x2="54" y2="48" stroke="var(--accent-fg)" stroke-width="1.5" stroke-linecap="round"/>
</svg>`;
// A click on a tile sends its text to the clipboard: the tile itself, a cursor
// over it, a motion arc for the copy, and the clipboard the line of text lands
// in. Replaces an earlier drawing whose two rectangles either side of a plus
// sign read as a battery rather than a click-to-copy action.
const TOUR_SVG_USAGE = `<svg viewBox="0 0 100 100" width="110" height="110" aria-hidden="true">
  <rect x="6" y="32" width="36" height="28" rx="4" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <line x1="13" y1="42" x2="35" y2="42" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
  <line x1="13" y1="50" x2="29" y2="50" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
  <path d="M34 46 L34 64 L38.2 60.4 L41.5 67.5 L45 65.8 L41.5 58.8 L47.5 58.8 Z" fill="currentColor"/>
  <path d="M48 44 Q64 26 80 36" fill="none" stroke="var(--accent)" stroke-width="2.2" stroke-dasharray="4 3" stroke-linecap="round"/>
  <path d="M75 32 L81 36 L76 41" fill="none" stroke="var(--accent)" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
  <rect x="64" y="48" width="30" height="36" rx="3" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <rect x="72" y="43" width="14" height="8" rx="2" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <line x1="71" y1="64" x2="89" y2="64" stroke="var(--accent)" stroke-width="2" stroke-linecap="round"/>
  <line x1="71" y1="72" x2="85" y2="72" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" opacity="0.6"/>
  <line x1="71" y1="80" x2="81" y2="80" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" opacity="0.4"/>
</svg>`;
const TOUR_SVG_TILES = `<svg viewBox="0 0 100 100" width="110" height="110" aria-hidden="true">
  <rect x="8" y="14" width="22" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <rect x="8" y="41" width="22" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="2.2" stroke-dasharray="3 3" opacity="0.55"/>
  <rect x="8" y="68" width="22" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <rect x="70" y="14" width="22" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <rect x="70" y="41" width="22" height="18" rx="3" fill="var(--accent)" stroke="var(--accent)" stroke-width="2.5"/>
  <rect x="70" y="68" width="22" height="18" rx="3" fill="none" stroke="currentColor" stroke-width="2.2"/>
  <path d="M32 23 Q50 6 68 23" fill="none" stroke="var(--accent)" stroke-width="2" stroke-dasharray="4 3"/>
  <path d="M64 20 L69 23 L65 27" fill="none" stroke="var(--accent)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
  <line x1="13" y1="19" x2="25" y2="19" stroke="currentColor" stroke-width="1.4"/>
  <line x1="13" y1="73" x2="25" y2="73" stroke="currentColor" stroke-width="1.4"/>
  <line x1="75" y1="19" x2="87" y2="19" stroke="currentColor" stroke-width="1.4"/>
  <line x1="75" y1="73" x2="87" y2="73" stroke="currentColor" stroke-width="1.4"/>
</svg>`;
const TOUR_SVG_LIBRARY = `<svg viewBox="0 0 100 100" width="110" height="110" aria-hidden="true">
  <line x1="10" y1="18" x2="68" y2="18" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"/>
  <line x1="10" y1="32" x2="58" y2="32" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"/>
  <line x1="10" y1="46" x2="64" y2="46" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"/>
  <rect x="10" y="60" width="18" height="10" rx="5" fill="none" stroke="currentColor" stroke-width="2"/>
  <rect x="32" y="60" width="18" height="10" rx="5" fill="none" stroke="currentColor" stroke-width="2"/>
  <circle cx="4" cy="65" r="2" fill="currentColor"/>
  <circle cx="26" cy="65" r="2" fill="currentColor"/>
  <circle cx="64" cy="52" r="17" fill="none" stroke="var(--accent)" stroke-width="3"/>
  <line x1="76" y1="64" x2="90" y2="78" stroke="var(--accent)" stroke-width="3.5" stroke-linecap="round"/>
  <line x1="56" y1="46" x2="72" y2="46" stroke="var(--accent)" stroke-width="1.6" opacity="0.6"/>
  <line x1="56" y1="58" x2="66" y2="58" stroke="var(--accent)" stroke-width="1.6" opacity="0.6"/>
</svg>`;

// The steps drawing + text (step 5, the settings table, is handled separately below).
const TOUR_STEPS = [
  { titleKey: "tourStep1Title", lineKeys: ["tourStep1Line1", "tourStep1Line2", "tourStep1Line3"], svg: TOUR_SVG_WELCOME },
  { titleKey: "tourStep2Title", lineKeys: ["tourStep2Line1", "tourStep2Line2", "tourStep2Line3"], svg: TOUR_SVG_USAGE },
  { titleKey: "tourStep3Title", lineKeys: ["tourStep3Line1", "tourStep3Line2", "tourStep3Line3"], svg: TOUR_SVG_TILES },
  { titleKey: "tourStep4Title", lineKeys: ["tourStep4Line1", "tourStep4Line2", "tourStep4Line3"], svg: TOUR_SVG_LIBRARY },
];
const TOUR_STEP_COUNT = TOUR_STEPS.length + 1; // + the settings step
const TOUR_LAST_STEP = TOUR_STEP_COUNT - 1;

// Step 5's table — written so a later point (the announced screenshot
// recording feature) can add its own row with exactly one new line.
const TOUR_OPTIONS = [
  { key: "screenshot", labelKey: "flagScreenshot", descKey: "tourOptScreenshot" },
  { key: "colorPicker", labelKey: "flagColorPicker", descKey: "tourOptPicker" },
  { key: "ocr", labelKey: "flagOcr", descKey: "tourOptOcr" },
  { key: "voice", labelKey: "flagVoice", descKey: "tourOptVoice" },
];

let tourStep = 0;
let tourFirstRun = false; // draft-buffered (true) vs. live-writing (false) step 5
let tourDraft = {}; // key -> bool, only meaningful while tourFirstRun is true

// Shared by both step-5 modes' "write it now" path: identical to flagRow()'s own
// change handler (minus the promptSync confirmation, which none of these three need).
async function tourSetFlagLive(key, enabled) {
  settings.ui_flags = { ...(settings.ui_flags || {}), [key]: enabled };
  try { await invoke("set_ui_flag", { key, enabled }); } catch (err) { errToast(err); }
  applyFlags();
}

function renderTourDrawingStep(step) {
  const wrap = document.createElement("div");
  wrap.className = "tour-step";
  const art = document.createElement("div");
  art.className = "tour-art";
  art.setAttribute("aria-hidden", "true");
  art.innerHTML = step.svg;
  const text = document.createElement("div");
  text.className = "tour-text";
  for (const lk of step.lineKeys) {
    const p = document.createElement("p");
    p.textContent = t(lk);
    text.appendChild(p);
  }
  wrap.append(art, text);
  return wrap;
}

function renderTourOptionsStep() {
  const wrap = document.createElement("div");
  wrap.className = "tour-options";
  for (const opt of TOUR_OPTIONS) {
    const row = document.createElement("label");
    row.className = "field switch-field tour-opt-row";
    const label = document.createElement("span");
    label.className = "tour-opt-label";
    const name = document.createElement("b");
    name.textContent = t(opt.labelKey);
    const desc = document.createElement("span");
    desc.className = "tour-opt-desc";
    desc.textContent = t(opt.descKey);
    label.append(name, desc);
    const note = document.createElement("span");
    note.className = "tour-opt-default";
    note.textContent = shipsOn(opt.key) ? t("tourDefaultOn") : t("tourDefaultOff");
    const input = document.createElement("input");
    input.type = "checkbox";
    input.className = "switch";
    input.checked = tourFirstRun ? tourDraft[opt.key] : flagState(opt.key);
    input.addEventListener("change", () => {
      if (tourFirstRun) tourDraft[opt.key] = input.checked;
      else tourSetFlagLive(opt.key, input.checked);
    });
    row.append(label, note, input);
    wrap.appendChild(row);
  }
  const restore = document.createElement("button");
  restore.type = "button";
  restore.className = "ghost-btn tour-restore-btn";
  restore.textContent = t("tourRestoreDefaults");
  restore.addEventListener("click", async () => {
    if (tourFirstRun) {
      tourDraft = Object.fromEntries(TOUR_OPTIONS.map((o) => [o.key, shipsOn(o.key)]));
    } else {
      for (const opt of TOUR_OPTIONS) await tourSetFlagLive(opt.key, shipsOn(opt.key));
    }
    renderTourStep(); // reflect the reset switches (and re-focus Next)
  });
  wrap.appendChild(restore);
  const foot = document.createElement("p");
  foot.className = "hint tour-options-foot";
  foot.textContent = t("tourFooterNote");
  wrap.appendChild(foot);
  return wrap;
}

function renderTourStep() {
  const isLast = tourStep === TOUR_LAST_STEP;
  const titleKey = tourStep < TOUR_STEPS.length ? TOUR_STEPS[tourStep].titleKey : "tourStep5Title";
  $("tour-title").textContent = t(titleKey);
  const body = $("tour-body");
  body.innerHTML = "";
  body.appendChild(tourStep < TOUR_STEPS.length ? renderTourDrawingStep(TOUR_STEPS[tourStep]) : renderTourOptionsStep());
  const dots = $("tour-dots");
  dots.innerHTML = "";
  for (let i = 0; i < TOUR_STEP_COUNT; i++) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "tour-dot" + (i === tourStep ? " active" : "");
    b.textContent = i === tourStep ? "●" : "○";
    b.setAttribute("aria-label", t(i < TOUR_STEPS.length ? TOUR_STEPS[i].titleKey : "tourStep5Title"));
    b.addEventListener("click", () => tourGoto(i));
    dots.appendChild(b);
  }
  $("tour-back").classList.toggle("hidden", tourStep === 0);
  $("tour-next").textContent = t(isLast ? "tourFinish" : "tourNext");
  $("tour-next").focus();
}

function tourGoto(i) {
  tourStep = Math.max(0, Math.min(TOUR_LAST_STEP, i));
  renderTourStep();
}

// Keyboard focus trap shared by every modal overlay with aria-modal="true"
// (screenshot editor, first-run tour): keeps Tab/Shift+Tab cycling within
// the container instead of escaping to the page behind it. Declared at top
// level so both bind()'s screenshot-editor code and the tour code below can
// call it without a second implementation.
function trapTabWithin(container, e) {
  const list = [...container.querySelectorAll('button, input, [tabindex]:not([tabindex="-1"]), select, textarea, a[href]')]
    .filter((el) => !el.disabled && el.offsetParent !== null);
  if (!list.length) return;
  const first = list[0], last = list[list.length - 1];
  if (!container.contains(document.activeElement)) { e.preventDefault(); first.focus(); return; }
  if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
  else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
}

function tourKeydown(e) {
  // A confirmation dialog on top (tourAskSkip) owns every key while it is open --
  // otherwise Escape would close both it and the tour underneath on one press.
  if (!$("confirm-modal").classList.contains("hidden")) return;
  if (e.key === "Tab") { trapTabWithin($("tour-modal"), e); return; }
  if (e.key === "Escape") { e.preventDefault(); tourAskSkip(); return; }
  if (e.key === "ArrowLeft") { e.preventDefault(); tourGoto(tourStep - 1); return; }
  if (e.key === "ArrowRight") { e.preventDefault(); tourGoto(tourStep + 1); return; }
  if (e.key === "Enter" && !e.target.closest("button")) {
    // A focused button already turns Enter into its own click; only steps in for
    // everything else (e.g. focus left on a step-5 switch).
    e.preventDefault();
    tourStep === TOUR_LAST_STEP ? tourFinish() : tourGoto(tourStep + 1);
  }
}

// The dialog must not visibly resize as the user pages through the five
// steps, but a single fixed number (the old tourStepMinH expert setting) is
// either too tall for the shortest step (an empty-looking box) or too short
// for the tallest (a jump on Next). Measuring every step once, right after the
// modal becomes visible (a hidden ancestor reports 0 for any height), and
// keeping the tallest is the actual fix -- there is nothing left to tune by
// hand, which is also why there is no expert setting for it.
function tourMeasureMaxStepHeight() {
  const body = $("tour-body");
  let max = 0;
  for (let i = 0; i < TOUR_STEP_COUNT; i++) {
    const wrap = i < TOUR_STEPS.length ? renderTourDrawingStep(TOUR_STEPS[i]) : renderTourOptionsStep();
    body.innerHTML = "";
    body.appendChild(wrap);
    max = Math.max(max, wrap.getBoundingClientRect().height);
  }
  return max;
}

async function openTour({ firstRun }) {
  tourFirstRun = firstRun;
  tourStep = 0;
  tourDraft = firstRun ? Object.fromEntries(TOUR_OPTIONS.map((o) => [o.key, shipsOn(o.key)])) : {};
  $("tour-modal").classList.remove("hidden");
  const maxH = tourMeasureMaxStepHeight();
  document.documentElement.style.setProperty("--tour-step-min-h", `${Math.ceil(maxH)}px`);
  renderTourStep();
  document.addEventListener("keydown", tourKeydown);
}

function closeTour() {
  $("tour-modal").classList.add("hidden");
  document.removeEventListener("keydown", tourKeydown);
}

async function tourSkip() {
  closeTour();
  await saveUiState("tourSeen", true); // Skip writes nothing else
}

// Escape and a stray background click are easy to trigger by accident, so both
// route here instead of skipping straight away; the top-right Skip button is a
// deliberate action and stays direct.
async function tourAskSkip() {
  const ok = await confirmDialog({
    title: t("tourSkipConfirmTitle"),
    message: t("tourSkipConfirmMsg"),
    confirmLabel: t("tourSkipConfirmOk"),
    cancelLabel: t("tourNext"),
    danger: false,
  });
  if (ok) await tourSkip();
}

// Only the switches that differ from the shipped default get written — an
// untouched draft leaves ui_flags exactly as it was, so a later default change
// still reaches this install.
async function tourFinishDraft() {
  for (const opt of TOUR_OPTIONS) {
    const shipped = shipsOn(opt.key);
    if (tourDraft[opt.key] !== shipped) await tourSetFlagLive(opt.key, tourDraft[opt.key]);
  }
}

async function tourFinish() {
  if (tourFirstRun) await tourFinishDraft();
  closeTour();
  await saveUiState("tourSeen", true);
}

EXPERT_ACTIONS["tour-open"] = () => openTour({ firstRun: false });

// The store stopped being sealed — told once per session, because it is a state,
// not an event stream: every following save fails the same way and a second dialog
// would only be in the way. The permanent record lives in the settings dialog's
// privacy group, which keeps saying so for as long as it is true; that is why this
// one has nothing to dismiss forever and no switch to silence it.
let storeUnsealedTold = false;
async function warnStoreUnsealed() {
  if (storeSealed || storeUnsealedTold) return false;
  storeUnsealedTold = true;
  await infoDialog(
    t("storeUnsealedTitle"),
    `${t("storeUnsealedMsg")}\n\n${t("storeUnsealedFix")}`,
    ICON_WARN
  );
  return true;
}

// Same "told once, state lives on in the privacy group" shape as
// warnStoreUnsealed above -- a stuck disk fails the same way on every
// following write, so a second dialog would only be in the way.
let settingsWriteFailedTold = false;
async function warnSettingsWriteFailed() {
  if (!settingsWriteFailed || settingsWriteFailedTold) return false;
  settingsWriteFailedTold = true;
  await infoDialog(t("settingsWriteFailedTitle"), t("settingsWriteFailedMsg"), ICON_WARN);
  return true;
}

// Called once, right after seedExamplesIfNeeded(), from init()'s reveal(). Runs
// AFTER offerTakeover() (the portable/installed data-folder adoption offer) so the
// takeover question is always seen first, but is not suppressed by it any more: an
// adopted store brings its own tourSeen along (settings.json is part of the copied
// files), so a returning user still gets no tour, while a declined offer means a
// genuinely new user, who should. The only dialog that still suppresses the tour
// outright is the unencrypted-store warning (sealWarned) — more urgent, and safe to
// retry next launch since tourSeen is only ever written by the tour itself. A
// missing-WebView2 message cannot race this at all: that one is native and shown,
// if ever, before the WebView2 runtime — and therefore this whole JS module —
// exists (see ensure_webview2 in lib.rs, the one allowed exception to routing every
// user-facing dialog through the themed confirmDialog()), so there is nothing left
// to check for it here by the time init() runs.
async function maybeOpenFirstRunTour(sealWarned) {
  if (sealWarned) return; // retried next launch — tourSeen was never written
  if (settings.ui_flags?.tourSeen) return;
  if (!flag("firstRunTour")) return;
  await openTour({ firstRun: true });
}

// ---- Init ----
// Re-read settings from the backend and apply the global, order-sensitive parts
// (palette, language, sizing, theme). Shared by init() and reloadEverything() so a
// new global settings value only needs adding here, not in both places.
async function applySettingsCore() {
  settings = await invoke("get_settings");
  loadPalette();
  LANG = pickLang(settings.language);
  applyI18n();
  fillSizeSelects();
  fillFontSelects();
  applyTheme(await invoke("current_theme").catch(() => settings.theme));
}

// Three onboarding examples — variable, image, Markdown. Text comes from i18n.js
// (LANG is already resolved by the time this runs, see applySettingsCore), so each
// install sees them in its own language instead of freezing whatever the OS locale
// was on first launch would have to.
const EXAMPLES = [
  { name: "example1Name", text: "example1Text", image: "" },
  { name: "example2Name", text: "example2Text", image: EXAMPLE_IMAGE_DATA_URL },
  { name: "example3Name", text: "example3Text", image: "" },
];

// The backend re-checks "library still empty AND never seeded before" itself, so
// this is safe (and cheap) to call unconditionally on every launch — it is a no-op
// the moment the library holds anything at all, examples included.
// Returns the prompts it actually seeded (empty when the library already had
// something, examples included — the backend decides, this only reports back).
async function seedExamplesIfNeeded() {
  const examples = EXAMPLES.map((e) => ({ name: t(e.name), text: t(e.text), image: e.image }));
  return invoke("seed_example_prompts", { examples }).catch(() => []);
}

async function updateExamplesBanner() {
  const banner = $("examples-banner");
  if (!banner) return;
  const ids = settings.example_prompt_ids || [];
  const stillHere = ids.length > 0 && ids.some((id) => prompts.some((p) => p.id === id));
  if (!stillHere) {
    banner.classList.add("hidden");
    return;
  }
  const pending = await invoke("example_prompts_pending").catch(() => ids.length);
  banner.classList.toggle("hidden", pending === 0);
  $("examples-dismiss").textContent = t("examplesDismiss").replace("{n}", String(pending));
}

// The rest of applying loaded settings — flags, toolbar, the grid itself and the
// settings-dialog controls. Split from applySettingsCore() because init() has to
// wire event handlers in between; reloadEverything() calls both back to back.
async function applySettingsUi() {
  applyFlags();
  applyBars();
  await renderGrid();
  syncSettingsControls();
}

async function init() {
  try {
    await applySettingsCore();
    bind();
    wireTooltips();
    wireWindowControls();
    wireClearButtons();
    wireLightbox();
    setupDragDrop();
    await applySettingsUi();
    invoke("app_version").then((v) => {
      appVersion = v;
      versionLabel = t("versionInstalled").replace("{v}", v);
      $("update-version").textContent = versionLabel;
      $("about-version").textContent = t("versionInstalled").replace("{v}", appVersion);
    }).catch(() => {});
    invoke("current_data_dir").then((d) => { currentDataDir = d || ""; }).catch(() => {});
    // Awaited, unlike the informational calls around it: the reveal below asks the
    // user about it straight away, so it has to be known before the window appears.
    storeSealed = (await invoke("store_sealed").catch(() => true)) !== false;
    refreshSettingsLocked();
    // Not awaited: only a write that failed before this window was even listening
    // (an early migration write, say) could already be true here -- everything
    // after boot arrives live through the "persist-failed" event instead. The
    // dialog itself is NOT opened from here -- it has to wait its turn in
    // reveal()'s chain below, same as warnStoreUnsealed, or it races the
    // takeover offer for the shared confirm-modal element.
    invoke("settings_write_failed").then((v) => {
      settingsWriteFailed = v === true;
      if (!$("settings-expert").classList.contains("hidden")) renderExpert();
    }).catch(() => {});
    invoke("default_screenshot_dir").then((d) => { defaultScreenshotDir = d || ""; }).catch(() => {});
    invoke("default_sync_dir").then((d) => { defaultSyncDir = d || ""; }).catch(() => {});
    invoke("default_recording_dir").then((d) => { defaultRecordingDir = d || ""; }).catch(() => {});
    // Whatever the startup sweep found (if anything) -- collected once,
    // here, since it ran before this window (or any of its listeners) existed.
    invoke("take_recording_sweep_notice").then((r) => {
      if (r && r[0] > 0) toast(t("recSweepDone").replace("{n}", String(r[0])).replace("{v}", fmtBytes(r[1])));
    }).catch(() => {});
    // OCR language list -- fetched once, cached, "Automatic" is always
    // prepended separately (see EXPERT_DROPDOWNS.ocrLang above).
    invoke("ocr_languages").then((langs) => { ocrLangsCache = langs || []; }).catch(() => {});
    // Microphone list for recMicDevice -- fetched once, cached, "Automatic"
    // (system default) is always prepended separately (see EXPERT_DROPDOWNS.recMicDevice above).
    invoke("list_mic_devices").then((devs) => { micDevicesCache = devs || []; }).catch(() => {});
    invoke("voice_languages").then((langs) => { voiceLangsCache = langs || []; }).catch(() => {});
    // Re-applies flags once this resolves so no-voice-engine reflects reality
    // (the optimistic default above assumes support until this says otherwise).
    invoke("voice_support").then((s) => { if (s) { voiceSupport = s; applyFlags(); } }).catch(() => {});
    applyTileStyle();
    autoGrow(inputEl);
    inputEl.focus();
    pollMissingFiles(); // this window's own first check - the ambient refresh below is a pushed event, not a timer
  } catch (e) {
    // Never strand the user with an invisible app: log, surface a toast if the
    // UI got far enough to have one, and still reveal the window below.
    console.error(e);
    try { toast(String((e && e.message) || e)); } catch (_) {}
  } finally {
    // Reveal only after the first fitted paint — the user never sees text sizing.
    // A hidden WebView2 window can stop producing frames, so a timer backs the
    // rAF chain up: showing late beats never showing at all. Whichever wins, the
    // reveal runs exactly once.
    let revealed = false;
    const reveal = () => {
      if (revealed) return;
      revealed = true;
      // Ask about adopting an installed store only once the themed window is up.
      // The first-run tour comes right after, but only if that offer (the one
      // other dialog that can fire this early) did not itself appear this launch.
      invoke("show_main_window").catch(() => {}).finally(async () => {
        await offerTakeover();
        // Seeded AFTER the takeover offer, not before: seeding earlier (e.g. from
        // applySettingsUi()) would make the library non-empty and takeover_offer()
        // would then never fire, hiding the adoption question behind the tour. Only
        // re-render if something actually landed — the common case (a library that
        // already has content) must not repaint the grid a second time on every
        // launch, which would invalidate tile references a user could already be
        // interacting with.
        const seeded = await seedExamplesIfNeeded();
        if (seeded && seeded.length) await renderGrid();
        // Ahead of the tour: an unencrypted store is the more urgent of the two,
        // and the tour is retried next launch (tourSeen is only written by it).
        const sealWarned = await warnStoreUnsealed();
        await warnSettingsWriteFailed();
        await maybeOpenFirstRunTour(sealWarned);
      });
    };
    setTimeout(reveal, 1500);
    requestAnimationFrame(() => {
      try { fitCache.clear(); fitAllTiles(); } catch (_) {}
      requestAnimationFrame(reveal);
    });
  }
}

// Push the loaded settings into the settings-dialog controls.
function syncSettingsControls() {
    fillThemeSelect();
    setLangSelect();
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
    $("opt-voice-hotkey").value = prettyAccel(settings.voice_hotkey);
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
  await refreshSettingsLocked(); // reset_settings/import_all may have just cleared the lock
  await applySettingsCore();
  await applySettingsUi(); // renderGrid() WITH the fetch: the prompts were replaced too
  // A wipe (delete_all_data) leaves the library empty, so the backend reseeds the
  // examples here; adopt/reset leave it non-empty, so this stays a no-op for them.
  const seeded = await seedExamplesIfNeeded();
  if (seeded && seeded.length) await renderGrid();
  wireClearButtons();
  fitCache.clear();
  fitAllTiles();
}

init();
