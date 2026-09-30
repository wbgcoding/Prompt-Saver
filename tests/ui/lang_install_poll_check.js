// Language install polling (ui/main.js, pollLangInstall / closeLangsModal): an
// install_result answer that arrives after the dialog was closed must still be
// handled. The backend deletes the result file and drops its nonce the moment it
// answers, so a dropped answer left the row busy and the next poller waiting for a
// result that can never come again (until the ~10 minute poll limit).
//
// main.js is one big module with boot code, so the real source text of the two
// functions is extracted and run in a vm against fake timers and a scripted backend.
// Run:  node tests/ui/lang_install_poll_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "..", "ui", "main.js"), "utf8").replace(/\r\n/g, "\n");
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
  if (timers.size) problems.push("a poller is still armed and would wait for a result that is gone");
  return problems;
}

(async () => {
  const all = [];
  for (const reopen of [false, true]) {
    for (const p of await scenario(reopen)) all.push(`${reopen ? "reopened" : "closed"}: ${p}`);
  }
  if (all.length) {
    console.error("lang_install_poll_check: FAILED\n  " + all.join("\n  "));
    process.exit(1);
  }
  console.log("lang_install_poll_check: ok");
})().catch((e) => { console.error(e); process.exit(1); });
