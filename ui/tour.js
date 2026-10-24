// First-run tour and the manual tour.
import { $, EXPERT_ACTIONS, t, undoToast } from "./core.js";
import { TOUR_LAST_STEP, TOUR_STEP_COUNT, TOUR_STEPS } from "./tourContent.js";
import { trapTabWithin } from "./floatShared.js";
import { saveUiState } from "./backups.js";

// ---- First-run tour ----
// A few fixed steps in one shared frame (#tour-modal), content swapped per step: a small
// hand-drawn illustration + up to three lines of text each (TOUR_STEPS). The tour changes
// no setting. Opened two ways:
//  - Automatically once, right after the example prompts are seeded on a genuinely
//    fresh install (maybeOpenFirstRunTour, called from init()'s reveal()).
//  - Manually via the expert menu's gated "tour-open" action (EXPERT_ACTIONS).
// Finish and Skip both only mark the tour as seen.

export function bindTour() {
  $("tour-skip").addEventListener("click", tourSkip);
  $("tour-back").addEventListener("click", () => tourGoto(tourStep - 1));
  $("tour-next").addEventListener("click", () => (tourStep === TOUR_LAST_STEP ? tourFinish() : tourGoto(tourStep + 1)));
}

export let tourStep = 0;

export function renderTourDrawingStep(step) {
  const wrap = document.createElement("div");
  wrap.className = "tour-step";
  const art = document.createElement("div");
  art.className = "tour-art";
  art.setAttribute("aria-hidden", "true");
  art.innerHTML = step.svg;
  const text = document.createElement("div");
  text.className = "tour-text";
  for (const lk of step.lineKeys) {
    const p = document.createElement("p");
    p.textContent = t(lk);
    text.appendChild(p);
  }
  wrap.append(art, text);
  return wrap;
}

export function renderTourStep() {
  const isLast = tourStep === TOUR_LAST_STEP;
  $("tour-title").textContent = t(TOUR_STEPS[tourStep].titleKey);
  const body = $("tour-body");
  body.innerHTML = "";
  body.appendChild(renderTourDrawingStep(TOUR_STEPS[tourStep]));
  const dots = $("tour-dots");
  dots.innerHTML = "";
  for (let i = 0; i < TOUR_STEP_COUNT; i++) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "tour-dot" + (i === tourStep ? " active" : "");
    b.textContent = i === tourStep ? "●" : "○";
    b.setAttribute("aria-label", t(TOUR_STEPS[i].titleKey));
    b.addEventListener("click", () => tourGoto(i));
    dots.appendChild(b);
  }
  $("tour-back").classList.toggle("hidden", tourStep === 0);
  $("tour-next").textContent = t(isLast ? "tourFinish" : "tourNext");
  $("tour-next").focus();
}

export function tourGoto(i) {
  tourStep = Math.max(0, Math.min(TOUR_LAST_STEP, i));
  renderTourStep();
}

// Keys while the first-run tour is open: Tab stays inside the tour, Escape skips it
// (with an undo), the arrows and Enter step through it.
export function tourKeydown(e) {
  // A confirmation dialog on top owns every key while it is open --
  // otherwise Escape would close both it and the tour underneath on one press.
  if (!$("confirm-modal").classList.contains("hidden")) return;
  if (e.key === "Tab") { trapTabWithin($("tour-modal"), e); return; }
  if (e.key === "Escape") { e.preventDefault(); tourSkipWithUndo(); return; }
  if (e.key === "ArrowLeft") { e.preventDefault(); tourGoto(tourStep - 1); return; }
  if (e.key === "ArrowRight") { e.preventDefault(); tourGoto(tourStep + 1); return; }
  if (e.key === "Enter" && !e.target.closest("button")) {
    // A focused button already turns Enter into its own click; only steps in for
    // everything else.
    e.preventDefault();
    tourStep === TOUR_LAST_STEP ? tourFinish() : tourGoto(tourStep + 1);
  }
}

// The dialog must not visibly resize while paging through the steps; a fixed height
// is either too tall for the shortest step or too short for the tallest. So every step is
// measured once, right after the modal becomes visible (a hidden ancestor reports 0
// height), and the tallest is kept.
export function tourMeasureMaxStepHeight() {
  const body = $("tour-body");
  let max = 0;
  for (let i = 0; i < TOUR_STEP_COUNT; i++) {
    const wrap = renderTourDrawingStep(TOUR_STEPS[i]);
    body.innerHTML = "";
    body.appendChild(wrap);
    // Layout height, not the painted rect: the open animation scales the dialog while it
    // is measured.
    max = Math.max(max, wrap.offsetHeight);
  }
  return max;
}

export async function openTour() {
  tourStep = 0;
  $("tour-modal").classList.remove("hidden");
  const maxH = tourMeasureMaxStepHeight();
  document.documentElement.style.setProperty("--tour-step-min-h", `${Math.ceil(maxH)}px`);
  renderTourStep();
  document.addEventListener("keydown", tourKeydown);
}

export function closeTour() {
  $("tour-modal").classList.add("hidden");
  document.removeEventListener("keydown", tourKeydown);
}

export async function tourSkip() {
  closeTour();
  await saveUiState("tourSeen", true); // Skip writes nothing else
}

// Escape skips at once; the toast's Undo reopens the tour where it started.
export async function tourSkipWithUndo() {
  await tourSkip();
  undoToast(t("tourSkippedToast"), () => openTour());
}

export async function tourFinish() {
  closeTour();
  await saveUiState("tourSeen", true);
}

export function wireTour() {
  EXPERT_ACTIONS["tour-open"] = () => openTour();
}
