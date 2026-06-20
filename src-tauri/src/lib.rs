// Prompt Saver backend (Tauri v2). Local SQLite storage, clipboard, import/export,
// multiple views, frameless floating quick-copy windows. No network, 100% offline.

use image::imageops::FilterType;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
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
    // Recently copied prompts (most recent first, with timestamp) + copy counts.
    #[serde(default)]
    copy_log: Vec<CopyEntry>,
    #[serde(default)]
    usage: HashMap<String, u32>,
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
            auto_update: true,
            skipped_versions: Vec::new(),
            ui_flags: HashMap::new(),
            ui_values: HashMap::new(),
            ui_texts: HashMap::new(),
            copy_log: Vec::new(),
            usage: HashMap::new(),
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
const GRID_MAX: u32 = 100;
const MAX_VIEWS: usize = 100;
const FLOAT_W: f64 = 360.0;
const FLOAT_H: f64 = 80.0; // flat pill shape, clearly wider than tall
const FLOAT_IMG: f64 = 400.0; // square box for image pills: S 300 / M 400 / L 560
const AUTOSTART_KEY: &str = "PromptSaver";

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
            self.active_view = self.views[0].id.clone();
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
}

type Db = Mutex<Store>;

// ---------- Paths + persistence ----------

fn data_dir(app: &AppHandle) -> PathBuf {
    // Never panic: fall back to a temp dir if the platform path is unavailable.
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("prompt-saver"));
    let _ = fs::create_dir_all(&dir);
    dir
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &PathBuf) -> T {
    match fs::read_to_string(path) {
        Ok(raw) if !raw.trim().is_empty() => serde_json::from_str(&raw).unwrap_or_default(),
        _ => T::default(),
    }
}

// Atomic write: temp file then rename, so a crash never truncates data.
fn write_json<T: Serialize>(path: &PathBuf, data: &T) {
    if let Ok(json) = serde_json::to_string_pretty(data) {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
}

// ---------- SQLite store ----------
// Prompts live one-per-row (ordered); settings are a single JSON row. The DB
// is the source of truth; the legacy JSON files are imported once on upgrade
// and otherwise only used as a fallback if the DB cannot be opened.

fn db_conn(app: &AppHandle) -> Option<Connection> {
    let conn = Connection::open(data_dir(app).join("data.db")).ok()?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);",
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
            let data = serde_json::to_string(p).unwrap_or_default();
            stmt.execute(params![p.id, i as i64, data])?;
        }
    }
    tx.commit()
}

fn db_write_settings(conn: &Connection, settings: &Settings) -> rusqlite::Result<()> {
    let json = serde_json::to_string(settings).unwrap_or_default();
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('settings', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        params![json],
    )?;
    Ok(())
}

fn db_load(conn: &Connection) -> (Vec<Prompt>, Option<Settings>) {
    let mut prompts = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT data FROM prompts ORDER BY ord") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for data in rows.flatten() {
                if let Ok(p) = serde_json::from_str::<Prompt>(&data) {
                    prompts.push(p);
                }
            }
        }
    }
    let settings = conn
        .query_row("SELECT value FROM meta WHERE key='settings'", [], |r| {
            r.get::<_, String>(0)
        })
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok());
    (prompts, settings)
}

fn save_prompts(app: &AppHandle, store: &Store) {
    if let Some(mut conn) = db_conn(app) {
        if db_write_prompts(&mut conn, &store.prompts).is_ok() {
            return;
        }
    }
    write_json(&data_dir(app).join("prompts.json"), &store.prompts);
}

fn save_settings(app: &AppHandle, settings: &Settings) {
    if let Some(conn) = db_conn(app) {
        if db_write_settings(&conn, settings).is_ok() {
            return;
        }
    }
    write_json(&data_dir(app).join("settings.json"), settings);
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
    if let Some(conn) = db_conn(app) {
        let (mut prompts, settings_opt) = db_load(&conn);
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
        prune_history(&mut settings);
        migrate_prompts(&mut prompts);
        return Store { prompts, settings };
    }
    // Fallback: DB unavailable → read the JSON files directly.
    let mut settings: Settings = read_json(&dir.join("settings.json"));
    settings.migrate();
    prune_history(&mut settings);
    let mut prompts: Vec<Prompt> = read_json(&dir.join("prompts.json"));
    migrate_prompts(&mut prompts);
    Store { prompts, settings }
}

fn gen_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("p{}", nanos)
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
    let (saved, scale, count) = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        (
            store.settings.floating.get(&prompt.id).copied(),
            float_scale_of(&store.settings, &prompt.id),
            store.settings.floating.len(),
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
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
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

fn copy_image_to_clipboard(data_url: &str) -> bool {
    let b64 = data_url.trim_start_matches("data:image/png;base64,");
    let bytes = base64_decode(b64);
    if bytes.is_empty() { return false; }
    let img = match image::load_from_memory(&bytes) {
        Ok(img) => img.to_rgba8(),
        Err(_) => return false,
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
    let img = image::open(&path).ok()?;
    let result = scale_and_encode(img);
    if result.is_empty() { None } else { Some(result) }
}

// Locate the pdfium library shipped next to the exe (installed build) or in the
// project/target dir during development.
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
    candidates.into_iter().find(|p| p.exists())
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
    let gone = |path: &str| !path.is_empty() && !std::path::Path::new(path).exists();
    lock(&state)
        .prompts
        .iter()
        .filter(|p| gone(&p.file_path) || gone(&p.icon_path))
        .map(|p| p.id.clone())
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
) -> Result<Prompt, String> {
    let prompt = Prompt {
        id: gen_id(),
        name,
        text,
        color,
        image: image.unwrap_or_default(),
        show_image: show_image.unwrap_or(false),
        copy_image: copy_image.unwrap_or(false),
        file_path: file_path.unwrap_or_default(),
        icon_path: icon_path.unwrap_or_default(),
        caption: caption.unwrap_or_default(),
        caption_size: clamp_caption_size(caption_size.unwrap_or(0)),
        font: font.unwrap_or_default(),
        font_size: clamp_font_size(font_size.unwrap_or(0)),
    };
    {
        let mut store = lock(&state);
        let view = store.settings.active_view_mut();
        if let Some(cell) = first_free_cell(view) {
            let key = grid_key(view.cols, view.rows);
            view.layouts.entry(key).or_default().insert(prompt.id.clone(), cell);
        }
        store.prompts.push(prompt.clone());
        save_prompts(&app, &store);
        save_settings(&app, &store.settings);
    }
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
) -> Result<Option<Prompt>, String> {
    let updated = {
        let mut store = lock(&state);
        let found = store.prompts.iter_mut().find(|p| p.id == id);
        match found {
            Some(p) => {
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
                let clone = p.clone();
                save_prompts(&app, &store);
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
        }
        changed
    };
    Ok(changed)
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
    let _ = apply_autostart(false, false);
    {
        let mut store = lock(&state);
        store.prompts.clear();
        store.settings = Settings::default();
        store.settings.migrate();
        save_prompts(&app, &store);
        save_settings(&app, &store.settings);
    }
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
        Some(p) => arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(p.text))
            .is_ok(),
        None => false,
    })
}

// Put arbitrary text on the clipboard — used after filling prompt placeholders.
#[tauri::command]
async fn copy_text(text: String) -> bool {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .is_ok()
}

// Hard ceiling for the copy-history length; the live limit is the expert value
// historyMax (default 50).
const COPY_HISTORY_MAX: usize = 200;

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
// (days; default 7). 0 or negative means keep forever.
fn history_max_age(settings: &Settings) -> Option<u64> {
    let days = settings.ui_values.get("historyDays").copied().unwrap_or(7.0);
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
    let mut store = lock(&state);
    // Respect the privacy toggle (expert menu): off = don't track.
    if store.settings.ui_flags.get("copyHistory") == Some(&false) {
        return Ok(());
    }
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
    // History length is an expert value (default 50, ceiling COPY_HISTORY_MAX).
    let cap = store
        .settings
        .ui_values
        .get("historyMax")
        .copied()
        .unwrap_or(50.0)
        .clamp(0.0, COPY_HISTORY_MAX as f64) as usize;
    store.settings.copy_log.retain(|e| e.id != id);
    if cap > 0 {
        store.settings.copy_log.insert(0, CopyEntry { id, ts });
        store.settings.copy_log.truncate(cap);
    }
    prune_history(&mut store.settings);
    save_settings(&app, &store.settings);
    Ok(())
}

// Wipe copy history + usage counters (privacy / journal "clear").
#[tauri::command]
async fn clear_copy_history(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    let mut store = lock(&state);
    store.settings.copy_log.clear();
    store.settings.usage.clear();
    save_settings(&app, &store.settings);
    Ok(())
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

// Close every per-monitor overlay window and bring the floating pills back.
fn close_all_snip(app: &AppHandle) {
    for (label, win) in app.webview_windows() {
        if label.starts_with("snip-") {
            let _ = win.close();
        }
    }
    set_app_windows_hidden(app, false);
    remove_snip_preview();
}

// Hide (or restore) the app's own windows — the main window and floating pills —
// around a snip, so a screenshot never contains Prompt Saver itself (this also
// reveals whatever the main window was covering, e.g. Task Manager).
fn set_app_windows_hidden(app: &AppHandle, hidden: bool) {
    for (label, win) in app.webview_windows() {
        if label == "main" || label.starts_with("float-") {
            let _ = if hidden { win.hide() } else { win.show() };
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

#[derive(Serialize)]
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
    if w <= 0 || h <= 0 {
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
        // GDI returns BGRA; convert to RGBA and force opaque alpha.
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }
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

// Freeze the desktop and open a single overlay spanning all screens. The
// frontend maps the cursor by ratio against the displayed frozen image, so the
// selection is pixel-exact on any DPI mix AND can be dragged across monitors.
#[tauri::command]
async fn open_snip(app: AppHandle, state: State<'_, SnipState>) -> Result<(), String> {
    if let Some((_, w)) = app
        .webview_windows()
        .into_iter()
        .find(|(l, _)| l.starts_with("snip-"))
    {
        let _ = w.set_focus();
        return Ok(());
    }
    // Hide our own windows BEFORE freezing so they're never in the capture and
    // the target underneath (e.g. Task Manager behind the main window) is
    // revealed. A short settle lets the hide reach the screen first.
    set_app_windows_hidden(&app, true);
    tokio::time::sleep(std::time::Duration::from_millis(45)).await;
    let (canvas, min_x, min_y) = freeze_desktop()?;
    let (total_w, total_h) = canvas.dimensions();
    // Store the frozen desktop as the single entry; its origin is (min_x,min_y).
    *state.0.lock().unwrap_or_else(|e| e.into_inner()) = vec![MonitorCap {
        image: canvas,
        x: min_x,
        y: min_y,
        width: total_w,
        height: total_h,
    }];

    // One opaque overlay covering the whole virtual desktop (shows the frozen
    // stitched screenshot — far more reliable than a transparent surface).
    // Borderless (WS_POPUP) so the client area starts exactly at the virtual
    // desktop origin — no frame inset, no left gap. shadow(false) drops the DWM
    // shadow. Not resizable/maximizable/minimizable + no drag region in the page
    // => the user cannot move it; no snap-back handler needed (that handler
    // fought tao's own placement and caused the visible drift).
    let win = WebviewWindowBuilder::new(&app, "snip-0", WebviewUrl::App("snip.html".into()))
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
    // Physical placement = exact virtual-desktop coverage (size first, then
    // position, so the final op is the position a resize could otherwise nudge).
    let _ = win.set_size(PhysicalSize::new(total_w, total_h));
    let _ = win.set_position(PhysicalPosition::new(min_x, min_y));
    let _ = win.show();
    // Re-assert once after the window settles on its monitors (WM_DPICHANGED).
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    let _ = win.set_size(PhysicalSize::new(total_w, total_h));
    let _ = win.set_position(PhysicalPosition::new(min_x, min_y));
    let _ = win.set_focus();
    Ok(())
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

// The frozen monitor capture as a data URL, only for the overlay preview — JPEG
// (no alpha needed) encodes far faster than PNG, so the overlay opens quicker.
// The saved/copied crop still comes from the lossless in-memory image.
// Async: the full-frame clone + JPEG encode (tens of MB) must not run on the UI
// thread. The image is cloned out under the lock, then encoded lock-free.
#[tauri::command]
async fn snip_background(state: State<'_, SnipState>, index: usize) -> Result<Option<SnipBg>, String> {
    let (img, full_w, full_h) = {
        let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        match guard.get(index) {
            Some(c) => (c.image.clone(), c.width, c.height),
            None => return Ok(None),
        }
    };
    // Display copy only — the marked crop always comes from the full-res in-memory
    // image. The preview stays full-resolution (crisp) up to a large cap; only an
    // extreme multi-monitor span is downscaled (fast box filter) to keep the JPEG
    // sane. The old cost was a Triangle resize (~1s), not the resolution, so full
    // res here still encodes in well under half a second.
    const DISPLAY_MAX: u32 = 10240;
    let longest = full_w.max(full_h);
    let disp = if longest > DISPLAY_MAX {
        let r = DISPLAY_MAX as f32 / longest as f32;
        image::imageops::thumbnail(
            &img,
            ((full_w as f32 * r) as u32).max(1),
            ((full_h as f32 * r) as u32).max(1),
        )
    } else {
        img
    };
    let rgb = image::DynamicImage::ImageRgba8(disp).into_rgb8();
    let mut buf = std::io::Cursor::new(Vec::<u8>::new());
    {
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 80);
        if enc.encode_image(&rgb).is_err() {
            return Ok(None);
        }
    }
    // Prefer a temp file (loaded via the asset protocol); fall back to a base64
    // data URL only if the file write fails.
    let jpeg = buf.into_inner();
    let (src, is_file) = match write_snip_preview(&jpeg) {
        Some(path) => (path, true),
        None => (format!("data:image/jpeg;base64,{}", base64_encode(&jpeg)), false),
    };
    Ok(Some(SnipBg { src, is_file, width: full_w, height: full_h }))
}

#[tauri::command]
fn snip_cancel(app: AppHandle, state: State<SnipState>) {
    close_all_snip(&app);
    state.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

// Save a screenshot PNG into the user's Pictures\Screenshots folder.
fn save_screenshot(png: &[u8]) -> String {
    let base = std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let dir = base.join("Pictures").join("Screenshots");
    let _ = fs::create_dir_all(&dir);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("PromptSaver-{}.png", ts));
    if fs::write(&path, png).is_ok() {
        path.to_string_lossy().to_string()
    } else {
        String::new()
    }
}

// Encode, copy to clipboard, save a PNG, close the overlay and notify the UI.
fn finalize_capture(app: &AppHandle, crop: image::RgbaImage) -> Result<(), String> {
    let (cw, ch) = crop.dimensions();
    if cw == 0 || ch == 0 {
        return Err("empty region".to_string());
    }
    let mut buf = std::io::Cursor::new(Vec::<u8>::new());
    image::DynamicImage::ImageRgba8(crop.clone())
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let png = buf.into_inner();
    let data_url = format!("data:image/png;base64,{}", base64_encode(&png));

    let _ = arboard::Clipboard::new().and_then(|mut c| {
        c.set_image(arboard::ImageData {
            width: cw as usize,
            height: ch as usize,
            bytes: crop.into_raw().into(),
        })
    });
    let path = save_screenshot(&png);

    close_all_snip(app);
    app.state::<SnipState>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    let _ = app.emit("snip-captured", SnipResult { data_url, path });
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
    finalize_capture(&app, crop)
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
        return finalize_capture(&app, im);
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
                        return finalize_capture(&app, im);
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
            Some(im) if !capture_is_blank(&im) => finalize_capture(&app, im),
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
            Some(im) => finalize_capture(&app, im),
            None => Err("blocked".to_string()),
        }
    }
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
    version_newer(&version, APP_VERSION).then(|| UpdateInfo {
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
    let mut store = lock(&state);
    store.settings.ui_flags.insert(key, enabled);
    save_settings(&app, &store.settings);
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
    let mut store = lock(&state);
    store.settings.ui_flags.clear();
    store.settings.ui_values.clear();
    store.settings.ui_texts.clear();
    save_settings(&app, &store.settings);
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

// Download the installer to %TEMP%, run it fully silent (/S), restart the
// app afterwards and quit so the installer can replace the binaries.
#[tauri::command]
async fn install_update(app: AppHandle, url: String) -> Result<(), String> {
    // Only our own signed release assets — not any github.com URL.
    if !url.starts_with("https://github.com/wbgcoding/Prompt-Saver/releases/download/") {
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
    let installer = std::env::temp_dir().join("prompt-saver-setup.exe");
    fs::write(&installer, &bytes).map_err(|e| format!("save installer: {}", e))?;

    // Helper script: silent install, relaunch the app, clean up after itself.
    let app_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = std::env::temp_dir().join("prompt-saver-update.cmd");
    let content = format!(
        "@echo off\r\n\"{}\" /S\r\nstart \"\" \"{}\"\r\ndel \"%~f0\"\r\n",
        installer.display(),
        app_exe.display()
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
fn position_lines(settings: &Settings, id: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for view in &settings.views {
        for (key, layout) in &view.layouts {
            if let Some(cell) = layout.get(id) {
                lines.push(format!("{}|{}={},{}", view.name, key, cell[0], cell[1]));
            }
        }
    }
    lines.sort();
    lines
}

// View definitions ("Name|CxR"), so an import can rebuild all views exactly.
fn view_def_lines(settings: &Settings) -> Vec<String> {
    settings
        .views
        .iter()
        .map(|v| format!("{}|{}", v.name, grid_key(v.cols, v.rows)))
        .collect()
}

fn csv_row(cells: &[&str]) -> String {
    cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(";")
}

// Exported UI preferences (one key=value per line in the @settings block).
// Machine-specific options (autostart, window geometry) stay local.
fn settings_lines(s: &Settings) -> String {
    format!(
        "language={}\ntheme={}\ntile_font={}\ntile_size={}\nminimize_to_tray={}\nauto_update={}\nshow_header={}\nshow_composer={}",
        s.language,
        s.theme,
        s.tile_font,
        s.tile_size,
        s.minimize_to_tray as u8,
        s.auto_update as u8,
        s.show_header as u8,
        s.show_composer as u8
    )
}

fn to_csv(prompts: &[Prompt], settings: &Settings) -> String {
    let head = [
        "name", "text", "positions", "color", "font", "size", "file", "icon", "show", "copy",
        "image", "caption", "capsize",
    ];
    let pad = |a: &str, b: &str| {
        let mut cells = vec![a.to_string(), b.to_string()];
        cells.resize(head.len(), String::new());
        csv_row(&cells.iter().map(|c| c.as_str()).collect::<Vec<_>>())
    };
    let mut rows = vec![
        csv_row(&head),
        pad("@settings", &settings_lines(settings)),
        pad("@views", &view_def_lines(settings).join("\n")),
    ];
    for p in prompts {
        let positions = position_lines(settings, &p.id).join("\n");
        rows.push(csv_row(&[
            &p.name,
            &p.text,
            &positions,
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

fn to_txt(prompts: &[Prompt], settings: &Settings) -> String {
    let mut blocks = vec![
        format!("@settings\n{}", settings_lines(settings)),
        format!("@views\n{}", view_def_lines(settings).join("\n")),
    ];
    blocks.extend(prompts.iter().map(|p| {
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
        let positions = position_lines(settings, &p.id);
        if !positions.is_empty() {
            block.push_str("\n@positions\n");
            block.push_str(&positions.join("\n"));
        }
        block
    }));
    blocks.join("\n\n---\n\n")
}

// Async: building the export (base64 images) + writing it stays off the UI thread.
#[tauri::command]
async fn export_prompts(app: AppHandle, state: State<'_, Db>, format: String) -> Result<usize, String> {
    let (content, count) = {
        let store = lock(&state);
        let content = match format.as_str() {
            "csv" => to_csv(&store.prompts, &store.settings),
            "txt" => to_txt(&store.prompts, &store.settings),
            _ => return Err(format!("Unsupported format: {}", format)),
        };
        (content, store.prompts.len())
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
    positions: Vec<String>, // "ViewName|6x5=c,r"
}

// 0 = follow settings, 1 = auto-fit, otherwise a fixed pixel size.
fn clamp_font_size(size: u32) -> u32 {
    if size <= 1 { size } else { size.clamp(10, 40) }
}

// Caption: 0 = default size, 1 = auto-scale, otherwise fixed 10..40.
fn clamp_caption_size(size: u32) -> u32 {
    if size <= 1 { size } else { size.clamp(10, 40) }
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
fn apply_view_defs(settings: &mut Settings, defs: &[String]) {
    for line in defs {
        let Some((name, key)) = line.split_once('|') else { continue };
        let Some((c, r)) = key.split_once('x') else { continue };
        let (Ok(cols), Ok(rows)) = (c.trim().parse::<u32>(), r.trim().parse::<u32>()) else {
            continue;
        };
        let (cols, rows) = (cols.clamp(GRID_MIN, GRID_MAX), rows.clamp(GRID_MIN, GRID_MAX));
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        match settings.views.iter().position(|v| v.name == name) {
            Some(i) => {
                settings.views[i].cols = cols;
                settings.views[i].rows = rows;
            }
            None if settings.views.len() < MAX_VIEWS => {
                settings.views.push(View {
                    id: gen_id(),
                    name: name.to_string(),
                    cols,
                    rows,
                    layouts: HashMap::new(),
                    color: String::new(),
                });
            }
            None => {}
        }
    }
}

// Apply "ViewName|CxR=c,r" placement lines for a freshly imported prompt.
fn apply_positions(settings: &mut Settings, id: &str, positions: &[String]) {
    for line in positions {
        let Some((view_name, rest)) = line.split_once('|') else { continue };
        let Some((key, cell)) = rest.split_once('=') else { continue };
        let Some((c, r)) = cell.split_once(',') else { continue };
        let (Ok(col), Ok(row)) = (c.trim().parse::<u32>(), r.trim().parse::<u32>()) else {
            continue;
        };
        let Some((kc, kr)) = key.split_once('x') else { continue };
        let (Ok(kcols), Ok(krows)) = (kc.parse::<u32>(), kr.parse::<u32>()) else { continue };
        if col >= kcols || row >= krows || kcols > GRID_MAX || krows > GRID_MAX {
            continue;
        }

        // Find or create the target view (respecting the view limit).
        let view_index = match settings.views.iter().position(|v| v.name == view_name) {
            Some(i) => i,
            None if settings.views.len() < MAX_VIEWS => {
                settings.views.push(View {
                    id: gen_id(),
                    name: view_name.to_string(),
                    cols: kcols.clamp(GRID_MIN, GRID_MAX),
                    rows: krows.clamp(GRID_MIN, GRID_MAX),
                    layouts: HashMap::new(),
                    color: String::new(),
                });
                settings.views.len() - 1
            }
            None => continue,
        };

        let layout = settings.views[view_index]
            .layouts
            .entry(key.to_string())
            .or_default();
        // Keep existing tiles; only fill the cell if it is free.
        let occupied = layout.values().any(|v| *v == [col, row]);
        if !occupied {
            layout.insert(id.to_string(), [col, row]);
        }
    }
}

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

    if data.prompts.is_empty() && data.view_defs.is_empty() {
        return Err("no prompts found".to_string());
    }

    let mut store = lock(&state);
    if let Some(lang) = &data.language {
        store.settings.language = lang.clone();
    }
    if let Some(theme) = &data.theme {
        store.settings.theme = theme.clone();
    }
    if let Some(font) = &data.tile_font {
        store.settings.tile_font = font.clone();
    }
    if let Some(size) = data.tile_size {
        store.settings.tile_size = if size == 0 { 0 } else { size.clamp(10, 40) };
    }
    if let Some(v) = data.minimize_to_tray {
        store.settings.minimize_to_tray = v;
    }
    if let Some(v) = data.auto_update {
        store.settings.auto_update = v;
    }
    if let Some(v) = data.show_header {
        store.settings.show_header = v;
    }
    if let Some(v) = data.show_composer {
        store.settings.show_composer = v;
    }
    apply_view_defs(&mut store.settings, &data.view_defs);
    let count = data.prompts.len();
    for item in data.prompts {
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
        };
        apply_positions(&mut store.settings, &prompt.id, &item.positions);
        store.prompts.push(prompt);
    }
    save_prompts(&app, &store);
    save_settings(&app, &store.settings);
    let pref = store.settings.theme.clone();
    drop(store);
    // An imported theme applies immediately (window background + UI event).
    let effective = effective_theme(&app, &pref);
    apply_window_bg(&app, &effective);
    let _ = app.emit("theme-changed", effective);
    Ok(count)
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
        let mut store = state.lock().unwrap_or_else(|e| e.into_inner());
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
        // Official Evergreen bootstrapper download.
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", "https://go.microsoft.com/fwlink/p/?LinkId=2124703"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    false
}

pub fn run() {
    #[cfg(windows)]
    if !ensure_webview2() {
        return;
    }
    // Surface panics on stderr (visible in a dev console) without writing files.
    std::panic::set_hook(Box::new(|info| eprintln!("{}", info)));

    tauri::Builder::default()
        // Only one instance app-wide (keyed by app identifier, independent of
        // the exe location). A second launch closes itself and focuses the first.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
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

            app.manage(Mutex::new(store));
            app.manage(SnipState(Mutex::new(Vec::new())));

            // Launched by autostart with --minimized: stay in the tray.
            let start_hidden = std::env::args().any(|a| a == "--minimized");

            if let Some(main) = app.get_webview_window("main") {
                let geom = resolve_geometry(&main, saved_geom);
                let _ = main.set_size(tauri::LogicalSize::new(geom.width, geom.height));
                let _ = main.set_position(PhysicalPosition::new(geom.x, geom.y));
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
                        let minimize = handle2
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
            delete_prompt,
            delete_all_data,
            set_language,
            set_tile_style,
            set_layout,
            set_view_grid,
            add_view,
            rename_view,
            set_view_color,
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
            snip_windows,
            snip_cancel,
            capture_region,
            capture_window,
            pdf_preview,
            set_autostart,
            set_start_minimized,
            export_prompts,
            import_prompts,
            get_clipboard_image,
            get_clipboard_file_path,
            pick_file_path,
            load_image_file,
            missing_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

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
            },
        ]
    }

    fn sample_settings() -> Settings {
        let mut s = Settings::default();
        s.migrate();
        s
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
        let out = to_txt(&sample_prompts(), &sample_settings());
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
        let out = to_csv(&sample_prompts(), &sample_settings());
        let rows = parse_csv(&out);
        // header + @settings + @views + 2 prompts
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[3][0], "Mail");
        assert_eq!(rows[3][4], "mono");
        assert_eq!(rows[3][5], "24");
        assert_eq!(rows[3][8], "1");
        assert_eq!(rows[3][9], "1");
        assert_eq!(rows[3][10], "data:image/png;base64,QUJD");
        assert_eq!(rows[4][6], "C:\\tmp\\report.pdf");
        assert_eq!(rows[4][7], "C:\\tmp\\clip.mp4");
        assert_eq!(rows[4][11], "Mein Untertitel");
        assert_eq!(rows[4][12], "18");
    }

    #[test]
    fn csv_export_feeds_import_parser_losslessly() {
        let mut s = sample_settings();
        s.language = "de".into();
        s.theme = "coffee".into();
        let out = to_csv(&sample_prompts(), &s);
        let data = parse_csv_data(&out);
        assert_eq!(data.language.as_deref(), Some("de"));
        assert_eq!(data.theme.as_deref(), Some("coffee"));
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
    fn settings_block_roundtrip() {
        let mut s = sample_settings();
        s.language = "de".into();
        s.theme = "midnight".into();
        s.tile_font = "georgia".into();
        s.tile_size = 18;
        s.minimize_to_tray = true;
        s.auto_update = false;
        s.show_header = false;
        s.show_composer = true;
        let out = to_txt(&[], &s);
        let data = parse_txt(&out);
        assert_eq!(data.language.as_deref(), Some("de"));
        assert_eq!(data.theme.as_deref(), Some("midnight"));
        assert_eq!(data.tile_font.as_deref(), Some("georgia"));
        assert_eq!(data.tile_size, Some(18));
        assert_eq!(data.minimize_to_tray, Some(true));
        assert_eq!(data.auto_update, Some(false));
        assert_eq!(data.show_header, Some(false));
        assert_eq!(data.show_composer, Some(true));
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
