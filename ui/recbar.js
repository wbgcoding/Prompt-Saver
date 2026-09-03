// Recording control bar (238): a small always-on-top strip with a red dot,
// elapsed time, live file size, Pause/Resume, Stop and Cancel. The window
// itself carries WDA_EXCLUDEFROMCAPTURE (set on the Rust side, unconditionally
// -- it must never appear in the recording it controls).
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const bar = document.getElementById("bar");
const dot = document.getElementById("dot");
const timeEl = document.getElementById("time");
const sizeEl = document.getElementById("size");
const pauseBtn = document.getElementById("pause-btn");
const pauseIcon = document.getElementById("pause-icon");
const resumeIcon = document.getElementById("resume-icon");
const stopBtn = document.getElementById("stop-btn");
const cancelBtn = document.getElementById("cancel-btn");

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

let pollTimer = 0;
async function poll() {
  try {
    const status = await invoke("recording_status");
    if (!status) return; // recording already stopped/cancelled elsewhere
    applyPaused(status.paused);
    timeEl.textContent = fmtTime(status.elapsed_ms);
    sizeEl.textContent = fmtSize(status.bytes);
  } catch {
    // transient -- next tick self-corrects
  }
}
pollTimer = setInterval(poll, 500);
poll();

pauseBtn.addEventListener("click", async () => {
  try {
    await invoke(paused ? "resume_recording" : "pause_recording");
    applyPaused(!paused);
  } catch {}
});
stopBtn.addEventListener("click", async () => {
  pauseBtn.disabled = true; stopBtn.disabled = true; cancelBtn.disabled = true;
  try {
    await invoke("stop_recording");
  } catch {}
  clearInterval(pollTimer);
  invoke("hide_rec_bar").catch(() => {});
});
cancelBtn.addEventListener("click", async () => {
  pauseBtn.disabled = true; stopBtn.disabled = true; cancelBtn.disabled = true;
  try {
    await invoke("cancel_recording");
  } catch {}
  clearInterval(pollTimer);
  invoke("hide_rec_bar").catch(() => {});
});

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
  })
  .catch(() => {});
