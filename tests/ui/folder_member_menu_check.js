// Folder window (ui/folder.js): pressing a member tile closes an open member menu. The tile's
// mousedown stops the event, so the document-level outside-click listener never sees it.
// Runs the real extracted handler in a node vm.
// Run:  node tests/ui/folder_member_menu_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "..", "ui", "folder.js"), "utf8").replace(/\r\n/g, "\n");
const marker = 'gridEl.addEventListener("mousedown", (e) => {';
const start = src.indexOf(marker);
if (start === -1) throw new Error("could not find the grid mousedown handler (source text changed?)");
let depth = 0, end = -1;
for (let i = src.indexOf("{", start); i < src.length; i++) {
  if (src[i] === "{") depth++;
  else if (src[i] === "}" && --depth === 0) { end = i + 1; break; }
}
const cls = new Set();
const sb = {
  memberMenu: { classList: { add: (c) => cls.add(c), remove: (c) => cls.delete(c), contains: (c) => cls.has(c) } },
  dragOutOn: false, dragOutMod: "ctrlKey", memberDrag: null,
  gridEl: { addEventListener: (_t, f) => { sb.handler = f; } },
};
vm.createContext(sb);
vm.runInContext(src.slice(start, end) + ");", sb);
const tile = { dataset: { id: "p1" }, draggable: false };
const ev = { target: { closest: () => tile }, button: 0, screenX: 1, screenY: 1, stopPropagation() {} };
const problems = [];
sb.memberMenu.classList.remove("hidden");
sb.handler(ev);
if (!sb.memberMenu.classList.contains("hidden")) problems.push("a tile press left the member menu open");
if (problems.length) {
  console.error("folder_member_menu_check: FAILED\n  " + problems.join("\n  "));
  process.exit(1);
}
console.log("folder_member_menu_check: ok");
