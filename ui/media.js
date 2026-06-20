// Attached-media detection shared by the grid and the floating pills.
// Stills get a stored preview, gifs/videos render straight from their path.
// Still images: every raster format the backend `image` crate can decode
// (re-encoded to a stored preview, so WebView2 always renders it).
export const IMAGE_EXT = /\.(png|apng|jpe?g|jfif|pjpe?g|pjp|webp|bmp|ico|tiff?|tga)$/i;
// Animated GIF keeps its own branch (rendered from the path, not re-encoded).
export const GIF_EXT = /\.gif$/i;
// Video: every container WebView2 (Chromium) plays back natively. Formats it
// cannot decode (mkv, avi, wmv, flv, heic, raw, …) fall through to
// mediaKind() === "" and are reported to the user as unsupported.
export const VIDEO_EXT = /\.(mp4|m4v|mov|webm|ogv|ogg|ogm)$/i;

export const mediaKind = (path) =>
  IMAGE_EXT.test(path) ? "image"
    : GIF_EXT.test(path) ? "gif"
    : VIDEO_EXT.test(path) ? "video"
    : "";

// ---- Video control bar (grid tiles + floating pills) ----

const ICON_PLAY =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M8 5v14l11-7Z"/></svg>';
const ICON_PAUSE =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M6 5h4v14H6V5Zm8 0h4v14h-4V5Z"/></svg>';
const ICON_SOUND =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M3 9v6h4l5 5V4L7 9H3Zm13.5 3a4.5 4.5 0 0 0-2.5-4v8a4.5 4.5 0 0 0 2.5-4Zm-2.5-9v2.1a7 7 0 0 1 0 13.8V21a9 9 0 0 0 0-18Z"/></svg>';
const ICON_MUTED =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M3 9v6h4l5 5V4L7 9H3Zm18.4 3 2.1 2.1-1.4 1.4-2.1-2.1-2.1 2.1-1.4-1.4 2.1-2.1-2.1-2.1 1.4-1.4 2.1 2.1 2.1-2.1 1.4 1.4-2.1 2.1Z"/></svg>';
const ICON_LOOP =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M7 7h10v3l4-4-4-4v3H5v6h2V7Zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4Z"/></svg>';
const ICON_LOOP_ONCE =
  '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M7 7h10v3l4-4-4-4v3H5v6h2V7Zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4Zm-4-2V9h-1l-2 1v1h1.5v4H13Z"/></svg>';

const fmtTime = (s) => {
  if (!Number.isFinite(s)) return "0:00";
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${m}:${String(sec).padStart(2, "0")}`;
};

// Global fallbacks (expert menu) used when a prompt has no saved player state.
let MEDIA_DEFAULTS = { volume: 100, muted: true, looped: true };
export function setMediaDefaults(d) {
  MEDIA_DEFAULTS = { ...MEDIA_DEFAULTS, ...d };
}

// Restore a saved player state (volume 0..100, mute, loop) onto a video.
// Missing per-prompt values fall back to the configured global defaults.
export function applyVideoPrefs(video, prefs) {
  video.volume = (prefs?.volume ?? MEDIA_DEFAULTS.volume) / 100;
  video.muted = prefs?.muted ?? MEDIA_DEFAULTS.muted;
  video.loop = prefs?.looped ?? MEDIA_DEFAULTS.looped;
  video.dispatchEvent(new Event("prefs-applied"));
}

// Play/pause, scrubber, time, loop and sound toggle. The bar swallows pointer
// events so the surface underneath neither copies nor starts a drag.
// onChange (debounced) reports {volume, muted, looped} for persistence.
export function buildMediaBar(video, { onChange } = {}) {
  const bar = document.createElement("div");
  bar.className = "media-bar";
  for (const ev of ["pointerdown", "mousedown", "click"]) {
    bar.addEventListener(ev, (e) => e.stopPropagation());
  }

  const mkBtn = (svg) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "media-btn";
    b.innerHTML = svg;
    return b;
  };

  const playBtn = mkBtn(ICON_PAUSE); // media autoplays
  playBtn.addEventListener("click", () => (video.paused ? video.play() : video.pause()));
  video.addEventListener("play", () => (playBtn.innerHTML = ICON_PAUSE));
  video.addEventListener("pause", () => (playBtn.innerHTML = ICON_PLAY));

  const seek = document.createElement("input");
  seek.type = "range";
  seek.className = "media-seek";
  seek.min = 0;
  seek.max = 1000;
  seek.value = 0;
  const time = document.createElement("span");
  time.className = "media-time";
  let scrubbing = false;
  seek.addEventListener("pointerdown", () => (scrubbing = true));
  seek.addEventListener("pointerup", () => (scrubbing = false));
  seek.addEventListener("input", () => {
    if (video.duration) video.currentTime = (seek.value / 1000) * video.duration;
  });
  video.addEventListener("timeupdate", () => {
    if (!scrubbing && video.duration) {
      seek.value = Math.round((video.currentTime / video.duration) * 1000);
    }
    time.textContent = `${fmtTime(video.currentTime)} / ${fmtTime(video.duration)}`;
  });

  // Debounced persistence of the player state.
  let saveTimer = 0;
  const persist = () => {
    if (!onChange) return;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => onChange({
      volume: Math.round(video.volume * 100),
      muted: video.muted,
      looped: video.loop,
    }), 400);
  };

  // Endless loop by default; one tap switches to play-once.
  const loopBtn = mkBtn(ICON_LOOP);
  loopBtn.addEventListener("click", () => {
    video.loop = !video.loop;
    loopBtn.innerHTML = video.loop ? ICON_LOOP : ICON_LOOP_ONCE;
    persist();
  });
  video.addEventListener("ended", () => (playBtn.innerHTML = ICON_PLAY));

  const soundBtn = mkBtn(ICON_MUTED); // muted by default
  soundBtn.addEventListener("click", () => {
    video.muted = !video.muted;
    persist();
  });

  // Vertical volume slider popping up while hovering the sound button.
  const soundWrap = document.createElement("div");
  soundWrap.className = "media-sound";
  const volPop = document.createElement("div");
  volPop.className = "media-vol-pop";
  const vol = document.createElement("input");
  vol.type = "range";
  vol.className = "media-vol";
  vol.min = 0;
  vol.max = 100;
  vol.value = 100;
  // While the slider is held, the popup must not close — even if the cursor
  // leaves it. Released: CSS :hover takes over again (closes unless on it).
  vol.addEventListener("pointerdown", () => {
    soundWrap.classList.add("dragging");
    window.addEventListener(
      "pointerup",
      () => soundWrap.classList.remove("dragging"),
      { once: true }
    );
  });
  vol.addEventListener("input", () => {
    const v = Number(vol.value);
    video.volume = v / 100;
    video.muted = v === 0; // sliding to zero mutes, anything above unmutes
    persist();
  });
  volPop.appendChild(vol);
  soundWrap.append(volPop, soundBtn);

  // Keep icon, slider and loop button in sync with the video element — also
  // when a saved state is applied from the outside (applyVideoPrefs).
  const syncSound = () => {
    soundBtn.innerHTML = video.muted ? ICON_MUTED : ICON_SOUND;
    vol.value = Math.round(video.volume * 100);
  };
  video.addEventListener("volumechange", syncSound);
  video.addEventListener("prefs-applied", () => {
    syncSound();
    loopBtn.innerHTML = video.loop ? ICON_LOOP : ICON_LOOP_ONCE;
  });
  syncSound();

  bar.append(playBtn, seek, time, loopBtn, soundWrap);
  return bar;
}
