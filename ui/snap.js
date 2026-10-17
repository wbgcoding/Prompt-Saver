// Shared drag-snap helper for the screenshot editor and the video editor: one pure
// function, so the two never drift apart.
// | `rect` is the shape being moved: {x, y, w, h} in the caller's coordinate space (image
// pixels, source video pixels). w:0, h:0 treats it as a bare point: near edge, centre and
// far edge collapse onto it, so a corner or text anchor snaps like a rectangle's edges.
// | `targets` is a list of rectangles (or points, same convention) to snap against:
// typically the frame itself plus every other shape or overlay. Each contributes three
// candidate lines per axis (near edge, centre, far edge).
// | Returns the corrected {x, y} for `rect`, plus guideX/guideY: the matched target
// line's coordinate on that axis (null when that axis did not snap), for drawing the
// temporary guide line while dragging.
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

// One axis at a time: of the three candidate points on the moving side and every
// candidate line on the target side, pick the closest pair within tolerance, so two
// possible targets resolve to the nearer one.
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
