// Recording control bar: a small always-on-top strip with a red dot, elapsed time, live
// file size, Pause/Resume, Stop and Cancel. The window carries WDA_EXCLUDEFROMCAPTURE
// (set in Rust, unconditionally) so it never appears in the recording it controls.
import { fmtTimeMMSS, fmtSizeCompact, fmtNum } from "./floatShared.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;
const appWin = getCurrentWebviewWindow();

const bar = document.getElementById("bar");
const dot = document.getElementById("dot");
const timeEl = document.getElementById("time");
const sizeEl = document.getElementById("size");
const switchBtn = document.getElementById("switch-btn");
const pauseBtn = document.getElementById("pause-btn");
const pauseIcon = document.getElementById("pause-icon");
const resumeIcon = document.getElementById("resume-icon");
const continueBtn = document.getElementById("continue-btn");
const continueGapEl = document.getElementById("continue-gap");
const stopBtn = document.getElementById("stop-btn");
const cancelBtn = document.getElementById("cancel-btn");
const audioSysEl = document.getElementById("audio-sys");
const audioMicEl = document.getElementById("audio-mic");
const hotkeyWarnEl = document.getElementById("hotkey-warn");
const diskWarnEl = document.getElementById("disk-warn");

// recBarSize gates the whole readout, shown from 500 MB on so the narrow bar stays
// narrow; the threshold lives in recording_status (rec_bar_size_state), this only renders
// it. etaSecs is set only past three quarters of recMaxMb, where the text turns warm and
// the tooltip gives the estimated time left at the observed rate.
function applySize(bytes, etaSecs) {
  sizeEl.textContent = fmtSizeCompact(bytes, LANG);
  sizeEl.classList.toggle("rec-size-warn", etaSecs != null);
  sizeEl.title = etaSecs != null && tx ? tx("recBarSizeEta").replace("{time}", fmtTimeMMSS(etaSecs * 1000)) : "";
}

// recTargetSwitch's button stays visible even when it cannot act (a Monitor/Region
// recording, or a Window recording without a placement canvas), with a tooltip naming
// why. The "same screen" check for a canvas-mode "monitor" recording happens once
// something is picked (the recording-target-switch-failed listener in the main window);
// this covers only the up-front half.
let switchEnabled = false;
function applyTargetSwitch(enabled) {
  switchEnabled = enabled;
  switchBtn.disabled = !enabled;
  if (!tx) return;
  const label = enabled ? tx("recBarSwitchTarget") : tx("recTargetSwitchUnsupported");
  if (label) { switchBtn.title = label; switchBtn.setAttribute("aria-label", label); }
}

// Mirrors the backend's 5-second refusal (finish_and_continue_recording) so the button is
// never enabled for a click the backend would reject; the backend guard still decides.
const CONTINUE_LOCKOUT_MS = 5000;
function applyContinueEnabled(elapsedMs) {
  continueBtn.disabled = elapsedMs < CONTINUE_LOCKOUT_MS;
}

// A successful continue's gap (ms the switch took) is shown for a few seconds where the
// file size sits, so the missing moment between the two files is never hidden. Not
// localized, like fmtSizeCompact's "KB"/"MB".
let gapHideTimer = 0;
function showContinueGap(gapMs) {
  clearTimeout(gapHideTimer);
  continueGapEl.textContent = gapMs >= 1000 ? `+${fmtNum(gapMs / 1000, 1, LANG)}s` : `+${gapMs}ms`;
  continueGapEl.classList.remove("hidden");
  gapHideTimer = setTimeout(() => continueGapEl.classList.add("hidden"), 4000);
}

let paused = false;
let tx = null; // the text lookup, set once the language has loaded
let LANG = "en"; // the window's own language, for the way numbers are written
function pauseBtnLabel() {
  if (!tx) return null;
  const key = paused ? "recBarResume" : "recBarPause";
  return tx(key);
}
function applyPaused(p) {
  const changed = paused !== p;
  paused = p;
  bar.classList.toggle("paused", p);
  pauseIcon.classList.toggle("hidden", p);
  resumeIcon.classList.toggle("hidden", !p);
  const label = pauseBtnLabel();
  if (label) { pauseBtn.title = label; pauseBtn.setAttribute("aria-label", label); }
  // Reuses the Pause/Resume label a sighted user just saw change; it names the current
  // state.
  if (changed && label) {
    const live = document.getElementById("live");
    live.textContent = "";
    live.textContent = label;
  }
}

// System-audio / mic buttons stay visible for the whole recording: dim when the source is
// off or muted, lit when on. audio_system/audio_mic are fixed per session and decide
// whether the button is usable; mid-recording muting (audio_system_muted/audio_mic_muted)
// sits on top. mic_lost can flip true if the device is unplugged and latches; system_lost
// is the same for the loopback capture (no render device, init refused).
let lastAudio = { system: false, mic: false, systemMuted: false, micMuted: false, micLost: false, systemLost: false };
function applyAudio({ audio_system, audio_mic, audio_system_muted, audio_mic_muted, mic_lost, system_lost }) {
  lastAudio = {
    system: !!audio_system, mic: !!audio_mic,
    systemMuted: !!audio_system_muted, micMuted: !!audio_mic_muted, micLost: !!mic_lost, systemLost: !!system_lost,
  };
  audioSysEl.classList.remove("hidden");
  audioSysEl.classList.toggle("on", lastAudio.system && !lastAudio.systemMuted && !lastAudio.systemLost);
  audioSysEl.classList.toggle("lost", lastAudio.system && lastAudio.systemLost);
  audioSysEl.disabled = !lastAudio.system || lastAudio.systemLost;
  audioMicEl.classList.remove("hidden");
  audioMicEl.classList.toggle("on", lastAudio.mic && !lastAudio.micMuted && !lastAudio.micLost);
  audioMicEl.classList.toggle("lost", lastAudio.mic && lastAudio.micLost);
  audioMicEl.disabled = !lastAudio.mic || lastAudio.micLost;
  if (!tx) return;
  const sysLabel = lastAudio.systemLost
    ? tx("recBarSystemLost")
    : !lastAudio.system
    ? tx("recBarSystemAudioOff")
    : (lastAudio.systemMuted ? tx("recBarUnmuteSystemAudio") : tx("recBarMuteSystemAudio"));
  if (sysLabel) { audioSysEl.title = sysLabel; audioSysEl.setAttribute("aria-label", sysLabel); }
  const micLabel = lastAudio.micLost
    ? tx("recBarMicLost")
    : !lastAudio.mic
      ? tx("recBarMicOff")
      : (lastAudio.micMuted ? tx("recBarUnmuteMic") : tx("recBarMuteMic"));
  if (micLabel) { audioMicEl.title = micLabel; audioMicEl.setAttribute("aria-label", micLabel); }
}
audioSysEl.addEventListener("click", async () => {
  if (!lastAudio.system || lastAudio.systemLost) return;
  const next = !lastAudio.systemMuted;
  try { await invoke("set_audio_muted", { source: "system", muted: next }); } catch { return; }
  applyAudio({
    audio_system: lastAudio.system, audio_mic: lastAudio.mic,
    audio_system_muted: next, audio_mic_muted: lastAudio.micMuted, mic_lost: lastAudio.micLost, system_lost: lastAudio.systemLost,
  });
});
audioMicEl.addEventListener("click", async () => {
  if (!lastAudio.mic || lastAudio.micLost) return;
  const next = !lastAudio.micMuted;
  try { await invoke("set_audio_muted", { source: "mic", muted: next }); } catch { return; }
  applyAudio({
    audio_system: lastAudio.system, audio_mic: lastAudio.mic,
    audio_system_muted: lastAudio.systemMuted, audio_mic_muted: next, mic_lost: lastAudio.micLost, system_lost: lastAudio.systemLost,
  });
});

// Names which of the two hotkeys (another app holds the combination) is affected.
function applyHotkeyWarn(stopOk, pauseOk) {
  const show = !stopOk || !pauseOk;
  hotkeyWarnEl.classList.toggle("hidden", !show);
  if (!show || !tx) return;
  const label = !stopOk && !pauseOk
    ? tx("recBarHotkeyBlockedBoth")
    : !stopOk ? tx("recBarHotkeyBlockedStop") : tx("recBarHotkeyBlockedPause");
  if (label) { hotkeyWarnEl.title = label; hotkeyWarnEl.setAttribute("aria-label", label); }
}

// The mid-recording half of the low-disk warning: fires at most once (the backend reports
// it once) and never hides, a standing reminder instead of a toast this narrow bar has no
// room for; free space recovering does not make the warning untrue.
let lastDiskLow = null;
function applyDiskLow(diskLow) {
  if (!diskLow || !tx) return;
  lastDiskLow = diskLow;
  const [mb, min] = diskLow;
  const label = tx("recDiskLow").replace("{mb}", String(mb)).replace("{min}", String(min));
  diskWarnEl.classList.remove("hidden");
  diskWarnEl.title = label;
  diskWarnEl.setAttribute("aria-label", label);
}

// The window is a fixed 320x56 canvas from show_rec_bar while the bar sizes to its
// children (recbar.css width: max-content); this keeps the window big enough to show
// every button instead of clipping the last off the right edge. One ResizeObserver covers
// the first paint and every later change.
const BAR_MARGIN = 8;
let lastFitW = 0;
let lastFitH = 0;
function fitBar() {
  const rect = bar.getBoundingClientRect();
  const width = Math.ceil(rect.width) + BAR_MARGIN * 2;
  const height = Math.ceil(rect.height) + BAR_MARGIN * 2;
  if (width === lastFitW && height === lastFitH) return;
  lastFitW = width;
  lastFitH = height;
  invoke("fit_rec_bar", { width, height }).catch(() => {});
}
new ResizeObserver(fitBar).observe(bar);

let pollTimer = 0;
// Bumped for every new recording: a poll still waiting on the previous one's
// status must not stop the new timer or hide the bar when it finally returns.
let pollGen = 0;
// While "finish and continue" runs the backend holds the session out of its state, so
// recording_status answers "none" though recording goes on. continueSeq changes at both
// ends of that window so a poll that straddles it cannot close the bar.
let continuing = false;
let continueSeq = 0;
let autoStopping = false;
// True once the take ended through this bar; its buttons then stay off.
let stopped = false;
async function poll() {
  const gen = pollGen;
  const seq = continueSeq;
  try {
    const status = await invoke("recording_status");
    if (gen !== pollGen) return;
    if (!status) {
      if (continuing || seq !== continueSeq) return;
      // Stopped elsewhere (auto-stop when the recorded window closes, or another race):
      // this bar's buttons would only error against a missing session, so close it too.
      clearInterval(pollTimer);
      invoke("hide_rec_bar").catch(() => {});
      return;
    }
    if (status.encode_error) {
      // The encoder gave up (e.g. disk full) and stopped feeding itself: finish the stop
      // properly (closes the file, emits recording-captured with the error) instead of
      // polling a session that looks alive. The poll runs until the stop went through: a
      // refused stop (a continue in flight) is retried on the next tick.
      if (autoStopping) return;
      autoStopping = true;
      invoke("stop_recording")
        .then(() => {
          clearInterval(pollTimer);
          invoke("hide_rec_bar").catch(() => {});
        })
        .catch(() => {})
        .finally(() => { autoStopping = false; });
      return;
    }
    applyPaused(status.paused);
    timeEl.textContent = fmtTimeMMSS(status.elapsed_ms);
    applySize(status.show_size ? status.bytes : 0, status.size_eta_secs);
    applyTargetSwitch(status.target_switch_enabled);
    applyContinueEnabled(status.elapsed_ms);
    applyAudio(status);
    applyHotkeyWarn(status.stop_hotkey_ok, status.pause_hotkey_ok);
    applyDiskLow(status.disk_low);
    // Belt and suspenders alongside the ResizeObserver: runs after every applyX call on
    // the 500ms cadence; fitBar() is a no-op when nothing changed.
    fitBar();
  } catch {
    // transient: the next tick self-corrects
  }
}
pollTimer = setInterval(poll, 500);
poll();

// The window is only hidden between recordings, so this page outlives a recording.
// show_rec_bar announces every new one; without it Stop/Cancel stay disabled and the
// stopped poll timer stays stopped, so the next recording would run behind a dead bar.
function resetForNewRecording() {
  clearInterval(pollTimer);
  pollGen++;
  stopped = false;
  autoStopping = false;
  paused = false;
  pauseBtn.disabled = false;
  stopBtn.disabled = false;
  cancelBtn.disabled = false;
  continueBtn.disabled = true;
  applyPaused(false);
  timeEl.textContent = fmtTimeMMSS(0);
  sizeEl.textContent = "";
  sizeEl.title = "";
  sizeEl.classList.remove("rec-size-warn");
  diskWarnEl.classList.add("hidden");
  lastDiskLow = null;
  continueGapEl.classList.add("hidden");
  clearTimeout(gapHideTimer);
  pollTimer = setInterval(poll, 500);
  poll();
}
listen("rec-bar-reset", resetForNewRecording);

// Opens the overlay recording starts from, in "pick a switch target" mode;
// recorder-target-picked (below) acts on the result.
switchBtn.addEventListener("click", () => {
  if (!switchEnabled) return;
  invoke("open_recorder_switch").catch(() => {});
});
listen("recorder-target-picked", async (e) => {
  const p = e.payload || {};
  if (!p.forSwitch || p.kind !== "window") return;
  try {
    await invoke("switch_recording_target", { targetKind: p.kind, targetId: p.id });
  } catch {
    // Surfaced through the main window's toast (it listens for
    // recording-target-switch-failed); this bar has no toast, and the previous source
    // keeps recording.
  }
});

pauseBtn.addEventListener("click", async () => {
  try {
    await invoke(paused ? "resume_recording" : "pause_recording");
    applyPaused(!paused);
  } catch {}
});
// Closes the running file like Stop, then opens the next one with the same
// target/settings. No target is sent: the backend reopens against whatever the closing
// session was capturing (see finish_and_continue_recording).
continueBtn.addEventListener("click", async () => {
  if (continueBtn.disabled) return;
  continueBtn.disabled = true;
  // Stop and Cancel would find no session while the switch holds it out of the
  // state, so they wait for the switch to end.
  stopBtn.disabled = true;
  cancelBtn.disabled = true;
  continuing = true;
  continueSeq++;
  try {
    const result = await invoke("finish_and_continue_recording");
    showContinueGap(result.gap_ms);
  } catch (err) {
    // The switch failed AFTER the old file was closed for good: the backend already
    // emitted "recording-captured" with every part (the main window's listener shows the
    // result), so this bar just closes like a normal stop.
    if (typeof err === "string" && err.startsWith("recording-continue-failed-stopped:")) {
      stopped = true;
      pauseBtn.disabled = true; stopBtn.disabled = true; cancelBtn.disabled = true; continueBtn.disabled = true;
      clearInterval(pollTimer);
      invoke("hide_rec_bar").catch(() => {});
      return;
    }
    // "recording-too-young-to-continue" (a race with the 5-second guard) or
    // "no active recording" -- the next poll tick re-syncs the button state.
  } finally {
    continuing = false;
    continueSeq++;
    if (!stopped) {
      stopBtn.disabled = false;
      cancelBtn.disabled = false;
    }
  }
});
// Stop and Cancel close the bar only when the command went through. A refused
// one (the recording is still alive) re-enables the buttons and the poll goes
// on; if the recording already ended, the next poll tick closes the bar.
async function endTake(command) {
  pauseBtn.disabled = true; stopBtn.disabled = true; cancelBtn.disabled = true; continueBtn.disabled = true;
  try {
    await invoke(command);
  } catch {
    pauseBtn.disabled = false; stopBtn.disabled = false; cancelBtn.disabled = false;
    return;
  }
  stopped = true;
  clearInterval(pollTimer);
  invoke("hide_rec_bar").catch(() => {});
}
stopBtn.addEventListener("click", () => endTake("stop_recording"));
cancelBtn.addEventListener("click", () => endTake("cancel_recording"));

// Keys act only after the user clicked into the bar (it is shown without focus so the
// recorded app keeps it). Escape maps to Stop (the take is kept), never Cancel, so a
// stray Esc cannot throw a recording away. Both keys go through the buttons.
document.addEventListener("keydown", (e) => {
  if (e.repeat) return;
  if (e.key === "Escape") {
    e.preventDefault();
    if (!stopBtn.disabled) stopBtn.click();
  } else if (e.code === "Space") {
    e.preventDefault();
    if (!pauseBtn.disabled) pauseBtn.click();
  }
});
// A focused button would also fire its own click when Space is released,
// which would undo the pause the key press just did.
document.addEventListener("keyup", (e) => {
  if (e.code === "Space") e.preventDefault();
});

// Dragging the bar's background (not a button) moves it freely to any monitor;
// rec_bar_mark_moved() tells the 200ms follow poll in rec_bar.rs to stop re-snapping it
// for the rest of this recording.
const DRAG_THRESHOLD = 4;
let dragStart = null;
bar.addEventListener("mousedown", (e) => {
  if (e.button !== 0 || e.target.closest("button")) return;
  dragStart = { x: e.screenX, y: e.screenY };
});
window.addEventListener("mousemove", async (e) => {
  if (!dragStart) return;
  if (e.buttons === 0) { dragStart = null; return; }
  if (Math.abs(e.screenX - dragStart.x) < DRAG_THRESHOLD && Math.abs(e.screenY - dragStart.y) < DRAG_THRESHOLD) return;
  dragStart = null;
  invoke("rec_bar_mark_moved").catch(() => {});
  await appWin.startDragging();
});
window.addEventListener("mouseup", () => { dragStart = null; });

import("./floattheme.js").then(({ applyFloatTheme }) => {
  listen("theme-changed", (e) => applyFloatTheme(e.payload?.theme, e.payload?.vars));
  Promise.all([invoke("current_theme"), invoke("theme_vars")])
    .then(([theme, vars]) => applyFloatTheme(theme, vars))
    .catch(() => {});
});
// The bar window is only hidden between recordings, so a language switch in the
// settings has to reach it through this same loader, not just at page load.
function loadTexts() {
  Promise.all([invoke("get_settings"), import("./i18n.js"), import("./floatShared.js")])
    .then(async ([s, { LANGS, fullTexts }, { resolveLang }]) => {
      LANG = resolveLang(s.language, LANGS);
      ({ tx } = await fullTexts(LANG));
      for (const el of document.querySelectorAll("[data-i18n-title]")) {
        const text = tx(el.getAttribute("data-i18n-title"));
        el.title = text;
        el.setAttribute("aria-label", text);
      }
      for (const el of document.querySelectorAll("[data-i18n]")) {
        el.textContent = tx(el.getAttribute("data-i18n"));
      }
      const label = pauseBtnLabel();
      if (label) { pauseBtn.title = label; pauseBtn.setAttribute("aria-label", label); }
      applyTargetSwitch(switchEnabled);
      applyAudio({
        audio_system: lastAudio.system, audio_mic: lastAudio.mic,
        audio_system_muted: lastAudio.systemMuted, audio_mic_muted: lastAudio.micMuted, mic_lost: lastAudio.micLost, system_lost: lastAudio.systemLost,
      });
      applyDiskLow(lastDiskLow);
    })
    .catch(() => {});
}
loadTexts();
listen("language-changed", loadTexts);
