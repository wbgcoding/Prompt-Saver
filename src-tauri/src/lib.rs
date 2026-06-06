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
                let _ = win.set_size(PhysicalSize::new(g.width, g.height));
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

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    #[serde(default = "default_theme")]
    theme: String,
    #[serde(default)]
    floating: HashMap<String, Pos>,
    // Per-floating-button size factor (1.0 = default).
    #[serde(default)]
    float_scale: HashMap<String, f64>,
    #[serde(default)]
    window: Option<WindowGeom>,
    #[serde(default)]
    minimize_to_tray: bool,
    #[serde(default)]
    autostart: bool,
    #[serde(default)]
    start_minimized: bool,
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
            window: None,
            minimize_to_tray: false,
            autostart: false,
            start_minimized: false,
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
const FLOAT_W: f64 = 160.0;
const FLOAT_H: f64 = 48.0;
const FLOAT_IMG: f64 = 180.0; // square box for image pills: S 135 / M 180 / L 252
const FLOAT_MENU_W: f64 = 220.0;
const FLOAT_MENU_H: f64 = 168.0;
const AUTOSTART_KEY: &str = "PromptSaver";

fn grid_key(cols: u32, rows: u32) -> String {
    format!("{}x{}", cols, rows)
}

fn is_german() -> bool {
    sys_locale::get_locale()
        .map(|l| l.to_lowercase().starts_with("de"))
        .unwrap_or(false)
}

// Resolve the effective UI language code ("auto" -> OS locale), EN fallback.
fn resolve_lang(pref: &str) -> &'static str {
    let raw = if pref != "auto" {
        pref.to_string()
    } else {
        sys_locale::get_locale().unwrap_or_default()
    };
    let low = raw.to_lowercase();
    for code in ["de", "es", "fr", "it", "pt", "pl", "ru", "zh", "ja"] {
        if low.starts_with(code) {
            // Return the matching static str.
            return match code {
                "de" => "de",
                "es" => "es",
                "fr" => "fr",
                "it" => "it",
                "pt" => "pt",
                "pl" => "pl",
                "ru" => "ru",
                "zh" => "zh",
                _ => "ja",
            };
        }
    }
    "en"
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
        _ => "Home", // en + it
    }
}

// Every possible default name -> a view still carrying one was never renamed.
const HOME_NAMES: [&str; 9] = [
    "Home", "Startseite", "Inicio", "Accueil", "Início",
    "Strona główna", "Главная", "主页", "ホーム",
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

fn effective_theme(app: &AppHandle, pref: &str) -> String {
    match pref {
        "light" | "dark" => pref.to_string(),
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

fn is_image_prompt(p: &Prompt) -> bool {
    p.show_image && !p.image.is_empty()
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
                if let Some(state) = app2.try_state::<Db>() {
                    let mut store = state.lock().unwrap_or_else(|e| e.into_inner());
                    if store.settings.floating.contains_key(&pid) {
                        store.settings.floating.insert(pid.clone(), Pos { x: p.x, y: p.y });
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

#[tauri::command]
fn get_clipboard_image() -> Option<String> {
    let mut cb = arboard::Clipboard::new().ok()?;
    let data = cb.get_image().ok()?;
    let bytes: Vec<u8> = data.bytes.into_owned();
    let img = image::RgbaImage::from_raw(data.width as u32, data.height as u32, bytes)?;
    let result = scale_and_encode(image::DynamicImage::ImageRgba8(img));
    if result.is_empty() { None } else { Some(result) }
}

#[tauri::command]
fn pick_image_file() -> Option<String> {
    let path = rfd::FileDialog::new()
        .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp"])
        .pick_file()?;
    let img = image::open(&path).ok()?;
    let result = scale_and_encode(img);
    if result.is_empty() { None } else { Some(result) }
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
) -> Prompt {
    let prompt = Prompt {
        id: gen_id(),
        name,
        text,
        color,
        image: image.unwrap_or_default(),
        show_image: show_image.unwrap_or(false),
        copy_image: copy_image.unwrap_or(false),
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
                let clone = p.clone();
                save_prompts(&app, &store);
                Some(clone)
            }
            None => None,
        }
    };
    if let Some(p) = &updated {
        let _ = app.emit("prompt-updated", p.clone());
        // An open pill switches between text pill and image box live.
        if let Some(win) = app.get_webview_window(&flabel(&p.id)) {
            let scale = float_scale_of(&lock(&state).settings, &p.id);
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

// UI language preference: "auto" | "en" | "de".
#[tauri::command]
fn set_language(app: AppHandle, state: State<Db>, lang: String) {
    let mut store = lock(&state);
    store.settings.language = lang;
    // Start page still has a default name -> translate it along.
    let home = home_name(resolve_lang(&store.settings.language));
    for view in &mut store.settings.views {
        if HOME_NAMES.contains(&view.name.as_str()) {
            view.name = home.to_string();
        }
    }
    save_settings(&app, &store.settings);
}

// Font family + size for the saved prompt tiles only. size 0 = auto-fit.
#[tauri::command]
fn set_tile_style(app: AppHandle, state: State<Db>, font: String, size: u32) {
    let mut store = lock(&state);
    store.settings.tile_font = font;
    store.settings.tile_size = if size == 0 { 0 } else { size.clamp(8, 32) };
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
    let old_key = grid_key(view.cols, view.rows);
    view.cols = cols.clamp(GRID_MIN, GRID_MAX);
    view.rows = rows.clamp(GRID_MIN, GRID_MAX);
    let new_key = grid_key(view.cols, view.rows);
    // First visit of this size: seed it with the fitting part of the previous
    // arrangement. Already-saved sizes keep their stored arrangement untouched.
    if new_key != old_key && !view.layouts.contains_key(&new_key) {
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

// Size factor of one pill; resizes the live window and persists the choice.
#[tauri::command]
async fn set_float_scale(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    scale: f64,
) -> Result<(), String> {
    let scale = if scale.is_finite() { scale.clamp(0.5, 2.0) } else { 1.0 };
    let is_img = {
        let mut store = lock(&state);
        store.settings.float_scale.insert(id.clone(), scale);
        save_settings(&app, &store.settings);
        store.prompts.iter().find(|p| p.id == id).map(is_image_prompt).unwrap_or(false)
    };
    if let Some(win) = app.get_webview_window(&flabel(&id)) {
        let (w, h) = pill_dims(is_img, scale);
        let _ = win.set_size(tauri::LogicalSize::new(w, h));
    }
    Ok(())
}

// Grow the pill window while its context menu is open; shrink back on close.
#[tauri::command]
async fn resize_float_menu(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    open: bool,
) -> Result<(), String> {
    let (scale, is_img) = {
        let store = lock(&state);
        (
            float_scale_of(&store.settings, &id),
            store.prompts.iter().find(|p| p.id == id).map(is_image_prompt).unwrap_or(false),
        )
    };
    if let Some(win) = app.get_webview_window(&flabel(&id)) {
        let (pw, ph) = pill_dims(is_img, scale);
        let (w, h) = if open { (pw.max(FLOAT_MENU_W), FLOAT_MENU_H) } else { (pw, ph) };
        let _ = win.set_size(tauri::LogicalSize::new(w, h));
    }
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

fn to_csv(prompts: &[Prompt], settings: &Settings) -> String {
    let mut rows = vec![format!(
        "{};{};{};{}",
        csv_cell("name"),
        csv_cell("text"),
        csv_cell("positions"),
        csv_cell("color")
    )];
    rows.push(format!(
        "{};{};{};{}",
        csv_cell("@settings"),
        csv_cell(&format!("language={}", settings.language)),
        csv_cell(""),
        csv_cell("")
    ));
    rows.push(format!(
        "{};{};{};{}",
        csv_cell("@views"),
        csv_cell(&view_def_lines(settings).join("\n")),
        csv_cell(""),
        csv_cell("")
    ));
    for p in prompts {
        let positions = position_lines(settings, &p.id).join("\n");
        rows.push(format!(
            "{};{};{};{}",
            csv_cell(&p.name),
            csv_cell(&p.text),
            csv_cell(&positions),
            csv_cell(&p.color)
        ));
    }
    rows.join("\r\n")
}

fn to_txt(prompts: &[Prompt], settings: &Settings) -> String {
    let mut blocks = vec![
        format!("@settings\nlanguage={}", settings.language),
        format!("@views\n{}", view_def_lines(settings).join("\n")),
    ];
    blocks.extend(prompts.iter().map(|p| {
        let mut block = format!("### {}\n{}", p.name, p.text);
        if !p.color.is_empty() {
            block.push_str(&format!("\n@color {}", p.color));
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
fn export_prompts(state: State<Db>, format: String) -> Result<usize, String> {
    let (content, count) = {
        let store = lock(&state);
        let content = match format.as_str() {
            "csv" => to_csv(&store.prompts, &store.settings),
            "txt" => to_txt(&store.prompts, &store.settings),
            _ => return Err(format!("Unsupported format: {}", format)),
        };
        (content, store.prompts.len())
    };
    let file = rfd::FileDialog::new()
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
    positions: Vec<String>, // "ViewName|6x5=c,r"
}

#[derive(Default)]
struct ImportData {
    language: Option<String>,
    view_defs: Vec<String>,
    prompts: Vec<ImportedPrompt>,
}

// "language=de" style lines from an @settings block.
fn parse_settings_lines(lines: &str, data: &mut ImportData) {
    for line in lines.lines() {
        if let Some(v) = line.trim().strip_prefix("language=") {
            data.language = Some(v.trim().to_string());
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
        let (text, color) = match body.rsplit_once("\n@color ") {
            Some((t, c)) => (t.to_string(), c.trim().to_string()),
            None => (body.to_string(), String::new()),
        };
        data.prompts.push(ImportedPrompt {
            name: name.trim().to_string(),
            text,
            color,
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

#[tauri::command]
fn import_prompts(app: AppHandle, state: State<Db>) -> Result<usize, String> {
    let file = rfd::FileDialog::new()
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

    let data: ImportData = if is_csv {
        let mut data = ImportData::default();
        for row in parse_csv(&content).into_iter().skip(1) {
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
                positions: row
                    .get(2)
                    .map(|p| {
                        p.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
                    })
                    .unwrap_or_default(),
            });
        }
        data
    } else {
        parse_txt(&content)
    };

    if data.prompts.is_empty() && data.view_defs.is_empty() {
        return Err("no prompts found".to_string());
    }

    let mut store = lock(&state);
    if let Some(lang) = &data.language {
        store.settings.language = lang.clone();
    }
    apply_view_defs(&mut store.settings, &data.view_defs);
    let count = data.prompts.len();
    for item in data.prompts {
        let prompt = Prompt {
            id: gen_id(),
            name: item.name,
            text: item.text,
            color: item.color,
            image: String::new(),
            show_image: false,
            copy_image: false,
        };
        apply_positions(&mut store.settings, &prompt.id, &item.positions);
        store.prompts.push(prompt);
    }
    save_prompts(&app, &store);
    save_settings(&app, &store.settings);
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

// Center a window of the given size on the primary monitor.
fn centered_on_primary(main: &tauri::WebviewWindow, width: u32, height: u32) -> WindowGeom {
    if let Some(m) = main.primary_monitor().ok().flatten() {
        let p = m.position();
        let s = m.size();
        let x = p.x + (s.width.saturating_sub(width) / 2) as i32;
        let y = p.y + (s.height.saturating_sub(height) / 2) as i32;
        return WindowGeom { x, y, width, height };
    }
    WindowGeom { x: 100, y: 100, width, height }
}

// First start: 50% of the primary monitor, centered. Afterwards the saved size
// is kept; if its monitor is gone, only the position is re-centered.
fn resolve_geometry(main: &tauri::WebviewWindow, saved: Option<WindowGeom>) -> WindowGeom {
    if let Some(g) = saved {
        if g.width > 0 && g.height > 0 {
            let monitors = main.available_monitors().unwrap_or_default();
            let cx = g.x + (g.width as i32) / 2;
            let cy = g.y + (g.height as i32) / 2;
            if point_on_monitor(&monitors, cx, cy) {
                return g;
            }
            return centered_on_primary(main, g.width, g.height);
        }
    }
    // First start: 50% x 50% of the primary screen, centered.
    let (width, height) = main
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| {
            let s = m.size();
            ((s.width / 2).max(400), (s.height / 2).max(300))
        })
        .unwrap_or((900, 600));
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
    let (title, msg) = if is_german() {
        (
            "WebView2 Runtime fehlt",
            "Prompt Saver benötigt die Microsoft WebView2 Runtime.\n\nJetzt herunterladen und installieren? Danach Prompt Saver einfach erneut starten.",
        )
    } else {
        (
            "WebView2 runtime missing",
            "Prompt Saver needs the Microsoft WebView2 runtime.\n\nDownload and install it now? Simply start Prompt Saver again afterwards.",
        )
    };
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
                let _ = main.set_size(PhysicalSize::new(geom.width, geom.height));
                let _ = main.set_position(PhysicalPosition::new(geom.x, geom.y));
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
                            update_geom(&handle2, |g| {
                                g.width = s.width;
                                g.height = s.height;
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
            let mut tray = TrayIconBuilder::new()
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
            resize_float_menu,
            edit_prompt_request,
            show_main_window,
            set_minimize_on_close,
            set_autostart,
            set_start_minimized,
            export_prompts,
            import_prompts,
            get_clipboard_image,
            pick_image_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
