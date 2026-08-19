// Standalone placeholder prompt. Opened by the backend for a copy that started
// OUTSIDE the app (a prompt's global hotkey, a tray quick copy) so nothing but this
// small window comes forward. It renders the rows the main window asked for and
// hands the answers straight back; every rule about what a variable MEANS stays in
// main.js. Esc, the Cancel button and losing focus all mean "cancelled".
import { applyFloatTheme } from "./floattheme.js";
import { APP_NAME } from "./brand.js";

// Frameless window: the title is invisible in the app but shows in the Alt-Tab
// switcher, so it comes from the brand rather than a hardcoded English word.
document.title = APP_NAME;

const { invoke } = window.__TAURI__.core;

const cardEl = document.getElementById("card");
const titleEl = document.getElementById("title");
const fieldsEl = document.getElementById("fields");
const okEl = document.getElementById("ok");
const cancelEl = document.getElementById("cancel");

let rows = [];
let answered = false;

// One answer, ever: the backend closes this window on the first call, and a blur
// racing the click must not send a cancel after a confirm.
function answer(values) {
  if (answered) return;
  answered = true;
  // If the call itself fails (backend not ready, IPC hiccup), un-stick the guard —
  // otherwise this window can never answer again and just sits there forever.
  invoke("var_window_result", { values }).catch(() => { answered = false; });
}

const confirm = () =>
  answer(JSON.stringify({
    values: rows.map((r) => r.value()),
    remember: rows.map((r) => r.remember()),
  }));

// Choice variables get a listbox built in this window instead of a native <select>:
// an OS dropdown is its own window, and opening one would blur this one — which is
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
    });
    list.appendChild(item);
  }
  button.addEventListener("click", () => {
    if (options.length > 1) list.classList.toggle("hidden");
  });
  wrap.append(button, list);
  row.appendChild(wrap);
  return { focus: () => button.focus(), value: () => value };
}

function buildText(row, initial) {
  const input = document.createElement("input");
  input.type = "text";
  input.className = "vars-input";
  input.value = initial;
  row.appendChild(input);
  return { focus: () => { input.focus(); input.select(); }, value: () => input.value };
}

function render(data) {
  applyFloatTheme(data.theme, data.vars);
  titleEl.textContent = data.title || "";
  okEl.textContent = data.ok || "";
  cancelEl.textContent = data.cancel || "";
  fieldsEl.innerHTML = "";
  rows = (data.rows || []).map((spec) => {
    const row = document.createElement("label");
    row.className = "vars-row";
    const name = document.createElement("span");
    name.className = "vars-name";
    name.textContent = spec.name; // textContent: a variable name is never markup
    row.appendChild(name);
    const control = spec.options && spec.options.length
      ? buildChoice(row, spec.options, spec.value)
      : buildText(row, spec.value || "");
    let box = null;
    // The remember switch belongs to free-text rows only — a choice keeps its last
    // pick through its own setting, with no checkbox to tick.
    if (data.askRemember && !(spec.options && spec.options.length)) {
      const keep = document.createElement("label");
      keep.className = "vars-remember";
      box = document.createElement("input");
      box.type = "checkbox";
      box.checked = !!spec.remember;
      const text = document.createElement("span");
      text.textContent = data.rememberLabel || "";
      keep.append(box, text);
      row.appendChild(keep);
    }
    fieldsEl.appendChild(row);
    return { ...control, remember: () => !!box?.checked };
  });
  rows[0]?.focus();
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") { e.preventDefault(); answer(null); }
  // Enter inside a choice control (the collapsed button, or a focused item in
  // the open list) must reach that control's OWN native activation (open the list,
  // or pick the focused item) instead of being hijacked into an immediate confirm
  // with whatever value was already selected before. A single-option choice has no
  // list to open at all (buildChoice only toggles it past options.length > 1) — its
  // button's native Enter activation is then a no-op, so Enter must fall through to
  // confirm() there same as any other row, or it does nothing at all.
  else if (e.key === "Enter") {
    // Cancel already gets its own native Enter activation (-> answer(null)) --
    // hijacking that into confirm() when Cancel has focus would confirm the form a
    // keyboard user just tried to dismiss.
    if (e.target === cancelEl) return;
    const choice = e.target.closest(".vars-choice");
    const hasChoices = choice && choice.querySelectorAll(".vars-choice-item").length > 1;
    if (!hasChoices) { e.preventDefault(); confirm(); }
  }
});
okEl.addEventListener("click", confirm);
cancelEl.addEventListener("click", () => answer(null));
// Clicking away means "not now" — the window never lingers over another app. The
// grace period covers the hand-off: the backend shows the window and only then takes
// the focus, and a blur fired in between would cancel before anything was on screen.
const OPENED_AT = Date.now();
const BLUR_GRACE_MS = 400;
window.addEventListener("blur", () => {
  if (Date.now() - OPENED_AT > BLUR_GRACE_MS) answer(null);
});
// A click on the transparent margin around the card is an outside click too.
document.body.addEventListener("mousedown", (e) => {
  if (!cardEl.contains(e.target)) answer(null);
});
// A click elsewhere in the card (a different row, say) left any other open
// choice list standing open — nothing ever closed one except picking an item or
// clicking its own button again.
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
