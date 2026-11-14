// A second recording result arriving while the result dialog still shows an undecided take
// (ui/capture.js): the earlier take is saved like a plain Save after its running work ended,
// never orphaned; a decided or closed dialog is left alone.
// Runs the real extracted source in a node vm. Run:  node tests/rec_handoff_check.js
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

function sandbox(over) {
  const sb = Object.assign({
    calls: [], toasts: [], errs: [],
    recModal: { classList: { contains: () => sb.hidden } }, hidden: false,
    pendingRecPath: "C:/rec/first.mp4", recSavedOrAttached: false, recGen: 3,
    recWork: Promise.resolve(), recSaveBusy: false, recSaveAsBusy: false, recCopyBusy: false,
    invoke: async (cmd, args) => { sb.calls.push([cmd, args.srcPath]); return "C:/saved/first.mp4"; },
    toast: (m) => sb.toasts.push(m), errToast: (e) => sb.errs.push(String(e)), t: (k) => k,
  }, over || {});
  vm.createContext(sb);
  vm.runInContext("var recHandoff = Promise.resolve();" + extractBlock("async function keepUndecidedRecording() {"), sb);
  return sb;
}

(async () => {
  {
    // The first take's conversion is still running: it is saved only after that finished.
    let release;
    const sb = sandbox({ recWork: new Promise((r) => { release = r; }) });
    const done = sb.keepUndecidedRecording();
    await Promise.resolve();
    check("undecided take waits for its running work", sb.calls, []);
    release();
    await done;
    check("undecided take is saved as a plain save", sb.calls, [["save_recording_now", "C:/rec/first.mp4"]]);
    check("saved note shown", sb.toasts, ["recPrevSaved"]);
    check("marked decided", sb.recSavedOrAttached, true);
  }
  {
    const sb = sandbox({ recSavedOrAttached: true });
    await sb.keepUndecidedRecording();
    check("already saved or attached: untouched", sb.calls, []);
  }
  {
    const sb = sandbox({ hidden: true });
    await sb.keepUndecidedRecording();
    check("closed dialog: untouched", sb.calls, []);
  }
  {
    // The user closed the dialog while the work ran: the close owns the take.
    let release;
    const sb = sandbox({ recWork: new Promise((r) => { release = r; }) });
    const done = sb.keepUndecidedRecording();
    sb.recGen = 4;
    release();
    await done;
    check("closed while waiting: untouched", sb.calls, []);
  }
  {
    const sb = sandbox({ invoke: async () => { throw "disk full"; } });
    await sb.keepUndecidedRecording();
    check("failed save: error shown, no saved note", [sb.errs, sb.toasts], [["disk full"], []]);
  }
  for (const p of problems) console.log("FAIL", p);
  console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
  process.exit(problems.length ? 1 : 0);
})();
