// Theme mapping for the small always-on-top windows (pills and pill folders).
// They carry their OWN tokens (floating.css), so a custom theme's colours are mapped
// onto them; a floating button follows the same palette as the main window instead
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
// "Black text from this brightness" / "Dark shadow behind white text" /
// "Shadow strength"), kept fresh from two sources: the theme-changed event's
// payload and one ink_prefs() fetch at module load (covers the window before
// the first event arrives). Defaults match the backend's own shipped default.
let INK = { auto: true, threshold: 60, halo: true, haloStrength: 75 };
let lastTheme = null;
let lastVars = null;
let applied = false; // true once applyFloatTheme has run at least once

// Relative luminance -> the ink that stays readable on `hex`. Duplicated from
// main.js's own readableInk rather than shared: this module ships to every
// satellite window, none of which load main.js.
function hexLuminance(hex) {
  const n = parseInt(hex.slice(1), 16);
  const lin = (c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(((n >> 16) & 255) / 255) + 0.7152 * lin(((n >> 8) & 255) / 255) + 0.0722 * lin((n & 255) / 255);
}

function readableInk(hex) {
  if (!INK.auto) return "#fff";
  return hexLuminance(hex) > INK.threshold / 100 ? "#000" : "#fff";
}

// White ink stays white up to the user's threshold (a deliberate look), but
// on a colour past the contrast crossover it gets a dark halo so it reads,
// unless the halo is switched off (INK.halo) or dialed to a different
// strength (INK.haloStrength, percent).
const INK_HALO_RATIO = 0.7 / 0.85;
function inkShadow(hex) {
  if (!INK.halo) return "none";
  const l = hexLuminance(hex);
  if (!(l > 0.179) || readableInk(hex) !== "#fff") return "none";
  const a1 = INK.haloStrength / 100;
  const a2 = Number((a1 * INK_HALO_RATIO).toFixed(2));
  return `0 0 2px rgba(0, 0, 0, ${Number(a1.toFixed(2))}), 0 1px 3px rgba(0, 0, 0, ${a2})`;
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
  style.setProperty("--pill-ink-shadow", FLOAT_HEX.test(pillBg) ? inkShadow(pillBg) : "none");
}

// Keep INK current without waiting for the next theme change, and re-paint
// the pill ink immediately once it arrives -- a satellite window must never
// need a restart to pick up a changed threshold or a flipped switch.
function setInk(ink) {
  if (!ink) return;
  INK = {
    auto: ink.auto !== false,
    threshold: Number.isFinite(ink.threshold) ? ink.threshold : 60,
    halo: ink.halo !== false,
    haloStrength: Number.isFinite(ink.haloStrength) ? ink.haloStrength : 75,
  };
  if (applied) applyFloatTheme(lastTheme, lastVars);
}
window.__TAURI__.event.listen("theme-changed", (e) => setInk(e.payload?.ink)).catch(() => {});
const fetchInk = () => window.__TAURI__.core.invoke("ink_prefs").then(setInk).catch(() => {});
fetchInk();
// A pill or folder window is told that a setting changed, but not which one --
// re-read the rule instead of waiting for the next theme change.
window.__TAURI__.event.listen("settings-changed", fetchInk).catch(() => {});
