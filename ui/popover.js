// ONE placement rule for every popover, dropdown and context menu in the main window:
// below the anchor when it fits, above it when it does not, always clamped into the
// window on both axes (a popover past an edge is unreachable). Call it AFTER the content
// is built: the size has to be measurable.
// | Main-window only: floating.js/folder.js run in small frameless windows that can be
// smaller than their own popover, so they clamp against the SCREEN
// (screen.availLeft/availWidth/...) with their own four-way flip logic. media.js's volume
// popup is a small fixed-size slider pinned by CSS to its button and flipped by one class
// toggle; it needs no clamping.
const POP_PAD = 6; // minimum gap between a popover and the window edge (px)

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
  // Everything below uses the height it will ACTUALLY have: clamping with the unshrunk
  // height would drag a tall popover to the top of the window and over its anchor.
  let shown = h;
  // A menu opened at a point (a right-click) has no anchor to keep uncovered, so one that
  // fits the window at all is moved up to fit, the way every system menu is, instead of
  // being squeezed into a scroller with its last entries out of sight.
  const atPoint = rect.top === rect.bottom;
  if (h > room && !(atPoint && h <= vh - 2 * POP_PAD)) {
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
