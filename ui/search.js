// Library search query parsing (computeLibSearchScore in library.js).
// Pulls "tag:word" / "var:word" terms (and their "-" negated forms) out of a
// raw query before the rest of it runs through the existing scoring, so they
// can be checked as hard conditions instead of scored hints.
//
// Loaded as a plain script (like voice.js), so everything here is a bare
// function/const in module scope. The guarded export at the bottom makes it
// directly testable from Node without affecting the window build.

const SEARCH_PREFIXES = ["tag", "var"];

// Splits a lowercased, trimmed query into the part that still runs through
// the normal scoring ("rest") and the tag:/var: hard conditions pulled out of
// it. Anything else -- including a plain "-word" exclusion -- is left in
// "rest" untouched, exactly as it was before these two prefixes existed.
function parseSearchQuery(raw) {
  const terms = String(raw || "").split(/\s+/).filter(Boolean);
  const rest = [];
  const conditions = [];
  for (const term of terms) {
    const neg = term.length > 1 && term[0] === "-";
    const body = neg ? term.slice(1) : term;
    const colon = body.indexOf(":");
    const prefix = colon > 0 ? body.slice(0, colon).toLowerCase() : "";
    const value = colon > 0 ? body.slice(colon + 1).trim() : "";
    if (SEARCH_PREFIXES.includes(prefix) && value) {
      conditions.push({ prefix, value: value.toLowerCase(), neg });
    } else {
      rest.push(term);
    }
  }
  return { rest: rest.join(" "), conditions };
}

if (typeof module !== "undefined") module.exports = { parseSearchQuery, SEARCH_PREFIXES };
