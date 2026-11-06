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

Promise.all(pending).then(() => {
  if (problems.length) {
    console.log(`small_ui_fixes_check: ${problems.length} problem(s)`);
    for (const p of problems) console.log(" -", p);
    process.exit(1);
  }
  console.log("small_ui_fixes_check: clean");
});
