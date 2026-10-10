// Custom Windows-11-style tray context menu, opened instead of the native
// popup when trayCustomMenu is on. Every entry runs the exact same activation
// the plain tray menu already used (tray_window_activate -> handle_tray_menu_id
// -> fire_tray_copy), so placeholder dialogs, cooldown and chains stay in
// the main window, never duplicated here. All text (labels, section titles, show/quit)
// comes pre-translated from the backend -- this file draws rows, nothing else.
import { applyFloatTheme } from "./floattheme.js";
import { APP_NAME } from "./brand.js";
import { hexToRgb } from "./floatShared.js";

// Frameless window: the title is invisible in the app but shows in the Alt-Tab
// switcher, so it comes from the brand rather than a hardcoded English word.
document.title = APP_NAME;

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const cardEl = document.getElementById("card");
const rowsEl = document.getElementById("rows");
const searchEl = document.getElementById("tray-search");

let rows = []; // flattened, in visual order: { id, el }
let active = -1;
let closed = true; // hidden until the first open

function close() {
  if (closed) return;
  closed = true;
  invoke("tray_window_close").catch(() => {});
}

function activate(id) {
  if (closed) return;
  closed = true;
  invoke("tray_window_activate", { id }).catch(() => {});
}

// Same four kinds, same fallback tint as the native menu's tray_kind_tint.
const KIND_TINT = { text: [90, 100, 114], image: [47, 111, 237], media: [139, 92, 246], file: [224, 138, 30] };

function luma([r, g, b]) {
  return (r * 299 + g * 587 + b * 114) / 1000;
}

// Same four glyphs as the native menu's hand-drawn 16x16 icon chips (tray_icon_pixels
// in tray.rs), redrawn as vectors: a colored chip -- the prompt's own tile color, or
// the kind's tint -- with a glyph on top, ink flipped light/dark by the chip's own
// brightness exactly like the native icon does. Each glyph is a plain list of SVG
// shape descriptors (never markup) built with createElementNS, so a prompt's own
// color string never runs through innerHTML.
const KIND_GLYPH = {
  text: [
    { tag: "path", d: "M4 5h10M4 8.2h10M4 11.4h7", strokeWidth: "1.6", strokeLinecap: "round" },
  ],
  image: [
    { tag: "rect", x: "3", y: "3.5", width: "12", height: "10", rx: "1.3", strokeWidth: "1.4", fill: "none" },
    { tag: "circle", cx: "6.3", cy: "7", r: "1.05" },
    {
      tag: "path",
      d: "M3.8 12.2l3.3-3.5 2.5 2.4 2-2.1 3.1 3.2",
      strokeWidth: "1.4",
      fill: "none",
      strokeLinecap: "round",
      strokeLinejoin: "round",
    },
  ],
  media: [{ tag: "path", d: "M5.5 4v10l8-5-8-5z" }],
  file: [
    { tag: "path", d: "M5 2.5h5l3 3v10H5z", fill: "none", strokeWidth: "1.4", strokeLinejoin: "round" },
    { tag: "path", d: "M10 2.5v3h3", fill: "none", strokeWidth: "1.4", strokeLinejoin: "round" },
    { tag: "path", d: "M7 9.5h4M7 12h4", strokeWidth: "1.3", strokeLinecap: "round" },
  ],
};

const SVG_NS = "http://www.w3.org/2000/svg";
const ATTR_NAME = { strokeWidth: "stroke-width", strokeLinecap: "stroke-linecap", strokeLinejoin: "stroke-linejoin" };

function iconEl(kind, color) {
  const fill = hexToRgb(color) || KIND_TINT[kind] || KIND_TINT.text;
  const ink = luma(fill) < 150 ? "#fff" : "#181a1f";
  const chip = document.createElement("span");
  chip.className = "tray-menu-icon";
  chip.style.background = `rgb(${fill[0]}, ${fill[1]}, ${fill[2]})`;
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 18 18");
  svg.setAttribute("width", "14");
  svg.setAttribute("height", "14");
  svg.setAttribute("fill", ink);
  svg.setAttribute("stroke", ink);
  for (const shape of KIND_GLYPH[kind] || KIND_GLYPH.text) {
    const el = document.createElementNS(SVG_NS, shape.tag);
    for (const [key, val] of Object.entries(shape)) {
      if (key === "tag") continue;
      el.setAttribute(ATTR_NAME[key] || key, val);
    }
    svg.appendChild(el);
  }
  chip.appendChild(svg);
  return chip;
}

function setActive(i) {
  rows[active]?.el.classList.remove("active");
  active = i;
  rows[active]?.el.classList.add("active");
  rows[active]?.el.scrollIntoView({ block: "nearest" });
}

function addRow(id, label, icon) {
  const row = document.createElement("div");
  row.className = "tray-menu-row";
  row.dataset.id = id;
  if (icon) row.appendChild(icon);
  const name = document.createElement("span");
  name.className = "tray-menu-row-name";
  name.textContent = label;
  row.appendChild(name);
  // Keep focus (and the keyboard highlight) inside the window on a mouse click.
  row.addEventListener("mousedown", (e) => e.preventDefault());
  row.addEventListener("click", () => activate(id));
  row.addEventListener("mouseenter", () => setActive(rows.findIndex((r) => r.el === row)));
  rowsEl.appendChild(row);
  rows.push({ id, el: row });
}

function addHeader(text) {
  if (!text) return; // the search-result section has no title, so it draws no bar
  const head = document.createElement("div");
  head.className = "tray-menu-head";
  head.textContent = text;
  rowsEl.appendChild(head);
}

function addSeparator() {
  rowsEl.appendChild(document.createElement("div")).className = "tray-menu-sep";
}

// Draws the rows below the (always-present, never-cleared) search field: the
// recording controls, then either the fixed favourites/recents sections or the
// search-result list the caller passes in, then show/quit. Re-running this on
// every keystroke is cheap -- at most a couple dozen rows.
function renderRows(recEntries, sections, showLabel, quitLabel) {
  rowsEl.innerHTML = "";
  rows = [];
  for (const entry of recEntries || []) addRow(entry.id, entry.label, null);
  if ((recEntries || []).length) addSeparator();
  for (const section of sections || []) {
    const entries = section.entries || [];
    if (!entries.length) continue;
    addHeader(section.title);
    for (const entry of entries) addRow(entry.id, entry.label, iconEl(entry.kind, entry.color));
    addSeparator();
  }
  addRow("show", showLabel || "", null);
  addRow("quit", quitLabel || "", null);
  setActive(rows.length ? 0 : -1);
}

function render(plan) {
  renderRows(plan.recEntries, plan.sections, plan.showLabel, plan.quitLabel);
}

// The first keystroke in the filter field hands over to the quick launcher: the
// backend builds it with what was typed already in its search box. This window keeps
// the focus, and every further key lands in this field, until the launcher page is up
// and the backend gives it the focus: that is the moment this window blurs, so the
// field then holds exactly what was typed up to the switch. It is passed on once,
// complete, and this window closes. Keys typed after the switch go to the launcher
// itself, which puts the passed-on text in front of them. A launcher that never takes
// the focus gets the text after HANDOFF_MAX_MS anyway. Arrow keys and Enter on the
// fixed rows work before typing.
const HANDOFF_MAX_MS = 3000;
let handover = null; // set once the first keystroke started the hand-off
let tailSent = false;
let tailTimer = 0;
function sendTail() {
  if (tailSent || !handover) return;
  tailSent = true;
  clearTimeout(tailTimer);
  invoke("tray_launcher_prefill", { query: searchEl.value }).catch(() => {}).then(close);
}
function onSearchInput() {
  if (handover) return;
  handover = invoke("tray_open_launcher", { query: searchEl.value }).catch(() => {});
  tailTimer = setTimeout(sendTail, HANDOFF_MAX_MS);
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    e.preventDefault();
    close();
  } else if (e.key === "ArrowDown") {
    e.preventDefault();
    setActive(Math.min(active + 1, rows.length - 1));
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    setActive(Math.max(active - 1, 0));
  } else if (e.key === "Enter") {
    e.preventDefault();
    const id = rows[active]?.id;
    if (id) activate(id);
  }
});

// Same grace-period pattern as launcher.js/vars.js: the backend shows the window
// and only then takes focus, and a blur fired in that hand-off window would
// close before anything was ever on screen.
let openedAt = Date.now();
const BLUR_GRACE_MS = 400;
window.addEventListener("blur", () => {
  if (handover) { sendTail(); return; } // the launcher took the focus: pass on the text, then close
  if (closed) return; // hidden between opens: nothing to close
  const left = BLUR_GRACE_MS - (Date.now() - openedAt);
  if (left <= 0) close();
  // A blur landing inside the hand-off window itself (the backend showing the
  // window, then taking focus) is not real activity to ignore forever -- if the
  // user genuinely clicks another app in that same instant, discarding the blur
  // outright left this always-on-top menu stranded on screen with nothing left
  // to close it. Defer instead: close once the grace window actually ends,
  // unless this window is focused again before then.
  else setTimeout(() => { if (!document.hasFocus() && !handover) close(); }, left);
});
document.body.addEventListener("mousedown", (e) => {
  if (!cardEl.contains(e.target) && !handover) close();
});

// The window lives on between opens (hidden), so every open starts from a clean
// slate: the backend parks the plan, says "tray-open", and this page takes it, draws
// it and only then asks to be shown. No plan waiting (the warm-up boot) = nothing.
async function openMenu() {
  const plan = await invoke("tray_window_plan").catch(() => null);
  if (!plan) return;
  clearTimeout(tailTimer);
  handover = null;
  tailSent = false;
  closed = false;
  searchEl.value = "";
  render(plan);
  searchEl.placeholder = plan.searchPh || "";
  searchEl.hidden = !plan.searchOn;
  openedAt = Date.now();
  await invoke("tray_window_reveal").catch(() => {});
  if (plan.searchOn) searchEl.focus();
}

async function boot() {
  applyFloatTheme(await invoke("current_theme").catch(() => null), await invoke("theme_vars").catch(() => ({})));
  listen("theme-changed", (e) => applyFloatTheme(e.payload?.theme, e.payload?.vars));
  searchEl.addEventListener("input", onSearchInput);
  await listen("tray-open", openMenu);
  openMenu(); // an open that arrived while this page was still loading
}

boot();
