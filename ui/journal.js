// The copy history and the clipboard inbox.
import { $, assignLibraryTab, assignSettings, debounced, errToast, flag, ICON_EDIT, ICON_TRASH, invoke, journalThumbCache, LANG, libraryEl, libraryTab, listen, makeKeyboardActivatable, optFlag, prompts, putIcon, settings, t, toast, undoToast, val } from "./core.js";
import { confirmDialog } from "./dialog.js";
import { _journalObs, assignActiveVidObs, assignJournalObs, buildColorFilter, copyResolved, hideLibrary, lastEdited, libLabel, libSelectMode, makeVideoObserver, maybeBackfillMediaMeta, mediaThumb, mediaTipLine, paintTypeChips, promptType, renderLibrary, setActiveFilter, setLibSelectMode, tagMatches, thumbEl, toastCopied, toggleTypeFilter, typeAllowed, typeFilterOf, usedColors } from "./library.js";
import { allTags, renderTagBar, tagChipEl, tagKey, tagsOn } from "./tags.js";
import { JOURNAL_ROWS_MAX } from "./expertConfig.js";
import { copyOnCooldown, recordCopy } from "./copyFlow.js";
import { wireSearchSuggest } from "./windowChrome.js";
import { varPreview } from "./floatShared.js";
import { applyFlags } from "./expertMenu.js";
import { editPrompt, openModal } from "./editor.js";

export let journalQuery = ""; // copy-history search text
export const journalTypes = new Set(); // additive, like the library's
export let journalColor = "all"; // copy-history color filter
export let journalFav = false; // copy-history "favorites only" filter
export const journalTags = new Set(); // same idea as libTags/gridTags, own selection

// ---- Copies from other apps (the stored clipboard entries, listed in the copy history) ----
// Undo for clearing the copies from other apps: re-inserts the rows clip_inbox_clear
// just removed, oldest first, through the same capped insert path a new watcher hit uses.
export async function restoreClipInbox(items) {
  if (!items || !items.length) return;
  try {
    await invoke("restore_clip_inbox", { items: [...items].reverse().map((it) => ({ ts: it.ts, text: it.text })) });
  } catch (err) { errToast(err); return; }
  await refreshClipInbox();
}

// Switching the collector off leaves whatever it gathered in place, still listed in the
// copy history: ask once whether to keep it. Closing the dialog keeps it.
export async function offerClipInboxCleanup() {
  const items = await invoke("clip_inbox_list").catch(() => []);
  if (!items.length) return;
  const drop = await confirmDialog({
    title: t("flagClipWatcher"),
    message: t("clipInboxKeepAsk"),
    confirmLabel: t("delete"),
    cancelLabel: t("clipInboxKeep"),
  });
  if (!drop) return;
  try { await invoke("clip_inbox_clear"); } catch (e) { errToast(e); return; }
  toast(t("clipInboxClearedToast"));
}

// Something changed in the stored entries: reload the copy history if it is on screen.
export async function refreshClipInbox() {
  if (isHistoryTabOpen()) { await loadJournal(); return; }
  clipStored = (await invoke("clip_inbox_list").catch(() => [])).length;
}

// One history entry, built like a library row on purpose: same thumbnail, same name
// and text lines, same tag chips. What the history adds is the meta line: when it
// was copied on the left, how often on the right: and a chip naming the type for
// everything that is not plain text.
export function journalRow(p, meta, onClick) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "lib-item journal-item";
  row.title = t("copy");
  const rowMedia = mediaTipLine(p);
  if (rowMedia) row.dataset.tipMedia = rowMedia;
  const body = document.createElement("span");
  body.className = "lib-body";
  const head = document.createElement("span");
  head.className = "journal-name-row";
  const name = document.createElement("span");
  name.className = "lib-name";
  name.textContent = libLabel(p);
  head.append(name);
  const kind = promptType(p);
  if (kind !== "text") {
    const chip = document.createElement("span");
    chip.className = "journal-kind";
    chip.textContent = t(`filter${kind[0].toUpperCase()}${kind.slice(1)}`);
    head.append(chip);
  }
  body.append(head);
  const preview = p.file_path || varPreview(p.text);
  if (preview) {
    const text = document.createElement("span");
    text.className = "lib-text";
    text.textContent = preview;
    body.append(text);
  }
  if (tagsOn() && (p.tags || []).length) {
    const tags = document.createElement("span");
    tags.className = "lib-tags";
    for (const tn of p.tags) tags.appendChild(tagChipEl(tn));
    body.append(tags);
  }
  if (meta && (meta.left || meta.right)) {
    const sub = document.createElement("span");
    sub.className = "lib-sub lib-subrow journal-meta";
    const l = document.createElement("span");
    l.textContent = meta.left || "";
    const r = document.createElement("span");
    r.className = "lib-sub-right";
    r.textContent = meta.right || "";
    sub.append(l, r);
    body.append(sub);
  }
  row.append(mediaThumb(p, thumbEl(p, journalThumbCache)), body);
  maybeBackfillMediaMeta(p, renderJournal);
  row.addEventListener("click", onClick);
  // Edit straight from the history: same icon/pattern as the library row.
  const edit = document.createElement("span");
  edit.className = "icon-btn lib-edit";
  edit.title = t("edit");
  putIcon(edit, ICON_EDIT);
  edit.addEventListener("pointerdown", (e) => e.stopPropagation());
  edit.addEventListener("click", (e) => { e.stopPropagation(); editPrompt(p.id); });
  makeKeyboardActivatable(edit);
  row.appendChild(edit);
  // Only the "recently copied" column carries a rowid: "most used" is a
  // usage-count aggregate with no single copy_log row a delete could target.
  if (meta && meta.rowid != null) {
    const del = document.createElement("span");
    del.className = "icon-btn journal-delete";
    del.title = t("delete");
    del.setAttribute("aria-label", t("delete"));
    putIcon(del, ICON_TRASH);
    del.addEventListener("pointerdown", (e) => e.stopPropagation());
    del.addEventListener("click", async (e) => {
      e.stopPropagation();
      const ok = await confirmDialog({
        title: t("delete"),
        message: t("journalEntryDeleteConfirm"),
        confirmLabel: t("delete"),
      });
      if (!ok) return;
      try { await invoke("delete_copy_entry", { rowid: meta.rowid }); } catch (err) { errToast(err); return; }
      await loadJournal(); // refetch: the entry (and only it) disappears
    });
    makeKeyboardActivatable(del);
    row.appendChild(del);
  }
  return row;
}

// Format a unix-seconds copy timestamp for the history column.
export const fmtCopyTime = (ts) => {
  if (!ts) return "";
  return new Date(ts * 1000).toLocaleString(LANG, { dateStyle: "short", timeStyle: "short" });
};

// The "last copied" / "last edited" lines a tile or library row's tooltip
// shows, each hanging off the function that actually owns that data -- the copy
// timestamp off the same settings.last_used map smartSort reads, the edit
// timestamp off promptHistory's lastEdited map -- so a value missing because its
// own feature is off (or just never happened yet) drops only its own line.
export function tileTimeLines(id) {
  const lines = [];
  const copied = settings.last_used?.[id];
  if (copied) lines.push(t("tipLastCopied").replace("{time}", fmtCopyTime(copied)));
  const edited = lastEdited[id];
  if (edited) lines.push(t("tipLastEdited").replace("{time}", fmtCopyTime(edited)));
  return lines.join("\n");
}

export function journalMatches(p) {
  if (!typeAllowed(journalTypes, p)) return false;
  if (journalFav && flag("favorites") && !p.favorite) return false;
  if (journalColor !== "all" && (p.color || "") !== journalColor) return false;
  if (tagsOn() && !tagMatches(p, journalTags)) return false;
  const q = journalQuery.trim().toLowerCase();
  if (!q) return true;
  return [p.name, p.text, p.file_path].some((s) => (s || "").toLowerCase().includes(q));
}

export function setJournalColor(color) {
  journalColor = color;
  setActiveFilter($("journal-colors"), ".lib-color", "color", color);
}

export let journalLog = []; // cached newest copy events; refetched only on open/group/copy
export let journalInbox = []; // text copied in other apps (the stored clipboard entries), newest first
export let clipStored = 0; // how many of those are stored: the history tab stays reachable while any are
export let journalSource = "all"; // source filter of the copy history: "all" | "app" | "elsewhere"
export let journalRecentRev = false; // recently-copied sorted oldest-first
export let journalUsedRev = false; // most-used sorted least-first

// Refetch the copy log (the query changes: open, grouped toggle, after a copy).
export async function loadJournal() {
  const grouped = flag("historyGrouped");
  journalLog = await invoke("recent_copies", { limit: Math.min(val("historyMax"), JOURNAL_ROWS_MAX), grouped }).catch(() => []);
  journalInbox = await invoke("clip_inbox_list").catch(() => []);
  clipStored = journalInbox.length;
  renderJournal();
}

// Undo for "clear copy history": writes the row-level log back plus the two
// counter maps clear_copy_history wipes alongside it.
export async function restoreCopyHistory(rows, usage, lastUsed) {
  try {
    await invoke("restore_copy_history", {
      entries: rows.map((r) => ({ ts: r.ts, id: r.id })),
      usage,
      lastUsed,
    });
  } catch (err) { errToast(err); return; }
  settings.usage = usage;
  settings.last_used = lastUsed;
  await loadJournal();
}

// Same bar as the library's (renderTagBar), with the journal's own selection.
export let _lastJournalTagFilterSig = null;
export function renderJournalTagFilter() {
  const bar = $("journal-tags");
  if (!bar) return;
  if (!tagsOn()) {
    bar.innerHTML = "";
    bar.classList.add("hidden");
    _lastJournalTagFilterSig = null;
    journalTags.clear();
    return;
  }
  const names = allTags();
  const live = new Set(names.map(tagKey));
  for (const key of [...journalTags]) if (!live.has(key)) journalTags.delete(key);
  // Same as the library's own bar: "manage tags" stays reachable even with
  // zero tags, so this no longer hides on an empty tag set.
  bar.classList.remove("hidden");
  const sig = LANG + "|" + names.join("") + "" + [...journalTags].sort().join("");
  if (sig === _lastJournalTagFilterSig) return;
  _lastJournalTagFilterSig = sig;
  renderTagBar(bar, journalTags, renderJournal);
}

// One entry copied in another app: time, a two-line text preview and the three
// actions (save as a prompt, copy again, dismiss). Shown inside the copy history.
export function journalClipRow(it) {
  const row = document.createElement("div");
  row.className = "clip-row";
  const time = document.createElement("span");
  time.className = "clip-time";
  time.textContent = new Date(it.ts * 1000).toLocaleString(LANG);
  const prev = document.createElement("div");
  prev.className = "clip-preview";
  prev.textContent = it.text;
  const actions = document.createElement("div");
  actions.className = "clip-actions";
  const save = document.createElement("button");
  save.type = "button"; save.className = "ghost-btn"; save.textContent = t("clipSave");
  save.addEventListener("click", () => {
    hideLibrary();
    openModal({ mode: "create", text: it.text, title: t("nameModalTitle") });
  });
  const copy = document.createElement("button");
  copy.type = "button"; copy.className = "ghost-btn"; copy.textContent = t("clipCopy");
  copy.addEventListener("click", () => { invoke("copy_text", { text: it.text }).then(() => toast(t("copied"))).catch((e) => errToast(e)); });
  const dismiss = document.createElement("button");
  dismiss.type = "button"; dismiss.className = "ghost-btn"; dismiss.textContent = t("clipDismiss");
  dismiss.addEventListener("click", async () => { await invoke("clip_inbox_dismiss", { id: it.id }).catch((e) => errToast(e)); refreshClipInbox(); });
  actions.append(save, copy, dismiss);
  row.append(time, prev, actions);
  return row;
}

// The source chips and the two buttons that only concern copies from other apps
// (pause/resume recording, purge from backups) show together, and only while the
// recorder is on or entries are still stored.
export function syncJournalSourceBar(on) {
  $("journal-source").classList.toggle("hidden", !on);
  $("clip-toggle").classList.toggle("hidden", !on);
  $("clip-purge-backups").classList.toggle("hidden", !on);
  $("clip-toggle").classList.toggle("active", optFlag("clipWatcher"));
  for (const b of $("journal-source").querySelectorAll(".lib-type")) {
    b.classList.toggle("active", b.dataset.source === journalSource);
  }
}

// Render from the cached log. Filters run client-side, so typing in the search
// box re-renders instantly without an IPC roundtrip (and without a render race).
export function renderJournal() {
  renderJournalTagFilter();
  const body = $("journal-body");
  if (_journalObs) _journalObs.disconnect();
  assignJournalObs(makeVideoObserver());
  assignActiveVidObs(_journalObs);
  const byId = new Map(prompts.map((p) => [p.id, p]));
  const colorsUsed = usedColors(journalLog.map((e) => byId.get(e.id)).filter(Boolean));
  if (journalColor !== "all" && !colorsUsed.includes(journalColor)) setJournalColor("all");
  buildColorFilter($("journal-colors"), journalColor, colorsUsed);
  const grouped = flag("historyGrouped");
  const showTime = flag("historyTimestamps");
  // Copies from other apps share the list when the recorder is on or entries are still
  // stored; the source chips (and the buttons that only concern those entries) follow.
  const sourcesOn = optFlag("clipWatcher") || journalInbox.length > 0;
  if (!sourcesOn) journalSource = "all";
  syncJournalSourceBar(sourcesOn);
  const showApp = journalSource !== "elsewhere";
  const showElsewhere = sourcesOn && journalSource !== "app";
  let recent = showApp
    ? journalLog
      .map((e) => [byId.get(e.id), e.ts, e.rowid])
      .filter(([p]) => p && journalMatches(p))
      .map(([p, ts, rowid]) => ({ p, ts, rowid }))
    : [];
  // An entry copied elsewhere has no type, colour, tag or favourite, so any of
  // those filters being narrowed leaves it out; the search matches its text.
  const narrowed = journalTypes.size > 0 || journalColor !== "all" || journalFav || journalTags.size > 0;
  const clipQuery = journalQuery.trim().toLowerCase();
  if (showElsewhere && !narrowed) {
    for (const it of journalInbox) {
      if (!clipQuery || String(it.text || "").toLowerCase().includes(clipQuery)) recent.push({ clip: it, ts: it.ts });
    }
  }
  // Newest first; the sort is stable, so equal timestamps keep the copy log first.
  recent.sort((a, b) => (b.ts || 0) - (a.ts || 0));
  if (journalRecentRev) recent.reverse();
  // How hard the filters are biting, in the header where the filters are:
  // same wording/element as the library, counted against entries with a
  // still-existing prompt (a log row for a deleted prompt is not "all").
  const jcount = $("journal-count");
  if (jcount) {
    const all = (showApp ? journalLog.filter((e) => byId.get(e.id)).length : 0) +
      (showElsewhere ? journalInbox.length : 0);
    jcount.textContent = recent.length === all
      ? String(all)
      : t("libraryCount").replace("{n}", recent.length).replace("{v}", all);
    const tabCount = $("library-tab-history-count");
    if (tabCount) tabCount.textContent = `(${jcount.textContent})`;
  }
  // Most used: copy count + last-used timestamp, busiest first (or least-first).
  const lastUsed = settings.last_used || {};
  const used = (journalSource === "elsewhere" ? [] : Object.entries(settings.usage || {}))
    .map(([id, n]) => [byId.get(id), n, lastUsed[id] || 0])
    .filter(([p]) => p && journalMatches(p))
    .sort((a, b) => (journalUsedRev ? a[1] - b[1] : b[1] - a[1]))
    .slice(0, 20);
  body.innerHTML = "";
  const copyAgain = (p) => async () => {
    if (copyOnCooldown(p.id)) return; // same prompt copied moments ago
    if (await copyResolved(p)) { // handles {{variables}} like the library does
      await recordCopy(p.id); // await: the log must refetch AFTER the copy is recorded
      toastCopied(p);
      assignSettings(await invoke("get_settings"));
      await loadJournal();
    }
  };
  // `rows` items are [prompt, meta]. `extra` is an optional control rendered to the
  // right of the column title.
  const column = (titleKey, rows, extra, cls, emptyKey, rowEl) => {
    const col = document.createElement("div");
    col.className = "journal-col" + (cls ? " " + cls : "");
    const head = document.createElement("div");
    head.className = "lib-section journal-head";
    const title = document.createElement("span");
    title.textContent = t(titleKey);
    head.append(title);
    if (extra) head.append(extra);
    col.appendChild(head);
    if (!rows.length) {
      const e = document.createElement("div");
      e.className = "hint";
      e.textContent = t(emptyKey || "journalEmpty");
      col.appendChild(e);
    } else {
      for (const row of rows) col.appendChild(rowEl ? rowEl(row) : journalRow(row[0], row[1], copyAgain(row[0])));
    }
    return col;
  };
  // Reverse-sort arrow for a column title (▼ newest/most first, ▲ reversed).
  const sortBtn = (rev, onClick) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "journal-sort" + (rev ? " rev" : "");
    b.textContent = rev ? "▲" : "▼";
    b.title = t("sortToggle");
    b.setAttribute("aria-label", t("sortToggle"));
    b.addEventListener("click", onClick);
    return b;
  };
  const headExtras = (...nodes) => {
    const s = document.createElement("span");
    s.className = "journal-head-extras";
    for (const n of nodes) if (n) s.append(n);
    return s;
  };
  // Toggle beside the "recently copied" title: group repeats vs show every copy.
  const groupBtn = document.createElement("button");
  groupBtn.type = "button";
  groupBtn.className = "journal-toggle" + (grouped ? " active" : "");
  groupBtn.textContent = t("journalGrouped");
  groupBtn.title = t("journalGrouped");
  groupBtn.setAttribute("aria-pressed", String(grouped));
  groupBtn.addEventListener("click", async () => {
    const next = !grouped;
    settings.ui_flags = { ...(settings.ui_flags || {}), historyGrouped: next };
    try { await invoke("set_ui_flag", { key: "historyGrouped", enabled: next }); } catch (err) { errToast(err); }
    await loadJournal(); // grouping changes the query → refetch
  });
  const recentSort = sortBtn(journalRecentRev, () => { journalRecentRev = !journalRecentRev; renderJournal(); });
  const usedSort = sortBtn(journalUsedRev, () => { journalUsedRev = !journalUsedRev; renderJournal(); });
  // "Most used" only knows prompts of the app, so it steps aside for the elsewhere filter.
  body.classList.toggle("journal-single", journalSource === "elsewhere");
  body.append(
    column("recentlyCopied",
      // Rowid rides along so journalRow() can offer a per-entry delete:
      // "most used" (below) has no single copy_log row to point at, so it stays
      // without one.
      recent,
      headExtras(groupBtn, recentSort), "journal-col-recent",
      journalSource === "elsewhere" ? "clipEmpty" : "journalEmpty",
      (e) => (e.clip
        ? journalClipRow(e.clip)
        : journalRow(e.p, { left: showTime ? fmtCopyTime(e.ts) : "", rowid: e.rowid }, copyAgain(e.p)))),
    // One meta line: last-used time on the left, copy count on the right.
    ...(journalSource === "elsewhere" ? [] : [column("mostUsed", used.map(([p, n, ts]) => [
      p,
      { left: showTime && ts ? fmtCopyTime(ts) : "", right: t("usedTimes").replace("{n}", n) },
    ]), headExtras(usedSort), "journal-col-used")]),
  );
}

export const isHistoryTabOpen = () => !libraryEl.classList.contains("hidden") && libraryTab === "history";
// The copy history tab exists while it records something (app copies or copies from other
// apps) and while stored entries from other apps remain, so they can still be seen,
// dismissed and purged from every backup after the recorder was switched off.
export const historyTabAvailable = () => flag("copyHistory") || optFlag("clipWatcher") || clipStored > 0;

export function setLibraryTab(tab) {
  const avail = historyTabAvailable();
  if (tab === "history" && !avail) tab = "prompts";
  // The batch bar belongs to the prompts tab.
  if (tab === "history" && libSelectMode) setLibSelectMode(false);
  assignLibraryTab(tab);
  libraryEl.dataset.tab = tab;
  $("library-tabs").classList.toggle("hidden", !avail);
  $("library-panel-prompts").classList.toggle("hidden", tab !== "prompts");
  $("library-panel-history").classList.toggle("hidden", tab !== "history");
  for (const [id, key] of [["library-tab-prompts", "prompts"], ["library-tab-history", "history"]]) {
    $(id).classList.toggle("active", tab === key);
    $(id).setAttribute("aria-selected", String(tab === key));
  }
  if (tab === "history") resetAndLoadJournal();
  else renderLibrary();
}

export async function resetAndLoadJournal() {
  assignSettings(await invoke("get_settings"));
  journalQuery = "";
  if ($("journal-q")) $("journal-q").value = "";
  journalTypes.clear();
  for (const type of typeFilterOf("journalTypeFilterPick")) journalTypes.add(type);
  paintTypeChips($("journal-types"), journalTypes);
  journalFav = false;
  $("journal-fav").classList.remove("active");
  setJournalColor("all");
  journalSource = "all";
  // renderJournal decides what the filter bar shows, so it also follows the source chips.
  await loadJournal();
  if (flag("searchAutofocus")) $("journal-q").focus();
}

export function bindJournal() {
  // Pause/resume recording copies from other apps straight from the copy history: same
  // switch as the expert one.
  $("clip-toggle").addEventListener("click", async () => {
    const on = !optFlag("clipWatcher");
    settings.ui_flags = { ...(settings.ui_flags || {}), clipWatcher: on };
    let accepted = true;
    try { await invoke("set_ui_flag", { key: "clipWatcher", enabled: on }); }
    catch (e) { accepted = false; errToast(e); }
    applyFlags();
    if (!on && accepted) await offerClipInboxCleanup();
    await refreshClipInbox();
  });
  $("journal-source").addEventListener("click", (e) => {
    const btn = e.target.closest("[data-source]");
    if (!btn) return;
    journalSource = btn.dataset.source;
    renderJournal();
  });
  // Emergency wipe: clears the live inbox AND scrubs clip_inbox rows out of every
  // rotating backup snapshot. Irreversible and touches every backup, so it asks
  // twice: the second dialog spells out that it cannot be undone.
  $("clip-purge-backups").addEventListener("click", async () => {
    const step1 = await confirmDialog({
      title: t("clipPurgeBackupsTitle"),
      message: t("clipPurgeBackupsBody1"),
      confirmLabel: t("clipPurgeBackupsConfirm"),
      cancelLabel: t("cancel"),
      danger: true,
    });
    if (!step1) return;
    const step2 = await confirmDialog({
      title: t("clipPurgeBackupsTitle"),
      message: t("clipPurgeBackupsBody2"),
      confirmLabel: t("clipPurgeBackupsConfirm2"),
      cancelLabel: t("cancel"),
      danger: true,
    });
    if (!step2) return;
    await invoke("clip_inbox_clear").catch((e) => errToast(e));
    const result = await invoke("purge_clip_inbox_from_backups").catch((e) => { errToast(e); return null; });
    refreshClipInbox();
    if (result !== null) {
      const [purged, failed] = result;
      let msg = t("clipPurgeBackupsDone").replace("{n}", String(purged));
      // Some snapshot could not be opened or its scrub could not be confirmed
      // written back -- an emergency wipe reporting silent success there would
      // leave the user believing they are clean when they may not be.
      if (failed > 0) msg += `. ${t("clipPurgeBackupsFailed").replace("{n}", String(failed))}`;
      toast(msg);
    }
  });
  listen("clip-inbox-changed", () => refreshClipInbox());

  // Copy history & usage journal.
  const renderJournalDebounced = debounced(renderJournal);
  $("journal-q").addEventListener("input", (e) => { journalQuery = e.target.value; renderJournalDebounced(); });
  wireSearchSuggest($("journal-q"), "recentSearchJournal");
  $("journal-types").addEventListener("click", (e) => {
    if (e.target.closest("#journal-fav")) {
      journalFav = !journalFav;
      $("journal-fav").classList.toggle("active", journalFav);
      renderJournal();
      return;
    }
    const btn = e.target.closest(".lib-type");
    if (btn) { toggleTypeFilter(journalTypes, btn.dataset.type, $("journal-types"), "journalTypeFilterPick"); renderJournal(); }
  });
  $("journal-colors").addEventListener("click", (e) => {
    const btn = e.target.closest(".lib-color");
    if (btn) { setJournalColor(btn.dataset.color); renderJournal(); }
  });
  // Clears exactly what the source filter shows: "all" the copy log and the copies from
  // other apps, "app" the copy log only, "elsewhere" the stored entries from other apps
  // only. The confirm names the counts of every store it is about to empty; an empty
  // target does nothing at all (no toast, no undo), and a failed clear stops right there.
  $("journal-clear").addEventListener("click", async () => {
    const sourcesOn = optFlag("clipWatcher") || journalInbox.length > 0;
    const source = sourcesOn ? journalSource : "all";
    const clearApp = source !== "elsewhere";
    const clearElsewhere = source !== "app";
    // Snapshot everything the clears are about to wipe, so Undo can write it all
    // back: the row-level log (reused via the same call the journal itself uses to
    // fill its list) plus the two counter maps, and the stored entries.
    let rows = [];
    let items = [];
    const usage = { ...(settings.usage || {}) };
    const lastUsed = { ...(settings.last_used || {}) };
    try {
      if (clearApp) rows = await invoke("recent_copies", { limit: 1000000, grouped: false });
      if (clearElsewhere) items = await invoke("clip_inbox_list");
    } catch (e) { errToast(e); return; }
    const appAffected = clearApp && (rows.length > 0 || Object.keys(usage).length > 0 || Object.keys(lastUsed).length > 0);
    const clipAffected = clearElsewhere && items.length > 0;
    if (!appAffected && !clipAffected) return;
    if (flag("confirmClearHistory")) {
      const message = appAffected && clipAffected
        ? t("clearHistoryConfirmBoth").replace("{n}", String(rows.length)).replace("{m}", String(items.length))
        : appAffected ? t("clearHistoryConfirm") : t("clearClipConfirm").replace("{m}", String(items.length));
      const ok = await confirmDialog({ title: t("clearHistory"), message, confirmLabel: t("clearHistory") });
      if (!ok) return;
    }
    if (appAffected) {
      try { await invoke("clear_copy_history"); } catch (e) { errToast(e); return; }
      settings.usage = {};
      settings.last_used = {};
    }
    if (clipAffected) {
      try { await invoke("clip_inbox_clear"); } catch (e) { errToast(e); items = []; if (!appAffected) return; }
    }
    await loadJournal();
    undoToast(t(appAffected ? "historyClearedToast" : "clipInboxClearedToast"), async () => {
      if (appAffected) await restoreCopyHistory(rows, usage, lastUsed);
      await restoreClipInbox(items);
    });
  });
}
