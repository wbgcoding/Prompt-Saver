// Satellite-window fixes (video editor, recording tools, variables, ...): each section runs the
// real extracted source of the function in a vm, or reads the one source line that carries the fix.
// UI_CHECK_DIR points the test at another copy of ui/ (to prove it fails on older code).
// Run:  node tests/ui/satellite_fixes_check.js      Exit: 0 = clean, 1 = problems.
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

// Batch export: two sources that map to the same output name get distinct names.
section(() => {
  const src = read("recordingTools.js");
  const sb = run(extractBlock(src, "export function batchDestNames(").replace("export ", ""), {});
  const names = sb.batchDestNames(["rec.mp4", "rec.mkv", "REC.webm", "other.mp4"], "mp4");
  check(names[0] === "rec.mp4", "the first file keeps its plain name");
  check(new Set(names.map((n) => n.toLowerCase())).size === names.length, "every batch output name is unique");
  check(names[1] === "rec (2).mp4" && names[2] === "REC (3).mp4", "later clashes get a numbered suffix");
  check(names[3] === "other.mp4", "a file without a clash keeps its name");
  check(/batchDestNames\(names, format\)/.test(src), "the batch export builds its destinations with batchDestNames");
});

// Video editor: a second "load" request queued behind the first confirm must not discard the
// edit state of the source the first request just loaded.
later(async () => {
  const src = read("video.js");
  let handler = null;
  const discards = [];
  const gates = [];
  const sb = {
    listen: (name, fn) => { if (name === "video-editor-load") handler = fn; },
    VE: { src: "A.mp4", ops: [1], dirty: true },
    t: (k) => k,
    stopEditStateAutosave: () => {},
    invoke: async (cmd, args) => { if (cmd === "discard_edit_state") discards.push(args.target); },
    confirmDialog: () => new Promise((resolve) => gates.push(resolve)),
    loadRecording: (p) => { sb.VE.src = p; sb.VE.ops = []; sb.VE.dirty = false; },
  };
  run(extractBlock(src, 'listen("video-editor-load", async (e)') + ");", sb);
  const first = handler({ payload: "B.mp4" });
  const second = handler({ payload: "C.mp4" });
  gates[0](true);
  await first;
  gates[1](true);
  await second;
  check(discards.length === 1 && discards[0] === "A.mp4", `only the replaced source loses its edit state (got ${JSON.stringify(discards)})`);
  check(sb.VE.src === "C.mp4", "the last confirmed request is the one that ends up loaded");
});

// Overlay "Change file": the commit after the picker/probe is skipped when the overlay was removed
// or another recording was loaded meanwhile.
section(() => {
  const src = read("videoOverlays.js");
  const sb = { VE: { loadGen: 3, ops: [{ id: "o1" }] } };
  const line = src.slice(src.indexOf("export const overlayStillInEditor")).split("\n")[0].replace("export ", "");
  vm.createContext(sb);
  sb.overlayStillInEditor = vm.runInContext(line + "\noverlayStillInEditor", sb);
  check(sb.overlayStillInEditor({ id: "o1" }, 3) === true, "an overlay still in the same recording may be committed");
  check(sb.overlayStillInEditor({ id: "o1" }, 2) === false, "another recording was loaded: no commit");
  check(sb.overlayStillInEditor({ id: "o2" }, 3) === false, "the overlay was removed: no commit");
  const guards = src.match(/const gen = VE\.loadGen;\n\s+const path = await invoke\("pick_file_path"\)\.catch\(\(\) => null\);\n\s+if \(!path \|\| !overlayStillInEditor\(op, gen\)\) return;/g) || [];
  check(guards.length === 2, "both Change file handlers re-check the overlay after the picker");
  check(/probe_extra_video_source[^\n]*\n\s+if \(!overlayStillInEditor\(op, gen\)\) return;/.test(src), "the video handler re-checks after the probe");
});

// Video editor opened without a source: the "no recording" message waits for the texts and the
// label's translation hook follows it, so a language pass cannot turn it back into "Loading".
section(() => {
  const src = read("video.js");
  const at = src.indexOf('if (path) await loadRecording(path, originDir, overlayPath);');
  const tail = src.slice(at, at + 700);
  check(/await textsReady;[\s\S]*dataset\.i18n = "veNoRecording"[\s\S]*t\("veNoRecording"\)/.test(tail), "the empty-window message waits for the texts and re-points its i18n hook");
  check(/ve-stage-msg"\)\.dataset\.i18n = "veLoading"/.test(read("videoLoad.js")), "loading a recording restores the loading label's hook");
});

// Video editor: a format change keeps the destination the user chose and swaps only the extension.
section(() => {
  const src = read("video.js");
  const line = src.slice(src.indexOf("export const withExtension")).split("\n")[0].replace("export ", "");
  const sb = {};
  vm.createContext(sb);
  sb.withExtension = vm.runInContext(line + "\nwithExtension", sb);
  check(sb.withExtension("D:\\clips\\talk.mp4", "webm") === "D:\\clips\\talk.webm", "the extension is swapped, folder and name stay");
  check(sb.withExtension("D:\\my.clips\\talk", "gif") === "D:\\my.clips\\talk.gif", "a dot in a folder name is not an extension");
  check(sb.withExtension("D:\\clips\\a.b.mkv", "mp4") === "D:\\clips\\a.b.mp4", "only the last extension is replaced");
  check(!src.includes('if (id === "ve-export-format") VE.destOverride = null;') &&/VE\.destOverride = withExtension\(VE\.destOverride/.test(src), "the format dropdown no longer clears the chosen destination");
});

// Only the backend's exact "cancelled" answer is a user cancel; a failure whose text merely
// mentions "cancel" (a file name, an ffmpeg line) still shows the error with its details.
section(() => {
  check(!/raw\.includes\("cancel"\)/.test(read("videoExport.js")) && /raw\.trim\(\) === "cancelled"/.test(read("videoExport.js")), "the video export treats only the exact cancelled answer as a cancel");
  check(!/raw\.includes\("cancel"\)/.test(read("capture.js")) && /raw\.trim\(\) === "cancelled"/.test(read("capture.js")), "the recording conversion treats only the exact cancelled answer as a cancel");
});

// Video editor timeline: "Add cut" clears the selection before the redraw, and the ruler labels
// of a recording of 100 minutes or more keep their full minutes.
section(() => {
  const src = read("video.js");
  const code = ["export const pad", "export function fmtTime(", "export function rulerLabel("]
    .map((m) => (m.includes("(") ? extractBlock(src, m) : src.slice(src.indexOf(m)).split("\n")[0].replace(/;$/, "") + ";").replace("export ", ""))
    .join("\n");
  const sb = {};
  vm.createContext(sb);
  sb.rulerLabel = vm.runInContext(code + "\nrulerLabel", sb);
  check(sb.rulerLabel(125000, false) === "02:05" && sb.rulerLabel(125250, true) === "02:05.250", "ruler labels keep their short and fine forms");
  check(sb.rulerLabel(100 * 60000 + 5000, false) === "100:05", "a ruler label past 100 minutes is not cut short");
  check(sb.rulerLabel(100 * 60000, true) === "100:00.000", "a fine ruler label past 100 minutes is whole");
  const add = extractBlock(src, "export function cutSelection(");
  check(add.indexOf("VE.sel = null") !== -1 && add.indexOf("VE.sel = null") < add.indexOf("pushOp("), "Add cut clears the selection before pushOp redraws the timeline");
});

// Loading another recording stops every inserted-video preview node before the layer is emptied.
section(() => {
  const src = read("videoPreview.js");
  class FakeVideo { constructor() { this.paused = false; this.src = "x"; this.loads = 0; } pause() { this.paused = true; } removeAttribute() { this.src = null; } load() { this.loads++; } }
  const vid = new FakeVideo();
  const layer = { innerHTML: "<video>" };
  const sb = { HTMLVideoElement: FakeVideo, VE: { liveEls: new Map([["a", vid], ["b", { tag: "img" }]]) } };
  const fn = extractBlock(src, "export function clearLiveOverlays(").replace("export ", "");
  vm.createContext(sb);
  sb.clearLiveOverlays = vm.runInContext(fn + "\nclearLiveOverlays", sb);
  sb.clearLiveOverlays(layer);
  check(vid.paused && vid.src === null && vid.loads === 1, "an inserted video node is paused and released");
  check(sb.VE.liveEls.size === 0 && layer.innerHTML === "", "the map and the layer are emptied");
  check(/clearLiveOverlays\(\$\("ve-overlay-live"\)\)/.test(read("video.js")), "loading another source uses the helper");
});

// Closing the screenshot editor gives the full-size canvases back (base, live, marker layer).
section(() => {
  const draw = read("snipDraw.js");
  const sb = { document: { createElement: () => ({ width: 10, height: 10 }) }, SE: { w: 10, h: 10 } };
  vm.createContext(sb);
  const body = draw.slice(draw.indexOf("let snipEditMarkerTmp = null;"), draw.indexOf("function snipEditDrawMarkerOp(") + 40);
  const code = body.replace(/function snipEditDrawMarkerOp\([\s\S]*$/, "") + "\nfunction grab() { return snipEditMarkerTmp = document.createElement('canvas'); }";
  vm.runInContext(code + "\nthis.release = snipEditReleaseMarkerLayer; this.grab = grab;", sb);
  sb.release();
  const tmp = sb.grab();
  sb.release();
  check(tmp.width === 0 && tmp.height === 0, "the marker stroke layer shrinks to nothing");
  const ed = read("snipEditor.js");
  const close = extractBlock(ed, "function snipEditClose(");
  check(/snipEditBaseCv\.width = 0;[\s\S]*snipEditLiveCv\.height = 0;[\s\S]*snipEditReleaseMarkerLayer\(\)/.test(close), "closing the editor empties the base and live canvases and the marker layer");
});

// Camera window: a device change refreshes the microphone list, and restarts the preview only
// when the picked microphone vanished or the stream is dead.
later(async () => {
  const src = read("camera.js");
  check(/navigator\.mediaDevices\?\.addEventListener\?\.\("devicechange", onMediaDevicesChanged\)/.test(src), "the camera window listens for devicechange");
  const make = (before, after, stream) => {
    const calls = { list: 0, preview: 0 };
    const sb = {
      permissionReady: true, settingsReady: true, stream,
      audioSelect: { value: before },
      populateAudioDevices: async () => { calls.list++; sb.audioSelect.value = after; },
      refreshPreview: () => { calls.preview++; },
    };
    vm.createContext(sb);
    vm.runInContext(extractBlock(src, "async function onMediaDevicesChanged(").replace("async function onMediaDevicesChanged", "sb_fn = async function"), Object.assign(sb, { sb_fn: null }));
    return { sb, calls };
  };
  let m = make("mic1", "mic1", { active: true });
  await m.sb.sb_fn();
  check(m.calls.list === 1 && m.calls.preview === 0, "an unrelated device change only rebuilds the list");
  m = make("mic1", "", { active: true });
  await m.sb.sb_fn();
  check(m.calls.preview === 1, "an unplugged picked microphone restarts the preview");
  m = make("", "", { active: false });
  await m.sb.sb_fn();
  check(m.calls.preview === 1, "a dead stream restarts the preview");
  m = make("", "", null);
  m.sb.permissionReady = false;
  await m.sb.sb_fn();
  check(m.calls.list === 0, "nothing happens before the first preview is allowed");
});

// Floating pill: switching a playing video to an image or text is the app's pause, not the
// user's, and a new video source forgets an older user pause.
later(async () => {
  const src = read("floating.js");
  const start = src.indexOf("let appPause = false;");
  const code = src.slice(start, src.indexOf("function pauseByApp()", start)) + extractBlock(src, "function pauseByApp(");
  const listeners = {};
  const videoEl = {
    paused: false,
    addEventListener: (n, fn) => { listeners[n] = fn; },
    pause() { if (!this.paused) { this.paused = true; Promise.resolve().then(() => listeners.pause()); } },
  };
  const sb = {};
  vm.createContext(sb);
  sb.videoEl = videoEl;
  vm.runInContext(code + "\nthis.pauseByApp = pauseByApp; this.state = () => userPaused;", sb);
  sb.pauseByApp();
  await Promise.resolve(); await Promise.resolve();
  check(sb.state() === false, "an app pause does not count as a user pause");
  const plain = src.slice(src.indexOf("pill.classList.remove(\"has-image\");") - 400, src.indexOf("pill.classList.remove(\"has-image\");"));
  check(!/videoEl\.pause\(\);/.test(plain) && /pauseByApp\(\);/.test(plain), "the text pill branch pauses through the app pause");
  check(/videoEl\.classList\.add\("hidden"\);\n\s+pauseByApp\(\);/.test(src), "the image branch pauses through the app pause");
  check(/videoEverPlayed = false;[^\n]*\n\s+userPaused = false;/.test(src), "a new video source resets the user pause");
});

// Number field: a held "+" stops at the limit instead of repeating change events forever.
section(() => {
  const src = read("floatShared.js");
  const code = extractBlock(src, "function stepBy(") + "\n" + extractBlock(src, "function armHold(");
  const timers = [];
  const el = { value: "8", min: "0", max: "10", step: "1", changes: 0, dispatchEvent() { this.changes++; return true; } };
  const sb = {
    el, bigStep: 0, Event: function Event() {},
    liveStep: () => 1,
    clampVal: (v) => Math.min(10, Math.max(0, v)),
    setTimeout: (fn) => { timers.push({ fn, interval: false, live: true }); return timers.length; },
    setInterval: (fn) => { timers.push({ fn, interval: true, live: true }); return timers.length; },
    clearTimeout: (id) => { if (timers[id - 1]) timers[id - 1].live = false; },
    clearInterval: (id) => { if (timers[id - 1]) timers[id - 1].live = false; },
  };
  vm.createContext(sb);
  vm.runInContext(code + "\nthis.armHold = armHold;", sb);
  const handlers = {};
  let captured = 0;
  const btn = { disabled: false, setPointerCapture: () => { captured++; }, addEventListener: (n, fn) => { handlers[n] = fn; } };
  sb.armHold(btn, 1);
  handlers.pointerdown({ button: 0, shiftKey: false, pointerId: 1 });
  check(captured === 1, "pressing a step button captures the pointer");
  timers[0].fn(); // the hold delay elapsed: the repeat starts
  const rep = timers[1];
  for (let i = 0; i < 6 && rep.live; i++) rep.fn();
  check(el.value === "10", `the held value ends at the maximum (got ${el.value})`);
  check(rep.live === false, "the repeat stops once a step changes nothing");
  check(el.changes <= 4, `no change event is sent for an unchanged value more than once (got ${el.changes})`);
  check(typeof handlers.lostpointercapture === "function", "losing the capture also ends the hold");
});

// Number field preset list: a row also reacts to click (Enter or Space), the selected row takes
// the focus on open, and a keyboard close hands the focus back to the list button.
section(() => {
  const src = read("floatShared.js");
  const mkEl = () => {
    const e = { handlers: {}, children: [], cls: new Set(), focused: 0, removed: false, parentNode: null };
    e.addEventListener = (n, fn) => { e.handlers[n] = fn; };
    e.appendChild = (c) => { e.children.push(c); c.parentNode = e; return c; };
    e.classList = { add: (c) => e.cls.add(c), contains: (c) => e.cls.has(c) };
    e.querySelector = (sel) => e.children.find((c) => c.cls.has(sel.slice(1))) || null;
    Object.defineProperty(e, "firstElementChild", { get: () => e.children[0] || null });
    e.focus = () => { e.focused++; sb.document.activeElement = e; };
    e.contains = (n) => n === e || e.children.includes(n);
    e.remove = () => { e.removed = true; };
    e.getBoundingClientRect = () => ({});
    return e;
  };
  const el = { value: "50", changes: 0, dispatchEvent() { this.changes++; return true; } };
  const listBtn = mkEl();
  const body = mkEl();
  const sb = { document: { createElement: mkEl, body, activeElement: null }, el, listBtn, presets: [25, 50, 100], clampVal: (v) => v, placePopover: () => {}, Event: function Event() {}, activeNumPop: null, activeNumPopTrigger: null };
  vm.createContext(sb);
  const closeFn = extractBlock(src, "function closeActiveNumPop(");
  const openFn = extractBlock(src, 'listBtn.addEventListener("click", () =>');
  vm.runInContext(closeFn + "\nthis.closeActiveNumPop = closeActiveNumPop;\nthis.open = () => {" + openFn.replace(/^listBtn\.addEventListener\("click", \(\) => \{/, "").replace(/\}\)?;?$/, "") + "};", sb);
  sb.open();
  const pop = body.children[0];
  const rows = pop.children;
  check(rows.length === 3 && rows[1].focused === 1, "the selected preset row takes the focus on open");
  rows[2].handlers.click();
  check(el.value === "100" && el.changes === 1, "Enter or Space on a row (a click) picks the preset");
  check(pop.removed && listBtn.focused === 1, "the list closes and the focus returns to the list button");
  rows[2].handlers.click();
  check(el.changes === 1, "a pointerdown followed by its click picks only once");
});

section(() => {
  const src = read("folder.js");
  const replies = [];
  const sb = { folderId: "f", state: null, invoke: () => new Promise((r) => replies.push(r)), renderIcon() {}, markSelectedSize() {}, menuEl: {}, open: "closed", reflow: async () => {} };
  vm.createContext(sb);
  vm.runInContext("var loadSeq = 0;\n" + extractBlock(src, "async function loadState(") + "\nthis.loadState = loadState;", sb);
  later(async () => {
    const first = sb.loadState();
    const second = sb.loadState();
    replies[1]({ scale: 1, members: ["new"] });
    await second;
    replies[0]({ scale: 1, members: ["old"] });
    await first;
    check(sb.state && sb.state.members[0] === "new", "an older state reply that lands last does not replace the newer one");
  });
});

section(() => {
  const src = read("snip.js");
  const calls = [];
  const mkBtn = () => { const b = { handlers: {}, classList: { add() {}, remove() {} }, style: {}, setAttribute() {}, addEventListener(n, f) { b.handlers[n] = f; } }; return b; };
  const buttons = [];
  const sb = {
    pickHistory: ["#ff0000"], pickMode: true, pickBusy: false, pickFmt: "hex", cur: { x: 1, y: 1 }, picks: 0,
    historyEl: { innerHTML: "", classList: { toggle() {} }, appendChild: (b) => buttons.push(b) },
    document: { createElement: () => mkBtn() },
    hexToRgb: () => [255, 0, 0], formatColor: () => "#ff0000",
    invoke: async (cmd) => { calls.push(cmd); return true; },
    pickHere() { sb.picks++; }, saveZoom() {}, setCur() {}, setZoom() {}, setTimeout: () => 0,
    NUDGE: {}, ZOOM_KEYS: {},
    window: { addEventListener: (n, f) => { sb.onKey = f; } },
  };
  vm.createContext(sb);
  vm.runInContext([extractBlock(src, "function renderHistory("), extractBlock(src, 'window.addEventListener("keydown"') + ");", "renderHistory();"].join(String.fromCharCode(10)), sb);
  const swatch = buttons[0];
  const evt = (extra) => Object.assign({ key: "Enter", preventDefault() { this.prevented = true; }, stopPropagation() {} }, extra);
  const onSwatch = evt({ target: { closest: (sel) => (sel === ".pick-history-item" ? swatch : null) } });
  sb.onKey(onSwatch);
  check(sb.picks === 0 && !onSwatch.prevented, "Enter on a focused history swatch does not pick the crosshair pixel");
  sb.onKey(evt({ target: { closest: () => null } }));
  check(sb.picks === 1, "Enter elsewhere still picks the crosshair pixel");
  later(async () => {
    swatch.handlers.click(evt({ detail: 0 }));
    await Promise.resolve();
    check(calls.includes("copy_text"), "a keyboard click on a swatch copies that swatch");
    calls.length = 0;
    swatch.handlers.click(evt({ detail: 1 }));
    await Promise.resolve();
    check(calls.length === 0, "a mouse click does not copy a second time after mouseup");
  });
});

section(() => {
  const src = read("snip.js");
  const toggles = [];
  const sb = {
    busy: false, start: { x: 10, y: 5 }, lastMove: { clientX: 80, clientY: 90 }, moveRaf: 1,
    sizeLabel: { textContent: "", classList: { toggle: (c, on) => toggles.push([c, on]) } },
    setRegion() {}, toImg: (x, y) => ({ x, y }), windowAt: () => null, fullDim() {}, WIN_RADIUS: 0,
  };
  vm.createContext(sb);
  vm.runInContext(extractBlock(src, "function onMove(") + "; this.onMove = onMove;", sb);
  sb.onMove();
  check(toggles.length === 1 && toggles[0][0] === "inside" && toggles[0][1] === true, "a drag whose box starts in the top 27 px puts the size label inside the box");
  sb.start = { x: 10, y: 60 }; sb.lastMove = { clientX: 80, clientY: 90 }; toggles.length = 0;
  sb.onMove();
  check(toggles.length === 1 && toggles[0][1] === false, "a drag below the top strip keeps the label above the box");
  check(/\.snip-size\.inside\s*\{[^}]*top:\s*4px/.test(fs.readFileSync(path.join(uiDir, "styles", "snip.css"), "utf8")), "the inside placement exists in the stylesheet");
});

section(() => {
  const html = read("video.html");
  const tag = (html.match(/<div id="confirm-modal"[^>]*>/) || [""])[0];
  check(/aria-labelledby="confirm-title"/.test(tag) && /aria-describedby="confirm-msg"/.test(tag), "the video window confirm dialog is named and described");
});

section(() => {
  // Each satellite window sets its page language where it loads its texts (the loaders run
  // inside promise chains at boot, so the source is the checkable surface).
  for (const f of ["camera.js", "floating.js", "folder.js", "launcher.js", "recbar.js", "snip.js", "vars.js", "tray.js"]) {
    check(/document\.documentElement\.lang\s*=/.test(read(f)), f + " sets the page language");
  }
});

section(() => {
  const src = read("launcher.js");
  const mkEl = () => {
    const e = { attrs: {}, children: [], dataset: {}, style: { setProperty() {} }, cls: new Set() };
    e.setAttribute = (k, v) => { e.attrs[k] = String(v); };
    e.removeAttribute = (k) => { delete e.attrs[k]; };
    e.getAttribute = (k) => e.attrs[k];
    e.appendChild = (c) => { e.children.push(c); return c; };
    e.addEventListener = () => {};
    e.scrollIntoView = () => {};
    e.classList = { add: (c) => e.cls.add(c), remove: (c) => e.cls.delete(c), toggle: (c, on) => (on ? e.cls.add(c) : e.cls.delete(c)) };
    Object.defineProperty(e, "innerHTML", { get: () => "", set: () => { e.children = []; } });
    return e;
  };
  const searchEl = mkEl();
  const resultsEl = mkEl();
  const emptyEl = mkEl();
  const rowsData = [{ id: "a", name: "A", text: "x" }, { id: "b", name: "B", text: "y" }];
  const sb = {
    searchEl, resultsEl, emptyEl, active: -1, TXT: {}, PREVIEW_LEN: 80,
    document: { createElement: mkEl },
    defaultList: () => rowsData, matchList: () => rowsData, varPreview: (x) => x, mdPlain: (x) => x, select() {},
  };
  searchEl.value = "";
  vm.createContext(sb);
  vm.runInContext([extractBlock(src, "function renderRow("), extractBlock(src, "function render("), extractBlock(src, "function syncActiveDescendant("), extractBlock(src, "function setActive("), "this.render = render; this.setActive = setActive;"].join(String.fromCharCode(10)), sb);
  sb.render();
  const rows = resultsEl.children;
  check(rows.length === 2 && rows.every((r) => r.attrs.role === "option" && r.id), "result rows are options with an id");
  check(rows[0].attrs["aria-selected"] === "true" && searchEl.attrs["aria-activedescendant"] === rows[0].id, "the first row is selected and referenced by the search field");
  sb.setActive(1);
  check(rows[0].attrs["aria-selected"] === "false" && rows[1].attrs["aria-selected"] === "true" && searchEl.attrs["aria-activedescendant"] === rows[1].id, "moving the highlight updates aria-selected and aria-activedescendant");
  const html = read("launcher.html");
  check(/id="results"[^>]*role="listbox"/.test(html) && /id="search"[^>]*aria-controls="results"/.test(html), "the launcher markup has the listbox and its controlling field");
  check(/searchEl\.setAttribute\("aria-label"/.test(src), "the search field gets an accessible name");
});

section(() => {
  const src = read("folder.js");
  const mkEl = () => {
    const e = { attrs: {}, children: [], dataset: {}, style: {}, className: "", textContent: "" };
    e.setAttribute = (k, v) => { e.attrs[k] = String(v); };
    e.appendChild = (c) => { e.children.push(c); return c; };
    e.querySelectorAll = () => [];
    return e;
  };
  const gridEl = mkEl();
  const sb = {
    gridEl, TILE: 72, GAP: 8, PAD: 10, state: { scale: 1, members: [{ id: "m1", name: "Greeting", variants: [1, 2] }] },
    TXT: { copied: "Copied" }, rotateBadgeOn: true, rotateInfo: new Map(), px: (n) => n + "px",
    rotateBadgeLabel: () => "2/5", primeMemberRotate() {}, paintFace() {}, fitText() {},
    gridSize: () => ({ cols: 1 }), document: { createElement: mkEl },
  };
  vm.createContext(sb);
  vm.runInContext(extractBlock(src, "function renderGrid(") + "; this.renderGrid = renderGrid;", sb);
  sb.renderGrid();
  const tile = gridEl.children[0];
  const badge = tile.children.find((c) => c.className === "rotate-badge");
  const ok = tile.children.find((c) => c.className === "member-ok");
  check(tile.attrs["aria-label"] === "Greeting", "the member tile is named by the prompt only");
  check(badge && badge.attrs["aria-hidden"] === "true" && ok && ok.attrs["aria-hidden"] === "true", "the rotate badge and the copied hint stay out of the accessible name");
});

section(() => {
  const src = read("tray.js");
  const mkEl = () => {
    const e = { attrs: {}, children: [], dataset: {}, style: {}, className: "", textContent: "" };
    e.setAttribute = (k, v) => { e.attrs[k] = String(v); };
    e.removeAttribute = (k) => { delete e.attrs[k]; };
    e.appendChild = (c) => { e.children.push(c); return c; };
    e.addEventListener = () => {};
    e.scrollIntoView = () => {};
    const set = new Set();
    e.classList = { add: (c) => set.add(c), remove: (c) => set.delete(c) };
    Object.defineProperty(e, "innerHTML", { get: () => "", set: () => { e.children = []; } });
    return e;
  };
  const rowsEl = mkEl();
  const searchEl = mkEl();
  const sb = { rowsEl, searchEl, rows: [], active: -1, document: { createElement: mkEl }, activate() {}, iconEl: () => null, actionIcon: () => null };
  vm.createContext(sb);
  vm.runInContext([extractBlock(src, "function setActive("), extractBlock(src, "function addRow("), extractBlock(src, "function addHeader("), extractBlock(src, "function addSeparator("), extractBlock(src, "function renderRows("), "this.renderRows = renderRows; this.setActive = setActive;"].join(String.fromCharCode(10)), sb);
  sb.renderRows([], [{ title: "Favs", entries: [{ id: "p1", label: "One" }, { id: "p2", label: "Two" }] }], "Show", "Quit");
  const opts = rowsEl.children.filter((c) => c.attrs.role === "option");
  check(opts.length === 4 && opts.every((o) => o.id), "tray rows are options with an id");
  check(rowsEl.children.filter((c) => c.className === "tray-menu-head" || c.className === "tray-menu-sep").every((c) => c.attrs.role === "presentation"), "headers and separators are presentational");
  check(opts[0].attrs["aria-selected"] === "true" && searchEl.attrs["aria-activedescendant"] === opts[0].id, "the first row is referenced by the search field");
  sb.setActive(2);
  check(opts[0].attrs["aria-selected"] === "false" && opts[2].attrs["aria-selected"] === "true" && searchEl.attrs["aria-activedescendant"] === opts[2].id, "moving the highlight updates aria-activedescendant");
  const html = read("tray.html");
  check(/id="rows"[^>]*role="listbox"/.test(html) && /id="tray-search"[^>]*aria-controls="rows"/.test(html), "the tray markup has the listbox and its controlling field");
  check(/searchEl\.setAttribute\("aria-label"/.test(src), "the tray search field gets an accessible name");
});

section(() => {
  const src = read("vars.js");
  const mkEl = (tag) => {
    const e = { tag, attrs: {}, children: [], style: {}, className: "", textContent: "", id: "", value: "", checked: false, classList: { add() {}, remove() {}, toggle() {} } };
    e.setAttribute = (k, v) => { e.attrs[k] = String(v); };
    e.appendChild = (c) => { e.children.push(c); return c; };
    e.append = (...c) => { e.children.push(...c); };
    e.addEventListener = () => {};
    e.focus = () => {}; e.select = () => {};
    return e;
  };
  const fieldsEl = mkEl("div");
  Object.defineProperty(fieldsEl, "innerHTML", { get: () => "", set: () => { fieldsEl.children = []; } });
  const sb = {
    fieldsEl, titleEl: mkEl("h1"), okEl: mkEl("button"), cancelEl: mkEl("button"), tooLongText: "", rows: [],
    FIXED_PIN_SVG: "", applyFloatTheme() {}, compactTexts: () => ({ rememberValue: "Remember value", fixedValueHint: "" }),
    document: { createElement: mkEl, documentElement: {} }, fitWindow() {},
  };
  vm.createContext(sb);
  vm.runInContext([extractBlock(src, "function buildChoice("), extractBlock(src, "function buildText("), extractBlock(src, "function render("), "this.render = render;"].join(String.fromCharCode(10)), sb);
  sb.render({ lang: "en", askRemember: true, rows: [{ name: "Who", value: "" }, { name: "Pick", options: ["a", "b"], value: "a" }] });
  const [textRow, choiceRow] = fieldsEl.children;
  const walk = (n, out = []) => { out.push(n); (n.children || []).forEach((c) => walk(c, out)); return out; };
  const labelsIn = (n) => walk(n).filter((x) => x.tag === "label");
  check(textRow.tag === "div" && choiceRow.tag === "div", "a field row is a plain block, not a label");
  check(!labelsIn(textRow).some((l) => walk(l).some((x) => x !== l && x.tag === "label")), "no label is nested inside another label");
  const nameLabel = labelsIn(textRow).find((l) => l.className === "vars-name");
  const input = walk(textRow).find((x) => x.tag === "input" && x.className === "vars-input");
  check(nameLabel && input && nameLabel.htmlFor === input.id && input.id, "the name label points at its text field");
  const btn = walk(choiceRow).find((x) => x.className.includes("vars-choice-btn"));
  check(btn && /vars-name-1 /.test(btn.attrs["aria-labelledby"] + " ") && btn.attrs["aria-labelledby"].includes(btn.id), "the choice button is named by the variable and its pick");
});

section(() => {
  const src = read("vars.js");
  let focused = 0;
  const handlers = [];
  const mkEl = (tag) => {
    const e = { tag, attrs: {}, children: [], style: {}, className: "", textContent: "", classList: { add() {}, remove() {}, toggle() {} } };
    e.setAttribute = () => {};
    e.appendChild = (c) => { e.children.push(c); return c; };
    e.append = (...c) => { e.children.push(...c); };
    e.addEventListener = (t, fn) => { if (t === "click" && tag === "button") handlers.push({ el: e, fn }); };
    e.focus = () => { if (e.className.includes("vars-choice-btn")) focused++; };
    return e;
  };
  const sb = { document: { createElement: mkEl } };
  vm.createContext(sb);
  vm.runInContext(extractBlock(src, "function buildChoice(") + "; this.buildChoice = buildChoice;", sb);
  sb.buildChoice(mkEl("div"), ["a", "b"], "a");
  const item = handlers.find((h) => h.el.className === "vars-choice-item" && h.el.textContent === "b");
  item.fn();
  check(focused === 1, "choosing an option returns the focus to the trigger button");
});

later(async () => {
  const src = read("recbar.js");
  let handler;
  const calls = [];
  const applied = [];
  let release;
  const sb = {
    paused: false,
    pauseBtn: { addEventListener: (t, fn) => { handler = fn; } },
    invoke: (cmd) => { calls.push(cmd); return new Promise((r) => { release = r; }); },
    applyPaused: (v) => { applied.push(v); sb.paused = v; },
  };
  vm.createContext(sb);
  vm.runInContext(extractBlock(src, 'pauseBtn.addEventListener("click", async () =>') + ");", sb);
  const done = handler();
  sb.paused = true; // the poll lands while the request is in flight
  release();
  await done;
  check(calls[0] === "pause_recording" && applied.length === 1 && applied[0] === true, "the pause click applies the state it asked for, not the flipped poll state");
});

later(async () => {
  const src = read("tray.js");
  let blurFn;
  const timers = new Map();
  let nextId = 1;
  let closes = 0;
  const el = () => ({ value: "", hidden: false, setAttribute() {}, focus() {} });
  const sb = {
    handover: null, closed: false, openedAt: Date.now(), BLUR_GRACE_MS: 400, tailTimer: 0, tailSent: false, searchEl: el(),
    window: { addEventListener: (t, fn) => { if (t === "blur") blurFn = fn; } },
    document: { hasFocus: () => false },
    setTimeout: (fn) => { const id = nextId++; timers.set(id, fn); return id; },
    clearTimeout: (id) => { timers.delete(id); },
    Date, close: () => { closes++; }, sendTail() {}, render() {}, syncDocLang() {},
    invoke: async (cmd) => (cmd === "tray_window_plan" ? { searchOn: false, searchPh: "x" } : undefined),
  };
  vm.createContext(sb);
  vm.runInContext(["var blurTimer = 0;", extractBlock(src, 'window.addEventListener("blur", () =>') + ");", extractBlock(src, "async function openMenu("), "this.openMenu = openMenu;"].join(String.fromCharCode(10)), sb);
  blurFn(); // a blur inside the grace period defers the close
  check(timers.size === 1, "a blur inside the grace period schedules one deferred close");
  await sb.openMenu();
  check(timers.size === 0, "reopening cancels the stale deferred close");
});

section(() => {
  const src = read("floatShared.js");
  const handlers = {};
  let steps = 0;
  const timers = [];
  const sb = {
    stepBy: () => { steps++; return true; },
    setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout() {}, setInterval: () => 1, clearInterval() {},
  };
  const btn = { disabled: false, setPointerCapture() {}, addEventListener: (t, fn) => { handlers[t] = fn; } };
  vm.createContext(sb);
  vm.runInContext(extractBlock(src, "function armHold(") + "; this.armHold = armHold;", sb);
  sb.armHold(btn, 1);
  handlers.pointerdown({ button: 2, pointerId: 1, shiftKey: false });
  handlers.pointerdown({ button: 1, pointerId: 1, shiftKey: false });
  check(steps === 0 && timers.length === 0, "a right or middle click neither steps the value nor starts the repeat");
  handlers.pointerdown({ button: 0, pointerId: 1, shiftKey: false });
  check(steps === 1 && timers.length === 1, "the primary button still steps and arms the repeat");
});

section(() => {
  const src = read("video.js");
  const block = extractBlock(src, '$("ve-still").addEventListener("click", async () =>');
  const run = async (saved) => {
    const toasts = [];
    let handler;
    const sb = {
      $: () => ({ addEventListener: (_n, fn) => { handler = fn; } }),
      invoke: async () => saved,
      VE: { src: "a.mp4", playhead: 1000 },
      toast: (m) => toasts.push(m),
      t: (k) => ({ veStillSaved: "Saved {path}", veStillNotSaved: "Not saved" }[k] || k),
      noteFfmpeg() {}, errMessageLogged: () => "err", Math,
    };
    vm.createContext(sb);
    vm.runInContext(block + ");", sb);
    await handler();
    return toasts[0];
  };
  later(async () => {
    check(await run("out/f.png") === "Saved out/f.png", "a saved frame reports its path");
    check(await run("") === "Not saved", "an empty result must not claim the frame was saved");
  });
});

section(() => {
  const src = read("video.js");
  const consts = src.match(/^const (waveColors|WAVE_HEIGHT|WAVE_MAX_PX).*$/gm) || [];
  const code = [...consts, extractBlock(src, "function waveColor("), extractBlock(src, "function drawWave(")].join(String.fromCharCode(10));
  let styleReads = 0;
  const canvas = { style: {}, getContext: () => ctx };
  const ctx = { setTransform() {}, clearRect() {}, fillRect() {}, fillStyle: "" };
  const sb = {
    $: () => canvas, VE: { peaks: [{ values: [0.5, 0.5] }] }, trackWidth: () => 200, Math, Number,
    window: { devicePixelRatio: 2 }, document: { documentElement: {} },
    getComputedStyle: () => { styleReads++; return { getPropertyValue: () => " #123456 " }; },
  };
  vm.createContext(sb);
  vm.runInContext(code + String.fromCharCode(10) + "this.drawWave = drawWave;", sb);
  sb.drawWave(0);
  sb.drawWave(0);
  check(canvas.width === 400 && canvas.height === 68, "the waveform backing store is scaled by the device pixel ratio");
  check(canvas.style.width === "200px" && canvas.style.height === "34px", "the waveform keeps its CSS layout size");
  check(styleReads === 1 && ctx.fillStyle === "#123456", "the waveform colour is read once and reused");
  check(/waveColors\[0\] = waveColors\[1\] = null/.test(src), "a theme change drops the cached waveform colours");
});

section(() => {
  const src = read("videoOverlays.js");
  const m = src.match(/endMs: (Math\.min\(VE\.dur, startMs .*\)),/);
  check(!!m, "the camera overlay end expression exists");
  if (!m) return;
  const end = (startMs, durationMs) => vm.runInNewContext(m[1], { Math, VE: { dur: 10000 }, startMs, durationMs });
  check(end(2000, 0) === 10000, "a probe without a duration runs to the end of the clip, not zero length");
  check(end(2000, 3000) === 5000, "a known duration still ends after start plus duration");
  check(end(2000, 30000) === 10000, "the overlay never runs past the clip end");
});

section(() => {
  const vsrc = read("video.js");
  const ids = [...vsrc.slice(vsrc.indexOf("export const FONT_FILES"), vsrc.indexOf("];", vsrc.indexOf("export const FONT_FILES"))).matchAll(/\["(\w+)", "/g)].map((m) => m[1]);
  const labelBlock = vsrc.match(/export const FONT_LABELS = (\{[^}]*\});/);
  check(!!labelBlock, "font display names exist");
  if (!labelBlock) return;
  const labels = vm.runInNewContext("(" + labelBlock[1] + ")");
  check(ids.length >= 5 && ids.every((id) => labels[id] && labels[id] !== id), "every font id has a readable display name");
  const osrc = read("videoOverlays.js");
  check(/selectInput\(FONT_FILES\.map\(\(\[id\]\) => \[id, FONT_LABELS\[id\] \|\| id\]\)/.test(osrc), "the font select shows the display names");
});

section(() => {
  const src = read("recordingTools.js");
  const code = extractBlock(src, "export async function openVideoCleanup(").replace("export ", "");
  const listeners = {};
  const mkEl = (id) => {
    const e = { id, value: "30", textContent: "", disabled: false, checked: false, children: [] };
    e.classList = { add() {}, remove() {}, toggle() {} };
    e.appendChild = (c) => { e.children.push(c); return c; };
    e.append = (...c) => { e.children.push(...c); };
    e.addEventListener = (n, fn) => { listeners[id + ":" + n] = fn; };
    e.removeEventListener = () => {};
    Object.defineProperty(e, "innerHTML", { get: () => "", set: () => { e.children = []; } });
    return e;
  };
  const els = {};
  const toasts = [];
  const errs = [];
  const sb = {
    $: (id) => (els[id] = els[id] || mkEl(id)),
    invoke: async (cmd) => {
      if (cmd === "video_folder_cleanup_preview") return [{ name: "a.mp4", size: 5 }];
      if (cmd === "list_discarded_recordings") return [];
      throw new Error("cleanup failed");
    },
    errToast: (e) => errs.push(String(e.message || e)), toast: (m) => toasts.push(m),
    t: (k) => k, LANG: "en", fmtBytes: () => "5 B", confirmDialog: async () => true,
    refreshVideoFolderInfo() {}, document: { createElement: () => mkEl("x") }, Number, String,
  };
  vm.createContext(sb);
  vm.runInContext(code + String.fromCharCode(10) + "this.run = openVideoCleanup;", sb);
  later(async () => {
    const done = sb.run("C:/rec");
    await new Promise((r) => setTimeout(r, 20));
    listeners["video-cleanup-confirm:click"]();
    await done;
    check(errs.length === 1, "a failed cleanup shows its error");
    check(!toasts.includes("videoFolderCleanupDone"), "a failed cleanup must not also report zero files deleted");
  });
});

section(() => {
  const ro = read("i18n/ro.js");
  const keys = ["tileFolderNew", "tileFolderNameMsg", "tileFolderPickSecond", "tileFolderNamePh", "tileFolderDefaultName", "tileFolderRename", "tileFolderColor", "tileFolderDissolve", "tileFolderCount", "errFolderNeedsTwo", "errMaxFolders", "errFolderNameTooLong", "errNotInFolder", "errNotFolderMember"];
  for (const k of keys) {
    const m = ro.match(new RegExp("^\\s*" + k + ': "([^"]*)"', "m"));
    check(!!m && !/dosar/i.test(m[1]) && /folder/i.test(m[1]), "Romanian " + k + " uses the folder term");
  }
  const fl = read("i18n/float.js").split(String.fromCharCode(10)).find((l) => /^\s*ro:/.test(l)) || "";
  check(/members: "[^"]*folder/.test(fl) && !/dosar/i.test(fl), "the Romanian pill strings use the folder term");
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
  console.log("satellite window fixes: ok");
});
