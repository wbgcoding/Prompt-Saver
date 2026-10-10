// Backup dialog: storage and retention panels, restore and selective restore.
import { $, armButton, DISARM_MS, disarmButton, errText, errToast, flag, invoke, LANG, LOCK_SVG, optFlag, settings, t, toast, undoToast, val, withTaskbarBusy } from "./core.js";
import { applyFlags, applyValues, EXPERT_PATHS, jumpToExpertSetting, openVideoCleanup, rowResetBtn, withTip } from "./expertMenu.js";
import { collapsibleHead, numRow, PROMPT_TYPE_LABELS, recordToolUsage, renderStatsPanel, statSection } from "./management.js";
import { fmtBytes } from "./floatShared.js";
import { restoreClipInbox } from "./journal.js";
import { maybeBackupBeforeWipe, runExportAll, runImportAll } from "./settingsPanel.js";
import { attachSelectPicker } from "./selectpop.js";
import { confirmDialog, EYE_SVG } from "./dialog.js";
import { wireClearButtons } from "./windowChrome.js";

// ---- Backups & restore ----
// A backup folder name as a readable local date + time ("29.07.2026 18:15" in German,
// and whatever the picked language writes elsewhere).
// One stat card: the value on top, its label below.
export function statCard(value, label) {
  const c = document.createElement("div"); c.className = "stat-card";
  const vv = document.createElement("div"); vv.className = "stat-val"; vv.textContent = value;
  const ll = document.createElement("div"); ll.className = "stat-label"; ll.textContent = label;
  c.append(vv, ll); return c;
}

export function prettyStamp(name) {
  const dt = parseStamp(name);
  if (dt) return `${dt.toLocaleDateString(LANG)} ${dt.toLocaleTimeString(LANG, { hour: "2-digit", minute: "2-digit" })}`;
  const [d, tm] = String(name).split("_");
  return tm ? `${d} ${tm.replace(/-/g, ":")}` : d;
}
export function parseStamp(name) {
  const [d, tm] = String(name).split("_");
  if (!d) return null;
  const dt = new Date(`${d}T${(tm || "00-00-00").replace(/-/g, ":")}`);
  return isNaN(dt.getTime()) ? null : dt;
}
export async function saveBackupSetting(kind, key, value) {
  if (kind === "flag") {
    settings.ui_flags = { ...(settings.ui_flags || {}), [key]: value };
    await invoke("set_ui_flag", { key, enabled: value }).catch((e) => errToast(e));
    applyFlags();
  } else {
    settings.ui_values = { ...(settings.ui_values || {}), [key]: value };
    await invoke("set_ui_value", { key, value }).catch((e) => errToast(e));
    applyValues();
  }
}
// Backups live in their own dialog (opened from settings), not in the expert menu:
// the page carries settings, diagnostics and a restore list and needs the room.
// What is typed into the backup-password field but not saved yet. The panel is
// rebuilt on every setting change, which would otherwise throw the input away
// mid-typing. Cleared on save and when the dialog closes.
export let backupPwDraft = "";
export function openBackups() {
  $("backup-modal").classList.remove("hidden");
  renderStatsPanel($("stats-panel"));
  renderStoragePanel($("storage-panel"));
  renderRetentionPanel($("retention-panel"));
  renderBackupPanel($("backup-panel"));
}
export function closeBackups() {
  $("backup-modal").classList.add("hidden");
  backupPwDraft = ""; // never keep an unsaved password around after the dialog is gone
}

// Remembered UI state (which panel sections are expanded). Kept in ui_flags so it
// survives a restart; the key is not in OPT_FLAG_LABELS, so it never shows up as an
// expert toggle and applyFlags() ignores it.
// The write failure is swallowed on purpose, everywhere this is called (including
// tourSeen): every key here is a one-off memory of what the user already saw or
// last had open, never data they typed. Losing a write just means the panel
// starts collapsed again, or the tour reopens once more, next launch -- an error
// toast at that exact moment (right as a tour finishes, or a panel closes) would
// be more disruptive than the failure it reports, for something that quietly
// self-corrects on its own.
export async function saveUiState(key, on) {
  settings.ui_flags = { ...(settings.ui_flags || {}), [key]: on };
  await invoke("set_ui_flag", { key, enabled: on }).catch(() => {});
}

// Label key per storage-breakdown row, in the order the backend returns them.
export const STORAGE_ROW_LABELS = {
  database: "storageKeyDatabase",
  attachments: "storageKeyAttachments",
  screenshots: "storageKeyScreenshots",
  recordings: "storageKeyRecordings",
  discarded_recordings: "storageKeyDiscardedRecordings",
  backups: "storageKeyBackups",
  temp_files: "storageKeyTempFiles",
  edit_states: "storageKeyEditStates",
  crash_log: "storageKeyCrashLog",
};

// The "Speicher" section of the statistics/backups window: how much disk space
// the app's own data actually uses, independent of the usage-statistics switch
// above it (storage is not usage tracking, the two must not share a toggle).
// The backend caches the scan, so reopening this window is instant; the
// "measure again" button is the only thing that forces a fresh walk.
export async function renderStoragePanel(root, force) {
  let data;
  try { data = await invoke("storage_breakdown", { force: !!force }); }
  catch (e) { errToast(e); return; }
  root.innerHTML = "";
  const panel = document.createElement("div");
  panel.className = "stats-panel-body";
  const head = collapsibleHead(t("storageTitle"), panel, "storagePanelOpen");
  head.classList.add("backup-split"); // divider from the usage section above
  root.append(head, panel);
  const body = panel;

  const headRow = document.createElement("div");
  headRow.className = "storage-head-row";
  const totalWrap = document.createElement("div");
  totalWrap.className = "storage-total-wrap";
  const totalEl = document.createElement("span");
  totalEl.className = "storage-total";
  totalEl.textContent = `${t("storageTotal")}: ${fmtBytes(data.total_bytes)}`;
  const measuredEl = document.createElement("span");
  measuredEl.className = "storage-measured";
  measuredEl.textContent = t("storageMeasured").replace("{when}", new Date(data.measured_at * 1000).toLocaleString(LANG));
  totalWrap.append(totalEl, measuredEl);
  const remeasureBtn = document.createElement("button");
  remeasureBtn.type = "button";
  remeasureBtn.id = "storage-remeasure-btn";
  remeasureBtn.className = "ghost-btn";
  remeasureBtn.textContent = t("storageRemeasure");
  remeasureBtn.addEventListener("click", () => renderStoragePanel(root, true));
  headRow.append(totalWrap, remeasureBtn);
  body.appendChild(headRow);

  const list = document.createElement("div");
  list.className = "storage-list";
  for (const item of data.items) {
    const row = document.createElement("div");
    row.className = "storage-row";
    row.dataset.key = item.key;
    const name = document.createElement("span");
    name.className = "storage-name";
    name.textContent = t(STORAGE_ROW_LABELS[item.key] || item.key);
    const size = document.createElement("span");
    size.className = "storage-size";
    if (item.denied) {
      size.textContent = t("storageDenied");
    } else {
      size.textContent = item.capped ? `${fmtBytes(item.bytes)} (${t("storageCapped")})` : fmtBytes(item.bytes);
    }
    row.append(name, size);
    if (item.path) {
      const openBtn = document.createElement("button");
      openBtn.type = "button";
      openBtn.className = "ghost-btn storage-open-btn";
      openBtn.textContent = t("storageOpenFolder");
      openBtn.addEventListener("click", () => invoke("open_storage_folder", { path: item.path }).catch((e) => errToast(e)));
      row.appendChild(openBtn);
    }
    list.appendChild(row);
  }
  body.appendChild(list);
}

// One row per kept-data category, in the fixed order the spec names
// them. Each entry names its own value/select key (for the retention wording
// AND the jump-to-setting button, which needs to know which EXPERT_TABS list
// the key lives in) and its own on-demand cleanup action. Language is the
// only thing normalized across rows (same order: what, how much, how long);
// each category keeps its own existing way of counting -- days for trash/
// recordings/discarded recordings/edit states, a plain count for copy
// history/the clipboard inbox, minutes for temporary exports -- because that
// is what each one already measured correctly before this row existed.
export const RETENTION_ROWS = [
  { key: "trash", nameKey: "trashTitle", settingKey: "trashRetentionDays", settingKind: "selects", wording: "days" },
  { key: "copyHistory", nameKey: "retentionCatCopyHistory", settingKey: "historyMax", settingKind: "values", wording: "count" },
  { key: "clipInbox", nameKey: "flagClipWatcher", settingKey: "clipInboxMax", settingKind: "values", wording: "count" },
  { key: "recordings", nameKey: "storageKeyRecordings", settingKey: "recordingRetentionDays", settingKind: "selects", wording: "days" },
  { key: "discardedRecordings", nameKey: "storageKeyDiscardedRecordings", settingKey: "recDiscardKeepH", settingKind: "values", wording: "hours" },
  { key: "tempExports", nameKey: "retentionCatTempExports", settingKey: "videoClipTtlMin", settingKind: "values", wording: "minutes" },
  { key: "editStates", nameKey: "storageKeyEditStates", settingKey: "editStateKeepDays", settingKind: "values", wording: "days" },
];

// "0 means unlimited" always says so in words (retentionForever), never a
// bare "0d" -- the one exception is temporary exports, whose minutes value
// has no off state (min 5) so it never needs the zero case at all.
export function retentionKeepText(row) {
  const n = val(row.settingKey);
  if (row.wording === "count") return t("retentionKeepsCount").replace("{n}", n);
  if (row.wording === "minutes") return t("retentionKeepsMinutes").replace("{n}", n);
  if (n <= 0) return t("retentionForever");
  return t(row.wording === "hours" ? "retentionKeepsHours" : "retentionKeepsDays").replace("{n}", n);
}

export async function retentionCleanupResult(promise, nothingKey) {
  try {
    const [n, bytes] = await promise;
    toast(n > 0 ? t("retentionCleanupDone").replace("{n}", String(n)).replace("{v}", fmtBytes(bytes)) : t(nothingKey));
  } catch (e) {
    errToast(e);
  }
}

// One backend action per row -- several already existed for their own
// button elsewhere (discarded recordings, temporary exports); trash and copy
// history get a new on-demand command each, since neither had one before.
// The clipboard inbox reuses the SAME full clear the copy history already
// offers: it is always kept at its cap on every insert, so there is nothing
// "expired" to sweep, and building a second, fake "clean up" that only ever
// removes zero items would be worse than being honest about that. Recordings
// opens the existing preview-then-confirm cleanup dialog instead of a silent
// delete, the same safety bar that dialog already holds everywhere else it
// is reached from.
export const RETENTION_CLEANUP = {
  trash: () => retentionCleanupResult(invoke("sweep_trash_now"), "recSweepNothing"),
  copyHistory: () => retentionCleanupResult(invoke("trim_copy_history_now"), "recSweepNothing"),
  clipInbox: async () => {
    const items = await invoke("clip_inbox_list").catch(() => []);
    if (!items.length) { toast(t("recSweepNothing")); return; }
    try { await invoke("clip_inbox_clear"); } catch (e) { errToast(e); return; }
    undoToast(t("clipInboxClearedToast"), () => restoreClipInbox(items));
  },
  recordings: () => { closeBackups(); openVideoCleanup(EXPERT_PATHS.recordingDir.get()); },
  discardedRecordings: () => retentionCleanupResult(invoke("sweep_discarded_recordings_now"), "recSweepNothing"),
  tempExports: () => retentionCleanupResult(invoke("sweep_clip_export_temp_now"), "recSweepNothing"),
  editStates: async () => {
    try {
      const n = await invoke("sweep_edit_states_now");
      toast(n > 0 ? t("retentionCleanupDoneCount").replace("{n}", String(n)) : t("recSweepNothing"));
    } catch (e) { errToast(e); }
  },
};

export async function renderRetentionPanel(root, force) {
  let data;
  try { data = await invoke("retention_overview", { force: !!force }); }
  catch (e) { errToast(e); return; }
  const byKey = Object.fromEntries(data.items.map((i) => [i.key, i]));
  root.innerHTML = "";
  const panel = document.createElement("div");
  panel.className = "stats-panel-body";
  const head = collapsibleHead(t("retentionSectionTitle"), panel, "retentionPanelOpen");
  head.classList.add("backup-split");
  root.append(head, panel);

  const measuredEl = document.createElement("span");
  measuredEl.className = "storage-measured";
  measuredEl.textContent = t("storageMeasured").replace("{when}", new Date(data.measured_at * 1000).toLocaleString(LANG));
  panel.appendChild(measuredEl);

  const list = document.createElement("div");
  list.className = "storage-list";
  for (const row of RETENTION_ROWS) {
    const item = byKey[row.key] || { count: 0, bytes: 0 };
    const wrap = document.createElement("div");
    wrap.dataset.settingRow = `retention-${row.key}`;

    const top = document.createElement("div");
    top.className = "storage-row";
    const name = document.createElement("span");
    name.className = "storage-name";
    name.textContent = t(row.nameKey);
    const amount = document.createElement("span");
    amount.className = "storage-size";
    amount.textContent = t("retentionAmount").replace("{n}", String(item.count)).replace("{size}", fmtBytes(item.bytes));
    top.append(name, amount);

    const bottom = document.createElement("div");
    bottom.className = "path-video-stats";
    const keep = document.createElement("span");
    keep.textContent = retentionKeepText(row);
    const btns = document.createElement("div");
    btns.className = "import-preview-actions";
    const jumpBtn = document.createElement("button");
    jumpBtn.type = "button";
    jumpBtn.className = "ghost-btn";
    jumpBtn.textContent = t("retentionJumpToSetting");
    jumpBtn.addEventListener("click", () => jumpToExpertSetting(row.settingKey, row.settingKind));
    const cleanBtn = document.createElement("button");
    cleanBtn.type = "button";
    cleanBtn.className = "ghost-btn";
    cleanBtn.textContent = t("recSweepNowAction"); // same wording as the discarded-recordings action elsewhere
    cleanBtn.disabled = item.count === 0 && row.key !== "recordings";
    cleanBtn.addEventListener("click", async () => {
      await RETENTION_CLEANUP[row.key]();
      if (row.key !== "recordings") renderRetentionPanel(root, true);
    });
    btns.append(jumpBtn, cleanBtn);
    bottom.append(keep, btns);

    wrap.append(top, bottom);
    list.appendChild(wrap);
  }
  panel.appendChild(list);
}

export async function renderBackupPanel(body) {
  // Fetch BEFORE clearing: emptying the panel and only then awaiting lets the browser
  // paint the empty state for a frame, which showed up as a visible flash on every
  // re-render (backup now, value change …).
  let backups, hadPw;
  try { [backups, hadPw] = await Promise.all([invoke("list_backups"), invoke("has_backup_password").catch(() => false)]); }
  catch (e) { errToast(e); return; }
  body.innerHTML = "";
  const nf = (n) => Number(n).toLocaleString(LANG);
  // Date/time card: date on top, time below (no comma), a touch larger than the
  // plain small value so the timestamps read clearly.
  const dtCard = (l, d) => {
    const c = document.createElement("div"); c.className = "stat-card";
    const vv = document.createElement("div"); vv.className = "stat-val stat-dt";
    if (d) {
      const dd = document.createElement("div"); dd.className = "dt-date"; dd.textContent = d.toLocaleDateString(LANG);
      const tt = document.createElement("div"); tt.className = "dt-time"; tt.textContent = d.toLocaleTimeString(LANG);
      vv.append(dd, tt);
    } else { vv.textContent = "\u2014"; }
    const ll = document.createElement("div"); ll.className = "stat-label"; ll.textContent = l;
    c.append(vv, ll); return c;
  };
  const collapseHead = collapsibleHead;

  const fmtInterval = (h) => (h < 24 ? `${h} h` : `${h / 24} d`); // ≥24h shown in days

  const switchRow = (labelText, on, onChange, tipKey) => {
    const l = document.createElement("label"); l.className = "field switch-field";
    const s = document.createElement("span"); s.textContent = labelText;
    const i = document.createElement("input"); i.type = "checkbox"; i.className = "switch"; i.checked = on;
    i.addEventListener("change", () => onChange(i.checked));
    l.append(s, i);
    return tipKey ? withTip(l, tipKey) : l;
  };

  // ---- Settings ---- Left column: making a backup and when it happens by itself.
  // Right column: how many of them survive.
  body.appendChild(statSection(t("backupSettings")));
  const set = document.createElement("div"); set.className = "backup-settings backup-cols";
  const left = document.createElement("div"); left.className = "backup-col";
  const right = document.createElement("div"); right.className = "backup-col";
  set.append(left, right);

  const nowBtn = document.createElement("button"); nowBtn.type = "button"; nowBtn.className = "ghost-btn backup-now-btn"; nowBtn.textContent = t("backupNow");
  nowBtn.addEventListener("click", async () => {
    // A second click while the first backup is still being written would start another one.
    nowBtn.disabled = true;
    try {
      await withTaskbarBusy("backup", () => invoke("create_backup", { keep: Math.round(val("backupKeep")) }));
      recordToolUsage("backup_create");
      toast(t("backupDone"));
    } catch (e) { errToast(e); } finally { nowBtn.disabled = false; }
    await saveUiState("backupListOpen", true); // reveal the fresh backup right away
    renderBackupPanel(body);
  });
  left.appendChild(nowBtn);
  const openFolderBtn = document.createElement("button");
  openFolderBtn.type = "button";
  openFolderBtn.className = "ghost-btn";
  openFolderBtn.textContent = t("backupOpenFolder");
  openFolderBtn.addEventListener("click", () => invoke("open_backups_folder").catch((e) => errToast(e)));
  left.appendChild(openFolderBtn);
  // Saving to and importing from a file of the user's choice live next to the
  // other backup actions.
  const saveFileBtn = document.createElement("button");
  saveFileBtn.type = "button";
  saveFileBtn.id = "backup-save-file";
  saveFileBtn.className = "ghost-btn";
  saveFileBtn.dataset.settingRow = "backup-save-file";
  saveFileBtn.textContent = t("backupSave");
  saveFileBtn.addEventListener("click", () => runExportAll());
  left.appendChild(saveFileBtn);
  const importFileBtn = document.createElement("button");
  importFileBtn.type = "button";
  importFileBtn.id = "backup-import-file";
  importFileBtn.className = "ghost-btn";
  importFileBtn.dataset.settingRow = "backup-import-file";
  importFileBtn.textContent = t("backupImport");
  importFileBtn.addEventListener("click", async () => { await runImportAll(); renderBackupPanel(body); });
  left.appendChild(importFileBtn);
  const autoBackupRow = switchRow(t("flagAutoBackup"), flag("autoBackup"), async (on) => {
    await saveBackupSetting("flag", "autoBackup", on);
    renderBackupPanel(body);
  });
  autoBackupRow.dataset.settingRow = "backup-auto";
  left.appendChild(autoBackupRow);
  // The interval only means something while automatic backups run.
  if (flag("autoBackup")) {
    const iv = document.createElement("label"); iv.className = "field backup-set-row backup-interval-row";
    iv.dataset.settingRow = "backup-interval";
    const ivl = document.createElement("span"); ivl.textContent = t("valBackupInterval");
    const ivs = document.createElement("select"); ivs.className = "modal-input backup-interval";
    for (const h of [6, 12, 24, 48, 168]) { const o = document.createElement("option"); o.value = String(h); o.textContent = fmtInterval(h); ivs.appendChild(o); }
    ivs.value = String(val("backupIntervalH"));
    ivs.addEventListener("change", async () => { await saveBackupSetting("value", "backupIntervalH", Number(ivs.value)); renderBackupPanel(body); });
    attachSelectPicker(ivs); // themed popup instead of the native OS dropdown
    const ivRight = document.createElement("span"); ivRight.className = "numrow-right";
    const ivReset = rowResetBtn("selects", "backupIntervalH", () => renderBackupPanel(body));
    if (ivReset) ivRight.append(ivReset);
    ivRight.append(ivs);
    iv.append(ivl, ivRight); left.appendChild(iv);
    const delayRow = numRow("valBackupStartDelay", "backupStartDelayS", 0, 300, async (v) => {
      await saveBackupSetting("value", "backupStartDelayS", v);
      renderBackupPanel(body);
    }, "backup-start-delay-row", () => renderBackupPanel(body));
    delayRow.dataset.settingRow = "backup-start-delay";
    left.appendChild(delayRow);
  }

  // Right column: retention. A plain number field replaces the three preset
  // buttons; "Smart retention" is a switch above it that turns the field grey
  // (backend ignores backupKeep while it is on) and activates the tiers below.
  const gfsOn = optFlag("backupGfs");
  const gfsRow = switchRow(t("flagBackupGfs"), gfsOn, async (on) => {
    await saveBackupSetting("flag", "backupGfs", on);
    renderBackupPanel(body);
  }, "tipBackupGfs");
  gfsRow.dataset.settingRow = "backup-gfs";
  right.appendChild(gfsRow);
  const keepRow = numRow("valBackupKeep", "backupKeep", 1, 50, async (v) => {
    await saveBackupSetting("value", "backupKeep", v);
    renderBackupPanel(body); // without this the reset icon never appears live
  }, "backup-keep-row", () => renderBackupPanel(body));
  keepRow.dataset.settingRow = "backup-keep";
  if (gfsOn) {
    keepRow.classList.add("is-disabled");
    keepRow.querySelector("input").disabled = true;
  }
  right.appendChild(keepRow);
  // Both retention states share ONE slot so picking a tile never moves the rows below
  // it. The tier grid is always BUILT and merely parked when smart retention is off:
  // a CSS min-height guessing its height was three pixels short of what the fields
  // really measure, and every font, UI scale and translation moved that target again.
  // Day / week beside each other, month / year underneath: four small fields. The
  // tiers replace the plain count entirely (the backend ignores it while smart
  // retention is on), so the count the user set stays untouched for when it is off.
  const slot = document.createElement("div"); slot.className = "backup-retention-slot";
  right.appendChild(slot);
  const tiers = document.createElement("div");
  tiers.className = "backup-tier-grid" + (gfsOn ? "" : " is-parked");
  // Re-renders like every other row in this panel, so a tier's reset icon
  // appears/disappears live instead of only after some unrelated change.
  const onTier = (key) => async (v) => { await saveBackupSetting("value", key, v); renderBackupPanel(body); };
  const rerenderPanel = () => renderBackupPanel(body);
  const tierRow = (labelKey, key, max, dataKey) => {
    const row = numRow(labelKey, key, 0, max, onTier(key), null, rerenderPanel);
    row.dataset.settingRow = dataKey;
    return row;
  };
  tiers.appendChild(tierRow("valBackupDaily", "backupDaily", 30, "backup-daily"));
  tiers.appendChild(tierRow("valBackupWeekly", "backupWeekly", 52, "backup-weekly"));
  tiers.appendChild(tierRow("valBackupMonthly", "backupMonthly", 60, "backup-monthly"));
  tiers.appendChild(tierRow("valBackupYearly", "backupYearly", 20, "backup-yearly"));
  slot.appendChild(tiers);
  // Optional backup password. Without one, backups use a fixed key and restore on any
  // PC; with one they need exactly this password. The stored password is never sent
  // back to the UI: the eye only reveals what was just typed in this session.
  const pwRow = document.createElement("div"); pwRow.className = "field backup-pw-row";
  pwRow.dataset.settingRow = "backup-password";
  // Dots as the PLACEHOLDER, never as a value: they show that a password exists
  // without putting anything revealable (or accidentally saveable) in the field.
  const PW_DOTS = "••••••••";
  const pwLabel = document.createElement("span"); pwLabel.textContent = t("backupPasswordLabel");
  const pwWrap = document.createElement("div"); pwWrap.className = "backup-pw-wrap";
  const pwIn = document.createElement("input");
  pwIn.type = "password"; pwIn.className = "modal-input"; pwIn.maxLength = 128;
  pwIn.autocomplete = "new-password";
  let hasPw = hadPw; // placeholder + the remove button follow this (syncPwButtons)
  pwIn.value = backupPwDraft; // survives the re-render a setting change triggers
  pwIn.addEventListener("input", () => { backupPwDraft = pwIn.value; });
  const eye = document.createElement("button");
  eye.type = "button"; eye.className = "icon-btn backup-pw-eye";
  eye.title = t("showPassword");
  eye.setAttribute("aria-label", t("showPassword"));
  eye.innerHTML = EYE_SVG;
  // Reveals what is typed right now. A stored password is never loaded back into
  // the field, so there is nothing secret to leak here.
  eye.addEventListener("click", () => {
    const show = pwIn.type === "password";
    pwIn.type = show ? "text" : "password";
    eye.title = t(show ? "hidePassword" : "showPassword");
    eye.setAttribute("aria-label", t(show ? "hidePassword" : "showPassword"));
  });
  // Saving happens ONLY through this button: leaving the field must never change
  // the password by accident.
  const save = document.createElement("button");
  save.type = "button";
  save.className = "ghost-btn backup-pw-save";
  save.textContent = t("save");
  // Removing is its own button below: an empty field here saves nothing, so the
  // password can never be dropped by an absent-minded click on Save.
  const remove = document.createElement("button");
  const syncPwButtons = () => {
    pwIn.placeholder = hasPw ? PW_DOTS : t("backupPasswordPh");
    remove.classList.toggle("hidden", !hasPw);
  };
  save.addEventListener("click", async () => {
    const pw = pwIn.value;
    if (!pw) { toast(t("backupPasswordEmpty")); return; }
    try {
      await invoke("set_backup_password", { password: pw });
      toast(t("backupPasswordWarn"));
      hasPw = true;
      backupPwDraft = "";
      pwIn.value = "";
      pwIn.type = "password";
      syncPwButtons();
    } catch (e) { errToast(e); }
  });
  remove.type = "button";
  remove.className = "ghost-btn danger-btn backup-pw-remove";
  remove.textContent = t("backupPasswordRemove");
  remove.addEventListener("click", async () => {
    const ok = await confirmDialog({
      title: t("backupPasswordRemove"),
      message: t("backupPasswordClearConfirm"),
      confirmLabel: t("backupPasswordRemove"),
      icon: LOCK_SVG,
    });
    if (!ok) return;
    try {
      await invoke("set_backup_password", { password: "" });
      toast(t("backupPasswordCleared"));
      hasPw = false;
      backupPwDraft = "";
      pwIn.value = "";
      pwIn.type = "password";
      syncPwButtons();
    } catch (e) { errToast(e); }
  });
  syncPwButtons();
  pwWrap.append(pwIn, eye);
  // Field and both buttons stay on one line, so the row spans both columns:
  // neither column alone is wide enough for all three.
  const pwControls = document.createElement("div"); pwControls.className = "backup-pw-controls";
  pwControls.append(pwWrap, save, remove);
  pwRow.append(pwLabel, pwControls);
  set.appendChild(pwRow);
  // The consequence of forgetting it, spelled out where it's set: not just
  // in a toast after the fact, which is easy to miss.
  const pwWarn = document.createElement("p");
  pwWarn.className = "modal-hint backup-pw-hint";
  pwWarn.textContent = t("backupPasswordForgetWarn");
  set.appendChild(pwWarn);
  body.appendChild(set);

  // ---- Diagnostics (collapsible) ----
  const total = backups.reduce((s, b) => s + b.size, 0);
  const newest = backups.length ? parseStamp(backups[0].name) : null;
  const oldest = backups.length ? parseStamp(backups[backups.length - 1].name) : null;
  const sizes = backups.map((b) => b.size);
  const cards = document.createElement("div"); cards.className = "stat-cards";
  // Row 1: how many backups there are and what they cost in disk space.
  // Row 2: the time axis; how far back they reach and when the next one is due.
  cards.append(
    statCard(nf(backups.length), t("backupCount")),
    statCard(fmtBytes(total), t("backupTotalSize")),
    statCard(backups.length ? fmtBytes(Math.max(...sizes)) : "\u2014", t("backupLargest")),
    statCard(backups.length ? fmtBytes(Math.min(...sizes)) : "\u2014", t("backupSmallest")),
    statCard(newest && oldest
      ? `${nf(Math.max(0, Math.round((newest - oldest) / 86400000)))} d`
      : "\u2014", t("backupSpan")),
    dtCard(t("backupNext"), newest && flag("autoBackup") ? new Date(newest.getTime() + val("backupIntervalH") * 3600000) : null),
    dtCard(t("backupNewest"), newest),
    dtCard(t("backupOldest"), oldest),
  );
  body.append(collapseHead(t("backupDiag"), cards, "backupDiagOpen"), cards);

  // ---- List (opened right after a manual backup, otherwise last known state) ----
  const list = document.createElement("div"); list.className = "backups-list";
  const listHead = collapseHead(`${t("backupList")} (${backups.length})`, list, "backupListOpen");
  if (!backups.length) {
    const e = document.createElement("div"); e.className = "hint"; e.textContent = t("backupsEmpty");
    list.appendChild(e);
  } else {
    for (const b of backups) {
      const row = document.createElement("div"); row.className = "backup-row";
      const name = document.createElement("span"); name.className = "backup-name"; name.textContent = prettyStamp(b.name);
      // What is actually in that snapshot: read from the snapshot itself, not guessed.
      const count = document.createElement("span"); count.className = "backup-count";
      count.textContent = b.prompts === null || b.prompts === undefined ? "\u2014" : nf(b.prompts);
      count.title = t("backupPrompts");
      // Names are read straight from the snapshot (DPAPI-sealed exactly like
      // the live library: a local snapshot is never password-protected, only the
      // separate manual export-to-file has that option), so there is nothing to ask
      // for here. An empty list just means an older backup made before this existed.
      let namesBox = null;
      if (b.items && b.items.length) {
        namesBox = document.createElement("ul");
        namesBox.className = "backup-names-list hidden";
        for (const item of b.items) {
          const li = document.createElement("li"); li.className = "backup-name-row";
          const nm = document.createElement("span"); nm.className = "backup-name-row-name"; nm.textContent = item.name;
          const kind = document.createElement("span");
          kind.className = "backup-name-row-kind";
          kind.textContent = t(PROMPT_TYPE_LABELS[item.kind] || item.kind);
          const chars = document.createElement("span");
          chars.className = "backup-name-row-chars";
          chars.textContent = item.chars ? nf(item.chars) : "\u2014";
          li.append(nm, kind, chars);
          namesBox.appendChild(li);
        }
        count.classList.add("backup-count-toggle");
        count.setAttribute("role", "button");
        count.tabIndex = 0;
        count.title = t("backupNamesToggle");
        const toggle = () => namesBox.classList.toggle("hidden");
        count.addEventListener("click", toggle);
        count.addEventListener("keydown", (e) => {
          if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggle(); }
        });
      }
      const size = document.createElement("span"); size.className = "backup-size"; size.textContent = fmtBytes(b.size);
      // Opens the same read rail restoring already uses (stage, validate, read),
      // just without ever writing the result anywhere -- see backup_contents.
      const contentBox = document.createElement("div");
      contentBox.className = "backup-content hidden";
      let contentLoaded = false;
      const viewContent = document.createElement("button");
      viewContent.type = "button"; viewContent.className = "ghost-btn"; viewContent.textContent = t("backupViewContent");
      viewContent.addEventListener("click", async () => {
        const opening = contentBox.classList.toggle("hidden") === false;
        if (opening && !contentLoaded) {
          contentLoaded = true;
          await renderBackupContent(contentBox, b.name);
        }
      });
      const restore = document.createElement("button"); restore.type = "button"; restore.className = "ghost-btn"; restore.textContent = t("backupRestore");
      restore.addEventListener("click", () => restoreBackup(b.name));
      const del = document.createElement("button"); del.type = "button"; del.className = "ghost-btn danger-btn"; del.textContent = t("delete");
      del.addEventListener("click", async () => {
        if (!armButton(del, `${t("delete")}?`)) { setTimeout(() => disarmButton(del, t("delete")), DISARM_MS); return; }
        await invoke("delete_backup", { name: b.name }).catch((e) => errToast(e));
        await saveUiState("backupListOpen", true); // stay in the list after delete
        renderBackupPanel(body);
      });
      row.append(name, count, size, viewContent, restore, del);
      list.appendChild(row);
      if (namesBox) list.appendChild(namesBox);
      list.appendChild(contentBox);
    }
  }
  body.append(listHead, list);
  wireClearButtons(body); // clear-X on the number fields
}
export async function restoreBackup(name) {
  // Both checked (the default) = the old full-file "all" restore; only one
  // checked = the new selective merge; the dialog itself refuses zero checked.
  const ok = await confirmDialog({
    title: t("backupRestore"), message: t("backupRestoreConfirm"), confirmLabel: t("backupRestore"),
    checkboxes: [
      { key: "data", label: t("backupRestoreData"), checked: true },
      { key: "settings", label: t("backupRestoreSettings"), checked: true },
    ],
  });
  if (!ok) return;
  const dataOn = $("confirm-cb-data").checked;
  const settingsOn = $("confirm-cb-settings").checked;
  const scope = dataOn && settingsOn ? "all" : dataOn ? "data" : "settings";
  // The same safety net as delete: restoring replaces the current
  // data just as permanently, so it follows the same "before delete" switch.
  await maybeBackupBeforeWipe("delete");
  try { await withTaskbarBusy("backup", () => invoke("restore_backup", { name, keep: Math.round(val("backupKeep")), scope })); }
  catch (e) { errToast(e); } // on success the app relaunches
}

// What a single snapshot actually holds: header counts, then every entry with
// name, date, size and kind, searchable and sortable -- restoring straight from
// here reuses restoreBackup() above rather than a second restore path.
export async function renderBackupContent(box, name) {
  box.innerHTML = "";
  let data;
  try { data = await invoke("backup_contents", { name }); }
  catch (e) { box.textContent = errText(e); return; }
  const nf = (n) => Number(n).toLocaleString(LANG);
  const cards = document.createElement("div"); cards.className = "stat-cards backup-content-cards";
  cards.append(
    statCard(nf(data.entries), t("backupContentEntries")),
    statCard(nf(data.views), t("backupContentViews")),
    statCard(nf(data.tags), t("backupContentTags")),
    statCard(data.last_change ? new Date(data.last_change * 1000).toLocaleDateString(LANG) : "\u2014", t("backupContentLastChange")),
    statCard(data.program_version || "\u2014", t("backupContentVersion")),
    statCard(data.encrypted ? t("backupContentEncryptedYes") : t("backupContentEncryptedNo"), t("backupContentEncrypted")),
  );
  box.appendChild(cards);

  const toolbar = document.createElement("div"); toolbar.className = "backup-content-toolbar";
  const search = document.createElement("input");
  search.type = "text"; search.className = "modal-input backup-content-search";
  search.placeholder = t("backupContentSearch");
  const restoreBtn = document.createElement("button");
  restoreBtn.type = "button"; restoreBtn.className = "ghost-btn"; restoreBtn.textContent = t("backupRestore");
  restoreBtn.addEventListener("click", () => restoreBackup(name));
  toolbar.append(search, restoreBtn);
  box.appendChild(toolbar);

  const table = document.createElement("div"); table.className = "backup-content-table";
  const head = document.createElement("div"); head.className = "backup-content-row backup-content-head";
  const cols = [
    ["name", t("backupContentColName")],
    ["date", t("backupContentColDate")],
    ["chars", t("backupContentColSize")],
    ["kind", t("backupContentColKind")],
  ];
  let sortKey = "name";
  let sortDir = 1;
  const headBtns = {};
  for (const [key, label] of cols) {
    const cell = document.createElement("button");
    cell.type = "button"; cell.className = `backup-content-col-btn backup-content-cell-${key}`;
    cell.textContent = label;
    cell.addEventListener("click", () => {
      if (sortKey === key) sortDir = -sortDir; else { sortKey = key; sortDir = 1; }
      renderRows();
    });
    headBtns[key] = cell;
    head.appendChild(cell);
  }
  table.appendChild(head);
  const rowsBox = document.createElement("div"); rowsBox.className = "backup-content-rows";
  table.appendChild(rowsBox);
  const emptyHint = document.createElement("div"); emptyHint.className = "hint backup-content-empty hidden";
  emptyHint.textContent = t("backupContentNoMatch");
  table.appendChild(emptyHint);
  box.appendChild(table);

  function renderRows() {
    for (const [key, cell] of Object.entries(headBtns)) {
      cell.classList.toggle("sorted", key === sortKey);
      cell.classList.toggle("sorted-desc", key === sortKey && sortDir === -1);
    }
    const q = search.value.trim().toLowerCase();
    const items = data.items
      .filter((it) => !q || it.name.toLowerCase().includes(q))
      .slice()
      .sort((a, b) => {
        if (sortKey === "name" || sortKey === "kind") {
          const av = (a[sortKey] || "").toLowerCase();
          const bv = (b[sortKey] || "").toLowerCase();
          if (av < bv) return -1 * sortDir;
          if (av > bv) return 1 * sortDir;
          return 0;
        }
        // A never-edited entry has no date and always sorts to the very end,
        // in either direction -- the same rule the library's own "last
        // edited" sort already uses, just not tied to one fixed direction.
        const av = a[sortKey];
        const bv = b[sortKey];
        if (av == null && bv == null) return 0;
        if (av == null) return 1;
        if (bv == null) return -1;
        if (av < bv) return -1 * sortDir;
        if (av > bv) return 1 * sortDir;
        return 0;
      });
    rowsBox.innerHTML = "";
    emptyHint.classList.toggle("hidden", items.length > 0);
    for (const it of items) {
      const row = document.createElement("div"); row.className = "backup-content-row";
      const nm = document.createElement("span"); nm.className = "backup-content-cell-name"; nm.textContent = it.name;
      const dt = document.createElement("span"); dt.className = "backup-content-cell-date";
      dt.textContent = it.date ? new Date(it.date * 1000).toLocaleDateString(LANG) : "\u2014";
      const sz = document.createElement("span"); sz.className = "backup-content-cell-chars"; sz.textContent = it.chars ? nf(it.chars) : "\u2014";
      const kd = document.createElement("span"); kd.className = "backup-content-cell-kind"; kd.textContent = t(PROMPT_TYPE_LABELS[it.kind] || it.kind);
      row.append(nm, dt, sz, kd);
      rowsBox.appendChild(row);
    }
  }
  search.addEventListener("input", renderRows);
  renderRows();
}

export function bindBackups() {
  $("backup-close").addEventListener("click", closeBackups);
  $("backup-modal").addEventListener("pointerdown", (e) => {
    if (e.target === $("backup-modal")) closeBackups();
  });
}
