// Live video-overlay preview style (ui/styles/video.css .ve-live-video):
// export scales a video overlay with force_original_aspect_ratio=decrease and
// pads the rest black (see the video branch of buildVideoChain() in
// ui/video.js), so the preview must not stretch the footage to fill its box.
// Run:  node tests/video_overlay_style_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");

const CSS_PATH = path.join(__dirname, "..", "ui", "styles", "video.css");
const css = fs.readFileSync(CSS_PATH, "utf8");

const problems = [];

const match = css.match(/\.ve-live-video\s*\{([^}]*)\}/);
if (!match) {
  problems.push(".ve-live-video rule not found in video.css");
} else {
  const body = match[1];
  if (!/object-fit\s*:\s*contain\b/.test(body)) {
    problems.push(`.ve-live-video must use object-fit: contain to match export's letterbox scaling, got: ${body.trim()}`);
  }
  if (/object-fit\s*:\s*fill\b/.test(body)) {
    problems.push(".ve-live-video still stretches the footage with object-fit: fill");
  }
}

// Export's own pad color for a video overlay (see buildVideoChain) is black --
// the letterboxed bars the preview now shows via object-fit: contain must be
// the same color, or a bright-background window would visibly disagree with
// what export actually produces.
const imageMatch = css.match(/\.ve-live-image\s*\{([^}]*)\}/);
if (imageMatch && /object-fit\s*:\s*contain\b/.test(imageMatch[1])) {
  problems.push(".ve-live-image should stay object-fit: fill -- an image overlay's export filter scales to the exact rect with no aspect preservation, unlike a video overlay");
}

if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log("PASS");
}
