// The shot stub (tests/ui/tauri_stub.js) carries a copy of the tray menu texts so side-window
// shots show the real labels. This check keeps that copy equal to the Rust tables in model.rs.
// Run:  node tests/ui/stub_tray_text_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");

const root = path.join(__dirname, "..", "..");
const rust = fs.readFileSync(path.join(root, "src-tauri", "src", "model.rs"), "utf8").replace(/\r\n/g, "\n");
const stub = fs.readFileSync(path.join(__dirname, "tauri_stub.js"), "utf8").replace(/\r\n/g, "\n");

// Body of `fn <name>(lang: &str) ...` up to the closing brace of its match.
function matchBody(name) {
  const start = rust.indexOf("fn " + name + "(");
  if (start < 0) throw new Error("model.rs has no fn " + name);
  const m = rust.indexOf("match lang {", start);
  const end = rust.indexOf("\n    }\n", m);
  return rust.slice(m, end);
}

// lang -> array of strings of the tuple (or the single string)
function parse(name) {
  const out = {};
  for (const line of matchBody(name).split("\n")) {
    const m = line.match(/^\s*("(\w+)"|_)\s*=>\s*(.+?),?\s*$/);
    if (!m) continue;
    const lang = m[2] || "en";
    if (m[2] === undefined && out.en) continue;
    out[lang] = [...m[3].matchAll(/"([^"]*)"/g)].map((x) => x[1]);
  }
  return out;
}

const labels = parse("tray_labels");
const sections = parse("tray_section_labels");
const search = parse("tray_search_placeholder");

const lit = stub.match(/const TRAY_TEXT = (\{[\s\S]*?\n\});/);
if (!lit) { console.log("FAIL: TRAY_TEXT not found in tauri_stub.js"); process.exit(1); }
const table = Function("return " + lit[1])();

let bad = 0;
const langs = Object.keys(labels);
const fields = [
  ["open", (l) => (labels[l] || [])[0]],
  ["quit", (l) => (labels[l] || [])[1]],
  ["recent", (l) => (sections[l] || [])[0]],
  ["favs", (l) => (sections[l] || [])[1]],
  ["search", (l) => (search[l] || [])[0]],
];
if (langs.length !== 11) { console.log("FAIL: expected 11 languages in tray_labels, found " + langs.length); bad++; }
for (const lang of langs) {
  if (!table[lang]) { console.log("FAIL: " + lang + ": missing in stub TRAY_TEXT"); bad++; continue; }
  for (const [field, get] of fields) {
    const want = get(lang);
    const have = table[lang][field];
    if (want === undefined) { console.log("FAIL: " + lang + " " + field + ": missing in Rust"); bad++; }
    else if (have !== want) { console.log("FAIL: " + lang + " " + field + ': stub "' + have + '" vs Rust "' + want + '"'); bad++; }
  }
}
for (const lang of Object.keys(table)) if (!labels[lang]) { console.log("FAIL: " + lang + ": in stub but not in Rust"); bad++; }
if (bad) process.exit(1);
console.log("ok: tray texts equal in " + langs.length + " languages");
