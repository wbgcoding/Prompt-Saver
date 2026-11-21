// Video editor Stop (ui/videoExport.js startExport / requestExportCancel): a Stop clicked before
// the backend has an export to cancel (disk-space question, clip temp name, export-folder
// picker) is remembered and ends the run as a cancel, instead of being dropped while the export
// still runs. Runs the real extracted source in a node vm.
// Run:  node tests/video_export_cancel_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "ui", "videoExport.js"), "utf8").replace(/\r\n/g, "\n");

function extractBlock(marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find ${marker} (source text changed?)`);
  const braceStart = src.indexOf("{", start);
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1).replace(/^export /, "");
  }
  throw new Error(`unbalanced braces extracting ${marker}`);
}

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

function deferred() {
  let resolve;
  const promise = new Promise((r) => { resolve = r; });
  return { promise, resolve };
}

function sandbox(over) {
  const els = {};
  const el = (id) => els[id] || (els[id] = { id, textContent: "", disabled: false, value: "", checked: false, open: false, classList: { add() {}, remove() {}, toggle() {} }, setAttribute() {} });
  const sb = Object.assign({
    VE: { exporting: false, loadGen: 1, rev: 0, src: "C:/rec/a.mp4", ffmpeg: true, dirty: false, destOverride: "", originDir: "" },
    calls: [], toasts: [], els,
    $: el, t: (k) => k, toast: (m) => sb.toasts.push(m),
    totalOutMs: () => 5000, copyAudioMode: () => null, exportFormat: () => "mp4", exportQuality: () => "high",
    destPath: () => "C:/out/a.mp4", formatExt: () => "mp4", winPath: (p) => p,
    errMessageLogged: (e) => String(e), syncExportButton() {}, applyExportGate() {},
    showExportErrorDetails() {}, exportEta: { reset() {} }, setProgress() {},
    buildVideoChain: () => "", buildAudioChain: () => "", buildCopyMixChain: () => "", exportMediaPaths: () => [],
    videoKbps: () => 1000, keptSegments: () => [{ a: 0, b: 5000 }], fmtBytes: () => "1 MB", LANG: "en",
    confirmDialog: async () => true, noteFfmpeg() {}, parseFfmpegError: (e) => ({ message: String(e), details: "" }),
    addExportTile: async () => {}, exportRun: null,
    invoke: async (cmd) => { sb.calls.push(cmd); return null; },
  }, over || {});
  vm.createContext(sb);
  vm.runInContext(
    [
      "let exportCancelRequested = false;",
      "function exportCoveredEdits(gen, rev) { return VE.loadGen === gen && VE.rev === rev; }",
      "function clearExportStatus() { $('ve-export-status').textContent = ''; }",
      extractBlock("export function requestExportCancel() {"),
      extractBlock("export async function startExport(toClipboard) {"),
    ].join("\n"),
    sb,
    { filename: "videoExport.js (extracted)" }
  );
  return sb;
}

const tick = async () => { for (let i = 0; i < 5; i++) await Promise.resolve(); };
const exportCalls = (sb) => sb.calls.filter((c) => c === "export_video").length;

(async () => {
  {
    // Stop while the disk-space question is open, then "Export anyway": nothing starts.
    const answer = deferred();
    const sb = sandbox({
      confirmDialog: () => answer.promise,
      invoke: async (cmd) => { sb.calls.push(cmd); return cmd === "check_export_disk_space" ? { drive: "C:", free_mb: 1, needed_mb: 9 } : null; },
    });
    const run = sb.startExport(false);
    await tick();
    sb.requestExportCancel();
    answer.resolve(true);
    await run;
    check("Stop during the disk-space question: no export started", exportCalls(sb), 0);
    check("Stop during the disk-space question: reads as cancelled", sb.els["ve-export-status"].textContent, "veExportCancelled");
    check("Stop during the disk-space question: no failure toast", sb.toasts, []);
    check("Stop during the disk-space question: run is over", sb.VE.exporting, false);
  }
  {
    // Stop while the export-folder picker is open after a refused destination.
    const picker = deferred();
    const sb = sandbox({
      invoke: async (cmd) => {
        sb.calls.push(cmd);
        if (cmd === "export_video") throw "export destination not recognized";
        if (cmd === "pick_export_folder") return picker.promise;
        return null;
      },
    });
    const run = sb.startExport(false);
    await tick();
    sb.requestExportCancel();
    picker.resolve("D:/chosen");
    await run;
    check("Stop during the folder picker: the export is not retried", exportCalls(sb), 1);
    check("Stop during the folder picker: reads as cancelled", sb.els["ve-export-status"].textContent, "veExportCancelled");
    check("Stop during the folder picker: run is over", sb.VE.exporting, false);
  }
  {
    // Stop while the picker is open and then dismissed: cancelled, not "destination refused".
    const picker = deferred();
    const sb = sandbox({
      invoke: async (cmd) => {
        sb.calls.push(cmd);
        if (cmd === "export_video") throw "export destination not recognized";
        if (cmd === "pick_export_folder") return picker.promise;
        return null;
      },
    });
    const run = sb.startExport(false);
    await tick();
    sb.requestExportCancel();
    picker.resolve(null);
    await run;
    check("Stop, then picker dismissed: cancelled wording", sb.els["ve-export-status"].textContent, "veExportCancelled");
  }
  {
    // Stop while the clipboard temp name is being made.
    const name = deferred();
    const sb = sandbox({ invoke: async (cmd) => { sb.calls.push(cmd); return cmd === "clip_export_path" ? name.promise : null; } });
    const run = sb.startExport(true);
    await tick();
    sb.requestExportCancel();
    name.resolve("C:/tmp/clip.mp4");
    await run;
    check("Stop before the clip export starts: no export", exportCalls(sb), 0);
    check("Stop before the clip export starts: run is over", sb.VE.exporting, false);
  }
  {
    // Stop with nothing running does nothing; a later export starts normally (flag is per run).
    const sb = sandbox();
    sb.requestExportCancel();
    check("Stop with no export running sends nothing", sb.calls, []);
    await sb.startExport(false);
    check("a stale Stop does not cancel the next export", exportCalls(sb), 1);
  }
  {
    // Stop during the real export still reaches the backend cancel.
    const running = deferred();
    const sb = sandbox({ invoke: async (cmd) => { sb.calls.push(cmd); return cmd === "export_video" ? running.promise : null; } });
    const run = sb.startExport(false);
    await tick();
    sb.requestExportCancel();
    check("Stop during the export reaches the backend", sb.calls.includes("cancel_export"), true);
    running.resolve("C:/out/a.mp4");
    await run;
  }
  for (const p of problems) console.log("FAIL", p);
  console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
  process.exit(problems.length ? 1 : 0);
})();
