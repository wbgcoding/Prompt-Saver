// Satellite-window fixes (video editor, recording tools, variables, ...): each section runs the
// real extracted source of the function in a vm, or reads the one source line that carries the fix.
// UI_CHECK_DIR points the test at another copy of ui/ (to prove it fails on older code).
// Run:  node tests/ui/satellite_fixes_check.js      Exit: 0 = clean, 1 = problems.
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
const section = (fn) => { try { fn(); } catch (e) { problems.push(String(e.message || e)); } };
const run = (code, sandbox) => { vm.createContext(sandbox); vm.runInContext(code, sandbox); return sandbox; };

// Batch export: two sources that map to the same output name get distinct names.
section(() => {
  const src = read("recordingTools.js");
  const sb = run(extractBlock(src, "export function batchDestNames(").replace("export ", ""), {});
  const names = sb.batchDestNames(["rec.mp4", "rec.mkv", "REC.webm", "other.mp4"], "mp4");
  check(names[0] === "rec.mp4", "the first file keeps its plain name");
  check(new Set(names.map((n) => n.toLowerCase())).size === names.length, "every batch output name is unique");
  check(names[1] === "rec (2).mp4" && names[2] === "REC (3).mp4", "later clashes get a numbered suffix");
  check(names[3] === "other.mp4", "a file without a clash keeps its name");
  check(/batchDestNames\(names, format\)/.test(src), "the batch export builds its destinations with batchDestNames");
});

// Video editor: a second "load" request queued behind the first confirm must not discard the
// edit state of the source the first request just loaded.
later(async () => {
  const src = read("video.js");
  let handler = null;
  const discards = [];
  const gates = [];
  const sb = {
    listen: (name, fn) => { if (name === "video-editor-load") handler = fn; },
    VE: { src: "A.mp4", ops: [1], dirty: true },
    t: (k) => k,
    stopEditStateAutosave: () => {},
    invoke: async (cmd, args) => { if (cmd === "discard_edit_state") discards.push(args.target); },
    confirmDialog: () => new Promise((resolve) => gates.push(resolve)),
    loadRecording: (p) => { sb.VE.src = p; sb.VE.ops = []; sb.VE.dirty = false; },
  };
  run(extractBlock(src, 'listen("video-editor-load", async (e)') + ");", sb);
  const first = handler({ payload: "B.mp4" });
  const second = handler({ payload: "C.mp4" });
  gates[0](true);
  await first;
  gates[1](true);
  await second;
  check(discards.length === 1 && discards[0] === "A.mp4", `only the replaced source loses its edit state (got ${JSON.stringify(discards)})`);
  check(sb.VE.src === "C.mp4", "the last confirmed request is the one that ends up loaded");
});

let guard;
const timedOut = new Promise((resolve) => { guard = setTimeout(() => { problems.push("an async check never finished"); resolve(); }, 3000); });
Promise.race([Promise.all(pending), timedOut]).then(() => {
  clearTimeout(guard);
  if (problems.length) {
    console.error("FAIL");
    for (const p of problems) console.error("  - " + p);
    process.exit(1);
  }
  console.log("satellite window fixes: ok");
});
