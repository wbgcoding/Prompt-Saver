// Composer tool-button stacking (ui/composerTools.js: composerToolCols): the column
// count of the tool grid follows the rendered input height, never leaves an empty row,
// and degrades safely for odd inputs.
// Run:  node tests/composer_stack_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { composerColChoices, composerToolCols, pickComposerCols } = require(path.join(__dirname, "..", "ui", "composerTools.js"));

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

// Column choices, fewest first.
check("choices for 6", composerColChoices(6), [1, 2, 3, 6]);
check("choices for 4", composerColChoices(4), [1, 2, 4]);
check("choices for 0", composerColChoices(0), [1]);

// The text box height depends on the column count: stacking widens the box, so the same
// text needs fewer lines. Model: a text of `chars` characters in a box whose line holds
// 40 characters at 6 columns, 52 at 3, 58 at 2, 64 at 1; 19 px per line plus 22 px frame.
const PER_LINE = { 6: 40, 3: 52, 2: 58, 1: 64 };
const heightFor = (chars) => (cols) => Math.min(160, 22 + 19 * Math.max(1, Math.ceil(chars / PER_LINE[cols])));
// Short text: one row of buttons.
check("short text keeps one row", pickComposerCols(6, heightFor(30)), 6);
// 90 characters: 3 lines (79 px) at 6 columns, but only 2 lines (60 px) at 3 columns.
// Stacking would shrink the box below its rows, so the row stays (the old rule flipped here).
check("borderline text stays one row", pickComposerCols(6, heightFor(90)), 6);
// 110 characters: 3 lines even at 3 columns, so the stack holds.
check("longer text stacks", pickComposerCols(6, heightFor(110)), 3);
// Typing one character at a time never makes the column count go back up.
let prevCols = 6;
for (let n = 1; n <= 600; n++) {
  const cols = pickComposerCols(6, heightFor(n));
  if (cols > prevCols) problems.push(`typing char ${n} went back from ${prevCols} to ${cols} columns`);
  prevCols = cols;
}
// The old rule (columns from the height at the current layout) on the same model flips;
// this pins the model so the check above cannot pass on a model that never flips.
let cur = 6, flips = 0;
for (let n = 80; n <= 110; n++) {
  const next = composerToolCols(heightFor(n)(cur), 6);
  if (next !== cur) flips++;
  const back = composerToolCols(heightFor(n)(next), 6);
  if (back !== next) flips++;
  cur = back;
}
if (flips === 0) problems.push("model never flips under the old rule: the typing check proves nothing");

if (problems.length) {
  console.error("composer_stack_check FAILED:\n" + problems.join("\n"));
  process.exit(1);
}
console.log("composer_stack_check: ok");
