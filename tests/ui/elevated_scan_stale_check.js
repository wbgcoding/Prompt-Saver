// Elevated language scan (ui/dictation.js, pollElevatedCaps / closeLangsModal): the backend
// deletes the result file the moment elevated_capabilities_result returns the list. If the
// dialog was closed while that call was in flight, the list must still be kept, or the next
// poll gets "no result" until the budget ends and the dialog claims "needs elevation" after
// a scan that worked. The real source of the two functions runs in a vm with fake timers.
// Run:  node tests/ui/elevated_scan_stale_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const vm = require("vm");

const src = require(path.join(__dirname, "..", "ui_sources.js")).mainWindowSource().replace(/\r\n/g, "\n");
function extractBlock(marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find (source text changed?): ${marker}`);
  const braceStart = src.indexOf("{", src.indexOf(")", start));
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces after: ${marker}`);
}
const code = [extractBlock("function pollElevatedCaps("), extractBlock("function closeLangsModal(")].join("\n");

async function main() {
  let nextId = 1;
  const timers = new Map();
  let release = null;
  const rendered = [];
  const retries = [];
  const sandbox = {
    LANG_DIALOG: { ocr: { cache: [], checked: 0, installTimers: {}, installChain: {}, elevTimer: null, elevRunning: true, elevLeft: 60 } },
    LANG_CMDS: { ocr: { elevatedResult: "ocr_elevated_result" } },
    langDialogKind: "ocr",
    $: () => ({ classList: { add() {}, remove() {} } }),
    t: (k) => k, errText: (e) => String(e),
    renderLangsBody: (k) => rendered.push(k),
    renderLangsRetry: (k, m) => retries.push(m),
    invoke: () => new Promise((res) => { release = res; }),
    setTimeout: (fn) => { const id = nextId++; timers.set(id, fn); return id; },
    clearTimeout: (id) => { timers.delete(id); },
    Date, Math,
  };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox);
  const st = sandbox.LANG_DIALOG.ocr;
  const flush = () => new Promise((r) => setImmediate(r));
  const problems = [];

  vm.runInContext('pollElevatedCaps("ocr")', sandbox);
  for (const [id, fn] of [...timers]) { timers.delete(id); fn(); await flush(); } // call in flight
  vm.runInContext("closeLangsModal()", sandbox);                                  // dialog closed meanwhile
  release([{ tag: "en-US" }]);                                                    // list arrives, file is gone
  await flush();
  if (st.cache.length !== 1) problems.push("the scan result was dropped after the dialog closed");
  if (st.elevRunning) problems.push("elevRunning stayed set after the result was consumed");
  if (timers.size) problems.push("a poll is still scheduled after the result was consumed");
  if (retries.length) problems.push(`a retry notice appeared: ${retries[0]}`);

  if (problems.length) {
    console.error("elevated_scan_stale_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("elevated_scan_stale_check: ok");
}
main().catch((e) => { console.error(e); process.exit(1); });
