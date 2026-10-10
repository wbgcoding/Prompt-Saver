// Chain overview variable-check checks (ui/chainvars.js: chainVarReport).
// Run:  node tests/chainvars_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { chainVarReport } = require(path.join(__dirname, "..", "ui", "chainvars.js"));

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

// Everything fulfilled: no step ever asks for a placeholder nothing can
// answer -- one step with nothing at all, two steps that only use reserved
// (self-filling) names.
check(
  "everything fulfilled",
  chainVarReport([
    "Plain text, no placeholders here.",
    "Timestamp it: {?@date}.",
    "And an id: {?@uuid}.",
  ]),
  [
    { needs: [], fills: [], missing: [] },
    { needs: [], fills: ["date"], missing: [] },
    { needs: [], fills: ["uuid"], missing: [] },
  ],
);

// One gap: the middle step asks for a name nothing before it carries.
check(
  "one gap",
  chainVarReport([
    "Intro, no placeholders.",
    "Needs {?name}.",
    "Closing, no placeholders.",
  ]),
  [
    { needs: [], fills: [], missing: [] },
    { needs: ["name"], fills: [], missing: ["name"] },
    { needs: [], fills: [], missing: [] },
  ],
);

// Several gaps: one step needs two different names, neither answered earlier.
check(
  "several gaps",
  chainVarReport(["Uses {?a} and {?b}."]),
  [{ needs: ["a", "b"], fills: [], missing: ["a", "b"] }],
);

// Reserved name: a step using only a self-filling name is never marked
// missing, no matter where it sits in the chain.
check(
  "reserved name",
  chainVarReport(["Needs {?topic}.", "Stamped {?@timestamp}."]),
  [
    { needs: ["topic"], fills: [], missing: ["topic"] },
    { needs: [], fills: ["timestamp"], missing: [] },
  ],
);

// A placeholder fulfilled only later must not un-flag the earlier step: the
// first step still shows the gap even though a later step uses the same name.
check(
  "placeholder fulfilled only later",
  chainVarReport([
    "Needs {?topic} now.",
    "This later step also mentions {?topic}.",
  ]),
  [
    { needs: ["topic"], fills: [], missing: ["topic"] },
    { needs: ["topic"], fills: [], missing: [] },
  ],
);

// Empty chain: nothing picked yet, nothing to report.
check("empty chain", chainVarReport([]), []);

if (problems.length) {
  console.error(`chainvars_check: ${problems.length} problem(s):`);
  for (const p of problems) console.error(" - " + p);
  process.exit(1);
} else {
  console.log("chainvars_check: all clean");
}
