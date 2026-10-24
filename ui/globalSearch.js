// Search everything overlay.
import { copyOnCooldown, recordCopy } from "./copyFlow.js";
import { assignLibQuery, computeLibSearchScore, copyResolved, libQuery, toastCopied } from "./library.js";
import { $, debounced, flag, gateOpen, invoke, optFlag, prompts, settings, t, val } from "./core.js";
import { editPrompt } from "./editor.js";
import { switchToView } from "./views.js";
import { varPreview } from "./floatShared.js";
import { clearSearches, forgetSearch, recentList, recentRow, rememberSearch } from "./recentSearches.js";
import { EXPERT_DEFS, EXPERT_LABEL_KEYS, jumpToExpertSetting } from "./expertMenu.js";
import { openSettingsDirectoryEntry, settingHit, SETTINGS_DIRECTORY, settingsDirectoryHit } from "./settingsSearch.js";
import { EXPERT_TABS, flagLabelKey, groupFlags, SETTINGS_AREA_LABEL } from "./expertConfig.js";
import { assignHelpCollapsed, assignHelpQuery, helpCategories, helpNorm, helpSectionLabel, renderHelp } from "./helpView.js";

// ---- Search everything ----
// One overlay over the main window that searches six sources at once: the prompt grid,
// the copy history, file attachments, views, settings (the expert menu and the normal
// settings directory) and the in-app help. It sits above those sources' own search fields
// and replaces none. It borrows the quick launcher's card/row look but is a plain overlay
// in this window, not a second webview: everything that makes settings searchable
// (EXPERT_TABS, SETTINGS_DIRECTORY, i18nHit, gateOpen) lives in this script's state, and
// a second window would need a copy of it or a risky expert-menu rebuild.
export let gsOpen = false;
export let gsQuery = "";
export let gsActive = -1; // index into gsFlatRows, the flat list a rendered pass produced
export let gsExpanded = new Set(); // group keys currently past their "show more"
let gsRowSeq = 0; // unique ids for the result rows
export let gsFlatRows = []; // [{ el, run }], in on-screen order, across every group
export let gsHistoryLog = []; // recent_copies, refetched each time the overlay opens

// Copies a prompt like the copy history's "copy again": cooldown guard, variable/rotation
// resolution through copyResolved, usage recorded, toast. Not libraryCopy(): that also
// reads the library's closeAfterCopy/auto-paste-double-click state, which does not apply
// to a result picked here.
export async function gsCopyPrompt(p) {
  if (copyOnCooldown(p.id)) return;
  if (!(await copyResolved(p))) return;
  recordCopy(p.id);
  toastCopied(p);
}

// Recognized picture text for the overlay's own query. Owned here: the library keeps its own
// hit set for its own search field, and this one never touches it. `gsImageAsked` is the
// query already sent, so a render never fires the same lookup twice.
let gsImageHits = { query: "", ids: new Set() };
let gsImageAsked = "";
export function gsResetImageHits() {
  gsImageHits = { query: "", ids: new Set() };
  gsImageAsked = "";
}
export async function gsRefreshImageHits() {
  const q = gsQuery.trim().toLowerCase();
  if (!optFlag("ocrIndex") || q.length < 2 || q === gsImageAsked) return;
  gsImageAsked = q;
  let ids = [];
  try { ids = (await invoke("search_image_text", { query: q })) || []; } catch { ids = []; }
  // An answer for an older query, or one that lands after the overlay closed, is dropped.
  if (!gsOpen || q !== gsQuery.trim().toLowerCase()) return;
  gsImageHits = { query: q, ids: new Set(ids) };
  if (ids.length) gsRender();
}

// Reuses the library's scoring (computeLibSearchScore). It reads the module-level
// libQuery instead of taking a query, so the query is swapped in and restored around one
// synchronous pass; nothing async runs in between, so a library search mid-flight never
// sees the swap.
export function gsPromptHits(q) {
  const saved = libQuery;
  assignLibQuery(q);
  let scored;
  try {
    scored = prompts
      .map((p) => ({ p, r: computeLibSearchScore(p, gsImageHits) }))
      .filter((x) => x.r.score > 0)
      // A prompt that matches only through its file path is listed under attachments.
      .filter((x) => x.r.field !== "file" || computeLibSearchScore({ ...x.p, file_path: "" }, gsImageHits).score > 0)
      .sort((a, b) => b.r.score - a.r.score);
  } finally {
    assignLibQuery(saved);
  }
  return scored.map(({ p }) => ({
    label: p.name || "",
    sub: varPreview(p.text).replace(/\s+/g, " ").trim().slice(0, 80),
    color: p.color,
    run: () => gsCopyPrompt(p),
  }));
}

// The copy history has no text of its own to search: each entry is just an id
// and a timestamp: so a hit here is "one of the recently-copied prompts whose
// name/body/tags/notes match", scored through the very same pass as the prompt
// group above. One entry per prompt (a prompt copied five times shows once).
export function gsHistoryHits(q) {
  const byId = new Map(prompts.map((p) => [p.id, p]));
  const saved = libQuery;
  assignLibQuery(q);
  let scored;
  try {
    const seen = new Set();
    scored = [];
    for (const e of gsHistoryLog) {
      const p = byId.get(e.id);
      if (!p || seen.has(p.id)) continue;
      const r = computeLibSearchScore(p, gsImageHits);
      if (r.score > 0) { scored.push({ p, r }); seen.add(p.id); }
    }
    scored.sort((a, b) => b.r.score - a.r.score);
  } finally {
    assignLibQuery(saved);
  }
  return scored.map(({ p }) => ({
    label: p.name || "",
    sub: varPreview(p.text).replace(/\s+/g, " ").trim().slice(0, 80),
    color: p.color,
    run: () => gsCopyPrompt(p),
  }));
}

// A prompt's attached file (p.file_path) is the only per-prompt attachment in the data
// model; matched on the full path so a hit works for the file name or a folder it lives
// under. "Open" uses the edit dialog a tile double-click opens (it renders the attachment
// preview); there is no separate viewer.
export function gsAttachmentHits(q) {
  const ql = q.toLowerCase();
  return prompts
    .filter((p) => p.file_path && p.file_path.toLowerCase().includes(ql))
    .map((p) => ({
      label: p.name || p.file_path.split(/[\\/]/).pop(),
      sub: p.file_path,
      color: p.color,
      run: () => editPrompt(p.id),
    }));
}

export function gsViewHits(q) {
  const ql = q.toLowerCase();
  return (settings.views || [])
    .filter((v) => (v.name || "").toLowerCase().includes(ql))
    .map((v) => ({ label: v.name || "", sub: "", color: v.color, run: () => switchToView(v.id) }));
}

// Walks both settings surfaces: SETTINGS_DIRECTORY (normal page, toolbar, library dialog,
// backup manager) and every EXPERT_TABS row, filtered like the expert menu's own search
// (renderExpert()).
export function gsSettingsHits(q) {
  const out = [];
  for (const entry of SETTINGS_DIRECTORY) {
    if (settingsDirectoryHit(entry, q)) {
      out.push({ label: t(entry.label), sub: t(SETTINGS_AREA_LABEL[entry.area]), run: () => openSettingsDirectoryEntry(entry) });
    }
  }
  for (const tab of EXPERT_TABS) {
    for (const group of tab.groups) {
      for (const f of groupFlags(group)) {
        if (!gateOpen(f.gate)) continue;
        const labelKey = flagLabelKey(f.key);
        if (settingHit(f.key, labelKey, q)) out.push({ label: t(labelKey), sub: t(tab.title), run: () => jumpToExpertSetting(f.key, "flags") });
      }
      for (const kind of ["values", "selects", "dropdowns", "paths"]) {
        for (const key of group[kind] || []) {
          if (!gateOpen(EXPERT_DEFS[kind]?.[key]?.gate)) continue;
          const labelKey = EXPERT_LABEL_KEYS[kind](key);
          if (settingHit(key, labelKey, q)) out.push({ label: t(labelKey), sub: t(tab.title), run: () => jumpToExpertSetting(key, kind) });
        }
      }
    }
  }
  return out;
}

// The in-app help is EN/DE only (ui/help.js); helpCategories() falls back to English for
// the other languages, like the help dialog's search.
export function gsHelpHits(q) {
  const ql = helpNorm(q);
  const out = [];
  for (const cat of helpCategories()) {
    const hay = helpNorm([cat.title, cat.whatItIs, (cat.howTo || []).join(" "), cat.whatHappens || "", (cat.settings || []).join(" "), cat.tip || ""].join(" "));
    if (hay.includes(ql)) out.push({ label: cat.title, sub: helpSectionLabel(cat.section), run: () => gsOpenHelp(cat) });
  }
  return out;
}

// Opens the help dialog pre-filtered to the matched article, through the dialog's own
// search field/renderHelp().
export function gsOpenHelp(cat) {
  assignHelpQuery(cat.title);
  $("help-search").value = cat.title;
  assignHelpCollapsed(new Set());
  renderHelp();
  $("help").classList.remove("hidden");
}

export const GS_GROUPS = [
  { key: "prompts", headerKey: "globalSearchGroupPrompts", hits: gsPromptHits },
  { key: "history", headerKey: "globalSearchGroupHistory", hits: gsHistoryHits },
  { key: "attachments", headerKey: "globalSearchGroupAttachments", hits: gsAttachmentHits },
  { key: "views", headerKey: "globalSearchGroupViews", hits: gsViewHits },
  { key: "settings", headerKey: "globalSearchGroupSettings", hits: gsSettingsHits },
  { key: "help", headerKey: "globalSearchGroupHelp", hits: gsHelpHits },
];

// `groupKey` (e.g. "prompts", "settings") stamps a stable, language-independent
// marker on the row: tests/ui/smoke_test.py finds a specific source group by
// this attribute instead of the translated heading text.
export function gsGroupHead(headerKey, groupKey, clearAll) {
  const head = document.createElement("div");
  head.className = "gs-group-head";
  const label = document.createElement("span");
  label.textContent = t(headerKey);
  head.appendChild(label);
  if (groupKey) head.dataset.group = groupKey;
  if (clearAll) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "gs-clear-all";
    btn.textContent = t("gsRecentClearAll");
    btn.addEventListener("mousedown", (e) => e.preventDefault());
    btn.addEventListener("click", (e) => { e.stopPropagation(); gsClearRecent(); });
    head.appendChild(btn);
  }
  return head;
}

export function gsRow(label, sub, color, onActivate) {
  const row = document.createElement("div");
  row.className = "gs-row";
  row.setAttribute("role", "option");
  row.id = `gs-opt-${++gsRowSeq}`;
  if (color) row.style.setProperty("--row-color", color);
  const name = document.createElement("span");
  name.className = "gs-row-name";
  name.textContent = label;
  row.appendChild(name);
  if (sub) {
    const subEl = document.createElement("span");
    subEl.className = "gs-row-sub";
    subEl.textContent = sub;
    row.appendChild(subEl);
  }
  row.addEventListener("mousedown", (e) => e.preventDefault()); // keep focus in the search field
  row.addEventListener("click", onActivate);
  return row;
}

// A recent-search row: the shared recent-searches row, with the nav classes this overlay
// needs (its own id for the active descendant, gs-recent-row / gs-recent-del hooks).
export function gsRecentRow(term) {
  return recentRow(term, {
    id: `gs-opt-${++gsRowSeq}`,
    rowClass: "gs-recent-row",
    delClass: "gs-recent-del",
    onPick: gsPickRecent,
    onForget: gsForgetRecent,
  });
}

function gsPickRecent(term) {
  $("gs-input").value = term;
  gsQuery = term;
  gsRender();
}

// Deleting a term keeps keyboard focus on a delete button at (or near) the
// same position so repeated Tab + Enter can clear several terms in a row.
export function gsForgetRecent(term) {
  const idx = recentList().findIndex((x) => x.toLowerCase() === term.toLowerCase());
  forgetSearch(term);
  gsRender();
  const dels = $("gs-results").querySelectorAll(".gs-recent-del");
  if (dels.length) dels[Math.max(0, Math.min(idx, dels.length - 1))].focus();
  else $("gs-input").focus();
}

export function gsClearRecent() {
  clearSearches();
  gsRender();
  $("gs-input").focus();
}

export function gsSetActive(i) {
  gsActive = i;
  gsFlatRows.forEach((r, idx) => {
    r.el.classList.toggle("active", idx === i);
    r.el.setAttribute("aria-selected", idx === i ? "true" : "false");
  });
  // Focus stays in the search field, so the highlighted row is announced through the active descendant.
  const input = $("gs-input");
  const cur = gsFlatRows[i]?.el;
  if (cur) input.setAttribute("aria-activedescendant", cur.id);
  else input.removeAttribute("aria-activedescendant");
  gsFlatRows[i]?.el.scrollIntoView({ block: "nearest" });
}

// Picking a result closes the overlay BEFORE running its action: every action
// (copy, switch view, open a dialog) wants the overlay itself out of the way
// first, same order the launcher uses for its own select().
export function gsActivate(hit) {
  rememberSearch(gsQuery);
  closeGlobalSearch();
  hit.run();
}

export function gsRender() {
  const box = $("gs-results");
  const empty = $("gs-empty");
  box.innerHTML = "";
  gsFlatRows = [];
  const q = gsQuery.trim();
  if (q.length < 2) {
    const list = recentList();
    if (!list.length) {
      empty.textContent = t("globalSearchHint");
      empty.classList.remove("hidden");
      gsSetActive(-1);
      return;
    }
    empty.classList.add("hidden");
    box.appendChild(gsGroupHead("globalSearchRecentLabel", "recent", true));
    for (const term of list) {
      const run = () => gsPickRecent(term);
      const row = gsRecentRow(term);
      box.appendChild(row);
      gsFlatRows.push({ el: row, run });
    }
    gsSetActive(gsFlatRows.length ? 0 : -1);
    return;
  }
  const limit = val("searchListMax");
  let any = false;
  for (const group of GS_GROUPS) {
    const hits = group.hits(q);
    if (!hits.length) continue;
    any = true;
    box.appendChild(gsGroupHead(group.headerKey, group.key));
    const expanded = gsExpanded.has(group.key);
    const shown = expanded ? hits : hits.slice(0, limit);
    for (const hit of shown) {
      const row = gsRow(hit.label, hit.sub, hit.color, () => gsActivate(hit));
      box.appendChild(row);
      gsFlatRows.push({ el: row, run: () => gsActivate(hit) });
    }
    if (!expanded && hits.length > limit) {
      const run = () => { gsExpanded.add(group.key); gsRender(); };
      const row = gsRow(t("globalSearchMore").replace("{n}", String(hits.length - limit)), "", "", run);
      row.classList.add("gs-more-row");
      box.appendChild(row);
      gsFlatRows.push({ el: row, run });
    }
  }
  empty.classList.toggle("hidden", any);
  if (!any) empty.textContent = t("globalSearchNoResults");
  gsSetActive(gsFlatRows.length ? 0 : -1);
  gsRefreshImageHits();
}
// True from a keystroke until its render ran, so Enter can render first instead of
// acting on the rows of the previous query.
let gsRenderPending = false;
export const gsRenderDebounced = debounced(() => { gsRenderPending = false; gsRender(); });

export async function openGlobalSearch() {
  if (gsOpen) return;
  gsOpen = true;
  gsQuery = "";
  gsExpanded = new Set();
  gsResetImageHits();
  $("gs-input").value = "";
  try { gsHistoryLog = await invoke("recent_copies", { limit: 500, grouped: false }); } catch { gsHistoryLog = []; }
  // A toggle or Escape during the fetch already closed it: showing it now would leave an
  // overlay on screen that every close path ignores (they all check gsOpen).
  if (!gsOpen) return;
  gsRender();
  $("global-search").classList.remove("hidden");
  $("gs-input").focus();
}

export function closeGlobalSearch() {
  if (!gsOpen) return;
  gsOpen = false;
  $("global-search").classList.add("hidden");
}

export function toggleGlobalSearch() {
  if (gsOpen) closeGlobalSearch();
  else openGlobalSearch();
}

export function bindGlobalSearch() {
  const gsEl = $("global-search");
  $("global-search-btn").addEventListener("click", toggleGlobalSearch);
  $("gs-close").addEventListener("click", closeGlobalSearch);
  gsEl.addEventListener("pointerdown", (e) => { if (e.target === gsEl) closeGlobalSearch(); });
  $("gs-input").addEventListener("input", (e) => { gsQuery = e.target.value; gsRenderPending = true; gsRenderDebounced(); });
  $("gs-input").addEventListener("keydown", (e) => {
    if (e.key === "Escape") { e.preventDefault(); closeGlobalSearch(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); if (gsFlatRows.length) gsSetActive(Math.min(gsActive + 1, gsFlatRows.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); if (gsFlatRows.length) gsSetActive(Math.max(gsActive - 1, 0)); }
    else if (e.key === "Enter") {
      e.preventDefault();
      if (gsRenderPending) { gsRenderDebounced.cancel(); gsRenderPending = false; gsRender(); }
      const row = gsFlatRows[gsActive];
      if (row) row.run();
    }
  });
  document.addEventListener("keydown", (e) => {
    if (!flag("globalSearch")) return;
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "k") {
      e.preventDefault();
      toggleGlobalSearch();
    }
  });
}
