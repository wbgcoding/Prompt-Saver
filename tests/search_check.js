// Library search query parsing checks (ui/search.js).
// Run:  node tests/search_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { parseSearchQuery } = require(path.join(__dirname, "..", "ui", "search.js"));

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

// A lone tag: condition consumes the whole query, nothing left to score.
check("tag only", parseSearchQuery("tag:Work"),
  { rest: "", conditions: [{ prefix: "tag", value: "work", neg: false }] });

// A lone var: condition, same shape.
check("var only", parseSearchQuery("var:Kunde"),
  { rest: "", conditions: [{ prefix: "var", value: "kunde", neg: false }] });

// Both prefixes combine into two conditions that must both hold.
check("tag and var combined", parseSearchQuery("tag:Work var:Kunde"),
  { rest: "", conditions: [{ prefix: "tag", value: "work", neg: false }, { prefix: "var", value: "kunde", neg: false }] });

// A leading "-" negates the condition instead of excluding a plain word.
check("negated tag", parseSearchQuery("-tag:Draft"),
  { rest: "", conditions: [{ prefix: "tag", value: "draft", neg: true }] });
check("negated var", parseSearchQuery("-var:Kunde"),
  { rest: "", conditions: [{ prefix: "var", value: "kunde", neg: true }] });

// An extra plain word stays in "rest" for the normal scoring pass.
check("condition plus a plain word", parseSearchQuery("tag:Work hello"),
  { rest: "hello", conditions: [{ prefix: "tag", value: "work", neg: false }] });

// A plain "-word" exclusion next to a condition is left completely alone --
// that behaviour belongs to the existing scoring code, not this parser.
check("condition plus an existing exclusion", parseSearchQuery("tag:Work -exclude"),
  { rest: "-exclude", conditions: [{ prefix: "tag", value: "work", neg: false }] });

// A prefix with nothing after the colon is not a condition at all -- the
// whole term falls through to "rest" unchanged, same as any other word.
check("empty value falls through", parseSearchQuery("tag:"),
  { rest: "tag:", conditions: [] });

if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log("PASS");
}
