// Hotkey recording and the hotkey list.
import { $, errToast, EXPERT_ACTIONS, flag, invoke, modal, modalState, prompts, settings, settingsEl, t, toast } from "./core.js";
import { hotkeyClearX, openAllShortcuts } from "./windowChrome.js";
import { assignSettingsOpener } from "./settingsPanel.js";
import { showExpertPage } from "./expertMenu.js";

// Build a Tauri accelerator ("CommandOrControl+Shift+P") from a keydown event.
// Returns null unless at least one modifier plus a real key was pressed.
export function accelFromEvent(e) {
  if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return null;
  const mods = [];
  if (e.ctrlKey) mods.push("CommandOrControl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  if (!mods.length) return null;
  let key = e.key;
  if (key === " ") key = "Space";
  else if (key.startsWith("Arrow")) key = key.slice(5);
  else if (key.length === 1) key = key.toUpperCase();
  return [...mods, key].join("+");
}

// Turn a stored Tauri accelerator ("CommandOrControl+Shift+P") into a readable,
// localized label ("Strg + Umschalt + P"). Display only: the raw accelerator is
// what stays saved and registered.
export const MOD_LABELS = {
  CommandOrControl: () => t("keyCtrl"), Control: () => t("keyCtrl"),
  Shift: () => t("keyShift"),
  Alt: () => "Alt", Option: () => "Alt",
  Super: () => "Win", Meta: () => "Win", Command: () => "Win", Cmd: () => "Win",
};
export function prettyAccel(accel) {
  if (!accel) return "";
  return accel.split("+").map((p) => (MOD_LABELS[p] ? MOD_LABELS[p]() : p)).join(" + ");
}

// ---- Per-prompt hotkey (editor row) ----
// The capture field mirrors the global-hotkey setting: click it, press a combo. The
// binding is stored with the prompt and only reaches the OS on save, so a cancelled
// edit changes nothing.
export const promptHotkeysOn = () => flag("promptHotkeys");

// The backend answers a rejected accelerator with a code so the reason can be shown
// in the app language; anything else passes through unchanged. "save-failed" isn't a
// hotkey code at all (update_prompt's generic write failure) but reaches this same
// mapper from Item-1's popup rows, so it gets the same friendly text the editor's
// own save path already shows for it.
export const HOTKEY_ERRORS = {
  "hotkey-invalid": "promptHotkeyInvalid",
  "hotkey-taken": "promptHotkeyTaken",
  "hotkey-system": "promptHotkeySystem",
  "hotkey-conflict": "promptHotkeyConflict",
};
export const hotkeyMessage = (err) =>
  HOTKEY_ERRORS[err] ? t(HOTKEY_ERRORS[err]) : err === "save-failed" ? t("persistFailed") : t("hotkeyFail");

// Show an inline reason and drop the capture; empty text clears the message again.
// Whatever the reason, the full list of what is already bound is one click away
// through the same button -- naming the owner is not enough on its own when the
// fix is to open that other binding and change it.
export function showHotkeyError(msg) {
  modal.hotkeyErr.textContent = msg || "";
  modal.hotkeyErr.classList.toggle("hidden", !msg);
  const showAll = $("modal-hotkey-showall");
  if (showAll) showAll.classList.toggle("hidden", !msg);
}

// What a hotkey without its own choice inherits: named on the row, so "Default"
// never leaves the user guessing which of the two it means.
export const hotkeyPasteDefault = () => (flag("promptHotkeyPaste") ? t("promptHotkeyPaste") : t("promptHotkeyCopy"));

export function renderPromptHotkey() {
  const accel = modalState?.hotkey || "";
  const pretty = prettyAccel(accel);
  modal.hotkeyInput.value = pretty;
  modal.hotkeyBadge.textContent = pretty;
  modal.hotkeyBadge.classList.toggle("hidden", !pretty);
  const sel = modal.hotkeyAction;
  if (sel) {
    sel.value = modalState?.hotkeyPaste || "";
    // The inherited mode is spelled out on the default entry, and the whole row is
    // pointless without a binding to act on.
    const def = sel.querySelector('option[value=""]');
    if (def) def.textContent = t("promptHotkeyActionDefault").replace("{mode}", () => hotkeyPasteDefault());
    modal.hotkeyActionRow.classList.toggle("hidden", !accel);
  }
  showHotkeyError("");
}

// Which combinations are already spoken for: the app hotkey and every OTHER prompt.
// Compared on the raw accelerator, normalized the way the backend does it.
export const accelKey = (a) => String(a || "").trim().toLowerCase()
  .split("+").map((s) => s.trim()).filter(Boolean)
  .map((s) => (["commandorcontrol", "commandorctrl", "cmdorctrl", "cmdorcontrol", "control"].includes(s) ? "ctrl"
    : ["command", "cmd", "meta", "win"].includes(s) ? "super"
      : s === "option" ? "alt" : s))
  .sort().join("+");

// Who already holds this combination inside the app: the app hotkey, another
// prompt, or nobody. Naming the owner is the difference between "taken" and a
// message the user can act on. `excludeId` is the prompt (if any) THIS field
// belongs to, so its own current binding never reads back as "already taken".
export function hotkeyOwner(accel, excludeId = null) {
  const key = accelKey(accel);
  if (!key) return null;
  if (accelKey(settings.hotkey) === key) return { app: true };
  if (accelKey(settings.voice_hotkey) === key) return { voice: true };
  if (accelKey(settings.rec_stop_hotkey) === key) return { recStop: true };
  if (accelKey(settings.rec_pause_hotkey) === key) return { recPause: true };
  const p = prompts.find((x) => x.id !== excludeId && accelKey(x.hotkey) === key);
  return p ? { name: p.name } : null;
}
export const hotkeyTakenMsg = (owner) =>
  owner.app ? t("promptHotkeyTakenApp")
    : owner.voice ? t("promptHotkeyTakenVoice")
    : owner.recStop ? t("promptHotkeyTakenRecStop")
    : owner.recPause ? t("promptHotkeyTakenRecPause")
    : t("promptHotkeyTakenBy").replace("{name}", () => owner.name);

// Global hotkeys have to be off while a field is capturing: Windows hands a
// registered combination to the owning process, not to the focused window, so the
// key press would never reach the field: the prompt behind it would simply be
// copied, with no error to show for it.
export let hotkeyCaptureOn = false;
export function hotkeyCapture(on) {
  if (on === hotkeyCaptureOn) return;
  hotkeyCaptureOn = on;
  invoke("set_hotkey_capture", { active: on }).catch(() => {});
}
// Storing a hotkey re-arms the whole set, so a field that still has the focus has
// to take them back off straight away.
export function reassertHotkeyCapture() {
  if (!hotkeyCaptureOn) return;
  hotkeyCaptureOn = false;
  hotkeyCapture(true);
}

// ONE reusable hotkey-recording field, wired onto `input`: click, press a
// combo. A client-side pre-check (hotkeyOwner) gives instant feedback; the BACKEND
// re-checks before anything is actually kept (only it knows the system blocklist
// and can ask Windows whether another app already owns the combination), so a
// refusal here always leaves the previous binding exactly as it was.
//   id: the prompt this field belongs to (null for the global hotkey), or a
//             function returning it fresh each time, for a field reused across
//             different prompts (the singleton editor).
//   errEl: element for an inline error message; omitted falls back to a toast.
//   current: () => the accel to restore the field to after a refused capture.
//   onCommit(accel): persists (or, for the editor, only stages) the change; must
//             throw the backend's rejection code on failure, and returns the accel
//             actually stored (not always what was requested: see the note
//             on update_prompt's own late OS-conflict check). Returning `null`
//             aborts silently without touching the field or showing an error: the
//             editor uses this when the user moved to a different prompt while an
//             await was in flight, so a stale answer never overwrites what is by
//             then a completely different prompt's row.
export function bindHotkeyField(input, { id = null, errEl = null, showAllEl = null, current, onCommit }) {
  const resolveId = () => (typeof id === "function" ? id() : id);
  const showErr = (msg) => {
    if (errEl) { errEl.textContent = msg || ""; errEl.classList.toggle("hidden", !msg); }
    else if (msg) toast(msg);
    if (showAllEl) showAllEl.classList.toggle("hidden", !msg);
  };
  // A commit is a round trip (check_hotkey, maybe update_prompt); a second keypress
  // landing before the first resolves could otherwise finish out of order and
  // clobber the newer request with the older one's result.
  let busy = false;
  input.addEventListener("focus", () => hotkeyCapture(true));
  input.addEventListener("blur", () => hotkeyCapture(false));
  input.addEventListener("keydown", async (e) => {
    if (e.key === "Tab" || e.key === "Escape") return; // let focus leave the field
    e.preventDefault();
    if (busy) return;
    const clearing = e.key === "Backspace" || e.key === "Delete";
    let accel = "";
    if (!clearing) {
      accel = accelFromEvent(e);
      if (!accel) { showErr(t("promptHotkeyInvalid")); return; }
      const owner = hotkeyOwner(accel, resolveId());
      if (owner) { showErr(hotkeyTakenMsg(owner)); return; }
    }
    showErr("");
    busy = true;
    try {
      const stored = await onCommit(accel);
      if (stored === null) return; // aborted silently, see onCommit's doc above
      input.value = prettyAccel(stored);
    } catch (err) {
      input.value = prettyAccel(current());
      showErr(hotkeyMessage(String(err)));
    } finally {
      busy = false;
      // A store re-arms every binding, even when it then fails on another one; the
      // field still has the focus, so the hotkeys have to come off again either way.
      reassertHotkeyCapture();
    }
  });
}

export function setPromptHotkey(accel) {
  if (!modalState) return;
  modalState.hotkey = accel;
  if (!accel) modalState.hotkeyPaste = ""; // no binding, no action to choose
  renderPromptHotkey();
}

// ---- "All hotkeys": global + every per-prompt hotkey, each row live ----

// The ONE place that actually persists the global hotkey: shared by the Settings
// panel's own field and the popup's global row, so they can never disagree.
export async function commitGlobalHotkey(accel) {
  await invoke("set_hotkey", { hotkey: accel });
  settings.hotkey = accel;
  if (accel) toast(t("hotkeySet"));
  renderHotkeysList(); // a no-op while the popup is closed
  return accel;
}
// The dictation hotkey's counterpart, same shape -- its own field, gated behind the voiceHotkey
// expert flag, checked through the same set_voice_hotkey/check_hotkey backend
// path (hotkeyOwner above already treats it as a real collision).
export async function commitVoiceHotkey(accel) {
  await invoke("set_voice_hotkey", { hotkey: accel });
  settings.voice_hotkey = accel;
  if (accel) toast(t("hotkeySet"));
  return accel;
}
// The recording stop hotkey, same shape again -- only ever actually registered while a
// recording runs (see rearm_hotkeys on the backend), so a successful commit
// here can still show as blocked later through the bar's own warning icon.
export async function commitRecStopHotkey(accel) {
  await invoke("set_rec_stop_hotkey", { hotkey: accel });
  settings.rec_stop_hotkey = accel;
  if (accel) toast(t("hotkeySet"));
  return accel;
}
export async function commitRecPauseHotkey(accel) {
  await invoke("set_rec_pause_hotkey", { hotkey: accel });
  settings.rec_pause_hotkey = accel;
  if (accel) toast(t("hotkeySet"));
  return accel;
}

// Unlike the editor (which only stages a hotkey until Save), a popup row has no
// save step of its own: it writes straight through update_prompt with every OTHER
// field taken from the prompt's own in-memory copy, unchanged. The text and the
// rotation entries are deliberately NOT sent: for a confidential prompt the
// cached copy can be empty (vault locked) or stale, and a missing field means
// "keep what is stored". check_hotkey runs
// first so a live OS conflict shows up immediately instead of only being caught by
// update_prompt's own late rearm (see the "the backend hands back what it
// really stored" note at the editor's save path).
export async function commitPromptHotkeyDirect(id, accel) {
  const p = prompts.find((x) => x.id === id);
  if (!p) throw "save-failed";
  if (accel) await invoke("check_hotkey", { hotkey: accel, id });
  const saved = await invoke("update_prompt", {
    id: p.id, name: p.name, color: p.color,
    // No image field: the prompt list no longer carries the bytes, and a missing
    // field means "keep the stored picture": exactly what this write wants.
    showImage: p.show_image, copyImage: p.copy_image,
    filePath: p.file_path, pdfPage: p.pdf_page, iconPath: p.icon_path,
    caption: p.caption, captionSize: p.caption_size, font: p.font, fontSize: p.font_size,
    favorite: p.favorite, tags: p.tags, notes: p.notes,
    hotkey: accel, hotkeyPaste: accel ? p.hotkey_paste : "",
    rotateMode: p.rotate_mode || "", rotateAuto: p.rotate_auto || false,
  });
  if (!saved) throw "save-failed";
  p.hotkey = saved.hotkey;
  p.hotkey_paste = saved.hotkey_paste;
  if (accel && !saved.hotkey) toast(t("promptHotkeyFail")); // OS refused between check and save
  renderHotkeysList();
  return saved.hotkey;
}

// Tile-menu checkmark entry: same "write straight through with everything
// else unchanged" shape as commitPromptHotkeyDirect. Turning it off also ends a
// session that happens to be running right now, instead of waiting for the next
// paste to notice the prompt no longer qualifies.
export async function toggleRotateAutoDirect(id) {
  const p = prompts.find((x) => x.id === id);
  if (!p) return;
  const nextAuto = !p.rotate_auto;
  try {
    const saved = await invoke("update_prompt", {
      id: p.id, name: p.name, color: p.color,
      // Same as commitPromptHotkeyDirect: no image, text or entries = keep the stored ones.
      showImage: p.show_image, copyImage: p.copy_image,
      filePath: p.file_path, pdfPage: p.pdf_page, iconPath: p.icon_path,
      caption: p.caption, captionSize: p.caption_size, font: p.font, fontSize: p.font_size,
      favorite: p.favorite, tags: p.tags, notes: p.notes,
      hotkey: p.hotkey || "", hotkeyPaste: p.hotkey_paste || "",
      rotateMode: p.rotate_mode || "", rotateAuto: nextAuto,
    });
    if (!saved) throw "save-failed";
    p.rotate_auto = saved.rotate_auto;
    if (!p.rotate_auto) await invoke("rotation_end_auto", { id }).catch(() => {});
  } catch (err) {
    errToast(err);
  }
}

// One entry per row kind: how to read its current value and how to commit a
// new one. "prompt" rows carry their own id (used both for the lookup and to
// exclude themselves from hotkeyOwner's conflict check); the other three share
// the null-id bucket the same way their own dedicated fields always have.
export const HOTKEYS_LIST_KINDS = {
  global: { current: () => settings.hotkey, onCommit: commitGlobalHotkey },
  prompt: {
    current: (id) => prompts.find((x) => x.id === id)?.hotkey || "",
    onCommit: (accel, id) => commitPromptHotkeyDirect(id, accel),
  },
  recStop: { current: () => settings.rec_stop_hotkey, onCommit: commitRecStopHotkey },
  recPause: { current: () => settings.rec_pause_hotkey, onCommit: commitRecPauseHotkey },
};
export function renderHotkeysList() {
  const box = $("hotkeys-list");
  if (!box || $("shortcuts-global").classList.contains("hidden")) return;
  const q = ($("hotkeys-search").value || "").trim().toLowerCase();
  const rows = [{ kind: "global", id: null, name: t("hotkeysGlobalRow"), accel: settings.hotkey }]
    .concat(promptHotkeysOn() ? prompts.filter((p) => p.hotkey).map((p) => ({ kind: "prompt", id: p.id, name: p.name, accel: p.hotkey })) : [])
    .concat(flag("screenRecord") && flag("recStopHotkey") ? [{ kind: "recStop", id: null, name: t("recStopHotkeyLabel"), accel: settings.rec_stop_hotkey }] : [])
    .concat(flag("screenRecord") && flag("recPauseHotkey") ? [{ kind: "recPause", id: null, name: t("recPauseHotkeyLabel"), accel: settings.rec_pause_hotkey }] : []);
  const hit = (r) => !q || r.name.toLowerCase().includes(q) || prettyAccel(r.accel).toLowerCase().includes(q);
  const filtered = rows.filter(hit);
  box.innerHTML = "";
  if (!filtered.length) {
    const empty = document.createElement("p");
    empty.className = "hotkeys-empty";
    empty.textContent = t("hotkeysNoMatch");
    box.appendChild(empty);
    return;
  }
  for (const r of filtered) {
    const kindDef = HOTKEYS_LIST_KINDS[r.kind];
    const row = document.createElement("div");
    row.className = "hotkeys-row";
    const name = document.createElement("span");
    name.className = "hotkeys-row-name" + (r.kind === "global" ? " hotkeys-row-global" : "");
    name.textContent = r.name;
    const input = document.createElement("input");
    input.className = "modal-input";
    input.type = "text";
    input.readOnly = true;
    input.placeholder = t("hotkeyPh");
    input.value = prettyAccel(r.accel);
    row.append(name, input);
    box.appendChild(row);
    bindHotkeyField(input, {
      id: r.id,
      current: () => kindDef.current(r.id),
      onCommit: (accel) => kindDef.onCommit(accel, r.id),
    });
    hotkeyClearX(input, async () => {
      try { input.value = prettyAccel(await kindDef.onCommit("", r.id)); }
      catch (err) { toast(hotkeyMessage(String(err))); }
    });
  }
}

export function bindHotkeys() {
  // Global hotkey: click the field and press a key combo to capture it.
  bindHotkeyField($("opt-hotkey"), {
    current: () => settings.hotkey,
    onCommit: commitGlobalHotkey,
  });
  hotkeyClearX($("opt-hotkey"), async () => {
    try { await commitGlobalHotkey(""); } catch (_) { /* Clear always leaves an empty field regardless */ }
    $("opt-hotkey").value = "";
  }, "hotkey-clear");
  $("hotkey-list-btn").addEventListener("click", openAllShortcuts);
  bindHotkeyField($("opt-voice-hotkey"), {
    current: () => settings.voice_hotkey,
    onCommit: commitVoiceHotkey,
  });
  hotkeyClearX($("opt-voice-hotkey"), async () => {
    try { await commitVoiceHotkey(""); } catch (_) { /* Clear always leaves an empty field regardless */ }
    $("opt-voice-hotkey").value = "";
  }, "voice-hotkey-clear");
  EXPERT_ACTIONS["voice-hotkey-set"] = () => {
    assignSettingsOpener(document.activeElement);
    showExpertPage(false);
    settingsEl.classList.remove("hidden");
    requestAnimationFrame(() => {
      $("opt-voice-hotkey").scrollIntoView({ block: "center" });
      $("opt-voice-hotkey").focus();
    });
  };
  $("hotkeys-search").addEventListener("input", renderHotkeysList);
}
