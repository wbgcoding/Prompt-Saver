// Application backend (Tauri v2). Local SQLite storage, clipboard, import/export,
// multiple views, frameless floating quick-copy windows. No network by default:
// the only outbound calls are the switchable update check and, only when
// WebView2 itself is missing, its consented runtime download. The one inbound
// exception is the local remote-access receiver (remote.rs) -- off by default,
// password-protected, loopback-only unless its own separate switch is on.

// clippy::pedantic and clippy::nursery were run once as a deliberately
// stricter pass, not adopted as standing lints. Together they raised ~1080
// warnings, almost all pure style noise for this codebase specifically: heavy
// Win32 FFI and image-processing code makes hundreds of `as` casts
// (cast_possible_truncation/sign_loss/wrap/lossless) intentional and already
// domain-bounded; most Tauri #[tauri::command] handlers correctly take their
// serde-deserialized arguments by value (needless_pass_by_value); and the rest
// (redundant_closure_for_method_calls, use_self, uninlined_format_args, ...) is
// cosmetic with no behavioural difference. Turning all of that on would mean a
// ~1000-site rewrite of an already-shipping file for zero bug-fixing value and
// real regression risk. One genuinely worth a look, clippy::float_cmp, was
// checked at its two real hits (rgb_to_hsl) and is a documented false positive
// there, not a bug. clippy::significant_drop_tightening (62 hits: a lock held
// longer than the critical section needs) is the one category with real
// contention/freeze relevance -- the worst offenders are sampled by the
// blocking-call audit rather than by a lint sweep.

pub mod brand;
mod camera;
mod demo_text;
mod cli;
mod remote;
mod remote_tls;

use image::imageops::FilterType;
use rusqlite::{params, Connection, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::menu::{IconMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, Modifiers};

mod main_window;
use main_window::*;

mod shortcuts;
use shortcuts::*;

mod var_prompt;
use var_prompt::*;

mod tray;
use tray::*;

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
    ]
}

impl Settings {
    pub(crate) fn has_removed_ui_values(&self) -> bool {
        removed_ui_value_keys().iter().any(|k| self.ui_values.contains_key(*k))
            || removed_ui_flag_keys().iter().any(|k| self.ui_flags.contains_key(*k))
    }

    // Ensure at least one view exists; migrate legacy single-grid data.
    pub(crate) fn migrate(&mut self) {
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

// ---------- Paths + persistence ----------

pub(crate) fn default_data_dir(app: &AppHandle) -> PathBuf {
    // Portable build keeps its data beside the exe so it travels with the folder;
    // installed builds use %APPDATA%\<DATA_FOLDER>. If the exe folder is not
    // writable (e.g. run from a read-only location), fall back to AppData.
    if is_portable() {
        if let Some(dir) = portable_data_dir() {
            if fs::create_dir_all(&dir).is_ok() {
                return dir;
            }
        }
    }
    let dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join(DATA_FOLDER))
        .or_else(|| app.path().app_data_dir().ok())
        .unwrap_or_else(|| std::env::temp_dir().join(DATA_FOLDER));
    let _ = fs::create_dir_all(&dir);
    migrate_legacy_data(app, &dir);
    dir
}

// Portable data lives directly beside the exe (no sub-folder) so a single
// portable download stays self-contained and the store travels with it.
pub(crate) fn portable_data_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|d| d.to_path_buf())
}

// Files that make up a store, in copy order.
pub(crate) const STORE_FILES: [&str; 3] = ["data.db", "prompts.json", "settings.json"];

// The installed copy's store, if this is a portable run and one exists elsewhere.
// Nothing is copied here: the app asks first (see `takeover_offer`), in its own
// window, so the question inherits the theme and language instead of being a bare
// system box before anything is loaded.
pub(crate) fn appdata_store_dir(app: &AppHandle, portable: &std::path::Path) -> Option<PathBuf> {
    let src = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join(DATA_FOLDER))
        .or_else(|| app.path().app_data_dir().ok())?;
    if src.as_path() == portable {
        return None;
    }
    STORE_FILES.iter().any(|f| src.join(f).exists()).then_some(src)
}

// One-time move of the store out of the old identifier-based folder
// (brand::LEGACY_DATA_FOLDER) into DATA_FOLDER, so the rename never loses data.
// Both the identifier folder Tauri resolves and the frozen legacy name are tried:
// the second one survives even if Tauri ever changes how app_data_dir is built.
pub(crate) fn migrate_legacy_data(app: &AppHandle, new: &std::path::Path) {
    if new.join("data.db").exists() {
        return;
    }
    let mut olds: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = app.path().app_data_dir() {
        olds.push(dir);
    }
    if !brand::LEGACY_DATA_FOLDER.is_empty() {
        if let Some(roaming) = std::env::var_os("APPDATA") {
            olds.push(PathBuf::from(roaming).join(brand::LEGACY_DATA_FOLDER));
        }
    }
    for old in olds {
        if old.as_path() == new || !STORE_FILES.iter().any(|f| old.join(f).exists()) {
            continue;
        }
        for f in STORE_FILES {
            if old.join(f).exists() {
                let _ = fs::copy(old.join(f), new.join(f));
            }
        }
        return;
    }
}

// Portable build = run without the installer's uninstall.exe beside the exe
// (standard installs write one; portable mode and dev runs do not). Portable
// always fully closes on window-close; installed honours the tray setting.
pub(crate) fn is_portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| !d.join("uninstall.exe").exists()))
        .unwrap_or(true)
}

// Shared by data_dir() and the command line's headless resolver below: a
// "datapath" pointer file in the canonical dir can redirect the store to a
// user-chosen folder (expert menu); anything missing or invalid falls back to
// the canonical dir, so data is never lost.
pub(crate) fn resolve_datapath_redirect(base: PathBuf) -> PathBuf {
    if let Ok(s) = fs::read_to_string(base.join("datapath")) {
        let p = PathBuf::from(s.trim());
        if !s.trim().is_empty() && p.is_dir() {
            return p;
        }
    }
    base
}

// Effective store location. A "datapath" pointer file in the canonical dir can
// redirect the store to a user-chosen folder (expert menu); on anything missing
// or invalid it falls back to the canonical dir, so data is never lost.
pub(crate) fn data_dir(app: &AppHandle) -> PathBuf {
    let resolved = resolve_datapath_redirect(default_data_dir(app));
    // Recorded for `append_crash_log`'s scrubbing, see `LAST_KNOWN_DATA_DIR`.
    *LAST_KNOWN_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(resolved.clone());
    resolved
}

// The command line's own copy of default_data_dir(): the same portable/APPDATA
// resolution, minus the AppHandle-only fallback (app.path().app_data_dir(),
// which default_data_dir only reaches when APPDATA itself is unset) and the
// legacy-folder migration (a no-op the moment data.db already exists, which it
// does for every install a command-line launch would ever find). A launch with
// no AppHandle in reach at all -- there is no window to build one for -- can
// still resolve the exact folder an ordinary launch would use.
pub(crate) fn default_data_dir_headless() -> PathBuf {
    if is_portable() {
        if let Some(dir) = portable_data_dir() {
            if fs::create_dir_all(&dir).is_ok() {
                return dir;
            }
        }
    }
    let dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join(DATA_FOLDER))
        .unwrap_or_else(|| std::env::temp_dir().join(DATA_FOLDER));
    let _ = fs::create_dir_all(&dir);
    dir
}

pub(crate) fn data_dir_headless() -> PathBuf {
    resolve_datapath_redirect(default_data_dir_headless())
}

// The store's JSON files (prompts.json / settings.json: the DB-write fallback and the
// legacy pre-SQLite format) hold full prompt bodies and private notes, so they are
// DPAPI-sealed exactly like the SQLite rows. `dec_str` passes an un-prefixed value
// through unchanged, which is what still lets a genuinely old plaintext install be
// migrated in; it is re-sealed the next time it is written.
// "Missing" (nothing there yet, safe to treat as empty) and "unreadable" (a file IS
// there but DPAPI/JSON decoding failed: wrong Windows account, corruption) are
// different situations: a caller that folds both into a bare default silently
// replaces real, still-present data with an empty one.
pub(crate) enum SealedRead<T> {
    Missing,
    Unreadable,
    Loaded(T),
}

pub(crate) fn read_sealed_json<T: for<'de> Deserialize<'de>>(path: &PathBuf) -> SealedRead<T> {
    match fs::read_to_string(path) {
        Ok(raw) if !raw.trim().is_empty() => match serde_json::from_str(&dec_str(&raw)) {
            Ok(v) => SealedRead::Loaded(v),
            Err(_) => SealedRead::Unreadable,
        },
        Ok(_) => SealedRead::Missing, // empty file: nothing meaningful was ever written
        Err(_) => SealedRead::Missing,
    }
}

// Every import reads a file the user picked (which may have come from someone else)
// fully into memory before parsing, so a wrong pick (a disk image, a video) has to fail
// with a clear message instead of exhausting memory. Mirrors the cap the decompression
// path already applies via `inflate_capped`.
pub(crate) const MAX_IMPORT_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MAX_THEME_BYTES: u64 = 4 * 1024 * 1024; // a theme is a handful of colour values

pub(crate) fn size_error(max: u64) -> String {
    format!("file is too large; the limit is {} MB", max / (1024 * 1024))
}

// For the one case that cannot read the bytes itself: the image handed to Windows'
// text recognizer is opened by Windows, so measuring it up front is the only check
// available: and a plausible reject message is worth more there than none.
pub(crate) fn check_import_size(path: &std::path::Path, max: u64) -> Result<(), String> {
    let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > max {
        return Err(format!(
            "file is too large ({} MB); the limit is {} MB",
            len / (1024 * 1024),
            max / (1024 * 1024)
        ));
    }
    Ok(())
}

// One open, one read, one decision. Measuring the file and then reading it means
// two separate opens: the file can grow (or be swapped for a different one)
// in between, and the limit then describes a file that is no longer the one being
// read. Reading one byte past the limit is what makes "too large" provable from
// the bytes actually in hand.
pub(crate) fn read_file_capped(path: &std::path::Path, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    file.take(max + 1).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    if buf.len() as u64 > max {
        return Err(size_error(max));
    }
    Ok(buf)
}

// The text counterpart, for the Markdown sync folder: the files there belong to the
// user and nothing stops one of them from being enormous, while the folder is
// rescanned on a timer. Anything past the limit is skipped rather than read into
// memory on every single cycle.
pub(crate) fn read_text_capped(path: &std::path::Path, max: u64) -> Option<String> {
    String::from_utf8(read_file_capped(path, max).ok()?).ok()
}

// Atomic write: temp file then rename, so a crash never truncates data. Returns
// whether the data actually reached disk, so callers can log a persist failure.
// The JSON pair is only the fallback for a failed database write. Once the
// database has accepted the same rows, a file left over from an earlier failed
// write is stale and would be read back as the truth on the next launch (see
// load_store), overwriting what was just saved.
pub(crate) fn drop_stale_fallback(dir: &Path, file: &str) {
    let path = dir.join(file);
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

pub(crate) fn write_sealed_json<T: Serialize>(path: &PathBuf, data: &T) -> bool {
    if let Ok(json) = serde_json::to_string(data).map(|j| enc_str(&j)) {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, json).is_ok() {
            return fs::rename(&tmp, path).is_ok();
        }
    }
    false
}

// Never truncate an existing file in place: a crash or a cloud-sync client
// reading mid-write would otherwise see a valid frontmatter head with a truncated
// body, and a later poll then imports that as truth. Temp-then-rename next to the
// target, same pattern write_sealed_json already uses. The temp name deliberately
// does not carry the caller's own extension (path.md -> path.tmp), so a filter that
// only touches one specific extension (sync_sweep_orphans' ".md" check) leaves a
// leftover temp file alone if the rename is ever interrupted.
pub(crate) fn write_file_atomic(path: &Path, content: &str) -> bool {
    write_bytes_atomic(path, content.as_bytes())
}

// The same, for bytes that are not necessarily text (a conflict copy keeps a file
// exactly as it was, whatever encoding an editor saved it in).
pub(crate) fn write_bytes_atomic(path: &Path, content: &[u8]) -> bool {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).is_ok() && fs::rename(&tmp, path).is_ok()
}

// Capitalise a brand word for a dialog title ("prompts" -> "Prompts").
pub(crate) fn title_case(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// ---------- SQLite store ----------
// Prompts live one-per-row (ordered); settings are a single JSON row. The DB
// is the source of truth; the legacy JSON files are imported once on upgrade
// and otherwise only used as a fallback if the DB cannot be opened.

// At-rest encryption for the local store. DPAPI (Windows, per-user, no password): the
// same Windows account always decrypts; copied elsewhere it can't be read. Falls back to
// plaintext if DPAPI is unavailable so data is never lost; legacy plaintext rows are
// re-encrypted on the next save (see load_store).
pub(crate) const ENC_PREFIX: &str = "enc:v1:";
// A NUL byte can never occur in text a user actually typed or pasted, so it is
// safe as an escape marker: the plaintext-fallback path below prepends it only
// when the plaintext itself happens to start with ENC_PREFIX (or, recursively,
// with this marker), so dec_str can tell "real ciphertext" from "plaintext that
// coincidentally looks like the marker" instead of misreading the latter as
// corrupt ciphertext and erasing it to "".
pub(crate) const PLAINTEXT_ESCAPE: &str = "\u{0}";

#[cfg(windows)]
mod store_crypt;

// Whether every write so far this run really went out sealed. Starts true and is
// flipped exactly once, by the plaintext fallback in enc_str below. The window reads
// it at startup through the `store_sealed` command and listens for `store-unsealed`
// for the rest of the session, so a failure that happens while the app is already
// open still reaches the user instead of only the log file.
//
// Deliberately never flips back: a row written in plaintext during the outage stays
// exactly that way on disk until something rewrites ITS content (write_rows only
// re-seals a row whose content actually changed, see "Diff writes"), so "the profile
// is repaired now" does not mean every row is sealed again -- only the ones touched
// from that point on. Clearing the flag on the first later success would tell the
// user the problem is fully gone while old plaintext rows could still be sitting on
// disk untouched; leaving it sticky for the rest of the session is the honest answer.
pub(crate) static STORE_SEALED: AtomicBool = AtomicBool::new(true);
// Set once the app handle exists; None while the store is still being loaded, which
// is fine: no window exists yet to receive an event, and the flag above already
// carries the state for the startup question.
//
// A closure rather than the AppHandle itself, deliberately: `enc_str` is linked into
// the unit-test binary, and a direct `handle.emit(...)` in its body pulls the whole
// webview event path (and the WebView2 import that comes with it) into a binary
// that has no webview beside it, which then fails to LOAD at all (every test
// "fails" before one of them runs). Behind this indirection the emit lives in the
// closure built in `run()`, which no test links.
pub(crate) type SealNotifier = Arc<dyn Fn() + Send + Sync>;
pub(crate) static SEAL_NOTIFY: Mutex<Option<SealNotifier>> = Mutex::new(None);

// Encrypt a JSON string for storage (prefix + base64 ciphertext).
//
// A DPAPI failure falls back to plaintext rather than refusing the write: hard-failing
// here would mean the user cannot save at all, trading a confidentiality problem for
// guaranteed data loss. It must never be SILENT though: a broken Windows profile
// would otherwise downgrade the whole store from sealed to readable with nothing to
// show for it. Logged and announced once per run (this runs per row; a full save
// would otherwise flood both the log and the window).
// Test-only escape hatch: forces the plaintext fallback below without needing a
// real DPAPI failure, which nothing in a unit test can induce. Thread-local, not a
// shared global: cargo test runs each test on its own thread, and a process-wide
// flag here would force EVERY other concurrently running test's enc_str calls into
// the fallback branch too, not just the one deliberately overriding it -- stealing
// STORE_SEALED's one-shot true-to-false transition out from under it. Compiled only
// into the test binary, so release behaviour is exactly what it was before this
// existed.
#[cfg(test)]
thread_local! {
    pub(crate) static FORCE_PLAINTEXT_FALLBACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn enc_str(plain: &str) -> String {
    #[cfg(windows)]
    {
        #[cfg(test)]
        let dpapi_ok = !FORCE_PLAINTEXT_FALLBACK.with(|f| f.get());
        #[cfg(not(test))]
        let dpapi_ok = true;
        if dpapi_ok {
            if let Some(ct) = store_crypt::protect(plain.as_bytes()) {
                return format!("{}{}", ENC_PREFIX, base64_encode(&ct));
            }
        }
        if STORE_SEALED.swap(false, Ordering::Relaxed) {
            append_crash_log(
                "DPAPI encryption unavailable - stored data is being written UNENCRYPTED. \
                 Check the Windows user profile; a repaired profile re-seals on the next save.",
            );
            // Cloned out of the lock first: notifying under it would hold the mutex
            // across a webview round trip for no reason.
            let notify = SEAL_NOTIFY.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(notify) = notify {
                notify();
            }
        }
    }
    if plain.starts_with(ENC_PREFIX) || plain.starts_with(PLAINTEXT_ESCAPE) {
        return format!("{PLAINTEXT_ESCAPE}{plain}");
    }
    plain.to_string()
}

// The state behind the startup warning: true while everything on disk is sealed,
// false once a write really had to fall back to plaintext this run.
#[tauri::command]
fn store_sealed() -> bool {
    STORE_SEALED.load(Ordering::Relaxed)
}

// Set at load_store() when the settings row was PRESENT but could not be read back
// or parsed (query error, decrypt failure, corrupt JSON) -- as opposed to genuinely
// absent, which is just a fresh install and never sets this. While true,
// write_settings_now refuses to rewrite the row: the in-memory settings are already
// Settings::default() at that point, and writing them would permanently overwrite
// whatever is actually sitting in the corrupt row with defaults. Mirrors
// Store.prompts_locked, but process-global rather than a Store field because
// save_settings/write_settings_now are the one choke point every settings mutation
// already funnels through, without a &Store to read a field from.
pub(crate) static SETTINGS_LOCKED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn settings_locked() -> bool {
    SETTINGS_LOCKED.load(Ordering::Relaxed)
}

// Whether the LAST settings write actually reached disk. Flips true the moment
// write_settings_now exhausts both the database and the JSON fallback, and back
// to false the moment a write succeeds again -- so it always reflects "is the
// in-memory state ahead of what is on disk right now", not a sticky one-shot
// like STORE_SEALED. The window reads it at startup (nothing to report from a
// clean run) and gets the live flips through the existing "persist-failed"
// event, which write_settings_now already emits with the "settings" payload --
// no separate notifier plumbing needed here, unlike enc_str's SEAL_NOTIFY: this
// function already takes an AppHandle directly, so it can just emit.
pub(crate) static SETTINGS_WRITE_FAILED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn settings_write_failed() -> bool {
    SETTINGS_WRITE_FAILED.load(Ordering::Relaxed)
}

// ---- Data folder lock ------------------------------------------------------
// The data folder can be pointed at a synchronised folder (OneDrive, Dropbox,
// Google Drive, iCloud Drive, Nextcloud), and the worst case for a SQLite file
// under one of those is a corrupted database once two machines write to it at
// close to the same time. `tauri_plugin_single_instance` already rules out a
// second process on the SAME machine; this only has to rule out a second one on
// a DIFFERENT machine sharing the same synced file.
//
// A small lock file (host name, process id, last-seen timestamp) sits beside
// the database, refreshed every DATA_DIR_LOCK_REFRESH_SECS by the scheduler job
// registered in run(). A machine that finds another host's lock fresh opens the
// store read-only for the rest of the session and says so in the window (a
// dedicated persistent line, not the shared expert-menu notice list, since a
// user who never opens the expert menu still has to know their edits are not
// being saved). A lock older than three missed refreshes is taken over without
// asking: the alternative is a crash on one machine locking every other one out
// permanently. This mirrors the settings write-lock (`SETTINGS_LOCKED`) as its
// model -- a process-global flag rather than a `Store` field, because the save
// functions below are the one choke point every prompt/settings write already
// funnels through, the same way the prompts_locked valve already gates every
// diff write path.
pub(crate) const DATA_DIR_LOCK_FILE: &str = "datadir.lock.json";
pub(crate) const DATA_DIR_LOCK_REFRESH_SECS: u64 = 30;
// Three missed refreshes: long enough that one slow tick under load is never
// mistaken for a crash, short enough that a real crash does not lock the other
// machine out of its own data for long.
pub(crate) const DATA_DIR_LOCK_STALE_SECS: u64 = DATA_DIR_LOCK_REFRESH_SECS * 3;

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct DataDirLockInfo {
    pub(crate) host: String,
    pub(crate) pid: u32,
    pub(crate) seen: u64,
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// Windows always sets COMPUTERNAME for the logged-in session; a missing value
// (never observed in practice) falls back to a label that still reads sensibly
// in the read-only banner rather than an empty string.
pub(crate) fn local_host_name() -> String {
    std::env::var("COMPUTERNAME").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "this computer".to_string())
}

pub(crate) fn data_dir_lock_path(dir: &Path) -> PathBuf {
    dir.join(DATA_DIR_LOCK_FILE)
}

pub(crate) fn read_data_dir_lock(path: &Path) -> Option<DataDirLockInfo> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

pub(crate) fn write_data_dir_lock(path: &Path, info: &DataDirLockInfo) -> bool {
    serde_json::to_string(info).is_ok_and(|json| fs::write(path, json).is_ok())
}

// A lock naming THIS machine is never "foreign", whatever its pid or age -- a
// crash-and-restart on the same computer is not the two-machine case this
// guards against, and the single-instance plugin already rules out a genuine
// second process here.
pub(crate) fn lock_is_ours(existing: &DataDirLockInfo) -> bool {
    existing.host == local_host_name()
}

pub(crate) fn lock_is_stale(existing: &DataDirLockInfo) -> bool {
    unix_now().saturating_sub(existing.seen) >= DATA_DIR_LOCK_STALE_SECS
}

pub(crate) static DATA_DIR_READONLY: AtomicBool = AtomicBool::new(false);
pub(crate) static DATA_DIR_LOCK_HOST: Mutex<Option<String>> = Mutex::new(None);

// Set by enable_demo_mode()/disable_demo_mode() once each has flushed
// whatever it needs to flush and is about to flip the "datapath" redirect
// (to the demo folder, or back to the real one) and call app.restart().
// Between that flip and the process actually exiting, the exit's own
// RunEvent::Exit flush (or a 250ms settings_flush_tick landing in the same
// window) -- or, on the enable side, any ordinary command still in flight --
// must not be allowed to write anything: at that point db_conn()/data_dir()
// already resolve to the OTHER store than the one just flushed, and a write
// there would land real data in the demo store (or demo data in the real
// one). Reset to false on a failed flip (see both commands' own Err paths) --
// left on, every save in the process would silently fail forever even though
// the switch never actually completed. On success it is never cleared
// explicitly: the process it belongs to is about to be replaced by
// app.restart(), and the fresh process starts with the default `false` again.
pub(crate) static DEMO_TRANSITION_LOCKED: AtomicBool = AtomicBool::new(false);

// Decides read-only vs. owned for `dir` and, if owned, claims it. Called once at
// load_store() and again after set_data_dir() points the app at a different
// folder. The `dataDirLock` toggle (default on) skips all of this when off,
// restoring today's behaviour with no lock file and no read-only state.
pub(crate) fn acquire_data_dir_lock(dir: &Path, enabled: bool) {
    if !enabled {
        DATA_DIR_READONLY.store(false, Ordering::Relaxed);
        *DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()) = None;
        return;
    }
    let path = data_dir_lock_path(dir);
    let existing = read_data_dir_lock(&path);
    let foreign_and_fresh = existing.as_ref().is_some_and(|e| !lock_is_ours(e) && !lock_is_stale(e));
    if foreign_and_fresh {
        DATA_DIR_READONLY.store(true, Ordering::Relaxed);
        *DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()) = existing.map(|e| e.host);
        return;
    }
    DATA_DIR_READONLY.store(false, Ordering::Relaxed);
    *DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()) = None;
    write_data_dir_lock(&path, &DataDirLockInfo { host: local_host_name(), pid: std::process::id(), seen: unix_now() });
}

// The scheduler tick (see "data-dir-lock" in run()). A read-only process must
// never touch this file -- it belongs to the other machine's session, and
// overwriting it would erase the very lock this process is refusing to ignore.
pub(crate) fn refresh_data_dir_lock(dir: &Path) {
    if DATA_DIR_READONLY.load(Ordering::Relaxed) {
        return;
    }
    write_data_dir_lock(&data_dir_lock_path(dir), &DataDirLockInfo { host: local_host_name(), pid: std::process::id(), seen: unix_now() });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataDirLockStatus {
    pub(crate) read_only: bool,
    pub(crate) other_host: Option<String>,
}

// Polled once at startup (init(), the same shape as store_sealed/settings_locked)
// to fill in the persistent read-only banner before the first render.
#[tauri::command]
fn data_dir_lock_status() -> DataDirLockStatus {
    DataDirLockStatus {
        read_only: DATA_DIR_READONLY.load(Ordering::Relaxed),
        other_host: DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    }
}

// The single choke point save_prompts/save_prompts_and_trash/save_trash/
// write_settings_now all call first: real refusal, not a silent no-op -- every
// blocked write emits the same event with the same host name, so the window
// can show the same clear message no matter which action triggered it.
pub(crate) fn data_dir_write_blocked(app: &AppHandle) -> bool {
    // No event emitted here (unlike the readonly case below): the window is
    // already on its way down for app.restart(), and this is not a condition
    // the user needs a banner about.
    if DEMO_TRANSITION_LOCKED.load(Ordering::Relaxed) {
        return true;
    }
    if !DATA_DIR_READONLY.load(Ordering::Relaxed) {
        return false;
    }
    let host = DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let _ = app.emit("data-dir-readonly-write-blocked", host);
    true
}

// Known synchronised-folder conventions: the environment variable a client
// publishes when it has one (more reliable than a folder name a business tenant
// can rename in Explorer without moving the actual sync root), then the folder
// name itself. Returns the provider's display name, or None for an ordinary
// local folder.
pub(crate) fn sync_folder_hint(path: &Path) -> Option<&'static str> {
    let lower = path.to_string_lossy().to_lowercase();
    for var in ["OneDriveCommercial", "OneDriveConsumer", "OneDrive"] {
        if let Some(root) = std::env::var_os(var) {
            let root_lower = root.to_string_lossy().to_lowercase();
            if !root_lower.is_empty() && lower.starts_with(&root_lower) {
                return Some("OneDrive");
            }
        }
    }
    const MARKERS: &[(&str, &str)] = &[
        ("onedrive", "OneDrive"),
        ("dropbox", "Dropbox"),
        ("google drive", "Google Drive"),
        ("googledrive", "Google Drive"),
        ("icloud drive", "iCloud Drive"),
        ("icloud", "iCloud Drive"),
        ("nextcloud", "Nextcloud"),
    ];
    MARKERS.iter().find(|(marker, _)| lower.contains(marker)).map(|(_, name)| *name)
}

// Used both when picking a new data folder (before set_data_dir commits to it)
// and at startup against the folder already in use, so the same warning applies
// whichever way a sync folder ends up configured.
#[tauri::command]
fn path_sync_folder_hint(path: String) -> Option<String> {
    sync_folder_hint(Path::new(&path)).map(str::to_string)
}

#[tauri::command]
fn current_data_dir_sync_hint(app: AppHandle) -> Option<String> {
    sync_folder_hint(&data_dir(&app)).map(str::to_string)
}

// Decrypt a stored value. Legacy (un-prefixed) values pass through for migration; an
// undecryptable prefixed value (e.g. another Windows account) yields "" so the row is
// skipped rather than fed garbage to the JSON parser.
pub(crate) fn dec_str(stored: &str) -> String {
    if let Some(rest) = stored.strip_prefix(PLAINTEXT_ESCAPE) {
        return rest.to_string();
    }
    #[cfg(windows)]
    if let Some(b64) = stored.strip_prefix(ENC_PREFIX) {
        let ct = base64_decode(b64);
        if !ct.is_empty() {
            if let Some(pt) = store_crypt::unprotect(&ct) {
                if let Ok(s) = String::from_utf8(pt) {
                    return s;
                }
            }
        }
        return String::new();
    }
    stored.to_string()
}

// Schema + migration probe for a data.db connection, run once per data folder and
// process (see db_conn_at below). Returns false on any failure; db_conn_at then
// fails closed for that call and tries again on the next one.
pub(crate) fn db_conn_init_schema(conn: &Connection) -> bool {
    if conn
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS idx_copy_log_ts ON copy_log(ts);
             CREATE INDEX IF NOT EXISTS idx_copy_log_id ON copy_log(id);
             CREATE TABLE IF NOT EXISTS prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
             CREATE INDEX IF NOT EXISTS idx_pv_prompt ON prompt_versions(prompt_id);
             CREATE TABLE IF NOT EXISTS clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS edit_state(id TEXT PRIMARY KEY, kind TEXT NOT NULL, target TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL, ops TEXT NOT NULL, updated_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS editor_drafts(id TEXT PRIMARY KEY, prompt_id TEXT NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL);",
        )
        .is_err()
    {
        return false;
    }
    // First-ever schema migration in this app: `prompt_versions` predates
    // `variants`. `CREATE TABLE IF NOT EXISTS` above already gives a brand-new
    // install the column; an EXISTING data.db still has the old 5-column table.
    // Checked via PRAGMA table_info rather than blindly firing ALTER TABLE and
    // swallowing whatever comes back: that would also swallow a REAL failure
    // (locked/full/corrupt disk), silently leaving the column missing with no way
    // to tell the difference from "already migrated".
    let has_variants_column = conn
        .prepare("SELECT 1 FROM pragma_table_info('prompt_versions') WHERE name = 'variants'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(true); // a PRAGMA query failing here means something is already
                           // badly wrong with the connection: do not also spam an
                           // ALTER TABLE into that same broken state.
    if !has_variants_column {
        // The whole point of checking has_variants_column first (rather than
        // firing the ALTER unconditionally and swallowing whatever comes back) was
        // to tell "already migrated" apart from a REAL failure -- discarding the
        // ALTER's own result here threw that away again. A real failure now fails
        // db_conn itself, which every caller already treats as "DB unavailable".
        if conn.execute("ALTER TABLE prompt_versions ADD COLUMN variants TEXT NOT NULL DEFAULT ''", []).is_err() {
            return false;
        }
    }
    true
}

pub(crate) fn db_conn(app: &AppHandle) -> Option<Connection> {
    // The single choke point EVERY writer goes through, not just the four that
    // also call data_dir_write_blocked() (save_prompts/save_trash/
    // save_prompts_and_trash/write_settings_now): db_conn(app) is also called
    // directly by save_editor_draft, save_edit_state, record_copy's copy_log
    // append, version snapshots and more, none of which check that flag
    // themselves. Checking it here instead of (or in addition to) each call
    // site means enable_demo_mode's/disable_demo_mode's DEMO_TRANSITION_LOCKED
    // window blocks every one of them, not only the ones already funneled through
    // data_dir_write_blocked. None here reads exactly like "DB unavailable" to
    // every existing caller, which already handles it (an error result, or a
    // JSON-fallback path that goes through data_dir_write_blocked itself).
    if DEMO_TRANSITION_LOCKED.load(Ordering::Relaxed) {
        return None;
    }
    db_conn_at(&data_dir(app))
}

// Split out of db_conn() so the command line's headless path (no AppHandle to
// resolve a folder from) can open the exact same connection, with the exact
// same pragma and one-time schema init, as every other caller.
pub(crate) fn db_conn_at(dir: &Path) -> Option<Connection> {
    let conn = Connection::open(dir.join("data.db")).ok()?;
    // busy_timeout is connection-scoped (NOT persisted in the DB file, unlike
    // journal_mode below) so it must be set on every connection, but the schema
    // batch + migration probe are idempotent and only ever need to run once per
    // process: every call used to pay for a 10-statement execute_batch plus a
    // pragma_table_info probe just to open a connection for one write.
    conn.execute_batch("PRAGMA busy_timeout=3000;").ok()?;
    // Keyed by folder, and only a SUCCESS is remembered: a store adopted from another
    // folder mid-session still gets its schema run, and one transient failure at the
    // first open (WAL switch, busy) does not disable the database for the process.
    static SCHEMA_READY: Mutex<Option<std::collections::HashSet<PathBuf>>> = Mutex::new(None);
    let already = SCHEMA_READY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|set| set.contains(dir));
    if !already {
        if !db_conn_init_schema(&conn) {
            return None;
        }
        SCHEMA_READY
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_with(std::collections::HashSet::new)
            .insert(dir.to_path_buf());
    }
    Some(conn)
}

// What the prompts/trash tables are known to hold: "<table>\u{1}<id>" → a hash of
// that row's PLAINTEXT json. Every row is sealed before it is stored and sealing
// the same text twice gives different bytes, so a stored blob cannot be compared
// against a new one directly: and decrypting the whole table to compare would
// cost exactly what this map exists to save. An entry means "the row with this id
// currently in that table decodes to content hashing to this value", which is what
// lets a save skip re-sealing every untouched row.
//
// Anything that changes those tables from OUTSIDE write_rows: a restored or
// swapped data.db, a raw DELETE, a move to a different data folder: must clear
// the map (Store::forget_written_rows), or the next save trusts an entry that no
// longer describes the disk and skips a row that really did need writing.
pub(crate) type WrittenRows = HashMap<String, u64>;

pub(crate) fn row_key(table: &str, id: &str) -> String {
    // \u{1} cannot occur in a table name, so the two halves can never run together.
    format!("{table}\u{1}{id}")
}

pub(crate) fn row_hash(json: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    json.hash(&mut h);
    h.finish()
}

// Two prompts sharing one id can only ever be a bug in the caller's list: writing
// it would silently collapse them into a single row (and, with the old replace-all
// write, hit the PRIMARY KEY instead). Reject the whole write, exactly as before.
pub(crate) fn duplicate_row_id_error(table: &str) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY),
        Some(format!("duplicate id in {table}")),
    )
}

// Bring one table in line with `rows`, touching as little as possible: rows that
// disappeared are deleted, rows whose content changed are re-sealed and rewritten,
// and rows that merely moved get their `ord` corrected: reordering costs no
// encryption at all. Always inside the caller's own transaction. `table` is always
// one of our own two hardcoded table names below, never user input, so
// interpolating it into the SQL text is safe.
//
// Returns the map entries describing the rows as they now stand. The caller adopts
// them only after its COMMIT actually succeeded, so a rolled-back or failed write
// never leaves the process believing a row reached the disk.
pub(crate) fn write_rows(
    tx: &Transaction,
    table: &str,
    rows: &[Prompt],
    written: &WrittenRows,
) -> rusqlite::Result<WrittenRows> {
    let mut keep: std::collections::HashSet<&str> = std::collections::HashSet::with_capacity(rows.len());
    for p in rows {
        if !keep.insert(p.id.as_str()) {
            return Err(duplicate_row_id_error(table));
        }
    }
    // What the table holds right now: ids and positions only, nothing decrypted.
    // Row PRESENCE is taken from here rather than from the map, so a row that is
    // missing from the table is re-inserted even if the map still lists it.
    let mut present: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = tx.prepare(&format!("SELECT id, ord FROM {table}"))?;
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            present.insert(r.get(0)?, r.get(1)?);
        }
    }
    {
        // A row missing from `keep` is only deleted if THIS process already knew
        // about it (present in `written`, its own last-synced view of the table).
        // With `allowMultipleInstances`, another process can commit a brand-new
        // row between this process's own load and this save; that row is present
        // in the DB but never made it into this process's in-memory list or its
        // `written` map. Deleting on absence-from-`keep` alone would treat that
        // foreign row as "removed by the user" and destroy it, even though this
        // process never showed it to anyone. Requiring prior knowledge keeps a
        // real, user-initiated deletion working (the row was loaded, so it is in
        // `written`) while no longer erasing another instance's unseen work.
        let mut del = tx.prepare(&format!("DELETE FROM {table} WHERE id = ?1"))?;
        for id in present.keys() {
            if !keep.contains(id.as_str()) && written.contains_key(&row_key(table, id)) {
                del.execute(params![id])?;
            }
        }
    }
    let mut fresh = WrittenRows::with_capacity(rows.len());
    let mut ins = tx.prepare(&format!("INSERT INTO {table}(id, ord, data) VALUES(?1, ?2, ?3)"))?;
    let mut upd = tx.prepare(&format!("UPDATE {table} SET ord = ?2, data = ?3 WHERE id = ?1"))?;
    let mut moved = tx.prepare(&format!("UPDATE {table} SET ord = ?2 WHERE id = ?1"))?;
    for (i, p) in rows.iter().enumerate() {
        let json = serde_json::to_string(p).unwrap_or_default();
        let hash = row_hash(&json);
        let key = row_key(table, &p.id);
        let ord = i as i64;
        match present.get(&p.id) {
            Some(&cur_ord) if written.get(&key) == Some(&hash) => {
                if cur_ord != ord {
                    moved.execute(params![p.id, ord])?;
                }
            }
            Some(_) => {
                upd.execute(params![p.id, ord, enc_str(&json)])?;
            }
            None => {
                ins.execute(params![p.id, ord, enc_str(&json)])?;
            }
        }
        fresh.insert(key, hash);
    }
    Ok(fresh)
}

// Swap in one table's half of the map, after that table's write committed.
pub(crate) fn adopt_written_rows(written: &mut WrittenRows, table: &str, fresh: WrittenRows) {
    let prefix = row_key(table, "");
    written.retain(|k, _| !k.starts_with(&prefix));
    written.extend(fresh);
}

pub(crate) fn db_write_prompts(conn: &mut Connection, prompts: &[Prompt], written: &mut WrittenRows) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    let fresh = write_rows(&tx, "prompts", prompts, written)?;
    tx.commit()?;
    adopt_written_rows(written, "prompts", fresh);
    Ok(())
}

// Same schema and shape as db_write_prompts/db_load's prompt half, but its own
// table. A prompt "in the trash" simply isn't in `prompts` any more, so every
// existing read of store.prompts (grid, library, tray, hotkeys, export, stats,
// duplicate finder...) stays correct with zero changes: the alternative, a
// deleted_at flag on Prompt filtered everywhere, would have needed every one of
// those call sites found and fixed, with real risk of missing one.
pub(crate) fn db_write_trash(conn: &mut Connection, trash: &[Prompt], written: &mut WrittenRows) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    let fresh = write_rows(&tx, "trash", trash, written)?;
    tx.commit()?;
    adopt_written_rows(written, "trash", fresh);
    Ok(())
}

// delete_prompt/restore_prompt move one row between these two tables. Writing
// them as two independent transactions (the old db_write_prompts + db_write_trash
// pair) left a window where the first commit could durably land while the second
// failed, stranding the prompt in neither table. One transaction for both DELETE+
// INSERT pairs means either the whole move lands or neither half does.
pub(crate) fn db_write_prompts_and_trash(
    conn: &mut Connection,
    prompts: &[Prompt],
    trash: &[Prompt],
    written: &mut WrittenRows,
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    let fresh_prompts = write_rows(&tx, "prompts", prompts, written)?;
    let fresh_trash = write_rows(&tx, "trash", trash, written)?;
    tx.commit()?;
    adopt_written_rows(written, "prompts", fresh_prompts);
    adopt_written_rows(written, "trash", fresh_trash);
    Ok(())
}

// An import that replaces both halves: the prompt rows and the settings row
// land in one transaction, so either both are on disk or neither is. Two
// separate commits could leave one half imported while the command reported a
// failure.
pub(crate) fn db_write_prompts_and_settings(
    conn: &mut Connection,
    prompts: &[Prompt],
    settings: &Settings,
    written: &mut WrittenRows,
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    let fresh = write_rows(&tx, "prompts", prompts, written)?;
    db_write_settings(&tx, settings)?;
    tx.commit()?;
    adopt_written_rows(written, "prompts", fresh);
    Ok(())
}

// `written` collects what the table was found to hold (see WrittenRows) so the
// first save of the session already only writes what actually changed. Recorded
// straight from the rows as they were read: never from a later, migrated or
// replaced list, which would claim content for the disk that never reached it.
//
// Second return value is a failure count, same convention as db_load: a row
// that could not be read back, or an encrypted row that would not decrypt or
// parse, is present-but-unaccounted-for, not simply absent: the caller must
// fold it into the same write-lock decision as db_load's own failures, or a
// trash mutation could permanently erase whatever this omitted.
pub(crate) fn db_load_trash(conn: &Connection, written: &mut WrittenRows) -> (Vec<Prompt>, usize) {
    let mut trash = Vec::new();
    let mut failures = 0usize;
    match conn.prepare("SELECT data FROM trash ORDER BY ord") {
        Ok(mut stmt) => match stmt.query_map([], |r| r.get::<_, String>(0)) {
            Ok(rows) => {
                for row in rows {
                    match row {
                        Ok(data) => {
                            let encrypted = data.starts_with(ENC_PREFIX);
                            match serde_json::from_str::<Prompt>(&dec_str(&data)) {
                                Ok(p) => {
                                    // Same rule as db_load: an unsealed row stays out of
                                    // the map so the next write seals it instead of
                                    // skipping it as unchanged.
                                    if encrypted {
                                        written.insert(
                                            row_key("trash", &p.id),
                                            row_hash(&serde_json::to_string(&p).unwrap_or_default()),
                                        );
                                    }
                                    trash.push(p);
                                }
                                Err(_) if encrypted => failures += 1,
                                Err(_) => {}
                            }
                        }
                        Err(_) => failures += 1,
                    }
                }
            }
            Err(_) => failures += 1,
        },
        Err(_) => failures += 1,
    }
    (trash, failures)
}

pub(crate) fn db_write_settings(conn: &Connection, settings: &Settings) -> rusqlite::Result<()> {
    let json = enc_str(&serde_json::to_string(settings).unwrap_or_default());
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('settings', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        params![json],
    )?;
    Ok(())
}

// Returns (prompts, settings, any_plaintext, load_failures). `load_failures`
// counts rows that are present-but-unaccounted-for: an encrypted row that
// would not decrypt/parse, a row the SQLite driver could not read back at all,
// or the SELECT itself failing to prepare/run (corrupt table, locked file):
// every one of them means the true prompt count is NOT known to be what this
// function returned, so the caller must avoid any destructive rewrite of the
// prompts table rather than treat a partial result as the whole truth.
pub(crate) fn db_load(conn: &Connection, written: &mut WrittenRows) -> (Vec<Prompt>, Option<Settings>, bool, usize, bool) {
    let mut prompts = Vec::new();
    let mut plaintext = false; // any legacy unencrypted row → re-save once to encrypt
    let mut failures = 0usize;
    match conn.prepare("SELECT data FROM prompts ORDER BY ord") {
        Ok(mut stmt) => match stmt.query_map([], |r| r.get::<_, String>(0)) {
            Ok(rows) => {
                for row in rows {
                    match row {
                        Ok(data) => {
                            let encrypted = data.starts_with(ENC_PREFIX);
                            if !encrypted {
                                plaintext = true;
                            }
                            match serde_json::from_str::<Prompt>(&dec_str(&data)) {
                                Ok(p) => {
                                    // A legacy plaintext row must be re-saved (sealed) even
                                    // though its content did not change, so it is deliberately
                                    // left out of the map and thus counts as changed.
                                    if encrypted {
                                        written.insert(
                                            row_key("prompts", &p.id),
                                            row_hash(&serde_json::to_string(&p).unwrap_or_default()),
                                        );
                                    }
                                    prompts.push(p);
                                }
                                // An encrypted row that won't decrypt/parse is present-but-unreadable;
                                // a plaintext row that won't parse is genuinely corrupt (not a key issue).
                                Err(_) if encrypted => failures += 1,
                                Err(_) => {}
                            }
                        }
                        Err(_) => failures += 1,
                    }
                }
            }
            Err(_) => failures += 1,
        },
        Err(_) => failures += 1,
    }
    // A genuinely absent row (fresh install, nothing saved yet) must never count as
    // a failure -- only a row that IS there but could not be read back or parsed is
    // "unreadable" in the sense that locks writes (see settings_locked below).
    let mut settings_unreadable = false;
    let raw = match conn.query_row("SELECT value FROM meta WHERE key='settings'", [], |r| {
        r.get::<_, String>(0)
    }) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(_) => {
            settings_unreadable = true;
            None
        }
    };
    if raw.as_deref().is_some_and(|v| !v.starts_with(ENC_PREFIX)) {
        plaintext = true;
    }
    let settings = raw.and_then(|s| match serde_json::from_str::<Settings>(&dec_str(&s)) {
        Ok(s) => Some(s),
        Err(_) => {
            settings_unreadable = true;
            None
        }
    });
    (prompts, settings, plaintext, failures, settings_unreadable)
}

// Peeks the "allowMultipleInstances" flag before the Tauri builder exists. The
// single-instance plugin has to be attached (or not) at builder time, before any
// AppHandle exists to resolve settings the normal way: so this mirrors
// default_data_dir()'s portable/APPDATA resolution and opens the DB directly. Any
// failure (first run, missing DB, undecryptable row) yields false: single instance,
// the safe default that matches every install before this option existed.
pub(crate) fn early_allow_multiple_instances() -> bool {
    let dir = if is_portable() {
        portable_data_dir()
    } else {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join(DATA_FOLDER))
    };
    let Some(base) = dir else { return false };
    let dir = fs::read_to_string(base.join("datapath"))
        .ok()
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| !p.as_os_str().is_empty() && p.is_dir())
        .unwrap_or(base);
    let Ok(conn) = Connection::open(dir.join("data.db")) else { return false };
    let raw: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key='settings'", [], |r| r.get(0))
        .ok();
    let Some(raw) = raw else { return false };
    serde_json::from_str::<Settings>(&dec_str(&raw))
        .map(|s| s.ui_flags.get("allowMultipleInstances").copied().unwrap_or(false))
        .unwrap_or(false)
}

// Whether any copy-history row exists; an unreadable table counts as yes, so an
// unsure case is treated as an existing install.
pub(crate) fn copy_log_has_rows(conn: &Connection) -> bool {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM copy_log)", [], |r| r.get::<_, i64>(0))
        .map(|v| v != 0)
        .unwrap_or(true)
}

// Append one copy event, then trim to the cap and the retention window. The cap
// trim uses the rowid (monotonic insertion order): "keep rows whose rowid is
// within `cap` of the newest" deletes at most one row per call in steady state
// and touches nothing during growth: O(log n), so a million-entry history is
// still cheap on every copy.
// Trim the history to the cap (newest `cap` rows) and the retention window. Rows
// kept are exactly those with rowid in (MAX-cap, MAX], i.e. at most `cap`, and
// age-prune only ever deletes from the old (low-rowid) end, so the kept block
// stays a contiguous newest-N: the cap can never be exceeded.
pub(crate) fn db_trim_copy(conn: &Connection, cap: usize, cutoff: Option<u64>) -> rusqlite::Result<()> {
    if cap == 0 {
        conn.execute("DELETE FROM copy_log", [])?;
        return Ok(());
    }
    conn.execute(
        "DELETE FROM copy_log WHERE rowid <= (SELECT MAX(rowid) FROM copy_log) - ?1",
        params![cap as i64],
    )?;
    if let Some(c) = cutoff {
        conn.execute("DELETE FROM copy_log WHERE ts > 0 AND ts < ?1", params![c as i64])?;
    }
    Ok(())
}

pub(crate) fn db_append_copy(conn: &Connection, entry: &CopyEntry, cap: usize, cutoff: Option<u64>) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO copy_log(ts, id) VALUES(?1, ?2)", params![entry.ts as i64, entry.id])?;
    db_trim_copy(conn, cap, cutoff)
}

// One journal row as shown/deletable in the UI: unlike CopyEntry (insert/trim/
// legacy-migration only), this carries the rowid a delete needs to target the exact
// row, even in grouped mode where several rows share the same prompt id.
#[derive(Serialize)]
pub(crate) struct JournalEntry {
    pub(crate) rowid: i64,
    pub(crate) id: String,
    pub(crate) ts: u64,
}

// Copy counts for the last `days` UTC days, oldest first, zero-filled: a
// single GROUP BY query bucketed by day-since-epoch, merged against a dense day
// range in Rust (cheaper than one query per day, and never skips an empty day).
pub(crate) fn db_daily_copies(conn: &Connection, days: u32) -> Vec<DayCount> {
    const DAY: u64 = 86400;
    let today_bucket = now_secs() / DAY;
    let start_bucket = today_bucket.saturating_sub(days as u64 - 1);
    let mut counts: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT ts / 86400, COUNT(*) FROM copy_log WHERE ts >= ?1 GROUP BY ts / 86400") {
        if let Ok(rows) = stmt.query_map(params![(start_bucket * DAY) as i64], |r| {
            Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as usize))
        }) {
            counts.extend(rows.flatten());
        }
    }
    (0..days as u64)
        .map(|i| {
            let bucket = start_bucket + i;
            DayCount { ts: bucket * DAY, count: counts.get(&bucket).copied().unwrap_or(0) }
        })
        .collect()
}

// Newest copy events for the journal. grouped = one row per prompt (its most
// recent copy); otherwise every event. Capped so the UI never renders millions.
pub(crate) fn db_recent_copies(conn: &Connection, limit: usize, grouped: bool) -> Vec<JournalEntry> {
    let sql = if grouped {
        // The whole newest row per prompt, not MAX(rowid) beside an independently
        // computed MAX(ts): two aggregates are evaluated separately, so the row id
        // the delete button targets could belong to a different copy than the
        // timestamp shown next to it.
        "SELECT rowid, id, ts FROM copy_log
         WHERE rowid IN (SELECT MAX(rowid) FROM copy_log GROUP BY id)
         ORDER BY rowid DESC LIMIT ?1"
    } else {
        "SELECT rowid, id, ts FROM copy_log ORDER BY rowid DESC LIMIT ?1"
    };
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(sql) {
        if let Ok(rows) = stmt.query_map(params![limit as i64], |r| {
            Ok(JournalEntry { rowid: r.get(0)?, id: r.get(1)?, ts: r.get::<_, i64>(2)? as u64 })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

// ---- Prompt version history ----
// Snapshots of a prompt's previous name/text, encrypted like the prompt rows. The
// table lives in data.db (not the settings blob) so history never bloats settings.
#[derive(Serialize)]
pub(crate) struct PromptVersion {
    pub(crate) id: i64,
    pub(crate) ts: u64,
    pub(crate) name: String,
    pub(crate) text: String,
    // Entries ARE content: a version snapshot without them would let a
    // restore silently drop entries 2..n. JSON-encoded in its own DB column
    // (unlike the export formats, nothing here needs to stay human-editable).
    pub(crate) variants: Vec<String>,
}

pub(crate) fn encode_variants(variants: &[String]) -> String {
    serde_json::to_string(variants).unwrap_or_default()
}
pub(crate) fn decode_variants(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

pub(crate) fn history_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("promptHistory") != Some(&false) // on by default
}
pub(crate) fn history_cap(settings: &Settings) -> usize {
    settings.ui_values.get("historyPerPrompt").copied().unwrap_or(20.0).clamp(5.0, 100.0) as usize
}

pub(crate) fn db_add_version(conn: &Connection, prompt_id: &str, ts: u64, name: &str, text: &str, variants: &[String], cap: usize) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO prompt_versions(prompt_id, ts, name, text, variants) VALUES(?1, ?2, ?3, ?4, ?5)",
        params![prompt_id, ts as i64, enc_str(name), enc_str(text), enc_str(&encode_variants(variants))],
    )?;
    // Keep only the newest `cap` snapshots for this prompt.
    conn.execute(
        "DELETE FROM prompt_versions WHERE prompt_id = ?1 AND id NOT IN
            (SELECT id FROM prompt_versions WHERE prompt_id = ?1 ORDER BY id DESC LIMIT ?2)",
        params![prompt_id, cap as i64],
    )?;
    Ok(())
}
pub(crate) fn db_list_versions(conn: &Connection, prompt_id: &str) -> Vec<PromptVersion> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, ts, name, text, variants FROM prompt_versions WHERE prompt_id = ?1 ORDER BY id DESC") {
        if let Ok(rows) = stmt.query_map(params![prompt_id], |r| {
            Ok(PromptVersion {
                id: r.get::<_, i64>(0)?,
                ts: r.get::<_, i64>(1)? as u64,
                name: dec_str(&r.get::<_, String>(2)?),
                text: dec_str(&r.get::<_, String>(3)?),
                variants: decode_variants(&dec_str(&r.get::<_, String>(4)?)),
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}
// One prompt's last real content edit (name or text change: the only kind
// that gets a version row, see update_prompt) is the newest `ts` it has in
// prompt_versions. Bundled for every prompt at once (one GROUP BY, not an N+1
// query per prompt): a prompt absent from the result has never had a version
// row: history was off for all its edits, or it was never edited since creation.
pub(crate) fn db_last_edited(conn: &Connection) -> std::collections::HashMap<String, u64> {
    let mut out = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT prompt_id, MAX(ts) FROM prompt_versions GROUP BY prompt_id") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))) {
            out.extend(rows.flatten());
        }
    }
    out
}
pub(crate) fn db_get_version(conn: &Connection, id: i64) -> Option<(String, String, String, Vec<String>)> {
    conn.query_row(
        "SELECT prompt_id, name, text, variants FROM prompt_versions WHERE id = ?1",
        params![id],
        |r| Ok((
            r.get::<_, String>(0)?,
            dec_str(&r.get::<_, String>(1)?),
            dec_str(&r.get::<_, String>(2)?),
            decode_variants(&dec_str(&r.get::<_, String>(3)?)),
        )),
    )
    .ok()
}
// Removes version history whose prompt is in neither the library nor the trash.
// Works on the plaintext id column alone, so it is safe even when the rows
// themselves could not be decrypted. Returns the number of rows removed.
// The orphan sweep deletes rows: never in a read-only instance (the store belongs
// to another machine) and never while a JSON fallback still holds prompts.
pub(crate) fn version_sweep_allowed(dir: &Path, read_only: bool) -> bool {
    !read_only && !dir.join("prompts.json").exists()
}

pub(crate) fn db_sweep_orphan_versions(conn: &Connection) -> usize {
    match conn.execute(
        "DELETE FROM prompt_versions WHERE prompt_id NOT IN (SELECT id FROM prompts UNION SELECT id FROM trash)",
        [],
    ) {
        Ok(n) => n,
        Err(e) => {
            append_crash_log(&format!("could not sweep orphaned version history: {e}"));
            0
        }
    }
}
pub(crate) fn db_delete_versions(conn: &Connection, prompt_id: &str) {
    // Called from a PERMANENT purge -- the prompt is meant to be entirely gone,
    // so a failure here silently leaving its text behind in prompt_versions is
    // a privacy gap, not just clutter.
    if let Err(e) = conn.execute("DELETE FROM prompt_versions WHERE prompt_id = ?1", params![prompt_id]) {
        append_crash_log(&format!("could not delete version history for purged prompt {prompt_id}: {e}"));
    }
}

// ---- Clipboard watcher ----
#[derive(Serialize)]
pub(crate) struct ClipItem {
    pub(crate) id: i64,
    pub(crate) ts: u64,
    pub(crate) text: String,
}
pub(crate) fn text_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

// Fingerprint of a seeded example prompt's editable content, so "remove examples"
// can tell whether the user has since made one their own. Mirrors SlimPrompt's own
// image_rev (0 for no image, otherwise a hash of the image bytes) rather than a
// separate stored field.
pub(crate) fn example_seed_fingerprint(name: &str, text: &str, image: &str) -> u64 {
    let image_rev = if image.is_empty() { 0 } else { text_hash(image) };
    text_hash(&format!("{name}\u{0}{text}\u{0}{image_rev}"))
}
pub(crate) fn db_add_clip(conn: &Connection, ts: u64, text: &str, cap: usize) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO clip_inbox(ts, text) VALUES(?1, ?2)", params![ts as i64, enc_str(text)])?;
    conn.execute(
        "DELETE FROM clip_inbox WHERE id <= (SELECT MAX(id) FROM clip_inbox) - ?1",
        params![cap as i64],
    )?;
    Ok(())
}
pub(crate) fn db_list_clips(conn: &Connection) -> Vec<ClipItem> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, ts, text FROM clip_inbox ORDER BY id DESC") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok(ClipItem { id: r.get::<_, i64>(0)?, ts: r.get::<_, i64>(1)? as u64, text: dec_str(&r.get::<_, String>(2)?) })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

// load_store()'s half of the relative <-> absolute round trip (the other
// half is strip_data_dir_prefix right below, its own comment has the full
// story). Pulled out as its own function, run over both `prompts` and
// `trash` (a third review finding: only `prompts` was resolved, so a demo
// attachment that got deleted during the session restored from trash with a
// bare, unresolvable filename), so it can be unit tested directly without an
// AppHandle, the same way strip_data_dir_prefix already is.
pub(crate) fn resolve_relative_file_paths(dir: &Path, rows: &mut [Prompt]) {
    for p in rows.iter_mut() {
        if !p.file_path.is_empty() && Path::new(&p.file_path).is_relative() {
            p.file_path = dir.join(&p.file_path).to_string_lossy().to_string();
        }
    }
}

// The other half of load_store()'s relative -> absolute resolution for
// `file_path` (see its own comment): reversed right before anything is
// persisted or exported, so a demo prompt's attachment is never baked into a
// saved row, a promptSync .md file or an export as an absolute path
// containing the Windows account name. Only ever touches a path whose PARENT
// is exactly the live data dir -- the one load_store() itself would have
// joined on -- so a real attachment (never placed directly inside the data
// dir by the file picker) is never touched. Every caller of db_write_prompts/
// db_write_prompts_and_trash, sync_write_all_impl and export_prompts must run
// its prompts through this first.
//
// Gated to actually being in demo mode right now (a third, then a fourth
// review finding): in portable mode the data dir IS the exe folder, and a
// custom data dir is common too, so a real user's file picked from directly
// inside that folder has an absolute path whose parent legitimately equals
// `dir`: stripping it unconditionally turned a real attachment into a bare
// filename that broke after any later data-dir move or portable/installed
// takeover. The first attempt at this gate compared `dir`'s folder NAME
// against `DEMO_SUBFOLDER`, which still fires for a real (if unlikely) data
// dir a user happens to have named `prompt-saver-demo` -- `is_demo` is an
// exact-path comparison instead (`data_dir(app) == default_data_dir(app).
// join(DEMO_SUBFOLDER)`), computed once by the caller, which already has
// `app` in scope. Demo mode is the only caller this needs to touch, so it
// only ever runs there.
// The exact-path check strip_data_dir_prefix's own comment explains: never a
// folder-name guess, always the live data dir compared against exactly where
// demo mode's own redirect would have pointed it.
pub(crate) fn data_dir_is_demo(app: &AppHandle, dir: &Path) -> bool {
    dir == default_data_dir(app).join(DEMO_SUBFOLDER)
}

pub(crate) fn strip_data_dir_prefix<'a>(dir: &Path, is_demo: bool, prompts: &'a [Prompt]) -> std::borrow::Cow<'a, [Prompt]> {
    if !is_demo {
        return std::borrow::Cow::Borrowed(prompts);
    }
    if !prompts.iter().any(|p| !p.file_path.is_empty() && Path::new(&p.file_path).parent() == Some(dir)) {
        return std::borrow::Cow::Borrowed(prompts);
    }
    let mut out = prompts.to_vec();
    for p in out.iter_mut() {
        if Path::new(&p.file_path).parent() == Some(dir) {
            if let Some(name) = Path::new(&p.file_path).file_name() {
                p.file_path = name.to_string_lossy().to_string();
            }
        }
    }
    std::borrow::Cow::Owned(out)
}

// Returns whether the prompts made it to durable storage (DB or JSON fallback):
// callers that follow a save with a destructive step (deleting an orphaned
// attachment file) must skip that step when this is false, or the deletion can
// outlive the very change that made the file orphaned.
pub(crate) fn save_prompts(app: &AppHandle, store: &Store) -> bool {
    if data_dir_write_blocked(app) {
        return false;
    }
    // Safety valve: if some rows failed to decrypt at load, the in-memory list is a
    // SUBSET of what's on disk. Rewriting the table (DELETE + re-INSERT) would delete
    // the unreadable rows permanently: so we refuse to persist prompt changes at all
    // until the situation is resolved. Prompt edits this session simply won't stick.
    if store.prompts_locked {
        return false;
    }
    let dir = data_dir(app);
    let prompts = strip_data_dir_prefix(&dir, data_dir_is_demo(app, &dir), &store.prompts);
    if let Some(mut conn) = db_conn(app) {
        // The map is only advanced by a committed write, so a failure here leaves it
        // describing exactly what the (rolled-back) table still holds: the JSON
        // fallback below must not touch it either, or the row would count as saved.
        let ok = {
            let mut written = store.written.borrow_mut();
            db_write_prompts(&mut conn, &prompts, &mut written).is_ok()
        };
        if ok {
            drop_stale_fallback(&dir, "prompts.json");
            spawn_sync_write_all(app, store); // Prompt-folders-as-Markdown: no-op unless the feature is on
            return true;
        }
    }
    // Last resort: JSON fallback. load_store() reads this back and retries the
    // DB write on next launch, so a fallback here is not itself data loss. If even
    // this fails (disk full / permission), the change really is lost: leave a
    // diagnosable trace instead of failing silently.
    if !write_sealed_json(&dir.join("prompts.json"), &prompts) {
        append_crash_log("persist failed: prompts could not be written to the database or the JSON fallback");
        let _ = app.emit("persist-failed", "prompts");
        return false;
    }
    spawn_sync_write_all(app, store);
    true
}

pub(crate) fn save_trash(app: &AppHandle, store: &Store) -> bool {
    if data_dir_write_blocked(app) {
        return false;
    }
    // Same lock as save_prompts: an unreadable-row situation must never trigger a
    // destructive rewrite of a second table either.
    if store.prompts_locked {
        return false;
    }
    let dir = data_dir(app);
    let trash = strip_data_dir_prefix(&dir, data_dir_is_demo(app, &dir), &store.trash);
    if let Some(mut conn) = db_conn(app) {
        let mut written = store.written.borrow_mut();
        if db_write_trash(&mut conn, &trash, &mut written).is_ok() {
            return true;
        }
    }
    append_crash_log("persist failed: trash could not be written to the database");
    let _ = app.emit("persist-failed", "trash");
    false
}

// The atomic counterpart to save_prompts+save_trash for delete_prompt/
// restore_prompt - see db_write_prompts_and_trash for why the two tables can no
// longer be saved as two separate commits for a single move.
pub(crate) fn save_prompts_and_trash(app: &AppHandle, store: &Store) -> bool {
    if data_dir_write_blocked(app) {
        return false;
    }
    if store.prompts_locked {
        return false;
    }
    let dir = data_dir(app);
    let demo = data_dir_is_demo(app, &dir);
    let prompts = strip_data_dir_prefix(&dir, demo, &store.prompts);
    let trash = strip_data_dir_prefix(&dir, demo, &store.trash);
    if let Some(mut conn) = db_conn(app) {
        let ok = {
            let mut written = store.written.borrow_mut();
            db_write_prompts_and_trash(&mut conn, &prompts, &trash, &mut written).is_ok()
        };
        if ok {
            drop_stale_fallback(&dir, "prompts.json");
            spawn_sync_write_all(app, store); // Prompt-folders-as-Markdown mirrors prompts, same as save_prompts
            return true;
        }
    }
    append_crash_log("persist failed: trash move could not be written to the database");
    let _ = app.emit("persist-failed", "trash");
    false
}

// Mirrors save_prompts' shape below - same DB-then-JSON-fallback order, same
// choke point every settings mutation already funnels through. Returning bool
// (instead of nothing) plus the same persist-failed event lets the existing
// frontend listener cover all of this function's callers for free instead of
// the setting silently reverting, unexplained, on the next restart.
pub(crate) fn save_settings(app: &AppHandle, settings: &Settings) -> bool {
    let _writing = settings_write_lock();
    // A full write covers whatever was collected: the counters live in this same
    // block, so once it is on disk nothing is owed any more.
    SETTINGS_DELAY.discard();
    write_settings_now(app, settings)
}

// The write itself, without the "cancel a collected write" step: the collecting
// path below calls this one so it does not clear its own mark twice.
pub(crate) fn write_settings_now(app: &AppHandle, settings: &Settings) -> bool {
    if data_dir_write_blocked(app) {
        return false;
    }
    // Safety valve: the row on disk is present but unreadable, so the in-memory
    // Settings is a default, not what is actually stored. Rewriting now would
    // permanently destroy the real (corrupt but recoverable-by-hand) row.
    if SETTINGS_LOCKED.load(Ordering::Relaxed) {
        return false;
    }
    if let Some(conn) = db_conn(app) {
        if db_write_settings(&conn, settings).is_ok() {
            drop_stale_fallback(&data_dir(app), "settings.json");
            // Only announced on the actual true -> false transition (swap, not a
            // plain store) -- every ordinary successful write also passes through
            // here and must not spam the window with a recovery event it never
            // regressed from.
            if SETTINGS_WRITE_FAILED.swap(false, Ordering::Relaxed) {
                let _ = app.emit("settings-write-recovered", ());
            }
            return true;
        }
    }
    if !write_sealed_json(&data_dir(app).join("settings.json"), settings) {
        append_crash_log("persist failed: settings could not be written to the database or the JSON fallback");
        SETTINGS_WRITE_FAILED.store(true, Ordering::Relaxed);
        let _ = app.emit("persist-failed", "settings");
        return false;
    }
    if SETTINGS_WRITE_FAILED.swap(false, Ordering::Relaxed) {
        let _ = app.emit("settings-write-recovered", ());
    }
    true
}

// ---- Counter writes are collected instead of going to disk one at a time ----
//
// save_settings re-encrypts and rewrites the WHOLE settings block (themes,
// layouts, window geometry, every toggle) each time it is called. Rotation
// pointers, usage counters and "last used" stamps move on every copy: and on a
// rotating prompt with auto-advance, on every single paste: so they used to buy
// one full settings write per Ctrl+V. They mark the block dirty instead, and the
// shared timer thread writes it once the marking has been quiet for a moment.
//
// Only the DISK write waits. Every marking call site has already updated
// store.settings under the store lock before it marks, so the next copy of that
// prompt still serves the right entry no matter when the write lands.
//
// What this costs in the worst case: if the process is killed outright; task
// manager, a power cut, a crash: up to one second of counter state is lost: a
// rotation pointer that stays where it was, a use count one short, a "last used"
// stamp from the previous copy. It can never cost a prompt, any text, or a
// setting the user just changed. Everything the user edits still writes through
// immediately, and every orderly exit, Windows session end, backup, export,
// import, reset and data-folder move flushes what is owed before it goes on.
pub(crate) const SETTINGS_QUIET_MS: u64 = 1000;
// How often the timer thread looks at the mark. Finer than the quiet window so
// the write lands just behind it rather than a whole window late; a round with
// nothing owed is one uncontended mutex and no disk access at all.
pub(crate) const SETTINGS_FLUSH_TICK_MS: u64 = 250;

// Marked = a settings write is owed, at the instant the mark was set. Every
// later mark within the quiet window pushes the write further out, so a burst of
// copies collapses into one write rather than one per copy.
pub(crate) struct SettingsDelay {
    pub(crate) marked: Mutex<Option<std::time::Instant>>,
    pub(crate) quiet: Mutex<Duration>,
}

impl SettingsDelay {
    pub(crate) const fn new(quiet_ms: u64) -> Self {
        SettingsDelay {
            marked: Mutex::new(None),
            quiet: Mutex::new(Duration::from_millis(quiet_ms)),
        }
    }

    pub(crate) fn mark(&self) {
        *self.marked.lock().unwrap_or_else(|e| e.into_inner()) = Some(std::time::Instant::now());
    }

    // Is anything owed at all? Cheap enough for the idle tick to ask every round.
    pub(crate) fn owed(&self) -> bool {
        self.marked.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    // The ONE place that decides whether a collected write happens, so the number
    // of `true` answers is exactly the number of disk writes on this path.
    // `force` skips the quiet window: an exit or a backup cannot wait it out.
    pub(crate) fn take(&self, force: bool) -> bool {
        let quiet = *self.quiet.lock().unwrap_or_else(|e| e.into_inner());
        let mut marked = self.marked.lock().unwrap_or_else(|e| e.into_inner());
        match *marked {
            Some(at) if force || at.elapsed() >= quiet => {
                *marked = None;
                true
            }
            _ => false,
        }
    }

    // An immediate write of the same block has just happened: nothing left owed.
    pub(crate) fn discard(&self) {
        *self.marked.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

pub(crate) static SETTINGS_DELAY: SettingsDelay = SettingsDelay::new(SETTINGS_QUIET_MS);

// Serializes the writes themselves, so a collected write started on the timer
// thread can never land on top of a newer immediate one. Lock order is always
// the store lock first, this one second: nothing under this lock ever takes it.
pub(crate) static SETTINGS_WRITE: Mutex<()> = Mutex::new(());

pub(crate) fn settings_write_lock() -> std::sync::MutexGuard<'static, ()> {
    SETTINGS_WRITE.lock().unwrap_or_else(|e| e.into_inner())
}

// A counter/pointer change: already correct in memory, worth one disk write per
// quiet second instead of one per copy. The caller must have updated
// store.settings before calling this.
pub(crate) fn save_settings_soon() {
    SETTINGS_DELAY.mark();
}

// Write a collected change out now. `settings` is the live block, which the
// caller already holds under the store lock.
pub(crate) fn flush_settings(app: &AppHandle, settings: &Settings) {
    let _writing = settings_write_lock();
    if SETTINGS_DELAY.take(true) {
        write_settings_now(app, settings);
    }
}

// Same, for a caller that does not hold the store lock itself.
pub(crate) fn flush_settings_now(app: &AppHandle) {
    if !SETTINGS_DELAY.owed() {
        return;
    }
    if let Some(state) = app.try_state::<Db>() {
        let store = state.lock().unwrap_or_else(|e| e.into_inner());
        flush_settings(app, &store.settings);
    }
}

// One round of the timer thread's flush job.
pub(crate) fn settings_flush_tick(app: &AppHandle) {
    if !SETTINGS_DELAY.owed() {
        return;
    }
    let Some(state) = app.try_state::<Db>() else {
        return;
    };
    let store = state.lock().unwrap_or_else(|e| e.into_inner());
    let _writing = settings_write_lock();
    if SETTINGS_DELAY.take(false) {
        write_settings_now(app, &store.settings);
    }
}

// Pre-1.9 builds saved image prompts without copy_image but copied on click.
// The self-filling placeholder names, kept in step with DYN_VARS in ui/main.js.
pub(crate) const DYN_NAMES: [&str; 16] = [
    "date", "time", "datetime", "weekday", "yesterday", "tomorrow", "week", "month",
    "year", "timestamp", "epoch", "clipboard", "username", "uuid", "random", "counter",
];

// Reserved names are now written {#{/date\}#} instead of {#{date}#}, which frees the
// plain word for a variable of the user's own. Rewrite the old spelling so an existing
// prompt keeps filling itself in. Idempotent: the new form no longer matches.
pub(crate) fn fence_dyn_vars(text: &str) -> String {
    if !text.contains("{#{") {
        return text.to_string();
    }
    let mut out = text.to_string();
    for n in DYN_NAMES {
        let old = ["{#{", n, "}#}"].concat();
        if out.contains(&old) {
            out = out.replace(&old, &["{#{/", n, "\\}#}"].concat());
        }
    }
    out
}

pub(crate) fn migrate_prompts(prompts: &mut [Prompt]) {
    // `id` is the prompts table's PRIMARY KEY, so a duplicate makes the whole
    // DELETE+INSERT rewrite fail: and save_prompts then quietly writes the JSON
    // fallback that the database path ignores, so the import looks like it worked and
    // the old prompts are back after a restart. An import (or a hand-edited backup) is
    // the only way two prompts can share an id; the second one gets a fresh one.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in prompts.iter_mut() {
        if p.id.is_empty() || !seen.insert(p.id.clone()) {
            p.id = gen_id();
            seen.insert(p.id.clone());
        }
    }
    for p in prompts {
        if !p.image.is_empty() && !p.copy_image && (p.text.is_empty() || p.text == p.name) {
            p.copy_image = true;
        }
        let fenced = fence_dyn_vars(&p.text);
        if fenced != p.text {
            p.text = fenced;
        }
    }
}

pub(crate) fn load_store(app: &AppHandle) -> Store {
    let dir = data_dir(app);
    if let Some(mut conn) = db_conn(app) {
        // Filled from the rows as they are read, so the first save of the session
        // already only writes what changed (see WrittenRows).
        let mut written = WrittenRows::new();
        let (mut prompts, settings_opt, needs_mig, prompt_fails, settings_unreadable) =
            db_load(&conn, &mut written);
        let (mut trash, trash_fails) = db_load_trash(&conn, &mut written);
        let decrypt_fails = prompt_fails + trash_fails;
        // Some row (prompts, trash, or settings) could not be read back or decrypted →
        // preserve the raw DB and lock writes to whatever was affected (see
        // prompts_locked/SETTINGS_LOCKED below, which gate their respective writes
        // the same way).
        if decrypt_fails > 0 || settings_unreadable {
            let db_path = dir.join("data.db");
            let bak = dir.join("data.db.locked.bak");
            if !bak.exists() {
                let _ = fs::copy(&db_path, &bak);
            }
            append_crash_log(&format!(
                "{} row(s) failed to load reliably (query error, unreadable row, or decrypt failure){}; \
                 writes locked, raw DB copied to data.db.locked.bak",
                decrypt_fails,
                if settings_unreadable { " (settings row included)" } else { "" }
            ));
        }
        SETTINGS_LOCKED.store(settings_unreadable, Ordering::Relaxed);
        let mut settings = settings_opt.clone().unwrap_or_default();
        // True while `settings` is a blank default rather than something read from disk.
        let mut settings_defaulted = settings_opt.is_none();
        // Empty DB but legacy JSON present → import it once, then own the data.
        // A corrupt-but-present settings row must never be mistaken for "nothing
        // here yet" -- that would silently paper over the SETTINGS_LOCKED case set
        // above with whatever an unrelated legacy JSON file happens to contain.
        if prompts.is_empty() && settings_opt.is_none() && !settings_unreadable {
            let j_settings_r: SealedRead<Settings> = read_sealed_json(&dir.join("settings.json"));
            let j_prompts_r: SealedRead<Vec<Prompt>> = read_sealed_json(&dir.join("prompts.json"));
            // An unreadable legacy file (wrong Windows account, corruption) must never be
            // treated as "nothing to import": that would both skip real data AND, below,
            // still be eligible for the empty-DB path to run again forever, or worse, look
            // imported. Skip the whole import this launch and leave the file for inspection.
            if matches!(j_settings_r, SealedRead::Unreadable) || matches!(j_prompts_r, SealedRead::Unreadable) {
                append_crash_log(
                    "legacy prompts.json/settings.json present but could not be decrypted/parsed; \
                     import skipped this launch, files kept",
                );
            } else {
                let j_prompts = match j_prompts_r {
                    SealedRead::Loaded(v) => v,
                    _ => Vec::new(),
                };
                let j_settings = match j_settings_r {
                    SealedRead::Loaded(v) => {
                        settings_defaulted = false;
                        v
                    }
                    _ => Settings::default(),
                };
                if !j_prompts.is_empty() || dir.join("settings.json").exists() {
                    let prompts_ok = db_conn(app)
                        .map(|mut c2| db_write_prompts(&mut c2, &j_prompts, &mut written).is_ok())
                        .unwrap_or(false);
                    let settings_ok = db_write_settings(&conn, &j_settings).is_ok();
                    // Once the data is safely in the DB, drop the legacy JSON files so
                    // the import never runs again and stale copies cannot diverge.
                    if prompts_ok && settings_ok {
                        let _ = fs::remove_file(dir.join("prompts.json"));
                        let _ = fs::remove_file(dir.join("settings.json"));
                    }
                }
                prompts = j_prompts;
                settings = j_settings;
            }
        } else {
            // prompts.json/settings.json otherwise only exist as a save_prompts/
            // save_settings fallback from a failed DB write earlier this session (they
            // are deleted the moment the DB accepts the data again): their presence
            // here means the DB never received that change, so the file, not the DB,
            // is the truth. Retry the write; keep the file (and the loaded data) if the
            // DB still refuses, so this keeps retrying every launch instead of quietly
            // losing the edit.
            // Rows that failed to decrypt already lock the store below (prompts_locked);
            // do not also retry the fallback write here, or a partially-successful DB
            // write would fight with that lock over what the DB is supposed to contain.
            if decrypt_fails == 0 {
                let pj = dir.join("prompts.json");
                if pj.exists() {
                    match read_sealed_json::<Vec<Prompt>>(&pj) {
                        SealedRead::Loaded(fallback) => {
                            prompts = fallback;
                            if db_write_prompts(&mut conn, &prompts, &mut written).is_ok() {
                                let _ = fs::remove_file(&pj);
                            }
                        }
                        SealedRead::Unreadable => append_crash_log(
                            "prompts.json fallback present but could not be decrypted/parsed; \
                             DB left untouched, file kept for inspection",
                        ),
                        SealedRead::Missing => {}
                    }
                }
                let sj = dir.join("settings.json");
                if sj.exists() {
                    match read_sealed_json::<Settings>(&sj) {
                        SealedRead::Loaded(s) => {
                            settings = s;
                            settings_defaulted = false;
                            if db_write_settings(&conn, &settings).is_ok() {
                                let _ = fs::remove_file(&sj);
                            }
                        }
                        SealedRead::Unreadable => append_crash_log(
                            "settings.json fallback present but could not be decrypted/parsed; \
                             DB left untouched, file kept for inspection",
                        ),
                        SealedRead::Missing => {}
                    }
                }
            }
        }
        // Force a resave when the "ai" theme is about to be rewritten to
        // "neon": otherwise the fix only lives in memory until some unrelated
        // setting change happens to persist it.
        let theme_ai_before =
            settings.theme == "ai" || settings.custom_themes.iter().any(|c| c.base == "ai");
        let pruned_removed_values = settings.has_removed_ui_values();
        // An unreadable settings row, or a missing one next to existing data, belongs
        // to an existing install: it must never start pruning under the new defaults.
        if settings_unreadable
            || (settings_defaulted && (!prompts.is_empty() || copy_log_has_rows(&conn)))
        {
            settings.retention_defaults_v2 = false;
        }
        let pin_retention = !settings.retention_defaults_v2;
        settings.migrate();
        load_pick_history(&settings); // restore the colour-pick history mirror
        // One-time: move any legacy in-blob copy history into the copy_log table.
        // Gated on a persistent `meta` marker (written in the same transaction as
        // the rows) so a failed post-migration resave can never re-import and
        // duplicate the history on the next launch.
        let already_migrated = conn
            .query_row("SELECT 1 FROM meta WHERE key='copy_log_migrated'", [], |_| Ok(()))
            .is_ok();
        let mut migrated_log = false;
        if !already_migrated {
            if !settings.copy_log.is_empty() {
                if let Ok(tx) = conn.transaction() {
                    {
                        // copy_log is newest-first: insert oldest-first so rowid order
                        // stays chronological; backfill last_used with each id's newest ts.
                        if let Ok(mut stmt) = tx.prepare("INSERT INTO copy_log(ts, id) VALUES(?1, ?2)") {
                            for e in settings.copy_log.iter().rev() {
                                let _ = stmt.execute(params![e.ts as i64, e.id]);
                                if e.ts > 0 {
                                    settings.last_used.insert(e.id.clone(), e.ts);
                                }
                            }
                        }
                        let _ = tx.execute(
                            "INSERT OR REPLACE INTO meta(key, value) VALUES('copy_log_migrated', '1')",
                            [],
                        );
                    }
                    if tx.commit().is_ok() {
                        migrated_log = true;
                    }
                }
            } else {
                // No legacy history to move: mark done so we never re-check.
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO meta(key, value) VALUES('copy_log_migrated', '1')",
                    [],
                );
            }
            settings.copy_log.clear();
        }
        // Enforce the cap + retention window on the table at startup (e.g. after
        // the cap was lowered, or right after a large legacy import).
        let (cap, cutoff) = history_cap_and_cutoff(&settings);
        let _ = db_trim_copy(&conn, cap, cutoff);
        migrate_prompts(&mut prompts);
        // A relative file_path (only ever produced by demo_prompts(), never by the
        // real attachment picker, which always yields an absolute path) is resolved
        // against the live data dir here instead of being stored absolute on disk --
        // see the comment on demo_prompts()'s attachment row and on
        // resolve_relative_file_paths itself.
        resolve_relative_file_paths(&dir, &mut prompts);
        resolve_relative_file_paths(&dir, &mut trash);
        let mut store = Store {
            prompts,
            trash,
            settings,
            prompts_locked: decrypt_fails > 0,
            written: std::cell::RefCell::new(written),
        };
        acquire_data_dir_lock(&dir, data_dir_lock_enabled(&store.settings));
        // Never let an existing install see the first-run tour -- a store
        // this is discovered for is, by definition, not the one that will ever
        // write the flag itself via the tour's own Finish/Skip.
        if tour_should_be_marked_seen(!store.prompts.is_empty(), store.settings.examples_seeded) {
            store.settings.ui_flags.entry("tourSeen".to_string()).or_insert(true);
        }
        // A purge whose version-history DELETE failed is never retried by the purge
        // itself, so the text would stay in prompt_versions for good. Only when no
        // JSON fallback is waiting: then the tables really are the whole library.
        if version_sweep_allowed(&dir, DATA_DIR_READONLY.load(Ordering::Relaxed)) {
            db_sweep_orphan_versions(&conn);
        }
        // Seal every plain edit_state row an earlier version left behind, off the
        // main thread; idempotent, so running it on each load costs one cheap scan.
        {
            let app = app.clone();
            std::thread::spawn(move || {
                if let Some(conn) = db_conn(&app) {
                    edit_state_seal_all(&conn);
                }
            });
        }
        store.normalize_folders(); // Drop members whose prompt is gone
        sweep_trash(app, &mut store); // drop anything past the retention window
        if needs_mig || migrated_log {
            // One-time: re-encrypt legacy plaintext rows / drop the migrated log from the blob.
            save_prompts(app, &store);
            save_settings(app, &store.settings);
        } else if theme_ai_before || pruned_removed_values || pin_retention {
            save_settings(app, &store.settings);
        }
        return store;
    }
    // Fallback: DB unavailable → read the JSON files directly.
    let mut store = load_store_from_json(&dir);
    acquire_data_dir_lock(&dir, data_dir_lock_enabled(&store.settings));
    if tour_should_be_marked_seen(!store.prompts.is_empty(), store.settings.examples_seeded) {
        store.settings.ui_flags.entry("tourSeen".to_string()).or_insert(true);
    }
    store.normalize_folders();
    sweep_trash(app, &mut store); // no-op here (fallback path never loads trash), kept for symmetry
    store
}

// The `dataDirLock` toggle (default on) -- same "absent/true = on, explicit
// false = off" convention as colorPicker and the other default-on flags.
pub(crate) fn data_dir_lock_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("dataDirLock") != Some(&false)
}

// What a launch can still recover when the database cannot be opened at all.
// Split out of load_store so the write-lock decision below can be exercised
// without an AppHandle around it: everything here depends only on the folder.
// There is no second source to fall back to on this path, so an unreadable file
// must lock writes (like load_store's decrypt_fails case) rather than silently
// present (and then save) an empty store.
pub(crate) fn load_store_from_json(dir: &Path) -> Store {
    let mut unreadable = false;
    let mut settings_missing = false;
    let mut settings = match read_sealed_json::<Settings>(&dir.join("settings.json")) {
        SealedRead::Loaded(s) => s,
        SealedRead::Unreadable => {
            unreadable = true;
            Settings { retention_defaults_v2: false, ..Settings::default() }
        }
        SealedRead::Missing => {
            settings_missing = true;
            Settings::default()
        }
    };
    settings.migrate();
    load_pick_history(&settings); // restore the colour-pick history mirror
    prune_history(&mut settings);
    let mut prompts_missing = false;
    let mut prompts = match read_sealed_json::<Vec<Prompt>>(&dir.join("prompts.json")) {
        SealedRead::Loaded(p) => p,
        SealedRead::Unreadable => {
            unreadable = true;
            Vec::new()
        }
        SealedRead::Missing => {
            prompts_missing = true;
            Vec::new()
        }
    };
    migrate_prompts(&mut prompts);
    // Missing settings next to existing prompts is an existing install, not a new one.
    if settings_missing && !prompts.is_empty() {
        settings.retention_defaults_v2 = false;
        settings.migrate();
    }
    // A previous session already wrote data.db, so its rows still exist on disk:
    // db_conn just could not open it this time (e.g. a transient ALTER failure).
    // Without prompts.json either, this launch has literally no way to see the real
    // prompts; presenting an empty-but-unlocked store would let the very next save
    // (or a stray auto-save) write that emptiness over the table: and to
    // prompts.json, which load_store's own "101" rule then trusts over the DB on the
    // next launch, wiping it for good the moment data.db is reachable again.
    // What settings.json happens to be doing is irrelevant here: it says nothing
    // about the prompts, and a session whose settings write once fell back to the
    // file leaves exactly that combination behind: settings.json present,
    // prompts.json not.
    if !unreadable && prompts_missing && dir.join("data.db").exists() {
        unreadable = true;
    }
    if unreadable {
        append_crash_log(
            "DB unavailable and prompts.json/settings.json fallback could not be decrypted/parsed; \
             writes locked to avoid saving an empty store over real data",
        );
    }
    // No usable database was read here, so nothing is known to be on disk: the map
    // starts empty and the next successful save writes every row.
    Store {
        prompts,
        trash: Vec::new(),
        settings,
        prompts_locked: unreadable,
        written: std::cell::RefCell::new(WrittenRows::new()),
    }
}

pub(crate) fn gen_id() -> String {
    // nanos give cross-restart uniqueness; the per-process counter prevents
    // collisions when many ids are minted in the same tick (e.g. bulk import).
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("p{}_{}", nanos, seq)
}

pub(crate) fn lock<'a>(state: &'a State<'a, Db>) -> std::sync::MutexGuard<'a, Store> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

mod theme;
use theme::*;

mod floating;
use floating::*;

mod images;
use images::*;

mod prompts;
use prompts::*;

mod trash;
use trash::*;

mod views;
use views::*;

mod settings_cmds;
use settings_cmds::*;

mod stats;
use stats::*;

mod backups;
use backups::*;

mod copy;
use copy::*;

mod rec_bar;
use rec_bar::*;

// ---- Auto-paste: track the last EXTERNAL foreground window so a copied prompt
// can be pasted straight into it. A WinEvent hook records every foreground change;
// WINEVENT_SKIPOWNPROCESS drops our own windows, so the stored handle is always the
// app the user was working in (covers grid tiles, library rows AND the no-activate
// floating pills, whose click never steals foreground).
#[cfg(windows)]
mod autopaste;

mod clip;
use clip::*;

// ---- Clipboard watcher: a message-only window with AddClipboardFormatListener
// (event-driven, no polling). New external TEXT is appended to the clip_inbox table;
// our own copies and consecutive duplicates are skipped. Off by default; toggling it
// registers/unregisters the listener with no leaked handle.
// The SAME window also carries the auto-advance delayed-rendering session (Strg+V
// AND right-click-Paste both end up requesting CF_UNICODETEXT, which is the one thing
// Windows tells us about): deliberately one window, one thread for the whole session.
#[cfg(windows)]
mod clipwatch;

// ---- Windows session end (logoff, shutdown, restart) ----
//
// Windows tells a top-level window its session is ending and then takes the
// process down: no window close, no app exit event, nothing else of ours runs
// after it. The collected counter writes have to land right there. The clipboard
// watcher's window cannot do it: that one is message-only, and Windows never
// sends session messages to those: so the main window's own window procedure is
// chained instead: this one answers the two session messages and hands every
// other message straight on to the original.
#[cfg(windows)]
mod sessionend;

mod drafts;
use drafts::*;

mod folders;
use folders::*;

mod snip;
use snip::*;

mod demo;
use demo::*;

mod capture;
use capture::*;

mod ocr_cmds;
use ocr_cmds::*;

// Windows.Media.Ocr binding. Deliberately goes through the file system for the
// bitmap load (StorageFile -> BitmapDecoder -> SoftwareBitmap) rather than
// IMemoryBufferByteAccess buffer wrangling -- that keeps this module almost
// entirely free of unsafe code; the one unsafe call is COM's own RoInitialize.
#[cfg(windows)]
mod ocr;

mod voice_cmds;
use voice_cmds::*;

// Windows.Media.SpeechRecognition binding. Same shape as mod ocr: COM init
// confined to the thread that calls into WinRT, no unsafe outside RoInitialize.
// Unlike OCR, the actual recognition session runs on ONE long-lived thread it
// owns for its whole lifetime (started by voice_start, see start_session) --
// never spawn_blocking, which would starve the tokio pool for a session that
// can run for minutes.
#[cfg(windows)]
mod voice;

mod rec_engine;
use rec_engine::*;

#[cfg(windows)]
mod audio;

// System-wide key capture for the recording's "keys" overlay. Only
// active while a recording is running AND the keyOverlay setting is on -- the
// low-level keyboard hook this installs exists nowhere else in the process,
// and comes down the instant the recording stops, is cancelled, or the
// process itself ends (a hook is thread/process-scoped in Windows, so a
// crash already removes it for free). Privacy: only a virtual-key id, its
// down/up edge and a timestamp relative to the recording's start are ever
// stored -- never a character, never the focused element's text, never a
// window title. A focused password field (UI Automation's IsPassword as the
// primary signal, the classic ES_PASSWORD edit style as a second one, any
// detection failure treated as a password field) produces a gap instead of
// events for as long as it stays focused.
#[cfg(windows)]
mod keylog;

#[cfg(windows)]
mod recording;

mod rec_cmds;
use rec_cmds::*;

mod update;
use update::*;

mod bg_ops;
use bg_ops::*;

mod ffmpeg;
use ffmpeg::*;

mod video_editor;
use video_editor::*;

mod media_files;
use media_files::*;

mod autostart;
use autostart::*;

mod import_export;
use import_export::*;

mod vault;
use vault::*;

mod logging;
use logging::*;

mod webview2;
use webview2::*;

// ---- One dedicated thread per periodic background job ----
//
// Each recurring chore (settings flush, auto-backup, update check, a
// language-pack watcher, ...) gets its own thread that spends its idle time
// inside `recv_timeout` on a private control channel: woken early only by its
// own re-time/stop message, never by another job's. A chore that hangs (a
// stuck PowerShell capability query, a slow disk) therefore blocks only itself;
// every other chore keeps firing on its own schedule regardless.
//
// Only genuinely periodic work belongs here: the `loop { sleep(fixed); work(); }`
// shape, with nothing else feeding it. Event-driven threads stay where they are:
// the clipboard watcher runs a Win32 message pump and has to own its thread, and
// the tray rebuild is a one-shot debounce that fires per burst, not on a cadence.
//
// Registering is a `register_periodic(name, first_delay, interval, job)` call
// from anywhere; it spawns the job's thread. A job that reads its own interval
// out of the settings calls `retime(name, interval)` at the end of each run, so
// a changed setting takes effect from the next wait onward: the new interval
// still counts from the job's last run, exactly as before this was split across
// threads.
mod scheduler;

// How long app close waits for the timer thread to leave a running job before
// going ahead without it.
pub(crate) const SCHEDULER_STOP_WAIT_MS: u64 = 500;

pub fn run() {
    // A launch whose own arguments name one of the command line's subcommands
    // never becomes a window at all: answered by the already-running instance
    // over its own named pipe, or -- nothing running -- executed standalone
    // against the store directly. Neither path needs WebView2, a Tauri app or
    // the single-instance plugin below, so this has to run before all three.
    if let Some(code) = cli::dispatch_from_launch_args() {
        std::process::exit(code);
    }
    // A plain second launch (double-click, autostart, the tray shortcut) or a
    // promptsaver:// link settles here instead of trusting the
    // single-instance plugin below to notice on its own: that plugin forwards
    // and exits unconditionally, with no way to tell "the running instance
    // brought up a window" from "the running instance is hung and never will"
    // -- exactly how prompt-saver.exe was once found running with zero
    // top-level windows and no way back short of killing it by hand. This
    // pings the same pipe cli.rs answers CLI commands on, with its own short
    // timeout, and only falls through to the plugin's mutex-based detection
    // (or, with nothing listening at all, straight into this process's own
    // setup) for a live instance to actually forward the link to. A link's
    // own ping never raises a window and its HandedOff is turned back into
    // Primary, so it can never end this launch here on its own.
    if !early_allow_multiple_instances() {
        let argv: Vec<String> = std::env::args().skip(1).collect();
        if let cli::LaunchDecision::HandedOff = cli::resolve_second_launch(&argv) {
            std::process::exit(0);
        }
    }
    #[cfg(windows)]
    if !ensure_webview2() {
        return;
    }
    // Surface panics on stderr (dev console) AND append a no-PII line to a local
    // crash log, so a failure in the field leaves a diagnosable trace.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("{}", info);
        append_crash_log(&info.to_string());
    }));

    let mut builder = tauri::Builder::default();
    // Only one instance app-wide (keyed by app identifier, independent of the exe
    // location) unless the user opted into "Allow multiple instances" (expert menu,
    // default off): then this plugin is left out entirely, since it enforces the
    // limit itself before .setup() ever runs (there is no AppHandle yet to consult
    // the setting the normal way; early_allow_multiple_instances() peeks the DB
    // directly for exactly this reason). A second launch, when the plugin IS
    // attached, closes itself and focuses the first instance.
    if !early_allow_multiple_instances() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // A promptsaver:// link relaunching this already-running instance
            // takes the exact same route a second command-line launch does --
            // handled here instead of stealing the window's focus first for a
            // link whose whole point is to copy quietly in the background.
            let scheme_url = remote::find_scheme_arg(&args).map(str::to_string);
            let is_copy_link = scheme_url.as_deref().is_some_and(remote::is_copy_link);
            if let Some(url) = &scheme_url {
                remote::handle_launch_url(app, url);
            }
            if !is_copy_link {
                show_main(app);
            }
        }));
    }
    builder
        // The app hotkey summons the window + quick-launcher; a prompt hotkey
        // copies that prompt wherever the user is.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        return;
                    }
                    // Read the owner out first: the handler must not call back into
                    // the app while the lookup lock is held.
                    let owner = app.try_state::<HotkeyTargets>().and_then(|targets| {
                        let list = targets.0.lock().unwrap_or_else(|e| e.into_inner());
                        list.iter().find(|(sc, _)| sc == shortcut).map(|(_, o)| o.clone())
                    });
                    match owner {
                        Some(HotkeyOwner::Prompt(id)) => fire_prompt_hotkey(app, &id),
                        Some(HotkeyOwner::Voice) => toggle_voice_from_hotkey(app),
                        Some(HotkeyOwner::RecStop) => trigger_rec_stop_hotkey(app),
                        Some(HotkeyOwner::RecPause) => trigger_rec_pause_hotkey(app),
                        _ => summon_launcher(app), // the app hotkey (or an unknown leftover)
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            // Before load_store: a sealing failure during the very first read/resave
            // should already be able to reach a window, once one exists.
            {
                let h = handle.clone();
                *SEAL_NOTIFY.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(Arc::new(move || {
                        let _ = h.emit("store-unsealed", ());
                    }));
            }
            sweep_stale_temp_files(); // crash/abnormal-exit leftovers from a previous run
            // Registered in the CANONICAL dir (never the "datapath" redirect,
            // which demo mode itself flips) so enable_demo_mode/
            // disable_demo_mode can tell a real second running instance apart
            // from the allowMultipleInstances setting -- see
            // other_instance_running's own comment.
            register_running_instance(&default_data_dir(&handle));
            let store = load_store(&handle);
            // A one-off pass over whatever the log already holds -- see its own
            // comment for why this cannot just check today's own account name.
            scrub_user_paths_from_crash_log();
            // Seed the crash-log rotation cap from whatever was persisted, before
            // anything below can possibly call append_crash_log.
            if let Some(kb) = store.settings.ui_values.get("errorLogKeepKb").copied() {
                ERROR_LOG_KEEP_BYTES.store((kb.max(1.0) * 1024.0) as u64, Ordering::Relaxed);
            }
            // Ditto, for undecided AND broken recordings -- Db is not managed
            // yet at this point, so this reads straight off the just-loaded `store`.
            {
                let referenced = orphan_referenced_set(&store);
                let broken_on = store.settings.ui_flags.get("recSweepBroken") != Some(&false);
                let retention_days = store.settings.ui_values.get("recordingRetentionDays").copied().unwrap_or(RECORDING_RETENTION_DAYS_DEFAULT);
                let retention_age = (retention_days > 0.0).then(|| Duration::from_secs(retention_days as u64 * 86_400));
                let (count, bytes) = sweep_stale_recordings(&handle, &referenced, broken_on, Duration::from_secs((REC_BROKEN_GRACE_MIN * 60.0) as u64), retention_age);
                if count > 0 {
                    append_crash_log(&format!("startup recording sweep: removed {count} file(s), {bytes} bytes"));
                    *PENDING_SWEEP_NOTICE.lock().unwrap_or_else(|e| e.into_inner()) = Some((count, bytes));
                }
            }

            // Same "Db not managed yet" situation as the recording sweep
            // above -- read the TTL straight off `store` for this one startup
            // pass; the periodic job below re-reads it live via app state.
            sweep_clip_export_temp(clip_export_ttl_from(&store.settings));

            let saved_geom = store.settings.window;
            let autostart = store.settings.autostart;
            let start_min = store.settings.start_minimized;
            let url_scheme_on = remote::url_scheme_enabled(&store.settings);
            let backup_start_delay_s = store
                .settings
                .ui_values
                .get("backupStartDelayS")
                .copied()
                .unwrap_or(20.0)
                .clamp(0.0, 300.0) as u64;

            app.manage(Mutex::new(store));
            app.manage(SnipState(Mutex::new(Vec::new())));
            app.manage(SnipPreview(Arc::new(Mutex::new(None))));
            app.manage(SnipModeState(Mutex::new(SnipMode::Capture)));
            app.manage(HotkeyTargets(Mutex::new(Vec::new())));
            app.manage(FolderExpand(Mutex::new(HashMap::new())));
            app.manage(HiddenFloats(Mutex::new(std::collections::HashSet::new())));
            app.manage(ParkedFloats(Mutex::new(HashMap::new())));
            app.manage(DragHint(Mutex::new((0, None)))); // live merge highlight
            app.manage(VarPrompt::new()); // standalone fill-in window
            app.manage(VideoEditorPending::new()); // video editor's pending target path
            app.manage(SyncWriteQueue(Mutex::new(SyncWriteState::default())));
            app.manage(VoiceState(Mutex::new(None), std::sync::atomic::AtomicBool::new(false))); // live dictation session
            app.manage(RecordingState(Mutex::new(None))); // live screen-recording session
            app.manage(camera::CameraState(Mutex::new(None))); // live camera-recording session
            app.manage(remote::RemoteApiState::new());
            app.manage(remote::PendingLaunchUrl(Default::default()));
            // Same "never starts without a password" rule start() enforces on
            // every caller; harmless to call unconditionally at every boot.
            let _ = remote::start(handle.clone());
            // A promptsaver:// link that started this very process (the app was
            // not already running yet) waits in PendingLaunchUrl until the main
            // window's script asks for it -- see run_pending_launch_url.
            if let Some(url) = std::env::args().skip(1).find(|a| a.starts_with("promptsaver://")) {
                if let Ok(mut g) = handle.state::<remote::PendingLaunchUrl>().0.lock() {
                    *g = Some(url);
                }
            }

            // Auto-paste: start tracking the last external foreground window.
            #[cfg(windows)]
            autopaste::install_hook();

            // The message-only window is harmless to run unconditionally (same
            // reasoning as the sync poll below): on_clipboard_update itself checks the
            // clipWatcher opt-in on every event, and 91's auto-advance session needs this
            // window regardless of that flag.
            #[cfg(windows)]
            clipwatch::start(handle.clone());

            // The default sync folder used to be called differently. Anyone who was
            // syncing into it without ever picking a folder of their own keeps it,
            // now as an explicit choice, so nothing moves under them unannounced.
            if let Some(state) = handle.try_state::<Db>() {
                let legacy = data_dir(&handle).join(SYNC_LEGACY_DIR_NAME);
                let adopted = {
                    let mut store = lock(&state);
                    // Checked here as well as inside, so that every later start is a
                    // map lookup and not a settings write for nothing.
                    if store.settings.ui_texts.contains_key(SYNC_LEGACY_MARK) {
                        None
                    } else {
                        let adopted = sync_adopt_legacy_dir(&mut store.settings, &legacy);
                        save_settings(&handle, &store.settings);
                        adopted
                    }
                };
                if let Some(path) = adopted {
                    append_crash_log(&format!(
                        "prompt-sync: kept the existing folder {path} as this installation's sync folder; new installations use {}",
                        sync_default_dir(&handle).display()
                    ));
                }
            }

            // Prompt-folders-as-Markdown: harmless to call unconditionally at setup;
            // if the feature is off this loop checks once and ends itself immediately
            // rather than ticking forever for nothing. set_ui_flag starts a fresh one
            // when the feature is switched back on.
            start_sync_poll(handle.clone());

            // One shared poll instead of every window running its own -
            // see start_missing_files_poll.
            start_missing_files_poll(handle.clone());

            // Take a backup whenever the newest one is older than the interval --
            // a short delay after start (backupStartDelayS, 0-300s, default 20 --
            // long enough for the window and store to settle, short enough that a
            // backup due at launch no longer waits out a full BACKUP_CHECK_SECS
            // tick), then on every tick for as long as the app runs. The settings
            // are re-read each round, so switching auto-backup off or changing the
            // interval takes effect without a restart. Off the UI thread so it
            // never delays the first paint.
            {
                let h = handle.clone();
                scheduler::register_periodic(
                    "auto-backup",
                    Duration::from_secs(backup_start_delay_s),
                    Duration::from_secs(BACKUP_CHECK_SECS),
                    move || auto_backup_tick(&h),
                );
            }

            // Keeps our own data-folder lock fresh for as long as this process
            // owns it -- a no-op while read-only or while the dataDirLock
            // toggle is off, see refresh_data_dir_lock/acquire_data_dir_lock.
            {
                let h = handle.clone();
                scheduler::register_periodic(
                    "data-dir-lock",
                    Duration::from_secs(DATA_DIR_LOCK_REFRESH_SECS),
                    Duration::from_secs(DATA_DIR_LOCK_REFRESH_SECS),
                    move || refresh_data_dir_lock(&data_dir(&h)),
                );
            }

            // Self-heal a rec-bar window that exists but somehow ended up
            // invisible while its recording is still running -- see rec_bar_heal_tick.
            {
                let h = handle.clone();
                scheduler::register_periodic(
                    "rec-bar-heal",
                    Duration::from_secs(5),
                    Duration::from_secs(5),
                    move || rec_bar_heal_tick(&h),
                );
            }

            // Answers a second, command-line launch of this same exe -- see cli.rs.
            cli::install_server(&handle);

            // "Only at startup" left a crashed recording's leftover sitting for
            // however long the app happened to stay open next -- the same flaw as any
            // other startup-only sweep. First delay = one full interval since the
            // startup sweep above just ran; each round re-times itself off the
            // fixed REC_SWEEP_EVERY_MIN interval.
            {
                let h = handle.clone();
                scheduler::register_periodic("recording-sweep", recording_sweep_interval(), recording_sweep_interval(), move || {
                    sweep_recordings_and_notify(&h);
                    scheduler::retime("recording-sweep", recording_sweep_interval());
                });
            }

            // Remembered unfinished-edit rows for a file that is gone, or old enough
            // to age out -- see sweep_edit_state_tick. A daily tick is plenty: unlike
            // stale recording files these cost a few bytes each, so leaving one a
            // little longer than strictly necessary is harmless.
            {
                let h = handle.clone();
                scheduler::register_periodic("edit-state-sweep", Duration::from_secs(60), Duration::from_secs(86400), move || {
                    sweep_edit_state_tick(&h);
                });
            }

            // A pill/folder parked long enough (floatParkCleanupH, default 0/off)
            // loses only its view -- exactly the existing "hide for now" mechanism,
            // never the entry in settings.floating/float_folders and never its
            // screen id, so it is still there (and still returns on its own) the
            // moment that screen comes back.
            {
                let h = handle.clone();
                scheduler::register_periodic("float-park-cleanup", Duration::from_secs(60), Duration::from_secs(60), move || {
                    let cleanup_h = h
                        .try_state::<Db>()
                        .map(|s| {
                            s.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_values.get("floatParkCleanupH").copied().unwrap_or(0.0)
                        })
                        .unwrap_or(0.0);
                    if cleanup_h <= 0.0 {
                        return;
                    }
                    let expired = h
                        .try_state::<ParkedFloats>()
                        .map(|s| {
                            let map = s.0.lock().unwrap_or_else(|e| e.into_inner());
                            let now = std::time::Instant::now();
                            let ages: HashMap<String, Duration> =
                                map.iter().map(|(id, at)| (id.clone(), now.saturating_duration_since(*at))).collect();
                            ids_past_park_cleanup(&ages, cleanup_h)
                        })
                        .unwrap_or_default();
                    if expired.is_empty() {
                        return;
                    }
                    if let Some(hidden) = h.try_state::<HiddenFloats>() {
                        let mut hs = hidden.0.lock().unwrap_or_else(|e| e.into_inner());
                        for id in &expired {
                            hs.insert(id.clone());
                        }
                    }
                    if let Some(parked) = h.try_state::<ParkedFloats>() {
                        let mut pf = parked.0.lock().unwrap_or_else(|e| e.into_inner());
                        for id in &expired {
                            pf.remove(id);
                        }
                    }
                    sync_float_windows(&h);
                    for id in &expired {
                        let _ = h.emit_to("main", "float-park-cleaned-up", serde_json::json!({ "id": id }));
                    }
                });
            }

            // Re-arm the app hotkey plus every prompt accelerator. Each
            // registration is a round trip to the main thread, so this runs on a
            // worker: the first paint never waits for it, however many prompts
            // carry a hotkey.
            {
                let h = handle.clone();
                std::thread::spawn(move || {
                    rearm_hotkeys(&h);
                });
            }

            // Launched by autostart with --minimized: stay in the tray.
            let start_hidden = std::env::args().any(|a| a == "--minimized");

            // Capture exclusion is re-read fresh from Db state inside build_main_window itself.
            if let Ok(main) = build_main_window(&handle) {
                let geom = resolve_geometry(&main, saved_geom);
                let _ = main.set_size(tauri::LogicalSize::new(geom.width, geom.height));
                let _ = main.set_position(PhysicalPosition::new(geom.x, geom.y));
                // A pill/folder left fully off-screen by an unplugged monitor is
                // never dropped here any more -- sync_float_windows() below (via
                // open_floating/open_folder_window) parks it on the main screen
                // instead, keeping its real position and screen id untouched so
                // it returns on its own once that screen is back.
                // Always-on-top and the window background are applied inside
                // build_main_window, so a rebuilt window arrives dressed the
                // same way this one does.
                // The window is revealed by the frontend (show_main_window) once
                // the first layout pass is done: no visible text re-sizing.
                // Safety net: show after 1.5s even if the frontend never calls.
                if !start_hidden {
                    let h = handle.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        if let Some(w) = h.get_webview_window("main") {
                            // Show on uncertainty too: this is the safety net.
                            if !w.is_visible().unwrap_or(false) {
                                let _ = w.show();
                            }
                        }
                    });
                }
                if let Some(state) = handle.try_state::<Db>() {
                    state.lock().unwrap_or_else(|e| e.into_inner()).settings.window = Some(geom);
                }
                // Event wiring (Moved/Resized/ThemeChanged/CloseRequested/Destroyed)
                // already happened inside build_main_window -- the single place
                // that registers it, whether this is the window Tauri just created
                // from tauri.conf.json or one rebuilt after the process outlived it.
            }

            // System tray: left-click or "Open" restores; "Quit" exits for real.
            let lang = {
                let state: State<Db> = app.state();
                let pref = state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .settings
                    .language
                    .clone();
                resolve_lang(&pref)
            };
            let (open_label, quit_label) = tray_labels(lang);
            let show_item = MenuItem::with_id(app, "show", open_label, true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let mut tray = TrayIconBuilder::with_id("tray")
                .tooltip(brand::DISPLAY_NAME)
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| handle_tray_menu_id(app, event.id.as_ref()))
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    } else if let TrayIconEvent::Click {
                        button: MouseButton::Right,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        // The native popup only shows up here when trayCustomMenu is
                        // off (apply_tray_menu detaches its menu entirely while it is
                        // on) -- otherwise this window would appear right next to it.
                        let app = tray.app_handle();
                        let custom = app
                            .try_state::<Db>()
                            .map(|s| tray_custom_menu_on(&lock(&s).settings))
                            .unwrap_or(false);
                        if custom {
                            // Same root cause as the show_rec_bar hang: this
                            // handler runs on the main thread, and building a webview
                            // window from inside it never returns. Queue the build for
                            // the next main-thread turn instead, with a log line on each
                            // side so a future hang would be visible instead of silent.
                            let app2 = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let app3 = app2.clone();
                                let _ = app2.run_on_main_thread(move || {
                                    append_crash_log("tray window: about to build");
                                    open_tray_window(&app3);
                                    append_crash_log("tray window: build ok");
                                });
                            });
                        }
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            apply_tray_menu(&handle); // Adds the quick-copy sections if they are on

            // Refresh autostart and the address scheme registrations (also follows
            // the exe when it moved). Nothing is removed here and another copy's
            // entry is never touched.
            let demo = data_dir_is_demo(&handle, &data_dir(&handle));
            reconcile_windows_state(autostart, start_min, url_scheme_on, demo);
            // A quit or crash mid-pack leaves raw copies and folders without a snapshot.
            cleanup_incomplete_backups(&backups_dir(&handle), Duration::from_secs(60));
            // A read-only instance never writes into the folder another machine owns.
            if !DATA_DIR_READONLY.load(Ordering::Relaxed) {
                remove_stale_staged_restores(&data_dir(&handle), Duration::from_secs(60));
                // Staged exports a killed run left in the user's folders.
                let journal = data_dir(&handle).join("export-staging.txt");
                sweep_stale_exports(&journal, &local_host_name(), pid_is_alive);
                let _ = EXPORT_JOURNAL.set(journal);
            }

            // One place decides which pills and which folders belong on screen.
            sync_float_windows(&handle);

            // Pre-warm the snip overlay shortly after launch (off the startup hot
            // path) so even the FIRST screenshot opens instantly: the WebView2
            // cold-start is paid here in the background, not on first use.
            let h_pw = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                let h = h_pw.clone();
                let _ = h_pw.run_on_main_thread(move || {
                    if h.get_webview_window("snip-0").is_none() {
                        // Just build it (hidden) so the first snip skips WebView2
                        // cold-start. Waking it from throttle + its first paint are
                        // handled by the off-screen show on the reuse path in open_snip.
                        let _ = build_snip_window(&h);
                    }
                });
            });

            // Update check: right after launch, then once a day (if enabled).
            {
                let h2 = handle.clone();
                scheduler::register_periodic(
                    "update-check",
                    Duration::from_secs(UPDATE_CHECK_FIRST_DELAY_SECS),
                    update_check_interval(&handle),
                    move || {
                        update_check_tick(&h2);
                        // Expert-tunable interval, re-read after every round: the
                        // change takes effect from the next wait onward.
                        scheduler::retime("update-check", update_check_interval(&h2));
                    },
                );
            }

            // Lands the collected rotation-pointer / usage-counter writes once
            // the copying has been quiet for SETTINGS_QUIET_MS. A round with
            // nothing owed costs one uncontended mutex.
            {
                let h = handle.clone();
                scheduler::register_periodic(
                    "settings-flush",
                    Duration::from_millis(SETTINGS_FLUSH_TICK_MS),
                    Duration::from_millis(SETTINGS_FLUSH_TICK_MS),
                    move || settings_flush_tick(&h),
                );
            }

            // Sweep the temp dir again now and then, not only at launch: an
            // instance left running for days otherwise never collects the
            // leftovers a crashed sibling dropped after it started.
            scheduler::register_periodic(
                "temp-sweep",
                Duration::from_secs(STALE_TEMP_SWEEP_SECS),
                Duration::from_secs(STALE_TEMP_SWEEP_SECS),
                sweep_stale_temp_files,
            );

            // The clip-export sweep needs its own finer-grained cadence since
            // videoClipTtlMin can be as low as 5 minutes -- re-reads the live
            // setting every round rather than a value captured once at
            // registration time.
            {
                let h = handle.clone();
                scheduler::register_periodic(
                    "clip-export-sweep",
                    Duration::from_secs(CLIP_EXPORT_SWEEP_SECS),
                    Duration::from_secs(CLIP_EXPORT_SWEEP_SECS),
                    move || { sweep_clip_export_temp(clip_export_ttl(&h)); },
                );
            }

            // OCR language-pack "updatable" watcher -- delayed, silent when
            // there is nothing to do, never runs at window build.
            #[cfg(windows)]
            {
                start_lang_update_watcher(handle.clone(), ocr::CapKind::Ocr);
                start_lang_update_watcher(handle, ocr::CapKind::Speech);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            prompts::get_prompt,
            prompts::add_prompt,
            prompts::seed_example_prompts,
            prompts::dismiss_example_prompts,
            prompts::example_prompts_pending,
            prompts::update_prompt,
            video_editor::backfill_media_meta,
            prompts::set_favorite,
            prompts::delete_prompt,
            prompts::batch_prompts,
            trash::list_trash,
            trash::restore_prompt,
            trash::placement_snapshot,
            trash::restore_prompts_batch,
            trash::purge_prompt,
            trash::empty_trash,
            logging::log_error,
            logging::list_error_log,
            logging::clear_error_log,
            prompts::list_versions,
            prompts::last_edited_map,
            prompts::restore_version,
            prompts::restore_version_as_new,
            stats::usage_stats,
            stats::export_stats,
            stats::storage_breakdown,
            stats::open_storage_folder,
            stats::retention_overview,
            trash::sweep_trash_now,
            copy::trim_copy_history_now,
            snip::sweep_clip_export_temp_now,
            drafts::sweep_edit_states_now,
            prompts::sweep_discarded_recordings_now,
            stats::record_tool_usage,
            stats::find_duplicates,
            stats::similar_to_saved,
            stats::ignore_dups,
            backups::list_backups,
            backups::open_backups_folder,
            backups::create_backup,
            backups::delete_backup,
            backups::restore_backup,
            backups::backup_contents,
            clip::clip_inbox_list,
            clip::clip_inbox_dismiss,
            clip::clip_inbox_clear,
            clip::restore_clip_inbox,
            drafts::purge_clip_inbox_from_backups,
            drafts::stat_file,
            drafts::save_edit_state,
            drafts::load_edit_state,
            drafts::discard_edit_state,
            drafts::save_editor_draft,
            drafts::load_editor_draft,
            drafts::discard_editor_draft,
            main_window::open_repo,
            main_window::third_party_notices,
            main_window::set_ui_zoom,
            trash::delete_all_data,
            trash::reset_settings,
            trash::set_language,
            trash::set_tile_style,
            views::set_layout,
            views::set_view_grid,
            views::rename_tag,
            views::delete_tag,
            views::set_tag_color,
            views::set_tag_colors,
            views::add_view,
            views::rename_view,
            views::set_view_color,
            views::reorder_views,
            views::remap_colors,
            views::delete_view,
            views::set_active_view,
            snip::copy_image_data,
            views::save_tile_folder,
            views::take_out_of_tile_folder,
            views::delete_tile_folder,
            settings_cmds::get_settings,
            settings_cmds::get_state,
            settings_cmds::prompt_image,
            vault::secret_vault_status,
            vault::secret_vault_unlock,
            vault::secret_vault_lock,
            vault::set_prompt_secret,
            settings_cmds::launcher_items,
            theme::current_theme,
            theme::theme_vars,
            theme::ink_prefs,
            theme::set_theme,
            theme::save_custom_theme,
            theme::delete_custom_theme,
            theme::export_custom_theme,
            theme::import_custom_theme,
            tray::launcher_select,
            tray::launcher_close,
            tray::launcher_ready,
            tray::tray_window_plan,
            tray::tray_window_activate,
            tray::tray_window_close,
            tray::tray_open_launcher,
            tray::tray_launcher_prefill,
            copy::copy_prompt,
            copy::rotation_peek,
            copy::rotation_commit,
            copy::rotation_reset,
            copy::rotation_arm_session,
            copy::rotation_end_auto,
            copy::copy_text,
            copy::copy_formatted,
            copy::clipboard_text,
            copy::record_copy,
            copy::clear_copy_history,
            copy::restore_copy_history,
            copy::recent_copies,
            copy::delete_copy_entry,
            floating::toggle_floating,
            floating::hide_float_for_now,
            floating::float_recall,
            floating::is_float_parked,
            floating::set_float_scale,
            floating::resize_float_pill,
            floating::resize_float_media,
            drafts::set_float_bounds,
            folders::set_folder_position,
            folders::float_folder_drop,
            folders::float_drag_hint,
            folders::float_folder_state,
            folders::expand_float_folder,
            folders::collapse_float_folder,
            folders::set_float_folder_scale,
            folders::rename_float_folder,
            folders::folder_rename_request,
            folders::dissolve_float_folder,
            folders::remove_folder_member,
            folders::set_video_prefs,
            media_files::edit_prompt_request,
            media_files::show_main_window,
            update::app_version,
            update::check_update,
            update::install_update,
            ffmpeg::ffmpeg_status,
            ffmpeg::download_ffmpeg,
            ffmpeg::cancel_ffmpeg_download,
            bg_ops::set_taskbar_progress,
            bg_ops::list_bg_ops,
            bg_ops::cancel_all_bg_ops,
            bg_ops::open_bg_op_folder,
            bg_ops::confirm_quit_with_running_ops,
            update::set_auto_update,
            shortcuts::set_hotkey,
            shortcuts::check_hotkey,
            shortcuts::set_hotkey_capture,
            shortcuts::focus_main_window,
            var_prompt::open_var_window,
            var_prompt::var_window_payload,
            var_prompt::var_window_result,
            var_prompt::var_window_fit,
            var_prompt::os_user_name,
            update::set_bars,
            autostart::set_minimize_on_close,
            autostart::set_always_on_top,
            update::set_ui_flag,
            update::set_ui_value,
            update::set_ui_text,
            update::set_screenshot_dir,
            update::set_video_export_dir,
            update::set_recording_dir,
            update::reset_expert,
            remote::remote_api_take_shown_password,
            remote::remote_api_regenerate_password,
            remote::remote_api_status,
            remote::remote_api_stop_now,
            remote::set_remote_allowed_ranges,
            remote::remote_tls_fingerprint,
            remote::remote_tls_save_cert,
            remote::remote_tls_renew,
            remote::remote_tls_set_short_lived,
            remote::run_pending_launch_url,
            update::skip_version,
            update::whats_new_check,
            update::whats_new_current,
            update::whats_new_refresh,
            snip::open_snip,
            snip::open_picker,
            snip::open_recorder,
            snip::open_recorder_switch,
            snip::recorder_target_picked,
            snip::pick_tile,
            snip::pick_color,
            snip::pick_history,
            snip::copy_pick_history_entry,
            snip::snip_background,
            snip::snip_present,
            snip::snip_should_begin,
            snip::snip_windows,
            snip::snip_cancel,
            capture::capture_region,
            capture::capture_window,
            images::pdf_preview,
            images::attachment_text_preview,
            autostart::set_autostart,
            autostart::set_start_minimized,
            import_export::export_prompts,
            import_export::preview_import,
            import_export::import_prompts,
            import_export::preview_import_folder,
            import_export::import_folder_files,
            vault::export_all,
            vault::import_all,
            vault::pick_backup_file,
            vault::export_settings_only,
            vault::pick_settings_import_file,
            vault::preview_settings_import,
            vault::apply_settings_import,
            trash::takeover_offer,
            trash::takeover_apply,
            vault::set_backup_password,
            vault::has_backup_password,
            images::get_clipboard_image,
            images::get_clipboard_file_path,
            images::pick_file_path,
            images::pick_media_file_path,
            media_files::open_file_in_editor,
            images::load_image_file,
            images::missing_files,
            snip::save_screenshot_now,
            snip::save_screenshot_as,
            snip::delete_screenshot_file,
            snip::default_screenshot_dir,
            snip::default_sync_dir,
            md_sync::set_sync_dir,
            md_sync::sync_dir_info,
            md_sync::sync_status,
            snip::pick_folder,
            snip::current_data_dir,
            store_sealed,
            settings_locked,
            settings_write_failed,
            data_dir_lock_status,
            path_sync_folder_hint,
            current_data_dir_sync_hint,
            snip::set_data_dir,
            snip::delete_data_dir,
            demo::enable_demo_mode,
            demo::disable_demo_mode,
            clip::paste_into_previous,
            ocr_cmds::ocr_languages,
            ocr_cmds::ocr_image,
            ocr_cmds::ocr_image_data,
            ocr_cmds::search_image_text,
            ocr_cmds::ocr_index_backfill,
            ocr_cmds::cancel_ocr_index_backfill,
            ocr_cmds::ocr_capabilities,
            ocr_cmds::ocr_capabilities_elevate,
            ocr_cmds::ocr_capabilities_elevated_result,
            ocr_cmds::ocr_install_language,
            ocr_cmds::ocr_install_result,
            ocr_cmds::ocr_lang_snooze,
            snip::set_snip_mode,
            voice_cmds::voice_support,
            voice_cmds::voice_languages,
            voice_cmds::voice_start,
            voice_cmds::voice_stop,
            voice_cmds::voice_enable_online_speech,
            ocr_cmds::speech_capabilities,
            ocr_cmds::speech_capabilities_elevate,
            ocr_cmds::speech_capabilities_elevated_result,
            ocr_cmds::speech_install_language,
            ocr_cmds::speech_install_result,
            ocr_cmds::voice_lang_snooze,
            voice_cmds::open_privacy_mic,
            shortcuts::set_voice_hotkey,
            shortcuts::set_rec_stop_hotkey,
            shortcuts::set_rec_pause_hotkey,
            rec_cmds::start_recording,
            rec_cmds::switch_recording_target,
            rec_cmds::finish_and_continue_recording,
            rec_cmds::check_recording_disk_space,
            rec_cmds::check_export_disk_space,
            rec_cmds::check_export_disk_space_batch,
            rec_cmds::pause_recording,
            rec_cmds::resume_recording,
            rec_cmds::stop_recording,
            rec_cmds::cancel_recording,
            ffmpeg::convert_recording,
            ffmpeg::cancel_recording_convert,
            ffmpeg::crop_recording_to_used_area,
            ffmpeg::cancel_recording_crop,
            video_editor::open_video_editor,
            media_files::video_editor_source,
            video_editor::probe_recording,
            video_editor::probe_extra_video_source,
            video_editor::read_key_sidecar,
            camera::open_camera_window,
            camera::camera_begin_recording,
            camera::camera_append_chunk,
            camera::camera_finish_recording,
            camera::camera_mark_finishing,
            camera::camera_discard_recording,
            camera::camera_abandon_recording,
            camera::read_camera_sidecar,
            camera::camera_screen_elapsed_ms,
            video_editor::windows_fonts_dir,
            video_editor::mux_mic_track,
            video_editor::generate_filmstrip,
            video_editor::generate_waveform_peaks,
            video_editor::extract_still_frame,
            video_editor::export_video,
            video_editor::export_recordings_batch,
            video_editor::estimate_export_size,
            video_editor::cancel_export,
            video_editor::cancel_export_job,
            media_files::save_recording_as,
            media_files::pick_export_folder,
            media_files::save_recording_now,
            media_files::copy_recording_to_clipboard,
            media_files::clip_export_path,
            media_files::copy_file_to_clipboard,
            media_files::default_recording_dir,
            media_files::video_folder_stats,
            media_files::video_folder_cleanup_preview,
            media_files::video_folder_cleanup,
            media_files::video_folder_file_durations_ms,
            media_files::discard_recording_file,
            media_files::list_discarded_recordings,
            media_files::restore_discarded_recording,
            media_files::delete_discarded_recording_now,
            prompts::take_recording_sweep_notice,
            prompts::sweep_recordings_now,
            rec_bar::show_rec_bar,
            rec_bar::fit_rec_bar,
            rec_bar::hide_rec_bar,
            rec_bar::rec_bar_mark_moved,
            rec_cmds::set_audio_muted,
            rec_bar::recording_status,
            rec_bar::list_mic_devices
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                // Backstop for every way out of the app: whatever counter state was
                // still being collected goes to disk before the process does.
                flush_settings_now(app);
                // Matches register_running_instance() in setup() above -- an
                // orderly exit un-registers this pid immediately; a crash or
                // task-kill leaves it for other_instance_running() to prune
                // opportunistically the next time anything checks.
                unregister_running_instance(&default_data_dir(app));
                // The timer thread is normally parked for hours; tell it to leave
                // rather than tearing the process down around a sleeping thread.
                scheduler::shutdown(Duration::from_millis(SCHEDULER_STOP_WAIT_MS));
                remove_own_temp_leftovers();
            }
        });
}

mod md_sync;
use md_sync::*;

#[cfg(test)]
mod tests;
