// Recording folder tools in the expert menu: the old-recordings cleanup and the batch
// re-export.
import { $, errToast, invoke, LANG, listen, t, toast } from "./core.js";
import { confirmDialog } from "./dialog.js";
import { fmtBytes } from "./floatShared.js";
import { refreshVideoFolderInfo } from "./expertMenu.js";

// Cleaning up old recordings now happens in the media manager window: filter by age or size,
// select, delete (with undo). The name stays because the expert menu, the backups screen and the
// record button's menu all open it through here; the folder is resolved by the backend.
export function openVideoCleanup() {
  return invoke("open_media_manager", { kind: "recordings" }).catch((e) => errToast(e));
}

export function openScreenshotManager() {
  return invoke("open_media_manager", { kind: "screenshots" }).catch((e) => errToast(e));
}

// What the manager window asks of this window: export the files it selected through the
// batch dialog here, and refresh the folder readout after it moved something to or from the
// holding folder.
export function bindMediaManagerEvents({ recordingDir }) {
  // The event is only a signal: the folder and names are pulled from the backend, which kept
  // the validated ones, never read from an event payload any window could emit.
  listen("manager-export", async () => {
    const { dir, names } = (await invoke("take_manager_export").catch(() => null)) || {};
    if (dir && Array.isArray(names) && names.length) runBatchExportRecordings(dir, names);
  });
  listen("manager-changed", (e) => {
    if (e.payload === "recordings") refreshVideoFolderInfo(recordingDir());
  });
}

// Video bitrate of the batch export's two lossy quality tiers.
const BATCH_KBPS_HIGH = 12000;
const BATCH_KBPS_SMALL = 4000;

// Output file names of a batch: the source name with the new extension, plus a " (2)", " (3)"
// suffix on every later file that would land on a name already taken in the same batch
// ("rec.mp4" and "rec.mkv" both exported to mp4). Compared case-insensitively like the disk.
// `reserved` are names already there that no output may take (the sources, when the export
// goes back into the folder they came from: an output over its own source is refused).
export function batchDestNames(names, format, reserved = []) {
  const taken = new Set(reserved.map((n) => n.toLowerCase()));
  return names.map((name) => {
    const stem = name.replace(/\.[^.]+$/, "");
    let candidate = `${stem}.${format}`;
    for (let n = 2; taken.has(candidate.toLowerCase()); n++) candidate = `${stem} (${n}).${format}`;
    taken.add(candidate.toLowerCase());
    return candidate;
  });
}

// The source names an export into `destDir` must not reuse: all of them when it goes back
// into the folder they came from (same folder in any slash, case or trailing-slash spelling).
export function batchReservedNames(dir, destDir, names) {
  const key = (p) => String(p).replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
  return key(dir) === key(destDir) ? names : [];
}

// One small dialog for exporting several finished recordings at once: format, quality,
// destination. The full video editor's export panel makes no sense for a batch of unrelated
// files, so this never opens it.
let batchExportClosePrevious = null;
export async function runBatchExportRecordings(dir, names) {
  // A second export asked for while the dialog still waits for an answer ends the first one
  // (answered "no") before it takes the dialog over, so no listener of it stays behind.
  if (batchExportClosePrevious) batchExportClosePrevious();
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
      if (batchExportClosePrevious === closeThis) batchExportClosePrevious = null;
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
    const closeThis = onCancel;
    batchExportClosePrevious = closeThis;
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
  const destNames = batchDestNames(names, format, batchReservedNames(dir, destDir, names));
  const items = names.map((name, i) => ({
    src_path: `${dir}\\${name}`,
    dest_path: `${destDir}\\${destNames[i]}`,
    total_ms: durationsMs[i] || 0,
  }));
  const outcome = await invoke("export_recordings_batch", { items, format, losslessQuality, videoKbps }).catch((e) => { errToast(e); return null; });
  if (!outcome) return;
  toast(t("batchExportDone").replace("{ok}", () => outcome.ok).replace("{failed}", () => outcome.failed));
}
