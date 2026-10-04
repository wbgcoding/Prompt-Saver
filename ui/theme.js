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
export const BUILTIN_THEMES = ["light", "dark", "programmer", "neon", "cyberpunk", "mono", "ocean"];

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
export function themeBase(pref) {
  const id = customThemeId(pref);
  if (!id) return pref;
  const raw = (settings.custom_themes || []).find((c) => c.id === id);
  return raw && BUILTIN_THEMES.includes(raw.base) ? raw.base : "system";
}

// Same data-URI SVG every built-in theme bakes its own copy of in main.css
// (each with that theme's --muted colour already substituted for the stroke,
// since a data URI cannot hold a var()) -- this is the one JS-side copy, used
// only to derive --select-chevron for a custom theme's runtime override.
// Keep in sync with the CSS copy if the SVG itself ever changes.
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

// Drop the filter dots (built again by the next render) after a palette change, and
// refresh any open list.
export function refreshColorUI() {
  for (const wrap of [$("library-colors"), $("journal-colors")]) {
    if (wrap) { wrap.innerHTML = ""; delete wrap.dataset.sig; }
  }
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
  cyberpunk: "themeCyberpunk", mono: "themeMono", ocean: "themeOcean",
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
// The draft as it looked the moment the editor opened, so "Revert changes"
// can undo everything done in THIS session without closing the editor: a
// separate snapshot from themeRestore, which is the real window's pre-edit
// theme (used only when the whole editor is cancelled).
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

// Live preview. Writing the variables costs a style recalc, which the browser
// already coalesces into the next frame: no grid rebuild, no IPC, so dragging a
// colour stays smooth without an extra throttle in front of it.
export function previewThemeDraft() {
  if (!themeDraft) return;
  document.documentElement.setAttribute("data-theme", themeDraft.base);
  applyThemeVars(themeDraft.vars);
  // The pills and folders are their own windows and only ever hear the backend's
  // theme-changed event, which fires on a real switch: without this the preview
  // would stop at the main window's edge while they kept the old colours.
  broadcastTheme(themeDraft.base, themeDraft.vars);
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
    broadcastTheme(themeRestore.theme, themeRestore.vars); // undo the preview in the pills too
  }
  themeDraft = null;
  themeRestore = null;
  themeDraftStart = null;
}

// D75 part 2: undo every change made since the editor opened, without closing it.
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

export async function saveThemeDraft() {
  if (!themeDraft) return;
  const name = $("theme-edit-name").value.trim();
  if (!name) {
    toast(t("themeNameRequired"));
    $("theme-edit-name").focus();
    return;
  }
  themeDraft.name = name;
  try {
    assignSettings(await invoke("save_custom_theme", {
      id: themeDraft.id,
      name,
      base: themeDraft.base,
      vars: themeDraft.vars,
    }));
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

export async function deleteThemeDraft() {
  if (!themeDraft?.id) return;
  const ok = await confirmDialog({
    title: t("themeDeleteTitle"),
    message: t("themeDeleteMsg").replace("{name}", themeDraft.name),
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

// Settings dropdown: the built-in themes, then the custom ones under a separator.
export function fillThemeSelect() {
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
  $("theme-edit-open")?.addEventListener("click", openThemeEditor);
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
  // Import creates a new saved theme on the backend right away (the same validation
  // as typing values in by hand), then opens it in the editor so it can be tweaked
  // or just confirmed with Cancel/Save.
  $("theme-edit-import")?.addEventListener("click", async () => {
    try {
      assignSettings(await invoke("import_custom_theme"));
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
}
