// Recording folder tools in the expert menu: the old-recordings cleanup and the batch
// re-export.
import { $, errToast, invoke, LANG, t, toast } from "./core.js";
import { confirmDialog } from "./dialog.js";
import { fmtBytes } from "./floatShared.js";
import { refreshVideoFolderInfo } from "./expertMenu.js";

// The full "asks for an age, lists what that would delete, deletes only after
// confirmation" flow for the recordingDir cleanup button in the expert menu. Preview and the
// real delete share the same backend scan, and the delete also receives the
// listed names, so only files that were shown (and are still stale) get removed.
export async function openVideoCleanup(dir) {
  const box = $("video-cleanup-modal");
  const ageSel = $("video-cleanup-age");
  const summary = $("video-cleanup-summary");
  const list = $("video-cleanup-list");
  const confirmBtn = $("video-cleanup-confirm");
  const selectAllBtn = $("video-cleanup-select-all");
  const selectNoneBtn = $("video-cleanup-select-none");
  const exportBtn = $("video-cleanup-export");
  const checks = new Map(); // file name -> its row checkbox, rebuilt on every refresh
  let shownNames = []; // exactly the names the list on screen shows, sent back on confirm
  const updateExportBtn = () => {
    exportBtn.disabled = ![...checks.values()].some((cb) => cb.checked);
  };
  const refresh = async () => {
    const ageDays = Number(ageSel.value);
    const files = await invoke("video_folder_cleanup_preview", { dir, ageDays }).catch((e) => { errToast(e); return []; });
    shownNames = files.map((f) => f.name);
    const totalBytes = files.reduce((s, f) => s + f.size, 0);
    summary.textContent = files.length
      ? t("videoCleanupSummary").replace("{n}", files.length).replace("{size}", fmtBytes(totalBytes, LANG))
      : t("videoCleanupNone");
    confirmBtn.disabled = files.length === 0;
    list.innerHTML = "";
    checks.clear();
    for (const f of files) {
      const row = document.createElement("div");
      row.className = "import-preview-row";
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.addEventListener("change", updateExportBtn);
      checks.set(f.name, cb);
      row.appendChild(cb);
      const name = document.createElement("span");
      name.className = "import-preview-name";
      name.textContent = f.name;
      const size = document.createElement("span");
      size.className = "import-preview-badge";
      size.textContent = fmtBytes(f.size, LANG);
      row.append(name, size);
      list.appendChild(row);
    }
    updateExportBtn();
  };
  // The trash section: a discarded recording in this folder's "verworfen" subfolder, restorable
  // or deletable one at a time. Independent of the age-based bulk cleanup above, so it refreshes
  // on its own after either action instead of closing the dialog.
  const discardedSection = $("video-cleanup-discarded-section");
  const discardedList = $("video-cleanup-discarded-list");
  const refreshDiscarded = async () => {
    const files = await invoke("list_discarded_recordings", { dir }).catch(() => []);
    discardedSection.classList.toggle("hidden", files.length === 0);
    discardedList.innerHTML = "";
    for (const f of files) {
      const row = document.createElement("div");
      row.className = "import-preview-row";
      const name = document.createElement("span");
      name.className = "import-preview-name";
      name.textContent = f.name;
      const size = document.createElement("span");
      size.className = "import-preview-badge";
      size.textContent = fmtBytes(f.size, LANG);
      const restoreBtn = document.createElement("button");
      restoreBtn.type = "button";
      restoreBtn.className = "ghost-btn";
      restoreBtn.textContent = t("videoDiscardedRestore");
      restoreBtn.addEventListener("click", async () => {
        const ok = await invoke("restore_discarded_recording", { dir, name: f.name }).catch(() => false);
        if (ok) { toast(t("videoDiscardedRestored")); refreshDiscarded(); refreshVideoFolderInfo(dir); }
        else toast(t("videoDiscardedRestoreFailed"));
      });
      const deleteBtn = document.createElement("button");
      deleteBtn.type = "button";
      deleteBtn.className = "ghost-btn danger-btn";
      deleteBtn.textContent = t("videoDiscardedDeleteNow");
      deleteBtn.addEventListener("click", async () => {
        const sure = await confirmDialog({ title: t("videoDiscardedDeleteNow"), message: t("videoDiscardedDeleteAsk"), confirmLabel: t("videoDiscardedDeleteNow") });
        if (!sure) return;
        const ok = await invoke("delete_discarded_recording_now", { dir, name: f.name }).catch(() => false);
        if (ok) { refreshDiscarded(); refreshVideoFolderInfo(dir); }
        else toast(t("videoDiscardedDeleteFailed"));
      });
      const actions = document.createElement("div");
      actions.className = "import-preview-actions";
      actions.append(restoreBtn, deleteBtn);
      row.append(name, size, actions);
      discardedList.appendChild(row);
    }
  };
  ageSel.value = "30";
  ageSel.onchange = refresh;
  selectAllBtn.onclick = () => { for (const cb of checks.values()) cb.checked = true; updateExportBtn(); };
  selectNoneBtn.onclick = () => { for (const cb of checks.values()) cb.checked = false; updateExportBtn(); };
  exportBtn.onclick = async () => {
    const names = [...checks.entries()].filter(([, cb]) => cb.checked).map(([name]) => name);
    if (!names.length) return;
    box.classList.add("hidden");
    await runBatchExportRecordings(dir, names);
    box.classList.remove("hidden");
    await refresh();
  };
  await refresh();
  await refreshDiscarded();
  box.classList.remove("hidden");
  const confirmed = await new Promise((resolve) => {
    const done = (v) => {
      box.classList.add("hidden");
      ageSel.onchange = null;
      selectAllBtn.onclick = null;
      selectNoneBtn.onclick = null;
      exportBtn.onclick = null;
      confirmBtn.removeEventListener("click", onOk);
      $("video-cleanup-cancel").removeEventListener("click", onCancel);
      $("video-cleanup-close").removeEventListener("click", onCancel);
      resolve(v);
    };
    const onOk = () => done(true);
    const onCancel = () => done(false);
    confirmBtn.addEventListener("click", onOk);
    $("video-cleanup-cancel").addEventListener("click", onCancel);
    $("video-cleanup-close").addEventListener("click", onCancel);
  });
  if (!confirmed) return;
  const ageDays = Number(ageSel.value);
  const deleted = await invoke("video_folder_cleanup", { dir, ageDays, names: shownNames }).catch((e) => { errToast(e); return 0; });
  toast(t("videoFolderCleanupDone").replace("{n}", () => deleted));
  refreshVideoFolderInfo(dir);
}

// Video bitrate of the batch export's two lossy quality tiers.
const BATCH_KBPS_HIGH = 12000;
const BATCH_KBPS_SMALL = 4000;

// Output file names of a batch: the source name with the new extension, plus a " (2)", " (3)"
// suffix on every later file that would land on a name already taken in the same batch
// ("rec.mp4" and "rec.mkv" both exported to mp4). Compared case-insensitively like the disk.
export function batchDestNames(names, format) {
  const taken = new Set();
  return names.map((name) => {
    const stem = name.replace(/\.[^.]+$/, "");
    let candidate = `${stem}.${format}`;
    for (let n = 2; taken.has(candidate.toLowerCase()); n++) candidate = `${stem} (${n}).${format}`;
    taken.add(candidate.toLowerCase());
    return candidate;
  });
}

// One small dialog for exporting several finished recordings at once: format, quality,
// destination. The full video editor's export panel makes no sense for a batch of unrelated
// files, so this never opens it.
export async function runBatchExportRecordings(dir, names) {
  const box = $("batch-export-modal");
  const summary = $("batch-export-summary");
  const formatSel = $("batch-export-format");
  const qualitySel = $("batch-export-quality");
  const destOut = $("batch-export-dest-out");
  const chooseDestBtn = $("batch-export-choose-dest");
  const startBtn = $("batch-export-start");
  summary.textContent = t("batchExportSummary").replace("{n}", names.length);
  formatSel.value = "mp4";
  qualitySel.value = "high";
  let destDir = "";
  destOut.textContent = "";
  chooseDestBtn.onclick = async () => {
    const picked = await invoke("pick_export_folder").catch(() => null);
    if (picked) { destDir = picked; destOut.textContent = destDir; }
  };
  box.classList.remove("hidden");
  const started = await new Promise((resolve) => {
    const done = (v) => {
      box.classList.add("hidden");
      chooseDestBtn.onclick = null;
      startBtn.removeEventListener("click", onOk);
      $("batch-export-cancel").removeEventListener("click", onCancel);
      $("batch-export-close").removeEventListener("click", onCancel);
      resolve(v);
    };
    const onOk = () => {
      if (!destDir) { toast(t("batchExportNoDest")); return; }
      done(true);
    };
    const onCancel = () => done(false);
    startBtn.addEventListener("click", onOk);
    $("batch-export-cancel").addEventListener("click", onCancel);
    $("batch-export-close").addEventListener("click", onCancel);
  });
  if (!started) return;
  const format = formatSel.value;
  const losslessQuality = qualitySel.value === "lossless";
  const videoKbps = losslessQuality ? null : (qualitySel.value === "small" ? BATCH_KBPS_SMALL : BATCH_KBPS_HIGH);
  const durationsMs = await invoke("video_folder_file_durations_ms", { dir, names }).catch(() => names.map(() => 0));
  const diskWarn = await invoke("check_export_disk_space_batch", { destDir, format, videoKbps, losslessQuality, durationsMs }).catch(() => null);
  if (diskWarn) {
    const msg = t("recDiskWarnExportMsg")
      .replace("{drive}", () => diskWarn.drive)
      .replace("{free}", fmtBytes(diskWarn.free_mb * 1024 * 1024, LANG))
      .replace("{needed}", fmtBytes(diskWarn.needed_mb * 1024 * 1024, LANG));
    const proceed = await confirmDialog({ title: t("recDiskWarnTitle"), message: msg, confirmLabel: t("recDiskWarnExportAnyway"), cancelLabel: t("cancel") });
    if (!proceed) return;
  }
  const destNames = batchDestNames(names, format);
  const items = names.map((name, i) => ({
    src_path: `${dir}\\${name}`,
    dest_path: `${destDir}\\${destNames[i]}`,
    total_ms: durationsMs[i] || 0,
  }));
  const outcome = await invoke("export_recordings_batch", { items, format, losslessQuality, videoKbps }).catch((e) => { errToast(e); return null; });
  if (!outcome) return;
  toast(t("batchExportDone").replace("{ok}", () => outcome.ok).replace("{failed}", () => outcome.failed));
}
