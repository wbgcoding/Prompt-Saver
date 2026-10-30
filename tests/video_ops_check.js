// Video editor export graph (ui/video.js): checks that pull the real source of the
// functions involved out of the file and run them against a fake editor state.
// video.js boots a real window on load, so there is nothing to import.
// Run:  node tests/video_ops_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const { videoWindowSource } = require("./ui_sources.js");
const src = videoWindowSource().replace(/\r\n/g, "\n");

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
      extractBlock("trimBounds", "function trimBounds() {"),
      extractBlock("keptSegments", "function keptSegments() {"),
      extractBlock("speedRate", "const speedRate = () => {") + ";",
      extractBlock("timelinePlan", "function timelinePlan() {"),
      extractBlock("planEntryOutMs", "function planEntryOutMs(entry) {"),
      extractBlock("outMs", "function outMs(srcMs, atEnd) {"),
      extractLine("totalOutMs", "const totalOutMs = () => timelinePlan().reduce((sum, e) => sum + planEntryOutMs(e), 0);"),
      extractLine("secs", "const secs = (ms) => (ms / 1000).toFixed(3);"),
      extractBlock("ffPath", "function ffPath(path) {"),
      extractBlock("videoOverlayWindowOutMs", "function videoOverlayWindowOutMs(op) {"),
      extractBlock("videoOverlayEnable", "function videoOverlayEnable(op) {"),
      extractBlock("videoOverlayLoopArg", "function videoOverlayLoopArg(op) {"),
      extractBlock("copyAudioMode", "function copyAudioMode() {"),
      extractBlock("isLosslessCopy", "function isLosslessCopy() {"),
      extractBlock("buildCopyMixChain", "function buildCopyMixChain() {"),
      extractBlock("silentGapLabel", "function silentGapLabel(entry, label, parts) {"),
      extractBlock("silentSpanLabel", "function silentSpanLabel(ms, label, parts) {"),
      extractBlock("clipAudioLabel", "function clipAudioLabel(entry, label, parts) {"),
      extractBlock("atempoChain", "function atempoChain(rate) {"),
      extractBlock("buildAudioChain", "function buildAudioChain() {"),
      extractBlock("pushOp", "function pushOp(op, opts) {"),
      extractBlock("pushReplacing", "function pushReplacing(match, op, opts) {"),
      extractLine("liveOverlay", "const liveOverlay = (op) => VE.ops.find((o) => o.id === op.id) || op;"),
      extractBlock("commitOverlay", "function commitOverlay(op, changes) {"),
      extractBlock("capPrevChain", "function capPrevChain(op, max) {"),
      extractLine("overlayStack", "const overlayStack = () => opsOf(\"overlay\").sort((a, b) => (a.z || 0) - (b.z || 0));"),
      extractBlock("enableExpr", "function enableExpr(op) {"),
      extractLine("hexToFf", "const hexToFf = (hex, opacity) => `0x${String(hex).replace(\"#\", \"\")}@${(clamp(opacity ?? 100, 0, 100) / 100).toFixed(2)}`;"),
      extractBlock("scaleTarget", "function scaleTarget() {"),
      extractBlock("clipVideoLabel", "function clipVideoLabel(op, label, parts, fps) {"),
      extractBlock("buildVideoChain", "function buildVideoChain() {"),
      extractBlock("undo", "function undo() {"),
      extractBlock("redo", "function redo() {"),
      extractBlock("removeOps", "function removeOps(match) {"),
      extractLine("even", "const even = (v) => Math.max(0, Math.round(v / 2) * 2);"),
      extractLine("scaleHeights", "const SCALE_HEIGHT_PRESETS = [2160, 1440, 1080, 720, 480, 360];"),
      extractLine("scalePercents", "const SCALE_PERCENT_PRESETS = [50, 25];"),
      extractBlock("effectiveSize", "function effectiveSize() {"),
      extractBlock("currentScalePresetKey", "function currentScalePresetKey() {"),
      extractBlock("changeCrop", "function changeCrop(change) {"),
      extractLine("pushCrop", "const pushCrop = (rect) => changeCrop(() => pushReplacing((o) => o.tool === \"crop\", { tool: \"crop\", rect }));"),
      extractBlock("videoKbps", "function videoKbps() {"),
      extractBlock("scaleForHeightPreset", "function scaleForHeightPreset(h) {"),
      extractBlock("scaleForPercent", "function scaleForPercent(pct) {"),
      extractBlock("applyScalePresetKey", "function applyScalePresetKey(key) {"),
      extractBlock("setScaleOp", "function setScaleOp(w, h) {"),
      extractBlock("restoreRemoved", "function restoreRemoved(marker) {"),
      extractLine("rectsEqual", "const rectsEqual = (a, b) => !!a && !!b && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;"),
      extractBlock("endRectDrag", "function endRectDrag() {"),
      extractBlock("bringForward", "async function bringForward() {"),
      extractLine("closeAsking", "let closeAsking = false;"),
      extractBlock("closeWindow", "async function closeWindow(opts) {"),
      extractBlock("closeWindowAsked", "async function closeWindowAsked(quit) {"),
      extractBlock("addAlignedCameraOverlay", "async function addAlignedCameraOverlay(path, gen) {"),
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

// Overlay panel handlers keep the overlay copy they were built with. Editing the
// volume and then the start time through that same stale copy must change only the
// field being edited (the start time used to bring the volume back to 0).
{
  const sb = makeSandbox({ afterOpsChanged() {}, rememberOverlayValues() {} });
  sb.VE = Object.assign(baseState([]), { floor: 0, redo: [], undoMax: 50 });
  const stale = { tool: "overlay", kind: "video", id: "V", startMs: 0, endMs: 9000, volume: 0, opacity: 100, loop: false };
  sb.VE.ops.push(stale);
  vm.runInContext("commitOverlay", sb)(stale, { volume: 100 });
  vm.runInContext("commitOverlay", sb)(stale, { startMs: 3000 });
  vm.runInContext("commitOverlay", sb)(stale, { endMs: 8000 });
  const cur = sb.VE.ops.find((o) => o.id === "V");
  expect("overlay volume survives a later start edit", cur.volume === 100, cur.volume);
  expect("overlay start survives a later end edit", cur.startMs === 3000, cur.startMs);
  expect("overlay end is the edited value", cur.endMs === 8000, cur.endMs);
  expect("overlay range handlers clamp against the live overlay", src.includes("Math.max(0, liveOverlay(op).endMs - OV_MIN_MS)") && src.includes("liveOverlay(op).startMs + OV_MIN_MS"));
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
  // A track the recording lacks keeps its default (muted) state and must not rule the copy out.
  sb.VE = { ...baseState([]), hasSys: true, hasMic: false, audio: { sys: { mute: false, vol: 100 }, mic: { mute: true, vol: 100 } } };
  expect("isLosslessCopy ignores the mic state without a mic track", vm.runInContext("isLosslessCopy()", sb) === true);
  sb.VE = { ...baseState([]), hasSys: true, hasMic: true, audio: { sys: { mute: false, vol: 100 }, mic: { mute: true, vol: 100 } } };
  expect("a muted mic no longer rules the stream copy out", vm.runInContext("isLosslessCopy()", sb) === true);
  expect("a muted mic keeps only the first audio stream", vm.runInContext("copyAudioMode()", sb) === "first", String(vm.runInContext("copyAudioMode()", sb)));
  // A recording with both tracks audible: the picture is copied, the sound is mixed into one stream.
  const mode = (hasSys, hasMic, sys, mic) => {
    sb.VE = { ...baseState([]), hasSys, hasMic, audio: { sys, mic } };
    return vm.runInContext("copyAudioMode()", sb);
  };
  const on = { mute: false, vol: 100 };
  expect("sys and mic both plain give a mixed copy", mode(true, true, on, on) === "mix", String(mode(true, true, on, on)));
  expect("a single sys track is copied as it is", mode(true, false, on, { mute: true, vol: 100 }) === "all");
  expect("a muted sys track needs a real re-encode", mode(true, true, { mute: true, vol: 100 }, on) === null);
  expect("a changed mic volume needs a real re-encode", mode(true, true, on, { mute: false, vol: 80 }) === null);
  expect("a changed sys volume without a mic needs a real re-encode", mode(true, false, { mute: false, vol: 80 }, on) === null);
  expect("a mic-only recording is copied as it is", mode(false, true, on, on) === "all", String(mode(false, true, on, on)));
  expect("the mix graph joins both tracks without gain loss", vm.runInContext("buildCopyMixChain()", sb) === "[0:a:0][0:a:1]amix=inputs=2:normalize=0[aout]");
  sb.VE = { ...baseState([]), hasSys: false, hasMic: false, audio: { sys: { mute: true, vol: 50 }, mic: { mute: true, vol: 100 } } };
  expect("isLosslessCopy ignores the state of absent tracks", vm.runInContext("isLosslessCopy()", sb) === true);
}

// The preview's second box of a double frame sits inside the border, at inset
// 2t..3t like the export's second drawbox.
expect("double frame preview puts its outline inside the border", src.includes("el.style.outlineOffset = `${-3 * px}px`;"));

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

// Overlays are applied after the inserted clips are concatenated in, so their
// time range has to move later by every clip that sits in front of them.
{
  const sb = makeSandbox({ exportFormat: () => "mp4", exportQuality: () => "high" });
  const text = { tool: "overlay", kind: "text", startMs: 10000, endMs: 12000 };
  const clip = (over) => Object.assign({ tool: "clip", position: "start", missing: false, hasAudio: false, path: "C:/x/clip.mp4", durationMs: 5000 }, over);
  sb.VE = baseState([clip(), text]);
  let e = vm.runInContext("enableExpr", sb)(text);
  expect("overlay window moves behind a start clip", e === "enable='between(t,15.000,17.000)'", e);
  sb.VE = baseState([clip({ position: "end" }), text]);
  e = vm.runInContext("enableExpr", sb)(text);
  expect("an end clip does not move an overlay", e === "enable='between(t,10.000,12.000)'", e);
  sb.VE = baseState([clip({ position: "cut", atMs: 4000 }), text]);
  e = vm.runInContext("enableExpr", sb)(text);
  expect("a clip cut in before the overlay moves it", e === "enable='between(t,15.000,17.000)'", e);
  sb.VE = baseState([clip({ position: "cut", atMs: 11000 }), text]);
  e = vm.runInContext("enableExpr", sb)(text);
  expect("a clip inside the overlay range stretches it", e === "enable='between(t,10.000,17.000)'", e);
  sb.VE = baseState([clip({ position: "cut", atMs: 12000 }), text]);
  e = vm.runInContext("enableExpr", sb)(text);
  expect("a clip right at the overlay end is not covered by it", e === "enable='between(t,10.000,12.000)'", e);
  sb.VE = baseState([clip(), { tool: "speed", rate: 2 }, text]);
  e = vm.runInContext("enableExpr", sb)(text);
  expect("clip length and speed combine", e === "enable='between(t,7.500,8.500)'", e);
}

// A cut-position clip whose stored position now lies past the end (trimmed away
// after the insert) is placed at the end, not dropped from the plan.
{
  const sb = makeSandbox({ exportFormat: () => "mp4", exportQuality: () => "high" });
  const clip = { tool: "clip", position: "cut", atMs: 30000, missing: false, hasAudio: false, path: "C:/x/clip.mp4", durationMs: 5000 };
  sb.VE = baseState([clip, { tool: "trim", startMs: 0, endMs: 10000 }]);
  const plan = vm.runInContext("timelinePlan()", sb);
  expect("a clip past the total goes to the end", plan.length === 2 && plan[1].type === "clip" && plan[1].op.atMs === 30000, JSON.stringify(plan.map((e) => e.type)));
  expect("the total counts that clip", vm.runInContext("totalOutMs()", sb) === 15000);
}

// With every recorded track muted (or turned down), the inserted clip's sound
// still plays at full level: it gets its own host instead of a muted track.
{
  const sb = makeSandbox({ exportFormat: () => "mp4", exportQuality: () => "high", formatHasAudio: () => true });
  const clip = { tool: "clip", position: "end", missing: false, hasAudio: true, path: "C:/x/clip.mp4", durationMs: 3000 };
  sb.VE = Object.assign(baseState([clip]), { hasSys: true, hasMic: false });
  sb.VE.audio.sys.mute = true;
  let g = vm.runInContext("buildAudioChain()", sb);
  expect("clip sound survives a muted recording track", g && g.includes("volume=1.00") && /amovie=/.test(g), g);
  expect("the muted track stays muted", g && g.includes("volume=0.00"), g);
  sb.VE.audio.sys = { mute: true, vol: 20 };
  g = vm.runInContext("buildAudioChain()", sb);
  expect("clip sound ignores the muted track's slider", g && !g.includes("volume=0.20"), g);
  sb.VE.audio.sys = { mute: false, vol: 20 };
  g = vm.runInContext("buildAudioChain()", sb);
  expect("an unmuted track still hosts the clip with its own volume", g && g.includes("volume=0.20") && !g.includes("volume=1.00"), g);
}

// Closing while an export runs: with the edit state not kept, the confirm must
// also warn about unexported edits.
async function closeChecks() {
  for (const [keep, dirty, warns] of [[false, true, true], [true, true, false], [false, false, false]]) {
    const texts = [];
    const sb = makeSandbox({
      t: (k) => k, confirmDialog: async ({ message }) => { texts.push(message); return false; },
      invoke: () => Promise.resolve(), saveEditStateNow: async () => {}, appWin: winStub().win,
    });
    sb.VE = baseState([{ tool: "cut", startMs: 0, endMs: 1000 }]);
    Object.assign(sb.VE, { exporting: true, dirty, editStateKeepOn: keep });
    await vm.runInContext("closeWindow()", sb);
    expect(`export close confirm (keep=${keep}, dirty=${dirty}) ${warns ? "warns" : "does not warn"} about unexported edits`, texts.length === 1 && texts[0].includes("veCloseUnsaved") === warns, texts.join("|"));
  }
}

function winStub() {
  const log = [];
  const ok = (name) => () => { log.push(name); return Promise.resolve(); };
  return { log, win: { close: ok("close"), unminimize: ok("unminimize"), show: ok("show"), setFocus: ok("focus") } };
}

// Every close path (button, Escape, and the backend's held-back Alt+F4 / taskbar / quit close) asks
// only when edits exist that no export covered, and lets the backend's close through only after
// the answer: "allow" before the window closes, "cancel" when the person keeps it open.
async function closeAskChecks() {
  const cases = [
    { name: "no edits closes at once", ops: [], dirty: false, answer: true, asks: false, closes: true },
    { name: "edits already exported (not dirty) close at once", ops: [{ tool: "cut", startMs: 0, endMs: 1000 }], dirty: false, answer: true, asks: false, closes: true },
    { name: "unexported edits ask, Discard closes", ops: [{ tool: "cut", startMs: 0, endMs: 1000 }], dirty: true, answer: true, asks: true, closes: true },
    { name: "unexported edits ask, Cancel keeps the window", ops: [{ tool: "cut", startMs: 0, endMs: 1000 }], dirty: true, answer: false, asks: true, closes: false },
    { name: "app quit with unexported edits asks", ops: [{ tool: "cut", startMs: 0, endMs: 1000 }], dirty: true, answer: true, asks: true, closes: true, opts: { quit: true } },
    { name: "app quit without edits closes silently", ops: [], dirty: false, answer: true, asks: false, closes: true, opts: { quit: true } },
    { name: "app quit during a clean export closes without the export question", ops: [], dirty: false, answer: true, asks: false, closes: true, opts: { quit: true }, exporting: true },
  ];
  for (const c of cases) {
    const stub = winStub();
    const acks = [];
    let asked = 0;
    const sb = makeSandbox({
      t: (k) => k, confirmDialog: async () => { asked += 1; return c.answer; },
      invoke: (cmd, args) => { if (cmd === "video_editor_close_ack") acks.push(args.state); return Promise.resolve(); },
      saveEditStateNow: async () => {}, stopEditStateAutosave: () => {}, appWin: stub.win,
    });
    sb.VE = baseState(c.ops);
    Object.assign(sb.VE, { exporting: !!c.exporting, dirty: c.dirty, editStateKeepOn: true, src: "C:/a.mp4" });
    await vm.runInContext("(o) => closeWindow(o)", sb)(c.opts);
    const closed = stub.log.includes("close");
    const detail = `asked=${asked} acks=${acks} win=${stub.log}`;
    expect(`close: ${c.name}`, (asked === 1) === c.asks && closed === c.closes && (c.closes ? acks.join() === "allow" : acks.join() === "cancel"), detail);
    if (c.closes) expect(`close: ${c.name} allows before closing`, acks[0] === "allow" && stub.log.lastIndexOf("close") === stub.log.length - 1, detail);
  }
  // A second request while the question is open is ignored (Alt+F4 pressed twice).
  const stub = winStub();
  let asked = 0;
  let answer;
  const sb = makeSandbox({
    t: (k) => k, confirmDialog: () => { asked += 1; return new Promise((r) => { answer = r; }); },
    invoke: () => Promise.resolve(), saveEditStateNow: async () => {}, stopEditStateAutosave: () => {}, appWin: stub.win,
  });
  sb.VE = baseState([{ tool: "cut", startMs: 0, endMs: 1000 }]);
  Object.assign(sb.VE, { dirty: true, src: "C:/a.mp4" });
  const closeFn = vm.runInContext("(o) => closeWindow(o)", sb);
  const first = closeFn();
  await new Promise((r) => setTimeout(r, 5));
  await closeFn();
  expect("close: a second close request while the question is open asks nothing more", asked === 1, String(asked));
  answer(false);
  await first;
}

// A recording opened while the camera overlay of the previous one is still being
// probed must not end up with that overlay.
async function cameraOverlayRaceCheck() {
  let release;
  const held = new Promise((r) => { release = r; });
  const sb = makeSandbox({
    invoke: (cmd) => (cmd === "probe_extra_video_source" ? held : Promise.resolve(null)),
    even: (n) => Math.round(n / 2) * 2, snapRect: (r) => r, newId: () => "cam1",
    afterOpsChanged: () => {},
  });
  sb.VE = baseState([]);
  Object.assign(sb.VE, { floor: 0, undoMax: 50, redo: [], loadGen: 1, w: 1920, h: 1080, selOv: null });
  const pending = vm.runInContext("addAlignedCameraOverlay", sb)("C:/cam.mp4", 1);
  sb.VE.loadGen = 2; // a second load started and reset the editor
  release({ duration_ms: 5000, has_audio: false });
  await pending;
  expect("a camera overlay from a superseded load is dropped", !sb.VE.ops.some((o) => o.path === "C:/cam.mp4"), JSON.stringify(sb.VE.ops));
  // Same load still current: the overlay arrives.
  const sb2 = makeSandbox({
    invoke: (cmd) => Promise.resolve(cmd === "probe_extra_video_source" ? { duration_ms: 5000, has_audio: false } : null),
    even: (n) => Math.round(n / 2) * 2, snapRect: (r) => r, newId: () => "cam1",
    afterOpsChanged: () => {},
  });
  sb2.VE = baseState([]);
  Object.assign(sb2.VE, { floor: 0, undoMax: 50, redo: [], loadGen: 1, w: 1920, h: 1080, selOv: null });
  await vm.runInContext("addAlignedCameraOverlay", sb2)("C:/cam.mp4", 1);
  expect("a camera overlay of the current load is added", sb2.VE.ops.some((o) => o.path === "C:/cam.mp4"));
}

// An export that finishes after another recording was opened in the editor must
// not touch that recording's unsaved flag, saved edit state or "Move to" target.
async function exportLoadRaceCheck() {
  const els = {};
  const el = (id) => els[id] || (els[id] = {
    textContent: "", value: "", disabled: false,
    classList: { add() {}, remove() {}, toggle() {} }, setAttribute() {},
  });
  let release;
  const held = new Promise((r) => { release = r; });
  const calls = [];
  const sb = {
    VE: { exporting: false, ffmpeg: true, src: "C:/a.mp4", dirty: false, editStateKeepOn: true, loadGen: 1, lastExport: "" },
    $: el, t: (k) => k, toast: () => {},
    invoke: (cmd, args) => { calls.push([cmd, args]); return cmd === "export_video" ? held : Promise.resolve(null); },
    totalOutMs: () => 5000, copyAudioMode: () => "all", buildCopyMixChain: () => "mix", exportFormat: () => "mp4", exportQuality: () => "high",
    formatExt: () => "mp4", destPath: () => "C:/out/a.mp4", showExportErrorDetails: () => {},
    exportEta: { reset() {} }, setProgress: () => {},
    buildVideoChain: () => null, buildAudioChain: () => null, exportMediaPaths: () => [], keptSegments: () => [{ a: 0, b: 5000 }],
    videoKbps: () => 1000, fmtBytes: String, LANG: "en", confirmDialog: async () => false, noteFfmpeg: () => {},
    parseFfmpegError: () => ({ message: "x", details: "" }), applyExportGate: () => {}, syncExportButton: () => {},
  };
  vm.createContext(sb);
  vm.runInContext(
    [
      extractBlock("clearExportStatus", "function clearExportStatus() {"),
      extractBlock("startExport", "async function startExport(toClipboard) {"),
    ].join(String.fromCharCode(10)),
    sb,
    { filename: "video.js (extracted)" }
  );
  const running = vm.runInContext("startExport(false)", sb);
  await new Promise((r) => setTimeout(r, 0));
  // Recording B is opened while A still exports, and gets edited.
  sb.VE.loadGen = 2;
  sb.VE.src = "C:/b.mp4";
  sb.VE.dirty = true;
  release();
  await running;
  expect("a finished export of the previous recording keeps the new one dirty", sb.VE.dirty === true);
  expect("a finished export of the previous recording leaves the new one's saved edit state", !calls.some(([c, a]) => c === "discard_edit_state" && a.target === "C:/b.mp4"), JSON.stringify(calls));
  expect("a finished export of the previous recording does not set the Move to target", sb.VE.lastExport === "");
  expect("the export flag is released", sb.VE.exporting === false);
}

// An export that stops before writing anything (no folder picked, disk warning
// declined) must not leave "Exporting" and a 0% bar behind.
async function exportIdleCheck() {
  const els = {};
  const el = (id) => els[id] || (els[id] = {
    textContent: "", value: "", disabled: false, _hidden: false,
    classList: { add(c) { if (c === "hidden") els[id]._hidden = true; }, remove(c) { if (c === "hidden") els[id]._hidden = false; }, toggle() {} },
    setAttribute() {},
  });
  const scenarios = [
    ["no folder picked", (cmd) => (cmd === "export_video" ? Promise.reject("export destination not recognized") : Promise.resolve(null)), true],
    ["disk warning declined", (cmd) => (cmd === "check_export_disk_space" ? Promise.resolve({ drive: "C:", free_mb: 1, needed_mb: 99 }) : Promise.resolve(null)), false],
  ];
  for (const [name, invokeStub, lossless] of scenarios) {
    for (const k of Object.keys(els)) delete els[k];
    const sb = {
      VE: { exporting: false, ffmpeg: true, src: "C:/a.mp4" },
      $: el, t: (k) => k, toast: () => {}, invoke: invokeStub,
      totalOutMs: () => 5000, copyAudioMode: () => (lossless ? "all" : null), buildCopyMixChain: () => "mix", exportFormat: () => "mp4", exportQuality: () => "high",
      formatExt: () => "mp4", destPath: () => "C:/out/a.mp4", showExportErrorDetails: () => {},
      exportEta: { reset() {} }, setProgress: () => el("ve-export-progress-row").classList.remove("hidden"),
      buildVideoChain: () => null, buildAudioChain: () => null, exportMediaPaths: () => [], keptSegments: () => [{ a: 0, b: 5000 }],
      videoKbps: () => 1000, fmtBytes: String, LANG: "en", confirmDialog: async () => false, noteFfmpeg: () => {},
      parseFfmpegError: () => ({ message: "x", details: "" }), applyExportGate: () => {}, syncExportButton: () => {},
    };
    vm.createContext(sb);
    vm.runInContext(
      [
        extractBlock("clearExportStatus", "function clearExportStatus() {"),
        extractBlock("startExport", "async function startExport(toClipboard) {"),
      ].join(String.fromCharCode(10)),
      sb,
      { filename: "video.js (extracted)" }
    );
    await vm.runInContext("startExport(false)", sb);
    const wantStatus = name === "no folder picked" ? "veExportDestRefused" : "";
    expect(`export that stops early (${name}) shows the right status line`, els["ve-export-status"].textContent === wantStatus, els["ve-export-status"].textContent);
    expect(`export that stops early (${name}) hides the progress row`, els["ve-export-progress-row"]._hidden === true);
  }
}

// Escape peels off one layer: a field being typed in (value restored, focus released), then the
// selected overlay, then the marked range, and only then the window.
{
  const log = [];
  const sb = {
    VE: { selOv: "o1", sel: { a: 1, b: 2 } },
    document: { activeElement: null },
    renderOverlayList: () => log.push("list"), renderOverlayEditor: () => log.push("editor"), renderRects: () => log.push("rects"),
    renderRegions: () => log.push("regions"), updateNudgeHint: () => log.push("hint"), closeWindow: () => log.push("close"),
    Event: class { constructor(type) { this.type = type; } },
    showPanel: (id) => log.push("panel:" + id),
    $: () => ({ classList: { contains: () => sb.__exportHidden } }),
    __exportHidden: true,
  };
  vm.createContext(sb);
  vm.runInContext(
    [
      extractLine("NON_TEXT_INPUT", 'const NON_TEXT_INPUT = new Set(["checkbox", "radio", "range", "button", "submit", "color", "file"]);'),
      extractLine("isTextField", 'const isTextField = (el) => !!el && (el.tagName === "TEXTAREA" || (el.tagName === "INPUT" && !NON_TEXT_INPUT.has(el.type)));'),
      "let fieldFocus = null;",
      extractBlock("handleEscape", "function handleEscape() {"),
    ].join("\n"),
    sb,
    { filename: "video.js (extracted)" }
  );
  const run = (code) => vm.runInContext(code, sb);
  const field = (value, type) => {
    const f = { tagName: "INPUT", type: type || "text", value, events: [], blurred: false };
    f.dispatchEvent = (ev) => f.events.push(ev.type);
    f.blur = () => { f.blurred = true; sb.document.activeElement = null; };
    return f;
  };
  const focusOn = (f, valueOnFocus) => {
    sb.__f = f;
    sb.document.activeElement = f;
    run(`fieldFocus = { el: __f, value: ${JSON.stringify(valueOnFocus)} }`);
  };
  // A typed-over field gets its old value back; overlay, range and window stay.
  const f = field("typed");
  focusOn(f, "name");
  expect("Escape in a field reports the field layer", run("handleEscape()") === "field");
  expect("Escape in a field restores the value it had on focus", f.value === "name", f.value);
  expect("Escape in a field announces the restore", f.events.join() === "input,change", f.events.join());
  expect("Escape in a field releases the focus", f.blurred === true);
  expect("Escape in a field keeps overlay, range and window", sb.VE.selOv === "o1" && sb.VE.sel !== null && !log.includes("close"));
  // An untouched field is only blurred.
  const g = field("same");
  focusOn(g, "same");
  run("handleEscape()");
  expect("Escape in an untouched field fires no events", g.events.length === 0 && g.blurred);
  // A checkbox is no text field: it falls through to the layers below.
  sb.document.activeElement = { tagName: "INPUT", type: "checkbox" };
  expect("Escape on a checkbox takes the overlay layer", run("handleEscape()") === "overlay" && sb.VE.selOv === null);
  sb.document.activeElement = null;
  expect("Escape then clears the marked range", run("handleEscape()") === "selection" && sb.VE.sel === null);
  expect("the window is still open", !log.includes("close"));
  sb.__exportHidden = false;
  sb.VE.exporting = true;
  expect("Escape on the Export tab during an export still closes", run("handleEscape()") === "close" && log.includes("close"));
  log.length = 0;
  sb.VE.exporting = false;
  expect("Escape on the Export tab returns to the Edit tab", run("handleEscape()") === "panel" && log.includes("panel:ve-panel-edit") && !log.includes("close"), log.join());
  sb.__exportHidden = true;
  expect("Escape with nothing left closes the window", run("handleEscape()") === "close" && log.includes("close"));
}

// The key overlay: fill under the label, label centred by its own size, no stray key for an empty
// pick, and a preview that draws the same grid. videoKeys.js is loaded whole with its imports stubbed.
{
  const keysSrc = fs.readFileSync(path.join(__dirname, "..", "ui", "videoKeys.js"), "utf8")
    .replace(/\r\n/g, "\n")
    .replace(/^import .*$/gm, "")
    .replace(/^export /gm, "");
  const sb = {
    FONT_FILES: [["segoeui", "segoeui.ttf"]], fontsDir: "F", outMs: (ms) => ms, t: (k) => k,
    VE: { keySidecar: { hold_ms: 400, events: [{ vk: 0x41, down: true, t_ms: 1000 }, { vk: 0x41, down: false, t_ms: 1200 }] } },
    commitOverlay: () => {}, liveOverlay: (o) => o,
    ffPath: (p) => p, ffText: (s) => String(s), hexToFf: (hex, pct) => `${hex}@${pct}`,
    enableExpr: (o) => (o.endMs > o.startMs ? `enable='between(t,${o.startMs / 1000},${o.endMs / 1000})'` : null),
  };
  vm.createContext(sb);
  vm.runInContext(keysSrc, sb, { filename: "videoKeys.js (extracted)" });
  const run = (code) => vm.runInContext(code, sb);
  sb.__op = { kind: "keys", preset: "game", rect: { x: 0, y: 0, w: 400, h: 250 }, color: "#ffffff", activeColor: "#0000ff", opacity: 100, startMs: 0, endMs: 5000, keys: [] };
  const filters = run("buildKeysFilters(__op)");
  const labelIdx = (ch) => filters.findIndex((f) => f.startsWith("drawtext") && f.endsWith(`text='${ch}'`));
  const aIdx = labelIdx("A");
  const fillBefore = (i) => { for (let j = i - 1; j >= 0 && !filters[j].startsWith("drawtext"); j--) if (filters[j].includes("t=fill")) return j; return -1; };
  expect("the key overlay has a label for key A", aIdx > 0);
  expect("a pressed key's fill is drawn before its label", fillBefore(aIdx) >= 0 && fillBefore(aIdx) < aIdx, filters.slice(aIdx - 3, aIdx + 1).join(" | "));
  expect("the fill sits right after the key outline", filters[fillBefore(aIdx) - 1].includes("t=2"));
  expect("a key that is never pressed gets no fill", fillBefore(labelIdx("Q")) === -1);
  // The export must match the preview, which hides the whole overlay outside its own range.
  sb.__ranged = { ...sb.__op, startMs: 10000, endMs: 20000 };
  const ranged = run("buildKeysFilters(__ranged)");
  expect("every key outline and label is limited to the overlay's range", ranged.length > 0 && ranged.filter((f) => f.includes("t=2") || f.startsWith("drawtext")).every((f) => f.includes("enable='between(t,10,20)'")), ranged.slice(0, 2).join(" | "));
  sb.__nothing = { ...sb.__op, startMs: 5000, endMs: 5000 };
  expect("an overlay with an empty range adds no filter", run("buildKeysFilters(__nothing)").length === 0);
  expect("the label is centred horizontally on its own width", /:x=\d+\+\(\d+-text_w\)\/2:/.test(filters[aIdx]), filters[aIdx]);
  expect("the label is centred vertically on its own height", /:y=\d+\+\(\d+-text_h\)\/2:/.test(filters[aIdx]), filters[aIdx]);
  expect("no fixed label offset is left", !/x=\d+:y=\d+:fontsize/.test(filters[aIdx]));
  // A long name shrinks to its key; a single letter keeps the larger size.
  sb.__wide = { ...sb.__op, preset: "custom", rect: { x: 0, y: 0, w: 200, h: 100 }, keys: [0x41, 0x20, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0x51, 0x52, 0x53] };
  const crowded = run("buildKeysFilters(__wide)");
  const sizeOf = (list, ch) => Number((list.find((f) => f.startsWith("drawtext") && f.endsWith(`text='${ch}'`)).match(/fontsize=(\d+)/) || [])[1]);
  expect("a long label is smaller than a letter on the same keys", sizeOf(crowded, "Space") < sizeOf(crowded, "A"), `${sizeOf(crowded, "Space")} vs ${sizeOf(crowded, "A")}`);
  // An empty custom pick draws nothing, and the preview says so.
  sb.__empty = { ...sb.__op, preset: "custom", keys: [] };
  expect("an empty custom pick adds no filter", run("buildKeysFilters(__empty)").length === 0);
  expect("an empty custom pick is flagged in the preview model", run("keysPreviewModel(__empty)").empty === true);
  // The preview model: same grid, the W key pressed; the pressed preset gets one sample bar.
  const grid = run("keysPreviewModel(__op)");
  expect("the preview draws the whole grid", grid.cells.length === 20 && !grid.empty && grid.bar === null, String(grid.cells.length));
  expect("the preview shows exactly one pressed sample key", grid.cells.filter((c) => c.active).length === 1 && grid.cells.find((c) => c.active).label === "W");
  sb.__pressed = { ...sb.__op, preset: "pressed" };
  const bar = run("keysPreviewModel(__pressed)");
  expect("the pressed preset previews a sample bar", bar.cells.length === 0 && bar.bar && bar.bar.w === 400 && bar.bar.label === "A");
  expect("the pressed preset still exports a fill before its label", (() => {
    const f = run("buildKeysFilters(__pressed)");
    return f.length === 2 && f[0].includes("t=fill") && f[1].startsWith("drawtext");
  })());
}

// Remembered key colours and opacity, and an overlay range that must span something.
{
  const calls = [];
  const sb = { VE: {}, invoke: (cmd, args) => { calls.push([cmd, args.key, args.value]); return Promise.resolve(); } };
  vm.createContext(sb);
  vm.runInContext(extractBlock("rememberOverlayValues", "function rememberOverlayValues(kind, changes) {"), sb);
  vm.runInContext('rememberOverlayValues("keys", { color: "#112233", activeColor: "#445566", opacity: 40 })', sb);
  expect("the key colour is remembered for the next key overlay", sb.VE.ovKeysColorDefault === "#112233");
  expect("the pressed colour is remembered for the next key overlay", sb.VE.ovKeysActiveColorDefault === "#445566");
  expect("the key opacity is remembered for the next key overlay", sb.VE.ovKeysOpacityDefault === 40);
  expect("the key defaults are saved in the settings store", ["veOvLastKeysColor", "veOvLastKeysActiveColor", "veOvLastKeysOpacity"].every((k) => calls.some((c) => c[1] === k)), JSON.stringify(calls));

  const pushed = [];
  const rsb = {
    VE: { sel: null, dur: 10000, w: 1920, h: 1080, ovColorDefault: "#ffffff", ovKeysColorDefault: "#112233", ovKeysOpacityDefault: 40, ovKeysActiveColorDefault: "#445566", ops: [] },
    opOf: (tool) => (tool === "trim" ? rsb.trim : null), trim: null,
    TXT_READY: true, textsReady: Promise.resolve(), even: (n) => Math.round(n / 2) * 2, snapRect: (r) => r,
    pushOp: (op) => pushed.push(op), showPanel: () => {}, t: (k) => k, FONT_FILES: [["segoeui", "segoeui.ttf"]], Date,
  };
  vm.createContext(rsb);
  vm.runInContext([
    extractLine("OV_MIN_MS", "const OV_MIN_MS = 100;"),
    "let overlaySeq = 0; const newId = () => `ov${++overlaySeq}`;",
    extractBlock("markedRange", "function markedRange() {"),
    extractBlock("defaultRange", "function defaultRange() {"),
    extractBlock("centeredRect", "function centeredRect(fraction) {"),
    extractBlock("addOverlay", "async function addOverlay(kind) {"),
  ].join("\n"), rsb);
  // addOverlay only awaits when the texts are not ready yet, so with them ready it runs straight through.
  {
    vm.runInContext('addOverlay("keys")', rsb);
    const k = pushed[0];
    expect("a new key overlay starts with the remembered colour", k && k.color === "#112233", k && k.color);
    expect("a new key overlay starts with the remembered opacity", k && k.opacity === 40, k && String(k.opacity));
    expect("a new key overlay starts with the remembered pressed colour", k && k.activeColor === "#445566");
    expect("a new overlay without a mark covers the whole recording", k && k.startMs === 0 && k.endMs === 10000);
    rsb.VE.sel = { a: 3000, b: 3000 };
    vm.runInContext('addOverlay("keys")', rsb);
    expect("a collapsed mark does not make a zero-length overlay", pushed[1].endMs > pushed[1].startMs, JSON.stringify([pushed[1].startMs, pushed[1].endMs]));
    rsb.VE.sel = { a: 4000, b: 2000 };
    vm.runInContext('addOverlay("keys")', rsb);
    expect("a real mark (either direction) sets the overlay range", pushed[2].startMs === 2000 && pushed[2].endMs === 4000);
    rsb.VE.sel = null;
    rsb.trim = { startMs: 500, endMs: 500 };
    vm.runInContext('addOverlay("keys")', rsb);
    expect("a degenerate trim falls back to the whole recording", pushed[3].startMs === 0 && pushed[3].endMs === 10000);

  }
}

// The export tab and the button beside Export: three tabs switch the panels, "Back" leads to the
// Edit tab while nothing runs and never closes the editor, "Stop export" cancels a running export.
{
  const mk = (id, extra) => {
    const el = { id, hidden: false, cls: new Set(), attrs: {}, dataset: {}, textContent: "", focused: false, ...extra };
    el.classList = { toggle: (name, on) => { if (on) el.cls.add(name); else el.cls.delete(name); }, contains: (name) => el.cls.has(name) };
    el.setAttribute = (k, v) => { el.attrs[k] = v; };
    el.focus = () => { el.focused = true; };
    return el;
  };
  const panels = ["ve-panel-edit", "ve-panel-overlays", "ve-panel-export"].map((id) => mk(id));
  const tabs = [["ve-tab-edit", "ve-panel-edit"], ["ve-tab-overlays", "ve-panel-overlays"], ["ve-tab-export", "ve-panel-export"]].map(([id, panel]) => mk(id, { dataset: { panel } }));
  const byId = Object.fromEntries([...panels, ...tabs].map((x) => [x.id, x]));
  byId["ve-export-cancel"] = mk("ve-export-cancel");
  const calls = [];
  const sb = {
    VE: { exporting: false },
    document: { querySelectorAll: (sel) => (sel === ".ve-panel" ? panels : sel === ".ve-tab" ? tabs : []) },
    $: (id) => byId[id], t: (k) => `<${k}>`,
    invoke: (cmd) => { calls.push(cmd); return Promise.resolve(); },
    closeWindow: () => calls.push("closeWindow"),
  };
  vm.createContext(sb);
  vm.runInContext([
    extractBlock("showPanel", "function showPanel(id) {"),
    extractBlock("syncExportButton", "function syncExportButton() {"),
    extractBlock("exportStopOrBack", "function exportStopOrBack() {"),
  ].join("\n"), sb);
  const run = (code) => vm.runInContext(code, sb);
  const shown = () => panels.filter((p) => !p.cls.has("hidden")).map((p) => p.id).join();
  run('showPanel("ve-panel-export")');
  expect("the Export tab shows the export panel only", shown() === "ve-panel-export", shown());
  expect("the Export tab is the selected one", byId["ve-tab-export"].attrs["aria-selected"] === "true" && byId["ve-tab-edit"].attrs["aria-selected"] === "false");
  expect("the html has the third tab wired to the export panel", /id="ve-tab-export"[^>]*data-panel="ve-panel-export"[^>]*data-i18n="veTabExport"/.test(fs.readFileSync(path.join(__dirname, "..", "ui", "video.html"), "utf8")));
  // Idle: the button says Back and leads to the Edit tab.
  run("syncExportButton()");
  expect("idle, the button reads Back", byId["ve-export-cancel"].textContent === "<veBack>" && byId["ve-export-cancel"].dataset.i18n === "veBack");
  run("exportStopOrBack()");
  expect("Back returns to the Edit tab", shown() === "ve-panel-edit", shown());
  expect("Back moves the focus to the Edit tab", byId["ve-tab-edit"].focused === true);
  expect("Back never closes the editor or cancels anything", calls.length === 0, calls.join());
  // Running: the button reads Stop export and cancels.
  sb.VE.exporting = true;
  run("syncExportButton()");
  expect("while exporting, the button reads Stop export", byId["ve-export-cancel"].textContent === "<veStopExport>" && byId["ve-export-cancel"].dataset.i18n === "veStopExport");
  run('showPanel("ve-panel-export")');
  run("exportStopOrBack()");
  expect("Stop export cancels the running export and stays on the panel", calls.join() === "cancel_export" && shown() === "ve-panel-export", calls.join());
  sb.VE.exporting = false;
  run("syncExportButton()");
  expect("the label goes back to Back once the export ended", byId["ve-export-cancel"].textContent === "<veBack>");
}

// Keyboard and screen reader: trim handles are millisecond sliders, mute buttons say what a click
// does and report their state, op rows are buttons that select and seek, Delete removes the op.
{
  const html = fs.readFileSync(path.join(__dirname, "..", "ui", "video.html"), "utf8");
  const tag = (id) => (html.match(new RegExp(`<[^>]*id="${id}"[^>]*>`)) || [""])[0];
  for (const id of ["ve-trim-a", "ve-trim-b"]) expect(`${id} is a slider`, /role="slider"/.test(tag(id)), tag(id));
  expect("the timeline group carries no value attributes", !/aria-value/.test(tag("ve-timeline")) && !/aria-valuenow/.test(fs.readFileSync(path.join(__dirname, "..", "ui", "video.js"), "utf8").replace(/\r\n/g, "\n").split("function renderPlayhead")[1].split("\n}\n")[0]));
  expect("the mute buttons start with aria-pressed", /aria-pressed=/.test(tag("ve-mute-sys")) && /aria-pressed=/.test(tag("ve-mute-mic")));
  expect("the export name label points at its field", /<label for="ve-export-name"/.test(html));
  expect("the shortcuts close button is a plain button", /<button[^>]*id="shortcuts-modal-close"[^>]*type="button"|<button[^>]*type="button"[^>]*id="shortcuts-modal-close"/.test(html));

  const attrs = {};
  const sb = { t: (k) => `<${k}>`, fmtTime: (ms) => `${ms}ms`, VE: { audio: { sys: { mute: false }, mic: { mute: true } } } };
  vm.createContext(sb);
  vm.runInContext([
    extractBlock("setTimeValue", "function setTimeValue(el, ms, durMs) {"),
    extractBlock("muteKey", "function muteKey(id, key, kind) {"),
  ].join("\n"), sb);
  sb.__el = { setAttribute: (k, v) => { attrs[k] = v; } };
  vm.runInContext("setTimeValue(__el, 1500, 4000)", sb);
  expect("a trim handle reports milliseconds", attrs["aria-valuenow"] === "1500" && attrs["aria-valuemax"] === "4000" && attrs["aria-valuemin"] === "0", JSON.stringify(attrs));
  expect("a trim handle reads out the time", /veA11y|a11yTimeOfTotal/.test(attrs["aria-valuetext"]), attrs["aria-valuetext"]);
  const mk = (id, key, kind) => vm.runInContext(`muteKey(${JSON.stringify(id)}, ${JSON.stringify(key)}, ${JSON.stringify(kind)})`, sb);
  expect("an unmuted system track offers Mute", mk("ve-mute-sys", "veMuteSystem", "sys") === "veMuteSystem");
  expect("a muted mic track offers Unmute", mk("ve-mute-mic", "veMuteMic", "mic") === "veUnmuteMic");
  sb.VE.audio.sys.mute = true;
  expect("a muted system track offers Unmute", mk("ve-mute-sys", "veMuteSystem", "sys") === "veUnmuteSystem");
  expect("a volume slider keeps its name when muted", mk("ve-vol-mic", "veVolumeMic", "mic") === "veVolumeMic");
  // The labels of the real function: a fixed accessible name, the tooltip swaps.
  {
    const els = {};
    const mkEl = () => ({ disabled: false, title: "", attrs: {}, setAttribute(k, v) { this.attrs[k] = v; } });
    for (const id of ["ve-mute-mic", "ve-vol-mic", "ve-mute-sys", "ve-vol-sys"]) els[id] = mkEl();
    const sb2 = { t: (k) => `<${k}>`, $: (id) => els[id], SILENT_FORMATS: new Set(), exportFormat: () => "mp4", VE: { hasMic: true, hasSys: true, audio: { sys: { mute: false }, mic: { mute: true } } } };
    vm.createContext(sb2);
    vm.runInContext([
      extractBlock("muteKey", "function muteKey(id, key, kind) {"),
      extractBlock("updateAudioLabels", "export function updateAudioLabels() {").replace("export function", "function"),
    ].join("\n"), sb2);
    vm.runInContext("updateAudioLabels()", sb2);
    expect("a muted mic button keeps its accessible name", els["ve-mute-mic"].attrs["aria-label"] === "<veMuteMic>", els["ve-mute-mic"].attrs["aria-label"]);
    expect("a muted mic button's tooltip offers Unmute", els["ve-mute-mic"].title === "<veUnmuteMic>", els["ve-mute-mic"].title);
    expect("an unmuted system button names and tips Mute", els["ve-mute-sys"].attrs["aria-label"] === "<veMuteSystem>" && els["ve-mute-sys"].title === "<veMuteSystem>");
    sb2.VE.hasMic = false;
    vm.runInContext("updateAudioLabels()", sb2);
    expect("a missing track says why in name and tooltip", els["ve-mute-mic"].attrs["aria-label"] === "<veNoMicTrack>" && els["ve-mute-mic"].title === "<veNoMicTrack>");
  }

  // Op rows: the label is a button; click selects (overlay) and seeks; Delete removes.
  const log = [];
  const osb = {
    VE: { selOv: null }, seek: (ms) => log.push(`seek${ms}`), showPanel: (id) => log.push(id), renderOverlayList: () => {}, renderOverlayEditor: () => {},
    renderRects: () => {}, updateNudgeHint: () => {}, renderRegions: () => {}, changeCrop: (fn) => { log.push("crop"); fn(); }, removeOps: () => log.push("remove"),
  };
  vm.createContext(osb);
  vm.runInContext([extractBlock("deleteOp", "function deleteOp(op) {"), extractBlock("selectOp", "function selectOp(op) {"), extractBlock("selectOverlay", "function selectOverlay(id) {")].join("\n"), osb);
  osb.__ov = { tool: "overlay", id: "o9", startMs: 700 };
  vm.runInContext("selectOp(__ov)", osb);
  expect("selecting an overlay row opens it and seeks to its start", osb.VE.selOv === "o9" && log.join() === "ve-panel-overlays,seek700", log.join());
  log.length = 0;
  osb.__tr = { tool: "trim", startMs: 0 };
  vm.runInContext("selectOp(__tr)", osb);
  expect("a plain row only seeks", log.join() === "seek0" && osb.VE.selOv === "o9", log.join());
  log.length = 0;
  osb.__sp = { tool: "speed" };
  vm.runInContext("selectOp(__sp)", osb);
  expect("a row without a start time does not seek", log.length === 0, log.join());
  osb.__cr = { tool: "crop" };
  vm.runInContext("deleteOp(__cr)", osb);
  vm.runInContext("deleteOp(__tr)", osb);
  expect("deleting a crop goes through the crop change", log.join() === "crop,remove,remove", log.join());
  const src = fs.readFileSync(path.join(__dirname, "..", "ui", "video.js"), "utf8").replace(/\r\n/g, "\n");
  expect("the op row label is a button handling Delete", /label = document\.createElement\("button"\)[\s\S]{0,400}"Delete"/.test(src));
  expect("the fade label takes its unit from the translation", /\$\{t\("veUnitMs"\)\}/.test(src) && !/op\.outMs\} ms`/.test(src));
}

// Small function fixes: one check per finding.
const pending = [];
{
  // Still frame: a second click while the first extraction runs does nothing.
  {
    let handler = null, calls = 0, release = null;
    const still = { disabled: false, addEventListener: (_, fn) => { handler = fn; } };
    const sb = {
      $: () => still, VE: { src: "a.mp4", playhead: 120, exporting: false }, t: (k) => k, toast: () => {}, noteFfmpeg: () => {}, errMessageLogged: () => "",
      invoke: () => { calls++; return new Promise((res) => { release = res; }); },
    };
    vm.createContext(sb);
    vm.runInContext(extractBlock("still", '$("ve-still").addEventListener("click", async () => {') + ");", sb);
    const first = handler();
    handler();
    expect("a second click on the still button starts no second extraction", calls === 1 && still.disabled === true, String(calls));
    release("x");
    pending.push(first.then(() => expect("the still button is usable again afterwards", still.disabled === false)));
  }
  // Marking in/out outside the kept part says why instead of doing nothing.
  {
    const handlers = {};
    const toasts = [];
    const sb = makeSandbox({
      $: (id) => ({ addEventListener: (_, fn) => { handlers[id] = fn; } }), toast: (m) => toasts.push(m), t: (k) => k, renderRegions: () => {},
    });
    sb.VE = Object.assign(baseState([{ tool: "trim", startMs: 1000, endMs: 5000 }]), { sel: null, playhead: 500 });
    vm.runInContext([
      extractBlock("playheadInKept", "function playheadInKept() {"),
      extractBlock("markIn", '$("ve-mark-in").addEventListener("click", () => {') + ");",
      extractBlock("markOut", '$("ve-mark-out").addEventListener("click", () => {') + ");",
    ].join("\n"), sb);
    handlers["ve-mark-in"]();
    expect("marking before the kept part toasts and marks nothing", toasts.join() === "veMarkOutside" && sb.VE.sel === null, toasts.join());
    sb.VE.playhead = 6000;
    handlers["ve-mark-out"]();
    expect("marking after the kept part toasts and marks nothing", toasts.length === 2 && sb.VE.sel === null);
    sb.VE.playhead = 2000;
    handlers["ve-mark-in"]();
    expect("marking inside the kept part sets the range", toasts.length === 2 && sb.VE.sel && sb.VE.sel.a === 2000);
  }
  // Home/End land where the jump buttons do; Shift reaches the raw ends.
  {
    const sb = makeSandbox({});
    sb.VE = baseState([{ tool: "trim", startMs: 1000, endMs: 8000 }, { tool: "cut", startMs: 1000, endMs: 2000 }, { tool: "cut", startMs: 7000, endMs: 8000 }]);
    sb.VE.dur = 10000;
    vm.runInContext(["const keptStart = () => keptSegments()[0].a;", "const keptEnd = () => keptSegments().slice(-1)[0].b;"].join("\n"), sb);
    expect("the shared start/end helpers mirror the jump buttons", src.includes("const keptStart = () => keptSegments()[0].a;") && src.includes("const keptEnd = () => keptSegments().slice(-1)[0].b;") && src.includes("seek(keptStart())") && src.includes("seek(keptEnd())"));
    expect("Home goes to the kept start and End to the kept end", vm.runInContext("keptStart()", sb) === 2000 && vm.runInContext("keptEnd()", sb) === 7000);
    expect("Shift+Home and Shift+End still reach the raw ends", src.includes("seek(e.shiftKey ? 0 : keptStart())") && src.includes("seek(e.shiftKey ? VE.dur : keptEnd())"));
  }
  // The arrow keys on a trim handle stop a minimum clip from the other edge.
  {
    const sb = { VE: { dur: 10000 }, MIN_CLIP_MS: 100, clamp: (v, lo, hi) => Math.min(hi, Math.max(lo, v)) };
    vm.createContext(sb);
    vm.runInContext(extractBlock("nudgeTrim", "function nudgeTrim(trim, edge, step) {"), sb);
    sb.__t = { startMs: 4000, endMs: 4200 };
    const r = vm.runInContext('nudgeTrim(__t, "a", 1000)', sb);
    expect("the start handle stops short of the end handle", r[0] === 4100 && r[1] === 4200, JSON.stringify(r));
    const r2 = vm.runInContext('nudgeTrim(__t, "b", -1000)', sb);
    expect("the end handle stops short of the start handle", r2[0] === 4000 && r2[1] === 4100, JSON.stringify(r2));
    const r3 = vm.runInContext('nudgeTrim(__t, "b", 99999)', sb);
    expect("the end handle stops at the recording end", r3[1] === 10000);
  }
  // The shaded regions use the trim exactly as the export does.
  {
    const sb = makeSandbox({});
    sb.VE = baseState([{ tool: "trim", startMs: -500, endMs: 99999 }]);
    sb.VE.dur = 10000;
    const b = vm.runInContext("trimBounds()", sb);
    expect("an out-of-range trim is held inside the recording", b.a === 0 && b.b === 10000, JSON.stringify(b));
    expect("the timeline regions read the held trim", extractBlock("renderRegions", "function renderRegions() {").includes("trimBounds()"));
  }
  // A failed filmstrip call leaves the count unset so the next request tries again.
  {
    let fail = true;
    const sb = {
      VE: { ffmpeg: true, src: "a", dur: 5000, stripCount: 0, loadGen: 1, strip: [] }, trackWidth: () => 800, THUMB_PX: 80, clamp: (v, lo, hi) => Math.min(hi, Math.max(lo, v)),
      invoke: () => (fail ? Promise.reject(new Error("x")) : Promise.resolve(["p1"])), renderStrip: () => {}, noteFfmpeg: () => {},
    };
    vm.createContext(sb);
    vm.runInContext(extractBlock("loadStrip", "async function loadStrip() {"), sb);
    pending.push(vm.runInContext("loadStrip()", sb).then(() => {
      expect("a failed filmstrip call keeps the count at zero", sb.VE.stripCount === 0, String(sb.VE.stripCount));
      fail = false;
      return vm.runInContext("loadStrip()", sb);
    }).then(() => expect("a later call fills the strip and records the count", sb.VE.stripCount === 10 && sb.VE.strip.length === 1, String(sb.VE.stripCount))));
  }
  // Undoing a removal raises the floor again; redoing it lowers it again.
  {
    const sb = makeSandbox({ afterOpsChanged() {} });
    sb.VE = Object.assign(baseState([{ tool: "cut", startMs: 0, endMs: 10 }, { tool: "cut", startMs: 20, endMs: 30 }]), { floor: 2, redo: [], undoMax: 50 });
    const run = (code) => vm.runInContext(code, sb);
    run("removeOps((o) => o.tool === 'cut')");
    expect("removing locked ops lowers the floor", sb.VE.floor === 0, String(sb.VE.floor));
    run("undo()");
    expect("undoing the removal restores the floor", sb.VE.floor === 2 && sb.VE.ops.length === 2, String(sb.VE.floor));
    run("undo()");
    expect("undo cannot step below the original floor", sb.VE.ops.length === 2);
    run("redo()");
    expect("redoing the removal lowers the floor again", sb.VE.floor === 0 && sb.VE.ops.filter((o) => o.tool !== "removal").length === 0, String(sb.VE.floor));
  }
  // A typed destination takes the extension of the chosen format.
  {
    let handler = null;
    const field = { value: " D:/out/clip.gif ", title: "", addEventListener: (_, fn) => { handler = fn; } };
    const sb = {
      $: () => field, VE: { destOverride: null }, exportFormat: () => "mp4", formatExt: (f) => f, updateExportNote: () => {},
      withExtension: (p, ext) => `${String(p).replace(/\.[^\\/.]+$/, "")}.${ext}`, destPath: () => sb.VE.destOverride || "auto.mp4",
    };
    vm.createContext(sb);
    vm.runInContext(extractBlock("dest", '$("ve-export-dest").addEventListener("change", () => {') + ");", sb);
    handler();
    expect("a typed clip.gif becomes clip.mp4 for the mp4 format", sb.VE.destOverride === "D:/out/clip.mp4" && field.value === "D:/out/clip.mp4", sb.VE.destOverride);
    field.value = "  ";
    handler();
    expect("an emptied field goes back to the automatic name", sb.VE.destOverride === null);
  }
  // Shift-drag on a corner keeps the opposite corner where it was.
  {
    const sb = {};
    vm.createContext(sb);
    vm.runInContext(extractBlock("resizeRect", "function resizeRect(orig, handle, point, keepRatio) {"), sb);
    sb.__o = { x: 100, y: 100, w: 200, h: 100 };
    const nw = vm.runInContext('resizeRect(__o, "nw", { x: 0, y: 40 }, true)', sb);
    expect("a north-west corner keeps the south-east corner fixed", nw.x + nw.w === 300 && nw.y + nw.h === 200 && Math.abs(nw.w / nw.h - 2) < 1e-9, JSON.stringify(nw));
    const se = vm.runInContext('resizeRect(__o, "se", { x: 400, y: 150 }, true)', sb);
    expect("a south-east corner keeps the north-west corner fixed", se.x === 100 && se.y === 100 && se.w === 300 && se.h === 150, JSON.stringify(se));
    const ne = vm.runInContext('resizeRect(__o, "ne", { x: 400, y: 0 }, true)', sb);
    expect("a north-east corner keeps the south-west corner fixed", ne.x === 100 && ne.y + ne.h === 200 && ne.w === 300, JSON.stringify(ne));
    const free = vm.runInContext('resizeRect(__o, "nw", { x: 50, y: 50 }, false)', sb);
    expect("without Shift the corner follows the pointer", free.x === 50 && free.y === 50 && free.w === 250 && free.h === 150);
  }
}

// The export looks like the preview: the soft frame is a halo OUTSIDE the main border (the
// preview draws a glow), the two overlay selects refresh in place, the copy note names the
// keyframe start, and the narrow-window stage and the tab radii follow the design.
{
  const sb = makeSandbox({
    exportFormat: () => "mp4", exportQuality: () => "high", formatHasAudio: () => true,
    SILENT_FORMATS: new Set(["gif"]), MAX_SILENT_DIM: 1280, afterOpsChanged: () => {},
  });
  sb.VE = baseState([]);
  Object.assign(sb.VE, { floor: 0, undoMax: 50, redo: [], w: 1920, h: 1080, fps: 30 });
  vm.runInContext("pushOp", sb)({ tool: "overlay", id: "S", kind: "frame", startMs: 0, endMs: 10000, rect: { x: 100, y: 80, w: 400, h: 300 }, color: "#ff0000", opacity: 100, thickness: 4, style: "soft" });
  const g = vm.runInContext("buildVideoChain()", sb) || "";
  const boxes = (g.match(/drawbox=[^,;\[\]]*/g) || []);
  expect("a soft frame draws the border and one outer box", boxes.length === 2, boxes.join(" | "));
  expect("the soft halo starts outside the frame by three borders", /drawbox=x=88:y=68:w=424:h=324:color=0xff0000@0\.4\d*:t=12/.test(g), g);
  expect("the soft halo does not paint over the picture inside the border", /drawbox=x=88:y=68:w=424:h=324:[^,;]*:t=12/.test(g), g);
  const overlays = fs.readFileSync(path.join(__dirname, "..", "ui", "videoOverlays.js"), "utf8").replace(/\r\n/g, "\n");
  expect("the text background select refreshes in place", /ovTrack\("back", selectInput\(/.test(overlays));
  expect("the keys preset select refreshes in place", /ovTrack\("preset", selectInput\(/.test(overlays));
  expect("no unused preview zoom limit is left", !/PREVIEW_ZOOM_MAX/.test(src));
  const css = fs.readFileSync(path.join(__dirname, "..", "ui", "styles", "video.css"), "utf8").replace(/\r\n/g, "\n");
  expect("the sticky preview is capped on short windows", /--ve-stage-h: min\(40vh, 260px\)/.test(css) && !/\b40vh;|40vh\)/.test(css.split("@media (max-width: 900px)")[1].split("}")[0] + css.split("@media (max-width: 900px)")[1].split("}")[1]));
  expect("the tabs and the toast use the design radius", !/border-radius: 8px/.test(css.slice(css.indexOf(".ve-tab {"), css.indexOf(".ve-tab:hover"))) && !/border-radius: 8px/.test(css.slice(css.indexOf(".ve-toast {"), css.indexOf(".ve-toast {") + 300)));
  const en = fs.readFileSync(path.join(__dirname, "..", "ui", "i18n", "en.js"), "utf8");
  expect("the copy note names the keyframe start", /veCopyNote: "[^"]*nearest keyframe before your start point\."/.test(en));
  const exportSrc = fs.readFileSync(path.join(__dirname, "..", "ui", "videoExport.js"), "utf8");
  expect("the mixed-sound copy has its own note", /copyMode === "mix" \? "veCopyMixNote" : "veCopyNote"/.test(exportSrc) && /veCopyMixNote: "[^"]*mixed into one\./.test(en));
}

// Clicking the preview picks the overlay under the pointer: the top-most one showing at the
// playhead, a hollow frame only on its border, a text overlay on its estimated box.
{
  const mkOps = () => [
    { tool: "overlay", id: "A", kind: "pixelate", startMs: 0, endMs: 10000, rect: { x: 100, y: 100, w: 400, h: 300 } },
    { tool: "overlay", id: "B", kind: "spotlight", startMs: 0, endMs: 10000, rect: { x: 200, y: 150, w: 400, h: 300 } },
    { tool: "overlay", id: "F", kind: "frame", startMs: 2000, endMs: 4000, rect: { x: 700, y: 100, w: 400, h: 300 }, thickness: 6 },
    { tool: "overlay", id: "T", kind: "text", startMs: 0, endMs: 10000, x: 800, y: 600, fontPx: 40, text: "Hello", back: "none" },
    { tool: "crop", rect: { x: 0, y: 0, w: 1920, h: 1080 } },
  ];
  const sb = {
    VE: { playhead: 1000, ops: mkOps(), selOv: null },
    opsOf: (tool) => sb.VE.ops.filter((o) => o.tool === tool),
  };
  vm.createContext(sb);
  vm.runInContext(["const overlayStack = () => opsOf('overlay').sort((a, b) => (a.z || 0) - (b.z || 0));", extractBlock("overlayAt", "function overlayAt(point, tol) {")].join("\n"), sb);
  const at = (x, y, tol = 6) => { sb.__p = { x, y }; const r = vm.runInContext("overlayAt(__p, " + tol + ")", sb); return r ? r.id : null; };
  expect("where two overlays overlap, the one on top is picked", at(300, 200) === "B", String(at(300, 200)));
  expect("outside the upper one the lower one is picked", at(150, 120) === "A", String(at(150, 120)));
  expect("an empty spot picks nothing, the crop box is no overlay", at(1500, 900) === null);
  expect("a text overlay is picked on its estimated box", at(850, 620) === "T" && at(900, 700) === null);
  expect("a frame that is not showing yet is not picked", at(702, 250) === null);
  sb.VE.playhead = 3000;
  expect("a showing frame is picked on its border", at(702, 250) === "F", String(at(702, 250)));
  expect("the hollow middle of a frame picks nothing", at(900, 250) === null, String(at(900, 250)));
  sb.VE.playhead = 4001;
  expect("after the end time the frame is gone again", at(702, 250) === null);
  sb.VE.playhead = 1000;
  expect("the slack makes a point just outside a rectangle hit", at(96, 250) === "A" && at(90, 250, 6) === null);
  // The stacking order follows z, like the export.
  sb.VE.ops = mkOps();
  sb.VE.ops.find((o) => o.id === "A").z = 5;
  expect("a higher z wins over a later list position", at(300, 200) === "A", String(at(300, 200)));
}

// Selecting by click and by list row share one function; the click does not move the playhead,
// and a click on empty picture deselects only when the pointer did not travel.
{
  const calls = [];
  const sb = {
    VE: { selOv: null }, showPanel: (id) => calls.push("panel:" + id), renderOverlayList: () => calls.push("list"),
    renderOverlayEditor: () => calls.push("editor"), renderRects: () => calls.push("rects"), updateNudgeHint: () => calls.push("hint"),
    renderRegions: () => calls.push("regions"),
  };
  vm.createContext(sb);
  vm.runInContext(extractBlock("selectOverlay", "export function selectOverlay(id) {".replace("export ", "")), sb);
  vm.runInContext('selectOverlay("X")', sb);
  expect("selecting opens the overlays panel and refreshes list, editor, rects, hint and bars", sb.VE.selOv === "X" && calls.join() === "panel:ve-panel-overlays,list,editor,rects,hint,regions", calls.join());
  calls.length = 0;
  vm.runInContext("selectOverlay(null)", sb);
  expect("deselecting keeps the panel and clears the selection", sb.VE.selOv === null && !calls.some((c) => c.startsWith("panel:")) && calls.includes("rects"), calls.join());
  const sel = extractBlock("selectOp", "function selectOp(op) {");
  expect("the list row uses the shared selection and still seeks", /selectOverlay\(op\.id\)/.test(sel) && /seek\(op\.startMs\)/.test(sel));
  const stage = src.slice(src.indexOf('$("ve-stage").addEventListener("pointerdown"'), src.indexOf('$("ve-stage").addEventListener("pointermove"'));
  expect("a press on a crop or overlay box or on the zoom pill is not taken by the stage", stage.indexOf('closest(".ve-rect")') < stage.indexOf("grabOverlayAt") && stage.indexOf('closest("#ve-preview-zoom")') < stage.indexOf("grabOverlayAt"));
  expect("an active drag or pan still blocks the stage handler first", stage.indexOf("if (VE.drag) return;") < stage.indexOf("grabOverlayAt"));
  expect("only the primary button picks an overlay", /e\.button === 0\) \{\s*if \(grabOverlayAt\(e\)\) return;/.test(stage));
  expect("panning at zoom above 1 stays reachable after an empty press", stage.indexOf("grabOverlayAt") < stage.indexOf("VE.previewZoom <= 1") && stage.includes('kind: "pan"'));
  const rel = src.slice(src.indexOf('$("ve-stage").addEventListener("pointerup"'), src.indexOf('$("ve-stage").addEventListener("pointercancel"'));
  expect("an empty click deselects only without travel", /Math\.hypot\([^)]*\) < 4\) selectOverlay\(null\)/.test(rel) && /press && VE\.selOv/.test(rel));
  const rects = src.slice(src.indexOf('$("ve-rects").addEventListener("pointerdown"'), src.indexOf('$("ve-rects").addEventListener("pointermove"'));
  expect("an overlay inside the crop box wins over moving the box, handles stay with the box", /op\.tool === "crop" && !handle && grabOverlayAt\(e\)/.test(rects));
}

// Removing an operation is an undo step: undo brings it back with every
// field in its old slot, redo removes it again, earlier edits stay as they were.
{
  const sb = makeSandbox({ afterOpsChanged() {} });
  sb.VE = Object.assign(baseState([]), { floor: 0, redo: [], undoMax: 50 });
  const run = (code) => vm.runInContext(code, sb);
  const crop = { tool: "crop", rect: { x: 1, y: 2, w: 300, h: 200 } };
  const text = { tool: "overlay", id: "T", kind: "text", startMs: 100, endMs: 900, text: "hi", z: 1 };
  const speed = { tool: "speed", rate: 2, pitch: true };
  run("pushOp")(crop);
  run("pushOp")(text);
  run("pushOp")(speed);
  const live = () => sb.VE.ops.filter((o) => o.tool !== "removal");
  run("removeOps((o) => o.id === 'T')");
  expect("removing an overlay takes it out of the list", !live().some((o) => o.id === "T") && live().length === 2);
  run("undo()");
  expect("undo restores the removed overlay in its slot", sb.VE.ops.length === 3 && sb.VE.ops[1] === text && sb.VE.ops[1].text === "hi" && sb.VE.ops[1].endMs === 900, JSON.stringify(sb.VE.ops.map((o) => o.tool)));
  expect("undo of a removal leaves earlier edits alone", sb.VE.ops[0] === crop && sb.VE.ops[2] === speed);
  run("redo()");
  expect("redo removes the overlay again", !live().some((o) => o.id === "T") && live().length === 2);
  run("undo()");
  run("undo()");
  expect("a second undo steps back one edit, not the removal twice", sb.VE.ops.length === 2 && !sb.VE.ops.some((o) => o.tool === "speed"));
  sb.VE.ops = []; sb.VE.redo = [];
  run("pushOp")({ tool: "cut", startMs: 0, endMs: 10 });
  run("pushOp")({ tool: "cut", startMs: 20, endMs: 30 });
  run("removeOps((o) => o.tool === 'cut')");
  run("undo()");
  expect("removing several ops at once is one undo step", sb.VE.ops.length === 2 && sb.VE.ops[0].startMs === 0 && sb.VE.ops[1].startMs === 20);
  expect("removeOps with no match pushes nothing", (() => { const n = sb.VE.ops.length; run("removeOps((o) => o.tool === 'fade')"); return sb.VE.ops.length === n; })());
  expect("the removal marker never reaches the ops list or the saved state", src.includes('op.tool !== "clip" && op.tool !== "removal"') && src.includes('VE.ops.filter((o) => o.tool !== "removal")'));
}

// A finished export clears "unsaved changes" only when nothing was edited while it ran
// and the same recording is still open.
{
  const sb = makeSandbox({});
  sb.VE = { loadGen: 4, rev: 10 };
  vm.runInContext(extractBlock("exportCoveredEdits", "function exportCoveredEdits(gen, rev) {"), sb);
  const covered = vm.runInContext("exportCoveredEdits", sb);
  expect("an untouched editor is covered by its export", covered(4, 10) === true);
  expect("an edit made while the export ran is not covered", (() => { sb.VE.rev = 11; return covered(4, 10) === false; })());
  expect("another recording is never covered", (() => { sb.VE.rev = 10; sb.VE.loadGen = 5; return covered(4, 10) === false; })());
  expect("every ops change bumps the revision", /function afterOpsChanged\(opts\) \{[^}]*VE\.rev \+= 1;/.test(src));
}

// The undo cap limits how far back undo reaches, never what the preview and the
// export render: a trim made before the cap was hit stays in the list.
{
  const sb = makeSandbox({ afterOpsChanged() {} });
  sb.VE = Object.assign(baseState([]), { floor: 0, redo: [], undoMax: 3 });
  const run = (code) => vm.runInContext(code, sb);
  run("pushOp")({ tool: "trim", startMs: 500, endMs: 9000 });
  for (let i = 0; i < 6; i++) run("pushOp")({ tool: "cut", startMs: i * 100, endMs: i * 100 + 50 });
  expect("a trim older than the undo window is still rendered", sb.VE.ops.some((o) => o.tool === "trim"), JSON.stringify(sb.VE.ops.map((o) => o.tool)));
  expect("every cut is still in the list", sb.VE.ops.filter((o) => o.tool === "cut").length === 6);
  for (let i = 0; i < 10; i++) run("undo()");
  expect("undo reaches back only the cap", sb.VE.ops.length === 4 && sb.VE.ops.some((o) => o.tool === "trim"), JSON.stringify(sb.VE.ops.map((o) => o.tool)));
}

// An explicit height preset may enlarge (with its hint); the "small for chat"
// and GIF presets only ever shrink; the source size is "no scaling" and removes
// the entry.
{
  const sb = makeSandbox({ afterOpsChanged() {} });
  sb.VE = Object.assign(baseState([]), { floor: 0, redo: [], undoMax: 50, w: 640, h: 360 });
  const run = (code) => vm.runInContext(code, sb);
  const scaleOp = () => sb.VE.ops.find((o) => o.tool === "scale") || null;
  run("applyScalePresetKey('720')");
  expect("an explicit 720p preset on a 360p source enlarges", !!scaleOp() && scaleOp().h === 720 && scaleOp().w === 1280, JSON.stringify(scaleOp()));
  const fields = {};
  sb.$ = (id) => (fields[id] = fields[id] || { value: "" });
  sb.persistExportSettings = () => {};
  sb.updateExportNote = () => {};
  run(extractBlock("applyPreset", "function applyPreset(format, quality, downscale) {"));
  run("applyPreset('mp4', 'small', '720')");
  expect("the chat preset on a smaller source keeps the source size", scaleOp() === null, JSON.stringify(scaleOp()));
  run("applyPreset('gif', 'small', '480')");
  expect("the GIF preset on a smaller source keeps the source size", scaleOp() === null, JSON.stringify(scaleOp()));
  sb.VE.w = 1280; sb.VE.h = 720;
  run("applyPreset('gif', 'small', '480')");
  expect("the GIF preset on a larger source shrinks", !!scaleOp() && scaleOp().h === 480, JSON.stringify(scaleOp()));
  run("applyScalePresetKey('720')");
  expect("a preset equal to the source adds no entry", scaleOp() === null);
  const p200 = run("scaleForPercent(200)");
  run("setScaleOp")(p200.w, p200.h);
  expect("the percent field may still enlarge", !!scaleOp() && scaleOp().w === 2560 && scaleOp().h === 1440, JSON.stringify(scaleOp()));
  const p100 = run("scaleForPercent(100)");
  run("setScaleOp")(p100.w, p100.h);
  expect("100 percent removes the scale entry", scaleOp() === null);
  run("setScaleOp")(640, 360);
  run("setScaleOp")(1280, 720);
  expect("typing the source size removes the scale entry", scaleOp() === null);
}

// Scale presets, the percent field and the size estimate work on the picture the crop leaves,
// not on the source: a 1920x1080 source cropped to 1280x800 and set to 720p is 1152x720 (the
// source aspect would have given a stretched 1280x720). A new crop works the scale out again.
{
  const sb = makeSandbox({
    afterOpsChanged() {}, exportQuality: () => "high", EXPORT_KBPS: { high: 12000, small: 4000, lossless: null },
    $: (id) => ({ checked: id === "ve-scale-lock" ? sb.lock : false }), lock: true,
  });
  sb.VE = Object.assign(baseState([]), { floor: 0, redo: [], undoMax: 50, w: 1920, h: 1080 });
  const run = (code) => vm.runInContext(code, sb);
  const scaleOp = () => sb.VE.ops.find((o) => o.tool === "scale") || null;
  const size = (o) => (o ? `${o.w}x${o.h}` : "none");
  run("pushCrop")({ x: 320, y: 140, w: 1280, h: 800 });
  run("applyScalePresetKey('720')");
  expect("720p on a 1280x800 crop is 1152x720", size(scaleOp()) === "1152x720", size(scaleOp()));
  expect("the effective size is the crop rectangle", JSON.stringify(run("effectiveSize()")) === JSON.stringify({ w: 1280, h: 800 }));
  expect("the size estimate is worked out from crop then scale", run("videoKbps()") === Math.round(12000 * Math.sqrt((1152 * 720) / (1920 * 1080))), String(run("videoKbps()")));
  // The crop changes: a preset follows it, so the same 720p is worked out for the new rectangle.
  run("pushCrop")({ x: 0, y: 0, w: 1600, h: 900 });
  expect("a 720p preset is re-applied to the new crop", size(scaleOp()) === "1280x720", size(scaleOp()));
  run("pushCrop")({ x: 0, y: 0, w: 1001, h: 801 });
  expect("scale dimensions stay even", scaleOp() && scaleOp().w % 2 === 0 && scaleOp().h % 2 === 0, size(scaleOp()));
  // A crop that already is 720 high needs no scale, so the entry goes away.
  run("pushCrop")({ x: 0, y: 0, w: 1440, h: 720 });
  expect("a preset equal to the new crop height drops the entry", scaleOp() === null, size(scaleOp()));
  run("pushCrop")({ x: 0, y: 0, w: 1600, h: 900 });
  run("applyScalePresetKey('720')");
  // A free size with the ratio lock keeps its height and takes the aspect of the new crop.
  run("setScaleOp")(1000, 562);
  run("pushCrop")({ x: 0, y: 0, w: 1000, h: 1000 });
  expect("a locked free size takes the new aspect", size(scaleOp()) === "562x562", size(scaleOp()));
  // A crop without any scale adds none, and the estimate follows the crop alone.
  run("removeOps((o) => o.tool === 'scale')");
  run("pushCrop")({ x: 0, y: 0, w: 960, h: 540 });
  expect("a crop without a scale op adds none", scaleOp() === null);
  expect("the estimate follows a crop alone", run("videoKbps()") === 6000, String(run("videoKbps()")));
  // The preset equal to the cropped height needs no op, like the source height before.
  run("applyScalePresetKey('540')");
  expect("a preset equal to the crop height adds no entry", scaleOp() === null);
}

// A refresh rewrites every field except one the user is typing in right now.
{
  const els = {};
  const fake = (id, value) => (els[id] = { id, value, checked: false, disabled: false, classList: { toggle() {} }, querySelectorAll: () => [], contains(n) { return n === this || (this.kids || []).includes(n); }, numFieldRefresh() {}, dataset: {} });
  const ids = ["ve-trim-start-val", "ve-trim-end-val", "ve-crop-on", "ve-crop-fields", "ve-crop-x", "ve-crop-y", "ve-crop-w", "ve-crop-h", "ve-speed-steps", "ve-speed-custom", "ve-speed-pitch", "ve-fade-on", "ve-fade-fields", "ve-fade-in", "ve-fade-out"];
  for (const id of ids) fake(id, "");
  els["ve-crop-fields"].kids = [els["ve-crop-x"], els["ve-crop-y"], els["ve-crop-w"], els["ve-crop-h"]];
  const sb = makeSandbox({ $: (id) => els[id], fmtTime: (ms) => String(ms), HTMLInputElement: function () {} });
  vm.runInContext([
    extractLine("uncommitted", "const uncommittedFields = new Set();"),
    extractBlock("isUncommitted", "function isUncommitted(el) {"),
    extractBlock("renderEditFields", "function renderEditFields() {"),
  ].join(String.fromCharCode(10)), sb);
  sb.VE = Object.assign(baseState([{ tool: "crop", rect: { x: 0, y: 0, w: 1920, h: 1080 } }]), { w: 1920, h: 1080 });
  els["ve-crop-w"].value = "640";
  vm.runInContext("renderEditFields()", sb);
  expect("a reset refreshes a focused, committed crop field", els["ve-crop-w"].value === 1920 && els["ve-crop-h"].value === 1080 && els["ve-crop-x"].value === 0, els["ve-crop-w"].value);
  els["ve-crop-w"].value = "6";
  vm.runInContext("uncommittedFields", sb).add(els["ve-crop-w"]);
  vm.runInContext("renderEditFields()", sb);
  expect("a field mid-typing keeps its text", els["ve-crop-w"].value === "6", els["ve-crop-w"].value);
  expect("only the uncommitted crop field is spared", els["ve-crop-h"].value === 1080);
  expect("no refresh guard is tied to focus any more", !/document\.activeElement\s*!==\s*\$\("ve-(trim|speed|fade|scale)/.test(src));
}

// A path with an apostrophe must survive both of ffmpeg's parsing levels (graph, then option
// list), read the way the backend's graph check reads them: av_get_token with quotes and
// backslash escapes.
{
  const avToken = (text, from, terms) => {
    let i = from;
    let out = "";
    while (i < text.length && !terms.includes(text[i])) {
      const ch = text[i++];
      if (ch === "\\" && i < text.length) out += text[i++];
      else if (ch === "'") {
        while (i < text.length && text[i] !== "'") out += text[i++];
        if (i < text.length) i++;
      } else out += ch;
    }
    return { value: out, end: i };
  };
  const sb = makeSandbox();
  for (const p of ["C:/Users/Example/clip.mp4", "C:/Users/Example's/Example's logo.png", "D:/it''s/a'b'c.png", "E:/x/'.png"]) {
    const arg = vm.runInContext("ffPath", sb)(p);
    const lvl1 = avToken(`'${arg}'[a]`, 0, ["[", "]", ",", ";"]).value;
    const lvl2 = avToken(lvl1, 0, [":"]);
    expect(`ffPath survives both parsing levels: ${p}`, lvl2.value === p && lvl2.end === lvl1.length, lvl2.value);
  }
}

cameraOverlayRaceCheck().then(() => Promise.all(pending)).then(exportIdleCheck).then(exportLoadRaceCheck).then(closeChecks).then(closeAskChecks).then(() => {
if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log("PASS");
}
});
