// Video export request (ui/video.js startExport): every numeric field the Rust
// side declares as an unsigned integer must be a whole number. A tempo such as
// 1.3x makes the output length fractional, and serde refuses a float for a u64
// ("invalid type: floating point ..., expected u64"), which aborted the export.
//
// video.js is an ES module that boots a real window, so this pulls the exact
// source of the helpers involved out of the real file and runs them.
// Run:  node tests/video_export_request_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "ui", "video.js"), "utf8").replace(/\r\n/g, "\n");

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
function extractLine(label, exact) {
  if (!src.includes(exact)) throw new Error(`could not find ${label} in video.js: ${exact}`);
  return exact;
}

const startExportBody = extractBlock("startExport", "async function startExport(toClipboard) {");
const totalDecl = startExportBody.match(/^ {2}const total = .*;$/m);
if (!totalDecl) throw new Error("could not find the total declaration in startExport");
const noteBody = extractBlock("refreshExportSizeNote", "async function refreshExportSizeNote(format) {");
const durLine = noteBody.match(/^ {4}durationMs: .*,$/m);
if (!durLine) throw new Error("could not find durationMs in refreshExportSizeNote");

const sandbox = { VE: null, KEPT_CUTS_BUF: [], KEPT_SEGMENTS_BUF: [] };
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
    extractLine("totalOutMs", "const totalOutMs = () => timelinePlan().reduce((sum, e) => sum + planEntryOutMs(e), 0);"),
  ].join("\n"),
  sandbox,
  { filename: "video.js (extracted)" }
);

const problems = [];
const scenarios = [
  { name: "speed 1.3 and one cut", ops: [{ tool: "cut", startMs: 2000, endMs: 4000 }, { tool: "speed", rate: 1.3 }] },
  { name: "speed 0.7 and no cut", ops: [{ tool: "speed", rate: 0.7 }] },
  { name: "speed 1.3, trim, no cut", ops: [{ tool: "trim", startMs: 111, endMs: 19999 }, { tool: "speed", rate: 1.3 }] },
];
for (const sc of scenarios) {
  sandbox.VE = { dur: 20000, ops: sc.ops };
  vm.runInContext(`(() => {\n${totalDecl[0]}\nresult = { total, dur: ${durLine[0].replace(/^ *durationMs: /, "").replace(/,$/, "")} };\n})();`, sandbox);
  const raw = vm.runInContext("totalOutMs()", sandbox);
  if (Number.isInteger(raw)) problems.push(`${sc.name}: fixture not fractional (${raw}), it would prove nothing`);
  if (!Number.isInteger(sandbox.result.total)) problems.push(`${sc.name}: total_ms is ${sandbox.result.total}, not an integer`);
  if (!Number.isInteger(sandbox.result.dur)) problems.push(`${sc.name}: size estimate durationMs is ${sandbox.result.dur}, not an integer`);
}
extractLine("request total_ms fed from total", "        total_ms: total,");
extractLine("disk check durationMs fed from total", "destPath: dest, format, videoKbps: request.video_kbps, losslessQuality: request.lossless_quality, durationMs: total,");

// A finished export is reported under the path the backend returned (it picks a free
// name when the requested one is taken), never under the requested destination.
if (!startExportBody.includes('const returned = await invoke("export_video", { req: request });')) {
  problems.push("startExport no longer keeps the path export_video returns");
}
const afterReturn = startExportBody.slice(startExportBody.indexOf("const returned"));
for (const bad of ['() => dest)', "path: dest }", "VE.lastExport = dest"]) {
  if (afterReturn.includes(bad)) problems.push(`startExport uses the requested destination after success: ${bad}`);
}
// Progress belongs to one export on one recording generation.
if (!src.includes("exportRun.gen !== VE.loadGen")) problems.push("export progress is not tied to the load generation");
if (!startExportBody.includes('if (!stillHere()) $("ve-export-progress-row").classList.add("hidden");')) {
  problems.push("startExport does not hide its progress row when the editor moved on");
}

// saveEditStateNow stores only changes made since the last export or load.
{
  const saved = [];
  const box = {
    VE: { editStateKeepOn: true, src: "a.mp4", ops: [{ tool: "trim" }], editStateSize: 5, editStateMtime: 1, dirty: false },
    invoke: async (cmd) => { saved.push(cmd); },
    JSON,
  };
  vm.createContext(box);
  vm.runInContext(extractBlock("saveEditStateNow", "async function saveEditStateNow() {"), box);
  (async () => {
    await vm.runInContext("saveEditStateNow()", box);
    if (saved.length) problems.push("saveEditStateNow saved although nothing changed since the export");
    box.VE.dirty = true;
    await vm.runInContext("saveEditStateNow()", box);
    if (saved.length !== 1) problems.push("saveEditStateNow did not save an edited state");
    finish();
  })();
}

function finish() {
if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
}
console.log("PASS");
}
