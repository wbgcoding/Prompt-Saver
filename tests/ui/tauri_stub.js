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
  // Text has a default (Hello) so the fill-in dialog's prefill can be checked;
  // Language has none, so the same check also covers the plain, empty case.
  text: i === 1
    ? "Translate {#{Text=Hello}#} into {#{Language}#}."
    : i === 3
      ? "Meeting on {#{/date\\}#} at {#{/time\\}#}, {#{/weekday\\}#} (week {#{/week\\}#} of {#{/month\\}#} {#{/year\\}#}), " +
        "yesterday {#{/yesterday\\}#}, tomorrow {#{/tomorrow\\}#}, stamp {#{/timestamp\\}#}, epoch {#{/epoch\\}#}, " +
        "by {#{/username\\}#} ref {#{/random\\}#} no {#{/counter\\}#}: {#{/clipboard\\}#}"
      : i === 5
        ? "Write a {#{tone=casual|formal,casual,witty}#} note about {#{Topic}#}."
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
  // payload and its file URI instead of the prompt body. The same prompt that
  // carries the stored preview image below also gets a file here, so there is one
  // fixture where both sit side by side and the preview/attachment hint can fire.
  file_path: i === 9 || i === 11 ? "C:\\Notes\\brief.txt" : "",
  // 400: a video attachment (icon_path, never file_path -- see the Prompt struct
  // comment) with its duration/resolution/size already measured, the way a real
  // attach stores them once. Lets the library/history badge + tooltip probe run
  // without depending on the backfill round trip through a real ffprobe.
  icon_path: i === 2 ? "C:\\Recordings\\ps-smoke-clip.mp4" : "",
  media_duration_ms: i === 2 ? 75000 : 0,
  media_width: i === 2 ? 1280 : 0,
  media_height: i === 2 ? 720 : 0,
  media_size_bytes: i === 2 ? 5242880 : 0,
  // 79: one rotating prompt, so the version-history probe can open an editor that
  // actually has more than one entry (list_versions below gives it real snapshots).
  variants: i === 10 ? ["Example prompt body number 11, second entry.", "Example prompt body number 11, third entry."] : [],
  rotate_mode: "",
  // That same prompt has Auto switched on, so a probe can check that copying it
  // hands the prompt id to copy_text: the thing the backend needs to open an
  // auto-advance session at all.
  rotate_auto: i === 10,
  // One prompt carries a stored image, and carries it the way get_state really
  // hands it over: the FACT plus a revision, never the base64 bytes. The picture
  // itself is fetched per prompt through prompt_image below, so a fixture entry
  // growing an `image` field again would be a regression, not a convenience.
  has_image: i === 11,
  image_rev: i === 11 ? 4242 : 0,
  show_image: i === 11,
}));

// 458: one confidential fixture, added only for the dedicated secret-vault probe
// so every other case keeps seeing exactly the twelve plain prompts above.
// SECRET_REAL_TEXT is what get_state/get_prompt hand back once unlocked; while
// locked they show "" the same way the real backend's slim_prompts blanks it.
const SECRET_REAL_TEXT = "REAL_TEXT_MARKER";
let SECRET_UNLOCKED = false;
let SECRET_CONFIGURED = false;
let SECRET_PASSWORD = null;
if (window.__SECRET_TEST__) {
  PROMPTS.push({
    id: "psecret", name: "Confidential prompt", text: SECRET_REAL_TEXT, color: "#a855f7",
    favorite: false, tags: [], notes: "", hotkey: "", hotkey_paste: "", file_path: "",
    pdf_page: 0, icon_path: "", caption: "", caption_size: 0, font: "", font_size: 0,
    media_duration_ms: 0, media_width: 0, media_height: 0, media_size_bytes: 0,
    variants: [], rotate_mode: "", rotate_auto: false, has_image: false, image_rev: 0,
    show_image: false, secret: true,
  });
  // Simulates a vault set up in an earlier session: the probe can then exercise
  // the plain unlock dialog (and a wrong password) instead of first-time setup.
  if (window.__SECRET_PRECONFIGURED__) {
    SECRET_CONFIGURED = true;
    SECRET_PASSWORD = "right";
  }
}
// Presents a prompt the way the real backend's slim_prompts does: blanked
// text/variants and locked:true with no session key, the real values once the
// vault has been unlocked. Returns non-secret prompts unchanged.
function presentPrompt(p) {
  if (!p.secret) return p;
  const locked = !SECRET_UNLOCKED;
  return { ...p, text: locked ? "" : p.text, variants: locked ? [] : (p.variants || []), locked };
}

// A valid 1x1 PNG: enough for an <img> to accept and for a probe to compare.
const STUB_IMAGE = "data:image/png;base64," +
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
window.__STUB_IMAGE__ = STUB_IMAGE;
// Where the bytes live in this fixture: id -> data URL, exactly like the backend
// keeping them in the store and handing out one at a time.
const STUB_IMAGES = new Map([["p11", STUB_IMAGE]]);
// Counted so a probe can prove the window asks once per image and then reuses it.
window.__PROMPT_IMAGE_CALLS__ = 0;

const LAYOUT = {};
PROMPTS.forEach((p, i) => { LAYOUT[p.id] = [i % 4, Math.floor(i / 4)]; });

// 32: mirrors Settings.rotate_pos: order-mode pointer per rotating prompt id.
const ROTATE_POS = new Map();

// 91: trash: three items so restore, single purge and empty-all each get their own
// target; t2 is anonymous, exercising the libLabel() fallback.
const TRASH = [
  { id: "t1", name: "Deleted idea", text: "Body of the deleted prompt.", trashed_at: 1750000000, color: "", tags: [], notes: "", hotkey: "", file_path: "" },
  { id: "t2", name: "", text: "Second trashed prompt.", trashed_at: 1750000100, color: "", tags: [], notes: "", hotkey: "", file_path: "" },
  { id: "t3", name: "Third trashed prompt", text: "Body three.", trashed_at: 1750000200, color: "", tags: [], notes: "", hotkey: "", file_path: "" },
];

// 439: recent-messages ring buffer, newest first (mirrors the real backend's
// insert-at-front, filled only through the log_error stub below).
const ERROR_LOG = [];

const SETTINGS = {
  theme: "dark", language: "en", floating: {}, float_scale: {}, video_prefs: {},
  window: null, minimize_to_tray: false, autostart: false, start_minimized: false,
  // F20: the app hotkey is set, so a prompt trying to claim it must be refused.
  always_on_top: false, hotkey: "CommandOrControl+Shift+P", voice_hotkey: "",
  // 343: a real value on each so the "All hotkeys" modal has something to render
  // and format for both of the moved expert-menu rows.
  rec_stop_hotkey: "CommandOrControl+Shift+8", rec_pause_hotkey: "CommandOrControl+Shift+7",
  auto_update: true, skipped_versions: [],
  // Tags, custom themes, view groups and private notes ship off; the stub turns them
  // on so the smoke run covers that UI too. The Markdown preview is deliberately NOT
  // listed; it ships on, and the run proves that.
  // F25 is on too, so the expert menu renders its gated sliders and the folder
  // window has something to draw.
  // F26 is on too, so the pipette shows in the composer bar and its gated expert
  // rows (notation, magnifier zoom) actually render.
  // tourSeen: true -- this fixture models an existing, already-onboarded install
  // (a non-empty library, real settings), so the first-run tour must not pop
  // open under every later check the way it would for a genuinely fresh one.
  // snipTextPlate: false models the same kind of existing install, from before
  // the text tool's background became a three-way choice -- its old boolean
  // setting must convert to "none" on read without the user touching anything.
  ui_flags: {
    tags: true, customThemes: true, promptNotes: true, pillFolders: true,
    // The statistics checks read a dashboard that is switched on; a fresh install starts
    // with it off (the dedicated probe seeds that case).
    usageStats: true,
    // 455/390: the dedicated first-run-tour probe boots with tourSeen unset so
    // maybeOpenFirstRunTour actually opens the draft-buffered path -- every other
    // boot keeps it true, same reasoning as secretPrompts/demoMode above.
    colorPicker: true, tourSeen: !window.__TOUR_FIRSTRUN_TEST__, snipTextPlate: false, archiveTag: true,
    // 458: off by default like the real flag -- the dedicated secret-vault probe
    // turns it on to reach the editor's confidential toggle. It also turns
    // autoPaste off there: two clicks close together on the same still-locked
    // tile must each open the password dialog, not be read as the unrelated
    // copy-then-paste double-click gesture.
    secretPrompts: !!window.__SECRET_TEST__,
    ...(window.__SECRET_TEST__ ? { autoPaste: false } : {}),
    // 459: models booting straight into an already-active demo session (what a
    // real restart into the sample library looks like from the window's side).
    demoMode: !!window.__DEMO_TEST__,
  },
  // A probe that needs non-default expert values seeds them before this file
  // loads (window.__SEED_UI_VALUES__ / __SEED_UI_TEXTS__); every other run
  // keeps the empty defaults.
  ui_values: { ...(window.__SEED_UI_VALUES__ || {}) }, ui_texts: { ...(window.__SEED_UI_TEXTS__ || {}) }, dup_ignored: [],
  // 428: p1 also has a canned entry in last_edited_map below, so it is the one
  // fixture with BOTH a copy and an edit timestamp; p2 (untouched) has neither.
  usage: { p1: 3 }, last_used: { p1: 1700000000 },
  example_prompt_ids: [], example_prompt_seeds: {},
  tag_colors: { work: "#3b82f6" },
  custom_themes: [{ id: "ct1", name: "Neon", base: "dark", vars: { accent: "#ff00aa" } }],
  tile_font: "system", tile_size: 0,
  // Point 419: membership lives in `members`, never derived from `layouts` any
  // more - home carries every prompt (matches the fully-packed 4x3 LAYOUT plus
  // the folder members below), the other three views start empty.
  views: [
    // 458: the secret-vault probe's extra "psecret" fixture needs a 13th cell,
    // so its own boot alone grows the fully-packed grid by one row rather than
    // disturbing the 4x3 fixture every other probe relies on.
    (() => {
      const rows = window.__SECRET_TEST__ ? 4 : 3;
      return { id: "home", name: "Home", cols: 4, rows, layouts: { [`4x${rows}`]: LAYOUT }, color: "", members: PROMPTS.map((p) => p.id) };
    })(),
    { id: "work", name: "Work", cols: 4, rows: 3, layouts: {}, color: "#ef4444", members: [] },
    { id: "notes", name: "Notes", cols: 4, rows: 3, layouts: {}, color: "", members: [] },
    { id: "archive", name: "Archive", cols: 4, rows: 3, layouts: {}, color: "", members: [] },
  ],
  // F27: one grid folder on the home view, so the folder cell and its popover are
  // walked on every run. Its members are drawn by the folder, not as tiles. Five
  // members on purpose: the open popover is then taller than a small window leaves,
  // which is the case the popover placement has to get right.
  tile_folders: [{ id: "tf1", name: "Drafts", color: "", view_id: "home", cell: [0, 1],
    members: ["p4", "p6", "p8", "p10", "p11"] }],
  active_view: "home",
};
// A member has no cell of its own while it sits in a folder (mirrors the backend
// dropping it on join); without this, dissolve would look like it worked even with
// a stub that never restored one, because the stale index-based cell was still there.
for (const f of SETTINGS.tile_folders) for (const m of f.members) delete LAYOUT[m];

// A probe that needs a non-default setting at boot (a window that reads its
// values once, like the video editor) sets window.__SEED_SETTINGS__ before this
// file loads; each of its ui_flags / ui_values / ui_texts maps is merged in.
if (window.__SEED_SETTINGS__) {
  for (const k of ["ui_flags", "ui_values", "ui_texts"]) {
    Object.assign(SETTINGS[k], window.__SEED_SETTINGS__[k] || {});
  }
  if (window.__SEED_SETTINGS__.language) SETTINGS.language = window.__SEED_SETTINGS__.language;
}

// 420: the drag-to-view probe needs one view it can fill with a single tile, to
// prove a genuinely full target refuses the whole drop -- shrinking "work" to
// its one and only cell is far cheaper than padding it with eleven throwaway
// placements first. Off by default, so every other run keeps "work" empty.
if (window.__DRAG_VIEW_TEST__) {
  const work = SETTINGS.views.find((v) => v.id === "work");
  if (work) { work.cols = 1; work.rows = 1; }
}

// The Undo time Off (the Quiet preset, or a store that had the old switch off): the
// undo-off probe boots with it so the actions that only an Undo could bring back
// have to ask first.
if (window.__UNDO_OFF_TEST__) SETTINGS.ui_values.undoToastMs = 0;

const clone = () => JSON.parse(JSON.stringify(SETTINGS));

const clipInboxSeed = () => [
  { id: 1, ts: 1750000000, text: "Smoke clip inbox item" },
  { id: 2, ts: 1750000100, text: "Second copy from another app" },
];
let CLIP_INBOX = clipInboxSeed();
window.__resetClipInbox = () => { CLIP_INBOX = clipInboxSeed(); };

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
  // Security pass 348: the folder-authority settings moved off the generic
  // set_ui_text path onto their own dedicated setters, each taking the folder
  // string the pick_folder dialog already returned.
  set_screenshot_dir: (a) => { SETTINGS.ui_texts.screenshotDir = a.dir; return null; },
  set_video_export_dir: (a) => { SETTINGS.ui_texts.videoExportDir = a.dir; return null; },
  set_recording_dir: (a) => { SETTINGS.ui_texts.recordingDir = a.dir; return null; },
  get_settings: () => JSON.parse(JSON.stringify(SETTINGS)),
  get_state: () => ({ prompts: PROMPTS.map(presentPrompt), settings: JSON.parse(JSON.stringify(SETTINGS)) }),
  // 458: the single in-memory session key model -- unlock/lock/status only,
  // never a real Argon2/AES round trip (that is the Rust unit tests' job).
  secret_vault_status: () => ({ unlocked: SECRET_UNLOCKED, configured: SECRET_CONFIGURED }),
  secret_vault_unlock: (a) => {
    const pw = a && a.password;
    if (!pw) throw new Error("password-required");
    if (!SECRET_CONFIGURED) {
      SECRET_PASSWORD = pw;
      SECRET_CONFIGURED = true;
      SECRET_UNLOCKED = true;
      return null;
    }
    if (pw !== SECRET_PASSWORD) throw "wrong-password";
    SECRET_UNLOCKED = true;
    return null;
  },
  set_prompt_secret: (a) => {
    const p = PROMPTS.find((x) => x.id === (a && a.id));
    if (!p) throw new Error("not-found");
    if (a.secret && !SECRET_UNLOCKED) throw "secret-locked";
    p.secret = !!a.secret;
    return null;
  },
  prompt_image: (a) => {
    window.__PROMPT_IMAGE_CALLS__++;
    return STUB_IMAGES.get(a && a.id) || "";
  },
  launcher_items: () => ({
    prompts: PROMPTS.map((p) => ({ id: p.id, name: p.name, text: p.text, color: p.color || "" })),
    language: SETTINGS.language || "",
    usage: JSON.parse(JSON.stringify(SETTINGS.usage || {})),
    last_used: JSON.parse(JSON.stringify(SETTINGS.last_used || {})),
    prefill: window.__LAUNCHER_PREFILL__ || "",
  }),
  current_theme: () => window.__SHOT_THEME__ || "dark",
  theme_vars: () => ({ accent: "#ff00aa" }),
  // A probe sets window.__ATTACH_TEXT_FIXTURES__[path] to {text, truncated,
  // readable} before opening the attachment; an unlisted path reads as
  // unreadable, same as a real missing/binary file would.
  attachment_text_preview: (a) => {
    const fixtures = window.__ATTACH_TEXT_FIXTURES__ || {};
    return fixtures[a && a.path] || { text: "", truncated: false, readable: false };
  },
  // Off by default (every existing pill/folder probe boots unparked);
  // window.__FLOAT_PARKED__ lets a probe flip a specific boot to true.
  is_float_parked: () => window.__FLOAT_PARKED__ === true,
  float_recall: () => null,
  // Recording control bar's poll target -- a fixed fixture so the probe can
  // assert the timer/size labels render from it.
  // show_size/size_eta_secs mirror what rec_bar_size_state would compute
  // server side; __REC_STATUS_OVERRIDE__ lets a probe swap in a different
  // reading (e.g. past the size ceiling) between polls.
  recording_status: () => Object.assign(
    { paused: false, elapsed_ms: 65000, bytes: 600000000, show_size: true, size_eta_secs: null, target_switch_enabled: false },
    window.__REC_STATUS_OVERRIDE__ || {},
  ),
  pause_recording: () => { window.__REC_PAUSED__ = true; return null; },
  resume_recording: () => { window.__REC_PAUSED__ = false; return null; },
  stop_recording: () => { window.__REC_STOPPED__ = true; return null; },
  cancel_recording: () => { window.__REC_CANCELLED__ = true; return null; },
  hide_rec_bar: () => { window.__REC_BAR_HIDDEN__ = true; return null; },
  open_recorder_switch: () => null,
  switch_recording_target: () => null,
  finish_and_continue_recording: () => {
    window.__REC_CONTINUED__ = true;
    return {
      finished_part_path: "C:\\Recordings\\ps-smoke-recording.mp4",
      finished_part_dropped_frames: 0,
      finished_part_encode_error: null,
      new_path: "C:\\Recordings\\ps-smoke-recording_part2.mp4",
      part_number: 2,
      gap_ms: 140,
    };
  },
  // Video editor: a fixed 10-minute 1080p30 recording with both a system and a
  // mic audio track -- real enough to exercise the filmstrip/waveform rendering
  // the same way an actual capture would, and shared by the UI smoke test and
  // the perf hot-path timing (both want the same non-degenerate fixture).
  // mic_sidecar stays empty so the boot never also has to route through
  // mux_mic_track, which nothing here stubs.
  video_editor_source: () => ({ path: "C:\\Recordings\\ps-smoke-recording.mp4", origin_dir: "" }),
  windows_fonts_dir: () => "C:/Windows/Fonts",
  // 334: the image-overlay "Add" button picks a file before it can add its
  // op -- a fixed fake path is enough to exercise the add flow and the live
  // preview's <img> element; the path never actually needs to resolve.
  pick_file_path: () => "C:\\Pictures\\ps-smoke-badge.png",
  // Right-click the screenshot/record button (or a library attachment) to
  // open an existing file in whichever editor its content calls for.
  // window.__OPEN_EDIT_PICK__ lets a probe steer which path the "pick a
  // file" dialog answers with; window.__OPEN_EDIT_RESULT__ lets a probe
  // steer open_file_in_editor's own answer (image vs video vs a rejection).
  pick_media_file_path: () => window.__OPEN_EDIT_PICK__ || "C:\\Pictures\\ps-smoke-openedit.mp4",
  open_file_in_editor: () => {
    if (window.__OPEN_EDIT_RESULT__) return window.__OPEN_EDIT_RESULT__;
    return {
      kind: "video",
      copied_path: "C:\\Temp\\ps-smoke-openedit-copy.mp4",
      origin_dir: "C:\\Pictures",
      name: "ps-smoke-openedit",
    };
  },
  open_video_editor: () => null,
  // 335: export-destination pickers for the video editor's export row.
  // A probe that needs the picker to answer with a specific folder sets
  // window.__PICK_FOLDER__ first; everything else gets the export folder.
  pick_folder: () => window.__PICK_FOLDER__ || "C:\\Videos\\Exports",
  // The export-only folder picker (video editor "choose folder", the
  // batch-export dialog's "choose destination") -- same fixture as
  // pick_folder above, since a probe driving either flow sets the same
  // window.__PICK_FOLDER__ before clicking the button.
  pick_export_folder: () => window.__PICK_FOLDER__ || "C:\\Videos\\Exports",
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
  // The same modal's own discard-trash section -- window.__DISCARDED_FIXTURE__
  // lets one probe step ask for an empty list (default) so the section stays
  // hidden, without changing it for every other list_discarded_recordings call.
  list_discarded_recordings: () => window.__DISCARDED_FIXTURE__ || [],
  restore_discarded_recording: () => true,
  delete_discarded_recording_now: () => true,
  // The video-cleanup modal's "Export selected" batch flow -- one
  // duration per file, no disk-space warning unless a probe sets
  // window.__EXPORT_DISK_WARN__, and the batch call itself recorded on
  // window.__BATCH_EXPORT_CALLED__ so a probe can assert its items.
  video_folder_file_durations_ms: (a) => (a.names || []).map(() => 5000),
  check_export_disk_space_batch: () => window.__EXPORT_DISK_WARN__ || null,
  export_recordings_batch: (a) => { window.__BATCH_EXPORT_CALLED__ = a; return { ok: (a.items || []).length, failed: 0 }; },
  // 527: has_keys_sidecar/read_key_sidecar let a probe exercise the "keys"
  // overlay end to end instead of only ever hitting its veOvKeysNoData
  // branch -- window.__KEY_SIDECAR__ (default: none) is the fixture read_key_sidecar
  // hands back, a plain {hold_ms, events} object shaped like keylog::KeySidecar.
  probe_recording: () => ({
    duration_ms: 600000, fps: 30, width: window.__PROBE_W__ || 1920, height: window.__PROBE_H__ || 1080,
    has_audio: true, has_mic_track: true, mic_sidecar: "",
    has_keys_sidecar: !!window.__KEY_SIDECAR__,
  }),
  read_key_sidecar: () => window.__KEY_SIDECAR__ || null,
  // 416: the "Add a video" flow's own ffprobe check. window.__EXTRA_VIDEO_FAIL__
  // (default: unset) lets one probe step exercise the rejection path -- the
  // same JSON envelope shape a real ffmpeg-error rejection uses -- without
  // changing every other call's success fixture.
  probe_extra_video_source: () => {
    if (window.__EXTRA_VIDEO_FAIL__) {
      throw JSON.stringify({ key: "ffErrUnreadable", code: null, raw: "" });
    }
    return window.__EXTRA_VIDEO_PROBE__ || {
      duration_ms: 4000, fps: 30, width: 640, height: 360,
      has_audio: true, has_mic_track: false, mic_sidecar: "",
    };
  },
  // A fixed (size, mtime) tuple so a probe can control whether it "matches" a
  // window.__EDIT_STATE_FIXTURE__ saved under the same numbers.
  stat_file: () => [4096, 1700000000],
  // window.__EDIT_STATE_FIXTURE__ (default: nothing saved) lets one probe step
  // ask for a matching saved state without changing it for every other call;
  // __EDIT_STATE_SAVED__/__EDIT_STATE_DISCARDED__ record what the editor did,
  // same pattern as __VIDEO_CLEANUP_CALLED__ above.
  load_edit_state: () => (window.__EDIT_STATE_FIXTURE__ !== undefined ? window.__EDIT_STATE_FIXTURE__ : null),
  save_edit_state: (a) => { window.__EDIT_STATE_SAVED__ = a; return null; },
  discard_edit_state: (a) => { window.__EDIT_STATE_DISCARDED__ = a; return null; },
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
    // 399: same classified-failure simulation as export_video below, rejecting
    // with the plain JSON string a real Result<_, String> command hands back.
    if (a && a.srcPath && a.srcPath.includes("FAILCLASSIFY")) {
      return Promise.reject(JSON.stringify({ key: "ffErrNoSpace", code: null, raw: "out.mp4: No space left on device" }));
    }
    const dst = a.srcPath.replace(/\.[a-z0-9]+$/i, "." + a.format);
    return new Promise((resolve) => setTimeout(() => resolve(dst), 80));
  },
  cancel_recording_convert: () => null,
  // 386/387: the export progress row and its remaining-time estimate both need
  // export_video to stay pending for a moment, the same way convert_recording
  // does above, so the probe can fire progress events while VE.exporting is
  // still true. window.__EXPORT_DELAY_MS__ lets one probe step ask for a
  // longer pending window than the default, without changing it for every
  // other export_video call in the suite.
  // 399: a destination path containing "FAILCLASSIFY" simulates a real
  // classified ffmpeg failure -- the exact JSON shape ffmpeg_error_json()
  // produces in ffmpeg.rs, so the probe can check the frontend never shows this
  // raw text as the headline. Rejecting with a plain string (not an Error
  // object) matches what a real Tauri Result<_, String> command actually
  // hands back to JS.
  // 503: a destination path containing "REFUSEDEST" simulates the backend's
  // purpose-bound export gate refusing a source it does not recognize (e.g.
  // opened from outside the app) -- rejected exactly once, since the
  // frontend's own retry (picking a new folder through pick_export_folder)
  // must not loop forever. window.__EXPORT_DEST_REFUSED_ONCE__ tracks whether
  // this probe run has already spent its one refusal.
  export_video: (a) => {
    const destPath = (a && a.req && a.req.dest_path) || "";
    if (destPath.includes("FAILCLASSIFY")) {
      return Promise.reject(JSON.stringify({ key: "ffErrNoSpace", code: null, raw: "out.mp4: No space left on device" }));
    }
    if (destPath.includes("REFUSEDEST") && !window.__EXPORT_DEST_REFUSED_ONCE__) {
      window.__EXPORT_DEST_REFUSED_ONCE__ = true;
      return Promise.reject("export destination not recognized");
    }
    window.__EXPORT_VIDEO_LAST_DEST__ = destPath;
    return new Promise((resolve) => setTimeout(() => resolve(null), window.__EXPORT_DELAY_MS__ || 80));
  },
  // 412: mirrors the shape of the real Rust estimate_export_bytes (a plain
  // byte figure for most formats, a low/high bracket for the three silent
  // image formats) closely enough for the probe to check the note text
  // changes and carries a size -- the exact numbers are Rust's own test's job.
  estimate_export_size: (a) => {
    const secs = (a && a.durationMs) / 1000 || 0;
    if (["gif"].includes(a && a.format)) {
      return { bytes: null, low_bytes: Math.round(secs * 3750), high_bytes: Math.round(secs * 37500) };
    }
    const kbps = ((a && a.videoKbps) || 8000) + (a && a.losslessQuality ? 320 : 192);
    return { bytes: Math.round((kbps * 1000) / 8 * secs), low_bytes: null, high_bytes: null };
  },
  set_taskbar_progress: () => null,
  save_recording_as: (a) => { window.__REC_SAVED_AS__ = a.srcPath; return "C:/fake/saved.mp4"; },
  // A number back (the retention hours), not a bare true/false, since
  // discard now moves the file into "verworfen" instead of deleting it.
  discard_recording_file: (a) => { window.__REC_DISCARDED__ = a.path; return 24; },
  // P100: the probe reads this back instead of a real backend hand-off to the main window.
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
    searchOn: true,
    searchPh: "Search",
  }),
  tray_window_activate: (a) => { window.__TRAY_ACTIVATED__ = a && a.id; return null; },
  tray_window_close: () => { window.__TRAY_CLOSED__ = true; return null; },
  // The filter row hands what was typed to the launcher; the probe reads it back.
  tray_open_launcher: (a) => { window.__TRAY_LAUNCHER_QUERY__ = a && a.query; window.__TRAY_LAUNCHER_CALLS__ = (window.__TRAY_LAUNCHER_CALLS__ || 0) + 1; return null; },
  // Text typed in the tray after the hand-off started.
  tray_launcher_prefill: (a) => { window.__TRAY_LAUNCHER_PREFILL__ = a && a.query; return null; },
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
  // 76: simulates picking a file and it parsing into a new saved theme, same
  // shape save_custom_theme returns, so the caller's refresh path is identical.
  import_custom_theme: () => {
    const id = "ct" + (SETTINGS.custom_themes.length + 1);
    const theme = { id, name: "Imported theme", base: "dark", vars: { bg: "#0a0a0a" } };
    SETTINGS.custom_themes.push(theme);
    SETTINGS.theme = "custom:" + id;
    return JSON.parse(JSON.stringify(SETTINGS));
  },
  // image_text never reaches the frontend, so this is the only way a search
  // hit inside a picture's recognized text can ever come back -- stubbed to
  // answer one fixed query with a fixture id nothing's name or body contains,
  // so a probe can prove the badge without a real OCR engine.
  search_image_text: (a) => (a && String(a.query || "").toLowerCase() === "lighthouse") ? ["p4"] : [],
  app_version: () => "0.0.0",
  current_data_dir: () => "C:\\Data",
  // Sealed unless a page says otherwise, so the default walk sees the healthy
  // app. The seal probe sets window.__STORE_SEALED__ = false before boot; read
  // fresh on every call for the same reason ocr_capabilities is.
  store_sealed: () => window.__STORE_SEALED__ !== false,
  // Same fresh-read idea as store_sealed, for the write-fail probe below.
  settings_write_failed: () => window.__SETTINGS_WRITE_FAILED__ === true,
  // Read-only unless a probe sets window.__DATA_DIR_READONLY__ = true before
  // boot, for the read-only-banner probe below. Read fresh for the same
  // reason as store_sealed.
  data_dir_lock_status: () => ({
    readOnly: window.__DATA_DIR_READONLY__ === true,
    otherHost: window.__DATA_DIR_OTHER_HOST__ || null,
  }),
  current_data_dir_sync_hint: () => window.__DATA_DIR_SYNC_HINT__ || null,
  path_sync_folder_hint: () => window.__DATA_DIR_SYNC_HINT__ || null,
  // 122: a probe sets window.__OCR_CAPS_TEST__ before opening #ocr-langs-modal.
  // Read fresh on every call (unlike core.js's own `const { invoke } = ...`
  // capture at module load), so a probe can set it any time before triggering
  // the actual invoke call, no matter how long the main window has already been running.
  ocr_capabilities: () => {
    // A probe sets window.__OCR_CAPS_ELEV__ to play the Windows builds that
    // refuse even the read-only listing to a normal user. 177: the real
    // backend error is "code|raw text", the raw half varying by machine.
    if (window.__OCR_CAPS_ELEV__) throw "ocr-caps-need-elevation|Access to the path \"C:\\WINDOWS\\Logs\\DISM\" is denied.";
    return window.__OCR_CAPS_TEST__ || [];
  },
  // The elevated listing: launching it arms the result, the next poll hands the
  // list over -- the same two-step the real backend does through the UAC prompt.
  // A probe sets window.__OCR_ELEV_ERROR__ to "ocr-elevate-no-rights" or
  // "ocr-elevate-cancelled" to play the two ways the elevation itself can fail
  // (378), instead of the launch succeeding.
  ocr_capabilities_elevate: () => {
    if (window.__OCR_ELEV_ERROR__) throw window.__OCR_ELEV_ERROR__;
    window.__OCR_CAPS_ELEV_DONE__ = true;
    return null;
  },
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
  set_rec_stop_hotkey: (a) => {
    SETTINGS.rec_stop_hotkey = a && a.hotkey ? a.hotkey : "";
    return null;
  },
  set_rec_pause_hotkey: (a) => {
    SETTINGS.rec_pause_hotkey = a && a.hotkey ? a.hotkey : "";
    return null;
  },
  default_screenshot_dir: () => "C:\\Pictures",
  default_sync_dir: () => "C:\\Data\\prompt-saver-sync",
  // The old sync folder holds three files this app wrote and nothing else, so
  // switching folders asks its three-way question instead of going straight
  // through.
  sync_dir_info: () => ({ path: "C:\\Data\\prompt-saver-sync", ours: 3, foreign: 0 }),
  sync_status: () => ({ enabled: true, last_ok: Math.floor(Date.now() / 1000) - 60, files: 3, paused: false, reason: "" }),
  // 503: window.__SYNC_DIR_REFUSE__ lets a probe simulate the backend
  // refusing the switch outright (a folder that was never actually picked
  // through the dialog) -- the frontend must then show the picked folder as
  // never having taken effect, not a misleading "old folder kept" dialog.
  set_sync_dir: (a) => {
    if (window.__SYNC_DIR_REFUSE__) return Promise.reject("dir-not-picked");
    window.__SYNC_DIR_CALL__ = { dir: a ? a.dir : null, deleteOld: a ? a.deleteOld : null };
    return 0;
  },
  // One snapshot carries its prompt count and names (61), one is from an older
  // build and has neither: the list has to show both without a gap in the row.
  open_backups_folder: () => null,
  // 17: p3 edited most recently, then p1, then p9: everything else has no
  // version row (never content-edited, or history was off) and sorts last.
  // None of these three are favorited (favorite: i % 5 === 0); favorites float
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
  // 446: a folder import picks a folder, then previews every .md/.txt file
  // found in it -- one plain note, one in a tagged subfolder, one too big to read.
  preview_import_folder: () => ({
    dir: "C:\\fake\\notes",
    truncated: false,
    items: [
      { rel_path: "Idea.txt", name: "Idea", tags: [], size: 42, readable: true, skip_reason: "" },
      { rel_path: "Work\\Meeting.md", name: "Meeting", tags: ["Work"], size: 128, readable: true, skip_reason: "" },
      { rel_path: "huge.txt", name: "huge", tags: [], size: 99999999, readable: false, skip_reason: "too-large" },
    ],
  }),
  import_folder_files: (a) => {
    if (!a || !Array.isArray(a.relPaths)) throw new Error("import_folder_files stub called without relPaths");
    return a.relPaths.length;
  },
  list_backups: () => [
    { name: "2026-07-25_18-30-00", size: 812345, prompts: 12, items: [
      { name: "Draft reply", chars: 214, kind: "text" },
      { name: "Meeting notes", chars: 0, kind: "text" }, // 103: legacy-cache stand-in (chars unknown -> "-")
    ] },
    { name: "2026-07-24_09-05-00", size: 654321, prompts: null, items: [] },
  ],
  // Three entries so search narrows the list and sorting by name actually
  // moves a row: alphabetically Apple, Draft, Meeting, but Meeting was never
  // edited (date null) and must sort to the end regardless of direction.
  backup_contents: () => ({
    entries: 3, views: 1, tags: 2, last_change: 1700000000,
    program_version: "2.6.0", encrypted: true,
    items: [
      { id: "p1", name: "Draft reply", date: 1700000000, chars: 214, kind: "text" },
      { id: "p2", name: "Meeting notes", date: null, chars: 0, kind: "text" },
      { id: "p3", name: "Apple pie recipe", date: 1690000000, chars: 512, kind: "text" },
    ],
  }),
  recent_copies: () => PROMPTS.slice(0, 4).map((p, i) => ({ id: p.id, ts: 1750000000 + i, rowid: i + 1 })),
  restore_copy_history: () => null,
  delete_copy_entry: () => true,
  // 2: the fixture library is never empty, so the real seeding never actually
  // triggers (matches the backend's own "already has prompts" guard); the mock
  // just needs to exist so the call is tracked, not error out.
  seed_example_prompts: () => [],
  example_prompts_pending: () => examplePristineIds().length,
  dismiss_example_prompts: () => {
    SETTINGS.example_prompt_ids = [];
    SETTINGS.example_prompt_seeds = {};
    return null;
  },
  // Two stored copies from other apps, so the copy history has real entries to list as
  // "copied elsewhere", to dismiss one of, and to snapshot and replay on a clear.
  // Clear and restore stay no-ops so every later check still finds both.
  clip_inbox_list: () => CLIP_INBOX.map((x) => ({ ...x })),
  clip_inbox_dismiss: (a) => { CLIP_INBOX = CLIP_INBOX.filter((x) => x.id !== (a && a.id)); return null; },
  restore_clip_inbox: () => null,
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
  // something to measure: an empty list only ever proved the "none found" line.
  find_duplicates: () => [{
    score: 0.94,
    members: [
      { id: "p0", name: PROMPTS[0].name, text: PROMPTS[0].text, usage: 12, views: ["Home"] },
      { id: "p1", name: PROMPTS[1].name, text: PROMPTS[1].text, usage: 3, views: ["Home", "Work"] },
    ],
  }],
  // window.__SIMILAR_HINT_FIXTURE__ lets one probe opt into a hit; every other
  // save the sweep makes along the way (there are many) must see nothing, or
  // its own toast assertions elsewhere would break.
  similar_to_saved: () => window.__SIMILAR_HINT_FIXTURE__
    ? { id: window.__SIMILAR_HINT_FIXTURE__.id, name: window.__SIMILAR_HINT_FIXTURE__.name }
    : null,
  // 443: window.__EDITOR_DRAFT_FIXTURE__ lets one probe supply a crash draft
  // for the boot-time recovery question; every other boot must see none, so
  // the ordinary startup chain never shows the restore dialog by surprise.
  load_editor_draft: () => window.__EDITOR_DRAFT_FIXTURE__ || null,
  save_editor_draft: () => null,
  discard_editor_draft: () => null,
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
  // 404: nothing new by default, so an unrelated probe boot never gets a
  // surprise startup toast sitting over its own buttons.
  // window.__WHATS_NEW_FIXTURE__ lets one probe supply a real range.
  whats_new_check: () => window.__WHATS_NEW_FIXTURE__ || { since: null, sections: [] },
  whats_new_current: () => ([
    { version: "1.2.0", markdown: "## Prompt Saver 1.2.0\n\nCurrent version only." },
  ]),
  whats_new_refresh: () => ([
    { version: "1.2.0", markdown: "## Prompt Saver 1.2.0\n\nRefreshed from GitHub." },
  ]),
  // 451: a small stand-in for the real, build-time generated list (src-tauri/build.rs) --
  // one crate resolved to a bundled license text, one manual entry with a note and a
  // text, and one with a note but no text (a public-domain style entry) so the "never a
  // blank panel" fallback in the main window has something to cover.
  third_party_notices: () => JSON.stringify({
    generatedFromCrateCount: 1,
    entries: [
      { name: "examplecrate", version: "1.0.0", license: "MIT OR Apache-2.0", licenseId: "MIT", note: null },
      { name: "FFmpeg", version: "9.0.1", license: "GPL-3.0-or-later", licenseId: "GPL-3.0-or-later", note: "Runs as a separate subprocess." },
      { name: "SQLite", version: "bundled", license: "Public Domain", licenseId: null, note: null },
    ],
    licenseTexts: {
      MIT: "MIT License\n\nPermission is hereby granted...",
      "GPL-3.0-or-later": "GNU General Public License, version 3 or later (summary)...",
    },
  }),
  // The FULL prompt, image bytes included: the editor's one fetch when it opens.
  get_prompt: (a) => {
    const p = PROMPTS.find((x) => x.id === (a && a.id)) || PROMPTS[0];
    if (p.secret && !SECRET_UNLOCKED) throw "secret-locked";
    return { ...p, image: STUB_IMAGES.get(p.id) || "" };
  },
  // 408: "Save as new entry" -- looks the version id up against the same P31/79
  // fixtures list_versions already hands out, then pushes a genuinely new prompt
  // onto the shared PROMPTS array so a following get_state actually shows it.
  restore_version_as_new: (a) => {
    const byId = {
      1: { pid: "p6", name: "Prompt 7", text: "Example draft text number 7.", variants: [] },
      2: { pid: "p6", name: "Prompt 7", text: "Example draft body number 7.", variants: [] },
      3: { pid: "p10", name: "Prompt eleven (old name)", text: "Example prompt body number 11, first entry.", variants: [] },
      4: {
        pid: "p10", name: "Prompt 11",
        text: "Example prompt body number 11, second entry.",
        variants: ["Example prompt body number 11, third entry."],
      },
    };
    const v = byId[a && a.versionId];
    const source = v && PROMPTS.find((p) => p.id === v.pid);
    if (!v || !source) return null;
    const suffix = (a && a.suffix) || "";
    const created = {
      ...source,
      id: "restored-" + a.versionId,
      name: suffix ? v.name + " " + suffix : v.name,
      text: v.text,
      variants: v.variants,
      favorite: false,
      tags: [],
      notes: "",
      hotkey: "",
      hotkey_paste: "",
    };
    PROMPTS.push(created);
    return created;
  },
  // 91: trash: restore/purge mutate the shared TRASH/PROMPTS fixtures so the dialog
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
  // Undo snapshot taken before a batch delete, and the one-call restore that uses it.
  placement_snapshot: (a) => ((a && a.ids) || []).map((id) => ({ id, views: [], floating: null, float_scale: null, video_prefs: null, tile_folder: null, float_folder: null })),
  restore_prompts_batch: (a) => {
    for (const pl of (a && a.placements) || []) {
      const idx = TRASH.findIndex((p) => p.id === pl.id);
      if (idx >= 0) {
        const [p] = TRASH.splice(idx, 1);
        p.trashed_at = 0;
        PROMPTS.push(p);
      }
    }
    return { prompts: JSON.parse(JSON.stringify(PROMPTS)), settings: JSON.parse(JSON.stringify(SETTINGS)) };
  },
  purge_prompt: (a) => {
    const idx = TRASH.findIndex((p) => p.id === (a && a.id));
    if (idx >= 0) TRASH.splice(idx, 1);
    return JSON.parse(JSON.stringify(TRASH));
  },
  empty_trash: () => { TRASH.length = 0; return []; },
  // 439: recent-messages ring buffer. Mirrors the real command's "off means
  // nothing is collected" behaviour off the same ui_flags/ui_values SETTINGS
  // set_ui_flag/set_ui_value already write to.
  log_error: (a) => {
    if (SETTINGS.ui_flags.errorLog === false) return null;
    ERROR_LOG.unshift({ ts: Math.floor(Date.now() / 1000), text: (a && a.text) || "", origin: (a && a.origin) || "", raw: (a && a.raw) || null });
    const max = 100;
    ERROR_LOG.length = Math.min(ERROR_LOG.length, max);
    return null;
  },
  list_error_log: () => JSON.parse(JSON.stringify(ERROR_LOG)),
  clear_error_log: () => { ERROR_LOG.length = 0; return null; },
  // F8/P11: shared by the library's and the grid's bulk toolbars. The caller
  // (doBatch/doGridBatch) already mirrors "delete"/"color" onto its own local
  // prompts array, so this only needs to track the call and hand back settings.
  batch_prompts: (a) => {
    const ids = (a && a.ids) || [];
    if (a && a.action === "color") {
      for (const id of ids) { const p = PROMPTS.find((x) => x.id === id); if (p) p.color = a.color || ""; }
    }
    // Tag actions touch the prompts' own tag lists, the way the backend does.
    if (a && (a.action === "tag-add" || a.action === "tag-remove")) {
      const key = String(a.tag || "").trim().toLowerCase();
      for (const id of ids) {
        const p = PROMPTS.find((x) => x.id === id);
        if (!p) continue;
        const has = (p.tags || []).some((t) => t.toLowerCase() === key);
        if (a.action === "tag-add" && !has) p.tags = [...(p.tags || []), String(a.tag).trim()];
        if (a.action === "tag-remove") p.tags = (p.tags || []).filter((t) => t.toLowerCase() !== key);
      }
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
          const need = ids.filter((id) => !(id in layout));
          const free = view.cols * view.rows - Object.keys(layout).length;
          if (need.length > free) throw new Error("view-full:" + (need.length - free));
          for (const id of ids) {
            if (id in layout) continue;
            const { cell } = firstFreeCell(view);
            if (cell) layout[id] = cell;
          }
        }
      }
    }
    // Dragging a tile onto a view tab moves it there in one call -- mirrors the
    // real backend's single-write moveView: place into the target (all-or-nothing,
    // same capacity check addView uses above), only then drop it from the source.
    if (a && a.action === "moveView") {
      const to = SETTINGS.views.find((v) => v.id === a.viewId);
      const from = SETTINGS.views.find((v) => v.id === a.fromViewId);
      if (to) {
        const toKey = to.cols + "x" + to.rows;
        const toLayout = to.layouts[toKey] || (to.layouts[toKey] = {});
        const need = ids.filter((id) => !(id in toLayout));
        const free = to.cols * to.rows - Object.keys(toLayout).length;
        if (need.length > free) throw new Error("view-full:" + (need.length - free));
        for (const id of ids) {
          if (id in toLayout) continue;
          const { cell } = firstFreeCell(to);
          if (cell) toLayout[id] = cell;
        }
        if (from && from !== to) {
          for (const layout of Object.values(from.layouts)) {
            for (const id of ids) delete layout[id];
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
  // save step of their own); mirrors the real backend's "hands back what it
  // really stored" contract closely enough for the wiring the smoke test proves.
  update_prompt: (a) => {
    const p = PROMPTS.find((x) => x.id === (a && a.id));
    if (!p) return null;
    if (a.hotkey !== undefined) p.hotkey = a.hotkey;
    if (a.hotkeyPaste !== undefined) p.hotkey_paste = a.hotkeyPaste;
    // 410: the free compare view's "take" actions go through this same command --
    // mirrors the real backend, which always overwrites name/text/colour/variants/
    // rotate mode and only touches image/show/copy when the caller sends them.
    if (a.name !== undefined) p.name = a.name;
    if (a.text !== undefined) p.text = a.text;
    if (a.color !== undefined) p.color = a.color;
    if (a.variants !== undefined) p.variants = a.variants;
    if (a.rotateMode !== undefined) p.rotate_mode = a.rotateMode;
    if (a.rotateAuto !== undefined) p.rotate_auto = a.rotateAuto;
    if (a.filePath !== undefined) p.file_path = a.filePath;
    if (a.image !== undefined) p.image = a.image;
    if (a.showImage !== undefined) p.show_image = a.showImage;
    if (a.copyImage !== undefined) p.copy_image = a.copyImage;
    return p;
  },
  // Persists the active view's arrangement so a later get_state shows it, like the
  // backend does; without this every re-fetch snapped a moved tile back.
  set_layout: (a) => {
    const view = SETTINGS.views.find((v) => v.id === SETTINGS.active_view);
    if (view && a && a.layout) view.layouts[view.cols + "x" + view.rows] = JSON.parse(JSON.stringify(a.layout));
    return null;
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
  // 440: a pure reorder of SETTINGS.views by id -- mirrors reorder_views_by_id.
  // An id missing from `order` keeps its old relative place instead of vanishing.
  reorder_views: (a) => {
    const order = (a && a.order) || [];
    const byId = new Map(SETTINGS.views.map((v) => [v.id, v]));
    const out = [];
    for (const id of order) {
      const v = byId.get(id);
      if (v) { out.push(v); byId.delete(id); }
    }
    for (const v of SETTINGS.views) { if (byId.has(v.id)) out.push(v); }
    SETTINGS.views = out;
    return clone();
  },
  // Mirror of the backend's rebuild_view_layout: membership never
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
  // F27 grid folders: same answers as the backend: the settings blob, invariants
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
      // Other folders still on this view keep their own cell: a dissolved member
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
  // not re-proven here); enough to exercise the peek→copy→commit wiring for real.
  copy_prompt: (a) => {
    const p = PROMPTS.find((x) => x.id === (a && a.id));
    if (p && p.secret && !SECRET_UNLOCKED) throw "secret-locked";
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
    // is a backend concern, proven in Rust unit tests); pos = idx + 1 mirrors what
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
  takeover_offer: () => ({ path: "C:\\Users\\Example\\AppData\\Roaming\\Example-Store", size: 812345, prompts: 7 }),
  takeover_apply: () => null,
  pick_backup_file: () => "C:\\Backups\\Backup 2026-07-25.psb",
  // Refuses until a password is supplied: drives the retry prompt.
  import_all: (a) => {
    if (!a || !a.password) throw new Error("password-required");
    if (a.password !== "letmein") throw new Error("wrong password or corrupt backup");
    return { name: "Backup 2026-07-25.psb", count: 12, dropped_hotkeys: 2 };
  },
  focus_main_window: () => null,
  // 19: the picker's right-click history popover: a fixed, non-empty list so
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
  var_window_fit: () => null,
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
    // 58: 14 days, newest last: a couple of zero days mixed in like a real history.
    daily: Array.from({ length: 14 }, (_, i) => ({
      ts: 1750000000 - (13 - i) * 86400,
      count: [0, 3, 1, 0, 5, 2, 4, 0, 1, 6, 2, 3, 1, 4][i],
    })),
    history_on: true,
    tools: [
      { name: "screenshot", count: 14, last_used: 1750000000 },
      { name: "ocr", count: 3, last_used: 1749900000 },
    ],
    tools_never_used: ["chain", "duplicate_finder"],
  }),
  export_stats: () => 12,
  storage_breakdown: () => ({
    items: [
      { key: "database", path: "C:\\data\\data.db", bytes: 2500000, count: 2, capped: false, denied: false },
      { key: "attachments", path: "", bytes: 800000, count: 3, capped: false, denied: false },
      { key: "screenshots", path: "C:\\Pictures\\Screenshots", bytes: 1200000, count: 4, capped: false, denied: false },
      { key: "recordings", path: "C:\\Videos\\Prompt Saver", bytes: 5000000, count: 2, capped: false, denied: false },
      { key: "discarded_recordings", path: "C:\\Videos\\Prompt Saver\\verworfen", bytes: 0, count: 0, capped: false, denied: false },
      { key: "backups", path: "C:\\data\\backups", bytes: 3000000, count: 5, capped: false, denied: false },
      { key: "temp_files", path: "C:\\Temp\\prompt-saver", bytes: 40000, count: 1, capped: false, denied: false },
      { key: "edit_states", path: "", bytes: 15000, count: 2, capped: false, denied: false },
      { key: "crash_log", path: "C:\\data\\crash.log", bytes: 0, count: 0, capped: false, denied: false },
    ],
    total_bytes: 12555000,
    measured_at: 1750000000,
  }),
  open_storage_folder: () => null,
  retention_overview: () => ({
    items: [
      { key: "trash", count: 3, bytes: 12000 },
      { key: "copyHistory", count: 40, bytes: 400 },
      { key: "clipInbox", count: 1, bytes: 20 },
      { key: "recordings", count: 2, bytes: 5000000 },
      { key: "discardedRecordings", count: 0, bytes: 0 },
      { key: "tempExports", count: 1, bytes: 40000 },
      { key: "editStates", count: 2, bytes: 15000 },
    ],
    measured_at: 1750000000,
  }),
  sweep_trash_now: () => [3, 12000],
  trim_copy_history_now: () => [40, 400],
  sweep_clip_export_temp_now: () => [1, 40000],
  sweep_edit_states_now: () => 2,
  sweep_discarded_recordings_now: () => [0, 0],
};

// What the main window sends to vars.html: one free-text row with the remember switch and
// one choice row, so the small window renders both shapes.
const VAR_REQUEST = {
  theme: "dark",
  vars: { accent: "#ff00aa" },
  lang: "en",
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
// the only way to exercise a code path the backend normally starts: e.g. a prompt
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
      // Runs the handler (mutating the shared fixtures) in real call order right
      // away, same as the backend would under its own mutex -- only handing the
      // OUTCOME back to the caller is what a probe can reorder or delay below, so
      // a "response B arrives before response A" race can be built on purpose.
      let outcome;
      try {
        outcome = { ok: true, value: fn ? fn(args) : null };
      } catch (e) {
        outcome = { ok: false, error: e };
      }
      if (cmd === "batch_prompts" && Array.isArray(window.__BATCH_PROMPTS_GATE__)) {
        await new Promise((resolve) => window.__BATCH_PROMPTS_GATE__.push(resolve));
      }
      if (!outcome.ok) throw outcome.error;
      return outcome.value;
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
