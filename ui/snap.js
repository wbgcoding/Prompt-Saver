// Shared drag-snap helper for the screenshot editor and the video editor.
// The screenshot editor used to snap only to the four canvas edges and its
// centre; the video editor did not snap at all. Rather than write the video
// editor its own copy (and let the two drift apart the way earlier duplicated
// logic between the two editors already had for other things), both call
// this one pure function.
//
// `rect` is the shape currently being moved: {x, y, w, h} in whatever
// coordinate space the caller uses (image pixels for the screenshot editor,
// source video pixels for the video editor). Passing w:0, h:0 treats it as a
// bare point instead of a rectangle -- the near edge, centre and far edge all
// collapse onto the point itself, so a shape corner or a text anchor snaps
// the same way a rectangle's edges do.
//
// `targets` is a list of rectangles (or points, same w:0/h:0 convention) to
// snap against: typically the canvas/frame itself plus every other shape or
// overlay. Each target contributes three candidate lines per axis (near
// edge, centre, far edge).
//
// Returns the corrected {x, y} for `rect`, plus guideX/guideY: the matched
// target line's coordinate on that axis (or null when that axis did not
// snap), for drawing the temporary accent-coloured guide line while dragging.
export function snapMove(rect, targets, tol) {
  const linesX = [];
  const linesY = [];
  for (const t of targets) {
    const tw = t.w || 0, th = t.h || 0;
    linesX.push(t.x, t.x + tw / 2, t.x + tw);
    linesY.push(t.y, t.y + th / 2, t.y + th);
  }
  const x = snapAxis(rect.x, rect.w || 0, linesX, tol);
  const y = snapAxis(rect.y, rect.h || 0, linesY, tol);
  return { x: x.value, y: y.value, guideX: x.guide, guideY: y.guide };
}

// One axis at a time: of the three candidate points on the moving side (near
// edge, centre, far edge) and every candidate line on the target side, pick
// whichever pair is closest, as long as it is within tolerance -- this is
// what makes two possible targets resolve to the nearer one.
function snapAxis(pos, size, lines, tol) {
  const offsets = size ? [0, size / 2, size] : [0];
  let best = null;
  for (const line of lines) {
    for (const offset of offsets) {
      const delta = line - (pos + offset);
      const dist = Math.abs(delta);
      if (dist <= tol && (!best || dist < best.dist)) best = { dist, delta, line };
    }
  }
  if (!best) return { value: pos, guide: null };
  return { value: pos + best.delta, guide: best.line };
}
