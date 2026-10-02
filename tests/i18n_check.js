// Translation consistency check.
//
//   * every language object must carry exactly the same keys as `en`,
//   * every t("literal") / data-i18n="…" key used in ui/ must exist in `en`,
//   * the static fallback text of every data-i18n element in ui/*.html equals its
//     `en` string (keys with {app} excepted),
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
  // A placeholder dropped in translation ships a sentence with a hole in it: the
  // count, the app name or the value never appears. Compare against en.
  if (lang !== "en") {
    for (const k of keys) {
      if (!en.has(k)) continue;
      // Only the tokens t()/the call sites really substitute. The variable-syntax
      // examples ("{#{Name}#}") are translated on purpose and are not holes.
      const holes = (s) => (String(s).match(/\{(?:app|n|v|f|name|path|mode)\}/g) || []).sort().join(",");
      const want = holes(I18N.en[k]);
      const got = holes(I18N[lang][k]);
      if (want !== got) problems.push(`${lang}: '${k}' has placeholders [${got}] instead of [${want}]`);
    }
  }
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

// The static text of every data-i18n element in a ui/*.html page is only a
// fallback shown until applyI18n() runs. It must equal the English string, or
// the page flashes (or, when the script fails, keeps) a different wording.
// Keys whose English string carries {app} are skipped: the fallback cannot spell
// the product name.
const decodeEntities = (s) =>
  s
    .replace(/&hellip;/g, "…")
    .replace(/&times;/g, "×")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, "&");
const squash = (s) => s.replace(/\s+/g, " ").trim();
for (const file of fs.readdirSync(UI).filter((f) => f.endsWith(".html"))) {
  const html = fs.readFileSync(path.join(UI, file), "utf8");
  const re = /<([a-z0-9]+)[^>]*\sdata-i18n="([^"]+)"[^>]*>([^<]*)<\/\1>/g;
  for (const m of html.matchAll(re)) {
    const key = m[2];
    const fallback = squash(decodeEntities(m[3]));
    if (!fallback || typeof I18N.en[key] !== "string") continue;
    if (I18N.en[key].includes("{app}")) continue;
    const want = squash(I18N.en[key]);
    if (fallback === want) continue;
    const line = html.slice(0, m.index).split("\n").length;
    problems.push(`fallback: ${file}:${line} ${key} html="${fallback}" en="${want}"`);
  }
}

// Many keys are reached indirectly (label: "valFoo", `tip${Key}`), so an unused
// key is a hint, not an error. Prefixes that the code builds at runtime count as
// referenced, otherwise the note is all noise.
const dynamic = ["tip", "flag", "val", "exp", "helpSec"].filter((p) => code.includes("`" + p + "${"));
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
