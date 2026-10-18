import { flag, invoke, LANG, settings, t, toast, TOOLTIP_PREVIEW_CHARS, txt } from "./core.js";
import { activeThemeVars } from "./theme.js";
import { varPreview, VAR_RE } from "./floatShared.js";

// ---- Prompt variables ({?placeholder}) ----
// A `{` directly followed by `?` and a letter, digit, `_` or `@` is a placeholder; that
// start does not occur in JSON or code, so pasted code is never mistaken for one. The
// 200-char body leaves room for a choice variable's option list. The pattern (VAR_RE)
// lives in floatShared.js so the pill and folder windows test the same one.
export function extractVars(text) {
  const seen = [];
  for (const m of (text || "").matchAll(VAR_RE)) {
    const k = m[1].trim();
    if (k && !seen.includes(k)) seen.push(k);
  }
  return seen;
}
export const fillVars = (text, values) => text.replace(VAR_RE, (_, k) => values[k.trim()] ?? "");
// A `{?` with no name ({?}, {? x}) never becomes a variable, so it would sit in the
// copied text unfilled and unexplained; caught before save.
const EMPTY_TOKEN_RE = /\{\?(?![\p{L}\p{N}_@])[^}\n]{0,200}\}/u;
export const hasEmptyVarToken = (text) => EMPTY_TOKEN_RE.test(String(text || ""));

// A placeholder gets a visible accent wherever raw prompt text is previewed. It reads as
// [name] (varPreview) and that shortened length counts against the preview limit. DOM
// text nodes + a plain span, never innerHTML: a token is user-typed text.
export function fillVarHighlighted(el, text) {
  el.textContent = "";
  let used = 0;
  let last = 0;
  const room = () => TOOLTIP_PREVIEW_CHARS - used;
  const addText = (s) => {
    if (!s) return;
    const part = s.slice(0, Math.max(0, room()));
    if (part) el.appendChild(document.createTextNode(part));
    used += part.length;
  };
  for (const m of text.matchAll(VAR_RE)) {
    if (room() <= 0) break;
    addText(text.slice(last, m.index));
    if (room() <= 0) { last = text.length; break; }
    const shown = varPreview(m[0]);
    const span = document.createElement("span");
    span.className = "var-token";
    span.textContent = shown;
    el.appendChild(span);
    used += shown.length;
    last = m.index + m[0].length;
  }
  addText(text.slice(last));
  if (varPreview(text).length > TOOLTIP_PREVIEW_CHARS) el.appendChild(document.createTextNode("…"));
}

// Reserved names that fill themselves in at copy time, no dialog. They win
// over a manual variable of the same name (documented in the editor's helper).
export const DYN_VARS = [
  "date", "time", "datetime", "weekday", "yesterday", "tomorrow",
  "week", "month", "year", "timestamp", "epoch",
  "clipboard", "username", "uuid", "random", "counter",
];
export const DYN_SET = new Set(DYN_VARS);
// A reserved name is WRITTEN with an @ in front: {?@date}. That keeps every plain word
// free for a variable of the user's own: "date" only fills itself in when spelled @date.
export const DYN_OPEN = "@";
export const dynToken = (name) => `${DYN_OPEN}${name}`;
export const dynName = (raw) => (raw.length > 1 && raw.startsWith(DYN_OPEN) ? raw.slice(1) : "");

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
// {?@counter} counts copies, so its value has to survive a restart. Kept in the
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

// A token is `name`, `name=default`, `name|opt,opt,…` or `name=default|opt,opt,…`.
// Options are trimmed and empty ones dropped; a comma always starts the next option. A
// default selects the matching entry in a choice list, or prefills a plain text variable
// without a list. A bar or equals sign without a usable name before it leaves the whole
// token as the name of an ordinary text variable, with no default.
export function parseVar(token) {
  const raw = String(token).trim();
  const bar = raw.indexOf("|");
  const namePart = bar < 0 ? raw : raw.slice(0, bar);
  const eq = namePart.indexOf("=");
  const rawName = eq < 0 ? namePart.trim() : namePart.slice(0, eq).trim();
  const rawDefault = eq < 0 ? "" : namePart.slice(eq + 1).trim();
  if (bar < 0 || !flag("choiceVars")) {
    return rawName ? { name: rawName, options: [], default: rawDefault } : { name: raw, options: [], default: "" };
  }
  const options = raw.slice(bar + 1).split(",").map((o) => o.trim()).filter(Boolean);
  return rawName && options.length
    ? { name: rawName, options, default: rawDefault }
    : { name: raw, options: [], default: "" };
}

export const isDynVar = (name) => flag("dynVars") && DYN_SET.has(dynName(name));
export const dynLocale = () => DYN_LOCALES[LANG] || LANG;
export const intlFmt = (opts) => {
  try { return new Intl.DateTimeFormat(dynLocale(), opts); } catch (_) { return new Intl.DateTimeFormat("en", opts); }
};
export function fmtDynDate(d, style) {
  if (style === "iso") return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
  return intlFmt({ dateStyle: style }).format(d);
}
export function fmtDynTime(d, style) {
  if (style === "iso") return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
  return intlFmt({ timeStyle: style }).format(d);
}
export function nextVarCounter() {
  const now = Math.floor(Number(settings.ui_texts?.varCounter) || 0);
  const next = now >= 0 && now < VAR_COUNTER_MAX ? now + 1 : 1;
  settings.ui_texts = { ...(settings.ui_texts || {}), varCounter: String(next) };
  invoke("set_ui_text", { key: "varCounter", value: String(next) }).catch(() => {});
  return next;
}

// Resolve one reserved name. `ctx` carries the values that cost a round trip or a
// write, fetched once per copy: the clipboard, the account name, the counter.
export function dynValue(name, now, ctx) {
  const ds = txt("dateStyle") || "short";
  const ts = txt("timeStyle") || "short";
  switch (name) {
    case "date": return fmtDynDate(now, ds);
    case "time": return fmtDynTime(now, ts);
    case "datetime": return `${fmtDynDate(now, ds)} ${fmtDynTime(now, ts)}`;
    case "weekday": return intlFmt({ weekday: "long" }).format(now);
    case "yesterday": return fmtDynDate(dayShift(now, -1), ds);
    case "tomorrow": return fmtDynDate(dayShift(now, 1), ds);
    case "week": return String(isoWeek(now));
    case "month": return intlFmt({ month: "long" }).format(now);
    case "year": return String(now.getFullYear());
    case "timestamp": return isoStamp(now);
    case "epoch": return String(Math.floor(now.getTime() / 1000));
    case "clipboard": return ctx.clip;
    case "username": return ctx.user;
    case "uuid": return uuidV4();
    case "random": return shortId();
    case "counter": return String(ctx.counter);
    default: return "";
  }
}

// The variable tokens of a prompt that are worth resolving at all (text prompts only: an
// image/file prompt copies its media, not its text). `text` overrides which string is
// scanned (the rotated entry about to be copied).
export const varTokens = (p, text) =>
  (flag("promptVars") && p && !p.copy_image && !p.file_path ? extractVars(text ?? p.text) : []);
// The subset the user still has to fill in by hand: the only reason to open a dialog. An
// override text (the peeked rotating entry) must pass through as in varTokens, or a
// placeholder living only in a later entry goes unnoticed.
export const dialogTokens = (p, text) => varTokens(p, text).filter((tok) => !isDynVar(parseVar(tok).name));
// True while any entry of a rotating prompt still carries a placeholder. An auto-advance
// session renders later entries from their raw text, so such a prompt must not open one,
// or a later paste would deliver the placeholder itself; unless every entry was filled up
// front (copyResolved hands them to rotation_arm_session).
export const anyEntryHasVars = (p) =>
  [p.text, ...(p.variants || [])].some((entry) => varTokens(p, entry).length > 0);

// The text a copy of `p` is about to use: the pointer's CURRENT entry for a rotating
// prompt, p.text otherwise. Read-only (never advances the pointer), so it is safe to call
// ahead of a decision (dialogTokens). Resolving p.text directly instead mis-detects and
// mis-copies once rotation has moved past entry 0.
export async function peekRotatedText(p) {
  if (!(p.variants || []).length) return p.text;
  const peek = await invoke("rotation_peek", { id: p.id }).catch(() => null);
  return peek ? peek.text : p.text;
}

// The one place every copy path goes through: reserved names resolve themselves,
// the rest are asked for once. Returns the finished text, null when the user
// cancelled, or when the small window the placeholders are asked in could not be
// opened (a toast says so; every caller treats null as a cancel).
export async function resolveVars(text) {
  const out = await resolveVarsList([text]);
  return out ? out[0] : null;
}

// Several texts filled from ONE round: the union of their placeholders is asked
// once and every text gets the same values (an auto-advancing prompt's entries).
// Returns the filled texts in input order, or null exactly where resolveVars does.
export async function resolveVarsList(texts) {
  const tokens = [...new Set(texts.flatMap((text) => extractVars(text)))];
  if (!tokens.length) return texts.slice();
  const values = {};
  const ask = [];
  const dyn = [];
  for (const tok of tokens) {
    (isDynVar(parseVar(tok).name) ? dyn : ask).push(tok);
  }
  const ctx = { clip: "", user: "", counter: 0 };
  const isCounter = (tok) => dynName(parseVar(tok).name) === "counter";
  if (dyn.length) {
    const names = new Set(dyn.map((tok) => dynName(parseVar(tok).name)));
    if (names.has("clipboard")) {
      ctx.clip = String(await invoke("clipboard_text").catch(() => "") || "").slice(0, CLIP_VAR_MAX);
      if (!ctx.clip) toast(t("clipVarEmpty")); // image/file payload, or an empty clipboard
    }
    if (names.has("username")) ctx.user = String(await invoke("os_user_name").catch(() => "") || "");
    const now = new Date();
    for (const tok of dyn) if (!isCounter(tok)) values[tok] = dynValue(dynName(parseVar(tok).name), now, ctx);
  }
  if (ask.length) {
    const filled = await promptVarsWindow(ask);
    if (filled === false) { toast(t("varsWindowFailed")); return null; }
    if (!filled) return null;
    Object.assign(values, filled);
  }
  // The counter advances only once the copy is going ahead: a cancelled dialog must not burn a value.
  if (dyn.some(isCounter)) {
    ctx.counter = nextVarCounter();
    const now = new Date();
    for (const tok of dyn) if (isCounter(tok)) values[tok] = dynValue("counter", now, ctx);
  }
  return texts.map((text) => fillVars(text, values));
}

// Values the user asked to remember, keyed by variable NAME (expert: varDefaults).
// Both fill-in surfaces read and write them through these two helpers.
export function readRememberedVars() {
  try { return JSON.parse(settings.ui_texts?.varDefaults || "{}"); } catch (_) { return {}; }
}
export function saveRememberedVars(next, prev) {
  const json = JSON.stringify(next);
  if (json === JSON.stringify(prev)) return;
  settings.ui_texts = { ...(settings.ui_texts || {}), varDefaults: json };
  invoke("set_ui_text", { key: "varDefaults", value: json }).catch(() => {});
}

// Fixed placeholder values, by variable NAME (expert: varFixed), maintained BY HAND in
// the expert menu and never written by a fill-in dialog. Kept apart from varDefaults so a
// deliberate value is never confused with whatever the user typed last time.
export function readFixedVars() {
  try { return JSON.parse(settings.ui_texts?.varFixed || "{}"); } catch (_) { return {}; }
}
export function saveFixedVars(next) {
  const json = JSON.stringify(next);
  if (json === (settings.ui_texts?.varFixed || "{}")) return;
  settings.ui_texts = { ...(settings.ui_texts || {}), varFixed: json };
  invoke("set_ui_text", { key: "varFixed", value: json }).catch(() => {});
}

// ---- Standalone fill-in window (vars.html) ----
// Every copy that needs typed placeholders asks here, inside the app or not: a small
// always-on-top window next to the pointer, so the app the user works in keeps the
// foreground. What the variables MEAN lives here; that window is a form that renders rows
// and hands the answers back.
export let varsWindowPending = null;
export function assignVarsWindowPending(v) { varsWindowPending = v; }

export async function promptVarsWindow(tokens) {
  const useRemember = flag("varDefaults");
  const keepChoice = flag("choiceRemember");
  const remembered = readRememberedVars();
  // Fixed values (expert: varFixed) sit between a remembered pick and the
  // text's own default. A plain text field only ever SUGGESTS one, as its
  // placeholder: vars.js never inserts it into the value on its own.
  const useFixed = flag("varFixed");
  const fixedVars = useFixed ? readFixedVars() : {};
  const rows = tokens.map((token) => {
    const { name, options, default: def } = parseVar(token);
    const last = remembered[name];
    if (options.length) {
      const usedLast = keepChoice && options.includes(last);
      const usedFixed = !usedLast && options.includes(fixedVars[name]);
      return {
        token,
        name,
        options,
        value: usedLast ? last : usedFixed ? fixedVars[name] : (def && options.includes(def) ? def : options[0]),
        remember: false,
        fixed: usedFixed,
        placeholder: "",
      };
    }
    const usedRemember = useRemember && name in remembered;
    const usedFixed = !usedRemember && useFixed && name in fixedVars;
    return {
      token,
      name,
      options,
      value: usedRemember ? String(last) : usedFixed ? "" : (def || ""),
      remember: usedRemember,
      fixed: usedFixed,
      placeholder: usedFixed ? fixedVars[name] : "",
    };
  });
  const payload = {
    theme: document.documentElement.getAttribute("data-theme") || "",
    vars: activeThemeVars(),
    lang: LANG,
    askRemember: useRemember,
    rows: rows.map(({ name, options, value, remember, fixed, placeholder }) => ({ name, options, value, remember, fixed, placeholder })),
  };
  // A second hotkey while a window is still open cancels the older copy: one
  // pending question at a time, or two answers would race for one clipboard.
  const stale = varsWindowPending;
  varsWindowPending = null;
  stale?.(null);
  const answer = await new Promise((resolve) => {
    varsWindowPending = resolve;
    invoke("open_var_window", { payload: JSON.stringify(payload), rows: rows.length })
      .catch(() => { if (varsWindowPending === resolve) varsWindowPending = null; resolve(false); });
  });
  if (answer === false) return false; // no window could be opened
  if (!answer) return null;
  let parsed;
  try { parsed = JSON.parse(answer); } catch (_) { return null; }
  if (!Array.isArray(parsed.values) || parsed.values.length !== rows.length) return null;
  const next = { ...remembered };
  const filled = {};
  rows.forEach((r, i) => {
    const value = String(parsed.values[i] ?? "");
    filled[r.token] = value;
    if (r.options.length) {
      if (keepChoice) next[r.name] = value;
    } else if (useRemember) {
      if (parsed.remember?.[i]) next[r.name] = value;
      else delete next[r.name];
    }
  });
  saveRememberedVars(next, remembered);
  return filled;
}
