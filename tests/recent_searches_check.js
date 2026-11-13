// Recent and pinned searches (ui/recentSearches.js): the shared recent list (minimum length,
// case-insensitive dedupe, newest first, the searchListMax cap, the switch), the one-time merge of
// the older per-field lists, bad stored data, and the pinned searches (normalised entries, same
// search = same query + tag set, the cap of ten, hiding a recent twin, unpin back to recent).
// The real source is run in a node vm with the core imports stubbed.
// Run:  node tests/recent_searches_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const problems = [];
let count = 0;
const check = (label, actual, expected) => {
  count++;
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) problems.push(`${label}: got ${a}, expected ${e}`);
};

const src = fs.readFileSync(path.join(__dirname, "..", "ui", "recentSearches.js"), "utf8")
  .replace(/^import .*$/gm, "")
  .replace(/^export /gm, "");

const writes = [];
const ctx = {
  console,
  document: { createElement: () => ({}) },
  flag: (k) => ctx.flags[k] !== false,
  flags: {},
  cap: 5,
  val: (k) => (k === "searchListMax" ? ctx.cap : 0),
  invoke: (cmd, args) => { writes.push([cmd, args]); return Promise.resolve(); },
  settings: {},
  t: (k) => k,
};
vm.createContext(ctx);
vm.runInContext(src + "\nthis.api = { PINNED_SEARCH_MAX, pinnedList, pinSearch, unpinSearch, forgetPinned, recentList, rememberSearch, forgetSearch, clearSearches };", ctx);
const R = ctx.api;

const reset = (texts = {}, opts = {}) => {
  ctx.settings = { ui_texts: texts };
  ctx.flags = opts.flags || {};
  ctx.cap = opts.cap || 5;
  writes.length = 0;
};
const stored = (key = "recentSearches") => JSON.parse(ctx.settings.ui_texts[key] || "[]");

// ---- recent list
reset();
check("nothing stored reads empty", R.recentList(), []);
R.rememberSearch("alpha");
check("a term is remembered", R.recentList(), ["alpha"]);
check("it is saved through set_ui_text", writes[0], ["set_ui_text", { key: "recentSearches", value: '["alpha"]' }]);
R.rememberSearch("beta");
check("newest first", R.recentList(), ["beta", "alpha"]);
R.rememberSearch("ALPHA");
check("a case twin moves to the top with the new spelling", R.recentList(), ["ALPHA", "beta"]);
R.rememberSearch("  gamma  ");
check("a term is trimmed", R.recentList()[0], "gamma");
R.rememberSearch("x");
check("one character is not kept", R.recentList().includes("x"), false);
R.rememberSearch(" y ");
check("a one-character term with blanks is not kept", R.recentList().includes("y"), false);
R.rememberSearch("");
R.rememberSearch(null);
R.rememberSearch(undefined);
check("empty, null and undefined are ignored", R.recentList(), ["gamma", "ALPHA", "beta"]);
R.rememberSearch("ab");
check("two characters are enough", R.recentList()[0], "ab");

reset({}, { cap: 3 });
for (const w of ["one1", "two2", "thr3", "fou4"]) R.rememberSearch(w);
check("the list is capped at searchListMax", R.recentList(), ["fou4", "thr3", "two2"]);
check("the stored list is capped as well", stored(), ["fou4", "thr3", "two2"]);
ctx.cap = 2;
check("a lower cap shortens what is shown", R.recentList(), ["fou4", "thr3"]);

reset({}, { flags: { searchSuggest: false } });
R.rememberSearch("hidden");
check("with the switch off nothing is remembered", [writes.length, ctx.settings.ui_texts.recentSearches], [0, undefined]);
reset({ recentSearches: '["kept1","kept2"]' }, { flags: { searchSuggest: false } });
check("with the switch off the list reads empty", R.recentList(), []);
check("with the switch off the pins read empty", (ctx.settings.ui_texts.pinnedSearches = '[{"query":"pin","tags":[],"sort":""}]', R.pinnedList()), []);

reset({ recentSearches: '["Keep","other"]' });
R.forgetSearch("keep");
check("forget is case-insensitive", R.recentList(), ["other"]);
R.clearSearches();
check("clear empties the list", [R.recentList(), ctx.settings.ui_texts.recentSearches], [[], "[]"]);

// ---- stored data that is not clean
reset({ recentSearches: '["a","ok",5,null,"  b "," cd ",{"x":1}]' });
check("short, non-string and blank entries are dropped", R.recentList(), ["ok", " cd "]);
reset({ recentSearches: "not json" });
check("broken JSON reads empty", R.recentList(), []);
reset({ recentSearches: '{"a":1}' });
check("a non-array reads empty", R.recentList(), []);
reset({ recentSearches: "" });
check("an empty string reads empty", R.recentList(), []);
reset({ recentSearches: '["aa","bb","cc","dd"]' }, { cap: 2 });
check("a stored list longer than the cap is shown cut", R.recentList(), ["aa", "bb"]);

// ---- merge of the older lists
reset({
  recentSearches: '["s1"]',
  globalSearchRecent: '["g1","g2"]',
  recentSearchLib: '["l1","G1"]',
  recentSearchJournal: '["j1"]',
}, { cap: 10 });
check("old lists merge after the shared one, deduped", R.recentList(), ["s1", "g1", "g2", "l1", "j1"]);
check("the merged list is saved", stored(), ["s1", "g1", "g2", "l1", "j1"]);
check("the old keys are emptied", ["globalSearchRecent", "recentSearchLib", "recentSearchJournal"].map((k) => ctx.settings.ui_texts[k]), ["", "", ""]);
check("each emptied key is sent to the backend", writes.filter((w) => w[1].value === "").map((w) => w[1].key).sort(), ["globalSearchRecent", "recentSearchJournal", "recentSearchLib"]);
const sent = writes.length;
R.recentList();
check("a second read does not merge again", writes.length, sent);
reset({ globalSearchRecent: '["g1"]' }, { cap: 10 });
R.rememberSearch("new1");
check("remembering also folds in the old list", R.recentList(), ["new1", "g1"]);
reset({ globalSearchRecent: '["aa","bb","cc"]', recentSearchLib: '["dd","ee"]' }, { cap: 3 });
check("the merge is cut to the cap", R.recentList(), ["aa", "bb", "cc"]);

// ---- pinned searches
reset();
check("no pins read empty", R.pinnedList(), []);
check("a pin is accepted", R.pinSearch({ query: "Foo", tags: ["B", "a", " a"], sort: "name" }), true);
check("the pin is normalised", R.pinnedList(), [{ query: "Foo", tags: ["a", "b"], sort: "name" }]);
check("the pin is saved under its own key", writes[0][1].key, "pinnedSearches");
check("an empty search is not pinned", R.pinSearch({ query: "  ", tags: [] }), false);
check("a missing entry is not pinned", R.pinSearch(undefined), false);
check("a tag-only search can be pinned", R.pinSearch({ query: "", tags: ["x"] }), true);
check("it reads back newest first", R.pinnedList().map((e) => e.query + "|" + e.tags.join(",")), ["|x", "Foo|a,b"]);
check("a non-string sort reads empty", R.pinSearch({ query: "q1", tags: [], sort: 5 }) && R.pinnedList()[0].sort, "");
check("a null tag is dropped", (R.pinSearch({ query: "q2", tags: [null, "t"] }), R.pinnedList()[0].tags), ["t"]);
const before = R.pinnedList().length;
check("the same query and tags is a twin: moved to the top", [R.pinSearch({ query: "FOO", tags: ["a", "b"] }), R.pinnedList().length, R.pinnedList()[0].query], [true, before, "FOO"]);
check("the same query with other tags is a different search", [R.pinSearch({ query: "foo", tags: ["a"] }), R.pinnedList().length], [true, before + 1]);

reset();
for (let i = 1; i <= 10; i++) R.pinSearch({ query: "pin" + i, tags: [] });
check("ten pins fit", R.pinnedList().length, 10);
const w = writes.length;
check("the eleventh is refused", R.pinSearch({ query: "pin11", tags: [] }), false);
check("a refusal saves nothing", [R.pinnedList().length, writes.length], [10, w]);
check("a twin at the cap is still accepted", R.pinSearch({ query: "PIN3", tags: [] }), true);
check("the twin moved to the top", [R.pinnedList().length, R.pinnedList()[0].query], [10, "PIN3"]);
check("the cap constant", R.PINNED_SEARCH_MAX, 10);

// ---- pins read from storage
reset({ pinnedSearches: JSON.stringify([
  { query: "a b", tags: ["T"] },
  { query: "A B", tags: ["t"] },
  { query: "", tags: [] },
  { query: " lone ", tags: [], sort: "date" },
  null,
]) });
check("duplicates and empty pins are dropped on read", R.pinnedList(), [{ query: "a b", tags: ["t"], sort: "" }, { query: "lone", tags: [], sort: "date" }]);
reset({ pinnedSearches: "not json" });
check("broken pin JSON reads empty", R.pinnedList(), []);
reset({ pinnedSearches: '{"query":"x"}' });
check("a non-array pin list reads empty", R.pinnedList(), []);
reset({ pinnedSearches: JSON.stringify(Array.from({ length: 14 }, (_, i) => ({ query: "p" + i, tags: [] }))) });
check("a stored list over the cap is cut to ten", R.pinnedList().length, 10);

// ---- pins and recents together
reset({ recentSearches: '["Alpha","beta"]', pinnedSearches: JSON.stringify([{ query: "alpha", tags: ["x"], sort: "" }]) });
check("a pinned query hides its recent twin", R.recentList(), ["beta"]);
reset({ recentSearches: '["aa","bb","cc"]', pinnedSearches: JSON.stringify([{ query: "aa", tags: [], sort: "" }]) }, { cap: 2 });
check("the cap counts the terms shown, not the pins", R.recentList(), ["bb", "cc"]);
reset({ pinnedSearches: JSON.stringify([{ query: "Gone", tags: ["t"], sort: "" }, { query: "stay", tags: [], sort: "" }]) });
R.unpinSearch({ query: "Gone", tags: ["T"] });
check("unpin removes the pin", R.pinnedList().map((e) => e.query), ["stay"]);
check("unpin gives the query back to the recents", R.recentList(), ["Gone"]);
reset({ pinnedSearches: JSON.stringify([{ query: "keep", tags: [], sort: "" }, { query: "drop", tags: ["a"], sort: "" }]) });
R.forgetPinned({ query: "DROP", tags: ["A"] });
check("forgetPinned removes by query and tag set, case-insensitively", R.pinnedList().map((e) => e.query), ["keep"]);
R.forgetPinned({ query: "keep", tags: ["other"] });
check("a different tag set removes nothing", R.pinnedList().map((e) => e.query), ["keep"]);
check("forgetPinned does not touch the recents", R.recentList(), []);

if (problems.length) {
  console.log(`${problems.length} problem(s):`);
  for (const p of problems) console.log(" - " + p);
  process.exit(1);
}
console.log(`recent_searches_check: ${count} assertions OK`);
