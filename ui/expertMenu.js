// Expert menu: the renderer, its rows and the live-state parts of the setting registry.
import { $, armButton, assignBUBBLE_MS, assignPrompts, assignSettings, COLORS, debounced, DEFAULT_COLORS, DISARM_MS, disarmButton, errText, errToast, EXPERT_ACTIONS, flag, flagState, gateOpen, HEADER_PAD_Y, HEX_RE, ICON_UNDO, ICON_WARN, invoke, LANG, openColorPop, optFlag, PALETTE_LEN, prompts, putIcon, setStatusText, settings, settingsEl, shipsOn, syncEdgeFade, t, toast, txt, val } from "./core.js";
import { ALL_FLAG_KEYS, normScreenshotFormat, EXPERT_BULK_NEVER, EXPERT_RESET_KEEP, EXPERT_SELECTS, EXPERT_TABS, EXPERT_TIP_SET, EXPERT_VALUES, FLAG_GATES, flagLabelKey, GRID_DOM_VALUES, groupFlags, OPT_FLAG_KEYS, REC_QUALITY_BITRATE, SELECT_CUSTOM_MAX, SELECT_CUSTOM_MIN, SETTINGS_AREA_LABEL, STYLE_OPTS, TILE_DOM_FLAGS, TILE_STYLES } from "./expertConfig.js";
import { confirmDialog, infoDialog } from "./dialog.js";
import { refreshFavViewUi, renderGrid, renderViews } from "./views.js";
import { fitAllTiles, fitBox, fitCache } from "./tiles.js";
import { loadPalette, refreshColorUI, refreshCustomThemes } from "./theme.js";
import { assignChainLock, chainLock, chainMode, setChainMode } from "./chain.js";
import { renderLibrary } from "./library.js";
import { offerClipInboxCleanup, refreshClipInbox } from "./journal.js";
import { applyBarToggleTips, hotkeyClearX } from "./windowChrome.js";
import { setMediaDefaults } from "./media.js";
import { attachSelectPicker } from "./selectpop.js";
import { ffmpegDownloadActive, fmtBytes, fmtUnit, normCopySizeLevel, normRecFormat, runFfmpegDownload, VIDEO_FORMATS, videoFormatLabelKey } from "./floatShared.js";
import { reloadEverything, syncSettingsControls } from "./settingsPanel.js";
import { bindHotkeyField, commitRecPauseHotkey, commitRecStopHotkey, prettyAccel } from "./hotkeys.js";
import { toggleShortLivedCert } from "./management.js";
import { closeBackups } from "./backups.js";
import { varFixedListRow } from "./editor.js";
import { openVideoCleanup } from "./recordingTools.js";
import { i18nHit, settingHit, SETTINGS_DIRECTORY, settingsDirectoryHit, flashSettingsRow, openSettingsDirectoryEntry } from "./settingsSearch.js";
import { EXPERT_NOTICES, noticeRow, noticeActionRow, actionRow } from "./expertNotices.js";

export let expertResetTimer = null;

// Write one ui_texts entry optimistically and put the previous value back when the store refuses
// it (a read-only data folder), so the dropdown, the CSS state and isRowChanged never show a
// value that was not kept. Resolves to whether the write stuck.
export async function persistUiText(key, value) {
  const texts = settings.ui_texts || {};
  const had = Object.prototype.hasOwnProperty.call(texts, key);
  const before = texts[key];
  settings.ui_texts = { ...texts, [key]: value };
  try {
    await invoke("set_ui_text", { key, value });
    return true;
  } catch (e) {
    const back = { ...(settings.ui_texts || {}) };
    if (had) back[key] = before; else delete back[key];
    settings.ui_texts = back;
    errToast(e);
    return false;
  }
}

// Build a simple string-enum dropdown backed by ui_texts (default when unset).
// `opts` is [[value, i18nLabelKey], …]. Used by expert enum params.
export function enumDropdown(key, labelKey, opts, def, gate, norm = (v) => v) {
  return {
    label: labelKey,
    gate,
    def, // "only changed" filter compares get() against this
    options: () => opts.map(([v, lk]) => [v, t(lk)]),
    get: () => norm(settings.ui_texts?.[key] || def),
    set: async (v) => {
      await persistUiText(key, v);
      applyValues();
    },
  };
}

const normPreviewSize = (v) => (["short", "medium", "long"].includes(v) ? v : "medium");

// Dropdowns like the settings' text-size / font selects.
export const EXPERT_DROPDOWNS = {
  // Live counter unit selection.
  counterUnits: enumDropdown("counterUnits", "valCounterUnits",
    [["all", "optAll"], ["chars", "lenChars"], ["words", "lenWords"], ["tokens", "lenTokens"]], "all", "lengthCounter"),
  // How the text on a tile looks: one choice instead of seven switches. Several
  // styles combined cannot be kept by a single choice (see Settings::migrate).
  tileStyle: enumDropdown("tileStyle", "valTileStyle",
    [["plain", "optTileStylePlain"], ["bold", "optTileStyleBold"], ["uppercase", "optTileStyleUpper"], ["mono", "optTileStyleMono"], ["italic", "optTileStyleItalic"], ["shadow", "optTileStyleShadow"], ["compact", "optTileStyleCompact"], ["gradient", "optTileStyleGradient"]], "plain"),
  // How several selected tag filters combine.
  tagFilterMode: enumDropdown("tagFilterMode", "valTagFilterMode",
    [["and", "optTagAnd"], ["or", "optTagOr"]], "and", "tags"),
  // How the self-filling date/time placeholders are written.
  chainSeparator: enumDropdown("chainSeparator", "valChainSeparator",
    [["blank", "optChainSepBlank"], ["break", "optChainSepBreak"], ["space", "optChainSepSpace"], ["none", "optChainSepNone"]], "blank", "chainPrompts"),
  dateStyle: enumDropdown("dateStyle", "valDateStyle", STYLE_OPTS, "short", "dynVars"),
  timeStyle: enumDropdown("timeStyle", "valTimeStyle", STYLE_OPTS, "short", "dynVars"),
  // Which key turns a press into a drag-out instead of the plain drag.
  dragOutModifier: enumDropdown("dragOutModifier", "valDragOutModifier",
    [["ctrl", "keyCtrl"], ["alt", "keyAlt"], ["shift", "keyShift"]], "ctrl", "dragOut"),
  // Mode a NEW rotating prompt starts with (each prompt can still switch its
  // own mode afterward in the editor).
  rotateDefaultMode: enumDropdown("rotateDefaultMode", "valRotateDefaultMode",
    [["order", "optRotateOrder"], ["random", "optRotateRandom"]], "order", "rotatingPrompts"),
  // The notation the picked colour is copied in.
  pickerFormat: enumDropdown("pickerFormat", "valPickerFormat",
    [["hex", "optFmtHex"], ["rgb", "optFmtRgb"], ["hsl", "optFmtHsl"]], "hex", "colorPicker"),
  // Which tool the screenshot editor opens with.
  snipEditDefaultTool: enumDropdown("snipEditDefaultTool", "valSnipEditDefaultTool",
    [["last", "optSnipToolLast"], ["hand", "snipToolHand"], ["crop", "snipToolCrop"], ["pen", "snipToolPen"], ["marker", "snipToolMarker"],
     ["arrow", "snipToolArrow"], ["text", "snipToolText"], ["step", "snipToolStep"]], "last", "snipEditor"),
  // What the text tool draws behind its letters. A stored boolean from the former snipTextPlate
  // flag converts once on read (below).
  snipTextBack: {
    label: "valSnipTextBack",
    gate: "snipEditor",
    def: "plate",
    options: () => [["none", t("optSnipTextBackNone")], ["plate", t("optSnipTextBackPlate")], ["outline", t("optSnipTextBackOutline")]],
    get: () => {
      const stored = settings.ui_texts?.snipTextBack;
      if (stored) return stored;
      return settings.ui_flags?.snipTextPlate === false ? "none" : "plate";
    },
    set: async (v) => {
      await persistUiText("snipTextBack", v);
      applyValues();
    },
  },
  // Options are fetched once from ocr_languages() at startup and cached (ocrLangsCache, near the
  // OCR wiring below); "Automatic" is always first.
  ocrLang: {
    label: "valOcrLang",
    gate: "ocr",
    def: "auto",
    // The language dialog opens from the ocr-langs-open button below.
    options: () => [["auto", t("langAuto")], ...ocrLangsCache.map((l) => [l.tag, l.label])],
    get: () => settings.ui_texts?.ocrLang || "auto",
    set: async (v) => {
      await persistUiText("ocrLang", v);
    },
  },
  // Which microphone to capture for recAudioMic. Options are fetched once
  // from list_mic_devices() at startup (micDevicesCache, near the recording
  // wiring below); empty string = the system default capture device.
  recMicDevice: {
    label: "valRecMicDevice",
    gate: "recAudioMic",
    def: "",
    options: () => [["", t("langAuto")], ...micDevicesCache.map((d) => [d.id, d.name])],
    get: () => settings.ui_texts?.recMicDevice || "",
    set: async (v) => {
      await persistUiText("recMicDevice", v);
    },
  },
  // Which camera the camera window opens with. Options come from
  // navigator.mediaDevices.enumerateDevices() (cameraDevicesCache); listing never turns the
  // camera light on, only getUserMedia does, so the main window can populate it. Empty string =
  // the system default camera.
  cameraDevice: {
    label: "valCameraDevice",
    gate: "cameraRecord",
    def: "",
    options: () => [["", t("langAuto")], ...cameraDevicesCache.map((d) => [d.id, d.name])],
    get: () => settings.ui_texts?.cameraDevice || "",
    set: async (v) => {
      await persistUiText("cameraDevice", v);
    },
  },
  cameraResolution: enumDropdown("cameraResolution", "valCameraResolution",
    [["480", "optCameraRes480"], ["720", "optCameraRes720"], ["1080", "optCameraRes1080"]], "720", "cameraRecord"),
  // Three quality steps, default "high". Picking a step re-suggests the matching recBitrateKbps
  // default (REC_QUALITY_BITRATE) so the two controls stay in sync; the slider still accepts any
  // later override. "Lossless" leaves the slider alone, since a separate ffmpeg CRF-0 pass
  // produces it after recording stops.
  recQuality: {
    label: "valRecQuality",
    gate: "screenRecord",
    def: "high",
    options: () => [
      ["small", t("optRecQualitySmall")], ["high", t("optRecQualityHigh")], ["lossless", t("optRecQualityLossless")],
    ],
    get: () => settings.ui_texts?.recQuality || "high",
    set: async (v) => {
      if (!(await persistUiText("recQuality", v))) { applyValues(); return; }
      if (REC_QUALITY_BITRATE[v] != null) {
        const before = settings.ui_values?.recBitrateKbps;
        settings.ui_values = { ...(settings.ui_values || {}), recBitrateKbps: REC_QUALITY_BITRATE[v] };
        try {
          await invoke("set_ui_value", { key: "recBitrateKbps", value: REC_QUALITY_BITRATE[v] });
        } catch (e) {
          const back = { ...(settings.ui_values || {}) };
          if (before === undefined) delete back.recBitrateKbps; else back.recBitrateKbps = before;
          settings.ui_values = back;
          errToast(e);
        }
      }
      applyValues();
    },
  },
  // MP4 is what the recorder writes; webm/gif come from an ffmpeg conversion pass after the
  // recording stops (convert_recording on the backend).
  recFormat: enumDropdown("recFormat", "valRecFormat",
    VIDEO_FORMATS.map((f) => [f, videoFormatLabelKey(f)]), "mp4", "screenRecord", normRecFormat),
  // "window" matches resolve_recording_settings' own fallback default exactly.
  recCanvasMode: enumDropdown("recCanvasMode", "valRecCanvasMode",
    [["all", "recCanvasAll"], ["monitor", "recCanvasMonitor"], ["window", "recCanvasWindow"]], "window", "screenRecord"),
  // Applies to every screenshot: the snip tool's save/save-as/save-to-folder paths and the video
  // editor's "export still frame". webp and bmp are always lossless (see encode_screenshot in
  // capture.rs); only jpeg has a quality, so snipSaveQuality only shows for jpeg.
  screenshotFormat: enumDropdown("screenshotFormat", "valScreenshotFormat",
    [["png", "optScreenshotFormatPng"], ["jpeg", "optScreenshotFormatJpeg"], ["webp", "optScreenshotFormatWebp"],
      ["bmp", "optScreenshotFormatBmp"]], "png", "screenshot", normScreenshotFormat),
  // Same pattern as ocrLang: fetched once from voice_languages() at startup (voiceLangsCache),
  // "Automatic" first, a trailing entry opens the shared language-pack dialog instead of being stored.
  voiceLang: {
    label: "valVoiceLang",
    gate: "voice",
    def: "auto",
    options: () => [["auto", t("langAuto")], ...voiceLangsCache.map((l) => [l.tag, l.label])],
    get: () => settings.ui_texts?.voiceLang || "auto",
    set: async (v) => {
      await persistUiText("voiceLang", v);
    },
  },
  voiceInsert: enumDropdown("voiceInsert", "valVoiceInsert",
    [["caret", "voiceInsertCaret"], ["end", "voiceInsertEnd"]], "caret", "voice"),
  voiceHotkeyTarget: enumDropdown("voiceHotkeyTarget", "valVoiceHotkeyTarget",
    [["clipboard", "voiceHotkeyTargetClip"], ["paste", "voiceHotkeyTargetPaste"], ["input", "voiceHotkeyTargetInput"]], "clipboard", "voiceHotkey"),
  // Which edge the toast notifications rise from.
  toastPos: enumDropdown("toastPos", "valToastPos",
    [["bottom", "optToastBottom"], ["top", "optToastTop"]], "bottom"),
  // Tag manager / filter chip order.
  tagSort: enumDropdown("tagSort", "valTagSort",
    [["alpha", "optTagSortAlpha"], ["usage", "optTagSortUsage"]], "alpha", "tags"),
  // Text size of the "Copied!" confirmation on tiles and pills: a share of the
  // size that fits the tile or button.
  copySizeLevel: enumDropdown("copySizeLevel", "valCopySize",
    [["small", "optCopySizeSmall"], ["medium", "optCopySizeMedium"], ["large", "optCopySizeLarge"]], "large", "bubbleMs", normCopySizeLevel),
  // How much of a text attachment the editor preview reads (sizes in ATTACH_PREVIEW_KB).
  attachPreviewSize: enumDropdown("attachPreviewSize", "valAttachPreviewKb",
    [["short", "optPreviewShort"], ["medium", "optPreviewMedium"], ["long", "optPreviewLong"]], "medium", "attachTextPreview", normPreviewSize),
};

// Mirror each disabled flag onto a body class so CSS can hide the matching UI.
export function applyFlags() {
  for (const key of ALL_FLAG_KEYS) {
    document.body.classList.toggle(`noflag-${key}`, !flag(key));
  }
  // Opt-in (default-off) flags: a `flag-<key>` class appears only when enabled.
  for (const key of OPT_FLAG_KEYS) {
    document.body.classList.toggle(`flag-${key}`, optFlag(key));
  }
  // Not a setting: a runtime fact from voice_support(). A Windows build
  // with no speech engine at all must hide the button regardless of the flag.
  document.body.classList.toggle("no-voice-engine", !voiceSupport.supported);
  refreshCustomThemes(); // The feature may have just been switched on or off
  // Flags that change .tile-name metrics via CSS have no apply* path, so a cached fit size from
  // before the toggle would linger until fitCache's own eviction. (The tile style refits itself
  // in applyValues.)
  fitCache.clear();
  if (chainMode && !flag("chainPrompts")) setChainMode(false); // killed in expert menu
  if (chainLock && !flag("chainLock")) { assignChainLock(false); $("chain-btn")?.classList.remove("chain-locked"); }
  applyValues();
  // An open library must follow a flag flip (the tag row appears/disappears).
  if (prompts.length && !$("library")?.classList.contains("hidden")) renderLibrary();
  refreshClipInbox(); // the copy history follows the watcher switch
  applyBarToggleTips();
  syncBannerStack(); // demo mode is a flag-driven notice
}

// Apply every numeric tweak to its live target (CSS vars + JS constants).
export let _lastUiZoom = null; // last native-zoom factor pushed to the backend (throttle)
export let _lastTileStyle = null; // last tile style applied (refit only when it changes)
export function applyValues() {
  const root = document.documentElement.style;
  // The tile style restyles .tile-name (and, for compact, the tile padding), so a
  // change refits every tile against a freshly measured box.
  const tileStyle = txt("tileStyle") || "plain";
  for (const s of TILE_STYLES) document.body.classList.toggle(`tile-style-${s}`, s === tileStyle);
  if (tileStyle !== _lastTileStyle) {
    _lastTileStyle = tileStyle;
    fitCache.clear();
    fitBox.key = "";
    fitAllTiles();
  }
  root.setProperty("--lib-preview-lines", String(Math.round(val("libPreviewLines"))));
  // Appearance scales. UI scale drives the WebView's native zoom (robust); the rest
  // stay CSS `zoom` on their own elements (1 = unchanged). Only push the native zoom
  // when it actually changed: applyValues runs on every slider tick, and a redundant
  // IPC per tick would flood the backend while dragging an unrelated value.
  const uiZoomFactor = val("uiScale") / 100;
  if (uiZoomFactor !== _lastUiZoom) {
    _lastUiZoom = uiZoomFactor;
    invoke("set_ui_zoom", { factor: uiZoomFactor }).catch(() => {});
  }
  root.setProperty("--icon-zoom", val("iconScale") / 100);
  // Sizes exposed to CSS (favorites star, chain badge, tile geometry, header, snip).
  root.setProperty("--tile-radius", `${val("tileRadius")}px`);
  root.setProperty("--tile-border-w", `${val("tileBorderWidth")}px`);
  root.setProperty("--header-pad-y", `${HEADER_PAD_Y}px`);
  root.setProperty("--frosted-blur", `${val("frostedBlur")}px`);
  root.setProperty("--hover-lift", `${val("hoverLift")}px`);
  root.setProperty("--lib-width", `${val("libraryWidth")}px`);
  const dim = val("hoverDim");
  document.body.classList.toggle("hover-dim", dim > 0);
  root.setProperty("--hover-dim-opacity", String((100 - dim) / 100));
  document.body.classList.toggle("toast-top", settings.ui_texts?.toastPos === "top");
  assignBUBBLE_MS(val("bubbleMs"));
  const nameMax = val("nameMaxLen");
  for (const id of ["modal-name", "modal-caption", "view-modal-name"]) {
    const el = $(id);
    if (el) el.maxLength = nameMax;
  }
  setMediaDefaults({ volume: val("videoVolume"), muted: flag("videoMuted"), looped: optFlag("videoLoop") });
}

// Attach the tooltip for `key` to a rendered expert row, if one is defined.
export function withTip(row, key) {
  if (!EXPERT_TIP_SET.has(key)) return row;
  const tipKey = `tip${key[0].toUpperCase()}${key.slice(1)}`;
  const tip = t(tipKey);
  // t() falls back to the key name itself, so never show a raw "tipFoo" as a tooltip.
  if (tip && tip !== tipKey) row.title = tip; // hover engine themes any [title]
  return row;
}

// A small per-row undo icon, visible only while this row differs from its default: resets just
// this one key, using the same isRowChanged() diff as the group reset and the per-key commands
// resetExpertGroup() uses. No confirmation, since one click back is reversible. `after` lets a
// caller outside the main expert menu (the backup/statistics panels) redraw itself instead of
// renderExpert(). Always built and shown/hidden through a class, so a value changed without a
// menu rebuild (a dragged slider, a flipped switch) still gets its reset button live;
// syncRowReset() flips it.
export function rowResetBtn(kind, key, after) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "expert-row-reset";
  btn.dataset.expertKind = kind;
  btn.dataset.expertKey = key;
  btn.classList.toggle("hidden", !isRowChanged(kind, key));
  const label = t("expertResetOne");
  btn.title = label;
  btn.setAttribute("aria-label", label);
  putIcon(btn, ICON_UNDO);
  btn.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    resetOneExpertValue(kind, key, after);
  });
  return btn;
}
// Re-checks one row's changed-from-default state and flips its already-built
// reset button in place: no menu rebuild, so scroll position/focus survive.
// A no-op if that row isn't on screen right now (row list is filtered, or a
// renderExpert() elsewhere already replaced it), which is fine either way.
export function syncRowReset(kind, key) {
  const btn = document.querySelector(`.expert-row-reset[data-expert-kind="${kind}"][data-expert-key="${key}"]`);
  if (btn) btn.classList.toggle("hidden", !isRowChanged(kind, key));
}
export async function resetOneExpertValue(kind, key, after) {
  if (kind === "flags" && EXPERT_BULK_NEVER.includes(key)) return;
  if (kind === "flags") {
    const enabled = shipsOn(key);
    const had = Object.prototype.hasOwnProperty.call(settings.ui_flags || {}, key);
    const before = settings.ui_flags?.[key];
    settings.ui_flags = { ...(settings.ui_flags || {}), [key]: enabled };
    try {
      await invoke("set_ui_flag", { key, enabled });
    } catch (err) {
      const back = { ...(settings.ui_flags || {}) };
      if (had) back[key] = before; else delete back[key];
      settings.ui_flags = back;
      errToast(err);
    }
    applyFlags();
    if (TILE_DOM_FLAGS.has(key)) await renderGrid(true); else fitAllTiles();
  } else if (kind === "values" || kind === "selects") {
    const cfg = EXPERT_VALUES[key] || EXPERT_SELECTS[key];
    const had = Object.prototype.hasOwnProperty.call(settings.ui_values || {}, key);
    const before = settings.ui_values?.[key];
    settings.ui_values = { ...(settings.ui_values || {}), [key]: cfg.def };
    try {
      await invoke("set_ui_value", { key, value: cfg.def });
    } catch (err) {
      const back = { ...(settings.ui_values || {}) };
      if (had) back[key] = before; else delete back[key];
      settings.ui_values = back;
      errToast(err);
    }
    applyValues();
    await refreshTiles(key);
  } else if (kind === "dropdowns") {
    const cfg = EXPERT_DROPDOWNS[key];
    await cfg.set(cfg.def);
  } else if (kind === "paths") {
    EXPERT_PATHS[key].onReset(); // already re-renders itself
    return;
  }
  if (after) after(); else renderExpert();
}

export function flagRow(key) {
  const row = document.createElement("label");
  row.className = "field switch-field";
  row.dataset.settingRow = key; // same jump-to-setting hook as valueRow
  withTip(row, key);
  const span = document.createElement("span");
  span.textContent = t(flagLabelKey(key));
  row.append(span);
  // Features that ship OFF get a small badge so it is clear which settings are additions beyond
  // the standard configuration. Read from the shipped default, so a brand that turns a feature
  // off is labelled correctly too.
  if (!shipsOn(key)) {
    const badge = document.createElement("span");
    badge.className = "default-off-badge";
    badge.title = t("defaultOff");
    badge.setAttribute("role", "img");
    badge.setAttribute("aria-label", t("defaultOff"));
    row.append(badge);
  }
  const input = document.createElement("input");
  input.type = "checkbox";
  input.className = "switch";
  input.checked = flagState(key);
  input.addEventListener("change", async () => {
    const enabled = input.checked;
    // Demo mode swaps the whole store and needs a relaunch either way, so it skips the normal
    // set_ui_flag path: turning it on calls its own backend command and restarts into the sample
    // library; turning it off asks first, since anything typed during the session disappears.
    if (key === "demoMode") {
      if (enabled) {
        try { await invoke("enable_demo_mode", { lang: LANG }); } // the app restarts on success and never returns here
        catch (err) { errToast(err); input.checked = false; }
      } else {
        const ok = await confirmDialog({
          title: t(flagLabelKey(key)),
          message: t("demoModeOffConfirm"),
          confirmLabel: t("demoModeOffConfirmOk"),
          danger: true,
        });
        if (!ok) { input.checked = true; return; }
        try { await invoke("disable_demo_mode"); }
        catch (err) { errToast(err); input.checked = true; }
      }
      return;
    }
    // The certificate lifetime switch makes a new certificate: confirmed first,
    // saved together with the certificate by its own command, and the switch
    // goes back to where it was when it is cancelled or fails.
    if (key === "remoteTlsShortLived") {
      if (!(await toggleShortLivedCert(enabled))) input.checked = !enabled;
      else syncRowReset("flags", key); // the reset icon follows the new value
      return;
    }
    // Turning sync on starts writing every text prompt as a plain-text .md file
    // next to the encrypted DB: say so before it happens, not just in the help doc.
    if (key === "promptSync" && enabled) {
      const ok = await confirmDialog({
        title: t(flagLabelKey(key)),
        message: t("syncPlaintextConfirm"),
        confirmLabel: t("syncPlaintextConfirmOk"),
      });
      if (!ok) { input.checked = false; return; }
    }
    settings.ui_flags = { ...(settings.ui_flags || {}), [key]: enabled };
    let accepted = true;
    try {
      await invoke("set_ui_flag", { key, enabled });
    } catch (err) {
      accepted = false;
      // The backend refused to switch the feature on (e.g. prompt sync whose safety backup or first
      // folder write failed) and left the flag off; the switch has to follow, since a checkbox
      // showing a state the store lacks is worse than the error.
      const msg = String(err);
      if (msg.startsWith("sync-backup-failed")) toast(t("syncBackupFailed"));
      else if (msg.startsWith("sync-write-failed")) toast(t("syncWriteFailed"));
      else if (msg === "demo-mode-active") toast(t("demoModeMultipleInstances"));
      else if (msg === "save-failed") toast(t("persistFailed"));
      else if (msg === "remote-off-not-saved") toast(t("remoteOffNotSaved"));
      else if (msg === "remote-network-off-not-saved") toast(t("remoteNetworkOffNotSaved"));
      else if (msg === "the certificate key could not be protected") toast(t("remoteTlsKeyNotProtected"));
      else errToast(err);
      // Switching the receiver off always takes effect, even when the write did
      // not land, so that one error keeps the switch off instead of reverting it.
      const stayed = msg === "remote-off-not-saved" || msg === "remote-network-off-not-saved";
      settings.ui_flags = { ...(settings.ui_flags || {}), [key]: stayed ? enabled : !enabled };
      input.checked = stayed ? enabled : !enabled;
    }
    // Turning the receiver on may have just minted its very first password
    // (see set_ui_flag's own remoteApi branch): shown here, once, in a
    // selectable field, and never retrievable again after this.
    if (key === "remoteApi" && enabled) {
      const shown = await invoke("remote_api_take_shown_password").catch(() => null);
      if (shown) {
        await confirmDialog({
          title: t("remotePasswordDialogTitle"),
          message: t("remotePasswordDialogMessageNew"),
          input: { type: "text", value: shown },
          confirmLabel: t("close"),
          oneButton: true,
        });
      }
    }
    // Read once at process start (before the builder exists): flipping it live
    // cannot change which guard is already running.
    if (key === "allowMultipleInstances" && accepted) toast(t("toastRestartNeeded"));
    applyFlags();
    if (key === "clipWatcher" && !enabled && accepted) await offerClipInboxCleanup();
    // "only changed" must recompute after every flag, not just gating ones: the row set depends on
    // this flag's own value.
    if (GATING_FLAGS.has(key) || expertOnlyChanged) renderExpert();
    else syncRowReset("flags", key); // reset icon must not wait for an unrelated rebuild
    renderViews();
    // Only these flags change what a tile contains; the rest are body classes,
    // so a full grid rebuild (and its flash) is unnecessary.
    if (TILE_DOM_FLAGS.has(key)) await renderGrid(true);
    else fitAllTiles();
  });
  const resetBtn = rowResetBtn("flags", key);
  if (resetBtn) row.append(resetBtn);
  row.append(input);
  return row;
}

// Re-fit the tile text to the new box size without touching the DOM.
export const refreshTiles = (key) => {
  // A view tab paints its ink as an inline style, so it needs its own
  // re-render: no CSS var carries the ink rule.
  if (key === "inkThreshold" || key === "inkHaloStrength") renderViews();
  return GRID_DOM_VALUES.has(key) ? renderGrid(true) : (fitAllTiles(), undefined);
};

// Live slider preview: reflect the value visually while dragging (CSS vars + at most
// one refit per frame), then persist it on release.
export let liveRenderQueued = false;
// offAtZero value -> was it above 0 when the current drag began. A drag writes the
// value live, so commitValue() could not tell otherwise whether it crossed zero.
export const offAtZeroWas = {};
// The stored value a drag started from: previewValue overwrites it live, and a refused
// write has to put this one back, not the last dragged position.
const valueBeforeDrag = {};
export function previewValue(key, value) {
  if (EXPERT_VALUES[key]?.offAtZero && !(key in offAtZeroWas)) offAtZeroWas[key] = val(key) > 0;
  if (!(key in valueBeforeDrag)) valueBeforeDrag[key] = (settings.ui_values || {})[key];
  settings.ui_values = { ...(settings.ui_values || {}), [key]: value };
  applyValues();
  if (liveRenderQueued) return;
  liveRenderQueued = true;
  requestAnimationFrame(() => { liveRenderQueued = false; refreshTiles(key); });
}
export async function commitValue(key, value) {
  const before = key in valueBeforeDrag ? valueBeforeDrag[key] : (settings.ui_values || {})[key];
  delete valueBeforeDrag[key];
  const wasOn = key in offAtZeroWas ? offAtZeroWas[key] : val(key) > 0;
  delete offAtZeroWas[key];
  settings.ui_values = { ...(settings.ui_values || {}), [key]: value };
  let failed = false;
  try { await invoke("set_ui_value", { key, value }); } catch (err) {
    errToast(err);
    // The backend kept the old value: show that one again.
    failed = true;
    const kept = { ...settings.ui_values };
    if (before === undefined) delete kept[key]; else kept[key] = before;
    settings.ui_values = kept;
  }
  applyValues();
  await refreshTiles(key);
  // The relock refetch is timed from the old timeout: a render asks the backend again.
  if (key === "secretTimeoutMin") await renderGrid(true);
  // Fires once on release, not per drag frame: same "no flash mid-drag" reason
  // previewValue() never calls this, safe to rebuild the (possibly filtered) list here.
  // An off-at-zero value that crossed zero opens or closes the rows gated on it.
  const crossedZero = EXPERT_VALUES[key]?.offAtZero && wasOn !== (val(key) > 0);
  if (failed || expertOnlyChanged || crossedZero) renderExpert();
  else syncRowReset("values", key); // reset icon must not wait for an unrelated rebuild
}

// How a number reads on its row. The stored value never changes, only the text:
// milliseconds show as seconds and the error log size in megabytes.
// A 0-stop can carry a special label (e.g. auto-advance "unlimited") instead of "0 min".
export function formatValue(cfg, v) {
  if (v === 0 && cfg.zeroLabel) return t(cfg.zeroLabel);
  const nf = (opts) => new Intl.NumberFormat(LANG, opts);
  if (cfg.display === "mb") return `${nf({ minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(v / 1024)} MB`;
  // seconds keep the SI symbol, which reads the same in every language
  if (cfg.unit === "ms") return `${nf({ minimumFractionDigits: 0, maximumFractionDigits: (cfg.step ?? 100) < 100 ? 2 : 1 }).format(v / 1000)} s`;
  const timeUnit = { d: "day", h: "hour", min: "minute" }[cfg.unit];
  if (timeUnit) return fmtUnit(v, timeUnit, LANG, 2);
  // One style for every unit: number, space, unit; percent, the times sign and a per-minute rate hug the number.
  return `${v}${cfg.unit && !/^[%×]$|^\//.test(cfg.unit) ? " " : ""}${cfg.unit}`;
}

export function valueRow(key) {
  const cfg = EXPERT_VALUES[key];
  const fmt = (v) => formatValue(cfg, v);
  const row = document.createElement("div");
  row.className = "field value-field";
  // A stable hook a caller elsewhere (the retention overview's jump-to-setting
  // button, so far) can scroll to and flash without re-deriving DOM structure.
  row.dataset.settingRow = key;
  withTip(row, key);
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(cfg.label);
  // A number typed directly instead of a slider (e.g. a port): every valid
  // value in range, nothing else, and no silent fallback to the last good one.
  if (cfg.input) {
    const right2 = document.createElement("span");
    right2.className = "value-head-right";
    const resetBtn2 = rowResetBtn("values", key);
    if (resetBtn2) right2.append(resetBtn2);
    head.append(span, right2);
    const input = document.createElement("input");
    input.type = "text";
    input.inputMode = "numeric";
    input.className = "modal-input expert-num";
    input.value = String(val(key));
    input.setAttribute("aria-invalid", "false");
    const hint = document.createElement("div");
    hint.className = "expert-row-hint expert-row-hint-danger hidden";
    hint.textContent = t(cfg.invalidKey || "valPortInvalid");
    const commit = () => {
      const n = Number(input.value);
      const valid = Number.isInteger(n) && n >= cfg.min && n <= cfg.max;
      input.classList.toggle("invalid", !valid);
      input.setAttribute("aria-invalid", String(!valid));
      hint.classList.toggle("hidden", valid);
      if (valid) commitValue(key, n);
    };
    input.addEventListener("change", commit);
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") { e.preventDefault(); commit(); }
    });
    row.append(head, input, hint);
    return row;
  }
  const out = document.createElement("span");
  out.className = "value-out";
  const slider = document.createElement("input");
  slider.type = "range";
  slider.className = "value-range";
  // topUnlimited adds one extra stop past the real max: a slider parked there shows the
  // "unlimited" label and stores 0, like a 0-stop's zeroLabel at the low end.
  const unlimitedStop = cfg.topUnlimited ? cfg.max + cfg.step : null;
  const isUnlimitedStop = () => cfg.topUnlimited && Number(slider.value) === unlimitedStop;
  // A `stops` list drives a stepped slider (e.g. history length up to a million,
  // coarser as it climbs); everything else is a plain min/max/step slider.
  const cur = cfg.stops
    ? () => cfg.stops[Number(slider.value)]
    : () => (isUnlimitedStop() ? 0 : Number(slider.value));
  const show = cfg.stops ? (v) => String(v) : (v) => (isUnlimitedStop() ? t("valUnlimited") : fmt(v));
  if (cfg.stops) {
    slider.min = 0;
    slider.max = cfg.stops.length - 1;
    slider.step = 1;
    const idx = cfg.stops.findIndex((s) => s >= val(key));
    slider.value = idx < 0 ? cfg.stops.length - 1 : idx;
  } else {
    slider.min = cfg.min;
    slider.max = unlimitedStop ?? cfg.max;
    slider.step = cfg.step;
    const stored = val(key);
    slider.value = cfg.topUnlimited && stored === 0 ? unlimitedStop : stored;
  }
  out.textContent = show(cur());
  slider.addEventListener("input", () => { out.textContent = show(cur()); previewValue(key, cur()); });
  slider.addEventListener("change", () => commitValue(key, cur()));
  // .value-head is `justify-content: space-between` over exactly two children;
  // a 3rd sibling would misplace, so the reset icon joins `out` in one flex group.
  const right = document.createElement("span");
  right.className = "value-head-right";
  right.append(out);
  const resetBtn = rowResetBtn("values", key);
  if (resetBtn) right.append(resetBtn);
  head.append(span, right);
  row.append(head, slider);
  // The quality slider is JPEG-only (every other screenshot format is lossless, see
  // encode_screenshot in capture.rs). dropdownRow's change handler toggles this row when
  // screenshotFormat changes, so it needs no listener of its own.
  if (key === "snipSaveQuality") {
    row.classList.toggle("hidden", txt("screenshotFormat") !== "jpeg");
  }
  return row;
}

// A preset dropdown (e.g. 1d/3d/7d/30d) plus a free-entry "custom" option.
export function selectRow(key) {
  const cfg = EXPERT_SELECTS[key];
  const cur = Number.isFinite(settings.ui_values?.[key]) ? settings.ui_values[key] : cfg.def;
  const row = document.createElement("div");
  row.className = "field value-field expert-dropdown-row"; // label left of the dropdown
  row.dataset.settingRow = key; // same jump-to-setting hook as valueRow
  withTip(row, key);
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(cfg.label);
  head.appendChild(span);
  const resetBtn = rowResetBtn("selects", key);
  if (resetBtn) head.appendChild(resetBtn);
  const sel = document.createElement("select");
  sel.className = "modal-input expert-select";
  for (const o of cfg.options) {
    const opt = document.createElement("option");
    opt.value = String(o);
    // A 0-stop can carry a special label (e.g. retention "∞ forever") instead of "0d".
    opt.textContent = formatValue(cfg, o);
    sel.appendChild(opt);
  }
  const customOpt = document.createElement("option");
  customOpt.value = "custom";
  customOpt.textContent = t("valCustom");
  sel.appendChild(customOpt);
  const num = document.createElement("input");
  num.type = "number";
  num.min = SELECT_CUSTOM_MIN;
  num.max = SELECT_CUSTOM_MAX;
  num.className = "modal-input expert-num";
  // The presets show their unit ("7d", "24h") on the option, but the free-entry field holds only
  // the number, so the unit sits beside it: otherwise a typed value looks unit-less and invites
  // typing the unit into it, which a number input silently discards.
  const unit = document.createElement("span");
  unit.className = "expert-num-unit";
  unit.textContent = cfg.unit;
  const isPreset = cfg.options.includes(cur);
  sel.value = isPreset ? String(cur) : "custom";
  num.value = cur;
  num.classList.toggle("hidden", isPreset);
  unit.classList.toggle("hidden", isPreset);
  attachSelectPicker(sel); // themed popup instead of the native OS dropdown
  // Show what the store really holds: the select, the number field and its unit.
  const resync = () => {
    const now = Number.isFinite(settings.ui_values?.[key]) ? settings.ui_values[key] : cfg.def;
    const preset = cfg.options.includes(now);
    sel.value = preset ? String(now) : "custom";
    num.value = now;
    num.classList.toggle("hidden", preset);
    unit.classList.toggle("hidden", preset);
  };
  // A refused write puts the previous value back and resyncs the controls.
  const persist = async (value) => {
    const had = Object.prototype.hasOwnProperty.call(settings.ui_values || {}, key);
    const before = settings.ui_values?.[key];
    settings.ui_values = { ...(settings.ui_values || {}), [key]: value };
    try {
      await invoke("set_ui_value", { key, value });
    } catch (err) {
      const back = { ...(settings.ui_values || {}) };
      if (had) back[key] = before; else delete back[key];
      settings.ui_values = back;
      errToast(err);
      resync();
    }
  };
  sel.addEventListener("change", () => {
    if (sel.value === "custom") {
      num.classList.remove("hidden");
      unit.classList.remove("hidden");
      num.focus();
      // No re-render here even under "only changed": it would rebuild the row (a fresh, unfocused
      // num) out from under the focus() above. The number field's own "change" below covers the
      // filter once a value is committed.
      persist(Math.max(1, Number(num.value) || cfg.def)).then(() => syncRowReset("selects", key));
    } else {
      num.classList.add("hidden");
      unit.classList.add("hidden");
      persist(Number(sel.value)).then(() => syncRowReset("selects", key));
      if (expertOnlyChanged) renderExpert();
    }
  });
  num.addEventListener("change", () => {
    const v = Math.max(SELECT_CUSTOM_MIN, Math.min(SELECT_CUSTOM_MAX, Number(num.value) || cfg.def));
    num.value = v;
    persist(v).then(() => syncRowReset("selects", key));
    if (expertOnlyChanged) renderExpert();
  });
  row.append(head, sel, num, unit);
  return row;
}

// A plain value/font dropdown (no custom entry), like the settings selects.
export function dropdownRow(key) {
  const cfg = EXPERT_DROPDOWNS[key];
  const row = document.createElement("div");
  row.className = "field value-field expert-dropdown-row"; // label left of the dropdown
  row.dataset.settingRow = key; // same jump-to-setting hook as valueRow
  withTip(row, key);
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(cfg.label);
  head.appendChild(span);
  const resetBtn = rowResetBtn("dropdowns", key);
  if (resetBtn) head.appendChild(resetBtn);
  const sel = document.createElement("select");
  sel.className = "modal-input expert-select";
  for (const [value, lbl, stack] of cfg.options()) {
    const o = document.createElement("option");
    o.value = value;
    o.textContent = lbl;
    if (stack) o.style.fontFamily = stack;
    sel.appendChild(o);
  }
  sel.value = cfg.get();
  sel.addEventListener("change", async () => {
    await cfg.set(sel.value);
    // A setter may decline the picked value; resync so the select never shows a choice that did
    // not take effect (a no-op when get() already reports what set() wrote).
    sel.value = cfg.get();
    // The JPEG-only quality slider follows the format without rebuilding the menu.
    if (key === "screenshotFormat") {
      document.querySelector('[data-setting-row="snipSaveQuality"]')?.classList.toggle("hidden", cfg.get() !== "jpeg");
    }
    if (expertOnlyChanged) renderExpert();
    else syncRowReset("dropdowns", key); // reset icon must not wait for an unrelated rebuild
  });
  attachSelectPicker(sel); // themed popup instead of the native OS dropdown
  row.append(head, sel);
  if (key === "recQuality" || key === "recFormat") row.appendChild(recFfmpegHintEl(key, sel));
  return row;
}

// The reason line under recQuality/recFormat: the GIF format's fixed 30s cap (always shown)
// and, when ffmpeg is missing, why the picked step/format cannot be produced plus an inline
// offer to fetch it, on the control itself and not only in the help.
export function recFfmpegHintEl(key, sel) {
  const hint = document.createElement("div");
  hint.className = "expert-row-hint";
  const update = () => {
    ensureFfmpegStatusChecked();
    const v = sel.value;
    const needsFfmpeg = key === "recQuality" ? v === "lossless" : v !== "mp4";
    const parts = [];
    if (key === "recFormat" && v === "gif") parts.push(t("recFormatGifHint"));
    if (key === "recQuality" && v === "lossless") parts.push(t("recQualityLosslessHint"));
    if (needsFfmpeg && ffmpegAvailable === false) parts.push(t("recFfmpegMissingHint"));
    hint.classList.toggle("hidden", parts.length === 0);
    hint.replaceChildren(document.createTextNode(parts.join(" ")));
    if (needsFfmpeg && ffmpegAvailable === false) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "ghost-btn expert-row-hint-btn";
      btn.textContent = t("ffmpegOfferConfirm");
      btn.addEventListener("click", () => EXPERT_ACTIONS["ffmpeg-download"]?.(btn));
      hint.appendChild(btn);
    }
  };
  update();
  sel.addEventListener("change", update);
  return hint;
}

// Folder-path options (expert): current path + a Change (folder picker) and,
// where applicable, a reset-to-default button.
export let currentDataDir = "";
export function assignCurrentDataDir(v) { currentDataDir = v; }
export let defaultScreenshotDir = "";
export function assignDefaultScreenshotDir(v) { defaultScreenshotDir = v; }
export let defaultSyncDir = "";
export function assignDefaultSyncDir(v) { defaultSyncDir = v; }
export let defaultRecordingDir = "";
export function assignDefaultRecordingDir(v) { defaultRecordingDir = v; }
export let ocrLangsCache = []; // [{tag, label}], fetched once at startup (init())
export function assignOcrLangsCache(v) { ocrLangsCache = v; }
export let micDevicesCache = []; // [{id, name}], fetched once at startup (init())
export function assignMicDevicesCache(v) { micDevicesCache = v; }
export let cameraDevicesCache = []; // [{id, name}], fetched once at startup (init())
export function assignCameraDevicesCache(v) { cameraDevicesCache = v; }
// Whether ffmpeg was found (see ffmpeg_status: never probed on app start, only the first time
// a control that needs it renders, i.e. the recQuality/recFormat expert rows).
// null = not checked yet.
export let ffmpegAvailable = null; // null = unknown/unchecked, true/false = confirmed present/absent
export let ffmpegChecking = false; // separate in-flight guard -- a slow probe must not render as "confirmed missing"
export function ensureFfmpegStatusChecked() {
  if (ffmpegAvailable !== null || ffmpegChecking) return;
  ffmpegChecking = true;
  invoke("ffmpeg_status")
    .then((p) => { ffmpegAvailable = !!p; ffmpegChecking = false; renderExpert(); })
    // A failed probe (the call itself erroring, not "ffmpeg missing") leaves this null rather than
    // latching false, so the next render retries. It does not re-render itself: that would start
    // the probe again at once, an endless loop of failing calls.
    .catch(() => { ffmpegAvailable = null; ffmpegChecking = false; });
}
export let voiceLangsCache = []; // [{tag, label}], fetched once at startup (init())
export function assignVoiceLangsCache(v) { voiceLangsCache = v; }
// voice_support()'s answer, optimistic before it resolves (this app is
// Windows-only, so an unsupported machine is the rare case, not the default).
export let voiceSupport = { supported: true, engine: "", langs: [], needs_online: false };
export function assignVoiceSupport(v) { voiceSupport = v; }
// Whether the backend still manages to seal everything it writes. Read once at startup and
// pushed by the "store-unsealed" event if it changes; assumed true until the backend says
// otherwise, so a failed call never invents a warning.
export let storeSealed = true;
export function assignStoreSealed(v) { storeSealed = v; }
// Whether the settings row on disk was present but unreadable at the last load (query error,
// decrypt failure, corrupt JSON). Unlike storeSealed it only changes through reset/import,
// which reload the whole window, so refreshSettingsLocked() is called from there instead of a
// push event. Assumed false until the backend says otherwise.
export let settingsLocked = false;
export async function refreshSettingsLocked() {
  settingsLocked = (await invoke("settings_locked").catch(() => false)) === true;
  if (!$("settings-expert").classList.contains("hidden")) renderExpert();
}
// Whether the last settings write reached disk. Like storeSealed it can flip mid-session, so
// the live state comes from the "persist-failed" event rather than a poll.
export let settingsWriteFailed = false;
export function assignSettingsWriteFailed(v) { settingsWriteFailed = v; }

// ---- Data folder lock -----------------------------------------------------
// Whether another machine's fresh lock on the data folder makes this process read-only, and
// which machine. Decided once at load (or right after set_data_dir points at a new folder);
// nothing changes it mid-session, so a poll at those two moments is enough.
export let dataDirLockStatus = { readOnly: false, otherHost: null };
export async function refreshDataDirLock() {
  dataDirLockStatus = (await invoke("data_dir_lock_status").catch(() => null)) || { readOnly: false, otherHost: null };
  renderDataDirReadOnlyBanner();
  if (!$("settings-expert").classList.contains("hidden")) renderExpert();
}
// The persistent line in the window itself, not only the expert-menu notice below: a user who
// never opens the expert menu still has to know their edits are not being saved.
export function renderDataDirReadOnlyBanner() {
  const banner = $("data-dir-readonly-banner");
  if (!banner) return;
  if (!dataDirLockStatus.readOnly) { banner.classList.add("hidden"); syncBannerStack(); return; }
  setStatusText($("data-dir-readonly-text"), "dataDirReadOnlyBanner", "host", dataDirLockStatus.otherHost || "?");
  banner.classList.remove("hidden");
  syncBannerStack();
}

// One slot above the grid for every notice: only the most important active one
// shows, a counter beside it steps through the rest. Highest priority first; the
// remote access sign is the app's one network surface and must never be the
// hidden one.
export const BANNER_PRIORITY = ["remote-api-banner", "data-dir-readonly-banner", "demo-mode-banner", "grid-overflow", "examples-banner"];
export let bannerFront = 0; // which of the active notices is showing (index, wraps)
export let bannerActiveKey = ""; // joined ids of the active set the index belongs to
export function bannerIsActive(id) {
  if (id === "demo-mode-banner") return optFlag("demoMode");
  const el = $(id);
  return !!el && !el.classList.contains("hidden");
}
export function syncBannerStack() {
  const more = $("banner-more");
  if (!more) return;
  const active = BANNER_PRIORITY.filter(bannerIsActive);
  const key = active.join(",");
  if (key !== bannerActiveKey) { bannerActiveKey = key; bannerFront = 0; }
  const n = active.length;
  const front = n ? active[bannerFront % n] : "";
  for (const id of BANNER_PRIORITY) $(id)?.classList.toggle("banner-stacked", id !== front);
  more.classList.toggle("hidden", n < 2);
  if (n > 1) {
    more.textContent = t("bannerMore").replace("{n}", String(n - 1));
    more.setAttribute("aria-label", t("bannerMoreAria"));
  }
}
// Told once per session: the folder's sync risk does not change on its own mid-session
// (changing the folder does, which confirmSyncFolderChoice below warns about).
export let dataDirSyncWarnedThisSession = false;
export async function warnDataDirSyncFolder() {
  if (dataDirSyncWarnedThisSession) return;
  dataDirSyncWarnedThisSession = true;
  const provider = await invoke("current_data_dir_sync_hint").catch(() => null);
  if (!provider) return;
  await infoDialog(t("dataDirSyncTitle"), t("dataDirSyncMsg").replace("{provider}", () => provider), ICON_WARN);
}
// Asked right before committing to a newly picked data folder.
// Returns true when the folder is fine as-is or the user chose to use it anyway;
// false cancels the change before set_data_dir is ever called.
export async function confirmSyncFolderChoice(dir) {
  const provider = await invoke("path_sync_folder_hint", { path: dir }).catch(() => null);
  if (!provider) return true;
  return await confirmDialog({
    title: t("dataDirSyncTitle"),
    message: t("dataDirSyncMsg").replace("{provider}", () => provider),
    confirmLabel: t("dataDirSyncContinue"),
    cancelLabel: t("cancel"),
    icon: ICON_WARN,
  });
}

// A capture folder the backend refused is shown as the old one again: the row only changes
// once the command took the new path.
export async function setDirText(key, cmd, dir) {
  const texts = settings.ui_texts || {};
  const had = Object.prototype.hasOwnProperty.call(texts, key);
  const before = texts[key];
  settings.ui_texts = { ...texts, [key]: dir };
  try {
    await invoke(cmd, { dir });
  } catch (e) {
    const back = { ...(settings.ui_texts || {}) };
    if (had) back[key] = before; else delete back[key];
    settings.ui_texts = back;
    errToast(e);
  }
  renderExpert();
}

export const EXPERT_PATHS = {
  screenshotDir: {
    label: "valScreenshotDir",
    gate: "screenshot", // no folder picker when the screenshot tool is off
    def: () => defaultScreenshotDir,
    get: () => settings.ui_texts?.screenshotDir || defaultScreenshotDir,
    change: async () => {
      const dir = await invoke("pick_folder", { purpose: "screenshot" }).catch(() => null);
      if (!dir) return;
      await setDirText("screenshotDir", "set_screenshot_dir", dir);
    },
    onReset: async () => {
      await setDirText("screenshotDir", "set_screenshot_dir", "");
    },
  },
  // Same storage key as the video editor's "Export to…" folder picker (video.js
  // persistExportSettings/videoExportDir); this row only makes that preference visible and
  // resettable from the expert menu. Empty means "next to the source file".
  videoExportDir: {
    label: "valVideoExportDir",
    gate: "videoEditor",
    def: () => "",
    get: () => settings.ui_texts?.videoExportDir || "",
    change: async () => {
      const dir = await invoke("pick_folder", { purpose: "videoExport" }).catch(() => null);
      if (!dir) return;
      await setDirText("videoExportDir", "set_video_export_dir", dir);
    },
    onReset: async () => {
      await setDirText("videoExportDir", "set_video_export_dir", "");
    },
  },
  dataDir: {
    label: "valDataDir",
    get: () => currentDataDir,
    change: async () => {
      const dir = await invoke("pick_folder", { purpose: "data" }).catch(() => null);
      if (!dir) return;
      if (!(await confirmSyncFolderChoice(dir))) return;
      let oldPath;
      try { oldPath = await invoke("set_data_dir", { dir }); } catch (e) { errToast(e); return; }
      // A respelled path of the folder in use is no move: the backend hands back the
      // same current folder, and then there is nothing to offer to delete.
      const nowDir = await invoke("current_data_dir").catch(() => dir);
      const moved = nowDir !== oldPath;
      currentDataDir = moved ? dir : oldPath;
      // An adopted library replaces the one the window shows, and its vault starts locked.
      if (moved) await reloadEverything().catch((e) => errToast(e));
      renderExpert();
      refreshDataDirLock(); // the new folder may already carry another machine's fresh lock
      // Offer to delete the old data (default = keep). Message notes the restart.
      if (moved && oldPath && oldPath !== dir) {
        const del = await confirmDialog({
          title: t("valDataDir"),
          message: t("dataDirConfirm"),
          confirmLabel: t("delete"),
          cancelLabel: t("dataDirKeep"),
        });
        if (del) {
          const removed = await invoke("delete_data_dir", { path: oldPath }).catch(() => false);
          if (!removed) toast(t("dataDirCleanFailed"));
        }
      }
    },
  },
  syncDir: {
    label: "valSyncDir",
    gate: "promptSync",
    syncStats: true,
    def: () => defaultSyncDir,
    get: () => settings.ui_texts?.syncDir || defaultSyncDir,
    change: async () => {
      const dir = await invoke("pick_folder", { purpose: "sync" }).catch(() => null);
      if (!dir) return;
      await applySyncDir(dir);
    },
    // Back to the default folder is a folder change like any other, same question.
    onReset: () => { applySyncDir(""); },
  },
  // Target for the recording result window's plain "Save"/"Copy". The raw capture stays in
  // <data>/recordings until one of those moves it here (or the window closes unsaved and it is deleted).
  recordingDir: {
    label: "valRecordingDir",
    gate: "screenRecord",
    def: () => defaultRecordingDir,
    get: () => settings.ui_texts?.recordingDir || defaultRecordingDir,
    change: async () => {
      const dir = await invoke("pick_folder", { purpose: "recording" }).catch(() => null);
      if (!dir) return;
      await setDirText("recordingDir", "set_recording_dir", dir);
    },
    onReset: async () => {
      await setDirText("recordingDir", "set_recording_dir", "");
    },
    // The only path row that also shows how much space its folder holds, plus a cleanup button;
    // pathRow() renders both only when this is set.
    videoCleanup: true,
  },
};

// A colour through the shared colorpop.js picker (never a native <input type=color>). Unlike
// the accent palette above, each is one stored hex string in ui_texts, so it stays out of the
// generic EXPERT_DEFS/isRowChanged machinery, like `palette`.
export const EXPERT_COLORS = {
  // Read by the backend when a region recording starts (see show_rec_outline).
  recOutlineColor: { label: "valRecOutlineColor", def: "#2563eb", gate: "recOutline" },
  // `def` is a function here because the screenshot editor's fallback (COLORS[1]) tracks the
  // user's custom palette; a fixed hex would stop following a recoloured palette. colorRow()
  // below resolves a function `def` before use.
  snipDefaultColor: { label: "valSnipDefaultColor", def: () => COLORS[1] || "#ef4444", gate: "snipEditor" },
};

// (folder path -> {count, bytes}) for the recordingDir video-folder readout, so switching the
// folder and back does not re-fetch what is known. Falsy means "not fetched yet for this
// path"; pathRow() shows a loading placeholder until refreshVideoFolderInfo() resolves.
export let videoFolderInfo = null;
export let videoFolderInfoDir = null;
export let videoFolderGen = 0;
export function refreshVideoFolderInfo(dir) {
  videoFolderInfoDir = dir;
  videoFolderInfo = null; // the numbers on screen belong to the folder just left
  const gen = ++videoFolderGen;
  invoke("video_folder_stats", { dir })
    .then(([count, bytes]) => { if (gen === videoFolderGen) { videoFolderInfo = { count, bytes }; renderExpert(); } })
    .catch(() => { if (gen === videoFolderGen) { videoFolderInfo = { count: 0, bytes: 0 }; renderExpert(); } });
}

// Switching the Markdown-sync folder. The files in the old folder stay unless the user says
// otherwise, so the question comes first and its answer rides along as an explicit
// instruction: the backend never deletes a folder by itself, and only removes files this app
// wrote (a stranger's file in there stops the whole delete and is reported).
export async function applySyncDir(dir) {
  const next = dir || "";
  if ((settings.ui_texts?.syncDir || "") === next) return;
  const info = await invoke("sync_dir_info").catch(() => null);
  let deleteOld = false;
  if (info && info.ours > 0) {
    const answer = await confirmDialog({
      title: t("syncOldDirTitle"),
      message: t("syncOldDirMsg").replace("{path}", () => info.path).replace("{n}", () => info.ours),
      confirmLabel: t("syncOldDirDelete"),
      altLabel: t("syncOldDirKeep"),
      cancelLabel: t("cancel"),
    });
    if (answer === false) return; // cancelled: the folder is not switched at all
    deleteOld = answer === true;
  }
  const previous = settings.ui_texts?.syncDir || "";
  settings.ui_texts = { ...(settings.ui_texts || {}), syncDir: next };
  renderExpert();
  try {
    await invoke("set_sync_dir", { dir: next, deleteOld });
  } catch (e) {
    // "dir-not-picked" means the backend refused before touching anything, so the folder shown
    // here goes back to what it still is. Any other error means the switch went through and only
    // the old-folder delete was refused, which the "old folder kept" dialog below covers.
    if (String(e) === "dir-not-picked") {
      settings.ui_texts = { ...(settings.ui_texts || {}), syncDir: previous };
      renderExpert();
      errToast(e);
      refreshSyncStatus(true);
      return;
    }
    // The switch itself went through; only the delete refused. Name what is left
    // in the old folder and why nothing there was touched.
    await infoDialog(
      t("syncOldDirTitle"),
      t("syncOldDirKept").replace("{path}", () => info?.path || "").replace("{what}", () => errText(e)),
    );
  }
  refreshSyncStatus(true);
}

// Live state of the Markdown sync, shown under its folder row: when the folder was last read,
// how many files it held, and why it is paused if it is. Fetched at most once per expert-menu
// render, so a second render while the answer is on its way starts no second round trip.
export let syncStatus = null;
export let syncStatusPending = false;
// Asked-at-all, not answered: a command that fails (or answers with nothing) must
// leave the row saying so, never start the next request from the redraw its own
// answer triggers. That would be an endless render loop, not a retry.
export let syncStatusAsked = false;
export function refreshSyncStatus(force) {
  if (syncStatusPending || (syncStatusAsked && !force)) return;
  syncStatusPending = true;
  syncStatusAsked = true;
  invoke("sync_status")
    .then((s) => { syncStatus = s || null; })
    .catch(() => { syncStatus = null; })
    .finally(() => { syncStatusPending = false; renderExpert(); });
}

// "just now" / "3 min ago" / a plain time for anything older, from a unix second.
export function syncAgoText(unixSecs) {
  if (!unixSecs) return t("syncStatsNever");
  const mins = Math.max(0, Math.round((Date.now() / 1000 - unixSecs) / 60));
  if (mins < 1) return t("syncStatsJustNow");
  if (mins < 60) return t("syncStatsMinsAgo").replace("{n}", () => mins);
  const when = new Date(unixSecs * 1000);
  const today = when.toDateString() === new Date().toDateString();
  return today
    ? when.toLocaleTimeString(LANG, { hour: "2-digit", minute: "2-digit" })
    : when.toLocaleString(LANG, { dateStyle: "short", timeStyle: "short" });
}

export function pathRow(key) {
  const cfg = EXPERT_PATHS[key];
  const row = document.createElement("div");
  row.className = "field value-field path-field";
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(cfg.label);
  const out = document.createElement("span");
  out.className = "value-out path-out";
  const cur = cfg.get() || t("pathDefault");
  out.textContent = cur;
  out.title = cur;
  head.append(span, out);
  const btns = document.createElement("div");
  btns.className = "path-btns";
  const change = document.createElement("button");
  change.type = "button";
  change.className = "ghost-btn";
  change.textContent = t("pathChange");
  change.addEventListener("click", cfg.change);
  btns.appendChild(change);
  const resetBtn = rowResetBtn("paths", key);
  if (resetBtn) btns.appendChild(resetBtn);
  row.append(head, btns);
  if (cfg.videoCleanup) {
    const dir = cfg.get();
    if (dir !== videoFolderInfoDir) refreshVideoFolderInfo(dir);
    const info = document.createElement("div");
    info.className = "path-video-stats";
    const text = document.createElement("span");
    text.textContent = videoFolderInfo
      ? t("videoFolderStatsLine").replace("{n}", () => videoFolderInfo.count).replace("{size}", fmtBytes(videoFolderInfo.bytes, LANG))
      : t("videoFolderStatsLoading");
    const cleanBtn = document.createElement("button");
    cleanBtn.type = "button";
    cleanBtn.className = "ghost-btn";
    cleanBtn.textContent = t("videoFolderCleanupBtn");
    cleanBtn.disabled = !videoFolderInfo || videoFolderInfo.count === 0;
    cleanBtn.addEventListener("click", () => openVideoCleanup(dir));
    info.append(text, cleanBtn);
    row.appendChild(info);
  }
  if (cfg.syncStats) {
    refreshSyncStatus();
    const info = document.createElement("div");
    info.className = "path-sync-stats";
    info.setAttribute("role", "status");
    const line = document.createElement("span");
    if (!syncStatus) {
      line.textContent = t("syncStatsLoading");
    } else if (syncStatus.paused) {
      line.textContent = t("syncStatsPaused").replace("{why}", () => syncStatus.reason || t("syncStatsNoFolder"));
    } else {
      line.textContent = t("syncStatsLine")
        .replace("{when}", () => syncAgoText(syncStatus.last_ok))
        .replace("{n}", () => syncStatus.files);
    }
    info.appendChild(line);
    row.appendChild(info);
  }
  return withTip(row, key);
}

export let expertTab = 0;
export let expertQuery = ""; // expert-menu search filter
export let expertOnlyChanged = false; // hide every row still at its default
// The filter is a pill whose checkbox is visually hidden, so the "on" look is a
// class rather than :checked (:has() is avoided project-wide, see main.css:733).
export const syncOnlyChangedChip = () =>
  $("expert-only-changed-chip")?.classList.toggle("active", expertOnlyChanged);
// Does this row differ from its shipped default? Actions and the colour palette
// have no single "default value" to diff against, so they are excluded up in
// renderExpert() instead of handled here.
export function isRowChanged(kind, key) {
  // A bulk reset never touches these, so they are never offered one.
  if (kind === "flags") return !EXPERT_BULK_NEVER.includes(key) && flagState(key) !== shipsOn(key);
  if (kind === "values" || kind === "selects") {
    const cfg = EXPERT_VALUES[key] || EXPERT_SELECTS[key];
    return val(key) !== cfg.def;
  }
  if (kind === "dropdowns") {
    const cfg = EXPERT_DROPDOWNS[key];
    return cfg.def !== undefined && cfg.get() !== cfg.def;
  }
  if (kind === "paths") {
    // Only a path with a "reset to default" button (screenshotDir, syncDir) has a
    // default to diff against: dataDir is always "current", never changed from
    // anything. A stored value equal to the (computed) default is unchanged:
    // picking exactly the default folder must not mark the row as changed.
    const cfg = EXPERT_PATHS[key];
    if (!cfg.onReset || !cfg.def) return false;
    const stored = settings.ui_texts?.[key] || "";
    return !!stored && stored !== cfg.def();
  }
  return false;
}
// Editable default-color palette: one themed color picker per hue + reset.
export function paletteRow() {
  const row = document.createElement("div");
  row.className = "field palette-field";
  const grid = document.createElement("div");
  grid.className = "palette-grid";
  const cur = COLORS.slice(1);
  const hues = [];
  for (let i = 0; i < PALETTE_LEN; i++) {
    hues.push(HEX_RE.test(cur[i]) ? cur[i] : DEFAULT_COLORS[i + 1]);
    const c = document.createElement("button");
    c.type = "button";
    c.className = "palette-swatch";
    const paint = (hex) => { c.style.background = hex; c.title = hex; };
    paint(hues[i]);
    c.setAttribute("aria-label", t("paletteSlot").replace("{n}", () => String(i + 1)));
    c.addEventListener("click", () => openColorPop(c, hues[i], (hex) => {
      hues[i] = hex;
      paint(hex);
      savePalette(hues.slice());
    }));
    grid.appendChild(c);
  }
  // Only show it while a hue actually differs from DEFAULT_COLORS: same rule
  // as every other row's rowResetBtn(), the palette just has no single key to diff.
  const changed = cur.some((c, i) => (HEX_RE.test(c) ? c : DEFAULT_COLORS[i + 1]) !== DEFAULT_COLORS[i + 1]);
  if (changed) {
    const reset = document.createElement("button");
    reset.type = "button";
    reset.className = "ghost-btn palette-reset";
    reset.textContent = t("paletteReset");
    reset.addEventListener("click", () => savePalette(null));
    row.append(grid, reset);
  } else {
    row.append(grid);
  }
  return row;
}

// Persist the palette (null = reset to defaults), remap every saved prompt/view
// color from the old hue to the new one, and refresh every colour surface live.
export async function savePalette(colors) {
  const oldHues = COLORS.slice(1);
  const value = colors ? JSON.stringify(colors) : "";
  const texts = settings.ui_texts || {};
  const had = Object.prototype.hasOwnProperty.call(texts, "palette");
  const before = texts.palette;
  settings.ui_texts = { ...texts, palette: value };
  try {
    await invoke("set_ui_text", { key: "palette", value });
  } catch (e) {
    // The store kept the old palette: put it back and leave every saved colour alone.
    const back = { ...(settings.ui_texts || {}) };
    if (had) back.palette = before; else delete back.palette;
    settings.ui_texts = back;
    errToast(e);
    loadPalette();
    renderExpert();
    return;
  }
  loadPalette();
  const newHues = COLORS.slice(1);
  const pairs = [];
  for (let i = 0; i < PALETTE_LEN; i++) {
    if (oldHues[i] && newHues[i] && oldHues[i] !== newHues[i]) pairs.push([oldHues[i], newHues[i]]);
  }
  if (pairs.length) {
    try { await invoke("remap_colors", { pairs }); } catch (e) { errToast(e); }
    // Reload the caches so the grid/views reflect the remap without a restart.
    try {
      const s = await invoke("get_state");
      assignPrompts(s.prompts);
      assignSettings(s.settings);
      loadPalette();
    } catch (_) { /* keep going with the in-memory state */ }
  }
  refreshColorUI();
  renderViews();
  renderExpert();
  await renderGrid(true);
}

export function colorRow(key) {
  const cfg = EXPERT_COLORS[key];
  // `def` is usually a fixed hex, but snipDefaultColor's is a function that
  // reads the user's live custom palette: resolve it fresh every render.
  const defColor = typeof cfg.def === "function" ? cfg.def() : cfg.def;
  const row = document.createElement("div");
  row.className = "field value-field color-field";
  row.dataset.settingRow = key; // same jump-to-setting hook as valueRow
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(cfg.label);
  const cur = HEX_RE.test(settings.ui_texts?.[key] || "") ? settings.ui_texts[key] : defColor;
  // .value-head is space-between over exactly two children: label plus this
  // wrapper, the same reason valueRow's slider reset joins .value-out instead
  // of becoming a 3rd sibling.
  const right = document.createElement("span");
  right.className = "value-head-right";
  const swatch = document.createElement("button");
  swatch.type = "button";
  swatch.className = "color-swatch";
  swatch.style.background = cur;
  swatch.title = cur;
  const hexOut = document.createElement("span");
  hexOut.className = "value-out";
  hexOut.textContent = cur;
  let reset = null;
  const setColor = async (hex) => {
    swatch.style.background = hex;
    swatch.title = hex;
    hexOut.textContent = hex;
    settings.ui_texts = { ...(settings.ui_texts || {}), [key]: hex };
    try { await invoke("set_ui_text", { key, value: hex }); } catch (e) { errToast(e); }
    if (reset) reset.classList.toggle("hidden", hex === defColor);
  };
  swatch.addEventListener("click", () => openColorPop(swatch, cur, setColor));
  right.append(swatch, hexOut);
  reset = document.createElement("button");
  reset.type = "button";
  reset.className = "expert-row-reset";
  reset.classList.toggle("hidden", cur === defColor);
  const label = t("expertResetOne");
  reset.title = label;
  reset.setAttribute("aria-label", label);
  putIcon(reset, ICON_UNDO);
  reset.addEventListener("click", (e) => { e.preventDefault(); e.stopPropagation(); setColor(defColor); });
  right.append(reset);
  head.append(span, right);
  row.appendChild(head);
  return withTip(row, key);
}

// Translation-key lookup per setting kind: used by the expert-menu search.
export const EXPERT_LABEL_KEYS = {
  flags: (k) => flagLabelKey(k),
  values: (k) => EXPERT_VALUES[k].label,
  selects: (k) => EXPERT_SELECTS[k].label,
  dropdowns: (k) => EXPERT_DROPDOWNS[k].label,
  paths: (k) => EXPERT_PATHS[k].label,
};
// The def a given setting kind lives in: so its optional `gate` can be read.
export const EXPERT_DEFS = { values: EXPERT_VALUES, selects: EXPERT_SELECTS, dropdowns: EXPERT_DROPDOWNS, paths: EXPERT_PATHS };
// Flags that own gated rows. Only those change the menu's shape when toggled; the rest are
// body classes, and re-rendering the whole tab for them made the list flicker.
export const GATING_FLAGS = new Set(
  [
    ...Object.values(EXPERT_DEFS).flatMap((defs) => Object.values(defs).map((d) => d.gate)),
    ...Object.values(FLAG_GATES),
    ...EXPERT_TABS.flatMap((tab) => tab.groups.flatMap((g) => (g.actions || []).map((a) => a.gate))),
  ].filter(Boolean).map((g) => g.replace(/^!/, ""))
);
// Which EXPERT_TABS tab a value/select key lives in, so a caller outside the
// expert menu (the retention overview, so far) can open the right tab
// before flashing the row: unlike SETTINGS_DIRECTORY above, which is only
// for settings OUTSIDE the expert menu, this walks EXPERT_TABS itself.
export function expertTabIndexOf(key, kind) {
  for (let i = 0; i < EXPERT_TABS.length; i++) {
    for (const g of EXPERT_TABS[i].groups) {
      // A flag entry is a bare key or { key, gate }; the other kinds are plain keys.
      if (kind === "flags" ? groupFlags(g).some((f) => f.key === key) : (g[kind] || []).includes(key)) return i;
    }
  }
  return 0;
}

// A click on the retention overview's "go to setting" button: close the
// backup window, open the expert menu on the right tab (query and "only
// changed" cleared so the row cannot be filtered out of view), then scroll
// it into view and flash it: same shape as openSettingsDirectoryEntry
// above, for a setting the expert search already owns.
export function jumpToExpertSetting(key, kind) {
  closeBackups();
  settingsEl.classList.remove("hidden");
  expertTab = expertTabIndexOf(key, kind);
  expertQuery = "";
  expertOnlyChanged = false;
  showExpertPage(true);
  flashSettingsRow(document.querySelector(`#settings-expert [data-setting-row="${key}"]`));
}

export function foreignHitRow(entry) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "field expert-foreign-row";
  const label = document.createElement("span");
  label.textContent = t(entry.label);
  const badge = document.createElement("span");
  badge.className = "expert-foreign-badge";
  badge.textContent = t(SETTINGS_AREA_LABEL[entry.area]);
  row.append(label, badge);
  row.addEventListener("click", () => openSettingsDirectoryEntry(entry));
  return row;
}

// Groups that start folded (kept tidy until wanted): troubleshooting is not for everyday use.
export const DIAGNOSTICS_GROUP = "expGroupDiagnostics";
export const DEFAULT_COLLAPSED_GROUPS = [DIAGNOSTICS_GROUP];
// Once per profile: a collapsed list stored before the Troubleshooting group existed
// gets it added, so the group starts folded for existing users too. A profile that
// never stored a list gets it from DEFAULT_COLLAPSED_GROUPS. The flag keeps this from
// folding the group again after the user has opened it.
export function seedDiagnosticsCollapsed() {
  if (settings.ui_flags?.diagGroupSeeded) return;
  settings.ui_flags = { ...(settings.ui_flags || {}), diagGroupSeeded: true };
  invoke("set_ui_flag", { key: "diagGroupSeeded", enabled: true }).catch(() => {});
  const stored = settings.ui_texts?.expertCollapsed;
  if (stored == null) return;
  let list = null;
  try { list = JSON.parse(stored); } catch (_) { return; }
  if (!Array.isArray(list) || list.includes(DIAGNOSTICS_GROUP)) return;
  const value = JSON.stringify([...list, DIAGNOSTICS_GROUP]);
  settings.ui_texts = { ...(settings.ui_texts || {}), expertCollapsed: value };
  invoke("set_ui_text", { key: "expertCollapsed", value }).catch(() => {});
}
// Collapsed expert categories, persisted as a JSON title list in ui_texts. Unset
// (first ever open) → everything expanded; after that the stored list wins.
export function expertCollapsedSet() {
  const stored = settings.ui_texts?.expertCollapsed;
  if (stored == null) return new Set(DEFAULT_COLLAPSED_GROUPS);
  try { return new Set(JSON.parse(stored)); } catch (_) { return new Set(); }
}
// Collapsing is a pure CSS state, so flip the class on the section itself. Going
// through renderExpert() rebuilt the whole tab (and re-fetched the backup/stats
// panels), which flashed on every click.
export function toggleExpertGroup(title, sec, fill) {
  const set = expertCollapsedSet();
  const collapsed = !set.has(title); // the state we are switching to
  collapsed ? set.add(title) : set.delete(title);
  const value = JSON.stringify([...set]);
  settings.ui_texts = { ...(settings.ui_texts || {}), expertCollapsed: value };
  invoke("set_ui_text", { key: "expertCollapsed", value }).catch(() => {});
  if (!sec) { renderExpert(); return; }
  sec.classList.toggle("collapsed", collapsed);
  if (!collapsed) fill?.(); // custom panels fill on first open
}

// A hotkey-capture row for the expert menu itself (click the input, press a combo, like every
// other hotkey field via bindHotkeyField). Rebuilt on every renderExpert() call, so the field
// is bound fresh each time rather than once at boot.
export const HOTKEY_ROW_DEFS = {
  recStopHotkey: { current: () => settings.rec_stop_hotkey, onCommit: commitRecStopHotkey },
  recPauseHotkey: { current: () => settings.rec_pause_hotkey, onCommit: commitRecPauseHotkey },
};
export function hotkeyRow(key, labelKey) {
  const def = HOTKEY_ROW_DEFS[key];
  const row = document.createElement("div");
  // Label above the capture field, stacked like pathRow: the expert menu's column is narrower
  // than the settings dialog's .field-inline layout, which would overflow.
  row.className = "field value-field hotkey-field";
  const head = document.createElement("div");
  head.className = "value-head";
  const span = document.createElement("span");
  span.textContent = t(labelKey);
  head.appendChild(span);
  const wrap = document.createElement("div");
  wrap.className = "hotkey-row";
  const input = document.createElement("input");
  input.className = "modal-input";
  input.type = "text";
  input.readOnly = true;
  input.placeholder = t("hotkeyPh");
  input.setAttribute("aria-label", t(labelKey));
  input.value = prettyAccel(def.current());
  wrap.append(input);
  row.append(head, wrap);
  hotkeyClearX(input, async () => {
    try { await def.onCommit(""); } catch (_) { /* Clear always leaves an empty field regardless */ }
    input.value = "";
  });
  bindHotkeyField(input, { current: def.current, onCommit: def.onCommit });
  return withTip(row, key);
}

// Rows that carry a label plus one narrow control fit two to a line; a slider needs
// the full width for its track, and anything with children must stay a full row so
// its subtree reads as one block.
export const COMPACT_KINDS = new Set(["dropdowns", "selects"]);
export const isCompactRow = (n) => COMPACT_KINDS.has(n.kind) && !n.kids.length;

// Emit one level of the row tree: runs of compact rows are paired into a two-column
// grid, everything else is a full-width row followed by its own subtree.
export function emitExpertRows(list, parent, actionById, depth) {
  const build = (n) => {
    const row = n.kind === "actions"
      ? actionRow(actionById.get(n.k))
      : n.kind === "varFixedList"
        ? varFixedListRow()
        : { flags: flagRow, values: valueRow, selects: selectRow, dropdowns: dropdownRow, paths: pathRow, colors: colorRow }[n.kind](n.k);
    // Indent by depth: a setting unlocked by a setting that is itself unlocked sits
    // one step further in, so the chain of dependencies reads off the left edge.
    if (depth > 0) {
      row.classList.add("expert-sub");
      row.style.setProperty("--expert-depth", String(depth));
    }
    return row;
  };
  for (let i = 0; i < list.length; i++) {
    if (isCompactRow(list[i])) {
      let j = i;
      while (j < list.length && isCompactRow(list[j])) j++;
      if (j - i >= 2) {
        const cols = document.createElement("div");
        cols.className = "expert-cols";
        if (depth > 0) {
          cols.classList.add("expert-sub");
          cols.style.setProperty("--expert-depth", String(depth));
        }
        for (let x = i; x < j; x++) {
          const row = build(list[x]);
          row.classList.remove("expert-sub"); // the wrapper carries the indent
          row.style.removeProperty("--expert-depth");
          cols.appendChild(row);
        }
        parent.appendChild(cols);
        i = j - 1;
        continue;
      }
    }
    parent.appendChild(build(list[i]));
    if (list[i].kids.length) emitExpertRows(list[i].kids, parent, actionById, depth + 1);
  }
}

// Scrolls the strip sideways (never the dialog) until the active tab is fully on screen,
// clear of the fade at either edge.
function showActiveTab(bar) {
  const act = bar.querySelector(".expert-tab.active");
  if (!act) return;
  const a = act.getBoundingClientRect(), b = bar.getBoundingClientRect();
  const FADE = 32;
  if (a.left < b.left + FADE) bar.scrollLeft -= b.left + FADE - a.left;
  else if (a.right > b.right - FADE) bar.scrollLeft += a.right - (b.right - FADE);
}

// The tab bar is rebuilt on every render; one observer follows the current bar.
let tabBarObserver = null;
// The reset button's wrapper lives in the page once; every render re-appends this same
// node (clearing the list detaches it), so the click listener and the armed state stay.
let expertEndEl = null;

export function renderExpert() {
  seedDiagnosticsCollapsed();
  const box = $("expert-flags");
  expertEndEl ??= $("expert-end");
  const tabBar = $("expert-tabs-bar");
  const keepScroll = box.scrollTop; // survive live re-renders (e.g. a toggled gate)
  const keepTabScroll = tabBar.querySelector(".expert-tabs")?.scrollLeft || 0;
  box.innerHTML = "";
  tabBar.innerHTML = "";
  if (tabBarObserver) { tabBarObserver.disconnect(); tabBarObserver = null; }
  if (expertTab >= EXPERT_TABS.length) expertTab = 0;
  const q = expertQuery.trim().toLowerCase();
  const searching = q.length > 0;
  // Like searching, "only changed" scans every tab at once: a per-tab filter
  // would defeat the point of a diff view across the whole expert menu.
  const globalView = searching || expertOnlyChanged;
  // Tab bar lives in the fixed header (stays visible while the rows scroll); it is
  // hidden while scanning every tab at once.
  if (!globalView) {
    const bar = document.createElement("div");
    bar.className = "expert-tabs";
    EXPERT_TABS.forEach((tab, i) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "expert-tab" + (i === expertTab ? " active" : "");
      b.textContent = t(tab.title);
      b.addEventListener("click", () => { expertTab = i; box.scrollTop = 0; renderExpert(); });
      bar.appendChild(b);
    });
    tabBar.appendChild(bar);
    // The bar is rebuilt on every render: put it back where it was, then bring the
    // chosen tab fully into view.
    bar.scrollLeft = keepTabScroll;
    showActiveTab(bar);
    syncEdgeFade(bar);
    bar.addEventListener("scroll", () => syncEdgeFade(bar), { passive: true });
    if (typeof ResizeObserver === "function") {
      tabBarObserver = new ResizeObserver(() => { showActiveTab(bar); syncEdgeFade(bar); });
      tabBarObserver.observe(bar);
    }
  }
  const tabs = globalView ? EXPERT_TABS : [EXPERT_TABS[expertTab]];
  const collapsed = expertCollapsedSet();
  let shown = 0;
  for (const tab of tabs) {
    for (const group of tab.groups) {
      const groupHit = searching && i18nHit(group.title, q);
      // A row whose gating feature is off is dropped in every mode, search included:
      // a setting the user cannot act on has no business being on screen. "Only
      // changed" stacks on top: a row also needs to differ from its default.
      const keep = (kind) => (group[kind] || []).filter((k) => {
        if (!gateOpen(EXPERT_DEFS[kind]?.[k]?.gate)) return false;
        if (expertOnlyChanged && !isRowChanged(kind, k)) return false;
        return !searching || groupHit || settingHit(k, EXPERT_LABEL_KEYS[kind](k), q);
      });
      const flags = groupFlags(group)
        .filter((f) => gateOpen(f.gate) && (!expertOnlyChanged || isRowChanged("flags", f.key)))
        .filter((f) => !searching || groupHit || settingHit(f.key, flagLabelKey(f.key), q))
        .map((f) => f.key);
      const values = keep("values");
      const selects = keep("selects");
      const dropdowns = keep("dropdowns");
      const paths = keep("paths");
      // Neither has a single "default" to diff against (a palette colour would need
      // its own per-swatch compare; an action is a button, not a value): "only
      // changed" drops both rather than showing them unconditionally.
      const palette = !expertOnlyChanged && group.palette && (!searching || groupHit || i18nHit("expGroupColors", q));
      // Same reasoning as palette above: a hand-maintained name/value list has
      // no single "default" to diff against, and it means nothing while the
      // flag it sits under is off, so it never shows on its own.
      const varFixedList = !expertOnlyChanged && group.varFixedList && flag("varFixed") &&
        (!searching || groupHit || i18nHit("expGroupVarFixed", q));
      // Bespoke, same reasoning as palette above: one hex string per key, no
      // single "default" the generic isRowChanged diff machinery could reuse.
      const colors = expertOnlyChanged ? [] : (group.colors || [])
        .filter((k) => gateOpen(EXPERT_COLORS[k]?.gate) && (!searching || groupHit || settingHit(k, EXPERT_COLORS[k]?.label, q)));
      const actions = expertOnlyChanged ? [] : (group.actions || []).filter((a) => gateOpen(a.gate) && (!searching || groupHit || i18nHit(a.label, q)));
      if (globalView && !flags.length && !values.length && !selects.length && !dropdowns.length && !paths.length && !palette && !varFixedList && !colors.length && !actions.length) continue;
      shown++;
      // Categories collapse on click (persisted); scanning every tab always shows them.
      const isCollapsed = !globalView && collapsed.has(group.title);
      const sec = document.createElement("div");
      sec.className = "expert-group" + (isCollapsed ? " collapsed" : "");
      const head = document.createElement("div");
      head.className = "expert-group-title";
      const titleSpan = document.createElement("span");
      titleSpan.textContent = t(group.title);
      head.appendChild(titleSpan);
      if (!globalView) {
        head.classList.add("collapsible");
        head.addEventListener("click", () => toggleExpertGroup(group.title, sec));
      }
      // Per-category reset: only shown when something in the group differs, using the same
      // isRowChanged() diff as the per-row reset. Data & system (expGroupData) mixes ordinary
      // settings with destructive one-click data actions (emergency purge, delete all data), so a
      // bulk-reset button there could look like another destructive action and never appears.
      const right = document.createElement("span");
      right.className = "expert-group-title-right";
      const changedHere = changedKeysInGroup(group);
      const changedCount = group.title === "expGroupData" ? 0 : changedHere.flags.length + changedHere.values.length +
        changedHere.selects.length + changedHere.dropdowns.length + changedHere.paths.length +
        (changedHere.palette ? 1 : 0);
      if (changedCount) {
        const resetBtn = document.createElement("button");
        resetBtn.type = "button";
        resetBtn.className = "expert-group-reset";
        resetBtn.title = t("expertResetGroupBtn");
        resetBtn.setAttribute("aria-label", t("expertResetGroupBtn"));
        putIcon(resetBtn, ICON_UNDO);
        resetBtn.addEventListener("click", (e) => {
          e.stopPropagation(); // must not also toggle the collapse
          resetExpertGroup(group, changedHere, changedCount);
        });
        right.appendChild(resetBtn);
      }
      head.appendChild(right);
      sec.appendChild(head);
      // State first, settings after: and nothing at all while the state does not
      // hold, so a healthy install sees no placeholder where a notice would go.
      for (const id of group.notices || []) {
        if (!EXPERT_NOTICES[id]?.when()) continue;
        sec.appendChild(noticeRow(id));
        const actionBtn = noticeActionRow(id);
        if (actionBtn) sec.appendChild(actionBtn);
      }
      const actionById = new Map(actions.map((a) => [a.id, a]));
      if (searching) {
        // Flat list while searching (grouping a match under a non-matching toggle
        // would hide it); adjacency only matters for the calm, browsable view.
        for (const key of flags) sec.appendChild(flagRow(key));
        for (const key of values) sec.appendChild(valueRow(key));
        for (const key of selects) sec.appendChild(selectRow(key));
        for (const key of dropdowns) sec.appendChild(dropdownRow(key));
        for (const key of paths) sec.appendChild(pathRow(key));
        for (const a of actions) sec.appendChild(actionRow(a));
      } else {
        // Every kind of row obeys the same contract: a setting whose gate names a toggle in this group
        // renders right beneath that toggle and vanishes with it, nested as deep as the gates go.
        const shownFlags = new Set(flags);
        const ownerOf = (kind, k) => {
          const g = kind === "flags" ? FLAG_GATES[k]
            : kind === "actions" ? actionById.get(k)?.gate
            : kind === "colors" ? EXPERT_COLORS[k]?.gate
            : kind === "varFixedList" ? "varFixed"
            : EXPERT_DEFS[kind]?.[k]?.gate;
          return g && g[0] !== "!" && g !== k && shownFlags.has(g) ? g : null;
        };
        const byFlag = new Map();
        const roots = [];
        const lists = { flags, values, selects, dropdowns, paths, actions: actions.map((a) => a.id), colors, varFixedList: varFixedList ? ["varFixedList"] : [] };
        // Flags first, so a parent always exists before anything hangs off it.
        for (const kind of ["flags", "values", "selects", "dropdowns", "paths", "actions", "colors", "varFixedList"]) {
          for (const k of lists[kind]) {
            const n = { kind, k, kids: [] };
            if (kind === "flags") byFlag.set(k, n);
            const owner = ownerOf(kind, k);
            (owner && byFlag.has(owner) ? byFlag.get(owner).kids : roots).push(n);
          }
        }
        emitExpertRows(roots, sec, actionById, 0);
      }
      if (palette) sec.appendChild(paletteRow());
      // Browsing mode already placed the list and the colours under their
      // switch; only the flat search list appends them here.
      if (searching) {
        if (varFixedList) sec.appendChild(varFixedListRow());
        for (const key of colors) sec.appendChild(colorRow(key));
      }
      box.appendChild(sec);
    }
  }
  // Hits outside the expert menu: normal settings, the toolbar, the library
  // dialog and the backup manager. Search only: "only changed" has no
  // meaning for a directory entry, there is nothing here to diff.
  let foreignShown = 0;
  if (searching) {
    const hits = SETTINGS_DIRECTORY.filter((e) => settingsDirectoryHit(e, q));
    if (hits.length) {
      foreignShown = hits.length;
      const sec = document.createElement("div");
      sec.className = "expert-group expert-foreign-group";
      const head = document.createElement("div");
      head.className = "expert-group-title";
      const titleSpan = document.createElement("span");
      titleSpan.textContent = t("expertForeignHits");
      head.appendChild(titleSpan);
      sec.appendChild(head);
      for (const entry of hits) sec.appendChild(foreignHitRow(entry));
      box.appendChild(sec);
    }
  }
  if (globalView && !shown && !foreignShown) {
    const empty = document.createElement("div");
    empty.className = "hint";
    empty.textContent = t(!searching && expertOnlyChanged ? "expertNothingChanged" : searching ? "expertSearchNoHits" : "libraryNoResults");
    box.appendChild(empty);
  }
  box.appendChild(expertEndEl);
  box.scrollTop = keepScroll;
}

// Which of this group's rows currently differ from their default, using the same isRowChanged()
// diff as renderExpert()'s keep(), standalone so the reset button can decide whether to show
// and what to touch. Deliberately not gated on gateOpen(): a row behind a switched-off toggle
// can still hold a changed value, and a group reset must clear that too.
export function changedKeysInGroup(group) {
  const out = { flags: [], values: [], selects: [], dropdowns: [], paths: [], palette: false };
  for (const f of groupFlags(group)) {
    if (isRowChanged("flags", f.key)) out.flags.push(f.key);
  }
  for (const kind of ["values", "selects", "dropdowns", "paths"]) {
    for (const k of group[kind] || []) {
      if (isRowChanged(kind, k)) out[kind].push(k);
    }
  }
  // Closes the one category the per-kind loop above cannot see: the colour
  // palette has no single key, just PALETTE_LEN live hues to diff against defaults.
  if (group.palette) out.palette = COLORS.slice(1).some((c, i) => c !== DEFAULT_COLORS[i + 1]);
  return out;
}

// Reset only what this category owns, through the same per-key
// set_ui_flag/set_ui_value/set_ui_text commands a manual edit uses: the backend's side effects
// (screensaver exclusion, tray, clipboard watcher, ...) are keyed on the setting touched, so
// resetting one category cannot drag in another's.
export async function resetExpertGroup(group, changed, count) {
  const ok = await confirmDialog({
    title: t(group.title),
    message: t("expertResetGroupConfirm").replace("{n}", String(count)),
    confirmLabel: t("expertReset"),
  });
  if (!ok) return;
  for (const key of changed.flags.filter((k) => !EXPERT_BULK_NEVER.includes(k))) {
    const enabled = shipsOn(key);
    settings.ui_flags = { ...(settings.ui_flags || {}), [key]: enabled };
    try { await invoke("set_ui_flag", { key, enabled }); } catch (err) { errToast(err); }
  }
  for (const key of [...changed.values, ...changed.selects]) {
    const cfg = EXPERT_VALUES[key] || EXPERT_SELECTS[key];
    settings.ui_values = { ...(settings.ui_values || {}), [key]: cfg.def };
    try { await invoke("set_ui_value", { key, value: cfg.def }); } catch (err) { errToast(err); }
  }
  for (const key of changed.dropdowns) {
    const cfg = EXPERT_DROPDOWNS[key];
    if (cfg.def === undefined) continue;
    await cfg.set(cfg.def);
  }
  for (const key of changed.paths) EXPERT_PATHS[key].onReset?.();
  if (changed.palette) await savePalette(null); // does its own remap + refresh
  // Batched once for the whole category instead of per key: same reasoning as
  // onResetExpert()'s own full-reset refresh below.
  applyFlags();
  applyValues();
  renderViews();
  await renderGrid(true);
  fitAllTiles();
  toast(t("expertResetGroupDone"));
  renderExpert();
}

export async function onResetExpert() {
  // The backend wipes every stored flag, demo mode included. A running demo
  // session must stay one (leaving it is the switch's own confirmed restart), so
  // its marker is written back straight after.
  const keep = EXPERT_RESET_KEEP.filter((k) => flag(k));
  settings.ui_flags = Object.fromEntries(keep.map((k) => [k, true]));
  settings.ui_values = {};
  settings.ui_texts = {};
  try {
    await invoke("reset_expert");
    for (const key of keep) await invoke("set_ui_flag", { key, enabled: true });
  } catch (err) {
    errToast(err);
  }
  loadPalette();
  refreshColorUI();
  applyFlags();
  renderExpert();
  // ui_flags also holds NORMAL settings (auto-paste, tooltips, screenshot saving,
  // favourites view). Wiping it resets those too, so the settings page has to be
  // redrawn or its checkboxes keep showing the old, no longer stored state.
  syncSettingsControls();
  refreshFavViewUi();
  renderViews();
  await renderGrid(true);
  toast(t("expertResetDone"));
}

// Switch between the main settings page and the expert sub-page.
export function showExpertPage(on) {
  $("settings-main").classList.toggle("hidden", on);
  $("settings-expert").classList.toggle("hidden", !on);
  $("expert-back").classList.toggle("hidden", !on);
  // Expert rows are single-line; keep the panel ~2 columns wide instead of the
  // wider 3-across settings width.
  settingsEl.classList.toggle("expert-active", on);
  $("settings-title").textContent = on ? t("expertMenu") : t("settings");
  if (on) {
    syncStatusAsked = false; // the sync readout is live: ask again on every open
    renderExpert();
  }
}

export function bindExpertMenu() {
  $("expert-open").addEventListener("click", () => {
    expertQuery = "";
    $("expert-search").value = "";
    expertOnlyChanged = false;
    $("expert-only-changed").checked = false;
    syncOnlyChangedChip();
    showExpertPage(true);
  });
  $("expert-back").addEventListener("click", () => showExpertPage(false));
  const renderExpertDebounced = debounced(renderExpert);
  $("expert-search").addEventListener("input", (e) => { expertQuery = e.target.value; renderExpertDebounced(); });
  $("expert-only-changed").addEventListener("change", (e) => {
    expertOnlyChanged = e.target.checked;
    syncOnlyChangedChip();
    renderExpert();
  });
  // Two-step confirm: first click arms it red, second within 3s resets.
  $("expert-reset").addEventListener("click", (e) => {
    const btn = e.currentTarget;
    if (!armButton(btn, t("expertResetConfirm"))) {
      clearTimeout(expertResetTimer);
      expertResetTimer = setTimeout(() => disarmButton(btn, t("expertReset")), DISARM_MS);
      return;
    }
    clearTimeout(expertResetTimer);
    disarmButton(btn, t("expertReset"));
    onResetExpert();
  });

  $("banner-more").addEventListener("click", () => { bannerFront += 1; syncBannerStack(); });

  attachSelectPicker($("video-cleanup-age"));

  // ---------- ffmpeg ----------
  // Fetched on demand only: never on app start, never in the background. Discovery (next to the
  // exe, data folder, PATH) runs on the Rust side and touches no network; this only runs when
  // the user clicks the expert menu's action. Any video feature that shells out to ffmpeg should
  // call the same three commands (ffmpeg_status / download_ffmpeg / cancel_ffmpeg_download)
  // where it first finds ffmpeg missing.
  const ffmpegRun = (btn) => runFfmpegDownload(btn, {
    t,
    onDone: () => {
      ffmpegAvailable = true;
      toast(t("ffmpegDownloadDone"));
      renderExpert();
    },
    onError: errToast,
  });
  EXPERT_ACTIONS["ffmpeg-download"] = async (btn) => {
    if (ffmpegDownloadActive()) {
      ffmpegRun(btn); // a click on the running download cancels it
      return;
    }
    const found = await invoke("ffmpeg_status").catch(() => null);
    ffmpegAvailable = !!found;
    if (found) {
      toast(t("ffmpegFoundMsg").replace("{path}", () => found));
      renderExpert();
      return;
    }
    const ok = await confirmDialog({
      title: t("ffmpegOfferTitle"),
      message: t("ffmpegOfferMsg"),
      confirmLabel: t("ffmpegOfferConfirm"),
      cancelLabel: t("cancel"),
      danger: false,
    });
    if (!ok) return;
    await ffmpegRun(btn);
  };
}
