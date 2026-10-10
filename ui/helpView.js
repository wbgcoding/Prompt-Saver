// The help view: search, tabs, collapsed sections and deep links.
import { HELP_CONTENT } from "./help.js";
import { $, ALL_SHORTCUT_SCOPES, debounced, LANG, t, txt } from "./core.js";
import { HELP_SKETCHES } from "./helpSketches.js";
import { openShortcutsModal } from "./windowChrome.js";

// In-app documentation. EN + DE only (ui/help.js): every other language
// falls back to English rather than showing untranslated placeholder text.
export let helpQuery = "";
export function assignHelpQuery(v) { helpQuery = v; }
export let helpTab = 0;
export let helpCollapsed = new Set(); // in-memory only; resets each time the dialog opens
export function assignHelpCollapsed(v) { helpCollapsed = v; }

export function helpCategories() {
  return HELP_CONTENT[LANG] || HELP_CONTENT.en;
}

// Section ids are stable keys (see ui/help.js), never translated text: the
// label comes from a helpSec-prefixed key (via t) so it follows the app's own
// 11 languages even though the article content itself stays EN/DE-only.
export function helpSectionLabel(id) {
  return t(`helpSec${id.charAt(0).toUpperCase()}${id.slice(1)}`);
}

// Strips case and diacritics so "ordner"/"Übersicht" match "Ordner"/"ubersicht" alike.
export const helpNorm = (s) => (s || "").toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "");

// Appends text nodes + <mark> spans around every term match, without clearing
// the element first (appendHelpText below calls this between icon swaps). Never
// innerHTML: the search query is user input and must not be parsed as markup.
export function appendTermMarks(el, text, terms) {
  if (!text) return;
  const norm = helpNorm(text);
  const ranges = [];
  for (const term of terms) {
    if (!term) continue;
    let from = 0;
    for (let idx; (idx = norm.indexOf(term, from)) !== -1; from = idx + term.length) {
      ranges.push([idx, idx + term.length]);
    }
  }
  if (!ranges.length) { el.appendChild(document.createTextNode(text)); return; }
  ranges.sort((a, b) => a[0] - b[0]);
  const merged = [ranges[0]];
  for (const r of ranges.slice(1)) {
    const last = merged[merged.length - 1];
    if (r[0] <= last[1]) last[1] = Math.max(last[1], r[1]);
    else merged.push(r);
  }
  let pos = 0;
  for (const [start, end] of merged) {
    if (start > pos) el.appendChild(document.createTextNode(text.slice(pos, start)));
    const mark = document.createElement("mark");
    mark.className = "help-mark";
    mark.textContent = text.slice(start, end);
    el.appendChild(mark);
    pos = end;
  }
  if (pos < text.length) el.appendChild(document.createTextNode(text.slice(pos)));
}

// Where each `{icon:key}` placeholder in ui/help.js gets its picture from: the
// real button's own SVG element, read live, never a copy of its path data. A
// help sentence naming a button this way always shows the button as it looks
// today, including a future icon redesign, with zero upkeep in help.js itself.
export const HELP_ICON_SOURCES = {
  screenshot: () => $("snip-btn")?.querySelector("svg")?.outerHTML,
  record: () => $("record-btn")?.querySelector("svg")?.outerHTML,
  ocr: () => $("ocr-btn")?.querySelector("svg")?.outerHTML,
  mic: () => $("voice-btn")?.querySelector("svg")?.outerHTML,
  library: () => $("library-btn")?.querySelector("svg")?.outerHTML,
  themeEditor: () => $("theme-edit-open")?.querySelector("svg")?.outerHTML,
};

// Builds the inline icon span for one `{icon:key}` placeholder, or null when the
// key is unknown or its button is missing, so the caller then just drops the
// placeholder rather than showing broken markup.
export function helpIconSpan(key) {
  const getSvg = HELP_ICON_SOURCES[key];
  const svg = getSvg && getSvg();
  if (!svg) return null;
  const span = document.createElement("span");
  span.className = "help-icon";
  span.innerHTML = svg;
  const inner = span.firstElementChild;
  if (inner) inner.setAttribute("aria-hidden", "true");
  return span;
}

// Splits text on `{icon:key}` placeholders, swapping each for the real button's
// icon, and marks search terms in the text around them via appendTermMarks.
export function appendHelpText(el, text, terms) {
  const re = /\{icon:([a-zA-Z][a-zA-Z0-9]*)\}/g;
  let last = 0;
  let m;
  while ((m = re.exec(text))) {
    appendTermMarks(el, text.slice(last, m.index), terms);
    const icon = helpIconSpan(m[1]);
    if (icon) el.appendChild(icon);
    last = re.lastIndex;
  }
  appendTermMarks(el, text.slice(last), terms);
}

// The one place help text (title, overview, steps, outcome, settings, tip) is
// written into the DOM: clears the element, then renders icons plus, only when
// asked, the search-term highlight markTerms used to do alone.
export function setHelpText(el, text, terms, highlight) {
  el.textContent = "";
  appendHelpText(el, text, highlight ? terms : []);
}

export function renderHelp() {
  const box = $("help-body");
  const tabBar = $("help-tabs-bar");
  box.innerHTML = "";
  tabBar.innerHTML = "";
  const terms = helpNorm(helpQuery).trim().split(/\s+/).filter(Boolean);
  const searching = terms.length > 0;
  const cats = helpCategories();
  const sections = [...new Set(cats.map((c) => c.section))];
  if (helpTab >= sections.length) helpTab = 0;
  // Tab bar lives in the fixed header (stays visible while the rows scroll); it is
  // hidden while searching across every section, same pattern as the expert menu.
  if (!searching && sections.length > 1) {
    const bar = document.createElement("div");
    bar.className = "expert-tabs";
    sections.forEach((id, i) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "expert-tab" + (i === helpTab ? " active" : "");
      b.textContent = helpSectionLabel(id);
      b.addEventListener("click", () => { helpTab = i; box.scrollTop = 0; renderHelp(); });
      bar.appendChild(b);
    });
    tabBar.appendChild(bar);
  }
  const activeSection = searching ? null : sections[helpTab];
  // Fields carry a search weight (title matters far more than a settings line); a
  // term must appear in SOME field for the article to count as a hit at all.
  let list;
  if (searching) {
    const wholeQuery = helpNorm(helpQuery).trim();
    const scored = [];
    for (const cat of cats) {
      const fields = [
        [cat.title, 10],
        [cat.whatItIs, 4],
        [(cat.howTo || []).join(" "), 3],
        [cat.whatHappens || "", 2],
        [cat.settings.join(" "), 2],
        [cat.tip || "", 1],
      ];
      let score = 0;
      let allTermsHit = true;
      for (const term of terms) {
        let termHit = false;
        for (const [text, weight] of fields) {
          if (helpNorm(text).includes(term)) { score += weight; termHit = true; }
        }
        if (!termHit) { allTermsHit = false; break; }
      }
      if (!allTermsHit) continue;
      if (helpNorm(cat.title).includes(wholeQuery)) score += 20;
      scored.push({ cat, score });
    }
    scored.sort((a, b) => b.score - a.score);
    list = scored.map((s) => s.cat);
  } else {
    list = cats.filter((cat) => cat.section === activeSection);
  }
  const shown = list.length;
  if (searching) {
    const hits = document.createElement("div");
    hits.className = "hint help-hits";
    hits.textContent = t("helpHits").replace("{n}", String(shown));
    box.appendChild(hits);
  }
  for (const cat of list) {
    // Reuses the expert menu's own group/collapse styling (arrow, spacing, hover):
    // same visual language, zero new CSS for the mechanic itself.
    const collapsedNow = searching ? shown > 1 : helpCollapsed.has(cat.title);
    const sec = document.createElement("div");
    sec.className = "expert-group help-group" + (collapsedNow ? " collapsed" : "");
    const head = document.createElement("div");
    head.className = "expert-group-title collapsible";
    const titleSpan = document.createElement("span");
    titleSpan.className = "help-group-title-text";
    setHelpText(titleSpan, cat.title, terms, searching);
    head.appendChild(titleSpan);
    if (searching) {
      const badge = document.createElement("span");
      badge.className = "help-sec-badge";
      badge.textContent = helpSectionLabel(cat.section);
      head.appendChild(badge);
    }
    head.addEventListener("click", () => {
      if (helpCollapsed.has(cat.title)) helpCollapsed.delete(cat.title);
      else helpCollapsed.add(cat.title);
      sec.classList.toggle("collapsed");
    });
    sec.appendChild(head);
    const body = document.createElement("div");
    body.className = "help-group-body";
    // 22-A: four parts per entry; what it is, how to do it, what happens, and the
    // settings that tune it. howTo/whatHappens are optional so a short standalone
    // entry (e.g. the quick launcher) doesn't have to force a step list.
    const ov = document.createElement("p");
    ov.className = "help-overview";
    setHelpText(ov, cat.whatItIs, terms, searching);
    body.appendChild(ov);
    if (cat.howTo && cat.howTo.length) {
      const ol = document.createElement("ol");
      ol.className = "help-steps";
      for (const step of cat.howTo) {
        const li = document.createElement("li");
        setHelpText(li, step, terms, searching);
        ol.appendChild(li);
      }
      body.appendChild(ol);
    }
    if (cat.sketches && cat.sketches.length) {
      // Sketches only ship English/German labels, matching help.js's own EN/DE
      // scope -- every other language falls back to the English drawing.
      const sketchLang = LANG === "de" ? "de" : "en";
      for (const id of cat.sketches) {
        const draw = HELP_SKETCHES[id];
        if (!draw) continue;
        const fig = document.createElement("div");
        fig.className = "help-sketch";
        fig.innerHTML = draw(sketchLang);
        body.appendChild(fig);
      }
    }
    if (cat.whatHappens) {
      const wh = document.createElement("p");
      wh.className = "help-happens";
      setHelpText(wh, cat.whatHappens, [], false);
      body.appendChild(wh);
    }
    if (cat.settings.length) {
      const ul = document.createElement("ul");
      ul.className = "help-settings";
      for (const s of cat.settings) {
        const li = document.createElement("li");
        setHelpText(li, s, [], false);
        ul.appendChild(li);
      }
      body.appendChild(ul);
    }
    if (cat.tip) {
      const tip = document.createElement("p");
      tip.className = "help-tip";
      const ico = document.createElement("span");
      ico.className = "help-tip-ico";
      ico.setAttribute("aria-hidden", "true");
      ico.innerHTML = '<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M8 1a5 5 0 0 0-3 9v1.5a1 1 0 0 0 1 1h4a1 1 0 0 0 1-1V10a5 5 0 0 0-3-9zm-1.5 13h3a.5.5 0 0 1 0 1h-3a.5.5 0 0 1 0-1z"/></svg>';
      const txt = document.createElement("span");
      setHelpText(txt, cat.tip, [], false);
      tip.appendChild(ico);
      tip.appendChild(txt);
      body.appendChild(tip);
    }
    sec.appendChild(body);
    box.appendChild(sec);
  }
  if (searching && !shown) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t("helpNoResults");
    box.appendChild(empty);
  }
}

export function openHelp() {
  helpQuery = "";
  $("help-search").value = "";
  helpCollapsed = new Set();
  renderHelp();
  $("help").classList.remove("hidden");
}

// Opens the help dialog pre-filtered to one article by its stable `id` (see
// ui/help.js), through the same search field gsOpenHelp uses for a global-search
// hit -- an anchor is a lookup key, never the (per-language) title text itself.
export function openHelpAt(id) {
  const cat = helpCategories().find((c) => c.id === id);
  helpQuery = cat ? cat.title : "";
  $("help-search").value = helpQuery;
  helpCollapsed = new Set();
  renderHelp();
  $("help").classList.remove("hidden");
}

export function bindHelpView() {
  // In-app documentation.
  const helpEl = $("help");
  $("help-open").addEventListener("click", openHelp);
  $("help-close").addEventListener("click", () => helpEl.classList.add("hidden"));
  $("help-shortcuts").addEventListener("click", () => openShortcutsModal(ALL_SHORTCUT_SCOPES, { withGlobal: true, allWindows: true }));
  helpEl.addEventListener("pointerdown", (e) => {
    if (e.target === helpEl) helpEl.classList.add("hidden");
  });
  const renderHelpDebounced = debounced(renderHelp);
  $("help-search").addEventListener("input", (e) => { helpQuery = e.target.value; renderHelpDebounced(); });
  return { helpEl };
}
