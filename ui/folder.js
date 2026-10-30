// Pill folder window. The collapsed icon shows up to four member previews plus a count;
// clicking it grows THIS window into a grid of the member pills.
// One window per folder keeps z-order and focus as simple as a single pill's. All geometry
// (side, edge flipping, DPI) is decided in the backend, which knows the monitors.
import { FONTS, fitText } from "./fonts.js";
import { applyFloatTheme, inkForColor, onInkChange } from "./floattheme.js";
import { mediaKind } from "./media.js";
import { APP_NAME } from "./brand.js";
import { LANGS, compactTexts } from "./i18n.js";
import { DRAG_THRESHOLD_DEFAULT } from "./copyDefaults.js";
import { COPIED_MS, RING_MIN, RING_MAX, DRAG_OUT_MODS, FOLDER_SVG, resolveLang, fileUri, playCopySound, markSelectedSize, rotateBadgeLabel, readCopyPrefs, copyGate, needsMainWindowCopy, pasteFailText } from "./floatShared.js";

// Frameless window: the title is invisible in the app but shows in the Alt-Tab
// switcher, so it comes from the brand rather than a hardcoded English word.
document.title = APP_NAME;

const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen, emit } = window.__TAURI__.event;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

const appWin = getCurrentWebviewWindow();
const folderId = appWin.label.replace(/^folder-/, "");

const iconEl = document.getElementById("icon");
const previewEl = document.getElementById("preview");
const nameEl = document.getElementById("fname");
const parkedBadge = document.getElementById("parked-badge");
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
const DRAG_THRESHOLD = DRAG_THRESHOLD_DEFAULT;
const MENU_W = 230; // matches .menu in floating.css

let state = { id: folderId, name: "", scale: 1, cols: 3, members: [] };
let open = null; // "grid" | "menu" | null
// Every open/close bumps this. A round trip that finishes after a newer one started
// drops its result instead of resizing the window behind the current state.
let seq = 0;
// Separate from `seq`: openPopover()'s own await of closePopover() can outlive a
// newer openPopover() call that started in the meantime (its close finishes after
// the newer one already opened something); this catches that case specifically.
let openReq = 0;
// openPopover() is awaiting the backend box (`open` is still null then, so a blur
// has nothing to close); the blur is remembered and applied once the box arrived.
let opening = false;
let blurredWhileOpening = false;
// Empty until the settings arrive, like the pill: every label reaches the screen
// through the compact strings (the item word also through their fork-brand
// substitution), never through an English literal here.
let TXT = { copied: "", size: "", edit: "", close: "", rename: "", dissolve: "", takeOut: "" };

let copyPrefs = readCopyPrefs({});
let animMs = 150;
// Drag-out: the modifier hands a member to the OS instead of pulling it out of
// the folder, so the two gestures never fight over the same drag.
let dragOutOn = false, dragOutMod = "ctrlKey";

// Rotating prompt: "next entry" badge per member, kept live by the backend's
// rotation-changed event (any window's copy can move another member's pointer).
const rotateInfo = new Map(); // member id -> { index, pos, total }
// A rotation-changed push can land while that id's initial rotation_peek is in flight; the
// generation lets the peek drop its stale reply.
const rotateReqs = new Map(); // member id -> latest request number
let rotateBadgeOn = true; // mirrors flag("rotateBadge"), which ships on

function primeMemberRotate(id) {
  if (rotateInfo.has(id)) return;
  const req = (rotateReqs.get(id) || 0) + 1;
  rotateReqs.set(id, req);
  invoke("rotation_peek", { id }).then((r) => {
    if (!r || rotateReqs.get(id) !== req) return;
    rotateInfo.set(id, { index: r.index, pos: r.pos, total: r.total });
    const el = gridEl.querySelector(`.folder-member[data-id="${CSS.escape(id)}"] .rotate-badge`);
    if (el) el.textContent = rotateBadgeLabel(rotateInfo.get(id));
  }).catch(() => {});
}

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
    // Text on a member's own colour takes its ink from that colour, like a
    // floating button does; without one the theme's ink applies.
    const ink = inkForColor(p.color);
    if (ink) {
      el.style.setProperty("--pill-fg", ink.fg);
      el.style.setProperty("--pill-ink-shadow", ink.shadow);
    } else {
      el.style.removeProperty("--pill-fg");
      el.style.removeProperty("--pill-ink-shadow");
    }
    const label = document.createElement("span");
    label.className = "face-text";
    label.textContent = withName ? p.name : (p.name || "").slice(0, 2);
    el.appendChild(label);
  }
}

// A changed ink rule repaints the faces on screen without a reload.
onInkChange(() => {
  renderIcon();
  if (open === "grid") renderGrid();
});

function renderIcon() {
  const box = FOLDER_BOX * state.scale;
  document.body.style.setProperty("--folder-box", px(box));
  // Ring thickness scales with the icon: a fixed band is a hairline on a large folder and
  // swallows a small one whole.
  const ring = Math.round(Math.min(RING_MAX, Math.max(RING_MIN, box * 0.18)));
  document.getElementById("resize")?.style.setProperty("--rz", `${ring}px`);
  previewEl.innerHTML = "";
  for (const p of state.members.slice(0, PREVIEW_MAX)) {
    const cell = document.createElement("span");
    cell.className = "folder-face";
    paintFace(cell, p, false);
    previewEl.appendChild(cell);
  }
  nameEl.textContent = state.name;
  nameEl.classList.toggle("hidden", !state.name);
  // The count lives in the tooltip only: a number stamped on the icon covers a preview
  // cell. The tooltip names the first two text pills and counts the rest.
  const many = TXT.members ? TXT.members.replace("{n}", state.members.length) : "";
  const names = state.members.map((p) => p.name).filter(Boolean);
  const shown = names.slice(0, 2).join(", ");
  const rest = names.length - 2;
  const who = shown && rest > 0 ? `${shown} +${rest}` : shown;
  iconEl.title = [state.name, who, many].filter(Boolean).join(" · ");
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
    // Named by the prompt alone: the copy confirmation and the rotate badge are decoration here.
    tile.setAttribute("aria-label", p.name);
    tile.style.width = px(t);
    tile.style.height = px(t);
    const face = document.createElement("span");
    face.className = "member-face";
    paintFace(face, p, true);
    tile.appendChild(face);
    if (rotateBadgeOn && (p.variants || []).length) {
      const rb = document.createElement("span");
      rb.className = "rotate-badge";
      rb.setAttribute("aria-hidden", "true");
      rb.textContent = rotateBadgeLabel(rotateInfo.get(p.id));
      tile.appendChild(rb);
      primeMemberRotate(p.id);
    }
    const ok = document.createElement("span");
    ok.className = "member-ok";
    ok.setAttribute("aria-hidden", "true");
    ok.textContent = TXT.copied;
    tile.appendChild(ok);
    gridEl.appendChild(tile);
  }
  // Tile names grow with the folder (same auto-fit pass as the grid tiles and the pills).
  const cap = Math.max(9, Math.round(t / 3));
  for (const el of gridEl.querySelectorAll(".member-face .face-text")) {
    fitText(el, t * 0.92, t * 0.92, cap);
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
  const req = ++openReq;
  await closePopover(true); // instant: no fade between two popovers
  if (req !== openReq) return; // a newer openPopover() call already won
  const token = ++seq;
  const el = kind === "menu" ? menuEl : popEl;
  if (kind === "grid") renderGrid();
  el.classList.remove("hidden");
  opening = true;
  blurredWhileOpening = false;
  try {
    if (!(await layout(kind, el, token))) return;
    open = kind;
    if (blurredWhileOpening) { blurredWhileOpening = false; await closePopover(true); return; }
    // Let the browser see the start state before the transition runs. Re-check the token: a
    // blur-driven closePopover() since the await must not re-add "shown" onto a hidden el.
    requestAnimationFrame(() => { if (token === seq) el.classList.add("shown"); });
  } catch (_) {
    el.classList.add("hidden");
  } finally {
    if (req === openReq) opening = false;
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

// Same popover, recomputed: after a member left, and when the monitor's scale factor
// changes (the known DPI trap: the physical box it was computed from is stale).
async function reflow() {
  if (!open) return;
  const kind = open;
  const el = openEl();
  if (kind === "grid") renderGrid();
  await layout(kind, el, ++seq).catch(() => {});
}

// ---- copying a member (same path as a pill: copy_prompt + record_copy) ----

// `notice` replaces "Copied!" when an auto-paste was refused: the text is on the clipboard
// either way, and the member must not stay silent about the paste.
function flash(id, notice) {
  const tile = gridEl.querySelector(`.folder-member[data-id="${CSS.escape(id)}"]`);
  const ok = tile && tile.querySelector(".member-ok");
  if (tile) {
    if (notice && ok) ok.textContent = notice;
    tile.classList.add("copied");
    setTimeout(() => {
      tile.classList.remove("copied");
      if (ok) ok.textContent = TXT.copied;
    }, COPIED_MS);
  }
  if (copyPrefs.copySoundOn) playCopySound(copyPrefs.copySoundVolume);
  const live = document.getElementById("live");
  live.textContent = "";
  live.textContent = notice || TXT.copied;
}

async function copyMember(id) {
  // The gate (cooldown, double-click paste window, stamp before the await,
  // rollback) is shared with the pill window. A member with a placeholder is
  // handed to the main window, which asks for it and copies; that route never
  // pastes on its own.
  const p = member(id);
  const r = await copyGate(id, async () => {
    if (p && needsMainWindowCopy(p.text, p.variants)) {
      return emit("prompt-tray-copy", id).then(() => "handoff", () => false);
    }
    return invoke("copy_prompt", { id }).catch(() => false);
  }, copyPrefs);
  if (r === "paste") {
    // A fast second click pastes the just-copied text into the last app.
    invoke("paste_into_previous", { delayMs: copyPrefs.apDelay, enter: copyPrefs.apEnter }).catch((e) => flash(id, pasteFailText(e, TXT)));
  } else if (r === "copied") {
    flash(id);
    invoke("record_copy", { id }).catch(() => {});
  }
}

// ---- state + settings ----

let loadSeq = 0;
async function loadState() {
  // Replies can arrive out of order; only the newest request may replace the state.
  const seq = ++loadSeq;
  const data = await invoke("float_folder_state", { id: folderId }).catch(() => null);
  if (!data || seq !== loadSeq) return;
  state = data;
  renderIcon();
  // The stored scale arrives here, not through applyPrefs, and both run in
  // parallel; without this the size menu keeps whatever applySettings marked.
  markSelectedSize(menuEl, state.scale);
  if (open === "grid") await reflow();
}

// Same split as the pill: behaviour is applied before the labels, so a failure while
// building the texts never takes it away.
async function applySettings() {
  const s = await invoke("get_settings");
  applyPrefs(s);
  applyTexts(s);
}

function applyTexts(s) {
  const lang = resolveLang(s.language, LANGS);
  document.documentElement.lang = lang;
  TXT = compactTexts(lang);
  document.getElementById("size-label").textContent = TXT.size;
  parkedBadge.title = TXT.parked;
  document.getElementById("menu-rename").textContent = TXT.rename;
  document.getElementById("menu-hide").textContent = TXT.hideForNow;
  // The same folder glyph as the grid-folder and view-group "ungroup" menu entries; built
  // once, language changes only touch the label.
  const dissolveBtn = document.getElementById("menu-dissolve");
  let dissolveIcon = dissolveBtn.querySelector(".menu-icon");
  if (!dissolveIcon) {
    dissolveBtn.classList.add("has-icon");
    dissolveIcon = document.createElement("span");
    dissolveIcon.className = "menu-icon";
    dissolveIcon.innerHTML = FOLDER_SVG;
    dissolveBtn.appendChild(dissolveIcon);
  }
  let dissolveLabel = dissolveBtn.querySelector(".menu-label");
  if (!dissolveLabel) {
    dissolveLabel = document.createElement("span");
    dissolveLabel.className = "menu-label";
    dissolveBtn.appendChild(dissolveLabel);
  }
  dissolveLabel.textContent = TXT.dissolve;
  document.getElementById("mm-edit").textContent = TXT.edit;
  document.getElementById("mm-out").textContent = TXT.takeOut;
  const closeBtn = document.getElementById("menu-close");
  closeBtn.title = TXT.close;
  closeBtn.setAttribute("aria-label", TXT.close);
  for (const b of menuEl.querySelectorAll(".size-row [data-scale]")) {
    b.title = `${TXT.size} ${b.textContent}`;
    b.setAttribute("aria-label", b.title);
  }
  // The icon tooltip and the grid labels are built from TXT, so they follow a language change
  // and the first settings answer arriving after the folder state.
  renderIcon();
  if (open === "grid") renderGrid();
}

function applyPrefs(s) {
  document.body.style.fontFamily = FONTS[s.tile_font] || FONTS.system;
  copyPrefs = readCopyPrefs(s);
  dragOutOn = s.ui_flags?.dragOut === true;
  dragOutMod = DRAG_OUT_MODS[s.ui_texts?.dragOutModifier] || "ctrlKey";
  // rotateBadge is a sub-toggle of rotatingPrompts: the parent off must freeze the whole
  // feature (nothing dependent stays visible with its parent off).
  rotateBadgeOn = s.ui_flags?.rotateBadge !== false && s.ui_flags?.rotatingPrompts !== false;
  // The folder animation reuses the global animation setting.
  animMs = s.ui_flags?.animations === false ? 0 : 150;
  document.documentElement.style.setProperty("--transition", `${animMs}ms cubic-bezier(0.4, 0, 0.2, 1)`);
  const op = Number(s.ui_values?.floatOpacity);
  iconEl.style.opacity = Number.isFinite(op) ? `${Math.max(0.2, op / 100)}` : "";
  markSelectedSize(menuEl, state.scale);
  if (open === "grid") renderGrid();
}

// ---- folder icon: click opens, press-and-move drags the window ----

let iconDrag = null;

// Exactly the pill's mechanic: the whole window is the drag surface (the popover and
// the menus handle their own presses), and startDragging() hands the window to the OS
// move loop, which CONSUMES the release: so the drag also ends on the first later
// event that reports no button held.
window.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  if (e.target.closest(".menu") || e.target.closest(".folder-pop")) return;
  // The edge ring resizes instead of moving/opening: check it FIRST, or the icon's
  // own press would swallow the gesture. rz-hole covers the centre, so only a press
  // on the edge lands here.
  if (e.target.closest("#rz-catch")) {
    gripDrag = { x: e.screenX, y: e.screenY, scale: state.scale };
    gripPainted = 0; // the first move of a new grab always draws
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
let gripPainted = 0;
// A mouse reports far more often than a screen refreshes and every report rebuilds the
// whole icon, previews included. One repaint per frame is enough; the release draws the
// final size.
const GRIP_PAINT_MS = 16;

function gripPaint() {
  gripPainted = performance.now();
  renderIcon();
  markSelectedSize(menuEl, state.scale);
}

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
  if (performance.now() - gripPainted >= GRIP_PAINT_MS) gripPaint();
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
  // Final exact value, in case the last frame was skipped: on screen and in the store.
  gripPaint();
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
  // The ring is otherwise invisible until it is grabbed: a thin accent edge on
  // the icon says "resize starts here" while the pointer sits in the band. e.target can
  // be the window itself for a synthetic event with no real target under the pointer.
  iconEl.classList.toggle("rz-hover", !!e.target?.closest?.("#rz-catch"));
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
// Enter and Space on the focused icon raise a click with no pointer (detail 0): same
// toggle as a mouse release. A mouse click is handled on mouseup above.
iconEl.addEventListener("click", (e) => {
  if (e.detail !== 0) return;
  if (open) closePopover();
  else openPopover("grid");
});

iconEl.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  e.stopPropagation();
  openPopover("menu");
});

window.addEventListener("contextmenu", (e) => e.preventDefault());
window.addEventListener("keydown", async (e) => {
  if (e.key === "Escape") { closePopover(); return; }
  // Arrow/Shift+arrow move like the floating pill; the folder window has no other keyboard drag.
  if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key) &&
      menuEl.classList.contains("hidden") && popEl.classList.contains("hidden") &&
      !iconDrag && !memberDrag && !gripDrag) {
    e.preventDefault();
    const step = e.shiftKey ? 20 : 4;
    const dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0;
    const dy = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
    try {
      const p = await appWin.outerPosition();
      const d = window.devicePixelRatio || 1;
      await invoke("set_folder_position", { id: folderId, x: p.x / d + dx, y: p.y / d + dy, clamp: true });
    } catch {}
  }
});
// A click anywhere else takes the focus away, which is what closes the folder.
window.addEventListener("blur", () => {
  if (iconDrag || memberDrag || gripDrag) return;
  if (opening && !open) blurredWhileOpening = true;
  closePopover();
});

// ---- folder menu ----

for (const btn of menuEl.querySelectorAll("[data-scale]")) {
  btn.addEventListener("click", async () => {
    const scale = Number(btn.dataset.scale) || 1;
    state.scale = scale;
    markSelectedSize(menuEl, state.scale);
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
  // The folder window hosts no themed dialog: the main window asks for the name.
  invoke("folder_rename_request", { id: folderId }).catch(() => {});
});

document.getElementById("menu-hide").addEventListener("click", () => {
  invoke("hide_float_for_now", { id: folderId }).catch(() => {}); // closes this window, until restart
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
  if (p.file_path) e.dataTransfer.setData("text/uri-list", fileUri(p.file_path));
  memberDrag = null;
});
// Keyboard activation of a member tile (a mouse click copies on mouseup below).
gridEl.addEventListener("click", (e) => {
  const tile = e.target.closest(".folder-member");
  if (tile && e.detail === 0) copyMember(tile.dataset.id);
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
  // A real OS drag would already have cleared memberDrag in dragstart, so an
  // out gesture that reaches here never moved: it is a plain click and copies.
  if (drag.out) { copyMember(drag.id); return; }
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
// #member-menu is a sibling of #pop, not inside #grid, so a listener on gridEl never sees a
// mousedown outside the grid; document catches every click.
document.addEventListener("mousedown", (e) => {
  if (!memberMenu.classList.contains("hidden") && !memberMenu.contains(e.target)) memberMenu.classList.add("hidden");
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
// A pill is being dragged over this folder: show that letting go drops it in.
listen("merge-hint", (e) => iconEl.classList.toggle("merge-hint", e.payload === true));
listen("theme-changed", (e) => applyFloatTheme(e.payload?.theme, e.payload?.vars));
listen("language-changed", () => applySettings().catch(() => {}));
// An expert change has to reach an already open folder. applySettings() alone never updates
// state.cols, which only arrives through loadState(); applySettings must land BEFORE
// loadState so the window is sized for a grid drawn with the new font and animation prefs.
listen("settings-changed", () => {
  applySettings().catch(() => {}).then(() => loadState());
  refreshParkedBadge();
});
listen("prompt-updated", (e) => {
  if (e.payload && state.members.some((p) => p.id === e.payload.id)) {
    // An edit can change entry count/text: drop the cached rotation state so
    // the reload below primes it fresh (primeMemberRotate only peeks once per id).
    rotateInfo.delete(e.payload.id);
    loadState();
  }
});
// A member's rotation pointer moved (any window may have copied it).
listen("rotation-changed", (e) => {
  const { id, index, pos, total } = e.payload || {};
  if (!id || !state.members.some((p) => p.id === id)) return;
  // Newer than anything primeMemberRotate has in flight for this id: invalidate it so a
  // slower peek reply cannot overwrite the position.
  rotateReqs.set(id, (rotateReqs.get(id) || 0) + 1);
  rotateInfo.set(id, { index, pos, total });
  const el = gridEl.querySelector(`.folder-member[data-id="${CSS.escape(id)}"] .rotate-badge`);
  if (el) el.textContent = rotateBadgeLabel(rotateInfo.get(id));
});

// Moving onto a monitor with a different DPI invalidates the physical box the
// expansion was computed from; recompute it (or re-assert the collapsed icon).
appWin.onScaleChanged(() => {
  if (open) reflow();
  else invoke("set_float_folder_scale", { id: folderId, scale: state.scale }).catch(() => {});
});

// Asked at boot and again after a settings change (the recall action moves a
// parked folder back).
function refreshParkedBadge() {
  invoke("is_float_parked", { id: folderId }).then((parked) => {
    parkedBadge.classList.toggle("hidden", !parked);
  }).catch(() => {});
}

(async () => {
  try {
    applyFloatTheme(await invoke("current_theme"), await invoke("theme_vars"));
  } catch (_) {}
  // The folder paints itself first; the settings land a moment later and only
  // relabel.
  applySettings().catch(() => {});
  await loadState().catch(() => {});
  refreshParkedBadge();
})();
