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
