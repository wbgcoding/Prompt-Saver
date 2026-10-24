// Screenshot editor: draws one finished operation (pen, marker, line, arrow, rect, ellipse,
// text, step, pixelate, spotlight) onto a canvas, for the finished layer and the live preview.
import { contrastTextColor } from "./floatShared.js";

export function snipEditNormRect(r) {
  if (r && "a" in r) {
    const x = Math.min(r.a.x, r.b.x), y = Math.min(r.a.y, r.b.y);
    return { x, y, w: Math.abs(r.b.x - r.a.x), h: Math.abs(r.b.y - r.a.y) };
  }
  return r;
}

// `SE` is the editor state; `snipEditBaseCtx` only measures text.
export function createSnipDraw(SE, snipEditBaseCtx, SNIP_DEFAULT_SIZES, SNIP_ARROW_HEAD_SCALE) {
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
  // Collects the whole stroke on its own transparent layer (so self-overlap within one stroke
  // never darkens further), then composites it once with alpha. The layer is reused across
  // calls: this runs once per marker op on every redraw, so a fresh full-resolution canvas each
  // time would cost marker count times image resolution.
  let snipEditMarkerTmp = null;
  // Shrinks the reused stroke layer (full capture size) once the editor closes.
  function snipEditReleaseMarkerLayer() {
    if (snipEditMarkerTmp) { snipEditMarkerTmp.width = 0; snipEditMarkerTmp.height = 0; }
  }
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
  // The shaft stops at the head's back edge, so its round cap lands under the head's fill and
  // never pokes out beside the triangle; the head is a long, slender point.
  function snipEditDrawArrowOp(ctx, op) {
    const [a, b] = op.points;
    const dist = Math.hypot(b.x - a.x, b.y - a.y);
    const angle = dist > 0 ? Math.atan2(b.y - a.y, b.x - a.x) : 0;
    // Capped at the drag distance: a drag shorter than one arrowhead would otherwise put the
    // shaft's back point past the start and flip the shaft backwards.
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
    if (op.back === "white") {
      // Solid white box, no shadow: the letters stay crisp on any background.
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(op.point.x - 4, op.point.y - 2, w + 8, h + 4);
      ctx.fillStyle = op.color;
      ctx.fillText(op.text, op.point.x, op.point.y);
      ctx.restore();
      return;
    }
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
  // Always samples SE.rawCanvas (the untouched capture), so overlapping pixelation never
  // compounds on itself.
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
  return {
    snipEditDrawPenOp, snipEditDrawMarkerOp, snipEditDrawLineOp, snipEditDrawArrowOp,
    snipEditDrawRectOp, snipEditDrawEllipseOp, snipEditDrawTextOp, snipEditMeasureTextWidth,
    snipEditDrawPixelate, snipEditDrawSpotlight, snipEditDrawCropDim, snipEditDrawOp, snipEditReleaseMarkerLayer,
  };
}
