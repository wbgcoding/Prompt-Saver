// Small fixes in the select picker, hotkey capture, shortcuts overview, tag suggestions,
// library, folder window and lightbox scripts. Each section runs the real extracted source in
// a node vm. UI_CHECK_DIR points the test at another copy of ui/ (to prove it fails on older code).
// Run:  node tests/ui/small_ui_fixes_check.js      Exit: 0 = clean, 1 = problems.
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
const later = (fn) => { pending.push(fn().catch((e) => problems.push(String(e.stack || e.message || e)))); };
const run = (code, sandbox) => { vm.createContext(sandbox); vm.runInContext(code, sandbox); return sandbox; };
const tick = () => new Promise((r) => setImmediate(r));

// A just-enough element: listeners, children, classes, attributes.
function fakeEl(tag = "div") {
  const listeners = {};
  const classes = new Set();
  const el = {
    tagName: tag, children: [], style: {}, dataset: {}, attrs: {}, textContent: "", disabled: false,
    classList: {
      add: (c) => classes.add(c), remove: (c) => classes.delete(c),
      contains: (c) => classes.has(c),
      toggle: (c, on) => { const want = on === undefined ? !classes.has(c) : !!on; if (want) classes.add(c); else classes.delete(c); return want; },
    },
    set className(v) { classes.clear(); String(v).split(/\s+/).filter(Boolean).forEach((c) => classes.add(c)); },
    get className() { return [...classes].join(" "); },
    set innerHTML(v) { el.children = []; },
    addEventListener: (type, fn) => { (listeners[type] = listeners[type] || []).push(fn); },
    dispatch: (type, ev) => { for (const fn of listeners[type] || []) fn(ev); },
    dispatchEvent: (ev) => { for (const fn of listeners[ev.type] || []) fn(ev); return true; },
    appendChild: (c) => { el.children.push(c); return c; },
    querySelectorAll: () => el.children.filter((c) => !c.classList.contains("sep")),
    contains: (t) => t === el || el.children.includes(t),
    setAttribute: (k, v) => { el.attrs[k] = String(v); },
    removeAttribute: (k) => { delete el.attrs[k]; },
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 10, bottom: 10, width: 10, height: 10 }),
    scrollIntoView: () => {},
    focus: () => {},
  };
  return el;
}

// selectpop.js: only the primary button picks an entry or opens the list.
later(async () => {
  let src = read("selectpop.js").replace(/^import .*$/m, "").replace(/export /g, "");
  const doc = { ...fakeEl("document"), body: fakeEl("body"), createElement: (t) => fakeEl(t) };
  const sandbox = run(src + "\n;globalThis.api = { openValuePop, attachSelectPicker, closeValuePop };", {
    document: doc,
    window: fakeEl("window"),
    placePopover: () => {},
    PointerEvent: class { constructor(type, init) { this.type = type; Object.assign(this, init); } },
    Event: class { constructor(type) { this.type = type; } },
  });
  const { openValuePop, attachSelectPicker, closeValuePop } = sandbox.api;
  const ev = (type, button) => ({ type, button, preventDefault() {}, stopPropagation() {}, target: null });

  const picks = [];
  const anchor = fakeEl("button");
  openValuePop(anchor, [{ value: "a", label: "A" }, { value: "b", label: "B" }], "a", (v) => picks.push(v));
  const pop = doc.body.children[0];
  const rowB = pop.children[1];
  rowB.dispatch("pointerdown", ev("pointerdown", 2));
  check(picks.length === 0, "a right-click on an entry picks nothing");
  rowB.dispatch("pointerdown", ev("pointerdown", 1));
  check(picks.length === 0, "a middle-click on an entry picks nothing");
  rowB.dispatch("pointerdown", ev("pointerdown", 0));
  check(picks.join() === "b", "a primary click picks the entry");
  closeValuePop();

  // A synthetic keyboard pick carries no button field set explicitly: PointerEvent default is 0.
  const sel = fakeEl("select");
  sel.options = [{ value: "x", textContent: "X", style: {}, disabled: false }];
  sel.value = "x";
  attachSelectPicker(sel);
  sel.dispatch("mousedown", ev("mousedown", 2));
  check(!(pop.classList.contains("hidden") === false && pop.children.length > 0 && pop.children[0].textContent === "X"), "a right-click on a select does not open its list");
  sel.dispatch("mousedown", ev("mousedown", 0));
  check(pop.children.length === 1 && pop.children[0].textContent === "X", "a primary press opens the list");
});

// tags.js: ArrowUp on an open list with nothing highlighted (typing re-renders it) lands on the last row.
later(async () => {
  const src = read("tags.js");
  const a = src.indexOf("let tagSuggestActive");
  const b = src.indexOf("export const closeTagPop");
  if (a === -1 || b === -1) { check(false, "tag suggestion keyboard state missing"); return; }
  const code = src.slice(a, b).replace(/export /g, "") + "\n" + extractBlock(src, "export function wireTagInput").replace(/export /g, "")
    + "\n;globalThis.wire = wireTagInput; globalThis.unhighlight = () => { tagSuggestActive = -1; };";
  const mkItem = (id) => ({ id, setAttribute() {}, scrollIntoView() {}, click() {}, classList: { toggle() {} } });
  const items = [mkItem("i0"), mkItem("i1"), mkItem("i2")];
  const tagSuggest = { classList: { contains: () => false, add() {}, remove() {} }, querySelectorAll: () => items };
  const attrs = {}; const handlers = {};
  const input = { value: "", addEventListener: (t, f) => { handlers[t] = f; }, setAttribute: (k, v) => { attrs[k] = v; }, removeAttribute: (k) => { delete attrs[k]; } };
  const sandbox = run(code, {
    $: (id) => (id === "modal-tag-input" ? input : null), tagSuggest,
    modalState: { tags: [] }, toast() {}, t: (k) => k, renderTagSuggest() {}, addTagToModal: () => true, renderTagChips() {},
  });
  sandbox.wire();
  const key = (k) => handlers.keydown({ key: k, preventDefault() {}, stopPropagation() {} });
  sandbox.unhighlight();
  key("ArrowUp");
  check(attrs["aria-activedescendant"] === "i2", `ArrowUp with nothing highlighted picks the last row, got ${attrs["aria-activedescendant"]}`);
  sandbox.unhighlight();
  key("ArrowDown");
  check(attrs["aria-activedescendant"] === "i0", `ArrowDown with nothing highlighted picks the first row, got ${attrs["aria-activedescendant"]}`);
});

// shortcuts.js: two actions that share one description (close and keep, close and drop) list both keys in one row.
later(async () => {
  const { pathToFileURL } = require("url");
  const shortcuts = await import(pathToFileURL(path.join(uiDir, "shortcuts.js")).href);
  const rows = shortcuts.rowsForScope("snipEditor", (k) => k, "en");
  const close = rows.find((r) => r.desc === "snipEditCloseTip");
  check(close && /Enter/.test(close.keys) && /Esc/.test(close.keys), `the editor close row lists Enter and Esc, got ${close && close.keys}`);
  check(rows.filter((r) => r.desc === "snipEditCloseTip").length === 1, "the close row appears once");
  const redo = rows.find((r) => r.desc === "snipEditRedoTip");
  check(redo && !/ \/ /.test(redo.keys), `an Alt binding stays folded into its primary row, got ${redo && redo.keys}`);
});

// library.js doBatch("delete"): the paging counter follows the rows left, so the prompts that move up get their row.
later(async () => {
  const code = extractBlock(read("library.js"), "export async function doBatch").replace(/export /g, "") + "\n;globalThis.doBatch = doBatch;";
  const mk = (id) => ({ id, dataset: { id }, remove() { rows.splice(rows.indexOf(this), 1); } });
  const rows = ["a", "b", "c"].map(mk); // rendered rows; d, e have none yet
  const sandbox = run(code, {
    libItems: ["a", "b", "c", "d", "e"].map((id) => ({ id })),
    libShown: 3, libDisplayOrder: ["a", "b", "c"], libSelected: new Set(), libLastId: null,
    gridDirty: false, prompts: [], mirrorBatch: async () => true,
    $: () => ({ querySelectorAll: () => [...rows] }),
    renderLibrary() {}, updateLibraryCount() {}, updateBatchBar() {}, renderViews() {},
    libMatches: () => true,
  });
  await sandbox.doBatch("delete", {}, ["a"]);
  check(sandbox.libItems.length === 4, "the deleted prompt left the list");
  check(sandbox.libShown === 2, `libShown counts the rows still rendered, got ${sandbox.libShown}`);
});

// windowChrome.js lightbox: the same number of wheel notches in and out lands exactly on 1, not 1.0000000000000002.
later(async () => {
  const src = read("windowChrome.js");
  const code = "let lbScale = 1, lbX = 0, lbY = 0, lbDrag = null;\n"
    + ["export function lbMedia", "export function lbApply", "export function lbToLocal", "export function wireLightbox"]
      .map((m) => extractBlock(src, m).replace(/export /g, "")).join("\n")
    + "\n;globalThis.wire = wireLightbox; globalThis.state = () => ({ lbScale, lbX, lbY });";
  const handlers = {};
  const media = { style: {}, classList: { contains: () => false }, getBoundingClientRect: () => ({ left: 0, top: 0 }) };
  const stage = { classList: { toggle() {}, add() {}, remove() {} }, addEventListener: (t, f) => { handlers[t] = f; } };
  const els = { "lightbox-img": media, "lightbox-stage": stage, "lightbox-close": { addEventListener() {} } };
  const sandbox = run(code, { $: (id) => els[id], closeLightbox() {} });
  sandbox.wire();
  for (let n = 1; n <= 24; n++) {
    const wheel = (dy) => handlers.wheel({ deltaY: dy, clientX: 40, clientY: 30, preventDefault() {} });
    for (let i = 0; i < n; i++) wheel(-1);
    for (let i = 0; i < n; i++) wheel(1);
    const st = sandbox.state();
    check(st.lbScale === 1 && st.lbX === 0 && st.lbY === 0, `${n} notches in and out are back at 1 with no pan, got ${st.lbScale} ${st.lbX} ${st.lbY}`);
  }
});

Promise.all(pending).then(() => {
  if (problems.length) {
    console.log(`small_ui_fixes_check: ${problems.length} problem(s)`);
    for (const p of problems) console.log(" -", p);
    process.exit(1);
  }
  console.log("small_ui_fixes_check: clean");
});
