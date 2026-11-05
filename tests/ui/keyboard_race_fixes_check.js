// Fixes in the chain, compare, copy-flow, editor, grid and tag scripts, each section runs the
// real extracted source of the function in a node vm. UI_CHECK_DIR points the test at another
// copy of ui/ (to prove it fails on older code).
// Run:  node tests/ui/keyboard_race_fixes_check.js      Exit: 0 = clean, 1 = problems.
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
const run = (code, sandbox) => { vm.createContext(sandbox); vm.runInContext(code, sandbox); return sandbox; };
const tick = () => new Promise((r) => setImmediate(r));
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };

// chain.js toggleChainPick: a locked confidential prompt goes through the vault gate first.
later(async () => {
  const src = read("chain.js");
  const code = extractBlock(src, "export async function toggleChainPick").replace(/export /g, "");
  const peeks = [];
  let gate = null;
  const sel = [];
  const sandbox = run(code + "\n;globalThis.pick = toggleChainPick;", {
    isTextPrompt: () => true,
    chainPicking: new Set(),
    chainSel: sel, chainTexts: new Map(), chainRaw: new Map(),
    chainMode: true,
    refreshChainUi: () => {}, pushChainClipboard: () => {},
    readyForCopy: async (p) => gate(p),
    invoke: async (cmd) => { peeks.push(cmd); return null; },
    varTokens: () => [],
    resolveVars: async (x) => x,
  });
  gate = async () => null;
  await sandbox.pick({ id: "a", name: "Secret", text: "", locked: true });
  check(sel.length === 0, `a refused unlock chains nothing, got ${sel}`);
  gate = async (p) => ({ ...p, text: "real text", locked: false });
  await sandbox.pick({ id: "b", name: "Secret", text: "", locked: true });
  check(sel.join() === "b", "an unlocked prompt is picked");
});

// management.js openCompareView: a locked vault asks for the key and fetches again once.
later(async () => {
  const code = extractBlock(read("management.js"), "export async function openCompareView").replace(/export /g, "");
  const run1 = async (fail, unlock) => {
    const log = { toasts: [], errs: [], unlocks: 0, calls: 0, rendered: 0 };
    const sandbox = run(code + "\n;globalThis.open = openCompareView;", {
      compareState: null,
      invoke: async (cmd, { id }) => { log.calls++; const r = fail(id, log.calls); if (r instanceof Error) throw r.message; return r; },
      ensureSecretVaultUnlocked: async () => { log.unlocks++; return unlock; },
      toast: (m) => log.toasts.push(m), t: (k) => k, errToast: (e) => log.errs.push(String(e)),
      renderCompareView: () => { log.rendered++; },
      $: () => ({ classList: { remove() {} } }),
    });
    await sandbox.open(["a", "b"]);
    return log;
  };
  let l = await run1((id, n) => (n === 1 ? new Error("secret-locked") : { id }), true);
  check(l.unlocks === 1 && l.rendered === 1 && !l.toasts.length, `locked vault: unlock once, then compare, got ${JSON.stringify(l)}`);
  l = await run1(() => new Error("secret-locked"), false);
  check(l.unlocks === 1 && !l.rendered && !l.toasts.length && !l.errs.length, `declined unlock opens nothing silently, got ${JSON.stringify(l)}`);
  l = await run1(() => new Error("boom"), true);
  check(l.errs.length === 1 && !l.toasts.length && !l.unlocks, `other errors go through errToast, got ${JSON.stringify(l)}`);
  l = await run1((id) => (id === "a" ? { id } : null), true);
  check(l.toasts.join() === "compareMissing", "a missing entry still says so");
});

// copyFlow.js copyTextWithVars: a press on another prompt while a fill-in window is open starts
// its own question; only the same prompt's second press upgrades the pending copy.
later(async () => {
  const src = read("copyFlow.js");
  const from = src.indexOf("let varsPendingId") >= 0 ? src.indexOf("// The prompt whose fill-in window") : src.indexOf("export async function copyTextWithVars");
  const code = src.slice(from, src.indexOf("// Copy formatted (context menu")).replace(/export /g, "");
  const started = [];
  const gate = deferred();
  const ctx = {
    varsWindowPending: null, varsPasteRequested: false,
    peekResolveCommit: async (p) => { started.push(p.id); ctx.varsWindowPending = () => {}; await gate.promise; ctx.varsWindowPending = null; return false; },
    invoke: async () => true, errToast() {}, flag: () => true,
    showCopied() {}, recordCopy() {}, autoPasteNow: async () => {}, maybeCloseOnCopy() {},
  };
  const sandbox = run(code + "\n;globalThis.go = copyTextWithVars; globalThis.req = () => varsPasteRequested;", ctx);
  const a = sandbox.go({ id: "A" }, null, false);
  await tick();
  await sandbox.go({ id: "A" }, null, false);
  check(started.join() === "A" && sandbox.req() === true, `same prompt: double click upgrades, got ${started}`);
  const b = sandbox.go({ id: "B" }, null, false);
  await tick();
  check(started.join() === "A,B", `another prompt starts its own question, got ${started}`);
  gate.resolve(); await a; await b;
});

// editor.js editPrompt: an editor opened while the fetch is pending is not overwritten.
later(async () => {
  const code = extractBlock(read("editor.js"), "export async function editPrompt").replace(/export /g, "");
  const gate = deferred();
  const opened = [];
  const sandbox = run("let modalSession = 0;\n" + code + "\n;globalThis.edit = editPrompt; globalThis.bump = () => { modalSession++; };", {
    modalState: null,
    confirmDiscardIfDirty: async () => true, closeModal() {},
    invoke: async () => { await gate.promise; return { id: "x" }; },
    ensureSecretVaultUnlocked: async () => true, errToast() {},
    promptToModal: (p) => p,
    openModal: (m) => opened.push(m.id),
  });
  const first = sandbox.edit("x");
  await tick();
  sandbox.bump();
  gate.resolve(); await first;
  check(opened.length === 0, `an editor opened meanwhile is kept, got ${opened}`);
  gate.resolve();
  await sandbox.edit("x");
  check(opened.length === 1, "an undisturbed fetch still opens the editor");
});

// editor.js onModalSecretToggle: closing and reopening the same prompt while the vault answers
// leaves the new session alone.
later(async () => {
  const code = extractBlock(read("editor.js"), "export async function onModalSecretToggle").replace(/export /g, "");
  const gate = deferred();
  const log = { closed: 0, ui: 0 };
  const sandbox = run("let modalSession = 0; let modalInitial = '';\n" + code + "\n;globalThis.toggle = onModalSecretToggle; globalThis.reopen = () => { modalSession += 2; };", {
    modalState: { mode: "edit", id: "p1", secret: false },
    modalSnapshot: () => "", modalInitial: "",
    toast() {}, t: (k) => k, errToast() {},
    ensureSecretVaultUnlocked: async () => true,
    invoke: async () => { await gate.promise; },
    updateSecretToggleUi: () => { log.ui++; },
    renderGrid: async () => {},
    closeModal: () => { log.closed++; },
    prompts: [], modal: { text: {} },
  });
  const p = sandbox.toggle();
  await tick();
  sandbox.reopen();
  gate.resolve(); await p;
  check(log.closed === 0 && log.ui === 0, `the reopened editor stays, got ${JSON.stringify(log)}`);
});

// editor.js loadHistory: two overlapping loads for one prompt draw their rows once.
later(async () => {
  const src = read("editor.js");
  const from = src.indexOf("let historySeq") >= 0 ? src.indexOf("let historySeq") : src.indexOf("export async function loadHistory");
  const code = src.slice(from, src.indexOf("  list.forEach((v, idx)", from)).replace(/export /g, "") + "}";
  const gates = [deferred(), deferred()];
  let n = 0;
  const kids = [];
  const body = { children: kids, set innerHTML(v) { kids.length = 0; }, appendChild(e) { kids.push(e); } };
  const sandbox = run(code + "\n;globalThis.load = loadHistory;", {
    $: () => body, modalState: { id: "p" }, t: (k) => k,
    invoke: () => gates[n++].promise,
    document: { createElement: () => ({}) },
  });
  const a = sandbox.load("p"); const b = sandbox.load("p");
  gates[0].resolve([]); gates[1].resolve([]);
  await a; await b;
  check(kids.length === 1, `overlapping loads draw once, got ${kids.length}`);
});

Promise.all(pending).then(() => {
  if (problems.length) { console.error("keyboard_race_fixes_check FAILED:\n" + problems.map((p) => "  - " + p).join("\n")); process.exit(1); }
  console.log("keyboard_race_fixes_check: ok");
});
