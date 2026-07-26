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
      ? "Meeting on {#{date}#} at {#{time}#}: {#{clipboard}#}"
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
}));

const LAYOUT = {};
PROMPTS.forEach((p, i) => { LAYOUT[p.id] = [i % 4, Math.floor(i / 4)]; });

const SETTINGS = {
  theme: "dark", language: "en", floating: {}, float_scale: {}, video_prefs: {},
  window: null, minimize_to_tray: false, autostart: false, start_minimized: false,
  always_on_top: false, hotkey: "", auto_update: true, skipped_versions: [],
  // Tags, custom themes, view groups, the Markdown preview and private notes ship
  // off; the stub turns them on so the smoke run covers that UI too.
  ui_flags: { tags: true, customThemes: true, viewGroups: true, mdPreview: true, promptNotes: true },
  ui_values: {}, ui_texts: {}, dup_ignored: [],
  tag_colors: { work: "#3b82f6" },
  custom_themes: [{ id: "ct1", name: "Neon", base: "dark", vars: { accent: "#ff00aa" } }],
  tile_font: "system", tile_size: 0,
  views: [
    { id: "home", name: "Home", cols: 4, rows: 3, layouts: { "4x3": LAYOUT }, color: "" },
    { id: "work", name: "Work", cols: 4, rows: 3, layouts: {}, color: "#ef4444" },
    { id: "notes", name: "Notes", cols: 4, rows: 3, layouts: {}, color: "" },
    { id: "archive", name: "Archive", cols: 4, rows: 3, layouts: {}, color: "" },
  ],
  // Two of the four views live in a folder tab, so the tab bar shows both shapes.
  view_groups: [{ id: "g1", name: "Docs", color: "", view_ids: ["notes", "archive"] }],
  active_view: "home",
};

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

const RESULTS = {
  get_settings: () => JSON.parse(JSON.stringify(SETTINGS)),
  get_state: () => ({ prompts: PROMPTS, settings: JSON.parse(JSON.stringify(SETTINGS)) }),
  current_theme: () => "dark",
  theme_vars: () => ({ accent: "#ff00aa" }),
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
  app_version: () => "0.0.0",
  current_data_dir: () => "C:\\Data",
  default_screenshot_dir: () => "C:\\Pictures",
  list_backups: () => [
    { name: "2026-07-25_18-30-00", size: 812345 },
    { name: "2026-07-24_09-05-00", size: 654321 },
  ],
  recent_copies: () => PROMPTS.slice(0, 4).map((p, i) => ({ id: p.id, ts: 1750000000 + i })),
  clip_inbox_list: () => [],
  list_versions: () => [],
  missing_files: () => [],
  has_backup_password: () => true,
  find_duplicates: () => [],
  // A release body in the shape GitHub delivers it, so the changelog popup has real
  // Markdown to render (heading, wrapped paragraph, table, bullets, bold, code, link).
  check_update: () => ({
    available: true,
    version: "9.9.9",
    url: "https://example.invalid/setup.exe",
    skipped: false,
    notes: [
      "## Prompt Saver 9.9.9",
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
  get_prompt: (a) => PROMPTS.find((p) => p.id === (a && a.id)) || PROMPTS[0],
  set_active_view: (a) => {
    if (SETTINGS.views.some((v) => v.id === (a && a.id))) SETTINGS.active_view = a.id;
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
  // The tag commands answer with the settings blob, like the real backend.
  rename_tag: () => JSON.parse(JSON.stringify(SETTINGS)),
  delete_tag: () => JSON.parse(JSON.stringify(SETTINGS)),
  set_tag_color: () => JSON.parse(JSON.stringify(SETTINGS)),
  // The copy commands report success; the UI skips everything that follows a copy
  // (feedback, history, auto-paste) when they come back falsy.
  copy_prompt: () => true,
  // The resolved text is kept so the probe can assert what the placeholders became.
  copy_text: (a) => { window.__LAST_COPY__ = a && a.text; return true; },
  clipboard_text: () => "CLIPTEXT",
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
    total_prompts: 12, total_copies: 40, copies7: 5, copies30: 20, used_count: 8, avg_copies: 3.3, total_chars: 900,
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
