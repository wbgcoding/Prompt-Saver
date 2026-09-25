// Camera recording window. getUserMedia feeds the live preview; once
// recording starts, the SAME stream is handed to the browser's own
// MediaRecorder, which does the encoding -- this window never touches raw
// pixels itself, unlike the screen recorder's own Rust/WGC pipeline. Chunks
// are appended to disk as they arrive (camera_append_chunk) instead of
// buffered here, so a long take never has to hold its whole size in memory.
//
// getUserMedia only resolves once WebView2's permission prompt has been
// answered, which happens in Rust (camera.rs, install_permission_handler) --
// this file waits for the camera-permission-ready event before ever calling
// it, closing the race where the page could ask before the handler exists.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;
const appWin = getCurrentWebviewWindow();

const preview = document.getElementById("cam-preview");
const noDeviceEl = document.getElementById("cam-no-device");
const recBadge = document.getElementById("cam-rec-badge");
const recTimeEl = document.getElementById("cam-rec-time");
const statusDot = document.getElementById("cam-status-dot");
const statusText = document.getElementById("cam-status-text");
const audioSelect = document.getElementById("cam-audio-select");
const audioLabel = document.getElementById("cam-audio-label");
const resSelect = document.getElementById("cam-res-select");
const resLabel = document.getElementById("cam-res-label");
const folderLabel = document.getElementById("cam-folder-label");
const folderPathEl = document.getElementById("cam-folder-path");
const folderBtn = document.getElementById("cam-folder-btn");
const startBtn = document.getElementById("cam-start-btn");
const pauseBtn = document.getElementById("cam-pause-btn");
const pauseIcon = document.getElementById("cam-pause-icon");
const resumeIcon = document.getElementById("cam-resume-icon");
const stopBtn = document.getElementById("cam-stop-btn");
const closeBtn = document.getElementById("cam-close");

let i18nDict = null;
let stream = null;
let recorder = null;
let recPath = null;
let recExt = "webm";
let paused = false;
let recStartWall = 0;
let recTimer = 0;
// The two expert-menu settings this window has no control of its own for
// (device, framerate) -- read once at boot, applied to every getUserMedia
// call below. Resolution has its own in-window select, so only its starting
// value comes from settings.
let videoDeviceId = "";
let frameRate = 30;

function t(key) {
  return (i18nDict && i18nDict[key]) || key;
}

function fmtTime(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const m = Math.floor(s / 60);
  const ss = s % 60;
  return `${String(m).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
}

function setStatus(state) {
  // state: "idle" | "recording" | "paused"
  statusDot.classList.toggle("on", state !== "idle");
  statusDot.classList.toggle("paused", state === "paused");
  statusText.textContent = state === "idle" ? "" : t("cameraRecording");
}

function bestMimeType() {
  const candidates = ["video/webm;codecs=vp8,opus", "video/webm;codecs=vp8", "video/webm"];
  for (const c of candidates) {
    if (window.MediaRecorder && MediaRecorder.isTypeSupported && MediaRecorder.isTypeSupported(c)) return c;
  }
  return "video/webm";
}

async function populateAudioDevices() {
  audioSelect.innerHTML = "";
  const none = document.createElement("option");
  none.value = "";
  none.textContent = t("cameraAudioNone");
  audioSelect.appendChild(none);
  try {
    const devices = await navigator.mediaDevices.enumerateDevices();
    for (const d of devices) {
      if (d.kind !== "audioinput") continue;
      const opt = document.createElement("option");
      opt.value = d.deviceId;
      opt.textContent = d.label || t("cameraAudioSource");
      audioSelect.appendChild(opt);
    }
  } catch {
    // No device enumeration permission yet -- the "no microphone" option
    // alone still lets the camera be used silently.
  }
}

function stopStream() {
  if (stream) {
    for (const track of stream.getTracks()) track.stop();
    stream = null;
  }
}

// Re-requests getUserMedia with the current resolution/audio picks. Called
// on load and whenever either control changes, but never while a take is
// actually recording -- swapping the live stream under an active
// MediaRecorder would just corrupt the file.
async function refreshPreview() {
  if (recorder && recorder.state !== "inactive") return;
  stopStream();
  const height = Number(resSelect.value) || 720;
  const width = Math.round((height * 16) / 9);
  const audioDeviceId = audioSelect.value;
  const constraints = {
    video: {
      width: { ideal: width },
      height: { ideal: height },
      frameRate: { ideal: frameRate },
      ...(videoDeviceId ? { deviceId: { exact: videoDeviceId } } : {}),
    },
    audio: audioDeviceId ? { deviceId: { exact: audioDeviceId } } : false,
  };
  try {
    stream = await navigator.mediaDevices.getUserMedia(constraints);
    preview.srcObject = stream;
    noDeviceEl.classList.add("hidden");
    startBtn.disabled = false;
  } catch {
    preview.srcObject = null;
    noDeviceEl.textContent = t("cameraNoDevice");
    noDeviceEl.classList.remove("hidden");
    startBtn.disabled = true;
  }
}

function toArray(buf) {
  return Array.from(new Uint8Array(buf));
}

async function startRecording() {
  if (!stream) return;
  const mimeType = bestMimeType();
  recExt = mimeType.includes("mp4") ? "mp4" : "webm";
  let begin;
  try {
    begin = await invoke("camera_begin_recording", { ext: recExt });
  } catch {
    return;
  }
  recPath = begin.path;
  recStartWall = Date.now();
  recorder = new MediaRecorder(stream, { mimeType });
  recorder.ondataavailable = async (e) => {
    if (!e.data || e.data.size === 0 || !recPath) return;
    const buf = await e.data.arrayBuffer();
    invoke("camera_append_chunk", { path: recPath, bytes: toArray(buf) }).catch(() => {});
  };
  recorder.onstop = async () => {
    clearInterval(recTimer);
    recBadge.classList.add("hidden");
    setStatus("idle");
    startBtn.classList.remove("hidden");
    pauseBtn.classList.add("hidden");
    stopBtn.classList.add("hidden");
    audioSelect.disabled = false;
    resSelect.disabled = false;
    const path = recPath;
    recPath = null;
    if (!path) return;
    try {
      const finished = await invoke("camera_finish_recording", { path });
      // A companion screen recording that has ALREADY stopped by now opens
      // as the primary source, with this camera take added as an aligned
      // overlay when one is ready -- otherwise the camera file just opens on
      // its own, same as any other recording.
      if (finished && finished.companion_path) {
        await invoke("open_video_editor", { srcPath: finished.companion_path, overlayPath: path });
      } else {
        await invoke("open_video_editor", { srcPath: path });
      }
    } catch {
      // A finish that fails leaves the raw file in place, still playable --
      // the same fallback the screen recorder's own encode-error path uses.
    }
    refreshPreview();
  };
  recorder.start(1000);
  paused = false;
  setStatus("recording");
  recBadge.classList.remove("hidden");
  startBtn.classList.add("hidden");
  pauseBtn.classList.remove("hidden");
  stopBtn.classList.remove("hidden");
  audioSelect.disabled = true;
  resSelect.disabled = true;
  recTimer = setInterval(() => {
    recTimeEl.textContent = fmtTime(Date.now() - recStartWall);
  }, 250);
}

function stopRecording() {
  if (recorder && recorder.state !== "inactive") recorder.stop();
}

// A cancel (window closed mid-take) discards instead of finishing -- no
// half-length recording, no sidecar, nothing for the library to trip over.
async function discardRecording() {
  if (!recorder || recorder.state === "inactive") return;
  const path = recPath;
  recorder.onstop = null;
  recorder.stop();
  recPath = null;
  if (path) invoke("camera_discard_recording", { path }).catch(() => {});
}

startBtn.addEventListener("click", startRecording);
pauseBtn.addEventListener("click", () => {
  if (!recorder) return;
  if (paused) {
    recorder.resume();
    paused = false;
  } else {
    recorder.pause();
    paused = true;
  }
  pauseIcon.classList.toggle("hidden", paused);
  resumeIcon.classList.toggle("hidden", !paused);
  setStatus(paused ? "paused" : "recording");
});
stopBtn.addEventListener("click", stopRecording);
audioSelect.addEventListener("change", refreshPreview);
resSelect.addEventListener("change", refreshPreview);
folderBtn.addEventListener("click", async () => {
  const dir = await invoke("pick_folder").catch(() => null);
  if (dir) {
    await invoke("set_ui_text", { key: "recordingDir", value: dir }).catch(() => {});
    folderPathEl.textContent = dir;
  }
});
closeBtn.addEventListener("click", async () => {
  await discardRecording();
  stopStream();
  appWin.close();
});

// The window has no OS titlebar; dragging the status strip moves it, same
// startDragging() pattern the recording bar uses (WebView2 ignores the CSS
// app-region property Electron apps rely on).
const titlebar = document.querySelector(".cam-titlebar");
const DRAG_THRESHOLD = 4;
let dragStart = null;
titlebar.addEventListener("mousedown", (e) => {
  if (e.button !== 0 || e.target.closest("button")) return;
  dragStart = { x: e.screenX, y: e.screenY };
});
window.addEventListener("mousemove", async (e) => {
  if (!dragStart) return;
  if (e.buttons === 0) { dragStart = null; return; }
  if (Math.abs(e.screenX - dragStart.x) < DRAG_THRESHOLD && Math.abs(e.screenY - dragStart.y) < DRAG_THRESHOLD) return;
  dragStart = null;
  await appWin.startDragging();
});
window.addEventListener("mouseup", () => { dragStart = null; });

listen("camera-permission-ready", () => {
  refreshPreview();
  populateAudioDevices();
});

// Theme + language, same shared helpers every satellite window uses.
import("./floattheme.js").then(({ applyFloatTheme }) => {
  listen("theme-changed", (e) => applyFloatTheme(e.payload?.theme, e.payload?.vars));
  Promise.all([invoke("current_theme"), invoke("theme_vars")])
    .then(([theme, vars]) => applyFloatTheme(theme, vars))
    .catch(() => {});
});
Promise.all([invoke("get_settings"), import("./i18n.js"), import("./floatShared.js"), invoke("default_recording_dir")])
  .then(([s, { I18N, LANGS }, { resolveLang }, defaultDir]) => {
    const lang = resolveLang(s.language, LANGS);
    i18nDict = I18N[lang] || I18N.en;
    audioLabel.textContent = t("cameraAudioSource");
    resLabel.textContent = t("cameraResolution");
    folderLabel.textContent = i18nDict.valRecordingDir || "";
    startBtn.textContent = t("cameraStart");
    folderPathEl.textContent = s.ui_texts?.recordingDir || defaultDir || "";
    videoDeviceId = s.ui_texts?.cameraDevice || "";
    frameRate = Number(s.ui_values?.cameraFramerate) || 30;
    const savedRes = s.ui_texts?.cameraResolution;
    if (savedRes && resSelect.querySelector(`option[value="${savedRes}"]`)) resSelect.value = savedRes;
    for (const el of document.querySelectorAll("[data-i18n-title]")) {
      const key = el.getAttribute("data-i18n-title");
      const text = i18nDict[key] ?? I18N.en[key];
      if (text) { el.title = text; el.setAttribute("aria-label", text); }
    }
    populateAudioDevices();
  })
  .catch(() => {});

// A fallback in case the permission-ready event is somehow missed (e.g. this
// window is reopened while WebView2 already answered it once for this
// origin, so no new prompt round trip happens at all) -- tries the preview
// once after a short delay regardless of the event.
setTimeout(() => { if (!stream) refreshPreview(); }, 1500);
