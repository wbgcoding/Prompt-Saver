// Minimal window.__TAURI__ stub so the real UI can boot in a plain browser.
// Every backend call is answered with plausible, empty-ish data; anything not
// listed resolves to null. Used by smoke_test.py.
window.__SMOKE_ERRORS__ = [];
window.addEventListener("error", (e) => window.__SMOKE_ERRORS__.push("error: " + (e.error && e.error.stack || e.message)));
window.addEventListener("unhandledrejection", (e) =>
  window.__SMOKE_ERRORS__.push("rejection: " + (e.reason && (e.reason.stack || e.reason.message) || e.reason)));

// F14/F15: one prompt carries every Markdown construct, a fenced block per supported
// language, and the injection attempts the preview must render as literal text.
const MD_SAMPLE = [
  "# Heading one",
  "## Heading two",
  "",
  "Plain text with **bold**, *italic*, `inline code` and a [link](https://example.invalid).",
  "",
  "- first bullet",
  "- second bullet",
  "",
  "1. first step",
  "2. second step",
  "",
  "> quoted line",
  "",
  "---",
  "",
  "```js",
  "// comment",
  "const greet = (name) => `hi ${name}`;",
  "greet(\"world\");",
  "```",
  "",
  "```python",
  "# comment",
  "def greet(name):",
  "    return 'hi'",
  "```",
  "",
  "```html",
  "<!-- comment -->",
  "<a class=\"x\">link</a>",
  "```",
  "",
  "```css",
  "/* comment */",
  ".tile { color: #ff0000; }",
  "```",
  "",
  "```json",
  "{ \"name\": \"demo\", \"ok\": true }",
  "```",
  "",
  "```sql",
  "-- comment",
  "SELECT name FROM prompts;",
  "```",
  "",
  "```bash",
  "# comment",
  "for f in *.txt; do echo ok; done",
  "```",
  "",
  "```rust",
  "// comment",
  "fn main() { let n = 42; }",
  "```",
  "",
  "```",
  "no language tag, stays plain",
  "```",
  "",
  "<script>window.xssRan = 1;</script>",
  "<img src=x onerror=window.xssRan=1>",
  "<b onmouseover=alert(1)>raw html</b>",
  "[click me](javascript:alert(1))",
  "Bare javascript:alert(1) stays text.",
].join("\n");

const PROMPTS = Array.from({ length: 12 }, (_, i) => ({
  id: "p" + i,
  name: "Prompt " + (i + 1),
  // One prompt carries a placeholder so the fill-in dialog gets exercised too;
  // p3 is dynamic-only (must NOT ask) and p5 mixes a choice with a text variable.
  text: i === 1
    ? "Translate {#{Text}#} into {#{Language}#}."
    : i === 3
      ? "Meeting on {#{/date\\}#} at {#{/time\\}#}, {#{/weekday\\}#} (week {#{/week\\}#} of {#{/month\\}#} {#{/year\\}#}), " +
        "yesterday {#{/yesterday\\}#}, tomorrow {#{/tomorrow\\}#}, stamp {#{/timestamp\\}#}, epoch {#{/epoch\\}#}, " +
        "by {#{/username\\}#} ref {#{/random\\}#} no {#{/counter\\}#}: {#{/clipboard\\}#}"
      : i === 5
        ? "Write a {#{tone|formal,casual,witty}#} note about {#{Topic}#}."
        : i === 7
          ? MD_SAMPLE
          : "Example prompt body number " + (i + 1) + ".",
  color: ["#3b82f6", "#ef4444", "#10b981", "#f59e0b"][i % 4],
  favorite: i % 5 === 0,
  // Two shared tags plus untagged prompts, so a filter chip visibly narrows the list.
  tags: i % 3 === 0 ? ["Work"] : i % 3 === 1 ? ["Draft", "Work"] : [],
  // F17: exactly one prompt carries a private note, so the tooltip section and the
  // library search can be checked against a prompt that has none. "Moonshot" appears
  // in no name or body, so a search hit can only have come from the note.
  notes: i === 0 ? "Moonshot idea — ask legal first.\nRewrite the ending." : "",
  // F20: one prompt ships with a bound accelerator so the editor row has something
  // to display; the rest are free, so a capture can be checked against them.
  hotkey: i === 0 ? "CommandOrControl+Shift+9" : "",
  // F22: one prompt points at a file, so a drag-out can be checked against the path
  // payload and its file URI instead of the prompt body.
  file_path: i === 9 ? "C:\\Notes\\brief.txt" : "",
  // 79: one rotating prompt, so the version-history probe can open an editor that
  // actually has more than one entry (list_versions below gives it real snapshots).
  variants: i === 10 ? ["Example prompt body number 11, second entry.", "Example prompt body number 11, third entry."] : [],
  rotate_mode: "",
  // That same prompt has Auto switched on, so a probe can check that copying it
  // hands the prompt id to copy_text — the thing the backend needs to open an
  // auto-advance session at all.
  rotate_auto: i === 10,
  // One prompt carries a stored image — and carries it the way get_state really
  // hands it over: the FACT plus a revision, never the base64 bytes. The picture
  // itself is fetched per prompt through prompt_image below, so a fixture entry
  // growing an `image` field again would be a regression, not a convenience.
  has_image: i === 11,
  image_rev: i === 11 ? 4242 : 0,
  show_image: i === 11,
}));

// A valid 1x1 PNG — enough for an <img> to accept and for a probe to compare.
const STUB_IMAGE = "data:image/png;base64," +
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
window.__STUB_IMAGE__ = STUB_IMAGE;
// Where the bytes live in this fixture — id -> data URL, exactly like the backend
// keeping them in the store and handing out one at a time.
const STUB_IMAGES = new Map([["p11", STUB_IMAGE]]);
// Counted so a probe can prove the window asks once per image and then reuses it.
window.__PROMPT_IMAGE_CALLS__ = 0;

const LAYOUT = {};
PROMPTS.forEach((p, i) => { LAYOUT[p.id] = [i % 4, Math.floor(i / 4)]; });

// 32: mirrors Settings.rotate_pos — order-mode pointer per rotating prompt id.
const ROTATE_POS = new Map();

// 91: trash — three items so restore, single purge and empty-all each get their own
// target; t2 is anonymous, exercising the libLabel() fallback.
const TRASH = [
  { id: "t1", name: "Deleted idea", text: "Body of the deleted prompt.", trashed_at: 1750000000, color: "", tags: [], notes: "", hotkey: "", file_path: "" },
  { id: "t2", name: "", text: "Second trashed prompt.", trashed_at: 1750000100, color: "", tags: [], notes: "", hotkey: "", file_path: "" },
  { id: "t3", name: "Third trashed prompt", text: "Body three.", trashed_at: 1750000200, color: "", tags: [], notes: "", hotkey: "", file_path: "" },
];

const SETTINGS = {
  theme: "dark", language: "en", floating: {}, float_scale: {}, video_prefs: {},
  window: null, minimize_to_tray: false, autostart: false, start_minimized: false,
  // F20: the app hotkey is set, so a prompt trying to claim it must be refused.
  always_on_top: false, hotkey: "CommandOrControl+Shift+P", voice_hotkey: "", auto_update: true, skipped_versions: [],
  // Tags, custom themes, view groups and private notes ship off; the stub turns them
  // on so the smoke run covers that UI too. The Markdown preview is deliberately NOT
  // listed — it ships on, and the run proves that.
  // F25 is on too, so the expert menu renders its gated sliders and the folder
  // window has something to draw.
  // F26 is on too, so the pipette shows in the composer bar and its gated expert
  // rows (notation, magnifier zoom) actually render.
  // tourSeen: true -- this fixture models an existing, already-onboarded install
  // (a non-empty library, real settings), so the first-run tour must not pop
  // open under every later check the way it would for a genuinely fresh one.
  ui_flags: { tags: true, customThemes: true, viewGroups: true, promptNotes: true, pillFolders: true, colorPicker: true, tourSeen: true },
  ui_values: {}, ui_texts: {}, dup_ignored: [],
  example_prompt_ids: [], example_prompt_seeds: {},
  tag_colors: { work: "#3b82f6" },
  custom_themes: [{ id: "ct1", name: "Neon", base: "dark", vars: { accent: "#ff00aa" } }],
  tile_font: "system", tile_size: 0,
  // Point 419: membership lives in `members`, never derived from `layouts` any
  // more - home carries every prompt (matches the fully-packed 4x3 LAYOUT plus
  // the folder members below), the other three views start empty.
  views: [
    { id: "home", name: "Home", cols: 4, rows: 3, layouts: { "4x3": LAYOUT }, color: "", members: PROMPTS.map((p) => p.id) },
    { id: "work", name: "Work", cols: 4, rows: 3, layouts: {}, color: "#ef4444", members: [] },
    { id: "notes", name: "Notes", cols: 4, rows: 3, layouts: {}, color: "", members: [] },
    { id: "archive", name: "Archive", cols: 4, rows: 3, layouts: {}, color: "", members: [] },
  ],
  // Two of the four views live in a folder tab, so the tab bar shows both shapes.
  view_groups: [{ id: "g1", name: "Docs", color: "", view_ids: ["notes", "archive"] }],
  // F27: one grid folder on the home view, so the folder cell and its popover are
  // walked on every run. Its members are drawn by the folder, not as tiles. Five
  // members on purpose: the open popover is then taller than a small window leaves,
  // which is the case the popover placement has to get right.
  tile_folders: [{ id: "tf1", name: "Drafts", color: "", view_id: "home", cell: [0, 1],
    members: ["p4", "p6", "p8", "p10", "p11"] }],
  active_view: "home",
};
// A member has no cell of its own while it sits in a folder (mirrors the backend
// dropping it on join) — without this, dissolve would look like it worked even with
// a stub that never restored one, because the stale index-based cell was still there.
for (const f of SETTINGS.tile_folders) for (const m of f.members) delete LAYOUT[m];

const clone = () => JSON.parse(JSON.stringify(SETTINGS));

// Mirror of the backend's normalize_view_groups: no unknown ids, no view in two
// groups, no empty group left behind.
function normalizeGroups() {
  const taken = new Set();
  for (const g of SETTINGS.view_groups) {
    g.view_ids = g.view_ids.filter((id) => {
      if (taken.has(id) || !SETTINGS.views.some((v) => v.id === id)) return false;
      taken.add(id);
      return true;
    });
  }
  SETTINGS.view_groups = SETTINGS.view_groups.filter((g) => g.view_ids.length);
}

// Mirrors the backend's first_free_cell_excluding: row-major scan of the view's own
// layout, also skipping any cell in `extra` (another grid folder's own cell, which
// lives outside view.layouts).
function firstFreeCell(view, extra) {
  const key = view.cols + "x" + view.rows;
  const layout = view.layouts[key] || (view.layouts[key] = {});
  const taken = new Set(Object.values(layout).map((c) => c[0] + "," + c[1]));
  for (const c of extra || []) taken.add(c[0] + "," + c[1]);
  for (let row = 0; row < view.rows; row++) {
    for (let col = 0; col < view.cols; col++) {
      if (!taken.has(col + "," + row)) return { layout, cell: [col, row] };
    }
  }
  return { layout, cell: null };
}

// Mirrors the backend's examples_still_pristine: an id with no seed on record
// (or whose prompt already vanished) counts as unchanged; a seed present and no
// longer matching the prompt's current name+text means the user made it their own.
function examplePristineIds() {
  return SETTINGS.example_prompt_ids.filter((id) => {
    const seed = SETTINGS.example_prompt_seeds[id];
    if (seed === undefined) return true;
    const p = PROMPTS.find((x) => x.id === id);
    return !p || seed === p.name + " " + p.text;
  });
}

const RESULTS = {
  // Expert writes have to land in the stored blob: every later settings read (and
  // every window that re-reads them) must see the switch the user just flipped.
  set_ui_flag: (a) => { SETTINGS.ui_flags[a.key] = a.enabled; return null; },
  set_ui_value: (a) => { SETTINGS.ui_values[a.key] = a.value; return null; },
  set_ui_text: (a) => { SETTINGS.ui_texts[a.key] = a.value; return null; },
  get_settings: () => JSON.parse(JSON.stringify(SETTINGS)),
  get_state: () => ({ prompts: PROMPTS, settings: JSON.parse(JSON.stringify(SETTINGS)) }),
  prompt_image: (a) => {
    window.__PROMPT_IMAGE_CALLS__++;
    return STUB_IMAGES.get(a && a.id) || "";
  },
  launcher_items: () => ({
    prompts: PROMPTS.map((p) => ({ id: p.id, name: p.name, text: p.text, color: p.color || "" })),
    language: SETTINGS.language || "",
    usage: JSON.parse(JSON.stringify(SETTINGS.usage || {})),
    last_used: JSON.parse(JSON.stringify(SETTINGS.last_used || {})),
  }),
  current_theme: () => "dark",
  theme_vars: () => ({ accent: "#ff00aa" }),
  // Recording control bar's poll target -- a fixed fixture so the probe can
  // assert the timer/size labels render from it.
  recording_status: () => ({ paused: false, elapsed_ms: 65000, bytes: 2500000 }),
  pause_recording: () => { window.__REC_PAUSED__ = true; return null; },
  resume_recording: () => { window.__REC_PAUSED__ = false; return null; },
  stop_recording: () => { window.__REC_STOPPED__ = true; return null; },
  cancel_recording: () => { window.__REC_CANCELLED__ = true; return null; },
  hide_rec_bar: () => { window.__REC_BAR_HIDDEN__ = true; return null; },
  // Video editor: a fixed 10-minute 1080p30 recording with both a system and a
  // mic audio track -- real enough to exercise the filmstrip/waveform rendering
  // the same way an actual capture would, and shared by the UI smoke test and
  // the perf hot-path timing (both want the same non-degenerate fixture).
  // mic_sidecar stays empty so the boot never also has to route through
  // mux_mic_track, which nothing here stubs.
  video_editor_source: () => "C:\\Recordings\\ps-smoke-recording.mp4",
  windows_fonts_dir: () => "C:/Windows/Fonts",
  // 334: the image-overlay "Add" button picks a file before it can add its
  // op -- a fixed fake path is enough to exercise the add flow and the live
  // preview's <img> element; the path never actually needs to resolve.
  pick_file_path: () => "C:\\Pictures\\ps-smoke-badge.png",
  // 335: export-destination pickers for the video editor's export row.
  pick_export_dest: (a) => "C:\\Videos\\" + (a && a.defaultName || "export.mp4"),
  pick_folder: () => "C:\\Videos\\Exports",
  // 336: "Copy to clipboard" export -- a temp path plus a clipboard write the
  // probe can observe without a real OS clipboard.
  clip_export_path: (a) => "C:\\Temp\\ps-clipexport-test." + ((a && a.ext) || "mp4"),
  copy_file_to_clipboard: (a) => { window.__CLIP_EXPORT_COPIED__ = a && a.path; return true; },
  // 340: recordingDir's folder-size readout and cleanup flow -- a fixed
  // fixture so the probe can assert both the stats line and the confirm
  // list's contents regardless of which age tier is selected.
  video_folder_stats: () => [2, 5242880],
  video_folder_cleanup_preview: () => [
    { name: "old-recording.mp4", size: 3145728 },
  ],
  video_folder_cleanup: (a) => { window.__VIDEO_CLEANUP_CALLED__ = a; return 1; },
  probe_recording: () => ({
    duration_ms: 600000, fps: 30, width: 1920, height: 1080,
    has_audio: true, has_mic_track: true, mic_sidecar: "",
  }),
  generate_filmstrip: (a) => {
    const n = Math.max(1, Number(a && a.count) || 12);
    return Array.from({ length: n }, (_, i) => "C:\\Temp\\ps-filmstrip-" + i + ".jpg");
  },
  generate_waveform_peaks: (a) => {
    const n = Math.max(1, Number(a && a.buckets) || 200);
    return Array.from({ length: n }, (_, i) => Math.sin(i / 13) * (0.3 + 0.5 * Math.abs(Math.sin(i / 97))));
  },
  // Post-capture menu (241): a real ffmpeg run takes visible time, so this
  // resolves on a short timeout instead of immediately -- the probe fires a
  // recording-convert-progress event and checks the interim "Converting..."
  // state before it resolves. A source path containing "MISSING" simulates
  // ffmpeg not being installed (the one real error convert_recording can
  // return before ever starting a process).
  convert_recording: (a) => {
    if (a && a.srcPath && a.srcPath.includes("MISSING")) throw new Error("ffmpeg-missing");
    const dst = a.srcPath.replace(/\.[a-z0-9]+$/i, "." + a.format);
    return new Promise((resolve) => setTimeout(() => resolve(dst), 80));
  },
  cancel_recording_convert: () => null,
  save_recording_as: (a) => { window.__REC_SAVED_AS__ = a.srcPath; return "C:/fake/saved.mp4"; },
  discard_recording_file: (a) => { window.__REC_DISCARDED__ = a.path; return true; },
  // P100: the probe reads this back instead of a real backend hand-off to main.js.
  launcher_select: (a) => { window.__LAUNCHER_SELECTED__ = a && a.id; return null; },
  launcher_close: () => { window.__LAUNCHER_CLOSED__ = true; return null; },
  // The custom tray window's own content -- a small fixed fixture covering all
  // four entry kinds and a custom tile color is enough to exercise rendering
  // and keyboard/mouse handling; the real section/ordering logic is Rust-side
  // and already covered by its own unit tests (tray_plan and friends).
  tray_window_plan: () => ({
    sections: [
      { title: "Favorites", entries: [
        { id: "tray-copy-p1", label: "First prompt", kind: "text", color: "" },
        { id: "tray-copy-p2", label: "Image prompt", kind: "image", color: "#ff00aa" },
      ] },
      { title: "Recent", entries: [
        { id: "tray-copy-p3", label: "File prompt", kind: "file", color: "" },
      ] },
    ],
    showLabel: "Open Prompt Saver",
    quitLabel: "Quit",
  }),
  tray_window_activate: (a) => { window.__TRAY_ACTIVATED__ = a && a.id; return null; },
  tray_window_close: () => { window.__TRAY_CLOSED__ = true; return null; },
  set_theme: () => "dark",
  // Saving hands back the settings blob with the edited theme in place.
  save_custom_theme: (a) => {
    const list = SETTINGS.custom_themes;
    const id = (a && a.id) || "ct" + (list.length + 1);
    const theme = { id, name: (a && a.name) || "", base: (a && a.base) || "dark", vars: (a && a.vars) || {} };
    const at = list.findIndex((c) => c.id === id);
    if (at >= 0) list[at] = theme; else { list.push(theme); SETTINGS.theme = "custom:" + id; }
    return JSON.parse(JSON.stringify(SETTINGS));
  },
  delete_custom_theme: (a) => {
    SETTINGS.custom_themes = SETTINGS.custom_themes.filter((c) => c.id !== (a && a.id));
    if (SETTINGS.theme === "custom:" + (a && a.id)) SETTINGS.theme = "system";
    return JSON.parse(JSON.stringify(SETTINGS));
  },
  // 76: simulates the OS save dialog succeeding; a real "canceled" rejection is
  // never exercised here, since there is no real dialog for the harness to cancel.
  export_custom_theme: () => null,
  // 76: simulates picking a file and it parsing into a new saved theme — same
  // shape save_custom_theme returns, so the caller's refresh path is identical.
  import_custom_theme: () => {
    const id = "ct" + (SETTINGS.custom_themes.length + 1);
    const theme = { id, name: "Imported theme", base: "dark", vars: { bg: "#0a0a0a" } };
    SETTINGS.custom_themes.push(theme);
    SETTINGS.theme = "custom:" + id;
    return JSON.parse(JSON.stringify(SETTINGS));
  },
  app_version: () => "0.0.0",
  current_data_dir: () => "C:\\Data",
  // Sealed unless a page says otherwise, so the default walk sees the healthy
  // app. The seal probe sets window.__STORE_SEALED__ = false before boot; read
  // fresh on every call for the same reason ocr_capabilities is.
  store_sealed: () => window.__STORE_SEALED__ !== false,
  // Same fresh-read idea as store_sealed, for the write-fail probe below.
  settings_write_failed: () => window.__SETTINGS_WRITE_FAILED__ === true,
  // 122: a probe sets window.__OCR_CAPS_TEST__ before opening #ocr-langs-modal.
  // Read fresh on every call (unlike main.js's own `const { invoke } = ...`
  // capture at module load), so a probe can set it any time before triggering
  // the actual invoke call, no matter how long main.js has already been running.
  ocr_capabilities: () => {
    // A probe sets window.__OCR_CAPS_ELEV__ to play the Windows builds that
    // refuse even the read-only listing to a normal user. 177: the real
    // backend error is "code|raw text", the raw half varying by machine.
    if (window.__OCR_CAPS_ELEV__) throw "ocr-caps-need-elevation|Access to the path \"C:\\WINDOWS\\Logs\\DISM\" is denied.";
    return window.__OCR_CAPS_TEST__ || [];
  },
  // The elevated listing: launching it arms the result, the next poll hands the
  // list over -- the same two-step the real backend does through the UAC prompt.
  ocr_capabilities_elevate: () => { window.__OCR_CAPS_ELEV_DONE__ = true; return null; },
  ocr_capabilities_elevated_result: () => (window.__OCR_CAPS_ELEV_DONE__ ? window.__OCR_CAPS_TEST__ || [] : null),
  // 230: same shape as the speech_install_* pair below -- the elevated launch
  // itself resolves right away, the result poller (3s ticks) is what a probe
  // would wait out separately.
  // 230: opt-in gate for the double-click race test -- when a probe sets
  // window.__OCR_INSTALL_GATE__ to an array, this call parks its resolver
  // there instead of answering immediately, simulating the real elevated
  // install() call, which stays pending for as long as the user takes to
  // answer the UAC prompt. Unset (the default) resolves immediately.
  ocr_install_language: () => {
    if (Array.isArray(window.__OCR_INSTALL_GATE__)) {
      return new Promise((resolve) => { window.__OCR_INSTALL_GATE__.push({ resolve }); });
    }
    return null;
  },
  ocr_install_result: () => ({ ok: true }),
  // 127/130: a supported machine with two installed languages, one of them
  // matching the ocrLangsCache-equivalent shape (voiceLangsCache).
  voice_support: () => ({ supported: true, engine: "windows", langs: ["en-US", "de-DE"], needs_online: false }),
  voice_languages: () => [{ tag: "en-US", label: "English (US)" }, { tag: "de-DE", label: "Deutsch (Deutschland)" }],
  // Always hands back the same session id -- the probe fires voice-partial/
  // voice-text/voice-stopped/voice-error itself via window.__FIRE__, there is
  // no real recognizer here to drive them.
  voice_start: () => ({ id: 1, lang: "en-US" }),
  voice_stop: () => null,
  voice_enable_online_speech: () => { SETTINGS.ui_flags = { ...SETTINGS.ui_flags, voiceOnlineOk: true }; return null; },
  speech_capabilities: () => window.__OCR_CAPS_TEST__ || [],
  speech_capabilities_elevate: () => { window.__OCR_CAPS_ELEV_DONE__ = true; return null; },
  speech_capabilities_elevated_result: () => (window.__OCR_CAPS_ELEV_DONE__ ? window.__OCR_CAPS_TEST__ || [] : null),
  speech_install_language: () => null,
  speech_install_result: () => ({ ok: true }),
  voice_lang_snooze: () => null,
  open_privacy_mic: () => null,
  set_voice_hotkey: (a) => {
    const accel = String((a && a.hotkey) || "").toLowerCase();
    if (/^(commandorcontrol|ctrl)\+[a-z]$/.test(accel)) throw new Error("hotkey-system");
    if (accel === "commandorcontrol+alt+b") throw new Error("hotkey-conflict");
    SETTINGS.voice_hotkey = a && a.hotkey ? a.hotkey : "";
    return null;
  },
  default_screenshot_dir: () => "C:\\Pictures",
  default_sync_dir: () => "C:\\Data\\Prompts",
  // One snapshot carries its prompt count and names (61), one is from an older
  // build and has neither — the list has to show both without a gap in the row.
  open_backups_folder: () => null,
  // 17: p3 edited most recently, then p1, then p9 — everything else has no
  // version row (never content-edited, or history was off) and sorts last.
  // None of these three are favorited (favorite: i % 5 === 0) — favorites float
  // to the top independently of this sort, which would otherwise confuse the test.
  last_edited_map: () => ({ p3: 3000, p1: 2000, p9: 1000 }),
  // 339: records the format/ids the caller asked for so a probe can assert on
  // it, and returns a count the same shape the real command would (selection
  // size, or the whole fixture library when no ids were passed).
  export_prompts: (a) => {
    window.__EXPORT_CALL__ = a || {};
    return (a && Array.isArray(a.ids)) ? a.ids.length : PROMPTS.length;
  },
  // 88: two new prompts, one duplicate of something already in the library.
  preview_import: () => ({
    path: "C:\\fake\\import.csv",
    hash: "deadbeef",
    items: [
      { name: "New A", duplicate: false },
      { name: "Old B", duplicate: true },
      { name: "New C", duplicate: false },
    ],
  }),
  // 273: the real command rejects a confirm that never sends the preview's
  // hash back -- the stub does the same so a frontend regression that drops
  // the argument fails loudly instead of silently importing.
  import_prompts: (a) => {
    if (!a || !a.hash) throw new Error("import_prompts stub called without hash");
    return 2; // New A + New C from the preview fixture above
  },
  list_backups: () => [
    { name: "2026-07-25_18-30-00", size: 812345, prompts: 12, items: [
      { name: "Draft reply", chars: 214, kind: "text" },
      { name: "Meeting notes", chars: 0, kind: "text" }, // 103: legacy-cache stand-in (chars unknown -> "-")
    ] },
    { name: "2026-07-24_09-05-00", size: 654321, prompts: null, items: [] },
  ],
  recent_copies: () => PROMPTS.slice(0, 4).map((p, i) => ({ id: p.id, ts: 1750000000 + i, rowid: i + 1 })),
  delete_copy_entry: () => true,
  // 2: the fixture library is never empty, so the real seeding never actually
  // triggers (matches the backend's own "already has prompts" guard) — the mock
  // just needs to exist so the call is tracked, not error out.
  seed_example_prompts: () => [],
  example_prompts_pending: () => examplePristineIds().length,
  dismiss_example_prompts: () => {
    SETTINGS.example_prompt_ids = [];
    SETTINGS.example_prompt_seeds = {};
    return null;
  },
  clip_inbox_list: () => [],
  // P31: two snapshots for p6, each one word off from the next, so the diff
  // view has something real to highlight (list_versions is newest-first).
  // 79: two snapshots for p10 (the rotating prompt) with different variant lists,
  // so the per-entry diff/chip/name-change rendering has real rotation data to show.
  list_versions: (a) => (a && a.promptId === "p6" ? [
    { id: 2, ts: 1700000200, name: "Prompt 7", text: "Example draft body number 7." },
    { id: 1, ts: 1700000100, name: "Prompt 7", text: "Example draft text number 7." },
  ] : a && a.promptId === "p10" ? [
    {
      id: 4, ts: 1700000400, name: "Prompt 11",
      text: "Example prompt body number 11, second entry.",
      variants: ["Example prompt body number 11, third entry."],
    },
    {
      id: 3, ts: 1700000300, name: "Prompt eleven (old name)",
      text: "Example prompt body number 11, first entry.",
      variants: [],
    },
  ] : []),
  missing_files: () => [],
  has_backup_password: () => true,
  // One real group, so the duplicate cards actually render and the layout scan has
  // something to measure — an empty list only ever proved the "none found" line.
  find_duplicates: () => [{
    score: 0.94,
    members: [
      { id: "p0", name: PROMPTS[0].name, text: PROMPTS[0].text, usage: 12, views: ["Home"] },
      { id: "p1", name: PROMPTS[1].name, text: PROMPTS[1].text, usage: 3, views: ["Home", "Work"] },
    ],
  }],
  // A release body in the shape GitHub delivers it, so the changelog popup has real
  // Markdown to render (heading, wrapped paragraph, table, bullets, bold, code, link).
  check_update: () => ({
    available: true,
    version: "9.9.9",
    url: "https://example.invalid/setup.exe",
    skipped: false,
    notes: [
      "## Release 9.9.9",
      "",
      "A short intro paragraph that the release notes",
      "wrap over two source lines.",
      "",
      "### Downloads",
      "",
      "| File | Use it when |",
      "|---|---|",
      "| `setup.exe` | Installer |",
      "",
      "### Highlights",
      "",
      "- **Bold lead** — followed by ordinary text.",
      "- A bullet with `code` and a [link](https://example.invalid).",
    ].join("\n"),
  }),
  // The FULL prompt, image bytes included — the editor's one fetch when it opens.
  get_prompt: (a) => {
    const p = PROMPTS.find((x) => x.id === (a && a.id)) || PROMPTS[0];
    return { ...p, image: STUB_IMAGES.get(p.id) || "" };
  },
  // 91: trash — restore/purge mutate the shared TRASH/PROMPTS fixtures so the dialog
  // reflects real effects, the way batch_prompts already mirrors "color" onto PROMPTS.
  list_trash: () => JSON.parse(JSON.stringify(TRASH)),
  restore_prompt: (a) => {
    const idx = TRASH.findIndex((p) => p.id === (a && a.id));
    if (idx >= 0) {
      const [p] = TRASH.splice(idx, 1);
      p.trashed_at = 0;
      PROMPTS.push(p);
    }
    return { prompts: JSON.parse(JSON.stringify(PROMPTS)), settings: JSON.parse(JSON.stringify(SETTINGS)) };
  },
  purge_prompt: (a) => {
    const idx = TRASH.findIndex((p) => p.id === (a && a.id));
    if (idx >= 0) TRASH.splice(idx, 1);
    return JSON.parse(JSON.stringify(TRASH));
  },
  empty_trash: () => { TRASH.length = 0; return []; },
  // F8/P11: shared by the library's and the grid's bulk toolbars. The caller
  // (doBatch/doGridBatch) already mirrors "delete"/"color" onto its own local
  // prompts array, so this only needs to track the call and hand back settings.
  batch_prompts: (a) => {
    const ids = (a && a.ids) || [];
    if (a && a.action === "color") {
      for (const id of ids) { const p = PROMPTS.find((x) => x.id === id); if (p) p.color = a.color || ""; }
    }
    // 51: mirrors the real backend enough for the "also show in" popover to see a
    // genuine placement change, not just a settings echo.
    if (a && (a.action === "addView" || a.action === "removeView")) {
      const view = SETTINGS.views.find((v) => v.id === a.viewId);
      if (view) {
        const key = view.cols + "x" + view.rows;
        const layout = view.layouts[key] || (view.layouts[key] = {});
        if (a.action === "removeView") {
          for (const id of ids) delete layout[id];
        } else {
          for (const id of ids) {
            if (id in layout) continue;
            const { cell } = firstFreeCell(view);
            if (cell) layout[id] = cell;
          }
        }
      }
    }
    return JSON.parse(JSON.stringify(SETTINGS));
  },
  // Mirrors the backend: a new prompt lands on the active view's first free cell,
  // same firstFreeCell scan the folder-dissolve stub above already uses.
  add_prompt: (a) => {
    const prompt = {
      id: "new" + (PROMPTS.length + 1), name: a.name || "", text: a.text || "",
      // Like the backend: the bytes stay out of the prompt list, only the fact
      // and a revision travel with it.
      color: a.color || "", has_image: !!a.image, image_rev: a.image ? 1 : 0, show_image: !!a.showImage,
      copy_image: !!a.copyImage, file_path: a.filePath || "", icon_path: a.iconPath || "",
      caption: a.caption || "", caption_size: a.captionSize || 0, font: a.font || "",
      font_size: a.fontSize || 0, favorite: !!a.favorite, tags: a.tags || [],
      notes: a.notes || "", hotkey: a.hotkey || "", hotkey_paste: a.hotkeyPaste || "",
      variants: a.variants || [], rotate_mode: a.rotateMode || "",
    };
    if (a.image) STUB_IMAGES.set(prompt.id, a.image);
    PROMPTS.push(prompt);
    const view = SETTINGS.views.find((v) => v.id === SETTINGS.active_view);
    if (view) {
      const { layout, cell } = firstFreeCell(view);
      if (cell) layout[prompt.id] = cell;
    }
    return prompt;
  },
  // Item 1: the popup's per-prompt rows write straight through this (no editor
  // save step of their own) — mirrors the real backend's "hands back what it
  // really stored" contract closely enough for the wiring the smoke test proves.
  update_prompt: (a) => {
    const p = PROMPTS.find((x) => x.id === (a && a.id));
    if (!p) return null;
    if (a.hotkey !== undefined) p.hotkey = a.hotkey;
    if (a.hotkeyPaste !== undefined) p.hotkey_paste = a.hotkeyPaste;
    return p;
  },
  set_active_view: (a) => {
    if (SETTINGS.views.some((v) => v.id === (a && a.id))) SETTINGS.active_view = a.id;
    return clone();
  },
  add_view: (a) => {
    const id = "v" + (SETTINGS.views.length + 1);
    SETTINGS.views.push({ id, name: (a && a.name) || "", cols: 5, rows: 4, layouts: {}, color: (a && a.color) || "", members: [] });
    SETTINGS.active_view = id;
    return clone();
  },
  rename_view: (a) => {
    const view = SETTINGS.views.find((v) => v.id === (a && a.id));
    if (view && a.name) view.name = a.name;
    return clone();
  },
  set_view_color: (a) => {
    const view = SETTINGS.views.find((v) => v.id === (a && a.id));
    if (view) view.color = (a && a.color) || "";
    return clone();
  },
  // Mirror of the backend's rebuild_view_layout (point 419): membership never
  // changes here, only where a member sits. Keeps whatever still fits at the new
  // size, then seats the rest in the first free cell - one left without room
  // stays a member with no cell (overflow), never dropped from `members`.
  set_view_grid: (a) => {
    const view = SETTINGS.views.find((v) => v.id === (a && a.id));
    if (view) {
      view.cols = a.cols;
      view.rows = a.rows;
      const key = view.cols + "x" + view.rows;
      const folderMembers = new Set(
        SETTINGS.tile_folders.filter((f) => f.view_id === view.id).flatMap((f) => f.members)
      );
      const folderCells = SETTINGS.tile_folders
        .filter((f) => f.view_id === view.id)
        .map((f) => f.cell[0] + "," + f.cell[1]);
      const layout = view.layouts[key] || (view.layouts[key] = {});
      for (const [pid, cell] of Object.entries(layout)) {
        if (
          !view.members.includes(pid) || folderMembers.has(pid) ||
          cell[0] >= view.cols || cell[1] >= view.rows
        ) delete layout[pid];
      }
      const occupied = new Set(folderCells);
      for (const c of Object.values(layout)) occupied.add(c[0] + "," + c[1]);
      for (const pid of view.members) {
        if (folderMembers.has(pid) || layout[pid]) continue;
        outer: for (let row = 0; row < view.rows; row++) {
          for (let col = 0; col < view.cols; col++) {
            if (!occupied.has(col + "," + row)) {
              layout[pid] = [col, row];
              occupied.add(col + "," + row);
              break outer;
            }
          }
        }
      }
    }
    return clone();
  },
  delete_view: (a) => {
    if (SETTINGS.views.length > 1) SETTINGS.views = SETTINGS.views.filter((v) => v.id !== (a && a.id));
    if (!SETTINGS.views.some((v) => v.id === SETTINGS.active_view)) SETTINGS.active_view = SETTINGS.views[0]?.id || "";
    return clone();
  },
  // The view-group commands answer with the settings blob, like the real backend.
  save_view_group: (a) => {
    const ids = (a && a.viewIds) || [];
    for (const g of SETTINGS.view_groups) {
      if (g.id !== (a && a.id)) g.view_ids = g.view_ids.filter((v) => !ids.includes(v));
    }
    const at = SETTINGS.view_groups.find((g) => g.id === (a && a.id));
    if (at) { at.name = a.name; at.color = a.color; at.view_ids = ids; }
    else SETTINGS.view_groups.push({ id: "g" + (SETTINGS.view_groups.length + 1), name: a.name, color: a.color, view_ids: ids });
    normalizeGroups();
    return clone();
  },
  set_view_group_member: (a) => {
    for (const g of SETTINGS.view_groups) g.view_ids = g.view_ids.filter((v) => v !== (a && a.viewId));
    const target = SETTINGS.view_groups.find((g) => g.id === (a && a.groupId));
    if (target) target.view_ids.push(a.viewId);
    normalizeGroups();
    return clone();
  },
  delete_view_group: (a) => {
    SETTINGS.view_groups = SETTINGS.view_groups.filter((g) => g.id !== (a && a.id));
    return clone();
  },
  // F27 grid folders: same answers as the backend — the settings blob, invariants
  // applied (no prompt in two folders, under two members dissolves).
  save_tile_folder: (a) => {
    const members = (a && a.members) || [];
    for (const f of SETTINGS.tile_folders) {
      if (f.id !== (a && a.id)) f.members = f.members.filter((m) => !members.includes(m));
    }
    const at = SETTINGS.tile_folders.find((f) => f.id === (a && a.id));
    if (at) { at.name = a.name; at.color = a.color; at.cell = a.cell; at.members = members; }
    else SETTINGS.tile_folders.push({ id: "tf" + (SETTINGS.tile_folders.length + 1), name: a.name, color: a.color, view_id: a.viewId, cell: a.cell, members });
    SETTINGS.tile_folders = SETTINGS.tile_folders.filter((f) => f.members.length >= 2);
    return clone();
  },
  take_out_of_tile_folder: (a) => {
    const f = SETTINGS.tile_folders.find((x) => x.members.includes(a && a.id));
    for (const g of SETTINGS.tile_folders) g.members = g.members.filter((m) => m !== (a && a.id));
    SETTINGS.tile_folders = SETTINGS.tile_folders.filter((g) => g.members.length >= 2);
    const view = f && SETTINGS.views.find((v) => v.id === f.view_id);
    if (view && a.cell && a.cell[0] < view.cols && a.cell[1] < view.rows) {
      const key = view.cols + "x" + view.rows;
      const layout = view.layouts[key] || (view.layouts[key] = {});
      for (const [pid, c] of Object.entries(layout)) if (c[0] === a.cell[0] && c[1] === a.cell[1]) delete layout[pid];
      layout[a.id] = a.cell;
    }
    return clone();
  },
  delete_tile_folder: (a) => {
    const f = SETTINGS.tile_folders.find((x) => x.id === (a && a.id));
    SETTINGS.tile_folders = SETTINGS.tile_folders.filter((x) => x.id !== (a && a.id));
    const view = f && SETTINGS.views.find((v) => v.id === f.view_id);
    if (view) {
      // Other folders still on this view keep their own cell — a dissolved member
      // must not be handed one of those, or it saves fine but never renders.
      const otherCells = SETTINGS.tile_folders.filter((g) => g.view_id === f.view_id).map((g) => g.cell);
      for (const member of f.members) {
        const { layout, cell } = firstFreeCell(view, otherCells);
        if (cell) layout[member] = cell;
      }
    }
    return clone();
  },
  // The tag commands answer with the settings blob, like the real backend.
  // Mirrors the backend's rename_tag_in + its color-transfer rule closely enough
  // for the tag manager's own refresh() (a real get_state fetch) to see the
  // renamed/merged result -- renaming onto an existing tag key dedupes per
  // prompt exactly like a merge would.
  rename_tag: (a) => {
    const oldKey = String((a && a.old) || "").trim().toLowerCase();
    const newName = String((a && a.new) || "").trim();
    const newKey = newName.toLowerCase();
    for (const p of PROMPTS) {
      if (!(p.tags || []).some((t) => t.toLowerCase() === oldKey)) continue;
      const kept = [];
      for (const t of p.tags) {
        const name = t.toLowerCase() === oldKey ? newName : t;
        if (!kept.some((k) => k.toLowerCase() === name.toLowerCase())) kept.push(name);
      }
      p.tags = kept;
    }
    if (oldKey in SETTINGS.tag_colors) {
      const color = SETTINGS.tag_colors[oldKey];
      delete SETTINGS.tag_colors[oldKey];
      if (!(newKey in SETTINGS.tag_colors)) SETTINGS.tag_colors[newKey] = color;
    }
    return clone();
  },
  delete_tag: () => JSON.parse(JSON.stringify(SETTINGS)),
  set_tag_color: (a) => {
    const key = String((a && a.name) || "").toLowerCase();
    if (a && a.color) SETTINGS.tag_colors[key] = a.color;
    else delete SETTINGS.tag_colors[key];
    return clone();
  },
  set_tag_colors: (a) => {
    for (const e of (a && a.entries) || []) {
      const key = String(e.name || "").toLowerCase();
      if (e.color) SETTINGS.tag_colors[key] = e.color;
      else delete SETTINGS.tag_colors[key];
    }
    return clone();
  },
  // The copy commands report success; the UI skips everything that follows a copy
  // (feedback, history, auto-paste) when they come back falsy.
  // 32: order-mode only (random's shuffle-bag is covered by the 5 Rust unit tests,
  // not re-proven here) — enough to exercise the peek→copy→commit wiring for real.
  copy_prompt: (a) => {
    const p = PROMPTS.find((x) => x.id === (a && a.id));
    if (p) {
      const total = 1 + (p.variants || []).length;
      const idx = (ROTATE_POS.get(p.id) || 0) % total;
      window.__LAST_COPY__ = idx === 0 ? p.text : p.variants[idx - 1];
      if (total > 1) ROTATE_POS.set(p.id, (idx + 1) % total);
    }
    return true;
  },
  rotation_peek: (a) => {
    // 230: opt-in gate for the stale-peek-reply race test -- when a probe sets
    // window.__ROTATION_PEEK_GATE__ to an array, this call parks its resolver
    // there instead of answering immediately, so the probe can deliver a newer
    // rotation-changed push first and resolve the stale peek after. Unset (the
    // default) behaves exactly as before for every other test.
    if (Array.isArray(window.__ROTATION_PEEK_GATE__)) {
      return new Promise((resolve) => { window.__ROTATION_PEEK_GATE__.push({ resolve }); });
    }
    const p = PROMPTS.find((x) => x.id === (a && a.id));
    if (!p) return null;
    const total = 1 + (p.variants || []).length;
    const idx = (ROTATE_POS.get(p.id) || 0) % total;
    // 33: the stub only ever simulates sequential advance (real random-bag shuffling
    // is a backend concern, proven in Rust unit tests) — pos = idx + 1 mirrors what
    // order mode itself returns, which is all the UI-side wiring needs to exercise.
    return { index: idx, pos: idx + 1, total, text: idx === 0 ? p.text : p.variants[idx - 1] };
  },
  rotation_commit: (a) => {
    const p = PROMPTS.find((x) => x.id === (a && a.id));
    if (!p) return null;
    const total = 1 + (p.variants || []).length;
    if (total > 1) ROTATE_POS.set(p.id, ((ROTATE_POS.get(p.id) || 0) + 1) % total);
    return null;
  },
  // The resolved text is kept so the probe can assert what the placeholders became.
  copy_text: (a) => {
    window.__LAST_COPY__ = a && a.text;
    // The id decides whether the backend opens an auto-advance session, so a probe
    // has to be able to see whether this path handed one over.
    window.__LAST_COPY_ID__ = a && a.id;
    return true;
  },
  clipboard_text: () => "CLIPTEXT",
  // Always offer the portable take-over so the dialog is exercised on every run.
  takeover_offer: () => "C:\\Users\\Example\\AppData\\Roaming\\Example-Store",
  takeover_apply: () => null,
  pick_backup_file: () => "C:\\Backups\\Backup 2026-07-25.psb",
  // Refuses until a password is supplied — drives the retry prompt.
  import_all: (a) => {
    if (!a || !a.password) throw new Error("password-required");
    if (a.password !== "letmein") throw new Error("wrong password or corrupt backup");
    return { name: "Backup 2026-07-25.psb", count: 12, dropped_hotkeys: 2 };
  },
  focus_main_window: () => null,
  // 19: the picker's right-click history popover — a fixed, non-empty list so
  // the happy path (list, click, copy) is exercised on every run.
  pick_history: () => ["#AABBCC", "#112233"],
  copy_pick_history_entry: (a) => { window.__LAST_COPY__ = a && a.hex; return (a && a.hex) || ""; },
  os_user_name: () => "tester",
  // F20: the backend is the authority on a capture. Same three answers the real
  // check gives, so the editor's inline error is driven by a real rejection.
  check_hotkey: (a) => {
    const accel = String((a && a.hotkey) || "").toLowerCase();
    if (/^(commandorcontrol|ctrl)\+[a-z]$/.test(accel)) throw new Error("hotkey-system");
    if (accel === "commandorcontrol+alt+b") throw new Error("hotkey-conflict");
    return null;
  },
  // Item 1: the global hotkey, same three rejections check_hotkey already gives.
  set_hotkey: (a) => {
    const accel = String((a && a.hotkey) || "").toLowerCase();
    if (/^(commandorcontrol|ctrl)\+[a-z]$/.test(accel)) throw new Error("hotkey-system");
    if (accel === "commandorcontrol+alt+b") throw new Error("hotkey-conflict");
    SETTINGS.hotkey = a && a.hotkey ? a.hotkey : "";
    return null;
  },
  // F12: the standalone fill-in window. The main window hands over a request here;
  // the small window (vars.html) pulls it and answers.
  open_var_window: (a) => { window.__VAR_PAYLOAD__ = a && a.payload; return null; },
  var_window_payload: () => JSON.stringify(VAR_REQUEST),
  var_window_result: (a) => { window.__VAR_RESULT__ = (a && a.values) ?? null; return null; },
  // F25 pill folders: one folder with five members, so the closed icon has more
  // previews than the 2x2 grid shows and the badge has a real count.
  float_folder_state: () => ({ id: "f1", name: "Drafts", scale: 1, cols: 3, members: PROMPTS.slice(0, 5) }),
  // The backend answers with the layout it applied to the window (logical px).
  expand_float_folder: (a) => ({
    side: "below",
    icon_x: 0, icon_y: 0,
    pop_x: 0, pop_y: 102,
    width: Math.max(96, (a && a.width) || 96),
    height: 102 + ((a && a.height) || 0),
  }),
  collapse_float_folder: () => null,
  set_float_folder_scale: () => null,
  dissolve_float_folder: () => null,
  remove_folder_member: () => null,
  rename_float_folder: () => null,
  folder_rename_request: () => null,
  float_folder_drop: () => false,
  // Pin/unpin a floating button. Like the backend: "pinned" is the settings entry,
  // the answer is the state the prompt was left in, and the position is only a
  // placeholder here because no real pill window exists in the stub.
  toggle_floating: (a) => {
    const id = a && a.id;
    if (!id) return false;
    if (SETTINGS.floating[id]) {
      delete SETTINGS.floating[id];
      return false;
    }
    SETTINGS.floating[id] = { x: 0, y: 0 };
    return true;
  },
  // Screenshot overlay: no capture session, nothing to draw.
  snip_should_begin: () => false,
  snip_background: () => null,
  snip_windows: () => [],
  snip_present: () => false,
  usage_stats: () => ({
    total_prompts: 12, total_copies: 40, copies7: 5, copies30: 20, used_count: 8, avg_copies: 3.3, total_chars: 900,
    recent_name: "Prompt 1", recent_ts: 1750000000,
    longest_name: "Prompt 2", longest_chars: 120,
    types: [{ name: "text", count: 12 }],
    top: [{ name: "Prompt 1", count: 9 }],
    never_used: [{ name: "Prompt 9", count: 0 }],
    per_view: [{ name: "Home", count: 12 }],
    // 58: 14 days, newest last — a couple of zero days mixed in like a real history.
    daily: Array.from({ length: 14 }, (_, i) => ({
      ts: 1750000000 - (13 - i) * 86400,
      count: [0, 3, 1, 0, 5, 2, 4, 0, 1, 6, 2, 3, 1, 4][i],
    })),
    history_on: true,
  }),
};

// What main.js sends to vars.html: one free-text row with the remember switch and
// one choice row, so the small window renders both shapes.
const VAR_REQUEST = {
  theme: "dark",
  vars: { accent: "#ff00aa" },
  title: "Fill in the prompt",
  ok: "Copy",
  cancel: "Cancel",
  rememberLabel: "Remember",
  askRemember: true,
  rows: [
    { name: "Topic", options: [], value: "roadmap", remember: true },
    { name: "tone", options: ["formal", "casual", "witty"], value: "casual", remember: false },
  ],
};

const noopWindow = {
  // floating.js derives the prompt id from the window label, folder.js the folder id.
  label: location.pathname.endsWith("folder.html") ? "folder-f1" : "float-p0",
  listen: async () => () => {}, onCloseRequested: async () => () => {},
  onFocusChanged: async () => () => {}, onResized: async () => () => {},
  onMoved: async () => () => {}, onScaleChanged: async () => () => {},
  onThemeChanged: async () => () => {}, setAlwaysOnTop: async () => {},
  setSize: async () => {}, setPosition: async () => {},
  outerPosition: async () => ({ x: 0, y: 0 }),
  outerSize: async () => ({ width: innerWidth, height: innerHeight }),
  startDragging: async () => {}, startResizeDragging: async () => {}, setIgnoreCursorEvents: async () => {},
  minimize: async () => {}, maximize: async () => {}, unmaximize: async () => {},
  isMaximized: async () => false, close: async () => {}, hide: async () => {},
  show: async () => {}, setFocus: async () => {},
  innerSize: async () => ({ width: innerWidth, height: innerHeight }),
  scaleFactor: async () => 1, setZoom: async () => {},
};

// Backend events are kept so the probe can deliver one (window.__FIRE__), which is
// the only way to exercise a code path the backend normally starts — e.g. a prompt
// hotkey being pressed while the app sits in the tray.
const LISTENERS = {};
window.__FIRE__ = (event, payload) => {
  for (const fn of LISTENERS[event] || []) fn({ event, payload });
};

window.__TAURI__ = {
  core: {
    invoke: async (cmd, args) => {
      (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []).push(cmd);
      (window.__SMOKE_ARGS__ = window.__SMOKE_ARGS__ || []).push([cmd, args]);
      const fn = RESULTS[cmd];
      return fn ? fn(args) : null;
    },
    // Real Tauri turns a filesystem path into a loadable "asset://..." URL --
    // the stub just needs SOMETHING stable so a <video>/<img> src assignment
    // does not throw when a probe exercises a real-file preview path.
    convertFileSrc: (path) => `asset://localhost/${encodeURIComponent(path)}`,
  },
  event: {
    listen: async (event, fn) => {
      (LISTENERS[event] = LISTENERS[event] || []).push(fn);
      return () => {};
    },
    emit: async () => {},
  },
  // 179: real Tauri v2 exports this alongside startResizeDragging() -- the
  // screenshot editor's resize grip reads window.__TAURI__.window.ResizeDirection.
  window: { getCurrentWindow: () => noopWindow, ResizeDirection: {
    North: "North", NorthEast: "NorthEast", East: "East", SouthEast: "SouthEast",
    South: "South", SouthWest: "SouthWest", West: "West", NorthWest: "NorthWest",
  } },
  webviewWindow: { getCurrentWebviewWindow: () => noopWindow },
};
window.__TAURI__.invoke = window.__TAURI__.core.invoke;
