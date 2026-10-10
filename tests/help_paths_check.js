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
  // Help areas: the tab list and its labels come from ui/helpView.js; labels reuse the
  // expert-menu tab names (basics has its own string).
  const viewSrc = read("ui/helpView.js");
  const sectionsM = viewSrc.match(/HELP_SECTIONS = \[([^\]]*)\]/);
  const labelsM = viewSrc.match(/HELP_SECTION_LABELS = \{([^}]*)\}/);
  if (!sectionsM || !labelsM) {
    problems.push("HELP_SECTIONS / HELP_SECTION_LABELS not found in ui/helpView.js");
  } else {
    const SECTIONS = [...sectionsM[1].matchAll(/"(\w+)"/g)].map((m) => m[1]);
    const SEC_KEY = Object.fromEntries([...labelsM[1].matchAll(/(\w+): "(\w+)"/g)].map((m) => [m[1], m[2]]));
    const secLabel = (lang, id) => (SEC_KEY[id] && SEC_KEY[id].startsWith("helpSec") ? I18N[lang][SEC_KEY[id]] : label(lang, SEC_KEY[id]));
    for (const id of SECTIONS) {
      if (!SEC_KEY[id]) problems.push(`area "${id}" has no label key in HELP_SECTION_LABELS`);
    }
    // Every article sits in a known area; EN and DE agree at every index.
    if (HELP.en.length !== HELP.de.length) problems.push(`EN has ${HELP.en.length} articles, DE has ${HELP.de.length}`);
    HELP.en.forEach((art, i) => {
      const de = HELP.de[i];
      if (!SECTIONS.includes(art.section)) problems.push(`en[${i}] "${art.title}": area "${art.section}" is not in HELP_SECTIONS`);
      if (de && !SECTIONS.includes(de.section)) problems.push(`de[${i}] "${de.title}": area "${de.section}" is not in HELP_SECTIONS`);
      if (de && de.section !== art.section) problems.push(`index ${i}: EN "${art.title}" is in "${art.section}" but DE "${de.title}" is in "${de.section}"`);
    });
    // Areas come in HELP_SECTIONS order, each as one run.
    for (const lang of ["en", "de"]) {
      let last = -1;
      const seen = new Set();
      HELP[lang].forEach((art, i) => {
        const k = SECTIONS.indexOf(art.section);
        if (k < 0) return;
        if (k < last) problems.push(`${lang}[${i}] "${art.title}": area "${art.section}" comes after a later area`);
        else if (k > last) {
          if (seen.has(k)) problems.push(`${lang}[${i}] "${art.title}": area "${art.section}" is not one contiguous run`);
          seen.add(k);
        }
        last = Math.max(last, k);
      });
    }
    // An article that quotes expert-menu paths names its own tab in at least one of them.
    for (const lang of ["en", "de"]) {
      const re = new RegExp(`${PREFIX[lang]} → ([^→.,;"]+?)(?: → ([^.,;"→]+?))?(?=[.,;"]|$)`, "g");
      HELP[lang].forEach((art, i) => {
        if (art.section === "basics" || !SECTIONS.includes(art.section)) return;
        const strings = [art.whatItIs, art.whatHappens, art.tip, ...(art.howTo || []), ...(art.settings || [])].filter(Boolean);
        const named = strings.flatMap((x) => [...x.matchAll(re)].map((m) => m[1].trim()));
        if (named.length && !named.includes(secLabel(lang, art.section)))
          problems.push(`${lang}[${i}] "${art.title}": sits under "${secLabel(lang, art.section)}" but its expert-menu paths name only ${[...new Set(named)].map((n) => `"${n}"`).join(", ")}`);
      });
    }
    // docs/help.md mirrors the areas: EN "##" headings, then the DE "###" area headings.
    const md = read("docs/help.md").replace(/\r\n/g, "\n").split("\n");
    const deAt = md.findIndex((l) => l === "## Hilfe (Deutsch)");
    if (deAt < 0) problems.push('docs/help.md has no "## Hilfe (Deutsch)" heading');
    else {
      const enHeads = md.slice(1, deAt).filter((l) => /^## /.test(l)).map((l) => l.slice(3));
      const deHeads = md.slice(deAt + 1).filter((l) => /^### /.test(l)).map((l) => l.slice(4));
      const wantEn = SECTIONS.map((id) => secLabel("en", id));
      const wantDe = SECTIONS.map((id) => secLabel("de", id));
      if (JSON.stringify(enHeads) !== JSON.stringify(wantEn)) problems.push(`docs/help.md EN area headings [${enHeads.join(" | ")}] differ from the tab labels [${wantEn.join(" | ")}]`);
      if (JSON.stringify(deHeads) !== JSON.stringify(wantDe)) problems.push(`docs/help.md DE area headings [${deHeads.join(" | ")}] differ from the tab labels [${wantDe.join(" | ")}]`);
    }
  }
  for (const p of problems) console.log("FAIL:", p);
  console.log(`${quotes} quoted labels checked, ${checked} paths checked, ${problems.length} problem(s)`);
  process.exit(problems.length ? 1 : 0);
})();
