// Tray filter row -> quick launcher hand-off (ui/tray.js, ui/launcher.js): whatever the
// user types while the launcher is still coming up must arrive exactly once, in order.
//
// Runs the real tray.js and launcher.js in node vms against a small model of the
// operating system and the backend: keystrokes go to whichever window owns the focus
// (and are lost on a launcher whose page is not loaded yet), the launcher page loads a
// while after its window was built, and every command takes a few milliseconds.
// HANDOFF_MODEL=old models the earlier backend, which focused the launcher the moment
// it was built; the default models the current one, which focuses it only when the
// launcher page reports it is ready (launcher_ready).
// Run:  node tests/ui/tray_launcher_handoff_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "..", "ui");
const MODEL = process.env.HANDOFF_MODEL === "old" ? "old" : "new";
const read = (f) => fs.readFileSync(path.join(uiDir, f), "utf8").replace(/\r\n/g, "\n");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function fakeEl() {
  const cls = new Set();
  const h = {};
  const el = {
    value: "", textContent: "", title: "", placeholder: "", hidden: false, className: "", disabled: false,
    style: { setProperty() {} }, dataset: {}, children: [], selection: -1,
    classList: {
      add: (...c) => c.forEach((x) => cls.add(x)),
      remove: (...c) => c.forEach((x) => cls.delete(x)),
      toggle: (c, on) => { if (on === undefined ? !cls.has(c) : on) cls.add(c); else cls.delete(c); },
      contains: (c) => cls.has(c),
    },
    addEventListener(t, fn) { (h[t] ||= []).push(fn); },
    removeEventListener() {},
    fire(t, ev) { for (const fn of h[t] || []) fn(ev || {}); },
    appendChild(c) { el.children.push(c); return c; },
    append() {}, setAttribute() {}, focus() {}, scrollIntoView() {}, contains: () => false,
    setSelectionRange(a) { el.selection = a; },
  };
  Object.defineProperty(el, "innerHTML", { get: () => "", set: () => { el.children = []; } });
  return el;
}

// One scripted run: keys typed every `keyMs`, launcher page loaded `loadMs` after its
// window was built, every command answered after `ipcMs`.
async function run({ text, keyMs, loadMs, ipcMs }) {
  const shared = await import(pathToFileURL(path.join(uiDir, "floatShared.js")).href);
  const i18n = await import(pathToFileURL(path.join(uiDir, "i18n.js")).href);
  const os = { focus: "tray", trayAlive: true, launcherLoaded: false };
  const trayEls = {}, launcherEls = {};
  const trayWinH = {};
  let launcherListeners = {};
  let prefill = "";
  const call = async (fn) => { await sleep(ipcMs); return fn(); };

  function bootLauncher() {
    const code = read("launcher.js")
      .replace(/^import \{ applyFloatTheme \} from "\.\/floattheme\.js";$/m, "const applyFloatTheme = () => {};")
      .replace(/^import \{ APP_NAME \} from "\.\/brand\.js";$/m, 'const APP_NAME = "App";')
      .replace(/^import \{ LANGS, compactTexts \} from "\.\/i18n\.js";$/m, "const { LANGS, compactTexts } = __i18n;")
      .replace(/^import \{ resolveLang \} from "\.\/floatShared\.js";$/m, "const { resolveLang } = __shared;");
    const sandbox = {
      console, setTimeout, clearTimeout, Date, __i18n: i18n, __shared: shared,
      document: {
        title: "", getElementById: (id) => (launcherEls[id] ||= fakeEl()), createElement: () => fakeEl(),
        addEventListener() {}, body: fakeEl(), hasFocus: () => os.focus === "launcher",
      },
      __TAURI__: {
        core: {
          invoke: (cmd) => call(() => {
            if (cmd === "launcher_items") {
              const p = prefill; prefill = "";
              return { prompts: [], language: "en", usage: {}, last_used: {}, prefill: p };
            }
            if (cmd === "launcher_ready") {
              if (MODEL === "new") { os.focus = "launcher"; (trayWinH.blur || []).forEach((fn) => fn()); }
              return null;
            }
            if (cmd === "get_settings") return {};
            if (cmd === "theme_vars") return {};
            return null;
          }),
        },
        event: { listen: (name, fn) => { launcherListeners[name] = fn; return Promise.resolve(() => {}); } },
      },
    };
    sandbox.window = { addEventListener() {}, __TAURI__: sandbox.__TAURI__ };
    vm.createContext(sandbox);
    vm.runInContext(code, sandbox, { filename: "launcher.js" });
    os.launcherLoaded = true;
  }

  const traySandbox = {
    console, setTimeout, clearTimeout, Date,
    document: {
      title: "", getElementById: (id) => (trayEls[id] ||= fakeEl()), createElement: () => fakeEl(),
      createElementNS: () => fakeEl(), addEventListener() {}, body: fakeEl(), hasFocus: () => os.focus === "tray",
    },
    __TAURI__: {
      core: {
        invoke: (cmd, args) => call(() => {
          if (cmd === "tray_window_plan") return { recEntries: [], sections: [], showLabel: "Show", quitLabel: "Quit", searchOn: true, searchPh: "" };
          if (cmd === "tray_open_launcher") {
            prefill = String(args.query || "").slice(0, 200);
            launcherListeners = {};
            setTimeout(bootLauncher, loadMs);
            if (MODEL === "old") {
              os.focus = "launcher";
              (trayWinH.blur || []).forEach((fn) => fn());
            }
            return null;
          }
          if (cmd === "tray_launcher_prefill") {
            prefill = String(args.query || "").slice(0, 200);
            if (launcherListeners["launcher-prefill"]) launcherListeners["launcher-prefill"]({ payload: prefill });
            return null;
          }
          if (cmd === "tray_window_close") { os.trayAlive = false; if (os.focus === "tray") os.focus = null; return null; }
          return null;
        }),
      },
      event: { listen: () => Promise.resolve(() => {}) },
    },
  };
  traySandbox.window = { addEventListener: (t, fn) => { (trayWinH[t] ||= []).push(fn); }, __TAURI__: traySandbox.__TAURI__ };
  const trayCode = read("tray.js")
    .replace(/^import \{ applyFloatTheme \} from "\.\/floattheme\.js";$/m, "const applyFloatTheme = () => {};")
    .replace(/^import \{ APP_NAME \} from "\.\/brand\.js";$/m, 'const APP_NAME = "App";');
  vm.createContext(traySandbox);
  vm.runInContext(trayCode, traySandbox, { filename: "tray.js" });
  await sleep(ipcMs * 4 + 20); // boot: plan, theme calls, search field armed

  // The typist: a key lands in the window that owns the focus; a launcher page that is
  // not loaded yet swallows it.
  const lost = [];
  for (const ch of text) {
    if (os.focus === "tray" && os.trayAlive) {
      const f = trayEls["tray-search"];
      f.value += ch;
      f.fire("input");
    } else if (os.focus === "launcher" && os.launcherLoaded) {
      const f = launcherEls.search;
      f.value += ch;
      f.fire("input");
    } else lost.push(ch);
    if (keyMs > 0) await sleep(keyMs); // 0 = one synchronous burst
  }
  await sleep(loadMs + ipcMs * 12 + 300);
  return { got: launcherEls.search ? launcherEls.search.value : null, lost, trayAlive: os.trayAlive };
}

async function main() {
  globalThis.document = globalThis.document || { addEventListener() {}, removeEventListener() {} };
  const problems = [];
  const text = "qwertzuiop1234";
  const cases = [
    { keyMs: 30, loadMs: 150, ipcMs: 8 },
    { keyMs: 50, loadMs: 150, ipcMs: 8 },
    { keyMs: 80, loadMs: 150, ipcMs: 8 },
    { keyMs: 200, loadMs: 150, ipcMs: 8 },
    { keyMs: 7, loadMs: 150, ipcMs: 8 },
    { keyMs: 30, loadMs: 400, ipcMs: 8 },
    { keyMs: 45, loadMs: 20, ipcMs: 25 },
  ];
  for (const c of cases) {
    const r = await run({ text, ...c });
    const label = `keys every ${c.keyMs} ms, launcher page ${c.loadMs} ms, ipc ${c.ipcMs} ms`;
    if (r.got !== text) problems.push(`${label}: launcher field ${JSON.stringify(r.got)}, want ${JSON.stringify(text)}${r.lost.length ? " (lost in flight: " + r.lost.join("") + ")" : ""}`);
    if (r.trayAlive) problems.push(`${label}: the tray window was never closed`);
  }
  // Over the field cap the text is cut at 200 characters, never longer.
  const long = "a".repeat(230);
  const r = await run({ text: long, keyMs: 0, loadMs: 300, ipcMs: 8 });
  if (r.got !== "a".repeat(200)) problems.push(`230 typed characters: launcher field holds ${r.got ? r.got.length : r.got} characters, want 200`);

  if (problems.length) {
    console.error(`tray_launcher_handoff_check (${MODEL} backend model): FAILED\n  ` + problems.join("\n  "));
    process.exit(1);
  }
  console.log(`tray_launcher_handoff_check (${MODEL} backend model): ok`);
  process.exit(0);
}
main().catch((e) => { console.error(e); process.exit(1); });
