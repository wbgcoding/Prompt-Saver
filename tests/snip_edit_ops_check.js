// Screenshot editor undo floor and keyboard routing (ui/snipEditor.js): replacing or
// discarding a crop/spotlight only shifts the undo floor for an entry that sat BELOW
// it, a nudge on top of an undone branch ends the redo branch, and typing in a field
// inside the editor never triggers a tool shortcut.
// Runs the real extracted source in a node vm. Run:  node tests/snip_edit_ops_check.js
const vm = require("vm");
const src = require("./ui_sources.js").mainWindowSource().replace(/\r\n/g, "\n");

function extractBlock(marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find ${marker} (source text changed?)`);
  const braceStart = src.indexOf("{", start);
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces extracting ${marker}`);
}

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

function sandbox() {
  const sb = {
    SE: { ops: [], redo: [], floor: 0, tool: "rect", cropRect: null, drag: null },
    tools: [],
    snipEditPushOp(op) { sb.SE.ops.push(op); },
    snipEditUpdateUndoRedoButtons() {}, snipEditClearLive() {}, snipEditRedrawBase() {},
    snipEditFit() {}, snipEditDrawCropOverlayLive() {}, snipEditFullCropRect: () => ({ x: 0, y: 0, w: 10, h: 10 }),
    snipEditSetTool(id) { sb.tools.push(id); },
    SNIP_TOOLS: [{ id: "rect", key: "r" }],
    snipEdit: { classList: { contains: () => false } },
    snipCtxPop: { classList: { contains: () => true } },
    snipEditTextInput: {}, document: { activeElement: null },
    $: () => ({ classList: { contains: () => true } }),
    snipEditContextIsOpen: () => false, snipColorPopIsOpen: () => false,
    snipEditDrawShapeSelectionLive() {}, snipEditUpdateNudgeHint() {},
    scalePopOpen: false, snipScalePopIsOpen() { return sb.scalePopOpen; }, closeSnipScalePop() { sb.scalePopOpen = false; sb.scaleClosed = true; },
    numPopEsc: false, numPopClosedByEscape: () => sb.numPopEsc,
    colorPop: { classList: { contains: () => true } }, closeColorPop() {},
    SNIP_EDIT_OVER_DIALOGS: ["ocr-modal", "ocr-langs-modal", "modal"],
    applied: false, snipEditApply() { sb.applied = true; }, pressed: 0,
    NUM_FIELD_BIG_STEP: 10, snipOriginalToggle: { press() { sb.pressed++; } }, snipEditWrap: { style: {}, classList: { add() {} } },
  };
  vm.createContext(sb);
  vm.runInContext(extractBlock("function snipEditPushUniqueOp(tool, op) {"), sb);
  vm.runInContext(extractBlock("function snipEditCropDiscard() {"), sb);
  vm.runInContext(extractBlock("function snipEditFieldOwnsKeys(tgt) {"), sb);
  vm.runInContext(extractBlock("function snipEditKeydown(e) {"), sb);
  vm.runInContext(extractBlock("function snipEditNudge(op, key, amount, resize) {"), sb);
  vm.runInContext(extractBlock("function snipEditGeom(op) {"), sb);
  vm.runInContext(extractBlock("function snipEditExportDataUrl() {"), sb);
  return sb;
}

// A crop above the floor leaving must not raise the floor over an undoable op.
{
  const sb = sandbox();
  sb.SE.ops = [{ tool: "rect" }, { tool: "rect" }, { tool: "crop", rect: {} }];
  sb.SE.floor = 1;
  sb.snipEditPushUniqueOp("crop", { tool: "crop", rect: {} });
  check("replace crop above the floor: floor kept", sb.SE.floor, 1);
}
// A crop below the floor leaving shifts it down by one.
{
  const sb = sandbox();
  sb.SE.ops = [{ tool: "crop", rect: {} }, { tool: "rect" }, { tool: "rect" }];
  sb.SE.floor = 2;
  sb.snipEditPushUniqueOp("crop", { tool: "crop", rect: {} });
  check("replace crop below the floor: floor shifts", sb.SE.floor, 1);
}
{
  const sb = sandbox();
  sb.SE.ops = [{ tool: "rect" }, { tool: "rect" }, { tool: "crop", rect: {} }];
  sb.SE.floor = 2;
  sb.snipEditCropDiscard();
  check("discard crop above the floor: floor kept", sb.SE.floor, 2);
  check("discard crop above the floor: op gone", sb.SE.ops.length, 2);
}
{
  const sb = sandbox();
  sb.SE.ops = [{ tool: "crop", rect: {} }, { tool: "rect" }];
  sb.SE.floor = 1;
  sb.snipEditCropDiscard();
  check("discard crop below the floor: floor shifts", sb.SE.floor, 0);
}
// Discarding the crop detaches its op: the arrow keys must not nudge it back to life.
{
  const sb = sandbox();
  const crop = { tool: "crop", rect: { x: 1, y: 1, w: 5, h: 5 } };
  sb.SE.ops = [crop];
  sb.SE.lastNudged = crop;
  sb.snipEditCropDiscard();
  check("discard crop: nudge target cleared", sb.SE.lastNudged, null);
  const shape = { tool: "rect", rect: {} };
  sb.SE.ops = [shape, { tool: "crop", rect: {} }];
  sb.SE.lastNudged = shape;
  sb.snipEditCropDiscard();
  check("discard crop: a shape's nudge target stays", sb.SE.lastNudged === shape, true);
}
// Escape closes the resize popover first, and leaves the editor open.
{
  const sb = sandbox();
  sb.scalePopOpen = true;
  sb.snipEditKeydown({ key: "Escape", target: { tagName: "BODY" }, preventDefault() {}, ctrlKey: false, metaKey: false, repeat: false });
  check("Escape closes the resize popover", sb.scaleClosed, true);
}
// Escape on the canvas is Close (applies and closes); while a text recognition or other dialog
// sits over the editor, the editor ignores the key so the dialog gets it.
{
  const sb = sandbox();
  const esc = { key: "Escape", target: { tagName: "BODY" }, preventDefault() {}, ctrlKey: false, metaKey: false, repeat: false };
  sb.$ = (id) => ({ classList: { contains: () => id !== "ocr-modal" } });
  sb.snipEditKeydown(esc);
  check("Escape with a dialog over the editor: the editor does nothing", sb.applied, false);
  sb.$ = () => ({ classList: { contains: () => true } });
  sb.snipEditKeydown(esc);
  check("Escape on the canvas closes the editor", sb.applied, true);
}
// Keys typed in a field never switch tools; the same key on the canvas does.
{
  const sb = sandbox();
  sb.snipCtxPop.classList.contains = () => true;
  const ev = (target) => ({ key: "r", target, preventDefault() {}, ctrlKey: false, metaKey: false, repeat: false });
  sb.snipEditKeydown(ev({ tagName: "INPUT" }));
  sb.snipEditKeydown(ev({ tagName: "SELECT" }));
  sb.snipEditKeydown(ev({ tagName: "DIV", isContentEditable: true }));
  check("typing in a field: no tool switch", sb.tools, []);
  sb.snipEditKeydown(ev({ tagName: "BODY" }));
  check("canvas key: tool switches", sb.tools, ["rect"]);
}

// The editor's own toggles and sliders keep the focus after a click: undo and the
// tool letters still work there, while a text field keeps them.
{
  const sb = sandbox();
  const ev = (target, key = "r", ctrlKey = false) => ({ key, target, preventDefault() {}, ctrlKey, metaKey: false, repeat: false });
  sb.snipEditKeydown(ev({ tagName: "INPUT", type: "range" }));
  sb.snipEditKeydown(ev({ tagName: "INPUT", type: "checkbox" }));
  check("focused slider or toggle: tool switches", sb.tools, ["rect", "rect"]);
  sb.snipEditKeydown(ev({ tagName: "INPUT", type: "text" }));
  sb.snipEditKeydown(ev({ tagName: "INPUT", type: "number" }));
  sb.snipEditKeydown(ev({ tagName: "TEXTAREA" }));
  check("text entry keeps its keys", sb.tools, ["rect", "rect"]);
}
// An Escape that only closed a preset list does nothing else; Enter on a button reached
// by keyboard presses that button instead of applying; Ctrl+V is no original toggle.
{
  const sb = sandbox();
  sb.scalePopOpen = true;
  sb.numPopEsc = true;
  sb.snipEditKeydown({ key: "Escape", target: { tagName: "BODY" }, preventDefault() {}, ctrlKey: false, metaKey: false, repeat: false });
  check("Escape spent on a preset list: popover stays", sb.scaleClosed, undefined);
  const ev = (target, key, extra = {}) => ({ key, target, preventDefault() {}, ctrlKey: false, metaKey: false, altKey: false, repeat: false, ...extra });
  sb.snipEditKeydown(ev({ tagName: "BUTTON", matches: () => true }, "Enter"));
  check("Enter on a keyboard-focused button: no apply", sb.applied, false);
  sb.snipEditKeydown(ev({ tagName: "BUTTON", matches: () => false }, "Enter"));
  check("Enter after a mouse click on a button: applies", sb.applied, true);
  sb.snipEditKeydown(ev({ tagName: "BODY" }, "v", { ctrlKey: true }));
  check("Ctrl+V: no original toggle", sb.pressed, 0);
  sb.snipEditKeydown(ev({ tagName: "BODY" }, "v"));
  check("plain V: original toggle", sb.pressed, 1);
}
// A nudge folded into the entry below the top is a new edit: it ends the redo branch.
{
  const sb = sandbox();
  const op = { tool: "rect", rect: { x: 1, y: 1, w: 10, h: 10 } };
  sb.SE.ops = [op, { tool: "edit", kind: "geom", op, before: {}, after: {}, nudgeOf: op, nudgeTool: "rect" }];
  sb.SE.tool = "rect";
  sb.SE.lastNudged = op;
  sb.SE.redo = [{ tool: "rect" }];
  sb.snipEditNudge(op, "ArrowRight", 1, false);
  check("merged nudge: rect moved", op.rect.x, 2);
  check("merged nudge: redo branch ended", sb.SE.redo.length, 0);
  check("merged nudge: still one undo entry", sb.SE.ops.length, 2);
}
// Export draws the cropped image from the bare base, never the dimmed one.
{
  const sb = sandbox();
  const calls = [];
  sb.snipEditRedrawBase = (bare) => calls.push(bare ? "bare" : "dimmed");
  sb.SE.ops = [{ tool: "crop", rect: { x: 0, y: 0, w: 20, h: 20 } }];
  sb.SE.w = 100; sb.SE.h = 100; sb.SE.tool = "rect";
  sb.snipEditClamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  sb.snipOriginalToggle = { reset() {} };
  sb.snipEditBaseCv = {};
  sb.document = { activeElement: null, createElement: () => ({ getContext: () => ({ drawImage: () => calls.push("draw") }), toDataURL: () => "data:" }) };
  sb.snipEditExportDataUrl();
  check("export crop: bare redraw before the draw, dimmed one after", calls, ["bare", "draw", "dimmed"]);
}

for (const p of problems) console.log("FAIL", p);
console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
process.exit(problems.length ? 1 : 0);
