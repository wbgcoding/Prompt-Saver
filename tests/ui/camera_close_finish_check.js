// Camera window (ui/camera.js): a close pressed while a take is being finished waits for the
// finish, but a finish error is shown first instead of closing the window over it. Runs the real
// file in a node vm with stand-in elements, a scripted backend and a fake recorder.
// Run:  node tests/ui/camera_close_finish_check.js      Exit: 0 = clean, 1 = problems.
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
  let closed = 0;
  const timers = [];
  class FakeDate extends Date { static now() { return now; } }
  class FakeRecorder {
    static isTypeSupported() { return true; }
    constructor() { this.state = "inactive"; }
    start() { this.state = "recording"; if (this.onstart) this.onstart(); }
    pause() { this.state = "paused"; }
    resume() { this.state = "recording"; }
    stop() { this.state = "inactive"; setTimeout(() => this.onstop && this.onstop(), 0); }
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
  sandbox.requestAnimationFrame = () => 0;
  sandbox.cancelAnimationFrame = () => {};
  sandbox.__TAURI__ = {
    core: {
      invoke: async (cmd) => {
        if (cmd === "get_settings") return { language: "en", ui_texts: {}, ui_values: {} };
        if (cmd === "default_recording_dir") return "C:/rec";
        if (cmd === "camera_begin_recording") return { path: "C:/rec/take.webm" };
        if (cmd === "camera_screen_elapsed_ms") return null;
        if (cmd === "camera_finish_recording") throw new Error("finish failed");
        return null;
      },
    },
    event: { listen: (name, fn) => { listeners[name] = fn; return Promise.resolve(() => {}); } },
    webviewWindow: { getCurrentWebviewWindow: () => ({ startDragging() {}, close() { closed++; } }) },
  };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox, { filename: "camera.js" });

  const wait = (ms = 150) => new Promise((r) => setTimeout(r, ms));
  const problems = [];
  await wait();
  listeners["camera-permission-ready"]();
  await wait();
  els["cam-start-btn"].click();
  await wait();
  els["cam-stop-btn"].click();
  els["cam-close"].click(); // the finish is on its way
  await wait(400);
  if (!els["cam-status-text"] || !els["cam-status-text"].textContent) problems.push("the finish error was never shown");
  if (closed !== 0) problems.push("the window closed over the unseen finish error");
  if (problems.length) {
    console.error("camera_close_finish_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("camera_close_finish_check: ok");
  process.exit(0);
}
main().catch((e) => { console.error(e); process.exit(1); });
