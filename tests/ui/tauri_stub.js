// Minimal window.__TAURI__ stub so the real UI can boot in a plain browser.
// Every backend call is answered with plausible, empty-ish data; anything not
// listed resolves to null. Used by smoke_test.py.
window.__SMOKE_ERRORS__ = [];
window.addEventListener("error", (e) => window.__SMOKE_ERRORS__.push("error: " + (e.error && e.error.stack || e.message)));
window.addEventListener("unhandledrejection", (e) =>
  window.__SMOKE_ERRORS__.push("rejection: " + (e.reason && (e.reason.stack || e.reason.message) || e.reason)));

const PROMPTS = Array.from({ length: 12 }, (_, i) => ({
  id: "p" + i,
  name: "Prompt " + (i + 1),
  // One prompt carries a placeholder so the fill-in dialog gets exercised too.
  text: i === 1
    ? "Translate {#{Text}#} into {#{Language}#}."
    : "Example prompt body number " + (i + 1) + ".",
  color: ["#3b82f6", "#ef4444", "#10b981", "#f59e0b"][i % 4],
  favorite: i % 5 === 0,
}));

const LAYOUT = {};
PROMPTS.forEach((p, i) => { LAYOUT[p.id] = [i % 4, Math.floor(i / 4)]; });

const SETTINGS = {
  theme: "dark", language: "en", floating: {}, float_scale: {}, video_prefs: {},
  window: null, minimize_to_tray: false, autostart: false, start_minimized: false,
  always_on_top: false, hotkey: "", auto_update: true, skipped_versions: [],
  ui_flags: {}, ui_values: {}, ui_texts: {}, dup_ignored: [],
  tile_font: "system", tile_size: 0,
  views: [{ id: "home", name: "Home", cols: 4, rows: 3, layouts: { "4x3": LAYOUT }, color: "" }],
  active_view: "home",
};

const RESULTS = {
  get_settings: () => JSON.parse(JSON.stringify(SETTINGS)),
  get_state: () => ({ prompts: PROMPTS, settings: JSON.parse(JSON.stringify(SETTINGS)) }),
  current_theme: () => "dark",
  app_version: () => "0.0.0",
  current_data_dir: () => "C:\\Data",
  default_screenshot_dir: () => "C:\\Pictures",
  list_backups: () => [],
  recent_copies: () => PROMPTS.slice(0, 4).map((p, i) => ({ id: p.id, ts: 1750000000 + i })),
  clip_inbox_list: () => [],
  list_versions: () => [],
  missing_files: () => [],
  has_backup_password: () => false,
  find_duplicates: () => [],
  check_update: () => null,
  get_prompt: (a) => PROMPTS.find((p) => p.id === (a && a.id)) || PROMPTS[0],
  // The copy commands report success; the UI skips everything that follows a copy
  // (feedback, history, auto-paste) when they come back falsy.
  copy_prompt: () => true,
  copy_text: () => true,
  // Always offer the portable take-over so the dialog is exercised on every run.
  takeover_offer: () => "C:\\Users\\Example\\AppData\\Roaming\\Prompt-Saver",
  takeover_apply: () => null,
  pick_backup_file: () => "C:\\Backups\\Prompt Saver 2026-07-25.psb",
  // Refuses until a password is supplied — drives the retry prompt.
  import_all: (a) => {
    if (!a || !a.password) throw new Error("password-required");
    if (a.password !== "letmein") throw new Error("wrong password or corrupt backup");
    return { name: "Prompt Saver 2026-07-25.psb", count: 12 };
  },
  // Screenshot overlay: no capture session, nothing to draw.
  snip_should_begin: () => false,
  snip_background: () => null,
  snip_windows: () => [],
  snip_present: () => false,
  usage_stats: () => ({
    total_prompts: 12, total_copies: 40, copies7: 5, copies30: 20, favorites: 3,
    unused: 4, used_count: 8, avg_copies: 3.3, total_chars: 900, colored: 12,
    recent_name: "Prompt 1", recent_ts: 1750000000,
    longest_name: "Prompt 2", longest_chars: 120,
    types: [{ name: "text", count: 12 }],
    top: [{ name: "Prompt 1", count: 9 }],
    never_used: [{ name: "Prompt 9", count: 0 }],
    per_view: [{ name: "Home", count: 12 }],
    history_on: true,
  }),
};

const noopWindow = {
  // floating.js derives the prompt id from the window label.
  label: "float-p0",
  listen: async () => () => {}, onCloseRequested: async () => () => {},
  onFocusChanged: async () => () => {}, onResized: async () => () => {},
  onMoved: async () => () => {}, onScaleChanged: async () => () => {},
  onThemeChanged: async () => () => {}, setAlwaysOnTop: async () => {},
  setSize: async () => {}, setPosition: async () => {},
  outerPosition: async () => ({ x: 0, y: 0 }),
  outerSize: async () => ({ width: innerWidth, height: innerHeight }),
  startDragging: async () => {}, setIgnoreCursorEvents: async () => {},
  minimize: async () => {}, maximize: async () => {}, unmaximize: async () => {},
  isMaximized: async () => false, close: async () => {}, hide: async () => {},
  show: async () => {}, setFocus: async () => {},
  innerSize: async () => ({ width: innerWidth, height: innerHeight }),
  scaleFactor: async () => 1, setZoom: async () => {},
};

window.__TAURI__ = {
  core: {
    invoke: async (cmd, args) => {
      (window.__SMOKE_CALLS__ = window.__SMOKE_CALLS__ || []).push(cmd);
      const fn = RESULTS[cmd];
      return fn ? fn(args) : null;
    },
  },
  event: { listen: async () => () => {}, emit: async () => {} },
  window: { getCurrentWindow: () => noopWindow },
  webviewWindow: { getCurrentWebviewWindow: () => noopWindow },
};
window.__TAURI__.invoke = window.__TAURI__.core.invoke;
