// Camera window (ui/camera.js): a recorder that cannot be built or started (the camera went
// away, an unsupported type) rolls the start back AND says why in the status strip instead of
// silently returning to idle. Runs the real file in a node vm with stand-in elements, a
// scripted backend and a fake recorder that throws on construction or on start.
// Run:  node tests/ui/camera_start_error_check.js      Exit: 0 = clean, 1 = problems.
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

async function scenario(failAt) {
  globalThis.document = globalThis.document || { addEventListener() {}, removeEventListener() {} };
  const shared = await import(pathToFileURL(path.join(uiDir, "floatShared.js")).href);
  const i18n = await import(pathToFileURL(path.join(uiDir, "i18n.js")).href);
  const els = {};
  const listeners = {};
  const discarded = [];
  class FakeRecorder {
    static isTypeSupported() { return true; }
    constructor() {
      if (failAt === "ctor") throw new Error("NotSupportedError");
      this.state = "inactive";
    }
    start() { if (failAt === "start") throw new Error("InvalidStateError"); this.state = "recording"; }
    stop() { this.state = "inactive"; }
  }
  const stream = { getTracks: () => [] };
  const sandbox = {
    console, MediaRecorder: FakeRecorder,
    navigator: { language: "en-US", mediaDevices: { enumerateDevices: async () => [], getUserMedia: async () => stream } },
    setTimeout, clearTimeout, setInterval: () => 1, clearInterval: () => {},
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
      invoke: async (cmd, args) => {
        if (cmd === "get_settings") return { language: "en", ui_texts: {}, ui_values: {} };
        if (cmd === "default_recording_dir") return "C:/rec";
        if (cmd === "camera_begin_recording") return { path: "C:/rec/take.webm" };
        if (cmd === "camera_discard_recording") discarded.push(args.path);
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
  await wait();
  listeners["camera-permission-ready"]();
  await wait();
  const before = els["cam-status-text"].textContent;
  els["cam-start-btn"].click();
  await wait(400);
  if (discarded.length !== 1) problems.push(`${failAt}: the reserved take was not released (${discarded.length} discards)`);
  const after = els["cam-status-text"].textContent;
  if (!after || after === before) problems.push(`${failAt}: no reason shown after the failed start (status "${after}")`);
  return problems;
}

async function main() {
  const problems = [...await scenario("ctor"), ...await scenario("start")];
  if (problems.length) {
    console.error("camera_start_error_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("camera_start_error_check: ok");
  process.exit(0);
}
main().catch((e) => { console.error(e); process.exit(1); });
