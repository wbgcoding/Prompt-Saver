// Prompt Saver backend (Tauri v2). Local JSON storage, clipboard, import/export,
// multiple views, frameless floating quick-copy windows. No network, 100% offline.

use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewUrl,
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
    #[serde(default = "default_on")]
    auto_update: bool,
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
            auto_update: true,
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
const GRID_MAX: u32 = 20;
const MAX_VIEWS: usize = 20;
const FLOAT_W: f64 = 360.0;
const FLOAT_H: f64 = 80.0; // flat pill shape, clearly wider than tall
const FLOAT_IMG: f64 = 400.0; // square box for image pills: S 300 / M 400 / L 560
const FLOAT_MENU_W: f64 = 230.0;
const FLOAT_MENU_H: f64 = 200.0;
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

fn save_prompts(app: &AppHandle, store: &Store) {
    write_json(&data_dir(app).join("prompts.json"), &store.prompts);
}

fn save_settings(app: &AppHandle, settings: &Settings) {
    write_json(&data_dir(app).join("settings.json"), settings);
}

fn load_store(app: &AppHandle) -> Store {
    let dir = data_dir(app);
    let mut settings: Settings = read_json(&dir.join("settings.json"));
    settings.migrate();
    let mut prompts: Vec<Prompt> = read_json(&dir.join("prompts.json"));
    // Migration: image prompts saved before copy_image existed copied the
    // image on click — keep that behaviour (name doubled as the text).
    for p in &mut prompts {
        if !p.image.is_empty() && !p.copy_image && (p.text.is_empty() || p.text == p.name) {
            p.copy_image = true;
        }
    }
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

// Append a line to the diagnostic log in %TEMP%.
fn log_debug(msg: &str) {
    let path = std::env::temp_dir().join("prompt-saver-panic.log");
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, format!("{}\n", msg).as_bytes()));
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
fn media_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    [".gif", ".mp4", ".webm", ".m4v", ".mov"].iter().any(|e| lower.ends_with(e))
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

    if let Err(e) = &win {
        log_debug(&format!("floating window create failed: {}", e));
    }
    if let Ok(win) = win {
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
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for c in bytes.chunks(4) {
        let v: Vec<u32> = c.iter()
            .filter_map(|&b| B64.iter().position(|&x| x == b).map(|i| i as u32))
            .collect();
        if v.len() >= 2 { out.push(((v[0] << 2) | (v[1] >> 4)) as u8); }
        if v.len() >= 3 { out.push(((v[1] << 4) | (v[2] >> 2)) as u8); }
        if v.len() >= 4 { out.push(((v[2] << 6) |  v[3]      ) as u8); }
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

#[tauri::command]
fn get_prompt(state: State<Db>, id: String) -> Option<Prompt> {
    lock(&state).prompts.iter().find(|p| p.id == id).cloned()
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

#[tauri::command]
fn add_prompt(
    app: AppHandle,
    state: State<Db>,
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
) -> Prompt {
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
    prompt
}

#[tauri::command]
fn update_prompt(
    app: AppHandle,
    state: State<Db>,
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
) -> Option<Prompt> {
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
    updated
}

#[tauri::command]
fn delete_prompt(app: AppHandle, state: State<Db>, id: String) -> bool {
    close_floating_window(&app, &id);
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
}

// Factory reset: wipe all prompts AND all settings (views, theme, window,
// behaviour, fonts) and remove the autostart registry entry.
#[tauri::command]
fn delete_all_data(app: AppHandle, state: State<Db>) {
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
    let mut store = lock(&state);
    store.prompts.clear();
    store.settings = Settings::default();
    store.settings.migrate();
    save_prompts(&app, &store);
    save_settings(&app, &store.settings);
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
#[tauri::command]
fn set_layout(app: AppHandle, state: State<Db>, layout: HashMap<String, [u32; 2]>) {
    let mut store = lock(&state);
    let view = store.settings.active_view_mut();
    let key = grid_key(view.cols, view.rows);
    view.layouts.insert(key, layout);
    save_settings(&app, &store.settings);
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
fn add_view(app: AppHandle, state: State<Db>, name: String) -> Result<Settings, String> {
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

#[tauri::command]
fn get_state(state: State<Db>) -> AppState {
    let store = lock(&state);
    AppState {
        prompts: store.prompts.clone(),
        settings: store.settings.clone(),
    }
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

#[tauri::command]
fn copy_prompt(state: State<Db>, id: String) -> bool {
    let prompt = {
        let store = lock(&state);
        store.prompts.iter().find(|p| p.id == id).cloned()
    };
    match prompt {
        Some(p) if p.copy_image && !p.image.is_empty() => copy_image_to_clipboard(&p.image),
        Some(p) if !p.file_path.is_empty() => set_clipboard_file(&p.file_path),
        Some(p) => arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(p.text))
            .is_ok(),
        None => false,
    }
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
    let scale = if scale.is_finite() { scale.clamp(0.5, 2.0) } else { 1.0 };
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

// Text pills grow with their label: the frontend measures the text and
// requests a matching window width (height stays the pill height).
#[tauri::command]
async fn resize_float_pill(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    width: f64,
) -> Result<(), String> {
    let scale = float_scale_of(&lock(&state).settings, &id);
    if let Some(win) = app.get_webview_window(&flabel(&id)) {
        let w = if width.is_finite() { width.clamp(135.0, 960.0) } else { FLOAT_W * scale };
        let _ = win.set_size(tauri::LogicalSize::new(w, FLOAT_H * scale));
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
            let w = width.clamp(60.0, 960.0);
            let h = height.clamp(60.0, 960.0);
            let _ = win.set_size(tauri::LogicalSize::new(w, h));
        }
    }
    Ok(())
}

// Menu open: the window grows to pill + menu so the pill stays visible and
// size changes preview live. width/height carry the pill's CURRENT box
// (text pills are wider than the default when their label is long).
#[tauri::command]
async fn resize_float_menu(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    open: bool,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<(), String> {
    let (scale, is_img) = {
        let store = lock(&state);
        (
            float_scale_of(&store.settings, &id),
            store.prompts.iter().find(|p| p.id == id).map(is_image_prompt).unwrap_or(false),
        )
    };
    if let Some(win) = app.get_webview_window(&flabel(&id)) {
        let (dw, dh) = pill_dims(is_img, scale);
        let pw = width.filter(|v| v.is_finite()).unwrap_or(dw).clamp(135.0, 960.0);
        let ph = height.filter(|v| v.is_finite()).unwrap_or(dh).clamp(40.0, 960.0);
        let (w, h) = if open { (pw.max(FLOAT_MENU_W), ph + FLOAT_MENU_H) } else { (pw, ph) };
        let _ = win.set_size(tauri::LogicalSize::new(w, h));
    }
    Ok(())
}

// Persist the per-prompt video player state (volume, mute, loop).
#[tauri::command]
fn set_video_prefs(app: AppHandle, state: State<Db>, id: String, volume: u32, muted: bool, looped: bool) {
    let mut store = lock(&state);
    store
        .settings
        .video_prefs
        .insert(id, VideoPrefs { volume: volume.min(100), muted, looped });
    save_settings(&app, &store.settings);
}

// ---------- Updates (GitHub releases) ----------

const UPDATE_API: &str = "https://api.github.com/repos/wbgcoding/Prompt-Saver/releases/latest";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const UPDATE_MAX_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Serialize, Clone)]
struct UpdateInfo {
    available: bool,
    version: String,
    url: String,
}

// Latest release tag + installer asset URL, None on any failure (offline,
// private repo, rate limit) — update checks must never disturb the app.
fn fetch_latest() -> Option<(String, String)> {
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
    Some((tag, url))
}

fn version_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    parse(latest) > parse(current)
}

fn updater_check() -> Option<UpdateInfo> {
    let (version, url) = fetch_latest()?;
    version_newer(&version, APP_VERSION).then(|| UpdateInfo {
        available: true,
        version,
        url,
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
async fn check_update() -> Result<UpdateInfo, String> {
    match fetch_latest() {
        Some((version, url)) => {
            let available = version_newer(&version, APP_VERSION);
            Ok(UpdateInfo {
                available,
                version: if available { version } else { APP_VERSION.to_string() },
                url: if available { url } else { String::new() },
            })
        }
        None => Err("update check failed".to_string()),
    }
}

// Download the installer to %TEMP%, run it fully silent (/S), restart the
// app afterwards and quit so the installer can replace the binaries.
#[tauri::command]
async fn install_update(app: AppHandle, url: String) -> Result<(), String> {
    if !url.starts_with("https://github.com/") {
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
        cmd.creation_flags(0x0800_0000); // no console window
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

#[tauri::command]
fn export_prompts(app: AppHandle, state: State<Db>, format: String) -> Result<usize, String> {
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

#[tauri::command]
fn import_prompts(app: AppHandle, state: State<Db>) -> Result<usize, String> {
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
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
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
    // Capture any panic (with location) to a log file for diagnosis.
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("{}\n", info);
        eprintln!("{}", msg);
        let path = std::env::temp_dir().join("prompt-saver-panic.log");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, msg.as_bytes()));
    }));

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

            app.manage(Mutex::new(store));

            // Launched by autostart with --minimized: stay in the tray.
            let start_hidden = std::env::args().any(|a| a == "--minimized");

            if let Some(main) = app.get_webview_window("main") {
                let geom = resolve_geometry(&main, saved_geom);
                let _ = main.set_size(tauri::LogicalSize::new(geom.width, geom.height));
                let _ = main.set_position(PhysicalPosition::new(geom.x, geom.y));
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
                            if !w.is_visible().unwrap_or(true) {
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
                            let _ = h2.emit("update-available", info);
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
            delete_view,
            set_active_view,
            get_settings,
            get_state,
            current_theme,
            set_theme,
            copy_prompt,
            toggle_floating,
            set_float_scale,
            resize_float_pill,
            resize_float_media,
            resize_float_menu,
            set_video_prefs,
            edit_prompt_request,
            show_main_window,
            app_version,
            check_update,
            install_update,
            set_auto_update,
            set_bars,
            set_minimize_on_close,
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
    fn font_size_clamped() {
        assert_eq!(clamp_font_size(0), 0);
        assert_eq!(clamp_font_size(1), 1);
        assert_eq!(clamp_font_size(8), 10);
        assert_eq!(clamp_font_size(99), 40);
    }
}
