// Main-window controller. Uses Tauri's global API (withGlobalTauri).
import { primeLang } from "./i18n.js";
import EN from "./i18n/en.js";
import { mediaKind } from "./media.js";
import { FOLDER_SVG, fmtBytes, trapTabWithin } from "./floatShared.js";
import { placePopover, placePopoverAt } from "./popover.js";
import { confirmDialog, infoDialog, dismissConfirm, initDialogs } from "./dialog.js";
import { FLAG_LABELS, OPT_FLAG_LABELS, OPT_FLAG_KEYS } from "./expertConfig.js";
import { hasMarkdownSyntax } from "./markdown.js";
import { assignVarsWindowPending, varsWindowPending } from "./promptVars.js";
import { $, applyBrand, appVersion, assignAppVersion, assignSettings, autoGrow, closeColorPop, colorPop, ctxEl, disarmButton, errToast, EXPERT_ACTIONS, favView, flag, gridTags, ICON_WARN, inputEl, invoke, libraryEl, listen, modal, optFlag, prompts, setStatusText, settings, settingsEl, t, TB_CHECK, toast, val, wireColorPop } from "./core.js";
import { isTextPrompt } from "./chain.js";
import { activeView, assignTabDragBlockClick, bindViews, closeMiniMenu, closeViewModal, gridKeyOf, layoutOf, miniMenu, refreshFavViewUi, renderGrid, renderViews, seedExamplesIfNeeded, switchToView, viewModal, viewOrder, viewRowBase, wireNumPop, wireViews } from "./views.js";
import { bindCopyFlow, copyFormattedPrompt, onPromptHotkey, wireCopyFlow } from "./copyFlow.js";
import { applyMissingFiles, fitAllTiles, fitCache, linkTiles, pollMissingFiles, wireTiles } from "./tiles.js";
import { bindWindowChrome, closeLightbox, closeShortcutsModal, wireClearButtons, wireLightbox, wireTooltips, wireWindowControls } from "./windowChrome.js";
import { bindHelpView } from "./helpView.js";
import { batchErrorToast, bindLibrary, ensureArchiveTagName, hideLibrary, isArchived, linkLibrary, renderLibrary, trashPromptWithUndo } from "./library.js";
import { allTags, closeTagManager, closeTagPop, tagChipEl, tagKey, tagPop, wireTags } from "./tags.js";
import { bindJournal } from "./journal.js";
import { applyTheme, applyTileStyle, bindTheme, wireTheme } from "./theme.js";
import { applySettingsCore, applySettingsUi, bindSettingsPanel, hideSettings, offerTakeover, runExportAll } from "./settingsPanel.js";
import { bindHotkeys, toggleRotateAutoDirect } from "./hotkeys.js";
import { bindAbout } from "./about.js";
import { applyFlags, assignCameraDevicesCache, assignCurrentDataDir, assignDefaultRecordingDir, assignDefaultScreenshotDir, assignDefaultSyncDir, assignMicDevicesCache, assignOcrLangsCache, assignSettingsWriteFailed, assignStoreSealed, assignVoiceLangsCache, assignVoiceSupport, bindExpertMenu, dataDirLockStatus, refreshDataDirLock, refreshSettingsLocked, renderExpert, settingsWriteFailed, storeSealed, syncBannerStack, warnDataDirSyncFolder } from "./expertMenu.js";
import { bindManagement, closeCompareView, recordToolUsage, wireManagement } from "./management.js";
import { bindBackups, closeBackups } from "./backups.js";
import { bindDictation, wireDictation } from "./dictation.js";
import { bindEditor, closeModal, confirmDiscardIfDirty, duplicatePrompt, editPrompt, openModal, promptToModal, setupDragDrop, wireEditor } from "./editor.js";
import { cacheFloatingState, closeTileFolderPop, foldersOfView, folderTitle, linkGridFolders, newTileFolder, saveFolder, tileFolderPop, tileFoldersOn, wireGridFolders } from "./gridFolders.js";
import { wireBgOps } from "./bgOps.js";
import { bindTour, openTour, wireTour } from "./tour.js";
import { bindGlobalSearch } from "./globalSearch.js";
import { bindCapture, wireCapture } from "./capture.js";
linkTiles({ openCtx });
linkLibrary({ openCtx });
linkGridFolders({ openCtx });
primeLang("en", EN);

let ctxId = null;

// Surface unexpected errors as a toast instead of failing silently.
window.addEventListener("error", (e) => toast(String(e.message)));
window.addEventListener("unhandledrejection", (e) => toast(String(e.reason)));

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

wireEditor();

wireCopyFlow();

wireManagement();

wireDictation();

wireTags();

wireTheme();

// ---- Copy history & usage journal ----

// ---- Settings: views editor ----
// Views are managed straight from the top view tabs: the "+" adds one, a
// right-click opens the view popup (rename, grid size, color, delete). The old
// settings-page views editor was removed.

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
