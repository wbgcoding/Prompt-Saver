use super::*;

// ---------- Data model ----------

#[derive(Serialize, Deserialize, Clone)]
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
    // Which page of an attached PDF the preview shows (0-based). Ignored for
    // any other attachment type. Clamped against the actual page count at render
    // time (pdf_preview), never here: the file can change independently of this
    // number (a replaced attachment, or a shorter PDF at the same path).
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
    // Unix seconds set the moment a prompt moves into the trash table; unused
    // (0) for every prompt still in store.prompts. Drives both the trash list's
    // "deleted N days ago" and the retention sweep: never touched for anything
    // else, so it never appears in an export or a fresh add_prompt/update_prompt.
    #[serde(default)]
    pub(crate) trashed_at: u64,
    // "Rotating prompts": `text` is entry 1, `variants` holds entries 2..n, so
    // every existing consumer (tile preview, search, export, dup-finder, tooltip,
    // Markdown sync) stays correct without a single change, and "rotation is on" is
    // exactly `!variants.is_empty()`: no separate bool that could drift from the data.
    #[serde(default)]
    pub(crate) variants: Vec<String>,
    // "" = in order, "random" = shuffled bag (see rotation_next).
    #[serde(default)]
    pub(crate) rotate_mode: String,
    // Auto-Wechsel: nach jedem Einfügen rückt die Zwischenablage auf den
    // nächsten Eintrag vor. Ohne variants bedeutungslos (wie rotate_mode).
    #[serde(default)]
    pub(crate) rotate_auto: bool,
    // Duration of an attached video, measured once when the attachment is set
    // (probe_media_meta) and never on every render. 0 = never measured, or the
    // file has no video stream. Read by the library/history thumbnail badge
    // only -- the main grid tile never reads this field.
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
    // Text recognized in the attached picture, a search index and nothing
    // else: never shown, exported or synchronized. Empty unless ocrIndex was
    // on when the picture was attached. Cleared the moment the picture is
    // removed or replaced.
    #[serde(default)]
    pub(crate) image_text: String,
    // Marked confidential: `text` and every entry of `variants` above hold
    // AES-GCM ciphertext (see secret_encrypt) instead of plain text, gated by
    // the session vault password (secret_vault_unlock). Name, tags and image
    // stay plain either way, or the library grid could not draw the tile at
    // all while locked.
    #[serde(default)]
    pub(crate) secret: bool,
}

// A saved floating window position, plus the identity of the screen it was
// on when it was last placed there -- name, resolution and its offset in
// the virtual desktop, next to the coordinates. Coordinates alone cannot
// tell a screen that came back from a different one that merely landed on
// the same spot; `screen` is what lets a parked pill find its way home.
// Empty on anything saved before this field existed, which is read as
// "unknown" rather than "no screen", so an ordinary upgrade never parks a
// button nothing has actually moved (see resolve_float_spot).
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
    // Ordered membership: who belongs to this view, independent of where a
    // grid size happens to have room to show them. `layouts` says WHERE a
    // member sits, never WHETHER it belongs -- that split exists so a
    // grid-size change can never silently drop or resurrect an entry. A
    // member with no cell in the current size's layout is an overflow
    // entry, not a lost one.
    #[serde(default)]
    pub(crate) members: Vec<String>,
}

// An Android-style folder inside the grid. It occupies ONE cell of one view and
// holds prompts that are no longer tiles of their own: a member's layout entry is
// dropped when it joins and it gets a free cell again when it leaves. A view can
// change its grid size, so a folder whose cell no longer exists is moved to the first
// free one instead of disappearing.
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

// An Android-style folder holding several floating pills. It owns ONE window
// (the folder icon; expanding only resizes it), so `pos` is the icon's top-left in
// physical desktop pixels: the same space `Settings.floating` uses. A member keeps
// its own `floating` entry so switching the feature off puts every pill back where
// it was.
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
    // Pill folders. Kept even while the feature is switched off, so turning it
    // back on restores the folders instead of losing them.
    #[serde(default)]
    pub(crate) float_folders: Vec<FloatFolder>,
    // Per-prompt video player state (volume, mute, loop), grid and pill.
    #[serde(default)]
    pub(crate) video_prefs: HashMap<String, VideoPrefs>,
    #[serde(default)]
    pub(crate) window: Option<WindowGeom>,
    #[serde(default = "default_on")]
    pub(crate) minimize_to_tray: bool,
    // False for a store written before the 30/90-day retention defaults; migrate()
    // pins such a store to keep-forever once.
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
    // Global voice-dictation hotkey; empty = off. Checked/cleaned through the
    // exact same functions as `hotkey` above -- see HotkeyOwner.
    #[serde(default)]
    pub(crate) voice_hotkey: String,
    // Global stop/pause hotkeys for a running recording -- the bar is otherwise
    // the ONLY way to stop or pause it. Registered only while a recording is
    // active (see rearm_hotkeys) -- unlike `hotkey`/`voice_hotkey` these ship
    // with a real default, since a long-running, disk-filling capture needs a
    // way out from day one.
    #[serde(default = "default_rec_stop_hotkey")]
    pub(crate) rec_stop_hotkey: String,
    #[serde(default = "default_rec_pause_hotkey")]
    pub(crate) rec_pause_hotkey: String,
    #[serde(default = "default_on")]
    pub(crate) auto_update: bool,
    // Whether the user allowed the automatic update check. A fresh profile
    // starts Unasked and never goes online until answered; a stored settings
    // blob without the key is an existing install (Legacy) and is turned into
    // Yes or No from its `auto_update` value by `migrate`.
    #[serde(default = "legacy_update_consent")]
    pub(crate) update_consent: UpdateConsent,
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
    // Seeded once on a genuinely fresh install (empty library, never seeded
    // before): never re-seeded just because the user later deletes everything.
    #[serde(default)]
    pub(crate) examples_seeded: bool,
    // Ids of the seeded example prompts, so "remove examples" can find exactly
    // those three (and only those) without guessing by name/content.
    #[serde(default)]
    pub(crate) example_prompt_ids: Vec<String>,
    // Content fingerprint of each seeded example at the moment it was created
    // (example_seed_fingerprint). "Remove examples" only deletes an id whose
    // current fingerprint still matches: one the user has since edited is now
    // theirs and is left alone. An id with no entry here (old installs seeded
    // before this field existed) is treated as unchanged, matching the
    // previous unconditional-delete behaviour.
    #[serde(default)]
    pub(crate) example_prompt_seeds: HashMap<String, u64>,
    // Each entry is one group of prompt ids the user dismissed together as
    // "not a duplicate" - never re-suggested as a group. Was Vec<[String; 2]> (one
    // entry per PAIR): ignoring an N-member group wrote N*(N-1)/2 entries, 79800 for
    // a 400-item group. A JSON array of strings deserializes into a Vec<String> the
    // same whether it has 2 elements or more, so every already-stored pair loads
    // here unchanged as a 2-member group - no migration needed.
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
    // Legacy in-blob copy history. Read once on upgrade to migrate into the
    // dedicated `copy_log` table, then never written back (the table owns it so
    // a million-entry history never bloats this encrypted settings blob).
    #[serde(default, skip_serializing)]
    pub(crate) copy_log: Vec<CopyEntry>,
    // Per-prompt copy count + last-used time (unix seconds). Both bounded by the
    // number of prompts, so they stay small in the blob.
    #[serde(default)]
    pub(crate) usage: HashMap<String, u32>,
    #[serde(default)]
    pub(crate) last_used: HashMap<String, u64>,
    // Rotation pointer for "variants" prompts: kept out of Prompt itself (see
    // rotation_next) so every copy doesn't rewrite prompts.json / bump the version
    // history / touch the Markdown sync file. Index of the NEXT entry to serve.
    #[serde(default)]
    pub(crate) rotate_pos: HashMap<String, u32>,
    // Shuffled remaining indices for "random" mode (the "bag": drawn without
    // repetition, reshuffled once empty). Absent/empty means "reshuffle on next use".
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
    // Grid folders. Kept while the feature is off as well: the members simply
    // show as their own tiles again until it comes back on.
    #[serde(default)]
    pub(crate) tile_folders: Vec<TileFolder>,
    // Legacy single-grid fields (pre-views). Read for migration, never written back.
    #[serde(default = "default_cols", rename = "cols", skip_serializing)]
    pub(crate) legacy_cols: u32,
    #[serde(default = "default_rows", rename = "rows", skip_serializing)]
    pub(crate) legacy_rows: u32,
    #[serde(default, rename = "layout", skip_serializing)]
    pub(crate) legacy_layout: HashMap<String, [u32; 2]>,
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
    // Was "Ctrl+Alt+P", identical to the quick-launcher's own default
    // (`hotkey` above) -- on every fresh install the two collided from the
    // start and the pause hotkey silently never registered until the user
    // changed one of them by hand.
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
            // A fresh install ships with the quick-launcher already bound, so the
            // feature is discoverable without a trip to settings. An EXISTING install's
            // stored value (including a deliberately empty/off one) is untouched --
            // #[serde(default)] only ever fills in a value that was never written.
            hotkey: "CommandOrControl+Alt+P".to_string(),
            // Unlike the app hotkey, voice dictation ships with NO global binding --
            // a fresh install is not expected to start listening from a key combo.
            voice_hotkey: String::new(),
            rec_stop_hotkey: default_rec_stop_hotkey(),
            rec_pause_hotkey: default_rec_pause_hotkey(),
            auto_update: true,
            update_consent: UpdateConsent::Unasked,
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
            legacy_cols: default_cols(),
            legacy_rows: default_rows(),
            legacy_layout: HashMap::new(),
        }
    }
}

pub(crate) const GRID_MIN: u32 = 1;
// Hard safety ceilings. The default-facing limits are 20 (enforced in the UI via
// the expert values gridMax / maxViews); these only cap how far those expert
// overrides can be pushed, so old data and extreme settings can't break things.
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
// Brand-owned identifiers. Every product name in the Rust code comes from
// src/brand.rs, which tools/brand.js writes from brands/<brand>.json.
pub(crate) const AUTOSTART_KEY: &str = brand::AUTOSTART_KEY;
// Local data folder under %APPDATA%\Roaming. Used by the pre-app panic hook
// (no AppHandle) and the store path; deliberately NOT the bundle identifier.
pub(crate) const DATA_FOLDER: &str = brand::DATA_FOLDER;

pub(crate) fn grid_key(cols: u32, rows: u32) -> String {
    format!("{}x{}", cols, rows)
}

// WebView2-missing dialog texts; shown before settings exist, so the OS
// locale decides (same language set as the UI). `{app}` is filled with the
// product name once, in webview2_dialog: the translations never spell it.
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

// Resolve the effective UI language code ("auto" -> OS locale), EN fallback.
// Supported UI languages besides English (keep in sync with ui/i18n.js).
pub(crate) const LANG_CODES: [&str; 10] = [
    "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro",
];

pub(crate) fn resolve_lang(pref: &str) -> &'static str {
    let raw = if pref != "auto" {
        pref.to_string()
    } else {
        sys_locale::get_locale().unwrap_or_default()
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

// Voice-hotkey feedback while the window may stay hidden: the tray tooltip
// switches to a recording state for the length of a dictation session, then
// back to the app name. Runs for every session regardless of what started it
// (button or hotkey) -- harmless to set even while the window is visible.
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

// The tray's own Stop/Pause/Resume labels for a running recording -- the
// tray is drawn by the OS, not the webview, so (like every other tray label
// above) these live here rather than in ui/i18n.js.
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

// Headers of the two tray quick-copy sections. The tray is drawn by the OS,
// not by the webview, so these live next to the other tray labels instead of in
// the frontend table.
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

// Placeholder text of the tray menu's own filter row -- same reasoning as
// tray_labels above: the tray window is a webview, but its text still lives
// here so every tray string is translated in one place.
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

// Localized default name of the auto-created start page. As long as the user
// never renamed it, it follows the UI language (see set_language).
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

// Expert-menu values that no longer exist. A loaded store drops them once, so a
// key that is added back later never inherits a stale stored value.
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
    ]
}

// Expert switches that no longer exist. A loaded store drops them once, like removed values.
pub(crate) fn removed_ui_flag_keys() -> &'static [&'static str] {
    &[
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
    ]
}

// Expert dropdown choices (stored as text) that no longer exist; dropped like removed values.
pub(crate) fn removed_ui_text_keys() -> &'static [&'static str] {
    &["snipScaleFilter", "fuzzyMaxTypos"]
}

// Holds every numeric expert value to the range its slider enforces (the table
// is generated from ui/expertConfig.js). A hand-edited settings file or an
// import can carry any number; a value that is not finite is dropped so the
// key falls back to its default, and a key whose top stop means "unlimited"
// keeps its 0.
pub(crate) fn clamp_ui_values(map: &mut HashMap<String, f64>) {
    // A number that is not finite would be written as null and make the whole
    // settings block unreadable on the next start, whatever key it sits under.
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

// A preset-or-custom select holds its presets or a whole number from the free-entry
// field's range. Anything below one (fractions, negatives) means "never" where the
// select has a zero preset (the safe reading for a retention that deletes things)
// and the smallest custom value where it has none; the rest is rounded and capped.
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
        // Four on/off switches each had a duration beside them. The duration is the
        // only control now and 0 means off: a switch that was stored off becomes a
        // stored 0. A switch that was on (or never set) leaves the stored or default
        // duration as it is.
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
        // Five separate search-list caps became one, "searchListMax". The largest value
        // a user set wins, so nobody loses rows they asked for; a cap that was never
        // stored counts with the default it showed before, so lowering one cap does
        // not shrink the lists the other four kept at their defaults. A stored 0
        // ("keep no recent searches") cannot be kept: the merged list has no off
        // stop. A value already stored under the new key is never overwritten.
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
        // Tile lift on hover was a highlight switch, a lift switch and a distance
        // (the lift was on unless stored off). It is one distance now, 0 = off, and
        // the new default is 0. An install that stored none of the three keys takes
        // the new default; one that stored any of them keeps exactly the lift it
        // shows today, where an unstored switch counts as its old default, on.
        // Only while an old switch is still stored: afterwards "hoverLift" is the
        // distance alone and a stored 0 (off) must survive every launch.
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
        // Three dim-until-hovered switches (tiles, media, top bar) with two opacity
        // sliders became one strength. Whoever dimmed anything keeps the strongest dim
        // they had, now on all three. Never overwrites a stored strength.
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
        // Seven look switches for tile text became one choice, "tileStyle". Styles that
        // were combined cannot be kept by a single choice: the first one in this order
        // wins. A choice already stored is never overwritten.
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
        // The favorites view used to be two switches: "favView" (settings) and
        // "favViewButton" (header star, on unless stored false). It is one now,
        // "favViewButton", off by default. The view stays on exactly when it was
        // visible before: favView on and the star not hidden. Runs only while
        // favView is stored, and removes it, so it happens once.
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
        // The expert drawing defaults of the screenshot editor became the sizes each
        // tool remembers. Whoever had set one keeps it as that tool's starting size,
        // unless sizes are already remembered. Values outside a field's range are not
        // carried (the editor would ignore them anyway).
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
        for k in removed_ui_value_keys() {
            self.ui_values.remove(*k);
        }
        for k in removed_ui_flag_keys() {
            self.ui_flags.remove(*k);
        }
        for k in removed_ui_text_keys() {
            self.ui_texts.remove(*k);
        }
        // The privacy switch used to be "hide from screen capture" (on unless stored
        // false); it is now "allow screenshots" (off unless stored true). Anyone who
        // had deliberately turned the hiding OFF keeps their screenshots.
        if let Some(hidden) = self.ui_flags.remove("captureExclusion") {
            if !hidden {
                self.ui_flags.insert(ALLOW_SCREENSHOT_KEY.to_string(), true);
            }
        }
        // The "ai" theme was renamed to "neon"; no "ai" anywhere, even
        // internally. Rewrite a stored preference so nobody's colours reset to
        // "system" on update.
        if self.theme == "ai" {
            self.theme = "neon".to_string();
        }
        for c in self.custom_themes.iter_mut() {
            if c.base == "ai" {
                c.base = "neon".to_string();
            }
        }
        // The Black & White theme was removed: it was a light one, so whoever used it
        // (as the theme itself or as the base of a custom theme) gets Light, with the
        // custom theme's own colours kept.
        if self.theme == "mono" {
            self.theme = "light".to_string();
        }
        for c in self.custom_themes.iter_mut() {
            if c.base == "mono" {
                c.base = "light".to_string();
            }
        }
        if self.views.is_empty() {
            let mut layouts = HashMap::new();
            if !self.legacy_layout.is_empty() {
                layouts.insert(
                    grid_key(self.legacy_cols, self.legacy_rows),
                    self.legacy_layout.clone(),
                );
            }
            let members = layouts.get(&grid_key(self.legacy_cols, self.legacy_rows))
                .map(|l| l.keys().cloned().collect())
                .unwrap_or_default();
            self.views.push(View {
                id: gen_id(),
                name: home_name(resolve_lang(&self.language)).to_string(),
                cols: self.legacy_cols,
                rows: self.legacy_rows,
                layouts,
                color: String::new(),
                members,
            });
        }
        // Every view arrives through here, including from an imported backup, so this
        // is where the same ceilings the commands enforce have to hold. Without them a
        // hand-edited backup with cols near u32::MAX makes the grid renderer build
        // billions of cells and the window never paints again.
        self.views.truncate(MAX_VIEWS);
        for v in self.views.iter_mut() {
            v.cols = v.cols.clamp(GRID_MIN, GRID_MAX);
            v.rows = v.rows.clamp(GRID_MIN, GRID_MAX);
            // One-time backfill for a view saved before membership tracking existed: derive
            // membership from the union of every saved grid size (current
            // size first) instead of leaving it empty, which would make
            // every existing entry look freshly hidden. Never runs again
            // once `members` is populated, and never used as a source of
            // truth after this.
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

// Whether an install should be marked as having already seen the first-run
// tour, so it is never sprung on an existing user. Mirrors seed_example_prompts'
// own freshness check exactly (real prompts already exist, OR the example-seeding
// marker is set -- the latter still counts a store as non-fresh even if its
// examples were later dismissed/deleted). A pure function, not a Settings method,
// so it is testable without a whole Store around it (called from load_store, once
// prompts are known, right after the Store itself is built).
pub(crate) fn tour_should_be_marked_seen(has_prompts: bool, examples_seeded: bool) -> bool {
    has_prompts || examples_seeded
}

// In-memory store, flushed to disk on mutations.
pub(crate) struct Store {
    pub(crate) prompts: Vec<Prompt>,
    // Prompts the user deleted, kept whole (media + version history untouched)
    // until a real purge. Newest-deleted first, matching how every other list in
    // the app (recent copies, history) already orders "most recent on top".
    pub(crate) trash: Vec<Prompt>,
    pub(crate) settings: Settings,
    // Set when db_load found encrypted prompt rows it could NOT decrypt (e.g. DPAPI
    // failed after a profile move / password reset). While true, save_prompts refuses
    // to rewrite the prompts table so a rewrite can never destroy the rows that are
    // present-but-unreadable (they are missing from the in-memory list, so a write
    // would delete them as "gone"). The raw data.db is snapshotted aside on detect.
    pub(crate) prompts_locked: bool,
    // What the prompts/trash tables already hold, so a save only re-seals the rows
    // that really changed (see WrittenRows). It belongs to the prompt list and sits
    // under the same lock rather than beside it: a second lock could be taken apart
    // from this one, letting one window update the map for a list another window is
    // still writing. A RefCell, not a Mutex: it is a detail of the write path, and
    // every save_* takes a shared &Store.
    pub(crate) written: std::cell::RefCell<WrittenRows>,
}

impl Store {
    // Forget everything believed to be on disk, so the next save rewrites every row.
    // Mandatory wherever the tables change without going through write_rows: a
    // wholesale replacement of the prompt set, a swapped or restored data.db, a raw
    // DELETE, a move to another data folder. Missing it anywhere leaves the process
    // trusting hashes for content the disk no longer holds.
    pub(crate) fn forget_written_rows(&self) {
        self.written.borrow_mut().clear();
    }

    // Keep both folder lists valid against the CURRENT prompt list: one call
    // after every mutation that can remove a prompt. Idempotent.
    pub(crate) fn normalize_folders(&mut self) {
        let ids: Vec<String> = self.prompts.iter().map(|p| p.id.clone()).collect();
        normalize_float_folders(&mut self.settings.float_folders, &ids);
        let view_ids: Vec<String> = self.settings.views.iter().map(|v| v.id.clone()).collect();
        normalize_tile_folders(&mut self.settings.tile_folders, &ids, &view_ids);
    }
}

pub(crate) type Db = Mutex<Store>;
