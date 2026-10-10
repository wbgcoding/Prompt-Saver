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
  // Paperclip button in the composer bar (files, images, gifs, videos).
  $("file-btn").addEventListener("click", startFileCreate);
  $("file-btn").addEventListener("contextmenu", (e) => {
    e.preventDefault();
    startFileFromClipboard();
  });

  // Snipping tool: open the overlay; the backend notifies us with the crop.
  $("snip-btn").addEventListener("click", () => {
    invoke("open_snip").catch((err) => errToast(err));
  });

  // Pipette: the same frozen-desktop overlay, in colour-pick mode. The result
  // is a clipboard value only: no prompt is created and nothing enters the history.
  $("picker-btn").addEventListener("click", () => {
    invoke("open_picker").catch((err) => errToast(err));
  });

  // Screen recording: the same frozen-desktop overlay, in record-target
  // mode. The backend resolves the click/drag into a window, monitor or region
  // and notifies us with "recorder-target-picked"; a short countdown (0 = none)
  // then the actual capture starts.
  $("record-btn").addEventListener("click", async () => {
    const btn = $("record-btn");
    // A glowing button always meant "start a second recording" before -- the
    // click reached open_recorder regardless, which the backend now refuses
    // with recording-busy, but the button itself should just stop the one
    // already running instead of making the user find the control bar first.
    // recording_status confirms a session is actually still live before
    // acting, so a stale glow (a missed event) falls through to open_recorder
    // like normal rather than trying to stop a recording that is already gone.
    if (btn.classList.contains("is-recording") || btn.classList.contains("is-paused")) {
      const status = await invoke("recording_status").catch(() => null);
      if (status) {
        // The control bar's own poll notices the session is gone and hides
        // itself within its next tick (recording_status turning null) --
        // nothing here has to close it directly.
        try { await invoke("stop_recording"); } catch (err) { errToast(err); }
        return;
      }
      setRecordBtnState("idle");
    }
    invoke("open_recorder").catch((err) => errToast(err));
  });

  // A right-click on either composer button opens a quiet, one-item
  // menu to edit an EXISTING file instead of only ever reaching an editor
  // through a fresh capture/recording. Which editor opens is decided by the
  // file's own content (open_file_in_editor), never by which button was used.
  const openEditorPop = document.createElement("div");
  openEditorPop.className = "ctx open-editor-pop hidden";
  openEditorPop.setAttribute("role", "menu");
  openEditorPop.innerHTML =
    '<button type="button" role="menuitem" id="open-editor-pop-item"></button>' +
    // A second, camera-only entry on the SAME popover the record button
    // already opens on right-click -- shown only when cameraRecord is on, so
    // the default-off feature stays invisible until someone actually enables it.
    '<button type="button" role="menuitem" id="camera-menu-item" class="hidden"></button>';
  document.body.appendChild(openEditorPop);
  function closeOpenEditorPop() {
    openEditorPop.classList.add("hidden");
  }
  // path is set only when the menu is opened for a KNOWN attachment (the
  // library's own right-click, below); left empty it means "let the user
  // pick a file", which is what the composer buttons always do.
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
  // Shared by the composer menu above and the library's own "Open in editor"
  // right-click on an image/video attachment -- existingPath lets the caller
  // hand over a path it already knows instead of showing the picker.
  async function openExternalFileInEditor(existingPath) {
    const path = existingPath || (await invoke("pick_media_file_path").catch(() => null));
    if (!path) return;
    let res;
    try {
      res = await invoke("open_file_in_editor", { path });
    } catch (err) {
      const msg = String(err);
      if (msg === "not-media") toast(t("openFileNotMedia"));
      else if (msg.startsWith("too-large:")) toast(t("openFileTooBig").replace("{mb}", msg.split(":")[1] || ""));
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
  // Driven by the recording state, not by the click -- set on a
  // successful start, cleared on stop/cancel/auto-stop, and re-synced by a
  // slow poll so a missed event can never leave the button glowing forever.
  function setRecordBtnState(state) {
    const btn = $("record-btn");
    btn.classList.toggle("is-recording", state === "recording");
    btn.classList.toggle("is-paused", state === "paused");
    // Idle keeps its own data-i18n-title tooltip (with the right-click hint) and
    // the plain name as accessible name; the click now stops the recording instead
    // of trying to start a second one while either indicator is glowing, so the
    // tooltip has to say that.
    const idle = state === "idle";
    btn.title = idle ? t(btn.dataset.i18nTitle) : t("recBtnStopTip");
    btn.setAttribute("aria-label", idle ? t(btn.dataset.i18nAria) : t("recBtnStopTip"));
  }
  async function syncRecordBtnFromStatus() {
    // Skipped while the window is hidden (minimized to tray) -- the button
    // it updates is not visible either, so there is nothing to resync until
    // the window is shown again, which re-applies the real state on its own.
    if (document.hidden) return;
    const status = await invoke("recording_status").catch(() => null);
    setRecordBtnState(!status ? "idle" : status.paused ? "paused" : "recording");
  }
  setInterval(syncRecordBtnFromStatus, 5000);
  // Catches up the moment the window is shown again instead of waiting out
  // the rest of the 5s poll interval for a state change that happened while
  // hidden.
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
    // A pick made from the running recording's own bar, switching its target
    // instead of starting a new one -- recbar.js's own listener handles that
    // one, this window must not also try to start a second recording from it.
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
        // A refused start leaves no recording: close any bar and put the
        // record button back before the single error toast.
        invoke("hide_rec_bar").catch(() => {});
        setRecordBtnState("idle");
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
    // Checked before the countdown even starts -- discovering low disk space
    // only after a 3-2-1 count would just waste the user's time on a "no" answer.
    const diskWarn = await invoke("check_recording_disk_space").catch(() => null);
    if (diskWarn) {
      const minutes = Math.max(1, Math.round(diskWarn.est_seconds / 60));
      const proceed = await confirmDialog({
        title: t("recDiskWarnTitle"),
        message: t("recDiskWarnMsg")
          .replace("{drive}", diskWarn.drive)
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
      toast(t("recCountdownToast").replace("{n}", n));
      n -= 1;
      if (n < 0) { recCountdownTimer = 0; startNow(); return; }
      recCountdownTimer = setTimeout(tick, 1000);
    };
    tick();
  });

  // ---- Recording result: mirrors the #snip-modal flow above, plus an
  // optional conversion step (recFormat/recQuality != the raw mp4 capture).
  // "recording-captured" is emitted by stop_recording (and by the auto-stop
  // path when the recorded window itself closes) -- the rec-bar is its own
  // window, so this is how the main window learns a file is ready.
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
  let recGen = 0; // invalidated on every close/discard so a late progress tick or a
                   // finished conversion from a superseded recording is ignored
  let recProgressOff = null;
  // Same "closed by default, raw text only under Technical details" treatment
  // as the video editor's own export error row, for a recording conversion
  // that fails instead of exporting.
  function showRecErrorDetails(raw) {
    const box = $("rec-modal-error-details");
    box.classList.toggle("hidden", !raw);
    box.open = false;
    $("rec-modal-error-raw").textContent = raw || "";
  }
  $("rec-modal-error-copy").addEventListener("click", () => {
    invoke("copy_text", { text: $("rec-modal-error-raw").textContent }).then(() => toast(t("copied"))).catch(() => {});
  });
  // Closing the result window without an explicit
  // Save/Save as.../Attach deletes the recording -- Discard, the X button,
  // the backdrop and Escape all end up in closeRecModal(). Copy alone does
  // NOT count as "saving" (it only relocates the file so the clipboard
  // reference survives), so a copy-then-close still removes the working file
  // -- recCopiedNotSaved is what gates the confirm-before-delete for that case.
  let recSavedOrAttached = false;
  let recCopiedNotSaved = false;
  // Elide a long path in the middle rather than at the end -- the last
  // segment (the actual folder name) is what tells destinations apart, so it
  // has to stay visible; the title attribute always carries the full path.
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
  // Once saved/attached, nothing that reaches this function may delete
  // the file anymore -- "discard" used to force deletion regardless, which is
  // exactly how clicking Discard right after a successful Save deleted the
  // file the user had just saved. Both a real Discard click and an
  // implicit close (X, backdrop, Escape) before any Save/Attach/Edit already
  // meant "throw this away", so the two no longer need separate handling.
  async function closeRecModal() {
    const path = pendingRecPath;
    const shouldDelete = !recSavedOrAttached;
    // Copy alone does not count as saving, but it already moved the file to
    // where the clipboard reference points -- closing without confirming
    // first would delete that file out from under the still-live clipboard
    // reference (280's own rule). A toast after the fact is too late; ask now,
    // before anything else changes, so declining leaves the modal untouched.
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
    invoke("cancel_recording_convert").catch(() => {});
    invoke("cancel_recording_crop").catch(() => {});
    if (recProgressOff) { recProgressOff(); recProgressOff = null; }
    closeRecMorePop();
    recModal.classList.add("hidden");
    recPreview.classList.add("hidden");
    recPreview.removeAttribute("src");
    recStatus.classList.remove("rec-status-cancellable");
    recStatus.onclick = null;
    pendingRecPath = "";
    recSavedOrAttached = false;
    recCopiedNotSaved = false;
    if (shouldDelete && path) {
      // A number back means it actually moved to "verworfen" (see
      // discard_recording_file) -- null/failure stays silent, same as before.
      const keepH = await invoke("discard_recording_file", { path }).catch(() => null);
      if (keepH != null) toast(t("recDiscardedToast").replace("{hours}", String(Math.round(keepH))));
    }
  }
  // needsCrop: the window recording's canvas was bigger than the window
  // ever used (369's multi-display canvas) -- trim the black margin first,
  // with the same visible/cancellable status line as the format conversion
  // below, before either showing the raw preview or starting that conversion.
  // A cancel or any failure here just keeps the untouched, letterboxed
  // recording -- never something worth an error toast.
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
  // Save, Save as, Copy, Attach and the video editor all act on pendingRecPath. While the
  // crop or the conversion still rewrites that file they must not run: one of them
  // would adopt another path just before the conversion answers and overwrites it.
  const REC_ACTION_IDS = ["rec-modal-save", "rec-modal-saveas", "rec-modal-copy", "rec-modal-yes", "rec-modal-edit"];
  function setRecActionsBusy(busy) {
    for (const id of REC_ACTION_IDS) $(id).disabled = busy;
  }
  async function openRecModal(rawPath, needsCrop) {
    pendingRecPath = rawPath;
    recSavedOrAttached = false;
    recCopiedNotSaved = false;
    recGen += 1;
    const gen = recGen;
    setRecActionsBusy(false);
    renderRecFolder();
    $("rec-modal-edit").classList.toggle("hidden", !flag("videoEditor"));
    recModal.classList.remove("hidden");
    // recAutoCrop off means the user chose to keep the border -- skip the whole
    // cropping step (and its status line) silently, same as a canvas that
    // never needed cropping in the first place.
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
      // The modal was closed/discarded/attached while this await was in
      // flight -- do not start a conversion nobody will see, and do not race
      // whatever that close already did with this recording's file (discard
      // deletes it by the same path convert_recording would then also touch).
      myOff();
      if (recProgressOff === myOff) recProgressOff = null;
      return;
    }
    try {
      const outPath = await invoke("convert_recording", { srcPath: rawPath, format, lossless });
      if (gen !== recGen) return;
      recStatus.classList.remove("rec-status-cancellable");
      recStatus.onclick = null;
      recStatus.textContent = "";
      // Another action already moved the working file elsewhere: its path stands, the
      // converted copy must not overwrite it.
      if (pendingRecPath !== rawPath) return;
      pendingRecPath = outPath;
      recPreview.classList.remove("hidden");
      recPreview.src = convertFileSrc(outPath);
    } catch (err) {
      if (gen !== recGen) return;
      recStatus.classList.remove("rec-status-cancellable");
      recStatus.onclick = null;
      // Cancelled by the user's own click above, ffmpeg missing, or ffmpeg
      // genuinely failed on this input -- either way the untouched raw
      // recording is still there to fall back to, and only a real failure
      // gets the "Technical details" row (never a cancel).
      const raw = String(err);
      if (raw.includes("cancel")) {
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
      // Unlisten this call's own listener, never whatever the global variable
      // currently points at -- a superseded (older) call's finally must not
      // tear down a newer, still-active openRecModal call's own listener.
      myOff();
      if (recProgressOff === myOff) recProgressOff = null;
    }
  }
  // The periodic sweep and "once a recording ends" both fire this live
  // (they always run after this listener exists) -- only when it removed
  // something, per "a toast... when the total is worth mentioning".
  listen("recording-sweep-done", (e) => {
    const { count, bytes } = e.payload || {};
    if (count > 0) toast(t("recSweepDone").replace("{n}", String(count)).replace("{v}", fmtBytes(bytes, LANG)));
  });
  // The control bar's own target-switch button lives in a separate
  // window with no toast of its own -- a failed switch always leaves the
  // previous source still recording, this is purely the "here is why" note.
  listen("recording-target-switch-failed", (e) => errToast(e.payload));
  listen("recording-captured", async (e) => {
    setRecordBtnState("idle"); // covers both a normal stop and the auto-stop path
    const path = e.payload?.path || "";
    // An encoder write failure (disk full, etc.) already stopped the feeder
    // itself before this event fired -- whatever got captured up to that
    // point is still a valid, playable file, just cut short unexpectedly.
    // "low-disk-space" is the one case with a message of its own: the
    // disk-space watch stopped the recording proactively, before the
    // encoder itself would have hit that same wall.
    if (e.payload?.encode_error === "low-disk-space") toast(t("recDiskStoppedMsg"));
    else if (e.payload?.encode_error === "max-limit-reached") toast(t("recStoppedByLimit"));
    else if (e.payload?.encode_error) toast(t("recEncodeError"));
    // A window recording whose canvas had to fall back below the
    // "every display" default (too large for the encoder, or the user
    // picked a smaller mode) says so once, here -- the normal "every
    // display" case stays silent since that is simply the default working.
    if (e.payload?.canvas_note === "monitor") toast(t("recCanvasFallbackMonitor"));
    else if (e.payload?.canvas_note === "window") toast(t("recCanvasFallbackWindow"));
    // The "window size" canvas default (today's Vorgabe) just cut off part
    // of the picture because the window grew mid-recording -- offer, once
    // per occurrence and only while the notice is on, to switch the canvas
    // to every display so a resize like this stops losing content. Runs
    // before the result menu itself opens, never instead of it.
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
    // "Finish and continue" split this recording into several files -- the
    // result dialog below still only ever opens the LAST one (opening every
    // part in sequence in the editor is future work), so at least name how
    // many earlier parts exist and were kept, rather than the split going
    // unmentioned.
    const partsCount = e.payload?.parts?.length || 0;
    if (partsCount > 0) toast(t("recContinuePartsSaved").replace("{n}", String(partsCount)));
    if (path) openRecModal(path, !!e.payload?.needs_crop);
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
  // Opening the editor hands the raw file to another window that keeps working
  // with it -- treat this like Save/Attach so closing this modal afterward never
  // deletes a file the editor might still be reading from.
  $("rec-modal-edit").addEventListener("click", async () => {
    if (!pendingRecPath) return;
    try {
      await invoke("open_video_editor", { srcPath: pendingRecPath });
      recordToolUsage("video_editor");
      recSavedOrAttached = true;
    } catch (err) {
      errToast(err);
    }
  });
  recFolderChangeBtn.addEventListener("click", async () => {
    await EXPERT_PATHS.recordingDir.change();
    renderRecFolder();
  });
  // Update the preview + pending path to a command's returned new location --
  // Save/Save as.../Copy all move the file, so a later action in the same
  // still-open window (Attach, another Copy) must act on where it ended up,
  // not on the now-stale original <data>/recordings path.
  function adoptRecPath(newPath) {
    if (!newPath || newPath === pendingRecPath) return;
    pendingRecPath = newPath;
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
      // The file is already relocated by the time the backend even
      // attempts the clipboard write, so the new path is adopted regardless
      // of whether that write succeeded -- otherwise a clipboard failure left
      // this window pointing at a path that no longer exists.
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
    // Reuses the exact same path a picked/dropped video file already takes --
    // media.js's video detection and the editor's video preview need nothing new.
    // Attaching counts as "saving": mark it before closeRecModal() so the
    // close path does not delete the file out from under the newly opened editor.
    recSavedOrAttached = true;
    closeRecModal();
    if (path) openFileCreate(path);
  });

  // Right-click reaches the picker's history without opening the overlay:
  // it lives in the backend (survives a restart), this popover just displays it.
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
    // The picked notation is already on the clipboard (unchanged behaviour);
    // these three chips let a click re-copy any of the OTHER notations too.
    const chips = ["hex", "rgb", "hsl"]
      .map((k) => e.payload?.[k])
      .filter(Boolean)
      .map((v) => ({
        label: v,
        // The notation already on the clipboard is marked instead of being repeated
        // in the message: that duplication was what pushed the row onto a second line.
        current: v === value,
        onClick: () => invoke("copy_text", { text: v }).catch((err) => errToast(err)),
      }));
    toast(t("colorCopied"), null, e.payload?.hex || "", chips);
    // Live-update the recent-colours popover if the user left it open across a
    // pick, instead of it staying frozen at whatever it showed when it opened.
    if (!pickHistoryPop.classList.contains("hidden")) {
      try { renderPickHistoryPop((await invoke("pick_history")) || []); } catch { /* leave it as-is on a read hiccup */ }
    }
  });

  // Switching to another program never touches a chain, locked or not: it ends
  // only through the chain button or Ctrl+V pressed inside this window.
  const { snipEditOpen, snipEdit, snipEditState, snipEditCancel } = initSnipEditor();

  const snipModal = $("snip-modal");
  let pendingSnip = null;
  let pendingSnipPath = ""; // disk path if this shot was auto-saved to the folder
  // Bumped by every new capture: an async save that finishes after a newer shot
  // arrived must not touch that shot's state or files.
  let snipCaptureGen = 0;
  let pendingSnipName = ""; // backend-suggested "Screenshot <app?> <timestamp>"
  // Set only for a file opened for editing from outside the app -- the folder
  // this "Save as..." should default into, since the original itself is never
  // touched. Empty for every ordinary capture.
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
    // A shot arriving over an open editor must not wipe the open session's state
    // (pending*) before the user said so: ask first when it holds edits, and only
    // then run that session's own cancel.
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
      // Off -> the pre-117/118 flow, unedited shot straight into the result dialog.
      $("snip-preview").src = pendingSnip;
      $("snip-save-folder").checked = optFlag("screenshotSave");
      $("snip-modal-saveas-format").value = normScreenshotFormat(txt("screenshotFormat"));
      snipModal.classList.remove("hidden");
      return;
    }
    // The editor sits between the capture and the result dialog.
    snipEditOpen(pendingSnip, {
      onApply: async (editedDataUrl) => {
        pendingSnip = editedDataUrl;
        // finalize_capture already put the UNEDITED image on the clipboard (and,
        // optionally, on disk) -- redo both side effects on the edited result.
        invoke("copy_image_data", { dataUrl: editedDataUrl }).catch((err) => errToast(err));
        let saveFailed = false;
        if (optFlag("screenshotSave")) {
          // Save the edited copy first and only then remove the unedited file, so a
          // failed save (unwritable folder, full disk, too large) never leaves the
          // user with neither of the two on disk.
          const oldPath = pendingSnipPath;
          const gen = snipCaptureGen;
          const saved = (await invoke("save_screenshot_now", { dataUrl: editedDataUrl }).catch(() => "")) || "";
          if (saved) {
            // The same timestamp name overwrites the old file in place: then the old
            // path IS the new one and must stay.
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
      // Cancel behaves like closing the result dialog today: the unedited shot
      // already on the clipboard/in the folder stays as-is, nothing more happens.
      onCancel: () => {
        pendingSnip = null;
        pendingSnipPath = "";
        pendingSnipName = "";
      },
    });
  });
  // An existing image opened from the composer buttons' right-click
  // menu or the library. Reuses the very same editor and result dialog as a
  // fresh capture, but the source file is a copy -- no clipboard write, no
  // auto-save to the screenshot folder, and "Save as..." below defaults to the
  // origin file's own folder and name instead of a fresh "Screenshot-<stamp>".
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
  // Folder toggle on the result dialog = the same setting as in the main settings.
  // On -> save this shot to the folder now; off -> delete the saved copy.
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
      // The command answers false (it does not reject) when the file could not be
      // removed: then the path is still the saved copy, and turning the switch back
      // on must not write a second one next to it.
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
  // Discard this shot and immediately start a new one -- hidden for a file
  // opened for editing from elsewhere, where "retry" (a fresh capture) makes no sense.
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

  // ---- OCR (text recognition, on by default -- flag "ocr") ----
  const ocrModal = $("ocr-modal");
  const ocrSettings = () => ({
    upscaleUnderPx: 300, // crops under this size are upscaled before recognition
    joinLines: flag("ocrJoinLines"),
    lang: txt("ocrLang") || "auto",
    autoPickLang: true,
  });
  const closeOcrModal = () => ocrModal.classList.add("hidden");
  // Shared by all three entry points (snip-modal, composer screenshot, composer
  // file) and the ocr-captured event below -- one place decides "empty -> toast
  // only" and "auto-copy -> copy now + footer note instead of a toast".
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
  // (a) "Recognize text" in #snip-modal -- runs on the capture already shown there.
  // pendingSnip only ever holds a data URL at this point (it may not be saved to
  // disk yet, see the folder-toggle handler above), hence ocr_image_data.
  $("snip-modal-ocr").addEventListener("click", async () => {
    if (!pendingSnip) return;
    try {
      showOcrResult(await invoke("ocr_image_data", { dataUrl: pendingSnip, ...ocrSettings() }));
      recordToolUsage("ocr");
    } catch (err) { ocrErrToast(err); }
  });
  // (b) standalone composer button -- small menu (Screenshot / From image file).
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
          // Arms the NEXT capture for OCR instead of a normal screenshot save; the
          // backend resets this the moment the session ends (capture or cancel).
          await invoke("set_snip_mode", { mode: "ocr" }).catch(() => {});
          invoke("open_snip").catch((err) => errToast(err));
        },
      },
    ], r.left, r.bottom + 4);
  });
  // Screenshot-into-OCR result, emitted by capture_region's OCR branch -- a
  // separate event from "snip-captured" so the normal screenshot flow above is
  // completely unaffected by OCR mode being armed.
  listen("ocr-captured", (e) => showOcrResult(e.payload));
  // Background thread found one or more installed language packs that need
  // updating (a version change, or Windows silently dropped one after a feature
  // update). Purely a notice -- the backend never auto-installs on its own, so a
  // toast the user ignores until it auto-hides is a complete no-op, not a missed
  // action. Cancel snoozes the next automatic check.
  listen("ocr-lang-update-due", (e) => {
    const due = e.payload || [];
    if (!due.length) return;
    const msg = due.length === 1
      ? t("ocrLangUpdateOne").replace("{lang}", due[0].label)
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
      ? t("ocrLangUpdateOne").replace("{lang}", due[0].label)
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
  return { openEditorPop, closeOpenEditorPop, openExternalFileInEditor, closeRecMorePop, closeRecModal, snipEdit };
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
