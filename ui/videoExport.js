// Video editor: the export run, its size estimate and the ffmpeg presence check.
import { makeEtaEstimator, formatEta, a11yProgressText, parseFfmpegError, fmtBytes } from "./floatShared.js";
import { errMessageLogged } from "./errors.js";
import { exportNameIsValid } from "./exportName.js";
import { confirmDialog } from "./dialog.js";
import { exportFormat, exportQuality, effectiveSize, scaleTarget, exportMediaPaths, buildVideoChain, buildAudioChain, buildCopyMixChain, copyAudioMode, isLosslessCopy } from "./videoGraph.js";
import { $, clamp, ETA_MIN_PCT, ETA_MIN_SECS, EXPORT_KBPS, fmtTime, formatExt, invoke, keptSegments, LANG, showExportErrorDetails, SILENT_FORMAT_LENGTH_WARN_MS, SILENT_FORMATS, t, toast, totalOutMs, updateAudioLabels, VE } from "./video.js";

// ---------------------------------------------------------------- export

// The source file's own stem, with no folder and no extension: the base
// every auto-named export starts from.
function sourceStem() {
  const cut = VE.src.replace(/[/\\]+$/, "");
  const dot = cut.lastIndexOf(".");
  const slash = Math.max(cut.lastIndexOf("\\"), cut.lastIndexOf("/"));
  const stem = dot > slash ? cut.slice(0, dot) : cut;
  return stem.replace(/^.*[\\/]/, "");
}

// The file-name field's own value, or the auto "<source stem>-edit" guess
// when the user never touched it (or cleared it back to empty).
export function exportNameStem() {
  const custom = (VE.exportNameOverride || "").trim();
  return custom || `${sourceStem()}-edit`;
}

// Three ways to land on a destination, checked in order: a full path
// the user typed or picked via "Save as…" wins outright; otherwise a
// remembered "Export to…" folder keeps the (auto or custom) filename but
// moves it there; otherwise the original folder next to the source file.
export function destPath() {
  if (VE.destOverride) return VE.destOverride;
  const format = exportFormat();
  const name = `${exportNameStem()}.${formatExt(format)}`;
  // A file opened for editing from outside the app suggests its own folder over the standing
  // videoExportDir preference, which is about ordinary recording exports, not this one-off copy.
  const dir = VE.originDir || VE.destDir;
  if (dir) return `${dir.replace(/[\\/]+$/, "")}/${name}`;
  const cut = VE.src.replace(/[/\\]+$/, "");
  const slash = Math.max(cut.lastIndexOf("\\"), cut.lastIndexOf("/"));
  const srcDir = slash >= 0 ? cut.slice(0, slash) : "";
  return srcDir ? `${srcDir}/${name}` : name;
}

// One explanatory note per format that needs one, kept as a lookup instead of
// a chain of ifs so a future format only ever needs one new entry here.
const FORMAT_NOTE_KEY = { gif: "veGifNote", avi: "veAviNote" };

// Builds everything the note can say without asking the backend, so the DOM update stays
// instant and synchronous. The size estimate (backend-computed) is patched in afterwards by
// refreshExportSizeNote.
function buildExportNoteBase(format) {
  const notes = [];
  if (FORMAT_NOTE_KEY[format]) notes.push(t(FORMAT_NOTE_KEY[format]));
  if (SILENT_FORMATS.has(format)) {
    notes.push(t("veNoAudioNote"));
    if (totalOutMs() > SILENT_FORMAT_LENGTH_WARN_MS) notes.push(t("veLengthWarn").replace("{n}", String(Math.round(totalOutMs() / 1000))));
  }
  const copyMode = copyAudioMode();
  if (copyMode) notes.push(t(copyMode === "mix" ? "veCopyMixNote" : "veCopyNote"));
  notes.push(t("veOutLength").replace("{d}", fmtTime(totalOutMs())));
  return notes;
}

function formatExportSizeNote(est) {
  if (!est) return "";
  if (est.low_bytes != null && est.high_bytes != null) {
    return t("veSizeRange").replace("{low}", fmtBytes(est.low_bytes, LANG)).replace("{high}", fmtBytes(est.high_bytes, LANG));
  }
  if (est.bytes != null) return t("veSizeAbout").replace("{size}", fmtBytes(est.bytes, LANG));
  return "";
}

// Monotonic guard so a slow estimate_export_size reply for a format the user
// has since clicked away from never overwrites a newer, still-current note.
let exportNoteGen = 0;

async function refreshExportSizeNote(format) {
  const myGen = ++exportNoteGen;
  const est = await invoke("estimate_export_size", {
    format,
    videoKbps: videoKbps(),
    losslessQuality: exportQuality() === "lossless",
    durationMs: Math.round(totalOutMs()),
  }).catch(() => null);
  if (myGen !== exportNoteGen) return; // superseded by a later call
  if (exportFormat() !== format || isLosslessCopy()) return; // stale by the time it landed
  const sizeText = formatExportSizeNote(est);
  if (!sizeText) return;
  const notes = buildExportNoteBase(format);
  notes.push(sizeText);
  $("ve-export-note").textContent = notes.join(" ");
}

// The button beside Export: "Stop export" while a run is going, otherwise "Back" to the Edit tab.
// The i18n key moves with the text so a later language pass keeps the right label.
export function syncExportButton() {
  const key = VE.exporting ? "veStopExport" : "veBack";
  const button = $("ve-export-cancel");
  button.dataset.i18n = key;
  button.textContent = t(key);
}

// The one place that decides whether Export/Copy-to-clipboard may be clicked: an invalid or
// empty file name joins the "still exporting"/"ffmpeg missing" reasons, so no call site has to
// know about the others.
export function applyExportGate() {
  syncExportButton();
  const valid = exportNameIsValid($("ve-export-name").value);
  $("ve-export-name").classList.toggle("invalid", !valid);
  $("ve-export-name-hint").classList.toggle("hidden", valid);
  const disabled = VE.exporting || !VE.ffmpeg;
  // "Copy to clipboard" writes to its own throwaway temp name (clip_export_path)
  // and never reads the file-name field at all, so a name the user has not
  // finished typing yet must not block it the way it blocks the real export.
  $("ve-export-start").disabled = disabled || !valid;
  $("ve-export-clip").disabled = disabled;
  return valid;
}

export function updateExportNote() {
  const format = exportFormat();
  $("ve-export-note").textContent = buildExportNoteBase(format).join(" ");
  $("ve-export-ext").textContent = `.${formatExt(format)}`;
  // Never overwrite what the user is actively typing into the field itself.
  if (document.activeElement !== $("ve-export-name")) $("ve-export-name").value = exportNameStem();
  if (document.activeElement !== $("ve-export-dest")) $("ve-export-dest").value = destPath();
  $("ve-export-dest").title = destPath();
  applyExportGate();
  // None of the three silent formats read the quality/bitrate setting at all.
  $("ve-export-quality").disabled = SILENT_FORMATS.has(format);
  updateAudioLabels();
  // A stream copy has no bitrate of its own to estimate from; veCopyNote
  // above already explains why no number is shown for it.
  if (!isLosslessCopy()) refreshExportSizeNote(format);
}

const exportEta = makeEtaEstimator();

// The running export's recording generation and output length, so progress events
// scale against this export and are ignored once another recording was loaded.
export let exportRun = null;

// Whether an export that started on recording `gen` at edit revision `rev` rendered
// everything that is in the editor now: the same recording, no edit made while it ran.
function exportCoveredEdits(gen, rev) {
  return VE.loadGen === gen && VE.rev === rev;
}

// Back to the idle look after an export that never started writing: no stale
// "Exporting" line, no 0% bar.
function clearExportStatus() {
  $("ve-export-status").textContent = "";
  $("ve-export-progress-row").classList.add("hidden");
}

export function setProgress(pct) {
  const clamped = clamp(pct, 0, 100);
  const row = $("ve-export-progress-row");
  row.classList.remove("hidden");
  $("ve-export-progress").value = clamped;
  $("ve-export-pct").textContent = `${Math.round(clamped)}%`;
  exportEta.feed(clamped);
  const estimate = exportEta.estimate(ETA_MIN_PCT, ETA_MIN_SECS);
  const etaSpan = $("ve-export-eta");
  etaSpan.textContent = formatEta(estimate, t);
  $("ve-export-progress").setAttribute("aria-valuetext", a11yProgressText(t, clamped, estimate));
}

// toClipboard runs the same export into a throwaway temp file instead of the normal
// destination, then puts that file on the clipboard (see clip_export_path/sweep_clip_export_temp
// in the backend for where and when the temp file goes away).
export async function startExport(toClipboard) {
  if (VE.exporting) return;
  if (!VE.ffmpeg) { toast(t("recFfmpegMissingHint")); return; }
  // The backend takes whole milliseconds; a tempo such as 1.3x makes the output length fractional.
  const total = Math.round(totalOutMs());
  if (total < 1) { toast(t("veNothingToExport")); return; }
  const copyMode = copyAudioMode();
  const lossless = copyMode !== null;
  const format = exportFormat();
  // Claimed before the first await, so a double click cannot start a second export.
  VE.exporting = true;
  let dest = toClipboard
    ? await invoke("clip_export_path", { ext: formatExt(format) }).catch((err) => { toast(errMessageLogged(err, t, "video")); return null; })
    : destPath();
  if (!dest) { VE.exporting = false; return; }
  // Another recording can be opened while this export still runs; its result
  // must never touch that recording's state.
  const gen = VE.loadGen;
  const rev = VE.rev;
  const src = VE.src;
  const stillHere = () => VE.loadGen === gen;
  VE.exporting = true;
  exportRun = { gen, total };
  syncExportButton();
  $("ve-export-start").disabled = true;
  $("ve-export-clip").disabled = true;
  $("ve-export-saveas").classList.add("hidden");
  $("ve-export-status").textContent = t("veExporting");
  showExportErrorDetails("");
  exportEta.reset();
  setProgress(0);
  // A source opened from outside the app (or a library attachment) has a destination the backend
  // never authorized through a picker or saved preference; retried at most once with a folder
  // the user just chose in the export-folder dialog. A clipboard export never lands here: its
  // destination is always the app's temp folder.
  let destRetried = false;
  try {
    for (;;) {
      const request = {
        src_path: src,
        dest_path: dest,
        filter_chain: lossless ? null : buildVideoChain(),
        audio_map: lossless ? (copyMode === "mix" ? buildCopyMixChain() : null) : buildAudioChain(),
        // What a stream copy does with the audio tracks: "all" copies every stream, "first" keeps
        // the first (mic muted), "mix" mixes both into one stream.
        copy_audio: copyMode || "all",
        media_paths: lossless ? [] : exportMediaPaths(),
        lossless_copy: lossless,
        // Distinct from lossless_copy: the "Lossless" quality tier forces a real re-encode (see
        // isLosslessCopy) that must stay lossless (-crf 0) instead of falling back to a bitrate target.
        lossless_quality: exportQuality() === "lossless",
        format,
        video_kbps: lossless ? null : videoKbps(),
        // Only meaningful on the stream-copy path, where there is no filter graph
        // that could carry the trim itself.
        trim_start_ms: lossless ? Math.round(keptSegments()[0].a) : null,
        trim_end_ms: lossless ? Math.round(keptSegments().slice(-1)[0].b) : null,
        total_ms: total,
      };
      // A stream copy has no bitrate-based estimate to check against (see
      // estimate_export_bytes): skipped the same way its size note above is.
      if (!lossless) {
        const diskWarn = await invoke("check_export_disk_space", {
          destPath: dest, format, videoKbps: request.video_kbps, losslessQuality: request.lossless_quality, durationMs: total,
        }).catch(() => null);
        if (diskWarn) {
          const msg = t("recDiskWarnExportMsg")
            .replace("{drive}", () => diskWarn.drive)
            .replace("{free}", fmtBytes(diskWarn.free_mb * 1024 * 1024, LANG))
            .replace("{needed}", fmtBytes(diskWarn.needed_mb * 1024 * 1024, LANG));
          const proceed = await confirmDialog({ title: t("recDiskWarnTitle"), message: msg, confirmLabel: t("recDiskWarnExportAnyway"), danger: false });
          if (!proceed) { clearExportStatus(); return; }
        }
      }
      try {
        // The backend lands the file on a free name when the requested one is
        // taken, so everything after success refers to the path it returns.
        const returned = await invoke("export_video", { req: request });
        const outPath = typeof returned === "string" && returned ? returned : dest;
        invoke("record_tool_usage", { tool: "video_export" }).catch(() => {});
        // Only when nothing was edited while it ran: a later edit is not in the file.
        const covered = exportCoveredEdits(gen, rev);
        if (covered) VE.dirty = false;
        // A finished export leaves nothing "unfinished": clear the remembered row so reopening this
        // recording never offers to restore work that already made it out. Later edits get a fresh row
        // from the next autosave tick.
        if (covered && VE.editStateKeepOn && src) {
          invoke("discard_edit_state", { kind: "video", target: src }).catch(() => {});
        }
        if (stillHere()) setProgress(100);
        if (toClipboard) {
          const clipOk = await invoke("copy_file_to_clipboard", { path: outPath }).catch(() => false);
          if (stillHere()) $("ve-export-status").textContent = clipOk ? t("veClipExportDone") : t("veExportDone").replace("{path}", () => outPath);
          toast(clipOk ? t("veClipExportDone") : t("veExportDone").replace("{path}", () => outPath));
        } else {
          if (stillHere()) {
            VE.lastExport = outPath;
            $("ve-export-status").textContent = t("veExportDone").replace("{path}", () => outPath);
            $("ve-export-saveas").classList.remove("hidden");
          }
          toast(t("veExportDone").replace("{path}", () => outPath));
          if ($("ve-export-tile").checked) await addExportTile(outPath);
        }
        return;
      } catch (err) {
        const raw = String(err);
        if (!toClipboard && !destRetried && raw.includes("export destination not recognized")) {
          destRetried = true;
          const folder = await invoke("pick_export_folder").catch(() => null);
          // Another recording was opened while the picker was up: this export is over.
          if (!stillHere()) return;
          if (!folder) {
            // Nothing was chosen (cancelled, or no dialog could open): the
            // export did not happen, and the status line says why.
            clearExportStatus();
            if (stillHere()) $("ve-export-status").textContent = t("veExportDestRefused");
            return;
          }
          const base = dest.replace(/^.*[\\/]/, "");
          dest = `${folder.replace(/[\\/]+$/, "")}/${base}`;
          continue;
        }
        throw err;
      }
    }
  } catch (err) {
    noteFfmpeg(err);
    const raw = String(err);
    // A user cancel is not a failure and never shows the error UI; a missing ffmpeg already has its
    // banner from noteFfmpeg(). Everything else is a real ffmpeg or filesystem failure, translated
    // by the shared classifier, with the raw text only under "Technical details".
    if (!stillHere()) {
      // the editor moved on to another recording; nothing of this one to show
    } else if (raw.trim() === "cancelled") {
      $("ve-export-status").textContent = t("veExportCancelled");
    } else if (raw.includes("ffmpeg-missing")) {
      $("ve-export-status").textContent = t("recFfmpegMissingHint");
    } else if (raw.includes("export destination not recognized")) {
      $("ve-export-status").textContent = t("veExportDestRefused");
    } else {
      const { message, details } = parseFfmpegError(err, t);
      $("ve-export-status").textContent = message;
      showExportErrorDetails(details);
    }
  } finally {
    VE.exporting = false;
    exportRun = null;
    applyExportGate();
    $("ve-export-eta").textContent = "";
    // The editor moved on to another recording: its progress row is not ours to leave showing.
    if (!stillHere()) $("ve-export-progress-row").classList.add("hidden");
  }
}

// "Add to a tile after saving": the finished file becomes a tile in the main window's active
// view. A full grid is told to the person and nothing is created. A failed tile never turns a
// finished export into an error.
async function addExportTile(path) {
  try {
    await invoke("video_editor_add_tile", { path });
    toast(t("veExportTileDone"));
  } catch (err) {
    toast(String(err).includes("view-full") ? t("gridFull") : errMessageLogged(err, t, "video"));
  }
}

export function videoKbps() {
  const base = EXPORT_KBPS[exportQuality()];
  if (base == null) return null;
  if (!VE.w || !VE.h) return base;
  // The output size is the crop, then the scale; the bitrate follows the linear size.
  const out = scaleTarget() || effectiveSize();
  const ratio = Math.sqrt((out.w * out.h) / (VE.w * VE.h));
  if (!(ratio < 1)) return base;
  // Fewer pixels need fewer bits; the floor keeps a 480p clip from going muddy.
  return Math.max(800, Math.round(base * ratio));
}

// ---------------------------------------------------------------- ffmpeg presence

export function noteFfmpeg(err) {
  if (!String(err).includes("ffmpeg-missing")) return;
  VE.ffmpeg = false;
  $("ve-ffmpeg-warn").classList.remove("hidden");
  $("ve-export-start").disabled = true;
  $("ve-export-clip").disabled = true;
  $("ve-still").disabled = true;
}
