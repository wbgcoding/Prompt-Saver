// Range table generator: copies the slider range of every numeric expert value in
// ui/expertConfig.js into src-tauri/src/ui_ranges.rs, so the backend can clamp a
// stored or imported value to the same limits the sliders enforce.
//
//   node tools/gen_ui_ranges.js            write src-tauri/src/ui_ranges.rs
//   node tools/gen_ui_ranges.js --check    fail when the file is out of date
//
// Re-run it whenever an expert value is added, removed or gets a new min or max.
// Entries with an `options` list are selects and are not emitted; an entry with
// `stops` takes its first and last stop as the range.
"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { pathToFileURL } = require("url");

const ROOT = path.resolve(__dirname, "..");
const OUT = path.join(ROOT, "src-tauri", "src", "ui_ranges.rs");

// The UI files are ES modules inside a package that is not marked as one, so they
// are imported from a private copy next to a package.json that says so.
async function loadValues() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "ui-ranges-"));
  try {
    fs.writeFileSync(path.join(dir, "package.json"), '{"type":"module"}');
    for (const f of ["expertConfig.js", "copyDefaults.js"]) {
      fs.copyFileSync(path.join(ROOT, "ui", f), path.join(dir, f));
    }
    return (await import(pathToFileURL(path.join(dir, "expertConfig.js")).href)).EXPERT_VALUES;
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function lit(v, key) {
  if (typeof v !== "number" || !Number.isFinite(v)) throw new Error(`${key}: range bound ${v} is not a finite number`);
  const s = String(v);
  if (/e/i.test(s)) throw new Error(`${key}: range bound ${s} needs a plain decimal form`);
  return s.includes(".") ? s : `${s}.0`;
}

async function render() {
  const rows = [];
  for (const [key, e] of Object.entries(await loadValues())) {
    if (e.options) continue;
    let min;
    let max;
    if (Array.isArray(e.stops) && e.stops.length) {
      min = e.stops[0];
      max = e.stops[e.stops.length - 1];
    } else {
      min = e.min;
      max = e.max;
    }
    if (min > max) throw new Error(`${key}: min ${min} is above max ${max}`);
    rows.push(`    ("${key}", ${lit(min, key)}, ${lit(max, key)}, ${e.topUnlimited ? "true" : "false"}),`);
  }
  return [
    "// Slider ranges of the numeric expert values. GENERATED: run",
    "// `node tools/gen_ui_ranges.js` after changing ui/expertConfig.js.",
    "//",
    "// Each row is key, min, max and whether the top stop means unlimited (0 stays 0).",
    "// Selects (preset lists) are not listed.",
    "",
    "pub(crate) const UI_VALUE_RANGES: &[(&str, f64, f64, bool)] = &[",
    ...rows,
    "];",
    "",
  ].join("\n");
}

module.exports = { render, OUT };

if (require.main === module) {
  render().then((text) => {
    if (process.argv.includes("--check")) {
      const cur = fs.existsSync(OUT) ? fs.readFileSync(OUT, "utf8").replace(/\r\n/g, "\n") : "";
      if (cur !== text) {
        console.error("ui_ranges.rs is out of date: run node tools/gen_ui_ranges.js");
        process.exit(1);
      }
      return;
    }
    fs.writeFileSync(OUT, text, { encoding: "utf8" });
    console.log("wrote " + path.relative(ROOT, OUT));
  }).catch((e) => { console.error("gen_ui_ranges: " + e.message); process.exit(1); });
}
