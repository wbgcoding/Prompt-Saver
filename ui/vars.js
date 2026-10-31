// Standalone placeholder prompt. Opened by the backend for a copy that started
// OUTSIDE the app (a prompt's global hotkey, a tray quick copy) so nothing but this
// small window comes forward. It renders the rows the main window asked for and
// hands the answers straight back; every rule about what a variable MEANS stays in
// promptVars.js. Esc, the Cancel button and losing focus all mean "cancelled".
import { applyFloatTheme } from "./floattheme.js";
import { APP_NAME } from "./brand.js";
import { compactTexts } from "./i18n.js";

// Frameless window: the title is invisible in the app but shows in the Alt-Tab
// switcher, so it comes from the brand rather than a hardcoded English word.
document.title = APP_NAME;

const { invoke } = window.__TAURI__.core;

// Pin glyph for the fixed-value marker; this window has no shared icon module, so it is a
// one-off inline SVG.
const FIXED_PIN_SVG =
  '<svg viewBox="0 0 24 24" width="12" height="12" aria-hidden="true"><path fill="currentColor" d="M12 2a7 7 0 0 0-7 7c0 5.25 7 13 7 13s7-7.75 7-13a7 7 0 0 0-7-7Zm0 9.5A2.5 2.5 0 1 1 12 6.5a2.5 2.5 0 0 1 0 5Z"/></svg>';

const cardEl = document.getElementById("card");
const titleEl = document.getElementById("title");
const fieldsEl = document.getElementById("fields");
const okEl = document.getElementById("ok");
const cancelEl = document.getElementById("cancel");
const noteEl = document.getElementById("note");
let tooLongText = "";

let rows = [];
let answered = false;

// One answer, ever: the backend closes this window on the first call, and a blur
// racing the click must not send a cancel after a confirm.
function answer(values) {
  if (answered) return;
  answered = true;
  // If the call itself fails (backend not ready, IPC hiccup), un-stick the guard:
  // otherwise this window can never answer again and just sits there forever.
  invoke("var_window_result", { values }).catch((e) => {
    answered = false;
    // The backend refuses an oversized answer; say so instead of sitting silent.
    if (String(e).includes("bad values") && tooLongText) {
      noteEl.textContent = tooLongText;
      noteEl.classList.remove("hidden");
      fitWindow();
    }
  });
}

const confirm = () =>
  answer(JSON.stringify({
    values: rows.map((r) => r.value()),
    remember: rows.map((r) => r.remember()),
  }));

// Choice variables get a listbox built in this window instead of a native <select>:
// an OS dropdown is its own window, and opening one would blur this one, which is
// exactly the gesture that cancels.
function buildChoice(row, options, initial) {
  let value = options.includes(initial) ? initial : options[0];
  const wrap = document.createElement("div");
  wrap.className = "vars-choice";
  const button = document.createElement("button");
  button.type = "button";
  button.className = "vars-input vars-choice-btn";
  button.textContent = value;
  const list = document.createElement("div");
  list.className = "vars-choice-list hidden";
  for (const option of options) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "vars-choice-item";
    item.textContent = option;
    item.addEventListener("click", () => {
      value = option;
      button.textContent = option;
      list.classList.add("hidden");
      button.focus(); // the focused item is hidden with the list: hand the focus back
    });
    list.appendChild(item);
  }
  button.addEventListener("click", () => {
    if (options.length > 1) list.classList.toggle("hidden");
  });
  wrap.append(button, list);
  row.appendChild(wrap);
  return { el: button, focus: () => button.focus(), value: () => value };
}

function buildText(row, initial, placeholder) {
  const input = document.createElement("input");
  input.type = "text";
  input.className = "vars-input";
  input.value = initial;
  // A fixed value is only SUGGESTED, as the placeholder: an untouched field stays empty.
  if (placeholder) input.placeholder = placeholder;
  row.appendChild(input);
  return { el: input, focus: () => { input.focus(); input.select(); }, value: () => input.value };
}

function render(data) {
  applyFloatTheme(data.theme, data.vars);
  document.documentElement.lang = data.lang || "en";
  const T = compactTexts(data.lang || "en");
  tooLongText = T.varsTooLong;
  titleEl.textContent = T.varsTitle;
  okEl.textContent = T.varsCopy;
  cancelEl.textContent = T.cancel;
  fieldsEl.innerHTML = "";
  rows = (data.rows || []).map((spec, idx) => {
    // A plain block: the name is the field's label, and the remember switch keeps its
    // own label beside it instead of nesting inside the name.
    const row = document.createElement("div");
    row.className = "vars-row";
    const nameRow = document.createElement("span");
    nameRow.className = "vars-name-row";
    const name = document.createElement("label");
    name.className = "vars-name";
    name.id = `vars-name-${idx}`;
    name.textContent = spec.name; // textContent: a variable name is never markup
    nameRow.appendChild(name);
    if (spec.fixed) {
      const marker = document.createElement("span");
      marker.className = "vars-fixed-marker";
      marker.title = T.fixedValueHint;
      marker.innerHTML = FIXED_PIN_SVG;
      nameRow.appendChild(marker);
    }
    row.appendChild(nameRow);
    const control = spec.options && spec.options.length
      ? buildChoice(row, spec.options, spec.value)
      : buildText(row, spec.value || "", spec.placeholder || "");
    control.el.id = `vars-field-${idx}`;
    name.htmlFor = control.el.id;
    // A choice button shows its current pick, so it is named by the variable and the pick.
    if (spec.options && spec.options.length) control.el.setAttribute("aria-labelledby", `${name.id} ${control.el.id}`);
    let box = null;
    // The remember switch belongs to free-text rows only: a choice keeps its last
    // pick through its own setting, with no checkbox to tick.
    if (data.askRemember && !(spec.options && spec.options.length)) {
      const keep = document.createElement("label");
      keep.className = "vars-remember";
      box = document.createElement("input");
      box.type = "checkbox";
      box.checked = !!spec.remember;
      const text = document.createElement("span");
      text.textContent = T.rememberValue;
      keep.append(box, text);
      row.appendChild(keep);
    }
    fieldsEl.appendChild(row);
    return { ...control, remember: () => !!box?.checked };
  });
  rows[0]?.focus();
  fitWindow();
}

// The backend sized this window from a per-row guess. Measure what the form really
// needs (everything outside the field list plus the list's full content) and ask for
// exactly that, so a short form shows no scrollbar and a long one only scrolls once
// the screen is full. The width follows the content too (the backend keeps it between
// its own floor and the opening width), so two short fields get a narrow window.
function fitWindow() {
  const first = fieldsEl.firstElementChild;
  const last = fieldsEl.lastElementChild;
  const content = first && last
    ? last.getBoundingClientRect().bottom - first.getBoundingClientRect().top
    : 0;
  const needed = Math.ceil(window.innerHeight - fieldsEl.clientHeight + content) + 2;
  document.body.classList.add("vars-measure");
  const pad = parseFloat(getComputedStyle(document.body).paddingLeft) || 0;
  const width = Math.ceil(cardEl.getBoundingClientRect().width + 2 * pad);
  document.body.classList.remove("vars-measure");
  invoke("var_window_fit", { height: needed, width }).catch(() => {});
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    e.preventDefault();
    // An open choice list closes first; only a second Escape cancels the form.
    const openList = document.querySelector(".vars-choice-list:not(.hidden)");
    if (openList) {
      openList.classList.add("hidden");
      openList.parentElement?.querySelector("button")?.focus();
      return;
    }
    answer(null);
  }
  // Enter inside a choice control (the collapsed button, or a focused item in the open
  // list) must reach that control's own activation (open the list, or pick the item)
  // instead of confirming with the earlier value. A single-option choice has no list to
  // open (buildChoice only toggles past options.length > 1), so Enter falls through to
  // confirm() there like any other row.
  else if (e.key === "Enter") {
    // Cancel keeps its native Enter activation (-> answer(null)): confirming from Cancel
    // would confirm the form a keyboard user tried to dismiss.
    if (e.target === cancelEl) return;
    const choice = e.target.closest(".vars-choice");
    const hasChoices = choice && choice.querySelectorAll(".vars-choice-item").length > 1;
    if (!hasChoices) { e.preventDefault(); confirm(); }
  }
});
okEl.addEventListener("click", confirm);
cancelEl.addEventListener("click", () => answer(null));
// Clicking away means "not now": the window never lingers over another app. The
// grace period covers the hand-off: the backend shows the window and only then takes
// the focus, and a blur fired in between would cancel before anything was on screen.
const OPENED_AT = Date.now();
const BLUR_GRACE_MS = 400;
window.addEventListener("blur", () => {
  const left = BLUR_GRACE_MS - (Date.now() - OPENED_AT);
  if (left <= 0) answer(null);
  // A blur inside the hand-off window is deferred, not dropped: close once the
  // grace ends unless this window has the focus again by then.
  else setTimeout(() => { if (!document.hasFocus()) answer(null); }, left);
});
// A click on the transparent margin around the card is an outside click too.
document.body.addEventListener("mousedown", (e) => {
  if (!cardEl.contains(e.target)) answer(null);
});
// A click elsewhere in the card (a different row, say) closes any other open choice list;
// otherwise only picking an item or clicking its own button would.
cardEl.addEventListener("mousedown", (e) => {
  for (const list of cardEl.querySelectorAll(".vars-choice-list:not(.hidden)")) {
    if (!list.parentElement.contains(e.target)) list.classList.add("hidden");
  }
});

// Pull the request only once this window can render it: pushing it would race the
// webview's first frame.
invoke("var_window_payload")
  .then((raw) => {
    const data = raw ? JSON.parse(raw) : null;
    if (data) render(data);
    else answer(null);
  })
  .catch(() => answer(null));
