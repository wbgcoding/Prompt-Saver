// Expert menu "only changed" filter (ui/expertMenu.js): a row that was shown as
// changed stays on screen after it is reset, until the view changes (filter
// toggled, expert page reopened or left, search typed, jump to a setting).
//
// expertMenu.js is wired into the main window, so nothing can be imported from it.
// This pulls the exact source text of keepChangedRow, renderExpert and the
// view-change handlers out of the real file, so the check fails on whatever the
// file contains and not on a hand-copied reimplementation.
// Run:  node tests/expert_only_changed_check.js      Exit: 0 = clean, 1 = problems.
const vm = require("vm");

const src = require("./ui_sources.js").mainWindowSource().replace(/\r\n/g, "\n");
const problems = [];
const fail = (msg) => problems.push(msg);

// "<marker> ... <matching closing brace>", counting braces from the marker's own
// opening one so a nested if/for does not end the extraction early.
function extractBlock(label, marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find ${label} in expertMenu.js (source text changed?): ${marker}`);
  if (src.indexOf(marker, start + 1) !== -1) throw new Error(`${label}: marker is not unique: ${marker}`);
  const braceStart = src.indexOf("{", start);
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}") {
      depth--;
      if (depth === 0) return src.slice(start, i + 1);
    }
  }
  throw new Error(`unbalanced braces extracting ${label} from expertMenu.js`);
}

let helper;
try {
  helper = extractBlock("keepChangedRow", "export function keepChangedRow(");
} catch (e) {
  console.log("FAIL: " + e.message);
  process.exit(1);
}
const sandbox = {};
vm.createContext(sandbox);
vm.runInContext(helper.replace(/^export /, ""), sandbox, { filename: "expertMenu.js (extracted)" });
const keep = (...a) => sandbox.keepChangedRow(...a);

// What the old rule did, to show the check is not vacuous: "changed" alone drops a
// row the moment it is reset.
const oldRule = (changed) => changed;
{
  let changedA = false;
  changedA = true;
  if (!oldRule(changedA)) fail("self-check: the old rule should show a changed row");
  changedA = false;
  if (oldRule(changedA)) fail("self-check: the old rule should drop a reset row");
}

// The sequence from the plan: filter on, change A, re-render; reset A; toggle the filter.
{
  const shown = new Set();
  let a = false;
  if (keep("values", "A", a, shown)) fail("an unchanged row must stay hidden before any change");
  a = true;
  if (!keep("values", "A", a, shown)) fail("a changed row must be shown");
  a = false;
  if (!keep("values", "A", a, shown)) fail("a row reset during this view must stay shown");
  if (!keep("values", "A", a, shown)) fail("the row must stay shown on every further render");
  if (keep("values", "B", false, shown)) fail("another unchanged row must not appear");
  if (keep("flags", "A", false, shown)) fail("the same key under another kind is a different row");
  shown.clear();
  if (keep("values", "A", a, shown)) fail("after the view changed (set cleared) the reset row must be gone");
  if (!keep("values", "A", true, shown)) fail("a changed row is shown again after the set was cleared");
}

// renderExpert uses the helper for both filters, never the bare isRowChanged() rule.
{
  const render = extractBlock("renderExpert", "export function renderExpert() {");
  if (!/keepChangedRow\(kind, k, isRowChanged\(kind, k\), expertShownChanged\)/.test(render)) {
    fail("renderExpert: the value/select/dropdown filter does not go through keepChangedRow");
  }
  if (!/keepChangedRow\("flags", f\.key, isRowChanged\("flags", f\.key\), expertShownChanged\)/.test(render)) {
    fail("renderExpert: the flag filter does not go through keepChangedRow");
  }
  if (/expertOnlyChanged && !isRowChanged\(/.test(render) || /!expertOnlyChanged \|\| isRowChanged\(/.test(render)) {
    fail("renderExpert: a bare isRowChanged() filter is still there");
  }
}

// Every place where the view changes clears the set.
const clearSites = {
  "showExpertPage": "export function showExpertPage(on) {",
  "jumpToExpertSetting": "export function jumpToExpertSetting(key, kind) {",
  "#expert-open click": "$(\"expert-open\").addEventListener(\"click\", () => {",
  "#expert-search input": "$(\"expert-search\").addEventListener(\"input\", (e) => {",
  "#expert-only-changed change": "$(\"expert-only-changed\").addEventListener(\"change\", (e) => {",
};
for (const [label, marker] of Object.entries(clearSites)) {
  let block;
  try { block = extractBlock(label, marker); } catch (e) { fail(e.message); continue; }
  if (!block.includes("expertShownChanged.clear()")) fail(`${label}: does not clear expertShownChanged`);
}
if (!/const expertShownChanged = new Set\(\);/.test(src)) fail("expertShownChanged is not declared as a Set");

if (problems.length) {
  console.log(`FAIL (${problems.length})`);
  for (const p of problems) console.log("  - " + p);
  process.exit(1);
}
console.log("PASS: expert only-changed keeps rows reset during this view");
