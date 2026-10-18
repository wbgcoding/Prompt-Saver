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
const pending = [];
const later = (fn) => { pending.push(fn().catch((e) => problems.push(String(e.message || e)))); };
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
  const make = (open, libraryFront, popOpen = false) => run(extractBlock(lib, "export function otherOverlayOpen(").replace("export ", ""), {
    isValuePopOpen: () => popOpen,
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
  check(make([], false, true).otherOverlayOpen("") === true, "an open value list keeps Escape for itself");
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

// 6. Launch-time steps: a failing consent write or a failing step does not stop the ones after it.
section(() => {
  const main = read("main.js");
  const toasts = [];
  const sbG = run(extractBlock(main, "async function runGuarded("), { errToast: (e) => toasts.push(String(e)) });
  const order = [];
  later(async () => {
    await sbG.runGuarded(async () => { throw new Error("boom"); });
    await sbG.runGuarded(async () => { order.push("second"); });
    check(order.join() === "second" && toasts.length === 1, "a failing launch step is reported and the next one still runs");
  });
  check(/runGuarded\(\(\) => maybeOpenFirstRunTour\(sealWarned\)\)/.test(main) && /runGuarded\(maybeOfferEditorDraftRecovery\)/.test(main), "the reveal chain guards the tour and the draft recovery");

  const settings = { update_consent: "unasked", auto_update: true };
  const errs = [];
  let invokeFails = true;
  const sbC = run(extractBlock(main, "async function maybeAskUpdateConsent("), {
    settings, flag: () => false, t: (k) => k, errToast: (e) => errs.push(String(e)),
    confirmDialog: async () => true,
    invoke: async (cmd) => { if (cmd === "set_update_consent" && invokeFails) throw new Error("locked"); },
  });
  later(async () => {
    await sbC.maybeAskUpdateConsent(false);
    check(errs.length === 1 && settings.update_consent === "unasked", "a refused consent write is reported and leaves the question open");
    invokeFails = false;
    await sbC.maybeAskUpdateConsent(false);
    check(settings.update_consent === "yes", "an accepted consent write is remembered");
  });
  check(/toLocaleString\(LANG\)/.test(main), "the crash-draft time is formatted in the UI language");
});

// 7. A toast shown right after hideToast() is not hidden by the stale 200 ms timer.
section(() => {
  const core = read("core.js");
  const timers = new Map();
  let nextId = 1;
  const classes = new Set(["show"]);
  const sandbox = run(`let toastHideTimer = null;\n${extractBlock(core, "export function hideToast(").replace("export ", "")}`, {
    toastEl: { classList: { remove: (c) => classes.delete(c), add: (c) => classes.add(c) } },
    setTimeout: (fn) => { const id = nextId++; timers.set(id, fn); return id; },
    clearTimeout: (id) => { timers.delete(id); },
  });
  sandbox.hideToast();
  sandbox.hideToast();
  check(timers.size === 1, `hideToast keeps a single pending hide timer (got ${timers.size})`);
  const toastHead = core.slice(core.indexOf("export function toast("), core.indexOf("export function toast(") + 200);
  check(/clearTimeout\(toastHideTimer\)/.test(toastHead), "toast() must cancel the pending hide timer first");
});

// 8. Tile drag cleanup and the move undo.
section(() => {
  const gf = read("gridFolders.js");
  const removed = [];
  const layer = (name) => ({ classList: { remove: (c) => removed.push(name + ":" + c), add: () => {} } });
  const sandbox = run(extractBlock(gf, "export function endMemberDragLayers(").replace("export ", ""), {
    tileFolderBackdrop: layer("backdrop"), tileFolderPop: layer("pop"),
  });
  sandbox.endMemberDragLayers();
  check(removed.join() === "backdrop:member-dragging,pop:member-dragging", "endMemberDragLayers frees both folder layers");
  const closeFn = extractBlock(gf, "export function closeTileFolderPop(");
  check(/endMemberDragLayers\(\)/.test(closeFn), "closing a folder popover frees the drag layers");
  const tiles = read("tiles.js");
  const cancelAt = tiles.indexOf('addEventListener("pointercancel"');
  check(/endMemberDragLayers\(\)/.test(tiles.slice(cancelAt, cancelAt + 700)), "pointercancel frees the folder layers");
  check(/undoToast\(t\("tileMoved"\)/.test(tiles), "the tile move offers its undo through undoToast");
  check(/toast\(t\("undoNotHere"\)\)/.test(tiles), "an undo after a view switch says so");
  check(/lock\.innerHTML = LOCK_SVG;/.test(tiles), "the tile lock badge reuses LOCK_SVG");
});

// 9. Views: one save per dialog confirm, a refused grid write restores the size boxes,
// a cancelled tab drag lets go, the test hooks stay out of the shipped window.
section(() => {
  const views = read("views.js");
  let release;
  const calls = [];
  const sb = run(`let viewModalSaving = false;\n${extractBlock(views, "export async function confirmViewModal(").replace("export ", "")}`, {
    viewModal: { name: { value: "Work", focus: () => {} } },
    viewModalId: null, viewModalColor: "",
    settings: { views: [] }, clampGrid: (v) => v,
    invoke: (cmd) => { calls.push(cmd); return new Promise((r) => { release = () => r({}); }); },
    assignSettings: () => {}, errToast: () => {}, closeViewModal: () => {}, renderViews: () => {}, renderGrid: async () => {},
  });
  later(async () => {
    const a = sb.confirmViewModal();
    const b = sb.confirmViewModal();
    release();
    await Promise.all([a, b]);
    check(calls.filter((c) => c === "add_view").length === 1, `two fast confirms create one view (got ${calls.length} calls)`);
    const c = sb.confirmViewModal();
    release();
    await c;
    check(calls.length === 2, "a later confirm works again after the first finished");
  });

  const inputs = { "qg-cols": { value: "9" }, "qg-rows": { value: "9" } };
  const toasts = [];
  let rendered = 0;
  const sb2 = run(extractBlock(views, "async function applyViewGrid("), {
    $: (id) => inputs[id], activeView: () => ({ cols: 3, rows: 2 }),
    invoke: async () => { throw new Error("locked"); }, assignSettings: () => {},
    errToast: (e) => toasts.push(String(e)), renderGrid: async () => { rendered++; },
  });
  later(async () => {
    await sb2.applyViewGrid({ id: "v", cols: 3, rows: 2 }, 9, 9);
    check(toasts.length === 1 && rendered === 0, "a refused grid write is reported and nothing is redrawn");
    check(inputs["qg-cols"].value === 3 && inputs["qg-rows"].value === 2, "the size boxes show the real grid again");
  });

  const wire = extractBlock(views, "export function wireViews(");
  check(/addEventListener\("pointercancel"[\s\S]*tabDrag = null/.test(wire), "a cancelled tab press ends the drag");
  check(/if \(window\.__SMOKE_ERRORS__\) \{\s*window\.__runViewMutation/.test(wire), "the view-mutation test hooks exist only under the test stub");
  check(/favViewButton: !on/.test(views), "a refused favorites-view switch is rolled back");
});

// 10. A cancelled fill-in dialog does not use up a counter value.
section(() => {
  const pv = read("promptVars.js");
  let counter = 0;
  let answer = null;
  const sb = run(extractBlock(pv, "export async function resolveVarsList(").replace("export ", ""), {
    CLIP_VAR_MAX: 10, toast: () => {}, t: (k) => k,
    extractVars: (text) => text.split(" "),
    parseVar: (tok) => ({ name: tok }),
    isDynVar: (name) => name === "@counter",
    dynName: (raw) => (raw.startsWith("@") ? raw.slice(1) : ""),
    dynValue: (name, now, ctx) => (name === "counter" ? String(ctx.counter) : name),
    fillVars: (text, values) => text.split(" ").map((tok) => values[tok] ?? tok).join(" "),
    nextVarCounter: () => ++counter,
    promptVarsWindow: async () => answer,
    invoke: async () => "",
  });
  later(async () => {
    answer = null; // the dialog is cancelled
    const cancelled = await sb.resolveVarsList(["@counter name"]);
    check(cancelled === null && counter === 0, `a cancelled dialog leaves the counter alone (counter ${counter})`);
    answer = { name: "Ann" };
    const done = await sb.resolveVarsList(["@counter name"]);
    check(counter === 1 && done[0] === "1 Ann", `a finished dialog uses one counter value (got ${JSON.stringify(done)}, counter ${counter})`);
    const only = await sb.resolveVarsList(["@counter"]);
    check(counter === 2 && only[0] === "2", "a text with only the counter still gets the next value");
  });
});

// 11. The live theme preview tells the other windows once per frame.
section(() => {
  const th = read("theme.js");
  const frames = new Map();
  let nextFrame = 1;
  const sent = [];
  const code = [
    "let themeBroadcastFrame = 0;\nlet themeBroadcastPending = null;",
    extractBlock(th, "function broadcastThemeSoon("),
    extractBlock(th, "function cancelThemeBroadcast("),
  ].join("\n");
  const sb = run(code, {
    requestAnimationFrame: (fn) => { const id = nextFrame++; frames.set(id, fn); return id; },
    cancelAnimationFrame: (id) => { frames.delete(id); },
    broadcastTheme: (theme, vars) => sent.push({ theme, vars }),
  });
  const vars = { "--bg": "#000" };
  for (let i = 0; i < 30; i++) { vars["--bg"] = "#" + i; sb.broadcastThemeSoon("dark", vars); }
  check(sent.length === 0 && frames.size === 1, "30 drag steps queue a single frame");
  for (const [id, fn] of [...frames]) { frames.delete(id); fn(); }
  check(sent.length === 1 && sent[0].vars["--bg"] === "#29", `one broadcast with the latest colours (got ${JSON.stringify(sent)})`);
  sb.broadcastThemeSoon("dark", vars);
  sb.cancelThemeBroadcast();
  check(frames.size === 0, "a revert cancels the queued preview frame");
  const preview = extractBlock(th, "export function previewThemeDraft(");
  check(/broadcastThemeSoon\(/.test(preview) && !/\bbroadcastTheme\(/.test(preview), "previewThemeDraft uses the throttled broadcast");
});

// 12. A refused write puts the previous value back instead of leaving UI and backend apart.
section(() => {
  const em = read("expertMenu.js");
  const settings = { ui_values: { gridMax: 8 } };
  let rendered = 0;
  let fail = true;
  const toasts = [];
  const code = [
    "const offAtZeroWas = {};\nconst valueBeforeDrag = {};\nlet liveRenderQueued = false;\nlet expertOnlyChanged = false;",
    extractBlock(em, "export function previewValue(").replace("export ", ""),
    extractBlock(em, "export async function commitValue(").replace("export ", ""),
  ].join("\n");
  const sb = run(code, {
    EXPERT_VALUES: {}, settings, val: (k) => settings.ui_values[k] || 0,
    applyValues: () => {}, refreshTiles: async () => {}, renderGrid: async () => {}, renderExpert: () => { rendered++; }, syncRowReset: () => {},
    requestAnimationFrame: () => 0, errToast: (e) => toasts.push(String(e)),
    invoke: async () => { if (fail) throw new Error("locked"); },
  });
  later(async () => {
    sb.previewValue("gridMax", 12); // dragging
    sb.previewValue("gridMax", 14);
    await sb.commitValue("gridMax", 14);
    check(settings.ui_values.gridMax === 8, `a refused slider write restores the value the drag started from (got ${settings.ui_values.gridMax})`);
    check(toasts.length === 1 && rendered === 1, "the failure is reported and the control redrawn");
    await sb.commitValue("newKey", 3);
    check(!("newKey" in settings.ui_values), "a refused write of a key that was unset removes it again");
    fail = false;
    sb.previewValue("gridMax", 10);
    await sb.commitValue("gridMax", 10);
    check(settings.ui_values.gridMax === 10, "an accepted write keeps the new value");
  });
  const sp = read("settingsPanel.js");
  check(/settings\.theme = prevTheme;/.test(sp), "a refused theme write restores settings.theme");
  check(/set_language[\s\S]{0,200}setLangSelect\(\);/.test(sp), "a refused language write restores the select");
  check(/\[key\]: !next/.test(read("gridMenus.js")), "a refused toolbar switch is flipped back");
});

// 13. Background job labels have their own strings; empty values use one shared mark.
section(() => {
  const bg = read("bgOps.js");
  const sb = run(extractBlock(bg, "export function bgOpLabel(").replace("export ", ""), { t: (k) => k });
  const labels = ["export", "convert", "ocr-index", "ffmpeg-download"].map((k) => sb.bgOpLabel(k));
  check(new Set(labels).size === 4 && labels.every((l) => /^bgOpLabel/.test(l)), `each job kind has its own label key (got ${labels})`);
  const bs = String.fromCharCode(92);
  for (const file of ["bgOps.js", "backups.js", "dictation.js", "management.js"]) {
    check(!read(file).includes(bs + "u2014"), `${file} writes no em dash escape`);
  }
  check(read("core.js").includes('export const EMPTY_VALUE = "' + bs + 'u2013";'), "core.js defines EMPTY_VALUE as a lone en dash");
  check(read("backups.js").includes('armButton(del, t("deleteConfirmArmed"))'), "the armed delete label is a translated string");
});

// 14. Destructive one-click clears now ask first.
section(() => {
  const mg = read("management.js");
  let answer = false;
  const calls = [];
  const sb = run(`let errorLogList = ["a"];\n${extractBlock(mg, "export async function clearErrorLogAll(").replace("export ", "")}`, {
    t: (k) => k, confirmDialog: async () => answer, invoke: async (c) => { calls.push(c); },
    errToast: () => {}, renderErrorLogList: () => {},
  });
  later(async () => {
    await sb.clearErrorLogAll();
    check(calls.length === 0, "declining the confirm keeps the error log");
    answer = true;
    await sb.clearErrorLogAll();
    check(calls.join() === "clear_error_log", "confirming clears the error log");
  });
  const em = read("recordingTools.js");
  const at = em.indexOf('invoke("delete_discarded_recording_now"');
  check(at > 0 && em.slice(at - 400, at).includes("videoDiscardedDeleteAsk"), "deleting a discarded recording asks first");
});

// 15. The chain overview keeps the placeholder syntax of the copy path: same pattern, same reserved names.
section(() => {
  const literal = (src, name) => {
    const m = new RegExp("const " + name + " = (/.*/[a-z]*);").exec(src);
    if (!m) throw new Error("pattern not found: " + name);
    return m[1];
  };
  const list = (src, name) => {
    const m = new RegExp("const " + name + " = \\[([^\\]]*)\\]").exec(src);
    if (!m) throw new Error("list not found: " + name);
    return m[1].replace(/\s+/g, " ").trim();
  };
  const chain = read("chainvars.js");
  check(literal(chain, "CHAIN_VAR_RE") === literal(read("floatShared.js"), "VAR_RE"), "CHAIN_VAR_RE drifted from VAR_RE in floatShared.js");
  check(list(chain, "CHAIN_DYN_VARS") === list(read("promptVars.js"), "DYN_VARS"), "CHAIN_DYN_VARS drifted from DYN_VARS in promptVars.js");
});

// 16. Grid tile and folder member tile derive their media and type badge in one place.
section(() => {
  const tiles = read("tiles.js");
  const code = [extractBlock(tiles, "export function tileMedia(").replace("export ", ""), extractBlock(tiles, "export function tileTypeIcon(").replace("export ", "")].join("\n");
  const sb = run(code, {
    mediaKind: (path) => (/\.mp4$/.test(path) ? "video" : /\.png$/.test(path) ? "image" : /\.gif$/.test(path) ? "gif" : ""),
    hasImage: (p) => !!p.has_image, imageOf: (p) => (p.has_image ? "data:x" : ""),
    ICON_VIDEO: "V", ICON_IMAGE: "I", ICON_PDF: "P", ICON_FILE: "F", PDF_EXT: /\.pdf$/i,
  });
  const m = sb.tileMedia({ file_path: "a.mp4", icon_path: "" });
  check(m.kind === "video" && m.showKind === "video" && m.showPath === "a.mp4" && !m.has, "a video file shows as a video");
  const m2 = sb.tileMedia({ file_path: "a.txt", icon_path: "b.gif" });
  check(m2.showKind === "gif" && m2.showPath === "b.gif", "an icon path wins for display when there is no stored image");
  const m3 = sb.tileMedia({ has_image: true, file_path: "a.png" });
  check(m3.has && m3.stored === "data:x" && m3.showKind === "", "a stored image decides the layout");
  check(sb.tileTypeIcon({ file_path: "a.mp4" }, "video") === "V" && sb.tileTypeIcon({ file_path: "a.pdf" }, "") === "P"
    && sb.tileTypeIcon({ file_path: "a.zip" }, "") === "F" && sb.tileTypeIcon({ copy_image: true }, "") === "I" && sb.tileTypeIcon({}, "") === "", "type badge icon by what the button copies");
  const gf = read("gridFolders.js");
  check(/tileMedia\(p\)/.test(gf) && /tileTypeIcon\(p, kind\)/.test(gf), "the folder member tile uses the shared helpers");
  check(/data-folder-id="\$\{CSS\.escape\(f\.id\)\}"/.test(gf), "the folder tile lookup escapes the id");
});

// 17. A text dropped into a field stays with the field.
section(() => {
  const ed = read("editor.js");
  const sb = run(extractBlock(ed, "function dropTargetsField("), {});
  const target = (matches) => ({ target: { closest: (sel) => (sel.split(",").some((x) => matches.includes(x.trim())) ? {} : null) } });
  check(sb.dropTargetsField(target(["textarea"])) === true, "a drop on a textarea is left to the field");
  check(sb.dropTargetsField(target(["input"])) === true, "a drop on an input is left to the field");
  check(sb.dropTargetsField(target(["[contenteditable]"])) === true, "a drop on an editable element is left to the field");
  check(sb.dropTargetsField(target([])) === false, "a drop on the page still creates a prompt");
  check(/flag\("dragDrop"\) \|\| dropTargetsField\(e\)/.test(ed), "the window drop handler asks dropTargetsField");
});

// 18. U7: a close request is ignored while a save is in flight.
section(() => {
  const ed = read("editor.js");
  const mk = (saving) => run(
    `let modalSaving = ${saving};\nvar confirmAsked = 0;\nfunction flag() { return true; }\nconst modalState = { mode: "create" };\nfunction modalSnapshot() { return "x"; }\nlet modalInitial = "";\nconst confirmDialog = async () => { confirmAsked++; return true; };\nfunction t(k) { return k; }\n${extractBlock(ed, "export async function confirmDiscardIfDirty(").replace("export ", "")}`,
    {},
  );
  const sb = mk(true);
  later(async () => {
    check((await sb.confirmDiscardIfDirty()) === false && sb.confirmAsked === 0, "no discard prompt and no close while a save is in flight");
    const idle = mk(false);
    check((await idle.confirmDiscardIfDirty()) === true, "an idle editor still asks and closes");
  });
  check(/export function closeModal\(\) \{\n  \/\/[^\n]*\n  if \(modalSaving\) return;/.test(ed), "closeModal waits for the save in flight");
});

// 19. U25: a failed safety backup before a wipe asks before the wipe goes on.
section(() => {
  const sp = read("settingsPanel.js");
  const src = extractBlock(sp, "export async function maybeBackupBeforeWipe(").replace("export ", "");
  const make = (backupFails, answer) => {
    const asked = [];
    const sb = run(src, {
      flag: () => true, val: () => 3, t: (k) => k, errText: (e) => String(e),
      invoke: async () => { if (backupFails) throw "disk-full"; },
      confirmDialog: async (o) => { asked.push(o); return answer; },
    });
    return { sb, asked };
  };
  later(async () => {
    const ok = make(false, false);
    check((await ok.sb.maybeBackupBeforeWipe("delete")) === true && ok.asked.length === 0, "a made backup lets the wipe go on without a question");
    const no = make(true, false);
    check((await no.sb.maybeBackupBeforeWipe("delete")) === false && no.asked.length === 1, "a failed backup, Cancel: the wipe stops");
    check(no.asked[0] && no.asked[0].focusCancel === true && no.asked[0].message === "backupFailedDelete", "the question defaults to Cancel and names the delete");
    const yes = make(true, true);
    check((await yes.sb.maybeBackupBeforeWipe("reset")) === true, "a failed backup, confirmed: the reset goes on");
  });
  check(/if \(!\(await maybeBackupBeforeWipe\("delete"\)\)\) return;/.test(sp) && /if \(!\(await maybeBackupBeforeWipe\("reset"\)\)\) return;/.test(sp), "both wipes honour the answer");
});

// 20. U57: the Escape dispatcher bows out for the tour like the Tab trap does.
section(() => {
  const main = read("main.js");
  const escAt = main.indexOf('if (e.key !== "Escape")');
  const escBlock = main.slice(escAt, main.indexOf('listen("theme-changed"'));
  const guardAt = escBlock.indexOf('$("tour-modal").classList.contains("hidden") && $("confirm-modal").classList.contains("hidden")');
  check(guardAt > 0 && guardAt < escBlock.indexOf("closeShortcutsModal"), "Escape returns early while the tour is topmost, before any overlay closes");
});

// 21. U1: the thumb cache prune builds the live id set once, not a scan per cached id.
section(() => {
  const src = extractBlock(read("core.js"), "export function pruneThumbCache(").replace("export ", "");
  let reads = 0;
  const list = [{ id: "a" }, { id: "b" }];
  const prompts = new Proxy(list, { get(t, k) { if (k === "some") reads += 100; return t[k]; } });
  const sb = run(src, { prompts });
  const cache = new Map([["a", 1], ["b", 2], ["gone", 3]]);
  sb.pruneThumbCache(cache);
  check([...cache.keys()].join() === "a,b", "prune drops only ids no prompt has");
  check(reads === 0, "prune does not scan the prompts once per cached id");
});

// 22. U5: the grid batch bar builds its palette once and keeps a picked view while the selection grows.
section(() => {
  const lib = read("library.js");
  const bc = extractBlock(lib, "export function buildBatchColors(").replace("export ", "");
  let built = 0;
  const wrap = { childElementCount: 0 };
  const sb = run(bc, { $: () => wrap, doGridBatch() {}, doBatch() {}, openColorPop() {}, buildSwatches: () => { built++; wrap.childElementCount = 5; } });
  sb.buildBatchColors("grid-batch-colors");
  sb.buildBatchColors("grid-batch-colors");
  check(built === 1, "the batch palette is built once, not on every checkbox click");
  const upd = extractBlock(lib, "export function updateGridBatchBar(").replace("export ", "");
  const make = (hidden) => {
    let filled = 0;
    const bar = { classList: { contains: () => hidden, toggle() {} } };
    const el = { classList: { toggle() {} }, textContent: "" };
    const s = run(upd, { $: (id) => (id === "grid-batch-bar" ? bar : el), gridSelected: new Set(["a"]), t: () => "", LANG: "en", tagsOn: () => true, buildBatchColors() {}, fillBatchViews: () => { filled++; } });
    s.updateGridBatchBar();
    return filled;
  };
  check(make(true) === 1, "the view list is filled when a selection starts");
  check(make(false) === 0, "the view list is left alone while the bar is already showing");
});

// 23. U8: closing the editor drops the draft only after a tick's write has landed.
section(() => {
  const ed = read("editor.js");
  const src = ["let draftWrite = null;", extractBlock(ed, "export async function saveDraftNow(").replace("export ", ""), extractBlock(ed, "export async function discardEditorDraft(").replace("export ", "")].join("\n");
  later(async () => {
    const log = [];
    let release;
    const gate = new Promise((r) => { release = r; });
    const sb = run(src, {
      draftEditorFields: () => ({ promptId: "x", name: "n", text: "t", variants: "[]" }),
      draftLastSaved: null,
      invoke: async (cmd) => { if (cmd === "save_editor_draft") { await gate; log.push("saved"); } else log.push("discarded"); },
    });
    const tick = sb.saveDraftNow();
    const drop = sb.discardEditorDraft();
    await new Promise((r) => setTimeout(r, 5));
    check(log.length === 0, "the discard waits while the draft write is in flight");
    release();
    await Promise.all([tick, drop]);
    check(log.join() === "saved,discarded", "the discard lands after the write");
    const idle = [];
    const sb2 = run(src, { draftEditorFields: () => null, invoke: async (c) => { idle.push(c); } });
    await sb2.discardEditorDraft();
    check(idle.join() === "discard_editor_draft", "an idle editor discards at once");
  });
  check(!/invoke\("discard_editor_draft"\)\.catch/.test(ed), "no unordered discard call is left in the editor");
});

// 24. U9: typing keeps the chip row; only a flag change or an explicit refresh rebuilds it.
section(() => {
  const ed = read("editor.js");
  const src = extractBlock(ed, "export function renderVarHelper(").replace("export ", "");
  const make = (dynOn) => {
    const chips = { childElementCount: 0, classList: { toggle() {} }, appendChild() { this.childElementCount++; }, set innerHTML(_) { this.childElementCount = 0; } };
    const other = { classList: { toggle() {} } };
    const doc = { createElement: () => ({ addEventListener() {}, set className(_) {} }) };
    const sb = run(src, { $: (id) => (id === "modal-vars-chips" ? chips : other), flag: (k) => (k === "dynVars" ? dynOn : false), document: doc, DYN_VARS: ["a", "b", "c"], dynToken: (n) => n, t: (k) => k, DYN_TIPS: { a: "", b: "", c: "" }, insertVarToken() {} });
    return { sb, chips };
  };
  const on = make(true);
  on.sb.renderVarHelper();
  const first = on.chips.childElementCount;
  let rebuilt = 0;
  const orig = Object.getOwnPropertyDescriptor(on.chips, "innerHTML");
  Object.defineProperty(on.chips, "innerHTML", { set(v) { rebuilt++; orig.set.call(on.chips, v); }, configurable: true });
  on.sb.renderVarHelper(false);
  check(first === 3 && on.chips.childElementCount === 3 && rebuilt === 0, "typing does not rebuild matching chips");
  on.sb.renderVarHelper();
  check(rebuilt === 1 && on.chips.childElementCount === 3, "a plain render still rebuilds the chips");
  const fresh = make(true);
  fresh.sb.renderVarHelper(false);
  check(fresh.chips.childElementCount === 3, "typing still builds chips that are missing");
  check(/syncVarsUi\(modal\.root\.classList\.contains\("tall"\), rotationUiOn\(\), true\), 60\)/.test(ed), "the input handler settles for 60 ms");
});

// 25. Expert menu: a refused write puts the previous value back instead of keeping the optimistic one.
section(() => {
  const src = read("expertMenu.js");
  const persist = extractBlock(src, "export async function persistUiText(").replace("export ", "");
  const reset = extractBlock(src, "export async function resetOneExpertValue(").replace("export ", "");
  const make = (fail, settings) => {
    const toasts = [];
    const sb = run([persist, reset].join("\n"), {
      settings, invoke: async () => { if (fail) throw new Error("denied"); }, errToast: (e) => toasts.push(String(e.message || e)),
      EXPERT_BULK_NEVER: [], shipsOn: () => true, applyFlags() {}, applyValues() {}, fitAllTiles() {}, refreshTiles: async () => {},
      renderGrid: async () => {}, renderExpert() {}, TILE_DOM_FLAGS: new Set(), EXPERT_VALUES: { gap: { def: 9 } }, EXPERT_SELECTS: {}, EXPERT_DROPDOWNS: {},
    });
    return { sb, toasts };
  };
  later(async () => {
    const kept = { ui_texts: { ocrLang: "eng" } };
    const a = make(true, kept);
    const ok = await a.sb.persistUiText("ocrLang", "deu");
    check(ok === false && kept.ui_texts.ocrLang === "eng" && a.toasts.length === 1, "a refused text write keeps the old value");
    const fresh = { ui_texts: {} };
    const b = make(true, fresh);
    await b.sb.persistUiText("voiceLang", "de");
    check(!("voiceLang" in fresh.ui_texts), "a refused first write leaves no key behind");
    const good = { ui_texts: {} };
    const c = make(false, good);
    check((await c.sb.persistUiText("voiceLang", "de")) === true && good.ui_texts.voiceLang === "de", "an accepted write stays");
    const flags = { ui_flags: { dynVars: false }, ui_values: { gap: 3 } };
    const d = make(true, flags);
    await d.sb.resetOneExpertValue("flags", "dynVars");
    await d.sb.resetOneExpertValue("values", "gap");
    check(flags.ui_flags.dynVars === false && flags.ui_values.gap === 3, "a refused reset keeps the flag and the value");
  });
});

// 26. Expert menu: the screenshot format toggles the quality row; a refused select write rolls back.
section(() => {
  const src = read("expertMenu.js");
  const dd = extractBlock(src, "export function dropdownRow(");
  check(/key === "screenshotFormat"[\s\S]*snipSaveQuality[\s\S]*classList\.toggle\("hidden"/.test(dd), "the format dropdown toggles the quality row");
  const sel = extractBlock(src, "export function selectRow(").replace("export ", "");
  const el = () => {
    const cl = new Set();
    return { value: "", dataset: {}, children: [], classList: { toggle: (c, on) => (on ? cl.add(c) : cl.delete(c)), remove: (c) => cl.delete(c), add: (c) => cl.add(c), has: (c) => cl.has(c) }, appendChild(c) { this.children.push(c); }, append() {}, setAttribute() {}, addEventListener(ev, fn) { (this.h = this.h || {})[ev] = fn; }, focus() {} };
  };
  const made = [];
  const settings = { ui_values: { retentionDays: 7 } };
  const sb = run(sel, {
    settings, EXPERT_SELECTS: { retentionDays: { label: "x", options: [1, 7, 30], def: 7, unit: "d" } },
    document: { createElement: () => { const e = el(); made.push(e); return e; } }, t: (k) => k, withTip() {}, rowResetBtn: () => null,
    formatValue: (c, o) => String(o), TIME_UNITS: { d: "day" }, fmtUnitName: () => "d", LANG: "en", attachSelectPicker() {}, SELECT_CUSTOM_MIN: 1, SELECT_CUSTOM_MAX: 999,
    invoke: async () => { throw new Error("denied"); }, errToast() {}, syncRowReset() {}, expertOnlyChanged: false, renderExpert() {},
  });
  sb.selectRow("retentionDays");
  later(async () => {
    const select = made.find((e) => e.h && e.h.change);
    select.value = "30";
    select.h.change();
    await new Promise((r) => setTimeout(r, 20));
    check(settings.ui_values.retentionDays === 7 && select.value === "7", "a refused select write restores the value and the control");
  });
});

// 27. Expert menu: a capture folder the backend refuses is shown as the old one again.
section(() => {
  const src = read("expertMenu.js");
  const fn = extractBlock(src, "export async function setDirText(").replace("export ", "");
  const make = (fail, settings) => {
    let renders = 0;
    const sb = run(fn, { settings, invoke: async () => { if (fail) throw new Error("denied"); }, errToast() {}, renderExpert() { renders++; } });
    return { sb, renders: () => renders };
  };
  later(async () => {
    const st = { ui_texts: { screenshotDir: "C:/old" } };
    const a = make(true, st);
    await a.sb.setDirText("screenshotDir", "set_screenshot_dir", "C:/new");
    check(st.ui_texts.screenshotDir === "C:/old" && a.renders() === 1, "a refused folder keeps the old one");
    const st2 = { ui_texts: {} };
    await make(true, st2).sb.setDirText("recordingDir", "set_recording_dir", "C:/new");
    check(!("recordingDir" in st2.ui_texts), "a refused first folder leaves no key");
    const st3 = { ui_texts: {} };
    await make(false, st3).sb.setDirText("recordingDir", "set_recording_dir", "C:/new");
    check(st3.ui_texts.recordingDir === "C:/new", "an accepted folder stays");
  });
});

// 28. Expert menu palette: themed swatch buttons, never a native colour input.
section(() => {
  const src = read("expertMenu.js");
  const row = extractBlock(src, "export function paletteRow(");
  check(!/type\s*=\s*"color"/.test(row) && /openColorPop\(/.test(row) && /aria-label/.test(row), "the palette swatches use the themed picker with a label");
});

// 29. A refused palette write skips the colour remap and restores the stored palette.
section(() => {
  const src = read("expertMenu.js");
  const fn = extractBlock(src, "export async function savePalette(").replace("export ", "");
  later(async () => {
    const calls = [];
    const settings = { ui_texts: { palette: "[\"#111111\"]" } };
    const sb = run(fn, {
      settings, COLORS: ["", "#111111"], PALETTE_LEN: 1, loadPalette() {}, renderExpert() {}, refreshColorUI() {}, renderViews() {}, renderGrid: async () => {},
      assignPrompts() {}, assignSettings() {}, errToast() {},
      invoke: async (cmd) => { calls.push(cmd); if (cmd === "set_ui_text") throw new Error("denied"); return {}; },
    });
    await sb.savePalette(["#222222"]);
    check(!calls.includes("remap_colors") && settings.ui_texts.palette === "[\"#111111\"]", "a refused palette write neither remaps colours nor keeps the new palette");
  });
});

// 30. Expert group titles: a real button with aria-expanded that follows the fold state.
section(() => {
  const src = read("expertMenu.js");
  check(/createElement\(globalView \? "span" : "button"\)/.test(src) && /setAttribute\("aria-expanded", String\(!isCollapsed\)\)/.test(src), "the collapsible group title is a button with aria-expanded");
  const fn = extractBlock(src, "export function toggleExpertGroup(").replace("export ", "");
  const attrs = {};
  const set = new Set();
  const sec = { classList: { toggle() {} }, querySelector: () => ({ setAttribute: (k, v) => { attrs[k] = v; } }) };
  const sb = run(fn, { expertCollapsedSet: () => set, settings: {}, invoke: async () => {}, renderExpert() {} });
  sb.toggleExpertGroup("g", sec);
  check(attrs["aria-expanded"] === "false", "folding sets aria-expanded to false");
  sb.toggleExpertGroup("g", sec);
  check(attrs["aria-expanded"] === "true", "opening sets aria-expanded to true");
});

// 31. A refused expert reset restores the three setting maps and shows no success toast.
section(() => {
  const src = read("expertMenu.js");
  const fn = extractBlock(src, "export async function onResetExpert(").replace("export ", "");
  later(async () => {
    const settings = { ui_flags: { a: true }, ui_values: { b: 1 }, ui_texts: { c: "x" } };
    const toasts = [];
    const sb = run(fn, {
      settings, EXPERT_RESET_KEEP: [], flag: () => false, invoke: async () => { throw new Error("denied"); }, errToast() {}, loadPalette() {}, refreshColorUI() {}, applyFlags() {},
      renderExpert() {}, syncSettingsControls() {}, refreshFavViewUi() {}, renderViews() {}, renderGrid: async () => {}, toast: (m) => toasts.push(m), t: (k) => k,
    });
    await sb.onResetExpert();
    check(settings.ui_flags.a === true && settings.ui_values.b === 1 && settings.ui_texts.c === "x" && !toasts.length, "a refused reset keeps the stored settings and says nothing is done");
  });
});

// 32. Every dialog overlay is named: aria-labelledby points at an existing id, or an aria label key is set.
section(() => {
  const html = read("index.html");
  const ids = new Set([...html.matchAll(/ id="([^"]+)"/g)].map((m) => m[1]));
  const dialogs = [...html.matchAll(/<div id="([^"]+)"[^>]*role="dialog"[^>]*>/g)];
  const bad = dialogs.filter((m) => {
    const ref = /aria-labelledby="([^"]+)"/.exec(m[0]);
    return ref ? !ids.has(ref[1]) : !/data-i18n-aria=|aria-label=/.test(m[0]);
  }).map((m) => m[1]);
  check(dialogs.length >= 28 && !bad.length, "every dialog has an accessible name: " + bad.join(","));
});

// 33. Settings switches: a refused write puts the switch and the stored value back; a refused settings reset still reloads.
section(() => {
  const src = read("settingsPanel.js");
  const persist = extractBlock(src, "export async function persistFlagSwitch(").replace("export ", "");
  const onTop = extractBlock(src, "function paintOnTop(") + "\n" + extractBlock(src, "export async function setOnTop(").replace("export ", "");
  const reset = extractBlock(src, "export async function resetSettings(").replace("export ", "");
  later(async () => {
    const pin = { classes: {}, attrs: {}, classList: { toggle(c, v) { pin.classes[c] = v; } }, setAttribute(k, v) { pin.attrs[k] = v; } };
    const ontop = { checked: false };
    const els = { "opt-ontop": ontop, "pin-top": pin };
    const settings = { ui_flags: { autoPaste: true }, always_on_top: false };
    const errs = [];
    const sb = run([persist, onTop].join("\n"), { settings, $: (id) => els[id], invoke: async () => { throw new Error("denied"); }, errToast: (e) => errs.push(e) });
    const box = { checked: false };
    await sb.persistFlagSwitch(box, "autoPaste");
    check(box.checked === true && settings.ui_flags.autoPaste === true && errs.length === 1, "a refused flag write flips the switch back and keeps the old flag");
    const fresh = { checked: true };
    await sb.persistFlagSwitch(fresh, "tooltipsEnabled");
    check(fresh.checked === false && !("tooltipsEnabled" in settings.ui_flags), "a refused first write leaves no flag behind");
    await sb.setOnTop(true);
    check(settings.always_on_top === false && ontop.checked === false && pin.attrs["aria-pressed"] === "false" && pin.classes.active === false, "a refused pin restores state, switch and pin");
    let reloaded = 0;
    const r = run(reset, { $: () => ({}), armButton: () => true, clearTimeout() {}, disarmButton() {}, t: (k) => k, maybeBackupBeforeWipe: async () => true, resetSettingsTimer: null,
      invoke: async () => { throw new Error("denied"); }, errToast: (e) => errs.push(e), reloadEverything: async () => { reloaded++; } });
    await r.resetSettings();
    check(errs.length === 4 && reloaded === 1, "a refused settings reset is reported and the window still resyncs");
  });
});

// 34. Tile style and interface size: a refused write restores the stored values and the selects.
section(() => {
  const src = read("settingsPanel.js");
  const tile = extractBlock(src, "export async function tileStyleChanged(").replace("export ", "");
  const scale = extractBlock(src, "export const scaleChanged = ").replace("export const scaleChanged", "var scaleChanged");
  later(async () => {
    const font = { value: "mono" };
    const size = { value: "20" };
    const els = { "tile-font": font, "tile-size": size };
    const settings = { tile_font: "system", tile_size: 14, ui_values: { uiScale: 100 } };
    const errs = [];
    let applied = 0;
    const filled = [];
    const sb = run([tile, scale].join("\n"), {
      settings, $: (id) => els[id], normSize: (n) => n, applyTileStyle() { applied++; }, renderGrid: async () => {}, applyValues() {},
      fillScaleSelect: (id, key) => filled.push(id + ":" + key), invoke: async () => { throw new Error("denied"); }, errToast: (e) => errs.push(e),
    });
    await sb.tileStyleChanged();
    check(settings.tile_font === "system" && settings.tile_size === 14 && font.value === "system" && size.value === "14" && errs.length === 1 && applied === 2, "a refused tile style restores font, size and both selects");
    await sb.scaleChanged("uiScale")({ target: { value: "150", id: "ui-scale" } });
    check(settings.ui_values.uiScale === 100 && filled.join() === "ui-scale:uiScale" && errs.length === 2, "a refused scale restores the value and the select");
    await sb.scaleChanged("iconScale")({ target: { value: "90", id: "x" } });
    check(!("iconScale" in settings.ui_values), "a refused first scale write leaves no key behind");
  });
});

// 35. Every settings directory area has a badge label key.
section(() => {
  const dir = read("settingsSearch.js");
  const areas = new Set([...dir.matchAll(/area: "([a-z]+)"/g)].map((m) => m[1]));
  const cfg = read("expertConfig.js");
  const labels = /SETTINGS_AREA_LABEL = \{([^}]*)\}/.exec(cfg)[1];
  const missing = [...areas].filter((a) => !new RegExp("\\b" + a + ":").test(labels));
  check(areas.size >= 5 && !missing.length, "area badge labels missing for: " + missing.join(","));
});

// 36. A backup hit from settings search opens the dialog through the shared path and waits for every panel.
section(() => {
  const open = extractBlock(read("backups.js"), "export function openBackups(");
  check(/renderStoragePanel/.test(open) && /renderRetentionPanel/.test(open) && /renderBackupPanel/.test(open) && /Promise\.all/.test(open), "openBackups returns the render of all three panels");
  const search = extractBlock(read("settingsSearch.js"), "export async function openSettingsDirectoryEntry(");
  check(/await openBackups\(\)/.test(search) && !/renderBackupPanel/.test(search), "settings search uses the shared backup open path");
});

// 37. Views: a half-finished undo still redraws; a half-saved edit shows what landed.
section(() => {
  const src = read("views.js");
  const restore = extractBlock(src, "export async function restoreDeletedView(").replace("export ", "");
  const confirm = extractBlock(src, "export async function confirmViewModal(").replace("export ", "");
  later(async () => {
    const calls = [];
    const settings = { active_view: "v9", views: [] };
    const sb = run(restore, {
      settings, val: () => 8, libraryEl: { classList: { contains: () => true } }, renderLibrary() {}, batchErrorToast() {},
      assignSettings: () => {}, invoke: async (cmd) => { if (cmd === "set_view_grid") throw new Error("denied"); return {}; },
      renderViews: () => calls.push("views"), renderGrid: async () => { calls.push("grid"); },
    });
    let threw = false;
    try { await sb.restoreDeletedView({ name: "A", cols: 2, rows: 2, members: [] }, null); } catch (_) { threw = true; }
    check(threw && calls.join() === "views,grid", "a failing undo step still redraws the tab bar and the grid");

    const assigned = [];
    const redraw = [];
    const errs = [];
    const modal = { name: { value: "New name", focus() {} }, cols: { value: "2" }, rows: { value: "2" } };
    const sb2 = run(confirm, {
      viewModal: modal, viewModalId: "v1", viewModalColor: "", viewModalSaving: false, settings: { views: [] }, clampGrid: (v) => Number(v),
      assignSettings: (r) => assigned.push(r), errToast: (e) => errs.push(e), closeViewModal() {}, renderViews: () => redraw.push("views"), renderGrid: async () => {},
      invoke: async (cmd) => { if (cmd === "set_view_color") throw new Error("denied"); return { renamed: true }; },
    });
    await sb2.confirmViewModal();
    check(assigned.length === 1 && assigned[0].renamed && redraw.length === 1 && errs.length === 1, "a rename that landed is assigned and drawn even when a later step fails");
  });
});

// 38. Placing a tile: a refused write reads the stored layout back and redraws; no undo is offered.
section(() => {
  const src = read("tiles.js");
  const place = extractBlock(src, "export async function placeTile(").replace("export ", "");
  later(async () => {
    for (const bad of ["set_layout", "batch_prompts"]) {
      const view = { id: "v1", members: [], layouts: {}, cols: 2, rows: 2 };
      const other = { id: "v2", members: ["p1"], layouts: { k: { p1: [0, 0] } } };
      const log = [];
      const sb = run(place, {
        gridEl: { querySelector: () => null }, activeView: () => view, optFlag: () => false, viewsContaining: () => [other],
        layoutOf: (v) => v.layouts.k || (v.layouts.k = {}), gridKeyOf: () => "k", moveTileDom: () => true,
        invoke: async (cmd) => { log.push(cmd); if (cmd === bad) throw new Error("denied"); return {}; },
        errToast: () => log.push("toast"), assignSettings: () => log.push("assign"), renderGrid: async () => { log.push("grid"); },
        folderById: () => null, saveFolder: async () => true,
      });
      const ok = await sb.placeTile("p1", 1, 1);
      check(ok === false && log.includes("toast") && log.includes("assign") && log[log.length - 1] === "grid", "a refused " + bad + " is toasted, re-read and redrawn");
    }
    const okLog = [];
    const sb2 = run(place, {
      gridEl: { querySelector: () => null }, activeView: () => ({ id: "v1", members: [], layouts: {} }), optFlag: () => true, viewsContaining: () => [],
      layoutOf: (v) => v.layouts.k || (v.layouts.k = {}), gridKeyOf: () => "k", moveTileDom: () => true,
      invoke: async () => ({}), errToast: () => okLog.push("toast"), assignSettings: () => okLog.push("assign"), renderGrid: async () => okLog.push("grid"),
      folderById: () => null, saveFolder: async () => true,
    });
    check((await sb2.placeTile("p1", 0, 0)) === true && !okLog.length, "an accepted placement stays quiet and returns true");
  });
});

// 39. Grid tiles carry a role and a name, and the keyboard ring is announced.
section(() => {
  const build = extractBlock(read("tiles.js"), "export function buildTile(");
  check(/setAttribute\("role", "button"\)/.test(build) && /setAttribute\("aria-label", p\.locked/.test(build), "a tile is a named button");
  const focus = extractBlock(read("copyFlow.js"), "export function focusTile(");
  check(/aria-current/.test(focus) && /grid-live/.test(focus), "the focus ring moves aria-current and a live region");
  check(/id="grid-live"[^>]*aria-live="polite"/.test(read("index.html")), "the live region exists in the page");
});

// 40. A refused backup setting is rolled back so the dialog shows what the backend uses.
section(() => {
  const save = extractBlock(read("backups.js"), "export async function saveBackupSetting(").replace("export ", "");
  later(async () => {
    const settings = { ui_flags: { autoBackup: true }, ui_values: { backupKeep: 5 } };
    const errs = [];
    let applied = 0;
    const sb = run(save, { settings, invoke: async () => { throw new Error("readonly"); }, errToast: (e) => errs.push(e), applyFlags() { applied++; }, applyValues() { applied++; } });
    await sb.saveBackupSetting("flag", "autoBackup", false);
    await sb.saveBackupSetting("value", "backupKeep", 9);
    await sb.saveBackupSetting("value", "backupGfsDays", 3);
    check(settings.ui_flags.autoBackup === true && settings.ui_values.backupKeep === 5 && !("backupGfsDays" in settings.ui_values) && errs.length === 3 && applied === 3, "a refused backup write restores the old value and redraws");
    const ok = { ui_flags: {}, ui_values: {} };
    const sb2 = run(save, { settings: ok, invoke: async () => ({}), errToast() {}, applyFlags() {}, applyValues() {} });
    await sb2.saveBackupSetting("value", "backupKeep", 7);
    check(ok.ui_values.backupKeep === 7, "an accepted backup write stays");
  });
});

// 41. Rebuilding the hotkey list while one of its inputs has the focus ends the capture switch.
section(() => {
  const render = extractBlock(read("hotkeys.js"), "export function renderHotkeysList(");
  const i = render.indexOf("box.innerHTML");
  const guardAt = render.indexOf("hotkeyCapture(false)");
  check(guardAt !== -1 && guardAt < i && /box\.contains\(document\.activeElement\)/.test(render), "the list ends hotkey capture before it drops a focused input");
});

// 42. Two overlapping copies of a rotating prompt advance the rotation once.
section(() => {
  const src = read("copyFlow.js");
  const set = "const rotatingInFlight = new Set();";
  const fn = extractBlock(src, "export async function peekResolveCommit(").replace("export ", "");
  check(src.includes(set), "the in-flight set exists");
  later(async () => {
    const calls = [];
    const sb = run(set + "\n" + fn, {
      invoke: async (cmd) => { calls.push(cmd); return cmd === "rotation_peek" ? { text: "x" } : {}; },
      resolveVars: async (x) => x, recordToolUsage() {},
    });
    const p = { id: "a", text: "t", variants: ["v"] };
    const slow = async () => { await new Promise((r) => setTimeout(r, 5)); return true; };
    const [one, two] = await Promise.all([sb.peekResolveCommit(p, slow), sb.peekResolveCommit(p, slow)]);
    check(one === true && two === false && calls.filter((c) => c === "rotation_commit").length === 1 && calls.filter((c) => c === "rotation_peek").length === 1, "the second overlapping copy is dropped");
    check((await sb.peekResolveCommit(p, slow)) === true, "a later copy works again");
    const plain = { id: "b", text: "t", variants: [] };
    const both = await Promise.all([sb.peekResolveCommit(plain, slow), sb.peekResolveCommit(plain, slow)]);
    check(both[0] === true && both[1] === true, "a plain prompt is never dropped");
  });
});

// 43. The auto update switch rolls back when the write is refused.
section(() => {
  const src = read("about.js");
  const m = /\$\("opt-autoupdate"\)\.addEventListener\("change", (async \(e\) => \{[\s\S]*?\n  \})\);/.exec(src);
  check(!!m, "the auto update handler is async");
  if (!m) return;
  later(async () => {
    let handler = null;
    const settings = { auto_update: true };
    const errs = [];
    const sb = run("var h = " + m[1] + ";", { settings, fillUpdateIntervalSelect() {}, errToast: (e) => errs.push(e), invoke: async () => { throw new Error("save-failed"); } });
    const box = { checked: false };
    await sb.h({ target: box });
    check(settings.auto_update === true && box.checked === true && errs.length === 1, "a refused auto update write restores the setting and the switch");
  });
});

// 44. A dictation delivered to the composer fires an input event.
section(() => {
  const src = read("dictation.js");
  const fn = extractBlock(src, "export async function deliverVoiceHotkeyText(").replace("export ", "");
  later(async () => {
    const events = [];
    const inputEl = { value: "", dispatchEvent: (e) => events.push(e.type) };
    const sb = run(fn, { inputEl, txt: () => "input", autoGrow() {}, Event: class { constructor(type) { this.type = type; } } });
    await sb.deliverVoiceHotkeyText("hello");
    check(inputEl.value === "hello" && events.join() === "input", "dictated text into the composer dispatches an input event");
  });
});

// 45. A help article head is a button that reports its folded state.
section(() => {
  const src = read("helpView.js");
  check(/titleSpan = document\.createElement\("button"\)/.test(src), "the help article title is a button");
  check(/titleSpan\.setAttribute\("aria-expanded", String\(!collapsedNow\)\)/.test(src), "the button starts with its folded state");
  check(/titleSpan\.setAttribute\("aria-expanded", String\(!sec\.classList\.contains\("collapsed"\)\)\)/.test(src), "a toggle updates aria-expanded");
});

// 46. The all-hotkeys list carries the dictation hotkey row.
section(() => {
  const src = read("hotkeys.js");
  check(/voice: \{ current: \(\) => settings\.voice_hotkey, onCommit: commitVoiceHotkey \}/.test(src), "the list has a voice kind");
  check(/flagState\("voiceHotkey"\) \? \[\{ kind: "voice"/.test(src), "the voice row is gated like the settings field");
  const fn = extractBlock(src, "export async function commitVoiceHotkey(");
  check(fn.includes("renderHotkeysList()"), "committing the dictation hotkey refreshes an open list");
});

// 47. The favorite context entry carries its translation key.
section(() => {
  const html = read("index.html");
  check(html.includes('<span class="ctx-label" data-i18n="favoriteAdd">Add to favorites</span>'), "the favorite menu label has data-i18n");
});

// 48. The confirm dialog is named and described.
section(() => {
  const html = read("index.html");
  check(/id="confirm-modal"[^>]*aria-labelledby="confirm-title"[^>]*aria-describedby="confirm-msg"/.test(html), "the confirm alertdialog names its title and message");
});

// 49. Camera fallback names use a keyed template and the draft preview cuts by code point.
section(() => {
  const src = read("main.js");
  check(src.includes('t("cameraDeviceN").replace("{n}", () => String(i + 1))'), "the camera fallback name uses the {n} template");
  check(!src.includes('${t("cameraMenuItem")} ${i + 1}'), "no hard-coded word order remains");
  check(/Array\.from\([^;]*\.trim\(\)\)\.slice\(0, 140\)\.join\(""\)/.test(src), "the draft preview is cut by code point");
});

// 50. Rotation entry and fixed variable inputs carry accessible names.
section(() => {
  const src = read("editor.js");
  check(src.includes('ta.setAttribute("aria-label", t("rotateEntryN").replace("{n}", () => String(i + 1)))'), "a rotation text field is named by its position");
  check(src.includes('valueEl.setAttribute("aria-label", name)'), "a fixed variable value field is named by its variable");
});

// 51. Undo toast label and suggested search name are cut by code point.
section(() => {
  const src = read("library.js");
  check(src.includes('Array.from(String(found?.text || "").trim()).slice(0, 40).join("")'), "the undo label is cut by code point");
  const fn = extractBlock(src, "export function suggestedSearchName(").replace("export ", "");
  const sb = run(fn + "; var out = [suggestedSearchName('a'.repeat(39) + String.fromCodePoint(0x1F600) + 'b'.repeat(10)), suggestedSearchName('short')];", {});
  check(sb.out[0] === 'a'.repeat(39) + String.fromCodePoint(0x1F600) + "…", "an emoji at the boundary stays whole");
  check(sb.out[1] === "short", "a short query is unchanged");
});

// 52. Expert value controls are named by their row label.
section(() => {
  const src = read("expertMenu.js");
  const n = (src.match(/\.setAttribute\("aria-label", span\.textContent\)/g) || []).length;
  check(n === 5, "the input, slider, both select rows and the custom number are named (" + n + ")");
});

// 53. The expert colour swatch reopens at the colour last picked.
section(() => {
  const src = read("expertMenu.js");
  check(src.includes("openColorPop(swatch, hexOut.textContent, setColor)"), "the popup starts at the live colour");
  check(!src.includes("openColorPop(swatch, cur, setColor)"), "no stale render-time colour is passed");
});

// 54. The custom-entry unit of a select row is localized and follows the count.
section(() => {
  const src = read("expertMenu.js");
  check(src.includes("fmtUnitName(n, TIME_UNITS[cfg.unit], LANG)"), "the custom unit goes through the Intl unit name");
  check(!src.includes("unit.textContent = cfg.unit;"), "no raw config unit is written beside the number");
  const fn = extractBlock(src, "export function selectRow(");
  check((fn.match(/setUnit\(/g) || []).length >= 3, "the unit refreshes on resync and on a typed value");
});

// 55. A prompts-only import re-renders once and applies no settings.
section(() => {
  const src = read("settingsPanel.js");
  const fn = extractBlock(src, "export async function runImport(");
  check((fn.match(/renderGrid\(/g) || []).length === 1, "the grid is built once after an import");
  check(!/applyTheme|applyTileStyle|applyBars|setLang\(/.test(fn), "no preference is re-applied after a prompts-only import");
});

let guard;
const timedOut = new Promise((resolve) => { guard = setTimeout(() => { problems.push("an async check never finished"); resolve(); }, 3000); });
Promise.race([Promise.all(pending), timedOut]).then(() => {
  clearTimeout(guard);
  if (problems.length) {
    console.error("FAIL");
    for (const p of problems) console.error("  - " + p);
    process.exit(1);
  }
  console.log("main window fixes: ok");
});
