// Remaining-time estimator checks (ui/floatShared.js: makeEtaEstimator, formatEta).
// Run:  node tests/eta_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
// floatShared.js also wires a document-level popover listener at module scope
// (for its shared numberField control) -- irrelevant here, so a bare stub lets
// this pure-function test load the module outside a browser.
if (typeof document === "undefined") global.document = { addEventListener() {} };
const { makeEtaEstimator, formatEta } = require(path.join(__dirname, "..", "ui", "floatShared.js"));

const problems = [];
const check = (label, actual, expected) => {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

// Too few points -- no rate to measure yet.
{
  const eta = makeEtaEstimator();
  eta.feed(5, 0);
  check("a single sample gives no estimate", eta.estimate(10, 20), null);
}

// Below minPct: even a fast, well-measured rate must not show yet.
{
  const eta = makeEtaEstimator();
  eta.feed(1, 0);
  eta.feed(5, 1000);
  check("below minPct stays hidden", eta.estimate(10, 20), null);
}

// A pause (no progress between two ticks) must not corrupt the average -- the
// zero-progress interval is simply skipped, not counted as a zero rate.
{
  const eta = makeEtaEstimator();
  eta.feed(10, 0);
  eta.feed(10, 5000); // stalled for 5s: dPct 0, contributes no rate sample
  eta.feed(20, 10000); // then 10% over 5s: rate 2%/s
  // remaining = (100-20)/2 = 40s -- above minSecs(20), so it must show, and
  // 40s is short enough for the seconds wording (rounded to the nearest 5).
  check("a mid-run pause is ignored, not averaged in", eta.estimate(10, 20), { key: "etaSeconds", value: 40 });
}

// Above minPct but not enough time left to be worth a number.
{
  const eta = makeEtaEstimator();
  eta.feed(0, 0);
  eta.feed(10, 1000); // rate 10%/s -> remaining = 9s, under minSecs(20)
  check("a remaining time under minSecs stays hidden", eta.estimate(10, 20), null);
}

// Two minutes to under an hour: minutes wording, never a seconds count.
{
  const eta = makeEtaEstimator();
  eta.feed(50, 0);
  eta.feed(51, 10000); // rate 0.1%/s -> remaining = 490s -> ~8 minutes
  check("minutes-range remaining rounds to the nearest minute", eta.estimate(10, 20), { key: "etaMinutes", value: 8 });
}

// An hour or more: the fixed "over an hour" wording, no further precision.
{
  const eta = makeEtaEstimator();
  eta.feed(50, 0);
  eta.feed(50.5, 60000); // rate ~0.0083%/s -> remaining ~5940s, well over an hour
  check("very slow progress reports over an hour", eta.estimate(10, 20), { key: "etaOverHour" });
}

// reset() drops every prior sample, same as a fresh estimator.
{
  const eta = makeEtaEstimator();
  eta.feed(50, 0);
  eta.feed(90, 1000);
  eta.reset();
  eta.feed(5, 0);
  check("reset() forgets prior samples", eta.estimate(10, 20), null);
}

// formatEta composes the localized string from an estimate() result, using
// the caller's own t() lookup -- it never talks to i18n directly.
{
  const T = {
    etaAbout: "about {time} left",
    etaOverHour: "more than an hour left",
    etaSeconds: "{n} seconds",
    etaMinutes: "{n} minutes",
  };
  const t = (k) => T[k];
  check("formatEta shows nothing for a null estimate", formatEta(null, t), "");
  check("formatEta composes the seconds wording", formatEta({ key: "etaSeconds", value: 40 }, t), "about 40 seconds left");
  check("formatEta composes the minutes wording", formatEta({ key: "etaMinutes", value: 8 }, t), "about 8 minutes left");
  check("formatEta uses the fixed over-an-hour wording as-is", formatEta({ key: "etaOverHour" }, t), "more than an hour left");
}

if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
} else {
  console.log("PASS");
}
