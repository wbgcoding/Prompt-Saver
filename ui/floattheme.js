// Theme mapping for the small always-on-top windows (pills and pill folders).
// They carry their OWN tokens (floating.css), so a custom theme's colours are mapped
// onto them — a floating button follows the same palette as the main window instead
// of falling back to the base theme's colours.
import { themeColorScheme } from "./floatShared.js";

const FLOAT_THEME_MAP = {
  accent: "--pill-bg",
  "accent-fg": "--pill-fg",
  surface: "--menu-bg",
  fg: "--menu-fg",
  border: "--menu-line",
  danger: "--danger",
};
const FLOAT_HEX = /^#[0-9a-fA-F]{6}$/;

// The ink rule (expert menu: "Pick the text colour on coloured areas" /
// "Black text from this brightness"), kept fresh from two sources: the
// theme-changed event's payload and one ink_prefs() fetch at module load
// (covers the window before the first event arrives). Defaults match the
// backend's own shipped default.
let INK = { auto: true, threshold: 60 };
let lastTheme = null;
let lastVars = null;
let applied = false; // true once applyFloatTheme has run at least once

// Relative luminance -> the ink that stays readable on `hex`. Duplicated from
// main.js's own readableInk rather than shared: this module ships to every
// satellite window, none of which load main.js.
function readableInk(hex) {
  if (!INK.auto) return "#fff";
  const n = parseInt(hex.slice(1), 16);
  const lin = (c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  const l = 0.2126 * lin(((n >> 16) & 255) / 255) + 0.7152 * lin(((n >> 8) & 255) / 255) + 0.0722 * lin((n & 255) / 255);
  return l > INK.threshold / 100 ? "#000" : "#fff";
}

export function applyFloatTheme(theme, vars) {
  lastTheme = theme;
  lastVars = vars;
  applied = true;
  if (theme) document.documentElement.setAttribute("data-theme", theme);
  document.documentElement.style.colorScheme = themeColorScheme(theme);
  const style = document.documentElement.style;
  for (const target of Object.values(FLOAT_THEME_MAP)) style.removeProperty(target);
  for (const [key, value] of Object.entries(vars || {})) {
    const target = FLOAT_THEME_MAP[key];
    if (target && FLOAT_HEX.test(value)) style.setProperty(target, value);
  }
  // The pill's own text sits directly on --pill-bg, so its ink is derived
  // from that colour instead of trusting the backend's accent-fg: a theme's
  // accent can be light enough that fixed white text no longer clears
  // WCAG's 4.5:1 minimum.
  const pillBg = style.getPropertyValue("--pill-bg").trim();
  if (FLOAT_HEX.test(pillBg)) style.setProperty("--pill-fg", readableInk(pillBg));
}

// Keep INK current without waiting for the next theme change, and re-paint
// the pill ink immediately once it arrives -- a satellite window must never
// need a restart to pick up a changed threshold or a flipped switch.
function setInk(ink) {
  if (!ink) return;
  INK = { auto: ink.auto !== false, threshold: Number.isFinite(ink.threshold) ? ink.threshold : 60 };
  if (applied) applyFloatTheme(lastTheme, lastVars);
}
window.__TAURI__.event.listen("theme-changed", (e) => setInk(e.payload?.ink)).catch(() => {});
const fetchInk = () => window.__TAURI__.core.invoke("ink_prefs").then(setInk).catch(() => {});
fetchInk();
// A pill or folder window is told that a setting changed, but not which one --
// re-read the rule instead of waiting for the next theme change.
window.__TAURI__.event.listen("settings-changed", fetchInk).catch(() => {});
