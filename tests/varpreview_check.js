// Placeholder preview checks (ui/floatShared.js: varPreview).
// Run:  node tests/varpreview_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
if (typeof document === "undefined") global.document = { addEventListener() {} };
const { varPreview, needsMainWindowCopy, VAR_RE } = require(path.join(__dirname, "..", "ui", "floatShared.js"));

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

// A pill or folder member with a placeholder (or a self-filling token) is copied by the
// main window; everything else, a literal double brace included, copies directly.
check("placeholder", needsMainWindowCopy("Hello {#{name}#}"), true);
check("reserved token", needsMainWindowCopy("On {#{/date\}#}"), true);
check("choice token", needsMainWindowCopy("{#{tone|a,b}#}"), true);
check("literal double brace", needsMainWindowCopy("x {{name}} y"), false);
check("plain text", needsMainWindowCopy("hello"), false);
check("empty and null", [needsMainWindowCopy(""), needsMainWindowCopy(null)], [false, false]);
check("token only in a rotation entry", needsMainWindowCopy("plain", ["also plain", "now {#{x}#}"]), true);
check("no variants", needsMainWindowCopy("plain", undefined), false);
check("repeatable (global flag keeps no state)", [needsMainWindowCopy("{#{a}#}"), needsMainWindowCopy("{#{a}#}"), needsMainWindowCopy("{#{a}#}")], [true, true, true]);

// One source for the pattern: promptVars.js imports VAR_RE from floatShared.js and
// keeps no copy of its own.
const src = (f) => fs.readFileSync(path.join(__dirname, "..", "ui", f), "utf8");
const pv = src("promptVars.js");
check("VAR_RE is exported by floatShared", VAR_RE instanceof RegExp, true);
check("promptVars imports VAR_RE", /import \{[^}]*\bVAR_RE\b[^}]*\} from "\.\/floatShared\.js"/.test(pv), true);
check("promptVars has no pattern copy", /VAR_RE\s*=/.test(pv), false);

if (problems.length) { console.error(problems.join("\n")); process.exit(1); }
console.log("varpreview_check: ok");
