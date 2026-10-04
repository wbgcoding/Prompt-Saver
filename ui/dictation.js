// Voice dictation and the OCR and speech language packs.
import { $, autoGrow, errToast, EXPERT_ACTIONS, flag, inputEl, invoke, LANG, listen, modal, settings, t, toast, txt, val } from "./core.js";
import { confirmDialog } from "./dialog.js";
import { fmtCopyTime } from "./journal.js";
import { fmtBytes } from "./floatShared.js";
import { applyFlags, assignOcrLangsCache, assignVoiceLangsCache } from "./expertMenu.js";
import { recordToolUsage } from "./management.js";
import { autoPasteNow } from "./tiles.js";

// ---- OCR + voice language pack management, one shared dialog ----
// #ocr-langs-modal serves both OCR and voice dictation language packs -- same
// markup, same flow (Windows-reported capability list, elevated listing/install),
// just pointed at a different set of Tauri commands and settings keys. `kind`
// ("ocr" | "speech") selects which.
// *_capabilities() lists every capability Windows knows about (installed AND
// not) with size/version straight from Windows -- a different list from
// ocrLangsCache/voiceLangsCache above, which are only the installed set actually
// used to pick a recognition language.
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
// Per-kind dialog state, kept independent so switching which dialog is open
// never loses the other's in-flight scan or install polls.
export const LANG_DIALOG = {
  // installing: the names whose elevated install was launched and has not reported
  // back yet. It outlives the dialog (only the pollers stop on close), so a reopened
  // dialog still knows the row is busy. installChain marks the one live poller per name.
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
  // A scan started on a previous open (or from the elevation retry button)
  // may still be running server-side -- resume polling it instead of the dialog
  // silently doing nothing until the user re-elevates from scratch.
  if (st.elevRunning) { renderLangsLoading(kind); pollElevatedCaps(kind); }
  else loadLangCaps(kind, false);
  // Installs started before the dialog was closed keep running as OS processes:
  // pick their result polling back up.
  for (const name of st.installing) pollLangInstall(kind, name);
}
export function closeLangsModal() {
  $("ocr-langs-modal").classList.add("hidden");
  const st = LANG_DIALOG[langDialogKind];
  // Installs and an elevated scan are OS processes and keep running regardless
  // -- only the polling loop (and the DOM it repaints) needs to stop while
  // nobody is looking. elevRunning itself is left set so reopening resumes.
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
    // Listing is supposed to need no elevated rights, but some Windows builds
    // require it even for this read-only query -- an honest inline reason
    // (plus a retry, 162) beats a blank dialog or a generic error toast. 177:
    // ANY failure here comes back this same way now (backend no longer tries
    // to classify by HResult/message text, which varies by machine/locale) --
    // "code|raw text" so the raw PowerShell/DISM reason can still be shown.
    const s = String(e);
    if (s.startsWith("ocr-caps-need-elevation")) {
      renderLangsRetry(kind, t("ocrLangsNeedElevation"), s.slice(s.indexOf("|") + 1));
    } else errToast(e);
    return;
  }
  renderLangsBody(kind);
}
// Runs the capability listing through the UAC prompt on machines that refuse it
// to a normal user. Same shape as an install: explain first, launch, then poll
// the result the elevated process writes.
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
    // A launch failure also gets the notice+retry pair, not a dead end --
    // except when the account cannot elevate at all: that gets its own
    // explanation instead, with no retry button, because trying again can
    // never succeed.
    const err = String(e);
    if (err === "ocr-elevate-no-rights") {
      renderLangsRetry(kind, `${t("capNeedsAdmin")} ${t("capNeedsAdminWhat")}`, null, true);
    } else {
      renderLangsRetry(kind, err === "ocr-elevate-cancelled" ? t("ocrElevateCancelled") : err);
    }
    return;
  }
  st.elevRunning = true;
  st.elevLeft = 60;
  pollElevatedCaps(kind);
}
// Polls the elevated process's result file a few seconds apart. Re-entrant: can
// be called again after the dialog was closed and reopened without losing
// the two-minute budget already spent (elevLeft lives on LANG_DIALOG, not in a
// closure local, so it survives that close/reopen).
export function pollElevatedCaps(kind) {
  const st = LANG_DIALOG[kind];
  const tick = async () => {
    st.elevTimer = null;
    let res;
    try { res = await invoke(LANG_CMDS[kind].elevatedResult); }
    catch (e) { st.elevRunning = false; renderLangsRetry(kind, String(e)); return; }
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
// Notice text and its retry button are always built together, in one
// place -- a failure (launch or the two-minute poll timeout alike) can no
// longer replace the body with text only and strand the user without a way
// to try again. `details` is the raw PowerShell/DISM text behind a
// translated reason like `message` -- shown collapsed so it never crowds out
// the actual instruction, but never silently dropped either. `noRetry` drops
// the button entirely: set when the account can never elevate, where another
// attempt cannot succeed and offering one would just be misleading.
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
  sub.textContent = `${cap.tag} · ${cap.version || "\u2014"}`;
  main.append(name, sub);
  const installed = cap.state === "Installed";
  const size = installed ? cap.install_size : cap.download_size;
  const sizeEl = document.createElement("div");
  sizeEl.className = "lang-row-size";
  // Windows does not always report a size -- never guess one, show the dash instead.
  sizeEl.textContent = size == null ? "\u2014" : fmtBytes(size);
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
    // "no-rights" is the account-can-never-elevate case, its own text below;
    // any other string is the backend's own reason (a real install failure);
    // `true` is the calm "you declined the rights prompt" case.
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
  // The size sits INSIDE the button label on purpose -- nobody should start a
  // download blind.
  btn.textContent = busy
    ? t("ocrLangsInstalling")
    : t("ocrLangsDownloadBtn").replace("{size}", cap.download_size == null ? "\u2014" : fmtBytes(cap.download_size));
  btn.addEventListener("click", () => startLangInstall(kind, cap));
  actions.appendChild(btn);
  return row;
}
// Every path that can lead to an elevated install goes through here first:
// explain the language, its size and that Windows will ask for admin rights
// once, BEFORE that prompt ever appears. No exceptions.
export async function startLangInstall(kind, cap) {
  const st = LANG_DIALOG[kind];
  const size = cap.download_size == null ? "\u2014" : fmtBytes(cap.download_size);
  const ok = await confirmDialog({
    title: t("ocrElevateTitle"),
    message: t("ocrElevateInfo").replace("{lang}", cap.label).replace("{size}", size),
    confirmLabel: t("ocrElevateInstall"),
    danger: false,
  });
  if (!ok) return;
  delete st.installErrors[cap.name];
  // Marks the row busy for the WHOLE gap, including the elevated install()
  // call itself (which blocks until the user answers the UAC prompt) -- set
  // synchronously, before any await, so a second click on the still-enabled
  // button can't start a second concurrent elevated install for the same
  // language while the first one is still waiting on Windows.
  st.installPending[cap.name] = true;
  renderLangsBody(kind);
  try {
    await invoke(LANG_CMDS[kind].install, { name: cap.name });
  } catch (e) {
    delete st.installPending[cap.name];
    // A declined UAC prompt is not an error: a calm inline note on the row,
    // no error tone, the row stays "not installed". An account that cannot
    // elevate at all gets its own note instead, and no download button --
    // clicking it again can never work.
    const err = String(e);
    if (err === "ocr-elevate-no-rights") st.installErrors[cap.name] = "no-rights";
    else if (err === "ocr-elevate-cancelled") st.installErrors[cap.name] = true;
    else errToast(e);
    // Every branch above just deleted installPending -- the row rendered disabled
    // ("Installing...") right before the await must be repainted regardless of
    // which branch ran, or the button stays stuck disabled until the dialog is
    // closed and reopened.
    renderLangsBody(kind);
    return;
  }
  delete st.installPending[cap.name];
  // Arm the poller (marks the row busy) BEFORE repainting, or the render still
  // reads installTimers[cap.name] as empty and shows an enabled Download button --
  // a second click then starts a second UAC-elevated install.
  pollLangInstall(kind, cap.name);
  renderLangsBody(kind);
}
// *_install_language() only resolves once the elevated process has LAUNCHED,
// not once it finished -- poll the result file ourselves, a few seconds apart,
// until it stops being null.
// A result file that never becomes readable (permanently corrupt, or wiped
// from under the install) used to look exactly like "still running" forever --
// polled every 3s with no way out. ~10 minutes is generous for even a large,
// slow-connection language pack, so hitting this is always a real stuck state.
export const MAX_LANG_INSTALL_POLLS = 200;
export function pollLangInstall(kind, name) {
  const st = LANG_DIALOG[kind];
  if (st.installTimers[name]) return;
  st.installing.add(name);
  const chain = {};
  st.installChain[name] = chain;
  let attempts = 0;
  // setTimeout re-armed after each result, not setInterval -- the callback
  // awaits an invoke, and a reply slower than the 3s tick would otherwise overlap
  // itself (two install_result calls, or two clearInterval/repaint races).
  const tick = async () => {
    attempts++;
    let res;
    try { res = await invoke(LANG_CMDS[kind].installResult, { name }); } catch (_) { res = undefined; }
    // The dialog was closed (and maybe reopened, with a fresh poller) while that
    // answer was on its way: this chain is stale and must not re-arm itself. A
    // terminal answer is still handled, though: the backend already deleted the
    // result and dropped its nonce, so no later poll will ever see it again.
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
    // A real failure (e.g. Windows Home / policy blocked) -- the reason belongs
    // in the row, never a silent failure. The backend's own internal keys are
    // translated here; anything else is a raw Windows exception message, shown
    // exactly as Windows worded it since there is no way to pre-translate that.
    st.installErrors[name] = res.error === "install-result-unreadable" ? t("ocrInstallResultUnreadable") : (res.error || true);
    renderLangsBody(kind);
  };
  st.installTimers[name] = setTimeout(tick, 3000);
}
// Thin OCR-flavoured wrapper so the expert-action entry below keeps working
// unchanged -- the generalization lives entirely behind `kind`.
export function openOcrLangsModal() { openLangsModal("ocr"); }
export function openVoiceLangsModal() { openLangsModal("speech"); }

// "ocr-no-language"/"voice-no-language" always have the same fix: open this
// dialog. Used at every call site that would otherwise show them as a bare errToast().
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
// The just-ended session's target, for one late-arriving voice-text that WinRT's
// unordered ResultGenerated/Completed handlers let land after voice-stopped
// already fired -- delivered instead of dropped, but only within the grace
// window and only to ITS OWN target, never adopted by whatever session (if any)
// has started in the meantime. Not an expert setting -- internal timing.
export let voicePrev = null; // {token, el, caret, lang, hotkey, endedAt, interim} | null
export const VOICE_LATE_GRACE_MS = 3000;

// The two-letter code voice.js's command tables are keyed by, derived from
// the BCP-47 tag voice_start resolved to -- NOT the UI's own display
// language, which can easily differ from what is actually being recognized
// (e.g. a German UI with only an English speech pack installed). Falls back
// to the UI language before a session has resolved one at all (the hotkey
// path's first partial can in rare cases race ahead of it).
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
// The button stays in its "arming" in-between state (so a fast double-click
// cannot start a second session) until the first backend event, of any kind,
// proves the session is actually alive.
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
      // The live partial still sitting in the field: the final text of this very
      // session replaces it instead of landing next to it.
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
  // A cancel that happened while this call was in flight (voice_start's own
  // spawn_blocking round trip is not instant) has to stop the session it
  // just now learned the id of -- stopVoice() itself had nothing to send.
  if (voiceArmCancelled) {
    voiceArmCancelled = false;
    invoke("voice_stop", { id }).catch(() => {});
    // Only reset the UI if no backend event has already adopted this session
    // (see voiceEventIsMine) -- if one has, its own listener owns the reset.
    if (voiceSessionId === null && voiceState === "arming") endVoiceUi();
    return;
  }
  if (voiceSessionId === null) voiceSessionId = id;
  voiceResolvedLang = lang || voiceResolvedLang;
}
export async function stopVoice() {
  if (voiceState === "arming" && voiceSessionId === null) {
    // Nothing to send yet -- voice_start hasn't resolved with an id. Mark the
    // cancel and let startVoice() finish the job the moment it does.
    voiceArmCancelled = true;
    return;
  }
  if (!voiceSessionId) return;
  try { await invoke("voice_stop", { id: voiceSessionId }); } catch (_) {}
  // The UI resets from the voice-stopped event the backend always sends back,
  // not here -- the button never claims "off" before the session (and its
  // mic) has actually finished.
}
// True if a token-tagged backend event belongs to the session currently
// starting or running. voiceToken is chosen by the frontend and sent to
// voice_start itself, so it is known before any event can possibly arrive --
// unlike the backend-assigned id, it is never adopted from an event. The two
// WinRT handlers (ResultGenerated, Completed) are not ordered against each
// other, so a session that already reported voice-stopped can still deliver
// one more voice-text after that -- its token no longer matches here and it
// falls to deliverLateVoiceText instead of being silently misattributed to
// whatever session (if any) is now starting.
export function voiceEventIsMine(payload) {
  return voiceState !== "idle" && !!payload && payload.token === voiceToken;
}
// A late voice-text for the session endVoiceUi just closed out (see voicePrev)
// -- delivered to ITS OWN target within VOICE_LATE_GRACE_MS instead of being
// dropped, and never touches voiceState/voiceSessionId/the buttons, since
// there is no live session left for this to belong to.
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
  if (prev.hotkey) { deliverVoiceHotkeyText(processed); return; }
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
  // No error toast for this one -- the fix is always the same, so the first
  // click just opens the dialog that offers it (the OCR button does the same).
  if (code === "voice-no-language") { openVoiceLangsModal(); return; }
  // voice-privacy-off is the same missing consent voice-online-required already
  // guards against -- Windows' own registry value, so the same dialog fixes it.
  if (code === "voice-online-required" || code === "voice-privacy-off") { openVoiceOnlineConsent(); return; }
  if (code === "voice-denied") {
    toast(t("voiceDeniedMsg"), { label: t("voiceOpenPrivacy"), onClick: () => invoke("open_privacy_mic").catch(() => {}) });
    return;
  }
  const key = { "voice-busy": "voiceErrBusy", "voice-failed": "voiceErrFailed" }[code];
  errToast(key ? t(key) : code);
}
// Joins a new recognized chunk onto whatever the target already holds: one
// space, except right after a newline or an opening bracket, and never a
// second space next to one already there.
export function voiceJoiner(before) {
  if (!before) return "";
  if (/[\n(]$/.test(before)) return "";
  if (/\s$/.test(before)) return "";
  return " ";
}
// interim: true for a live voice-partial update (replaced by the next update
// or the final result), false for the committed voice-text result. The marker
// (voiceInterim) is what makes a partial replaceable in place instead of
// piling up duplicate text on every update; tracked per the target element,
// discarded the moment the user types or moves the cursor themselves (see the
// discard listeners below), at which point it just stays as plain text.
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
// Sibling of insertVoiceText for deliverLateVoiceText's one-shot delivery --
// deliberately does not touch voiceInterim/voiceSuppressInput, since there is
// no live session left for the discard-tracking those belong to.
export function insertLateVoiceText(el, text, at) {
  const value = el.value;
  const joiner = voiceJoiner(value.slice(0, at));
  el.value = value.slice(0, at) + joiner + text + value.slice(at);
  el.selectionStart = el.selectionEnd = at + joiner.length + text.length;
  el.dispatchEvent(new Event("input", { bubbles: true }));
}
// The recognized text is still ours to lay claim to right up until the user
// visibly acts on the field themselves -- then the interim span is just text.
export function voiceWatchDiscard(el) {
  if (!el) return;
  const discard = () => { if (voiceInterim && voiceTargetEl === el) voiceInterim = null; };
  el.addEventListener("input", () => { if (!voiceSuppressInput) discard(); });
  el.addEventListener("click", discard);
  el.addEventListener("keydown", (e) => {
    if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"].includes(e.key)) discard();
  });
}
// 131's global hotkey has no visible field to dictate into (the window is
// never brought forward), so it targets this detached textarea instead --
// never added to the DOM, but a real element, so it rides the exact same
// insertVoiceText/voiceJoiner pipeline as the two visible buttons rather than
// a second copy of it.
export const voiceHotkeyBuffer = document.createElement("textarea");
export async function deliverVoiceHotkeyText(text) {
  if (!text.trim()) return;
  const target = txt("voiceHotkeyTarget") || "clipboard";
  if (target === "input") {
    inputEl.value = inputEl.value ? `${inputEl.value}\n${text}` : text;
    autoGrow(inputEl);
    return;
  }
  // clipboard and paste both copy first -- paste then adds the same
  // paste-into-previous-app step a prompt hotkey already uses.
  try { await invoke("copy_text", { text }); } catch (e) { errToast(e); return; }
  if (target === "paste") await autoPasteNow(null);
  else toast(t("copied"));
}

export function bindDictation() {
  // Voice dictation: the two buttons (composer, edit dialog) share one wiring
  // helper and one event pipeline -- see the section above for all of it.
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
    endVoiceUi();
    if (wasHotkey) deliverVoiceHotkeyText(finalText);
  });
  listen("voice-error", (e) => {
    const payload = e.payload || {};
    if (!voiceEventIsMine(payload)) return;
    endVoiceUi();
    routeVoiceError(payload.code);
  });
  // The global hotkey toggled a session ON (a toggle OFF is handled entirely
  // on the Rust side, no event needed for that direction).
  listen("voice-hotkey-toggle", () => {
    if (voiceState !== "idle") return;
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
