// Expert-menu notices and plain action rows.
import { EXPERT_ACTIONS, ICON_WARN, putIcon, t } from "./core.js";
import { dataDirLockStatus, hotkeyRow, jumpToExpertSetting, settingsLocked, settingsWriteFailed, storeSealed } from "./expertMenu.js";

// ---- Expert-menu notices -------------------------------------------------
// A notice is not a setting: it reports a state the app is currently in, has nothing to
// switch, and appears only while that state holds. Notices render at the top of their group so
// the state is read before the controls it affects.
//
// There is deliberately no switch to hide the unsealed-store notice: a warning describing what
// is on disk right now cannot be hideable, because a warning nobody can see is worth nothing.
// This is the one exception to "every feature gets a toggle".
export const EXPERT_NOTICES = {
  storeUnsealed: {
    when: () => !storeSealed,
    title: "storeUnsealedRow",
    body: "storeUnsealedRowNote",
    // Leads to the one control that reacts to this warning: turning individual entries
    // confidential encrypts them with their own password, independent of DPAPI. The button sits
    // next to the notice, not inside it, since a notice row stays plain text with no control (see
    // noticeActionRow).
    action: { label: "storeUnsealedAction", run: () => jumpToExpertSetting("secretPrompts", "flags") },
  },
  settingsLocked: {
    when: () => settingsLocked,
    title: "settingsLockedRow",
    body: "settingsLockedRowNote",
  },
  settingsWriteFailed: {
    when: () => settingsWriteFailed,
    title: "settingsWriteFailedRow",
    body: "settingsWriteFailedRowNote",
  },
  dataDirReadOnly: {
    when: () => dataDirLockStatus.readOnly,
    title: "dataDirReadOnlyRow",
    // The only notice whose text needs a value filled in: noticeRow() fills {host} for every
    // notice, a no-op for the others.
    body: "dataDirReadOnlyRowNote",
  },
};

export function noticeRow(id) {
  const def = EXPERT_NOTICES[id];
  const row = document.createElement("div");
  row.className = "expert-notice";
  row.dataset.notice = id;
  row.setAttribute("role", "status");
  const icon = document.createElement("span");
  icon.className = "expert-notice-icon";
  putIcon(icon, ICON_WARN);
  const text = document.createElement("div");
  const head = document.createElement("strong");
  head.textContent = t(def.title);
  const body = document.createElement("span");
  body.textContent = t(def.body).replace("{host}", () => dataDirLockStatus.otherHost || "?");
  text.append(head, body);
  row.append(icon, text);
  return row;
}

// A notice stays a notice with no control inside it (smoke_test.py checks the
// row itself carries no input/button/select of any kind): but the group MAY
// place a plain button right after it, pointing at the one setting that
// actually addresses it. storeUnsealed is the first to use this.
export function noticeActionRow(id) {
  const def = EXPERT_NOTICES[id];
  if (!def?.action) return null;
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "ghost-btn expert-notice-action";
  btn.textContent = t(def.action.label);
  btn.addEventListener("click", () => def.action.run());
  return btn;
}

export function actionRow(a) {
  if (a.type === "hotkey") return hotkeyRow(a.hotkeyKey, a.label);
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "ghost-btn expert-action" + (a.danger ? " danger-btn expert-action-danger" : "");
  btn.dataset.action = a.id;
  btn.textContent = t(a.label);
  if (a.tip) btn.title = t(a.tip);
  // Passing the button itself lets a long-running action (ffmpeg-download) show progress in its
  // own label and repurpose a second click as cancel; other handlers ignore the extra argument.
  btn.addEventListener("click", () => EXPERT_ACTIONS[a.id]?.(btn));
  if (!a.hint) return btn;
  // A button with an explaining line under it travels as one row.
  const block = document.createElement("div");
  block.className = "expert-action-block";
  const hint = document.createElement("div");
  hint.className = "hint";
  hint.textContent = t(a.hint);
  block.append(btn, hint);
  return block;
}
