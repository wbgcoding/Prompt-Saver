// The screenshot editor's right-click shape menu must stack above the editor
// overlay: a plain .ctx (z-index 70) opened behind it, invisible and unclickable.
// Reads the real stylesheet and markup. Run:  node tests/snip_ctx_layer_check.js
const fs = require("fs");
const path = require("path");
const ui = path.join(__dirname, "..", "ui");
const css = fs.readFileSync(path.join(ui, "styles", "main.css"), "utf8");
const js = fs.readFileSync(path.join(ui, "main.js"), "utf8");
const problems = [];
const zOf = (re) => {
  const m = css.match(re);
  return m ? Number(m[1]) : NaN;
};
const overlay = zOf(/#snip-edit\.overlay\s*\{[^}]*?z-index:\s*(\d+)/);
const menu = zOf(/\.ctx\.snip-ctx-pop\s*\{[^}]*?z-index:\s*(\d+)/);
if (!(menu > overlay)) problems.push(`menu z-index ${menu} is not above the editor overlay ${overlay}`);
if (!/snipCtxPop\.className\s*=\s*"ctx snip-ctx-pop /.test(js)) problems.push("snipCtxPop is not given the snip-ctx-pop class");
if (problems.length) { console.error("snip_ctx_layer_check: FAILED\n  " + problems.join("\n  ")); process.exit(1); }
console.log("snip_ctx_layer_check: ok");
