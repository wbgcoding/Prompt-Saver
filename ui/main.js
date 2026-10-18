// Main-window controller. Uses Tauri's global API (withGlobalTauri).
import { primeLang } from "./i18n.js";
import EN from "./i18n/en.js";
import { FOLDER_SVG, fmtBytes, trapTabWithin } from "./floatShared.js";
import { confirmDialog, infoDialog, dismissConfirm, initDialogs } from "./dialog.js";
import { assignVarsWindowPending, varsWindowPending } from "./promptVars.js";
import { $, applyBrand, appVersion, assignAppVersion, autoGrow, closeColorPop, colorPop, ctxEl, errToast, EXPERT_ACTIONS, flag, ICON_WARN, inputEl, invoke, LANG, libraryEl, listen, modal, prompts, setStatusText, settings, settingsEl, t, toast, val, wireColorPop } from "./core.js";
import { assignTabDragBlockClick, bindViews, closeMiniMenu, closeViewModal, miniMenu, renderGrid, seedExamplesIfNeeded, switchToView, viewModal, viewOrder, wireNumPop, wireViews } from "./views.js";
import { bindCopyFlow, onPromptHotkey, wireCopyFlow } from "./copyFlow.js";
import { applyMissingFiles, fitAllTiles, fitCache, pollMissingFiles, wireTiles } from "./tiles.js";
import { bindWindowChrome, closeLightbox, closeShortcutsModal, wireClearButtons, wireLightbox, wireTooltips, wireWindowControls } from "./windowChrome.js";
import { bindHelpView } from "./helpView.js";
import { bindLibrary, hideLibrary, renderLibrary } from "./library.js";
import { closeTagManager, closeTagPop, tagPop, wireTags } from "./tags.js";
import { bindJournal } from "./journal.js";
import { applyTheme, applyTileStyle, bindTheme, closeThemeEditor, wireTheme } from "./theme.js";
import { applySettingsCore, applySettingsUi, bindSettingsPanel, closeImportMenu, hideSettings, offerTakeover, runExportAll } from "./settingsPanel.js";
import { bindHotkeys } from "./hotkeys.js";
import { bindAbout } from "./about.js";
import { applyFlags, assignCameraDevicesCache, assignCurrentDataDir, assignDefaultRecordingDir, assignDefaultScreenshotDir, assignDefaultSyncDir, assignMicDevicesCache, assignOcrLangsCache, assignSettingsWriteFailed, assignStoreSealed, assignVoiceLangsCache, assignVoiceSupport, bindExpertMenu, dataDirLockStatus, refreshDataDirLock, refreshSettingsLocked, renderExpert, settingsWriteFailed, storeSealed, syncBannerStack, warnDataDirSyncFolder } from "./expertMenu.js";
import { bindManagement, bindStats, closeCompareView, closeStats, wireManagement } from "./management.js";
import { bindBackups, closeBackups } from "./backups.js";
import { bindDictation, closeLangsModal, wireDictation } from "./dictation.js";
import { bindEditor, closeModal, confirmDiscardIfDirty, editPrompt, openModal, promptToModal, setupDragDrop, wireEditor } from "./editor.js";
import { cacheFloatingState, closeTileFolderPop, tileFolderPop, wireGridFolders } from "./gridFolders.js";
import { bgOpsPop, closeBgOpsPop, wireBgOps } from "./bgOps.js";
import { bindTour, openTour, wireTour } from "./tour.js";
import { bindGlobalSearch, closeGlobalSearch } from "./globalSearch.js";
import { bindCapture, wireCapture } from "./capture.js";
import { bindGridMenus, closeCtx, closeGridTagPop, closeMoveToFolderPop, closeToolbarMenu, closeViewsPop, gridTagPop, moveToFolderPop, toolbarMenu, viewsPop, wireGridMenus } from "./gridMenus.js";
primeLang("en", EN);

// Surface unexpected errors as a toast instead of failing silently.
window.addEventListener("error", (e) => errToast(e.message));
window.addEventListener("unhandledrejection", (e) => errToast(e.reason));

// Right-click is disabled everywhere inside the app (tiles re-enable it).
window.addEventListener("contextmenu", (e) => e.preventDefault());

applyBrand();

wireNumPop();

wireCapture();

wireBgOps();

// ---- Custom color picker (theme-styled popup with SV field + hue bar) ----
// Shared with the video editor through colorpop.js, the one implementation
// both windows load.
wireColorPop();

wireViews();

wireTiles();

wireGridFolders();

wireGridMenus();

wireEditor();

wireCopyFlow();

wireManagement();

wireDictation();

wireTags();

wireTheme();

// ---- Wire events ----
function bind() {
  bindEditor();
  bindCopyFlow();
  const { openEditorPop, closeOpenEditorPop, openExternalFileInEditor, closeRecMorePop, closeRecModal, closeSnipModal, snipEdit } = bindCapture();
  bindDictation();
  bindSettingsPanel();
  bindExpertMenu();
  bindViews();
  bindWindowChrome();
  bindLibrary();

  // Gone with one click: no confirm dialog, unlike a real delete
  // (these are throwaway onboarding content, not the user's own data).
  $("examples-dismiss").addEventListener("click", async () => {
    try { await invoke("dismiss_example_prompts"); } catch (err) { errToast(err); return; }
    await renderGrid();
  });

  bindManagement();
  bindJournal();
  const { closeUpdateModal } = bindAbout();
  bindTour();
  const { helpEl } = bindHelpView();
  bindGlobalSearch();
  bindGridMenus({ openExternalFileInEditor });
  bindHotkeys();
  bindBackups();
  bindStats();
  bindTheme();

  document.addEventListener("pointerdown", (e) => {
    assignTabDragBlockClick(false); // a fresh press ends the suppression a drop armed
    if (!ctxEl.classList.contains("hidden") && !ctxEl.contains(e.target)) closeCtx();
    if (!toolbarMenu.classList.contains("hidden") && !toolbarMenu.contains(e.target)) closeToolbarMenu();
    if (!miniMenu.classList.contains("hidden") && !miniMenu.contains(e.target)) closeMiniMenu();
    // The folder tile toggles its own popover, so let its press through.
    if (!tileFolderPop.classList.contains("hidden") && !tileFolderPop.contains(e.target) &&
        !e.target.closest(".folder-tile")) closeTileFolderPop();
    // Opened from the (already-closed) tile context menu, so nothing else needs excluding.
    if (!viewsPop.classList.contains("hidden") && !viewsPop.contains(e.target)) closeViewsPop();
    if (!moveToFolderPop.classList.contains("hidden") && !moveToFolderPop.contains(e.target)) closeMoveToFolderPop();
    // The toolbar button toggles its own popover, so let its own press through.
    if (!gridTagPop.classList.contains("hidden") && !gridTagPop.contains(e.target) &&
        !e.target.closest("#grid-tag-btn")) closeGridTagPop();
    // The two composer buttons toggle it themselves (their own contextmenu handler).
    if (!openEditorPop.classList.contains("hidden") && !openEditorPop.contains(e.target) &&
        !e.target.closest("#snip-btn") && !e.target.closest("#record-btn")) closeOpenEditorPop();
  });
  // Ctrl+Tab walks the views in the order the tab bar shows them.
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Tab" || !e.ctrlKey || e.altKey) return;
    if (!flag("multiView") || settings.views.length < 2) return;
    if (document.querySelector(".overlay:not(.hidden)")) return; // a dialog is open
    e.preventDefault();
    const order = viewOrder();
    const at = order.findIndex((v) => v.id === settings.active_view);
    const next = order[((at < 0 ? 0 : at) + (e.shiftKey ? -1 : 1) + order.length) % order.length];
    if (next && next.id !== settings.active_view) switchToView(next.id);
  });

  // Dialogs that live at the base z-index and later in the page than most other
  // overlays, topmost first. Promise-based ones have no close function of their
  // own: pressing their cancel/close button resolves them the way a click does.
  const STACKED_DIALOGS = ["ocr-langs-modal", "ocr-modal", "batch-export-modal", "video-cleanup-modal", "import-folder-modal", "import-preview-modal", "error-log-modal"];
  const STACKED_DIALOG_CLOSE = {
    "ocr-langs-modal": closeLangsModal,
    "ocr-modal": () => $("ocr-modal-close").click(),
    "batch-export-modal": () => $("batch-export-cancel").click(),
    "video-cleanup-modal": () => $("video-cleanup-cancel").click(),
    "import-folder-modal": () => $("import-folder-cancel").click(),
    "import-preview-modal": () => $("import-preview-cancel").click(),
    "error-log-modal": () => $("error-log-close").click(),
  };
  function closeStackedDialog() {
    const id = STACKED_DIALOGS.find((d) => !$(d).classList.contains("hidden"));
    if (!id) return false;
    STACKED_DIALOG_CLOSE[id]();
    return true;
  }

  // Keeps Tab from leaking past whichever overlay is currently topmost,
  // into the (visually dimmed) page behind it. Mirrors the Escape dispatcher's
  // own "what's on top wins" cascade below one for one, reusing the same
  // container references, so a new overlay only needs adding in one place to
  // stay consistent between the two. snipEdit and the first-run tour own their
  // own Tab handling already (trapTabWithin call sites in snipEditKeydown/
  // tourKeydown) and are skipped here to avoid trapping twice.
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Tab" || e.ctrlKey) return; // Ctrl+Tab is the view-switcher above
    if (!snipEdit.classList.contains("hidden")) return;
    // tourKeydown owns Tab while the tour is the topmost thing on screen, but it
    // bows out the moment a confirm dialog opens on top of it: and
    // that dialog's own trap below is never reached if this bails unconditionally
    // just because the tour is still open underneath. Only defer to the tour when
    // nothing is layered above it.
    if (!$("tour-modal").classList.contains("hidden") && $("confirm-modal").classList.contains("hidden")) return;
    if (!$("shortcuts-modal").classList.contains("hidden")) { trapTabWithin($("shortcuts-modal"), e); return; }
    if (!toolbarMenu.classList.contains("hidden")) { trapTabWithin(toolbarMenu, e); return; }
    if (!miniMenu.classList.contains("hidden")) { trapTabWithin(miniMenu, e); return; }
    if (!tileFolderPop.classList.contains("hidden")) { trapTabWithin(tileFolderPop, e); return; }
    if (!viewsPop.classList.contains("hidden")) { trapTabWithin(viewsPop, e); return; }
    if (!moveToFolderPop.classList.contains("hidden")) { trapTabWithin(moveToFolderPop, e); return; }
    if (!gridTagPop.classList.contains("hidden")) { trapTabWithin(gridTagPop, e); return; }
    if (!openEditorPop.classList.contains("hidden")) { trapTabWithin(openEditorPop, e); return; }
    if (!$("rec-modal-more-pop").classList.contains("hidden")) { trapTabWithin($("rec-modal-more-pop"), e); return; }
    if (!$("import-menu-pop").classList.contains("hidden")) { trapTabWithin($("import-menu-pop"), e); return; }
    if (!bgOpsPop.classList.contains("hidden")) { trapTabWithin(bgOpsPop, e); return; }
    if (!$("lightbox").classList.contains("hidden")) { trapTabWithin($("lightbox"), e); return; }
    if (!$("global-search").classList.contains("hidden")) { trapTabWithin($("global-search"), e); return; }
    if (!$("confirm-modal").classList.contains("hidden")) { trapTabWithin($("confirm-modal"), e); return; }
    if (!colorPop.classList.contains("hidden")) { trapTabWithin(colorPop, e); return; }
    if (!tagPop.classList.contains("hidden")) { trapTabWithin(tagPop, e); return; }
    if (!$("compare-modal").classList.contains("hidden")) { trapTabWithin($("compare-modal"), e); return; }
    if (!$("theme-modal").classList.contains("hidden")) { trapTabWithin($("theme-modal"), e); return; }
    if (!$("tag-modal").classList.contains("hidden")) { trapTabWithin($("tag-modal"), e); return; }
    if (!ctxEl.classList.contains("hidden")) { trapTabWithin(ctxEl, e); return; }
    // Promise-based dialogs and the OCR dialogs sit at the base z-index but later in the page than the
    // dialogs below, so they are the ones on top when opened from settings, the library or a capture.
    for (const id of STACKED_DIALOGS) if (!$(id).classList.contains("hidden")) { trapTabWithin($(id), e); return; }
    if (!$("update-modal").classList.contains("hidden")) { trapTabWithin($("update-modal"), e); return; }
    if (!$("about-modal").classList.contains("hidden")) { trapTabWithin($("about-modal"), e); return; }
    if (!$("snip-modal").classList.contains("hidden")) { trapTabWithin($("snip-modal"), e); return; }
    if (!$("rec-modal").classList.contains("hidden")) { trapTabWithin($("rec-modal"), e); return; }
    if (!viewModal.root.classList.contains("hidden")) { trapTabWithin(viewModal.root, e); return; }
    if (!modal.root.classList.contains("hidden")) { trapTabWithin(modal.root, e); return; }
    if (!$("dupes-modal").classList.contains("hidden")) { trapTabWithin($("dupes-modal"), e); return; }
    if (!$("trash-modal").classList.contains("hidden")) { trapTabWithin($("trash-modal"), e); return; }
    if (document.body.classList.contains("library-front") && !libraryEl.classList.contains("hidden")) { trapTabWithin(libraryEl, e); return; }
    if (!$("stats-modal").classList.contains("hidden")) { trapTabWithin($("stats-modal"), e); return; }
    if (!$("backup-modal").classList.contains("hidden")) { trapTabWithin($("backup-modal"), e); return; }
    if (!libraryEl.classList.contains("hidden")) { trapTabWithin(libraryEl, e); return; }
    if (!helpEl.classList.contains("hidden")) { trapTabWithin(helpEl, e); return; }
    if (!settingsEl.classList.contains("hidden")) { trapTabWithin(settingsEl, e); return; }
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (!snipEdit.classList.contains("hidden")) return; // the editor owns Escape while open
    // tourKeydown owns Escape while the tour is the topmost thing, same bow-out as the Tab trap above.
    if (!$("tour-modal").classList.contains("hidden") && $("confirm-modal").classList.contains("hidden")) return;
    if (!$("shortcuts-modal").classList.contains("hidden")) { closeShortcutsModal(); return; }
    if (!toolbarMenu.classList.contains("hidden")) { closeToolbarMenu(); return; }
    if (!miniMenu.classList.contains("hidden")) { closeMiniMenu(); return; }
    if (!tileFolderPop.classList.contains("hidden")) { closeTileFolderPop(); return; }
    if (!viewsPop.classList.contains("hidden")) { closeViewsPop(); return; }
    if (!moveToFolderPop.classList.contains("hidden")) { closeMoveToFolderPop(); return; }
    if (!gridTagPop.classList.contains("hidden")) { closeGridTagPop(); return; }
    if (!openEditorPop.classList.contains("hidden")) { closeOpenEditorPop(); return; }
    if (!$("rec-modal-more-pop").classList.contains("hidden")) { closeRecMorePop(); return; }
    if (!$("import-menu-pop").classList.contains("hidden")) { closeImportMenu(); return; }
    if (!bgOpsPop.classList.contains("hidden")) { closeBgOpsPop(); return; }
    if (!$("lightbox").classList.contains("hidden")) closeLightbox();
    else if (!$("global-search").classList.contains("hidden")) closeGlobalSearch();
    else if (dismissConfirm()) { /* the dialog closed itself */ }
    else if (!colorPop.classList.contains("hidden")) closeColorPop();
    else if (!tagPop.classList.contains("hidden")) closeTagPop();
    else if (!$("compare-modal").classList.contains("hidden")) closeCompareView();
    else if (!$("theme-modal").classList.contains("hidden")) closeThemeEditor();
    else if (!$("tag-modal").classList.contains("hidden")) closeTagManager();
    else if (!ctxEl.classList.contains("hidden")) closeCtx();
    else if (closeStackedDialog()) { /* the dialog closed itself */ }
    else if (!$("update-modal").classList.contains("hidden")) closeUpdateModal();
    else if (!$("about-modal").classList.contains("hidden")) $("about-modal").classList.add("hidden");
    else if (!$("snip-modal").classList.contains("hidden")) closeSnipModal();
    else if (!$("rec-modal").classList.contains("hidden")) closeRecModal();
    else if (!viewModal.root.classList.contains("hidden")) closeViewModal();
    else if (!modal.root.classList.contains("hidden")) confirmDiscardIfDirty().then((ok) => { if (ok) closeModal(); });
    else if (!$("dupes-modal").classList.contains("hidden")) $("dupes-modal").classList.add("hidden");
    else if (!$("trash-modal").classList.contains("hidden")) $("trash-modal").classList.add("hidden");
    // A library raised over the backups dialog is the topmost thing: it closes first.
    else if (document.body.classList.contains("library-front") && !libraryEl.classList.contains("hidden")) hideLibrary();
    else if (!$("stats-modal").classList.contains("hidden")) closeStats();
    else if (!$("backup-modal").classList.contains("hidden")) closeBackups();
    else if (!libraryEl.classList.contains("hidden")) hideLibrary();
    // Help opens over the settings panel: close it before the
    // settings underneath, same pattern as library-front/journal above.
    else if (!helpEl.classList.contains("hidden")) helpEl.classList.add("hidden");
    else if (!settingsEl.classList.contains("hidden")) hideSettings();
  });

  listen("theme-changed", (e) => applyTheme(e.payload?.theme, e.payload?.vars));
  // "Edit prompt" chosen in a floating pill's right-click menu.
  listen("edit-prompt", (e) => editPrompt(String(e.payload)));
  // A prompt's global hotkey was pressed: run the same activation a tile does.
  // Whether it also pastes is the hotkey's OWN setting, falling back to the global one.
  listen("prompt-hotkey", (e) => {
    const id = String(e.payload);
    const mode = prompts.find((p) => p.id === id)?.hotkey_paste || "";
    onPromptHotkey(id, mode ? mode === "paste" : flag("promptHotkeyPaste"));
  });
  // A quick-copy entry in the tray menu: same activation, never auto-paste.
  listen("prompt-tray-copy", (e) => onPromptHotkey(String(e.payload), false));
  // The automatic backup has no frontend call site of its own to toast
  // from: the backend says so once the snapshot is really on disk.
  listen("backup-done", () => toast(t("backupDone")));
  // Same shape, the failure side: an automatic backup that silently never ran
  // again would otherwise leave the user unprotected with nothing telling them.
  listen("backup-failed", () => toast(t("autoBackupFailed")));
  // A pill can unpin ITSELF (its own "Remove" action) without this window ever
  // being told: the tile context menu's pin/unpin label then kept reading a
  // stale settings.floating and could re-open a pill the label said to remove.
  listen("floating-pin-changed", (e) => cacheFloatingState(e.payload.id, e.payload.pinned));
  // floatParkCleanupH hid a parked button's view (never its saved entry) --
  // point at the recall action, the one place that brings it back.
  listen("float-park-cleaned-up", () => {
    toast(t("floatParkCleanedUp"), { label: t("floatRecallAction"), onClick: () => EXPERT_ACTIONS["float-recall"]?.() });
  });
  // A copy recorded from a pill, the folder window or the tray never touched
  // this window's own settings.usage/last_used mirror: "most used"/"recently
  // used" library sort silently froze at whatever it read at the last full
  // get_state. Only worth a re-render while the library is actually open.
  listen("usage-changed", (e) => {
    if (!settings.usage) settings.usage = {};
    if (!settings.last_used) settings.last_used = {};
    settings.usage[e.payload.id] = e.payload.usage;
    if (e.payload.lastUsed) settings.last_used[e.payload.id] = e.payload.lastUsed;
    else delete settings.last_used[e.payload.id];
    if (!libraryEl.classList.contains("hidden")) renderLibrary();
  });
  // save_prompts/save_trash used to fail silently to the user when both the DB
  // and the JSON fallback failed (disk full, permissions). One toast per event:
  // a stuck disk would otherwise spam this on every edit. A failed SETTINGS
  // write gets the sticky treatment instead of the toast: it is a state, not a
  // one-off event: every setting changed for the rest of this session keeps
  // failing the same way and is lost on restart, so it stays visible in the
  // privacy group (like storeUnsealed) rather than something a missed toast
  // could hide.
  listen("persist-failed", (e) => {
    if (e.payload === "settings") {
      assignSettingsWriteFailed(true);
      warnSettingsWriteFailed();
      if (!$("settings-expert").classList.contains("hidden")) renderExpert();
    } else {
      toast(t("persistFailed"));
    }
  });
  // Every write path the backend gates on the data-folder lock (save_prompts,
  // save_prompts_and_trash, save_trash, write_settings_now) emits this same
  // event on refusal: one toast, the same wording every time, instead of
  // each action failing quietly in its own different way.
  listen("data-dir-readonly-write-blocked", (e) => {
    toast(t("dataDirReadOnlyToast").replace("{host}", () => e.payload || dataDirLockStatus.otherHost || "?"));
  });
  // The disk problem that caused the above went away and a later settings write
  // reached it again: the notice does not need to survive that, so it comes
  // straight back down (the dialog itself still stays told-once for the session).
  listen("settings-write-recovered", () => {
    assignSettingsWriteFailed(false);
    if (!$("settings-expert").classList.contains("hidden")) renderExpert();
  });
  // The one persistent, always-visible sign that the remote-access receiver
  // is reachable at all: shown for as long as it is actually running, with
  // its port and an immediate off switch, never just while the setting is on.
  function updateRemoteApiBanner(status) {
    const running = !!status?.running;
    $("remote-api-banner").classList.toggle("hidden", !running);
    if (running) {
      setStatusText($("remote-api-banner-text"), "remoteApiBadge", "port", status.port);
    }
    syncBannerStack();
  }
  listen("remote-api-status", (e) => updateRemoteApiBanner(e.payload));
  // The receiver replaced an expired certificate by itself: say so, like a manual renewal.
  listen("remote-tls-renewed", () => toast(t("remoteTlsRenewed")));
  invoke("remote_api_status").then(updateRemoteApiBanner).catch(() => {});
  $("remote-api-banner-off").addEventListener("click", async () => {
    try { await invoke("remote_api_stop_now"); } catch (e) { errToast(e); }
  });
  // Sealing broke while the app was already running. The startup question is long
  // over by then, so the backend pushes it instead of this window polling for it.
  listen("store-unsealed", () => {
    assignStoreSealed(false);
    warnStoreUnsealed();
    // An expert menu that happens to be open must not keep claiming everything is
    // fine until it is reopened.
    if (!$("settings-expert").classList.contains("hidden")) renderExpert();
  });
  // The folder sync imported something behind this window: refetch instead of
  // showing (and later saving back) a stale grid.
  listen("store-synced", async () => {
    try {
      await renderGrid();
    } finally {
      // A grid that throws must not leave the open library showing the old data.
      if (!libraryEl.classList.contains("hidden")) renderLibrary();
    }
  });
  // Pushed by the backend every few seconds instead of this window polling
  // missing_files on its own timer.
  listen("missing-files", (e) => applyMissingFiles(e.payload || []));
  // The standalone fill-in window answered (null = cancelled or closed).
  listen("vars-filled", (e) => {
    const resolve = varsWindowPending;
    assignVarsWindowPending(null);
    resolve?.(e.payload ?? null);
  });
  // "Rename" in a pill folder's menu. The folder window hosts no themed dialog,
  // so the name is asked for here and written back from here.
  listen("folder-rename", async (e) => {
    const id = String(e.payload?.id || "");
    if (!id) return;
    const name = await confirmDialog({
      title: t("folderRenameTitle"),
      message: t("folderRenameMsg"),
      confirmLabel: t("save"),
      icon: FOLDER_SVG,
      danger: false,
      input: { type: "text", value: String(e.payload?.name || "") },
    });
    if (name === null) return;
    invoke("rename_float_folder", { id, name }).catch((err) => errToast(err));
  });
}

wireTour();

// The store stopped being sealed: told once per session, because it is a state,
// not an event stream: every following save fails the same way and a second dialog
// would only be in the way. The permanent record lives in the settings dialog's
// privacy group, which keeps saying so for as long as it is true; that is why this
// one has nothing to dismiss forever and no switch to silence it.
let storeUnsealedTold = false;
async function warnStoreUnsealed() {
  if (storeSealed || storeUnsealedTold) return false;
  storeUnsealedTold = true;
  // The manual export (export_all, the .psb file) is encrypted with its own key,
  // never DPAPI, so it is the one way out that still works right now: offer it
  // instead of only explaining the problem.
  const choice = await confirmDialog({
    title: t("storeUnsealedTitle"),
    message: `${t("storeUnsealedMsg")}\n\n${t("storeUnsealedFix")}`,
    confirmLabel: t("close"),
    altLabel: t("backupSave"),
    icon: ICON_WARN,
    danger: false,
    oneButton: true,
  });
  if (choice === "alt") await runExportAll();
  return true;
}

// Same "told once, state lives on in the privacy group" shape as
// warnStoreUnsealed above: a stuck disk fails the same way on every
// following write, so a second dialog would only be in the way.
let settingsWriteFailedTold = false;
async function warnSettingsWriteFailed() {
  if (!settingsWriteFailed || settingsWriteFailedTold) return false;
  settingsWriteFailedTold = true;
  await infoDialog(t("settingsWriteFailedTitle"), t("settingsWriteFailedMsg"), ICON_WARN);
  return true;
}

// Called once, right after seedExamplesIfNeeded(), from init()'s reveal(). Runs
// AFTER offerTakeover() (the portable/installed data-folder adoption offer) so the
// takeover question is always seen first, but is not suppressed by it any more: an
// adopted store brings its own tourSeen along (settings.json is part of the copied
// files), so a returning user still gets no tour, while a declined offer means a
// genuinely new user, who should. The only dialog that still suppresses the tour
// outright is the unencrypted-store warning (sealWarned): more urgent, and safe to
// retry next launch since tourSeen is only ever written by the tour itself. A
// missing-WebView2 message cannot race this at all: that one is native and shown,
// if ever, before the WebView2 runtime (and therefore this whole JS module)
// exists (see ensure_webview2 in webview2.rs, the one allowed exception to routing every
// user-facing dialog through the themed confirmDialog()), so there is nothing left
// to check for it here by the time init() runs.
async function maybeOpenFirstRunTour(sealWarned) {
  if (sealWarned) return; // retried next launch: tourSeen was never written
  if (settings.ui_flags?.tourSeen) return;
  if (!flag("firstRunTour")) return;
  await openTour({ firstRun: true });
}

// The update check never goes online before the user said yes. Asked once per
// launch until answered: Cancel, Escape and a click outside leave the question
// open for the next start. The unencrypted-store warning goes first, same as for
// the tour, and demo mode never asks (it has no real settings to keep).
async function maybeAskUpdateConsent(sealWarned) {
  if (sealWarned || flag("demoMode")) return;
  if (settings.update_consent !== "unasked") return;
  const r = await confirmDialog({
    title: t("updConsentTitle"),
    message: t("updConsentMsg"),
    confirmLabel: t("updConsentYes"),
    altLabel: t("updConsentNo"),
    danger: false,
  });
  if (r !== true && r !== "alt") return;
  const enabled = r === true;
  // A refused write (locked settings, read-only data folder) keeps the question open for the next start.
  try { await invoke("set_update_consent", { enabled }); } catch (err) { errToast(err); return; }
  settings.update_consent = enabled ? "yes" : "no";
  settings.auto_update = enabled;
  // A yes starts the first check now, not at the next launch.
  if (enabled) invoke("run_update_check").catch(() => {});
}

// A crash draft from a previous session, asked about exactly once at the next
// start (never again on later editor opens this same session). "No" removes
// it right away rather than leaving it to resurface on the next launch too.
async function maybeOfferEditorDraftRecovery() {
  if (val("editorDraftSecs") <= 0) return;
  let draft = null;
  try { draft = await invoke("load_editor_draft"); } catch (_) { return; }
  if (!draft) return;
  const when = new Date(draft.updatedAt * 1000).toLocaleString(LANG);
  const preview = (draft.name || draft.text || "").split("\n").slice(0, 2).join(" ").trim().slice(0, 140);
  const restore = await confirmDialog({
    title: t("draftRecoverTitle"),
    message: t("draftRecoverBody").replace("{time}", () => when).replace("{preview}", () => preview),
    confirmLabel: t("draftRecoverRestore"),
    cancelLabel: t("draftRecoverDiscard"),
    danger: false,
  });
  if (!restore) {
    await invoke("discard_editor_draft").catch(() => {});
    return;
  }
  let variants = [];
  try { variants = JSON.parse(draft.variants || "[]"); } catch (_) { variants = []; }
  if (draft.promptId && draft.promptId !== "neu") {
    let p = null;
    try { p = await invoke("get_prompt", { id: draft.promptId }); } catch (_) { p = null; }
    if (p) {
      openModal({ ...promptToModal(p), mode: "edit", id: draft.promptId, name: draft.name, text: draft.text, variants: variants.length ? variants : (p.variants || []) });
      return;
    }
  }
  openModal({ mode: "create", name: draft.name, text: draft.text, variants, title: t("nameModalTitle") });
}

// Each launch-time step runs on its own: one that fails must not skip the rest
// (the tour, the crash-draft recovery, a promptsaver:// link that started the app).
async function runGuarded(step) {
  try { return await step(); } catch (err) { errToast(err); return undefined; }
}

async function init() {
  initDialogs({ t, toast });
  try {
    await applySettingsCore();
    bind();
    wireTooltips();
    wireWindowControls();
    wireClearButtons();
    wireLightbox();
    setupDragDrop();
    await applySettingsUi();
    invoke("app_version").then((v) => {
      assignAppVersion(v);
      setStatusText($("update-version"), "versionInstalled", "v", v);
      setStatusText($("about-version"), "versionInstalled", "v", appVersion);
    }).catch(() => {});
    invoke("current_data_dir").then((d) => { assignCurrentDataDir(d || ""); }).catch(() => {});
    // Awaited, unlike the informational calls around it: the reveal below asks the
    // user about it straight away, so it has to be known before the window appears.
    assignStoreSealed((await invoke("store_sealed").catch(() => true)) !== false);
    refreshSettingsLocked();
    refreshDataDirLock();
    // Not awaited: only a write that failed before this window was even listening
    // (an early migration write, say) could already be true here: everything
    // after boot arrives live through the "persist-failed" event instead. The
    // dialog itself is NOT opened from here: it has to wait its turn in
    // reveal()'s chain below, same as warnStoreUnsealed, or it races the
    // takeover offer for the shared confirm-modal element.
    invoke("settings_write_failed").then((v) => {
      assignSettingsWriteFailed(v === true);
      if (!$("settings-expert").classList.contains("hidden")) renderExpert();
    }).catch(() => {});
    invoke("default_screenshot_dir").then((d) => { assignDefaultScreenshotDir(d || ""); }).catch(() => {});
    invoke("default_sync_dir").then((d) => { assignDefaultSyncDir(d || ""); }).catch(() => {});
    invoke("default_recording_dir").then((d) => { assignDefaultRecordingDir(d || ""); }).catch(() => {});
    // Whatever the startup sweep found (if anything): collected once,
    // here, since it ran before this window (or any of its listeners) existed.
    invoke("take_recording_sweep_notice").then((r) => {
      if (r && r[0] > 0) toast(t("recSweepDone").replace("{n}", String(r[0])).replace("{v}", fmtBytes(r[1], LANG)));
    }).catch(() => {});
    // OCR language list: fetched once, cached, "Automatic" is always
    // prepended separately (see EXPERT_DROPDOWNS.ocrLang above).
    invoke("ocr_languages").then((langs) => { assignOcrLangsCache(langs || []); }).catch(() => {});
    // Microphone list for recMicDevice: fetched once, cached, "Automatic"
    // (system default) is always prepended separately (see EXPERT_DROPDOWNS.recMicDevice above).
    invoke("list_mic_devices").then((devs) => { assignMicDevicesCache(devs || []); }).catch(() => {});
    // Camera list for EXPERT_DROPDOWNS.cameraDevice above: a browser API
    // call, not a Rust round trip, and safe to make from this window: listing
    // devices never opens a stream, so no camera light and no permission
    // prompt happens here.
    navigator.mediaDevices?.enumerateDevices?.()
      .then((devs) => {
        assignCameraDevicesCache(devs.filter((d) => d.kind === "videoinput").map((d, i) => ({ id: d.deviceId, name: d.label || `${t("cameraMenuItem")} ${i + 1}` })));
      })
      .catch(() => {});
    invoke("voice_languages").then((langs) => { assignVoiceLangsCache(langs || []); }).catch(() => {});
    // Re-applies flags once this resolves so no-voice-engine reflects reality
    // (the optimistic default above assumes support until this says otherwise).
    invoke("voice_support").then((s) => { if (s) { assignVoiceSupport(s); applyFlags(); } }).catch(() => {});
    applyTileStyle();
    autoGrow(inputEl);
    inputEl.focus();
    pollMissingFiles(); // this window's own first check - the ambient refresh below is a pushed event, not a timer
  } catch (e) {
    // Never strand the user with an invisible app: log, surface a toast if the
    // UI got far enough to have one, and still reveal the window below.
    console.error(e);
    try { errToast(e); } catch (_) {}
  } finally {
    // Reveal only after the first fitted paint: the user never sees text sizing.
    // A hidden WebView2 window can stop producing frames, so a timer backs the
    // rAF chain up: showing late beats never showing at all. Whichever wins, the
    // reveal runs exactly once.
    let revealed = false;
    const reveal = () => {
      if (revealed) return;
      revealed = true;
      // Ask about adopting an installed store only once the themed window is up.
      // The first-run tour comes right after, but only if that offer (the one
      // other dialog that can fire this early) did not itself appear this launch.
      invoke("show_main_window").catch(() => {}).finally(async () => {
        await runGuarded(offerTakeover);
        // Seeded AFTER the takeover offer, not before: seeding earlier (e.g. from
        // applySettingsUi()) would make the library non-empty and takeover_offer()
        // would then never fire, hiding the adoption question behind the tour. Only
        // re-render if something actually landed: the common case (a library that
        // already has content) must not repaint the grid a second time on every
        // launch, which would invalidate tile references a user could already be
        // interacting with.
        const seeded = await runGuarded(seedExamplesIfNeeded);
        if (seeded && seeded.length) await runGuarded(() => renderGrid());
        // Ahead of the tour: an unencrypted store is the more urgent of the two,
        // and the tour is retried next launch (tourSeen is only written by it).
        const sealWarned = await runGuarded(warnStoreUnsealed);
        await runGuarded(warnSettingsWriteFailed);
        await runGuarded(warnDataDirSyncFolder);
        await runGuarded(() => maybeAskUpdateConsent(sealWarned));
        await runGuarded(() => maybeOpenFirstRunTour(sealWarned));
        await runGuarded(maybeOfferEditorDraftRecovery);
        // A promptsaver:// link that started this process waits in the backend
        // until this listener side is up; ask for it only now.
        invoke("run_pending_launch_url").catch(() => {});
      });
    };
    setTimeout(reveal, 1500);
    requestAnimationFrame(() => {
      try { fitCache.clear(); fitAllTiles(); } catch (_) {}
      requestAnimationFrame(reveal);
    });
  }
}

init();
