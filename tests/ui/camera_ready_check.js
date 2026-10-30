// Camera window (ui/camera.js): the first preview waits for the saved settings
// (camera, frame rate, resolution), also when the permission event fired before the page
// listened and the backend's answer to camera_permission_ready stands in for it. A settings
// read slower than that answer must not open the default camera first and then stay on it.
//
// Runs the real file in a node vm with stand-in elements and a scripted backend.
// Run:  node tests/ui/camera_ready_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "..", "ui");
const src = fs.readFileSync(path.join(uiDir, "camera.js"), "utf8").replace(/\r\n/g, "\n");
const code = src
  .replace(/^import \{[^}]*\} from "\.\/floatShared\.js";$/m, "const { fmtTimeMMSS } = __shared;")
  .replace(/^import \{ LANGS, fullTexts \} from "\.\/i18n\.js";$/m, "const { LANGS, fullTexts } = __i18n;")
  .replace(/^import \{[^}]*\} from "\.\/selectpop\.js";$/m, "const attachSelectPicker = () => {};")
  .replace(/import\("\.\/([\w.]+)"\)/g, "__imp(\"$1\")");

function fakeEl() {
  const cls = new Set();
  const own = {};
  const el = {
    textContent: "", title: "", disabled: false, value: "", style: {}, options: [], children: [],
    classList: {
      add: (...c) => c.forEach((x) => cls.add(x)),
      remove: (...c) => c.forEach((x) => cls.delete(x)),
      toggle: (c, on) => { if (on === undefined ? !cls.has(c) : on) cls.add(c); else cls.delete(c); },
      contains: (c) => cls.has(c),
    },
    addEventListener() {}, removeEventListener() {}, click() {}, focus() {},
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
  const settings = { language: "en", ui_texts: { cameraDevice: "cam-2" }, ui_values: {} };
  const els = {};
  const asked = [];
  const sandbox = {
    console, setTimeout, clearTimeout, setInterval, clearInterval,
    navigator: {
      language: "en-US",
      mediaDevices: {
        enumerateDevices: async () => [],
        getUserMedia: async (c) => { asked.push(c); throw new Error("none"); },
      },
    },
    document: {
      documentElement: {},
      documentElement: {},
      getElementById: (id) => (els[id] ||= fakeEl()),
      querySelector: () => fakeEl(),
      createElement: () => fakeEl(),
      querySelectorAll: () => [],
      addEventListener() {},
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
      // Permission is already granted (its event fired before the listener); the settings
      // read answers much later.
      invoke: async (cmd) => {
        if (cmd === "camera_permission_ready") return true;
        if (cmd === "get_settings") { await new Promise((r) => setTimeout(r, 2200)); return JSON.parse(JSON.stringify(settings)); }
        if (cmd === "default_recording_dir") return "C:/rec";
        return null;
      },
    },
    event: { listen: () => Promise.resolve(() => {}) },
    webviewWindow: { getCurrentWebviewWindow: () => ({ startDragging() {}, close() {} }) },
  };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox, { filename: "camera.js" });

  const tick = (ms) => new Promise((r) => setTimeout(r, ms));
  const problems = [];
  await tick(1900); // permission answered, settings still pending
  if (asked.length !== 0) problems.push(`the camera opened before the settings arrived (${asked.length} request)`);
  await tick(1200); // settings in
  if (asked.length === 0) problems.push("no preview was started once the settings arrived");
  else if (!asked[0].video.deviceId || asked[0].video.deviceId.exact !== "cam-2") {
    problems.push(`first preview ignored the saved camera: ${JSON.stringify(asked[0].video)}`);
  }

  if (problems.length) {
    console.error("camera_ready_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("camera_ready_check: ok");
  process.exit(0);
}
main().catch((e) => { console.error(e); process.exit(1); });
