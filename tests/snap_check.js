// Shared drag-snap helper checks (ui/snap.js: snapMove), used by both the
// screenshot editor and the video editor while dragging a shape/overlay.
// Run:  node tests/snap_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { snapMove } = require(path.join(__dirname, "..", "ui", "snap.js"));

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

// A 100x100 canvas is the one target; a small rectangle just inside the
// tolerance of the left edge snaps its left edge exactly onto it. Its y
// position (30, height 8) is kept clear of every canvas line (0/50/100) so
// only the x axis is expected to move.
{
  const canvas = { x: 0, y: 0, w: 100, h: 100 };
  const r = snapMove({ x: 4, y: 30, w: 20, h: 8 }, [canvas], 6);
  check("snaps to the near edge within tolerance", { x: r.x, y: r.y }, { x: 0, y: 30 });
  check("reports the matched line for the guide", r.guideX, 0);
}

// The same rectangle, moved out past the tolerance, is left untouched.
{
  const canvas = { x: 0, y: 0, w: 100, h: 100 };
  const r = snapMove({ x: 10, y: 30, w: 20, h: 8 }, [canvas], 6);
  check("does not snap beyond tolerance", { x: r.x, y: r.y }, { x: 10, y: 30 });
  check("no guide line when nothing snapped", r.guideX, null);
}

// A rectangle whose horizontal center lands near the canvas's own center
// snaps there too, not just at its edges.
{
  const canvas = { x: 0, y: 0, w: 100, h: 100 };
  const r = snapMove({ x: 48, y: 40, w: 6, h: 10 }, [canvas], 4);
  check("snaps a shape's center to the canvas center", r.x, 47); // centers rect on x=50
}

// Two candidate targets in range on the same axis: the nearer one wins.
{
  const near = { x: 30, y: 0, w: 0, h: 0 };
  const far = { x: 40, y: 0, w: 0, h: 0 };
  const r = snapMove({ x: 33, y: 0, w: 0, h: 0 }, [near, far], 10);
  check("prefers the nearer of two possible targets", r.x, 30);
}

// A bare point (w:0, h:0) degrades to snapping the point itself, no edges or
// center to consider -- this is what the screenshot editor's shape-creation
// drag uses, since a new shape has no size yet while it is being drawn.
{
  const other = { x: 20, y: 20, w: 40, h: 10 }; // edges at 20/40/60, center 40/25
  const r = snapMove({ x: 39, y: 20, w: 0, h: 0 }, [other], 3);
  check("a bare point snaps to another shape's center", { x: r.x, y: r.y }, { x: 40, y: 20 });
}

if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log("PASS");
}
