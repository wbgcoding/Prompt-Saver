// Floating quick-copy pill. Click copies; press-and-move drags the window;
// right-click opens a small menu (size, edit prompt, remove).
import { FLOAT_TXT as TXT } from "./i18n.js";
import { FONTS } from "./fonts.js";
import { mediaKind, buildMediaBar, applyVideoPrefs } from "./media.js";

const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

const appWin = getCurrentWebviewWindow();
const promptId = appWin.label.replace(/^float-/, "");

const pill = document.getElementById("pill");
const label = document.getElementById("label");
const menu = document.getElementById("menu");

let scale = 1;
let tileSize = 0; // global text size setting (0 = auto)
let globalFontKey = "system";
let pFont = ""; // per-prompt overrides ("" / 0 = follow settings)
let pSize = 0;
let menuOpen = false;

function applyFont() {
  pill.style.fontFamily = FONTS[pFont || globalFontKey] || FONTS.system;
}

// Keep in sync with the backend pill constants (FLOAT_H / FLOAT_IMG).
const PILL_H = 80;
const PILL_IMG = 400;
const PILL_FONT_DEFAULT = 26; // text pill font size at scale 1
const PILL_TEXT_MIN_W = 200; // text pill width range; beyond max, ellipsis
const PILL_TEXT_MAX_W = 960;
const PILL_BORDER = 5; // colored frame width around media pills
const SIZE_EPSILON = 2; // px tolerance ending the self-healing resize loop
const FILE_POLL_MS = 5000;
const COPIED_MS = 900;

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
function pillFontSize() {
  // pSize/tileSize: 0 = inherit, 1 = auto-fit (treated as default here).
  const base = pSize > 1 ? pSize : tileSize > 1 ? tileSize : PILL_FONT_DEFAULT;
  return Math.round(base * scale);
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
  label.style.fontSize = `${pillFontSize()}px`;
  return {
    w: Math.min(PILL_TEXT_MAX_W, Math.max(PILL_TEXT_MIN_W, label.scrollWidth + 60)),
    h: PILL_H * scale,
  };
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
  invoke("resize_float_pill", { id: promptId, width: pillBox().w }).catch(() => {});
}

function markSelectedSize() {
  for (const btn of menu.querySelectorAll("[data-scale]")) {
    btn.classList.toggle("sel", Math.abs(Number(btn.dataset.scale) - scale) < 0.01);
  }
}

async function applySettings() {
  const s = await invoke("get_settings");
  const pref = (s.language && s.language !== "auto"
    ? s.language
    : navigator.language || "en"
  ).toLowerCase();
  const lang = Object.keys(TXT).find((c) => pref.startsWith(c)) || "en";
  const t = TXT[lang];
  document.getElementById("feedback").textContent = t.copied;
  document.getElementById("size-label").textContent = t.size;
  document.getElementById("menu-edit").textContent = t.edit;
  document.getElementById("menu-remove").textContent = t.remove;
  const closeBtn = document.getElementById("menu-close");
  closeBtn.title = t.close;
  closeBtn.setAttribute("aria-label", t.close);
  document.getElementById("file-error").textContent = t.missing;
  applyVideoPrefs(videoEl, s.video_prefs && s.video_prefs[promptId]);
  scale = (s.float_scale && s.float_scale[promptId]) || 1;
  tileSize = Number(s.tile_size) || 0;
  globalFontKey = s.tile_font || "system";
  applyFont();
  markSelectedSize();
  applyPillSize();
}

// Self-healing size: DPI handoffs during cross-monitor drags can leave the
// window at a wrong size — whenever it differs from the pill's real box,
// snap it back. The ±2px tolerance ends the loop once it matches.
function enforcePillSize() {
  // No resize ping-pong while the OS moves the window; mouseup re-applies.
  if (menuOpen || pill.classList.contains("dragging")) return;
  if (pill.classList.contains("has-image") && !imgRatio) return;
  const box = pillBox();
  if (Math.abs(window.innerWidth - box.w) > SIZE_EPSILON ||
      Math.abs(window.innerHeight - box.h) > SIZE_EPSILON) {
    applyPillSize();
  }
}

let fitRaf = 0;
window.addEventListener("resize", () => {
  cancelAnimationFrame(fitRaf);
  fitRaf = requestAnimationFrame(enforcePillSize);
});

const DRAG_THRESHOLD = 4;
let drag = null; // {x, y, moved}
let copiedTimer = null;

// Width/height ratio of the pill image (0 = text pill); the active frame
// width is part of the window box (see pillBox).
let imgRatio = 0;
let borderW = 0;

// Apply name, color and media (stored still, gif or video) onto the pill.
const videoEl = document.getElementById("video");

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
  if (videoEl.classList.contains("hidden") || document.hidden || userPaused || menuOpen || drag) {
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
    // Longhand only: the `background` SHORTHAND would write an inline
    // background-size that overrides the class' exact-fill 100% 100%.
    pill.style.background = "";
    pill.style.backgroundColor = "transparent";
    // The chosen color frames the media; the window hugs the media box.
    borderW = p.color ? PILL_BORDER : 0;
    pill.style.border = p.color ? `${PILL_BORDER}px solid ${p.color}` : "none";
    const onRatio = (ratio) => {
      imgRatio = ratio || 1;
      applyPillSize(); // resize the window to the media's aspect ratio
    };
    if (pathMedia && kind === "video") {
      pill.style.backgroundImage = "";
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
      pill.style.backgroundImage = `url(${src})`;
      const im = new Image();
      im.onload = () => onRatio(im.naturalWidth / im.naturalHeight);
      im.src = src;
    }
  } else {
    videoEl.classList.add("hidden");
    videoEl.removeAttribute("src");
    pill.classList.remove("has-image");
    pill.style.backgroundImage = "";
    pill.style.background = p.color || "";
    pill.style.border = "";
    imgRatio = 0;
    borderW = 0;
  }
  hasFile = !!(p.file_path || p.icon_path);
  pFont = p.font || "";
  pSize = p.font_size || 0;
  applyFont();
  applyPillSize();
  pollFileMissing();
}

async function loadName() {
  const p = await invoke("get_prompt", { id: promptId });
  if (p) applyPrompt(p);
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

async function openMenu() {
  if (menuOpen) return;
  menuOpen = true;
  mediaBar.classList.add("hidden");
  const w = window.innerWidth;
  const h = window.innerHeight;
  lockPillBox(w, h);
  document.body.classList.add("menu-open");
  await invoke("resize_float_menu", { id: promptId, open: true, width: w, height: h });
  menu.classList.remove("hidden");
}

async function closeMenu() {
  if (!menuOpen) return;
  menuOpen = false;
  menu.classList.add("hidden");
  document.body.classList.remove("menu-open");
  pill.style.width = "";
  pill.style.height = "";
  // One resize straight to the pill's real box — no jump via the defaults.
  const box = pillBox();
  await invoke("resize_float_menu", {
    id: promptId, open: false, width: box.w, height: box.h,
  });
  ensureVideoPlaying();
}

// Clicking the pill area (not the menu) closes an open menu instead of
// copying — same window, so no blur fires.
let swallowPress = false;
window.addEventListener("pointerdown", (e) => {
  if (menuOpen && !menu.contains(e.target)) {
    swallowPress = true; // this press only closes the menu
    closeMenu();
  }
}, true);

window.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  openMenu();
});
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") closeMenu();
});
window.addEventListener("blur", closeMenu);

for (const btn of menu.querySelectorAll("[data-scale]")) {
  btn.addEventListener("click", async () => {
    scale = Number(btn.dataset.scale) || 1;
    markSelectedSize();
    // Persist the factor without touching the window (no size jump), then
    // grow the window around the pill's new box — the menu stays open and
    // the change is visible immediately.
    await invoke("set_float_scale", { id: promptId, scale, resize: false });
    const box = pillBox();
    lockPillBox(box.w, box.h);
    await invoke("resize_float_menu", { id: promptId, open: true, width: box.w, height: box.h });
    ensureVideoPlaying();
  });
}

document.getElementById("menu-close").addEventListener("click", () => closeMenu());

document.getElementById("menu-edit").addEventListener("click", async () => {
  await closeMenu();
  await invoke("edit_prompt_request", { id: promptId });
});

document.getElementById("menu-remove").addEventListener("click", () => {
  invoke("toggle_floating", { id: promptId }); // closes this window
});

// ---- Drag / copy ----
pill.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  if (swallowPress) {
    swallowPress = false;
    return;
  }
  drag = { x: e.screenX, y: e.screenY, moved: false };
});

window.addEventListener("mousemove", async (e) => {
  // Lost mouseup (can happen when the OS swallows it after a window drag):
  // no button is held anymore, so end the drag state and recover playback.
  if (drag && e.buttons === 0) {
    drag = null;
    pill.classList.remove("dragging");
    applyPillSize();
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
  await appWin.startDragging(); // OS takes over; position saved in backend on move
});

window.addEventListener("mouseup", async () => {
  swallowPress = false; // a press never outlives its own click
  if (!drag) return;
  const wasDrag = drag.moved;
  drag = null;
  pill.classList.remove("dragging");
  if (!wasDrag) {
    if (await invoke("copy_prompt", { id: promptId }).catch(() => false)) showCopied();
  } else {
    applyPillSize(); // clean repaint after a (cross-monitor) drag
    ensureVideoPlaying();
  }
});

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
  document.documentElement.setAttribute("data-theme", await invoke("current_theme"));
  applySettings();
  loadName();
})();
