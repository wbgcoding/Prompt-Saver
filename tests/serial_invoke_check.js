// Ordered backend calls (ui/floatShared.js: serialInvoke, orderedInvoke).
// Run:  node tests/serial_invoke_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
if (typeof document === "undefined") global.document = { addEventListener() {} };

const started = [];
let delays = {};
global.window = {
  __TAURI__: {
    core: {
      invoke: (cmd, args) => {
        started.push(cmd + ":" + (args && args.v));
        const ms = delays[cmd + ":" + (args && args.v)] || 0;
        return new Promise((resolve, reject) => setTimeout(() => (args && args.fail ? reject(new Error("boom")) : resolve(cmd + ":" + (args && args.v))), ms));
      },
    },
  },
};
const { serialInvoke, orderedInvoke } = require(path.join(__dirname, "..", "ui", "floatShared.js"));

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

(async () => {
  // An idle key starts its call at once.
  started.length = 0;
  const first = serialInvoke("k", "set_theme", { v: 1 });
  check("an idle key calls straight away", started, ["set_theme:1"]);
  await first;
  await sleep(0);

  // A slow first call still goes out before a quick second one.
  started.length = 0;
  delays = { "set_theme:1": 40 };
  const a = serialInvoke("k", "set_theme", { v: 1 });
  const b = serialInvoke("k", "set_theme", { v: 2 });
  check("the second call waits for the first", started, ["set_theme:1"]);
  await Promise.all([a, b]);
  check("calls reach the backend in order", started, ["set_theme:1", "set_theme:2"]);

  // A failed call reaches its caller and does not stop the chain.
  started.length = 0;
  delays = {};
  const bad = serialInvoke("k", "set_theme", { v: 1, fail: true });
  const next = serialInvoke("k", "set_theme", { v: 2 });
  let failed = false;
  await bad.catch(() => { failed = true; });
  check("the caller of a failed call sees the error", failed, true);
  check("the call after a failure still runs", await next, "set_theme:2");

  // Other keys do not wait for each other.
  started.length = 0;
  delays = { "set_theme:1": 40 };
  const slow = serialInvoke("x", "set_theme", { v: 1 });
  const other = serialInvoke("y", "set_language", { v: 2 });
  check("another key starts at once", started, ["set_theme:1", "set_language:2"]);
  await Promise.all([slow, other]);

  // Rotation: a peek waits for the commit before it, per prompt only.
  started.length = 0;
  delays = { "rotation_commit:p1": 40 };
  const commit = orderedInvoke("rotation_commit", { v: "p1", id: "p1" });
  const peek = orderedInvoke("rotation_peek", { v: "p1x", id: "p1" });
  const peekOther = orderedInvoke("rotation_peek", { v: "p2", id: "p2" });
  await sleep(5);
  check("a peek of the same prompt waits, another prompt does not", started, ["rotation_commit:p1", "rotation_peek:p2"]);
  await Promise.all([commit, peek, peekOther]);
  check("the peek follows its commit", started, ["rotation_commit:p1", "rotation_peek:p2", "rotation_peek:p1x"]);

  // The editor draft write and its discard share one chain.
  started.length = 0;
  delays = { "save_editor_draft:1": 40 };
  const tick = orderedInvoke("save_editor_draft", { v: 1 });
  const discard = orderedInvoke("discard_editor_draft", { v: 2 });
  await sleep(5);
  check("a discard waits for the draft write in flight", started, ["save_editor_draft:1"]);
  await Promise.all([tick, discard]);
  check("the discard follows the draft write", started, ["save_editor_draft:1", "discard_editor_draft:2"]);

  // Hotkey capture on/off pairs keep their order.
  started.length = 0;
  delays = { "set_hotkey_capture:1": 40 };
  const capOn = orderedInvoke("set_hotkey_capture", { v: 1, active: true });
  const capOff = orderedInvoke("set_hotkey_capture", { v: 2, active: false });
  await sleep(5);
  check("the capture-off call waits for the capture-on call", started, ["set_hotkey_capture:1"]);
  await Promise.all([capOn, capOff]);

  // Commands outside the table are not chained.
  started.length = 0;
  delays = { "copy_text:1": 40 };
  const c1 = orderedInvoke("copy_text", { v: 1 });
  const c2 = orderedInvoke("copy_text", { v: 2 });
  check("an unordered command is not held back", started, ["copy_text:1", "copy_text:2"]);
  await Promise.all([c1, c2]);

  if (problems.length) {
    console.log(problems.join("\n"));
    process.exit(1);
  }
  console.log("serial_invoke_check: clean");
})();
