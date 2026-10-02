// Recording bar (ui/recbar.js): the bar window is only hidden between recordings,
// so its button titles, labels and warnings must follow a UI language switch made
// in between, the same way the screenshot overlay does.
//
// Runs the real file in a node vm with stand-in elements and a scripted backend.
// Run:  node tests/ui/recbar_lang_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "..", "ui");
const src = fs.readFileSync(path.join(uiDir, "recbar.js"), "utf8").replace(/\r\n/g, "\n");
const code = src
  .replace(/^import \{[^}]*\} from "\.\/floatShared\.js";$/m, "const { fmtTimeMMSS, fmtSizeCompact } = __shared;")
  .replace(/import\("\.\/([\w.]+)"\)/g, "__imp(\"$1\")");

function fakeEl(attrs) {
  const cls = new Set();
  const own = { ...(attrs || {}) };
  return {
    textContent: "", title: "", disabled: false, style: {},
    classList: {
      add: (...c) => c.forEach((x) => cls.add(x)),
      remove: (...c) => c.forEach((x) => cls.delete(x)),
      toggle: (c, on) => { if (on === undefined ? !cls.has(c) : on) cls.add(c); else cls.delete(c); },
      contains: (c) => cls.has(c),
    },
    addEventListener(t, fn) { (this.h ||= {})[t] = fn; },
    click() { return this.h && this.h.click && this.h.click(); },
    setAttribute(k, v) { own[k] = String(v); },
    getAttribute(k) { return own[k] ?? null; },
    closest: () => null,
    getBoundingClientRect: () => ({ width: 300, height: 40 }),
  };
}

async function main() {
  globalThis.document = globalThis.document || { addEventListener() {}, removeEventListener() {} };
  const shared = await import(pathToFileURL(path.join(uiDir, "floatShared.js")).href);
  const listeners = {};
  const settings = { language: "en" };
  const els = {};
  // The titled/labelled elements recbar.html carries, as the real page would list them.
  const titled = [
    ["stop-btn", "recBarStop"], ["cancel-btn", "recBarCancel"], ["continue-btn", "recBarContinue"],
  ];
  const titledEls = titled.map(([id, key]) => {
    const el = (els[id] ||= fakeEl());
    el.getAttribute = ((orig) => (k) => (k === "data-i18n-title" ? key : orig(k)))(el.getAttribute);
    return el;
  });
  const sandbox = {
    console, navigator: { language: "en-US" },
    setTimeout, clearTimeout, setInterval, clearInterval,
    ResizeObserver: class { observe() {} },
    document: {
      getElementById: (id) => (els[id] ||= fakeEl()),
      querySelectorAll: (sel) => (sel === "[data-i18n-title]" ? titledEls : []),
      addEventListener() {},
    },
    __shared: shared,
    __imp: (f) => f === "floattheme.js" ? Promise.resolve({ applyFloatTheme() {} }) : import(pathToFileURL(path.join(uiDir, f)).href),
  };
  sandbox.window = sandbox;
  sandbox.addEventListener = () => {};
  sandbox.__TAURI__ = {
    core: {
      invoke: async (cmd) => {
        if (cmd === "recording_status") {
          return {
            paused: false, elapsed_ms: 10000, bytes: 0, show_size: false, target_switch_enabled: true,
            audio_system: true, audio_mic: true, stop_hotkey_ok: true, pause_hotkey_ok: true,
          };
        }
        if (cmd === "get_settings") return { ...settings };
        return null;
      },
    },
    event: { listen: (name, fn) => { listeners[name] = fn; return Promise.resolve(() => {}); } },
    webviewWindow: { getCurrentWebviewWindow: () => ({}) },
  };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox, { filename: "recbar.js" });

  const tick = (ms = 150) => new Promise((r) => setTimeout(r, ms));
  const { I18N } = require("../i18n_load.js");
  const problems = [];
  const expect = (label, got, want) => {
    if (got !== want) problems.push(`${label}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
  };

  await tick(700);
  expect("start (en): stop title", els["stop-btn"].title, I18N.en.recBarStop);
  expect("start (en): pause title", els["pause-btn"].title, I18N.en.recBarPause);
  expect("start (en): mute system title", els["audio-sys"].title, I18N.en.recBarMuteSystemAudio);

  // The user switches the UI language in the settings; the backend announces it.
  settings.language = "de";
  expect("language-changed listener registered", typeof listeners["language-changed"], "function");
  if (listeners["language-changed"]) listeners["language-changed"]("de");
  await tick(700);
  expect("after the switch (de): stop title", els["stop-btn"].title, I18N.de.recBarStop);
  expect("after the switch (de): stop aria-label", els["stop-btn"].getAttribute("aria-label"), I18N.de.recBarStop);
  expect("after the switch (de): pause title", els["pause-btn"].title, I18N.de.recBarPause);
  expect("after the switch (de): mute system title", els["audio-sys"].title, I18N.de.recBarMuteSystemAudio);
  expect("after the switch (de): mute mic title", els["audio-mic"].title, I18N.de.recBarMuteMic);
  expect("after the switch (de): target switch title", els["switch-btn"].title, I18N.de.recBarSwitchTarget);

  // And back again.
  settings.language = "en";
  if (listeners["language-changed"]) listeners["language-changed"]("en");
  await tick(700);
  expect("switched back (en): stop title", els["stop-btn"].title, I18N.en.recBarStop);

  if (problems.length) {
    console.error("recbar_lang_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("recbar_lang_check: ok");
  process.exit(0);
}
main().catch((e) => { console.error(e); process.exit(1); });
