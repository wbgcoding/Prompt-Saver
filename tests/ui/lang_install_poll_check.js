// Language install polling (ui/dictation.js, pollLangInstall / closeLangsModal): an
// install_result answer that arrives after the dialog was closed must still be
// handled. The backend deletes the result file and drops its nonce the moment it
// answers, so a dropped answer left the row busy and the next poller waiting for a
// result that can never come again (until the ~10 minute poll limit).
//
// The window script is split into modules with boot code, so the real source text of the two
// functions is extracted and run in a vm against fake timers and a scripted backend.
// Run:  node tests/ui/lang_install_poll_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = require(path.join(__dirname, "..", "ui_sources.js")).mainWindowSource().replace(/\r\n/g, "\n");
function extractBlock(marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find in main.js (source text changed?): ${marker}`);
  const braceStart = src.indexOf("{", start);
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces after: ${marker}`);
}
const code = [
  "const MAX_LANG_INSTALL_POLLS = 200;",
  extractBlock("function pollLangInstall("),
  extractBlock("function closeLangsModal("),
  extractBlock("async function refreshLangCaches("),
  extractBlock("function openLangsModal("),
].join("\n");

async function scenario(reopenBeforeAnswer) {
  let nextId = 1;
  const timers = new Map();
  const calls = [];
  let deferred = null;
  let firstCall = true;
  const make = () => ({ cache: [], installTimers: {}, installErrors: {}, installing: new Set(), installChain: {}, elevTimer: null });
  const sandbox = {
    LANG_DIALOG: { ocr: make() },
    LANG_CMDS: { ocr: { installResult: "ocr_install_result", languages: "ocr_languages" } },
    langDialogKind: "ocr",
    ocrLangsCache: [], voiceLangsCache: [],
    assignOcrLangsCache: (v) => { calls.push("setter:ocr"); },
    assignVoiceLangsCache: (v) => { calls.push("setter:speech"); },
    $: () => ({ classList: { add() {}, remove() {} } }),
    t: (k) => k,
    renderLangsBody: () => {},
    loadLangCaps: async (kind, force) => { calls.push(`caps:${kind}:${force}`); },
    invoke: (cmd) => {
      if (cmd === "ocr_install_result") {
        if (firstCall) { firstCall = false; return new Promise((res) => { deferred = res; }); }
        return Promise.resolve(null);
      }
      return Promise.resolve([]);
    },
    setTimeout: (fn) => { const id = nextId++; timers.set(id, fn); return id; },
    clearTimeout: (id) => { timers.delete(id); },
  };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox);
  const st = sandbox.LANG_DIALOG.ocr;
  const flush = () => new Promise((r) => setImmediate(r));
  const fireTimers = async () => { for (const [id, fn] of [...timers]) { timers.delete(id); fn(); await flush(); } };

  vm.runInContext('pollLangInstall("ocr", "en-US")', sandbox);
  await fireTimers();               // the first tick: install_result is now in flight
  vm.runInContext("closeLangsModal()", sandbox);
  if (reopenBeforeAnswer) vm.runInContext('pollLangInstall("ocr", "en-US")', sandbox);
  deferred({ ok: true });           // the terminal answer lands on the stale chain
  await flush(); await flush();

  const problems = [];
  if (st.installing.has("en-US")) problems.push("row still busy after the terminal answer");
  if (st.installErrors["en-US"]) problems.push(`unexpected error: ${st.installErrors["en-US"]}`);
  if (!calls.includes("caps:ocr:true")) problems.push("capability list was not refreshed");
  if (!calls.includes("setter:ocr")) problems.push("the OCR language cache was not replaced through its setter");
  if (timers.size) problems.push("a poller is still armed and would wait for a result that is gone");
  return problems;
}

// A poll that gives up at the limit while the install is still running: the late result must not
// be lost. The language lists are refreshed at once, and reopening the dialog polls the name again.
async function timeoutScenario() {
  let nextId = 1;
  const timers = new Map();
  const calls = [];
  let answer = null;
  const el = { classList: { add() {}, remove() {} }, textContent: "" };
  const sandbox = {
    LANG_DIALOG: { ocr: { cache: [], checked: 0, installTimers: {}, installErrors: {}, installing: new Set(), lateInstall: new Set(), installChain: {}, elevTimer: null, elevRunning: false } },
    LANG_CMDS: { ocr: { installResult: "ocr_install_result", languages: "ocr_languages" } },
    langDialogKind: "ocr",
    assignOcrLangsCache: () => { calls.push("setter:ocr"); },
    assignVoiceLangsCache: () => {},
    $: () => el,
    t: (k) => k,
    renderLangsBody: () => {},
    renderLangsLoading: () => {},
    pollElevatedCaps: () => {},
    loadLangCaps: async (kind, force) => { calls.push(`caps:${kind}:${force}`); },
    invoke: (cmd) => (cmd === "ocr_install_result" ? Promise.resolve(answer) : Promise.resolve([])),
    setTimeout: (fn) => { const id = nextId++; timers.set(id, fn); return id; },
    clearTimeout: (id) => { timers.delete(id); },
  };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox);
  const st = sandbox.LANG_DIALOG.ocr;
  const flush = () => new Promise((r) => setImmediate(r));
  const fireTimers = async () => { for (const [id, fn] of [...timers]) { timers.delete(id); fn(); await flush(); } };

  vm.runInContext('pollLangInstall("ocr", "de-DE")', sandbox);
  for (let i = 0; i < 205 && timers.size; i++) await fireTimers();
  const problems = [];
  if (st.installing.has("de-DE")) problems.push("row still busy after the poll limit");
  if (!calls.includes("caps:ocr:true")) problems.push("timeout did not refresh the capability list");
  if (!calls.includes("setter:ocr")) problems.push("timeout did not refresh the language cache");
  calls.length = 0;

  answer = { ok: true };            // the install finishes after the poller gave up
  vm.runInContext('openLangsModal("ocr")', sandbox);
  await fireTimers(); await flush(); await flush();
  if (st.installErrors["de-DE"]) problems.push("the timeout message stayed after the reopen");
  if (st.installing.has("de-DE")) problems.push("a late result was not picked up on reopen");
  if (!calls.includes("caps:ocr:true")) problems.push("a late result did not refresh the lists");
  if (timers.size) problems.push("a poller is still armed after the late result");
  return problems;
}

(async () => {
  const all = [];
  for (const p of await timeoutScenario()) all.push(`timeout: ${p}`);
  for (const reopen of [false, true]) {
    for (const p of await scenario(reopen)) all.push(`${reopen ? "reopened" : "closed"}: ${p}`);
  }
  if (all.length) {
    console.error("lang_install_poll_check: FAILED\n  " + all.join("\n  "));
    process.exit(1);
  }
  console.log("lang_install_poll_check: ok");
})().catch((e) => { console.error(e); process.exit(1); });
