// The one themed confirm/info dialog, shared by the main window and the video
// editor. A window using it needs the confirm-modal markup (see index.html) and
// one initDialogs() call before anything can open a dialog.
const $ = (id) => document.getElementById(id);
let t = (key) => key;
let toast = () => {};

export function initDialogs({ t: translate, toast: showToast }) {
  t = translate;
  toast = showToast;
}

const COPY_SVG = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><rect x="9" y="9" width="11" height="11" rx="2" fill="none" stroke="currentColor" stroke-width="2"/><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" d="M5 15V6a2 2 0 0 1 2-2h9"/></svg>';
export const EYE_SVG ='<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M2 12s3.6-6 10-6 10 6 10 6-3.6 6-10 6-10-6-10-6Z"/><circle cx="12" cy="12" r="2.6" fill="none" stroke="currentColor" stroke-width="2"/></svg>';

// Themed yes/no dialog. Resolves true on confirm, false on cancel / dismiss.
// `icon` puts an SVG above the title, `input` turns it into a one-field prompt
// (then it resolves to the entered string, or null when dismissed).
let confirmCleanup = null;
// `checkboxes`: [{key, label, checked}]; rendered into #confirm-checkbox-row,
// read back by the caller via document.getElementById(`confirm-cb-${key}`) once
// the promise resolves true (the return value itself stays plain bool/string, so
// no existing `if (!ok) return` caller needs to change). Default requires at
// least one to stay checked (the confirm button would otherwise offer nothing
// to actually do) -- pass `checkboxesOptional: true` for a plain standalone
// toggle (e.g. "don't show this again") that must never block either button.
// `altLabel` adds a third choice between Cancel and the confirm button and
// resolves to the string "alt": a question with two different ways to go ahead
// (delete the old sync folder, keep it) plus the way out. Left out, the dialog is
// exactly the two-button one every other caller gets.
export function confirmDialog({ title, message, confirmLabel, cancelLabel, altLabel, icon, input, checkboxes, checkboxesOptional = false, danger = true, oneButton = false }) {
  // Restored once this dialog resolves, so a keyboard user's next Tab
  // continues from where they were instead of restarting from the page top.
  const opener = document.activeElement;
  const root = $("confirm-modal");
  $("confirm-title").textContent = title;
  $("confirm-msg").textContent = message;
  const iconEl = $("confirm-icon");
  iconEl.innerHTML = icon || "";
  iconEl.classList.toggle("hidden", !icon);
  const row = $("confirm-input-row");
  const field = $("confirm-input");
  const eye = $("confirm-input-eye");
  row.classList.toggle("hidden", !input);
  const ok = $("confirm-ok");
  const cancel = $("confirm-cancel");
  // Part of THIS call's own render, not a side effect toggled by the caller
  // around await confirmDialog(...): a second, unrelated dialog opening while
  // this one is still pending must never flip the wrong dialog's cancel button.
  const alt = $("confirm-alt");
  cancel.classList.toggle("hidden", oneButton);
  alt.classList.toggle("hidden", !altLabel);
  alt.textContent = altLabel || "";
  ok.classList.toggle("danger-confirm", danger);
  ok.textContent = confirmLabel;
  cancel.textContent = cancelLabel || t("cancel");
  if (input) {
    // Defaults to a masked field; `type: "text"` makes it a plain one-line prompt
    // (renaming a tag), where the reveal eye has nothing to reveal.
    field.type = input.type || "password";
    field.value = input.value || "";
    field.placeholder = input.placeholder || "";
    eye.innerHTML = input.copy ? COPY_SVG : EYE_SVG;
    eye.title = input.copy ? t("copy") : "";
    eye.setAttribute("aria-label", input.copy ? t("copy") : t("showPassword"));
    eye.classList.toggle("hidden", field.type !== "password" && !input.copy);
    // Native browser completion from the given list (e.g. existing tag names) --
    // only wired when the caller actually passes one, so every other prompt keeps
    // its plain input.
    const datalist = $("confirm-input-datalist");
    datalist.innerHTML = "";
    for (const value of input.datalist || []) {
      const opt = document.createElement("option");
      opt.value = value;
      datalist.appendChild(opt);
    }
  }
  const cbRow = $("confirm-checkbox-row");
  cbRow.innerHTML = "";
  cbRow.classList.toggle("hidden", !checkboxes);
  const syncOk = () => { ok.disabled = !checkboxesOptional && !!checkboxes && !cbRow.querySelector("input:checked"); };
  if (checkboxes) {
    for (const cb of checkboxes) {
      const label = document.createElement("label");
      label.className = "confirm-checkbox";
      const box = document.createElement("input");
      box.type = "checkbox";
      box.id = `confirm-cb-${cb.key}`;
      box.checked = cb.checked !== false;
      box.addEventListener("change", syncOk);
      label.append(box, document.createTextNode(cb.label));
      cbRow.appendChild(label);
    }
  }
  // Resolve any stale dialog BEFORE syncOk(), not after -- its done() resets
  // ok.disabled = false unconditionally, which otherwise clobbered a fresh
  // checkbox dialog's correct disabled-with-nothing-checked state.
  confirmCleanup?.(false);
  syncOk();
  root.classList.remove("hidden");
  return new Promise((resolve) => {
    const done = (val) => {
      root.classList.add("hidden");
      ok.disabled = false;
      ok.removeEventListener("click", onOk);
      cancel.removeEventListener("click", onCancel);
      alt.removeEventListener("click", onAlt);
      root.removeEventListener("pointerdown", onBg);
      field.removeEventListener("keydown", onKey);
      eye.removeEventListener("click", onEye);
      confirmCleanup = null;
      opener?.focus?.();
      resolve(val);
    };
    const finish = (okPressed) => done(input ? (okPressed ? field.value : null) : okPressed);
    const onOk = () => finish(true);
    const onCancel = () => finish(false);
    // The third choice is its own answer, never true and never the cancel answer.
    const onAlt = () => done("alt");
    const onBg = (e) => { if (e.target === root) finish(false); };
    const onKey = (e) => { if (e.key === "Enter" && !ok.disabled) { e.preventDefault(); finish(true); } };
    const onEye = () => {
      if (input && input.copy) {
        field.select();
        const value = field.value;
        Promise.resolve(navigator.clipboard?.writeText(value)).then(() => toast(t("copied"))).catch(() => {
          try { document.execCommand("copy"); toast(t("copied")); } catch (e) { /* nothing more to try */ }
        });
        return;
      }
      field.type = field.type === "password" ? "text" : "password";
    };
    ok.addEventListener("click", onOk);
    cancel.addEventListener("click", onCancel);
    alt.addEventListener("click", onAlt);
    root.addEventListener("pointerdown", onBg);
    field.addEventListener("keydown", onKey);
    eye.addEventListener("click", onEye);
    // Escape and the stale-dialog sweep both come through here, so they must map
    // their answer exactly like Cancel does: a prompt with an input field owes its
    // caller null, never false.
    confirmCleanup = finish;
    (input ? field : ok).focus();
    if (input && field.value) field.select();
  });
}

// Same dialog with nothing to decide: one button, used to report a result.
export async function infoDialog(title, message, icon) {
  await confirmDialog({ title, message, confirmLabel: t("close"), icon, danger: false, oneButton: true });
}

// Closes an open dialog the way Cancel does. Returns whether one was open, so
// an Escape handler knows the key was consumed.
export function dismissConfirm() {
  if (!confirmCleanup) return false;
  confirmCleanup(false);
  return true;
}
