// First-run tour and the manual tour.
import { $, errToast, EXPERT_ACTIONS, flagState, invoke, settings, shipsOn, t, undoToast } from "./core.js";
import { TOUR_FEATURES, TOUR_LAST_STEP, TOUR_STEP_COUNT, TOUR_STEPS } from "./tourContent.js";
import { applyFlags, currentDataDir, jumpToExpertSetting } from "./expertMenu.js";
import { openHelpAt } from "./helpView.js";
import { trapTabWithin } from "./floatShared.js";
import { saveUiState } from "./backups.js";

// ---- First-run tour ----
// Five fixed steps in one shared frame (#tour-modal), content swapped per step.
// Steps 1-4 are a small hand-drawn illustration + up to three lines of text; step 5
// is what else the program can do (TOUR_FEATURES below) plus what is already set
// up (TOUR_SETUP below). Opened two ways:
//  - Automatically once, right after the example prompts are seeded on a genuinely
//    fresh install (maybeOpenFirstRunTour, called from init()'s reveal()). Step 5
//    buffers its switches in a local draft and only writes what differs from the
//    shipped default (features) or the value the tour opened with (setup), on
//    Finish. Skip writes nothing but tourSeen.
//  - Manually via the expert menu's gated "tour-open" action (EXPERT_ACTIONS). Step 5
//    then writes live, exactly like a normal expert-menu flag row: nothing is
//    buffered or reset just from reopening it this way.

export function bindTour() {
  $("tour-skip").addEventListener("click", tourSkip);
  $("tour-back").addEventListener("click", () => tourGoto(tourStep - 1));
  $("tour-next").addEventListener("click", () => (tourStep === TOUR_LAST_STEP ? tourFinish() : tourGoto(tourStep + 1)));
}

// Step 5's second section: the settings decided on first launch, read-only except a plain
// switch flip. They are real settings with their own invoke commands, not feature flags,
// so each row carries its own live-write function.
export const TOUR_SETUP = [
  { key: "autostart", labelKey: "autostart", textKey: "tourSetAutostart",
    get: () => settings.autostart === true,
    setLive: (v) => tourSetSettingLive("set_autostart", "autostart", v) },
  { key: "minimizeOnClose", labelKey: "minimizeOnClose", textKey: "tourSetMinimize",
    get: () => settings.minimize_to_tray === true,
    setLive: (v) => tourSetSettingLive("set_minimize_on_close", "minimize_to_tray", v) },
  { key: "autoUpdate", labelKey: "autoUpdate", textKey: "tourSetUpdate",
    get: () => settings.auto_update !== false,
    setLive: (v) => tourSetSettingLive("set_auto_update", "auto_update", v) },
];

export let tourStep = 0;
export let tourFirstRun = false; // draft-buffered (true) vs. live-writing (false) step 5
export let tourDraft = {}; // flagKey -> bool, only meaningful while tourFirstRun is true
export let tourSetupDraft = {}; // TOUR_SETUP key -> bool, only meaningful while tourFirstRun is true
export let tourSetupInitial = {}; // snapshot of tourSetupDraft at open, to know what actually changed

// Shared by both step-5 modes' "write it now" path: identical to flagRow()'s own
// change handler (minus the promptSync confirmation, which none of these three need).
export async function tourSetFlagLive(key, enabled) {
  settings.ui_flags = { ...(settings.ui_flags || {}), [key]: enabled };
  try { await invoke("set_ui_flag", { key, enabled }); } catch (err) { errToast(err); }
  applyFlags();
}

// Live-write for a TOUR_SETUP row -- these are real settings fields (autostart,
// auto_update), each with its own invoke command, not ui_flags.
export async function tourSetSettingLive(cmd, field, enabled) {
  try { await invoke(cmd, { enabled }); settings[field] = enabled; } catch (err) { errToast(err); }
}

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

// Small "?" button next to a feature row, opening the in-app help at the article for the
// row's helpAnchor. Lives inside the row's <label>, so stopPropagation keeps the click
// from also toggling the switch.
export function tourHelpButton(anchor) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "icon-btn tour-help-btn";
  btn.title = t("tourHelpBtn");
  btn.setAttribute("aria-label", t("tourHelpBtn"));
  btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M8 1a5 5 0 0 0-1 9.9V12h2v-1.1A5 5 0 0 0 8 1zm0 12.5a1.1 1.1 0 1 0 0 2.2 1.1 1.1 0 0 0 0-2.2z"/></svg>';
  btn.addEventListener("click", async (e) => {
    e.preventDefault();
    e.stopPropagation();
    // Leaving the tour for the help keeps the choices made so far, like Finish: the
    // draft is written and the tour is marked seen, or it would reopen on the next launch.
    await tourFinish();
    openHelpAt(anchor);
  });
  return btn;
}

export function renderTourOptionsStep() {
  const wrap = document.createElement("div");
  wrap.className = "tour-options";
  for (const opt of TOUR_FEATURES) {
    const row = document.createElement("label");
    row.className = "field switch-field tour-opt-row";
    const label = document.createElement("span");
    label.className = "tour-opt-label";
    const name = document.createElement("b");
    name.textContent = t(opt.labelKey);
    const desc = document.createElement("span");
    desc.className = "tour-opt-desc";
    desc.textContent = t(opt.textKey);
    label.append(name, desc);
    const note = document.createElement("span");
    note.className = "tour-opt-default";
    note.textContent = shipsOn(opt.flagKey) ? t("tourDefaultOn") : t("tourDefaultOff");
    const input = document.createElement("input");
    input.type = "checkbox";
    input.className = "switch";
    input.checked = tourFirstRun ? tourDraft[opt.flagKey] : flagState(opt.flagKey);
    input.addEventListener("change", () => {
      if (tourFirstRun) tourDraft[opt.flagKey] = input.checked;
      else tourSetFlagLive(opt.flagKey, input.checked);
    });
    row.append(label, tourHelpButton(opt.helpAnchor), note, input);
    wrap.appendChild(row);
  }
  const restore = document.createElement("button");
  restore.type = "button";
  restore.className = "ghost-btn tour-restore-btn";
  restore.textContent = t("tourRestoreDefaults");
  restore.addEventListener("click", async () => {
    if (tourFirstRun) {
      tourDraft = Object.fromEntries(TOUR_FEATURES.map((o) => [o.flagKey, shipsOn(o.flagKey)]));
    } else {
      for (const opt of TOUR_FEATURES) await tourSetFlagLive(opt.flagKey, shipsOn(opt.flagKey));
    }
    renderTourStep(); // reflect the reset switches (and re-focus Next)
  });
  wrap.appendChild(restore);
  wrap.appendChild(renderTourSetupSection());
  const foot = document.createElement("p");
  foot.className = "hint tour-options-foot";
  foot.textContent = t("tourFooterNote");
  wrap.appendChild(foot);
  return wrap;
}

// The second section of step 5: the settings already decided on first launch.
// Autostart and the update check are plain switches sharing the feature rows' buffering;
// the data folder is read-only on purpose: moving it is a multi-step flow, out of place
// inside the tour.
export function renderTourSetupSection() {
  const section = document.createElement("div");
  section.className = "tour-setup";
  const heading = document.createElement("div");
  heading.className = "tour-setup-heading";
  heading.textContent = t("tourSetupHeading");
  section.appendChild(heading);
  // Autostart and the update check side by side: two switches in one row's height keep
  // step 5 within the measured min-height budget of the other steps.
  const pair = document.createElement("div");
  pair.className = "tour-setup-pair";
  for (const opt of TOUR_SETUP) {
    const row = document.createElement("label");
    row.className = "field switch-field tour-setup-row";
    const label = document.createElement("span");
    label.className = "tour-opt-label";
    const name = document.createElement("b");
    name.textContent = t(opt.labelKey);
    const desc = document.createElement("span");
    desc.className = "tour-opt-desc";
    desc.textContent = t(opt.textKey);
    label.append(name, desc);
    const input = document.createElement("input");
    input.type = "checkbox";
    input.className = "switch";
    input.checked = tourFirstRun ? tourSetupDraft[opt.key] : opt.get();
    input.addEventListener("change", () => {
      if (tourFirstRun) tourSetupDraft[opt.key] = input.checked;
      else opt.setLive(input.checked);
    });
    row.append(label, input);
    pair.appendChild(row);
  }
  section.appendChild(pair);
  const dataRow = document.createElement("div");
  dataRow.className = "field tour-setup-row tour-setup-datadir";
  const dataLabel = document.createElement("span");
  dataLabel.className = "tour-opt-label";
  const dataName = document.createElement("b");
  dataName.textContent = t("tourSetDataDir");
  const dataPath = document.createElement("span");
  dataPath.className = "tour-opt-desc";
  dataPath.textContent = currentDataDir;
  dataLabel.append(dataName, dataPath);
  const dataChange = document.createElement("button");
  dataChange.type = "button";
  dataChange.className = "ghost-btn tour-setup-change-btn";
  dataChange.textContent = t("tourSetDataDirChange");
  dataChange.addEventListener("click", async () => {
    await tourFinish();
    jumpToExpertSetting("dataDir", "paths");
  });
  dataRow.append(dataLabel, dataChange);
  section.appendChild(dataRow);
  return section;
}

export function renderTourStep() {
  const isLast = tourStep === TOUR_LAST_STEP;
  const titleKey = tourStep < TOUR_STEPS.length ? TOUR_STEPS[tourStep].titleKey : "tourStep5Title";
  $("tour-title").textContent = t(titleKey);
  const body = $("tour-body");
  body.innerHTML = "";
  body.appendChild(tourStep < TOUR_STEPS.length ? renderTourDrawingStep(TOUR_STEPS[tourStep]) : renderTourOptionsStep());
  const dots = $("tour-dots");
  dots.innerHTML = "";
  for (let i = 0; i < TOUR_STEP_COUNT; i++) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "tour-dot" + (i === tourStep ? " active" : "");
    b.textContent = i === tourStep ? "●" : "○";
    b.setAttribute("aria-label", t(i < TOUR_STEPS.length ? TOUR_STEPS[i].titleKey : "tourStep5Title"));
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
    // everything else (e.g. focus left on a step-5 switch).
    e.preventDefault();
    tourStep === TOUR_LAST_STEP ? tourFinish() : tourGoto(tourStep + 1);
  }
}

// The dialog must not visibly resize while paging through the five steps; a fixed height
// is either too tall for the shortest step or too short for the tallest. So every step is
// measured once, right after the modal becomes visible (a hidden ancestor reports 0
// height), and the tallest is kept.
export function tourMeasureMaxStepHeight() {
  const body = $("tour-body");
  let max = 0;
  for (let i = 0; i < TOUR_STEP_COUNT; i++) {
    const wrap = i < TOUR_STEPS.length ? renderTourDrawingStep(TOUR_STEPS[i]) : renderTourOptionsStep();
    body.innerHTML = "";
    body.appendChild(wrap);
    // Layout height, not the painted rect: the open animation scales the dialog while it
    // is measured.
    max = Math.max(max, wrap.offsetHeight);
  }
  return max;
}

export async function openTour({ firstRun }) {
  tourFirstRun = firstRun;
  tourStep = 0;
  tourDraft = firstRun ? Object.fromEntries(TOUR_FEATURES.map((o) => [o.flagKey, shipsOn(o.flagKey)])) : {};
  tourSetupDraft = firstRun ? Object.fromEntries(TOUR_SETUP.map((o) => [o.key, o.get()])) : {};
  tourSetupInitial = firstRun ? { ...tourSetupDraft } : {};
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
  const firstRun = tourFirstRun;
  await tourSkip();
  undoToast(t("tourSkippedToast"), () => openTour({ firstRun }));
}

// Only the switches that differ from the shipped default get written: an
// untouched draft leaves ui_flags exactly as it was, so a later default change
// still reaches this install. The setup rows have no shipped default to compare
// against (autostart/auto_update are real settings): they compare against
// whatever they actually were when the tour opened instead.
export async function tourFinishDraft() {
  for (const opt of TOUR_FEATURES) {
    const shipped = shipsOn(opt.flagKey);
    if (tourDraft[opt.flagKey] !== shipped) await tourSetFlagLive(opt.flagKey, tourDraft[opt.flagKey]);
  }
  for (const opt of TOUR_SETUP) {
    if (tourSetupDraft[opt.key] !== tourSetupInitial[opt.key]) await opt.setLive(tourSetupDraft[opt.key]);
  }
}

export async function tourFinish() {
  if (tourFirstRun) await tourFinishDraft();
  closeTour();
  await saveUiState("tourSeen", true);
}

export function wireTour() {
  EXPERT_ACTIONS["tour-open"] = () => openTour({ firstRun: false });
}
