// Shared by the always-on-top floating windows (pill, folder, launcher, snip
// overlay) and, for the drag-out helper, by the main window too: constants and
// helpers that had drifted into near-duplicate copies. Values are unchanged from
// what each window already used on its own -- this is drift prevention.
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
