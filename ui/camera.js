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
import { fmtTimeMMSS } from "./floatShared.js";
import { LANGS, fullTexts } from "./i18n.js";
import { attachSelectPicker } from "./selectpop.js";

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
// Themed list popup instead of the native OS dropdown. A locked select (a
// take in flight, see the disabled toggles below) never opens it.
attachSelectPicker(audioSelect);
attachSelectPicker(resSelect);
const folderLabel = document.getElementById("cam-folder-label");
const folderPathEl = document.getElementById("cam-folder-path");
const folderBtn = document.getElementById("cam-folder-btn");
const startBtn = document.getElementById("cam-start-btn");
const pauseBtn = document.getElementById("cam-pause-btn");
const pauseIcon = document.getElementById("cam-pause-icon");
const resumeIcon = document.getElementById("cam-resume-icon");
const stopBtn = document.getElementById("cam-stop-btn");
const closeBtn = document.getElementById("cam-close");

let tx = null; // the text lookup, set once the language has loaded
let stream = null;
let recorder = null;
let recPath = null;
let recExt = "webm";
let paused = false;
let recStartWall = 0;
// The screen recording's own pause-aware position at the moment this take's
// recorder reported that it actually started -- read live
// (camera_screen_elapsed_ms) from the recorder's start event, never
// reconstructed later from wall-clock stamps alone, or a pause in the screen
// recording between the two starts would silently inflate the overlay's
// placement by however long it lasted. Holds the pending read (a promise),
// which onstop awaits.
let recScreenElapsedAtStart = null;
// The same read once it has resolved (null until then), so onstop can mark
// the session finishing without waiting on the read.
let recScreenElapsedValue = null;
let recTimer = 0;
// The two expert-menu settings this window has no control of its own for
// (device, framerate) -- read once at boot, applied to every getUserMedia
// call below. Resolution has its own in-window select, so only its starting
// value comes from settings.
let videoDeviceId = "";
let frameRate = 30;
// Bumped on every refreshPreview() call; a getUserMedia that resolves after
// a newer generation has already started stops its own tracks instead of
// replacing `stream` out from under whatever is now current -- otherwise
// the ready-event refresh and the 1500ms fallback refresh (see below) could
// both resolve and leak the first stream's tracks (camera light stays on
// with nothing showing it).
let previewGen = 0;

function t(key) {
  return tx ? tx(key) : key;
}

// Icon, tooltip and accessible name of the pause button follow the state, with
// the same texts the recording bar uses.
function applyPauseState() {
  pauseIcon.classList.toggle("hidden", paused);
  resumeIcon.classList.toggle("hidden", !paused);
  const label = t(paused ? "recBarResume" : "recBarPause");
  pauseBtn.title = label;
  pauseBtn.setAttribute("aria-label", label);
}

let statusState = "idle";
function setStatus(state) {
  // state: "idle" | "recording" | "paused"
  statusDot.classList.toggle("on", state !== "idle");
  statusDot.classList.toggle("paused", state === "paused");
  statusState = state;
  statusText.textContent = state === "idle" ? "" : t("cameraRecording");
  statusText.title = "";
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
  const gen = ++previewGen;
  stopStream();
  const height = Number(resSelect.value) || 720;
  const width = Math.round((height * 16) / 9);
  const audioDeviceId = audioSelect.value;
  const constraintsFor = (withDevice) => ({
    video: {
      width: { ideal: width },
      height: { ideal: height },
      frameRate: { ideal: frameRate },
      ...(withDevice && videoDeviceId ? { deviceId: { exact: videoDeviceId } } : {}),
    },
    audio: audioDeviceId ? { deviceId: { exact: audioDeviceId } } : false,
  });
  try {
    let newStream;
    try {
      newStream = await navigator.mediaDevices.getUserMedia(constraintsFor(true));
    } catch (err) {
      // A saved camera that has since been unplugged must not switch the
      // window off for good: one retry with the default camera, and only for
      // the two errors a missing device produces (a permission denial or any
      // other failure is not a stale device and is not retried).
      const missing = err && (err.name === "NotFoundError" || err.name === "OverconstrainedError");
      if (!videoDeviceId || !missing || gen !== previewGen) throw err;
      newStream = await navigator.mediaDevices.getUserMedia(constraintsFor(false));
    }
    if (gen !== previewGen) {
      // A newer refreshPreview() call (or a stop) already won the race --
      // never let a late, now-stale stream take over the camera light.
      for (const track of newStream.getTracks()) track.stop();
      return;
    }
    stream = newStream;
    preview.srcObject = stream;
    noDeviceEl.classList.add("hidden");
    startBtn.disabled = false;
  } catch {
    if (gen !== previewGen) return;
    preview.srcObject = null;
    noDeviceEl.textContent = t("cameraNoDevice");
    noDeviceEl.classList.remove("hidden");
    startBtn.disabled = true;
  }
}

// Chunk-uploads a base64 string instead of a JSON array of numbers -- about
// a third bigger than raw instead of three to four times bigger, over an
// IPC channel that already pays a per-call and a serialization cost on top.
function bufToBase64(buf) {
  const bytes = new Uint8Array(buf);
  let binary = "";
  const chunkSize = 0x8000; // String.fromCharCode's own argument-count limit
  for (let i = 0; i < bytes.length; i += chunkSize) {
    binary += String.fromCharCode.apply(null, bytes.subarray(i, i + chunkSize));
  }
  return btoa(binary);
}

// Every in-flight camera_append_chunk call for the CURRENT take -- onstop
// awaits all of them before calling camera_finish_recording, or the very
// last chunk (still mid-flight when MediaRecorder's own final
// "dataavailable" fires right before "stop") loses its race against
// finish's session.take() and is dropped on the floor, silently, with the
// recording missing its last second or so.
let pendingAppends = [];
// Each chunk's append starts only after the previous one finished: the backend
// command is async, so two calls in flight at once could write out of order.
let appendChain = Promise.resolve();
// True once any append of the current take was rejected (disk full, denied
// write, IPC failure). The drain in onstop still waits for every in-flight
// append, then refuses to present a take with a hole in it as a good one.
let appendFailed = false;
// The in-flight release of the session started by the FIRST failed append
// (see ondataavailable): onstop awaits it, never issues a second one.
let abandonCall = null;

// The one way back from a start that got as far as reserving the take in the
// backend (file + session) and then failed in the recorder: releases that
// reservation through the ordinary discard command and puts the window back
// to its idle state, so the next Start begins from scratch.
async function rollbackFailedStart() {
  const path = recPath;
  if (recorder) {
    recorder.ondataavailable = null;
    recorder.onstart = null;
    recorder.onstop = null;
  }
  recorder = null;
  recPath = null;
  pendingAppends = [];
  appendChain = Promise.resolve();
  recScreenElapsedAtStart = null;
  recScreenElapsedValue = null;
  paused = false;
  clearInterval(recTimer);
  recBadge.classList.add("hidden");
  setStatus("idle");
  startBtn.classList.remove("hidden");
  pauseBtn.classList.add("hidden");
  stopBtn.classList.add("hidden");
  audioSelect.disabled = false;
  resSelect.disabled = false;
  if (path) await invoke("camera_discard_recording", { path }).catch(() => {});
  startBtn.disabled = !stream;
}

async function startRecording() {
  if (!stream) return;
  // Guards against a second click landing while the first is still awaiting
  // camera_begin_recording or the live elapsed-time read below -- without
  // this, that second call replaces the session mid-flight and leaves the
  // first recorder.start() writing into a take that no longer exists.
  if (startBtn.disabled) return;
  startBtn.disabled = true;
  const mimeType = bestMimeType();
  recExt = mimeType.includes("mp4") ? "mp4" : "webm";
  let begin;
  try {
    begin = await invoke("camera_begin_recording", { ext: recExt });
  } catch {
    startBtn.disabled = false;
    return;
  }
  recPath = begin.path;
  // Provisional until the recorder's start event stamps the real value.
  recStartWall = Date.now();
  recScreenElapsedAtStart = null;
  pendingAppends = [];
  appendChain = Promise.resolve();
  appendFailed = false;
  abandonCall = null;
  recScreenElapsedValue = null;
  try {
    recorder = new MediaRecorder(stream, { mimeType });
  } catch {
    await rollbackFailedStart();
    return;
  }
  recorder.ondataavailable = (e) => {
    if (!e.data || e.data.size === 0 || !recPath) return;
    const path = recPath;
    const append = appendChain
      .then(() => e.data.arrayBuffer())
      .then((buf) => invoke("camera_append_chunk", { path, bytesB64: bufToBase64(buf) }))
      .catch((err) => {
        console.error("camera chunk lost", err);
        if (appendFailed) return;
        appendFailed = true;
        // The take now has a hole in it. Release the session at once, before
        // anything can finish it or a closing window can write its sidecar;
        // the partial file itself is kept (see camera_abandon_recording).
        abandonCall = invoke("camera_abandon_recording", { path }).catch(() => {});
      });
    pendingAppends.push(append);
    appendChain = append;
  };
  // Stamped when the recorder really reports it is running, not when Start
  // was clicked or the backend reserved the take: this is the one moment
  // that matters for alignment, not begin time (which can sit open for a
  // while with the camera preview idle) and not finish time (the screen
  // recording may have moved on, paused again, or stopped by then).
  recorder.onstart = () => {
    recStartWall = Date.now();
    recScreenElapsedAtStart = invoke("camera_screen_elapsed_ms")
      .catch(() => null)
      .then((v) => {
        recScreenElapsedValue = v;
        return v;
      });
  };
  recorder.onstop = async () => {
    clearInterval(recTimer);
    recBadge.classList.add("hidden");
    setStatus("idle");
    // Start and the two selects stay disabled until the whole finish below
    // has run: a new take (or a preview refresh that would re-enable Start)
    // while the old one is still uploading its last chunks would fight it
    // over the one session.
    startBtn.classList.remove("hidden");
    pauseBtn.classList.add("hidden");
    stopBtn.classList.add("hidden");
    const path = recPath;
    const startedAtMs = recStartWall;
    recPath = null;
    const idleControls = () => {
      audioSelect.disabled = false;
      resSelect.disabled = false;
    };
    if (!path) {
      idleControls();
      startBtn.disabled = !stream;
      return;
    }
    // Marks the session finishing before any slow wait below -- Alt+F4/
    // taskbar-close/app-quit landing in that window must finish this take,
    // not delete it as if it were still an active, abandoned recording (see
    // camera::CamSession::finishing). A take whose chunk write already
    // failed is not marked: it was released for good and must never get a
    // sidecar.
    let screenElapsedMs = recScreenElapsedValue;
    const elapsedRead = recScreenElapsedAtStart;
    recScreenElapsedAtStart = null;
    if (!appendFailed) {
      await invoke("camera_mark_finishing", { path, camStartMs: startedAtMs, screenElapsedMs }).catch(() => {});
    }
    // The elapsed read normally resolved long ago; a stop right after the
    // start event can beat it, and the mark is then refreshed with the value.
    // A recorder that never reached its start event has nothing to align.
    const lateElapsed = await (elapsedRead || Promise.resolve(null));
    if (lateElapsed !== screenElapsedMs) {
      screenElapsedMs = lateElapsed;
      if (!appendFailed) {
        await invoke("camera_mark_finishing", { path, camStartMs: startedAtMs, screenElapsedMs }).catch(() => {});
      }
    }
    await Promise.allSettled(pendingAppends);
    pendingAppends = [];
    if (appendFailed) {
      // A chunk never reached the disk, so the file is truncated or has
      // holes in it. The session was already released (without deleting
      // anything, the partial file may be the only copy); skip the editor
      // and say where the file is.
      appendFailed = false;
      await abandonCall;
      abandonCall = null;
      const text = t("camChunkWriteFailed").replace("{path}", path);
      statusText.textContent = text;
      statusText.title = text;
      idleControls();
      refreshPreview();
      return;
    }
    try {
      const finished = await invoke("camera_finish_recording", { path, camStartMs: startedAtMs, screenElapsedMs });
      // A companion screen recording that has ALREADY stopped by now opens
      // as the primary source, with this camera take added as an aligned
      // overlay when one is ready -- otherwise the camera file just opens on
      // its own, same as any other recording. The overlay's own alignment
      // lives in its sidecar (just written above), not in anything passed
      // through open_video_editor.
      if (finished && finished.companion_path) {
        await invoke("open_video_editor", { srcPath: finished.companion_path, overlayPath: path });
      } else {
        await invoke("open_video_editor", { srcPath: path });
      }
    } catch {
      // A finish that fails leaves the raw file in place, still playable --
      // the same fallback the screen recorder's own encode-error path uses.
    }
    idleControls();
    refreshPreview();
  };
  try {
    recorder.start(1000);
  } catch {
    await rollbackFailedStart();
    return;
  }
  paused = false;
  applyPauseState();
  setStatus("recording");
  recBadge.classList.remove("hidden");
  startBtn.classList.add("hidden");
  pauseBtn.classList.remove("hidden");
  stopBtn.classList.remove("hidden");
  audioSelect.disabled = true;
  resSelect.disabled = true;
  recTimer = setInterval(() => {
    recTimeEl.textContent = fmtTimeMMSS(Date.now() - recStartWall);
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
  applyPauseState();
  setStatus(paused ? "paused" : "recording");
});
stopBtn.addEventListener("click", stopRecording);
audioSelect.addEventListener("change", refreshPreview);
resSelect.addEventListener("change", refreshPreview);
folderBtn.addEventListener("click", async () => {
  const dir = await invoke("pick_folder", { purpose: "recording" }).catch(() => null);
  if (dir) {
    await invoke("set_recording_dir", { dir }).catch(() => {});
    folderPathEl.textContent = dir;
  }
});
closeBtn.addEventListener("click", async () => {
  await discardRecording();
  stopStream();
  appWin.close();
});

// Keys act once the window has focus. Escape stops a running take (the file
// is kept) and only closes the window when nothing is recording; Space
// pauses or resumes a running take and otherwise keeps its native job on
// the focused control. Both go through the buttons, like a click.
const isRecording = () => !!recorder && recorder.state !== "inactive";
document.addEventListener("keydown", (e) => {
  if (e.repeat) return;
  const tag = e.target && e.target.tagName;
  if (tag === "SELECT" || tag === "INPUT") return;
  if (e.key === "Escape") {
    e.preventDefault();
    if (isRecording()) stopBtn.click(); else closeBtn.click();
  } else if (e.code === "Space" && isRecording()) {
    e.preventDefault();
    pauseBtn.click();
  }
});
// A focused button would also fire its own click when Space is released,
// which would undo the pause the key press just did.
document.addEventListener("keyup", (e) => {
  const tag = e.target && e.target.tagName;
  if (e.code === "Space" && isRecording() && tag !== "SELECT" && tag !== "INPUT") e.preventDefault();
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
Promise.all([invoke("get_settings"), import("./floatShared.js"), invoke("default_recording_dir")])
  .then(async ([s, { resolveLang }, defaultDir]) => {
    ({ tx } = await fullTexts(resolveLang(s.language, LANGS)));
    applyTexts();
    folderPathEl.textContent = s.ui_texts?.recordingDir || defaultDir || "";
    videoDeviceId = s.ui_texts?.cameraDevice || "";
    frameRate = Number(s.ui_values?.cameraFramerate) || 30;
    const savedRes = s.ui_texts?.cameraResolution;
    if (savedRes && resSelect.querySelector(`option[value="${savedRes}"]`)) resSelect.value = savedRes;
    populateAudioDevices();
  })
  .catch(() => {});

// Every text this window shows, drawn again from the current language. The window
// stays open while the language can be switched in the main window, so this runs
// at load and on every language-changed.
function applyTexts() {
  audioLabel.textContent = t("cameraAudioSource");
  resLabel.textContent = t("cameraResolution");
  folderLabel.textContent = t("valRecordingDir");
  startBtn.textContent = t("cameraStart");
  for (const el of document.querySelectorAll("[data-i18n-title]")) {
    const text = t(el.getAttribute("data-i18n-title"));
    el.title = text;
    el.setAttribute("aria-label", text);
  }
  applyPauseState();
  setStatus(statusState);
  const silent = audioSelect.options[0];
  if (silent && silent.value === "") silent.textContent = t("cameraAudioNone");
  if (!noDeviceEl.classList.contains("hidden")) noDeviceEl.textContent = t("cameraNoDevice");
}
listen("language-changed", () => {
  Promise.all([invoke("get_settings"), import("./floatShared.js")])
    .then(async ([s, { resolveLang }]) => {
      ({ tx } = await fullTexts(resolveLang(s.language, LANGS)));
      applyTexts();
    })
    .catch(() => {});
});

// A fallback in case the permission-ready event is somehow missed (e.g. this
// window is reopened while WebView2 already answered it once for this
// origin, so no new prompt round trip happens at all) -- tries the preview
// once after a short delay regardless of the event.
setTimeout(() => { if (!stream) refreshPreview(); }, 1500);
