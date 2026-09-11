// Recording control bar: a small always-on-top strip with a red dot,
// elapsed time, live file size, Pause/Resume, Stop and Cancel. The window
// itself carries WDA_EXCLUDEFROMCAPTURE (set on the Rust side, unconditionally
// -- it must never appear in the recording it controls).
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

function fmtTime(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const m = Math.floor(s / 60);
  const ss = s % 60;
  return `${String(m).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
}
function fmtSize(bytes) {
  if (!bytes) return "";
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// recBarSize gates the whole readout, shown from recBarSizeFromMb onward
// so the narrow bar stays narrow in the normal case -- the threshold decision
// itself lives in recording_status (rec_bar_size_state), this only renders
// what it already decided. etaSecs is only ever set past three quarters of
// recMaxMb, where the text turns into a warm color and gets a tooltip with
// the estimated time left at the recording's own observed rate so far.
function applySize(bytes, etaSecs) {
  sizeEl.textContent = fmtSize(bytes);
  sizeEl.classList.toggle("rec-size-warn", etaSecs != null);
  sizeEl.title = etaSecs != null && i18nDict ? (i18nDict.recBarSizeEta || "").replace("{time}", fmtTime(etaSecs * 1000)) : "";
}

// recTargetSwitch's own button: stays visible even when it cannot do
// anything right now (a Monitor/Region recording, or a Window recording that
// never got a placement canvas) rather than popping in and out, with a
// tooltip naming why. The actual "same screen" check for a canvas-mode
// "monitor" recording only happens once something is actually picked (see
// the recording-target-switch-failed listener in main.js) -- this only
// covers the up-front, always-knowable half of the gate.
let switchEnabled = false;
function applyTargetSwitch(enabled) {
  switchEnabled = enabled;
  switchBtn.disabled = !enabled;
  if (!i18nDict) return;
  const label = enabled ? i18nDict.recBarSwitchTarget : i18nDict.recTargetSwitchUnsupported;
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
// hidden from the user -- not localized, same as fmtSize's "KB"/"MB".
let gapHideTimer = 0;
function showContinueGap(gapMs) {
  clearTimeout(gapHideTimer);
  continueGapEl.textContent = gapMs >= 1000 ? `+${(gapMs / 1000).toFixed(1)}s` : `+${gapMs}ms`;
  continueGapEl.classList.remove("hidden");
  gapHideTimer = setTimeout(() => continueGapEl.classList.add("hidden"), 4000);
}

let paused = false;
let i18nDict = null;
function pauseBtnLabel() {
  if (!i18nDict) return null;
  const key = paused ? "recBarResume" : "recBarPause";
  return i18nDict[key] ?? i18nDict.recBarPause;
}
function applyPaused(p) {
  paused = p;
  bar.classList.toggle("paused", p);
  pauseIcon.classList.toggle("hidden", p);
  resumeIcon.classList.toggle("hidden", !p);
  const label = pauseBtnLabel();
  if (label) { pauseBtn.title = label; pauseBtn.setAttribute("aria-label", label); }
}

// System-audio / mic buttons, shown for the whole recording (never hidden) so
// the user can always see at a glance what is being captured -- dim when that
// source is off or muted, lit when on. audio_system/audio_mic are each a
// fixed per-session choice (set once when the recording starts) that decides
// whether the button can be used at all; 332 added mid-recording muting on
// top (audio_system_muted/audio_mic_muted), toggled by clicking the button.
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
  if (!i18nDict) return;
  const sysLabel = !lastAudio.system
    ? i18nDict.recBarSystemAudioOff
    : (lastAudio.systemMuted ? i18nDict.recBarUnmuteSystemAudio : i18nDict.recBarMuteSystemAudio);
  if (sysLabel) { audioSysEl.title = sysLabel; audioSysEl.setAttribute("aria-label", sysLabel); }
  const micLabel = lastAudio.micLost
    ? i18nDict.recBarMicLost
    : !lastAudio.mic
      ? i18nDict.recBarMicOff
      : (lastAudio.micMuted ? i18nDict.recBarUnmuteMic : i18nDict.recBarMuteMic);
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
  if (!show || !i18nDict) return;
  const label = !stopOk && !pauseOk
    ? i18nDict.recBarHotkeyBlockedBoth
    : !stopOk ? i18nDict.recBarHotkeyBlockedStop : i18nDict.recBarHotkeyBlockedPause;
  if (label) { hotkeyWarnEl.title = label; hotkeyWarnEl.setAttribute("aria-label", label); }
}

// recDiskWarnMb's mid-recording half -- fires at most once (the backend
// already only reports it once), never hides again: a quiet standing
// reminder rather than a toast this narrow bar has no room for, and free
// space recovering mid-recording does not make the earlier warning untrue.
function applyDiskLow(diskLow) {
  if (!diskLow || !i18nDict) return;
  const [mb, min] = diskLow;
  const label = (i18nDict.recDiskLow || "").replace("{mb}", String(mb)).replace("{min}", String(min));
  diskWarnEl.classList.remove("hidden");
  diskWarnEl.title = label;
  diskWarnEl.setAttribute("aria-label", label);
}

let pollTimer = 0;
async function poll() {
  try {
    const status = await invoke("recording_status");
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
    timeEl.textContent = fmtTime(status.elapsed_ms);
    applySize(status.show_size ? status.bytes : 0, status.size_eta_secs);
    applyTargetSwitch(status.target_switch_enabled);
    applyContinueEnabled(status.elapsed_ms);
    applyAudio(status);
    applyHotkeyWarn(status.stop_hotkey_ok, status.pause_hotkey_ok);
    applyDiskLow(status.disk_low);
  } catch {
    // transient -- next tick self-corrects
  }
}
pollTimer = setInterval(poll, 500);
poll();

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
    // Surfaced to the user via the main window's own toast (main.js listens
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
    // folded in (main.js's own listener shows the result), this bar just
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

// The bar used to sit locked to the recorded window's corner. Dragging
// from its background (not a button) now moves it freely to any monitor;
// rec_bar_mark_moved() tells the 200ms follow poll in lib.rs to stop
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
Promise.all([invoke("get_settings"), import("./i18n.js"), import("./floatShared.js")])
  .then(([s, { I18N, LANGS }, { resolveLang }]) => {
    const lang = resolveLang(s.language, LANGS);
    const d = I18N[lang] || I18N.en;
    i18nDict = d;
    for (const el of document.querySelectorAll("[data-i18n-title]")) {
      const key = el.getAttribute("data-i18n-title");
      const text = d[key] ?? I18N.en[key];
      if (text) { el.title = text; el.setAttribute("aria-label", text); }
    }
    const label = pauseBtnLabel();
    if (label) { pauseBtn.title = label; pauseBtn.setAttribute("aria-label", label); }
    applyTargetSwitch(switchEnabled);
    applyAudio({
      audio_system: lastAudio.system, audio_mic: lastAudio.mic,
      audio_system_muted: lastAudio.systemMuted, audio_mic_muted: lastAudio.micMuted, mic_lost: lastAudio.micLost,
    });
  })
  .catch(() => {});
