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
const main = require("./ui_sources.js").mainWindowSource();
const start = main.indexOf("const EXPERT_TABS = [");
const end = main.indexOf("\n];", start);
if (start < 0 || end < 0) { console.error("EXPERT_TABS not found in ui/main.js"); process.exit(1); }
const tabs = [];
for (const m of main.slice(start, end).matchAll(/title: "(expTab\w+|expGroup\w+)"/g)) {
  if (m[1].startsWith("expTab")) tabs.push({ key: m[1], groups: [] });
  else tabs[tabs.length - 1].groups.push(m[1]);
}

// Labels per language from the i18n blocks.
const lines = require("./i18n_load.js").combinedSource().split("\n");
const blockStart = (code) => lines.findIndex((l) => l.startsWith(`  ${code}: {`));
const labels = {};
for (const [code, next] of [["en", "de"], ["de", "es"]]) {
  const block = lines.slice(blockStart(code), blockStart(next)).join("\n");
  labels[code] = Object.fromEntries(
    [...block.matchAll(/\b((?:expTab|expGroup)\w+): "([^"]*)"/g)].map((m) => [m[1], m[2]]));
}
const label = (lang, key) => labels[lang][key];

// Quotes in the help that are not UI labels (spoken words, symptom phrases, examples).
// Each entry: [quote, why it is allowed].
const NOT_LABELS = {
  en: [
    ["comma", "spoken dictation word"],
    ["new line", "spoken dictation word"],
    ["literal", "spoken dictation word"],
    ["2/5", "example of a rotation counter"],
    ["My screenshot came out empty", "symptom phrase, a question heading"],
    ["The app will not start", "symptom phrase, a question heading"],
    ["I cannot find a setting", "symptom phrase, a question heading"],
    ["A hotkey does not do anything", "symptom phrase, a question heading"],
  ],
  de: [
    ["Komma", "spoken dictation word"],
    ["neue Zeile", "spoken dictation word"],
    ["wörtlich", "spoken dictation word"],
    ["2/5", "example of a rotation counter"],
    ["Mein Screenshot ist leer", "symptom phrase, a question heading"],
    ["Die App startet nicht", "symptom phrase, a question heading"],
    ["Ich finde eine Einstellung nicht", "symptom phrase, a question heading"],
    ["Ein Tastenkürzel bewirkt nichts", "symptom phrase, a question heading"],
  ],
};

(async () => {
  const tmp = path.join(os.tmpdir(), `help-paths-${process.pid}.mjs`);
  fs.writeFileSync(tmp, read("ui/help.js"));
  let HELP;
  try { HELP = (await import(pathToFileURL(tmp).href)).HELP_CONTENT; } finally { fs.unlinkSync(tmp); }

  const problems = [];
  let checked = 0;
  const PREFIX = { en: "Expert menu", de: "Expertenmenü" };
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

  // Every quoted label (DE „…“, EN “…” or "...") must equal a UI string of that language,
  // or the title of a help article ("see X below"), or be listed in NOT_LABELS.
  const { I18N, FLOAT_TXT } = require("./i18n_load.js");
  const uiStrings = (lang) => {
    const out = [];
    const walk = (v) => {
      if (typeof v === "string") out.push(v);
      else if (v && typeof v === "object") Object.values(v).forEach(walk);
    };
    walk(I18N[lang]);
    walk(FLOAT_TXT[lang]);
    return out;
  };
  // Trailing "…" / ":" ignored; a menu item may start lower case in a sentence.
  const norm = (t) => {
    const x = t.replace(/\s+/g, " ").replace(/(?:…|\.\.\.|:)+$/, "").trim();
    return x.charAt(0).toLowerCase() + x.slice(1);
  };
  // A UI string with {n}-style placeholders matches any text in that spot.
  const toMatcher = (t) => {
    const parts = norm(t).split(/\{\w+\}/).map((x) => x.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
    return new RegExp(`^${parts.join(".+?")}$`);
  };
  const QUOTE = {
    de: /„([^“”"]+)[“”]/g,
    en: /“([^“”]+)”|"([^"]+)"/g,
  };
  let quotes = 0;
  for (const lang of ["en", "de"]) {
    const strs = uiStrings(lang);
    const exact = new Set(strs.map(norm));
    for (const art of HELP[lang]) exact.add(norm(art.title));
    const matchers = strs.filter((t) => /\{\w+\}/.test(t)).map(toMatcher);
    const allow = new Set(NOT_LABELS[lang].map(([q]) => norm(q)));
    HELP[lang].forEach((art, i) => {
      const strings = [art.whatItIs, art.whatHappens, art.tip, ...(art.howTo || []), ...(art.settings || [])];
      for (const s of strings.filter(Boolean)) {
        for (const m of s.matchAll(QUOTE[lang])) {
          const q = norm(m[1] || m[2]);
          quotes++;
          if (allow.has(q) || exact.has(q) || matchers.some((r) => r.test(q))) continue;
          problems.push(`${lang}[${i}] "${art.title}": quoted label "${m[1] || m[2]}" is no ${lang} UI string`);
        }
      }
    });
  }
  for (const p of problems) console.log("FAIL:", p);
  console.log(`${quotes} quoted labels checked, ${checked} paths checked, ${problems.length} problem(s)`);
  process.exit(problems.length ? 1 : 0);
})();
