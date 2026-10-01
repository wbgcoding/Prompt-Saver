// Every expert-menu path quoted in the in-app help (ui/help.js), EN and DE, must name a
// real tab and a real group of that tab, with the label the menu shows in that language.
// Run:  node tests/help_paths_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const os = require("os");
const { pathToFileURL } = require("url");

const root = path.join(__dirname, "..");
const read = (rel) => fs.readFileSync(path.join(root, rel), "utf8");

// Tab and group keys in menu order, straight from the EXPERT_TABS registry.
const main = read("ui/main.js");
const start = main.indexOf("const EXPERT_TABS = [");
const end = main.indexOf("\n];", start);
if (start < 0 || end < 0) { console.error("EXPERT_TABS not found in ui/main.js"); process.exit(1); }
const tabs = [];
for (const m of main.slice(start, end).matchAll(/title: "(expTab\w+|expGroup\w+)"/g)) {
  if (m[1].startsWith("expTab")) tabs.push({ key: m[1], groups: [] });
  else tabs[tabs.length - 1].groups.push(m[1]);
}

// Labels per language from the i18n blocks.
const lines = read("ui/i18n.js").split("\n");
const blockStart = (code) => lines.findIndex((l) => l.startsWith(`  ${code}: {`));
const labels = {};
for (const [code, next] of [["en", "de"], ["de", "es"]]) {
  const block = lines.slice(blockStart(code), blockStart(next)).join("\n");
  labels[code] = Object.fromEntries(
    [...block.matchAll(/\b((?:expTab|expGroup)\w+): "([^"]*)"/g)].map((m) => [m[1], m[2]]));
}
// A group heading whose string is not in i18n.js yet falls back to its agreed wording,
// so the help can be checked before that string lands; once it exists the real one wins.
const PENDING = { expGroupRemote: { en: "Remote access", de: "Fernzugriff" } };
const label = (lang, key) => labels[lang][key] ?? (PENDING[key] && PENDING[key][lang]);

(async () => {
  const tmp = path.join(os.tmpdir(), `help-paths-${process.pid}.mjs`);
  fs.writeFileSync(tmp, read("ui/help.js"));
  let HELP;
  try { HELP = (await import(pathToFileURL(tmp).href)).HELP_CONTENT; } finally { fs.unlinkSync(tmp); }

  const problems = [];
  let checked = 0;
  const PREFIX = { en: "Expert menu", de: "Experten-Menü" };
  for (const lang of ["en", "de"]) {
    // "Expert menu → Tab → Group" (the group part is optional), ended by . , ; " or the end.
    const re = new RegExp(`${PREFIX[lang]} → ([^→.,;"]+?)(?: → ([^.,;"→]+?))?(?=[.,;"]|$)`, "g");
    HELP[lang].forEach((art, i) => {
      const strings = [art.whatItIs, art.whatHappens, art.tip, ...(art.howTo || []), ...(art.settings || [])];
      for (const s of strings.filter(Boolean)) {
        for (const m of s.matchAll(re)) {
          checked++;
          const where = `${lang}[${i}] "${art.title}": ${m[0]}`;
          const tab = tabs.find((t) => label(lang, t.key) === m[1].trim());
          if (!tab) { problems.push(`${where}  (no tab "${m[1].trim()}")`); continue; }
          if (m[2] && !tab.groups.some((g) => label(lang, g) === m[2].trim()))
            problems.push(`${where}  (tab "${m[1].trim()}" has no group "${m[2].trim()}")`);
        }
      }
    });
  }
  if (!checked) problems.push("no expert-menu path found in the help at all");
  for (const p of problems) console.log("FAIL:", p);
  console.log(`${checked} paths checked, ${problems.length} problem(s)`);
  process.exit(problems.length ? 1 : 0);
})();
