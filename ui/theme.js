// Themes and the custom theme editor.
import { $, assignCOLORS, assignSettings, CLEAR_SVG, closeColorPop, DEFAULT_COLORS, emit, errToast, flag, HEX_RE, invoke, libraryEl, openColorPop, PALETTE_LEN, settings, t, toast } from "./core.js";
import { themeColorScheme } from "./floatShared.js";
import { renderLibrary } from "./library.js";
import { isHistoryTabOpen, renderJournal } from "./journal.js";
import { FONTS } from "./fonts.js";
import { fitCache } from "./tiles.js";
import { confirmDialog } from "./dialog.js";
import { attachSelectPicker } from "./selectpop.js";

// ---- Helpers ----
// Custom themes: the colour tokens of the `:root` block in main.css that a
// custom theme may override. Everything else stays with the inherited base theme.
export const THEME_VARS = ["bg", "surface", "surface-2", "fg", "muted", "border", "accent", "accent-fg", "danger"];
// Editor layout: which tokens belong together, in the order they are shown.
export const THEME_VAR_GROUPS = [
  ["themeGroupBg", ["bg"]],
  ["themeGroupSurface", ["surface", "surface-2", "border"]],
  ["themeGroupText", ["fg", "muted"]],
  ["themeGroupAccent", ["accent", "accent-fg", "danger"]],
];
// The built-in themes a custom theme can inherit from ("system" is not one of
// them: it has no rules of its own). Order matches the settings dropdown.
export const BUILTIN_THEMES = ["light", "dark", "programmer", "neon", "cyberpunk", "ocean"];

export const customThemes = () => (flag("customThemes") ? settings.custom_themes || [] : []);
export const customThemeId = (pref) => (String(pref || "").startsWith("custom:") ? String(pref).slice(7) : "");
export const findCustomTheme = (pref) => customThemes().find((c) => c.id === customThemeId(pref)) || null;

// Overrides of the active theme, straight from the local settings: the backend
// sends the same set with every theme-changed event.
export function activeThemeVars() {
  return findCustomTheme(settings.theme)?.vars || {};
}

// `theme` is always a built-in key (a custom theme resolves to its base first);
// `vars` are the colour tokens layered on top as inline styles.
export function applyTheme(theme, vars) {
  if (theme) document.documentElement.setAttribute("data-theme", theme);
  // Lets a native control we do not style ourselves -- the video element's own
  // three-dot menu -- follow dark/light instead of always drawing light.
  document.documentElement.style.colorScheme = themeColorScheme(theme);
  applyThemeVars(vars ?? activeThemeVars());
}

// Inline overrides are the whole custom-theme mechanism, so always clear the full
// set first: switching to a built-in theme must leave nothing behind.
export function applyThemeVars(vars) {
  const style = document.documentElement.style;
  for (const key of THEME_VARS) style.removeProperty(`--${key}`);
  style.removeProperty("--shadow");
  style.removeProperty("--select-chevron");
  for (const [key, value] of Object.entries(vars || {})) {
    if (THEME_VARS.includes(key) && HEX_RE.test(value)) style.setProperty(`--${key}`, value);
  }
  // The window glow is the base theme's, so a custom accent would sit in a shadow of the
  // wrong colour and not follow an edit; derive it.
  const accent = vars?.accent;
  if (HEX_RE.test(accent || "")) {
    style.setProperty("--shadow", `0 8px 28px color-mix(in srgb, ${accent} 32%, transparent)`);
  }
  // Same reasoning as --shadow: a data-URI SVG cannot hold a var(), so a custom theme's
  // muted colour needs its own baked chevron or every select keeps the base theme's arrow
  // colour.
  const muted = vars?.muted;
  if (HEX_RE.test(muted || "")) {
    style.setProperty("--select-chevron", selectChevronVar(muted));
  }
}

// Built-in themes that no longer exist, and the one each renders as now (the removed
// Black & White theme was a light one). The backend rewrites a stored value on load;
// this keeps a not yet migrated copy from falling through to "system".
const RETIRED_BASES = { mono: "light" };

// Which built-in theme a preference renders as. A custom theme keeps its base even
// while the feature is switched off, so the dropdown still shows something truthful.
export function themeBase(pref) {
  const id = customThemeId(pref);
  if (!id) return pref;
  const raw = (settings.custom_themes || []).find((c) => c.id === id);
  const base = raw && (RETIRED_BASES[raw.base] || raw.base);
  return BUILTIN_THEMES.includes(base) ? base : "system";
}

// The one JS-side copy of the data-URI SVG every built-in theme bakes into main.css (with
// that theme's --muted colour substituted, since a data URI cannot hold a var()); used to
// derive --select-chevron for a custom theme's runtime override. Keep in sync with the
// CSS copy.
export const selectChevronVar = (hex) =>
  `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%23${hex.slice(1)}' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M6 9l6 6 6-6'/%3E%3C/svg%3E")`;

// Load the custom palette from settings (ui_texts.palette = JSON hex array), or
// fall back to defaults. Invalid/partial data is ignored so colors never break.
export function loadPalette() {
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
  assignCOLORS(custom ? ["", ...custom] : [...DEFAULT_COLORS]);
}

// Refresh any open list after a palette change.
export function refreshColorUI() {
  if (!libraryEl.classList.contains("hidden")) renderLibrary();
  if (isHistoryTabOpen()) renderJournal();
}

export function applyTileStyle() {
  const root = document.documentElement.style;
  root.setProperty("--tile-font", FONTS[settings.tile_font] || FONTS.system);
  // tile_size 0 = auto-fit (per-tile, handled after every grid render).
  root.setProperty("--tile-size", `${settings.tile_size || 15}px`);
  fitCache.clear(); // font metrics changed -> cached fit sizes are stale
}

// ---- Custom theme editor ----
export const THEME_LABELS = {
  light: "themeLight", dark: "themeDark", programmer: "themeProgrammer", neon: "themeNeon",
  cyberpunk: "themeCyberpunk", ocean: "themeOcean",
};
export const THEME_VAR_LABELS = {
  bg: "themeVarBg", surface: "themeVarSurface", "surface-2": "themeVarSurface2",
  fg: "themeVarFg", muted: "themeVarMuted", border: "themeVarBorder",
  accent: "themeVarAccent", "accent-fg": "themeVarAccentFg", danger: "themeVarDanger",
};

// The theme tokens are authored as hex or rgba(), and a color picker needs hex.
// Alpha is dropped: only a value the user actually overrides is ever stored.
export function cssColorToHex(value) {
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
// probe runs on the document element itself and restores its state right away:
// no frame is painted in between, so nothing flickers.
export function baseThemeColors(base) {
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
export let themeDraft = null;
export let themeRestore = null;
// The draft as it looked when the editor opened, so "Revert changes" undoes this
// session's edits without closing the editor. Separate from themeRestore, the real
// window's pre-edit theme (used only when the whole editor is cancelled).
export let themeDraftStart = null;

export function newThemeDraft() {
  const current = document.documentElement.getAttribute("data-theme");
  return {
    id: "",
    name: t("themeNewName"),
    base: BUILTIN_THEMES.includes(current) ? current : "light",
    vars: {},
  };
}

export function draftFrom(theme) {
  return { id: theme.id, name: theme.name, base: theme.base, vars: { ...(theme.vars || {}) } };
}

// Live preview. Writing the variables costs a style recalc the browser coalesces into the
// next frame; there is no grid rebuild. The broadcast to the pill and folder windows
// (each repaints on it) is coalesced to one per frame, so dragging a colour stays smooth.
export function previewThemeDraft() {
  if (!themeDraft) return;
  document.documentElement.setAttribute("data-theme", themeDraft.base);
  document.documentElement.style.colorScheme = themeColorScheme(themeDraft.base);
  applyThemeVars(themeDraft.vars);
  // The pills and folders only hear the backend's theme-changed event, which fires on a
  // real switch; without this broadcast the preview would stop at the main window's edge.
  broadcastThemeSoon(themeDraft.base, themeDraft.vars);
}

let themeBroadcastFrame = 0;
let themeBroadcastPending = null;
function broadcastThemeSoon(theme, vars) {
  themeBroadcastPending = { theme, vars: { ...vars } };
  if (themeBroadcastFrame) return;
  themeBroadcastFrame = requestAnimationFrame(() => {
    themeBroadcastFrame = 0;
    const pending = themeBroadcastPending;
    themeBroadcastPending = null;
    if (pending) broadcastTheme(pending.theme, pending.vars);
  });
}
function cancelThemeBroadcast() {
  if (themeBroadcastFrame) cancelAnimationFrame(themeBroadcastFrame);
  themeBroadcastFrame = 0;
  themeBroadcastPending = null;
}

// Same payload shape the backend emits, so the small windows need no new listener.
export function broadcastTheme(theme, vars) {
  emit("theme-changed", { theme, vars: vars || {} }).catch(() => {});
}

export function openThemeEditor() {
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

export function closeThemeEditor(revert = true) {
  $("theme-modal").classList.add("hidden");
  closeColorPop();
  if (revert && themeRestore) {
    applyTheme(themeRestore.theme, themeRestore.vars);
    cancelThemeBroadcast(); // a queued preview frame must not land after the revert
    broadcastTheme(themeRestore.theme, themeRestore.vars); // undo the preview in the pills too
  }
  themeDraft = null;
  themeRestore = null;
  themeDraftStart = null;
}

// Undo every change made since the editor opened, without closing it.
export function revertThemeDraft() {
  if (!themeDraftStart) return;
  themeDraft = draftFrom(themeDraftStart);
  renderThemeEditor();
  previewThemeDraft();
}

export function renderThemeEditor() {
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

export function renderThemeVars() {
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

export function themeVarRow(key, fallback) {
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

// True while a save is in flight: a second click on Save must not create the theme twice
// (a new theme's draft id stays empty until the editor closes).
let savingTheme = false;

export async function saveThemeDraft() {
  if (!themeDraft || savingTheme) return;
  const name = $("theme-edit-name").value.trim();
  if (!name) {
    toast(t("themeNameRequired"));
    $("theme-edit-name").focus();
    return;
  }
  themeDraft.name = name;
  // Everything the rest of this function needs is read now: the editor can be
  // closed (Cancel, Escape) while the backend call runs, which clears the draft.
  const draftId = themeDraft.id;
  const { base, vars } = themeDraft;
  const themeSaveEl = $("theme-edit-save");
  savingTheme = true;
  if (themeSaveEl) themeSaveEl.disabled = true;
  try {
    try {
      assignSettings(await invoke("save_custom_theme", { id: draftId, name, base, vars }));
    } catch (err) {
      errToast(err);
      return;
    }
    // A new theme is created active by the backend; editing an existing one switches
    // to it, so the live preview simply carries on instead of snapping away.
    const id = draftId || customThemeId(settings.theme);
    const pref = `custom:${id}`;
    if (settings.theme !== pref) {
      settings.theme = pref;
      await invoke("set_theme", { theme: pref }).catch((err) => errToast(err));
    }
    fillThemeSelect();
    applyTheme(themeBase(pref));
    if (themeDraft) closeThemeEditor(false);
  } finally {
    savingTheme = false;
    if (themeSaveEl) themeSaveEl.disabled = false;
  }
}

export async function deleteThemeDraft() {
  if (!themeDraft?.id) return;
  const ok = await confirmDialog({
    title: t("themeDeleteTitle"),
    message: t("themeDeleteMsg").replace("{name}", () => themeDraft.name),
    confirmLabel: t("delete"),
  });
  if (!ok) return;
  try {
    assignSettings(await invoke("delete_custom_theme", { id: themeDraft.id }));
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

// Value of the "Custom theme..." entry at the end of the theme list.
export const THEME_EDITOR_ENTRY = "__theme-editor";

// Settings dropdown: the built-in themes, then the custom ones under a separator.
export function fillThemeSelect() {
  const sel = $("theme-select");
  if (!sel) return;
  for (const opt of [...sel.querySelectorAll("option.custom-theme")]) opt.remove();
  const list = customThemes();
  const editorEntry = flag("customThemes");
  if (list.length || editorEntry) {
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
  // The way into the theme editor is the last entry of the list; it is never the
  // selected value (the settings change handler opens the editor and puts the
  // current theme back).
  if (editorEntry) {
    const opt = document.createElement("option");
    opt.className = "custom-theme theme-editor-entry";
    opt.value = THEME_EDITOR_ENTRY;
    opt.textContent = t("themeEditorEntry");
    sel.appendChild(opt);
  }
  sel.value = settings.theme || "system";
  // A parked custom theme (feature off) has no entry: show the base it renders as.
  if (!sel.value) sel.value = themeBase(settings.theme);
  // A theme that was dropped from the product leaves the select with no match at
  // all; fall back to the automatic one rather than showing an empty picker.
  if (!sel.value) sel.value = "system";
}

// Re-apply after the feature was toggled: switching it off parks every custom theme
// on its base theme without touching the stored data.
export function refreshCustomThemes() {
  fillThemeSelect();
  const base = themeBase(settings.theme);
  if (customThemeId(settings.theme) && BUILTIN_THEMES.includes(base)) {
    document.documentElement.setAttribute("data-theme", base);
  }
  applyThemeVars(activeThemeVars());
}

export function bindTheme() {
  attachSelectPicker($("theme-edit-target"));
  attachSelectPicker($("theme-edit-base"));
}

export function wireTheme() {
  $("theme-modal-close")?.addEventListener("click", () => closeThemeEditor());
  $("theme-edit-cancel")?.addEventListener("click", () => closeThemeEditor());
  $("theme-edit-revert")?.addEventListener("click", revertThemeDraft);
  // Export the currently selected SAVED theme (an unsaved draft has no id to
  // look up on the backend, hence the button hides for one: see renderThemeEditor).
  $("theme-edit-export")?.addEventListener("click", async () => {
    if (!themeDraft?.id) return;
    try {
      await invoke("export_custom_theme", { id: themeDraft.id });
    } catch (err) {
      if (err !== "canceled") errToast(err);
    }
  });
  // Import creates a saved theme on the backend right away (same validation as typing
  // values by hand), then opens it in the editor for tweaks or a plain Cancel/Save.
  $("theme-edit-import")?.addEventListener("click", async () => {
    try {
      assignSettings(await invoke("import_custom_theme"));
    } catch (err) {
      if (err !== "canceled") errToast(err);
      return;
    }
    fillThemeSelect();
    const imported = findCustomTheme(settings.theme);
    // The backend made the imported theme the active one: that is now the state
    // Cancel returns to, not the theme the editor was opened on.
    if (imported) themeRestore = { theme: imported.base, vars: { ...(imported.vars || {}) } };
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
    // "Revert changes" goes back to the theme being edited now, not to the one the
    // editor was opened on.
    themeDraftStart = draftFrom(themeDraft);
    renderThemeEditor();
    previewThemeDraft();
  });
  $("theme-edit-base")?.addEventListener("change", (e) => {
    if (!themeDraft) return;
    themeDraft.base = e.target.value;
    renderThemeVars(); // the un-overridden colours now come from another theme
    previewThemeDraft();
  });
}
