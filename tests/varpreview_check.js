// Placeholder preview checks (ui/floatShared.js: varPreview).
// Run:  node tests/varpreview_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
if (typeof document === "undefined") global.document = { addEventListener() {} };
const { varPreview } = require(path.join(__dirname, "..", "ui", "floatShared.js"));

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

check("choice variable and reserved name", varPreview("A {#{tone=casual|formal,casual}#} B {#{/date/}#}"), "A [tone] B [date]");
check("real slash fence", varPreview("On {#{/date\}#}"), "On [date]");
check("plain name", varPreview("Hi {#{name}#}!"), "Hi [name]!");
check("name with default", varPreview("{#{city=Berlin}#}"), "[city]");
check("name with options only", varPreview("{#{lang|en,de}#}"), "[lang]");
check("spaces are trimmed", varPreview("{#{  who  }#}"), "[who]");
check("no token stays as it is", varPreview("plain {text} here"), "plain {text} here");
check("empty input", varPreview(""), "");
check("null input", varPreview(null), "");
check("empty fence stays raw", varPreview("x {#{ }#} y"), "x {#{ }#} y");

// The satellite copy of the pattern must stay identical to the main window's.
const src = (f) => fs.readFileSync(path.join(__dirname, "..", "ui", f), "utf8");
const grab = (text, re) => (text.match(re) || [])[1];
const a = grab(src("promptVars.js"), /export const VAR_RE = (\/.*\/g);/);
const b = grab(src("floatShared.js"), /const VAR_PATTERN = (\/.*\/g);/);
check("pattern copy matches VAR_RE", b, a);

if (problems.length) { console.error(problems.join("\n")); process.exit(1); }
console.log("varpreview_check: ok");
