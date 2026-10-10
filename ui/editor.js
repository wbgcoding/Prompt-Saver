// Prompt editor modal: drafts, variable helpers, diff and history, file preview and dialogs.
import { $, assignModalState, autoGrow, closeColorPop, COLORS, convertFileSrc, disarmButton, errToast, flag, HISTORY_PREVIEW_LEN, ICON_DOWN, ICON_FILE, ICON_PDF, ICON_TRASH, ICON_UP, inputEl, invoke, LANG, libraryEl, listen, modal, modalState, openColorPop, optFlag, PDF_EXT, prompts, putIcon, saveBtn, t, toast, txt, val } from "./core.js";
import { DYN_VARS, dynToken, extractVars, hasEmptyVarToken, readFixedVars, resolveVars, saveFixedVars } from "./promptVars.js";
import { ATTACH_PREVIEW_KB } from "./expertConfig.js";
import { EXPERT_DROPDOWNS, renderExpert } from "./expertMenu.js";
import { normSize } from "./settingsPanel.js";
import { closeTagPop, closeTagSuggest, renderTagChips } from "./tags.js";
import { bindHotkeyField, HOTKEY_ERRORS, hotkeyMessage, promptHotkeysOn, renderPromptHotkey, setPromptHotkey, showHotkeyError } from "./hotkeys.js";
import { confirmDialog } from "./dialog.js";
import { renderGrid } from "./views.js";
import { hasMarkdownSyntax, renderMarkdown } from "./markdown.js";
import { focusTile } from "./copyFlow.js";
import { IMAGE_EXT, mediaKind } from "./media.js";
import { stopVoice, voiceState, voiceTargetEl } from "./dictation.js";
import { applyAutoBadge, applyRotateState, autoActiveIds, placeTile, pollMissingFiles, rotateReqs, rotateState } from "./tiles.js";
import { refreshLastEdited, renderLibrary, trashPromptWithUndo } from "./library.js";
import { maybeDupImportToast, maybeShowSimilarHint } from "./management.js";
import { openAllShortcuts, openLightbox } from "./windowChrome.js";
import { attachSelectPicker } from "./selectpop.js";

// Bumped by openModal/closeModal. An async pick/preview started for one editor
// session (pickReplacementMedia, replaceFile) captures this before its awaits and
// checks it again after -- `modalState` alone is not enough, since closing and
// opening a DIFFERENT prompt leaves modalState just as truthy, letting a slow
// reply from the old prompt land in the new one's editor.
export let modalSession = 0;
// Whatever had keyboard focus right before the editor opened, so closing
// it returns focus there instead of dropping it to document.body -- without
// this a keyboard user's next Tab press restarts from the top of the page.
export let modalOpener = null;
export let modalInitial = ""; // snapshot of the open modal's content (unsaved-change guard)
export let modalSaving = false; // re-entry guard, a double-click/double-Enter must not fire two saves
export let draftTimer = null; // editor-draft autosave tick, armed by openModal, disarmed by closeModal
export let draftLastSaved = null; // fingerprint of the last written draft, so an unchanged tick skips the write

// ---- Modal ----
// Reusable palette: "no color" + free picker + presets, into any container.
// onPick(hex) for none/preset; onCustom(anchor, current) opens the free picker.
// opts.compact (editor only) shortens the row to none, the colours in use (opts.used,
// already ordered), the custom swatch only while a custom colour is selected, and a
// More colors button that shows the full set incl. the free picker.
export function buildSwatches(container, selected, onPick, onCustom, opts = {}) {
  const compact = !!opts.compact;
  container.innerHTML = "";
  container.classList.toggle("swatches-compact", compact);
  const isCustom = !!selected && !COLORS.includes(selected);

  const mkSwatch = (cls, bg) => {
    const sw = document.createElement("button");
    sw.type = "button";
    sw.className = `swatch ${cls}`.trim();
    if (bg) sw.style.background = bg;
    container.appendChild(sw);
    return sw;
  };

  const none = mkSwatch("none" + (selected === "" ? " sel" : ""));
  none.title = t("colorNone");
  none.setAttribute("aria-label", t("colorNone"));
  none.addEventListener("click", () => onPick(""));

  if (!compact || isCustom) {
    const custom = mkSwatch("custom" + (isCustom ? " sel" : ""), isCustom ? selected : "");
    custom.title = t("colorCustom");
    custom.setAttribute("aria-label", t("colorCustom"));
    custom.addEventListener("click", () => onCustom(custom, isCustom ? selected : ""));
  }

  let hues = COLORS.slice(1);
  if (compact) {
    hues = (opts.used || []).filter((c) => hues.includes(c));
    // The current choice stays visible even when no other prompt uses it.
    if (selected && COLORS.includes(selected) && !hues.includes(selected)) hues.push(selected);
  }
  for (const c of hues) {
    const sw = mkSwatch(c === selected ? "sel" : "", c);
    sw.title = c;
    sw.setAttribute("aria-label", c);
    sw.addEventListener("click", () => onPick(c));
  }
  if (compact) {
    const more = mkSwatch("more");
    more.textContent = "…";
    more.title = t("colorMore");
    more.setAttribute("aria-label", t("colorMore"));
    more.addEventListener("click", () => {
      if (opts.onMore) opts.onMore();
      else buildSwatches(container, selected, onPick, onCustom);
    });
  }
}

// The editor shows a short colour row first. Reset by openModal, set by "More colors".
let colorsExpanded = false;
// Up to eight palette colours in use, most used first, ties in palette order; with
// none in use the first eight of the palette.
function colorsInUse() {
  const hues = COLORS.slice(1);
  const count = new Map();
  for (const p of prompts) if (hues.includes(p.color)) count.set(p.color, (count.get(p.color) || 0) + 1);
  const ranked = hues.filter((c) => count.has(c)).sort((a, b) => count.get(b) - count.get(a) || hues.indexOf(a) - hues.indexOf(b));
  return (ranked.length ? ranked : hues).slice(0, 8);
}

export function renderSwatches(selected) {
  buildSwatches(
    $("color-row"),
    selected,
    (hex) => { modalState.color = hex; renderSwatches(hex); },
    (anchor, cur) => openColorPop(anchor, cur, (hex) => {
      if (!modalState) return;
      modalState.color = hex;
      renderSwatches(hex);
    }),
    {
      compact: !colorsExpanded,
      used: colorsInUse(),
      onMore: () => { colorsExpanded = true; renderSwatches(selected); },
    }
  );
}

// Live caption overlay on the modal media preview: mirrors the on-tile render
// (0 = default size, 1 = auto-scale, else fixed px) so the user sees it as-is.
export function updateCaptionPreview() {
  const el = modal.captionPreview;
  if (!el) return;
  const text = modal.caption.value.trim();
  const size = Number(modal.captionSize.value) || 0;
  el.textContent = text;
  el.classList.toggle("hidden", !text);
  el.classList.toggle("auto", size === 1);
  el.style.fontSize = size > 1 ? `${size}px` : "";
}

// Serialized content of the open modal: compared on dismiss to detect edits.
export function modalSnapshot() {
  if (!modalState) return "";
  return JSON.stringify([
    modal.name.value, modal.text.value, modal.caption.value, modal.captionSize.value,
    modal.fontSel.value, modal.sizeSel.value, modalState.color || "", modalState.image || "",
    !!modalState.showImage, modalState.filePath || "", modalState.iconPath || "", modal.fav.classList.contains("active"),
    modalState.tags || [], modal.notesText.value, modalState.hotkey || "", modalState.hotkeyPaste || "",
    // The entry list + mode are edited entirely in memory until Save.
    modalState.rotateEntries || [], modalState.rotateMode || "", !!modalState.rotateAuto,
    modalState.pdfPage || 0,
  ]);
}

// ---- Editor variable helper ----
// One-line explanation per reserved name, shown as the chip's tooltip.
export const DYN_TIPS = {
  date: "dynDate", time: "dynTime", datetime: "dynDatetime",
  weekday: "dynWeekday", yesterday: "dynYesterday", tomorrow: "dynTomorrow",
  week: "dynWeek", month: "dynMonth", year: "dynYear",
  timestamp: "dynTimestamp", epoch: "dynEpoch",
  clipboard: "dynClipboard", username: "dynUsername",
  uuid: "dynUuid", random: "dynRandom", counter: "dynCounter",
};
// A name or option must not carry the fence characters, or the token breaks.
export const cleanVarPart = (s) => s.replace(/[|{}\n]/g, "").trim();

// Write a placeholder at the caret of the prompt textarea.
export function insertVarToken(body) {
  const ta = modal.text;
  const start = ta.selectionStart ?? ta.value.length;
  const end = ta.selectionEnd ?? start;
  const token = `{?${body}}`;
  ta.value = ta.value.slice(0, start) + token + ta.value.slice(end);
  ta.focus();
  ta.setSelectionRange(start + token.length, start + token.length);
  ta.dispatchEvent(new Event("input", { bubbles: true })); // length counter + clear-X
}

// Rebuild the helper for the current feature flags (chips only with promptVars,
// the options field only with choiceVars).
export function renderVarHelper() {
  const chips = $("modal-vars-chips");
  const dyn = flag("dynVars");
  chips.classList.toggle("hidden", !dyn);
  $("modal-vars-dyn-hint").classList.toggle("hidden", !dyn);
  chips.innerHTML = "";
  if (dyn) {
    for (const name of DYN_VARS) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "vars-chip";
      b.textContent = dynToken(name);
      b.title = t(DYN_TIPS[name]); // hover engine themes any [title]
      b.addEventListener("click", () => insertVarToken(dynToken(name)));
      chips.appendChild(b);
    }
  }
  const choice = flag("choiceVars");
  $("modal-var-opts").classList.toggle("hidden", !choice);
  $("modal-vars-choice-hint").classList.toggle("hidden", !choice);
}

export function insertHelperVar() {
  const nameEl = $("modal-var-name");
  const optsEl = $("modal-var-opts");
  const name = cleanVarPart(nameEl.value);
  if (!name) { nameEl.focus(); return; }
  const opts = flag("choiceVars")
    ? optsEl.value.split(",").map(cleanVarPart).filter(Boolean)
    : [];
  insertVarToken(opts.length ? `${name}|${opts.join(",")}` : name);
  nameEl.value = "";
  optsEl.value = "";
}

// The expert menu's own list editor for varFixed: one row per fixed value
// (name, its value, a delete button) plus an add row. Empty by default.
export function varFixedListRow() {
  const wrap = document.createElement("div");
  wrap.className = "field var-fixed-field";
  // The switch above only carries the on/off label -- this list is its own
  // widget, so it gets its own small heading rather than relying on that.
  const heading = document.createElement("div");
  heading.className = "var-fixed-heading";
  heading.textContent = t("expGroupVarFixed");
  wrap.appendChild(heading);
  const fixed = readFixedVars();
  const names = Object.keys(fixed).sort((a, b) => a.localeCompare(b));
  const list = document.createElement("div");
  list.className = "var-fixed-list";
  for (const name of names) {
    const row = document.createElement("div");
    row.className = "var-fixed-row";
    const nameEl = document.createElement("span");
    nameEl.className = "var-fixed-name";
    nameEl.textContent = name; // textContent: a variable name is never markup
    const valueEl = document.createElement("input");
    valueEl.type = "text";
    valueEl.className = "modal-input var-fixed-value";
    valueEl.value = fixed[name];
    valueEl.addEventListener("change", () => {
      const next = { ...readFixedVars() };
      const v = valueEl.value.trim();
      if (v) next[name] = v; else delete next[name]; // cleared value = removed row
      saveFixedVars(next);
      renderExpert();
    });
    const del = document.createElement("button");
    del.type = "button";
    del.className = "icon-btn var-fixed-del";
    del.title = t("varFixedRemove");
    del.setAttribute("aria-label", t("varFixedRemove"));
    putIcon(del, ICON_TRASH);
    del.addEventListener("click", () => {
      const next = { ...readFixedVars() };
      delete next[name];
      saveFixedVars(next);
      renderExpert();
    });
    row.append(nameEl, valueEl, del);
    list.appendChild(row);
  }
  wrap.appendChild(list);
  if (!names.length) {
    const hint = document.createElement("div");
    hint.className = "hint var-fixed-empty";
    hint.textContent = t("varFixedEmpty");
    wrap.appendChild(hint);
  }
  const addRow = document.createElement("div");
  addRow.className = "var-fixed-add";
  const addName = document.createElement("input");
  addName.type = "text";
  addName.className = "modal-input var-fixed-add-name";
  addName.placeholder = t("varFixedNamePh");
  const addValue = document.createElement("input");
  addValue.type = "text";
  addValue.className = "modal-input var-fixed-add-value";
  addValue.placeholder = t("varFixedValuePh");
  const addBtn = document.createElement("button");
  addBtn.type = "button";
  addBtn.className = "ghost-btn var-fixed-add-btn";
  addBtn.textContent = t("varFixedAdd");
  const commitAdd = () => {
    const name = cleanVarPart(addName.value);
    const value = addValue.value.trim();
    if (!name || !value) { (name ? addValue : addName).focus(); return; }
    saveFixedVars({ ...readFixedVars(), [name]: value });
    renderExpert();
  };
  addBtn.addEventListener("click", commitAdd);
  for (const el of [addName, addValue]) {
    el.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); commitAdd(); } });
  }
  addRow.append(addName, addValue, addBtn);
  wrap.appendChild(addRow);
  return wrap;
}

// ---- Editor draft autosave ----
// Only the text side (name/text/variants) ever gets drafted -- an attached
// image is never re-derived from a crash draft, it stays with the prompt
// itself. One row on the backend (id "current"), since only one editor can be
// open at a time; the fingerprint check below is what keeps an unchanged tick
// from writing that row over and over while the user reads rather than types.
export function draftEditorFields() {
  if (!modalState) return null;
  // A confidential entry is never drafted: the draft row would hold its revealed text.
  if (modalState.secret) return null;
  const rotating = rotationUiOn();
  const hasVariants = rotationActive();
  const entries = rotating
    ? modalState.rotateEntries
    : hasVariants
      ? [modal.text.value, ...modalState.rotateEntries.slice(1)]
      : [modal.text.value];
  const promptId = modalState.mode === "edit" ? modalState.id : "neu";
  return { promptId, name: modal.name.value, text: entries[0] || "", variants: JSON.stringify(entries.slice(1)) };
}

export async function saveDraftNow() {
  const fields = draftEditorFields();
  if (!fields) return;
  const fp = JSON.stringify(fields);
  if (fp === draftLastSaved) return; // nothing changed since the last write
  draftLastSaved = fp;
  try {
    await invoke("save_editor_draft", fields);
  } catch (_) {
    draftLastSaved = null; // let the next tick retry a failed write
  }
}

export function armDraftAutosave() {
  if (draftTimer) { clearInterval(draftTimer); draftTimer = null; }
  draftLastSaved = null;
  if (val("editorDraftSecs") <= 0) return;
  const secs = Math.min(30, Math.max(1, val("editorDraftSecs")));
  draftTimer = setInterval(saveDraftNow, secs * 1000);
}

export function disarmDraftAutosave() {
  if (draftTimer) { clearInterval(draftTimer); draftTimer = null; }
  draftLastSaved = null;
}

export function openModal({ mode, id, name = "", text = "", color = "", image = "", showImage = false, copyImage = false, filePath = "", pdfPage = 0, iconPath = "", caption = "", captionSize = 0, font = "", fontSize = 0, favorite = false, tags = [], notes = "", hotkey = "", hotkeyPaste = "", variants = [], rotateMode = "", rotateAuto = false, secret = false, locked = false, title, targetCell }) {
  modalSession++;
  modalOpener = document.activeElement;
  varsRevealed = false;
  assignModalState({
    mode, id, color, image, showImage, copyImage, filePath, pdfPage, iconPath, tags: [...tags], hotkey, hotkeyPaste, targetCell,
    // What the editor opened with. Save compares against it and only sends an
    // image field when the user really changed the picture: a save must never be
    // able to clear a stored image just by not having it to hand.
    imageOpened: image,
    // The FULL entry list (entry 0 = what would otherwise be modal.text's value)
    // while rotation is active, empty while it is not.
    rotateEntries: variants.length ? [text, ...variants] : [],
    rotateMode,
    rotateAuto: !!rotateAuto,
    // Confidential-entry state. Toggling it is its own immediate backend action
    // (set_prompt_secret), never part of the regular save -- see
    // onModalSecretToggle, which is the only place that changes this in memory.
    secret: !!secret,
    locked: !!locked,
  });
  modal.title.textContent = title;
  modal.fav.classList.toggle("active", !!favorite);
  modal.fav.setAttribute("aria-pressed", String(!!favorite));
  updateSecretToggleUi(mode, !!secret, !!copyImage, filePath);
  modal.name.value = name;
  modal.text.value = text;
  mdEditing = false; // every prompt opens rendered; a click on the pane starts editing
  mdLastText = null;
  modal.notesText.value = notes;
  // A note of several lines opens at its full height (a one-row field would crush it).
  requestAnimationFrame(() => autoGrow(modal.notesText));
  modal.caption.value = caption;
  modal.captionSize.value = String(normSize(captionSize) || 0);
  updateCaptionPreview();
  // Per-tile style overrides, available when creating and editing.
  modal.fontSel.value = font;
  modal.sizeSel.value = String(normSize(fontSize));
  modal.delete.classList.toggle("hidden", mode !== "edit");
  disarmButton(modal.delete, t("delete"));
  colorsExpanded = false;
  renderSwatches(color);
  const tagInput = $("modal-tag-input");
  if (tagInput) tagInput.value = "";
  closeTagSuggest();
  renderTagChips();
  renderPromptHotkey();
  // Show the dialog before wiring the preview so the <video>/<img> decode and
  // lay out in a visible context (a hidden element never renders a first frame).
  modal.root.classList.remove("hidden");
  syncModalImageUi(mode);
  modalInitial = modalSnapshot();
  armDraftAutosave();
  if (secret) invoke("discard_editor_draft").catch(() => {});
  focusOnOpen(mode);
}

// Where the caret lands when the editor opens. A new prompt starts in the name with
// everything selected. An existing prompt with plain text opens in the text, the caret
// at its end; a Markdown prompt stays rendered, so the caret goes to the end of the name
// (nothing selected). Rotating, locked and media prompts have no text field to land in.
function focusOnOpen(mode) {
  if (mode !== "edit") {
    modal.name.focus();
    modal.name.select();
    return;
  }
  const textApplies = modal.root.classList.contains("tall") && !rotationUiOn() && !modalState.locked;
  if (textApplies && (!flag("mdPreview") || !hasMarkdownSyntax(modal.text.value))) {
    focusModalText();
    const n = modal.text.value.length;
    modal.text.setSelectionRange(n, n);
    modal.text.scrollTop = modal.text.scrollHeight;
    return;
  }
  modal.name.focus();
  const n = modal.name.value.length;
  modal.name.setSelectionRange(n, n);
}

// ---- Confidential entries (Vorschlag 68) ----
// Marking a prompt confidential only ever applies to a saved, plain text
// entry -- an image/file prompt has nothing in `text` worth encrypting, and a
// brand-new "create" draft has no id yet for set_prompt_secret to act on.
export function updateSecretToggleUi(mode, secret, copyImage, filePath) {
  const eligible = mode === "edit" && flag("secretPrompts") && !copyImage && !filePath;
  modal.secret.classList.toggle("hidden", !eligible);
  modal.secret.classList.toggle("active", !!secret);
  modal.secret.setAttribute("aria-pressed", String(!!secret));
}

// Ensures the vault has a live session key, asking to set one up on first use
// or to unlock it otherwise. Returns false on cancel or a wrong password --
// the caller must not proceed with anything that needs the key.
export async function ensureSecretVaultUnlocked() {
  const status = await invoke("secret_vault_status").catch(() => null);
  if (!status) return false;
  if (status.unlocked) return true;
  const pw = await confirmDialog(
    status.configured
      ? {
          title: t("secretUnlockTitle"),
          message: t("secretUnlockMsg"),
          confirmLabel: t("secretUnlockConfirmBtn"),
          input: { type: "password", placeholder: t("secretPasswordPh") },
          danger: false,
        }
      : {
          title: t("secretSetupTitle"),
          message: t("secretSetupMsg"),
          confirmLabel: t("secretSetupConfirmBtn"),
          input: { type: "password", placeholder: t("secretPasswordPh") },
          checkboxes: [{ key: "understand", label: t("secretSetupConfirm"), checked: false }],
          danger: false,
        }
  );
  if (!pw) return false;
  try {
    await invoke("secret_vault_unlock", { password: pw });
    return true;
  } catch (e) {
    toast(t("secretWrongPassword"));
    return false;
  }
}

// The one click handler for the editor's confidential toggle, on or off alike
// -- both directions re-encrypt/decrypt the STORED text with the vault key
// (set_prompt_secret), so any unsaved edit in the field must be saved first or
// this would silently discard it.
export async function onModalSecretToggle() {
  if (!modalState || modalState.mode !== "edit") return;
  if (modalSnapshot() !== modalInitial) {
    toast(t("secretNeedsSaveFirst"));
    return;
  }
  const id = modalState.id;
  const next = !modalState.secret;
  if (!(await ensureSecretVaultUnlocked())) return;
  try {
    await invoke("set_prompt_secret", { id, secret: next });
  } catch (e) {
    if (String(e) === "secret-locked") toast(t("secretWrongPassword")); else errToast(e);
    return;
  }
  modalState.secret = next;
  updateSecretToggleUi(modalState.mode, next, modalState.copyImage, modalState.filePath);
  await renderGrid();
  if (next) {
    // The field now holds ciphertext at rest -- closing avoids the editor
    // resaving the plain text it still shows over that.
    closeModal();
  } else {
    const fresh = prompts.find((x) => x.id === id);
    if (fresh) {
      modal.text.value = fresh.text;
      modalInitial = modalSnapshot();
    }
  }
}

// ---- live length counter (chars / words / ~tokens) in the editor ----
export let lenRaf = 0;
export function updateLenCounter() {
  const el = $("modal-length");
  if (!el) return;
  // Still a text prompt while the Markdown preview stands in for the textarea; never
  // while rotation replaced the textarea with the variants list instead.
  const on = flag("lengthCounter") && !rotationUiOn() &&
    !(modal.text.classList.contains("hidden") && modal.mdPreview.classList.contains("hidden"));
  el.classList.toggle("hidden", !on);
  if (!on) return;
  const s = modal.text.value;
  const chars = [...s].length; // code points, so emoji count as one
  const words = (s.trim().match(/\S+/g) || []).length;
  const tokens = Math.ceil(chars / 4); // rough heuristic, labeled with "~"
  const nf = (n) => n.toLocaleString(LANG);
  const units = txt("counterUnits") || "all";
  const parts = [];
  if (units === "all" || units === "chars") parts.push(`${nf(chars)} ${t("lenChars")}`);
  if (units === "all" || units === "words") parts.push(`${nf(words)} ${t("lenWords")}`);
  if (units === "all" || units === "tokens") parts.push(`~${nf(tokens)} ${t("lenTokens")}`);
  el.textContent = parts.join(" · ");
}
export function scheduleLenCounter() {
  if (lenRaf) return;
  lenRaf = requestAnimationFrame(() => { lenRaf = 0; updateLenCounter(); });
}

// Replace layout: the rendered pane stands in for the textarea until the user clicks
// into it. `mdEditing` is that moment: everywhere else the prompt reads as Markdown.
export let mdEditing = false;
export let mdTimer = 0;
export let mdLastText = null;

export function renderMdPreview(force) {
  const box = modal.mdPreview;
  if (!box) return;
  const s = modal.text.value;
  if (!force && s === mdLastText) return;
  mdLastText = s;
  // The only innerHTML in this feature, and it is fed a string assembled purely from
  // escaped slices plus literal tags: see renderMarkdown.
  box.innerHTML = renderMarkdown(s);
  scheduleFitText();
}
export function scheduleMdPreview() {
  // Nothing to draw into while the pane is hidden: but mark it stale, so the next
  // time it is shown it is rebuilt from the current text instead of a stale render.
  if (modal.mdPreview.classList.contains("hidden")) { mdLastText = null; return; }
  clearTimeout(mdTimer);
  // A plain timer, deliberately not requestAnimationFrame: a window that paints no
  // frames (occluded, minimised to the tray, restored from it) never runs a rAF
  // callback, and the pane would then sit there empty.
  mdTimer = setTimeout(() => renderMdPreview(), 60);
}

// Bring toolbar, textarea and preview in line with the flags and the picked layout.
// When `rotating`, the variants list IS the editing surface, so neither the
// textarea nor the rendered pane may stay in flow. Both used to survive it: the
// clear-X wrapper as an empty 120px box (the gap above the colour row) and, with
// on, the preview still rendering entry 1, which read as a stray field.
export function syncMdUi(textVisible, rotating = false) {
  const on = textVisible && !rotating && flag("mdPreview");
  if (!on) mdEditing = false;
  // ONE field: the rendered pane stands in for the textarea and hands it back on a
  // click. Two panes side by side was the second thing on screen that did the same
  // job: there is no layout option any more.
  modal.mdWrap.classList.toggle("hidden", !on || mdEditing);
  // Hide the WRAPPER, not just the textarea: the clear-X wrapper stays in flow with
  // an empty box and pushes the rendered pane down, so the field appeared to jump
  // 19px every time it handed over.
  const field = modal.text.closest(".clear-wrap-ta") || modal.text;
  if (textVisible) {
    const hide = rotating || (on && !mdEditing);
    field.classList.toggle("hidden", hide);
    modal.text.classList.toggle("hidden", hide);
    // Tied to `rotating` alone, not the full `hide` -- the mic still dictates
    // into modal.text's value while its rendered Markdown preview is what's on
    // screen, and keeping the row's own visibility stable across THAT swap is
    // what keeps the field from jumping vertically when it hands over.
    modal.voiceRow.classList.toggle("hidden", rotating);
  }
  // Fill the pane whenever the feature is on, visible or not: the first paint must
  // never depend on the pane already being on screen, or switching to it shows a
  // blank box until the next keystroke.
  clearTimeout(mdTimer);
  if (on) renderMdPreview(true);
  if (modal.copyFormattedBtn) {
    // Independent of `on`/`mdEditing` on purpose: if this depended on which of
    // the two (preview vs. textarea) is showing, the button's own hidden/shown
    // state would flip exactly when the field hands over, taking space away
    // from one side and not the other -- the same field-jump class of bug the
    // wrapper-hiding rule above already exists to avoid.
    const cfShow = textVisible && !rotating && flag("copyFormatted") && hasMarkdownSyntax(modal.text.value);
    modal.copyFormattedBtn.classList.toggle("hidden", !cfShow);
  }
}

// The placeholder bar shows when the text holds a placeholder, or once the user asked for
// it with the link under the text. Once shown it stays until the editor closes, so it
// never jumps away while a token is being typed.
let varsRevealed = false;
function syncVarsUi(textVisible, rotating) {
  const varsUi = textVisible && !rotating && flag("promptVars");
  const hasVars = extractVars(modal.text.value).length > 0;
  const showBar = varsUi && (hasVars || varsRevealed);
  if (showBar) varsRevealed = true;
  modal.textBar.classList.toggle("hidden", !showBar);
  modal.varsHint.classList.toggle("hidden", !showBar);
  $("modal-vars-link").classList.toggle("hidden", !varsUi || showBar);
  if (showBar) renderVarHelper(); // chips + insert row follow their flags
}

// The prompt field grows with its content up to the visible dialog height; past
// that it scrolls. The history panel keeps its own cap (history-open).
export function fitEditorTextHeight() {
  const box = modal.root.querySelector(".modal");
  if (!modal.root.classList.contains("tall") || modal.root.classList.contains("history-open")) {
    box.style.removeProperty("--ta-fit");
    return;
  }
  const field = modal.text.classList.contains("hidden") || (flag("mdPreview") && !mdEditing && !modal.mdWrap.classList.contains("hidden"))
    ? modal.mdPreview : modal.text;
  const keep = box.scrollTop;
  box.style.setProperty("--ta-fit", "120px"); // measure the content, not the old height
  const content = field.scrollHeight + 2;
  const fixed = modal.root.querySelector(".modal-name-row").offsetHeight + modal.root.querySelector(".modal-actions").offsetHeight;
  const cap = box.clientHeight - fixed - 48;
  box.style.setProperty("--ta-fit", Math.max(120, Math.min(content, cap)) + "px");
  box.scrollTop = keep;
}
// A plain timer, not requestAnimationFrame: a window that paints no frames (occluded,
// minimised to the tray) never runs a frame callback, and a pending one would block
// every later request (see scheduleMdPreview).
let fitTimer = 0;
export function scheduleFitText() {
  if (fitTimer) return;
  fitTimer = setTimeout(() => { fitTimer = 0; fitEditorTextHeight(); }, 16);
}

// Focus the prompt text field. With the Markdown preview standing in for it, the
// field is hidden until handed over, so a plain focus() would land nowhere.
export function focusModalText() {
  if (flag("mdPreview") && !mdEditing && !modal.mdWrap.classList.contains("hidden")) {
    mdEditing = true;
    syncMdUi(true);
  }
  modal.text.focus();
}

// Word-level diff between two texts via a classic LCS backtrack: no
// dependency, the whole thing is a couple dozen lines. O(n*m) on word count, so
// a pathologically huge pasted text falls back to plain text (DIFF_WORD_CAP)
// rather than freezing the editor while it opens the history panel.
export const DIFF_WORD_CAP = 4000;
export function diffWords(oldText, newText) {
  const tokenize = (s) => (s || "").split(/(\s+)/).filter((t) => t !== "");
  const a = tokenize(oldText);
  const b = tokenize(newText);
  const n = a.length, m = b.length;
  // Cap on the PRODUCT n*m (the actual DP cost), not each side independently --
  // a long old text diffed against a short new one (or vice versa) is cheap either way.
  if (n * m > DIFF_WORD_CAP * DIFF_WORD_CAP) return null;
  const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const out = [];
  let i = 0, j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) { out.push({ type: "same", text: a[i] }); i++; j++; }
    else if (dp[i + 1][j] >= dp[i][j + 1]) { out.push({ type: "del", text: a[i] }); i++; }
    else { out.push({ type: "add", text: b[j] }); j++; }
  }
  while (i < n) out.push({ type: "del", text: a[i++] });
  while (j < m) out.push({ type: "add", text: b[j++] });
  return out;
}
// One word-diff computed once, split into each side's own highlighted tokens --
// the left side keeps "same"/"del" (what it has that the right does not), the
// right side keeps "same"/"add" (what it has that the left does not). Both the
// duplicate finder's own columns and the free "compare two entries" view call
// this, instead of each running its own comparison.
export function splitDiffSides(leftText, rightText) {
  const tokens = diffWords(leftText, rightText);
  if (!tokens) return null; // DIFF_WORD_CAP fallback -- caller shows plain text
  return {
    left: tokens.filter((tk) => tk.type !== "add"),
    right: tokens.filter((tk) => tk.type !== "del"),
  };
}
// Text nodes only: never innerHTML on prompt content, same rule the Markdown
// preview follows (a prompt's own text is fully user-controlled).
export function renderDiffInto(el, tokens) {
  el.textContent = "";
  for (const t of tokens) {
    if (t.type === "same") {
      el.appendChild(document.createTextNode(t.text));
      continue;
    }
    const span = document.createElement("span");
    span.className = t.type === "add" ? "diff-add" : "diff-del";
    span.textContent = t.text;
    el.appendChild(span);
  }
}

// A snapshot's full entry list: index 0 is the base text, 1..n are rotation variants.
export const versionEntries = (v) => (v.variants?.length ? [v.text || "", ...v.variants] : [v.text || ""]);
// The editor's current, unsaved state: the "newer" side of the newest row's diff.
export const liveEntries = () => (modalState?.rotateEntries?.length ? [...modalState.rotateEntries] : [modal.text.value || ""]);
export async function loadHistory(id) {
  const body = $("modal-history-body");
  if (!body || !id) return;
  body.innerHTML = "";
  let list = [];
  try { list = await invoke("list_versions", { promptId: id }); } catch (_) {}
  // A slower response for a since-abandoned prompt must not land in the editor
  // after it has already moved on to a different one.
  if (modalState?.id !== id) return;
  if (!list.length) {
    const e = document.createElement("div");
    e.className = "hint";
    e.textContent = t("historyEmpty");
    body.appendChild(e);
    return;
  }
  list.forEach((v, idx) => {
    // Diff against whatever came right after this snapshot: the live editor
    // state for the newest row, the next-newer snapshot for every other one. Each
    // side is the FULL entry list (rotation variants included), not just entry 1.
    const olderEntries = versionEntries(v);
    const newerEntries = idx === 0 ? liveEntries() : versionEntries(list[idx - 1]);
    const newerName = idx === 0 ? modal.name.value : list[idx - 1].name;
    const row = document.createElement("div");
    row.className = "hist-row";
    const head = document.createElement("div");
    head.className = "hist-head";
    const time = document.createElement("span");
    time.className = "hist-time";
    time.textContent = new Date(v.ts * 1000).toLocaleString(LANG);
    const prev = document.createElement("span");
    prev.className = "hist-preview";
    prev.textContent = (v.name ? v.name + ": " : "") + olderEntries[0].slice(0, HISTORY_PREVIEW_LEN);
    head.append(time, prev);
    if (olderEntries.length > 1) {
      const chip = document.createElement("span");
      chip.className = "hist-entries";
      chip.textContent = t("historyEntriesN").replace("{n}", String(olderEntries.length));
      head.appendChild(chip);
    }
    // Explicit expand chevron (collapsed by default) → reveals the COMPLETE text.
    const expand = document.createElement("button");
    expand.type = "button";
    expand.className = "hist-expand";
    expand.title = t("historyExpandToggle");
    expand.setAttribute("aria-expanded", "false");
    expand.innerHTML = '<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M6 9l6 6 6-6"/></svg>';
    const restore = document.createElement("button");
    restore.type = "button";
    restore.className = "ghost-btn hist-restore";
    restore.textContent = t("historyRestore");
    restore.addEventListener("click", async () => {
      try {
        const p = await invoke("restore_version", { versionId: v.id });
        // The editor may have moved on to a different (or no) prompt while this
        // invoke was in flight: never write a stale restore into the wrong modal.
        if (modalState?.id !== id) return;
        if (p) {
          const pIdx = prompts.findIndex((x) => x.id === p.id);
          if (pIdx >= 0) prompts[pIdx] = p;
          modal.name.value = p.name;
          modal.text.value = p.text;
          // The open editor must pick up the restored rotation entries too, or the
          // next save writes the still-open (stale) entries back over them.
          modalState.rotateEntries = (p.variants || []).length ? [p.text, ...p.variants] : [];
          modalState.rotateMode = p.rotate_mode || "";
          renderRotateUi();
          updateLenCounter();
          toast(t("historyRestored"));
          await renderGrid(true);
          loadHistory(id); // the restore itself added a fresh snapshot
        }
      } catch (err) {
        errToast(err);
      }
    });
    // Keeps the current entry untouched and adds the snapshot as a separate one
    // instead, for comparing them side by side rather than losing whichever
    // version restore_version would have overwritten.
    const restoreNew = document.createElement("button");
    restoreNew.type = "button";
    restoreNew.className = "ghost-btn hist-restore-new";
    restoreNew.textContent = t("historyRestoreAsNew");
    restoreNew.addEventListener("click", async () => {
      try {
        const suffix = t("historyRestoreAsNewSuffix").replace("{date}", new Date(v.ts * 1000).toLocaleString(LANG));
        const created = await invoke("restore_version_as_new", { versionId: v.id, suffix });
        // Same staleness guard as the restore button above: the editor may have
        // moved on while this invoke was in flight.
        if (modalState?.id !== id) return;
        if (created) {
          closeModal();
          await renderGrid();
          focusTile(created.id); // "gets selected after creation" -- see what happened
          toast(t("historyRestoredAsNew"));
        }
      } catch (err) {
        errToast(err);
      }
    });
    head.append(expand, restore, restoreNew);
    const full = document.createElement("div");
    full.className = "hist-full hidden";
    // Building this (word-diffing every entry) is expensive at long prompts /
    // many rotation entries; each row starts collapsed, so defer it to first expand
    // and cache the result -- most rows are never opened at all.
    let fullBuilt = false;
    const buildFull = () => {
      if (fullBuilt) return;
      fullBuilt = true;
      if (v.name !== newerName) {
        const nameRow = document.createElement("div");
        nameRow.className = "hist-name-change";
        const label = document.createElement("span");
        label.className = "hist-name-label";
        label.textContent = t("historyNameChanged") + ": ";
        const oldSpan = document.createElement("span");
        oldSpan.className = "diff-del";
        oldSpan.textContent = v.name || "";
        const newSpan = document.createElement("span");
        newSpan.className = "diff-add";
        newSpan.textContent = newerName || "";
        nameRow.append(label, oldSpan, document.createTextNode(" → "), newSpan);
        full.appendChild(nameRow);
      }
      if (olderEntries.every((e) => !e) && newerEntries.every((e) => !e)) {
        const hint = document.createElement("div");
        hint.className = "hint";
        hint.textContent = t("historyNoText");
        full.appendChild(hint);
      } else {
        const maxLen = Math.max(olderEntries.length, newerEntries.length);
        for (let i = 0; i < maxLen; i++) {
          const hasOlder = i < olderEntries.length;
          const hasNewer = i < newerEntries.length;
          const block = document.createElement("div");
          if (maxLen > 1) {
            block.className = "hist-entry-block";
            const num = document.createElement("div");
            num.className = "hist-entry-num";
            num.textContent = String(i + 1);
            block.appendChild(num);
          }
          const content = document.createElement("div");
          content.className = "hist-entry-content";
          if (!hasNewer) {
            const span = document.createElement("span");
            span.className = "diff-del";
            span.textContent = olderEntries[i];
            content.appendChild(span);
          } else if (!hasOlder) {
            const span = document.createElement("span");
            span.className = "diff-add";
            span.textContent = newerEntries[i];
            content.appendChild(span);
          } else {
            const tokens = diffWords(olderEntries[i], newerEntries[i]);
            if (tokens) renderDiffInto(content, tokens);
            else content.textContent = olderEntries[i]; // DIFF_WORD_CAP fallback: plain text
          }
          block.appendChild(content);
          full.appendChild(block);
        }
      }
    };
    const toggle = () => {
      const open = !full.classList.toggle("hidden");
      if (open) buildFull();
      expand.setAttribute("aria-expanded", String(open));
      row.classList.toggle("open", open);
    };
    expand.addEventListener("click", toggle);
    prev.addEventListener("click", toggle); // clicking the preview also expands
    row.append(head, full);
    body.appendChild(row);
  });
}

// Keep all image/file modal controls consistent with modalState.
export function syncModalImageUi(mode) {
  const { image, showImage, copyImage, filePath, iconPath } = modalState;
  const kind = filePath ? mediaKind(filePath) : "";
  const iconKind = !image && iconPath ? mediaKind(iconPath) : "";
  // Any media file previews straight from its path (image/gif/video): so an
  // image still shows even when the backend preview re-encode returned nothing.
  const fileMedia = !image && !iconKind && !!kind;
  const hasPreview = !!image || !!iconKind || fileMedia;
  // Image and file prompts have no text field: the name doubles as the copy text.
  // Plain text prompts expose it on create (review before saving) and edit.
  const textVisible = (mode === "edit" || mode === "create") && !copyImage && !filePath;
  // Full-height dialog only when editing a text prompt (max textarea space).
  modal.root.classList.toggle("tall", textVisible);
  // Only a text prompt, and only while the feature is on, ever offers rotation.
  modal.rotateToggleRow.classList.toggle("hidden", !textVisible || !flag("rotatingPrompts"));
  renderRotateUi();
  // rotationUiOn(), not rotationActive(): with the feature off but the prompt
  // still holding stored entries, the plain textarea is what must show (entry 0,
  // editable), or the editor is stuck showing a list with no way back to it (its
  // own toggle row is hidden too while the feature is off).
  const rotating = textVisible && rotationUiOn();
  modal.text.classList.toggle("hidden", !textVisible || rotating);
  modal.voiceRow.classList.toggle("hidden", !textVisible || rotating);
  $("modal-text-label").classList.toggle("hidden", !textVisible || rotating);
  syncVarsUi(textVisible, rotating); // placeholder bar, or the link that reveals it
  syncMdUi(textVisible, rotating); // preview toolbar + rendered pane
  updateLenCounter();
  // The variable helper is a reference, so it reopens collapsed too.
  modal.varsBody.classList.add("hidden");
  modal.varsHead.setAttribute("aria-expanded", "false");
  modal.textBar.classList.remove("open");
  // Tags/hotkey/history share one "Advanced options" collapsible, which always
  // reopens collapsed. The hotkey row exists for every prompt kind (a hotkey
  // copies whatever the prompt holds). Version history is offered for every existing
  // prompt kind: a snapshot also captures name-only changes on an image/file/colour
  // prompt, so it stays reachable there too. The outer section itself never hides.
  modal.hotkey.classList.toggle("hidden", !promptHotkeysOn());
  showHotkeyError("");
  const histOn = mode === "edit" && flag("promptHistory");
  $("modal-history").classList.toggle("hidden", !histOn);
  if (histOn) $("modal-history-body").innerHTML = "";
  // Font and tile text size always live in here, so the head never hides.
  modal.advanced.classList.remove("hidden");
  modal.advancedBody.classList.add("hidden");
  modal.advancedHead.setAttribute("aria-expanded", "false");
  modal.advanced.classList.remove("open");
  modal.root.classList.remove("history-open");
  modal.name.placeholder = filePath
    ? t(kind === "video" ? "videoNamePh" : kind ? "imageNamePh" : "fileNamePh")
    : copyImage ? t("imageNamePh") : t("namePh");
  modal.imgWrap.classList.toggle("hidden", !hasPreview);
  modal.addIcon.classList.toggle("hidden", hasPreview);
  // "Replace media" always available next to "Remove media"; only the image
  // of a clipboard-image prompt cannot be removed (it IS the prompt).
  modal.removeImg.classList.toggle("hidden", copyImage);
  if (hasPreview) {
    const previewPath = iconKind ? iconPath : filePath;
    const isVideo = !image && mediaKind(previewPath) === "video";
    modal.img.classList.toggle("hidden", isVideo);
    modal.video.classList.toggle("hidden", !isVideo);
    if (isVideo) {
      modal.video.src = convertFileSrc(previewPath);
      modal.video.load();
      modal.video.play().catch(() => {}); // show a frame even if autoplay is held
    } else {
      // Prefer the re-encoded preview; fall back to the raw file, and if even
      // that fails to decode, ask the backend for a data URL once.
      const session = modalSession;
      modal.img.onerror = () => {
        modal.img.onerror = null;
        loadFilePreview(previewPath, modalState?.pdfPage || 0).then((d) => {
          if (d && session === modalSession) modal.img.src = d;
        });
      };
      modal.img.src = image || convertFileSrc(previewPath);
      modal.video.removeAttribute("src");
    }
    modal.showImage.classList.toggle("active", showImage);
    modal.showText.classList.toggle("active", !showImage);
  } else {
    modal.video.removeAttribute("src");
  }
  modal.fileWrap.classList.toggle("hidden", !filePath);
  const fileHint = $("modal-file-hint");
  fileHint.classList.toggle("hidden", !filePath);
  fileHint.textContent = t(optFlag("storeFiles") ? "fileHintStored" : "fileHint");
  // The two only actually need telling apart when both a stored preview and a
  // real attachment exist side by side -- otherwise there is nothing to confuse.
  const previewAttachHint = $("modal-preview-attach-hint");
  previewAttachHint.classList.toggle("hidden", !(hasPreview && filePath));
  const fileIconEl = $("modal-file-icon");
  fileIconEl.title = t("previewAttachHint");
  if (filePath) {
    const isPdf = PDF_EXT.test(filePath);
    $("modal-file-icon").innerHTML = isPdf ? ICON_PDF : ICON_FILE;
    modal.fileName.textContent = filePath.split(/[\\/]/).pop();
    modal.fileName.title = filePath;
    // Shown 1-based (page 1, 2, ...): pdfPage itself stays the 0-based index
    // pdf_preview expects, so the two never need converting anywhere else.
    modal.pdfPage.classList.toggle("hidden", !isPdf);
    if (isPdf) modal.pdfPage.value = String((modalState?.pdfPage || 0) + 1);
  } else {
    modal.pdfPage.classList.add("hidden");
  }
  renderAttachTextPreview(filePath);
  scheduleFitText();
}

// ---- Rotating prompts (editor) ----
// While active, modalState.rotateEntries holds EVERY entry (index 0 = what would
// otherwise be modal.text's own value): kept separate from modal.text so the two
// never fight over which one is "the" source of truth while the switch is on.
export const rotationActive = () => (modalState?.rotateEntries || []).length > 0;
// rotationActive() alone answers "does this prompt have stored entries" (used
// by confirmModal to decide what to preserve on save); this answers "is the rotate
// list actually the thing shown right now": false while the feature itself is off,
// even with entries still stored. DISPLAY decisions (which surface to show/hide)
// must use this one, or a prompt edited while the feature is off gets its rotate
// list shown with no way back to the textarea (its own toggle row is hidden too).
export const rotationUiOn = () => rotationActive() && flag("rotatingPrompts");

export function renderRotateUi() {
  if (!modalState) return;
  // UI-gated, same reasoning as syncModalImageUi -- the list must stay hidden
  // (in favour of the plain textarea) while the feature itself is off, even with
  // entries still stored on the prompt.
  const active = rotationUiOn();
  modal.rotateBadge.classList.toggle("hidden", !active);
  modal.rotateSwitch.checked = active;
  modal.rotateList.classList.toggle("hidden", !active);
  if (!active) return;
  renderRotateEntries();
  modal.rotateMode.value = modalState.rotateMode === "random" ? "random" : "order";
  refreshRotateNext();
  renderRotateAutoBtn();
}

// Mirrors the backend's auto_eligible minus the two feature gates:
// rotatingPrompts already gates this whole row (rotationUiOn), and rotateAuto
// (the feature switch, 93) is checked here so the button disables live if it
// gets flipped off with the editor open.
export function rotateAutoEligibleHere() {
  const filled = (modalState.rotateEntries || []).filter((e) => (e || "").trim()).length;
  return flag("rotateAuto") && !modalState.image && !modalState.filePath && filled >= 2;
}

export function renderRotateAutoBtn() {
  const eligible = rotateAutoEligibleHere();
  modal.rotateAuto.disabled = !eligible;
  const on = eligible && !!modalState.rotateAuto;
  modal.rotateAuto.classList.toggle("active", on);
  modal.rotateAuto.setAttribute("aria-pressed", String(on));
  const hasVars = (modalState.rotateEntries || []).some((entry) => extractVars(entry).length);
  modal.rotateAuto.title = !eligible ? t("rotateAutoUnavailable") : hasVars ? t("rotateAutoVarsHint") : t("rotateAutoTip");
}

export function renderRotateEntries() {
  const list = modalState.rotateEntries || [];
  const max = Math.max(2, Math.round(val("rotateMaxEntries")) || 20);
  modal.rotateEntries.innerHTML = "";
  list.forEach((entry, i) => {
    const row = document.createElement("div");
    row.className = "rotate-entry-row";
    const num = document.createElement("span");
    num.className = "rotate-entry-num";
    num.textContent = String(i + 1);
    const ta = document.createElement("textarea");
    ta.className = "modal-input rotate-entry-input";
    ta.rows = 1;
    ta.value = entry;
    // Typing can flip an entry between blank/filled (eligibility) or add/remove
    // a placeholder (the button's hint tooltip): both must stay live, not just on open.
    ta.addEventListener("input", () => { list[i] = ta.value; autoGrow(ta); renderRotateAutoBtn(); });
    const btns = document.createElement("div");
    btns.className = "rotate-entry-btns";
    const up = document.createElement("button");
    up.type = "button";
    up.className = "icon-btn rotate-entry-move";
    up.title = t("rotateMoveUp");
    up.setAttribute("aria-label", t("rotateMoveUp"));
    putIcon(up, ICON_UP);
    up.disabled = i === 0;
    up.addEventListener("click", () => { [list[i - 1], list[i]] = [list[i], list[i - 1]]; renderRotateEntries(); renderRotateAutoBtn(); });
    const down = document.createElement("button");
    down.type = "button";
    down.className = "icon-btn rotate-entry-move";
    down.title = t("rotateMoveDown");
    down.setAttribute("aria-label", t("rotateMoveDown"));
    putIcon(down, ICON_DOWN);
    down.disabled = i === list.length - 1;
    down.addEventListener("click", () => { [list[i + 1], list[i]] = [list[i], list[i + 1]]; renderRotateEntries(); renderRotateAutoBtn(); });
    const del = document.createElement("button");
    del.type = "button";
    del.className = "icon-btn rotate-entry-del";
    del.title = t("delete");
    del.setAttribute("aria-label", t("delete"));
    putIcon(del, ICON_TRASH);
    del.disabled = list.length <= 1;
    del.addEventListener("click", () => { list.splice(i, 1); renderRotateEntries(); renderRotateAutoBtn(); });
    btns.append(up, down, del);
    row.append(num, ta, btns);
    modal.rotateEntries.appendChild(row);
    requestAnimationFrame(() => autoGrow(ta));
  });
  modal.rotateAdd.disabled = list.length >= max;
}

// "Next: n/total" + "From the start" only mean anything for an ALREADY SAVED
// rotating prompt: a brand-new or not-yet-multi-entry one has no backend pointer
// to read yet.
export async function refreshRotateNext() {
  const list = modalState.rotateEntries || [];
  const showPeek = modalState.mode === "edit" && !!modalState.id && list.length > 1;
  modal.rotateNext.classList.toggle("hidden", !showPeek);
  modal.rotateReset.classList.toggle("hidden", !showPeek);
  if (!showPeek) return;
  const id = modalState.id; // snapshot before the await, re-check after
  try {
    const peek = await invoke("rotation_peek", { id });
    if (peek && modalState.id === id) {
      modal.rotateNext.textContent = t("rotateNextLabel")
        .replace("{n}", String(peek.pos)).replace("{total}", String(peek.total));
    }
  } catch (_) { /* the peek is a convenience readout, never worth surfacing an error for */ }
}

// One native dialog at a time: ignore further requests until it is closed
// (the dialog is also window-modal on the backend side).
export let dialogBusy = false;
export async function withDialog(fn) {
  if (dialogBusy) return null;
  dialogBusy = true;
  try {
    return await fn();
  } finally {
    dialogBusy = false;
  }
}

// Page defaults to the first (0). The backend clamps against the real page
// count, so a stale number from a previously longer PDF at the same path still
// renders something instead of nothing.
export async function loadFilePreview(path, page = 0) {
  if (PDF_EXT.test(path)) {
    return (await invoke("pdf_preview", { path, page }).catch(() => "")) || "";
  }
  if (!IMAGE_EXT.test(path)) return "";
  return (await invoke("load_image_file", { path }).catch(() => "")) || "";
}

// The four text attachment kinds a plain-text preview is offered for,
// alongside the image/PDF preview above. Matches TEXT_PREVIEW_EXT in images.rs
// -- one list of kinds, kept in agreement on both sides by hand since the two
// cannot share a source at build time.
export const TEXT_PREVIEW_EXT = /\.(txt|md|json|csv)$/i;

// Fills the attachment's plain-text preview box, or hides it. `textContent`
// only, never innerHTML -- the four formats this covers (txt/md/json/csv) are
// shown as literal text, not rendered as markup, even a .md file with real
// Markdown syntax in it.
export async function renderAttachTextPreview(path) {
  const box = modal.textAttachPreview;
  if (!box) return;
  const eligible = !!path && flag("attachTextPreview") && TEXT_PREVIEW_EXT.test(path);
  box.classList.toggle("hidden", !eligible);
  if (!eligible) return;
  const session = modalSession;
  modal.textAttachContent.textContent = "";
  modal.textAttachTruncated.classList.add("hidden");
  modal.textAttachUnreadable.classList.add("hidden");
  let result;
  try {
    result = await invoke("attachment_text_preview", { path, maxKb: ATTACH_PREVIEW_KB[EXPERT_DROPDOWNS.attachPreviewSize.get()] });
  } catch (_) {
    return; // not a text-preview extension after all, or the file vanished mid-read
  }
  if (session !== modalSession || modalState?.filePath !== path) return; // a later modal already took over
  if (!result || !result.readable) {
    modal.textAttachUnreadable.classList.remove("hidden");
    return;
  }
  modal.textAttachContent.textContent = result.text;
  modal.textAttachTruncated.classList.toggle("hidden", !result.truncated);
}

// Build a file-attach prompt from a path. Image, gif, video and PDF
// attachments behave like media prompts; a PDF shows its rendered first page
// but still copies the file on click.
export async function openFileCreate(path) {
  const kind = mediaKind(path);
  const isPdf = PDF_EXT.test(path);
  const image = await loadFilePreview(path);
  openModal({
    mode: "file-create",
    title: t(kind === "video" ? "videoModalTitle" : kind || isPdf ? "imageModalTitle" : "fileModalTitle"),
    name: path.split(/[\\/]/).pop(),
    filePath: path,
    image,
    showImage: !!kind || (isPdf && !!image),
  });
}

// Left click: open the file dialog directly.
export async function startFileCreate() {
  await withDialog(async () => {
    const path = await invoke("pick_file_path");
    if (path) await openFileCreate(path);
  });
}

// Right click: take a file from the clipboard (not text); a copied file, or a
// copied image (screenshot) kept as a new file.
export async function startFileFromClipboard() {
  await withDialog(async () => {
    const path = await invoke("get_clipboard_file_path").catch(() => "");
    if (path) { await openFileCreate(path); return; }
    const clipImg = await invoke("get_clipboard_image").catch(() => "");
    if (clipImg) {
      openModal({ mode: "image-create", title: t("imageModalTitle"), name: t("filterImage"), image: clipImg, showImage: true, copyImage: true });
      return;
    }
    toast(t("noClipboardFile"));
  });
}
export function closeModal() {
  if (voiceState !== "idle" && voiceTargetEl === modal.text) stopVoice(); // its field is about to disappear
  closeColorPop();
  closeTagSuggest();
  closeTagPop();
  modal.root.classList.add("hidden");
  modal.video.removeAttribute("src"); // stop a playing preview
  // An orderly close -- save or deliberate cancel/discard, every caller funnels
  // through here -- drops the autosave draft right away rather than leaving it
  // for the boot-time recovery check to find and offer back for nothing.
  disarmDraftAutosave();
  if (modalState) invoke("discard_editor_draft").catch(() => {});
  assignModalState(null);
  modalInitial = "";
  // A tile the editor was opened from can be gone by the time it closes (the
  // prompt got deleted mid-edit, or a re-render replaced the DOM node) --
  // focusing a detached element is a harmless no-op, never an error.
  modalOpener?.focus?.();
  modalOpener = null;
}

// Ask before dismissing the modal via the background. A new save dialog (text,
// image, file, screenshot) always holds unsaved work; an edit only when changed.
export async function confirmDiscardIfDirty() {
  if (!flag("confirmDiscard")) return true;
  const isNew = modalState && modalState.mode !== "edit";
  if (!isNew && modalSnapshot() === modalInitial) return true;
  return confirmDialog({
    title: t("discardTitle"),
    message: t("discardMsg"),
    confirmLabel: t("discardConfirm"),
    cancelLabel: t("keepEditing"),
  });
}

// Same guard as the X button/backdrop/Escape, for an action (drag-drop) that can
// fire with no editor open at all -- confirmDiscardIfDirty relies on modalState
// being set, so it is only asked when the modal is actually visible.
export function confirmDiscardOrProceed(run) {
  if (modal.root.classList.contains("hidden")) {
    run();
    return;
  }
  confirmDiscardIfDirty().then((ok) => { if (ok) run(); });
}

export function startCreate() {
  const text = inputEl.value.trim();
  if (!text) return;
  // Carry the composed text into the dialog so it can be reviewed/edited (and
  // previewed as Markdown) before saving.
  openModal({ mode: "create", text, title: t("nameModalTitle") });
}

export async function confirmModal() {
  if (!modalState || modalSaving) return;
  // Saving ends a dictation session targeting this dialog's text field --
  // one of the five stop paths, so the recognizer never keeps listening into a
  // field that just closed.
  if (voiceState !== "idle" && voiceTargetEl === modal.text) stopVoice();
  const name = modal.name.value.trim();
  if (!name) { modal.name.focus(); return; }
  // While rotating, the real text lives in rotateEntries, not modal.text.
  // rotationActive() (raw): with the feature off but the prompt still holding
  // stored entries, they must still be preserved on save, just sourced
  // from wherever each one actually lives right now: entry 0 from the visible,
  // editable textarea (rotationUiOn() is false, so THAT is the surface the user just
  // edited), entries 1..n untouched from modalState (never shown, never editable
  // while the feature is off).
  const rotating = rotationUiOn();
  const hasVariants = rotationActive();
  const rawTexts = rotating
    ? modalState.rotateEntries
    : hasVariants
      ? [modal.text.value, ...modalState.rotateEntries.slice(1)]
      : [modal.text.value];
  // A {?} with no name inside never becomes a fillable variable: warn now,
  // while it is still one click away from being fixed, instead of after it is saved.
  if (rawTexts.some(hasEmptyVarToken)) {
    const proceed = await confirmDialog({
      title: t("emptyVarTitle"),
      message: t("emptyVarBody"),
      confirmLabel: t("emptyVarSaveAnyway"),
      danger: false,
    });
    if (!proceed) { if (!rotating) focusModalText(); return; }
  }
  // "empty entries are dropped on save" applies to entry 1 too: the first
  // surviving non-blank entry becomes the prompt's own text.
  const rotateFilled = (rotating || hasVariants) ? rawTexts.map((s) => s.trim()).filter(Boolean) : [];
  const variants = rotateFilled.slice(1);
  const rotateMode = variants.length ? (modalState.rotateMode === "random" ? "random" : "") : "";
  const rotateAuto = variants.length ? !!modalState.rotateAuto : false;

  const color = modalState.color || "";
  const image = modalState.image || "";
  const filePath = modalState.filePath || "";
  const iconPath = modalState.iconPath || "";
  const caption = modal.caption.value.trim();
  const captionSize = Number(modal.captionSize.value) || 0;
  const font = modal.fontSel.value;
  const fontSize = Number(modal.sizeSel.value) || 0;
  const favorite = modal.fav.classList.contains("active");
  // Sent even while the feature is off, so switching it off never drops stored
  // tags or notes.
  const tags = modalState.tags || [];
  const notes = modal.notesText.value;
  const hotkey = modalState.hotkey || "";
  const hotkeyPaste = hotkey ? (modalState.hotkeyPaste || "") : "";
  const pdfPage = modalState.pdfPage || 0;
  // Double-click on an empty cell opened this dialog for that exact spot: the
  // backend's own first-free-cell placement would otherwise ignore it.
  const targetCell = modalState.targetCell;
  // NOTE: Tauri expects camelCase keys for snake_case Rust args.
  // Path media (gif/video icon or attachment) shows without a stored image.
  const showImage =
    image || mediaKind(filePath) || mediaKind(iconPath) ? modalState.showImage : false;
  const copyImage = image ? modalState.copyImage : false;
  let saved = null;
  modalSaving = true;
  modal.confirm.disabled = true;
  try {
    if (modalState.mode === "create") {
      const text = (rotating || hasVariants) ? (rotateFilled[0] || "") : modal.text.value.trim();
      // Nothing to save yet: keep the dialog (and everything else typed into it)
      // and point at the missing text, like a missing name does.
      if (!text) { if (!rotating) focusModalText(); return; }
      saved = await invoke("add_prompt", { name, text, color, image, showImage, copyImage, filePath, pdfPage, iconPath, caption, captionSize, font, fontSize, favorite, tags, notes, hotkey, hotkeyPaste, variants, rotateMode, rotateAuto });
      inputEl.value = "";
      autoGrow(inputEl);
      saveBtn.disabled = true;
    } else if (modalState.mode === "image-create" || modalState.mode === "file-create") {
      // The name doubles as the copy text when shown as text.
      saved = await invoke("add_prompt", { name, text: name, color, image, showImage, copyImage, filePath, pdfPage, iconPath, caption, captionSize, font, fontSize, favorite, tags, notes, hotkey, hotkeyPaste, variants: [], rotateMode: "", rotateAuto: false });
    } else {
      const text = copyImage || filePath ? name : ((rotating || hasVariants) ? (rotateFilled[0] || "") : modal.text.value);
      const args = { id: modalState.id, name, text, color, showImage, copyImage, filePath, pdfPage, iconPath, caption, captionSize, font, fontSize, favorite, tags, notes, hotkey, hotkeyPaste, variants, rotateMode, rotateAuto };
      // The image field only travels when it actually changed: an untouched
      // picture is left to the backend, and "" only ever means the remove button.
      if (image !== (modalState.imageOpened || "")) args.image = image;
      saved = await invoke("update_prompt", args);
      // Save can turn rotateAuto off (or drop it below two entries) while a
      // session for THIS prompt happens to be armed: end it now instead of
      // waiting for the next paste to notice, same as the tile menu's own toggle.
      // A no-op on the backend if no session is running for this id.
      if (!rotateAuto) await invoke("rotation_end_auto", { id: modalState.id }).catch(() => {});
    }
  } catch (err) {
    const errStr = String(err);
    // save-failed has no hotkey involved - route it through the same friendly
    // message the persist-failed event uses, instead of hotkeyMessage's fallback
    // of showing the raw error code verbatim.
    toast(errStr === "save-failed" ? t("persistFailed") : hotkeyMessage(errStr)); // keep the modal open so nothing typed is lost
    // A binding the backend refused was never stored, so the field must not keep
    // showing it: drop it and name the reason where it was captured.
    if (HOTKEY_ERRORS[errStr] && hotkey) {
      setPromptHotkey("");
      showHotkeyError(hotkeyMessage(errStr));
    }
    return;
  } finally {
    modalSaving = false;
    modal.confirm.disabled = false;
  }
  // The backend hands back what it really stored. An empty hotkey where one
  // was sent means the OS refused the combination between the check and the save.
  if (hotkey && saved && !saved.hotkey) toast(t("promptHotkeyFail"));
  // The same near-duplicate check the import path already runs, now also
  // after creating a prompt by hand: before closeModal() clears modalState.
  const wasCreate = modalState.mode === "create" || modalState.mode === "image-create" || modalState.mode === "file-create";
  const wasEdit = modalState.mode === "edit";
  // An edit can change entry count/text: drop the cached rotation state so the
  // grid re-render below primes it fresh instead of keeping a stale badge/tooltip
  // (primeRotateState only ever peeks once per id otherwise).
  if (saved) rotateState.delete(saved.id);
  closeModal();
  await renderGrid();
  if (saved && targetCell) await placeTile(saved.id, targetCell[0], targetCell[1]);
  pollMissingFiles(); // a replaced file clears the error immediately
  if (!libraryEl.classList.contains("hidden")) {
    // An edit MAY have just changed this prompt's "last edited" rank: the
    // library stayed open underneath the modal, so the usual refresh-on-open
    // (library-btn click) never fired for it.
    if (wasEdit && flag("promptHistory")) await refreshLastEdited();
    renderLibrary();
  }
  if (saved && wasCreate) maybeDupImportToast();
  // Runs for a create AND an edit, unlike maybeDupImportToast just above --
  // fired here, after everything the save itself needed is already done, and
  // never awaited, so a slow scan can only ever delay the hint toast.
  if (saved) maybeShowSimilarHint(saved.id);
}

// Everything about a prompt except what identifies it and the hotkey (one accelerator,
// one prompt): opens the normal create dialog prefilled, so the user can review or
// cancel before anything is saved. Placement then follows the regular create path.
// A stored prompt as the editor's fields. Both "edit" and "duplicate" go through
// here so a newly added prompt field can never reach one and be forgotten in the
// other: each caller then overrides only what genuinely differs between them.
export function promptToModal(p) {
  const kind = p.file_path ? mediaKind(p.file_path) : "";
  return {
    name: p.name,
    text: p.text,
    color: p.color || "",
    image: p.image || "",
    showImage: p.show_image || false,
    copyImage: p.copy_image || false,
    filePath: p.file_path || "",
    pdfPage: p.pdf_page || 0,
    iconPath: p.icon_path || "",
    caption: p.caption || "",
    captionSize: p.caption_size || 0,
    font: p.font || "",
    fontSize: p.font_size || 0,
    favorite: p.favorite || false,
    tags: p.tags || [],
    notes: p.notes || "",
    hotkey: p.hotkey || "",
    hotkeyPaste: p.hotkey_paste || "",
    variants: p.variants || [],
    rotateMode: p.rotate_mode || "",
    rotateAuto: p.rotate_auto || false,
    // get_prompt already refuses a locked confidential entry outright, so by
    // the time this runs `secret` (if true) always means "decrypted, editable
    // text below" -- never leftover ciphertext.
    secret: p.secret || false,
    title: p.file_path
      ? t(kind === "video" ? "videoEditTitle" : kind ? "imageEditTitle" : "fileEditTitle")
      : p.copy_image ? t("imageEditTitle") : t("editModalTitle"),
  };
}

export async function duplicatePrompt(id) {
  let p;
  try {
    p = await invoke("get_prompt", { id });
  } catch (err) {
    if (String(err) === "secret-locked" && (await ensureSecretVaultUnlocked())) {
      p = await invoke("get_prompt", { id }).catch((e2) => { errToast(e2); return null; });
    } else {
      if (String(err) !== "secret-locked") errToast(err);
      return;
    }
  }
  if (!p) return;
  // A copy starts unstarred, unbound and not confidential: one accelerator
  // belongs to one prompt, and turning the copy confidential again is its own
  // deliberate action, not something a duplicate inherits.
  openModal({
    ...promptToModal(p),
    mode: "create",
    name: `${p.name} (${t("duplicateSuffix")})`,
    favorite: false,
    secret: false,
    hotkey: "",
    hotkeyPaste: "",
  });
}

export async function editPrompt(id) {
  // A request from outside (the tray, a pill menu) can arrive while the editor holds
  // unsaved work: ask first, and close the open session the way Cancel does.
  if (modalState) {
    if (!(await confirmDiscardIfDirty())) return;
    closeModal();
  }
  let p;
  try {
    p = await invoke("get_prompt", { id });
  } catch (err) {
    // A confidential entry the vault has no key for right now -- ask for it,
    // exactly the same first-access moment a copy would ask for it, then
    // retry the same fetch once instead of just refusing.
    if (String(err) === "secret-locked" && (await ensureSecretVaultUnlocked())) {
      p = await invoke("get_prompt", { id }).catch((e2) => { errToast(e2); return null; });
    } else {
      if (String(err) !== "secret-locked") errToast(err);
      return;
    }
  }
  if (p) {
    openModal({ ...promptToModal(p), mode: "edit", id });
  }
}

export function bindEditor() {
  inputEl.addEventListener("input", () => {
    autoGrow(inputEl);
    saveBtn.disabled = !inputEl.value.trim();
  });
  inputEl.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.key === "Enter") { e.preventDefault(); startCreate(); }
    // Escape ends a dictation session targeting this field, instead of
    // falling through to whatever the global Escape handler would otherwise do.
    if (e.key === "Escape" && voiceState !== "idle" && voiceTargetEl === inputEl) {
      e.stopPropagation();
      stopVoice();
    }
  });
  // Paste an image or file (Ctrl+V) to save it as a button; plain text pastes
  // normally. A broken/unreadable image is reported, never saved.
  inputEl.addEventListener("paste", async (e) => {
    if (!flag("pasteMedia")) return;
    const dt = e.clipboardData;
    if (!dt) return;
    // Only intercept when the clipboard carries a file/image payload; plain text
    // keeps pasting normally.
    const hasPayload = dt.files?.length > 0 || [...(dt.items || [])].some((it) => it.kind === "file");
    if (!hasPayload) return;
    e.preventDefault();
    await withDialog(async () => {
      // Pasted bitmaps (screenshots) arrive as a file item with an empty type,
      // so try the image clipboard first, then fall back to a real file path.
      const img = await invoke("get_clipboard_image").catch(() => "");
      if (img) {
        openModal({ mode: "image-create", title: t("imageModalTitle"), name: t("filterImage"), image: img, showImage: true, copyImage: true });
        return;
      }
      const path = await invoke("get_clipboard_file_path").catch(() => "");
      if (path) await openFileCreate(path);
      else toast(t("imagePasteFailed"));
    });
  });
  saveBtn.addEventListener("click", startCreate);

  modal.confirm.addEventListener("click", confirmModal);
  modal.closeX.addEventListener("click", () => confirmDiscardIfDirty().then((ok) => { if (ok) closeModal(); }));
  modal.cancel.addEventListener("click", () => confirmDiscardIfDirty().then((ok) => { if (ok) closeModal(); }));
  modal.fav.addEventListener("click", () => {
    const on = modal.fav.classList.toggle("active");
    modal.fav.setAttribute("aria-pressed", String(on));
  });
  modal.secret.addEventListener("click", onModalSecretToggle);
  // Rotating prompts.
  modal.rotateSwitch.addEventListener("change", async () => {
    if (modal.rotateSwitch.checked) {
      modalState.rotateEntries = [modal.text.value];
      // Only a prompt that never rotated before takes the shipped/expert default:
      // re-checking the box mid-edit must not clobber a mode already chosen.
      if (!modalState.rotateMode) {
        modalState.rotateMode = EXPERT_DROPDOWNS.rotateDefaultMode.get() === "random" ? "random" : "";
      }
    } else {
      const hasExtra = (modalState.rotateEntries || []).length > 1;
      if (hasExtra) {
        const ok = await confirmDialog({
          title: t("rotateToggle"),
          message: t("rotateDiscardConfirm"),
          confirmLabel: t("discardConfirm"),
          danger: true,
        });
        if (!ok) { modal.rotateSwitch.checked = true; return; }
      }
      modal.text.value = (modalState.rotateEntries || [])[0] ?? modal.text.value;
      modalState.rotateEntries = [];
    }
    renderRotateUi();
    // The switch row only exists for a text prompt, so textVisible is true here.
    // Flipping it has to move the same three surfaces syncModalImageUi does: only
    // hiding the textarea left the clear-X wrapper (an empty 120px box) and, with
    // on, the rendered pane still showing entry 1 sitting above the colours.
    const rotating = rotationActive();
    modal.text.classList.toggle("hidden", rotating);
    modal.voiceRow.classList.toggle("hidden", rotating);
    $("modal-text-label").classList.toggle("hidden", rotating);
    syncVarsUi(true, rotating);
    syncMdUi(true, rotating);
    scheduleFitText();
    if (!modal.rotateSwitch.checked) modal.text.focus();
    // This must hide/show the readout on the SAME tick as the toggle: the
    // rAF-deferred scheduleLenCounter() left it visible for one frame, long enough
    // for a synchronous check right after the toggle to see it.
    updateLenCounter();
  });
  modal.rotateAdd.addEventListener("click", () => {
    const max = Math.max(2, Math.round(val("rotateMaxEntries")) || 20);
    if ((modalState.rotateEntries || []).length >= max) return;
    modalState.rotateEntries.push("");
    renderRotateEntries();
    renderRotateAutoBtn();
    const rows = modal.rotateEntries.querySelectorAll(".rotate-entry-input");
    rows[rows.length - 1]?.focus();
  });
  modal.rotateMode.addEventListener("change", () => {
    if (!modalState) return;
    modalState.rotateMode = modal.rotateMode.value === "random" ? "random" : "";
  });
  modal.rotateAuto.addEventListener("click", () => {
    if (modal.rotateAuto.disabled) return;
    modalState.rotateAuto = !modalState.rotateAuto;
    renderRotateAutoBtn();
  });
  modal.rotateReset.addEventListener("click", async () => {
    if (!modalState?.id) return;
    try {
      await invoke("rotation_reset", { id: modalState.id });
    } catch (err) {
      errToast(err);
      return;
    }
    toast(t("rotateResetDone"));
    refreshRotateNext();
  });
  // Click the preview media → fullscreen zoom/pan viewer.
  modal.img.addEventListener("click", () => { if (modal.img.src) openLightbox(modal.img.src, false); });
  modal.video.addEventListener("click", () => { if (modal.video.src) openLightbox(modal.video.src, true); });
  modal.name.addEventListener("keydown", (e) => { if (e.key === "Enter") confirmModal(); });
  modal.text.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && voiceState !== "idle" && voiceTargetEl === modal.text) {
      e.stopPropagation();
      stopVoice();
    }
  });
  modal.notesText.addEventListener("input", () => autoGrow(modal.notesText));
  modal.text.addEventListener("input", scheduleLenCounter); // live length readout
  modal.text.addEventListener("input", scheduleMdPreview);  // live preview (rAF-throttled)
  modal.text.addEventListener("input", scheduleFitText);    // the field follows its content
  modal.text.addEventListener("input", () => syncVarsUi(modal.root.classList.contains("tall"), rotationUiOn()));
  $("modal-vars-link").addEventListener("click", () => {
    varsRevealed = true;
    syncVarsUi(true, false);
    modal.varsBody.classList.remove("hidden");
    modal.varsHead.setAttribute("aria-expanded", "true");
    modal.textBar.classList.add("open");
    $("modal-var-name").focus();
  });
  window.addEventListener("resize", scheduleFitText);
  // Replace layout: the rendered pane IS the resting state, so it has to hand the
  // textarea over on a click and take back over once the user is done typing.
  const mdSwap = (editing) => {
    const from = editing ? modal.mdPreview : modal.text;
    const span = from.scrollHeight - from.clientHeight;
    const ratio = span > 0 ? from.scrollTop / span : 0;
    mdEditing = editing;
    syncMdUi(true);
    scheduleFitText();
    const to = editing ? modal.text : modal.mdPreview;
    if (editing) to.focus(); // never behind a frame callback: see scheduleMdPreview
    // Only the scroll position waits for layout; losing it costs nothing.
    requestAnimationFrame(() => {
      fitEditorTextHeight();
      const s = to.scrollHeight - to.clientHeight;
      to.scrollTop = s > 0 ? Math.round(ratio * s) : 0;
    });
  };
  modal.mdPreview.addEventListener("click", () => { if (flag("mdPreview")) mdSwap(true); });
  modal.mdPreview.addEventListener("focus", () => { if (flag("mdPreview")) mdSwap(true); });
  // Swapping back changes the height of the block, so doing it on the blur that a
  // mousedown produces moves whatever the user is halfway through clicking: the
  // click then lands somewhere else, or nowhere. Wait for the pointer to come up.
  let swapAfterClick = false;
  let pointerIsDown = false;
  window.addEventListener("pointerdown", () => { pointerIsDown = true; }, true);
  window.addEventListener("pointerup", () => {
    pointerIsDown = false;
    if (!swapAfterClick) return;
    swapAfterClick = false;
    if (mdEditing && document.activeElement !== modal.text) mdSwap(false);
  }, true);
  modal.text.addEventListener("blur", () => {
    if (!mdEditing) return;
    if (pointerIsDown) { swapAfterClick = true; return; }
    mdSwap(false);
  });
  // Variable helper collapsible (same convention as the notes row).
  modal.varsHead.addEventListener("click", () => {
    const open = !modal.varsBody.classList.toggle("hidden");
    modal.varsHead.setAttribute("aria-expanded", String(open));
    modal.textBar.classList.toggle("open", open);
  });
  // Capture: the field is read-only, the key press itself is the input.
  bindHotkeyField(modal.hotkeyInput, {
    id: () => modalState?.id || null,
    errEl: modal.hotkeyErr,
    showAllEl: $("modal-hotkey-showall"),
    current: () => modalState?.hotkey || "",
    onCommit: async (accel) => {
      const before = modalState?.id;
      try {
        await invoke("check_hotkey", { hotkey: accel, id: modalState?.id || null });
      } catch (err) {
        if (modalState?.id !== before) return null; // moved on: nothing to show for it
        throw err;
      }
      if (modalState?.id !== before) return null; // moved on between the check and here
      setPromptHotkey(accel);
      return accel;
    },
  });
  $("modal-hotkey-clear").addEventListener("click", () => setPromptHotkey(""));
  $("modal-hotkey-showall").addEventListener("click", () => openAllShortcuts());
  modal.hotkeyAction.addEventListener("change", () => {
    if (modalState) modalState.hotkeyPaste = modal.hotkeyAction.value;
  });
  // Tags/hotkey/history collapsible, combined into one "Advanced options".
  modal.advancedHead.addEventListener("click", () => {
    const open = !modal.advancedBody.classList.toggle("hidden");
    modal.advancedHead.setAttribute("aria-expanded", String(open));
    modal.advanced.classList.toggle("open", open);
    // In tall (text-edit) mode the textarea flex-grows and eats the height the open
    // history needs; history-open caps it (CSS) so the list + Save row stay reachable.
    // Only relevant when history itself applies to this prompt (mode/type/flag).
    const histApplies = !$("modal-history").classList.contains("hidden");
    modal.root.classList.toggle("history-open", open && histApplies);
    scheduleFitText();
    if (open) {
      if (histApplies) loadHistory(modalState?.id);
      // Scroll the modal's own scroll container to the bottom so the freshly opened
      // section (and the Save row below it) come fully into view on small windows.
      // scrollIntoView("nearest") only nudged the already-visible head, hence unreliable.
      const scroller = modal.advanced.closest(".modal");
      if (scroller) requestAnimationFrame(() => requestAnimationFrame(() => {
        scroller.scrollTo({ top: scroller.scrollHeight, behavior: "smooth" });
      }));
    }
  });

  // Image / Text display toggle (text mode shows the name on the tile).
  modal.showImage.addEventListener("click", () => {
    if (!modalState) return;
    modalState.showImage = true;
    syncModalImageUi(modalState.mode);
  });
  modal.showText.addEventListener("click", () => {
    if (!modalState) return;
    modalState.showImage = false;
    syncModalImageUi(modalState.mode);
  });
  // Editor's own "Copy formatted" button: same resolveVars() path every other
  // copy goes through, not the raw stored text.
  modal.copyFormattedBtn.addEventListener("click", async () => {
    const text = await resolveVars(modal.text.value);
    if (text === null) return; // the placeholder dialog was cancelled
    const html = renderMarkdown(text);
    const ok = await invoke("copy_formatted", { text, html }).catch((e) => { errToast(e); return false; });
    if (ok) toast(t("copied"));
  });
  // Shared media pick: image, gif or video. Stills are stored as a preview,
  // gif/video become the icon path (shown, never copied). An image keeps the
  // prompt's copy behaviour; a gif/video icon switches copying back to text.
  async function pickReplacementMedia() {
    const session = modalSession;
    const path = await withDialog(() => invoke("pick_file_path"));
    if (!path || !modalState || session !== modalSession) return false;
    const kind = mediaKind(path);
    if (!kind) {
      toast(t("unsupportedFile"));
      return false;
    }
    if (kind === "image") {
      const data = await loadFilePreview(path);
      if (!data || !modalState || session !== modalSession) return false;
      modalState.image = data;
      modalState.iconPath = "";
    } else {
      modalState.image = "";
      modalState.iconPath = path;
      modalState.copyImage = false;
    }
    modalState.showImage = true;
    return true;
  }
  // "Replace media" (preview exists) and "Choose a tile picture" (no media yet).
  modal.replaceImg.addEventListener("click", async () => {
    if (modalState && (await pickReplacementMedia())) syncModalImageUi(modalState.mode);
  });
  modal.addIcon.addEventListener("click", async () => {
    if (modalState && (await pickReplacementMedia())) syncModalImageUi(modalState.mode);
  });
  modal.removeImg.addEventListener("click", () => {
    if (!modalState) return;
    modalState.image = "";
    modalState.iconPath = "";
    modalState.showImage = false;
    syncModalImageUi(modalState.mode);
  });

  // Replacing the attached file re-classifies the prompt by file type:
  // stills/gifs/videos switch to the media layout, anything else to plain file.
  modal.replaceFile.addEventListener("click", async () => {
    if (!modalState) return;
    const session = modalSession;
    const path = await withDialog(() => invoke("pick_file_path"));
    if (!path || !modalState || session !== modalSession) return;
    modalState.filePath = path;
    modalState.pdfPage = 0; // a different file starts back at its own first page
    const kind = mediaKind(path);
    const data = await loadFilePreview(path);
    if (!modalState || session !== modalSession) return;
    modalState.image = data;
    modalState.copyImage = false; // the file itself is what gets copied
    modalState.showImage = !!kind;
    syncModalImageUi(modalState.mode);
  });

  // Re-requests the preview at the chosen page and saves it into pdf_page:
  // the field only ever shows for a PDF attachment (syncModalImageUi).
  // Request token -- a fast second page change must not let the slower
  // older reply land after the newer one and overwrite its already-fresher preview.
  let pdfPageReq = 0;
  modal.pdfPage.addEventListener("change", async () => {
    if (!modalState) return;
    const session = modalSession;
    const page = Math.max(0, (Number(modal.pdfPage.value) || 1) - 1);
    modalState.pdfPage = page;
    const req = ++pdfPageReq;
    const data = await loadFilePreview(modalState.filePath, page);
    if (req !== pdfPageReq || !modalState || session !== modalSession) return;
    if (data) { modalState.image = data; modal.img.src = data; }
  });

  // Delete from the edit dialog: goes to the trash at once, Undo brings it back.
  modal.delete.addEventListener("click", async () => {
    if (!modalState || modalState.mode !== "edit") return;
    const id = modalState.id;
    const typedName = modal.name ? modal.name.value : "";
    if (!(await trashPromptWithUndo(id, typedName))) return; // keep the editor open so the user can retry
    closeModal();
    await renderGrid();
    if (!libraryEl.classList.contains("hidden")) renderLibrary();
  });
  modal.root.addEventListener("pointerdown", async (e) => {
    if (e.target !== modal.root) return;
    if (await confirmDiscardIfDirty()) closeModal();
  });

  // Any window's copy of a rotating prompt lands here: refresh every tile of
  // that id (usually one, but "prompt in multiple views" can place several).
  listen("rotation-changed", (e) => {
    const { id, index, pos, total, text } = e.payload || {};
    if (!id) return;
    // Invalidates any in-flight primeRotateState peek for this id, so its reply
    // can no longer overwrite the fresher position this push just delivered.
    rotateReqs.set(id, (rotateReqs.get(id) || 0) + 1);
    rotateState.set(id, { index, pos, total, text });
    document.querySelectorAll(`.tile[data-id="${CSS.escape(id)}"]`).forEach((tile) => applyRotateState(tile, id));
  });
  // Auto-advance session start/end: drives the badge's is-auto accent and,
  // on start only, a toast (no toast on end: that would just be noise).
  listen("rotation-auto", (e) => {
    const { id, active } = e.payload || {};
    if (!id) return;
    if (active) autoActiveIds.add(id); else autoActiveIds.delete(id);
    document.querySelectorAll(`.tile[data-id="${CSS.escape(id)}"]`).forEach((tile) => applyAutoBadge(tile, id));
    if (active && val("bubbleMs") > 0) toast(t("rotateAutoStarted"));
  });
  listen("rotation-auto-blocked", () => {
    toast(t("rotateAutoBlocked"));
  });

  attachSelectPicker(modal.sizeSel);
  attachSelectPicker(modal.captionSize);
  attachSelectPicker(modal.hotkeyAction);
  attachSelectPicker(modal.rotateMode);
  modal.caption.addEventListener("input", updateCaptionPreview);
  modal.captionSize.addEventListener("change", updateCaptionPreview);

  attachSelectPicker(modal.fontSel);
}

// Drag & drop onto the window: a dropped file/image/PDF becomes a button right
// away (same dialog as the paperclip); dropped text fills the composer and opens
// the save dialog. Extends the existing clipboard/screenshot paths.
export function setupDragDrop() {
  const wv = window.__TAURI__?.webview?.getCurrentWebview?.();
  wv?.onDragDropEvent?.((e) => {
    if (!flag("dragDrop")) return;
    const kind = e.payload?.type;
    document.body.classList.toggle("drag-over", kind === "enter" || kind === "over");
    if (kind === "drop") {
      document.body.classList.remove("drag-over");
      const path = e.payload?.paths?.[0];
      if (!path) return;
      // A drop replaces whatever the editor is showing with a brand-new create
      // dialog -- if it is already open with unsaved changes, ask first, same as
      // the X button/backdrop/Escape.
      confirmDiscardOrProceed(() => openFileCreate(path).catch((err) => errToast(err)));
    }
  }).catch?.(() => {});
  // Text dropped from another app (no file paths): native handler ignores it.
  window.addEventListener("dragover", (e) => e.preventDefault());
  window.addEventListener("drop", (e) => {
    if (!flag("dragDrop")) return;
    const text = e.dataTransfer?.getData("text");
    if (text && !(e.dataTransfer.files && e.dataTransfer.files.length)) {
      e.preventDefault();
      confirmDiscardOrProceed(() => {
        inputEl.value = text;
        autoGrow(inputEl);
        saveBtn.disabled = !text.trim();
        startCreate();
      });
    }
  });
}

export function wireEditor() {
  $("modal-var-add").addEventListener("click", insertHelperVar);
  for (const id of ["modal-var-name", "modal-var-opts"]) {
    $(id).addEventListener("keydown", (e) => {
      if (e.key !== "Enter") return;
      e.preventDefault(); // Enter here inserts, it must not submit the editor
      insertHelperVar();
    });
  }
}
