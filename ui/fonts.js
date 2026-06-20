// Shrink an element's text to fit a box in one proportional pass (measure at a
// cap size, scale down). The element must already be in the DOM. Shared by the
// grid copy bubble and the floating pill so the "Copied!" text tracks the size.
export function fitText(el, maxW, maxH, cap = 96) {
  // Reset to the stylesheet size first; if the box isn't laid out yet or the text
  // can't be measured, leave that default instead of blowing up to the cap size.
  el.style.fontSize = "";
  if (!(maxW > 0) || !el.textContent) return;
  el.style.fontSize = `${cap}px`;
  // Measure the text itself via a Range — robust inside centered flex/overflow
  // containers where scrollWidth under-reports.
  const range = document.createRange();
  range.selectNodeContents(el);
  const rect = range.getBoundingClientRect();
  if (!rect.width || !rect.height) { el.style.fontSize = ""; return; }
  let s = (cap * maxW) / rect.width;
  if (maxH > 0) s = Math.min(s, (cap * maxH) / rect.height);
  el.style.fontSize = `${Math.max(7, Math.min(cap, Math.floor(s)))}px`;
}

// Shared font catalog for the grid tiles and the floating pills.
// Keys are stored in prompts/settings; stacks fall back to common Windows fonts.
export const FONTS = {
  // Sans serif
  system: '"Segoe UI", system-ui, sans-serif',
  arial: "Arial, Helvetica, sans-serif",
  calibri: 'Calibri, "Segoe UI", sans-serif',
  verdana: "Verdana, Geneva, sans-serif",
  tahoma: "Tahoma, Geneva, sans-serif",
  trebuchet: '"Trebuchet MS", "Segoe UI", sans-serif',
  bahnschrift: 'Bahnschrift, "Segoe UI", sans-serif',
  candara: 'Candara, "Segoe UI", sans-serif',
  corbel: 'Corbel, "Segoe UI", sans-serif',
  // Serif
  georgia: "Georgia, serif",
  times: '"Times New Roman", Times, serif',
  cambria: "Cambria, Georgia, serif",
  garamond: 'Garamond, "Palatino Linotype", serif',
  palatino: '"Palatino Linotype", "Book Antiqua", serif',
  // Monospace
  mono: 'Consolas, "Courier New", monospace',
  courier: '"Courier New", Courier, monospace',
  lucida: '"Lucida Console", Consolas, monospace',
  // Script & display
  script: '"Segoe Script", "Comic Sans MS", cursive',
  comic: '"Comic Sans MS", "Segoe Script", cursive',
  impact: 'Impact, "Arial Black", sans-serif',
};

// Display names; null = translated via i18n (fontSystem / fontScript).
export const FONT_LABELS = {
  system: null,
  arial: "Arial",
  calibri: "Calibri",
  verdana: "Verdana",
  tahoma: "Tahoma",
  trebuchet: "Trebuchet MS",
  bahnschrift: "Bahnschrift",
  candara: "Candara",
  corbel: "Corbel",
  georgia: "Georgia",
  times: "Times New Roman",
  cambria: "Cambria",
  garamond: "Garamond",
  palatino: "Palatino",
  mono: "Consolas",
  courier: "Courier New",
  lucida: "Lucida Console",
  script: null,
  comic: "Comic Sans",
  impact: "Impact",
};
