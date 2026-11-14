// Screenshot editor text styling (ui/snipEditor.js): a text keeps its selection while the tool
// panel is used, and size, colour, background and outline apply to the text being typed AND to a
// selected text, each change one undo record. Runs the real extracted source in a node vm.
// Run:  node tests/snip_text_style_check.js
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

function classList() {
  const set = new Set(["hidden"]);
  return { add: (c) => set.add(c), remove: (c) => set.delete(c), contains: (c) => set.has(c) };
}

function sandbox() {
  const ts = { color: "#000000", fontPx: 24, back: "none", outlinePx: 3 };
  const sb = {
    ts,
    clock: 1000,
    Date: { now: () => sb.clock },
    SE: { tool: "text", ops: [], redo: [], floor: 0, drag: null, textEdit: null, lastNudged: null, eyedropArm: null },
    snipEditToolState: () => ts,
    snipEditScale: () => 1,
    snipEditTextInput: { value: "", style: {}, classList: classList(), blur() {}, focus() {}, select() {} },
    snipEditBaseCtx: { save() {}, restore() {}, measureText: (s) => ({ width: s.length * 8 }) },
    contrastTextColor: (c) => (c === "#ffffff" ? "#000000" : "#ffffff"),
    redraws: 0, snipEditRedrawBase() { sb.redraws++; },
    snipEditUpdateUndoRedoButtons() {}, snipEditNoteChange() {}, snipEditUpdateNudgeHint() {},
    snipEditShowTextSelection() {},
    snipEditRemoveOp() {},
    snipEditPushOp(op, nudge) { sb.SE.ops.push(op); sb.SE.redo.length = 0; sb.SE.lastNudged = nudge === undefined ? op : nudge; },
    SNIP_CTX_INSIDE: "#snip-edit-context, .snip-color-pop, .color-pop, .select-pop, .numfield-pop",
    SNIP_TEXT_TOOL_BTN: '#snip-edit-tools [data-tool="text"]',
  };
  vm.createContext(sb);
  for (const marker of [
    "function snipEditTextTarget() {",
    "function snipEditSyncTextToolFrom(op) {",
    "function snipEditTextInputStyle(look, scale) {",
    "function snipEditStyleTextInput() {",
    "function snipEditTextToolChanged() {",
    "function snipEditFitTextInput() {",
    "function snipEditCommitTextInput() {",
    "function snipEditOnTextBlur(e) {",
    "function snipEditUndoEdit(e) {",
    "function snipEditRedoEdit(e) {",
  ]) vm.runInContext(extractBlock(marker), sb);
  return sb;
}
const look = (op) => ({ text: op.text, color: op.color, fontPx: op.fontPx, back: op.back, outlinePx: op.outlinePx });
const closest = (hit) => ({ relatedTarget: { closest: (sel) => (hit && sel.includes(hit) ? {} : null) } });

// Blur into the context panel keeps NEW text open, as does a blur onto the text tool button.
{
  const sb = sandbox();
  sb.SE.textEdit = { point: { x: 5, y: 5 } };
  sb.snipEditTextInput.value = "hello";
  sb.snipEditOnTextBlur(closest("#snip-edit-context"));
  check("blur into the panel keeps new text open", !!sb.SE.textEdit, true);
  sb.snipEditOnTextBlur(closest('[data-tool="text"]'));
  check("blur onto the text tool button keeps it open", !!sb.SE.textEdit, true);
  sb.snipEditOnTextBlur({ relatedTarget: null });
  check("blur elsewhere commits", sb.SE.textEdit, null);
  check("blur elsewhere pushed the text", sb.SE.ops.length, 1);
}

// A size change while typing new text commits with the new size and styles the input.
{
  const sb = sandbox();
  sb.SE.textEdit = { point: { x: 5, y: 5 } };
  sb.snipEditTextInput.value = "hello";
  sb.ts.fontPx = 48;
  sb.ts.back = "white";
  sb.snipEditTextToolChanged();
  check("input font follows the size", sb.snipEditTextInput.style.fontSize, "48px");
  check("input background follows the white back", sb.snipEditTextInput.style.background, "#fff");
  sb.snipEditCommitTextInput();
  check("committed text carries the new size", sb.SE.ops[0].fontPx, 48);
  check("committed text carries the background", sb.SE.ops[0].back, "white");
}

// Input look per background: mirrors the renderer.
{
  const sb = sandbox();
  const s = (back, extra) => sb.snipEditTextInputStyle({ fontPx: 20, color: "#112233", back, outlinePx: 4, ...extra }, 1.5);
  check("none: transparent", s("none").background, "transparent");
  check("none: text colour", s("none").color, "#112233");
  check("white: white", s("white").background, "#fff");
  check("plate: the plate colour", s("plate").background, "rgba(0,0,0,0.45)");
  check("outline: transparent", s("outline").background, "transparent");
  check("outline: stroke scaled", s("outline").webkitTextStroke, "6px #ffffff");
  check("font scaled", s("none").fontSize, "30px");
}

// A selected (not edited) text: each of the four fields changes it with one undo record.
for (const [field, value] of [["fontPx", 40], ["color", "#ff0000"], ["back", "plate"], ["outlinePx", 7]]) {
  const sb = sandbox();
  const op = { tool: "text", point: { x: 1, y: 2 }, text: "abc", color: "#000000", fontPx: 24, back: "none", outlinePx: 3 };
  sb.SE.ops = [op];
  sb.SE.lastNudged = op;
  const start = look(op);
  sb.snipEditSyncTextToolFrom(op);
  sb.snipEditTextToolChanged();
  check(`${field}: no change, no record`, sb.SE.ops.length, 1);
  sb.ts[field] = value;
  sb.snipEditTextToolChanged();
  check(`${field}: applied to the op`, op[field], value);
  check(`${field}: exactly one record`, sb.SE.ops.length, 2);
  // Another render pass with the same values must not push again.
  sb.snipEditTextToolChanged();
  check(`${field}: repeated call pushes nothing`, sb.SE.ops.length, 2);
  const rec = sb.SE.ops[1];
  check(`${field}: record kind`, [rec.tool, rec.kind], ["edit", "text"]);
  sb.snipEditUndoEdit(rec);
  check(`${field}: undo restores all four`, look(op), start);
  sb.snipEditRedoEdit(rec);
  check(`${field}: redo applies it again`, op[field], value);
}

// Coalescing: the same field within 800 ms replaces the record's after, later or another field pushes.
{
  const sb = sandbox();
  const op = { tool: "text", point: { x: 1, y: 2 }, text: "abc", color: "#000000", fontPx: 24, back: "none", outlinePx: 3 };
  sb.SE.ops = [op];
  sb.SE.lastNudged = op;
  sb.snipEditSyncTextToolFrom(op);
  sb.ts.fontPx = 30; sb.snipEditTextToolChanged();
  sb.clock += 300; sb.ts.fontPx = 31; sb.snipEditTextToolChanged();
  sb.clock += 300; sb.ts.fontPx = 32; sb.snipEditTextToolChanged();
  check("same field within 800 ms: one record", sb.SE.ops.length, 2);
  check("the record keeps the first before", sb.SE.ops[1].before.fontPx, 24);
  check("the record ends on the last value", sb.SE.ops[1].after.fontPx, 32);
  sb.clock += 900; sb.ts.fontPx = 40; sb.snipEditTextToolChanged();
  check("after 800 ms: a new record", sb.SE.ops.length, 3);
  sb.clock += 100; sb.ts.color = "#00ff00"; sb.snipEditTextToolChanged();
  check("another field: a new record", sb.SE.ops.length, 4);
  sb.snipEditUndoEdit(sb.SE.ops[3]);
  sb.snipEditUndoEdit(sb.SE.ops[2]);
  sb.snipEditUndoEdit(sb.SE.ops[1]);
  check("undoing all restores the start", look(op), { text: "abc", color: "#000000", fontPx: 24, back: "none", outlinePx: 3 });
}

// An op being edited takes the values too, without an own record (the commit makes it).
{
  const sb = sandbox();
  const op = { tool: "text", point: { x: 1, y: 2 }, text: "abc", color: "#000000", fontPx: 24, back: "none", outlinePx: 3, editing: true };
  sb.SE.ops = [op];
  sb.SE.textEdit = { point: op.point, op, before: look(op) };
  sb.snipEditTextInput.value = "abc";
  sb.ts.fontPx = 36; sb.ts.back = "outline"; sb.ts.outlinePx = 5;
  sb.snipEditTextToolChanged();
  check("edited op gets size, back and outline", [op.fontPx, op.back, op.outlinePx], [36, "outline", 5]);
  check("no record while editing", sb.SE.ops.length, 1);
  sb.snipEditCommitTextInput();
  check("the commit pushes one record", sb.SE.ops.length, 2);
  check("the record carries before and after", [sb.SE.ops[1].before.fontPx, sb.SE.ops[1].after.fontPx, sb.SE.ops[1].after.back], [24, 36, "outline"]);
}

// Switching to the text tool with an open edit keeps it (the tool switch itself).
{
  const sb = sandbox();
  sb.SE.tool = "rect";
  Object.assign(sb, {
    snipEditFinishTextIfOpen() { sb.finished = true; },
    closeSnipEditContext() {}, snipEditClearLive() {}, snipEditUpdateNudgeHint() {},
    snipEditWrap: { style: {}, classList: { toggle() {} } },
    snipEditSyncCropFromOps() {}, snipEditRenderToolButtons() {}, snipEditRenderContext() {},
    snipEditOpenContext() {}, snipRememberTool() {}, finished: false,
  });
  vm.runInContext(extractBlock("function snipEditSetTool(tool) {"), sb);
  const op = { tool: "text", point: { x: 1, y: 2 }, text: "abc", color: "#123456", fontPx: 33, back: "plate", outlinePx: 2, editing: true };
  sb.SE.ops = [op];
  sb.SE.textEdit = { point: op.point, op, before: look(op) };
  sb.SE.lastNudged = op;
  sb.snipEditSetTool("text");
  check("text tool with an open edit: edit kept", sb.SE.textEdit && sb.SE.textEdit.op === op, true);
  check("text tool with an open edit: not committed", sb.finished, false);
  check("text tool with an open edit: selection kept", sb.SE.lastNudged === op, true);
  check("tool state synced from the op", look({ text: "", ...sb.ts }).fontPx, 33);
  check("tool state back synced", sb.ts.back, "plate");
  sb.snipEditSetTool("rect");
  check("another tool still commits", sb.finished, true);
  check("another tool clears the selection", sb.SE.lastNudged, null);
}

for (const p of problems) console.log("FAIL", p);
console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
process.exit(problems.length ? 1 : 0);
