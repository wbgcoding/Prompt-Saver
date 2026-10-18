// Floating quick-copy pill. Click copies; press-and-move drags the window;
// right-click opens a small menu (size, edit prompt, remove).
import { FONTS, fitText } from "./fonts.js";
import { mediaKind, buildMediaBar, applyVideoPrefs, setMediaDefaults } from "./media.js";
// Custom themes: shared with the folder window (floattheme.js).
import { applyFloatTheme, inkForColor, onInkChange } from "./floattheme.js";
import { APP_NAME } from "./brand.js";
import { LANGS, compactTexts } from "./i18n.js";
import { DRAG_THRESHOLD_DEFAULT } from "./copyDefaults.js";
import { COPIED_MS, RING_MIN, RING_MAX, DRAG_OUT_MODS, resolveLang, fileUri, playCopySound, applyCopySizeLevel, markSelectedSize, readCopyPrefs, copyGate, needsMainWindowCopy } from "./floatShared.js";

// Frameless window: the title is invisible in the app but shows in the Alt-Tab switcher, so
// it comes from the brand.
document.title = APP_NAME;

const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen, emit } = window.__TAURI__.event;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

const appWin = getCurrentWebviewWindow();
let copiedText = ""; // localized "Copied!", re-announced on every copy (see showCopied)
const promptId = appWin.label.replace(/^float-/, "");

const pill = document.getElementById("pill");
const label = document.getElementById("label");
const menu = document.getElementById("menu");
const resizeEl = document.getElementById("resize");
const rzCatch = document.getElementById("rz-catch");
const parkedBadge = document.getElementById("parked-badge");

let scale = 1;
let tileSize = 0; // global text size setting (0 = auto)
let globalFontKey = "system";
let copySizeLevel = "large"; // "Copied!" text size level: small, medium or large (large fills the pill)
let pFont = ""; // per-prompt overrides ("" / 0 = follow settings)
let copyPrefs = readCopyPrefs({}); // cooldown, double-click paste and copy sound (expert values)
let videoControlsOn = true; // gates the hover media bar, mirrors flag("videoControls") in core.js
let videoAutoplayOn = false; // gates the initial auto-start, mirrors optFlag("videoAutoplay") in core.js
let pSize = 0;
let pCaptionSize = 0; // per-prompt caption size (0 = default, 1 = auto)
let menuOpen = false;
// Drag-out (default off): the modifier separates "drag the prompt into another app" from the
// plain drag, which moves this window.
let dragOutOn = false, dragOutMod = "ctrlKey";
let foldersOn = false; // pill folders; applyPrefs turns it on unless the setting says off
let pText = "", pFile = "", pVariants = [];
const dragOutHeld = (e) => dragOutOn && e[dragOutMod] === true;

// Rotating prompt: "next entry" badge. A pill window holds one prompt, so a single cached
// {index,total} suffices; kept live by the backend's rotation-changed event.
let rotateBadgeOn = true; // mirrors flag("rotateBadge"), which ships on
let rotateVariants = 0;
let rotateInfo = null; // { index, pos, total } once known
let rotateReq = 0; // bumped on every primeRotateBadge call, so an in-flight peek
                    // A superseded primeRotateBadge call's in-flight peek must not overwrite a newer result.
const rotateBadgeEl = document.getElementById("rotate-badge");

function renderRotateBadge() {
  const show = rotateBadgeOn && rotateVariants > 0 && !!rotateInfo;
  rotateBadgeEl.classList.toggle("hidden", !show);
  // Pos (not index + 1): in random mode index is the absolute entry drawn next and jumps
  // around; pos is the 1-based draw number within the shuffle cycle.
  if (show) rotateBadgeEl.textContent = `${rotateInfo.pos}/${rotateInfo.total}`;
}

function primeRotateBadge() {
  rotateInfo = null;
  const req = ++rotateReq;
  if (!rotateVariants || !rotateBadgeOn) { renderRotateBadge(); return; } // no peek while off
  invoke("rotation_peek", { id: promptId }).then((r) => {
    if (!r || req !== rotateReq) return;
    rotateInfo = { index: r.index, pos: r.pos, total: r.total };
    renderRotateBadge();
  }).catch(() => {});
  renderRotateBadge(); // hide immediately; primeRotateBadge's own peek fills it in
}

function applyFont() {
  pill.style.fontFamily = FONTS[pFont || globalFontKey] || FONTS.system;
}

// Keep in sync with the backend pill constants (FLOAT_H / FLOAT_IMG, FLOAT_SCALE_MIN /
// FLOAT_SCALE_MAX).
const SCALE_MIN = 0.3;
const SCALE_MAX = 8;
const PILL_H = 80;
const PILL_IMG = 400;
const PILL_FONT_SIZE_DEFAULT = 26; // text pill font size at scale 1
const PILL_TEXT_MIN_W = 200; // smallest text-pill width; grows with the label
const PILL_BORDER_W = 5; // colored frame width around media pills
const SIZE_EPSILON = 2; // px tolerance ending the self-healing resize loop
const PILL_RING_FRAC = 0.18; // ring thickness as a fraction of the shorter window side
// pillTextFontBase, not pillFontSize: that name is the function below.
let pillTextFontBase = PILL_FONT_SIZE_DEFAULT;

// Resizes and visibility flips can leave the looping video paused; a visible video must play
// again afterwards. Only a pause made through the control bar (or a play-once end) is kept.
function ensureVideoPlaying() {
  if (userPaused) return;
  // A video that never started because videoAutoplay is off stays paused until the user starts
  // it; once it has played, app-driven pauses (drag, hidden window) auto-resume.
  if (!videoAutoplayOn && !videoEverPlayed) return;
  if (!videoEl.classList.contains("hidden") && videoEl.paused) {
    videoEl.play().catch(() => {});
  }
}

// Text pills use a fixed, readable font size; the window width grows with the label (capped,
// then ellipsis).
function pillFontSize(effScale = scale) {
  // pSize/tileSize: 0 = inherit, 1 = auto-fit (treated as default here).
  const base = pSize > 1 ? pSize : tileSize > 1 ? tileSize : pillTextFontBase;
  return Math.round(base * effScale);
}

// Current pill box at the active scale. Media pills follow their aspect ratio exactly
// (longest side = PILL_IMG, plus the colored frame), so the picture fills the button without
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
  // Font tracks the actual window height, not the raw scale: if the window is capped (OS size
  // limit, DPI handoff) the text stops growing instead of ballooning past the frozen pill. When
  // the box grows normally innerHeight equals h; min() guards a stale-large innerHeight
  // (mid-shrink).
  const effH = Math.min(h, window.innerHeight || h);
  label.style.fontSize = `${pillFontSize(effH / PILL_H)}px`;
  // Width hugs the label. Padding and the min width scale with the pill so it shrinks and grows
  // together with the text; floored to a pill ratio so it never rounds into a circle.
  const padX = Math.round(30 * scale);
  const minW = Math.round(PILL_TEXT_MIN_W * scale);
  const w = Math.max(minW, label.scrollWidth + 2 * padX, Math.round(h * RATIO));
  return { w, h };
}

function applyPillSize() {
  if (menuOpen) return;
  if (pill.classList.contains("has-image")) {
    if (!imgRatio) return;
    const box = pillBox();
    invoke("resize_float_media", { id: promptId, width: box.w, height: box.h }).catch(() => {});
    return;
  }
  const box = pillBox();
  invoke("resize_float_pill", { id: promptId, width: box.w, height: box.h }).catch(() => {});
}

// Behaviour (cooldown, opacity, folders, drag-out) is applied before the labels, so a failure
// while building the texts never takes it away.
async function applySettings() {
  const s = await invoke("get_settings");
  applyPrefs(s);
  applyTexts(s);
}

function applyTexts(s) {
  const lang = resolveLang(s.language, LANGS);
  document.documentElement.lang = lang;
  const t = compactTexts(lang);
  copiedText = t.copied;
  document.getElementById("feedback").textContent = copiedText;
  document.getElementById("size-label").textContent = t.size;
  // Localized tooltip for the S/M/L size buttons (visible letters stay universal).
  for (const b of document.querySelectorAll(".size-row [data-scale]")) {
    b.title = `${t.size} ${b.textContent}`;
    b.setAttribute("aria-label", b.title);
  }
  document.getElementById("menu-edit").textContent = t.edit;
  document.getElementById("menu-hide").textContent = t.hideForNow;
  document.getElementById("menu-remove").textContent = t.remove;
  const closeBtn = document.getElementById("menu-close");
  closeBtn.title = t.close;
  closeBtn.setAttribute("aria-label", t.close);
  document.getElementById("file-error").textContent = t.missing;
  parkedBadge.title = t.parked;
  mediaBar.setLabels({
    play: t.mediaPlay, pause: t.mediaPause,
    loopOn: t.mediaLoopOn, loopOff: t.mediaLoopOff,
    mute: t.mediaMute, unmute: t.mediaUnmute,
  });
}

function applyPrefs(s) {
  applyVideoPrefs(videoEl, s.video_prefs && s.video_prefs[promptId]);
  // Pills apply the expert videoVolume/videoMuted/videoLoop settings with the same defaults
  // applyValues() in expertMenu.js pushes for the grid tiles.
  const vol = s.ui_values?.videoVolume;
  // 0 is a valid volume (the expert slider starts at 0), so only a missing value falls back.
  setMediaDefaults({ volume: vol != null && Number.isFinite(Number(vol)) ? Number(vol) : 100, muted: s.ui_flags?.videoMuted !== false, looped: s.ui_flags?.videoLoop === true });
  videoAutoplayOn = s.ui_flags?.videoAutoplay === true; // default off, matches optFlag("videoAutoplay")
  videoControlsOn = s.ui_flags?.videoControls !== false; // default on, matches flag("videoControls")
  if (!videoControlsOn) mediaBar.classList.add("hidden"); // off mid-hover must not leave it stuck open
  // Clamp to the backend's stored range (SCALE_MIN to SCALE_MAX): a corrupt 0/negative/NaN
  // factor would give a degenerate window, and a range differing from the backend's would snap
  // the "L" preset and freely resized pills back on the next start.
  const rawScale = Number(s.float_scale && s.float_scale[promptId]);
  scale = Number.isFinite(rawScale) && rawScale > 0 ? Math.min(SCALE_MAX, Math.max(SCALE_MIN, rawScale)) : 1;
  tileSize = Number(s.tile_size) || 0;
  globalFontKey = s.tile_font || "system";
  copySizeLevel = s.ui_texts?.copySizeLevel;
  copyPrefs = readCopyPrefs(s);
  dragOutOn = s.ui_flags?.dragOut === true;
  dragOutMod = DRAG_OUT_MODS[s.ui_texts?.dragOutModifier] || "ctrlKey";
  foldersOn = s.ui_flags?.folders !== false; // default ON, mirrors folders_on() in floating.rs
  // rotateBadge is a sub-toggle of rotatingPrompts: the parent off freezes the whole feature,
  // not just the badge.
  rotateBadgeOn = s.ui_flags?.rotateBadge !== false && s.ui_flags?.rotatingPrompts !== false;
  // primeRotateBadge, not renderRotateBadge alone: rotateInfo stays null while the badge is off
  // (no peek while off), so flipping the flag back on needs a fresh peek.
  primeRotateBadge();
  const op = Number(s.ui_values?.floatOpacity);
  pill.style.opacity = Number.isFinite(op) ? `${Math.max(0.2, op / 100)}` : "";
  const pfs = Number(s.ui_values?.pillFontSize);
  pillTextFontBase = Number.isFinite(pfs) ? pfs : PILL_FONT_SIZE_DEFAULT;
  applyFont();
  markSelectedSize(menu, scale);
  applyPillSize();
}

// Self-healing size: DPI handoffs during cross-monitor drags can leave the window at a wrong
// size; whenever it differs from the pill's real box, snap it back. The ±2px tolerance ends
// the loop.
function enforcePillSize() {
  // No resize ping-pong while the OS moves the window, the user resizes it, or the
  // merge preview is holding the window at its grown size.
  if (menuOpen || mergeHint || pill.classList.contains("dragging") || pill.classList.contains("resizing")) return;
  if (pill.classList.contains("has-image") && !imgRatio) return;
  const box = pillBox();
  if (Math.abs(window.innerWidth - box.w) > SIZE_EPSILON ||
      Math.abs(window.innerHeight - box.h) > SIZE_EPSILON) {
    applyPillSize();
  }
}

// Size the resize-grip band in proportion to the pill so the hitbox sits at the rounded edge:
// thin on a small pill, a comfortable ring on a big one, with the inner area free for
// dragging and copying.
function updateResizeRing() {
  const ring = Math.max(RING_MIN, Math.min(RING_MAX, Math.round(Math.min(window.innerWidth, window.innerHeight) * PILL_RING_FRAC)));
  resizeEl.style.setProperty("--rz", `${ring}px`);
}

let fitRaf = 0;
window.addEventListener("resize", () => {
  cancelAnimationFrame(fitRaf);
  fitRaf = requestAnimationFrame(() => { enforcePillSize(); updateResizeRing(); });
});
updateResizeRing();

const DRAG_THRESHOLD = DRAG_THRESHOLD_DEFAULT;
let drag = null; // {x, y, moved}
let copiedTimer = null;

// Width/height ratio of the pill image (0 = text pill); the active frame width is part of the
// window box (see pillBox).
let imgRatio = 0;
let borderW = 0;

// The own-colour ink on the text pill, remembered so a changed ink rule can repaint it.
let ownColor = "";
function paintOwnInk(color) {
  ownColor = color || "";
  const ink = inkForColor(ownColor);
  // The pill's text and overlays read these variables, so setting them on the pill overrides
  // the theme's ink inside it.
  if (ink) {
    pill.style.setProperty("--pill-fg", ink.fg);
    pill.style.setProperty("--pill-ink-shadow", ink.shadow);
  } else {
    pill.style.removeProperty("--pill-fg");
    pill.style.removeProperty("--pill-ink-shadow");
  }
}
onInkChange(() => {
  if (ownColor) paintOwnInk(ownColor);
});

// Apply name, color and media (stored still, gif or video) onto the pill.
const videoEl = document.getElementById("video");
const imgEl = document.getElementById("img");
// floating.html hardcodes the autoplay attribute. Strip it before any source is assigned so
// videoAutoplay=off is honoured like in the main grid (explicit play() instead of the
// unreliable attribute).
videoEl.removeAttribute("autoplay");

// ---- Video controls + freeze watchdog ----
// Pause bookkeeping: only pauses made through the control bar (or a play-once end) stay
// paused; every pause the app makes (drag, hidden window) is resumed automatically.
let appPause = false;
let userPaused = false;
// True once this loaded source has started playing: distinguishes "never started because
// autoplay is off" (stay paused) from "already playing or app-paused" (auto-resume).
let videoEverPlayed = false;
videoEl.addEventListener("pause", () => { userPaused = !appPause; appPause = false; });
videoEl.addEventListener("play", () => { userPaused = false; appPause = false; videoEverPlayed = true; });

function pauseByApp() {
  if (!videoEl.paused) {
    appPause = true;
    videoEl.pause();
  }
}

// Hover control bar (shared with the grid tiles). Lives next to the pill: nested inside the
// <button> pill it would be invalid HTML.
const mediaBar = buildMediaBar(videoEl, {
  onChange: (prefs) => invoke("set_video_prefs", { id: promptId, ...prefs }).catch(() => {}),
});
mediaBar.classList.add("hidden");
document.body.appendChild(mediaBar);

// The bar appears while the cursor is in the lower zone of a video pill and stays while the
// bar itself (incl. the volume popup above it) is used.
const volDragging = () => !!mediaBar.querySelector(".media-sound.dragging");
window.addEventListener("mousemove", (e) => {
  const zone = Math.max(48, window.innerHeight * 0.35);
  const show = videoControlsOn && !videoEl.classList.contains("hidden") && !menuOpen &&
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

// Worst-case recovery: the pill video must never stay frozen. If playback is suspended
// without a user pause, resume it; if it reports "playing" but the position stops advancing
// (decoder stalled after a DPI handoff or window move), reload the source and continue where
// it stopped.
const STALL_POLL_MS = 3000;
let lastVideoTime = -1;
setInterval(() => {
  // Never touch the video while the window is dragged: resuming playback mid-drag makes the
  // drag stutter.
  if (videoEl.classList.contains("hidden") || document.hidden || userPaused || menuOpen || drag || resizing) {
    lastVideoTime = -1;
    return;
  }
  // A play-once video that reached its end is finished, not suspended.
  if (videoEl.ended && !videoEl.loop) {
    lastVideoTime = -1;
    return;
  }
  if (videoEl.paused) {
    ensureVideoPlaying(); // respects videoAutoplay for a not-yet-started video
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
  pCaptionSize = p.caption_size || 0;
  updateCaptionScale();
  if (showMedia) {
    pill.classList.add("has-image");
    pill.style.background = "transparent";
    paintOwnInk("");
    // The chosen color frames the media; the window hugs the media box.
    borderW = p.color ? PILL_BORDER_W : 0;
    pill.style.border = p.color ? `${PILL_BORDER_W}px solid ${p.color}` : "none";
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
        videoEverPlayed = false; // a new source must re-honour videoAutoplay
        userPaused = false; // and an old user pause does not carry over to it
        videoEl.onloadedmetadata = () =>
          onRatio(videoEl.videoWidth / videoEl.videoHeight);
      } else if (videoEl.videoWidth) {
        onRatio(videoEl.videoWidth / videoEl.videoHeight);
      }
      videoEl.classList.remove("hidden");
      ensureVideoPlaying();
    } else {
      videoEl.classList.add("hidden");
      pauseByApp(); // not a user pause: a later video source starts again
      videoEl.removeAttribute("src");
      videoEl.load(); // the attribute alone neither stops playback nor frees the file
      const src = p.image || convertFileSrc(showPath);
      imgEl.onload = () => onRatio(imgEl.naturalWidth / imgEl.naturalHeight);
      imgEl.onerror = () => onRatio(1); // broken image: still size the pill
      imgEl.src = src;
      imgEl.classList.remove("hidden");
    }
  } else {
    videoEl.classList.add("hidden");
    pauseByApp();
    videoEl.removeAttribute("src");
    videoEl.load();
    imgEl.classList.add("hidden");
    imgEl.removeAttribute("src");
    pill.classList.remove("has-image");
    pill.style.background = p.color || "";
    paintOwnInk(p.color);
    pill.style.border = "";
    imgRatio = 0;
    borderW = 0;
  }
  hasFile = !!(p.file_path || p.icon_path);
  pText = p.text || "";
  pFile = p.file_path || "";
  pVariants = p.variants || [];
  rotateVariants = (p.variants || []).length;
  primeRotateBadge();
  pFont = p.font || "";
  pSize = p.font_size || 0;
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
// The ambient refresh is a backend-pushed "missing-files" event (listened for below), so one
// check covers every open pill. pollFileMissing stays for the one-off "recheck now" call in
// applyPrompt.
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
listen("missing-files", (e) => {
  if (hasFile) pill.classList.toggle("file-missing", (e.payload || []).includes(promptId));
});

function showCopied() {
  const fb = document.getElementById("feedback");
  fitText(fb, pill.clientWidth - 10, pill.clientHeight - 6, 120);
  applyCopySizeLevel(fb, copySizeLevel);
  // Cleared and reset rather than left alone: the text never changed since the pill was made,
  // and a screen reader needs a real mutation to announce a second copy in a row.
  fb.textContent = "";
  fb.textContent = copiedText;
  pill.classList.add("copied");
  clearTimeout(copiedTimer);
  copiedTimer = setTimeout(() => pill.classList.remove("copied"), COPIED_MS);
  if (copyPrefs.copySoundOn) playCopySound(copyPrefs.copySoundVolume);
}

// ---- Right-click menu ----
// The window grows below the pill, which stays visible, so size changes preview live while
// the menu is open.
function lockPillBox(w, h) {
  pill.style.width = `${w}px`;
  pill.style.height = `${h}px`;
}

const MENU_GAP = 6;
let menuState = null; // { px, py, side }: pill screen pos + chosen side

// With the menu open the window grows past the pill, so pin the resize grips to the pill's
// box (offset offX/offY inside the window); edges and corners stay on the pill, never on the
// menu's far edge. Reset to full-window on close.
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
  // offX/offY = the pill's offset inside the window, so the pill can be put back exactly on
  // close (even after the window was dragged).
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
  invoke("set_float_bounds", { id: promptId, x, y, width: w, height: h, clamp: true }).catch(() => {});
}

async function openMenu() {
  if (menuOpen) return;
  menuOpen = true;
  mediaBar.classList.add("hidden");
  // Lock the pill to its current on-screen size (not a fresh measurement) so opening the menu
  // never nudges its size.
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
  // closeMenu() may have run while we awaited outerPosition (Escape/blur); respect it instead
  // of reopening with a stale menuState.
  if (!menuOpen) return;
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
  // Restore the window to just the pill at its current screen position (it may have been
  // dragged while the menu was open). Shrink first, with the pill still locked, so it never
  // stretches to the larger box.
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
window.addEventListener("keydown", async (e) => {
  if (e.key === "Escape") { closeMenu(); return; }
  // Moves the whole pill window like a mouse drag; the pill is a plain OS window with no
  // keyboard drag gesture otherwise.
  if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key) && !menuOpen && !resizing) {
    e.preventDefault();
    const step = e.shiftKey ? 20 : 4;
    const dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0;
    const dy = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
    try {
      const p = await appWin.outerPosition();
      const d = window.devicePixelRatio || 1;
      await invoke("set_float_bounds", {
        id: promptId, x: p.x / d + dx, y: p.y / d + dy,
        width: window.innerWidth, height: window.innerHeight, clamp: true,
      });
    } catch {}
  }
});
// Closing on blur, but not while dragging (a drag must keep the menu attached).
window.addEventListener("blur", () => { if (!drag) closeMenu(); });

for (const btn of menu.querySelectorAll("[data-scale]")) {
  btn.addEventListener("click", async () => {
    scale = Number(btn.dataset.scale) || 1;
    markSelectedSize(menu, scale);
    await invoke("set_float_scale", { id: promptId, scale, resize: false }).catch(() => {});
    // closeMenu (Escape) may have run while the scale was being stored and already restored the
    // window.
    if (!menuOpen) return;
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

document.getElementById("menu-hide").addEventListener("click", () => {
  invoke("hide_float_for_now", { id: promptId }).catch(() => {}); // closes this window, until restart
});

document.getElementById("menu-remove").addEventListener("click", () => {
  invoke("toggle_floating", { id: promptId }).catch(() => {}); // closes this window
});

// ---- Drag / copy ----
// The whole pill window is the drag/copy surface; the menu buttons and resize grips handle
// their own presses.
window.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  if (e.target.closest(".menu") || e.target.closest(".rz-catch")) return;
  // The modifier hands the press to the OS drag loop instead of moving the window. A
  // modifier-click that never moves still copies.
  const out = dragOutHeld(e);
  pill.draggable = out;
  drag = { x: e.screenX, y: e.screenY, moved: false, out };
});

// Same payload rules as the main window: raw text, placeholders as written (a fill-in dialog
// cannot open while the OS drag loop holds the pointer).
pill.addEventListener("dragstart", (e) => {
  const text = pFile || pText;
  if (!dragOutOn || !text) { e.preventDefault(); return; }
  e.dataTransfer.effectAllowed = "copy";
  e.dataTransfer.setData("text/plain", text);
  if (pFile) e.dataTransfer.setData("text/uri-list", fileUri(pFile));
  drag = null;
});
pill.addEventListener("dragend", () => { pill.draggable = false; });

// One call per drag end: the pills are separate windows, so only the backend can tell whether
// this drop landed on another one. Nothing is sent while dragging; a drag ending away from a
// pill gets `false`.
const mergeAfterDrop = () => {
  if (!foldersOn) return;
  // With the menu open nothing merges, but the highlight still has to go out.
  if (menuOpen) {
    invoke("float_drag_hint", { id: promptId, active: false }).catch(() => {});
    return;
  }
  invoke("float_folder_drop", { id: promptId }).catch(() => {});
};

// startDragging() hands the window to the OS move loop, which consumes the button release: a
// dragged pill never sees `mouseup`. So the release is also recognised from the first later
// event that reports no button held, and the drag ends (and merges) there.
function pointerReleased(e) {
  if (!drag || e.buttons !== 0) return false;
  const wasDrag = drag.moved;
  drag = null;
  pill.classList.remove("dragging");
  if (!menuOpen) applyPillSize();
  ensureVideoPlaying();
  if (wasDrag) mergeAfterDrop();
  return true;
}

// Leaving the window counts as well: after a swallowed release the pointer may leave the pill
// before moving inside it again.
window.addEventListener("mouseout", (e) => pointerReleased(e));

window.addEventListener("mousemove", async (e) => {
  if (pointerReleased(e)) return;
  if (!drag || drag.moved || drag.out) return;
  if (Math.abs(e.screenX - drag.x) < DRAG_THRESHOLD &&
      Math.abs(e.screenY - drag.y) < DRAG_THRESHOLD) return;
  drag.moved = true;
  pill.classList.add("dragging");
  // The backend follows this window while the OS moves it and lights up whatever letting go
  // would merge into. Nothing is computed here; the pointer belongs to the move loop from the
  // next line on.
  if (foldersOn) invoke("float_drag_hint", { id: promptId, active: true }).catch(() => {});
  // Decoding video frames while the OS moves the window makes the drag stutter; pause for the
  // drag.
  if (!videoEl.classList.contains("hidden")) pauseByApp();
  // The whole window moves: an open menu rides along with the pill.
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
    mergeAfterDrop();
  } else {
    await activatePill();
  }
});

// A click on the pill closes an open menu, otherwise copies. Enter and Space on the focused
// pill raise a click with no pointer (detail 0) and take the same path; a mouse click is
// handled on mouseup above.
async function activatePill() {
  if (menuOpen) {
    closeMenu(); // a click on the pill closes the open menu
    return;
  }
  // The gate (cooldown, double-click paste window, stamp before the await, rollback) is shared
  // with the folder window. A prompt with a placeholder is handed to the main window, which
  // asks and copies; that route never pastes on its own.
  const r = await copyGate(promptId, async () => {
    if (needsMainWindowCopy(pText, pVariants)) {
      return emit("prompt-tray-copy", promptId).then(() => "handoff", () => false);
    }
    return invoke("copy_prompt", { id: promptId }).catch(() => false);
  }, copyPrefs);
  if (r === "paste") {
    // Fast second click pastes the just-copied text into the last app.
    invoke("paste_into_previous", { delayMs: copyPrefs.apDelay, enter: copyPrefs.apEnter }).catch(() => {});
  } else if (r === "copied") {
    showCopied();
    invoke("record_copy", { id: promptId }).catch(() => {});
  }
}
pill.addEventListener("click", (e) => { if (e.detail === 0) activatePill(); });

// ---- Edge/corner resize ----
// The grabbed edge or corner follows the cursor 1:1 while the opposite side stays put.
// Content scales with the window (aspect kept), up to nearly full screen.
let resizing = null;
let rzRaf = 0;
let rzPending = null;

function updateCaptionScale() {
  document.getElementById("caption").style.fontSize = pCaptionSize > 1 ? `${Math.round(pCaptionSize * scale)}px` : "";
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
  await invoke("set_float_scale", { id: promptId, scale, resize: false }).catch(() => {});
  markSelectedSize(menu, scale);
  ensureVideoPlaying();
}

// Which third of the pill the pointer is in picks the resize anchor; shared by the cursor
// hint and the mousedown handler so both agree on edge vs corner.
function rzDir(e) {
  const W = window.innerWidth || 1, H = window.innerHeight || 1;
  const dx = e.clientX < W / 3 ? -1 : e.clientX > (2 * W) / 3 ? 1 : 0;
  const dy = e.clientY < H / 3 ? -1 : e.clientY > (2 * H) / 3 ? 1 : 0;
  return { dx, dy };
}
// Cursor matches the grabbed segment: straight edges get the axis arrow (ns/ew), only the
// rounded corners a diagonal one.
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

// Grab the stadium ring to resize. Which third of the pill was grabbed picks the anchor (the
// opposite edge stays put); for the uniform pill scale that only sets which way it grows.
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
  const p = e.payload;
  if (!p || p.id !== promptId) return;
  if (!p.secret) { applyPrompt(p); return; }
  // The event carries the stored (sealed) text of a confidential entry: the pill reloads it
  // opened, and keeps its current text when the vault is locked.
  invoke("get_prompt", { id: promptId })
    .then((plain) => { if (plain) applyPrompt(plain); })
    .catch(() => applyPrompt({ ...p, text: pText, variants: pVariants }));
});
// This pill's own rotation pointer moved (any window may have copied it). A pushed event is
// fresher than any in-flight peek; bump rotateReq so a slow earlier primeRotateBadge() peek
// cannot overwrite it.
listen("rotation-changed", (e) => {
  if (e.payload && e.payload.id === promptId) {
    rotateReq++;
    rotateInfo = { index: e.payload.index, pos: e.payload.pos, total: e.payload.total };
    renderRotateBadge();
  }
});

// Pause the looping pill video while this window is hidden.
document.addEventListener("visibilitychange", () => {
  if (videoEl.classList.contains("hidden")) return;
  if (document.hidden) pauseByApp();
  else ensureVideoPlaying();
});

// Another pill is being dragged over this one: show what letting go would do. The window
// grows a little (down-right, so no window Moved event fires and the stored position does not
// creep with every hint), making the pill swell as the target. The exact old box is restored
// on the way out, so a clamp at a screen edge cannot leave it shifted.
let mergeHint = false;
let mergeHintBox = null;
const MERGE_GROW = 1.12;
listen("merge-hint", async (e) => {
  const on = e.payload === true;
  pill.classList.toggle("merge-hint", on);
  if (on === mergeHint) return;
  mergeHint = on;
  if (on) {
    if (menuOpen || resizing) { mergeHint = false; return; }
    let x, y;
    try {
      const p = await appWin.outerPosition();
      const d = window.devicePixelRatio || 1;
      x = p.x / d;
      y = p.y / d;
    } catch { mergeHint = false; return; }
    // A merge-hint-off event may have landed while we awaited outerPosition; respect it instead
    // of growing the window into a state nothing will shrink back from.
    if (!mergeHint) return;
    const w = window.innerWidth, h = window.innerHeight;
    mergeHintBox = { x, y, w, h };
    invoke("set_float_bounds", {
      id: promptId, x, y,
      width: Math.round(w * MERGE_GROW), height: Math.round(h * MERGE_GROW),
    }).catch(() => {});
  } else if (mergeHintBox) {
    const b = mergeHintBox;
    mergeHintBox = null;
    invoke("set_float_bounds", { id: promptId, x: b.x, y: b.y, width: b.w, height: b.h })
      .catch(() => {});
  }
});

listen("theme-changed", (e) => applyFloatTheme(e.payload?.theme, e.payload?.vars));
// Language switched in the settings: re-translate the pill texts in place.
listen("language-changed", () => applySettings().catch(() => {}));
// An expert setting changed: re-read it here too, or a feature switched on while this pill is
// on screen would stay off until the next restart.
listen("settings-changed", () => {
  applySettings().catch(() => {});
  refreshParkedBadge();
});

// Moving onto a monitor with a different DPI can leave a stale frame:
// re-apply the size so the window repaints at the new scale factor.
appWin.onScaleChanged(() => {
  applyPillSize();
  ensureVideoPlaying();
});

// Whether the pill sits on a stand-in spot instead of its own: asked at boot and
// again after the recall action moved it back.
function refreshParkedBadge() {
  invoke("is_float_parked", { id: promptId }).then((parked) => {
    parkedBadge.classList.toggle("hidden", !parked);
  }).catch(() => {});
}

(async () => {
  // A transient IPC failure must not leave the pill blank: guard each step independently.
  try {
    applyFloatTheme(await invoke("current_theme"), await invoke("theme_vars"));
  } catch (_) {}
  applySettings().catch(() => {});
  loadName().catch(() => {});
  // Checked at boot and after a settings change: the window only opens freshly placed (parked
  // or on its remembered spot), so only the float-recall action changes this mid-session.
  refreshParkedBadge();
})();
