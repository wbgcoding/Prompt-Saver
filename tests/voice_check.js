// Voice command post-processing checks (ui/voice.js).
// Run:  node tests/voice_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { applyVoiceCommands, VOICE_PUNCT, VOICE_FUNCS, VOICE_LITERAL_WORD } =
  require(path.join(__dirname, "..", "ui", "voice.js"));

const problems = [];
const check = (label, actual, expected) => {
  if (actual !== expected) problems.push(`${label}: got ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`);
};

const on = (text, extra) => applyVoiceCommands(text, { punct: true, autoCaps: true, lang: "de", ...extra });

// Punctuation sticks to the previous word, no space before it.
check("punct sticks", on("Test Komma und weiter"), "Test, und weiter");

// Multi-word commands win over a single-word reading.
check("multi-word wins", on("neuer Absatz beginnt"), "\n\nBeginnt"); // auto-caps after a paragraph break too
check("multi-word wins (klammer)", on("Klammer auf Text Klammer zu"), "(Text)");

// Capitalization only right after a mark WE just inserted.
check("caps after sentence end", on("Das ist gut Punkt weiter geht es"), "Das ist gut. Weiter geht es");
check("no caps mid-sentence", on("eins zwei drei"), "eins zwei drei");

// Escape word: the next word is emitted literally, not command-matched.
check("literal escape word", on("sag wörtlich Punkt jetzt"), "sag Punkt jetzt");

// Disabled -> text passes through completely unchanged.
check("disabled leaves text alone", applyVoiceCommands("Test Komma weiter.", { punct: false, lang: "de" }), "Test Komma weiter.");

// Mixed already-punctuated text: existing marks are never touched, only bare
// command words are still replaced.
check("mixed text", on("Das ist ein Test. Weiter Komma dann Schluss"), "Das ist ein Test. Weiter, dann Schluss");

// Double application must be safe: running an already-processed (already
// punctuated, no leftover command words) string through again changes nothing.
const once = on("Test Komma weiter Punkt");
check("double application is a no-op", on(once), once);

// English table sanity (hand-written, same shape as German).
check("english punctuation", applyVoiceCommands("Test comma more period", { punct: true, autoCaps: true, lang: "en" }), "Test, more.");

// A trailing escape word with nothing after it (the recognizer's segment
// boundary landed right there) must not silently vanish -- it is emitted as
// ordinary text instead of being eaten with no output at all.
check("trailing literal word is not swallowed", on("sag jetzt wörtlich"), "sag jetzt wörtlich");

// CJK carries no spaces between words -- command phrases are matched by
// scanning raw characters, not by splitting on whitespace (which would hand
// the whole sentence back as a single unmatched token).
const zh = (text) => applyVoiceCommands(text, { punct: true, autoCaps: true, lang: "zh" });
check("zh command matches without spaces", zh("你好句号世界"), "你好.世界");
check("zh literal escape word", zh("说字面量句号完"), "说句号完");
check("zh trailing literal word is not swallowed", zh("说完字面量"), "说完字面量");

// "point" is a deliberate English synonym for "period" (documented in
// WORK.md) -- ordinary sentences that happen to contain the word "point" as
// plain English are a known, accepted trade-off of the feature (the same one
// every spoken-punctuation dictation tool has), not a bug; the literal escape
// word is the documented way around it. Asserted here so a future change to
// this trade-off is a deliberate edit to this test, not a silent regression.
check("english 'point' is a punctuation command, not just a word", applyVoiceCommands("the point is clear", { punct: true, autoCaps: false, lang: "en" }), "the. is clear");

// Per-language command-key parity: every language must define the same
// concepts (punctuation + function words + the literal escape word) as DE --
// same parity discipline as tests/i18n_check.js, just for this file's own
// recognition tables instead of ui/i18n.js.
// Compared by VALUE set, not key count -- a language may use a synonym or
// two (English has both "period" and "point"), it just has to cover every
// mark/function DE covers.
const LANGS = ["en", "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro"];
const dePunctValues = new Set(Object.values(VOICE_PUNCT.de));
const deFuncValues = new Set(Object.values(VOICE_FUNCS.de));
for (const lang of LANGS) {
  if (!VOICE_PUNCT[lang]) { problems.push(`${lang}: missing from VOICE_PUNCT`); continue; }
  if (!VOICE_FUNCS[lang]) { problems.push(`${lang}: missing from VOICE_FUNCS`); continue; }
  if (!VOICE_LITERAL_WORD[lang]) problems.push(`${lang}: missing from VOICE_LITERAL_WORD`);
  const pv = new Set(Object.values(VOICE_PUNCT[lang]));
  for (const v of dePunctValues) if (!pv.has(v)) problems.push(`${lang}: VOICE_PUNCT has no word for '${v}'`);
  const fv = new Set(Object.values(VOICE_FUNCS[lang]));
  for (const v of deFuncValues) if (!fv.has(v)) problems.push(`${lang}: VOICE_FUNCS has no word for the '${JSON.stringify(v)}' function`);
}

if (problems.length) {
  console.log(`voice_check: ${problems.length} problem(s)`);
  for (const p of problems) console.log(" -", p);
  process.exit(1);
}
console.log("voice_check: clean");
