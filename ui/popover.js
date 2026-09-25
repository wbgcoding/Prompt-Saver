// ---- ONE placement rule for every popover, dropdown and context menu in the
// main window ---- Below the anchor when it fits, above it when it does not, and
// always clamped into the window on both axes -- a popover that opens past an
// edge is unreachable, and that was the same defect in the theme editor, the
// value pickers and the colour row. Call it AFTER the content is built: the size
// has to be measurable.
//
// Main-window only: floating.js/folder.js run in small frameless windows that
// can be smaller than their own popover, so they clamp against the SCREEN
// (screen.availLeft/availWidth/...) with their own four-way flip logic instead
// of the window -- a genuinely different reference frame, not just a copy that
// drifted. media.js's volume popup is a third case again: a small fixed-size
// slider pinned by CSS relative to its own button, flipped with one class
// toggle -- it never needs horizontal clamping or scrolling, so folding it onto
// this pixel-computed helper would be a rewrite for its own sake, not a fix.
let POP_PAD = 6; // expert-tunable, see setPopoverPad()

export function setPopoverPad(px) {
  POP_PAD = px;
}

export function placePopover(el, rect, gap = 4) {
  el.classList.remove("hidden"); // a hidden element has no box to measure
  el.style.maxHeight = "";
  el.classList.remove("pop-clamped");
  const w = el.offsetWidth;
  const h = el.offsetHeight;
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const below = vh - rect.bottom - gap - POP_PAD;
  const above = rect.top - gap - POP_PAD;
  let top;
  let room;
  if (h <= below || below >= above) { top = rect.bottom + gap; room = below; }
  else { top = rect.top - gap - Math.min(h, above); room = above; }
  // Too tall for either side: keep it inside and let it scroll rather than cut it.
  // Everything below reasons about the height it will ACTUALLY have: clamping with
  // the unshrunk height would drag a tall popover to the top of the window and over
  // the very thing it belongs to.
  let shown = h;
  if (h > room) {
    shown = Math.max(80, Math.min(room, vh - 2 * POP_PAD));
    el.style.maxHeight = `${shown}px`;
    el.classList.add("pop-clamped");
    if (top < POP_PAD) top = POP_PAD;
  }
  el.style.left = `${Math.max(POP_PAD, Math.min(rect.left, vw - w - POP_PAD))}px`;
  el.style.top = `${Math.max(POP_PAD, Math.min(top, vh - shown - POP_PAD))}px`;
}

// Cursor-anchored variant (context menus): the click point is the anchor.
export const placePopoverAt = (el, x, y) =>
  placePopover(el, { left: x, right: x, top: y, bottom: y });
