// Copying a prompt: usage log, formatted copy, variable paste, keyboard focus ring and
// close-on-copy.
import { $, errToast, favView, flag, gridEl, invoke, KB_FOCUS_FADE_MS, optFlag, prompts, t, toast, val } from "./core.js";
import { playCopySound } from "./floatShared.js";
import { dialogTokens, peekRotatedText, resolveVars, varsWindowPending } from "./promptVars.js";
import { renderMarkdown } from "./markdown.js";
import { activeView, favLayoutMap, layoutOf } from "./views.js";
import { chainLock, chainMode, chainSel, onChainClick, setChainLock, setChainMode, toggleChainPick } from "./chain.js";
import { autoPasteNow, showCopied } from "./tiles.js";
import { copyResolved } from "./library.js";
import { recordToolUsage } from "./management.js";

// Record a copy for the history/usage journal (the backend honours the privacy flag).
// Every successful copy path ends here, so this is the one place that may start the
// cooldown.
export const recordCopy = (id) => {
  markCopied(id);
  if (optFlag("copySound")) playCopySound(val("copySoundVolume"));
  return invoke("record_copy", { id }).catch(() => {});
};

// Per-prompt copy throttle: block re-copying the SAME prompt within the cooldown
// (expert value, 0 = off). Other prompts are never blocked, so a wrong pick can
// be corrected instantly. Returns true if this copy should be skipped.
// The check is pure: a copy that never happened (the placeholder dialog was cancelled)
// must not lock the prompt out for the whole cooldown.
export const copyTimes = new Map();
export const COPY_TIMES_CAP = 256; // sweep entries past the cooldown once the map grows
export function copyOnCooldown(id) {
  const ms = val("copyCooldownMs");
  if (!ms) return false;
  const last = copyTimes.get(id);
  return !!last && Date.now() - last < ms;
}
// Start the cooldown for a prompt that really was copied.
export function markCopied(id) {
  const ms = val("copyCooldownMs");
  if (!ms) return;
  const now = Date.now();
  if (copyTimes.size >= COPY_TIMES_CAP) {
    for (const [k, t0] of copyTimes) if (now - t0 >= ms) copyTimes.delete(k);
  }
  copyTimes.set(id, now);
}

// Peek the entry a rotating prompt would copy, fill its placeholders, hand the text
// to `copyFn` and advance the rotation only when that copy really happened (a
// cancelled placeholder window or a failed copy burns no entry; a failed peek falls
// back to the stored text and must not be followed by a commit). `afterResolve` runs
// right after the placeholders are answered, cancelled or not. Resolves to copyFn's
// result, false when cancelled.
// Rotating prompts being copied right now: a second activation (double press, hotkey during a
// click) would peek the same entry and then advance the rotation twice, skipping an entry that
// was never copied, so it is dropped while the first one runs.
const rotatingInFlight = new Set();
export async function peekResolveCommit(p, copyFn, opts) {
  const afterResolve = opts?.afterResolve;
  const resolve = opts?.resolve ?? resolveVars;
  const rotating = (p.variants || []).length > 0;
  if (rotating) {
    if (rotatingInFlight.has(p.id)) return false;
    rotatingInFlight.add(p.id);
  }
  try {
    const peek = rotating ? await invoke("rotation_peek", { id: p.id }).catch(() => null) : null;
    const rawText = peek ? peek.text : p.text;
    const text = await resolve(rawText);
    afterResolve?.();
    if (text === null || text === false) return false;
    const ok = await copyFn(text);
    if (ok && rotating && peek) { invoke("rotation_commit", { id: p.id }).catch(() => {}); recordToolUsage("rotation_advance"); }
    return ok;
  } finally {
    if (rotating) rotatingInFlight.delete(p.id);
  }
}

// Copy a text prompt after filling its placeholders. `paste` is only set by
// activations that mean "paste" (Enter on a focused tile, a prompt hotkey with the
// paste option on); a plain click copies and nothing else, exactly like a prompt
// without variables.
export let varsPasteRequested = false;
export async function copyTextWithVars(p, el, paste = false) {
  // The placeholder window is already up for an earlier press: this one is the
  // second half of a double click. It must not start another question, it only
  // upgrades the pending copy to a paste.
  if (varsWindowPending) { varsPasteRequested = true; return; }
  varsPasteRequested = false;
  let pasteAfter = paste;
  const ok = await peekResolveCommit(
    p,
    (text) => invoke("copy_text", { text }).catch((e) => { errToast(e); return false; }),
    {
      afterResolve: () => {
        pasteAfter = paste || (varsPasteRequested && flag("autoPaste"));
        varsPasteRequested = false;
      },
    },
  );
  if (ok) {
    showCopied(el);
    recordCopy(p.id);
    if (pasteAfter) await autoPasteNow(el);
    maybeCloseOnCopy();
  }
}

// Copy formatted (context menu + editor): puts the Markdown text AND its rendered HTML on
// the clipboard in one go. Placeholders go through resolveVars(), and the HTML comes from
// the Markdown preview's own renderer, so the app has one Markdown interpretation.
export async function copyFormattedPrompt(id) {
  const p = prompts.find((x) => x.id === id);
  if (!p) return;
  const ok = await peekResolveCommit(
    p,
    (text) => invoke("copy_formatted", { text, html: renderMarkdown(text) }).catch((e) => { errToast(e); return false; }),
  );
  if (ok) {
    toast(t("copied"));
    recordCopy(id);
  }
}

// ---- Keyboard navigation: arrow keys move a focus ring across the grid,
// Enter/Space copies the focused tile (feature-gated by keyboardNav).
export let kbFocus = null;
export let kbFadeTimer = null;
export function focusTile(id) {
  clearTimeout(kbFadeTimer);
  kbFocus = id;
  for (const el of gridEl.querySelectorAll(".tile")) {
    el.classList.toggle("kb-focus", el.dataset.id === id);
    if (el.dataset.id === id) el.setAttribute("aria-current", "true"); else el.removeAttribute("aria-current");
  }
  // The ring is visual only; the live region makes a screen reader say which tile it sits on.
  const live = document.getElementById("grid-live");
  if (live) live.textContent = gridEl.querySelector(`.tile[data-id="${CSS.escape(id)}"]`)?.getAttribute("aria-label") || "";
  for (const el of gridEl.querySelectorAll(".tile.kb-leaving")) el.classList.remove("kb-leaving");
  gridEl.querySelector(`.tile[data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" });
}
// Clears the focus ring. `fade` smoothly transitions it out (used on window blur).
export function clearKbFocus(fade) {
  const el = kbFocus ? gridEl.querySelector(`.tile[data-id="${CSS.escape(kbFocus)}"]`) : null;
  kbFocus = null;
  for (const tile of gridEl.querySelectorAll(".tile.kb-focus")) { tile.classList.remove("kb-focus"); tile.removeAttribute("aria-current"); }
  const live = document.getElementById("grid-live");
  if (live) live.textContent = "";
  if (fade && el) {
    el.classList.add("kb-leaving");
    setTimeout(() => el.classList.remove("kb-leaving"), 520);
  }
}

export function moveKbFocus(dc, dr) {
  // Use whichever layout the grid is currently showing: the favorites view lays out
  // from favLayoutMap(), not the active view, so arrow keys must follow that.
  const layout = favView ? favLayoutMap() : layoutOf(activeView());
  const cells = Object.entries(layout);
  if (!cells.length) return;
  if (!kbFocus || !layout[kbFocus]) { focusTile(cells[0][0]); return; }
  const [cc, cr] = layout[kbFocus];
  let best = null;
  let bestD = Infinity;
  for (const [id, [col, row]] of cells) {
    if (id === kbFocus) continue;
    const inDir = (dc > 0 && col > cc) || (dc < 0 && col < cc) || (dr > 0 && row > cr) || (dr < 0 && row < cr);
    if (!inDir) continue;
    // Prefer the same row/column: penalise the off-axis distance heavily.
    const d = dc ? Math.abs(col - cc) + Math.abs(row - cr) * 4 : Math.abs(row - cr) + Math.abs(col - cc) * 4;
    if (d < bestD) { bestD = d; best = id; }
  }
  if (best) focusTile(best);
}
// A copy swallowed by the cooldown would give no feedback; a quick shake on the tile says
// "that did nothing".
export function blockedByCooldown(id, el) {
  if (!copyOnCooldown(id)) return false;
  if (el) {
    el.classList.add("cooldown-blocked");
    setTimeout(() => el.classList.remove("cooldown-blocked"), 250);
  }
  return true;
}
// Enter and Space belong to a focused control (button, select, link, ...), not to the focus ring.
function controlOwnsActivateKey(target) {
  return !!target?.closest?.("button, select, a, summary, [role=button], [tabindex]");
}
// One activation path for "copy this prompt now": Enter on a focused tile and a
// prompt hotkey both land here, so usage recording, chain semantics and
// placeholder filling behave identically. `paste` decides whether the copy is also
// pushed into the last app: Enter follows the auto-paste setting, a hotkey follows
// its own option.
export async function activateTile(id, paste = flag("autoPaste")) {
  const p = prompts.find((x) => x.id === id);
  if (!p) return;
  const el = gridEl.querySelector(`.tile[data-id="${CSS.escape(id)}"]`);
  if (chainMode) { await toggleChainPick(p); return; }
  if (blockedByCooldown(id, el)) return;
  if (dialogTokens(p, await peekRotatedText(p)).length) {
    await copyTextWithVars(p, el, paste);
  } else if (await copyResolved(p)) {
    if (el) showCopied(el);
    recordCopy(id);
    if (paste) await autoPasteNow(el);
    maybeCloseOnCopy();
  }
}

// A prompt was triggered from outside the grid: its global hotkey or a tray
// quick-copy entry. Placeholders that still need typing are asked for in a
// small window of their own, so the app the user is working in stays in front;
// everything else copies (and optionally pastes) without ever showing up.
export async function onPromptHotkey(id, paste) {
  const p = prompts.find((x) => x.id === id);
  if (!p) return;
  if (dialogTokens(p, await peekRotatedText(p)).length) {
    // Chain mode is an in-window gesture: a hotkey then still goes through the grid.
    if (!chainMode) { await copyOutsideApp(p, paste); return; }
    await invoke("focus_main_window").catch(() => {});
  }
  await activateTile(id, paste);
}

// The hotkey/tray copy of a prompt with manual placeholders: ask in the standalone
// window, then finish the copy exactly like every other path does.
export async function copyOutsideApp(p, paste) {
  if (copyOnCooldown(p.id)) return;
  const ok = await peekResolveCommit(
    p,
    (text) => invoke("copy_text", { text }).catch((e) => { errToast(e); return false; }),
    { afterResolve: () => { varsPasteRequested = false; } }, // a second press only upgrades copies started in the window
  );
  if (ok) {
    recordCopy(p.id);
    if (paste) await autoPasteNow(null);
  }
}
// Expert opt: hide the window to the tray right after a prompt copy.
export function maybeCloseOnCopy() {
  if (!optFlag("hideWindowOnCopy")) return;
  try { window.__TAURI__?.window?.getCurrentWindow?.()?.hide?.().catch?.(() => {}); } catch { /* no-op */ }
}

export function bindCopyFlow() {
  // Chain mode: combine several text buttons into one clipboard copy. A single
  // click toggles it.
  $("chain-btn").addEventListener("click", onChainClick);
  // A paste inside this window ends chain mode (unless latched); the clipboard
  // already holds the chain. A paste in another program is not seen here, so the
  // chain simply stays until the chain button ends it.
  document.addEventListener("keydown", (e) => {
    if (chainMode && !chainLock && (e.ctrlKey || e.metaKey) && (e.key === "v" || e.key === "V")) setChainMode(false);
  });
  // Arrow keys move a focus ring across grid tiles; Enter/Space copies.
  const NAV = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
  document.addEventListener("keydown", (e) => {
    if (!flag("keyboardNav")) return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.target?.closest?.("input, textarea, [contenteditable]")) return;
    if (document.querySelector(".overlay:not(.hidden)")) return; // a dialog is open
    if (NAV[e.key]) { e.preventDefault(); moveKbFocus(...NAV[e.key]); }
    else if ((e.key === "Enter" || e.key === " ") && kbFocus && !controlOwnsActivateKey(e.target)) { e.preventDefault(); activateTile(kbFocus); }
  });
  // Hold Shift during keyboard navigation to arm chaining on the fly: it switches
  // on while Shift is down and off again when released: unless a prompt was picked
  // into the chain during the hold, in which case it latches "always on" so the
  // user can keep building the chain from there.
  let shiftChain = false;
  const kbBlocked = (e) => e.target.closest("input, textarea, [contenteditable]") || document.querySelector(".overlay:not(.hidden)");
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Shift" || e.repeat || shiftChain || chainMode) return;
    // Only while the focus ring is in use: Shift+Tab or Shift+wheel elsewhere is not chaining.
    if (!kbFocus || !flag("keyboardNav") || !flag("chainPrompts") || kbBlocked(e)) return;
    shiftChain = true;
    setChainMode(true);
  });
  document.addEventListener("keyup", (e) => {
    if (e.key !== "Shift" || !shiftChain) return;
    shiftChain = false;
    if (!chainSel.length) { setChainMode(false); return; }
    if (flag("chainLock")) setChainLock(); // picked something → keep the chain alive
  });
}

export function wireCopyFlow() {
  // The keyboard focus ring fades a short while after the window loses focus, so a
  // left-behind highlight doesn't linger over other apps (the delay is
  // KB_FOCUS_FADE_MS; the fade-out itself is a smooth 480ms CSS transition).
  window.addEventListener("blur", () => {
    if (!kbFocus || !flag("keyboardNav")) return;
    clearTimeout(kbFadeTimer);
    kbFadeTimer = setTimeout(() => clearKbFocus(true), KB_FOCUS_FADE_MS);
  });
  window.addEventListener("focus", () => { clearTimeout(kbFadeTimer); });
}
