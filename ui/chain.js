// Chain mode: queue several prompts and copy them as one block.
import { $, DRAG_THRESHOLD, errToast, flag, gridEl, invoke, prompts, settings, t } from "./core.js";
import { resolveVars, varTokens } from "./promptVars.js";
import { recordToolUsage } from "./management.js";

// ---- Prompt chaining ----
// The chain button turns several TEXT buttons into one clipboard copy: click it,
// then click text buttons in the order you want them; each shows its position (1,
// 2, 3 …) and the joined text (newline-separated) lands on the clipboard after
// every pick. It ends after a paste (Ctrl+V inside this window) or when
// the button is clicked again; switching to another program leaves it
// untouched. Only text
// prompts qualify: image/file/video buttons copy media, which can't be joined.
export let chainMode = false;
export let chainLock = false; // "always on": stays after a copy until turned off by hand
export function assignChainLock(v) { chainLock = v; }
export const chainSel = []; // selected prompt ids, in pick order
export const chainTexts = new Map(); // id -> resolved copy text (variables already filled)
const chainRaw = new Map(); // id -> the entry as picked, placeholders still in (for the overview)

export const isTextPrompt = (p) => !!p && !p.copy_image && !p.file_path;

// What goes between two chained prompts: the chainSeparator expert choice
// (a blank line unless changed).
export const CHAIN_SEPARATORS = { blank: "\n\n", break: "\n", space: " ", none: "" };

export function chainSeparator() {
  const key = settings.ui_texts?.chainSeparator;
  return Object.prototype.hasOwnProperty.call(CHAIN_SEPARATORS, key) ? CHAIN_SEPARATORS[key] : CHAIN_SEPARATORS.blank;
}

export function chainText() {
  return chainSel.map((id) => chainTexts.get(id) ?? "").join(chainSeparator());
}

// `commitId` is the prompt just newly picked into the chain (if it rotates):
// its pointer only advances once this actual clipboard write succeeds, never from
// the pick alone (a pick that never reaches the clipboard must burn nothing).
export function pushChainClipboard(commitId) {
  if (!chainSel.length) return;
  invoke("copy_text", { text: chainText() })
    .then((ok) => {
      if (!ok) return;
      recordToolUsage("chain");
      if (commitId) invoke("rotation_commit", { id: commitId }).catch(() => {});
    })
    .catch((e) => errToast(e));
}

// Reflect one tile's chain state (skip / picked + order badge) without a re-render.
export function applyChainToTile(tile, p) {
  tile.classList.toggle("chain-skip", chainMode && !isTextPrompt(p));
  const idx = chainMode ? chainSel.indexOf(p.id) : -1;
  tile.classList.toggle("chain-picked", idx >= 0);
  tile.querySelector(".tile-chain-badge")?.remove();
  if (idx >= 0) {
    const badge = document.createElement("span");
    badge.className = "tile-chain-badge";
    badge.textContent = String(idx + 1);
    tile.appendChild(badge);
  }
}

export function refreshChainUi() {
  for (const tile of gridEl.querySelectorAll(".tile")) {
    const p = prompts.find((x) => x.id === tile.dataset.id);
    if (p) applyChainToTile(tile, p);
  }
  renderChainOverview();
}

// The chain overview strip: one row per picked step, in pick order, with the
// placeholders it needs and the reserved names it fills, from chainVarReport()
// (ui/chainvars.js).
export function renderChainOverview() {
  const bar = $("chain-overview");
  if (!bar) return;
  bar.classList.toggle("hidden", !(chainMode && chainSel.length > 0));
  if (!(chainMode && chainSel.length > 0)) { bar.innerHTML = ""; return; }
  const report = chainVarReport(chainSel.map((id) => chainRaw.get(id) ?? (prompts.find((x) => x.id === id) || {}).text ?? ""));
  bar.innerHTML = "";
  const title = document.createElement("div");
  title.className = "chain-ov-title";
  title.textContent = t("chainOverviewTitle");
  bar.appendChild(title);
  // Rows get their own wrapping row instead of sharing the column with the
  // title above, so the steps flow left to right and wrap as a block.
  const list = document.createElement("div");
  list.className = "chain-ov-list";
  bar.appendChild(list);
  chainSel.forEach((id, i) => {
    const p = prompts.find((x) => x.id === id);
    const info = report[i] || { needs: [], fills: [], missing: [] };
    const row = document.createElement("div");
    row.className = "chain-ov-row";
    row.classList.toggle("chain-ov-missing", info.missing.length > 0);
    const num = document.createElement("span");
    num.className = "chain-ov-num";
    num.textContent = String(i + 1);
    row.append(num);
    const name = document.createElement("span");
    name.className = "chain-ov-name";
    name.textContent = p ? p.name || "" : "";
    row.append(name);
    const badges = document.createElement("span");
    badges.className = "chain-ov-badges";
    const addBadges = (labelKey, names, cls) => {
      if (!names.length) return;
      const lbl = document.createElement("span");
      lbl.className = "chain-ov-label";
      lbl.textContent = t(labelKey) + ":";
      badges.append(lbl);
      for (const n of names) {
        const b = document.createElement("span");
        b.className = `chain-ov-badge ${cls}`;
        b.textContent = n;
        badges.append(b);
      }
    };
    addBadges("chainNeeds", info.needs, "chain-ov-needs");
    addBadges("chainFills", info.fills, "chain-ov-fills");
    row.append(badges);
    if (info.missing.length) {
      const miss = document.createElement("span");
      miss.className = "chain-ov-missing-note";
      miss.textContent = `${t("chainMissing")} ${info.missing.join(", ")}`;
      row.append(miss);
    }
    row.title = t("chainStepHint");
    row.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      e.stopPropagation();
      startChainStepDrag(e, row, list);
    });
    row.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      removeChainStep(i);
    });
    list.append(row);
  });
}

// Drops one step from the chain (same removal as un-picking its tile) and puts
// the shortened chain on the clipboard. An empty chain stays active.
export function removeChainStep(i) {
  const id = chainSel[i];
  if (id === undefined) return;
  chainSel.splice(i, 1);
  chainTexts.delete(id);
  chainRaw.delete(id);
  refreshChainUi();
  pushChainClipboard();
}

// Reorders the chain steps with plain pointer events (the webview's own drop
// target swallows HTML5 drag and drop). A press that moves past DRAG_THRESHOLD
// starts the drag; the step under the pointer shows a before/after mark from
// the pointer's side of its middle, which also works across wrapped lines.
export function startChainStepDrag(down, row, list) {
  const rows = () => [...list.querySelectorAll(".chain-ov-row")];
  const from = rows().indexOf(row);
  if (from < 0) return;
  let dragging = false;
  let target = null; // { el, after }
  const clearMarks = () => {
    for (const r of rows()) r.classList.remove("drag-over", "drop-before", "drop-after");
  };
  const move = (e) => {
    if (e.pointerId !== down.pointerId) return;
    if (!dragging) {
      if (Math.abs(e.clientX - down.clientX) < DRAG_THRESHOLD &&
          Math.abs(e.clientY - down.clientY) < DRAG_THRESHOLD) return;
      dragging = true;
      row.classList.add("dragging");
      try { row.setPointerCapture(down.pointerId); } catch (_) {}
    }
    e.stopPropagation();
    clearMarks();
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest?.(".chain-ov-row");
    if (el && el !== row && list.contains(el)) {
      const r = el.getBoundingClientRect();
      const after = e.clientX > r.left + r.width / 2;
      el.classList.add("drag-over", after ? "drop-after" : "drop-before");
      target = { el, after };
    } else {
      target = null;
    }
  };
  const end = (e, commit) => {
    if (e.pointerId !== down.pointerId) return;
    window.removeEventListener("pointermove", move, true);
    window.removeEventListener("pointerup", up, true);
    window.removeEventListener("pointercancel", cancel, true);
    try { row.releasePointerCapture(down.pointerId); } catch (_) {}
    row.classList.remove("dragging");
    clearMarks();
    if (!dragging) return;
    e.stopPropagation();
    if (!commit || !target) return;
    let to = rows().indexOf(target.el) + (target.after ? 1 : 0);
    if (to > from) to -= 1;
    if (to === from) return;
    const [moved] = chainSel.splice(from, 1);
    chainSel.splice(to, 0, moved);
    refreshChainUi();
    pushChainClipboard();
  };
  const up = (e) => end(e, true);
  const cancel = (e) => end(e, false);
  window.addEventListener("pointermove", move, true);
  window.addEventListener("pointerup", up, true);
  window.addEventListener("pointercancel", cancel, true);
}

// A rotating/variable prompt's pick goes through an await (peek, var fill)
// between reading chainSel and mutating it -- without this, a fast double
// click on the same tile passes the "already selected?" check twice, both
// branches push, and the tile that DID get de-picked on the next click still
// matches indexOf on the leftover duplicate, so it stays visibly picked with
// its text silently gone. One in-flight pick/unpick per id at a time.
export const chainPicking = new Set();
export async function toggleChainPick(p) {
  if (!isTextPrompt(p)) return; // only text buttons chain
  if (chainPicking.has(p.id)) return;
  chainPicking.add(p.id);
  try {
    const i = chainSel.indexOf(p.id);
    if (i >= 0) {
      chainSel.splice(i, 1); // deselect → the rest renumber
      chainTexts.delete(p.id);
      chainRaw.delete(p.id);
      refreshChainUi();
      pushChainClipboard();
      return;
    }
    // Rotating prompt: peek the entry a copy would use (never consumes), resolve
    // {?variables} on THAT text, right when the button is picked into the chain.
    let rawText = p.text || p.name || "";
    let peeked = false; // only a successful peek may be followed by a commit below:
                         // a failed peek already fell back to p.text (a stale/wrong
                         // entry to consume) and committing anyway would advance
                         // whatever the pointer's REAL position actually is.
    if ((p.variants || []).length) {
      const peek = await invoke("rotation_peek", { id: p.id }).catch(() => null);
      if (peek) { rawText = peek.text; peeked = true; }
    }
    let text = rawText;
    if (varTokens(p, rawText).length) {
      const filled = await resolveVars(rawText);
      if (filled === null) return; // cancelled → leave it unselected
      text = filled;
    }
    if (!chainMode) return; // the chain ended while the entry or its dialog was pending
    chainSel.push(p.id);
    chainTexts.set(p.id, text);
    chainRaw.set(p.id, rawText);
    refreshChainUi();
    pushChainClipboard(peeked ? p.id : null);
  } finally {
    chainPicking.delete(p.id);
  }
}

// The tile's own chain-start button: a second entry point into the SAME
// mechanism a click on #chain-btn followed by a first tile pick already
// uses, not a parallel one. chainLock stays untouched: starting from a tile
// behaves like a normal, unlatched start.
export async function startChainWith(p) {
  if (!flag("chainPrompts") || !flag("chainQuickStart")) return;
  if (chainMode || !isTextPrompt(p)) return; // button is hidden either way -- belt and braces
  setChainMode(true);
  await toggleChainPick(p);
}

export function setChainMode(on) {
  if (on && !flag("chainPrompts")) return;
  chainMode = on;
  if (!on) { chainSel.length = 0; chainTexts.clear(); chainRaw.clear(); chainLock = false; }
  document.body.classList.toggle("chain-mode", on);
  const btn = $("chain-btn");
  if (btn) {
    btn.classList.toggle("active", on);
    btn.classList.toggle("chain-locked", chainLock);
    btn.setAttribute("aria-pressed", String(on));
  }
  refreshChainUi();
}

// Latch chaining "always on": an unlocked chain ends after a paste in this
// window, a locked one stays until the user turns it off by hand. The button
// fills with the accent colour to mark the locked (infinite-repeat) state.
export function setChainLock() {
  if (!flag("chainPrompts") || !flag("chainLock")) return;
  chainLock = true;
  setChainMode(true); // turning on never clears the current picks; refreshes the button state
}

// One tap turns chaining on, the next turns it off. With latching enabled the
// tap goes straight to the latched (∞) state, so the chain survives a copy.
export function onChainClick() {
  if (chainMode) { setChainMode(false); return; }
  if (flag("chainLock")) setChainLock();
  else setChainMode(true);
}
