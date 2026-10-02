// Standalone quick-launcher, opened by the global hotkey. Fuzzy search over
// every prompt, Enter/click copies. Picking a result only ever tells the backend
// which id was chosen (launcher_select); the backend runs the exact same
// "copy from outside the grid" path the tray menu already uses (fire_tray_copy),
// so placeholder dialogs, cooldown, chains and recording stay in main.js, not
// duplicated here.
import { applyFloatTheme } from "./floattheme.js";
import { APP_NAME } from "./brand.js";
// Static, not dynamic: this window has to boot instantly (it opens on a hotkey
// press) and unlike floating.js's pill, it needs the translated placeholder/empty
// text from the very first frame, not after a lazy follow-up load.
import { FLOAT_TXT, LANGS } from "./i18n.js";
import { applyTerms } from "./terms.js";
import { resolveLang } from "./floatShared.js";

// Frameless window: the title is invisible in the app but shows in the Alt-Tab
// switcher, so it comes from the brand rather than a hardcoded English word.
document.title = APP_NAME;

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const cardEl = document.getElementById("card");
const searchEl = document.getElementById("search");
const resultsEl = document.getElementById("results");
const emptyEl = document.getElementById("empty");

let TXT = null;
let prompts = [];
let usage = {};
let lastUsed = {};
let active = -1; // keyboard-highlighted row index into the CURRENT rendered list
let closed = false;

// Rows listed at once: the searchListMax expert value, read in boot().
const MAX_RESULTS_DEFAULT = 8;
let maxResults = MAX_RESULTS_DEFAULT;
const PREVIEW_LEN = 70;

function close() {
  if (closed) return;
  closed = true;
  invoke("launcher_close").catch(() => {});
}

function select(id) {
  if (closed) return;
  closed = true;
  invoke("launcher_select", { id }).catch(() => {});
}

// Case-insensitive substring score: a name hit ranks far above a body hit, an
// earlier match ranks above a later one. No dependency, no fuzzy typo-tolerance:
// this is a quick filter, not the library's full search.
// p.lname/p.ltext are precomputed once in boot() -- re-lowercasing every
// item's name and (often much longer) body on every keystroke was the jank.
function score(p, q) {
  const ni = p.lname.indexOf(q);
  if (ni === 0) return 1000;
  if (ni > 0) return 500 - ni;
  const ti = p.ltext.indexOf(q);
  if (ti >= 0) return 100 - Math.min(ti, 90);
  return -1;
}

function defaultList() {
  return [...prompts]
    .sort((a, b) => (usage[b.id] || 0) - (usage[a.id] || 0) || (lastUsed[b.id] || 0) - (lastUsed[a.id] || 0))
    .slice(0, maxResults);
}

function matchList(q) {
  return prompts
    .map((p) => [p, score(p, q)])
    .filter(([, s]) => s >= 0)
    .sort((a, b) => b[1] - a[1])
    .slice(0, maxResults)
    .map(([p]) => p);
}

function renderRow(p, i) {
  const row = document.createElement("div");
  row.className = "launcher-row" + (i === active ? " active" : "");
  row.dataset.id = p.id;
  if (p.color) row.style.setProperty("--row-color", p.color);
  const name = document.createElement("span");
  name.className = "launcher-row-name";
  name.textContent = p.name || "";
  row.appendChild(name);
  const preview = (p.text || "").replace(/\s+/g, " ").trim();
  if (preview) {
    const prev = document.createElement("span");
    prev.className = "launcher-row-preview";
    prev.textContent = preview.length > PREVIEW_LEN ? preview.slice(0, PREVIEW_LEN) + "…" : preview;
    row.appendChild(prev);
  }
  row.addEventListener("mousedown", (e) => e.preventDefault()); // keep focus in the search field
  row.addEventListener("click", () => select(p.id));
  row.addEventListener("mouseenter", () => setActive(i));
  return row;
}

function render() {
  const q = searchEl.value.trim().toLowerCase();
  const list = q ? matchList(q) : defaultList();
  active = list.length ? 0 : -1;
  resultsEl.innerHTML = "";
  for (const [i, p] of list.entries()) resultsEl.appendChild(renderRow(p, i));
  emptyEl.classList.toggle("hidden", list.length > 0 || !q);
  if (!list.length && q) emptyEl.textContent = TXT?.noMatches || "";
  resultsEl.dataset.count = String(list.length);
}

function setActive(i) {
  const rows = [...resultsEl.children];
  if (i < 0 || i >= rows.length) return;
  rows[active]?.classList.remove("active");
  active = i;
  rows[active]?.classList.add("active");
  rows[active]?.scrollIntoView({ block: "nearest" });
}

searchEl.addEventListener("input", render);
document.addEventListener("keydown", (e) => {
  const rows = [...resultsEl.children];
  if (e.key === "Escape") { e.preventDefault(); close(); }
  else if (e.key === "ArrowDown") { e.preventDefault(); setActive(Math.min(active + 1, rows.length - 1)); }
  else if (e.key === "ArrowUp") { e.preventDefault(); setActive(Math.max(active - 1, 0)); }
  else if (e.key === "Enter") {
    e.preventDefault();
    const id = rows[active]?.dataset.id;
    if (id) select(id);
  }
});

// Same grace-period pattern as vars.js: the backend shows the window and only then
// takes focus, and a blur fired in that hand-off window would close before anything
// was ever on screen.
const OPENED_AT = Date.now();
const BLUR_GRACE_MS = 400;
window.addEventListener("blur", () => {
  const left = BLUR_GRACE_MS - (Date.now() - OPENED_AT);
  if (left <= 0) close();
  // A blur inside the hand-off window is deferred, not dropped: close once the
  // grace ends unless this window has the focus again by then.
  else setTimeout(() => { if (!document.hasFocus()) close(); }, left);
});
document.body.addEventListener("mousedown", (e) => {
  if (!cardEl.contains(e.target)) close();
});

// Text the tray filter row sends after this window was opened (the characters typed
// before it had focus): applied at once, or held until the first render has happened.
let bootDone = false;
let lateFill = null;
function applyPrefill(text) {
  searchEl.value = String(text || "").slice(0, 200);
  render();
  searchEl.setSelectionRange(searchEl.value.length, searchEl.value.length);
}

async function boot() {
  await listen("launcher-prefill", (e) => {
    if (bootDone) applyPrefill(e.payload);
    else lateFill = String(e.payload || "");
  });
  // A lean command instead of get_state: this window has to be ready the instant
  // its hotkey fires, and it needs four small fields, not a whole prompt each.
  const state = await invoke("launcher_items").catch(() => ({ prompts: [], language: "", usage: {}, last_used: {}, prefill: "" }));
  const cfg = await invoke("get_settings").catch(() => null);
  const wanted = Number(cfg?.ui_values?.searchListMax);
  maxResults = Number.isFinite(wanted) ? Math.min(20, Math.max(3, Math.round(wanted))) : MAX_RESULTS_DEFAULT;
  applyFloatTheme(await invoke("current_theme").catch(() => null), await invoke("theme_vars").catch(() => ({})));
  listen("theme-changed", (e) => applyFloatTheme(e.payload?.theme, e.payload?.vars));

  const lang = resolveLang(state.language, LANGS);
  TXT = Object.fromEntries(Object.entries(FLOAT_TXT[lang]).map(([k, v]) => [k, applyTerms(v, lang)]));
  searchEl.placeholder = TXT.searchPh;

  prompts = state.prompts || [];
  for (const p of prompts) { p.lname = (p.name || "").toLowerCase(); p.ltext = (p.text || "").toLowerCase(); }
  usage = state.usage || {};
  lastUsed = state.last_used || {};
  if (state.prefill) searchEl.value = state.prefill;
  if (lateFill !== null) searchEl.value = lateFill.slice(0, 200);
  render();
  searchEl.focus();
  searchEl.setSelectionRange(searchEl.value.length, searchEl.value.length);
  bootDone = true;
}
boot();
