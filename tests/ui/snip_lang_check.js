// Screenshot overlay (ui/snip.js): the overlay window is only hidden between snips,
// so its hint and error texts must follow a UI language switch made in between.
//
// snip.js is a browser page script, so this runs the real file in a node vm with a
// stand-in document and a scripted backend (no Chromium needed: the page pulls its
// translations with a dynamic import, which the headless browser suites cannot wait
// for on their virtual clock). The dynamic imports are pointed at the real modules.
// Run:  node tests/ui/snip_lang_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "..", "ui");
const src = fs.readFileSync(path.join(uiDir, "snip.js"), "utf8").replace(/\r\n/g, "\n");
const code = src.replace(/import\("\.\/([\w.]+)"\)/g, "__imp(\"$1\")");

function fakeEl() {
  const cls = new Set();
  const el = {
    textContent: "",
    style: {},
    dataset: {},
    children: [],
    classList: {
      add: (...c) => c.forEach((x) => cls.add(x)),
      remove: (...c) => c.forEach((x) => cls.delete(x)),
      toggle: (c, on) => { if (on === undefined ? !cls.has(c) : on) cls.add(c); else cls.delete(c); },
      contains: (c) => cls.has(c),
    },
    addEventListener() {}, removeEventListener() {}, appendChild() {}, setAttribute() {},
    getContext: () => new Proxy({}, { get: () => () => {}, set: () => true }),
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 0, height: 0 }),
  };
  return el;
}

async function main() {
  const listeners = {};
  const settings = { language: "en" };
  const els = {};
  const sandbox = {
    console,
    navigator: { language: "en-US" },
    setTimeout, clearTimeout,
    requestAnimationFrame: () => 0,
    cancelAnimationFrame: () => {},
    document: {
      getElementById: (id) => (els[id] ||= fakeEl()),
      createElement: () => fakeEl(),
      body: fakeEl(),
    },
    __imp: (f) => import(pathToFileURL(path.join(uiDir, f)).href),
  };
  sandbox.window = sandbox;
  sandbox.innerWidth = 1920;
  sandbox.innerHeight = 1080;
  sandbox.addEventListener = () => {};
  sandbox.__TAURI__ = {
    core: {
      invoke: async (cmd) => {
        if (cmd === "get_settings") return { ...settings };
        if (cmd === "snip_should_begin") return false;
        if (cmd === "snip_background") return { width: 100, height: 100, src: "x.jpg", is_file: true };
        return null;
      },
      convertFileSrc: (p) => p,
    },
    event: { listen: (name, fn) => { listeners[name] = fn; return Promise.resolve(() => {}); } },
  };
  // The shared window helpers register document listeners when they load.
  globalThis.document = globalThis.document || { addEventListener() {}, removeEventListener() {} };
  vm.createContext(sandbox);
  vm.runInContext(code, sandbox, { filename: "snip.js" });

  const tick = () => new Promise((r) => setTimeout(r, 120));
  const hint = () => els["snip-hint"].textContent;
  const { I18N } = require("../i18n_load.js");
  const problems = [];
  const expect = (label, got, want) => {
    if (got !== want) problems.push(`${label}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
  };

  listeners["snip-begin"]();
  await tick();
  expect("first session (en)", hint(), I18N.en.snipHint);

  // The user switches the UI language in the settings; the backend announces it.
  settings.language = "de";
  if (listeners["language-changed"]) listeners["language-changed"]("de");
  await tick();
  listeners["snip-begin"]();
  await tick();
  expect("second session after the switch (de)", hint(), I18N.de.snipHint);

  if (problems.length) {
    console.error("snip_lang_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("snip_lang_check: ok");
}
main().catch((e) => { console.error(e); process.exit(1); });
