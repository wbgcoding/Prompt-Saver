// Single source of truth for every app-defined keyboard shortcut. A handler
// that reacts to one of these keys reads it from here (or is at least checked
// against it by tests/shortcut_check.py); a tooltip that mentions one gets the
// text through withShortcut() below instead of spelling the key itself.
//
// A key is described in KeyboardEvent.code form ("KeyZ", "Digit1", "Enter"). For a letter
// that is the letter printed on the key, not its physical position: a German keyboard has
// Z where a US one has Y, and Ctrl+Z must still undo there. Handlers match letters through
// letterCode() below for that reason. Modifier names are never translated except "Ctrl",
// which becomes "Strg" in German.
//
// scopes: "snipEditor" (the screenshot editor overlay in the main window),
// "videoEditor" (the video editor window), "mainWindow" (the main window
// itself, outside either editor) and "library" (reserved for a library-only
// shortcut, none exist yet). The scopes "launcher", "tray", "vars", "floating",
// "snipOverlay", "recBar" and "camera" are list-only: their keys are handled by
// each window's own key listener and are documented here for the overview.

export const SHORTCUTS = [
  // ---- main window ----
  { action: "mainCreate", code: "Enter", mods: ["ctrl"], scopes: ["mainWindow"], descKey: "save" },
  { action: "mainGlobalSearch", code: "KeyK", mods: ["ctrl"], scopes: ["mainWindow"], descKey: "globalSearchBtn" },
  { action: "mainNextView", code: "Tab", mods: ["ctrl"], scopes: ["mainWindow"], descKey: "shortcutNextView" },
  { action: "mainPrevView", code: "Tab", mods: ["ctrl", "shift"], scopes: ["mainWindow"], descKey: "shortcutPrevView" },

  // ---- screenshot editor ----
  { action: "snipUndo", code: "KeyZ", mods: ["ctrl"], scopes: ["snipEditor"], descKey: "snipEditUndoTip" },
  { action: "snipRedo", code: "KeyY", mods: ["ctrl"], scopes: ["snipEditor"], descKey: "snipEditRedoTip" },
  { action: "snipRedoAlt", code: "KeyZ", mods: ["ctrl", "shift"], scopes: ["snipEditor"], descKey: "snipEditRedoTip" },
  { action: "snipZoomIn", code: "Equal", mods: ["ctrl"], scopes: ["snipEditor"], descKey: "snipEditZoomIn" },
  { action: "snipZoomOut", code: "Minus", mods: ["ctrl"], scopes: ["snipEditor"], descKey: "snipEditZoomOut" },
  { action: "snipZoomReset", code: "Digit0", mods: ["ctrl"], scopes: ["snipEditor"], descKey: "snipEditFitTip" },
  { action: "snipApply", code: "Enter", mods: [], scopes: ["snipEditor"], descKey: "snipEditCloseTip" },
  { action: "snipCancel", code: "Escape", mods: [], scopes: ["snipEditor"], descKey: "snipEditCloseTip" },
  { action: "snipEditText", code: "F2", mods: [], scopes: ["snipEditor"], descKey: "snipEditTextAgain" },
  { action: "snipShowOriginal", code: "KeyV", mods: [], scopes: ["snipEditor"], descKey: "showOriginalTip" },
  { action: "snipTool.crop", code: "Digit1", mods: [], scopes: ["snipEditor"], descKey: "snipToolCrop" },
  { action: "snipTool.pen", code: "Digit2", mods: [], scopes: ["snipEditor"], descKey: "snipToolPen" },
  { action: "snipTool.marker", code: "Digit3", mods: [], scopes: ["snipEditor"], descKey: "snipToolMarker" },
  { action: "snipTool.line", code: "Digit4", mods: [], scopes: ["snipEditor"], descKey: "snipToolLine" },
  { action: "snipTool.arrow", code: "Digit5", mods: [], scopes: ["snipEditor"], descKey: "snipToolArrow" },
  { action: "snipTool.rect", code: "Digit6", mods: [], scopes: ["snipEditor"], descKey: "snipToolRect" },
  { action: "snipTool.ellipse", code: "Digit7", mods: [], scopes: ["snipEditor"], descKey: "snipToolEllipse" },
  { action: "snipTool.text", code: "Digit8", mods: [], scopes: ["snipEditor"], descKey: "snipToolText" },
  { action: "snipTool.step", code: "Digit9", mods: [], scopes: ["snipEditor"], descKey: "snipToolStep" },
  { action: "snipTool.pixelate", code: "Digit0", mods: [], scopes: ["snipEditor"], descKey: "snipToolPixelate" },
  { action: "snipTool.spotlight", code: "Minus", mods: [], scopes: ["snipEditor"], descKey: "snipToolSpotlight" },
  { action: "snipTool.hand", code: "KeyH", mods: [], scopes: ["snipEditor"], descKey: "snipToolHand" },

  // ---- video editor ----
  { action: "veUndo", code: "KeyZ", mods: ["ctrl"], scopes: ["videoEditor"], descKey: "veUndoTip" },
  { action: "veRedo", code: "KeyY", mods: ["ctrl"], scopes: ["videoEditor"], descKey: "veRedoTip" },
  { action: "veRedoAlt", code: "KeyZ", mods: ["ctrl", "shift"], scopes: ["videoEditor"], descKey: "veRedoTip" },
  { action: "vePreviewZoomIn", code: "Equal", mods: ["ctrl"], scopes: ["videoEditor"], descKey: "vePreviewZoomIn" },
  { action: "vePreviewZoomOut", code: "Minus", mods: ["ctrl"], scopes: ["videoEditor"], descKey: "vePreviewZoomOut" },
  { action: "vePreviewZoomReset", code: "Digit0", mods: ["ctrl"], scopes: ["videoEditor"], descKey: "veResetZoom" },
  { action: "veTimelineZoomIn", code: "Equal", mods: [], scopes: ["videoEditor"], descKey: "veZoomIn" },
  { action: "veTimelineZoomOut", code: "Minus", mods: [], scopes: ["videoEditor"], descKey: "veZoomOut" },
  { action: "vePlayPause", code: "Space", mods: [], scopes: ["videoEditor"], descKey: "vePlay" },
  { action: "veStepPrev", code: "ArrowLeft", mods: [], scopes: ["videoEditor"], descKey: "vePrevFrame" },
  { action: "veStepNext", code: "ArrowRight", mods: [], scopes: ["videoEditor"], descKey: "veNextFrame" },
  { action: "veSeekStart", code: "Home", mods: [], scopes: ["videoEditor"], descKey: "veToStart" },
  { action: "veSeekEnd", code: "End", mods: [], scopes: ["videoEditor"], descKey: "veToEnd" },
  { action: "veSeekStartAlt", code: "Home", mods: ["shift"], scopes: ["videoEditor"], descKey: "veToSourceStart" },
  { action: "veSeekEndAlt", code: "End", mods: ["shift"], scopes: ["videoEditor"], descKey: "veToSourceEnd" },
  { action: "veMarkOut", code: "KeyO", mods: [], scopes: ["videoEditor"], descKey: "veMarkOutTip" },
  { action: "veMarkIn", code: "KeyI", mods: [], scopes: ["videoEditor"], descKey: "veMarkInTip" },
  { action: "veCutSel", code: "KeyC", mods: [], scopes: ["videoEditor"], descKey: "veCutSelTip" },
  { action: "veShowOriginal", code: "KeyV", mods: [], scopes: ["videoEditor"], descKey: "showOriginalTip" },
  { action: "veHelp", code: "F1", mods: [], scopes: ["videoEditor"], descKey: "veHelpOpen" },

  // ---- list-only: windows that handle their own keys ----
  { action: "launcherClose", code: "Escape", mods: [], scopes: ["launcher"], descKey: "close" },
  { action: "launcherCopy", code: "Enter", mods: [], scopes: ["launcher"], descKey: "shortcutCopySelected" },
  { action: "launcherNext", code: "ArrowDown", mods: [], scopes: ["launcher"], descKey: "shortcutNextEntry" },
  { action: "launcherPrev", code: "ArrowUp", mods: [], scopes: ["launcher"], descKey: "shortcutPrevEntry" },
  { action: "trayClose", code: "Escape", mods: [], scopes: ["tray"], descKey: "close" },
  { action: "trayCopy", code: "Enter", mods: [], scopes: ["tray"], descKey: "shortcutCopySelected" },
  { action: "trayNext", code: "ArrowDown", mods: [], scopes: ["tray"], descKey: "shortcutNextEntry" },
  { action: "trayPrev", code: "ArrowUp", mods: [], scopes: ["tray"], descKey: "shortcutPrevEntry" },
  { action: "varsCancel", code: "Escape", mods: [], scopes: ["vars"], descKey: "cancel" },
  { action: "varsConfirm", code: "Enter", mods: [], scopes: ["vars"], descKey: "varsCopy" },
  { action: "floatingCloseMenu", code: "Escape", mods: [], scopes: ["floating"], descKey: "shortcutCloseMenu" },
  { action: "floatingNudge", code: "Arrows", mods: [], scopes: ["floating"], descKey: "shortcutNudgeButton" },
  { action: "snipOverlayCancel", code: "Escape", mods: [], scopes: ["snipOverlay"], descKey: "cancel" },
  { action: "snipOverlayPick", code: "Enter", mods: [], scopes: ["snipOverlay"], descKey: "shortcutCaptureWindow" },
  { action: "recBarPauseKey", code: "Space", mods: [], scopes: ["recBar"], descKey: "shortcutPauseResume" },
  { action: "recBarStopKey", code: "Escape", mods: [], scopes: ["recBar"], descKey: "shortcutStopRecording" },
  { action: "cameraPauseKey", code: "Space", mods: [], scopes: ["camera"], descKey: "shortcutPauseResume" },
  { action: "cameraEscKey", code: "Escape", mods: [], scopes: ["camera"], descKey: "shortcutCameraEsc" },
];

// The "KeyX" code of the letter a key event types: e.key when that is a Latin letter (the
// layout's own letter, so Ctrl+Z is the key labelled Z), else the physical e.code, so the
// shortcuts still work on a Cyrillic or Greek layout.
export function letterCode(e) {
  const k = e.key;
  if (typeof k === "string" && k.length === 1 && /[a-z]/i.test(k)) return "Key" + k.toUpperCase();
  return e.code;
}

// KeyboardEvent.code -> the character shown to the user. Only the codes this
// app actually binds are listed; an unlisted code falls back to itself.
// "Arrows" is not a real code: it is a display-only entry standing for any of
// the four arrow keys.
const CODE_LABELS = {
  Equal: "+", Minus: "-", Comma: ",", Period: ".", Space: "Space",
  Enter: "Enter", Escape: "Esc", Tab: "Tab",
  ArrowLeft: "Left", ArrowRight: "Right", ArrowUp: "Up", ArrowDown: "Down",
  Home: "Home", End: "End",
  Arrows: "↑↓←→",
};
function codeLabel(code) {
  if (CODE_LABELS[code]) return CODE_LABELS[code];
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  return code;
}

const MOD_ORDER = ["ctrl", "shift", "alt"];
function modLabel(mod, lang) {
  if (mod === "ctrl") return lang === "de" ? "Strg" : "Ctrl";
  if (mod === "shift") return "Shift";
  if (mod === "alt") return "Alt";
  return mod;
}

// "Ctrl+Shift+Z" / "Strg+Z" / "V": the key half of the tooltip, never
// translated except the "Strg" abbreviation above.
export function formatKeys(entry, lang) {
  const mods = MOD_ORDER.filter((m) => (entry.mods || []).includes(m)).map((m) => modLabel(m, lang));
  return [...mods, codeLabel(entry.code)].join("+");
}

function findEntry(action) {
  return SHORTCUTS.find((s) => s.action === action);
}

// The key combo of one action as text ("Ctrl+Enter", "Strg+Enter"), for a
// sentence that names the shortcut instead of a tooltip.
export function keysLabel(action, lang) {
  const entry = findEntry(action);
  return entry ? formatKeys(entry, lang) : "";
}

// A tooltip already ending in "(<something>)" (optionally followed by "." or "。") carries
// a shortcut hint baked in by hand in a translation; strip it before appending the
// computed one so the two never stack and re-applying withShortcut (e.g. on a language
// switch) stays idempotent.
const TRAILING_HINT = /\s*\([^()]*\)[.。]?\s*$/;
function stripTrailingHint(text) {
  return text.replace(TRAILING_HINT, "");
}

// Appends this action's key combo to el's tooltip (title); aria-label is left alone. `t`
// is the caller's translator and `lang` its UI language, since shortcuts.js has no i18n
// state of its own.
export function withShortcut(el, action, t, lang) {
  const entry = findEntry(action);
  if (!entry || !el) return el;
  const base = stripTrailingHint(t(entry.descKey));
  el.title = `${base} (${formatKeys(entry, lang)})`;
  el.dataset.shortcutAction = action;
  return el;
}

// Entries whose overview row names another way in as well: the row shows this text instead of the
// key alone.
const KEYS_TEXT = { snipEditText: "snipEditTextAgainKeys" };

// Rows for a "?" overview table, one row per distinct key combo (an action
// with an alt binding, e.g. veRedoAlt, is folded into its primary row's
// description rather than listed twice). `scope` is one scope name or an
// array of them: the main window's overview shows mainWindow + library
// together, since a user there has no reason to tell those two apart.
export function rowsForScope(scope, t, lang) {
  const scopes = Array.isArray(scope) ? scope : [scope];
  const seen = new Set();
  const rows = [];
  for (const entry of SHORTCUTS) {
    if (!entry.scopes.some((s) => scopes.includes(s))) continue;
    if (entry.action.endsWith("Alt")) continue; // shown as part of the primary action's row
    if (seen.has(entry.descKey)) continue;
    seen.add(entry.descKey);
    const keys = KEYS_TEXT[entry.action] ? t(KEYS_TEXT[entry.action]) : formatKeys(entry, lang);
    rows.push({ desc: stripTrailingHint(t(entry.descKey)), keys });
  }
  return rows;
}

// Fills an overview list with one row per { desc, keys } entry.
export function buildShortcutRows(container, rows) {
  for (const row of rows) {
    const r = document.createElement("div");
    r.className = "shortcuts-row";
    const desc = document.createElement("span");
    desc.className = "shortcuts-row-desc";
    desc.textContent = row.desc;
    const keys = document.createElement("span");
    keys.className = "shortcuts-row-keys";
    keys.textContent = row.keys;
    r.append(desc, keys);
    container.appendChild(r);
  }
}

// Shows and hides the overview dialog. Closing puts the focus back on the control that
// opened it; when that one is gone by then (a row removed by its own click) the body
// takes it, so the page never ends up with no focus owner at all.
let shortcutsOverlayOpener = null;
export function openShortcutsOverlay(modal) {
  shortcutsOverlayOpener = document.activeElement;
  modal.classList.remove("hidden");
}
export function closeShortcutsOverlay(modal) {
  modal.classList.add("hidden");
  const target = (shortcutsOverlayOpener?.isConnected && shortcutsOverlayOpener) || document.body;
  if (typeof target.focus === "function") target.focus();
  shortcutsOverlayOpener = null;
}

// Which heading each list-only window gets in the all-windows overview.
export const SCOPE_TITLE_KEYS = {
  mainWindow: "shortcutsScopeMain",
  launcher: "shortcutsScopeLauncher",
  tray: "shortcutsScopeTray",
  vars: "shortcutsScopeVars",
  floating: "shortcutsScopeFloating",
  snipOverlay: "shortcutsScopeSnip",
  recBar: "shortcutsScopeRecBar",
  camera: "shortcutsScopeCamera",
};

// One block of rows per scope, in the order given, so two windows that both
// have a "Close" row each keep their own (de-duplication is per scope only).
export function sectionsForScopes(scopes, t, lang) {
  return scopes.map((scope) => ({ scope, rows: rowsForScope(scope, t, lang) }));
}
