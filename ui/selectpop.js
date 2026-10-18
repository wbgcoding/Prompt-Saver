// The one themed list popup for every <select> in every window. The native
// dropdown of a <select> is a square OS list that ignores the theme; this
// replaces it with the same rounded popup the grid-size and settings pickers
// use. Each window that imports this gets its own popup element, created on
// first use, and the popup is placed by the shared rule in popover.js.
//
// attachSelectPicker(sel) marks the select with data-select-picker so a test
// (and a reader of the DOM) can tell a wired select from a bare one.
import { placePopover } from "./popover.js";

let pop = null;
let onPick = null;
let anchor = null; // the select that opened the popup (click again = close)
let suppressOpen = false;
let cursor = -1; // keyboard highlight, index into the pickable rows

function ensurePop() {
  if (pop) return pop;
  pop = document.createElement("div");
  pop.className = "num-pop select-pop hidden";
  pop.setAttribute("role", "listbox");
  // A press inside the popup (its scrollbar included) must not pull focus off the
  // select, or the keyboard would stop steering the list.
  pop.addEventListener("mousedown", (e) => e.preventDefault());
  document.body.appendChild(pop);
  document.addEventListener("pointerdown", (e) => {
    if (!onPick || pop.contains(e.target)) return;
    // Clicking the anchor of the open popup toggles it closed instead of
    // letting the following mousedown reopen it immediately.
    suppressOpen = !!anchor && anchor.contains(e.target);
    closeValuePop();
  });
  // The popup is fixed to the window, so a scrolling panel behind it would leave
  // it floating away from its select.
  document.addEventListener("scroll", (e) => {
    if (onPick && e.target !== document && !pop.contains(e.target)) closeValuePop();
  }, true);
  window.addEventListener("resize", () => closeValuePop());
  return pop;
}

const rowsOf = () => (pop ? [...pop.querySelectorAll("button:not(.sep)")] : []);

function highlight(i) {
  const rows = rowsOf();
  if (!rows.length) return;
  cursor = Math.max(0, Math.min(rows.length - 1, i));
  rows.forEach((r, k) => r.classList.toggle("kb", k === cursor));
  rows[cursor].scrollIntoView({ block: "nearest" });
}

export function openValuePop(a, items, current, pick) {
  ensurePop();
  onPick = pick;
  anchor = a;
  pop.innerHTML = "";
  let selected = null;
  for (const item of items) {
    const row = document.createElement("button");
    row.type = "button";
    row.tabIndex = -1;
    row.textContent = item.label;
    // Font entries preview themselves in their own typeface.
    if (item.font) row.style.fontFamily = item.font;
    // A disabled option is a section separator, not a choice.
    if (item.disabled) {
      row.className = "sep";
      row.disabled = true;
      pop.appendChild(row);
      continue;
    }
    row.setAttribute("role", "option");
    if (String(item.value) === String(current)) {
      row.className = "sel";
      row.setAttribute("aria-selected", "true");
      selected = row;
    }
    row.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      const done = onPick;
      closeValuePop();
      done(item.value);
    });
    pop.appendChild(row);
  }
  if (anchor.setAttribute) anchor.setAttribute("aria-expanded", "true");
  // Width follows the widest entry, not the anchor: so the clamp has to run on the
  // real box, which is why it happens after the rows exist.
  placePopover(pop, a.getBoundingClientRect());
  if (selected) selected.scrollIntoView({ block: "center" });
  const rows = rowsOf();
  cursor = selected ? rows.indexOf(selected) : -1;
}

// True while a value list is open, so a caller's Escape handler can leave the key to the list.
export function isValuePopOpen() {
  return !!onPick;
}

export function closeValuePop() {
  if (anchor && anchor.setAttribute) anchor.setAttribute("aria-expanded", "false");
  onPick = null;
  anchor = null;
  cursor = -1;
  if (pop) pop.classList.add("hidden");
}

// Replace the native dropdown of a <select> with the scrollable popup.
export function attachSelectPicker(sel) {
  if (!sel || sel.dataset.selectPicker) return;
  sel.dataset.selectPicker = "1";
  sel.setAttribute("aria-haspopup", "listbox");
  sel.setAttribute("aria-expanded", "false");

  const open = () => {
    if (sel.disabled) return;
    const items = [...sel.options].map((o) => ({
      value: o.value,
      label: o.textContent,
      font: o.style.fontFamily,
      disabled: o.disabled,
    }));
    openValuePop(sel, items, sel.value, (v) => {
      // Locked while the list was open (a take started): the choice is void.
      if (sel.disabled) return;
      sel.value = v;
      sel.dispatchEvent(new Event("change"));
      sel.focus();
    });
  };

  sel.addEventListener("mousedown", (e) => {
    e.preventDefault();
    // The default action would have focused the select; do it by hand so the
    // keyboard keeps working on the open list.
    if (!sel.disabled) sel.focus();
    if (suppressOpen) { suppressOpen = false; return; }
    open();
  });

  sel.addEventListener("keydown", (e) => {
    const isOpen = anchor === sel && onPick;
    if (!isOpen) {
      if (e.key === "Enter" || e.key === " " || (e.altKey && e.key === "ArrowDown")) {
        e.preventDefault();
        open();
      }
      return;
    }
    const rows = rowsOf();
    switch (e.key) {
      case "ArrowDown": highlight(cursor + 1); break;
      case "ArrowUp": highlight(cursor < 0 ? rows.length - 1 : cursor - 1); break;
      case "Home": highlight(0); break;
      case "End": highlight(rows.length - 1); break;
      case "Enter":
      case " ": {
        const row = rows[cursor];
        if (row) row.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true }));
        else closeValuePop();
        break;
      }
      case "Escape":
        // Only the list closes; a dialog around the select stays.
        e.stopPropagation();
        closeValuePop();
        break;
      case "Tab":
        closeValuePop();
        return;
      default:
        return;
    }
    e.preventDefault();
  });
}
