import { t } from "./core.js";

// ---- Prompt variables ({#{placeholder}#}) ----
// The {#{ ... }#} fence is deliberately uncommon so a literal "{{" pasted from a
// code block is never mistaken for a variable. The 200-char body leaves room for
// a choice variable's option list, not just a name.
export const VAR_RE = /\{#\{\s*([^}\n]{1,200}?)\s*\}#\}/g;
export function extractVars(text) {
  const seen = [];
  for (const m of (text || "").matchAll(VAR_RE)) {
    const k = m[1].trim();
    if (k && !seen.includes(k)) seen.push(k);
  }
  return seen;
}
export const fillVars = (text, values) => text.replace(VAR_RE, (_, k) => values[k.trim()] ?? "");
// A fence whose body is only whitespace ({#{ }#}) never becomes a real variable:
// extractVars() silently drops it: so it would sit in the copied text unfilled and
// unexplained. Caught before save instead of left a silent trap.
export const hasEmptyVarToken = (text) =>
  [...String(text || "").matchAll(VAR_RE)].some((m) => !m[1].trim());

// A placeholder gets a visible accent wherever raw prompt text is previewed, so
// "does this prompt ask for anything" no longer surprises you only at copy time. DOM
// text nodes + a plain span, never innerHTML: a token is user-typed text.
export function fillVarHighlighted(el, text) {
  el.textContent = "";
  let last = 0;
  for (const m of text.matchAll(VAR_RE)) {
    if (m.index > last) el.appendChild(document.createTextNode(text.slice(last, m.index)));
    const span = document.createElement("span");
    span.className = "var-token";
    span.textContent = m[0];
    el.appendChild(span);
    last = m.index + m[0].length;
  }
  if (last < text.length) el.appendChild(document.createTextNode(text.slice(last)));
}

// Reserved names that fill themselves in at copy time, no dialog. They win
// over a manual variable of the same name (documented in the editor's helper).
export const DYN_VARS = [
  "date", "time", "datetime", "weekday", "yesterday", "tomorrow",
  "week", "month", "year", "timestamp", "epoch",
  "clipboard", "username", "uuid", "random", "counter",
];
export const DYN_SET = new Set(DYN_VARS);
// A reserved name is WRITTEN with a slash fence: {#{/date\}#}. That keeps every plain
// word free for a variable of the user's own: "date" only fills itself in when it is
// spelled /date\. Old prompts are rewritten once by the store migration.
export const DYN_OPEN = "/";
export const DYN_CLOSE = "\\";
export const dynToken = (name) => `${DYN_OPEN}${name}${DYN_CLOSE}`;
export const dynName = (raw) =>
  raw.length > 2 && raw.startsWith(DYN_OPEN) && raw.endsWith(DYN_CLOSE) ? raw.slice(1, -1) : "";

// The app language is a bare ISO code, which is already a valid BCP-47 tag; only
// where the default region would be the wrong one is it spelled out. The app's
// Portuguese is European, so "pt" must not fall back to the Brazilian default.
export const DYN_LOCALES = { pt: "pt-PT" };

export const pad2 = (n) => String(n).padStart(2, "0");

// Same calendar day ± n days, keeping the local time of day (so a DST change never
// shifts the date by one).
export function dayShift(d, days) {
  const out = new Date(d.getTime());
  out.setDate(out.getDate() + days);
  return out;
}
// ISO-8601 week number: the week holding the Thursday of the current week.
export function isoWeek(d) {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  t.setUTCDate(t.getUTCDate() + 4 - (t.getUTCDay() || 7));
  const jan1 = Date.UTC(t.getUTCFullYear(), 0, 1);
  return Math.ceil(((t - jan1) / 86400000 + 1) / 7);
}
// ISO-8601 local timestamp with the UTC offset ("2026-07-27T14:03:22+02:00").
export function isoStamp(d) {
  const off = -d.getTimezoneOffset();
  const sign = off < 0 ? "-" : "+";
  const abs = Math.abs(off);
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}` +
    `T${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}` +
    `${sign}${pad2(Math.floor(abs / 60))}:${pad2(abs % 60)}`;
}
// Short throwaway id: the same platform RNG as the UUID, just eight characters.
export const shortId = () =>
  [...crypto.getRandomValues(new Uint8Array(6))].map((b) => b.toString(36).padStart(2, "0")).join("").slice(0, 8);
// {#{counter}#} counts copies, so its value has to survive a restart. Kept in the
// same ui_texts store the remembered variable values use.
export const VAR_COUNTER_MAX = 1e9;

// RFC 4122 v4, lowercase: built from the platform RNG, no dependency.
export function uuidV4() {
  const b = crypto.getRandomValues(new Uint8Array(16));
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const h = [...b].map((x) => x.toString(16).padStart(2, "0")).join("");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}
// Clipboard text is foreign input pasted into the prompt: cap it so one runaway
// copy can never blow up the clipboard payload.
export const CLIP_VAR_MAX = 100000;
