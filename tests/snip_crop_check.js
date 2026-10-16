// Screenshot editor crop box (ui/main.js): a shift-drag of a corner handle keeps
// the aspect ratio AND the opposite edges where they are.
//
// main.js is one big module with its own boot code, so nothing can be imported
// from it. This pulls the exact source text of snipEditResizeCropRect out of the
// real file and runs it, so the check fails on whatever the file contains and not
// on a hand-copied reimplementation.
// Run:  node tests/snip_crop_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = require("./ui_sources.js").mainWindowSource().replace(/\r\n/g, "\n");

// "<marker> ... <matching closing brace>", counting braces from the marker's own
// opening one so a nested if/for does not end the extraction early.
function extractBlock(label, marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find ${label} in main.js (source text changed?): ${marker}`);
  const braceStart = src.indexOf("{", start);
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}") {
      depth--;
      if (depth === 0) return src.slice(start, i + 1);
    }
  }
  throw new Error(`unbalanced braces extracting ${label} from main.js`);
}

const sandbox = { SE: { w: 1000, h: 1000 } };
vm.createContext(sandbox);
vm.runInContext(
  extractBlock("snipEditResizeCropRect", "function snipEditResizeCropRect(orig, handle, pt, shift) {"),
  sandbox,
  { filename: "main.js (extracted)" }
);
const resize = (...a) => sandbox.snipEditResizeCropRect(...a);

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

const orig = { x: 0, y: 0, w: 200, h: 100 };

// Top-left corner, shift held: the bottom edge (y 100) and the right edge
// (x 200) stay where they are, the box keeps its 2:1 shape.
{
  const nr = resize(orig, "nw", { x: 50, y: 20 }, true);
  check("nw shift: bottom edge stays", nr.y + nr.h, 100);
  check("nw shift: right edge stays", nr.x + nr.w, 200);
  check("nw shift: ratio kept", nr.w / nr.h, 2);
}
// Top-right corner, same rule: bottom edge and left edge stay.
{
  const nr = resize(orig, "ne", { x: 150, y: 30 }, true);
  check("ne shift: bottom edge stays", nr.y + nr.h, 100);
  check("ne shift: left edge stays", nr.x, 0);
  check("ne shift: ratio kept", nr.w / nr.h, 2);
}
// Dragged past the bottom edge (flipped): the box hangs from the old bottom edge.
{
  const nr = resize(orig, "nw", { x: 50, y: 160 }, true);
  check("nw shift flipped: top edge is the old bottom edge", nr.y, 100);
  check("nw shift flipped: ratio kept", nr.w / nr.h, 2);
}
// Without shift nothing changes from the plain pointer rectangle.
{
  const nr = resize(orig, "nw", { x: 50, y: 20 }, false);
  check("nw plain: follows the pointer", nr, { x: 50, y: 20, w: 150, h: 80 });
}
// A locked ratio against the image border: the box shrinks as a whole, it never
// loses its shape to a one-sided trim.
{
  const edge = { x: 900, y: 400, w: 100, h: 100 };
  const n = resize(edge, "n", { x: 950, y: 200 }, true);
  check("n shift at the right border: still square", n.w, n.h);
  check("n shift at the right border: inside the image", n.x + n.w <= 1000, true);
  check("n shift at the right border: bottom edge stays", n.y + n.h, 500);
  const se = resize({ x: 800, y: 800, w: 100, h: 100 }, "se", { x: 1500, y: 1500 }, true);
  check("se shift past both borders: ratio kept", se.w, se.h);
  check("se shift past both borders: inside the image", se.x + se.w <= 1000 && se.y + se.h <= 1000, true);
}
// The bottom handles were already anchored on the top edge and stay that way.
{
  const nr = resize(orig, "se", { x: 120, y: 80 }, true);
  check("se shift: top edge stays", nr.y, 0);
  check("se shift: left edge stays", nr.x, 0);
}

// The locked ratio never pushes an edge past the image: a 100x100 box at the right
// edge, top handle pulled up with shift, would grow to 200 wide and overhang by 100.
{
  sandbox.SE = { w: 300, h: 200 };
  const nr = resize({ x: 200, y: 0, w: 100, h: 100 }, "n", { x: 250, y: -100 }, true);
  check("n shift at the right edge: stays inside the image", nr, { x: 200, y: 0, w: 100, h: 100 });
}

for (const p of problems) console.log("FAIL", p);
console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
process.exit(problems.length ? 1 : 0);
