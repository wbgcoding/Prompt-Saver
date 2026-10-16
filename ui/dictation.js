// Voice dictation and the OCR and speech language packs.
import { $, autoGrow, EMPTY_VALUE, errText, errToast, EXPERT_ACTIONS, flag, inputEl, invoke, LANG, listen, modal, settings, t, toast, txt, val } from "./core.js";
import { confirmDialog } from "./dialog.js";
import { fmtCopyTime } from "./journal.js";
import { fmtBytes } from "./floatShared.js";
import { applyFlags, assignOcrLangsCache, assignVoiceLangsCache } from "./expertMenu.js";
import { recordToolUsage } from "./management.js";
import { autoPasteNow } from "./tiles.js";

// ---- OCR + voice language pack management, one shared dialog ----
// #ocr-langs-modal serves both OCR and voice dictation packs: same markup and flow
// (Windows-reported capability list, elevated listing/install), pointed at different Tauri
// commands and settings keys. `kind` ("ocr" | "speech") selects which. *_capabilities()
// lists every capability Windows knows about (installed and not) with size and version,
// unlike ocrLangsCache/voiceLangsCache, which hold only the installed set used to pick a
// recognition language.
export const LANG_CMDS = {
  ocr: {
    capabilities: "ocr_capabilities", elevate: "ocr_capabilities_elevate",
    elevatedResult: "ocr_capabilities_elevated_result",
    install: "ocr_install_language", installResult: "ocr_install_result",
    languages: "ocr_languages",
  },
  speech: {
    capabilities: "speech_capabilities", elevate: "speech_capabilities_elevate",
    elevatedResult: "speech_capabilities_elevated_result",
    install: "speech_install_language", installResult: "speech_install_result",
    languages: "voice_languages",
  },
};
// Per-kind dialog state, so switching which dialog is open never loses the other's in-flight
// scan or install polls.
export const LANG_DIALOG = {
  // installing: the names whose elevated install was launched and has not reported back. It
  // outlives the dialog (only the pollers stop on close), so a reopened dialog still knows the
  // row is busy. installChain marks the one live poller per name.
  ocr: { cache: [], checked: 0, installTimers: {}, installPending: {}, installErrors: {}, installing: new Set(), installChain: {}, elevTimer: null, elevRunning: false, elevLeft: 60 },
  speech: { cache: [], checked: 0, installTimers: {}, installPending: {}, installErrors: {}, installing: new Set(), installChain: {}, elevTimer: null, elevRunning: false, elevLeft: 60 },
};
export let langDialogKind = "ocr"; // which kind #ocr-langs-modal currently displays
export function openLangsModal(kind) {
  langDialogKind = kind;
  $("ocr-langs-title").textContent = t(kind === "speech" ? "voiceLangsManage" : "ocrLangsManage");
  $("ocr-langs-hint").textContent = t(kind === "speech" ? "voiceLangsHint" : "ocrLangsHint");
  $("ocr-langs-modal").classList.remove("hidden");
  const st = LANG_DIALOG[kind];
  // A scan started on a previous open (or from the elevation retry button) may still be running
  // server-side; resume polling it instead of doing nothing until the user re-elevates.
  if (st.elevRunning) { renderLangsLoading(kind); pollElevatedCaps(kind); }
  else loadLangCaps(kind, false);
  // Installs started before the dialog was closed keep running as OS processes; pick their
  // result polling back up.
  for (const name of st.installing) pollLangInstall(kind, name);
}
export function closeLangsModal() {
  $("ocr-langs-modal").classList.add("hidden");
  const st = LANG_DIALOG[langDialogKind];
  // Installs and an elevated scan are OS processes and keep running; only the polling loop (and
  // the DOM it repaints) stops while nobody is looking. elevRunning stays set so reopening
  // resumes.
  for (const id of Object.values(st.installTimers)) clearTimeout(id);
  st.installTimers = {};
  st.installChain = {};
  if (st.elevTimer) { clearTimeout(st.elevTimer); st.elevTimer = null; }
}
export async function loadLangCaps(kind, force) {
  const st = LANG_DIALOG[kind];
  if (!st.cache.length || force) renderLangsLoading(kind);
  try {
    st.cache = await invoke(LANG_CMDS[kind].capabilities, { force: !!force });
    st.checked = Math.floor(Date.now() / 1000);
  } catch (e) {
    // Listing is meant to need no elevated rights, but some Windows builds require it even for
    // this read-only query: an inline reason (plus a retry) beats a blank dialog or a generic
    // toast. Any failure comes back as "code|raw text" (the backend does not classify by
    // HResult/message text, which varies by machine and locale), so the raw PowerShell/DISM
    // reason can still be shown.
    const s = String(e);
    if (s.startsWith("ocr-caps-need-elevation")) {
      renderLangsRetry(kind, t("ocrLangsNeedElevation"), s.slice(s.indexOf("|") + 1));
    } else errToast(e);
    return;
  }
  renderLangsBody(kind);
}
// Runs the capability listing through the UAC prompt on machines that refuse it to a normal
// user. Same shape as an install: explain first, launch, then poll the result the elevated
// process writes.
export async function startElevatedCapsScan(kind) {
  const ok = await confirmDialog({
    title: t("ocrElevateTitle"),
    message: t("ocrCapsElevateInfo"),
    confirmLabel: t("ocrLangsElevateBtn"),
    danger: false,
  });
  if (!ok) return;
  const st = LANG_DIALOG[kind];
  renderLangsLoading(kind);
  try {
    await invoke(LANG_CMDS[kind].elevate);
  } catch (e) {
    // A launch failure also gets the notice+retry pair, not a dead end, except when the account
    // cannot elevate at all: that gets its own explanation with no retry button, since trying
    // again can never succeed.
    const err = String(e);
    if (err === "ocr-elevate-no-rights") {
      renderLangsRetry(kind, `${t("capNeedsAdmin")} ${t("capNeedsAdminWhat")}`, null, true);
    } else {
      renderLangsRetry(kind, err === "ocr-elevate-cancelled" ? t("ocrElevateCancelled") : errText(e));
    }
    return;
  }
  st.elevRunning = true;
  st.elevLeft = 60;
  pollElevatedCaps(kind);
}
// Polls the elevated process's result file a few seconds apart. Re-entrant: it can be called
// again after the dialog was closed and reopened without losing the two-minute budget already
// spent (elevLeft lives on LANG_DIALOG, not in a closure local).
export function pollElevatedCaps(kind) {
  const st = LANG_DIALOG[kind];
  const tick = async () => {
    st.elevTimer = null;
    let res;
    try { res = await invoke(LANG_CMDS[kind].elevatedResult); }
    catch (e) { st.elevRunning = false; renderLangsRetry(kind, errText(e)); return; }
    if (res == null) {
      if (--st.elevLeft <= 0) { st.elevRunning = false; renderLangsRetry(kind, t("ocrLangsNeedElevation")); return; }
      st.elevTimer = setTimeout(tick, 2000);
      return;
    }
    st.elevRunning = false;
    st.cache = res;
    st.checked = Math.floor(Date.now() / 1000);
    renderLangsBody(kind);
  };
  st.elevTimer = setTimeout(tick, 2000);
}
export function renderLangsLoading(kind) {
  if (langDialogKind !== kind) return;
  const body = $("ocr-langs-body");
  body.innerHTML = "";
  const loading = document.createElement("div");
  loading.className = "hint";
  loading.textContent = t("ocrLangsScanning");
  body.appendChild(loading);
}
// Notice text and its retry button are always built together, so a failure (launch or the
// two-minute poll timeout) never leaves the user with text only and no way to retry.
// `details` is the raw PowerShell/DISM text behind a translated reason like `message`, shown
// collapsed so it does not crowd out the instruction. `noRetry` drops the button: set when the
// account can never elevate and another attempt cannot succeed.
export function renderLangsRetry(kind, message, details, noRetry) {
  if (langDialogKind !== kind) return;
  const body = $("ocr-langs-body");
  body.innerHTML = "";
  const note = document.createElement("div");
  note.className = "hint";
  note.textContent = message;
  body.appendChild(note);
  if (details && details.trim()) {
    const disc = document.createElement("details");
    disc.className = "lang-error-details";
    const summary = document.createElement("summary");
    summary.textContent = t("ocrLangsErrorDetails");
    disc.appendChild(summary);
    const pre = document.createElement("div");
    pre.className = "lang-error-details-text";
    pre.textContent = details.trim();
    disc.appendChild(pre);
    body.appendChild(disc);
  }
  if (noRetry) return;
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "ghost-btn";
  btn.textContent = t("ocrLangsElevateBtn");
  btn.addEventListener("click", () => startElevatedCapsScan(kind));
  body.appendChild(btn);
}
export function renderLangsBody(kind) {
  if (langDialogKind !== kind) return; // a stale async result for a dialog that moved on
  const st = LANG_DIALOG[kind];
  const body = $("ocr-langs-body");
  body.innerHTML = "";
  for (const cap of st.cache) body.appendChild(langRow(kind, cap));
  $("ocr-langs-checked").textContent = st.checked
    ? t("ocrLangsLastChecked").replace("{t}", fmtCopyTime(st.checked))
    : t("ocrLangsNeverChecked");
}
export function langRow(kind, cap) {
  const st = LANG_DIALOG[kind];
  const row = document.createElement("div");
  row.className = "clip-row ocr-lang-row lang-row";
  const main = document.createElement("div");
  main.className = "lang-row-main";
  const name = document.createElement("div");
  name.className = "lang-row-name";
  name.textContent = cap.label;
  const sub = document.createElement("div");
  sub.className = "lang-row-sub";
  sub.textContent = `${cap.tag} · ${cap.version || EMPTY_VALUE}`;
  main.append(name, sub);
  const installed = cap.state === "Installed";
  const size = installed ? cap.install_size : cap.download_size;
  const sizeEl = document.createElement("div");
  sizeEl.className = "lang-row-size";
  // Windows does not always report a size; never guess one, show the dash.
  sizeEl.textContent = size == null ? EMPTY_VALUE : fmtBytes(size, LANG);
  const chip = document.createElement("span");
  chip.className = installed ? "lang-row-chip installed" : "lang-row-chip";
  chip.textContent = installed ? t("ocrLangsInstalled") : t("ocrLangsNotInstalled");
  const actions = document.createElement("div");
  actions.className = "clip-actions";
  row.append(main, sizeEl, chip, actions);
  if (installed) {
    const check = document.createElement("span");
    check.className = "ocr-lang-check";
    check.textContent = "✓";
    check.title = t("ocrLangsInstalled");
    actions.appendChild(check);
    return row;
  }
  const note = st.installErrors[cap.name];
  if (note) {
    const hint = document.createElement("span");
    hint.className = "ocr-lang-note";
    // "no-rights" is the account-can-never-elevate case with its own text below; any other string
    // is the backend's reason (a real install failure); `true` is the calm "you declined the
    // rights prompt" case.
    hint.textContent =
      note === "no-rights" ? `${t("capNeedsAdmin")} ${t("capNeedsAdminWhat")}` : typeof note === "string" ? note : t("ocrElevateCancelled");
    actions.appendChild(hint);
  }
  if (note === "no-rights") return row; // a retry can never succeed here -- no download button
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "ghost-btn";
  const busy = st.installing.has(cap.name) || !!st.installTimers[cap.name] || !!st.installPending[cap.name];
  btn.disabled = busy;
  // The size sits inside the button label on purpose, so nobody starts a download blind.
  btn.textContent = busy
    ? t("ocrLangsInstalling")
    : t("ocrLangsDownloadBtn").replace("{size}", () => cap.download_size == null ? EMPTY_VALUE : fmtBytes(cap.download_size, LANG));
  btn.addEventListener("click", () => startLangInstall(kind, cap));
  actions.appendChild(btn);
  return row;
}
// Every path that can lead to an elevated install goes through here first: explain the
// language, its size and that Windows will ask for admin rights once, before that prompt
// appears.
export async function startLangInstall(kind, cap) {
  const st = LANG_DIALOG[kind];
  const size = cap.download_size == null ? EMPTY_VALUE : fmtBytes(cap.download_size, LANG);
  const ok = await confirmDialog({
    title: t("ocrElevateTitle"),
    message: t("ocrElevateInfo").replace("{lang}", () => cap.label).replace("{size}", () => size),
    confirmLabel: t("ocrElevateInstall"),
    danger: false,
  });
  if (!ok) return;
  delete st.installErrors[cap.name];
  // Marks the row busy for the whole gap, including the elevated install() call (which blocks
  // until the user answers the UAC prompt). Set synchronously before any await, so a second
  // click on the still-enabled button cannot start a second concurrent elevated install.
  st.installPending[cap.name] = true;
  renderLangsBody(kind);
  try {
    await invoke(LANG_CMDS[kind].install, { name: cap.name });
  } catch (e) {
    delete st.installPending[cap.name];
    // A declined UAC prompt is not an error: a calm inline note on the row, the row stays "not
    // installed". An account that cannot elevate gets its own note and no download button.
    const err = String(e);
    if (err === "ocr-elevate-no-rights") st.installErrors[cap.name] = "no-rights";
    else if (err === "ocr-elevate-cancelled") st.installErrors[cap.name] = true;
    else errToast(e);
    // Every branch above deleted installPending; the row rendered disabled ("Installing...")
    // before the await must be repainted whichever branch ran, or the button stays disabled until
    // the dialog is reopened.
    renderLangsBody(kind);
    return;
  }
  delete st.installPending[cap.name];
  // Arm the poller (marks the row busy) before repainting, or the render sees an empty
  // installTimers[cap.name] and shows an enabled Download button, and a second click starts a
  // second UAC-elevated install.
  pollLangInstall(kind, cap.name);
  renderLangsBody(kind);
}
// *_install_language() resolves once the elevated process has launched, not finished; poll
// the result file ourselves, a few seconds apart, until it stops being null. A result file
// that never becomes readable (corrupt, or wiped under the install) would look like "still
// running" forever, so the polling has a limit: ~10 minutes is generous even for a large
// language pack on a slow connection, so hitting it is always a real stuck state.
export const MAX_LANG_INSTALL_POLLS = 200;
export function pollLangInstall(kind, name) {
  const st = LANG_DIALOG[kind];
  if (st.installTimers[name]) return;
  st.installing.add(name);
  const chain = {};
  st.installChain[name] = chain;
  let attempts = 0;
  // setTimeout re-armed after each result, not setInterval: the callback awaits an invoke, and
  // a reply slower than the 3s tick would overlap itself.
  const tick = async () => {
    attempts++;
    let res;
    try { res = await invoke(LANG_CMDS[kind].installResult, { name }); } catch (_) { res = undefined; }
    // The dialog was closed (and maybe reopened with a fresh poller) while that answer was on its
    // way: this chain is stale and must not re-arm itself. A terminal answer is still handled,
    // since the backend already deleted the result and dropped its nonce.
    const stale = st.installChain[name] !== chain;
    if (res == null) {
      if (stale) return;
      if (attempts >= MAX_LANG_INSTALL_POLLS) {
        delete st.installTimers[name];
        st.installing.delete(name);
        st.installErrors[name] = t("ocrInstallResultTimeout");
        renderLangsBody(kind);
        return;
      }
      st.installTimers[name] = setTimeout(tick, 3000); // still running (or a transient probe failure)
      return;
    }
    // A poller started by a reopen in the meantime has nothing left to wait for.
    clearTimeout(st.installTimers[name]);
    delete st.installTimers[name];
    delete st.installChain[name];
    st.installing.delete(name);
    if (res.ok) {
      // Clear both caches so the new pack is selectable immediately, no restart.
      invoke(LANG_CMDS[kind].languages).then((langs) => {
        if (kind === "speech") assignVoiceLangsCache(langs || []); else assignOcrLangsCache(langs || []);
      }).catch(() => {});
      await loadLangCaps(kind, true); // force -- also drops the backend's own cache
      return;
    }
    // A real failure (e.g. Windows Home or policy blocked): the reason belongs in the row. The
    // backend's internal keys are translated here; anything else is a raw Windows exception
    // message, shown as Windows worded it.
    st.installErrors[name] = res.error === "install-result-unreadable" ? t("ocrInstallResultUnreadable") : (res.error || true);
    renderLangsBody(kind);
  };
  st.installTimers[name] = setTimeout(tick, 3000);
}
// Thin OCR-flavoured wrapper so the expert-action entry below keeps working.
export function openOcrLangsModal() { openLangsModal("ocr"); }
export function openVoiceLangsModal() { openLangsModal("speech"); }

// "ocr-no-language" and "voice-no-language" always have the same fix: open this dialog. Used
// at every call site that would otherwise show them as a bare errToast().
export function ocrErrToast(err) {
  if (String(err) === "ocr-no-language") {
    toast(t("ocrNoLanguageMsg"), { label: t("ocrLangsManage"), onClick: openOcrLangsModal });
    return;
  }
  errToast(err);
}

// ---- Voice dictation session control ----
export let voiceSessionId = null;
export let voiceToken = 0; // frontend-chosen, sent to voice_start and echoed on every event -- the only identity events are matched on
export let voiceState = "idle"; // idle | arming | listening
export let voiceTargetEl = null;
export let voiceActiveBtn = null;
export let voiceInterim = null; // {start, len} inside voiceTargetEl.value, or null
export let voiceSuppressInput = false; // true while a programmatic .value write is in flight
export let voiceResolvedLang = ""; // BCP-47 tag voice_start actually recognized with, for command post-processing
export let voiceArmCancelled = false; // stopVoice() was called before voice_start's invoke() resolved
// The just-ended session's target, for one late voice-text that WinRT's unordered
// ResultGenerated/Completed handlers let land after voice-stopped: delivered instead of
// dropped, but only within the grace window and only to its own target, never adopted by a
// session started in the meantime. Internal timing, not an expert setting.
export let voicePrev = null; // {token, el, caret, lang, hotkey, endedAt, interim} | null
export const VOICE_LATE_GRACE_MS = 3000;

// The two-letter code voice.js's command tables are keyed by, derived from the BCP-47 tag
// voice_start resolved to, not the UI display language (a German UI may have only an English
// speech pack). Falls back to the UI language before a session has resolved one (the hotkey
// path's first partial can race ahead of it).
export function voiceLangCode() {
  return (voiceResolvedLang || LANG).slice(0, 2).toLowerCase();
}
export function voiceButtonsAll() { return [$("voice-btn"), modal.voiceBtn].filter(Boolean); }
export function setVoiceBtnState(activeBtn, cls) {
  for (const b of voiceButtonsAll()) {
    b.classList.remove("is-arming", "is-listening");
    b.setAttribute("aria-pressed", "false");
    b.title = t("voiceBtnTitle");
  }
  if (!activeBtn || !cls) return;
  activeBtn.classList.add(cls);
  if (cls === "is-listening") {
    activeBtn.setAttribute("aria-pressed", "true");
    activeBtn.title = t("voiceStopTitle");
  }
}
// The button stays in its "arming" state (so a fast double-click cannot start a second
// session) until the first backend event proves the session is alive.
export function resolveVoiceArming() {
  if (voiceState === "arming") {
    voiceState = "listening";
    setVoiceBtnState(voiceActiveBtn, "is-listening");
  }
}
export function endVoiceUi() {
  if (voiceTargetEl) {
    voicePrev = {
      token: voiceToken,
      el: voiceTargetEl,
      caret: voiceTargetEl.selectionEnd ?? null,
      // The live partial still sitting in the field: the final text of this session replaces it
      // instead of landing next to it.
      interim: voiceInterim
        ? { start: voiceInterim.start, len: voiceInterim.len, text: voiceTargetEl.value.substr(voiceInterim.start, voiceInterim.len) }
        : null,
      lang: voiceResolvedLang,
      hotkey: voiceTargetEl === voiceHotkeyBuffer,
      endedAt: Date.now(),
    };
  }
  voiceState = "idle";
  voiceSessionId = null;
  voiceTargetEl = null;
  voiceInterim = null;
  voiceResolvedLang = "";
  setVoiceBtnState(null, null);
  voiceActiveBtn = null;
}
export async function startVoice(target, btn) {
  if (voiceState !== "idle" || !target) return;
  voiceState = "arming";
  voiceTargetEl = target;
  voiceActiveBtn = btn;
  voiceInterim = null;
  voiceArmCancelled = false;
  voiceToken = (voiceToken % 1e9) + 1;
  setVoiceBtnState(btn, "is-arming");
  const myToken = voiceToken;
  let res;
  try {
    res = await invoke("voice_start", {
      lang: txt("voiceLang") || "auto",
      autoStopSec: flag("voiceAutoStop") ? Math.round(val("voiceAutoStopSec")) : 0,
      maxSec: Math.round(val("voiceMaxMin")) * 60,
      interim: flag("voiceInterim"),
      token: voiceToken,
    });
  } catch (e) {
    endVoiceUi();
    routeVoiceError(e);
    return;
  }
  recordToolUsage("dictation");
  const { id, lang } = res;
  // A cancel during this call (voice_start's spawn_blocking round trip is not instant) must
  // stop the session it just learned the id of; stopVoice() itself had nothing to send.
  if (voiceArmCancelled) {
    voiceArmCancelled = false;
    invoke("voice_stop", { id }).catch(() => {});
    // Reset the UI only if no backend event has already adopted this session (see
    // voiceEventIsMine); if one has, its own listener owns the reset.
    if (voiceSessionId === null && voiceState === "arming") endVoiceUi();
    return;
  }
  // A session that ended (or was replaced) before this reply was processed has nothing to
  // adopt: a dead id stored while idle would never be overwritten, and the next stop would go
  // to it.
  if (voiceState === "idle" || myToken !== voiceToken) return;
  if (voiceSessionId === null) voiceSessionId = id;
  voiceResolvedLang = lang || voiceResolvedLang;
}
export async function stopVoice() {
  if (voiceState === "arming" && voiceSessionId === null) {
    // Nothing to send yet: voice_start has not resolved with an id. Mark the cancel and let
    // startVoice() finish the job when it does.
    voiceArmCancelled = true;
    return;
  }
  if (!voiceSessionId) return;
  try { await invoke("voice_stop", { id: voiceSessionId }); } catch (_) {}
  // The UI resets from the voice-stopped event the backend always sends back, not here, so the
  // button never claims "off" before the session (and its mic) has finished.
}
// True if a token-tagged backend event belongs to the session currently starting or running.
// voiceToken is chosen by the frontend and sent to voice_start, so it is known before any
// event can arrive (unlike the backend-assigned id). The two WinRT handlers (ResultGenerated,
// Completed) are not ordered against each other, so a session that reported voice-stopped can
// still deliver one more voice-text; its token no longer matches here and it falls to
// deliverLateVoiceText instead of being misattributed to a session now starting.
export function voiceEventIsMine(payload) {
  return voiceState !== "idle" && !!payload && payload.token === voiceToken;
}
// A late voice-text for the session endVoiceUi just closed out (see voicePrev): delivered to
// its own target within VOICE_LATE_GRACE_MS instead of dropped, and never touching
// voiceState/voiceSessionId/the buttons, since no live session is left.
export function deliverLateVoiceText(payload) {
  const prev = voicePrev;
  if (!prev || !payload || payload.token !== prev.token) return;
  if (Date.now() - prev.endedAt >= VOICE_LATE_GRACE_MS) return;
  voicePrev = null;
  const text = payload.text || "";
  if (!text) return;
  const processed = applyVoiceCommands(text, {
    punct: flag("voicePunct"),
    autoCaps: flag("voiceAutoCaps"),
    lang: (prev.lang || LANG).slice(0, 2).toLowerCase(),
  });
  if (prev.hotkey) {
    // The buffer was held back because it still carried the live partial of this phrase: the
    // final text takes the partial's place and goes out once.
    const held = voiceHotkeyHold;
    const part = prev.interim;
    if (held && part && held.text.substr(part.start, part.len) === part.text) {
      clearTimeout(held.timer);
      voiceHotkeyHold = null;
      deliverVoiceHotkeyText(held.text.slice(0, part.start) + processed + held.text.slice(part.start + part.len));
      return;
    }
    flushVoiceHotkeyHold();
    deliverVoiceHotkeyText(processed);
    return;
  }
  const el = prev.el;
  if (!el || el.classList.contains("hidden")) return;
  const part = prev.interim;
  if (part && el.value.substr(part.start, part.len) === part.text) {
    // Still exactly the partial the session left behind: swap it for the final text.
    el.value = el.value.slice(0, part.start) + processed + el.value.slice(part.start + part.len);
    el.selectionStart = el.selectionEnd = part.start + processed.length;
    el.dispatchEvent(new Event("input", { bubbles: true }));
    return;
  }
  const atEnd = txt("voiceInsert") === "end";
  const at = atEnd ? el.value.length : (prev.caret ?? el.value.length);
  insertLateVoiceText(el, processed, at);
}
export function wireVoiceButton(btn, targetFn) {
  if (!btn) return;
  btn.addEventListener("click", () => {
    if (voiceState !== "idle") { stopVoice(); return; }
    startVoice(targetFn(), btn);
  });
}
export async function openVoiceOnlineConsent() {
  const ok = await confirmDialog({
    title: t("voiceOnlineTitle"),
    message: t("voiceOnlineInfo"),
    confirmLabel: t("voiceOnlineAllow"),
    danger: false,
  });
  if (!ok) return;
  try { await invoke("voice_enable_online_speech"); } catch (e) { errToast(e); return; }
  settings.ui_flags = { ...(settings.ui_flags || {}), voiceOnlineOk: true };
  applyFlags();
}
export function routeVoiceError(err) {
  const code = String(err);
  // No error toast: the fix is always the same, so the first click just opens the dialog that
  // offers it (the OCR button does the same).
  if (code === "voice-no-language") { openVoiceLangsModal(); return; }
  // voice-privacy-off is the same missing consent voice-online-required guards against (a
  // Windows registry value), so the same dialog fixes it.
  if (code === "voice-online-required" || code === "voice-privacy-off") { openVoiceOnlineConsent(); return; }
  if (code === "voice-denied") {
    toast(t("voiceDeniedMsg"), { label: t("voiceOpenPrivacy"), onClick: () => invoke("open_privacy_mic").catch(() => {}) });
    return;
  }
  const key = { "voice-busy": "voiceErrBusy", "voice-failed": "voiceErrFailed" }[code];
  if (key) toast(t(key)); else errToast(code);
}
// Joins a new recognized chunk onto the target: one space, except right after a newline or an
// opening bracket, and never a second space next to one already there.
export function voiceJoiner(before) {
  if (!before) return "";
  if (/[\n(]$/.test(before)) return "";
  if (/\s$/.test(before)) return "";
  return " ";
}
// interim: true for a live voice-partial update (replaced by the next update or the final
// result), false for the committed voice-text. The marker (voiceInterim) makes a partial
// replaceable in place instead of piling up duplicate text; it is tracked per target element
// and discarded when the user types or moves the cursor (see the discard listeners below),
// after which it stays as plain text.
export function insertVoiceText(text, interim) {
  const el = voiceTargetEl;
  if (!el || el.classList.contains("hidden")) return; // its field disappeared mid-session
  const value = el.value;
  let start, replaceLen;
  if (voiceInterim) {
    start = voiceInterim.start;
    replaceLen = voiceInterim.len;
  } else {
    const atEnd = txt("voiceInsert") === "end";
    start = atEnd ? value.length : (el.selectionStart ?? value.length);
    replaceLen = 0;
  }
  const joiner = voiceInterim ? "" : voiceJoiner(value.slice(0, start));
  const newValue = value.slice(0, start) + joiner + text + value.slice(start + replaceLen);
  const newStart = start + joiner.length;
  voiceSuppressInput = true;
  el.value = newValue;
  el.selectionStart = el.selectionEnd = newStart + text.length;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  voiceSuppressInput = false;
  voiceInterim = interim ? { start: newStart, len: text.length } : null;
}
// Sibling of insertVoiceText for deliverLateVoiceText's one-shot delivery; it does not touch
// voiceInterim/voiceSuppressInput, since no live session is left to track.
export function insertLateVoiceText(el, text, at) {
  const value = el.value;
  const joiner = voiceJoiner(value.slice(0, at));
  el.value = value.slice(0, at) + joiner + text + value.slice(at);
  el.selectionStart = el.selectionEnd = at + joiner.length + text.length;
  el.dispatchEvent(new Event("input", { bubbles: true }));
}
// The recognized text is ours until the user visibly acts on the field; then the interim span
// is just text.
export function voiceWatchDiscard(el) {
  if (!el) return;
  const discard = () => { if (voiceInterim && voiceTargetEl === el) voiceInterim = null; };
  el.addEventListener("input", () => { if (!voiceSuppressInput) discard(); });
  el.addEventListener("click", discard);
  el.addEventListener("keydown", (e) => {
    if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"].includes(e.key)) discard();
  });
}
// The global dictation hotkey has no visible field (the window is never brought forward), so
// it targets this detached textarea. It is never added to the DOM but rides the same
// insertVoiceText/voiceJoiner pipeline as the two visible buttons.
export const voiceHotkeyBuffer = document.createElement("textarea");
// A hotkey session that stopped while its buffer still held a live partial: the final text
// for that phrase can arrive just after voice-stopped, so delivery waits for it (up to
// VOICE_LATE_GRACE_MS) instead of sending the partial and then the final as a second text.
let voiceHotkeyHold = null; // {text, timer} | null
export function flushVoiceHotkeyHold() {
  const held = voiceHotkeyHold;
  if (!held) return;
  voiceHotkeyHold = null;
  clearTimeout(held.timer);
  deliverVoiceHotkeyText(held.text);
}
export function holdVoiceHotkeyText(text) {
  flushVoiceHotkeyHold();
  voiceHotkeyHold = { text, timer: setTimeout(flushVoiceHotkeyHold, VOICE_LATE_GRACE_MS) };
}
export async function deliverVoiceHotkeyText(text) {
  if (!text.trim()) return;
  const target = txt("voiceHotkeyTarget") || "clipboard";
  if (target === "input") {
    inputEl.value = inputEl.value ? `${inputEl.value}\n${text}` : text;
    autoGrow(inputEl);
    return;
  }
  // Clipboard and paste both copy first; paste then adds the paste-into-previous-app step a
  // prompt hotkey already uses.
  try { await invoke("copy_text", { text }); } catch (e) { errToast(e); return; }
  if (target === "paste") await autoPasteNow(null);
  else toast(t("copied"));
}

export function bindDictation() {
  // Voice dictation: the two buttons (composer, edit dialog) share one wiring helper and one
  // event pipeline (see the section above).
  wireVoiceButton($("voice-btn"), () => inputEl);
  wireVoiceButton(modal.voiceBtn, () => modal.text);
  voiceWatchDiscard(inputEl);
  voiceWatchDiscard(modal.text);
  listen("voice-partial", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) return; // a late partial is ignored, not delivered
    resolveVoiceArming();
    insertVoiceText(payload.text || "", true);
  });
  listen("voice-text", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) { deliverLateVoiceText(payload); return; }
    resolveVoiceArming();
    const processed = applyVoiceCommands(payload.text || "", { punct: flag("voicePunct"), autoCaps: flag("voiceAutoCaps"), lang: voiceLangCode() });
    insertVoiceText(processed, false);
  });
  listen("voice-stopped", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) return;
    const wasHotkey = voiceTargetEl === voiceHotkeyBuffer;
    const finalText = wasHotkey ? voiceHotkeyBuffer.value : "";
    const hadPartial = wasHotkey && voiceInterim !== null;
    endVoiceUi();
    if (wasHotkey) {
      if (hadPartial) holdVoiceHotkeyText(finalText);
      else deliverVoiceHotkeyText(finalText);
    }
  });
  listen("voice-error", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) return;
    endVoiceUi();
    routeVoiceError(payload.code);
  });
  // The global hotkey toggled a session ON (toggling OFF is handled on the Rust side).
  listen("voice-hotkey-toggle", () => {
    if (voiceState !== "idle") return;
    flushVoiceHotkeyHold();
    voiceHotkeyBuffer.value = "";
    startVoice(voiceHotkeyBuffer, null);
  });
  // Management dialog chrome.
  $("ocr-langs-close-x").addEventListener("click", closeLangsModal);
  $("ocr-langs-modal").addEventListener("pointerdown", (e) => { if (e.target === $("ocr-langs-modal")) closeLangsModal(); });
  $("ocr-langs-recheck").addEventListener("click", () => loadLangCaps(langDialogKind, true));
}

export function wireDictation() {
  EXPERT_ACTIONS["ocr-langs-open"] = openOcrLangsModal;
  EXPERT_ACTIONS["voice-langs-open"] = openVoiceLangsModal;
}
