// Text loader. Each language lives in its own file under i18n/, so a window only
// fetches the language it shows (plus English as the fallback). The tray menu is the
// one exception: its labels come from the backend, because the native menu has no JS.
import FLOAT_TXT from "./i18n/float.js";
import { applyTerms } from "./terms.js";
import { APP_NAME } from "./brand.js";
export const LANGS = ["en", "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro"];
export { FLOAT_TXT };
const loaded = new Map();
export function primeLang(code, dict) { loaded.set(code, Promise.resolve(dict)); }
export function loadLang(code) {
  const c = LANGS.includes(code) ? code : "en";
  if (!loaded.has(c)) loaded.set(c, import(`./i18n/${c}.js`).then((m) => m.default));
  return loaded.get(c);
}
const fill = (raw, code) => applyTerms(String(raw).replaceAll("{app}", APP_NAME), code);
// Full dictionary: windows with many strings (main, video, camera, recorder bar, snip).
export async function fullTexts(code) {
  const [dict, en] = await Promise.all([loadLang(code), loadLang("en")]);
  return { lang: code, dict, en, tx: (key) => fill(dict[key] ?? en[key] ?? key, code) };
}
// Compact strings: the small windows (pills, pill folder, launcher, placeholder prompt).
export function compactTexts(code) {
  const src = { ...FLOAT_TXT.en, ...(FLOAT_TXT[code] || {}) };
  return Object.fromEntries(Object.entries(src).map(([k, v]) => [k, fill(v, code)]));
}
