// Pure helpers of the screenshot overlay (ui/snip.js), run on the real extracted source:
//   snipErrorText: the overlay text for a failed capture, one mapping for both callers.
//   pickWindow: which window a pointer hits (the list carries each window's visible
//   frame, so a click on a title bar selects that window, not the one behind it).
// Run:  node tests/ui/snip_pick_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "..", "..", "ui", "snip.js"), "utf8").replace(/\r\n/g, "\n");

function extract(name) {
  const at = src.indexOf("function " + name + "(");
  if (at < 0) throw new Error("function " + name + " not found in ui/snip.js");
  const end = src.indexOf("\n}\n", at);
  return src.slice(at, end + 3);
}

const problems = [];
function check(label, got, want) {
  if (JSON.stringify(got) !== JSON.stringify(want)) problems.push(label + ": got " + JSON.stringify(got) + ", want " + JSON.stringify(want));
}

try {
  const ctx = vm.createContext({});
  vm.runInContext(extract("pickWindow"), ctx);
  const pick = (ws, p) => vm.runInContext("pickWindow", ctx)(ws, p);
  const A = { id: 1, x: 0, y: 0, width: 100, height: 100 };
  const B = { id: 2, x: 0, y: 0, width: 200, height: 200 };
  // A sits above B and its list rect is its full frame: the title bar point is A's.
  const hit = pick([A, B], { x: 50, y: 10 });
  check("title bar point picks the top window", hit && hit.id, 1);
  check("a point outside the top window picks the one behind", (pick([A, B], { x: 150, y: 150 }) || {}).id, 2);
  check("a point over nothing picks nothing", pick([A, B], { x: 500, y: 500 }), null);
  vm.runInContext(extract("snipErrorText"), ctx);
  const text = (err) => vm.runInContext("snipErrorText", ctx)(err, { ocrNoLang: "NO-LANG" });
  check("no ocr language maps to its own text", text("ocr-no-language"), "NO-LANG");
  check("an Error object with that message maps too", text(new Error("ocr-no-language")), "NO-LANG");
  check("any other error uses the default text", text("blocked"), undefined);
  check("a cancelled session shows nothing", text("cancelled"), null);
  // A colour pick that fails after a newer session started must not paint into that session.
  const shown = [];
  const pctx = vm.createContext({
    pickBusy: false, cur: { x: 1, y: 2 }, sessionSeq: 0, T: { pickFailed: "FAILED" },
    saveZoom() {}, showError: (m) => shown.push(m),
    invoke: () => { pctx.sessionSeq++; return Promise.reject(new Error("boom")); },
  });
  vm.runInContext(extract("snipErrorText"), pctx);
  vm.runInContext(extract("pickHere").replace("function pickHere", "this.pickHere = async function"), pctx);
  pctx.pickHere().then(() => {
    check("a stale pick error is not shown", shown, []);
    finish();
  });
} catch (e) {
  problems.push(String(e && e.message || e));
}

function finish() {
  if (problems.length) {
    console.error(problems.join("\n"));
    process.exit(1);
  }
  console.log("snip_pick_check: ok");
}
if (problems.length) finish();
