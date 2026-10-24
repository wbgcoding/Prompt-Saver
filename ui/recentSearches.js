// The one recent-search list shared by the library, the journal and the global search.
// Terms live in ui_texts (encrypted like the rest of the settings), newest first, deduped
// case-insensitively, capped at the searchListMax expert value.
import { flag, invoke, settings, t, val } from "./core.js";

const KEY = "recentSearches";
// Per-field lists of earlier versions, merged into the shared one on first read.
const OLD_KEYS = ["globalSearchRecent", "recentSearchLib", "recentSearchJournal"];

function rsParse(raw) {
  try {
    const a = JSON.parse(raw || "[]");
    return Array.isArray(a) ? a.filter((x) => typeof x === "string" && x.trim().length >= 2) : [];
  } catch { return []; }
}

function rsDedupe(list) {
  const seen = new Set();
  const out = [];
  for (const term of list) {
    const k = term.toLowerCase();
    if (seen.has(k)) continue;
    seen.add(k);
    out.push(term);
  }
  return out;
}

function rsWrite(key, value) {
  settings.ui_texts = { ...(settings.ui_texts || {}), [key]: value };
  invoke("set_ui_text", { key, value }).catch(() => {});
}

function rsSave(list) {
  rsWrite(KEY, JSON.stringify(list.slice(0, val("searchListMax"))));
}

// Folds the old per-field keys into the shared key (global, library, journal order after the
// terms already in the shared list), then empties them.
function rsMigrate() {
  const texts = settings.ui_texts || {};
  const old = OLD_KEYS.filter((k) => texts[k]);
  if (!old.length) return;
  const merged = rsDedupe([KEY, ...OLD_KEYS].flatMap((k) => rsParse(texts[k])));
  rsSave(merged);
  for (const k of old) rsWrite(k, "");
}

function rsStored() {
  rsMigrate();
  return rsParse(settings.ui_texts?.[KEY]).slice(0, val("searchListMax"));
}

// The terms to offer; nothing while the recent-searches switch is off.
export function recentList() {
  return flag("searchSuggest") ? rsStored() : [];
}

export function rememberSearch(term) {
  term = (term || "").trim();
  if (term.length < 2 || !flag("searchSuggest")) return;
  rsSave([term, ...rsStored().filter((x) => x.toLowerCase() !== term.toLowerCase())]);
}

export function forgetSearch(term) {
  rsSave(rsStored().filter((x) => x.toLowerCase() !== term.toLowerCase()));
}

export function clearSearches() {
  rsSave([]);
}

// One row of the list, the same markup and classes in every place that shows it.
// opts: id, rowClass / delClass (extra hooks), onPick(term), onForget(term).
export function recentRow(term, opts) {
  const row = document.createElement("div");
  row.className = "search-suggest-item" + (opts.rowClass ? " " + opts.rowClass : "");
  if (opts.id) row.id = opts.id;
  row.setAttribute("role", "option");
  row.setAttribute("aria-selected", "false");
  row.dataset.term = term;
  const label = document.createElement("span");
  label.className = "search-suggest-text";
  label.textContent = term;
  const del = document.createElement("button");
  del.type = "button";
  del.className = "search-suggest-del" + (opts.delClass ? " " + opts.delClass : "");
  del.textContent = "×";
  del.title = t("delete");
  del.setAttribute("aria-label", t("delete"));
  // The field keeps the focus: a press must not blur it before the click lands.
  row.addEventListener("mousedown", (e) => e.preventDefault());
  del.addEventListener("click", (e) => {
    e.stopPropagation();
    opts.onForget(term);
  });
  row.addEventListener("click", () => opts.onPick(term));
  row.append(label, del);
  return row;
}
