use super::*;

// ---------- Theme ----------

// Every theme that has its own rules in main.css. "system" is not in here: it
// resolves to light/dark at runtime, and it is not a valid custom-theme base.
pub(crate) const BUILTIN_THEMES: [&str; 6] =
    ["light", "dark", "programmer", "neon", "ocean", "cyberpunk"];
// The colour variables a custom theme may override: the `:root` colour tokens of
// main.css. Everything else (shadow, radius, timings) stays with the base theme.
pub(crate) const THEME_VAR_KEYS: [&str; 9] = [
    "bg", "surface", "surface-2", "fg", "muted", "border", "accent", "accent-fg", "danger",
];
pub(crate) const MAX_THEME_NAME: usize = 40;
// Hard ceiling for the user-configurable limit (expert value `customThemeMax`).
pub(crate) const MAX_CUSTOM_THEMES: usize = 20;
pub(crate) const DEFAULT_CUSTOM_THEMES: usize = 8;

pub(crate) fn custom_theme_cap(settings: &Settings) -> usize {
    let raw = settings
        .ui_values
        .get("customThemeMax")
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(DEFAULT_CUSTOM_THEMES as f64);
    (raw.round() as i64).clamp(1, MAX_CUSTOM_THEMES as i64) as usize
}

pub(crate) fn valid_theme_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("theme name is empty".to_string());
    }
    if name.chars().count() > MAX_THEME_NAME {
        return Err(format!("theme name longer than {} characters", MAX_THEME_NAME));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err("theme name contains a control character".to_string());
    }
    Ok(name.to_string())
}

// A custom theme may only inherit from a built-in one: never from another custom
// theme, so inheritance can never chain or form a cycle.
pub(crate) fn valid_theme_base(base: &str) -> Result<String, String> {
    let base = base.trim();
    if BUILTIN_THEMES.contains(&base) {
        Ok(base.to_string())
    } else {
        Err("unknown base theme".to_string())
    }
}

// Keep the known colour tokens with a valid #rrggbb value; an empty value means
// "not overridden" and is dropped rather than stored.
pub(crate) fn valid_theme_vars(vars: &HashMap<String, String>) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    for (key, value) in vars {
        if !THEME_VAR_KEYS.contains(&key.as_str()) {
            return Err(format!("unknown theme variable '{}'", key));
        }
        let color = valid_tag_color(value)?;
        if !color.is_empty() {
            out.insert(key.clone(), color);
        }
    }
    Ok(out)
}

// Resolve a stored preference to a theme with real CSS rules: a custom theme
// renders as its base plus the overridden variables.
pub(crate) fn theme_base_of(settings: &Settings, pref: &str) -> String {
    match pref.strip_prefix("custom:") {
        Some(id) => settings
            .custom_themes
            .iter()
            .find(|c| c.id == id)
            .map(|c| known_theme(&c.base))
            .unwrap_or_else(default_theme),
        None => known_theme(pref),
    }
}

// A theme that was dropped from the product (or a hand-edited settings file) must
// not leave the app with no colour tokens at all: an unknown name resolves to the
// default, exactly like a missing one.
pub(crate) fn known_theme(name: &str) -> String {
    if name == "system" || BUILTIN_THEMES.contains(&name) {
        name.to_string()
    } else {
        default_theme()
    }
}

// Variable overrides of the active theme, or nothing at all: switching the feature
// off in the expert menu parks the custom themes without deleting them.
pub(crate) fn theme_vars_of(settings: &Settings) -> HashMap<String, String> {
    if settings.ui_flags.get("customThemes") != Some(&true) {
        return HashMap::new();
    }
    settings
        .theme
        .strip_prefix("custom:")
        .and_then(|id| settings.custom_themes.iter().find(|c| c.id == id))
        .map(|c| c.vars.clone())
        .unwrap_or_default()
}

#[derive(Serialize, Clone)]
pub(crate) struct InkPrefs {
    pub(crate) auto: bool,
    pub(crate) threshold: f64,
    pub(crate) halo: bool,
    #[serde(rename = "haloStrength")]
    pub(crate) halo_strength: f64,
}

// Expert-tunable ink rule for text on coloured areas (tiles, folders, view tabs,
// toasts, floating pills): same clamp pattern as tray_label_max, an absent or
// out-of-range stored value falls back to the shipped default.
pub(crate) fn ink_prefs_of(settings: &Settings) -> InkPrefs {
    InkPrefs {
        auto: settings.ui_flags.get("autoInk") != Some(&false), // on by default
        threshold: settings.ui_values.get("inkThreshold").copied().unwrap_or(25.0).clamp(20.0, 90.0),
        halo: settings.ui_flags.get("inkHalo") != Some(&false), // on by default
        halo_strength: settings.ui_values.get("inkHaloStrength").copied().unwrap_or(75.0).clamp(10.0, 100.0),
    }
}

#[derive(Serialize, Clone)]
pub(crate) struct ThemeState {
    pub(crate) theme: String,
    pub(crate) vars: HashMap<String, String>,
    pub(crate) ink: InkPrefs,
}

// Apply the active theme everywhere: native window background, main window and
// every floating pill (they listen for the event).
pub(crate) fn push_theme(app: &AppHandle, settings: &Settings) -> String {
    let base = theme_base_of(settings, &settings.theme);
    let effective = effective_theme(app, &base);
    let vars = theme_vars_of(settings);
    apply_window_bg(app, &effective, vars.get("bg").map(|s| s.as_str()));
    let ink = ink_prefs_of(settings);
    let _ = app.emit("theme-changed", ThemeState { theme: effective.clone(), vars, ink });
    effective
}

// Native window background = theme background, so the area exposed while
// resizing never flashes white. `custom_bg` is the active custom theme's override.
pub(crate) fn apply_window_bg(app: &AppHandle, theme: &str, custom_bg: Option<&str>) {
    if let Some(win) = app.get_webview_window("main") {
        if let Some(color) = custom_bg.and_then(hex_to_color) {
            let _ = win.set_background_color(Some(color));
            return;
        }
        // Matches the --bg token of each theme in main.css.
        let color = match theme {
            "dark" => tauri::webview::Color(27, 27, 29, 255),
            "programmer" => tauri::webview::Color(13, 17, 23, 255),
            "neon" => tauri::webview::Color(12, 8, 23, 255),
            "ocean" => tauri::webview::Color(238, 249, 254, 255),
            "cyberpunk" => tauri::webview::Color(10, 10, 18, 255),
            _ => tauri::webview::Color(247, 247, 248, 255),
        };
        let _ = win.set_background_color(Some(color));
    }
}

pub(crate) fn hex_to_color(hex: &str) -> Option<tauri::webview::Color> {
    let d = hex.strip_prefix('#')?;
    if d.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(d, 16).ok()?;
    Some(tauri::webview::Color(
        (n >> 16) as u8,
        ((n >> 8) & 0xff) as u8,
        (n & 0xff) as u8,
        255,
    ))
}

pub(crate) fn effective_theme(app: &AppHandle, pref: &str) -> String {
    if BUILTIN_THEMES.contains(&pref) {
        return pref.to_string();
    }
    app.get_webview_window("main")
        .and_then(|w| w.theme().ok())
        .map(|t| match t {
            tauri::Theme::Dark => "dark",
            _ => "light",
        })
        .unwrap_or("light")
        .to_string()
}

#[tauri::command(async)]
pub(crate) fn current_theme(app: AppHandle, state: State<Db>) -> String {
    let settings = lock(&state).settings.clone();
    effective_theme(&app, &theme_base_of(&settings, &settings.theme))
}

// Colour overrides of the active theme; empty for every built-in one.
#[tauri::command(async)]
pub(crate) fn theme_vars(state: State<Db>) -> HashMap<String, String> {
    theme_vars_of(&lock(&state).settings)
}

// The ink rule a satellite window fetches once at load, before the first
// theme-changed event arrives.
#[tauri::command(async)]
pub(crate) fn ink_prefs(state: State<Db>) -> InkPrefs {
    ink_prefs_of(&lock(&state).settings)
}

#[tauri::command(async)]
pub(crate) fn set_theme(app: AppHandle, state: State<Db>, theme: String) -> String {
    let settings = {
        let mut store = lock(&state);
        store.settings.theme = theme;
        save_settings(&app, &store.settings);
        store.settings.clone()
    };
    push_theme(&app, &settings)
}

// Create (empty id) or update one custom theme, then apply it. Every field is
// validated here: the frontend is not the only way into this command.
#[tauri::command(async)]
pub(crate) fn save_custom_theme(
    app: AppHandle,
    state: State<Db>,
    id: String,
    name: String,
    base: String,
    vars: HashMap<String, String>,
) -> Result<Settings, String> {
    let name = valid_theme_name(&name)?;
    let base = valid_theme_base(&base)?;
    let vars = valid_theme_vars(&vars)?;
    let settings = {
        let mut store = lock(&state);
        match store.settings.custom_themes.iter_mut().find(|c| c.id == id) {
            Some(theme) => {
                theme.name = name;
                theme.base = base;
                theme.vars = vars;
            }
            None => {
                let cap = custom_theme_cap(&store.settings);
                if store.settings.custom_themes.len() >= cap {
                    return Err(format!("max {} custom themes", cap));
                }
                let theme = CustomTheme { id: gen_id(), name, base, vars };
                store.settings.theme = format!("custom:{}", theme.id);
                store.settings.custom_themes.push(theme);
            }
        }
        save_settings(&app, &store.settings);
        webview_settings(&store.settings)
    };
    push_theme(&app, &settings);
    Ok(settings)
}

// Drop a custom theme; deleting the active one falls back to the system theme.
#[tauri::command(async)]
pub(crate) fn delete_custom_theme(app: AppHandle, state: State<Db>, id: String) -> Result<Settings, String> {
    let settings = {
        let mut store = lock(&state);
        store.settings.custom_themes.retain(|c| c.id != id);
        if store.settings.theme == format!("custom:{}", id) {
            store.settings.theme = default_theme();
        }
        save_settings(&app, &store.settings);
        webview_settings(&store.settings)
    };
    push_theme(&app, &settings);
    Ok(settings)
}

// Name/base/vars only: never the internal id, so importing a theme on another
// install always creates a fresh one instead of colliding with (or silently
// overwriting) whatever already has that id there.
#[derive(Serialize, Deserialize)]
pub(crate) struct ThemeExport {
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default = "default_custom_base")]
    pub(crate) base: String,
    #[serde(default)]
    pub(crate) vars: HashMap<String, String>,
}

// async like the other file-picker commands: a native dialog can sit open for as long as the
// user takes, and a plain fn would block the dispatch thread and every other command waiting on it.
#[tauri::command]
pub(crate) async fn export_custom_theme(app: AppHandle, state: State<'_, Db>, id: String) -> Result<(), String> {
    let theme = {
        let store = lock(&state);
        store
            .settings
            .custom_themes
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| "theme not found".to_string())?
    };
    let json = serde_json::to_string_pretty(&ThemeExport { name: theme.name.clone(), base: theme.base, vars: theme.vars })
        .map_err(|e| e.to_string())?;
    let file = file_dialog(&app)
        .set_file_name(format!("{}.json", export_file_name_from(&theme.name, "theme")))
        .add_filter("JSON", &["json"])
        .save_file();
    match file {
        Some(path) => fs::write(&path, json).map_err(|e| e.to_string()),
        None => Err("canceled".to_string()),
    }
}

// Reuses save_custom_theme's validation (name/base/vars), so an imported file is never trusted
// more than a hand-typed value. async for the same reason as export_custom_theme.
#[tauri::command]
pub(crate) async fn import_custom_theme(app: AppHandle, state: State<'_, Db>) -> Result<Settings, String> {
    let file = file_dialog(&app).add_filter("JSON", &["json"]).pick_file();
    let Some(path) = file else {
        return Err("canceled".to_string());
    };
    let content = String::from_utf8(read_file_capped(&path, MAX_THEME_BYTES)?)
        .map_err(|_| "invalid theme file".to_string())?;
    let parsed: ThemeExport = serde_json::from_str(&content).map_err(|_| "invalid theme file".to_string())?;
    save_custom_theme(app, state, String::new(), parsed.name, parsed.base, parsed.vars)
}
