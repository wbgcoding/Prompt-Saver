// Main-window fixes (ui/main.js, library.js, core.js, ...): each section runs the real
// extracted source of the function in a vm, or reads the one source line that carries the fix.
// UI_CHECK_DIR points the test at another copy of ui/ (to prove it fails on older code).
// Run:  node tests/ui/main_window_fixes_check.js      Exit: 0 = clean, 1 = problems.
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
const check = (ok, what) => { if (!ok) problems.push(what); };
const section = (fn) => { try { fn(); } catch (e) { problems.push(String(e.message || e)); } };
const run = (code, sandbox) => { vm.createContext(sandbox); vm.runInContext(code, sandbox); return sandbox; };

// 1. Escape and Tab dispatchers: every main-window overlay is in both cascades.
section(() => {
  const main = read("main.js");
  const tabAt = main.indexOf('if (e.key !== "Tab"');
  const escAt = main.indexOf('if (e.key !== "Escape")');
  const tabBlock = main.slice(tabAt, escAt);
  const escBlock = main.slice(escAt, main.indexOf('listen("theme-changed"'));
  for (const needle of ['$("global-search")', '$("theme-modal")', "bgOpsPop", "STACKED_DIALOGS", "closeSnipModal"]) {
    if (needle === "closeSnipModal") check(escBlock.includes(needle), `Escape dispatcher misses ${needle}`);
    else if (needle === "STACKED_DIALOGS") {
      check(tabBlock.includes(needle), "Tab dispatcher misses the stacked dialogs");
      check(escBlock.includes("closeStackedDialog"), "Escape dispatcher misses the stacked dialogs");
    } else {
      check(tabBlock.includes(needle), `Tab dispatcher misses ${needle}`);
      check(escBlock.includes(needle), `Escape dispatcher misses ${needle}`);
    }
  }
  // Every overlay the dispatchers list must close through its own button or function.
  const listMatch = /const STACKED_DIALOGS = (\[[^\]]*\]);/.exec(main);
  check(!!listMatch, "STACKED_DIALOGS list not found");
  if (listMatch) {
    const ids = JSON.parse(listMatch[1]);
    for (const id of ["ocr-langs-modal", "ocr-modal", "batch-export-modal", "video-cleanup-modal", "import-folder-modal", "import-preview-modal", "error-log-modal"]) {
      check(ids.includes(id), `${id} is not a stacked dialog`);
    }
    // closeStackedDialog closes the first visible one and reports it.
    const closed = [];
    const hiddenSet = new Set(ids);
    const closeMap = Object.fromEntries(ids.map((id) => [id, () => closed.push(id)]));
    const sandbox = run(
      `const STACKED_DIALOGS = ${listMatch[1]};\nconst STACKED_DIALOG_CLOSE = closeMap;\n${extractBlock(main, "function closeStackedDialog(")}`,
      { closeMap, $: (id) => ({ classList: { contains: () => hiddenSet.has(id) } }) },
    );
    check(sandbox.closeStackedDialog() === false && closed.length === 0, "closeStackedDialog with nothing open must do nothing");
    hiddenSet.delete("import-preview-modal");
    hiddenSet.delete("error-log-modal");
    check(sandbox.closeStackedDialog() === true && closed.join() === "import-preview-modal", `closeStackedDialog closes the topmost open one (got ${closed})`);
  }
});

// 2. Selection Escape listeners stand back for any other open overlay.
section(() => {
  const lib = read("library.js");
  const make = (open, libraryFront) => run(extractBlock(lib, "export function otherOverlayOpen(").replace("export ", ""), {
    document: {
      body: { classList: { contains: (c) => c === "library-front" && libraryFront } },
      querySelectorAll: () => open.map((id) => ({ id })),
    },
  });
  check(make([], false).otherOverlayOpen("") === false, "no overlay open: selection may take Escape");
  check(make(["library"], false).otherOverlayOpen("library") === false, "only the library itself is open");
  check(make(["library", "confirm-modal"], false).otherOverlayOpen("library") === true, "a confirm over the library keeps Escape");
  check(make(["modal"], false).otherOverlayOpen("") === true, "the editor open: the grid selection keeps its hands off Escape");
  check(make(["library", "backup-modal"], true).otherOverlayOpen("library") === false, "backups under a raised library do not count");
  check(make(["library", "backup-modal", "confirm-modal"], true).otherOverlayOpen("library") === true, "a confirm over a raised library still counts");
  check(/Escape" && libSelectMode[^\n]*otherOverlayOpen\("library"\)/.test(lib), "library select Escape listener must use otherOverlayOpen");
  check(/Escape" && gridSelected\.size[^\n]*otherOverlayOpen\(""\)/.test(lib), "grid select Escape listener must use otherOverlayOpen");
});

// 3. Add to layout never picks a folder's cell.
section(() => {
  const lib = read("library.js");
  const at = lib.indexOf("lib-add");
  const seg = lib.slice(at, at + 1500);
  check(/foldersOfView\(view\.id\)/.test(seg) && seg.indexOf("foldersOfView") < seg.indexOf("firstFree("), "add-to-layout must count folder cells as occupied before firstFree");
});

// 4. Media-meta backfill repaints once for a burst of answers and not at all while the panel is closed.
section(() => {
  const lib = read("library.js");
  const timers = new Map();
  let nextId = 1;
  let hidden = false;
  const sandbox = run(
    `const backfillRepaint = new Map();\n${extractBlock(lib, "function scheduleBackfillRepaint(")}`,
    {
      libraryEl: { classList: { contains: () => hidden } },
      setTimeout: (fn) => { const id = nextId++; timers.set(id, fn); return id; },
      clearTimeout: (id) => { timers.delete(id); },
    },
  );
  let paints = 0;
  const rerender = () => { paints++; };
  for (let i = 0; i < 20; i++) sandbox.scheduleBackfillRepaint(rerender);
  for (const [id, fn] of [...timers]) { timers.delete(id); fn(); }
  check(paints === 1, `20 backfill answers repaint once (got ${paints})`);
  hidden = true;
  sandbox.scheduleBackfillRepaint(rerender);
  for (const [id, fn] of [...timers]) { timers.delete(id); fn(); }
  check(paints === 1, "a closed library is not repainted by a backfill answer");
});

// 5. Keyboard activation in the grid: a focused control keeps Enter/Space, a cooldown shakes the tile.
section(() => {
  const cf = read("copyFlow.js");
  const keyFn = run(extractBlock(cf, "function controlOwnsActivateKey("), {}).controlOwnsActivateKey;
  const target = (matches) => ({ closest: (sel) => (sel.split(",").map((x) => x.trim()).some((x) => matches.includes(x)) ? {} : null) });
  check(keyFn(target(["button"])) === true, "Enter on a focused button must not copy the focused tile");
  check(keyFn(target(["select"])) === true, "Enter on a focused select must not copy the focused tile");
  check(keyFn(target(["[tabindex]"])) === true, "Enter on a tabindex element must not copy the focused tile");
  check(keyFn(target([])) === false, "Enter on the plain page still activates the focused tile");
  check(/&& kbFocus && !controlOwnsActivateKey\(e\.target\)/.test(cf), "the Enter/Space branch must ask controlOwnsActivateKey");
  check(/if \(!kbFocus \|\| !flag\("keyboardNav"\) \|\| !flag\("chainPrompts"\)/.test(cf), "Shift arms chain mode only while the focus ring is in use");

  let cooling = true;
  const timers = [];
  const sandbox = run(extractBlock(cf, "export function blockedByCooldown(").replace("export ", ""), {
    copyOnCooldown: () => cooling,
    setTimeout: (fn) => timers.push(fn),
  });
  const classes = new Set();
  const el = { classList: { add: (c) => classes.add(c), remove: (c) => classes.delete(c) } };
  check(sandbox.blockedByCooldown("a", el) === true && classes.has("cooldown-blocked"), "a blocked keyboard activation shakes the tile");
  timers.forEach((fn) => fn());
  check(!classes.has("cooldown-blocked"), "the shake class is removed again");
  check(sandbox.blockedByCooldown("a", null) === true, "a blocked copy without a tile element still reports blocked");
  cooling = false;
  check(sandbox.blockedByCooldown("a", el) === false && !classes.has("cooldown-blocked"), "no cooldown: nothing blocked, nothing shaken");
  check(/if \(blockedByCooldown\(id, el\)\) return;\n  if \(dialogTokens/.test(cf), "activateTile must use the shared cooldown shake");
});

if (problems.length) {
  console.error("FAIL");
  for (const p of problems) console.error("  - " + p);
  process.exit(1);
}
console.log("main window fixes: ok");
