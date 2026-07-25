// Translation consistency check.
//
//   * every language object must carry exactly the same keys as `en`,
//   * every t("literal") / data-i18n="…" key used in ui/ must exist in `en`,
//   * reports strings defined in `en` that nothing references (info only).
//
// Run:  node tests/i18n_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const REPO = path.resolve(__dirname, "..");
const UI = path.join(REPO, "ui");

const src = fs.readFileSync(path.join(UI, "i18n.js"), "utf8");
// i18n.js is a plain script, not a module: evaluate it in a bare sandbox and read
// the object it declares.
// It is an ES module; drop the `export` keywords so a plain sandbox can run it.
const plain = src.replace(/^export\s+/gm, "");
const sandbox = { window: {}, document: {}, navigator: { language: "en" } };
vm.runInNewContext(plain + "\nthis.__I18N = typeof I18N !== 'undefined' ? I18N : null;", sandbox);
const I18N = sandbox.__I18N;
if (!I18N) {
  console.log("FAIL i18n.js did not define I18N");
  process.exit(1);
}

const langs = Object.keys(I18N);
const en = new Set(Object.keys(I18N.en));
const problems = [];

for (const lang of langs) {
  const keys = new Set(Object.keys(I18N[lang]));
  for (const k of en) if (!keys.has(k)) problems.push(`${lang}: missing '${k}'`);
  for (const k of keys) if (!en.has(k)) problems.push(`${lang}: has '${k}' which en does not`);
  const empty = [...keys].filter((k) => !String(I18N[lang][k]).trim());
  for (const k of empty) problems.push(`${lang}: '${k}' is empty`);
}

// Keys actually referenced from the UI code.
const code = fs
  .readdirSync(UI)
  .filter((f) => f.endsWith(".js") || f.endsWith(".html"))
  .filter((f) => f !== "i18n.js")
  .map((f) => fs.readFileSync(path.join(UI, f), "utf8"))
  .join("\n");

const used = new Set();
for (const m of code.matchAll(/\bt\("([^"]+)"\)/g)) used.add(m[1]);
for (const m of code.matchAll(/data-i18n(?:-title|-ph|-aria)?="([^"]+)"/g)) used.add(m[1]);

for (const k of used) if (!en.has(k)) problems.push(`ui code uses '${k}', which en does not define`);

// Many keys are reached indirectly (label: "valFoo", `tip${Key}`), so an unused
// key is a hint, not an error. Prefixes that the code builds at runtime count as
// referenced, otherwise the note is all noise.
const dynamic = ["tip", "flag", "val", "exp"].filter((p) => code.includes("`" + p + "${"));
const unused = [...en].filter((k) =>
  !used.has(k) &&
  !code.includes(`"${k}"`) &&
  !code.includes(`'${k}'`) &&
  !code.includes(`.${k}`) && // read straight off the language object (snip.js)
  !dynamic.some((p) => k.startsWith(p) && k.length > p.length && k[p.length] === k[p.length].toUpperCase()));

console.log(`languages: ${langs.length}, keys per language: ${en.size}`);
if (unused.length) console.log(`note: ${unused.length} key(s) never referenced literally: ${unused.join(", ")}`);
for (const p of problems) console.log("FAIL", p);
console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
process.exit(problems.length ? 1 : 0);
