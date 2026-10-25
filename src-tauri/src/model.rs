use super::*;

// ---------- Data model ----------

#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct Prompt {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) text: String,
    // Optional tile color (hex); empty = default surface color.
    #[serde(default)]
    pub(crate) color: String,
    // Optional PNG data URL (scaled to ≤1024px); empty = no image.
    #[serde(default)]
    pub(crate) image: String,
    // When true the tile shows the image instead of the name text.
    #[serde(default)]
    pub(crate) show_image: bool,
    // True = clicking copies the image itself; false = the image is only an
    // icon and clicking copies the text.
    #[serde(default)]
    pub(crate) copy_image: bool,
    // Attached file: clicking puts the file itself on the clipboard.
    #[serde(default)]
    pub(crate) file_path: String,
    // Page of an attached PDF the preview shows (0-based), ignored for other attachments.
    // Clamped against the real page count at render time (pdf_preview), since the file can
    // change independently of this number.
    #[serde(default)]
    pub(crate) pdf_page: u32,
    // Gif/video used as the tile icon only: shown, never copied.
    #[serde(default)]
    pub(crate) icon_path: String,
    // Optional caption shown over media tiles (size 0 = default).
    #[serde(default)]
    pub(crate) caption: String,
    #[serde(default)]
    pub(crate) caption_size: u32,
    // Per-tile style overrides; empty / 0 = follow the global settings.
    #[serde(default)]
    pub(crate) font: String,
    #[serde(default)]
    pub(crate) font_size: u32,
    // Marked as a favorite (starred) in the library.
    #[serde(default)]
    pub(crate) favorite: bool,
    // Free-form tag names. Compared case-insensitively; the casing stored here
    // is the one first used in the library (see sanitize_tags).
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    // Private context for the user. Shown in the editor and (optionally) in the
    // tile tooltip: never part of anything that reaches the clipboard.
    #[serde(default)]
    pub(crate) notes: String,
    // Global accelerator that copies this prompt from anywhere ("" = none).
    #[serde(default)]
    pub(crate) hotkey: String,
    // What THIS hotkey does: "copy", "paste" (copy and paste into the app the
    // user was in), or "" = follow the global promptHotkeyPaste setting.
    #[serde(default)]
    pub(crate) hotkey_paste: String,
    // Unix seconds set when a prompt moves into the trash table; 0 for every prompt still in
    // store.prompts. Drives the trash list's "deleted N days ago" and the retention sweep, and
    // never appears in an export or a fresh add_prompt/update_prompt.
    #[serde(default)]
    pub(crate) trashed_at: u64,
    // "Rotating prompts": `text` is entry 1 and `variants` holds entries 2..n, so every consumer
    // of `text` (tile preview, search, export, dup-finder, tooltip, Markdown sync) stays correct,
    // and "rotation is on" is exactly `!variants.is_empty()` with no separate bool to drift.
    #[serde(default)]
    pub(crate) variants: Vec<String>,
    // "" = in order, "random" = shuffled bag (see rotation_next).
    #[serde(default)]
    pub(crate) rotate_mode: String,
    // Auto-rotate: after each paste the clipboard advances to the next entry. Meaningless
    // without variants (like rotate_mode).
    #[serde(default)]
    pub(crate) rotate_auto: bool,
    // Duration of an attached video, measured once when the attachment is set (probe_media_meta),
    // never on render. 0 = never measured, or no video stream. Read only by the library/history
    // thumbnail badge.
    #[serde(default)]
    pub(crate) media_duration_ms: u64,
    // Pixel width/height of an attached video or still image, 0 = unknown.
    #[serde(default)]
    pub(crate) media_width: u32,
    #[serde(default)]
    pub(crate) media_height: u32,
    // Size in bytes of the attached file (file_path/icon_path), 0 = unknown.
    #[serde(default)]
    pub(crate) media_size_bytes: u64,
    // Text recognized in the attached picture: a search index only, never shown, exported or
    // synchronized. Empty unless ocrIndex was on when the picture was attached; cleared when the
    // picture is removed or replaced.
    #[serde(default)]
    pub(crate) image_text: String,
    // Marked confidential: `text` and every entry of `variants` hold AES-GCM ciphertext (see
    // secret_encrypt), gated by the session vault password (secret_vault_unlock). Name, tags and
    // image stay plain, or the library grid could not draw the tile while locked.
    #[serde(default)]
    pub(crate) secret: bool,
}

// A saved floating window position plus the identity of the screen it was placed on (name,
// resolution, offset in the virtual desktop). Coordinates alone cannot tell a returning
// screen from a different one at the same spot; `screen` lets a parked pill find its way
// home. Empty on older saves, read as "unknown" rather than "no screen", so an upgrade never
// parks a button nothing moved (see resolve_float_spot).
#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct Pos {
    pub(crate) x: i32,
    pub(crate) y: i32,
    #[serde(default)]
    pub(crate) screen: String,
}

// Saved main-window geometry (physical pixels). Validated against monitors on load.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub(crate) struct WindowGeom {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    // The window was maximized when last seen. The bounds above stay the normal (restored)
    // ones, so closing while maximized and starting again restores the old size under it.
    #[serde(default)]
    pub(crate) maximized: bool,
}

// A named page with its own grid size and one placement map per grid size,
// so switching grid dimensions restores the arrangement saved for that size.
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct View {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default = "default_cols")]
    pub(crate) cols: u32,
    #[serde(default = "default_rows")]
    pub(crate) rows: u32,
    // "6x5" -> promptId -> [col,row]
    #[serde(default)]
    pub(crate) layouts: HashMap<String, HashMap<String, [u32; 2]>>,
    // Optional tab color (hex); empty = default.
    #[serde(default)]
    pub(crate) color: String,
    // Ordered membership: who belongs to this view, independent of where a grid size has room
    // to show them. `layouts` says where a member sits, never whether it belongs, so a
    // grid-size change cannot drop or resurrect an entry. A member without a cell in the
    // current size's layout is an overflow entry.
    #[serde(default)]
    pub(crate) members: Vec<String>,
    // Ids other machines' files use for this view when it was matched to them by name (it keeps
    // its own id here). Kept with the view so the match survives a restart; the smallest one
    // that is not a local view's id is what this view's files carry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) sync_ids: Vec<String>,
}

// An Android-style folder inside the grid. It occupies one cell of one view and holds
// prompts that are no longer tiles of their own: a member's layout entry is dropped when it
// joins and it gets a free cell when it leaves. If the view's grid size changes and the
// folder's cell disappears, it moves to the first free one.
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct TileFolder {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) name: String,
    // Optional folder colour (hex); empty = default.
    #[serde(default)]
    pub(crate) color: String,
    #[serde(default)]
    pub(crate) view_id: String,
    #[serde(default)]
    pub(crate) cell: [u32; 2],
    #[serde(default)]
    pub(crate) members: Vec<String>,
}

// An Android-style folder holding several floating pills. It owns one window (the folder
// icon; expanding only resizes it), so `pos` is the icon's top-left in physical desktop
// pixels, the space `Settings.floating` uses. A member keeps its own `floating` entry so
// switching the feature off restores every pill.
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct FloatFolder {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) pos: Pos,
    #[serde(default = "default_folder_scale")]
    pub(crate) scale: f64,
    #[serde(default)]
    pub(crate) members: Vec<String>,
}

pub(crate) fn default_folder_scale() -> f64 {
    1.0
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub(crate) struct VideoPrefs {
    pub(crate) volume: u32, // 0..=100
    pub(crate) muted: bool,
    #[serde(rename = "loop")]
    pub(crate) looped: bool,
}

// A user-defined theme. It inherits every rule of `base` (one of the built-in
// themes) and overrides only the colour variables listed in `vars`.
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct CustomTheme {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default = "default_custom_base")]
    pub(crate) base: String,
    #[serde(default)]
    pub(crate) vars: HashMap<String, String>,
}

pub(crate) fn default_custom_base() -> String {
    "light".to_string()
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Settings {
    #[serde(default = "default_theme")]
    pub(crate) theme: String,
    #[serde(default)]
    pub(crate) floating: HashMap<String, Pos>,
    // Per-floating-button size factor (1.0 = default).
    #[serde(default)]
    pub(crate) float_scale: HashMap<String, f64>,
    // Pill folders. Kept while the feature is off, so turning it back on restores them.
    #[serde(default)]
    pub(crate) float_folders: Vec<FloatFolder>,
    // Per-prompt video player state (volume, mute, loop), grid and pill.
    #[serde(default)]
    pub(crate) video_prefs: HashMap<String, VideoPrefs>,
    #[serde(default)]
    pub(crate) window: Option<WindowGeom>,
    #[serde(default = "default_on")]
    pub(crate) minimize_to_tray: bool,
    // False for a store written before the 30/90-day retention defaults; migrate() pins such a
    // store to keep-forever once.
    #[serde(default)]
    pub(crate) retention_defaults_v2: bool,
    #[serde(default)]
    pub(crate) autostart: bool,
    #[serde(default)]
    pub(crate) start_minimized: bool,
    // Keep the main window above all other windows (except while minimized).
    #[serde(default)]
    pub(crate) always_on_top: bool,
    // Global hotkey accelerator (e.g. "CommandOrControl+Shift+P"); empty = off.
    #[serde(default)]
    pub(crate) hotkey: String,
    // Global voice-dictation hotkey; empty = off. Validated like `hotkey` (see HotkeyOwner).
    #[serde(default)]
    pub(crate) voice_hotkey: String,
    // Global stop/pause hotkeys for a running recording, registered only while one is active
    // (see rearm_hotkeys). Unlike `hotkey`/`voice_hotkey` they ship with a default, since a
    // long-running capture needs a way out from the start.
    #[serde(default = "default_rec_stop_hotkey")]
    pub(crate) rec_stop_hotkey: String,
    #[serde(default = "default_rec_pause_hotkey")]
    pub(crate) rec_pause_hotkey: String,
    #[serde(default = "default_on")]
    pub(crate) auto_update: bool,
    // Whether the user allowed the automatic update check. A fresh profile starts Unasked and
    // never goes online until answered; a stored blob without the key is an existing install
    // (Legacy), which `migrate` turns into Yes or No from its `auto_update` value.
    #[serde(default = "legacy_update_consent")]
    pub(crate) update_consent: UpdateConsent,
    // How far the one-time rewrites of stored content have run (see DATA_VERSION).
    // A stored settings blob without the key is at step 0.
    #[serde(default)]
    pub(crate) data_version: u32,
    // Update versions the user chose to skip: never offered again.
    #[serde(default)]
    pub(crate) skipped_versions: Vec<String>,
    // Highest app version the "what's new" list has already shown. Empty means
    // never shown (fresh install), see whats_new_check.
    #[serde(default)]
    pub(crate) last_seen_version: String,
    // Expert toggles: feature key -> enabled. A missing key means enabled, so
    // every feature is on by default and only explicit `false` disables it.
    #[serde(default)]
    pub(crate) ui_flags: HashMap<String, bool>,
    // Expert numeric tweaks: key -> value. A missing key uses the frontend default.
    #[serde(default)]
    pub(crate) ui_values: HashMap<String, f64>,
    // Expert string options (e.g. copy-feedback font). Missing key = default.
    #[serde(default)]
    pub(crate) ui_texts: HashMap<String, String>,
    // Seeded once on a genuinely fresh install; never re-seeded after the user deletes everything.
    #[serde(default)]
    pub(crate) examples_seeded: bool,
    // Ids of the seeded example prompts, so "remove examples" finds exactly those.
    #[serde(default)]
    pub(crate) example_prompt_ids: Vec<String>,
    // Content fingerprint of each seeded example when it was created (example_seed_fingerprint).
    // "Remove examples" deletes only an id whose fingerprint still matches; an edited one is the
    // user's now. An id with no entry (installs seeded before this field existed) counts as
    // unchanged.
    #[serde(default)]
    pub(crate) example_prompt_seeds: HashMap<String, u64>,
    // Each entry is one group of prompt ids dismissed together as "not a duplicate", never
    // re-suggested as a group. A stored array of two ids loads as a 2-member group, so older
    // pair-shaped saves need no migration.
    #[serde(default)]
    pub(crate) dup_ignored: Vec<Vec<String>>,
    // Tag key (the lowercased name) -> chip color (#rrggbb). Keyed lowercase so a
    // recased tag keeps its color. A tag without an entry renders neutral.
    #[serde(default)]
    pub(crate) tag_colors: HashMap<String, String>,
    // User-defined themes. `theme` points at one of them with "custom:<id>";
    // each carries only the variables that differ from its built-in base.
    #[serde(default)]
    pub(crate) custom_themes: Vec<CustomTheme>,
    // Legacy in-blob copy history: read once on upgrade to migrate into the `copy_log` table,
    // never written back (the table owns it, so a huge history does not bloat this encrypted
    // settings blob).
    #[serde(default, skip_serializing)]
    pub(crate) copy_log: Vec<CopyEntry>,
    // Per-prompt copy count + last-used time (unix seconds). Both bounded by the
    // number of prompts, so they stay small in the blob.
    #[serde(default)]
    pub(crate) usage: HashMap<String, u32>,
    #[serde(default)]
    pub(crate) last_used: HashMap<String, u64>,
    // Rotation pointer for "variants" prompts, kept out of Prompt (see rotation_next) so a copy
    // does not rewrite prompts.json, bump the version history or touch the Markdown sync file.
    // Index of the next entry to serve.
    #[serde(default)]
    pub(crate) rotate_pos: HashMap<String, u32>,
    // Shuffled remaining indices for "random" mode (drawn without repetition, reshuffled when
    // empty). Absent or empty means "reshuffle on next use".
    #[serde(default)]
    pub(crate) rotate_bag: HashMap<String, Vec<u32>>,
    #[serde(default = "default_on")]
    pub(crate) show_header: bool,
    #[serde(default = "default_on")]
    pub(crate) show_composer: bool,
    #[serde(default = "default_language")]
    pub(crate) language: String,
    #[serde(default = "default_tile_font")]
    pub(crate) tile_font: String,
    #[serde(default = "default_tile_size")]
    pub(crate) tile_size: u32,
    #[serde(default)]
    pub(crate) active_view: String,
    #[serde(default)]
    pub(crate) views: Vec<View>,
    // Grid folders. Kept while the feature is off: members show as their own tiles again.
    #[serde(default)]
    pub(crate) tile_folders: Vec<TileFolder>,
    // Where a synced file put a tile that this machine had to seat elsewhere, per prompt id.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(crate) sync_file_cells: HashMap<String, crate::md_sync::SyncFileCell>,
    // What the folder sync knows about the delete marker of a trashed prompt, per id: 0 while
    // the marker is still owed to the folder (the delete was made here), otherwise the time it
    // was first seen there. Lets a restart tell a delete that never reached the folder from a
    // restore made on another machine.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(crate) sync_trash_marks: HashMap<String, u64>,
}

pub(crate) fn default_theme() -> String {
    "system".to_string()
}
pub(crate) fn default_cols() -> u32 {
    5
}
pub(crate) fn default_rows() -> u32 {
    4
}
pub(crate) fn default_on() -> bool {
    true
}
pub(crate) fn default_language() -> String {
    "auto".to_string()
}
pub(crate) fn default_tile_font() -> String {
    "system".to_string()
}
// 0 = auto-fit (default): each tile text grows to the largest size that fits.
pub(crate) fn default_tile_size() -> u32 {
    0
}
pub(crate) fn default_rec_stop_hotkey() -> String {
    "Ctrl+Alt+S".to_string()
}
pub(crate) fn default_rec_pause_hotkey() -> String {
    // Distinct from the quick-launcher's default (`hotkey`), which would collide on a fresh
    // install and keep the pause hotkey from registering.
    "Ctrl+Alt+O".to_string()
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub(crate) enum UpdateConsent {
    Unasked,
    Yes,
    No,
    Legacy,
}

pub(crate) fn legacy_update_consent() -> UpdateConsent {
    UpdateConsent::Legacy
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: default_theme(),
            floating: HashMap::new(),
            float_scale: HashMap::new(),
            float_folders: Vec::new(),
            video_prefs: HashMap::new(),
            window: None,
            minimize_to_tray: true,
            retention_defaults_v2: true,
            autostart: false,
            start_minimized: false,
            always_on_top: false,
            // A fresh install ships with the quick-launcher bound so the feature is discoverable. An
            // existing install's stored value (even empty/off) is untouched, since #[serde(default)]
            // only fills in values never written.
            hotkey: "CommandOrControl+Alt+P".to_string(),
            // Voice dictation ships without a global binding: a fresh install should not start
            // listening on a key combo.
            voice_hotkey: String::new(),
            rec_stop_hotkey: default_rec_stop_hotkey(),
            rec_pause_hotkey: default_rec_pause_hotkey(),
            auto_update: true,
            update_consent: UpdateConsent::Unasked,
            data_version: 0,
            skipped_versions: Vec::new(),
            last_seen_version: String::new(),
            ui_flags: HashMap::new(),
            ui_values: HashMap::new(),
            ui_texts: HashMap::new(),
            examples_seeded: false,
            example_prompt_ids: Vec::new(),
            example_prompt_seeds: HashMap::new(),
            dup_ignored: Vec::new(),
            tag_colors: HashMap::new(),
            custom_themes: Vec::new(),
            copy_log: Vec::new(),
            usage: HashMap::new(),
            last_used: HashMap::new(),
            rotate_pos: HashMap::new(),
            rotate_bag: HashMap::new(),
            show_header: true,
            show_composer: true,
            language: default_language(),
            tile_font: default_tile_font(),
            tile_size: default_tile_size(),
            active_view: String::new(),
            views: Vec::new(),
            tile_folders: Vec::new(),
            sync_file_cells: HashMap::new(),
            sync_trash_marks: HashMap::new(),
        }
    }
}

// Current step of the one-time stored-content rewrites; load_store runs every step above
// `Settings::data_version` once. Step 1: placeholders in the old {#{...}#} spelling in the
// version history and the trash.
pub(crate) const DATA_VERSION: u32 = 1;
pub(crate) const GRID_MIN: u32 = 1;
// Hard safety ceilings. The user-facing limits are 20 (enforced in the UI via the expert
// values gridMax / maxViews); these only cap how far those overrides can be pushed.
pub(crate) const GRID_MAX: u32 = 200;
pub(crate) const MAX_VIEWS: usize = 200;
// Grid folders. One per cell at most, so a huge grid is the real ceiling.
pub(crate) const MAX_TILE_FOLDERS: usize = 200;
pub(crate) const FLOAT_W: f64 = 360.0;
pub(crate) const FLOAT_H: f64 = 80.0; // flat pill shape, clearly wider than tall
pub(crate) const FLOAT_IMG: f64 = 400.0; // square box for image pills: S 300 / M 400 / L 560
// Pill folders.
pub(crate) const MAX_FLOAT_FOLDERS: usize = 100;
pub(crate) const MAX_FOLDER_NAME: usize = 40;
pub(crate) const FOLDER_BOX: f64 = 96.0; // collapsed folder icon (square, logical px at scale 1)
// Brand-owned identifiers: every product name in the Rust code comes from src/brand.rs,
// which tools/brand.js writes from brands/<brand>.json.
pub(crate) const AUTOSTART_KEY: &str = brand::AUTOSTART_KEY;
// Local data folder under %APPDATA%\Roaming, used by the pre-app panic hook (no AppHandle)
// and the store path; deliberately not the bundle identifier.
pub(crate) const DATA_FOLDER: &str = brand::DATA_FOLDER;

pub(crate) fn grid_key(cols: u32, rows: u32) -> String {
    format!("{}x{}", cols, rows)
}

// WebView2-missing dialog texts. Shown before settings exist, so the OS locale decides.
// `{app}` is filled with the product name in webview2_dialog.
pub(crate) fn webview2_texts(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "de" => ("WebView2 Runtime fehlt", "{app} benötigt die Microsoft WebView2 Runtime.\n\nJetzt herunterladen und installieren? Danach {app} einfach erneut starten."),
        "es" => ("Falta WebView2 Runtime", "{app} necesita Microsoft WebView2 Runtime.\n\n¿Descargarlo e instalarlo ahora? Después, simplemente inicia {app} de nuevo."),
        "it" => ("WebView2 Runtime mancante", "{app} richiede Microsoft WebView2 Runtime.\n\nScaricarlo e installarlo ora? Dopo, riavvia semplicemente {app}."),
        "pt" => ("WebView2 Runtime ausente", "O {app} precisa do Microsoft WebView2 Runtime.\n\nBaixar e instalar agora? Depois, basta iniciar o {app} novamente."),
        "pl" => ("Brak środowiska WebView2", "{app} wymaga Microsoft WebView2 Runtime.\n\nPobrać i zainstalować teraz? Następnie po prostu uruchom {app} ponownie."),
        "zh" => ("缺少 WebView2 运行时", "{app} 需要 Microsoft WebView2 运行时。\n\n现在下载并安装吗？安装后重新启动 {app} 即可。"),
        "ja" => ("WebView2 ランタイムがありません", "{app} には Microsoft WebView2 ランタイムが必要です。\n\n今すぐダウンロードしてインストールしますか？その後、{app} を再起動してください。"),
        "nl" => ("WebView2-runtime ontbreekt", "{app} heeft de Microsoft WebView2-runtime nodig.\n\nNu downloaden en installeren? Start {app} daarna gewoon opnieuw."),
        "tr" => ("WebView2 çalışma zamanı eksik", "{app}, Microsoft WebView2 çalışma zamanına ihtiyaç duyar.\n\nŞimdi indirilip kurulsun mu? Ardından {app}'ı yeniden başlatmanız yeterli."),
        "ro" => ("Lipsește WebView2 Runtime", "{app} are nevoie de Microsoft WebView2 Runtime.\n\nDescărcați și instalați acum? Apoi porniți pur și simplu {app} din nou."),
        _ => ("WebView2 runtime missing", "{app} needs the Microsoft WebView2 runtime.\n\nDownload and install it now? Simply start {app} again afterwards."),
    }
}

// The same texts with the product name filled in.
pub(crate) fn webview2_dialog(lang: &str) -> (String, String) {
    let (title, msg) = webview2_texts(lang);
    (
        title.replace("{app}", brand::DISPLAY_NAME),
        msg.replace("{app}", brand::DISPLAY_NAME),
    )
}

// Supported UI languages besides English (keep in sync with ui/i18n.js).
pub(crate) const LANG_CODES: [&str; 10] = [
    "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro",
];

#[link(name = "kernel32")]
extern "system" {
    fn GetUserPreferredUILanguages(
        flags: u32,
        num_languages: *mut u32,
        buffer: *mut u16,
        buffer_len: *mut u32,
    ) -> i32;
    fn GetUserDefaultUILanguage() -> u16;
    fn LCIDToLocaleName(locale: u32, name: *mut u16, len: i32, flags: u32) -> i32;
}

const MUI_LANGUAGE_NAME: u32 = 0x8;

// First entry of a double-nul terminated multi-string buffer; empty when there is none.
pub(crate) fn first_multi_sz(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

// The Windows display language as a BCP-47 tag such as "de-DE" (the language the interface
// follows, not the regional format); empty when it cannot be read.
pub(crate) fn system_locale() -> String {
    let mut count: u32 = 0;
    let mut len: u32 = 0;
    let sized = unsafe {
        GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, std::ptr::null_mut(), &mut len)
    };
    if sized != 0 && len > 1 {
        let mut buf = vec![0u16; len as usize];
        let ok = unsafe {
            GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buf.as_mut_ptr(), &mut len)
        };
        if ok != 0 {
            let first = first_multi_sz(&buf);
            if !first.is_empty() {
                return first;
            }
        }
    }
    // Fallback: the install-independent UI language id, converted to a locale name.
    let lcid = unsafe { GetUserDefaultUILanguage() } as u32;
    let mut name = [0u16; 85];
    let n = unsafe { LCIDToLocaleName(lcid, name.as_mut_ptr(), name.len() as i32, 0) };
    if n > 1 {
        return String::from_utf16_lossy(&name[..(n as usize - 1)]);
    }
    String::new()
}

pub(crate) fn resolve_lang(pref: &str) -> &'static str {
    let raw = if pref != "auto" {
        pref.to_string()
    } else {
        system_locale()
    };
    let low = raw.to_lowercase();
    LANG_CODES
        .iter()
        .find(|code| low.starts_with(**code))
        .copied()
        .unwrap_or("en")
}

// Tray menu labels per language.
pub(crate) fn tray_labels(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "de" => ("Öffnen", "Beenden"),
        "es" => ("Abrir", "Salir"),
        "it" => ("Apri", "Esci"),
        "pt" => ("Abrir", "Sair"),
        "pl" => ("Otwórz", "Zakończ"),
        "zh" => ("打开", "退出"),
        "ja" => ("開く", "終了"),
        "nl" => ("Openen", "Afsluiten"),
        "tr" => ("Aç", "Çıkış"),
        "ro" => ("Deschide", "Ieșire"),
        _ => ("Open", "Quit"),
    }
}

// Voice-hotkey feedback while the window may stay hidden: the tray tooltip switches to a
// recording state for the length of a dictation session, then back to the app name.
pub(crate) fn set_tray_recording(app: &AppHandle, recording: bool) {
    let Some(tray) = app.tray_by_id("tray") else { return };
    let text = if recording {
        let lang = app
            .try_state::<Db>()
            .map(|state| lock(&state).settings.language.clone())
            .unwrap_or_else(|| "auto".to_string());
        tray_recording_label(resolve_lang(&lang)).to_string()
    } else {
        brand::DISPLAY_NAME.to_string()
    };
    let _ = tray.set_tooltip(Some(text));
}

// Tray tooltip text while a voice-dictation session is running.
pub(crate) fn tray_recording_label(lang: &str) -> &'static str {
    match lang {
        "de" => "Aufnahme läuft…",
        "es" => "Grabando…",
        "it" => "Registrazione…",
        "pt" => "A gravar…",
        "pl" => "Nagrywanie…",
        "zh" => "录音中…",
        "ja" => "録音中…",
        "nl" => "Opname…",
        "tr" => "Kaydediliyor…",
        "ro" => "Se înregistrează…",
        _ => "Recording…",
    }
}

// The tray's Stop/Pause/Resume labels for a running recording. The tray is drawn by the OS,
// not the webview, so these live here rather than in ui/i18n.js.
pub(crate) fn tray_rec_stop_label(lang: &str) -> &'static str {
    match lang {
        "de" => "Aufnahme beenden",
        "es" => "Detener grabación",
        "it" => "Interrompi registrazione",
        "pt" => "Parar gravação",
        "pl" => "Zatrzymaj nagrywanie",
        "zh" => "停止录制",
        "ja" => "録画を停止",
        "nl" => "Opname stoppen",
        "tr" => "Kaydı durdur",
        "ro" => "Oprește înregistrarea",
        _ => "Stop recording",
    }
}
pub(crate) fn tray_rec_pause_label(lang: &str, paused: bool) -> &'static str {
    if paused {
        match lang {
            "de" => "Aufnahme fortsetzen",
            "es" => "Reanudar grabación",
            "it" => "Riprendi registrazione",
            "pt" => "Retomar gravação",
            "pl" => "Wznów nagrywanie",
            "zh" => "继续录制",
            "ja" => "録画を再開",
            "nl" => "Opname hervatten",
            "tr" => "Kaydı sürdür",
            "ro" => "Reia înregistrarea",
            _ => "Resume recording",
        }
    } else {
        match lang {
            "de" => "Aufnahme pausieren",
            "es" => "Pausar grabación",
            "it" => "Metti in pausa la registrazione",
            "pt" => "Pausar gravação",
            "pl" => "Wstrzymaj nagrywanie",
            "zh" => "暂停录制",
            "ja" => "録画を一時停止",
            "nl" => "Opname pauzeren",
            "tr" => "Kaydı duraklat",
            "ro" => "Pune înregistrarea pe pauză",
            _ => "Pause recording",
        }
    }
}

// Headers of the two tray quick-copy sections; they live here because the tray is drawn by
// the OS, not the webview.
pub(crate) fn tray_section_labels(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "de" => ("Zuletzt benutzt", "Favoriten"),
        "es" => ("Usados recientemente", "Favoritos"),
        "it" => ("Usati di recente", "Preferiti"),
        "pt" => ("Usados recentemente", "Favoritos"),
        "pl" => ("Ostatnio używane", "Ulubione"),
        "zh" => ("最近使用", "收藏"),
        "ja" => ("最近使ったもの", "お気に入り"),
        "nl" => ("Onlangs gebruikt", "Favorieten"),
        "tr" => ("Son kullanılanlar", "Favoriler"),
        "ro" => ("Utilizate recent", "Favorite"),
        _ => ("Recently used", "Favorites"),
    }
}

// Placeholder of the tray menu's filter row; kept here so every tray string is translated in
// one place.
pub(crate) fn tray_search_placeholder(lang: &str) -> &'static str {
    match lang {
        "de" => "Suchen…",
        "es" => "Buscar…",
        "it" => "Cerca…",
        "pt" => "Pesquisar…",
        "pl" => "Szukaj…",
        "zh" => "搜索…",
        "ja" => "検索…",
        "nl" => "Zoeken…",
        "tr" => "Ara…",
        "ro" => "Caută…",
        _ => "Search…",
    }
}

// Localized default name of the auto-created start page; it follows the UI language until
// the user renames it (see set_language).
pub(crate) fn home_name(lang: &str) -> &'static str {
    match lang {
        "de" => "Startseite",
        "es" => "Inicio",
        "pt" => "Início",
        "pl" => "Strona główna",
        "zh" => "主页",
        "ja" => "ホーム",
        "tr" => "Ana sayfa",
        "ro" => "Acasă",
        _ => "Home", // en + it + nl
    }
}

// Every possible default name -> a view still carrying one was never renamed.
pub(crate) const HOME_NAMES: [&str; 18] = [
    "Home", "Startseite", "Inicio", "Accueil", "Início",
    "Strona główna", "Главная", "主页", "ホーム",
    "Ana sayfa", "홈", "होम", "Beranda", "Trang chủ",
    "Domů", "Головна", "Hem", "Acasă",
];

// Expert-menu values that no longer exist. A loaded store drops them once, so a key added
// back later never inherits a stale stored value.
pub(crate) fn removed_ui_value_keys() -> &'static [&'static str] {
    &[
        "snipPenPx",
        "snipMarkerPx",
        "snipLinePx",
        "snipArrowPx",
        "snipRectPx",
        "snipEllipsePx",
        "snipStepPx",
        "snipArrowHeadScale",
        "snipTextPx",
        "snipTextOutlinePx",
        "snipPixelBlock",
        "snipSpotlightDim",
        "voiceLangCacheH",
        "voiceLangCheckH",
        "voiceLangNoticeMs",
        "voiceLangSnoozeH",
        "ocrLangCacheH",
        "ocrLangCheckH",
        "ocrLangNoticeMs",
        "ocrLangSnoozeH",
        "rotateAutoSettleMs",
        "rotateAutoInputMs",
        "rotateAutoRearmMax",
        "recBrokenGraceMin",
        "recSweepEveryMin",
        "syncFailMax",
        "editStateSaveSecs",
        "mdMaxChars",
        "hlMaxChars",
        "diffWordCap",
        "errorLogMax",
        "numFieldBigStep",
        "clipSecretMinLen",
        "pillRingFrac",
        "pillMenuGap",
        "pillFolderPad",
        "pillFolderTileGap",
        "pillFolderPopGap",
        "pillMergeGrow",
        "headerPadY",
        "popoverPad",
        "viewTabEdgeZone",
        "tileFolderZone",
        "kbFocusFadeMs",
        "etaMinPct",
        "etaMinSecs",
        "journalLimit",
        "snipQuality",
        "searchRecentMax",
        "globalSearchLimit",
        "gsRecentMax",
        "launcherMaxResults",
        "trayMenuMax",
        "tooltipTimeoutMs",
        "previewLen",
        "tilesHoverOpacity",
        "mediaHoverOpacity",
        "journalWidth",
        "viewDropHoverMs",
        "viewGroupMaxRows",
        "clipSeenId",
        "tooltipWidth",
        "scrollbarWidth",
        "toastWidth",
        "toastMs",
        "modalRadius",
        "modalScale",
        "loupeCells",
        "pickSwatchPx",
        "tagChipPx",
        "tagPopCols",
        "searchDebounceMs",
        "tileBlur",
        "gridPad",
        "gridGap",
        "overlayDim",
        "animSpeed",
        "dragThreshold",
        "disarmMs",
        "composerMaxH",
        "modalMaxVh",
        "composerScale",
        "primaryScale",
        "snipPreviewVh",
        "ocrUpscaleUnderPx",
        "ocrIndexMaxChars",
        "recDiskWarnMb",
        "recAudioKbps",
        "keyOverlayHoldMs",
        "videoClipTtlMin",
        "syncPollSecs",
        "hlFontPx",
        "historyPreviewLen",
        "rotateTipLen",
        "trayLabelMax",
        "favStarSize",
        "viewBorder",
        "tileHotkeyBadgeOpacity",
        "mediaRadius",
        "mediaBlur",
        "snipEditUndoMax",
        "videoEditUndoMax",
        "savedSearchMax",
        "attachPreviewKb",
        "remotePort",
        "remoteRateLimit",
    ]
}

// Expert switches that no longer exist. A loaded store drops them once, like removed values.
pub(crate) fn removed_ui_flag_keys() -> &'static [&'static str] {
    &[
        "tileFolders",
        "pillFolders",
        "colorFilter",
        "pinButton",
        "favView",
        "libraryCloseToggle",
        "copyBubble",
        "undoToast",
        "editorDraft",
        "tooltipDelay",
        "tilePreview",
        "iconTooltips",
        "tooltipTimeout",
        "tileTimeTip",
        "showUpdates",
        "checkUpdateOnStart",
        "whatsNew",
        "tileHover",
        "tileHoverLift",
        "tilesHoverDim",
        "mediaHoverDim",
        "toolbarHoverDim",
        "boldTileNames",
        "uppercaseTiles",
        "monospaceTiles",
        "italicTiles",
        "tileTextShadow",
        "compactTiles",
        "tileGradient",
        "showJournal",
        "varPopupWindow",
        "viewGroups",
        "pausedIcons",
        "blurTilesUntilHover",
        "grayscaleMedia",
        "hideTileNames",
        "accentScrollbar",
        "smoothScroll",
        "floatPark",
        "etaShow",
        "chainVarCheck",
        "ocrAutoPickLang",
        "voiceAutoPickLang",
        "ocrLangAutoUpdate",
        "voiceLangAutoUpdate",
        "helpTips",
        "helpHighlight",
        "helpSketches",
        "historyDiff",
        "veOvRemember",
        "remoteApi",
        "remoteNetworkFilter",
        "remoteTlsShortLived",
        "urlScheme",
        "urlSchemeOpenAllowed",
        "urlSchemeCopyAllowed",
        "tileReorder",
        "favViewReorder",
        "snipEditor",
    ]
}

// Expert dropdown choices (stored as text) that no longer exist; dropped like removed values.
pub(crate) fn removed_ui_text_keys() -> &'static [&'static str] {
    &[
        "snipScaleFilter",
        "fuzzyMaxTypos",
        // The old remote receiver's rows: its password hash and TLS key, certificate and CA
        // must not survive on disk once the feature is gone.
        "remoteAllowedRanges",
        "remotePasswordHash",
        "remoteTlsKey",
        "remoteTlsCert",
        "remoteTlsCa",
    ]
}

// Holds every numeric expert value to the range its slider enforces (the table is generated
// from ui/expertConfig.js). A hand-edited file or an import can carry any number: a value
// that is not finite is dropped so the key falls back to its default, and a key whose top
// stop means "unlimited" keeps its 0.
pub(crate) fn clamp_ui_values(map: &mut HashMap<String, f64>) {
    // A non-finite number would be written as null and make the whole settings block unreadable
    // on the next start, whatever key it sits under.
    map.retain(|_, v| v.is_finite());
    let keys = crate::ui_ranges::UI_VALUE_RANGES.iter().map(|r| r.0).chain(crate::ui_ranges::UI_SELECT_RANGES.iter().map(|r| r.0));
    for key in keys {
        if let Some(&v) = map.get(key) {
            if let Some(held) = clamp_ui_value(key, v) {
                map.insert(key.to_string(), held);
            }
        }
    }
}

// One value held to the range of its key: None when it is not a finite number,
// the value unchanged for a key the range tables do not know.
pub(crate) fn clamp_ui_value(key: &str, v: f64) -> Option<f64> {
    if !v.is_finite() {
        return None;
    }
    for &(k, min, max, top_unlimited) in crate::ui_ranges::UI_VALUE_RANGES {
        if k == key {
            return Some(if top_unlimited && v == 0.0 { v } else { v.clamp(min, max) });
        }
    }
    for &(k, options) in crate::ui_ranges::UI_SELECT_RANGES {
        if k == key {
            return Some(snap_select_value(v, options));
        }
    }
    Some(v)
}

// A preset-or-custom select holds its presets or a whole number from the free-entry field's
// range. Anything below one (fractions, negatives) means "never" where the select has a zero
// preset (the safe reading for a retention that deletes things) and the smallest custom
// value where it has none; the rest is rounded and capped.
pub(crate) fn snap_select_value(v: f64, options: &[f64]) -> f64 {
    let min = crate::ui_ranges::SELECT_CUSTOM_MIN;
    if v < min {
        return if options.contains(&0.0) { 0.0 } else { min };
    }
    v.round().clamp(min, crate::ui_ranges::SELECT_CUSTOM_MAX)
}

impl Settings {
    pub(crate) fn has_removed_ui_values(&self) -> bool {
        removed_ui_value_keys().iter().any(|k| self.ui_values.contains_key(*k))
            || removed_ui_flag_keys().iter().any(|k| self.ui_flags.contains_key(*k))
            || removed_ui_text_keys().iter().any(|k| self.ui_texts.contains_key(*k))
    }

    // Ensure at least one view exists; migrate legacy single-grid data.
    pub(crate) fn migrate(&mut self) {
        // An install that stored settings before the consent question existed
        // keeps exactly the update behaviour it had.
        if self.update_consent == UpdateConsent::Legacy {
            self.update_consent = if self.auto_update { UpdateConsent::Yes } else { UpdateConsent::No };
        }
        // Merges that read an old key run first, before the removal loops below.
        //
        // Four on/off switches used to sit beside a duration; the duration alone now controls it and
        // 0 means off. A switch stored off becomes a stored 0; one that was on (or unset) leaves the
        // stored or default duration.
        for (flag, value) in [
            ("copyBubble", "bubbleMs"),
            ("undoToast", "undoToastMs"),
            ("editorDraft", "editorDraftSecs"),
            ("tooltipDelay", "tooltipDelayMs"),
        ] {
            if self.ui_flags.get(flag) == Some(&false) {
                self.ui_values.insert(value.into(), 0.0);
            }
        }
        //
        // Five search-list caps merged into "searchListMax": the largest user-set value wins so no
        // one loses rows, and an unstored cap counts with its old default so lowering one cap does
        // not shrink the others. A stored 0 ("keep no recent searches") cannot be kept, since the
        // merged list has no off stop. A value already under the new key is never overwritten.
        if !self.ui_values.contains_key("searchListMax") {
            let caps: [(&str, f64); 5] = [
                ("searchRecentMax", 8.0),
                ("globalSearchLimit", 5.0),
                ("gsRecentMax", 5.0),
                ("launcherMaxResults", 8.0),
                ("trayMenuMax", 8.0),
            ];
            let stored_max = caps
                .iter()
                .filter_map(|(k, _)| self.ui_values.get(*k).copied())
                .filter(|v| *v > 0.0)
                .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
            if let Some(stored) = stored_max {
                let largest = caps
                    .iter()
                    .filter(|(k, _)| !self.ui_values.contains_key(*k))
                    .fold(stored, |m, (_, def)| m.max(*def));
                self.ui_values.insert("searchListMax".into(), largest.clamp(3.0, 20.0));
            }
        }
        //
        // Tile lift on hover merged a highlight switch, a lift switch and a distance into one
        // distance ("hoverLift", 0 = off, default 0). An install with none of the old keys takes the
        // default; one with any of them keeps the lift it shows today (an unstored switch counts as
        // its old default, on). Runs only while an old switch is stored, so a later stored 0 survives
        // every launch.
        if self.ui_flags.contains_key("tileHoverLift") || self.ui_flags.contains_key("tileHover") {
            let on = self.ui_flags.get("tileHover") != Some(&false)
                && self.ui_flags.get("tileHoverLift") != Some(&false);
            if on {
                let v = self.ui_values.get("hoverLift").copied().unwrap_or(1.0).max(1.0);
                self.ui_values.insert("hoverLift".into(), v);
            } else {
                self.ui_values.remove("hoverLift");
            }
        }
        // Three dim-until-hovered switches (tiles, media, top bar) and two opacity sliders merged
        // into one strength, "hoverDim". The strongest dim anyone had applies to all three. Never
        // overwrites a stored strength.
        if !self.ui_values.contains_key("hoverDim") {
            let mut s: f64 = 0.0;
            if self.ui_flags.get("tilesHoverDim") == Some(&true) {
                s = s.max(100.0 - self.ui_values.get("tilesHoverOpacity").copied().unwrap_or(45.0));
            }
            if self.ui_flags.get("mediaHoverDim") == Some(&true) {
                s = s.max(100.0 - self.ui_values.get("mediaHoverOpacity").copied().unwrap_or(80.0));
            }
            if self.ui_flags.get("toolbarHoverDim") == Some(&true) {
                s = s.max(50.0);
            }
            if s > 0.0 {
                self.ui_values.insert("hoverDim".into(), (s / 5.0).round() * 5.0);
            }
        }
        // The attachment preview size slider became three steps: a stored size of 4 KB or less
        // reads as "short", 20 KB or more as "long", anything between stays "medium" (no key). A
        // stored choice is never overwritten.
        if !self.ui_texts.contains_key("attachPreviewSize") {
            if let Some(kb) = self.ui_values.get("attachPreviewKb").copied() {
                if kb <= 4.0 {
                    self.ui_texts.insert("attachPreviewSize".into(), "short".into());
                } else if kb >= 20.0 {
                    self.ui_texts.insert("attachPreviewSize".into(), "long".into());
                }
            }
        }
        // Seven look switches for tile text merged into one choice, "tileStyle". Combined styles
        // cannot be kept, so the first in this order wins. A stored choice is never overwritten.
        if !self.ui_texts.contains_key("tileStyle") {
            for (flag, style) in [
                ("boldTileNames", "bold"),
                ("uppercaseTiles", "uppercase"),
                ("monospaceTiles", "mono"),
                ("italicTiles", "italic"),
                ("tileTextShadow", "shadow"),
                ("compactTiles", "compact"),
                ("tileGradient", "gradient"),
            ] {
                if self.ui_flags.get(flag) == Some(&true) {
                    self.ui_texts.insert("tileStyle".into(), style.into());
                    break;
                }
            }
        }
        //
        // The favorites view merged "favView" and "favViewButton" (header star, on unless stored
        // false) into "favViewButton", off by default. It stays on exactly when it was visible
        // before: favView on and the star not hidden. Runs only while favView is stored, and removes
        // it, so it happens once.
        if let Some(on) = self.ui_flags.remove("favView") {
            let star = self.ui_flags.get("favViewButton") != Some(&false);
            if on && star {
                self.ui_flags.insert("favViewButton".into(), true);
            } else {
                self.ui_flags.remove("favViewButton");
            }
        }
        if !self.retention_defaults_v2 {
            for k in ["historyDays", "recordingRetentionDays"] {
                self.ui_values.entry(k.to_string()).or_insert(0.0);
            }
            self.retention_defaults_v2 = true;
        }
        // The screenshot editor's expert drawing defaults became the sizes each tool remembers. A
        // set value becomes that tool's starting size unless sizes are already remembered; values
        // outside a field's range are not carried.
        if !self.ui_texts.contains_key("snipToolSizes") {
            let mut sizes = serde_json::Map::new();
            let mut put = |old: &str, tool: &str, field: &str, scale: f64, lo: f64, hi: f64| {
                if let Some(v) = self.ui_values.get(old).copied() {
                    let v = v / scale;
                    if v.is_finite() && v >= lo && v <= hi {
                        let entry = sizes.entry(tool.to_string()).or_insert_with(|| serde_json::json!({}));
                        entry[field] = serde_json::json!(v);
                    }
                }
            };
            for (old, tool) in [
                ("snipPenPx", "pen"),
                ("snipMarkerPx", "marker"),
                ("snipLinePx", "line"),
                ("snipArrowPx", "arrow"),
                ("snipRectPx", "rect"),
                ("snipEllipsePx", "ellipse"),
                ("snipStepPx", "step"),
            ] {
                put(old, tool, "width", 1.0, 1.0, 80.0);
            }
            put("snipTextPx", "text", "fontPx", 1.0, 10.0, 96.0);
            put("snipTextOutlinePx", "text", "outlinePx", 1.0, 1.0, 12.0);
            put("snipPixelBlock", "pixelate", "blockPx", 1.0, 4.0, 64.0);
            put("snipSpotlightDim", "spotlight", "dim", 100.0, 0.1, 0.9);
            if !sizes.is_empty() {
                self.ui_texts.insert("snipToolSizes".into(), serde_json::Value::Object(sizes).to_string());
            }
        }
        if let Some(h) = self.ui_values.get_mut("updateCheckH") {
            *h = clamp_update_hours(*h);
        }
        // Grid folders and floating-button folders share one switch now. Folders stay on unless both
        // old switches were stored off, since switching them off hides the folders a user kept.
        if !self.ui_flags.contains_key("folders")
            && (self.ui_flags.contains_key("tileFolders") || self.ui_flags.contains_key("pillFolders"))
        {
            let both_off = self.ui_flags.get("tileFolders") == Some(&false)
                && self.ui_flags.get("pillFolders") == Some(&false);
            self.ui_flags.insert("folders".into(), !both_off);
        }
        for k in removed_ui_value_keys() {
            self.ui_values.remove(*k);
        }
        for k in removed_ui_flag_keys() {
            self.ui_flags.remove(*k);
        }
        for k in removed_ui_text_keys() {
            self.ui_texts.remove(*k);
        }
        // "Hide from screen capture" (on unless stored false) became "allow screenshots" (off unless
        // stored true). Anyone who had turned the hiding off keeps their screenshots.
        if let Some(hidden) = self.ui_flags.remove("captureExclusion") {
            if !hidden {
                self.ui_flags.insert(ALLOW_SCREENSHOT_KEY.to_string(), true);
            }
        }
        // Rewrite a stored "ai" theme (renamed "neon") so nobody's colours reset to "system".
        if self.theme == "ai" {
            self.theme = "neon".to_string();
        }
        for c in self.custom_themes.iter_mut() {
            if c.base == "ai" {
                c.base = "neon".to_string();
            }
        }
        // The Black & White theme was removed. It was a light one, so whoever used it (as the theme
        // or as a custom theme's base) gets Light, keeping the custom theme's own colours.
        if self.theme == "mono" {
            self.theme = "light".to_string();
        }
        for c in self.custom_themes.iter_mut() {
            if c.base == "mono" {
                c.base = "light".to_string();
            }
        }
        if self.views.is_empty() {
            self.views.push(View {
                id: gen_id(),
                name: home_name(resolve_lang(&self.language)).to_string(),
                cols: default_cols(),
                rows: default_rows(),
                layouts: HashMap::new(),
                color: String::new(),
                members: Vec::new(),
                sync_ids: Vec::new(),
            });
        }
        // Every view arrives through here, including from an imported backup, so this is where the
        // ceilings the commands enforce must hold: a hand-edited backup with cols near u32::MAX
        // would make the grid renderer build billions of cells and the window never paint again.
        self.views.truncate(MAX_VIEWS);
        for v in self.views.iter_mut() {
            v.cols = v.cols.clamp(GRID_MIN, GRID_MAX);
            v.rows = v.rows.clamp(GRID_MIN, GRID_MAX);
            v.sync_ids.truncate(16);
            // One-time backfill for a view saved before membership tracking: derive membership from the
            // union of every saved grid size (current size first) instead of leaving it empty, which
            // would make every entry look hidden. Never runs once `members` is populated.
            if v.members.is_empty() {
                let current_key = grid_key(v.cols, v.rows);
                let mut sizes: Vec<&String> = v.layouts.keys().collect();
                sizes.sort_by_key(|k| if **k == current_key { 0 } else { 1 });
                let mut seen = std::collections::HashSet::new();
                for key in sizes {
                    if let Some(layout) = v.layouts.get(key) {
                        for id in layout.keys() {
                            if seen.insert(id.clone()) {
                                v.members.push(id.clone());
                            }
                        }
                    }
                }
            }
        }
        // A record whose tile is no longer in any view is stale.
        let views = &self.views;
        self.sync_file_cells.retain(|id, _| views.iter().any(|v| v.layouts.values().any(|m| m.contains_key(id))));
        if !self.views.iter().any(|v| v.id == self.active_view) {
            if let Some(first) = self.views.first() {
                self.active_view = first.id.clone();
            }
        }
        // Last, so values the steps above wrote are held to the same limits.
        clamp_ui_values(&mut self.ui_values);
    }

    pub(crate) fn active_index(&self) -> usize {
        self.views
            .iter()
            .position(|v| v.id == self.active_view)
            .unwrap_or(0)
    }

    pub(crate) fn active_view_mut(&mut self) -> &mut View {
        let i = self.active_index();
        &mut self.views[i]
    }
}

// Whether an install is marked as having seen the first-run tour, so it is never sprung on
// an existing user. Mirrors seed_example_prompts' freshness check (prompts exist, or the
// example-seeding marker is set, which counts even after the examples were deleted). Pure
// rather than a Settings method so it is testable without a Store.
pub(crate) fn tour_should_be_marked_seen(has_prompts: bool, examples_seeded: bool) -> bool {
    has_prompts || examples_seeded
}

// In-memory store, flushed to disk on mutations.
pub(crate) struct Store {
    pub(crate) prompts: Vec<Prompt>,
    // Prompts the user deleted, kept whole (media and version history untouched) until a real
    // purge. Newest-deleted first, like the other "most recent on top" lists.
    pub(crate) trash: Vec<Prompt>,
    pub(crate) settings: Settings,
    // Set when db_load found encrypted prompt rows it could not decrypt (e.g. DPAPI failed
    // after a profile move or password reset). While true, save_prompts refuses to rewrite the
    // prompts table: those rows are missing from the in-memory list, so a write would delete
    // them as "gone". The raw data.db is snapshotted aside on detect.
    pub(crate) prompts_locked: bool,
    // What the prompts/trash tables already hold, so a save re-seals only rows that changed
    // (see WrittenRows). It sits under the same lock as the prompt list: a separate lock could
    // let one window update the map for a list another window is still writing. A RefCell, not a
    // Mutex, since it is a detail of the write path and every save_* takes a shared &Store.
    pub(crate) written: std::cell::RefCell<WrittenRows>,
}

impl Store {
    // Void everything believed to be on disk, so the next save rewrites every row. The keys stay
    // (only the hashes go), so a row this process knew about is still deletable by the first
    // save. Mandatory wherever the tables change without going through write_rows: a wholesale
    // replacement of the prompt set, a swapped or restored data.db, a raw DELETE, a move to
    // another data folder.
    pub(crate) fn forget_written_rows(&self) {
        for hash in self.written.borrow_mut().values_mut() {
            *hash = crate::store::STALE_ROW_HASH;
        }
    }

    // Keeps both folder lists valid against the current prompt list: call after every mutation
    // that can remove a prompt. Idempotent.
    pub(crate) fn normalize_folders(&mut self) {
        let ids: Vec<String> = self.prompts.iter().map(|p| p.id.clone()).collect();
        normalize_float_folders(&mut self.settings.float_folders, &ids);
        let view_ids: Vec<String> = self.settings.views.iter().map(|v| v.id.clone()).collect();
        normalize_tile_folders(&mut self.settings.tile_folders, &ids, &view_ids);
    }
}

pub(crate) type Db = Mutex<Store>;
