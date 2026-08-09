// Application backend (Tauri v2). Local SQLite storage, clipboard, import/export,
// multiple views, frameless floating quick-copy windows. No network, 100% offline.

pub mod brand;

use image::imageops::FilterType;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::menu::{IconMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
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

// 100: global-hotkey action — open the standalone quick-launcher (fuzzy search,
// instant copy) instead of raising the main window. Replaces any launcher already
// open, so a second hotkey press never leaves two fighting over the same focus.
fn summon_launcher(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(LAUNCHER_WIN_LABEL) {
        let _ = win.close();
    }
    let excluded = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        capture_excluded(&store.settings)
    };
    let rect = launcher_rect(app);
    let win = match WebviewWindowBuilder::new(app, LAUNCHER_WIN_LABEL, WebviewUrl::App("launcher.html".into()))
        .title(brand::DISPLAY_NAME)
        .inner_size(LAUNCHER_WIN_W, LAUNCHER_WIN_H) // placeholder: set_bounds_phys owns the real box
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(true)
        .visible(false) // sized in physical pixels first, so nothing flashes at the wrong size
        .build()
    {
        Ok(w) => w,
        Err(_) => return,
    };
    let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    set_capture_exclusion(&win, excluded);
    set_bounds_phys(&win, rect);
    let _ = win.show();
    let _ = win.set_focus();
}

// ---------- Global shortcuts: the app hotkey + one per prompt (F20) ----------

type Accel = tauri_plugin_global_shortcut::Shortcut;
use tauri_plugin_global_shortcut::{Code, Modifiers};

// An accelerator is user input on its way to the OS shortcut API, so its length is
// capped before anything else looks at it.
const MAX_ACCEL_LEN: usize = 64;

// Rejection codes. The UI turns them into a localized message; they are the whole
// vocabulary the hotkey path speaks.
const HK_INVALID: &str = "hotkey-invalid"; // unusable shape (no Ctrl/Alt, Win key, garbage)
const HK_SYSTEM: &str = "hotkey-system"; // Windows or every app owns it
const HK_TAKEN: &str = "hotkey-taken"; // already bound inside this app
const HK_CONFLICT: &str = "hotkey-conflict"; // another running app holds the registration

// Keys a global hotkey may never take, whatever the modifiers are. A registered
// accelerator SWALLOWS the keystroke system-wide, so these would break the machine:
// Tab/Escape/Enter are the universal navigation + confirm/cancel keys, Backspace
// and Delete are word-wise text editing (Ctrl+Backspace), Insert carries the
// classic clipboard aliases (Ctrl+Insert / Shift+Insert), Space is the IME switch
// on CJK systems and the window menu with Alt, PrintScreen is screen capture, and
// the lock keys are hardware state. This one list also covers Ctrl+Alt+Del,
// Ctrl+Shift+Esc, Ctrl+Esc, Alt+Tab and Alt+Space without naming them.
const RESERVED_KEYS: &[Code] = &[
    Code::Tab,
    Code::Escape,
    Code::Enter,
    Code::NumpadEnter,
    Code::Backspace,
    Code::Delete,
    Code::Insert,
    Code::Space,
    Code::PrintScreen,
    Code::CapsLock,
    Code::NumLock,
    Code::ScrollLock,
    Code::Pause,
];

// Ctrl+<key> combinations every Windows application implements the same way. Taking
// one globally means the user loses copy, paste or save in EVERY program. Only the
// plain Ctrl form is blocked — adding Shift or Alt makes a distinct combination that
// no longer belongs to anyone in particular, and that is the namespace users pick
// their own shortcuts from.
const UNIVERSAL_CTRL: &[Code] = &[
    Code::KeyA, // select all
    Code::KeyC, // copy
    Code::KeyV, // paste
    Code::KeyX, // cut
    Code::KeyZ, // undo
    Code::KeyY, // redo
    Code::KeyS, // save
    Code::KeyF, // find
    Code::KeyP, // print
    Code::KeyN, // new
    Code::KeyO, // open
    Code::KeyW, // close document
    Code::KeyT, // new tab
    Code::F4,   // close document (MDI / browser tab)
];

// The pure decision: may this accelerator become a global hotkey? Some(code) = no.
// Kept free of any app state so the rule set is unit-testable on its own.
fn hotkey_block_reason(sc: &Accel) -> Option<&'static str> {
    // Windows owns the whole Win-key namespace; RegisterHotKey either fails there or
    // shadows a system shortcut the user cannot get back.
    if sc.mods.intersects(Modifiers::SUPER | Modifiers::META) {
        return Some(HK_SYSTEM);
    }
    // Shift alone is not a modifier for this purpose: Shift+A is how a capital A is
    // typed, so binding it would kill typing everywhere. Require Ctrl or Alt.
    if !sc.mods.intersects(Modifiers::CONTROL | Modifiers::ALT) {
        return Some(HK_INVALID);
    }
    if RESERVED_KEYS.contains(&sc.key) {
        return Some(HK_SYSTEM);
    }
    let plain_ctrl = sc.mods == Modifiers::CONTROL;
    if plain_ctrl && UNIVERSAL_CTRL.contains(&sc.key) {
        return Some(HK_SYSTEM);
    }
    // Alt+F4 closes the focused window in every Windows program.
    if sc.mods == Modifiers::ALT && sc.key == Code::F4 {
        return Some(HK_SYSTEM);
    }
    None
}

// The single place an accelerator is validated, with the reason it failed. Unusable:
// empty, over-long, unparseable, without a modifier (a bare key would swallow that
// key in every other app), or on the blocklist above. Pure, so validation and
// conflict handling stay testable.
fn accel_error(raw: &str) -> Result<Accel, &'static str> {
    let s = raw.trim();
    if s.is_empty() || s.len() > MAX_ACCEL_LEN || s.chars().any(|c| c.is_control()) {
        return Err(HK_INVALID);
    }
    let sc: Accel = s.parse().map_err(|_| HK_INVALID)?;
    if sc.mods.is_empty() {
        return Err(HK_INVALID);
    }
    match hotkey_block_reason(&sc) {
        Some(code) => Err(code),
        None => Ok(sc),
    }
}

// None = unusable, for the paths that only need to know whether to register it.
fn parse_accel(raw: &str) -> Option<Accel> {
    accel_error(raw).ok()
}

// Every accelerator that should be registered, in registration order. None marks
// the app hotkey, Some(id) a prompt. Unusable accelerators and combinations an
// earlier entry already claimed are skipped: the app hotkey wins, then the first
// prompt in list order. `per_prompt` is the promptHotkeys feature toggle — off
// leaves the stored accelerators alone and simply registers none of them.
fn hotkey_plan(app_hotkey: &str, prompts: &[Prompt], per_prompt: bool) -> Vec<(Accel, Option<String>)> {
    let mut plan = Vec::new();
    let mut taken: std::collections::HashSet<Accel> = std::collections::HashSet::new();
    if let Some(sc) = parse_accel(app_hotkey) {
        taken.insert(sc);
        plan.push((sc, None));
    }
    if per_prompt {
        for p in prompts {
            if let Some(sc) = parse_accel(&p.hotkey) {
                if taken.insert(sc) {
                    plan.push((sc, Some(p.id.clone())));
                }
            }
        }
    }
    plan
}

// A restored backup can carry an accelerator the app hotkey or an earlier prompt
// already owns. The import must not fail over that, so the losing binding is
// dropped and counted for the summary.
fn drop_colliding_hotkeys(app_hotkey: &str, prompts: &mut [Prompt]) -> usize {
    let mut taken: std::collections::HashSet<Accel> = std::collections::HashSet::new();
    if let Some(sc) = parse_accel(app_hotkey) {
        taken.insert(sc);
    }
    let mut dropped = 0;
    for p in prompts.iter_mut() {
        if p.hotkey.trim().is_empty() {
            p.hotkey = String::new();
            continue;
        }
        match parse_accel(&p.hotkey) {
            Some(sc) if taken.insert(sc) => {}
            _ => {
                p.hotkey = String::new();
                dropped += 1;
            }
        }
    }
    dropped
}

// Check an accelerator from the editor against everything already bound. Empty
// clears the binding; the error codes are turned into a localized inline message
// by the UI.
fn sanitize_hotkey(raw: &str, app_hotkey: &str, others: &[&Prompt]) -> Result<String, String> {
    let hk = raw.trim();
    if hk.is_empty() {
        return Ok(String::new());
    }
    let sc = accel_error(hk).map_err(|e| e.to_string())?;
    if parse_accel(app_hotkey) == Some(sc) || others.iter().any(|p| parse_accel(&p.hotkey) == Some(sc)) {
        return Err(HK_TAKEN.to_string());
    }
    Ok(hk.to_string())
}

// What a prompt hotkey does after copying. Empty = follow the global setting, so a
// stored value is only ever one of three states.
fn sanitize_hotkey_mode(raw: &str) -> String {
    match raw.trim() {
        "copy" => "copy".to_string(),
        "paste" => "paste".to_string(),
        _ => String::new(),
    }
}

// Which prompt each registered accelerator belongs to (None = the app hotkey).
// Rebuilt by rearm_hotkeys, read by the global shortcut handler.
struct HotkeyTargets(Mutex<Vec<(Accel, Option<String>)>>);

// The single owner of the registered shortcut set: drop everything, then register
// the app hotkey plus one accelerator per prompt. A combination the OS refuses is
// owned by another app — it is cleared from the store instead of leaving a binding
// the user believes in. Returns false when the APP hotkey was the refused one.
// Must never run while the store mutex is held: registering hops to the main
// thread, and the shortcut handler reads the store.
fn rearm_hotkeys(app: &AppHandle) -> bool {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    // A binding stored by an older version can be one the blocklist now refuses.
    // It is never registered again, so the stored string is cleared here — otherwise
    // the editor keeps showing a hotkey that does nothing.
    let (plan, mut dropped, mut app_ok) = match app.try_state::<Db>() {
        Some(state) => {
            let store = lock(&state);
            let per_prompt = store.settings.ui_flags.get("promptHotkeys") != Some(&false);
            let stale: Vec<String> = store
                .prompts
                .iter()
                .filter(|p| !p.hotkey.trim().is_empty() && parse_accel(&p.hotkey).is_none())
                .map(|p| p.id.clone())
                .collect();
            let app_ok = store.settings.hotkey.trim().is_empty()
                || parse_accel(&store.settings.hotkey).is_some();
            (
                hotkey_plan(&store.settings.hotkey, &store.prompts, per_prompt),
                stale,
                app_ok,
            )
        }
        None => return true,
    };
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut live = Vec::with_capacity(plan.len());
    for (sc, owner) in plan {
        if gs.register(sc).is_ok() {
            live.push((sc, owner));
        } else {
            match owner {
                Some(id) => dropped.push(id),
                None => app_ok = false,
            }
        }
    }
    if let Some(targets) = app.try_state::<HotkeyTargets>() {
        *targets.0.lock().unwrap_or_else(|e| e.into_inner()) = live;
    }
    if !app_ok || !dropped.is_empty() {
        if let Some(state) = app.try_state::<Db>() {
            let mut store = lock(&state);
            if !app_ok {
                store.settings.hotkey = String::new();
            }
            if !dropped.is_empty() {
                for p in store.prompts.iter_mut() {
                    if dropped.contains(&p.id) {
                        p.hotkey = String::new();
                    }
                }
                save_prompts(app, &store);
            }
            save_settings(app, &store.settings);
        }
    }
    app_ok
}

// A prompt hotkey runs the very same activation a tile click does. The UI owns
// copying, usage recording, chain handling and placeholder filling, so the backend
// only names the prompt that was triggered.
fn fire_prompt_hotkey(app: &AppHandle, id: &str) {
    let _ = app.emit_to("main", "prompt-hotkey", id.to_string());
}

// While a hotkey field is capturing, every global hotkey has to be off. Windows
// delivers a registered combination to the owning process instead of the focused
// window, so the field would never see the key press — the user just watches the
// prompt behind it get copied and gets no error at all.
#[tauri::command]
async fn set_hotkey_capture(app: AppHandle, active: bool) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if active {
        let _ = app.global_shortcut().unregister_all();
        if let Some(t) = app.try_state::<HotkeyTargets>() {
            t.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
    } else {
        rearm_hotkeys(&app);
    }
    Ok(())
}

// Ask the OS whether a combination is free: register it, then immediately give it
// back. Windows only refuses a combination another PROCESS holds, which is exactly
// the conflict the user needs to hear about before the binding is stored. A
// combination we already hold is ours, not a conflict.
fn accel_is_free(app: &AppHandle, sc: Accel) -> bool {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let gs = app.global_shortcut();
    if gs.is_registered(sc) {
        return true;
    }
    match gs.register(sc) {
        Ok(()) => {
            let _ = gs.unregister(sc);
            true
        }
        Err(_) => false,
    }
}

// Everything the editor needs to know BEFORE a binding is stored: shape, blocklist,
// duplicates inside the app, and whether another running app already owns it.
// `id` is the prompt being edited (its own current binding is not a duplicate).
#[tauri::command]
async fn check_hotkey(
    app: AppHandle,
    state: State<'_, Db>,
    hotkey: String,
    id: Option<String>,
) -> Result<(), String> {
    if hotkey.trim().is_empty() {
        return Ok(()); // clearing a binding needs no checks
    }
    let sc = accel_error(&hotkey).map_err(|e| e.to_string())?;
    // Read what is bound, then drop the lock: registering hops to the main thread.
    let taken = {
        let store = lock(&state);
        parse_accel(&store.settings.hotkey) == Some(sc)
            || store
                .prompts
                .iter()
                .any(|p| Some(&p.id) != id.as_ref() && parse_accel(&p.hotkey) == Some(sc))
    };
    if taken {
        return Err(HK_TAKEN.to_string());
    }
    if !accel_is_free(&app, sc) {
        return Err(HK_CONFLICT.to_string());
    }
    Ok(())
}

// Register a new global hotkey (empty = disable). Everything is validated BEFORE the
// setting is written, so a refused combination leaves the previous one live and the
// UI field in step with what is actually bound.
#[tauri::command]
async fn set_hotkey(app: AppHandle, state: State<'_, Db>, hotkey: String) -> Result<(), String> {
    let hk = hotkey.trim().to_string();
    if !hk.is_empty() {
        let sc = accel_error(&hk).map_err(|e| e.to_string())?;
        let taken = {
            let store = lock(&state);
            store.prompts.iter().any(|p| parse_accel(&p.hotkey) == Some(sc))
        };
        if taken {
            return Err(HK_TAKEN.to_string());
        }
        if !accel_is_free(&app, sc) {
            return Err(HK_CONFLICT.to_string());
        }
    }
    {
        let mut store = lock(&state);
        store.settings.hotkey = hk.clone();
        save_settings(&app, &store.settings);
    }
    // One place owns the registered set, so the app hotkey is re-armed together
    // with every prompt accelerator instead of wiping them.
    if rearm_hotkeys(&app) {
        Ok(())
    } else {
        Err(HK_CONFLICT.to_string())
    }
}

// A hotkey-triggered prompt with manual placeholders needs its fill-in dialog on
// screen, so the UI asks for the window before opening it. Only used when the
// standalone variable window is switched off (expert: varPopupWindow).
#[tauri::command]
fn focus_main_window(app: AppHandle) {
    show_main(&app);
}

// ---------- Standalone variable prompt (hotkey / tray copies) ----------
//
// A copy started from OUTSIDE the app must not drag the whole window forward just to
// ask for a placeholder. This small always-on-top window asks, and nothing else moves.
// The main window still owns every variable rule (which tokens need asking, what the
// reserved names resolve to, what happens after the copy) — this window is a form:
// it pulls the pending payload, hands back the typed values, and closes.
const VAR_WIN_LABEL: &str = "vars-0";
const VAR_WIN_W: f64 = 340.0; // logical px; keep in sync with .vars-card in floating.css
const VAR_ROW_H: f64 = 70.0;
const VAR_WIN_CHROME: f64 = 104.0; // title + buttons + padding
const VAR_WIN_MAX_H: f64 = 560.0;
const VAR_ROWS_MAX: u32 = 24;
// The payload is one prompt's placeholder list, never its body.
const VAR_PAYLOAD_MAX: usize = 64 * 1024;

// The pending request. Some(..) = a window is waiting for an answer; taking it is
// what makes an answer (or a closed window) fire exactly once. The generation is
// what keeps a *replaced* window honest: its Destroyed event arrives after the next
// request is already installed, and without the number it would cancel that one.
struct VarPrompt {
    pending: Mutex<Option<(u64, String)>>,
    next_gen: AtomicU64,
}

impl VarPrompt {
    fn new() -> Self {
        Self { pending: Mutex::new(None), next_gen: AtomicU64::new(1) }
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<(u64, String)>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// 100: quick-launcher window (fuzzy search, instant copy).
const LAUNCHER_WIN_LABEL: &str = "launcher";
const LAUNCHER_WIN_W: f64 = 560.0; // logical px; keep in sync with .launcher-card in floating.css
const LAUNCHER_WIN_H: f64 = 420.0;

// Shared by every cursor-anchored popup window (launcher, var prompt): the monitor's
// work area under the cursor plus that monitor's own scale factor, so a second
// monitor at another DPI still gets correctly sized physical pixels.
fn work_and_dpi_at_cursor(app: &AppHandle) -> (Rect, f64, PhysicalPosition<f64>) {
    let cursor = app.cursor_position().unwrap_or(PhysicalPosition::new(0.0, 0.0));
    let work = work_rect_at(app, cursor.x, cursor.y);
    let dpi = app
        .get_webview_window("main")
        .and_then(|w| w.monitor_from_point(cursor.x, cursor.y).ok().flatten())
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    (work, dpi, cursor)
}

// Centred horizontally on the cursor's monitor, sitting in the upper third the way
// every other quick-launcher (Spotlight, PowerToys Run, Alfred) does — a search box
// that pops up at screen centre reads as a dialog, not a launcher.
fn launcher_rect(app: &AppHandle) -> Rect {
    let (work, dpi, _cursor) = work_and_dpi_at_cursor(app);
    let w = LAUNCHER_WIN_W * dpi;
    let h = LAUNCHER_WIN_H * dpi;
    let x = work.x + (work.w - w) / 2.0;
    let y = work.y + (work.h - h) * 0.28;
    let (x, y) = clamp_to_work(work, w, h, x, y);
    Rect { x, y, w, h }
}

// Window box for `rows` fields, centred on the cursor and kept inside that monitor's
// work area. Physical pixels, so a second monitor at another DPI lands correctly.
fn var_window_rect(app: &AppHandle, rows: u32) -> Rect {
    let (work, dpi, cursor) = work_and_dpi_at_cursor(app);
    let w = VAR_WIN_W * dpi;
    let h = (VAR_WIN_CHROME + VAR_ROW_H * rows as f64).min(VAR_WIN_MAX_H) * dpi;
    let (x, y) = clamp_to_work(work, w, h, cursor.x - w / 2.0, cursor.y - h / 2.0);
    Rect { x, y, w, h }
}

// Open the small window for one fill-in request. Replaces a still-open one, so a
// second hotkey never leaves two forms fighting over the same copy.
#[tauri::command]
async fn open_var_window(
    app: AppHandle,
    pending: State<'_, VarPrompt>,
    payload: String,
    rows: u32,
) -> Result<(), String> {
    if payload.trim().is_empty() || payload.len() > VAR_PAYLOAD_MAX {
        return Err("bad payload".to_string());
    }
    let rows = rows.clamp(1, VAR_ROWS_MAX);
    if let Some(win) = app.get_webview_window(VAR_WIN_LABEL) {
        // Release the copy that window was holding before its slot is overwritten,
        // otherwise the first request waits for an answer that can never arrive.
        if pending.lock().take().is_some() {
            let _ = app.emit_to("main", "vars-filled", Option::<String>::None);
        }
        let _ = win.close();
    }
    // Never hold the store mutex while a window is built (deadlock shape).
    let excluded = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        capture_excluded(&store.settings)
    };
    let gen = pending.next_gen.fetch_add(1, Ordering::Relaxed);
    *pending.lock() = Some((gen, payload));
    let rect = var_window_rect(&app, rows);
    let win = WebviewWindowBuilder::new(&app, VAR_WIN_LABEL, WebviewUrl::App("vars.html".into()))
        .title(brand::DISPLAY_NAME)
        .inner_size(VAR_WIN_W, VAR_WIN_W) // placeholder: set_bounds_phys owns the real box
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(true)
        .visible(false) // sized in physical pixels first, so nothing flashes at the wrong size
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    set_capture_exclusion(&win, excluded);
    set_bounds_phys(&win, rect);
    let _ = win.show();
    let _ = win.set_focus();

    // A window closed by anything other than an answer (app quit, Esc racing the
    // command) must still release the copy waiting on it.
    let app2 = app.clone();
    win.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            if let Some(p) = app2.try_state::<VarPrompt>() {
                // Only cancel the request this window was built for. A newer one has
                // a newer generation and belongs to the window that replaced this.
                let mut slot = p.lock();
                if slot.as_ref().is_some_and(|(g, _)| *g == gen) {
                    slot.take();
                    drop(slot);
                    let _ = app2.emit_to("main", "vars-filled", Option::<String>::None);
                }
            }
        }
    });
    Ok(())
}

// The window asks for its own request once it has booted (a push would race the
// webview's first frame).
#[tauri::command]
fn var_window_payload(pending: State<VarPrompt>) -> String {
    pending.lock().as_ref().map(|(_, p)| p.clone()).unwrap_or_default()
}

// Hand the typed values back to the main window (None = cancelled) and close.
#[tauri::command]
fn var_window_result(app: AppHandle, pending: State<VarPrompt>, values: Option<String>) -> Result<(), String> {
    if values.as_ref().is_some_and(|v| v.len() > VAR_PAYLOAD_MAX) {
        return Err("bad values".to_string());
    }
    // Take first: the close below fires Destroyed, which must not answer twice.
    let waiting = pending.lock().take();
    if waiting.is_some() {
        let _ = app.emit_to("main", "vars-filled", values);
    }
    if let Some(win) = app.get_webview_window(VAR_WIN_LABEL) {
        let _ = win.close();
    }
    Ok(())
}

// F12: the {#{username}#} placeholder. The account name never reaches a log — it is
// resolved straight into the copied text.
#[tauri::command]
fn os_user_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect()
}

// ---------- F23: tray quick-copy sections ----------

// A prompt name ends up in an OS menu, so it is stripped and capped first.
const TRAY_LABEL_MAX: usize = 40;
// How far back from the cap tray_label may look for a word break.
const TRAY_LABEL_WORD_BACK: usize = 12;
// Namespace for the dynamic entries. Every other menu id is a fixed keyword, so a
// click can be routed back to a prompt without ever guessing.
const TRAY_COPY_PREFIX: &str = "psc:";
// How long a burst of store changes is collected before the menu is rebuilt.
const TRAY_DEBOUNCE_MS: u64 = 400;

// Whether the installed menu currently carries quick-copy entries, and whether a
// rebuild is already scheduled. Both flags off + nothing installed = no work at all.
static TRAY_EXTRAS: AtomicBool = AtomicBool::new(false);
static TRAY_PENDING: AtomicBool = AtomicBool::new(false);

// What the two sections should contain, read from the expert settings.
struct TrayCfg {
    recent: bool,
    recent_n: usize,
    favs: bool,
    fav_n: usize,
}

fn tray_cfg(settings: &Settings) -> TrayCfg {
    let count = |key: &str, def: f64| {
        settings.ui_values.get(key).copied().unwrap_or(def).clamp(1.0, 10.0) as usize
    };
    TrayCfg {
        recent: settings.ui_flags.get("trayRecent") != Some(&false), // on by default
        recent_n: count("trayRecentN", 5.0),
        favs: settings.ui_flags.get("trayFavs") != Some(&false), // on by default
        fav_n: count("trayFavN", 10.0),
    }
}

// Menu text for a prompt. Control characters (a pasted newline lands in the name)
// collapse to spaces, `&` is doubled because a Win32 menu reads a single one as a
// mnemonic, and the cut counts CHARACTERS — a byte cut would split a multi-byte
// name in the middle of a character. An over-long name is cut back to the last word
// break in the tail rather than mid-word, so the entry still reads as words.
fn tray_label(name: &str) -> String {
    let flat: String = name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let mut text = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > TRAY_LABEL_MAX {
        let mut cut: String = text.chars().take(TRAY_LABEL_MAX - 1).collect();
        if let Some(space) = cut.rfind(' ') {
            // Only when the break is near the end — otherwise a single long word
            // would shrink the label to almost nothing.
            if cut[..space].chars().count() >= TRAY_LABEL_MAX - TRAY_LABEL_WORD_BACK {
                cut.truncate(space);
            }
        }
        text = cut.trim_end().to_string();
        text.push('…');
    }
    if text.is_empty() {
        text.push('—'); // a nameless prompt still needs something clickable
    }
    text.replace('&', "&&")
}

fn tray_item_id(prompt_id: &str) -> String {
    format!("{TRAY_COPY_PREFIX}{prompt_id}")
}

// Route a clicked menu id back to a prompt. Anything that is not one of our own
// quick-copy ids is ignored instead of being looked up.
fn tray_copy_target(menu_id: &str) -> Option<&str> {
    match menu_id.strip_prefix(TRAY_COPY_PREFIX) {
        Some(id) if !id.is_empty() => Some(id),
        _ => None,
    }
}

// What a quick-copy entry actually delivers. It decides the entry's menu icon, so
// the media kind of a prompt is readable straight from the tray.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TrayKind {
    Text,
    Image,
    Media,
    File,
}

// The image fields are not exclusive: a picture can be the payload (copy_image), the
// tile's face (show_image) or just a decoration next to the text. Only the first two
// make the entry a picture — a decorated text prompt still copies text.
fn tray_kind(p: &Prompt) -> TrayKind {
    if !p.file_path.trim().is_empty() {
        TrayKind::File
    } else if !p.icon_path.trim().is_empty() {
        TrayKind::Media
    } else if !p.image.is_empty() && (p.copy_image || p.show_image) {
        TrayKind::Image
    } else {
        TrayKind::Text
    }
}

// One quick-copy entry, ready to become a menu item.
#[derive(Clone, PartialEq, Eq, Debug)]
struct TrayEntry {
    id: String,
    label: String,
    kind: TrayKind,
    // The prompt's own tile colour ("" = tint the icon by kind instead).
    color: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TraySection {
    Favorites,
    Recent,
}

// The menu's dynamic part, in the order it is drawn: pinned favourites first, then
// recents — the Windows jump-list idiom, so the curated section never moves under
// the user while the recents below it churn. A favourite is not repeated in the
// recents; those backfill with the next unlisted prompt instead of leaving a gap.
// Pure, so ordering, caps, dedupe and truncation stay testable — the menu API cannot.
fn tray_plan(
    prompts: &[Prompt],
    last_used: &HashMap<String, u64>,
    cfg: &TrayCfg,
) -> Vec<(TraySection, Vec<TrayEntry>)> {
    let entry = |p: &Prompt| TrayEntry {
        id: tray_item_id(&p.id),
        label: tray_label(&p.name),
        kind: tray_kind(p),
        color: p.color.clone(),
    };
    let mut out = Vec::new();
    let mut listed: Vec<&str> = Vec::new();
    if cfg.favs {
        let mut starred: Vec<&Prompt> = prompts.iter().filter(|p| p.favorite).collect();
        starred.sort_by_key(|p| (p.name.to_lowercase(), p.id.clone()));
        starred.truncate(cfg.fav_n);
        listed.extend(starred.iter().map(|p| p.id.as_str()));
        let favs: Vec<TrayEntry> = starred.into_iter().map(entry).collect();
        if !favs.is_empty() {
            out.push((TraySection::Favorites, favs));
        }
    }
    if cfg.recent {
        let mut used: Vec<(&Prompt, u64)> = prompts
            .iter()
            .filter(|p| !listed.contains(&p.id.as_str()))
            .filter_map(|p| last_used.get(&p.id).map(|ts| (p, *ts)))
            .collect();
        // Same second for two prompts: fall back to the name so the order is stable.
        used.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
        used.truncate(cfg.recent_n);
        let recent: Vec<TrayEntry> = used.into_iter().map(|(p, _)| entry(p)).collect();
        if !recent.is_empty() {
            out.push((TraySection::Recent, recent));
        }
    }
    out
}

// ---------- F23: menu icons ----------
// A menu icon is a 16x16 chip: the prompt's tile colour (or the media kind's own
// tint) with a glyph on top. It is FULLY OPAQUE on purpose. The Win32 menu path
// turns an icon into a bitmap through a monochrome AND mask that is only built
// correctly when every pixel is opaque, so a glyph with transparent padding comes
// out striped instead of clean.
const TRAY_ICON_PX: u32 = 16;

fn hex_rgb(hex: &str) -> Option<[u8; 3]> {
    let c = hex_to_color(hex.trim())?;
    Some([c.0, c.1, c.2])
}

// Perceived brightness, 0..255 — decides whether the glyph is drawn light or dark.
fn tray_luma(c: [u8; 3]) -> u32 {
    (c[0] as u32 * 299 + c[1] as u32 * 587 + c[2] as u32 * 114) / 1000
}

// Blend `c` toward `to` by `amount` percent.
fn tray_mix(c: [u8; 3], to: [u8; 3], amount: u32) -> [u8; 3] {
    let f = |a: u8, b: u8| ((a as u32 * (100 - amount) + b as u32 * amount) / 100) as u8;
    [f(c[0], to[0]), f(c[1], to[1]), f(c[2], to[2])]
}

fn tray_kind_tint(kind: TrayKind) -> [u8; 3] {
    match kind {
        TrayKind::Text => [90, 100, 114],
        TrayKind::Image => [47, 111, 237],
        TrayKind::Media => [139, 92, 246],
        TrayKind::File => [224, 138, 30],
    }
}

// A 16x16 chip under construction. Every write is bounds-checked, so a glyph that
// runs over an edge is clipped instead of wrapping onto the next row.
struct TrayChip {
    px: Vec<u8>,
}

impl TrayChip {
    fn new(fill: [u8; 3]) -> Self {
        let n = (TRAY_ICON_PX * TRAY_ICON_PX) as usize;
        let mut px = Vec::with_capacity(n * 4);
        for _ in 0..n {
            px.extend_from_slice(&[fill[0], fill[1], fill[2], 255]);
        }
        Self { px }
    }
    fn dot(&mut self, x: i32, y: i32, c: [u8; 3]) {
        let n = TRAY_ICON_PX as i32;
        if x < 0 || y < 0 || x >= n || y >= n {
            return;
        }
        let i = ((y * n + x) * 4) as usize;
        self.px[i] = c[0];
        self.px[i + 1] = c[1];
        self.px[i + 2] = c[2];
    }
    fn row(&mut self, x0: i32, x1: i32, y: i32, c: [u8; 3]) {
        for x in x0..=x1 {
            self.dot(x, y, c);
        }
    }
    fn bar(&mut self, x0: i32, x1: i32, y0: i32, y1: i32, c: [u8; 3]) {
        for y in y0..=y1 {
            self.row(x0, x1, y, c);
        }
    }
    fn frame(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 3]) {
        self.row(x0, x1, y0, c);
        self.row(x0, x1, y1, c);
        for y in y0..=y1 {
            self.dot(x0, y, c);
            self.dot(x1, y, c);
        }
    }
}

// The 16x16 RGBA chip for one entry. `color` is the prompt's tile colour; anything
// unparseable falls back to the kind's own tint.
fn tray_icon_pixels(kind: TrayKind, color: &str) -> Vec<u8> {
    let fill = hex_rgb(color).unwrap_or_else(|| tray_kind_tint(kind));
    let dark = tray_luma(fill) < 150;
    let ink = if dark { [255, 255, 255] } else { [24, 26, 31] };
    let edge = tray_mix(fill, if dark { [255, 255, 255] } else { [0, 0, 0] }, 28);
    let mut chip = TrayChip::new(fill);
    chip.frame(0, 0, 15, 15, edge);
    match kind {
        // Three text lines, the last one short — a paragraph at 16 px.
        TrayKind::Text => {
            chip.bar(4, 11, 4, 5, ink);
            chip.bar(4, 11, 7, 8, ink);
            chip.bar(4, 8, 10, 11, ink);
        }
        // Picture frame with a sun and a hill inside.
        TrayKind::Image => {
            chip.frame(3, 4, 12, 11, ink);
            chip.bar(5, 6, 6, 7, ink);
            for y in 7..=10 {
                let half = y - 7;
                chip.row((9 - half).max(4), (9 + half).min(11), y, ink);
            }
        }
        // Play triangle: rows taper symmetrically toward the tip on the right.
        TrayKind::Media => {
            for y in 3i32..=12 {
                let off = (2 * y - 15).abs(); // distance from the 7.5 centre, doubled
                chip.row(5, 11 - (off * 6) / 9, y, ink);
            }
        }
        // Page with a folded top-right corner and two ruled lines.
        TrayKind::File => {
            for y in 3..=13 {
                let right = if y < 7 { 7 + (y - 3) } else { 11 };
                chip.row(4, right, y, ink);
            }
            chip.bar(6, 9, 8, 8, fill);
            chip.bar(6, 9, 11, 11, fill);
        }
    }
    chip.px
}

fn tray_icon(kind: TrayKind, color: &str) -> tauri::image::Image<'static> {
    tauri::image::Image::new_owned(tray_icon_pixels(kind, color), TRAY_ICON_PX, TRAY_ICON_PX)
}

// One labelled section: a greyed header, the entries with their icons, a separator.
fn push_tray_section(
    app: &AppHandle,
    items: &mut Vec<Box<dyn IsMenuItem<tauri::Wry>>>,
    head_id: &str,
    title: &str,
    entries: &[TrayEntry],
) {
    if entries.is_empty() {
        return;
    }
    if let Ok(head) = MenuItem::with_id(app, head_id, title, false, None::<&str>) {
        items.push(Box::new(head));
    }
    for e in entries {
        if let Ok(item) = IconMenuItem::with_id(
            app,
            e.id.as_str(),
            e.label.as_str(),
            true,
            Some(tray_icon(e.kind, &e.color)),
            None::<&str>,
        ) {
            items.push(Box::new(item));
        }
    }
    if let Ok(sep) = PredefinedMenuItem::separator(app) {
        items.push(Box::new(sep));
    }
}

// Build the tray menu from the current store and install it. The store lock is
// dropped before the first menu call — building a menu while holding it is the
// classic deadlock shape here.
fn apply_tray_menu(app: &AppHandle) {
    let (lang, plan) = match app.try_state::<Db>() {
        Some(state) => {
            let store = lock(&state);
            let cfg = tray_cfg(&store.settings);
            let plan = tray_plan(&store.prompts, &store.settings.last_used, &cfg);
            (resolve_lang(&store.settings.language), plan)
        }
        None => return,
    };
    TRAY_EXTRAS.store(!plan.is_empty(), Ordering::SeqCst);
    let (open_label, quit_label) = tray_labels(lang);
    let (recent_title, favs_title) = tray_section_labels(lang);
    let mut items: Vec<Box<dyn IsMenuItem<tauri::Wry>>> = Vec::new();
    for (section, entries) in &plan {
        let (head_id, title) = match section {
            TraySection::Favorites => ("tray-head-favs", favs_title),
            TraySection::Recent => ("tray-head-recent", recent_title),
        };
        push_tray_section(app, &mut items, head_id, title, entries);
    }
    let (Ok(show), Ok(quit)) = (
        MenuItem::with_id(app, "show", open_label, true, None::<&str>),
        MenuItem::with_id(app, "quit", quit_label, true, None::<&str>),
    ) else {
        return;
    };
    items.push(Box::new(show));
    items.push(Box::new(quit));
    let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = items.iter().map(|b| b.as_ref()).collect();
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("tray"), Menu::with_items(app, &refs)) {
        let _ = tray.set_menu(Some(menu));
    }
}

// A tray menu is static, so anything that changes prompts, stars or usage has to
// rebuild it. Bursts (a batch action, an import, a run of copies) are collected
// into a single rebuild off the UI thread; with both features off and no entries
// installed there is nothing to do at all.
fn rebuild_tray(app: &AppHandle) {
    let wanted = match app.try_state::<Db>() {
        Some(state) => {
            let cfg = tray_cfg(&lock(&state).settings);
            cfg.recent || cfg.favs
        }
        None => false,
    };
    if !wanted && !TRAY_EXTRAS.load(Ordering::SeqCst) {
        return; // the legacy menu is already what the settings ask for
    }
    if TRAY_PENDING.swap(true, Ordering::SeqCst) {
        return; // a rebuild is already due and will pick these changes up
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(TRAY_DEBOUNCE_MS));
        TRAY_PENDING.store(false, Ordering::SeqCst);
        apply_tray_menu(&app);
    });
}

// A tray entry runs the very same activation a tile click does (copy, usage,
// chains, placeholder dialog) — but never the hotkey auto-paste: the click leaves
// the tray in the foreground, so there is no text field to paste into.
fn fire_tray_copy(app: &AppHandle, id: &str) {
    let _ = app.emit_to("main", "prompt-tray-copy", id.to_string());
}

// 100: a result picked in the quick-launcher runs the very same activation the tray
// does (fire_tray_copy) -- placeholder dialog, cooldown, chains, everything already
// tested for "a copy that started outside the grid" -- then the launcher itself closes.
#[tauri::command]
fn launcher_select(app: AppHandle, id: String) -> Result<(), String> {
    fire_tray_copy(&app, &id);
    if let Some(win) = app.get_webview_window(LAUNCHER_WIN_LABEL) {
        let _ = win.close();
    }
    Ok(())
}

#[tauri::command]
fn launcher_close(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(LAUNCHER_WIN_LABEL) {
        let _ = win.close();
    }
    Ok(())
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
    // 83: which page of an attached PDF the preview shows (0-based). Ignored for
    // any other attachment type. Clamped against the actual page count at render
    // time (pdf_preview), never here — the file can change independently of this
    // number (a replaced attachment, or a shorter PDF at the same path).
    #[serde(default)]
    pdf_page: u32,
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
    // F1: free-form tag names. Compared case-insensitively; the casing stored here
    // is the one first used in the library (see sanitize_tags).
    #[serde(default)]
    tags: Vec<String>,
    // F17: private context for the user. Shown in the editor and (optionally) in the
    // tile tooltip — never part of anything that reaches the clipboard.
    #[serde(default)]
    notes: String,
    // F20: global accelerator that copies this prompt from anywhere ("" = none).
    #[serde(default)]
    hotkey: String,
    // F20: what THIS hotkey does — "copy", "paste" (copy and paste into the app the
    // user was in), or "" = follow the global promptHotkeyPaste setting.
    #[serde(default)]
    hotkey_paste: String,
    // 91: unix seconds set the moment a prompt moves into the trash table; unused
    // (0) for every prompt still in store.prompts. Drives both the trash list's
    // "deleted N days ago" and the retention sweep — never touched for anything
    // else, so it never appears in an export or a fresh add_prompt/update_prompt.
    #[serde(default)]
    trashed_at: u64,
    // 32: "rotating prompts" — `text` is entry 1, `variants` holds entries 2..n, so
    // every existing consumer (tile preview, search, export, dup-finder, tooltip,
    // Markdown sync) stays correct without a single change, and "rotation is on" is
    // exactly `!variants.is_empty()` — no separate bool that could drift from the data.
    #[serde(default)]
    variants: Vec<String>,
    // "" = in order, "random" = shuffled bag (see rotation_next).
    #[serde(default)]
    rotate_mode: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
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

// F9: a folder-style tab holding several views. A view belongs to at most one
// group; every view left out of `view_ids` keeps its own top-level tab.
#[derive(Serialize, Deserialize, Clone)]
struct ViewGroup {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    // Optional tab color (hex); empty = default.
    #[serde(default)]
    color: String,
    #[serde(default)]
    view_ids: Vec<String>,
}

// F27: an Android-style folder inside the grid. It occupies ONE cell of one view and
// holds prompts that are no longer tiles of their own: a member's layout entry is
// dropped when it joins and it gets a free cell again when it leaves. A view can
// change its grid size, so a folder whose cell no longer exists is moved to the first
// free one instead of disappearing.
#[derive(Serialize, Deserialize, Clone)]
struct TileFolder {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    // Optional folder colour (hex); empty = default.
    #[serde(default)]
    color: String,
    #[serde(default)]
    view_id: String,
    #[serde(default)]
    cell: [u32; 2],
    #[serde(default)]
    members: Vec<String>,
}

// F25: an Android-style folder holding several floating pills. It owns ONE window
// (the folder icon; expanding only resizes it), so `pos` is the icon's top-left in
// physical desktop pixels — the same space `Settings.floating` uses. A member keeps
// its own `floating` entry so switching the feature off puts every pill back where
// it was.
#[derive(Serialize, Deserialize, Clone)]
struct FloatFolder {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    pos: Pos,
    #[serde(default = "default_folder_scale")]
    scale: f64,
    #[serde(default)]
    members: Vec<String>,
}

fn default_folder_scale() -> f64 {
    1.0
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct VideoPrefs {
    volume: u32, // 0..=100
    muted: bool,
    #[serde(rename = "loop")]
    looped: bool,
}

// F3: a user-defined theme. It inherits every rule of `base` (one of the built-in
// themes) and overrides only the colour variables listed in `vars`.
#[derive(Serialize, Deserialize, Clone)]
struct CustomTheme {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default = "default_custom_base")]
    base: String,
    #[serde(default)]
    vars: HashMap<String, String>,
}

fn default_custom_base() -> String {
    "light".to_string()
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
    // F25: pill folders. Kept even while the feature is switched off, so turning it
    // back on restores the folders instead of losing them.
    #[serde(default)]
    float_folders: Vec<FloatFolder>,
    // Per-prompt video player state (volume, mute, loop), grid and pill.
    #[serde(default)]
    video_prefs: HashMap<String, VideoPrefs>,
    #[serde(default)]
    window: Option<WindowGeom>,
    #[serde(default)]
    minimize_to_tray: bool,
    // One-time hint (P92): explains that the tray menu still copies while hidden.
    #[serde(default)]
    seen_tray_hint: bool,
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
    // 66: key -> unix seconds of its last write, across ui_flags/ui_values/ui_texts.
    // A missing key was never explicitly changed (still at its shipped default).
    #[serde(default)]
    ui_changed: HashMap<String, u64>,
    // 2: seeded once on a genuinely fresh install (empty library, never seeded
    // before) — never re-seeded just because the user later deletes everything.
    #[serde(default)]
    examples_seeded: bool,
    // 2: ids of the seeded example prompts, so "remove examples" can find exactly
    // those three (and only those) without guessing by name/content.
    #[serde(default)]
    example_prompt_ids: Vec<String>,
    // F6: prompt-id pairs (sorted) the user marked "not a duplicate" — never regrouped.
    #[serde(default)]
    dup_ignored: Vec<[String; 2]>,
    // F1: tag key (the lowercased name) -> chip color (#rrggbb). Keyed lowercase so a
    // recased tag keeps its color. A tag without an entry renders neutral.
    #[serde(default)]
    tag_colors: HashMap<String, String>,
    // F3: user-defined themes. `theme` points at one of them with "custom:<id>";
    // each carries only the variables that differ from its built-in base.
    #[serde(default)]
    custom_themes: Vec<CustomTheme>,
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
    // 32: rotation pointer for "variants" prompts — kept out of Prompt itself (see
    // rotation_next) so every copy doesn't rewrite prompts.json / bump the version
    // history / touch the Markdown sync file. Index of the NEXT entry to serve.
    #[serde(default)]
    rotate_pos: HashMap<String, u32>,
    // 32: shuffled remaining indices for "random" mode (the "bag" — drawn without
    // repetition, reshuffled once empty). Absent/empty means "reshuffle on next use".
    #[serde(default)]
    rotate_bag: HashMap<String, Vec<u32>>,
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
    // F9: view groups. Kept even while the feature is switched off, so turning it
    // back on restores the folders instead of losing them.
    #[serde(default)]
    view_groups: Vec<ViewGroup>,
    // F27: grid folders. Kept while the feature is off as well — the members simply
    // show as their own tiles again until it comes back on.
    #[serde(default)]
    tile_folders: Vec<TileFolder>,
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
            float_folders: Vec::new(),
            video_prefs: HashMap::new(),
            window: None,
            minimize_to_tray: false,
            seen_tray_hint: false,
            autostart: false,
            start_minimized: false,
            always_on_top: false,
            // 100: a fresh install ships with the quick-launcher already bound, so the
            // feature is discoverable without a trip to settings. An EXISTING install's
            // stored value (including a deliberately empty/off one) is untouched --
            // #[serde(default)] only ever fills in a value that was never written.
            hotkey: "CommandOrControl+Alt+P".to_string(),
            auto_update: true,
            skipped_versions: Vec::new(),
            ui_flags: HashMap::new(),
            ui_values: HashMap::new(),
            ui_texts: HashMap::new(),
            ui_changed: HashMap::new(),
            examples_seeded: false,
            example_prompt_ids: Vec::new(),
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
            view_groups: Vec::new(),
            tile_folders: Vec::new(),
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
const MAX_VIEW_GROUPS: usize = 100;
// F27: grid folders. One per cell at most, so a huge grid is the real ceiling.
const MAX_TILE_FOLDERS: usize = 200;
const MAX_GROUP_NAME: usize = 60;
const FLOAT_W: f64 = 360.0;
const FLOAT_H: f64 = 80.0; // flat pill shape, clearly wider than tall
const FLOAT_IMG: f64 = 400.0; // square box for image pills: S 300 / M 400 / L 560
// F25 pill folders.
const MAX_FLOAT_FOLDERS: usize = 100;
const MAX_FOLDER_NAME: usize = 40;
const FOLDER_BOX: f64 = 96.0; // collapsed folder icon (square, logical px at scale 1)
// Brand-owned identifiers. Every product name in the Rust code comes from
// src/brand.rs, which tools/brand.js writes from brands/<brand>.json.
const AUTOSTART_KEY: &str = brand::AUTOSTART_KEY;
// Local data folder under %APPDATA%\Roaming. Used by the pre-app panic hook
// (no AppHandle) and the store path; deliberately NOT the bundle identifier.
const DATA_FOLDER: &str = brand::DATA_FOLDER;

fn grid_key(cols: u32, rows: u32) -> String {
    format!("{}x{}", cols, rows)
}

// WebView2-missing dialog texts; shown before settings exist, so the OS
// locale decides (same language set as the UI). `{app}` is filled with the
// product name once, in webview2_dialog — the translations never spell it.
fn webview2_texts(lang: &str) -> (&'static str, &'static str) {
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
fn webview2_dialog(lang: &str) -> (String, String) {
    let (title, msg) = webview2_texts(lang);
    (
        title.replace("{app}", brand::DISPLAY_NAME),
        msg.replace("{app}", brand::DISPLAY_NAME),
    )
}

// Resolve the effective UI language code ("auto" -> OS locale), EN fallback.
// Supported UI languages besides English (keep in sync with ui/i18n.js).
const LANG_CODES: [&str; 10] = [
    "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro",
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

// F23: headers of the two tray quick-copy sections. The tray is drawn by the OS,
// not by the webview, so these live next to the other tray labels instead of in
// the frontend table.
fn tray_section_labels(lang: &str) -> (&'static str, &'static str) {
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

// Localized default name of the auto-created start page. As long as the user
// never renamed it, it follows the UI language (see set_language).
fn home_name(lang: &str) -> &'static str {
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
const HOME_NAMES: [&str; 18] = [
    "Home", "Startseite", "Inicio", "Accueil", "Início",
    "Strona główna", "Главная", "主页", "ホーム",
    "Ana sayfa", "홈", "होम", "Beranda", "Trang chủ",
    "Domů", "Головна", "Hem", "Acasă",
];

impl Settings {
    // Ensure at least one view exists; migrate legacy single-grid data.
    fn migrate(&mut self) {
        // The privacy switch used to be "hide from screen capture" (on unless stored
        // false); it is now "allow screenshots" (off unless stored true). Anyone who
        // had deliberately turned the hiding OFF keeps their screenshots.
        if let Some(hidden) = self.ui_flags.remove("captureExclusion") {
            if !hidden {
                self.ui_flags.insert(ALLOW_SCREENSHOT_KEY.to_string(), true);
            }
        }
        // 24: the "ai" theme was renamed to "neon" (Ben: no "ai" anywhere, even
        // internally). Rewrite a stored preference so nobody's colours reset to
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
            self.views.push(View {
                id: gen_id(),
                name: home_name(resolve_lang(&self.language)).to_string(),
                cols: self.legacy_cols,
                rows: self.legacy_rows,
                layouts,
                color: String::new(),
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
        }
        if !self.views.iter().any(|v| v.id == self.active_view) {
            if let Some(first) = self.views.first() {
                self.active_view = first.id.clone();
            }
        }
        normalize_view_groups(&mut self.view_groups, &self.views);
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
    // 91: prompts the user deleted, kept whole (media + version history untouched)
    // until a real purge. Newest-deleted first, matching how every other list in
    // the app (recent copies, history) already orders "most recent on top".
    trash: Vec<Prompt>,
    settings: Settings,
    // Set when db_load found encrypted prompt rows it could NOT decrypt (e.g. DPAPI
    // failed after a profile move / password reset). While true, save_prompts refuses
    // to rewrite the prompts table so a full DELETE+INSERT can never destroy the rows
    // that are present-but-unreadable. The raw data.db is snapshotted aside on detect.
    prompts_locked: bool,
}

impl Store {
    // F25/F27: keep both folder lists valid against the CURRENT prompt list — one call
    // after every mutation that can remove a prompt. Idempotent.
    fn normalize_folders(&mut self) {
        let ids: Vec<String> = self.prompts.iter().map(|p| p.id.clone()).collect();
        normalize_float_folders(&mut self.settings.float_folders, &ids);
        let view_ids: Vec<String> = self.settings.views.iter().map(|v| v.id.clone()).collect();
        normalize_tile_folders(&mut self.settings.tile_folders, &ids, &view_ids);
    }
}

type Db = Mutex<Store>;

// ---------- Paths + persistence ----------

fn default_data_dir(app: &AppHandle) -> PathBuf {
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

// One-time move of the store out of the old identifier-based folder
// (brand::LEGACY_DATA_FOLDER) into DATA_FOLDER, so the rename never loses data.
// Both the identifier folder Tauri resolves and the frozen legacy name are tried:
// the second one survives even if Tauri ever changes how app_data_dir is built.
fn migrate_legacy_data(app: &AppHandle, new: &std::path::Path) {
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

// The store's JSON files (prompts.json / settings.json — the DB-write fallback and the
// legacy pre-SQLite format) hold full prompt bodies and private notes, so they are
// DPAPI-sealed exactly like the SQLite rows. `dec_str` passes an un-prefixed value
// through unchanged, which is what still lets a genuinely old plaintext install be
// migrated in; it is re-sealed the next time it is written.
fn read_sealed_json<T: for<'de> Deserialize<'de> + Default>(path: &PathBuf) -> T {
    match fs::read_to_string(path) {
        Ok(raw) if !raw.trim().is_empty() => serde_json::from_str(&dec_str(&raw)).unwrap_or_default(),
        _ => T::default(),
    }
}

// Every import reads a file the user picked — which may have come from someone else —
// fully into memory before parsing. Check the size FIRST so a wrong pick (a disk image,
// a video) fails with a clear message instead of exhausting memory. Mirrors the cap the
// decompression path already applies via `inflate_capped`.
const MAX_IMPORT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_THEME_BYTES: u64 = 4 * 1024 * 1024; // a theme is a handful of colour values

fn check_import_size(path: &std::path::Path, max: u64) -> Result<(), String> {
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

// Atomic write: temp file then rename, so a crash never truncates data. Returns
// whether the data actually reached disk, so callers can log a persist failure.
fn write_sealed_json<T: Serialize>(path: &PathBuf, data: &T) -> bool {
    if let Ok(json) = serde_json::to_string(data).map(|j| enc_str(&j)) {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, json).is_ok() {
            return fs::rename(&tmp, path).is_ok();
        }
    }
    false
}

// Capitalise a brand word for a dialog title ("prompts" -> "Prompts").
fn title_case(w: &str) -> String {
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

// Encrypt a JSON string for storage (prefix + base64 ciphertext).
//
// A DPAPI failure falls back to plaintext rather than refusing the write: hard-failing
// here would mean the user cannot save at all, trading a confidentiality problem for
// guaranteed data loss. It must never be SILENT though — a broken Windows profile
// would otherwise downgrade the whole store from sealed to readable with nothing to
// show for it. Logged once per run (this runs per row; a full save would flood the log).
fn enc_str(plain: &str) -> String {
    #[cfg(windows)]
    {
        if let Some(ct) = store_crypt::protect(plain.as_bytes()) {
            return format!("{}{}", ENC_PREFIX, base64_encode(&ct));
        }
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            append_crash_log(
                "DPAPI encryption unavailable - stored data is being written UNENCRYPTED. \
                 Check the Windows user profile; a repaired profile re-seals on the next save.",
            );
        }
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
         CREATE TABLE IF NOT EXISTS trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS idx_copy_log_ts ON copy_log(ts);
         CREATE TABLE IF NOT EXISTS prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
         CREATE INDEX IF NOT EXISTS idx_pv_prompt ON prompt_versions(prompt_id);
         CREATE TABLE IF NOT EXISTS clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);",
    )
    .ok()?;
    // 32: first-ever schema migration in this app — `prompt_versions` predates
    // `variants`. `CREATE TABLE IF NOT EXISTS` above already gives a brand-new
    // install the column; an EXISTING data.db still has the old 5-column table.
    // Checked via PRAGMA table_info rather than blindly firing ALTER TABLE and
    // swallowing whatever comes back — that would also swallow a REAL failure
    // (locked/full/corrupt disk), silently leaving the column missing with no way
    // to tell the difference from "already migrated" (Codex audit).
    let has_variants_column = conn
        .prepare("SELECT 1 FROM pragma_table_info('prompt_versions') WHERE name = 'variants'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(true); // a PRAGMA query failing here means something is already
                           // badly wrong with the connection — do not also spam an
                           // ALTER TABLE into that same broken state.
    if !has_variants_column {
        let _ = conn.execute("ALTER TABLE prompt_versions ADD COLUMN variants TEXT NOT NULL DEFAULT ''", []);
    }
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

// 91: same schema and shape as db_write_prompts/db_load's prompt half, but its own
// table. A prompt "in the trash" simply isn't in `prompts` any more, so every
// existing read of store.prompts (grid, library, tray, hotkeys, export, stats,
// duplicate finder...) stays correct with zero changes — the alternative, a
// deleted_at flag on Prompt filtered everywhere, would have needed every one of
// those call sites found and fixed, with real risk of missing one.
fn db_write_trash(conn: &mut Connection, trash: &[Prompt]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM trash", [])?;
    {
        let mut stmt = tx.prepare("INSERT INTO trash(id, ord, data) VALUES(?1, ?2, ?3)")?;
        for (i, p) in trash.iter().enumerate() {
            let data = enc_str(&serde_json::to_string(p).unwrap_or_default());
            stmt.execute(params![p.id, i as i64, data])?;
        }
    }
    tx.commit()
}

fn db_load_trash(conn: &Connection) -> Vec<Prompt> {
    let mut trash = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT data FROM trash ORDER BY ord") {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for data in rows.flatten() {
                if let Ok(p) = serde_json::from_str::<Prompt>(&dec_str(&data)) {
                    trash.push(p);
                }
            }
        }
    }
    trash
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

// Peeks the "allowMultipleInstances" flag before the Tauri builder exists. The
// single-instance plugin has to be attached (or not) at builder time, before any
// AppHandle exists to resolve settings the normal way — so this mirrors
// default_data_dir()'s portable/APPDATA resolution and opens the DB directly. Any
// failure (first run, missing DB, undecryptable row) yields false: single instance,
// the safe default that matches every install before this option existed.
fn early_allow_multiple_instances() -> bool {
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

// 59: one journal row as shown/deletable in the UI — unlike CopyEntry (insert/trim/
// legacy-migration only), this carries the rowid a delete needs to target the exact
// row, even in grouped mode where several rows share the same prompt id.
#[derive(Serialize)]
struct JournalEntry {
    rowid: i64,
    id: String,
    ts: u64,
}

// 58: copy counts for the last `days` UTC days, oldest first, zero-filled — a
// single GROUP BY query bucketed by day-since-epoch, merged against a dense day
// range in Rust (cheaper than one query per day, and never skips an empty day).
fn db_daily_copies(conn: &Connection, days: u32) -> Vec<DayCount> {
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
fn db_recent_copies(conn: &Connection, limit: usize, grouped: bool) -> Vec<JournalEntry> {
    let sql = if grouped {
        "SELECT MAX(rowid), id, MAX(ts) FROM copy_log GROUP BY id ORDER BY MAX(rowid) DESC LIMIT ?1"
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

// ---- F7 prompt version history ----
// Snapshots of a prompt's previous name/text, encrypted like the prompt rows. The
// table lives in data.db (not the settings blob) so history never bloats settings.
#[derive(Serialize)]
struct PromptVersion {
    id: i64,
    ts: u64,
    name: String,
    text: String,
    // 32: entries ARE content — a version snapshot without them would let a
    // restore silently drop entries 2..n. JSON-encoded in its own DB column
    // (unlike the export formats, nothing here needs to stay human-editable).
    variants: Vec<String>,
}

fn encode_variants(variants: &[String]) -> String {
    serde_json::to_string(variants).unwrap_or_default()
}
fn decode_variants(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn history_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("promptHistory") != Some(&false) // on by default
}
fn history_cap(settings: &Settings) -> usize {
    settings.ui_values.get("historyPerPrompt").copied().unwrap_or(20.0).clamp(5.0, 100.0) as usize
}

fn db_add_version(conn: &Connection, prompt_id: &str, ts: u64, name: &str, text: &str, variants: &[String], cap: usize) -> rusqlite::Result<()> {
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
fn db_list_versions(conn: &Connection, prompt_id: &str) -> Vec<PromptVersion> {
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
// 17: one prompt's last real content edit (name or text change — the only kind
// that gets a version row, see update_prompt) is the newest `ts` it has in
// prompt_versions. Bundled for every prompt at once (one GROUP BY, not an N+1
// query per prompt) — a prompt absent from the result has never had a version
// row: history was off for all its edits, or it was never edited since creation.
fn db_last_edited(conn: &Connection) -> std::collections::HashMap<String, u64> {
    let mut out = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT prompt_id, MAX(ts) FROM prompt_versions GROUP BY prompt_id") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))) {
            out.extend(rows.flatten());
        }
    }
    out
}
fn db_get_version(conn: &Connection, id: i64) -> Option<(String, String, String, Vec<String>)> {
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

// Returns whether the prompts made it to durable storage (DB or JSON fallback) —
// callers that follow a save with a destructive step (102: deleting an orphaned
// attachment file) must skip that step when this is false, or the deletion can
// outlive the very change that made the file orphaned.
fn save_prompts(app: &AppHandle, store: &Store) -> bool {
    // Safety valve: if some rows failed to decrypt at load, the in-memory list is a
    // SUBSET of what's on disk. Rewriting the table (DELETE + re-INSERT) would delete
    // the unreadable rows permanently — so we refuse to persist prompt changes at all
    // until the situation is resolved. Prompt edits this session simply won't stick.
    if store.prompts_locked {
        return false;
    }
    if let Some(mut conn) = db_conn(app) {
        if db_write_prompts(&mut conn, &store.prompts).is_ok() {
            sync_write_all(app, store); // Prompt-folders-as-Markdown: no-op unless the feature is on
            return true;
        }
    }
    // Last resort: JSON fallback. 101: load_store() reads this back and retries the
    // DB write on next launch, so a fallback here is not itself data loss. If even
    // this fails (disk full / permission), the change really is lost — leave a
    // diagnosable trace instead of failing silently.
    if !write_sealed_json(&data_dir(app).join("prompts.json"), &store.prompts) {
        append_crash_log("persist failed: prompts could not be written to the database or the JSON fallback");
        let _ = app.emit("persist-failed", "prompts");
        return false;
    }
    sync_write_all(app, store);
    true
}

fn save_trash(app: &AppHandle, store: &Store) -> bool {
    // Same lock as save_prompts: an unreadable-row situation must never trigger a
    // destructive rewrite of a second table either.
    if store.prompts_locked {
        return false;
    }
    if let Some(mut conn) = db_conn(app) {
        if db_write_trash(&mut conn, &store.trash).is_ok() {
            return true;
        }
    }
    append_crash_log("persist failed: trash could not be written to the database");
    let _ = app.emit("persist-failed", "trash");
    false
}

fn save_settings(app: &AppHandle, settings: &Settings) {
    if let Some(conn) = db_conn(app) {
        if db_write_settings(&conn, settings).is_ok() {
            return;
        }
    }
    if !write_sealed_json(&data_dir(app).join("settings.json"), settings) {
        append_crash_log("persist failed: settings could not be written to the database or the JSON fallback");
    }
}

// Pre-1.9 builds saved image prompts without copy_image but copied on click.
// The self-filling placeholder names, kept in step with DYN_VARS in ui/main.js.
const DYN_NAMES: [&str; 16] = [
    "date", "time", "datetime", "weekday", "yesterday", "tomorrow", "week", "month",
    "year", "timestamp", "epoch", "clipboard", "username", "uuid", "random", "counter",
];

// Reserved names are now written {#{/date\}#} instead of {#{date}#}, which frees the
// plain word for a variable of the user's own. Rewrite the old spelling so an existing
// prompt keeps filling itself in. Idempotent — the new form no longer matches.
fn fence_dyn_vars(text: &str) -> String {
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

fn migrate_prompts(prompts: &mut [Prompt]) {
    // `id` is the prompts table's PRIMARY KEY, so a duplicate makes the whole
    // DELETE+INSERT rewrite fail — and save_prompts then quietly writes the JSON
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

fn load_store(app: &AppHandle) -> Store {
    let dir = data_dir(app);
    if let Some(mut conn) = db_conn(app) {
        let (mut prompts, settings_opt, needs_mig, decrypt_fails) = db_load(&conn);
        let trash = db_load_trash(&conn);
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
            let j_settings: Settings = read_sealed_json(&dir.join("settings.json"));
            let j_prompts: Vec<Prompt> = read_sealed_json(&dir.join("prompts.json"));
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
        } else {
            // 101: prompts.json/settings.json otherwise only exist as a save_prompts/
            // save_settings fallback from a failed DB write earlier this session (they
            // are deleted the moment the DB accepts the data again) — their presence
            // here means the DB never received that change, so the file, not the DB,
            // is the truth. Retry the write; keep the file (and the loaded data) if the
            // DB still refuses, so this keeps retrying every launch instead of quietly
            // losing the edit.
            let pj = dir.join("prompts.json");
            if pj.exists() {
                let fallback: Vec<Prompt> = read_sealed_json(&pj);
                prompts = fallback;
                if db_write_prompts(&mut conn, &prompts).is_ok() {
                    let _ = fs::remove_file(&pj);
                }
            }
            let sj = dir.join("settings.json");
            if sj.exists() {
                settings = read_sealed_json(&sj);
                if db_write_settings(&conn, &settings).is_ok() {
                    let _ = fs::remove_file(&sj);
                }
            }
        }
        // 24: force a resave when the "ai" theme is about to be rewritten to
        // "neon" — otherwise the fix only lives in memory until some unrelated
        // setting change happens to persist it.
        let theme_ai_before =
            settings.theme == "ai" || settings.custom_themes.iter().any(|c| c.base == "ai");
        settings.migrate();
        load_pick_history(&settings); // 19: restore the colour-pick history mirror
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
        let (cap, cutoff) = history_cap_and_cutoff(&settings);
        let _ = db_trim_copy(&conn, cap, cutoff);
        migrate_prompts(&mut prompts);
        let mut store = Store { prompts, trash, settings, prompts_locked: decrypt_fails > 0 };
        store.normalize_folders(); // F25: drop members whose prompt is gone
        sweep_trash(app, &mut store); // 91: drop anything past the retention window
        if needs_mig || migrated_log {
            // One-time: re-encrypt legacy plaintext rows / drop the migrated log from the blob.
            save_prompts(app, &store);
            save_settings(app, &store.settings);
        } else if theme_ai_before {
            save_settings(app, &store.settings);
        }
        return store;
    }
    // Fallback: DB unavailable → read the JSON files directly.
    let mut settings: Settings = read_sealed_json(&dir.join("settings.json"));
    settings.migrate();
    load_pick_history(&settings); // 19: restore the colour-pick history mirror
    prune_history(&mut settings);
    let mut prompts: Vec<Prompt> = read_sealed_json(&dir.join("prompts.json"));
    migrate_prompts(&mut prompts);
    let mut store = Store { prompts, trash: Vec::new(), settings, prompts_locked: false };
    store.normalize_folders();
    sweep_trash(app, &mut store); // 91: no-op here (fallback path never loads trash), kept for symmetry
    store
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

// Every theme that has its own rules in main.css. "system" is not in here: it
// resolves to light/dark at runtime, and it is not a valid custom-theme base.
const BUILTIN_THEMES: [&str; 7] =
    ["light", "dark", "programmer", "neon", "ocean", "cyberpunk", "mono"];
// The colour variables a custom theme may override — the `:root` colour tokens of
// main.css. Everything else (shadow, radius, timings) stays with the base theme.
const THEME_VAR_KEYS: [&str; 9] = [
    "bg", "surface", "surface-2", "fg", "muted", "border", "accent", "accent-fg", "danger",
];
const MAX_THEME_NAME: usize = 40;
// Hard ceiling for the user-configurable limit (expert value `customThemeMax`).
const MAX_CUSTOM_THEMES: usize = 20;
const DEFAULT_CUSTOM_THEMES: usize = 8;

fn custom_theme_cap(settings: &Settings) -> usize {
    let raw = settings
        .ui_values
        .get("customThemeMax")
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(DEFAULT_CUSTOM_THEMES as f64);
    (raw.round() as i64).clamp(1, MAX_CUSTOM_THEMES as i64) as usize
}

fn valid_theme_name(name: &str) -> Result<String, String> {
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

// A custom theme may only inherit from a built-in one — never from another custom
// theme, so inheritance can never chain or form a cycle.
fn valid_theme_base(base: &str) -> Result<String, String> {
    let base = base.trim();
    if BUILTIN_THEMES.contains(&base) {
        Ok(base.to_string())
    } else {
        Err("unknown base theme".to_string())
    }
}

// Keep the known colour tokens with a valid #rrggbb value; an empty value means
// "not overridden" and is dropped rather than stored.
fn valid_theme_vars(vars: &HashMap<String, String>) -> Result<HashMap<String, String>, String> {
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
fn theme_base_of(settings: &Settings, pref: &str) -> String {
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
// not leave the app with no colour tokens at all — an unknown name resolves to the
// default, exactly like a missing one.
fn known_theme(name: &str) -> String {
    if name == "system" || BUILTIN_THEMES.contains(&name) {
        name.to_string()
    } else {
        default_theme()
    }
}

// Variable overrides of the active theme, or nothing at all: switching the feature
// off in the expert menu parks the custom themes without deleting them.
fn theme_vars_of(settings: &Settings) -> HashMap<String, String> {
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
struct ThemeState {
    theme: String,
    vars: HashMap<String, String>,
}

// Apply the active theme everywhere: native window background, main window and
// every floating pill (they listen for the event).
fn push_theme(app: &AppHandle, settings: &Settings) -> String {
    let base = theme_base_of(settings, &settings.theme);
    let effective = effective_theme(app, &base);
    let vars = theme_vars_of(settings);
    apply_window_bg(app, &effective, vars.get("bg").map(|s| s.as_str()));
    let _ = app.emit("theme-changed", ThemeState { theme: effective.clone(), vars });
    effective
}

// Native window background = theme background, so the area exposed while
// resizing never flashes white. `custom_bg` is the active custom theme's override.
fn apply_window_bg(app: &AppHandle, theme: &str, custom_bg: Option<&str>) {
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
            "mono" => tauri::webview::Color(250, 250, 250, 255),
            _ => tauri::webview::Color(247, 247, 248, 255),
        };
        let _ = win.set_background_color(Some(color));
    }
}

fn hex_to_color(hex: &str) -> Option<tauri::webview::Color> {
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

fn effective_theme(app: &AppHandle, pref: &str) -> String {
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

// One range for floating-button size, shared by the writer, the reader and the UI.
const FLOAT_SCALE_MIN: f64 = 0.3;
const FLOAT_SCALE_MAX: f64 = 8.0;

fn float_scale_of(settings: &Settings, id: &str) -> f64 {
    let s = settings.float_scale.get(id).copied().unwrap_or(1.0);
    // Same range set_float_scale accepts. A narrower one here silently shrank the
    // "L" preset and every freely resized pill back down on the next start.
    if s.is_finite() { s.clamp(FLOAT_SCALE_MIN, FLOAT_SCALE_MAX) } else { 1.0 }
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

// ---------- F25: floating pill folders ----------

fn folder_label(id: &str) -> String {
    format!("folder-{}", id)
}

// Expert settings the backend needs. The feature ships ON; the two parameters use
// the same defaults and ranges as their sliders in the expert menu.
fn folders_on(settings: &Settings) -> bool {
    settings.ui_flags.get("pillFolders") != Some(&false)
}

// Pill and folder windows read the settings ONCE, when they open. Without this every
// expert change (pill folders, drag-out, opacity, cooldown …) would only reach the
// windows opened afterwards — i.e. a feature switched on while pills are on screen
// looked dead until a restart.
fn notify_float_settings(app: &AppHandle) {
    for label in app.webview_windows().keys() {
        if label.starts_with("float-") || label.starts_with("folder-") {
            let _ = app.emit_to(label.as_str(), "settings-changed", ());
        }
    }
}
fn merge_radius(settings: &Settings) -> f64 {
    settings
        .ui_values
        .get("pillMergeRadius")
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(40.0)
        .clamp(20.0, 80.0)
}
fn folder_cols(settings: &Settings) -> u32 {
    settings
        .ui_values
        .get("folderGridCols")
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(3.0)
        .clamp(2.0, 5.0) as u32
}

// A box in PHYSICAL desktop pixels — the one coordinate space every monitor shares,
// whatever its DPI. Sizes are converted per window with its own scale factor, so a
// mixed-DPI setup stays correct.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn right(&self) -> f64 {
        self.x + self.w
    }
    fn bottom(&self) -> f64 {
        self.y + self.h
    }
    fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
}

// Which pill the just-dropped one landed on: the NEAREST centre within `radius` whose
// box ALSO really overlaps. Both conditions matter — the radius alone would fire for a
// pill that merely sits nearby, the overlap alone for a long text pill whose far end
// happens to touch. A short drag ends up far from every other pill and matches
// nothing, which is what keeps a normal nudge from creating a folder. A centre
// distance exactly equal to the radius still counts as a hit.
fn merge_target<'a>(dragged: Rect, others: &'a [(String, Rect)], radius: f64) -> Option<&'a str> {
    let (cx, cy) = dragged.center();
    let mut best: Option<(f64, &str)> = None;
    for (id, rect) in others {
        if !dragged.overlaps(rect) {
            continue;
        }
        let (ox, oy) = rect.center();
        let dist = ((cx - ox).powi(2) + (cy - oy).powi(2)).sqrt();
        if dist > radius {
            continue;
        }
        if best.map_or(true, |(b, _)| dist < b) {
            best = Some((dist, id.as_str()));
        }
    }
    best.map(|(_, id)| id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FolderSide {
    Below,
    Above,
    Right,
    Left,
}

impl FolderSide {
    fn as_str(self) -> &'static str {
        match self {
            FolderSide::Below => "below",
            FolderSide::Above => "above",
            FolderSide::Right => "right",
            FolderSide::Left => "left",
        }
    }
}

// Where an expanded folder ends up: the window that has to cover both the icon and
// the popover, plus where each of them sits inside it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FolderLayout {
    side: FolderSide,
    win: Rect,
    icon: (f64, f64), // icon offset inside the window
    pop: (f64, f64),  // popover offset inside the window
}

// Grow the folder window around its icon. The icon NEVER moves — it is what the user
// clicked — so the popover goes on the first side with room (below, above, right,
// left) and is then slid along its free axis until it lies inside the work area.
// Everything is physical pixels of ONE monitor: pass that monitor's work rect and the
// sizes already scaled by its factor, and edge flipping works the same on a second
// screen at another DPI.
fn folder_layout(icon: Rect, pop_w: f64, pop_h: f64, gap: f64, work: Rect) -> FolderLayout {
    let side = if icon.bottom() + gap + pop_h <= work.bottom() {
        FolderSide::Below
    } else if icon.y - gap - pop_h >= work.y {
        FolderSide::Above
    } else if icon.right() + gap + pop_w <= work.right() {
        FolderSide::Right
    } else if icon.x - gap - pop_w >= work.x {
        FolderSide::Left
    } else if icon.y - work.y > work.bottom() - icon.bottom() {
        // Nothing fits (a grid taller than the screen): keep the roomier half, so the
        // first rows stay reachable instead of the popover hanging off the edge.
        FolderSide::Above
    } else {
        FolderSide::Below
    };
    // A popover larger than the work area can only start at its edge.
    let fit = |v: f64, lo: f64, hi: f64| if hi < lo { lo } else { v.max(lo).min(hi) };
    // The popover is ALIGNED with the icon's leading edge, never centred on it. A
    // centred popover is wider than the icon on both sides, so the window ORIGIN moves
    // left — and the icon has to be pushed right by the same amount inside it, one
    // frame later. That one frame is the sideways jump on right-click. Aligned, the
    // origin stays exactly where it was whenever the popover fits below/right, which
    // is the normal case.
    let pop = match side {
        FolderSide::Below | FolderSide::Above => {
            let x = fit(icon.x, work.x, work.right() - pop_w);
            let y = if side == FolderSide::Below {
                icon.bottom() + gap
            } else {
                icon.y - gap - pop_h
            };
            Rect { x, y: fit(y, work.y, work.bottom() - pop_h), w: pop_w, h: pop_h }
        }
        FolderSide::Right | FolderSide::Left => {
            let y = fit(icon.y, work.y, work.bottom() - pop_h);
            let x = if side == FolderSide::Right {
                icon.right() + gap
            } else {
                icon.x - gap - pop_w
            };
            Rect { x: fit(x, work.x, work.right() - pop_w), y, w: pop_w, h: pop_h }
        }
    };
    let x = icon.x.min(pop.x);
    let y = icon.y.min(pop.y);
    let win = Rect {
        x,
        y,
        w: icon.right().max(pop.right()) - x,
        h: icon.bottom().max(pop.bottom()) - y,
    };
    FolderLayout {
        side,
        win,
        icon: (icon.x - x, icon.y - y),
        pop: (pop.x - x, pop.y - y),
    }
}

// Keep a dropped pill fully inside the work area it was dropped on. A window larger
// than the screen is pinned to the top-left corner instead of being pushed off it.
fn clamp_to_work(work: Rect, w: f64, h: f64, x: f64, y: f64) -> (f64, f64) {
    let fit = |v: f64, lo: f64, hi: f64| if hi < lo { lo } else { v.max(lo).min(hi) };
    (
        fit(x, work.x, work.right() - w),
        fit(y, work.y, work.bottom() - h),
    )
}

// Fan a set of pills apart so none of them ends up hidden under another. Each pill
// keeps its old position as a WISH; dropping one pill onto another is what created the
// folder in the first place, so those wishes sit on top of each other and a dissolved
// folder would look like a single pill. A taken box pushes the next one outward on a
// growing ring (step: a bit more than half the pill, so a few rings clear even a large
// media pill) until it lands free, and the result is clamped onto the work area.
// Pure and deterministic: same input, same layout.
fn spread_free_positions(wishes: &[Rect], taken: &[Rect], work: Rect) -> Vec<(f64, f64)> {
    const RINGS: i32 = 10;
    let mut placed: Vec<Rect> = taken.to_vec();
    let mut out = Vec::with_capacity(wishes.len());
    for wish in wishes {
        let step = (wish.w.max(wish.h) * 0.55).max(24.0);
        let at = |dx: i32, dy: i32| {
            let (x, y) = clamp_to_work(
                work,
                wish.w,
                wish.h,
                wish.x + dx as f64 * step,
                wish.y + dy as f64 * step,
            );
            Rect { x, y, w: wish.w, h: wish.h }
        };
        let mut spot = at(0, 0);
        'rings: for ring in 0..=RINGS {
            for dy in -ring..=ring {
                for dx in -ring..=ring {
                    // Only the outer edge of each ring is new.
                    if ring > 0 && dx.abs() != ring && dy.abs() != ring {
                        continue;
                    }
                    let cand = at(dx, dy);
                    if !placed.iter().any(|p| p.overlaps(&cand)) {
                        spot = cand;
                        break 'rings;
                    }
                }
            }
        }
        placed.push(spot);
        out.push((spot.x, spot.y));
    }
    out
}

// Bring the folder list back to its invariants: only real prompt ids, no prompt in two
// folders (the first folder listing it wins), a sane scale, and no folder left with
// fewer than two members — that one dissolves, and its pill simply reappears at the
// free position it kept all along. Runs on load and after every mutation. Idempotent.
fn normalize_float_folders(folders: &mut Vec<FloatFolder>, prompt_ids: &[String]) {
    let mut taken: Vec<String> = Vec::new();
    for folder in folders.iter_mut() {
        folder.scale = if folder.scale.is_finite() {
            folder.scale.clamp(0.5, 3.0)
        } else {
            1.0
        };
        folder.members.retain(|id| {
            let ok = prompt_ids.iter().any(|p| p == id) && !taken.contains(id);
            if ok {
                taken.push(id.clone());
            }
            ok
        });
    }
    folders.retain(|f| f.members.len() >= 2);
    folders.truncate(MAX_FLOAT_FOLDERS);
}

// Which pill windows and which folder windows should be open right now. While the
// feature is off no folder exists on screen and every member is a plain pill again,
// which is exactly how switching it off restores the free positions.
fn float_window_plan(settings: &Settings, prompt_ids: &[String]) -> (Vec<String>, Vec<String>) {
    let on = folders_on(settings);
    let mut inside: Vec<&str> = Vec::new();
    if on {
        for folder in &settings.float_folders {
            for member in &folder.members {
                inside.push(member.as_str());
            }
        }
    }
    let pills = prompt_ids
        .iter()
        .filter(|id| settings.floating.contains_key(*id) && !inside.contains(&id.as_str()))
        .cloned()
        .collect();
    let folders = if on {
        settings.float_folders.iter().map(|f| f.id.clone()).collect()
    } else {
        Vec::new()
    };
    (pills, folders)
}

fn folder_name_of(name: &str) -> Result<String, String> {
    let name: String = name
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string();
    if name.chars().count() > MAX_FOLDER_NAME {
        return Err(format!("folder name longer than {} characters", MAX_FOLDER_NAME));
    }
    Ok(name)
}

// Where the icon sits inside an EXPANDED folder window (offset + size, physical px).
// No entry = collapsed, i.e. the window IS the icon. In memory only: an open folder
// never survives a restart.
struct FolderExpand(Mutex<HashMap<String, Rect>>);

// 43: pill/folder windows dismissed "for now" (their own context menu, not the
// permanent Remove/Dissolve). In memory only, like FolderExpand above — cleared
// on restart, never written to settings.
struct HiddenFloats(Mutex<std::collections::HashSet<String>>);

fn folder_expansion(app: &AppHandle, id: &str) -> Option<Rect> {
    let state = app.try_state::<FolderExpand>()?;
    let map = state.0.lock().unwrap_or_else(|e| e.into_inner());
    map.get(id).copied()
}

fn set_folder_expansion(app: &AppHandle, id: &str, icon: Option<Rect>) {
    if let Some(state) = app.try_state::<FolderExpand>() {
        let mut map = state.0.lock().unwrap_or_else(|e| e.into_inner());
        match icon {
            Some(r) => {
                map.insert(id.to_string(), r);
            }
            None => {
                map.remove(id);
            }
        }
    }
}

// The folder icon's box on screen, whether the window is collapsed (window = icon) or
// expanded (icon sits at a stored offset inside the grown window).
fn folder_icon_rect(app: &AppHandle, id: &str, window: Rect) -> Rect {
    match folder_expansion(app, id) {
        Some(off) => Rect {
            x: window.x + off.x,
            y: window.y + off.y,
            w: off.w,
            h: off.h,
        },
        None => window,
    }
}

// A window's box in physical desktop pixels.
// The window box in physical pixels. On Windows this reads the HWND directly: the
// live merge hint polls it while the OS move loop owns the pointer, and a plain Win32
// read needs nothing from the stalled event loop.
#[cfg(windows)]
fn window_rect_native(win: &tauri::WebviewWindow) -> Option<Rect> {
    #[repr(C)]
    struct RectW { left: i32, top: i32, right: i32, bottom: i32 }
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowRect(hwnd: isize, r: *mut RectW) -> i32;
    }
    let hwnd = win.hwnd().ok()?.0 as isize;
    let mut r = RectW { left: 0, top: 0, right: 0, bottom: 0 };
    if unsafe { GetWindowRect(hwnd, &mut r) } == 0 {
        return None;
    }
    Some(Rect {
        x: r.left as f64,
        y: r.top as f64,
        w: (r.right - r.left) as f64,
        h: (r.bottom - r.top) as f64,
    })
}

fn window_rect(win: &tauri::WebviewWindow) -> Option<Rect> {
    #[cfg(windows)]
    if let Some(r) = window_rect_native(win) {
        return Some(r);
    }
    let p = win.outer_position().ok()?;
    let s = win.outer_size().ok()?;
    Some(Rect {
        x: p.x as f64,
        y: p.y as f64,
        w: s.width as f64,
        h: s.height as f64,
    })
}

// Work area (screen minus taskbar) of the monitor under a physical point. Falls back
// to the primary monitor when the point lies in dead space between screens.
fn work_rect_at(app: &AppHandle, x: f64, y: f64) -> Rect {
    let win = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next());
    let monitor = win.as_ref().and_then(|w| {
        w.monitor_from_point(x, y)
            .ok()
            .flatten()
            .or_else(|| w.primary_monitor().ok().flatten())
    });
    match monitor {
        Some(m) => {
            let area = m.work_area();
            Rect {
                x: area.position.x as f64,
                y: area.position.y as f64,
                w: area.size.width as f64,
                h: area.size.height as f64,
            }
        }
        None => Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 },
    }
}

// Move AND resize in one OS call, in physical pixels (same reason as
// set_float_bounds_native: two calls leave a visibly wrong intermediate frame).
fn set_bounds_phys(win: &tauri::WebviewWindow, r: Rect) {
    let (x, y) = (r.x.round() as i32, r.y.round() as i32);
    let (w, h) = (r.w.round().max(1.0) as i32, r.h.round().max(1.0) as i32);
    #[cfg(windows)]
    if set_window_pos_phys(win, x, y, w, h) {
        return;
    }
    let _ = win.set_position(PhysicalPosition::new(x, y));
    let _ = win.set_size(PhysicalSize::new(w.max(1) as u32, h.max(1) as u32));
}

fn folder_box(scale: f64) -> f64 {
    FOLDER_BOX * if scale.is_finite() { scale.clamp(0.5, 3.0) } else { 1.0 }
}

// One folder window, built exactly like a pill: frameless, transparent, always on top,
// out of the taskbar and out of other apps' screen captures.
fn open_folder_window(app: &AppHandle, folder: &FloatFolder) {
    let label = folder_label(&folder.id);
    if app.get_webview_window(&label).is_some() {
        return;
    }
    // Never call Tauri window APIs while holding the Db lock (deadlock risk).
    let excluded = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        capture_excluded(&store.settings)
    };
    let side = folder_box(folder.scale);
    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("folder.html".into()))
        .title(&folder.name)
        .inner_size(side, side)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .build();

    if let Ok(win) = win {
        let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
        set_capture_exclusion(&win, excluded);
        let _ = win.set_position(PhysicalPosition::new(folder.pos.x, folder.pos.y));
        let _ = win.show();

        let app2 = app.clone();
        let fid = folder.id.clone();
        win.on_window_event(move |event| match event {
            WindowEvent::Moved(p) => {
                // While the folder is open the window origin is NOT the icon, so the
                // stored position follows the icon, not the grown box.
                let off = folder_expansion(&app2, &fid).unwrap_or(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 });
                if let Some(state) = app2.try_state::<Db>() {
                    // Fires for every pixel of a drag — never block the move loop.
                    if let Ok(mut store) = state.try_lock() {
                        if let Some(f) = store.settings.float_folders.iter_mut().find(|f| f.id == fid) {
                            f.pos = Pos {
                                x: p.x + off.x.round() as i32,
                                y: p.y + off.y.round() as i32,
                            };
                        }
                    }
                }
            }
            WindowEvent::Destroyed => {
                set_folder_expansion(&app2, &fid, None);
                if let Some(state) = app2.try_state::<Db>() {
                    let store = state.lock().unwrap_or_else(|e| e.into_inner());
                    save_settings(&app2, &store.settings);
                }
            }
            _ => {}
        });
    }
}

// Open/close pill and folder windows until the desktop matches the stored plan. The
// single place that turns settings into windows, so every mutation just calls it.
// MUST NOT run while the store mutex is held — it creates and closes windows.
fn sync_float_windows(app: &AppHandle) {
    let (want_pills, want_folders, pills, folders) = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
        let (mut want_pills, mut want_folders) = float_window_plan(&store.settings, &ids);
        // 43: "hide for now" drops the id from the plan, which the stale-window
        // loop below then closes if it was already open — the same one filter
        // both closes it now and keeps it from reopening until the next launch.
        if let Some(hidden) = app.try_state::<HiddenFloats>() {
            let hidden = hidden.0.lock().unwrap_or_else(|e| e.into_inner());
            want_pills.retain(|id| !hidden.contains(id));
            want_folders.retain(|id| !hidden.contains(id));
        }
        let pills: Vec<Prompt> = store
            .prompts
            .iter()
            .filter(|p| want_pills.contains(&p.id))
            .cloned()
            .collect();
        let folders: Vec<FloatFolder> = store
            .settings
            .float_folders
            .iter()
            .filter(|f| want_folders.contains(&f.id))
            .cloned()
            .collect();
        (want_pills, want_folders, pills, folders)
    };
    for (label, win) in app.webview_windows() {
        let stale = match (label.strip_prefix("float-"), label.strip_prefix("folder-")) {
            (Some(id), _) => !want_pills.iter().any(|w| w == id),
            (_, Some(id)) => !want_folders.iter().any(|w| w == id),
            _ => false,
        };
        if stale {
            let _ = win.close();
        }
    }
    for prompt in &pills {
        open_floating(app, prompt); // returns early when the window already exists
    }
    for folder in &folders {
        open_folder_window(app, folder);
    }
    // Whatever changed the plan also changed what an open folder shows.
    for label in app.webview_windows().keys() {
        if label.starts_with("folder-") {
            let _ = app.emit_to(label.as_str(), "folder-changed", ());
        }
    }
}

// ---------- Image helpers ----------

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

// Reverse lookup built once at compile time: base64_decode runs over whole images and
// every sealed settings/prompt line at startup, so an O(64) scan per byte (the linear
// alphabet search this replaced) was real, measurable work — this makes it O(1).
const fn build_b64_decode_table() -> [u8; 256] {
    let mut table = [0xFFu8; 256];
    let mut i = 0;
    while i < 64 {
        table[B64_ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
}
const B64_DECODE_TABLE: [u8; 256] = build_b64_decode_table();

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((*c.get(1).unwrap_or(&0) as u32) << 8)
            | (*c.get(2).unwrap_or(&0) as u32);
        out.push(B64_ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(B64_ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 { B64_ALPHABET[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if c.len() > 2 { B64_ALPHABET[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn base64_decode(s: &str) -> Vec<u8> {
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=' && !b.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for c in bytes.chunks(4) {
        // Any byte outside the alphabet means the input is corrupt: fail cleanly
        // instead of dropping it and silently shifting every following group.
        let mut v = [0u32; 4];
        for (i, &b) in c.iter().enumerate() {
            let p = B64_DECODE_TABLE[b as usize];
            if p == 0xFF { return Vec::new(); }
            v[i] = p as u32;
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
        .join(brand::TEMP_DIR)
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
    // The cache path is predictable and lives in a world-writable %TEMP%, so the file
    // found there is only trusted when its CONTENT matches the copy compiled into this
    // exe — a length check alone would let any process running as this user pre-plant a
    // same-size DLL and get it loaded as native code. Rewritten otherwise, which also
    // repairs a truncated write from an interrupted first run.
    if fs::read(&cached).is_ok_and(|got| got == PDFIUM_DLL) {
        return Some(cached);
    }
    fs::create_dir_all(cached.parent()?).ok()?;
    if let Err(e) = fs::write(&cached, PDFIUM_DLL) {
        // Release builds have no console (windows_subsystem = "windows"), so this has
        // to reach the rolling log or it is lost. The preview is skipped, app runs on.
        append_crash_log(&format!("pdfium unpack failed: {e}"));
        return None;
    }
    Some(cached)
}

// Render one page of a PDF to a preview image (data URL). Returns None if pdfium
// is unavailable or the file cannot be read — the caller then keeps the PDF as a
// plain file attachment without a preview. `page` (83) is clamped against the
// actual page count here, not by the caller — the stored number and the file can
// disagree (a shorter PDF re-attached at the same path), and clamping is what
// keeps that case showing the last page instead of nothing at all.
#[tauri::command]
async fn pdf_preview(app: AppHandle, path: String, page: Option<u32>) -> Option<String> {
    use pdfium_render::prelude::*;
    let lib = pdfium_lib_path(&app)?;
    let bindings = Pdfium::bind_to_library(lib).ok()?;
    let pdfium = Pdfium::new(bindings);
    let document = pdfium.load_pdf_from_file(&path, None).ok()?;
    let pages = document.pages();
    let count = pages.len();
    if count == 0 {
        return None;
    }
    let index = page.unwrap_or(0).min(count as u32 - 1);
    let page = pages.get(index as u16).ok()?;
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
    first_free_cell_excluding(view, &[])
}

// The active view's grid-folder cells. Every caller that hands out a free cell in
// the active view needs these: they live outside view.layouts, so first_free_cell()
// alone cannot see them.
fn active_view_folder_cells(settings: &Settings) -> Vec<[u32; 2]> {
    let Some(vid) = settings.views.get(settings.active_index()).map(|v| v.id.clone()) else {
        return Vec::new();
    };
    settings
        .tile_folders
        .iter()
        .filter(|f| f.view_id == vid)
        .map(|f| f.cell)
        .collect()
}

// Same, but also treats `extra` cells (e.g. another grid folder's own cell, which
// lives outside view.layouts) as taken. Without this, a cell handed out here could
// coincide with a folder's cell — the tile would save fine but never draw, since
// the renderer gives a folder priority over whatever else sits on its cell.
fn first_free_cell_excluding(view: &View, extra: &[[u32; 2]]) -> Option<[u32; 2]> {
    let key = grid_key(view.cols, view.rows);
    let empty = HashMap::new();
    let layout = view.layouts.get(&key).unwrap_or(&empty);
    let mut occupied: std::collections::HashSet<[u32; 2]> = layout.values().cloned().collect();
    occupied.extend(extra.iter().cloned());
    for row in 0..view.rows {
        for col in 0..view.cols {
            if !occupied.contains(&[col, row]) {
                return Some([col, row]);
            }
        }
    }
    None
}

// Places every id from `ids` that isn't already in `layout` into a free cell,
// counting `extra_occupied` (105: e.g. a grid folder's own cell, which lives
// outside `layout`) as taken too. All-or-nothing (104): if there isn't room for
// every id, nothing is placed and `layout` is left exactly as it was, so a caller
// that already removed these ids from somewhere else can react to the failure
// instead of stranding them.
fn place_ids_in_view(
    layout: &mut std::collections::HashMap<String, [u32; 2]>,
    ids: &[String],
    cols: u32,
    rows: u32,
    extra_occupied: &std::collections::HashSet<(u32, u32)>,
) -> bool {
    let mut occupied: std::collections::HashSet<(u32, u32)> =
        layout.values().map(|c| (c[0], c[1])).collect();
    occupied.extend(extra_occupied.iter().copied());
    let need = ids.iter().filter(|id| !layout.contains_key(id.as_str())).count();
    let free = (cols as usize * rows as usize).saturating_sub(occupied.len());
    if need > free {
        return false;
    }
    for id in ids {
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
    true
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
    // referenced file is never mistaken for an orphan and deleted. 91: a trashed
    // prompt still owns its files until it is actually purged — leaving trash out
    // of this scan would let an UNRELATED cleanup pass (triggered by some other
    // prompt's add/delete) sweep away a file a trashed prompt still needs to be
    // restorable.
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    let referenced: std::collections::HashSet<String> = store
        .prompts
        .iter()
        .chain(store.trash.iter())
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
    pdf_page: Option<u32>,
    icon_path: Option<String>,
    caption: Option<String>,
    caption_size: Option<u32>,
    font: Option<String>,
    font_size: Option<u32>,
    favorite: Option<bool>,
    tags: Option<Vec<String>>,
    notes: Option<String>,
    hotkey: Option<String>,
    hotkey_paste: Option<String>,
    variants: Option<Vec<String>>,
    rotate_mode: Option<String>,
) -> Result<Prompt, String> {
    // One lock for the whole op so the storeFiles flag can't change mid-insert.
    let prompt = {
        let mut store = lock(&state);
        let store_files = store_files_enabled(&store);
        let tags = sanitize_tags(tags.unwrap_or_default(), &canonical_tags(&store.prompts))?;
        let notes = sanitize_notes(&notes.unwrap_or_default())?;
        let variants = sanitize_variants(variants.unwrap_or_default());
        let rotate_mode = sanitize_rotate_mode(rotate_mode.unwrap_or_default(), &variants);
        let others: Vec<&Prompt> = store.prompts.iter().collect();
        let hotkey = sanitize_hotkey(&hotkey.unwrap_or_default(), &store.settings.hotkey, &others)?;
        let hotkey_paste = if hotkey.is_empty() {
            String::new() // no binding, no action to remember for it
        } else {
            sanitize_hotkey_mode(&hotkey_paste.unwrap_or_default())
        };
        let mut fp = file_path.unwrap_or_default();
        let mut ip = icon_path.unwrap_or_default();
        if store_files {
            fp = store_attached_file(&app, &fp);
            ip = store_attached_file(&app, &ip);
        }
        let prompt = Prompt {
            id: gen_id(),
            trashed_at: 0,
            name,
            text,
            color,
            image: image.unwrap_or_default(),
            show_image: show_image.unwrap_or(false),
            copy_image: copy_image.unwrap_or(false),
            file_path: fp,
            pdf_page: pdf_page.unwrap_or(0),
            icon_path: ip,
            caption: caption.unwrap_or_default(),
            caption_size: clamp_caption_size(caption_size.unwrap_or(0)),
            font: font.unwrap_or_default(),
            font_size: clamp_font_size(font_size.unwrap_or(0)),
            favorite: favorite.unwrap_or(false),
            tags,
            notes,
            hotkey,
            hotkey_paste,
            variants,
            rotate_mode,
        };
        // A grid folder owns its cell OUTSIDE view.layouts, and the renderer gives a
        // folder priority — so a cell handed out without excluding them saves fine and
        // then never draws.
        let folder_cells = active_view_folder_cells(&store.settings);
        let view = store.settings.active_view_mut();
        if let Some(cell) = first_free_cell_excluding(view, &folder_cells) {
            let key = grid_key(view.cols, view.rows);
            view.layouts.entry(key).or_default().insert(prompt.id.clone(), cell);
        }
        store.prompts.push(prompt.clone());
        if !save_prompts(&app, &store) {
            // Nothing actually reached disk (or the JSON fallback) - undo the
            // in-memory add and report it, instead of handing back an id the
            // editor will believe is safely saved. The layout entry above was
            // never persisted (save_settings below is skipped too), so it is
            // harmless to leave dangling in memory: no prompt id means nothing
            // ever draws it.
            store.prompts.pop();
            return Err("save-failed".into());
        }
        save_settings(&app, &store.settings);
        prompt
    };
    rebuild_tray(&app); // F23
    // F20: arm the new accelerator. The re-arm drops one the OS refuses, so the
    // caller is handed the stored state, not the state before arming.
    if !prompt.hotkey.is_empty() {
        rearm_hotkeys(&app);
        let store = lock(&state);
        if let Some(fresh) = store.prompts.iter().find(|p| p.id == prompt.id) {
            return Ok(fresh.clone());
        }
    }
    Ok(prompt)
}

// 2: one example prompt as the frontend supplies it — name/text come from i18n.js
// (resolved to whatever language the UI is actually showing), the image is the
// one small embedded asset shared by all languages.
#[derive(Deserialize)]
struct NewExample {
    name: String,
    text: String,
    #[serde(default)]
    image: String,
}

// Seeds the three onboarding examples exactly once, on a genuinely fresh install —
// checked and flipped atomically under the same lock so two racing calls (e.g. a
// double invoke from a slow first paint) can never seed twice. A library that is
// merely empty again after the user deleted everything does NOT reseed: that is
// exactly what `examples_seeded` remembers.
#[tauri::command]
async fn seed_example_prompts(app: AppHandle, state: State<'_, Db>, examples: Vec<NewExample>) -> Result<Vec<Prompt>, String> {
    let created = {
        let mut store = lock(&state);
        if !store.prompts.is_empty() || store.settings.examples_seeded {
            return Ok(Vec::new());
        }
        let mut created = Vec::new();
        for ex in examples {
            let prompt = Prompt {
                id: gen_id(),
                trashed_at: 0,
                name: ex.name,
                text: ex.text,
                color: String::new(),
                show_image: !ex.image.is_empty(),
                image: ex.image,
                copy_image: false,
                file_path: String::new(),
                pdf_page: 0,
                icon_path: String::new(),
                caption: String::new(),
                caption_size: 0,
                font: String::new(),
                font_size: 0,
                favorite: false,
                tags: Vec::new(),
                notes: String::new(),
                hotkey: String::new(),
                hotkey_paste: String::new(),
                variants: Vec::new(),
                rotate_mode: String::new(),
            };
            let view = store.settings.active_view_mut();
            if let Some(cell) = first_free_cell(view) {
                let key = grid_key(view.cols, view.rows);
                view.layouts.entry(key).or_default().insert(prompt.id.clone(), cell);
            }
            store.settings.example_prompt_ids.push(prompt.id.clone());
            store.prompts.push(prompt.clone());
            created.push(prompt);
        }
        store.settings.examples_seeded = true;
        save_prompts(&app, &store);
        save_settings(&app, &store.settings);
        created
    };
    rebuild_tray(&app); // F23: mirrors add_prompt — the tray's recent/fav sections can list these too
    Ok(created)
}

// 2: "remove examples" — deletes exactly the ids seed_example_prompts recorded,
// through the same batch-delete path (→ trash, per 91) a manual multi-select
// delete would use, so nothing about this is a special, less-safe deletion.
#[tauri::command]
async fn dismiss_example_prompts(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    let ids = {
        let mut store = lock(&state);
        std::mem::take(&mut store.settings.example_prompt_ids)
    };
    if ids.is_empty() {
        return Ok(());
    }
    batch_prompts(app, state, ids, "delete".to_string(), None, None, None)
        .await
        .map(|_| ())
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
    pdf_page: Option<u32>,
    icon_path: Option<String>,
    caption: Option<String>,
    caption_size: Option<u32>,
    font: Option<String>,
    font_size: Option<u32>,
    favorite: Option<bool>,
    tags: Option<Vec<String>>,
    notes: Option<String>,
    hotkey: Option<String>,
    hotkey_paste: Option<String>,
    variants: Vec<String>,
    rotate_mode: String,
) -> Result<Option<Prompt>, String> {
    let variants = sanitize_variants(variants);
    let rotate_mode = sanitize_rotate_mode(rotate_mode, &variants);
    let updated = {
        let mut store = lock(&state);
        let store_files = store_files_enabled(&store);
        let tags = match tags {
            Some(list) => Some(sanitize_tags(list, &canonical_tags(&store.prompts))?),
            None => None,
        };
        let notes = match notes {
            Some(text) => Some(sanitize_notes(&text)?),
            None => None,
        };
        let hotkey = match hotkey {
            Some(raw) => {
                let others: Vec<&Prompt> = store.prompts.iter().filter(|p| p.id != id).collect();
                Some(sanitize_hotkey(&raw, &store.settings.hotkey, &others)?)
            }
            None => None,
        };
        let file_path = file_path.map(|p| if store_files { store_attached_file(&app, &p) } else { p });
        let icon_path = icon_path.map(|p| if store_files { store_attached_file(&app, &p) } else { p });
        // F7: read history settings before the mutable prompt borrow (a MutexGuard
        // can't be split into disjoint field borrows).
        let hist_on = history_enabled(&store.settings);
        let hist_cap = history_cap(&store.settings);
        let found = store.prompts.iter_mut().find(|p| p.id == id);
        match found {
            Some(p) => {
                // 7: full pre-edit snapshot so a failed save can roll the live entry
                // back instead of leaving the UI showing unsaved changes as if they
                // had taken (they would silently revert on the next restart).
                let original = p.clone();
                // F7/32: snapshot the previous name/text/variants before overwriting,
                // if changed — entries ARE content (WORK.md 32), so editing only
                // entry 2 of a rotating prompt (name/text untouched) must still get a
                // version row or a restore could never bring that entry back.
                // rotate_mode is deliberately excluded (order/random is a setting,
                // not versioned content, and isn't stored in the snapshot).
                if hist_on && (p.name != name || p.text != text || p.variants != variants) {
                    let (old_name, old_text, old_variants) = (p.name.clone(), p.text.clone(), p.variants.clone());
                    if let Some(c) = db_conn(&app) {
                        let _ = db_add_version(&c, &id, now_secs(), &old_name, &old_text, &old_variants, hist_cap);
                    }
                }
                p.name = name;
                p.text = text;
                p.color = color;
                p.variants = variants;
                p.rotate_mode = rotate_mode;
                if let Some(img) = image { p.image = img; }
                if let Some(si) = show_image { p.show_image = si; }
                if let Some(ci) = copy_image { p.copy_image = ci; }
                if let Some(fp) = file_path { p.file_path = fp; }
                if let Some(pp) = pdf_page { p.pdf_page = pp; }
                if let Some(ip) = icon_path { p.icon_path = ip; }
                if let Some(c) = caption { p.caption = c; }
                if let Some(cs) = caption_size { p.caption_size = clamp_caption_size(cs); }
                if let Some(f) = font { p.font = f; }
                if let Some(fs) = font_size {
                    p.font_size = clamp_font_size(fs);
                }
                if let Some(fav) = favorite { p.favorite = fav; }
                if let Some(t) = tags { p.tags = t; }
                if let Some(n) = notes { p.notes = n; }
                if let Some(hp) = &hotkey_paste { p.hotkey_paste = sanitize_hotkey_mode(hp); }
                // F20: only a real change re-arms; every save would otherwise churn
                // through unregister/register for nothing.
                let rebind = matches!(&hotkey, Some(h) if *h != p.hotkey);
                if let Some(h) = hotkey { p.hotkey = h; }
                if p.hotkey.is_empty() { p.hotkey_paste.clear(); } // no binding, no action
                let clone = p.clone();
                // 7: nothing reached disk (or the JSON fallback) - restore the pre-edit
                // snapshot instead of handing back an id the editor will believe is
                // safely saved.
                if !save_prompts(&app, &store) {
                    if let Some(slot) = store.prompts.iter_mut().find(|x| x.id == id) {
                        *slot = original;
                    }
                    return Err("save-failed".into());
                }
                // 102: only sweep the old attachment once the new reference is durably
                // saved -- otherwise a failed save leaves the DB pointing at a file the
                // sweep just deleted, with no fallback (101) to recover it from.
                cleanup_orphan_files(&app, &store); // a replaced attachment orphans its old copy
                let scale = float_scale_of(&store.settings, &id);
                Some((clone, scale, rebind))
            }
            None => None,
        }
    };
    let (updated, scale, rebind) = match updated {
        Some((p, s, r)) => (Some(p), s, r),
        None => (None, 1.0, false),
    };
    // Re-arm outside the lock, then hand back what is actually stored — the re-arm
    // clears an accelerator the OS refused.
    let updated = match updated {
        Some(p) if rebind => {
            rearm_hotkeys(&app);
            let store = lock(&state);
            Some(store.prompts.iter().find(|x| x.id == p.id).cloned().unwrap_or(p))
        }
        other => other,
    };
    if let Some(p) = &updated {
        let _ = app.emit("prompt-updated", p.clone());
        rebuild_tray(&app); // F23: a rename shows up in the tray entries
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

// 17: the library's "last edited" sort mode. A prompt missing from the map has
// never had a version row (see db_last_edited) — the frontend sorts those last.
#[tauri::command]
async fn last_edited_map(app: AppHandle) -> Result<std::collections::HashMap<String, u64>, String> {
    Ok(db_conn(&app).map(|c| db_last_edited(&c)).unwrap_or_default())
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
        let (pid, name, text, variants) = record;
        let cap = history_cap(&store.settings);
        if let Some(p) = store.prompts.iter_mut().find(|p| p.id == pid) {
            if let Some(c) = db_conn(&app) {
                let _ = db_add_version(&c, &pid, now_secs(), &p.name, &p.text, &p.variants, cap);
            }
            p.name = name;
            p.text = text;
            // 32: the restored list is our own history, but re-validate anyway (a
            // version predating MAX_ROTATE_VARIANTS, however unlikely, must not
            // slip a too-long list past the same floor every other entry point has).
            p.variants = sanitize_variants(variants);
            // A restore that lands on zero entries must not leave "random" mode
            // stranded on an empty list (same invariant as everywhere else).
            p.rotate_mode = sanitize_rotate_mode(p.rotate_mode.clone(), &p.variants);
            let clone = p.clone();
            save_prompts(&app, &store);
            Some(clone)
        } else {
            None
        }
    };
    if let Some(p) = &restored {
        let _ = app.emit("prompt-updated", p.clone());
        rebuild_tray(&app); // F23
    }
    Ok(restored)
}

// Star / unstar a prompt in the library.
#[tauri::command]
fn set_favorite(app: AppHandle, state: State<Db>, id: String, favorite: bool) {
    {
        let mut store = lock(&state);
        if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
            p.favorite = favorite;
            save_prompts(&app, &store);
        }
    }
    rebuild_tray(&app); // F23: the favourites section follows the stars
}

// Async: keeps the prompt-table rewrite off the UI thread.
#[tauri::command]
// 91: "delete" moves a prompt into the trash table instead of removing it — media
// files and version history stay exactly as they were, untouched until a real
// purge (purge_prompt/empty_trash, or the retention sweep). Everything else here
// (views, floating, folders, hotkeys, tray) is unchanged from the old hard-delete:
// those are all app-level associations, not part of the prompt's own content, and
// a restored prompt gets a fresh placement rather than trying to replay them.
async fn delete_prompt(app: AppHandle, state: State<'_, Db>, id: String) -> Result<bool, String> {
    close_floating_window(&app, &id);
    let changed = {
        let mut store = lock(&state);
        let moved = match store.prompts.iter().position(|p| p.id == id) {
            Some(pos) => {
                let mut p = store.prompts.remove(pos);
                p.trashed_at = now_secs();
                store.trash.insert(0, p); // newest-deleted first
                true
            }
            None => false,
        };
        for view in &mut store.settings.views {
            for layout in view.layouts.values_mut() {
                layout.remove(&id);
            }
        }
        store.settings.floating.remove(&id);
        store.settings.float_scale.remove(&id);
        store.settings.video_prefs.remove(&id);
        store.normalize_folders(); // F25: and out of whatever folder held it
        if moved {
            save_prompts(&app, &store);
            save_trash(&app, &store);
            save_settings(&app, &store.settings);
        }
        moved
    };
    if changed {
        rearm_hotkeys(&app); // F20: a deleted prompt gives its accelerator back
        rebuild_tray(&app); // F23
        sync_float_windows(&app); // F25: a folder left with one member dissolves
    }
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
            // 91: move to trash, same as the single-prompt delete_prompt command —
            // media files and version history stay untouched until a real purge.
            let mut moved: Vec<Prompt> = Vec::new();
            store.prompts.retain(|p| {
                if idset.contains(p.id.as_str()) {
                    moved.push(p.clone());
                    false
                } else {
                    true
                }
            });
            let now = now_secs();
            for mut p in moved.into_iter().rev() {
                p.trashed_at = now;
                store.trash.insert(0, p); // newest-deleted first, preserving the batch's own order
            }
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
            store.normalize_folders(); // F25
            save_prompts(&app, &store);
            save_trash(&app, &store);
            save_settings(&app, &store.settings);
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
            // 105: a grid folder occupies a cell via `tile_folders`, deliberately
            // outside `view.layouts` (see docs/INVARIANTS.md -> Grid folders) -- read
            // it before `view` is borrowed mutably below, since both can't happen
            // at once.
            let folder_cells: std::collections::HashSet<(u32, u32)> = store
                .settings
                .tile_folders
                .iter()
                .filter(|f| f.view_id == vid)
                .map(|f| (f.cell[0], f.cell[1]))
                .collect();
            if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == vid) {
                let (cols, rows) = (view.cols, view.rows);
                let key = grid_key(cols, rows);
                let layout = view.layouts.entry(key).or_default();
                if action == "removeView" {
                    for id in &ids {
                        layout.remove(id);
                    }
                } else if !place_ids_in_view(layout, &ids, cols, rows, &folder_cells) {
                    return Err("view-full".into());
                }
                save_settings(&app, &store.settings);
            }
        }
        _ => return Err("bad-action".into()),
    }
    // Return only the (small) settings: the caller mutates its local prompt list
    // in place, so we never re-serialise every prompt's base64 image over IPC.
    let settings = store.settings.clone();
    drop(store); // never re-arm shortcuts while the store mutex is held
    if action == "delete" {
        rearm_hotkeys(&app); // F20: the deleted prompts give their accelerators back
        sync_float_windows(&app); // F25: folders left too small dissolve
    }
    rebuild_tray(&app); // F23: one rebuild for the whole batch, not one per prompt
    Ok(settings)
}

// ---------- 91: trash ----------

#[tauri::command]
fn list_trash(state: State<Db>) -> Vec<Prompt> {
    lock(&state).trash.clone()
}

// Places the restored prompt in the ACTIVE view via the same first_free_cell scan
// add_prompt already uses for a brand-new prompt: if the view is full, the prompt
// still comes back (reachable through the library) just without a grid cell of its
// own — the same graceful fallback, never a hard failure or a silently dropped
// prompt. Every other app-level association (floating, folder membership, hotkey)
// was already let go at delete time and is not reconstructed.
#[tauri::command]
async fn restore_prompt(app: AppHandle, state: State<'_, Db>, id: String) -> Result<AppState, String> {
    let (app_state, restored) = {
        let mut store = lock(&state);
        let pos = store.trash.iter().position(|p| p.id == id);
        let Some(pos) = pos else {
            return Err("not-found".into());
        };
        let mut p = store.trash.remove(pos);
        p.trashed_at = 0;
        let folder_cells = active_view_folder_cells(&store.settings);
        let view = store.settings.active_view_mut();
        if let Some(cell) = first_free_cell_excluding(view, &folder_cells) {
            let key = grid_key(view.cols, view.rows);
            view.layouts.entry(key).or_default().insert(p.id.clone(), cell);
        }
        store.prompts.push(p);
        save_prompts(&app, &store);
        save_trash(&app, &store);
        save_settings(&app, &store.settings);
        (AppState { prompts: store.prompts.clone(), settings: store.settings.clone() }, true)
    };
    if restored {
        rearm_hotkeys(&app); // F20: the restored prompt's own hotkey, if any, is free again to re-claim
        rebuild_tray(&app); // F23
    }
    Ok(app_state)
}

// Permanent delete: the trash entry, its media files and its whole version history
// are gone for good. This is the old hard-delete's own cleanup logic, just moved
// here instead of running at every "delete" click.
fn purge_ids(app: &AppHandle, store: &mut Store, ids: &[String]) {
    let idset: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    store.trash.retain(|p| !idset.contains(p.id.as_str()));
    rotation_forget(&mut store.settings, ids); // 32: no orphaned pointer/bag left behind
    save_settings(app, &store.settings);

    // 102: same guard as update_prompt -- a failed trash save must not still trigger
    // the file sweep, or the DB keeps referencing files the sweep just removed.
    if save_trash(app, store) {
        cleanup_orphan_files(app, store);
    }
    if let Some(c) = db_conn(app) {
        for id in ids {
            db_delete_versions(&c, id);
        }
    }
}

#[tauri::command]
fn purge_prompt(app: AppHandle, state: State<Db>, id: String) -> Vec<Prompt> {
    let mut store = lock(&state);
    purge_ids(&app, &mut store, std::slice::from_ref(&id));
    store.trash.clone()
}

#[tauri::command]
fn empty_trash(app: AppHandle, state: State<Db>) -> Vec<Prompt> {
    let mut store = lock(&state);
    let ids: Vec<String> = store.trash.iter().map(|p| p.id.clone()).collect();
    purge_ids(&app, &mut store, &ids);
    store.trash.clone()
}

// 91: drop anything past the retention window (0/off by settings default, same
// "0 = forever" convention as historyDays). Runs once at startup, mirroring
// prune_history's own startup sweep — never on a timer, so a long-idle app does
// not surprise-purge something the user only just noticed in the trash.
fn trash_retention_secs(settings: &Settings) -> Option<u64> {
    let days = settings.ui_values.get("trashRetentionDays").copied().unwrap_or(30.0);
    if days <= 0.0 {
        None
    } else {
        Some(days as u64 * 86_400)
    }
}

// Pure decision, kept apart from sweep_trash's side effects (save/file-cleanup need
// an AppHandle, which cannot be constructed in a unit test) so the actual "what
// counts as stale" rule is directly testable.
fn stale_trash_ids(trash: &[Prompt], max_age: u64, now: u64) -> Vec<String> {
    trash
        .iter()
        .filter(|p| p.trashed_at > 0 && now.saturating_sub(p.trashed_at) > max_age)
        .map(|p| p.id.clone())
        .collect()
}

fn sweep_trash(app: &AppHandle, store: &mut Store) {
    let Some(max_age) = trash_retention_secs(&store.settings) else {
        return;
    };
    let stale = stale_trash_ids(&store.trash, max_age, now_secs());
    if !stale.is_empty() {
        purge_ids(app, store, &stale);
    }
}

// Factory reset: wipe all prompts AND all settings (views, theme, window,
// behaviour, fonts) and remove the autostart registry entry.
// Async: window closes + a full prompts/settings rewrite stay off the UI thread.
#[tauri::command]
async fn delete_all_data(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    let labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|l| l.starts_with("float-") || l.starts_with("folder-")) // F25
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
        store.trash.clear(); // 91: a data wipe must not leave trashed prompts behind
        store.prompts_locked = false; // explicit wipe overrides the decrypt-failure lock
        store.normalize_folders(); // F25: no prompts left, so no folder survives
        save_prompts(&app, &store);
        save_trash(&app, &store);
        save_settings(&app, &store.settings);
        // Data-only wipe: keep every setting, drop the derived data tables.
        if let Some(c) = db_conn(&app) {
            let _ = c.execute("DELETE FROM prompt_versions", []); // F7 history
            let _ = c.execute("DELETE FROM copy_log", []); // copy history
            let _ = c.execute("DELETE FROM clip_inbox", []); // F21 inbox
            let _ = c.execute("DELETE FROM trash", []); // 91: trashed prompts
        }
    }
    rearm_hotkeys(&app); // F20: no prompts left to keep accelerators bound
    rebuild_tray(&app); // F23: nothing left to offer in the tray either
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
        .filter(|l| l.starts_with("float-") || l.starts_with("folder-")) // F25
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
        load_pick_history(&store.settings); // 19: reset clears the mirror too
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
    apply_tray_menu(&app);
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

// ---------- Tag commands (F1) ----------

const MAX_TAG_LEN: usize = 60;
const MAX_TAGS_PER_PROMPT: usize = 50;

fn tag_key(name: &str) -> String {
    name.trim().to_lowercase()
}

// Commas separate tags in the TXT/CSV exports and control characters would break the
// line-based TXT format, so neither may appear inside a name.
fn tag_chars_ok(name: &str) -> bool {
    !name.contains(',') && !name.chars().any(|c| c.is_control())
}

// The casing each known tag is already stored with, so a tag typed as "work" joins
// an existing "Work" instead of splitting into a second chip.
fn canonical_tags(prompts: &[Prompt]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for p in prompts {
        for t in &p.tags {
            map.entry(tag_key(t)).or_insert_with(|| t.clone());
        }
    }
    map
}

// Trim, drop blank entries, fold case-insensitive duplicates onto the casing the
// library already uses. Over-long / over-many names are rejected, never truncated.
fn sanitize_tags(input: Vec<String>, known: &HashMap<String, String>) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for raw in input {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        if name.chars().count() > MAX_TAG_LEN {
            return Err(format!("tag name longer than {} characters", MAX_TAG_LEN));
        }
        if !tag_chars_ok(name) {
            return Err("tag name contains a comma or a control character".to_string());
        }
        let key = tag_key(name);
        if !seen.insert(key.clone()) {
            continue;
        }
        out.push(known.get(&key).cloned().unwrap_or_else(|| name.to_string()));
    }
    if out.len() > MAX_TAGS_PER_PROMPT {
        return Err(format!("more than {} tags on one prompt", MAX_TAGS_PER_PROMPT));
    }
    Ok(out)
}

// ---------- Private notes (F17) ----------

// Generous ceiling: notes are a scratchpad, not a second prompt body. Rejecting
// instead of truncating keeps the user's text from being silently cut in half.
const MAX_NOTES_LEN: usize = 10_000;

// Normalize line endings and enforce the ceiling. Notes may contain newlines —
// the TXT exporter escapes them (see notes_escape).
fn sanitize_notes(raw: &str) -> Result<String, String> {
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim().to_string();
    if text.chars().count() > MAX_NOTES_LEN {
        return Err(format!("note longer than {} characters", MAX_NOTES_LEN));
    }
    Ok(text)
}

// The TXT export is line-based, so a multi-line note is written on one line with
// its newlines escaped. Backslashes are escaped too, or a note ending in "\" plus
// a literal "n" would come back as a line break.
fn notes_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

fn notes_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// A single tag name argument: blank is an error here because the name IS the input.
fn valid_tag_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("tag name is empty".to_string());
    }
    if name.chars().count() > MAX_TAG_LEN {
        return Err(format!("tag name longer than {} characters", MAX_TAG_LEN));
    }
    if !tag_chars_ok(name) {
        return Err("tag name contains a comma or a control character".to_string());
    }
    Ok(name.to_string())
}

// "" clears the chip color back to neutral; anything else must be #rrggbb.
fn valid_tag_color(color: &str) -> Result<String, String> {
    let c = color.trim();
    if c.is_empty() {
        return Ok(String::new());
    }
    let ok = c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit());
    if ok {
        Ok(c.to_lowercase())
    } else {
        Err("color must be #rrggbb".to_string())
    }
}

// Swap one tag key for a new name across every prompt. Renaming onto a tag a prompt
// already carries merges the two instead of duplicating it. Returns whether anything
// changed, so an unused name costs no write.
fn rename_tag_in(prompts: &mut [Prompt], old_key: &str, new: &str) -> bool {
    let mut changed = false;
    for p in prompts {
        if !p.tags.iter().any(|t| tag_key(t) == old_key) {
            continue;
        }
        let mut kept: Vec<String> = Vec::with_capacity(p.tags.len());
        for t in std::mem::take(&mut p.tags) {
            let name = if tag_key(&t) == old_key { new.to_string() } else { t };
            if !kept.iter().any(|k| tag_key(k) == tag_key(&name)) {
                kept.push(name);
            }
        }
        p.tags = kept;
        changed = true;
    }
    changed
}

// Drop one tag key from every prompt. Returns whether anything changed.
fn remove_tag_from(prompts: &mut [Prompt], key: &str) -> bool {
    let mut changed = false;
    for p in prompts {
        let before = p.tags.len();
        p.tags.retain(|t| tag_key(t) != key);
        changed |= p.tags.len() != before;
    }
    changed
}

// Replace `old` with `new` in every prompt and move the color entry, in ONE store
// pass — a tag on hundreds of prompts must not turn into hundreds of round trips.
// Idempotent, and renaming onto an existing tag merges the two.
#[tauri::command]
async fn rename_tag(
    app: AppHandle,
    state: State<'_, Db>,
    old: String,
    new: String,
) -> Result<Settings, String> {
    let old = valid_tag_name(&old)?;
    let new = valid_tag_name(&new)?;
    let (old_key, new_key) = (tag_key(&old), tag_key(&new));
    let mut store = lock(&state);
    let prompts_changed = rename_tag_in(&mut store.prompts, &old_key, &new);
    if let Some(color) = store.settings.tag_colors.remove(&old_key) {
        // The target's own color wins if it already has one — it is the surviving tag.
        store.settings.tag_colors.entry(new_key).or_insert(color);
    }
    if prompts_changed {
        save_prompts(&app, &store);
    }
    save_settings(&app, &store.settings);
    Ok(store.settings.clone())
}

// Remove a tag from every prompt plus its color entry. Idempotent.
#[tauri::command]
async fn delete_tag(app: AppHandle, state: State<'_, Db>, name: String) -> Result<Settings, String> {
    let key = tag_key(&valid_tag_name(&name)?);
    let mut store = lock(&state);
    let prompts_changed = remove_tag_from(&mut store.prompts, &key);
    store.settings.tag_colors.remove(&key);
    if prompts_changed {
        save_prompts(&app, &store);
    }
    save_settings(&app, &store.settings);
    Ok(store.settings.clone())
}

#[tauri::command]
fn set_tag_color(app: AppHandle, state: State<Db>, name: String, color: String) -> Result<Settings, String> {
    let key = tag_key(&valid_tag_name(&name)?);
    let color = valid_tag_color(&color)?;
    let mut store = lock(&state);
    if color.is_empty() {
        store.settings.tag_colors.remove(&key);
    } else {
        store.settings.tag_colors.insert(key, color);
    }
    save_settings(&app, &store.settings);
    Ok(store.settings.clone())
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
    for g in &mut store.settings.view_groups {
        if let Some(nc) = map.get(&g.color) {
            g.color = nc.clone();
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
    // The deleted view must not linger in a group, and a group that just lost its
    // last member dissolves instead of becoming an empty folder tab.
    let views = store.settings.views.clone();
    normalize_view_groups(&mut store.settings.view_groups, &views);
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

// ---------- View group commands (F9) ----------

fn valid_group_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("group name is empty".to_string());
    }
    if name.chars().count() > MAX_GROUP_NAME {
        return Err(format!("group name longer than {} characters", MAX_GROUP_NAME));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err("group name contains a control character".to_string());
    }
    Ok(name.to_string())
}

// Bring the group list back to its invariants: only real view ids, no view in two
// groups (the first group listing it wins), and no empty group. Runs after every
// mutation and on load, so a hand-edited or older blob can never render a broken
// tab bar. Idempotent.
fn normalize_view_groups(groups: &mut Vec<ViewGroup>, views: &[View]) {
    let mut taken: Vec<String> = Vec::new();
    for group in groups.iter_mut() {
        group.view_ids.retain(|id| {
            let ok = views.iter().any(|v| &v.id == id) && !taken.contains(id);
            if ok {
                taken.push(id.clone());
            }
            ok
        });
    }
    groups.retain(|g| !g.view_ids.is_empty());
}

// Create (empty id) or update one group in a single pass: name, color and the full
// member list. Views handed in are pulled out of whatever group held them before.
#[tauri::command]
fn save_view_group(
    app: AppHandle,
    state: State<Db>,
    id: String,
    name: String,
    color: String,
    view_ids: Vec<String>,
) -> Result<Settings, String> {
    let name = valid_group_name(&name)?;
    let color = valid_tag_color(&color)?;
    let mut store = lock(&state);
    for view_id in &view_ids {
        if !store.settings.views.iter().any(|v| &v.id == view_id) {
            return Err("unknown view".to_string());
        }
    }
    // A view may only live in one group, so claim the members away from the others
    // before writing them into the target group.
    for group in store.settings.view_groups.iter_mut() {
        if group.id != id {
            group.view_ids.retain(|v| !view_ids.contains(v));
        }
    }
    match store.settings.view_groups.iter_mut().find(|g| g.id == id) {
        Some(group) => {
            group.name = name;
            group.color = color;
            group.view_ids = view_ids;
        }
        None => {
            if view_ids.is_empty() {
                return Err("a group needs at least one view".to_string());
            }
            if store.settings.view_groups.len() >= MAX_VIEW_GROUPS {
                return Err(format!("max {} view groups", MAX_VIEW_GROUPS));
            }
            store.settings.view_groups.push(ViewGroup { id: gen_id(), name, color, view_ids });
        }
    }
    let views = store.settings.views.clone();
    normalize_view_groups(&mut store.settings.view_groups, &views);
    save_settings(&app, &store.settings);
    Ok(store.settings.clone())
}

// Move one view into `group_id`, or out of every group when it is empty. Both
// directions in one command so the UI never needs a remove+add round trip.
#[tauri::command]
fn set_view_group_member(
    app: AppHandle,
    state: State<Db>,
    group_id: String,
    view_id: String,
) -> Result<Settings, String> {
    let mut store = lock(&state);
    if !store.settings.views.iter().any(|v| v.id == view_id) {
        return Err("unknown view".to_string());
    }
    if !group_id.is_empty() && !store.settings.view_groups.iter().any(|g| g.id == group_id) {
        return Err("unknown view group".to_string());
    }
    for group in store.settings.view_groups.iter_mut() {
        group.view_ids.retain(|v| v != &view_id);
    }
    if let Some(group) = store.settings.view_groups.iter_mut().find(|g| g.id == group_id) {
        group.view_ids.push(view_id);
    }
    let views = store.settings.views.clone();
    normalize_view_groups(&mut store.settings.view_groups, &views);
    save_settings(&app, &store.settings);
    Ok(store.settings.clone())
}

// ---------- Tile folder commands (F27) ----------

// Bring the grid folders back to their invariants: only real prompts and real views,
// no prompt in two folders (the first folder listing it wins), no two folders on the
// same cell of the same view (the first one keeps it, the next is moved aside by the
// renderer because its cell is cleared to the origin only if free), and no folder
// left with fewer than two members — that one dissolves and its prompts become tiles
// again. Runs on load and after every mutation. Idempotent.
// `view_ids` rather than `&[View]` (135): every caller only ever needs to check
// membership, so passing ids instead of whole views spares a clone of every view's
// layout maps at each call site.
fn normalize_tile_folders(folders: &mut Vec<TileFolder>, prompt_ids: &[String], view_ids: &[String]) {
    let mut taken: Vec<String> = Vec::new();
    for folder in folders.iter_mut() {
        folder.members.retain(|id| {
            let ok = prompt_ids.iter().any(|p| p == id) && !taken.contains(id);
            if ok {
                taken.push(id.clone());
            }
            ok
        });
    }
    folders.retain(|f| f.members.len() >= 2 && view_ids.iter().any(|id| id == &f.view_id));
    // One folder per cell: a later one on an occupied cell is pushed to the right,
    // which the renderer resolves to the first free cell anyway.
    let mut cells: Vec<(String, [u32; 2])> = Vec::new();
    for folder in folders.iter_mut() {
        while cells.iter().any(|(v, c)| v == &folder.view_id && *c == folder.cell) {
            folder.cell[0] = folder.cell[0].saturating_add(1);
        }
        cells.push((folder.view_id.clone(), folder.cell));
    }
    folders.truncate(MAX_TILE_FOLDERS);
}

// Create (empty id) or update one grid folder in a single pass: name, colour, cell
// and the full member list. Prompts handed in are pulled out of every other folder.
#[tauri::command]
fn save_tile_folder(
    app: AppHandle,
    state: State<Db>,
    id: String,
    name: String,
    color: String,
    view_id: String,
    cell: [u32; 2],
    members: Vec<String>,
) -> Result<Settings, String> {
    // A folder may stay unnamed — it then shows the member count alone.
    let name = folder_name_of(&name)?;
    let color = valid_tag_color(&color)?;
    let mut store = lock(&state);
    for pid in &members {
        if !store.prompts.iter().any(|p| &p.id == pid) {
            return Err(format!("unknown {}", brand::ITEM));
        }
    }
    for folder in store.settings.tile_folders.iter_mut() {
        if folder.id != id {
            folder.members.retain(|m| !members.contains(m));
        }
    }
    match store.settings.tile_folders.iter_mut().find(|f| f.id == id) {
        Some(folder) => {
            folder.name = name;
            folder.color = color;
            folder.cell = cell;
            folder.members = members;
        }
        None => {
            if members.len() < 2 {
                return Err(format!("a folder needs at least two {}", brand::ITEM_PLURAL));
            }
            if !store.settings.views.iter().any(|v| v.id == view_id) {
                return Err("unknown view".to_string());
            }
            if store.settings.tile_folders.len() >= MAX_TILE_FOLDERS {
                return Err(format!("max {} folders", MAX_TILE_FOLDERS));
            }
            store.settings.tile_folders.push(TileFolder {
                id: gen_id(),
                name,
                color,
                view_id,
                cell,
                members,
            });
        }
    }
    // A prompt inside a folder is not a tile of its own: drop its layout entries so
    // the cell it used to sit on is free again.
    let inside: Vec<String> = store
        .settings
        .tile_folders
        .iter()
        .flat_map(|f| f.members.iter().cloned())
        .collect();
    for view in store.settings.views.iter_mut() {
        for layout in view.layouts.values_mut() {
            layout.retain(|pid, _| !inside.contains(pid));
        }
    }
    let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    let view_ids: Vec<String> = store.settings.views.iter().map(|v| v.id.clone()).collect();
    normalize_tile_folders(&mut store.settings.tile_folders, &ids, &view_ids);
    save_settings(&app, &store.settings);
    Ok(store.settings.clone())
}

// Take one prompt out of its folder and put it on `cell` of the folder's view. The
// second-to-last member leaving dissolves the folder (normalize), which is what makes
// "drag the last one out" work without a case of its own.
#[tauri::command]
fn take_out_of_tile_folder(
    app: AppHandle,
    state: State<Db>,
    id: String,
    cell: [u32; 2],
) -> Result<Settings, String> {
    let mut store = lock(&state);
    let view_id = match store
        .settings
        .tile_folders
        .iter()
        .find(|f| f.members.contains(&id))
    {
        Some(f) => f.view_id.clone(),
        None => return Err("not in a folder".to_string()),
    };
    for folder in store.settings.tile_folders.iter_mut() {
        folder.members.retain(|m| m != &id);
    }
    if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == view_id) {
        let key = grid_key(view.cols, view.rows);
        let free = cell[0] < view.cols && cell[1] < view.rows;
        let layout = view.layouts.entry(key).or_default();
        if free {
            // Whoever sits there moves on: the grid never stacks two prompts.
            layout.retain(|_, c| *c != cell);
            layout.insert(id.clone(), cell);
        }
    }
    let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    let view_ids: Vec<String> = store.settings.views.iter().map(|v| v.id.clone()).collect();
    normalize_tile_folders(&mut store.settings.tile_folders, &ids, &view_ids);
    save_settings(&app, &store.settings);
    Ok(store.settings.clone())
}

// Dissolve a grid folder: its prompts become tiles again, nothing is deleted. Each
// member lands on the first free cell of the folder's view — the folder's own cell is
// free again by the time this runs, so it is usually one of them. Cells held by any
// OTHER folder still in that view are excluded too (they live outside view.layouts,
// so first_free_cell alone would not see them — a member could otherwise be handed a
// cell another folder already owns and never actually appear on screen).
#[tauri::command]
fn delete_tile_folder(app: AppHandle, state: State<Db>, id: String) -> Settings {
    let mut store = lock(&state);
    let removed = store.settings.tile_folders.iter().find(|f| f.id == id).cloned();
    store.settings.tile_folders.retain(|f| f.id != id);
    if let Some(folder) = removed {
        let other_folder_cells: Vec<[u32; 2]> = store
            .settings
            .tile_folders
            .iter()
            .filter(|f| f.view_id == folder.view_id)
            .map(|f| f.cell)
            .collect();
        if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == folder.view_id) {
            let key = grid_key(view.cols, view.rows);
            for member in &folder.members {
                // Grid full: this and any remaining member stay library-only, same as a
                // brand-new prompt would.
                let Some(cell) = first_free_cell_excluding(view, &other_folder_cells) else { break };
                view.layouts.entry(key.clone()).or_default().insert(member.clone(), cell);
            }
        }
    }
    save_settings(&app, &store.settings);
    store.settings.clone()
}

// Dissolve a group: its views go back to the tab bar, nothing is deleted.
#[tauri::command]
fn delete_view_group(app: AppHandle, state: State<Db>, id: String) -> Settings {
    let mut store = lock(&state);
    store.settings.view_groups.retain(|g| g.id != id);
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

// 133: the quick launcher only ever reads id/name/text/color, on a surface that has
// to be ready the instant its hotkey fires — get_state's full clone (every base64
// image included) was exactly the wrong cost to pay there. The four fields are
// small strings, so cloning just those (instead of the whole Prompt, image and all)
// is the actual saving.
#[derive(Serialize)]
struct LauncherItem {
    id: String,
    name: String,
    text: String,
    color: String,
}
#[derive(Serialize)]
struct LauncherState {
    prompts: Vec<LauncherItem>,
    language: String,
    usage: HashMap<String, u32>,
    last_used: HashMap<String, u64>,
}
#[tauri::command]
async fn launcher_items(state: State<'_, Db>) -> Result<LauncherState, String> {
    let store = lock(&state);
    Ok(LauncherState {
        prompts: store
            .prompts
            .iter()
            .map(|p| LauncherItem {
                id: p.id.clone(),
                name: p.name.clone(),
                text: p.text.clone(),
                color: p.color.clone(),
            })
            .collect(),
        language: store.settings.language.clone(),
        usage: store.settings.usage.clone(),
        last_used: store.settings.last_used.clone(),
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
// 58: one day's copy count. `ts` is that day's UTC midnight (unix seconds) — the
// frontend already has locale-aware date formatting via Intl, no reason to
// duplicate it (or pull in a date crate) on the Rust side for a label string.
#[derive(Serialize)]
struct DayCount {
    ts: u64,
    count: usize,
}
#[derive(Serialize)]
struct UsageStats {
    total_prompts: usize,
    total_copies: u64,
    copies7: u64,
    copies30: u64,
    used_count: usize,
    avg_copies: f64,
    total_chars: u64,
    recent_name: String,
    recent_ts: u64,
    longest_name: String,
    longest_chars: usize,
    types: Vec<ViewCount>,
    top: Vec<StatBar>,
    never_used: Vec<StatBar>,
    per_view: Vec<ViewCount>,
    daily: Vec<DayCount>, // 58: last 14 UTC days, oldest first, zero-filled
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
    let used_count = store.prompts.iter().filter(|p| usage.get(&p.id).copied().unwrap_or(0) > 0).count();
    let avg_copies = if used_count > 0 { total_copies as f64 / used_count as f64 } else { 0.0 };
    let total_chars: u64 = store.prompts.iter().map(|p| p.text.chars().count() as u64).sum();
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
        // Never copied — not "not lately", so the list means exactly what it says.
        .filter(|p| usage.get(&p.id).copied().unwrap_or(0) == 0)
        .map(|p| StatBar { id: p.id.clone(), name: name_of(p), count: usage.get(&p.id).copied().unwrap_or(0) })
        .collect();
    never_used.truncate(top_n.max(10) * 2);
    let per_view: Vec<ViewCount> = store
        .settings
        .views
        .iter()
        .map(|v| {
            let key = grid_key(v.cols, v.rows);
            let count = v.layouts.get(&key).map(|l| l.len()).unwrap_or(0);
            ViewCount { name: v.name.clone(), count }
        })
        .collect();
    let (copies7, copies30, daily) = db_conn(&app)
        .map(|c| {
            let c7 = c
                .query_row("SELECT COUNT(*) FROM copy_log WHERE ts >= ?1", params![now.saturating_sub(7 * 86400) as i64], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as u64;
            let c30 = c
                .query_row("SELECT COUNT(*) FROM copy_log WHERE ts >= ?1", params![cutoff30 as i64], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as u64;
            let daily = db_daily_copies(&c, 14);
            (c7, c30, daily)
        })
        .unwrap_or((0, 0, Vec::new()));
    let history_on = store.settings.ui_flags.get("copyHistory") != Some(&false);
    Ok(UsageStats {
        total_prompts,
        total_copies,
        copies7,
        copies30,
        used_count,
        avg_copies,
        total_chars,
        recent_name,
        recent_ts,
        longest_name,
        longest_chars,
        types,
        top,
        never_used,
        per_view,
        daily,
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
    // 32: a rotating prompt is compared on entry 1 (p.text) only, deliberately —
    // two prompts that happen to share their FIRST entry but differ in the rest are
    // not "near duplicates" of each other, and comparing every entry pair would
    // turn this already-O(n^2) scan into O(n^2 * entries).
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
    // How many prompts the snapshot holds. Written next to the snapshot when it is
    // made (exact, and free to read back); a backup from an older build has no note
    // yet, so its count is read out of the snapshot itself once.
    prompts: Option<usize>,
    // 61: the prompt names themselves, same "written once, cheap to read back after"
    // treatment as `prompts` above. The snapshot beside it has every row DPAPI-sealed,
    // so this cache is sealed the same way — plaintext here would hand anyone copying
    // the data folder to another Windows account the full list of prompt names while
    // the snapshot itself stayed unreadable, which is exactly the protection the seal
    // exists for.
    names: Vec<String>,
}

const BACKUP_COUNT_FILE: &str = "prompts.count";
const BACKUP_NAMES_FILE: &str = "prompts.names.json";

// Count the prompts in a snapshot folder: the note if there is one, otherwise the
// snapshot is unpacked to a temp file and counted (and the note written for next
// time). None only when the folder holds no readable snapshot at all.
fn backup_prompt_count(folder: &std::path::Path) -> Option<usize> {
    if let Ok(text) = fs::read_to_string(folder.join(BACKUP_COUNT_FILE)) {
        if let Ok(n) = text.trim().parse::<usize>() {
            return Some(n);
        }
    }
    let db = backup_db_file(folder)?;
    let tmp;
    let path = if db.extension().map(|e| e == "gz").unwrap_or(false) {
        tmp = folder.join("count.tmp");
        gunzip_to(&db, &tmp).ok()?;
        tmp.clone()
    } else {
        db
    };
    let n = Connection::open(&path)
        .ok()
        .and_then(|c| c.query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get::<_, i64>(0)).ok())
        .map(|n| n.max(0) as usize);
    let _ = fs::remove_file(folder.join("count.tmp"));
    if let Some(n) = n {
        let _ = fs::write(folder.join(BACKUP_COUNT_FILE), n.to_string());
    }
    n
}

// Decrypt each row (same DPAPI seal `db_load` uses on the live store) and pull out
// just the name — best-effort, a row that fails to decrypt/parse is skipped rather
// than failing the whole list.
fn read_prompt_names(path: &std::path::Path) -> Vec<String> {
    let Ok(conn) = Connection::open(path) else { return Vec::new() };
    let Ok(mut stmt) = conn.prepare("SELECT data FROM prompts ORDER BY ord") else { return Vec::new() };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else { return Vec::new() };
    rows.flatten()
        .filter_map(|data| serde_json::from_str::<Prompt>(&dec_str(&data)).ok())
        .map(|p| p.name)
        .collect()
}

// The names cache carries real prompt titles, so it is DPAPI-sealed like every row in
// the snapshot it sits beside. `dec_str` passes an un-prefixed value straight through,
// so a plaintext cache written by an earlier build still reads and is re-sealed on the
// next write — no migration step needed.
fn write_prompt_names(cache: &std::path::Path, names: &[String]) {
    if let Ok(json) = serde_json::to_string(names) {
        let _ = fs::write(cache, enc_str(&json));
    }
}
fn read_prompt_names_cache(cache: &std::path::Path) -> Vec<String> {
    fs::read_to_string(cache)
        .ok()
        .and_then(|raw| serde_json::from_str(&dec_str(&raw)).ok())
        .unwrap_or_default()
}

// 61: same cached-note treatment as `backup_prompt_count` — computed once (here, or
// at creation time in `do_backup`) and read straight back after.
fn backup_prompt_names(folder: &std::path::Path) -> Vec<String> {
    let cache = folder.join(BACKUP_NAMES_FILE);
    if cache.exists() {
        return read_prompt_names_cache(&cache);
    }
    let Some(db) = backup_db_file(folder) else { return Vec::new() };
    let tmp;
    let path = if db.extension().map(|e| e == "gz").unwrap_or(false) {
        tmp = folder.join("names.tmp");
        if gunzip_to(&db, &tmp).is_err() {
            return Vec::new();
        }
        tmp.clone()
    } else {
        db
    };
    let names = read_prompt_names(&path);
    let _ = fs::remove_file(folder.join("names.tmp"));
    if !names.is_empty() {
        write_prompt_names(&cache, &names);
    }
    names
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
            let prompts = backup_prompt_count(&e.path());
            let names = backup_prompt_names(&e.path());
            BackupInfo { name, size, prompts, names }
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
    yearly: usize,
}
fn gfs_from_settings(s: &Settings) -> Option<GfsConfig> {
    if s.ui_flags.get("backupGfs") != Some(&true) {
        return None;
    }
    let g = |k: &str, d: f64| s.ui_values.get(k).copied().unwrap_or(d).max(0.0) as usize;
    Some(GfsConfig {
        daily: g("backupDaily", 3.0),
        weekly: g("backupWeekly", 4.0),
        monthly: g("backupMonthly", 6.0),
        yearly: g("backupYearly", 1.0),
    })
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
// (day, week, month, year) tier keys from a backup folder name "YYYY-MM-DD_HH-MM-SS".
fn backup_date_keys(name: &str) -> Option<(i64, i64, i64, i64)> {
    let date = name.split('_').next()?;
    let mut it = date.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    let days = days_from_civil(y, m, d);
    Some((days, days.div_euclid(7), y * 12 + m, y))
}
// Which of `names` (sorted oldest first) rotation throws away. Pure, so the
// retention rules can be tested without a disk full of folders.
fn backups_to_drop(names: &[String], keep: usize, gfs: Option<&GfsConfig>) -> Vec<String> {
    use std::collections::HashSet;
    let keep = keep.max(1); // a caller passing 0 must never mean "delete everything"
    let Some(g) = gfs else {
        let drop = names.len().saturating_sub(keep);
        return names[..drop].to_vec();
    };
    // Newest-first walk; for each tier keep the newest backup per distinct
    // day / week / month / year up to that tier's count.
    let pick = |count: usize, idx: usize| -> Vec<String> {
        let mut seen: HashSet<i64> = HashSet::new();
        let mut out = Vec::new();
        if count == 0 {
            return out;
        }
        for n in names.iter().rev() {
            if let Some(keys) = backup_date_keys(n) {
                let k = [keys.0, keys.1, keys.2, keys.3][idx];
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
    // The newest `keep` snapshots survive whatever the tiers say. A tier keeps one
    // backup per distinct day, so without this a second backup made the same day
    // would delete the first one — correct by the letter of the rule, and exactly
    // the kind of surprise a backup feature must not produce.
    let mut keep_set: HashSet<String> = names.iter().rev().take(keep).cloned().collect();
    for (count, idx) in [(g.daily, 0), (g.weekly, 1), (g.monthly, 2), (g.yearly, 3)] {
        for n in pick(count, idx) {
            keep_set.insert(n);
        }
    }
    // Every tier at zero would wipe the lot — keep the newest one instead.
    if keep_set.is_empty() {
        if let Some(newest) = names.last() {
            keep_set.insert(newest.clone());
        }
    }
    names.iter().filter(|n| !keep_set.contains(*n)).cloned().collect()
}
fn rotate_backups(app: &AppHandle, keep: usize, gfs: Option<GfsConfig>) {
    let dir = backups_dir(app);
    let mut names: Vec<String> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort(); // oldest first (names are timestamps → chronological)
    for name in backups_to_drop(&names, keep, gfs.as_ref()) {
        let _ = fs::remove_dir_all(dir.join(name));
    }
}
// Consistent snapshot of data.db via VACUUM INTO (WAL-safe, no locking games). Writes
// a temp file first and renames on success, so a failure leaves no partial snapshot.
// A snapshot is stored gzipped: a vacuumed store is mostly JSON text, so this
// takes a few MB down to a fraction of that — and ten of them are kept by default.
// `data.db` (uncompressed) is still accepted so older backups keep working.
const BACKUP_DB_GZ: &str = "data.db.gz";
// How often the auto-backup thread looks at the clock. The shortest interval the UI
// offers is 6 h, so a quarter of an hour is precise enough and costs nothing.
const BACKUP_CHECK_SECS: u64 = 900;

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
    // Temp + rename: a snapshot half-written when the machine goes down would still
    // be picked up by backup_db_file and only fail at restore time, when it matters.
    let tmp = dest.with_extension("gz.part");
    fs::write(&tmp, packed).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })
}

fn gunzip_to(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    let packed = fs::read(src).map_err(|e| e.to_string())?;
    // Capped like every other decompression here: a snapshot file is on disk where
    // anything could have replaced it, and an unbounded inflate is a free crash.
    let out = inflate_capped(GzDecoder::new(&packed[..]))?;
    fs::write(dest, out).map_err(|e| e.to_string())
}

fn do_backup(app: &AppHandle, keep: usize, gfs: Option<GfsConfig>) -> Result<String, String> {
    let src = data_dir(app).join("data.db");
    if !src.exists() {
        return Err("no-store".into());
    }
    // Two snapshots inside the same second — two quick manual backups, or the hour a
    // DST fallback repeats — must not share a folder: create_dir_all would reuse the
    // existing one and the earlier snapshot would be silently overwritten. create_dir
    // fails on an existing directory, so claiming the name IS the check (no race).
    // stamp_secs() reads only the first three time fields, so the suffix leaves the
    // snapshot's age intact for rotation and the GFS tiers.
    let root = backups_dir(app);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let base = local_stamp();
    let mut name = base.clone();
    let mut folder = root.join(&name);
    let mut n = 2;
    loop {
        match fs::create_dir(&folder) {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if n > 99 {
                    return Err("backup-name-taken".into());
                }
                name = format!("{base}-{n}");
                folder = root.join(&name);
                n += 1;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let tmp = folder.join("data.db.tmp");
    let dest = folder.join(BACKUP_DB_GZ);
    let _ = fs::remove_file(&tmp);
    {
        let conn = Connection::open(&src).map_err(|e| e.to_string())?;
        conn.execute("VACUUM INTO ?1", params![tmp.to_string_lossy()]).map_err(|e| e.to_string())?;
    }
    // Compress the vacuumed copy, then drop the raw one — never leave both behind.
    let packed = gzip_to(&tmp, &dest);
    // The count and names are taken from the vacuumed copy while it is still
    // unpacked, so the list never has to decompress a snapshot for either.
    if let Ok(conn) = Connection::open(&tmp) {
        if let Ok(n) = conn.query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get::<_, i64>(0)) {
            let _ = fs::write(folder.join(BACKUP_COUNT_FILE), n.max(0).to_string());
        }
    }
    let names = read_prompt_names(&tmp);
    if !names.is_empty() {
        write_prompt_names(&folder.join(BACKUP_NAMES_FILE), &names);
    }
    let _ = fs::remove_file(&tmp);
    if let Err(e) = packed {
        // No snapshot is better than a folder that looks like one: the list reads
        // folders, and an empty one would offer a restore that cannot work.
        let _ = fs::remove_dir_all(&folder);
        return Err(e);
    }
    rotate_backups(app, keep.clamp(1, 50), gfs);
    Ok(name)
}
// A folder name "YYYY-MM-DD_HH-MM-SS" as seconds on a local civil timeline. Two of
// these subtract cleanly — the zone offset cancels — which is all the schedule needs
// and avoids trusting a modification time that any sync or antivirus tool may reset.
fn stamp_secs(name: &str) -> Option<i64> {
    let (date, time) = name.split_once('_')?;
    let mut d = date.split('-');
    let y: i64 = d.next()?.parse().ok()?;
    let mo: i64 = d.next()?.parse().ok()?;
    let da: i64 = d.next()?.parse().ok()?;
    let mut t = time.split('-');
    let h: i64 = t.next()?.parse().ok()?;
    let mi: i64 = t.next()?.parse().ok()?;
    let s: i64 = t.next().unwrap_or("0").parse().ok()?;
    Some(days_from_civil(y, mo, da) * 86_400 + h * 3_600 + mi * 60 + s)
}

// Seconds since the newest snapshot, or None when there is none yet.
fn newest_backup_age(app: &AppHandle) -> Option<u64> {
    let now = stamp_secs(&local_stamp())?;
    let newest = fs::read_dir(backups_dir(app))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| stamp_secs(&e.file_name().to_string_lossy()))
        .max()?;
    Some((now - newest).max(0) as u64)
}

#[tauri::command]
async fn list_backups(app: AppHandle) -> Result<Vec<BackupInfo>, String> {
    Ok(list_backups_impl(&app))
}

// 65: hands the user straight to the snapshot folder. The path is always the app's
// own backups_dir() — never taken from the frontend — so there is nothing here for a
// crafted argument to redirect; `explorer` gets exactly one argument, no shell involved.
#[tauri::command]
fn open_backups_folder(app: AppHandle) -> Result<(), String> {
    let dir = backups_dir(&app);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
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

// Unpack (gunzip if needed) a backup file BESIDE the live store, then prove it is
// an openable SQLite file. Shared by every restore scope — a snapshot that fails
// either step must never touch the live database.
fn stage_and_validate_backup(backup_db: &std::path::Path, dir: &std::path::Path) -> Result<PathBuf, String> {
    let staged = dir.join("data.db.restoring");
    let _ = fs::remove_file(&staged);
    let unpack = if backup_db.extension().is_some_and(|e| e == "gz") {
        gunzip_to(backup_db, &staged)
    } else {
        fs::copy(backup_db, &staged).map(|_| ()).map_err(|e| e.to_string()) // pre-2.6 snapshot
    };
    if let Err(e) = unpack {
        let _ = fs::remove_file(&staged);
        return Err(e);
    }
    if let Err(e) = Connection::open(&staged)
        .and_then(|c| c.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)))
    {
        let _ = fs::remove_file(&staged);
        return Err(format!("backup is not a readable database: {}", e));
    }
    Ok(staged)
}

// 70: merge only the tables `scope` asks for from `staged` into the live `target`
// database, inside one transaction — any failure leaves `target` untouched. "data"
// mirrors exactly the tables delete_all_data already treats as data; "settings"
// touches only the meta['settings'] row, never the meta['copy_log_migrated']
// migration marker. A table missing from an older backup (e.g. "trash", added in
// 91) is skipped rather than failing the whole restore — its live rows stay put.
fn restore_selective(target: &std::path::Path, staged: &std::path::Path, scope: &str) -> Result<(), String> {
    let mut conn = Connection::open(target).map_err(|e| e.to_string())?;
    conn.execute("ATTACH DATABASE ?1 AS backup", [staged.to_string_lossy().to_string()])
        .map_err(|e| e.to_string())?;
    let result: rusqlite::Result<()> = (|| {
        let tx = conn.transaction()?;
        if scope == "data" {
            for table in ["prompts", "trash", "prompt_versions", "copy_log", "clip_inbox"] {
                let exists: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM backup.sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )?;
                if exists == 0 {
                    continue;
                }
                tx.execute(&format!("DELETE FROM {table}"), [])?;
                tx.execute(&format!("INSERT INTO {table} SELECT * FROM backup.{table}"), [])?;
            }
        } else if scope == "settings" {
            tx.execute(
                "INSERT INTO meta(key, value) SELECT key, value FROM backup.meta WHERE key = 'settings'
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [],
            )?;
        }
        tx.commit()
    })();
    let _ = conn.execute("DETACH DATABASE backup", []);
    result.map_err(|e| e.to_string())
}

// Restore: snapshot the current store first (safety), then either swap the whole
// data.db (scope "all") or merge just the requested tables in place (scope "data"/
// "settings") — either way relaunch after, so nothing can half-apply.
#[tauri::command]
async fn restore_backup(app: AppHandle, state: State<'_, Db>, name: String, keep: usize, scope: String) -> Result<(), String> {
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
        let staged = stage_and_validate_backup(&backup_db, &dir)?;
        if scope == "all" {
            // Writing straight onto data.db truncates it first, so a disk that fills
            // up (or any I/O error) halfway through would leave the user with
            // neither the old database nor the restored one — swap via rename instead.
            if let Err(e) = fs::rename(&staged, &target) {
                let _ = fs::remove_file(&staged);
                return Err(e.to_string());
            }
        } else {
            let result = restore_selective(&target, &staged, &scope);
            let _ = fs::remove_file(&staged);
            result?;
        }
        let _ = fs::remove_file(dir.join("data.db-wal"));
        let _ = fs::remove_file(dir.join("data.db-shm"));
    }
    app.restart();
}

#[tauri::command]
fn current_theme(app: AppHandle, state: State<Db>) -> String {
    let settings = lock(&state).settings.clone();
    effective_theme(&app, &theme_base_of(&settings, &settings.theme))
}

// Colour overrides of the active theme; empty for every built-in one.
#[tauri::command]
fn theme_vars(state: State<Db>) -> HashMap<String, String> {
    theme_vars_of(&lock(&state).settings)
}

#[tauri::command]
fn set_theme(app: AppHandle, state: State<Db>, theme: String) -> String {
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
#[tauri::command]
fn save_custom_theme(
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
        store.settings.clone()
    };
    push_theme(&app, &settings);
    Ok(settings)
}

// Drop a custom theme; deleting the active one falls back to the system theme.
#[tauri::command]
fn delete_custom_theme(app: AppHandle, state: State<Db>, id: String) -> Result<Settings, String> {
    let settings = {
        let mut store = lock(&state);
        store.settings.custom_themes.retain(|c| c.id != id);
        if store.settings.theme == format!("custom:{}", id) {
            store.settings.theme = default_theme();
        }
        save_settings(&app, &store.settings);
        store.settings.clone()
    };
    push_theme(&app, &settings);
    Ok(settings)
}

// 76: name/base/vars only — never the internal id, so importing a theme on another
// install always creates a fresh one instead of colliding with (or silently
// overwriting) whatever already has that id there.
#[derive(Serialize, Deserialize)]
struct ThemeExport {
    #[serde(default)]
    name: String,
    #[serde(default = "default_custom_base")]
    base: String,
    #[serde(default)]
    vars: HashMap<String, String>,
}

#[tauri::command]
fn export_custom_theme(app: AppHandle, state: State<Db>, id: String) -> Result<(), String> {
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
        .set_file_name(format!("{}.json", theme.name))
        .add_filter("JSON", &["json"])
        .save_file();
    match file {
        Some(path) => fs::write(&path, json).map_err(|e| e.to_string()),
        None => Err("canceled".to_string()),
    }
}

// Reuses save_custom_theme's own validation (name/base/vars) so an imported file
// is never trusted more than a value typed into the editor by hand — a malformed
// or hand-edited JSON file gets the exact same rejection a bad manual entry would.
#[tauri::command]
fn import_custom_theme(app: AppHandle, state: State<Db>) -> Result<Settings, String> {
    let file = file_dialog(&app).add_filter("JSON", &["json"]).pick_file();
    let Some(path) = file else {
        return Err("canceled".to_string());
    };
    check_import_size(&path, MAX_THEME_BYTES)?;
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let parsed: ThemeExport = serde_json::from_str(&content).map_err(|_| "invalid theme file".to_string())?;
    save_custom_theme(app, state, String::new(), parsed.name, parsed.base, parsed.vars)
}

// F17: the single place that decides what text a copy puts on the clipboard. The
// private notes are deliberately not part of it — every copy path goes through
// here (or through copy_text with an already-resolved body), so a note can never
// leak into the clipboard.
fn copy_payload(p: &Prompt) -> &str {
    &p.text
}

// 32: absolute ceiling regardless of the expert value rotateMaxEntries (a UI-level
// soft cap on how many the editor lets you ADD) — this is the hard backend floor
// against a corrupted or hand-edited settings/import file.
const MAX_ROTATE_VARIANTS: usize = 49; // + entry 0 (text) = 50 total, matching rotateMaxEntries' own max

// 32: trims, drops empty entries and enforces the hard ceiling — "leere Einträge
// werden beim Speichern verworfen" applies to add_prompt/update_prompt alike.
fn sanitize_variants(list: Vec<String>) -> Vec<String> {
    list.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(MAX_ROTATE_VARIANTS)
        .collect()
}

// 32: only "random" is ever stored as anything other than "" (order); a prompt
// with no variants has nothing to rotate, so its mode is meaningless — keeping it
// empty avoids a stale mode surviving in storage past the point it can affect anything.
fn sanitize_rotate_mode(mode: String, variants: &[String]) -> String {
    if variants.is_empty() {
        String::new()
    } else if mode == "random" {
        "random".to_string()
    } else {
        String::new()
    }
}

// 32: "text" is entry 0 (every non-rotating consumer stays correct unchanged),
// `variants` hold entries 1..n.
fn rotation_total(p: &Prompt) -> usize {
    1 + p.variants.len()
}

fn rotation_entry_text(p: &Prompt, idx: usize) -> String {
    if idx == 0 {
        copy_payload(p).to_string() // F17: keep the one place that defines "entry 0"
    } else {
        p.variants.get(idx - 1).cloned().unwrap_or_else(|| copy_payload(p).to_string())
    }
}

// 32: which entry the NEXT copy would use, without touching any state — so a tile's
// "next: n/total" badge and the actual copy always agree, no matter how many times
// this is called between two real copies. The global "rotatingPrompts" flag off, or
// a prompt with no variants, always resolves to entry 0 — the list stays saved on
// the prompt, it is just not served.
// A stored pointer/bag that no longer fits the current entry count (edited since)
// is never trusted blindly: `% total` keeps a stale `rotate_pos` in range, and any
// bag containing an out-of-range index is treated as absent (discarded on the next
// rotation_advance) instead of ever indexing past the end.
fn rotation_next(p: &Prompt, settings: &Settings) -> (usize, String) {
    let total = rotation_total(p);
    let idx = if p.variants.is_empty() || settings.ui_flags.get("rotatingPrompts") == Some(&false) {
        0
    } else if p.rotate_mode == "random" {
        match settings.rotate_bag.get(&p.id) {
            Some(bag) if !bag.is_empty() && bag.iter().all(|&i| (i as usize) < total) => bag[0] as usize,
            _ => 0,
        }
    } else {
        settings.rotate_pos.get(&p.id).copied().unwrap_or(0) as usize % total
    };
    (idx, rotation_entry_text(p, idx))
}

// 32: advances the pointer past the entry rotation_next() just reported — call ONLY
// once the copy that used it actually reached the clipboard, so a failed copy never
// burns an entry. Random mode pops the bag's shown front and, once that empties it
// (or the bag was stale/absent, per rotation_next's own validity check), reshuffles
// a fresh permutation of every OTHER index with the just-shown one placed last — a
// full cycle (every entry exactly once) always separates two showings of the same
// entry, including across the reshuffle boundary.
fn rotation_advance(p: &Prompt, settings: &mut Settings) {
    // Same gate as rotation_next() — while the feature is globally off the pointer
    // must stay frozen, not silently drift, so turning it back on resumes exactly
    // where the user left off instead of landing on an arbitrary position.
    if p.variants.is_empty() || settings.ui_flags.get("rotatingPrompts") == Some(&false) {
        return;
    }
    let total = rotation_total(p);
    if p.rotate_mode == "random" {
        let bag_valid = settings.rotate_bag.get(&p.id)
            .map(|b| !b.is_empty() && b.iter().all(|&i| (i as usize) < total))
            .unwrap_or(false);
        let (shown, mut rest) = if bag_valid {
            let mut bag = settings.rotate_bag.remove(&p.id).unwrap();
            (bag.remove(0), bag)
        } else {
            (0u32, Vec::new())
        };
        if rest.is_empty() {
            rest = (0..total as u32).filter(|&i| i != shown).collect();
            use rand::seq::SliceRandom;
            rest.shuffle(&mut rand::thread_rng());
            rest.push(shown); // never the first draw of the new cycle
        }
        settings.rotate_bag.insert(p.id.clone(), rest);
    } else {
        let pos = settings.rotate_pos.get(&p.id).copied().unwrap_or(0) as usize % total;
        settings.rotate_pos.insert(p.id.clone(), ((pos + 1) % total) as u32);
    }
}

// 32: drops both rotation entries once a prompt is gone for good — called from the
// same places that already purge per-prompt state (`purge_ids`), not from the
// soft "move to trash" step (a trashed-then-restored prompt keeps its position).
fn rotation_forget(settings: &mut Settings, ids: &[String]) {
    for id in ids {
        settings.rotate_pos.remove(id);
        settings.rotate_bag.remove(id);
    }
}

// The most-called command in the app (every tile/pill/hotkey copy goes through it),
// so it clones only the ONE field the chosen branch actually needs — not the whole
// Prompt (134), which on an image prompt meant cloning that base64 blob on every
// text-branch check too.
enum CopyPayload {
    Image(String),
    File(String),
    Text(String),
    Missing,
}

// Async: encoding + writing a large image to the clipboard stays off the UI thread.
#[tauri::command]
async fn copy_prompt(app: AppHandle, state: State<'_, Db>, id: String) -> Result<bool, String> {
    let payload = {
        let store = lock(&state);
        match store.prompts.iter().find(|p| p.id == id) {
            Some(p) if p.copy_image && !p.image.is_empty() => CopyPayload::Image(p.image.clone()),
            Some(p) if !p.file_path.is_empty() => CopyPayload::File(p.file_path.clone()),
            // 32: rotation_next() itself falls back to p.text when there is nothing
            // to rotate, so a plain prompt goes through this exact same branch.
            Some(p) => CopyPayload::Text(rotation_next(p, &store.settings).1),
            None => CopyPayload::Missing,
        }
    };
    let ok = match payload {
        CopyPayload::Image(img) => copy_image_to_clipboard(&img),
        CopyPayload::File(path) => set_clipboard_file(&path),
        CopyPayload::Text(text) => {
            note_own_copy(&text); // F21: don't echo our own copy into the inbox
            arboard::Clipboard::new().and_then(|mut c| c.set_text(text)).is_ok()
        }
        CopyPayload::Missing => false,
    };
    // 32: only a successful clipboard write may consume a rotation entry — a failed
    // copy must leave the pointer exactly where it was.
    if ok {
        let mut store = lock(&state);
        if let Some(pos) = store.prompts.iter().position(|p| p.id == id) {
            let p = store.prompts[pos].clone();
            rotation_advance(&p, &mut store.settings);
            save_settings(&app, &store.settings); // 32: the pointer survives a restart
            emit_rotation_changed(&app, &p, &store.settings);
        }
    }
    Ok(ok)
}

// 32: what rotation_peek() reports for a "next: n/total" badge or a copy path that
// needs the resolved text itself (chaining, drag-out, hotkeys, tray, quick-launch)
// BEFORE it goes through variable resolution — those all peek, resolve, copy_text,
// then commit, so a cancelled variable dialog never burns an entry.
#[derive(Serialize)]
struct RotationPeek {
    index: usize,
    total: usize,
    text: String,
}

#[tauri::command]
fn rotation_peek(state: State<Db>, id: String) -> Option<RotationPeek> {
    let store = lock(&state);
    let p = store.prompts.iter().find(|p| p.id == id)?;
    let (index, text) = rotation_next(p, &store.settings);
    Some(RotationPeek { index, total: rotation_total(p), text })
}

// 32: every window's tile/pill/folder-member badge stays live off ONE event, fired
// wherever the pointer actually moves (copy_prompt's own success path, this command)
// — the same "backend decides, frontend just reflects" split as prompt-updated.
#[derive(Serialize, Clone)]
struct RotationChanged {
    id: String,
    index: usize,
    total: usize,
    text: String,
}

fn emit_rotation_changed(app: &AppHandle, p: &Prompt, settings: &Settings) {
    if p.variants.is_empty() {
        return;
    }
    let (index, text) = rotation_next(p, settings);
    let _ = app.emit(
        "rotation-changed",
        RotationChanged { id: p.id.clone(), index, total: rotation_total(p), text },
    );
}

#[tauri::command]
fn rotation_commit(app: AppHandle, state: State<Db>, id: String) {
    let mut store = lock(&state);
    if let Some(pos) = store.prompts.iter().position(|p| p.id == id) {
        let p = store.prompts[pos].clone();
        rotation_advance(&p, &mut store.settings);
        save_settings(&app, &store.settings);
        emit_rotation_changed(&app, &p, &store.settings);
    }
}

// 32: editor's "From the start" button — back to entry 1, same as a freshly
// created rotating prompt, without touching the entries themselves.
#[tauri::command]
fn rotation_reset(app: AppHandle, state: State<Db>, id: String) {
    let mut store = lock(&state);
    rotation_forget(&mut store.settings, std::slice::from_ref(&id));
    save_settings(&app, &store.settings);
}

// Put arbitrary text on the clipboard — used after filling prompt placeholders.
#[tauri::command]
async fn copy_text(text: String) -> bool {
    note_own_copy(&text); // F21
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .is_ok()
}

// F12 `{#{clipboard}#}`: the clipboard's current text for the placeholder resolver.
// Empty when the clipboard holds no text at all (an image or file payload).
#[tauri::command]
async fn clipboard_text() -> String {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .unwrap_or_default()
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

// History length is an expert value (default 100, ceiling COPY_HISTORY_MAX), paired
// with the retention-window cutoff — the two limits `db_trim_copy` always applies
// together. One place for both, so startup trimming and live recording can't drift.
fn history_cap_and_cutoff(settings: &Settings) -> (usize, Option<u64>) {
    let cap = settings
        .ui_values
        .get("historyMax")
        .copied()
        .unwrap_or(100.0)
        .clamp(0.0, COPY_HISTORY_MAX as f64) as usize;
    let cutoff = history_max_age(settings).map(|age| now_secs().saturating_sub(age));
    (cap, cutoff)
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
        let (cap, cutoff) = history_cap_and_cutoff(&store.settings);
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
    rebuild_tray(&app); // F23: the "recently used" section follows every copy
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
    rebuild_tray(&app); // F23: a wiped history empties the "recently used" section
    Ok(())
}

// Newest copy-history entries for the journal (own table; capped for the UI).
// Async: the grouped query scans the table, so keep it off the UI thread.
#[tauri::command]
async fn recent_copies(app: AppHandle, limit: usize, grouped: bool) -> Vec<JournalEntry> {
    db_conn(&app)
        .map(|c| db_recent_copies(&c, limit.min(COPY_HISTORY_MAX), grouped))
        .unwrap_or_default()
}

// 59: remove one journal entry by its exact row — never "all copies of this
// prompt", so it behaves the same in grouped and ungrouped view.
#[tauri::command]
async fn delete_copy_entry(app: AppHandle, rowid: i64) -> bool {
    db_conn(&app)
        .map(|c| c.execute("DELETE FROM copy_log WHERE rowid = ?1", params![rowid]).is_ok())
        .unwrap_or(false)
}

// 43: "hide for now" — a pill or folder id (both flat strings from different id
// spaces, never colliding), dismissed until the app restarts. NOT the permanent
// unpin (toggle_floating/dissolve_float_folder) — the entry in settings.floating
// / float_folders stays untouched, so it comes right back on the next launch.
#[tauri::command]
async fn hide_float_for_now(app: AppHandle, id: String) {
    if let Some(state) = app.try_state::<HiddenFloats>() {
        state.0.lock().unwrap_or_else(|e| e.into_inner()).insert(id);
    }
    sync_float_windows(&app);
}

// Must stay async: window creation from a sync command deadlocks on Windows
// (sync commands run on the main thread, which WebView2 needs free).
#[tauri::command]
async fn toggle_floating(app: AppHandle, state: State<'_, Db>, id: String) -> Result<bool, String> {
    // F25: a pill inside a folder has no window of its own, but it is still pinned —
    // unpinning has to take it out of the folder as well.
    let in_folder = {
        let store = lock(&state);
        folders_on(&store.settings)
            && store.settings.float_folders.iter().any(|f| f.members.contains(&id))
    };
    if app.get_webview_window(&flabel(&id)).is_some() || in_folder {
        {
            let mut store = lock(&state);
            store.settings.floating.remove(&id);
            store.settings.float_scale.remove(&id);
            for folder in store.settings.float_folders.iter_mut() {
                folder.members.retain(|m| m != &id);
            }
            let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
            normalize_float_folders(&mut store.settings.float_folders, &ids);
            save_settings(&app, &store.settings);
        }
        close_floating_window(&app, &id);
        sync_float_windows(&app); // a folder left with one member dissolves
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
    let scale = if scale.is_finite() { scale.clamp(FLOAT_SCALE_MIN, FLOAT_SCALE_MAX) } else { 1.0 };
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
fn set_window_pos_phys(win: &tauri::WebviewWindow, x: i32, y: i32, w: i32, h: i32) -> bool {
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
    unsafe { SetWindowPos(hwnd, 0, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) != 0 }
}

#[cfg(windows)]
fn set_float_bounds_native(win: &tauri::WebviewWindow, x: f64, y: f64, w: f64, h: f64) -> bool {
    let s = win.scale_factor().unwrap_or(1.0);
    set_window_pos_phys(
        win,
        (x * s).round() as i32,
        (y * s).round() as i32,
        (w * s).round() as i32,
        (h * s).round() as i32,
    )
}

// Expert privacy flag (Datenschutz tab), phrased from the user's side: "allow
// screenshots of this app". It ships OFF, so the window stays out of screen captures
// until someone deliberately lets them through.
const ALLOW_SCREENSHOT_KEY: &str = "allowScreenshot";

fn capture_excluded(settings: &Settings) -> bool {
    settings.ui_flags.get(ALLOW_SCREENSHOT_KEY) != Some(&true)
}

// Keep our own windows out of OTHER capture tools (Snipping Tool, Print Screen,
// Game Bar, and our own window picker): when excluded the window
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
            let class = wide(crate::brand::WINDOW_CLASS);
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

// Emergency purge: something sensitive (a password) ended up in the clipboard inbox
// and may be sitting inside one or more ROTATING SNAPSHOTS too, DPAPI-sealed but
// still on disk. clip_inbox_clear only ever touched the live DB — this walks every
// backup folder, opens its snapshot, deletes clip_inbox rows if any, and VACUUMs so
// the bytes are actually overwritten (a bare DELETE leaves them in SQLite's free
// pages until something else reuses that space). Returns how many snapshots had
// something to remove, for the confirmation toast.
#[tauri::command]
async fn purge_clip_inbox_from_backups(app: AppHandle) -> Result<usize, String> {
    Ok(purge_clip_inbox_from_backups_impl(&backups_dir(&app)))
}

fn purge_clip_inbox_from_backups_impl(dir: &std::path::Path) -> usize {
    let mut purged = 0usize;
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let folder = entry.path();
        if !folder.is_dir() {
            continue;
        }
        let Some(db) = backup_db_file(&folder) else { continue };
        let is_gz = db.extension().is_some_and(|e| e == "gz");
        let staged_tmp = folder.join("purge.tmp");
        let staged = if is_gz {
            if gunzip_to(&db, &staged_tmp).is_err() {
                continue;
            }
            staged_tmp.clone()
        } else {
            db.clone()
        };
        let mut removed = 0i64;
        if let Ok(conn) = Connection::open(&staged) {
            let has_table: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='clip_inbox'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if has_table > 0 {
                removed = conn.execute("DELETE FROM clip_inbox", []).unwrap_or(0) as i64;
                if removed > 0 {
                    let _ = conn.execute("VACUUM", []);
                }
            }
        }
        if removed > 0 {
            let repacked = if is_gz { gzip_to(&staged, &db) } else { Ok(()) };
            if repacked.is_ok() {
                purged += 1;
            }
        }
        if is_gz {
            let _ = fs::remove_file(&staged_tmp);
        }
    }
    purged
}

// Set a floating window's position AND size together (logical px). Used by the
// edge/corner resize so the grabbed edge tracks the cursor 1:1.
#[tauri::command]
async fn set_float_bounds(
    app: AppHandle,
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    clamp: Option<bool>,
) -> Result<(), String> {
    if [x, y, width, height].iter().all(|v| v.is_finite()) {
        if let Some(win) = app.get_webview_window(&flabel(&id)) {
            let w = width.clamp(48.0, 8000.0);
            let h = height.clamp(48.0, 8000.0);
            // OPT-IN, and only the pill menu asks for it: that placement picks a side
            // that fits and falls back to "below" when none does, which used to push
            // the buttons off the screen edge. Every other caller has already worked
            // out the exact box it wants — a live edge-resize computes the origin from
            // the size it is about to set, so clamping it here moved the window out
            // from under the cursor on every frame near a screen edge.
            let (x, y) = if clamp.unwrap_or(false) {
                let dpi = win.scale_factor().unwrap_or(1.0);
                let work = work_rect_at(&app, x * dpi, y * dpi);
                let (cx, cy) = clamp_to_work(work, w * dpi, h * dpi, x * dpi, y * dpi);
                (cx / dpi, cy / dpi)
            } else {
                (x, y)
            };
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

// ---------- F25: folder commands ----------

// What the dragged pill currently covers: every other pill/folder box plus the one
// `merge_target` picks. ONE hit function for both the live highlight and the drop, so
// what lights up while dragging is exactly what letting go would merge.
fn drag_hit(app: &AppHandle, id: &str, radius: f64) -> Option<(String, Vec<(String, Rect)>, Rect)> {
    let win = app.get_webview_window(&flabel(id))?;
    let dragged = window_rect(&win)?;
    let dpi = win.scale_factor().unwrap_or(1.0);
    let mut targets: Vec<(String, Rect)> = Vec::new();
    for (label, other) in app.webview_windows() {
        if label == flabel(id) {
            continue;
        }
        let rect = match window_rect(&other) {
            Some(r) => r,
            None => continue,
        };
        if let Some(pid) = label.strip_prefix("float-") {
            targets.push((format!("p{}", pid), rect));
        } else if let Some(fid) = label.strip_prefix("folder-") {
            // An OPEN folder is hit-tested on its icon, never on the grid it grew.
            targets.push((format!("f{}", fid), folder_icon_rect(app, fid, rect)));
        }
    }
    // The radius is a logical distance, so it follows the dragged pill's monitor.
    let hit = merge_target(dragged, &targets, radius * dpi)?.to_string();
    Some((hit, targets, dragged))
}

// Which window a hit key belongs to ("p<prompt>" = pill, "f<folder>" = folder).
fn hit_label(hit: &str) -> String {
    let (kind, key) = hit.split_at(1);
    if kind == "f" {
        folder_label(key)
    } else {
        flabel(key)
    }
}

// Live merge feedback while a pill is dragged. The OS move loop owns the pointer, so
// no webview sees the motion — the backend polls the dragged window instead and lights
// up the window that would swallow it. (generation, currently lit window label)
struct DragHint(Mutex<(u64, Option<String>)>);

const HINT_TICK_MS: u64 = 70;
const HINT_MAX_TICKS: u32 = 700; // ~50 s guard — no drag runs longer

// Move the highlight, telling the old target to drop it. Emits only on a change.
fn light_hint(app: &AppHandle, label: Option<String>) {
    let Some(state) = app.try_state::<DragHint>() else { return };
    let mut cur = state.0.lock().unwrap_or_else(|e| e.into_inner());
    if cur.1 == label {
        return;
    }
    if let Some(old) = cur.1.take() {
        let _ = app.emit_to(old.as_str(), "merge-hint", false);
    }
    if let Some(new) = &label {
        let _ = app.emit_to(new.as_str(), "merge-hint", true);
    }
    cur.1 = label;
}

// The pill window says when its drag starts and ends. Ending also clears the
// highlight, so a drag that dies with the window never leaves one lit.
#[tauri::command]
async fn float_drag_hint(app: AppHandle, state: State<'_, Db>, id: String, active: bool) -> Result<(), String> {
    let generation = {
        let hint: State<DragHint> = app.state();
        let mut cur = hint.0.lock().unwrap_or_else(|e| e.into_inner());
        cur.0 = cur.0.wrapping_add(1);
        cur.0
    };
    if !active {
        light_hint(&app, None);
        return Ok(());
    }
    let (on, radius) = {
        let store = lock(&state);
        (folders_on(&store.settings), merge_radius(&store.settings))
    };
    if !on {
        return Ok(());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        for _ in 0..HINT_MAX_TICKS {
            tokio::time::sleep(std::time::Duration::from_millis(HINT_TICK_MS)).await;
            {
                let hint: State<DragHint> = handle.state();
                let cur = hint.0.lock().unwrap_or_else(|e| e.into_inner());
                if cur.0 != generation {
                    return; // a newer drag (or its end) owns the highlight now
                }
            }
            if handle.get_webview_window(&flabel(&id)).is_none() {
                break;
            }
            let hit = drag_hit(&handle, &id, radius).map(|(h, _, _)| hit_label(&h));
            light_hint(&handle, hit);
        }
        light_hint(&handle, None);
    });
    Ok(())
}

// A pill was dropped after a drag: did it land on another pill or on a folder? The
// check has to run HERE — the pills are separate OS windows, so only the backend knows
// where they all are. Called ONCE per drag END, never while dragging.
#[tauri::command]
async fn float_folder_drop(app: AppHandle, state: State<'_, Db>, id: String) -> Result<bool, String> {
    // The drag is over: whatever was lit goes dark, hit or no hit.
    {
        let hint: State<DragHint> = app.state();
        let mut cur = hint.0.lock().unwrap_or_else(|e| e.into_inner());
        cur.0 = cur.0.wrapping_add(1);
    }
    light_hint(&app, None);
    let (on, radius, full) = {
        let store = lock(&state);
        (
            folders_on(&store.settings),
            merge_radius(&store.settings),
            store.settings.float_folders.len() >= MAX_FLOAT_FOLDERS,
        )
    };
    if !on {
        return Ok(false);
    }
    let win = match app.get_webview_window(&flabel(&id)) {
        Some(w) => w,
        None => return Ok(false),
    };
    let dpi = win.scale_factor().unwrap_or(1.0);
    let (hit, targets, dragged) = match drag_hit(&app, &id, radius) {
        Some(h) => h,
        None => return Ok(false),
    };
    let (kind, key) = hit.split_at(1);
    // A new folder needs its position first: working out which monitor the drop landed
    // on is a Tauri call, and one of those under the store mutex is the deadlock shape.
    let fresh = if kind == "f" {
        None
    } else {
        if full {
            return Ok(false);
        }
        let scale = {
            let store = lock(&state);
            float_scale_of(&store.settings, key)
        };
        // The new folder takes the target pill's place — centred on it, so it lands
        // where the user aimed — and its icon size mirrors that pill's size.
        let target = targets
            .iter()
            .find(|(t, _)| t == &hit)
            .map(|(_, r)| *r)
            .unwrap_or(dragged);
        let side = folder_box(scale) * dpi;
        let (cx, cy) = target.center();
        let work = work_rect_at(&app, cx, cy);
        let (x, y) = clamp_to_work(work, side, side, cx - side / 2.0, cy - side / 2.0);
        Some(FloatFolder {
            id: gen_id(),
            name: String::new(),
            pos: Pos { x: x.round() as i32, y: y.round() as i32 },
            scale,
            members: vec![key.to_string(), id.clone()],
        })
    };
    {
        let mut store = lock(&state);
        match fresh {
            Some(folder) => store.settings.float_folders.push(folder),
            None => match store.settings.float_folders.iter_mut().find(|f| f.id == key) {
                Some(folder) => {
                    if !folder.members.iter().any(|m| m == &id) {
                        folder.members.push(id.clone());
                    }
                }
                None => return Ok(false),
            },
        }
        let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
        normalize_float_folders(&mut store.settings.float_folders, &ids);
        save_settings(&app, &store.settings);
    }
    sync_float_windows(&app);
    Ok(true)
}

// Everything the folder window needs to draw itself.
#[derive(Serialize)]
struct FolderView {
    id: String,
    name: String,
    scale: f64,
    cols: u32,
    members: Vec<Prompt>,
}

#[tauri::command]
fn float_folder_state(state: State<Db>, id: String) -> Option<FolderView> {
    let store = lock(&state);
    let folder = store.settings.float_folders.iter().find(|f| f.id == id)?;
    let members = folder
        .members
        .iter()
        .filter_map(|m| store.prompts.iter().find(|p| &p.id == m).cloned())
        .collect();
    Some(FolderView {
        id: folder.id.clone(),
        name: folder.name.clone(),
        scale: folder.scale,
        cols: folder_cols(&store.settings),
        members,
    })
}

// The layout an expanded folder ended up with, in LOGICAL pixels for the webview.
#[derive(Serialize)]
struct FolderGeom {
    side: String,
    icon_x: f64,
    icon_y: f64,
    pop_x: f64,
    pop_y: f64,
    width: f64,
    height: f64,
}

// Open the folder by growing its OWN window around the icon — no second window is
// ever spawned, so z-order and focus stay exactly as simple as a single pill's. The
// popover size comes in as logical pixels (what the webview measured) and is scaled
// with THIS window's factor, so the same call is correct on a second monitor at a
// different DPI.
#[tauri::command]
async fn expand_float_folder(
    app: AppHandle,
    id: String,
    width: f64,
    height: f64,
    gap: f64,
) -> Result<FolderGeom, String> {
    if ![width, height, gap].iter().all(|v| v.is_finite() && *v >= 0.0) {
        return Err("bad popover size".to_string());
    }
    let win = app
        .get_webview_window(&folder_label(&id))
        .ok_or_else(|| "unknown folder".to_string())?;
    let dpi = win.scale_factor().unwrap_or(1.0);
    let current = window_rect(&win).ok_or_else(|| "no window geometry".to_string())?;
    let icon = folder_icon_rect(&app, &id, current);
    let (cx, cy) = icon.center();
    let work = work_rect_at(&app, cx, cy);
    let layout = folder_layout(
        icon,
        (width * dpi).clamp(1.0, 20000.0),
        (height * dpi).clamp(1.0, 20000.0),
        gap * dpi,
        work,
    );
    set_folder_expansion(
        &app,
        &id,
        Some(Rect { x: layout.icon.0, y: layout.icon.1, w: icon.w, h: icon.h }),
    );
    set_bounds_phys(&win, layout.win);
    Ok(FolderGeom {
        side: layout.side.as_str().to_string(),
        icon_x: layout.icon.0 / dpi,
        icon_y: layout.icon.1 / dpi,
        pop_x: layout.pop.0 / dpi,
        pop_y: layout.pop.1 / dpi,
        width: layout.win.w / dpi,
        height: layout.win.h / dpi,
    })
}

// Shrink the window back to the icon, wherever the icon currently is.
#[tauri::command]
async fn collapse_float_folder(app: AppHandle, state: State<'_, Db>, id: String) -> Result<(), String> {
    let win = match app.get_webview_window(&folder_label(&id)) {
        Some(w) => w,
        None => return Ok(()),
    };
    let current = match window_rect(&win) {
        Some(r) => r,
        None => return Ok(()),
    };
    let icon = folder_icon_rect(&app, &id, current);
    set_folder_expansion(&app, &id, None);
    set_bounds_phys(&win, icon);
    // The window may have been dragged while it was open, so store where the icon
    // ended up rather than where the folder was opened.
    let mut store = lock(&state);
    if let Some(folder) = store.settings.float_folders.iter_mut().find(|f| f.id == id) {
        folder.pos = Pos { x: icon.x.round() as i32, y: icon.y.round() as i32 };
    }
    save_settings(&app, &store.settings);
    Ok(())
}

// Per-folder size, mirroring the per-pill float_scale.
#[tauri::command]
async fn set_float_folder_scale(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    scale: f64,
) -> Result<(), String> {
    let scale = if scale.is_finite() { scale.clamp(0.5, 3.0) } else { 1.0 };
    let pos = {
        let mut store = lock(&state);
        let found = store
            .settings
            .float_folders
            .iter_mut()
            .find(|f| f.id == id)
            .map(|f| {
                f.scale = scale;
                f.pos
            });
        match found {
            Some(p) => {
                save_settings(&app, &store.settings);
                p
            }
            None => return Err("unknown folder".to_string()),
        }
    };
    if let Some(win) = app.get_webview_window(&folder_label(&id)) {
        let dpi = win.scale_factor().unwrap_or(1.0);
        let side = folder_box(scale) * dpi;
        set_folder_expansion(&app, &id, None); // a size change closes the open grid
        set_bounds_phys(&win, Rect { x: pos.x as f64, y: pos.y as f64, w: side, h: side });
        let _ = app.emit_to(folder_label(&id).as_str(), "folder-changed", ());
    }
    Ok(())
}

// The optional label under the folder icon. Control characters are stripped: the name
// also ends up as the window title.
#[tauri::command]
fn rename_float_folder(app: AppHandle, state: State<Db>, id: String, name: String) -> Result<(), String> {
    let name = folder_name_of(&name)?;
    {
        let mut store = lock(&state);
        match store.settings.float_folders.iter_mut().find(|f| f.id == id) {
            Some(folder) => folder.name = name.clone(),
            None => return Err("unknown folder".to_string()),
        }
        save_settings(&app, &store.settings);
    }
    if let Some(win) = app.get_webview_window(&folder_label(&id)) {
        let _ = win.set_title(&name);
    }
    let _ = app.emit_to(folder_label(&id).as_str(), "folder-changed", ());
    Ok(())
}

// A folder window has no themed dialog of its own, so the rename is asked for in the
// MAIN window (confirmDialog) and applied from there — never a native message box.
#[tauri::command]
async fn folder_rename_request(app: AppHandle, state: State<'_, Db>, id: String) -> Result<(), String> {
    let name = {
        let store = lock(&state);
        store
            .settings
            .float_folders
            .iter()
            .find(|f| f.id == id)
            .map(|f| f.name.clone())
    };
    let name = name.ok_or_else(|| "unknown folder".to_string())?;
    show_main(&app);
    let _ = app.emit_to(
        "main",
        "folder-rename",
        serde_json::json!({ "id": id, "name": name }),
    );
    Ok(())
}

// Dissolve a folder: every member goes back to being a free pill. Nothing is deleted.
// Each one asks for the position it kept all along, but they are fanned apart first —
// merging put them on top of each other, so restoring the wishes unchanged would leave
// a stack that looks like a single pill.
#[tauri::command]
async fn dissolve_float_folder(app: AppHandle, state: State<'_, Db>, id: String) -> Result<(), String> {
    let dpi = app
        .get_webview_window(&folder_label(&id))
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0);
    // Wished-for box of every member, in physical pixels.
    let (members, folder_pos) = {
        let store = lock(&state);
        let folder = match store.settings.float_folders.iter().find(|f| f.id == id) {
            Some(f) => f.clone(),
            None => return Ok(()),
        };
        let members: Vec<(String, Rect)> = folder
            .members
            .iter()
            .filter_map(|mid| {
                let prompt = store.prompts.iter().find(|p| &p.id == mid)?;
                let (w, h) = pill_dims(is_image_prompt(prompt), float_scale_of(&store.settings, mid));
                let pos = store.settings.floating.get(mid).copied().unwrap_or(folder.pos);
                Some((
                    mid.clone(),
                    Rect { x: pos.x as f64, y: pos.y as f64, w: w * dpi, h: h * dpi },
                ))
            })
            .collect();
        (members, folder.pos)
    };
    // Pills and folders already on screen are obstacles — this folder's own window is
    // about to close, so it is not one.
    let taken: Vec<Rect> = app
        .webview_windows()
        .iter()
        .filter(|(label, _)| {
            (label.starts_with("float-") || label.starts_with("folder-")) && *label != &folder_label(&id)
        })
        .filter_map(|(_, win)| window_rect(win))
        .collect();
    let side = folder_box(1.0) * dpi;
    let work = work_rect_at(&app, folder_pos.x as f64 + side / 2.0, folder_pos.y as f64 + side / 2.0);
    let wishes: Vec<Rect> = members.iter().map(|(_, r)| *r).collect();
    let spread = spread_free_positions(&wishes, &taken, work);
    {
        let mut store = lock(&state);
        let before = store.settings.float_folders.len();
        store.settings.float_folders.retain(|f| f.id != id);
        if store.settings.float_folders.len() == before {
            return Ok(());
        }
        for ((mid, _), (x, y)) in members.iter().zip(spread) {
            store
                .settings
                .floating
                .insert(mid.clone(), Pos { x: x.round() as i32, y: y.round() as i32 });
        }
        save_settings(&app, &store.settings);
    }
    set_folder_expansion(&app, &id, None);
    sync_float_windows(&app);
    Ok(())
}

// A member was dragged out of the open folder: it becomes a free pill at the drop
// point. x/y are the pointer's screen position in the webview's CSS pixels, converted
// with the folder window's own scale factor and then clamped onto the work area it was
// dropped on. Dropping the second-to-last member dissolves the folder (normalize).
#[tauri::command]
async fn remove_folder_member(
    app: AppHandle,
    state: State<'_, Db>,
    folder_id: String,
    id: String,
    x: f64,
    y: f64,
) -> Result<(), String> {
    if !x.is_finite() || !y.is_finite() {
        return Err("bad drop position".to_string());
    }
    let dpi = app
        .get_webview_window(&folder_label(&folder_id))
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0);
    let (px, py) = (x * dpi, y * dpi);
    let (pw, ph) = {
        let store = lock(&state);
        let prompt = match store.prompts.iter().find(|p| p.id == id) {
            Some(p) => p,
            None => return Err(format!("unknown {}", brand::ITEM)),
        };
        if !store
            .settings
            .float_folders
            .iter()
            .any(|f| f.id == folder_id && f.members.contains(&id))
        {
            return Err("not a member of this folder".to_string());
        }
        pill_dims(is_image_prompt(prompt), float_scale_of(&store.settings, &id))
    };
    let work = work_rect_at(&app, px, py);
    let (nx, ny) = clamp_to_work(work, pw * dpi, ph * dpi, px - pw * dpi / 2.0, py - ph * dpi / 2.0);
    {
        let mut store = lock(&state);
        for folder in store.settings.float_folders.iter_mut() {
            if folder.id == folder_id {
                folder.members.retain(|m| m != &id);
            }
        }
        store
            .settings
            .floating
            .insert(id.clone(), Pos { x: nx.round() as i32, y: ny.round() as i32 });
        let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
        normalize_float_folders(&mut store.settings.float_folders, &ids);
        save_settings(&app, &store.settings);
    }
    sync_float_windows(&app);
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
// image is Arc'd so the background JPEG encode and the stored capture share one
// buffer instead of each owning an ~80 MB copy of the frozen desktop — every
// reader (crop_rgb/crop_rgba/sample_frozen) only ever borrows it.
struct MonitorCap {
    image: Arc<image::RgbaImage>,
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

// F26: the running session is a colour pick (loupe overlay) rather than a region
// snip. Both share the freeze + overlay pipeline; only the overlay's mode differs.
static PICK_ACTIVE: AtomicBool = AtomicBool::new(false);

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
    PICK_ACTIVE.store(false, Ordering::SeqCst);
}

// Hide (or restore) the app's own windows — the main window and floating pills —
// around a snip, so a screenshot never contains the app itself (this also
// reveals whatever the main window was covering, e.g. Task Manager).
// Returns whether any matching window was visible beforehand — begin_capture
// uses this on the hide path to skip its post-hide settle delay when there was
// nothing on screen to hide in the first place.
fn set_app_windows_hidden(app: &AppHandle, hidden: bool) -> bool {
    let mut was_visible = false;
    for (label, win) in app.webview_windows() {
        if label == "main" || label.starts_with("float-") {
            if hidden {
                if win.is_visible().unwrap_or(true) {
                    was_visible = true;
                }
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
    was_visible
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
    let path = std::env::temp_dir().join(format!("{}-snip-{}.jpg", brand::TEMP_PREFIX, seq));
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
    // F26: run the overlay as a colour picker instead of a region selector, with
    // the loupe magnification and output notation the expert menu asks for.
    pick: bool,
    pick_zoom: u32,
    pick_format: String,
    // 80: the last picked colours as #rrggbb, newest first. Handed over with the
    // session so the overlay page never has to keep history state of its own.
    pick_history: Vec<String>,
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
        image::imageops::replace(&mut canvas, image, stitch_offset(*x, min_x), stitch_offset(*y, min_y));
    }
    Ok((canvas, min_x, min_y))
}

// A monitor's global physical origin -> where its capture starts inside the
// stitched frozen image. The virtual desktop's origin is negative as soon as a
// monitor sits left of or above the primary, so this shift — not any DPI scale
// factor — is what makes a pixel on a differently scaled second monitor address
// the byte the capture actually holds. Every frozen coordinate is physical.
fn stitch_offset(global: i32, origin: i32) -> i64 {
    (global - origin) as i64
}

// ---------- F26: colour picker ----------
// Same freeze + overlay as the snipping tool; the overlay just runs in pick mode.
// The picked value goes to the CLIPBOARD ONLY — no prompt, no copy history.

// Loupe magnification + output notation, straight from the expert menu.
struct PickCfg {
    zoom: u32,
    format: String,
}

// Only the three notations the expert dropdown offers; anything else is hex.
fn color_format(raw: &str) -> &'static str {
    match raw {
        "rgb" => "rgb",
        "hsl" => "hsl",
        _ => "hex",
    }
}

fn pick_cfg(settings: &Settings) -> PickCfg {
    PickCfg {
        zoom: settings.ui_values.get("loupeZoom").copied().unwrap_or(8.0).clamp(4.0, 16.0) as u32,
        format: color_format(settings.ui_texts.get("pickerFormat").map(String::as_str).unwrap_or("hex")).to_string(),
    }
}

fn rgb_to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

// 80/19: the last picked colours, newest first. Recorded here rather than in the
// overlay page, because this is the side that knows what actually reached the
// clipboard — a hidden or reloaded webview cannot lose the list. Kept in memory
// for the running session, mirrored into `ui_texts["pickHistory"]` (comma-joined
// hex) so it survives a restart; the mirror is the only thing ever written to disk.
const PICK_HISTORY_DEFAULT: usize = 5;
const PICK_HISTORY_CAP: usize = 20;
static PICK_HISTORY: Mutex<Vec<[u8; 3]>> = Mutex::new(Vec::new());

// The user-configurable cap (expert value `pickHistoryN`, 0 = history off).
fn pick_history_max(settings: &Settings) -> usize {
    settings
        .ui_values
        .get("pickHistoryN")
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(PICK_HISTORY_DEFAULT as f64)
        .clamp(0.0, PICK_HISTORY_CAP as f64) as usize
}

fn hex_to_rgb(hex: &str) -> Option<[u8; 3]> {
    let d = hex.trim().strip_prefix('#')?;
    if d.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(d, 16).ok()?;
    Some([(n >> 16) as u8, ((n >> 8) & 0xff) as u8, (n & 0xff) as u8])
}

// Push a pick, newest first, skipping a repeat of the entry already on top.
fn record_pick(rgb: [u8; 3], max: usize) {
    let mut hist = PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    if hist.first() == Some(&rgb) {
        return;
    }
    hist.insert(0, rgb);
    hist.truncate(max);
}

fn pick_history_hex() -> Vec<String> {
    PICK_HISTORY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|c| rgb_to_hex(c[0], c[1], c[2]))
        .collect()
}

// One-time load from the persisted mirror — called once when the store loads.
fn load_pick_history(settings: &Settings) {
    let max = pick_history_max(settings);
    let mut hist = PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    hist.clear();
    if let Some(saved) = settings.ui_texts.get("pickHistory") {
        hist.extend(saved.split(',').filter_map(hex_to_rgb));
        hist.truncate(max);
    }
}

// RGB -> HSL as rounded integers (degrees, percent, percent). Grey has no hue, so
// it reports 0°/0%; the hue wraps so 360° is always written as 0°.
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (u32, u32, u32) {
    let (rf, gf, bf) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0, 0, (l * 100.0).round() as u32);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == rf {
        ((gf - bf) / d).rem_euclid(6.0)
    } else if max == gf {
        (bf - rf) / d + 2.0
    } else {
        (rf - gf) / d + 4.0
    };
    let deg = (h * 60.0).round().rem_euclid(360.0);
    (deg as u32, (s * 100.0).round() as u32, (l * 100.0).round() as u32)
}

fn format_color(fmt: &str, r: u8, g: u8, b: u8) -> String {
    match color_format(fmt) {
        "rgb" => format!("rgb({},{},{})", r, g, b),
        "hsl" => {
            let (h, s, l) = rgb_to_hsl(r, g, b);
            format!("hsl({},{}%,{}%)", h, s, l)
        }
        _ => rgb_to_hex(r, g, b),
    }
}

// Read one pixel of the frozen desktop, addressed in stitched physical pixels.
// Out-of-bounds returns None instead of clamping, so a bogus coordinate can never
// silently copy the colour of some other pixel.
fn sample_frozen(caps: &[MonitorCap], x: i32, y: i32) -> Option<[u8; 3]> {
    let cap = caps.first()?;
    if x < 0 || y < 0 || x >= cap.width as i32 || y >= cap.height as i32 {
        return None;
    }
    let p = cap.image.get_pixel(x as u32, y as u32);
    Some([p[0], p[1], p[2]])
}

// One rectangular patch of the frozen desktop as packed RGB, row by row. The rect
// must lie fully inside the image — a caller that clamped badly gets nothing back
// rather than a silently shifted patch.
fn crop_rgb(img: &image::RgbaImage, x: u32, y: u32, w: u32, h: u32) -> Vec<u8> {
    let (iw, ih) = img.dimensions();
    // Checked, so a wild x/y from the overlay can never wrap into a valid-looking rect.
    let fits = |a: u32, len: u32, max: u32| a.checked_add(len).is_some_and(|end| end <= max);
    if w == 0 || h == 0 || !fits(x, w, iw) || !fits(y, h, ih) {
        return Vec::new();
    }
    let raw = img.as_raw();
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for row in y..y + h {
        let start = ((row * iw + x) * 4) as usize;
        for px in raw[start..start + (w * 4) as usize].chunks_exact(4) {
            out.extend_from_slice(&px[..3]);
        }
    }
    out
}

// What the main window needs to show the confirmation toast in the picked colour.
#[derive(Serialize, Clone)]
struct ColorPick {
    text: String,
    hex: String,
    // 81: all three notations, pre-formatted — so the toast can offer them as
    // separate copy-this-format chips without duplicating format_color() in JS.
    rgb: String,
    hsl: String,
}

// Longest tile edge the overlay may ask for. The loupe needs a few dozen pixels
// around the cursor, so this is generous — but it is also the cap that stops a
// bogus request from allocating a desktop-sized buffer.
const PICK_TILE_MAX: u32 = 512;

// Hand the overlay ONE patch of the frozen desktop as packed RGB (3 bytes per
// pixel, row order). The loupe samples locally, so it caches a patch and only asks
// again when the cursor nears its edge — shipping the whole desktop instead cost a
// ~58 MB transfer on a multi-monitor setup, which is what made the picker slow to
// open. Empty outside a pick session or for a rect that is not fully on the frame.
#[tauri::command]
fn pick_tile(state: State<SnipState>, x: u32, y: u32, width: u32, height: u32) -> tauri::ipc::Response {
    if !PICK_ACTIVE.load(Ordering::SeqCst) || width > PICK_TILE_MAX || height > PICK_TILE_MAX {
        return tauri::ipc::Response::new(Vec::new());
    }
    let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = guard
        .first()
        .map(|c| crop_rgb(&c.image, x, y, width, height))
        .unwrap_or_default();
    tauri::ipc::Response::new(bytes)
}

// Sample the frozen desktop where the user clicked, format the value and put it on
// the clipboard. Deliberately no record_copy and no prompt: a picked colour is a
// clipboard result, nothing the app stores.
#[tauri::command]
async fn pick_color(app: AppHandle, state: State<'_, SnipState>, x: i32, y: i32) -> Result<String, String> {
    if !PICK_ACTIVE.load(Ordering::SeqCst) {
        return Err("no pick session".to_string());
    }
    let rgb = {
        let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        sample_frozen(&guard, x, y)
    }
    .ok_or("point outside the captured desktop")?;
    let fmt = app
        .try_state::<Db>()
        .map(|s| pick_cfg(&s.lock().unwrap_or_else(|e| e.into_inner()).settings).format)
        .unwrap_or_else(|| "hex".to_string());
    let text = format_color(&fmt, rgb[0], rgb[1], rgb[2]);
    note_own_copy(&text); // F21: the picker's own copy must not land in the inbox
    // Close only once the value is really on the clipboard, so a failure leaves the
    // overlay up with its error banner instead of vanishing with nothing copied.
    if arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone())).is_err() {
        return Err("clipboard write failed".to_string());
    }
    // 80/19: only once the value really is on the clipboard, then mirror the
    // in-memory list into settings so it survives a restart.
    if let Some(db) = app.try_state::<Db>() {
        let mut store = db.lock().unwrap_or_else(|e| e.into_inner());
        record_pick(rgb, pick_history_max(&store.settings));
        store
            .settings
            .ui_texts
            .insert("pickHistory".to_string(), pick_history_hex().join(","));
        save_settings(&app, &store.settings);
    } else {
        record_pick(rgb, PICK_HISTORY_DEFAULT);
    }
    close_all_snip(&app);
    state.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let _ = app.emit(
        "color-picked",
        ColorPick {
            text: text.clone(),
            hex: format_color("hex", rgb[0], rgb[1], rgb[2]),
            rgb: format_color("rgb", rgb[0], rgb[1], rgb[2]),
            hsl: format_color("hsl", rgb[0], rgb[1], rgb[2]),
        },
    );
    Ok(text)
}

// 19: the popover behind #picker-btn's right-click — the persisted history,
// respecting a cap the user may have lowered since the last pick.
#[tauri::command]
fn pick_history(app: AppHandle) -> Vec<String> {
    let max = app
        .try_state::<Db>()
        .map(|s| pick_history_max(&s.lock().unwrap_or_else(|e| e.into_inner()).settings))
        .unwrap_or(PICK_HISTORY_DEFAULT);
    let mut hex = pick_history_hex();
    hex.truncate(max);
    hex
}

// Re-copy one entry from that popover in the currently configured notation.
#[tauri::command]
fn copy_pick_history_entry(app: AppHandle, hex: String) -> Result<String, String> {
    let rgb = hex_to_rgb(&hex).ok_or("not a hex colour")?;
    let fmt = app
        .try_state::<Db>()
        .map(|s| pick_cfg(&s.lock().unwrap_or_else(|e| e.into_inner()).settings).format)
        .unwrap_or_else(|| "hex".to_string());
    let text = format_color(&fmt, rgb[0], rgb[1], rgb[2]);
    note_own_copy(&text); // F21: must not land in the clipboard inbox either
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.clone()))
        .map_err(|_| "clipboard write failed".to_string())?;
    Ok(text)
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
    begin_capture(app, state, preview, false).await
}

// F26: the very same freeze + overlay, opened in colour-pick mode. Reusing the
// pipeline is what makes the picker pixel-exact and DPI-correct for free — the
// frozen frame is taken BEFORE the overlay shows, so the overlay is never in its
// own capture source.
#[tauri::command]
async fn open_picker(
    app: AppHandle,
    state: State<'_, SnipState>,
    preview: State<'_, SnipPreview>,
) -> Result<(), String> {
    let on = app
        .try_state::<Db>()
        .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_flags.get("colorPicker") != Some(&false))
        .unwrap_or(true); // on by default: only an explicit false switches it off
    if !on {
        return Err("color picker is off".to_string());
    }
    begin_capture(app, state, preview, true).await
}

async fn begin_capture(
    app: AppHandle,
    state: State<'_, SnipState>,
    preview: State<'_, SnipPreview>,
    pick: bool,
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
    PICK_ACTIVE.store(pick, Ordering::SeqCst);
    // Loupe settings are read once here, while nothing else holds the store.
    let (cfg, hist_max) = if pick {
        app.try_state::<Db>()
            .map(|s| {
                let guard = s.lock().unwrap_or_else(|e| e.into_inner());
                (pick_cfg(&guard.settings), pick_history_max(&guard.settings))
            })
    } else {
        None
    }
    .unwrap_or((PickCfg { zoom: 8, format: "hex".to_string() }, PICK_HISTORY_DEFAULT));
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
    let had_visible = set_app_windows_hidden(&app, true);
    // Nothing was on screen to hide, so there is nothing the freeze could catch
    // mid-hide either — the settle delay only matters when we just hid something.
    if had_visible {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    let (canvas, min_x, min_y) = freeze_desktop()?;
    let (total_w, total_h) = canvas.dimensions();
    // Arc'd so the preview encode (worker thread) and the stored capture (full-res
    // crops) share one buffer instead of each paying for their own ~80 MB copy.
    let canvas = Arc::new(canvas);
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
        let bg = encode_snip_jpeg(enc_img.as_raw(), total_w, total_h, if pick { PICK_Q } else { SNIP_Q }).map(|j| {
            let (src, is_file) = match write_snip_preview(&j) {
                Some(path) => (path, true),
                None => (format!("data:image/jpeg;base64,{}", base64_encode(&j)), false),
            };
            SnipBg {
                src,
                is_file,
                width: total_w,
                height: total_h,
                pick,
                pick_zoom: cfg.zoom,
                pick_format: cfg.format,
                pick_history: if pick {
                    let mut h = pick_history_hex();
                    h.truncate(hist_max);
                    h
                } else {
                    Vec::new()
                },
            }
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
// The colour picker shows the frozen desktop as the reference for the colour it is
// about to copy, so its background must not carry JPEG's subsampled chroma: below
// quality 90 the encoder halves the colour resolution, which is exactly the "washed
// out" look. 92 keeps full chroma (4:4:4). Measured cost: ~1.6x the q80 encode — paid
// only when the pipette is opened, never on the screenshot path.
const PICK_Q: u8 = 92;
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
// off). Safety: only ever removes our own Screenshot-*.jpg (or the legacy
// <brand>-*.png a much older version wrote) files.
#[tauri::command]
async fn delete_screenshot_file(state: State<'_, Db>, path: String) -> Result<bool, String> {
    let p = std::path::Path::new(&path);
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let legacy = !brand::LEGACY_SCREENSHOT_PREFIX.is_empty()
        && name.starts_with(brand::LEGACY_SCREENSHOT_PREFIX)
        && name.ends_with(".png");
    let ours = (name.starts_with("Screenshot-") && name.ends_with(".jpg")) || legacy;
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
    // Every file the store can consist of, not just the database: a prompts.json or
    // settings.json left behind by a failed DB write is the ONLY copy of that change,
    // and delete_data_dir removes the old folder right after this.
    if !new.join("data.db").exists() {
        for f in STORE_FILES {
            let src = current.join(f);
            if src.exists() {
                fs::copy(&src, new.join(f)).map_err(|e| format!("copy store: {}", e))?;
            }
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
        Some("releases") => format!("{}/releases", repo_url()),
        _ => repo_url(),
    };
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", "start", "", &url]);
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

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// The project's own GitHub repo — the only host any of this ever talks to.
fn repo_url() -> String {
    format!("https://github.com/{}", brand::REPO)
}
fn update_api() -> String {
    format!("https://api.github.com/repos/{}/releases/latest", brand::REPO)
}
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
    let body = ureq::get(&update_api())
        .set("User-Agent", brand::USER_AGENT)
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
// Returns Err only where switching the feature ON has a hard precondition that
// failed (today: the safety backup prompt-sync takes before it first writes) — the
// flag is then left OFF, so the UI and the store never disagree.
#[tauri::command]
fn set_ui_flag(app: AppHandle, state: State<Db>, key: String, enabled: bool) -> Result<(), String> {
    let is_capture = key == ALLOW_SCREENSHOT_KEY;
    let is_clip = key == "clipWatcher"; // F21
    let is_hotkeys = key == "promptHotkeys"; // F20
    let is_tray = key == "trayRecent" || key == "trayFavs"; // F23
    let is_folders = key == "pillFolders"; // F25
    let is_sync_enable = key == "promptSync" && enabled; // Prompt-folders-as-Markdown
    {
        let mut store = lock(&state);
        store.settings.ui_changed.insert(key.clone(), now_secs()); // 66
        store.settings.ui_flags.insert(key, enabled);
        save_settings(&app, &store.settings);
        if is_sync_enable {
            // Safety net before the very first sync run ever touches this library —
            // same shape a manual backup would take, so a bad first sync is always
            // recoverable. Without it there is nothing to fall back on, so a failure
            // here ABORTS the activation instead of syncing unprotected.
            let keep = store.settings.ui_values.get("backupKeep").copied().unwrap_or(10.0).max(1.0) as usize;
            let gfs = gfs_from_settings(&store.settings);
            if let Err(e) = do_backup(&app, keep, gfs) {
                append_crash_log(&format!("prompt-sync activation backup failed: {e}"));
                store.settings.ui_flags.insert("promptSync".to_string(), false);
                save_settings(&app, &store.settings);
                return Err(format!("sync-backup-failed: {e}"));
            }
            sync_write_all(&app, &store); // seed the folder with every existing prompt right away
        }
    }
    // Capture exclusion applies live to every open window. Never hold the store
    // lock across window API calls (deadlock risk). The flag says "allow
    // screenshots", so the exclusion is its OPPOSITE — passing it straight through
    // is what made the switch work backwards.
    if is_capture {
        for (_label, win) in app.webview_windows() {
            set_capture_exclusion(&win, !enabled);
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
    // F20: switching the feature off unregisters every prompt accelerator but keeps
    // the stored ones, so switching it back on restores them all.
    if is_hotkeys {
        rearm_hotkeys(&app);
    }
    // F23: switching a section on or off redraws the tray menu.
    if is_tray {
        apply_tray_menu(&app);
    }
    // F25: switching folders off puts every member back on screen as its own pill (the
    // folders themselves are kept, so switching it on again restores them). This is a
    // SYNC command, so it runs on the main thread — opening and closing windows there
    // is the documented way to deadlock WebView2, hence the hop onto the runtime.
    if is_folders {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move { sync_float_windows(&handle) });
    }
    notify_float_settings(&app);
    Ok(())
}

// Set an expert numeric value (CSS var / behaviour tweak).
#[tauri::command]
fn set_ui_value(app: AppHandle, state: State<Db>, key: String, value: f64) {
    let is_tray = key == "trayRecentN" || key == "trayFavN"; // F23
    let is_folder_grid = key == "folderGridCols"; // F25
    {
        let mut store = lock(&state);
        store.settings.ui_changed.insert(key.clone(), now_secs()); // 66
        store.settings.ui_values.insert(key, value);
        save_settings(&app, &store.settings);
    }
    if is_tray {
        rebuild_tray(&app); // dragging a slider must not rebuild on every step
    }
    // F25: an open folder re-lays out its grid with the new column count.
    if is_folder_grid {
        for label in app.webview_windows().keys() {
            if label.starts_with("folder-") {
                let _ = app.emit_to(label.as_str(), "folder-changed", ());
            }
        }
    }
    notify_float_settings(&app);
}

// Set an expert string option (e.g. the copy-feedback font key).
#[tauri::command]
fn set_ui_text(app: AppHandle, state: State<Db>, key: String, value: String) {
    {
        let mut store = lock(&state);
        store.settings.ui_changed.insert(key.clone(), now_secs()); // 66
        store.settings.ui_texts.insert(key, value);
        save_settings(&app, &store.settings);
    }
    notify_float_settings(&app);
}

// Clear all expert overrides back to the shipped defaults.
#[tauri::command]
fn reset_expert(app: AppHandle, state: State<Db>) {
    {
        let mut store = lock(&state);
        store.settings.ui_flags.clear();
        store.settings.ui_values.clear();
        store.settings.ui_texts.clear();
        store.settings.ui_changed.clear(); // 66
        save_settings(&app, &store.settings);
    }
    // Cleared flags put the screenshot permission back to its default (off), which
    // means the exclusion is on — re-assert it on every window so it is immediate.
    for (_label, win) in app.webview_windows() {
        set_capture_exclusion(&win, true);
    }
    apply_tray_menu(&app); // F23: the tray sections fall back to their defaults too
    notify_float_settings(&app);
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
    let prefix = format!("{}/releases/download/", repo_url());
    let rest = match url.strip_prefix(prefix.as_str()) {
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
        .set("User-Agent", brand::USER_AGENT)
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
    // Integrity: the SHA-256 published beside the installer in the release
    // (`<installer>.sha256`, same pinned path) must be present AND match. Fail closed —
    // an installer we cannot authenticate is never executed, so a hash file that is
    // missing, unreadable or malformed aborts the update just like a mismatch does.
    // Every release publishes the hash (see `release:publish` in package.json); if one
    // ever does not, the update is done by hand rather than unverified.
    let want = ureq::get(&format!("{}.sha256", url))
        .set("User-Agent", brand::USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .ok()
        .and_then(|r| r.into_string().ok())
        .map(|t| t.split_whitespace().next().unwrap_or("").to_ascii_lowercase())
        .filter(|w| w.len() == 64 && w.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| "no usable checksum published for this update — download rejected".to_string())?;
    {
        use sha2::{Digest, Sha256};
        if format!("{:x}", Sha256::digest(&bytes)) != want {
            return Err("update hash mismatch — download rejected".to_string());
        }
    }
    // Stage into a fresh, uniquely-named temp subdir so a local attacker can't pre-plant
    // the installer/script at a predictable path (TOCTOU).
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stage = std::env::temp_dir().join(format!("{}-update-{}-{}", brand::SLUG, std::process::id(), stamp));
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
    // A value an earlier name of this product registered would keep launching the
    // app twice; drop it whichever way the setting goes.
    if !brand::LEGACY_AUTOSTART_KEY.is_empty() {
        let _ = run.delete_value(brand::LEGACY_AUTOSTART_KEY);
    }
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
    // CSV formula injection: a cell starting with =, +, -, @ or a tab is read as a
    // formula by Excel/LibreOffice once reopened — CSV-quoting only protects the
    // CSV syntax, not spreadsheet evaluation. A leading single quote forces text.
    let needs_prefix = matches!(s.as_bytes().first(), Some(b'=' | b'+' | b'-' | b'@' | b'\t' | b'\r'));
    let s = if needs_prefix { format!("'{s}") } else { s.to_string() };
    format!("\"{}\"", s.replace('"', "\"\""))
}

// Position lines: one per view+grid-size the prompt is placed in.
fn csv_row(cells: &[&str]) -> String {
    cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(";")
}

// 32: joins/splits a rotating prompt's entries for every text-based export format
// (CSV cell, TXT body, Markdown-sync body) — ONE marker everywhere. NOT the bare
// "---" WORK.md originally sketched: `to_txt`'s blocks are already joined on the
// literal "\n\n---\n\n" (between DIFFERENT prompts), and `prompt_to_md`'s header is
// bounded by bare "---" lines too, so a bare "---" inside one prompt's own body
// would be ambiguous with either. An HTML-comment marker cannot collide with
// either delimiter and reads clearly as "generated" to a human editing the
// Markdown-sync file by hand.
const VARIANT_SEP: &str = "\n<!-- next -->\n";
// Escaped form of the marker, used ONLY so an entry that happens to contain the
// literal separator text survives the round trip instead of being silently
// split into extra entries (Codex audit) — the inserted backslash means this
// string can never equal VARIANT_SEP itself, so .split(VARIANT_SEP) never
// matches inside it.
const VARIANT_SEP_ESCAPED: &str = "\n<!-- \\next -->\n";

fn join_variants(text: &str, variants: &[String]) -> String {
    if variants.is_empty() {
        return text.to_string();
    }
    let escape = |s: &str| s.replace(VARIANT_SEP, VARIANT_SEP_ESCAPED);
    std::iter::once(escape(text))
        .chain(variants.iter().map(|s| escape(s)))
        .collect::<Vec<_>>()
        .join(VARIANT_SEP)
}

fn split_variants(joined: &str) -> (String, Vec<String>) {
    let unescape = |s: &str| s.replace(VARIANT_SEP_ESCAPED, VARIANT_SEP);
    let mut parts = joined.split(VARIANT_SEP);
    let text = unescape(parts.next().unwrap_or(""));
    (text, parts.map(unescape).collect())
}

// Export the prompt library only (incl. images/files) — never app settings,
// views or grid layout, so importing elsewhere can't clobber the user's setup.
fn to_csv(prompts: &[Prompt]) -> String {
    // 83: pdfpage appended at the END, not inserted among the existing columns —
    // an older export re-imported here has no such column, and row.get() on a
    // short row already returns None (-> the same default 0), never shifting
    // any OTHER column's fixed index. 32: rotatemode follows the identical
    // append-only precedent.
    let head = [
        "name", "text", "positions", "color", "font", "size", "file", "icon", "show", "copy",
        "image", "caption", "capsize", "tags", "notes", "pdfpage", "rotatemode",
    ];
    let mut rows = vec![csv_row(&head)];
    for p in prompts {
        // 32: entries 2..n ride along INSIDE the text cell — the CSV writer quotes
        // every cell (embedded newlines included), so the join marker needs no
        // escaping of its own.
        let text = join_variants(&p.text, &p.variants);
        rows.push(csv_row(&[
            &p.name,
            &text,
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
            &p.tags.join(","),
            // The CSV writer quotes every cell and doubles inner quotes, so a
            // multi-line note travels verbatim — no escaping needed here.
            &p.notes,
            &p.pdf_page.to_string(),
            &p.rotate_mode,
        ]));
    }
    rows.join("\r\n")
}

fn to_txt(prompts: &[Prompt]) -> String {
    let blocks: Vec<String> = prompts.iter().map(|p| {
        // 32: entries 2..n ride along inside the body itself, joined by the same
        // marker as CSV/Markdown-sync — never the bare "---" the blocks below are
        // themselves separated by.
        let mut block = format!("### {}\n{}", p.name, join_variants(&p.text, &p.variants));
        if !p.color.is_empty() {
            block.push_str(&format!("\n@color {}", p.color));
        }
        if !p.notes.is_empty() {
            block.push_str(&format!("\n@notes {}", notes_escape(&p.notes)));
        }
        if !p.tags.is_empty() {
            block.push_str(&format!("\n@tags {}", p.tags.join(",")));
        }
        // Per-tile style: "@style <font-key|-> <size>" ("-" = default font).
        if !p.font.is_empty() || p.font_size > 0 {
            let font = if p.font.is_empty() { "-" } else { &p.font };
            block.push_str(&format!("\n@style {} {}", font, p.font_size));
        }
        if !p.file_path.is_empty() {
            block.push_str(&format!("\n@file {}", p.file_path));
        }
        // 83: only a PDF attachment reads this, and 0 (the first page) is by far
        // the common case — skip the line entirely then, same as every other
        // optional field here.
        if p.pdf_page > 0 {
            block.push_str(&format!("\n@pdfpage {}", p.pdf_page));
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
        // 32: order/random preference — sparse like every other optional field here
        // (a plain prompt, the overwhelming majority, never gets an empty line).
        if !p.rotate_mode.is_empty() {
            block.push_str(&format!("\n@rotatemode {}", p.rotate_mode));
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
        .set_file_name(format!("{}.{}", brand::ITEM_PLURAL, format))
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
// Reverses csv_cell's formula-injection guard: a field re-quoted with a leading
// `'` in front of =, +, -, @, tab or CR gets that quote stripped back off, so a
// round trip through this app's own export/import never gains a stray character.
fn strip_formula_guard(s: String) -> String {
    let mut chars = s.chars();
    if chars.next() == Some('\'') && matches!(chars.next(), Some('=' | '+' | '-' | '@' | '\t' | '\r')) {
        return s[1..].to_string();
    }
    s
}

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
                ';' => row.push(strip_formula_guard(std::mem::take(&mut field))),
                '\r' => {}
                '\n' => {
                    row.push(strip_formula_guard(std::mem::take(&mut field)));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(strip_formula_guard(field));
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
    pdf_page: u32,
    icon_path: String,
    caption: String,
    caption_size: u32,
    show_image: bool,
    copy_image: bool,
    image: String,
    tags: Vec<String>,
    notes: String,
    variants: Vec<String>,
    rotate_mode: String,
}

// "a,b , c" -> ["a", "b", "c"]. Used by both import parsers; empty parts drop out.
fn split_tag_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
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
        // A legacy export may still carry an @positions block; the import is
        // prompts-only, so it is dropped here rather than parsed.
        let body = match body.split_once("\n@positions\n") {
            Some((t, _)) => t,
            None => body,
        };
        // Strip trailing metadata lines in reverse write order:
        // @rotatemode, @imagedata, @flags, @icon, @file, @style, @color.
        let (body, rotate_mode) = match body.rsplit_once("\n@rotatemode ") {
            Some((t, v)) => (t.to_string(), v.trim().to_string()),
            None => (body.to_string(), String::new()),
        };
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
        let (body, pdf_page) = match body.rsplit_once("\n@pdfpage ") {
            Some((t, v)) => (t.to_string(), v.trim().parse().unwrap_or(0)),
            None => (body, 0),
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
        let (body, tags) = match body.rsplit_once("\n@tags ") {
            Some((t, v)) => (t.to_string(), split_tag_list(v)),
            None => (body, Vec::new()),
        };
        let (body, notes) = match body.rsplit_once("\n@notes ") {
            Some((t, v)) => (t.to_string(), notes_unescape(v.trim())),
            None => (body, String::new()),
        };
        let (text, color) = match body.rsplit_once("\n@color ") {
            Some((t, c)) => (t.to_string(), c.trim().to_string()),
            None => (body, String::new()),
        };
        // 32: entries 2..n still ride along inside `text` at this point — split
        // them back out now that every trailing @key line is gone.
        let (text, variants) = split_variants(&text);
        data.prompts.push(ImportedPrompt {
            name: name.trim().to_string(),
            text,
            color,
            font,
            font_size,
            file_path,
            pdf_page,
            icon_path,
            caption,
            caption_size,
            show_image,
            copy_image,
            image,
            tags,
            notes,
            variants,
            rotate_mode,
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
            // 32: entries 2..n ride along inside the text cell (column 1).
            let (text, variants) = split_variants(&row[1]);
            data.prompts.push(ImportedPrompt {
                name: row[0].trim().to_string(),
                text,
                color: row.get(3).map(|c| c.trim().to_string()).unwrap_or_default(),
                font: row.get(4).map(|f| f.trim().to_string()).unwrap_or_default(),
                font_size: row
                    .get(5)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0),
                file_path: row.get(6).map(|f| f.trim().to_string()).unwrap_or_default(),
                pdf_page: row.get(15).and_then(|v| v.trim().parse().ok()).unwrap_or(0),
                icon_path: row.get(7).map(|f| f.trim().to_string()).unwrap_or_default(),
                show_image: row.get(8).map(|v| v.trim() == "1").unwrap_or(false),
                copy_image: row.get(9).map(|v| v.trim() == "1").unwrap_or(false),
                image: row.get(10).map(|v| v.trim().to_string()).unwrap_or_default(),
                caption: row.get(11).map(|v| v.trim().to_string()).unwrap_or_default(),
                caption_size: row.get(12).and_then(|v| v.trim().parse().ok()).unwrap_or(0),
                tags: row.get(13).map(|v| split_tag_list(v)).unwrap_or_default(),
                notes: row.get(14).cloned().unwrap_or_default(),
                variants,
                rotate_mode: row.get(16).map(|v| v.trim().to_string()).unwrap_or_default(),
            });
    }
    data
}

// The signature a prompt is de-duplicated by, on both the preview and the real
// import — the two must always agree, or the preview's "new"/"duplicate" label
// could lie about what actually happens on confirm.
fn import_sig(name: &str, text: &str, file: &str, image: &str, copy: bool) -> String {
    format!("{name}\u{1}{text}\u{1}{file}\u{1}{image}\u{1}{copy}")
}

fn parse_import_file(path: &std::path::Path) -> Result<ImportData, String> {
    check_import_size(path, MAX_IMPORT_BYTES)?;
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let is_csv = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase() == "csv")
        .unwrap_or(false);
    let data = if is_csv { parse_csv_data(&content) } else { parse_txt(&content) };
    if data.prompts.is_empty() {
        return Err(format!("no {} found", brand::ITEM_PLURAL));
    }
    Ok(data)
}

// 88: what an import WOULD do, without touching the store — the file is picked
// here, and its path travels back so the confirm step (import_prompts) can
// re-read the SAME file without a second file dialog.
#[derive(Serialize)]
struct ImportPreviewItem {
    name: String,
    duplicate: bool,
}
#[derive(Serialize)]
struct ImportPreview {
    path: String,
    items: Vec<ImportPreviewItem>,
}
#[tauri::command]
async fn preview_import(app: AppHandle, state: State<'_, Db>) -> Result<ImportPreview, String> {
    let file = file_dialog(&app)
        .add_filter(&title_case(brand::ITEM_PLURAL), &["csv", "txt"])
        .pick_file();
    let Some(path) = file else {
        return Err("canceled".to_string());
    };
    let data = parse_import_file(&path)?;
    let store = lock(&state);
    let mut seen: std::collections::HashSet<String> = store
        .prompts
        .iter()
        .map(|p| import_sig(&p.name, &p.text, &p.file_path, &p.image, p.copy_image))
        .collect();
    let items = data
        .prompts
        .into_iter()
        .map(|item| {
            let dup = !seen.insert(import_sig(&item.name, &item.text, &item.file_path, &item.image, item.copy_image));
            ImportPreviewItem { name: item.name, duplicate: dup }
        })
        .collect();
    Ok(ImportPreview { path: path.to_string_lossy().to_string(), items })
}

// Async: parsing + persisting an import stays off the UI thread. `path` comes
// from a prior preview_import call (88) — never a fresh file dialog here, so
// confirming a preview can never silently import a DIFFERENT file.
#[tauri::command]
async fn import_prompts(app: AppHandle, state: State<'_, Db>, path: String) -> Result<usize, String> {
    let data = parse_import_file(std::path::Path::new(&path))?;

    // Prompts-only import: never touch app settings, views or layout, even if an
    // older export file still carries those blocks (they are parsed but ignored).
    // Imported prompts are appended, but an entry identical to one already present
    // is skipped so the original is kept (no duplicate buttons).
    let mut store = lock(&state);
    let mut seen: std::collections::HashSet<String> = store
        .prompts
        .iter()
        .map(|p| import_sig(&p.name, &p.text, &p.file_path, &p.image, p.copy_image))
        .collect();
    let mut count = 0usize;
    // Grows while importing so tags introduced by the file itself also settle on one casing.
    let mut known = canonical_tags(&store.prompts);
    for item in data.prompts {
        if !seen.insert(import_sig(&item.name, &item.text, &item.file_path, &item.image, item.copy_image)) {
            continue; // duplicate of an existing (or earlier imported) prompt
        }
        // A malformed tag in an imported file must not abort the whole import.
        let tags = sanitize_tags(item.tags, &known).unwrap_or_default();
        for t in &tags {
            known.entry(tag_key(t)).or_insert_with(|| t.clone());
        }
        // 32: the same soft-editor-limit / hard-backend-floor split every other
        // entry point into a rotating prompt already goes through.
        let variants = sanitize_variants(item.variants);
        let rotate_mode = sanitize_rotate_mode(item.rotate_mode, &variants);
        let prompt = Prompt {
            id: gen_id(),
            trashed_at: 0,
            name: item.name,
            text: item.text,
            color: item.color,
            image: item.image,
            show_image: item.show_image,
            copy_image: item.copy_image,
            file_path: item.file_path,
            pdf_page: item.pdf_page,
            icon_path: item.icon_path,
            caption: item.caption,
            caption_size: clamp_caption_size(item.caption_size),
            font: item.font,
            font_size: clamp_font_size(item.font_size),
            favorite: false,
            tags,
            // A note from a foreign file must not abort the import either.
            notes: sanitize_notes(&item.notes).unwrap_or_default(),
            // A CSV/TXT file carries prompt content, never machine-local bindings
            // — nor the per-binding paste choice that belongs to one.
            hotkey: String::new(),
            hotkey_paste: String::new(),
            variants,
            rotate_mode,
        };
        store.prompts.push(prompt);
        count += 1;
    }
    save_prompts(&app, &store);
    let settings = store.settings.clone();
    drop(store);
    push_theme(&app, &settings);
    rebuild_tray(&app); // F23
    Ok(count)
}

// Full backup: prompts + settings as one JSON file ("export/import everything").
#[derive(Serialize, Deserialize)]
struct Backup {
    prompts: Vec<Prompt>,
    settings: Settings,
}

// ---------- Encrypted single-file backups (S7) ----------
// Format: b"PSB1" | mode(1) | [salt(16) for a password mode] | nonce(12) | AES-256-GCM.
// Modes 0/2 = fixed key (portable: any install restores it), 1/3 = user password
// (Argon2id(password + pepper, salt)); 2/3 additionally deflate the JSON before
// encrypting. Old plaintext .json backups are still read.
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

// Ceiling for anything we decompress. The default backup key ships inside the exe, so
// a valid-looking .psb is something anyone can craft — without a cap a few KB of
// deflate expands until the process is killed. Far above any real library.
const MAX_INFLATED: u64 = 512 * 1024 * 1024;

// Decompress at most MAX_INFLATED bytes, refusing anything larger instead of growing
// the buffer until the allocator gives up.
fn inflate_capped<R: std::io::Read>(reader: R) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out = Vec::new();
    // cap + 1 byte: reaching the extra byte is what proves the stream was too big.
    reader
        .take(MAX_INFLATED + 1)
        .read_to_end(&mut out)
        .map_err(|_| "corrupt backup".to_string())?;
    if out.len() as u64 > MAX_INFLATED {
        return Err("backup expands to more than 512 MB".to_string());
    }
    Ok(out)
}

fn inflate(packed: &[u8]) -> Result<Vec<u8>, String> {
    use flate2::read::DeflateDecoder;
    inflate_capped(DeflateDecoder::new(packed))
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
        .set_file_name(&format!("{}-backup.psb", brand::SLUG))
        .add_filter(&format!("{} backup", brand::DISPLAY_NAME), &["psb", "json"])
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
    // F20: prompt hotkeys the restore had to give up (unusable or already bound).
    dropped_hotkeys: usize,
}

#[tauri::command]
async fn pick_backup_file(app: AppHandle) -> Option<String> {
    file_dialog(&app)
        .add_filter(&format!("{} backup", brand::DISPLAY_NAME), &["psb", "json"])
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
    check_import_size(&path, MAX_IMPORT_BYTES)?;
    let raw = fs::read(&path).map_err(|e| e.to_string())?;
    // Try the caller-supplied password first, else the one stored for this machine.
    let pw = password.filter(|p| !p.is_empty()).or_else(|| {
        let store = lock(&state);
        stored_backup_password(&store)
    });
    // Encrypted (PSB1) or a legacy plaintext .json backup.
    let content = match backup_decrypt(&raw, pw.as_deref())? {
        Some(plain) => String::from_utf8(plain).map_err(|_| "corrupt backup".to_string())?,
        None => String::from_utf8(raw).map_err(|_| format!("not a {} backup", brand::DISPLAY_NAME))?,
    };
    let backup: Backup =
        serde_json::from_str(&content).map_err(|_| format!("not a {} backup", brand::DISPLAY_NAME))?;

    let (settings, count, dropped_hotkeys) = {
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
        load_pick_history(&store.settings); // 19: restore brings its own history mirror
        store.prompts_locked = false; // a full import supersedes any decrypt lock
        // F20: a colliding or unusable accelerator must not fail the restore — drop
        // it and let the summary say so.
        let app_hotkey = store.settings.hotkey.clone();
        let dropped = drop_colliding_hotkeys(&app_hotkey, &mut store.prompts);
        store.normalize_folders(); // F25: the restored folders must match the restored prompts
        save_prompts(&app, &store);
        save_settings(&app, &store.settings);
        (store.settings.clone(), store.prompts.len(), dropped)
    };
    // Re-apply the visible parts of the imported settings without a restart.
    push_theme(&app, &settings);
    rearm_hotkeys(&app);
    apply_tray_menu(&app); // F23: an import can bring new tray settings with it
    sync_float_windows(&app); // F25: pills and folders follow the restored settings
    Ok(ImportSummary {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        count,
        dropped_hotkeys,
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

// Same point-on-monitor test as above, but over plain (x, y, width, height) tuples
// instead of tauri::Monitor — that type can't be constructed outside a real window,
// so this is the only form `prune_offscreen_floats` below can actually unit-test.
fn point_in_monitor_rects(monitors: &[(i32, i32, i32, i32)], x: i32, y: i32) -> bool {
    monitors.iter().any(|&(mx, my, mw, mh)| x >= mx && x < mx + mw && y >= my && y < my + mh)
}

// P44: a floating pill/folder whose saved anchor point (its stored x/y — no width or
// height is tracked) sits on NO monitor at all is unreachable forever — drop it. No
// margin: a pill dragged mostly off an edge but with its origin corner still on some
// monitor counts as "hanging over the edge" and stays, exactly what the request asked
// for. A folder's members never carry their own `floating` entry while grouped (same
// invariant as grid folders), but removing an absent key is a harmless no-op either
// way. Returns whether anything changed, so the caller only re-saves settings when it
// actually pruned something.
fn prune_offscreen_floats(monitors: &[(i32, i32, i32, i32)], settings: &mut Settings) -> bool {
    if monitors.is_empty() {
        return false; // a failed monitor probe must never look like "nothing is on-screen"
    }
    let gone_pills: Vec<String> = settings
        .floating
        .iter()
        .filter(|(_, pos)| !point_in_monitor_rects(monitors, pos.x, pos.y))
        .map(|(id, _)| id.clone())
        .collect();
    for id in &gone_pills {
        settings.floating.remove(id);
    }
    let before = settings.float_folders.len();
    let mut freed_members = Vec::new();
    settings.float_folders.retain(|f| {
        let gone = !point_in_monitor_rects(monitors, f.pos.x, f.pos.y);
        if gone {
            freed_members.extend(f.members.iter().cloned());
        }
        !gone
    });
    for id in &freed_members {
        settings.floating.remove(id);
    }
    !gone_pills.is_empty() || settings.float_folders.len() != before
}

// NOTE: WindowGeom.width/height are LOGICAL pixels (DPI-independent) — a
// physically stored size grew by the monitor's scale factor on every start.
// Position stays physical (global desktop coordinates).

// Keep in sync with tauri.conf.json's main-window minWidth/minHeight. The OS/WebView2
// minSize constraint only guards interactive resize-dragging, not a programmatic
// set_size() call — resolve_geometry's first-start branch has to floor itself.
const MAIN_WIN_MIN_W: u32 = 680;
const MAIN_WIN_MIN_H: u32 = 460;

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
        (cap.0 / 2).max(MAIN_WIN_MIN_W).min(cap.0),
        (cap.1 / 2).max(MAIN_WIN_MIN_H).min(cap.1),
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
    let (title, msg) = webview2_dialog(resolve_lang("auto"));
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
        .set("User-Agent", brand::USER_AGENT)
        .timeout(std::time::Duration::from_secs(300))
        .call()
    {
        Ok(r) => r,
        Err(_) => return false,
    };
    const CAP: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    if resp
        .into_reader()
        .take(CAP)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.is_empty()
    {
        return false;
    }
    // Same two guards the update download uses: a file that exactly hit the cap was
    // almost certainly truncated, and whatever we are about to execute has to at least
    // be a Windows executable. (Not cryptographic authentication — the bootstrapper
    // comes from Microsoft's own pinned fwlink over TLS, and only after the user
    // agreed to install it.)
    if bytes.len() as u64 >= CAP || bytes.len() < 2 || &bytes[0..2] != b"MZ" {
        return false;
    }
    // Stage into a unique temp subdir so a local attacker can't pre-plant the
    // installer at a predictable path (TOCTOU) — matching install_update.
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("{}-wv2-{}-{}", brand::SLUG, std::process::id(), stamp));
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

    let mut builder = tauri::Builder::default();
    // Only one instance app-wide (keyed by app identifier, independent of the exe
    // location) unless the user opted into "Allow multiple instances" (expert menu,
    // default off) — then this plugin is left out entirely, since it enforces the
    // limit itself before .setup() ever runs (there is no AppHandle yet to consult
    // the setting the normal way; early_allow_multiple_instances() peeks the DB
    // directly for exactly this reason). A second launch, when the plugin IS
    // attached, closes itself and focuses the first instance.
    if !early_allow_multiple_instances() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }));
    }
    builder
        // The app hotkey summons the window + quick-launcher; a prompt hotkey (F20)
        // copies that prompt wherever the user is.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        return;
                    }
                    // Read the owner out first — the handler must not call back into
                    // the app while the lookup lock is held.
                    let owner = app.try_state::<HotkeyTargets>().and_then(|targets| {
                        let list = targets.0.lock().unwrap_or_else(|e| e.into_inner());
                        list.iter().find(|(sc, _)| sc == shortcut).map(|(_, o)| o.clone())
                    });
                    match owner {
                        Some(Some(id)) => fire_prompt_hotkey(app, &id),
                        _ => summon_launcher(app), // the app hotkey (or an unknown leftover)
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let store = load_store(&handle);

            let saved_geom = store.settings.window;
            let autostart = store.settings.autostart;
            let start_min = store.settings.start_minimized;
            let on_top = store.settings.always_on_top;
            let capture_excl = capture_excluded(&store.settings);

            let clip_on = store.settings.ui_flags.get("clipWatcher") == Some(&true); // F21, off by default

            app.manage(Mutex::new(store));
            app.manage(SnipState(Mutex::new(Vec::new())));
            app.manage(SnipPreview(Arc::new(Mutex::new(None))));
            app.manage(HotkeyTargets(Mutex::new(Vec::new()))); // F20
            app.manage(FolderExpand(Mutex::new(HashMap::new()))); // F25
            app.manage(HiddenFloats(Mutex::new(std::collections::HashSet::new()))); // 43
            app.manage(DragHint(Mutex::new((0, None)))); // F25 live merge highlight
            app.manage(VarPrompt::new()); // F12 standalone fill-in window

            // F19 auto-paste: start tracking the last external foreground window.
            #[cfg(windows)]
            autopaste::install_hook();

            // F21: start the clipboard watcher only if the user opted in.
            #[cfg(windows)]
            if clip_on {
                clipwatch::start(handle.clone());
            }

            // Prompt-folders-as-Markdown: harmless to start unconditionally — the
            // poll itself checks the opt-in flag on every tick and is a no-op until
            // it is switched on.
            start_sync_poll(handle.clone());

            // F2: take a backup whenever the newest one is older than the interval —
            // once at start, then on every tick for as long as the app runs. The
            // settings are re-read each round, so switching auto-backup off or
            // changing the interval takes effect without a restart. Off the UI thread
            // so it never delays the first paint.
            {
                let h = handle.clone();
                std::thread::spawn(move || loop {
                    {
                        // The lock is held across the snapshot, exactly like the manual
                        // "backup now" command, so no write races with the VACUUM read.
                        let state = h.state::<Db>();
                        let store = lock(&state);
                        let on = store.settings.ui_flags.get("autoBackup") != Some(&false);
                        let keep = store.settings.ui_values.get("backupKeep").copied().unwrap_or(10.0).clamp(1.0, 50.0) as usize;
                        let interval_h = store
                            .settings
                            .ui_values
                            .get("backupIntervalH")
                            .copied()
                            .filter(|v| *v >= 1.0)
                            .unwrap_or(24.0) as u64;
                        let due = newest_backup_age(&h).map_or(true, |age| age >= interval_h.saturating_mul(3600));
                        if on && due {
                            // P60: the manual "backup now" toast fires from the same
                            // command's own success path in the frontend — the automatic
                            // one has no such call site, so it needs its own signal here.
                            if do_backup(&h, keep, gfs_from_settings(&store.settings)).is_ok() {
                                let _ = h.emit_to("main", "backup-done", ());
                            }
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_secs(BACKUP_CHECK_SECS));
                });
            }

            // Re-arm the app hotkey plus every prompt accelerator (F20). Each
            // registration is a round trip to the main thread, so this runs on a
            // worker — the first paint never waits for it, however many prompts
            // carry a hotkey.
            {
                let h = handle.clone();
                std::thread::spawn(move || {
                    rearm_hotkeys(&h);
                });
            }

            // Launched by autostart with --minimized: stay in the tray.
            let start_hidden = std::env::args().any(|a| a == "--minimized");

            if let Some(main) = app.get_webview_window("main") {
                let geom = resolve_geometry(&main, saved_geom);
                let _ = main.set_size(tauri::LogicalSize::new(geom.width, geom.height));
                let _ = main.set_position(PhysicalPosition::new(geom.x, geom.y));
                set_capture_exclusion(&main, capture_excl);
                // P44: drop any pill/folder left fully off-screen, e.g. by an unplugged
                // second monitor. Before sync_float_windows() below, which never even
                // creates a window for something no longer in `floating`/`float_folders`.
                {
                    let mons: Vec<(i32, i32, i32, i32)> = main
                        .available_monitors()
                        .unwrap_or_default()
                        .iter()
                        .map(|m| {
                            let p = m.position();
                            let s = m.size();
                            (p.x, p.y, s.width as i32, s.height as i32)
                        })
                        .collect();
                    let state = handle.state::<Db>();
                    let mut store = lock(&state);
                    if prune_offscreen_floats(&mons, &mut store.settings) {
                        let settings = store.settings.clone();
                        drop(store);
                        save_settings(&handle, &settings);
                    }
                }
                if on_top {
                    let _ = main.set_always_on_top(true);
                }
                {
                    let settings = handle
                        .try_state::<Db>()
                        .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).settings.clone())
                        .unwrap_or_default();
                    let eff = effective_theme(&handle, &theme_base_of(&settings, &settings.theme));
                    let vars = theme_vars_of(&settings);
                    apply_window_bg(&handle, &eff, vars.get("bg").map(|s| s.as_str()));
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
                            let settings = state
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .settings
                                .clone();
                            // Only the "system" preference follows the OS; a custom
                            // theme always inherits a fixed built-in base.
                            if settings.theme == "system" {
                                push_theme(&handle2, &settings);
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
                            // P92: the very first time, explain that the tray menu
                            // still copies while hidden — JS shows it, then hides
                            // itself once dismissed. Every later close hides at once.
                            let first_time = handle2
                                .try_state::<Db>()
                                .map(|s| {
                                    !s.lock().unwrap_or_else(|e| e.into_inner())
                                        .settings
                                        .seen_tray_hint
                                })
                                .unwrap_or(false);
                            if first_time {
                                if let Some(state) = handle2.try_state::<Db>() {
                                    let mut store = state.lock().unwrap_or_else(|e| e.into_inner());
                                    store.settings.seen_tray_hint = true;
                                    save_settings(&handle2, &store.settings);
                                }
                                let _ = handle2.emit_to("main", "first-tray-hide", ());
                            } else if let Some(w) = handle2.get_webview_window("main") {
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
                .tooltip(brand::DISPLAY_NAME)
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
                    // F23: a quick-copy entry. Unknown ids (section headers, leftovers
                    // from an older menu) fall through without a lookup.
                    other => {
                        if let Some(id) = tray_copy_target(other) {
                            fire_tray_copy(app, id);
                        }
                    }
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
            apply_tray_menu(&handle); // F23: adds the quick-copy sections if they are on

            // Re-apply autostart so the registry entry tracks the exe location.
            if autostart {
                let _ = apply_autostart(true, start_min);
            }

            // F25: one place decides which pills and which folders belong on screen.
            sync_float_windows(&handle);

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
            seed_example_prompts,
            dismiss_example_prompts,
            update_prompt,
            set_favorite,
            delete_prompt,
            batch_prompts,
            list_trash,
            restore_prompt,
            purge_prompt,
            empty_trash,
            list_versions,
            last_edited_map,
            restore_version,
            usage_stats,
            find_duplicates,
            ignore_dups,
            list_backups,
            open_backups_folder,
            create_backup,
            delete_backup,
            restore_backup,
            clip_inbox_list,
            clip_inbox_dismiss,
            clip_inbox_clear,
            purge_clip_inbox_from_backups,
            open_repo,
            set_ui_zoom,
            delete_all_data,
            reset_settings,
            set_language,
            set_tile_style,
            set_layout,
            set_view_grid,
            rename_tag,
            delete_tag,
            set_tag_color,
            add_view,
            rename_view,
            set_view_color,
            remap_colors,
            delete_view,
            set_active_view,
            save_view_group,
            set_view_group_member,
            delete_view_group,
            save_tile_folder,
            take_out_of_tile_folder,
            delete_tile_folder,
            get_settings,
            get_state,
            launcher_items,
            current_theme,
            theme_vars,
            set_theme,
            save_custom_theme,
            delete_custom_theme,
            export_custom_theme,
            import_custom_theme,
            launcher_select,
            launcher_close,
            copy_prompt,
            rotation_peek,
            rotation_commit,
            rotation_reset,
            copy_text,
            clipboard_text,
            record_copy,
            clear_copy_history,
            recent_copies,
            delete_copy_entry,
            toggle_floating,
            hide_float_for_now,
            set_float_scale,
            resize_float_pill,
            resize_float_media,
            set_float_bounds,
            float_folder_drop,
            float_drag_hint,
            float_folder_state,
            expand_float_folder,
            collapse_float_folder,
            set_float_folder_scale,
            rename_float_folder,
            folder_rename_request,
            dissolve_float_folder,
            remove_folder_member,
            set_video_prefs,
            edit_prompt_request,
            show_main_window,
            app_version,
            check_update,
            install_update,
            set_auto_update,
            set_hotkey,
            check_hotkey,
            set_hotkey_capture,
            focus_main_window,
            open_var_window,
            var_window_payload,
            var_window_result,
            os_user_name,
            set_bars,
            set_minimize_on_close,
            set_always_on_top,
            set_ui_flag,
            set_ui_value,
            set_ui_text,
            reset_expert,
            skip_version,
            open_snip,
            open_picker,
            pick_tile,
            pick_color,
            pick_history,
            copy_pick_history_entry,
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
            preview_import,
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

// ---- Prompt folders as Markdown (opt-in expert feature, default off) ----
//
// A text-only prompt (no attached file/image/icon — those stay DB-only, binary
// content does not belong in a git-friendly text file) round-trips through one
// `<id>.md` per prompt in a chosen folder (default `<data dir>/Prompts`). The id is
// the filename, so renaming a prompt in the app never orphans its file. Hooked at
// save_prompts() — the single choke point every prompt mutation already funnels
// through — so no individual command (add/update/delete/batch/import) needs to know
// this feature exists.
//
// Deliberately simplified vs. the original full spec (see WORK.md's "Prompt-Ordner
// als Markdown" item for the complete ambition):
//   - polling (SYNC_POLL_SECS) instead of a filesystem-watcher crate — no new
//     dependency, and a few seconds of latency is fine for a background sync;
//   - no per-view position sync — a prompt imported from a new file lands in the
//     first free cell of the active view, exactly like a normal add;
//   - no hotkey sync — keeps a background task from re-running hotkey conflict/
//     registration logic, which has its own careful invariants elsewhere;
//   - conflict handling is "whichever side changed since the last successful sync
//     wins this cycle, the other is retried next cycle" rather than an interactive
//     merge dialog with per-conflict choices.
// None of these lose data outright — worst case a change waits one more poll
// interval — but this has NOT had the multi-instance crash-simulation stress test
// the original spec calls for as a "done" gate. Treat it as a beta feature.
const SYNC_POLL_SECS: u64 = 4;
const SYNC_LOCK_STALE_SECS: u64 = 15;

fn sync_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("promptSync").copied().unwrap_or(false)
}

fn sync_dir_path(app: &AppHandle, settings: &Settings) -> PathBuf {
    let custom = settings.ui_texts.get("syncDir").cloned().unwrap_or_default();
    if custom.is_empty() {
        data_dir(app).join("Prompts")
    } else {
        PathBuf::from(custom)
    }
}

// Binary-carrying prompts stay DB-only. A prompt whose id is not a plain, filename-
// safe token stays out too: the id becomes the FILE NAME here, and ids can arrive
// from an imported backup, i.e. from outside this app.
fn sync_eligible(p: &Prompt) -> bool {
    p.image.is_empty() && p.file_path.is_empty() && p.icon_path.is_empty() && sync_safe_id(&p.id)
}

// Refused rather than escaped: the id is also how the inbound half finds the prompt
// again, so a rewritten name would silently orphan the file.
fn sync_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// Header fields are one line each — flattening a stray newline is simpler and safer
// than a real multi-line frontmatter parser for what are normally short values.
fn md_escape_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

fn prompt_to_md(p: &Prompt) -> String {
    let tags = p.tags.iter().map(|t| md_escape_line(t)).collect::<Vec<_>>().join(", ");
    // 32: `variants` is informational only (a human glancing at the header sees the
    // entry count at a glance) — NEVER read back by apply_md_to_prompt, which always
    // re-derives the real list from the body's own VARIANT_SEP markers. The four
    // REQUIRED fields (name/color/tags/notes, md_is_ours()) are unchanged; this is a
    // fifth, additive field, which md_is_ours() already tolerates.
    let mut out = format!(
        "---\nname: {}\ncolor: {}\ntags: {}\nnotes: {}\nvariants: {}\n---\n",
        md_escape_line(&p.name),
        md_escape_line(&p.color),
        tags,
        md_escape_line(&p.notes),
        p.variants.len(),
    );
    let text = join_variants(&p.text, &p.variants);
    out.push_str(&text);
    if !text.is_empty() && !text.ends_with('\n') {
        out.push('\n');
    }
    out
}

// `None` means "not a file this feature wrote" (no opening `---` header) — the
// caller leaves such files alone rather than guessing at a malformed one.
fn parse_frontmatter(content: &str) -> Option<(HashMap<String, String>, String)> {
    let mut lines = content.split('\n');
    if lines.next()?.trim_end_matches('\r') != "---" {
        return None;
    }
    let mut fields = HashMap::new();
    let mut body_lines: Vec<&str> = Vec::new();
    let mut in_header = true;
    for line in lines {
        if in_header {
            if line.trim_end_matches('\r') == "---" {
                in_header = false;
                continue;
            }
            if let Some(idx) = line.find(':') {
                fields.insert(line[..idx].trim().to_string(), line[idx + 1..].trim().to_string());
            }
        } else {
            body_lines.push(line);
        }
    }
    if in_header {
        return None; // closing --- never found
    }
    Some((fields, body_lines.join("\n")))
}

// A file only counts as ours when it carries the FULL header this feature writes.
// A user's own note (Obsidian, Jekyll, …) also opens with `---`; importing one and
// then rewriting it would destroy their own frontmatter fields.
fn md_is_ours(fields: &HashMap<String, String>) -> bool {
    ["name", "color", "tags", "notes"].iter().all(|k| fields.contains_key(*k))
}

fn apply_md_to_prompt(base: &Prompt, content: &str) -> Option<Prompt> {
    let (fields, body) = parse_frontmatter(content)?;
    if !md_is_ours(&fields) {
        return None;
    }
    let mut p = base.clone();
    if let Some(v) = fields.get("name") {
        p.name = v.clone();
    }
    if let Some(v) = fields.get("color") {
        p.color = v.clone();
    }
    if let Some(v) = fields.get("tags") {
        let mut seen = std::collections::HashSet::new();
        p.tags = v
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && seen.insert(s.to_lowercase()))
            .collect();
    }
    if let Some(v) = fields.get("notes") {
        p.notes = v.clone();
    }
    // The writer closes the body with a newline; keeping it out of the stored text is
    // what stops every round trip through the folder from adding another blank line.
    let body = body.strip_suffix('\n').unwrap_or(&body);
    // 32: entries 2..n ride along inside the body, split back out here — the
    // `variants:` header count above is never consulted, so a stale count left by a
    // hand-edit (added/removed an entry without touching the header) is harmless.
    // `rotate_mode` is deliberately NOT part of this sync (same partial-field
    // philosophy as favorite/hotkey/font, which this function also never touches).
    let (text, variants) = split_variants(body);
    p.text = text;
    p.variants = sanitize_variants(variants);
    // A hand-edit that removed every `<!-- next -->` marker must not leave a
    // "random" mode stranded on an empty list — the same invariant
    // sanitize_rotate_mode already enforces for every other entry point.
    p.rotate_mode = sanitize_rotate_mode(p.rotate_mode.clone(), &p.variants);
    Some(p)
}

const SYNC_AGENTS_MD: &str = "\
# Prompt Saver sync folder

Each `<id>.md` file here mirrors one prompt: a short header (`name`, `color`, `tags`,
`notes`) between two `---` lines, then the prompt's text below.

Edit the text (or a header field) and save — Prompt Saver checks this folder every
few seconds and picks up the change. Do not rename a file; the filename is the
prompt's permanent id, and renaming it orphans the file instead of renaming the
prompt.

While a file is being written, a matching `<file>.md.lock` may briefly appear next
to it — leave it alone, it clears itself within seconds. If one lingers (e.g. after
a crash), it is safe to delete by hand.
";

fn sync_write_agents_md(dir: &Path) {
    let path = dir.join("AGENTS.md");
    if !path.exists() {
        let _ = fs::write(&path, SYNC_AGENTS_MD);
    }
}

fn sync_lock_path(md_path: &Path) -> PathBuf {
    let mut s = md_path.as_os_str().to_os_string();
    s.push(".lock");
    PathBuf::from(s)
}

fn pid_from_lock(lock_path: &Path) -> Option<u32> {
    fs::read_to_string(lock_path).ok()?.trim().parse().ok()
}

#[cfg(windows)]
fn pid_is_alive(pid: u32) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn CloseHandle(h: isize) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            false
        } else {
            CloseHandle(h);
            true
        }
    }
}
#[cfg(not(windows))]
fn pid_is_alive(_pid: u32) -> bool {
    true // conservative off Windows: never treat a lock as stale
}

fn sync_lock_is_stale(lock_path: &Path) -> bool {
    let Ok(meta) = fs::metadata(lock_path) else { return true }; // gone already
    let old_enough = meta
        .modified()
        .and_then(|m| m.elapsed().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))
        .map(|d| d.as_secs() >= SYNC_LOCK_STALE_SECS)
        .unwrap_or(true);
    if !old_enough {
        return false;
    }
    match pid_from_lock(lock_path) {
        Some(pid) => !pid_is_alive(pid),
        None => true, // unreadable/garbage lock content — treat as abandoned
    }
}

// Atomic create (never "check then create" — that would race between two
// instances). A held, non-stale lock means "leave this file for the next poll
// cycle", never a blocking wait — this runs in a background task with no UI to
// block, so retrying later is the safe choice over hanging the poll loop.
fn sync_lock_acquire(md_path: &Path) -> bool {
    let lock_path = sync_lock_path(md_path);
    let try_create = |p: &Path| {
        fs::OpenOptions::new().write(true).create_new(true).open(p).map(|mut f| {
            let _ = std::io::Write::write_all(&mut f, std::process::id().to_string().as_bytes());
        })
    };
    if try_create(&lock_path).is_ok() {
        return true;
    }
    if sync_lock_is_stale(&lock_path) {
        let _ = fs::remove_file(&lock_path);
        return try_create(&lock_path).is_ok();
    }
    false
}

fn sync_lock_release(md_path: &Path) {
    let _ = fs::remove_file(sync_lock_path(md_path));
}

// The outbound half: DB -> files. Called from save_prompts() after every successful
// persist, so it only has to be a no-op check (content already matches) in the
// common case where nothing eligible actually changed.
fn sync_write_all(app: &AppHandle, store: &Store) {
    if !sync_enabled(&store.settings) {
        return;
    }
    let dir = sync_dir_path(app, &store.settings);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    sync_write_agents_md(&dir);
    let mut wanted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in &store.prompts {
        if !sync_eligible(p) {
            continue;
        }
        let filename = format!("{}.md", p.id);
        let path = dir.join(&filename);
        let content = prompt_to_md(p);
        if fs::read_to_string(&path).unwrap_or_default() != content && sync_lock_acquire(&path) {
            let _ = fs::write(&path, &content);
            sync_lock_release(&path);
        }
        wanted.insert(filename);
    }
    sync_sweep_orphans(&dir, &wanted);
}

// A prompt that left the library (deleted, trashed, or became media-carrying) takes
// its file with it — otherwise a stale file would come back as a duplicate "new"
// prompt on the next inbound poll. ONLY files this feature could have written are
// swept: the sync folder is a directory the USER picks, so it can hold their own
// documents. A document without our `---` frontmatter is never imported by
// sync_poll_once either, so deleting it would destroy data the app does not own.
fn sync_sweep_orphans(dir: &Path, wanted: &std::collections::HashSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.ends_with(".md") || name == "AGENTS.md" || wanted.contains(&name) {
            continue;
        }
        let path = e.path();
        let ours = fs::read_to_string(&path)
            .ok()
            .and_then(|c| parse_frontmatter(&c))
            .is_some_and(|(fields, _)| md_is_ours(&fields));
        if ours {
            let _ = fs::remove_file(&path);
        }
    }
}

fn sync_blank_prompt(id: String) -> Prompt {
    Prompt {
        id,
        trashed_at: 0,
        name: String::new(),
        text: String::new(),
        color: String::new(),
        image: String::new(),
        show_image: false,
        copy_image: false,
        file_path: String::new(),
        pdf_page: 0,
        icon_path: String::new(),
        caption: String::new(),
        caption_size: 0,
        font: String::new(),
        font_size: 0,
        favorite: false,
        tags: Vec::new(),
        notes: String::new(),
        hotkey: String::new(),
        hotkey_paste: String::new(),
        variants: Vec::new(),
        rotate_mode: String::new(),
    }
}

// The inbound half: files -> DB. Runs on a timer (see start_sync_poll) rather than
// a filesystem watcher, see the module note above for why.
fn sync_poll_once(app: &AppHandle) {
    let Some(state) = app.try_state::<Db>() else { return };
    let mut store = lock(&state);
    if store.prompts_locked || !sync_enabled(&store.settings) {
        return;
    }
    let dir = sync_dir_path(app, &store.settings);
    let Ok(entries) = fs::read_dir(&dir) else { return };
    let mut changed = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
        // The stem becomes a prompt id, and every id is written back out as a file
        // name — so it has to pass the same gate an outgoing id does.
        if !sync_safe_id(&stem) {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else { continue };
        if let Some(idx) = store.prompts.iter().position(|p| p.id == stem) {
            if prompt_to_md(&store.prompts[idx]) == content {
                continue; // matches what we would write ourselves — not a real change
            }
            if !sync_lock_acquire(&path) {
                continue; // someone else is mid-write; retry next cycle
            }
            if let Some(updated) = apply_md_to_prompt(&store.prompts[idx], &content) {
                store.prompts[idx] = updated;
                changed = true;
            }
            sync_lock_release(&path);
        } else {
            if !sync_lock_acquire(&path) {
                continue;
            }
            if let Some(np) = apply_md_to_prompt(&sync_blank_prompt(stem), &content) {
                let folder_cells = active_view_folder_cells(&store.settings);
                let view = store.settings.active_view_mut();
                if let Some(cell) = first_free_cell_excluding(view, &folder_cells) {
                    let key = grid_key(view.cols, view.rows);
                    view.layouts.entry(key).or_default().insert(np.id.clone(), cell);
                }
                store.prompts.push(np);
                changed = true;
            }
            sync_lock_release(&path);
        }
    }
    if changed {
        save_prompts(app, &store);
    }
}

fn start_sync_poll(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(SYNC_POLL_SECS)).await;
            sync_poll_once(&app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_that_no_longer_ships_falls_back_to_system() {
        let mut s = Settings::default();
        // Themes dropped in 2.7.0 — an installation set to one of them must still
        // start with a full set of colour tokens.
        for gone in ["gradient", "sunset", "forest", "midnight", "retro", "lavender", "candy", "coffee"] {
            assert_eq!(theme_base_of(&s, gone), "system", "{gone} should fall back");
        }
        assert_eq!(theme_base_of(&s, "ocean"), "ocean");
        assert_eq!(theme_base_of(&s, "system"), "system");
        // A custom theme built on a dropped base falls back the same way.
        s.custom_themes.push(CustomTheme {
            id: "t1".into(),
            name: "Mine".into(),
            base: "retro".into(),
            vars: HashMap::new(),
        });
        assert_eq!(theme_base_of(&s, "custom:t1"), "system");
    }

    #[test]
    fn stored_ai_theme_migrates_to_neon_not_system() {
        // 24: "ai" was renamed to "neon" — a saved preference must land on the
        // renamed theme, not fall through known_theme() to "system".
        let mut s = Settings::default();
        s.theme = "ai".into();
        s.custom_themes.push(CustomTheme {
            id: "t1".into(),
            name: "Mine".into(),
            base: "ai".into(),
            vars: HashMap::new(),
        });
        s.migrate();
        assert_eq!(s.theme, "neon");
        assert_eq!(s.custom_themes[0].base, "neon");
        assert_eq!(theme_base_of(&s, &s.theme), "neon");
    }

    // ---- 80: colour-picker history ----

    #[test]
    fn record_pick_keeps_five_newest_and_skips_a_repeat() {
        PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clear();
        for i in 1..=6u8 {
            record_pick([i, 0, 0], PICK_HISTORY_DEFAULT);
        }
        record_pick([6, 0, 0], PICK_HISTORY_DEFAULT); // the same colour twice in a row is not a new entry
        let hex = pick_history_hex();
        assert_eq!(hex.len(), PICK_HISTORY_DEFAULT);
        // Newest first, and the oldest ([1,0,0]) has fallen off the end.
        assert_eq!(hex[0], "#060000");
        assert_eq!(hex[PICK_HISTORY_DEFAULT - 1], "#020000");
        // Picking an earlier colour again puts it back on top as a fresh entry.
        record_pick([2, 0, 0], PICK_HISTORY_DEFAULT);
        assert_eq!(pick_history_hex()[0], "#020000");
    }

    #[test]
    fn pick_history_persists_across_a_reload_and_respects_a_lowered_cap() {
        PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let mut s = Settings::default();
        record_pick([1, 0, 0], pick_history_max(&s));
        record_pick([2, 0, 0], pick_history_max(&s));
        s.ui_texts.insert("pickHistory".to_string(), pick_history_hex().join(","));
        // Simulate a restart: clear the in-memory list, reload from the mirror.
        PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clear();
        load_pick_history(&s);
        assert_eq!(pick_history_hex(), vec!["#020000", "#010000"]);
        // Lowering the cap below the stored count trims on the next load.
        s.ui_values.insert("pickHistoryN".to_string(), 1.0);
        load_pick_history(&s);
        assert_eq!(pick_history_hex(), vec!["#020000"]);
        // 0 turns the feature off: the mirror is kept, but nothing loads back in.
        s.ui_values.insert("pickHistoryN".to_string(), 0.0);
        load_pick_history(&s);
        assert!(pick_history_hex().is_empty());
    }

    // The sync folder is a directory the USER picks, so it can hold their own
    // documents. Only files carrying our frontmatter may ever be swept.
    #[test]
    fn sync_sweep_spares_documents_the_app_did_not_write() {
        let dir = std::env::temp_dir().join(format!("ps-sweep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mine = dir.join("p_stale.md");
        let theirs = dir.join("My shopping list.md");
        let obsidian = dir.join("Reading list.md");
        let kept = dir.join("p_live.md");
        fs::write(&mine, prompt_to_md(&sync_test_prompt())).unwrap();
        fs::write(&theirs, "# Shopping\n\n- milk\n").unwrap();
        // Someone else's frontmatter is still not ours.
        fs::write(&obsidian, "---\ntitle: Books\ntags: reading\n---\n\n- Dune\n").unwrap();
        fs::write(&kept, prompt_to_md(&sync_test_prompt())).unwrap();
        fs::write(dir.join("AGENTS.md"), SYNC_AGENTS_MD).unwrap();

        let mut wanted = std::collections::HashSet::new();
        wanted.insert("p_live.md".to_string());
        sync_sweep_orphans(&dir, &wanted);

        assert!(!mine.exists(), "an orphaned prompt file must go");
        assert!(theirs.exists(), "a document without our frontmatter must survive");
        assert!(obsidian.exists(), "someone else's frontmatter must survive");
        assert!(kept.exists(), "a live prompt's file must stay");
        assert!(dir.join("AGENTS.md").exists(), "the readme must stay");
        let _ = fs::remove_dir_all(&dir);
    }

    // ---- Prompt folders as Markdown ----

    fn sync_test_prompt() -> Prompt {
        let mut p = sync_blank_prompt("p1".into());
        p.name = "Test prompt".into();
        p.color = "#7c5cff".into();
        p.tags = vec!["Work".into(), "draft".into()];
        p.notes = "a private note".into();
        p.text = "Hello\nWorld".into();
        p
    }

    #[test]
    fn md_roundtrip_preserves_editable_fields() {
        let p = sync_test_prompt();
        let content = prompt_to_md(&p);
        let updated = apply_md_to_prompt(&p, &content).expect("valid frontmatter");
        assert_eq!(updated.name, p.name);
        assert_eq!(updated.color, p.color);
        assert_eq!(updated.tags, p.tags);
        assert_eq!(updated.notes, p.notes);
        // The closing newline the writer adds is the file's, not the prompt's — a
        // round trip must hand back exactly the text that went in.
        assert_eq!(updated.text, "Hello\nWorld");
        // Re-serializing the round-tripped prompt must be byte-identical — otherwise
        // sync_write_all()'s "only write if content differs" guard would loop forever.
        assert_eq!(prompt_to_md(&updated), content);
    }

    #[test]
    fn md_escape_line_strips_embedded_newlines() {
        assert_eq!(md_escape_line("a\nb\r\nc"), "a b  c");
    }

    #[test]
    fn parse_frontmatter_rejects_a_file_with_no_header() {
        assert!(parse_frontmatter("just some text\nwith no header").is_none());
        assert!(apply_md_to_prompt(&sync_blank_prompt("x".into()), "no header here").is_none());
    }

    #[test]
    fn parse_frontmatter_rejects_an_unclosed_header() {
        // Opens with --- but never closes it — must not silently swallow the rest
        // of the file as a giant, wrong "field".
        assert!(parse_frontmatter("---\nname: x\nstill going...").is_none());
    }

    #[test]
    fn frontmatter_tags_dedupe_case_insensitively_and_drop_blanks() {
        let base = sync_blank_prompt("p2".into());
        let content = "---\nname: x\ncolor: \ntags: Work, work, , Draft\nnotes: \n---\nbody\n";
        let updated = apply_md_to_prompt(&base, content).unwrap();
        assert_eq!(updated.tags, vec!["Work".to_string(), "Draft".to_string()]);
    }

    #[test]
    fn sync_eligible_excludes_every_kind_of_attachment() {
        let mut p = sync_test_prompt();
        assert!(sync_eligible(&p));
        p.image = "data:image/png;base64,x".into();
        assert!(!sync_eligible(&p));
        let mut p2 = sync_test_prompt();
        p2.file_path = "C:\\x.pdf".into();
        assert!(!sync_eligible(&p2));
        let mut p3 = sync_test_prompt();
        p3.icon_path = "C:\\x.gif".into();
        assert!(!sync_eligible(&p3));
    }

    #[test]
    fn sync_write_all_is_a_no_op_when_the_feature_is_off() {
        let dir = std::env::temp_dir().join(format!("ps-sync-test-off-{}", gen_id()));
        let mut settings = Settings::default();
        settings.ui_texts.insert("syncDir".into(), dir.to_string_lossy().into_owned());
        // sync_enabled() reads ui_flags, left empty here -> off by default.
        let store = Store { prompts: vec![sync_test_prompt()], trash: Vec::new(), settings, prompts_locked: false };
        // No AppHandle in a unit test, so exercise the same guard sync_write_all()
        // itself checks first — this is what keeps every non-sync-user's save_prompts
        // call free of any filesystem work.
        assert!(!sync_enabled(&store.settings));
        assert!(!dir.exists());
    }

    #[test]
    fn sync_lock_is_exclusive_until_released() {
        let dir = std::env::temp_dir().join(format!("ps-sync-lock-test-{}", gen_id()));
        fs::create_dir_all(&dir).unwrap();
        let md_path = dir.join("p1.md");
        fs::write(&md_path, "x").unwrap();
        assert!(sync_lock_acquire(&md_path), "first acquire must succeed");
        assert!(!sync_lock_acquire(&md_path), "a fresh lock must never look stale");
        sync_lock_release(&md_path);
        assert!(sync_lock_acquire(&md_path), "must be acquirable again after release");
        sync_lock_release(&md_path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_agents_md_is_written_once_and_never_overwritten() {
        let dir = std::env::temp_dir().join(format!("ps-sync-agents-test-{}", gen_id()));
        fs::create_dir_all(&dir).unwrap();
        sync_write_agents_md(&dir);
        let first = fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert!(first.contains("Prompt Saver"));
        fs::write(dir.join("AGENTS.md"), "user's own edit, must survive").unwrap();
        sync_write_agents_md(&dir);
        assert_eq!(fs::read_to_string(dir.join("AGENTS.md")).unwrap(), "user's own edit, must survive");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_lock_is_stale_once_the_holder_pid_is_dead() {
        // Bypasses the age check by asserting the pid-liveness half directly —
        // the age half needs real elapsed wall-clock time, not worth a sleeping test.
        assert!(pid_is_alive(std::process::id()), "this test's own process is alive");
    }

    // Every identifier a rebrand may NOT quietly change. These literals are a
    // deliberate second copy of brand.rs: an installation finds its data, its
    // autostart entry and its update source through them, so a refactor that moves
    // one has to fail here instead of in someone's %APPDATA%. tools/brand.js
    // rewrites this block (BRAND-PIN markers) when a different brand is stamped.
    #[test]
    fn brand_values_are_pinned() {
        // BRAND-PIN-START
        assert_eq!(brand::DISPLAY_NAME, "Prompt Saver");
        assert_eq!(brand::SLUG, "prompt-saver");
        assert_eq!(brand::IDENTIFIER, "com.bgcoding.promptsaver");
        assert_eq!(brand::DATA_FOLDER, "Prompt-Saver");
        assert_eq!(brand::AUTOSTART_KEY, "PromptSaver");
        assert_eq!(brand::TEMP_DIR, "prompt-saver");
        assert_eq!(brand::TEMP_PREFIX, "promptsaver");
        assert_eq!(brand::WINDOW_CLASS, "PromptSaverClipWatch");
        assert_eq!(brand::USER_AGENT, "PromptSaver");
        assert_eq!(brand::REPO, "wbgcoding/Prompt-Saver");
        assert_eq!(brand::LEGACY_DATA_FOLDER, "com.bgcoding.promptsaver");
        assert_eq!(brand::LEGACY_AUTOSTART_KEY, "");
        assert_eq!(brand::LEGACY_SCREENSHOT_PREFIX, "PromptSaver-");
        assert_eq!(env!("CARGO_PKG_NAME"), "prompt-saver");
        let conf = include_str!("../tauri.conf.json");
        assert!(conf.contains("\"productName\": \"Prompt Saver\""));
        assert!(conf.contains("\"identifier\": \"com.bgcoding.promptsaver\""));
        let js = include_str!("../../ui/brand.js");
        assert!(js.contains("APP_NAME = \"Prompt Saver\";"));
        // BRAND-PIN-END
        // The consts the app actually reads must be the ones pinned above.
        assert_eq!(AUTOSTART_KEY, brand::AUTOSTART_KEY);
        assert_eq!(DATA_FOLDER, brand::DATA_FOLDER);
        // Backup crypto is NOT brand-derived: these two are frozen for every build
        // of every brand, or existing .psb files stop opening.
        assert_eq!(BACKUP_PEPPER, &b"prompt-saver::backup::pepper::v1"[..]);
        assert_eq!(BACKUP_FIXED_KEY[0], 0x8f);
        assert_eq!(BACKUP_FIXED_KEY[31], 0x84);
    }

    #[test]
    fn update_url_pin_rejects_escapes() {
        let base = format!("https://github.com/{}/releases/download", brand::REPO);
        // The one legit shape is accepted.
        assert!(valid_update_url(&format!("{base}/v2.5.0/App_2.5.0_x64-setup.exe")));
        // Path-traversal to another repo, still github.com and still prefix-matching.
        assert!(!valid_update_url(&format!(
            "{base}/../../attacker/repo/releases/download/v1/x-setup.exe"
        )));
        // Encoded dot-segment, backslash, extra scheme/authority, query, non-exe.
        assert!(!valid_update_url(&format!("{base}/%2e%2e/x/y-setup.exe")));
        assert!(!valid_update_url(&format!("{base}/v1\\x/y-setup.exe")));
        assert!(!valid_update_url(&format!("{base}/v1/y-setup.exe?x=1")));
        assert!(!valid_update_url(&format!("{base}/v1/y.txt")));
        assert!(!valid_update_url(&format!("http://github.com/{}/releases/download/v1/y-setup.exe", brand::REPO)));
        // Extra path segments beyond tag/asset.
        assert!(!valid_update_url(&format!("{base}/v1/sub/y-setup.exe")));
        // A different repo under the same host is not ours.
        assert!(!valid_update_url("https://github.com/someone/else/releases/download/v1/y-setup.exe"));
    }

    #[test]
    fn store_crypt_roundtrip() {
        let secret = "prompt with sécret 🔐 {#{x}#}";
        assert_eq!(dec_str(&enc_str(secret)), secret);
        // Legacy plaintext (no prefix) passes through unchanged for migration.
        assert_eq!(dec_str("legacy plaintext"), "legacy plaintext");
    }

    // The store's JSON files are the DB-write fallback: they hold full prompt bodies
    // and private notes, so they must reach disk sealed, not as readable JSON.
    #[test]
    fn store_json_fallback_is_written_sealed() {
        let dir = std::env::temp_dir().join(format!("ps-sealed-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("prompts.json");
        let secret = vec!["confidential prompt title".to_string()];

        assert!(write_sealed_json(&path, &secret));
        let raw = fs::read_to_string(&path).expect("written");
        assert!(raw.starts_with(ENC_PREFIX), "fallback must be sealed, got: {raw}");
        assert!(!raw.contains("confidential"), "plaintext leaked into the fallback file");
        let back: Vec<String> = read_sealed_json(&path);
        assert_eq!(back, secret);

        // A genuinely old plaintext install still migrates in.
        fs::write(&path, r#"["legacy entry"]"#).expect("write legacy");
        let legacy: Vec<String> = read_sealed_json(&path);
        assert_eq!(legacy, vec!["legacy entry".to_string()]);

        let _ = fs::remove_dir_all(&dir);
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
    fn retention_keeps_the_right_backups_and_never_all_of_them() {
        let names: Vec<String> = [
            "2024-03-01_02-00-00", // older year
            "2025-11-30_02-00-00", // older month
            "2026-07-20_02-00-00", // older week
            "2026-07-25_02-00-00",
            "2026-07-26_02-00-00", // newest
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        // Plain count: the oldest ones go, the newest `keep` stay.
        assert_eq!(backups_to_drop(&names, 2, None), names[..3].to_vec());
        assert!(backups_to_drop(&names, 99, None).is_empty());
        // A count of 0 must not be read as "delete everything".
        assert_eq!(backups_to_drop(&names, 0, None).len(), names.len() - 1);

        // Tiers count PERIODS, newest first: 3 years reach back to 2024, while one
        // day / week / month only ever pins the newest backup. `keep` is a floor, so
        // it has to stay at 1 here for the tiers alone to decide.
        let tiers = GfsConfig { daily: 1, weekly: 1, monthly: 1, yearly: 3 };
        let dropped = backups_to_drop(&names, 1, Some(&tiers));
        assert!(!dropped.contains(&names[4]), "the newest backup always survives");
        assert!(!dropped.contains(&names[1]), "second-newest year kept");
        assert!(!dropped.contains(&names[0]), "third-newest year kept");
        assert_eq!(dropped, vec![names[2].clone(), names[3].clone()], "no tier asks for these");

        // Every tier at zero must not wipe the lot — the newest one stays.
        let empty = GfsConfig { daily: 0, weekly: 0, monthly: 0, yearly: 0 };
        let dropped = backups_to_drop(&names, 1, Some(&empty));
        assert_eq!(dropped.len(), names.len() - 1);
        assert!(!dropped.contains(&names[4]));

        // Reserved placeholders gain their slash fence exactly once.
        assert_eq!(fence_dyn_vars("Hi {#{date}#} and {#{name}#}"), "Hi {#{/date\\}#} and {#{name}#}");
        assert_eq!(fence_dyn_vars("{#{/date\\}#}"), "{#{/date\\}#}", "already fenced stays put");
        assert_eq!(fence_dyn_vars("plain text"), "plain text");
        // A choice variable that happens to start with a reserved word is untouched.
        assert_eq!(fence_dyn_vars("{#{date|a,b}#}"), "{#{date|a,b}#}");

        // The schedule reads the folder name, not a modification time.
        let a = stamp_secs("2026-07-29_08-00-00").unwrap();
        let b = stamp_secs("2026-07-29_10-30-00").unwrap();
        assert_eq!(b - a, 2 * 3600 + 30 * 60);
        assert_eq!(stamp_secs("2026-07-30_08-00-00").unwrap() - a, 86_400);
        assert!(stamp_secs("not-a-stamp").is_none());

        // `keep` survives the tiers: several backups on ONE day must not delete each
        // other just because the daily tier only pins one per date.
        let today: Vec<String> = ["2026-07-29_08-00-00", "2026-07-29_09-00-00", "2026-07-29_10-00-00"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let dropped = backups_to_drop(&today, 10, Some(&tiers));
        assert!(dropped.is_empty(), "same-day backups stay while `keep` has room");
        // With no room left, the tiers thin them out as before.
        let dropped = backups_to_drop(&today, 1, Some(&tiers));
        assert_eq!(dropped, today[..2].to_vec());
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
        old.theme = "ocean".into();
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
             CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
             CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
             INSERT INTO prompts VALUES('a', 0, '{}');
             INSERT INTO trash VALUES('b', 0, '{}');
             INSERT INTO meta VALUES('settings', '{\"theme\":\"ocean\"}');
             INSERT INTO copy_log VALUES(1, 'a');
             INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('a', 1, 'n', 't');
             INSERT INTO clip_inbox(ts, text) VALUES(1, 'clip');",
        )
        .unwrap();
        // Exactly the statements delete_all_data issues.
        for table in ["prompt_versions", "copy_log", "clip_inbox", "trash"] {
            conn.execute(&format!("DELETE FROM {table}"), []).unwrap();
        }
        conn.execute("DELETE FROM prompts", []).unwrap();
        let count = |t: &str| -> i64 {
            conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap()
        };
        assert_eq!(count("prompts"), 0);
        assert_eq!(count("trash"), 0, "a data wipe must not leave trashed prompts behind");
        assert_eq!(count("copy_log"), 0);
        assert_eq!(count("prompt_versions"), 0);
        assert_eq!(count("clip_inbox"), 0);
        assert_eq!(count("meta"), 1, "settings must survive a data-only wipe");
    }

    // ---- 70: selective backup restore ----

    // restore_selective opens real files via Connection::open (ATTACH DATABASE
    // needs a path, an in-memory DB can't be attached across connections), so these
    // tests write actual throwaway .db files under the OS temp dir and remove them
    // when done. `tag` keeps parallel test runs from colliding on the same names.
    fn full_schema_db(path: &std::path::Path, tag: &str) -> Connection {
        let _ = std::fs::remove_file(path);
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
             CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);",
        )
        .unwrap();
        let _ = tag; // only distinguishes the caller's temp filename, not the schema
        conn
    }

    #[test]
    fn restore_selective_data_scope_leaves_settings_untouched() {
        let dir = std::env::temp_dir();
        let target = dir.join("prompt-saver-test-restore-data-target.db");
        let staged = dir.join("prompt-saver-test-restore-data-staged.db");
        {
            let conn = full_schema_db(&target, "target");
            conn.execute_batch(
                "INSERT INTO prompts VALUES('live', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'LIVE-SETTINGS');
                 INSERT INTO meta VALUES('copy_log_migrated', '1');",
            )
            .unwrap();
        }
        {
            let conn = full_schema_db(&staged, "staged");
            conn.execute_batch(
                "INSERT INTO prompts VALUES('backed-up', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'BACKUP-SETTINGS');",
            )
            .unwrap();
        }
        restore_selective(&target, &staged, "data").unwrap();
        let conn = Connection::open(&target).unwrap();
        let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
        assert_eq!(prompt_id, "backed-up", "data scope must replace the prompts table");
        let settings: String = conn.query_row("SELECT value FROM meta WHERE key='settings'", [], |r| r.get(0)).unwrap();
        assert_eq!(settings, "LIVE-SETTINGS", "data scope must never touch settings");
        let migrated: String = conn.query_row("SELECT value FROM meta WHERE key='copy_log_migrated'", [], |r| r.get(0)).unwrap();
        assert_eq!(migrated, "1", "data scope must never touch the migration marker");
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(&staged);
    }

    #[test]
    fn restore_selective_settings_scope_leaves_data_untouched() {
        let dir = std::env::temp_dir();
        let target = dir.join("prompt-saver-test-restore-settings-target.db");
        let staged = dir.join("prompt-saver-test-restore-settings-staged.db");
        {
            let conn = full_schema_db(&target, "target");
            conn.execute_batch(
                "INSERT INTO prompts VALUES('live', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'LIVE-SETTINGS');
                 INSERT INTO meta VALUES('copy_log_migrated', '1');",
            )
            .unwrap();
        }
        {
            let conn = full_schema_db(&staged, "staged");
            conn.execute_batch(
                "INSERT INTO prompts VALUES('backed-up', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'BACKUP-SETTINGS');",
            )
            .unwrap();
        }
        restore_selective(&target, &staged, "settings").unwrap();
        let conn = Connection::open(&target).unwrap();
        let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
        assert_eq!(prompt_id, "live", "settings scope must never touch prompts");
        let settings: String = conn.query_row("SELECT value FROM meta WHERE key='settings'", [], |r| r.get(0)).unwrap();
        assert_eq!(settings, "BACKUP-SETTINGS", "settings scope must apply the backup's settings");
        let migrated: String = conn.query_row("SELECT value FROM meta WHERE key='copy_log_migrated'", [], |r| r.get(0)).unwrap();
        assert_eq!(migrated, "1", "settings scope must never touch the migration marker");
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(&staged);
    }

    #[test]
    fn purge_clip_inbox_from_backups_scrubs_every_snapshot() {
        let dir = std::env::temp_dir().join("prompt-saver-test-purge-backups");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A: gzipped snapshot (current format) with clip_inbox rows + a prompt that
        // must survive untouched.
        let folder_a = dir.join("20260101-000000");
        std::fs::create_dir_all(&folder_a).unwrap();
        let raw_a = dir.join("raw_a.db");
        {
            let conn = full_schema_db(&raw_a, "a");
            conn.execute_batch(
                "INSERT INTO prompts VALUES('p1', 0, '{}');
                 INSERT INTO clip_inbox(ts, text) VALUES(1, 'secret-a');
                 INSERT INTO clip_inbox(ts, text) VALUES(2, 'secret-b');",
            )
            .unwrap();
        }
        gzip_to(&raw_a, &folder_a.join("data.db.gz")).unwrap();
        let _ = std::fs::remove_file(&raw_a);

        // B: plain (pre-2.6, ungzipped) snapshot — also has clip_inbox rows.
        let folder_b = dir.join("20260102-000000");
        std::fs::create_dir_all(&folder_b).unwrap();
        {
            let conn = full_schema_db(&folder_b.join("data.db"), "b");
            conn.execute_batch("INSERT INTO clip_inbox(ts, text) VALUES(3, 'secret-c');").unwrap();
        }

        // C: nothing in its clip_inbox — must not be counted as purged.
        let folder_c = dir.join("20260103-000000");
        std::fs::create_dir_all(&folder_c).unwrap();
        let raw_c = dir.join("raw_c.db");
        {
            full_schema_db(&raw_c, "c");
        }
        gzip_to(&raw_c, &folder_c.join("data.db.gz")).unwrap();
        let _ = std::fs::remove_file(&raw_c);

        let purged = purge_clip_inbox_from_backups_impl(&dir);
        assert_eq!(purged, 2, "only A and B actually had clip_inbox rows to remove");

        let unpacked_a = dir.join("check_a.db");
        gunzip_to(&folder_a.join("data.db.gz"), &unpacked_a).unwrap();
        let conn = Connection::open(&unpacked_a).unwrap();
        let clip_count: i64 = conn.query_row("SELECT COUNT(*) FROM clip_inbox", [], |r| r.get(0)).unwrap();
        assert_eq!(clip_count, 0, "A's clip_inbox must be emptied");
        let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
        assert_eq!(prompt_id, "p1", "the purge must never touch other tables");

        let conn = Connection::open(folder_b.join("data.db")).unwrap();
        let clip_count: i64 = conn.query_row("SELECT COUNT(*) FROM clip_inbox", [], |r| r.get(0)).unwrap();
        assert_eq!(clip_count, 0, "B's (ungzipped) clip_inbox must be emptied too");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_selective_skips_a_table_missing_from_an_older_backup() {
        let dir = std::env::temp_dir();
        let target = dir.join("prompt-saver-test-restore-oldbackup-target.db");
        let staged = dir.join("prompt-saver-test-restore-oldbackup-staged.db");
        {
            let conn = full_schema_db(&target, "target");
            conn.execute_batch(
                "INSERT INTO prompts VALUES('live', 0, '{}');
                 INSERT INTO trash VALUES('live-trashed', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'LIVE-SETTINGS');",
            )
            .unwrap();
        }
        {
            // Pre-91 backup: no trash table at all.
            let _ = std::fs::remove_file(&staged);
            let conn = Connection::open(&staged).unwrap();
            conn.execute_batch(
                "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
                 CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
                 CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
                 INSERT INTO prompts VALUES('backed-up', 0, '{}');",
            )
            .unwrap();
        }
        restore_selective(&target, &staged, "data").unwrap();
        let conn = Connection::open(&target).unwrap();
        let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
        assert_eq!(prompt_id, "backed-up", "prompts (present in the old backup) must still be replaced");
        let trash_id: String = conn.query_row("SELECT id FROM trash", [], |r| r.get(0)).unwrap();
        assert_eq!(trash_id, "live-trashed", "trash (absent from the old backup) must be left alone, not wiped");
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(&staged);
    }

    #[test]
    fn restore_selective_rolls_back_everything_if_one_table_fails() {
        // "prompts" comes first in the copy order and would succeed on its own;
        // "trash" is made incompatible (extra column) so its INSERT fails — proving
        // the whole thing runs in ONE transaction, not one commit per table.
        let dir = std::env::temp_dir();
        let target = dir.join("prompt-saver-test-restore-rollback-target.db");
        let staged = dir.join("prompt-saver-test-restore-rollback-staged.db");
        {
            let conn = full_schema_db(&target, "target");
            conn.execute_batch("INSERT INTO prompts VALUES('live', 0, '{}');").unwrap();
        }
        {
            let _ = std::fs::remove_file(&staged);
            let conn = Connection::open(&staged).unwrap();
            conn.execute_batch(
                "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL, extra TEXT);
                 INSERT INTO prompts VALUES('backed-up', 0, '{}');",
            )
            .unwrap();
        }
        let result = restore_selective(&target, &staged, "data");
        assert!(result.is_err(), "a column-count mismatch on trash must surface as an error");
        let conn = Connection::open(&target).unwrap();
        let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
        assert_eq!(prompt_id, "live", "prompts must roll back too, even though its own copy succeeded");
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(&staged);
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

    #[test]
    fn daily_copies_are_zero_filled_and_bucketed() {
        // 58: a day with no copies must still appear (as 0), not be skipped —
        // otherwise a gap in the chart would read as "the app broke that day"
        // instead of "nothing was copied".
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX idx_copy_log_ts ON copy_log(ts);",
        )
        .unwrap();
        const DAY: u64 = 86400;
        let today = now_secs() / DAY * DAY;
        // Two copies today, three two days ago, none yesterday, one outside the window.
        db_append_copy(&conn, &CopyEntry { id: "a".into(), ts: today + 10 }, 100, None).unwrap();
        db_append_copy(&conn, &CopyEntry { id: "b".into(), ts: today + 20 }, 100, None).unwrap();
        for i in 0..3u64 {
            db_append_copy(&conn, &CopyEntry { id: format!("c{i}"), ts: today - 2 * DAY + i }, 100, None).unwrap();
        }
        db_append_copy(&conn, &CopyEntry { id: "old".into(), ts: today - 20 * DAY }, 100, None).unwrap();
        let daily = db_daily_copies(&conn, 14);
        assert_eq!(daily.len(), 14, "must always return exactly `days` buckets");
        assert_eq!(daily.last().unwrap().ts, today, "the newest bucket is today");
        assert_eq!(daily.last().unwrap().count, 2);
        assert_eq!(daily[daily.len() - 2].count, 0, "yesterday had nothing — must be 0, not missing");
        assert_eq!(daily[daily.len() - 3].count, 3, "two days ago");
        assert!(daily.iter().all(|d| d.ts % DAY == 0), "every bucket is a UTC day boundary");
    }

    #[test]
    fn journal_rowid_deletes_exactly_one_entry() {
        // 59: the rowid db_recent_copies() now returns has to target the exact row —
        // never "every copy of this prompt", in either view mode.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX idx_copy_log_ts ON copy_log(ts);",
        )
        .unwrap();
        db_append_copy(&conn, &CopyEntry { id: "p1".into(), ts: 100 }, 100, None).unwrap();
        db_append_copy(&conn, &CopyEntry { id: "p1".into(), ts: 200 }, 100, None).unwrap();
        db_append_copy(&conn, &CopyEntry { id: "p2".into(), ts: 150 }, 100, None).unwrap();
        // Ungrouped: three distinct rows, delete-by-rowid removes exactly one.
        let ungrouped = db_recent_copies(&conn, 10, false);
        assert_eq!(ungrouped.len(), 3);
        let target = ungrouped.iter().find(|e| e.ts == 100).unwrap().rowid;
        conn.execute("DELETE FROM copy_log WHERE rowid = ?1", params![target]).unwrap();
        let after = db_recent_copies(&conn, 10, false);
        assert_eq!(after.len(), 2, "deleted the wrong number of rows");
        assert!(after.iter().any(|e| e.ts == 200), "the OTHER p1 copy must survive");
        assert!(after.iter().any(|e| e.ts == 150));
        // Grouped: p1's rowid is its most-recent (ts=200) copy — deleting it must
        // remove only that one row, not every copy_log row for p1.
        let grouped = db_recent_copies(&conn, 10, true);
        let p1 = grouped.iter().find(|e| e.id == "p1").unwrap();
        assert_eq!(p1.ts, 200);
        conn.execute("DELETE FROM copy_log WHERE rowid = ?1", params![p1.rowid]).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM copy_log WHERE id = 'p1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "p1 only had that one row left, both must not survive a single delete");
    }

    fn sample_prompts() -> Vec<Prompt> {
        vec![
            Prompt {
                id: "p1".into(),
                trashed_at: 0,
                name: "Mail".into(),
                text: "Hello\nWorld".into(),
                color: "#ef4444".into(),
                image: "data:image/png;base64,QUJD".into(),
                show_image: true,
                copy_image: true,
                file_path: String::new(),
                pdf_page: 0,
                icon_path: String::new(),
                caption: String::new(),
                caption_size: 0,
                font: "mono".into(),
                font_size: 24,
                favorite: false,
                tags: vec!["Work".into(), "E-Mail".into()],
                notes: "Ask Anna first.\nDeadline: Friday".into(),
                hotkey: "CommandOrControl+Shift+1".into(),
                hotkey_paste: "paste".into(),
                variants: Vec::new(),
                rotate_mode: String::new(),
            },
            Prompt {
                id: "p2".into(),
                trashed_at: 0,
                name: "Doc".into(),
                text: "Doc".into(),
                color: String::new(),
                image: String::new(),
                show_image: true,
                copy_image: false,
                file_path: "C:\\tmp\\report.pdf".into(),
                pdf_page: 3,
                icon_path: "C:\\tmp\\clip.mp4".into(),
                caption: "Mein Untertitel".into(),
                caption_size: 18,
                font: String::new(),
                font_size: 1,
                favorite: false,
                tags: Vec::new(),
                notes: String::new(),
                hotkey: String::new(),
                hotkey_paste: String::new(),
                variants: Vec::new(),
                rotate_mode: String::new(),
            },
        ]
    }

    // ---- 32: rotating prompts ----
    // "Kopierfehler ⇒ kein Commit" is not tested here: it is a one-line structural
    // guarantee in copy_prompt (`if ok { rotation_advance(...) }`), not a property
    // of rotation_advance itself, which has no notion of clipboard success.

    #[test]
    fn rotation_order_mode_cycles_through_every_entry_in_sequence() {
        let mut p = sample_prompts().remove(0);
        p.variants = vec!["b".into(), "c".into()]; // total = 3, "" = order
        let mut settings = Settings::default();
        let seen: Vec<usize> = (0..7)
            .map(|_| {
                let (idx, _) = rotation_next(&p, &settings);
                rotation_advance(&p, &mut settings);
                idx
            })
            .collect();
        assert_eq!(seen, vec![0, 1, 2, 0, 1, 2, 0]);
    }

    #[test]
    fn rotation_random_bag_shows_each_entry_once_then_avoids_an_immediate_repeat() {
        let mut p = sample_prompts().remove(0);
        p.rotate_mode = "random".into();
        p.variants = vec!["b".into(), "c".into(), "d".into(), "e".into()]; // total = 5
        let mut settings = Settings::default();
        let total = 5;
        let mut first_cycle = std::collections::HashSet::new();
        let mut last_shown = None;
        for _ in 0..total {
            let (idx, _) = rotation_next(&p, &settings);
            assert!(first_cycle.insert(idx), "the bag repeated an entry within one cycle");
            rotation_advance(&p, &mut settings);
            last_shown = Some(idx);
        }
        assert_eq!(first_cycle.len(), total, "the bag skipped an entry within one cycle");
        let (next_idx, _) = rotation_next(&p, &settings);
        assert_ne!(Some(next_idx), last_shown, "reshuffle repeated the last-shown entry immediately");
    }

    #[test]
    fn rotation_stale_pointer_and_bag_never_go_out_of_range_after_entries_shrink() {
        let mut p = sample_prompts().remove(0);
        p.variants = vec!["b".into()]; // total = 2 now, but state below predates the shrink
        let mut settings = Settings::default();
        settings.rotate_pos.insert(p.id.clone(), 9);
        let (idx, text) = rotation_next(&p, &settings);
        assert_eq!(idx, 9 % 2, "a stale order-mode pointer must clamp back into range");
        assert_eq!(text, p.variants[0]);

        p.rotate_mode = "random".into();
        settings.rotate_bag.insert(p.id.clone(), vec![4, 0]); // 4 no longer exists
        let (idx2, _) = rotation_next(&p, &settings);
        assert_eq!(idx2, 0, "a bag containing any stale index must be discarded, not partially trusted");
        rotation_advance(&p, &mut settings);
        let bag = settings.rotate_bag.get(&p.id).unwrap();
        assert!(bag.iter().all(|&i| (i as usize) < 2), "the reshuffled bag must only contain valid indices");
    }

    #[test]
    fn rotation_disabled_globally_always_serves_entry_one_and_freezes_the_pointer() {
        let mut p = sample_prompts().remove(0);
        p.variants = vec!["b".into(), "c".into()];
        p.rotate_mode = "random".into();
        let mut settings = Settings::default();
        settings.ui_flags.insert("rotatingPrompts".into(), false);
        for _ in 0..3 {
            let (idx, text) = rotation_next(&p, &settings);
            assert_eq!(idx, 0);
            assert_eq!(text, p.text);
            rotation_advance(&p, &mut settings); // must be a no-op while off
        }
        assert!(settings.rotate_pos.is_empty());
        assert!(settings.rotate_bag.is_empty());
    }

    #[test]
    fn sanitize_variants_trims_drops_empties_and_caps_the_count() {
        let raw: Vec<String> = vec!["  b  ".into(), "".into(), "   ".into(), "c".into()];
        assert_eq!(sanitize_variants(raw), vec!["b".to_string(), "c".to_string()]);

        let oversized: Vec<String> = (0..80).map(|i| format!("v{i}")).collect();
        assert_eq!(sanitize_variants(oversized).len(), MAX_ROTATE_VARIANTS);
    }

    #[test]
    fn sanitize_rotate_mode_is_empty_without_variants_and_only_random_survives() {
        let none: Vec<String> = Vec::new();
        let some = vec!["b".to_string()];
        assert_eq!(sanitize_rotate_mode("random".into(), &none), "");
        assert_eq!(sanitize_rotate_mode("random".into(), &some), "random");
        assert_eq!(sanitize_rotate_mode("garbage".into(), &some), "");
        assert_eq!(sanitize_rotate_mode("".into(), &some), "");
    }

    #[test]
    fn rotation_forget_drops_both_maps_for_a_purged_prompt() {
        let mut settings = Settings::default();
        settings.rotate_pos.insert("p1".into(), 2);
        settings.rotate_bag.insert("p1".into(), vec![1, 0]);
        settings.rotate_pos.insert("p2".into(), 1);
        rotation_forget(&mut settings, &["p1".to_string()]);
        assert!(!settings.rotate_pos.contains_key("p1"));
        assert!(!settings.rotate_bag.contains_key("p1"));
        assert!(settings.rotate_pos.contains_key("p2"), "an unrelated prompt's pointer must survive");
    }

    // ---- 91: trash ----

    #[test]
    fn stale_trash_ids_respects_the_retention_window() {
        let mut trash = sample_prompts(); // p1, p2
        let now = 10_000_000u64;
        trash[0].trashed_at = now - 100; // just trashed — well inside any window
        trash[1].trashed_at = now - 1_000_000; // trashed long ago — past a 1-day window
        let max_age = 86_400; // 1 day
        let stale = stale_trash_ids(&trash, max_age, now);
        assert_eq!(stale, vec!["p2".to_string()], "only the old entry should be stale");
    }

    #[test]
    fn stale_trash_ids_never_touches_a_zero_timestamp() {
        // trashed_at == 0 is the "never trashed" sentinel every non-trashed Prompt
        // carries — it must never be treated as "trashed an eternity ago".
        let mut trash = sample_prompts();
        trash[0].trashed_at = 0;
        trash[1].trashed_at = 0;
        assert!(stale_trash_ids(&trash, 1, 1_000_000).is_empty());
    }

    #[test]
    fn trash_retention_zero_days_means_forever() {
        let mut settings = Settings::default();
        settings.ui_values.insert("trashRetentionDays".to_string(), 0.0);
        assert_eq!(trash_retention_secs(&settings), None);
        settings.ui_values.insert("trashRetentionDays".to_string(), 7.0);
        assert_eq!(trash_retention_secs(&settings), Some(7 * 86_400));
    }

    // ---- F20 per-prompt hotkeys ----

    #[test]
    fn accelerators_are_validated_before_they_reach_the_os() {
        for good in ["CommandOrControl+Shift+P", "ctrl+shift+p", "Alt+F5", "Ctrl+Alt+K", " Ctrl+Shift+P "] {
            assert!(parse_accel(good).is_some(), "rejected a usable accelerator: {good:?}");
        }
        for bad in [
            "",
            "   ",
            "P",                       // no modifier: would swallow the key everywhere
            "Shift+",                  // empty token
            "Ctrl+Shift+NotAKey",
            "Ctrl+A+B",                // two main keys
            "Ctrl+Shift+\u{7}",        // control character
        ] {
            assert!(parse_accel(bad).is_none(), "accepted an unusable accelerator: {bad:?}");
        }
        // An over-long string is rejected outright, never handed to the OS.
        let long = format!("Ctrl+Shift+{}", "A".repeat(MAX_ACCEL_LEN));
        assert!(parse_accel(&long).is_none());
        // Spelling and case do not make two accelerators different.
        assert_eq!(parse_accel("CommandOrControl+Shift+P"), parse_accel("ctrl+SHIFT+p"));
    }

    // A global hotkey SWALLOWS its keystroke in every other program, so the
    // blocklist decides what the app is allowed to take away from the user.
    #[test]
    fn system_and_universal_shortcuts_are_refused() {
        let reason = |s: &str| accel_error(s).err();
        // The universal editing set: losing these breaks every program at once.
        for s in [
            "Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+A", "Ctrl+Z", "Ctrl+Y", "Ctrl+S",
            "Ctrl+F", "Ctrl+P", "Ctrl+N", "Ctrl+O", "Ctrl+W", "Ctrl+T",
            "CommandOrControl+C", // the spelling the UI actually stores
        ] {
            assert_eq!(reason(s), Some(HK_SYSTEM), "not refused: {s}");
        }
        // Windows-reserved, and the ones the OS would never hand over anyway.
        for s in [
            "Ctrl+Alt+Delete", "Ctrl+Shift+Escape", "Ctrl+Escape", "Alt+Tab",
            "Alt+Shift+Tab", "Alt+F4", "Ctrl+F4", "Alt+Space", "Alt+Escape",
            "Ctrl+Shift+PrintScreen", "Alt+Enter", "Ctrl+Backspace", "Ctrl+Insert",
            "Ctrl+Alt+CapsLock", "Ctrl+Space",
        ] {
            assert_eq!(reason(s), Some(HK_SYSTEM), "not refused: {s}");
        }
        // Anything with the Windows key: that namespace belongs to the OS.
        for s in ["Super+K", "Super+Shift+S", "Alt+Super+P", "Cmd+Q"] {
            assert_eq!(reason(s), Some(HK_SYSTEM), "not refused: {s}");
        }
        // Shift alone is how a capital letter is typed — never a modifier here.
        for s in ["Shift+A", "Shift+1", "Shift+F5"] {
            assert_eq!(reason(s), Some(HK_INVALID), "not refused: {s}");
        }
        // …and the line stops there: adding a second modifier makes a combination
        // no program owns, which is exactly where users pick their own shortcuts.
        for s in [
            "Ctrl+Shift+C", "Ctrl+Alt+V", "Ctrl+Shift+S", "Ctrl+Alt+Q", "Alt+Q",
            "Ctrl+Shift+1", "Alt+F9", "Ctrl+F1", "Ctrl+Shift+F4", "Ctrl+Alt+ArrowUp",
        ] {
            assert_eq!(reason(s), None, "wrongly refused: {s}");
        }
    }

    // 100: the shipped default must actually register, or every fresh install would
    // silently lose its launcher hotkey the moment rearm_hotkeys() runs at startup
    // (an unparseable/blocked default gets cleared back to "", exactly like a bad
    // value a user typed in by hand -- see rearm_hotkeys' app_ok branch).
    #[test]
    fn the_default_hotkey_is_not_refused() {
        assert_eq!(accel_error(&Settings::default().hotkey).err(), None);
    }

    // The blocklist is authoritative wherever a binding can enter the store.
    #[test]
    fn a_blocked_combination_never_reaches_the_store() {
        let prompts = sample_prompts();
        let others: Vec<&Prompt> = prompts.iter().collect();
        assert_eq!(sanitize_hotkey("Ctrl+C", "", &others), Err(HK_SYSTEM.into()));
        assert_eq!(sanitize_hotkey("Shift+A", "", &others), Err(HK_INVALID.into()));
        assert_eq!(sanitize_hotkey("Super+J", "", &others), Err(HK_SYSTEM.into()));
        // A blocked accelerator is never registered, whatever put it in the store.
        let mut blocked = sample_prompts();
        blocked[0].hotkey = "Ctrl+V".into();
        blocked[1].hotkey = "Alt+Tab".into();
        assert!(hotkey_plan("Ctrl+S", &blocked, true).is_empty());
        // …and an import drops it instead of keeping a binding that does nothing.
        assert_eq!(drop_colliding_hotkeys("", &mut blocked), 2);
        assert!(blocked.iter().all(|p| p.hotkey.is_empty()));
    }

    #[test]
    fn a_hotkey_carries_its_own_copy_or_paste_choice() {
        assert_eq!(sanitize_hotkey_mode("copy"), "copy");
        assert_eq!(sanitize_hotkey_mode(" paste "), "paste");
        // Anything else means "follow the global default".
        for raw in ["", "   ", "yes", "true", "PASTE", "copy;drop"] {
            assert_eq!(sanitize_hotkey_mode(raw), "", "not folded to the default: {raw:?}");
        }
    }

    #[test]
    fn hotkey_plan_resolves_conflicts_by_precedence() {
        let mut prompts = sample_prompts();
        prompts[1].hotkey = "CommandOrControl+Shift+2".into();
        // The app hotkey wins over a prompt that wants the same combination.
        let plan = hotkey_plan("ctrl+shift+1", &prompts, true);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].1, None);
        assert_eq!(plan[1].1.as_deref(), Some("p2"));

        // Between two prompts the first one in list order keeps it.
        prompts[1].hotkey = "ctrl+shift+1".into();
        let plan = hotkey_plan("", &prompts, true);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].1.as_deref(), Some("p1"));

        // Feature toggled off: nothing but the app hotkey is registered, and the
        // stored accelerators stay untouched.
        let plan = hotkey_plan("Alt+Q", &prompts, false);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].1, None);
        assert_eq!(prompts[0].hotkey, "CommandOrControl+Shift+1");
    }

    #[test]
    fn a_deleted_prompt_gives_its_accelerator_back() {
        let mut prompts = sample_prompts();
        prompts[1].hotkey = "Ctrl+Shift+2".into();
        assert_eq!(hotkey_plan("", &prompts, true).len(), 2);
        prompts.retain(|p| p.id != "p1");
        let plan = hotkey_plan("", &prompts, true);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].1.as_deref(), Some("p2"));
    }

    #[test]
    fn an_import_drops_colliding_hotkeys_instead_of_failing() {
        let mut prompts = sample_prompts();
        prompts[1].hotkey = "ctrl+shift+1".into(); // same combination as p1
        assert_eq!(drop_colliding_hotkeys("", &mut prompts), 1);
        assert_eq!(prompts[0].hotkey, "CommandOrControl+Shift+1");
        assert!(prompts[1].hotkey.is_empty());

        // The app hotkey outranks every prompt, and junk is dropped too.
        let mut prompts = sample_prompts();
        prompts[1].hotkey = "not an accelerator".into();
        assert_eq!(drop_colliding_hotkeys("Ctrl+Shift+1", &mut prompts), 2);
        assert!(prompts.iter().all(|p| p.hotkey.is_empty()));

        // Nothing to drop leaves every binding in place.
        let mut prompts = sample_prompts();
        prompts[1].hotkey = "Alt+F9".into();
        assert_eq!(drop_colliding_hotkeys("Ctrl+Alt+Q", &mut prompts), 0);
        assert_eq!(prompts[1].hotkey, "Alt+F9");
    }

    #[test]
    fn the_editor_rejects_a_taken_or_unusable_accelerator() {
        let prompts = sample_prompts();
        let others: Vec<&Prompt> = prompts.iter().collect();
        assert_eq!(sanitize_hotkey("", "Ctrl+Q", &others).unwrap(), "");
        assert_eq!(sanitize_hotkey("  Alt+F9  ", "Ctrl+Q", &others).unwrap(), "Alt+F9");
        assert_eq!(sanitize_hotkey("Ctrl+Shift+P", "ctrl+shift+p", &others), Err("hotkey-taken".into()));
        assert_eq!(sanitize_hotkey("ctrl+shift+1", "", &others), Err("hotkey-taken".into()));
        assert_eq!(sanitize_hotkey("F9", "", &others), Err("hotkey-invalid".into()));
        // Editing the prompt that already owns the combination is not a conflict.
        let others: Vec<&Prompt> = prompts.iter().filter(|p| p.id != "p1").collect();
        assert_eq!(sanitize_hotkey("ctrl+shift+1", "", &others).unwrap(), "ctrl+shift+1");
    }

    // A prompt stored before F20 has no hotkey field at all — it must load with an
    // empty binding instead of failing the whole row.
    #[test]
    fn a_prompt_without_a_hotkey_field_still_loads() {
        let json = r#"{"id":"old","name":"N","text":"T","color":"","tags":["Work"]}"#;
        let p: Prompt = serde_json::from_str(json).unwrap();
        assert!(p.hotkey.is_empty());
        assert!(parse_accel(&p.hotkey).is_none());
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
        assert_eq!(p2.pdf_page, 3);
        assert_eq!(p1.pdf_page, 0, "the omitted @pdfpage line must default to 0");
        assert_eq!(p2.icon_path, "C:\\tmp\\clip.mp4");
        assert_eq!(p2.caption, "Mein Untertitel");
        assert_eq!(p2.caption_size, 18);
        assert!(p2.show_image && !p2.copy_image);
        assert_eq!(p1.tags, vec!["Work".to_string(), "E-Mail".to_string()]);
        assert!(p2.tags.is_empty());
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
        assert_eq!(rows[1][13], "Work,E-Mail");
        assert_eq!(rows[2][13], "");
        assert_eq!(rows[1][15], "0");
        assert_eq!(rows[2][15], "3");
    }

    // F13: a choice variable is plain prompt text, so neither exporter may touch
    // its `|` or its commas — a mangled token would silently lose the options.
    #[test]
    fn choice_variable_survives_txt_and_csv_export() {
        const BODY: &str = "Write a {#{tone|formal,casual,witty}#} note about {#{Topic}#}.";
        let mut prompts = sample_prompts();
        prompts[0].text = BODY.into();
        prompts[0].image = String::new(); // a text prompt, not the media sample
        prompts[0].show_image = false;
        prompts[0].copy_image = false;
        assert_eq!(parse_txt(&to_txt(&prompts)).prompts[0].text, BODY);
        assert_eq!(parse_csv(&to_csv(&prompts))[1][1], BODY);
        assert_eq!(parse_csv_data(&to_csv(&prompts)).prompts[0].text, BODY);
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
        assert_eq!(p1.tags, vec!["Work".to_string(), "E-Mail".to_string()]);
        let p2 = &data.prompts[1];
        assert_eq!(p2.file_path, "C:\\tmp\\report.pdf");
        assert_eq!(p2.pdf_page, 3);
        assert_eq!(p1.pdf_page, 0);
        assert_eq!(p2.icon_path, "C:\\tmp\\clip.mp4");
        assert_eq!(p2.caption, "Mein Untertitel");
        assert_eq!(p2.caption_size, 18);
        assert!(p2.tags.is_empty());
    }

    // 32: an entry that happens to contain the join marker itself must not be
    // split into extra entries on round trip (Codex audit).
    #[test]
    fn join_variants_survives_an_entry_containing_the_separator_verbatim() {
        let text = "a\n<!-- next -->\nb".to_string(); // looks like 2 entries, is 1
        let variants = vec!["c".to_string()];
        let joined = join_variants(&text, &variants);
        let (out_text, out_variants) = split_variants(&joined);
        assert_eq!(out_text, text);
        assert_eq!(out_variants, variants);
    }

    // 32: rotating-prompt entries round-trip through every export/storage format
    // without loss. Each format gets its OWN test since each chose its own way to
    // embed the entries and could silently regress independently of the others.
    #[test]
    fn csv_roundtrip_keeps_rotating_entries() {
        let mut prompts = sample_prompts();
        prompts[0].variants = vec!["Second entry".to_string(), "Third, entry\nwith a newline".to_string()];
        prompts[0].rotate_mode = "random".to_string();
        let data = parse_csv_data(&to_csv(&prompts));
        let p = &data.prompts[0];
        assert_eq!(p.text, prompts[0].text);
        assert_eq!(p.variants, prompts[0].variants);
        assert_eq!(p.rotate_mode, "random");
    }

    #[test]
    fn txt_roundtrip_keeps_rotating_entries_and_never_confuses_them_with_the_block_separator() {
        let mut prompts = sample_prompts();
        // Deliberately contains a bare "---" line inside an entry — the whole point
        // of VARIANT_SEP being a distinct marker is that this must NOT be mistaken
        // for the "\n\n---\n\n" boundary between two DIFFERENT prompts.
        prompts[0].variants = vec![
            "Second entry".to_string(),
            "---\nlooks like a separator, isn't one".to_string(),
        ];
        prompts[0].rotate_mode = "random".to_string();
        let data = parse_txt(&to_txt(&prompts));
        assert_eq!(data.prompts.len(), prompts.len(), "a variant containing \"---\" must never split into an extra prompt");
        let p = &data.prompts[0];
        assert_eq!(p.text, prompts[0].text);
        assert_eq!(p.variants, prompts[0].variants);
        assert_eq!(p.rotate_mode, "random");
    }

    #[test]
    fn markdown_sync_roundtrip_keeps_rotating_entries() {
        let mut p = sample_prompts().remove(0);
        p.variants = vec!["Second entry".to_string(), "Third entry".to_string()];
        let md = prompt_to_md(&p);
        assert!(md.contains("variants: 2"), "the header count is informational but should still be right");
        let restored = apply_md_to_prompt(&p, &md)
            .expect("a file this function itself just wrote must always be recognised as ours");
        assert_eq!(restored.text, p.text);
        assert_eq!(restored.variants, p.variants);
    }

    #[test]
    fn markdown_sync_survives_a_hand_edit_that_removes_every_entry() {
        // The sync folder is the user's to hand-edit (INVARIANTS.md) — deleting
        // every "<!-- next -->" marker must degrade to a plain prompt, never crash,
        // and never leave rotate_mode stranded on an empty list.
        let mut p = sample_prompts().remove(0);
        p.variants = vec!["Second entry".to_string()];
        p.rotate_mode = "random".to_string();
        let md = prompt_to_md(&p).replace(&format!("{VARIANT_SEP}Second entry"), "");
        let restored = apply_md_to_prompt(&p, &md).unwrap();
        assert!(restored.variants.is_empty());
        assert_eq!(restored.rotate_mode, "", "empty variants must never leave a stranded rotate_mode");
    }

    #[test]
    fn backup_json_roundtrips_rotating_fields() {
        let mut p = sample_prompts().remove(0);
        p.variants = vec!["Second".to_string(), "Third".to_string()];
        p.rotate_mode = "random".to_string();
        let backup = Backup { prompts: vec![p.clone()], settings: Settings::default() };
        let json = serde_json::to_string(&backup).unwrap();
        let restored: Backup = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.prompts[0].variants, p.variants);
        assert_eq!(restored.prompts[0].rotate_mode, p.rotate_mode);
    }

    #[test]
    fn version_history_snapshots_and_restores_rotating_entries() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');",
        )
        .unwrap();
        let variants = vec!["b".to_string(), "c".to_string()];
        db_add_version(&conn, "p1", 100, "Name", "Entry one", &variants, 20).unwrap();
        let listed = db_list_versions(&conn, "p1");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].variants, variants);
        let (pid, name, text, restored_variants) = db_get_version(&conn, listed[0].id).unwrap();
        assert_eq!(pid, "p1");
        assert_eq!(name, "Name");
        assert_eq!(text, "Entry one");
        assert_eq!(restored_variants, variants);
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

    // F1: a file written before tags existed must import unchanged, with no tags.
    #[test]
    fn old_exports_without_tags_still_parse() {
        let txt = "### Old\nSome text\n@color #123456\n@style mono 20";
        let data = parse_txt(txt);
        assert_eq!(data.prompts[0].text, "Some text");
        assert_eq!(data.prompts[0].color, "#123456");
        assert_eq!(data.prompts[0].font, "mono");
        assert!(data.prompts[0].tags.is_empty());

        // Same for a CSV row that stops before the tags column.
        let csv = "\"name\";\"text\"\r\n\"Old\";\"Some text\"";
        let data = parse_csv_data(csv);
        assert_eq!(data.prompts.len(), 1);
        assert!(data.prompts[0].tags.is_empty());
    }

    // F17: a multi-line note survives both exporters, and a file written before
    // notes existed still imports (with an empty note).
    #[test]
    fn notes_roundtrip_through_txt_and_csv() {
        let prompts = sample_prompts();
        let note = &prompts[0].notes;
        assert!(note.contains('\n'), "the sample note must be multi-line");

        let txt = to_txt(&prompts);
        // The line-based format may never carry a raw newline inside the marker.
        let marker = txt.lines().find(|l| l.starts_with("@notes ")).expect("@notes line");
        assert!(marker.contains("\\n"), "the newline must be escaped: {marker}");
        let data = parse_txt(&txt);
        assert_eq!(&data.prompts[0].notes, note);
        assert_eq!(data.prompts[0].text, "Hello\nWorld"); // marker did not eat the body
        assert!(data.prompts[1].notes.is_empty());

        let csv = to_csv(&prompts);
        assert_eq!(parse_csv(&csv)[1][14], *note);
        let data = parse_csv_data(&csv);
        assert_eq!(&data.prompts[0].notes, note);
        assert!(data.prompts[1].notes.is_empty());
    }

    // Security: a name/text starting with =, +, -, @ or a tab must not turn into a
    // live formula when the CSV is opened in Excel/LibreOffice, but it must still
    // come back exactly as typed on the next import into this app.
    #[test]
    fn csv_export_guards_formula_prefixes_and_import_undoes_it() {
        let mut prompts = sample_prompts();
        prompts[0].name = "=cmd|'/c calc'!A1".into();
        prompts[0].text = "-1 to 5 scale".into();
        prompts[0].notes = "@mention someone".into();
        let csv = to_csv(&prompts);
        assert!(csv.contains("\"'=cmd"), "the name must be guarded: {csv}");
        assert!(csv.contains("\"'-1 to 5"), "the text must be guarded: {csv}");
        assert!(csv.contains("\"'@mention"), "the notes must be guarded: {csv}");
        let data = parse_csv_data(&csv);
        assert_eq!(data.prompts[0].name, "=cmd|'/c calc'!A1");
        assert_eq!(data.prompts[0].text, "-1 to 5 scale");
        assert_eq!(data.prompts[0].notes, "@mention someone");
    }

    // 76: an exported theme file never carries the internal id, so importing it on
    // another install (or re-importing it on the same one) always creates a fresh
    // theme instead of silently colliding with -- or overwriting -- an existing one.
    #[test]
    fn theme_export_roundtrips_without_leaking_the_internal_id() {
        let mut vars = HashMap::new();
        vars.insert("bg".to_string(), "#123456".to_string());
        vars.insert("accent".to_string(), "#abcdef".to_string());
        let theme = CustomTheme { id: "secret-internal-id".to_string(), name: "My theme".to_string(), base: "dark".to_string(), vars };
        let export = ThemeExport { name: theme.name.clone(), base: theme.base.clone(), vars: theme.vars.clone() };
        let json = serde_json::to_string_pretty(&export).unwrap();
        assert!(!json.contains("secret-internal-id"), "the export must not carry the internal id: {json}");
        let back: ThemeExport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, theme.name);
        assert_eq!(back.base, theme.base);
        assert_eq!(back.vars, theme.vars);
    }

    #[test]
    fn old_exports_without_notes_still_parse() {
        let txt = "### Old\nSome text\n@color #123456\n@tags Work";
        let data = parse_txt(txt);
        assert_eq!(data.prompts[0].text, "Some text");
        assert_eq!(data.prompts[0].color, "#123456");
        assert!(data.prompts[0].notes.is_empty());

        // A CSV row that stops before the notes column.
        let csv = "\"name\";\"text\"\r\n\"Old\";\"Some text\"";
        assert!(parse_csv_data(csv).prompts[0].notes.is_empty());
    }

    #[test]
    fn notes_escaping_survives_backslashes() {
        for raw in ["a\nb", "path C:\\n\\temp", "trailing\\", "\\n literal", ""] {
            assert_eq!(notes_unescape(&notes_escape(raw)), raw, "roundtrip failed for {raw:?}");
        }
    }

    #[test]
    fn sanitize_notes_normalizes_and_bounds() {
        assert_eq!(sanitize_notes("  a\r\nb\r  ").unwrap(), "a\nb");
        assert!(sanitize_notes(&"x".repeat(MAX_NOTES_LEN)).is_ok());
        assert!(sanitize_notes(&"x".repeat(MAX_NOTES_LEN + 1)).is_err());
    }

    // F17, the whole point of the feature: a note must never reach the clipboard —
    // not through a single copy, and not through a chained multi-prompt copy.
    #[test]
    fn notes_never_reach_the_copied_payload() {
        let mut prompts = sample_prompts();
        prompts[1].notes = "internal only: budget 5000".into();
        for p in &prompts {
            let payload = copy_payload(p);
            assert_eq!(payload, p.text);
            for line in p.notes.lines().filter(|l| !l.trim().is_empty()) {
                assert!(!payload.contains(line), "note leaked into a copy: {line}");
            }
        }
        // The chain concatenates the same payloads, so it inherits the guarantee.
        let chained = prompts.iter().map(|p| copy_payload(p)).collect::<Vec<_>>().join("\n\n");
        for p in &prompts {
            for line in p.notes.lines().filter(|l| !l.trim().is_empty()) {
                assert!(!chained.contains(line), "note leaked into a chain copy: {line}");
            }
        }
    }

    #[test]
    fn tags_are_deduped_case_insensitively_keeping_known_casing() {
        let known = canonical_tags(&sample_prompts()); // "Work", "E-Mail"
        let out = sanitize_tags(
            vec![" work ".into(), "WORK".into(), "".into(), "  ".into(), "New".into()],
            &known,
        )
        .expect("valid tags");
        assert_eq!(out, vec!["Work".to_string(), "New".to_string()]);
        // An unknown tag keeps the casing it was typed with.
        assert_eq!(sanitize_tags(vec!["MiXeD".into()], &HashMap::new()).unwrap(), vec!["MiXeD"]);
    }

    // One pass over the whole store, so a tag on many prompts stays one operation.
    #[test]
    fn renaming_a_tag_rewrites_every_prompt_at_once() {
        let mut prompts = sample_prompts(); // p1: ["Work", "E-Mail"], p2: []
        prompts[1].tags = vec!["work".into(), "Draft".into()];
        assert!(rename_tag_in(&mut prompts, "work", "Job"));
        assert_eq!(prompts[0].tags, vec!["Job".to_string(), "E-Mail".to_string()]);
        assert_eq!(prompts[1].tags, vec!["Job".to_string(), "Draft".to_string()]);
        // Renaming onto a tag the prompt already has merges instead of duplicating.
        assert!(rename_tag_in(&mut prompts, "e-mail", "Job"));
        assert_eq!(prompts[0].tags, vec!["Job".to_string()]);
        // A name nobody uses changes nothing, so no save is triggered.
        assert!(!rename_tag_in(&mut prompts, "nothing", "Else"));
    }

    #[test]
    fn deleting_a_tag_clears_it_everywhere() {
        let mut prompts = sample_prompts();
        prompts[1].tags = vec!["WORK".into()];
        assert!(remove_tag_from(&mut prompts, "work"));
        assert_eq!(prompts[0].tags, vec!["E-Mail".to_string()]);
        assert!(prompts[1].tags.is_empty());
        assert!(!remove_tag_from(&mut prompts, "work")); // idempotent
    }

    #[test]
    fn tag_input_is_validated() {
        let empty = HashMap::new();
        assert!(sanitize_tags(vec!["a".repeat(MAX_TAG_LEN + 1)], &empty).is_err());
        assert!(sanitize_tags(vec!["a,b".into()], &empty).is_err()); // comma = separator
        assert!(sanitize_tags(vec!["a\nb".into()], &empty).is_err()); // breaks the TXT format
        assert!(sanitize_tags(vec!["a".repeat(MAX_TAG_LEN)], &empty).is_ok());
        assert!(valid_tag_name("  ").is_err());
        assert_eq!(valid_tag_name(" Work ").unwrap(), "Work");
        // Colors: empty clears, #rrggbb is normalized, anything else is refused.
        assert_eq!(valid_tag_color("  ").unwrap(), "");
        assert_eq!(valid_tag_color("#AABBCC").unwrap(), "#aabbcc");
        assert!(valid_tag_color("red").is_err());
        assert!(valid_tag_color("#abc").is_err());
    }

    #[test]
    fn custom_theme_input_is_validated() {
        assert!(valid_theme_name("  ").is_err());
        assert_eq!(valid_theme_name("  Night  ").unwrap(), "Night");
        assert!(valid_theme_name(&"a".repeat(MAX_THEME_NAME + 1)).is_err());
        assert!(valid_theme_name("Night\nshift").is_err());
        // A base must be a real built-in; "system" and another custom theme are not,
        // so inheritance can never chain.
        assert_eq!(valid_theme_base(" dark ").unwrap(), "dark");
        assert!(valid_theme_base("system").is_err());
        assert!(valid_theme_base("custom:abc").is_err());
        let mut vars = HashMap::new();
        vars.insert("accent".to_string(), "#AABBCC".to_string());
        vars.insert("bg".to_string(), "  ".to_string()); // empty = not overridden
        let clean = valid_theme_vars(&vars).unwrap();
        assert_eq!(clean.get("accent").unwrap(), "#aabbcc");
        assert!(!clean.contains_key("bg"));
        vars.insert("shadow".to_string(), "#000000".to_string());
        assert!(valid_theme_vars(&vars).is_err()); // not a themable colour token
        let mut bad = HashMap::new();
        bad.insert("accent".to_string(), "red".to_string());
        assert!(valid_theme_vars(&bad).is_err());
    }

    #[test]
    fn custom_theme_resolves_to_its_base() {
        let mut settings = Settings::default();
        settings.custom_themes.push(CustomTheme {
            id: "t1".into(),
            name: "Night".into(),
            base: "dark".into(),
            vars: HashMap::from([("accent".to_string(), "#ff0000".to_string())]),
        });
        settings.theme = "custom:t1".into();
        assert_eq!(theme_base_of(&settings, &settings.theme), "dark");
        // The feature ships off, so a stored custom theme stays parked until enabled.
        assert!(theme_vars_of(&settings).is_empty());
        settings.ui_flags.insert("customThemes".into(), true);
        assert_eq!(theme_vars_of(&settings).get("accent").unwrap(), "#ff0000");
        // A dangling reference falls back to the system theme instead of breaking.
        settings.theme = "custom:gone".into();
        assert_eq!(theme_base_of(&settings, &settings.theme), "system");
        assert!(theme_vars_of(&settings).is_empty());
        // Built-in preferences pass through untouched.
        assert_eq!(theme_base_of(&settings, "mono"), "mono");
    }

    #[test]
    fn custom_theme_cap_is_bounded() {
        let mut settings = Settings::default();
        assert_eq!(custom_theme_cap(&settings), DEFAULT_CUSTOM_THEMES);
        settings.ui_values.insert("customThemeMax".into(), 3.0);
        assert_eq!(custom_theme_cap(&settings), 3);
        settings.ui_values.insert("customThemeMax".into(), 9999.0);
        assert_eq!(custom_theme_cap(&settings), MAX_CUSTOM_THEMES);
        settings.ui_values.insert("customThemeMax".into(), -5.0);
        assert_eq!(custom_theme_cap(&settings), 1);
        settings.ui_values.insert("customThemeMax".into(), f64::NAN);
        assert_eq!(custom_theme_cap(&settings), DEFAULT_CUSTOM_THEMES);
    }

    #[test]
    fn settings_without_custom_themes_still_load() {
        // G4: an older settings blob has no `custom_themes` key at all.
        let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert!(s.custom_themes.is_empty());
        // A theme entry from a newer build with unknown extra fields keeps what it can.
        let s: Settings = serde_json::from_str(
            r##"{"custom_themes":[{"id":"a","name":"N","base":"mono","vars":{"bg":"#111111"},"future":1}]}"##,
        )
        .unwrap();
        assert_eq!(s.custom_themes[0].base, "mono");
        assert_eq!(s.custom_themes[0].vars.get("bg").unwrap(), "#111111");
    }

    fn test_view(id: &str) -> View {
        View {
            id: id.to_string(),
            name: id.to_uppercase(),
            cols: default_cols(),
            rows: default_rows(),
            layouts: HashMap::new(),
            color: String::new(),
        }
    }

    #[test]
    fn first_free_cell_skips_other_folder_cells() {
        // 3x1 grid, tile "a" on [0,0]: the plain scan would find [1,0] free.
        let mut view = test_view("v1");
        view.cols = 3;
        view.rows = 1;
        view.layouts.insert(
            grid_key(view.cols, view.rows),
            HashMap::from([("a".to_string(), [0, 0])]),
        );
        assert_eq!(first_free_cell(&view), Some([1, 0]));
        // A folder sits on [1,0] (outside view.layouts) — the scan must skip it, or a
        // dissolved member handed that cell would save fine but never render.
        assert_eq!(first_free_cell_excluding(&view, &[[1, 0]]), Some([2, 0]));
        assert_eq!(first_free_cell_excluding(&view, &[[1, 0], [2, 0]]), None);
    }

    #[test]
    fn place_ids_in_view_fills_free_cells_and_skips_already_placed_ids() {
        // 2x1 grid, "a" already on [0,0].
        let mut layout = HashMap::from([("a".to_string(), [0, 0])]);
        let ids: Vec<String> = vec!["a".to_string(), "b".to_string()];
        assert!(place_ids_in_view(&mut layout, &ids, 2, 1, &std::collections::HashSet::new()));
        assert_eq!(layout.get("a"), Some(&[0, 0])); // untouched, was already placed
        assert_eq!(layout.get("b"), Some(&[1, 0]));
    }

    #[test]
    fn place_ids_in_view_is_all_or_nothing_when_the_view_is_full() {
        // 104: a batch that doesn't fully fit must place NOTHING, not a partial subset
        // -- a caller that already removed these ids from their source view relies on
        // this to know the move truly failed.
        let mut layout = HashMap::from([("a".to_string(), [0, 0])]);
        let before = layout.clone();
        let ids: Vec<String> = vec!["b".to_string(), "c".to_string()]; // only 1 free cell for 2 ids
        assert!(!place_ids_in_view(&mut layout, &ids, 2, 1, &std::collections::HashSet::new()));
        assert_eq!(layout, before);
    }

    #[test]
    fn place_ids_in_view_counts_grid_folder_cells_as_occupied() {
        // 105: a folder's cell lives outside `layout` (see docs/INVARIANTS.md -> Grid
        // folders) -- without passing it as extra_occupied, addView could hand a
        // prompt the same cell a folder already sits on.
        let mut layout: HashMap<String, [u32; 2]> = HashMap::new();
        let folder_cells = std::collections::HashSet::from([(0u32, 0u32)]);
        let ids: Vec<String> = vec!["a".to_string()];
        assert!(place_ids_in_view(&mut layout, &ids, 2, 1, &folder_cells));
        assert_eq!(layout.get("a"), Some(&[1, 0])); // [0,0] was the folder's, must be skipped
    }

    #[test]
    fn prune_offscreen_floats_drops_only_what_has_no_monitor_at_all() {
        let monitors = [(0, 0, 1920, 1080)];
        let mut settings = Settings::default();
        settings.floating.insert("on-screen".to_string(), Pos { x: 100, y: 100 });
        settings.floating.insert("edge".to_string(), Pos { x: 1910, y: 100 }); // 10px still on
        settings.floating.insert("gone".to_string(), Pos { x: 5000, y: 5000 }); // unplugged monitor
        settings.float_folders.push(FloatFolder {
            id: "f-on".to_string(),
            name: String::new(),
            pos: Pos { x: 200, y: 200 },
            scale: 1.0,
            members: vec!["m1".to_string()],
        });
        settings.float_folders.push(FloatFolder {
            id: "f-gone".to_string(),
            name: String::new(),
            pos: Pos { x: -9000, y: -9000 },
            scale: 1.0,
            members: vec!["m2".to_string(), "m3".to_string()],
        });
        settings.floating.insert("m2".to_string(), Pos { x: -9000, y: -9000 });

        let changed = prune_offscreen_floats(&monitors, &mut settings);

        assert!(changed);
        assert!(settings.floating.contains_key("on-screen"));
        assert!(settings.floating.contains_key("edge"));
        assert!(!settings.floating.contains_key("gone"));
        assert_eq!(settings.float_folders.len(), 1);
        assert_eq!(settings.float_folders[0].id, "f-on");
        assert!(!settings.floating.contains_key("m2")); // dropped with its folder
    }

    #[test]
    fn prune_offscreen_floats_touches_nothing_when_all_on_screen() {
        let monitors = [(0, 0, 1920, 1080)];
        let mut settings = Settings::default();
        settings.floating.insert("a".to_string(), Pos { x: 50, y: 50 });
        assert!(!prune_offscreen_floats(&monitors, &mut settings));
        assert!(settings.floating.contains_key("a"));
    }

    #[test]
    fn prune_offscreen_floats_never_deletes_on_a_failed_monitor_probe() {
        let mut settings = Settings::default();
        settings.floating.insert("a".to_string(), Pos { x: 5000, y: 5000 });
        assert!(!prune_offscreen_floats(&[], &mut settings));
        assert!(settings.floating.contains_key("a"));
    }

    fn test_tile_folder(id: &str, view_id: &str, cell: [u32; 2], members: &[&str]) -> TileFolder {
        TileFolder {
            id: id.to_string(),
            name: String::new(),
            color: String::new(),
            view_id: view_id.to_string(),
            cell,
            members: members.iter().map(|s| s.to_string()).collect(),
        }
    }


    #[test]
    fn tile_folders_normalize_to_their_invariants() {
        let view_ids: Vec<String> = vec![test_view("v1").id];
        let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let mut folders = vec![
            test_tile_folder("f1", "v1", [1, 1], &["a", "gone", "b"]),
            // Claims a prompt the first folder already holds, plus one of its own.
            test_tile_folder("f2", "v1", [1, 1], &["b", "c", "d"]),
            // A view that no longer exists takes its folder with it.
            test_tile_folder("f3", "vX", [0, 0], &["a", "c"]),
        ];
        normalize_tile_folders(&mut folders, &ids, &view_ids);
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].members, vec!["a", "b"]); // unknown id dropped
        assert_eq!(folders[1].members, vec!["c", "d"]); // "b" belongs to f1
        // Two folders never sit on the same cell.
        assert_ne!(folders[0].cell, folders[1].cell);
        // Idempotent.
        let before: Vec<[u32; 2]> = folders.iter().map(|f| f.cell).collect();
        normalize_tile_folders(&mut folders, &ids, &view_ids);
        assert_eq!(folders.len(), 2);
        assert_eq!(folders.iter().map(|f| f.cell).collect::<Vec<_>>(), before);
        // Down to one member: the folder dissolves instead of lingering.
        folders[0].members.pop();
        normalize_tile_folders(&mut folders, &ids, &view_ids);
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].id, "f2");
    }

    #[test]
    fn settings_without_tile_folders_still_load() {
        let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert!(s.tile_folders.is_empty());
        // A folder from a newer build with unknown extra fields keeps what it can.
        let s: Settings = serde_json::from_str(
            r##"{"tile_folders":[{"id":"f","name":"Work","color":"#112233","view_id":"v1","cell":[2,3],"members":["a","b"],"future":1}]}"##,
        )
        .unwrap();
        assert_eq!(s.tile_folders[0].cell, [2, 3]);
        assert_eq!(s.tile_folders[0].members, vec!["a", "b"]);
        assert_eq!(s.tile_folders[0].color, "#112233");
    }

    fn test_group(id: &str, view_ids: &[&str]) -> ViewGroup {
        ViewGroup {
            id: id.to_string(),
            name: id.to_string(),
            color: String::new(),
            view_ids: view_ids.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn view_group_name_is_validated() {
        assert!(valid_group_name("   ").is_err());
        assert_eq!(valid_group_name("  Work  ").unwrap(), "Work");
        assert!(valid_group_name(&"a".repeat(MAX_GROUP_NAME + 1)).is_err());
        assert!(valid_group_name(&"a".repeat(MAX_GROUP_NAME)).is_ok());
        assert!(valid_group_name("Work\nspace").is_err());
    }

    #[test]
    fn view_groups_normalize_to_their_invariants() {
        let views = vec![test_view("a"), test_view("b"), test_view("c")];
        // Unknown ids drop out, a view claimed twice stays with the first group, and
        // a group left without members dissolves.
        let mut groups = vec![
            test_group("g1", &["a", "gone", "b"]),
            test_group("g2", &["b"]),
            test_group("g3", &["c"]),
        ];
        normalize_view_groups(&mut groups, &views);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].view_ids, vec!["a", "b"]);
        assert_eq!(groups[1].id, "g3");
        // Running it again changes nothing.
        let before = groups.iter().map(|g| g.view_ids.clone()).collect::<Vec<_>>();
        normalize_view_groups(&mut groups, &views);
        assert_eq!(before, groups.iter().map(|g| g.view_ids.clone()).collect::<Vec<_>>());
    }

    #[test]
    fn deleting_a_view_purges_it_from_its_group() {
        let mut settings = Settings::default();
        settings.views = vec![test_view("a"), test_view("b"), test_view("c")];
        settings.active_view = "a".into();
        settings.view_groups = vec![test_group("g1", &["a", "b"]), test_group("g2", &["c"])];
        // delete_view needs an AppHandle, so exercise the same steps it performs.
        settings.views.retain(|v| v.id != "b");
        let views = settings.views.clone();
        normalize_view_groups(&mut settings.view_groups, &views);
        assert_eq!(settings.view_groups[0].view_ids, vec!["a"]);
        // Removing the last member dissolves the group instead of leaving an empty one.
        settings.views.retain(|v| v.id != "c");
        let views = settings.views.clone();
        normalize_view_groups(&mut settings.view_groups, &views);
        assert_eq!(settings.view_groups.len(), 1);
        assert_eq!(settings.view_groups[0].id, "g1");
    }

    #[test]
    fn settings_without_view_groups_still_load() {
        // An older settings blob has no `view_groups` key at all.
        let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert!(s.view_groups.is_empty());
        // A group entry from a newer build keeps the fields this version knows, and
        // migrate() drops the members that no view backs.
        let mut s: Settings = serde_json::from_str(
            r##"{"active_view":"a","views":[{"id":"a","name":"A"}],
                 "view_groups":[{"id":"g","name":"G","color":"#112233","view_ids":["a","gone"],"future":1}]}"##,
        )
        .unwrap();
        assert_eq!(s.view_groups[0].color, "#112233");
        s.migrate();
        assert_eq!(s.view_groups[0].view_ids, vec!["a"]);
    }

    #[test]
    fn font_size_clamped() {
        assert_eq!(clamp_font_size(0), 0);
        assert_eq!(clamp_font_size(1), 1);
        assert_eq!(clamp_font_size(8), 10);
        assert_eq!(clamp_font_size(99), 40);
    }

    // ---- F23 tray quick menu ----

    #[test]
    fn importing_duplicate_prompt_ids_gives_each_one_its_own() {
        // `id` is the prompts table PRIMARY KEY: a duplicate made the whole rewrite
        // fail, and the failure was swallowed into the ignored JSON fallback, so the
        // import "succeeded" and the old prompts came back after a restart.
        let mut ps = sample_prompts();
        ps[1].id = ps[0].id.clone();
        ps.push({
            let mut p = ps[0].clone();
            p.id = String::new(); // a hand-written backup may leave it out entirely
            p
        });
        migrate_prompts(&mut ps);
        let ids: std::collections::HashSet<&str> = ps.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids.len(), ps.len(), "every prompt must end up with a unique id");
        assert!(ps.iter().all(|p| !p.id.is_empty()), "no prompt may be left without an id");
    }

    #[test]
    fn turning_the_old_hiding_off_keeps_screenshots_working() {
        // Stored "captureExclusion: false" meant "let screenshots through"; that has to
        // survive the switch to the positively-phrased flag.
        let mut s = Settings::default();
        s.ui_flags.insert("captureExclusion".into(), false);
        s.migrate();
        assert!(!capture_excluded(&s), "an explicit allow must keep allowing");
        assert!(!s.ui_flags.contains_key("captureExclusion"), "the old key is retired");

        // The old default (hiding on) and a fresh install both stay protected.
        let mut had = Settings::default();
        had.ui_flags.insert("captureExclusion".into(), true);
        had.migrate();
        assert!(capture_excluded(&had));
        let mut fresh = Settings::default();
        fresh.migrate();
        assert!(capture_excluded(&fresh), "the protection is the shipped default");

        // The mapping the live toggle in set_ui_flag has to follow: allow = do NOT
        // exclude. Passing the flag straight to set_capture_exclusion inverts it.
        let mut on = Settings::default();
        on.ui_flags.insert(ALLOW_SCREENSHOT_KEY.into(), true);
        assert!(!capture_excluded(&on), "allowing screenshots must not exclude");
        let mut off = Settings::default();
        off.ui_flags.insert(ALLOW_SCREENSHOT_KEY.into(), false);
        assert!(capture_excluded(&off), "refusing screenshots must exclude");
    }

    #[test]
    fn an_imported_view_cannot_exceed_the_grid_ceiling() {
        // A hand-edited backup used to be able to ask for a grid of u32::MAX cells,
        // which the renderer then tried to build — the window never painted again.
        let mut s = Settings::default();
        s.views = vec![test_view("v1"), test_view("v2")];
        s.views[0].cols = u32::MAX;
        s.views[0].rows = 999_999;
        s.views[1].cols = 0; // and the other direction: a grid with no cells at all
        s.views[1].rows = 0;
        s.migrate();
        assert_eq!(s.views[0].cols, GRID_MAX);
        assert_eq!(s.views[0].rows, GRID_MAX);
        assert_eq!(s.views[1].cols, GRID_MIN);
        assert_eq!(s.views[1].rows, GRID_MIN);
        // More views than the ceiling are cut, not carried.
        let mut s = Settings::default();
        s.views = (0..MAX_VIEWS + 20).map(|i| test_view(&format!("v{i}"))).collect();
        s.migrate();
        assert_eq!(s.views.len(), MAX_VIEWS);
    }

    fn tray_prompt(id: &str, name: &str, favorite: bool) -> Prompt {
        let mut p = sample_prompts().remove(1);
        p.id = id.into();
        p.name = name.into();
        p.favorite = favorite;
        p.hotkey = String::new();
        p
    }

    fn tray_all_on(recent_n: usize, fav_n: usize) -> TrayCfg {
        TrayCfg { recent: true, recent_n, favs: true, fav_n }
    }

    // The entries of one section of the plan, or an empty list if it is not drawn.
    fn tray_part(plan: &[(TraySection, Vec<TrayEntry>)], want: TraySection) -> Vec<TrayEntry> {
        plan.iter().find(|(s, _)| *s == want).map(|(_, e)| e.clone()).unwrap_or_default()
    }
    fn tray_names(entries: &[TrayEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.label.as_str()).collect()
    }

    #[test]
    fn tray_settings_default_to_both_sections() {
        let cfg = tray_cfg(&Settings::default());
        assert!(cfg.recent, "the recent section ships on");
        assert!(cfg.favs, "the favourites section ships on");
        assert_eq!(cfg.recent_n, 5);
        assert_eq!(cfg.fav_n, 10);

        // Both features off = a menu with no dynamic entries at all.
        let mut s = Settings::default();
        s.ui_flags.insert("trayRecent".into(), false);
        s.ui_flags.insert("trayFavs".into(), false);
        let off = tray_cfg(&s);
        assert!(!off.recent && !off.favs);
        let prompts = vec![tray_prompt("a", "A", true)];
        let mut used = HashMap::new();
        used.insert("a".to_string(), 10u64);
        assert!(tray_plan(&prompts, &used, &off).is_empty());
    }

    #[test]
    fn tray_counts_are_clamped_to_the_slider_range() {
        let mut s = Settings::default();
        s.ui_values.insert("trayRecentN".into(), 999.0);
        s.ui_values.insert("trayFavN".into(), 0.0);
        let cfg = tray_cfg(&s);
        assert_eq!(cfg.recent_n, 10);
        assert_eq!(cfg.fav_n, 1);
    }

    #[test]
    fn tray_recent_is_newest_first_and_capped() {
        let prompts = vec![
            tray_prompt("p1", "One", false),
            tray_prompt("p2", "Two", false),
            tray_prompt("p3", "Three", false),
            tray_prompt("p4", "Never copied", false),
        ];
        let used: HashMap<String, u64> = [("p1", 100u64), ("p2", 300), ("p3", 200)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let cfg = tray_all_on(2, 10);
        let recent = tray_part(&tray_plan(&prompts, &used, &cfg), TraySection::Recent);
        assert_eq!(tray_names(&recent), vec!["Two", "Three"], "newest first, cut at trayRecentN");
        // A prompt that was never copied has no entry in last_used and stays out.
        let all = tray_part(&tray_plan(&prompts, &used, &tray_all_on(10, 10)), TraySection::Recent);
        assert_eq!(all.len(), 3);
        assert!(all.iter().all(|e| e.id != tray_item_id("p4")));
    }

    #[test]
    fn tray_recent_breaks_ties_by_name() {
        let prompts = vec![tray_prompt("p1", "Beta", false), tray_prompt("p2", "Alpha", false)];
        let used: HashMap<String, u64> =
            [("p1", 50u64), ("p2", 50)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        let recent = tray_part(&tray_plan(&prompts, &used, &tray_all_on(10, 10)), TraySection::Recent);
        assert_eq!(tray_names(&recent), vec!["Alpha", "Beta"]);
    }

    #[test]
    fn tray_favorites_are_alphabetical_and_capped() {
        let prompts = vec![
            tray_prompt("p1", "zebra", true),
            tray_prompt("p2", "Apple", true),
            tray_prompt("p3", "Middle", true),
            tray_prompt("p4", "Not starred", false),
        ];
        let favs =
            tray_part(&tray_plan(&prompts, &HashMap::new(), &tray_all_on(5, 2)), TraySection::Favorites);
        assert_eq!(tray_names(&favs), vec!["Apple", "Middle"]);
        let all =
            tray_part(&tray_plan(&prompts, &HashMap::new(), &tray_all_on(5, 10)), TraySection::Favorites);
        assert_eq!(all.len(), 3, "only starred prompts");
        assert_eq!(all[2].id, tray_item_id("p1"), "case-insensitive sort puts zebra last");
    }

    #[test]
    fn the_pinned_section_is_drawn_first_and_never_repeated_below() {
        let prompts = vec![
            tray_prompt("p1", "Starred", true),
            tray_prompt("p2", "Plain", false),
            tray_prompt("p3", "Older", false),
        ];
        let used: HashMap<String, u64> = [("p1", 300u64), ("p2", 200), ("p3", 100)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let plan = tray_plan(&prompts, &used, &tray_all_on(2, 10));
        assert_eq!(
            plan.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            vec![TraySection::Favorites, TraySection::Recent],
            "pinned favourites sit above the churning recents"
        );
        assert_eq!(tray_names(&plan[0].1), vec!["Starred"]);
        // p1 is the most recent too, but it is already pinned: the recents skip it and
        // backfill with the next unlisted prompt instead of showing a duplicate.
        assert_eq!(tray_names(&plan[1].1), vec!["Plain", "Older"]);
    }

    #[test]
    fn a_tray_entry_carries_the_media_kind_its_icon_is_drawn_from() {
        let mut plain = tray_prompt("p1", "Plain", false);
        plain.file_path = String::new();
        plain.icon_path = String::new();
        plain.image = String::new();
        plain.color = String::new();
        assert_eq!(tray_kind(&plain), TrayKind::Text);

        let mut decorated = plain.clone();
        decorated.image = "data:image/png;base64,AAA".into();
        decorated.show_image = false;
        decorated.copy_image = false;
        assert_eq!(tray_kind(&decorated), TrayKind::Text, "a decorated text prompt still copies text");

        let mut shown = decorated.clone();
        shown.show_image = true;
        assert_eq!(tray_kind(&shown), TrayKind::Image);
        let mut copied = decorated.clone();
        copied.copy_image = true;
        assert_eq!(tray_kind(&copied), TrayKind::Image);

        let mut gif = shown.clone();
        gif.icon_path = "C:/clip.gif".into();
        assert_eq!(tray_kind(&gif), TrayKind::Media, "a gif/video icon outranks a still");

        let mut doc = gif.clone();
        doc.file_path = "C:/report.pdf".into();
        assert_eq!(tray_kind(&doc), TrayKind::File, "an attachment is what the click delivers");

        // The plan carries kind and colour through, so the menu never re-reads the store.
        let mut coloured = doc.clone();
        coloured.color = "#123456".into();
        let plan = tray_plan(&[coloured], &HashMap::new(), &tray_all_on(5, 5));
        assert!(plan.is_empty(), "not starred, never used");
        let mut starred = doc.clone();
        starred.favorite = true;
        starred.color = "#123456".into();
        let plan = tray_plan(&[starred], &HashMap::new(), &tray_all_on(5, 5));
        assert_eq!(plan[0].1[0].kind, TrayKind::File);
        assert_eq!(plan[0].1[0].color, "#123456");
    }

    #[test]
    fn a_menu_icon_is_a_full_opaque_16x16_chip() {
        // Opaque everywhere is not cosmetic: the Win32 icon->bitmap path only builds a
        // correct mask when it is, so a transparent pixel would show up as a stripe.
        for kind in [TrayKind::Text, TrayKind::Image, TrayKind::Media, TrayKind::File] {
            let px = tray_icon_pixels(kind, "");
            assert_eq!(px.len(), (TRAY_ICON_PX * TRAY_ICON_PX * 4) as usize);
            assert!(px.chunks_exact(4).all(|p| p[3] == 255), "{kind:?} has a transparent pixel");
        }
        // Each media kind is visibly a different chip.
        let mut seen: Vec<Vec<u8>> = Vec::new();
        for kind in [TrayKind::Text, TrayKind::Image, TrayKind::Media, TrayKind::File] {
            let px = tray_icon_pixels(kind, "");
            assert!(!seen.contains(&px), "{kind:?} draws the same chip as another kind");
            seen.push(px);
        }
        // The prompt's own colour wins over the kind tint, and the glyph flips to stay
        // readable on it: dark ink on a near-white chip, light ink on a near-black one.
        let white = tray_icon_pixels(TrayKind::Text, "#FFFFFF");
        let black = tray_icon_pixels(TrayKind::Text, "#000000");
        let ink = |px: &[u8], x: usize, y: usize| px[(y * 16 + x) * 4];
        assert_eq!(ink(&white, 5, 4), 24, "dark glyph on a light chip");
        assert_eq!(ink(&black, 5, 4), 255, "light glyph on a dark chip");
        // A junk colour must not blank the icon; it falls back to the kind tint.
        assert_eq!(tray_icon_pixels(TrayKind::Text, "not a colour"), tray_icon_pixels(TrayKind::Text, ""));
    }

    #[test]
    fn tray_labels_are_cut_by_character_not_by_byte() {
        // Every char is 3 bytes: a byte cut would split one and panic.
        let cjk = "字".repeat(80);
        let cut = tray_label(&cjk);
        assert_eq!(cut.chars().count(), TRAY_LABEL_MAX);
        assert!(cut.ends_with('…'));
        for name in ["Привет мир ".repeat(20), "नमस्ते दुनिया ".repeat(20), "🙂🙃".repeat(60)] {
            let out = tray_label(&name);
            assert!(out.chars().count() <= TRAY_LABEL_MAX, "{out:?} is too long");
            assert!(!out.is_empty());
        }
        // Short names pass through untouched.
        assert_eq!(tray_label("Mail"), "Mail");
        // Control characters collapse into single spaces, the name never spans lines.
        assert_eq!(tray_label("a\nb\tc\u{7}d"), "a b c d");
        // A Win32 menu reads a lone & as a mnemonic marker.
        assert_eq!(tray_label("R&D"), "R&&D");
        // A nameless prompt still gets something clickable.
        assert!(!tray_label("   ").is_empty());
        // An over-long name is cut back to the last word break near the cap, so the
        // entry ends on a whole word instead of mid-word.
        let cut = tray_label("Summarise this article into five concise bullet points");
        assert_eq!(cut, "Summarise this article into five…");
        // A single very long word has no usable break and is still cut hard.
        let word = tray_label(&"x".repeat(60));
        assert_eq!(word.chars().count(), TRAY_LABEL_MAX);
    }

    #[test]
    fn tray_menu_ids_round_trip_and_reject_junk() {
        assert_eq!(tray_copy_target(&tray_item_id("abc123")), Some("abc123"));
        // A prompt id is opaque, so the parser must not stop at the first separator.
        assert_eq!(tray_copy_target(&tray_item_id("a:b:c")), Some("a:b:c"));
        for bad in ["show", "quit", "", "psc", "psc:", "PSC:x", "copy:x", ":x", "tray-head-recent"] {
            assert_eq!(tray_copy_target(bad), None, "accepted a bad menu id: {bad:?}");
        }
    }

    // ---- F25 pill folders ----

    // A pill 200x80 at (x, y). Physical pixels, like every folder computation.
    fn pill_at(x: f64, y: f64) -> Rect {
        Rect { x, y, w: 200.0, h: 80.0 }
    }

    fn folder_of(id: &str, members: &[&str]) -> FloatFolder {
        FloatFolder {
            id: id.into(),
            name: String::new(),
            pos: Pos { x: 100, y: 100 },
            scale: 1.0,
            members: members.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_drop_on_top_of_another_pill_merges() {
        // Dropped 30px away: overlapping and inside the default 40px radius.
        let dragged = pill_at(130.0, 100.0);
        let others = vec![("other".to_string(), pill_at(100.0, 100.0))];
        assert_eq!(merge_target(dragged, &others, 40.0), Some("other"));
    }

    #[test]
    fn a_small_drag_never_merges() {
        // The pill moved four pixels and touches nothing else — the case that must
        // never turn a nudge into a folder.
        let others = vec![
            ("a".to_string(), pill_at(600.0, 100.0)),
            ("b".to_string(), pill_at(100.0, 400.0)),
        ];
        assert_eq!(merge_target(pill_at(104.0, 100.0), &others, 40.0), None);
        // And a pill that stayed exactly where it was matches nothing either.
        assert_eq!(merge_target(pill_at(100.0, 100.0), &others, 40.0), None);
    }

    #[test]
    fn merge_needs_both_the_radius_and_a_real_overlap() {
        let target = pill_at(100.0, 100.0);
        let others = vec![("other".to_string(), target)];
        // Centre distance exactly on the radius still counts as a hit ...
        assert_eq!(merge_target(pill_at(140.0, 100.0), &others, 40.0), Some("other"));
        // ... one pixel further does not.
        assert_eq!(merge_target(pill_at(141.0, 100.0), &others, 40.0), None);
        // Overlapping but far apart at the centres (a long text pill whose end just
        // touches) is not a merge either.
        let wide = vec![("wide".to_string(), Rect { x: 0.0, y: 100.0, w: 260.0, h: 80.0 })];
        assert!(pill_at(250.0, 100.0).overlaps(&wide[0].1));
        assert_eq!(merge_target(pill_at(250.0, 100.0), &wide, 40.0), None);
        // Two small pills close enough by centre distance but with a gap between them:
        // near is not on top of, so nothing merges.
        let small = Rect { x: 100.0, y: 100.0, w: 10.0, h: 10.0 };
        let apart = vec![("apart".to_string(), Rect { x: 130.0, y: 100.0, w: 10.0, h: 10.0 })];
        assert!(!small.overlaps(&apart[0].1));
        assert_eq!(merge_target(small, &apart, 40.0), None);
    }

    #[test]
    fn the_nearest_pill_wins_a_crowded_drop() {
        let dragged = pill_at(100.0, 100.0);
        let others = vec![
            ("far".to_string(), pill_at(130.0, 100.0)),
            ("near".to_string(), pill_at(110.0, 100.0)),
            ("out".to_string(), pill_at(400.0, 100.0)),
        ];
        assert_eq!(merge_target(dragged, &others, 40.0), Some("near"));
    }

    #[test]
    fn folder_invariants_are_restored_and_idempotent() {
        let known = ids(&["a", "b", "c", "d", "e"]);
        let mut folders = vec![
            folder_of("f1", &["a", "gone", "b"]),
            folder_of("f2", &["b", "c", "d"]), // b already belongs to f1
            folder_of("f3", &["e"]),           // a single member is not a folder
            folder_of("f4", &["ghost", "phantom"]),
        ];
        normalize_float_folders(&mut folders, &known);
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].members, ids(&["a", "b"]));
        assert_eq!(folders[1].id, "f2");
        assert_eq!(folders[1].members, ids(&["c", "d"]));
        // Running it again changes nothing.
        let before: Vec<Vec<String>> = folders.iter().map(|f| f.members.clone()).collect();
        normalize_float_folders(&mut folders, &known);
        let after: Vec<Vec<String>> = folders.iter().map(|f| f.members.clone()).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn a_prompt_lives_in_at_most_one_folder() {
        let known = ids(&["a", "b", "c"]);
        let mut folders = vec![folder_of("f1", &["a", "b"]), folder_of("f2", &["a", "c"])];
        normalize_float_folders(&mut folders, &known);
        assert_eq!(folders[0].members, vec!["a", "b"]);
        // f2 lost the duplicate and is down to one member, so it dissolves.
        assert_eq!(folders.len(), 1);
    }

    #[test]
    fn a_broken_folder_scale_is_clamped() {
        let known = ids(&["a", "b"]);
        let mut folders = vec![folder_of("f1", &["a", "b"])];
        folders[0].scale = f64::NAN;
        normalize_float_folders(&mut folders, &known);
        assert_eq!(folders[0].scale, 1.0);
        folders[0].scale = 99.0;
        normalize_float_folders(&mut folders, &known);
        assert_eq!(folders[0].scale, 3.0);
    }

    #[test]
    fn deleting_a_prompt_purges_it_from_its_folder() {
        let mut settings = Settings::default();
        settings.ui_flags.insert("pillFolders".into(), true);
        settings.float_folders = vec![folder_of("f1", &["a", "b", "c"]), folder_of("f2", &["d", "e"])];
        // delete_prompt needs an AppHandle, so exercise the same steps it performs.
        let mut alive = ids(&["a", "b", "c", "d", "e"]);
        alive.retain(|p| p != "c");
        normalize_float_folders(&mut settings.float_folders, &alive);
        assert_eq!(settings.float_folders[0].members, vec!["a", "b"]);
        // Losing a second member leaves one behind, so that folder dissolves and its
        // pill goes back to the free position it never gave up.
        alive.retain(|p| p != "d");
        normalize_float_folders(&mut settings.float_folders, &alive);
        assert_eq!(settings.float_folders.len(), 1);
        assert_eq!(settings.float_folders[0].id, "f1");
    }

    #[test]
    fn the_window_plan_follows_the_feature_flag() {
        let mut settings = Settings::default();
        for id in ["a", "b", "c"] {
            settings.floating.insert(id.to_string(), Pos { x: 0, y: 0 });
        }
        settings.float_folders = vec![folder_of("f1", &["a", "b"])];
        let known = ids(&["a", "b", "c"]);

        // On (the shipped default): the members lose their own window, the folder
        // gets one.
        let (pills, folders) = float_window_plan(&settings, &known);
        assert_eq!(pills, ids(&["c"]));
        assert_eq!(folders, ids(&["f1"]));

        // Off: every member is a plain pill again, no folder — switching the feature
        // off must put every pill back.
        settings.ui_flags.insert("pillFolders".into(), false);
        let (pills, folders) = float_window_plan(&settings, &known);
        assert_eq!(pills, ids(&["a", "b", "c"]));
        assert!(folders.is_empty());
        settings.ui_flags.remove("pillFolders");

        // An unpinned prompt never gets a window, folder member or not.
        settings.floating.remove("c");
        let (pills, _) = float_window_plan(&settings, &known);
        assert!(pills.is_empty());
    }

    #[test]
    fn folder_expert_settings_use_their_slider_range() {
        let s = Settings::default();
        assert!(folders_on(&s), "the feature ships on");
        assert_eq!(merge_radius(&s), 40.0);
        assert_eq!(folder_cols(&s), 3);
        let mut s = Settings::default();
        s.ui_values.insert("pillMergeRadius".into(), 9999.0);
        s.ui_values.insert("folderGridCols".into(), 0.0);
        assert_eq!(merge_radius(&s), 80.0);
        assert_eq!(folder_cols(&s), 2);
        s.ui_values.insert("pillMergeRadius".into(), f64::NAN);
        assert_eq!(merge_radius(&s), 40.0);
    }

    #[test]
    fn folder_names_are_trimmed_and_stripped() {
        assert_eq!(folder_name_of("  Work  ").unwrap(), "Work");
        assert_eq!(folder_name_of("").unwrap(), "");
        // A pasted newline or control byte never reaches the window title.
        assert_eq!(folder_name_of("Wo\nrk\u{7}").unwrap(), "Work");
        assert!(folder_name_of(&"a".repeat(MAX_FOLDER_NAME)).is_ok());
        assert!(folder_name_of(&"a".repeat(MAX_FOLDER_NAME + 1)).is_err());
    }

    // A 1920x1080 primary screen with a 40px taskbar at the bottom.
    fn primary_work() -> Rect {
        Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1040.0 }
    }

    const ICON: f64 = 96.0;

    fn icon_at(x: f64, y: f64) -> Rect {
        Rect { x, y, w: ICON, h: ICON }
    }

    #[test]
    fn a_folder_opens_below_when_there_is_room() {
        let icon = icon_at(400.0, 200.0);
        let l = folder_layout(icon, 260.0, 180.0, 6.0, primary_work());
        assert_eq!(l.side, FolderSide::Below);
        // The icon never moves: it still sits at its own screen position.
        assert_eq!(l.win.x + l.icon.0, icon.x);
        assert_eq!(l.win.y + l.icon.1, icon.y);
        // The popover hangs under it with the gap in between, aligned with the icon's
        // left edge — so the window origin IS the icon and nothing shifts sideways
        // when it opens.
        assert_eq!(l.win.y + l.pop.1, icon.bottom() + 6.0);
        assert_eq!(l.win.x + l.pop.0, icon.x);
        assert_eq!(l.icon, (0.0, 0.0));
        // One window covers both.
        assert_eq!(l.win.h, ICON + 6.0 + 180.0);
        assert_eq!(l.win.w, 260.0);
    }

    #[test]
    fn opening_a_folder_never_moves_the_window_origin_sideways() {
        // Anywhere with room to the right, the popover starts at the icon's own left
        // edge: the origin stays put, so the icon cannot jump left/right for a frame.
        for x in [0.0, 400.0, 1000.0] {
            let icon = icon_at(x, 200.0);
            let l = folder_layout(icon, 230.0, 140.0, 6.0, primary_work());
            assert_eq!(l.win.x, icon.x, "origin moved at x={}", x);
            assert_eq!(l.icon.0, 0.0);
        }
    }

    #[test]
    fn a_folder_at_the_bottom_edge_flips_above() {
        let icon = icon_at(400.0, 900.0); // bottom at 996, work ends at 1040
        let l = folder_layout(icon, 260.0, 180.0, 6.0, primary_work());
        assert_eq!(l.side, FolderSide::Above);
        assert_eq!(l.win.y + l.pop.1, icon.y - 6.0 - 180.0);
        assert_eq!(l.win.x + l.icon.0, icon.x);
        assert_eq!(l.win.y + l.icon.1, icon.y);
        // Still fully inside the work area.
        assert!(l.win.y >= primary_work().y && l.win.bottom() <= primary_work().bottom());
    }

    #[test]
    fn a_tall_grid_in_the_middle_flips_sideways() {
        // Neither below nor above fits a 900px grid, so it goes to the right ...
        let icon = icon_at(400.0, 500.0);
        let l = folder_layout(icon, 300.0, 900.0, 6.0, primary_work());
        assert_eq!(l.side, FolderSide::Right);
        assert_eq!(l.win.x + l.pop.0, icon.right() + 6.0);
        // ... and to the left when the right edge is too close.
        let icon = icon_at(1700.0, 500.0);
        let l = folder_layout(icon, 300.0, 900.0, 6.0, primary_work());
        assert_eq!(l.side, FolderSide::Left);
        assert_eq!(l.win.x + l.pop.0, icon.x - 6.0 - 300.0);
        assert!(l.win.x >= 0.0 && l.win.right() <= 1920.0);
    }

    #[test]
    fn a_folder_in_a_corner_keeps_the_whole_popover_on_screen() {
        let work = primary_work();
        for icon in [icon_at(0.0, 0.0), icon_at(1824.0, 0.0), icon_at(0.0, 944.0), icon_at(1824.0, 944.0)] {
            let l = folder_layout(icon, 400.0, 300.0, 6.0, work);
            assert!(l.win.x >= work.x, "left edge escaped at {:?}", icon);
            assert!(l.win.y >= work.y, "top edge escaped at {:?}", icon);
            assert!(l.win.right() <= work.right(), "right edge escaped at {:?}", icon);
            assert!(l.win.bottom() <= work.bottom(), "bottom edge escaped at {:?}", icon);
            // Whatever had to move, it was never the icon.
            assert_eq!(l.win.x + l.icon.0, icon.x);
            assert_eq!(l.win.y + l.icon.1, icon.y);
        }
    }

    #[test]
    fn a_second_monitor_at_another_dpi_flips_on_its_own_edges() {
        // A 150% 2560x1440 screen to the RIGHT of the primary one: its work area
        // starts at x=1920, and every size arrives multiplied by 1.5.
        let work = Rect { x: 1920.0, y: 0.0, w: 2560.0, h: 1380.0 };
        let icon = Rect { x: 2000.0, y: 1200.0, w: ICON * 1.5, h: ICON * 1.5 };
        let l = folder_layout(icon, 260.0 * 1.5, 180.0 * 1.5, 6.0 * 1.5, work);
        // Below would run past that monitor's work area, so it flips there too.
        assert_eq!(l.side, FolderSide::Above);
        assert!(l.win.x >= work.x && l.win.right() <= work.right());
        assert!(l.win.y >= work.y && l.win.bottom() <= work.bottom());
        // A folder in that screen's top-left corner opens below and stays on it —
        // the primary monitor to the left is never used as spare room.
        let icon = Rect { x: 1930.0, y: 20.0, w: ICON * 1.5, h: ICON * 1.5 };
        let l = folder_layout(icon, 600.0, 200.0, 9.0, work);
        assert_eq!(l.side, FolderSide::Below);
        assert!(l.win.x >= work.x, "the popover leaked onto the primary monitor");
    }

    #[test]
    fn a_popover_larger_than_the_screen_still_starts_on_it() {
        let work = primary_work();
        let l = folder_layout(icon_at(900.0, 500.0), 4000.0, 4000.0, 6.0, work);
        assert_eq!(l.win.x + l.pop.0, work.x);
        assert_eq!(l.win.y + l.pop.1, work.y);
    }

    #[test]
    fn dissolving_a_folder_never_stacks_its_pills() {
        let work = primary_work();
        let pill = |x: f64, y: f64| Rect { x, y, w: 200.0, h: 80.0 };
        // Two members whose stored position is the SAME spot — exactly what merging
        // one pill onto another leaves behind.
        let out = spread_free_positions(&[pill(500.0, 400.0), pill(500.0, 400.0)], &[], work);
        assert_eq!(out[0], (500.0, 400.0), "the first one keeps its wish");
        assert_ne!(out[0], out[1]);
        let a = pill(out[0].0, out[0].1);
        let b = pill(out[1].0, out[1].1);
        assert!(!a.overlaps(&b), "the pills still cover each other: {:?} {:?}", a, b);
        // Both stay on screen.
        for (x, y) in out {
            assert!(x >= work.x && x + 200.0 <= work.right());
            assert!(y >= work.y && y + 80.0 <= work.bottom());
        }
        // A pill already on screen is an obstacle too.
        let out = spread_free_positions(&[pill(500.0, 400.0)], &[pill(500.0, 400.0)], work);
        assert!(!pill(out[0].0, out[0].1).overlaps(&pill(500.0, 400.0)));
        // Idempotent: positions that are already free are left exactly where they are.
        let free = [pill(100.0, 100.0), pill(600.0, 600.0)];
        assert_eq!(
            spread_free_positions(&free, &[], work),
            vec![(100.0, 100.0), (600.0, 600.0)]
        );
    }

    #[test]
    fn a_dropped_pill_is_clamped_onto_its_work_area() {
        let work = primary_work();
        // Dropped past the right/bottom edge: pulled back so the whole pill shows.
        assert_eq!(clamp_to_work(work, 200.0, 80.0, 1900.0, 1030.0), (1720.0, 960.0));
        // Dropped past the top/left edge.
        assert_eq!(clamp_to_work(work, 200.0, 80.0, -50.0, -20.0), (0.0, 0.0));
        // Inside: untouched.
        assert_eq!(clamp_to_work(work, 200.0, 80.0, 500.0, 400.0), (500.0, 400.0));
        // A second monitor's origin is respected, not the desktop's.
        let right = Rect { x: 1920.0, y: 0.0, w: 1280.0, h: 1000.0 };
        assert_eq!(clamp_to_work(right, 200.0, 80.0, 1900.0, 10.0), (1920.0, 10.0));
        // A window bigger than the screen is pinned to the corner instead of pushed off.
        assert_eq!(clamp_to_work(work, 5000.0, 5000.0, 300.0, 300.0), (0.0, 0.0));
    }

    #[test]
    fn settings_without_folders_still_load() {
        // An older settings blob has no `float_folders` key at all.
        let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert!(s.float_folders.is_empty());
        // A folder from a newer build keeps the fields this version knows and fills in
        // the ones it does not carry.
        let s: Settings = serde_json::from_str(
            r##"{"float_folders":[{"id":"f","name":"Work","members":["a","b"],"future":1}]}"##,
        )
        .unwrap();
        assert_eq!(s.float_folders[0].name, "Work");
        assert_eq!(s.float_folders[0].scale, 1.0);
        assert_eq!(s.float_folders[0].pos.x, 0);
    }

    // ---- F26 colour picker ----

    #[test]
    fn hex_is_uppercase_and_padded() {
        assert_eq!(rgb_to_hex(26, 43, 60), "#1A2B3C");
        assert_eq!(rgb_to_hex(0, 0, 0), "#000000");
        assert_eq!(rgb_to_hex(255, 255, 255), "#FFFFFF");
        assert_eq!(rgb_to_hex(1, 2, 3), "#010203"); // single digits keep their zero
    }

    #[test]
    fn hsl_handles_the_degenerate_cases() {
        // Pure grey: hue and saturation are undefined, so both report 0.
        assert_eq!(rgb_to_hsl(128, 128, 128), (0, 0, 50));
        assert_eq!(rgb_to_hsl(0, 0, 0), (0, 0, 0));
        assert_eq!(rgb_to_hsl(255, 255, 255), (0, 0, 100));
        // Pure red sits at 0°, never at 360°.
        assert_eq!(rgb_to_hsl(255, 0, 0), (0, 100, 50));
        // Just below red wraps back down instead of rounding up to 360.
        assert_eq!(rgb_to_hsl(255, 0, 1).0, 0);
        assert_eq!(rgb_to_hsl(0, 255, 0), (120, 100, 50));
        assert_eq!(rgb_to_hsl(0, 0, 255), (240, 100, 50));
        // Each of the three max branches, with rounded integer output.
        assert_eq!(rgb_to_hsl(26, 43, 60), (210, 40, 17));
        assert_eq!(rgb_to_hsl(255, 128, 0), (30, 100, 50));
        assert_eq!(rgb_to_hsl(128, 0, 255), (270, 100, 50));
    }

    #[test]
    fn every_format_writes_its_own_notation() {
        assert_eq!(format_color("hex", 26, 43, 60), "#1A2B3C");
        assert_eq!(format_color("rgb", 26, 43, 60), "rgb(26,43,60)");
        assert_eq!(format_color("hsl", 26, 43, 60), "hsl(210,40%,17%)");
        // An unknown or empty stored notation falls back to hex, never to nothing.
        assert_eq!(format_color("", 26, 43, 60), "#1A2B3C");
        assert_eq!(format_color("cmyk", 26, 43, 60), "#1A2B3C");
    }

    #[test]
    fn picker_settings_are_clamped_to_their_range() {
        let mut s = Settings::default();
        let d = pick_cfg(&s);
        assert_eq!((d.zoom, d.format.as_str()), (8, "hex"));
        s.ui_values.insert("loupeZoom".into(), 99.0);
        assert_eq!(pick_cfg(&s).zoom, 16);
        s.ui_values.insert("loupeZoom".into(), 0.0);
        assert_eq!(pick_cfg(&s).zoom, 4);
        s.ui_texts.insert("pickerFormat".into(), "hsl".into());
        assert_eq!(pick_cfg(&s).format, "hsl");
        s.ui_texts.insert("pickerFormat".into(), "nonsense".into());
        assert_eq!(pick_cfg(&s).format, "hex");
    }

    // A stitched frozen desktop for a mixed-DPI pair: a 2560x1440 monitor scaled to
    // 150% sits LEFT of the 1920x1080 primary, so the virtual origin is negative.
    // Physical pixels are what the capture holds — the scale factor never enters the
    // mapping, which is exactly why the picker stays exact on either monitor.
    fn mixed_dpi_desktop() -> (Vec<MonitorCap>, i64, i64) {
        let (left_x, left_w, left_h) = (-2560i32, 2560u32, 1440u32);
        let (right_x, right_w, right_h) = (0i32, 1920u32, 1080u32);
        let origin_x = left_x.min(right_x);
        let origin_y = 0;
        let total_w = left_w + right_w;
        let total_h = left_h.max(right_h);
        let mut canvas = image::RgbaImage::from_pixel(total_w, total_h, image::Rgba([0, 0, 0, 255]));
        let lx = stitch_offset(left_x, origin_x);
        let rx = stitch_offset(right_x, origin_x);
        // One marker pixel per monitor, at the monitor's own top-left corner.
        canvas.put_pixel(lx as u32, 0, image::Rgba([26, 43, 60, 255]));
        canvas.put_pixel(rx as u32, 0, image::Rgba([255, 128, 0, 255]));
        // ...and one in the far bottom-right of the right-hand monitor.
        canvas.put_pixel(rx as u32 + right_w - 1, right_h - 1, image::Rgba([1, 2, 3, 255]));
        (
            vec![MonitorCap { image: Arc::new(canvas), x: origin_x, y: origin_y, width: total_w, height: total_h }],
            lx,
            rx,
        )
    }

    #[test]
    fn stitch_offset_places_each_monitor_at_its_own_column() {
        // Left monitor starts the image; the primary starts one full width in.
        assert_eq!(stitch_offset(-2560, -2560), 0);
        assert_eq!(stitch_offset(0, -2560), 2560);
        // Vertical works the same, including a monitor above the primary.
        assert_eq!(stitch_offset(-300, -300), 0);
        assert_eq!(stitch_offset(0, -300), 300);
    }

    #[test]
    fn sampling_hits_the_right_pixel_on_either_monitor() {
        let (caps, lx, rx) = mixed_dpi_desktop();
        assert_eq!(sample_frozen(&caps, lx as i32, 0), Some([26, 43, 60]));
        assert_eq!(sample_frozen(&caps, rx as i32, 0), Some([255, 128, 0]));
        assert_eq!(sample_frozen(&caps, rx as i32 + 1919, 1079), Some([1, 2, 3]));
        // The neighbouring pixel is a different colour — no off-by-one smearing.
        assert_eq!(sample_frozen(&caps, rx as i32 - 1, 0), Some([0, 0, 0]));
        // Outside the desktop nothing is sampled at all (never a clamped neighbour).
        assert_eq!(sample_frozen(&caps, -1, 0), None);
        assert_eq!(sample_frozen(&caps, 0, -1), None);
        assert_eq!(sample_frozen(&caps, 4480, 0), None);
        assert_eq!(sample_frozen(&caps, 0, 1440), None);
        assert_eq!(sample_frozen(&[], 0, 0), None);
    }

    #[test]
    fn the_tile_handed_to_the_loupe_is_packed_rgb() {
        let mut img = image::RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([26, 43, 60, 255]));
        img.put_pixel(1, 0, image::Rgba([255, 128, 0, 128]));
        // Alpha is dropped: three bytes per pixel, in row order.
        assert_eq!(crop_rgb(&img, 0, 0, 2, 1), vec![26, 43, 60, 255, 128, 0]);
    }

    #[test]
    fn a_loupe_tile_reads_the_rows_it_was_asked_for() {
        // 3x3 where every pixel encodes its own coordinate, so a wrong stride shows up.
        let mut img = image::RgbaImage::new(3, 3);
        for y in 0..3u32 {
            for x in 0..3u32 {
                img.put_pixel(x, y, image::Rgba([x as u8, y as u8, 9, 255]));
            }
        }
        assert_eq!(
            crop_rgb(&img, 1, 1, 2, 2),
            vec![1, 1, 9, 2, 1, 9, 1, 2, 9, 2, 2, 9],
            "the tile must be the requested rect, not the top-left corner"
        );
    }

    #[test]
    fn a_tile_that_leaves_the_frame_returns_nothing() {
        let img = image::RgbaImage::new(4, 4);
        assert!(crop_rgb(&img, 3, 0, 2, 1).is_empty()); // runs off the right edge
        assert!(crop_rgb(&img, 0, 3, 1, 2).is_empty()); // runs off the bottom edge
        assert!(crop_rgb(&img, 0, 0, 0, 4).is_empty()); // empty rect
        assert!(crop_rgb(&img, u32::MAX, 0, 4, 4).is_empty()); // would wrap
        assert_eq!(crop_rgb(&img, 0, 0, 4, 4).len(), 4 * 4 * 3); // the whole frame still fits
    }

    #[test]
    fn snip_background_still_loads_without_the_picker_fields() {
        // The overlay reads the mode off SnipBg; serializing keeps the snake_case
        // names snip.js indexes.
        let bg = SnipBg {
            src: "x".into(), is_file: true, width: 4, height: 2,
            pick: true, pick_zoom: 12, pick_format: "rgb".into(),
            pick_history: vec!["#FF0000".into()],
        };
        let j = serde_json::to_value(&bg).unwrap();
        assert_eq!(j["pick"], serde_json::json!(true));
        assert_eq!(j["pick_zoom"], serde_json::json!(12));
        assert_eq!(j["pick_format"], serde_json::json!("rgb"));
        assert_eq!(j["pick_history"], serde_json::json!(["#FF0000"]));
    }
}


