import { attachSelectPicker } from "./selectpop.js";
import { contrastTextColor, makeHoldOrTapToggle, numberField, trapTabWithin } from "./floatShared.js";
import { placePopover, placePopoverAt } from "./popover.js";
import { snapMove } from "./snap.js";
import { $, colorPop, COLORS, drag, errToast, flag, getCurrentWindow, invoke, NUM_FIELD_BIG_STEP, openColorPop, ResizeDirection, settings, t, toast, txt, withShortcut } from "./core.js";
import { closeShortcutsModal, openShortcutsModal } from "./windowChrome.js";
import { EXPERT_DROPDOWNS } from "./expertMenu.js";
import { recordToolUsage } from "./management.js";
import { buildSwatches } from "./editor.js";

export function initSnipEditor() {
  // ==== 117/118: screenshot editor ====
  // Opens right after a capture, before #snip-modal. Pure frontend: it receives a
  // data URL and hands back an edited one; everything downstream of #snip-modal is
  // untouched. Every knob below reads live from the expert menu.
  // Starting sizes of a tool nobody has used yet. After that each tool starts with
  // the size it was last used at (ui_texts "snipToolSizes", see snipRememberSize).
  const SNIP_DEFAULT_SIZES = { pen: 6, marker: 18, line: 4, arrow: 4, rect: 4, ellipse: 4, step: 8, textPx: 24, outlinePx: 3, blockPx: 12, dim: 0.55 };
  const SNIP_ARROW_HEAD_SCALE = 3.2;
  const SNIP_SIZE_LIMITS = { width: [1, 80], fontPx: [10, 96], outlinePx: [1, 12], blockPx: [4, 64], dim: [0.1, 0.9] };
  function snipSizesRead() {
    try {
      const all = JSON.parse(txt("snipToolSizes") || "{}");
      return all && typeof all === "object" && !Array.isArray(all) ? all : {};
    } catch (e) { return {}; }
  }
  let snipSizesTimer = 0;
  function snipRememberSize(tool, field, value) {
    const lim = SNIP_SIZE_LIMITS[field];
    const n = Number(value);
    if (!lim || !Number.isFinite(n)) return;
    const all = snipSizesRead();
    const entry = all[tool] && typeof all[tool] === "object" ? all[tool] : {};
    entry[field] = Math.round(Math.min(lim[1], Math.max(lim[0], n)) * 100) / 100;
    all[tool] = entry;
    const json = JSON.stringify(all);
    settings.ui_texts = { ...(settings.ui_texts || {}), snipToolSizes: json };
    clearTimeout(snipSizesTimer);
    snipSizesTimer = setTimeout(() => {
      invoke("set_ui_text", { key: "snipToolSizes", value: json }).catch(() => {});
    }, 400);
  }
  let snipToolTimer = 0;
  function snipRememberTool(tool) {
    settings.ui_texts = { ...(settings.ui_texts || {}), snipLastTool: tool };
    clearTimeout(snipToolTimer);
    snipToolTimer = setTimeout(() => {
      invoke("set_ui_text", { key: "snipLastTool", value: tool }).catch(() => {});
    }, 400);
  }
  const SNIP_EDIT_UNDO_MAX = 50; // undo steps kept per edit
  // Refuse a target this large outright rather than risk an out-of-memory
  // canvas allocation -- comfortably above any real screen resolution.
  const SNIP_SCALE_MAX_AREA = 80_000_000;
  const SNIP_EDIT_SNAP = () => flag("snipEditSnap");
  const SNIP_TEXT_BACK = () => EXPERT_DROPDOWNS.snipTextBack.get();
  // The tool the editor opens with: a fixed choice from the expert menu, or (the
  // default) the tool picked last, kept in ui_texts "snipLastTool". A first open
  // or a stale id falls back to the hand.
  const SNIP_EDIT_DEFAULT_TOOL = () => {
    const pick = txt("snipEditDefaultTool") || "last";
    if (pick !== "last") return pick;
    const last = txt("snipLastTool");
    return SNIP_TOOLS.some((d) => d.id === last) ? last : "hand";
  };
  const SNIP_EDIT_SNAP_TOL = 6; // px (screen)
  const SNIP_DUP_OFFSET_PX = 16;
  // Small / medium / large stroke widths per tool; the middle one is the starting size.
  const SNIP_WIDTH_PRESETS_OF = (tool) => ({
    pen: [2, SNIP_DEFAULT_SIZES.pen, 14],
    marker: [10, SNIP_DEFAULT_SIZES.marker, 28],
    line: [2, SNIP_DEFAULT_SIZES.line, 8],
    arrow: [2, SNIP_DEFAULT_SIZES.arrow, 8],
    rect: [2, SNIP_DEFAULT_SIZES.rect, 8],
    ellipse: [2, SNIP_DEFAULT_SIZES.ellipse, 8],
    step: [4, SNIP_DEFAULT_SIZES.step, 14],
  }[tool] || [2, 4, 8]);
  const SNIP_TOOL_SVG = {
    crop: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M6 2v14a2 2 0 0 0 2 2h14M2 6h14a2 2 0 0 1 2 2v14"/></svg>',
    pen: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M4 20l1-4L15.5 5.5l3 3L8 19l-4 1Z"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M13.5 7l3 3"/></svg>',
    marker: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" stroke-linecap="round" d="M8 14l7-7 3 3-7 7-4 1 1-4Z"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M4 21h7"/></svg>',
    line: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><circle cx="5" cy="19" r="1.6" fill="currentColor"/><circle cx="19" cy="5" r="1.6" fill="currentColor"/><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M6.3 17.7 17.7 6.3"/></svg>',
    arrow: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M5 19 18 6M11 6h7v7"/></svg>',
    rect: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><rect x="4" y="6" width="16" height="12" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.8"/></svg>',
    ellipse: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><ellipse cx="12" cy="12" rx="8" ry="6" fill="none" stroke="currentColor" stroke-width="1.8"/></svg>',
    text: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M5 5.5h14M12 5.5v13"/></svg>',
    step: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.8"/><text x="12" y="15.5" font-size="9" text-anchor="middle" fill="currentColor" font-family="sans-serif">1</text></svg>',
    pixelate: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><g fill="currentColor"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7" opacity="0.5"/><rect x="3" y="14" width="7" height="7" opacity="0.5"/><rect x="14" y="14" width="7" height="7"/></g></svg>',
    spotlight: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><circle cx="12" cy="12" r="5" fill="none" stroke="currentColor" stroke-width="1.8"/><path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" opacity="0.6" d="M12 1.5v3M12 19.5v3M1.5 12h3M19.5 12h3"/></svg>',
    // The only way to reach panning used to be an undiscoverable middle-click
    // or holding Space -- a real toolbar entry for the exact same gesture.
    hand: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M8 13V6a1.5 1.5 0 0 1 3 0v5.5M11 11.2V4.5a1.5 1.5 0 0 1 3 0V11M14 11.3V6a1.5 1.5 0 0 1 3 0v7M17 12.5V9a1.5 1.5 0 0 1 3 0v6c0 3.6-2.2 6.5-6 6.5h-1c-2.7 0-4-.9-5.4-2.8L4.8 15c-.6-.9-.3-1.9.5-2.3.7-.4 1.5-.2 2.1.5L8 14"/></svg>',
  };
  const SNIP_EYEDROP_SVG = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="M15 5l4 4-9.5 9.5H5v-4.5L14.5 5Z"/><path fill="none" stroke="currentColor" stroke-width="1.8" d="M13 7l4 4"/></svg>';
  const SNIP_TOOLS = [
    { id: "crop", key: "1", label: "snipToolCrop" },
    { id: "pen", key: "2", label: "snipToolPen" },
    { id: "marker", key: "3", label: "snipToolMarker" },
    { id: "line", key: "4", label: "snipToolLine" },
    { id: "arrow", key: "5", label: "snipToolArrow" },
    { id: "rect", key: "6", label: "snipToolRect" },
    { id: "ellipse", key: "7", label: "snipToolEllipse" },
    { id: "text", key: "8", label: "snipToolText" },
    { id: "step", key: "9", label: "snipToolStep" },
    { id: "pixelate", key: "0", label: "snipToolPixelate" },
    { id: "spotlight", key: "-", label: "snipToolSpotlight" },
    { id: "hand", key: "h", label: "snipToolHand" },
  ];

  const snipEdit = $("snip-edit");
  const snipEditToolsEl = $("snip-edit-tools");
  const snipEditContextEl = $("snip-edit-context");
  const snipEditWrap = $("snip-edit-canvas-wrap");
  const snipEditStage = $("snip-edit-stage");
  const snipEditBaseCv = $("snip-edit-base");
  const snipEditLiveCv = $("snip-edit-live");
  const snipEditTextInput = $("snip-edit-text-input");
  const snipEditMeasureEl = $("snip-edit-measure");
  const snipEditNudgeHintEl = $("snip-edit-nudge-hint");
  // Right-click menu for duplicating/deleting a shape, built once like the
  // other dynamic popovers (tileFolderPop, viewsPop) instead of static markup.
  const snipCtxPop = document.createElement("div");
  snipCtxPop.className = "ctx snip-ctx-pop hidden";
  snipCtxPop.setAttribute("role", "menu");
  const snipCtxDupBtn = document.createElement("button");
  snipCtxDupBtn.type = "button";
  const snipCtxDelBtn = document.createElement("button");
  snipCtxDelBtn.type = "button";
  snipCtxDelBtn.className = "danger";
  snipCtxPop.append(snipCtxDupBtn, snipCtxDelBtn);
  document.body.appendChild(snipCtxPop);
  let snipCtxOp = null;
  function closeSnipCtxPop() { snipCtxPop.classList.add("hidden"); snipCtxOp = null; }
  function openSnipCtxPop(op, x, y) {
    snipCtxOp = op;
    snipCtxDupBtn.textContent = t("duplicate");
    snipCtxDupBtn.classList.toggle("hidden", op.tool === "spotlight");
    snipCtxDelBtn.textContent = t("delete");
    placePopoverAt(snipCtxPop, x, y);
  }
  snipCtxDupBtn.addEventListener("click", () => {
    const op = snipCtxOp;
    closeSnipCtxPop();
    if (op) snipEditDuplicateOp(op);
  });
  snipCtxDelBtn.addEventListener("click", () => {
    const op = snipCtxOp;
    closeSnipCtxPop();
    if (op) snipEditRemoveOp(op);
  });
  document.addEventListener("pointerdown", (e) => {
    if (snipCtxPop.classList.contains("hidden") || snipCtxPop.contains(e.target)) return;
    closeSnipCtxPop();
  });
  const snipEditBaseCtx = snipEditBaseCv.getContext("2d");
  const snipEditLiveCtx = snipEditLiveCv.getContext("2d");

  // Drawing model: SE.ops is the ordered operation list ("base image, then every
  // operation in sequence"). SE.rawCanvas holds ONLY the untouched capture (native
  // size) so pixelate always samples the base, never already-drawn operations.
  const SE = {
    active: false, img: null, rawCanvas: null, w: 0, h: 0,
    ops: [], redo: [], floor: 0, perTool: {}, tool: SNIP_EDIT_DEFAULT_TOOL(),
    zoom: 1, drag: null, panState: null, spaceDown: false, showRaw: false,
    cropRect: null, lastNudged: null, eyedropArm: null, textEdit: null, selOp: null,
    onApply: null, onCancel: null,
  };

  // Hold the toolbar button (or V) to peek at the untouched capture, or tap
  // it to lock the original on screen until pressed again; the same control
  // drives both the button and the key, see the wiring further down.
  const snipOriginalToggle = makeHoldOrTapToggle((shown) => {
    SE.showRaw = shown;
    snipEditRedrawBase();
    $("snip-edit-original").classList.toggle("active", shown);
    $("snip-edit-original-badge").classList.toggle("hidden", !shown);
  });

  function snipEditClamp(v, lo, hi) { return Math.min(hi, Math.max(lo, v)); }
  // One zoom step (20 %) for the wheel, the buttons and the keys, and the
  // 8x ceiling, the same in this editor and the video editor.
  const snipZoomStepFactor = 1.2;
  const snipZoomMax = 8;
  // Zoom is image pixels per DEVICE pixel (100 % = one image pixel on one screen
  // pixel), so a CSS length is zoom / devicePixelRatio per image pixel. Every
  // conversion between client (CSS) coordinates and image coordinates goes
  // through this one factor.
  function snipEditScale() { return SE.zoom / (window.devicePixelRatio || 1); }
  function snipEditDist(a, b) { return Math.hypot(b.x - a.x, b.y - a.y); }
  function snipEditNormRect(r) {
    if (r && "a" in r) {
      const x = Math.min(r.a.x, r.b.x), y = Math.min(r.a.y, r.b.y);
      return { x, y, w: Math.abs(r.b.x - r.a.x), h: Math.abs(r.b.y - r.a.y) };
    }
    return r;
  }
  // The one place image-pixel coordinates are computed. Canvas backing stores are
  // always the image's native size; the CSS display size is SE.w * snipEditScale(),
  // and clientX/clientY and getBoundingClientRect are both CSS pixels.
  function snipEditToImage(ev) {
    const r = snipEditBaseCv.getBoundingClientRect();
    const k = snipEditScale();
    return {
      x: snipEditClamp((ev.clientX - r.left) / k, 0, SE.w),
      y: snipEditClamp((ev.clientY - r.top) / k, 0, SE.h),
    };
  }
  // Bounding rect of an op for snap purposes -- a rect-shaped op already has
  // one, a two-point op (line/arrow/pen/marker) gets the box around its points.
  function snipEditOpBoundingRect(op) {
    if (op.rect) return op.rect;
    if (op.points && op.points.length >= 2) {
      const xs = op.points.map((p) => p.x), ys = op.points.map((p) => p.y);
      return { x: Math.min(...xs), y: Math.min(...ys), w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys) };
    }
    return null;
  }
  // The canvas itself (its four edges plus both centres) counts as one target,
  // then every already-placed shape adds its own edges and centres.
  function snipEditSnapTargets() {
    const targets = [{ x: 0, y: 0, w: SE.w, h: SE.h }];
    for (const op of SE.ops) {
      const r = snipEditOpBoundingRect(op);
      if (r) targets.push(r);
    }
    return targets;
  }
  // Set by snipEditSnapPoint while a drag is snapped, read by the live-preview
  // draw functions to show a thin accent-coloured guide line; cleared again as
  // soon as nothing is snapped, so it never lingers after the pointer moves on.
  let snipEditGuide = null;
  function snipEditSnapPoint(pt) {
    if (!SNIP_EDIT_SNAP()) { snipEditGuide = null; return pt; }
    const tol = SNIP_EDIT_SNAP_TOL / snipEditScale();
    const r = snapMove({ x: pt.x, y: pt.y, w: 0, h: 0 }, snipEditSnapTargets(), tol);
    snipEditGuide = (r.guideX != null || r.guideY != null) ? { x: r.guideX, y: r.guideY } : null;
    return { x: r.x, y: r.y };
  }
  // Alt disables snapping for the duration of the drag, same as the shift-key
  // constraint below is per-drag rather than a persistent mode.
  function snipEditApplySnap(pt0, altKey) {
    if (altKey) { snipEditGuide = null; return pt0; }
    return snipEditSnapPoint(pt0);
  }
  function snipEditDrawGuides() {
    if (!snipEditGuide) return;
    const ctx = snipEditLiveCtx;
    const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#2563eb";
    ctx.save();
    ctx.strokeStyle = accent;
    ctx.lineWidth = 1 / snipEditScale();
    ctx.setLineDash([4 / snipEditScale(), 3 / snipEditScale()]);
    if (snipEditGuide.x != null) { ctx.beginPath(); ctx.moveTo(snipEditGuide.x, 0); ctx.lineTo(snipEditGuide.x, SE.h); ctx.stroke(); }
    if (snipEditGuide.y != null) { ctx.beginPath(); ctx.moveTo(0, snipEditGuide.y); ctx.lineTo(SE.w, snipEditGuide.y); ctx.stroke(); }
    ctx.restore();
  }

  function snipEditToolState(tool) {
    if (!SE.perTool[tool]) {
      const widths = SNIP_WIDTH_PRESETS_OF(tool);
      const saved = snipSizesRead()[tool];
      // A remembered size counts only when it is a number inside the field's range;
      // anything else (a broken or hand-edited entry) falls back to the start size.
      const pick = (field, def) => {
        const lim = SNIP_SIZE_LIMITS[field];
        const v = saved && typeof saved === "object" ? Number(saved[field]) : NaN;
        return Number.isFinite(v) && v >= lim[0] && v <= lim[1] ? v : def;
      };
      SE.perTool[tool] = {
        color: txt("snipDefaultColor") || COLORS[1] || "#ef4444",
        width: pick("width", widths[1]),
        filled: false,
        fontPx: pick("fontPx", SNIP_DEFAULT_SIZES.textPx),
        back: SNIP_TEXT_BACK(),
        outlinePx: pick("outlinePx", SNIP_DEFAULT_SIZES.outlinePx),
        blockPx: pick("blockPx", SNIP_DEFAULT_SIZES.blockPx),
        dim: pick("dim", SNIP_DEFAULT_SIZES.dim),
      };
    }
    return SE.perTool[tool];
  }
  function snipEditGetPixelColor(pt) {
    const x = snipEditClamp(Math.round(pt.x), 0, SE.w - 1);
    const y = snipEditClamp(Math.round(pt.y), 0, SE.h - 1);
    const d = snipEditBaseCtx.getImageData(x, y, 1, 1).data;
    return "#" + [d[0], d[1], d[2]].map((v) => v.toString(16).padStart(2, "0")).join("");
  }

  // ---- per-tool drawing (shared by the finished layer and the live preview) ----
  function snipEditStrokeSmoothPath(ctx, points) {
    if (points.length < 2) return;
    ctx.beginPath();
    ctx.moveTo(points[0].x, points[0].y);
    for (let i = 1; i < points.length - 1; i++) {
      const mx = (points[i].x + points[i + 1].x) / 2;
      const my = (points[i].y + points[i + 1].y) / 2;
      ctx.quadraticCurveTo(points[i].x, points[i].y, mx, my);
    }
    const last = points[points.length - 1];
    ctx.lineTo(last.x, last.y);
    ctx.stroke();
  }
  function snipEditDrawPenOp(ctx, op) {
    ctx.save();
    ctx.strokeStyle = op.color;
    ctx.lineWidth = op.width;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    snipEditStrokeSmoothPath(ctx, op.points);
    ctx.restore();
  }
  // Collects the whole stroke on its own transparent layer first (so self-overlap
  // within one stroke never darkens further), then composites it once with alpha.
  // Reused across calls instead of a fresh full-resolution canvas per marker
  // stroke -- snipEditRedrawBase calls this once per marker op on EVERY redraw
  // (every new operation, undo/redo, nudge and tool switch), so cost used to
  // scale with marker count times image resolution.
  let snipEditMarkerTmp = null;
  function snipEditDrawMarkerOp(ctx, op) {
    const tmp = snipEditMarkerTmp || (snipEditMarkerTmp = document.createElement("canvas"));
    if (tmp.width !== SE.w) tmp.width = SE.w;
    if (tmp.height !== SE.h) tmp.height = SE.h;
    const tctx = tmp.getContext("2d");
    tctx.clearRect(0, 0, tmp.width, tmp.height);
    tctx.strokeStyle = op.color;
    tctx.lineWidth = op.width;
    tctx.lineCap = "round";
    tctx.lineJoin = "round";
    snipEditStrokeSmoothPath(tctx, op.points);
    ctx.save();
    ctx.globalAlpha = 0.5;
    ctx.globalCompositeOperation = "multiply";
    ctx.drawImage(tmp, 0, 0);
    ctx.restore();
  }
  function snipEditDrawLineOp(ctx, op) {
    ctx.save();
    ctx.strokeStyle = op.color;
    ctx.lineWidth = op.width;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(op.points[0].x, op.points[0].y);
    ctx.lineTo(op.points[1].x, op.points[1].y);
    ctx.stroke();
    ctx.restore();
  }
  // Used to draw the full shaft (round-capped) all the way to the tip and
  // then lay a short, fairly wide triangle on top -- the round cap regularly
  // poked out from under/beside that triangle, looking like a buggy double
  // point. Now the shaft stops exactly at the head's own back edge (so the
  // cap's bulge lands under the head's fill, never past its sides), and the
  // head itself is a longer, more slender point instead of a stubby wedge.
  function snipEditDrawArrowOp(ctx, op) {
    const [a, b] = op.points;
    const dist = Math.hypot(b.x - a.x, b.y - a.y);
    const angle = dist > 0 ? Math.atan2(b.y - a.y, b.x - a.x) : 0;
    // Capped at the actual drag distance -- otherwise a drag shorter than one
    // arrowhead put the shaft's "back" point PAST the start point, flipping
    // the shaft to point backwards during the first few pixels of every drag.
    const headLen = Math.min(7 + op.width * SNIP_ARROW_HEAD_SCALE, dist); // tip-to-back distance along the shaft
    const headHalfAngle = Math.PI / 9;  // ~20 deg either side -- slender, not dumpy
    const backX = b.x - headLen * Math.cos(angle);
    const backY = b.y - headLen * Math.sin(angle);
    ctx.save();
    ctx.strokeStyle = op.color;
    ctx.lineWidth = op.width;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(backX, backY);
    ctx.stroke();
    ctx.restore();
    const headSide = headLen / Math.cos(headHalfAngle); // tip-to-back-corner distance
    ctx.save();
    ctx.fillStyle = op.color;
    ctx.beginPath();
    ctx.moveTo(b.x, b.y);
    ctx.lineTo(b.x - headSide * Math.cos(angle - headHalfAngle), b.y - headSide * Math.sin(angle - headHalfAngle));
    ctx.lineTo(b.x - headSide * Math.cos(angle + headHalfAngle), b.y - headSide * Math.sin(angle + headHalfAngle));
    ctx.closePath();
    ctx.fill();
    ctx.restore();
  }
  function snipEditDrawRectOp(ctx, op) {
    const r = op.rect;
    ctx.save();
    ctx.strokeStyle = op.color;
    ctx.fillStyle = op.color;
    ctx.lineWidth = op.width;
    if (op.filled) ctx.fillRect(r.x, r.y, r.w, r.h);
    else ctx.strokeRect(r.x + op.width / 2, r.y + op.width / 2, Math.max(0, r.w - op.width), Math.max(0, r.h - op.width));
    ctx.restore();
  }
  function snipEditDrawEllipseOp(ctx, op) {
    const r = op.rect;
    ctx.save();
    ctx.strokeStyle = op.color;
    ctx.fillStyle = op.color;
    ctx.lineWidth = op.width;
    ctx.beginPath();
    ctx.ellipse(r.x + r.w / 2, r.y + r.h / 2, Math.max(0, r.w / 2 - op.width / 2), Math.max(0, r.h / 2 - op.width / 2), 0, 0, Math.PI * 2);
    if (op.filled) ctx.fill(); else ctx.stroke();
    ctx.restore();
  }
  function snipEditDrawTextOp(ctx, op) {
    ctx.save();
    ctx.font = `${op.fontPx}px sans-serif`;
    ctx.textBaseline = "top";
    const w = ctx.measureText(op.text).width;
    const h = op.fontPx * 1.25;
    if (op.back === "plate") {
      ctx.fillStyle = "rgba(0,0,0,0.45)";
      ctx.fillRect(op.point.x - 4, op.point.y - 2, w + 8, h + 4);
    }
    ctx.shadowColor = "rgba(0,0,0,0.6)";
    ctx.shadowBlur = 3;
    ctx.shadowOffsetX = 1;
    ctx.shadowOffsetY = 1;
    if (op.back === "outline") {
      ctx.lineWidth = op.outlinePx || SNIP_DEFAULT_SIZES.outlinePx;
      ctx.strokeStyle = contrastTextColor(op.color);
      ctx.strokeText(op.text, op.point.x, op.point.y);
    }
    ctx.fillStyle = op.color;
    ctx.fillText(op.text, op.point.x, op.point.y);
    ctx.restore();
  }
  function snipEditMeasureTextWidth(op) {
    snipEditBaseCtx.save();
    snipEditBaseCtx.font = `${op.fontPx}px sans-serif`;
    const w = snipEditBaseCtx.measureText(op.text).width;
    snipEditBaseCtx.restore();
    return w;
  }
  function snipEditDrawStepOp(ctx, op, n) {
    const r = 10 + op.width;
    ctx.save();
    ctx.fillStyle = op.color;
    ctx.beginPath();
    ctx.arc(op.point.x, op.point.y, r, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = contrastTextColor(op.color);
    ctx.font = `${Math.round(r * 1.1)}px sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(String(n), op.point.x, op.point.y + 1);
    ctx.restore();
  }
  // Always samples SE.rawCanvas (the untouched capture) -- never already-drawn
  // operations -- so overlapping pixelation never compounds on itself.
  function snipEditDrawPixelate(ctx, rect, blockPx) {
    const { x, y, w, h } = snipEditNormRect(rect);
    if (w < 2 || h < 2) return;
    const bw = Math.max(1, Math.round(w / Math.max(2, blockPx)));
    const bh = Math.max(1, Math.round(h / Math.max(2, blockPx)));
    const tmp = document.createElement("canvas");
    tmp.width = bw;
    tmp.height = bh;
    const tctx = tmp.getContext("2d");
    tctx.imageSmoothingEnabled = true;
    tctx.drawImage(SE.rawCanvas, x, y, w, h, 0, 0, bw, bh);
    ctx.save();
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(tmp, 0, 0, bw, bh, x, y, w, h);
    ctx.restore();
  }
  function snipEditDrawSpotlight(ctx, rect, dim) {
    const { x, y, w, h } = snipEditNormRect(rect);
    ctx.save();
    ctx.fillStyle = `rgba(0,0,0,${dim})`;
    ctx.fillRect(0, 0, SE.w, y);
    ctx.fillRect(0, y + h, SE.w, SE.h - (y + h));
    ctx.fillRect(0, y, x, h);
    ctx.fillRect(x + w, y, SE.w - (x + w), h);
    ctx.restore();
  }
  function snipEditDrawCropDim(ctx, r, alpha = 0.55) {
    ctx.save();
    ctx.fillStyle = `rgba(0,0,0,${alpha})`;
    ctx.fillRect(0, 0, SE.w, r.y);
    ctx.fillRect(0, r.y + r.h, SE.w, SE.h - (r.y + r.h));
    ctx.fillRect(0, r.y, r.x, r.h);
    ctx.fillRect(r.x + r.w, r.y, SE.w - (r.x + r.w), r.h);
    ctx.restore();
  }
  function snipEditDrawOp(ctx, op, stepN) {
    switch (op.tool) {
      case "pen": snipEditDrawPenOp(ctx, op); break;
      case "marker": snipEditDrawMarkerOp(ctx, op); break;
      case "line": snipEditDrawLineOp(ctx, op); break;
      case "arrow": snipEditDrawArrowOp(ctx, op); break;
      case "rect": snipEditDrawRectOp(ctx, op); break;
      case "ellipse": snipEditDrawEllipseOp(ctx, op); break;
      case "text": snipEditDrawTextOp(ctx, op); break;
      case "pixelate": snipEditDrawPixelate(ctx, op.rect, op.blockPx); break;
      case "spotlight": snipEditDrawSpotlight(ctx, op.rect, op.dim); break;
      case "step": snipEditDrawStepOp(ctx, op, stepN); break;
    }
  }

  // Full recomposite: base image, then every finished operation in sequence. Only
  // called on discrete events (undo/redo/crop-change/zoom-change/finalize) -- never
  // per mousemove, so a 4K capture never redraws its whole bitmap on a drag.
  // `bare` leaves the crop dimming out, for the one caller that bakes this canvas
  // into the image itself (a resize).
  function snipEditRedrawBase(bare) {
    if (!SE.img) return; // no session (the editor was closed)
    const ctx = snipEditBaseCtx;
    ctx.clearRect(0, 0, SE.w, SE.h);
    // Showing the original means the untouched capture, not just "before the
    // pending annotations" -- SE.img is the last baked composite (a scale op
    // bakes every annotation drawn before it), so only SE.rawCanvas is still
    // the same pixels the capture started with.
    if (SE.showRaw) { ctx.drawImage(SE.rawCanvas, 0, 0, SE.w, SE.h); return; }
    ctx.drawImage(SE.img, 0, 0, SE.w, SE.h);
    // A resize bakes the markers placed so far into SE.img; the scale op remembers
    // how many, so the next marker carries on from there instead of restarting at 1.
    const baked = SE.ops.find((o) => o.tool === "scale");
    let stepN = baked ? baked.stepBase : 0;
    for (const op of SE.ops) {
      if (op.tool === "crop") continue; // drawn as an overlay pass below
      if (op.tool === "scale") continue; // already baked into SE.img, see snipEditApplyScale
      if (op.tool === "edit") continue; // an undo record for a moved/deleted shape, nothing to draw
      if (op.tool === "step") stepN++;
      snipEditDrawOp(ctx, op, stepN);
    }
    const crop = SE.ops.find((o) => o.tool === "crop");
    if (crop && SE.tool !== "crop" && !bare) snipEditDrawCropDim(ctx, crop.rect); // crop tool shows its own live overlay instead
  }
  function snipEditClearLive() { snipEditLiveCtx.clearRect(0, 0, SE.w, SE.h); }

  function snipEditUpdateUndoRedoButtons() {
    $("snip-edit-undo").disabled = SE.ops.length <= SE.floor;
    $("snip-edit-redo").disabled = !SE.redo.length;
  }
  // A resize keeps the image, raw capture and op list from before it so Undo can swap them
  // back. Once undo can no longer reach the resize those are dead weight, and every earlier
  // resolution of the screenshot would otherwise stay alive until the next editor session.
  function snipEditReleaseResize(op) {
    op.prevImg = null;
    op.prevRawCanvas = null;
    op.prevOps = [];
  }
  // Resizes that sit below the undo floor can never be undone.
  function snipEditReleaseBelowFloor() {
    const n = Math.min(SE.floor, SE.ops.length);
    for (let i = 0; i < n; i++) if (SE.ops[i].tool === "scale") snipEditReleaseResize(SE.ops[i]);
  }
  // Resizes bake the history into the image, so they chain through `prevOps` instead of
  // piling up in SE.ops: the undo limit has to be applied along that chain. The resize past
  // the limit is released, and the next newer one remembers it as permanent (its floor).
  function snipEditTrimResizeChain(newest) {
    let depth = 1;
    let cur = newest;
    for (;;) {
      const older = cur.prevOps.find((o) => o.tool === "scale");
      if (!older) return;
      depth++;
      if (depth > SNIP_EDIT_UNDO_MAX) {
        if (!older.prevImg) return; // this one and everything behind it was released already
        cur.prevFloor = Math.max(cur.prevFloor, cur.prevOps.indexOf(older) + 1);
        snipEditReleaseResize(older);
      }
      cur = older;
    }
  }
  // `nudge` is what the arrow keys act on afterwards: the op itself by default, or
  // whatever the caller names (an undo entry is not something to nudge).
  function snipEditPushOp(op, nudge) {
    SE.ops.push(op);
    // SE.ops IS the render list -- never drop a rendered op. Once the undo
    // window is exceeded, advance the floor instead of shifting the oldest op out;
    // it stays drawn (snipEditRedrawBase still walks every op) but becomes
    // permanent, undo just can no longer walk back past it.
    if (SE.ops.length - SE.floor > SNIP_EDIT_UNDO_MAX) SE.floor = SE.ops.length - SNIP_EDIT_UNDO_MAX;
    snipEditReleaseBelowFloor();
    SE.redo.length = 0;
    SE.lastNudged = nudge === undefined ? op : nudge;
    snipEditUpdateNudgeHint();
    snipEditRedrawBase();
    snipEditUpdateUndoRedoButtons();
  }
  // Moving, resizing, re-pointing or deleting a finished shape or text changes an
  // op that already sits in SE.ops, so it gets an undo entry of its own: a
  // {tool: "edit"} record that is never drawn, only applied backwards by undo and
  // forwards again by redo (see snipEditUndoEdit / snipEditRedoEdit).
  function snipEditGeom(op) {
    const g = {};
    if (op.points) g.points = op.points.map((p) => ({ ...p }));
    if (op.rect) g.rect = { ...op.rect };
    if (op.point) g.point = { ...op.point };
    return g;
  }
  function snipEditApplyGeom(op, g) {
    if (g.points) op.points = g.points.map((p) => ({ ...p }));
    if (g.rect) op.rect = { ...g.rect };
    if (g.point) op.point = { ...g.point };
  }
  function snipEditCommitGeomEdit(op, before) {
    const after = snipEditGeom(op);
    if (JSON.stringify(before) === JSON.stringify(after)) return;
    snipEditPushOp({ tool: "edit", kind: "geom", op, before, after }, SE.lastNudged);
  }
  // Puts an op back next to the neighbours it had when it left SE.ops. Positions are
  // never stored as raw indexes: a crop commit or discard in between filters an op
  // out and would shift every index behind it.
  function snipEditReinsert(op, prev, next) {
    let i = next ? SE.ops.indexOf(next) : -1;
    if (i < 0) {
      const p = prev ? SE.ops.indexOf(prev) : -1;
      i = p >= 0 ? p + 1 : (prev || next) ? SE.ops.length : 0;
    }
    SE.ops.splice(i, 0, op);
  }
  function snipEditUndoEdit(e) {
    if (e.kind === "geom") snipEditApplyGeom(e.op, e.before);
    else if (e.kind === "remove") { snipEditReinsert(e.op, e.prev, e.next); SE.floor += e.floorShift; }
  }
  function snipEditRedoEdit(e) {
    if (e.kind === "geom") snipEditApplyGeom(e.op, e.after);
    else if (e.kind === "remove") {
      const i = SE.ops.indexOf(e.op);
      if (i >= 0) SE.ops.splice(i, 1);
      SE.floor = Math.max(0, SE.floor - e.floorShift);
    }
  }
  // Spotlight/crop: exactly one may exist -- a new one replaces the old rather than
  // stacking two dimming layers.
  function snipEditPushUniqueOp(tool, op) {
    const at = SE.ops.findIndex((o) => o.tool === tool);
    // A replaced spotlight or crop is something the user made: Ctrl+Z on its replacement
    // must bring it back, so the new op remembers it and where it sat.
    if ((tool === "spotlight" || tool === "crop") && at >= 0) {
      op.replaced = { op: SE.ops[at], prev: SE.ops[at - 1] || null, next: SE.ops[at + 1] || null, floorShift: at < SE.floor ? 1 : 0 };
    }
    // floor is a count-based boundary into SE.ops; only an entry that sat below
    // it shifts it (an undoable op above the floor leaving must not re-lock one).
    SE.floor = Math.max(0, SE.floor - SE.ops.slice(0, SE.floor).filter((o) => o.tool === tool).length);
    SE.ops = SE.ops.filter((o) => o.tool !== tool);
    snipEditPushOp(op);
  }
  // Halving-step resample: a single drawImage straight down to a much smaller
  // canvas blurs/aliases badly (the classic "shrunk screenshot text turns to
  // mush" problem), so downscaling by more than 2x repeatedly halves first and
  // only does the final, non-halved step at the end. Upscaling runs in one step.
  function snipEditScaleCanvas(src, targetW, targetH) {
    let cur = src, curW = src.width, curH = src.height;
    while (curW > targetW * 2 && curH > targetH * 2) {
      const nextW = Math.max(targetW, Math.round(curW / 2));
      const nextH = Math.max(targetH, Math.round(curH / 2));
      const step = document.createElement("canvas");
      step.width = nextW;
      step.height = nextH;
      const sctx = step.getContext("2d");
      sctx.imageSmoothingEnabled = true;
      sctx.imageSmoothingQuality = "high";
      sctx.drawImage(cur, 0, 0, curW, curH, 0, 0, nextW, nextH);
      cur = step;
      curW = nextW;
      curH = nextH;
    }
    const out = document.createElement("canvas");
    out.width = targetW;
    out.height = targetH;
    const octx = out.getContext("2d");
    octx.imageSmoothingEnabled = true;
    octx.imageSmoothingQuality = "high";
    octx.drawImage(cur, 0, 0, curW, curH, 0, 0, targetW, targetH);
    return out;
  }
  // Scaling is an operation like any other tool, not a destructive edit: it
  // pushes a "scale" op so undo/redo work the normal way. Every other op stores
  // its coordinates in image pixels, so instead of trying to rescale each one
  // individually this bakes the base image and every op drawn before it into a
  // new canvas at the new size (a flatten step) -- see the "scale" skip in
  // snipEditRedrawBase and the special-cased undo/redo just below.
  function snipEditApplyScale(targetW, targetH) {
    targetW = Math.max(1, Math.round(targetW));
    targetH = Math.max(1, Math.round(targetH));
    if (targetW * targetH > SNIP_SCALE_MAX_AREA) { toast(t("snipScaleTooLarge")); return false; }
    if (SE.tool === "crop") snipEditCropCommit(); // same guard snipEditExportDataUrl uses for a drawn-but-uncommitted crop
    snipEditFinishTextIfOpen();
    // With the original locked on, the base canvas shows only the raw capture and
    // the bake below would drop every annotation; export resets it the same way.
    snipOriginalToggle.reset();
    const prevW = SE.w, prevH = SE.h, prevImg = SE.img, prevRawCanvas = SE.rawCanvas, prevOps = SE.ops.slice();
    const sx = targetW / SE.w, sy = targetH / SE.h;
    snipEditRedrawBase(true); // make sure snipEditBaseCv reflects every op (but crop) before it gets baked in
    const baked = snipEditScaleCanvas(snipEditBaseCv, targetW, targetH);
    const rawBaked = snipEditScaleCanvas(SE.rawCanvas, targetW, targetH);
    // Crop is never rasterized (it only ever dims an overlay preview, see
    // snipEditRedrawBase) -- its rect is the one thing stored in image-pixel
    // coordinates that survives the bake, so it is rescaled by hand instead of
    // being baked away, or it would point at the wrong region afterward.
    const keptOps = SE.ops
      .filter((o) => o.tool === "crop")
      .map((o) => ({ ...o, rect: { x: o.rect.x * sx, y: o.rect.y * sy, w: o.rect.w * sx, h: o.rect.h * sy } }));
    const prevFloor = SE.floor;
    SE.floor = Math.max(0, SE.floor - (prevOps.length - keptOps.length));
    const prevScale = prevOps.find((o) => o.tool === "scale");
    const stepBase = (prevScale ? prevScale.stepBase : 0) + prevOps.filter((o) => o.tool === "step").length;
    const op = {
      tool: "scale", w: targetW, h: targetH, img: baked, rawCanvas: rawBaked, keptOps, stepBase,
      prevW, prevH, prevImg, prevRawCanvas, prevOps, prevFloor,
    };
    SE.img = baked;
    SE.rawCanvas = rawBaked;
    SE.w = targetW;
    SE.h = targetH;
    SE.ops = keptOps;
    snipEditBaseCv.width = targetW;
    snipEditBaseCv.height = targetH;
    snipEditLiveCv.width = targetW;
    snipEditLiveCv.height = targetH;
    snipEditPushOp(op);
    snipEditTrimResizeChain(op);
    snipEditSyncCropFromOps(); // the crop op was rescaled above, the editable box must follow
    snipEditFit();
    return true;
  }
  // Scale bakes the whole composite into a new SE.img/SE.rawCanvas at a new
  // SE.w/SE.h (see snipEditApplyScale) instead of being replayed like every
  // other op, so undo/redo can't just pop/push it -- the op itself carries the
  // before/after canvases and sizes needed to swap the working image back and
  // forth, same idea as SE.rawCanvas already keeping the untouched capture.
  function snipEditRestoreScaleState(w, h, img, rawCanvas, ops) {
    SE.w = w;
    SE.h = h;
    SE.img = img;
    SE.rawCanvas = rawCanvas;
    SE.ops = ops.slice();
    snipEditBaseCv.width = w;
    snipEditBaseCv.height = h;
    snipEditLiveCv.width = w;
    snipEditLiveCv.height = h;
    snipEditFit();
  }
  function snipEditUndo() {
    if (SE.ops.length <= SE.floor) return;
    const op = SE.ops.pop();
    SE.redo.push(op);
    if (op.tool === "scale") {
      snipEditRestoreScaleState(op.prevW, op.prevH, op.prevImg, op.prevRawCanvas, op.prevOps);
      SE.floor = op.prevFloor; // the undo limit as it was before the resize
    } else if (op.tool === "edit") {
      snipEditUndoEdit(op);
    } else if ((op.tool === "spotlight" || op.tool === "crop") && op.replaced) {
      const r = op.replaced;
      snipEditReinsert(r.op, r.prev, r.next);
      SE.floor += r.floorShift;
    }
    snipEditDeselectShape();
    SE.lastNudged = null;
    snipEditUpdateNudgeHint();
    snipEditRedrawBase();
    snipEditSyncCropFromOps();
    if (SE.tool === "crop") snipEditFitToCrop();
    snipEditUpdateUndoRedoButtons();
  }
  function snipEditRedo() {
    if (!SE.redo.length) return;
    const op = SE.redo.pop();
    SE.ops.push(op);
    if (op.tool === "scale") {
      snipEditRestoreScaleState(op.w, op.h, op.img, op.rawCanvas, [...op.keptOps, op]);
      // The floor the resize itself produced, worked out the same way it was then.
      SE.floor = Math.max(0, op.prevFloor - (op.prevOps.length - op.keptOps.length));
      if (SE.ops.length - SE.floor > SNIP_EDIT_UNDO_MAX) SE.floor = SE.ops.length - SNIP_EDIT_UNDO_MAX;
      snipEditReleaseBelowFloor();
    } else if (op.tool === "edit") {
      snipEditRedoEdit(op);
    } else if ((op.tool === "spotlight" || op.tool === "crop") && op.replaced) {
      const i = SE.ops.indexOf(op.replaced.op);
      if (i >= 0) SE.ops.splice(i, 1);
      SE.floor = Math.max(0, SE.floor - op.replaced.floorShift);
    }
    snipEditDeselectShape();
    SE.lastNudged = null;
    snipEditUpdateNudgeHint();
    snipEditRedrawBase();
    snipEditSyncCropFromOps();
    if (SE.tool === "crop") snipEditFitToCrop();
    snipEditUpdateUndoRedoButtons();
  }
  // Shown only while something is nudgeable -- the arrow keys have no other
  // job in this editor (no frames to step through), but a silent behaviour
  // switch is still a silent behaviour switch, so a short line always says
  // what they currently do.
  function snipEditUpdateNudgeHint() {
    snipEditNudgeHintEl.classList.toggle("hidden", !SE.lastNudged);
  }
  function snipEditNudge(op, key, amount, resize) {
    const before = snipEditGeom(op);
    const dx = key === "ArrowLeft" ? -amount : key === "ArrowRight" ? amount : 0;
    const dy = key === "ArrowUp" ? -amount : key === "ArrowDown" ? amount : 0;
    if (resize) {
      // Only a rect has a size to change; a line/arrow/pen/text/step has none,
      // so Ctrl+Arrow is simply a no-op for those instead of guessing at what
      // "bigger" would mean for a bare pair of points.
      if (!op.rect) return;
      op.rect.w = Math.max(4, op.rect.w + dx);
      op.rect.h = Math.max(4, op.rect.h + dy);
    } else {
      if (op.points) for (const p of op.points) { p.x += dx; p.y += dy; }
      if (op.point) { op.point.x += dx; op.point.y += dy; }
      if (op.rect) { op.rect.x += dx; op.rect.y += dy; }
    }
    // One undo step per run of nudges: a run on the same op and tool folds into the
    // entry just below the top; a different op or tool, or any other action in
    // between, starts a new one.
    const last = SE.ops[SE.ops.length - 1];
    if (last && last.tool === "edit" && last.kind === "geom" && last.nudgeOf === op && last.nudgeTool === SE.tool) {
      last.after = snipEditGeom(op);
      // A new edit ends the redo branch, like every pushed op does.
      if (SE.redo.length) { SE.redo.length = 0; snipEditUpdateUndoRedoButtons(); }
      snipEditRedrawBase();
    } else {
      const after = snipEditGeom(op);
      if (JSON.stringify(before) === JSON.stringify(after)) { snipEditRedrawBase(); }
      else snipEditPushOp({ tool: "edit", kind: "geom", op, before, after, nudgeOf: op, nudgeTool: SE.tool }, op);
    }
    // The move/resize above only touched the op's own coordinates -- the
    // selection handles are a separate live-layer drawing keyed to the same
    // op, and would otherwise keep showing the pre-nudge position/size.
    if (SE.selOp === op) snipEditDrawShapeSelectionLive();
    // The crop tool's own live overlay (handles + dim) is drawn from SE.cropRect,
    // not from the committed op -- without this, nudging a just-committed crop
    // moves what gets exported while the on-screen handles stay put, so Apply/
    // Copy would silently produce an image cropped from a region never shown.
    if (op.tool === "crop" && op.rect) {
      SE.cropRect = { ...op.rect };
      snipEditDrawCropOverlayLive();
    }
  }

  // ---- crop: a rect operation, only applied to pixels at export time ----
  function snipEditCropHandlePoints(r) {
    return {
      nw: { x: r.x, y: r.y }, n: { x: r.x + r.w / 2, y: r.y }, ne: { x: r.x + r.w, y: r.y },
      w: { x: r.x, y: r.y + r.h / 2 }, e: { x: r.x + r.w, y: r.y + r.h / 2 },
      sw: { x: r.x, y: r.y + r.h }, s: { x: r.x + r.w / 2, y: r.y + r.h }, se: { x: r.x + r.w, y: r.y + r.h },
    };
  }
  function snipEditHitCropHandle(pt, r) {
    const tol = 10 / snipEditScale();
    for (const [name, p] of Object.entries(snipEditCropHandlePoints(r))) {
      if (Math.abs(pt.x - p.x) <= tol && Math.abs(pt.y - p.y) <= tol) return name;
    }
    // Past the handles, every edge is grabbable along its whole length.
    const edge = 8 / snipEditScale();
    const inX = pt.x >= r.x - edge && pt.x <= r.x + r.w + edge;
    const inY = pt.y >= r.y - edge && pt.y <= r.y + r.h + edge;
    if (inX && Math.abs(pt.y - r.y) <= edge) return "n";
    if (inX && Math.abs(pt.y - (r.y + r.h)) <= edge) return "s";
    if (inY && Math.abs(pt.x - r.x) <= edge) return "w";
    if (inY && Math.abs(pt.x - (r.x + r.w)) <= edge) return "e";
    return null;
  }
  // The crop box always spans the image until the user pulls an edge in; a box equal
  // to the whole image is no crop at all.
  function snipEditFullCropRect() { return { x: 0, y: 0, w: SE.w, h: SE.h }; }
  function snipEditCropIsFull(r) { return !!r && r.x === 0 && r.y === 0 && r.w === SE.w && r.h === SE.h; }
  // Same handle-to-cursor mapping the video editor already uses for its own
  // crop/overlay handles (video.css .ve-h-*): a directional resize cursor at
  // each corner/edge and a move cursor inside a box smaller than the image.
  const SNIP_CROP_HANDLE_CURSORS = {
    nw: "nwse-resize", n: "ns-resize", ne: "nesw-resize",
    w: "ew-resize", e: "ew-resize",
    sw: "nesw-resize", s: "ns-resize", se: "nwse-resize",
  };
  function snipEditCropCursorFor(pt) {
    const r = SE.cropRect;
    if (!r) return "crosshair";
    const handle = snipEditHitCropHandle(pt, r);
    if (handle) return SNIP_CROP_HANDLE_CURSORS[handle];
    if (!snipEditCropIsFull(r) && pt.x >= r.x && pt.x <= r.x + r.w && pt.y >= r.y && pt.y <= r.y + r.h) return "move";
    return "default";
  }
  function snipEditDrawCropOverlayLive() {
    snipEditClearLive();
    const r = SE.cropRect;
    if (!r) return;
    // Heavier dim once the box is at rest, so the picture reads as cropped; lighter
    // while an edge is being dragged, so what would come back stays visible.
    snipEditDrawCropDim(snipEditLiveCtx, r, SE.drag ? 0.55 : 0.85);
    const hs = 8;
    snipEditLiveCtx.save();
    snipEditLiveCtx.fillStyle = "#fff";
    snipEditLiveCtx.strokeStyle = "#2563eb";
    snipEditLiveCtx.lineWidth = 1.5;
    for (const p of Object.values(snipEditCropHandlePoints(r))) {
      snipEditLiveCtx.fillRect(p.x - hs / 2, p.y - hs / 2, hs, hs);
      snipEditLiveCtx.strokeRect(p.x - hs / 2, p.y - hs / 2, hs, hs);
    }
    snipEditLiveCtx.restore();
    snipEditDrawGuides();
  }
  // While the crop tool is active SE.cropRect is the editable copy of the crop op;
  // whatever changes SE.ops behind its back (undo, redo, a resize) must pull it
  // back in line, or the next export commits the stale rectangle again.
  function snipEditSyncCropFromOps() {
    if (SE.tool !== "crop") return;
    const existing = SE.ops.find((o) => o.tool === "crop");
    SE.cropRect = existing ? { ...existing.rect } : snipEditFullCropRect();
    snipEditDrawCropOverlayLive();
  }
  function snipEditCropPointerDown(pt) {
    const r = SE.cropRect;
    if (r) {
      const handle = snipEditHitCropHandle(pt, r);
      if (handle) { SE.drag = { kind: "crop-resize", handle, orig: { ...r } }; return; }
      // A box smaller than the image can be moved; a press anywhere else does nothing.
      if (!snipEditCropIsFull(r) && pt.x >= r.x && pt.x <= r.x + r.w && pt.y >= r.y && pt.y <= r.y + r.h) {
        SE.drag = { kind: "crop-move", start: pt, orig: { ...r } };
      }
    }
  }
  function snipEditResizeCropRect(orig, handle, pt, shift) {
    let x = orig.x, y = orig.y, x2 = orig.x + orig.w, y2 = orig.y + orig.h;
    if (handle.includes("n")) y = pt.y;
    if (handle.includes("s")) y2 = pt.y;
    if (handle.includes("w")) x = pt.x;
    if (handle.includes("e")) x2 = pt.x;
    const nr = { x: Math.min(x, x2), y: Math.min(y, y2), w: Math.abs(x2 - x), h: Math.abs(y2 - y) };
    if (shift && orig.w && orig.h) {
      const ratio = orig.w / orig.h;
      if (handle === "n" || handle === "s") nr.w = nr.h * ratio; else nr.h = nr.w / ratio;
      // The edge opposite a top handle is the anchor: re-derive the top from it, or
      // the new height would push that fixed edge around.
      if (handle.includes("n")) nr.y = y <= y2 ? y2 - nr.h : y2;
    }
    return nr;
  }
  function snipEditCropPointerMove(pt, shift) {
    const d = SE.drag;
    if (d.kind === "crop-move") {
      const dx = pt.x - d.start.x, dy = pt.y - d.start.y;
      SE.cropRect = {
        x: snipEditClamp(d.orig.x + dx, 0, SE.w - d.orig.w),
        y: snipEditClamp(d.orig.y + dy, 0, SE.h - d.orig.h),
        w: d.orig.w, h: d.orig.h,
      };
    } else if (d.kind === "crop-resize") {
      SE.cropRect = snipEditResizeCropRect(d.orig, d.handle, pt, shift);
    }
    snipEditDrawCropOverlayLive();
  }
  function snipEditCropPointerUp() {
    if (SE.cropRect && (SE.cropRect.w < 8 || SE.cropRect.h < 8)) {
      const committed = SE.ops.find((o) => o.tool === "crop");
      SE.cropRect = committed ? { ...committed.rect } : snipEditFullCropRect(); // "smaller than 8x8 -> ignore"
    }
    // The drag is the edit: it becomes the (undoable) crop op right away and the view
    // follows the new box.
    snipEditCropCommit();
    snipEditFitToCrop();
    snipEditDrawCropOverlayLive();
  }
  function snipEditCropCommit() {
    const r = SE.cropRect;
    if (!r || r.w < 8 || r.h < 8) return;
    const existing = SE.ops.find((o) => o.tool === "crop");
    if (snipEditCropIsFull(r)) {
      // A box equal to the image is no crop: no op is added, and an earlier crop that the
      // box was dragged back out of is taken away again (undoable like any removal).
      if (existing) snipEditRemoveOp(existing);
      return;
    }
    if (existing && JSON.stringify(existing.rect) === JSON.stringify(r)) return;
    snipEditPushUniqueOp("crop", { tool: "crop", rect: { ...r } });
  }
  // Zooms so the crop box fills the view (width or height, whichever limits) and
  // centres it; a box that spans the whole image is the plain fit.
  function snipEditFitToCrop() {
    const r = SE.cropRect;
    if (!r || snipEditCropIsFull(r)) { snipEditFit(); return; }
    const vw = snipEditWrap.clientWidth, vh = snipEditWrap.clientHeight;
    snipEditSetZoom(Math.min(vw / r.w, vh / r.h) * (window.devicePixelRatio || 1));
    const wr = snipEditWrap.getBoundingClientRect(), sr = snipEditStage.getBoundingClientRect();
    const k = snipEditScale();
    snipEditWrap.scrollLeft += sr.left + (r.x + r.w / 2) * k - (wr.left + snipEditWrap.clientLeft + vw / 2);
    snipEditWrap.scrollTop += sr.top + (r.y + r.h / 2) * k - (wr.top + snipEditWrap.clientTop + vh / 2);
  }
  // Esc while the crop tool is active discards only the crop, not the whole editor.
  function snipEditCropDiscard() {
    SE.drag = null;
    SE.cropRect = snipEditFullCropRect();
    // Same floor adjustment as snipEditPushUniqueOp: only crops below the floor shift it.
    SE.floor = Math.max(0, SE.floor - SE.ops.slice(0, SE.floor).filter((o) => o.tool === "crop").length);
    SE.ops = SE.ops.filter((o) => o.tool !== "crop");
    snipEditUpdateUndoRedoButtons();
    snipEditClearLive();
    snipEditRedrawBase();
    snipEditFit();
    snipEditDrawCropOverlayLive();
  }

  // ---- shape selection: a click on an existing rect/ellipse/line/arrow, made
  // while that same tool is active, selects it instead of starting a new one --
  // the same 8-handle box the crop tool already draws (2 endpoint handles for a
  // line/arrow). Pen, marker, pixelate, spotlight, text and step keep their prior
  // behaviour; text already has its own move-only selection above. ----
  const SNIP_SHAPE_TOOLS = ["rect", "ellipse", "line", "arrow"];
  function snipEditShapeBounds(op) {
    if (op.rect) return op.rect;
    const [a, b] = op.points;
    return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) };
  }
  function snipEditDistToSegment(pt, a, b) {
    const dx = b.x - a.x, dy = b.y - a.y;
    const lenSq = dx * dx + dy * dy;
    const t = lenSq ? snipEditClamp(((pt.x - a.x) * dx + (pt.y - a.y) * dy) / lenSq, 0, 1) : 0;
    return snipEditDist(pt, { x: a.x + t * dx, y: a.y + t * dy });
  }
  function snipEditHitTestShape(pt) {
    if (!SNIP_SHAPE_TOOLS.includes(SE.tool)) return null;
    const tol = 8 / snipEditScale();
    for (let i = SE.ops.length - 1; i >= 0; i--) {
      const op = SE.ops[i];
      if (op.tool !== SE.tool) continue;
      if (op.points) {
        if (snipEditDistToSegment(pt, op.points[0], op.points[1]) <= tol) return op;
        continue;
      }
      const r = op.rect;
      const inBox = pt.x >= r.x - tol && pt.x <= r.x + r.w + tol && pt.y >= r.y - tol && pt.y <= r.y + r.h + tol;
      if (!inBox) continue;
      const onBorder = Math.abs(pt.x - r.x) <= tol || Math.abs(pt.x - r.x - r.w) <= tol
        || Math.abs(pt.y - r.y) <= tol || Math.abs(pt.y - r.y - r.h) <= tol;
      const inside = op.filled && pt.x >= r.x && pt.x <= r.x + r.w && pt.y >= r.y && pt.y <= r.y + r.h;
      if (onBorder || inside) return op;
    }
    return null;
  }
  function snipEditShapeHandlePoints(op) {
    if (op.points) return { a: op.points[0], b: op.points[1] };
    return snipEditCropHandlePoints(op.rect);
  }
  function snipEditHitShapeHandle(pt, op) {
    const tol = 10 / snipEditScale();
    for (const [name, p] of Object.entries(snipEditShapeHandlePoints(op))) {
      if (Math.abs(pt.x - p.x) <= tol && Math.abs(pt.y - p.y) <= tol) return name;
    }
    return null;
  }
  function snipEditShapeCursorFor(pt) {
    const op = SE.selOp;
    if (!op || op.tool !== SE.tool) return "";
    const handle = snipEditHitShapeHandle(pt, op);
    if (handle) return op.points ? "move" : SNIP_CROP_HANDLE_CURSORS[handle];
    const r = snipEditShapeBounds(op);
    if (pt.x >= r.x && pt.x <= r.x + r.w && pt.y >= r.y && pt.y <= r.y + r.h) return "move";
    return "";
  }
  function snipEditPaintShapeHandles(op) {
    const hs = 8;
    snipEditLiveCtx.save();
    snipEditLiveCtx.fillStyle = "#fff";
    snipEditLiveCtx.strokeStyle = "#2563eb";
    snipEditLiveCtx.lineWidth = 1.5;
    for (const p of Object.values(snipEditShapeHandlePoints(op))) {
      snipEditLiveCtx.fillRect(p.x - hs / 2, p.y - hs / 2, hs, hs);
      snipEditLiveCtx.strokeRect(p.x - hs / 2, p.y - hs / 2, hs, hs);
    }
    snipEditLiveCtx.restore();
  }
  function snipEditDrawShapeSelectionLive() {
    snipEditClearLive();
    if (SE.selOp) snipEditPaintShapeHandles(SE.selOp);
  }
  // Drag-preview ghost: drawn on the live layer at the new position while the
  // committed op (old position) stays visible underneath on the base canvas
  // until pointerup -- the same simplification already used for a text move.
  function snipEditDrawShapeDragLive(op) {
    snipEditClearLive();
    snipEditDrawOp(snipEditLiveCtx, op);
    snipEditPaintShapeHandles(op);
  }
  function snipEditSelectShape(op) {
    SE.selOp = op;
    SE.lastNudged = op;
    snipEditUpdateNudgeHint();
    snipEditDrawShapeSelectionLive();
  }
  function snipEditDeselectShape() {
    SE.lastNudged = null;
    snipEditUpdateNudgeHint();
    if (!SE.selOp) return;
    SE.selOp = null;
    snipEditClearLive();
  }

  // ---- duplicate / delete via right-click. Independent of the active tool: a
  // right-click always tests every duplicable kind, not only SE.tool's, so the
  // menu works for whatever shape sits under the pointer. ----
  const SNIP_CTX_RECT_KINDS = ["rect", "ellipse", "spotlight", "pixelate"];
  function snipEditHitTestForCtx(pt) {
    const tol = 8 / snipEditScale();
    for (let i = SE.ops.length - 1; i >= 0; i--) {
      const op = SE.ops[i];
      if (op.tool === "text") {
        const w = snipEditMeasureTextWidth(op);
        const h = op.fontPx * 1.3;
        if (pt.x >= op.point.x && pt.x <= op.point.x + w && pt.y >= op.point.y - 2 && pt.y <= op.point.y + h) return op;
        continue;
      }
      if (op.tool === "line" || op.tool === "arrow") {
        if (snipEditDistToSegment(pt, op.points[0], op.points[1]) <= tol) return op;
        continue;
      }
      if (!SNIP_CTX_RECT_KINDS.includes(op.tool)) continue;
      const r = op.rect;
      if (!r) continue;
      const inBox = pt.x >= r.x - tol && pt.x <= r.x + r.w + tol && pt.y >= r.y - tol && pt.y <= r.y + r.h + tol;
      if (!inBox) continue;
      // Spotlight/pixelate have no outline-only edge -- the whole rect is the target.
      if (op.tool === "spotlight" || op.tool === "pixelate") return op;
      const onBorder = Math.abs(pt.x - r.x) <= tol || Math.abs(pt.x - r.x - r.w) <= tol
        || Math.abs(pt.y - r.y) <= tol || Math.abs(pt.y - r.y - r.h) <= tol;
      const inside = op.filled && pt.x >= r.x && pt.x <= r.x + r.w && pt.y >= r.y && pt.y <= r.y + r.h;
      if (onBorder || inside) return op;
    }
    return null;
  }
  // A fresh push at the end, never spliced next to the source -- snipEditUndo()
  // always pops the last entry, so anything inserted mid-array would make undo
  // remove the wrong operation.
  function snipEditDuplicateOp(op) {
    if (op.tool === "spotlight") return; // only one spotlight may exist, there is nothing to copy
    const offset = SNIP_DUP_OFFSET_PX;
    const dup = JSON.parse(JSON.stringify(op));
    if (dup.rect) {
      dup.rect = { ...dup.rect, x: snipEditClamp(dup.rect.x + offset, 0, SE.w - dup.rect.w), y: snipEditClamp(dup.rect.y + offset, 0, SE.h - dup.rect.h) };
    } else if (dup.points) {
      dup.points = dup.points.map((p) => ({ x: snipEditClamp(p.x + offset, 0, SE.w), y: snipEditClamp(p.y + offset, 0, SE.h) }));
    } else if (dup.point) {
      dup.point = { x: snipEditClamp(dup.point.x + offset, 0, SE.w), y: snipEditClamp(dup.point.y + offset, 0, SE.h) };
    }
    snipEditPushOp(dup);
  }
  // Removes one specific op wherever it sits in SE.ops. The removal is an undo
  // entry too, so Ctrl+Z brings the op back at its old place.
  function snipEditRemoveOp(op) {
    const index = SE.ops.indexOf(op);
    if (index < 0) return;
    SE.ops.splice(index, 1);
    // The floor is a count: an op below it leaving shifts it down by one. Only that
    // delta is stored, so a crop commit or discard in between cannot be undone by
    // restoring an absolute value.
    const floorShift = index < SE.floor ? 1 : 0;
    SE.floor -= floorShift;
    if (SE.selOp === op) snipEditDeselectShape();
    snipEditPushOp({ tool: "edit", kind: "remove", op, prev: SE.ops[index - 1] || null, next: SE.ops[index] || null, floorShift }, null);
  }

  // ---- text: a real <input> positioned over the canvas, in stage-local (i.e.
  // already-zoomed) pixels, since the input shares the stage's containing block. ----
  function snipEditHitTestText(pt) {
    for (let i = SE.ops.length - 1; i >= 0; i--) {
      const op = SE.ops[i];
      if (op.tool !== "text") continue;
      const w = snipEditMeasureTextWidth(op);
      const h = op.fontPx * 1.3;
      if (pt.x >= op.point.x && pt.x <= op.point.x + w && pt.y >= op.point.y - 2 && pt.y <= op.point.y + h) return op;
    }
    return null;
  }
  function snipEditOpenTextInput(pt) {
    const ts = snipEditToolState("text");
    SE.textEdit = { point: pt };
    snipEditTextInput.value = "";
    snipEditTextInput.style.left = `${pt.x * snipEditScale()}px`;
    snipEditTextInput.style.top = `${pt.y * snipEditScale()}px`;
    snipEditTextInput.style.fontSize = `${ts.fontPx * snipEditScale()}px`;
    snipEditTextInput.style.color = ts.color;
    snipEditTextInput.classList.remove("hidden");
    snipEditTextInput.focus();
  }
  function snipEditCommitTextInput() {
    if (!SE.textEdit) return;
    const { point } = SE.textEdit;
    SE.textEdit = null;
    const typed = snipEditTextInput.value;
    snipEditTextInput.classList.add("hidden");
    if (!typed.trim()) return; // empty input -> discard the operation
    const ts = snipEditToolState("text");
    snipEditPushOp({ tool: "text", point, text: typed, color: ts.color, fontPx: ts.fontPx, back: ts.back, outlinePx: ts.outlinePx });
  }
  function snipEditCancelTextInput() {
    SE.textEdit = null;
    snipEditTextInput.classList.add("hidden");
  }
  function snipEditFinishTextIfOpen() { if (SE.textEdit) snipEditCommitTextInput(); }
  function snipEditTextPointerDown(pt) {
    snipEditFinishTextIfOpen();
    const hit = snipEditHitTestText(pt);
    if (hit) { SE.drag = { kind: "text-move", op: hit, dx: pt.x - hit.point.x, dy: pt.y - hit.point.y }; return; }
    snipEditOpenTextInput(pt);
  }

  // ---- contextual (tool-specific) toolbar: only what the active tool understands ----
  function snipEditBuildWidthField(tool, ts) {
    // A plain div, not a <label>: the S/M/L buttons below sit before the
    // exact-value field, so a <label> here would forward a click on the
    // "Width" text to the first of THEM, not to the number field -- the
    // field gets its own aria-label instead (see the .setAttribute below).
    const wrap = document.createElement("div");
    wrap.className = "snip-edit-field";
    wrap.append(Object.assign(document.createElement("span"), { textContent: t("snipEditWidth") }));
    const row = document.createElement("div");
    row.className = "snip-edit-width-row";
    const presets = SNIP_WIDTH_PRESETS_OF(tool);
    // Three visibly different stroke weights read at a glance -- the S/M/L
    // letters said nothing about WHICH was thicker without reading the tooltip.
    const STROKE_ICON_W = [2.5, 5, 8];
    ["snipEditWidthS", "snipEditWidthM", "snipEditWidthL"].forEach((lab, i) => {
      const px = presets[i];
      const b = document.createElement("button");
      b.type = "button";
      b.className = "snip-edit-width-btn" + (ts.width === px ? " sel" : "");
      b.innerHTML = `<svg viewBox="0 0 20 12" width="20" height="12" aria-hidden="true"><line x1="2" y1="6" x2="18" y2="6" stroke="currentColor" stroke-width="${STROKE_ICON_W[i]}" stroke-linecap="round"/></svg>`;
      b.title = `${t(lab)} (${px}px)`;
      b.setAttribute("aria-label", `${t(lab)} (${px}px)`);
      b.addEventListener("click", () => { ts.width = px; snipRememberSize(tool, "width", px); snipEditRenderContext(); });
      row.appendChild(b);
    });
    wrap.appendChild(row);
    const control = numberField({
      value: ts.width, min: 1, max: 80, step: 1, steppers: false,
      onChange: (v) => { ts.width = v; snipRememberSize(tool, "width", v); snipEditRenderContext(); },
    });
    const num = control.querySelector("input");
    num.title = t("snipEditWidthTip");
    num.setAttribute("aria-label", t("snipEditWidthTip"));
    wrap.appendChild(control);
    return wrap;
  }
  // Single trigger button (current color) opens a popover holding the full
  // palette + eyedropper, so the toolbar stays one row tall regardless of
  // palette size (a full inline grid there used to force it multi-row).
  const snipColorPop = document.createElement("div");
  snipColorPop.className = "snip-color-pop hidden";
  document.body.appendChild(snipColorPop);
  function closeSnipColorPop() {
    snipColorPop.classList.add("hidden");
  }
  function snipColorPopIsOpen() {
    return !snipColorPop.classList.contains("hidden");
  }
  function openSnipColorPop(anchor, tool, ts) {
    snipColorPop.innerHTML = "";
    const sw = document.createElement("div");
    sw.className = "swatches";
    buildSwatches(sw, ts.color,
      (hex) => { ts.color = hex; snipEditRenderContext(); closeSnipColorPop(); },
      (anchor2, cur) => openColorPop(anchor2, cur, (hex) => { ts.color = hex; snipEditRenderContext(); }));
    snipColorPop.appendChild(sw);
    snipColorPop.classList.remove("hidden");
    placePopover(snipColorPop, anchor.getBoundingClientRect(), 6);
  }
  document.addEventListener("pointerdown", (e) => {
    if (!snipColorPopIsOpen()) return;
    if (snipColorPop.contains(e.target)) return;
    if (!colorPop.classList.contains("hidden") && colorPop.contains(e.target)) return;
    if (e.target.closest(".snip-edit-color-trigger")) return;
    closeSnipColorPop();
  });
  function snipEditBuildColorField(tool, ts) {
    const wrap = document.createElement("div");
    wrap.className = "snip-edit-field";
    const trigger = document.createElement("button");
    trigger.type = "button";
    trigger.className = "snip-edit-color-trigger";
    trigger.title = t("snipEditColorTip");
    trigger.setAttribute("aria-label", t("snipEditColor"));
    const chip = document.createElement("span");
    chip.className = "snip-edit-color-chip" + (ts.color ? "" : " none");
    if (ts.color) chip.style.background = ts.color;
    trigger.appendChild(chip);
    trigger.addEventListener("click", () => openSnipColorPop(trigger, tool, ts));
    wrap.appendChild(trigger);
    const eye = document.createElement("button");
    eye.type = "button";
    eye.className = "icon-btn snip-edit-eyedrop" + (SE.eyedropArm === tool ? " active" : "");
    eye.title = t("snipEditEyedropTip");
    eye.setAttribute("aria-label", t("snipEditEyedrop"));
    eye.innerHTML = SNIP_EYEDROP_SVG;
    eye.addEventListener("click", () => { SE.eyedropArm = tool; snipEditRenderContext(); });
    wrap.appendChild(eye);
    return wrap;
  }
  function snipEditBuildToggle(labelText, checked, onChange) {
    const wrap = document.createElement("label");
    wrap.className = "snip-edit-toggle";
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = checked;
    cb.addEventListener("change", () => onChange(cb.checked));
    wrap.append(cb, Object.assign(document.createElement("span"), { textContent: labelText }));
    return wrap;
  }
  function snipEditBuildSelectField(labelText, value, options, onChange) {
    const wrap = document.createElement("label");
    wrap.className = "snip-edit-field";
    wrap.append(Object.assign(document.createElement("span"), { textContent: labelText }));
    const select = document.createElement("select");
    select.className = "modal-input snip-edit-select";
    for (const [v, label] of options) {
      select.appendChild(Object.assign(document.createElement("option"), { value: v, textContent: label }));
    }
    select.value = value;
    select.addEventListener("change", () => onChange(select.value));
    attachSelectPicker(select);
    wrap.appendChild(select);
    return wrap;
  }
  function snipEditBuildNumField(labelText, value, min, max, title, onChange, presets) {
    // A real <label> (not a plain div) so the input keeps an accessible name
    // from its own visible text -- see the numFieldDec/-Inc/-List buttons
    // below for why they need an explicit aria-label instead.
    const wrap = document.createElement("label");
    wrap.className = "snip-edit-field";
    wrap.append(Object.assign(document.createElement("span"), { textContent: labelText }));
    const control = numberField({
      value, min, max, step: 1, presets, onChange,
      decreaseLabel: t("numFieldDecrease"), increaseLabel: t("numFieldIncrease"), presetsLabel: t("numFieldPresets"),
      decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease", presetsAriaKey: "numFieldPresets",
    });
    control.querySelector("input").title = title;
    wrap.appendChild(control);
    return wrap;
  }
  function snipEditBuildDimField(ts) {
    const wrap = document.createElement("div");
    wrap.className = "snip-edit-field";
    wrap.append(Object.assign(document.createElement("span"), { textContent: t("snipEditDim") }));
    const range = document.createElement("input");
    range.type = "range";
    range.min = "10";
    range.max = "90";
    range.step = "5";
    range.value = String(Math.round(ts.dim * 100));
    range.title = t("snipEditDimTip");
    range.addEventListener("input", () => { ts.dim = Number(range.value) / 100; snipRememberSize("spotlight", "dim", ts.dim); });
    wrap.appendChild(range);
    return wrap;
  }
  // The tool's controls live in a panel under its button. Only the tools with fields
  // get one; the hand and crop tools have nothing to show.
  const SNIP_CTX_INSIDE = "#snip-edit-context, .snip-color-pop, .color-pop, .select-pop, .numfield-pop";
  const snipEditToolHasPanel = (tool) => tool !== "hand" && tool !== "crop";
  function snipEditContextIsOpen() { return !snipEditContextEl.classList.contains("hidden"); }
  function closeSnipEditContext() {
    snipEditContextEl.classList.add("hidden");
    closeSnipColorPop();
  }
  function snipEditPlaceContext() {
    const btn = snipEditToolsEl.querySelector("button.active");
    if (btn) placePopover(snipEditContextEl, btn.getBoundingClientRect(), 6);
  }
  function snipEditOpenContext() {
    if (!snipEditToolHasPanel(SE.tool) || !snipEditContextEl.firstElementChild) { closeSnipEditContext(); return; }
    snipEditPlaceContext();
  }
  // One listener for the whole editor's lifetime. Capture phase, so the check runs
  // before a control inside the panel rebuilds the panel (a rebuilt control is no
  // longer attached and would look like an outside press). A press on a tool
  // button is handled by that button's own click.
  document.addEventListener("pointerdown", (e) => {
    if (!snipEditContextIsOpen()) return;
    if (e.target.closest?.(SNIP_CTX_INSIDE) || e.target.closest?.("#snip-edit-tools button")) return;
    closeSnipEditContext();
  }, true);
  window.addEventListener("resize", () => { if (SE.active && snipEditContextIsOpen()) snipEditPlaceContext(); });
  function snipEditRenderContext() {
    const box = snipEditContextEl;
    box.innerHTML = "";
    const tool = SE.tool;
    if (!snipEditToolHasPanel(tool)) { closeSnipEditContext(); return; }
    const ts = snipEditToolState(tool);
    const hasWidth = ["pen", "marker", "line", "arrow", "rect", "ellipse", "step"].includes(tool);
    if (hasWidth) box.appendChild(snipEditBuildWidthField(tool, ts));
    if (hasWidth || tool === "text") box.appendChild(snipEditBuildColorField(tool, ts));
    if (tool === "rect" || tool === "ellipse") {
      box.appendChild(snipEditBuildToggle(t("snipEditFilled"), !!ts.filled, (v) => { ts.filled = v; }));
    }
    if (tool === "text") {
      box.appendChild(snipEditBuildNumField(t("snipEditTextSize"), ts.fontPx, 10, 96, t("snipEditTextSizeTip"), (v) => { ts.fontPx = v; snipRememberSize("text", "fontPx", v); }, [12, 16, 24, 32, 48, 64, 96]));
      box.appendChild(snipEditBuildSelectField(t("snipEditTextBack"), ts.back, [
        ["none", t("optSnipTextBackNone")], ["plate", t("optSnipTextBackPlate")], ["outline", t("optSnipTextBackOutline")],
      ], (v) => { ts.back = v; snipEditRenderContext(); }));
      if (ts.back === "outline") {
        box.appendChild(snipEditBuildNumField(t("snipEditTextOutline"), ts.outlinePx, 1, 12, t("snipEditTextOutlineTip"), (v) => { ts.outlinePx = v; snipRememberSize("text", "outlinePx", v); }));
      }
    }
    if (tool === "pixelate") {
      box.appendChild(snipEditBuildNumField(t("snipEditBlockSize"), ts.blockPx, 4, 64, t("snipEditBlockSizeTip"), (v) => { ts.blockPx = v; snipRememberSize("pixelate", "blockPx", v); }, [8, 12, 16, 24, 32]));
    }
    if (tool === "spotlight") box.appendChild(snipEditBuildDimField(ts));
    if (snipEditContextIsOpen()) snipEditPlaceContext();
  }

  // ---- toolbar (left): one tool active at a time ----
  function snipEditBuildToolButtons() {
    snipEditToolsEl.innerHTML = "";
    for (const tdef of SNIP_TOOLS) {
      // Dividers: between the drawing tools (pen..step) and the image tools
      // (pixelate/spotlight) -- same grouping the Snipping Tool uses -- and
      // again before the view tool (hand), which does not draw anything.
      if (tdef.id === "pixelate" || tdef.id === "hand") {
        const sep = document.createElement("span");
        sep.className = "snip-edit-sep";
        sep.setAttribute("aria-hidden", "true");
        snipEditToolsEl.appendChild(sep);
      }
      const b = document.createElement("button");
      b.type = "button";
      b.className = "icon-btn";
      b.dataset.tool = tdef.id;
      b.setAttribute("aria-label", t(tdef.label));
      b.setAttribute("aria-pressed", "false");
      b.innerHTML = SNIP_TOOL_SVG[tdef.id];
      withShortcut(b, `snipTool.${tdef.id}`);
      b.addEventListener("click", () => {
        // A second click on the active tool toggles its panel.
        if (SE.tool === tdef.id && !SE.drag) {
          if (snipEditContextIsOpen()) closeSnipEditContext(); else snipEditOpenContext();
          return;
        }
        snipEditSetTool(tdef.id);
      });
      snipEditToolsEl.appendChild(b);
    }
  }
  function snipEditRenderToolButtons() {
    for (const b of snipEditToolsEl.children) {
      const on = b.dataset.tool === SE.tool;
      b.classList.toggle("active", on);
      b.setAttribute("aria-pressed", String(on));
    }
  }
  function snipEditSetTool(tool) {
    if (SE.drag) return;
    snipEditFinishTextIfOpen();
    SE.eyedropArm = null;
    closeSnipEditContext();
    SE.tool = tool;
    SE.lastNudged = null;
    snipEditUpdateNudgeHint();
    snipEditClearLive();
    // Clear any crop-hover cursor left over from the previous tool -- an
    // inline style would otherwise outrank the panning class below and the
    // per-tool crosshair default.
    snipEditWrap.style.cursor = "";
    // A grab cursor as soon as the Hand tool is picked, not only once a
    // drag actually starts -- the same affordance Space-hold already gets.
    snipEditWrap.classList.toggle("panning", tool === "hand");
    snipEditSyncCropFromOps();
    snipEditRenderToolButtons();
    snipEditRenderContext();
    snipEditOpenContext();
    snipEditRedrawBase(); // toggles the crop-dim pass on/off for the new tool
    snipRememberTool(tool);
  }

  // ---- live previews for the currently-dragging tool (top canvas only) ----
  function snipEditDrawLivePenStroke() {
    snipEditClearLive();
    const ts = snipEditToolState(SE.tool);
    const op = { color: ts.color, width: ts.width, points: SE.drag.points };
    if (SE.tool === "marker") snipEditDrawMarkerOp(snipEditLiveCtx, op); else snipEditDrawPenOp(snipEditLiveCtx, op);
    snipEditDrawGuides();
  }
  function snipEditDrawLiveLineArrow() {
    snipEditClearLive();
    const ts = snipEditToolState(SE.tool);
    const op = { color: ts.color, width: ts.width, points: [SE.drag.a, SE.drag.b] };
    if (SE.tool === "arrow") snipEditDrawArrowOp(snipEditLiveCtx, op); else snipEditDrawLineOp(snipEditLiveCtx, op);
    snipEditDrawGuides();
  }
  function snipEditDrawLiveRectEllipse() {
    snipEditClearLive();
    const ts = snipEditToolState(SE.tool);
    const op = { color: ts.color, width: ts.width, rect: snipEditNormRect(SE.drag), filled: !!ts.filled };
    if (SE.tool === "ellipse") snipEditDrawEllipseOp(snipEditLiveCtx, op); else snipEditDrawRectOp(snipEditLiveCtx, op);
    snipEditDrawGuides();
  }
  function snipEditDrawLivePixelatePreview() {
    snipEditClearLive();
    const ts = snipEditToolState("pixelate");
    snipEditDrawPixelate(snipEditLiveCtx, snipEditNormRect(SE.drag), ts.blockPx);
    snipEditDrawGuides();
  }
  function snipEditDrawLiveSpotlightPreview() {
    snipEditClearLive();
    const ts = snipEditToolState("spotlight");
    snipEditDrawSpotlight(snipEditLiveCtx, snipEditNormRect(SE.drag), ts.dim);
    snipEditDrawGuides();
  }

  function snipEditShowMeasure(e, rect) {
    const r = snipEditNormRect(rect);
    snipEditMeasureEl.textContent = `${Math.round(r.w)} × ${Math.round(r.h)}`;
    const wr = snipEditWrap.getBoundingClientRect();
    snipEditMeasureEl.style.left = `${e.clientX - wr.left + snipEditWrap.scrollLeft + 14}px`;
    snipEditMeasureEl.style.top = `${e.clientY - wr.top + snipEditWrap.scrollTop + 14}px`;
    snipEditMeasureEl.classList.remove("hidden");
  }
  function snipEditHideMeasure() { snipEditMeasureEl.classList.add("hidden"); }

  function snipEditApplyShiftConstraint(drag, pt) {
    if (drag.kind === "line" || drag.kind === "arrow") {
      const dx = pt.x - drag.a.x, dy = pt.y - drag.a.y;
      const dist = Math.hypot(dx, dy);
      const angle = Math.round(Math.atan2(dy, dx) / (Math.PI / 12)) * (Math.PI / 12); // 15deg steps
      return { x: drag.a.x + Math.cos(angle) * dist, y: drag.a.y + Math.sin(angle) * dist };
    }
    if (drag.kind === "rect" || drag.kind === "ellipse") {
      const dx = pt.x - drag.a.x, dy = pt.y - drag.a.y;
      const s = Math.max(Math.abs(dx), Math.abs(dy));
      return { x: drag.a.x + Math.sign(dx || 1) * s, y: drag.a.y + Math.sign(dy || 1) * s };
    }
    return pt;
  }

  // ---- pan (space+drag or middle mouse) ----
  function snipEditStartPan(e) {
    SE.panState = { x: e.clientX, y: e.clientY, sl: snipEditWrap.scrollLeft, st: snipEditWrap.scrollTop };
    // Without capture, dragging past the canvas edge (zoomed in beyond the
    // viewport) stops delivering pointermove once the cursor leaves it.
    snipEditLiveCv.setPointerCapture(e.pointerId);
    // A leftover crop-tool inline cursor otherwise outranks the grab/grabbing
    // cursor below for this whole drag (only the Space-key path cleared it).
    snipEditWrap.style.cursor = "";
    snipEditWrap.classList.add("panning");
    e.preventDefault();
  }
  function snipEditDoPan(e) {
    if (!SE.panState) return;
    snipEditWrap.scrollLeft = SE.panState.sl - (e.clientX - SE.panState.x);
    snipEditWrap.scrollTop = SE.panState.st - (e.clientY - SE.panState.y);
  }
  function snipEditEndPan() {
    SE.panState = null;
    if (!SE.spaceDown && SE.tool !== "hand") snipEditWrap.classList.remove("panning");
  }

  // ---- zoom: pure CSS scaling of the native-resolution canvas -- no redraw needed ----
  function snipEditSetZoom(z) {
    SE.zoom = snipEditClamp(z, 0.05, snipZoomMax);
    const sw = SE.w * snipEditScale(), sh = SE.h * snipEditScale();
    // Magnified pixels stay square and sharp instead of being smoothed.
    snipEditStage.classList.toggle("pixelated", SE.zoom > 1);
    snipEditStage.style.width = `${sw}px`;
    snipEditStage.style.height = `${sh}px`;
    // Once the picture is larger than the view, leave half a view of room on
    // every side so any corner or edge can be panned into the middle. While it
    // fits, the CSS auto margin keeps it centred.
    const vw = snipEditWrap.clientWidth, vh = snipEditWrap.clientHeight;
    snipEditStage.style.margin = (sw > vw || sh > vh) ? `${Math.round(vh / 2)}px ${Math.round(vw / 2)}px` : "";
    $("snip-edit-zoom-val").textContent = `${Math.round(SE.zoom * 100)}%`;
  }
  // Zooms while the picture point under (clientX, clientY) stays where it is.
  function snipEditZoomAt(z, clientX, clientY) {
    const r0 = snipEditStage.getBoundingClientRect();
    const k0 = snipEditScale();
    const ix = (clientX - r0.left) / k0, iy = (clientY - r0.top) / k0;
    snipEditSetZoom(z);
    const r1 = snipEditStage.getBoundingClientRect();
    const k1 = snipEditScale();
    snipEditWrap.scrollLeft += r1.left + ix * k1 - clientX;
    snipEditWrap.scrollTop += r1.top + iy * k1 - clientY;
  }
  function snipEditZoomBy(factor) {
    const wr = snipEditWrap.getBoundingClientRect();
    snipEditZoomAt(SE.zoom * factor, wr.left + snipEditWrap.clientWidth / 2, wr.top + snipEditWrap.clientHeight / 2);
  }
  function snipEditFit() {
    const dpr = window.devicePixelRatio || 1;
    const availW = (snipEditWrap.clientWidth - 24) * dpr;
    const availH = (snipEditWrap.clientHeight - 24) * dpr;
    snipEditSetZoom(Math.min(1, availW / SE.w, availH / SE.h));
  }

  // ---- pointer wiring ----
  function snipEditOnPointerDown(e) {
    if (!SE.active) return;
    if (e.button === 1 || SE.spaceDown || (e.button === 0 && SE.tool === "hand")) { snipEditStartPan(e); return; }
    if (e.button !== 0) return;
    // The canvas itself is never a focus target, so the browser's default
    // mousedown action for it is to blur whatever is currently focused --
    // including the text-tool input this same handler is about to focus below,
    // since that default action runs AFTER this handler returns. Without this,
    // opening the text field looked like it immediately lost focus again.
    e.preventDefault();
    snipEditFinishTextIfOpen();
    const pt0 = snipEditToImage(e);
    if (SE.eyedropArm) {
      snipEditToolState(SE.eyedropArm).color = snipEditGetPixelColor(pt0);
      SE.eyedropArm = null;
      snipEditRenderContext();
      return;
    }
    snipEditLiveCv.setPointerCapture(e.pointerId);
    SE.lastNudged = null;
    snipEditUpdateNudgeHint();
    const pt = snipEditApplySnap(pt0, e.altKey);
    switch (SE.tool) {
      case "crop": snipEditCropPointerDown(pt); break;
      case "pen": case "marker": SE.drag = { kind: SE.tool, points: [pt] }; break;
      case "line": case "arrow": case "rect": case "ellipse": {
        if (SE.selOp && SE.selOp.tool === SE.tool) {
          const handle = snipEditHitShapeHandle(pt, SE.selOp);
          if (handle) {
            SE.drag = SE.selOp.points
              ? { kind: "shape-endpoint", op: SE.selOp, handle, orig: SE.selOp.points.map((p) => ({ ...p })) }
              : { kind: "shape-resize", op: SE.selOp, handle, orig: { ...SE.selOp.rect } };
            break;
          }
        }
        const hit = snipEditHitTestShape(pt);
        if (hit) {
          snipEditSelectShape(hit);
          SE.drag = { kind: "shape-move", op: hit, start: pt, orig: hit.points ? hit.points.map((p) => ({ ...p })) : { ...hit.rect } };
          break;
        }
        snipEditDeselectShape();
        SE.drag = { kind: SE.tool, a: pt, b: pt };
        break;
      }
      case "pixelate": case "spotlight": SE.drag = { kind: SE.tool, a: pt, b: pt }; break;
      case "text": snipEditTextPointerDown(pt); break;
      case "step": snipEditPushOp({ tool: "step", point: pt, color: snipEditToolState("step").color, width: snipEditToolState("step").width }); break;
    }
  }
  function snipEditOnPointerMove(e) {
    if (!SE.active) return;
    if (SE.panState) { snipEditDoPan(e); return; }
    if (!SE.drag) {
      if (SE.tool === "crop") {
        // Space held means a pan is about to start (or the panning CSS class
        // is already showing its grab cursor) -- an inline crop cursor would
        // otherwise outrank that class and never let go/grabbing show.
        if (!SE.spaceDown) snipEditWrap.style.cursor = snipEditCropCursorFor(snipEditToImage(e));
        // The overlay is already on screen and unchanged by a plain hover
        // (its handles do not react to hover position) -- redrawing it here
        // was a full-image clear+refill on every mousemove for nothing.
      } else if (SNIP_SHAPE_TOOLS.includes(SE.tool)) {
        if (!SE.spaceDown) snipEditWrap.style.cursor = snipEditShapeCursorFor(snipEditToImage(e));
      }
      return;
    }
    const pt0 = snipEditToImage(e);
    let pt = snipEditApplySnap(pt0, e.altKey);
    if (e.shiftKey) pt = snipEditApplyShiftConstraint(SE.drag, pt);
    switch (SE.drag.kind) {
      case "pen": case "marker": SE.drag.points.push(pt); snipEditDrawLivePenStroke(); break;
      case "line": case "arrow": SE.drag.b = pt; snipEditDrawLiveLineArrow(); break;
      case "rect": case "ellipse": SE.drag.b = pt; snipEditDrawLiveRectEllipse(); snipEditShowMeasure(e, SE.drag); break;
      case "pixelate": SE.drag.b = pt; snipEditDrawLivePixelatePreview(); break;
      case "spotlight": SE.drag.b = pt; snipEditDrawLiveSpotlightPreview(); break;
      case "crop-move": case "crop-resize":
        snipEditCropPointerMove(pt, e.shiftKey);
        snipEditShowMeasure(e, SE.cropRect);
        break;
      case "text-move": {
        const d = SE.drag;
        const newPoint = { x: pt.x - d.dx, y: pt.y - d.dy };
        snipEditClearLive();
        snipEditDrawTextOp(snipEditLiveCtx, { ...d.op, point: newPoint });
        d.newPoint = newPoint;
        break;
      }
      case "shape-move": {
        const d = SE.drag;
        const dx = pt.x - d.start.x, dy = pt.y - d.start.y;
        if (d.op.points) {
          d.newPoints = d.orig.map((p) => ({ x: p.x + dx, y: p.y + dy }));
          snipEditDrawShapeDragLive({ ...d.op, points: d.newPoints });
        } else {
          d.newRect = { ...d.orig, x: d.orig.x + dx, y: d.orig.y + dy };
          snipEditDrawShapeDragLive({ ...d.op, rect: d.newRect });
        }
        break;
      }
      case "shape-resize": {
        const d = SE.drag;
        d.newRect = snipEditResizeCropRect(d.orig, d.handle, pt, e.shiftKey);
        snipEditDrawShapeDragLive({ ...d.op, rect: d.newRect });
        break;
      }
      case "shape-endpoint": {
        const d = SE.drag;
        d.newPoints = d.orig.map((p) => ({ ...p }));
        d.newPoints[d.handle === "a" ? 0 : 1] = pt;
        snipEditDrawShapeDragLive({ ...d.op, points: d.newPoints });
        break;
      }
    }
  }
  function snipEditOnPointerUp() {
    if (!SE.active) return;
    if (SE.panState) { snipEditEndPan(); return; }
    if (!SE.drag) return;
    const d = SE.drag;
    SE.drag = null;
    snipEditHideMeasure();
    snipEditClearLive();
    const ts = snipEditToolState(SE.tool);
    switch (d.kind) {
      case "pen": if (d.points.length >= 2) snipEditPushOp({ tool: "pen", points: d.points, color: ts.color, width: ts.width }); break;
      case "marker": if (d.points.length >= 2) snipEditPushOp({ tool: "marker", points: d.points, color: ts.color, width: ts.width }); break;
      case "line": case "arrow":
        if (snipEditDist(d.a, d.b) >= 2) snipEditPushOp({ tool: d.kind, points: [d.a, d.b], color: ts.color, width: ts.width });
        break;
      case "rect": case "ellipse": {
        const r = snipEditNormRect(d);
        if (r.w >= 4 && r.h >= 4) snipEditPushOp({ tool: d.kind, rect: r, color: ts.color, width: ts.width, filled: !!ts.filled });
        break;
      }
      case "pixelate": {
        const r = snipEditNormRect(d);
        if (r.w >= 4 && r.h >= 4) snipEditPushOp({ tool: "pixelate", rect: r, blockPx: ts.blockPx });
        break;
      }
      case "spotlight": {
        const r = snipEditNormRect(d);
        if (r.w >= 4 && r.h >= 4) snipEditPushUniqueOp("spotlight", { tool: "spotlight", rect: r, dim: ts.dim });
        break;
      }
      case "crop-move": case "crop-resize": snipEditCropPointerUp(); break;
      case "text-move": case "shape-move": case "shape-resize": case "shape-endpoint": {
        const before = snipEditGeom(d.op);
        if (d.newPoint) d.op.point = d.newPoint;
        else if (d.newPoints) d.op.points = d.newPoints;
        else if (d.newRect) d.op.rect = d.newRect;
        snipEditRedrawBase();
        snipEditCommitGeomEdit(d.op, before);
        break;
      }
    }
    // snipEditClearLive() above wiped the selection handles a plain click
    // (kind "shape-move" with no actual movement) just painted on mousedown --
    // redraw them so the selection stays visible instead of vanishing the
    // instant the button is released.
    if (SE.selOp && SE.selOp.tool === SE.tool) snipEditDrawShapeSelectionLive();
  }
  snipEditLiveCv.addEventListener("pointerdown", snipEditOnPointerDown);
  snipEditLiveCv.addEventListener("pointermove", snipEditOnPointerMove);
  snipEditLiveCv.addEventListener("contextmenu", (e) => {
    if (!SE.active) return;
    e.preventDefault();
    const hit = snipEditHitTestForCtx(snipEditToImage(e));
    if (!hit) { closeSnipCtxPop(); return; }
    openSnipCtxPop(hit, e.clientX, e.clientY);
  });
  window.addEventListener("pointerup", snipEditOnPointerUp);
  // A cancelled pointer stream (e.g. the browser claiming a touch drag mid-
  // gesture) would otherwise leave SE.drag/SE.panState set forever, which
  // makes every tool button and shortcut a no-op for the rest of the session.
  window.addEventListener("pointercancel", snipEditOnPointerUp);
  // Alt-Tabbing away while Space/V is held never delivers the matching keyup
  // to this window -- without this, every later click would start a pan
  // (SE.spaceDown stuck true) or the raw/before image would stay stuck shown.
  window.addEventListener("blur", () => {
    if (SE.spaceDown) { SE.spaceDown = false; if (SE.tool !== "hand") snipEditWrap.classList.remove("panning"); }
    snipOriginalToggle.reset();
  });
  snipEditWrap.addEventListener("wheel", (e) => {
    if (!SE.active) return;
    e.preventDefault();
    const step = snipZoomStepFactor;
    snipEditZoomAt(SE.zoom * (e.deltaY < 0 ? step : 1 / step), e.clientX, e.clientY);
  }, { passive: false });
  // The pan room depends on the view size, so a resized window recomputes it.
  window.addEventListener("resize", () => { if (SE.active) snipEditSetZoom(SE.zoom); });

  snipEditTextInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); e.stopPropagation(); snipEditCommitTextInput(); }
    else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); snipEditCancelTextInput(); }
  });
  snipEditTextInput.addEventListener("blur", () => { if (SE.textEdit) snipEditCommitTextInput(); });

  // ---- keyboard: tool shortcuts, undo/redo, Enter/Esc, nudge, pan, before/after ----
  function snipEditKeydown(e) {
    if (snipEdit.classList.contains("hidden")) return;
    // The shortcuts overview is a sibling overlay, not inside snipEdit's own
    // DOM subtree, so it needs its own check before the editor's normal Tab
    // trap and tool shortcuts take over.
    if (!$("shortcuts-modal").classList.contains("hidden")) {
      if (e.key === "Tab") { trapTabWithin($("shortcuts-modal"), e); return; }
      if (e.key === "Escape") { e.preventDefault(); closeShortcutsModal(); return; }
      return;
    }
    // Appended straight to document.body (see its own declaration), so it sits
    // outside snipEdit's own DOM subtree -- the plain trap below would let Tab
    // walk back into the editor underneath while this menu is still open.
    if (!snipCtxPop.classList.contains("hidden")) {
      if (e.key === "Tab") { trapTabWithin(snipCtxPop, e); return; }
      if (e.key === "Escape") { e.preventDefault(); closeSnipCtxPop(); return; }
    }
    if (e.key === "Tab") { trapTabWithin(snipEdit, e); return; }
    if (e.key === "Escape") {
      e.preventDefault();
      if (!snipCtxPop.classList.contains("hidden")) { closeSnipCtxPop(); return; }
      if (snipColorPopIsOpen()) { closeSnipColorPop(); return; }
      if (snipEditContextIsOpen()) { closeSnipEditContext(); return; }
      if (SE.eyedropArm) { SE.eyedropArm = null; snipEditRenderContext(); return; }
      if (SE.tool === "crop" && (SE.drag || (SE.cropRect && !snipEditCropIsFull(SE.cropRect)) || SE.ops.some((o) => o.tool === "crop"))) { snipEditCropDiscard(); return; }
      // A selected/just-drawn shape only gives up the arrow keys, never the
      // whole editor -- closing on Escape here would throw away every edit
      // made so far just because something happened to be selected.
      if (SE.lastNudged) { snipEditDeselectShape(); return; }
      snipEditCancel();
      return;
    }
    if (document.activeElement === snipEditTextInput) return; // input owns its own keys
    // Any other field (size, colour hex, select) owns its keys too: a letter
    // typed there must not switch tools, and arrows must move its caret.
    const tgt = e.target;
    if (tgt && (tgt.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(tgt.tagName))) return;
    if (e.key === "Enter") {
      e.preventDefault();
      // The untouched full-image box is no crop, so Enter then applies the editor.
      if (SE.tool === "crop" && SE.cropRect && !snipEditCropIsFull(SE.cropRect)) { snipEditCropCommit(); return; }
      snipEditApply();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") { e.preventDefault(); if (e.shiftKey) snipEditRedo(); else snipEditUndo(); return; }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "y") { e.preventDefault(); snipEditRedo(); return; }
    // Canvas zoom: Ctrl+Plus/Minus/0, matching the video editor's preview zoom
    // keys -- plain 0 and - stay the pixelate/spotlight tool shortcuts above.
    if ((e.ctrlKey || e.metaKey) && (e.key === "+" || e.key === "=")) { e.preventDefault(); snipEditZoomBy(snipZoomStepFactor); return; }
    if ((e.ctrlKey || e.metaKey) && e.key === "-") { e.preventDefault(); snipEditZoomBy(1 / snipZoomStepFactor); return; }
    if ((e.ctrlKey || e.metaKey) && e.key === "0") { e.preventDefault(); snipEditFit(); return; }
    if (e.key === " ") { SE.spaceDown = true; snipEditWrap.style.cursor = ""; snipEditWrap.classList.add("panning"); e.preventDefault(); return; }
    if (e.key.toLowerCase() === "v" && !e.repeat) { snipOriginalToggle.press(); return; }
    if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key) && SE.lastNudged) {
      e.preventDefault();
      const amount = e.shiftKey ? NUM_FIELD_BIG_STEP : 1;
      snipEditNudge(SE.lastNudged, e.key, amount, e.ctrlKey || e.metaKey);
      return;
    }
    const tdef = SNIP_TOOLS.find((t) => t.key === e.key || (t.key === "-" && e.key === "ß"));
    if (tdef) { e.preventDefault(); snipEditSetTool(tdef.id); }
  }
  function snipEditKeyup(e) {
    if (snipEdit.classList.contains("hidden")) return;
    if (e.key === " ") { SE.spaceDown = false; if (SE.tool !== "hand") snipEditWrap.classList.remove("panning"); }
    if (e.key.toLowerCase() === "v") snipOriginalToggle.release();
  }
  document.addEventListener("keydown", snipEditKeydown);
  document.addEventListener("keyup", snipEditKeyup);

  // ---- open / cancel / apply / export ----
  function snipEditOpen(dataUrl, { onApply, onCancel }) {
    const img = new Image();
    // A capture that fails to decode (truncated data URL, a broken picked file)
    // must not leave the editor silently never opening -- fall back to the
    // same path a user cancel already takes, so nothing is left half-open.
    img.onerror = () => {
      toast(t("snipEditOpenFailed"));
      onCancel?.();
    };
    img.onload = () => {
      recordToolUsage("screenshot_editor");
      SE.img = img;
      SE.w = img.naturalWidth;
      SE.h = img.naturalHeight;
      SE.ops = [];
      SE.redo = [];
      SE.floor = 0;
      SE.perTool = {};
      SE.tool = SNIP_EDIT_DEFAULT_TOOL();
      SE.cropRect = null;
      SE.selOp = null;
      SE.lastNudged = null;
      snipEditUpdateNudgeHint();
      SE.eyedropArm = null;
      snipOriginalToggle.reset();
      SE.drag = null;
      closeSnipEditContext();
      SE.onApply = onApply;
      SE.onCancel = onCancel;
      SE.active = true;
      snipEditWrap.style.cursor = ""; // clear a cursor left inline by the previous session

      snipEditBaseCv.width = SE.w;
      snipEditBaseCv.height = SE.h;
      snipEditLiveCv.width = SE.w;
      snipEditLiveCv.height = SE.h;
      SE.rawCanvas = document.createElement("canvas");
      SE.rawCanvas.width = SE.w;
      SE.rawCanvas.height = SE.h;
      SE.rawCanvas.getContext("2d").drawImage(img, 0, 0);

      snipEditBuildToolButtons();
      snipEditRenderToolButtons();
      snipEditRenderContext();
      snipEditRedrawBase();
      snipEditSyncCropFromOps(); // a default tool of crop starts with the full-image box
      snipEditUpdateUndoRedoButtons();
      snipEdit.classList.remove("hidden");
      snipEditFit();
      snipEditWrap.scrollLeft = 0;
      snipEditWrap.scrollTop = 0;
    };
    img.src = dataUrl;
  }
  function snipEditClose() {
    SE.active = false;
    snipEdit.classList.add("hidden");
    SE.drag = null;
    SE.panState = null;
    SE.spaceDown = false;
    SE.eyedropArm = null;
    snipOriginalToggle.reset();
    snipEditWrap.classList.remove("panning");
    snipEditWrap.style.cursor = "";
    snipEditCancelTextInput();
    closeSnipCtxPop();
    closeSnipEditContext();
    // Nothing of the session stays alive behind the hidden editor: the next open starts
    // from a fresh capture anyway.
    SE.img = null;
    SE.rawCanvas = null;
    SE.ops = [];
    SE.redo = [];
    SE.floor = 0;
  }
  function snipEditCancel() {
    const cb = SE.onCancel;
    snipEditClose();
    cb?.();
  }
  // Applies the crop (if any) only now -- everything else stays undoable until export.
  // The output canvas is never larger than the source, so a 4K/panorama capture is
  // never upscaled, and this is a single synchronous pass, never a per-block loop.
  function snipEditExportDataUrl() {
    // A drawn crop rectangle otherwise only reaches SE.ops via an Enter keypress --
    // export must not require that hidden step, or a crop drawn (or redrawn over an
    // older one) and exported straight away silently exports uncropped/stale instead.
    // Scoped to the crop tool being active (same guard the Enter handler already
    // uses) so this never resurrects a crop the user has since undone while parked
    // on another tool -- SE.cropRect is only kept in sync with SE.ops while crop is
    // the active tool (see snipEditSetTool).
    if (SE.tool === "crop") snipEditCropCommit();
    snipOriginalToggle.reset();
    const crop = SE.ops.find((o) => o.tool === "crop");
    let sx = 0, sy = 0, sw = SE.w, sh = SE.h;
    if (crop) {
      sx = snipEditClamp(Math.round(crop.rect.x), 0, SE.w);
      sy = snipEditClamp(Math.round(crop.rect.y), 0, SE.h);
      sw = Math.min(SE.w - sx, Math.round(crop.rect.w));
      sh = Math.min(SE.h - sy, Math.round(crop.rect.h));
    }
    if (sw < 8 || sh < 8) { sx = 0; sy = 0; sw = SE.w; sh = SE.h; }
    const out = document.createElement("canvas");
    out.width = sw;
    out.height = sh;
    // The base canvas carries the crop dimming outside the rect; with a fractional
    // rect the rounded edge pixels would pick up a dim line, so draw it bare.
    if (crop) snipEditRedrawBase(true);
    out.getContext("2d").drawImage(snipEditBaseCv, sx, sy, sw, sh, 0, 0, sw, sh);
    if (crop) snipEditRedrawBase();
    // Always lossless png regardless of the screenshotFormat setting -- the
    // setting's actual target format is applied once, in Rust, at the real
    // save step (save_screenshot_now/_as re-encode from this data URL), so an
    // edited screenshot never picks up a lossy detour on its way there.
    return out.toDataURL("image/png");
  }
  function snipEditApply() {
    snipEditFinishTextIfOpen();
    const dataUrl = snipEditExportDataUrl();
    const cb = SE.onApply;
    snipEditClose();
    cb?.(dataUrl);
  }
  $("snip-edit-cancel").addEventListener("click", snipEditCancel);
  $("snip-edit-close").addEventListener("click", snipEditCancel);
  $("snip-edit-shortcuts").addEventListener("click", () => openShortcutsModal("snipEditor"));
  $("shortcuts-modal-close").addEventListener("click", closeShortcutsModal);
  $("shortcuts-modal").addEventListener("pointerdown", (e) => { if (e.target === $("shortcuts-modal")) closeShortcutsModal(); });
  $("snip-edit-apply").addEventListener("click", snipEditApply);
  $("snip-edit-copy").addEventListener("click", async () => {
    const dataUrl = snipEditExportDataUrl();
    try {
      await invoke("copy_image_data", { dataUrl });
      toast(t("copied"));
    } catch (err) {
      errToast(err);
    }
  });
  $("snip-edit-undo").addEventListener("click", snipEditUndo);
  $("snip-edit-redo").addEventListener("click", snipEditRedo);
  $("snip-edit-zoom-in").addEventListener("click", () => snipEditZoomBy(snipZoomStepFactor));
  $("snip-edit-zoom-out").addEventListener("click", () => snipEditZoomBy(1 / snipZoomStepFactor));
  $("snip-edit-fit").addEventListener("click", snipEditFit);
  $("snip-edit-original").addEventListener("pointerdown", () => snipOriginalToggle.press());
  document.addEventListener("pointerup", () => snipOriginalToggle.release());
  document.addEventListener("pointercancel", () => snipOriginalToggle.release());

  // ---- resize (scale the image itself, not the view -- see snipEditApplyScale) ----
  const snipScalePop = document.createElement("div");
  snipScalePop.className = "snip-scale-pop hidden";
  document.body.appendChild(snipScalePop);
  function closeSnipScalePop() { snipScalePop.classList.add("hidden"); }
  function snipScalePopIsOpen() { return !snipScalePop.classList.contains("hidden"); }
  function openSnipScalePop(anchor) {
    const origW = SE.w, origH = SE.h;
    let lockAspect = true;
    snipScalePop.innerHTML = "";

    const size = document.createElement("div");
    size.className = "snip-scale-current";
    size.textContent = t("snipScaleCurrentSize").replace("{w}", origW).replace("{h}", origH);
    snipScalePop.appendChild(size);

    const widthField = numberField({
      value: origW, min: 1, max: 20000, step: 1,
      decreaseLabel: t("numFieldDecrease"), increaseLabel: t("numFieldIncrease"),
      decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease",
      onChange: (w) => {
        if (lockAspect) heightField.setValue(Math.max(1, Math.round((origH * w) / origW)));
        percentField.setValue(Math.round((w / origW) * 100));
      },
    });
    const heightField = numberField({
      value: origH, min: 1, max: 20000, step: 1,
      decreaseLabel: t("numFieldDecrease"), increaseLabel: t("numFieldIncrease"),
      decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease",
      onChange: (h) => {
        if (lockAspect) widthField.setValue(Math.max(1, Math.round((origW * h) / origH)));
        percentField.setValue(Math.round((h / origH) * 100));
      },
    });
    // Presets and the percent field itself share this: they always set both
    // dimensions from the ORIGINAL size, regardless of the lock-aspect toggle
    // (a percentage is inherently aspect-preserving).
    const setByPercent = (p) => {
      percentField.setValue(p);
      widthField.setValue(Math.max(1, Math.round((origW * p) / 100)));
      heightField.setValue(Math.max(1, Math.round((origH * p) / 100)));
    };
    const percentField = numberField({
      value: 100, min: 1, max: 1000, step: 5,
      decreaseLabel: t("numFieldDecrease"), increaseLabel: t("numFieldIncrease"),
      decreaseAriaKey: "numFieldDecrease", increaseAriaKey: "numFieldIncrease",
      onChange: (p) => {
        widthField.setValue(Math.max(1, Math.round((origW * p) / 100)));
        heightField.setValue(Math.max(1, Math.round((origH * p) / 100)));
      },
    });

    const wRow = document.createElement("label");
    wRow.className = "snip-scale-row";
    wRow.textContent = t("snipScaleWidth");
    wRow.appendChild(widthField);
    const hRow = document.createElement("label");
    hRow.className = "snip-scale-row";
    hRow.textContent = t("snipScaleHeight");
    hRow.appendChild(heightField);
    const pRow = document.createElement("label");
    pRow.className = "snip-scale-row";
    pRow.textContent = t("snipScalePercent");
    pRow.appendChild(percentField);
    snipScalePop.append(wRow, hRow, pRow);

    const lockRow = document.createElement("label");
    lockRow.className = "snip-scale-lock";
    const lockBox = document.createElement("input");
    lockBox.type = "checkbox";
    lockBox.checked = true;
    lockBox.addEventListener("change", () => { lockAspect = lockBox.checked; });
    lockRow.append(lockBox, document.createTextNode(t("snipScaleLockAspect")));
    snipScalePop.appendChild(lockRow);

    const presets = document.createElement("div");
    presets.className = "snip-scale-presets";
    [25, 50, 200].forEach((p) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = `${p}%`;
      b.addEventListener("click", () => setByPercent(p));
      presets.appendChild(b);
    });
    const fitBtn = document.createElement("button");
    fitBtn.type = "button";
    fitBtn.textContent = t("snipScalePresetFit1920");
    fitBtn.addEventListener("click", () => {
      widthField.setValue(1920);
      heightField.setValue(Math.max(1, Math.round((origH * 1920) / origW)));
      percentField.setValue(Math.round((1920 / origW) * 100));
    });
    presets.appendChild(fitBtn);
    snipScalePop.appendChild(presets);

    const actions = document.createElement("div");
    actions.className = "snip-scale-actions";
    const cancelBtn = document.createElement("button");
    cancelBtn.type = "button";
    cancelBtn.className = "ghost-btn";
    cancelBtn.textContent = t("cancel");
    cancelBtn.addEventListener("click", closeSnipScalePop);
    const applyBtn = document.createElement("button");
    applyBtn.type = "button";
    applyBtn.className = "primary-btn";
    applyBtn.textContent = t("apply");
    applyBtn.addEventListener("click", () => {
      const w = Number(widthField.querySelector("input").value);
      const h = Number(heightField.querySelector("input").value);
      if (snipEditApplyScale(w, h)) closeSnipScalePop();
    });
    actions.append(cancelBtn, applyBtn);
    snipScalePop.appendChild(actions);

    snipScalePop.classList.remove("hidden");
    placePopover(snipScalePop, anchor.getBoundingClientRect(), 6);
  }
  $("snip-edit-scale").addEventListener("click", (e) => {
    if (snipScalePopIsOpen()) { closeSnipScalePop(); return; }
    openSnipScalePop(e.currentTarget);
  });
  document.addEventListener("pointerdown", (e) => {
    if (!snipScalePopIsOpen()) return;
    if (snipScalePop.contains(e.target) || e.target.closest("#snip-edit-scale")) return;
    closeSnipScalePop();
  });
  $("snip-edit-saveas").addEventListener("click", async () => {
    const dataUrl = snipEditExportDataUrl();
    try {
      const path = await invoke("save_screenshot_as", { dataUrl });
      if (path) toast(t("snipEditSavedTo").replace("{path}", path));
    } catch (e) { errToast(e); }
  });
  // SouthEast-only resize handle. The move side of the titlebar needs no JS
  // at all -- data-tauri-drag-region alone drags the window (same as the main
  // window's own header) -- but there is no HTML-attribute equivalent for
  // resizing, so this is the one spot that calls into the window API directly.
  $("snip-edit-resize-grip").addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    getCurrentWindow().startResizeDragging(ResizeDirection.SouthEast).catch(() => {});
  });
  // What a new capture has to know before it takes the editor over: whether a session is
  // open and whether it holds edits that would be lost (any operation drawn so far).
  function snipEditState() {
    return { active: SE.active, dirty: SE.ops.length > 0 };
  }
  // Read by the smoke test: what the session still keeps alive, counting the resizes whose
  // earlier image is still held (also inside the history a resize carries) and the redo list.
  window.__snipEditHeld = () => {
    const seen = new Set();
    const heldResizes = (list) => {
      let n = 0;
      for (const o of list) {
        if (o.tool !== "scale" || seen.has(o)) continue;
        seen.add(o);
        if (o.prevImg) n++;
        n += heldResizes(o.prevOps || []);
      }
      return n;
    };
    return { resizes: heldResizes([...SE.ops, ...SE.redo]), ops: SE.ops.length, redo: SE.redo.length, img: !!SE.img, raw: !!SE.rawCanvas };
  };
  return { snipEditOpen, snipEdit, snipEditState, snipEditCancel };
}
