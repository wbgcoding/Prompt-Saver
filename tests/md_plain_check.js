// Markdown marks stripped for one-line previews (ui/floatShared.js: mdPlain).
// Run:  node tests/md_plain_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
if (typeof document === "undefined") global.document = { addEventListener() {} };
const { mdPlain } = require(path.join(__dirname, "..", "ui", "floatShared.js"));

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

check("headings", mdPlain("# Heading one\n## Heading two"), "Heading one\nHeading two");
check("bold and italic", mdPlain("some **bold** and *it*"), "some bold and it");
check("task and numbered list", mdPlain("- [ ] task\n1. step"), "task\nstep");
check("done task and bullets", mdPlain("- [x] done\n* star item\n+ plus item\n2) paren"), "done\nstar item\nplus item\nparen");
check("link and image", mdPlain("[site](https://x.y) ![pic](a.png)"), "site pic");
check("snake_case and spaced stars stay", mdPlain("use my_var_name and 2 * 3 * 4"), "use my_var_name and 2 * 3 * 4");
check("tight stars between digits stay", mdPlain("2*3*4"), "2*3*4");
check("blockquote", mdPlain("> quoted"), "quoted");
check("fenced code keeps its content", mdPlain("```js\ncode()\n```"), "code()");
check("tilde fence", mdPlain("~~~\nx\n~~~"), "x");
check("inline code", mdPlain("`x`"), "x");
check("underscore bold and strike", mdPlain("__b__ and ~~gone~~"), "b and gone");
check("horizontal rules", mdPlain("a\n---\nb\n* * *\nc\n___\nd"), "a\nb\nc\nd");
check("table separator row", mdPlain("| a | b |\n|---|:--:|\n| 1 | 2 |"), "| a | b |\n| 1 | 2 |");
check("placeholder token survives", mdPlain("Hi {#{my_name=Bob}#}, **go**"), "Hi {#{my_name=Bob}#}, go");
check("empty input", mdPlain(""), "");
check("null input", mdPlain(null), "");
check("plain text unchanged", mdPlain("Just a sentence. Nothing here."), "Just a sentence. Nothing here.");

if (problems.length) { console.error(problems.join("\n")); process.exit(1); }
console.log("md_plain_check: ok");
