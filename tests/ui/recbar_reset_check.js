// Recording bar (ui/recbar.js): the bar window is only hidden between recordings,
// so the page must come back alive for the next one. Runs the real file in a node
// vm against a scripted backend: start, Stop, then the "rec-bar-reset" event the
// backend sends with every show must re-arm the buttons, the timer and the poll.
// Run:  node tests/ui/recbar_reset_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "..", "ui");
const src = fs.readFileSync(path.join(uiDir, "recbar.js"), "utf8").replace(/\r\n/g, "\n");
const code = src
  .replace(/^import \{[^}]*\} from "\.\/floatShared\.js";$/m, "const { fmtTimeMMSS, fmtSizeCompact } = __shared;")
  .replace(/import\("\.\/([\w.]+)"\)/g, "__imp(\"$1\")");

function fakeEl() {
  const cls = new Set();
  return {
    textContent: "", title: "", disabled: false, style: {},
    classList: {
      add: (...c) => c.forEach((x) => cls.add(x)),
      remove: (...c) => c.forEach((x) => cls.delete(x)),
      toggle: (c, on) => { if (on === undefined ? !cls.has(c) : on) cls.add(c); else cls.delete(c); },
      contains: (c) => cls.has(c),
    },
    addEventListener(t, fn) { (this.h ||= {})[t] = fn; },
    setAttribute() {}, closest: () => null,
    getBoundingClientRect: () => ({ width: 300, height: 40 }),
  };
}

async function main() {
  globalThis.document = globalThis.document || { addEventListener() {}, removeEventListener() {} };
  const shared = await import(pathToFileURL(path.join(uiDir, "floatShared.js")).href);
  const listeners = {};
  const els = {};
  let active = true;
  let elapsed = 70000;
  const calls = [];
  const sandbox = {
    console, navigator: { language: "en-US" },
    setTimeout, clearTimeout, setInterval, clearInterval,
    ResizeObserver: class { observe() {} },
    document: { getElementById: (id) => (els[id] ||= fakeEl()), querySelectorAll: () => [] },
    __shared: shared,
    __imp: (f) => f === "floattheme.js" ? Promise.resolve({ applyFloatTheme() {} }) : import(pathToFileURL(path.join(uiDir, f)).href),
  };
  sandbox.window = sandbox;
  sandbox.addEventListener = () => {};
  sandbox.__TAURI__ = {
    core: {
      invoke: async (cmd) => {
        calls.push(cmd);
        if (cmd === "recording_status") {
          return active ? {
            paused: false, elapsed_ms: elapsed, bytes: 0, show_size: false, target_switch_enabled: false,
            stop_hotkey_ok: true, pause_hotkey_ok: true,
          } : null;
        }
        if (cmd === "get_settings") return { language: "en" };
        return null;
      },
    },
    event: { listen: (name, fn) => { listeners[name] = fn; return Promise.resolve(() => {}); } },
    webviewWindow: { getCurrentWebviewWindow: () => ({}) },
  };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox, { filename: "recbar.js" });

  const tick = (ms = 80) => new Promise((r) => setTimeout(r, ms));
  const problems = [];
  const expect = (label, got, want) => {
    if (got !== want) problems.push(`${label}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
  };
  const btns = () => ["pause-btn", "stop-btn", "cancel-btn"].map((id) => els[id].disabled);

  await tick();
  expect("first recording: time shown", els.time.textContent, "01:10");
  expect("first recording: buttons enabled", btns().join(), "false,false,false");

  // Stop, exactly as the user does: buttons go dead, the bar hides.
  await els["stop-btn"].h.click();
  active = false;
  expect("after Stop: buttons disabled", btns().join(), "true,true,true");

  // Second recording: the backend shows the same window again and announces it.
  active = true;
  elapsed = 2000;
  calls.length = 0;
  expect("reset listener registered", typeof listeners["rec-bar-reset"], "function");
  listeners["rec-bar-reset"]();
  await tick(700);
  expect("second recording: buttons enabled", btns().join(), "false,false,false");
  expect("second recording: time follows the new recording", els.time.textContent, "00:02");
  const polls = calls.filter((c) => c === "recording_status").length;
  if (polls < 2) problems.push(`second recording: poll not running (${polls} status calls)`);

  // A third one behaves the same.
  await els["cancel-btn"].h.click();
  active = true;
  elapsed = 500;
  listeners["rec-bar-reset"]();
  await tick(100);
  expect("third recording: buttons enabled", btns().join(), "false,false,false");
  expect("third recording: time", els.time.textContent, "00:00");
  els["stop-btn"].disabled = true; // leave nothing running
  sandbox.clearInterval(0);

  if (problems.length) {
    console.error("recbar_reset_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("recbar_reset_check: ok");
  process.exit(0);
}
main().catch((e) => { console.error(e); process.exit(1); });
