// Live video-overlay preview timing (ui/video.js): the clip time shown while
// scrubbing must land on the same footage frame the export actually produces,
// once a cut or a speed change makes the source and output timelines diverge.
//
// video.js has no exports and its own top-level code boots a real Tauri
// window (invoke calls, DOM lookups by id), so there is no module to import
// here. Instead this pulls the exact source text of the handful of pure
// helpers involved (clamp, opOf/opsOf, keptSegments, speedRate, outMs, and
// the two lines that compute a video overlay's own playback position) out of
// the real file and runs them, so the probe fails on whatever the file
// actually contains rather than on a hand-copied reimplementation of it.
// Run:  node tests/video_overlay_time_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const SRC_PATH = path.join(__dirname, "..", "ui", "video.js");
const src = fs.readFileSync(SRC_PATH, "utf8").replace(/\r\n/g, "\n");

function extractLine(label, exact) {
  if (!src.includes(exact)) throw new Error(`could not find ${label} in video.js (source text changed?): ${exact}`);
  return exact;
}

// Extracts "<marker> ... <matching closing brace>" by counting braces from
// the marker's own opening one, so a nested for/if inside the function does
// not end the extraction early.
function extractBlock(label, marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find ${label} in video.js (source text changed?): ${marker}`);
  const braceStart = src.indexOf("{", start);
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}") {
      depth--;
      if (depth === 0) return src.slice(start, i + 1);
    }
  }
  throw new Error(`unbalanced braces extracting ${label} from video.js`);
}

const clampLine = extractLine("clamp", "const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));");
const opOfLine = extractLine("opOf", "const opOf = (tool) => VE.ops.filter((o) => o.tool === tool).pop() || null;");
const opsOfLine = extractLine("opsOf", "const opsOf = (tool) => VE.ops.filter((o) => o.tool === tool);");
const keptSegmentsBlock = extractBlock("keptSegments", "function keptSegments() {");
const speedRateBlock = extractBlock("speedRate", "const speedRate = () => {") + ";";
const outMsBlock = extractBlock("outMs", "function outMs(srcMs) {");

// The two lines inside buildLiveOverlayEl's video branch that turn the
// current playhead into the overlay's own clip time. Pinning the exact text
// (rather than a looser regex) means a future rewrite that quietly drops the
// outMs() call trips this probe immediately instead of staying green.
const overlayTimeLines = extractLine(
  "video overlay clip-time lines",
  "let t = outMs(clamp(VE.playhead, 0, VE.dur)) / 1000;\n    if (op.loop && op.videoDurationMs) t %= op.videoDurationMs / 1000;"
);

const sandbox = { VE: null, KEPT_CUTS_BUF: [], KEPT_SEGMENTS_BUF: [] };
vm.createContext(sandbox);
vm.runInContext(
  [clampLine, opOfLine, opsOfLine, keptSegmentsBlock, speedRateBlock, outMsBlock].join("\n"),
  sandbox,
  { filename: "video.js (extracted)" }
);

// A cut early in the clip plus a 2x speed op: the two things buildVideoChain
// comments say already make the output timeline diverge from the source one
// (see the "outMs" doc comment in video.js). If the live preview ever goes
// back to reading VE.playhead directly, this cut+speed combination is exactly
// what would make it visibly drift from the export.
sandbox.VE = {
  dur: 20000,
  ops: [
    { tool: "cut", startMs: 2000, endMs: 4000 },
    { tool: "speed", rate: 2 },
  ],
};

const overlayOp = { startMs: 10000, endMs: 14000, loop: false, videoDurationMs: 0 };

function overlayClipTimeAtPlayhead(playheadMs) {
  sandbox.VE.playhead = playheadMs;
  sandbox.op = overlayOp;
  vm.runInContext(`result = (() => {\n${overlayTimeLines}\nreturn t;\n})();`, sandbox);
  return sandbox.result;
}

// What the export's own videoOverlayEnable()/enableExpr() use as the
// overlay's window boundaries on the output timeline -- outMs() run on the
// exact same VE state.
function exportOutputSeconds(srcMs) {
  vm.runInContext(`result = outMs(clamp(${srcMs}, 0, VE.dur)) / 1000;`, sandbox);
  return sandbox.result;
}

const problems = [];
const check = (label, actual, expected) => {
  if (Math.abs(actual - expected) > 1e-9) problems.push(`${label}: got ${actual}, expected ${expected}`);
};

// At the exact moment the overlay window opens (playhead == op.startMs), the
// export shows the overlay's footage at output time outMs(op.startMs) (see
// the comment now sitting next to the fix: the movie= source is never
// delayed to op.startMs, it plays in lockstep with the whole export from
// t=0). The live preview's clip time at that same instant must equal it.
const windowStartOutputS = exportOutputSeconds(overlayOp.startMs);
check("preview clip time at the overlay's own start matches the export's output-time offset", overlayClipTimeAtPlayhead(overlayOp.startMs), windowStartOutputS);
// Sanity check on the fixture: with a 2s cut sitting before the overlay and a
// 2x speed op, the output time at startMs must actually be less than
// startMs/1000 -- otherwise this fixture would not be exercising the bug at
// all (the old, wrong formula and the new one would coincidentally agree).
check("fixture actually diverges from a naive source-timeline reading", windowStartOutputS, 4);

// 2 seconds later on the source timeline, with no cut inside this stretch,
// only the 2x speed applies: 1 output-timeline second must pass. The old
// (VE.playhead - op.startMs) formula would report 2 full seconds here since
// it never looked at speed at all.
const twoSecondsLaterS = overlayClipTimeAtPlayhead(overlayOp.startMs + 2000);
check("clip time advances at the sped-up output rate, not the raw source rate", twoSecondsLaterS - overlayClipTimeAtPlayhead(overlayOp.startMs), 1);

if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log("PASS");
}
