// Recording control bar: a small always-on-top strip with a red dot,
// elapsed time, live file size, Pause/Resume, Stop and Cancel. The window
// itself carries WDA_EXCLUDEFROMCAPTURE (set on the Rust side, unconditionally
// -- it must never appear in the recording it controls).
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

// recBarSize gates the whole readout, shown from 500 MB onward
// so the narrow bar stays narrow in the normal case -- the threshold decision
// itself lives in recording_status (rec_bar_size_state), this only renders
// what it already decided. etaSecs is only ever set past three quarters of
// recMaxMb, where the text turns into a warm color and gets a tooltip with
// the estimated time left at the recording's own observed rate so far.
function applySize(bytes, etaSecs) {
  sizeEl.textContent = fmtSizeCompact(bytes, LANG);
  sizeEl.classList.toggle("rec-size-warn", etaSecs != null);
  sizeEl.title = etaSecs != null && tx ? tx("recBarSizeEta").replace("{time}", fmtTimeMMSS(etaSecs * 1000)) : "";
}

// recTargetSwitch's own button: stays visible even when it cannot do
// anything right now (a Monitor/Region recording, or a Window recording that
// never got a placement canvas) rather than popping in and out, with a
// tooltip naming why. The actual "same screen" check for a canvas-mode
// "monitor" recording only happens once something is actually picked (see
// the recording-target-switch-failed listener in the main window) -- this only
// covers the up-front, always-knowable half of the gate.
let switchEnabled = false;
function applyTargetSwitch(enabled) {
  switchEnabled = enabled;
  switchBtn.disabled = !enabled;
  if (!tx) return;
  const label = enabled ? tx("recBarSwitchTarget") : tx("recTargetSwitchUnsupported");
  if (label) { switchBtn.title = label; switchBtn.setAttribute("aria-label", label); }
}

// Mirrors the backend's own 5-second refusal (finish_and_continue_recording)
// so the button is never enabled for a click the backend would reject anyway
// -- the backend guard is still what actually decides, this only avoids a
// pointless round trip during the riskiest moment for a file switch.
const CONTINUE_LOCKOUT_MS = 5000;
function applyContinueEnabled(elapsedMs) {
  continueBtn.disabled = elapsedMs < CONTINUE_LOCKOUT_MS;
}

// A successful continue's own gap (ms the switch itself took, start to
// finish) is shown for a few seconds right where the file size normally
// sits, so the missing moment between the two files is never silently
// hidden from the user -- not localized, same as fmtSizeCompact's "KB"/"MB".
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
  // Reuses the same button label a sighted user just saw change (Pause/Resume):
  // that already names which state the recording is in now.
  if (changed && label) {
    const live = document.getElementById("live");
    live.textContent = "";
    live.textContent = label;
  }
}

// System-audio / mic buttons, shown for the whole recording (never hidden) so
// the user can always see at a glance what is being captured -- dim when that
// source is off or muted, lit when on. audio_system/audio_mic are each a
// fixed per-session choice (set once when the recording starts) that decides
// whether the button can be used at all; mid-recording muting sits on top
// (audio_system_muted/audio_mic_muted), toggled by clicking the button.
// mic_lost can flip true mid-recording if the device gets unplugged, and latches.
let lastAudio = { system: false, mic: false, systemMuted: false, micMuted: false, micLost: false };
function applyAudio({ audio_system, audio_mic, audio_system_muted, audio_mic_muted, mic_lost }) {
  lastAudio = {
    system: !!audio_system, mic: !!audio_mic,
    systemMuted: !!audio_system_muted, micMuted: !!audio_mic_muted, micLost: !!mic_lost,
  };
  audioSysEl.classList.remove("hidden");
  audioSysEl.classList.toggle("on", lastAudio.system && !lastAudio.systemMuted);
  audioSysEl.disabled = !lastAudio.system;
  audioMicEl.classList.remove("hidden");
  audioMicEl.classList.toggle("on", lastAudio.mic && !lastAudio.micMuted && !lastAudio.micLost);
  audioMicEl.classList.toggle("lost", lastAudio.mic && lastAudio.micLost);
  audioMicEl.disabled = !lastAudio.mic || lastAudio.micLost;
  if (!tx) return;
  const sysLabel = !lastAudio.system
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
  if (!lastAudio.system) return;
  const next = !lastAudio.systemMuted;
  try { await invoke("set_audio_muted", { source: "system", muted: next }); } catch { return; }
  applyAudio({
    audio_system: lastAudio.system, audio_mic: lastAudio.mic,
    audio_system_muted: next, audio_mic_muted: lastAudio.micMuted, mic_lost: lastAudio.micLost,
  });
});
audioMicEl.addEventListener("click", async () => {
  if (!lastAudio.mic || lastAudio.micLost) return;
  const next = !lastAudio.micMuted;
  try { await invoke("set_audio_muted", { source: "mic", muted: next }); } catch { return; }
  applyAudio({
    audio_system: lastAudio.system, audio_mic: lastAudio.mic,
    audio_system_muted: lastAudio.systemMuted, audio_mic_muted: next, mic_lost: lastAudio.micLost,
  });
});

// Names which of the two hotkeys (another app already holds the
// combination) is affected, so "it doesn't work" always has an answer.
function applyHotkeyWarn(stopOk, pauseOk) {
  const show = !stopOk || !pauseOk;
  hotkeyWarnEl.classList.toggle("hidden", !show);
  if (!show || !tx) return;
  const label = !stopOk && !pauseOk
    ? tx("recBarHotkeyBlockedBoth")
    : !stopOk ? tx("recBarHotkeyBlockedStop") : tx("recBarHotkeyBlockedPause");
  if (label) { hotkeyWarnEl.title = label; hotkeyWarnEl.setAttribute("aria-label", label); }
}

// The mid-recording half of the low-disk warning -- fires at most once (the backend
// already only reports it once), never hides again: a quiet standing
// reminder rather than a toast this narrow bar has no room for, and free
// space recovering mid-recording does not make the earlier warning untrue.
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

// The window itself is a fixed 320x56 canvas from show_rec_bar; the bar's
// content now sizes to its own children (recbar.css's width: max-content), so
// this is what keeps the window big enough to actually show every one of
// them instead of clipping the last buttons off the right edge. Covers the
// first paint and every later change (an audio icon or warning appearing,
// the continue gap showing) through one ResizeObserver rather than a list of
// events to remember to also call this from.
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
async function poll() {
  const gen = pollGen;
  try {
    const status = await invoke("recording_status");
    if (gen !== pollGen) return;
    if (!status) {
      // Stopped elsewhere -- the auto-stop path when the recorded window itself
      // closes, or any other race. This bar's own buttons would now just error
      // against a session that no longer exists, so close it too.
      clearInterval(pollTimer);
      invoke("hide_rec_bar").catch(() => {});
      return;
    }
    if (status.encode_error) {
      // The encoder already gave up (e.g. disk full) and stopped feeding
      // itself -- finish the stop properly (closes the file, emits
      // recording-captured with the error message) rather than sitting here
      // polling a session that looks alive but is no longer writing video.
      clearInterval(pollTimer);
      invoke("stop_recording").catch(() => {});
      invoke("hide_rec_bar").catch(() => {});
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
    // Belt and suspenders alongside the ResizeObserver above: every one of
    // the applyX calls just above can show or hide an element, and this runs
    // right after every one of them regardless, on the same 500ms cadence
    // that already drives the rest of the bar -- fitBar()'s own width/height
    // check is a no-op when nothing actually changed.
    fitBar();
  } catch {
    // transient -- next tick self-corrects
  }
}
pollTimer = setInterval(poll, 500);
poll();

// The window is only hidden between recordings, never closed, so this page
// outlives a recording. show_rec_bar announces every new one; without it the
// buttons Stop/Cancel disabled and the stopped poll timer would stay that way
// and the next recording would run behind a dead bar.
function resetForNewRecording() {
  clearInterval(pollTimer);
  pollGen++;
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

// Opens the same overlay recording itself starts from, in "pick a switch
// target" mode -- recorder-target-picked (below) is what actually acts on
// the result, since the overlay closes and reopens no window of its own.
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
    // Surfaced to the user via the main window's own toast (the main window listens
    // for recording-target-switch-failed) -- this bar has no toast of its own
    // and the previous source is already still recording regardless.
  }
});

pauseBtn.addEventListener("click", async () => {
  try {
    await invoke(paused ? "resume_recording" : "pause_recording");
    applyPaused(!paused);
  } catch {}
});
// Closes the running file exactly like Stop, then immediately opens the
// next one with the same target/settings -- no target is sent along, the
// backend reopens against whatever the session it is closing was actually
// capturing (see finish_and_continue_recording).
continueBtn.addEventListener("click", async () => {
  if (continueBtn.disabled) return;
  continueBtn.disabled = true;
  try {
    const result = await invoke("finish_and_continue_recording");
    showContinueGap(result.gap_ms);
  } catch (err) {
    // The switch failed AFTER the old file was already closed for good --
    // the backend already emitted "recording-captured" with every part
    // folded in (the main window's own listener shows the result), this bar just
    // closes like a normal stop instead of surfacing a second, redundant
    // error on top of that.
    if (typeof err === "string" && err.startsWith("recording-continue-failed-stopped:")) {
      pauseBtn.disabled = true; stopBtn.disabled = true; cancelBtn.disabled = true; continueBtn.disabled = true;
      clearInterval(pollTimer);
      invoke("hide_rec_bar").catch(() => {});
      return;
    }
    // "recording-too-young-to-continue" (a race with the 5-second guard) or
    // "no active recording" -- the next poll tick re-syncs the button state.
  }
});
stopBtn.addEventListener("click", async () => {
  pauseBtn.disabled = true; stopBtn.disabled = true; cancelBtn.disabled = true; continueBtn.disabled = true;
  try {
    await invoke("stop_recording");
  } catch {}
  clearInterval(pollTimer);
  invoke("hide_rec_bar").catch(() => {});
});
cancelBtn.addEventListener("click", async () => {
  pauseBtn.disabled = true; stopBtn.disabled = true; cancelBtn.disabled = true; continueBtn.disabled = true;
  try {
    await invoke("cancel_recording");
  } catch {}
  clearInterval(pollTimer);
  invoke("hide_rec_bar").catch(() => {});
});

// Keys act only once the user has clicked into the bar: the bar is shown
// without focus on purpose, so the recorded app keeps it. Escape maps to Stop
// (the take is kept), never to Cancel, so a stray Esc cannot throw a
// recording away. Both keys go through the buttons, so pause/resume and stop
// reach the backend through the same commands as a click.
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

// The bar used to sit locked to the recorded window's corner. Dragging
// from its background (not a button) now moves it freely to any monitor;
// rec_bar_mark_moved() tells the 200ms follow poll in rec_bar.rs to stop
// re-snapping it back for the rest of this recording -- its new position IS
// the remembered one, simply by no longer being overwritten.
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

// Theme + language: same shared helpers every satellite window uses.
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
        audio_system_muted: lastAudio.systemMuted, audio_mic_muted: lastAudio.micMuted, mic_lost: lastAudio.micLost,
      });
      applyDiskLow(lastDiskLow);
    })
    .catch(() => {});
}
loadTexts();
listen("language-changed", loadTexts);
