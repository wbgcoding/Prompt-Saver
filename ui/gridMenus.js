// Tile context menu, views popup, move-to-folder popup, grid tag popup and the toolbar menu.
import { $, assignSettings, ctxEl, errToast, favView, flag, gridTags, invoke, libraryEl, optFlag, prompts, settings, t, TB_CHECK, toast } from "./core.js";
import { chainMode, isTextPrompt, startChainWith } from "./chain.js";
import { hasMarkdownSyntax } from "./markdown.js";
import { batchErrorToast, ensureArchiveTagName, isArchived, renderLibrary, toggleFavoritePrompt, toggleGridSelect, trashPromptWithUndo } from "./library.js";
import { cacheFloatingState, foldersOfView, folderTitle, newTileFolder, saveFolder, tileFoldersOn } from "./gridFolders.js";
import { mediaKind } from "./media.js";
import { placePopover, placePopoverAt } from "./popover.js";
import { activeView, gridKeyOf, layoutOf, refreshFavViewUi, renderGrid, renderViews, viewRowBase } from "./views.js";
import { allTags, tagChipEl, tagKey } from "./tags.js";
import { FLAG_LABELS, OPT_FLAG_KEYS, OPT_FLAG_LABELS } from "./expertConfig.js";
import { applyFlags } from "./expertMenu.js";
import { duplicatePrompt, editPrompt } from "./editor.js";
import { activateTile, copyFormattedPrompt } from "./copyFlow.js";
import { recordToolUsage } from "./management.js";
import { prettyAccel, promptHotkeysOn, toggleRotateAutoDirect } from "./hotkeys.js";

export let ctxId = null;

// ---- Context menu ----
export function openCtx(id, x, y) {
  ctxId = id;
  const cp = prompts.find((x) => x.id === id);
  // Copy only, like a plain tile click. The prompt's own hotkey, when it has one and
  // prompt hotkeys are on, sits right-aligned beside it.
  const kbd = ctxEl.querySelector('[data-act="copy"] .ctx-kbd');
  const hotkeyText = cp?.hotkey && promptHotkeysOn() ? prettyAccel(cp.hotkey) : "";
  kbd.textContent = hotkeyText;
  kbd.classList.toggle("hidden", !hotkeyText);
  const favBtn = ctxEl.querySelector('[data-act="favorite"]');
  favBtn.classList.toggle("hidden", !flag("favorites"));
  favBtn.querySelector(".ctx-label").textContent = t(cp?.favorite ? "favoriteRemove" : "favoriteAdd");
  // Starting a chain here only makes sense for a text prompt while no chain runs.
  ctxEl.querySelector('[data-act="chain"]').classList.toggle("hidden", !(isTextPrompt(cp) && flag("chainPrompts") && flag("chainQuickStart") && !chainMode));
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
  pinBtn.querySelector(".ctx-label").textContent = t(pinned ? "removeFloating" : "showFloating");
  // Archive: hidden unless the feature is on; the label names the action, not a state.
  const archiveBtn = ctxEl.querySelector('[data-act="archive"]');
  archiveBtn.classList.toggle("hidden", !flag("archiveTag"));
  const archivedNow = isArchived(prompts.find((x) => x.id === id) || {});
  archiveBtn.querySelector(".ctx-label").textContent = t(archivedNow ? "unarchiveAction" : "archiveAction");
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
  // A separator only stays while at least one entry between it and the previous
  // separator is shown.
  let groupShown = false;
  for (const el of ctxEl.children) {
    if (el.classList.contains("ctx-sep")) { el.classList.toggle("hidden", !groupShown); groupShown = false; }
    else if (!el.classList.contains("hidden")) groupShown = true;
  }
  placePopoverAt(ctxEl, x, y);
}
export function closeCtx() {
  ctxEl.classList.add("hidden");
  ctxId = null;
}

// ---- "also show in" popover: place/unplace this prompt on other views ----
export const viewsPop = document.createElement("div");

export function closeViewsPop() {
  viewsPop.classList.add("hidden");
}
export function renderViewsPop(id) {
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
export function openViewsPop(id, rect) {
  renderViewsPop(id);
  placePopover(viewsPop, rect);
}

// ---- Move a tile into an existing folder by keyboard (the mouse path is a
// drag onto an armed cell, see dropIntoFolder above) ----
export const moveToFolderPop = document.createElement("div");

export function closeMoveToFolderPop() {
  moveToFolderPop.classList.add("hidden");
}
export function renderMoveToFolderPop(id) {
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
export function openMoveToFolderPop(id, rect) {
  renderMoveToFolderPop(id);
  placePopover(moveToFolderPop, rect);
}

// ---- Grid tag filter: header toolbar popover, own Set from the library's ----
export const gridTagPop = document.createElement("div");

export function closeGridTagPop() {
  gridTagPop.classList.add("hidden");
}
export function updateGridTagBtn() {
  $("grid-tag-btn")?.classList.toggle("active", gridTags.size > 0);
}
export function renderGridTagPop() {
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
export function openGridTagPop() {
  renderGridTagPop();
  placePopover(gridTagPop, $("grid-tag-btn").getBoundingClientRect());
}

// ---- Right-click toolbar menu: show or hide header buttons. Whole features (views, quick grid, clipboard watcher, chaining) are switched in the expert menu, never here. ----
export const TOOLBAR_TOOLS = ["favViewButton", "showLibrary", "barToggles", "showLogo", "showTitle"];
// Little glyph shown beside each tool name so the menu reads at a glance.
export const tbIcon = (d, fill) => `<svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path ${fill ? 'fill="currentColor"' : 'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"'} d="${d}"/></svg>`;
// Tool-menu icons mirror the real toolbar buttons so the menu reads the same as
// the header (library and journal match 1:1).
export const TOOL_ICONS = {
  favViewButton: tbIcon("M12 3.5l2.5 5.2 5.7.8-4.1 4 1 5.7L12 16.9l-5.1 2.6 1-5.7-4.1-4 5.7-.8z", true),
  showLibrary: tbIcon("M4 6h2v2H4V6Zm4 0h12v2H8V6ZM4 11h2v2H4v-2Zm4 0h12v2H8v-2ZM4 16h2v2H4v-2Zm4 0h12v2H8v-2Z", true),
  barToggles: tbIcon("M6 9l6 6 6-6"),
  showLogo: tbIcon("M4 5h16v14H4zM7 15l3.5-3.5 3 3 2.5-2.5 2 2M8.6 10a1.4 1.4 0 1 1-2.8 0 1.4 1.4 0 0 1 2.8 0"),
  showTitle: tbIcon("M6 7V5h12v2M12 5v14M9 19h6"),
};
export const toolbarMenu = document.createElement("div");

export const toolFlagOn = (key) => (OPT_FLAG_KEYS.includes(key) ? optFlag(key) : flag(key));
export function buildToolbarMenu() {
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
export function openToolbarMenu(x, y) {
  buildToolbarMenu();
  placePopoverAt(toolbarMenu, x, y);
}
export function closeToolbarMenu() { toolbarMenu.classList.add("hidden"); }

export function bindGridMenus({ openExternalFileInEditor }) {
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
    if (act === "copy") {
      await activateTile(id, false); // copy only, even with auto-paste on
    } else if (act === "favorite") {
      const p = prompts.find((x) => x.id === id);
      if (p) await toggleFavoritePrompt(p);
    } else if (act === "select") {
      toggleGridSelect(id);
    } else if (act === "chain") {
      const p = prompts.find((x) => x.id === id);
      if (p) await startChainWith(p);
    } else if (act === "edit") {
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

export function wireGridMenus() {
  viewsPop.className = "ctx views-pop hidden";
  viewsPop.setAttribute("role", "menu");
  document.body.appendChild(viewsPop);
  moveToFolderPop.className = "ctx move-to-folder-pop hidden";
  moveToFolderPop.setAttribute("role", "menu");
  document.body.appendChild(moveToFolderPop);
  gridTagPop.className = "ctx grid-tag-pop hidden";
  gridTagPop.setAttribute("role", "menu");
  document.body.appendChild(gridTagPop);
  toolbarMenu.className = "ctx toolbar-menu hidden";
  toolbarMenu.setAttribute("role", "menu");
  document.body.appendChild(toolbarMenu);
}
