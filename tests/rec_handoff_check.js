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
    recWork: Promise.resolve(), recActions: new Set(),
    recTakeKey: "C:/rec/first-last.mp4", partCalls: [],
    invoke: async (cmd, args) => {
      if (cmd === "save_recording_parts") { sb.partCalls.push(args); return [2, 0]; }
      sb.calls.push([cmd, args.srcPath]);
      return "C:/saved/first.mp4";
    },
    toast: (m) => sb.toasts.push(m), errToast: (e) => sb.errs.push(String(e)), t: (k) => k,
  }, over || {});
  vm.createContext(sb);
  vm.runInContext("var recHandoff = Promise.resolve();" + extractBlock("async function keepRecParts(besideSaveAs, take = recTakeKey) {") + extractBlock("async function keepUndecidedRecording() {"), sb);
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
    check("saved note shown", sb.toasts, ["recContinuePartsSaved", "recPrevSaved"]);
    check("marked decided", sb.recSavedOrAttached, true);
    // The displaced take's extra parts go with it, named by its own take key.
    check("displaced take's parts are saved with it", sb.partCalls, [{ besideSaveAs: false, take: "C:/rec/first-last.mp4" }]);
  }
  {
    const sb = sandbox({ recSavedOrAttached: true });
    await sb.keepUndecidedRecording();
    check("already saved or attached: untouched", [sb.calls, sb.partCalls], [[], []]);
  }
  {
    const sb = sandbox({ hidden: true });
    await sb.keepUndecidedRecording();
    check("closed dialog: untouched", [sb.calls, sb.partCalls], [[], []]);
  }
  {
    // The user closed the dialog while the work ran: the close owns the take.
    let release;
    const sb = sandbox({ recWork: new Promise((r) => { release = r; }) });
    const done = sb.keepUndecidedRecording();
    sb.recGen = 4;
    release();
    await done;
    check("closed while waiting: untouched", [sb.calls, sb.partCalls], [[], []]);
  }
  {
    const sb = sandbox({ invoke: async () => { throw "disk full"; } });
    await sb.keepUndecidedRecording();
    check("failed save: error shown, no saved note", [sb.errs, sb.toasts], [["disk full"], []]);
    check("failed save: no parts moved", sb.partCalls, []);
  }
  {
    // The new dialog's take key is set only after the displaced take was saved with its own key,
    // and a parts save reads the key when it starts (a later swap cannot redirect it).
    const m = src.match(/const mine = recHandoff[\s\S]*?await keepUndecidedRecording\(\);([\s\S]*?)recWork = openRecModal/);
    check("result handler sets the new take key after the handoff", !!m && /recTakeKey = path;/.test(m[1]), true);
    const keep = extractBlock("async function keepRecParts(besideSaveAs, take = recTakeKey) {");
    check("parts save takes the key as an argument that defaults to the current one", /await invoke\("save_recording_parts", \{ besideSaveAs, take \}\)/.test(keep), true);
    const discard = src.match(/invoke\("discard_recording_parts"[^)]*\)/);
    check("discard names the take", !!discard && discard[0].includes("take"), true);
  }
  {
    // A new take arrives while the "discard after copy" question is open: confirming it must not
    // close the dialog or discard anything of the new take (the old one was already kept).
    let answer;
    const el = () => ({ classList: { add() {}, remove() {} }, pause() {}, removeAttribute() {}, load() {} });
    const sb = {
      calls: [], toasts: [],
      pendingRecPath: "C:/rec/first.mp4", recTakeKey: "C:/rec/first-last.mp4",
      recSavedOrAttached: false, recCopiedNotSaved: true, recGen: 3, recLateOutput: "",
      recWork: Promise.resolve(), recProgressOff: null,
      recModal: el(), recPreview: el(), recStatus: { classList: { remove() {} }, onclick: null },
      confirmDialog: () => new Promise((r) => { answer = r; }),
      invoke: async (cmd, args) => { sb.calls.push([cmd, args && (args.path || args.take)]); return null; },
      closeRecMorePop() {}, setPendingRec(p) { sb.pendingRecPath = p; },
      toast: (m) => sb.toasts.push(m), t: (k) => k,
    };
    vm.createContext(sb);
    vm.runInContext(extractBlock("async function closeRecModal() {"), sb);
    const done = sb.closeRecModal();
    await Promise.resolve();
    sb.pendingRecPath = "C:/rec/second.mp4";
    sb.recTakeKey = "C:/rec/second-last.mp4";
    answer(true);
    await done;
    check("new take during the close question: nothing discarded", sb.calls, []);
    check("new take during the close question: dialog stays on the new take", sb.pendingRecPath, "C:/rec/second.mp4");
    // Same question with no new take: the close goes ahead and discards that take.
    sb.calls = []; sb.recCopiedNotSaved = true;
    const again = sb.closeRecModal();
    await Promise.resolve();
    answer(true);
    await again;
    check("confirmed close without a new take discards it", sb.calls.filter(([c]) => c.startsWith("discard")).map((c) => c[1]), ["C:/rec/second.mp4", "C:/rec/second-last.mp4"]);
  }
  {
    // Save / Save as / Edit still running when the next take opens its dialog: the new take is
    // never adopted into, marked by or closed by the old action; only the old take's own parts
    // are finished.
    const handlers = (over) => {
      const sb = Object.assign({
        pendingRecPath: "C:/rec/a.mp4", recSavedOrAttached: false, recGen: 3, recTakeKey: "A-last",
        recSaveBusy: false, recSaveAsBusy: false, recCopyBusy: false, recCopiedNotSaved: false,
        partCalls: [], adopted: [], toasts: [], errs: [], closed: 0, gate: null,
        adoptRecPath: (p) => sb.adopted.push(p), recordToolUsage() {}, closeRecModal: () => { sb.closed++; },
        openVideoEditor: () => sb.gate,
        invoke: async (cmd, args) => {
          if (cmd === "save_recording_parts") { sb.partCalls.push(args); return [1, 0]; }
          if (cmd === "copy_recording_to_clipboard") { await sb.gate; return ["C:/rec/a-copy.mp4", true]; }
          await sb.gate;
          return "C:/saved/a.mp4";
        },
        toast: (m) => sb.toasts.push(m), errToast: (e) => sb.errs.push(String(e)), t: (k) => k,
      }, over || {});
      vm.createContext(sb);
      vm.runInContext(extractBlock("async function keepRecParts(besideSaveAs, take = recTakeKey) {") + extractBlock("async function saveRecNow() {") + extractBlock("async function saveRecAs() {") + extractBlock("async function editRecTake() {") + extractBlock("async function copyRecToClipboard() {"), sb);
      return sb;
    };
    const swap = (sb) => { sb.recGen = 4; sb.recTakeKey = "B-last"; sb.pendingRecPath = "C:/rec/b.mp4"; };
    for (const [name, fn, besideSaveAs, edit] of [["Save", "saveRecNow", false], ["Save as", "saveRecAs", true], ["Edit", "editRecTake", false, true]]) {
      let release;
      const sb = handlers({ gate: new Promise((r) => { release = r; }) });
      if (edit) sb.gate = sb.gate.then(() => true);
      const done = sb[fn]();
      await Promise.resolve();
      swap(sb);
      release();
      await done;
      check(name + " superseded: new take's path not adopted", sb.adopted, []);
      check(name + " superseded: new take not marked decided", sb.recSavedOrAttached, false);
      check(name + " superseded: new take's dialog not closed", sb.closed, 0);
      check(name + " superseded: only the old take's parts are finished", sb.partCalls, [{ besideSaveAs, take: "A-last" }]);
    }
    {
      let release;
      const sb = handlers({ gate: new Promise((r) => { release = r; }) });
      const done = sb.copyRecToClipboard();
      await Promise.resolve();
      swap(sb);
      release();
      await done;
      check("Copy superseded: new take's path not replaced", [sb.adopted, sb.recCopiedNotSaved], [[], false]);
    }
    {
      const sb = handlers({ gate: Promise.resolve() });
      await sb.saveRecNow();
      check("Save without a new take: adopted, marked, parts of its own take", [sb.adopted, sb.recSavedOrAttached, sb.partCalls], [["C:/saved/a.mp4"], true, [{ besideSaveAs: false, take: "A-last" }]]);
    }
  }
  {
    // A new result arriving during Save as waits for the picker: a cancelled picker leaves the
    // take undecided and it is then saved like a plain Save; a finished Save leaves it alone.
    for (const outcome of ["cancelled", "saved"]) {
      let finish;
      const sb = sandbox();
      const running = new Promise((r) => { finish = r; });
      sb.recActions.add(running);
      running.then(() => sb.recActions.delete(running));
      const done = sb.keepUndecidedRecording();
      await Promise.resolve(); await Promise.resolve(); await Promise.resolve();
      check(outcome + ": waits for the running action", sb.calls, []);
      if (outcome === "saved") sb.recSavedOrAttached = true;
      finish();
      await done;
      check(outcome + " picker", sb.calls, outcome === "saved" ? [] : [["save_recording_now", "C:/rec/first.mp4"]]);
    }
  }
  for (const p of problems) console.log("FAIL", p);
  console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
  process.exit(problems.length ? 1 : 0);
})();
