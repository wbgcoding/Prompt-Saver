// Every window script with a BLUR_GRACE_MS blur handler must also re-check
// after the grace window ends (an `else` branch that calls setTimeout, as tray.js has).
// A blur that fires while the window is still being shown is otherwise dropped
// for good and an always-on-top window stays up after a quick click-away.
// folder.js and floating.js close on blur without any grace on purpose and
// never mention BLUR_GRACE_MS, so they are not flagged.
// Run:  node tests/blur_grace_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");

const uiDir = path.join(__dirname, "..", "ui");
const problems = [];
let checked = 0;

for (const name of fs.readdirSync(uiDir).filter((f) => f.endsWith(".js")).sort()) {
  const src = fs.readFileSync(path.join(uiDir, name), "utf8");
  const start = src.search(/addEventListener\(\s*["']blur["']/);
  if (!/\bBLUR_GRACE_MS\b/.test(src.replace(/const BLUR_GRACE_MS\b[^;]*;/, ""))) continue;
  checked++;
  if (start < 0) {
    problems.push(`ui/${name}: BLUR_GRACE_MS is used but no blur handler was found`);
    continue;
  }
  // The handler runs up to the next top-level statement after its closing "});".
  const end = src.indexOf("\n});", start);
  const handler = src.slice(start, end < 0 ? undefined : end + 4);
  if (!/\belse\s*(?:\{\s*)?(?:[\w.]+\s*=\s*)?(?:clearTimeout\([^)]*\);\s*[\w.]+\s*=\s*)?setTimeout\(/.test(handler)) {
    problems.push(`ui/${name}: the BLUR_GRACE_MS blur handler has no "else setTimeout(" re-check branch`);
  }
}

if (checked === 0) problems.push("no window script with a BLUR_GRACE_MS blur handler found (check is stale)");

if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log(`ok (${checked} handlers checked)`);
}
