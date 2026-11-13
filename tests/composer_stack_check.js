// Composer tool-button stacking (ui/composerTools.js: composerToolCols): the column
// count of the tool grid follows the rendered input height, never leaves an empty row,
// and degrades safely for odd inputs.
// Run:  node tests/composer_stack_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { composerToolCols } = require(path.join(__dirname, "..", "ui", "composerTools.js"));

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

// Six visible buttons: one row up to the single-line height, then 2, 3 and 3 rows.
check("6 buttons, height 40", composerToolCols(40, 6), 6);
check("6 buttons, height 82", composerToolCols(82, 6), 3);
check("6 buttons, height 124", composerToolCols(124, 6), 2);
check("6 buttons, height 160", composerToolCols(160, 6), 2);

// Four visible buttons at full height: 3 fit, but 3 rows would leave the last row
// short of an empty one, so the result is 2 columns / 2 rows.
check("4 buttons, height 160", composerToolCols(160, 4), 2);
check("4 buttons, height 40", composerToolCols(40, 4), 4);

// Edges: one button, none, and an unusable height.
check("1 button, height 160", composerToolCols(160, 1), 1);
check("0 buttons", composerToolCols(160, 0), 1);
check("zero height", composerToolCols(0, 6), 6);
check("NaN height", composerToolCols(NaN, 6), 6);

// No empty row for any combination: rows = ceil(visible / cols) never drops below the
// rows the height allows by more than the rounding step, and cols never exceeds visible.
for (let v = 1; v <= 6; v++) {
  for (let h = 30; h <= 200; h += 2) {
    const cols = composerToolCols(h, v);
    const rows = Math.ceil(v / cols);
    if (cols < 1 || cols > v) problems.push(`cols out of range for h=${h} v=${v}: ${cols}`);
    if (rows * cols - v >= cols) problems.push(`empty row for h=${h} v=${v}: ${rows}x${cols}`);
  }
}

if (problems.length) {
  console.error("composer_stack_check FAILED:\n" + problems.join("\n"));
  process.exit(1);
}
console.log("composer_stack_check: ok");
