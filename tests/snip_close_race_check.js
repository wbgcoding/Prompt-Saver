// Screenshot editor close question vs. a new capture (ui/snipEditor.js): a capture that
// arrives while the user's own Save / Don't save / Cancel question is open joins that run, but
// the run must then behave like a replacing close, so the old edited image never lands on the
// clipboard after the new shot. Runs the real extracted source in a node vm.
// Run:  node tests/snip_close_race_check.js
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

function sandbox() {
  const sb = {
    log: [],
    answer: null,
    SE: { actions: { saveAs: async () => "C:\\x.png", saveToFolder: async () => "C:\\x.png" }, autoSaves: () => false, rev: 1, savedRev: 0 },
    t: (k) => k,
    $: () => ({ value: "png" }),
    snipEditFinishTextIfOpen() {},
    snipEditDirty: () => true,
    snipEditExportDataUrl: () => "data:image/png;base64,AA==",
    snipEditSyncDirty() {},
    snipEditApply: async () => { sb.log.push("apply"); },
    snipEditCancel: () => { sb.log.push("cancel"); },
    answers: [],
    confirmDialog: () => new Promise((res) => { sb.answer = res; sb.answers.push(res); }),
  };
  vm.createContext(sb);
  const decl = "  let snipCloseRun = null;";
  const declStart = src.indexOf(decl);
  if (declStart === -1) throw new Error("could not find the close-run state (source text changed?)");
  const end = src.indexOf('$("snip-edit-close").addEventListener', declStart);
  if (end === -1) throw new Error("could not find the end of the close logic (source text changed?)");
  vm.runInContext(src.slice(declStart, end), sb);
  return sb;
}

const tick = () => new Promise((r) => setImmediate(r));

(async () => {
  // A user close, then a capture during the question, answered Save: the old image is not applied.
  {
    const sb = sandbox();
    const user = sb.snipEditRequestClose();
    await tick();
    const capture = sb.snipEditRequestClose({ replacing: true });
    await tick();
    sb.answer(true);
    check("save: user close resolves true", await user, true);
    check("save: capture close resolves true", await capture, true);
    check("save: closes as a replacement, no clipboard apply", sb.log, ["cancel"]);
    // The flag must not leak into the next close.
    const again = sb.snipEditRequestClose();
    await tick();
    sb.answer("alt");
    await again;
    check("a later user close applies again", sb.log, ["cancel", "apply"]);
  }
  // Same with Don't save.
  {
    const sb = sandbox();
    const user = sb.snipEditRequestClose();
    await tick();
    const capture = sb.snipEditRequestClose({ replacing: true });
    await tick();
    sb.answer("alt");
    await user; await capture;
    check("dont save: closes as a replacement", sb.log, ["cancel"]);
  }
  // Cancel stays a cancel for both.
  {
    const sb = sandbox();
    const user = sb.snipEditRequestClose();
    await tick();
    const capture = sb.snipEditRequestClose({ replacing: true });
    await tick();
    sb.answer(false);
    check("cancel: user close false", await user, false);
    check("cancel: capture close false", await capture, false);
    check("cancel: nothing closed", sb.log, []);
  }
  // Untouched paths: a lone user close applies, a lone replacing close cancels.
  {
    const sb = sandbox();
    const user = sb.snipEditRequestClose();
    await tick();
    sb.answer("alt");
    await user;
    check("lone user close applies", sb.log, ["apply"]);
  }
  {
    const sb = sandbox();
    const cap = sb.snipEditRequestClose({ replacing: true });
    await tick();
    sb.answer("alt");
    await cap;
    check("lone replacing close cancels", sb.log, ["cancel"]);
  }
  // A stale run from an earlier session is dropped when a session opens, so the next close asks
  // again instead of joining a run whose question nobody can answer any more.
  {
    const sb = sandbox();
    const stale = sb.snipEditRequestClose();
    await tick();
    check("drop: the stale run is waiting", sb.answers.length, 1);
    check("drop: the function exists", typeof sb.snipEditDropCloseRun, "function");
    if (typeof sb.snipEditDropCloseRun === "function") {
      sb.snipEditDropCloseRun();
      const fresh = sb.snipEditRequestClose();
      await tick();
      check("drop: a new close starts its own run", fresh !== stale, true);
      check("drop: and asks again", sb.answers.length, 2);
      sb.answer("alt");
      await fresh;
      check("drop: the new run completes on its own", sb.log, ["apply"]);
      sb.answers[0](false);
      await stale;
    }
  }
  // The old run finishing later must not clear a newer run or its replacing flag.
  {
    const sb = sandbox();
    const old = sb.snipEditRequestClose();
    await tick();
    if (typeof sb.snipEditDropCloseRun === "function") {
      sb.snipEditDropCloseRun();
      const newer = sb.snipEditRequestClose({ replacing: true });
      await tick();
      sb.answers[0](false); // the old run ends now
      await old;
      await tick();
      const joined = sb.snipEditRequestClose();
      check("old finally: a newer run is still the one joined", joined === newer, true);
      sb.answers[1]("alt");
      await newer;
      check("old finally: the newer run keeps its replacing flag", sb.log, ["cancel"]);
    } else {
      check("old finally: drop function exists", false, true);
    }
  }
  if (problems.length) {
    console.error("FAIL\n" + problems.join("\n"));
    process.exit(1);
  }
  console.log("snip close race check ok");
})();
