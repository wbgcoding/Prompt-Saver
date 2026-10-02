// Fork vocabulary check.
//
// The shipped texts name the unit this app stores ("prompt", and its forms in each
// language). A brand that calls it something else supplies a per-language table of
// those forms in brands/<slug>.json → terms.forms.
//
// Contract, and what this enforces:
//   * a language listed in terms.forms must cover EVERY form that appears in that
//     language's strings: a half-translated language would ship a mixed sentence;
//   * a form that must stay as it is (the "promt/prompt" typo example) is mapped to
//     itself, so leaving it alone is a decision, not an oversight;
//   * languages not listed are reported as still to do, and do not fail the run.
//
// Run:  node tests/terms_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const REPO = path.resolve(__dirname, "..");
const UI = path.join(REPO, "ui");

// How the unit is written in each language TODAY. Not derived: Polish and Romanian
// inflect it, Turkish agglutinates, and Chinese has a near-miss word (提示 = "hint")
// that must not be caught.
const FAMILY = {
  en: /\bprompts?\b/gi,
  de: /\bprompts?\b/gi,
  es: /\bprompts?\b/gi,
  it: /\bprompts?\b/gi,
  pt: /\bprompts?\b/gi,
  nl: /\bprompts?\b/gi,
  pl: /\bprompt\w*\b/gi,
  ro: /\bprompt\w*\b/gi,
  tr: /\bistem(?:ler)?(?:i|e|le|iyle|leri)?\b/gi,
  zh: /提示词/g,
  ja: /プロンプト/g,
};

const { I18N, FLOAT_TXT: FLOAT } = require("./i18n_load.js");

// 63: a stray English loanword ("Prompt"/"Prompts") can leak into ANY opted-in
// language regardless of its own native word family -- checked in ADDITION to
// FAMILY, never instead of it. For en/de/es/it/pt/nl this is the same pattern
// FAMILY already uses (harmless no-op); for pl/ro it is a subset of their own
// broader stem match (also harmless); for tr/zh/ja it is genuinely new coverage:
// tr's own family (istem-derived forms) and zh/ja's native-script patterns cannot
// themselves match Latin "prompt" text, which is exactly how 62 went undetected.
const LOANWORD_LEAK = /\bprompts?\b/gi;

// tipFuzzySearch's whole point is the English "promt"/"prompt" typo pair as a
// literal example: the one deliberate exception, not a translation gap. A
// self-map entry ("prompt": "prompt") would "explain" it, but formsIn() below
// only tracks distinct SURFACE FORMS across a whole language, not per string,
// so a self-map for that one string would just as well hide any unrelated,
// genuine "prompt" leak elsewhere in the same language. Skip the exempt key
// instead, so the loanword scan still catches everything else.
const LOANWORD_EXEMPT_KEYS = new Set(["tipFuzzySearch"]);

// Drop the exempt keys once, up front: every scan below, FAMILY or
// LOANWORD_LEAK alike, must skip them the same way, or a language whose own
// FAMILY pattern happens to equal LOANWORD_LEAK (en/de/es/it/pt/nl, and a
// superset for pl/ro) picks tipFuzzySearch's "prompt" straight back up.
const withoutExempt = (obj) =>
  Object.fromEntries(Object.entries(obj || {}).filter(([k]) => !LOANWORD_EXEMPT_KEYS.has(k)));

// Every distinct surface form a language's visible strings contain.
function formsIn(lang) {
  const found = new Set();
  const scan = (obj, re) => {
    for (const v of Object.values(obj || {})) {
      for (const m of String(v).matchAll(re)) found.add(m[0]);
    }
  };
  const i18nLang = withoutExempt(I18N[lang]);
  const floatLang = withoutExempt(FLOAT[lang]);
  if (FAMILY[lang]) { scan(i18nLang, FAMILY[lang]); scan(floatLang, FAMILY[lang]); }
  scan(i18nLang, LOANWORD_LEAK);
  scan(floatLang, LOANWORD_LEAK);
  return found;
}

const problems = [];
const notes = [];
const brands = fs.readdirSync(path.join(REPO, "brands")).filter((f) => f.endsWith(".json"));

for (const file of brands) {
  const slug = file.replace(/\.json$/, "");
  const b = JSON.parse(fs.readFileSync(path.join(REPO, "brands", file), "utf8"));
  if (!b.terms) continue; // this brand's own word is the one the texts already use
  if (!b.terms.item || !b.terms.itemPlural) {
    problems.push(`${slug}: terms needs both item and itemPlural`);
  }
  const forms = b.terms.forms || {};
  const todo = [];
  for (const lang of Object.keys(FAMILY)) {
    const found = formsIn(lang);
    if (!found.size) continue;
    if (!forms[lang]) { todo.push(`${lang} (${found.size} form(s))`); continue; }
    const missing = [...found].filter((f) => !(f in forms[lang]));
    for (const m of missing) problems.push(`${slug}: ${lang} leaves "${m}" untranslated`);
    // A multi-word key is a context override ("eines Prompts" → genitive), matched
    // before the bare form. It is meant not to appear on its own.
    const spare = Object.keys(forms[lang])
      .filter((f) => !f.includes(" ") && !found.has(f));
    for (const sp of spare) notes.push(`${slug}: ${lang} maps "${sp}", which no string contains`);
  }
  if (todo.length) notes.push(`${slug}: still to translate — ${todo.join(", ")}`);
}

for (const n of notes) console.log("note:", n);
for (const p of problems) console.log("FAIL", p);
console.log(problems.length ? `${problems.length} problem(s)` : "PASS");
process.exit(problems.length ? 1 : 0);
