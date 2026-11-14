// Main-window fixes, real extracted source in a node vm: shifted and AltGr keys recorded for a
// hotkey, the recovered crash draft's unsaved-change baseline, text dropped on the window, the
// image repaint that waits out a drag, the keyboard walk over drawn tiles only, and the fixed
// placeholder values list following its gate. UI_CHECK_DIR points at another copy of ui/.
// Run:  node tests/ui/uimain_fixes_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const uiDir = process.env.UI_CHECK_DIR || path.join(__dirname, "..", "..", "ui");
const read = (name) => fs.readFileSync(path.join(uiDir, name), "utf8").replace(/\r\n/g, "\n");
function extractBlock(src, marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find (text changed?): ${marker}`);
  const braceStart = src.indexOf("{", src.indexOf(")", start));
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces after: ${marker}`);
}
const problems = [];
const check = (actual, expected, what) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${what}: got ${a}, expected ${e}`);
};
const section = (fn) => { try { fn(); } catch (e) { problems.push(String(e.message || e)); } };
const run = (code, sandbox) => { vm.createContext(sandbox); vm.runInContext(code, sandbox); return sandbox; };

// 1. Hotkey recording: the key is named by the physical key, not by the shifted character.
section(() => {
  const src = read("hotkeys.js").replace(/^import .*$/gm, "").replace(/^export /gm, "");
  const ctx = run(src + "\nthis.api = { accelFromEvent };", { t: (k) => k });
  const ev = (o) => Object.assign({ key: "", code: "", keyCode: 0, ctrlKey: true, altKey: false, shiftKey: false, metaKey: false }, o);
  const A = ctx.api.accelFromEvent;
  check(A(ev({ shiftKey: true, key: "!", code: "Digit1", keyCode: 49 })), "CommandOrControl+Shift+1", "Ctrl+Shift+1");
  check(A(ev({ shiftKey: true, key: "/", code: "Digit7", keyCode: 55 })), "CommandOrControl+Shift+7", "German Ctrl+Shift+7");
  check(A(ev({ altKey: true, key: "@", code: "KeyQ", keyCode: 81 })), "CommandOrControl+Alt+Q", "AltGr+Q");
  check(A(ev({ key: "ü", code: "BracketLeft", keyCode: 186 })), "CommandOrControl+Semicolon", "umlaut key follows its virtual key");
  check(A(ev({ key: "?", code: "Slash", keyCode: 191, shiftKey: true })), "CommandOrControl+Shift+Slash", "shifted slash key");
  check(A(ev({ key: "ф", code: "KeyA", keyCode: 65 })), "CommandOrControl+A", "non-Latin letter key");
  check(A(ev({ key: ".", code: "Period", keyCode: 0 })), "CommandOrControl+Period", "punctuation by code when no virtual key");
  check(A(ev({ key: "p", code: "KeyP", keyCode: 80 })), "CommandOrControl+P", "a letter stays");
  check(A(ev({ key: "7" })), "CommandOrControl+7", "a plain digit event stays");
  check(A(ev({ key: "," })), "CommandOrControl+,", "an event without code keeps the typed key");
});

// 2. A recovered draft: the stored prompt is the baseline, so the recovered edits count as changed.
section(() => {
  const src = read("editor.js");
  const snapAt = src.indexOf("const SNAP_RESTORED");
  const snapLine = src.slice(snapAt, src.indexOf("\n", snapAt));
  const ctx = run(
    `let modalInitial = "";\n${snapLine}\n${extractBlock(src, "export function rebaseModalOnStored(").replace("export ", "")}\nthis.api = { rebase: rebaseModalOnStored, get: () => modalInitial, set: (v) => { modalInitial = v; } };`,
    {},
  );
  const snap = (name, text, entries) => JSON.stringify([name, text, "", "0", "", "", "", "", false, "", "", false, [], "", "", "", entries, "", false, 0]);
  ctx.api.set(snap("draft name", "draft text", ["draft text", "b"]));
  ctx.api.rebase({ name: "stored", text: "stored text", variants: [] });
  check(ctx.api.get(), snap("stored", "stored text", []), "baseline holds the stored name, text and entries");
  ctx.api.set(snap("d", "d", []));
  ctx.api.rebase({ name: "n", text: "t", variants: ["u"] });
  check(JSON.parse(ctx.api.get())[16], ["t", "u"], "stored entries become the baseline list");
  ctx.api.set("not json");
  ctx.api.rebase({ name: "n", text: "t", variants: [] });
  check(ctx.api.get(), "", "an unreadable baseline stays dirty");
  const main = read("main.js");
  check(/openModal\(\{ \.\.\.promptToModal\(p\), mode: "edit"[^\n]*\n[^\n]*\n\s*rebaseModalOnStored\(p\);/.test(main), true, "recovery calls rebaseModalOnStored after opening the edit");
});

// 3. Dropped text: the create dialog opens with it and the composer input is not touched.
section(() => {
  const src = read("editor.js");
  const opened = [];
  const inputEl = { value: "half typed" };
  const ctx = run(`${extractBlock(src, "export function startCreate(").replace("export ", "")}\nthis.api = { startCreate };`,
    { inputEl, t: (k) => k, openModal: (o) => opened.push(o) });
  ctx.api.startCreate(" dropped text ");
  check(opened.map((o) => o.text), ["dropped text"], "dropped text opens the dialog");
  check(inputEl.value, "half typed", "the composer keeps its text");
  ctx.api.startCreate();
  check(opened[1]?.text, "half typed", "no argument still reads the composer");
  const drop = extractBlock(src, "export function setupDragDrop(");
  check(/confirmDiscardOrProceed\(\(\) => startCreate\(text\)\)/.test(drop), true, "the drop handler passes the text");
  check(/inputEl\.value = text/.test(drop), false, "the drop handler no longer writes the composer");
});

// 4. Image repaint during a drag stays pending and runs after it.
section(() => {
  const src = read("tiles.js");
  const timers = [];
  let dragNow = { on: true };
  let replaced = 0;
  const waiting = { dataset: { imgWait: "p1" }, classList: { contains: (c) => c === "tile" }, replaceWith: () => { replaced++; } };
  const ctx = run(
    `let imagePaintQueued = false;\n${extractBlock(src, "export function scheduleImagePaint(").replace("export ", "")}\nthis.api = { scheduleImagePaint, queued: () => imagePaintQueued };`,
    {
      get drag() { return dragNow; },
      IMG_WAIT: "imgWait", prompts: [{ id: "p1" }], imageOf: () => "data:x", buildTile: () => ({}), fitAllTiles: () => {},
      document: { querySelectorAll: () => [waiting] },
      setTimeout: (fn, ms) => { timers.push({ fn, ms }); },
    },
  );
  ctx.api.scheduleImagePaint();
  timers.shift().fn();
  check(replaced, 0, "nothing is painted while a drag runs");
  check(timers.length, 1, "the paint is re-armed during a drag");
  check(ctx.api.queued(), true, "a queued paint is not dropped");
  dragNow = null;
  timers.shift().fn();
  check(replaced, 1, "the paint runs once the drag is over");
  check(ctx.api.queued(), false, "the queue is free again afterwards");
});

// 5. Keyboard walk: a placed prompt without a drawn tile (archived) is not a stop.
section(() => {
  const src = read("copyFlow.js");
  const tiles = [{ dataset: { id: "a" } }, { dataset: { id: "c" } }];
  const gridEl = { querySelectorAll: (sel) => (sel === ".tile" ? tiles : []) };
  const ctx = run(`${extractBlock(src, "export function kbNavMap(").replace("export ", "")}\nthis.api = { kbNavMap };`, {
    favView: false, favLayoutMap: () => ({}), layoutOf: () => ({ a: [0, 0], b: [1, 0], c: [2, 0] }), activeView: () => "v", gridEl, FOLDER_KEY: "folder:", Number,
  });
  check(Object.keys(ctx.api.kbNavMap()), ["a", "c"], "the hidden prompt is skipped");
  const fav = run(`${extractBlock(src, "export function kbNavMap(").replace("export ", "")}\nthis.api = { kbNavMap };`, {
    favView: true, favLayoutMap: () => ({ a: [0, 0], b: [1, 0] }), layoutOf: () => ({}), activeView: () => "v", gridEl, FOLDER_KEY: "folder:", Number,
  });
  check(Object.keys(fav.api.kbNavMap()), ["a"], "the favorites view skips it too");
});

// 6. The fixed values list follows its gate, not only its own flag.
section(() => {
  const src = read("expertMenu.js");
  check(/group\.varFixedList && gateOpen\("varFixed"\)/.test(src), true, "varFixedList uses gateOpen");
});

if (problems.length) {
  console.error(problems.join("\n"));
  process.exit(1);
}
console.log("uimain fixes: ok");
