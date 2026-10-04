// Main-window controller. Uses Tauri's global API (withGlobalTauri).
import { primeLang } from "./i18n.js";
import EN from "./i18n/en.js";
import { FONTS } from "./fonts.js";
import { IMAGE_EXT, mediaKind } from "./media.js";
import { FOLDER_SVG, parseFfmpegError, fmtBytes, trapTabWithin } from "./floatShared.js";
import { placePopover, placePopoverAt } from "./popover.js";
import { attachSelectPicker } from "./selectpop.js";
import { confirmDialog, infoDialog, dismissConfirm, initDialogs, EYE_SVG } from "./dialog.js";
import { FLAG_LABELS, OPT_FLAG_LABELS, OPT_FLAG_KEYS, flagLabelKey, groupFlags, EXPERT_TABS, SETTINGS_AREA_LABEL } from "./expertConfig.js";
import { hasMarkdownSyntax, renderMarkdown } from "./markdown.js";
import { assignVarsWindowPending, dialogTokens, DYN_VARS, dynToken, extractVars, hasEmptyVarToken, peekRotatedText, readFixedVars, resolveVars, saveFixedVars, varsWindowPending } from "./promptVars.js";
import { TOUR_STEPS, TOUR_STEP_COUNT, TOUR_LAST_STEP, TOUR_FEATURES } from "./tourContent.js";
import { initSnipEditor } from "./snipEditor.js";
import { $, applyBrand, appVersion, armButton, assignAppVersion, assignDrag, assignFolderArm, assignMemberDrag, assignModalState, assignSettings, autoGrow, closeColorPop, colorPop, COLORS, convertFileSrc, ctxEl, debounced, DISARM_MS, disarmButton, drag, DRAG_THRESHOLD, errToast, EXPERT_ACTIONS, favView, flag, flagState, folderArm, gateOpen, gridEl, gridTags, HISTORY_PREVIEW_LEN, ICON_DOWN, ICON_FILE, ICON_IMAGE, ICON_PDF, ICON_TRASH, ICON_UP, ICON_VIDEO, ICON_WARN, inkShadow, inputEl, invoke, LANG, libraryEl, listen, LOCK_SVG, memberDrag, modal, modalState, NUM_FIELD_BIG_STEP, OCR_LANG_NOTICE_MS, openColorPop, optFlag, PDF_EXT, prompts, putIcon, readableInk, saveBtn, setStatusText, settings, settingsEl, shipsOn, t, TB_CHECK, TILE_FOLDER_ZONE, toast, txt, undoToast, val, VOICE_LANG_NOTICE_MS, wireColorPop, withShortcut, withTaskbarBusy } from "./core.js";
import { chainMode, isTextPrompt, toggleChainPick } from "./chain.js";
import { activeView, assignTabDragBlockClick, bindViews, closeMiniMenu, closeViewModal, gridKeyOf, layoutOf, linkViews, miniMenu, openMiniMenu, refreshFavViewUi, renderGrid, renderViews, seedExamplesIfNeeded, switchToView, viewModal, viewOrder, viewRowBase, wireNumPop, wireViews } from "./views.js";
import { bindCopyFlow, copyFormattedPrompt, copyOnCooldown, copyTextWithVars, focusTile, maybeCloseOnCopy, onPromptHotkey, recordCopy, wireCopyFlow } from "./copyFlow.js";
import { applyAutoBadge, applyMissingFiles, applyRotateState, autoActiveIds, autoPasteDouble, autoPasteNow, cellAt, endDragVisuals, fitAllTiles, fitCache, fitTileText, hasImage, imageOf, linkTiles, missingFiles, placeTile, pollMissingFiles, rotateReqs, rotateState, setHoverCell, showCopied, wireTiles } from "./tiles.js";
import { bindWindowChrome, closeLightbox, closeShortcutsModal, openAllShortcuts, openLightbox, openShortcutsModal, wireClearButtons, wireLightbox, wireTooltips, wireWindowControls } from "./windowChrome.js";
import { assignHelpCollapsed, assignHelpQuery, bindHelpView, helpCategories, helpNorm, helpSectionLabel, openHelpAt, renderHelp } from "./helpView.js";
import { assignLibQuery, batchErrorToast, bindLibrary, computeLibSearchScore, copyResolved, ensureArchiveTagName, hideLibrary, isArchived, libQuery, linkLibrary, refreshLastEdited, renderLibrary, toastCopied, trashPromptWithUndo } from "./library.js";
import { allTags, closeTagManager, closeTagPop, closeTagSuggest, linkTags, renderTagChips, tagChipEl, tagKey, tagPop, tagsOn, wireTags } from "./tags.js";
import { bindJournal, fmtCopyTime, linkJournal, restoreClipInbox } from "./journal.js";
import { applyTheme, applyTileStyle, bindTheme, wireTheme } from "./theme.js";
import { applySettingsCore, applySettingsUi, bindSettingsPanel, hideSettings, linkSettingsPanel, maybeBackupBeforeWipe, normSize, offerTakeover, runExportAll, runImportAll } from "./settingsPanel.js";
import { bindHotkeyField, bindHotkeys, HOTKEY_ERRORS, hotkeyMessage, promptHotkeysOn, renderPromptHotkey, setPromptHotkey, showHotkeyError, toggleRotateAutoDirect } from "./hotkeys.js";
import { bindAbout } from "./about.js";
import { applyFlags, applyValues, assignCameraDevicesCache, assignCurrentDataDir, assignDefaultRecordingDir, assignDefaultScreenshotDir, assignDefaultSyncDir, assignMicDevicesCache, assignOcrLangsCache, assignSettingsWriteFailed, assignStoreSealed, assignVoiceLangsCache, assignVoiceSupport, bindExpertMenu, currentDataDir, dataDirLockStatus, EXPERT_DEFS, EXPERT_DROPDOWNS, EXPERT_LABEL_KEYS, EXPERT_PATHS, jumpToExpertSetting, linkExpertMenu, openSettingsDirectoryEntry, openVideoCleanup, refreshDataDirLock, refreshSettingsLocked, renderExpert, rowResetBtn, settingHit, SETTINGS_DIRECTORY, settingsDirectoryHit, settingsWriteFailed, storeSealed, syncBannerStack, warnDataDirSyncFolder, withTip } from "./expertMenu.js";
import { bindManagement, closeCompareView, collapsibleHead, linkManagement, maybeDupImportToast, maybeShowSimilarHint, numRow, PROMPT_TYPE_LABELS, recordToolUsage, renderStatsPanel, statSection, wireManagement } from "./management.js";
linkViews({ buildFolderTile, buildSwatches, foldersOfView: (...x) => foldersOfView(...x), saveFolder });
linkTiles({ armFolderDrop, disarmFolder, dropIntoFolder, folderById: (...x) => folderById(...x), openCtx, openModal, openTileFolderPop, saveFolder });
linkLibrary({ buildSwatches, closeBackups, editPrompt, ensureSecretVaultUnlocked, foldersOfView: (...x) => foldersOfView(...x), openCtx });
linkTags({ buildSwatches });
linkJournal({ editPrompt, openModal });
linkSettingsPanel({ openBackups, withDialog });
linkExpertMenu({ closeBackups, openOcrLangsModal, openVoiceLangsModal, renderBackupPanel, varFixedListRow });
linkManagement({ editPrompt, renderDiffInto, saveBackupSetting, saveUiState, splitDiffSides });
primeLang("en", EN);

// Bumped by openModal/closeModal. An async pick/preview started for one editor
// session (pickReplacementMedia, replaceFile) captures this before its awaits and
// checks it again after -- `modalState` alone is not enough, since closing and
// opening a DIFFERENT prompt leaves modalState just as truthy, letting a slow
// reply from the old prompt land in the new one's editor.
let modalSession = 0;
// Whatever had keyboard focus right before the editor opened, so closing
// it returns focus there instead of dropping it to document.body -- without
// this a keyboard user's next Tab press restarts from the top of the page.
let modalOpener = null;
let modalInitial = ""; // snapshot of the open modal's content (unsaved-change guard)
let modalSaving = false; // re-entry guard, a double-click/double-Enter must not fire two saves
let draftTimer = null; // editor-draft autosave tick, armed by openModal, disarmed by closeModal
let draftLastSaved = null; // fingerprint of the last written draft, so an unchanged tick skips the write
let ctxId = null;

// Surface unexpected errors as a toast instead of failing silently.
window.addEventListener("error", (e) => toast(String(e.message)));
window.addEventListener("unhandledrejection", (e) => toast(String(e.reason)));

// Right-click is disabled everywhere inside the app (tiles re-enable it).
window.addEventListener("contextmenu", (e) => e.preventDefault());

applyBrand();

wireNumPop();

// ---- Colour-pick history popover (right-click on #picker-btn) ----
const pickHistoryPop = document.createElement("div");
pickHistoryPop.className = "pick-history-pop hidden";
document.body.appendChild(pickHistoryPop);
function closePickHistoryPop() { pickHistoryPop.classList.add("hidden"); }
document.addEventListener("pointerdown", (e) => {
  if (pickHistoryPop.classList.contains("hidden")) return;
  if (!pickHistoryPop.contains(e.target) && !e.target.closest("#picker-btn")) {
    closePickHistoryPop();
  }
});

// ---- Background jobs popover (header button) --------------------------
// The registry lives entirely in Rust (see list_bg_ops/push_bg_op) and is
// shared with the taskbar icon -- this just displays it. Export runs in the
// video-editor window, convert and the ffmpeg download can run from this one,
// but every one of their progress events is broadcast app-wide, so this
// window learns about all three the same way regardless of which window
// actually started them.
const BG_OP_CANCEL_CMD = {
  export: "cancel_export",
  convert: "cancel_recording_convert",
  "ffmpeg-download": "cancel_ffmpeg_download",
  "ocr-index": "cancel_ocr_index_backfill",
};
function bgKindNoun(kind) {
  if (kind === "export") return t("bgKindExport");
  if (kind === "convert") return t("bgKindConvert");
  if (kind === "ocr-index") return t("bgKindOcrIndex");
  return t("bgKindDownload");
}
// The four progress strings ("Converting… {s}s (click to cancel)", etc.) all
// start with the same short present-tense phrase this popover needs -- reusing
// them here avoids a fifth near-duplicate translation for every language.
function bgOpLabel(kind) {
  const key = kind === "export" ? "veExporting" : kind === "convert" ? "recConverting" : kind === "ocr-index" ? "ocrIndexBackfilling" : "ffmpegDownloading";
  const s = t(key);
  const i = s.indexOf("{");
  return (i >= 0 ? s.slice(0, i) : s).trim();
}
const bgOpsPop = document.createElement("div");
bgOpsPop.className = "pick-history-pop hidden";
document.body.appendChild(bgOpsPop);
function closeBgOpsPop() { bgOpsPop.classList.add("hidden"); }
document.addEventListener("pointerdown", (e) => {
  if (bgOpsPop.classList.contains("hidden")) return;
  if (!bgOpsPop.contains(e.target) && !e.target.closest("#bg-ops-btn")) closeBgOpsPop();
});

function renderBgOpsPop(ops) {
  bgOpsPop.innerHTML = "";
  if (!ops.length) {
    const empty = document.createElement("div");
    empty.className = "pick-history-empty";
    empty.textContent = t("bgNone");
    bgOpsPop.appendChild(empty);
    return;
  }
  for (const op of ops) {
    const row = document.createElement("div");
    row.className = "bg-ops-row";
    const label = document.createElement("span");
    label.className = "bg-ops-label";
    label.textContent = bgOpLabel(op.kind);
    const pct = document.createElement("span");
    pct.className = "bg-ops-pct";
    const pctNum = op.state === "done" ? 100 : op.pct != null ? op.pct : null;
    pct.textContent = op.state === "error" ? "—" : op.state === "done" ? "100%" : (op.pct != null ? `${op.pct}%` : "…");
    if (pctNum != null) {
      pct.setAttribute("role", "progressbar");
      pct.setAttribute("aria-valuemin", "0");
      pct.setAttribute("aria-valuemax", "100");
      pct.setAttribute("aria-valuenow", String(pctNum));
      pct.setAttribute("aria-valuetext", t("a11yPctOnly").replace("{pct}", String(pctNum)));
    }
    row.append(label, pct);
    if (op.target) {
      const openBtn = document.createElement("button");
      openBtn.type = "button";
      openBtn.className = "ghost-btn bg-ops-action";
      openBtn.textContent = t("bgOpenFolder");
      openBtn.addEventListener("click", () => invoke("open_bg_op_folder", { path: op.target }).catch((err) => errToast(err)));
      row.appendChild(openBtn);
    }
    if (op.state === "running" || op.state === "indeterminate") {
      const cancelCmd = BG_OP_CANCEL_CMD[op.kind];
      if (cancelCmd) {
        const cancelBtn = document.createElement("button");
        cancelBtn.type = "button";
        cancelBtn.className = "ghost-btn bg-ops-action";
        cancelBtn.textContent = t("cancel");
        cancelBtn.addEventListener("click", () => {
          // A batch export can show several "export" rows at once, each
          // with its own generation number riding in op.id ("export-<n>") --
          // a plain cancel_export() would always hit the video editor's own
          // last export instead of the specific row that was actually clicked.
          if (op.kind === "export") {
            const generation = Number(op.id.split("-").pop());
            invoke("cancel_export_job", { generation }).catch(() => {});
          } else {
            invoke(cancelCmd).catch(() => {});
          }
        });
        row.appendChild(cancelBtn);
      }
    }
    bgOpsPop.appendChild(row);
  }
}

// Keyed by "kind:started_at_ms" -- the registry itself is keyed by kind alone
// (only one job per kind at a time today), but that combination is what tells
// this toast-once check a NEW run of the same kind from one it already showed.
const bgOpsToasted = new Set();
let bgOpsFirstRun = true;

async function refreshBgOps() {
  let ops = [];
  try { ops = (await invoke("list_bg_ops")) || []; } catch { return; }
  const running = ops.filter((o) => o.state === "running" || o.state === "indeterminate");
  $("bg-ops-btn").classList.toggle("hidden", running.length === 0);
  $("bg-ops-badge").classList.toggle("hidden", running.length === 0);
  $("bg-ops-badge").textContent = String(running.length);
  if (!bgOpsPop.classList.contains("hidden")) renderBgOpsPop(ops);
  for (const op of ops) {
    if (op.state !== "done" && op.state !== "error") continue;
    const key = `${op.kind}:${op.started_at_ms}`;
    if (bgOpsToasted.has(key)) continue;
    bgOpsToasted.add(key);
    // First call after load just learns the current state -- a job finished
    // before this window ever asked about it does not need a toast for it.
    if (bgOpsFirstRun || op.state !== "done") continue;
    const action = op.target
      ? { label: t("bgOpenFolder"), onClick: () => invoke("open_bg_op_folder", { path: op.target }).catch((err) => errToast(err)) }
      : null;
    toast(t("bgDoneToast").replace("{what}", bgKindNoun(op.kind)), action);
  }
  bgOpsFirstRun = false;
}

$("bg-ops-btn").addEventListener("click", async () => {
  if (!bgOpsPop.classList.contains("hidden")) { closeBgOpsPop(); return; }
  let ops = [];
  try { ops = (await invoke("list_bg_ops")) || []; } catch { /* show empty rather than nothing */ }
  renderBgOpsPop(ops);
  bgOpsPop.classList.remove("hidden");
  placePopover(bgOpsPop, $("bg-ops-btn").getBoundingClientRect());
});

// push_bg_op (Rust) broadcasts this on every state change -- running tick,
// done, error -- regardless of which window (if any) started the job, so this
// popover stays live without a separate poll timer.
listen("bg-ops-changed", refreshBgOps);
// Some Windows builds keep a window out of screenshots after the switch is turned
// on; the backend says so once when its readback still shows it.
listen("capture-restart-notice", () => toast(t("allowScreenshotRestart")));
refreshBgOps();

listen("bg-quit-requested", async (e) => {
  const n = Number(e.payload) || 0;
  const ok = await confirmDialog({
    title: t("bgTitle"),
    message: t("bgQuitRunning").replace("{n}", String(n)),
    confirmLabel: t("bgCancelClose"),
    danger: true,
  });
  if (ok) invoke("confirm_quit_with_running_ops").catch(() => {});
});

// ---- Custom color picker (theme-styled popup with SV field + hue bar) ----
// Shared with the video editor through colorpop.js, the one implementation
// both windows load.
wireColorPop();

wireViews();

wireTiles();

// ---- Grid folders: several prompts collapsed into one cell ----
// Ships ON. A member has no layout entry of its own while it sits in a folder, so
// switching the feature off simply shows every prompt as its own tile again: the
// folders stay stored and come back with it.
const tileFoldersOn = () => flag("tileFolders");
const tileFolderList = () => (tileFoldersOn() ? settings.tile_folders || [] : []);
const foldersOfView = (viewId) => tileFolderList().filter((f) => f.view_id === viewId);
const folderById = (id) => tileFolderList().find((f) => f.id === id);
const folderMembers = (f) =>
  (f.members || []).map((id) => prompts.find((p) => p.id === id)).filter(Boolean);
const folderTitle = (f) => f.name || t("tileFolderDefaultName");

// One write per change: name, colour, cell and members always travel together, so
// the grid never shows a half-moved folder.
async function saveFolder(f, patch) {
  try {
    assignSettings(await invoke("save_tile_folder", {
      id: f.id || "",
      name: patch.name ?? f.name ?? "",
      color: patch.color ?? f.color ?? "",
      viewId: f.view_id,
      cell: patch.cell ?? f.cell ?? [0, 0],
      members: patch.members ?? f.members ?? [],
    }));
  } catch (e) {
    errToast(e);
    return false;
  }
  return true;
}

// Two tiles dropped on each other become a folder on the target's cell.
async function newTileFolder(memberIds, cell) {
  const name = await confirmDialog({
    title: t("tileFolderNew"),
    message: t("tileFolderNameMsg"),
    confirmLabel: t("save"),
    input: { type: "text", value: t("tileFolderDefaultName"), placeholder: t("tileFolderNamePh") },
    danger: false,
  });
  if (name === null) return; // cancelled: nothing moves, the tiles stay as they were
  const view = activeView();
  const ok = await saveFolder(
    { id: "", view_id: view.id },
    { name: String(name).trim(), cell, members: memberIds }
  );
  if (ok) {
    recordToolUsage("folder_create");
    await renderGrid(true);
  }
}

async function renameTileFolder(f) {
  const name = await confirmDialog({
    title: t("tileFolderRename"),
    message: t("tileFolderNameMsg"),
    confirmLabel: t("save"),
    input: { type: "text", value: f.name, placeholder: t("tileFolderNamePh") },
    danger: false,
  });
  if (name === null || String(name).trim() === f.name) return;
  if (await saveFolder(f, { name: String(name).trim() })) await renderGrid(true);
}

let folderColorTimer = null;
function pickTileFolderColor(f, anchor) {
  openColorPop(anchor, f.color || "", (hex) => {
    const live = folderById(f.id);
    if (!live) return;
    live.color = hex;
    // The free picker's SV field fires this on every drag frame: a full
    // renderGrid per frame would rebuild every tile in the view. Paint just
    // this one folder tile live, the same way buildFolderTile itself does.
    const tile = gridEl.querySelector(`.folder-tile[data-folder-id="${f.id}"]`);
    if (tile) {
      tile.classList.toggle("tinted", !!hex);
      tile.style.background = hex;
      tile.style.borderColor = hex;
      tile.style.color = hex ? readableInk(hex) : "";
      if (hex) tile.style.setProperty("--ink-shadow", inkShadow(hex));
      else tile.style.removeProperty("--ink-shadow");
    }
    clearTimeout(folderColorTimer);
    // Re-read the folder on write: the settings object was replaced in between.
    folderColorTimer = setTimeout(() => {
      const now = folderById(f.id);
      if (now) saveFolder(now, { color: hex });
    }, 300);
  });
}

async function dissolveTileFolder(f) {
  const ok = await confirmDialog({
    title: t("tileFolderDissolve"),
    message: t("tileFolderDissolveMsg").replace("{name}", folderTitle(f)),
    confirmLabel: t("tileFolderDissolve"),
  });
  if (!ok) return;
  try {
    assignSettings(await invoke("delete_tile_folder", { id: f.id }));
  } catch (e) {
    errToast(e);
    return;
  }
  closeTileFolderPop();
  await renderGrid(true);
}

// Take one member out and put it on `cell`. The second-to-last one leaving dissolves
// the folder: the backend's invariant, so there is no special case here.
async function takeOutOfFolder(id, cell) {
  try {
    assignSettings(await invoke("take_out_of_tile_folder", { id, cell }));
  } catch (e) {
    errToast(e);
    return;
  }
  closeTileFolderPop();
  await renderGrid(true);
}

// A folder cell: up to four member faces, the name and the count.
function buildFolderTile(f) {
  const tile = document.createElement("div");
  tile.className = "tile folder-tile";
  tile.dataset.folderId = f.id;
  if (f.color) {
    tile.classList.add("tinted");
    tile.style.background = f.color;
    tile.style.borderColor = f.color;
    tile.style.color = readableInk(f.color);
    tile.style.setProperty("--ink-shadow", inkShadow(f.color));
  }
  const mini = document.createElement("span");
  mini.className = "folder-mini";
  const members = folderMembers(f);
  for (const p of members.slice(0, 4)) {
    const face = document.createElement("span");
    face.className = "folder-mini-cell";
    if (p.color) {
      face.style.background = p.color;
      face.style.color = readableInk(p.color);
      face.style.setProperty("--ink-shadow", inkShadow(p.color));
    } else face.style.setProperty("--ink-shadow", "none");
    const faceImg = imageOf(p);
    if (faceImg) {
      const img = document.createElement("img");
      img.src = faceImg;
      img.draggable = false;
      face.appendChild(img);
    } else {
      face.textContent = (p.name || "").slice(0, 2);
    }
    mini.appendChild(face);
  }
  tile.appendChild(mini);
  const name = document.createElement("span");
  // NOT .tile-name: that class is what the auto-fitter scales, and a folder label
  // blown up to prompt size is what made these tiles unreadable.
  name.className = "folder-title";
  name.textContent = folderTitle(f);
  tile.appendChild(name);
  tile.dataset.tipName = folderTitle(f);
  tile.dataset.tipBody = members.map((p) => p.name).join(", ");
  tile.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    openMiniMenu([
      { label: t("tileFolderRename"), run: () => renameTileFolder(f) },
      { label: t("tileFolderColor"), run: () => pickTileFolderColor(f, tile) },
      { label: t("tileFolderDissolve"), icon: FOLDER_SVG, danger: true, run: () => dissolveTileFolder(f) },
    ], e.clientX, e.clientY);
  });
  // Same drag machine as a tile: only what a press means differs (open, not copy).
  tile.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    assignDrag({ folderId: f.id, startX: e.clientX, startY: e.clientY, moved: false, el: tile });
  });
  return tile;
}

// ---- Folder popover: the members, opened by a click on the folder ----
const tileFolderPop = document.createElement("div");
tileFolderPop.className = "ctx tile-folder-pop hidden";
tileFolderPop.setAttribute("role", "menu");
document.body.appendChild(tileFolderPop);
let tileFolderPopId = null;

function closeTileFolderPop() {
  tileFolderPop.classList.add("hidden");
  tileFolderPopId = null;
}

function openTileFolderPop(f, anchor) {
  if (tileFolderPopId === f.id) { closeTileFolderPop(); return; }
  tileFolderPop.innerHTML = "";
  tileFolderPopId = f.id;
  const head = document.createElement("div");
  head.className = "tile-folder-pop-head";
  head.textContent = folderTitle(f);
  tileFolderPop.appendChild(head);
  const grid = document.createElement("div");
  grid.className = "tile-folder-grid";
  grid.style.gridTemplateColumns = `repeat(${val("tileFolderCols")}, 1fr)`;
  for (const p of folderMembers(f)) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "folder-member-tile";
    btn.dataset.id = p.id;
    btn.title = p.name;
    // Same content as a real grid tile (buildTile): image/gif thumb, type
    // badge, tinted background: so a folder never shows less than the grid
    // underneath it. Interactive bits (select checkbox, tile menu, video player)
    // stay out: this button already has its own click/drag/context-menu handling.
    const kind = p.file_path ? mediaKind(p.file_path) : "";
    const has = hasImage(p);
    const stored = has ? imageOf(p) : "";
    const iconKind = !has && p.icon_path ? mediaKind(p.icon_path) : "";
    const showPath = iconKind ? p.icon_path : p.file_path;
    const showKind = has ? "" : iconKind || kind;
    const showable = p.show_image && (stored || showKind === "image" || showKind === "gif");
    if (showable) {
      btn.classList.add("has-image");
      if (p.color) { btn.style.background = p.color; btn.style.borderColor = p.color; }
      const img = document.createElement("img");
      img.className = "tile-img";
      img.src = stored || convertFileSrc(showPath);
      img.draggable = false;
      btn.appendChild(img);
    } else if (p.color) {
      btn.style.background = p.color;
      btn.style.borderColor = p.color;
      btn.style.color = readableInk(p.color);
      btn.style.setProperty("--ink-shadow", inkShadow(p.color));
    }
    const label = document.createElement("span");
    label.className = "tile-name";
    label.textContent = p.name;
    // Same per-prompt font overrides as the grid tile (0 = follow settings,
    // 1 = auto-fit, else a fixed size that fitTileText leaves alone).
    if (p.font) label.style.fontFamily = FONTS[p.font] || "";
    if (p.font_size === 1) {
      btn.dataset.fitMode = "auto";
    } else if (p.font_size > 1) {
      label.style.fontSize = `${p.font_size}px`;
      btn.dataset.fitMode = "fixed";
    }
    btn.appendChild(label);
    const typeIcon = p.file_path
      ? (kind === "video" ? ICON_VIDEO : kind ? ICON_IMAGE : PDF_EXT.test(p.file_path) ? ICON_PDF : ICON_FILE)
      : p.copy_image ? ICON_IMAGE : "";
    if (typeIcon && flag("typeBadges")) {
      const badge = document.createElement("span");
      badge.className = "tile-type";
      badge.innerHTML = typeIcon;
      btn.appendChild(badge);
    }
    // Same missing-file banner as the grid tile, so a folder never hides a broken prompt.
    if (p.file_path || p.icon_path) {
      const err = document.createElement("span");
      err.className = "tile-error";
      err.textContent = t("fileMissing");
      btn.appendChild(err);
      if (missingFiles.has(p.id)) btn.classList.add("file-missing");
    }
    btn.addEventListener("contextmenu", (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      openCtx(p.id, ev.clientX, ev.clientY);
    });
    // Press and pull onto a cell to take the prompt out of the folder again.
    btn.addEventListener("pointerdown", (ev) => {
      if (ev.button !== 0) return;
      assignMemberDrag({ id: p.id, startX: ev.clientX, startY: ev.clientY, moved: false, el: btn });
    });
    grid.appendChild(btn);
  }
  tileFolderPop.appendChild(grid);
  placePopover(tileFolderPop, anchor.getBoundingClientRect());
  // Cells now size themselves from tileFolderCols + the popover width (CSS
  // grid + aspect-ratio), so the fit box is measured per tile instead of a
  // constant: 1px border + 4px padding each side, same auto-shrink as the grid.
  for (const btn of grid.children) {
    const r = btn.getBoundingClientRect();
    // Floor, not the raw fractional rect: scrollWidth/Height are integers, so a
    // fractional maxW like 76.65 can fail its own <= check against a rounded 77.
    fitTileText(btn, Math.floor(r.width - 10), Math.floor(r.height - 10));
  }
}

window.addEventListener("pointermove", (e) => {
  if (!memberDrag) return;
  if (!memberDrag.moved) {
    if (Math.abs(e.clientX - memberDrag.startX) < DRAG_THRESHOLD &&
        Math.abs(e.clientY - memberDrag.startY) < DRAG_THRESHOLD) return;
    memberDrag.moved = true;
    memberDrag.el.classList.add("dragging");
    document.body.classList.add("drag-active");
    // A live ghost under the cursor, exactly as a tile drag has: pulling a member out
    // of the folder showed nothing moving before, so the gesture read as "not working".
    const r = memberDrag.el.getBoundingClientRect();
    memberDrag.offX = memberDrag.startX - r.left;
    memberDrag.offY = memberDrag.startY - r.top;
    const ghost = memberDrag.el.cloneNode(true);
    ghost.classList.add("drag-ghost");
    ghost.classList.remove("dragging");
    ghost.style.width = `${r.width}px`;
    ghost.style.height = `${r.height}px`;
    document.body.appendChild(ghost);
    memberDrag.ghost = ghost;
  }
  memberDrag.ghost.style.transform =
    `translate(${e.clientX - memberDrag.offX}px, ${e.clientY - memberDrag.offY}px)`;
  setHoverCell(cellAt(e.clientX, e.clientY));
});
window.addEventListener("pointerup", async (e) => {
  if (!memberDrag) return;
  const d = memberDrag;
  assignMemberDrag(null);
  d.ghost?.remove();
  d.el.classList.remove("dragging");
  endDragVisuals();
  const p = prompts.find((x) => x.id === d.id);
  if (!d.moved) {
    // A plain click copies, exactly like the tile it stands for.
    if (!p) return;
    if (chainMode) { await toggleChainPick(p); return; } // pick (asks vars), don't copy
    if (dialogTokens(p, await peekRotatedText(p)).length) { await copyTextWithVars(p, d.el); return; }
    if (autoPasteDouble(d.id)) { await autoPasteNow(d.el); return; } // 2nd fast click pastes
    if (copyOnCooldown(d.id)) return;
    if (await copyResolved(p)) {
      showCopied(d.el);
      recordCopy(d.id);
      maybeCloseOnCopy();
    }
    return;
  }
  const cell = cellAt(e.clientX, e.clientY);
  if (!cell) return;
  await takeOutOfFolder(d.id, [Number(cell.dataset.col), Number(cell.dataset.row)]);
});

function disarmFolder() {
  if (!folderArm) return;
  clearTimeout(folderArm.timer);
  folderArm.el.classList.remove("folder-arm", "folder-arming");
  folderArm.el.style.removeProperty("--arm-ms");
  assignFolderArm(null);
}
function armFolderDrop(e) {
  if (!tileFoldersOn() || favView || !drag || drag.folderId) return disarmFolder();
  const host = cellAt(e.clientX, e.clientY)?.firstElementChild;
  if (!host || host === drag.el) return disarmFolder();
  const r = host.getBoundingClientRect();
  const zone = TILE_FOLDER_ZONE / 100;
  const inside =
    Math.abs(e.clientX - (r.left + r.width / 2)) <= (r.width * zone) / 2 &&
    Math.abs(e.clientY - (r.top + r.height / 2)) <= (r.height * zone) / 2;
  if (!inside) return disarmFolder();
  if (folderArm && folderArm.el === host) return;
  disarmFolder();
  const arm = { el: host, armed: false, timer: 0 };
  const wait = Math.max(0, val("tileFolderHoverMs"));
  // Show the wait instead of making the user guess it: a ring fills over exactly the
  // dwell time, then the target flips to the armed scrim. Without it the folder
  // appeared out of nothing and a drag that was only passing through looked broken.
  host.style.setProperty("--arm-ms", `${wait}ms`);
  host.classList.add("folder-arming");
  arm.timer = setTimeout(() => {
    host.classList.remove("folder-arming");
    host.classList.add("folder-arm");
    arm.armed = true;
  }, wait);
  assignFolderArm(arm);
}

// A drop in an armed centre: join that folder, or make one out of the two tiles.
async function dropIntoFolder(id, host, cell) {
  const f = host.dataset.folderId ? folderById(host.dataset.folderId) : null;
  if (f) {
    if (f.members.includes(id)) return;
    if (await saveFolder(f, { members: [...f.members, id] })) await renderGrid(true);
    return;
  }
  if (!host.dataset.id) return;
  await newTileFolder([host.dataset.id, id], [Number(cell.dataset.col), Number(cell.dataset.row)]);
}

// `settings.floating` maps every pinned prompt to its pill position, and the backend
// treats exactly that entry as "pinned". The grid only ever asks whether an entry
// exists, so after a pin/unpin the cached copy is brought in line straight away from
// what the backend reports: waiting for the next full state fetch would leave the
// context menu offering the opposite of what a click actually does. The placeholder
// position is replaced by the real one with that next fetch; it is never sent back.
function cacheFloatingState(id, pinned) {
  if (!settings.floating) settings.floating = {};
  if (pinned) settings.floating[id] = settings.floating[id] || { x: 0, y: 0 };
  else delete settings.floating[id];
}

// ---- Context menu ----
function openCtx(id, x, y) {
  ctxId = id;
  // Reset an armed delete confirmation from a previous open.
  disarmButton(ctxEl.querySelector('[data-act="delete"]'), t("delete"));
  // Only a text prompt whose body actually contains Markdown syntax, same
  // check the formatted-copy entry in the editor uses.
  const cfPrompt = prompts.find((x) => x.id === id);
  const cfEligible = flag("copyFormatted") && isTextPrompt(cfPrompt) && hasMarkdownSyntax(cfPrompt?.text);
  ctxEl.querySelector('[data-act="copyFormatted"]').classList.toggle("hidden", !cfEligible);
  // Hide the floating-button action when that feature is switched off; otherwise
  // name the actual effect of a click instead of a state-less "toggle".
  const pinBtn = ctxEl.querySelector('[data-act="pin"]');
  pinBtn.classList.toggle("hidden", !flag("floating"));
  const pinned = !!settings.floating?.[id];
  pinBtn.textContent = t(pinned ? "removeFloating" : "showFloating");
  // Archive: hidden unless the feature is on; the label names the action, not a state.
  const archiveBtn = ctxEl.querySelector('[data-act="archive"]');
  archiveBtn.classList.toggle("hidden", !flag("archiveTag"));
  const archivedNow = isArchived(prompts.find((x) => x.id === id) || {});
  archiveBtn.textContent = t(archivedNow ? "unarchiveAction" : "archiveAction");
  // In the favorites view a prompt can't be hidden/removed: unstar it instead.
  ctxEl.querySelector('[data-act="hide"]').classList.toggle("hidden", favView);
  // Nothing to place this prompt into besides the view it is already on.
  ctxEl.querySelector('[data-act="views"]').classList.toggle("hidden", !optFlag("multiViewPrompt") || settings.views.length < 2);
  // Keyboard path for the mouse drag-onto-a-folder move: favorites' free-form
  // layout has no grid cell for a folder to occupy, so it stays out of scope
  // there the same way the mouse gesture already is.
  const moveFolderBtn = ctxEl.querySelector('[data-act="moveToFolder"]');
  moveFolderBtn.classList.toggle("hidden", !tileFoldersOn() || favView);
  // Only visible when this prompt actually rotates and both auto gates are on:
  // grey-and-disabled would still be a control the user cannot act on.
  const rmenuBtn = ctxEl.querySelector('[data-act="rotateAutoMenu"]');
  const rp = prompts.find((x) => x.id === id);
  ctxEl.querySelector('[data-act="editVideo"]').classList.toggle("hidden", !(flag("videoEditor") && rp?.file_path && mediaKind(rp.file_path) === "video"));
  ctxEl.querySelector('[data-act="editImage"]').classList.toggle("hidden", !(flag("screenshot") && flag("snipEditor") && rp?.file_path && ["image", "gif"].includes(mediaKind(rp.file_path))));
  const rotateMenuEligible = !!rp && (rp.variants || []).length > 0 && flag("rotatingPrompts") && flag("rotateAuto");
  rmenuBtn.classList.toggle("hidden", !rotateMenuEligible);
  if (rotateMenuEligible) {
    const on = !!rp.rotate_auto;
    rmenuBtn.setAttribute("aria-checked", String(on));
    rmenuBtn.classList.toggle("checked", on);
    rmenuBtn.querySelector(".tb-check").innerHTML = on ? TB_CHECK : "";
  }
  placePopoverAt(ctxEl, x, y);
}
function closeCtx() {
  ctxEl.classList.add("hidden");
  ctxId = null;
}

// ---- "also show in" popover: place/unplace this prompt on other views ----
const viewsPop = document.createElement("div");
viewsPop.className = "ctx views-pop hidden";
viewsPop.setAttribute("role", "menu");
document.body.appendChild(viewsPop);
function closeViewsPop() {
  viewsPop.classList.add("hidden");
}
function renderViewsPop(id) {
  viewsPop.innerHTML = "";
  for (const v of settings.views) {
    if (v.id === settings.active_view) continue; // already here: that's why the menu was opened
    const on = Object.prototype.hasOwnProperty.call(layoutOf(v), id);
    const row = viewRowBase(v, on);
    row.setAttribute("role", "menuitemcheckbox");
    row.setAttribute("aria-checked", String(on));
    row.addEventListener("click", async () => {
      try {
        assignSettings(await invoke("batch_prompts", { ids: [id], action: on ? "removeView" : "addView", color: null, favorite: null, viewId: v.id }));
      } catch (e) { batchErrorToast(e); return; }
      renderViewsPop(id);
    });
    viewsPop.appendChild(row);
  }
}
function openViewsPop(id, rect) {
  renderViewsPop(id);
  placePopover(viewsPop, rect);
}

// ---- Move a tile into an existing folder by keyboard (the mouse path is a
// drag onto an armed cell, see dropIntoFolder above) ----
const moveToFolderPop = document.createElement("div");
moveToFolderPop.className = "ctx move-to-folder-pop hidden";
moveToFolderPop.setAttribute("role", "menu");
document.body.appendChild(moveToFolderPop);
function closeMoveToFolderPop() {
  moveToFolderPop.classList.add("hidden");
}
function renderMoveToFolderPop(id) {
  moveToFolderPop.innerHTML = "";
  const newRow = document.createElement("button");
  newRow.type = "button";
  newRow.setAttribute("role", "menuitem");
  newRow.textContent = t("tileFolderNew");
  newRow.addEventListener("click", async () => {
    closeMoveToFolderPop();
    const cell = layoutOf(activeView())[id];
    if (!cell) return; // favorites' free-form layout has no cell to replace
    await newTileFolder([id], cell);
  });
  moveToFolderPop.appendChild(newRow);
  for (const f of foldersOfView(activeView().id)) {
    const row = document.createElement("button");
    row.type = "button";
    row.setAttribute("role", "menuitem");
    row.textContent = folderTitle(f);
    row.disabled = f.members.includes(id);
    row.addEventListener("click", async () => {
      if (row.disabled) return;
      closeMoveToFolderPop();
      if (await saveFolder(f, { members: [...f.members, id] })) {
        await renderGrid(true);
        toast(t("movedToFolderToast").replace("{folder}", folderTitle(f)));
      }
    });
    moveToFolderPop.appendChild(row);
  }
}
function openMoveToFolderPop(id, rect) {
  renderMoveToFolderPop(id);
  placePopover(moveToFolderPop, rect);
}

// ---- Grid tag filter: header toolbar popover, own Set from the library's ----
const gridTagPop = document.createElement("div");
gridTagPop.className = "ctx grid-tag-pop hidden";
gridTagPop.setAttribute("role", "menu");
document.body.appendChild(gridTagPop);
function closeGridTagPop() {
  gridTagPop.classList.add("hidden");
}
function updateGridTagBtn() {
  $("grid-tag-btn")?.classList.toggle("active", gridTags.size > 0);
}
function renderGridTagPop() {
  gridTagPop.innerHTML = "";
  const names = allTags();
  for (const name of names) {
    const chip = tagChipEl(name, "button");
    chip.classList.add("tag-filter");
    const on = gridTags.has(tagKey(name));
    chip.classList.toggle("active", on);
    chip.setAttribute("aria-pressed", String(on));
    chip.addEventListener("click", () => {
      const key = tagKey(name);
      if (gridTags.has(key)) gridTags.delete(key); else gridTags.add(key);
      renderGridTagPop();
      updateGridTagBtn();
      renderGrid(true);
    });
    gridTagPop.appendChild(chip);
  }
  if (gridTags.size) {
    const clear = document.createElement("button");
    clear.type = "button";
    clear.className = "tag-action";
    clear.textContent = t("tagClearFilter");
    clear.addEventListener("click", () => {
      gridTags.clear();
      renderGridTagPop();
      updateGridTagBtn();
      renderGrid(true);
    });
    gridTagPop.appendChild(clear);
  }
  if (!names.length && !gridTags.size) {
    const hint = document.createElement("div");
    hint.className = "hint";
    hint.textContent = t("tagFilterEmpty");
    gridTagPop.appendChild(hint);
  }
}
function openGridTagPop() {
  renderGridTagPop();
  placePopover(gridTagPop, $("grid-tag-btn").getBoundingClientRect());
}

// ---- Right-click toolbar menu: show or hide header buttons. Whole features (views, quick grid, clipboard watcher, chaining) are switched in the expert menu, never here. ----
const TOOLBAR_TOOLS = ["favViewButton", "showLibrary", "barToggles", "showLogo", "showTitle"];
// Little glyph shown beside each tool name so the menu reads at a glance.
const tbIcon = (d, fill) => `<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path ${fill ? 'fill="currentColor"' : 'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"'} d="${d}"/></svg>`;
// Tool-menu icons mirror the real toolbar buttons so the menu reads the same as
// the header (library and journal match 1:1).
const TOOL_ICONS = {
  favViewButton: tbIcon("M12 3.5l2.5 5.2 5.7.8-4.1 4 1 5.7L12 16.9l-5.1 2.6 1-5.7-4.1-4 5.7-.8z", true),
  showLibrary: tbIcon("M4 6h2v2H4V6Zm4 0h12v2H8V6ZM4 11h2v2H4v-2Zm4 0h12v2H8v-2ZM4 16h2v2H4v-2Zm4 0h12v2H8v-2Z", true),
  barToggles: tbIcon("M6 9l6 6 6-6"),
  showLogo: tbIcon("M4 5h16v14H4zM7 15l3.5-3.5 3 3 2.5-2.5 2 2M8.6 10a1.4 1.4 0 1 1-2.8 0 1.4 1.4 0 0 1 2.8 0"),
  showTitle: tbIcon("M6 7V5h12v2M12 5v14M9 19h6"),
};
const toolbarMenu = document.createElement("div");
toolbarMenu.className = "ctx toolbar-menu hidden";
toolbarMenu.setAttribute("role", "menu");
document.body.appendChild(toolbarMenu);
const toolFlagOn = (key) => (OPT_FLAG_KEYS.includes(key) ? optFlag(key) : flag(key));
function buildToolbarMenu() {
  toolbarMenu.innerHTML = "";
  const head = document.createElement("div");
  head.className = "toolbar-menu-head";
  head.textContent = t("toolbarMenuTitle");
  toolbarMenu.appendChild(head);
  for (const key of TOOLBAR_TOOLS) {
    const on = toolFlagOn(key);
    const b = document.createElement("button");
    b.type = "button";
    b.className = "toolbar-item" + (on ? " on" : "");
    b.role = "menuitemcheckbox";
    b.setAttribute("aria-checked", String(on));
    const check = document.createElement("span");
    check.className = "tb-check";
    check.innerHTML = on ? TB_CHECK : "";
    const icon = document.createElement("span");
    icon.className = "tb-icon";
    icon.innerHTML = TOOL_ICONS[key] || "";
    const label = document.createElement("span");
    label.textContent = t(FLAG_LABELS[key] || OPT_FLAG_LABELS[key]);
    b.append(check, icon, label);
    b.addEventListener("click", async () => {
      const next = !toolFlagOn(key);
      settings.ui_flags = { ...(settings.ui_flags || {}), [key]: next };
      await invoke("set_ui_flag", { key, enabled: next }).catch((e) => errToast(e));
      applyFlags();
      renderViews();
      refreshFavViewUi();
      buildToolbarMenu(); // reflect the new state, keep the menu open
    });
    toolbarMenu.appendChild(b);
  }
}
function openToolbarMenu(x, y) {
  buildToolbarMenu();
  placePopoverAt(toolbarMenu, x, y);
}
function closeToolbarMenu() { toolbarMenu.classList.add("hidden"); }

// ---- Modal ----
// Reusable palette: "no color" + free picker + presets, into any container.
// onPick(hex) for none/preset; onCustom(anchor, current) opens the free picker.
function buildSwatches(container, selected, onPick, onCustom) {
  container.innerHTML = "";
  const isCustom = !!selected && !COLORS.includes(selected);

  const mkSwatch = (cls, bg) => {
    const sw = document.createElement("button");
    sw.type = "button";
    sw.className = `swatch ${cls}`.trim();
    if (bg) sw.style.background = bg;
    container.appendChild(sw);
    return sw;
  };

  const none = mkSwatch("none" + (selected === "" ? " sel" : ""));
  none.title = t("colorNone");
  none.setAttribute("aria-label", t("colorNone"));
  none.addEventListener("click", () => onPick(""));

  const custom = mkSwatch("custom" + (isCustom ? " sel" : ""), isCustom ? selected : "");
  custom.title = t("colorCustom");
  custom.setAttribute("aria-label", t("colorCustom"));
  custom.addEventListener("click", () => onCustom(custom, isCustom ? selected : ""));

  for (const c of COLORS.slice(1)) {
    const sw = mkSwatch(c === selected ? "sel" : "", c);
    sw.title = c;
    sw.setAttribute("aria-label", c);
    sw.addEventListener("click", () => onPick(c));
  }
}

function renderSwatches(selected) {
  buildSwatches(
    $("color-row"),
    selected,
    (hex) => { modalState.color = hex; renderSwatches(hex); },
    (anchor, cur) => openColorPop(anchor, cur, (hex) => {
      if (!modalState) return;
      modalState.color = hex;
      renderSwatches(hex);
    })
  );
}

// Live caption overlay on the modal media preview: mirrors the on-tile render
// (0 = default size, 1 = auto-scale, else fixed px) so the user sees it as-is.
function updateCaptionPreview() {
  const el = modal.captionPreview;
  if (!el) return;
  const text = modal.caption.value.trim();
  const size = Number(modal.captionSize.value) || 0;
  el.textContent = text;
  el.classList.toggle("hidden", !text);
  el.classList.toggle("auto", size === 1);
  el.style.fontSize = size > 1 ? `${size}px` : "";
}

// Serialized content of the open modal: compared on dismiss to detect edits.
function modalSnapshot() {
  if (!modalState) return "";
  return JSON.stringify([
    modal.name.value, modal.text.value, modal.caption.value, modal.captionSize.value,
    modal.fontSel.value, modal.sizeSel.value, modalState.color || "", modalState.image || "",
    !!modalState.showImage, modalState.filePath || "", modalState.iconPath || "", modal.fav.classList.contains("active"),
    modalState.tags || [], modal.notesText.value, modalState.hotkey || "", modalState.hotkeyPaste || "",
    // The entry list + mode are edited entirely in memory until Save.
    modalState.rotateEntries || [], modalState.rotateMode || "", !!modalState.rotateAuto,
    modalState.pdfPage || 0,
  ]);
}

// ---- Editor variable helper ----
// One-line explanation per reserved name, shown as the chip's tooltip.
const DYN_TIPS = {
  date: "dynDate", time: "dynTime", datetime: "dynDatetime",
  weekday: "dynWeekday", yesterday: "dynYesterday", tomorrow: "dynTomorrow",
  week: "dynWeek", month: "dynMonth", year: "dynYear",
  timestamp: "dynTimestamp", epoch: "dynEpoch",
  clipboard: "dynClipboard", username: "dynUsername",
  uuid: "dynUuid", random: "dynRandom", counter: "dynCounter",
};
// A name or option must not carry the fence characters, or the token breaks.
const cleanVarPart = (s) => s.replace(/[|{}\n]/g, "").trim();

// Write a placeholder at the caret of the prompt textarea.
function insertVarToken(body) {
  const ta = modal.text;
  const start = ta.selectionStart ?? ta.value.length;
  const end = ta.selectionEnd ?? start;
  const token = `{#{${body}}#}`;
  ta.value = ta.value.slice(0, start) + token + ta.value.slice(end);
  ta.focus();
  ta.setSelectionRange(start + token.length, start + token.length);
  ta.dispatchEvent(new Event("input", { bubbles: true })); // length counter + clear-X
}

// Rebuild the helper for the current feature flags (chips only with promptVars,
// the options field only with choiceVars).
function renderVarHelper() {
  const chips = $("modal-vars-chips");
  const dyn = flag("dynVars");
  chips.classList.toggle("hidden", !dyn);
  $("modal-vars-dyn-hint").classList.toggle("hidden", !dyn);
  chips.innerHTML = "";
  if (dyn) {
    for (const name of DYN_VARS) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "vars-chip";
      b.textContent = dynToken(name);
      b.title = t(DYN_TIPS[name]); // hover engine themes any [title]
      b.addEventListener("click", () => insertVarToken(dynToken(name)));
      chips.appendChild(b);
    }
  }
  const choice = flag("choiceVars");
  $("modal-var-opts").classList.toggle("hidden", !choice);
  $("modal-vars-choice-hint").classList.toggle("hidden", !choice);
}

function insertHelperVar() {
  const nameEl = $("modal-var-name");
  const optsEl = $("modal-var-opts");
  const name = cleanVarPart(nameEl.value);
  if (!name) { nameEl.focus(); return; }
  const opts = flag("choiceVars")
    ? optsEl.value.split(",").map(cleanVarPart).filter(Boolean)
    : [];
  insertVarToken(opts.length ? `${name}|${opts.join(",")}` : name);
  nameEl.value = "";
  optsEl.value = "";
}
$("modal-var-add").addEventListener("click", insertHelperVar);
for (const id of ["modal-var-name", "modal-var-opts"]) {
  $(id).addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    e.preventDefault(); // Enter here inserts, it must not submit the editor
    insertHelperVar();
  });
}

// The expert menu's own list editor for varFixed: one row per fixed value
// (name, its value, a delete button) plus an add row. Empty by default.
function varFixedListRow() {
  const wrap = document.createElement("div");
  wrap.className = "field var-fixed-field";
  // The switch above only carries the on/off label -- this list is its own
  // widget, so it gets its own small heading rather than relying on that.
  const heading = document.createElement("div");
  heading.className = "var-fixed-heading";
  heading.textContent = t("expGroupVarFixed");
  wrap.appendChild(heading);
  const fixed = readFixedVars();
  const names = Object.keys(fixed).sort((a, b) => a.localeCompare(b));
  const list = document.createElement("div");
  list.className = "var-fixed-list";
  for (const name of names) {
    const row = document.createElement("div");
    row.className = "var-fixed-row";
    const nameEl = document.createElement("span");
    nameEl.className = "var-fixed-name";
    nameEl.textContent = name; // textContent: a variable name is never markup
    const valueEl = document.createElement("input");
    valueEl.type = "text";
    valueEl.className = "modal-input var-fixed-value";
    valueEl.value = fixed[name];
    valueEl.addEventListener("change", () => {
      const next = { ...readFixedVars() };
      const v = valueEl.value.trim();
      if (v) next[name] = v; else delete next[name]; // cleared value = removed row
      saveFixedVars(next);
      renderExpert();
    });
    const del = document.createElement("button");
    del.type = "button";
    del.className = "icon-btn var-fixed-del";
    del.title = t("varFixedRemove");
    del.setAttribute("aria-label", t("varFixedRemove"));
    putIcon(del, ICON_TRASH);
    del.addEventListener("click", () => {
      const next = { ...readFixedVars() };
      delete next[name];
      saveFixedVars(next);
      renderExpert();
    });
    row.append(nameEl, valueEl, del);
    list.appendChild(row);
  }
  wrap.appendChild(list);
  if (!names.length) {
    const hint = document.createElement("div");
    hint.className = "hint var-fixed-empty";
    hint.textContent = t("varFixedEmpty");
    wrap.appendChild(hint);
  }
  const addRow = document.createElement("div");
  addRow.className = "var-fixed-add";
  const addName = document.createElement("input");
  addName.type = "text";
  addName.className = "modal-input var-fixed-add-name";
  addName.placeholder = t("varFixedNamePh");
  const addValue = document.createElement("input");
  addValue.type = "text";
  addValue.className = "modal-input var-fixed-add-value";
  addValue.placeholder = t("varFixedValuePh");
  const addBtn = document.createElement("button");
  addBtn.type = "button";
  addBtn.className = "ghost-btn var-fixed-add-btn";
  addBtn.textContent = t("varFixedAdd");
  const commitAdd = () => {
    const name = cleanVarPart(addName.value);
    const value = addValue.value.trim();
    if (!name || !value) { (name ? addValue : addName).focus(); return; }
    saveFixedVars({ ...readFixedVars(), [name]: value });
    renderExpert();
  };
  addBtn.addEventListener("click", commitAdd);
  for (const el of [addName, addValue]) {
    el.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); commitAdd(); } });
  }
  addRow.append(addName, addValue, addBtn);
  wrap.appendChild(addRow);
  return wrap;
}

wireCopyFlow();

// ---- Editor draft autosave ----
// Only the text side (name/text/variants) ever gets drafted -- an attached
// image is never re-derived from a crash draft, it stays with the prompt
// itself. One row on the backend (id "current"), since only one editor can be
// open at a time; the fingerprint check below is what keeps an unchanged tick
// from writing that row over and over while the user reads rather than types.
function draftEditorFields() {
  if (!modalState) return null;
  // A confidential entry is never drafted: the draft row would hold its revealed text.
  if (modalState.secret) return null;
  const rotating = rotationUiOn();
  const hasVariants = rotationActive();
  const entries = rotating
    ? modalState.rotateEntries
    : hasVariants
      ? [modal.text.value, ...modalState.rotateEntries.slice(1)]
      : [modal.text.value];
  const promptId = modalState.mode === "edit" ? modalState.id : "neu";
  return { promptId, name: modal.name.value, text: entries[0] || "", variants: JSON.stringify(entries.slice(1)) };
}

async function saveDraftNow() {
  const fields = draftEditorFields();
  if (!fields) return;
  const fp = JSON.stringify(fields);
  if (fp === draftLastSaved) return; // nothing changed since the last write
  draftLastSaved = fp;
  try {
    await invoke("save_editor_draft", fields);
  } catch (_) {
    draftLastSaved = null; // let the next tick retry a failed write
  }
}

function armDraftAutosave() {
  if (draftTimer) { clearInterval(draftTimer); draftTimer = null; }
  draftLastSaved = null;
  if (val("editorDraftSecs") <= 0) return;
  const secs = Math.min(30, Math.max(1, val("editorDraftSecs")));
  draftTimer = setInterval(saveDraftNow, secs * 1000);
}

function disarmDraftAutosave() {
  if (draftTimer) { clearInterval(draftTimer); draftTimer = null; }
  draftLastSaved = null;
}

function openModal({ mode, id, name = "", text = "", color = "", image = "", showImage = false, copyImage = false, filePath = "", pdfPage = 0, iconPath = "", caption = "", captionSize = 0, font = "", fontSize = 0, favorite = false, tags = [], notes = "", hotkey = "", hotkeyPaste = "", variants = [], rotateMode = "", rotateAuto = false, secret = false, locked = false, title, targetCell }) {
  modalSession++;
  modalOpener = document.activeElement;
  assignModalState({
    mode, id, color, image, showImage, copyImage, filePath, pdfPage, iconPath, tags: [...tags], hotkey, hotkeyPaste, targetCell,
    // What the editor opened with. Save compares against it and only sends an
    // image field when the user really changed the picture: a save must never be
    // able to clear a stored image just by not having it to hand.
    imageOpened: image,
    // The FULL entry list (entry 0 = what would otherwise be modal.text's value)
    // while rotation is active, empty while it is not.
    rotateEntries: variants.length ? [text, ...variants] : [],
    rotateMode,
    rotateAuto: !!rotateAuto,
    // Confidential-entry state. Toggling it is its own immediate backend action
    // (set_prompt_secret), never part of the regular save -- see
    // onModalSecretToggle, which is the only place that changes this in memory.
    secret: !!secret,
    locked: !!locked,
  });
  modal.title.textContent = title;
  modal.fav.classList.toggle("active", !!favorite);
  modal.fav.setAttribute("aria-pressed", String(!!favorite));
  updateSecretToggleUi(mode, !!secret, !!copyImage, filePath);
  modal.name.value = name;
  modal.text.value = text;
  mdEditing = false; // every prompt opens rendered; a click on the pane starts editing
  mdLastText = null;
  modal.notesText.value = notes;
  modal.caption.value = caption;
  modal.captionSize.value = String(normSize(captionSize) || 0);
  updateCaptionPreview();
  // Per-tile style overrides, available when creating and editing.
  modal.fontSel.value = font;
  modal.sizeSel.value = String(normSize(fontSize));
  modal.delete.classList.toggle("hidden", mode !== "edit");
  disarmButton(modal.delete, t("delete"));
  renderSwatches(color);
  const tagInput = $("modal-tag-input");
  if (tagInput) tagInput.value = "";
  closeTagSuggest();
  renderTagChips();
  renderPromptHotkey();
  // Show the dialog before wiring the preview so the <video>/<img> decode and
  // lay out in a visible context (a hidden element never renders a first frame).
  modal.root.classList.remove("hidden");
  syncModalImageUi(mode);
  modalInitial = modalSnapshot();
  armDraftAutosave();
  if (secret) invoke("discard_editor_draft").catch(() => {});
  modal.name.focus();
  modal.name.select();
}

// ---- Confidential entries (Vorschlag 68) ----
// Marking a prompt confidential only ever applies to a saved, plain text
// entry -- an image/file prompt has nothing in `text` worth encrypting, and a
// brand-new "create" draft has no id yet for set_prompt_secret to act on.
function updateSecretToggleUi(mode, secret, copyImage, filePath) {
  const eligible = mode === "edit" && flag("secretPrompts") && !copyImage && !filePath;
  modal.secret.classList.toggle("hidden", !eligible);
  modal.secret.classList.toggle("active", !!secret);
  modal.secret.setAttribute("aria-pressed", String(!!secret));
}

// Ensures the vault has a live session key, asking to set one up on first use
// or to unlock it otherwise. Returns false on cancel or a wrong password --
// the caller must not proceed with anything that needs the key.
async function ensureSecretVaultUnlocked() {
  const status = await invoke("secret_vault_status").catch(() => null);
  if (!status) return false;
  if (status.unlocked) return true;
  const pw = await confirmDialog(
    status.configured
      ? {
          title: t("secretUnlockTitle"),
          message: t("secretUnlockMsg"),
          confirmLabel: t("secretUnlockConfirmBtn"),
          input: { type: "password", placeholder: t("secretPasswordPh") },
          danger: false,
        }
      : {
          title: t("secretSetupTitle"),
          message: t("secretSetupMsg"),
          confirmLabel: t("secretSetupConfirmBtn"),
          input: { type: "password", placeholder: t("secretPasswordPh") },
          checkboxes: [{ key: "understand", label: t("secretSetupConfirm"), checked: false }],
          danger: false,
        }
  );
  if (!pw) return false;
  try {
    await invoke("secret_vault_unlock", { password: pw });
    return true;
  } catch (e) {
    toast(t("secretWrongPassword"));
    return false;
  }
}

// The one click handler for the editor's confidential toggle, on or off alike
// -- both directions re-encrypt/decrypt the STORED text with the vault key
// (set_prompt_secret), so any unsaved edit in the field must be saved first or
// this would silently discard it.
async function onModalSecretToggle() {
  if (!modalState || modalState.mode !== "edit") return;
  if (modalSnapshot() !== modalInitial) {
    toast(t("secretNeedsSaveFirst"));
    return;
  }
  const id = modalState.id;
  const next = !modalState.secret;
  if (!(await ensureSecretVaultUnlocked())) return;
  try {
    await invoke("set_prompt_secret", { id, secret: next });
  } catch (e) {
    toast(String(e) === "secret-locked" ? t("secretWrongPassword") : String(e));
    return;
  }
  modalState.secret = next;
  updateSecretToggleUi(modalState.mode, next, modalState.copyImage, modalState.filePath);
  await renderGrid();
  if (next) {
    // The field now holds ciphertext at rest -- closing avoids the editor
    // resaving the plain text it still shows over that.
    closeModal();
  } else {
    const fresh = prompts.find((x) => x.id === id);
    if (fresh) {
      modal.text.value = fresh.text;
      modalInitial = modalSnapshot();
    }
  }
}

// ---- live length counter (chars / words / ~tokens) in the editor ----
let lenRaf = 0;
function updateLenCounter() {
  const el = $("modal-length");
  if (!el) return;
  // Still a text prompt while the Markdown preview stands in for the textarea; never
  // while rotation replaced the textarea with the variants list instead.
  const on = flag("lengthCounter") && !rotationUiOn() &&
    !(modal.text.classList.contains("hidden") && modal.mdPreview.classList.contains("hidden"));
  el.classList.toggle("hidden", !on);
  if (!on) return;
  const s = modal.text.value;
  const chars = [...s].length; // code points, so emoji count as one
  const words = (s.trim().match(/\S+/g) || []).length;
  const tokens = Math.ceil(chars / 4); // rough heuristic, labeled with "~"
  const nf = (n) => n.toLocaleString(LANG);
  const units = txt("counterUnits") || "all";
  const parts = [];
  if (units === "all" || units === "chars") parts.push(`${nf(chars)} ${t("lenChars")}`);
  if (units === "all" || units === "words") parts.push(`${nf(words)} ${t("lenWords")}`);
  if (units === "all" || units === "tokens") parts.push(`~${nf(tokens)} ${t("lenTokens")}`);
  el.textContent = parts.join(" · ");
}
function scheduleLenCounter() {
  if (lenRaf) return;
  lenRaf = requestAnimationFrame(() => { lenRaf = 0; updateLenCounter(); });
}

// Replace layout: the rendered pane stands in for the textarea until the user clicks
// into it. `mdEditing` is that moment: everywhere else the prompt reads as Markdown.
let mdEditing = false;
let mdTimer = 0;
let mdLastText = null;

function renderMdPreview(force) {
  const box = modal.mdPreview;
  if (!box) return;
  const s = modal.text.value;
  if (!force && s === mdLastText) return;
  mdLastText = s;
  // The only innerHTML in this feature, and it is fed a string assembled purely from
  // escaped slices plus literal tags: see renderMarkdown.
  box.innerHTML = renderMarkdown(s);
}
function scheduleMdPreview() {
  // Nothing to draw into while the pane is hidden: but mark it stale, so the next
  // time it is shown it is rebuilt from the current text instead of a stale render.
  if (modal.mdPreview.classList.contains("hidden")) { mdLastText = null; return; }
  clearTimeout(mdTimer);
  // A plain timer, deliberately not requestAnimationFrame: a window that paints no
  // frames (occluded, minimised to the tray, restored from it) never runs a rAF
  // callback, and the pane would then sit there empty.
  mdTimer = setTimeout(() => renderMdPreview(), 60);
}

// Bring toolbar, textarea and preview in line with the flags and the picked layout.
// When `rotating`, the variants list IS the editing surface, so neither the
// textarea nor the rendered pane may stay in flow. Both used to survive it: the
// clear-X wrapper as an empty 120px box (the gap above the colour row) and, with
// on, the preview still rendering entry 1, which read as a stray field.
function syncMdUi(textVisible, rotating = false) {
  const on = textVisible && !rotating && flag("mdPreview");
  if (!on) mdEditing = false;
  // ONE field: the rendered pane stands in for the textarea and hands it back on a
  // click. Two panes side by side was the second thing on screen that did the same
  // job: there is no layout option any more.
  modal.mdWrap.classList.toggle("hidden", !on || mdEditing);
  // Hide the WRAPPER, not just the textarea: the clear-X wrapper stays in flow with
  // an empty box and pushes the rendered pane down, so the field appeared to jump
  // 19px every time it handed over.
  const field = modal.text.closest(".clear-wrap-ta") || modal.text;
  if (textVisible) {
    const hide = rotating || (on && !mdEditing);
    field.classList.toggle("hidden", hide);
    modal.text.classList.toggle("hidden", hide);
    // Tied to `rotating` alone, not the full `hide` -- the mic still dictates
    // into modal.text's value while its rendered Markdown preview is what's on
    // screen, and keeping the row's own visibility stable across THAT swap is
    // what keeps the field from jumping vertically when it hands over.
    modal.voiceRow.classList.toggle("hidden", rotating);
  }
  // Fill the pane whenever the feature is on, visible or not: the first paint must
  // never depend on the pane already being on screen, or switching to it shows a
  // blank box until the next keystroke.
  clearTimeout(mdTimer);
  if (on) renderMdPreview(true);
  if (modal.copyFormattedBtn) {
    // Independent of `on`/`mdEditing` on purpose: if this depended on which of
    // the two (preview vs. textarea) is showing, the button's own hidden/shown
    // state would flip exactly when the field hands over, taking space away
    // from one side and not the other -- the same field-jump class of bug the
    // wrapper-hiding rule above already exists to avoid.
    const cfShow = textVisible && !rotating && flag("copyFormatted") && hasMarkdownSyntax(modal.text.value);
    modal.copyFormattedBtn.classList.toggle("hidden", !cfShow);
  }
}

// Focus the prompt text field. With the Markdown preview standing in for it, the
// field is hidden until handed over, so a plain focus() would land nowhere.
function focusModalText() {
  if (flag("mdPreview") && !mdEditing && !modal.mdWrap.classList.contains("hidden")) {
    mdEditing = true;
    syncMdUi(true);
  }
  modal.text.focus();
}

// Word-level diff between two texts via a classic LCS backtrack: no
// dependency, the whole thing is a couple dozen lines. O(n*m) on word count, so
// a pathologically huge pasted text falls back to plain text (DIFF_WORD_CAP)
// rather than freezing the editor while it opens the history panel.
const DIFF_WORD_CAP = 4000;
function diffWords(oldText, newText) {
  const tokenize = (s) => (s || "").split(/(\s+)/).filter((t) => t !== "");
  const a = tokenize(oldText);
  const b = tokenize(newText);
  const n = a.length, m = b.length;
  // Cap on the PRODUCT n*m (the actual DP cost), not each side independently --
  // a long old text diffed against a short new one (or vice versa) is cheap either way.
  if (n * m > DIFF_WORD_CAP * DIFF_WORD_CAP) return null;
  const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const out = [];
  let i = 0, j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) { out.push({ type: "same", text: a[i] }); i++; j++; }
    else if (dp[i + 1][j] >= dp[i][j + 1]) { out.push({ type: "del", text: a[i] }); i++; }
    else { out.push({ type: "add", text: b[j] }); j++; }
  }
  while (i < n) out.push({ type: "del", text: a[i++] });
  while (j < m) out.push({ type: "add", text: b[j++] });
  return out;
}
// One word-diff computed once, split into each side's own highlighted tokens --
// the left side keeps "same"/"del" (what it has that the right does not), the
// right side keeps "same"/"add" (what it has that the left does not). Both the
// duplicate finder's own columns and the free "compare two entries" view call
// this, instead of each running its own comparison.
function splitDiffSides(leftText, rightText) {
  const tokens = diffWords(leftText, rightText);
  if (!tokens) return null; // DIFF_WORD_CAP fallback -- caller shows plain text
  return {
    left: tokens.filter((tk) => tk.type !== "add"),
    right: tokens.filter((tk) => tk.type !== "del"),
  };
}
// Text nodes only: never innerHTML on prompt content, same rule the Markdown
// preview follows (a prompt's own text is fully user-controlled).
function renderDiffInto(el, tokens) {
  el.textContent = "";
  for (const t of tokens) {
    if (t.type === "same") {
      el.appendChild(document.createTextNode(t.text));
      continue;
    }
    const span = document.createElement("span");
    span.className = t.type === "add" ? "diff-add" : "diff-del";
    span.textContent = t.text;
    el.appendChild(span);
  }
}

// A snapshot's full entry list: index 0 is the base text, 1..n are rotation variants.
const versionEntries = (v) => (v.variants?.length ? [v.text || "", ...v.variants] : [v.text || ""]);
// The editor's current, unsaved state: the "newer" side of the newest row's diff.
const liveEntries = () => (modalState?.rotateEntries?.length ? [...modalState.rotateEntries] : [modal.text.value || ""]);
async function loadHistory(id) {
  const body = $("modal-history-body");
  if (!body || !id) return;
  body.innerHTML = "";
  let list = [];
  try { list = await invoke("list_versions", { promptId: id }); } catch (_) {}
  // A slower response for a since-abandoned prompt must not land in the editor
  // after it has already moved on to a different one.
  if (modalState?.id !== id) return;
  if (!list.length) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t("historyEmpty");
    body.appendChild(e);
    return;
  }
  list.forEach((v, idx) => {
    // Diff against whatever came right after this snapshot: the live editor
    // state for the newest row, the next-newer snapshot for every other one. Each
    // side is the FULL entry list (rotation variants included), not just entry 1.
    const olderEntries = versionEntries(v);
    const newerEntries = idx === 0 ? liveEntries() : versionEntries(list[idx - 1]);
    const newerName = idx === 0 ? modal.name.value : list[idx - 1].name;
    const row = document.createElement("div");
    row.className = "hist-row";
    const head = document.createElement("div");
    head.className = "hist-head";
    const time = document.createElement("span");
    time.className = "hist-time";
    time.textContent = new Date(v.ts * 1000).toLocaleString(LANG);
    const prev = document.createElement("span");
    prev.className = "hist-preview";
    prev.textContent = (v.name ? v.name + ": " : "") + olderEntries[0].slice(0, HISTORY_PREVIEW_LEN);
    head.append(time, prev);
    if (olderEntries.length > 1) {
      const chip = document.createElement("span");
      chip.className = "hist-entries";
      chip.textContent = t("historyEntriesN").replace("{n}", String(olderEntries.length));
      head.appendChild(chip);
    }
    // Explicit expand chevron (collapsed by default) → reveals the COMPLETE text.
    const expand = document.createElement("button");
    expand.type = "button";
    expand.className = "hist-expand";
    expand.title = t("historyExpandToggle");
    expand.setAttribute("aria-expanded", "false");
    expand.innerHTML = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M6 9l6 6 6-6"/></svg>';
    const restore = document.createElement("button");
    restore.type = "button";
    restore.className = "ghost-btn hist-restore";
    restore.textContent = t("historyRestore");
    restore.addEventListener("click", async () => {
      try {
        const p = await invoke("restore_version", { versionId: v.id });
        // The editor may have moved on to a different (or no) prompt while this
        // invoke was in flight: never write a stale restore into the wrong modal.
        if (modalState?.id !== id) return;
        if (p) {
          const pIdx = prompts.findIndex((x) => x.id === p.id);
          if (pIdx >= 0) prompts[pIdx] = p;
          modal.name.value = p.name;
          modal.text.value = p.text;
          // The open editor must pick up the restored rotation entries too, or the
          // next save writes the still-open (stale) entries back over them.
          modalState.rotateEntries = (p.variants || []).length ? [p.text, ...p.variants] : [];
          modalState.rotateMode = p.rotate_mode || "";
          renderRotateUi();
          updateLenCounter();
          toast(t("historyRestored"));
          await renderGrid(true);
          loadHistory(id); // the restore itself added a fresh snapshot
        }
      } catch (err) {
        const s = String(err);
        toast(s === "save-failed" || s === "snapshot-failed" ? t("persistFailed") : s);
      }
    });
    // Keeps the current entry untouched and adds the snapshot as a separate one
    // instead, for comparing them side by side rather than losing whichever
    // version restore_version would have overwritten.
    const restoreNew = document.createElement("button");
    restoreNew.type = "button";
    restoreNew.className = "ghost-btn hist-restore-new";
    restoreNew.textContent = t("historyRestoreAsNew");
    restoreNew.addEventListener("click", async () => {
      try {
        const suffix = t("historyRestoreAsNewSuffix").replace("{date}", new Date(v.ts * 1000).toLocaleString(LANG));
        const created = await invoke("restore_version_as_new", { versionId: v.id, suffix });
        // Same staleness guard as the restore button above: the editor may have
        // moved on while this invoke was in flight.
        if (modalState?.id !== id) return;
        if (created) {
          closeModal();
          await renderGrid();
          focusTile(created.id); // "gets selected after creation" -- see what happened
          toast(t("historyRestoredAsNew"));
        }
      } catch (err) {
        const s = String(err);
        toast(s === "save-failed" ? t("persistFailed") : s);
      }
    });
    head.append(expand, restore, restoreNew);
    const full = document.createElement("div");
    full.className = "hist-full hidden";
    // Building this (word-diffing every entry) is expensive at long prompts /
    // many rotation entries; each row starts collapsed, so defer it to first expand
    // and cache the result -- most rows are never opened at all.
    let fullBuilt = false;
    const buildFull = () => {
      if (fullBuilt) return;
      fullBuilt = true;
      if (v.name !== newerName) {
        const nameRow = document.createElement("div");
        nameRow.className = "hist-name-change";
        const label = document.createElement("span");
        label.className = "hist-name-label";
        label.textContent = t("historyNameChanged") + ": ";
        const oldSpan = document.createElement("span");
        oldSpan.className = "diff-del";
        oldSpan.textContent = v.name || "";
        const newSpan = document.createElement("span");
        newSpan.className = "diff-add";
        newSpan.textContent = newerName || "";
        nameRow.append(label, oldSpan, document.createTextNode(" → "), newSpan);
        full.appendChild(nameRow);
      }
      if (olderEntries.every((e) => !e) && newerEntries.every((e) => !e)) {
        const hint = document.createElement("div");
        hint.className = "hint";
        hint.textContent = t("historyNoText");
        full.appendChild(hint);
      } else {
        const maxLen = Math.max(olderEntries.length, newerEntries.length);
        for (let i = 0; i < maxLen; i++) {
          const hasOlder = i < olderEntries.length;
          const hasNewer = i < newerEntries.length;
          const block = document.createElement("div");
          if (maxLen > 1) {
            block.className = "hist-entry-block";
            const num = document.createElement("div");
            num.className = "hist-entry-num";
            num.textContent = String(i + 1);
            block.appendChild(num);
          }
          const content = document.createElement("div");
          content.className = "hist-entry-content";
          if (!hasNewer) {
            const span = document.createElement("span");
            span.className = "diff-del";
            span.textContent = olderEntries[i];
            content.appendChild(span);
          } else if (!hasOlder) {
            const span = document.createElement("span");
            span.className = "diff-add";
            span.textContent = newerEntries[i];
            content.appendChild(span);
          } else if (!flag("historyDiff")) {
            content.textContent = olderEntries[i]; // word diff switched off, show the plain old text
          } else {
            const tokens = diffWords(olderEntries[i], newerEntries[i]);
            if (tokens) renderDiffInto(content, tokens);
            else content.textContent = olderEntries[i]; // DIFF_WORD_CAP fallback: plain text
          }
          block.appendChild(content);
          full.appendChild(block);
        }
      }
    };
    const toggle = () => {
      const open = !full.classList.toggle("hidden");
      if (open) buildFull();
      expand.setAttribute("aria-expanded", String(open));
      row.classList.toggle("open", open);
    };
    expand.addEventListener("click", toggle);
    prev.addEventListener("click", toggle); // clicking the preview also expands
    row.append(head, full);
    body.appendChild(row);
  });
}

wireManagement();

// ---- OCR + voice language pack management, one shared dialog ----
// #ocr-langs-modal serves both OCR and voice dictation language packs -- same
// markup, same flow (Windows-reported capability list, elevated listing/install),
// just pointed at a different set of Tauri commands and settings keys. `kind`
// ("ocr" | "speech") selects which.
// *_capabilities() lists every capability Windows knows about (installed AND
// not) with size/version straight from Windows -- a different list from
// ocrLangsCache/voiceLangsCache above, which are only the installed set actually
// used to pick a recognition language.
const LANG_CMDS = {
  ocr: {
    capabilities: "ocr_capabilities", elevate: "ocr_capabilities_elevate",
    elevatedResult: "ocr_capabilities_elevated_result",
    install: "ocr_install_language", installResult: "ocr_install_result",
    languages: "ocr_languages",
  },
  speech: {
    capabilities: "speech_capabilities", elevate: "speech_capabilities_elevate",
    elevatedResult: "speech_capabilities_elevated_result",
    install: "speech_install_language", installResult: "speech_install_result",
    languages: "voice_languages",
  },
};
// Per-kind dialog state, kept independent so switching which dialog is open
// never loses the other's in-flight scan or install polls.
const LANG_DIALOG = {
  // installing: the names whose elevated install was launched and has not reported
  // back yet. It outlives the dialog (only the pollers stop on close), so a reopened
  // dialog still knows the row is busy. installChain marks the one live poller per name.
  ocr: { cache: [], checked: 0, installTimers: {}, installPending: {}, installErrors: {}, installing: new Set(), installChain: {}, elevTimer: null, elevRunning: false, elevLeft: 60 },
  speech: { cache: [], checked: 0, installTimers: {}, installPending: {}, installErrors: {}, installing: new Set(), installChain: {}, elevTimer: null, elevRunning: false, elevLeft: 60 },
};
let langDialogKind = "ocr"; // which kind #ocr-langs-modal currently displays
function openLangsModal(kind) {
  langDialogKind = kind;
  $("ocr-langs-title").textContent = t(kind === "speech" ? "voiceLangsManage" : "ocrLangsManage");
  $("ocr-langs-hint").textContent = t(kind === "speech" ? "voiceLangsHint" : "ocrLangsHint");
  $("ocr-langs-modal").classList.remove("hidden");
  const st = LANG_DIALOG[kind];
  // A scan started on a previous open (or from the elevation retry button)
  // may still be running server-side -- resume polling it instead of the dialog
  // silently doing nothing until the user re-elevates from scratch.
  if (st.elevRunning) { renderLangsLoading(kind); pollElevatedCaps(kind); }
  else loadLangCaps(kind, false);
  // Installs started before the dialog was closed keep running as OS processes:
  // pick their result polling back up.
  for (const name of st.installing) pollLangInstall(kind, name);
}
function closeLangsModal() {
  $("ocr-langs-modal").classList.add("hidden");
  const st = LANG_DIALOG[langDialogKind];
  // Installs and an elevated scan are OS processes and keep running regardless
  // -- only the polling loop (and the DOM it repaints) needs to stop while
  // nobody is looking. elevRunning itself is left set so reopening resumes.
  for (const id of Object.values(st.installTimers)) clearTimeout(id);
  st.installTimers = {};
  st.installChain = {};
  if (st.elevTimer) { clearTimeout(st.elevTimer); st.elevTimer = null; }
}
async function loadLangCaps(kind, force) {
  const st = LANG_DIALOG[kind];
  if (!st.cache.length || force) renderLangsLoading(kind);
  try {
    st.cache = await invoke(LANG_CMDS[kind].capabilities, { force: !!force });
    st.checked = Math.floor(Date.now() / 1000);
  } catch (e) {
    // Listing is supposed to need no elevated rights, but some Windows builds
    // require it even for this read-only query -- an honest inline reason
    // (plus a retry, 162) beats a blank dialog or a generic error toast. 177:
    // ANY failure here comes back this same way now (backend no longer tries
    // to classify by HResult/message text, which varies by machine/locale) --
    // "code|raw text" so the raw PowerShell/DISM reason can still be shown.
    const s = String(e);
    if (s.startsWith("ocr-caps-need-elevation")) {
      renderLangsRetry(kind, t("ocrLangsNeedElevation"), s.slice(s.indexOf("|") + 1));
    } else errToast(e);
    return;
  }
  renderLangsBody(kind);
}
// Runs the capability listing through the UAC prompt on machines that refuse it
// to a normal user. Same shape as an install: explain first, launch, then poll
// the result the elevated process writes.
async function startElevatedCapsScan(kind) {
  const ok = await confirmDialog({
    title: t("ocrElevateTitle"),
    message: t("ocrCapsElevateInfo"),
    confirmLabel: t("ocrLangsElevateBtn"),
    danger: false,
  });
  if (!ok) return;
  const st = LANG_DIALOG[kind];
  renderLangsLoading(kind);
  try {
    await invoke(LANG_CMDS[kind].elevate);
  } catch (e) {
    // A launch failure also gets the notice+retry pair, not a dead end --
    // except when the account cannot elevate at all: that gets its own
    // explanation instead, with no retry button, because trying again can
    // never succeed.
    const err = String(e);
    if (err === "ocr-elevate-no-rights") {
      renderLangsRetry(kind, `${t("capNeedsAdmin")} ${t("capNeedsAdminWhat")}`, null, true);
    } else {
      renderLangsRetry(kind, err === "ocr-elevate-cancelled" ? t("ocrElevateCancelled") : err);
    }
    return;
  }
  st.elevRunning = true;
  st.elevLeft = 60;
  pollElevatedCaps(kind);
}
// Polls the elevated process's result file a few seconds apart. Re-entrant: can
// be called again after the dialog was closed and reopened without losing
// the two-minute budget already spent (elevLeft lives on LANG_DIALOG, not in a
// closure local, so it survives that close/reopen).
function pollElevatedCaps(kind) {
  const st = LANG_DIALOG[kind];
  const tick = async () => {
    st.elevTimer = null;
    let res;
    try { res = await invoke(LANG_CMDS[kind].elevatedResult); }
    catch (e) { st.elevRunning = false; renderLangsRetry(kind, String(e)); return; }
    if (res == null) {
      if (--st.elevLeft <= 0) { st.elevRunning = false; renderLangsRetry(kind, t("ocrLangsNeedElevation")); return; }
      st.elevTimer = setTimeout(tick, 2000);
      return;
    }
    st.elevRunning = false;
    st.cache = res;
    st.checked = Math.floor(Date.now() / 1000);
    renderLangsBody(kind);
  };
  st.elevTimer = setTimeout(tick, 2000);
}
function renderLangsLoading(kind) {
  if (langDialogKind !== kind) return;
  const body = $("ocr-langs-body");
  body.innerHTML = "";
  const loading = document.createElement("div");
  loading.className = "hint";
  loading.textContent = t("ocrLangsScanning");
  body.appendChild(loading);
}
// Notice text and its retry button are always built together, in one
// place -- a failure (launch or the two-minute poll timeout alike) can no
// longer replace the body with text only and strand the user without a way
// to try again. `details` is the raw PowerShell/DISM text behind a
// translated reason like `message` -- shown collapsed so it never crowds out
// the actual instruction, but never silently dropped either. `noRetry` drops
// the button entirely: set when the account can never elevate, where another
// attempt cannot succeed and offering one would just be misleading.
function renderLangsRetry(kind, message, details, noRetry) {
  if (langDialogKind !== kind) return;
  const body = $("ocr-langs-body");
  body.innerHTML = "";
  const note = document.createElement("div");
  note.className = "hint";
  note.textContent = message;
  body.appendChild(note);
  if (details && details.trim()) {
    const disc = document.createElement("details");
    disc.className = "lang-error-details";
    const summary = document.createElement("summary");
    summary.textContent = t("ocrLangsErrorDetails");
    disc.appendChild(summary);
    const pre = document.createElement("div");
    pre.className = "lang-error-details-text";
    pre.textContent = details.trim();
    disc.appendChild(pre);
    body.appendChild(disc);
  }
  if (noRetry) return;
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "ghost-btn";
  btn.textContent = t("ocrLangsElevateBtn");
  btn.addEventListener("click", () => startElevatedCapsScan(kind));
  body.appendChild(btn);
}
function renderLangsBody(kind) {
  if (langDialogKind !== kind) return; // a stale async result for a dialog that moved on
  const st = LANG_DIALOG[kind];
  const body = $("ocr-langs-body");
  body.innerHTML = "";
  for (const cap of st.cache) body.appendChild(langRow(kind, cap));
  $("ocr-langs-checked").textContent = st.checked
    ? t("ocrLangsLastChecked").replace("{t}", fmtCopyTime(st.checked))
    : t("ocrLangsNeverChecked");
}
function langRow(kind, cap) {
  const st = LANG_DIALOG[kind];
  const row = document.createElement("div");
  row.className = "clip-row ocr-lang-row lang-row";
  const main = document.createElement("div");
  main.className = "lang-row-main";
  const name = document.createElement("div");
  name.className = "lang-row-name";
  name.textContent = cap.label;
  const sub = document.createElement("div");
  sub.className = "lang-row-sub";
  sub.textContent = `${cap.tag} · ${cap.version || "—"}`;
  main.append(name, sub);
  const installed = cap.state === "Installed";
  const size = installed ? cap.install_size : cap.download_size;
  const sizeEl = document.createElement("div");
  sizeEl.className = "lang-row-size";
  // Windows does not always report a size -- never guess one, show the dash instead.
  sizeEl.textContent = size == null ? "—" : fmtBytes(size);
  const chip = document.createElement("span");
  chip.className = installed ? "lang-row-chip installed" : "lang-row-chip";
  chip.textContent = installed ? t("ocrLangsInstalled") : t("ocrLangsNotInstalled");
  const actions = document.createElement("div");
  actions.className = "clip-actions";
  row.append(main, sizeEl, chip, actions);
  if (installed) {
    const check = document.createElement("span");
    check.className = "ocr-lang-check";
    check.textContent = "✓";
    check.title = t("ocrLangsInstalled");
    actions.appendChild(check);
    return row;
  }
  const note = st.installErrors[cap.name];
  if (note) {
    const hint = document.createElement("span");
    hint.className = "ocr-lang-note";
    // "no-rights" is the account-can-never-elevate case, its own text below;
    // any other string is the backend's own reason (a real install failure);
    // `true` is the calm "you declined the rights prompt" case.
    hint.textContent =
      note === "no-rights" ? `${t("capNeedsAdmin")} ${t("capNeedsAdminWhat")}` : typeof note === "string" ? note : t("ocrElevateCancelled");
    actions.appendChild(hint);
  }
  if (note === "no-rights") return row; // a retry can never succeed here -- no download button
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "ghost-btn";
  const busy = st.installing.has(cap.name) || !!st.installTimers[cap.name] || !!st.installPending[cap.name];
  btn.disabled = busy;
  // The size sits INSIDE the button label on purpose -- nobody should start a
  // download blind.
  btn.textContent = busy
    ? t("ocrLangsInstalling")
    : t("ocrLangsDownloadBtn").replace("{size}", cap.download_size == null ? "—" : fmtBytes(cap.download_size));
  btn.addEventListener("click", () => startLangInstall(kind, cap));
  actions.appendChild(btn);
  return row;
}
// Every path that can lead to an elevated install goes through here first:
// explain the language, its size and that Windows will ask for admin rights
// once, BEFORE that prompt ever appears. No exceptions.
async function startLangInstall(kind, cap) {
  const st = LANG_DIALOG[kind];
  const size = cap.download_size == null ? "—" : fmtBytes(cap.download_size);
  const ok = await confirmDialog({
    title: t("ocrElevateTitle"),
    message: t("ocrElevateInfo").replace("{lang}", cap.label).replace("{size}", size),
    confirmLabel: t("ocrElevateInstall"),
    danger: false,
  });
  if (!ok) return;
  delete st.installErrors[cap.name];
  // Marks the row busy for the WHOLE gap, including the elevated install()
  // call itself (which blocks until the user answers the UAC prompt) -- set
  // synchronously, before any await, so a second click on the still-enabled
  // button can't start a second concurrent elevated install for the same
  // language while the first one is still waiting on Windows.
  st.installPending[cap.name] = true;
  renderLangsBody(kind);
  try {
    await invoke(LANG_CMDS[kind].install, { name: cap.name });
  } catch (e) {
    delete st.installPending[cap.name];
    // A declined UAC prompt is not an error: a calm inline note on the row,
    // no error tone, the row stays "not installed". An account that cannot
    // elevate at all gets its own note instead, and no download button --
    // clicking it again can never work.
    const err = String(e);
    if (err === "ocr-elevate-no-rights") st.installErrors[cap.name] = "no-rights";
    else if (err === "ocr-elevate-cancelled") st.installErrors[cap.name] = true;
    else errToast(e);
    // Every branch above just deleted installPending -- the row rendered disabled
    // ("Installing...") right before the await must be repainted regardless of
    // which branch ran, or the button stays stuck disabled until the dialog is
    // closed and reopened.
    renderLangsBody(kind);
    return;
  }
  delete st.installPending[cap.name];
  // Arm the poller (marks the row busy) BEFORE repainting, or the render still
  // reads installTimers[cap.name] as empty and shows an enabled Download button --
  // a second click then starts a second UAC-elevated install.
  pollLangInstall(kind, cap.name);
  renderLangsBody(kind);
}
// *_install_language() only resolves once the elevated process has LAUNCHED,
// not once it finished -- poll the result file ourselves, a few seconds apart,
// until it stops being null.
// A result file that never becomes readable (permanently corrupt, or wiped
// from under the install) used to look exactly like "still running" forever --
// polled every 3s with no way out. ~10 minutes is generous for even a large,
// slow-connection language pack, so hitting this is always a real stuck state.
const MAX_LANG_INSTALL_POLLS = 200;
function pollLangInstall(kind, name) {
  const st = LANG_DIALOG[kind];
  if (st.installTimers[name]) return;
  st.installing.add(name);
  const chain = {};
  st.installChain[name] = chain;
  let attempts = 0;
  // setTimeout re-armed after each result, not setInterval -- the callback
  // awaits an invoke, and a reply slower than the 3s tick would otherwise overlap
  // itself (two install_result calls, or two clearInterval/repaint races).
  const tick = async () => {
    attempts++;
    let res;
    try { res = await invoke(LANG_CMDS[kind].installResult, { name }); } catch (_) { res = undefined; }
    // The dialog was closed (and maybe reopened, with a fresh poller) while that
    // answer was on its way: this chain is stale and must not re-arm itself. A
    // terminal answer is still handled, though: the backend already deleted the
    // result and dropped its nonce, so no later poll will ever see it again.
    const stale = st.installChain[name] !== chain;
    if (res == null) {
      if (stale) return;
      if (attempts >= MAX_LANG_INSTALL_POLLS) {
        delete st.installTimers[name];
        st.installing.delete(name);
        st.installErrors[name] = t("ocrInstallResultTimeout");
        renderLangsBody(kind);
        return;
      }
      st.installTimers[name] = setTimeout(tick, 3000); // still running (or a transient probe failure)
      return;
    }
    // A poller started by a reopen in the meantime has nothing left to wait for.
    clearTimeout(st.installTimers[name]);
    delete st.installTimers[name];
    delete st.installChain[name];
    st.installing.delete(name);
    if (res.ok) {
      // Clear both caches so the new pack is selectable immediately, no restart.
      invoke(LANG_CMDS[kind].languages).then((langs) => {
        if (kind === "speech") assignVoiceLangsCache(langs || []); else assignOcrLangsCache(langs || []);
      }).catch(() => {});
      await loadLangCaps(kind, true); // force -- also drops the backend's own cache
      return;
    }
    // A real failure (e.g. Windows Home / policy blocked) -- the reason belongs
    // in the row, never a silent failure. The backend's own internal keys are
    // translated here; anything else is a raw Windows exception message, shown
    // exactly as Windows worded it since there is no way to pre-translate that.
    st.installErrors[name] = res.error === "install-result-unreadable" ? t("ocrInstallResultUnreadable") : (res.error || true);
    renderLangsBody(kind);
  };
  st.installTimers[name] = setTimeout(tick, 3000);
}
// Thin OCR-flavoured wrapper so the expert-action entry below keeps working
// unchanged -- the generalization lives entirely behind `kind`.
function openOcrLangsModal() { openLangsModal("ocr"); }
function openVoiceLangsModal() { openLangsModal("speech"); }
EXPERT_ACTIONS["ocr-langs-open"] = openOcrLangsModal;
EXPERT_ACTIONS["voice-langs-open"] = openVoiceLangsModal;
// "ocr-no-language"/"voice-no-language" always have the same fix: open this
// dialog. Used at every call site that would otherwise show them as a bare errToast().
function ocrErrToast(err) {
  if (String(err) === "ocr-no-language") {
    toast(t("ocrNoLanguageMsg"), { label: t("ocrLangsManage"), onClick: openOcrLangsModal });
    return;
  }
  errToast(err);
}

// ---- Voice dictation session control ----
let voiceSessionId = null;
let voiceToken = 0; // frontend-chosen, sent to voice_start and echoed on every event -- the only identity events are matched on
let voiceState = "idle"; // idle | arming | listening
let voiceTargetEl = null;
let voiceActiveBtn = null;
let voiceInterim = null; // {start, len} inside voiceTargetEl.value, or null
let voiceSuppressInput = false; // true while a programmatic .value write is in flight
let voiceResolvedLang = ""; // BCP-47 tag voice_start actually recognized with, for command post-processing
let voiceArmCancelled = false; // stopVoice() was called before voice_start's invoke() resolved
// The just-ended session's target, for one late-arriving voice-text that WinRT's
// unordered ResultGenerated/Completed handlers let land after voice-stopped
// already fired -- delivered instead of dropped, but only within the grace
// window and only to ITS OWN target, never adopted by whatever session (if any)
// has started in the meantime. Not an expert setting -- internal timing.
let voicePrev = null; // {token, el, caret, lang, hotkey, endedAt, interim} | null
const VOICE_LATE_GRACE_MS = 3000;

// The two-letter code voice.js's command tables are keyed by, derived from
// the BCP-47 tag voice_start resolved to -- NOT the UI's own display
// language, which can easily differ from what is actually being recognized
// (e.g. a German UI with only an English speech pack installed). Falls back
// to the UI language before a session has resolved one at all (the hotkey
// path's first partial can in rare cases race ahead of it).
function voiceLangCode() {
  return (voiceResolvedLang || LANG).slice(0, 2).toLowerCase();
}
function voiceButtonsAll() { return [$("voice-btn"), modal.voiceBtn].filter(Boolean); }
function setVoiceBtnState(activeBtn, cls) {
  for (const b of voiceButtonsAll()) {
    b.classList.remove("is-arming", "is-listening");
    b.setAttribute("aria-pressed", "false");
    b.title = t("voiceBtnTitle");
  }
  if (!activeBtn || !cls) return;
  activeBtn.classList.add(cls);
  if (cls === "is-listening") {
    activeBtn.setAttribute("aria-pressed", "true");
    activeBtn.title = t("voiceStopTitle");
  }
}
// The button stays in its "arming" in-between state (so a fast double-click
// cannot start a second session) until the first backend event, of any kind,
// proves the session is actually alive.
function resolveVoiceArming() {
  if (voiceState === "arming") {
    voiceState = "listening";
    setVoiceBtnState(voiceActiveBtn, "is-listening");
  }
}
function endVoiceUi() {
  if (voiceTargetEl) {
    voicePrev = {
      token: voiceToken,
      el: voiceTargetEl,
      caret: voiceTargetEl.selectionEnd ?? null,
      // The live partial still sitting in the field: the final text of this very
      // session replaces it instead of landing next to it.
      interim: voiceInterim
        ? { start: voiceInterim.start, len: voiceInterim.len, text: voiceTargetEl.value.substr(voiceInterim.start, voiceInterim.len) }
        : null,
      lang: voiceResolvedLang,
      hotkey: voiceTargetEl === voiceHotkeyBuffer,
      endedAt: Date.now(),
    };
  }
  voiceState = "idle";
  voiceSessionId = null;
  voiceTargetEl = null;
  voiceInterim = null;
  voiceResolvedLang = "";
  setVoiceBtnState(null, null);
  voiceActiveBtn = null;
}
async function startVoice(target, btn) {
  if (voiceState !== "idle" || !target) return;
  voiceState = "arming";
  voiceTargetEl = target;
  voiceActiveBtn = btn;
  voiceInterim = null;
  voiceArmCancelled = false;
  voiceToken = (voiceToken % 1e9) + 1;
  setVoiceBtnState(btn, "is-arming");
  let res;
  try {
    res = await invoke("voice_start", {
      lang: txt("voiceLang") || "auto",
      autoStopSec: flag("voiceAutoStop") ? Math.round(val("voiceAutoStopSec")) : 0,
      maxSec: Math.round(val("voiceMaxMin")) * 60,
      interim: flag("voiceInterim"),
      token: voiceToken,
    });
  } catch (e) {
    endVoiceUi();
    routeVoiceError(e);
    return;
  }
  recordToolUsage("dictation");
  const { id, lang } = res;
  // A cancel that happened while this call was in flight (voice_start's own
  // spawn_blocking round trip is not instant) has to stop the session it
  // just now learned the id of -- stopVoice() itself had nothing to send.
  if (voiceArmCancelled) {
    voiceArmCancelled = false;
    invoke("voice_stop", { id }).catch(() => {});
    // Only reset the UI if no backend event has already adopted this session
    // (see voiceEventIsMine) -- if one has, its own listener owns the reset.
    if (voiceSessionId === null && voiceState === "arming") endVoiceUi();
    return;
  }
  if (voiceSessionId === null) voiceSessionId = id;
  voiceResolvedLang = lang || voiceResolvedLang;
}
async function stopVoice() {
  if (voiceState === "arming" && voiceSessionId === null) {
    // Nothing to send yet -- voice_start hasn't resolved with an id. Mark the
    // cancel and let startVoice() finish the job the moment it does.
    voiceArmCancelled = true;
    return;
  }
  if (!voiceSessionId) return;
  try { await invoke("voice_stop", { id: voiceSessionId }); } catch (_) {}
  // The UI resets from the voice-stopped event the backend always sends back,
  // not here -- the button never claims "off" before the session (and its
  // mic) has actually finished.
}
// True if a token-tagged backend event belongs to the session currently
// starting or running. voiceToken is chosen by the frontend and sent to
// voice_start itself, so it is known before any event can possibly arrive --
// unlike the backend-assigned id, it is never adopted from an event. The two
// WinRT handlers (ResultGenerated, Completed) are not ordered against each
// other, so a session that already reported voice-stopped can still deliver
// one more voice-text after that -- its token no longer matches here and it
// falls to deliverLateVoiceText instead of being silently misattributed to
// whatever session (if any) is now starting.
function voiceEventIsMine(payload) {
  return voiceState !== "idle" && !!payload && payload.token === voiceToken;
}
// A late voice-text for the session endVoiceUi just closed out (see voicePrev)
// -- delivered to ITS OWN target within VOICE_LATE_GRACE_MS instead of being
// dropped, and never touches voiceState/voiceSessionId/the buttons, since
// there is no live session left for this to belong to.
function deliverLateVoiceText(payload) {
  const prev = voicePrev;
  if (!prev || !payload || payload.token !== prev.token) return;
  if (Date.now() - prev.endedAt >= VOICE_LATE_GRACE_MS) return;
  voicePrev = null;
  const text = payload.text || "";
  if (!text) return;
  const processed = applyVoiceCommands(text, {
    punct: flag("voicePunct"),
    autoCaps: flag("voiceAutoCaps"),
    lang: (prev.lang || LANG).slice(0, 2).toLowerCase(),
  });
  if (prev.hotkey) { deliverVoiceHotkeyText(processed); return; }
  const el = prev.el;
  if (!el || el.classList.contains("hidden")) return;
  const part = prev.interim;
  if (part && el.value.substr(part.start, part.len) === part.text) {
    // Still exactly the partial the session left behind: swap it for the final text.
    el.value = el.value.slice(0, part.start) + processed + el.value.slice(part.start + part.len);
    el.selectionStart = el.selectionEnd = part.start + processed.length;
    el.dispatchEvent(new Event("input", { bubbles: true }));
    return;
  }
  const atEnd = txt("voiceInsert") === "end";
  const at = atEnd ? el.value.length : (prev.caret ?? el.value.length);
  insertLateVoiceText(el, processed, at);
}
function wireVoiceButton(btn, targetFn) {
  if (!btn) return;
  btn.addEventListener("click", () => {
    if (voiceState !== "idle") { stopVoice(); return; }
    startVoice(targetFn(), btn);
  });
}
async function openVoiceOnlineConsent() {
  const ok = await confirmDialog({
    title: t("voiceOnlineTitle"),
    message: t("voiceOnlineInfo"),
    confirmLabel: t("voiceOnlineAllow"),
    danger: false,
  });
  if (!ok) return;
  try { await invoke("voice_enable_online_speech"); } catch (e) { errToast(e); return; }
  settings.ui_flags = { ...(settings.ui_flags || {}), voiceOnlineOk: true };
  applyFlags();
}
function routeVoiceError(err) {
  const code = String(err);
  // No error toast for this one -- the fix is always the same, so the first
  // click just opens the dialog that offers it (the OCR button does the same).
  if (code === "voice-no-language") { openVoiceLangsModal(); return; }
  // voice-privacy-off is the same missing consent voice-online-required already
  // guards against -- Windows' own registry value, so the same dialog fixes it.
  if (code === "voice-online-required" || code === "voice-privacy-off") { openVoiceOnlineConsent(); return; }
  if (code === "voice-denied") {
    toast(t("voiceDeniedMsg"), { label: t("voiceOpenPrivacy"), onClick: () => invoke("open_privacy_mic").catch(() => {}) });
    return;
  }
  const key = { "voice-busy": "voiceErrBusy", "voice-failed": "voiceErrFailed" }[code];
  errToast(key ? t(key) : code);
}
// Joins a new recognized chunk onto whatever the target already holds: one
// space, except right after a newline or an opening bracket, and never a
// second space next to one already there.
function voiceJoiner(before) {
  if (!before) return "";
  if (/[\n(]$/.test(before)) return "";
  if (/\s$/.test(before)) return "";
  return " ";
}
// interim: true for a live voice-partial update (replaced by the next update
// or the final result), false for the committed voice-text result. The marker
// (voiceInterim) is what makes a partial replaceable in place instead of
// piling up duplicate text on every update; tracked per the target element,
// discarded the moment the user types or moves the cursor themselves (see the
// discard listeners below), at which point it just stays as plain text.
function insertVoiceText(text, interim) {
  const el = voiceTargetEl;
  if (!el || el.classList.contains("hidden")) return; // its field disappeared mid-session
  const value = el.value;
  let start, replaceLen;
  if (voiceInterim) {
    start = voiceInterim.start;
    replaceLen = voiceInterim.len;
  } else {
    const atEnd = txt("voiceInsert") === "end";
    start = atEnd ? value.length : (el.selectionStart ?? value.length);
    replaceLen = 0;
  }
  const joiner = voiceInterim ? "" : voiceJoiner(value.slice(0, start));
  const newValue = value.slice(0, start) + joiner + text + value.slice(start + replaceLen);
  const newStart = start + joiner.length;
  voiceSuppressInput = true;
  el.value = newValue;
  el.selectionStart = el.selectionEnd = newStart + text.length;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  voiceSuppressInput = false;
  voiceInterim = interim ? { start: newStart, len: text.length } : null;
}
// Sibling of insertVoiceText for deliverLateVoiceText's one-shot delivery --
// deliberately does not touch voiceInterim/voiceSuppressInput, since there is
// no live session left for the discard-tracking those belong to.
function insertLateVoiceText(el, text, at) {
  const value = el.value;
  const joiner = voiceJoiner(value.slice(0, at));
  el.value = value.slice(0, at) + joiner + text + value.slice(at);
  el.selectionStart = el.selectionEnd = at + joiner.length + text.length;
  el.dispatchEvent(new Event("input", { bubbles: true }));
}
// The recognized text is still ours to lay claim to right up until the user
// visibly acts on the field themselves -- then the interim span is just text.
function voiceWatchDiscard(el) {
  if (!el) return;
  const discard = () => { if (voiceInterim && voiceTargetEl === el) voiceInterim = null; };
  el.addEventListener("input", () => { if (!voiceSuppressInput) discard(); });
  el.addEventListener("click", discard);
  el.addEventListener("keydown", (e) => {
    if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"].includes(e.key)) discard();
  });
}
// 131's global hotkey has no visible field to dictate into (the window is
// never brought forward), so it targets this detached textarea instead --
// never added to the DOM, but a real element, so it rides the exact same
// insertVoiceText/voiceJoiner pipeline as the two visible buttons rather than
// a second copy of it.
const voiceHotkeyBuffer = document.createElement("textarea");
async function deliverVoiceHotkeyText(text) {
  if (!text.trim()) return;
  const target = txt("voiceHotkeyTarget") || "clipboard";
  if (target === "input") {
    inputEl.value = inputEl.value ? `${inputEl.value}\n${text}` : text;
    autoGrow(inputEl);
    return;
  }
  // clipboard and paste both copy first -- paste then adds the same
  // paste-into-previous-app step a prompt hotkey already uses.
  try { await invoke("copy_text", { text }); } catch (e) { errToast(e); return; }
  if (target === "paste") await autoPasteNow(null);
  else toast(t("copied"));
}

// ---- Backups & restore ----
// A backup folder name as a readable local date + time ("29.07.2026 18:15" in German,
// and whatever the picked language writes elsewhere).
function prettyStamp(name) {
  const dt = parseStamp(name);
  if (dt) return `${dt.toLocaleDateString(LANG)} ${dt.toLocaleTimeString(LANG, { hour: "2-digit", minute: "2-digit" })}`;
  const [d, tm] = String(name).split("_");
  return tm ? `${d} ${tm.replace(/-/g, ":")}` : d;
}
function parseStamp(name) {
  const [d, tm] = String(name).split("_");
  if (!d) return null;
  const dt = new Date(`${d}T${(tm || "00-00-00").replace(/-/g, ":")}`);
  return isNaN(dt.getTime()) ? null : dt;
}
async function saveBackupSetting(kind, key, value) {
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
let backupPwDraft = "";
function openBackups() {
  $("backup-modal").classList.remove("hidden");
  renderStatsPanel($("stats-panel"));
  renderStoragePanel($("storage-panel"));
  renderRetentionPanel($("retention-panel"));
  renderBackupPanel($("backup-panel"));
}
function closeBackups() {
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
async function saveUiState(key, on) {
  settings.ui_flags = { ...(settings.ui_flags || {}), [key]: on };
  await invoke("set_ui_flag", { key, enabled: on }).catch(() => {});
}

// Label key per storage-breakdown row, in the order the backend returns them.
const STORAGE_ROW_LABELS = {
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
async function renderStoragePanel(root, force) {
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
const RETENTION_ROWS = [
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
function retentionKeepText(row) {
  const n = val(row.settingKey);
  if (row.wording === "count") return t("retentionKeepsCount").replace("{n}", n);
  if (row.wording === "minutes") return t("retentionKeepsMinutes").replace("{n}", n);
  if (n <= 0) return t("retentionForever");
  return t(row.wording === "hours" ? "retentionKeepsHours" : "retentionKeepsDays").replace("{n}", n);
}

async function retentionCleanupResult(promise, nothingKey) {
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
const RETENTION_CLEANUP = {
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

async function renderRetentionPanel(root, force) {
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

async function renderBackupPanel(body) {
  // Fetch BEFORE clearing: emptying the panel and only then awaiting lets the browser
  // paint the empty state for a frame, which showed up as a visible flash on every
  // re-render (backup now, value change …).
  let backups, hadPw;
  try { [backups, hadPw] = await Promise.all([invoke("list_backups"), invoke("has_backup_password").catch(() => false)]); }
  catch (e) { errToast(e); return; }
  body.innerHTML = "";
  const nf = (n) => Number(n).toLocaleString(LANG);
  const card = (l, v) => {
    const c = document.createElement("div"); c.className = "stat-card";
    const vv = document.createElement("div"); vv.className = "stat-val"; vv.textContent = v;
    const ll = document.createElement("div"); ll.className = "stat-label"; ll.textContent = l;
    c.append(vv, ll); return c;
  };
  // Date/time card: date on top, time below (no comma), a touch larger than the
  // plain small value so the timestamps read clearly.
  const dtCard = (l, d) => {
    const c = document.createElement("div"); c.className = "stat-card";
    const vv = document.createElement("div"); vv.className = "stat-val stat-dt";
    if (d) {
      const dd = document.createElement("div"); dd.className = "dt-date"; dd.textContent = d.toLocaleDateString(LANG);
      const tt = document.createElement("div"); tt.className = "dt-time"; tt.textContent = d.toLocaleTimeString(LANG);
      vv.append(dd, tt);
    } else { vv.textContent = "—"; }
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
    try {
      await withTaskbarBusy("backup", () => invoke("create_backup", { keep: Math.round(val("backupKeep")) }));
      recordToolUsage("backup_create");
      toast(t("backupDone"));
    } catch (e) { errToast(e); }
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
    } catch (e) { toast(String(e) === "save-failed" ? t("persistFailed") : String(e)); }
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
    } catch (e) { toast(String(e) === "save-failed" ? t("persistFailed") : String(e)); }
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
    card(t("backupCount"), nf(backups.length)),
    card(t("backupTotalSize"), fmtBytes(total)),
    card(t("backupLargest"), backups.length ? fmtBytes(Math.max(...sizes)) : "—"),
    card(t("backupSmallest"), backups.length ? fmtBytes(Math.min(...sizes)) : "—"),
    card(t("backupSpan"), newest && oldest
      ? `${nf(Math.max(0, Math.round((newest - oldest) / 86400000)))} d`
      : "—"),
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
      count.textContent = b.prompts === null || b.prompts === undefined ? "—" : nf(b.prompts);
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
          chars.textContent = item.chars ? nf(item.chars) : "—";
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
async function restoreBackup(name) {
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
async function renderBackupContent(box, name) {
  box.innerHTML = "";
  let data;
  try { data = await invoke("backup_contents", { name }); }
  catch (e) { box.textContent = String(e); return; }
  const nf = (n) => Number(n).toLocaleString(LANG);
  const mkCard = (l, v) => {
    const c = document.createElement("div"); c.className = "stat-card";
    const vv = document.createElement("div"); vv.className = "stat-val"; vv.textContent = v;
    const ll = document.createElement("div"); ll.className = "stat-label"; ll.textContent = l;
    c.append(vv, ll); return c;
  };
  const cards = document.createElement("div"); cards.className = "stat-cards backup-content-cards";
  cards.append(
    mkCard(t("backupContentEntries"), nf(data.entries)),
    mkCard(t("backupContentViews"), nf(data.views)),
    mkCard(t("backupContentTags"), nf(data.tags)),
    mkCard(t("backupContentLastChange"), data.last_change ? new Date(data.last_change * 1000).toLocaleDateString(LANG) : "—"),
    mkCard(t("backupContentVersion"), data.program_version || "—"),
    mkCard(t("backupContentEncrypted"), data.encrypted ? t("backupContentEncryptedYes") : t("backupContentEncryptedNo")),
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
      dt.textContent = it.date ? new Date(it.date * 1000).toLocaleDateString(LANG) : "—";
      const sz = document.createElement("span"); sz.className = "backup-content-cell-chars"; sz.textContent = it.chars ? nf(it.chars) : "—";
      const kd = document.createElement("span"); kd.className = "backup-content-cell-kind"; kd.textContent = t(PROMPT_TYPE_LABELS[it.kind] || it.kind);
      row.append(nm, dt, sz, kd);
      rowsBox.appendChild(row);
    }
  }
  search.addEventListener("input", renderRows);
  renderRows();
}

// Keep all image/file modal controls consistent with modalState.
function syncModalImageUi(mode) {
  const { image, showImage, copyImage, filePath, iconPath } = modalState;
  const kind = filePath ? mediaKind(filePath) : "";
  const iconKind = !image && iconPath ? mediaKind(iconPath) : "";
  // Any media file previews straight from its path (image/gif/video): so an
  // image still shows even when the backend preview re-encode returned nothing.
  const fileMedia = !image && !iconKind && !!kind;
  const hasPreview = !!image || !!iconKind || fileMedia;
  // Image and file prompts have no text field: the name doubles as the copy text.
  // Plain text prompts expose it on create (review before saving) and edit.
  const textVisible = (mode === "edit" || mode === "create") && !copyImage && !filePath;
  // Full-height dialog only when editing a text prompt (max textarea space).
  modal.root.classList.toggle("tall", textVisible);
  // Only a text prompt, and only while the feature is on, ever offers rotation.
  modal.rotateToggleRow.classList.toggle("hidden", !textVisible || !flag("rotatingPrompts"));
  renderRotateUi();
  // rotationUiOn(), not rotationActive(): with the feature off but the prompt
  // still holding stored entries, the plain textarea is what must show (entry 0,
  // editable), or the editor is stuck showing a list with no way back to it (its
  // own toggle row is hidden too while the feature is off).
  const rotating = textVisible && rotationUiOn();
  modal.text.classList.toggle("hidden", !textVisible || rotating);
  modal.voiceRow.classList.toggle("hidden", !textVisible || rotating);
  const varsUi = textVisible && !rotating && flag("promptVars");
  modal.textBar.classList.toggle("hidden", !varsUi);
  modal.varsHint.classList.toggle("hidden", !varsUi);
  if (varsUi) renderVarHelper(); // chips + insert row follow their flags
  syncMdUi(textVisible, rotating); // preview toolbar + rendered pane
  updateLenCounter();
  // The variable helper is a reference, so it reopens collapsed too.
  modal.varsBody.classList.add("hidden");
  modal.varsHead.setAttribute("aria-expanded", "false");
  modal.textBar.classList.remove("open");
  // Tags/hotkey/history share one "Advanced options" collapsible, which always
  // reopens collapsed. The hotkey row exists for every prompt kind (a hotkey
  // copies whatever the prompt holds). Version history is offered for every existing
  // prompt kind: a snapshot also captures name-only changes on an image/file/colour
  // prompt, so it stays reachable there too. The outer section itself hides only if
  // none of the three apply.
  modal.hotkey.classList.toggle("hidden", !promptHotkeysOn());
  showHotkeyError("");
  const histOn = mode === "edit" && flag("promptHistory");
  $("modal-history").classList.toggle("hidden", !histOn);
  if (histOn) $("modal-history-body").innerHTML = "";
  modal.advanced.classList.toggle("hidden", !tagsOn() && !promptHotkeysOn() && !histOn);
  modal.advancedBody.classList.add("hidden");
  modal.advancedHead.setAttribute("aria-expanded", "false");
  modal.advanced.classList.remove("open");
  modal.root.classList.remove("history-open");
  modal.name.placeholder = filePath
    ? t(kind === "video" ? "videoNamePh" : kind ? "imageNamePh" : "fileNamePh")
    : copyImage ? t("imageNamePh") : t("namePh");
  modal.imgWrap.classList.toggle("hidden", !hasPreview);
  modal.addIcon.classList.toggle("hidden", hasPreview);
  // "Replace media" always available next to "Remove media"; only the image
  // of a clipboard-image prompt cannot be removed (it IS the prompt).
  modal.removeImg.classList.toggle("hidden", copyImage);
  if (hasPreview) {
    const previewPath = iconKind ? iconPath : filePath;
    const isVideo = !image && mediaKind(previewPath) === "video";
    modal.img.classList.toggle("hidden", isVideo);
    modal.video.classList.toggle("hidden", !isVideo);
    if (isVideo) {
      modal.video.src = convertFileSrc(previewPath);
      modal.video.load();
      modal.video.play().catch(() => {}); // show a frame even if autoplay is held
    } else {
      // Prefer the re-encoded preview; fall back to the raw file, and if even
      // that fails to decode, ask the backend for a data URL once.
      const session = modalSession;
      modal.img.onerror = () => {
        modal.img.onerror = null;
        loadFilePreview(previewPath, modalState?.pdfPage || 0).then((d) => {
          if (d && session === modalSession) modal.img.src = d;
        });
      };
      modal.img.src = image || convertFileSrc(previewPath);
      modal.video.removeAttribute("src");
    }
    modal.showImage.classList.toggle("active", showImage);
    modal.showText.classList.toggle("active", !showImage);
  } else {
    modal.video.removeAttribute("src");
  }
  modal.fileWrap.classList.toggle("hidden", !filePath);
  const fileHint = $("modal-file-hint");
  fileHint.classList.toggle("hidden", !filePath);
  fileHint.textContent = t(optFlag("storeFiles") ? "fileHintStored" : "fileHint");
  // The two only actually need telling apart when both a stored preview and a
  // real attachment exist side by side -- otherwise there is nothing to confuse.
  const previewAttachHint = $("modal-preview-attach-hint");
  previewAttachHint.classList.toggle("hidden", !(hasPreview && filePath));
  const fileIconEl = $("modal-file-icon");
  fileIconEl.title = t("previewAttachHint");
  if (filePath) {
    const isPdf = PDF_EXT.test(filePath);
    $("modal-file-icon").innerHTML = isPdf ? ICON_PDF : ICON_FILE;
    modal.fileName.textContent = filePath.split(/[\\/]/).pop();
    modal.fileName.title = filePath;
    // Shown 1-based (page 1, 2, ...): pdfPage itself stays the 0-based index
    // pdf_preview expects, so the two never need converting anywhere else.
    modal.pdfPage.classList.toggle("hidden", !isPdf);
    if (isPdf) modal.pdfPage.value = String((modalState?.pdfPage || 0) + 1);
  } else {
    modal.pdfPage.classList.add("hidden");
  }
  renderAttachTextPreview(filePath);
}

// ---- Rotating prompts (editor) ----
// While active, modalState.rotateEntries holds EVERY entry (index 0 = what would
// otherwise be modal.text's own value): kept separate from modal.text so the two
// never fight over which one is "the" source of truth while the switch is on.
const rotationActive = () => (modalState?.rotateEntries || []).length > 0;
// rotationActive() alone answers "does this prompt have stored entries" (used
// by confirmModal to decide what to preserve on save); this answers "is the rotate
// list actually the thing shown right now": false while the feature itself is off,
// even with entries still stored. DISPLAY decisions (which surface to show/hide)
// must use this one, or a prompt edited while the feature is off gets its rotate
// list shown with no way back to the textarea (its own toggle row is hidden too).
const rotationUiOn = () => rotationActive() && flag("rotatingPrompts");

function renderRotateUi() {
  if (!modalState) return;
  // UI-gated, same reasoning as syncModalImageUi -- the list must stay hidden
  // (in favour of the plain textarea) while the feature itself is off, even with
  // entries still stored on the prompt.
  const active = rotationUiOn();
  modal.rotateSwitch.checked = active;
  modal.rotateList.classList.toggle("hidden", !active);
  if (!active) return;
  renderRotateEntries();
  modal.rotateMode.value = modalState.rotateMode === "random" ? "random" : "order";
  refreshRotateNext();
  renderRotateAutoBtn();
}

// Mirrors the backend's auto_eligible minus the two feature gates:
// rotatingPrompts already gates this whole row (rotationUiOn), and rotateAuto
// (the feature switch, 93) is checked here so the button disables live if it
// gets flipped off with the editor open.
function rotateAutoEligibleHere() {
  const filled = (modalState.rotateEntries || []).filter((e) => (e || "").trim()).length;
  return flag("rotateAuto") && !modalState.image && !modalState.filePath && filled >= 2;
}

function renderRotateAutoBtn() {
  const eligible = rotateAutoEligibleHere();
  modal.rotateAuto.disabled = !eligible;
  const on = eligible && !!modalState.rotateAuto;
  modal.rotateAuto.classList.toggle("active", on);
  modal.rotateAuto.setAttribute("aria-pressed", String(on));
  const hasVars = (modalState.rotateEntries || []).some((entry) => extractVars(entry).length);
  modal.rotateAuto.title = !eligible ? t("rotateAutoUnavailable") : hasVars ? t("rotateAutoVarsHint") : t("rotateAutoTip");
}

function renderRotateEntries() {
  const list = modalState.rotateEntries || [];
  const max = Math.max(2, Math.round(val("rotateMaxEntries")) || 20);
  modal.rotateEntries.innerHTML = "";
  list.forEach((entry, i) => {
    const row = document.createElement("div");
    row.className = "rotate-entry-row";
    const num = document.createElement("span");
    num.className = "rotate-entry-num";
    num.textContent = String(i + 1);
    const ta = document.createElement("textarea");
    ta.className = "modal-input rotate-entry-input";
    ta.rows = 1;
    ta.value = entry;
    // Typing can flip an entry between blank/filled (eligibility) or add/remove
    // a placeholder (the button's hint tooltip): both must stay live, not just on open.
    ta.addEventListener("input", () => { list[i] = ta.value; autoGrow(ta); renderRotateAutoBtn(); });
    const btns = document.createElement("div");
    btns.className = "rotate-entry-btns";
    const up = document.createElement("button");
    up.type = "button";
    up.className = "icon-btn rotate-entry-move";
    up.title = t("rotateMoveUp");
    up.setAttribute("aria-label", t("rotateMoveUp"));
    putIcon(up, ICON_UP);
    up.disabled = i === 0;
    up.addEventListener("click", () => { [list[i - 1], list[i]] = [list[i], list[i - 1]]; renderRotateEntries(); renderRotateAutoBtn(); });
    const down = document.createElement("button");
    down.type = "button";
    down.className = "icon-btn rotate-entry-move";
    down.title = t("rotateMoveDown");
    down.setAttribute("aria-label", t("rotateMoveDown"));
    putIcon(down, ICON_DOWN);
    down.disabled = i === list.length - 1;
    down.addEventListener("click", () => { [list[i + 1], list[i]] = [list[i], list[i + 1]]; renderRotateEntries(); renderRotateAutoBtn(); });
    const del = document.createElement("button");
    del.type = "button";
    del.className = "icon-btn rotate-entry-del";
    del.title = t("delete");
    del.setAttribute("aria-label", t("delete"));
    putIcon(del, ICON_TRASH);
    del.disabled = list.length <= 1;
    del.addEventListener("click", () => { list.splice(i, 1); renderRotateEntries(); renderRotateAutoBtn(); });
    btns.append(up, down, del);
    row.append(num, ta, btns);
    modal.rotateEntries.appendChild(row);
    requestAnimationFrame(() => autoGrow(ta));
  });
  modal.rotateAdd.disabled = list.length >= max;
}

// "Next: n/total" + "From the start" only mean anything for an ALREADY SAVED
// rotating prompt: a brand-new or not-yet-multi-entry one has no backend pointer
// to read yet.
async function refreshRotateNext() {
  const list = modalState.rotateEntries || [];
  const showPeek = modalState.mode === "edit" && !!modalState.id && list.length > 1;
  modal.rotateNext.classList.toggle("hidden", !showPeek);
  modal.rotateReset.classList.toggle("hidden", !showPeek);
  if (!showPeek) return;
  const id = modalState.id; // snapshot before the await, re-check after
  try {
    const peek = await invoke("rotation_peek", { id });
    if (peek && modalState.id === id) {
      modal.rotateNext.textContent = t("rotateNextLabel")
        .replace("{n}", String(peek.pos)).replace("{total}", String(peek.total));
    }
  } catch (_) { /* the peek is a convenience readout, never worth surfacing an error for */ }
}

// One native dialog at a time: ignore further requests until it is closed
// (the dialog is also window-modal on the backend side).
let dialogBusy = false;
async function withDialog(fn) {
  if (dialogBusy) return null;
  dialogBusy = true;
  try {
    return await fn();
  } finally {
    dialogBusy = false;
  }
}

// Page defaults to the first (0). The backend clamps against the real page
// count, so a stale number from a previously longer PDF at the same path still
// renders something instead of nothing.
async function loadFilePreview(path, page = 0) {
  if (PDF_EXT.test(path)) {
    return (await invoke("pdf_preview", { path, page }).catch(() => "")) || "";
  }
  if (!IMAGE_EXT.test(path)) return "";
  return (await invoke("load_image_file", { path }).catch(() => "")) || "";
}

// The four text attachment kinds a plain-text preview is offered for,
// alongside the image/PDF preview above. Matches TEXT_PREVIEW_EXT in lib.rs
// -- one list of kinds, kept in agreement on both sides by hand since the two
// cannot share a source at build time.
const TEXT_PREVIEW_EXT = /\.(txt|md|json|csv)$/i;

// Fills the attachment's plain-text preview box, or hides it. `textContent`
// only, never innerHTML -- the four formats this covers (txt/md/json/csv) are
// shown as literal text, not rendered as markup, even a .md file with real
// Markdown syntax in it.
async function renderAttachTextPreview(path) {
  const box = modal.textAttachPreview;
  if (!box) return;
  const eligible = !!path && flag("attachTextPreview") && TEXT_PREVIEW_EXT.test(path);
  box.classList.toggle("hidden", !eligible);
  if (!eligible) return;
  const session = modalSession;
  modal.textAttachContent.textContent = "";
  modal.textAttachTruncated.classList.add("hidden");
  modal.textAttachUnreadable.classList.add("hidden");
  let result;
  try {
    result = await invoke("attachment_text_preview", { path, maxKb: Math.round(val("attachPreviewKb")) });
  } catch (_) {
    return; // not a text-preview extension after all, or the file vanished mid-read
  }
  if (session !== modalSession || modalState?.filePath !== path) return; // a later modal already took over
  if (!result || !result.readable) {
    modal.textAttachUnreadable.classList.remove("hidden");
    return;
  }
  modal.textAttachContent.textContent = result.text;
  modal.textAttachTruncated.classList.toggle("hidden", !result.truncated);
}

// Build a file-attach prompt from a path. Image, gif, video and PDF
// attachments behave like media prompts; a PDF shows its rendered first page
// but still copies the file on click.
async function openFileCreate(path) {
  const kind = mediaKind(path);
  const isPdf = PDF_EXT.test(path);
  const image = await loadFilePreview(path);
  openModal({
    mode: "file-create",
    title: t(kind === "video" ? "videoModalTitle" : kind || isPdf ? "imageModalTitle" : "fileModalTitle"),
    name: path.split(/[\\/]/).pop(),
    filePath: path,
    image,
    showImage: !!kind || (isPdf && !!image),
  });
}

// Left click: open the file dialog directly.
async function startFileCreate() {
  await withDialog(async () => {
    const path = await invoke("pick_file_path");
    if (path) await openFileCreate(path);
  });
}

// Right click: take a file from the clipboard (not text); a copied file, or a
// copied image (screenshot) kept as a new file.
async function startFileFromClipboard() {
  await withDialog(async () => {
    const path = await invoke("get_clipboard_file_path").catch(() => "");
    if (path) { await openFileCreate(path); return; }
    const clipImg = await invoke("get_clipboard_image").catch(() => "");
    if (clipImg) {
      openModal({ mode: "image-create", title: t("imageModalTitle"), name: t("filterImage"), image: clipImg, showImage: true, copyImage: true });
      return;
    }
    toast(t("noClipboardFile"));
  });
}
function closeModal() {
  if (voiceState !== "idle" && voiceTargetEl === modal.text) stopVoice(); // its field is about to disappear
  closeColorPop();
  closeTagSuggest();
  closeTagPop();
  modal.root.classList.add("hidden");
  modal.video.removeAttribute("src"); // stop a playing preview
  // An orderly close -- save or deliberate cancel/discard, every caller funnels
  // through here -- drops the autosave draft right away rather than leaving it
  // for the boot-time recovery check to find and offer back for nothing.
  disarmDraftAutosave();
  if (modalState) invoke("discard_editor_draft").catch(() => {});
  assignModalState(null);
  modalInitial = "";
  // A tile the editor was opened from can be gone by the time it closes (the
  // prompt got deleted mid-edit, or a re-render replaced the DOM node) --
  // focusing a detached element is a harmless no-op, never an error.
  modalOpener?.focus?.();
  modalOpener = null;
}

// Ask before dismissing the modal via the background. A new save dialog (text,
// image, file, screenshot) always holds unsaved work; an edit only when changed.
async function confirmDiscardIfDirty() {
  if (!flag("confirmDiscard")) return true;
  const isNew = modalState && modalState.mode !== "edit";
  if (!isNew && modalSnapshot() === modalInitial) return true;
  return confirmDialog({
    title: t("discardTitle"),
    message: t("discardMsg"),
    confirmLabel: t("discardConfirm"),
    cancelLabel: t("keepEditing"),
  });
}

// Same guard as the X button/backdrop/Escape, for an action (drag-drop) that can
// fire with no editor open at all -- confirmDiscardIfDirty relies on modalState
// being set, so it is only asked when the modal is actually visible.
function confirmDiscardOrProceed(run) {
  if (modal.root.classList.contains("hidden")) {
    run();
    return;
  }
  confirmDiscardIfDirty().then((ok) => { if (ok) run(); });
}

function startCreate() {
  const text = inputEl.value.trim();
  if (!text) return;
  // Carry the composed text into the dialog so it can be reviewed/edited (and
  // previewed as Markdown) before saving.
  openModal({ mode: "create", text, title: t("nameModalTitle") });
}

async function confirmModal() {
  if (!modalState || modalSaving) return;
  // Saving ends a dictation session targeting this dialog's text field --
  // one of the five stop paths, so the recognizer never keeps listening into a
  // field that just closed.
  if (voiceState !== "idle" && voiceTargetEl === modal.text) stopVoice();
  const name = modal.name.value.trim();
  if (!name) { modal.name.focus(); return; }
  // While rotating, the real text lives in rotateEntries, not modal.text.
  // rotationActive() (raw): with the feature off but the prompt still holding
  // stored entries, they must still be preserved on save, just sourced
  // from wherever each one actually lives right now: entry 0 from the visible,
  // editable textarea (rotationUiOn() is false, so THAT is the surface the user just
  // edited), entries 1..n untouched from modalState (never shown, never editable
  // while the feature is off).
  const rotating = rotationUiOn();
  const hasVariants = rotationActive();
  const rawTexts = rotating
    ? modalState.rotateEntries
    : hasVariants
      ? [modal.text.value, ...modalState.rotateEntries.slice(1)]
      : [modal.text.value];
  // A {#{ }#} with no name inside never becomes a fillable variable: warn now,
  // while it is still one click away from being fixed, instead of after it is saved.
  if (rawTexts.some(hasEmptyVarToken)) {
    const proceed = await confirmDialog({
      title: t("emptyVarTitle"),
      message: t("emptyVarBody"),
      confirmLabel: t("emptyVarSaveAnyway"),
      danger: false,
    });
    if (!proceed) { if (!rotating) focusModalText(); return; }
  }
  // "empty entries are dropped on save" applies to entry 1 too: the first
  // surviving non-blank entry becomes the prompt's own text.
  const rotateFilled = (rotating || hasVariants) ? rawTexts.map((s) => s.trim()).filter(Boolean) : [];
  const variants = rotateFilled.slice(1);
  const rotateMode = variants.length ? (modalState.rotateMode === "random" ? "random" : "") : "";
  const rotateAuto = variants.length ? !!modalState.rotateAuto : false;

  const color = modalState.color || "";
  const image = modalState.image || "";
  const filePath = modalState.filePath || "";
  const iconPath = modalState.iconPath || "";
  const caption = modal.caption.value.trim();
  const captionSize = Number(modal.captionSize.value) || 0;
  const font = modal.fontSel.value;
  const fontSize = Number(modal.sizeSel.value) || 0;
  const favorite = modal.fav.classList.contains("active");
  // Sent even while the feature is off, so switching it off never drops stored
  // tags or notes.
  const tags = modalState.tags || [];
  const notes = modal.notesText.value;
  const hotkey = modalState.hotkey || "";
  const hotkeyPaste = hotkey ? (modalState.hotkeyPaste || "") : "";
  const pdfPage = modalState.pdfPage || 0;
  // Double-click on an empty cell opened this dialog for that exact spot: the
  // backend's own first-free-cell placement would otherwise ignore it.
  const targetCell = modalState.targetCell;
  // NOTE: Tauri expects camelCase keys for snake_case Rust args.
  // Path media (gif/video icon or attachment) shows without a stored image.
  const showImage =
    image || mediaKind(filePath) || mediaKind(iconPath) ? modalState.showImage : false;
  const copyImage = image ? modalState.copyImage : false;
  let saved = null;
  modalSaving = true;
  modal.confirm.disabled = true;
  try {
    if (modalState.mode === "create") {
      const text = (rotating || hasVariants) ? (rotateFilled[0] || "") : modal.text.value.trim();
      // Nothing to save yet: keep the dialog (and everything else typed into it)
      // and point at the missing text, like a missing name does.
      if (!text) { if (!rotating) focusModalText(); return; }
      saved = await invoke("add_prompt", { name, text, color, image, showImage, copyImage, filePath, pdfPage, iconPath, caption, captionSize, font, fontSize, favorite, tags, notes, hotkey, hotkeyPaste, variants, rotateMode, rotateAuto });
      inputEl.value = "";
      autoGrow(inputEl);
      saveBtn.disabled = true;
    } else if (modalState.mode === "image-create" || modalState.mode === "file-create") {
      // The name doubles as the copy text when shown as text.
      saved = await invoke("add_prompt", { name, text: name, color, image, showImage, copyImage, filePath, pdfPage, iconPath, caption, captionSize, font, fontSize, favorite, tags, notes, hotkey, hotkeyPaste, variants: [], rotateMode: "", rotateAuto: false });
    } else {
      const text = copyImage || filePath ? name : ((rotating || hasVariants) ? (rotateFilled[0] || "") : modal.text.value);
      const args = { id: modalState.id, name, text, color, showImage, copyImage, filePath, pdfPage, iconPath, caption, captionSize, font, fontSize, favorite, tags, notes, hotkey, hotkeyPaste, variants, rotateMode, rotateAuto };
      // The image field only travels when it actually changed: an untouched
      // picture is left to the backend, and "" only ever means the remove button.
      if (image !== (modalState.imageOpened || "")) args.image = image;
      saved = await invoke("update_prompt", args);
      // Save can turn rotateAuto off (or drop it below two entries) while a
      // session for THIS prompt happens to be armed: end it now instead of
      // waiting for the next paste to notice, same as the tile-menu toggle.
      // A no-op on the backend if no session is running for this id.
      if (!rotateAuto) await invoke("rotation_end_auto", { id: modalState.id }).catch(() => {});
    }
  } catch (err) {
    const errStr = String(err);
    // save-failed has no hotkey involved - route it through the same friendly
    // message the persist-failed event uses, instead of hotkeyMessage's fallback
    // of showing the raw error code verbatim.
    toast(errStr === "save-failed" ? t("persistFailed") : hotkeyMessage(errStr)); // keep the modal open so nothing typed is lost
    // A binding the backend refused was never stored, so the field must not keep
    // showing it: drop it and name the reason where it was captured.
    if (HOTKEY_ERRORS[errStr] && hotkey) {
      setPromptHotkey("");
      showHotkeyError(hotkeyMessage(errStr));
    }
    return;
  } finally {
    modalSaving = false;
    modal.confirm.disabled = false;
  }
  // The backend hands back what it really stored. An empty hotkey where one
  // was sent means the OS refused the combination between the check and the save.
  if (hotkey && saved && !saved.hotkey) toast(t("promptHotkeyFail"));
  // The same near-duplicate check the import path already runs, now also
  // after creating a prompt by hand: before closeModal() clears modalState.
  const wasCreate = modalState.mode === "create" || modalState.mode === "image-create" || modalState.mode === "file-create";
  const wasEdit = modalState.mode === "edit";
  // An edit can change entry count/text: drop the cached rotation state so the
  // grid re-render below primes it fresh instead of keeping a stale badge/tooltip
  // (primeRotateState only ever peeks once per id otherwise).
  if (saved) rotateState.delete(saved.id);
  closeModal();
  await renderGrid();
  if (saved && targetCell) await placeTile(saved.id, targetCell[0], targetCell[1]);
  pollMissingFiles(); // a replaced file clears the error immediately
  if (!libraryEl.classList.contains("hidden")) {
    // An edit MAY have just changed this prompt's "last edited" rank: the
    // library stayed open underneath the modal, so the usual refresh-on-open
    // (library-btn click) never fired for it.
    if (wasEdit && flag("promptHistory")) await refreshLastEdited();
    renderLibrary();
  }
  if (saved && wasCreate) maybeDupImportToast();
  // Runs for a create AND an edit, unlike maybeDupImportToast just above --
  // fired here, after everything the save itself needed is already done, and
  // never awaited, so a slow scan can only ever delay the hint toast.
  if (saved) maybeShowSimilarHint(saved.id);
}

// Everything about a prompt except what identifies it and the hotkey (one accelerator,
// one prompt): opens the normal create dialog prefilled, so the user can review or
// cancel before anything is saved. Placement then follows the regular create path.
// A stored prompt as the editor's fields. Both "edit" and "duplicate" go through
// here so a newly added prompt field can never reach one and be forgotten in the
// other: each caller then overrides only what genuinely differs between them.
function promptToModal(p) {
  const kind = p.file_path ? mediaKind(p.file_path) : "";
  return {
    name: p.name,
    text: p.text,
    color: p.color || "",
    image: p.image || "",
    showImage: p.show_image || false,
    copyImage: p.copy_image || false,
    filePath: p.file_path || "",
    pdfPage: p.pdf_page || 0,
    iconPath: p.icon_path || "",
    caption: p.caption || "",
    captionSize: p.caption_size || 0,
    font: p.font || "",
    fontSize: p.font_size || 0,
    favorite: p.favorite || false,
    tags: p.tags || [],
    notes: p.notes || "",
    hotkey: p.hotkey || "",
    hotkeyPaste: p.hotkey_paste || "",
    variants: p.variants || [],
    rotateMode: p.rotate_mode || "",
    rotateAuto: p.rotate_auto || false,
    // get_prompt already refuses a locked confidential entry outright, so by
    // the time this runs `secret` (if true) always means "decrypted, editable
    // text below" -- never leftover ciphertext.
    secret: p.secret || false,
    title: p.file_path
      ? t(kind === "video" ? "videoEditTitle" : kind ? "imageEditTitle" : "fileEditTitle")
      : p.copy_image ? t("imageEditTitle") : t("editModalTitle"),
  };
}

async function duplicatePrompt(id) {
  let p;
  try {
    p = await invoke("get_prompt", { id });
  } catch (err) {
    if (String(err) === "secret-locked" && (await ensureSecretVaultUnlocked())) {
      p = await invoke("get_prompt", { id }).catch((e2) => { errToast(e2); return null; });
    } else {
      if (String(err) !== "secret-locked") errToast(err);
      return;
    }
  }
  if (!p) return;
  // A copy starts unstarred, unbound and not confidential: one accelerator
  // belongs to one prompt, and turning the copy confidential again is its own
  // deliberate action, not something a duplicate inherits.
  openModal({
    ...promptToModal(p),
    mode: "create",
    name: `${p.name} (${t("duplicateSuffix")})`,
    favorite: false,
    secret: false,
    hotkey: "",
    hotkeyPaste: "",
  });
}

async function editPrompt(id) {
  let p;
  try {
    p = await invoke("get_prompt", { id });
  } catch (err) {
    // A confidential entry the vault has no key for right now -- ask for it,
    // exactly the same first-access moment a copy would ask for it, then
    // retry the same fetch once instead of just refusing.
    if (String(err) === "secret-locked" && (await ensureSecretVaultUnlocked())) {
      p = await invoke("get_prompt", { id }).catch((e2) => { errToast(e2); return null; });
    } else {
      if (String(err) !== "secret-locked") errToast(err);
      return;
    }
  }
  if (p) {
    openModal({ ...promptToModal(p), mode: "edit", id });
  }
}

wireTags();

wireTheme();

// ---- Copy history & usage journal ----

// ---- Search everything ----
// One overlay, opened over the main window, that searches six sources at once:
// the prompt grid, the copy history, file attachments, views, settings (both
// the expert menu and the normal settings directory already built for the
// expert search) and the in-app help. It deliberately does NOT replace any of
// those sources' own search field -- it sits above them. Visually it borrows
// the quick launcher's card/row look, but it is a plain overlay inside this
// window, not a second webview: everything that makes settings searchable
// (EXPERT_TABS, SETTINGS_DIRECTORY, i18nHit, gateOpen) already lives in this
// script's own state, and a second window would only reach it through a
// second copy of that state or a risky expert-menu rebuild.
let gsOpen = false;
let gsQuery = "";
let gsActive = -1; // index into gsFlatRows, the flat list a rendered pass produced
let gsExpanded = new Set(); // group keys currently past their "show more"
let gsFlatRows = []; // [{ el, run }], in on-screen order, across every group
let gsHistoryLog = []; // recent_copies, refetched each time the overlay opens

// Copies a prompt the same way the copy history's own "copy again" does --
// cooldown guard, variable/rotation resolution through copyResolved, usage
// recorded, a toast shown. Deliberately not libraryCopy(): that one also reads
// the library's own closeAfterCopy/auto-paste-double-click state, neither of
// which applies to a result picked from this overlay.
async function gsCopyPrompt(p) {
  if (copyOnCooldown(p.id)) return;
  if (!(await copyResolved(p))) return;
  recordCopy(p.id);
  toastCopied(p);
}

// Reuses the library's own scoring (computeLibSearchScore), not a second one --
// the spec is explicit about that. computeLibSearchScore reads the module-level
// libQuery instead of taking a query argument, so it is swapped in and restored
// around one synchronous pass; nothing async runs in between, so the real
// library search (if a query is mid-flight there too) never sees the swap.
function gsPromptHits(q) {
  const saved = libQuery;
  assignLibQuery(q);
  let scored;
  try {
    scored = prompts
      .map((p) => ({ p, r: computeLibSearchScore(p) }))
      .filter((x) => x.r.score > 0)
      .sort((a, b) => b.r.score - a.r.score);
  } finally {
    assignLibQuery(saved);
  }
  return scored.map(({ p }) => ({
    label: p.name || "",
    sub: (p.text || "").replace(/\s+/g, " ").trim().slice(0, 80),
    color: p.color,
    run: () => gsCopyPrompt(p),
  }));
}

// The copy history has no text of its own to search -- each entry is just an id
// and a timestamp -- so a hit here is "one of the recently-copied prompts whose
// name/body/tags/notes match", scored through the very same pass as the prompt
// group above. One entry per prompt (a prompt copied five times shows once).
function gsHistoryHits(q) {
  const byId = new Map(prompts.map((p) => [p.id, p]));
  const saved = libQuery;
  assignLibQuery(q);
  let scored;
  try {
    const seen = new Set();
    scored = [];
    for (const e of gsHistoryLog) {
      const p = byId.get(e.id);
      if (!p || seen.has(p.id)) continue;
      const r = computeLibSearchScore(p);
      if (r.score > 0) { scored.push({ p, r }); seen.add(p.id); }
    }
    scored.sort((a, b) => b.r.score - a.r.score);
  } finally {
    assignLibQuery(saved);
  }
  return scored.map(({ p }) => ({
    label: p.name || "",
    sub: (p.text || "").replace(/\s+/g, " ").trim().slice(0, 80),
    color: p.color,
    run: () => gsCopyPrompt(p),
  }));
}

// A prompt's attached file (p.file_path) is the only per-prompt "attachment" the
// data model has -- matched on the full path so a hit works whether the query is
// the file name or a folder it lives under. "Open" reuses the same edit dialog a
// double-click on the tile opens (it already renders the attachment preview);
// there is no separate viewer to open it in.
function gsAttachmentHits(q) {
  const ql = q.toLowerCase();
  return prompts
    .filter((p) => p.file_path && p.file_path.toLowerCase().includes(ql))
    .map((p) => ({
      label: p.name || p.file_path.split(/[\\/]/).pop(),
      sub: p.file_path,
      color: p.color,
      run: () => editPrompt(p.id),
    }));
}

function gsViewHits(q) {
  const ql = q.toLowerCase();
  return (settings.views || [])
    .filter((v) => (v.name || "").toLowerCase().includes(ql))
    .map((v) => ({ label: v.name || "", sub: "", color: v.color, run: () => switchToView(v.id) }));
}

// Walks both settings surfaces this app already has: SETTINGS_DIRECTORY (the
// normal page, toolbar, library dialog, backup manager) and every EXPERT_TABS
// row, filtered the same way the expert menu's own search (renderExpert())
// already does. No third copy of "what is a setting" is introduced here.
function gsSettingsHits(q) {
  const out = [];
  for (const entry of SETTINGS_DIRECTORY) {
    if (settingsDirectoryHit(entry, q)) {
      out.push({ label: t(entry.label), sub: t(SETTINGS_AREA_LABEL[entry.area]), run: () => openSettingsDirectoryEntry(entry) });
    }
  }
  for (const tab of EXPERT_TABS) {
    for (const group of tab.groups) {
      for (const f of groupFlags(group)) {
        if (!gateOpen(f.gate)) continue;
        const labelKey = flagLabelKey(f.key);
        if (settingHit(f.key, labelKey, q)) out.push({ label: t(labelKey), sub: t(tab.title), run: () => jumpToExpertSetting(f.key, "flags") });
      }
      for (const kind of ["values", "selects", "dropdowns", "paths"]) {
        for (const key of group[kind] || []) {
          if (!gateOpen(EXPERT_DEFS[kind]?.[key]?.gate)) continue;
          const labelKey = EXPERT_LABEL_KEYS[kind](key);
          if (settingHit(key, labelKey, q)) out.push({ label: t(labelKey), sub: t(tab.title), run: () => jumpToExpertSetting(key, kind) });
        }
      }
    }
  }
  return out;
}

// The in-app help is EN/DE only (ui/help.js) -- helpCategories() already falls
// back to English for the other nine, same as the help dialog's own search.
function gsHelpHits(q) {
  const ql = helpNorm(q);
  const out = [];
  for (const cat of helpCategories()) {
    const hay = helpNorm([cat.title, cat.whatItIs, (cat.howTo || []).join(" "), cat.whatHappens || "", (cat.settings || []).join(" ")].join(" "));
    if (hay.includes(ql)) out.push({ label: cat.title, sub: helpSectionLabel(cat.section), run: () => gsOpenHelp(cat) });
  }
  return out;
}

// Opens the help dialog pre-filtered to the matched article, through the same
// search field/renderHelp() the dialog's own search box uses -- not a second
// "scroll to section" mechanism.
function gsOpenHelp(cat) {
  assignHelpQuery(cat.title);
  $("help-search").value = cat.title;
  assignHelpCollapsed(new Set());
  renderHelp();
  $("help").classList.remove("hidden");
}

const GS_GROUPS = [
  { key: "prompts", headerKey: "globalSearchGroupPrompts", hits: gsPromptHits },
  { key: "history", headerKey: "globalSearchGroupHistory", hits: gsHistoryHits },
  { key: "attachments", headerKey: "globalSearchGroupAttachments", hits: gsAttachmentHits },
  { key: "views", headerKey: "globalSearchGroupViews", hits: gsViewHits },
  { key: "settings", headerKey: "globalSearchGroupSettings", hits: gsSettingsHits },
  { key: "help", headerKey: "globalSearchGroupHelp", hits: gsHelpHits },
];

// `groupKey` (e.g. "prompts", "settings") stamps a stable, language-independent
// marker on the row -- tests/ui/smoke_test.py finds a specific source group by
// this attribute instead of the translated heading text.
function gsGroupHead(headerKey, groupKey, clearAll) {
  const head = document.createElement("div");
  head.className = "gs-group-head";
  const label = document.createElement("span");
  label.textContent = t(headerKey);
  head.appendChild(label);
  if (groupKey) head.dataset.group = groupKey;
  if (clearAll) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "gs-clear-all";
    btn.textContent = t("gsRecentClearAll");
    btn.addEventListener("mousedown", (e) => e.preventDefault());
    btn.addEventListener("click", (e) => { e.stopPropagation(); gsClearRecent(); });
    head.appendChild(btn);
  }
  return head;
}

function gsRow(label, sub, color, onActivate) {
  const row = document.createElement("div");
  row.className = "gs-row";
  if (color) row.style.setProperty("--row-color", color);
  const name = document.createElement("span");
  name.className = "gs-row-name";
  name.textContent = label;
  row.appendChild(name);
  if (sub) {
    const subEl = document.createElement("span");
    subEl.className = "gs-row-sub";
    subEl.textContent = sub;
    row.appendChild(subEl);
  }
  row.addEventListener("mousedown", (e) => e.preventDefault()); // keep focus in the search field
  row.addEventListener("click", onActivate);
  return row;
}

// A recent-search row: same look as gsRow plus a per-term delete button, since
// the recent list (unlike a live search hit) is something the user may want
// to prune one term at a time.
function gsRecentRow(term) {
  const row = document.createElement("div");
  row.className = "gs-row gs-recent-row";
  const name = document.createElement("span");
  name.className = "gs-row-name";
  name.textContent = term;
  row.appendChild(name);
  const del = document.createElement("button");
  del.type = "button";
  del.className = "search-suggest-del gs-recent-del";
  del.textContent = "×";
  del.title = t("delete");
  del.setAttribute("aria-label", t("delete"));
  del.addEventListener("mousedown", (e) => e.preventDefault());
  del.addEventListener("click", (e) => {
    e.stopPropagation();
    gsForgetRecent(term);
  });
  row.appendChild(del);
  row.addEventListener("mousedown", (e) => e.preventDefault());
  row.addEventListener("click", () => { $("gs-input").value = term; gsQuery = term; gsRender(); });
  return row;
}

function gsRecentList() {
  let list;
  try { list = JSON.parse(settings.ui_texts?.globalSearchRecent || "[]"); } catch { list = []; }
  return Array.isArray(list) ? list.slice(0, val("searchListMax")) : [];
}

function gsSaveRecent(list) {
  const value = JSON.stringify(list);
  settings.ui_texts = { ...(settings.ui_texts || {}), globalSearchRecent: value };
  invoke("set_ui_text", { key: "globalSearchRecent", value }).catch(() => {});
}

// Deleting a term keeps keyboard focus on a delete button at (or near) the
// same position so repeated Tab + Enter can clear several terms in a row.
function gsForgetRecent(term) {
  const list = gsRecentList();
  const idx = list.findIndex((x) => x.toLowerCase() === term.toLowerCase());
  gsSaveRecent(list.filter((x) => x.toLowerCase() !== term.toLowerCase()));
  gsRender();
  const dels = $("gs-results").querySelectorAll(".gs-recent-del");
  if (dels.length) dels[Math.min(idx, dels.length - 1)].focus();
  else $("gs-input").focus();
}

function gsClearRecent() {
  gsSaveRecent([]);
  gsRender();
  $("gs-input").focus();
}

// Remembers a query for the "recent searches" list shown at rest -- capped at
// searchListMax (8 unless changed), newest first, deduped case-insensitively.
function gsRemember(term) {
  term = term.trim();
  if (term.length < 2) return;
  let list = gsRecentList().filter((x) => x.toLowerCase() !== term.toLowerCase());
  list.unshift(term);
  gsSaveRecent(list.slice(0, val("searchListMax")));
}

function gsSetActive(i) {
  gsActive = i;
  gsFlatRows.forEach((r, idx) => r.el.classList.toggle("active", idx === i));
  gsFlatRows[i]?.el.scrollIntoView({ block: "nearest" });
}

// Picking a result closes the overlay BEFORE running its action -- every action
// (copy, switch view, open a dialog) wants the overlay itself out of the way
// first, same order the launcher uses for its own select().
function gsActivate(hit) {
  gsRemember(gsQuery);
  closeGlobalSearch();
  hit.run();
}

function gsRender() {
  const box = $("gs-results");
  const empty = $("gs-empty");
  box.innerHTML = "";
  gsFlatRows = [];
  const q = gsQuery.trim();
  if (q.length < 2) {
    const list = gsRecentList();
    if (!list.length) {
      empty.textContent = t("globalSearchHint");
      empty.classList.remove("hidden");
      gsSetActive(-1);
      return;
    }
    empty.classList.add("hidden");
    box.appendChild(gsGroupHead("globalSearchRecentLabel", "recent", true));
    for (const term of list) {
      const run = () => { $("gs-input").value = term; gsQuery = term; gsRender(); };
      const row = gsRecentRow(term);
      box.appendChild(row);
      gsFlatRows.push({ el: row, run });
    }
    gsSetActive(gsFlatRows.length ? 0 : -1);
    return;
  }
  const limit = val("searchListMax");
  let any = false;
  for (const group of GS_GROUPS) {
    const hits = group.hits(q);
    if (!hits.length) continue;
    any = true;
    box.appendChild(gsGroupHead(group.headerKey, group.key));
    const expanded = gsExpanded.has(group.key);
    const shown = expanded ? hits : hits.slice(0, limit);
    for (const hit of shown) {
      const row = gsRow(hit.label, hit.sub, hit.color, () => gsActivate(hit));
      box.appendChild(row);
      gsFlatRows.push({ el: row, run: () => gsActivate(hit) });
    }
    if (!expanded && hits.length > limit) {
      const run = () => { gsExpanded.add(group.key); gsRender(); };
      const row = gsRow(t("globalSearchMore").replace("{n}", String(hits.length - limit)), "", "", run);
      row.classList.add("gs-more-row");
      box.appendChild(row);
      gsFlatRows.push({ el: row, run });
    }
  }
  empty.classList.toggle("hidden", any);
  if (!any) empty.textContent = t("globalSearchNoResults");
  gsSetActive(gsFlatRows.length ? 0 : -1);
}
const gsRenderDebounced = debounced(() => gsRender(), () => val("searchDebounceMs"));

async function openGlobalSearch() {
  if (gsOpen) return;
  gsOpen = true;
  gsQuery = "";
  gsExpanded = new Set();
  $("gs-input").value = "";
  try { gsHistoryLog = await invoke("recent_copies", { limit: 500, grouped: false }); } catch { gsHistoryLog = []; }
  gsRender();
  $("global-search").classList.remove("hidden");
  $("gs-input").focus();
}

function closeGlobalSearch() {
  if (!gsOpen) return;
  gsOpen = false;
  $("global-search").classList.add("hidden");
}

function toggleGlobalSearch() {
  if (gsOpen) closeGlobalSearch();
  else openGlobalSearch();
}

// ---- Settings: views editor ----
// Views are managed straight from the top view tabs: the "+" adds one, a
// right-click opens the view popup (rename, grid size, color, delete). The old
// settings-page views editor was removed.

function bindEditor() {
  inputEl.addEventListener("input", () => {
    autoGrow(inputEl);
    saveBtn.disabled = !inputEl.value.trim();
  });
  inputEl.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.key === "Enter") { e.preventDefault(); startCreate(); }
    // Escape ends a dictation session targeting this field, instead of
    // falling through to whatever the global Escape handler would otherwise do.
    if (e.key === "Escape" && voiceState !== "idle" && voiceTargetEl === inputEl) {
      e.stopPropagation();
      stopVoice();
    }
  });
  // Paste an image or file (Ctrl+V) to save it as a button; plain text pastes
  // normally. A broken/unreadable image is reported, never saved.
  inputEl.addEventListener("paste", async (e) => {
    if (!flag("pasteMedia")) return;
    const dt = e.clipboardData;
    if (!dt) return;
    // Only intercept when the clipboard carries a file/image payload; plain text
    // keeps pasting normally.
    const hasPayload = dt.files?.length > 0 || [...(dt.items || [])].some((it) => it.kind === "file");
    if (!hasPayload) return;
    e.preventDefault();
    await withDialog(async () => {
      // Pasted bitmaps (screenshots) arrive as a file item with an empty type,
      // so try the image clipboard first, then fall back to a real file path.
      const img = await invoke("get_clipboard_image").catch(() => "");
      if (img) {
        openModal({ mode: "image-create", title: t("imageModalTitle"), name: t("filterImage"), image: img, showImage: true, copyImage: true });
        return;
      }
      const path = await invoke("get_clipboard_file_path").catch(() => "");
      if (path) await openFileCreate(path);
      else toast(t("imagePasteFailed"));
    });
  });
  saveBtn.addEventListener("click", startCreate);

  modal.confirm.addEventListener("click", confirmModal);
  modal.closeX.addEventListener("click", () => confirmDiscardIfDirty().then((ok) => { if (ok) closeModal(); }));
  modal.fav.addEventListener("click", () => {
    const on = modal.fav.classList.toggle("active");
    modal.fav.setAttribute("aria-pressed", String(on));
  });
  modal.secret.addEventListener("click", onModalSecretToggle);
  // Rotating prompts.
  modal.rotateSwitch.addEventListener("change", async () => {
    if (modal.rotateSwitch.checked) {
      modalState.rotateEntries = [modal.text.value];
      // Only a prompt that never rotated before takes the shipped/expert default:
      // re-checking the box mid-edit must not clobber a mode already chosen.
      if (!modalState.rotateMode) {
        modalState.rotateMode = EXPERT_DROPDOWNS.rotateDefaultMode.get() === "random" ? "random" : "";
      }
    } else {
      const hasExtra = (modalState.rotateEntries || []).length > 1;
      if (hasExtra) {
        const ok = await confirmDialog({
          title: t("rotateToggle"),
          message: t("rotateDiscardConfirm"),
          confirmLabel: t("discardConfirm"),
          danger: true,
        });
        if (!ok) { modal.rotateSwitch.checked = true; return; }
      }
      modal.text.value = (modalState.rotateEntries || [])[0] ?? modal.text.value;
      modalState.rotateEntries = [];
    }
    renderRotateUi();
    // The switch row only exists for a text prompt, so textVisible is true here.
    // Flipping it has to move the same three surfaces syncModalImageUi does: only
    // hiding the textarea left the clear-X wrapper (an empty 120px box) and, with
    // on, the rendered pane still showing entry 1 sitting above the colours.
    const rotating = rotationActive();
    modal.text.classList.toggle("hidden", rotating);
    modal.voiceRow.classList.toggle("hidden", rotating);
    const varsUi = !rotating && flag("promptVars");
    modal.textBar.classList.toggle("hidden", !varsUi);
    modal.varsHint.classList.toggle("hidden", !varsUi);
    if (varsUi) renderVarHelper();
    syncMdUi(true, rotating);
    if (!modal.rotateSwitch.checked) modal.text.focus();
    // This must hide/show the readout on the SAME tick as the toggle: the
    // rAF-deferred scheduleLenCounter() left it visible for one frame, long enough
    // for a synchronous check right after the toggle to see it.
    updateLenCounter();
  });
  modal.rotateAdd.addEventListener("click", () => {
    const max = Math.max(2, Math.round(val("rotateMaxEntries")) || 20);
    if ((modalState.rotateEntries || []).length >= max) return;
    modalState.rotateEntries.push("");
    renderRotateEntries();
    renderRotateAutoBtn();
    const rows = modal.rotateEntries.querySelectorAll(".rotate-entry-input");
    rows[rows.length - 1]?.focus();
  });
  modal.rotateMode.addEventListener("change", () => {
    if (!modalState) return;
    modalState.rotateMode = modal.rotateMode.value === "random" ? "random" : "";
  });
  modal.rotateAuto.addEventListener("click", () => {
    if (modal.rotateAuto.disabled) return;
    modalState.rotateAuto = !modalState.rotateAuto;
    renderRotateAutoBtn();
  });
  modal.rotateReset.addEventListener("click", async () => {
    if (!modalState?.id) return;
    try {
      await invoke("rotation_reset", { id: modalState.id });
    } catch (err) {
      errToast(err);
      return;
    }
    toast(t("rotateResetDone"));
    refreshRotateNext();
  });
  // Click the preview media → fullscreen zoom/pan viewer.
  modal.img.addEventListener("click", () => { if (modal.img.src) openLightbox(modal.img.src, false); });
  modal.video.addEventListener("click", () => { if (modal.video.src) openLightbox(modal.video.src, true); });
  modal.name.addEventListener("keydown", (e) => { if (e.key === "Enter") confirmModal(); });
  modal.text.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && voiceState !== "idle" && voiceTargetEl === modal.text) {
      e.stopPropagation();
      stopVoice();
    }
  });
  modal.text.addEventListener("input", scheduleLenCounter); // live length readout
  modal.text.addEventListener("input", scheduleMdPreview);  // live preview (rAF-throttled)
  // Replace layout: the rendered pane IS the resting state, so it has to hand the
  // textarea over on a click and take back over once the user is done typing.
  const mdSwap = (editing) => {
    const from = editing ? modal.mdPreview : modal.text;
    const span = from.scrollHeight - from.clientHeight;
    const ratio = span > 0 ? from.scrollTop / span : 0;
    mdEditing = editing;
    syncMdUi(true);
    const to = editing ? modal.text : modal.mdPreview;
    if (editing) to.focus(); // never behind a frame callback: see scheduleMdPreview
    // Only the scroll position waits for layout; losing it costs nothing.
    requestAnimationFrame(() => {
      const s = to.scrollHeight - to.clientHeight;
      to.scrollTop = s > 0 ? Math.round(ratio * s) : 0;
    });
  };
  modal.mdPreview.addEventListener("click", () => { if (flag("mdPreview")) mdSwap(true); });
  modal.mdPreview.addEventListener("focus", () => { if (flag("mdPreview")) mdSwap(true); });
  // Swapping back changes the height of the block, so doing it on the blur that a
  // mousedown produces moves whatever the user is halfway through clicking: the
  // click then lands somewhere else, or nowhere. Wait for the pointer to come up.
  let swapAfterClick = false;
  let pointerIsDown = false;
  window.addEventListener("pointerdown", () => { pointerIsDown = true; }, true);
  window.addEventListener("pointerup", () => {
    pointerIsDown = false;
    if (!swapAfterClick) return;
    swapAfterClick = false;
    if (mdEditing && document.activeElement !== modal.text) mdSwap(false);
  }, true);
  modal.text.addEventListener("blur", () => {
    if (!mdEditing) return;
    if (pointerIsDown) { swapAfterClick = true; return; }
    mdSwap(false);
  });
  // Variable helper collapsible (same convention as the notes row).
  modal.varsHead.addEventListener("click", () => {
    const open = !modal.varsBody.classList.toggle("hidden");
    modal.varsHead.setAttribute("aria-expanded", String(open));
    modal.textBar.classList.toggle("open", open);
  });
  // Capture: the field is read-only, the key press itself is the input.
  bindHotkeyField(modal.hotkeyInput, {
    id: () => modalState?.id || null,
    errEl: modal.hotkeyErr,
    showAllEl: $("modal-hotkey-showall"),
    current: () => modalState?.hotkey || "",
    onCommit: async (accel) => {
      const before = modalState?.id;
      try {
        await invoke("check_hotkey", { hotkey: accel, id: modalState?.id || null });
      } catch (err) {
        if (modalState?.id !== before) return null; // moved on: nothing to show for it
        throw err;
      }
      if (modalState?.id !== before) return null; // moved on between the check and here
      setPromptHotkey(accel);
      return accel;
    },
  });
  $("modal-hotkey-clear").addEventListener("click", () => setPromptHotkey(""));
  $("modal-hotkey-showall").addEventListener("click", () => openAllShortcuts());
  modal.hotkeyAction.addEventListener("change", () => {
    if (modalState) modalState.hotkeyPaste = modal.hotkeyAction.value;
  });
  // Tags/hotkey/history collapsible, combined into one "Advanced options".
  modal.advancedHead.addEventListener("click", () => {
    const open = !modal.advancedBody.classList.toggle("hidden");
    modal.advancedHead.setAttribute("aria-expanded", String(open));
    modal.advanced.classList.toggle("open", open);
    // In tall (text-edit) mode the textarea flex-grows and eats the height the open
    // history needs; history-open caps it (CSS) so the list + Save row stay reachable.
    // Only relevant when history itself applies to this prompt (mode/type/flag).
    const histApplies = !$("modal-history").classList.contains("hidden");
    modal.root.classList.toggle("history-open", open && histApplies);
    if (open) {
      if (histApplies) loadHistory(modalState?.id);
      // Scroll the modal's own scroll container to the bottom so the freshly opened
      // section (and the Save row below it) come fully into view on small windows.
      // scrollIntoView("nearest") only nudged the already-visible head, hence unreliable.
      const scroller = modal.advanced.closest(".modal");
      if (scroller) requestAnimationFrame(() => requestAnimationFrame(() => {
        scroller.scrollTo({ top: scroller.scrollHeight, behavior: "smooth" });
      }));
    }
  });

  // Image / Text display toggle (text mode shows the name on the tile).
  modal.showImage.addEventListener("click", () => {
    if (!modalState) return;
    modalState.showImage = true;
    syncModalImageUi(modalState.mode);
  });
  modal.showText.addEventListener("click", () => {
    if (!modalState) return;
    modalState.showImage = false;
    syncModalImageUi(modalState.mode);
  });
  // Editor's own "Copy formatted" button: same resolveVars() path every other
  // copy goes through, not the raw stored text.
  modal.copyFormattedBtn.addEventListener("click", async () => {
    const text = await resolveVars(modal.text.value);
    if (text === null) return; // the placeholder dialog was cancelled
    const html = renderMarkdown(text);
    const ok = await invoke("copy_formatted", { text, html }).catch((e) => { errToast(e); return false; });
    if (ok) toast(t("copied"));
  });
  // Shared media pick: image, gif or video. Stills are stored as a preview,
  // gif/video become the icon path (shown, never copied). An image keeps the
  // prompt's copy behaviour; a gif/video icon switches copying back to text.
  async function pickReplacementMedia() {
    const session = modalSession;
    const path = await withDialog(() => invoke("pick_file_path"));
    if (!path || !modalState || session !== modalSession) return false;
    const kind = mediaKind(path);
    if (!kind) {
      toast(t("unsupportedFile"));
      return false;
    }
    if (kind === "image") {
      const data = await loadFilePreview(path);
      if (!data || !modalState || session !== modalSession) return false;
      modalState.image = data;
      modalState.iconPath = "";
    } else {
      modalState.image = "";
      modalState.iconPath = path;
      modalState.copyImage = false;
    }
    modalState.showImage = true;
    return true;
  }
  // "Replace media" (preview exists) and "Media as icon" (no media yet).
  modal.replaceImg.addEventListener("click", async () => {
    if (modalState && (await pickReplacementMedia())) syncModalImageUi(modalState.mode);
  });
  modal.addIcon.addEventListener("click", async () => {
    if (modalState && (await pickReplacementMedia())) syncModalImageUi(modalState.mode);
  });
  modal.removeImg.addEventListener("click", () => {
    if (!modalState) return;
    modalState.image = "";
    modalState.iconPath = "";
    modalState.showImage = false;
    syncModalImageUi(modalState.mode);
  });

  // Replacing the attached file re-classifies the prompt by file type:
  // stills/gifs/videos switch to the media layout, anything else to plain file.
  modal.replaceFile.addEventListener("click", async () => {
    if (!modalState) return;
    const session = modalSession;
    const path = await withDialog(() => invoke("pick_file_path"));
    if (!path || !modalState || session !== modalSession) return;
    modalState.filePath = path;
    modalState.pdfPage = 0; // a different file starts back at its own first page
    const kind = mediaKind(path);
    const data = await loadFilePreview(path);
    if (!modalState || session !== modalSession) return;
    modalState.image = data;
    modalState.copyImage = false; // the file itself is what gets copied
    modalState.showImage = !!kind;
    syncModalImageUi(modalState.mode);
  });

  // Re-requests the preview at the chosen page and saves it into pdf_page:
  // the field only ever shows for a PDF attachment (syncModalImageUi).
  // Request token -- a fast second page change must not let the slower
  // older reply land after the newer one and overwrite its already-fresher preview.
  let pdfPageReq = 0;
  modal.pdfPage.addEventListener("change", async () => {
    if (!modalState) return;
    const session = modalSession;
    const page = Math.max(0, (Number(modal.pdfPage.value) || 1) - 1);
    modalState.pdfPage = page;
    const req = ++pdfPageReq;
    const data = await loadFilePreview(modalState.filePath, page);
    if (req !== pdfPageReq || !modalState || session !== modalSession) return;
    if (data) { modalState.image = data; modal.img.src = data; }
  });

  // Delete from the edit dialog: goes to the trash at once, Undo brings it back.
  modal.delete.addEventListener("click", async () => {
    if (!modalState || modalState.mode !== "edit") return;
    const id = modalState.id;
    const typedName = modal.name ? modal.name.value : "";
    if (!(await trashPromptWithUndo(id, typedName))) return; // keep the editor open so the user can retry
    closeModal();
    await renderGrid();
    if (!libraryEl.classList.contains("hidden")) renderLibrary();
  });
  modal.root.addEventListener("pointerdown", async (e) => {
    if (e.target !== modal.root) return;
    if (await confirmDiscardIfDirty()) closeModal();
  });

  // Any window's copy of a rotating prompt lands here: refresh every tile of
  // that id (usually one, but "prompt in multiple views" can place several).
  listen("rotation-changed", (e) => {
    const { id, index, pos, total, text } = e.payload || {};
    if (!id) return;
    // Invalidates any in-flight primeRotateState peek for this id, so its reply
    // can no longer overwrite the fresher position this push just delivered.
    rotateReqs.set(id, (rotateReqs.get(id) || 0) + 1);
    rotateState.set(id, { index, pos, total, text });
    document.querySelectorAll(`.tile[data-id="${CSS.escape(id)}"]`).forEach((tile) => applyRotateState(tile, id));
  });
  // Auto-advance session start/end: drives the badge's is-auto accent and,
  // on start only, a toast (no toast on end: that would just be noise).
  listen("rotation-auto", (e) => {
    const { id, active } = e.payload || {};
    if (!id) return;
    if (active) autoActiveIds.add(id); else autoActiveIds.delete(id);
    document.querySelectorAll(`.tile[data-id="${CSS.escape(id)}"]`).forEach((tile) => applyAutoBadge(tile, id));
    if (active && val("bubbleMs") > 0) toast(t("rotateAutoStarted"));
  });
  listen("rotation-auto-blocked", () => {
    toast(t("rotateAutoBlocked"));
  });

  attachSelectPicker(modal.sizeSel);
  attachSelectPicker(modal.captionSize);
  attachSelectPicker(modal.hotkeyAction);
  attachSelectPicker(modal.rotateMode);
  modal.caption.addEventListener("input", updateCaptionPreview);
  modal.captionSize.addEventListener("change", updateCaptionPreview);

  attachSelectPicker(modal.fontSel);
}

function bindCapture() {
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
        await invoke("show_rec_bar", { targetKind: kind, id: targetId });
      } catch (err) { errToast(err); }
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
          .replace("{free}", fmtBytes(diskWarn.free_mb * 1024 * 1024))
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
  async function openRecModal(rawPath, needsCrop) {
    pendingRecPath = rawPath;
    recSavedOrAttached = false;
    recCopiedNotSaved = false;
    recGen += 1;
    const gen = recGen;
    renderRecFolder();
    $("rec-modal-edit").classList.toggle("hidden", !flag("videoEditor"));
    recModal.classList.remove("hidden");
    // recAutoCrop off means the user chose to keep the border -- skip the whole
    // cropping step (and its status line) silently, same as a canvas that
    // never needed cropping in the first place.
    if (needsCrop && flag("recAutoCrop")) {
      recPreview.classList.add("hidden");
      showRecErrorDetails("");
      await runRecCrop(rawPath, gen);
      if (gen !== recGen) return;
    }
    const format = settings.ui_texts?.recFormat || "mp4";
    const lossless = (settings.ui_texts?.recQuality || "high") === "lossless";
    if (format === "mp4" && !lossless) {
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
      pendingRecPath = outPath;
      recStatus.classList.remove("rec-status-cancellable");
      recStatus.onclick = null;
      recStatus.textContent = "";
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
        const { message, details } = parseFfmpegError(err, t);
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
    if (count > 0) toast(t("recSweepDone").replace("{n}", String(count)).replace("{v}", fmtBytes(bytes)));
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
  const { snipEditOpen, snipEdit } = initSnipEditor({ get settings() { return settings; }, get COLORS() { return COLORS; }, EXPERT_DROPDOWNS, NUM_FIELD_BIG_STEP, buildSwatches, closeShortcutsModal, colorPop, errToast, flag, openColorPop, openShortcutsModal, recordToolUsage, t, toast, txt, val, withShortcut });

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
  listen("snip-captured", (e) => {
    pendingSnip = e.payload?.data_url || "";
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
      $("snip-modal-saveas-format").value = txt("screenshotFormat") || "png";
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
        $("snip-modal-saveas-format").value = txt("screenshotFormat") || "png";
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
        $("snip-modal-saveas-format").value = txt("screenshotFormat") || "png";
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
    upscaleUnderPx: Math.round(val("ocrUpscaleUnderPx")),
    joinLines: flag("ocrJoinLines"),
    lang: txt("ocrLang") || "auto",
    autoPickLang: flag("ocrAutoPickLang"),
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

function bindDictation() {
  // Voice dictation: the two buttons (composer, edit dialog) share one wiring
  // helper and one event pipeline -- see the section above for all of it.
  wireVoiceButton($("voice-btn"), () => inputEl);
  wireVoiceButton(modal.voiceBtn, () => modal.text);
  voiceWatchDiscard(inputEl);
  voiceWatchDiscard(modal.text);
  listen("voice-partial", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) return; // a late partial is ignored, not delivered
    resolveVoiceArming();
    insertVoiceText(payload.text || "", true);
  });
  listen("voice-text", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) { deliverLateVoiceText(payload); return; }
    resolveVoiceArming();
    const processed = applyVoiceCommands(payload.text || "", { punct: flag("voicePunct"), autoCaps: flag("voiceAutoCaps"), lang: voiceLangCode() });
    insertVoiceText(processed, false);
  });
  listen("voice-stopped", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) return;
    const wasHotkey = voiceTargetEl === voiceHotkeyBuffer;
    const finalText = wasHotkey ? voiceHotkeyBuffer.value : "";
    endVoiceUi();
    if (wasHotkey) deliverVoiceHotkeyText(finalText);
  });
  listen("voice-error", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) return;
    endVoiceUi();
    routeVoiceError(payload.code);
  });
  // The global hotkey toggled a session ON (a toggle OFF is handled entirely
  // on the Rust side, no event needed for that direction).
  listen("voice-hotkey-toggle", () => {
    if (voiceState !== "idle") return;
    voiceHotkeyBuffer.value = "";
    startVoice(voiceHotkeyBuffer, null);
  });
  // Management dialog chrome.
  $("ocr-langs-close-x").addEventListener("click", closeLangsModal);
  $("ocr-langs-modal").addEventListener("pointerdown", (e) => { if (e.target === $("ocr-langs-modal")) closeLangsModal(); });
  $("ocr-langs-recheck").addEventListener("click", () => loadLangCaps(langDialogKind, true));
}

function bindTour() {
  $("tour-skip").addEventListener("click", tourSkip);
  $("tour-back").addEventListener("click", () => tourGoto(tourStep - 1));
  $("tour-next").addEventListener("click", () => (tourStep === TOUR_LAST_STEP ? tourFinish() : tourGoto(tourStep + 1)));
}

function bindGlobalSearch() {
  // Search everything: toolbar icon, Ctrl+K, Escape/click-outside/icon to close.
  const gsEl = $("global-search");
  $("global-search-btn").addEventListener("click", toggleGlobalSearch);
  $("gs-close").addEventListener("click", closeGlobalSearch);
  gsEl.addEventListener("pointerdown", (e) => { if (e.target === gsEl) closeGlobalSearch(); });
  $("gs-input").addEventListener("input", (e) => { gsQuery = e.target.value; gsRenderDebounced(); });
  $("gs-input").addEventListener("keydown", (e) => {
    if (e.key === "Escape") { e.preventDefault(); closeGlobalSearch(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); if (gsFlatRows.length) gsSetActive(Math.min(gsActive + 1, gsFlatRows.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); if (gsFlatRows.length) gsSetActive(Math.max(gsActive - 1, 0)); }
    else if (e.key === "Enter") {
      e.preventDefault();
      const row = gsFlatRows[gsActive];
      if (row) row.run();
    }
  });
  document.addEventListener("keydown", (e) => {
    if (!flag("globalSearch")) return;
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "k") {
      e.preventDefault();
      toggleGlobalSearch();
    }
  });
}

function bindGridMenus({ openExternalFileInEditor }) {
  // Grid tag filter toolbar button.
  $("grid-tag-btn").addEventListener("click", () => {
    if (!gridTagPop.classList.contains("hidden")) { closeGridTagPop(); return; }
    openGridTagPop();
  });

  ctxEl.addEventListener("click", async (e) => {
    const btn = e.target.closest("button");
    const act = btn?.dataset.act;
    if (!act || !ctxId) return;
    const id = ctxId;
    if (act === "delete") {
      closeCtx();
      const ctxPrompt = prompts.find((x) => x.id === id);
      if (!(await trashPromptWithUndo(id, ctxPrompt?.name))) return;
      await renderGrid();
      return;
    }
    if (act === "views") {
      const rect = ctxEl.getBoundingClientRect();
      closeCtx();
      openViewsPop(id, rect);
      return;
    }
    if (act === "moveToFolder") {
      const rect = ctxEl.getBoundingClientRect();
      closeCtx();
      openMoveToFolderPop(id, rect);
      return;
    }
    closeCtx();
    if (act === "edit") {
      await editPrompt(id);
    } else if (act === "copyFormatted") {
      await copyFormattedPrompt(id);
    } else if (act === "editVideo") {
      const p = prompts.find((x) => x.id === id);
      if (p?.file_path) {
        await invoke("open_video_editor", { srcPath: p.file_path })
          .then(() => recordToolUsage("video_editor"))
          .catch((e) => errToast(e));
      }
    } else if (act === "editImage") {
      const p = prompts.find((x) => x.id === id);
      if (p?.file_path) await openExternalFileInEditor(p.file_path);
    } else if (act === "duplicate") {
      await duplicatePrompt(id);
    } else if (act === "hide") {
      // Remove from the active view's grid; stays available in the library.
      const view = activeView();
      const layout = { ...layoutOf(view) };
      delete layout[id];
      view.layouts[gridKeyOf(view)] = layout;
      invoke("set_layout", { layout }).catch((e) => errToast(e));
      await renderGrid(true);
    } else if (act === "pin") {
      // The command answers with the state it actually left the prompt in: take that
      // instead of assuming the click flipped the label that was just on screen.
      await invoke("toggle_floating", { id })
        .then((pinned) => cacheFloatingState(id, pinned))
        .catch((e) => errToast(e));
    } else if (act === "rotateAutoMenu") {
      await toggleRotateAutoDirect(id);
    } else if (act === "archive") {
      const p = prompts.find((x) => x.id === id);
      if (!p) return;
      const tag = await ensureArchiveTagName();
      const key = tagKey(tag);
      const already = (p.tags || []).some((x) => tagKey(x) === key);
      try {
        assignSettings(await invoke("batch_prompts", {
          ids: [id], action: already ? "tag-remove" : "tag-add",
          color: null, favorite: null, viewId: null, tag,
        }));
      } catch (e) { errToast(e); return; }
      p.tags = already ? (p.tags || []).filter((x) => tagKey(x) !== key) : [...(p.tags || []), tag];
      await renderGrid(true);
      if (!libraryEl.classList.contains("hidden")) renderLibrary();
    }
  });

  // Right-click anywhere in the header actions → toggle which top-bar tools show.
  document.querySelector(".header-actions")?.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    openToolbarMenu(e.clientX, e.clientY);
  });
}

function bindBackups() {
  $("backup-close").addEventListener("click", closeBackups);
  $("backup-modal").addEventListener("pointerdown", (e) => {
    if (e.target === $("backup-modal")) closeBackups();
  });
}

// ---- Wire events ----
function bind() {
  bindEditor();
  bindCopyFlow();
  const { openEditorPop, closeOpenEditorPop, openExternalFileInEditor, closeRecMorePop, closeRecModal, snipEdit } = bindCapture();
  bindDictation();
  bindSettingsPanel();
  bindExpertMenu();
  bindViews();
  bindWindowChrome();
  bindLibrary();

  // "gone with one click" per spec: no confirm dialog, unlike a real delete
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
    // bows out the moment a confirm dialog opens on top of it -- and
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
    if (!$("lightbox").classList.contains("hidden")) { trapTabWithin($("lightbox"), e); return; }
    if (!$("confirm-modal").classList.contains("hidden")) { trapTabWithin($("confirm-modal"), e); return; }
    if (!colorPop.classList.contains("hidden")) { trapTabWithin(colorPop, e); return; }
    if (!tagPop.classList.contains("hidden")) { trapTabWithin(tagPop, e); return; }
    if (!$("compare-modal").classList.contains("hidden")) { trapTabWithin($("compare-modal"), e); return; }
    if (!$("tag-modal").classList.contains("hidden")) { trapTabWithin($("tag-modal"), e); return; }
    if (!ctxEl.classList.contains("hidden")) { trapTabWithin(ctxEl, e); return; }
    if (!$("update-modal").classList.contains("hidden")) { trapTabWithin($("update-modal"), e); return; }
    if (!$("about-modal").classList.contains("hidden")) { trapTabWithin($("about-modal"), e); return; }
    if (!$("snip-modal").classList.contains("hidden")) { trapTabWithin($("snip-modal"), e); return; }
    if (!$("rec-modal").classList.contains("hidden")) { trapTabWithin($("rec-modal"), e); return; }
    if (!viewModal.root.classList.contains("hidden")) { trapTabWithin(viewModal.root, e); return; }
    if (!modal.root.classList.contains("hidden")) { trapTabWithin(modal.root, e); return; }
    if (!$("dupes-modal").classList.contains("hidden")) { trapTabWithin($("dupes-modal"), e); return; }
    if (!$("trash-modal").classList.contains("hidden")) { trapTabWithin($("trash-modal"), e); return; }
    if (document.body.classList.contains("library-front") && !libraryEl.classList.contains("hidden")) { trapTabWithin(libraryEl, e); return; }
    if (!$("backup-modal").classList.contains("hidden")) { trapTabWithin($("backup-modal"), e); return; }
    if (!libraryEl.classList.contains("hidden")) { trapTabWithin(libraryEl, e); return; }
    if (!helpEl.classList.contains("hidden")) { trapTabWithin(helpEl, e); return; }
    if (!settingsEl.classList.contains("hidden")) { trapTabWithin(settingsEl, e); return; }
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (!snipEdit.classList.contains("hidden")) return; // the editor owns Escape while open
    if (!$("shortcuts-modal").classList.contains("hidden")) { closeShortcutsModal(); return; }
    if (!toolbarMenu.classList.contains("hidden")) { closeToolbarMenu(); return; }
    if (!miniMenu.classList.contains("hidden")) { closeMiniMenu(); return; }
    if (!tileFolderPop.classList.contains("hidden")) { closeTileFolderPop(); return; }
    if (!viewsPop.classList.contains("hidden")) { closeViewsPop(); return; }
    if (!moveToFolderPop.classList.contains("hidden")) { closeMoveToFolderPop(); return; }
    if (!gridTagPop.classList.contains("hidden")) { closeGridTagPop(); return; }
    if (!openEditorPop.classList.contains("hidden")) { closeOpenEditorPop(); return; }
    if (!$("rec-modal-more-pop").classList.contains("hidden")) { closeRecMorePop(); return; }
    if (!$("lightbox").classList.contains("hidden")) closeLightbox();
    else if (dismissConfirm()) { /* the dialog closed itself */ }
    else if (!colorPop.classList.contains("hidden")) closeColorPop();
    else if (!tagPop.classList.contains("hidden")) closeTagPop();
    else if (!$("compare-modal").classList.contains("hidden")) closeCompareView();
    else if (!$("tag-modal").classList.contains("hidden")) closeTagManager();
    else if (!ctxEl.classList.contains("hidden")) closeCtx();
    else if (!$("update-modal").classList.contains("hidden")) closeUpdateModal();
    else if (!$("about-modal").classList.contains("hidden")) $("about-modal").classList.add("hidden");
    else if (!$("snip-modal").classList.contains("hidden")) $("snip-modal").classList.add("hidden");
    else if (!$("rec-modal").classList.contains("hidden")) closeRecModal();
    else if (!viewModal.root.classList.contains("hidden")) closeViewModal();
    else if (!modal.root.classList.contains("hidden")) confirmDiscardIfDirty().then((ok) => { if (ok) closeModal(); });
    else if (!$("dupes-modal").classList.contains("hidden")) $("dupes-modal").classList.add("hidden");
    else if (!$("trash-modal").classList.contains("hidden")) $("trash-modal").classList.add("hidden");
    // A library raised over the backups dialog is the topmost thing: it closes first.
    else if (document.body.classList.contains("library-front") && !libraryEl.classList.contains("hidden")) hideLibrary();
    else if (!$("backup-modal").classList.contains("hidden")) closeBackups();
    else if (!libraryEl.classList.contains("hidden")) hideLibrary();
    // Help opens over the settings panel (from expGroupHelp): close it before the
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
  // Same shape, the failure side -- an automatic backup that silently never ran
  // again would otherwise leave the user unprotected with nothing telling them.
  listen("backup-failed", () => toast(t("autoBackupFailed")));
  // A pill can unpin ITSELF (its own "Remove" action) without this window ever
  // being told -- the tile context menu's pin/unpin label then kept reading a
  // stale settings.floating and could re-open a pill the label said to remove.
  listen("floating-pin-changed", (e) => cacheFloatingState(e.payload.id, e.payload.pinned));
  // floatParkCleanupH hid a parked button's view (never its saved entry) --
  // point at the recall action, the one place that brings it back.
  listen("float-park-cleaned-up", () => {
    toast(t("floatParkCleanedUp"), { label: t("floatRecallAction"), onClick: () => EXPERT_ACTIONS["float-recall"]?.() });
  });
  // A copy recorded from a pill, the folder window or the tray never touched
  // this window's own settings.usage/last_used mirror -- "most used"/"recently
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
  // one-off event -- every setting changed for the rest of this session keeps
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
  // Every write path lib.rs gates on the data-folder lock (save_prompts,
  // save_prompts_and_trash, save_trash, write_settings_now) emits this same
  // event on refusal -- one toast, the same wording every time, instead of
  // each action failing quietly in its own different way.
  listen("data-dir-readonly-write-blocked", (e) => {
    toast(t("dataDirReadOnlyToast").replace("{host}", e.payload || dataDirLockStatus.otherHost || "?"));
  });
  // The disk problem that caused the above went away and a later settings write
  // reached it again -- the notice does not need to survive that, so it comes
  // straight back down (the dialog itself still stays told-once for the session).
  listen("settings-write-recovered", () => {
    assignSettingsWriteFailed(false);
    if (!$("settings-expert").classList.contains("hidden")) renderExpert();
  });
  // The one persistent, always-visible sign that the remote-access receiver
  // is reachable at all -- shown for as long as it is actually running, with
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

// Drag & drop onto the window: a dropped file/image/PDF becomes a button right
// away (same dialog as the paperclip); dropped text fills the composer and opens
// the save dialog. Extends the existing clipboard/screenshot paths.
function setupDragDrop() {
  const wv = window.__TAURI__?.webview?.getCurrentWebview?.();
  wv?.onDragDropEvent?.((e) => {
    if (!flag("dragDrop")) return;
    const kind = e.payload?.type;
    document.body.classList.toggle("drag-over", kind === "enter" || kind === "over");
    if (kind === "drop") {
      document.body.classList.remove("drag-over");
      const path = e.payload?.paths?.[0];
      if (!path) return;
      // A drop replaces whatever the editor is showing with a brand-new create
      // dialog -- if it is already open with unsaved changes, ask first, same as
      // the X button/backdrop/Escape.
      confirmDiscardOrProceed(() => openFileCreate(path).catch((err) => errToast(err)));
    }
  }).catch?.(() => {});
  // Text dropped from another app (no file paths): native handler ignores it.
  window.addEventListener("dragover", (e) => e.preventDefault());
  window.addEventListener("drop", (e) => {
    if (!flag("dragDrop")) return;
    const text = e.dataTransfer?.getData("text");
    if (text && !(e.dataTransfer.files && e.dataTransfer.files.length)) {
      e.preventDefault();
      confirmDiscardOrProceed(() => {
        inputEl.value = text;
        autoGrow(inputEl);
        saveBtn.disabled = !text.trim();
        startCreate();
      });
    }
  });
}

// ---- First-run tour ----
// Five fixed steps in one shared frame (#tour-modal), content swapped per step.
// Steps 1-4 are a small hand-drawn illustration + up to three lines of text; step 5
// is what else the program can do (TOUR_FEATURES below) plus what is already set
// up (TOUR_SETUP below). Opened two ways:
//  - Automatically once, right after the example prompts are seeded on a genuinely
//    fresh install (maybeOpenFirstRunTour, called from init()'s reveal()). Step 5
//    buffers its switches in a local draft and only writes what differs from the
//    shipped default (features) or the value the tour opened with (setup), on
//    Finish. Skip writes nothing but tourSeen.
//  - Manually via the expert menu's gated "tour-open" action (EXPERT_ACTIONS). Step 5
//    then writes live, exactly like a normal expert-menu flag row: nothing is
//    buffered or reset just from reopening it this way.

// Step 5's second section: the settings already decided on first launch, shown
// read-only except a plain switch flip. Not feature flags -- real settings with
// their own invoke commands, so each row carries its own live-write function.
const TOUR_SETUP = [
  { key: "autostart", labelKey: "autostart", textKey: "tourSetAutostart",
    get: () => settings.autostart === true,
    setLive: (v) => tourSetSettingLive("set_autostart", "autostart", v) },
  { key: "minimizeOnClose", labelKey: "minimizeOnClose", textKey: "tourSetMinimize",
    get: () => settings.minimize_to_tray === true,
    setLive: (v) => tourSetSettingLive("set_minimize_on_close", "minimize_to_tray", v) },
  { key: "autoUpdate", labelKey: "autoUpdate", textKey: "tourSetUpdate",
    get: () => settings.auto_update !== false,
    setLive: (v) => tourSetSettingLive("set_auto_update", "auto_update", v) },
];

let tourStep = 0;
let tourFirstRun = false; // draft-buffered (true) vs. live-writing (false) step 5
let tourDraft = {}; // flagKey -> bool, only meaningful while tourFirstRun is true
let tourSetupDraft = {}; // TOUR_SETUP key -> bool, only meaningful while tourFirstRun is true
let tourSetupInitial = {}; // snapshot of tourSetupDraft at open, to know what actually changed

// Shared by both step-5 modes' "write it now" path: identical to flagRow()'s own
// change handler (minus the promptSync confirmation, which none of these three need).
async function tourSetFlagLive(key, enabled) {
  settings.ui_flags = { ...(settings.ui_flags || {}), [key]: enabled };
  try { await invoke("set_ui_flag", { key, enabled }); } catch (err) { errToast(err); }
  applyFlags();
}

// Live-write for a TOUR_SETUP row -- these are real settings fields (autostart,
// auto_update), each with its own invoke command, not ui_flags.
async function tourSetSettingLive(cmd, field, enabled) {
  try { await invoke(cmd, { enabled }); settings[field] = enabled; } catch (err) { errToast(err); }
}

function renderTourDrawingStep(step) {
  const wrap = document.createElement("div");
  wrap.className = "tour-step";
  const art = document.createElement("div");
  art.className = "tour-art";
  art.setAttribute("aria-hidden", "true");
  art.innerHTML = step.svg;
  const text = document.createElement("div");
  text.className = "tour-text";
  for (const lk of step.lineKeys) {
    const p = document.createElement("p");
    p.textContent = t(lk);
    text.appendChild(p);
  }
  wrap.append(art, text);
  return wrap;
}

// Small "?" button next to a feature row, opening the in-app help at the
// article matching that row's helpAnchor. Lives inside the row's own <label>
// (same trick as the expert menu's per-row reset button, e.g. valScreenshotDir):
// stopPropagation keeps the click from also toggling the row's switch.
function tourHelpButton(anchor) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "icon-btn tour-help-btn";
  btn.title = t("tourHelpBtn");
  btn.setAttribute("aria-label", t("tourHelpBtn"));
  btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M8 1a5 5 0 0 0-1 9.9V12h2v-1.1A5 5 0 0 0 8 1zm0 12.5a1.1 1.1 0 1 0 0 2.2 1.1 1.1 0 0 0 0-2.2z"/></svg>';
  btn.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    closeTour();
    openHelpAt(anchor);
  });
  return btn;
}

function renderTourOptionsStep() {
  const wrap = document.createElement("div");
  wrap.className = "tour-options";
  for (const opt of TOUR_FEATURES) {
    const row = document.createElement("label");
    row.className = "field switch-field tour-opt-row";
    const label = document.createElement("span");
    label.className = "tour-opt-label";
    const name = document.createElement("b");
    name.textContent = t(opt.labelKey);
    const desc = document.createElement("span");
    desc.className = "tour-opt-desc";
    desc.textContent = t(opt.textKey);
    label.append(name, desc);
    const note = document.createElement("span");
    note.className = "tour-opt-default";
    note.textContent = shipsOn(opt.flagKey) ? t("tourDefaultOn") : t("tourDefaultOff");
    const input = document.createElement("input");
    input.type = "checkbox";
    input.className = "switch";
    input.checked = tourFirstRun ? tourDraft[opt.flagKey] : flagState(opt.flagKey);
    input.addEventListener("change", () => {
      if (tourFirstRun) tourDraft[opt.flagKey] = input.checked;
      else tourSetFlagLive(opt.flagKey, input.checked);
    });
    row.append(label, tourHelpButton(opt.helpAnchor), note, input);
    wrap.appendChild(row);
  }
  const restore = document.createElement("button");
  restore.type = "button";
  restore.className = "ghost-btn tour-restore-btn";
  restore.textContent = t("tourRestoreDefaults");
  restore.addEventListener("click", async () => {
    if (tourFirstRun) {
      tourDraft = Object.fromEntries(TOUR_FEATURES.map((o) => [o.flagKey, shipsOn(o.flagKey)]));
    } else {
      for (const opt of TOUR_FEATURES) await tourSetFlagLive(opt.flagKey, shipsOn(opt.flagKey));
    }
    renderTourStep(); // reflect the reset switches (and re-focus Next)
  });
  wrap.appendChild(restore);
  wrap.appendChild(renderTourSetupSection());
  const foot = document.createElement("p");
  foot.className = "hint tour-options-foot";
  foot.textContent = t("tourFooterNote");
  wrap.appendChild(foot);
  return wrap;
}

// The second section of step 5: the settings already decided on first launch.
// Autostart and the update check are plain switches sharing the feature rows'
// buffering; the data folder is read-only here on purpose -- moving it is a
// multi-step flow of its own, out of place inside the tour.
function renderTourSetupSection() {
  const section = document.createElement("div");
  section.className = "tour-setup";
  const heading = document.createElement("div");
  heading.className = "tour-setup-heading";
  heading.textContent = t("tourSetupHeading");
  section.appendChild(heading);
  // Autostart and the update check side by side -- two switches sharing one
  // row's height instead of two, the difference that keeps step 5 within the
  // measured min-height budget every other step already fits.
  const pair = document.createElement("div");
  pair.className = "tour-setup-pair";
  for (const opt of TOUR_SETUP) {
    const row = document.createElement("label");
    row.className = "field switch-field tour-setup-row";
    const label = document.createElement("span");
    label.className = "tour-opt-label";
    const name = document.createElement("b");
    name.textContent = t(opt.labelKey);
    const desc = document.createElement("span");
    desc.className = "tour-opt-desc";
    desc.textContent = t(opt.textKey);
    label.append(name, desc);
    const input = document.createElement("input");
    input.type = "checkbox";
    input.className = "switch";
    input.checked = tourFirstRun ? tourSetupDraft[opt.key] : opt.get();
    input.addEventListener("change", () => {
      if (tourFirstRun) tourSetupDraft[opt.key] = input.checked;
      else opt.setLive(input.checked);
    });
    row.append(label, input);
    pair.appendChild(row);
  }
  section.appendChild(pair);
  const dataRow = document.createElement("div");
  dataRow.className = "field tour-setup-row tour-setup-datadir";
  const dataLabel = document.createElement("span");
  dataLabel.className = "tour-opt-label";
  const dataName = document.createElement("b");
  dataName.textContent = t("tourSetDataDir");
  const dataPath = document.createElement("span");
  dataPath.className = "tour-opt-desc";
  dataPath.textContent = currentDataDir;
  dataLabel.append(dataName, dataPath);
  const dataChange = document.createElement("button");
  dataChange.type = "button";
  dataChange.className = "ghost-btn tour-setup-change-btn";
  dataChange.textContent = t("tourSetDataDirChange");
  dataChange.addEventListener("click", () => {
    closeTour();
    jumpToExpertSetting("dataDir", "paths");
  });
  dataRow.append(dataLabel, dataChange);
  section.appendChild(dataRow);
  return section;
}

function renderTourStep() {
  const isLast = tourStep === TOUR_LAST_STEP;
  const titleKey = tourStep < TOUR_STEPS.length ? TOUR_STEPS[tourStep].titleKey : "tourStep5Title";
  $("tour-title").textContent = t(titleKey);
  const body = $("tour-body");
  body.innerHTML = "";
  body.appendChild(tourStep < TOUR_STEPS.length ? renderTourDrawingStep(TOUR_STEPS[tourStep]) : renderTourOptionsStep());
  const dots = $("tour-dots");
  dots.innerHTML = "";
  for (let i = 0; i < TOUR_STEP_COUNT; i++) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "tour-dot" + (i === tourStep ? " active" : "");
    b.textContent = i === tourStep ? "●" : "○";
    b.setAttribute("aria-label", t(i < TOUR_STEPS.length ? TOUR_STEPS[i].titleKey : "tourStep5Title"));
    b.addEventListener("click", () => tourGoto(i));
    dots.appendChild(b);
  }
  $("tour-back").classList.toggle("hidden", tourStep === 0);
  $("tour-next").textContent = t(isLast ? "tourFinish" : "tourNext");
  $("tour-next").focus();
}

function tourGoto(i) {
  tourStep = Math.max(0, Math.min(TOUR_LAST_STEP, i));
  renderTourStep();
}

// Keyboard focus trap shared by every modal overlay with aria-modal="true"
// (screenshot editor, first-run tour): keeps Tab/Shift+Tab cycling within
// the container instead of escaping to the page behind it. Declared at top
// level so both bind()'s screenshot-editor code and the tour code below can
// call it without a second implementation.
function tourKeydown(e) {
  // A confirmation dialog on top owns every key while it is open --
  // otherwise Escape would close both it and the tour underneath on one press.
  if (!$("confirm-modal").classList.contains("hidden")) return;
  if (e.key === "Tab") { trapTabWithin($("tour-modal"), e); return; }
  if (e.key === "Escape") { e.preventDefault(); tourSkipWithUndo(); return; }
  if (e.key === "ArrowLeft") { e.preventDefault(); tourGoto(tourStep - 1); return; }
  if (e.key === "ArrowRight") { e.preventDefault(); tourGoto(tourStep + 1); return; }
  if (e.key === "Enter" && !e.target.closest("button")) {
    // A focused button already turns Enter into its own click; only steps in for
    // everything else (e.g. focus left on a step-5 switch).
    e.preventDefault();
    tourStep === TOUR_LAST_STEP ? tourFinish() : tourGoto(tourStep + 1);
  }
}

// The dialog must not visibly resize as the user pages through the five
// steps, but a single fixed number (the old tourStepMinH expert setting) is
// either too tall for the shortest step (an empty-looking box) or too short
// for the tallest (a jump on Next). Measuring every step once, right after the
// modal becomes visible (a hidden ancestor reports 0 for any height), and
// keeping the tallest is the actual fix -- there is nothing left to tune by
// hand, which is also why there is no expert setting for it.
function tourMeasureMaxStepHeight() {
  const body = $("tour-body");
  let max = 0;
  for (let i = 0; i < TOUR_STEP_COUNT; i++) {
    const wrap = i < TOUR_STEPS.length ? renderTourDrawingStep(TOUR_STEPS[i]) : renderTourOptionsStep();
    body.innerHTML = "";
    body.appendChild(wrap);
    // Layout height, not the painted rect: the open animation scales the dialog
    // while it is measured, which made the tallest step read about 3% short.
    max = Math.max(max, wrap.offsetHeight);
  }
  return max;
}

async function openTour({ firstRun }) {
  tourFirstRun = firstRun;
  tourStep = 0;
  tourDraft = firstRun ? Object.fromEntries(TOUR_FEATURES.map((o) => [o.flagKey, shipsOn(o.flagKey)])) : {};
  tourSetupDraft = firstRun ? Object.fromEntries(TOUR_SETUP.map((o) => [o.key, o.get()])) : {};
  tourSetupInitial = firstRun ? { ...tourSetupDraft } : {};
  $("tour-modal").classList.remove("hidden");
  const maxH = tourMeasureMaxStepHeight();
  document.documentElement.style.setProperty("--tour-step-min-h", `${Math.ceil(maxH)}px`);
  renderTourStep();
  document.addEventListener("keydown", tourKeydown);
}

function closeTour() {
  $("tour-modal").classList.add("hidden");
  document.removeEventListener("keydown", tourKeydown);
}

async function tourSkip() {
  closeTour();
  await saveUiState("tourSeen", true); // Skip writes nothing else
}

// Escape skips at once; the toast's Undo reopens the tour where it started.
async function tourSkipWithUndo() {
  const firstRun = tourFirstRun;
  await tourSkip();
  undoToast(t("tourSkippedToast"), () => openTour({ firstRun }));
}

// Only the switches that differ from the shipped default get written: an
// untouched draft leaves ui_flags exactly as it was, so a later default change
// still reaches this install. The setup rows have no shipped default to compare
// against (autostart/auto_update are real settings): they compare against
// whatever they actually were when the tour opened instead.
async function tourFinishDraft() {
  for (const opt of TOUR_FEATURES) {
    const shipped = shipsOn(opt.flagKey);
    if (tourDraft[opt.flagKey] !== shipped) await tourSetFlagLive(opt.flagKey, tourDraft[opt.flagKey]);
  }
  for (const opt of TOUR_SETUP) {
    if (tourSetupDraft[opt.key] !== tourSetupInitial[opt.key]) await opt.setLive(tourSetupDraft[opt.key]);
  }
}

async function tourFinish() {
  if (tourFirstRun) await tourFinishDraft();
  closeTour();
  await saveUiState("tourSeen", true);
}

EXPERT_ACTIONS["tour-open"] = () => openTour({ firstRun: false });

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
  // never DPAPI, so it is the one way out that still works right now -- offer it
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
// warnStoreUnsealed above -- a stuck disk fails the same way on every
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
// exists (see ensure_webview2 in lib.rs, the one allowed exception to routing every
// user-facing dialog through the themed confirmDialog()), so there is nothing left
// to check for it here by the time init() runs.
async function maybeOpenFirstRunTour(sealWarned) {
  if (sealWarned) return; // retried next launch: tourSeen was never written
  if (settings.ui_flags?.tourSeen) return;
  if (!flag("firstRunTour")) return;
  await openTour({ firstRun: true });
}

// A crash draft from a previous session, asked about exactly once at the next
// start (never again on later editor opens this same session). "No" removes
// it right away rather than leaving it to resurface on the next launch too.
async function maybeOfferEditorDraftRecovery() {
  if (val("editorDraftSecs") <= 0) return;
  let draft = null;
  try { draft = await invoke("load_editor_draft"); } catch (_) { return; }
  if (!draft) return;
  const when = new Date(draft.updatedAt * 1000).toLocaleString();
  const preview = (draft.name || draft.text || "").split("\n").slice(0, 2).join(" ").trim().slice(0, 140);
  const restore = await confirmDialog({
    title: t("draftRecoverTitle"),
    message: t("draftRecoverBody").replace("{time}", when).replace("{preview}", preview),
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
    // (an early migration write, say) could already be true here -- everything
    // after boot arrives live through the "persist-failed" event instead. The
    // dialog itself is NOT opened from here -- it has to wait its turn in
    // reveal()'s chain below, same as warnStoreUnsealed, or it races the
    // takeover offer for the shared confirm-modal element.
    invoke("settings_write_failed").then((v) => {
      assignSettingsWriteFailed(v === true);
      if (!$("settings-expert").classList.contains("hidden")) renderExpert();
    }).catch(() => {});
    invoke("default_screenshot_dir").then((d) => { assignDefaultScreenshotDir(d || ""); }).catch(() => {});
    invoke("default_sync_dir").then((d) => { assignDefaultSyncDir(d || ""); }).catch(() => {});
    invoke("default_recording_dir").then((d) => { assignDefaultRecordingDir(d || ""); }).catch(() => {});
    // Whatever the startup sweep found (if anything) -- collected once,
    // here, since it ran before this window (or any of its listeners) existed.
    invoke("take_recording_sweep_notice").then((r) => {
      if (r && r[0] > 0) toast(t("recSweepDone").replace("{n}", String(r[0])).replace("{v}", fmtBytes(r[1])));
    }).catch(() => {});
    // OCR language list -- fetched once, cached, "Automatic" is always
    // prepended separately (see EXPERT_DROPDOWNS.ocrLang above).
    invoke("ocr_languages").then((langs) => { assignOcrLangsCache(langs || []); }).catch(() => {});
    // Microphone list for recMicDevice -- fetched once, cached, "Automatic"
    // (system default) is always prepended separately (see EXPERT_DROPDOWNS.recMicDevice above).
    invoke("list_mic_devices").then((devs) => { assignMicDevicesCache(devs || []); }).catch(() => {});
    // Camera list for EXPERT_DROPDOWNS.cameraDevice above -- a browser API
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
    try { toast(String((e && e.message) || e)); } catch (_) {}
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
        await offerTakeover();
        // Seeded AFTER the takeover offer, not before: seeding earlier (e.g. from
        // applySettingsUi()) would make the library non-empty and takeover_offer()
        // would then never fire, hiding the adoption question behind the tour. Only
        // re-render if something actually landed: the common case (a library that
        // already has content) must not repaint the grid a second time on every
        // launch, which would invalidate tile references a user could already be
        // interacting with.
        const seeded = await seedExamplesIfNeeded();
        if (seeded && seeded.length) await renderGrid();
        // Ahead of the tour: an unencrypted store is the more urgent of the two,
        // and the tour is retried next launch (tourSeen is only written by it).
        const sealWarned = await warnStoreUnsealed();
        await warnSettingsWriteFailed();
        await warnDataDirSyncFolder();
        await maybeOpenFirstRunTour(sealWarned);
        await maybeOfferEditorDraftRecovery();
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
