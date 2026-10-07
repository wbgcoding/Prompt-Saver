// View tabs, the view modal, the grid-size number popup, the examples banner and the
// favourites view.
import { $, assignFavView, assignPrompts, assignSettings, cellKey, clampGrid, confirmIfNoUndo, disarmButton, drag, DRAG_THRESHOLD, errToast, favView, flag, gridEl, gridSelected, inkShadow, invoke, journalThumbCache, libraryEl, libThumbCache, openColorPop, optFlag, PLUS_ICON, prompts, pruneThumbCache, readableInk, settings, syncEdgeFade, t, TB_CHECK, toast, undoToast, val, viewsEl } from "./core.js";
import { placePopover, placePopoverAt } from "./popover.js";
import { EXAMPLE_IMAGE_DATA_URL } from "./exampleAssets.js";
import { buildTile, fitAllTiles, pruneVideoCache } from "./tiles.js";
import { hideTileTip } from "./windowChrome.js";
import { clearGridSelection, isArchived, renderLibrary } from "./library.js";
import { applyFlags, syncBannerStack } from "./expertMenu.js";
import { buildSwatches } from "./editor.js";
import { buildFolderTile, foldersOfView, saveFolder } from "./gridFolders.js";

// ---- Grid-size value picker ----
// Scrollable popup under a grid-size input: lists every value with a
// visible scrollbar, highlights and centers the current selection.
export const numPop = document.createElement("div");

export let popInput = null;
export let popApply = null;

export function renderNumPop() {
  const v = clampGrid(popInput.value, 1);
  numPop.innerHTML = "";
  let selected = null;
  for (let i = 1; i <= val("gridMax"); i++) {
    const row = document.createElement("button");
    row.type = "button";
    row.textContent = i;
    if (i === v) {
      row.className = "sel";
      selected = row;
    }
    row.addEventListener("pointerdown", (e) => {
      e.preventDefault(); // keep the input focused
      popInput.value = i;
      closeNumPop(true);
    });
    numPop.appendChild(row);
  }
  if (selected) selected.scrollIntoView({ block: "center" });
}

// UI scale now uses the WebView's NATIVE zoom (set from Rust, like Ctrl +/-), which
// keeps event coords and getBoundingClientRect in one coordinate space: so
// JS-positioned fixed popups need no compensation. Kept as a hook (always 1) so the
// existing `/ z` call sites stay no-ops instead of being ripped out everywhere.
export const uiZoom = () => 1;

// placePopover/placePopoverAt now live in ui/popover.js -- the one placement
// rule for every popover, dropdown and context menu in THIS window. See that
// file for why floating.js/folder.js and media.js's volume popup stay separate.

export function openNumPop(input, apply) {
  popInput = input;
  popApply = apply;
  // Fill first: the placement needs a laid-out list, and so does centering the
  // selected row.
  numPop.classList.remove("hidden");
  renderNumPop();
  placePopover(numPop, input.getBoundingClientRect());
}

export function closeNumPop(apply) {
  if (!popInput) return;
  numPop.classList.add("hidden");
  const done = popApply;
  popInput = null;
  popApply = null;
  if (apply && done) done();
}

// Combo behaviour: typing stays possible, focus opens the picker,
// wheel / arrow keys step through the values, blur or Enter applies.
export function attachGridPicker(input, apply) {
  input.addEventListener("focus", () => openNumPop(input, apply));
  // Re-open on click even when the input kept focus after a pick.
  // Apply a still-open sibling picker first (pointerdown fires before blur).
  input.addEventListener("pointerdown", () => {
    if (popInput === input) return;
    closeNumPop(true);
    openNumPop(input, apply);
  });
  input.addEventListener("input", () => { if (popInput === input) renderNumPop(); });
  input.addEventListener("blur", () => { if (popInput === input) closeNumPop(true); });
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") input.blur();
    else if (e.key === "Escape") closeNumPop(false);
  });
  input.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      input.value = clampGrid(Number(input.value) + (e.deltaY > 0 ? 1 : -1), 1);
      if (popInput === input) renderNumPop();
      else openNumPop(input, apply);
    },
    { passive: false }
  );
}

// ---- View helpers ----
export function activeView() {
  return settings.views.find((v) => v.id === settings.active_view) || settings.views[0];
}
export const gridKeyOf = (v) => `${v.cols}x${v.rows}`;
export const layoutOf = (v) => v.layouts?.[gridKeyOf(v)] || {};
// Every OTHER view currently holding this prompt: a client-side scan, no
// round trip, since every view's layout is already loaded in `settings`.
export const viewsContaining = (id, exceptViewId) =>
  settings.views.filter((v) => v.id !== exceptViewId && Object.prototype.hasOwnProperty.call(layoutOf(v), id));

export const favViewEnabled = () => flag("favorites") && optFlag("favViewButton");

// Favorites layout: prompt id -> [col,row]. Free placement (gaps allowed), saved
// in ui_texts; the grid grows to fit the count and to cover any hand-placed cell.
export function favLayoutMap() {
  try { const m = JSON.parse(settings.ui_texts?.favLayout || "{}"); return m && typeof m === "object" ? m : {}; } catch (_) { return {}; }
}
export function saveFavLayout(map) {
  const value = JSON.stringify(map);
  settings.ui_texts = { ...(settings.ui_texts || {}), favLayout: value };
  invoke("set_ui_text", { key: "favLayout", value }).catch(() => {});
}

// Star button visibility (setting + expert flag) + pressed state. In the favorites
// view the grid-size control stays visible (so the size is shown) but read-only:
// the favorites grid sizes itself.
export function refreshFavViewUi() {
  const btn = $("fav-view-btn");
  if (btn) {
    btn.classList.toggle("hidden", !favViewEnabled());
    btn.classList.toggle("active", favView);
  }
  const qg = $("quick-grid");
  if (qg) qg.classList.toggle("qg-readonly", favView);
  $("qg-cols").readOnly = favView;
  $("qg-rows").readOnly = favView;
}

export function setFavView(on) {
  if (gridSelected.size) clearGridSelection(); // a selection belongs to what's on screen
  assignFavView(on && favViewEnabled());
  renderGrid(true);
}

// Favorites grid with FREE placement inside a grid that always fits the count:
// a favorite keeps the cell it is dragged to (gaps allowed), but the grid resizes
// the moment the count changes, and any button now outside it is re-fitted.
export function renderFavGrid() {
  $("grid-empty").classList.add("hidden"); // this view has its own empty message
  const favs = prompts.filter((p) => p.favorite);
  const n = favs.length;
  const ids = new Set(favs.map((p) => p.id));
  const layout = favLayoutMap();
  let changed = false;
  for (const id of Object.keys(layout)) if (!ids.has(id)) { delete layout[id]; changed = true; }

  // Size fits the count (landscape); an expert cap can pin the column count.
  let cols = Math.max(1, Math.ceil(n / Math.max(1, Math.floor(Math.sqrt(n)))));
  const capCols = val("favMaxCols");
  if (capCols > 0) cols = Math.min(cols, capCols);
  let rows = Math.max(1, Math.ceil(n / cols));
  while (cols * rows < n) rows++; // always room for every favorite

  // Placed cells outside the (re)computed grid are dropped and re-placed below,
  // so shrinking the grid re-fits any button that fell off it.
  const occupied = new Map();
  for (const [id, cell] of Object.entries(layout)) {
    if (cell[0] < cols && cell[1] < rows && !occupied.has(cellKey(cell[0], cell[1]))) {
      occupied.set(cellKey(cell[0], cell[1]), id);
    } else { delete layout[id]; changed = true; }
  }
  // Newly starred favorites (not yet placed) drop into the first free cell.
  for (const p of favs) {
    if (layout[p.id]) continue;
    const cell = firstFree(occupied, cols, rows);
    if (cell) { layout[p.id] = cell; occupied.set(cellKey(...cell), p.id); changed = true; }
  }
  if (changed) saveFavLayout(layout);

  const byCell = new Map();
  for (const p of favs) { const c = layout[p.id]; if (c) byCell.set(cellKey(c[0], c[1]), p); }
  gridEl.style.gridTemplateColumns = `repeat(${cols}, 1fr)`;
  gridEl.style.gridTemplateRows = `repeat(${rows}, 1fr)`;
  $("qg-cols").value = cols; // show (read-only) the auto-computed favorites size
  $("qg-rows").value = rows;
  const frag = document.createDocumentFragment();
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const cellEl = document.createElement("div");
      cellEl.className = "cell";
      cellEl.dataset.col = c;
      cellEl.dataset.row = r;
      const p = byCell.get(cellKey(c, r));
      if (p) cellEl.appendChild(buildTile(p));
      frag.appendChild(cellEl);
    }
  }
  gridEl.innerHTML = "";
  gridEl.appendChild(frag);
  if (!n) {
    const empty = document.createElement("div");
    empty.className = "fav-empty";
    empty.textContent = t("favViewEmpty");
    gridEl.appendChild(empty);
  }
  pruneVideoCache();
  pruneThumbCache(libThumbCache);
  pruneThumbCache(journalThumbCache);
  renderViews();
  refreshFavViewUi();
  fitAllTiles();
}

// Move a favorite to the drop cell, swapping with any occupant. Dropping on an
// empty cell just moves it there and frees the old one: gaps are preserved.
export async function placeFav(id, col, row) {
  if (!flag("favViewReorder")) return;
  const layout = favLayoutMap();
  const ids = new Set(prompts.filter((p) => p.favorite).map((p) => p.id));
  for (const k of Object.keys(layout)) if (!ids.has(k)) delete layout[k];
  const old = layout[id];
  const occupant = Object.entries(layout).find(([oid, c]) => oid !== id && c[0] === col && c[1] === row);
  if (occupant) { if (old) layout[occupant[0]] = old; else delete layout[occupant[0]]; }
  layout[id] = [col, row];
  saveFavLayout(layout);
  await renderGrid(true);
}

// ---- Layout normalization (active view, current grid size) ----
export function firstFree(occupied, cols, rows) {
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      if (!occupied.has(cellKey(c, r))) return [c, r];
    }
  }
  return null;
}

export function normalizeLayout(view) {
  const { cols, rows } = view;
  const layout = { ...layoutOf(view) };
  const occupied = new Map();
  const ids = new Set(prompts.map((p) => p.id));
  let changed = false;

  for (const id of Object.keys(layout)) {
    if (!ids.has(id)) { delete layout[id]; changed = true; }
  }
  for (const p of prompts) {
    const cell = layout[p.id];
    if (cell && cell[0] < cols && cell[1] < rows && !occupied.has(cellKey(...cell))) {
      occupied.set(cellKey(...cell), p.id);
    } else if (cell) {
      delete layout[p.id];
      changed = true;
    }
  }
  // Unplaced prompts stay reachable via the library. No auto-fill: saved
  // per-grid-size arrangements must stay untouched.
  view.layouts[gridKeyOf(view)] = layout;
  return changed;
}

// ---- Render ----
// skipFetch: caller already updated the local state (drag/hide hot path).
export async function renderGrid(skipFetch = false) {
  hideTileTip(); // the hovered tile may be replaced by this render
  if (!skipFetch) {
    try {
      const s = await invoke("get_state");
      assignPrompts(s.prompts);
      assignSettings(s.settings);
      await updateExamplesBanner(); // re-evaluate on every real fetch, not just at startup
    } catch (e) {
      // IPC hiccup: keep the last good state instead of throwing into the
      // global handler and leaving a half-rendered grid.
      errToast(e);
      if (!settings || !prompts) return;
    }
  }
  if (favView && !favViewEnabled()) assignFavView(false); // setting switched off underneath us
  if (favView) { renderFavGrid(); return; }
  const view = activeView();
  // A colored border around the grid itself (expert opt, default off):
  // reuses the view's own color, kept current on every render so it never
  // shows a stale color after switching views.
  document.documentElement.style.setProperty("--active-view-color", view.color || "transparent");
  if (normalizeLayout(view)) {
    invoke("set_layout", { layout: layoutOf(view) }).catch(() => {});
  }

  const { cols, rows } = view;
  const layout = layoutOf(view);
  gridEl.style.gridTemplateColumns = `repeat(${cols}, 1fr)`;
  gridEl.style.gridTemplateRows = `repeat(${rows}, 1fr)`;
  $("qg-cols").value = cols;
  $("qg-rows").value = rows;
  $("qg-cols").max = val("gridMax");
  $("qg-rows").max = val("gridMax");

  // A prompt inside a grid folder is drawn by that folder, not by itself.
  const folders = foldersOfView(view.id);
  const inFolder = new Set(folders.flatMap((f) => f.members || []));
  const byCell = new Map();
  for (const p of prompts) {
    if (inFolder.has(p.id)) continue;
    if (flag("archiveTag") && isArchived(p)) continue;
    const cell = layout[p.id];
    if (cell) byCell.set(cellKey(...cell), p);
  }
  // A folder whose cell fell outside a resized grid (or landed on a taken one)
  // moves to the first free cell, and that repair is written back.
  const folderAt = new Map();
  for (const f of folders) {
    const [fc, fr] = f.cell || [0, 0];
    let cell = [fc, fr];
    const free = (c) => c[0] < cols && c[1] < rows &&
      !byCell.has(cellKey(c[0], c[1])) && !folderAt.has(cellKey(c[0], c[1]));
    if (!free(cell)) {
      const spot = firstFree(new Map([...byCell, ...folderAt]), cols, rows);
      if (!spot) continue; // no room at all: it reappears when the grid grows again
      cell = spot;
      saveFolder(f, { cell });
    }
    folderAt.set(cellKey(cell[0], cell[1]), f);
  }

  // Build off-DOM, attach once (1 reflow instead of cols*rows).
  const frag = document.createDocumentFragment();
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const cellEl = document.createElement("div");
      cellEl.className = "cell";
      cellEl.dataset.col = c;
      cellEl.dataset.row = r;
      const f = folderAt.get(cellKey(c, r));
      const p = byCell.get(cellKey(c, r));
      if (f) cellEl.appendChild(buildFolderTile(f));
      else if (p) cellEl.appendChild(buildTile(p));
      frag.appendChild(cellEl);
    }
  }
  gridEl.innerHTML = "";
  gridEl.appendChild(frag);
  // First run, nothing anywhere yet: a plain grid explains nothing on its own.
  $("grid-empty").classList.toggle("hidden", prompts.length > 0);

  // Members that belong to this view but have no cell at this grid size (grid
  // shrunk under them) still belong here: shown as a visible overhang instead
  // of a silent loss.
  const seated = new Set([...byCell.values()].map((p) => p.id));
  const overflowCount = view.members.filter((id) => !seated.has(id) && !inFolder.has(id)).length;
  $("grid-overflow").classList.toggle("hidden", overflowCount === 0);
  if (overflowCount > 0) {
    $("grid-overflow-text").textContent = t("viewOverflowTitle").replace("{n}", String(overflowCount)) + " " + t("viewOverflowBody");
  }
  syncBannerStack();

  pruneVideoCache();
  pruneThumbCache(libThumbCache);
  pruneThumbCache(journalThumbCache);
  renderViews();
  refreshFavViewUi();
  fitAllTiles();
}

// Header view switcher: one button per view (switch on click, manage on
// right-click) plus a trailing "+" to add a view via the name popup.
export function renderViews() {
  viewsEl.innerHTML = "";
  // Multi-view disabled (expert menu): no switcher, no add button.
  if (!flag("multiView")) {
    viewsEl.classList.add("hidden");
    return;
  }
  for (const v of settings.views) viewsEl.appendChild(viewTabEl(v));
  if (settings.views.length < val("maxViews")) {
    const add = document.createElement("button");
    add.className = "view-btn view-add-top";
    add.innerHTML = PLUS_ICON;
    add.setAttribute("aria-label", t("addView")); // aria only: no hover tooltip
    add.addEventListener("click", () => openViewModal(null));
    viewsEl.appendChild(add);
  }
  viewsEl.classList.toggle("hidden", viewsEl.childElementCount === 0);
  syncEdgeFade(viewsEl);
}

export async function switchToView(id) {
  assignFavView(false); // picking a normal view leaves the favorites grid
  if (gridSelected.size) clearGridSelection(); // a selection belongs to what's on screen
  try {
    assignSettings(await invoke("set_active_view", { id }));
  } catch (err) {
    errToast(err);
    return; // a rejection must not leave the click doing nothing silently
  }
  await renderGrid(true); // prompts unchanged, settings already fresh
}

export function viewTabEl(v) {
  const btn = document.createElement("button");
  // While the favorites view is open, no normal view is the active one.
  btn.className = "view-btn" + (!favView && v.id === settings.active_view ? " active" : "");
  btn.dataset.viewId = v.id;
  if (v.color) {
    btn.classList.add("colored");
    btn.style.setProperty("--view-color", v.color);
    btn.style.setProperty("--view-ink", readableInk(v.color));
    btn.style.setProperty("--ink-shadow", inkShadow(v.color));
  }
  // The name lives in its own span so the merge preview can shrink it back into the
  // tab the way a folder tab's spans already do.
  const label = document.createElement("span");
  label.textContent = v.name;
  btn.appendChild(label);
  btn.title = t("viewSwitchHint");
  btn.addEventListener("click", () => { if (!tabDragBlockClick) switchToView(v.id); });
  btn.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    const items = [
      { label: t("gridSizeMenu"), run: () => openGridSizeFor(v) },
      { label: t("editViewTitle"), run: () => openViewModal(v) },
      { label: t("duplicate"), run: () => duplicateView(v) },
      ...moveTabMenuItems(`v:${v.id}`),
    ];
    openMiniMenu(items, e.clientX, e.clientY);
  });
  makeTabDraggable(btn, v.id);
  return btn;
}

export const viewById = (id) => settings.views.find((v) => v.id === id);
// The order the tab bar shows the views in, and the order Ctrl+Tab walks.
export const viewOrder = () => settings.views;

// ---- Tab-bar order -- the only place that decides it is settings.views
// itself, so moving a tab is nothing but reordering that one array. A slot is
// one draggable unit of the bar: a view's own id.
export function topLevelSlots() {
  return settings.views.map((v) => ({ viewId: v.id, ids: [v.id] }));
}
export const slotKey = (s) => `v:${s.viewId}`;

export async function applyViewOrder(order) {
  try {
    assignSettings(await invoke("reorder_views", { order }));
  } catch (e) {
    errToast(e);
    return;
  }
  renderViews();
}

// Moves the slot `key` to sit right before/after the slot `targetKey` -- one
// operation for both the one-step menu move (target = the current neighbour)
// and an arbitrary drag drop (target = wherever the pointer landed).
export async function moveTopLevelSlot(key, targetKey, side) {
  const slots = topLevelSlots();
  const fromIdx = slots.findIndex((s) => slotKey(s) === key);
  if (fromIdx < 0) return;
  const [moved] = slots.splice(fromIdx, 1);
  let toIdx = slots.findIndex((s) => slotKey(s) === targetKey);
  if (toIdx < 0) return;
  if (side === "after") toIdx += 1;
  slots.splice(toIdx, 0, moved);
  await applyViewOrder(slots.flatMap((s) => s.ids));
}

// The "move further left/right" menu entries: swap with the immediate neighbour.
export async function stepTopLevelSlot(key, dir) {
  const slots = topLevelSlots();
  const idx = slots.findIndex((s) => slotKey(s) === key);
  const targetIdx = idx + dir;
  if (idx < 0 || targetIdx < 0 || targetIdx >= slots.length) return;
  await moveTopLevelSlot(key, slotKey(slots[targetIdx]), dir < 0 ? "before" : "after");
}

// The tab context menu's move entries -- greyed out (never hidden) at whichever
// end already has nothing further to move past.
export function moveTabMenuItems(key) {
  const slots = topLevelSlots();
  const idx = slots.findIndex((s) => slotKey(s) === key);
  return [
    { label: t("moveTabLeft"), disabled: idx <= 0, run: () => stepTopLevelSlot(key, -1) },
    { label: t("moveTabRight"), disabled: idx < 0 || idx >= slots.length - 1, run: () => stepTopLevelSlot(key, 1) },
  ];
}

// Builds the checkbox + optional colour dot + name row of a view in the "Also show
// in" popover.
export function viewRowBase(v, on) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "view-row" + (on ? " on" : "");
  const check = document.createElement("span");
  check.className = "tb-check";
  check.innerHTML = on ? TB_CHECK : "";
  const label = document.createElement("span");
  label.className = "view-row-name";
  label.textContent = v.name;
  if (v.color) {
    const dot = document.createElement("span");
    dot.className = "view-row-dot";
    dot.style.background = v.color;
    label.prepend(dot);
  }
  row.append(check, label);
  return row;
}

// Small shared right-click menu for view tabs and group tabs.
export const miniMenu = document.createElement("div");

export function closeMiniMenu() { miniMenu.classList.add("hidden"); }
export function openMiniMenu(items, x, y) {
  miniMenu.innerHTML = "";
  for (const it of items) {
    const b = document.createElement("button");
    b.type = "button";
    b.setAttribute("role", "menuitem");
    b.className = it.danger ? "danger" : "";
    // A move-left/-right entry stays visible at either end of the tab bar,
    // disabled rather than removed, so the menu does not reflow under the
    // pointer depending on which tab it was opened on.
    if (it.disabled) {
      b.disabled = true;
      b.setAttribute("aria-disabled", "true");
    }
    // The shared folder glyph on a "group into a folder" / "ungroup" entry --
    // same markup wherever one of the three folder kinds offers it.
    if (it.icon) {
      b.classList.add("has-icon");
      const ic = document.createElement("span");
      ic.className = "menu-icon";
      ic.innerHTML = it.icon;
      b.appendChild(ic);
    }
    const lbl = document.createElement("span");
    lbl.textContent = it.label;
    b.appendChild(lbl);
    b.addEventListener("click", () => { if (it.disabled) return; closeMiniMenu(); it.run(); });
    miniMenu.appendChild(b);
  }
  placePopoverAt(miniMenu, x, y);
}

// Dragging a tab onto another one reorders them. Pointer events rather than
// HTML5 drag-and-drop: the webview reserves that for file drops.
export let tabDrag = null;
export let tabDragBlockClick = false;
export function assignTabDragBlockClick(v) { tabDragBlockClick = v; }

export function makeTabDraggable(el, viewId) {
  el.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || !flag("multiView")) return;
    tabDrag = { viewId, el, startX: e.clientX, startY: e.clientY, moved: false, target: null, zone: null };
  });
}

// A tab-bar button: the same hit test a tab-to-tab drag (below) and a tile-to-tab
// drag (further down) both call, so hovering either looks and behaves identically.
export const tabDropTarget = (x, y) => document.elementFromPoint(x, y)?.closest(".view-btn") || null;

// Which half of `el` the pointer sits over: the left half drops before the tab,
// the right half after it.
export function tabDropZone(el, x) {
  const rect = el.getBoundingClientRect();
  if (!rect.width) return "after";
  return x - rect.left < rect.width / 2 ? "before" : "after";
}

// Takes the element explicitly rather than reading it off `tabDrag` -- a
// pointerup already cleared `tabDrag` to null by the time it cleans up here,
// so reading through the (now gone) drag state would throw.
export function clearTabDropMark(el) {
  el?.classList.remove("tab-drop", "tab-drop-edge", "tab-drop-before", "tab-drop-after");
}

export function markTabDrop(el, zone) {
  if (tabDrag.target === el && tabDrag.zone === zone) return;
  clearTabDropMark(tabDrag.target);
  tabDrag.target = el;
  tabDrag.zone = el ? zone : null;
  if (!el) return;
  el.classList.add("tab-drop-edge", zone === "before" ? "tab-drop-before" : "tab-drop-after");
}

// ---- Dragging a grid tile onto a view tab -----------------------------
// The tile's own drag state (`drag`, defined with the grid below) grows a
// second target next to `tabDrag` above rather than sharing its state: the two
// gestures start on different elements (a tile vs. a tab) and are never live
// together, but a tab-to-tab drag must keep working exactly as before.
export function clearViewDropHover() {
  if (!drag) return;
  drag.viewTarget?.classList.remove("tab-drop");
  if (drag.viewTarget) drag.viewTarget.title = drag.viewTargetTitle ?? drag.viewTarget.title;
  drag.viewTarget = null;
}

// Resolves a drop target element to the view it means.
export const resolveViewDropTarget = (el) => (el?.dataset.viewId ? viewById(el.dataset.viewId) : null);

export function updateViewDropHover(x, y, copy) {
  const hit = tabDropTarget(x, y);
  if (drag.viewTarget !== hit) {
    if (drag.viewTarget) {
      drag.viewTarget.classList.remove("tab-drop");
      drag.viewTarget.title = drag.viewTargetTitle ?? drag.viewTarget.title;
    }
    drag.viewTarget = hit;
    if (hit) {
      drag.viewTargetTitle = hit.title;
      hit.title = t("viewDropMoveHint"); // names the Ctrl-to-copy key while hovered
      hit.classList.add("tab-drop");
    }
  }
  drag.ghost?.classList.toggle("will-copy", !!copy && !!hit);
}

// Serialises view-tab drops: two dragged onto a tab back to back used to both
// reach the backend, the slower one landing after the faster one already
// rendered, so its stale source view rode along and left the tile in both
// views. While one drop's round trip is still open, a second is dropped here
// instead of sent -- the tile it was carrying simply stays where it was,
// nothing to undo.
export let viewDropBusy = false;
// Bumped once per accepted drop; a response is only applied while it still
// carries the newest number, so a call whose answer arrives out of order can
// never overwrite whatever a later one already put on screen.
export let viewMutationSeq = 0;

// One invoke moves or copies the dragged id(s) into `targetViewId`, then
// re-renders whatever the source view's grid shows now. A rejected drop (the
// target is full) leaves both views untouched -- batch_prompts is all-or-nothing.
// Split from dropTilesOnView so the UI test suite can call it directly with two
// overlapping, independently-resolved invocations and prove the ordering rule
// above without racing real timers.
export async function runViewMutation(ids, targetViewId, fromViewId, copy, mySeq) {
  let result;
  try {
    result = await invoke("batch_prompts", {
      ids, action: copy ? "addView" : "moveView",
      color: null, favorite: null, viewId: targetViewId, fromViewId,
    });
  } catch (e) {
    if (mySeq !== viewMutationSeq) return; // superseded meanwhile -- its own toast would be stale too
    const m = String(e).match(/view-full:(\d+)/);
    if (m) toast(t("viewDropFull").replace("{name}", viewById(targetViewId)?.name || "").replace("{n}", m[1]));
    else errToast(e);
    return;
  }
  if (mySeq !== viewMutationSeq) return; // a later drop already applied; this answer is stale
  assignSettings(result);
  clearGridSelection();
  await renderGrid(true);
}

export async function dropTilesOnView(ids, targetViewId, fromViewId, copy) {
  if (viewDropBusy) return;
  viewDropBusy = true;
  try {
    await runViewMutation(ids, targetViewId, fromViewId, copy, ++viewMutationSeq);
  } finally {
    viewDropBusy = false;
  }
}

// ---- View add / rename / delete popup ----
export const viewModal = {
  root: $("view-modal"),
  title: $("view-modal-title"),
  name: $("view-modal-name"),
  colorRow: $("view-color-row"),
  gridField: $("view-modal-grid-field"),
  cols: $("view-modal-cols"),
  rows: $("view-modal-rows"),
  confirm: $("view-modal-confirm"),
  delete: $("view-modal-delete"),
  close: $("view-modal-close"),
};
export let viewModalId = null; // null = create, otherwise the view being edited
export let viewModalColor = ""; // empty = default

export function renderViewColor() {
  buildSwatches(
    viewModal.colorRow,
    viewModalColor,
    (hex) => { viewModalColor = hex; renderViewColor(); },
    (anchor, cur) => openColorPop(anchor, cur, (hex) => { viewModalColor = hex; renderViewColor(); })
  );
}

export function openViewModal(view) {
  viewModalId = view ? view.id : null;
  viewModalColor = view ? (view.color || "") : "";
  // The add-view label carries a leading "+"; the popup title drops it.
  viewModal.title.textContent = view ? t("editViewTitle") : t("addView").replace(/^\+\s*/, "");
  viewModal.name.value = view ? view.name : "";
  renderViewColor();
  // Grid size (columns × rows) is editable only for an existing view: a new one
  // starts at the default and can be resized here afterwards.
  viewModal.gridField.classList.toggle("hidden", !view);
  if (view) {
    const gm = val("gridMax");
    viewModal.cols.max = gm; viewModal.rows.max = gm;
    viewModal.cols.value = String(view.cols);
    viewModal.rows.value = String(view.rows);
  }
  // Delete only when editing and at least two views remain.
  const canDelete = !!view && settings.views.length > 1;
  viewModal.delete.classList.toggle("hidden", !canDelete);
  disarmButton(viewModal.delete, t("delete"));
  viewModal.root.classList.remove("hidden");
  viewModal.name.focus();
  viewModal.name.select();
}

// Entry point for the grid size: the view dialog with the columns field ready.
export function openGridSizeFor(view) {
  if (!view) return;
  openViewModal(view);
  viewModal.cols.focus();
  viewModal.cols.select();
}

export function closeViewModal() {
  viewModal.root.classList.add("hidden");
  viewModalId = null;
}

// Undo for a deleted view: rebuilt through the normal view commands (a new id,
// never the old one) rather than writing the View struct back whole, so members
// and layouts stay in lockstep the same way any other view mutation keeps them.
// The per-size layout arrangement itself is not restored, only membership and the
// current grid size: the members reflow into free cells, same as a resize would.
export async function restoreDeletedView(deleted, reactivateId) {
  assignSettings(await invoke("add_view", { name: deleted.name, color: deleted.color || "" }));
  const newId = settings.active_view;
  assignSettings(await invoke("set_view_grid", { id: newId, cols: deleted.cols, rows: deleted.rows }));
  if (deleted.members && deleted.members.length) {
    assignSettings(await invoke("batch_prompts", { ids: deleted.members, action: "addView", color: null, favorite: null, viewId: newId }));
  }
  if (reactivateId && settings.views.some((v) => v.id === reactivateId)) {
    assignSettings(await invoke("set_active_view", { id: reactivateId }));
  }
  renderViews();
  await renderGrid(true);
  if (!libraryEl.classList.contains("hidden")) renderLibrary();
}

export async function confirmViewModal() {
  const name = viewModal.name.value.trim();
  if (!name) { viewModal.name.focus(); return; }
  try {
    if (viewModalId) {
      await invoke("rename_view", { id: viewModalId, name });
      await invoke("set_view_color", { id: viewModalId, color: viewModalColor });
      // Apply the grid size too (clamped to the expert gridMax).
      const view = settings.views.find((v) => v.id === viewModalId);
      const cols = clampGrid(viewModal.cols.value, view?.cols ?? 1);
      const rows = clampGrid(viewModal.rows.value, view?.rows ?? 1);
      assignSettings(await invoke("set_view_grid", { id: viewModalId, cols, rows }));
    } else {
      assignSettings(await invoke("add_view", { name, color: viewModalColor }));
    }
  } catch (err) {
    errToast(err);
    return;
  }
  closeViewModal();
  renderViews();
  await renderGrid(true);
}

// A new, empty view with the same name/color/grid size as the source: its
// tile placements are not copied, since a prompt cannot yet sit in two views at
// once.
export async function duplicateView(v) {
  try {
    assignSettings(await invoke("add_view", { name: `${v.name} (${t("duplicateSuffix")})`, color: v.color || "" }));
    const newId = settings.active_view; // add_view switches to the view it just created
    assignSettings(await invoke("set_view_grid", { id: newId, cols: v.cols, rows: v.rows }));
  } catch (err) {
    errToast(err);
    return;
  }
  renderViews();
  await renderGrid(true);
}

export function bindViews() {
  // Quick grid-size control (top-right of the layout, active view).
  const applyQuickGrid = async () => {
    if (favView) return; // favorites grid sizes itself; the control is read-only there
    const view = activeView();
    const cols = clampGrid($("qg-cols").value, view.cols);
    const rows = clampGrid($("qg-rows").value, view.rows);
    if (cols === view.cols && rows === view.rows) return;
    assignSettings(await invoke("set_view_grid", { id: view.id, cols, rows }));
    await renderGrid(true);
  };
  attachGridPicker($("qg-cols"), applyQuickGrid);
  attachGridPicker($("qg-rows"), applyQuickGrid);

  // Overhang row: grow the grid one step at a time, capped at
  // gridMax, until the view has enough cells for every member (or it hits the
  // cap - the row then just stays, since nothing was lost either way).
  $("grid-overflow-grow").addEventListener("click", async () => {
    if (favView) return;
    const view = activeView();
    const gm = val("gridMax");
    let cols = view.cols;
    let rows = view.rows;
    while (cols * rows < view.members.length && (cols < gm || rows < gm)) {
      if (cols <= rows && cols < gm) cols++;
      else if (rows < gm) rows++;
      else break;
    }
    if (cols === view.cols && rows === view.rows) return;
    assignSettings(await invoke("set_view_grid", { id: view.id, cols, rows }));
    await renderGrid(true);
  });

  // View add / rename / delete popup.
  viewModal.confirm.addEventListener("click", confirmViewModal);
  // Grid size uses the same dropdown number picker as the main layout; values are
  // applied on Save, so the pick callback is a no-op here.
  attachGridPicker(viewModal.cols, () => {});
  attachGridPicker(viewModal.rows, () => {});
  viewModal.name.addEventListener("keydown", (e) => {
    if (e.key === "Enter") confirmViewModal();
  });
  viewModal.close.addEventListener("click", closeViewModal);
  viewModal.root.addEventListener("pointerdown", (e) => {
    if (e.target === viewModal.root) closeViewModal();
  });
  // Deletes at once; the Undo toast brings the view back.
  viewModal.delete.addEventListener("click", async () => {
    // Snapshot before the delete call: `settings` is only replaced once it
    // resolves, so the pre-delete view is still sitting in the current list.
    const deleted = settings.views.find((v) => v.id === viewModalId);
    const wasActive = settings.active_view === viewModalId;
    const prevActive = settings.active_view;
    if (deleted) {
      const sure = await confirmIfNoUndo({
        title: t("viewDeleteTitle"),
        message: t("viewDeleteMsg").replace("{name}", deleted.name),
        confirmLabel: t("delete"),
      });
      if (!sure) return;
    }
    try {
      assignSettings(await invoke("delete_view", { id: viewModalId }));
    } catch (err) {
      errToast(err);
      return;
    }
    closeViewModal();
    renderViews();
    await renderGrid(true);
    if (deleted) {
      undoToast(t("viewDeletedToast").replace("{name}", deleted.name), () => restoreDeletedView(deleted, wasActive ? null : prevActive));
    }
  });

  // Favorites view: setting enables the header star; the star toggles the grid.
  $("opt-favview").addEventListener("change", (e) => {
    const on = e.target.checked;
    settings.ui_flags = { ...(settings.ui_flags || {}), favViewButton: on };
    invoke("set_ui_flag", { key: "favViewButton", enabled: on }).catch((err) => errToast(err));
    if (!on && favView) assignFavView(false);
    applyFlags();
    refreshFavViewUi();
    renderGrid(true);
  });
  $("fav-view-btn").addEventListener("click", () => setFavView(!favView));
}

// Three onboarding examples: placeholder, image, Markdown. Text comes from i18n.js
// (LANG is already resolved by the time this runs, see applySettingsCore), so each
// install sees them in its own language instead of freezing whatever the OS locale
// was on first launch would have to.
export const EXAMPLES = [
  { name: "example1Name", text: "example1Text", image: "" },
  { name: "example2Name", text: "example2Text", image: EXAMPLE_IMAGE_DATA_URL },
  { name: "example3Name", text: "example3Text", image: "" },
];

// The backend re-checks "library still empty AND never seeded before" itself, so
// this is safe (and cheap) to call unconditionally on every launch: it is a no-op
// the moment the library holds anything at all, examples included.
// Returns the prompts it actually seeded (empty when the library already had
// something, examples included: the backend decides, this only reports back).
export async function seedExamplesIfNeeded() {
  const examples = EXAMPLES.map((e) => ({ name: t(e.name), text: t(e.text), image: e.image }));
  return invoke("seed_example_prompts", { examples }).catch(() => []);
}

export async function updateExamplesBanner() {
  const banner = $("examples-banner");
  if (!banner) return;
  const ids = settings.example_prompt_ids || [];
  const stillHere = ids.length > 0 && ids.some((id) => prompts.some((p) => p.id === id));
  if (!stillHere) {
    banner.classList.add("hidden");
    syncBannerStack();
    return;
  }
  const pending = await invoke("example_prompts_pending").catch(() => ids.length);
  banner.classList.toggle("hidden", pending === 0);
  $("examples-dismiss").textContent = t("examplesDismiss").replace("{n}", String(pending));
  syncBannerStack();
}

export function wireNumPop() {
  numPop.className = "num-pop hidden";
  document.body.appendChild(numPop);
}

export function wireViews() {
  // A narrow window scrolls the view tabs sideways; a soft fade marks the hidden side.
  viewsEl.addEventListener("scroll", () => syncEdgeFade(viewsEl), { passive: true });
  if (typeof ResizeObserver === "function") new ResizeObserver(() => syncEdgeFade(viewsEl)).observe(viewsEl);
  miniMenu.className = "ctx tab-menu hidden";
  miniMenu.setAttribute("role", "menu");
  document.body.appendChild(miniMenu);
  // Exercised directly by the UI test suite to prove the sequence-number rule
  // without needing two real drags to race in wall-clock time.
  window.__runViewMutation = runViewMutation;
  window.__nextViewMutationSeq = () => ++viewMutationSeq;
  window.addEventListener("pointermove", (e) => {
    if (!tabDrag) return;
    if (!tabDrag.moved) {
      if (Math.abs(e.clientX - tabDrag.startX) < DRAG_THRESHOLD &&
          Math.abs(e.clientY - tabDrag.startY) < DRAG_THRESHOLD) return;
      tabDrag.moved = true;
      tabDragBlockClick = true; // the click that follows the drop must not switch views
      tabDrag.el.classList.add("tab-dragging");
    }
    const hit = tabDropTarget(e.clientX, e.clientY);
    const valid = hit && hit !== tabDrag.el && !hit.classList.contains("view-add-top") ? hit : null;
    if (!valid) { markTabDrop(null); return; }
    markTabDrop(valid, tabDropZone(valid, e.clientX));
  });
  window.addEventListener("pointerup", async (e) => {
    if (!tabDrag) return;
    const d = tabDrag;
    tabDrag = null;
    d.el.classList.remove("tab-dragging");
    clearTabDropMark(d.target);
    if (!d.moved) return;
    const target = d.target;
    if (target && d.zone && target.classList.contains("view-btn") && target.dataset.viewId && target.dataset.viewId !== d.viewId) {
      await moveTopLevelSlot(`v:${d.viewId}`, `v:${target.dataset.viewId}`, d.zone);
    }
  });
}
