// Window chrome: language apply, bar toggles, the tile tooltip engine, clear buttons,
// the lightbox and the shortcuts modal.
import { $, ALL_SHORTCUT_SCOPES, applyBrand, CLEAR_SVG, drag, flag, invoke, LANG, mediaBarLabels, settings, shortcutRows, shortcutSections, t, val, videoCache, withShortcut } from "./core.js";
import { uiZoom, updateExamplesBanner } from "./views.js";
import { fitAllTiles } from "./tiles.js";
import { fillVarHighlighted } from "./promptVars.js";
import { placePopover } from "./popover.js";
import { SCOPE_TITLE_KEYS } from "./shortcuts.js";

let renderHotkeysList; // linked from main.js
export function linkWindowChrome(fns) { ({ renderHotkeysList } = fns); }

export function applyI18n() {
  document.documentElement.lang = LANG;
  applyBrand();
  document.querySelectorAll("[data-i18n]").forEach((el) => { el.textContent = t(el.dataset.i18n); });
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => { el.placeholder = t(el.dataset.i18nPh); });
  // A shortcut-bearing element gets its tooltip built from SHORTCUTS (base
  // description + key combo) instead of the plain translation.
  document.querySelectorAll("[data-i18n-title]").forEach((el) => {
    if (el.dataset.shortcut) withShortcut(el, el.dataset.shortcut);
    else el.title = t(el.dataset.i18nTitle);
  });
  document.querySelectorAll("[data-i18n-aria]").forEach((el) => { el.setAttribute("aria-label", t(el.dataset.i18nAria)); });
  // The maximize button's title flips between maximize and restore with the window state.
  document.querySelectorAll("[data-status-key]").forEach((el) => {
    el.textContent = t(el.dataset.statusKey).replace("{" + el.dataset.statusToken + "}", el.dataset.statusValue);
  });
  const winMax = document.getElementById("win-max");
  if (winMax) winMax.title = t(document.body.classList.contains("win-maximized") ? "winRestore" : "winMaximize");
  // Cached video tiles keep their media bar across a language switch: its
  // button labels are not markup, so data-i18n above never reaches them. Walk
  // videoCache itself, not just the live DOM: a tile from a view that is not
  // the active one is cached but detached, so querySelectorAll would miss it.
  for (const { wrap } of videoCache.values()) {
    wrap.querySelector(".media-bar")?.setLabels?.(mediaBarLabels());
  }
  updateExamplesBanner(); // its label carries a live count, not a plain data-i18n string
  applyBarToggleTips();
}

// ---- Collapsible bars (header / composer) ----
export function applyBars() {
  const header = settings.show_header !== false;
  const composer = settings.show_composer !== false;
  document.body.classList.toggle("no-header", !header);
  document.body.classList.toggle("no-composer", !composer);
  $("show-top").classList.toggle("hidden", header);
  $("show-bottom").classList.toggle("hidden", composer);
  // The grid area changed size: re-fit the tile text.
  requestAnimationFrame(() => fitAllTiles());
}

export function setBars(header, composer) {
  settings.show_header = header;
  settings.show_composer = composer;
  invoke("set_bars", { header, composer }).catch(() => {});
  applyBars();
}

// Mentions the double-click shortcut in the four bar buttons' own tooltip
// while barToggleBoth is on, reusing the expert-menu row's own explanation
// text instead of a second translated sentence.
export function applyBarToggleTips() {
  const hint = flag("barToggleBoth") ? ` ${t("tipBarToggleBoth")}` : "";
  for (const id of ["hide-top", "hide-bottom"]) $(id).title = t("hideBar") + hint;
  for (const id of ["show-top", "show-bottom"]) $(id).title = t("showBar") + hint;
}

// ---- Themed tooltips: ONE shared popup for every hover hint; rich tile previews
// AND the native `title` on any button/icon: so nothing falls back to the OS
// tooltip. A raw `title` is converted to `data-tip` on first hover (removed so the
// OS tip never fires); tiles carry `data-tip-name`/`data-tip-body` for the richer
// name + preview + hint layout.
export let tipEl = null;
export let tipCur = null;   // element the tooltip is currently anchored to
export let tipTimer = null;   // auto-hide (AFK) timer once a tip is shown
export const TOOLTIP_HIDE_MS = 15000; // how long a shown tooltip stays before it hides itself
export let tipShowTimer = null; // hover show-delay timer before a tip appears
export let tipPending = null;   // element whose tip is scheduled but not yet shown
export let tipExpired = null; // element whose tooltip timed out; suppressed until another is hovered
export let tipPtrX = 0, tipPtrY = 0; // last pointer position (a delayed tip places here)
export function clearTipShow() {
  if (tipShowTimer) { clearTimeout(tipShowTimer); tipShowTimer = null; }
  tipPending = null;
}
export function hideTileTip() {
  tipCur = null;
  clearTipShow();
  if (tipTimer) { clearTimeout(tipTimer); tipTimer = null; }
  tipEl?.classList.remove("show");
}
// Reveal toggle for password fields (backup settings + the restore prompt).
export function wireClear(el) {
  if (!el || el.dataset.clearWired || el.readOnly) return;
  const isTa = el.tagName === "TEXTAREA";
  if (!isTa) {
    const type = (el.getAttribute("type") || "text").toLowerCase();
    if (!["text", "search", "number", ""].includes(type)) return;
    if (el.classList.contains("grid-mini")) return; // too tiny for an X
  }
  el.dataset.clearWired = "1";
  const wrap = document.createElement("span");
  wrap.className = "clear-wrap" + (isTa ? " clear-wrap-ta" : "");
  el.parentNode.insertBefore(wrap, el);
  wrap.appendChild(el);
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "clear-btn";
  btn.tabIndex = -1;
  btn.dataset.i18nAria = "clearField"; // applyI18n keeps it current across a language switch
  btn.setAttribute("aria-label", t("clearField"));
  btn.innerHTML = CLEAR_SVG;
  wrap.appendChild(btn);
  const upd = () => btn.classList.toggle("show", !!el.value);
  el.addEventListener("input", upd);
  btn.addEventListener("mousedown", (e) => e.preventDefault()); // keep focus
  btn.addEventListener("click", () => {
    el.value = "";
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.focus();
    upd();
  });
  upd();
}
// Wire every text/number input + textarea (idempotent): call again after any render
// that creates new fields (expert menu, dialogs, backups panel).
export function wireClearButtons(root = document) {
  const sel = "input:not([type=checkbox]):not([type=radio]):not([type=color]):not([type=range]), textarea";
  for (const el of root.querySelectorAll(sel)) wireClear(el);
}

export function wireTooltips() {
  tipEl = document.createElement("div");
  tipEl.className = "tile-tip";
  tipEl.setAttribute("role", "tooltip");
  tipEl.innerHTML = '<div class="tile-tip-name"></div><div class="tile-tip-body"></div><div class="tile-tip-len"></div><div class="tile-tip-times"></div><div class="tile-tip-media"></div><div class="tile-tip-notes"></div><div class="tile-tip-rotate"></div><div class="tile-tip-hint"></div>';
  document.body.appendChild(tipEl);
  const nameEl = tipEl.querySelector(".tile-tip-name");
  const bodyEl = tipEl.querySelector(".tile-tip-body");
  const lenEl = tipEl.querySelector(".tile-tip-len");
  const timesEl = tipEl.querySelector(".tile-tip-times");
  const mediaEl = tipEl.querySelector(".tile-tip-media");
  const notesEl = tipEl.querySelector(".tile-tip-notes");
  const rotateEl = tipEl.querySelector(".tile-tip-rotate");
  const hintEl = tipEl.querySelector(".tile-tip-hint");
  const OFF = 16, PAD = 12;
  const placeAt = (cx, cy) => {
    const r = tipEl.getBoundingClientRect();
    let x = cx + OFF;
    let y = cy + OFF;
    if (x + r.width + PAD > innerWidth) x = cx - OFF - r.width;
    if (y + r.height + PAD > innerHeight) y = cy - OFF - r.height;
    const z = uiZoom(); // fixed popup lives inside the zoomed body
    // The flip above only guards the edge it checked: near a corner, with a
    // wide tooltip (expert tooltipWidth), the flipped side can itself run off the
    // FAR edge. Clamp both bounds, same as placePopover does for every other popup.
    tipEl.style.left = `${Math.max(PAD, Math.min(x, innerWidth - r.width - PAD)) / z}px`;
    tipEl.style.top = `${Math.max(PAD, Math.min(y, innerHeight - r.height - PAD)) / z}px`;
  };
  // Actually reveal the tip for `el` (called immediately or after the show-delay).
  const showNow = (el) => {
    const rich = !!el.dataset.tipName;
    const name = el.dataset.tipName || el.dataset.tip || "";
    if (!name) return;
    tipExpired = null;
    tipCur = el;
    // Plain (icon/label) tooltips get a compact single-line style; tiles get the
    // richer name + preview + hint box.
    tipEl.classList.toggle("plain", !rich);
    // Read regardless of rich/plain -- a library row's plain "Copy" tooltip
    // carries these same two lines, just without the rest of the rich box.
    timesEl.textContent = el.dataset.tipTimes || "";
    mediaEl.textContent = el.dataset.tipMedia || "";
    if (rich) {
      nameEl.textContent = name;
      fillVarHighlighted(bodyEl, el.dataset.tipBody || "");
      lenEl.textContent = el.dataset.tipLen || "";
      // textContent, so a note is shown as the literal text the user typed.
      notesEl.textContent = el.dataset.tipNotes || "";
      rotateEl.textContent = el.dataset.tipRotate || "";
      hintEl.textContent = t("tileTooltip") + (flag("autoPaste") ? " · " + t("autoPasteHint") : "");
    } else {
      // Plain setting/icon tip: break "<lead sentence>. <details>" into two tidy
      // paragraphs (e.g. the "Expert menu: …" clause drops onto its own line).
      const sp = name.indexOf(". ");
      if (sp > 0 && sp < name.length - 2) {
        nameEl.textContent = name.slice(0, sp + 1);
        bodyEl.textContent = name.slice(sp + 2);
      } else {
        nameEl.textContent = name;
        bodyEl.textContent = "";
      }
      lenEl.textContent = "";
      notesEl.textContent = "";
      // The rotate line is shared with the rich tile tip: leaving it filled here
      // stuck the last hovered tile's "next entry" text under every plain tooltip.
      rotateEl.textContent = "";
      hintEl.textContent = "";
    }
    placeAt(tipPtrX, tipPtrY);
    tipEl.classList.add("show");
    // AFK cleanness: auto-hide after the timeout, then don't reshow on this same
    // element until the pointer visits a different one.
    tipTimer = setTimeout(() => { tipExpired = el; hideTileTip(); }, TOOLTIP_HIDE_MS);
  };
  document.addEventListener("pointerover", (e) => {
    if (drag) return; // no hover hints mid-drag
    if (!flag("tooltipsEnabled")) { if (tipCur || tipPending) hideTileTip(); return; } // master switch off
    const el = e.target.closest?.("[data-tip-name], [data-tip], [title]");
    // Minimize/maximize/close never get a hint: they are universally understood
    // and a tooltip right under the cursor there is only in the way.
    if (!el || el.closest(".win-controls")) { if (tipCur) hideTileTip(); clearTipShow(); return; }
    if (el.hasAttribute("title")) { el.dataset.tip = el.getAttribute("title"); el.removeAttribute("title"); }
    const name = el.dataset.tipName || el.dataset.tip || "";
    if (!name) { hideTileTip(); return; }
    if (el === tipCur) return;     // already showing on this element
    if (el === tipPending) return; // already scheduled for this element
    if (el === tipExpired) return; // timed out here: hover elsewhere to reset
    // New target: drop any pending/showing tip, then (re)arm the show-delay so
    // casually sweeping the pointer across the window shows nothing.
    clearTipShow();
    if (tipCur && tipCur !== el) hideTileTip();
    tipPtrX = e.clientX; tipPtrY = e.clientY;
    const delay = val("tooltipDelayMs");
    if (delay <= 0) { showNow(el); return; }
    tipPending = el;
    tipShowTimer = setTimeout(() => {
      tipShowTimer = null;
      if (tipPending === el) { tipPending = null; showNow(el); }
    }, delay);
  });
  document.addEventListener("pointermove", (e) => {
    tipPtrX = e.clientX; tipPtrY = e.clientY;
    if (tipCur) placeAt(e.clientX, e.clientY);
  });
  document.addEventListener("pointerout", (e) => {
    if (tipPending && !tipPending.contains(e.relatedTarget)) clearTipShow();
    if (tipCur && !tipCur.contains(e.relatedTarget)) hideTileTip();
  });
  document.addEventListener("pointerdown", hideTileTip); // out of the way while clicking/dragging
}

// ---- Custom window controls (the native OS titlebar is disabled in
// tauri.conf.json, so minimize / maximize / close live in the header). close()
// fires the same CloseRequested event the native X did, so tray behaviour is kept.
export async function wireWindowControls() {
  const win = window.__TAURI__?.window?.getCurrentWindow?.();
  if (!win) return;
  const syncMax = async () => {
    try {
      const max = await win.isMaximized();
      document.body.classList.toggle("win-maximized", max);
      const b = $("win-max");
      if (b) { b.title = t(max ? "winRestore" : "winMaximize"); delete b.dataset.tip; }
    } catch (_) {}
  };
  $("win-min")?.addEventListener("click", () => { win.minimize().catch(() => {}); });
  $("win-max")?.addEventListener("click", async () => {
    try { await win.toggleMaximize(); } catch (_) {}
    syncMax();
  });
  $("win-close")?.addEventListener("click", () => { win.close().catch(() => {}); });
  try { await win.onResized(syncMax); } catch (_) {}
  syncMax();
}

// Themed recent-searches dropdown for a search field, replacing the browser's
// unstyled native autofill. Terms live in ui_texts (DPAPI-encrypted like the rest),
// most-recent first, deduped, capped. Picking one re-runs the field's own search.
export function wireSearchSuggest(input, storeKey) {
  if (!input) return;
  input.setAttribute("autocomplete", "off");
  const pop = document.createElement("div");
  pop.className = "search-suggest hidden";
  document.body.appendChild(pop);
  const load = () => {
    try { const a = JSON.parse(settings.ui_texts?.[storeKey] || "[]"); return Array.isArray(a) ? a : []; }
    catch { return []; }
  };
  const remember = (term) => {
    term = term.trim();
    if (term.length < 2 || !flag("searchSuggest")) return;
    const list = load().filter((x) => x.toLowerCase() !== term.toLowerCase());
    list.unshift(term);
    const value = JSON.stringify(list.slice(0, val("searchListMax")));
    settings.ui_texts = { ...(settings.ui_texts || {}), [storeKey]: value };
    invoke("set_ui_text", { key: storeKey, value }).catch(() => {});
  };
  const forget = (term) => {
    const value = JSON.stringify(load().filter((x) => x.toLowerCase() !== term.toLowerCase()));
    settings.ui_texts = { ...(settings.ui_texts || {}), [storeKey]: value };
    invoke("set_ui_text", { key: storeKey, value }).catch(() => {});
  };
  const hide = () => pop.classList.add("hidden");
  const show = () => {
    if (!flag("searchSuggest")) { hide(); return; }
    const q = input.value.trim().toLowerCase();
    const items = load().filter((x) => !q || x.toLowerCase().includes(q)).slice(0, val("searchListMax"));
    pop.innerHTML = "";
    if (!items.length) { hide(); return; }
    for (const term of items) {
      const row = document.createElement("div");
      row.className = "search-suggest-item";
      const label = document.createElement("span");
      label.className = "search-suggest-text";
      label.textContent = term;
      label.addEventListener("mousedown", (e) => {
        e.preventDefault(); // keep focus so blur-hide doesn't beat the click
        input.value = term;
        input.dispatchEvent(new Event("input", { bubbles: true }));
        remember(term);
        hide();
      });
      const del = document.createElement("button");
      del.type = "button";
      del.className = "search-suggest-del";
      del.textContent = "×";
      del.title = t("delete");
      del.setAttribute("aria-label", t("delete"));
      del.addEventListener("mousedown", (e) => {
        e.preventDefault();
        e.stopPropagation();
        forget(term);
        show(); // re-render the shortened list, keeping the popup open
      });
      row.append(label, del);
      pop.appendChild(row);
    }
    const r = input.getBoundingClientRect();
    // Size to the terms (short), not the full search field: a full-width bar over a
    // wide library search looked oversized. Clamp: at least 220px, at most the field.
    pop.style.width = "";
    pop.style.minWidth = `${Math.min(220, r.width)}px`;
    pop.style.maxWidth = `${r.width}px`;
    placePopover(pop, r);
  };
  // Open on an explicit click/type, NOT on the focus the dialog may grab on open.
  input.addEventListener("pointerdown", show);
  input.addEventListener("input", show);
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") { remember(input.value); hide(); } });
  input.addEventListener("blur", () => { remember(input.value); setTimeout(hide, 120); });
}

// ---- Fullscreen media viewer (lightbox): click a preview to zoom (wheel) + pan
// (drag). Stays inside the app window; X / Escape / backdrop-click close it.
export let lbScale = 1, lbX = 0, lbY = 0, lbDrag = null;
export function lbMedia() {
  return $("lightbox-img").classList.contains("hidden") ? $("lightbox-video") : $("lightbox-img");
}
export function lbApply() {
  lbMedia().style.transform = `translate(${lbX}px, ${lbY}px) scale(${lbScale})`;
  $("lightbox-stage").classList.toggle("zoomed", lbScale > 1);
}
// Screen point -> the media's own local CSS coordinates at the CURRENT scale
// removed again, so it stays comparable to lbX/lbY (also pre-scale units)
// regardless of zoom level. Reads the live, already-transformed bounding
// rect, same trick as the video editor's toSource().
export function lbToLocal(clientX, clientY) {
  const rect = lbMedia().getBoundingClientRect();
  return { x: (clientX - rect.left) / lbScale, y: (clientY - rect.top) / lbScale };
}
export function openLightbox(src, isVideo) {
  if (!src) return;
  lbScale = 1; lbX = 0; lbY = 0;
  const img = $("lightbox-img"), vid = $("lightbox-video");
  img.classList.toggle("hidden", isVideo);
  vid.classList.toggle("hidden", !isVideo);
  if (isVideo) { vid.src = src; vid.play?.().catch(() => {}); }
  else { vid.pause?.(); vid.removeAttribute("src"); img.src = src; }
  lbApply();
  $("lightbox").classList.remove("hidden");
}
export function closeLightbox() {
  if ($("lightbox").classList.contains("hidden")) return;
  $("lightbox").classList.add("hidden");
  const vid = $("lightbox-video");
  vid.pause?.(); vid.removeAttribute("src");
  $("lightbox-img").removeAttribute("src");
}
export function wireLightbox() {
  const stage = $("lightbox-stage");
  $("lightbox-close").addEventListener("click", closeLightbox);
  // Backdrop click closes only when not zoomed (a zoomed click may be a pan).
  stage.addEventListener("click", (e) => { if (lbScale === 1 && e.target === stage) closeLightbox(); });
  stage.addEventListener("wheel", (e) => {
    e.preventDefault();
    // Keep the point under the cursor fixed instead of always zooming to the
    // media's centre: read it before and after the scale change and slide
    // the pan by the difference, scaled back up -- the pan is a CSS translate
    // applied AFTER the scale in the same transform, so it moves the media in
    // already-scaled screen pixels, not the pre-scale ones lbToLocal reads in.
    const before = lbToLocal(e.clientX, e.clientY);
    lbScale = Math.min(8, Math.max(1, lbScale * (e.deltaY < 0 ? 1.15 : 1 / 1.15)));
    if (lbScale === 1) {
      lbX = 0; lbY = 0;
    } else {
      lbApply(); // commit the new scale so the rect below reflects it
      const after = lbToLocal(e.clientX, e.clientY);
      lbX += (after.x - before.x) * lbScale;
      lbY += (after.y - before.y) * lbScale;
    }
    lbApply();
  }, { passive: false });
  stage.addEventListener("pointerdown", (e) => {
    if (lbScale <= 1 || e.target.closest(".lightbox-close")) return;
    lbDrag = { x: e.clientX, y: e.clientY };
    stage.classList.add("grabbing");
    stage.setPointerCapture?.(e.pointerId);
  });
  stage.addEventListener("pointermove", (e) => {
    if (!lbDrag) return;
    const z = uiZoom();
    lbX += (e.clientX - lbDrag.x) / z;
    lbY += (e.clientY - lbDrag.y) / z;
    lbDrag.x = e.clientX; lbDrag.y = e.clientY;
    lbApply();
  });
  const endDrag = () => { lbDrag = null; stage.classList.remove("grabbing"); };
  stage.addEventListener("pointerup", endDrag);
  stage.addEventListener("pointercancel", endDrag);
}

// The "?" overview both editors open -- same modal/modal-narrow/modal-titlebar
// shell as every other list modal, filled straight from SHORTCUTS.
export let shortcutsModalOpener = null;
// `withGlobal` adds the rebindable global/per-tile hotkey list on top (and gives the
// dialog the width it needs); the editors' "?" buttons open the plain key list.
// `allWindows` lists one titled section per window instead of one flat table.
export function openShortcutsModal(scope, { withGlobal = false, allWindows = false } = {}) {
  shortcutsModalOpener = document.activeElement;
  $("shortcuts-global").classList.toggle("hidden", !withGlobal);
  $("shortcuts-modal").querySelector(".modal").classList.toggle("modal-narrow", !withGlobal);
  if (withGlobal) {
    $("hotkeys-search").value = "";
    renderHotkeysList();
  }
  const list = $("shortcuts-modal-list");
  list.innerHTML = "";
  const addRows = (rows) => {
    for (const row of rows) {
      const r = document.createElement("div");
      r.className = "shortcuts-row";
      const desc = document.createElement("span");
      desc.className = "shortcuts-row-desc";
      desc.textContent = row.desc;
      const keys = document.createElement("span");
      keys.className = "shortcuts-row-keys";
      keys.textContent = row.keys;
      r.append(desc, keys);
      list.appendChild(r);
    }
  };
  if (allWindows) {
    // The library has no keys of its own: its rows (if any) belong to the main window.
    const sections = [];
    for (const s of shortcutSections(Array.isArray(scope) ? scope : [scope])) {
      const key = s.scope === "library" ? "mainWindow" : s.scope;
      const into = sections.find((x) => x.key === key);
      if (into) into.rows.push(...s.rows);
      else sections.push({ key, rows: [...s.rows] });
    }
    for (const s of sections) {
      if (!s.rows.length) continue;
      const h = document.createElement("h3");
      h.className = "shortcuts-section-title";
      h.textContent = t(SCOPE_TITLE_KEYS[s.key]);
      list.appendChild(h);
      addRows(s.rows);
    }
  } else {
    addRows(shortcutRows(scope));
  }
  $("shortcuts-modal").classList.remove("hidden");
}
// Every shortcut in one dialog: global hotkeys on top, the keys of each window below.
export function openAllShortcuts() {
  openShortcutsModal(ALL_SHORTCUT_SCOPES, { withGlobal: true, allWindows: true });
  $("hotkeys-search").focus();
}
export function closeShortcutsModal() {
  $("shortcuts-modal").classList.add("hidden");
  // Mirrors the video editor's own shortcuts overlay: the opener button is
  // usually still there, but it can be gone by the time this runs (a right
  // click that removed its own row, for example) -- focus falls back to the
  // body rather than calling focus on a detached element or leaving the page
  // with no focus at all.
  const target = (shortcutsModalOpener?.isConnected && shortcutsModalOpener) || document.body;
  if (typeof target.focus === "function") target.focus();
  shortcutsModalOpener = null;
}

export function bindWindowChrome() {
  // Show/hide the top and bottom bars.
  $("hide-top").addEventListener("click", () => setBars(false, settings.show_composer !== false));
  $("show-top").addEventListener("click", () => setBars(true, settings.show_composer !== false));
  $("hide-bottom").addEventListener("click", () => setBars(settings.show_header !== false, false));
  $("show-bottom").addEventListener("click", () => setBars(settings.show_header !== false, true));

  // A double-click on any one of the four buttons folds or unfolds both bars
  // together, target state taken from the clicked button; the single click
  // that already fired on the way to the double-click is left as it landed.
  const bindBarToggleDbl = (id, target) => $(id).addEventListener("dblclick", () => {
    if (flag("barToggleBoth")) setBars(target, target);
  });
  bindBarToggleDbl("hide-top", false);
  bindBarToggleDbl("show-top", true);
  bindBarToggleDbl("hide-bottom", false);
  bindBarToggleDbl("show-bottom", true);
}
