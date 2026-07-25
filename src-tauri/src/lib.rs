// Prompt Saver backend (Tauri v2). Local SQLite storage, clipboard, import/export,
// multiple views, frameless floating quick-copy windows. No network, 100% offline.

use image::imageops::FilterType;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};

// Bring the main window back from the tray — always at the saved size/position.
fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if let Some(state) = app.try_state::<Db>() {
            let geom = state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .settings
                .window;
            if let Some(g) = geom {
                let _ = win.set_size(tauri::LogicalSize::new(g.width, g.height));
                let _ = win.set_position(PhysicalPosition::new(g.x, g.y));
            }
        }
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

// Global-hotkey action: bring the window forward and tell the UI to open the
// quick-launcher (the prompt library with its search focused).
fn summon_launcher(app: &AppHandle) {
    show_main(app);
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.emit("summon-launcher", ());
    }
}

// Register a new global hotkey (empty = disable). Returns an error the UI can
// show if the accelerator is invalid or already taken by another app.
#[tauri::command]
fn set_hotkey(app: AppHandle, state: State<Db>, hotkey: String) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let hk = hotkey.trim().to_string();
    let result = if hk.is_empty() {
        Ok(())
    } else {
        match hk.parse::<Shortcut>() {
            Ok(sc) => gs.register(sc).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        }
    };
    let mut store = lock(&state);
    store.settings.hotkey = if result.is_ok() { hk } else { String::new() };
    save_settings(&app, &store.settings);
    result
}

// ---------- Data model ----------

#[derive(Serialize, Deserialize, Clone)]
struct Prompt {
    id: String,
    name: String,
    text: String,
    // Optional tile color (hex); empty = default surface color.
    #[serde(default)]
    color: String,
    // Optional PNG data URL (scaled to ≤1024px); empty = no image.
    #[serde(default)]
    image: String,
    // When true the tile shows the image instead of the name text.
    #[serde(default)]
    show_image: bool,
    // True = clicking copies the image itself; false = the image is only an
    // icon and clicking copies the text.
    #[serde(default)]
    copy_image: bool,
    // Attached file: clicking puts the file itself on the clipboard.
    #[serde(default)]
    file_path: String,
    // Gif/video used as the tile icon only — shown, never copied.
    #[serde(default)]
    icon_path: String,
    // Optional caption shown over media tiles (size 0 = default).
    #[serde(default)]
    caption: String,
    #[serde(default)]
    caption_size: u32,
    // Per-tile style overrides; empty / 0 = follow the global settings.
    #[serde(default)]
    font: String,
    #[serde(default)]
    font_size: u32,
    // Marked as a favorite (starred) in the library.
    #[serde(default)]
    favorite: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct Pos {
    x: i32,
    y: i32,
}

// Saved main-window geometry (physical pixels). Validated against monitors on load.
#[derive(Serialize, Deserialize, Clone, Copy)]
struct WindowGeom {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

// A named page with its own grid size and one placement map per grid size,
// so switching grid dimensions restores the arrangement saved for that size.
#[derive(Serialize, Deserialize, Clone)]
struct View {
    id: String,
    name: String,
    #[serde(default = "default_cols")]
    cols: u32,
    #[serde(default = "default_rows")]
    rows: u32,
    // "6x5" -> promptId -> [col,row]
    #[serde(default)]
    layouts: HashMap<String, HashMap<String, [u32; 2]>>,
    // Optional tab color (hex); empty = default.
    #[serde(default)]
    color: String,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct VideoPrefs {
    volume: u32, // 0..=100
    muted: bool,
    #[serde(rename = "loop")]
    looped: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    #[serde(default = "default_theme")]
    theme: String,
    #[serde(default)]
    floating: HashMap<String, Pos>,
    // Per-floating-button size factor (1.0 = default).
    #[serde(default)]
    float_scale: HashMap<String, f64>,
    // Per-prompt video player state (volume, mute, loop), grid and pill.
    #[serde(default)]
    video_prefs: HashMap<String, VideoPrefs>,
    #[serde(default)]
    window: Option<WindowGeom>,
    #[serde(default)]
    minimize_to_tray: bool,
    #[serde(default)]
    autostart: bool,
    #[serde(default)]
    start_minimized: bool,
    // Keep the main window above all other windows (except while minimized).
    #[serde(default)]
    always_on_top: bool,
    // Global hotkey accelerator (e.g. "CommandOrControl+Shift+P"); empty = off.
    #[serde(default)]
    hotkey: String,
    #[serde(default = "default_on")]
    auto_update: bool,
    // Update versions the user chose to skip — never offered again.
    #[serde(default)]
    skipped_versions: Vec<String>,
    // Expert toggles: feature key -> enabled. A missing key means enabled, so
    // every feature is on by default and only explicit `false` disables it.
    #[serde(default)]
    ui_flags: HashMap<String, bool>,
    // Expert numeric tweaks: key -> value. A missing key uses the frontend default.
    #[serde(default)]
    ui_values: HashMap<String, f64>,
    // Expert string options (e.g. copy-feedback font). Missing key = default.
    #[serde(default)]
    ui_texts: HashMap<String, String>,
    // F6: prompt-id pairs (sorted) the user marked "not a duplicate" — never regrouped.
    #[serde(default)]
    dup_ignored: Vec<[String; 2]>,
    // Legacy in-blob copy history. Read once on upgrade to migrate into the
    // dedicated `copy_log` table, then never written back (the table owns it so
    // a million-entry history never bloats this encrypted settings blob).
    #[serde(default, skip_serializing)]
    copy_log: Vec<CopyEntry>,
    // Per-prompt copy count + last-used time (unix seconds). Both bounded by the
    // number of prompts, so they stay small in the blob.
    #[serde(default)]
    usage: HashMap<String, u32>,
    #[serde(default)]
    last_used: HashMap<String, u64>,
    #[serde(default = "default_on")]
    show_header: bool,
    #[serde(default = "default_on")]
    show_composer: bool,
    #[serde(default = "default_language")]
    language: String,
    #[serde(default = "default_tile_font")]
    tile_font: String,
    #[serde(default = "default_tile_size")]
    tile_size: u32,
    #[serde(default)]
    active_view: String,
    #[serde(default)]
    views: Vec<View>,
    // Legacy single-grid fields (pre-views). Read for migration, never written back.
    #[serde(default = "default_cols", rename = "cols", skip_serializing)]
    legacy_cols: u32,
    #[serde(default = "default_rows", rename = "rows", skip_serializing)]
    legacy_rows: u32,
    #[serde(default, rename = "layout", skip_serializing)]
    legacy_layout: HashMap<String, [u32; 2]>,
}

fn default_theme() -> String {
    "system".to_string()
}
fn default_cols() -> u32 {
    5
}
fn default_rows() -> u32 {
    4
}
fn default_on() -> bool {
    true
}
fn default_language() -> String {
    "auto".to_string()
}
fn default_tile_font() -> String {
    "system".to_string()
}
// 0 = auto-fit (default): each tile text grows to the largest size that fits.
fn default_tile_size() -> u32 {
    0
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: default_theme(),
            floating: HashMap::new(),
            float_scale: HashMap::new(),
            video_prefs: HashMap::new(),
            window: None,
            minimize_to_tray: false,
            autostart: false,
            start_minimized: false,
            always_on_top: false,
            hotkey: String::new(),
            auto_update: true,
            skipped_versions: Vec::new(),
            ui_flags: HashMap::new(),
            ui_values: HashMap::new(),
            ui_texts: HashMap::new(),
            dup_ignored: Vec::new(),
            copy_log: Vec::new(),
            usage: HashMap::new(),
            last_used: HashMap::new(),
            show_header: true,
            show_composer: true,
            language: default_language(),
            tile_font: default_tile_font(),
            tile_size: default_tile_size(),
            active_view: String::new(),
            views: Vec::new(),
            legacy_cols: default_cols(),
            legacy_rows: default_rows(),
            legacy_layout: HashMap::new(),
        }
    }
}

const GRID_MIN: u32 = 1;
// Hard safety ceilings. The default-facing limits are 20 (enforced in the UI via
// the expert values gridMax / maxViews); these only cap how far those expert
// overrides can be pushed, so old data and extreme settings can't break things.
const GRID_MAX: u32 = 200;
const MAX_VIEWS: usize = 200;
const FLOAT_W: f64 = 360.0;
const FLOAT_H: f64 = 80.0; // flat pill shape, clearly wider than tall
const FLOAT_IMG: f64 = 400.0; // square box for image pills: S 300 / M 400 / L 560
const AUTOSTART_KEY: &str = "PromptSaver";
// Local data folder under %APPDATA%\Roaming. Used by the pre-app panic hook
// (no AppHandle) and the store path; deliberately NOT the bundle identifier.
const DATA_FOLDER: &str = "Prompt-Saver";

fn grid_key(cols: u32, rows: u32) -> String {
    format!("{}x{}", cols, rows)
}

// WebView2-missing dialog texts; shown before settings exist, so the OS
// locale decides (same language set as the UI).
fn webview2_texts(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "de" => ("WebView2 Runtime fehlt", "Prompt Saver benötigt die Microsoft WebView2 Runtime.\n\nJetzt herunterladen und installieren? Danach Prompt Saver einfach erneut starten."),
        "es" => ("Falta WebView2 Runtime", "Prompt Saver necesita Microsoft WebView2 Runtime.\n\n¿Descargarlo e instalarlo ahora? Después, simplemente inicia Prompt Saver de nuevo."),
        "fr" => ("WebView2 Runtime manquant", "Prompt Saver nécessite Microsoft WebView2 Runtime.\n\nLe télécharger et l'installer maintenant ? Relancez ensuite simplement Prompt Saver."),
        "it" => ("WebView2 Runtime mancante", "Prompt Saver richiede Microsoft WebView2 Runtime.\n\nScaricarlo e installarlo ora? Dopo, riavvia semplicemente Prompt Saver."),
        "pt" => ("WebView2 Runtime ausente", "O Prompt Saver precisa do Microsoft WebView2 Runtime.\n\nBaixar e instalar agora? Depois, basta iniciar o Prompt Saver novamente."),
        "pl" => ("Brak środowiska WebView2", "Prompt Saver wymaga Microsoft WebView2 Runtime.\n\nPobrać i zainstalować teraz? Następnie po prostu uruchom Prompt Saver ponownie."),
        "ru" => ("Отсутствует WebView2 Runtime", "Prompt Saver требуется Microsoft WebView2 Runtime.\n\nСкачать и установить сейчас? После этого просто запустите Prompt Saver снова."),
        "zh" => ("缺少 WebView2 运行时", "Prompt Saver 需要 Microsoft WebView2 运行时。\n\n现在下载并安装吗？安装后重新启动 Prompt Saver 即可。"),
        "ja" => ("WebView2 ランタイムがありません", "Prompt Saver には Microsoft WebView2 ランタイムが必要です。\n\n今すぐダウンロードしてインストールしますか？その後、Prompt Saver を再起動してください。"),
        "nl" => ("WebView2-runtime ontbreekt", "Prompt Saver heeft de Microsoft WebView2-runtime nodig.\n\nNu downloaden en installeren? Start Prompt Saver daarna gewoon opnieuw."),
        "tr" => ("WebView2 çalışma zamanı eksik", "Prompt Saver, Microsoft WebView2 çalışma zamanına ihtiyaç duyar.\n\nŞimdi indirilip kurulsun mu? Ardından Prompt Saver'ı yeniden başlatmanız yeterli."),
        "ko" => ("WebView2 런타임 없음", "Prompt Saver에는 Microsoft WebView2 런타임이 필요합니다.\n\n지금 다운로드하여 설치할까요? 설치 후 Prompt Saver를 다시 시작하면 됩니다."),
        "hi" => ("WebView2 रनटाइम मौजूद नहीं है", "Prompt Saver को Microsoft WebView2 रनटाइम की आवश्यकता है।\n\nअभी डाउनलोड और इंस्टॉल करें? उसके बाद बस Prompt Saver फिर से शुरू करें।"),
        "id" => ("WebView2 Runtime tidak ditemukan", "Prompt Saver memerlukan Microsoft WebView2 Runtime.\n\nUnduh dan pasang sekarang? Setelah itu cukup jalankan Prompt Saver lagi."),
        "vi" => ("Thiếu WebView2 Runtime", "Prompt Saver cần Microsoft WebView2 Runtime.\n\nTải xuống và cài đặt ngay? Sau đó chỉ cần khởi động lại Prompt Saver."),
        "cs" => ("Chybí WebView2 Runtime", "Prompt Saver vyžaduje Microsoft WebView2 Runtime.\n\nStáhnout a nainstalovat nyní? Poté stačí Prompt Saver znovu spustit."),
        "uk" => ("Відсутній WebView2 Runtime", "Prompt Saver потребує Microsoft WebView2 Runtime.\n\nЗавантажити та встановити зараз? Після цього просто запустіть Prompt Saver знову."),
        "sv" => ("WebView2-runtime saknas", "Prompt Saver behöver Microsoft WebView2-runtime.\n\nLadda ner och installera nu? Starta sedan bara Prompt Saver igen."),
        "ro" => ("Lipsește WebView2 Runtime", "Prompt Saver are nevoie de Microsoft WebView2 Runtime.\n\nDescărcați și instalați acum? Apoi porniți pur și simplu Prompt Saver din nou."),
        _ => ("WebView2 runtime missing", "Prompt Saver needs the Microsoft WebView2 runtime.\n\nDownload and install it now? Simply start Prompt Saver again afterwards."),
    }
}

// Resolve the effective UI language code ("auto" -> OS locale), EN fallback.
// Supported UI languages besides English (keep in sync with ui/i18n.js).
const LANG_CODES: [&str; 19] = [
    "de", "es", "fr", "it", "pt", "pl", "ru", "zh", "ja", "nl", "tr", "ko", "hi", "id", "vi",
    "cs", "uk", "sv", "ro",
];

fn resolve_lang(pref: &str) -> &'static str {
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
fn tray_labels(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "de" => ("Öffnen", "Beenden"),
        "es" => ("Abrir", "Salir"),
        "fr" => ("Ouvrir", "Quitter"),
        "it" => ("Apri", "Esci"),
        "pt" => ("Abrir", "Sair"),
        "pl" => ("Otwórz", "Zakończ"),
        "ru" => ("Открыть", "Выход"),
        "zh" => ("打开", "退出"),
        "ja" => ("開く", "終了"),
        "nl" => ("Openen", "Afsluiten"),
        "tr" => ("Aç", "Çıkış"),
        "ko" => ("열기", "종료"),
        "hi" => ("खोलें", "बंद करें"),
        "id" => ("Buka", "Keluar"),
        "vi" => ("Mở", "Thoát"),
        "cs" => ("Otevřít", "Ukončit"),
        "uk" => ("Відкрити", "Вийти"),
        "sv" => ("Öppna", "Avsluta"),
        "ro" => ("Deschide", "Ieșire"),
        _ => ("Open", "Quit"),
    }
}

// Localized default name of the auto-created start page. As long as the user
// never renamed it, it follows the UI language (see set_language).
fn home_name(lang: &str) -> &'static str {
    match lang {
        "de" => "Startseite",
        "es" => "Inicio",
        "fr" => "Accueil",
        "pt" => "Início",
        "pl" => "Strona główna",
        "ru" => "Главная",
        "zh" => "主页",
        "ja" => "ホーム",
        "tr" => "Ana sayfa",
        "ko" => "홈",
        "hi" => "होम",
        "id" => "Beranda",
        "vi" => "Trang chủ",
        "cs" => "Domů",
        "uk" => "Головна",
        "sv" => "Hem",
        "ro" => "Acasă",
        _ => "Home", // en + it + nl
    }
}

// Every possible default name -> a view still carrying one was never renamed.
const HOME_NAMES: [&str; 18] = [
    "Home", "Startseite", "Inicio", "Accueil", "Início",
    "Strona główna", "Главная", "主页", "ホーム",
    "Ana sayfa", "홈", "होम", "Beranda", "Trang chủ",
    "Domů", "Головна", "Hem", "Acasă",
];

impl Settings {
    // Ensure at least one view exists; migrate legacy single-grid data.
    fn migrate(&mut self) {
        if self.views.is_empty() {
            let mut layouts = HashMap::new();
            if !self.legacy_layout.is_empty() {
                layouts.insert(
                    grid_key(self.legacy_cols, self.legacy_rows),
                    self.legacy_layout.clone(),
                );
            }
            self.views.push(View {
                id: gen_id(),
                name: home_name(resolve_lang(&self.language)).to_string(),
                cols: self.legacy_cols,
                rows: self.legacy_rows,
                layouts,
                color: String::new(),
            });
        }
        if !self.views.iter().any(|v| v.id == self.active_view) {
            if let Some(first) = self.views.first() {
                self.active_view = first.id.clone();
            }
        }
    }

    fn active_index(&self) -> usize {
        self.views
            .iter()
            .position(|v| v.id == self.active_view)
            .unwrap_or(0)
    }

    fn active_view_mut(&mut self) -> &mut View {
        let i = self.active_index();
        &mut self.views[i]
    }
}

// In-memory store, flushed to disk on mutations.
struct Store {
    prompts: Vec<Prompt>,
    settings: Settings,
    // Set when db_load found encrypted prompt rows it could NOT decrypt (e.g. DPAPI
    // failed after a profile move / password reset). While true, save_prompts refuses
    // to rewrite the prompts table so a full DELETE+INSERT can never destroy the rows
    // that are present-but-unreadable. The raw data.db is snapshotted aside on detect.
    prompts_locked: bool,
}

type Db = Mutex<Store>;

// ---------- Paths + persistence ----------

fn default_data_dir(app: &AppHandle) -> PathBuf {
    // Portable build keeps its data beside the exe so it travels with the folder;
    // installed builds use %APPDATA%\Prompt-Saver. If the exe folder is not
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
fn portable_data_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|d| d.to_path_buf())
}

// Files that make up a store, in copy order.
const STORE_FILES: [&str; 3] = ["data.db", "prompts.json", "settings.json"];

// The installed copy's store, if this is a portable run and one exists elsewhere.
// Nothing is copied here: the app asks first (see `takeover_offer`), in its own
// window, so the question inherits the theme and language instead of being a bare
// system box before anything is loaded.
fn appdata_store_dir(app: &AppHandle, portable: &std::path::Path) -> Option<PathBuf> {
    let src = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join(DATA_FOLDER))
        .or_else(|| app.path().app_data_dir().ok())?;
    if src.as_path() == portable {
        return None;
    }
    STORE_FILES.iter().any(|f| src.join(f).exists()).then_some(src)
}

// One-time move of the store from the old identifier-based folder
// (com.bgcoding.promptsaver) to Prompt-Saver, so the rename never loses data.
fn migrate_legacy_data(app: &AppHandle, new: &std::path::Path) {
    if new.join("data.db").exists() {
        return;
    }
    if let Ok(old) = app.path().app_data_dir() {
        if old.as_path() != new {
            for f in ["data.db", "prompts.json", "settings.json"] {
                if old.join(f).exists() {
                    let _ = fs::copy(old.join(f), new.join(f));
                }
            }
        }
    }
}

// Portable build = run without the installer's uninstall.exe beside the exe
// (standard installs write one; portable mode and dev runs do not). Portable
// always fully closes on window-close; installed honours the tray setting.
fn is_portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| !d.join("uninstall.exe").exists()))
        .unwrap_or(true)
}

// Effective store location. A "datapath" pointer file in the canonical dir can
// redirect the store to a user-chosen folder (expert menu); on anything missing
// or invalid it falls back to the canonical dir, so data is never lost.
fn data_dir(app: &AppHandle) -> PathBuf {
    let base = default_data_dir(app);
    if let Ok(s) = fs::read_to_string(base.join("datapath")) {
        let p = PathBuf::from(s.trim());
        if !s.trim().is_empty() && p.is_dir() {
            return p;
        }
    }
    base
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &PathBuf) -> T {
    match fs::read_to_string(path) {
        Ok(raw) if !raw.trim().is_empty() => serde_json::from_str(&raw).unwrap_or_default(),
        _ => T::default(),
    }
}

// Atomic write: temp file then rename, so a crash never truncates data. Returns
// whether the data actually reached disk, so callers can log a persist failure.
fn write_json<T: Serialize>(path: &PathBuf, data: &T) -> bool {
    if let Ok(json) = serde_json::to_string_pretty(data) {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, json).is_ok() {
            return fs::rename(&tmp, path).is_ok();
        }
    }
    false
}

// ---------- SQLite store ----------
// Prompts live one-per-row (ordered); settings are a single JSON row. The DB
// is the source of truth; the legacy JSON files are imported once on upgrade
// and otherwise only used as a fallback if the DB cannot be opened.

// At-rest encryption for the local store. DPAPI (Windows, per-user, no password): the
// same Windows account always decrypts; copied elsewhere it can't be read. Falls back to
// plaintext if DPAPI is unavailable so data is never lost; legacy plaintext rows are
// re-encrypted on the next save (see load_store).
const ENC_PREFIX: &str = "enc:v1:";

#[cfg(windows)]
mod store_crypt {
    use std::ffi::c_void;
    #[repr(C)]
    struct Blob {
        cb: u32,
        pb: *mut u8,
    }
    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(d: *const Blob, desc: *const u16, ent: *const Blob, res: *mut c_void, prompt: *mut c_void, flags: u32, out: *mut Blob) -> i32;
        fn CryptUnprotectData(d: *const Blob, desc: *mut *mut u16, ent: *const Blob, res: *mut c_void, prompt: *mut c_void, flags: u32, out: *mut Blob) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(h: *mut c_void) -> *mut c_void;
    }
    const UI_FORBIDDEN: u32 = 0x1;

    fn run(input: &[u8], protect: bool) -> Option<Vec<u8>> {
        unsafe {
            let in_blob = Blob { cb: input.len() as u32, pb: input.as_ptr() as *mut u8 };
            let mut out = Blob { cb: 0, pb: std::ptr::null_mut() };
            let ok = if protect {
                CryptProtectData(&in_blob, std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), UI_FORBIDDEN, &mut out)
            } else {
                CryptUnprotectData(&in_blob, std::ptr::null_mut(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), UI_FORBIDDEN, &mut out)
            };
            if ok == 0 || out.pb.is_null() {
                return None;
            }
            let v = std::slice::from_raw_parts(out.pb, out.cb as usize).to_vec();
            LocalFree(out.pb as *mut c_void);
            Some(v)
        }
    }
    pub fn protect(d: &[u8]) -> Option<Vec<u8>> { run(d, true) }
    pub fn unprotect(d: &[u8]) -> Option<Vec<u8>> { run(d, false) }
}

// Encrypt a JSON string for storage (prefix + base64 ciphertext); plaintext on failure.
fn enc_str(plain: &str) -> String {
    #[cfg(windows)]
    if let Some(ct) = store_crypt::protect(plain.as_bytes()) {
        return format!("{}{}", ENC_PREFIX, base64_encode(&ct));
    }
    plain.to_string()
}

// Decrypt a stored value. Legacy (un-prefixed) values pass through for migration; an
// undecryptable prefixed value (e.g. another Windows account) yields "" so the row is
// skipped rather than fed garbage to the JSON parser.
fn dec_str(stored: &str) -> String {
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

fn db_conn(app: &AppHandle) -> Option<Connection> {
    let conn = Connection::open(data_dir(app).join("data.db")).ok()?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA busy_timeout=3000;
         CREATE TABLE IF NOT EXISTS prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS idx_copy_log_ts ON copy_log(ts);
         CREATE TABLE IF NOT EXISTS prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS idx_pv_prompt ON prompt_versions(prompt_id);
         CREATE TABLE IF NOT EXISTS clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);",
    )
    .ok()?;
    Some(conn)
}

fn db_write_prompts(conn: &mut Connection, prompts: &[Prompt]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM prompts", [])?;
    {
        let mut stmt = tx.prepare("INSERT INTO prompts(id, ord, data) VALUES(?1, ?2, ?3)")?;
        for (i, p) in prompts.iter().enumerate() {
            let data = enc_str(&serde_json::to_string(p).unwrap_or_default());
            stmt.execute(params![p.id, i as i64, data])?;
        }
    }
    tx.commit()
}

fn db_write_settings(conn: &Connection, settings: &Settings) -> rusqlite::Result<()> {
    let json = enc_str(&serde_json::to_string(settings).unwrap_or_default());
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('settings', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        params![json],
    )?;
    Ok(())
}

// Returns (prompts, settings, any_plaintext, decrypt_failures). `decrypt_failures`
// counts rows that WERE encrypted (ENC_PREFIX) but could not be decrypted/parsed —
// the caller must then avoid any destructive rewrite of the prompts table.
fn db_load(conn: &Connection) -> (Vec<Prompt>, Option<Settings>, bool, usize) {
    let mut prompts = Vec::new();
    let mut plaintext = false; // any legacy unencrypted row → re-save once to encrypt
    let mut failures = 0usize;
    if let Ok(mut stmt) = conn.prepare("SELECT data FROM prompts ORDER BY ord") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for data in rows.flatten() {
                let encrypted = data.starts_with(ENC_PREFIX);
                if !encrypted {
                    plaintext = true;
                }
                match serde_json::from_str::<Prompt>(&dec_str(&data)) {
                    Ok(p) => prompts.push(p),
                    // An encrypted row that won't decrypt/parse is present-but-unreadable;
                    // a plaintext row that won't parse is genuinely corrupt (not a key issue).
                    Err(_) if encrypted => failures += 1,
                    Err(_) => {}
                }
            }
        }
    }
    let raw = conn
        .query_row("SELECT value FROM meta WHERE key='settings'", [], |r| {
            r.get::<_, String>(0)
        })
        .ok();
    if raw.as_deref().is_some_and(|v| !v.starts_with(ENC_PREFIX)) {
        plaintext = true;
    }
    let settings = raw.and_then(|s| serde_json::from_str::<Settings>(&dec_str(&s)).ok());
    (prompts, settings, plaintext, failures)
}

// Append one copy event, then trim to the cap and the retention window. The cap
// trim uses the rowid (monotonic insertion order): "keep rows whose rowid is
// within `cap` of the newest" deletes at most one row per call in steady state
// and touches nothing during growth — O(log n), so a million-entry history is
// still cheap on every copy.
// Trim the history to the cap (newest `cap` rows) and the retention window. Rows
// kept are exactly those with rowid in (MAX-cap, MAX], i.e. at most `cap`, and
// age-prune only ever deletes from the old (low-rowid) end, so the kept block
// stays a contiguous newest-N — the cap can never be exceeded.
fn db_trim_copy(conn: &Connection, cap: usize, cutoff: Option<u64>) -> rusqlite::Result<()> {
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

fn db_append_copy(conn: &Connection, entry: &CopyEntry, cap: usize, cutoff: Option<u64>) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO copy_log(ts, id) VALUES(?1, ?2)", params![entry.ts as i64, entry.id])?;
    db_trim_copy(conn, cap, cutoff)
}

// Newest copy events for the journal. grouped = one row per prompt (its most
// recent copy); otherwise every event. Capped so the UI never renders millions.
fn db_recent_copies(conn: &Connection, limit: usize, grouped: bool) -> Vec<CopyEntry> {
    let sql = if grouped {
        "SELECT id, MAX(ts) FROM copy_log GROUP BY id ORDER BY MAX(rowid) DESC LIMIT ?1"
    } else {
        "SELECT id, ts FROM copy_log ORDER BY rowid DESC LIMIT ?1"
    };
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(sql) {
        if let Ok(rows) = stmt.query_map(params![limit as i64], |r| {
            Ok(CopyEntry { id: r.get::<_, String>(0)?, ts: r.get::<_, i64>(1)? as u64 })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

// ---- F7 prompt version history ----
// Snapshots of a prompt's previous name/text, encrypted like the prompt rows. The
// table lives in data.db (not the settings blob) so history never bloats settings.
#[derive(Serialize)]
struct PromptVersion {
    id: i64,
    ts: u64,
    name: String,
    text: String,
}

fn history_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("promptHistory") != Some(&false) // on by default
}
fn history_cap(settings: &Settings) -> usize {
    settings.ui_values.get("historyPerPrompt").copied().unwrap_or(20.0).clamp(5.0, 100.0) as usize
}

fn db_add_version(conn: &Connection, prompt_id: &str, ts: u64, name: &str, text: &str, cap: usize) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES(?1, ?2, ?3, ?4)",
        params![prompt_id, ts as i64, enc_str(name), enc_str(text)],
    )?;
    // Keep only the newest `cap` snapshots for this prompt.
    conn.execute(
        "DELETE FROM prompt_versions WHERE prompt_id = ?1 AND id NOT IN
            (SELECT id FROM prompt_versions WHERE prompt_id = ?1 ORDER BY id DESC LIMIT ?2)",
        params![prompt_id, cap as i64],
    )?;
    Ok(())
}
fn db_list_versions(conn: &Connection, prompt_id: &str) -> Vec<PromptVersion> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, ts, name, text FROM prompt_versions WHERE prompt_id = ?1 ORDER BY id DESC") {
        if let Ok(rows) = stmt.query_map(params![prompt_id], |r| {
            Ok(PromptVersion {
                id: r.get::<_, i64>(0)?,
                ts: r.get::<_, i64>(1)? as u64,
                name: dec_str(&r.get::<_, String>(2)?),
                text: dec_str(&r.get::<_, String>(3)?),
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}
fn db_get_version(conn: &Connection, id: i64) -> Option<(String, String, String)> {
    conn.query_row(
        "SELECT prompt_id, name, text FROM prompt_versions WHERE id = ?1",
        params![id],
        |r| Ok((r.get::<_, String>(0)?, dec_str(&r.get::<_, String>(1)?), dec_str(&r.get::<_, String>(2)?))),
    )
    .ok()
}
fn db_delete_versions(conn: &Connection, prompt_id: &str) {
    let _ = conn.execute("DELETE FROM prompt_versions WHERE prompt_id = ?1", params![prompt_id]);
}

// ---- F21 clipboard watcher ----
#[derive(Serialize)]
struct ClipItem {
    id: i64,
    ts: u64,
    text: String,
}
fn text_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}
fn db_add_clip(conn: &Connection, ts: u64, text: &str, cap: usize) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO clip_inbox(ts, text) VALUES(?1, ?2)", params![ts as i64, enc_str(text)])?;
    conn.execute(
        "DELETE FROM clip_inbox WHERE id <= (SELECT MAX(id) FROM clip_inbox) - ?1",
        params![cap as i64],
    )?;
    Ok(())
}
fn db_list_clips(conn: &Connection) -> Vec<ClipItem> {
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

fn save_prompts(app: &AppHandle, store: &Store) {
    // Safety valve: if some rows failed to decrypt at load, the in-memory list is a
    // SUBSET of what's on disk. Rewriting the table (DELETE + re-INSERT) would delete
    // the unreadable rows permanently — so we refuse to persist prompt changes at all
    // until the situation is resolved. Prompt edits this session simply won't stick.
    if store.prompts_locked {
        return;
    }
    if let Some(mut conn) = db_conn(app) {
        if db_write_prompts(&mut conn, &store.prompts).is_ok() {
            return;
        }
    }
    // Last resort: JSON fallback. If even that fails (disk full / permission), the
    // change is lost — leave a diagnosable trace instead of failing silently.
    if !write_json(&data_dir(app).join("prompts.json"), &store.prompts) {
        append_crash_log("persist failed: prompts could not be written to the database or the JSON fallback");
    }
}

fn save_settings(app: &AppHandle, settings: &Settings) {
    if let Some(conn) = db_conn(app) {
        if db_write_settings(&conn, settings).is_ok() {
            return;
        }
    }
    if !write_json(&data_dir(app).join("settings.json"), settings) {
        append_crash_log("persist failed: settings could not be written to the database or the JSON fallback");
    }
}

// Pre-1.9 builds saved image prompts without copy_image but copied on click.
fn migrate_prompts(prompts: &mut [Prompt]) {
    for p in prompts {
        if !p.image.is_empty() && !p.copy_image && (p.text.is_empty() || p.text == p.name) {
            p.copy_image = true;
        }
    }
}

fn load_store(app: &AppHandle) -> Store {
    let dir = data_dir(app);
    if let Some(mut conn) = db_conn(app) {
        let (mut prompts, settings_opt, needs_mig, decrypt_fails) = db_load(&conn);
        // Some encrypted rows would not decrypt → preserve the raw DB and lock writes.
        if decrypt_fails > 0 {
            let db_path = dir.join("data.db");
            let bak = dir.join("data.db.locked.bak");
            if !bak.exists() {
                let _ = fs::copy(&db_path, &bak);
            }
            append_crash_log(&format!(
                "{} prompt row(s) failed to decrypt; prompt writes locked, raw DB copied to data.db.locked.bak",
                decrypt_fails
            ));
        }
        let mut settings = settings_opt.clone().unwrap_or_default();
        // Empty DB but legacy JSON present → import it once, then own the data.
        if prompts.is_empty() && settings_opt.is_none() {
            let j_settings: Settings = read_json(&dir.join("settings.json"));
            let j_prompts: Vec<Prompt> = read_json(&dir.join("prompts.json"));
            if !j_prompts.is_empty() || dir.join("settings.json").exists() {
                let prompts_ok = db_conn(app)
                    .map(|mut c2| db_write_prompts(&mut c2, &j_prompts).is_ok())
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
        settings.migrate();
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
                // No legacy history to move — mark done so we never re-check.
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO meta(key, value) VALUES('copy_log_migrated', '1')",
                    [],
                );
            }
            settings.copy_log.clear();
        }
        // Enforce the cap + retention window on the table at startup (e.g. after
        // the cap was lowered, or right after a large legacy import).
        let cap = settings
            .ui_values
            .get("historyMax")
            .copied()
            .unwrap_or(100.0)
            .clamp(0.0, COPY_HISTORY_MAX as f64) as usize;
        let cutoff = history_max_age(&settings).map(|age| now_secs().saturating_sub(age));
        let _ = db_trim_copy(&conn, cap, cutoff);
        migrate_prompts(&mut prompts);
        let store = Store { prompts, settings, prompts_locked: decrypt_fails > 0 };
        if needs_mig || migrated_log {
            // One-time: re-encrypt legacy plaintext rows / drop the migrated log from the blob.
            save_prompts(app, &store);
            save_settings(app, &store.settings);
        }
        return store;
    }
    // Fallback: DB unavailable → read the JSON files directly.
    let mut settings: Settings = read_json(&dir.join("settings.json"));
    settings.migrate();
    prune_history(&mut settings);
    let mut prompts: Vec<Prompt> = read_json(&dir.join("prompts.json"));
    migrate_prompts(&mut prompts);
    Store { prompts, settings, prompts_locked: false }
}

fn gen_id() -> String {
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

fn lock<'a>(state: &'a State<'a, Db>) -> std::sync::MutexGuard<'a, Store> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------- Theme ----------

// Native window background = theme background, so the area exposed while
// resizing never flashes white.
fn apply_window_bg(app: &AppHandle, theme: &str) {
    if let Some(win) = app.get_webview_window("main") {
        // Matches the --bg token of each theme in main.css.
        let color = match theme {
            "dark" => tauri::webview::Color(27, 27, 29, 255),
            "programmer" => tauri::webview::Color(13, 17, 23, 255),
            "ai" => tauri::webview::Color(12, 8, 23, 255),
            "gradient" => tauri::webview::Color(109, 40, 217, 255),
            "sunset" => tauri::webview::Color(255, 247, 237, 255),
            "ocean" => tauri::webview::Color(238, 249, 254, 255),
            "forest" => tauri::webview::Color(240, 253, 244, 255),
            "midnight" => tauri::webview::Color(15, 23, 42, 255),
            "cyberpunk" => tauri::webview::Color(10, 10, 18, 255),
            "retro" => tauri::webview::Color(26, 18, 8, 255),
            "mono" => tauri::webview::Color(250, 250, 250, 255),
            "lavender" => tauri::webview::Color(245, 243, 255, 255),
            "candy" => tauri::webview::Color(253, 242, 248, 255),
            "coffee" => tauri::webview::Color(247, 241, 232, 255),
            _ => tauri::webview::Color(247, 247, 248, 255),
        };
        let _ = win.set_background_color(Some(color));
    }
}

fn effective_theme(app: &AppHandle, pref: &str) -> String {
    match pref {
        "light" | "dark" | "programmer" | "ai" | "gradient" | "sunset" | "ocean" | "forest"
        | "midnight" | "cyberpunk" | "retro" | "mono" | "lavender" | "candy" | "coffee" => {
            pref.to_string()
        }
        _ => app
            .get_webview_window("main")
            .and_then(|w| w.theme().ok())
            .map(|t| match t {
                tauri::Theme::Dark => "dark",
                _ => "light",
            })
            .unwrap_or("light")
            .to_string(),
    }
}

// ---------- Floating windows ----------

fn flabel(id: &str) -> String {
    format!("float-{}", id)
}

// New pills spawn at the top-left of the primary monitor, cascaded slightly
// so several pills never fully overlap.
fn default_pos(app: &AppHandle, index: usize) -> Pos {
    let off = 28 * (index as i32 % 8);
    let base = app
        .get_webview_window("main")
        .and_then(|w| w.primary_monitor().ok().flatten())
        .map(|m| {
            let p = m.position();
            Pos { x: p.x + 24, y: p.y + 24 }
        })
        .unwrap_or(Pos { x: 24, y: 24 });
    Pos { x: base.x + off, y: base.y + off }
}

fn float_scale_of(settings: &Settings, id: &str) -> f64 {
    let s = settings.float_scale.get(id).copied().unwrap_or(1.0);
    if s.is_finite() { s.clamp(0.5, 2.0) } else { 1.0 }
}

// Pill window size: square box for image pills, classic pill for text.
fn pill_dims(is_image: bool, scale: f64) -> (f64, f64) {
    if is_image {
        (FLOAT_IMG * scale, FLOAT_IMG * scale)
    } else {
        (FLOAT_W * scale, FLOAT_H * scale)
    }
}

// Gif/video attachments render straight from their path (no stored preview).
// Keep in sync with GIF_EXT / VIDEO_EXT in ui/media.js.
fn media_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    [".gif", ".mp4", ".m4v", ".mov", ".webm", ".ogv", ".ogg", ".ogm"]
        .iter()
        .any(|e| lower.ends_with(e))
}

fn is_image_prompt(p: &Prompt) -> bool {
    p.show_image && (!p.image.is_empty() || media_path(&p.file_path) || media_path(&p.icon_path))
}

// All file dialogs are parented to the main window: the window is blocked
// until the dialog is closed, so a second dialog can never stack on top.
fn file_dialog(app: &AppHandle) -> rfd::FileDialog {
    let dlg = rfd::FileDialog::new();
    match app.get_webview_window("main") {
        Some(win) => dlg.set_parent(&win),
        None => dlg,
    }
}

fn open_floating(app: &AppHandle, prompt: &Prompt) {
    let label = flabel(&prompt.id);
    if app.get_webview_window(&label).is_some() {
        return;
    }

    // Never call Tauri window APIs while holding the Db lock (deadlock risk).
    let (saved, scale, count, excluded) = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        (
            store.settings.floating.get(&prompt.id).copied(),
            float_scale_of(&store.settings, &prompt.id),
            store.settings.floating.len(),
            capture_excluded(&store.settings),
        )
    };
    let pos = saved.unwrap_or_else(|| default_pos(app, count));
    if saved.is_none() {
        let state: State<Db> = app.state();
        let mut store = lock(&state);
        store.settings.floating.insert(prompt.id.clone(), pos);
        save_settings(app, &store.settings);
    }

    let (pw, ph) = pill_dims(is_image_prompt(prompt), scale);
    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("floating.html".into()))
        .title(&prompt.name)
        .inner_size(pw, ph)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .build();

    if let Ok(win) = win {
        // Transparent native backdrop: resizing never flashes a white/opaque
        // rectangle behind the rounded pill (mirrors apply_window_bg for main).
        let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
        set_capture_exclusion(&win, excluded);
        let _ = win.set_position(PhysicalPosition::new(pos.x, pos.y));
        let _ = win.show();

        let app2 = app.clone();
        let pid = prompt.id.clone();
        win.on_window_event(move |event| match event {
            WindowEvent::Moved(p) => {
                // Fires for every pixel during a drag — never block the move
                // loop on a busy store; the next event carries the position.
                if let Some(state) = app2.try_state::<Db>() {
                    if let Ok(mut store) = state.try_lock() {
                        if store.settings.floating.contains_key(&pid) {
                            store.settings.floating.insert(pid.clone(), Pos { x: p.x, y: p.y });
                        }
                    }
                }
            }
            WindowEvent::Destroyed => {
                if let Some(state) = app2.try_state::<Db>() {
                    let store = state.lock().unwrap_or_else(|e| e.into_inner());
                    save_settings(&app2, &store.settings);
                }
            }
            _ => {}
        });
    }
}

fn close_floating_window(app: &AppHandle, id: &str) {
    if let Some(win) = app.get_webview_window(&flabel(id)) {
        let _ = win.close();
    }
}

// ---------- Image helpers ----------

fn base64_encode(data: &[u8]) -> String {
    const B: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((*c.get(1).unwrap_or(&0) as u32) << 8)
            | (*c.get(2).unwrap_or(&0) as u32);
        out.push(B[((n >> 18) & 63) as usize] as char);
        out.push(B[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 { B[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if c.len() > 2 { B[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn base64_decode(s: &str) -> Vec<u8> {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=' && !b.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for c in bytes.chunks(4) {
        // Any byte outside the alphabet means the input is corrupt: fail cleanly
        // instead of dropping it and silently shifting every following group.
        let mut v = [0u32; 4];
        for (i, &b) in c.iter().enumerate() {
            match B64.iter().position(|&x| x == b) {
                Some(p) => v[i] = p as u32,
                None => return Vec::new(),
            }
        }
        if c.len() >= 2 { out.push(((v[0] << 2) | (v[1] >> 4)) as u8); }
        if c.len() >= 3 { out.push(((v[1] << 4) | (v[2] >> 2)) as u8); }
        if c.len() >= 4 { out.push(((v[2] << 6) |  v[3]      ) as u8); }
    }
    out
}

// Cap decode dimensions so a malformed or decompression-bomb image fails
// gracefully (None) instead of OOM-crashing the app. Generous enough for any
// real photo or screenshot; image's default 512 MiB alloc cap still applies.
const MAX_DECODE_DIM: u32 = 30_000;

fn decode_image<R: std::io::BufRead + std::io::Seek>(reader: image::ImageReader<R>) -> Option<image::DynamicImage> {
    let mut reader = reader.with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIM);
    limits.max_image_height = Some(MAX_DECODE_DIM);
    reader.limits(limits);
    reader.decode().ok()
}

fn decode_image_mem(bytes: &[u8]) -> Option<image::DynamicImage> {
    decode_image(image::ImageReader::new(std::io::Cursor::new(bytes)))
}

fn decode_image_file(path: &str) -> Option<image::DynamicImage> {
    decode_image(image::ImageReader::open(path).ok()?)
}

fn copy_image_to_clipboard(data_url: &str) -> bool {
    let bytes = data_url_bytes(data_url);
    if bytes.is_empty() { return false; }
    let img = match decode_image_mem(&bytes) {
        Some(img) => img.to_rgba8(),
        None => return false,
    };
    let (w, h) = img.dimensions();
    let img_data = arboard::ImageData {
        width: w as usize,
        height: h as usize,
        bytes: img.into_raw().into(),
    };
    arboard::Clipboard::new().and_then(|mut c| c.set_image(img_data)).is_ok()
}

fn scale_and_encode(img: image::DynamicImage) -> String {
    // High quality: generous max size + Lanczos filtering keeps tiles sharp.
    const MAX: u32 = 1024;
    let img = if img.width() > MAX || img.height() > MAX {
        img.resize(MAX, MAX, FilterType::Lanczos3)
    } else {
        img
    };
    let mut buf = std::io::Cursor::new(Vec::<u8>::new());
    if img.write_to(&mut buf, image::ImageFormat::Png).is_ok() {
        format!("data:image/png;base64,{}", base64_encode(buf.get_ref()))
    } else {
        String::new()
    }
}

// Async: dialogs and clipboard reads run off the main thread — the window
// stays responsive and the pickers open without a noticeable delay.
#[tauri::command]
async fn get_clipboard_image() -> Option<String> {
    let mut cb = arboard::Clipboard::new().ok()?;
    let data = cb.get_image().ok()?;
    let bytes: Vec<u8> = data.bytes.into_owned();
    let img = image::RgbaImage::from_raw(data.width as u32, data.height as u32, bytes)?;
    let result = scale_and_encode(image::DynamicImage::ImageRgba8(img));
    if result.is_empty() { None } else { Some(result) }
}

// ---------- File clipboard (CF_HDROP) ----------

// Put a file on the clipboard, exactly like Ctrl+C in Explorer.
#[cfg(windows)]
fn set_clipboard_file(path: &str) -> bool {
    if !std::path::Path::new(path).exists() {
        return false;
    }
    // DROPFILES header (20 bytes) + UTF-16 path + double NUL terminator.
    let wide: Vec<u16> = path.encode_utf16().chain([0, 0]).collect();
    let mut data = vec![0u8; 20 + wide.len() * 2];
    data[0] = 20; // pFiles: offset of the path list
    data[16] = 1; // fWide: UTF-16
    for (i, w) in wide.iter().enumerate() {
        let b = w.to_le_bytes();
        data[20 + i * 2] = b[0];
        data[21 + i * 2] = b[1];
    }
    let Ok(_clip) = clipboard_win::Clipboard::new_attempts(10) else {
        return false;
    };
    const CF_HDROP: u32 = 15;
    clipboard_win::raw::empty().is_ok() && clipboard_win::raw::set(CF_HDROP, &data).is_ok()
}

#[cfg(not(windows))]
fn set_clipboard_file(_path: &str) -> bool {
    false
}

// First file currently on the clipboard (copied in Explorer), if any.
#[tauri::command]
async fn get_clipboard_file_path() -> Option<String> {
    #[cfg(windows)]
    {
        clipboard_win::get_clipboard::<Vec<String>, _>(clipboard_win::formats::FileList)
            .ok()?
            .into_iter()
            .next()
    }
    #[cfg(not(windows))]
    None
}

#[tauri::command]
async fn pick_file_path(app: AppHandle) -> Option<String> {
    file_dialog(&app)
        .pick_file()
        .map(|p| p.to_string_lossy().to_string())
}

// Preview for an attached file that happens to be an image.
#[tauri::command]
async fn load_image_file(path: String) -> Option<String> {
    let img = decode_image_file(&path)?;
    let result = scale_and_encode(img);
    if result.is_empty() { None } else { Some(result) }
}

// pdfium is only needed for PDF previews, and it is a plain DLL — so it travels
// inside the exe and is unpacked on first use. That keeps the portable build a
// single self-contained file instead of "exe plus a DLL you must not lose".
static PDFIUM_DLL: &[u8] = include_bytes!("../pdfium.dll");

// Where the embedded copy is unpacked. Versioned, so a newer build never binds
// against a stale DLL left behind by an older one.
fn pdfium_cache_path() -> PathBuf {
    std::env::temp_dir()
        .join("prompt-saver")
        .join(format!("pdfium-{APP_VERSION}.dll"))
}

// A DLL next to the exe wins (lets a user drop in a newer pdfium); otherwise the
// embedded copy is written to the cache path once and reused.
fn pdfium_lib_path(app: &AppHandle) -> Option<PathBuf> {
    let name = "pdfium.dll";
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        candidates.push(res.join(name));
    }
    if let Some(found) = candidates.into_iter().find(|p| p.exists()) {
        return Some(found);
    }
    let cached = pdfium_cache_path();
    // Size check catches a truncated write from an interrupted first run.
    if fs::metadata(&cached).map(|m| m.len() as usize).ok() == Some(PDFIUM_DLL.len()) {
        return Some(cached);
    }
    fs::create_dir_all(cached.parent()?).ok()?;
    if let Err(e) = fs::write(&cached, PDFIUM_DLL) {
        eprintln!("pdfium unpack failed: {e}"); // preview is skipped, app keeps running
        return None;
    }
    Some(cached)
}

// Render the first page of a PDF to a preview image (data URL). Returns None if
// pdfium is unavailable or the file cannot be read — the caller then keeps the
// PDF as a plain file attachment without a preview.
#[tauri::command]
async fn pdf_preview(app: AppHandle, path: String) -> Option<String> {
    use pdfium_render::prelude::*;
    let lib = pdfium_lib_path(&app)?;
    let bindings = Pdfium::bind_to_library(lib).ok()?;
    let pdfium = Pdfium::new(bindings);
    let document = pdfium.load_pdf_from_file(&path, None).ok()?;
    let page = document.pages().get(0).ok()?;
    let config = PdfRenderConfig::new()
        .set_target_width(1000)
        .set_maximum_height(1400);
    let image = page.render_with_config(&config).ok()?.as_image();
    let result = scale_and_encode(image);
    if result.is_empty() { None } else { Some(result) }
}

// IDs of prompts whose attached file OR media icon is gone (polled by the UI).
#[tauri::command]
fn missing_files(state: State<Db>) -> Vec<String> {
    // Snapshot ids + paths under the lock, then stat OUTSIDE it: a slow or dead
    // drive must never block other DB operations behind this polled command.
    let entries: Vec<(String, String, String)> = {
        let g = lock(&state);
        g.prompts
            .iter()
            .map(|p| (p.id.clone(), p.file_path.clone(), p.icon_path.clone()))
            .collect()
    };
    let gone = |path: &str| !path.is_empty() && !std::path::Path::new(path).exists();
    entries
        .into_iter()
        .filter(|(_, fp, ip)| gone(fp) || gone(ip))
        .map(|(id, _, _)| id)
        .collect()
}

// ---------- Prompt commands ----------

// Async: clones one prompt that may carry a large base64 image — off the UI thread.
#[tauri::command]
async fn get_prompt(state: State<'_, Db>, id: String) -> Result<Option<Prompt>, String> {
    Ok(lock(&state).prompts.iter().find(|p| p.id == id).cloned())
}

// First free [col,row] in row-major order; None if the grid is full.
fn first_free_cell(view: &View) -> Option<[u32; 2]> {
    let key = grid_key(view.cols, view.rows);
    let empty = HashMap::new();
    let layout = view.layouts.get(&key).unwrap_or(&empty);
    let occupied: std::collections::HashSet<&[u32; 2]> = layout.values().collect();
    for row in 0..view.rows {
        for col in 0..view.cols {
            if !occupied.contains(&[col, row]) {
                return Some([col, row]);
            }
        }
    }
    None
}

// "Store files" opt-in flag (off by default).
fn store_files_enabled(store: &Store) -> bool {
    store.settings.ui_flags.get("storeFiles").copied().unwrap_or(false)
}

// Copy an attached file into <data>/files so the prompt keeps working even if the
// original is moved or deleted. Returns the stored path, or the original unchanged
// on any problem or if it is already inside the store (never loses the reference).
fn store_attached_file(app: &AppHandle, path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let src = std::path::Path::new(path);
    let files_dir = data_dir(app).join("files");
    if src.starts_with(&files_dir) || !src.is_file() {
        return path.to_string();
    }
    if fs::create_dir_all(&files_dir).is_err() {
        return path.to_string();
    }
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let dest = files_dir.join(format!("{}_{}", gen_id(), name));
    match fs::copy(src, &dest) {
        Ok(_) => dest.to_string_lossy().to_string(),
        Err(_) => path.to_string(),
    }
}

// Remove copies in <data>/files that no prompt references any more. Gated by the
// cleanupFiles flag (default on); only ever touches files inside our own store
// folder, so a user's originals elsewhere are never affected.
fn cleanup_orphan_files(app: &AppHandle, store: &Store) {
    if !store.settings.ui_flags.get("cleanupFiles").copied().unwrap_or(true) {
        return;
    }
    let entries = match fs::read_dir(data_dir(app).join("files")) {
        Ok(e) => e,
        Err(_) => return, // no store folder yet -> nothing to clean
    };
    // Compare case-insensitively with normalized separators (Windows) so a still
    // referenced file is never mistaken for an orphan and deleted.
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    let referenced: std::collections::HashSet<String> = store
        .prompts
        .iter()
        .flat_map(|p| [p.file_path.clone(), p.icon_path.clone()])
        .filter(|s| !s.is_empty())
        .map(|s| norm(&s))
        .collect();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && !referenced.contains(&norm(&path.to_string_lossy())) {
            let _ = fs::remove_file(&path);
        }
    }
}

// Async: persisting all prompts (base64 images) to SQLite must not block the UI
// thread — a sync command runs on it and froze the window while saving.
#[tauri::command]
async fn add_prompt(
    app: AppHandle,
    state: State<'_, Db>,
    name: String,
    text: String,
    color: String,
    image: Option<String>,
    show_image: Option<bool>,
    copy_image: Option<bool>,
    file_path: Option<String>,
    icon_path: Option<String>,
    caption: Option<String>,
    caption_size: Option<u32>,
    font: Option<String>,
    font_size: Option<u32>,
    favorite: Option<bool>,
) -> Result<Prompt, String> {
    // One lock for the whole op so the storeFiles flag can't change mid-insert.
    let prompt = {
        let mut store = lock(&state);
        let store_files = store_files_enabled(&store);
        let mut fp = file_path.unwrap_or_default();
        let mut ip = icon_path.unwrap_or_default();
        if store_files {
            fp = store_attached_file(&app, &fp);
            ip = store_attached_file(&app, &ip);
        }
        let prompt = Prompt {
            id: gen_id(),
            name,
            text,
            color,
            image: image.unwrap_or_default(),
            show_image: show_image.unwrap_or(false),
            copy_image: copy_image.unwrap_or(false),
            file_path: fp,
            icon_path: ip,
            caption: caption.unwrap_or_default(),
            caption_size: clamp_caption_size(caption_size.unwrap_or(0)),
            font: font.unwrap_or_default(),
            font_size: clamp_font_size(font_size.unwrap_or(0)),
            favorite: favorite.unwrap_or(false),
        };
        let view = store.settings.active_view_mut();
        if let Some(cell) = first_free_cell(view) {
            let key = grid_key(view.cols, view.rows);
            view.layouts.entry(key).or_default().insert(prompt.id.clone(), cell);
        }
        store.prompts.push(prompt.clone());
        save_prompts(&app, &store);
        save_settings(&app, &store.settings);
        prompt
    };
    Ok(prompt)
}

// Async: same reason as add_prompt — the SQLite write stays off the UI thread.
#[tauri::command]
async fn update_prompt(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    name: String,
    text: String,
    color: String,
    image: Option<String>,
    show_image: Option<bool>,
    copy_image: Option<bool>,
    file_path: Option<String>,
    icon_path: Option<String>,
    caption: Option<String>,
    caption_size: Option<u32>,
    font: Option<String>,
    font_size: Option<u32>,
    favorite: Option<bool>,
) -> Result<Option<Prompt>, String> {
    let updated = {
        let mut store = lock(&state);
        let store_files = store_files_enabled(&store);
        let file_path = file_path.map(|p| if store_files { store_attached_file(&app, &p) } else { p });
        let icon_path = icon_path.map(|p| if store_files { store_attached_file(&app, &p) } else { p });
        // F7: read history settings before the mutable prompt borrow (a MutexGuard
        // can't be split into disjoint field borrows).
        let hist_on = history_enabled(&store.settings);
        let hist_cap = history_cap(&store.settings);
        let found = store.prompts.iter_mut().find(|p| p.id == id);
        match found {
            Some(p) => {
                // F7: snapshot the previous name/text before overwriting, if changed.
                if hist_on && (p.name != name || p.text != text) {
                    let (old_name, old_text) = (p.name.clone(), p.text.clone());
                    if let Some(c) = db_conn(&app) {
                        let _ = db_add_version(&c, &id, now_secs(), &old_name, &old_text, hist_cap);
                    }
                }
                p.name = name;
                p.text = text;
                p.color = color;
                if let Some(img) = image { p.image = img; }
                if let Some(si) = show_image { p.show_image = si; }
                if let Some(ci) = copy_image { p.copy_image = ci; }
                if let Some(fp) = file_path { p.file_path = fp; }
                if let Some(ip) = icon_path { p.icon_path = ip; }
                if let Some(c) = caption { p.caption = c; }
                if let Some(cs) = caption_size { p.caption_size = clamp_caption_size(cs); }
                if let Some(f) = font { p.font = f; }
                if let Some(fs) = font_size {
                    p.font_size = clamp_font_size(fs);
                }
                if let Some(fav) = favorite { p.favorite = fav; }
                let clone = p.clone();
                save_prompts(&app, &store);
                cleanup_orphan_files(&app, &store); // a replaced attachment orphans its old copy
                let scale = float_scale_of(&store.settings, &id);
                Some((clone, scale))
            }
            None => None,
        }
    };
    let (updated, scale) = match updated {
        Some((p, s)) => (Some(p), s),
        None => (None, 1.0),
    };
    if let Some(p) = &updated {
        let _ = app.emit("prompt-updated", p.clone());
        // An open pill switches between text pill and image box live.
        if let Some(win) = app.get_webview_window(&flabel(&p.id)) {
            let (w, h) = pill_dims(is_image_prompt(p), scale);
            let _ = win.set_size(tauri::LogicalSize::new(w, h));
        }
    }
    Ok(updated)
}

// F7: list a prompt's version snapshots (newest first).
#[tauri::command]
async fn list_versions(app: AppHandle, prompt_id: String) -> Result<Vec<PromptVersion>, String> {
    Ok(db_conn(&app).map(|c| db_list_versions(&c, &prompt_id)).unwrap_or_default())
}

// F7: restore a snapshot. The current state is snapshotted first, so a restore is
// itself undoable and nothing is ever lost.
#[tauri::command]
async fn restore_version(app: AppHandle, state: State<'_, Db>, version_id: i64) -> Result<Option<Prompt>, String> {
    let restored = {
        let mut store = lock(&state);
        let record = match db_conn(&app).and_then(|c| db_get_version(&c, version_id)) {
            Some(r) => r,
            None => return Ok(None),
        };
        let (pid, name, text) = record;
        let cap = history_cap(&store.settings);
        if let Some(p) = store.prompts.iter_mut().find(|p| p.id == pid) {
            if let Some(c) = db_conn(&app) {
                let _ = db_add_version(&c, &pid, now_secs(), &p.name, &p.text, cap);
            }
            p.name = name;
            p.text = text;
            let clone = p.clone();
            save_prompts(&app, &store);
            Some(clone)
        } else {
            None
        }
    };
    if let Some(p) = &restored {
        let _ = app.emit("prompt-updated", p.clone());
    }
    Ok(restored)
}

// Star / unstar a prompt in the library.
#[tauri::command]
fn set_favorite(app: AppHandle, state: State<Db>, id: String, favorite: bool) {
    let mut store = lock(&state);
    if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
        p.favorite = favorite;
        save_prompts(&app, &store);
    }
}

// Async: keeps the prompt-table rewrite off the UI thread.
#[tauri::command]
async fn delete_prompt(app: AppHandle, state: State<'_, Db>, id: String) -> Result<bool, String> {
    close_floating_window(&app, &id);
    let changed = {
        let mut store = lock(&state);
        let before = store.prompts.len();
        store.prompts.retain(|p| p.id != id);
        for view in &mut store.settings.views {
            for layout in view.layouts.values_mut() {
                layout.remove(&id);
            }
        }
        store.settings.floating.remove(&id);
        store.settings.float_scale.remove(&id);
        store.settings.video_prefs.remove(&id);
        let changed = store.prompts.len() != before;
        if changed {
            save_prompts(&app, &store);
            save_settings(&app, &store.settings);
            cleanup_orphan_files(&app, &store); // drop the deleted prompt's stored copies
            if let Some(c) = db_conn(&app) { db_delete_versions(&c, &id); } // F7 cascade
        }
        changed
    };
    Ok(changed)
}

// F8 batch operations: apply one action to many prompts in a SINGLE store pass
// (no per-item save loop, so 100+ selected prompts never stall the UI). Returns the
// fresh state so the caller re-renders once. Actions: delete / favorite / color /
// addView / removeView.
#[tauri::command]
async fn batch_prompts(
    app: AppHandle,
    state: State<'_, Db>,
    ids: Vec<String>,
    action: String,
    color: Option<String>,
    favorite: Option<bool>,
    view_id: Option<String>,
) -> Result<Settings, String> {
    if action == "delete" {
        // Tauri window calls must stay OUTSIDE the store mutex (deadlock trap).
        for id in &ids {
            close_floating_window(&app, id);
        }
    }
    let idset: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let mut store = lock(&state);
    match action.as_str() {
        "delete" => {
            store.prompts.retain(|p| !idset.contains(p.id.as_str()));
            for view in &mut store.settings.views {
                for layout in view.layouts.values_mut() {
                    layout.retain(|k, _| !idset.contains(k.as_str()));
                }
            }
            for id in &ids {
                store.settings.floating.remove(id);
                store.settings.float_scale.remove(id);
                store.settings.video_prefs.remove(id);
            }
            save_prompts(&app, &store);
            save_settings(&app, &store.settings);
            cleanup_orphan_files(&app, &store);
            if let Some(c) = db_conn(&app) { for id in &ids { db_delete_versions(&c, id); } } // F7 cascade
        }
        "favorite" => {
            let fav = favorite.unwrap_or(true);
            for p in store.prompts.iter_mut() {
                if idset.contains(p.id.as_str()) {
                    p.favorite = fav;
                }
            }
            save_prompts(&app, &store);
        }
        "color" => {
            let c = color.unwrap_or_default();
            for p in store.prompts.iter_mut() {
                if idset.contains(p.id.as_str()) {
                    p.color = c.clone();
                }
            }
            save_prompts(&app, &store);
        }
        "addView" | "removeView" => {
            let vid = view_id.unwrap_or_default();
            if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == vid) {
                let (cols, rows) = (view.cols, view.rows);
                let key = format!("{}x{}", cols, rows);
                let layout = view.layouts.entry(key).or_default();
                if action == "removeView" {
                    for id in &ids {
                        layout.remove(id);
                    }
                } else {
                    let mut occupied: std::collections::HashSet<(u32, u32)> =
                        layout.values().map(|c| (c[0], c[1])).collect();
                    for id in &ids {
                        if layout.contains_key(id) {
                            continue;
                        }
                        'find: for row in 0..rows {
                            for col in 0..cols {
                                if !occupied.contains(&(col, row)) {
                                    layout.insert(id.clone(), [col, row]);
                                    occupied.insert((col, row));
                                    break 'find;
                                }
                            }
                        }
                    }
                }
                save_settings(&app, &store.settings);
            }
        }
        _ => return Err("bad-action".into()),
    }
    // Return only the (small) settings: the caller mutates its local prompt list
    // in place, so we never re-serialise every prompt's base64 image over IPC.
    Ok(store.settings.clone())
}

// Factory reset: wipe all prompts AND all settings (views, theme, window,
// behaviour, fonts) and remove the autostart registry entry.
// Async: window closes + a full prompts/settings rewrite stay off the UI thread.
#[tauri::command]
async fn delete_all_data(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    let labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|l| l.starts_with("float-"))
        .cloned()
        .collect();
    for label in labels {
        if let Some(win) = app.get_webview_window(&label) {
            let _ = win.close();
        }
    }
    {
        let mut store = lock(&state);
        store.prompts.clear();
        store.prompts_locked = false; // explicit wipe overrides the decrypt-failure lock
        save_prompts(&app, &store);
        // Data-only wipe: keep every setting, drop the derived data tables.
        if let Some(c) = db_conn(&app) {
            let _ = c.execute("DELETE FROM prompt_versions", []); // F7 history
            let _ = c.execute("DELETE FROM copy_log", []); // copy history
            let _ = c.execute("DELETE FROM clip_inbox", []); // F21 inbox
        }
    }
    Ok(())
}

// Reset every SETTING to its default (theme, views/layouts, toggles, expert values)
// while keeping the stored prompts intact. The front-end reloads so each default
// re-applies cleanly; cleared views make normalizeLayout re-place the kept prompts.
#[tauri::command]
async fn reset_settings(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    let labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|l| l.starts_with("float-"))
        .cloned()
        .collect();
    for label in labels {
        if let Some(win) = app.get_webview_window(&label) {
            let _ = win.close();
        }
    }
    let _ = apply_autostart(false, false);
    {
        let mut store = lock(&state);
        let prompts = std::mem::take(&mut store.prompts); // keep the prompts
        // The backup password is a key, not a preference: dropping it would make
        // every password-protected backup permanently unreadable.
        let backup_pw = store.settings.ui_texts.remove(BACKUP_PW_KEY);
        store.settings = Settings::default();
        store.settings.migrate();
        if let Some(pw) = backup_pw {
            store.settings.ui_texts.insert(BACKUP_PW_KEY.to_string(), pw);
        }
        store.prompts = prompts;
        save_settings(&app, &store.settings);
    }
    Ok(())
}

// Remembers that the take-over question was answered, so it is asked exactly once.
const TAKEOVER_ASKED: &str = "takeoverAsked";

// Portable run, own store still empty, an installed store present elsewhere →
// the UI offers to adopt it. Returns the folder to name in the dialog.
#[tauri::command]
fn takeover_offer(app: AppHandle, state: State<Db>) -> Option<String> {
    if !is_portable() {
        return None;
    }
    {
        let store = lock(&state);
        if store.settings.ui_flags.get(TAKEOVER_ASKED) == Some(&true)
            || !store.prompts.is_empty()
        {
            return None;
        }
    }
    let dir = data_dir(&app);
    appdata_store_dir(&app, &dir).map(|p| p.display().to_string())
}

// Answer to that question. `adopt` copies the installed store beside the exe (the
// original stays untouched) and reloads it; either way the question is settled.
#[tauri::command]
fn takeover_apply(app: AppHandle, state: State<'_, Db>, adopt: bool) -> Result<(), String> {
    let dir = data_dir(&app);
    if adopt {
        let src = appdata_store_dir(&app, &dir).ok_or("no-source")?;
        for f in STORE_FILES {
            if src.join(f).exists() {
                fs::copy(src.join(f), dir.join(f)).map_err(|e| e.to_string())?;
            }
        }
        // The freshly copied database supersedes whatever the empty one wrote, so
        // its write-ahead files must not survive into the new one.
        let _ = fs::remove_file(dir.join("data.db-wal"));
        let _ = fs::remove_file(dir.join("data.db-shm"));
        // One lock for both steps: taking it twice would deadlock.
        let fresh = load_store(&app);
        *lock(&state) = fresh;
    }
    let mut store = lock(&state);
    store.settings.ui_flags.insert(TAKEOVER_ASKED.to_string(), true);
    save_settings(&app, &store.settings);
    Ok(())
}

// UI language preference: "auto" or one of LANG_CODES.
#[tauri::command]
fn set_language(app: AppHandle, state: State<Db>, lang: String) {
    let resolved = {
        let mut store = lock(&state);
        store.settings.language = lang;
        let resolved = resolve_lang(&store.settings.language);
        // Start page still has a default name -> translate it along.
        let home = home_name(resolved);
        for view in &mut store.settings.views {
            if HOME_NAMES.contains(&view.name.as_str()) {
                view.name = home.to_string();
            }
        }
        save_settings(&app, &store.settings);
        resolved
    };
    // Live updates without a restart: tray menu + floating pills.
    let (open_label, quit_label) = tray_labels(resolved);
    if let Some(tray) = app.tray_by_id("tray") {
        if let (Ok(show), Ok(quit)) = (
            MenuItem::with_id(&app, "show", open_label, true, None::<&str>),
            MenuItem::with_id(&app, "quit", quit_label, true, None::<&str>),
        ) {
            if let Ok(menu) = Menu::with_items(&app, &[&show, &quit]) {
                let _ = tray.set_menu(Some(menu));
            }
        }
    }
    let _ = app.emit("language-changed", resolved);
}

// Font family + size for the saved prompt tiles only. size 0 = auto-fit.
#[tauri::command]
fn set_tile_style(app: AppHandle, state: State<Db>, font: String, size: u32) {
    let mut store = lock(&state);
    store.settings.tile_font = font;
    store.settings.tile_size = if size == 0 { 0 } else { size.clamp(10, 40) };
    save_settings(&app, &store.settings);
}

// ---------- Grid / layout commands (per active view) ----------

// Replace the placement map of the active view's current grid size.
// Async: fires on every drag/hide — keep the settings write off the UI thread.
#[tauri::command]
async fn set_layout(app: AppHandle, state: State<'_, Db>, layout: HashMap<String, [u32; 2]>) -> Result<(), String> {
    let mut store = lock(&state);
    let view = store.settings.active_view_mut();
    let key = grid_key(view.cols, view.rows);
    view.layouts.insert(key, layout);
    save_settings(&app, &store.settings);
    Ok(())
}

// Change the active view's grid dimensions. The arrangement saved for the new
// size (if any) is restored automatically because layouts are keyed per size.
#[tauri::command]
fn set_view_grid(app: AppHandle, state: State<Db>, id: String, cols: u32, rows: u32) -> Settings {
    let mut store = lock(&state);
    let Some(view) = store.settings.views.iter_mut().find(|v| v.id == id) else {
        return store.settings.clone();
    };
    let (old_cols, old_rows) = (view.cols, view.rows);
    let old_key = grid_key(view.cols, view.rows);
    view.cols = cols.clamp(GRID_MIN, GRID_MAX);
    view.rows = rows.clamp(GRID_MIN, GRID_MAX);
    let new_key = grid_key(view.cols, view.rows);
    if new_key != old_key {
        if view.cols >= old_cols && view.rows >= old_rows {
            // Growing: the grid expands around the current arrangement.
            // Saved layouts double as backups — prompts that an earlier
            // shrink pushed out return to their remembered spots (largest
            // arrangement first, only onto free cells).
            let merged = {
                let mut merged = view.layouts.get(&old_key).cloned().unwrap_or_default();
                let mut occupied: std::collections::HashSet<[u32; 2]> =
                    merged.values().copied().collect();
                let mut saved: Vec<_> = view.layouts.iter().collect();
                saved.sort_by_key(|(key, _)| {
                    std::cmp::Reverse(
                        key.split_once('x')
                            .and_then(|(c, r)| {
                                Some(c.parse::<u64>().ok()? * r.parse::<u64>().ok()?)
                            })
                            .unwrap_or(0),
                    )
                });
                for (_, layout) in saved {
                    for (id, cell) in layout {
                        if cell[0] < view.cols
                            && cell[1] < view.rows
                            && !merged.contains_key(id)
                            && !occupied.contains(cell)
                        {
                            merged.insert(id.clone(), *cell);
                            occupied.insert(*cell);
                        }
                    }
                }
                merged
            };
            view.layouts.insert(new_key, merged);
        } else if !view.layouts.contains_key(&new_key) {
            // Shrinking, first visit: seed with the fitting part of the
            // previous arrangement. Saved sizes keep their arrangement.
            if let Some(old) = view.layouts.get(&old_key) {
                let (c_max, r_max) = (view.cols, view.rows);
                let seeded: HashMap<String, [u32; 2]> = old
                    .iter()
                    .filter(|(_, cell)| cell[0] < c_max && cell[1] < r_max)
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                view.layouts.insert(new_key, seeded);
            }
        }
    }
    save_settings(&app, &store.settings);
    store.settings.clone()
}

// ---------- View commands ----------

#[tauri::command]
fn add_view(app: AppHandle, state: State<Db>, name: String, color: String) -> Result<Settings, String> {
    let mut store = lock(&state);
    if store.settings.views.len() >= MAX_VIEWS {
        return Err(format!("max {} views", MAX_VIEWS));
    }
    let trimmed = name.trim();
    let view = View {
        id: gen_id(),
        name: if trimmed.is_empty() {
            format!("View {}", store.settings.views.len() + 1)
        } else {
            trimmed.to_string()
        },
        cols: default_cols(),
        rows: default_rows(),
        layouts: HashMap::new(),
        color: color.trim().to_string(),
    };
    store.settings.active_view = view.id.clone();
    store.settings.views.push(view);
    save_settings(&app, &store.settings);
    Ok(store.settings.clone())
}

#[tauri::command]
fn rename_view(app: AppHandle, state: State<Db>, id: String, name: String) -> Settings {
    let mut store = lock(&state);
    if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == id) {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            view.name = trimmed.to_string();
        }
    }
    save_settings(&app, &store.settings);
    store.settings.clone()
}

// Set a view's tab color (hex); empty string clears it back to default.
#[tauri::command]
fn set_view_color(app: AppHandle, state: State<Db>, id: String, color: String) -> Settings {
    let mut store = lock(&state);
    if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == id) {
        view.color = color.trim().to_string();
    }
    save_settings(&app, &store.settings);
    store.settings.clone()
}

// Recolor saved data when the global palette changes: every prompt/view whose
// color equals an old palette hex is moved to the new one (pairs = [[old,new],...]).
#[tauri::command]
fn remap_colors(app: AppHandle, state: State<Db>, pairs: Vec<(String, String)>) {
    if pairs.is_empty() {
        return;
    }
    let map: HashMap<String, String> = pairs.into_iter().filter(|(o, n)| o != n).collect();
    if map.is_empty() {
        return;
    }
    let mut store = lock(&state);
    let mut prompts_changed = false;
    for p in &mut store.prompts {
        if let Some(nc) = map.get(&p.color) {
            p.color = nc.clone();
            prompts_changed = true;
        }
    }
    let mut views_changed = false;
    for v in &mut store.settings.views {
        if let Some(nc) = map.get(&v.color) {
            v.color = nc.clone();
            views_changed = true;
        }
    }
    if prompts_changed {
        save_prompts(&app, &store);
    }
    if views_changed {
        save_settings(&app, &store.settings);
    }
}

#[tauri::command]
fn delete_view(app: AppHandle, state: State<Db>, id: String) -> Result<Settings, String> {
    let mut store = lock(&state);
    if store.settings.views.len() <= 1 {
        return Err("cannot delete the last view".to_string());
    }
    store.settings.views.retain(|v| v.id != id);
    if store.settings.active_view == id {
        store.settings.active_view = store.settings.views[0].id.clone();
    }
    save_settings(&app, &store.settings);
    Ok(store.settings.clone())
}

#[tauri::command]
fn set_active_view(app: AppHandle, state: State<Db>, id: String) -> Settings {
    let mut store = lock(&state);
    if store.settings.views.iter().any(|v| v.id == id) {
        store.settings.active_view = id;
    }
    save_settings(&app, &store.settings);
    store.settings.clone()
}

// ---------- Settings commands ----------

#[tauri::command]
fn get_settings(state: State<Db>) -> Settings {
    lock(&state).settings.clone()
}

// Prompts + settings in one IPC roundtrip (renderGrid hot path).
#[derive(Serialize)]
struct AppState {
    prompts: Vec<Prompt>,
    settings: Settings,
}

// Async: cloning every prompt (incl. large base64 images) must not run on the
// UI thread — a big library would otherwise stall the window on each render.
#[tauri::command]
async fn get_state(state: State<'_, Db>) -> Result<AppState, String> {
    let store = lock(&state);
    Ok(AppState {
        prompts: store.prompts.clone(),
        settings: store.settings.clone(),
    })
}

// ---- F11 usage statistics ----
#[derive(Serialize)]
struct StatBar {
    id: String,
    name: String,
    count: u32,
}
#[derive(Serialize)]
struct ViewCount {
    name: String,
    count: usize,
}
#[derive(Serialize)]
struct UsageStats {
    total_prompts: usize,
    total_copies: u64,
    copies7: u64,
    copies30: u64,
    favorites: usize,
    unused: usize,
    used_count: usize,
    avg_copies: f64,
    total_chars: u64,
    colored: usize,
    recent_name: String,
    recent_ts: u64,
    longest_name: String,
    longest_chars: usize,
    types: Vec<ViewCount>,
    top: Vec<StatBar>,
    never_used: Vec<StatBar>,
    per_view: Vec<ViewCount>,
    history_on: bool,
}

// Coarse type of a prompt for the statistics breakdown.
fn prompt_stat_type(p: &Prompt) -> &'static str {
    if !p.file_path.is_empty() {
        let lc = p.file_path.to_lowercase();
        if [".mp4", ".webm", ".mkv", ".mov", ".avi", ".m4v"].iter().any(|e| lc.ends_with(e)) {
            "video"
        } else if lc.ends_with(".pdf") {
            "pdf"
        } else if [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".tiff", ".tga", ".ico"].iter().any(|e| lc.ends_with(e)) {
            "image"
        } else {
            "file"
        }
    } else if !p.image.is_empty() {
        "image"
    } else {
        "text"
    }
}

// Aggregate everything in Rust so a 1000-prompt library returns one compact struct.
#[tauri::command]
async fn usage_stats(app: AppHandle, state: State<'_, Db>, top_n: usize) -> Result<UsageStats, String> {
    let store = lock(&state);
    let usage = &store.settings.usage;
    let last_used = &store.settings.last_used;
    let name_of = |p: &Prompt| -> String {
        if !p.name.is_empty() {
            p.name.clone()
        } else if !p.text.is_empty() {
            p.text.chars().take(40).collect()
        } else {
            p.id.clone()
        }
    };
    let total_prompts = store.prompts.len();
    let total_copies: u64 = usage.values().map(|v| *v as u64).sum();
    let favorites = store.prompts.iter().filter(|p| p.favorite).count();
    let used_count = store.prompts.iter().filter(|p| usage.get(&p.id).copied().unwrap_or(0) > 0).count();
    let unused = total_prompts.saturating_sub(used_count);
    let avg_copies = if used_count > 0 { total_copies as f64 / used_count as f64 } else { 0.0 };
    let total_chars: u64 = store.prompts.iter().map(|p| p.text.chars().count() as u64).sum();
    let colored = store.prompts.iter().filter(|p| !p.color.is_empty()).count();
    let (mut recent_name, mut recent_ts) = (String::new(), 0u64);
    for p in &store.prompts {
        let lu = last_used.get(&p.id).copied().unwrap_or(0);
        if lu > recent_ts { recent_ts = lu; recent_name = name_of(p); }
    }
    let (mut longest_name, mut longest_chars) = (String::new(), 0usize);
    for p in &store.prompts {
        let c = p.text.chars().count();
        if c > longest_chars { longest_chars = c; longest_name = name_of(p); }
    }
    let mut type_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for p in &store.prompts {
        *type_counts.entry(prompt_stat_type(p)).or_insert(0) += 1;
    }
    let types: Vec<ViewCount> = ["text", "image", "video", "pdf", "file"]
        .iter()
        .filter_map(|k| {
            let c = type_counts.get(k).copied().unwrap_or(0);
            if c > 0 { Some(ViewCount { name: (*k).to_string(), count: c }) } else { None }
        })
        .collect();
    let mut used: Vec<&Prompt> = store
        .prompts
        .iter()
        .filter(|p| usage.get(&p.id).copied().unwrap_or(0) > 0)
        .collect();
    used.sort_by(|a, b| {
        let ua = usage.get(&a.id).copied().unwrap_or(0);
        let ub = usage.get(&b.id).copied().unwrap_or(0);
        ub.cmp(&ua).then_with(|| name_of(a).to_lowercase().cmp(&name_of(b).to_lowercase()))
    });
    let top: Vec<StatBar> = used
        .iter()
        .take(top_n)
        .map(|p| StatBar { id: p.id.clone(), name: name_of(p), count: usage.get(&p.id).copied().unwrap_or(0) })
        .collect();
    let now = now_secs();
    let cutoff30 = now.saturating_sub(30 * 86400);
    let mut never_used: Vec<StatBar> = store
        .prompts
        .iter()
        .filter(|p| {
            let u = usage.get(&p.id).copied().unwrap_or(0);
            let lu = last_used.get(&p.id).copied().unwrap_or(0);
            u == 0 || lu < cutoff30
        })
        .map(|p| StatBar { id: p.id.clone(), name: name_of(p), count: usage.get(&p.id).copied().unwrap_or(0) })
        .collect();
    never_used.truncate(top_n.max(10) * 2);
    let per_view: Vec<ViewCount> = store
        .settings
        .views
        .iter()
        .map(|v| {
            let key = format!("{}x{}", v.cols, v.rows);
            let count = v.layouts.get(&key).map(|l| l.len()).unwrap_or(0);
            ViewCount { name: v.name.clone(), count }
        })
        .collect();
    let (copies7, copies30) = db_conn(&app)
        .map(|c| {
            let c7 = c
                .query_row("SELECT COUNT(*) FROM copy_log WHERE ts >= ?1", params![now.saturating_sub(7 * 86400) as i64], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as u64;
            let c30 = c
                .query_row("SELECT COUNT(*) FROM copy_log WHERE ts >= ?1", params![cutoff30 as i64], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as u64;
            (c7, c30)
        })
        .unwrap_or((0, 0));
    let history_on = store.settings.ui_flags.get("copyHistory") != Some(&false);
    Ok(UsageStats {
        total_prompts,
        total_copies,
        copies7,
        copies30,
        favorites,
        unused,
        used_count,
        avg_copies,
        total_chars,
        colored,
        recent_name,
        recent_ts,
        longest_name,
        longest_chars,
        types,
        top,
        never_used,
        per_view,
        history_on,
    })
}

// ---- F6 duplicate finder ----
#[derive(Serialize)]
struct DupMember {
    id: String,
    name: String,
    text: String,
    usage: u32,
    views: Vec<String>,
}
#[derive(Serialize)]
struct DupGroup {
    score: f64,
    members: Vec<DupMember>,
}

fn normalize_dup(s: &str) -> String {
    s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}
fn dup_trigrams(s: &str) -> std::collections::HashSet<[char; 3]> {
    let chars: Vec<char> = s.chars().collect();
    let mut set = std::collections::HashSet::new();
    for w in chars.windows(3) {
        set.insert([w[0], w[1], w[2]]);
    }
    set
}
fn dup_dice(a: &std::collections::HashSet<[char; 3]>, b: &std::collections::HashSet<[char; 3]>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.iter().filter(|t| b.contains(*t)).count();
    2.0 * inter as f64 / (a.len() + b.len()) as f64
}
fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

// Find near-duplicate prompts by trigram/Dice similarity. Rayon-parallel over the
// upper-triangle of pairs; 1000 prompts finish well under a second.
#[tauri::command]
async fn find_duplicates(state: State<'_, Db>, threshold: f64) -> Result<Vec<DupGroup>, String> {
    use rayon::prelude::*;
    use std::collections::{HashMap as Map, HashSet as Set};
    let store = lock(&state);
    let thr = (threshold / 100.0).clamp(0.0, 1.0);
    let items: Vec<(&Prompt, String)> = store
        .prompts
        .iter()
        .map(|p| {
            let base = if !p.text.is_empty() { p.text.as_str() } else { p.name.as_str() };
            (p, normalize_dup(base))
        })
        .filter(|(_, n)| n.chars().count() >= 3)
        .collect();
    let n = items.len();
    if n < 2 {
        return Ok(Vec::new());
    }
    let grams: Vec<Set<[char; 3]>> = items.par_iter().map(|(_, s)| dup_trigrams(s)).collect();
    let ignored: Set<(String, String)> = store
        .settings
        .dup_ignored
        .iter()
        .map(|p| {
            let (a, b) = (p[0].clone(), p[1].clone());
            if a <= b { (a, b) } else { (b, a) }
        })
        .collect();
    let edges: Vec<(usize, usize, f64)> = (0..n)
        .into_par_iter()
        .flat_map(|i| {
            let gi = &grams[i];
            let mut local = Vec::new();
            for j in (i + 1)..n {
                let d = dup_dice(gi, &grams[j]);
                if d >= thr {
                    let (a, b) = (&items[i].0.id, &items[j].0.id);
                    let key = if a <= b { (a.clone(), b.clone()) } else { (b.clone(), a.clone()) };
                    if !ignored.contains(&key) {
                        local.push((i, j, d));
                    }
                }
            }
            local
        })
        .collect();
    // Union-find into connected components; track the strongest edge per group.
    let mut parent: Vec<usize> = (0..n).collect();
    let mut best: Map<usize, f64> = Map::new();
    for &(i, j, _) in &edges {
        let (ri, rj) = (uf_find(&mut parent, i), uf_find(&mut parent, j));
        if ri != rj {
            parent[ri] = rj;
        }
    }
    for &(i, _, d) in &edges {
        let r = uf_find(&mut parent, i);
        let e = best.entry(r).or_insert(0.0);
        if d > *e {
            *e = d;
        }
    }
    let mut groups: Map<usize, Vec<usize>> = Map::new();
    for i in 0..n {
        let r = uf_find(&mut parent, i);
        if best.contains_key(&r) {
            groups.entry(r).or_default().push(i);
        }
    }
    // Precompute view membership: prompt id -> view names containing it.
    let mut in_views: Map<&str, Vec<String>> = Map::new();
    for v in &store.settings.views {
        let mut seen: Set<&str> = Set::new();
        for layout in v.layouts.values() {
            for id in layout.keys() {
                if seen.insert(id.as_str()) {
                    in_views.entry(id.as_str()).or_default().push(v.name.clone());
                }
            }
        }
    }
    let mut out: Vec<DupGroup> = groups
        .into_iter()
        .filter(|(_, m)| m.len() >= 2)
        .map(|(root, idxs)| {
            let members = idxs
                .iter()
                .map(|&i| {
                    let p = items[i].0;
                    DupMember {
                        id: p.id.clone(),
                        name: if p.name.is_empty() { p.text.chars().take(40).collect() } else { p.name.clone() },
                        text: p.text.clone(),
                        usage: store.settings.usage.get(&p.id).copied().unwrap_or(0),
                        views: in_views.get(p.id.as_str()).cloned().unwrap_or_default(),
                    }
                })
                .collect();
            DupGroup { score: best.get(&root).copied().unwrap_or(0.0), members }
        })
        .collect();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

// F6: mark all pairwise combinations of these prompt ids as "not duplicates".
#[tauri::command]
async fn ignore_dups(app: AppHandle, state: State<'_, Db>, ids: Vec<String>) -> Result<(), String> {
    let mut store = lock(&state);
    let mut existing: std::collections::HashSet<(String, String)> = store
        .settings
        .dup_ignored
        .iter()
        .map(|p| (p[0].clone(), p[1].clone()))
        .collect();
    for a in 0..ids.len() {
        for b in (a + 1)..ids.len() {
            let (x, y) = if ids[a] <= ids[b] { (ids[a].clone(), ids[b].clone()) } else { (ids[b].clone(), ids[a].clone()) };
            if existing.insert((x.clone(), y.clone())) {
                store.settings.dup_ignored.push([x, y]);
            }
        }
    }
    save_settings(&app, &store.settings);
    Ok(())
}

// ---- F2 automatic backups & restore ----
#[derive(Serialize)]
struct BackupInfo {
    name: String,
    size: u64,
}
fn backups_dir(app: &AppHandle) -> std::path::PathBuf {
    data_dir(app).join("backups")
}
// A backup folder name is a local_stamp() (digits, '-', '_') — reject anything with
// path separators or traversal so a crafted name can never escape the backups dir.
fn valid_backup_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '_')
}
fn list_backups_impl(app: &AppHandle) -> Vec<BackupInfo> {
    let dir = backups_dir(app);
    let mut out: Vec<BackupInfo> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let size = backup_db_file(&e.path())
                .and_then(|p| fs::metadata(p).ok())
                .map(|m| m.len())
                .unwrap_or(0);
            BackupInfo { name, size }
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name)); // newest first (stamp sorts chronologically)
    out
}
// Grandfather-father-son retention: keep the newest N backups per time tier.
struct GfsConfig {
    daily: usize,
    weekly: usize,
    monthly: usize,
}
fn gfs_from_settings(s: &Settings) -> Option<GfsConfig> {
    if s.ui_flags.get("backupGfs") != Some(&true) {
        return None;
    }
    let g = |k: &str, d: f64| s.ui_values.get(k).copied().unwrap_or(d).max(0.0) as usize;
    Some(GfsConfig { daily: g("backupDaily", 3.0), weekly: g("backupWeekly", 4.0), monthly: g("backupMonthly", 12.0) })
}
// Days since 1970-01-01 for a proleptic Gregorian date (Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
// (day, week, month) tier keys from a backup folder name "YYYY-MM-DD_HH-MM-SS".
fn backup_date_keys(name: &str) -> Option<(i64, i64, i64)> {
    let date = name.split('_').next()?;
    let mut it = date.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    let days = days_from_civil(y, m, d);
    Some((days, days.div_euclid(7), y * 12 + m))
}
fn rotate_backups(app: &AppHandle, keep: usize, gfs: Option<GfsConfig>) {
    use std::collections::HashSet;
    let dir = backups_dir(app);
    let mut names: Vec<String> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort(); // oldest first (names are timestamps → chronological)
    if let Some(g) = gfs {
        // Newest-first walk; for each tier keep the newest backup per distinct
        // day / week / month up to that tier's count.
        let mut desc = names.clone();
        desc.reverse();
        let pick = |count: usize, idx: usize| -> Vec<String> {
            let mut seen: HashSet<i64> = HashSet::new();
            let mut out = Vec::new();
            if count == 0 {
                return out;
            }
            for n in &desc {
                if let Some(keys) = backup_date_keys(n) {
                    let k = [keys.0, keys.1, keys.2][idx];
                    if seen.insert(k) {
                        out.push(n.clone());
                        if seen.len() >= count {
                            break;
                        }
                    }
                }
            }
            out
        };
        let mut keep_set: HashSet<String> = HashSet::new();
        for n in pick(g.daily, 0) { keep_set.insert(n); }
        for n in pick(g.weekly, 1) { keep_set.insert(n); }
        for n in pick(g.monthly, 2) { keep_set.insert(n); }
        for n in &names {
            if !keep_set.contains(n) {
                let _ = fs::remove_dir_all(dir.join(n));
            }
        }
    } else if names.len() > keep {
        for name in &names[..names.len() - keep] {
            let _ = fs::remove_dir_all(dir.join(name));
        }
    }
}
// Consistent snapshot of data.db via VACUUM INTO (WAL-safe, no locking games). Writes
// a temp file first and renames on success, so a failure leaves no partial snapshot.
// A snapshot is stored gzipped: a vacuumed store is mostly JSON text, so this
// takes a few MB down to a fraction of that — and ten of them are kept by default.
// `data.db` (uncompressed) is still accepted so older backups keep working.
const BACKUP_DB_GZ: &str = "data.db.gz";

fn backup_db_file(folder: &std::path::Path) -> Option<PathBuf> {
    let gz = folder.join(BACKUP_DB_GZ);
    if gz.exists() {
        return Some(gz);
    }
    let plain = folder.join("data.db");
    plain.exists().then_some(plain)
}

fn gzip_to(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    use flate2::{write::GzEncoder, Compression};
    let raw = fs::read(src).map_err(|e| e.to_string())?;
    let mut enc = GzEncoder::new(Vec::new(), Compression::best());
    std::io::Write::write_all(&mut enc, &raw).map_err(|e| e.to_string())?;
    let packed = enc.finish().map_err(|e| e.to_string())?;
    fs::write(dest, packed).map_err(|e| e.to_string())
}

fn gunzip_to(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    let packed = fs::read(src).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut GzDecoder::new(&packed[..]), &mut out)
        .map_err(|e| e.to_string())?;
    fs::write(dest, out).map_err(|e| e.to_string())
}

fn do_backup(app: &AppHandle, keep: usize, gfs: Option<GfsConfig>) -> Result<String, String> {
    let src = data_dir(app).join("data.db");
    if !src.exists() {
        return Err("no-store".into());
    }
    let name = local_stamp();
    let folder = backups_dir(app).join(&name);
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let tmp = folder.join("data.db.tmp");
    let dest = folder.join(BACKUP_DB_GZ);
    let _ = fs::remove_file(&tmp);
    {
        let conn = Connection::open(&src).map_err(|e| e.to_string())?;
        conn.execute("VACUUM INTO ?1", params![tmp.to_string_lossy()]).map_err(|e| e.to_string())?;
    }
    // Compress the vacuumed copy, then drop the raw one — never leave both behind.
    let packed = gzip_to(&tmp, &dest);
    let _ = fs::remove_file(&tmp);
    packed?;
    rotate_backups(app, keep.clamp(1, 50), gfs);
    Ok(name)
}
fn newest_backup_age(app: &AppHandle) -> Option<u64> {
    let dir = backups_dir(app);
    let mut newest: Option<std::time::SystemTime> = None;
    for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
        if let Ok(m) = e.metadata() {
            if let Ok(t) = m.modified() {
                if newest.map_or(true, |n| t > n) {
                    newest = Some(t);
                }
            }
        }
    }
    newest.and_then(|t| t.elapsed().ok()).map(|d| d.as_secs())
}

#[tauri::command]
async fn list_backups(app: AppHandle) -> Result<Vec<BackupInfo>, String> {
    Ok(list_backups_impl(&app))
}

#[tauri::command]
async fn create_backup(app: AppHandle, state: State<'_, Db>, keep: usize) -> Result<String, String> {
    let store = lock(&state); // no writes race with the VACUUM read
    let gfs = gfs_from_settings(&store.settings);
    do_backup(&app, keep, gfs)
}

#[tauri::command]
async fn delete_backup(app: AppHandle, name: String) -> Result<(), String> {
    if !valid_backup_name(&name) {
        return Err("bad-name".into());
    }
    fs::remove_dir_all(backups_dir(&app).join(&name)).map_err(|e| e.to_string())
}

// Restore: snapshot the current store first (safety), swap data.db, drop the stale
// WAL/SHM, then relaunch so nothing can half-apply.
#[tauri::command]
async fn restore_backup(app: AppHandle, state: State<'_, Db>, name: String, keep: usize) -> Result<(), String> {
    if !valid_backup_name(&name) {
        return Err("bad-name".into());
    }
    let Some(backup_db) = backup_db_file(&backups_dir(&app).join(&name)) else {
        return Err("missing".into());
    };
    {
        let store = lock(&state);
        let gfs = gfs_from_settings(&store.settings);
        let _ = do_backup(&app, keep, gfs); // pre-restore safety snapshot
        let dir = data_dir(&app);
        let target = dir.join("data.db");
        if backup_db.extension().is_some_and(|e| e == "gz") {
            gunzip_to(&backup_db, &target)?;
        } else {
            fs::copy(&backup_db, &target).map_err(|e| e.to_string())?; // pre-2.6 snapshot
        }
        let _ = fs::remove_file(dir.join("data.db-wal"));
        let _ = fs::remove_file(dir.join("data.db-shm"));
    }
    app.restart();
}

#[tauri::command]
fn current_theme(app: AppHandle, state: State<Db>) -> String {
    let pref = lock(&state).settings.theme.clone();
    effective_theme(&app, &pref)
}

#[tauri::command]
fn set_theme(app: AppHandle, state: State<Db>, theme: String) -> String {
    {
        let mut store = lock(&state);
        store.settings.theme = theme.clone();
        save_settings(&app, &store.settings);
    }
    let effective = effective_theme(&app, &theme);
    apply_window_bg(&app, &effective);
    let _ = app.emit("theme-changed", effective.clone());
    effective
}

// Async: encoding + writing a large image to the clipboard stays off the UI thread.
#[tauri::command]
async fn copy_prompt(state: State<'_, Db>, id: String) -> Result<bool, String> {
    let prompt = {
        let store = lock(&state);
        store.prompts.iter().find(|p| p.id == id).cloned()
    };
    Ok(match prompt {
        Some(p) if p.copy_image && !p.image.is_empty() => copy_image_to_clipboard(&p.image),
        Some(p) if !p.file_path.is_empty() => set_clipboard_file(&p.file_path),
        Some(p) => {
            note_own_copy(&p.text); // F21: don't echo our own copy into the inbox
            arboard::Clipboard::new().and_then(|mut c| c.set_text(p.text)).is_ok()
        }
        None => false,
    })
}

// Put arbitrary text on the clipboard — used after filling prompt placeholders.
#[tauri::command]
async fn copy_text(text: String) -> bool {
    note_own_copy(&text); // F21
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .is_ok()
}

// Safety ceiling for the copy-history length; the live limit is the expert value
// historyMax (default 100, up to this ceiling). The history lives in its own
// table with O(log n) append/trim, so even the ceiling stays fast on every copy.
const COPY_HISTORY_MAX: usize = 1_000_000;

// One copy-history entry: which prompt + when (unix seconds).
#[derive(Serialize, Deserialize, Clone)]
struct CopyEntry {
    id: String,
    ts: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Retention window in seconds from the expert "history retention" setting
// (days). Default 0 = keep forever: the copy history is bounded by its entry
// count (historyMax), so a time-based prune would only surprise users who copy
// infrequently by silently emptying "recently copied". 0 or negative = forever.
fn history_max_age(settings: &Settings) -> Option<u64> {
    let days = settings.ui_values.get("historyDays").copied().unwrap_or(0.0);
    if days <= 0.0 {
        None
    } else {
        Some(days as u64 * 86_400)
    }
}

// Drop copy-history entries older than the retention window (auto-delete).
fn prune_history(settings: &mut Settings) {
    if let Some(max_age) = history_max_age(settings) {
        let now = now_secs();
        settings.copy_log.retain(|e| now.saturating_sub(e.ts) <= max_age);
    }
}

// Record a copy in the history + usage stats (called by the UI after any copy).
#[tauri::command]
async fn record_copy(app: AppHandle, state: State<'_, Db>, id: String) -> Result<(), String> {
    let (entry, cap, cutoff, dedup) = {
        let mut store = lock(&state);
        // Respect the privacy toggle (expert menu): off = don't track.
        if store.settings.ui_flags.get("copyHistory") == Some(&false) {
            return Ok(());
        }
        // Expert opt: skip logging a copy identical to the previous one.
        let dedup = store.settings.ui_flags.get("dedupCopyLog") == Some(&true);
        if !store.prompts.iter().any(|p| p.id == id) {
            return Ok(());
        }
        *store.settings.usage.entry(id.clone()).or_insert(0) += 1;
        // Timestamp storage is a privacy toggle (default on).
        let ts = if store.settings.ui_flags.get("historyTimestamps") == Some(&false) {
            0
        } else {
            now_secs()
        };
        if ts > 0 {
            store.settings.last_used.insert(id.clone(), ts);
        }
        // History length is an expert value (default 100, ceiling COPY_HISTORY_MAX).
        let cap = store
            .settings
            .ui_values
            .get("historyMax")
            .copied()
            .unwrap_or(100.0)
            .clamp(0.0, COPY_HISTORY_MAX as f64) as usize;
        let cutoff = history_max_age(&store.settings).map(|age| now_secs().saturating_sub(age));
        save_settings(&app, &store.settings);
        (CopyEntry { id, ts }, cap, cutoff, dedup)
    };
    // The history itself lives in its own table — appended outside the lock so a
    // huge log never blocks other DB work and never bloats the settings blob.
    if let Some(conn) = db_conn(&app) {
        if dedup {
            let last: Option<String> = conn
                .query_row("SELECT id FROM copy_log ORDER BY rowid DESC LIMIT 1", [], |r| r.get(0))
                .ok();
            if last.as_deref() == Some(entry.id.as_str()) {
                return Ok(()); // identical consecutive copy — don't add another row
            }
        }
        let _ = db_append_copy(&conn, &entry, cap, cutoff);
    }
    Ok(())
}

// Wipe copy history + usage counters (privacy / journal "clear").
#[tauri::command]
async fn clear_copy_history(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    {
        let mut store = lock(&state);
        store.settings.usage.clear();
        store.settings.last_used.clear();
        save_settings(&app, &store.settings);
    }
    if let Some(conn) = db_conn(&app) {
        let _ = conn.execute("DELETE FROM copy_log", []);
    }
    Ok(())
}

// Newest copy-history entries for the journal (own table; capped for the UI).
// Async: the grouped query scans the table, so keep it off the UI thread.
#[tauri::command]
async fn recent_copies(app: AppHandle, limit: usize, grouped: bool) -> Vec<CopyEntry> {
    db_conn(&app)
        .map(|c| db_recent_copies(&c, limit.min(COPY_HISTORY_MAX), grouped))
        .unwrap_or_default()
}

// Must stay async: window creation from a sync command deadlocks on Windows
// (sync commands run on the main thread, which WebView2 needs free).
#[tauri::command]
async fn toggle_floating(app: AppHandle, state: State<'_, Db>, id: String) -> Result<bool, String> {
    if app.get_webview_window(&flabel(&id)).is_some() {
        {
            let mut store = lock(&state);
            store.settings.floating.remove(&id);
            store.settings.float_scale.remove(&id);
            save_settings(&app, &store.settings);
        }
        close_floating_window(&app, &id);
        Ok(false)
    } else {
        let prompt = {
            let store = lock(&state);
            store.prompts.iter().find(|p| p.id == id).cloned()
        };
        match prompt {
            Some(p) => {
                open_floating(&app, &p);
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

// Size factor of one pill; persists the choice. resize=false leaves the
// window alone (the menu flow sizes it itself, avoiding a visible jump).
#[tauri::command]
async fn set_float_scale(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    scale: f64,
    resize: Option<bool>,
) -> Result<(), String> {
    let scale = if scale.is_finite() { scale.clamp(0.3, 8.0) } else { 1.0 };
    let is_img = {
        let mut store = lock(&state);
        store.settings.float_scale.insert(id.clone(), scale);
        save_settings(&app, &store.settings);
        store.prompts.iter().find(|p| p.id == id).map(is_image_prompt).unwrap_or(false)
    };
    if resize.unwrap_or(true) {
        if let Some(win) = app.get_webview_window(&flabel(&id)) {
            let (w, h) = pill_dims(is_img, scale);
            let _ = win.set_size(tauri::LogicalSize::new(w, h));
        }
    }
    Ok(())
}

// Text pills grow with their label: the frontend measures the text and requests
// the matching window box (width and height, kept pill-shaped so it never rounds
// into a circle).
#[tauri::command]
async fn resize_float_pill(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let scale = float_scale_of(&lock(&state).settings, &id);
    if let Some(win) = app.get_webview_window(&flabel(&id)) {
        let w = if width.is_finite() { width.clamp(80.0, 8000.0) } else { FLOAT_W * scale };
        let h = if height.is_finite() { height.clamp(40.0, 8000.0) } else { FLOAT_H * scale };
        let _ = win.set_size(tauri::LogicalSize::new(w, h));
    }
    Ok(())
}

// Grow the pill window while its context menu is open; shrink back on close.
// Media pills: the window matches the media's aspect ratio (no invisible
// click area beyond the visible video/image).
#[tauri::command]
async fn resize_float_media(app: AppHandle, id: String, width: f64, height: f64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&flabel(&id)) {
        if width.is_finite() && height.is_finite() {
            let w = width.clamp(48.0, 4000.0);
            let h = height.clamp(48.0, 4000.0);
            let _ = win.set_size(tauri::LogicalSize::new(w, h));
        }
    }
    Ok(())
}

// Move AND resize a floating window in ONE OS call so the resize stays smooth.
// Tauri's separate set_position + set_size leave a one-frame intermediate state
// (new position, old size) that flickers the edges while dragging a grip.
#[cfg(windows)]
fn set_float_bounds_native(win: &tauri::WebviewWindow, x: f64, y: f64, w: f64, h: f64) -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    }
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    let hwnd = match win.hwnd() {
        Ok(h) => h.0 as isize,
        Err(_) => return false,
    };
    let s = win.scale_factor().unwrap_or(1.0);
    let (px, py, cx, cy) = (
        (x * s).round() as i32,
        (y * s).round() as i32,
        (w * s).round() as i32,
        (h * s).round() as i32,
    );
    unsafe { SetWindowPos(hwnd, 0, px, py, cx, cy, SWP_NOZORDER | SWP_NOACTIVATE) != 0 }
}

// Expert privacy flag (Datenschutz tab). Missing = enabled, like every expert
// flag, so capture exclusion is ON by default.
const CAPTURE_FLAG_KEY: &str = "captureExclusion";

fn capture_excluded(settings: &Settings) -> bool {
    settings.ui_flags.get(CAPTURE_FLAG_KEY) != Some(&false)
}

// Keep our own windows out of OTHER capture tools (Snipping Tool, Print Screen,
// Game Bar, OBS, and Prompt Saver's own window picker): when excluded the window
// stays fully visible to the user but is omitted from any screen capture. Our own
// snip already hides these windows before grabbing, so this never affects it.
// WDA_EXCLUDEFROMCAPTURE needs Windows 10 2004+; on older builds the call simply
// fails and the window stays capturable. No WDA_MONITOR fallback: a black
// rectangle in screenshots is worse than the window simply showing, and 2004+
// covers every supported system.
#[cfg(windows)]
fn set_capture_exclusion(win: &tauri::WebviewWindow, excluded: bool) {
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowDisplayAffinity(hwnd: isize, affinity: u32) -> i32;
    }
    const WDA_NONE: u32 = 0x0;
    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;
    if let Ok(h) = win.hwnd() {
        let aff = if excluded { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
        unsafe {
            SetWindowDisplayAffinity(h.0 as isize, aff);
        }
    }
}

#[cfg(not(windows))]
fn set_capture_exclusion(_win: &tauri::WebviewWindow, _excluded: bool) {}

// ---- F19 auto-paste: track the last EXTERNAL foreground window so a copied prompt
// can be pasted straight into it. A WinEvent hook records every foreground change;
// WINEVENT_SKIPOWNPROCESS drops our own windows, so the stored handle is always the
// app the user was working in (covers grid tiles, library rows AND the no-activate
// floating pills, whose click never steals foreground).
#[cfg(windows)]
mod autopaste {
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::time::Duration;

    static LAST_FG: AtomicIsize = AtomicIsize::new(0);

    #[link(name = "user32")]
    extern "system" {
        fn SetWinEventHook(min: u32, max: u32, hmod: isize, cb: WinEventProc, pid: u32, tid: u32, flags: u32) -> isize;
        fn GetForegroundWindow() -> isize;
        fn SetForegroundWindow(hwnd: isize) -> i32;
        fn IsWindow(hwnd: isize) -> i32;
        fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
    }
    type WinEventProc = unsafe extern "system" fn(isize, u32, isize, i32, i32, u32, u32);

    const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
    const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
    const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const VK_CONTROL: u8 = 0x11;
    const VK_RETURN: u8 = 0x0D;
    const VK_V: u8 = 0x56;
    const OBJID_WINDOW: i32 = 0;

    unsafe extern "system" fn on_foreground(_h: isize, _e: u32, hwnd: isize, id_object: i32, _c: i32, _t: u32, _ms: u32) {
        if id_object == OBJID_WINDOW && hwnd != 0 {
            LAST_FG.store(hwnd, Ordering::Relaxed);
        }
    }

    // Install on the calling (main/UI) thread; its OUTOFCONTEXT callback is delivered
    // by that thread's message loop, which tao already pumps. Handle is leaked for
    // the app lifetime (unhooked automatically at process exit).
    pub fn install_hook() {
        unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, 0,
                on_foreground, 0, 0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
        }
    }

    // Bring the stored external window to the foreground; returns its handle so the
    // caller can wait out the settle delay before sending the keystrokes.
    pub fn begin_paste() -> Result<isize, String> {
        let hwnd = LAST_FG.load(Ordering::Relaxed);
        if hwnd == 0 || unsafe { IsWindow(hwnd) } == 0 {
            return Err("no-target".into());
        }
        unsafe { SetForegroundWindow(hwnd); }
        Ok(hwnd)
    }

    // After the delay: verify the target really took focus (elevated/UIPI windows
    // refuse it), then synthesize Ctrl+V and, optionally, Enter.
    pub fn finish_paste(hwnd: isize, send_enter: bool) -> Result<(), String> {
        if unsafe { GetForegroundWindow() } != hwnd {
            return Err("focus-failed".into());
        }
        unsafe {
            keybd_event(VK_CONTROL, 0, 0, 0);
            keybd_event(VK_V, 0, 0, 0);
            keybd_event(VK_V, 0, KEYEVENTF_KEYUP, 0);
            keybd_event(VK_CONTROL, 0, KEYEVENTF_KEYUP, 0);
            if send_enter {
                std::thread::sleep(Duration::from_millis(20));
                keybd_event(VK_RETURN, 0, 0, 0);
                keybd_event(VK_RETURN, 0, KEYEVENTF_KEYUP, 0);
            }
        }
        Ok(())
    }
}

// F19: paste the current clipboard into the last external window. The delay lets the
// target window finish taking focus before Ctrl+V; kept off the UI thread via tokio.
#[tauri::command]
async fn paste_into_previous(delay_ms: u64, enter: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let hwnd = autopaste::begin_paste()?;
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms.clamp(10, 1000))).await;
        autopaste::finish_paste(hwnd, enter)
    }
    #[cfg(not(windows))]
    {
        let _ = (delay_ms, enter);
        Err("unsupported".into())
    }
}

// ---- F21 clipboard watcher: a message-only window with AddClipboardFormatListener
// (event-driven, no polling). New external TEXT is appended to the clip_inbox table;
// our own copies and consecutive duplicates are skipped. Off by default; toggling it
// registers/unregisters the listener with no leaked handle.
#[cfg(windows)]
mod clipwatch {
    use super::{db_add_clip, db_conn, lock, now_secs, text_hash, Db};
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
    use std::sync::Mutex;
    use tauri::{AppHandle, Emitter, Manager};

    static WATCHER: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
    static WATCHER_APP: Mutex<Option<AppHandle>> = Mutex::new(None);
    static WATCHER_HWND: AtomicIsize = AtomicIsize::new(0);
    // Set by stop() so a thread that has not yet entered its message loop tears down
    // immediately instead of blocking join() forever (fast on/off toggle deadlock).
    static WATCHER_STOP: AtomicBool = AtomicBool::new(false);
    static LAST_OWN: AtomicU64 = AtomicU64::new(0);
    static LAST_INSERTED: AtomicU64 = AtomicU64::new(0);

    const WM_DESTROY: u32 = 0x0002;
    const WM_CLIPBOARDUPDATE: u32 = 0x031D;
    const HWND_MESSAGE: isize = -3;

    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt_x: i32,
        pt_y: i32,
    }
    type WndProc = unsafe extern "system" fn(isize, u32, usize, isize) -> isize;
    #[repr(C)]
    struct WndClassW {
        style: u32,
        wndproc: Option<WndProc>,
        cls_extra: i32,
        wnd_extra: i32,
        hinstance: isize,
        hicon: isize,
        hcursor: isize,
        hbr: isize,
        menu: *const u16,
        class_name: *const u16,
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterClassW(c: *const WndClassW) -> u16;
        fn CreateWindowExW(ex: u32, class: *const u16, name: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: isize, menu: isize, inst: isize, param: *const core::ffi::c_void) -> isize;
        fn DefWindowProcW(hwnd: isize, msg: u32, w: usize, l: isize) -> isize;
        fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
        fn TranslateMessage(msg: *const Msg) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> isize;
        fn AddClipboardFormatListener(hwnd: isize) -> i32;
        fn RemoveClipboardFormatListener(hwnd: isize) -> i32;
        fn PostMessageW(hwnd: isize, msg: u32, w: usize, l: isize) -> i32;
        fn PostQuitMessage(code: i32);
        fn DestroyWindow(hwnd: isize) -> i32;
        fn RegisterClipboardFormatW(name: *const u16) -> u32;
        fn IsClipboardFormatAvailable(format: u32) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    // Record the hash of text WE just put on the clipboard, so the watcher ignores it.
    pub fn note_own(text: &str) {
        LAST_OWN.store(text_hash(text.trim()), Ordering::Relaxed);
    }

    fn on_clipboard_update() {
        // Poison-tolerant locks: a panic elsewhere must never turn a clipboard event
        // into a process abort (this runs inside an extern "system" wndproc).
        let app = match WATCHER_APP.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            Some(a) => a,
            None => return,
        };
        let (min, cap, enabled) = {
            let state = app.state::<Db>();
            let store = lock(&state);
            let enabled = store.settings.ui_flags.get("clipWatcher") == Some(&true);
            let min = store.settings.ui_values.get("clipMinChars").copied().unwrap_or(3.0).max(1.0) as usize;
            let cap = store.settings.ui_values.get("clipInboxMax").copied().unwrap_or(50.0).clamp(10.0, 200.0) as usize;
            (min, cap, enabled)
        };
        if !enabled {
            return;
        }
        // Respect the clipboard opt-out format password managers (KeePass, 1Password, …)
        // set so their copied secrets never land in monitors / history.
        unsafe {
            let excl = RegisterClipboardFormatW(wide("ExcludeClipboardContentFromMonitorProcessing").as_ptr());
            if excl != 0 && IsClipboardFormatAvailable(excl) != 0 {
                return;
            }
        }
        // Text only — image/file clipboard payloads are ignored.
        let text = match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(t) => t,
            Err(_) => return,
        };
        let trimmed = text.trim();
        if trimmed.chars().count() < min {
            return;
        }
        let h = text_hash(trimmed);
        if h == LAST_OWN.load(Ordering::Relaxed) || h == LAST_INSERTED.load(Ordering::Relaxed) {
            return;
        }
        LAST_INSERTED.store(h, Ordering::Relaxed);
        if let Some(conn) = db_conn(&app) {
            let _ = db_add_clip(&conn, now_secs(), trimmed, cap);
        }
        let _ = app.emit("clip-inbox-changed", ());
    }

    unsafe extern "system" fn wndproc(hwnd: isize, msg: u32, w: usize, l: isize) -> isize {
        match msg {
            WM_CLIPBOARDUPDATE => {
                // A panic must not unwind across this extern "system" boundary (→ abort).
                let _ = std::panic::catch_unwind(on_clipboard_update);
                0
            }
            WM_DESTROY => {
                RemoveClipboardFormatListener(hwnd);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }

    pub fn start(app: AppHandle) {
        let mut g = WATCHER.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_some() {
            return; // already running
        }
        WATCHER_STOP.store(false, Ordering::Release);
        *WATCHER_APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app);
        let handle = std::thread::spawn(|| unsafe {
            let class = wide("PromptSaverClipWatch");
            let name = wide("psclip");
            let wc = WndClassW {
                style: 0,
                wndproc: Some(wndproc),
                cls_extra: 0,
                wnd_extra: 0,
                hinstance: 0,
                hicon: 0,
                hcursor: 0,
                hbr: 0,
                menu: std::ptr::null(),
                class_name: class.as_ptr(),
            };
            RegisterClassW(&wc); // ok if already registered from a previous cycle
            let hwnd = CreateWindowExW(0, class.as_ptr(), name.as_ptr(), 0, 0, 0, 0, 0, HWND_MESSAGE, 0, 0, std::ptr::null());
            if hwnd == 0 {
                return;
            }
            AddClipboardFormatListener(hwnd);
            WATCHER_HWND.store(hwnd, Ordering::Release);
            // stop() may have fired before the window existed (its PostMessage would
            // have been lost) — tear down now instead of blocking on GetMessageW forever.
            if WATCHER_STOP.load(Ordering::Acquire) {
                RemoveClipboardFormatListener(hwnd);
                DestroyWindow(hwnd);
                WATCHER_HWND.store(0, Ordering::Release);
                return;
            }
            let mut msg: Msg = std::mem::zeroed();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
        *g = Some(handle);
    }

    pub fn stop() {
        // Signal first: a thread still starting up will see this right after it
        // publishes its hwnd and exit on its own (see start()).
        WATCHER_STOP.store(true, Ordering::Release);
        let hwnd = WATCHER_HWND.swap(0, Ordering::AcqRel);
        if hwnd != 0 {
            // WM_CLOSE -> DefWindowProc destroys -> WM_DESTROY -> PostQuitMessage.
            unsafe { PostMessageW(hwnd, 0x0010, 0, 0); }
        }
        if let Some(h) = WATCHER.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = h.join();
        }
        *WATCHER_APP.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

// No-op own-copy note on non-Windows.
#[cfg(windows)]
fn note_own_copy(text: &str) {
    clipwatch::note_own(text);
}
#[cfg(not(windows))]
fn note_own_copy(_text: &str) {}

// F21: list / dismiss / clear the clipboard inbox.
#[tauri::command]
async fn clip_inbox_list(app: AppHandle) -> Result<Vec<ClipItem>, String> {
    Ok(db_conn(&app).map(|c| db_list_clips(&c)).unwrap_or_default())
}
#[tauri::command]
async fn clip_inbox_dismiss(app: AppHandle, id: i64) -> Result<(), String> {
    if let Some(c) = db_conn(&app) {
        let _ = c.execute("DELETE FROM clip_inbox WHERE id = ?1", params![id]);
    }
    Ok(())
}
#[tauri::command]
async fn clip_inbox_clear(app: AppHandle) -> Result<(), String> {
    if let Some(c) = db_conn(&app) {
        let _ = c.execute("DELETE FROM clip_inbox", []);
    }
    Ok(())
}

// Set a floating window's position AND size together (logical px). Used by the
// edge/corner resize so the grabbed edge tracks the cursor 1:1.
#[tauri::command]
async fn set_float_bounds(app: AppHandle, id: String, x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
    if [x, y, width, height].iter().all(|v| v.is_finite()) {
        if let Some(win) = app.get_webview_window(&flabel(&id)) {
            let w = width.clamp(48.0, 8000.0);
            let h = height.clamp(48.0, 8000.0);
            #[cfg(windows)]
            if set_float_bounds_native(&win, x, y, w, h) {
                return Ok(());
            }
            let _ = win.set_position(tauri::LogicalPosition::new(x, y));
            let _ = win.set_size(tauri::LogicalSize::new(w, h));
        }
    }
    Ok(())
}

// Persist the per-prompt video player state (volume, mute, loop).
// Async: can fire while scrubbing — keep the settings write off the UI thread.
#[tauri::command]
async fn set_video_prefs(app: AppHandle, state: State<'_, Db>, id: String, volume: u32, muted: bool, looped: bool) -> Result<(), String> {
    let mut store = lock(&state);
    store
        .settings
        .video_prefs
        .insert(id, VideoPrefs { volume: volume.min(100), muted, looped });
    save_settings(&app, &store.settings);
    Ok(())
}

// ---------- Snipping tool ----------
// open_snip freezes the active monitor, shows a transparent overlay window for
// the user to mark a region, then capture_region crops the frozen image,
// copies it to the clipboard, saves a PNG and hands the crop to the main UI.

// One frozen monitor: its capture plus its global physical origin and size.
struct MonitorCap {
    image: image::RgbaImage,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}
struct SnipState(Mutex<Vec<MonitorCap>>);

// Overlay preview prepared in the background the instant the desktop is frozen,
// so the (heavy) JPEG encode overlaps WebView creation instead of running after
// it. None = still encoding, Some(None) = encode failed, Some(Some) = ready.
#[derive(Clone)]
// Frozen-desktop preview. None = still encoding, Some(None) = failed, Some(Some) = ready.
struct SnipPreview(Arc<Mutex<Option<Option<SnipBg>>>>);

// True while a snip session is on screen (between open_snip and capture/cancel).
// The overlay window itself is kept alive and reused, so this flag — not the
// window's existence — is what tells open_snip a snip is already in progress.
static SNIP_ACTIVE: AtomicBool = AtomicBool::new(false);

// End the current snip session: HIDE (never close) the reusable overlay so its
// warm WebView2 survives for the next snip, bring the app's own windows back, and
// drop the temp preview. Closing the overlay is what forced every snip to pay the
// full WebView2 cold-start — keeping it alive is the whole speed win.
fn close_all_snip(app: &AppHandle) {
    for (label, win) in app.webview_windows() {
        if label.starts_with("snip-") {
            let _ = win.hide();
        }
    }
    set_app_windows_hidden(app, false);
    remove_snip_preview();
    SNIP_ACTIVE.store(false, Ordering::SeqCst);
}

// Hide (or restore) the app's own windows — the main window and floating pills —
// around a snip, so a screenshot never contains Prompt Saver itself (this also
// reveals whatever the main window was covering, e.g. Task Manager).
fn set_app_windows_hidden(app: &AppHandle, hidden: bool) {
    for (label, win) in app.webview_windows() {
        if label == "main" || label.starts_with("float-") {
            if hidden {
                let _ = win.hide();
            } else {
                let _ = win.show();
                // Bring the main window back to the front (and out of any minimized
                // state) so finishing/cancelling a snip never leaves it hidden behind
                // other windows — which reads as the app having minimized itself.
                if label == "main" {
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            }
        }
    }
}

// Hide (or re-show) only the snip overlay(s). Used by the window-capture
// workaround, which needs to grab the LIVE desktop — the overlay (showing the
// frozen image) would otherwise be what gets captured.
fn set_snip_overlay_hidden(app: &AppHandle, hidden: bool) {
    for (label, win) in app.webview_windows() {
        if label.starts_with("snip-") {
            let _ = if hidden { win.hide() } else { win.show() };
        }
    }
}

// The snip preview JPEG is written to a temp file and shown via the asset
// protocol — far quicker than passing a multi-MB base64 data URL through IPC and
// decoding it in the overlay. A rotating name avoids any stale webview cache.
static SNIP_PREVIEW_SEQ: AtomicU64 = AtomicU64::new(0);
static SNIP_PREVIEW_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

fn write_snip_preview(jpeg: &[u8]) -> Option<String> {
    let seq = SNIP_PREVIEW_SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("promptsaver-snip-{}.jpg", seq));
    fs::write(&path, jpeg).ok()?;
    if let Some(old) = SNIP_PREVIEW_FILE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(path.clone())
    {
        let _ = fs::remove_file(old);
    }
    Some(path.to_string_lossy().into_owned())
}

fn remove_snip_preview() {
    if let Some(p) = SNIP_PREVIEW_FILE.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = fs::remove_file(p);
    }
}

#[derive(Serialize, Clone)]
struct SnipBg {
    // Preview source: a temp-file path (is_file=true, loaded via convertFileSrc)
    // or a base64 data URL fallback (is_file=false).
    src: String,
    is_file: bool,
    // Full stitched dimensions (the display image may be downscaled); the
    // frontend maps coordinates against these, so crops stay full-resolution.
    width: u32,
    height: u32,
}

#[derive(Serialize, Clone)]
struct SnipResult {
    data_url: String,
    path: String,
    // Suggested button name: "Screenshot <app?> <timestamp>".
    name: String,
}

// A selectable top-level window in stitched-image physical pixels.
#[derive(Serialize)]
struct SnipWindow {
    id: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

// ---- Fast screen capture via GDI (one BitBlt; no per-monitor DXGI setup) ----

// Capture a screen rectangle (global physical px) with GDI BitBlt. Immediate (no
// DXGI latency) and includes every visible window — elevated ones too.
#[cfg(windows)]
fn capture_screen_rect(x: i32, y: i32, w: i32, h: i32) -> Option<image::RgbaImage> {
    #[link(name = "user32")]
    extern "system" {
        fn GetDC(hwnd: isize) -> isize;
        fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(hdc: isize) -> isize;
        fn CreateCompatibleBitmap(hdc: isize, w: i32, h: i32) -> isize;
        fn SelectObject(hdc: isize, h: isize) -> isize;
        fn BitBlt(dst: isize, x: i32, y: i32, w: i32, h: i32, src: isize, sx: i32, sy: i32, rop: u32) -> i32;
        fn DeleteObject(h: isize) -> i32;
        fn DeleteDC(hdc: isize) -> i32;
        fn GetDIBits(hdc: isize, hbm: isize, start: u32, lines: u32, bits: *mut u8, bmi: *mut BmInfo, usage: u32) -> i32;
    }
    #[repr(C)]
    struct BmHeader {
        size: u32, width: i32, height: i32, planes: u16, bit_count: u16,
        compression: u32, size_image: u32, x_ppm: i32, y_ppm: i32, clr_used: u32, clr_important: u32,
    }
    #[repr(C)]
    struct BmInfo { header: BmHeader, colors: [u32; 3] }
    const SRCCOPY: u32 = 0x00CC_0020;
    // Reject non-positive or absurdly large rects: keep w*h*4 a sane allocation so
    // a spoofed/extreme virtual-screen metric can never abort the process on OOM.
    // 300M px ≈ 1.2 GB RGBA — far above any real multi-monitor desktop.
    if w <= 0 || h <= 0 || (w as u64) * (h as u64) > 300_000_000 {
        return None;
    }
    unsafe {
        let screen = GetDC(0);
        if screen == 0 {
            return None;
        }
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp);
        // SRCCOPY only (no CAPTUREBLT — it forces a slow full recomposite). The
        // DWM-composited desktop already includes every normal/elevated window.
        let blt_ok = BitBlt(mem, 0, 0, w, h, screen, x, y, SRCCOPY) != 0;
        // Deselect the bitmap before GetDIBits (required by the API).
        let _ = SelectObject(mem, old);
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let mut bmi = BmInfo {
            header: BmHeader {
                size: std::mem::size_of::<BmHeader>() as u32,
                width: w,
                height: -h, // negative => top-down rows
                planes: 1,
                bit_count: 32,
                compression: 0,
                size_image: 0,
                x_ppm: 0,
                y_ppm: 0,
                clr_used: 0,
                clr_important: 0,
            },
            colors: [0; 3],
        };
        let got = if blt_ok {
            GetDIBits(mem, bmp, 0, h as u32, buf.as_mut_ptr(), &mut bmi, 0)
        } else {
            0
        };
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(0, screen);
        if got == 0 {
            return None;
        }
        // GDI returns BGRA; convert to RGBA and force opaque alpha. Parallelized —
        // a multi-monitor capture is tens of millions of pixels, so a serial swap
        // was a big slice of the freeze time.
        use rayon::prelude::*;
        buf.par_chunks_exact_mut(4).for_each(|px| {
            px.swap(0, 2);
            px[3] = 255;
        });
        image::RgbaImage::from_raw(w as u32, h as u32, buf)
    }
}

// Whole virtual desktop via GDI → (image, origin_x, origin_y) global physical px.
#[cfg(windows)]
fn capture_desktop() -> Option<(image::RgbaImage, i32, i32)> {
    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(n: i32) -> i32;
    }
    const SM_XVIRTUALSCREEN: i32 = 76;
    const SM_YVIRTUALSCREEN: i32 = 77;
    const SM_CXVIRTUALSCREEN: i32 = 78;
    const SM_CYVIRTUALSCREEN: i32 = 79;
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let img = capture_screen_rect(vx, vy, vw, vh)?;
    Some((img, vx, vy))
}

// Freeze the whole desktop into one image (global physical px). Fast GDI path on
// Windows; per-monitor xcap stitch as a fallback (and on other platforms).
fn freeze_desktop() -> Result<(image::RgbaImage, i32, i32), String> {
    #[cfg(windows)]
    if let Some(res) = capture_desktop() {
        return Ok(res);
    }
    let mut caps = Vec::new();
    {
        let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
        for m in &monitors {
            if let Ok(image) = m.capture_image() {
                caps.push((image, m.x(), m.y(), m.width(), m.height()));
            }
        }
    }
    if caps.is_empty() {
        return Err("capture failed".to_string());
    }
    let min_x = caps.iter().map(|c| c.1).min().unwrap_or(0);
    let min_y = caps.iter().map(|c| c.2).min().unwrap_or(0);
    let max_x = caps.iter().map(|c| c.1 + c.3 as i32).max().unwrap_or(0);
    let max_y = caps.iter().map(|c| c.2 + c.4 as i32).max().unwrap_or(0);
    let total_w = (max_x - min_x).max(1) as u32;
    let total_h = (max_y - min_y).max(1) as u32;
    let mut canvas = image::RgbaImage::new(total_w, total_h);
    for (image, x, y, _, _) in &caps {
        image::imageops::replace(&mut canvas, image, (*x - min_x) as i64, (*y - min_y) as i64);
    }
    Ok((canvas, min_x, min_y))
}

// Build the (hidden) snip overlay window — borderless WS_POPUP, no shadow, fixed
// size, always-on-top. Created once and reused across snips so the WebView2
// cold-start is paid a single time. A Destroyed handler clears the session and
// restores the app's windows if the overlay is ever torn down.
fn build_snip_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let win = WebviewWindowBuilder::new(app, "snip-0", WebviewUrl::App("snip.html".into()))
        .title("")
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    let excluded = app
        .try_state::<Db>()
        .map(|s| capture_excluded(&s.lock().unwrap_or_else(|e| e.into_inner()).settings))
        .unwrap_or(true);
    set_capture_exclusion(&win, excluded);
    let app_evt = app.clone();
    win.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) && SNIP_ACTIVE.load(Ordering::SeqCst) {
            SNIP_ACTIVE.store(false, Ordering::SeqCst);
            set_app_windows_hidden(&app_evt, false);
        }
    });
    Ok(win)
}

// Freeze the desktop and open a single overlay spanning all screens. The
// frontend maps the cursor by ratio against the displayed frozen image, so the
// selection is pixel-exact on any DPI mix AND can be dragged across monitors.
//
// Speed: the preview JPEG is encoded on a background thread the moment the
// desktop is frozen, so it runs CONCURRENTLY with WebView creation rather than
// serially after it; the overlay window is built hidden and only revealed once
// its frozen image has decoded (snip_present), so it never flashes up blank.
#[tauri::command]
async fn open_snip(
    app: AppHandle,
    state: State<'_, SnipState>,
    preview: State<'_, SnipPreview>,
) -> Result<(), String> {
    // Atomically claim the session. If one is already active and its overlay still
    // exists, surface that instead of starting a second. (Stale flag with no
    // window — e.g. the overlay was destroyed — falls through: reclaim and start.)
    if SNIP_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        if let Some(win) = app.get_webview_window("snip-0") {
            let _ = win.show();
            let _ = win.set_focus();
            return Ok(());
        }
        SNIP_ACTIVE.store(true, Ordering::SeqCst);
    }
    // Wake the overlay's WebView2 NOW (off-screen) so a long-idle, background-throttled
    // webview un-throttles DURING the freeze+encode below. Without this head start the
    // icon path (overlay idle a while) lags behind a retry (whose webview was just
    // on-screen, still awake). The reuse path re-parks it full off-screen, then emits.
    if let Some(win) = app.get_webview_window("snip-0") {
        let _ = win.set_size(PhysicalSize::new(2, 2));
        let _ = win.set_position(PhysicalPosition::new(-32000, -32000));
        let _ = win.show();
    }
    // Hide our own windows BEFORE freezing so they're never in the capture and
    // the target underneath (e.g. Task Manager behind the main window) is
    // revealed. A short settle lets the hide reach the screen first.
    set_app_windows_hidden(&app, true);
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let (canvas, min_x, min_y) = freeze_desktop()?;
    let (total_w, total_h) = canvas.dimensions();
    // Kick off the preview encode immediately (clone for the worker; the original
    // stays in state for full-res crops). snip_background just awaits the result.
    let enc_img = canvas.clone();
    *state.0.lock().unwrap_or_else(|e| e.into_inner()) = vec![MonitorCap {
        image: canvas,
        x: min_x,
        y: min_y,
        width: total_w,
        height: total_h,
    }];
    let slot = preview.0.clone();
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = None;
    tauri::async_runtime::spawn_blocking(move || {
        // Encode the capture at NATIVE resolution — pixel-sharp overlay, no downscale.
        let bg = encode_snip_jpeg(enc_img.as_raw(), total_w, total_h, SNIP_Q).map(|j| {
            let (src, is_file) = match write_snip_preview(&j) {
                Some(path) => (path, true),
                None => (format!("data:image/jpeg;base64,{}", base64_encode(&j)), false),
            };
            SnipBg { src, is_file, width: total_w, height: total_h }
        });
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(bg);
    });

    // Reuse the overlay if it already exists. A long-idle HIDDEN WebView2 is heavily
    // background-throttled (clamped timers, paused rAF/IPC) — THAT, not cold-start, is
    // why the first snip lagged behind a retry (whose webview was shown moments ago).
    // So wake it WITHOUT a flash: show it 2×2 off-screen, which flips the page to
    // "visible" and resumes its JS at full speed. snip_present (fired on the frozen
    // image's onload, now prompt) sizes + moves it on-screen, so the reveal is instant
    // and flash-free. Same path for the first snip and every retry → same speed.
    if let Some(win) = app.get_webview_window("snip-0") {
        // Park it at FULL size but just off-screen (below everything), shown so the
        // webview un-throttles and paints. snip_present then only MOVES it on-screen —
        // no resize, no DPI re-assert — so the reveal can't flicker.
        let _ = win.set_size(PhysicalSize::new(total_w, total_h));
        let _ = win.set_position(PhysicalPosition::new(min_x, min_y + total_h as i32 + 100));
        let _ = win.show();
        let _ = app.emit_to("snip-0", "snip-begin", ());
        spawn_reveal_watchdog(app.clone());
        return Ok(());
    }

    // Pre-warm missed (first snip happened before startup pre-warm built it): build
    // the overlay now, paying the WebView2 cold-start once. SNIP_ACTIVE is already
    // set, so the page's load-time snip_should_begin check drives this session.
    let win = match build_snip_window(&app) {
        Ok(w) => w,
        Err(e) => {
            // Build failed: clear the session so the app's windows come back.
            SNIP_ACTIVE.store(false, Ordering::SeqCst);
            set_app_windows_hidden(&app, false);
            return Err(e);
        }
    };
    // Size/position the still-hidden window to cover the virtual desktop exactly
    // (size first, then position, so the final op is the position a resize could
    // otherwise nudge). snip_present re-asserts after the DPI settle, then shows.
    let _ = win.set_size(PhysicalSize::new(total_w, total_h));
    let _ = win.set_position(PhysicalPosition::new(min_x, min_y));
    spawn_reveal_watchdog(app.clone());
    Ok(())
}

// Show the overlay (idempotent): place it to the frozen-desktop geometry, reveal,
// and re-assert after the per-monitor DPI settle the show can trigger. Used by the
// page (snip_present, fast path after the frozen image decodes) and by a backend
// watchdog that force-reveals if the page somehow never asks.
async fn reveal_snip(app: &AppHandle) {
    let geom = {
        let state: State<SnipState> = app.state();
        let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.first().map(|c| (c.x, c.y, c.width, c.height))
    };
    let (min_x, min_y, total_w, total_h) = match geom {
        Some(g) => g,
        None => return,
    };
    if let Some(win) = app.get_webview_window("snip-0") {
        // Already full-size (parked off-screen on reuse), so this moves it on-screen in
        // one shot — no resize, no re-assert → no flicker. The frozen image is already
        // decoded (snip_present fires on its onload). set_size covers the cold path.
        let _ = win.set_size(PhysicalSize::new(total_w, total_h));
        let _ = win.set_position(PhysicalPosition::new(min_x, min_y));
        let _ = win.show();
        let _ = win.set_focus();
    }
}

// Reveal the prepared overlay once its frozen image has decoded in the page, so
// the screenshot view appears already-painted instead of flashing blank first.
#[tauri::command]
async fn snip_present(app: AppHandle) -> Result<(), String> {
    reveal_snip(&app).await;
    Ok(())
}

// Belt-and-suspenders: if the page hasn't revealed the overlay shortly after a
// snip starts (e.g. a hidden WebView2 ever suspended its JS), force it visible so
// the screenshot view can never get stuck hidden. No-op once already shown.
fn spawn_reveal_watchdog(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        if !SNIP_ACTIVE.load(Ordering::SeqCst) {
            return;
        }
        if let Some(win) = app.get_webview_window("snip-0") {
            if !win.is_visible().unwrap_or(true) {
                reveal_snip(&app).await;
            }
        }
    });
}

// The overlay page asks this on load: begin a session immediately only if one is
// already active (the cold-build path), otherwise stay idle — this is what lets the
// window be pre-warmed at startup without auto-triggering a bogus snip. Reused
// windows are driven per-snip by the "snip-begin" event instead.
#[tauri::command]
fn snip_should_begin() -> bool {
    SNIP_ACTIVE.load(Ordering::SeqCst)
}

// Visible top-level windows overlapping the virtual desktop, topmost first, in
// stitched-image physical pixels — for hover highlight + single-window capture.
#[tauri::command]
fn snip_windows(state: State<SnipState>, index: usize) -> Vec<SnipWindow> {
    // Read the stitched origin/bounds, then drop the lock before the OS-wide
    // window enumeration so the snip state isn't held during the slow Win32 calls.
    let (ox, oy, ow, oh) = {
        let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        match guard.get(index) {
            Some(c) => (c.x, c.y, c.width as i32, c.height as i32),
            None => return Vec::new(),
        }
    };
    let mut out = Vec::new();
    if let Ok(windows) = xcap::Window::all() {
        for w in windows {
            if w.is_minimized() || w.title().is_empty() {
                continue;
            }
            let (gx, gy, ww, wh) = (w.x(), w.y(), w.width() as i32, w.height() as i32);
            if ww <= 0 || wh <= 0 {
                continue;
            }
            // Keep only windows overlapping the virtual desktop.
            if gx >= ox + ow || gy >= oy + oh || gx + ww <= ox || gy + wh <= oy {
                continue;
            }
            // Local to the stitched image.
            out.push(SnipWindow {
                id: w.id(),
                x: gx - ox,
                y: gy - oy,
                width: ww as u32,
                height: wh as u32,
            });
        }
    }
    out
}

// Overlay preview JPEG quality (encoded at native resolution — pixel-sharp).
const SNIP_Q: u8 = 80;
// Saved-button JPEG quality: higher (crisp text) but still a fraction of a PNG's
// size + encode time — a full-res PNG data URL used to stall the whole app on save.
const SNIP_SAVE_Q: u8 = 90;

// Encode RGBA bytes to a JPEG for the overlay preview. jpeg-encoder = SIMD baseline JPEG,
// much faster than the image crate's. Returns the raw JPEG bytes.
fn encode_snip_jpeg(rgba: &[u8], w: u32, h: u32, quality: u8) -> Option<Vec<u8>> {
    let mut buf = Vec::<u8>::new();
    jpeg_encoder::Encoder::new(&mut buf, quality)
        .encode(rgba, w as u16, h as u16, jpeg_encoder::ColorType::Rgba)
        .ok()?;
    Some(buf)
}

// Hand the overlay its frozen-desktop preview. The encode was started in
// open_snip the instant the desktop was frozen; here we just wait for it (the
// slot is usually already filled, so this returns near-instantly) and hand it
// over. index is kept for the existing single-overlay call signature.
#[tauri::command]
async fn snip_background(preview: State<'_, SnipPreview>, index: usize) -> Result<Option<SnipBg>, String> {
    let _ = index;
    for _ in 0..2000 {
        let ready = preview.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(bg) = ready {
            return Ok(bg);
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    Ok(None)
}

#[tauri::command]
fn snip_cancel(app: AppHandle, state: State<SnipState>) {
    close_all_snip(&app);
    state.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

// Target folder for saved screenshots: a custom expert path, else Pictures\Screenshots.
fn screenshot_dir(custom: &str) -> PathBuf {
    let c = custom.trim();
    if !c.is_empty() {
        return PathBuf::from(c);
    }
    let base = std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("Pictures").join("Screenshots")
}

// Save screenshot bytes (JPEG) into the given folder as "<stem>.jpg". "" on failure.
fn save_screenshot(bytes: &[u8], dir: &std::path::Path, stem: &str) -> String {
    let _ = fs::create_dir_all(dir);
    let path = dir.join(format!("{}.jpg", stem));
    if fs::write(&path, bytes).is_ok() {
        path.to_string_lossy().to_string()
    } else {
        String::new()
    }
}

// Decode a "data:...;base64,..." URL to its raw image bytes (format-agnostic).
fn data_url_bytes(data_url: &str) -> Vec<u8> {
    let b64 = data_url.rsplit_once(',').map(|(_, b)| b).unwrap_or(data_url);
    base64_decode(b64.trim())
}

// Save a screenshot data URL into Pictures\Screenshots now (result dialog: the
// user enabled folder-saving after the shot was taken). Returns the path.
#[tauri::command]
async fn save_screenshot_now(state: State<'_, Db>, data_url: String) -> Result<Option<String>, String> {
    let bytes = data_url_bytes(&data_url);
    if bytes.is_empty() {
        return Ok(None);
    }
    let custom = lock(&state).settings.ui_texts.get("screenshotDir").cloned().unwrap_or_default();
    let path = save_screenshot(&bytes, &screenshot_dir(&custom), &format!("Screenshot-{}", local_stamp()));
    Ok(if path.is_empty() { None } else { Some(path) })
}

// Save the screenshot anywhere via a native "Save as" dialog. Returns the chosen
// path, or None if the user cancelled.
#[tauri::command]
async fn save_screenshot_as(app: AppHandle, data_url: String) -> Result<Option<String>, String> {
    let bytes = data_url_bytes(&data_url);
    if bytes.is_empty() {
        return Err("empty image".to_string());
    }
    let name = format!("Screenshot-{}.jpg", local_stamp());
    match file_dialog(&app)
        .add_filter("JPEG", &["jpg"])
        .set_file_name(&name)
        .save_file()
    {
        Some(p) => {
            fs::write(&p, &bytes).map_err(|e| e.to_string())?;
            Ok(Some(p.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

// Delete a previously auto-saved screenshot (result dialog: folder-saving turned
// off). Safety: only ever removes our own Screenshot-*.jpg (or legacy
// PromptSaver-*.png) files.
#[tauri::command]
async fn delete_screenshot_file(state: State<'_, Db>, path: String) -> Result<bool, String> {
    let p = std::path::Path::new(&path);
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ours = (name.starts_with("Screenshot-") && name.ends_with(".jpg"))
        || (name.starts_with("PromptSaver-") && name.ends_with(".png"));
    if !ours || !p.is_file() {
        return Ok(false);
    }
    // Defense in depth: only ever delete inside a folder we actually save screenshots
    // to (the configured/default screenshot dir, or temp) — never an arbitrary location
    // a matching file name could otherwise point at.
    let parent = match p.parent().and_then(|d| d.canonicalize().ok()) {
        Some(d) => d,
        None => return Ok(false),
    };
    let custom = lock(&state).settings.ui_texts.get("screenshotDir").cloned().unwrap_or_default();
    let allowed = [screenshot_dir(&custom), std::env::temp_dir()];
    let in_allowed = allowed
        .iter()
        .any(|d| d.canonicalize().map(|c| c == parent).unwrap_or(false));
    Ok(in_allowed && fs::remove_file(p).is_ok())
}

// Default screenshot folder path (shown greyed in the expert menu when no custom
// folder is set), so the user always sees where screenshots would go.
#[tauri::command]
fn default_screenshot_dir() -> String {
    screenshot_dir("").to_string_lossy().to_string()
}

// Native folder picker (used by the expert screenshot-dir + data-dir options).
#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<String> {
    file_dialog(&app)
        .pick_folder()
        .map(|p| p.to_string_lossy().to_string())
}

// Current effective store location (shown in the expert menu).
#[tauri::command]
fn current_data_dir(app: AppHandle) -> String {
    data_dir(&app).to_string_lossy().to_string()
}

// The exact folder the last set_data_dir moved AWAY from — the only directory
// delete_data_dir is allowed to clean, so the webview can never point that command
// at another app's folder full of same-named files (settings.json / data.db).
static LAST_OLD_DATA_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

// Redirect the store to a new folder (applies on next launch). Adopts an existing
// store in the target if present, otherwise copies the current one across. The old
// data is left untouched; the caller may offer to delete it. Returns the old path.
#[tauri::command]
async fn set_data_dir(app: AppHandle, state: State<'_, Db>, dir: String) -> Result<String, String> {
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        return Err("empty path".to_string());
    }
    let new = PathBuf::from(trimmed);
    fs::create_dir_all(&new).map_err(|e| e.to_string())?;
    let current = data_dir(&app);
    let old = current.to_string_lossy().to_string();
    if new == current {
        return Ok(old);
    }
    // Remember which folder we are leaving — the only one delete_data_dir may clean.
    *LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(current.clone());
    // Flush the live state to the current location first.
    {
        let store = lock(&state);
        save_prompts(&app, &store);
        save_settings(&app, &store.settings);
    }
    // Fold the WAL back into data.db so the single-file copy below is complete
    // (recent copy_log appends can otherwise still live in data.db-wal).
    if let Some(c) = db_conn(&app) {
        let _ = c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }
    // Adopt an existing store in the target; otherwise copy ours across. Abort
    // before writing the redirect pointer if the copy fails, so a launch can
    // never be pointed at a truncated store.
    if !new.join("data.db").exists() {
        let src = current.join("data.db");
        if src.exists() {
            fs::copy(&src, new.join("data.db")).map_err(|e| format!("copy store: {}", e))?;
        }
    }
    // Point future launches at the new folder (pointer lives in the canonical dir).
    fs::write(
        default_data_dir(&app).join("datapath"),
        new.to_string_lossy().as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    Ok(old)
}

// Delete the store files in a folder (old location after a move). Only ever
// removes our known data files — never the folder itself or anything else.
#[tauri::command]
async fn delete_data_dir(path: String) -> bool {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return false;
    }
    // Only ever clean the folder set_data_dir just moved away from — never an
    // arbitrary webview-supplied directory that happens to hold same-named files.
    let allowed = LAST_OLD_DATA_DIR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|p| *p == dir);
    if !allowed {
        return false;
    }
    let mut ok = true;
    for f in ["data.db", "data.db-wal", "data.db-shm", "prompts.json", "settings.json"] {
        let p = dir.join(f);
        if p.exists() {
            ok &= fs::remove_file(&p).is_ok();
        }
    }
    ok
}

// Local wall-clock stamp for screenshot names: "YYYY-MM-DD_HH-MM-SS" (filename-safe).
#[cfg(windows)]
fn local_stamp() -> String {
    #[repr(C)]
    struct SystemTimeW { year: u16, month: u16, dow: u16, day: u16, hour: u16, min: u16, sec: u16, ms: u16 }
    #[link(name = "kernel32")]
    extern "system" { fn GetLocalTime(st: *mut SystemTimeW); }
    let mut st = SystemTimeW { year: 0, month: 0, dow: 0, day: 0, hour: 0, min: 0, sec: 0, ms: 0 };
    unsafe { GetLocalTime(&mut st) };
    format!("{:04}-{:02}-{:02}_{:02}-{:02}-{:02}", st.year, st.month, st.day, st.hour, st.min, st.sec)
}
#[cfg(not(windows))]
fn local_stamp() -> String { now_secs().to_string() }

// App label for a window shot: drop ".exe" + any path, strip characters illegal in
// a filename, trim. Empty if nothing usable.
fn clean_app_name(raw: &str) -> String {
    let base = raw.rsplit(['\\', '/']).next().unwrap_or(raw);
    let stem = base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base);
    stem.chars().filter(|c| !"<>:\"/\\|?*".contains(*c)).collect::<String>().trim().to_string()
}

// Encode as JPEG, copy to the clipboard, optionally archive to the folder, close the
// overlay and notify the UI. JPEG (not PNG): a full-res PNG data URL stalled the save.
fn finalize_capture(app: &AppHandle, crop: image::RgbaImage, app_name: Option<String>) -> Result<(), String> {
    let (cw, ch) = crop.dimensions();
    if cw == 0 || ch == 0 {
        return Err("empty region".to_string());
    }
    let jpeg = encode_snip_jpeg(crop.as_raw(), cw, ch, SNIP_SAVE_Q).ok_or("encode failed")?;
    let data_url = format!("data:image/jpeg;base64,{}", base64_encode(&jpeg));

    let _ = arboard::Clipboard::new().and_then(|mut c| {
        c.set_image(arboard::ImageData {
            width: cw as usize,
            height: ch as usize,
            bytes: crop.into_raw().into(),
        })
    });

    // "Screenshot" + a precise timestamp (+ the source app for a window shot) — the
    // button name and the optional folder file share the same stamp.
    let stamp = local_stamp();
    let label = app_name.as_deref().map(clean_app_name).filter(|s| !s.is_empty());
    let (name, stem) = match &label {
        Some(a) => (format!("Screenshot {} {}", a, stamp), format!("Screenshot-{}-{}", a, stamp)),
        None => (format!("Screenshot {}", stamp), format!("Screenshot-{}", stamp)),
    };

    // Default: temp-only — the shot lives only as an in-app prompt (encrypted,
    // removed when the prompt is deleted). The folder copy is opt-in and honors
    // a custom screenshot directory (expert menu).
    let (save_to_folder, custom_dir) = app
        .try_state::<Db>()
        .map(|s| {
            let g = s.lock().unwrap_or_else(|e| e.into_inner());
            (
                g.settings.ui_flags.get("screenshotSave") == Some(&true),
                g.settings.ui_texts.get("screenshotDir").cloned().unwrap_or_default(),
            )
        })
        .unwrap_or((false, String::new()));
    let path = if save_to_folder {
        save_screenshot(&jpeg, &screenshot_dir(&custom_dir), &stem)
    } else {
        String::new()
    };

    close_all_snip(app);
    app.state::<SnipState>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    let _ = app.emit("snip-captured", SnipResult { data_url, path, name });
    Ok(())
}

// Crop an image at a physical-pixel rect (clamped to its bounds).
fn crop_rgba(img: &image::RgbaImage, x: i32, y: i32, width: u32, height: u32) -> Option<image::RgbaImage> {
    let (iw, ih) = img.dimensions();
    let cx = x.max(0) as u32;
    let cy = y.max(0) as u32;
    let cw = width.min(iw.saturating_sub(cx));
    let ch = height.min(ih.saturating_sub(cy));
    if cw == 0 || ch == 0 {
        return None;
    }
    Some(image::imageops::crop_imm(img, cx, cy, cw, ch).to_image())
}

// Crop the frozen monitor capture at a physical-pixel rect.
fn crop_frozen(
    state: &State<SnipState>,
    index: usize,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Option<image::RgbaImage> {
    let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
    crop_rgba(&guard.get(index)?.image, x, y, width, height)
}

// Min/max luminance over a 4×4 sample grid (used by the capture quality checks).
fn luma_range(img: &image::RgbaImage) -> Option<(i32, i32)> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let (mut min_l, mut max_l) = (255i32, 0i32);
    for gy in 0..4u32 {
        for gx in 0..4u32 {
            let px = (gx * (w - 1) / 3).min(w - 1);
            let py = (gy * (h - 1) / 3).min(h - 1);
            let p = img.get_pixel(px, py);
            let l = (p[0] as i32 + p[1] as i32 + p[2] as i32) / 3;
            min_l = min_l.min(l);
            max_l = max_l.max(l);
        }
    }
    Some((min_l, max_l))
}

// True when a direct PrintWindow capture looks blocked — near black OR perfectly
// uniform. Used only to decide whether to fall back to the frozen screen, where a
// uniform-but-valid window still ends up captured.
fn capture_looks_bad(img: &image::RgbaImage) -> bool {
    match luma_range(img) {
        Some((min_l, max_l)) => max_l < 12 || max_l - min_l < 4,
        None => true,
    }
}

// True when the FINAL crop is unusable — empty or pure black. Protected windows
// (Task Manager, secured dialogs) that even the frozen screen could not capture
// end up black; such a defective image must never be saved (we error instead).
fn capture_is_blank(img: &image::RgbaImage) -> bool {
    match luma_range(img) {
        Some((_, max_l)) => max_l < 12,
        None => true,
    }
}

// Crop the frozen capture (physical pixels) and finalize.
#[tauri::command]
async fn capture_region(
    app: AppHandle,
    state: State<'_, SnipState>,
    index: usize,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let crop = crop_frozen(&state, index, x, y, width, height).ok_or("empty region")?;
    finalize_capture(&app, crop, None)
}

// Top-level windows stacked ABOVE the target that overlap its rect (global
// physical px), topmost first. These are what hide the target on the frozen
// desktop; the capture workaround minimizes them. Windows without a caption
// (overlays, shell surfaces, our own title-less snip overlay) are skipped.
#[cfg(windows)]
fn occluders_above(id: u32, tx: i32, ty: i32, tw: i32, th: i32) -> Vec<isize> {
    #[repr(C)]
    struct RectW { left: i32, top: i32, right: i32, bottom: i32 }
    #[link(name = "user32")]
    extern "system" {
        fn GetTopWindow(hwnd: isize) -> isize;
        fn GetWindow(hwnd: isize, cmd: u32) -> isize;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn IsIconic(hwnd: isize) -> i32;
        fn GetWindowRect(hwnd: isize, r: *mut RectW) -> i32;
        fn GetWindowTextLengthW(hwnd: isize) -> i32;
    }
    const GW_HWNDNEXT: u32 = 2;
    let (tl, tt, tr, tb) = (tx, ty, tx + tw, ty + th);
    let mut out = Vec::new();
    let mut found = false;
    unsafe {
        let mut h = GetTopWindow(0);
        while h != 0 {
            if h as u32 == id {
                found = true;
                break; // everything after the target sits behind it
            }
            if IsWindowVisible(h) != 0 && IsIconic(h) == 0 && GetWindowTextLengthW(h) > 0 {
                let mut r = RectW { left: 0, top: 0, right: 0, bottom: 0 };
                if GetWindowRect(h, &mut r) != 0
                    && r.left < tr && r.right > tl && r.top < tb && r.bottom > tt
                {
                    out.push(h);
                }
            }
            h = GetWindow(h, GW_HWNDNEXT);
        }
    }
    // Target's z-position unknown (cloaked/child): don't minimize blindly.
    if !found { out.clear(); }
    out
}

#[cfg(windows)]
fn show_window(hwnd: isize, cmd: i32) {
    #[link(name = "user32")]
    extern "system" {
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    }
    unsafe { ShowWindow(hwnd, cmd); }
}

// True when the window belongs to a higher-integrity / elevated process (e.g.
// Task Manager). Such windows block PrintWindow, which can return a misleading
// part-black image instead of failing cleanly — so we skip the direct path for
// them and capture via the frozen desktop / minimize workaround, which work
// regardless of elevation. Heuristic: a medium-integrity caller cannot open an
// elevated process for PROCESS_QUERY_INFORMATION (access denied).
#[cfg(windows)]
fn window_is_protected(id: u32) -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn CloseHandle(h: isize) -> i32;
    }
    const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(id as isize, &mut pid);
        if pid == 0 {
            return false;
        }
        let h = OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
        if h == 0 {
            true
        } else {
            CloseHandle(h);
            false
        }
    }
}

// Capture the chosen window. PrintWindow first — it grabs the window's own
// content, excluding anything stacked on top (clean for normal windows).
// Protected/elevated windows (e.g. Task Manager) block PrintWindow. If nothing
// covers the target, crop the frozen desktop (taken with our windows hidden) —
// a GDI grab contains even elevated windows, with no flicker. If other windows
// DO cover it, the freeze has them baked in, so run the workaround: hide our
// overlay, minimize the covering windows, grab the now-clear area live, then
// restore them.
#[tauri::command]
async fn capture_window(app: AppHandle, state: State<'_, SnipState>, id: u32) -> Result<(), String> {
    // Source app name for the button label (best-effort; empty for unknown windows).
    let app_name = xcap::Window::all()
        .ok()
        .and_then(|ws| ws.into_iter().find(|w| w.id() == id))
        .map(|w| w.app_name().to_string());
    let protected = {
        #[cfg(windows)]
        {
            window_is_protected(id)
        }
        #[cfg(not(windows))]
        {
            false
        }
    };
    // 1. Direct PrintWindow — best for normal windows (own content, no overlays).
    // Skipped for elevated windows: PrintWindow is unreliable there and can pass
    // the quality check with a wrong image, so route them to the robust paths.
    let direct = if protected {
        None
    } else {
        xcap::Window::all()
            .ok()
            .and_then(|ws| ws.into_iter().find(|w| w.id() == id))
            .and_then(|w| w.capture_image().ok())
            .filter(|im| !capture_looks_bad(im))
    };
    if let Some(im) = direct {
        return finalize_capture(&app, im, app_name.clone());
    }

    // Target's global physical rect + frozen-image origin.
    let rect = xcap::Window::all()
        .ok()
        .and_then(|ws| ws.into_iter().find(|w| w.id() == id))
        .map(|w| (w.x(), w.y(), w.width() as i32, w.height() as i32));
    let (wx, wy, ww, wh) = match rect {
        Some(r) => r,
        None => return Err("blocked".to_string()),
    };
    let origin = {
        let g = state.0.lock().unwrap_or_else(|e| e.into_inner());
        g.first().map(|c| (c.x, c.y))
    };

    #[cfg(windows)]
    {
        let occ = occluders_above(id, wx, wy, ww, wh);

        // 2. Unobstructed: the frozen desktop already shows the target cleanly.
        if occ.is_empty() {
            if let Some((ox, oy)) = origin {
                if let Some(im) = crop_frozen(&state, 0, wx - ox, wy - oy, ww as u32, wh as u32) {
                    if !capture_is_blank(&im) {
                        return finalize_capture(&app, im, app_name.clone());
                    }
                }
            }
        }

        // 3. Workaround: minimize the covering windows, grab the cleared area live.
        const SW_SHOWMINNOACTIVE: i32 = 7;
        const SW_SHOWNOACTIVATE: i32 = 4;
        set_snip_overlay_hidden(&app, true);
        for &o in &occ {
            show_window(o, SW_SHOWMINNOACTIVE);
        }
        // Let the minimize animation finish (so the area is truly clear) and the
        // desktop recompose before grabbing.
        tokio::time::sleep(std::time::Duration::from_millis(if occ.is_empty() { 40 } else { 280 })).await;
        let grab = capture_screen_rect(wx, wy, ww, wh);
        for &o in occ.iter().rev() {
            show_window(o, SW_SHOWNOACTIVATE);
        }
        match grab {
            Some(im) if !capture_is_blank(&im) => finalize_capture(&app, im, app_name.clone()),
            _ => {
                set_snip_overlay_hidden(&app, false);
                Err("blocked".to_string())
            }
        }
    }
    #[cfg(not(windows))]
    {
        let img = origin
            .and_then(|(ox, oy)| crop_frozen(&state, 0, wx - ox, wy - oy, ww as u32, wh as u32))
            .filter(|im| !capture_is_blank(im));
        match img {
            Some(im) => finalize_capture(&app, im, app_name.clone()),
            None => Err("blocked".to_string()),
        }
    }
}

// Open the project's GitHub page in the default browser (app logo → repo,
// version label → releases). The URLs are hardcoded — the webview can never open
// an arbitrary address.
#[tauri::command]
fn open_repo(page: Option<String>) -> Result<(), String> {
    let url = match page.as_deref() {
        Some("releases") => "https://github.com/wbgcoding/Prompt-Saver/releases",
        _ => "https://github.com/wbgcoding/Prompt-Saver",
    };
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", "start", "", url]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

// Scale the whole main-window UI with the browser's native zoom (like Ctrl +/-).
// Native zoom keeps event coordinates and getBoundingClientRect in one coordinate
// space, so — unlike a CSS `zoom` on <body> — it never desyncs popup positioning
// or grid hit-testing. Clamped to the slider's 50–300% range.
#[tauri::command]
fn set_ui_zoom(app: AppHandle, factor: f64) -> Result<(), String> {
    let f = factor.clamp(0.5, 3.0);
    if let Some(win) = app.get_webview_window("main") {
        win.set_zoom(f).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------- Updates (GitHub releases) ----------

const UPDATE_API: &str = "https://api.github.com/repos/wbgcoding/Prompt-Saver/releases/latest";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const UPDATE_MAX_BYTES: u64 = 100 * 1024 * 1024;

// Spawn a child process without flashing a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize, Clone)]
struct UpdateInfo {
    available: bool,
    version: String,
    url: String,
    // Release changelog (GitHub release body); empty when none was published.
    notes: String,
    // True when this version is on the user's skip list (manual check only).
    skipped: bool,
}

// Latest release tag, installer asset URL and changelog body. None on any
// failure (offline, private repo, rate limit) — checks never disturb the app.
fn fetch_latest() -> Option<(String, String, String)> {
    let body = ureq::get(UPDATE_API)
        .set("User-Agent", "PromptSaver")
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .ok()?
        .into_string()
        .ok()?;
    let json: serde_json::Value = serde_json::from_str(&body).ok()?;
    let tag = json["tag_name"].as_str()?.trim_start_matches('v').to_string();
    let url = json["assets"].as_array()?.iter().find_map(|a| {
        let name = a["name"].as_str()?;
        if name.ends_with("-setup.exe") {
            a["browser_download_url"].as_str().map(String::from)
        } else {
            None
        }
    })?;
    let notes = json["body"].as_str().unwrap_or("").trim().to_string();
    Some((tag, url, notes))
}

fn version_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    // Compare component-wise, zero-padding the shorter version, so "1.9" and
    // "1.9.0" rank equal instead of one being treated as newer.
    let (a, b) = (parse(latest), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

fn updater_check() -> Option<UpdateInfo> {
    let (version, url, notes) = fetch_latest()?;
    version_newer(&version, APP_VERSION).then_some(UpdateInfo {
        available: true,
        version,
        url,
        notes,
        skipped: false,
    })
}

#[tauri::command]
fn set_bars(app: AppHandle, state: State<Db>, header: bool, composer: bool) {
    let mut store = lock(&state);
    store.settings.show_header = header;
    store.settings.show_composer = composer;
    save_settings(&app, &store.settings);
}

#[tauri::command]
fn set_auto_update(app: AppHandle, state: State<Db>, enabled: bool) {
    let mut store = lock(&state);
    store.settings.auto_update = enabled;
    save_settings(&app, &store.settings);
}

#[tauri::command]
fn app_version() -> String {
    APP_VERSION.to_string()
}

#[tauri::command]
async fn check_update(state: State<'_, Db>) -> Result<UpdateInfo, String> {
    match fetch_latest() {
        Some((version, url, notes)) => {
            let available = version_newer(&version, APP_VERSION);
            let skipped = available && lock(&state).settings.skipped_versions.contains(&version);
            Ok(UpdateInfo {
                available,
                version: if available { version } else { APP_VERSION.to_string() },
                url: if available { url } else { String::new() },
                notes: if available { notes } else { String::new() },
                skipped,
            })
        }
        None => Err("update check failed".to_string()),
    }
}

// Toggle an expert feature flag (enabled = feature on).
#[tauri::command]
fn set_ui_flag(app: AppHandle, state: State<Db>, key: String, enabled: bool) {
    let is_capture = key == CAPTURE_FLAG_KEY;
    let is_clip = key == "clipWatcher"; // F21
    {
        let mut store = lock(&state);
        store.settings.ui_flags.insert(key, enabled);
        save_settings(&app, &store.settings);
    }
    // Capture exclusion applies live to every open window. Never hold the store
    // lock across window API calls (deadlock risk).
    if is_capture {
        for (_label, win) in app.webview_windows() {
            set_capture_exclusion(&win, enabled);
        }
    }
    // F21: start/stop the clipboard listener as the toggle flips (no leaked handle).
    #[cfg(windows)]
    if is_clip {
        if enabled {
            clipwatch::start(app.clone());
        } else {
            clipwatch::stop();
        }
    }
}

// Set an expert numeric value (CSS var / behaviour tweak).
#[tauri::command]
fn set_ui_value(app: AppHandle, state: State<Db>, key: String, value: f64) {
    let mut store = lock(&state);
    store.settings.ui_values.insert(key, value);
    save_settings(&app, &store.settings);
}

// Set an expert string option (e.g. the copy-feedback font key).
#[tauri::command]
fn set_ui_text(app: AppHandle, state: State<Db>, key: String, value: String) {
    let mut store = lock(&state);
    store.settings.ui_texts.insert(key, value);
    save_settings(&app, &store.settings);
}

// Clear all expert overrides back to the shipped defaults.
#[tauri::command]
fn reset_expert(app: AppHandle, state: State<Db>) {
    {
        let mut store = lock(&state);
        store.settings.ui_flags.clear();
        store.settings.ui_values.clear();
        store.settings.ui_texts.clear();
        save_settings(&app, &store.settings);
    }
    // Cleared flags revert capture exclusion to its default (on) — re-assert it
    // on every open window so the change is immediate.
    for (_label, win) in app.webview_windows() {
        set_capture_exclusion(&win, true);
    }
}

// Add a version to the skip list — it will not be offered again.
#[tauri::command]
fn skip_version(app: AppHandle, state: State<Db>, version: String) {
    let mut store = lock(&state);
    if !store.settings.skipped_versions.contains(&version) {
        store.settings.skipped_versions.push(version);
        save_settings(&app, &store.settings);
    }
}

// Strictly pin an update download URL to `<repo>/releases/download/<tag>/<asset>.exe`.
// A bare `starts_with` prefix check is not enough: the URL crate (and Windows) treat
// `..` and `\` as path separators, so `.../releases/download/../../attacker/...` would
// pass a prefix check yet resolve to a different repo. Reject every escape vector and
// require exactly the two trailing path segments (tag / asset), no query or fragment.
fn valid_update_url(url: &str) -> bool {
    const PREFIX: &str = "https://github.com/wbgcoding/Prompt-Saver/releases/download/";
    let rest = match url.strip_prefix(PREFIX) {
        Some(r) => r,
        None => return false,
    };
    let low = rest.to_ascii_lowercase();
    if rest.contains("..") || rest.contains('\\') || rest.contains("//")
        || rest.contains(':') || rest.contains('@') || rest.contains('?')
        || rest.contains('#') || low.contains("%2e") || low.contains("%2f")
        || low.contains("%5c")
    {
        return false;
    }
    let mut segs = rest.splitn(3, '/');
    let tag = segs.next().unwrap_or("");
    let asset = segs.next().unwrap_or("");
    let extra = segs.next();
    !tag.is_empty() && !asset.is_empty() && extra.is_none() && low.ends_with(".exe")
}

// Download the installer to %TEMP%, run it fully silent (/S), restart the
// app afterwards and quit so the installer can replace the binaries.
#[tauri::command]
async fn install_update(app: AppHandle, url: String) -> Result<(), String> {
    // Only our own release assets under the exact pinned repo path — not any github.com URL.
    if !valid_update_url(&url) {
        return Err("invalid update source".to_string());
    }
    let resp = ureq::get(&url)
        .set("User-Agent", "PromptSaver")
        .timeout(std::time::Duration::from_secs(300))
        .call()
        .map_err(|e| format!("download: {}", e))?;
    let mut bytes = Vec::new();
    use std::io::Read;
    resp.into_reader()
        .take(UPDATE_MAX_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read: {}", e))?;
    // A file that exactly hit the cap was almost certainly truncated — never run a
    // partial installer.
    if bytes.len() as u64 >= UPDATE_MAX_BYTES {
        return Err("installer too large / truncated".to_string());
    }
    // Sanity-check the download is actually a Windows executable (PE "MZ" header)
    // before we ever run it — a corruption/wrong-format guard. NOTE: this is not
    // cryptographic authentication; integrity today rests on TLS to the pinned repo
    // path. Publishing a per-release SHA-256 (or code-signing) and verifying it here
    // is the next hardening step.
    if bytes.len() < 2 || &bytes[0..2] != b"MZ" {
        return Err("downloaded file is not a valid installer".to_string());
    }
    // Integrity (Option A): verify the SHA-256 against the hash published beside the
    // installer in the release (`<installer>.sha256`, same pinned path). A PRESENT hash
    // that mismatches always aborts; a release that ships no hash file proceeds (so the
    // updater still works for any release that predates the hash convention).
    if let Ok(resp) = ureq::get(&format!("{}.sha256", url))
        .set("User-Agent", "PromptSaver")
        .timeout(std::time::Duration::from_secs(30))
        .call()
    {
        if let Ok(text) = resp.into_string() {
            let want = text.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
            if want.len() == 64 && want.chars().all(|c| c.is_ascii_hexdigit()) {
                use sha2::{Digest, Sha256};
                let got = format!("{:x}", Sha256::digest(&bytes));
                if got != want {
                    return Err("update hash mismatch — download rejected".to_string());
                }
            }
        }
    }
    // Stage into a fresh, uniquely-named temp subdir so a local attacker can't pre-plant
    // the installer/script at a predictable path (TOCTOU).
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stage = std::env::temp_dir().join(format!("prompt-saver-update-{}-{}", std::process::id(), stamp));
    fs::create_dir_all(&stage).map_err(|e| format!("stage dir: {}", e))?;
    let installer = stage.join("setup.exe");
    fs::write(&installer, &bytes).map_err(|e| format!("save installer: {}", e))?;

    // Helper script: silent install, relaunch the app, delete the installer + itself.
    let app_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = stage.join("update.cmd");
    let content = format!(
        "@echo off\r\n\"{}\" /S\r\nstart \"\" \"{}\"\r\ndel \"{}\"\r\ndel \"%~f0\"\r\n",
        installer.display(),
        app_exe.display(),
        installer.display()
    );
    fs::write(&script, content).map_err(|e| format!("save script: {}", e))?;

    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", &script.to_string_lossy()]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn().map_err(|e| format!("start installer: {}", e))?;

    // Exit slightly delayed so this command's reply still reaches the UI.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(800));
        if let Some(state) = app.try_state::<Db>() {
            let store = state.lock().unwrap_or_else(|e| e.into_inner());
            save_settings(&app, &store.settings);
        }
        app.exit(0);
    });
    Ok(())
}

// Called by the frontend once the first render + text fit is complete.
#[tauri::command]
fn show_main_window(app: AppHandle) {
    if std::env::args().any(|a| a == "--minimized") {
        return;
    }
    if let Some(w) = app.get_webview_window("main") {
        if !w.is_visible().unwrap_or(false) {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

// "Edit prompt" from a pill: bring up the main window and open its edit modal.
#[tauri::command]
async fn edit_prompt_request(app: AppHandle, id: String) -> Result<(), String> {
    show_main(&app);
    let _ = app.emit("edit-prompt", id);
    Ok(())
}

// ---------- Background / autostart ----------

// Add/remove the app in the per-user Windows autostart registry key.
#[cfg(windows)]
fn apply_autostart(enabled: bool, minimized: bool) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::RegKey;
    let run = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE,
        )
        .map_err(|e| e.to_string())?;
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let flag = if minimized { " --minimized" } else { "" };
        run.set_value(AUTOSTART_KEY, &format!("\"{}\"{}", exe.display(), flag))
            .map_err(|e| e.to_string())
    } else {
        match run.delete_value(AUTOSTART_KEY) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(not(windows))]
fn apply_autostart(_enabled: bool, _minimized: bool) -> Result<(), String> {
    Err("autostart is only supported on Windows".to_string())
}

#[tauri::command]
fn set_minimize_on_close(app: AppHandle, state: State<Db>, enabled: bool) {
    let mut store = lock(&state);
    store.settings.minimize_to_tray = enabled;
    save_settings(&app, &store.settings);
}

#[tauri::command]
fn set_always_on_top(app: AppHandle, state: State<Db>, enabled: bool) {
    {
        let mut store = lock(&state);
        store.settings.always_on_top = enabled;
        save_settings(&app, &store.settings);
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_always_on_top(enabled);
    }
}

#[tauri::command]
fn set_autostart(app: AppHandle, state: State<Db>, enabled: bool) -> Result<bool, String> {
    let minimized = lock(&state).settings.start_minimized;
    apply_autostart(enabled, minimized)?;
    let mut store = lock(&state);
    store.settings.autostart = enabled;
    save_settings(&app, &store.settings);
    Ok(enabled)
}

#[tauri::command]
fn set_start_minimized(app: AppHandle, state: State<Db>, enabled: bool) -> Result<bool, String> {
    let autostart = {
        let mut store = lock(&state);
        store.settings.start_minimized = enabled;
        save_settings(&app, &store.settings);
        store.settings.autostart
    };
    if autostart {
        apply_autostart(true, enabled)?;
    }
    Ok(enabled)
}

// ---------- Import / export ----------

fn csv_cell(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

// Position lines: one per view+grid-size the prompt is placed in.
fn csv_row(cells: &[&str]) -> String {
    cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(";")
}

// Export the prompt library only (incl. images/files) — never app settings,
// views or grid layout, so importing elsewhere can't clobber the user's setup.
fn to_csv(prompts: &[Prompt]) -> String {
    let head = [
        "name", "text", "positions", "color", "font", "size", "file", "icon", "show", "copy",
        "image", "caption", "capsize",
    ];
    let mut rows = vec![csv_row(&head)];
    for p in prompts {
        rows.push(csv_row(&[
            &p.name,
            &p.text,
            "", // positions omitted (prompts-only export)
            &p.color,
            &p.font,
            &p.font_size.to_string(),
            &p.file_path,
            &p.icon_path,
            if p.show_image { "1" } else { "0" },
            if p.copy_image { "1" } else { "0" },
            &p.image,
            &p.caption,
            &p.caption_size.to_string(),
        ]));
    }
    rows.join("\r\n")
}

fn to_txt(prompts: &[Prompt]) -> String {
    let blocks: Vec<String> = prompts.iter().map(|p| {
        let mut block = format!("### {}\n{}", p.name, p.text);
        if !p.color.is_empty() {
            block.push_str(&format!("\n@color {}", p.color));
        }
        // Per-tile style: "@style <font-key|-> <size>" ("-" = default font).
        if !p.font.is_empty() || p.font_size > 0 {
            let font = if p.font.is_empty() { "-" } else { &p.font };
            block.push_str(&format!("\n@style {} {}", font, p.font_size));
        }
        if !p.file_path.is_empty() {
            block.push_str(&format!("\n@file {}", p.file_path));
        }
        if !p.icon_path.is_empty() {
            block.push_str(&format!("\n@icon {}", p.icon_path));
        }
        if !p.caption.is_empty() {
            block.push_str(&format!("\n@caption {}", p.caption));
        }
        if p.caption_size > 0 {
            block.push_str(&format!("\n@capsize {}", p.caption_size));
        }
        if p.show_image || p.copy_image {
            block.push_str(&format!(
                "\n@flags {} {}",
                p.show_image as u8, p.copy_image as u8
            ));
        }
        if !p.image.is_empty() {
            block.push_str(&format!("\n@imagedata {}", p.image));
        }
        block
    }).collect();
    blocks.join("\n\n---\n\n")
}

// Async: building the export (base64 images) + writing it stays off the UI thread.
#[tauri::command]
async fn export_prompts(app: AppHandle, state: State<'_, Db>, format: String, ids: Option<Vec<String>>) -> Result<usize, String> {
    let (content, count) = {
        let store = lock(&state);
        // F8: an explicit id list exports only the selection; otherwise everything.
        let selected: Vec<Prompt> = match &ids {
            Some(list) if !list.is_empty() => {
                let set: std::collections::HashSet<&str> = list.iter().map(|s| s.as_str()).collect();
                store.prompts.iter().filter(|p| set.contains(p.id.as_str())).cloned().collect()
            }
            _ => store.prompts.clone(),
        };
        let content = match format.as_str() {
            "csv" => to_csv(&selected),
            "txt" => to_txt(&selected),
            _ => return Err(format!("Unsupported format: {}", format)),
        };
        (content, selected.len())
    };
    let file = file_dialog(&app)
        .set_file_name(format!("prompts.{}", format))
        .add_filter(format.to_uppercase(), &[format.as_str()])
        .save_file();
    match file {
        Some(path) => {
            fs::write(&path, content).map_err(|e| e.to_string())?;
            Ok(count)
        }
        None => Err("canceled".to_string()),
    }
}

// Minimal CSV reader for our own export format (quoted fields, ';' delimiter).
fn parse_csv(content: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                _ => field.push(c),
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ';' => row.push(std::mem::take(&mut field)),
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

struct ImportedPrompt {
    name: String,
    text: String,
    color: String,
    font: String,
    font_size: u32,
    file_path: String,
    icon_path: String,
    caption: String,
    caption_size: u32,
    show_image: bool,
    copy_image: bool,
    image: String,
    // Parsed from legacy exports so their @positions block is consumed cleanly;
    // grid layout is no longer imported (prompts-only import).
    #[allow(dead_code)]
    positions: Vec<String>,
}

// 0 = follow settings, 1 = auto-fit, otherwise a fixed pixel size.
fn clamp_font_size(size: u32) -> u32 {
    if size <= 1 { size } else { size.clamp(10, 40) }
}

fn clamp_caption_size(size: u32) -> u32 {
    clamp_font_size(size)
}

#[derive(Default)]
struct ImportData {
    language: Option<String>,
    theme: Option<String>,
    tile_font: Option<String>,
    tile_size: Option<u32>,
    minimize_to_tray: Option<bool>,
    auto_update: Option<bool>,
    show_header: Option<bool>,
    show_composer: Option<bool>,
    view_defs: Vec<String>,
    prompts: Vec<ImportedPrompt>,
}

// "key=value" lines from an @settings block.
fn parse_settings_lines(lines: &str, data: &mut ImportData) {
    let flag = |v: &str| Some(v.trim() == "1");
    for line in lines.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("language=") {
            data.language = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("theme=") {
            data.theme = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("tile_font=") {
            data.tile_font = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("tile_size=") {
            data.tile_size = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("minimize_to_tray=") {
            data.minimize_to_tray = flag(v);
        } else if let Some(v) = line.strip_prefix("auto_update=") {
            data.auto_update = flag(v);
        } else if let Some(v) = line.strip_prefix("show_header=") {
            data.show_header = flag(v);
        } else if let Some(v) = line.strip_prefix("show_composer=") {
            data.show_composer = flag(v);
        }
    }
}

fn parse_txt(content: &str) -> ImportData {
    let mut data = ImportData::default();
    for block in content.replace("\r\n", "\n").split("\n\n---\n\n") {
        let block = block.trim();
        if let Some(s) = block.strip_prefix("@settings") {
            parse_settings_lines(s, &mut data);
            continue;
        }
        if let Some(defs) = block.strip_prefix("@views") {
            data.view_defs.extend(
                defs.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()),
            );
            continue;
        }
        let Some(rest) = block.strip_prefix("### ") else { continue };
        let (name, body) = rest.split_once('\n').unwrap_or((rest, ""));
        let (body, positions) = match body.split_once("\n@positions\n") {
            Some((t, pos)) => (
                t,
                pos.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect(),
            ),
            None => (body, Vec::new()),
        };
        // Strip trailing metadata lines in reverse write order:
        // @imagedata, @flags, @icon, @file, @style, @color.
        let (body, image) = match body.rsplit_once("\n@imagedata ") {
            Some((t, v)) => (t.to_string(), v.trim().to_string()),
            None => (body.to_string(), String::new()),
        };
        let (body, show_image, copy_image) = match body.rsplit_once("\n@flags ") {
            Some((t, v)) => {
                let mut parts = v.split_whitespace();
                let show = parts.next() == Some("1");
                let copy = parts.next() == Some("1");
                (t.to_string(), show, copy)
            }
            None => (body, false, false),
        };
        let (body, caption_size) = match body.rsplit_once("\n@capsize ") {
            Some((t, v)) => (t.to_string(), v.trim().parse().unwrap_or(0)),
            None => (body, 0),
        };
        let (body, caption) = match body.rsplit_once("\n@caption ") {
            Some((t, v)) => (t.to_string(), v.trim().to_string()),
            None => (body, String::new()),
        };
        let (body, icon_path) = match body.rsplit_once("\n@icon ") {
            Some((t, v)) => (t.to_string(), v.trim().to_string()),
            None => (body, String::new()),
        };
        let (body, file_path) = match body.rsplit_once("\n@file ") {
            Some((t, f)) => (t.to_string(), f.trim().to_string()),
            None => (body, String::new()),
        };
        let (body, font, font_size) = match body.rsplit_once("\n@style ") {
            Some((t, s)) => {
                let mut parts = s.split_whitespace();
                let font = parts.next().filter(|f| *f != "-").unwrap_or("").to_string();
                let size = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                (t.to_string(), font, size)
            }
            None => (body, String::new(), 0),
        };
        let (text, color) = match body.rsplit_once("\n@color ") {
            Some((t, c)) => (t.to_string(), c.trim().to_string()),
            None => (body, String::new()),
        };
        data.prompts.push(ImportedPrompt {
            name: name.trim().to_string(),
            text,
            color,
            font,
            font_size,
            file_path,
            icon_path,
            caption,
            caption_size,
            show_image,
            copy_image,
            image,
            positions,
        });
    }
    data
}

// Create/update views from "Name|CxR" definition lines.

// Our own CSV export format back into import data.
fn parse_csv_data(content: &str) -> ImportData {
    let mut data = ImportData::default();
    for row in parse_csv(content).into_iter().skip(1) {
            if row.is_empty() || row[0].trim().is_empty() {
                continue;
            }
            match row[0].trim() {
                "@settings" => {
                    if let Some(s) = row.get(1) {
                        parse_settings_lines(s, &mut data);
                    }
                    continue;
                }
                "@views" => {
                    if let Some(d) = row.get(1) {
                        data.view_defs.extend(
                            d.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()),
                        );
                    }
                    continue;
                }
                _ => {}
            }
            if row.len() < 2 {
                continue;
            }
            data.prompts.push(ImportedPrompt {
                name: row[0].trim().to_string(),
                text: row[1].clone(),
                color: row.get(3).map(|c| c.trim().to_string()).unwrap_or_default(),
                font: row.get(4).map(|f| f.trim().to_string()).unwrap_or_default(),
                font_size: row
                    .get(5)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0),
                file_path: row.get(6).map(|f| f.trim().to_string()).unwrap_or_default(),
                icon_path: row.get(7).map(|f| f.trim().to_string()).unwrap_or_default(),
                show_image: row.get(8).map(|v| v.trim() == "1").unwrap_or(false),
                copy_image: row.get(9).map(|v| v.trim() == "1").unwrap_or(false),
                image: row.get(10).map(|v| v.trim().to_string()).unwrap_or_default(),
                caption: row.get(11).map(|v| v.trim().to_string()).unwrap_or_default(),
                caption_size: row.get(12).and_then(|v| v.trim().parse().ok()).unwrap_or(0),
                positions: row
                    .get(2)
                    .map(|p| {
                        p.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
                    })
                    .unwrap_or_default(),
            });
    }
    data
}

// Async: parsing + persisting an import stays off the UI thread.
#[tauri::command]
async fn import_prompts(app: AppHandle, state: State<'_, Db>) -> Result<usize, String> {
    let file = file_dialog(&app)
        .add_filter("Prompts", &["csv", "txt"])
        .pick_file();
    let Some(path) = file else {
        return Err("canceled".to_string());
    };
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let is_csv = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase() == "csv")
        .unwrap_or(false);
    let data = if is_csv { parse_csv_data(&content) } else { parse_txt(&content) };

    if data.prompts.is_empty() {
        return Err("no prompts found".to_string());
    }

    // Prompts-only import: never touch app settings, views or layout, even if an
    // older export file still carries those blocks (they are parsed but ignored).
    // Imported prompts are appended, but an entry identical to one already present
    // is skipped so the original is kept (no duplicate buttons).
    let mut store = lock(&state);
    let sig = |name: &str, text: &str, file: &str, image: &str, copy: bool| {
        format!("{name}\u{1}{text}\u{1}{file}\u{1}{image}\u{1}{copy}")
    };
    let mut seen: std::collections::HashSet<String> = store
        .prompts
        .iter()
        .map(|p| sig(&p.name, &p.text, &p.file_path, &p.image, p.copy_image))
        .collect();
    let mut count = 0usize;
    for item in data.prompts {
        if !seen.insert(sig(&item.name, &item.text, &item.file_path, &item.image, item.copy_image)) {
            continue; // duplicate of an existing (or earlier imported) prompt
        }
        let prompt = Prompt {
            id: gen_id(),
            name: item.name,
            text: item.text,
            color: item.color,
            image: item.image,
            show_image: item.show_image,
            copy_image: item.copy_image,
            file_path: item.file_path,
            icon_path: item.icon_path,
            caption: item.caption,
            caption_size: clamp_caption_size(item.caption_size),
            font: item.font,
            font_size: clamp_font_size(item.font_size),
            favorite: false,
        };
        store.prompts.push(prompt);
        count += 1;
    }
    save_prompts(&app, &store);
    let pref = store.settings.theme.clone();
    drop(store);
    let effective = effective_theme(&app, &pref);
    apply_window_bg(&app, &effective);
    let _ = app.emit("theme-changed", effective);
    Ok(count)
}

// Full backup: prompts + settings as one JSON file ("export/import everything").
#[derive(Serialize, Deserialize)]
struct Backup {
    prompts: Vec<Prompt>,
    settings: Settings,
}

// ---------- Encrypted single-file backups (S7) ----------
// Format: b"PSB1" | mode(1) | [salt(16) if mode==1] | nonce(12) | AES-256-GCM ciphertext.
// mode 0 = fixed key (portable: any install restores it). mode 1 = user password
// (Argon2id(password + pepper, salt)). Old plaintext .json backups are still read.
const BACKUP_MAGIC: &[u8; 4] = b"PSB1";
// Fixed obfuscation key — MUST stay identical across all versions or old fixed-key
// backups become unreadable. Not a secret (open source); it only keeps backups from
// being plainly readable while staying restorable on any machine.
const BACKUP_FIXED_KEY: [u8; 32] = [
    0x8f, 0x2a, 0x41, 0xd7, 0x63, 0x1c, 0xb9, 0x05, 0xe4, 0x77, 0x38, 0xaa, 0x1e, 0x90, 0x6d, 0xc2,
    0x54, 0x0b, 0xf3, 0x82, 0x9c, 0x2d, 0x71, 0x48, 0xba, 0x66, 0x0f, 0xd5, 0x37, 0xe1, 0x59, 0x84,
];
// Appended to the user's password before the KDF to harden brute force ("extend by code").
const BACKUP_PEPPER: &[u8] = b"prompt-saver::backup::pepper::v1";
const BACKUP_PW_MAX: usize = 128; // password length ceiling (bounds KDF input)
const BACKUP_PW_KEY: &str = "backupPw"; // ui_texts slot (DPAPI-sealed via enc_str)

fn backup_derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    use argon2::Argon2;
    let mut input = Vec::with_capacity(password.len() + BACKUP_PEPPER.len());
    input.extend_from_slice(password.as_bytes());
    input.extend_from_slice(BACKUP_PEPPER);
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(&input, salt, &mut key)
        .map_err(|e| e.to_string())?;
    Ok(key)
}

// A backup is JSON, so deflating it before encryption shrinks the file to a
// fraction. Compression must happen BEFORE encryption — ciphertext does not
// compress. Modes 2/3 mark a deflated payload; 0/1 stay readable forever.
fn deflate(raw: &[u8]) -> Result<Vec<u8>, String> {
    use flate2::{write::DeflateEncoder, Compression};
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
    std::io::Write::write_all(&mut enc, raw).map_err(|e| e.to_string())?;
    enc.finish().map_err(|e| e.to_string())
}

fn inflate(packed: &[u8]) -> Result<Vec<u8>, String> {
    use flate2::read::DeflateDecoder;
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut DeflateDecoder::new(packed), &mut out)
        .map_err(|_| "corrupt backup".to_string())?;
    Ok(out)
}

fn backup_encrypt(plain: &[u8], password: Option<&str>) -> Result<Vec<u8>, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Key, Nonce};
    use rand::RngCore;
    let body = deflate(plain)?;
    let mut out = Vec::with_capacity(body.len() + 64);
    out.extend_from_slice(BACKUP_MAGIC);
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let key_bytes: [u8; 32] = match password {
        Some(pw) => {
            out.push(3); // password + deflate
            let mut salt = [0u8; 16];
            rand::rngs::OsRng.fill_bytes(&mut salt);
            let k = backup_derive_key(pw, &salt)?;
            out.extend_from_slice(&salt);
            k
        }
        None => {
            out.push(2); // fixed key + deflate
            BACKUP_FIXED_KEY
        }
    };
    out.extend_from_slice(&nonce);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), body.as_slice())
        .map_err(|_| "backup encryption failed".to_string())?;
    out.extend_from_slice(&ct);
    Ok(out)
}

// Ok(Some(json)) = decrypted; Ok(None) = not our format (caller tries plaintext JSON);
// Err("password-required") = a password-mode backup and no/failed key.
fn backup_decrypt(data: &[u8], password: Option<&str>) -> Result<Option<Vec<u8>>, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Key, Nonce};
    if data.len() < 5 || &data[0..4] != BACKUP_MAGIC {
        return Ok(None);
    }
    // 0/1 = uncompressed (pre-2.6.0 files), 2/3 = deflated payload.
    let packed = matches!(data[4], 2 | 3);
    let (key_bytes, rest): ([u8; 32], &[u8]) = match data[4] {
        0 | 2 => (BACKUP_FIXED_KEY, &data[5..]),
        1 | 3 => {
            if data.len() < 21 {
                return Err("corrupt backup".to_string());
            }
            let pw = password.ok_or_else(|| "password-required".to_string())?;
            (backup_derive_key(pw, &data[5..21])?, &data[21..])
        }
        _ => return Err("unknown backup format".to_string()),
    };
    if rest.len() < 12 {
        return Err("corrupt backup".to_string());
    }
    let (nonce, ct) = rest.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let body = cipher
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|_| {
            if password.is_some() {
                "wrong password or corrupt backup".to_string()
            } else {
                "password-required".to_string()
            }
        })?;
    Ok(Some(if packed { inflate(&body)? } else { body }))
}

// Store (DPAPI-sealed) or clear the optional backup password. Never returned in clear.
#[tauri::command]
async fn set_backup_password(app: AppHandle, state: State<'_, Db>, password: String) -> Result<(), String> {
    if password.len() > BACKUP_PW_MAX {
        return Err("password too long".to_string());
    }
    {
        let mut store = lock(&state);
        if password.is_empty() {
            store.settings.ui_texts.remove(BACKUP_PW_KEY);
        } else {
            store
                .settings
                .ui_texts
                .insert(BACKUP_PW_KEY.to_string(), enc_str(&password));
        }
        save_settings(&app, &store.settings);
    }
    Ok(())
}

#[tauri::command]
async fn has_backup_password(state: State<'_, Db>) -> Result<bool, String> {
    let store = lock(&state);
    Ok(store
        .settings
        .ui_texts
        .get(BACKUP_PW_KEY)
        .map(|v| !v.is_empty())
        .unwrap_or(false))
}

// The clear backup password for this machine (sealed → dec_str), if one is set.
fn stored_backup_password(store: &Store) -> Option<String> {
    store
        .settings
        .ui_texts
        .get(BACKUP_PW_KEY)
        .map(|v| dec_str(v))
        .filter(|s| !s.is_empty())
}

#[tauri::command]
async fn export_all(app: AppHandle, state: State<'_, Db>) -> Result<usize, String> {
    let (json, count, password) = {
        let store = lock(&state);
        let backup = Backup {
            prompts: store.prompts.clone(),
            settings: store.settings.clone(),
        };
        let json = serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())?;
        (json, store.prompts.len(), stored_backup_password(&store))
    };
    // Always encrypted: fixed key by default (restorable anywhere), user password if set.
    let blob = backup_encrypt(json.as_bytes(), password.as_deref())?;
    let file = file_dialog(&app)
        .set_file_name("prompt-saver-backup.psb")
        .add_filter("Prompt Saver backup", &["psb", "json"])
        .save_file();
    match file {
        Some(path) => {
            fs::write(&path, blob).map_err(|e| e.to_string())?;
            Ok(count)
        }
        None => Err("canceled".to_string()),
    }
}

// What a full import restored — the UI names the file in its confirmation.
#[derive(Serialize)]
struct ImportSummary {
    name: String,
    count: usize,
}

#[tauri::command]
async fn pick_backup_file(app: AppHandle) -> Option<String> {
    file_dialog(&app)
        .add_filter("Prompt Saver backup", &["psb", "json"])
        .pick_file()
        .map(|p| p.display().to_string())
}

// The file is picked separately so a wrong password can be retried on the SAME
// file instead of sending the user back through the picker.
#[tauri::command]
async fn import_all(
    app: AppHandle,
    state: State<'_, Db>,
    path: String,
    password: Option<String>,
) -> Result<ImportSummary, String> {
    let path = PathBuf::from(path);
    let raw = fs::read(&path).map_err(|e| e.to_string())?;
    // Try the caller-supplied password first, else the one stored for this machine.
    let pw = password.filter(|p| !p.is_empty()).or_else(|| {
        let store = lock(&state);
        stored_backup_password(&store)
    });
    // Encrypted (PSB1) or a legacy plaintext .json backup.
    let content = match backup_decrypt(&raw, pw.as_deref())? {
        Some(plain) => String::from_utf8(plain).map_err(|_| "corrupt backup".to_string())?,
        None => String::from_utf8(raw).map_err(|_| "not a Prompt Saver backup".to_string())?,
    };
    let backup: Backup =
        serde_json::from_str(&content).map_err(|_| "not a Prompt Saver backup".to_string())?;

    let (theme, hotkey, count) = {
        let mut store = lock(&state);
        store.prompts = backup.prompts;
        // A backup written by hand could miss ids; keep every prompt addressable.
        for p in store.prompts.iter_mut() {
            if p.id.is_empty() {
                p.id = gen_id();
            }
        }
        migrate_prompts(&mut store.prompts);
        store.settings = backup.settings;
        // migrate() fills defaults and guarantees at least one view — without it a
        // hand-edited backup with "views": [] makes active_view_mut index [0] panic
        // on the next mutation (and poisons the Db mutex).
        store.settings.migrate();
        store.prompts_locked = false; // a full import supersedes any decrypt lock
        save_prompts(&app, &store);
        save_settings(&app, &store.settings);
        (
            store.settings.theme.clone(),
            store.settings.hotkey.clone(),
            store.prompts.len(),
        )
    };
    // Re-apply the visible parts of the imported settings without a restart.
    let effective = effective_theme(&app, &theme);
    apply_window_bg(&app, &effective);
    let _ = app.emit("theme-changed", effective);
    {
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
        let gs = app.global_shortcut();
        let _ = gs.unregister_all();
        if let Ok(sc) = hotkey.trim().parse::<Shortcut>() {
            let _ = gs.register(sc);
        }
    }
    Ok(ImportSummary {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        count,
    })
}

// ---------- Window geometry ----------

fn point_on_monitor(monitors: &[tauri::Monitor], x: i32, y: i32) -> bool {
    monitors.iter().any(|m| {
        let p = m.position();
        let s = m.size();
        x >= p.x && x < p.x + s.width as i32 && y >= p.y && y < p.y + s.height as i32
    })
}

// NOTE: WindowGeom.width/height are LOGICAL pixels (DPI-independent) — a
// physically stored size grew by the monitor's scale factor on every start.
// Position stays physical (global desktop coordinates).

// Center a window of the given LOGICAL size on the primary monitor.
fn centered_on_primary(main: &tauri::WebviewWindow, width: u32, height: u32) -> WindowGeom {
    if let Some(m) = main.primary_monitor().ok().flatten() {
        let p = m.position();
        let s = m.size();
        let pw = (width as f64 * m.scale_factor()) as u32;
        let ph = (height as f64 * m.scale_factor()) as u32;
        let x = p.x + (s.width.saturating_sub(pw) / 2) as i32;
        let y = p.y + (s.height.saturating_sub(ph) / 2) as i32;
        return WindowGeom { x, y, width, height };
    }
    WindowGeom { x: 100, y: 100, width, height }
}

// First start: 50% of the primary monitor, centered. Afterwards the saved size
// is kept (capped at the monitor); if its monitor is gone, re-center.
fn resolve_geometry(main: &tauri::WebviewWindow, saved: Option<WindowGeom>) -> WindowGeom {
    // Hard cap: the window can never start larger than the primary monitor.
    let cap = main
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| {
            let s = m.size();
            (
                (s.width as f64 / m.scale_factor()) as u32,
                (s.height as f64 / m.scale_factor()) as u32,
            )
        })
        .unwrap_or((u32::MAX, u32::MAX));
    if let Some(mut g) = saved {
        if g.width > 0 && g.height > 0 {
            g.width = g.width.min(cap.0);
            g.height = g.height.min(cap.1);
            let monitors = main.available_monitors().unwrap_or_default();
            if point_on_monitor(&monitors, g.x + 40, g.y + 20) {
                return g;
            }
            return centered_on_primary(main, g.width, g.height);
        }
    }
    // First start: 50% x 50% of the primary screen (logical), centered.
    let (width, height) = (
        (cap.0 / 2).max(400).min(cap.0),
        (cap.1 / 2).max(300).min(cap.1),
    );
    centered_on_primary(main, width, height)
}

// Persist a partial geometry update in memory (flushed to disk on close).
fn update_geom<F: FnOnce(&mut WindowGeom)>(handle: &AppHandle, f: F) {
    if let Some(state) = handle.try_state::<Db>() {
        // try_lock, never block: this runs on the event-loop thread for every move/
        // resize tick. If the store is busy (e.g. a backup VACUUM holds the lock), skip
        // this update — the next move/resize event carries the current geometry anyway.
        let mut store = match state.try_lock() {
            Ok(g) => g,
            Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        let mut g = store.settings.window.unwrap_or(WindowGeom {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        f(&mut g);
        store.settings.window = Some(g);
    }
}

// Append one line to a local crash log so a hard failure on a user's machine
// leaves something diagnosable. Privacy: the message is only the panic location
// and our own static panic text (never prompt content or other user data), and
// the file is local — never uploaded. Rotates past 256 KB so it can't grow
// without bound. Best-effort: every step is ignored on failure.
fn append_crash_log(msg: &str) {
    let dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(DATA_FOLDER);
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("error.log");
    if fs::metadata(&path).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
        let _ = fs::remove_file(&path);
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[t={} v{}] {}", secs, env!("CARGO_PKG_VERSION"), msg);
    }
}

// ---------- App entry ----------

// WebView2 runtime is the only external requirement; offer the official
// installer if it is missing instead of failing with a cryptic error.
#[cfg(windows)]
fn ensure_webview2() -> bool {
    if tauri::webview_version().is_ok() {
        return true;
    }
    let (title, msg) = webview2_texts(resolve_lang("auto"));
    let answer = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title(title)
        .set_description(msg)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();
    if answer == rfd::MessageDialogResult::Yes {
        // Try a silent download + install of the official Evergreen runtime; if
        // that fails (offline/blocked), fall back to opening the download page.
        if install_webview2_runtime() {
            // The runtime exists now, but this process started without it —
            // relaunch so a fresh instance loads the webview.
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe).spawn();
            }
        } else {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", "", "https://go.microsoft.com/fwlink/p/?LinkId=2124703"])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
    }
    false
}

// Download the Microsoft Evergreen WebView2 bootstrapper to %TEMP% and run it.
// Returns true only if the installer reports success. Fully guarded: any failure
// falls back to the manual download link.
#[cfg(windows)]
fn install_webview2_runtime() -> bool {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    const BOOTSTRAPPER_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
    let resp = match ureq::get(BOOTSTRAPPER_URL)
        .set("User-Agent", "PromptSaver")
        .timeout(std::time::Duration::from_secs(300))
        .call()
    {
        Ok(r) => r,
        Err(_) => return false,
    };
    let mut bytes = Vec::new();
    if resp
        .into_reader()
        .take(8 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.is_empty()
    {
        return false;
    }
    // Stage into a unique temp subdir so a local attacker can't pre-plant the
    // installer at a predictable path (TOCTOU) — matching install_update.
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("prompt-saver-wv2-{}-{}", std::process::id(), stamp));
    if fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let path = dir.join("MicrosoftEdgeWebview2Setup.exe");
    if fs::write(&path, &bytes).is_err() {
        return false;
    }
    std::process::Command::new(&path)
        .args(["/install"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn run() {
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

    tauri::Builder::default()
        // Only one instance app-wide (keyed by app identifier, independent of
        // the exe location). A second launch closes itself and focuses the first.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        // Optional global hotkey: pressing it summons the window + quick-launcher.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        summon_launcher(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let store = load_store(&handle);

            let to_restore: Vec<Prompt> = store
                .prompts
                .iter()
                .filter(|p| store.settings.floating.contains_key(&p.id))
                .cloned()
                .collect();
            let saved_geom = store.settings.window;
            let autostart = store.settings.autostart;
            let start_min = store.settings.start_minimized;
            let on_top = store.settings.always_on_top;
            let capture_excl = capture_excluded(&store.settings);
            let hotkey = store.settings.hotkey.clone();

            // F2 auto-backup config (read before the store is moved into state).
            let auto_backup = store.settings.ui_flags.get("autoBackup") != Some(&false);
            let backup_keep = store.settings.ui_values.get("backupKeep").copied().unwrap_or(10.0).clamp(1.0, 50.0) as usize;
            let backup_interval_h: u64 = store
                .settings
                .ui_values
                .get("backupIntervalH")
                .copied()
                .filter(|v| *v >= 1.0)
                .unwrap_or(24.0) as u64;
            let clip_on = store.settings.ui_flags.get("clipWatcher") == Some(&true); // F21, off by default
            let backup_gfs = gfs_from_settings(&store.settings); // F2 GFS retention

            app.manage(Mutex::new(store));
            app.manage(SnipState(Mutex::new(Vec::new())));
            app.manage(SnipPreview(Arc::new(Mutex::new(None))));

            // F19 auto-paste: start tracking the last external foreground window.
            #[cfg(windows)]
            autopaste::install_hook();

            // F21: start the clipboard watcher only if the user opted in.
            #[cfg(windows)]
            if clip_on {
                clipwatch::start(handle.clone());
            }

            // F2: on start, take a fresh backup if the newest one is older than the
            // interval. Off the UI thread so it never delays the first paint.
            if auto_backup {
                let h = handle.clone();
                std::thread::spawn(move || {
                    let interval = backup_interval_h.saturating_mul(3600);
                    if newest_backup_age(&h).map_or(true, |age| age >= interval) {
                        let _ = do_backup(&h, backup_keep, backup_gfs);
                    }
                });
            }

            // Re-arm the saved global hotkey (ignored if unset or invalid).
            if !hotkey.is_empty() {
                if let Ok(sc) = hotkey.parse::<tauri_plugin_global_shortcut::Shortcut>() {
                    use tauri_plugin_global_shortcut::GlobalShortcutExt;
                    let _ = app.global_shortcut().register(sc);
                }
            }

            // Launched by autostart with --minimized: stay in the tray.
            let start_hidden = std::env::args().any(|a| a == "--minimized");

            if let Some(main) = app.get_webview_window("main") {
                let geom = resolve_geometry(&main, saved_geom);
                let _ = main.set_size(tauri::LogicalSize::new(geom.width, geom.height));
                let _ = main.set_position(PhysicalPosition::new(geom.x, geom.y));
                set_capture_exclusion(&main, capture_excl);
                if on_top {
                    let _ = main.set_always_on_top(true);
                }
                {
                    let pref = handle
                        .try_state::<Db>()
                        .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).settings.theme.clone())
                        .unwrap_or_else(|| "system".to_string());
                    let eff = effective_theme(&handle, &pref);
                    apply_window_bg(&handle, &eff);
                }
                // The window is revealed by the frontend (show_main_window) once
                // the first layout pass is done — no visible text re-sizing.
                // Safety net: show after 1.5s even if the frontend never calls.
                if !start_hidden {
                    let h = handle.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        if let Some(w) = h.get_webview_window("main") {
                            // Show on uncertainty too — this is the safety net.
                            if !w.is_visible().unwrap_or(false) {
                                let _ = w.show();
                            }
                        }
                    });
                }
                if let Some(state) = handle.try_state::<Db>() {
                    state.lock().unwrap_or_else(|e| e.into_inner()).settings.window = Some(geom);
                }

                let handle2 = handle.clone();
                main.on_window_event(move |event| match event {
                    WindowEvent::Moved(p) => {
                        if p.x > -30000 && p.y > -30000 {
                            update_geom(&handle2, |g| {
                                g.x = p.x;
                                g.y = p.y;
                            });
                        }
                    }
                    WindowEvent::Resized(s) => {
                        if s.width > 0 && s.height > 0 {
                            // Store LOGICAL pixels — physical values re-scaled
                            // by the monitor factor on every start.
                            let scale = handle2
                                .get_webview_window("main")
                                .and_then(|w| w.scale_factor().ok())
                                .unwrap_or(1.0);
                            let logical = s.to_logical::<f64>(scale);
                            update_geom(&handle2, |g| {
                                g.width = logical.width.round() as u32;
                                g.height = logical.height.round() as u32;
                            });
                        }
                    }
                    WindowEvent::ThemeChanged(_) => {
                        if let Some(state) = handle2.try_state::<Db>() {
                            let pref = state
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .settings
                                .theme
                                .clone();
                            if pref == "system" {
                                let eff = effective_theme(&handle2, &pref);
                                apply_window_bg(&handle2, &eff);
                                let _ = handle2.emit("theme-changed", eff);
                            }
                        }
                    }
                    WindowEvent::CloseRequested { api, .. } => {
                        // Portable always closes fully; installed honours the tray setting.
                        let minimize = !is_portable()
                            && handle2
                                .try_state::<Db>()
                                .map(|s| {
                                    s.lock().unwrap_or_else(|e| e.into_inner())
                                        .settings
                                        .minimize_to_tray
                                })
                                .unwrap_or(false);
                        if minimize {
                            api.prevent_close();
                            if let Some(w) = handle2.get_webview_window("main") {
                                let _ = w.hide();
                            }
                        } else {
                            if let Some(state) = handle2.try_state::<Db>() {
                                let store = state.lock().unwrap_or_else(|e| e.into_inner());
                                save_settings(&handle2, &store.settings);
                            }
                            handle2.exit(0);
                        }
                    }
                    WindowEvent::Destroyed => {
                        if let Some(state) = handle2.try_state::<Db>() {
                            let store = state.lock().unwrap_or_else(|e| e.into_inner());
                            save_settings(&handle2, &store.settings);
                        }
                    }
                    _ => {}
                });
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
                .tooltip("Prompt Saver")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => {
                        if let Some(state) = app.try_state::<Db>() {
                            let store = state.lock().unwrap_or_else(|e| e.into_inner());
                            save_settings(app, &store.settings);
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Re-apply autostart so the registry entry tracks the exe location.
            if autostart {
                let _ = apply_autostart(true, start_min);
            }

            for prompt in &to_restore {
                open_floating(&handle, prompt);
            }

            // Pre-warm the snip overlay shortly after launch (off the startup hot
            // path) so even the FIRST screenshot opens instantly — the WebView2
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
            let h2 = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                loop {
                    let enabled = h2
                        .try_state::<Db>()
                        .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).settings.auto_update)
                        .unwrap_or(true);
                    if enabled {
                        if let Some(info) = updater_check() {
                            // Honour the skip list: a skipped version never
                            // pops up on its own, only on a manual check.
                            let skipped = h2
                                .try_state::<Db>()
                                .map(|s| {
                                    s.lock()
                                        .unwrap_or_else(|e| e.into_inner())
                                        .settings
                                        .skipped_versions
                                        .contains(&info.version)
                                })
                                .unwrap_or(false);
                            if !skipped {
                                let _ = h2.emit("update-available", info);
                            }
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_secs(24 * 60 * 60));
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_prompt,
            add_prompt,
            update_prompt,
            set_favorite,
            delete_prompt,
            batch_prompts,
            list_versions,
            restore_version,
            usage_stats,
            find_duplicates,
            ignore_dups,
            list_backups,
            create_backup,
            delete_backup,
            restore_backup,
            clip_inbox_list,
            clip_inbox_dismiss,
            clip_inbox_clear,
            open_repo,
            set_ui_zoom,
            delete_all_data,
            reset_settings,
            set_language,
            set_tile_style,
            set_layout,
            set_view_grid,
            add_view,
            rename_view,
            set_view_color,
            remap_colors,
            delete_view,
            set_active_view,
            get_settings,
            get_state,
            current_theme,
            set_theme,
            copy_prompt,
            copy_text,
            record_copy,
            clear_copy_history,
            recent_copies,
            toggle_floating,
            set_float_scale,
            resize_float_pill,
            resize_float_media,
            set_float_bounds,
            set_video_prefs,
            edit_prompt_request,
            show_main_window,
            app_version,
            check_update,
            install_update,
            set_auto_update,
            set_hotkey,
            set_bars,
            set_minimize_on_close,
            set_always_on_top,
            set_ui_flag,
            set_ui_value,
            set_ui_text,
            reset_expert,
            skip_version,
            open_snip,
            snip_background,
            snip_present,
            snip_should_begin,
            snip_windows,
            snip_cancel,
            capture_region,
            capture_window,
            pdf_preview,
            set_autostart,
            set_start_minimized,
            export_prompts,
            import_prompts,
            export_all,
            import_all,
            pick_backup_file,
            takeover_offer,
            takeover_apply,
            set_backup_password,
            has_backup_password,
            get_clipboard_image,
            get_clipboard_file_path,
            pick_file_path,
            load_image_file,
            missing_files,
            save_screenshot_now,
            save_screenshot_as,
            delete_screenshot_file,
            default_screenshot_dir,
            pick_folder,
            current_data_dir,
            set_data_dir,
            delete_data_dir,
            paste_into_previous
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_url_pin_rejects_escapes() {
        // The one legit shape is accepted.
        assert!(valid_update_url(
            "https://github.com/wbgcoding/Prompt-Saver/releases/download/v2.5.0/Prompt-Saver_2.5.0_x64-setup.exe"
        ));
        // Path-traversal to another repo, still github.com and still prefix-matching.
        assert!(!valid_update_url(
            "https://github.com/wbgcoding/Prompt-Saver/releases/download/../../attacker/repo/releases/download/v1/x-setup.exe"
        ));
        // Encoded dot-segment, backslash, extra scheme/authority, query, non-exe.
        assert!(!valid_update_url("https://github.com/wbgcoding/Prompt-Saver/releases/download/%2e%2e/x/y-setup.exe"));
        assert!(!valid_update_url("https://github.com/wbgcoding/Prompt-Saver/releases/download/v1\\x/y-setup.exe"));
        assert!(!valid_update_url("https://github.com/wbgcoding/Prompt-Saver/releases/download/v1/y-setup.exe?x=1"));
        assert!(!valid_update_url("https://github.com/wbgcoding/Prompt-Saver/releases/download/v1/y.txt"));
        assert!(!valid_update_url("http://github.com/wbgcoding/Prompt-Saver/releases/download/v1/y-setup.exe"));
        // Extra path segments beyond tag/asset.
        assert!(!valid_update_url("https://github.com/wbgcoding/Prompt-Saver/releases/download/v1/sub/y-setup.exe"));
    }

    #[test]
    fn store_crypt_roundtrip() {
        let secret = "prompt with sécret 🔐 {#{x}#}";
        assert_eq!(dec_str(&enc_str(secret)), secret);
        // Legacy plaintext (no prefix) passes through unchanged for migration.
        assert_eq!(dec_str("legacy plaintext"), "legacy plaintext");
    }

    #[test]
    fn history_retention_defaults_to_forever() {
        // Regression: the default used to be 7 days, which silently emptied the copy
        // history for anyone who copied less than weekly. Default is now keep-forever.
        let mut s = Settings::default();
        assert_eq!(history_max_age(&s), None, "no historyDays set => keep forever");
        s.ui_values.insert("historyDays".into(), 0.0);
        assert_eq!(history_max_age(&s), None, "0 => keep forever");
        s.ui_values.insert("historyDays".into(), 7.0);
        assert_eq!(history_max_age(&s), Some(7 * 86_400), "explicit days still prune");
    }

    #[test]
    fn backup_fixed_key_roundtrip_is_portable() {
        // Default backups carry no password: any install must be able to restore them.
        let plain = br#"{"prompts":[],"settings":{}}"#;
        let blob = backup_encrypt(plain, None).expect("encrypt");
        assert_eq!(&blob[0..4], BACKUP_MAGIC, "header magic");
        assert_eq!(blob[4], 2, "mode 2 = fixed key + compressed");
        assert_ne!(&blob[5..], &plain[..], "payload is not stored in the clear");
        let out = backup_decrypt(&blob, None).expect("decrypt").expect("psb format");
        assert_eq!(out, plain);
    }

    #[test]
    fn backup_password_roundtrip_and_wrong_password_fails() {
        let plain = b"secret prompts";
        let blob = backup_encrypt(plain, Some("correct horse")).expect("encrypt");
        assert_eq!(blob[4], 3, "mode 3 = password + compressed");
        let out = backup_decrypt(&blob, Some("correct horse")).expect("decrypt").expect("psb");
        assert_eq!(out, plain);
        assert!(backup_decrypt(&blob, Some("wrong")).is_err(), "wrong password rejected");
        assert!(backup_decrypt(&blob, None).is_err(), "missing password rejected");
        // The fixed key must NOT open a password-protected backup.
        assert!(backup_decrypt(&blob, Some("")).is_err());
    }

    #[test]
    fn backups_are_compressed_and_old_ones_still_open() {
        // Repetitive JSON is what a real backup looks like; it must shrink a lot.
        let plain = br#"{"prompts":[{"id":"a","name":"Review","text":"Review the code."}]}"#
            .repeat(40);
        let blob = backup_encrypt(&plain, None).expect("encrypt");
        assert_eq!(blob[4], 2, "mode 2 = fixed key + compressed");
        assert!(blob.len() * 4 < plain.len(), "compressed to well under a quarter");
        assert_eq!(backup_decrypt(&blob, None).expect("decrypt").expect("psb"), plain);

        // An uncompressed mode-0 file (written before 2.6.0) must still restore.
        let mut legacy = Vec::from(&BACKUP_MAGIC[..]);
        legacy.push(0);
        {
            use aes_gcm::aead::{Aead, KeyInit};
            use aes_gcm::{Aes256Gcm, Key, Nonce};
            let nonce = [7u8; 12];
            legacy.extend_from_slice(&nonce);
            let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&BACKUP_FIXED_KEY));
            legacy.extend_from_slice(&cipher.encrypt(Nonce::from_slice(&nonce), &plain[..]).unwrap());
        }
        assert_eq!(backup_decrypt(&legacy, None).expect("decrypt").expect("psb"), plain);
    }

    #[test]
    fn password_errors_stay_recognisable_for_the_ui() {
        // The restore flow decides whether to show the password prompt by matching
        // "password" in the error text. Rewording these would silently disable it.
        let blob = backup_encrypt(b"x", Some("pw")).expect("encrypt");
        let missing = backup_decrypt(&blob, None).unwrap_err();
        let wrong = backup_decrypt(&blob, Some("nope")).unwrap_err();
        assert_eq!(missing, "password-required");
        for msg in [&missing, &wrong] {
            assert!(msg.to_lowercase().contains("password"), "must mention password: {msg}");
        }
        // A backup without a password must NOT trigger the prompt.
        let fixed = backup_encrypt(b"x", None).expect("encrypt");
        assert!(backup_decrypt(&fixed, None).is_ok(), "fixed-key backup opens silently");
    }

    #[test]
    fn reset_settings_leaves_a_usable_state() {
        // reset_settings replaces the settings with Settings::default() + migrate().
        // If that produced no view, the first grid mutation would index views[0] and
        // panic (the same trap import_all guards against), leaving a dead app.
        let mut s = Settings::default();
        s.migrate();
        assert!(!s.views.is_empty(), "reset must leave at least one view");
        assert!(
            s.views.iter().any(|v| v.id == s.active_view),
            "active_view must point at an existing view"
        );
        assert_eq!(s.views[0].cols, default_cols());
        assert_eq!(s.views[0].rows, default_rows());
        assert_eq!(s.theme, default_theme(), "theme back to default");
        assert!(s.ui_flags.is_empty() && s.ui_values.is_empty(), "expert tweaks cleared");
    }

    #[test]
    fn reset_settings_keeps_the_backup_password() {
        // Mirrors what reset_settings does with the store's settings. The sealed
        // backup password must survive: without it every password-protected
        // backup on disk would be undecryptable.
        let mut old = Settings::default();
        old.theme = "gradient".into();
        old.ui_texts.insert(BACKUP_PW_KEY.to_string(), "sealed-blob".into());
        old.ui_texts.insert("screenshotDir".to_string(), "C:/shots".into());

        let backup_pw = old.ui_texts.remove(BACKUP_PW_KEY);
        let mut fresh = Settings::default();
        fresh.migrate();
        if let Some(pw) = backup_pw {
            fresh.ui_texts.insert(BACKUP_PW_KEY.to_string(), pw);
        }

        assert_eq!(fresh.ui_texts.get(BACKUP_PW_KEY).map(String::as_str), Some("sealed-blob"));
        assert!(!fresh.ui_texts.contains_key("screenshotDir"), "other texts are reset");
        assert_eq!(fresh.theme, default_theme());
    }

    #[test]
    fn delete_all_data_clears_data_tables_only() {
        // The data-only wipe must empty every derived table but touch no settings.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL);
             CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
             INSERT INTO prompts VALUES('a', 0, '{}');
             INSERT INTO meta VALUES('settings', '{\"theme\":\"ocean\"}');
             INSERT INTO copy_log VALUES(1, 'a');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('a', 1, 'n', 't');
             INSERT INTO clip_inbox(ts, text) VALUES(1, 'clip');",
        )
        .unwrap();
        // Exactly the statements delete_all_data issues.
        for table in ["prompt_versions", "copy_log", "clip_inbox"] {
            conn.execute(&format!("DELETE FROM {table}"), []).unwrap();
        }
        conn.execute("DELETE FROM prompts", []).unwrap();
        let count = |t: &str| -> i64 {
            conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap()
        };
        assert_eq!(count("prompts"), 0);
        assert_eq!(count("copy_log"), 0);
        assert_eq!(count("prompt_versions"), 0);
        assert_eq!(count("clip_inbox"), 0);
        assert_eq!(count("meta"), 1, "settings must survive a data-only wipe");
    }

    #[test]
    fn backup_decrypt_passes_through_legacy_plaintext() {
        // Old .json backups have no PSB1 header — the caller falls back to plain JSON.
        assert!(backup_decrypt(br#"{"prompts":[]}"#, None).expect("no error").is_none());
    }

    #[test]
    fn copy_log_caps_groups_and_prunes() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX idx_copy_log_ts ON copy_log(ts);",
        )
        .unwrap();
        // Cap 3: only the three newest events survive, newest-first by insertion.
        for i in 0..5u64 {
            db_append_copy(&conn, &CopyEntry { id: format!("p{i}"), ts: 100 + i }, 3, None).unwrap();
        }
        let recent: Vec<String> = db_recent_copies(&conn, 10, false).into_iter().map(|e| e.id).collect();
        assert_eq!(recent, ["p4", "p3", "p2"]);
        // Grouped: a repeated id collapses to one row at its most recent position.
        db_append_copy(&conn, &CopyEntry { id: "p4".into(), ts: 200 }, 3, None).unwrap();
        let grouped = db_recent_copies(&conn, 10, true);
        assert_eq!(grouped[0].id, "p4");
        assert_eq!(grouped.iter().filter(|e| e.id == "p4").count(), 1);
        // Retention cutoff drops entries older than the window.
        db_append_copy(&conn, &CopyEntry { id: "old".into(), ts: 50 }, 100, Some(150)).unwrap();
        assert!(db_recent_copies(&conn, 100, false).iter().all(|e| e.id != "old"));
        // Gapped rowids (e.g. after an age-prune) must never let the cap be exceeded.
        conn.execute("DELETE FROM copy_log", []).unwrap();
        for i in 0..10u64 {
            db_append_copy(&conn, &CopyEntry { id: format!("g{i}"), ts: 0 }, 5, None).unwrap();
        }
        conn.execute("DELETE FROM copy_log WHERE id IN ('g7','g8')", []).unwrap();
        for i in 10..15u64 {
            db_append_copy(&conn, &CopyEntry { id: format!("g{i}"), ts: 0 }, 5, None).unwrap();
        }
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM copy_log", [], |r| r.get(0)).unwrap();
        assert!(n <= 5, "cap exceeded after gaps: {n}");
    }

    fn sample_prompts() -> Vec<Prompt> {
        vec![
            Prompt {
                id: "p1".into(),
                name: "Mail".into(),
                text: "Hello\nWorld".into(),
                color: "#ef4444".into(),
                image: "data:image/png;base64,QUJD".into(),
                show_image: true,
                copy_image: true,
                file_path: String::new(),
                icon_path: String::new(),
                caption: String::new(),
                caption_size: 0,
                font: "mono".into(),
                font_size: 24,
                favorite: false,
            },
            Prompt {
                id: "p2".into(),
                name: "Doc".into(),
                text: "Doc".into(),
                color: String::new(),
                image: String::new(),
                show_image: true,
                copy_image: false,
                file_path: "C:\\tmp\\report.pdf".into(),
                icon_path: "C:\\tmp\\clip.mp4".into(),
                caption: "Mein Untertitel".into(),
                caption_size: 18,
                font: String::new(),
                font_size: 1,
                favorite: false,
            },
        ]
    }

    // Settings JSON from an older version: unknown/removed fields (copy_history,
    // a future field) are ignored and every missing field defaults — never a total
    // parse failure that would wipe the user's settings.
    #[test]
    fn old_settings_json_migrates() {
        let json = r#"{
            "theme":"dark","language":"de","minimize_to_tray":true,
            "copy_history":["a","b"],"ui_flags":{"floating":false},
            "tile_size":18,"removed_future_field":42
        }"#;
        let s: Settings = serde_json::from_str(json).expect("old settings must still parse");
        assert_eq!(s.theme, "dark");
        assert_eq!(s.language, "de");
        assert!(s.minimize_to_tray);
        assert_eq!(s.ui_flags.get("floating"), Some(&false));
        assert_eq!(s.tile_size, 18);
        assert!(s.copy_log.is_empty()); // new field defaults
        assert!(s.usage.is_empty());
        assert!(s.auto_update); // missing field -> default_on
    }

    // Prompt JSON from an older version: removed field (favorite) ignored, missing
    // new fields default; the prompt still loads.
    #[test]
    fn old_prompt_json_migrates() {
        let json = r#"{"id":"x1","name":"Old","text":"hi","favorite":true,"show_image":true}"#;
        let p: Prompt = serde_json::from_str(json).expect("old prompt must still parse");
        assert_eq!(p.id, "x1");
        assert_eq!(p.name, "Old");
        assert!(p.show_image);
        assert_eq!(p.color, ""); // missing -> default
        assert_eq!(p.caption_size, 0);
    }

    #[test]
    fn txt_roundtrip_keeps_every_field() {
        let out = to_txt(&sample_prompts());
        let data = parse_txt(&out);
        assert_eq!(data.prompts.len(), 2);
        let p1 = &data.prompts[0];
        assert_eq!(p1.name, "Mail");
        assert_eq!(p1.text, "Hello\nWorld");
        assert_eq!(p1.color, "#ef4444");
        assert_eq!(p1.font, "mono");
        assert_eq!(p1.font_size, 24);
        assert_eq!(p1.image, "data:image/png;base64,QUJD");
        assert!(p1.show_image && p1.copy_image);
        let p2 = &data.prompts[1];
        assert_eq!(p2.font, "");
        assert_eq!(p2.font_size, 1);
        assert_eq!(p2.file_path, "C:\\tmp\\report.pdf");
        assert_eq!(p2.icon_path, "C:\\tmp\\clip.mp4");
        assert_eq!(p2.caption, "Mein Untertitel");
        assert_eq!(p2.caption_size, 18);
        assert!(p2.show_image && !p2.copy_image);
    }

    #[test]
    fn csv_roundtrip_keeps_every_field() {
        let out = to_csv(&sample_prompts());
        let rows = parse_csv(&out);
        // header + 2 prompts (no settings/views blocks in a prompts-only export)
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1][0], "Mail");
        assert_eq!(rows[1][4], "mono");
        assert_eq!(rows[1][5], "24");
        assert_eq!(rows[1][8], "1");
        assert_eq!(rows[1][9], "1");
        assert_eq!(rows[1][10], "data:image/png;base64,QUJD");
        assert_eq!(rows[2][6], "C:\\tmp\\report.pdf");
        assert_eq!(rows[2][7], "C:\\tmp\\clip.mp4");
        assert_eq!(rows[2][11], "Mein Untertitel");
        assert_eq!(rows[2][12], "18");
    }

    #[test]
    fn csv_export_feeds_import_parser_losslessly() {
        let out = to_csv(&sample_prompts());
        let data = parse_csv_data(&out);
        assert_eq!(data.prompts.len(), 2);
        let p1 = &data.prompts[0];
        assert!(p1.show_image && p1.copy_image);
        assert_eq!(p1.image, "data:image/png;base64,QUJD");
        assert_eq!(p1.text, "Hello\nWorld");
        let p2 = &data.prompts[1];
        assert_eq!(p2.file_path, "C:\\tmp\\report.pdf");
        assert_eq!(p2.icon_path, "C:\\tmp\\clip.mp4");
        assert_eq!(p2.caption, "Mein Untertitel");
        assert_eq!(p2.caption_size, 18);
    }

    #[test]
    fn old_export_with_settings_imports_prompts_only() {
        // A legacy file that still carries @settings/@views must import its prompts
        // and silently ignore the settings blocks (no panic, no bogus prompt).
        let txt = "@settings\nlanguage=de\ntheme=midnight\n\n---\n\n@views\nv|View\n\n---\n\n### Old\nSome text";
        let data = parse_txt(txt);
        assert_eq!(data.prompts.len(), 1);
        assert_eq!(data.prompts[0].name, "Old");
    }

    #[test]
    fn old_exports_without_style_still_parse() {
        let txt = "### Old\nSome text\n@color #123456";
        let data = parse_txt(txt);
        assert_eq!(data.prompts.len(), 1);
        assert_eq!(data.prompts[0].text, "Some text");
        assert_eq!(data.prompts[0].color, "#123456");
        assert_eq!(data.prompts[0].font, "");
        assert_eq!(data.prompts[0].font_size, 0);
        assert_eq!(data.prompts[0].file_path, "");
    }

    #[test]
    fn version_compare_pads_components() {
        assert!(version_newer("1.9.0", "1.8.9"));
        assert!(version_newer("2.0", "1.9.9"));
        assert!(!version_newer("1.9", "1.9.0")); // equal once padded
        assert!(!version_newer("1.9.0", "1.9"));
        assert!(!version_newer("1.8.0", "1.9.0"));
        assert!(version_newer("1.10.0", "1.9.0")); // numeric, not lexical
    }

    #[test]
    fn snip_preview_rotates_and_cleans() {
        let p1 = write_snip_preview(b"first").expect("write1");
        assert!(std::path::Path::new(&p1).exists());
        let p2 = write_snip_preview(b"second").expect("write2");
        assert_ne!(p1, p2);
        assert!(!std::path::Path::new(&p1).exists(), "previous preview deleted");
        assert!(std::path::Path::new(&p2).exists());
        remove_snip_preview();
        assert!(!std::path::Path::new(&p2).exists(), "cleanup removes file");
    }

    #[test]
    fn base64_roundtrip_and_reject() {
        let data = b"\x00\x01\x02\xff\xfe hello";
        assert_eq!(base64_decode(&base64_encode(data)), data);
        assert!(base64_decode("not valid base64!@#").is_empty());
    }

    #[test]
    fn font_size_clamped() {
        assert_eq!(clamp_font_size(0), 0);
        assert_eq!(clamp_font_size(1), 1);
        assert_eq!(clamp_font_size(8), 10);
        assert_eq!(clamp_font_size(99), 40);
    }
}
