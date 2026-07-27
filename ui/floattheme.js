// Theme mapping for the small always-on-top windows (pills and pill folders).
// They carry their OWN tokens (floating.css), so a custom theme's colours are mapped
// onto them — a floating button follows the same palette as the main window instead
// of falling back to the base theme's colours.
const FLOAT_THEME_MAP = {
  accent: "--pill-bg",
  "accent-fg": "--pill-fg",
  surface: "--menu-bg",
  fg: "--menu-fg",
  border: "--menu-line",
  danger: "--danger",
};
const FLOAT_HEX = /^#[0-9a-fA-F]{6}$/;

export function applyFloatTheme(theme, vars) {
  if (theme) document.documentElement.setAttribute("data-theme", theme);
  const style = document.documentElement.style;
  for (const target of Object.values(FLOAT_THEME_MAP)) style.removeProperty(target);
  for (const [key, value] of Object.entries(vars || {})) {
    const target = FLOAT_THEME_MAP[key];
    if (target && FLOAT_HEX.test(value)) style.setProperty(target, value);
  }
}
