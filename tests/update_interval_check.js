// The update check interval defaults to daily (24 h) in the UI: the expert value
// default, the settings dropdown's entry for 24 h and its German label.
// The Rust side (unwrap_or(24.0), clamp_update_hours) is pinned by cargo tests.
// Run:  node tests/update_interval_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "ui");
const problems = [];

async function main() {
  const cfg = await import(pathToFileURL(path.join(uiDir, "expertConfig.js")).href);
  const sel = cfg.EXPERT_SELECTS.updateCheckH;
  if (!sel) problems.push("EXPERT_SELECTS.updateCheckH is missing");
  else {
    if (sel.def !== 24) problems.push(`updateCheckH default is ${sel.def}, expected 24`);
    if (!sel.options.includes(24)) problems.push("updateCheckH options do not offer 24 h");
  }
  const panel = fs.readFileSync(path.join(uiDir, "settingsPanel.js"), "utf8");
  if (!/\[24, "updateDaily"\]/.test(panel)) problems.push("the settings dropdown has no daily entry for 24 h");
  if (!/Number\.isFinite\(raw\) \? [^:]+ : 24;/.test(panel)) problems.push("fillUpdateIntervalSelect no longer falls back to 24 h");
  const de = (await import(pathToFileURL(path.join(uiDir, "i18n", "de.js")).href)).default;
  if (de.updateDaily !== "Täglich") problems.push(`German label for 24 h is ${JSON.stringify(de.updateDaily)}`);
}

main().then(() => {
  if (problems.length) {
    console.log(`FAIL (${problems.length})`);
    for (const p of problems) console.log("  - " + p);
    process.exit(1);
  }
  console.log("PASS: update check interval defaults to daily");
}, (e) => { console.log("FAIL: " + e.message); process.exit(1); });
