// The copy history and the clipboard inbox.
import { $, assignLibraryTab, assignSettings, debounced, errToast, errUndoToast, flag, ICON_ADD_SQUARE, ICON_EDIT, ICON_TRASH, invoke, journalThumbCache, LANG, libraryEl, libraryTab, listen, makeKeyboardActivatable, optFlag, prompts, putIcon, settings, t, toast, undoToast, val } from "./core.js";
import { confirmDialog } from "./dialog.js";
import { attachSelectPicker } from "./selectpop.js";
import { _journalObs, assignActiveVidObs, assignJournalObs, copyResolved, hideLibrary, lastEdited, libLabel, libSelectMode, makeVideoObserver, maybeBackfillMediaMeta, mediaThumb, mediaTipLine, paintTypeChips, promptType, renderLibrary, setLibSelectMode, thumbEl, toastCopied, toggleTypeFilter, typeAllowed, typeFilterOf } from "./library.js";
import { JOURNAL_ROWS_MAX } from "./expertConfig.js";
import { copyOnCooldown, recordCopy } from "./copyFlow.js";
import { wireSearchSuggest } from "./windowChrome.js";
import { dynLocale } from "./promptVars.js";
import { applyFlags } from "./expertMenu.js";
import { notesOn } from "./tiles.js";
import { tagsOn } from "./tags.js";
import { editPrompt, openModal } from "./editor.js";

export let journalQuery = ""; // copy-history search text
export const journalTypes = new Set(); // additive, like the library's

// ---- Copies from other apps (the stored clipboard entries, listed in the copy history) ----
// Undo for clearing the copies from other apps: re-inserts the rows clip_inbox_clear
// just removed, oldest first, through the same capped insert path a new watcher hit uses.
export async function restoreClipInbox(items) {
  if (!items || !items.length) return;
  try {
    await invoke("restore_clip_inbox", { items: [...items].reverse().map((it) => ({ id: it.id, ts: it.ts, text: it.text })) });
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
  clipStored = Number(await invoke("clip_inbox_count").catch(() => 0)) || 0;
}

// One history entry on a single line: the same thumbnail as a library row, the name, a
// chip naming the type for everything that is not plain text (except a video whose
// thumbnail already shows its duration), and on the right when it
// was copied (plus how often in the most-used list). The preview text and the full time
// ride in the tooltip.
export function journalRow(p, meta, onClick) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "lib-item journal-item";
  row.dataset.tipName = libLabel(p);
  const preview = p.file_path || p.text || "";
  if (preview && preview !== row.dataset.tipName) row.dataset.tipBody = preview;
  if (meta && meta.title) row.dataset.tipTimes = meta.title;
  row.dataset.tipHint = t("journalRowHint");
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
  const showsDuration = kind === "video" && flag("mediaMeta") && !!p.media_duration_ms;
  if (kind !== "text" && !showsDuration) {
    const chip = document.createElement("span");
    chip.className = "journal-kind";
    chip.textContent = t(`filter${kind[0].toUpperCase()}${kind.slice(1)}`);
    head.append(chip);
  }
  if (meta && meta.left) {
    const when = document.createElement("span");
    when.className = "journal-when";
    when.textContent = meta.left;
    head.append(when);
  }
  if (meta && meta.right) {
    const count = document.createElement("span");
    count.className = "lib-sub-right";
    count.textContent = meta.right;
    head.append(count);
  }
  body.append(head);
  row.append(mediaThumb(p, thumbEl(p, journalThumbCache)), body);
  maybeBackfillMediaMeta(p, renderJournal);
  row.addEventListener("click", onClick);
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
      // The command answers false (it does not reject) when the DELETE failed.
      let deleted = false;
      try { deleted = await invoke("delete_copy_entry", { rowid: meta.rowid }); } catch (err) { errToast(err); return; }
      if (!deleted) { toast(t("persistFailed")); return; }
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

// The history's time wording: today reads relative ("5 minutes ago", "2 hours ago"), any
// other day the usual short date and time. The full stamp rides along as a tooltip.
export const fmtWhen = (ts) => {
  if (!ts) return "";
  const then = new Date(ts * 1000);
  const now = new Date();
  if (then.toDateString() !== now.toDateString()) return fmtCopyTime(ts);
  const mins = Math.max(0, Math.round((now - then) / 60000));
  let rtf;
  try { rtf = new Intl.RelativeTimeFormat(dynLocale(), { numeric: "auto" }); } catch (_) { rtf = new Intl.RelativeTimeFormat("en", { numeric: "auto" }); }
  return mins < 60 ? rtf.format(-mins, "minute") : rtf.format(-Math.floor(mins / 60), "hour");
};
const fmtFull = (ts) => (ts ? new Date(ts * 1000).toLocaleString(LANG) : "");

// The "last copied" / "last edited" lines of a tile or library row tooltip, each hanging
// off the function that owns that data (the copy timestamp off settings.last_used, the
// edit timestamp off promptHistory's lastEdited), so a value missing because its feature
// is off drops only its own line.
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
  const q = journalQuery.trim().toLowerCase();
  if (!q) return true;
  // The same fields the library search reads: every rotation entry, and tags and notes while
  // those features are on.
  const fields = [p.name, p.text, p.file_path, ...(p.variants || [])];
  if (tagsOn()) fields.push((p.tags || []).join(" "));
  if (notesOn()) fields.push(p.notes);
  return fields.some((s) => (s || "").toLowerCase().includes(q));
}

export let journalLog = []; // cached newest copy events; refetched only on open/group/copy
export let journalInbox = []; // text copied in other apps (the stored clipboard entries), newest first
export let clipStored = 0; // how many of those are stored: the history tab stays reachable while any are
export let journalSource = "all"; // source filter of the copy history: "all" | "app" | "elsewhere"
export let journalRange = "all"; // time range of the "recently copied" list: "all" | "today" | "7" | "30"
// Start of the chosen time range in unix seconds (0 = no limit). Entries without a stamp count as 0.
function journalRangeStart() {
  if (journalRange === "today") return new Date().setHours(0, 0, 0, 0) / 1000;
  if (journalRange === "7" || journalRange === "30") return Date.now() / 1000 - Number(journalRange) * 86400;
  return 0;
}
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

// One entry copied in another app: time, a two-line text preview and two icons (save as a
// prompt, dismiss). The row copies the text again; shown inside the copy history.
export function journalClipRow(it) {
  const row = document.createElement("div");
  row.className = "clip-row journal-clip";
  row.dataset.tipName = t("sourceElsewhere");
  row.dataset.tipBody = it.text;
  row.dataset.tipTimes = fmtFull(it.ts);
  row.dataset.tipHint = t("journalRowHint");
  const main = document.createElement("div");
  main.className = "clip-main";
  const time = document.createElement("span");
  time.className = "clip-time";
  time.textContent = fmtWhen(it.ts);
  time.title = fmtFull(it.ts);
  const prev = document.createElement("div");
  prev.className = "clip-preview";
  prev.textContent = it.text;
  main.append(prev, time);
  row.addEventListener("click", () => { invoke("copy_text", { text: it.text }).then(() => toast(t("copied"))).catch((e) => errToast(e)); });
  makeKeyboardActivatable(row);
  const actions = document.createElement("div");
  actions.className = "clip-actions";
  const icon = (svg, label, onClick) => {
    const b = document.createElement("span");
    b.className = "icon-btn";
    b.title = label;
    b.setAttribute("aria-label", label);
    putIcon(b, svg);
    b.addEventListener("pointerdown", (e) => e.stopPropagation());
    b.addEventListener("click", (e) => { e.stopPropagation(); onClick(); });
    makeKeyboardActivatable(b);
    return b;
  };
  const save = icon(ICON_ADD_SQUARE, t("clipSave"), () => {
    hideLibrary();
    openModal({ mode: "create", text: it.text, title: t("nameModalTitle") });
  });
  save.classList.add("clip-save");
  const saveLabel = document.createElement("span");
  saveLabel.textContent = t("clipSaveShort");
  save.append(saveLabel);
  actions.append(
    save,
    icon(ICON_TRASH, t("clipDismiss"), async () => {
      await invoke("clip_inbox_dismiss", { id: it.id }).catch((e) => errToast(e));
      refreshClipInbox();
    }),
  );
  row.append(main, actions);
  return row;
}

// The source field and the button that only concern copies from other apps
// (pause/resume recording) show together, and only while the
// recorder is on or entries are still stored.
export function syncJournalSourceBar(on) {
  $("journal-source").classList.toggle("hidden", !on);
  $("journal-source-field").classList.toggle("hidden", !on);
  $("clip-toggle").classList.toggle("hidden", !on);
  const watching = optFlag("clipWatcher");
  $("clip-toggle").classList.toggle("active", watching);
  $("clip-toggle").querySelector(".clip-toggle-label").textContent = t(watching ? "clipWatchOn" : "clipWatchOff");
  $("journal-source").value = journalSource;
}

// Render from the cached log. Filters run client-side, so typing in the search
// box re-renders instantly without an IPC roundtrip (and without a render race).
export function renderJournal() {
  const body = $("journal-body");
  if (_journalObs) _journalObs.disconnect();
  assignJournalObs(makeVideoObserver());
  assignActiveVidObs(_journalObs);
  const byId = new Map(prompts.map((p) => [p.id, p]));
  const grouped = flag("historyGrouped");
  const showTime = flag("historyTimestamps");
  // Copies from other apps share the list when the recorder is on or entries are still
  // stored; the source field (and the buttons that only concern those entries) follow.
  const sourcesOn = optFlag("clipWatcher") || journalInbox.length > 0;
  if (!sourcesOn) journalSource = "all";
  syncJournalSourceBar(sourcesOn);
  const showApp = journalSource !== "elsewhere";
  const showElsewhere = sourcesOn && journalSource !== "app";
  // "Most used" shows lifetime counts with no per-copy time, so the range steps aside there.
  const usedMode = journalSource !== "elsewhere" && journalSortMode() === "used";
  $("journal-range").classList.toggle("hidden", usedMode);
  const since = usedMode ? 0 : journalRangeStart();
  let recent = showApp
    ? journalLog
      .map((e) => [byId.get(e.id), e.ts, e.rowid])
      .filter(([p, ts]) => p && (ts || 0) >= since && journalMatches(p))
      .map(([p, ts, rowid]) => ({ p, ts, rowid }))
    : [];
  // An entry copied elsewhere has no type, so a type filter leaves it out; the search matches its text.
  const narrowed = journalTypes.size > 0;
  const clipQuery = journalQuery.trim().toLowerCase();
  if (showElsewhere && !narrowed) {
    for (const it of journalInbox) {
      if ((it.ts || 0) >= since && (!clipQuery || String(it.text || "").toLowerCase().includes(clipQuery))) recent.push({ clip: it, ts: it.ts });
    }
  }
  // Newest first; the sort is stable, so equal timestamps keep the copy log first.
  recent.sort((a, b) => (b.ts || 0) - (a.ts || 0));
  if (journalRecentRev) recent.reverse();
  // How hard the filters are biting, in the header where the filters are: counted against
  // entries with a still-existing prompt (a log row for a deleted prompt is not "all").
  const jcount = $("journal-count");
  if (jcount) {
    const all = (showApp ? journalLog.filter((e) => byId.get(e.id)).length : 0) +
      (showElsewhere ? journalInbox.length : 0);
    jcount.textContent = recent.length === all
      ? String(all)
      : t("libraryCount").replace("{n}", recent.length).replace("{v}", () => all);
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
    if (await copyResolved(p)) {
      await recordCopy(p.id); // await: the log must refetch AFTER the copy is recorded
      toastCopied(p);
      assignSettings(await invoke("get_settings"));
      await loadJournal();
    }
  };
  // Reverse-order button beside the sort field: an arrow and the words for the order the list
  // is in now (▼ newest/most first, ▲ reversed); the tooltip says what a click does.
  const sortBtn = (rev, onClick, labels) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "journal-sort" + (rev ? " rev" : "");
    b.textContent = (rev ? "▲ " : "▼ ") + t(rev ? labels[1] : labels[0]);
    b.title = t("sortToggle");
    b.setAttribute("aria-label", t("sortToggle"));
    b.addEventListener("click", onClick);
    return b;
  };
  // "Most used" only knows prompts of the app, so it steps aside for the elsewhere filter.
  const elsewhereOnly = journalSource === "elsewhere";
  const mode = elsewhereOnly ? "recent" : journalSortMode();
  const head = document.createElement("div");
  head.className = "lib-section journal-head";
  if (elsewhereOnly) {
    const title = document.createElement("span");
    title.textContent = t("recentlyCopied");
    head.append(title);
  } else {
    const sel = document.createElement("select");
    sel.id = "journal-sort";
    sel.className = "lib-sort";
    sel.setAttribute("aria-label", t("sortBy"));
    for (const [v, k] of [["recent", "recentlyCopied"], ["used", "mostUsed"]]) {
      const o = document.createElement("option");
      o.value = v; o.textContent = t(k);
      sel.appendChild(o);
    }
    sel.value = mode;
    sel.addEventListener("change", () => {
      settings.ui_texts = { ...(settings.ui_texts || {}), journalSort: sel.value };
      invoke("set_ui_text", { key: "journalSort", value: sel.value }).catch((err) => errToast(err));
      renderJournal();
      $("journal-sort")?.focus();
    });
    attachSelectPicker(sel);
    head.append(sel);
  }
  const extras = document.createElement("span");
  extras.className = "journal-head-extras";
  if (mode === "recent") {
    // Toggle beside the sort field: group repeats vs show every copy.
    const groupLbl = document.createElement("label");
    groupLbl.className = "head-check";
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.id = "journal-grouped";
    cb.checked = grouped;
    const sp = document.createElement("span");
    sp.textContent = t("journalGrouped");
    groupLbl.append(cb, sp);
    cb.addEventListener("change", async () => {
      const next = cb.checked;
      settings.ui_flags = { ...(settings.ui_flags || {}), historyGrouped: next };
      try { await invoke("set_ui_flag", { key: "historyGrouped", enabled: next }); } catch (err) { errToast(err); }
      await loadJournal(); // grouping changes the query → refetch
    });
    extras.append(groupLbl, sortBtn(journalRecentRev, () => { journalRecentRev = !journalRecentRev; renderJournal(); }, ["journalNewestFirst", "journalOldestFirst"]));
  } else {
    extras.append(sortBtn(journalUsedRev, () => { journalUsedRev = !journalUsedRev; renderJournal(); }, ["journalMostFirst", "journalLeastFirst"]));
  }
  head.append(extras);
  const col = document.createElement("div");
  col.className = "journal-col" + (mode === "recent" ? " journal-col-recent" : "");
  col.appendChild(head);
  // Recent rows carry a rowid so journalRow() can offer a per-entry delete: "most used"
  // is a usage-count aggregate with no single copy_log row to point at.
  const rowEls = mode === "recent"
    ? recent.map((e) => (e.clip
      ? journalClipRow(e.clip)
      : journalRow(e.p, { left: showTime ? fmtWhen(e.ts) : "", title: showTime ? fmtFull(e.ts) : "", rowid: e.rowid }, copyAgain(e.p))))
    // One meta line: last-used time on the left, copy count on the right.
    : used.map(([p, n, ts]) => journalRow(p, {
      left: showTime && ts ? fmtWhen(ts) : "",
      title: showTime && ts ? fmtFull(ts) : "",
      right: t("usedTimes").replace("{n}", () => n),
    }, copyAgain(p)));
  if (!rowEls.length) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t(mode === "recent" && elsewhereOnly ? "clipEmpty" : "journalEmpty");
    col.appendChild(e);
  } else {
    for (const r of rowEls) col.appendChild(r);
  }
  body.append(col);
}

// The saved pick of the history sort field; anything unknown falls back to "recent".
function journalSortMode() {
  return settings.ui_texts?.journalSort === "used" ? "used" : "recent";
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
  journalSource = "all";
  journalRange = "all";
  $("journal-range").value = "all";
  // renderJournal decides what the filter bar shows, so it also follows the source field.
  await loadJournal();
  if (flag("searchAutofocus")) $("journal-q").focus();
}

export function bindJournal() {
  // Pause/resume recording copies from other apps straight from the copy history.
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
  attachSelectPicker($("journal-range"));
  $("journal-range").addEventListener("change", (e) => {
    journalRange = e.target.value;
    renderJournal();
  });
  attachSelectPicker($("journal-source"));
  $("journal-source").addEventListener("change", (e) => {
    journalSource = e.target.value;
    renderJournal();
  });
  listen("clip-inbox-changed", () => refreshClipInbox());

  const renderJournalDebounced = debounced(renderJournal);
  $("journal-q").addEventListener("input", (e) => { journalQuery = e.target.value; renderJournalDebounced(); });
  wireSearchSuggest($("journal-q"), "recentSearchJournal");
  $("journal-types").addEventListener("click", (e) => {
    const btn = e.target.closest(".lib-type");
    if (btn) { toggleTypeFilter(journalTypes, btn.dataset.type, $("journal-types"), "journalTypeFilterPick"); renderJournal(); }
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
    // Snapshot everything the clears are about to wipe, so Undo can write it all back:
    // the row-level log, the two counter maps and the stored entries.
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
    // The scrub of every backup snapshot cannot be undone, so the entries stay gone.
    // Returns the result text; a failed purge throws and the caller shows it.
    const purgeFromBackups = async () => {
      const [purged, failed] = await invoke("purge_clip_inbox_from_backups");
      let msg = t("clipPurgeBackupsDone").replace("{n}", String(purged));
      // A snapshot that could not be opened or whose scrub could not be confirmed
      // written back must not read as a silent success.
      if (failed > 0) msg += `. ${t("clipPurgeBackupsFailed").replace("{n}", String(failed))}`;
      return msg;
    };
    // Offered only inside the confirmation, and only when the clear covers the copies from
    // other apps. With the confirmation switched off there is no dialog and no way to purge:
    // the backups stay untouched.
    const offerPurge = sourcesOn && clearElsewhere;
    const failAfterClear = (e) => {
      if (appAffected) errUndoToast(e, () => restoreCopyHistory(rows, usage, lastUsed));
      else errToast(e);
    };
    if (!appAffected && !clipAffected) {
      // Nothing to clear, but the backups may still hold entries cleared earlier without
      // the box ticked: ask the purge question alone.
      if (!(flag("confirmClearHistory") && offerPurge)) return;
      const okPurge = await confirmDialog({ title: t("clearHistory"), message: t("clipPurgeOnlyConfirm"), confirmLabel: t("delete") });
      if (!okPurge) return;
      try { toast(await purgeFromBackups()); } catch (e) { errToast(e); }
      return;
    }
    let purgeBackups = false;
    if (flag("confirmClearHistory")) {
      const message = appAffected && clipAffected
        ? t("clearHistoryConfirmBoth").replace("{n}", String(rows.length)).replace("{m}", String(items.length))
        : appAffected ? t("clearHistoryConfirm") : t("clearClipConfirm").replace("{m}", String(items.length));
      const ok = await confirmDialog({
        title: t("clearHistory"),
        message,
        confirmLabel: t("clearHistory"),
        ...(offerPurge ? { checkboxes: [{ key: "purgeBackups", label: t("clipPurgeBackupsCheck"), checked: false }], checkboxesOptional: true } : {}),
      });
      if (!ok) return;
      // Read right now: the box lives in the dialog this very call just closed.
      if (offerPurge) purgeBackups = document.getElementById("confirm-cb-purgeBackups")?.checked === true;
    }
    if (appAffected) {
      try { await invoke("clear_copy_history"); } catch (e) { errToast(e); return; }
      settings.usage = {};
      settings.last_used = {};
    }
    if (clipAffected) {
      // A failed clear stops right here: no purge. The copy log, if it was cleared a
      // moment ago, stays undoable in the very toast that shows the error.
      try { await invoke("clip_inbox_clear"); } catch (e) {
        if (appAffected) await loadJournal();
        failAfterClear(e);
        return;
      }
    }
    let purgeMsg = "";
    let purgeErr = null;
    if (purgeBackups) {
      try { purgeMsg = await purgeFromBackups(); } catch (e) { purgeErr = e; }
    }
    await loadJournal();
    // The stored entries stay gone as asked; only the copy log can come back.
    if (purgeErr !== null) { failAfterClear(purgeErr); return; }
    const clearedMsg = t(appAffected ? "historyClearedToast" : "clipInboxClearedToast");
    if (!appAffected && purgeBackups) {
      toast(purgeMsg || clearedMsg);
      return;
    }
    undoToast(purgeMsg ? `${clearedMsg}. ${purgeMsg}` : clearedMsg, async () => {
      if (appAffected) await restoreCopyHistory(rows, usage, lastUsed);
      if (!purgeBackups) await restoreClipInbox(items);
    });
  });
}
