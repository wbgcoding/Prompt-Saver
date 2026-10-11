// Folder window (ui/folder.js): a blur that arrives while the popover is still
// waiting for its backend box must close it once the box arrives, instead of leaving
// the grid open over a window that already lost focus.
// Runs the real extracted openPopover/closePopover/layout in a node vm.
// Run:  node tests/ui/folder_blur_open_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "..", "ui", "folder.js"), "utf8").replace(/\r\n/g, "\n");
function extract(marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find ${marker} (source text changed?)`);
  const b = src.indexOf("{", start);
  let d = 0;
  for (let i = b; i < src.length; i++) {
    if (src[i] === "{") d++;
    else if (src[i] === "}" && --d === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces extracting ${marker}`);
}
function fakeEl() {
  const cls = new Set(["hidden"]);
  return { style: {}, classList: { add: (c) => cls.add(c), remove: (c) => cls.delete(c), contains: (c) => cls.has(c) } };
}

(async () => {
  const problems = [];
  let release;
  const calls = [];
  const el = fakeEl();
  const sb = {
    open: null, seq: 0, openReq: 0, opening: false, blurredWhileOpening: false,
    animMs: 0, folderId: "f1", POP_GAP: 4, state: { scale: 1 },
    menuEl: fakeEl(), popEl: el, memberMenu: fakeEl(), iconEl: fakeEl(),
    document: { body: { classList: { add() {}, remove() {} }, dataset: {} } },
    requestAnimationFrame: (f) => f(),
    px: (v) => v + "px", popoverSize: () => ({ w: 10, h: 10 }), renderGrid() {},
    sleep: () => Promise.resolve(),
    openEl() { return el; },
    invoke(cmd) {
      calls.push(cmd);
      if (cmd === "expand_float_folder") return new Promise((r) => { release = () => r({ side: "r", icon_x: 0, icon_y: 0, pop_x: 0, pop_y: 0 }); });
      return Promise.resolve();
    },
  };
  vm.createContext(sb);
  for (const m of ["async function layout(kind, el, token) {", "async function openPopover(kind) {", "async function closePopover(instant) {"]) vm.runInContext(extract(m), sb);
  const p = sb.openPopover("grid");
  await Promise.resolve(); await Promise.resolve();
  // the blur handler as written in the file
  const blur = src.slice(src.indexOf('window.addEventListener("blur"'));
  if (!/opening && !open\) blurredWhileOpening = true/.test(blur.slice(0, 300))) problems.push("blur handler does not remember a blur during opening");
  sb.blurredWhileOpening = true; // what that handler sets
  release();
  await p;
  if (sb.open !== null) problems.push("popover stayed open after a blur during opening");
  if (!calls.includes("collapse_float_folder")) problems.push("window was not collapsed again");
  for (const x of problems) console.log("FAIL", x);
  console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
  process.exit(problems.length ? 1 : 0);
})();
