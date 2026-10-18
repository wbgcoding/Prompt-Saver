// Screen capture, recording, OCR and the editor hand-off.
import { $, convertFileSrc, errToast, flag, invoke, LANG, listen, OCR_LANG_NOTICE_MS, optFlag, settings, t, toast, txt, val, VOICE_LANG_NOTICE_MS } from "./core.js";
import { openFileCreate, openModal, startFileCreate, startFileFromClipboard, withDialog } from "./editor.js";
import { placePopover } from "./popover.js";
import { recordToolUsage } from "./management.js";
import { confirmDialog } from "./dialog.js";
import { fmtBytes, normRecFormat, parseFfmpegError } from "./floatShared.js";
import { EXPERT_PATHS } from "./expertMenu.js";
import { normScreenshotFormat } from "./expertConfig.js";
import { initSnipEditor } from "./snipEditor.js";
import { openLightbox } from "./windowChrome.js";
import { ocrErrToast, openOcrLangsModal } from "./dictation.js";
import { openMiniMenu } from "./views.js";
import { attachSelectPicker } from "./selectpop.js";

// ---- Colour-pick history popover (right-click on #picker-btn) ----
export const pickHistoryPop = document.createElement("div");

export function closePickHistoryPop() { pickHistoryPop.classList.add("hidden"); }

export function bindCapture() {
  $("file-btn").addEventListener("click", startFileCreate);
  $("file-btn").addEventListener("contextmenu", (e) => {
    e.preventDefault();
    startFileFromClipboard();
  });

  $("snip-btn").addEventListener("click", () => {
    invoke("open_snip").catch((err) => errToast(err));
  });

  // Pipette: the same frozen-desktop overlay in colour-pick mode. The result is a clipboard
  // value only; no prompt is created and nothing enters the history.
  $("picker-btn").addEventListener("click", () => {
    invoke("open_picker").catch((err) => errToast(err));
  });

  // Screen recording: the same overlay in record-target mode. The backend resolves the pick
  // into a window, monitor or region and notifies us with "recorder-target-picked"; a short
  // countdown (0 = none) then precedes the capture.
  $("record-btn").addEventListener("click", async () => {
    const btn = $("record-btn");
    // A glowing button stops the running recording instead of trying to start a second one.
    // recording_status confirms a session is still live first, so a stale glow (a missed event)
    // falls through to open_recorder like normal.
    if (btn.classList.contains("is-recording") || btn.classList.contains("is-paused")) {
      const status = await invoke("recording_status").catch(() => null);
      if (status) {
        // The control bar's own poll notices the session is gone and hides itself; nothing here has
        // to close it.
        try { await invoke("stop_recording"); } catch (err) { errToast(err); }
        return;
      }
      setRecordBtnState("idle");
    }
    invoke("open_recorder").catch((err) => errToast(err));
  });

  // A right-click on either composer button opens a one-item menu to edit an existing file.
  // Which editor opens is decided by the file's content (open_file_in_editor), not by the
  // button.
  const openEditorPop = document.createElement("div");
  openEditorPop.className = "ctx open-editor-pop hidden";
  openEditorPop.setAttribute("role", "menu");
  openEditorPop.innerHTML =
    '<button type="button" role="menuitem" id="open-editor-pop-item"></button>' +
    // A camera-only entry on the same popover, shown only when cameraRecord is on.
    '<button type="button" role="menuitem" id="camera-menu-item" class="hidden"></button>';
  document.body.appendChild(openEditorPop);
  function closeOpenEditorPop() {
    openEditorPop.classList.add("hidden");
  }
  // path is set only when the menu is opened for a known attachment (the library's
  // right-click); left empty it lets the user pick a file.
  let openEditorPopPath = "";
  function openOpenEditorPopAt(anchorEl, path, showCamera) {
    openEditorPopPath = path || "";
    openEditorPop.querySelector("#open-editor-pop-item").textContent = t("openFileInEditor");
    const camItem = openEditorPop.querySelector("#camera-menu-item");
    camItem.classList.toggle("hidden", !showCamera);
    if (showCamera) camItem.textContent = t("cameraMenuItem");
    placePopover(openEditorPop, anchorEl.getBoundingClientRect());
  }
  openEditorPop.querySelector("#open-editor-pop-item").addEventListener("click", () => {
    const path = openEditorPopPath;
    closeOpenEditorPop();
    openExternalFileInEditor(path || undefined);
  });
  openEditorPop.querySelector("#camera-menu-item").addEventListener("click", () => {
    closeOpenEditorPop();
    invoke("open_camera_window").catch((e) => errToast(e));
  });
  $("snip-btn").addEventListener("contextmenu", (e) => {
    e.preventDefault();
    openOpenEditorPopAt($("snip-btn"));
  });
  $("record-btn").addEventListener("contextmenu", (e) => {
    e.preventDefault();
    openOpenEditorPopAt($("record-btn"), undefined, optFlag("cameraRecord"));
  });
  // Shared by the composer menu and the library's "Open in editor" on an image/video
  // attachment; existingPath hands over a path the caller already knows.
  async function openExternalFileInEditor(existingPath) {
    const path = existingPath || (await invoke("pick_media_file_path").catch(() => null));
    if (!path) return;
    let res;
    try {
      res = await invoke("open_file_in_editor", { path });
    } catch (err) {
      const msg = String(err);
      if (msg === "not-media") toast(t("openFileNotMedia"));
      else if (msg.startsWith("too-large:")) toast(t("openFileTooBig").replace("{mb}", () => msg.split(":")[1] || ""));
      else errToast(err);
      return;
    }
    if (res.kind === "image") {
      openImageFileInEditor(res);
    } else if (res.kind === "video") {
      toast(t("openedCopyNote"));
      await invoke("open_video_editor", { srcPath: res.copied_path, originDir: res.origin_dir })
        .then(() => recordToolUsage("video_editor"))
        .catch((e) => errToast(e));
    }
  }
  // Driven by the recording state, not the click: set on a successful start, cleared on
  // stop/cancel/auto-stop, and re-synced by a slow poll so a missed event cannot leave the
  // button glowing.
  function setRecordBtnState(state) {
    const btn = $("record-btn");
    btn.classList.toggle("is-recording", state === "recording");
    btn.classList.toggle("is-paused", state === "paused");
    // Idle keeps its own data-i18n-title tooltip (with the right-click hint) and the plain name
    // as accessible name; while recording, a click stops it, so the tooltip says that.
    const idle = state === "idle";
    btn.title = idle ? t(btn.dataset.i18nTitle) : t("recBtnStopTip");
    btn.setAttribute("aria-label", idle ? t(btn.dataset.i18nAria) : t("recBtnStopTip"));
  }
  async function syncRecordBtnFromStatus() {
    // Skipped while the window is hidden (minimized to tray): the button is not visible, and
    // showing the window re-applies the real state.
    if (document.hidden) return;
    const status = await invoke("recording_status").catch(() => null);
    setRecordBtnState(!status ? "idle" : status.paused ? "paused" : "recording");
  }
  setInterval(syncRecordBtnFromStatus, 5000);
  // Catches up when the window is shown again instead of waiting out the poll interval.
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) syncRecordBtnFromStatus();
  });
  listen("recording-cancelled", () => setRecordBtnState("idle"));
  let recCountdownTimer = 0;
  function cancelRecCountdown() {
    clearTimeout(recCountdownTimer);
    recCountdownTimer = 0;
  }
  listen("recorder-target-picked", async (e) => {
    const p = e.payload || {};
    // A pick made from the running recording's own bar switches its target; recbar.js handles
    // that, so this window must not start a second recording from it.
    if (p.forSwitch) return;
    const kind = p.kind;
    let targetId = 0, regionX = 0, regionY = 0, regionW = 0, regionH = 0;
    if (kind === "window") targetId = p.id;
    else if (kind === "monitor") targetId = p.monitor;
    else if (kind === "region") {
      targetId = p.monitor;
      regionX = p.x; regionY = p.y; regionW = p.width; regionH = p.height;
    } else return;

    const startNow = async () => {
      try {
        await invoke("start_recording", { targetKind: kind, targetId, regionX, regionY, regionW, regionH });
        recordToolUsage("recording_start");
        setRecordBtnState("recording");
      } catch (err) {
        // A refused start leaves no recording: close any bar and reset the record button before the
        // single error toast. "recording-busy" means one is still running (a continue is switching
        // its file), so its bar and button stay.
        if (!String(err).includes("recording-busy")) {
          invoke("hide_rec_bar").catch(() => {});
          setRecordBtnState("idle");
        }
        errToast(err);
        return;
      }
      // The recording runs from here on. A bar that fails to open only costs the
      // controls, so the state stays and the person is told.
      try {
        await invoke("show_rec_bar", { targetKind: kind, id: targetId });
      } catch (err) {
        errToast(err);
      }
    };
    // Checked before the countdown, so a low-disk "no" does not come after a 3-2-1 count.
    const diskWarn = await invoke("check_recording_disk_space").catch(() => null);
    if (diskWarn) {
      const minutes = Math.max(1, Math.round(diskWarn.est_seconds / 60));
      const proceed = await confirmDialog({
        title: t("recDiskWarnTitle"),
        message: t("recDiskWarnMsg")
          .replace("{drive}", () => diskWarn.drive)
          .replace("{free}", fmtBytes(diskWarn.free_mb * 1024 * 1024, LANG))
          .replace("{minutes}", String(minutes)),
        confirmLabel: t("recDiskWarnRecordAnyway"),
      });
      if (!proceed) return;
    }
    cancelRecCountdown();
    let n = Math.round(val("recCountdownS"));
    if (n <= 0) { await startNow(); return; }
    const tick = () => {
      toast(t("recCountdownToast").replace("{n}", () => n));
      n -= 1;
      if (n < 0) { recCountdownTimer = 0; startNow(); return; }
      recCountdownTimer = setTimeout(tick, 1000);
    };
    tick();
  });

  // ---- Recording result: mirrors the #snip-modal flow, plus an optional conversion step
  // (recFormat/recQuality other than the raw mp4 capture). "recording-captured" is emitted by
  // stop_recording (and the auto-stop path when the recorded window closes); the rec-bar is its
  // own window, so this is how the main window learns a file is ready.
  const recModal = $("rec-modal");
  const recPreview = $("rec-preview");
  const recStatus = $("rec-modal-status");
  const recFolderOut = $("rec-modal-folder");
  const recFolderChangeBtn = $("rec-modal-folder-change");
  const recMoreBtn = $("rec-modal-more");
  const recMorePop = $("rec-modal-more-pop");
  // The menu behind "More...": editing, Save as, Copy and the save folder.
  function closeRecMorePop() {
    if (recMorePop.classList.contains("hidden")) return;
    const hadFocus = recMorePop.contains(document.activeElement);
    recMorePop.classList.add("hidden");
    recMoreBtn.setAttribute("aria-expanded", "false");
    if (hadFocus) recMoreBtn.focus();
  }
  let pendingRecPath = "";
  // The backend sweep spares the file this dialog shows; every change of the
  // path is reported, and the unload clears it.
  function setPendingRec(path) {
    pendingRecPath = path;
    invoke("set_pending_recording", { path: path || null }).catch(() => {});
  }
  window.addEventListener("pagehide", () => { invoke("set_pending_recording", { path: null }).catch(() => {}); });
  // The running crop/conversion of the open dialog and the converted file a closed dialog's
  // conversion finished anyway, so a close can wait for ffmpeg to release the take before
  // moving it away.
  let recWork = Promise.resolve();
  let recLateOutput = "";
  let recGen = 0; // invalidated on every close/discard so a late progress tick or a
                   // finished conversion from a superseded recording is ignored
  let recProgressOff = null;
  // Same "closed by default, raw text only under Technical details" treatment as the video
  // editor's export error row.
  function showRecErrorDetails(raw) {
    const box = $("rec-modal-error-details");
    box.classList.toggle("hidden", !raw);
    box.open = false;
    $("rec-modal-error-raw").textContent = raw || "";
  }
  $("rec-modal-error-copy").addEventListener("click", () => {
    invoke("copy_text", { text: $("rec-modal-error-raw").textContent }).then(() => toast(t("copied"))).catch(() => {});
  });
  // Closing the result window without Save/Save as/Attach deletes the recording; Discard, the X
  // button, the backdrop and Escape all end up in closeRecModal(). Copy alone does not count as
  // saving (it only relocates the file so the clipboard reference survives), so a copy-then-close
  // still removes the working file; recCopiedNotSaved gates the confirm-before-delete for that.
  let recSavedOrAttached = false;
  let recCopiedNotSaved = false;
  // The earlier parts of a "finish and continue" take go where the last file
  // went; the toast names only what really got saved.
  async function keepRecParts(besideSaveAs) {
    try {
      const [saved, failed] = await invoke("save_recording_parts", { besideSaveAs });
      if (saved > 0) toast(t("recContinuePartsSaved").replace("{n}", String(saved)));
      if (failed > 0) toast(t("recPartsNotSaved").replace("{n}", String(failed)));
    } catch (err) {
      errToast(err);
    }
  }
  // Elide a long path in the middle: the last segment (the folder name) tells destinations
  // apart and must stay visible; the title attribute carries the full path.
  function elideMiddle(s, max = 60) {
    if (s.length <= max) return s;
    const head = Math.ceil((max - 1) / 2);
    return s.slice(0, head) + "…" + s.slice(s.length - (max - 1 - head));
  }
  function renderRecFolder() {
    const dir = EXPERT_PATHS.recordingDir.get();
    recFolderOut.textContent = elideMiddle(dir);
    recFolderOut.title = dir;
  }
  // Once saved or attached, nothing reaching this function may delete the file. A real Discard
  // click and an implicit close (X, backdrop, Escape) before any Save/Attach/Edit both mean
  // "throw this away", so they share one path.
  async function closeRecModal() {
    const path = pendingRecPath;
    const shouldDelete = !recSavedOrAttached;
    // Copy alone does not count as saving, but it moved the file to where the clipboard reference
    // points; closing unconfirmed would delete that file under the live reference. Ask before
    // anything else changes, so declining leaves the modal untouched.
    if (shouldDelete && path && recCopiedNotSaved) {
      const ok = await confirmDialog({
        title: t("recDiscard"),
        message: t("recClosedAfterCopy"),
        confirmLabel: t("recDiscard"),
        cancelLabel: t("cancel"),
      });
      if (!ok) return;
    }
    recGen += 1;
    recLateOutput = "";
    const work = recWork;
    invoke("cancel_recording_convert").catch(() => {});
    invoke("cancel_recording_crop").catch(() => {});
    if (recProgressOff) { recProgressOff(); recProgressOff = null; }
    closeRecMorePop();
    recModal.classList.add("hidden");
    recPreview.classList.add("hidden");
    recPreview.pause();
    recPreview.removeAttribute("src");
    recPreview.load(); // removing the attribute alone neither stops playback nor frees the file
    recStatus.classList.remove("rec-status-cancellable");
    recStatus.onclick = null;
    setPendingRec("");
    recSavedOrAttached = false;
    recCopiedNotSaved = false;
    if (shouldDelete) await work.catch(() => {});
    if (shouldDelete && path) {
      // A number back means it moved to "verworfen" (see discard_recording_file); null or failure
      // stays silent.
      const keepH = await invoke("discard_recording_file", { path }).catch(() => null);
      if (keepH != null) toast(t("recDiscardedToast").replace("{hours}", String(Math.round(keepH))));
    }
    // A conversion that finished before the cancel took hold left its output behind.
    const late = recLateOutput;
    recLateOutput = "";
    if (shouldDelete && late && late !== path) invoke("discard_recording_file", { path: late }).catch(() => {});
    if (shouldDelete) invoke("discard_recording_parts").catch(() => {});
  }
  // needsCrop: the window recording's canvas was bigger than the window ever was (a
  // multi-display canvas). Trim the black margin first, with a visible, cancellable status line
  // like the format conversion, before the raw preview or the conversion. A cancel or failure
  // keeps the untouched, letterboxed recording without an error toast.
  async function runRecCrop(rawPath, gen) {
    let secs = 0;
    const showCropping = () => { recStatus.textContent = t("recCropping").replace("{s}", String(secs)); };
    showCropping();
    recStatus.classList.add("rec-status-cancellable");
    recStatus.onclick = () => { if (gen === recGen) invoke("cancel_recording_crop").catch(() => {}); };
    recProgressOff = await listen("recording-crop-progress", (e) => {
      if (gen !== recGen) return;
      secs = Math.floor((e.payload?.ms || 0) / 1000);
      showCropping();
    });
    const myOff = recProgressOff;
    if (gen === recGen) {
      try {
        await invoke("crop_recording_to_used_area", { path: rawPath });
      } catch (e) {
        if (e === "ffmpeg-missing") toast(t("recCropSkippedNoTools"));
      }
    }
    myOff();
    if (recProgressOff === myOff) recProgressOff = null;
    if (gen !== recGen) return;
    recStatus.classList.remove("rec-status-cancellable");
    recStatus.onclick = null;
    recStatus.textContent = "";
  }
  // Save, Save as, Copy, Attach and the video editor all act on pendingRecPath. While the crop
  // or conversion still rewrites that file they must not run, or one would adopt another path
  // just before the conversion answers and overwrites it.
  const REC_ACTION_IDS = ["rec-modal-save", "rec-modal-saveas", "rec-modal-copy", "rec-modal-yes", "rec-modal-edit"];
  function setRecActionsBusy(busy) {
    for (const id of REC_ACTION_IDS) $(id).disabled = busy;
  }
  async function openRecModal(rawPath, needsCrop) {
    setPendingRec(rawPath);
    recLateOutput = "";
    recSavedOrAttached = false;
    recCopiedNotSaved = false;
    recGen += 1;
    const gen = recGen;
    setRecActionsBusy(false);
    renderRecFolder();
    $("rec-modal-edit").classList.toggle("hidden", !flag("videoEditor"));
    recModal.classList.remove("hidden");
    // recAutoCrop off means the user chose to keep the border: skip the cropping step and its
    // status line.
    const outputChoice = () => {
      const format = normRecFormat(settings.ui_texts?.recFormat);
      const lossless = (settings.ui_texts?.recQuality || "high") === "lossless";
      return { format, lossless, converting: !(format === "mp4" && !lossless) };
    };
    const cropping = needsCrop && flag("recAutoCrop");
    if (cropping || outputChoice().converting) setRecActionsBusy(true);
    try {
      if (cropping) {
        recPreview.classList.add("hidden");
        showRecErrorDetails("");
        await runRecCrop(rawPath, gen);
        if (gen !== recGen) return;
      }
      // Read after the crop: the choice may have changed while it ran.
      const { format, lossless, converting } = outputChoice();
      await convertRecording(rawPath, gen, format, lossless, converting);
    } finally {
      // Only the dialog still on screen is unlocked: a superseded call must not
      // undo the lock of the newer one.
      if (gen === recGen) setRecActionsBusy(false);
    }
  }
  async function convertRecording(rawPath, gen, format, lossless, converting) {
    if (!converting) {
      recPreview.classList.remove("hidden");
      recPreview.src = convertFileSrc(rawPath);
      recStatus.textContent = "";
      return;
    }
    recPreview.classList.add("hidden");
    showRecErrorDetails("");
    let secs = 0;
    const showConverting = () => { recStatus.textContent = t("recConverting").replace("{s}", String(secs)); };
    showConverting();
    recStatus.classList.add("rec-status-cancellable");
    recStatus.onclick = () => { if (gen === recGen) invoke("cancel_recording_convert").catch(() => {}); };
    recProgressOff = await listen("recording-convert-progress", (e) => {
      if (gen !== recGen) return;
      secs = Math.floor((e.payload?.ms || 0) / 1000);
      showConverting();
    });
    const myOff = recProgressOff;
    if (gen !== recGen) {
      // The modal was closed, discarded or attached while this await was in flight: do not start a
      // conversion nobody will see, and do not race what that close did with the file.
      myOff();
      if (recProgressOff === myOff) recProgressOff = null;
      return;
    }
    try {
      const outPath = await invoke("convert_recording", { srcPath: rawPath, format, lossless });
      if (gen !== recGen) { recLateOutput = outPath; return; }
      recStatus.classList.remove("rec-status-cancellable");
      recStatus.onclick = null;
      recStatus.textContent = "";
      // Another action already moved the working file elsewhere: its path stands, the
      // converted copy must not overwrite it.
      if (pendingRecPath !== rawPath) return;
      setPendingRec(outPath);
      recPreview.classList.remove("hidden");
      recPreview.src = convertFileSrc(outPath);
    } catch (err) {
      if (gen !== recGen) return;
      recStatus.classList.remove("rec-status-cancellable");
      recStatus.onclick = null;
      // Cancelled by the user, ffmpeg missing, or a real failure: the untouched raw recording is
      // still there to fall back to, and only a real failure gets the "Technical details" row.
      const raw = String(err);
      if (raw.trim() === "cancelled") {
        recStatus.textContent = t("recConvertCancelled");
      } else if (raw.includes("ffmpeg-missing")) {
        recStatus.textContent = `${t("recFfmpegMissingHint")} ${t("recConvertFailedKeepRaw")}`;
      } else {
        const { message, details } = parseFfmpegError(err, t, "main");
        recStatus.textContent = `${message} ${t("recConvertFailedKeepRaw")}`;
        showRecErrorDetails(details);
      }
      recPreview.classList.remove("hidden");
      recPreview.src = convertFileSrc(rawPath);
    } finally {
      // Unlisten this call's own listener, never whatever the global points at, so a superseded
      // call's finally cannot tear down a newer call's listener.
      myOff();
      if (recProgressOff === myOff) recProgressOff = null;
    }
  }
  // The periodic sweep and "once a recording ends" both fire this live; a toast only when it
  // removed something.
  listen("recording-sweep-done", (e) => {
    const { count, bytes } = e.payload || {};
    if (count > 0) toast(t("recSweepDone").replace("{n}", String(count)).replace("{v}", fmtBytes(bytes, LANG)));
  });
  // The control bar's target-switch button lives in a window with no toast of its own. A
  // failed switch leaves the previous source recording; this is the "here is why" note.
  listen("recording-target-switch-failed", (e) => errToast(e.payload));
  listen("recording-captured", async (e) => {
    setRecordBtnState("idle"); // covers both a normal stop and the auto-stop path
    const path = e.payload?.path || "";
    // An encoder write failure (disk full, etc.) already stopped the feeder before this event;
    // whatever was captured is a valid, playable file, just cut short. "low-disk-space" has its
    // own message: the disk-space watch stopped the recording before the encoder hit the wall.
    if (e.payload?.encode_error === "low-disk-space") toast(t("recDiskStoppedMsg"));
    else if (e.payload?.encode_error === "max-limit-reached") toast(t("recStoppedByLimit"));
    else if (e.payload?.encode_error) toast(t("recEncodeError"));
    // A window recording whose canvas fell back below the "every display" default (too large for
    // the encoder, or a smaller mode picked) says so once; the default case stays silent.
    if (e.payload?.canvas_note === "monitor") toast(t("recCanvasFallbackMonitor"));
    else if (e.payload?.canvas_note === "window") toast(t("recCanvasFallbackWindow"));
    // The "window size" canvas default cut off part of the picture because the window grew
    // mid-recording: offer, once per occurrence and only while the notice is on, to switch the
    // canvas to every display. Runs before the result menu opens, never instead of it.
    const cropMs = e.payload?.resize_crop_at_ms;
    if (cropMs != null && flag("recCropNotice")) {
      const secs = Math.floor(cropMs / 1000);
      toast(t("recCropNoticeMsg").replace("{s}", String(secs)), {
        label: t("recCropNoticeSwitch"),
        onClick: () => {
          settings.ui_texts = { ...(settings.ui_texts || {}), recCanvasMode: "all" };
          invoke("set_ui_text", { key: "recCanvasMode", value: "all" }).catch(() => {});
        },
      }, "", null, 12000);
    }
    // "Finish and continue" split this recording into several files. The
    // dialog opens the LAST one; the backend keeps the earlier parts and they
    // follow it on Save, Attach or Edit and go to the discard folder with it.
    if (path) recWork = openRecModal(path, !!e.payload?.needs_crop);
  });
  $("rec-modal-close").addEventListener("click", () => closeRecModal());
  recModal.addEventListener("pointerdown", (e) => {
    if (e.target === recModal) closeRecModal();
  });
  $("rec-modal-discard").addEventListener("click", () => closeRecModal());
  recMoreBtn.addEventListener("click", () => {
    if (!recMorePop.classList.contains("hidden")) { closeRecMorePop(); return; }
    placePopover(recMorePop, recMoreBtn.getBoundingClientRect());
    recMoreBtn.setAttribute("aria-expanded", "true");
    recMorePop.querySelector("button:not(.hidden)")?.focus();
  });
  // Capture phase: any menu item closes the menu first, then runs its own listener.
  recMorePop.addEventListener("click", (e) => { if (e.target.closest("button")) closeRecMorePop(); }, true);
  document.addEventListener("pointerdown", (e) => {
    if (recMorePop.classList.contains("hidden")) return;
    if (recMorePop.contains(e.target) || recMoreBtn.contains(e.target)) return;
    closeRecMorePop();
  });
  // Opening the editor hands the raw file to another window that owns it from then on. Treat
  // this like Save/Attach so closing the modal never deletes a file the editor is reading, and
  // close it at once so no Save/Save as/Copy here can move the file out from under the editor.
  $("rec-modal-edit").addEventListener("click", async () => {
    if (!pendingRecPath || recSaveBusy || recSaveAsBusy || recCopyBusy) return;
    try {
      const taken = await invoke("open_video_editor", { srcPath: pendingRecPath });
      // The open editor kept its unsaved work: this take stays here to decide on.
      if (taken === false) return;
      recordToolUsage("video_editor");
      recSavedOrAttached = true;
      keepRecParts(false);
      closeRecModal();
    } catch (err) {
      errToast(err);
    }
  });
  recFolderChangeBtn.addEventListener("click", async () => {
    await EXPERT_PATHS.recordingDir.change();
    renderRecFolder();
  });
  // Update the preview and pending path to a command's returned location. Save, Save as and
  // Copy move the file, so a later action in the same open window must act on where it ended
  // up, not on the stale original path.
  function adoptRecPath(newPath) {
    if (!newPath || newPath === pendingRecPath) return;
    setPendingRec(newPath);
    if (!recPreview.classList.contains("hidden")) recPreview.src = convertFileSrc(newPath);
  }
  let recSaveBusy = false;
  $("rec-modal-save").addEventListener("click", async () => {
    if (!pendingRecPath || recSaveBusy) return;
    recSaveBusy = true;
    try {
      const newPath = await invoke("save_recording_now", { srcPath: pendingRecPath });
      adoptRecPath(newPath);
      recSavedOrAttached = true;
      toast(t("recSaved"));
      await keepRecParts(false);
    } catch (err) {
      errToast(err);
    } finally {
      recSaveBusy = false;
    }
  });
  let recSaveAsBusy = false;
  $("rec-modal-saveas").addEventListener("click", async () => {
    if (!pendingRecPath || recSaveAsBusy) return;
    recSaveAsBusy = true;
    try {
      const newPath = await invoke("save_recording_as", { srcPath: pendingRecPath }).catch((err) => { errToast(err); return null; });
      if (newPath) {
        adoptRecPath(newPath);
        recSavedOrAttached = true;
        await keepRecParts(true);
      }
    } finally {
      recSaveAsBusy = false;
    }
  });
  let recCopyBusy = false;
  $("rec-modal-copy").addEventListener("click", async () => {
    if (!pendingRecPath || recCopyBusy) return;
    recCopyBusy = true;
    try {
      // The file is relocated before the backend attempts the clipboard write, so the new path is
      // adopted whether or not the write succeeded; otherwise this window would point at a path
      // that no longer exists.
      const [newPath, clipOk] = await invoke("copy_recording_to_clipboard", { srcPath: pendingRecPath });
      adoptRecPath(newPath);
      if (clipOk) {
        recCopiedNotSaved = !recSavedOrAttached;
        toast(t("copied"));
      } else {
        errToast("clipboard write failed");
      }
    } catch (err) {
      errToast(err);
    } finally {
      recCopyBusy = false;
    }
  });
  $("rec-modal-yes").addEventListener("click", () => {
    const path = pendingRecPath;
    // Reuses the path a picked or dropped video already takes. Attaching counts as saving: mark
    // it before closeRecModal() so the close does not delete the file under the new editor.
    recSavedOrAttached = true;
    keepRecParts(false);
    closeRecModal();
    if (path) openFileCreate(path);
  });

  // Right-click reaches the picker's history without opening the overlay; it lives in the
  // backend (survives a restart) and this popover just displays it.
  function renderPickHistoryPop(hex) {
    pickHistoryPop.innerHTML = "";
    if (!hex.length) {
      const empty = document.createElement("div");
      empty.className = "pick-history-empty";
      empty.textContent = t("pickHistoryEmpty");
      pickHistoryPop.appendChild(empty);
    } else {
      for (const h of hex) {
        const row = document.createElement("button");
        row.type = "button";
        row.className = "pick-history-row";
        const sw = document.createElement("span");
        sw.className = "pick-history-swatch";
        sw.style.background = h;
        const label = document.createElement("span");
        label.className = "pick-history-hex";
        label.textContent = h;
        row.append(sw, label);
        row.addEventListener("click", async () => {
          closePickHistoryPop();
          try {
            const text = await invoke("copy_pick_history_entry", { hex: h });
            toast(t("colorCopied"), null, text);
          } catch (err) { errToast(err); }
        });
        pickHistoryPop.appendChild(row);
      }
    }
  }
  $("picker-btn").addEventListener("contextmenu", async (e) => {
    e.preventDefault();
    let hex = [];
    try { hex = (await invoke("pick_history")) || []; } catch { /* history stays empty */ }
    renderPickHistoryPop(hex);
    placePopover(pickHistoryPop, $("picker-btn").getBoundingClientRect());
  });
  listen("color-picked", async (e) => {
    const value = e.payload?.text || "";
    if (!value) return;
    // The picked notation is already on the clipboard; these chips re-copy the other notations.
    const chips = ["hex", "rgb", "hsl"]
      .map((k) => e.payload?.[k])
      .filter(Boolean)
      .map((v) => ({
        label: v,
        // The notation already on the clipboard is marked instead of repeated in the message, which
        // would push the row onto a second line.
        current: v === value,
        onClick: () => invoke("copy_text", { text: v }).catch((err) => errToast(err)),
      }));
    toast(t("colorCopied"), null, e.payload?.hex || "", chips);
    // Live-update the recent-colours popover if it was left open across a pick.
    if (!pickHistoryPop.classList.contains("hidden")) {
      try { renderPickHistoryPop((await invoke("pick_history")) || []); } catch { /* leave it as-is on a read hiccup */ }
    }
  });

  // Switching to another program never touches a chain, locked or not: it ends only through
  // the chain button or Ctrl+V pressed inside this window.
  const { snipEditOpen, snipEdit, snipEditState, snipEditCancel } = initSnipEditor();

  const snipModal = $("snip-modal");
  let pendingSnip = null;
  let pendingSnipPath = ""; // disk path if this shot was auto-saved to the folder
  // Bumped by every new capture: an async save finishing after a newer shot arrived must not
  // touch that shot's state or files.
  let snipCaptureGen = 0;
  let pendingSnipName = ""; // backend-suggested "Screenshot <app?> <timestamp>"
  // Set only for a file opened for editing from outside the app: the folder "Save as..."
  // defaults into, since the original is never touched. Empty for an ordinary capture.
  let pendingSnipOriginDir = "";
  const closeSnipModal = () => {
    snipModal.classList.add("hidden");
    pendingSnip = null;
    pendingSnipPath = "";
    pendingSnipName = "";
    pendingSnipOriginDir = "";
  };
  listen("snip-captured", async (e) => {
    const shot = e.payload?.data_url || "";
    // A shot arriving over an open editor must not wipe the open session's state before the user
    // agrees: ask first when it holds edits, then run that session's own cancel.
    if (shot && flag("snipEditor") && snipEditState().active) {
      if (snipEditState().dirty) {
        const ok = await confirmDialog({
          title: t("snipReplaceTitle"),
          message: t("snipReplaceMsg"),
          confirmLabel: t("snipReplaceOk"),
          cancelLabel: t("snipReplaceKeep"),
          danger: false,
        });
        if (!ok) return; // the new shot stays as every capture does (clipboard, folder)
      }
      if (snipEditState().active) snipEditCancel();
    }
    pendingSnip = shot;
    if (!pendingSnip) return;
    snipCaptureGen++;
    recordToolUsage("screenshot");
    pendingSnipPath = e.payload?.path || "";
    pendingSnipName = e.payload?.name || "";
    pendingSnipOriginDir = "";
    if (!flag("snipEditor")) {
      // Editor off: the unedited shot goes straight into the result dialog.
      $("snip-preview").src = pendingSnip;
      $("snip-modal-retry").classList.remove("hidden"); // a fresh capture can be retaken
      $("snip-save-folder").checked = optFlag("screenshotSave");
      $("snip-modal-saveas-format").value = normScreenshotFormat(txt("screenshotFormat"));
      snipModal.classList.remove("hidden");
      return;
    }
    // The editor sits between the capture and the result dialog.
    snipEditOpen(pendingSnip, {
      onApply: async (editedDataUrl) => {
        pendingSnip = editedDataUrl;
        // finalize_capture already put the unedited image on the clipboard (and optionally on disk);
        // redo both side effects on the edited result.
        invoke("copy_image_data", { dataUrl: editedDataUrl }).catch((err) => errToast(err));
        let saveFailed = false;
        if (optFlag("screenshotSave")) {
          // Save the edited copy first and only then remove the unedited file, so a failed save
          // (unwritable folder, full disk, too large) never leaves neither on disk.
          const oldPath = pendingSnipPath;
          const gen = snipCaptureGen;
          const saved = (await invoke("save_screenshot_now", { dataUrl: editedDataUrl }).catch(() => "")) || "";
          if (saved) {
            // The same timestamp name overwrites the old file in place; then the old path is the new one
            // and must stay.
            if (oldPath && oldPath !== saved) await invoke("delete_screenshot_file", { path: oldPath }).catch(() => {});
            // A newer capture arrived while saving: it owns the state and the dialog now.
            if (gen !== snipCaptureGen) return;
            pendingSnipPath = saved;
          } else {
            if (gen !== snipCaptureGen) return;
            // The unedited file stays where it is; the dialog's shot is simply not saved.
            pendingSnipPath = "";
            saveFailed = true;
            toast(t("snipSaveFailed"));
          }
        }
        $("snip-preview").src = pendingSnip;
        $("snip-save-folder").checked = optFlag("screenshotSave") && !saveFailed;
        $("snip-modal-saveas-format").value = normScreenshotFormat(txt("screenshotFormat"));
        $("snip-modal-retry").classList.remove("hidden");
        snipModal.classList.remove("hidden");
      },
      // Cancel behaves like closing the result dialog: the unedited shot already on the clipboard
      // or in the folder stays as is.
      onCancel: () => {
        pendingSnip = null;
        pendingSnipPath = "";
        pendingSnipName = "";
      },
    });
  });
  // An existing image opened from the composer buttons' right-click menu or the library. Uses
  // the same editor and result dialog as a capture, but the source file is a copy: no
  // clipboard write, no auto-save, and "Save as..." defaults to the origin file's folder and
  // name.
  function openImageFileInEditor(res) {
    snipEditOpen(res.data_url, {
      onApply: (editedDataUrl) => {
        pendingSnip = editedDataUrl;
        pendingSnipPath = "";
        pendingSnipName = res.name || "";
        pendingSnipOriginDir = res.origin_dir || "";
        toast(t("openedCopyNote"));
        $("snip-preview").src = pendingSnip;
        $("snip-save-folder").checked = false;
        $("snip-modal-saveas-format").value = normScreenshotFormat(txt("screenshotFormat"));
        $("snip-modal-retry").classList.add("hidden"); // "retry" means recapture -- meaningless for an opened file
        snipModal.classList.remove("hidden");
      },
      onCancel: () => {},
    });
  }
  // Folder toggle on the result dialog = the same setting as in the main settings. On: save
  // this shot now; off: delete the saved copy.
  $("snip-save-folder").addEventListener("change", async (ev) => {
    const on = ev.target.checked;
    settings.ui_flags = { ...(settings.ui_flags || {}), screenshotSave: on };
    invoke("set_ui_flag", { key: "screenshotSave", enabled: on }).catch((err) => errToast(err));
    const optBox = $("opt-screenshot-folder");
    if (optBox) optBox.checked = on;
    if (on) {
      if (!pendingSnipPath && pendingSnip) {
        const gen = snipCaptureGen;
        const savedNow = (await invoke("save_screenshot_now", { dataUrl: pendingSnip }).catch(() => "")) || "";
        if (gen !== snipCaptureGen) return; // a newer capture owns the state now
        if (!ev.target.checked) {
          // Switched off while the save ran: the off branch found no saved copy to remove, so this one
          // must.
          if (savedNow) {
            const removed = await invoke("delete_screenshot_file", { path: savedNow }).catch(() => false);
            if (!removed && gen === snipCaptureGen) pendingSnipPath = savedNow;
          }
          return;
        }
        pendingSnipPath = savedNow;
        if (!pendingSnipPath) {
          // Nothing reached the disk: say so and put the switch back off, setting included.
          toast(t("snipSaveFailed"));
          ev.target.checked = false;
          settings.ui_flags = { ...(settings.ui_flags || {}), screenshotSave: false };
          invoke("set_ui_flag", { key: "screenshotSave", enabled: false }).catch((err) => errToast(err));
          if (optBox) optBox.checked = false;
        }
      }
    } else if (pendingSnipPath) {
      // The command answers false (it does not reject) when the file could not be removed: the
      // path is still the saved copy, and turning the switch back on must not write a second one.
      const removed = await invoke("delete_screenshot_file", { path: pendingSnipPath }).catch(() => false);
      if (removed) pendingSnipPath = "";
    }
  });
  $("snip-modal-saveas").addEventListener("click", async () => {
    if (!pendingSnip) return;
    const format = $("snip-modal-saveas-format").value || undefined;
    await invoke("save_screenshot_as", {
      dataUrl: pendingSnip,
      format,
      defaultDir: pendingSnipOriginDir || undefined,
      defaultName: pendingSnipOriginDir ? pendingSnipName : undefined,
    }).catch((err) => errToast(err));
  });
  $("snip-modal-yes").addEventListener("click", () => {
    const image = pendingSnip;
    const name = pendingSnipName || t("snipTitle");
    closeSnipModal();
    if (!image) return;
    // The screenshot becomes a button that copies the image on click.
    openModal({
      mode: "image-create",
      title: t("imageModalTitle"),
      name,
      image,
      showImage: true,
      copyImage: true,
    });
  });
  // Discard this shot and start a new one; hidden for a file opened from elsewhere, where a
  // fresh capture makes no sense.
  $("snip-modal-retry").addEventListener("click", () => {
    if (pendingSnipOriginDir) { closeSnipModal(); return; }
    closeSnipModal();
    invoke("open_snip").catch((err) => errToast(err));
  });
  $("snip-modal-close").addEventListener("click", closeSnipModal);
  $("snip-preview").addEventListener("click", () => { const s = $("snip-preview").src; if (s) openLightbox(s, false); });
  snipModal.addEventListener("pointerdown", (e) => {
    if (e.target === snipModal) closeSnipModal();
  });

  // ---- OCR (text recognition, on by default: flag "ocr") ----
  const ocrModal = $("ocr-modal");
  const ocrSettings = () => ({
    upscaleUnderPx: 300, // crops under this size are upscaled before recognition
    joinLines: flag("ocrJoinLines"),
    lang: txt("ocrLang") || "auto",
    autoPickLang: true,
  });
  const closeOcrModal = () => ocrModal.classList.add("hidden");
  // Shared by all entry points (snip-modal, composer screenshot, composer file) and the
  // ocr-captured event: one place decides "empty -> toast only" and "auto-copy -> copy now +
  // footer note".
  function showOcrResult(result) {
    const text = (result?.text || "").trim();
    if (!text) {
      toast(t("ocrNoTextFound"), { label: t("ocrLangsManage"), onClick: openOcrLangsModal });
      return;
    }
    const autoCopy = flag("ocrAutoCopy");
    if (autoCopy) invoke("copy_text", { text }).catch(() => {});
    $("ocr-result-text").value = text;
    $("ocr-result-meta").textContent = `${result.lang || ""} · ${text.length}`;
    $("ocr-autocopy-note").classList.toggle("hidden", !autoCopy);
    ocrModal.classList.remove("hidden");
  }
  // (a) "Recognize text" in #snip-modal runs on the capture shown there. pendingSnip holds a
  // data URL at this point (possibly not on disk yet, see the folder-toggle handler), hence
  // ocr_image_data.
  $("snip-modal-ocr").addEventListener("click", async () => {
    if (!pendingSnip) return;
    try {
      showOcrResult(await invoke("ocr_image_data", { dataUrl: pendingSnip, ...ocrSettings() }));
      recordToolUsage("ocr");
    } catch (err) { ocrErrToast(err); }
  });
  // (b) standalone composer button: small menu (Screenshot / From image file).
  $("ocr-btn").addEventListener("click", () => {
    const r = $("ocr-btn").getBoundingClientRect();
    openMiniMenu([
      {
        label: t("ocrMenuFromFile"),
        run: async () => {
          const path = await withDialog(() => invoke("pick_file_path"));
          if (!path) return;
          try {
            showOcrResult(await invoke("ocr_image", { path, ...ocrSettings() }));
            recordToolUsage("ocr");
          } catch (err) { ocrErrToast(err); }
        },
      },
      {
        label: t("ocrMenuScreenshot"),
        run: async () => {
          // Arms the next capture for OCR instead of a normal save; the backend resets this when the
          // session ends (capture or cancel).
          await invoke("set_snip_mode", { mode: "ocr" }).catch(() => {});
          invoke("open_snip").catch((err) => errToast(err));
        },
      },
    ], r.left, r.bottom + 4);
  });
  // Screenshot-into-OCR result, emitted by capture_region's OCR branch; a separate event from
  // "snip-captured" so the normal flow is unaffected.
  listen("ocr-captured", (e) => showOcrResult(e.payload));
  // A background thread found installed language packs that need updating (a version change,
  // or Windows dropped one after a feature update). Only a notice: the backend never
  // auto-installs, so an ignored toast is a complete no-op. Cancel snoozes the next automatic
  // check.
  listen("ocr-lang-update-due", (e) => {
    const due = e.payload || [];
    if (!due.length) return;
    const msg = due.length === 1
      ? t("ocrLangUpdateOne").replace("{lang}", () => due[0].label)
      : t("ocrLangUpdateMany").replace("{n}", String(due.length));
    toast(
      msg,
      { label: t("cancel"), onClick: () => invoke("ocr_lang_snooze").catch(() => {}) },
      "", null,
      OCR_LANG_NOTICE_MS,
    );
  });
  // Same background-watcher notice, for voice language packs.
  listen("voice-lang-update-due", (e) => {
    const due = e.payload || [];
    if (!due.length) return;
    const msg = due.length === 1
      ? t("ocrLangUpdateOne").replace("{lang}", () => due[0].label)
      : t("ocrLangUpdateMany").replace("{n}", String(due.length));
    toast(
      msg,
      { label: t("cancel"), onClick: () => invoke("voice_lang_snooze").catch(() => {}) },
      "", null,
      VOICE_LANG_NOTICE_MS,
    );
  });

  $("ocr-modal-copy").addEventListener("click", () => {
    const text = $("ocr-result-text").value;
    invoke("copy_text", { text }).then(() => toast(t("copied"))).catch((err) => errToast(err));
  });
  $("ocr-modal-create").addEventListener("click", () => {
    const text = $("ocr-result-text").value;
    closeOcrModal();
    openModal({ mode: "create", text, title: t("nameModalTitle") });
  });
  $("ocr-modal-close").addEventListener("click", closeOcrModal);
  $("ocr-modal-close-x").addEventListener("click", closeOcrModal);
  ocrModal.addEventListener("pointerdown", (e) => {
    if (e.target === ocrModal) closeOcrModal();
  });

  attachSelectPicker($("snip-modal-saveas-format"));
  return { openEditorPop, closeOpenEditorPop, openExternalFileInEditor, closeRecMorePop, closeRecModal, closeSnipModal, snipEdit };
}

export function wireCapture() {
  pickHistoryPop.className = "pick-history-pop hidden";
  document.body.appendChild(pickHistoryPop);
  document.addEventListener("pointerdown", (e) => {
    if (pickHistoryPop.classList.contains("hidden")) return;
    if (!pickHistoryPop.contains(e.target) && !e.target.closest("#picker-btn")) {
      closePickHistoryPop();
    }
  });
}
