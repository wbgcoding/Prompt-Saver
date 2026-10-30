// Video editor: loads a recording and keeps its unfinished edit state on disk, so an
// interrupted edit reopens where it was left.
import { confirmDialog } from "./dialog.js";
import { noteFfmpeg } from "./videoExport.js";
import { $, afterOpsChanged, applyDefaultScale, convertFileSrc, fitZoom, fmtTime, invoke, layoutTrack, loadStrip, resetForNewSource, scheduleWaves, setZoom, t, textsReady, toast, TXT_READY, updateAudioLabels, VE, videoEl } from "./video.js";
import { addAlignedCameraOverlay, renderClipList, renderOverlayList } from "./videoOverlays.js";
import { renderRects } from "./videoPreview.js";

// ---------------------------------------------------------------- edit state
//
// Remembers unfinished edits (cuts, overlays) across closing and reopening the same recording.
// Keyed by the source file's (size, mtime) as of the load that produced these ops, so a changed
// file never matches and ops drawn against content that is gone are never restored. Only the
// video editor uses this; the screenshot editor (snipEditor.js) works on an in-memory image and
// has no file-keyed state, though the backend already accepts kind="snip".
let editStateTimer = null;
// Seconds between two autosaves of the unfinished edit state.
const EDIT_STATE_SAVE_SECS = 30;

export function stopEditStateAutosave() {
  if (editStateTimer) { clearInterval(editStateTimer); editStateTimer = null; }
}

export function armEditStateAutosave() {
  stopEditStateAutosave();
  if (!VE.editStateKeepOn) return;
  editStateTimer = setInterval(saveEditStateNow, EDIT_STATE_SAVE_SECS * 1000);
}

export async function saveEditStateNow() {
  // Nothing changed since the last export or load: the remembered row was either
  // just discarded on purpose or is already current, and re-saving would offer
  // exported work back on the next open.
  if (!VE.editStateKeepOn || !VE.src || !VE.ops.length || !VE.editStateSize || !VE.dirty) return;
  try {
    await invoke("save_edit_state", {
      kind: "video",
      target: VE.src,
      size: VE.editStateSize,
      mtime: VE.editStateMtime,
      ops: JSON.stringify({ v: 1, ops: VE.ops.filter((o) => o.tool !== "removal") }),
    });
  } catch (_) {
    // Best-effort: a failed autosave tick is not worth bothering the user; the next tick tries again.
  }
}

// Re-checks every extra video source (a section clip or a video overlay)
// still restored into VE.ops, so a moved or deleted file shows up as missing
// instead of the editor just going on as if it were still there.
async function refreshExtraSourceAvailability(myGen) {
  const extras = VE.ops.filter((o) => o.tool === "clip" || (o.tool === "overlay" && o.kind === "video"));
  for (const op of extras) {
    try {
      await invoke("stat_file", { path: op.path });
      op.missing = false;
    } catch (_) {
      op.missing = true;
    }
    if (VE.loadGen !== myGen) return;
  }
  if (extras.length) { renderClipList(); renderOverlayList(); }
}

// Called once per loadRecording(), after resetForNewSource() has already put
// VE.ops back to empty: so any ops this finds and applies are entirely the
// restored ones, never mixed with anything from the source file's own probe.
async function checkEditState(path, myGen) {
  VE.editStateSize = 0;
  VE.editStateMtime = 0;
  if (!VE.editStateKeepOn) return;
  let stat;
  try {
    stat = await invoke("stat_file", { path });
  } catch (_) {
    return; // no stat, no key to save or restore against this session
  }
  if (VE.loadGen !== myGen) return;
  if (!Array.isArray(stat) || stat.length !== 2) return; // an unreadable stat leaves no key to save or restore against
  const [size, mtime] = stat;
  VE.editStateSize = size;
  VE.editStateMtime = mtime;
  let saved;
  try {
    saved = await invoke("load_edit_state", { kind: "video", target: path, size, mtime });
  } catch (_) {
    return;
  }
  if (VE.loadGen !== myGen || !saved) return;
  let parsed;
  try {
    parsed = JSON.parse(saved);
  } catch (_) {
    parsed = null;
  }
  // Rust already filters an unreadable "v" before this ever arrives; this local
  // check costs nothing and keeps this function correct even if that changes.
  if (!parsed || parsed.v !== 1 || !Array.isArray(parsed.ops)) return;
  // The dialog's title, message and buttons are t() calls made once at open time; opening before
  // loadTexts() has landed would freeze raw i18n keys into a modal that never re-renders.
  if (!TXT_READY) await textsReady;
  if (VE.loadGen !== myGen) return;
  const ok = await askRestoreEditState(parsed.ops.length);
  if (VE.loadGen !== myGen) return;
  if (ok === true) {
    // The files its overlays and clips name were allowed by a pick in an earlier session; the
    // backend re-allows exactly those from the stored state before the previews ask for them.
    await invoke("allow_edit_state_files", { kind: "video", target: path, size, mtime }).catch(() => {});
    if (VE.loadGen !== myGen) return;
    VE.ops = parsed.ops;
    VE.floor = 0;
    VE.redo = [];
    afterOpsChanged();
    // A restored extra source that is no longer where it was gets marked missing rather than
    // silently skipped. Best-effort and never awaited, so the editor opens with a stale badge for
    // a moment instead of waiting on a stat per source.
    refreshExtraSourceAvailability(myGen);
  } else if (ok === "alt") {
    // Only the explicit "start fresh" button drops the saved state. Cancel, Escape and a click on
    // the dimmed background (the dialog opens by itself after the load) leave it for the next
    // open, and a dialog superseded by another source never gets here (the load generation check
    // above returns first).
    invoke("discard_edit_state", { kind: "video", target: path }).catch(() => {});
  }
}

function askRestoreEditState(opCount) {
  return confirmDialog({
    title: t("editStateRestoreTitle"),
    message: t("editStateRestoreMsg").replace("{count}", String(opCount)),
    confirmLabel: t("editStateRestoreContinue"),
    altLabel: t("editStateRestoreDiscard"),
    danger: false,
  });
}

export async function loadRecording(path, originDir, overlayPath) {
  if (!path) return;
  // Two "Edit video" clicks in quick succession (another recording before the first finished
  // probing/muxing) must not let the older chain's late result win, so every await below
  // re-checks this generation before touching shared state.
  const myGen = ++VE.loadGen;
  VE.src = path;
  resetForNewSource();
  // When this is set, VE.src is a throwaway copy, never the file the user
  // actually opened: destPath() already prefers this folder for the "Save" default,
  // and this note says why the working file's own name looks unfamiliar.
  VE.originDir = originDir || "";
  if (VE.originDir) toast(t("openedCopyNote"));
  $("ve-file").textContent = path.split(/[\\/]/).pop() || "";
  $("ve-file").title = path;
  $("ve-stage-msg").dataset.i18n = "veLoading";
  $("ve-stage-msg").textContent = t("veLoading");
  $("ve-stage-msg").classList.remove("hidden");
  // The element gets the file only after the probe and, for a recording with a mic
  // sidecar, the mux: the mux replaces the file, which Windows refuses while the element
  // holds it open without delete sharing.
  videoEl.removeAttribute("src");
  videoEl.load();

  let probe = null;
  try {
    probe = await invoke("probe_recording", { path });
  } catch (err) {
    noteFfmpeg(err);
  }
  if (VE.loadGen !== myGen) return;

  // A mic recording arrives as a sidecar WAV next to the mp4; the editor
  // consolidates the two the first time it opens that recording, so from then on
  // there is one file with two audio tracks. Failing here is not fatal; only the
  // mic controls stay off.
  if (probe && probe.mic_sidecar) {
    try {
      await invoke("mux_mic_track", { videoPath: path, sidecarPath: probe.mic_sidecar });
      if (VE.loadGen !== myGen) return;
      probe = await invoke("probe_recording", { path });
    } catch (err) {
      noteFfmpeg(err);
    }
  }
  if (VE.loadGen !== myGen) return;
  videoEl.src = convertFileSrc(path);
  videoEl.load();

  applyProbe(probe);
  // The key-overlay sidecar, when this recording captured one, fetched once per load like the
  // probe. Failing to read it just leaves the "keys" overlay with nothing to show.
  VE.keySidecar = null;
  if (probe && probe.has_keys_sidecar) {
    try {
      VE.keySidecar = await invoke("read_key_sidecar", { path });
    } catch {
      VE.keySidecar = null;
    }
    if (VE.loadGen !== myGen) return;
  }
  $("ve-stage-msg").classList.add("hidden");
  setZoom(fitZoom());
  // Only a refresh of the panels: a default scale applied by applyProbe leaves the editor clean.
  const dirtyBefore = VE.dirty;
  afterOpsChanged();
  VE.dirty = dirtyBefore;
  loadStrip();
  scheduleWaves();
  await checkEditState(path, myGen);
  if (VE.loadGen !== myGen) return;
  await addAlignedCameraOverlay(overlayPath, myGen);
  if (VE.loadGen === myGen) armEditStateAutosave();
}

export function applyProbe(probe) {
  VE.probe = probe;
  if (probe) {
    VE.dur = Number(probe.duration_ms) || 0;
    VE.fps = Number(probe.fps) > 0 ? Number(probe.fps) : 30;
    VE.w = Number(probe.width) || 0;
    VE.h = Number(probe.height) || 0;
    VE.hasMic = !!probe.has_mic_track;
    VE.hasSys = probe.has_audio !== false;
  }
  // Without ffmpeg the clip still plays, so the element itself supplies what it
  // knows; the frame counter then follows a nominal rate rather than the real one.
  if (!VE.dur && Number.isFinite(videoEl.duration)) VE.dur = Math.round(videoEl.duration * 1000);
  if (!VE.w) VE.w = videoEl.videoWidth || 1920;
  if (!VE.h) VE.h = videoEl.videoHeight || 1080;

  // Deferred from applySettings(), which ran before VE.w/VE.h were known --
  // consumed once here so the loadedmetadata fallback re-entry never re-applies it.
  if (VE.pendingScaleKey != null) {
    applyDefaultScale(VE.pendingScaleKey);
    VE.pendingScaleKey = null;
  }

  $("ve-duration").textContent = fmtTime(VE.dur);
  $("ve-wave-mic").classList.toggle("hidden", !VE.hasMic);
  updateAudioLabels();
  $("ve-crop-w").max = String(VE.w);
  $("ve-crop-h").max = String(VE.h);
  $("ve-crop-w").numFieldRefresh?.();
  $("ve-crop-h").numFieldRefresh?.();
  layoutTrack();
  renderRects();
}
