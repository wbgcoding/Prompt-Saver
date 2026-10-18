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
