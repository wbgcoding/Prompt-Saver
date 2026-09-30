// The folder sync imported something behind the main window (ui/main.js, the
// "store-synced" listener): the window has to refetch the state and redraw, or it
// keeps showing (and later saving back) what it fetched before the import.
//
// main.js is one big module with boot code, so the real source text of the listener
// body is extracted and run in a vm against a scripted backend.
// Run:  node tests/ui/store_synced_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "..", "ui", "main.js"), "utf8").replace(/\r\n/g, "\n");
const marker = 'listen("store-synced", ';
const start = src.indexOf(marker);
if (start === -1) {
  console.log("FAIL: ui/main.js has no store-synced listener");
  process.exit(1);
}
const fnStart = start + marker.length;
const braceStart = src.indexOf("{", fnStart);
let depth = 0;
let end = -1;
for (let i = braceStart; i < src.length; i++) {
  if (src[i] === "{") depth++;
  else if (src[i] === "}" && --depth === 0) { end = i + 1; break; }
}
const handlerSrc = src.slice(fnStart, end);

async function run(libraryHidden) {
  const calls = [];
  const sandbox = {
    renderGrid: async (skipFetch) => { calls.push(skipFetch ? "grid:cached" : "grid:fetch"); },
    renderLibrary: () => { calls.push("library"); },
    libraryEl: { classList: { contains: () => libraryHidden } },
  };
  vm.createContext(sandbox);
  const handler = vm.runInContext("(" + handlerSrc + ")", sandbox);
  await handler();
  return calls;
}

(async () => {
  const problems = [];
  const closed = await run(true);
  if (closed.join() !== "grid:fetch") problems.push("closed library: expected one fetching grid render, got " + closed.join());
  const open = await run(false);
  if (open.join() !== "grid:fetch,library") problems.push("open library: expected grid fetch then library render, got " + open.join());
  if (problems.length) {
    problems.forEach((p) => console.log("FAIL: " + p));
    process.exit(1);
  }
  console.log("ok");
})();
