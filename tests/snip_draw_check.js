// Screenshot editor drawing (ui/snipDraw.js): snipEditNormRect, and every per-tool draw routine
// of createSnipDraw run against a recording canvas context. The real module is imported; the
// context is a fake that logs each call, so the expected calls and numbers are literal.
// Run:  node tests/snip_draw_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { pathToFileURL } = require("url");

const problems = [];
let count = 0;
const check = (label, actual, expected) => {
  count++;
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

// A context that logs calls as [name, ...args] and records style properties as ["set", name, value].
function recorder(measure) {
  const log = [];
  const ctx = new Proxy({}, {
    get(_, name) {
      if (name === "log") return log;
      if (name === "measureText") return (s) => { log.push(["measureText", s]); return { width: measure ? measure(s) : s.length * 10 }; };
      return (...args) => { log.push([name, ...args]); };
    },
    set(_, name, value) { log.push(["set", name, value]); return true; },
  });
  return ctx;
}
const calls = (ctx, ...names) => ctx.log.filter((e) => names.includes(e[0]));
const noNaN = (ctx) => ctx.log.every((e) => e.slice(1).every((v) => typeof v !== "number" || Number.isFinite(v)));

async function main() {
  globalThis.document = globalThis.document || { addEventListener() {}, removeEventListener() {} };
  const shared = await import(pathToFileURL(path.join(__dirname, "..", "ui", "floatShared.js")).href);
  const mod = await import(pathToFileURL(path.join(__dirname, "..", "ui", "snipDraw.js")).href);
  const { snipEditNormRect, createSnipDraw } = mod;

  // ---- snipEditNormRect
  check("a rect passes through", snipEditNormRect({ x: 1, y: 2, w: 3, h: 4 }), { x: 1, y: 2, w: 3, h: 4 });
  const plain = { x: 1, y: 2, w: 3, h: 4 };
  check("a rect is the same object", snipEditNormRect(plain) === plain, true);
  check("a drag from top left to bottom right", snipEditNormRect({ a: { x: 4, y: 20 }, b: { x: 10, y: 30 } }), { x: 4, y: 20, w: 6, h: 10 });
  check("a drag from bottom right to top left", snipEditNormRect({ a: { x: 10, y: 30 }, b: { x: 4, y: 20 } }), { x: 4, y: 20, w: 6, h: 10 });
  check("a drag from top right to bottom left", snipEditNormRect({ a: { x: 10, y: 20 }, b: { x: 4, y: 30 } }), { x: 4, y: 20, w: 6, h: 10 });
  check("a drag with no movement", snipEditNormRect({ a: { x: 5, y: 5 }, b: { x: 5, y: 5 } }), { x: 5, y: 5, w: 0, h: 0 });
  check("a drag at the origin", snipEditNormRect({ a: { x: 0, y: 0 }, b: { x: 0, y: 7 } }), { x: 0, y: 0, w: 0, h: 7 });
  check("a drag with fractions", snipEditNormRect({ a: { x: 1.5, y: 2.25 }, b: { x: 0.5, y: 4.75 } }), { x: 0.5, y: 2.25, w: 1, h: 2.5 });
  check("a drag with negative coordinates", snipEditNormRect({ a: { x: -10, y: -4 }, b: { x: -2, y: -20 } }), { x: -10, y: -20, w: 8, h: 16 });
  check("null passes through", snipEditNormRect(null), null);
  check("undefined passes through", snipEditNormRect(undefined), undefined);

  // ---- the draw routines
  const SE = { w: 100, h: 80, rawCanvas: { raw: true } };
  const base = recorder();
  const created = [];
  globalThis.document = {
    addEventListener() {}, removeEventListener() {},
    createElement: () => { const c = { width: 0, height: 0, ctx: recorder() }; c.getContext = () => c.ctx; created.push(c); return c; },
  };
  const D = createSnipDraw(SE, base, { outlinePx: 4 }, 2);
  const rects = (ctx) => calls(ctx, "fillRect").map((e) => e.slice(1));

  // spotlight and crop dim darken everything outside the rectangle
  let c = recorder();
  D.snipEditDrawSpotlight(c, { x: 10, y: 20, w: 30, h: 40 }, 0.6);
  check("spotlight darkens the four bands outside", rects(c), [[0, 0, 100, 20], [0, 60, 100, 20], [0, 20, 10, 40], [40, 20, 60, 40]]);
  check("spotlight fill colour", calls(c, "set").find((e) => e[1] === "fillStyle")[2], "rgba(0,0,0,0.6)");
  check("spotlight saves and restores", [c.log[0][0], c.log[c.log.length - 1][0]], ["save", "restore"]);
  c = recorder();
  D.snipEditDrawSpotlight(c, { a: { x: 40, y: 60 }, b: { x: 10, y: 20 } }, 0.6);
  check("spotlight takes a dragged corner pair", rects(c), [[0, 0, 100, 20], [0, 60, 100, 20], [0, 20, 10, 40], [40, 20, 60, 40]]);
  c = recorder();
  D.snipEditDrawCropDim(c, { x: 10, y: 20, w: 30, h: 40 });
  check("crop dim uses the same four bands", rects(c), [[0, 0, 100, 20], [0, 60, 100, 20], [0, 20, 10, 40], [40, 20, 60, 40]]);
  check("crop dim default strength", calls(c, "set").find((e) => e[1] === "fillStyle")[2], "rgba(0,0,0,0.55)");
  c = recorder();
  D.snipEditDrawCropDim(c, { x: 0, y: 0, w: 100, h: 80 }, 0.3);
  check("crop dim of the whole image has only empty bands", rects(c), [[0, 0, 100, 0], [0, 80, 100, 0], [0, 0, 0, 80], [100, 0, 0, 80]]);
  check("crop dim custom strength", calls(c, "set").find((e) => e[1] === "fillStyle")[2], "rgba(0,0,0,0.3)");

  // pen
  c = recorder();
  D.snipEditDrawPenOp(c, { color: "#ff0000", width: 4, points: [{ x: 1, y: 1 }] });
  check("a one-point pen stroke draws no path", calls(c, "stroke", "beginPath").length, 0);
  c = recorder();
  D.snipEditDrawPenOp(c, { color: "#ff0000", width: 4, points: [{ x: 1, y: 1 }, { x: 5, y: 7 }] });
  check("a two-point stroke is one straight segment", calls(c, "moveTo", "lineTo", "stroke"), [["moveTo", 1, 1], ["lineTo", 5, 7], ["stroke"]]);
  check("the pen sets colour, width and round ends", calls(c, "set").map((e) => e.slice(1)), [["strokeStyle", "#ff0000"], ["lineWidth", 4], ["lineCap", "round"], ["lineJoin", "round"]]);
  c = recorder();
  D.snipEditDrawPenOp(c, { color: "#00f", width: 2, points: [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }, { x: 20, y: 10 }] });
  check("a longer stroke is smoothed through midpoints", calls(c, "moveTo", "quadraticCurveTo", "lineTo"), [
    ["moveTo", 0, 0], ["quadraticCurveTo", 10, 0, 10, 5], ["quadraticCurveTo", 10, 10, 15, 10], ["lineTo", 20, 10],
  ]);
  check("a pen stroke is wrapped in save and restore", [c.log[0][0], c.log[c.log.length - 1][0]], ["save", "restore"]);

  // line
  c = recorder();
  D.snipEditDrawLineOp(c, { color: "#0f0", width: 3, points: [{ x: 2, y: 3 }, { x: 8, y: 9 }] });
  check("a line is one segment", calls(c, "moveTo", "lineTo", "stroke"), [["moveTo", 2, 3], ["lineTo", 8, 9], ["stroke"]]);

  // arrow
  c = recorder();
  D.snipEditDrawArrowOp(c, { color: "#f00", width: 3, points: [{ x: 0, y: 0 }, { x: 100, y: 0 }] });
  check("the arrow shaft stops at the back of the head", calls(c, "moveTo", "lineTo").slice(0, 2), [["moveTo", 0, 0], ["lineTo", 87, 0]]);
  check("the arrow head starts at the tip", calls(c, "moveTo")[1], ["moveTo", 100, 0]);
  check("the arrow head is filled", calls(c, "fill").length, 1);
  const head = calls(c, "lineTo").slice(1, 3);
  check("the head corners are symmetric about the shaft", Math.abs(head[0][2] + head[1][2]) < 1e-9 && Math.abs(head[0][1] - head[1][1]) < 1e-9, true);
  check("the head corners sit behind the tip", head.every((e) => e[1] < 100), true);
  c = recorder();
  D.snipEditDrawArrowOp(c, { color: "#f00", width: 3, points: [{ x: 0, y: 0 }, { x: 5, y: 0 }] });
  check("a drag shorter than the head keeps the shaft from flipping", calls(c, "lineTo")[0], ["lineTo", 0, 0]);
  c = recorder();
  D.snipEditDrawArrowOp(c, { color: "#f00", width: 3, points: [{ x: 7, y: 7 }, { x: 7, y: 7 }] });
  check("a zero-length arrow produces no NaN", noNaN(c), true);
  c = recorder();
  D.snipEditDrawArrowOp(c, { color: "#f00", width: 3, points: [{ x: 0, y: 0 }, { x: 0, y: 100 }] });
  check("a downward arrow shaft ends on the same column", calls(c, "lineTo")[0].slice(1).map((v) => Math.round(v * 1000) / 1000), [0, 87]);
  c = recorder();
  D.snipEditDrawArrowOp(c, { color: "#f00", width: 10, points: [{ x: 0, y: 0 }, { x: 100, y: 0 }] });
  check("a thicker line grows the head", calls(c, "lineTo")[0], ["lineTo", 73, 0]);

  // rect
  c = recorder();
  D.snipEditDrawRectOp(c, { color: "#00f", width: 4, filled: true, rect: { x: 10, y: 20, w: 30, h: 40 } });
  check("a filled rect fills its full area", calls(c, "fillRect", "strokeRect"), [["fillRect", 10, 20, 30, 40]]);
  c = recorder();
  D.snipEditDrawRectOp(c, { color: "#00f", width: 4, filled: false, rect: { x: 10, y: 20, w: 30, h: 40 } });
  check("an outline rect stays inside its box", calls(c, "fillRect", "strokeRect"), [["strokeRect", 12, 22, 26, 36]]);
  c = recorder();
  D.snipEditDrawRectOp(c, { color: "#00f", width: 10, filled: false, rect: { x: 0, y: 0, w: 6, h: 8 } });
  check("an outline wider than the box does not go negative", calls(c, "strokeRect"), [["strokeRect", 5, 5, 0, 0]]);

  // ellipse
  c = recorder();
  D.snipEditDrawEllipseOp(c, { color: "#00f", width: 4, filled: false, rect: { x: 10, y: 20, w: 30, h: 40 } });
  check("an outline ellipse is centred and inset by half the stroke", calls(c, "ellipse"), [["ellipse", 25, 40, 13, 18, 0, 0, Math.PI * 2]]);
  check("an outline ellipse is stroked", [calls(c, "stroke").length, calls(c, "fill").length], [1, 0]);
  c = recorder();
  D.snipEditDrawEllipseOp(c, { color: "#00f", width: 4, filled: true, rect: { x: 0, y: 0, w: 2, h: 2 } });
  check("a tiny ellipse clamps its radii at zero", calls(c, "ellipse")[0].slice(3, 5), [0, 0]);
  check("a filled ellipse is filled", [calls(c, "stroke").length, calls(c, "fill").length], [0, 1]);

  // text
  const textOp = (o) => Object.assign({ tool: "text", text: "abc", color: "#112233", fontPx: 20, point: { x: 50, y: 30 }, back: "none" }, o);
  c = recorder();
  D.snipEditDrawTextOp(c, textOp({ editing: true }));
  check("text being edited draws nothing", c.log.length, 0);
  c = recorder();
  D.snipEditDrawTextOp(c, textOp({ back: "white" }));
  check("a white box surrounds the text", calls(c, "fillRect"), [["fillRect", 46, 28, 38, 29]]);
  check("white box text is drawn without a shadow", calls(c, "set").some((e) => e[1] === "shadowBlur"), false);
  check("white box text is drawn once", calls(c, "fillText"), [["fillText", "abc", 50, 30]]);
  check("the font uses the pixel size", calls(c, "set").find((e) => e[1] === "font")[2], "20px sans-serif");
  c = recorder();
  D.snipEditDrawTextOp(c, textOp({ back: "plate" }));
  check("a plate sits behind the text", calls(c, "fillRect"), [["fillRect", 46, 28, 38, 29]]);
  check("a plate is dark and translucent", calls(c, "set").filter((e) => e[1] === "fillStyle").map((e) => e[2])[0], "rgba(0,0,0,0.45)");
  check("plate text has a shadow", calls(c, "set").find((e) => e[1] === "shadowBlur")[2], 3);
  c = recorder();
  D.snipEditDrawTextOp(c, textOp({ back: "outline", outlinePx: 6 }));
  check("an outline strokes the text first", calls(c, "strokeText", "fillText").map((e) => e[0]), ["strokeText", "fillText"]);
  check("the outline width is the stored one", calls(c, "set").find((e) => e[1] === "lineWidth")[2], 6);
  check("the outline contrasts with the text colour", calls(c, "set").find((e) => e[1] === "strokeStyle")[2], shared.contrastTextColor("#112233"));
  c = recorder();
  D.snipEditDrawTextOp(c, textOp({ back: "outline" }));
  check("the outline width defaults from the size table", calls(c, "set").find((e) => e[1] === "lineWidth")[2], 4);
  c = recorder();
  D.snipEditDrawTextOp(c, textOp({}));
  check("plain text is one fill with a shadow", [calls(c, "fillText").length, calls(c, "strokeText").length, calls(c, "fillRect").length], [1, 0, 0]);
  check("the measure width of the base context is returned", (() => {
    const w = D.snipEditMeasureTextWidth({ text: "abcd", fontPx: 12 });
    return [w, base.log.filter((e) => e[0] === "set" && e[1] === "font").map((e) => e[2])];
  })(), [40, ["12px sans-serif"]]);
  check("measuring saves and restores the base context", base.log.map((e) => e[0]).slice(0, 4), ["save", "set", "measureText", "restore"]);

  // step marker
  c = recorder();
  D.snipEditDrawOp(c, { tool: "step", color: "#ff0000", width: 4, point: { x: 30, y: 40 } }, 7);
  check("a step circle has radius ten plus the width", calls(c, "arc"), [["arc", 30, 40, 14, 0, Math.PI * 2]]);
  check("the step number is drawn centred a pixel low", calls(c, "fillText"), [["fillText", "7", 30, 41]]);
  check("the step font follows the radius", calls(c, "set").find((e) => e[1] === "font")[2], "15px sans-serif");
  check("the step number contrasts with the circle", calls(c, "set").filter((e) => e[1] === "fillStyle")[1][2], shared.contrastTextColor("#ff0000"));

  // marker layer
  created.length = 0;
  c = recorder();
  D.snipEditDrawMarkerOp(c, { color: "#ff0", width: 6, points: [{ x: 0, y: 0 }, { x: 9, y: 9 }] });
  check("the marker layer is sized like the image", [created[0].width, created[0].height], [100, 80]);
  check("the marker composes once with half alpha and multiply", [calls(c, "drawImage").length, calls(c, "set").filter((e) => e[1] === "globalAlpha")[0][2], calls(c, "set").find((e) => e[1] === "globalCompositeOperation")[2]], [1, 0.5, "multiply"]);
  check("the marker layer is cleared before each stroke", calls(created[0].ctx, "clearRect"), [["clearRect", 0, 0, 100, 80]]);
  D.snipEditDrawMarkerOp(c, { color: "#ff0", width: 6, points: [{ x: 0, y: 0 }, { x: 9, y: 9 }] });
  check("the marker layer is reused", created.length, 1);
  D.snipEditReleaseMarkerLayer();
  check("releasing shrinks the marker layer", [created[0].width, created[0].height], [0, 0]);
  D.snipEditDrawMarkerOp(c, { color: "#ff0", width: 6, points: [{ x: 0, y: 0 }, { x: 9, y: 9 }] });
  check("the marker layer grows back on the next stroke", [created.length, created[0].width, created[0].height], [1, 100, 80]);
  check("releasing before any marker is harmless", (() => { createSnipDraw(SE, base, {}, 2).snipEditReleaseMarkerLayer(); return true; })(), true);

  // pixelate
  created.length = 0;
  c = recorder();
  D.snipEditDrawPixelate(c, { x: 0, y: 0, w: 100, h: 50 }, 10);
  check("pixelate samples the untouched capture into a small canvas", calls(created[0].ctx, "drawImage"), [["drawImage", SE.rawCanvas, 0, 0, 100, 50, 0, 0, 10, 5]]);
  check("pixelate small canvas size", [created[0].width, created[0].height], [10, 5]);
  check("pixelate scales it back up", calls(c, "drawImage").map((e) => e.slice(2)), [[0, 0, 10, 5, 0, 0, 100, 50]]);
  check("pixelate turns smoothing off for the scale-up", calls(c, "set").find((e) => e[1] === "imageSmoothingEnabled")[2], false);
  c = recorder();
  created.length = 0;
  D.snipEditDrawPixelate(c, { a: { x: 60, y: 40 }, b: { x: 20, y: 20 } }, 10);
  check("pixelate takes a dragged corner pair", calls(c, "drawImage").map((e) => e.slice(2)), [[0, 0, 4, 2, 20, 20, 40, 20]]);
  c = recorder();
  created.length = 0;
  D.snipEditDrawPixelate(c, { x: 0, y: 0, w: 100, h: 50 }, 1);
  check("a block size under two counts as two", [created[0].width, created[0].height], [50, 25]);
  c = recorder();
  created.length = 0;
  D.snipEditDrawPixelate(c, { x: 0, y: 0, w: 3, h: 3 }, 100);
  check("a huge block still leaves one cell", [created[0].width, created[0].height], [1, 1]);
  c = recorder();
  created.length = 0;
  D.snipEditDrawPixelate(c, { x: 0, y: 0, w: 1, h: 50 }, 10);
  check("a sliver under two pixels wide draws nothing", [c.log.length, created.length], [0, 0]);
  D.snipEditDrawPixelate(c, { x: 0, y: 0, w: 50, h: 1 }, 10);
  check("a sliver under two pixels high draws nothing", [c.log.length, created.length], [0, 0]);

  // dispatch
  const dispatch = (op, n) => { const k = recorder(); D.snipEditDrawOp(k, op, n); return k; };
  check("an unknown tool draws nothing", dispatch({ tool: "laser" }).log.length, 0);
  check("dispatch: pen", calls(dispatch({ tool: "pen", color: "#000", width: 1, points: [{ x: 0, y: 0 }, { x: 1, y: 1 }] }), "stroke").length, 1);
  check("dispatch: line", calls(dispatch({ tool: "line", color: "#000", width: 1, points: [{ x: 0, y: 0 }, { x: 1, y: 1 }] }), "lineTo").length, 1);
  check("dispatch: arrow", calls(dispatch({ tool: "arrow", color: "#000", width: 1, points: [{ x: 0, y: 0 }, { x: 50, y: 1 }] }), "fill").length, 1);
  check("dispatch: rect", calls(dispatch({ tool: "rect", color: "#000", width: 1, filled: true, rect: { x: 0, y: 0, w: 5, h: 5 } }), "fillRect").length, 1);
  check("dispatch: ellipse", calls(dispatch({ tool: "ellipse", color: "#000", width: 1, rect: { x: 0, y: 0, w: 5, h: 5 } }), "ellipse").length, 1);
  check("dispatch: text", calls(dispatch(textOp({})), "fillText").length, 1);
  check("dispatch: marker", calls(dispatch({ tool: "marker", color: "#000", width: 1, points: [{ x: 0, y: 0 }, { x: 1, y: 1 }] }), "drawImage").length, 1);
  check("dispatch: pixelate", calls(dispatch({ tool: "pixelate", rect: { x: 0, y: 0, w: 20, h: 20 }, blockPx: 5 }), "drawImage").length, 1);
  check("dispatch: spotlight", rects(dispatch({ tool: "spotlight", rect: { x: 1, y: 1, w: 5, h: 5 }, dim: 0.5 })).length, 4);
  check("dispatch: step gets its number", calls(dispatch({ tool: "step", color: "#000", width: 1, point: { x: 1, y: 1 } }, 3), "fillText"), [["fillText", "3", 1, 2]]);
}

main().then(() => {
  if (problems.length) {
    console.log(`${problems.length} problem(s):`);
    for (const p of problems) console.log(" - " + p);
    process.exit(1);
  }
  console.log(`snip_draw_check: ${count} assertions OK`);
}).catch((e) => { console.log(e.stack || e); process.exit(1); });
