// Video export "File name" field validation (ui/exportName.js: exportNameIsValid).
// Run:  node tests/export_name_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { exportNameIsValid, stripFormatExt } = require(path.join(__dirname, "..", "ui", "exportName.js"));

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
check("spaces before the dot do not hide a reserved name", exportNameIsValid("CON .mp4"), false);

check("a typed extension of the format is dropped", stripFormatExt("clip.mp4", "mp4"), "clip");
check("the extension match ignores letter case", stripFormatExt("clip.MP4", "mp4"), "clip");
check("a name without the extension stays", stripFormatExt("clip", "mp4"), "clip");
check("another extension stays", stripFormatExt("clip.webm", "mp4"), "clip.webm");
check("only a trailing extension goes", stripFormatExt("a.mp4.take", "mp4"), "a.mp4.take");
check("a name that is only the extension stays", stripFormatExt(".mp4", "mp4"), ".mp4");
check("empty input gives empty", stripFormatExt(null, "mp4"), "");

if (problems.length) {
  console.log("FAIL");
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log("PASS");
}
