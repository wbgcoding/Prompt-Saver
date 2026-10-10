import { COOLDOWN_MS_DEFAULT, AUTO_PASTE_DELAY_MS_DEFAULT, DBL_CLICK_MS_DEFAULT, COPY_SOUND_VOLUME_DEFAULT } from "./copyDefaults.js";

// ---- Expert feature flags & values ----
// Every feature ships enabled; the expert menu lets the user switch single
// pieces off. A missing flag means enabled, so old configs keep everything on.
export const FLAG_LABELS = {
  fileAttach: "flagFileAttach",
  screenshot: "flagScreenshot",
  snipEditor: "flagSnipEditor",
  snipEditSnap: "flagSnipEditSnap",
  multiView: "flagMultiView",
  floating: "flagFloating",
  barToggles: "flagBarToggles",
  barToggleBoth: "flagBarToggleBoth",
  tileMenu: "flagTileMenu",
  tooltipsEnabled: "showTooltips", // the Settings switch; the master for every tooltip
  typeBadges: "flagTypeBadges",
  tileHotkeyBadge: "flagTileHotkeyBadge",
  tileFavBadge: "flagTileFavBadge",
  captions: "flagCaptions",
  videoControls: "flagVideoControls",
  videoMuted: "flagVideoMuted",
  // Duration badge + resolution/size tooltip line in the library and copy
  // history -- the main grid tile never reads either field.
  mediaMeta: "flagMediaMeta",
  mediaMetaBackfill: "flagMediaMetaBackfill",
  animations: "flagAnimations",
  pasteMedia: "flagPasteMedia",
  dragDrop: "flagDragDrop",
  promptVars: "flagPromptVars",
  copyHistory: "flagCopyHistory",
  historyTimestamps: "flagHistoryTimestamps",
  historyGrouped: "flagHistoryGrouped",
  librarySearch: "flagLibrarySearch",
  searchAutofocus: "flagSearchAutofocus",
  // A search overlay reaching across prompts, copy history, attachments,
  // views, settings and help all at once -- separate from librarySearch,
  // which only ever covers the prompt grid.
  globalSearch: "flagGlobalSearch",
  searchSuggest: "flagSearchSuggest",
  savedSearches: "flagSavedSearches",
  libraryTiles: "flagLibraryTiles",
  closeAfterCopy: "flagCloseAfterCopy",
  imagePreview: "flagImagePreview",
  libraryVideoPreview: "flagLibraryVideoPreview",
  confirmDiscard: "flagConfirmDiscard",
  showLogo: "flagShowLogo",
  showTitle: "flagShowTitle",
  showLibrary: "library",   // top-bar library button visibility (toolbar menu)
  chainPrompts: "flagChainPrompts",
  // Windows taskbar icon progress during export, conversion, backup, import
  // and the ffmpeg download; and the shared "about how long is left" estimate
  // shown on every progress bar in the app.
  taskbarProgress: "flagTaskbarProgress",
  importExport: "flagImportExport",
  tileReorder: "flagTileReorder",
  dragToView: "flagDragToView",
  libraryTypeFilter: "flagLibraryTypeFilter",
  confirmClearHistory: "flagConfirmClearHistory",
  chainLock: "flagChainLock",
  chainQuickStart: "flagChainQuickStart",
  favorites: "flagFavorites",
  keyboardNav: "flagKeyboardNav",
  varDefaults: "flagVarDefaults",
  favViewReorder: "flagFavViewReorder",
  tileShadow: "flagTileShadow",
  autoInk: "expAutoInk",
  inkHalo: "flagInkHalo",
  tilePressScale: "flagTilePressScale",
  copyFlash: "flagCopyFlash",
  cleanupFiles: "flagCleanupFiles",
  // A text prompt can hold a list of variants instead of one text; each
  // copy serves the next entry (in order, or shuffled without repeats). Default
  // on since 33; shipped default off at 32's original release.
  rotatingPrompts: "flagRotatingPrompts",
  // Sub-toggle of rotatingPrompts, itself also default on.
  rotateBadge: "flagRotateBadge",
  // Sub-toggle of rotatingPrompts, default on.
  rotateAuto: "flagRotateAuto",
  // New features (default on).
  lengthCounter: "flagLengthCounter",
  smartSort: "flagSmartSort",
  fuzzySearch: "flagFuzzySearch",
  batchOps: "flagBatchOps",
  promptHistory: "flagPromptHistory",
  mdPreview: "flagMdPreview",           // rendered Markdown preview in the editor
  promptNotes: "flagPromptNotes",               // private notes row in the editor
  dupFinder: "flagDupFinder",
  // A background check after every save, naming the closest existing prompt
  // once it clears similarThreshold (the same value the duplicate finder
  // above uses) -- never a dialog, never holds up the save itself.
  similarHint: "flagSimilarHint",
  dupImportCheck: "flagDupImportCheck",
  autoBackup: "flagAutoBackup",
  autoPaste: "flagAutoPaste",           // a normal setting now (default on)
  dynVars: "flagDynVars",               // self-filling placeholder names
  choiceVars: "flagChoiceVars",         // {?name|a,b} dropdown variables
  choiceRemember: "flagChoiceRemember", // reopen a choice on its last pick
  codeHighlight: "flagCodeHighlight",   // syntax colours in the preview's code blocks
  codeAutoDetect: "flagCodeAutoDetect", // render an unfenced code prompt as code
  notesInTooltip: "flagNotesInTooltip", // show the private note in the tile tooltip
  promptHotkeys: "flagPromptHotkeys",   // per-prompt global hotkeys
  // Sub-toggle of promptHotkeys, default on -- moved from
  // OPT_FLAG_LABELS; no CSS class keys on this flag, so the move is risk-free.
  promptHotkeyPaste: "flagPromptHotkeyPaste",
  // A custom, rounded/themed tray context menu instead of the native Win32
  // popup. Off falls back to the plain OS menu -- the escape hatch if the
  // custom window ever misbehaves on some setup.
  trayCustomMenu: "flagTrayCustomMenu",
  trayRecent: "flagTrayRecent",         // "recently used" section in the tray menu
  colorPicker: "flagColorPicker",       // pipette; pick a screen colour to the clipboard
  // Windows Media.Ocr text recognition (screenshot / image file -> editable text).
  // Ships ON; the button hides itself when Windows has no recognizer at all.
  ocr: "flagOcr",
  customThemes: "flagCustomThemes",     // theme editor + custom entries in the theme list
  folders: "flagFolders",               // grid folders and floating-button folders, one switch
  tags: "flagTags",                     // tags + library tag filter
  trayFavs: "flagTrayFavs",             // favourites section in the tray menu
  // Filter row at the top of the custom tray menu, searching every prompt
  // rather than only the shown favourites/recents.
  traySearch: "flagTraySearch",
  // Split from the old single autoBackupBeforeWipe so either case can be
  // switched off on its own. Both ship default on.
  backupBeforeReset: "flagBackupBeforeReset",
  backupBeforeDelete: "flagBackupBeforeDelete",
  // Sub-toggles of ocr (OPT_FLAG_LABELS, default off), themselves default on.
  ocrAutoCopy: "flagOcrAutoCopy",
  ocrJoinLines: "flagOcrJoinLines",
  // Auto-opens the five-step first-run tour once, right after the example
  // prompts are seeded. Off only stops the AUTOMATIC open: the Help-menu entry
  // that reopens the tour manually stays available either way.
  firstRunTour: "flagFirstRunTour",
  // Voice dictation (Windows.Media.SpeechRecognition). Ships ON, like ocr; the
  // button hides itself when Windows reports no speech engine at all.
  voice: "flagVoice",
  voiceInterim: "flagVoiceInterim",
  voicePunct: "flagVoicePunct",
  voiceAutoCaps: "flagVoiceAutoCaps",
  voiceAutoStop: "flagVoiceAutoStop",
  // Screen recording. Ships ON, like ocr/voice.
  screenRecord: "flagScreenRecord",
  // Sub-toggle of screenRecord: whether the mouse pointer appears in the
  // recording. Ships ON.
  recCursor: "flagRecCursor",
  // Sub-toggle of screenRecord: system audio (WASAPI loopback) into the
  // recording's audio track. Ships ON.
  recAudioSystem: "flagRecAudioSystem",
  // Sub-toggle of screenRecord, microphone audio into its own track --
  // moved here from OPT_FLAG_LABELS, its shipped default flipping from off to
  // on (into a sidecar track only, muxed in audibly later from the video
  // editor -- see AudioConfig.mic in recording.rs). A stored user value from
  // before this change, or an explicit opt-out, still wins.
  recAudioMic: "flagRecAudioMic",
  // A running recording would otherwise have only the bar itself as a way
  // to stop or pause it. All three ship ON.
  // Sub-toggle of screenRecord: the one-time reactive notice offering to
  // switch the recording canvas to "every display" the first time a window
  // recording (still on its "window size" default) actually grows past its
  // starting size and starts losing picture at the edge. Ships ON.
  recCropNotice: "flagRecCropNotice",
  // The control bar's file-size readout, from 500 MB on, plus a
  // highlighted ETA once the recording nears recMaxMb. Ships ON.
  recBarSize: "flagRecBarSize",
  // The control bar's target-switch button: swaps a running window
  // recording's own source without restarting it. Ships ON.
  recTargetSwitch: "flagRecTargetSwitch",
  // Trims the black border the larger-than-window recording area leaves.
  // Ships ON, matching today's unconditional automatic crop exactly.
  recAutoCrop: "flagRecAutoCrop",
  numFieldWheel: "flagNumFieldWheel",
  editStateKeep: "flagEditStateKeep",
  recStopHotkey: "flagRecStopHotkey",
  recPauseHotkey: "flagRecPauseHotkey",
  recTrayEntries: "flagRecTrayEntries",
  // Catches a crashed recording's unplayable leftover before it sits
  // there forever -- ships ON, like the rest of the recorder's own upkeep.
  recSweepBroken: "flagRecSweepBroken",
  // The video editor satellite window, reachable from a recording's
  // result modal and from any video prompt's context menu. Ships ON, like the
  // rest of the media-editing features (snipEditor, ocr, voice).
  videoEditor: "flagVideoEditor",
  // The screenshot editor has had edge/center snapping for a while
  // (snipEditSnap); this is the same behaviour for the video editor's crop box
  // and overlays, kept as its own switch since the two editors want it
  // independently.
  videoEditSnap: "flagVideoEditSnap",
  // Only matters once clipWatcher (itself off by default) is on -- ships ON so a
  // guessed password never has to be manually turned away in the first place.
  clipSkipSecrets: "flagClipSkipSecrets",
  // Plain-text preview (txt/md/json/csv) for a file attachment, next to the
  // existing image/PDF preview. Ships ON.
  attachTextPreview: "flagAttachTextPreview",
  // A second copy entry that puts both the Markdown text and its rendered
  // form on the clipboard, for apps that read CF_HTML. Ships ON.
  copyFormatted: "flagCopyFormatted",
  // Keeps a ring buffer of the last error messages shown, reachable from the
  // expert menu's "Recent messages" action. Off means the list stays empty and
  // nothing is collected. Ships ON.
  errorLog: "flagErrorLog",
  // The thin frame around a region recording. It is never part of the video
  // (its window is excluded from capture). Ships ON.
  recOutline: "flagRecOutline",
};
export const ALL_FLAG_KEYS = Object.keys(FLAG_LABELS);

// Opt-in flags: OFF by default (the mirror of the flags above, which are ON by
// default). Enabled only when explicitly set true; wired via a `flag-<key>` body
// class that is present exactly when the option is on.
export const OPT_FLAG_LABELS = {
  // Features extras
  // Favorites view: the header star that opens a live grid of favorites. Ships OFF,
  // like the old settings switch it replaced.
  favViewButton: "flagFavViewButton",
  // Statistics: counts tool use only once switched on (the backend has always
  // read it that way). Only ever rendered in the backups dialog.
  usageStats: "statsEnable",
  // Save every screenshot into the Screenshots folder: the Settings switch and
  // the snip dialog switch are the two places it is shown.
  screenshotSave: "screenshotToFolder",
  gridViewBorder: "flagGridViewBorder",         // colored border around the grid in the view's color
  // header columns x rows fields; the grid size otherwise lives in the view menu
  quickGrid: "flagQuickGrid",
  quickGridHoverOnly: "flagQuickGridHoverOnly", // quick-grid control shows only while the header is hovered
  // Named from the user's side: the window is kept out of screenshots, and this is
  // the switch that lets them through. Ships off, so the protection is the default
  // and whoever misses their screenshots finds the reason by its name.
  allowScreenshot: "flagAllowScreenshot",
  storeFiles: "flagStoreFiles",
  gridLines: "flagGridLines",
  // Library/search extras
  // Reads text out of an attached picture and adds it to the library search,
  // lowest priority behind the name and the prompt body. Ships off: text
  // recognition costs time and reads a picture the user may not expect read.
  ocrIndex: "flagOcrIndex",
  // Appearance extras
  frostedModals: "flagFrostedModals",
  hideScrollbars: "flagHideScrollbars",
  // Privacy extras
  blurMediaUntilHover: "flagBlurMediaUntilHover",
  // Media extras
  mediaBorder: "flagMediaBorder",
  roundMedia: "flagRoundMedia",
  // New features (default off).
  autoPasteEnter: "flagAutoPasteEnter",
  clipWatcher: "flagClipWatcher",
  backupGfs: "flagBackupGfs",           // period-based retention
  // v2.6.0 additions (all default off, shown with the default-off suffix).
  editorMonospace: "flagEditorMonospace",       // monospace editor textarea
  hideWindowOnCopy: "flagHideWindowOnCopy",     // hide window after copying a prompt
  dedupCopyLog: "flagDedupCopyLog",             // skip identical consecutive copies
  dragOut: "flagDragOut",                       // drag a prompt into another app
  // Swaps the two gestures: off (shipped) = hold the key to drag out, plain drag
  // moves the tile; on = plain drag leaves the app and the key moves the tile.
  dragOutInvert: "flagDragOutInvert",
  // Off = one instance app-wide (a second launch focuses the first, as always). On =
  // the OS-level single-instance guard is skipped entirely at startup, so several
  // windows can run side by side. Read once at launch, before settings normally
  // load: toggling it needs a restart to take effect (flagRow() toasts that).
  allowMultipleInstances: "flagAllowMultipleInstances",
  // Off = today's behaviour: the data folder is written to unconditionally,
  // whatever it holds. On (shipped) = a lock file next to the database refuses
  // writes for as long as another machine's lock on the same (typically synced)
  // folder is still fresh -- see acquire_data_dir_lock/DATA_DIR_READONLY in
  // store.rs.
  dataDirLock: "flagDataDirLock",
  // Swaps the whole window over to a bundled sample library -- for showing the
  // app or taking a screenshot without exposing real data. The real store is
  // never touched; see enable_demo_mode/disable_demo_mode in demo.rs. This flag
  // is never toggled through the normal set_ui_flag path (see flagRow's own
  // demoMode branch below) -- it only ever reads true from the sample
  // settings a demo session itself ships with.
  demoMode: "flagDemoMode",
  // Off keeps the historical "one placement" guarantee (adding elsewhere moves
  // it there); on lets a prompt sit in several views at once, sharing the same data.
  multiViewPrompt: "flagMultiViewPrompt",
  // Prompt-folders-as-Markdown (beta: see the module note above sync_enabled() in
  // md_sync.rs): mirrors every text-only prompt as a .md file in a folder, editable by
  // hand or another tool.
  promptSync: "flagPromptSync",
  // Voice dictation's two default-off options: sending audio to Microsoft's
  // online recognizer needs its own consent, and a global hotkey
  // is a bigger commitment than the in-window button.
  voiceOnlineOk: "flagVoiceOnlineOk",
  voiceHotkey: "flagVoiceHotkey",
  // A short synthesised beep on every successful copy, mainly for the floating
  // button (easy to miss visually) and for screen-reader users.
  copySound: "flagCopySound",
  // Marks a prompt with a reserved tag instead of a real archived state, so every
  // other tag-aware path already handles it. Browsing hides it; searching still
  // finds it, and the library toggle brings it back into view.
  archiveTag: "flagArchiveTag",
  // A maintained list of name/value pairs (expert menu, empty by default) that
  // the fill-in dialog SUGGESTS for a matching placeholder name, one tier below
  // a remembered last input and above the text's own {?name=default}. Kept
  // apart from varDefaults on purpose -- a deliberately maintained value must
  // never be mistaken for whatever happened to be typed in last.
  varFixed: "flagVarFixed",
  // Per-entry encryption. Ships off: an untouched install has nothing to type a
  // vault password for.
  secretPrompts: "flagSecretPrompts",
  // The local remote-access receiver and the promptsaver:// link
  // scheme. Both off by default -- this is the app's one new network surface.
  remoteApi: "flagRemoteApi",
  remoteNetworkFilter: "flagRemoteNetworkFilter",
  // Certificate lifetime (825 days instead of ten years). Flipping it makes a
  // new certificate, so it has its own confirmed command (toggleShortLivedCert).
  remoteTlsShortLived: "remoteTlsShortLived",
  urlScheme: "flagUrlScheme",
  // System-wide key capture for the video editor's "keys" overlay. Ships
  // OFF -- a low-level keyboard hook exists nowhere in this app unless the
  // user turns this on for a specific recording.
  keyOverlay: "flagKeyOverlay",
  // Sub-toggle of keyOverlay: restricts capture to shortcut and navigation
  // keys only (see mod keylog's resolve_verdict), for a recording with
  // confidential input. Ships OFF -- the plain default records every key.
  keyOverlayShortcutsOnly: "flagKeyOverlayShortcutsOnly",
  // The record button's right-click "Record camera" entry and its own window.
  // Ships OFF, like every feature that can turn a camera light on -- see
  // camera.rs for the WebView2 permission handling this switch gates.
  cameraRecord: "flagCameraRecord",
  // Tile videos start playing on their own. Ships OFF.
  videoAutoplay: "flagVideoAutoplay",
  // Tile videos loop. Ships OFF.
  videoLoop: "flagVideoLoop",
};
export const OPT_FLAG_KEYS = Object.keys(OPT_FLAG_LABELS);
export const isOptFlag = (key) => key in OPT_FLAG_LABELS;

export const flagLabelKey = (key) => OPT_FLAG_LABELS[key] || FLAG_LABELS[key];
// A setting may declare `gate: "flagKey"` (or "!flagKey" to invert) so its row hides
// while that feature is off: no dead controls for disabled things. Values, selects,
// dropdowns and paths carry it in their definition; a flag carries it in the group's
// `flags` list, written as { key, gate } instead of a bare key.
export const flagEntry = (f) => (typeof f === "string" ? { key: f } : f);
export const groupFlags = (group) => (group.flags || []).map(flagEntry);

// Rendering ceiling for the copy history window, the same top the old row slider had.
export const JOURNAL_ROWS_MAX = 10000;

// Copy-history slider stops: coarser steps as the value grows (10s -> 100s ->
// 1000s -> 10000s -> 100000s) so the slider stays smooth all the way to 1,000,000.
export const HISTORY_STOPS = (() => {
  const s = [];
  for (let v = 0; v <= 100; v += 10) s.push(v);
  for (let v = 200; v <= 1000; v += 100) s.push(v);
  for (let v = 2000; v <= 10000; v += 1000) s.push(v);
  for (let v = 20000; v <= 100000; v += 10000) s.push(v);
  for (let v = 200000; v <= 1000000; v += 100000) s.push(v);
  return s;
})();

// Numeric value tweaks. `def` is the shipped default; a missing value uses it.
export const EXPERT_VALUES = {
  videoVolume: { label: "valVideoVolume", min: 0, max: 100, step: 5, def: 100, unit: "%", gate: "!videoMuted" },
  bubbleMs: { label: "valBubbleMs", min: 0, max: 6000, step: 50, def: 950, unit: "ms", zeroLabel: "valOff", offAtZero: true },
  // Floating-button opacity (%). Floored at 20% so the pill never vanishes.
  floatOpacity: { label: "valFloatOpacity", min: 20, max: 100, step: 5, def: 100, unit: "%", gate: "floating" },
  // Floating pill appearance -- the text size.
  pillFontSize: { label: "valPillFontSize", min: 14, max: 40, step: 1, def: 26, unit: "px", gate: "floating" },
  // 0 = off (never clears the parked view, only the position keeps living in
  // settings either way). Above 0, a button parked this many hours loses only
  // its view -- the same "hide for now" mechanism -- never the saved entry.
  floatParkCleanupH: { label: "valFloatParkCleanupH", min: 0, max: 168, step: 1, def: 0, unit: "h", zeroLabel: "updateNever", gate: "floating" },
  // Pill folders: how much a pill grows for the merge-target hint, and the open
  // folder popover's own tile size / gap / padding.
  // Custom tile tooltip max width (it wraps + grows taller instead of widening).
  // How long the pointer must rest on an element before its tooltip appears, so a
  // casual sweep across the window shows nothing. 0 shows it at once.
  tooltipDelayMs: { label: "valTooltipDelay", min: 0, max: 2000, step: 50, def: 500, unit: "ms", zeroLabel: "valOff", offAtZero: true, gate: "tooltipsEnabled" },
  // Appearance scales (percent, 100 = unchanged). Applied as CSS zoom.
  uiScale: { label: "valUiScale", min: 75, max: 300, step: 5, def: 100, unit: "%" },
  iconScale: { label: "valIconScale", min: 50, max: 300, step: 5, def: 100, unit: "%" },
  // Max length for prompt names + captions (raise it for long labels).
  nameMaxLen: { label: "valNameMaxLen", min: 20, max: 1000, step: 10, def: 80, unit: "" },
  // Limit overrides (defaults match the classic 20; backend ceiling is 200).
  maxViews: { label: "valMaxViews", min: 1, max: 200, step: 1, def: 20, unit: "", gate: "multiView" },
  gridMax: { label: "valGridMax", min: 1, max: 200, step: 1, def: 20, unit: "" },
  // Copy-history length (0 = keep none).
  historyMax: { label: "valHistoryMax", def: 100, unit: "", stops: HISTORY_STOPS, gate: "copyHistory" },
  // Max copy-history rows the journal renders at once (UI cap, not storage).
  // Copy-history dialog width.
  // Block re-copying the SAME prompt within this window (0 = off). Never blocks others.
  copyCooldownMs: { label: "valCopyCooldown", min: 0, max: 10000, step: 100, def: COOLDOWN_MS_DEFAULT, unit: "ms" },
  copySoundVolume: { label: "valCopySoundVolume", min: 0, max: 100, step: 5, def: COPY_SOUND_VOLUME_DEFAULT, unit: "%", gate: "copySound" },
  // Favorites grid: cap the column count (0 = auto-size to the count).
  favMaxCols: { label: "valFavMaxCols", min: 0, max: 20, step: 1, def: 0, unit: "", gate: "favViewButton" },
  // Prompt-list (library) column options: how many columns the header toggle offers.
  libraryCols: { label: "valLibraryCols", min: 1, max: 6, step: 1, def: 1, unit: "", gate: "librarySearch" },
  libPreviewLines: { label: "valLibPreviewLines", min: 1, max: 4, step: 1, def: 2, unit: "" },
  // Prompt-list (library) dialog width.
  libraryWidth: { label: "valLibraryWidth", min: 520, max: 1400, step: 20, def: 900, unit: "px" },
  // One entry limit for every search list: the library's suggestions and recent
  // searches, Search everything (hits per group and recent searches), the quick
  // launcher and the tray menu's filter row. It serves several features, so it has no gate.
  searchListMax: { label: "valSearchListMax", min: 3, max: 20, step: 1, def: 8, unit: "" },
  libraryTileSize: { label: "valLibraryTileSize", min: 80, max: 240, step: 10, def: 140, unit: "px", gate: "libraryTiles" },
  // Tile geometry.
  tileRadius: { label: "valTileRadius", min: 0, max: 30, step: 1, def: 10, unit: "px" },
  tileBorderWidth: { label: "valTileBorderWidth", min: 0, max: 6, step: 1, def: 1, unit: "px" },
  // How far a tile lifts when the pointer is on it; 0 is off.
  hoverLift: { label: "valHoverLift", min: 0, max: 12, step: 1, def: 0, unit: "px", zeroLabel: "valOff", offAtZero: true },
  // How strongly everything but the hovered item fades (tiles, media, the top bar
  // together); 0 is off.
  hoverDim: { label: "valHoverDim", min: 0, max: 90, step: 5, def: 0, unit: "%", zeroLabel: "valOff", offAtZero: true },
  // A delay before the capture (lets a menu stay open in the shot).
  snipDelayMs: { label: "valSnipDelayMs", min: 0, max: 5000, step: 100, def: 0, unit: "ms", gate: "screenshot" },
  // The actual saved/copied/exported JPEG quality -- every save/copy path, edited
  // or not, shares this one value (see SNIP_SAVE_Q in snip.rs).
  snipSaveQuality: { label: "valSnipSaveQuality", min: 50, max: 100, step: 5, def: 90, unit: "%", gate: "screenshot" },
  // A file past this size is rejected before it loads when opening one for
  // editing -- open_file_in_editor reads the very same key.
  // 0 means forever, same convention as trashRetentionDays.
  editStateKeepDays: { label: "valEditStateKeepDays", min: 0, max: 365, step: 1, def: 30, unit: "", gate: "editStateKeep" },
  // Delay between picking a recording target and the capture actually starting -- 0 disables it.
  recCountdownS: { label: "valRecCountdownS", min: 0, max: 10, step: 1, def: 3, unit: "s", gate: "screenRecord" },
  // The encoder always recorded at a fixed 30fps regardless of the source
  // monitor's real refresh rate until this was added -- a 60/144Hz screen can
  // now be captured at its own rate instead of being silently capped.
  recFps: { label: "valRecFps", min: 10, max: 60, step: 5, def: 30, unit: "fps", gate: "screenRecord" },
  // Native H.264 bitrate. Has no effect once recQuality is "lossless" -- that
  // step is produced by a separate ffmpeg CRF-0 re-encode after the recording
  // stops, not by this slider (see the recQuality/recBitrateKbps tooltips).
  recBitrateKbps: { label: "valRecBitrateKbps", min: 1000, max: 50000, step: 500, def: 12000, unit: "kbps", gate: "screenRecord" },
  cameraFramerate: { label: "valCameraFramerate", min: 10, max: 60, step: 5, def: 30, unit: "fps", gate: "cameraRecord" },
  recMaxMin: { label: "valRecMaxMin", min: 0, max: 600, step: 5, def: 0, unit: "min", zeroLabel: "valUnlimited", gate: "screenRecord" },
  recMaxMb: { label: "valRecMaxMb", min: 0, max: 100000, step: 500, def: 0, unit: "MB", zeroLabel: "valUnlimited", gate: "screenRecord" },
  // How many recordings the "export selected" batch dialog encodes at once --
  // more overlaps ffmpeg calls and finishes sooner, at the cost of more CPU
  // and disk I/O in parallel.
  // How young a moov-less (broken) recording has to be to still get a pass
  // (the post-capture menu's stop might just still be finalising) and how often
  // the background sweep for it (and for merely-undecided recordings) runs.
  // How long a discarded recording sits in its "verworfen" trash before the
  // background sweep removes it for good. Shown under the recording switch
  // only; it is not tied to recSweepBroken because the backend runs this
  // sweep whether or not the broken-recording sweep is on.
  recDiscardKeepH: { label: "valRecDiscardKeepH", min: 0, max: 720, step: 1, def: 24, unit: "h", zeroLabel: "retentionForever", gate: "screenRecord" },
  voiceAutoStopSec: { label: "valVoiceAutoStopSec", min: 1, max: 60, step: 1, def: 10, unit: "s", gate: "voiceAutoStop" },
  voiceMaxMin: { label: "valVoiceMaxMin", min: 1, max: 60, step: 1, def: 10, unit: "min", gate: "voice" },
  // Frosted-glass dialog background blur (paired with the frostedModals toggle).
  frostedBlur: { label: "valFrostedBlur", min: 0, max: 20, step: 1, def: 4, unit: "px", gate: "frostedModals" },
  // Prompt version history: versions kept per prompt.
  historyPerPrompt: { label: "valHistoryPerPrompt", min: 5, max: 100, step: 5, def: 20, unit: "", gate: "promptHistory" },
  // Usage statistics: how many rows the top-N lists show.
  statsTopN: { label: "valStatsTopN", min: 5, max: 100, step: 1, def: 10, unit: "", gate: "usageStats" },
  // Duplicate finder: similarity threshold.
  // Shared by the duplicate finder above and the save-time similar-prompt hint --
  // one number, one notion of "similar", per the spec. Gated on similarHint
  // rather than dupFinder since the hint is the more commonly-hit path (it
  // fires on every save); no OR-gate exists for a row two flags both use.
  similarThreshold: { label: "valSimilarThreshold", min: 70, max: 99, step: 1, def: 85, unit: "%", gate: "similarHint" },
  // Import from a folder: how many subfolder levels to scan (1 = the picked
  // folder only) and how many files to stop at.
  importFolderDepth: { label: "valImportFolderDepth", min: 1, max: 5, step: 1, def: 3, unit: "", gate: "importExport" },
  importFolderMax: { label: "valImportFolderMax", min: 10, max: 2000, step: 10, def: 500, unit: "", gate: "importExport" },
  // How often the open editor writes its autosave draft.
  editorDraftSecs: { label: "valEditorDraftSecs", min: 0, max: 30, step: 1, def: 5, unit: "s", zeroLabel: "valOff", offAtZero: true },
  // Matches append_crash_log's own pre-existing hardcoded rotation cap (256 KB).
  errorLogKeepKb: { label: "valErrorLogKeepKb", min: 64, max: 4096, step: 64, def: 256, unit: "KB", display: "mb", gate: "errorLog" },
  // Automatic backups: how many rotating snapshots to keep.
  backupKeep: { label: "valBackupKeep", min: 1, max: 50, step: 1, def: 10, unit: "", gate: "autoBackup" },
  // How long the auto-backup job waits after launch before its first check --
  // long enough for the window and store to settle, short enough that a
  // backup due at launch does not sit for a full BACKUP_CHECK_SECS tick.
  backupStartDelayS: { label: "valBackupStartDelay", min: 0, max: 300, step: 5, def: 20, unit: "s", gate: "autoBackup" },
  // GFS retention tiers (kept per distinct day / week / month / year).
  backupDaily: { label: "valBackupDaily", min: 0, max: 30, step: 1, def: 3, unit: "", gate: "backupGfs" },
  backupWeekly: { label: "valBackupWeekly", min: 0, max: 52, step: 1, def: 4, unit: "", gate: "backupGfs" },
  backupMonthly: { label: "valBackupMonthly", min: 0, max: 60, step: 1, def: 6, unit: "", gate: "backupGfs" },
  backupYearly: { label: "valBackupYearly", min: 0, max: 20, step: 1, def: 1, unit: "", gate: "backupGfs" },
  // Auto-paste: SetForeground→Ctrl+V delay + double-click window.
  autoPasteDelayMs: { label: "valAutoPasteDelay", min: 30, max: 300, step: 10, def: AUTO_PASTE_DELAY_MS_DEFAULT, unit: "ms", gate: "autoPaste" },
  dblClickMs: { label: "valDblClick", min: 200, max: 600, step: 20, def: DBL_CLICK_MS_DEFAULT, unit: "ms", gate: "autoPaste" },
  // Clipboard watcher: min length + inbox cap.
  clipMinChars: { label: "valClipMin", min: 1, max: 100, step: 1, def: 3, unit: "", gate: "clipWatcher" },
  clipInboxMax: { label: "valClipMax", min: 10, max: 200, step: 10, def: 50, unit: "", gate: "clipWatcher" },
  // Confidential-entry vault: how long an unlocked session stays unlocked. 0 = until quit.
  secretTimeoutMin: { label: "valSecretTimeoutMin", min: 0, max: 240, step: 5, def: 15, unit: "min", zeroLabel: "secretTimeoutNever", gate: "secretPrompts" },
  // Keyboard grid focus fades this long after the window loses focus.
  // Tag chip text size (editor, library filter row, manager).
  // Column count for the grid tag-filter popover.
  // How many custom themes may be stored (keeps the settings blob bounded).
  customThemeMax: { label: "valCustomThemeMax", min: 1, max: 20, step: 1, def: 8, unit: "", gate: "customThemes" },
  // How many views a group popover lists before it starts scrolling.
  // Relative-luminance cutoff (/100) above which readableInk() picks black text.
  inkThreshold: { label: "valInkThreshold", min: 20, max: 90, step: 5, def: 25, unit: "%", gate: "autoInk" },
  // How dark the halo behind white text gets, in percent.
  inkHaloStrength: { label: "valInkHaloStrength", min: 10, max: 100, step: 5, def: 75, unit: "%", gate: "inkHalo" },
  // How much of a tab's outer edge counts as "drop here to reorder" rather than
  // "drop here to group" -- a wider tab needs a wider zone to stay easy to aim at.
  // How many entries each tray quick-copy section lists.
  trayRecentN: { label: "valTrayRecentN", min: 1, max: 10, step: 1, def: 5, unit: "", gate: "trayRecent" },
  trayFavN: { label: "valTrayFavN", min: 1, max: 10, step: 1, def: 10, unit: "", gate: "trayFavs" },
  // How much of a private note the tile tooltip shows (0 = the whole note).
  notesTipLen: { label: "valNotesTipLen", min: 0, max: 2000, step: 20, def: 300, unit: "", gate: "notesInTooltip" },
  // How far the picker's loupe magnifies the pixels around the cursor.
  loupeZoom: { label: "valLoupeZoom", min: 4, max: 16, step: 1, def: 4, unit: "×", gate: "colorPicker" },
  // Loupe grid size (odd numbers only: the centre cell must be the exact picked pixel).
  // How many recent picks the right-click popover (and the overlay strip) keep. 0 = off.
  pickHistoryN: { label: "valPickHistoryN", min: 0, max: 20, step: 1, def: 5, unit: "", gate: "colorPicker" },
  // Swatch diameter in the right-click colour-history popover.
  // Cap on how many variants one rotating prompt may hold.
  rotateMaxEntries: { label: "valRotateMaxEntries", min: 2, max: 50, step: 1, def: 20, unit: "", gate: "rotatingPrompts" },
  // Auto-advance session tuning: mirrors the backend's auto_cfg() defaults.
  // A remaining-time estimate only shows once a progress bar has moved this
  // far, and only if what's left is still worth naming a number for.
  bgDoneKeepMin: { label: "bgDoneKeepMin", min: 1, max: 120, step: 1, def: 10, unit: "min" },
  // How long the Undo button on a destructive-action toast stays clickable.
  undoToastMs: { label: "valUndoToastMs", min: 0, max: 20000, step: 500, def: 8000, unit: "ms", zeroLabel: "valOff", offAtZero: true },
  // How many recent error messages the ring buffer keeps.
  // The port shows on the persistent badge while the receiver
  // runs -- a companion program reads it from there, not from this slider.
  remotePort: { label: "valRemotePort", min: 1024, max: 65535, step: 1, def: 51823, unit: "", gate: "remoteApi", input: true },
  remoteRateLimit: { label: "valRemoteRateLimit", min: 5, max: 300, step: 5, def: 60, unit: "/min", gate: "remoteApi", topUnlimited: true },
};

// Preset-or-custom numeric settings (dropdown with a free-entry option). The
// free-entry field takes a whole number from SELECT_CUSTOM_MIN to SELECT_CUSTOM_MAX;
// the backend holds a stored value to the same bounds (see tools/gen_ui_ranges.js).
export const SELECT_CUSTOM_MIN = 1;
export const SELECT_CUSTOM_MAX = 3650;
export const EXPERT_SELECTS = {
  historyDays: { label: "valHistoryDays", options: [0, 7, 30, 90, 365], def: 30, unit: "d", zeroLabel: "retentionForever", gate: "copyHistory" },
  // Backup interval (hours): a preset-or-custom select right under backupKeep.
  backupIntervalH: { label: "valBackupInterval", options: [6, 12, 24, 48, 168], def: 24, unit: "h", gate: "autoBackup" },
  // How long a deleted prompt sits in the trash before it is purged automatically.
  // No feature gate: trash is not optional, delete always goes through it.
  trashRetentionDays: { label: "valTrashRetentionDays", options: [0, 7, 30, 90, 365], def: 30, unit: "d", zeroLabel: "retentionForever" },
  // How long a KEPT recording may age before an unreferenced one becomes fair
  // game for the same sweep that already handles undecided/broken files.
  // Off by default (0), same "0 = forever" convention, so nothing changes for
  // anyone who never opens this row -- a referenced recording never qualifies
  // regardless of this value.
  recordingRetentionDays: { label: "valRecordingRetentionDays", options: [0, 7, 30, 90, 365], def: 90, unit: "d", zeroLabel: "retentionForever", gate: "screenRecord" },
  // How often the background thread checks for a new release.
  updateCheckH: { label: "valUpdateCheckH", options: [6, 12, 24, 48, 168], def: 24, unit: "h" },
};

// Date/time width, shared by both style dropdowns. "iso" is the only entry
// that bypasses Intl and always writes the machine format.
export const STYLE_OPTS = [["short", "optStyleShort"], ["medium", "optStyleMedium"], ["long", "optStyleLong"], ["iso", "optStyleIso"]];

// recQuality's own suggested recBitrateKbps default, applied when the step is
// picked (see recQuality's set() below); documented in docs/help.md.
export const REC_QUALITY_BITRATE = { small: 4000, high: 12000 };

// Expert menu organised into tabs. The shape is deliberate: the first tab holds
// FEATURES: a toggle followed by everything that tunes it, so a switched-off
// feature costs exactly one line: while the remaining tabs hold the general
// values that belong to no single feature.
export const EXPERT_TABS = [
  { title: "expTabFeatures", groups: [
    // Everything that puts new content into the app.
    { title: "expGroupInput", flags: ["fileAttach", { key: "storeFiles", gate: "fileAttach" }, { key: "cleanupFiles", gate: "storeFiles" }, { key: "attachTextPreview", gate: "fileAttach" }, "pasteMedia", "dragDrop"], dropdowns: ["attachPreviewSize"] },
    { title: "expGroupViews", flags: ["multiView", { key: "multiViewPrompt", gate: "multiView" }, { key: "gridViewBorder", gate: "multiView" }, "quickGrid", { key: "quickGridHoverOnly", gate: "quickGrid" }], values: ["maxViews", "gridMax"] },
    // Placeholders, from the grammar to where the app asks for them. varFixed
    // (a hand-maintained name/value list, empty by default, only ever
    // SUGGESTED in the fill-in dialog) stays in this same group rather than
    // its own -- its gate parent promptVars lives here, and a gated flag has
    // to sit with its parent.
    { title: "expGroupVars", flags: ["promptVars", { key: "varDefaults", gate: "promptVars" }, { key: "dynVars", gate: "promptVars" }, { key: "choiceVars", gate: "promptVars" }, { key: "choiceRemember", gate: "choiceVars" }, { key: "varFixed", gate: "promptVars" }], dropdowns: ["dateStyle", "timeStyle"], varFixedList: true },
    // The editor: what it shows beside the text box. Preview and highlighter belong together; the
    // highlighter only ever paints inside the preview.
    { title: "expGroupEditor", flags: ["mdPreview", { key: "codeHighlight", gate: "mdPreview" }, { key: "codeAutoDetect", gate: "mdPreview" }, { key: "copyFormatted", gate: "mdPreview" }, "lengthCounter", "promptHistory", "promptNotes", { key: "notesInTooltip", gate: "promptNotes" }, "editorMonospace", "confirmDiscard"], values: ["historyPerPrompt", "notesTipLen", "editorDraftSecs", "nameMaxLen"], dropdowns: ["counterUnits"] },
    // Everything that happens between a click and the clipboard.
    { title: "expGroupCopy", flags: ["promptHotkeys", { key: "promptHotkeyPaste", gate: "promptHotkeys" }, "dragOut", { key: "dragOutInvert", gate: "dragOut" }, { key: "autoPasteEnter", gate: "autoPaste" }, "hideWindowOnCopy", "copyFlash", "copySound"], values: ["copyCooldownMs", "autoPasteDelayMs", "dblClickMs", "bubbleMs", "copySoundVolume"], dropdowns: ["dragOutModifier", "copySizeLevel"] },
    // Copying several different prompts as one text.
    { title: "expGroupChain", flags: ["chainPrompts", { key: "chainLock", gate: "chainPrompts" }, { key: "chainQuickStart", gate: "chainPrompts" }], dropdowns: ["chainSeparator"] },
    // Floating buttons and the two kinds of folder they can form, plus the
    // Markdown mirror of prompt folders (beta, text-only prompts as .md files).
    { title: "expGroupFolders", flags: ["floating", "folders", "promptSync"], values: ["floatOpacity", "pillFontSize", "floatParkCleanupH"], paths: ["syncDir"],
      actions: [{ id: "float-recall", label: "floatRecallAction", gate: "floating" }] },
    // The quick-copy sections of the tray menu, each with its own entry count.
    { title: "expGroupTray", flags: ["trayCustomMenu", "trayRecent", "trayFavs", "traySearch"], values: ["trayRecentN", "trayFavN"] },
    // Auto-advancing through a rotation -- not a capture tool, stays in tab 1.
    { title: "expGroupRotation", flags: ["rotatingPrompts", { key: "rotateBadge", gate: "rotatingPrompts" }, { key: "rotateAuto", gate: "rotatingPrompts" }], values: ["rotateMaxEntries"], dropdowns: ["rotateDefaultMode"] },
    // Voice dictation -- not a capture tool either (it puts text into the composer,
    // the same way typing does), stays in tab 1.
    { title: "expGroupVoice", flags: ["voice", { key: "voiceInterim", gate: "voice" }, { key: "voicePunct", gate: "voice" }, { key: "voiceAutoCaps", gate: "voicePunct" }, { key: "voiceAutoStop", gate: "voice" }, { key: "voiceOnlineOk", gate: "voice" }, { key: "voiceHotkey", gate: "voice" }], values: ["voiceAutoStopSec", "voiceMaxMin"], dropdowns: ["voiceLang", "voiceInsert", "voiceHotkeyTarget"],
      actions: [{ id: "voice-langs-open", label: "voiceLangsManage", gate: "voice" }, { id: "voice-hotkey-set", label: "voiceHotkeySet", gate: "voiceHotkey" }] },
  ] },
  { title: "expTabCapture", groups: [
    // Everything below is a capture tool: it reads something off the screen or the
    // microphone rather than putting typed content into the app.
    { title: "expGroupScreenshot", flags: ["screenshot"], values: ["snipSaveQuality", "snipDelayMs"], dropdowns: ["screenshotFormat"], paths: ["screenshotDir"] },
    { title: "expGroupSnipEditor", flags: [{ key: "snipEditor", gate: "screenshot" }, { key: "snipEditSnap", gate: "snipEditor" }], dropdowns: ["snipEditDefaultTool", "snipTextBack"], colors: ["snipDefaultColor"] },
    // Opens #ocr-langs-modal -- same gate as the rest of the feature.
    { title: "expGroupOcr", flags: ["ocr", { key: "ocrAutoCopy", gate: "ocr" }, { key: "ocrJoinLines", gate: "ocr" }], dropdowns: ["ocrLang"],
      actions: [{ id: "ocr-langs-open", label: "ocrLangsManage", gate: "ocr" }] },
    { title: "expGroupPicker", flags: ["colorPicker"], values: ["loupeZoom", "pickHistoryN"], dropdowns: ["pickerFormat"] },
    // The recording hotkeys render their own capture row directly (hotkeyRow())
    // instead of a button that jumps to a settings field -- see actionRow()'s
    // "hotkey" branch.
    { title: "expGroupRecording", flags: ["screenRecord"], values: ["recCountdownS", "recFps", "recBitrateKbps", "recMaxMin", "recMaxMb", "recDiscardKeepH"], selects: ["recordingRetentionDays"], dropdowns: ["recQuality", "recFormat", "recCanvasMode"], paths: ["recordingDir"] },
    { title: "expGroupRecWhile", flags: [{ key: "recCursor", gate: "screenRecord" }, { key: "recAudioSystem", gate: "screenRecord" }, { key: "recAudioMic", gate: "screenRecord" }, { key: "recStopHotkey", gate: "screenRecord" }, { key: "recPauseHotkey", gate: "screenRecord" }, { key: "recTrayEntries", gate: "screenRecord" }, { key: "recCropNotice", gate: "screenRecord" }, { key: "recBarSize", gate: "screenRecord" }, { key: "recTargetSwitch", gate: "screenRecord" }, { key: "recAutoCrop", gate: "screenRecord" }, { key: "recOutline", gate: "screenRecord" }, { key: "keyOverlay", gate: "screenRecord" }, { key: "keyOverlayShortcutsOnly", gate: "keyOverlay" }], dropdowns: ["recMicDevice"], colors: ["recOutlineColor"],
      actions: [{ id: "rec-stop-hotkey-row", type: "hotkey", hotkeyKey: "recStopHotkey", label: "recHotkeyFieldLabel", gate: "recStopHotkey" }, { id: "rec-pause-hotkey-row", type: "hotkey", hotkeyKey: "recPauseHotkey", label: "recHotkeyFieldLabel", gate: "recPauseHotkey" }] },
    { title: "expGroupCamera", flags: ["cameraRecord"], values: ["cameraFramerate"], dropdowns: ["cameraDevice", "cameraResolution"] },
    { title: "expGroupVideoEditor", flags: ["videoEditor", { key: "videoEditSnap", gate: "videoEditor" }], paths: ["videoExportDir"] },
    // What the screenshot editor and the video editor share -- created here so
    // no setting key is ever declared in two groups (expert_menu_check.py).
    { title: "expGroupEditors", flags: ["numFieldWheel", "editStateKeep"], values: ["editStateKeepDays"] },
  ] },
  { title: "expTabTiles", groups: [
    // Split by what a setting actually does: acting on a tile, the hover tooltip,
    // and pure looks. One 26-entry blob made everything hard to find.
    { title: "expGroupTiles", flags: ["tileMenu", "tileReorder", "dragToView", "tilePressScale", "tileHotkeyBadge"], values: ["hoverLift", "hoverDim"] },
    { title: "expGroupTooltips", values: ["tooltipDelayMs"] },
    { title: "expGroupTileLook", flags: ["tileShadow", "typeBadges", "captions", "gridLines", "autoInk", "inkHalo"], values: ["tileRadius", "tileBorderWidth", "inkThreshold", "inkHaloStrength"], dropdowns: ["tileStyle"] },
    { title: "expGroupFavorites", flags: ["favorites", { key: "tileFavBadge", gate: "favorites" }, { key: "favViewReorder", gate: "favViewButton" }], values: ["favMaxCols"] },
    { title: "expGroupMedia", flags: ["videoControls", "videoAutoplay", "videoMuted", "videoLoop", "mediaMeta", "mediaBorder", "roundMedia"], values: ["videoVolume"] },
  ] },
  { title: "expTabAppearance", groups: [
    { title: "expGroupScales", values: ["iconScale"] },
    { title: "expGroupAppearanceDetails", flags: ["animations", "hideScrollbars", "frostedModals", "customThemes"], values: ["frostedBlur", "customThemeMax"], dropdowns: ["toastPos"], palette: true },
  ] },
  { title: "expTabLibrary", groups: [
    { title: "expGroupLibSearch", flags: ["librarySearch", "globalSearch", "searchSuggest", "savedSearches", "fuzzySearch", "libraryTypeFilter", "searchAutofocus", "ocrIndex"], values: ["searchListMax", "libraryCols"],
      actions: [{ id: "ocr-index-backfill", label: "ocrIndexBackfillAction", gate: "ocrIndex" }] },
    { title: "expGroupLibrary", flags: ["smartSort", "batchOps", "imagePreview", "libraryVideoPreview", "libraryTiles"], values: ["libraryWidth", "libPreviewLines", "libraryTileSize"] },
    { title: "expGroupTags", flags: ["tags", "archiveTag"], dropdowns: ["tagFilterMode", "tagSort"] },
    // Everything that records or shows copy history lives with the history view.
    { title: "expGroupJournal", flags: ["copyHistory", "historyTimestamps", "historyGrouped", "dedupCopyLog", "confirmClearHistory", "clipWatcher", { key: "clipSkipSecrets", gate: "clipWatcher" }], values: ["historyMax", "clipMinChars", "clipInboxMax"], selects: ["historyDays"] },
  ] },
  { title: "expTabSystem", groups: [
    // Toolbar button visibility (mirrors the header right-click tool list).
    { title: "expGroupToolbar", flags: [{ key: "barToggleBoth", gate: "barToggles" }, "showLibrary"] },
    { title: "expGroupPrivacy", notices: ["storeUnsealed", "settingsLocked", "settingsWriteFailed"], flags: ["allowScreenshot", "blurMediaUntilHover", "secretPrompts"], values: ["secretTimeoutMin"] },
    { title: "expGroupSystem", notices: ["dataDirReadOnly"], flags: ["keyboardNav", "taskbarProgress", "firstRunTour"], paths: ["dataDir"],
      actions: [{ id: "ffmpeg-download", label: "ffmpegDownloadAction" }, { id: "tour-open", label: "tourOpenAction", gate: "firstRunTour" }] },
    { title: "expGroupData", flags: ["dupFinder", { key: "dupImportCheck", gate: "dupFinder" }, "similarHint", "backupBeforeReset", "backupBeforeDelete", "importExport", "errorLog"], values: ["similarThreshold", "importFolderDepth", "importFolderMax", "undoToastMs"], selects: ["trashRetentionDays"],
      actions: [{ id: "trash-open", label: "trashTitle" }, { id: "settings-export", label: "settingsExportAction" }, { id: "settings-import", label: "settingsImportAction" }, { id: "error-log-open", label: "errorLogAction", gate: "errorLog" }] },
    // The only new network surface in the app, off by default.
    // A link can only open or copy an entry; the receiver can do everything
    // the command line can, gated by its own password and network switch.
    { title: "expGroupRemote", flags: ["remoteApi", { key: "remoteNetworkFilter", gate: "remoteApi" }, { key: "remoteTlsShortLived", gate: "remoteNetworkFilter" }, "urlScheme"], values: ["remotePort", "remoteRateLimit"],
      actions: [{ id: "remote-password-open", label: "remotePasswordAction", gate: "remoteApi" }, { id: "remote-ranges-open", label: "remoteRangesAction", gate: "remoteNetworkFilter" }, { id: "remote-tls-open", label: "remoteTlsAction", tip: "remoteTlsActionTip", gate: "remoteNetworkFilter" }] },
    // Switches and caps for finding a fault, not for everyday use: folded until opened.
    { title: "expGroupDiagnostics", flags: ["allowMultipleInstances", "dataDirLock", "demoMode", "mediaMetaBackfill", { key: "recSweepBroken", gate: "screenRecord" }], values: ["errorLogKeepKb", "bgDoneKeepMin"],
      actions: [{ id: "rec-sweep-now", label: "recSweepNowAction", gate: "recSweepBroken" }] },
  ] },
];

// Sub-toggle -> the feature it belongs to, collected from the group lists above so a
// gate is declared in exactly one place. gateOpen() walks it, which is what makes a
// whole branch (feature -> sub-toggle -> its sliders) vanish together.
export const FLAG_GATES = Object.fromEntries(
  EXPERT_TABS.flatMap((tab) => tab.groups.flatMap(groupFlags))
    .filter((f) => f.gate)
    .map((f) => [f.key, f.gate])
);
// The switch lives in Settings; its gate still applies to the dependent rows.
FLAG_GATES.favViewButton = "favorites";

// Demo mode is a session with its own enable and disable commands and a restart,
// so no bulk action (category reset, reset all) may ever write it.
export const EXPERT_BULK_NEVER = ["demoMode", "remoteTlsShortLived"];
// What an expert reset writes back: the backend clears the certificate lifetime flag
// itself (the certificate stays), so only the demo session marker survives it.
export const EXPERT_RESET_KEEP = ["demoMode"];
// Kilobytes of a text attachment the preview reads, per step of the attachPreviewSize choice.
export const ATTACH_PREVIEW_KB = { short: 3, medium: 8, long: 32 };
export const TILE_STYLES = ["bold", "uppercase", "mono", "italic", "shadow", "compact", "gradient"];

// Expert settings whose name alone doesn't explain what they do get a themed
// tooltip (the hover engine picks up [title] and styles it). Key -> i18n tip key,
// convention tip<Key>. Self-explanatory settings are deliberately left out.
export const EXPERT_TIPS = [
  // General / creating
  "storeFiles", "attachTextPreview", "attachPreviewSize", "varDefaults", "dynVars", "choiceVars", "choiceRemember", "varFixed",
  "lengthCounter", "promptHistory", "editorMonospace",
  "promptNotes", "notesInTooltip", "notesTipLen",
  "mdPreview", "codeHighlight", "codeAutoDetect", "copyFormatted",
  "colorPicker", "pickerFormat", "loupeZoom", "pickHistoryN",
  "screenRecord", "recCursor", "recCountdownS", "recFps", "recAudioSystem", "recAudioMic", "recMicDevice",
  "recQuality", "recBitrateKbps", "recFormat", "recordingDir",
  "recStopHotkey", "recPauseHotkey", "recTrayEntries", "recCropNotice", "recBarSize", "recTargetSwitch",
  "recAutoCrop", "recCanvasMode", "recMaxMin", "recMaxMb",
  "keyOverlay", "keyOverlayShortcutsOnly",
  "cameraRecord", "cameraDevice", "cameraResolution", "cameraFramerate",
  "videoEditor", "videoEditSnap",
  "videoExportDir",
  "numFieldWheel", "errorLogKeepKb",
  "editStateKeep", "editStateKeepDays", "editorDraftSecs",
  "snipSaveQuality", "snipDelayMs",
  "snipDefaultColor",
  "snipEditSnap", "snipTextBack", "snipEditDefaultTool",
  "ocr", "ocrLang",
  "voice", "voiceInterim", "voicePunct", "voiceAutoCaps", "voiceAutoStop", "voiceAutoStopSec", "voiceMaxMin",
  "voiceLang", "voiceInsert", "voiceOnlineOk",
  "voiceHotkey", "voiceHotkeyTarget",
  "trayCustomMenu", "trayRecent", "trayFavs", "trayRecentN", "trayFavN", "traySearch",
  "importExport", "hoverDim", "keyboardNav", "allowMultipleInstances",
  "taskbarProgress", "dataDir", "dataDirLock", "demoMode",
  "floating", "floatParkCleanupH",
  "folders", "pillFontSize",
  "rotatingPrompts", "rotateDefaultMode", "rotateMaxEntries", "rotateBadge",
  "rotateAuto",
  "firstRunTour",
  // Tiles / copying
  "tileMenu", "typeBadges", "tileHotkeyBadge", "tileFavBadge", "bubbleMs", "copyFlash", "tilePressScale", "chainPrompts", "chainLock", "chainQuickStart", "autoPasteEnter",
  "promptHotkeys", "promptHotkeyPaste", "dragOut", "dragOutInvert", "dragOutModifier",
  "tooltipDelayMs",
  "dragToView",
  // Appearance
  "hideScrollbars", "gridLines", "autoInk", "inkThreshold", "inkHalo", "inkHaloStrength",
  "customThemes", "customThemeMax",
  "toastPos", "tileStyle",
  // Library / privacy
  "mediaMeta", "mediaMetaBackfill", "ocrIndex",
  "smartSort", "fuzzySearch", "batchOps", "searchSuggest", "searchAutofocus",
  "globalSearch",
  "tags", "tagFilterMode", "tagSort",
  "allowScreenshot", "copyHistory", "historyTimestamps", "historyGrouped",
  "dedupCopyLog", "cleanupFiles",
  // Tools / data
  "dupFinder", "clipWatcher", "backupGfs", "backupBeforeReset", "backupBeforeDelete",
  "recSweepBroken", "recDiscardKeepH",
  "usageStats", "promptSync", "syncDir", "clipSkipSecrets",
  "secretPrompts", "secretTimeoutMin",
  "undoToastMs",
  "errorLog",
  "archiveTag",
  // Values that need a unit/behaviour explanation
  "copyCooldownMs", "dblClickMs", "autoPasteDelayMs",
  "similarThreshold", "importFolderDepth", "similarHint", "clipMinChars", "clipInboxMax", "historyMax",
  "nameMaxLen", "searchListMax", "maxViews", "gridMax",
  "statsTopN", "multiViewPrompt",
  "barToggles", "barToggleBoth", "quickGrid", "quickGridHoverOnly", "multiView", "libraryCols", "copySizeLevel", "showLibrary",
  "remoteApi", "remoteNetworkFilter", "remoteTlsShortLived", "urlScheme", "remotePort", "remoteRateLimit",
  "recOutline", "recOutlineColor", "chainSeparator",
];
export const EXPERT_TIP_SET = new Set(EXPERT_TIPS);

// The screenshot formats on offer. A stored choice outside the list (an old
// install) reads as PNG.
export const SCREENSHOT_FORMATS = ["png", "jpeg", "webp", "bmp"];
export const normScreenshotFormat = (v) => (SCREENSHOT_FORMATS.includes(v) ? v : "png");

// Flags whose state changes the tile DOM itself (everything else is a body class).
// rotatingPrompts/rotateBadge gate a tile's own rotate-badge element -- without
// a full rebuild here, an already-open grid keeps showing (or hiding) it until some
// unrelated re-render happens to catch up.
export const TILE_DOM_FLAGS = new Set(["captions", "promptNotes", "notesInTooltip", "rotatingPrompts", "rotateBadge", "tileHotkeyBadge", "tileFavBadge", "autoInk", "inkHalo"]);

// Almost every expert value only drives a CSS variable, so rebuilding the whole grid
// for it threw away and recreated every tile: visible as a flash through the
// translucent settings overlay. Only these actually change tile DOM.
export const GRID_DOM_VALUES = new Set(["gridMax", "notesTipLen", "inkThreshold", "inkHaloStrength"]);

export const SETTINGS_AREA_LABEL = { normal: "settingsAreaNormal", toolbar: "settingsAreaToolbar", library: "settingsAreaLibrary", backup: "settingsAreaBackup" };
