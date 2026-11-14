// The batch export dialog of the media manager (ui/recordingTools.js): a second export asked
// for while the dialog still waits closes the first one and leaves exactly one set of
// Start/Cancel listeners behind.
// Runs the real extracted source in a node vm. Run:  node tests/batch_export_reentry_check.js
const vm = require("vm");
const src = require("./ui_sources.js").mainWindowSource().replace(/\r\n/g, "\n");

function extractBlock(marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find ${marker} (source text changed?)`);
  const braceStart = src.indexOf("{", start);
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces extracting ${marker}`);
}

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

const els = {};
const el = (id) => els[id] || (els[id] = {
  id, value: "", textContent: "", onclick: null, listeners: [],
  classList: { add() {}, remove() {} },
  addEventListener(type, fn) { this.listeners.push(fn); },
  removeEventListener(type, fn) { this.listeners = this.listeners.filter((f) => f !== fn); },
});
const sb = {
  $: el, t: (k) => k, toast() {}, errToast() {}, LANG: "en", fmtBytes: String, confirmDialog: async () => true,
  BATCH_KBPS_HIGH: 1, BATCH_KBPS_SMALL: 1,
  invoke: async () => { throw new Error("stop here"); },
};
vm.createContext(sb);
vm.runInContext("var batchExportClosePrevious = null;" + extractBlock("async function runBatchExportRecordings(dir, names) {"), sb);

let finished = false;
process.on("exit", (code) => { if (!finished && code === 0) { console.log("FAIL the first dialog call never ended"); process.exitCode = 1; } });
(async () => {
  const first = sb.runBatchExportRecordings("C:/v", ["a.mp4"]);
  await Promise.resolve();
  check("first dialog: one Start listener", el("batch-export-start").listeners.length, 1);
  const second = sb.runBatchExportRecordings("C:/v", ["b.mp4"]);
  await Promise.resolve();
  check("second dialog: still one Start listener", el("batch-export-start").listeners.length, 1);
  check("second dialog: one Cancel listener", el("batch-export-cancel").listeners.length, 1);
  check("second dialog: one close listener", el("batch-export-close").listeners.length, 1);
  await first; // the first call has ended (no answer) instead of hanging
  el("batch-export-cancel").listeners[0]();
  await second;
  check("after Cancel: nothing left attached", [el("batch-export-start").listeners.length, el("batch-export-cancel").listeners.length], [0, 0]);
  finished = true;
  for (const p of problems) console.log("FAIL", p);
  console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
  process.exit(problems.length ? 1 : 0);
})();
