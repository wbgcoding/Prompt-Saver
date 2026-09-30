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
      extractBlock("videoOverlayWindowOutMs", "function videoOverlayWindowOutMs(op) {"),
      extractBlock("videoOverlayEnable", "function videoOverlayEnable(op) {"),
      extractBlock("videoOverlayLoopArg", "function videoOverlayLoopArg(op) {"),
      extractBlock("isLosslessCopy", "function isLosslessCopy() {"),
      extractBlock("silentGapLabel", "function silentGapLabel(entry, label, parts) {"),
      extractBlock("silentSpanLabel", "function silentSpanLabel(ms, label, parts) {"),
      extractBlock("clipAudioLabel", "function clipAudioLabel(entry, label, parts) {"),
      extractBlock("atempoChain", "function atempoChain(rate) {"),
      extractBlock("buildAudioChain", "function buildAudioChain() {"),
      extractBlock("pushOp", "function pushOp(op, opts) {"),
      extractBlock("pushReplacing", "function pushReplacing(match, op, opts) {"),
      extractBlock("capPrevChain", "function capPrevChain(op, max) {"),
      extractLine("overlayStack", "const overlayStack = () => opsOf(\"overlay\").sort((a, b) => (a.z || 0) - (b.z || 0));"),
      extractBlock("enableExpr", "function enableExpr(op) {"),
      extractLine("hexToFf", "const hexToFf = (hex, opacity) => `0x${String(hex).replace(\"#\", \"\")}@${(clamp(opacity ?? 100, 0, 100) / 100).toFixed(2)}`;"),
      extractBlock("scaleTarget", "function scaleTarget() {"),
      extractBlock("clipVideoLabel", "function clipVideoLabel(op, label, parts, fps) {"),
      extractBlock("buildVideoChain", "function buildVideoChain() {"),
      extractBlock("undo", "function undo() {"),
      extractLine("rectsEqual", "const rectsEqual = (a, b) => !!a && !!b && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;"),
      extractBlock("endRectDrag", "function endRectDrag() {"),
      extractBlock("closeWindow", "async function closeWindow() {"),
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

// A video overlay's footage runs at native speed on the output clock: its
// enable window must end at start + footage length in OUTPUT seconds, whatever
// speed op sits on the main timeline.
{
  const sb = makeSandbox({});
  const overlay = { tool: "overlay", kind: "video", startMs: 0, endMs: 20000, loop: false, videoDurationMs: 8000 };
  const endOf = (ops) => {
    sb.VE = baseState(ops);
    const m = /between\(t,([\d.]+),([\d.]+)\)/.exec(vm.runInContext("videoOverlayEnable", sb)(overlay) || "");
    return m ? Number(m[2]) : null;
  };
  expect("overlay window ends at its footage length at 2x speed", endOf([{ tool: "speed", rate: 2 }, overlay]) === 8, String(endOf([{ tool: "speed", rate: 2 }, overlay])));
  expect("overlay window ends at its footage length at 0.5x speed", endOf([{ tool: "speed", rate: 0.5 }, overlay]) === 8, String(endOf([{ tool: "speed", rate: 0.5 }, overlay])));
  expect("overlay window at native speed is unchanged", endOf([overlay]) === 8);
  sb.VE = baseState([{ tool: "speed", rate: 2 }]);
  const looping = Object.assign({}, overlay, { loop: true, videoDurationMs: 3000 });
  // 20 s source at 2x = 10 s of output, repeated 3 s footage: 4 plays = loop count 3.
  expect("loop count follows the output window", vm.runInContext("videoOverlayLoopArg", sb)(looping) === ":loop=3", vm.runInContext("videoOverlayLoopArg", sb)(looping));
}

// Overlays stack by a number given at creation, not by position in the op list:
// editing the lower one re-pushes it at the end of the list, and the export must
// still draw it below the one added later.
{
  const sb = makeSandbox({
    exportFormat: () => "mp4", exportQuality: () => "high", formatHasAudio: () => true,
    SILENT_FORMATS: new Set(["gif"]), MAX_SILENT_DIM: 1280, afterOpsChanged: () => {},
  });
  sb.VE = baseState([]);
  Object.assign(sb.VE, { floor: 0, undoMax: 50, redo: [], w: 1920, h: 1080, fps: 30 });
  const frame = (id, color) => ({ tool: "overlay", id, kind: "frame", startMs: 0, endMs: 10000, rect: { x: 10, y: 10, w: 100, h: 100 }, color, opacity: 100, thickness: 4, style: "solid" });
  vm.runInContext("pushOp", sb)(frame("A", "#ff0000"));
  vm.runInContext("pushOp", sb)(frame("B", "#00ff00"));
  const a = sb.VE.ops.find((o) => o.id === "A");
  vm.runInContext("pushReplacing", sb)((o) => o.id === "A", Object.assign({}, a, { opacity: 50 }));
  expect("the edited overlay moved to the end of the op list", sb.VE.ops[sb.VE.ops.length - 1].id === "A");
  const g = vm.runInContext("buildVideoChain()", sb) || "";
  expect("the edited lower overlay is still drawn first", g.indexOf("0xff0000") !== -1 && g.indexOf("0xff0000") < g.indexOf("0x00ff00"), g);
}

// Dragging an overlay must leave a real undo step that restores where it was.
// The live drag moves the op in place, so the step has to be built from the
// original position; a text op has no rect and used to get an empty one.
{
  const sb = makeSandbox({
    changed: 0,
    afterOpsChanged: () => { sb.changed++; }, renderSnapGuides: () => {}, renderRects: () => {}, RECT_MIN_PX: 16,
    snapRect: (r) => r,
  });
  sb.VE = baseState([]);
  Object.assign(sb.VE, { floor: 0, undoMax: 50, redo: [], drag: null, guide: null });
  const text = { tool: "overlay", id: "T", kind: "text", startMs: 0, endMs: 5000, x: 100, y: 100, text: "hi" };
  vm.runInContext("pushOp", sb)(text);
  const before = sb.VE.ops.length;
  sb.changed = 0;
  // What pointerdown and pointermove leave behind after a 50 px drag to the right.
  text.x = 150;
  sb.VE.drag = { kind: "rect", op: text, handle: null, start: { x: 0, y: 0 }, orig: { x: 100, y: 100, w: 0, h: 0 }, live: null, livePoint: { x: 150, y: 100 } };
  vm.runInContext("endRectDrag()", sb);
  const now = sb.VE.ops.find((o) => o.id === "T");
  expect("a text drag creates one replacing op", sb.VE.ops.length === before && !!now._prev);
  expect("a text drag keeps the new position", now.x === 150 && now.y === 100);
  expect("a text drag never gives the text a rect", now.rect === undefined && now._prev.rect === undefined);
  expect("a text drag runs the op-changed path (dirty flag, autosave)", sb.changed === 1, String(sb.changed));
  vm.runInContext("undo()", sb);
  const undone = sb.VE.ops.find((o) => o.id === "T");
  expect("undoing a text drag restores the old position", undone.x === 100 && undone.y === 100, JSON.stringify(undone));

  // A click without movement is no edit at all and must not add a rect.
  const still = sb.VE.ops.find((o) => o.id === "T");
  sb.VE.drag = { kind: "rect", op: still, handle: null, start: { x: 0, y: 0 }, orig: { x: 100, y: 100, w: 0, h: 0 }, live: null, livePoint: { x: 100, y: 100 } };
  const len = sb.VE.ops.length, redoLen = sb.VE.redo.length;
  vm.runInContext("endRectDrag()", sb);
  expect("a text click without movement changes nothing", sb.VE.ops.length === len && sb.VE.redo.length === redoLen && still.rect === undefined);

  // A frame overlay: the drag changes op.rect in place, undo must bring back the original.
  const frame = { tool: "overlay", id: "F", kind: "frame", startMs: 0, endMs: 5000, rect: { x: 10, y: 10, w: 100, h: 100 } };
  vm.runInContext("pushOp", sb)(frame);
  frame.rect = { x: 60, y: 10, w: 100, h: 100 };
  sb.VE.drag = { kind: "rect", op: frame, handle: null, start: { x: 0, y: 0 }, orig: { x: 10, y: 10, w: 100, h: 100 }, live: frame.rect, livePoint: null };
  vm.runInContext("endRectDrag()", sb);
  expect("a frame drag keeps the new rect", sb.VE.ops.find((o) => o.id === "F").rect.x === 60);
  vm.runInContext("undo()", sb);
  const back = sb.VE.ops.find((o) => o.id === "F");
  expect("undoing a frame drag restores the original rect", back.rect.x === 10 && back.rect.w === 100, JSON.stringify(back.rect));
}

// An inserted clip that carries sound must be decoded into the audio graph,
// into exactly one track, not replaced by silence.
{
  const sb = makeSandbox({ exportFormat: () => "mp4", exportQuality: () => "high", formatHasAudio: () => true });
  const clip = { tool: "clip", position: "end", missing: false, hasAudio: true, path: "C:/x/clip.mp4", durationMs: 3000 };
  const state = (hasSys, hasMic) => Object.assign(baseState([clip]), { hasSys, hasMic });
  const count = (s, needle) => s.split(needle).length - 1;
  sb.VE = state(true, true);
  let g = vm.runInContext("buildAudioChain()", sb);
  expect("clip audio is decoded with amovie", g && /amovie='C[^']*clip\.mp4'/.test(g), g);
  expect("clip audio is used in exactly one track", g && count(g, "amovie=") === 1, g);
  expect("the other track keeps a silent gap", g && count(g, "anullsrc=") === 1, g);
  sb.VE = state(false, false);
  g = vm.runInContext("buildAudioChain()", sb);
  expect("a recording without audio still gets the clip sound", g && /amovie='C[^']*clip\.mp4'/.test(g), g);
  sb.VE = state(true, false);
  sb.VE.ops = [Object.assign({}, clip, { hasAudio: false })];
  g = vm.runInContext("buildAudioChain()", sb);
  expect("a clip without audio stays a silent gap", g && !g.includes("amovie=") && g.includes("anullsrc="), g);
}

// Closing while an export runs: with the edit state not kept, the confirm must
// also warn about unexported edits.
async function closeChecks() {
  for (const [keep, dirty, warns] of [[false, true, true], [true, true, false], [false, false, false]]) {
    const texts = [];
    const sb = makeSandbox({
      t: (k) => k, veConfirm: async (title, text) => { texts.push(text); return false; },
      invoke: () => Promise.resolve(), saveEditStateNow: async () => {}, appWin: { close: () => Promise.resolve() },
    });
    sb.VE = baseState([{ tool: "cut", startMs: 0, endMs: 1000 }]);
    Object.assign(sb.VE, { exporting: true, dirty, editStateKeepOn: keep });
    await vm.runInContext("closeWindow()", sb);
    expect(`export close confirm (keep=${keep}, dirty=${dirty}) ${warns ? "warns" : "does not warn"} about unexported edits`, texts.length === 1 && texts[0].includes("veCloseUnsaved") === warns, texts.join("|"));
  }
}

closeChecks().then(() => {
if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log("PASS");
}
});
