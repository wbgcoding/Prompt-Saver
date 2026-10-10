// The backend range table (src-tauri/src/ui_ranges.rs) must match the sliders in
// ui/expertConfig.js, or an out-of-range stored value is clamped to stale limits.
// Run:  node tests/ui_ranges_check.js      Exit: 0 = clean, 1 = problems.
"use strict";
const fs = require("fs");
const { render, OUT } = require("../tools/gen_ui_ranges.js");

render().then((expected) => {
  const actual = fs.existsSync(OUT) ? fs.readFileSync(OUT, "utf8").replace(/\r\n/g, "\n") : "";
  if (actual !== expected) {
    console.error("src-tauri/src/ui_ranges.rs is out of date with ui/expertConfig.js: run node tools/gen_ui_ranges.js");
    process.exit(1);
  }
  const rows = (expected.match(/^    \("/gm) || []).length;
  console.log(`ui_ranges_check: ${rows} ranges in step with expertConfig.js`);
}).catch((e) => { console.error("ui_ranges_check: " + e.message); process.exit(1); });
