// Shared across every always-on-top floating window (pill, folder, launcher, snip
// overlay): constants and a helper that had drifted into near-duplicate copies
// (125). Values are unchanged from what each window already used on its own --
// this is drift prevention, not a behaviour change.
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
