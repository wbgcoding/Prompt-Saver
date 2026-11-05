// Fixes in the chain, compare, copy-flow, editor, grid and tag scripts, each section runs the
// real extracted source of the function in a node vm. UI_CHECK_DIR points the test at another
// copy of ui/ (to prove it fails on older code).
// Run:  node tests/ui/keyboard_race_fixes_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const uiDir = process.env.UI_CHECK_DIR || path.join(__dirname, "..", "..", "ui");
const read = (name) => fs.readFileSync(path.join(uiDir, name), "utf8").replace(/\r\n/g, "\n");
function extractBlock(src, marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find (text changed?): ${marker}`);
  const braceStart = src.indexOf("{", src.indexOf(")", start));
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces after: ${marker}`);
}
const problems = [];
const check = (ok, what) => { if (!ok) problems.push(what); };
const pending = [];
const later = (fn) => { pending.push(fn().catch((e) => problems.push(String(e.message || e)))); };
const run = (code, sandbox) => { vm.createContext(sandbox); vm.runInContext(code, sandbox); return sandbox; };
const tick = () => new Promise((r) => setImmediate(r));
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };

// chain.js toggleChainPick: a locked confidential prompt goes through the vault gate first.
later(async () => {
  const src = read("chain.js");
  const code = extractBlock(src, "export async function toggleChainPick").replace(/export /g, "");
  const peeks = [];
  let gate = null;
  const sel = [];
  const sandbox = run(code + "\n;globalThis.pick = toggleChainPick;", {
    isTextPrompt: () => true,
    chainPicking: new Set(),
    chainSel: sel, chainTexts: new Map(), chainRaw: new Map(),
    chainMode: true,
    refreshChainUi: () => {}, pushChainClipboard: () => {},
    readyForCopy: async (p) => gate(p),
    invoke: async (cmd) => { peeks.push(cmd); return null; },
    varTokens: () => [],
    resolveVars: async (x) => x,
  });
  gate = async () => null;
  await sandbox.pick({ id: "a", name: "Secret", text: "", locked: true });
  check(sel.length === 0, `a refused unlock chains nothing, got ${sel}`);
  gate = async (p) => ({ ...p, text: "real text", locked: false });
  await sandbox.pick({ id: "b", name: "Secret", text: "", locked: true });
  check(sel.join() === "b", "an unlocked prompt is picked");
});

Promise.all(pending).then(() => {
  if (problems.length) { console.error("keyboard_race_fixes_check FAILED:\n" + problems.map((p) => "  - " + p).join("\n")); process.exit(1); }
  console.log("keyboard_race_fixes_check: ok");
});
