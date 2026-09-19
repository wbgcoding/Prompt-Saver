// Chain step variable-need check (the chain overview strip in main.js).
//
// Loaded as a plain script (like search.js and voice.js), so everything here is
// a bare function/const in module scope. The guarded export at the bottom makes
// it directly testable from Node without affecting the window build.
//
// Mirrors the token syntax and reserved-name notation main.js itself uses for
// prompt variables (VAR_RE, DYN_VARS, DYN_SET), but stays free of flag() and
// every other piece of app state, so it can be handed a plain list of step
// texts and always give the same answer back.
const CHAIN_VAR_RE = /\{#\{\s*([^}\n]{1,200}?)\s*\}#\}/g;
const CHAIN_DYN_VARS = [
  "date", "time", "datetime", "weekday", "yesterday", "tomorrow",
  "week", "month", "year", "timestamp", "epoch",
  "clipboard", "username", "uuid", "random", "counter",
];
const CHAIN_DYN_SET = new Set(CHAIN_DYN_VARS);

// Same split as parseVar() in main.js: the text up to the first "|" is the
// name/default part, and an "=" inside that splits a default off the name.
function chainVarName(token) {
  const raw = String(token).trim();
  const bar = raw.indexOf("|");
  const namePart = bar < 0 ? raw : raw.slice(0, bar);
  const eq = namePart.indexOf("=");
  const name = (eq < 0 ? namePart : namePart.slice(0, eq)).trim();
  return name || raw;
}

// A reserved name is written with the slash fence ("/date\") and listed in
// CHAIN_DYN_SET -- it fills itself in, no earlier step required.
function chainIsReserved(name) {
  return name.length > 2 && name.startsWith("/") && name.endsWith("\\") && CHAIN_DYN_SET.has(name.slice(1, -1));
}
// The bare reserved name behind a fence ("/date\" -> "date"), for display and
// for the `fills` list -- callers already know it is reserved when they use this.
const chainDynName = (name) => name.slice(1, -1);

// The distinct placeholder names a step's own text carries, in first-seen order.
function chainStepNames(text) {
  const names = [];
  for (const m of String(text || "").matchAll(CHAIN_VAR_RE)) {
    const name = chainVarName(m[1]);
    if (name && !names.includes(name)) names.push(name);
  }
  return names;
}

// The one pure check the overview strip renders: for each step (its raw prompt
// text, in pick order), which placeholder names it needs, which it fills by
// itself (a reserved name), and which of its needs nothing earlier in the
// chain answers -- a name only counts as answered by a STRICTLY earlier step,
// never by one that comes later.
function chainVarReport(steps) {
  const seenEarlier = new Set();
  const report = [];
  for (const text of steps || []) {
    const names = chainStepNames(text);
    const needs = names.filter((n) => !chainIsReserved(n));
    const fills = names.filter((n) => chainIsReserved(n)).map(chainDynName);
    const missing = needs.filter((n) => !seenEarlier.has(n));
    report.push({ needs, fills, missing });
    for (const n of names) seenEarlier.add(n);
  }
  return report;
}

if (typeof module !== "undefined") {
  module.exports = { chainVarReport, chainStepNames, chainVarName, chainIsReserved, chainDynName };
}
