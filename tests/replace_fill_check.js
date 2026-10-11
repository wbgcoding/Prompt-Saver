// A text template is filled with String.replace("{name}", value). Passed as the
// replacement STRING, a value that holds `$&`, `$'`, `` $` `` or `$1` (a prompt, view or tag
// called "Price $&", a path with a dollar sign) is read as a pattern and the toast shows
// the template back inside the name. A replacer function is read literally.
//
// So every ui/*.js call .replace("{key}", value) must pass either a replacer function, or a
// value that cannot carry user text: a string literal without `$`, a number, a String() /
// Number() / Math call, a t() translation, an fmt*() formatter, a .length or a
// .toLocaleString(...) number.
//
// Run:  node tests/replace_fill_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");

const UI = path.join(__dirname, "..", "ui");

// Index of the bracket closing the one opened at `open`, skipping quoted text.
function closing(s, open) {
  let depth = 0;
  let quote = null;
  for (let i = open; i < s.length; i++) {
    const c = s[i];
    if (quote) {
      if (c === "\\") i++;
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'" || c === "`") quote = c;
    else if ("([{".includes(c)) depth++;
    else if (")]}".includes(c) && --depth === 0) return i;
  }
  return -1;
}

// True when `arg` is one call `name(...)` and nothing else.
function isSingleCall(arg, namePattern) {
  const m = new RegExp(`^(?:${namePattern})\\(`).exec(arg);
  if (!m) return false;
  return closing(arg, m[0].length - 1) === arg.length - 1;
}

function isLiteralSafe(arg) {
  if (/^\(?\s*(?:\(\s*\)|[A-Za-z_$][\w$]*|_)\s*\)?\s*=>/.test(arg)) return true; // replacer function
  if (/^-?\d+(\.\d+)?$/.test(arg)) return true;
  if (/^"[^"\\$]*"$/.test(arg) || /^'[^'\\$]*'$/.test(arg)) return true; // a literal without a dollar sign
  if (isSingleCall(arg, "String|Number|Math\\.\\w+|t|fmt\\w*")) return true;
  if (/^[\w$.?\[\]]+\.length$/.test(arg)) return true;
  if (/^[\w$.?\[\]()]+\.toLocaleString\((?:LANG)?\)$/.test(arg)) return true;
  return false;
}

// Every .replace("{key}", arg) call: { file, line, arg, start, end } with start/end
// the arg's offsets in the file text.
function replaceSites(text) {
  const out = [];
  const re = /\.replace\(\s*(["'`])\{[A-Za-z_]+\}\1\s*,\s*/g;
  let m;
  while ((m = re.exec(text))) {
    const start = m.index + m[0].length;
    let depth = 0;
    let quote = null;
    let i = start;
    for (; i < text.length; i++) {
      const c = text[i];
      if (quote) {
        if (c === "\\") i++;
        else if (c === quote) quote = null;
      } else if (c === '"' || c === "'" || c === "`") quote = c;
      else if ("([{".includes(c)) depth++;
      else if (")]}".includes(c)) {
        if (depth === 0) break;
        depth--;
      } else if (c === "," && depth === 0) break;
    }
    out.push({ line: text.slice(0, m.index).split("\n").length, arg: text.slice(start, i).trim(), start, end: i });
  }
  return out;
}

function uiFiles(dir = UI) {
  const files = [];
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (e.name !== "i18n") files.push(...uiFiles(p));
    } else if (e.name.endsWith(".js")) files.push(p);
  }
  return files;
}

function unsafeSites() {
  const bad = [];
  for (const file of uiFiles()) {
    const text = fs.readFileSync(file, "utf8");
    for (const s of replaceSites(text)) {
      if (!isLiteralSafe(s.arg)) bad.push({ file, ...s });
    }
  }
  return bad;
}

module.exports = { unsafeSites, replaceSites, isLiteralSafe };

if (require.main === module) {
  const problems = [];

  // The classifier itself.
  const safe = ['() => name', '(_) => x', '"All"', "42", "String(n)", "t(\"k\")", "fmtBytes(b, LANG)", "list.length", "n.toLocaleString(LANG)"];
  const risky = ["name", "v.name", "String(a) + b", "path || \"\"", "t(\"k\") + name", "\"$&\"", "x ? 1 : 2"];
  for (const a of safe) if (!isLiteralSafe(a)) problems.push(`classifier: ${a} should be accepted`);
  for (const a of risky) if (isLiteralSafe(a)) problems.push(`classifier: ${a} should be refused`);

  for (const s of unsafeSites()) {
    problems.push(`${path.relative(path.join(__dirname, ".."), s.file).replace(/\\/g, "/")}:${s.line}: .replace(..., ${s.arg}) passes the value as a replacement string; use () => value`);
  }
  if (problems.length) {
    console.error("replace_fill_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("replace_fill_check: ok");
}
