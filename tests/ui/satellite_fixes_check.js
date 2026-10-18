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

// Overlay "Change file": the commit after the picker/probe is skipped when the overlay was removed
// or another recording was loaded meanwhile.
section(() => {
  const src = read("videoOverlays.js");
  const sb = { VE: { loadGen: 3, ops: [{ id: "o1" }] } };
  const line = src.slice(src.indexOf("export const overlayStillInEditor")).split("\n")[0].replace("export ", "");
  vm.createContext(sb);
  sb.overlayStillInEditor = vm.runInContext(line + "\noverlayStillInEditor", sb);
  check(sb.overlayStillInEditor({ id: "o1" }, 3) === true, "an overlay still in the same recording may be committed");
  check(sb.overlayStillInEditor({ id: "o1" }, 2) === false, "another recording was loaded: no commit");
  check(sb.overlayStillInEditor({ id: "o2" }, 3) === false, "the overlay was removed: no commit");
  const guards = src.match(/const gen = VE\.loadGen;\n\s+const path = await invoke\("pick_file_path"\)\.catch\(\(\) => null\);\n\s+if \(!path \|\| !overlayStillInEditor\(op, gen\)\) return;/g) || [];
  check(guards.length === 2, "both Change file handlers re-check the overlay after the picker");
  check(/probe_extra_video_source[^\n]*\n\s+if \(!overlayStillInEditor\(op, gen\)\) return;/.test(src), "the video handler re-checks after the probe");
});

// Video editor opened without a source: the "no recording" message waits for the texts and the
// label's translation hook follows it, so a language pass cannot turn it back into "Loading".
section(() => {
  const src = read("video.js");
  const at = src.indexOf('if (path) await loadRecording(path, originDir, overlayPath);');
  const tail = src.slice(at, at + 700);
  check(/await textsReady;[\s\S]*dataset\.i18n = "veNoRecording"[\s\S]*t\("veNoRecording"\)/.test(tail), "the empty-window message waits for the texts and re-points its i18n hook");
  check(/ve-stage-msg"\)\.dataset\.i18n = "veLoading"/.test(read("videoLoad.js")), "loading a recording restores the loading label's hook");
});

// Video editor: a format change keeps the destination the user chose and swaps only the extension.
section(() => {
  const src = read("video.js");
  const line = src.slice(src.indexOf("export const withExtension")).split("\n")[0].replace("export ", "");
  const sb = {};
  vm.createContext(sb);
  sb.withExtension = vm.runInContext(line + "\nwithExtension", sb);
  check(sb.withExtension("D:\\clips\\talk.mp4", "webm") === "D:\\clips\\talk.webm", "the extension is swapped, folder and name stay");
  check(sb.withExtension("D:\\my.clips\\talk", "gif") === "D:\\my.clips\\talk.gif", "a dot in a folder name is not an extension");
  check(sb.withExtension("D:\\clips\\a.b.mkv", "mp4") === "D:\\clips\\a.b.mp4", "only the last extension is replaced");
  check(!src.includes('if (id === "ve-export-format") VE.destOverride = null;') &&/VE\.destOverride = withExtension\(VE\.destOverride/.test(src), "the format dropdown no longer clears the chosen destination");
});

// Only the backend's exact "cancelled" answer is a user cancel; a failure whose text merely
// mentions "cancel" (a file name, an ffmpeg line) still shows the error with its details.
section(() => {
  check(!/raw\.includes\("cancel"\)/.test(read("videoExport.js")) && /raw\.trim\(\) === "cancelled"/.test(read("videoExport.js")), "the video export treats only the exact cancelled answer as a cancel");
  check(!/raw\.includes\("cancel"\)/.test(read("capture.js")) && /raw\.trim\(\) === "cancelled"/.test(read("capture.js")), "the recording conversion treats only the exact cancelled answer as a cancel");
});

// Video editor timeline: "Add cut" clears the selection before the redraw, and the ruler labels
// of a recording of 100 minutes or more keep their full minutes.
section(() => {
  const src = read("video.js");
  const code = ["export const pad", "export function fmtTime(", "export function rulerLabel("]
    .map((m) => (m.includes("(") ? extractBlock(src, m) : src.slice(src.indexOf(m)).split("\n")[0].replace(/;$/, "") + ";").replace("export ", ""))
    .join("\n");
  const sb = {};
  vm.createContext(sb);
  sb.rulerLabel = vm.runInContext(code + "\nrulerLabel", sb);
  check(sb.rulerLabel(125000, false) === "02:05" && sb.rulerLabel(125250, true) === "02:05.250", "ruler labels keep their short and fine forms");
  check(sb.rulerLabel(100 * 60000 + 5000, false) === "100:05", "a ruler label past 100 minutes is not cut short");
  check(sb.rulerLabel(100 * 60000, true) === "100:00.000", "a fine ruler label past 100 minutes is whole");
  const add = extractBlock(src, '$("ve-cut-add").addEventListener("click", () =>');
  check(add.indexOf("VE.sel = null") !== -1 && add.indexOf("VE.sel = null") < add.indexOf("pushOp("), "Add cut clears the selection before pushOp redraws the timeline");
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
