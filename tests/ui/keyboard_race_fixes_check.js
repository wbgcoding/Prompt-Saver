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

// editor.js modalSnapshot: the positions a version restore rewrites still hold name, text, entry
// list and rotate mode.
later(async () => {
  const src = read("editor.js");
  const from = src.indexOf("const SNAP_RESTORED") >= 0 ? src.indexOf("const SNAP_RESTORED") : src.indexOf("export function modalSnapshot");
  const code = src.slice(from, src.indexOf("// ---- Editor variable helper")).replace(/export /g, "");
  const fld = (value) => ({ value, classList: { contains: () => false } });
  const sandbox = run(code + "\n;globalThis.snap = modalSnapshot; globalThis.pos = typeof SNAP_RESTORED === 'undefined' ? null : SNAP_RESTORED;", {
    modalState: { rotateEntries: ["ENTRIES"], rotateMode: "MODE", tags: [] },
    modal: { name: fld("NAME"), text: fld("TEXT"), caption: fld(""), captionSize: fld(""), fontSel: fld(""), sizeSel: fld(""), fav: fld(""), notesText: fld("") },
  });
  const arr = JSON.parse(sandbox.snap());
  const pos = sandbox.pos;
  check(!!pos && arr[pos.name] === "NAME" && arr[pos.text] === "TEXT" && String(arr[pos.entries]) === "ENTRIES" && arr[pos.rotateMode] === "MODE", `restore positions match the snapshot layout, got ${JSON.stringify(pos)}`);
});

// views.js renderGrid: of two overlapping fetching renders the older answer never overwrites the newer.
later(async () => {
  const src = read("views.js");
  const from = src.indexOf("let renderGridSeq") >= 0 ? src.indexOf("let renderGridSeq") : src.indexOf("export async function renderGrid");
  const code = src.slice(from, src.indexOf("  armVaultRelock();\n  if (favView", from)).replace(/export /g, "") + "}";
  const gates = [deferred(), deferred()];
  let n = 0;
  const sandbox = run(code + "\n;globalThis.go = renderGrid;", {
    hideTileTip() {}, invoke: () => gates[n++].promise, errToast() {},
    assignPrompts: (v) => { sandbox.state = v; }, assignSettings() {}, updateExamplesBanner: async () => {},
    settings: {}, prompts: [],
  });
  const a = sandbox.go(); const b = sandbox.go();
  gates[1].resolve({ prompts: "new", settings: {} });
  await tick();
  gates[0].resolve({ prompts: "old", settings: {} });
  await a; await b;
  check(sandbox.state === "new", `the newest snapshot stays, got ${sandbox.state}`);
});

// gridFolders.js: a folder member answers Enter/Space (a click with no pointer, detail 0) through
// the same copy path as the pointer release, and a mouse click is left to the pointer path.
later(async () => {
  const src = read("gridFolders.js");
  check(/btn\.addEventListener\("click", \(ev\) => \{\s*if \(ev\.detail !== 0\) return;\s*copyFolderMember\(/.test(src), "member button has a keyboard click handler guarded by detail 0");
  const code = src.indexOf("async function copyFolderMember") >= 0 ? extractBlock(src, "async function copyFolderMember") : "async function copyFolderMember() { throw new Error('missing copyFolderMember'); }";
  const log = [];
  const sandbox = run(code + "\n;globalThis.copy = copyFolderMember;", {
    chainMode: false, toggleChainPick: async () => log.push("chain"),
    dialogTokens: () => [], peekRotatedText: async (p) => p.text, copyOnCooldown: () => false,
    autoPasteDouble: () => false, autoPasteNow: async () => log.push("paste"),
    copyResolved: async () => { log.push("copy"); return true; },
    showCopied: () => log.push("shown"), recordCopy: () => log.push("rec"), maybeCloseOnCopy: () => log.push("close"),
    copyTextWithVars: async () => log.push("vars"),
  });
  await sandbox.copy({ id: "m", text: "hi" }, {});
  check(log.join() === "copy,shown,rec,close", `member copy path, got ${log}`);
  await sandbox.copy(undefined, {});
  check(log.length === 4, "a vanished prompt copies nothing");
});

// copyFlow.js arrow-key navigation: grid-folder cells are part of the walk, Enter opens one and
// hands the focus to its first member.
later(async () => {
  const src = read("copyFlow.js");
  const from = src.indexOf("// The cells the arrow keys walk");
  if (from < 0) { check(false, "arrow-key walk includes folder cells (kbNavMap missing)"); return; }
  const code = src.slice(from, src.indexOf("// A copy swallowed by the cooldown")).replace(/export /g, "");
  const cellEl = (col, row) => ({ dataset: { col: String(col), row: String(row) } });
  const folderTile = { dataset: { folderId: "f1" }, closest: () => cellEl(1, 0) };
  const focused = [];
  const opened = [];
  const member = { focus: () => focused.push("member") };
  const sandbox = run("const FOLDER_KEY = 'folder:'; const kbElOf = () => folderTile; let kbFocus = null;\n" + code + "\n;globalThis.walk = (dc, dr, start) => { kbFocus = start; moveKbFocus(dc, dr); return kbFocus; }; globalThis.act = activateKbFocus;", {
    folderTile,
    favView: false, favLayoutMap: () => ({}), layoutOf: () => ({ a: [0, 0], b: [2, 0] }), activeView: () => ({}),
    gridEl: { querySelectorAll: () => [folderTile] },
    document: { querySelector: () => member },
    focusTile: (id) => focused.push(id),
    folderById: (id) => ({ id }), openTileFolderPop: (f) => opened.push(f.id),
    activateTile: (id) => opened.push("tile:" + id),
  });
  sandbox.walk(1, 0, "a");
  sandbox.walk(1, 0, "folder:f1");
  check(focused.join() === "folder:f1,b", `the right arrow walks a, folder, b, got ${focused}`);
  focused.length = 0;
  sandbox.act("folder:f1");
  check(opened.join() === "f1" && focused.join() === "member", `Enter opens the folder and focuses its first member, got ${opened} ${focused}`);
  sandbox.act("a");
  check(opened.join() === "f1,tile:a", "Enter on a prompt still copies it");
});

// copyFlow.js: Menu key / Shift+F10 on the ring opens the cell's menu and focuses its first item.
later(async () => {
  const src = read("copyFlow.js");
  check(/e\.key === "ContextMenu" \|\| \(e\.key === "F10" && e\.shiftKey\)/.test(src) && src.includes("openKbMenu(kbFocus)"), "the grid key handler maps Menu and Shift+F10 to the tile menu");
  if (!src.includes("export function openKbMenu")) { check(false, "openKbMenu missing"); return; }
  const code = extractBlock(src, "export function openKbMenu").replace(/export /g, "");
  const log = [];
  const item = (n) => ({ focus: () => log.push("focus:" + n) });
  const mkMenu = (hidden, first) => ({ classList: { contains: () => hidden }, querySelector: () => first });
  const run1 = (ctxHidden) => {
    log.length = 0;
    const tile = { getBoundingClientRect: () => ({ left: 10, top: 20, width: 40, height: 30 }), dispatchEvent: (ev) => { log.push(`ctx@${ev.clientX},${ev.clientY}`); } };
    const sandbox = run(code + "\n;globalThis.open = openKbMenu;", {
      kbElOf: () => tile, ctxEl: mkMenu(ctxHidden, item("ctx")), miniMenu: mkMenu(!ctxHidden, item("mini")),
      MouseEvent: class { constructor(t, o) { this.type = t; Object.assign(this, o); } },
    });
    sandbox.open("a");
    return log.join();
  };
  check(run1(false) === "ctx@30,35,focus:ctx", `a prompt tile menu opens at the tile centre and takes focus, got ${run1(false)}`);
  check(run1(true) === "ctx@30,35,focus:mini", `a folder cell's mini menu takes focus, got ${run1(true)}`);
});

// tags.js: ArrowUp/Down walk the suggestion list, Enter picks the highlighted row.
later(async () => {
  const src = read("tags.js");
  const a = src.indexOf("let tagSuggestActive");
  const b = src.indexOf("// ---- Editor: assigned chips");
  if (a === -1 || b === -1) { check(false, "tag suggestion keyboard state missing"); return; }
  const code = src.slice(a, b).replace(/export /g, "") + "\n" + extractBlock(src, "export function wireTagInput").replace(/export /g, "") + "\n;globalThis.wire = wireTagInput;";
  const log = [];
  const mkItem = (id) => {
    const attrs = {}; const cls = new Set();
    return { id, attrs, cls, setAttribute: (k, v) => { attrs[k] = v; }, scrollIntoView() {}, click: () => log.push("pick:" + id),
      classList: { toggle: (c, on) => { on ? cls.add(c) : cls.delete(c); } } };
  };
  let items = [];
  const hidden = new Set(["hidden"]);
  const tagSuggest = { classList: { contains: (c) => hidden.has(c), add: (c) => hidden.add(c), remove: (c) => hidden.delete(c) }, querySelectorAll: () => items };
  const attrs = {}; const handlers = {};
  const input = { value: "", addEventListener: (t, f) => { handlers[t] = f; }, setAttribute: (k, v) => { attrs[k] = v; }, removeAttribute: (k) => { delete attrs[k]; } };
  const sandbox = run(code, {
    $: (id) => (id === "modal-tag-input" ? input : null), tagSuggest,
    modalState: { tags: [] }, toast() {}, t: (k) => k,
    renderTagSuggest: (all) => { items = [mkItem("i0"), mkItem("i1"), mkItem("i2")]; hidden.delete("hidden"); },
    addTagToModal: () => true, renderTagChips() {},
  });
  sandbox.wire();
  const key = (k) => { const ev = { key: k, prevented: false, preventDefault() { this.prevented = true; }, stopPropagation() {} }; handlers.keydown(ev); return ev; };
  key("ArrowDown");
  check(!hidden.has("hidden") && attrs["aria-activedescendant"] === "i0", `ArrowDown on a closed list opens it on the first row, got ${attrs["aria-activedescendant"]}`);
  key("ArrowDown"); key("ArrowDown");
  check(attrs["aria-activedescendant"] === "i2", "ArrowDown walks down");
  key("ArrowDown");
  check(attrs["aria-activedescendant"] === "i0", "ArrowDown wraps");
  key("ArrowUp");
  check(attrs["aria-activedescendant"] === "i2" && items[2].cls.has("active") && !items[0].cls.has("active"), "ArrowUp wraps back and marks one row");
  const ev = key("Enter");
  check(log.join() === "pick:i2" && ev.prevented, `Enter picks the highlighted row, got ${log}`);
  key("Escape");
  check(!("aria-activedescendant" in attrs), "closing the list clears the active descendant");
  log.length = 0; input.value = "";
  key("Enter");
  check(log.length === 0, "Enter with no highlighted row commits the typed text instead");
});

// media.css: the volume popup shows while the sound button or the slider has focus, so Tab can reach the slider.
later(async () => {
  const css = fs.readFileSync(path.join(uiDir, "styles", "media.css"), "utf8");
  check(/\.media-sound:focus-within \.media-vol-pop[^{]*\{[^}]*visibility:\s*visible/.test(css), "the volume popup is visible on :focus-within");
});

// main.js + core.js: Escape on about, dupes, trash and help gives focus back to the opener.
later(async () => {
  const main = read("main.js");
  for (const id of ['$("about-modal")', '$("dupes-modal")', '$("trash-modal")', "helpEl"]) {
    check(main.includes(`hideRestoringFocus(${id})`), `Escape closes ${id} through hideRestoringFocus`);
  }
  const core = read("core.js");
  const a = core.indexOf("const dialogOpeners");
  if (a === -1) { check(false, "dialog opener memory missing"); return; }
  const code = (core.slice(a, core.indexOf("// An unprompted notice")).replace(/export /g, "")) + "\n;globalThis.api = { rememberOpener, hideRestoringFocus };";
  const log = [];
  const el = (name, connected = true) => ({ name, isConnected: connected, focus: () => log.push("focus:" + name) });
  const dlg = () => { const h = new Set(["hidden"]); return { classList: { contains: (c) => h.has(c), add: (c) => h.add(c), remove: (c) => h.delete(c) } }; };
  const doc = { activeElement: null, body: el("body") };
  const sandbox = run(code, { document: doc, WeakMap });
  const { rememberOpener, hideRestoringFocus } = sandbox.api;
  const d = dlg();
  doc.activeElement = el("btn");
  rememberOpener(d);
  d.classList.remove("hidden");
  doc.activeElement = el("inside");
  rememberOpener(d); // already open: must not overwrite the opener
  hideRestoringFocus(d);
  check(d.classList.contains("hidden") && log.join() === "focus:btn", `focus returns to the opener, got ${log}`);
  log.length = 0;
  hideRestoringFocus(d);
  check(log.length === 0, "the opener is used once");
  const d2 = dlg();
  doc.activeElement = el("gone", false);
  rememberOpener(d2); d2.classList.remove("hidden");
  hideRestoringFocus(d2);
  check(log.length === 0, "a removed opener is skipped");
});

// core.js noticeToast: notices raised while hidden share one visibility listener and none is lost.
later(async () => {
  const core = read("core.js");
  const a = core.indexOf("const hiddenNotices");
  if (a === -1) { check(false, "notice queue missing"); return; }
  const code = core.slice(a, core.indexOf("// Logs the error and returns")).replace(/export /g, "") + "\n;globalThis.api = { noticeToast };";
  const shown = [];
  const listeners = new Set(); let added = 0;
  const doc = { hidden: true, addEventListener: (t, f) => { added++; listeners.add(f); }, removeEventListener: (t, f) => listeners.delete(f) };
  const timers = [];
  const sandbox = run(code, { document: doc, toast: (m) => shown.push(m), TOAST_NOTICE_MS: 1, setTimeout: (f) => timers.push(f) });
  sandbox.api.noticeToast("one"); sandbox.api.noticeToast("two");
  check(added === 1, `two hidden notices add one listener, got ${added}`);
  check(shown.length === 0, "nothing shows while hidden");
  doc.hidden = false;
  [...listeners].forEach((f) => f());
  check(shown.join() === "one" && listeners.size === 0, `the first shows when visible and the listener is gone, got ${shown}`);
  while (timers.length) timers.shift()();
  check(shown.join() === "one,two", `the second follows, got ${shown}`);
  sandbox.api.noticeToast("three");
  check(shown.join() === "one,two,three", "a notice while visible shows at once");
});

Promise.all(pending).then(() => {
  if (problems.length) { console.error("keyboard_race_fixes_check FAILED:\n" + problems.map((p) => "  - " + p).join("\n")); process.exit(1); }
  console.log("keyboard_race_fixes_check: ok");
});
