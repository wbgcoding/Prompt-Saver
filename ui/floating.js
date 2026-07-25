// Floating quick-copy pill. Click copies; press-and-move drags the window;
// right-click opens a small menu (size, edit prompt, remove).
// i18n is loaded lazily (dynamic import) so the pill window opens fast.
import { FONTS, fitText } from "./fonts.js";
import { mediaKind, buildMediaBar, applyVideoPrefs } from "./media.js";

const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

const appWin = getCurrentWebviewWindow();
const promptId = appWin.label.replace(/^float-/, "");

const pill = document.getElementById("pill");
const label = document.getElementById("label");
const menu = document.getElementById("menu");
const resizeEl = document.getElementById("resize");
const rzCatch = document.getElementById("rz-catch");

let scale = 1;
let tileSize = 0; // global text size setting (0 = auto)
let globalFontKey = "system";
let copySize = 0; // expert copy-text size (0 = auto-fit to the pill)
let copyFont = ""; // expert copy-text font key ("" = pill font)
let pFont = ""; // per-prompt overrides ("" / 0 = follow settings)
let cooldownMs = 2000; // re-copy throttle for this pill (expert value)
let lastCopyTs = 0;
// F19 auto-paste on this pill (all default off; a fast second click pastes).
let apOn = false, apEnter = false, apDelay = 80, apDblMs = 400, lastPillTs = 0;
let pSize = 0;
let pCaptionSize = 0; // per-prompt caption size (0 = default, 1 = auto)
let menuOpen = false;

function applyFont() {
  pill.style.fontFamily = FONTS[pFont || globalFontKey] || FONTS.system;
}

// Keep in sync with the backend pill constants (FLOAT_H / FLOAT_IMG).
const PILL_H = 80;
const PILL_IMG = 400;
const PILL_FONT_DEFAULT = 26; // text pill font size at scale 1
const PILL_TEXT_MIN_W = 200; // smallest text-pill width; grows with the label
const PILL_BORDER = 5; // colored frame width around media pills
const SIZE_EPSILON = 2; // px tolerance ending the self-healing resize loop
const FILE_POLL_MS = 5000;
const COPIED_MS = 900;
const RING_MIN = 8; // smallest resize-ring thickness (px)
const RING_MAX = 60; // largest resize-ring thickness (px)
const RING_FRAC = 0.18; // ring thickness as a fraction of the shorter window side

// Resizes and visibility flips can leave the looping video paused — make
// sure a visible video is always playing again afterwards. Only a pause made
// through the control bar (or play-once ending) is respected.
function ensureVideoPlaying() {
  if (userPaused) return;
  if (!videoEl.classList.contains("hidden") && videoEl.paused) {
    videoEl.play().catch(() => {});
  }
}

// Text pills use a fixed, readable font size; the window WIDTH grows with
// the label instead (capped, then ellipsis kicks in).
function pillFontSize(effScale = scale) {
  // pSize/tileSize: 0 = inherit, 1 = auto-fit (treated as default here).
  const base = pSize > 1 ? pSize : tileSize > 1 ? tileSize : PILL_FONT_DEFAULT;
  return Math.round(base * effScale);
}

// Current pill box at the active scale. Media pills follow their aspect
// ratio EXACTLY (longest side = PILL_IMG, plus the colored frame): the window
// then equals the media box, so the picture fills the whole button without
// cropping or letterboxing. Text width follows the label.
function pillBox() {
  if (pill.classList.contains("has-image")) {
    const base = PILL_IMG * scale;
    let w = base;
    let h = base;
    if (imgRatio && imgRatio !== 1) {
      if (imgRatio > 1) h = Math.round(base / imgRatio);
      else w = Math.round(base * imgRatio);
    }
    return { w: w + 2 * borderW, h: h + 2 * borderW };
  }
  const RATIO = 1.3;
  const h = PILL_H * scale;
  // Font tracks the ACTUAL window height, not the raw scale. If the window is ever
  // capped so it can't grow any further (OS size limit, DPI handoff), the text
  // stops growing with it instead of ballooning on past the frozen pill edge — the
  // reported "only the text keeps getting bigger" bug. When the box grows normally
  // innerHeight equals h, so this is exactly the old base*scale. min() guards a
  // stale-large innerHeight (mid-shrink) from oversizing the font.
  const effH = Math.min(h, window.innerHeight || h);
  label.style.fontSize = `${pillFontSize(effH / PILL_H)}px`;
  // Width hugs the label. Padding AND the min width scale WITH the pill, so it keeps
  // shrinking (and growing) together with the text instead of stalling at a fixed
  // floor while only the text changes. Floored to a pill ratio so it never rounds
  // into a circle. (At scale 1 this is exactly the old 200 min / 60 padding.)
  const padX = Math.round(30 * scale);
  const minW = Math.round(PILL_TEXT_MIN_W * scale);
  const w = Math.max(minW, label.scrollWidth + 2 * padX, Math.round(h * RATIO));
  return { w, h };
}

function applyPillSize() {
  if (menuOpen) return;
  if (pill.classList.contains("has-image")) {
    // The window hugs the visible media — no oversized invisible button.
    if (!imgRatio) return;
    const box = pillBox();
    invoke("resize_float_media", { id: promptId, width: box.w, height: box.h }).catch(() => {});
    return;
  }
  const box = pillBox();
  invoke("resize_float_pill", { id: promptId, width: box.w, height: box.h }).catch(() => {});
}

function markSelectedSize() {
  for (const btn of menu.querySelectorAll("[data-scale]")) {
    btn.classList.toggle("sel", Math.abs(Number(btn.dataset.scale) - scale) < 0.01);
  }
}

async function applySettings() {
  const [s, { FLOAT_TXT: TXT }] = await Promise.all([
    invoke("get_settings"),
    import("./i18n.js"),
  ]);
  const pref = (s.language && s.language !== "auto"
    ? s.language
    : navigator.language || "en"
  ).toLowerCase();
  const lang = Object.keys(TXT).find((c) => pref.startsWith(c)) || "en";
  const t = TXT[lang];
  document.getElementById("feedback").textContent = t.copied;
  document.getElementById("size-label").textContent = t.size;
  // Localized tooltip for the S/M/L size buttons (visible letters stay universal).
  for (const b of document.querySelectorAll(".size-row [data-scale]")) {
    b.title = `${t.size} ${b.textContent}`;
    b.setAttribute("aria-label", b.title);
  }
  document.getElementById("menu-edit").textContent = t.edit;
  document.getElementById("menu-remove").textContent = t.remove;
  const closeBtn = document.getElementById("menu-close");
  closeBtn.title = t.close;
  closeBtn.setAttribute("aria-label", t.close);
  document.getElementById("file-error").textContent = t.missing;
  applyVideoPrefs(videoEl, s.video_prefs && s.video_prefs[promptId]);
  // Clamp to the backend's stored range (0.5–2.0): a corrupt 0/negative/NaN
  // factor would otherwise produce a degenerate (zero/inverted) window.
  const rawScale = Number(s.float_scale && s.float_scale[promptId]);
  scale = Number.isFinite(rawScale) && rawScale > 0 ? Math.min(2, Math.max(0.5, rawScale)) : 1;
  tileSize = Number(s.tile_size) || 0;
  globalFontKey = s.tile_font || "system";
  copySize = Number(s.ui_values?.copySize) || 0;
  const cd = Number(s.ui_values?.copyCooldownMs);
  cooldownMs = Number.isFinite(cd) ? cd : 2000;
  apOn = s.ui_flags?.autoPaste !== false; // default on
  apEnter = s.ui_flags?.autoPasteEnter === true;
  const apd = Number(s.ui_values?.autoPasteDelayMs);
  apDelay = Number.isFinite(apd) ? apd : 80;
  const adb = Number(s.ui_values?.dblClickMs);
  apDblMs = Number.isFinite(adb) ? adb : 400;
  copyFont = s.ui_texts?.copyFont || "";
  document.getElementById("feedback").style.fontFamily = FONTS[copyFont] || "";
  const op = Number(s.ui_values?.floatOpacity);
  pill.style.opacity = Number.isFinite(op) ? `${Math.max(0.2, op / 100)}` : "";
  applyFont();
  markSelectedSize();
  applyPillSize();
}

// Self-healing size: DPI handoffs during cross-monitor drags can leave the
// window at a wrong size — whenever it differs from the pill's real box,
// snap it back. The ±2px tolerance ends the loop once it matches.
function enforcePillSize() {
  // No resize ping-pong while the OS moves the window or the user resizes it.
  if (menuOpen || pill.classList.contains("dragging") || pill.classList.contains("resizing")) return;
  if (pill.classList.contains("has-image") && !imgRatio) return;
  const box = pillBox();
  if (Math.abs(window.innerWidth - box.w) > SIZE_EPSILON ||
      Math.abs(window.innerHeight - box.h) > SIZE_EPSILON) {
    applyPillSize();
  }
}

// Size the resize-grip band in proportion to the pill so the resize hitbox always
// sits right at the (rounded) edge — thin on a small pill, a comfortable ring on a
// big one — while the inner area stays free for dragging/copying.
function updateResizeRing() {
  const ring = Math.max(RING_MIN, Math.min(RING_MAX, Math.round(Math.min(window.innerWidth, window.innerHeight) * RING_FRAC)));
  resizeEl.style.setProperty("--rz", `${ring}px`);
}

let fitRaf = 0;
window.addEventListener("resize", () => {
  cancelAnimationFrame(fitRaf);
  fitRaf = requestAnimationFrame(() => { enforcePillSize(); updateResizeRing(); });
});
updateResizeRing();

const DRAG_THRESHOLD = 4;
let drag = null; // {x, y, moved}
let copiedTimer = null;

// Width/height ratio of the pill image (0 = text pill); the active frame
// width is part of the window box (see pillBox).
let imgRatio = 0;
let borderW = 0;

// Apply name, color and media (stored still, gif or video) onto the pill.
const videoEl = document.getElementById("video");
const imgEl = document.getElementById("img");

// ---- Video controls + freeze watchdog ----
// Pause bookkeeping: only pauses made through the control bar (or a video
// that finished in play-once mode) stay paused — every pause the app makes
// itself (drag, hidden window) is resumed automatically.
let appPause = false;
let userPaused = false;
videoEl.addEventListener("pause", () => { userPaused = !appPause; appPause = false; });
videoEl.addEventListener("play", () => { userPaused = false; appPause = false; });

function pauseByApp() {
  if (!videoEl.paused) {
    appPause = true;
    videoEl.pause();
  }
}

// Hover control bar (shared with the grid tiles). Lives next to the pill —
// nesting it inside the <button> pill would be invalid HTML.
const mediaBar = buildMediaBar(videoEl, {
  onChange: (prefs) => invoke("set_video_prefs", { id: promptId, ...prefs }).catch(() => {}),
});
mediaBar.classList.add("hidden");
document.body.appendChild(mediaBar);

// The bar appears while the cursor is in the lower zone of a video pill and
// stays while the bar itself (incl. the volume popup above it) is used.
const volDragging = () => !!mediaBar.querySelector(".media-sound.dragging");
window.addEventListener("mousemove", (e) => {
  const zone = Math.max(48, window.innerHeight * 0.35);
  const show = !videoEl.classList.contains("hidden") && !menuOpen &&
    (e.clientY > window.innerHeight - zone ||
      !!e.target.closest?.(".media-bar") || volDragging());
  mediaBar.classList.toggle("hidden", !show);
  document.getElementById("caption").classList.toggle("bar-open", show);
});
window.addEventListener("mouseout", (e) => {
  if (!e.relatedTarget && !volDragging()) {
    mediaBar.classList.add("hidden");
    document.getElementById("caption").classList.remove("bar-open");
  }
});

// Worst-case recovery: the pill video must never stay frozen. If playback is
// suspended without a user pause, resume it; if it reports "playing" but the
// position no longer advances (decoder stalled after a DPI handoff or window
// move), reload the source and continue where it stopped.
const STALL_POLL_MS = 3000;
let lastVideoTime = -1;
setInterval(() => {
  // Never touch the video while the window is being dragged — resuming
  // playback mid-drag is exactly what makes the drag stutter.
  if (videoEl.classList.contains("hidden") || document.hidden || userPaused || menuOpen || drag || resizing) {
    lastVideoTime = -1;
    return;
  }
  if (videoEl.paused) {
    videoEl.play().catch(() => {});
    return;
  }
  if (videoEl.currentTime === lastVideoTime && !videoEl.seeking) {
    const at = lastVideoTime;
    lastVideoTime = -1;
    if (!videoEl.getAttribute("src")) return;
    videoEl.load(); // hard reset of the decoder pipeline
    videoEl.addEventListener(
      "loadedmetadata",
      () => {
        videoEl.currentTime = Math.min(at, Math.max(0, (videoEl.duration || 0) - 0.1));
        videoEl.play().catch(() => {});
      },
      { once: true }
    );
    return;
  }
  lastVideoTime = videoEl.currentTime;
}, STALL_POLL_MS);

function applyPrompt(p) {
  label.textContent = p.name;
  pill.title = p.name;
  const iconKind = !p.image && p.icon_path ? mediaKind(p.icon_path) : "";
  const showPath = iconKind ? p.icon_path : p.file_path;
  const kind = showPath ? mediaKind(showPath) : "";
  const pathMedia = !p.image && (kind === "gif" || kind === "video");
  const captionEl = document.getElementById("caption");
  const showMedia = p.show_image && (p.image || pathMedia);
  captionEl.textContent = p.caption || "";
  captionEl.classList.toggle("hidden", !showMedia || !p.caption);
  captionEl.classList.toggle("auto", p.caption_size === 1);
  captionEl.style.fontSize = p.caption_size > 1 ? `${Math.round(p.caption_size * scale)}px` : "";
  if (showMedia) {
    pill.classList.add("has-image");
    pill.style.background = "transparent";
    // The chosen color frames the media; the window hugs the media box.
    borderW = p.color ? PILL_BORDER : 0;
    pill.style.border = p.color ? `${PILL_BORDER}px solid ${p.color}` : "none";
    const onRatio = (ratio) => {
      imgRatio = ratio || 1;
      applyPillSize(); // resize the window to the media's aspect ratio
    };
    if (pathMedia && kind === "video") {
      imgEl.classList.add("hidden");
      imgEl.removeAttribute("src");
      const src = convertFileSrc(showPath);
      if (videoEl.getAttribute("src") !== src) {
        videoEl.src = src; // only a changed source reloads the video
        videoEl.onloadedmetadata = () =>
          onRatio(videoEl.videoWidth / videoEl.videoHeight);
      } else if (videoEl.videoWidth) {
        onRatio(videoEl.videoWidth / videoEl.videoHeight);
      }
      videoEl.classList.remove("hidden");
      ensureVideoPlaying();
    } else {
      videoEl.classList.add("hidden");
      videoEl.removeAttribute("src");
      const src = p.image || convertFileSrc(showPath);
      imgEl.onload = () => onRatio(imgEl.naturalWidth / imgEl.naturalHeight);
      imgEl.onerror = () => onRatio(1); // broken image: still size the pill
      imgEl.src = src;
      imgEl.classList.remove("hidden");
    }
  } else {
    videoEl.classList.add("hidden");
    videoEl.removeAttribute("src");
    imgEl.classList.add("hidden");
    imgEl.removeAttribute("src");
    pill.classList.remove("has-image");
    pill.style.background = p.color || "";
    pill.style.border = "";
    imgRatio = 0;
    borderW = 0;
  }
  hasFile = !!(p.file_path || p.icon_path);
  pFont = p.font || "";
  pSize = p.font_size || 0;
  pCaptionSize = p.caption_size || 0;
  applyFont();
  applyPillSize();
  pollFileMissing();
}

async function loadName() {
  try {
    const p = await invoke("get_prompt", { id: promptId });
    if (p) applyPrompt(p);
  } catch (_) {}
}

// ---- Missing-file watcher ----
let hasFile = false;

async function pollFileMissing() {
  if (!hasFile) {
    pill.classList.remove("file-missing");
    return;
  }
  try {
    const ids = await invoke("missing_files");
    pill.classList.toggle("file-missing", ids.includes(promptId));
  } catch {}
}
setInterval(pollFileMissing, FILE_POLL_MS);

function showCopied() {
  const fb = document.getElementById("feedback");
  if (copySize > 0) fb.style.fontSize = `${copySize}px`;
  else fitText(fb, pill.clientWidth - 10, pill.clientHeight - 6, 120);
  pill.classList.add("copied");
  clearTimeout(copiedTimer);
  copiedTimer = setTimeout(() => pill.classList.remove("copied"), COPIED_MS);
}

// ---- Right-click menu ----
// The window grows below the pill: the pill stays visible, so size changes
// preview live while the menu is open.
function lockPillBox(w, h) {
  pill.style.width = `${w}px`;
  pill.style.height = `${h}px`;
}

const MENU_GAP = 6;
let menuState = null; // { px, py, side } — pill screen pos + chosen side

// With the menu open the window grows past the pill, so pin the resize grips to
// the pill's box (offset offX/offY inside the window) — the edges/corners stay on
// the pill itself, never on the menu's far edge. Reset to full-window on close.
function confineResizeToPill(offX, offY, w, h) {
  const s = resizeEl.style;
  s.left = `${offX}px`;
  s.top = `${offY}px`;
  s.right = "auto";
  s.bottom = "auto";
  s.width = `${w}px`;
  s.height = `${h}px`;
}
function resetResizeOverlay() {
  const s = resizeEl.style;
  s.left = s.top = s.right = s.bottom = s.width = s.height = "";
}

// Grow the window and place the menu on whichever side of the pill has room
// (below, else above, else right, else left), centred on the pill.
function placeMenu(box, mw, mh) {
  const { px, py, side } = menuState;
  document.body.classList.remove("menu-below", "menu-above", "menu-right", "menu-left");
  document.body.classList.add("menu-open", "menu-" + side);
  // offX/offY = the pill's offset inside the window, so the pill can be put
  // back exactly on close (even after the window was dragged).
  let x = px, y = py, w, h, offX = 0, offY = 0;
  if (side === "below" || side === "above") {
    w = Math.max(box.w, mw);
    h = box.h + MENU_GAP + mh;
    offX = (w - box.w) / 2;
    x = px - offX;
    if (side === "above") { offY = MENU_GAP + mh; y = py - offY; }
  } else {
    w = box.w + MENU_GAP + mw;
    h = Math.max(box.h, mh);
    offY = (h - box.h) / 2;
    y = py - offY;
    if (side === "left") { offX = MENU_GAP + mw; x = px - offX; }
  }
  menuState.offX = offX;
  menuState.offY = offY;
  confineResizeToPill(offX, offY, box.w, box.h);
  invoke("set_float_bounds", { id: promptId, x, y, width: w, height: h }).catch(() => {});
}

async function openMenu() {
  if (menuOpen) return;
  menuOpen = true;
  mediaBar.classList.add("hidden");
  // Lock the pill to its CURRENT on-screen size (not a fresh measurement) so
  // opening the menu never nudges the button's size.
  const box = { w: window.innerWidth, h: window.innerHeight };
  lockPillBox(box.w, box.h);
  // Pill screen position (logical px); the window equals the pill while closed.
  let px = 0, py = 0;
  try {
    const p = await appWin.outerPosition();
    const d = window.devicePixelRatio || 1;
    px = p.x / d;
    py = p.y / d;
  } catch {}
  menu.classList.remove("hidden");
  const mw = menu.offsetWidth;
  const mh = menu.offsetHeight;
  const al = screen.availLeft || 0, at = screen.availTop || 0;
  const aw = screen.availWidth, ah = screen.availHeight;
  let side;
  if (py + box.h + MENU_GAP + mh <= at + ah) side = "below";
  else if (py - MENU_GAP - mh >= at) side = "above";
  else if (px + box.w + MENU_GAP + mw <= al + aw) side = "right";
  else if (px - MENU_GAP - mw >= al) side = "left";
  else side = "below";
  menuState = { px, py, side };
  placeMenu(box, mw, mh);
}

async function closeMenu() {
  if (!menuOpen) return;
  menuOpen = false;
  const st = menuState;
  menuState = null;
  menu.classList.add("hidden");
  // Restore the window to just the pill, at the pill's CURRENT screen position
  // (it may have been dragged while the menu was open). Shrink first, with the
  // pill still locked, so it never stretches to the larger box.
  const box = pillBox();
  let x = st ? st.px : 0, y = st ? st.py : 0;
  try {
    const p = await appWin.outerPosition();
    const d = window.devicePixelRatio || 1;
    x = p.x / d + (st ? st.offX : 0);
    y = p.y / d + (st ? st.offY : 0);
  } catch {}
  await invoke("set_float_bounds", { id: promptId, x, y, width: box.w, height: box.h }).catch(() => {});
  document.body.classList.remove("menu-open", "menu-below", "menu-above", "menu-right", "menu-left");
  resetResizeOverlay();
  pill.style.width = "";
  pill.style.height = "";
  ensureVideoPlaying();
}

window.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  openMenu();
});
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") closeMenu();
});
// Closing on blur, but not while dragging (a drag must keep the menu attached).
window.addEventListener("blur", () => { if (!drag) closeMenu(); });

for (const btn of menu.querySelectorAll("[data-scale]")) {
  btn.addEventListener("click", async () => {
    scale = Number(btn.dataset.scale) || 1;
    markSelectedSize();
    await invoke("set_float_scale", { id: promptId, scale, resize: false });
    const box = pillBox();
    lockPillBox(box.w, box.h);
    placeMenu(box, menu.offsetWidth, menu.offsetHeight);
    ensureVideoPlaying();
  });
}

document.getElementById("menu-close").addEventListener("click", () => closeMenu());

document.getElementById("menu-edit").addEventListener("click", async () => {
  await closeMenu();
  await invoke("edit_prompt_request", { id: promptId }).catch(() => {});
});

document.getElementById("menu-remove").addEventListener("click", () => {
  invoke("toggle_floating", { id: promptId }).catch(() => {}); // closes this window
});

// ---- Drag / copy ----
// The whole pill window is the drag/copy surface; the menu buttons and resize
// grips handle their own presses and are excluded.
window.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  if (e.target.closest(".menu") || e.target.closest(".rz-catch")) return;
  drag = { x: e.screenX, y: e.screenY, moved: false };
});

window.addEventListener("mousemove", async (e) => {
  // Lost mouseup (can happen when the OS swallows it after a window drag):
  // no button is held anymore, so end the drag state and recover playback.
  if (drag && e.buttons === 0) {
    drag = null;
    pill.classList.remove("dragging");
    if (!menuOpen) applyPillSize();
    ensureVideoPlaying();
    return;
  }
  if (!drag || drag.moved) return;
  if (Math.abs(e.screenX - drag.x) < DRAG_THRESHOLD &&
      Math.abs(e.screenY - drag.y) < DRAG_THRESHOLD) return;
  drag.moved = true;
  pill.classList.add("dragging");
  // Decoding video frames while the OS moves the window makes the drag
  // stutter — pause for the duration of the drag.
  if (!videoEl.classList.contains("hidden")) pauseByApp();
  // The whole window moves — an open menu rides along with the pill.
  await appWin.startDragging();
});

window.addEventListener("mouseup", async () => {
  if (!drag) return;
  const wasDrag = drag.moved;
  drag = null;
  pill.classList.remove("dragging");
  if (wasDrag) {
    if (!menuOpen) applyPillSize(); // keep the menu+window when one is open
    ensureVideoPlaying();
  } else if (menuOpen) {
    closeMenu(); // a click on the pill closes the open menu
  } else if (apOn && lastPillTs && Date.now() - lastPillTs <= apDblMs) {
    // F19: fast second click pastes the just-copied text into the last app.
    lastPillTs = 0;
    invoke("paste_into_previous", { delayMs: apDelay, enter: apEnter }).catch(() => {});
  } else if (cooldownMs && Date.now() - lastCopyTs < cooldownMs) {
    // Same pill copied moments ago — throttle to avoid accidental repeats.
  } else if (await invoke("copy_prompt", { id: promptId }).catch(() => false)) {
    lastCopyTs = Date.now();
    lastPillTs = Date.now(); // arm the double-click paste window
    showCopied();
    invoke("record_copy", { id: promptId }).catch(() => {});
  }
});

// ---- Edge/corner resize ----
// The grabbed edge/corner follows the cursor 1:1; the opposite side stays put.
// Content scales with the window (aspect kept), up to nearly full screen.
const SCALE_MIN = 0.3;
let resizing = null;
let rzRaf = 0;
let rzPending = null;

function updateCaptionScale() {
  if (pCaptionSize > 1) {
    document.getElementById("caption").style.fontSize = `${Math.round(pCaptionSize * scale)}px`;
  }
}

// Pill box (logical px) at a given scale, leaving the live scale untouched.
function boxAtScale(s) {
  const saved = scale;
  scale = s;
  const b = pillBox();
  scale = saved;
  pillBox(); // restore the label font to the current scale
  return b;
}

function liveResize(s) {
  const r = resizing;
  scale = Math.max(SCALE_MIN, Math.min(r.sMax, s));
  updateCaptionScale();
  const box = pillBox(); // actual size at the new scale (re-measures text)
  const left = r.dirX > 0 ? r.L : r.dirX < 0 ? r.L + r.w0 - box.w : r.L + (r.w0 - box.w) / 2;
  const top = r.dirY > 0 ? r.T : r.dirY < 0 ? r.T + r.h0 - box.h : r.T + (r.h0 - box.h) / 2;
  invoke("set_float_bounds", { id: promptId, x: left, y: top, width: box.w, height: box.h }).catch(() => {});
}

async function endResize() {
  if (!resizing) return;
  resizing = null;
  cancelAnimationFrame(rzRaf);
  rzRaf = 0;
  rzPending = null;
  pill.classList.remove("resizing");
  await invoke("set_float_scale", { id: promptId, scale, resize: false });
  markSelectedSize();
  ensureVideoPlaying();
}

// Which third of the pill the pointer is in picks the resize anchor. Shared by the
// cursor hint and the mousedown handler so both agree on edge vs corner.
function rzDir(e) {
  const W = window.innerWidth || 1, H = window.innerHeight || 1;
  const dx = e.clientX < W / 3 ? -1 : e.clientX > (2 * W) / 3 ? 1 : 0;
  const dy = e.clientY < H / 3 ? -1 : e.clientY > (2 * H) / 3 ? 1 : 0;
  return { dx, dy };
}
// Cursor matches the grabbed segment: straight edges get the axis arrow (ns/ew),
// only the rounded corners get a diagonal one — no diagonal arrow on a flat edge.
function rzCursor(dx, dy) {
  if (dx && dy) return dx * dy > 0 ? "nwse-resize" : "nesw-resize";
  if (dy) return "ns-resize";
  return "ew-resize";
}
rzCatch.addEventListener("mousemove", (e) => {
  if (resizing) return; // keep the cursor locked while actively resizing
  const { dx, dy } = rzDir(e);
  rzCatch.style.cursor = rzCursor(dx || (dy ? 0 : 1), dy);
});

// Grab the stadium ring to resize. Which third of the pill was grabbed picks the
// anchor (the opposite edge stays put); for the uniform pill scale that just sets
// which way it grows. Same cursor-distance -> scale math as the old edge grips.
rzCatch.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  e.preventDefault();
  e.stopPropagation();
  const d = rzDir(e);
  let dirX = d.dx;
  const dirY = d.dy;
  if (!dirX && !dirY) dirX = 1; // always drive at least one axis
  rzCatch.style.cursor = rzCursor(dirX, dirY); // lock the matching cursor
  const pr = pill.getBoundingClientRect();
  // Pill top-left in screen CSS px (works whatever side the menu is on).
  const L = e.screenX - e.clientX + pr.left;
  const T = e.screenY - e.clientY + pr.top;
  // Right-click menu open? Collapse it and resize the pill itself.
  if (menuOpen) {
    menuOpen = false;
    menu.classList.add("hidden");
    document.body.classList.remove("menu-open", "menu-below", "menu-above", "menu-right", "menu-left");
    resetResizeOverlay();
    pill.style.width = "";
    pill.style.height = "";
    menuState = null;
  }
  const b1 = boxAtScale(1); // base size: maps cursor distance to scale
  resizing = {
    dirX, dirY,
    L, T, w0: pr.width, h0: pr.height,
    w1: b1.w, h1: b1.h,
    sMax: Math.max(scale, Math.min(screen.availWidth * 0.97 / b1.w, screen.availHeight * 0.97 / b1.h)),
  };
  pill.classList.add("resizing");
  if (!videoEl.classList.contains("hidden")) pauseByApp();
  liveResize(scale); // snap the window to the pill box right away
});

window.addEventListener("mousemove", (e) => {
  if (!resizing) return;
  if (e.buttons === 0) { endResize(); return; }
  const r = resizing;
  const sx = r.dirX > 0 ? (e.screenX - r.L) / r.w1
    : r.dirX < 0 ? (r.L + r.w0 - e.screenX) / r.w1 : null;
  const sy = r.dirY > 0 ? (e.screenY - r.T) / r.h1
    : r.dirY < 0 ? (r.T + r.h0 - e.screenY) / r.h1 : null;
  rzPending = sx != null && sy != null ? Math.max(sx, sy) : sx != null ? sx : sy;
  if (!rzRaf) {
    rzRaf = requestAnimationFrame(() => {
      rzRaf = 0;
      if (resizing && rzPending != null) liveResize(rzPending);
    });
  }
});
window.addEventListener("mouseup", () => { if (resizing) endResize(); });

// Refresh pill when the prompt is edited in the main window.
listen("prompt-updated", (e) => {
  if (e.payload && e.payload.id === promptId) applyPrompt(e.payload);
});

// Pause the looping pill video while this window is hidden.
document.addEventListener("visibilitychange", () => {
  if (videoEl.classList.contains("hidden")) return;
  if (document.hidden) pauseByApp();
  else ensureVideoPlaying();
});

listen("theme-changed", (e) => document.documentElement.setAttribute("data-theme", e.payload));
// Language switched in the settings — re-translate the pill texts in place.
listen("language-changed", () => applySettings());

// Moving onto a monitor with a different DPI can leave a stale frame —
// re-apply the size so the window repaints at the new scale factor.
appWin.onScaleChanged(() => {
  applyPillSize();
  ensureVideoPlaying();
});

(async () => {
  // A transient IPC failure must not leave the pill blank/unstyled — guard each
  // step independently so the rest still runs.
  try {
    document.documentElement.setAttribute("data-theme", await invoke("current_theme"));
  } catch (_) {}
  applySettings().catch(() => {});
  loadName().catch(() => {});
})();
