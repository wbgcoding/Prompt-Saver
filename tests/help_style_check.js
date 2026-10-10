// Help sentences stay short: every how-to step is one sentence of at most 160 characters,
// every sentence of the prose fields (whatItIs, whatHappens, tip) is at most 220
// characters, and no parenthesis sits inside another. "settings" lines are path lists,
// not sentences, so only the parenthesis rule applies to them.
// Run:  node tests/help_style_check.js      Exit: 0 = clean, 1 = hits.
const fs = require("fs");
const path = require("path");
const os = require("os");
const { pathToFileURL } = require("url");

const STEP_MAX = 160;
const SENTENCE_MAX = 220;

// Abbreviations whose dot does not end a sentence, and {icon:...} tokens, are masked
// (same length) before the split, so offsets still match the original text.
const ABBREV = /\b(?:bzw|z\. ?B|d\. ?h|ca|usw|etc|e\.g|i\.e|vs)\./g;
const mask = (s) => s
  .replace(ABBREV, (m) => m.replace(/\./g, "\u0001"))
  .replace(/\{icon:[^}]*\}/g, (m) => "x".repeat(m.length));

// A sentence ends at . ! or ? followed by whitespace and an upper-case letter, a digit
// or an opening quote.
function sentences(text) {
  const m = mask(text);
  const out = [];
  let from = 0;
  const re = /[.!?]\s+(?=[\p{Lu}0-9„"“(])/gu;
  let hit;
  while ((hit = re.exec(m))) {
    const end = hit.index + 1;
    out.push(text.slice(from, end));
    from = hit.index + hit[0].length;
  }
  out.push(text.slice(from));
  return out.filter((x) => x.trim());
}

function nestedParens(text) {
  let depth = 0;
  for (const ch of text) {
    if (ch === "(") { depth++; if (depth > 1) return true; }
    else if (ch === ")" && depth > 0) depth--;
  }
  return false;
}

(async () => {
  const tmp = path.join(os.tmpdir(), `help-style-${process.pid}.mjs`);
  fs.writeFileSync(tmp, fs.readFileSync(path.join(__dirname, "..", "ui", "help.js"), "utf8"));
  let HELP;
  try { HELP = (await import(pathToFileURL(tmp).href)).HELP_CONTENT; } finally { fs.unlinkSync(tmp); }

  const hits = [];
  let steps = 0, prose = 0;
  const hit = (lang, art, field, len, text) =>
    hits.push(`FAIL: ${lang} "${art.title}" ${field}: ${len} ${text.slice(0, 60)}`);
  for (const lang of ["en", "de"]) {
    for (const art of HELP[lang]) {
      (art.howTo || []).forEach((step, i) => {
        steps++;
        const n = sentences(step).length;
        if (n > 1) hit(lang, art, `howTo[${i}] (${n} sentences)`, step.length, step);
        else if (step.length > STEP_MAX) hit(lang, art, `howTo[${i}]`, step.length, step);
      });
      for (const field of ["whatItIs", "whatHappens", "tip"]) {
        if (!art[field]) continue;
        for (const s of sentences(art[field])) {
          prose++;
          if (s.length > SENTENCE_MAX) hit(lang, art, field, s.length, s);
        }
      }
      const all = [art.whatItIs, art.whatHappens, art.tip, ...(art.howTo || []), ...(art.settings || [])];
      for (const s of all.filter(Boolean))
        if (nestedParens(s)) hit(lang, art, "nested parentheses", s.length, s);
    }
  }
  for (const h of hits) console.log(h);
  console.log(`${steps} steps and ${prose} prose sentences checked, ${hits.length} hit(s)`);
  process.exit(hits.length ? 1 : 0);
})();
