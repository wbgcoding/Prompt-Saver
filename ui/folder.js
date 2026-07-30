// F25 pill folder window. The collapsed icon shows up to four member previews plus a
// count; clicking it grows THIS window into a grid of the member pills. There is only
// ever one window per folder — expanding resizes it, so z-order and focus stay as
// simple as a single pill's. All geometry (side, edge flipping, DPI) is decided in the
// backend, which is the only place that knows the monitors.
import { FONTS, fitText } from "./fonts.js";
import { applyFloatTheme } from "./floattheme.js";
import { mediaKind } from "./media.js";

const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

const appWin = getCurrentWebviewWindow();
const folderId = appWin.label.replace(/^folder-/, "");

const iconEl = document.getElementById("icon");
const previewEl = document.getElementById("preview");
const badgeEl = document.getElementById("badge");
const nameEl = document.getElementById("fname");
const popEl = document.getElementById("pop");
const gridEl = document.getElementById("grid");
const menuEl = document.getElementById("menu");
const memberMenu = document.getElementById("member-menu");

// Keep FOLDER_BOX in sync with the backend constant of the same name.
const FOLDER_BOX = 96; // collapsed icon (square) at scale 1
const TILE = 72; // one member tile in the open grid
const GAP = 8; // gap between tiles
const PAD = 10; // padding inside the popover
const POP_GAP = 6; // gap between icon and popover
const PREVIEW_MAX = 4; // mini previews on the closed folder (2x2)
const COPIED_MS = 900;
const DRAG_THRESHOLD = 4;
const MENU_W = 230; // matches .menu in floating.css

let state = { id: folderId, name: "", scale: 1, cols: 3, members: [] };
let open = null; // "grid" | "menu" | null
// Every open/close bumps this. A round trip that finishes after a newer one started
// drops its result instead of resizing the window behind the current state.
let seq = 0;
let TXT = { copied: "Copied!", size: "Size", edit: "Edit prompt", close: "Close", rename: "Rename", dissolve: "Dissolve folder", takeOut: "Take out of folder" };

// Copy behaviour, mirrored from the pill window.
let cooldownMs = 2000;
let apOn = false, apEnter = false, apDelay = 80, apDblMs = 400;
const lastCopy = {};
const lastClick = {};
let animMs = 150;
// F22 drag-out: the modifier hands a member to the OS instead of pulling it out of
// the folder, so the two gestures never fight over the same drag.
const DRAG_OUT_MODS = { ctrl: "ctrlKey", alt: "altKey", shift: "shiftKey" };
let dragOutOn = false, dragOutMod = "ctrlKey";

const member = (id) => state.members.find((p) => p.id === id);
const px = (v) => `${Math.round(v)}px`;

// ---- rendering ----

// Picture for a preview/grid tile: the stored still, else a gif/image attachment.
// A video falls back to the colour + name face (a decoder per mini tile would cost
// far more than it shows).
function faceSrc(p) {
  if (!p.show_image) return "";
  if (p.image) return p.image;
  const path = p.icon_path || p.file_path;
  const kind = path ? mediaKind(path) : "";
  return kind === "image" || kind === "gif" ? convertFileSrc(path) : "";
}

function paintFace(el, p, withName) {
  const src = faceSrc(p);
  el.style.background = src ? "transparent" : p.color || "";
  el.innerHTML = "";
  if (src) {
    const img = document.createElement("img");
    img.className = "face-img";
    img.alt = "";
    img.draggable = false;
    img.src = src;
    el.appendChild(img);
    if (p.color) el.style.boxShadow = `inset 0 0 0 2px ${p.color}`;
  } else {
    el.style.boxShadow = "";
    const label = document.createElement("span");
    label.className = "face-text";
    label.textContent = withName ? p.name : (p.name || "").slice(0, 2);
    el.appendChild(label);
  }
}

// Ring thickness, in proportion to the icon — the same reasoning as the pill's:
// a fixed band is a hairline on a large folder and swallows a small one whole.
const RING_MIN = 8;
const RING_MAX = 60;

function renderIcon() {
  const box = FOLDER_BOX * state.scale;
  document.body.style.setProperty("--folder-box", px(box));
  const ring = Math.round(Math.min(RING_MAX, Math.max(RING_MIN, box * 0.14)));
  document.getElementById("resize")?.style.setProperty("--rz", `${ring}px`);
  previewEl.innerHTML = "";
  for (const p of state.members.slice(0, PREVIEW_MAX)) {
    const cell = document.createElement("span");
    cell.className = "folder-face";
    paintFace(cell, p, false);
    previewEl.appendChild(cell);
  }
  badgeEl.textContent = String(state.members.length);
  nameEl.textContent = state.name;
  nameEl.classList.toggle("hidden", !state.name);
  // The count needs saying once: a bare number on a folder explains nothing. And the
  // 2x2 preview only identifies IMAGE members — for text pills it is four coloured
  // squares, so the tooltip names the first two and counts the rest.
  const many = TXT.members ? TXT.members.replace("{n}", state.members.length) : "";
  const names = state.members.map((p) => p.name).filter(Boolean);
  const shown = names.slice(0, 2).join(", ");
  const rest = names.length - 2;
  const who = shown && rest > 0 ? `${shown} +${rest}` : shown;
  iconEl.title = [state.name, who, many].filter(Boolean).join(" — ");
  badgeEl.title = many;
}

// Popover size for the open grid, from the column count and the folder's own scale.
function gridSize() {
  const n = Math.max(1, state.members.length);
  const cols = Math.max(1, Math.min(state.cols, n));
  const rows = Math.ceil(n / cols);
  const t = TILE * state.scale, g = GAP * state.scale, p = PAD * state.scale;
  return { w: cols * t + (cols - 1) * g + 2 * p, h: rows * t + (rows - 1) * g + 2 * p, cols };
}

function renderGrid() {
  const { cols } = gridSize();
  const t = TILE * state.scale;
  gridEl.style.gridTemplateColumns = `repeat(${cols}, ${px(t)})`;
  gridEl.style.gap = px(GAP * state.scale);
  gridEl.style.padding = px(PAD * state.scale);
  gridEl.innerHTML = "";
  for (const p of state.members) {
    const tile = document.createElement("button");
    tile.type = "button";
    tile.className = "folder-member";
    tile.dataset.id = p.id;
    tile.title = p.name;
    tile.style.width = px(t);
    tile.style.height = px(t);
    const face = document.createElement("span");
    face.className = "member-face";
    paintFace(face, p, true);
    tile.appendChild(face);
    const ok = document.createElement("span");
    ok.className = "member-ok";
    ok.textContent = TXT.copied;
    tile.appendChild(ok);
    gridEl.appendChild(tile);
  }
  // The tiles grow with the folder size, so the names have to as well — a fixed
  // small font left a large folder full of unreadable labels. Same auto-fit pass
  // the grid tiles and the pills use.
  const cap = Math.max(9, Math.round(t / 3));
  for (const el of gridEl.querySelectorAll(".member-face .face-text")) {
    fitText(el, t * 0.86, t * 0.86, cap);
  }
  for (const el of gridEl.querySelectorAll(".member-ok")) {
    fitText(el, t * 0.8, t * 0.5, cap);
  }
}

// ---- expand / collapse (one window, resized) ----

function openEl() {
  return open === "menu" ? menuEl : popEl;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Size of the popover this window has to grow around, in logical pixels.
function popoverSize(kind) {
  // The menu has a fixed width; its height only exists once it is laid out.
  return kind === "menu" ? { w: MENU_W, h: menuEl.offsetHeight || 120 } : gridSize();
}

// Ask the backend for the window box and place icon + popover inside it.
async function layout(kind, el, token) {
  const size = popoverSize(kind);
  const geom = await invoke("expand_float_folder", {
    id: folderId,
    width: size.w,
    height: size.h,
    gap: POP_GAP * state.scale,
  });
  if (token !== seq) return false; // superseded while the call was in flight
  document.body.classList.add("pop-open");
  document.body.dataset.side = geom.side;
  iconEl.style.left = px(geom.icon_x);
  iconEl.style.top = px(geom.icon_y);
  el.style.left = px(geom.pop_x);
  el.style.top = px(geom.pop_y);
  el.style.width = px(size.w);
  if (kind === "grid") el.style.height = px(size.h);
  return true;
}

// Grow the window around the icon; the icon itself never moves on screen.
async function openPopover(kind) {
  if (open === kind) return;
  await closePopover(true); // instant: no fade between two popovers
  const token = ++seq;
  const el = kind === "menu" ? menuEl : popEl;
  if (kind === "grid") renderGrid();
  el.classList.remove("hidden");
  try {
    if (!(await layout(kind, el, token))) return;
    open = kind;
    // Let the browser see the start state before the transition runs.
    requestAnimationFrame(() => el.classList.add("shown"));
  } catch (_) {
    el.classList.add("hidden");
  }
}

// Shrink back to the icon. The fade plays first, so the window never snaps away from
// a still fully painted grid; `instant` skips it (a window drag must not wait).
async function closePopover(instant) {
  if (!open) return;
  const el = openEl();
  const kind = open;
  open = null;
  const token = ++seq;
  memberMenu.classList.add("hidden");
  el.classList.remove("shown");
  if (!instant && animMs > 0) await sleep(animMs);
  if (token !== seq) return;
  el.classList.add("hidden");
  if (kind === "grid") el.style.height = "";
  document.body.classList.remove("pop-open");
  delete document.body.dataset.side;
  iconEl.style.left = "0px";
  iconEl.style.top = "0px";
  await invoke("collapse_float_folder", { id: folderId }).catch(() => {});
}

// Same popover, recomputed — after a member left, and when the monitor's scale factor
// changes (the known DPI trap: the physical box it was computed from is stale).
async function reflow() {
  if (!open) return;
  const kind = open;
  const el = openEl();
  if (kind === "grid") renderGrid();
  await layout(kind, el, ++seq).catch(() => {});
}

// ---- copying a member (same path as a pill: copy_prompt + record_copy) ----

function flash(id) {
  const tile = gridEl.querySelector(`.folder-member[data-id="${CSS.escape(id)}"]`);
  if (!tile) return;
  tile.classList.add("copied");
  setTimeout(() => tile.classList.remove("copied"), COPIED_MS);
}

async function copyMember(id) {
  const now = Date.now();
  // F19: a fast second click pastes the just-copied text into the last app.
  if (apOn && lastClick[id] && now - lastClick[id] <= apDblMs) {
    lastClick[id] = 0;
    invoke("paste_into_previous", { delayMs: apDelay, enter: apEnter }).catch(() => {});
    return;
  }
  if (cooldownMs && lastCopy[id] && now - lastCopy[id] < cooldownMs) return;
  if (await invoke("copy_prompt", { id }).catch(() => false)) {
    lastCopy[id] = Date.now();
    lastClick[id] = Date.now();
    flash(id);
    invoke("record_copy", { id }).catch(() => {});
  }
}

// ---- state + settings ----

async function loadState() {
  const data = await invoke("float_folder_state", { id: folderId }).catch(() => null);
  if (!data) return;
  state = data;
  renderIcon();
  // The stored scale arrives here, not through applyPrefs, and both run in
  // parallel — without this the size menu keeps whatever applySettings marked.
  markSelectedSize();
  if (open === "grid") await reflow();
}

// Same split as the pill: the lazily imported translations must never hold up the
// settings that drive behaviour.
async function applySettings() {
  const s = await invoke("get_settings");
  applyPrefs(s);
  await applyTexts(s);
}

async function applyTexts(s) {
  const i18n = await import("./i18n.js");
  const pref = (s.language && s.language !== "auto" ? s.language : navigator.language || "en").toLowerCase();
  const lang = Object.keys(i18n.FLOAT_TXT).find((c) => pref.startsWith(c)) || "en";
  const { applyTerms } = await import("./terms.js");
  TXT = Object.fromEntries(
    Object.entries(i18n.FLOAT_TXT[lang]).map(([k, v]) => [k, applyTerms(v, lang)]));
  document.getElementById("size-label").textContent = TXT.size;
  document.getElementById("menu-rename").textContent = TXT.rename;
  document.getElementById("menu-dissolve").textContent = TXT.dissolve;
  document.getElementById("mm-edit").textContent = TXT.edit;
  document.getElementById("mm-out").textContent = TXT.takeOut;
  const closeBtn = document.getElementById("menu-close");
  closeBtn.title = TXT.close;
  closeBtn.setAttribute("aria-label", TXT.close);
  for (const b of menuEl.querySelectorAll(".size-row [data-scale]")) {
    b.title = `${TXT.size} ${b.textContent}`;
    b.setAttribute("aria-label", b.title);
  }
}

function applyPrefs(s) {
  document.body.style.fontFamily = FONTS[s.tile_font] || FONTS.system;
  const cd = Number(s.ui_values?.copyCooldownMs);
  cooldownMs = Number.isFinite(cd) ? cd : 2000;
  apOn = s.ui_flags?.autoPaste !== false; // default on
  apEnter = s.ui_flags?.autoPasteEnter === true;
  const apd = Number(s.ui_values?.autoPasteDelayMs);
  apDelay = Number.isFinite(apd) ? apd : 80;
  const adb = Number(s.ui_values?.dblClickMs);
  apDblMs = Number.isFinite(adb) ? adb : 400;
  dragOutOn = s.ui_flags?.dragOut === true;
  dragOutMod = DRAG_OUT_MODS[s.ui_texts?.dragOutModifier] || "ctrlKey";
  // The folder animation reuses the global animation setting.
  const speed = Number(s.ui_values?.animSpeed);
  animMs = s.ui_flags?.animations === false ? 0 : Number.isFinite(speed) ? Math.max(0, speed) : 150;
  document.documentElement.style.setProperty("--transition", `${animMs}ms cubic-bezier(0.4, 0, 0.2, 1)`);
  const op = Number(s.ui_values?.floatOpacity);
  iconEl.style.opacity = Number.isFinite(op) ? `${Math.max(0.2, op / 100)}` : "";
  markSelectedSize();
  if (open === "grid") renderGrid();
}

function markSelectedSize() {
  for (const btn of menuEl.querySelectorAll("[data-scale]")) {
    btn.classList.toggle("sel", Math.abs(Number(btn.dataset.scale) - state.scale) < 0.01);
  }
}

// ---- folder icon: click opens, press-and-move drags the window ----

let iconDrag = null;

// Exactly the pill's mechanic: the whole window is the drag surface (the popover and
// the menus handle their own presses), and startDragging() hands the window to the OS
// move loop, which CONSUMES the release — so the drag also ends on the first later
// event that reports no button held.
window.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  if (e.target.closest(".menu") || e.target.closest(".folder-pop")) return;
  // The edge ring resizes instead of moving/opening — check it FIRST, or the icon's
  // own press would swallow the gesture. rz-hole covers the centre, so only a press
  // on the edge lands here.
  if (e.target.closest("#rz-catch")) {
    gripDrag = { x: e.screenX, y: e.screenY, scale: state.scale };
    document.body.classList.add("resizing");
    e.preventDefault();
    return;
  }
  iconDrag = { x: e.screenX, y: e.screenY, moved: false };
});

// ---- Free resize (edge ring) ----
// The folder's size IS its scale, so the drag maps the diagonal the pointer travelled
// onto that one number and writes it through the same command the S/M/L presets use.
// The backend clamps it (0.5-3.0), which is where the real limits live.
let gripDrag = null;
let gripSave = 0;

function gripResize(e) {
  if (!gripDrag) return;
  // The larger of the two axes wins, so a mostly-horizontal or mostly-vertical drag
  // both work; FOLDER_BOX is the icon's size at scale 1, which turns pixels into scale.
  const dx = e.screenX - gripDrag.x;
  const dy = e.screenY - gripDrag.y;
  const delta = Math.abs(dx) > Math.abs(dy) ? dx : dy;
  const next = Math.min(3, Math.max(0.5, gripDrag.scale + delta / FOLDER_BOX));
  if (Math.abs(next - state.scale) < 0.01) return;
  state.scale = next;
  renderIcon(); // paint the new size immediately; the window follows below
  markSelectedSize();
  // One write per frame at most: dragging fires far more often than the window can
  // actually be resized, and every call is a round trip.
  if (gripSave) return;
  gripSave = requestAnimationFrame(() => {
    gripSave = 0;
    invoke("set_float_folder_scale", { id: folderId, scale: state.scale }).catch(() => {});
  });
}

function gripReleased() {
  if (!gripDrag) return;
  gripDrag = null;
  document.body.classList.remove("resizing");
  if (gripSave) { cancelAnimationFrame(gripSave); gripSave = 0; }
  // Final exact value, in case the last frame was skipped.
  invoke("set_float_folder_scale", { id: folderId, scale: state.scale }).catch(() => {});
}

function iconReleased(e) {
  if (!iconDrag || e.buttons !== 0) return false;
  iconDrag = null;
  iconEl.classList.remove("dragging");
  return true;
}

window.addEventListener("mouseout", (e) => iconReleased(e));

window.addEventListener("mousemove", async (e) => {
  if (gripDrag) {
    if (e.buttons === 0) { gripReleased(); return; }
    gripResize(e);
    return;
  }
  if (iconReleased(e)) return;
  if (!iconDrag || iconDrag.moved) return;
  if (Math.abs(e.screenX - iconDrag.x) < DRAG_THRESHOLD && Math.abs(e.screenY - iconDrag.y) < DRAG_THRESHOLD) return;
  iconDrag.moved = true;
  iconEl.classList.add("dragging");
  await closePopover(true); // the grid would ride along on a window move
  await appWin.startDragging();
});

window.addEventListener("mouseup", () => {
  if (gripDrag) { gripReleased(); return; } // a resize never opens the folder
  if (!iconDrag) return;
  const wasDrag = iconDrag.moved;
  iconDrag = null;
  iconEl.classList.remove("dragging");
  if (wasDrag) return;
  if (open) closePopover();
  else openPopover("grid");
});

iconEl.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  e.stopPropagation();
  openPopover("menu");
});

window.addEventListener("contextmenu", (e) => e.preventDefault());
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") closePopover();
});
// A click anywhere else takes the focus away, which is what closes the folder.
window.addEventListener("blur", () => {
  if (!iconDrag && !memberDrag && !gripDrag) closePopover();
});

// ---- folder menu ----

for (const btn of menuEl.querySelectorAll("[data-scale]")) {
  btn.addEventListener("click", async () => {
    const scale = Number(btn.dataset.scale) || 1;
    state.scale = scale;
    markSelectedSize();
    renderIcon();
    open = null; // the backend collapses the window with the new size
    menuEl.classList.remove("shown");
    menuEl.classList.add("hidden");
    document.body.classList.remove("pop-open");
    iconEl.style.left = "0px";
    iconEl.style.top = "0px";
    await invoke("set_float_folder_scale", { id: folderId, scale }).catch(() => {});
  });
}

document.getElementById("menu-close").addEventListener("click", () => closePopover());

document.getElementById("menu-rename").addEventListener("click", async () => {
  await closePopover();
  // The folder window hosts no themed dialog — the main window asks for the name.
  invoke("folder_rename_request", { id: folderId }).catch(() => {});
});

document.getElementById("menu-dissolve").addEventListener("click", () => {
  invoke("dissolve_float_folder", { id: folderId }).catch(() => {}); // closes this window
});

// ---- members: click copies, drag pulls one out of the folder ----

let memberDrag = null;

gridEl.addEventListener("mousedown", (e) => {
  const tile = e.target.closest(".folder-member");
  if (!tile || e.button !== 0) return;
  e.stopPropagation();
  const out = dragOutOn && e[dragOutMod] === true;
  tile.draggable = out;
  memberDrag = { id: tile.dataset.id, el: tile, x: e.screenX, y: e.screenY, moved: false, out };
});

gridEl.addEventListener("dragstart", (e) => {
  const tile = e.target.closest(".folder-member");
  const p = tile && member(tile.dataset.id);
  const text = p ? p.file_path || p.text || "" : "";
  if (!dragOutOn || !text) { e.preventDefault(); return; }
  e.dataTransfer.effectAllowed = "copy";
  e.dataTransfer.setData("text/plain", text);
  if (p.file_path) {
    const u = encodeURI(p.file_path.replace(/\\/g, "/"));
    e.dataTransfer.setData("text/uri-list", u.startsWith("//") ? `file:${u}` : `file:///${u}`);
  }
  memberDrag = null;
});
gridEl.addEventListener("dragend", (e) => {
  const tile = e.target.closest(".folder-member");
  if (tile) tile.draggable = false;
});

window.addEventListener("mousemove", (e) => {
  if (!memberDrag || memberDrag.out) return;
  if (e.buttons === 0) {
    memberDrag.el.classList.remove("dragging");
    memberDrag = null;
    return;
  }
  if (memberDrag.moved) return;
  if (Math.abs(e.screenX - memberDrag.x) < DRAG_THRESHOLD && Math.abs(e.screenY - memberDrag.y) < DRAG_THRESHOLD) return;
  memberDrag.moved = true;
  memberDrag.el.classList.add("dragging");
});

window.addEventListener("mouseup", async (e) => {
  if (!memberDrag) return;
  const drag = memberDrag;
  memberDrag = null;
  drag.el.classList.remove("dragging");
  if (drag.out) return; // the OS drag owns this gesture (F22)
  if (!drag.moved) {
    copyMember(drag.id);
    return;
  }
  // Dropped inside the grid: nothing happened, it is not a reorder.
  const r = popEl.getBoundingClientRect();
  if (e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom) return;
  // Dropped outside: the member becomes a free pill right there.
  await closePopover();
  invoke("remove_folder_member", {
    folderId,
    id: drag.id,
    x: e.screenX,
    y: e.screenY,
  }).catch(() => {});
});

// Member context menu, laid over the open grid and kept inside it.
gridEl.addEventListener("contextmenu", (e) => {
  const tile = e.target.closest(".folder-member");
  if (!tile) return;
  e.preventDefault();
  e.stopPropagation();
  memberMenu.dataset.id = tile.dataset.id;
  memberMenu.classList.remove("hidden");
  const pop = popEl.getBoundingClientRect();
  const w = memberMenu.offsetWidth, h = memberMenu.offsetHeight;
  const left = Math.min(Math.max(e.clientX, pop.left), Math.max(pop.left, pop.right - w));
  const top = Math.min(Math.max(e.clientY, pop.top), Math.max(pop.top, pop.bottom - h));
  memberMenu.style.left = px(left);
  memberMenu.style.top = px(top);
});
gridEl.addEventListener("mousedown", (e) => {
  if (!memberMenu.contains(e.target)) memberMenu.classList.add("hidden");
});

document.getElementById("mm-edit").addEventListener("click", async () => {
  const id = memberMenu.dataset.id;
  memberMenu.classList.add("hidden");
  await closePopover();
  invoke("edit_prompt_request", { id }).catch(() => {});
});

document.getElementById("mm-out").addEventListener("click", async () => {
  const id = memberMenu.dataset.id;
  memberMenu.classList.add("hidden");
  const box = iconEl.getBoundingClientRect();
  const sx = window.screenX + box.left + box.width / 2;
  const sy = window.screenY + box.bottom + FOLDER_BOX * state.scale;
  await closePopover();
  invoke("remove_folder_member", { folderId, id, x: sx, y: sy }).catch(() => {});
});

// ---- backend events ----

listen("folder-changed", () => loadState());
// A pill is being dragged over this folder — show that letting go drops it in.
listen("merge-hint", (e) => iconEl.classList.toggle("merge-hint", e.payload === true));
listen("theme-changed", (e) => applyFloatTheme(e.payload?.theme, e.payload?.vars));
listen("language-changed", () => applySettings());
// Same as the pill: an expert change has to reach a folder that is already open.
listen("settings-changed", () => applySettings());
listen("prompt-updated", (e) => {
  if (e.payload && state.members.some((p) => p.id === e.payload.id)) loadState();
});

// Moving onto a monitor with a different DPI invalidates the physical box the
// expansion was computed from — recompute it (or re-assert the collapsed icon).
appWin.onScaleChanged(() => {
  if (open) reflow();
  else invoke("set_float_folder_scale", { id: folderId, scale: state.scale }).catch(() => {});
});

(async () => {
  try {
    applyFloatTheme(await invoke("current_theme"), await invoke("theme_vars"));
  } catch (_) {}
  // The folder paints itself first; the translations (a large module, imported
  // lazily so the window opens fast) land a moment later and only relabel.
  applySettings().catch(() => {});
  await loadState().catch(() => {});
})();
