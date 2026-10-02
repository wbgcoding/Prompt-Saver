// Loads the UI dictionaries for the Node tests. The texts live in one file per
// language (ui/i18n/<code>.js, plus float.js for the compact strings); this helper
// evaluates them and can also rebuild the old single-file text for the regex-based
// checks. Tests read the dictionaries through here, never by opening a language file.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const DIR = path.join(__dirname, "..", "ui", "i18n");
const LANGS = ["en", "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro"];

const read = (name) => fs.readFileSync(path.join(DIR, name + ".js"), "utf8").replace(/\r\n/g, "\n");

function evalDefault(name) {
  const sandbox = {};
  vm.runInNewContext(read(name).replace(/^export default /m, "this.__d = "), sandbox);
  return sandbox.__d;
}

// The lines between "export default {" and the closing "};".
function bodyOf(name) {
  const lines = read(name).split("\n");
  return lines.slice(2, lines.lastIndexOf("};")).join("\n");
}

const I18N = Object.fromEntries(LANGS.map((c) => [c, evalDefault(c)]));
const FLOAT_TXT = evalDefault("float");

function combinedSource() {
  let out = "// Shared translations: main window (I18N), floating pills (FLOAT_TXT).\n" +
    "// 11 languages; EN is the fallback for missing keys.\n";
  out += `export const LANGS = [\n  ${LANGS.map((c) => `"${c}"`).join(", ")},\n];\n\nexport const I18N = {\n`;
  for (const c of LANGS) out += `  ${c}: {\n${bodyOf(c)}\n  },\n`;
  out += `};\n\n// Compact strings for the floating pill windows.\nexport const FLOAT_TXT = {\n${bodyOf("float")}\n};\n`;
  return out;
}

module.exports = { LANGS, I18N, FLOAT_TXT, combinedSource, DIR };
