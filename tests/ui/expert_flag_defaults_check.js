// Expert flag defaults (ui/expertConfig.js): a flag the backend treats as ON when unset
// (store.rs: `!= Some(&false)`) must sit in FLAG_LABELS (default on), not OPT_FLAG_LABELS
// (default off), or a clean install shows the switch off while the feature runs.
// Run:  node tests/ui/expert_flag_defaults_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { pathToFileURL } = require("url");

const DEFAULT_ON_IN_BACKEND = ["dataDirLock"];

async function main() {
  const cfg = await import(pathToFileURL(path.join(__dirname, "..", "..", "ui", "expertConfig.js")).href);
  const problems = [];
  for (const key of DEFAULT_ON_IN_BACKEND) {
    if (!(key in cfg.FLAG_LABELS)) problems.push(`${key} is not a default-on flag (FLAG_LABELS)`);
    if (key in cfg.OPT_FLAG_LABELS) problems.push(`${key} is listed as default-off (OPT_FLAG_LABELS)`);
  }
  if (problems.length) {
    console.error("expert_flag_defaults_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("expert_flag_defaults_check: ok");
}
main().catch((e) => { console.error(e); process.exit(1); });
