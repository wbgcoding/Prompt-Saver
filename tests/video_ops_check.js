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
      extractBlock("isLosslessCopy", "function isLosslessCopy() {"),
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
      extractBlock("closeWindow", "async function closeWindow() {"),
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
  expect("overlay range handlers clamp against the live overlay", src.includes("clamp(ms, 0, liveOverlay(op).endMs)") && src.includes("clamp(ms, liveOverlay(op).startMs, VE.dur)"));
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
  expect("isLosslessCopy is false when an existing mic track is muted", vm.runInContext("isLosslessCopy()", sb) === false);
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
      invoke: () => Promise.resolve(), saveEditStateNow: async () => {}, appWin: { close: () => Promise.resolve() },
    });
    sb.VE = baseState([{ tool: "cut", startMs: 0, endMs: 1000 }]);
    Object.assign(sb.VE, { exporting: true, dirty, editStateKeepOn: keep });
    await vm.runInContext("closeWindow()", sb);
    expect(`export close confirm (keep=${keep}, dirty=${dirty}) ${warns ? "warns" : "does not warn"} about unexported edits`, texts.length === 1 && texts[0].includes("veCloseUnsaved") === warns, texts.join("|"));
  }
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
    totalOutMs: () => 5000, isLosslessCopy: () => true, exportFormat: () => "mp4", exportQuality: () => "high",
    formatExt: () => "mp4", destPath: () => "C:/out/a.mp4", showExportErrorDetails: () => {},
    exportEta: { reset() {} }, setProgress: () => {},
    buildVideoChain: () => null, buildAudioChain: () => null, exportMediaPaths: () => [], keptSegments: () => [{ a: 0, b: 5000 }],
    videoKbps: () => 1000, fmtBytes: String, LANG: "en", confirmDialog: async () => false, noteFfmpeg: () => {},
    parseFfmpegError: () => ({ message: "x", details: "" }), applyExportGate: () => {},
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
      totalOutMs: () => 5000, isLosslessCopy: () => lossless, exportFormat: () => "mp4", exportQuality: () => "high",
      formatExt: () => "mp4", destPath: () => "C:/out/a.mp4", showExportErrorDetails: () => {},
      exportEta: { reset() {} }, setProgress: () => el("ve-export-progress-row").classList.remove("hidden"),
      buildVideoChain: () => null, buildAudioChain: () => null, exportMediaPaths: () => [], keptSegments: () => [{ a: 0, b: 5000 }],
      videoKbps: () => 1000, fmtBytes: String, LANG: "en", confirmDialog: async () => false, noteFfmpeg: () => {},
      parseFfmpegError: () => ({ message: "x", details: "" }), applyExportGate: () => {},
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
  for (const p of ["C:/Users/Ben/clip.mp4", "C:/Users/Ben's/Ben's logo.png", "D:/it''s/a'b'c.png", "E:/x/'.png"]) {
    const arg = vm.runInContext("ffPath", sb)(p);
    const lvl1 = avToken(`'${arg}'[a]`, 0, ["[", "]", ",", ";"]).value;
    const lvl2 = avToken(lvl1, 0, [":"]);
    expect(`ffPath survives both parsing levels: ${p}`, lvl2.value === p && lvl2.end === lvl1.length, lvl2.value);
  }
}

cameraOverlayRaceCheck().then(exportIdleCheck).then(exportLoadRaceCheck).then(closeChecks).then(() => {
if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log("PASS");
}
});
