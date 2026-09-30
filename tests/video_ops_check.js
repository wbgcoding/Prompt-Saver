// Video editor export graph (ui/video.js): checks that pull the real source of the
// functions involved out of the file and run them against a fake editor state.
// video.js boots a real window on load, so there is nothing to import.
// Run:  node tests/video_ops_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "ui", "video.js"), "utf8").replace(/\r\n/g, "\n");

function extractLine(label, exact) {
  if (!src.includes(exact)) throw new Error(`could not find ${label} in video.js: ${exact}`);
  return exact;
}
function extractBlock(label, marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find ${label} in video.js: ${marker}`);
  const braceStart = src.indexOf("{", start);
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}") {
      depth--;
      if (depth === 0) return src.slice(start, i + 1);
    }
  }
  throw new Error(`unbalanced braces extracting ${label}`);
}

const problems = [];
const expect = (label, ok, detail) => {
  if (!ok) problems.push(`${label}${detail === undefined ? "" : `: ${detail}`}`);
};

// Fresh sandbox per scenario so one state never leaks into the next.
function makeSandbox(extra) {
  const sandbox = Object.assign({ VE: null, KEPT_CUTS_BUF: [], KEPT_SEGMENTS_BUF: [], fontsDir: "C:/fonts" }, extra || {});
  vm.createContext(sandbox);
  vm.runInContext(
    [
      extractLine("clamp", "const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));"),
      extractLine("opOf", "const opOf = (tool) => VE.ops.filter((o) => o.tool === tool).pop() || null;"),
      extractLine("opsOf", "const opsOf = (tool) => VE.ops.filter((o) => o.tool === tool);"),
      extractBlock("keptSegments", "function keptSegments() {"),
      extractBlock("speedRate", "const speedRate = () => {") + ";",
      extractBlock("timelinePlan", "function timelinePlan() {"),
      extractBlock("planEntryOutMs", "function planEntryOutMs(entry) {"),
      extractBlock("outMs", "function outMs(srcMs) {"),
      extractLine("totalOutMs", "const totalOutMs = () => timelinePlan().reduce((sum, e) => sum + planEntryOutMs(e), 0);"),
      extractLine("secs", "const secs = (ms) => (ms / 1000).toFixed(3);"),
      extractBlock("ffPath", "function ffPath(path) {"),
      extractBlock("videoOverlayEffectiveEndMs", "function videoOverlayEffectiveEndMs(op) {"),
      extractBlock("videoOverlayEnable", "function videoOverlayEnable(op) {"),
      extractBlock("videoOverlayLoopArg", "function videoOverlayLoopArg(op) {"),
      extractBlock("isLosslessCopy", "function isLosslessCopy() {"),
    ].join("\n"),
    sandbox,
    { filename: "video.js (extracted)" }
  );
  return sandbox;
}

function baseState(ops) {
  return {
    dur: 20000,
    ops,
    probe: { stream_copy_safe: true },
    audio: { sys: { mute: false, vol: 100 }, mic: { mute: false, vol: 100 } },
  };
}

// An inserted section is only present as a movie= source inside the filter
// graph, so a plain stream copy of the main file must never be chosen for it.
{
  const sb = makeSandbox({ exportFormat: () => "mp4", exportQuality: () => "high" });
  sb.VE = baseState([{ tool: "clip", position: "end", missing: false, path: "C:/x/clip.mp4", durationMs: 3000 }]);
  expect("isLosslessCopy is false with an inserted clip", vm.runInContext("isLosslessCopy()", sb) === false);
  sb.VE = baseState([{ tool: "clip", position: "end", missing: true, path: "C:/x/clip.mp4", durationMs: 3000 }]);
  expect("isLosslessCopy stays true when the only clip is missing", vm.runInContext("isLosslessCopy()", sb) === true);
  sb.VE = baseState([]);
  expect("isLosslessCopy stays true for an untouched recording", vm.runInContext("isLosslessCopy()", sb) === true);
}

if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log("PASS");
}
