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
// A launcher window that is built with the focus flag takes the operating system focus
// away from the tray even while it is still hidden (measured in a real window: the tray
// blurred about 60 ms after the first key, before the launcher page existed, and the key
// typed in that gap went nowhere). The model therefore reads the Rust sources: when the hand-off
// build in open_launcher_window still passes `.focused(true)`, the launcher takes the
// focus at build time; the same goes for the 200 character limit of the launcher field
// (maxlength in launcher.html), which only native typing is subject to.
// Run:  node tests/ui/tray_launcher_handoff_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "..", "ui");
const MODEL = process.env.HANDOFF_MODEL === "old" ? "old" : "new";
// Every Rust source of the crate except the test file, so the check follows the
// launcher code into whichever file holds it.
const rustDir = path.join(__dirname, "..", "..", "src-tauri", "src");
const rustSrc = fs
  .readdirSync(rustDir)
  .filter((n) => n.endsWith(".rs") && n !== "tests.rs")
  .sort()
  .map((n) => fs.readFileSync(path.join(rustDir, n), "utf8"))
  .join("\n");
const openAt = rustSrc.indexOf("fn open_launcher_window(");
const revealAt = openAt < 0 ? -1 : rustSrc.indexOf("fn reveal_launcher(", openAt);
if (openAt < 0 || revealAt < 0) throw new Error("open_launcher_window or reveal_launcher not found in the Rust sources");
const openFn = rustSrc.slice(openAt, revealAt);
// true: the hidden launcher is built focused and grabs the focus from the tray at once
const BUILD_STEALS_FOCUS = /\.focused\(true\)/.test(openFn);
const maxLenMatch = /id="search"[^>]*\smaxlength="(\d+)"/.exec(fs.readFileSync(path.join(uiDir, "launcher.html"), "utf8"));
const FIELD_MAX = maxLenMatch ? Number(maxLenMatch[1]) : Infinity;
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
            if (MODEL === "old" || BUILD_STEALS_FOCUS) {
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
      if (f.value.length < FIELD_MAX) f.value += ch;
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
  // Over the field cap the text is cut at 200 characters, never longer: as one burst,
  // and typed slowly enough that most of it arrives after the launcher took the focus.
  const long = Array.from({ length: 230 }, (_, i) => String.fromCharCode(97 + (i % 26))).join("");
  for (const keyMs of [0, 12]) {
    const r = await run({ text: long, keyMs, loadMs: 300, ipcMs: 8 });
    if (r.got !== long.slice(0, 200)) problems.push(`230 typed characters (keys every ${keyMs} ms): launcher field holds ${r.got ? r.got.length : r.got} characters${r.got && r.got.length === 200 ? " (wrong ones)" : ""}, want the first 200`);
  }

  if (problems.length) {
    console.error(`tray_launcher_handoff_check (${MODEL} backend model): FAILED\n  ` + problems.join("\n  "));
    process.exit(1);
  }
  console.log(`tray_launcher_handoff_check (${MODEL} backend model): ok`);
  process.exit(0);
}
main().catch((e) => { console.error(e); process.exit(1); });
