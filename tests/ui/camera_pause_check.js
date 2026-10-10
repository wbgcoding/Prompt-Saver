// Camera window (ui/camera.js): the running clock must leave out the time a take was paused,
// the same way MediaRecorder leaves it out of the file. Runs the real file in a node vm with
// stand-in elements, a scripted backend, a fake recorder and a hand-driven clock.
// Run:  node tests/ui/camera_pause_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "..", "ui");
const src = fs.readFileSync(path.join(uiDir, "camera.js"), "utf8").replace(/\r\n/g, "\n");
const code = src
  .replace(/^import \{[^}]*\} from "\.\/floatShared\.js";$/m, "const { fmtTimeMMSS, takeElapsedMs } = __shared;")
  .replace(/^import \{ LANGS, fullTexts \} from "\.\/i18n\.js";$/m, "const { LANGS, fullTexts } = __i18n;")
  .replace(/^import \{[^}]*\} from "\.\/selectpop\.js";$/m, "const attachSelectPicker = () => {};")
  .replace(/import\("\.\/([\w.]+)"\)/g, "__imp(\"$1\")");

function fakeEl() {
  const cls = new Set();
  const own = {};
  const el = {
    textContent: "", title: "", disabled: false, value: "", style: {}, options: [], children: [], h: {},
    classList: {
      add: (...c) => c.forEach((x) => cls.add(x)),
      remove: (...c) => c.forEach((x) => cls.delete(x)),
      toggle: (c, on) => { if (on === undefined ? !cls.has(c) : on) cls.add(c); else cls.delete(c); },
      contains: (c) => cls.has(c),
    },
    addEventListener(type, fn) { el.h[type] = fn; }, removeEventListener() {}, focus() {},
    click() { return el.disabled ? undefined : el.h.click && el.h.click(); },
    setAttribute(k, v) { own[k] = String(v); },
    getAttribute(k) { return own[k] ?? null; },
    querySelector: () => null, closest: () => null,
    appendChild(c) { this.options.push(c); this.children.push(c); return c; },
    getBoundingClientRect: () => ({ width: 300, height: 40 }),
  };
  Object.defineProperty(el, "innerHTML", { get: () => "", set: () => { el.options = []; el.children = []; } });
  return el;
}

async function main() {
  globalThis.document = globalThis.document || { addEventListener() {}, removeEventListener() {} };
  const shared = await import(pathToFileURL(path.join(uiDir, "floatShared.js")).href);
  const i18n = await import(pathToFileURL(path.join(uiDir, "i18n.js")).href);
  const els = {};
  const listeners = {};
  let now = 1_000_000;
  const timers = [];
  class FakeDate extends Date { static now() { return now; } }
  class FakeRecorder {
    static isTypeSupported() { return true; }
    constructor() { this.state = "inactive"; }
    start() { this.state = "recording"; if (this.onstart) this.onstart(); }
    pause() { this.state = "paused"; }
    resume() { this.state = "recording"; }
    stop() { this.state = "inactive"; }
  }
  const stream = { getTracks: () => [] };
  const sandbox = {
    console, Date: FakeDate, MediaRecorder: FakeRecorder,
    navigator: { language: "en-US", mediaDevices: { enumerateDevices: async () => [], getUserMedia: async () => stream } },
    setTimeout, clearTimeout,
    setInterval: (fn) => { timers.push(fn); return timers.length; },
    clearInterval: () => {},
    document: {
      getElementById: (id) => (els[id] ||= fakeEl()),
      querySelector: () => fakeEl(), createElement: () => fakeEl(),
      querySelectorAll: () => [], addEventListener() {},
    },
    __shared: shared, __i18n: i18n,
    __imp: (f) => f === "floattheme.js" ? Promise.resolve({ applyFloatTheme() {} }) : import(pathToFileURL(path.join(uiDir, f)).href),
  };
  sandbox.window = sandbox;
  sandbox.addEventListener = () => {};
  sandbox.__TAURI__ = {
    core: {
      invoke: async (cmd) => {
        if (cmd === "get_settings") return { language: "en", ui_texts: {}, ui_values: {} };
        if (cmd === "default_recording_dir") return "C:/rec";
        if (cmd === "camera_begin_recording") return { path: "C:/rec/take.webm" };
        if (cmd === "camera_screen_elapsed_ms") return null;
        return null;
      },
    },
    event: { listen: (name, fn) => { listeners[name] = fn; return Promise.resolve(() => {}); } },
    webviewWindow: { getCurrentWebviewWindow: () => ({ startDragging() {}, close() {} }) },
  };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox, { filename: "camera.js" });

  const wait = (ms = 150) => new Promise((r) => setTimeout(r, ms));
  const problems = [];
  const expect = (label, got, want) => {
    if (got !== want) problems.push(`${label}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
  };
  const timeEl = els["cam-rec-time"];
  const tick = () => timers.forEach((fn) => fn());

  // The pure helper first: a running take, a take paused right now, a resumed one.
  expect("helper: running", shared.takeElapsedMs ? shared.takeElapsedMs(10_000, 4_000, 0, 0) : "missing", 6000);
  expect("helper: paused freezes at the pause", shared.takeElapsedMs ? shared.takeElapsedMs(20_000, 4_000, 7_000, 0) : "missing", 3000);
  expect("helper: pause time left out", shared.takeElapsedMs ? shared.takeElapsedMs(12_000, 4_000, 0, 3_000) : "missing", 5000);

  await wait();
  listeners["camera-permission-ready"]();
  await wait();
  els["cam-start-btn"].click();
  await wait();
  now += 2000; tick();
  expect("after 2 s", timeEl.textContent, "00:02");
  els["cam-pause-btn"].click();
  now += 3000; tick();
  expect("while paused for 3 s", timeEl.textContent, "00:02");
  els["cam-pause-btn"].click();
  now += 1000; tick();
  expect("1 s after the resume", timeEl.textContent, "00:03");

  // A second pause adds to the first, and a new take starts from zero again.
  els["cam-pause-btn"].click();
  now += 5000;
  els["cam-pause-btn"].click();
  now += 2000; tick();
  expect("after a second pause", timeEl.textContent, "00:05");
  els["cam-stop-btn"].click();
  await wait();
  els["cam-start-btn"].disabled = false;
  els["cam-start-btn"].click();
  await wait();
  now += 1000; tick();
  expect("a new take starts at zero", timeEl.textContent, "00:01");

  if (problems.length) {
    console.error("camera_pause_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("camera_pause_check: ok");
  process.exit(0);
}
main().catch((e) => { console.error(e); process.exit(1); });
