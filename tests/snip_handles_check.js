// Screenshot editor handles (ui/snipEditor.js): the square handles of the crop box and of a
// selected rect/ellipse lie fully OUTSIDE the frame and touch it (never clamped into the picture),
// line/arrow end points stay centred on the point, and every handle press, whether it landed on
// the handle element or on the image next to it, starts the matching drag through one function.
// Runs the real extracted source in a node vm. Run:  node tests/snip_handles_check.js
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

const sb = {
  SE: { w: 1000, h: 800, cropRect: null, selOp: null, lastNudged: null, drag: null },
  hints: 0,
  snipEditUpdateNudgeHint() { sb.hints++; },
};
vm.createContext(sb);
vm.runInContext("const SNIP_HANDLE_PX = 10;", sb);
for (const marker of [
  "function snipEditCropHandlePoints(r) {",
  "function snipEditHandleBoxes(points, rect, scale, outward) {",
  "function snipEditStartHandleDrag(kind, handle, pt) {",
]) vm.runInContext(extractBlock(marker), sb);

const NAMES = ["nw", "n", "ne", "w", "e", "sw", "s", "se"];
const outside = (b, r) => b.x + b.w <= r.x || b.x >= r.x + r.w || b.y + b.h <= r.y || b.y >= r.y + r.h;
const touches = (b, r) => b.x <= r.x + r.w && b.x + b.w >= r.x && b.y <= r.y + r.h && b.y + b.h >= r.y;
const close = (a, b) => Math.abs(a - b) < 1e-9;

// Frame handles: fully outside, touching, 10 CSS px big, mid handles centred on their edge.
for (const scale of [1, 1.5]) {
  for (const r of [{ x: 0, y: 0, w: 1000, h: 800 }, { x: 120, y: 90, w: 300, h: 200 }, { x: 700, y: 600, w: 300, h: 200 }]) {
    const boxes = sb.snipEditHandleBoxes(sb.snipEditCropHandlePoints(r), r, scale, true);
    check(`scale ${scale} ${JSON.stringify(r)}: eight handles`, Object.keys(boxes).sort(), NAMES.slice().sort());
    for (const n of NAMES) {
      const b = boxes[n];
      const tag = `scale ${scale} ${JSON.stringify(r)} ${n}`;
      check(`${tag}: lies outside the frame`, outside(b, r), true);
      check(`${tag}: touches the frame`, touches(b, r), true);
      check(`${tag}: 10 CSS px`, close(b.w * scale, 10) && close(b.h * scale, 10), true);
    }
    check(`scale ${scale}: n centred on the top edge`, close(boxes.n.x + boxes.n.w / 2, r.x + r.w / 2), true);
    check(`scale ${scale}: e centred on the right edge`, close(boxes.e.y + boxes.e.h / 2, r.y + r.h / 2), true);
    check(`scale ${scale}: nw sits beyond the corner`, boxes.nw.x < r.x && boxes.nw.y < r.y, true);
    check(`scale ${scale}: se sits beyond the corner`, boxes.se.x >= r.x + r.w && boxes.se.y >= r.y + r.h, true);
  }
}
// Line/arrow end points stay centred on the point.
{
  const pts = { a: { x: 40, y: 50 }, b: { x: 300, y: 10 } };
  const boxes = sb.snipEditHandleBoxes(pts, null, 1.5, false);
  for (const n of ["a", "b"]) {
    const b = boxes[n];
    check(`end point ${n}: centred`, close(b.x + b.w / 2, pts[n].x) && close(b.y + b.h / 2, pts[n].y), true);
  }
}

// The shared press: every handle name starts the matching drag.
for (const n of NAMES) {
  sb.SE.drag = null;
  sb.SE.cropRect = { x: 10, y: 20, w: 300, h: 200 };
  check(`crop press ${n}`, sb.snipEditStartHandleDrag("crop-resize", n, { x: 0, y: 0 }), true);
  check(`crop press ${n}: drag`, sb.SE.drag, { kind: "crop-resize", handle: n, orig: { x: 10, y: 20, w: 300, h: 200 } });
  check(`crop press ${n}: orig is a copy`, sb.SE.drag.orig !== sb.SE.cropRect, true);

  sb.SE.drag = null;
  const op = { tool: "rect", rect: { x: 5, y: 6, w: 70, h: 80 } };
  sb.SE.selOp = op;
  check(`shape press ${n}`, sb.snipEditStartHandleDrag("shape-resize", n, { x: 0, y: 0 }), true);
  check(`shape press ${n}: drag`, sb.SE.drag, { kind: "shape-resize", op, handle: n, orig: { x: 5, y: 6, w: 70, h: 80 } });
  check(`shape press ${n}: op stays the nudge target`, sb.SE.lastNudged === op, true);
}
for (const n of ["a", "b"]) {
  sb.SE.drag = null;
  const op = { tool: "arrow", points: [{ x: 1, y: 2 }, { x: 30, y: 40 }] };
  sb.SE.selOp = op;
  check(`endpoint press ${n}`, sb.snipEditStartHandleDrag("shape-endpoint", n, { x: 0, y: 0 }), true);
  check(`endpoint press ${n}: drag`, sb.SE.drag, { kind: "shape-endpoint", op, handle: n, orig: [{ x: 1, y: 2 }, { x: 30, y: 40 }] });
}
// Nothing to drag: no drag starts.
{
  sb.SE.drag = null; sb.SE.selOp = null; sb.SE.cropRect = null;
  check("no selection: shape press refused", sb.snipEditStartHandleDrag("shape-resize", "n", { x: 0, y: 0 }), false);
  check("no crop box: crop press refused", sb.snipEditStartHandleDrag("crop-resize", "n", { x: 0, y: 0 }), false);
  check("refused presses leave no drag", sb.SE.drag, null);
}

// The crop tool's hint: shown from the start, gone after the first drag, back on a re-click.
{
  const hidden = new Set();
  const hintText = {
    dataset: {}, textContent: "",
    classList: { toggle: (c, on) => { if (on) hidden.add(c); else hidden.delete(c); } },
  };
  const hsb = {
    SE: { tool: "crop", cropHintDismissed: false, lastNudged: null, cropRect: { x: 0, y: 0, w: 100, h: 100 }, drag: null },
    snipEditNudgeHintEl: { firstElementChild: hintText },
    t: (k) => k,
  };
  vm.createContext(hsb);
  vm.runInContext("const SNIP_HANDLE_PX = 10;", hsb);
  for (const marker of ["function snipEditUpdateNudgeHint() {", "function snipEditStartHandleDrag(kind, handle, pt) {"]) {
    vm.runInContext(extractBlock(marker), hsb);
  }
  hsb.snipEditUpdateNudgeHint();
  check("crop hint: shown on a fresh crop tool", [hidden.has("hidden"), hintText.textContent], [false, "snipCropHint"]);
  hsb.snipEditStartHandleDrag("crop-resize", "e", { x: 0, y: 0 });
  check("crop hint: gone after the first drag", hidden.has("hidden"), true);
  hsb.SE.cropHintDismissed = false;
  hsb.snipEditUpdateNudgeHint();
  check("crop hint: back after a reset", [hidden.has("hidden"), hintText.textContent], [false, "snipCropHint"]);
  hsb.SE.tool = "rect";
  hsb.snipEditUpdateNudgeHint();
  check("crop hint: not shown for another tool", hidden.has("hidden"), true);
}

for (const p of problems) console.log("FAIL", p);
console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
process.exit(problems.length ? 1 : 0);
