// Main grid (ui/gridMenus.js): a second right-click on the same tile within CTX_DOUBLE_MS
// opens the editor instead of the menu; a slow second click or another tile opens the menu.
// The menu's own contextmenu listener routes through the same function. Runs the real
// extracted code in a node vm.
// Run:  node tests/ui/ctx_double_edit_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "..", "ui", "gridMenus.js"), "utf8").replace(/\r\n/g, "\n");
const start = src.indexOf("export const CTX_DOUBLE_MS");
const fnAt = src.indexOf("export function openTileCtx(", start);
if (start === -1 || fnAt === -1) throw new Error("could not find openTileCtx (source text changed?)");
let depth = 0, end = -1;
for (let i = src.indexOf("{", fnAt); i < src.length; i++) {
  if (src[i] === "{") depth++;
  else if (src[i] === "}" && --depth === 0) { end = i + 1; break; }
}
const problems = [];
if (!/ctxEl\.addEventListener\("contextmenu"[\s\S]{0,200}openTileCtx\(ctxId/.test(src)) {
  problems.push("the open menu does not route its own right-click through openTileCtx");
}
const log = [];
const sb = {
  openCtx: (id) => log.push("menu:" + id),
  closeCtx: () => log.push("close"),
  editPrompt: (id) => log.push("edit:" + id),
  performance: { now: () => 0 },
};
vm.createContext(sb);
vm.runInContext(src.slice(start, end).replace(/export /g, "") + "\nthis.openTileCtx = openTileCtx; this.MS = CTX_DOUBLE_MS;", sb);
const run = (id, t) => { log.length = 0; sb.openTileCtx(id, 0, 0, t); return log.join(","); };
if (run("a", 1000) !== "menu:a") problems.push("first right-click did not open the menu");
if (run("a", 1000 + sb.MS - 50) !== "close,edit:a") problems.push("quick second right-click did not open the editor");
if (run("a", 1000 + sb.MS) !== "menu:a") problems.push("a third right-click after a double did not start over with the menu");
if (run("a", 1000 + 3 * sb.MS) !== "menu:a") problems.push("slow second right-click opened the editor");
if (run("b", 1000 + 3 * sb.MS + 10) !== "menu:b") problems.push("right-click on another tile opened the editor");
if (problems.length) {
  console.error("ctx_double_edit_check: FAILED\n  " + problems.join("\n  "));
  process.exit(1);
}
console.log("ctx_double_edit_check: ok");
