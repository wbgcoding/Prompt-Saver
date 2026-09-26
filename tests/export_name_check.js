// Video export "File name" field validation (ui/exportName.js: exportNameIsValid).
// Run:  node tests/export_name_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { exportNameIsValid } = require(path.join(__dirname, "..", "ui", "exportName.js"));

const problems = [];
const check = (label, actual, expected) => {
  if (actual !== expected) problems.push(`${label}: got ${actual}, expected ${expected}`);
};

check("a plain name is valid", exportNameIsValid("Demo 1"), true);
check("the auto stem shape is valid", exportNameIsValid("clip-edit"), true);
check("a dotted version-looking name is valid", exportNameIsValid("My Video v1.2.3"), true);

// A bare ".." only means "walk up a level" when it stands alone -- inside a
// longer name it is just two dots and must stay legal.
check("two dots inside a longer name stays legal", exportNameIsValid("v1..2"), true);
check("two dots between two letters stays legal", exportNameIsValid("a..b"), true);

check("empty is refused", exportNameIsValid(""), false);
check("blank is refused", exportNameIsValid("   "), false);
check("a lone dot is refused", exportNameIsValid("."), false);
check("a lone dot-dot is refused", exportNameIsValid(".."), false);
check("a path separator is refused", exportNameIsValid("a/b"), false);
check("a backslash is refused", exportNameIsValid("a\\b"), false);
check("a colon is refused", exportNameIsValid("a:b"), false);
check("a control character is refused", exportNameIsValid("a\u0007b"), false);
check("a trailing dot is refused", exportNameIsValid("clip."), false);
check("a reserved device name is refused", exportNameIsValid("CON"), false);
check("a reserved device name is case-insensitive", exportNameIsValid("con"), false);
check("a reserved device name stays reserved with an extension-looking suffix", exportNameIsValid("CON.take2"), false);
check("only an exact reserved segment is refused, not a prefix", exportNameIsValid("Console"), true);

if (problems.length) {
  console.log("FAIL");
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log("PASS");
}
