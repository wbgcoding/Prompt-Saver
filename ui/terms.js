// Fork vocabulary. The app's own texts name its unit ("prompt") in every language;
// a brand that stores something else supplies, per language, the surface forms to
// swap and what to swap them for (brands/<slug>.json → TERMS in brand.js).
//
// Why a substitution instead of {item} placeholders in every string: the word is
// inflected in half the languages we ship, so a single token could not carry the
// case. A form table is the translator's job and leaves the source texts untouched
// — with no table (this brand) nothing runs at all.
import { TERMS } from "./brand.js";

const cache = new Map();

function ruleFor(lang) {
  if (cache.has(lang)) return cache.get(lang);
  const map = TERMS[lang];
  let rule = null;
  if (map && Object.keys(map).length) {
    // Longest form first: "prompts" must not be matched by the rule for "prompt".
    const forms = Object.keys(map).sort((a, b) => b.length - a.length);
    const escaped = forms.map((f) => f.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
    rule = { re: new RegExp(escaped.join("|"), "g"), map };
  }
  cache.set(lang, rule);
  return rule;
}

export const hasTerms = (lang) => !!ruleFor(lang);

// Apply the brand's vocabulary to one finished string. A no-op — same string object
// — for a brand without a table, which is the case that has to stay free.
export function applyTerms(text, lang) {
  const rule = ruleFor(lang);
  if (!rule) return text;
  return String(text).replace(rule.re, (m) => rule.map[m]);
}
