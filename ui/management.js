// Statistics, duplicate finder, compare view, trash, error log and the receiver dialogs.
import { $, assignLibraryTab, assignPrompts, assignSettings, errToast, EXPERT_ACTIONS, flag, hideToast, invoke, LANG, libraryEl, optFlag, prompts, settings, t, toast, txt, undoToast, val } from "./core.js";
import { EXPERT_VALUES } from "./expertConfig.js";
import { applyFlags, applyValues, renderExpert, rowResetBtn, withTip } from "./expertMenu.js";
import { loadPalette, refreshColorUI } from "./theme.js";
import { attachSelectPicker } from "./selectpop.js";
import { runStatsExport, syncSettingsControls } from "./settingsPanel.js";
import { assignLibQuery, lastEdited, libLabel, libQuery, refreshImageHits, renderLibrary, restoreBatchDelete } from "./library.js";
import { renderGrid, renderViews } from "./views.js";
import { confirmDialog } from "./dialog.js";
import { fmtBytes, varPreview } from "./floatShared.js";
import { saveBackupSetting, saveUiState, statCard } from "./backups.js";
import { editPrompt, renderDiffInto, splitDiffSides } from "./editor.js";

// ---- Usage statistics dashboard (opened from the expert menu) ----
export function statSection(text) {
  const h = document.createElement("div");
  h.className = "stat-section";
  h.textContent = text;
  return h;
}
// Collapsible section head (chevron) shared by the backup and statistics panels: it
// folds `contentEl` away and remembers the state under `stateKey`.
export function collapsibleHead(labelText, contentEl, stateKey) {
  const open = optFlag(stateKey);
  const h = document.createElement("button");
  h.type = "button";
  h.className = "stat-section backup-list-head" + (open ? " open" : "");
  const s = document.createElement("span");
  s.textContent = labelText;
  h.appendChild(s);
  h.insertAdjacentHTML("beforeend", '<svg class="collapse-chevron" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M6 9l6 6 6-6"/></svg>');
  contentEl.classList.toggle("hidden", !open);
  h.addEventListener("click", () => {
    const o = !contentEl.classList.toggle("hidden");
    h.classList.toggle("open", o);
    saveUiState(stateKey, o);
  });
  return h;
}
// Label + bounded number field, as used by the backup and statistics panels.
// Clearing the field means "back to the default": it is committed on blur, so
// an empty field mid-typing is left alone.
// `after` redraws the caller's own panel once a reset restores the default,
// same job renderExpert() does for the main expert menu's rows.
export function numRow(labelKey, key, min, max, onSet, extraClass, after) {
  const l = document.createElement("label");
  l.className = "field backup-set-row" + (extraClass ? " " + extraClass : "");
  const s = document.createElement("span"); s.textContent = t(labelKey);
  const i = document.createElement("input");
  i.type = "number"; i.min = min; i.max = max; i.className = "modal-input"; i.value = String(val(key));
  const def = EXPERT_VALUES[key]?.def ?? min;
  const commit = async (raw) => {
    // Number(raw) || def treated a typed 0 as falsy and silently fell back to
    // the default -- a GFS retention tier (min 0 = "none") could never be set to 0.
    const n = Number(raw);
    const v = raw === "" || !Number.isFinite(n) ? def : Math.max(min, Math.min(max, n));
    i.value = String(v);
    // The clear-X wipes the field programmatically, so only an `input` event
    // brings it back once the default has been written into the field again.
    i.dispatchEvent(new Event("input", { bubbles: true }));
    await onSet(v);
  };
  i.addEventListener("change", () => commit(i.value.trim()));
  i.addEventListener("blur", () => { if (i.value.trim() === "") commit(""); });
  const right = document.createElement("span"); right.className = "numrow-right";
  const resetBtn = rowResetBtn("values", key, after);
  if (resetBtn) right.append(resetBtn);
  right.append(i);
  l.append(s, right);
  return l;
}

// Shared with the backup list's per-prompt type chip -- one classifier's
// worth of i18n keys, not two.
export const PROMPT_TYPE_LABELS = { text: "statTypeText", image: "filterImage", video: "filterVideo", pdf: "filterPdf", file: "filterFile" };

// Fire-and-forget: a missed count is not worth interrupting or even toasting
// over, and the backend itself already counts nothing while statistics are
// off, so there is no client-side gate to duplicate here.
export function recordToolUsage(tool) {
  invoke("record_tool_usage", { tool }).catch(() => {});
}

// The usage statistics dialog: the switch, the export row, two figures, the most
// used prompts by name and the never used ones. The full numbers stay in the export.
const STATS_TOP_CHOICES = [5, 10, 20];
export async function renderStatsPanel(body) {
  const on = optFlag("usageStats");
  // Fetch BEFORE clearing: otherwise the panel paints empty during the await,
  // which read as a flash on every re-render.
  let s = null;
  if (on) {
    try { s = await invoke("usage_stats", { topN: Math.round(val("statsTopN")) }); }
    catch (e) { errToast(e); return; }
  }
  body.innerHTML = "";
  // Re-render the container the caller handed in, not the inner panel, so a toggle
  // replaces the content instead of nesting one panel deeper each time.
  const root = body;
  const panel = document.createElement("div");
  panel.className = "stats-panel-body";
  root.appendChild(panel);
  body = panel;
  const headRow = document.createElement("div"); headRow.className = "stats-head-row";
  const sw = document.createElement("label"); sw.className = "field switch-field";
  sw.dataset.settingRow = "usage-stats";
  const swl = document.createElement("span"); swl.textContent = t("statsEnable");
  const swi = document.createElement("input"); swi.type = "checkbox"; swi.className = "switch"; swi.checked = on;
  swi.addEventListener("change", async () => { await saveBackupSetting("flag", "usageStats", swi.checked); renderStatsPanel(root); });
  sw.append(swl, swi); headRow.appendChild(withTip(sw, "usageStats"));
  body.appendChild(headRow);
  if (!on) return;
  // Statistics export: same format select + save-dialog shape as the settings
  // dialog's prompt export, placed here because this is where the data is on screen.
  const exportRow = document.createElement("div");
  exportRow.className = "export-row stats-export-row";
  const exportSelect = document.createElement("select");
  exportSelect.id = "stats-export-format";
  exportSelect.className = "batch-view-select";
  exportSelect.setAttribute("aria-label", t("exportFormatLabel"));
  for (const fmt of ["csv", "txt", "json"]) {
    const opt = document.createElement("option");
    opt.value = fmt;
    opt.textContent = t(fmt === "csv" ? "exportCsv" : fmt === "txt" ? "exportTxt" : "exportJson");
    exportSelect.appendChild(opt);
  }
  attachSelectPicker(exportSelect);
  const exportBtn = document.createElement("button");
  exportBtn.type = "button";
  exportBtn.id = "stats-export-btn";
  exportBtn.className = "ghost-btn";
  exportBtn.textContent = t("statsExport");
  exportBtn.addEventListener("click", () => runStatsExport(exportSelect.value));
  exportRow.append(exportSelect, exportBtn);
  body.appendChild(exportRow);
  const nf = (n) => Number(n).toLocaleString(LANG);
  const cards = document.createElement("div");
  cards.className = "stat-cards";
  cards.append(
    statCard(nf(s.total_prompts), t("statTotalPrompts")),
    statCard(s.history_on ? nf(s.copies7) : "\u2014", t("statCopies7")),
  );
  body.appendChild(cards);
  // Most used: the heading shares its row with the list length choice.
  const topHead = document.createElement("div");
  topHead.className = "stats-top-head";
  const topSel = document.createElement("select");
  topSel.id = "stats-topn";
  topSel.className = "batch-view-select";
  topSel.setAttribute("aria-label", t("valStatsTopN"));
  const curN = Math.round(val("statsTopN"));
  for (const n of [...new Set([...STATS_TOP_CHOICES, curN])].sort((a, b) => a - b)) {
    const o = document.createElement("option");
    o.value = String(n);
    o.textContent = String(n);
    topSel.appendChild(o);
  }
  topSel.value = String(curN);
  attachSelectPicker(topSel);
  topSel.addEventListener("change", async () => {
    await saveBackupSetting("value", "statsTopN", Number(topSel.value));
    renderStatsPanel(root);
  });
  topHead.append(statSection(t("statTop")), topSel);
  body.appendChild(topHead);
  if (s.top.length) {
    const list = document.createElement("div"); list.className = "stat-bars";
    const max = s.top[0].count || 1;
    for (const b of s.top) {
      const row = document.createElement("div"); row.className = "stat-bar-row";
      const name = document.createElement("span"); name.className = "stat-bar-name"; name.textContent = b.name;
      const track = document.createElement("div"); track.className = "stat-bar-track";
      const fill = document.createElement("div"); fill.className = "stat-bar-fill";
      fill.style.width = `${Math.max(4, (b.count / max) * 100)}%`;
      track.appendChild(fill);
      const cnt = document.createElement("span"); cnt.className = "stat-bar-count"; cnt.textContent = nf(b.count);
      row.append(name, track, cnt);
      list.appendChild(row);
    }
    body.appendChild(list);
  } else {
    const none = document.createElement("div"); none.className = "hint"; none.textContent = t("statTopEmpty");
    body.appendChild(none);
  }
  if (s.never_used.length) {
    body.appendChild(statSection(t("statCleanup")));
    const list = document.createElement("div"); list.className = "stat-cleanup";
    for (const b of s.never_used) {
      const item = document.createElement("button");
      item.type = "button"; item.className = "stat-cleanup-item"; item.textContent = b.name;
      item.title = t("statJump");
      item.addEventListener("click", () => jumpToPrompt(b.name));
      list.appendChild(item);
    }
    body.appendChild(list);
  }
  const note = document.createElement("div");
  note.className = "stat-local-only";
  note.textContent = t("statsLocalOnly");
  body.appendChild(note);
}
// The statistics get their own dialog, opened from the settings.
export async function openStats() {
  $("stats-modal").classList.remove("hidden");
  await renderStatsPanel($("stats-panel"));
}
export function closeStats() {
  $("stats-modal").classList.add("hidden");
}
export function bindStats() {
  $("stats-close").addEventListener("click", closeStats);
  $("settings-stats-btn").addEventListener("click", openStats);
  $("stats-modal").addEventListener("pointerdown", (e) => {
    if (e.target === $("stats-modal")) closeStats();
  });
}
export function jumpToPrompt(name) {
  // The settings and the statistics dialog stay open underneath: the library is only
  // raised above them, so closing it lands back on the statistics it was opened from.
  document.body.classList.add("library-front");
  assignLibraryTab("prompts");
  $("library-btn").click(); // resets + opens the library
  assignLibQuery(name || "");
  $("library-q").value = libQuery;
  renderLibrary();
}
// Inline expert panels (backups + statistics): filled on demand by renderExpert.

// ---- Duplicate finder ----
export async function openDupes() {
  $("dupes-modal").classList.remove("hidden");
  await scanDupes();
}
// A found group is now only a summary card -- name list plus score -- with a
// "Compare" button that opens the SAME view the library's/tile's free compare
// uses (openCompareView below), passing this group along so that view shows
// the finder's own "keep one side" actions instead of "take left/right".
// The Rescan button shows a ring for at least a second (opts.minBusyMs) so a scan
// that answers in a few milliseconds still visibly did something. Only one scan
// runs at a time: a second click on the button is ignored, any other caller that
// arrives meanwhile (a list change) is replayed once the running scan is done.
export let dupScanBusy = false;
export let dupScanPending = false;
export async function scanDupes(opts = {}) {
  if (dupScanBusy) {
    if (!opts.fromButton) dupScanPending = true;
    return;
  }
  dupScanBusy = true;
  const body = $("dupes-body");
  const btn = $("dupes-rescan");
  btn.classList.add("is-busy");
  btn.setAttribute("aria-busy", "true");
  // Old cards stay until the new list is ready; only an empty dialog shows the
  // scanning line, so a rescan never flashes it.
  if (!body.children.length) {
    const loading = document.createElement("div");
    loading.className = "hint";
    loading.textContent = t("dupScanning");
    body.appendChild(loading);
  }
  const started = Date.now();
  let groups = null;
  let failure = null;
  try { groups = await invoke("find_duplicates", { threshold: Math.round(val("similarThreshold")) }); }
  catch (e) { failure = e; }
  const left = (opts.minBusyMs || 0) - (Date.now() - started);
  if (left > 0) await new Promise((r) => setTimeout(r, left));
  dupScanBusy = false;
  btn.classList.remove("is-busy");
  btn.removeAttribute("aria-busy");
  if (dupScanPending) { dupScanPending = false; scanDupes(); return; }
  if (failure) { errToast(failure); body.innerHTML = ""; return; }
  recordToolUsage("duplicate_finder");
  body.innerHTML = "";
  if (!groups.length) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t("dupNone");
    body.appendChild(e);
    return;
  }
  for (const g of groups) {
    const card = document.createElement("div");
    card.className = "dup-group";
    const head = document.createElement("div");
    head.className = "dup-head";
    const score = document.createElement("span");
    score.className = "dup-score";
    score.textContent = `${Math.round(g.score * 100)}%`;
    head.appendChild(score);
    card.appendChild(head);
    const names = document.createElement("div");
    names.className = "dup-names";
    names.textContent = g.members.map((m) => m.name).join(" · ");
    card.appendChild(names);
    const actions = document.createElement("div");
    actions.className = "dup-actions";
    const compare = document.createElement("button");
    compare.type = "button";
    compare.className = "ghost-btn";
    compare.textContent = t("compareBtn");
    compare.addEventListener("click", () => openCompareView(g.members.map((m) => m.id), g));
    actions.appendChild(compare);
    const ignore = document.createElement("button");
    ignore.type = "button";
    ignore.className = "ghost-btn";
    ignore.textContent = t("dupIgnore");
    ignore.addEventListener("click", async () => {
      await invoke("ignore_dups", { ids: g.members.map((m) => m.id) }).catch((e) => errToast(e));
      scanDupes();
    });
    actions.appendChild(ignore);
    card.appendChild(actions);
    body.appendChild(card);
  }
}
// Deletes every OTHER member of the group, keeping only `keepId` -- reachable
// exclusively from inside the compare view now (one such button per column,
// see renderCompareView), never from the group card directly.
export async function keepOneDup(group, keepId) {
  const remove = group.members.filter((m) => m.id !== keepId).map((m) => m.id);
  if (!remove.length) return;
  const placements = await invoke("placement_snapshot", { ids: remove }).catch(() => null);
  try {
    const gone = new Set(remove);
    assignSettings(await invoke("batch_prompts", { ids: remove, action: "delete", color: null, favorite: null, viewId: null }));
    assignPrompts(prompts.filter((p) => !gone.has(p.id)));
  } catch (e) { errToast(e); return; }
  closeCompareView();
  renderViews();
  await renderGrid(true);
  scanDupes();
  undoToast(t("batchDeleteDone").replace("{n}", String(remove.length)), async () => {
    await restoreBatchDelete(remove, placements);
    scanDupes();
  });
}

// ---- Compare view ----
// Detached from the duplicate finder: it now takes any set of prompt ids
// (normally two, from the library's own selection or a tile's "Compare
// with…"), fetching each fresh via get_prompt instead of trusting whatever a
// caller already had cached. The duplicate finder calls this SAME view for a
// found group -- there is no second one -- passing the group along so the
// finder's own "keep one side" buttons show here instead of "take left/right".
export let compareState = null;
export async function openCompareView(ids, dupGroup = null) {
  const fetched = await Promise.all(ids.map((id) => invoke("get_prompt", { id }).catch(() => null)));
  if (fetched.some((p) => !p)) { toast(t("compareMissing")); return; }
  compareState = { ids, prompts: fetched, dupGroup };
  renderCompareView();
  $("compare-modal").classList.remove("hidden");
}
export function closeCompareView() {
  $("compare-modal").classList.add("hidden");
  compareState = null;
}
// A rotating prompt's full entry list, entry 1 first -- same shape the version
// history already builds (versionEntries/liveEntries above).
export const compareEntryList = (p) => ((p.variants || []).length ? [p.text || "", ...p.variants] : [p.text || ""]);
export function renderCompareView() {
  if (!compareState) return;
  const { prompts: ps, dupGroup } = compareState;
  const body = $("compare-body");
  body.innerHTML = "";
  // Word-diff highlighting only makes sense between exactly two entries -- a
  // duplicate group of three or more (rare, but the finder's own pairwise
  // matching can produce one) falls back to plain text per column instead of
  // guessing which other column each one should be diffed against.
  const entries = ps.map(compareEntryList);
  ps.forEach((p, idx) => {
    const col = document.createElement("div");
    col.className = "dup-col compare-col";
    const head = document.createElement("div");
    head.className = "compare-col-head";
    const swatch = document.createElement("span");
    swatch.className = "compare-swatch";
    swatch.style.background = p.color || "transparent";
    const name = document.createElement("div");
    name.className = "dup-name";
    name.textContent = p.name;
    head.append(swatch, name);
    col.appendChild(head);
    if (p.image) {
      const img = document.createElement("img");
      img.className = "compare-image";
      img.src = p.image;
      img.alt = "";
      col.appendChild(img);
    }
    const meta = document.createElement("div");
    meta.className = "dup-meta";
    const ts = flag("promptHistory") ? lastEdited[p.id] : null;
    meta.textContent = ts ? new Date(ts * 1000).toLocaleString(LANG, { dateStyle: "short", timeStyle: "short" }) : "";
    col.appendChild(meta);
    const txt = document.createElement("div");
    txt.className = "dup-text";
    if (ps.length === 2) {
      const mine = entries[idx], other = entries[1 - idx];
      const del = idx === 0 ? "diff-del" : "diff-add";
      const maxLen = Math.max(mine.length, other.length);
      for (let i = 0; i < maxLen; i++) {
        const block = document.createElement("div");
        block.className = "compare-entry";
        const hasMine = i < mine.length;
        const hasOther = i < other.length;
        if (!hasMine) continue; // this side simply has fewer entries -- nothing to show
        if (!hasOther) {
          const span = document.createElement("span");
          span.className = del;
          span.textContent = mine[i];
          block.appendChild(span);
        } else {
          const split = idx === 0 ? splitDiffSides(mine[i], other[i]) : splitDiffSides(other[i], mine[i]);
          if (split) renderDiffInto(block, idx === 0 ? split.left : split.right);
          else block.textContent = mine[i]; // DIFF_WORD_CAP fallback: plain text
        }
        txt.appendChild(block);
      }
    } else {
      txt.textContent = p.text;
    }
    col.appendChild(txt);
    if (dupGroup) {
      const keep = document.createElement("button");
      keep.type = "button";
      keep.className = "ghost-btn dup-keep";
      keep.textContent = t("dupKeep");
      keep.addEventListener("click", () => keepOneDup(dupGroup, p.id));
      col.appendChild(keep);
    }
    body.appendChild(col);
  });
  const actions = $("compare-actions");
  actions.innerHTML = "";
  if (dupGroup) {
    // The finder's own actions: keep exactly one of the group, deleting the
    // rest (same keepOneDup as before, just reached from here now). Each
    // column now carries its own keep button; only "ignore" stays here.
    const ignore = document.createElement("button");
    ignore.type = "button";
    ignore.className = "ghost-btn";
    ignore.textContent = t("dupIgnore");
    ignore.addEventListener("click", async () => {
      await invoke("ignore_dups", { ids: dupGroup.members.map((m) => m.id) }).catch((e) => errToast(e));
      closeCompareView();
      scanDupes();
    });
    actions.appendChild(ignore);
  } else {
    const takeLeft = document.createElement("button");
    takeLeft.type = "button";
    takeLeft.className = "ghost-btn";
    takeLeft.textContent = t("compareTakeLeft");
    takeLeft.addEventListener("click", () => takeCompareSide(ps[0], ps[1]));
    const takeRight = document.createElement("button");
    takeRight.type = "button";
    takeRight.className = "ghost-btn";
    takeRight.textContent = t("compareTakeRight");
    takeRight.addEventListener("click", () => takeCompareSide(ps[1], ps[0]));
    actions.append(takeLeft, takeRight);
  }
  const closeBtn = document.createElement("button");
  closeBtn.type = "button";
  closeBtn.className = "ghost-btn";
  closeBtn.textContent = t("close");
  closeBtn.addEventListener("click", closeCompareView);
  actions.appendChild(closeBtn);
}
// Overwrites `target` with `source`'s name/text/colour/image -- the normal
// update_prompt path, so it snapshots the overwritten side into its own version
// history first (see update_prompt) instead of silently discarding it.
export async function takeCompareSide(target, source) {
  let updated;
  try {
    updated = await invoke("update_prompt", {
      id: target.id,
      name: source.name,
      text: source.text,
      color: source.color,
      image: source.image,
      showImage: source.show_image,
      copyImage: source.copy_image,
      variants: source.variants || [],
      rotateMode: source.rotate_mode || "",
      rotateAuto: source.rotate_auto || false,
    });
  } catch (err) {
    errToast(err);
    return;
  }
  // update_prompt hands back the same shape get_state delivers (a confidential
  // entry decrypted, never its sealed text), so it can go straight into the cache.
  if (updated) {
    const idx = prompts.findIndex((x) => x.id === updated.id);
    if (idx >= 0) prompts[idx] = updated;
  }
  closeCompareView();
  await renderGrid(true);
  if (!libraryEl.classList.contains("hidden")) renderLibrary();
  toast(t("compareTaken"));
}
// Import hook: after an import, flag near-duplicates with a toast linking to the tool.
export async function maybeDupImportToast() {
  if (!flag("dupFinder") || !flag("dupImportCheck")) return;
  try {
    const groups = await invoke("find_duplicates", { threshold: Math.round(val("similarThreshold")) });
    if (groups.length) toast(t("dupImportToast").replace("{n}", String(groups.length)));
  } catch (_) {}
}
// The save-time cousin of maybeDupImportToast above: runs after EVERY save
// (create or edit, called with no await from saveModal), never holds up
// anything, and never asks a question -- ignoring the toast loses nothing.
// Shares similarThreshold with the duplicate finder on purpose (the spec's
// own reasoning: one number, one notion of "similar").
export async function maybeShowSimilarHint(id) {
  if (!flag("similarHint")) return;
  try {
    const hit = await invoke("similar_to_saved", { id, threshold: Math.round(val("similarThreshold")) });
    if (!hit) return;
    toast(t("similarHintToast").replace("{name}", () => hit.name), null, "", [
      { label: t("similarHintOpen"), onClick: () => { hideToast(); editPrompt(hit.id); } },
      { label: t("compareBtn"), onClick: () => { hideToast(); openCompareView([id, hit.id]); } },
    ]);
  } catch (_) {}
}

// ---- Trash ----
// Deleting a prompt (grid tile, library row, editor, multi-select) always moves it
// here first: delete_prompt/batch_prompts("delete") no longer hard-remove. This
// dialog is the only place it can come back from, or be purged for good.
export let trashList = [];
export async function openTrash() {
  $("trash-modal").classList.remove("hidden");
  try { trashList = await invoke("list_trash"); } catch (e) { errToast(e); trashList = []; }
  renderTrashList();
}
export function renderTrashList() {
  const body = $("trash-body");
  body.innerHTML = "";
  if (!trashList.length) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t("trashEmpty");
    body.appendChild(e);
    return;
  }
  for (const p of trashList) {
    const row = document.createElement("div");
    row.className = "clip-row";
    const head = document.createElement("span");
    head.className = "clip-time";
    head.textContent = `${libLabel(p)} · ${new Date(p.trashed_at * 1000).toLocaleDateString(LANG)}`;
    const prev = document.createElement("div");
    prev.className = "clip-preview";
    prev.textContent = p.file_path || varPreview(p.text);
    const actions = document.createElement("div");
    actions.className = "clip-actions";
    const restore = document.createElement("button");
    restore.type = "button"; restore.className = "ghost-btn"; restore.textContent = t("trashRestore");
    restore.addEventListener("click", () => restoreTrashItem(p.id));
    const purge = document.createElement("button");
    purge.type = "button"; purge.className = "ghost-btn"; purge.textContent = t("trashPurge");
    purge.addEventListener("click", () => purgeTrashItem(p.id, libLabel(p)));
    actions.append(restore, purge);
    row.append(head, prev, actions);
    body.appendChild(row);
  }
}
export async function restoreTrashItem(id) {
  let st;
  try { st = await invoke("restore_prompt", { id }); } catch (e) { errToast(e); return; }
  assignPrompts(st.prompts);
  assignSettings(st.settings);
  trashList = trashList.filter((p) => p.id !== id);
  renderTrashList();
  renderViews();
  await renderGrid(true);
  if (!libraryEl.classList.contains("hidden")) renderLibrary();
}
export async function purgeTrashItem(id, name) {
  const ok = await confirmDialog({ title: t("trashPurge"), message: `${t("trashPurgeConfirm")}\n\n${name}`, confirmLabel: t("trashPurge") });
  if (!ok) return;
  try { trashList = await invoke("purge_prompt", { id }); } catch (e) { errToast(e); return; }
  renderTrashList();
}
export async function emptyTrashAll() {
  if (!trashList.length) return;
  const ok = await confirmDialog({ title: t("trashTitle"), message: t("trashEmptyConfirm").replace("{n}", String(trashList.length)), confirmLabel: t("trashPurge") });
  if (!ok) return;
  try { trashList = await invoke("empty_trash"); } catch (e) { errToast(e); return; }
  renderTrashList();
}

// ---- Recent messages (error log) ----
// The `errorLog` ring buffer, filled from errToast. Reloaded fresh every time
// the window opens rather than kept live -- it is a look-back tool, not
// something that needs to update while open.
export let errorLogList = [];
export async function openErrorLog() {
  $("error-log-modal").classList.remove("hidden");
  try { errorLogList = await invoke("list_error_log"); } catch (e) { errToast(e); errorLogList = []; }
  renderErrorLogList();
}
export function renderErrorLogList() {
  const body = $("error-log-body");
  body.innerHTML = "";
  if (!flag("errorLog")) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t("errorLogOff");
    body.appendChild(e);
    return;
  }
  if (!errorLogList.length) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t("errorLogEmpty");
    body.appendChild(e);
    return;
  }
  for (const entry of errorLogList) {
    const row = document.createElement("div");
    row.className = "clip-row";
    const head = document.createElement("span");
    head.className = "clip-time";
    head.textContent = `${new Date(entry.ts * 1000).toLocaleString(LANG)} · ${entry.origin}`;
    const prev = document.createElement("div");
    prev.className = "clip-preview";
    prev.textContent = entry.text;
    row.append(head, prev);
    if (entry.raw && entry.raw !== entry.text) {
      const actions = document.createElement("div");
      actions.className = "clip-actions";
      const toggle = document.createElement("button");
      toggle.type = "button"; toggle.className = "ghost-btn"; toggle.textContent = t("errorLogDetails");
      const raw = document.createElement("div");
      raw.className = "clip-preview hidden";
      raw.textContent = entry.raw;
      toggle.addEventListener("click", () => raw.classList.toggle("hidden"));
      actions.appendChild(toggle);
      row.append(actions, raw);
    }
    body.appendChild(row);
  }
}
export async function copyErrorLogAll() {
  if (!errorLogList.length) return;
  const text = errorLogList
    .map((e) => `${new Date(e.ts * 1000).toLocaleString(LANG)} · ${e.origin} · ${e.text}${e.raw && e.raw !== e.text ? ` (${e.raw})` : ""}`)
    .join("\n");
  try { await invoke("copy_text", { text }); toast(t("copied")); } catch (e) { errToast(e); }
}
export async function clearErrorLogAll() {
  if (!errorLogList.length) return;
  try { await invoke("clear_error_log"); } catch (e) { errToast(e); return; }
  errorLogList = [];
  renderErrorLogList();
}

// Regenerating replaces the stored hash, so a fresh plaintext is
// shown exactly once, the same way turning the receiver on for the first
// time already does (flagRow's own remoteApi branch).
export async function openRemotePasswordDialog() {
  const ok = await confirmDialog({
    title: t("remotePasswordDialogTitle"),
    message: t("remotePasswordDialogMessageExisting"),
    confirmLabel: t("remotePasswordRegenerate"),
  });
  if (!ok) return;
  let fresh;
  try {
    fresh = await invoke("remote_api_regenerate_password");
  } catch (e) {
    errToast(e);
    return;
  }
  await confirmDialog({
    title: t("remotePasswordDialogTitle"),
    message: t("remotePasswordDialogMessageNew"),
    input: { type: "text", value: fresh },
    confirmLabel: t("close"),
    oneButton: true,
  });
}

// One line or comma per address/range -- validated server-side (the same
// CIDR check the receiver itself uses) before it is ever saved.
export async function openRemoteRangesDialog() {
  const current = settings.ui_texts?.remoteAllowedRanges || "";
  const value = await confirmDialog({
    title: t("remoteRangesDialogTitle"),
    message: t("remoteRangesDialogMessage"),
    input: { type: "text", value: current, placeholder: "192.168.1.0/24" },
    confirmLabel: t("save"),
  });
  if (value === null) return;
  try {
    await invoke("set_remote_allowed_ranges", { ranges: value });
    settings.ui_texts = { ...(settings.ui_texts || {}), remoteAllowedRanges: value };
  } catch (e) {
    const prefix = "not a valid address or range: ";
    if (String(e).startsWith(prefix)) toast(t("remoteRangesInvalid").replace("{value}", () => String(e).slice(prefix.length)));
    else errToast(e);
  }
}

// The certificate other devices check: its fingerprint (with a copy button), a
// save-for-other-devices button and a renewal. Renewing invalidates the old
// certificate at once, so it asks first; the loop shows the new fingerprint.
export async function openRemoteTlsDialog() {
  for (;;) {
    let fingerprint = null;
    try { fingerprint = await invoke("remote_tls_fingerprint"); } catch (e) { fingerprint = null; }
    const answer = await confirmDialog({
      title: t("remoteTlsFingerprint"),
      message: t("remoteTlsActionTip"),
      input: { type: "text", value: fingerprint || "\u2013", copy: true },
      confirmLabel: t("remoteTlsSave"),
      altLabel: t("remoteTlsRenew"),
      cancelLabel: t("close"),
      danger: false,
    });
    if (answer === null || answer === false) return;
    if (answer === "alt") {
      const sure = await confirmDialog({ title: t("remoteTlsRenew"), message: t("remoteTlsRenewConfirm"), confirmLabel: t("remoteTlsRenew") });
      if (!sure) continue;
      try {
        await invoke("remote_tls_renew");
        toast(t("remoteTlsRenewed"));
      } catch (e) { errToast(e); }
      continue;
    }
    try {
      await invoke("remote_tls_save_cert");
    } catch (e) {
      if (String(e) !== "canceled") errToast(e);
    }
    return;
  }
}

// The certificate lifetime switch. Switching makes a new certificate, so it asks
// first; nothing is saved on cancel. The backend saves the flag and the new
// certificate in one write and restarts a running receiver. True when applied.
export async function toggleShortLivedCert(enabled) {
  const sure = await confirmDialog({ title: t("remoteTlsRenew"), message: t("remoteTlsRenewConfirm"), confirmLabel: t("remoteTlsRenew") });
  if (!sure) return false;
  try {
    await invoke("remote_tls_set_short_lived", { enabled });
  } catch (e) {
    const msg = String(e);
    // Flag and certificate were saved; only the receiver failed to come back up.
    // The switch keeps the saved state and the restart error is shown on its own.
    if (msg.startsWith("restart-failed:")) {
      settings.ui_flags = { ...(settings.ui_flags || {}), remoteTlsShortLived: enabled };
      errToast(msg.slice("restart-failed:".length));
      return true;
    }
    if (msg === "save-failed") toast(t("persistFailed"));
    else if (msg === "the certificate key could not be protected") toast(t("remoteTlsKeyNotProtected"));
    else errToast(e);
    return false;
  }
  settings.ui_flags = { ...(settings.ui_flags || {}), remoteTlsShortLived: enabled };
  toast(t("remoteTlsRenewed"));
  return true;
}

export function bindManagement() {
  // Duplicate finder modal (backups + statistics are now inline expert panels).
  $("dupes-close").addEventListener("click", () => $("dupes-modal").classList.add("hidden"));
  $("dupes-modal").addEventListener("pointerdown", (e) => { if (e.target === $("dupes-modal")) $("dupes-modal").classList.add("hidden"); });
  $("dupes-rescan").addEventListener("click", () => scanDupes({ fromButton: true, minBusyMs: 1000 }));
  // Compare view -- shared by the duplicate finder's own pairs and the
  // library's/tile's free "compare two entries" entry points below.
  $("compare-close").addEventListener("click", closeCompareView);
  $("compare-modal").addEventListener("pointerdown", (e) => { if (e.target === $("compare-modal")) closeCompareView(); });
  $("trash-close").addEventListener("click", () => $("trash-modal").classList.add("hidden"));
  $("trash-modal").addEventListener("pointerdown", (e) => { if (e.target === $("trash-modal")) $("trash-modal").classList.add("hidden"); });
  $("trash-clear").addEventListener("click", () => emptyTrashAll());
  $("error-log-close").addEventListener("click", () => $("error-log-modal").classList.add("hidden"));
  $("error-log-modal").addEventListener("pointerdown", (e) => { if (e.target === $("error-log-modal")) $("error-log-modal").classList.add("hidden"); });
  $("error-log-copy").addEventListener("click", () => copyErrorLogAll());
  $("error-log-clear").addEventListener("click", () => clearErrorLogAll());

  $("library-dupes").addEventListener("click", openDupes);
}

export function wireManagement() {
  EXPERT_ACTIONS["trash-open"] = openTrash;
  EXPERT_ACTIONS["error-log-open"] = openErrorLog;
  EXPERT_ACTIONS["remote-password-open"] = openRemotePasswordDialog;
  EXPERT_ACTIONS["remote-ranges-open"] = openRemoteRangesDialog;
  EXPERT_ACTIONS["remote-tls-open"] = openRemoteTlsDialog;
  // Runs the same sweep on demand -- unlike the silent background sweep,
  // an explicit click always gets an answer, even "nothing to clean up".
  EXPERT_ACTIONS["rec-sweep-now"] = async () => {
    try {
      const [count, bytes] = await invoke("sweep_recordings_now");
      toast(count > 0 ? t("recSweepDone").replace("{n}", String(count)).replace("{v}", fmtBytes(bytes, LANG)) : t("recSweepNothing"));
    } catch (e) {
      errToast(e);
    }
  };
  // The parking notion's own escape hatch: every floating/folder position moves
  // back onto the main screen, valid ones included, and forgets every stored
  // screen identity -- for when nothing about the current placement is trusted
  // any more.
  EXPERT_ACTIONS["float-recall"] = async () => {
    try {
      await invoke("float_recall");
    } catch (e) {
      errToast(e);
    }
  };
  // Only ui_flags/ui_values/ui_texts, plain JSON, no encryption -- never prompts,
  // never hotkeys (see export_settings_only in the backend for why this is a
  // separate, smaller pair of commands from the full-backup export/import).
  EXPERT_ACTIONS["settings-export"] = async () => {
    try {
      const path = await invoke("export_settings_only");
      toast(t("settingsExportDone").replace("{path}", () => path));
    } catch (e) {
      if (e !== "canceled") errToast(e);
    }
  };
  EXPERT_ACTIONS["settings-import"] = async () => {
    const path = await invoke("pick_settings_import_file").catch(() => null);
    if (!path) return;
    let changed, total, skipped;
    try {
      [changed, total, skipped] = await invoke("preview_settings_import", { path });
    } catch (e) {
      toast(t("settingsImportBadFile"));
      return;
    }
    if (changed === 0) {
      toast(t("settingsImportNone"));
      return;
    }
    const ok = await confirmDialog({
      title: t("settingsImportAction"),
      message: t("settingsImportPreview").replace("{n}", String(changed)).replace("{total}", String(total))
        + (skipped > 0 ? " " + t("settingsImportSkipped").replace("{s}", String(skipped)) : ""),
      confirmLabel: t("settingsImportAction"),
      cancelLabel: t("cancel"),
      danger: false,
    });
    if (!ok) return;
    try {
      await invoke("apply_settings_import", { path });
      const s = await invoke("get_state");
      assignPrompts(s.prompts);
      assignSettings(s.settings);
      // The file carried flags, values and texts (palette included): every place
      // that reads them is brought in line, not only the expert menu.
      loadPalette();
      applyFlags();
      applyValues();
      refreshColorUI();
      syncSettingsControls();
      renderExpert();
      renderViews();
      renderGrid(true);
    } catch (e) {
      errToast(e);
    }
  };
  // Reads every picture that has none yet, one at a time -- progress, the
  // running badge and cancel all come from the shared background-jobs popover
  // (see BG_OP_CANCEL_CMD/bgOpLabel above), so this only has to start the job.
  EXPERT_ACTIONS["ocr-index-backfill"] = async () => {
    try {
      await invoke("ocr_index_backfill");
      // New picture text is searchable now: refresh the hits of a query already typed.
      const q = libQuery.trim().toLowerCase();
      if (q) refreshImageHits(q);
    } catch (e) {
      // A pass is already running: its progress and cancel button are in the jobs popover.
      if (String(e) !== "ocr-index-busy") errToast(e);
    }
  };
}
