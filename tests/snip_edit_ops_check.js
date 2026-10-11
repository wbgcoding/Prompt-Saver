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
    NUM_FIELD_BIG_STEP: 10, snipOriginalToggle: { press() {} }, snipEditWrap: { style: {}, classList: { add() {} } },
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
