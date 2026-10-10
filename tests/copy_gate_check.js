// Click gate shared by the pill and the folder window (ui/floatShared.js: copyGate,
// readCopyPrefs). Run:  node tests/copy_gate_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
if (typeof document === "undefined") global.document = { addEventListener() {} };
const { copyGate, readCopyPrefs } = require(path.join(__dirname, "..", "ui", "floatShared.js"));

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

(async () => {
  const prefs = readCopyPrefs({});
  check("defaults", [prefs.cooldownMs, prefs.apOn, prefs.apDblMs], [2000, true, 400]);
  const custom = readCopyPrefs({ ui_values: { copyCooldownMs: 0, dblClickMs: 120 }, ui_flags: { autoPaste: false } });
  check("stored values", [custom.cooldownMs, custom.apDblMs, custom.apOn], [0, 120, false]);

  // Stamps are taken before the await: a second click while the first copy is in
  // flight is already the double-click paste.
  let release;
  const slow = () => new Promise((r) => { release = r; });
  const first = copyGate("a", slow, prefs);
  check("click during an in-flight copy pastes", await copyGate("a", async () => true, prefs), "paste");
  release(true);
  check("first copy lands", await first, "copied");
  // Next click: the paste reset the double-click window, the cooldown still stands.
  check("cooldown holds", await copyGate("a", async () => true, prefs), "throttled");

  // A failed copy rolls both stamps back, so the next click copies at once.
  check("failed copy", await copyGate("b", async () => false, prefs), "failed");
  check("retry after a failure", await copyGate("b", async () => true, prefs), "copied");

  // A hand-off keeps the cooldown but never arms the paste window.
  check("hand-off", await copyGate("c", async () => "handoff", prefs), "handoff");
  check("second click after a hand-off is throttled, not a paste", await copyGate("c", async () => true, prefs), "throttled");

  // Ids do not share stamps; cooldown 0 never throttles; auto-paste off never pastes.
  check("other id", await copyGate("d", async () => true, prefs), "copied");
  check("no cooldown 1", await copyGate("e", async () => true, custom), "copied");
  check("no cooldown 2", await copyGate("e", async () => true, custom), "copied");

  if (problems.length) { console.error(problems.join("\n")); process.exit(1); }
  console.log("copy_gate_check: ok");
})();
