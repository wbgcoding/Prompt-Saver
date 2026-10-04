// The screenshot editor's right-click shape menu must stack above the editor
// overlay: a plain .ctx (z-index 70) opened behind it, invisible and unclickable.
// Reads the real stylesheet and markup. Run:  node tests/snip_ctx_layer_check.js
const fs = require("fs");
const path = require("path");
const ui = path.join(__dirname, "..", "ui");
const css = fs.readFileSync(path.join(ui, "styles", "main.css"), "utf8");
const js = require("./ui_sources.js").mainWindowSource();
const problems = [];
const zOf = (re) => {
  const m = css.match(re);
  return m ? Number(m[1]) : NaN;
};
const overlay = zOf(/#snip-edit\.overlay\s*\{[^}]*?z-index:\s*(\d+)/);
const menu = zOf(/\.ctx\.snip-ctx-pop\s*\{[^}]*?z-index:\s*(\d+)/);
if (!(menu > overlay)) problems.push(`menu z-index ${menu} is not above the editor overlay ${overlay}`);
// Everything the editor opens must sit above it: the shortcut list (a modal shared
// with the main window) and every pop-up the editor builds itself.
const above = (label, re) => {
  const z = zOf(re);
  if (!(z > overlay)) problems.push(`${label} z-index ${z} is not above the editor overlay ${overlay}`);
};
above("shortcut list", /#shortcuts-modal\.overlay\s*\{[^}]*?z-index:\s*(\d+)/);
above("colour pop-up", /\.snip-color-pop\s*\{[^}]*?z-index:\s*(\d+)/);
above("scale pop-up", /\.snip-scale-pop\s*\{[^}]*?z-index:\s*(\d+)/);
above("custom colour picker", /\.color-pop\s*\{[^}]*?z-index:\s*(\d+)/);
above("number preset list", /\.numfield-pop\s*\{[^}]*?z-index:\s*(\d+)/);
above("toast", /\.toast\s*\{[^}]*?z-index:\s*(\d+)/);
const shortcutsOpenedFromEditor = /snip-edit-shortcuts"\)\.addEventListener\("click", \(\) => openShortcutsModal\("snipEditor"\)\)/.test(js);
if (!shortcutsOpenedFromEditor) problems.push("the editor no longer opens the shortcut list; update this check");
if (!/snipCtxPop\.className\s*=\s*"ctx snip-ctx-pop /.test(js)) problems.push("snipCtxPop is not given the snip-ctx-pop class");
if (problems.length) { console.error("snip_ctx_layer_check: FAILED\n  " + problems.join("\n  ")); process.exit(1); }
console.log("snip_ctx_layer_check: ok");
