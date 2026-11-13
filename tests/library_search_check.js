// Library search scoring (ui/library.js): computeLibSearchScore end to end (tiers, field weights,
// the entry a rotating prompt matched, "-word" exclusions, tag: / var: conditions, the typo-tolerant
// tier, the recognized-picture-text last resort, the per-query and per-prompt caches that must
// never serve a stale answer), plus fuzzyMaxTyposFor, editDistWithin, fuzzyTokens, fuzzyWordHit.
// The real source spans of library.js run in a node vm next to the real promptVars.js and
// search.js; only flags, the query and the image-hit lists are fakes.
// Run:  node tests/library_search_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "ui");
const problems = [];
let count = 0;
const check = (label, actual, expected) => {
  count++;
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};
const read = (f) => fs.readFileSync(path.join(uiDir, f), "utf8").replace(/\r\n/g, "\n");
const span = (src, from, to) => {
  const i = src.indexOf(from);
  const j = src.indexOf(to, i);
  if (i < 0 || j < 0) throw new Error(`span not found: ${from} .. ${to}`);
  return src.slice(i, j);
};

async function main() {
  globalThis.document = globalThis.document || { addEventListener() {}, removeEventListener() {} };
  const shared = await import(pathToFileURL(path.join(uiDir, "floatShared.js")).href);
  const { parseSearchQuery } = require(path.join(uiDir, "search.js"));

  const lib = read("library.js");
  const libCode = span(lib, "export function fuzzyMaxTyposFor", "// Fills `el` with `str`")
    + "\n" + span(lib, "// Split into alphanumeric tokens", "// The archive is a reserved tag");
  const tagKeyLine = /export const tagKey = .*;/.exec(read("tags.js"))[0];
  const pv = read("promptVars.js").replace(/^import .*$/gm, "");

  const sb = {
    FLAGS: { fuzzySearch: false, tags: true, promptNotes: true, promptVars: true, choiceVars: true, dynVars: true },
    OPT: { ocrIndex: false },
    flag: (k) => !!sb.FLAGS[k],
    optFlag: (k) => !!sb.OPT[k],
    tagsOn: () => !!sb.FLAGS.tags,
    notesOn: () => !!sb.FLAGS.promptNotes,
    libQuery: "",
    imageHitIds: new Set(),
    imageHitQuery: "",
    parseSearchQuery,
    // promptVars.js dependencies
    invoke: () => Promise.resolve(), LANG: "en", settings: {}, t: (k) => k, toast() {}, TOOLTIP_PREVIEW_CHARS: 20,
    txt: () => "", activeThemeVars: () => ({}), varPreview: shared.varPreview, VAR_RE: shared.VAR_RE,
    crypto: globalThis.crypto, Intl,
  };
  vm.createContext(sb);
  vm.runInContext(
    pv.replace(/^export /gm, "") + "\n" + tagKeyLine.replace(/^export /, "") + "\n" + libCode.replace(/^export /gm, "")
    + "\nthis.api = { fuzzyMaxTyposFor, editDistWithin, fuzzyTokens, fuzzyWordHit, libSearchScore, searchConditionMet, computeLibSearchScore, tagKey, libScoreCache };",
    sb
  );
  const api = sb.api;

  const setFlags = (o) => { sb.FLAGS = Object.assign({ fuzzySearch: false, tags: true, promptNotes: true, promptVars: true, choiceVars: true, dynVars: true }, o); };
  const score = (p, q, flags, imageHits) => {
    setFlags(flags || {});
    sb.libQuery = q;
    return api.computeLibSearchScore(p, imageHits);
  };
  const none = { score: 0, fuzzy: false, entryIndex: 0 };
  const all = { score: 1, fuzzy: false, entryIndex: 0 };
  let nextId = 1;
  const mk = (o) => Object.assign({ id: nextId++, name: "", text: "", tags: [], variants: [] }, o);

  // ---- the small helpers
  check("typos allowed: empty", api.fuzzyMaxTyposFor(""), 1);
  check("typos allowed: five letters", api.fuzzyMaxTyposFor("abcde"), 1);
  check("typos allowed: six letters", api.fuzzyMaxTyposFor("abcdef"), 2);
  check("typos allowed: long", api.fuzzyMaxTyposFor("a".repeat(30)), 2);
  check("distance of equal words", api.editDistWithin("same", "same", 2), 0);
  check("distance of the classic pair", api.editDistWithin("kitten", "sitting", 3), 3);
  check("distance above the bound reports bound + 1", api.editDistWithin("kitten", "sitting", 2), 3);
  check("distance with a length gap above the bound", api.editDistWithin("a", "abcd", 2), 3);
  check("distance of one insertion", api.editDistWithin("abc", "abdc", 1), 1);
  check("distance of one substitution", api.editDistWithin("abc", "abd", 1), 1);
  check("a swap of two letters costs two", api.editDistWithin("ab", "ba", 2), 2);
  check("a swap is out of reach at one", api.editDistWithin("ab", "ba", 1), 2);
  check("distance from the empty word", api.editDistWithin("", "abc", 3), 3);
  check("distance between two empty words", api.editDistWithin("", "", 1), 0);
  check("distance is symmetric", api.editDistWithin("flaw", "lawn", 2), api.editDistWithin("lawn", "flaw", 2));
  check("tokens split on punctuation", api.fuzzyTokens("code, code. foo_bar"), ["code", "code", "foo", "bar"]);
  check("tokens keep accented letters", api.fuzzyTokens("na\u00efve caf\u00e9"), ["na\u00efve", "caf\u00e9"]);
  check("tokens keep digits", api.fuzzyTokens("v2.10 beta3"), ["v2", "10", "beta3"]);
  check("a script without spaces stays one token", api.fuzzyTokens("\u65e5\u672c\u8a9e\u30c6\u30b9\u30c8"), ["\u65e5\u672c\u8a9e\u30c6\u30b9\u30c8"]);
  check("tokens of nothing", api.fuzzyTokens(""), []);
  check("tokens of punctuation only", api.fuzzyTokens("!? ..."), []);
  check("word hit: substring", api.fuzzyWordHit("ode", "code", 1), true);
  check("word hit: one typo", api.fuzzyWordHit("cde", "code", 1), true);
  check("word hit: typo against a long word's start", api.fuzzyWordHit("cde", "codebase", 1), true);
  check("word hit: nothing alike", api.fuzzyWordHit("xyz", "codebase", 1), false);
  check("word hit: two typos need the longer allowance", [api.fuzzyWordHit("progrming", "programming", 1), api.fuzzyWordHit("progrming", "programming", 2)], [false, true]);
  check("word hit: a longer query than the word", api.fuzzyWordHit("codebase", "code", 1), false);

  // ---- no query
  const base = mk({ name: "Foo bar", text: "hello world" });
  check("an empty query matches everything", score(base, ""), all);
  check("a blank query matches everything", score(base, "   "), all);

  // ---- tiers and field weights
  check("name prefix", score(base, "foo"), { score: 360, fuzzy: false, entryIndex: 0, field: "name", q: "foo" });
  check("name inside", score(base, "bar"), { score: 300, fuzzy: false, entryIndex: 0, field: "name", q: "bar" });
  check("text prefix", score(base, "hello"), { score: 120, fuzzy: false, entryIndex: 0, field: "text", q: "hello" });
  check("text inside", score(base, "world"), { score: 100, fuzzy: false, entryIndex: 0, field: "text", q: "world" });
  check("a phrase across a space", score(base, "o w"), { score: 100, fuzzy: false, entryIndex: 0, field: "text", q: "o w" });
  check("no match without the typo tier", score(base, "zzz"), none);
  check("the query is case-folded", score(base, "  FOO  "), { score: 360, fuzzy: false, entryIndex: 0, field: "name", q: "foo" });
  check("a text in capitals is found by lower case", score(mk({ name: "x", text: "SHOUT" }), "shout").score, 120);
  check("non-ascii letters are folded", score(mk({ name: "\u00c4rger", text: "" }), "\u00e4rg").score, 360);
  check("the name outranks a better-placed body hit", score(mk({ name: "foo bar", text: "bar is here" }), "bar").score, 300);
  check("the body prefix beats a body hit inside", score(mk({ name: "n", text: "bar and bar" }), "bar").score, 120);
  check("the field with the higher weight wins even inside", score(mk({ name: "n1", text: "zz", tags: ["alpha"] }), "alp").score, 240);
  const tagged = mk({ name: "n", text: "t", tags: ["Work", "Home"] });
  check("a tag prefix ranks between name and body", score(tagged, "work"), { score: 240, fuzzy: false, entryIndex: 0, field: "tags", q: "work" });
  check("a tag inside", score(tagged, "ome").score, 200);
  check("tags are not searched when tags are off", score(tagged, "work", { tags: false }), none);
  const noted = mk({ name: "n", text: "t", notes: "Remember milk" });
  check("a note prefix ranks lowest", score(noted, "remember"), { score: 60, fuzzy: false, entryIndex: 0, field: "notes", q: "remember" });
  check("a note inside", score(noted, "milk").score, 50);
  check("notes are not searched when notes are off", score(noted, "milk", { promptNotes: false }), none);
  const filed = mk({ name: "n", text: "t", file_path: "C:\\Docs\\Report.pdf" });
  check("a file path prefix", score(filed, "c:\\docs"), { score: 120, fuzzy: false, entryIndex: 0, field: "file", q: "c:\\docs" });
  check("a file path inside", score(filed, "report").score, 100);
  const rotating = mk({ name: "n", text: "alpha", variants: ["beta", "gamma delta"] });
  check("the base text is entry zero", score(rotating, "alpha"), { score: 120, fuzzy: false, entryIndex: 0, field: "text", q: "alpha" });
  check("the first variant is entry one", score(rotating, "beta"), { score: 120, fuzzy: false, entryIndex: 1, field: "variant", q: "beta" });
  check("the second variant is entry two", score(rotating, "delta"), { score: 100, fuzzy: false, entryIndex: 2, field: "variant", q: "delta" });
  check("an equal score keeps the first entry", score(mk({ name: "n", text: "xy1", variants: ["xy2"] }), "xy").entryIndex, 0);
  check("a prompt with no variants key still scores", score({ id: 900, name: "foo" }, "foo").score, 360);
  check("a prompt with every field missing is no match", score({ id: 901 }, "foo"), none);

  // ---- exclusions
  const hw = mk({ name: "Foo", text: "hello world" });
  check("an excluded word removes the prompt", score(hw, "hello -world"), none);
  check("another excluded word leaves the match", score(hw, "hello -xyz").score, 120);
  check("an exclusion-only query keeps what lacks the word", score(mk({ text: "hi" }), "-world"), all);
  check("an exclusion-only query drops what has the word", score(hw, "-world"), none);
  check("an exclusion looks in the name too", score(hw, "hello -foo"), none);
  const withNote = mk({ name: "n", text: "hello", notes: "world" });
  check("an exclusion looks in notes while notes are on", score(withNote, "hello -world"), none);
  check("an exclusion ignores notes while notes are off", score(withNote, "hello -world", { promptNotes: false }).score, 120);
  check("an exclusion looks in the variants", score(mk({ name: "n", text: "hello", variants: ["has world"] }), "hello -world"), none);
  check("several exclusions all apply", score(hw, "hello -xyz -world"), none);
  check("a lone dash is a plain word", score(mk({ name: "n", text: "a-b" }), "-").score, 100);
  check("a dash term inside a phrase is a plain word", score(mk({ name: "n", text: "foo - bar" }), "foo -").score, 120);
  check("an exclusion is case-folded", score(hw, "hello -WORLD"), none);

  // ---- tag: and var: conditions
  const w = mk({ name: "n", text: "hello", tags: ["Work"] });
  check("tag condition met", score(w, "tag:work"), all);
  check("tag condition is case-insensitive", score(w, "tag:WORK"), all);
  check("tag condition is exact, not a substring", score(mk({ tags: ["Workshop"] }), "tag:work"), none);
  check("tag condition on a prompt with no tags", score(mk({}), "tag:work"), none);
  check("tag names are compared trimmed", score(mk({ tags: [" Work "] }), "tag:work"), all);
  check("negated tag condition fails a tagged prompt", score(w, "-tag:work"), none);
  check("negated tag condition keeps an untagged prompt", score(mk({}), "-tag:work"), all);
  check("a condition plus a word scores the word", score(w, "tag:work hello").score, 120);
  check("a failed condition removes a matching prompt", score(w, "tag:other hello"), none);
  check("a met condition with a missing word is no match", score(w, "tag:work zzz"), none);
  check("a condition with an exclusion", score(w, "tag:work -hello"), none);
  check("an empty tag value is a plain word", score(mk({ name: "tag:x", text: "" }), "tag:").score, 360);
  const v = mk({ name: "n", text: "Hi {?Name=Bob} and {?c|a,b} {?@date}", variants: ["{?topic}"] });
  check("var condition met by a plain name", score(v, "var:name"), all);
  check("var condition reads the name before the default", score(v, "var:bob"), none);
  check("var condition reads the name before the options", score(v, "var:c"), all);
  check("var condition finds a reserved name", score(v, "var:@date"), all);
  check("var condition finds a variant's variable", score(v, "var:topic"), all);
  check("var condition is exact", score(v, "var:nam"), none);
  check("var condition is case-insensitive", score(mk({ text: "{?Kunde}" }), "var:KUNDE"), all);
  check("negated var condition", score(v, "-var:name"), none);
  check("negated var condition keeps a prompt without it", score(mk({ text: "plain" }), "-var:name"), all);
  check("two conditions both have to hold", [score(mk({ text: "{?a}", tags: ["x"] }), "tag:x var:a").score, score(mk({ text: "{?a}", tags: ["y"] }), "tag:x var:a").score], [1, 0]);
  const vv = mk({ text: "{?a}" });
  check("var condition before an edit", score(vv, "var:a"), all);
  vv.text = "{?b}";
  check("an edited body drops the old variable", score(vv, "var:a"), none);
  check("an edited body brings the new variable", score(vv, "var:b"), all);
  check("searchConditionMet: tag", [api.searchConditionMet({ prefix: "tag", value: "work", neg: false }, w), api.searchConditionMet({ prefix: "tag", value: "work", neg: true }, w)], [true, false]);

  // ---- the typo-tolerant tier
  const fz = { fuzzySearch: true };
  const mt = mk({ name: "Meeting notes", text: "agenda for monday" });
  check("typo tier off: a typo finds nothing", score(mt, "agnda"), none);
  check("typo tier on: one typo in a body word", score(mt, "agnda", fz), { score: 40, fuzzy: true, entryIndex: 0, field: "text", q: "agnda" });
  check("typo tier on: the name weighs three times", score(mt, "metting", fz), { score: 120, fuzzy: true, entryIndex: 0, field: "name", q: "metting" });
  check("typo tier on: two words in one field", score(mt, "meting nots", fz).score, 120);
  check("typo tier on: one of two words is not enough", score(mt, "meting zzzz", fz), none);
  check("typo tier on: words must hit inside one field", score(mt, "meting agnda", fz), none);
  check("typo tier on: too many typos", score(mt, "mtng", fz), none);
  check("typo tier on: an exact substring stays exact", score(mt, "agenda", fz), { score: 120, fuzzy: false, entryIndex: 0, field: "text", q: "agenda" });
  check("typo tier on: a tag typo", score(mk({ name: "n", text: "t", tags: ["Urgent"] }), "urgnt", fz), { score: 80, fuzzy: true, entryIndex: 0, field: "tags", q: "urgnt" });
  check("typo tier on: the start of a longer word", score(mk({ name: "n", text: "codebase layout" }), "cde", fz).score, 40);
  check("typo tier on: nothing alike", score(mk({ name: "n", text: "codebase layout" }), "xyz", fz), none);
  check("typo tier on: only punctuation has no words", score(mt, "!!", fz), none);
  check("typo tier on: a variant typo reports the entry", score(mk({ name: "n", text: "alpha", variants: ["omega star"] }), "omga", fz), { score: 40, fuzzy: true, entryIndex: 1, field: "variant", q: "omga" });
  check("typo tier on: exclusions still apply", score(mt, "agnda -monday", fz), none);
  check("typo tier on: conditions still apply", score(mk({ name: "Meeting", text: "agenda", tags: ["a"] }), "tag:b agnda", fz), none);
  check("typo tier on: unicode words", score(mk({ name: "n", text: "caf\u00e9 cr\u00e8me" }), "cafe", fz).fuzzy, true);

  // ---- last resort: text recognized inside a picture
  const pic = mk({ name: "zzz", text: "" });
  const hits = (q, ids) => ({ query: q, ids: new Set(ids) });
  sb.OPT.ocrIndex = true;
  check("a picture hit scores ninety", score(pic, "needle", fz, hits("needle", [pic.id])), { score: 90, fuzzy: false, entryIndex: 0, fromImage: true });
  check("a picture hit also counts while the typo tier is off", score(pic, "needle", {}, hits("needle", [pic.id])), { score: 90, fuzzy: false, entryIndex: 0, fromImage: true });
  check("a picture hit counts for a query without word tokens", score(pic, "!?", fz, hits("!?", [pic.id])), { score: 90, fuzzy: false, entryIndex: 0, fromImage: true });
  check("a picture hit for another query does not count", score(pic, "needle", fz, hits("needl", [pic.id])), none);
  check("a picture hit for another prompt does not count", score(pic, "needle", fz, hits("needle", [pic.id + 1000])), none);
  check("the query is compared trimmed and folded", score(pic, "  NeEdLe ", fz, hits("needle", [pic.id])).score, 90);
  sb.imageHitQuery = "needle"; sb.imageHitIds = new Set([pic.id]);
  check("without a handed-in list the library's own list is used", score(pic, "needle", fz).score, 90);
  sb.imageHitQuery = ""; sb.imageHitIds = new Set();
  check("a name hit outranks a picture hit", score(mk({ name: "needle" }), "needle", fz, hits("needle", [nextId - 1])).score, 360);
  sb.OPT.ocrIndex = false;
  check("a picture hit is ignored when the feature is off", score(pic, "needle", fz, hits("needle", [pic.id])), none);

  // ---- caches never answer for an older query or an older text
  const c = mk({ name: "Foo", text: "alpha" });
  check("query one", score(c, "alpha").score, 120);
  check("query two on the same prompt", score(c, "foo").score, 360);
  check("query one again", score(c, "alpha").score, 120);
  check("the typo flag is part of the query memo", [score(c, "alpa").score, score(c, "alpa", fz).score], [0, 40]);
  c.text = "omega";
  check("an edited body is not read stale", [score(c, "alpha").score, score(c, "omega").score], [0, 120]);
  c.name = "Renamed";
  check("an edited name is not read stale", [score(c, "foo").score, score(c, "renamed").score], [0, 360]);
  const n = mk({ name: "n", text: "t", notes: "milk" });
  check("a notes switch is not read stale", [score(n, "milk").score, score(n, "milk", { promptNotes: false }).score, score(n, "milk").score], [60, 0, 60]);
  const tg = mk({ name: "n", text: "t", tags: ["red"] });
  check("a tags switch is not read stale", [score(tg, "red").score, score(tg, "red", { tags: false }).score], [240, 0]);
  tg.tags.push("blue");
  check("a tag added in place is found", score(tg, "blue").score, 200);
  const vr = mk({ name: "n", text: "t", variants: [] });
  check("before a variant is added", score(vr, "extra").score, 0);
  vr.variants.push("extra thing");
  check("a variant added in place is found", score(vr, "extra"), { score: 120, fuzzy: false, entryIndex: 1, field: "variant", q: "extra" });
  vr.variants[0] = "other";
  check("a variant changed in place is not read stale", [score(vr, "extra").score, score(vr, "other").score], [0, 120]);

  // The per-render score memo keys on the prompt id (cleared by the caller each render).
  setFlags({});
  sb.libQuery = "foo";
  const pa = mk({ name: "Foo" });
  const first = api.libSearchScore(pa);
  check("libSearchScore memoizes per prompt id", api.libSearchScore(pa) === first, true);
  check("libSearchScore keeps one entry per id", api.libScoreCache.size >= 1 && api.libScoreCache.has(pa.id), true);
  api.libScoreCache.clear();
  sb.libQuery = "zzz";
  check("after the memo is cleared the new query scores", api.libSearchScore(pa).score, 0);
}

main().then(() => {
  if (problems.length) {
    console.log(`${problems.length} problem(s):`);
    for (const p of problems) console.log(" - " + p);
    process.exit(1);
  }
  console.log(`library_search_check: ${count} assertions OK`);
}).catch((e) => { console.log(e.stack || e); process.exit(1); });
