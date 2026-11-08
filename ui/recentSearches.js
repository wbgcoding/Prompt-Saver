// The one recent-search list shared by the library, the journal and the global search.
// Terms live in ui_texts (encrypted like the rest of the settings), newest first, deduped
// case-insensitively, capped at the searchListMax expert value. Pinned searches live beside
// them in their own key, newest first, at most 10, and hide a recent twin.
import { flag, invoke, settings, t, val } from "./core.js";

const KEY = "recentSearches";
const PIN_KEY = "pinnedSearches";
export const PINNED_SEARCH_MAX = 10;
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

// ---- Pinned searches: {query, tags (lowercase keys, sorted), sort}, newest first ----
function pinNorm(e) {
  const tags = [...new Set((Array.isArray(e?.tags) ? e.tags : []).map((x) => String(x ?? "").trim().toLowerCase()).filter(Boolean))].sort();
  return { query: String(e?.query ?? "").trim(), tags, sort: typeof e?.sort === "string" ? e.sort : "" };
}

// Same search = same query (case-insensitive) and the same tag set.
const pinKey = (e) => e.query.toLowerCase() + "\u0000" + e.tags.join(",");

function pinStored() {
  try {
    const a = JSON.parse(settings.ui_texts?.[PIN_KEY] || "[]");
    if (!Array.isArray(a)) return [];
    const seen = new Set();
    const out = [];
    for (const raw of a) {
      const e = pinNorm(raw);
      if ((!e.query && !e.tags.length) || seen.has(pinKey(e))) continue;
      seen.add(pinKey(e));
      out.push(e);
    }
    return out.slice(0, PINNED_SEARCH_MAX);
  } catch { return []; }
}

function pinSave(list) {
  rsWrite(PIN_KEY, JSON.stringify(list.slice(0, PINNED_SEARCH_MAX)));
}

// The pinned searches to offer, newest first; nothing while the recent-searches switch is off.
export function pinnedList() {
  return flag("searchSuggest") ? pinStored() : [];
}

// Pins a search (moves an existing twin to the top). False at the cap, nothing saved then.
export function pinSearch(entry) {
  const e = pinNorm(entry);
  if (!e.query && !e.tags.length) return false;
  const rest = pinStored().filter((x) => pinKey(x) !== pinKey(e));
  const had = rest.length !== pinStored().length;
  if (!had && rest.length >= PINNED_SEARCH_MAX) return false;
  pinSave([e, ...rest]);
  return true;
}

// Removes the pin and gives its query back to the top of the recent searches.
export function unpinSearch(entry) {
  forgetPinned(entry);
  rememberSearch(entry.query);
}

// Removes the pin only.
export function forgetPinned(entry) {
  const e = pinNorm(entry);
  pinSave(pinStored().filter((x) => pinKey(x) !== pinKey(e)));
}

// The terms to offer; nothing while the recent-searches switch is off. A term a pin already
// holds is left out; the cap counts the terms shown, not the pins.
export function recentList() {
  if (!flag("searchSuggest")) return [];
  rsMigrate();
  const pinned = new Set(pinStored().map((e) => e.query.toLowerCase()));
  return rsParse(settings.ui_texts?.[KEY]).filter((x) => !pinned.has(x.toLowerCase())).slice(0, val("searchListMax"));
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
// opts: id, rowClass / delClass (extra hooks), onPick(term), onForget(term). With onPin (a
// recent row) or onUnpin (a pinned one) a pin button sits before the x; a pinned row also
// passes pinned: true and its entry, which adds its tags after the text.
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
  const parts = term ? [label] : [];
  if (opts.pinned) {
    row.dataset.pinned = "1";
    const tags = opts.entry?.tags || [];
    if (tags.length) {
      const span = document.createElement("span");
      span.className = "search-suggest-tags";
      span.textContent = tags.map((x) => "#" + x).join(" ");
      parts.push(span);
    }
  }
  if (opts.onPin || opts.onUnpin) {
    const pin = document.createElement("button");
    pin.type = "button";
    pin.className = "search-suggest-pin";
    const name = t(opts.pinned ? "searchUnpin" : "searchPin");
    pin.title = name;
    pin.setAttribute("aria-label", name);
    pin.setAttribute("aria-pressed", String(!!opts.pinned));
    pin.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12" aria-hidden="true"><g transform="rotate(45 12 12)"><path fill="currentColor" d="M8.5 2.5h7a1.25 1.25 0 0 1 0 2.5H15v4.2l2.6 2.6c.25.25.4.6.4.95V14a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1v-1.25c0-.35.15-.7.4-.95L9 9.2V5h-.5a1.25 1.25 0 0 1 0-2.5Z"/><path fill="currentColor" d="M11.1 15h1.8v5.3L12 22l-.9-1.7Z"/></g></svg>';
    pin.addEventListener("click", (e) => {
      e.stopPropagation();
      if (opts.pinned) opts.onUnpin?.(opts.entry); else opts.onPin?.(term);
    });
    parts.push(pin);
  }
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
  row.append(...parts, del);
  return row;
}
