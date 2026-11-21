// Pure helpers of the video export filter graph (ui/videoGraph.js): ffmpeg path and text
// escaping, time and colour formatting, the overlay enable window, the rounded-area radius
// and mask, the sideways-clip rotation filter and the atempo stage chain.
//
// videoGraph.js imports video.js, which boots a real window, so it cannot be imported. This
// runs the real text of videoGraph.js (import lines dropped, `export` keywords removed) next to
// the real timeline helpers of video.js (clamp, opOf, keptSegments, speedRate, outMs, ...),
// both read from the files, so a change to either shows up here.
// Run:  node tests/video_graph_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const { videoWindowSource } = require("./ui_sources.js");
const videoSrc = videoWindowSource().replace(/\r\n/g, "\n");
const graphSrc = fs.readFileSync(path.join(__dirname, "..", "ui", "videoGraph.js"), "utf8").replace(/\r\n/g, "\n");

function extractBlock(label, marker) {
  const start = videoSrc.indexOf(marker);
  if (start === -1) throw new Error(`could not find ${label} in video.js: ${marker}`);
  const braceStart = videoSrc.indexOf("{", start);
  let depth = 0;
  for (let i = braceStart; i < videoSrc.length; i++) {
    if (videoSrc[i] === "{") depth++;
    else if (videoSrc[i] === "}") {
      depth--;
      if (depth === 0) return videoSrc.slice(start, i + 1);
    }
  }
  throw new Error(`unbalanced braces extracting ${label}`);
}
function extractLine(label, exact) {
  if (!videoSrc.includes(exact)) throw new Error(`could not find ${label} in video.js: ${exact}`);
  return exact;
}

const timelineParts = [
  extractLine("clamp", "const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));"),
  extractLine("opOf", "const opOf = (tool) => VE.ops.filter((o) => o.tool === tool).pop() || null;"),
  extractLine("opsOf", "const opsOf = (tool) => VE.ops.filter((o) => o.tool === tool);"),
  extractBlock("trimBounds", "function trimBounds() {"),
  extractBlock("keptSegments", "function keptSegments() {"),
  extractBlock("speedRate", "const speedRate = () => {") + ";",
  extractBlock("timelinePlan", "function timelinePlan() {"),
  extractBlock("planEntryOutMs", "function planEntryOutMs(entry) {"),
  extractBlock("outMs", "function outMs(srcMs, atEnd) {"),
];
const graphCode = graphSrc.replace(/^import .*$/gm, "").replace(/^export /gm, "");

const sandbox = { VE: null, KEPT_CUTS_BUF: [], KEPT_SEGMENTS_BUF: [] };
vm.createContext(sandbox);
vm.runInContext(
  timelineParts.join("\n") + "\n" + graphCode + "\n" +
  "this.api = { ffPath, ffText, typographicApostrophe, secs, hexToFf, enableExpr, videoOverlayWindowOutMs, shapeRadius, shapeMask, rotationFilter, atempoChain };",
  sandbox,
  { filename: "videoGraph.js (extracted)" }
);
const api = sandbox.api;

const problems = [];
let count = 0;
const check = (label, actual, expected) => {
  count++;
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

// ---- ffPath
check("ffPath plain drive path", api.ffPath("C:\\Users\\Public\\a\\clip.mp4"), String.raw`C\:/Users/Public/a/clip.mp4`);
check("ffPath keeps blanks", api.ffPath("D:\\My Clips\\a b.mp4"), String.raw`D\:/My Clips/a b.mp4`);
check("ffPath apostrophe closes, escapes and reopens the quote", api.ffPath("C:\\it's.mp4"), String.raw`C\:/it'\\\''s.mp4`);
check("ffPath two apostrophes", api.ffPath("a'b'c"), String.raw`a'\\\''b'\\\''c`);
check("ffPath without drive or separators is unchanged", api.ffPath("clip.mp4"), "clip.mp4");
check("ffPath empty", api.ffPath(""), "");
check("ffPath non-string input", api.ffPath(12), "12");
check("ffPath unicode untouched", api.ffPath("C:\\Vid\u00e9o\\\u65e5\u672c.mp4"), "C\\:/Vid\u00e9o/\u65e5\u672c.mp4");

// ---- typographicApostrophe / ffText
check("apostrophe becomes typographic", api.typographicApostrophe("it's"), "it\u2019s");
check("every apostrophe", api.typographicApostrophe("'a' 'b'"), "\u2019a\u2019 \u2019b\u2019");
check("no apostrophe unchanged", api.typographicApostrophe("plain"), "plain");
check("typographic non-string", api.typographicApostrophe(null), "null");
check("ffText apostrophe", api.ffText("it's"), "it\u2019s");
check("ffText colon", api.ffText("a:b"), "a\\:b");
check("ffText backslash is doubled", api.ffText("a\\b"), "a\\\\b");
check("ffText backslash before colon escapes each once", api.ffText("\\:"), "\\\\\\:");
check("ffText newline becomes a blank", api.ffText("a\nb"), "a b");
check("ffText CRLF is one blank", api.ffText("a\r\nb"), "a b");
check("ffText inner blanks stay", api.ffText("a  b"), "a  b");
check("ffText leading and trailing blanks are escaped", api.ffText("  hi  "), "\\ \\ hi\\ \\ ");
check("ffText a trailing newline ends up as an escaped blank", api.ffText("a\n"), "a\\ ");
check("ffText leading tab is escaped", api.ffText("\tx"), "\\\tx");
check("ffText only blanks", api.ffText("  "), "\\ \\ ");
check("ffText empty", api.ffText(""), "");
check("ffText number input", api.ffText(5), "5");
check("ffText unicode untouched", api.ffText("\u00fc\u65e5 \ud83d\ude00"), "\u00fc\u65e5 \ud83d\ude00");
check("ffText percent untouched", api.ffText("100%"), "100%");

// ---- secs
check("secs whole", api.secs(1000), "1.000");
check("secs fraction", api.secs(1500), "1.500");
check("secs zero", api.secs(0), "0.000");
check("secs one ms", api.secs(1), "0.001");
check("secs rounds to ms", api.secs(1234.6), "1.235");
check("secs negative", api.secs(-250), "-0.250");
check("secs large", api.secs(3600000), "3600.000");

// ---- hexToFf (opacity clamped to 0..100)
check("hexToFf full by default", api.hexToFf("#ff0000"), "0xff0000@1.00");
check("hexToFf null opacity is full", api.hexToFf("#ff0000", null), "0xff0000@1.00");
check("hexToFf half", api.hexToFf("#00ff00", 50), "0x00ff00@0.50");
check("hexToFf zero stays zero", api.hexToFf("#00ff00", 0), "0x00ff00@0.00");
check("hexToFf above 100 clamps", api.hexToFf("#000000", 150), "0x000000@1.00");
check("hexToFf below 0 clamps", api.hexToFf("#000000", -20), "0x000000@0.00");
check("hexToFf huge clamps", api.hexToFf("#123456", 1e9), "0x123456@1.00");
check("hexToFf without hash", api.hexToFf("abcdef", 25), "0xabcdef@0.25");
check("hexToFf one third", api.hexToFf("#ffffff", 33), "0xffffff@0.33");

// ---- enableExpr / videoOverlayWindowOutMs
sandbox.VE = { dur: 20000, ops: [] };
check("enableExpr plain range", api.enableExpr({ startMs: 1000, endMs: 3500 }), "enable='between(t,1.000,3.500)'");
check("enableExpr empty range", api.enableExpr({ startMs: 2000, endMs: 2000 }), null);
check("enableExpr reversed range", api.enableExpr({ startMs: 3000, endMs: 1000 }), null);
check("enableExpr start below zero clamps", api.enableExpr({ startMs: -500, endMs: 1000 }), "enable='between(t,0.000,1.000)'");
check("enableExpr end past the clip clamps", api.enableExpr({ startMs: 19000, endMs: 99999 }), "enable='between(t,19.000,20.000)'");
check("enableExpr range fully past the clip", api.enableExpr({ startMs: 30000, endMs: 40000 }), null);

sandbox.VE = { dur: 20000, ops: [{ tool: "cut", startMs: 2000, endMs: 4000 }, { tool: "speed", rate: 2 }] };
check("enableExpr follows cut and speed onto the output clock", api.enableExpr({ startMs: 10000, endMs: 14000 }), "enable='between(t,4.000,6.000)'");
check("enableExpr range inside a cut is empty", api.enableExpr({ startMs: 2500, endMs: 3500 }), null);
check("window of a play-once clip is capped at its footage length", api.videoOverlayWindowOutMs({ startMs: 10000, endMs: 14000, loop: false, videoDurationMs: 1000 }), { aOut: 4000, endOut: 5000 });
check("window of a long play-once clip is not capped", api.videoOverlayWindowOutMs({ startMs: 10000, endMs: 14000, loop: false, videoDurationMs: 90000 }), { aOut: 4000, endOut: 6000 });
check("window of a looping clip is not capped", api.videoOverlayWindowOutMs({ startMs: 10000, endMs: 14000, loop: true, videoDurationMs: 1000 }), { aOut: 4000, endOut: 6000 });
check("window without a known footage length is not capped", api.videoOverlayWindowOutMs({ startMs: 10000, endMs: 14000, loop: false, videoDurationMs: 0 }), { aOut: 4000, endOut: 6000 });
sandbox.VE = { dur: 20000, ops: [] };
check("window cap on the plain timeline", api.videoOverlayWindowOutMs({ startMs: 1000, endMs: 9000, loop: false, videoDurationMs: 2500 }), { aOut: 1000, endOut: 3500 });

// ---- shapeRadius / shapeMask
check("shapeRadius is a sixth of the short side", api.shapeRadius({ w: 60, h: 120 }), 10);
check("shapeRadius uses the shorter side when wide", api.shapeRadius({ w: 300, h: 30 }), 5);
check("shapeRadius never below 2", api.shapeRadius({ w: 6, h: 6 }), 2);
check("shapeRadius of an empty rect", api.shapeRadius({ w: 0, h: 0 }), 2);
check("shapeRadius rounds", api.shapeRadius({ w: 35, h: 35 }), 6);
check("shapeMask rect is null", api.shapeMask({ w: 60, h: 60 }, "rect"), null);
check("shapeMask undefined is null", api.shapeMask({ w: 60, h: 60 }, undefined), null);
check("shapeMask unknown shape is null", api.shapeMask({ w: 60, h: 60 }, "star"), null);
check(
  "shapeMask round",
  api.shapeMask({ w: 60, h: 60 }, "round"),
  "color=c=black:s=60x60:d=1,format=gray,geq=lum='255*lte(pow(max(0,max(10-X,X-(W-1-10))),2)+pow(max(0,max(10-Y,Y-(H-1-10))),2),100)',loop=loop=-1:size=1"
);
check(
  "shapeMask ellipse",
  api.shapeMask({ w: 40, h: 20 }, "ellipse"),
  "color=c=black:s=40x20:d=1,format=gray,geq=lum='255*lte(pow((X-W/2+0.5)/(W/2),2)+pow((Y-H/2+0.5)/(H/2),2),1)',loop=loop=-1:size=1"
);

// ---- rotationFilter
check("rotation 0", api.rotationFilter(0), "");
check("rotation 1 quarter", api.rotationFilter(1), "transpose=cclock");
check("rotation 2 quarters", api.rotationFilter(2), "hflip,vflip");
check("rotation 3 quarters", api.rotationFilter(3), "transpose=clock");
check("rotation 4 quarters is none", api.rotationFilter(4), "");
check("rotation 5 quarters wraps", api.rotationFilter(5), "transpose=cclock");
check("rotation -1 wraps to 3", api.rotationFilter(-1), "transpose=clock");
check("rotation missing is none", api.rotationFilter(undefined), "");
check("rotation null is none", api.rotationFilter(null), "");
check("rotation garbage is none", api.rotationFilter("abc"), "");
check("rotation numeric string", api.rotationFilter("2"), "hflip,vflip");
check("rotation fraction rounds", api.rotationFilter(1.4), "transpose=cclock");
check("rotation small negative fraction is none", api.rotationFilter(-0.4), "");

// ---- atempoChain: every stage inside 0.5..2, product equals the rate
const stagesOf = (rate) => api.atempoChain(rate).split(",").map((s) => {
  if (!s.startsWith("atempo=")) throw new Error(`bad stage ${s}`);
  return Number(s.slice(7));
});
check("atempo 1", api.atempoChain(1), "atempo=1");
check("atempo 1.5 is one stage", api.atempoChain(1.5), "atempo=1.5");
check("atempo 2 is one stage", api.atempoChain(2), "atempo=2");
check("atempo 0.5 is one stage", api.atempoChain(0.5), "atempo=0.5");
check("atempo 4 chains two", api.atempoChain(4), "atempo=2,atempo=2");
check("atempo 3", api.atempoChain(3), "atempo=2,atempo=1.5");
check("atempo 0.25 chains two", api.atempoChain(0.25), "atempo=0.5,atempo=0.5");
check("atempo 0.3", api.atempoChain(0.3), "atempo=0.5,atempo=0.6");
check("atempo 10", api.atempoChain(10), "atempo=2,atempo=2,atempo=2,atempo=1.25");
check("atempo 0.1", api.atempoChain(0.1), "atempo=0.5,atempo=0.5,atempo=0.5,atempo=0.8");
for (const rate of [0.1, 0.13, 0.25, 0.3, 0.49, 0.5, 0.75, 1, 1.3, 1.9, 2, 2.01, 3, 4, 7.7, 10, 100, 0.01]) {
  const stages = stagesOf(rate);
  const product = stages.reduce((p, s) => p * s, 1);
  check(`atempo ${rate}: product equals the rate`, Math.abs(product / rate - 1) < 1e-4, true);
  check(`atempo ${rate}: every stage inside 0.5..2`, stages.every((s) => s >= 0.4999 && s <= 2.0001), true);
}

if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
}
console.log(`video_graph_check: ${count} assertions OK`);
