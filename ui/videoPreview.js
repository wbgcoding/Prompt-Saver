// Video editor: the preview rectangles (crop, overlays) and zooming and panning the
// preview.
import { contrastTextColor } from "./floatShared.js";
import { snapMove } from "./snap.js";
import { clamp, convertFileSrc, even, FONT_CSS, NUM_FIELD_BIG_STEP, opOf, opsOf, outMs, overlayStack, pushCrop, pushReplacing, RECT_MIN_PX, t, VE, videoEl } from "./video.js";

// Own lookup: this module wires its listeners while it loads, before video.js has run.
const $ = (id) => document.getElementById(id);

// ---------------------------------------------------------------- preview rectangles

// The video is letterboxed inside its element, so every rectangle is placed
// against the painted picture, not the element box. One helper, used by both the
// drawing and the hit-testing side, keeps the two from ever disagreeing.
export function videoBox() {
  const stage = $("ve-stage").getBoundingClientRect();
  const box = $("ve-video").getBoundingClientRect();
  const w = VE.w || 16, h = VE.h || 9;
  const scale = Math.min(box.width / w, box.height / h) || 1;
  const pw = w * scale, ph = h * scale;
  return {
    left: box.left - stage.left + (box.width - pw) / 2,
    top: box.top - stage.top + (box.height - ph) / 2,
    clientLeft: box.left + (box.width - pw) / 2,
    clientTop: box.top + (box.height - ph) / 2,
    width: pw,
    height: ph,
    scale,
  };
}

const toSource = (clientX, clientY) => {
  const vb = videoBox();
  return { x: (clientX - vb.clientLeft) / vb.scale, y: (clientY - vb.clientTop) / vb.scale };
};

// ---------------------------------------------------------------- preview zoom/pan
//
// A plain CSS transform on the <video> element. videoBox()/toSource() need no changes: both
// read the video element's live bounding rect, which reflects the transform, so crop/overlay
// rectangles stay placed and draggable at any zoom/pan. 1 = fit-to-stage and the floor:
// shrinking further would only add letterboxing.
const PREVIEW_ZOOM_MIN = 1;
const PREVIEW_ZOOM_MAX = 8;

// The video's own painted size at zoom 1, straight from the stage + source
// aspect ratio: kept independent of the live (already zoomed) bounding rect
// so panning can be clamped without having to divide that rect back down.
export function fitBoxSize() {
  const stage = $("ve-stage").getBoundingClientRect();
  const w = VE.w || 16, h = VE.h || 9;
  const scale = Math.min(stage.width / w, stage.height / h) || 1;
  return { w: w * scale, h: h * scale, stageW: stage.width, stageH: stage.height };
}

// Any point of the picture may be panned up to the middle of the stage, so
// corners and edges can be looked at head-on instead of only at the border.
// Nothing to pan at zoom 1.
export function clampPan(pan, zoom) {
  if (zoom <= 1) return { x: 0, y: 0 };
  const { w, h } = fitBoxSize();
  const maxX = (w * zoom) / 2, maxY = (h * zoom) / 2;
  return { x: clamp(pan.x, -maxX, maxX), y: clamp(pan.y, -maxY, maxY) };
}

export function applyPreviewTransform() {
  const zoomed = VE.previewZoom !== 1 || VE.panPx.x || VE.panPx.y;
  $("ve-video").style.transform = zoomed ? `translate(${VE.panPx.x}px, ${VE.panPx.y}px) scale(${VE.previewZoom})` : "";
  $("ve-stage").classList.toggle("ve-zoomed", VE.previewZoom !== 1);
  const pill = $("ve-preview-zoom");
  if (pill) {
    pill.classList.toggle("hidden", VE.previewZoom === 1);
    $("ve-preview-zoom-val").textContent = `${Math.round(VE.previewZoom * 100)}%`;
  }
  renderRects(); // rectangle overlays are positioned from the same live rect
}

export function setPreviewZoom(z) {
  VE.previewZoom = clamp(z, PREVIEW_ZOOM_MIN, VE.zoomMax);
  VE.panPx = clampPan(VE.panPx, VE.previewZoom);
  applyPreviewTransform();
}

export function resetPreviewZoom() {
  VE.previewZoom = 1;
  VE.panPx = { x: 0, y: 0 };
  applyPreviewTransform();
}

// Plain wheel, no modifier: the stage never scrolls on its own, so the
// gesture is free to mean zoom (unlike the timeline, which needs Ctrl+wheel
// because a plain wheel already scrolls it sideways).
$("ve-stage").addEventListener("wheel", (e) => {
  if (!VE.w) return;
  e.preventDefault();
  // Keep the source point under the cursor fixed instead of zooming to the stage centre: read it
  // before and after the zoom (toSource() reflects the live transformed rect) and slide panPx by
  // the difference, converted via the new live scale. panPx is a CSS translate applied after the
  // scale, so it moves the picture in already-scaled screen pixels.
  const before = toSource(e.clientX, e.clientY);
  setPreviewZoom(VE.previewZoom * (e.deltaY < 0 ? VE.zoomStepFactor : 1 / VE.zoomStepFactor));
  const after = toSource(e.clientX, e.clientY);
  const scale = videoBox().scale;
  VE.panPx = clampPan({
    x: VE.panPx.x + (after.x - before.x) * scale,
    y: VE.panPx.y + (after.y - before.y) * scale,
  }, VE.previewZoom);
  applyPreviewTransform();
}, { passive: false });

// Pan: middle-click always (as in the screenshot editor), plain left-drag only once zoomed in,
// so at zoom 1 an empty-stage drag arms no useless grab cursor. Space is Play/Pause in this
// window, so it is not a pan modifier. A click on a crop/overlay rectangle is left to
// $("ve-rects")'s own pointerdown.
$("ve-stage").addEventListener("pointerdown", (e) => {
  if (!VE.w || (e.button !== 0 && e.button !== 1)) return;
  // A second mouse button pressed during a rect/trim/select/scrub drag fires another pointerdown
  // for the same pointer; without this guard it would steal capture and overwrite that drag.
  if (VE.drag) return;
  if (e.target.closest(".ve-rect")) return;
  if (e.target.closest("#ve-preview-zoom")) return; // the zoom pill/reset button sits inside the stage
  if (VE.previewZoom <= 1) return; // nothing to pan at 1:1, whichever button
  e.preventDefault();
  $("ve-stage").setPointerCapture(e.pointerId);
  VE.drag = { kind: "pan", x: e.clientX, y: e.clientY, start: { ...VE.panPx } };
  $("ve-stage").classList.add("ve-panning");
});
$("ve-stage").addEventListener("pointermove", (e) => {
  const drag = VE.drag;
  if (!drag || drag.kind !== "pan") return;
  VE.panPx = clampPan({ x: drag.start.x + (e.clientX - drag.x), y: drag.start.y + (e.clientY - drag.y) }, VE.previewZoom);
  applyPreviewTransform();
});
function endPreviewPan() {
  if (VE.drag && VE.drag.kind === "pan") VE.drag = null;
  $("ve-stage").classList.remove("ve-panning");
}
$("ve-stage").addEventListener("pointerup", endPreviewPan);
$("ve-stage").addEventListener("pointercancel", endPreviewPan);
$("ve-preview-zoom-reset").addEventListener("click", resetPreviewZoom);
$("ve-preview-zoom-in").addEventListener("click", () => setPreviewZoom(VE.previewZoom * VE.zoomStepFactor));
$("ve-preview-zoom-out").addEventListener("click", () => setPreviewZoom(VE.previewZoom / VE.zoomStepFactor));

// Thin accent-coloured lines shown only while a rect/overlay drag is snapped to an edge or
// centre; created once and toggled hidden, like the screenshot editor's own guides.
const veGuideX = document.createElement("div");
veGuideX.className = "ve-snap-guide ve-snap-guide-x hidden";
const veGuideY = document.createElement("div");
veGuideY.className = "ve-snap-guide ve-snap-guide-y hidden";
$("ve-rects").after(veGuideX, veGuideY);

function renderSnapGuides(vb) {
  const box = vb || videoBox();
  if (VE.guide && VE.guide.x != null) {
    veGuideX.style.left = `${box.left + VE.guide.x * box.scale}px`;
    veGuideX.style.top = `${box.top}px`;
    veGuideX.style.height = `${box.height}px`;
    veGuideX.classList.remove("hidden");
  } else {
    veGuideX.classList.add("hidden");
  }
  if (VE.guide && VE.guide.y != null) {
    veGuideY.style.top = `${box.top + VE.guide.y * box.scale}px`;
    veGuideY.style.left = `${box.left}px`;
    veGuideY.style.width = `${box.width}px`;
    veGuideY.classList.remove("hidden");
  } else {
    veGuideY.classList.add("hidden");
  }
}

// Rectangles snap to even numbers while dragged, so the numbers on screen are the numbers
// ffmpeg gets.
export function snapRect(rect) {
  // Clamping an even width/height against an odd source dimension can land on an odd number
  // (even(641)=642, clamped to 641), but ffmpeg's yuv420p output needs both even, so the
  // ceiling itself is made even first.
  const wMax = Math.max(2, VE.w - (VE.w % 2));
  const hMax = Math.max(2, VE.h - (VE.h % 2));
  const w = clamp(even(rect.w), 2, wMax);
  const h = clamp(even(rect.h), 2, hMax);
  // The position ceiling needs the same treatment: odd source minus even size is odd.
  const xMax = Math.max(0, VE.w - w);
  const yMax = Math.max(0, VE.h - h);
  return {
    x: clamp(even(rect.x), 0, xMax - (xMax % 2)),
    y: clamp(even(rect.y), 0, yMax - (yMax % 2)),
    w, h,
  };
}

const HANDLES = ["nw", "n", "ne", "w", "e", "sw", "s", "se"];

function rectFor(op) {
  if (op.tool === "crop") return op.rect;
  if (op.tool === "overlay" && op.rect) return op.rect;
  return null;
}

// Arrow-key move/resize for the selected overlay, one step at a time like the mouse drag in
// the "ve-rects" handlers below. Ctrl changes the size instead of the position; a text
// overlay has no rect, so that combination is a no-op there.
export function nudgeSelectedOverlay(op, e) {
  const dx = e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : 0;
  const dy = e.key === "ArrowUp" ? -1 : e.key === "ArrowDown" ? 1 : 0;
  const rect = rectFor(op);
  // Rect-based overlays (crop/image/frame/pixelate/spotlight) must stay on even coordinates (see
  // snapRect), and rounding after adding an odd step would skip every other press, so the step
  // is rounded up to an even number first. Text has no rect and no such constraint.
  const rawAmount = e.shiftKey ? NUM_FIELD_BIG_STEP : VE.numFieldStep;
  const amount = rect ? Math.max(2, even(rawAmount)) : rawAmount;
  let patch;
  if (e.ctrlKey || e.metaKey) {
    if (!rect) return;
    patch = { rect: snapRect({ ...rect, w: Math.max(RECT_MIN_PX, rect.w + dx * amount), h: Math.max(RECT_MIN_PX, rect.h + dy * amount) }) };
  } else if (rect) {
    patch = { rect: snapRect({ ...rect, x: rect.x + dx * amount, y: rect.y + dy * amount }) };
  } else {
    patch = { x: clamp(Math.round(op.x + dx * amount), 0, VE.w), y: clamp(Math.round(op.y + dy * amount), 0, VE.h) };
  }
  pushReplacing((o) => o.id === op.id, { ...op, ...patch }, { keepEditor: true });
}

// The frame itself (four edges plus both centres) is one target, and every other crop/overlay
// adds its own edges and centre. The op being dragged is excluded so it never snaps to its
// own last committed position. A text overlay is a point, which snapMove treats as a
// zero-size target.
function veSnapTargets(excludeOp) {
  const targets = [{ x: 0, y: 0, w: VE.w, h: VE.h }];
  for (const op of VE.ops) {
    if (op === excludeOp) continue;
    const r = rectFor(op);
    if (r) targets.push(r);
    else if (op.tool === "overlay" && op.kind === "text") targets.push({ x: op.x, y: op.y, w: 0, h: 0 });
  }
  return targets;
}

// The visible rectangles: the crop box plus whichever overlay is being edited.
export function renderRects() {
  const layer = $("ve-rects");
  layer.innerHTML = "";
  const vb = videoBox();
  const items = [];
  const crop = opOf("crop");
  if (crop) items.push({ op: crop, kind: "crop" });
  const selected = VE.ops.find((o) => o.id && o.id === VE.selOv);
  if (selected && rectFor(selected)) items.push({ op: selected, kind: selected.kind });

  for (const item of items) {
    const rect = rectFor(item.op);
    const el = document.createElement("div");
    el.className = `ve-rect ve-rect-${item.kind}`;
    el.dataset.op = item.op.id || "crop";
    el.style.left = `${vb.left + rect.x * vb.scale}px`;
    el.style.top = `${vb.top + rect.y * vb.scale}px`;
    el.style.width = `${rect.w * vb.scale}px`;
    el.style.height = `${rect.h * vb.scale}px`;
    const size = document.createElement("span");
    size.className = "ve-rect-size";
    size.textContent = `${rect.w}×${rect.h}`;
    el.appendChild(size);
    for (const name of HANDLES) {
      const handle = document.createElement("span");
      handle.className = `ve-rect-handle ve-h-${name}`;
      handle.dataset.handle = name;
      el.appendChild(handle);
    }
    layer.appendChild(el);
  }

  // A text overlay has a point, not a rectangle: it gets a draggable label chip
  // showing roughly where the line will start.
  if (selected && selected.kind === "text") {
    const chip = document.createElement("div");
    chip.className = "ve-rect ve-rect-text ve-text-chip";
    chip.dataset.op = selected.id;
    chip.style.left = `${vb.left + selected.x * vb.scale}px`;
    chip.style.top = `${vb.top + selected.y * vb.scale}px`;
    chip.setAttribute("aria-label", selected.text || t("veOvText"));
    layer.appendChild(chip);
  }

  renderOverlayLive(vb);
}

// Drops every live-preview node. A detached <video> keeps decoding until its clip ends, so each
// inserted video is paused and released before the layer is emptied.
export function clearLiveOverlays(layer) {
  for (const el of VE.liveEls.values()) {
    if (el instanceof HTMLVideoElement) { el.pause(); el.removeAttribute("src"); el.load(); }
  }
  VE.liveEls.clear();
  layer.innerHTML = "";
}

// Every overlay active at the current playhead, drawn as close to its exported look as the
// browser manages, not just an outline. The enable check compares against op.startMs/op.endMs
// directly (source timeline, like VE.playhead); a video overlay's own clip position needs the
// outMs() conversion (see below).
//
// VE.liveEls keeps one DOM node per overlay id across frames instead of rebuilding the layer
// every tick (a fresh <img> or two <canvas> elements per frame made the preview stall).
// buildLiveOverlayEl() updates a passed-in element in place when the overlay already has one.
export function renderOverlayLive(vb) {
  const layer = $("ve-overlay-live");
  if (!VE.dur) { clearLiveOverlays(layer); return; }
  const box = vb || videoBox();
  const now = VE.playhead;
  const stack = overlayStack();
  for (const op of stack) {
    const prev = VE.liveEls.get(op.id);
    if (now < op.startMs || now > op.endMs) {
      if (prev) {
        prev.hidden = true;
        // A hidden inserted video would otherwise keep decoding off screen.
        if (prev instanceof HTMLVideoElement && !prev.paused) prev.pause();
      }
      continue;
    }
    const el = buildLiveOverlayEl(op, box, prev || null);
    if (!el) continue;
    if (el !== prev) {
      if (prev && prev.parentNode) prev.remove();
      layer.appendChild(el);
      VE.liveEls.set(op.id, el);
    }
    el.hidden = false;
  }
  // The layer order has to follow the export's stacking. Nodes are only moved
  // when they are out of order, since re-inserting a playing <video> restarts it.
  const wanted = stack.map((op) => VE.liveEls.get(op.id)).filter((el) => el && el.parentNode === layer);
  const inLayer = [...layer.children].filter((el) => wanted.includes(el));
  if (wanted.some((el, i) => el !== inLayer[i])) for (const el of wanted) layer.appendChild(el);
  // Overlays that were removed entirely (not just out of range) must not
  // keep their node alive in the map or the layer.
  const liveIds = new Set(opsOf("overlay").map((o) => o.id));
  for (const [id, el] of [...VE.liveEls]) {
    if (!liveIds.has(id)) {
      // A detached <video> keeps playing until it is collected, so stop it first.
      if (el instanceof HTMLVideoElement) el.pause();
      el.remove();
      VE.liveEls.delete(id);
    }
  }
}

// `reuse`, when given, is the element returned for this overlay last frame: every branch
// updates it in place instead of allocating, and resets whatever an earlier variant (frame
// style, text background) left behind. The pixelate canvas is still redrawn every frame since
// it samples the live video, but its scratch canvas is cached on it.
function buildLiveOverlayEl(op, vb, reuse) {
  const rectStyle = (el, rect) => {
    el.style.left = `${vb.left + rect.x * vb.scale}px`;
    el.style.top = `${vb.top + rect.y * vb.scale}px`;
    el.style.width = `${rect.w * vb.scale}px`;
    el.style.height = `${rect.h * vb.scale}px`;
  };
  if (op.kind === "image") {
    const img = reuse instanceof HTMLImageElement ? reuse : document.createElement("img");
    img.className = "ve-live ve-live-image";
    const src = convertFileSrc(op.path);
    if (img.src !== src) img.src = src;
    rectStyle(img, op.rect);
    img.style.opacity = String(clamp(op.opacity, 5, 100) / 100);
    return img;
  }
  if (op.kind === "video") {
    // A missing source (file moved or deleted) has no live preview, and the export chain skips it too.
    if (op.missing) return null;
    const el = reuse instanceof HTMLVideoElement ? reuse : document.createElement("video");
    el.className = "ve-live ve-live-video";
    rectStyle(el, op.rect);
    el.style.opacity = String(clamp(op.opacity, 5, 100) / 100);
    // Muted in the preview even when the export mixes its audio in (see buildAudioChain): a
    // second, unsynced audio source would only sound wrong.
    el.muted = true;
    el.playsInline = true;
    const src = convertFileSrc(op.path);
    if (el.dataset.ovSrc !== src) { el.src = src; el.dataset.ovSrc = src; }
    // The export shifts this overlay's picture (setpts, see the export filter builder) so its
    // first frame lands on the output at op.startMs, matching the adelay on its audio. The
    // preview mirrors that: the overlay plays from its beginning once the playhead reaches its
    // start, both mapped through outMs() so a variable-speed timeline lines up as in export.
    let t = (outMs(clamp(VE.playhead, 0, VE.dur)) - outMs(clamp(op.startMs, 0, VE.dur))) / 1000;
    if (t < 0) t = 0;
    if (op.loop && op.videoDurationMs) t %= op.videoDurationMs / 1000;
    if (Number.isFinite(t) && t >= 0 && Math.abs(el.currentTime - t) > 0.3) {
      try { el.currentTime = t; } catch {}
    }
    if (!videoEl.paused && t >= 0) { if (el.paused) el.play().catch(() => {}); }
    else if (!el.paused) el.pause();
    return el;
  }
  if (op.kind === "frame") {
    const el = reuse instanceof HTMLElement && reuse.tagName === "DIV" && reuse.classList.contains("ve-live-frame") ? reuse : document.createElement("div");
    el.className = "ve-live ve-live-frame";
    rectStyle(el, op.rect);
    const px = Math.max(1, op.thickness * vb.scale);
    el.style.opacity = String(clamp(op.opacity, 0, 100) / 100);
    el.style.borderWidth = `${px}px`;
    el.style.borderColor = op.color;
    el.style.outline = "";
    el.style.boxShadow = "";
    if (op.style === "double") {
      // Approximates the second, inset drawbox with an outline: a negative offset moves it inside
      // the border (-3t with width t covers inset 2t..3t, where the export draws it).
      el.style.outline = `${px}px solid ${op.color}`;
      el.style.outlineOffset = `${-3 * px}px`;
    } else if (op.style === "soft") {
      el.style.boxShadow = `0 0 ${px * 2}px ${px}px ${op.color}`;
    }
    return el;
  }
  if (op.kind === "text") {
    const el = reuse instanceof HTMLElement && reuse.classList.contains("ve-live-text") ? reuse : document.createElement("div");
    el.className = "ve-live ve-live-text";
    el.style.left = `${vb.left + op.x * vb.scale}px`;
    el.style.top = `${vb.top + op.y * vb.scale}px`;
    el.style.fontSize = `${Math.max(1, op.fontPx * vb.scale)}px`;
    el.style.fontFamily = FONT_CSS[op.font] || FONT_CSS.segoeui;
    el.style.fontWeight = op.font === "arialbd" ? "bold" : "normal";
    el.style.color = op.color;
    el.style.background = "";
    el.style.padding = "";
    el.style.webkitTextStroke = "";
    if (op.back === "plate") {
      el.style.background = "rgba(0, 0, 0, 0.5)";
      el.style.padding = `${Math.max(4, Math.round(op.fontPx / 4)) * vb.scale}px`;
    } else if (op.back === "outline") {
      el.style.webkitTextStroke = `${Math.max(1, (op.outlinePx || 3) * vb.scale)}px ${contrastTextColor(op.color)}`;
    }
    el.textContent = op.text;
    return el;
  }
  if (op.kind === "pixelate") {
    const w = Math.max(1, Math.round(op.rect.w * vb.scale)), h = Math.max(1, Math.round(op.rect.h * vb.scale));
    const canvas = reuse instanceof HTMLCanvasElement ? reuse : document.createElement("canvas");
    canvas.className = "ve-live ve-live-pixelate";
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    rectStyle(canvas, op.rect);
    // Down-sample the live video frame to the real block grid, then blow it back up with smoothing
    // off: the same two-step mosaic buildVideoChain() asks ffmpeg for. The small sampling canvas
    // is cached on the element, so it is resized rather than reallocated per frame.
    const bw = Math.max(1, Math.round(op.rect.w / op.blockPx)), bh = Math.max(1, Math.round(op.rect.h / op.blockPx));
    try {
      const tiny = canvas._veTiny || (canvas._veTiny = document.createElement("canvas"));
      if (tiny.width !== bw) tiny.width = bw;
      if (tiny.height !== bh) tiny.height = bh;
      tiny.getContext("2d").drawImage(videoEl, op.rect.x, op.rect.y, op.rect.w, op.rect.h, 0, 0, bw, bh);
      const ctx = canvas.getContext("2d");
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(tiny, 0, 0, bw, bh, 0, 0, w, h);
    } catch {
      // A frame not yet decoded (seek in flight): skip this tick, the next one draws.
    }
    return canvas;
  }
  if (op.kind === "spotlight") {
    const el = reuse instanceof HTMLElement && reuse.classList.contains("ve-live-spotlight") ? reuse : document.createElement("div");
    el.className = "ve-live ve-live-spotlight";
    // Covers the whole painted picture and cuts the clear rect out with clip-path instead of an
    // oversized box-shadow spread: some GPUs drop a 9999px spread when compositing, which left the
    // dim invisible while the drag border still rendered.
    el.style.left = `${vb.left}px`;
    el.style.top = `${vb.top}px`;
    el.style.width = `${vb.width}px`;
    el.style.height = `${vb.height}px`;
    el.style.background = `rgba(0, 0, 0, ${clamp(op.dim, 0, 1).toFixed(2)})`;
    const hx = op.rect.x * vb.scale, hy = op.rect.y * vb.scale;
    const hw = op.rect.w * vb.scale, hh = op.rect.h * vb.scale;
    const w = vb.width, h = vb.height;
    el.style.clipPath = `polygon(evenodd, 0 0, 0 ${h}px, ${w}px ${h}px, ${w}px 0, 0 0, `
      + `${hx}px ${hy}px, ${hx}px ${hy + hh}px, ${hx + hw}px ${hy + hh}px, ${hx + hw}px ${hy}px, ${hx}px ${hy}px)`;
    return el;
  }
  if (op.kind === "keys") {
    // A plain outline plus its preset name; the per-key layout and press highlighting is drawn by
    // the export's ffmpeg filters, so the preview only shows where and how big the overlay sits.
    const el = reuse instanceof HTMLElement && reuse.classList.contains("ve-live-keys") ? reuse : document.createElement("div");
    el.className = "ve-live ve-live-keys";
    rectStyle(el, op.rect);
    el.style.borderColor = op.color;
    el.style.opacity = String(clamp(op.opacity, 5, 100) / 100);
    el.textContent = t(overlayLabelKeysPresetKey(op.preset));
    return el;
  }
  return null;
}

const overlayLabelKeysPresetKey = (preset) => ({
  pressed: "veOvKeysPressed", half: "veOvKeysHalf", full: "veOvKeysFull", game: "veOvKeysGame", custom: "veOvKeysCustom",
}[preset] || "veOvKeys");

function resizeRect(orig, handle, point, keepRatio) {
  let x1 = orig.x, y1 = orig.y, x2 = orig.x + orig.w, y2 = orig.y + orig.h;
  if (handle.includes("n")) y1 = point.y;
  if (handle.includes("s")) y2 = point.y;
  if (handle.includes("w")) x1 = point.x;
  if (handle.includes("e")) x2 = point.x;
  const next = { x: Math.min(x1, x2), y: Math.min(y1, y2), w: Math.abs(x2 - x1), h: Math.abs(y2 - y1) };
  if (keepRatio && orig.w && orig.h) {
    const ratio = orig.w / orig.h;
    if (handle === "n" || handle === "s") next.w = next.h * ratio;
    else next.h = next.w / ratio;
  }
  return next;
}

$("ve-rects").addEventListener("pointerdown", (e) => {
  const rectEl = e.target.closest(".ve-rect");
  if (!rectEl) return;
  const id = rectEl.dataset.op;
  const op = id === "crop" ? opOf("crop") : VE.ops.find((o) => o.id === id);
  if (!op) return;
  e.preventDefault();
  $("ve-rects").setPointerCapture(e.pointerId);
  const point = toSource(e.clientX, e.clientY);
  const handle = e.target.dataset.handle || null;
  VE.drag = {
    kind: "rect",
    op,
    handle,
    start: point,
    orig: op.rect ? { ...op.rect } : { x: op.x, y: op.y, w: 0, h: 0 },
    live: op.rect ? { ...op.rect } : null,
    livePoint: op.rect ? null : { x: op.x, y: op.y },
  };
});

$("ve-rects").addEventListener("pointermove", (e) => {
  const drag = VE.drag;
  if (!drag || drag.kind !== "rect") return;
  const vb = videoBox();
  const point = { x: (e.clientX - vb.clientLeft) / vb.scale, y: (e.clientY - vb.clientTop) / vb.scale };
  // Alt disables snapping for the drag, as in the screenshot editor. Tolerance is in source
  // pixels like the drag math, so the on-screen setting is divided by the box scale.
  const tol = !e.altKey && VE.snapOn ? VE.snapTolPx / vb.scale : 0;
  const targets = tol ? veSnapTargets(drag.op) : null;
  VE.guide = null;
  if (drag.live) {
    let next;
    if (drag.handle) {
      let anchor = point;
      if (tol) {
        const s = snapMove({ x: point.x, y: point.y, w: 0, h: 0 }, targets, tol);
        anchor = { x: s.x, y: s.y };
        VE.guide = { x: s.guideX, y: s.guideY };
      }
      next = resizeRect(drag.orig, drag.handle, anchor, e.shiftKey);
    } else {
      next = {
        x: drag.orig.x + (point.x - drag.start.x),
        y: drag.orig.y + (point.y - drag.start.y),
        w: drag.orig.w, h: drag.orig.h,
      };
      if (tol) {
        const s = snapMove(next, targets, tol);
        next.x = s.x; next.y = s.y;
        VE.guide = { x: s.guideX, y: s.guideY };
      }
    }
    drag.live = snapRect(next);
    drag.op.rect = drag.live; // live preview only; the op is re-pushed on release
  } else {
    let p = { x: drag.orig.x + (point.x - drag.start.x), y: drag.orig.y + (point.y - drag.start.y) };
    if (tol) {
      const s = snapMove({ x: p.x, y: p.y, w: 0, h: 0 }, targets, tol);
      p = { x: s.x, y: s.y };
      VE.guide = { x: s.guideX, y: s.guideY };
    }
    drag.livePoint = { x: clamp(Math.round(p.x), 0, VE.w), y: clamp(Math.round(p.y), 0, VE.h) };
    drag.op.x = drag.livePoint.x;
    drag.op.y = drag.livePoint.y;
  }
  renderRects();
  renderSnapGuides(vb);
});

const rectsEqual = (a, b) => !!a && !!b && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;

function endRectDrag() {
  const drag = VE.drag;
  VE.drag = null;
  VE.guide = null;
  renderSnapGuides();
  if (!drag || drag.kind !== "rect") return;
  const op = drag.op;
  // A point op (text) has no rect: the move already changed x/y in place for
  // the live preview, so put the old spot back before the op is re-pushed, or
  // the undo step would restore nothing.
  if (!drag.live) {
    const p = drag.livePoint;
    op.x = drag.orig.x;
    op.y = drag.orig.y;
    if (p.x === op.x && p.y === op.y) { renderRects(); return; }
    pushReplacing((o) => o.id === op.id, { ...op, x: p.x, y: p.y }, { keepEditor: true });
    return;
  }
  const raw = drag.live && drag.live.w >= RECT_MIN_PX && drag.live.h >= RECT_MIN_PX ? drag.live : drag.orig;
  // The move also changed op.rect in place; restore it so the undo step keeps
  // the rect the op had before the drag.
  op.rect = { ...drag.orig };
  if (op.tool === "crop") {
    const rect = snapRect(raw);
    // A click-release with no movement (or one that snaps back to where it started) is not an
    // edit and must not push an undo step or wipe the redo stack.
    if (rectsEqual(rect, drag.orig)) { op.rect = rect; renderRects(); return; }
    pushCrop(rect);
  } else {
    if (rectsEqual(raw, drag.orig)) { op.rect = raw; renderRects(); return; }
    pushReplacing((o) => o.id === op.id, { ...op, rect: raw }, { keepEditor: true });
  }
}

$("ve-rects").addEventListener("pointerup", endRectDrag);
$("ve-rects").addEventListener("pointercancel", endRectDrag);
