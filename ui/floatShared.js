// Shared by the always-on-top floating windows (pill, folder, launcher, snip
// overlay) and, for the drag-out helper, by the main window too: constants and
// helpers that had drifted into near-duplicate copies. Values are unchanged from
// what each window already used on its own -- this is drift prevention.
import { placePopover } from "./popover.js";

export const COPIED_MS = 900; // "copied!" tile/pill flash duration
export const RING_MIN = 8;    // smallest resize-ring thickness (px)
export const RING_MAX = 60;   // largest resize-ring thickness (px)
export const DRAG_OUT_MODS = { ctrl: "ctrlKey", alt: "altKey", shift: "shiftKey" };

// Resolve the active UI language: the explicit setting first, the browser locale
// as fallback, matched against a list of available language codes (pass the
// canonical LANGS export from i18n.js). Takes that list as a parameter instead of
// importing i18n.js itself, so a caller that lazy-loads the translations (the
// pill/folder windows have to open fast) keeps doing so.
export function resolveLang(settingsLanguage, langs) {
  const pref = (settingsLanguage && settingsLanguage !== "auto"
    ? settingsLanguage
    : navigator.language || "en"
  ).toLowerCase();
  return langs.find((c) => pref.startsWith(c)) || "en";
}

// Windows path -> file URI, so a drop target that understands URIs (browsers, most
// editors) gets a usable reference instead of a bare path string. Every window that
// can drag a file-backed prompt out shares this one implementation: `?` and `#` are
// legal in a file name but survive encodeURI, and a receiver would read them as the
// start of a query string or a fragment and truncate the path there.
export function fileUri(path) {
  const p = encodeURI(path.replace(/\\/g, "/")).replace(/\?/g, "%3F").replace(/#/g, "%23");
  return p.startsWith("//") ? `file:${p}` : `file:///${p}`;
}

// Which of the 7 built-in themes render dark, so every window can set the
// standard `color-scheme` CSS property from the active theme name. Without it
// a WebView2 native control that isn't styled by our own CSS -- the video
// element's built-in three-dot menu is the one that got reported -- always
// renders light, regardless of which theme the app itself is drawn in.
const DARK_THEMES = new Set(["dark", "programmer", "neon", "cyberpunk"]);
export function themeColorScheme(theme) {
  return DARK_THEMES.has(theme) ? "dark" : "light";
}

// The "ffmpeg-download-progress" event payload is { downloaded, total } (bytes),
// not a ready percentage -- both listeners (main window expert menu, video editor)
// compute the same rounded 0-100 value from it, so a payload shape change only
// needs fixing here.
export function ffmpegDownloadPct(payload) {
  const { downloaded, total } = payload || {};
  return total ? Math.min(100, Math.round((downloaded / total) * 100)) : 0;
}

// A rough "how much longer" estimate for a progress bar, shared by export,
// recording conversion and the ffmpeg download. Feed it every progress update;
// it keeps a short rolling window of percent/time pairs and turns the average
// rate of change over that window into a remaining-time guess, so a single
// stall or a single burst does not swing the estimate on its own.
export function makeEtaEstimator() {
  let samples = [];
  return {
    reset() {
      samples = [];
    },
    feed(pct, now = Date.now()) {
      samples.push({ pct, t: now });
      if (samples.length > 11) samples.shift(); // last 11 points -> 10 rate measurements
    },
    // Returns null (show nothing) once too little has run, once the shown
    // percentage has not reached minPct yet, or once what remains is not worth
    // naming a number for; otherwise { key, value? } ready for formatEta below.
    estimate(minPct, minSecs) {
      if (samples.length < 2) return null;
      const last = samples[samples.length - 1];
      if (last.pct < minPct) return null;
      const rates = [];
      for (let i = 1; i < samples.length; i++) {
        const dt = (samples[i].t - samples[i - 1].t) / 1000;
        const dPct = samples[i].pct - samples[i - 1].pct;
        if (dt > 0 && dPct > 0) rates.push(dPct / dt);
      }
      if (!rates.length) return null;
      const avgRate = rates.reduce((a, b) => a + b, 0) / rates.length;
      const remainingSecs = (100 - last.pct) / avgRate;
      if (!(remainingSecs > minSecs)) return null;
      if (remainingSecs >= 3600) return { key: "etaOverHour" };
      if (remainingSecs > 120) return { key: "etaMinutes", value: Math.round(remainingSecs / 60) };
      return { key: "etaSeconds", value: Math.max(5, Math.round(remainingSecs / 5) * 5) };
    },
  };
}

// Shared by the backups list and the video editor's export size note --
// same rounding, same thresholds, so a number never reads differently in
// one place than the other.
export function fmtBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

// Turns an estimate() result into the localized display string, or "" once
// there is nothing worth showing. `t` is the caller's own translation lookup
// (main.js and video.js each have their own, same one-argument shape).
export function formatEta(result, t) {
  if (!result) return "";
  if (result.key === "etaOverHour") return t("etaOverHour");
  return t("etaAbout").replace("{time}", t(result.key).replace("{n}", String(result.value)));
}

// Turns a rejected export_video/convert_recording call into what a person
// should actually see. The backend wraps a classified ffmpeg (or ffmpeg-
// adjacent Windows IO) failure as `{"key":..., "code":..., "raw":...}` JSON
// (ffmpeg_error_json in lib.rs) -- exactly one place builds that shape, so
// this is the one place that reads it back, rather than each caller
// reimplementing the same JSON.parse/fallback dance. Anything that is not
// that JSON shape (the plain "cancelled" and "ffmpeg-missing" sentinels, or
// an unrelated command's own error string) is returned unparsed with no raw
// detail, so an existing `.includes("cancel")`/`.includes("ffmpeg-missing")`
// check elsewhere keeps seeing exactly what it always did.
export function parseFfmpegError(err, t) {
  const text = String(err);
  let payload = null;
  try {
    const parsed = JSON.parse(text);
    if (parsed && typeof parsed.key === "string") payload = parsed;
  } catch {
    payload = null;
  }
  if (!payload) return { message: text, details: "" };
  const message = payload.key === "ffErrOther"
    ? t("ffErrOther").replace("{code}", payload.code != null ? String(payload.code) : "?")
    : t(payload.key);
  return { message, details: payload.raw || "" };
}

// A short synthesised "copy" beep (the copySound expert switch) -- no audio file
// is shipped or downloaded. Every window that can copy (main, pill, folder) calls
// this from its own one success path, each with its own lazily-created
// AudioContext -- there is no audio state to share across windows, only the code.
let copySoundCtx = null;
export function playCopySound(volumePct) {
  if (!(volumePct > 0)) return; // 0 (or off, passed as undefined/NaN) plays nothing
  try {
    if (!copySoundCtx) copySoundCtx = new (window.AudioContext || window.webkitAudioContext)();
    const ctx = copySoundCtx;
    const now = ctx.currentTime;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = "sine";
    osc.frequency.value = 880;
    const peak = Math.min(1, volumePct / 100) * 0.2; // headroom -- never a harsh beep
    gain.gain.setValueAtTime(0, now);
    gain.gain.linearRampToValueAtTime(peak, now + 0.008); // fade in
    gain.gain.linearRampToValueAtTime(0, now + 0.06); // and back out, so it never clicks
    osc.connect(gain).connect(ctx.destination);
    osc.start(now);
    osc.stop(now + 0.07);
  } catch {
    // No output device, or a context the browser refuses to resume -- a missing
    // sound must never be allowed to block or fail a copy.
  }
}

// ---- Shared hold-or-tap "show original" toggle ----
// A press that releases quickly locks the shown state on until pressed once
// more; the same press held past HOLD_MS instead shows it only for as long
// as it is held, reverting the moment it is released. One state machine
// drives the screenshot and video editor's "show original" control, whatever
// reaches it -- a pointer on the toolbar button or a held key. 400ms matches
// the hold delay the number field's press-and-repeat buttons already use.
const HOLD_OR_TAP_MS = 400;
export function makeHoldOrTapToggle(onChange) {
  let shown = false;
  let locked = false;
  let pressed = false;
  let downAt = 0;
  function set(v) {
    if (shown === v) return;
    shown = v;
    onChange(v);
  }
  function press() {
    if (pressed) return; // key auto-repeat, or a second pointerdown mid-press
    pressed = true;
    if (locked) return; // this press is the one that unlocks on release
    downAt = Date.now();
    set(true);
  }
  function release() {
    if (!pressed) return;
    pressed = false;
    if (locked) { locked = false; set(false); return; }
    if (Date.now() - downAt < HOLD_OR_TAP_MS) { locked = true; return; } // stays shown until pressed again
    set(false);
  }
  // Alt-tabbing away, closing the editor or loading a new file mid-press must
  // never leave the original stuck on screen with nothing left to release it.
  function reset() {
    pressed = false;
    locked = false;
    set(false);
  }
  return { press, release, reset, isShown: () => shown };
}

// ---- Shared mouse-only number field ----
// A minus button, the number input itself, a plus button and, when `presets`
// is given, a small popover of common values -- one implementation so the
// video editor's overlay/crop/speed/fade fields and the screenshot editor's
// tool-size fields share the same click/hold/wheel/preset behaviour instead
// of five near-identical copies. Pass an existing <input> via `input` to
// decorate it in place (it keeps its id and any listener already attached --
// used for the video editor's static crop/speed/fade fields); otherwise a
// fresh input is created from `value`/`min`/`max`/`step`. Min/max/step are
// always read live off the input's own attributes rather than captured once,
// so a caller that changes them later (the crop fields' `.max`, set only once
// the clip's real dimensions are known) does not need to tell this control
// again -- only `bigStep` and `presets`, which have no attribute of their
// own, come from the options.
let activeNumPop = null;
let activeNumPopTrigger = null;
function closeActiveNumPop() {
  if (activeNumPop) activeNumPop.remove();
  activeNumPop = null;
  activeNumPopTrigger = null;
}
document.addEventListener("pointerdown", (e) => {
  if (!activeNumPop) return;
  if (activeNumPop.contains(e.target) || e.target === activeNumPopTrigger) return;
  closeActiveNumPop();
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && activeNumPop) closeActiveNumPop();
});

export function numberField({ input, value, min, max, step, bigStep, presets, decreaseLabel, increaseLabel, presetsLabel, decreaseAriaKey, increaseAriaKey, presetsAriaKey, onChange, wheelEnabled = true }) {
  const el = input || document.createElement("input");
  el.type = "number";
  if (min != null) el.min = String(min);
  if (max != null) el.max = String(max);
  if (step != null) el.step = String(step);
  if (value != null) el.value = String(value);

  const liveMin = () => (el.min === "" ? -Infinity : Number(el.min));
  const liveMax = () => (el.max === "" ? Infinity : Number(el.max));
  const liveStep = () => { const s = Number(el.step); return s > 0 ? s : 1; };
  const clampVal = (v) => Math.min(liveMax(), Math.max(liveMin(), v));

  const minusBtn = document.createElement("button");
  minusBtn.type = "button";
  minusBtn.className = "numfield-dec";
  minusBtn.textContent = "−";
  minusBtn.setAttribute("aria-label", decreaseLabel || "Decrease value");
  // Lets the caller's own applyI18n() keep this in sync on every language
  // switch (and, for a lazily-loaded dictionary, once it finishes loading)
  // the same generic way it already refreshes every other [data-i18n-aria]
  // element -- decreaseLabel/increaseLabel/presetsLabel above are only the
  // best-effort value at construction time, otherwise never revisited.
  if (decreaseAriaKey) minusBtn.dataset.i18nAria = decreaseAriaKey;
  // Native stepping (ArrowUp/Down, PageUp/Down) already works on the input
  // itself, so these are mouse/wheel shortcuts, not a second keyboard path --
  // skipped from Tab order to keep it short, and so a <label> wrapping this
  // whole control (see fieldRow() in ui/video.js) forwards a label click to
  // the input, not to whichever button happens to sit first in the DOM.
  minusBtn.tabIndex = -1;

  const plusBtn = document.createElement("button");
  plusBtn.type = "button";
  plusBtn.className = "numfield-inc";
  plusBtn.textContent = "+";
  plusBtn.setAttribute("aria-label", increaseLabel || "Increase value");
  if (increaseAriaKey) plusBtn.dataset.i18nAria = increaseAriaKey;
  plusBtn.tabIndex = -1;

  function refreshButtons() {
    const v = Number(el.value);
    minusBtn.disabled = !(v > liveMin());
    plusBtn.disabled = !(v < liveMax());
  }

  el.addEventListener("change", () => {
    const next = clampVal(Number(el.value) || 0);
    el.value = String(next);
    refreshButtons();
    if (onChange) onChange(next);
  });
  // Lets code that writes `.value` directly from outside (a rect drag, a
  // settings sync) ask this control to re-check its disabled state too --
  // see the call sites next to those writes in ui/video.js.
  el.numFieldRefresh = refreshButtons;

  function stepBy(dir, big) {
    const delta = (big ? (bigStep || liveStep() * 10) : liveStep()) * dir;
    el.value = String(clampVal((Number(el.value) || 0) + delta));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }

  el.addEventListener(
    "wheel",
    (e) => {
      if (!wheelEnabled) return;
      e.preventDefault();
      stepBy(e.deltaY < 0 ? 1 : -1, e.shiftKey);
    },
    { passive: false }
  );

  function armHold(btn, dir) {
    let holdTimeout = null;
    let holdInterval = null;
    const stop = () => {
      clearTimeout(holdTimeout);
      clearInterval(holdInterval);
      holdTimeout = null;
      holdInterval = null;
    };
    btn.addEventListener("pointerdown", (e) => {
      stepBy(dir, e.shiftKey);
      holdTimeout = setTimeout(() => {
        holdInterval = setInterval(() => stepBy(dir, e.shiftKey), 60);
      }, 400);
    });
    ["pointerup", "pointerleave", "pointercancel"].forEach((ev) => btn.addEventListener(ev, stop));
  }
  armHold(minusBtn, -1);
  armHold(plusBtn, 1);

  const wrap = document.createElement("div");
  wrap.className = "numfield";
  if (input && input.parentNode) input.replaceWith(wrap); // wrap takes input's spot, then reclaims it below
  // The input goes first in the DOM (CSS `order` below puts it back visually
  // between the two buttons) so a <label> wrapping this control forwards a
  // click to the input, not to the minus button sitting first otherwise.
  wrap.append(el, minusBtn, plusBtn);

  if (presets && presets.length) {
    const listBtn = document.createElement("button");
    listBtn.type = "button";
    listBtn.className = "numfield-list";
    listBtn.textContent = "▾";
    listBtn.setAttribute("aria-label", presetsLabel || "Common values");
    if (presetsAriaKey) listBtn.dataset.i18nAria = presetsAriaKey;
    listBtn.addEventListener("click", () => {
      if (activeNumPopTrigger === listBtn) { closeActiveNumPop(); return; } // toggle closed
      closeActiveNumPop();
      const pop = document.createElement("div");
      pop.className = "numfield-pop";
      for (const p of presets) {
        const row = document.createElement("button");
        row.type = "button";
        row.textContent = String(p);
        if (Number(el.value) === p) row.classList.add("sel");
        row.addEventListener("pointerdown", (e) => {
          e.preventDefault(); // keep the input focused
          el.value = String(clampVal(p));
          el.dispatchEvent(new Event("change", { bubbles: true }));
          closeActiveNumPop();
        });
        pop.appendChild(row);
      }
      document.body.appendChild(pop);
      placePopover(pop, listBtn.getBoundingClientRect());
      activeNumPop = pop;
      activeNumPopTrigger = listBtn;
    });
    wrap.appendChild(listBtn);
  }

  refreshButtons();
  wrap.setValue = (v) => { el.value = String(clampVal(v)); refreshButtons(); };
  return wrap;
}
