// The shared confirm dialog (ui/dialog.js, real file in a node vm over a small fake DOM):
// a dialog opened while another one is on screen waits its turn instead of answering the
// open one, each answer reaches its own caller, and Escape (dismissConfirm) only answers
// the dialog that is showing.
// Run:  node tests/ui/dialog_queue_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "..", "ui", "dialog.js"), "utf8")
  .replace(/^export (function|const|async function) /gm, "$1 ")
  + "\n;globalThis.__api = { confirmDialog, dismissConfirm, infoDialog, initDialogs };";

function makeEl(id) {
  const handlers = {};
  const classes = new Set(["hidden"]);
  return {
    id, textContent: "", innerHTML: "", value: "", type: "", placeholder: "", title: "", disabled: false, dataset: {},
    classList: {
      add: (c) => classes.add(c),
      remove: (c) => classes.delete(c),
      contains: (c) => classes.has(c),
      toggle: (c, on) => { const want = on === undefined ? !classes.has(c) : !!on; if (want) classes.add(c); else classes.delete(c); return want; },
    },
    addEventListener: (t, f) => { (handlers[t] ||= new Set()).add(f); },
    removeEventListener: (t, f) => { handlers[t]?.delete(f); },
    fire: (t) => { for (const f of [...(handlers[t] || [])]) f({ target: null }); },
    setAttribute() {}, appendChild() {}, focus() { doc.activeElement = this; }, select() {},
    querySelector: () => null,
  };
}
const els = {};
const doc = { activeElement: null, getElementById: (id) => (els[id] ||= makeEl(id)) };
const sandbox = { document: doc, setTimeout, clearTimeout, console, Promise, navigator: {} };
vm.createContext(sandbox);
vm.runInContext(src, sandbox);
const { confirmDialog, dismissConfirm, initDialogs } = sandbox.__api;
initDialogs({ t: (k) => k, toast() {} });

const problems = [];
const check = (label, actual, expected) => {
  if (actual !== expected) problems.push(`${label}: got ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`);
};
const tick = () => new Promise((r) => setTimeout(r, 20));
const shown = () => !els["confirm-modal"].classList.contains("hidden");
const title = () => els["confirm-title"].textContent;
const open = (name) => confirmDialog({ title: name, message: "m", confirmLabel: "ok", cancelLabel: "no" });

(async () => {
  // Two questions at once: the second waits.
  const a = open("A");
  const b = open("B");
  const c = open("C");
  let aDone = null;
  a.then((v) => { aDone = v; });
  await tick();
  check("A is on screen first", title(), "A");
  check("A is still open (not answered by B)", aDone, null);
  els["confirm-ok"].fire("click");
  await tick();
  check("A answered yes", aDone, true);
  check("B shows after A", title(), "B");
  check("the dialog stays visible for B", shown(), true);
  // Escape answers the showing dialog (B) only.
  check("dismissConfirm reports an open dialog", dismissConfirm(), true);
  check("B answered no", await b, false);
  await tick();
  check("C shows after B", title(), "C");
  els["confirm-ok"].fire("click");
  check("C answered yes", await c, true);
  await tick();
  check("nothing left on screen", shown(), false);
  check("dismissConfirm with nothing open", dismissConfirm(), false);
  // A fresh dialog on an idle queue opens at once (synchronously).
  const d = open("D");
  check("an idle queue shows at once", title(), "D");
  els["confirm-cancel"].fire("click");
  check("D answered no", await d, false);

  if (problems.length) {
    console.log("dialog_queue_check: FAIL\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("dialog_queue_check: ok");
})();
