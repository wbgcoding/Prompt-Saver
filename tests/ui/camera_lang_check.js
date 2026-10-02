// Camera window (ui/camera.js): the window stays open while the user can switch the
// UI language in the main window, so its labels, button names and the silent-take
// option must follow a `language-changed` event without a restart.
//
// Runs the real file in a node vm with stand-in elements and a scripted backend.
// Run:  node tests/ui/camera_lang_check.js      Exit: 0 = clean, 1 = problems.
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

function fakeEl(attrs) {
  const cls = new Set();
  const own = { ...(attrs || {}) };
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
  const listeners = {};
  const settings = { language: "en", ui_texts: {}, ui_values: {} };
  const els = {};
  const closeBtn = (els["cam-close"] = fakeEl({ "data-i18n-title": "close" }));
  const pauseBtn = (els["cam-pause-btn"] = fakeEl({ "data-i18n-title": "recBarPause" }));
  const sandbox = {
    console, navigator: { language: "en-US", mediaDevices: { enumerateDevices: async () => [], getUserMedia: async () => { throw new Error("none"); } } },
    setTimeout, clearTimeout, setInterval, clearInterval,
    document: {
      getElementById: (id) => (els[id] ||= fakeEl()),
      querySelector: () => fakeEl(),
      createElement: () => fakeEl(),
      querySelectorAll: (sel) => (sel === "[data-i18n-title]" ? [closeBtn, pauseBtn] : []),
      addEventListener() {},
    },
    __shared: shared, __i18n: i18n,
    __imp: (f) => f === "floattheme.js" ? Promise.resolve({ applyFloatTheme() {} }) : import(pathToFileURL(path.join(uiDir, f)).href),
  };
  sandbox.window = sandbox;
  sandbox.addEventListener = () => {};
  sandbox.__TAURI__ = {
    core: {
      invoke: async (cmd) => {
        if (cmd === "get_settings") return JSON.parse(JSON.stringify(settings));
        if (cmd === "default_recording_dir") return "C:/rec";
        return null;
      },
    },
    event: { listen: (name, fn) => { listeners[name] = fn; return Promise.resolve(() => {}); } },
    webviewWindow: { getCurrentWebviewWindow: () => ({ startDragging() {}, close() {} }) },
  };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox, { filename: "camera.js" });

  const tick = (ms = 200) => new Promise((r) => setTimeout(r, ms));
  const { I18N } = require("../i18n_load.js");
  const problems = [];
  const expect = (label, got, want) => {
    if (got !== want) problems.push(`${label}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
  };

  await tick();
  expect("start (en): start button", els["cam-start-btn"].textContent, I18N.en.cameraStart);
  expect("start (en): close title", closeBtn.title, I18N.en.close);
  expect("start (en): silent option", els["cam-audio-select"].options[0] && els["cam-audio-select"].options[0].textContent, I18N.en.cameraAudioNone);

  // The permission answer arrives, no device exists, so the "no camera" notice shows.
  if (listeners["camera-permission-ready"]) listeners["camera-permission-ready"]();
  await tick();
  settings.language = "de";
  expect("language-changed listener registered", typeof listeners["language-changed"], "function");
  if (listeners["language-changed"]) listeners["language-changed"]("de");
  await tick(500);
  expect("after the switch (de): start button", els["cam-start-btn"].textContent, I18N.de.cameraStart);
  expect("after the switch (de): audio label", els["cam-audio-label"].textContent, I18N.de.cameraAudioSource);
  expect("after the switch (de): close title", closeBtn.title, I18N.de.close);
  expect("after the switch (de): close aria-label", closeBtn.getAttribute("aria-label"), I18N.de.close);
  expect("after the switch (de): pause title", pauseBtn.title, I18N.de.recBarPause);
  expect("after the switch (de): silent option", els["cam-audio-select"].options[0] && els["cam-audio-select"].options[0].textContent, I18N.de.cameraAudioNone);
  expect("after the switch (de): no-device text", els["cam-no-device"].textContent, I18N.de.cameraNoDevice);

  if (problems.length) {
    console.error("camera_lang_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("camera_lang_check: ok");
  process.exit(0);
}
main().catch((e) => { console.error(e); process.exit(1); });
