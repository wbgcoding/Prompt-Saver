// Grid folders: folder tiles, the folder popup, drag in and out.
import { assignDrag, assignFolderArm, assignMemberDrag, assignSettings, convertFileSrc, drag, DRAG_THRESHOLD, errToast, favView, flag, folderArm, gridEl, ICON_EDIT, ICON_FILE, ICON_IMAGE, ICON_PDF, ICON_VIDEO, inkShadow, invoke, memberDrag, openColorPop, PDF_EXT, prompts, readableInk, settings, t, TILE_FOLDER_ZONE } from "./core.js";
import { confirmDialog } from "./dialog.js";
import { activeView, openMiniMenu, renderGrid } from "./views.js";
import { recordToolUsage } from "./management.js";
import { autoPasteDouble, autoPasteNow, cellAt, endDragVisuals, fitTilesShared, hasImage, imageOf, missingFiles, setHoverCell, showCopied } from "./tiles.js";
import { FOLDER_SVG } from "./floatShared.js";
import { mediaKind } from "./media.js";
import { FONTS } from "./fonts.js";
import { placePopover } from "./popover.js";
import { chainMode, toggleChainPick } from "./chain.js";
import { dialogTokens, peekRotatedText } from "./promptVars.js";
import { copyOnCooldown, copyTextWithVars, maybeCloseOnCopy, recordCopy } from "./copyFlow.js";
import { copyResolved } from "./library.js";
import { openCtx } from "./gridMenus.js";

const TILE_FOLDER_COLS = 3; // most columns of the open grid-folder popover
const TILE_FOLDER_GAP = 6; // gap between member tiles, matches .tile-folder-grid
const TILE_FOLDER_HOVER_MS = 300; // dwell time over a folder before a dragged prompt drops in

// ---- Grid folders: several prompts collapsed into one cell ----
// Ships ON. A member has no layout entry of its own while it sits in a folder, so
// switching the feature off simply shows every prompt as its own tile again: the
// folders stay stored and come back with it.
export const tileFoldersOn = () => flag("folders");
export const tileFolderList = () => (tileFoldersOn() ? settings.tile_folders || [] : []);
export const foldersOfView = (viewId) => tileFolderList().filter((f) => f.view_id === viewId);
export const folderById = (id) => tileFolderList().find((f) => f.id === id);
export const folderMembers = (f) =>
  (f.members || []).map((id) => prompts.find((p) => p.id === id)).filter(Boolean);
export const folderTitle = (f) => f.name || t("tileFolderDefaultName");

// One write per change: name, colour, cell and members always travel together, so
// the grid never shows a half-moved folder.
export async function saveFolder(f, patch) {
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
export async function newTileFolder(memberIds, cell) {
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

export async function renameTileFolder(f) {
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

export let folderColorTimer = null;
export function pickTileFolderColor(f, anchor) {
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

export async function dissolveTileFolder(f) {
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
export async function takeOutOfFolder(id, cell) {
  try {
    assignSettings(await invoke("take_out_of_tile_folder", { id, cell }));
  } catch (e) {
    errToast(e);
    return;
  }
  closeTileFolderPop();
  await renderGrid(true);
}

// A folder cell: the number of tiles it holds, large, and one colour dot per tile (at most eight).
export function buildFolderTile(f) {
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
  mini.setAttribute("role", "img");
  mini.setAttribute("aria-label", t("tileFolderCount").replace("{n}", String(members.length)));
  const count = document.createElement("span");
  count.className = "folder-count";
  count.textContent = String(members.length);
  mini.appendChild(count);
  const dots = document.createElement("span");
  dots.className = "folder-dots";
  for (const p of members.slice(0, 8)) {
    const dot = document.createElement("span");
    dot.className = "folder-dot";
    if (p.color) dot.style.background = p.color;
    else {
      const dotImg = imageOf(p);
      if (dotImg) {
        const img = document.createElement("img");
        img.src = dotImg;
        img.draggable = false;
        dot.appendChild(img);
      } else dot.classList.add("folder-dot-plain");
    }
    dots.appendChild(dot);
  }
  mini.appendChild(dots);
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
export const tileFolderPop = document.createElement("div");

export let tileFolderPopId = null;

// Dims the grid behind an open folder. A press on it closes the folder through the
// outside-press handler in main.js; while a member is being dragged out it lets the
// pointer through (class member-dragging) so the cell under the cursor is reachable.
export const tileFolderBackdrop = document.createElement("div");

export function closeTileFolderPop() {
  tileFolderPop.classList.add("hidden");
  tileFolderBackdrop.classList.add("hidden");
  tileFolderPopId = null;
}

const CLOSE_ICON =
  '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="currentColor" d="M6.4 5 12 10.6 17.6 5 19 6.4 13.4 12 19 17.6 17.6 19 12 13.4 6.4 19 5 17.6 10.6 12 5 6.4Z"/></svg>';

function folderPopButton(icon, label, run) {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "icon-btn tile-folder-pop-btn";
  b.title = label;
  b.setAttribute("aria-label", label);
  b.innerHTML = icon;
  b.addEventListener("click", (ev) => { ev.stopPropagation(); run(); });
  return b;
}

export function openTileFolderPop(f, anchor) {
  if (tileFolderPopId === f.id) { closeTileFolderPop(); return; }
  tileFolderPop.innerHTML = "";
  tileFolderPopId = f.id;
  const head = document.createElement("div");
  head.className = "tile-folder-pop-head";
  const title = document.createElement("span");
  title.className = "tile-folder-pop-title";
  title.textContent = folderTitle(f);
  head.appendChild(title);
  head.appendChild(folderPopButton(ICON_EDIT, t("tileFolderRename"), () => { closeTileFolderPop(); renameTileFolder(f); }));
  head.appendChild(folderPopButton(CLOSE_ICON, t("close"), closeTileFolderPop));
  tileFolderPop.appendChild(head);
  const grid = document.createElement("div");
  grid.className = "tile-folder-grid";
  // Members are as big as a tile in the grid (the folder cell itself), shrunk only
  // as far as the window needs to fit the columns side by side.
  const members = folderMembers(f);
  const cell = anchor.getBoundingClientRect();
  const cols = Math.max(1, Math.min(TILE_FOLDER_COLS, members.length));
  const rows = Math.ceil(members.length / cols);
  const room = window.innerWidth * 0.96 - 18 - (cols - 1) * TILE_FOLDER_GAP;
  // The title row is 36px, the popover's padding and border 18px; the height budget keeps
  // every row inside the window so the popover never has to scroll.
  const roomH = window.innerHeight * 0.92 - 36 - 18 - (rows - 1) * TILE_FOLDER_GAP;
  const scale = Math.min(1, room / (cols * cell.width), roomH / (rows * cell.height));
  const memberW = Math.max(60, Math.floor(cell.width * scale));
  const memberH = Math.max(40, Math.floor(cell.height * scale));
  grid.style.gridTemplateColumns = `repeat(${cols}, ${memberW}px)`;
  grid.style.setProperty("--member-w", `${memberW}px`);
  grid.style.setProperty("--member-h", `${memberH}px`);
  for (const p of members) {
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
    // 1 = auto-fit, else a fixed size that fitTilesShared leaves alone).
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
  tileFolderBackdrop.classList.remove("hidden");
  const anchorRect = anchor.getBoundingClientRect();
  placePopover(tileFolderPop, anchorRect);
  // Beside the folder there is not always room for every row. The grid is dimmed behind
  // the popover anyway, so it may open over its own folder instead of scrolling.
  if (tileFolderPop.classList.contains("pop-clamped")) {
    placePopover(tileFolderPop, { left: anchorRect.left, right: anchorRect.left, top: anchorRect.top, bottom: anchorRect.top });
  }
  // Same text sizing rule as the grid: auto-fit when the grid auto-fits or the
  // prompt asks for it, the box being the member tile minus its border and padding.
  const globalAuto = Number(settings.tile_size) === 0;
  // The same air around the text as on a grid tile (fitAllTiles in tiles.js).
  const airX = 2 * Math.round(memberW * 0.04);
  const airY = memberH >= 80 ? 2 * 16 : 2 * 4;
  // One shared box for all members (they have the same size): the grid rule of
  // fitTilesShared then applies here too (one shared size, one line when it is cheap).
  const fitTiles = [];
  let boxW = Infinity;
  let boxH = Infinity;
  for (const btn of grid.children) {
    if (!globalAuto && btn.dataset.fitMode !== "auto") continue;
    const cs = getComputedStyle(btn);
    // clientWidth/Height already leave out the border; floor because scrollWidth/Height
    // are integers and a fractional box can fail its own <= check.
    boxW = Math.min(boxW, Math.floor(btn.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight) - airX));
    boxH = Math.min(boxH, Math.floor(btn.clientHeight - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom) - airY));
    fitTiles.push(btn);
  }
  if (fitTiles.length && boxW > 0 && boxH > 0) fitTilesShared(fitTiles, boxW, boxH);
}

export function disarmFolder() {
  if (!folderArm) return;
  clearTimeout(folderArm.timer);
  folderArm.el.classList.remove("folder-arm", "folder-arming");
  folderArm.el.style.removeProperty("--arm-ms");
  assignFolderArm(null);
}
export function armFolderDrop(e) {
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
  const wait = TILE_FOLDER_HOVER_MS;
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
export async function dropIntoFolder(id, host, cell) {
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
export function cacheFloatingState(id, pinned) {
  if (!settings.floating) settings.floating = {};
  if (pinned) settings.floating[id] = settings.floating[id] || { x: 0, y: 0 };
  else delete settings.floating[id];
}

export function wireGridFolders() {
  tileFolderBackdrop.className = "tile-folder-backdrop hidden";
  document.body.appendChild(tileFolderBackdrop);
  tileFolderPop.className = "ctx tile-folder-pop hidden";
  tileFolderPop.setAttribute("role", "menu");
  document.body.appendChild(tileFolderPop);
  window.addEventListener("pointermove", (e) => {
    if (!memberDrag) return;
    if (!memberDrag.moved) {
      if (Math.abs(e.clientX - memberDrag.startX) < DRAG_THRESHOLD &&
          Math.abs(e.clientY - memberDrag.startY) < DRAG_THRESHOLD) return;
      memberDrag.moved = true;
      tileFolderBackdrop.classList.add("member-dragging");
      tileFolderPop.classList.add("member-dragging");
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
      if (dialogTokens(p, await peekRotatedText(p)).length) {
        // Cooldown first, like a plain copy: a copy that just happened must not ask again.
        if (copyOnCooldown(d.id)) return;
        await copyTextWithVars(p, d.el);
        return;
      }
      if (autoPasteDouble(d.id)) { await autoPasteNow(d.el); return; } // 2nd fast click pastes
      if (copyOnCooldown(d.id)) return;
      if (await copyResolved(p)) {
        showCopied(d.el);
        recordCopy(d.id);
        maybeCloseOnCopy();
      }
      return;
    }
    // Looked up while the layers still let the pointer through, released right after.
    const cell = cellAt(e.clientX, e.clientY);
    tileFolderBackdrop.classList.remove("member-dragging");
    tileFolderPop.classList.remove("member-dragging");
    if (!cell) return;
    await takeOutOfFolder(d.id, [Number(cell.dataset.col), Number(cell.dataset.row)]);
  });
}
