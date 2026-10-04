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

// Bring the main window back from the tray: always at the saved size/position.
// Routed through resolve_geometry() (the same validation the startup path
// uses) instead of trusting the saved geometry raw -- a monitor unplugged
// while the app sat minimized in the tray must not be able to restore the
// window off every remaining screen. A window that is simply gone (the process
// kept running with zero top-level windows, the never-comes-back state a
// crashed-out webview or a bug can leave behind) is rebuilt first instead of
// leaving the call a silent no-op.
pub(crate) fn show_main(app: &AppHandle) {
    let win = show_main_seam(app.get_webview_window("main"), || build_main_window(app));
    let Some(win) = win else { return };
    if let Some(state) = app.try_state::<Db>() {
        let geom = state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .settings
            .window;
        let g = resolve_geometry(&win, geom);
        let _ = win.set_size(tauri::LogicalSize::new(g.width, g.height));
        let _ = win.set_position(PhysicalPosition::new(g.x, g.y));
    }
    let _ = win.show();
    let _ = win.unminimize();
    let _ = win.set_focus();
}

// The decision `show_main` runs on: reuse the window it already has, or ask
// for a fresh one. Pulled apart from `show_main` itself so a test can hand it
// a counter instead of a real window and prove the missing case actually
// triggers a build rather than doing nothing (the bug this whole point fixes).
pub(crate) fn show_main_seam<W>(existing: Option<W>, build: impl FnOnce() -> tauri::Result<W>) -> Option<W> {
    match existing {
        Some(w) => Some(w),
        None => match build() {
            Ok(w) => Some(w),
            Err(e) => {
                append_crash_log(&format!("show_main: could not rebuild the main window: {e}"));
                None
            }
        },
    }
}

// The one place the main window is built and wired up -- used by setup() for
// the window Tauri already created from tauri.conf.json at normal startup, and
// by show_main() to replace one that was destroyed out from under a process
// that kept running. Same label, same starting geometry, same decorations and
// the same event handlers either way, so a rebuilt window behaves exactly like
// the one the app started with.
pub(crate) fn build_main_window(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let win = match app.get_webview_window("main") {
        Some(w) => w,
        None => WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title(brand::DISPLAY_NAME)
            .inner_size(880.0, 600.0)
            .min_inner_size(f64::from(MAIN_WIN_MIN_W), f64::from(MAIN_WIN_MIN_H))
            .resizable(true)
            .decorations(false)
            .transparent(true)
            .visible(false)
            .build()?,
    };
    let settings = app
        .try_state::<Db>()
        .map(|s| lock(&s).settings.clone())
        .unwrap_or_default();
    set_capture_exclusion(&win, capture_excluded(&settings));
    if settings.always_on_top {
        let _ = win.set_always_on_top(true);
    }
    // Without this the frameless, transparent window shows through until the
    // frontend paints -- a rebuilt window has to arrive dressed the same way
    // the one created at startup does.
    {
        let eff = effective_theme(app, &theme_base_of(&settings, &settings.theme));
        let vars = theme_vars_of(&settings);
        apply_window_bg(app, &eff, vars.get("bg").map(|s| s.as_str()));
    }
    #[cfg(windows)]
    if let Ok(hwnd) = win.hwnd() {
        sessionend::hook(app, hwnd.0 as isize);
    }
    let handle2 = app.clone();
    win.on_window_event(move |event| match event {
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
                // Store LOGICAL pixels: physical values re-scaled
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
                // Hide instead of quitting; a hidden window never keeps listening.
                voice_shutdown(&handle2);
                if let Some(w) = handle2.get_webview_window("main") {
                    let _ = w.hide();
                }
            } else {
                // Never vanish while an export, conversion, ffmpeg
                // download, backup or import is still going --
                // request_app_quit asks the main window first,
                // exactly once per quit attempt (QUIT_CONFIRMED
                // short-circuits the second CloseRequested that
                // confirm_quit_with_running_ops itself triggers).
                api.prevent_close();
                request_app_quit(&handle2);
            }
        }
        WindowEvent::Destroyed => {
            if let Some(state) = handle2.try_state::<Db>() {
                let store = state.lock().unwrap_or_else(|e| e.into_inner());
                save_settings(&handle2, &store.settings);
            }
            voice_shutdown(&handle2);
            // No process without a main window: end this one right away, and
            // back that up with a watchdog in case exit() itself never
            // completes (a stuck shutdown handler, for instance) -- the same
            // silent-zombie shape the bug this guards against started from.
            handle2.exit(0);
            let deadline = SystemTime::now() + WATCHDOG_GRACE;
            let hard_deadline = SystemTime::now() + WATCHDOG_JOB_GRACE;
            std::thread::spawn(move || loop {
                if watchdog_deadline_reached(SystemTime::now(), deadline) {
                    // A finishing export, conversion or backup is given the
                    // longer grace: killing it here would leave a half-written
                    // file behind, which is worse than a few more seconds of a
                    // windowless process.
                    let busy = bg_ops_running_count() > 0
                        && !watchdog_deadline_reached(SystemTime::now(), hard_deadline);
                    if !busy {
                        append_crash_log("watchdog: forced the process to end after the main window closed");
                        std::process::exit(0);
                    }
                }
                std::thread::sleep(Duration::from_millis(200));
            });
        }
        _ => {}
    });
    Ok(win)
}

// How long the Destroyed-event watchdog gives handle.exit(0) to actually end
// the process before forcing it.
pub(crate) const WATCHDOG_GRACE: Duration = Duration::from_secs(5);

// The longer ceiling used while a background job is still finishing, so the
// forced exit never truncates an export, conversion or backup mid-write.
pub(crate) const WATCHDOG_JOB_GRACE: Duration = Duration::from_secs(120);

// Pure decision for the watchdog thread above -- kept apart from the sleep
// loop so a test can drive it with fixed instants instead of a real 5-second
// wait.
pub(crate) fn watchdog_deadline_reached(now: SystemTime, deadline: SystemTime) -> bool {
    now >= deadline
}

// Global-hotkey action: open the standalone quick-launcher (fuzzy search,
// instant copy) instead of raising the main window. Replaces any launcher already
// open, so a second hotkey press never leaves two fighting over the same focus.
pub(crate) fn summon_launcher(app: &AppHandle) {
    LAUNCHER_PREFILL.lock().unwrap_or_else(|e| e.into_inner()).clear();
    open_launcher_window(app, false);
}

// The launcher window itself, shared by the hotkey and by the tray filter row.
// `deferred` builds it hidden and leaves the focus where it is: the tray hand-off keeps
// the typist in the tray's own field until the launcher page has loaded (it then calls
// launcher_ready), because a keystroke sent to a launcher whose page is not there yet is
// lost. A launcher that never reports in is revealed after LAUNCHER_REVEAL_FALLBACK anyway.
pub(crate) fn open_launcher_window(app: &AppHandle, deferred: bool) {
    if let Some(win) = app.get_webview_window(LAUNCHER_WIN_LABEL) {
        let _ = win.close();
    }
    let gen = LAUNCHER_GEN.fetch_add(1, Ordering::SeqCst) + 1;
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
        .focused(!deferred) // a hidden window built focused still takes the focus from the tray
        .visible(false) // sized in physical pixels first, so nothing flashes at the wrong size
        .build()
    {
        Ok(w) => w,
        Err(_) => return,
    };
    let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    set_capture_exclusion(&win, excluded);
    set_bounds_phys(&win, rect);
    if !deferred {
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let handle = app.clone();
    let _ = std::thread::Builder::new().name("launcher-reveal".into()).spawn(move || {
        std::thread::sleep(LAUNCHER_REVEAL_FALLBACK);
        if LAUNCHER_GEN.load(Ordering::SeqCst) == gen {
            reveal_launcher(&handle);
        }
    });
}

// Shows the launcher and gives it the focus, unless it already is on screen.
pub(crate) fn reveal_launcher(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(LAUNCHER_WIN_LABEL) {
        if !win.is_visible().unwrap_or(false) {
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

// ---------- Global shortcuts: the app hotkey + one per prompt ----------

pub(crate) type Accel = tauri_plugin_global_shortcut::Shortcut;

// An accelerator is user input on its way to the OS shortcut API, so its length is
// capped before anything else looks at it.
pub(crate) const MAX_ACCEL_LEN: usize = 64;

// Rejection codes. The UI turns them into a localized message; they are the whole
// vocabulary the hotkey path speaks.
pub(crate) const HK_INVALID: &str = "hotkey-invalid"; // unusable shape (no Ctrl/Alt, Win key, garbage)
pub(crate) const HK_SYSTEM: &str = "hotkey-system"; // Windows or every app owns it
pub(crate) const HK_TAKEN: &str = "hotkey-taken"; // already bound inside this app
pub(crate) const HK_CONFLICT: &str = "hotkey-conflict"; // another running app holds the registration

// Keys a global hotkey may never take, whatever the modifiers are. A registered
// accelerator SWALLOWS the keystroke system-wide, so these would break the machine:
// Tab/Escape/Enter are the universal navigation + confirm/cancel keys, Backspace
// and Delete are word-wise text editing (Ctrl+Backspace), Insert carries the
// classic clipboard aliases (Ctrl+Insert / Shift+Insert), Space is the IME switch
// on CJK systems and the window menu with Alt, PrintScreen is screen capture, and
// the lock keys are hardware state. This one list also covers Ctrl+Alt+Del,
// Ctrl+Shift+Esc, Ctrl+Esc, Alt+Tab and Alt+Space without naming them.
pub(crate) const RESERVED_KEYS: &[Code] = &[
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
// plain Ctrl form is blocked: adding Shift or Alt makes a distinct combination that
// no longer belongs to anyone in particular, and that is the namespace users pick
// their own shortcuts from.
pub(crate) const UNIVERSAL_CTRL: &[Code] = &[
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
pub(crate) fn hotkey_block_reason(sc: &Accel) -> Option<&'static str> {
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
pub(crate) fn accel_error(raw: &str) -> Result<Accel, &'static str> {
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
pub(crate) fn parse_accel(raw: &str) -> Option<Accel> {
    accel_error(raw).ok()
}

// Which feature a registered accelerator belongs to. A plain `Option<String>`
// (None = app hotkey) stopped being enough once a THIRD kind of global hotkey
// (131, voice dictation) needed its own owner that is neither the app hotkey nor
// a prompt -- a sentinel prompt id would silently pass as a real Prompt(id) in
// rearm_hotkeys and the handler below, so a real enum replaces it everywhere.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum HotkeyOwner {
    App,
    Prompt(String),
    Voice,
    // Registered only while a recording is running (rearm_hotkeys checks
    // RecPhase), so a stop/pause combination never steals input from another
    // app the rest of the time.
    RecStop,
    RecPause,
}

// Every accelerator that should be registered, in registration order. Unusable
// accelerators and combinations an earlier entry already claimed are skipped: the
// app hotkey wins, then voice dictation, then the two recording controls, then
// prompts in list order. `per_prompt` is the promptHotkeys feature toggle -- off
// leaves the stored accelerators alone and simply registers none of them.
// `voice_hotkey` is only ever Some when both the `voice` and `voiceHotkey` flags
// are on; `rec_stop_hotkey`/`rec_pause_hotkey` only ever Some while a recording
// is actually running AND their own toggle is on -- both already-resolved
// choices, not re-checked here.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hotkey_plan(
    app_hotkey: &str,
    voice_hotkey: Option<&str>,
    rec_stop_hotkey: Option<&str>,
    rec_pause_hotkey: Option<&str>,
    prompts: &[Prompt],
    per_prompt: bool,
) -> Vec<(Accel, HotkeyOwner)> {
    let mut plan = Vec::new();
    let mut taken: std::collections::HashSet<Accel> = std::collections::HashSet::new();
    if let Some(sc) = parse_accel(app_hotkey) {
        taken.insert(sc);
        plan.push((sc, HotkeyOwner::App));
    }
    if let Some(vh) = voice_hotkey {
        if let Some(sc) = parse_accel(vh) {
            if taken.insert(sc) {
                plan.push((sc, HotkeyOwner::Voice));
            }
        }
    }
    for (hk, owner) in [(rec_stop_hotkey, HotkeyOwner::RecStop), (rec_pause_hotkey, HotkeyOwner::RecPause)] {
        if let Some(hk) = hk {
            if let Some(sc) = parse_accel(hk) {
                if taken.insert(sc) {
                    plan.push((sc, owner));
                }
            }
        }
    }
    if per_prompt {
        for p in prompts {
            if let Some(sc) = parse_accel(&p.hotkey) {
                if taken.insert(sc) {
                    plan.push((sc, HotkeyOwner::Prompt(p.id.clone())));
                }
            }
        }
    }
    plan
}

// A restored backup can carry an accelerator the app hotkey or an earlier prompt
// already owns. The import must not fail over that, so the losing binding is
// dropped and counted for the summary.
pub(crate) fn drop_colliding_hotkeys(app_hotkey: &str, prompts: &mut [Prompt]) -> usize {
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
// by the UI. rec_stop_hotkey/rec_pause_hotkey are only checked while their own
// feature is on -- setting a prompt hotkey to the same combination used to
// go through fine, then silently stop firing for the whole duration of every
// future recording, because hotkey_plan gives the recording controls priority.
// Checking here closes that loophole before it can be created.
pub(crate) fn sanitize_hotkey(
    raw: &str,
    app_hotkey: &str,
    rec_stop_hotkey: &str,
    rec_pause_hotkey: &str,
    others: &[&Prompt],
) -> Result<String, String> {
    let hk = raw.trim();
    if hk.is_empty() {
        return Ok(String::new());
    }
    let sc = accel_error(hk).map_err(|e| e.to_string())?;
    if parse_accel(app_hotkey) == Some(sc)
        || parse_accel(rec_stop_hotkey) == Some(sc)
        || parse_accel(rec_pause_hotkey) == Some(sc)
        || others.iter().any(|p| parse_accel(&p.hotkey) == Some(sc))
    {
        return Err(HK_TAKEN.to_string());
    }
    Ok(hk.to_string())
}

// The rec-stop/rec-pause accelerator to check prompt hotkeys against, or ""
// when that control's own feature/hotkey toggle is off and the combination is
// therefore free to reuse. Mirrors rearm_hotkeys' own gating minus
// recording_active() -- a prompt hotkey is set long before any recording runs.
pub(crate) fn active_rec_hotkey<'a>(hk: &'a str, flag_key: &str, flags: &HashMap<String, bool>) -> &'a str {
    if flags.get("screenRecord") != Some(&false) && flags.get(flag_key) != Some(&false) {
        hk
    } else {
        ""
    }
}

// What a prompt hotkey does after copying. Empty = follow the global setting, so a
// stored value is only ever one of three states.
pub(crate) fn sanitize_hotkey_mode(raw: &str) -> String {
    match raw.trim() {
        "copy" => "copy".to_string(),
        "paste" => "paste".to_string(),
        _ => String::new(),
    }
}

// Which feature each registered accelerator belongs to. Rebuilt by
// rearm_hotkeys, read by the global shortcut handler.
pub(crate) struct HotkeyTargets(pub(crate) Mutex<Vec<(Accel, HotkeyOwner)>>);

// The single owner of the registered shortcut set: drop everything, then register
// the app hotkey plus one accelerator per prompt. A combination the OS refuses is
// owned by another app: it is cleared from the store instead of leaving a binding
// the user believes in. Returns false when the APP hotkey was the refused one.
// Must never run while the store mutex is held: registering hops to the main
// thread, and the shortcut handler reads the store.
// Whether a recording is currently in flight (Recording or Paused) --
// both configs of RecordingState just wrap an Option, so this needs no
// windows-only gating of its own.
pub(crate) fn recording_active(app: &AppHandle) -> bool {
    app.try_state::<RecordingState>()
        .map(|s| s.0.lock().unwrap_or_else(|e| e.into_inner()).is_some())
        .unwrap_or(false)
}

// The running session's phase, for the two callers that need to tell Paused
// from Recording (the tray menu's Pause/Resume label) rather than just
// active-or-not. Windows-only build carries a real RecSession with a phase; the
// non-Windows stub's Option<()> has none, so it always reports "not recording".
#[cfg(windows)]
pub(crate) fn recording_phase(app: &AppHandle) -> Option<RecPhase> {
    app.try_state::<RecordingState>()?.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.phase)
}
#[cfg(not(windows))]
pub(crate) fn recording_phase(_app: &AppHandle) -> Option<RecPhase> {
    None
}

// Whether the last rearm_hotkeys() actually got the stop/pause combination
// registered -- read by recording_status() so the bar can warn instead of the
// user believing in a binding that silently never fires. Unlike the app/voice
// hotkey, a conflict here never clears the stored default: the default ships
// pre-picked, and a transient collision (another app briefly holding the same
// combination) should not permanently blank it out from under the user.
pub(crate) static REC_STOP_HOTKEY_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
pub(crate) static REC_PAUSE_HOTKEY_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub(crate) fn rearm_hotkeys(app: &AppHandle) -> bool {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let rec_active = recording_active(app);
    // A binding stored by an older version can be one the blocklist now refuses.
    // It is never registered again, so the stored string is cleared here: otherwise
    // the editor keeps showing a hotkey that does nothing.
    let (plan, mut dropped, mut app_ok, mut voice_ok, mut rec_stop_ok, mut rec_pause_ok) = match app.try_state::<Db>() {
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
            // Registered only while BOTH the feature and its own opt-in flag are on --
            // switching either off unregisters it without touching the stored string,
            // the same way promptHotkeys already works for per_prompt above.
            let voice_hotkey_on = store.settings.ui_flags.get("voice") != Some(&false)
                && store.settings.ui_flags.get("voiceHotkey") == Some(&true);
            let voice_hotkey = voice_hotkey_on.then_some(store.settings.voice_hotkey.as_str());
            let voice_ok = !voice_hotkey_on
                || store.settings.voice_hotkey.trim().is_empty()
                || parse_accel(&store.settings.voice_hotkey).is_some();
            // Registered only while a recording is actually running -- see
            // recording_active above -- AND their own toggle is on (both ship ON).
            let rec_stop_on = rec_active
                && store.settings.ui_flags.get("screenRecord") != Some(&false)
                && store.settings.ui_flags.get("recStopHotkey") != Some(&false);
            let rec_pause_on = rec_active
                && store.settings.ui_flags.get("screenRecord") != Some(&false)
                && store.settings.ui_flags.get("recPauseHotkey") != Some(&false);
            let rec_stop_hotkey = rec_stop_on.then_some(store.settings.rec_stop_hotkey.as_str());
            let rec_pause_hotkey = rec_pause_on.then_some(store.settings.rec_pause_hotkey.as_str());
            let rec_stop_ok = !rec_stop_on
                || store.settings.rec_stop_hotkey.trim().is_empty()
                || parse_accel(&store.settings.rec_stop_hotkey).is_some();
            let rec_pause_ok = !rec_pause_on
                || store.settings.rec_pause_hotkey.trim().is_empty()
                || parse_accel(&store.settings.rec_pause_hotkey).is_some();
            (
                hotkey_plan(&store.settings.hotkey, voice_hotkey, rec_stop_hotkey, rec_pause_hotkey, &store.prompts, per_prompt),
                stale,
                app_ok,
                voice_ok,
                rec_stop_ok,
                rec_pause_ok,
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
                HotkeyOwner::Prompt(id) => dropped.push(id),
                HotkeyOwner::App => app_ok = false,
                HotkeyOwner::Voice => voice_ok = false,
                HotkeyOwner::RecStop => rec_stop_ok = false,
                HotkeyOwner::RecPause => rec_pause_ok = false,
            }
        }
    }
    if let Some(targets) = app.try_state::<HotkeyTargets>() {
        *targets.0.lock().unwrap_or_else(|e| e.into_inner()) = live;
    }
    REC_STOP_HOTKEY_OK.store(rec_stop_ok, std::sync::atomic::Ordering::Relaxed);
    REC_PAUSE_HOTKEY_OK.store(rec_pause_ok, std::sync::atomic::Ordering::Relaxed);
    if !app_ok || !voice_ok || !dropped.is_empty() {
        if let Some(state) = app.try_state::<Db>() {
            let mut store = lock(&state);
            if !app_ok {
                store.settings.hotkey = String::new();
            }
            if !voice_ok {
                store.settings.voice_hotkey = String::new();
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
pub(crate) fn fire_prompt_hotkey(app: &AppHandle, id: &str) {
    let _ = app.emit_to("main", "prompt-hotkey", id.to_string());
}

// One key press starts, the next stops -- same toggle logic as the button,
// just without ever bringing the window forward. A running session is stopped
// exactly the way voice_stop's own command body does (send "manual" on its
// channel); starting one is left to the main window's own JS, which already
// owns every setting lookup, the command-word post-processing and where
// the finished text goes -- the window stays fully capable of all of that while
// hidden, the same way a prompt hotkey already copies without ever showing it.
pub(crate) fn toggle_voice_from_hotkey(app: &AppHandle) {
    if let Some(vstate) = app.try_state::<VoiceState>() {
        let guard = vstate.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = guard.as_ref() {
            if !h.ended.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = h.stop.send("manual");
                return;
            }
        }
    }
    let _ = app.emit_to("main", "voice-hotkey-toggle", ());
}

// The tray/hotkey counterparts of the recording bar's own Stop/Pause
// buttons -- same commands, never a second implementation. Both run off a
// throwaway thread rather than inline: the global-shortcut handler fires on the
// main thread, and stopping finalizes the encoder (disk I/O), which must not
// stall the message loop the way the show_rec_bar/open_tray_window bugs did.
#[cfg(windows)]
pub(crate) fn trigger_rec_stop_hotkey(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new().name("rec-hotkey-stop".into()).spawn(move || {
        let _ = stop_recording_now(&app);
        if let Some(w) = app.get_webview_window("rec-bar") {
            let _ = w.hide();
        }
    });
}
#[cfg(not(windows))]
pub(crate) fn trigger_rec_stop_hotkey(_app: &AppHandle) {}

// The one place that actually pauses/resumes a session -- the recbar
// buttons, the global hotkey and the tray menu item all end up here, so
// there is exactly one spot that can forget to also pause/resume the key
// overlay capture (see keylog::pause/resume) instead of three.
#[cfg(windows)]
pub(crate) fn do_pause_recording(session: &mut recording::RecSession) {
    recording::pause(session);
    session.phase = RecPhase::Paused;
    // The key-overlay capture used to keep running straight through a pause
    // -- this drops every key struck while paused and freezes the
    // active-recording clock keys are timed against, the same freeze-on-pause
    // behaviour RecordClock already gives the video track.
    keylog::pause();
}

#[cfg(windows)]
pub(crate) fn do_resume_recording(session: &mut recording::RecSession) {
    recording::resume(session);
    session.phase = RecPhase::Recording;
    keylog::resume();
}

#[cfg(windows)]
pub(crate) fn trigger_rec_pause_hotkey(app: &AppHandle) {
    let toggled = if let Some(rstate) = app.try_state::<RecordingState>() {
        let mut guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_mut() {
            Some(session) if rec_can_pause(session.phase) => {
                do_pause_recording(session);
                true
            }
            Some(session) if rec_can_resume(session.phase) => {
                do_resume_recording(session);
                true
            }
            _ => false,
        }
    } else {
        false
    };
    if toggled {
        apply_tray_menu(app); // keep the tray's Pause/Resume label in sync
    }
}
#[cfg(not(windows))]
pub(crate) fn trigger_rec_pause_hotkey(_app: &AppHandle) {}

// While a hotkey field is capturing, every global hotkey has to be off. Windows
// delivers a registered combination to the owning process instead of the focused
// window, so the field would never see the key press: the user just watches the
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
pub(crate) fn accel_is_free(app: &AppHandle, sc: Accel) -> bool {
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
            || parse_accel(&store.settings.voice_hotkey) == Some(sc)
            || parse_accel(&store.settings.rec_stop_hotkey) == Some(sc)
            || parse_accel(&store.settings.rec_pause_hotkey) == Some(sc)
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
            parse_accel(&store.settings.voice_hotkey) == Some(sc)
                || parse_accel(&store.settings.rec_stop_hotkey) == Some(sc)
                || parse_accel(&store.settings.rec_pause_hotkey) == Some(sc)
                || store.prompts.iter().any(|p| parse_accel(&p.hotkey) == Some(sc))
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
        store.settings.hotkey = hk;
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

// 131's counterpart to set_hotkey, same shape: validated before the write, the
// app hotkey and every prompt binding checked as a collision, nothing new that
// check_hotkey/rearm_hotkeys does not already do for the other two kinds.
#[tauri::command]
async fn set_voice_hotkey(app: AppHandle, state: State<'_, Db>, hotkey: String) -> Result<(), String> {
    let hk = hotkey.trim().to_string();
    if !hk.is_empty() {
        let sc = accel_error(&hk).map_err(|e| e.to_string())?;
        let taken = {
            let store = lock(&state);
            parse_accel(&store.settings.hotkey) == Some(sc)
                || parse_accel(&store.settings.rec_stop_hotkey) == Some(sc)
                || parse_accel(&store.settings.rec_pause_hotkey) == Some(sc)
                || store.prompts.iter().any(|p| parse_accel(&p.hotkey) == Some(sc))
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
        store.settings.voice_hotkey.clone_from(&hk);
        save_settings(&app, &store.settings);
    }
    // rearm_hotkeys' own return value only reports the APP hotkey's fate; a
    // refused voice hotkey is instead cleared from the store by rearm_hotkeys
    // itself, so re-reading it here is what actually answers "did this one land".
    rearm_hotkeys(&app);
    if hk.is_empty() { return Ok(()); }
    let store = lock(&state);
    if store.settings.voice_hotkey == hk {
        Ok(())
    } else {
        Err(HK_CONFLICT.to_string())
    }
}

// Stop/pause counterparts of set_voice_hotkey, same shape -- validated
// before the write against every OTHER hotkey kind (including each other), then
// stored. Unlike the app/voice hotkey these are only actually registered while a
// recording runs (rearm_hotkeys), so a "successful" set here can still show as
// blocked later if another app claims the combination only once recording starts
// -- that case is reported through recording_status(), not this call.
#[tauri::command]
async fn set_rec_stop_hotkey(app: AppHandle, state: State<'_, Db>, hotkey: String) -> Result<(), String> {
    let hk = hotkey.trim().to_string();
    if !hk.is_empty() {
        let sc = accel_error(&hk).map_err(|e| e.to_string())?;
        let taken = {
            let store = lock(&state);
            parse_accel(&store.settings.hotkey) == Some(sc)
                || parse_accel(&store.settings.voice_hotkey) == Some(sc)
                || parse_accel(&store.settings.rec_pause_hotkey) == Some(sc)
                || store.prompts.iter().any(|p| parse_accel(&p.hotkey) == Some(sc))
        };
        if taken {
            return Err(HK_TAKEN.to_string());
        }
    }
    {
        let mut store = lock(&state);
        store.settings.rec_stop_hotkey.clone_from(&hk);
        save_settings(&app, &store.settings);
    }
    rearm_hotkeys(&app);
    Ok(())
}
#[tauri::command]
async fn set_rec_pause_hotkey(app: AppHandle, state: State<'_, Db>, hotkey: String) -> Result<(), String> {
    let hk = hotkey.trim().to_string();
    if !hk.is_empty() {
        let sc = accel_error(&hk).map_err(|e| e.to_string())?;
        let taken = {
            let store = lock(&state);
            parse_accel(&store.settings.hotkey) == Some(sc)
                || parse_accel(&store.settings.voice_hotkey) == Some(sc)
                || parse_accel(&store.settings.rec_stop_hotkey) == Some(sc)
                || store.prompts.iter().any(|p| parse_accel(&p.hotkey) == Some(sc))
        };
        if taken {
            return Err(HK_TAKEN.to_string());
        }
    }
    {
        let mut store = lock(&state);
        store.settings.rec_pause_hotkey.clone_from(&hk);
        save_settings(&app, &store.settings);
    }
    rearm_hotkeys(&app);
    Ok(())
}

// A hotkey-triggered prompt that has to go through the grid (chain mode) needs
// the main window in front first, so the UI asks for it before activating the tile.
#[tauri::command]
fn focus_main_window(app: AppHandle) {
    show_main(&app);
}

// ---------- Standalone variable prompt (hotkey / tray copies) ----------
//
// A copy started from OUTSIDE the app must not drag the whole window forward just to
// ask for a placeholder. This small always-on-top window asks, and nothing else moves.
// The main window still owns every variable rule (which tokens need asking, what the
// reserved names resolve to, what happens after the copy): this window is a form:
// it pulls the pending payload, hands back the typed values, and closes.
pub(crate) const VAR_WIN_LABEL: &str = "vars-0";
pub(crate) const VAR_WIN_W: f64 = 340.0; // logical px; keep in sync with .vars-card in floating.css
pub(crate) const VAR_ROW_H: f64 = 70.0;
pub(crate) const VAR_WIN_CHROME: f64 = 104.0; // title + buttons + padding
pub(crate) const VAR_WIN_MAX_H: f64 = 720.0; // the work area caps it further on small screens
pub(crate) const VAR_ROWS_MAX: u32 = 24;
// The payload is one prompt's placeholder list, never its body.
pub(crate) const VAR_PAYLOAD_MAX: usize = 64 * 1024;

// The pending request. Some(..) = a window is waiting for an answer; taking it is
// what makes an answer (or a closed window) fire exactly once. The generation is
// what keeps a *replaced* window honest: its Destroyed event arrives after the next
// request is already installed, and without the number it would cancel that one.
pub(crate) struct VarPrompt {
    pub(crate) pending: Mutex<Option<(u64, String)>>,
    pub(crate) next_gen: AtomicU64,
}

impl VarPrompt {
    pub(crate) fn new() -> Self {
        Self { pending: Mutex::new(None), next_gen: AtomicU64::new(1) }
    }
    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, Option<(u64, String)>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// Quick-launcher window (fuzzy search, instant copy).
pub(crate) const LAUNCHER_WIN_LABEL: &str = "launcher";
// Text the tray filter row hands over: launcher_items takes it once and clears it, and a
// hotkey summon clears it first so that path never inherits a stale query.
pub(crate) static LAUNCHER_PREFILL: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
// Bumped for every launcher window that gets built, so a reveal timer of an earlier
// window never shows a newer one before its page has loaded.
pub(crate) static LAUNCHER_GEN: AtomicU64 = AtomicU64::new(0);
pub(crate) const LAUNCHER_REVEAL_FALLBACK: Duration = Duration::from_millis(1500);
pub(crate) const LAUNCHER_WIN_W: f64 = 560.0; // logical px; keep in sync with .launcher-card in floating.css
pub(crate) const LAUNCHER_WIN_H: f64 = 420.0;

// Shared by every cursor-anchored popup window (launcher, var prompt): the monitor's
// work area under the cursor plus that monitor's own scale factor, so a second
// monitor at another DPI still gets correctly sized physical pixels.
pub(crate) fn work_and_dpi_at_cursor(app: &AppHandle) -> (Rect, f64, PhysicalPosition<f64>) {
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
// every other quick-launcher (Spotlight, PowerToys Run, Alfred) does: a search box
// that pops up at screen centre reads as a dialog, not a launcher.
pub(crate) fn launcher_rect(app: &AppHandle) -> Rect {
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
pub(crate) fn var_window_rect(app: &AppHandle, rows: u32) -> Rect {
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
    win.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            if let Some(p) = app.try_state::<VarPrompt>() {
                // Only cancel the request this window was built for. A newer one has
                // a newer generation and belongs to the window that replaced this.
                let mut slot = p.lock();
                if slot.as_ref().is_some_and(|(g, _)| *g == gen) {
                    slot.take();
                    drop(slot);
                    let _ = app.emit_to("main", "vars-filled", Option::<String>::None);
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

// The rows estimate in var_window_rect is a guess (a remember switch, a long name or a
// larger system font all add height). Once the form has rendered it reports the
// height it really needs, in logical px, and the window grows or shrinks to that,
// staying centred where it was and inside its monitor's work area.
#[tauri::command]
fn var_window_fit(app: AppHandle, height: f64) -> Result<(), String> {
    let Some(win) = app.get_webview_window(VAR_WIN_LABEL) else { return Ok(()) };
    if !height.is_finite() || height <= 0.0 {
        return Err("bad height".to_string());
    }
    let dpi = win.scale_factor().unwrap_or(1.0);
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) else { return Ok(()) };
    let (x, w, old_h) = (pos.x as f64, size.width as f64, size.height as f64);
    let mid_y = pos.y as f64 + old_h / 2.0;
    let work = work_rect_at(&app, x + w / 2.0, mid_y);
    let h = (height.min(VAR_WIN_MAX_H) * dpi).min(work.h).max(1.0);
    if (h - old_h).abs() < 1.0 {
        return Ok(());
    }
    let (x, y) = clamp_to_work(work, w, h, x, mid_y - h / 2.0);
    set_bounds_phys(&win, Rect { x, y, w, h });
    Ok(())
}

// The {#{username}#} placeholder. The account name never reaches a log: it is
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

// ---------- tray quick-copy sections ----------

// A prompt name ends up in an OS menu, so it is stripped and capped first.
pub(crate) const TRAY_LABEL_MAX: usize = 40;
// How far back from the cap tray_label may look for a word break.
pub(crate) const TRAY_LABEL_WORD_BACK: usize = 12;
// Namespace for the dynamic entries. Every other menu id is a fixed keyword, so a
// click can be routed back to a prompt without ever guessing.
pub(crate) const TRAY_COPY_PREFIX: &str = "psc:";
// How long a burst of store changes is collected before the menu is rebuilt.
pub(crate) const TRAY_DEBOUNCE_MS: u64 = 400;

// Whether the installed menu currently carries quick-copy entries, and whether a
// rebuild is already scheduled. Both flags off + nothing installed = no work at all.
pub(crate) static TRAY_EXTRAS: AtomicBool = AtomicBool::new(false);
pub(crate) static TRAY_PENDING: AtomicBool = AtomicBool::new(false);

// What the two sections should contain, read from the expert settings.
pub(crate) struct TrayCfg {
    pub(crate) recent: bool,
    pub(crate) recent_n: usize,
    pub(crate) favs: bool,
    pub(crate) fav_n: usize,
    pub(crate) label_max: usize,
}

pub(crate) fn tray_cfg(settings: &Settings) -> TrayCfg {
    let count = |key: &str, def: f64| {
        settings.ui_values.get(key).copied().unwrap_or(def).clamp(1.0, 10.0) as usize
    };
    TrayCfg {
        recent: settings.ui_flags.get("trayRecent") != Some(&false), // on by default
        recent_n: count("trayRecentN", 5.0),
        favs: settings.ui_flags.get("trayFavs") != Some(&false), // on by default
        fav_n: count("trayFavN", 10.0),
        label_max: tray_label_max(settings),
    }
}

// Expert-tunable cap for tray_label, clamped the same way every other
// ui_values-backed expert row is: an absent/out-of-range stored value falls
// back to the shipped default rather than reaching tray_label unclamped.
pub(crate) fn tray_label_max(settings: &Settings) -> usize {
    settings.ui_values.get("trayLabelMax").copied().unwrap_or(TRAY_LABEL_MAX as f64).clamp(20.0, 80.0) as usize
}

// Whether the tray menu's filter row is drawn at all.
pub(crate) fn tray_search_on(settings: &Settings) -> bool {
    settings.ui_flags.get("traySearch") != Some(&false) // on by default
}

// Menu text for a prompt. Control characters (a pasted newline lands in the name)
// collapse to spaces, `&` is doubled because a Win32 menu reads a single one as a
// mnemonic, and the cut counts CHARACTERS: a byte cut would split a multi-byte
// name in the middle of a character. An over-long name is cut back to the last word
// break in the tail rather than mid-word, so the entry still reads as words.
pub(crate) fn tray_label(name: &str, max: usize) -> String {
    let flat: String = name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let mut text = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > max {
        let mut cut: String = text.chars().take(max - 1).collect();
        if let Some(space) = cut.rfind(' ') {
            // Only when the break is near the end: otherwise a single long word
            // would shrink the label to almost nothing.
            if cut[..space].chars().count() >= max.saturating_sub(TRAY_LABEL_WORD_BACK) {
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

pub(crate) fn tray_item_id(prompt_id: &str) -> String {
    format!("{TRAY_COPY_PREFIX}{prompt_id}")
}

// Route a clicked menu id back to a prompt. Anything that is not one of our own
// quick-copy ids is ignored instead of being looked up.
pub(crate) fn tray_copy_target(menu_id: &str) -> Option<&str> {
    match menu_id.strip_prefix(TRAY_COPY_PREFIX) {
        Some(id) if !id.is_empty() => Some(id),
        _ => None,
    }
}

// What a quick-copy entry actually delivers. It decides the entry's menu icon, so
// the media kind of a prompt is readable straight from the tray.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TrayKind {
    Text,
    Image,
    Media,
    File,
}

// The image fields are not exclusive: a picture can be the payload (copy_image), the
// tile's face (show_image) or just a decoration next to the text. Only the first two
// make the entry a picture: a decorated text prompt still copies text.
pub(crate) fn tray_kind(p: &Prompt) -> TrayKind {
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
pub(crate) struct TrayEntry {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: TrayKind,
    // The prompt's own tile colour ("" = tint the icon by kind instead).
    pub(crate) color: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TraySection {
    Favorites,
    Recent,
}

// The menu's dynamic part, in the order it is drawn: pinned favourites first, then
// recents: the Windows jump-list idiom, so the curated section never moves under
// the user while the recents below it churn. A favourite is not repeated in the
// recents; those backfill with the next unlisted prompt instead of leaving a gap.
// Pure, so ordering, caps, dedupe and truncation stay testable: the menu API cannot.
pub(crate) fn tray_plan(
    prompts: &[Prompt],
    last_used: &HashMap<String, u64>,
    cfg: &TrayCfg,
) -> Vec<(TraySection, Vec<TrayEntry>)> {
    let entry = |p: &Prompt| TrayEntry {
        id: tray_item_id(&p.id),
        label: tray_label(&p.name, cfg.label_max),
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

// ---------- menu icons ----------
// A menu icon is a 16x16 chip: the prompt's tile colour (or the media kind's own
// tint) with a glyph on top. It is FULLY OPAQUE on purpose. The Win32 menu path
// turns an icon into a bitmap through a monochrome AND mask that is only built
// correctly when every pixel is opaque, so a glyph with transparent padding comes
// out striped instead of clean.
pub(crate) const TRAY_ICON_PX: u32 = 16;

pub(crate) fn hex_rgb(hex: &str) -> Option<[u8; 3]> {
    let c = hex_to_color(hex.trim())?;
    Some([c.0, c.1, c.2])
}

// Perceived brightness, 0..255: decides whether the glyph is drawn light or dark.
pub(crate) fn tray_luma(c: [u8; 3]) -> u32 {
    (c[0] as u32 * 299 + c[1] as u32 * 587 + c[2] as u32 * 114) / 1000
}

// Blend `c` toward `to` by `amount` percent.
pub(crate) fn tray_mix(c: [u8; 3], to: [u8; 3], amount: u32) -> [u8; 3] {
    let f = |a: u8, b: u8| ((a as u32 * (100 - amount) + b as u32 * amount) / 100) as u8;
    [f(c[0], to[0]), f(c[1], to[1]), f(c[2], to[2])]
}

pub(crate) fn tray_kind_tint(kind: TrayKind) -> [u8; 3] {
    match kind {
        TrayKind::Text => [90, 100, 114],
        TrayKind::Image => [47, 111, 237],
        TrayKind::Media => [139, 92, 246],
        TrayKind::File => [224, 138, 30],
    }
}

// A 16x16 chip under construction. Every write is bounds-checked, so a glyph that
// runs over an edge is clipped instead of wrapping onto the next row.
pub(crate) struct TrayChip {
    pub(crate) px: Vec<u8>,
}

impl TrayChip {
    pub(crate) fn new(fill: [u8; 3]) -> Self {
        let n = (TRAY_ICON_PX * TRAY_ICON_PX) as usize;
        let mut px = Vec::with_capacity(n * 4);
        for _ in 0..n {
            px.extend_from_slice(&[fill[0], fill[1], fill[2], 255]);
        }
        Self { px }
    }
    pub(crate) fn dot(&mut self, x: i32, y: i32, c: [u8; 3]) {
        let n = TRAY_ICON_PX as i32;
        if x < 0 || y < 0 || x >= n || y >= n {
            return;
        }
        let i = ((y * n + x) * 4) as usize;
        self.px[i] = c[0];
        self.px[i + 1] = c[1];
        self.px[i + 2] = c[2];
    }
    pub(crate) fn row(&mut self, x0: i32, x1: i32, y: i32, c: [u8; 3]) {
        for x in x0..=x1 {
            self.dot(x, y, c);
        }
    }
    pub(crate) fn bar(&mut self, x0: i32, x1: i32, y0: i32, y1: i32, c: [u8; 3]) {
        for y in y0..=y1 {
            self.row(x0, x1, y, c);
        }
    }
    pub(crate) fn frame(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 3]) {
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
pub(crate) fn tray_icon_pixels(kind: TrayKind, color: &str) -> Vec<u8> {
    let fill = hex_rgb(color).unwrap_or_else(|| tray_kind_tint(kind));
    let dark = tray_luma(fill) < 150;
    let ink = if dark { [255, 255, 255] } else { [24, 26, 31] };
    let edge = tray_mix(fill, if dark { [255, 255, 255] } else { [0, 0, 0] }, 28);
    let mut chip = TrayChip::new(fill);
    chip.frame(0, 0, 15, 15, edge);
    match kind {
        // Three text lines, the last one short: a paragraph at 16 px.
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

pub(crate) fn tray_icon(kind: TrayKind, color: &str) -> tauri::image::Image<'static> {
    tauri::image::Image::new_owned(tray_icon_pixels(kind, color), TRAY_ICON_PX, TRAY_ICON_PX)
}

// One labelled section: a greyed header, the entries with their icons, a separator.
pub(crate) fn push_tray_section(
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

// The custom Windows-11-style tray window replaces the native right-click
// popup. Ships on (a rounder, themed look was the whole point of building it);
// switching it off is the escape hatch back to the plain OS menu if the
// custom window ever misbehaves.
pub(crate) const TRAY_CUSTOM_MENU_KEY: &str = "trayCustomMenu";

pub(crate) fn tray_custom_menu_on(settings: &Settings) -> bool {
    settings.ui_flags.get(TRAY_CUSTOM_MENU_KEY) != Some(&false)
}

// Build the tray menu from the current store and install it. The store lock is
// dropped before the first menu call: building a menu while holding it is the
// classic deadlock shape here.
pub(crate) fn apply_tray_menu(app: &AppHandle) {
    let (lang, plan, custom, rec_entries_on) = match app.try_state::<Db>() {
        Some(state) => {
            let store = lock(&state);
            let cfg = tray_cfg(&store.settings);
            let plan = tray_plan(&store.prompts, &store.settings.last_used, &cfg);
            let rec_entries_on = store.settings.ui_flags.get("screenRecord") != Some(&false)
                && store.settings.ui_flags.get("recTrayEntries") != Some(&false);
            (resolve_lang(&store.settings.language), plan, tray_custom_menu_on(&store.settings), rec_entries_on)
        }
        None => return,
    };
    if custom {
        // The custom window builds its own content live from tray_window_plan()
        // on every right-click, so there is nothing to precompute here: just
        // make sure no native popup is left attached to fight it for the
        // right-click. Detaching also skips every rebuild this function would
        // otherwise do while the toggle stays on.
        if let Some(tray) = app.tray_by_id("tray") {
            let _ = tray.set_menu::<Menu<tauri::Wry>>(None);
        }
        return;
    }
    TRAY_EXTRAS.store(!plan.is_empty(), Ordering::SeqCst);
    let (open_label, quit_label) = tray_labels(lang);
    let (recent_title, favs_title) = tray_section_labels(lang);
    let mut items: Vec<Box<dyn IsMenuItem<tauri::Wry>>> = Vec::new();
    // While a recording runs, two extra entries at the very top -- the
    // tray-menu counterpart of the bar's own Stop/Pause buttons, same commands.
    if rec_entries_on {
        if let Some(phase) = recording_phase(app) {
            let paused = phase == RecPhase::Paused;
            if let (Ok(stop), Ok(pause)) = (
                MenuItem::with_id(app, "rec-stop", tray_rec_stop_label(lang), true, None::<&str>),
                MenuItem::with_id(app, "rec-pause-toggle", tray_rec_pause_label(lang, paused), true, None::<&str>),
            ) {
                items.push(Box::new(stop));
                items.push(Box::new(pause));
                if let Ok(sep) = PredefinedMenuItem::separator(app) {
                    items.push(Box::new(sep));
                }
            }
        }
    }
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
pub(crate) fn rebuild_tray(app: &AppHandle) {
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
// chains, placeholder dialog): but never the hotkey auto-paste: the click leaves
// the tray in the foreground, so there is no text field to paste into.
pub(crate) fn fire_tray_copy(app: &AppHandle, id: &str) {
    let _ = app.emit_to("main", "prompt-tray-copy", id.to_string());
}

// One place for what a tray menu id DOES, shared by the native popup's
// on_menu_event and the custom tray window's tray_window_activate command --
// the two menus must never drift into behaving differently for the same id.
pub(crate) fn handle_tray_menu_id(app: &AppHandle, id: &str) {
    match id {
        "show" => show_main(app),
        "quit" => request_app_quit(app),
        "rec-stop" => trigger_rec_stop_hotkey(app),
        "rec-pause-toggle" => trigger_rec_pause_hotkey(app),
        other => {
            if let Some(id) = tray_copy_target(other) {
                fire_tray_copy(app, id);
            }
        }
    }
}

// ---------- Custom Windows-11-style tray window ----------
// The native TrackPopupMenu cannot be rounded or restyled -- that is a shell/XAML
// look, not something Win32 menus support. apply_tray_menu() detaches the native
// menu entirely while trayCustomMenu is on (see there), and this window takes
// over the right-click instead, drawing its OWN content from the plan below.

pub(crate) const TRAY_WIN_LABEL: &str = "tray-menu";
pub(crate) const TRAY_WIN_W: f64 = 240.0; // logical px; keep in sync with .tray-menu-card in floating.css
pub(crate) const TRAY_WIN_ROW_H: f64 = 30.0;
pub(crate) const TRAY_WIN_HEAD_H: f64 = 22.0;
pub(crate) const TRAY_WIN_SEP_H: f64 = 9.0; // 1px line + 4px margin top/bottom, one per section
pub(crate) const TRAY_WIN_CHROME: f64 = 20.0; // card padding, independent of row count

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrayWinEntry {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) color: String,
}

#[derive(Serialize, Clone)]
pub(crate) struct TrayWinSection {
    pub(crate) title: String,
    pub(crate) entries: Vec<TrayWinEntry>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrayWinPlan {
    // 0 or 2 entries (Stop, Pause/Resume) -- no header, own separator,
    // rendered before every other row. ui/tray.js draws them with no icon,
    // exactly like show/quit below.
    pub(crate) rec_entries: Vec<TrayWinEntry>,
    pub(crate) sections: Vec<TrayWinSection>,
    pub(crate) show_label: String,
    pub(crate) quit_label: String,
    // Whether the search row at the top of the menu is drawn at all (traySearch,
    // default on) -- tray_window_logical_height only reserves a row for it when
    // this is true, so an unrelated off switch never grows the window for nothing.
    pub(crate) search_on: bool,
    pub(crate) search_ph: String,
}

pub(crate) fn tray_kind_str(kind: TrayKind) -> &'static str {
    match kind {
        TrayKind::Text => "text",
        TrayKind::Image => "image",
        TrayKind::Media => "media",
        TrayKind::File => "file",
    }
}

// Same content, same order as the native menu (tray_plan/tray_labels) -- this is
// a restyle, not a second menu with its own rules. Built fresh on every open, so
// (unlike the native HMENU) there is no separate rebuild-on-change path to keep
// in sync.
pub(crate) fn build_tray_win_plan(app: &AppHandle) -> TrayWinPlan {
    let state: State<Db> = match app.try_state::<Db>() {
        Some(s) => s,
        None => {
            return TrayWinPlan { rec_entries: Vec::new(), sections: Vec::new(), show_label: String::new(), quit_label: String::new(), search_on: false, search_ph: String::new() };
        }
    };
    let store = lock(&state);
    let cfg = tray_cfg(&store.settings);
    let plan = tray_plan(&store.prompts, &store.settings.last_used, &cfg);
    let lang = resolve_lang(&store.settings.language);
    let (open_label, quit_label) = tray_labels(lang);
    let (recent_title, favs_title) = tray_section_labels(lang);
    let rec_entries_on = store.settings.ui_flags.get("screenRecord") != Some(&false)
        && store.settings.ui_flags.get("recTrayEntries") != Some(&false);
    let search_on = tray_search_on(&store.settings);
    drop(store);
    let rec_entries = if rec_entries_on {
        match recording_phase(app) {
            Some(phase) => {
                let paused = phase == RecPhase::Paused;
                vec![
                    TrayWinEntry { id: "rec-stop".to_string(), label: tray_rec_stop_label(lang).to_string(), kind: "text".to_string(), color: String::new() },
                    TrayWinEntry { id: "rec-pause-toggle".to_string(), label: tray_rec_pause_label(lang, paused).to_string(), kind: "text".to_string(), color: String::new() },
                ]
            }
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let sections = plan
        .into_iter()
        .map(|(section, entries)| TrayWinSection {
            title: match section {
                TraySection::Favorites => favs_title,
                TraySection::Recent => recent_title,
            }
            .to_string(),
            entries: entries
                .into_iter()
                .map(|e| TrayWinEntry { id: e.id, label: e.label, kind: tray_kind_str(e.kind).to_string(), color: e.color })
                .collect(),
        })
        .collect();
    TrayWinPlan {
        rec_entries,
        sections,
        show_label: open_label.to_string(),
        quit_label: quit_label.to_string(),
        search_on,
        search_ph: tray_search_placeholder(lang).to_string(),
    }
}

// Pure, so the row/header/separator arithmetic stays testable without an app
// handle -- mirrors tray_plan's own reasoning for why it takes plain data.
pub(crate) fn tray_window_logical_height(plan: &TrayWinPlan) -> f64 {
    let mut logical_h = TRAY_WIN_CHROME + 2.0 * TRAY_WIN_ROW_H; // show + quit always present
    if plan.search_on {
        logical_h += TRAY_WIN_ROW_H; // the filter row at the very top
    }
    if !plan.rec_entries.is_empty() {
        logical_h += plan.rec_entries.len() as f64 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H;
    }
    for s in &plan.sections {
        logical_h += TRAY_WIN_HEAD_H + s.entries.len() as f64 * TRAY_WIN_ROW_H + TRAY_WIN_SEP_H;
    }
    logical_h
}

// clamp_to_work only ever repositions -- it cannot shrink a window taller than
// the work area, so at the supported 10-favorites/10-recents limits (both
// sections on by default) the uncapped content could reach ~742 logical px and
// overhang the bottom of a small or heavily-scaled display, with the card's own
// (former) "overflow: hidden" then making the lower rows -- Quit included --
// impossible to reach. Capped to the work area with an 8px margin so it never
// sits flush against the taskbar/screen edge; floating.css makes the card
// scroll internally once its content no longer fits the capped height. Split
// out from tray_window_rect so the cap math is testable without an AppHandle.
pub(crate) fn tray_window_capped_height(plan: &TrayWinPlan, work_h: f64, dpi: f64) -> f64 {
    (tray_window_logical_height(plan) * dpi).min((work_h - 8.0 * dpi).max(TRAY_WIN_ROW_H * dpi))
}

pub(crate) fn tray_window_rect(app: &AppHandle, plan: &TrayWinPlan) -> Rect {
    let (work, dpi, cursor) = work_and_dpi_at_cursor(app);
    let w = TRAY_WIN_W * dpi;
    let h = tray_window_capped_height(plan, work.h, dpi);
    let (x, y) = clamp_to_work(work, w, h, cursor.x, cursor.y);
    Rect { x, y, w, h }
}

// Opens at the cursor, clamped into that monitor's work area -- same helper every
// other cursor-anchored popup (launcher, var prompt) already uses. Replaces an
// already-open one instead of stacking a second, matching those windows too.
pub(crate) fn open_tray_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(TRAY_WIN_LABEL) {
        let _ = win.close();
    }
    let excluded = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        capture_excluded(&store.settings)
    };
    let plan = build_tray_win_plan(app);
    let rect = tray_window_rect(app, &plan);
    let win = match WebviewWindowBuilder::new(app, TRAY_WIN_LABEL, WebviewUrl::App("tray.html".into()))
        .title(brand::DISPLAY_NAME)
        .inner_size(TRAY_WIN_W, TRAY_WIN_W) // placeholder: set_bounds_phys owns the real box
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

#[tauri::command]
fn tray_window_plan(app: AppHandle) -> TrayWinPlan {
    build_tray_win_plan(&app)
}

#[tauri::command]
fn tray_window_activate(app: AppHandle, id: String) {
    handle_tray_menu_id(&app, &id);
    if let Some(win) = app.get_webview_window(TRAY_WIN_LABEL) {
        let _ = win.close();
    }
}

#[tauri::command]
fn tray_window_close(app: AppHandle) {
    if let Some(win) = app.get_webview_window(TRAY_WIN_LABEL) {
        let _ = win.close();
    }
}

// The tray filter row hands its first keystroke to the quick launcher: what was typed
// becomes the launcher's starting query and the launcher is built, but it only appears
// (launcher_ready) once its page is up. The tray window keeps the focus and every key
// until then, passes on the complete text (tray_launcher_prefill) the moment the launcher
// takes the focus away, and closes itself after that.
// Must stay async: it builds the launcher window, and window creation from a sync
// command deadlocks on Windows (sync commands run on the main thread).
#[tauri::command]
async fn tray_open_launcher(app: AppHandle, query: String) {
    *LAUNCHER_PREFILL.lock().unwrap_or_else(|e| e.into_inner()) = launcher_prefill_text(&query);
    open_launcher_window(&app, true);
}

// The launcher page has loaded and is listening: now the window may appear and take the
// focus. The tray notices it lost the focus and passes on everything typed so far.
#[tauri::command]
fn launcher_ready(app: AppHandle) {
    reveal_launcher(&app);
}

// Everything typed in the tray before the launcher took the focus: the newer text replaces
// the pending starting query and is pushed to the launcher, which puts it in front of any
// key it received on its own since.
#[tauri::command]
fn tray_launcher_prefill(app: AppHandle, query: String) {
    let text = launcher_prefill_text(&query);
    *LAUNCHER_PREFILL.lock().unwrap_or_else(|e| e.into_inner()) = text.clone();
    let _ = app.emit_to(LAUNCHER_WIN_LABEL, "launcher-prefill", text);
}

// The starting query is user input crossing a window boundary: capped like the launcher's own field.
pub(crate) fn launcher_prefill_text(q: &str) -> String {
    q.chars().take(200).collect()
}

// A result picked in the quick-launcher runs the very same activation the tray
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

// ---------- Theme ----------

// Every theme that has its own rules in main.css. "system" is not in here: it
// resolves to light/dark at runtime, and it is not a valid custom-theme base.
pub(crate) const BUILTIN_THEMES: [&str; 7] =
    ["light", "dark", "programmer", "neon", "ocean", "cyberpunk", "mono"];
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
        threshold: settings.ui_values.get("inkThreshold").copied().unwrap_or(60.0).clamp(20.0, 90.0),
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
            "mono" => tauri::webview::Color(250, 250, 250, 255),
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

// ---------- Floating windows ----------

pub(crate) fn flabel(id: &str) -> String {
    format!("float-{}", id)
}

// New pills spawn at the top-left of the primary monitor, cascaded slightly
// so several pills never fully overlap.
pub(crate) fn default_pos(app: &AppHandle, index: usize) -> Pos {
    let off = 28 * (index as i32 % 8);
    let base = app
        .get_webview_window("main")
        .and_then(|w| w.primary_monitor().ok().flatten())
        .map(|m| {
            let p = m.position();
            Pos { x: p.x + 24, y: p.y + 24, ..Default::default() }
        })
        .unwrap_or(Pos { x: 24, y: 24, ..Default::default() });
    let x = base.x + off;
    let y = base.y + off;
    Pos { x, y, screen: screen_id_at(app, x, y) }
}

// One range for floating-button size, shared by the writer, the reader and the UI.
pub(crate) const FLOAT_SCALE_MIN: f64 = 0.3;
pub(crate) const FLOAT_SCALE_MAX: f64 = 8.0;

pub(crate) fn float_scale_of(settings: &Settings, id: &str) -> f64 {
    let s = settings.float_scale.get(id).copied().unwrap_or(1.0);
    // Same range set_float_scale accepts. A narrower one here silently shrank the
    // "L" preset and every freely resized pill back down on the next start.
    if s.is_finite() { s.clamp(FLOAT_SCALE_MIN, FLOAT_SCALE_MAX) } else { 1.0 }
}

// Pill window size: square box for image pills, classic pill for text.
pub(crate) fn pill_dims(is_image: bool, scale: f64) -> (f64, f64) {
    if is_image {
        (FLOAT_IMG * scale, FLOAT_IMG * scale)
    } else {
        (FLOAT_W * scale, FLOAT_H * scale)
    }
}

// Gif/video attachments render straight from their path (no stored preview).
// Keep in sync with GIF_EXT / VIDEO_EXT in ui/media.js.
pub(crate) fn media_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    [".gif", ".mp4", ".m4v", ".mov", ".webm", ".ogv", ".ogg", ".ogm"]
        .iter()
        .any(|e| lower.ends_with(e))
}

pub(crate) fn is_image_prompt(p: &Prompt) -> bool {
    p.show_image && (!p.image.is_empty() || media_path(&p.file_path) || media_path(&p.icon_path))
}

// Every file dialog is parented to the window that opened it: that window is
// blocked (and only that window) until the dialog closes, so a second dialog
// can never stack on top of it, and a satellite window (e.g. the video
// editor) never loses focus to "main" behind the dialog's back.
pub(crate) fn file_dialog_for(app: &AppHandle, label: &str) -> rfd::FileDialog {
    let dlg = rfd::FileDialog::new();
    let chosen = resolve_dialog_parent_label(|l| app.get_webview_window(l).is_some(), label);
    match chosen.and_then(|l| app.get_webview_window(l)) {
        Some(win) => dlg.set_parent(&win),
        None => dlg,
    }
}

// Pure label choice: the requested window if it is open, else "main" as a
// fallback, else nobody. Kept apart from the AppHandle lookup so the choice
// itself is unit-testable without a live Tauri app.
pub(crate) fn resolve_dialog_parent_label(exists: impl Fn(&str) -> bool, label: &str) -> Option<&str> {
    if exists(label) {
        Some(label)
    } else if exists("main") {
        Some("main")
    } else {
        None
    }
}

pub(crate) fn file_dialog(app: &AppHandle) -> rfd::FileDialog {
    file_dialog_for(app, "main")
}

// `taken` collects the box this pill actually lands in (real or parked) so
// the next call in the same sync_float_windows pass fans its own park spot
// out from this one instead of landing on top of it.
pub(crate) fn open_floating(app: &AppHandle, prompt: &Prompt, taken: &mut Vec<Rect>) {
    let label = flabel(&prompt.id);
    if app.get_webview_window(&label).is_some() {
        return;
    }

    // Never call Tauri window APIs while holding the Db lock (deadlock risk).
    let (saved, scale, count, excluded, park_on) = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        (
            store.settings.floating.get(&prompt.id).cloned(),
            float_scale_of(&store.settings, &prompt.id),
            store.settings.floating.len(),
            capture_excluded(&store.settings),
            store.settings.ui_flags.get("floatPark") != Some(&false),
        )
    };
    let is_new = saved.is_none();
    let pos = saved.unwrap_or_else(|| default_pos(app, count));
    if is_new {
        let state: State<Db> = app.state();
        let mut store = lock(&state);
        store.settings.floating.insert(prompt.id.clone(), pos.clone());
        save_settings(app, &store.settings);
    }

    let (pw, ph) = pill_dims(is_image_prompt(prompt), scale);
    let (x, y, parked) = if park_on {
        resolve_float_spot(&pos, pw, ph, &screen_snapshot(app), taken)
    } else {
        let work = work_rect_at(app, pos.x as f64, pos.y as f64);
        let (x, y) = clamp_to_work(work, pw, ph, pos.x as f64, pos.y as f64);
        (x, y, false)
    };
    taken.push(Rect { x, y, w: pw, h: ph });
    mark_parked(app, &prompt.id, parked);

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
        let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        let _ = win.show();

        let app2 = app.clone();
        let pid = prompt.id.clone();
        win.on_window_event(move |event| match event {
            WindowEvent::Moved(p) => {
                // Fires for every pixel during a drag: never block the move
                // loop on a busy store; the next event carries the position.
                // A drag is the user placing the pill themselves, so it
                // always becomes the new real position AND ends parking --
                // there is nothing left to return to once they have moved it.
                let screen = screen_id_at(&app2, p.x, p.y);
                if let Some(state) = app2.try_state::<Db>() {
                    if let Ok(mut store) = state.try_lock() {
                        if store.settings.floating.contains_key(&pid) {
                            store.settings.floating.insert(pid.clone(), Pos { x: p.x, y: p.y, screen });
                        }
                    }
                }
                mark_parked(&app2, &pid, false);
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

pub(crate) fn close_floating_window(app: &AppHandle, id: &str) {
    if let Some(win) = app.get_webview_window(&flabel(id)) {
        let _ = win.close();
    }
}

// ---------- floating pill folders ----------

pub(crate) fn folder_label(id: &str) -> String {
    format!("folder-{}", id)
}

// Expert settings the backend needs. The feature ships ON; the two parameters use
// the same defaults and ranges as their sliders in the expert menu.
pub(crate) fn folders_on(settings: &Settings) -> bool {
    settings.ui_flags.get("pillFolders") != Some(&false)
}

// Pill and folder windows read the settings ONCE, when they open. Without this every
// expert change (pill folders, drag-out, opacity, cooldown …) would only reach the
// windows opened afterwards: i.e. a feature switched on while pills are on screen
// looked dead until a restart.
pub(crate) fn notify_float_settings(app: &AppHandle) {
    for label in app.webview_windows().keys() {
        if label.starts_with("float-") || label.starts_with("folder-") {
            let _ = app.emit_to(label.as_str(), "settings-changed", ());
        }
    }
}
pub(crate) fn merge_radius(settings: &Settings) -> f64 {
    settings
        .ui_values
        .get("pillMergeRadius")
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(40.0)
        .clamp(20.0, 80.0)
}
pub(crate) fn folder_cols(settings: &Settings) -> u32 {
    settings
        .ui_values
        .get("folderGridCols")
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(3.0)
        .clamp(2.0, 5.0) as u32
}

// A box in PHYSICAL desktop pixels: the one coordinate space every monitor shares,
// whatever its DPI. Sizes are converted per window with its own scale factor, so a
// mixed-DPI setup stays correct.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

impl Rect {
    pub(crate) fn right(&self) -> f64 {
        self.x + self.w
    }
    pub(crate) fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub(crate) fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub(crate) fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
}

// Which pill the just-dropped one landed on: the NEAREST centre within `radius` whose
// box ALSO really overlaps. Both conditions matter: the radius alone would fire for a
// pill that merely sits nearby, the overlap alone for a long text pill whose far end
// happens to touch. A short drag ends up far from every other pill and matches
// nothing, which is what keeps a normal nudge from creating a folder. A centre
// distance exactly equal to the radius still counts as a hit.
pub(crate) fn merge_target(dragged: Rect, others: &[(String, Rect)], radius: f64) -> Option<&str> {
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
pub(crate) enum FolderSide {
    Below,
    Above,
    Right,
    Left,
}

impl FolderSide {
    pub(crate) fn as_str(self) -> &'static str {
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
pub(crate) struct FolderLayout {
    pub(crate) side: FolderSide,
    pub(crate) win: Rect,
    pub(crate) icon: (f64, f64), // icon offset inside the window
    pub(crate) pop: (f64, f64),  // popover offset inside the window
}

// Grow the folder window around its icon. The icon NEVER moves: it is what the user
// clicked: so the popover goes on the first side with room (below, above, right,
// left) and is then slid along its free axis until it lies inside the work area.
// Everything is physical pixels of ONE monitor: pass that monitor's work rect and the
// sizes already scaled by its factor, and edge flipping works the same on a second
// screen at another DPI.
pub(crate) fn folder_layout(icon: Rect, pop_w: f64, pop_h: f64, gap: f64, work: Rect) -> FolderLayout {
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
    // left: and the icon has to be pushed right by the same amount inside it, one
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
pub(crate) fn clamp_to_work(work: Rect, w: f64, h: f64, x: f64, y: f64) -> (f64, f64) {
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
pub(crate) fn spread_free_positions(wishes: &[Rect], taken: &[Rect], work: Rect) -> Vec<(f64, f64)> {
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

// A screen's identity: its name, resolution and offset in the virtual
// desktop, hashed down to a short stable string. Two screens with the same
// dimensions at the same virtual-desktop position but a different name
// (a different physical monitor happening to land where an old one used to
// be) must never compare equal, which is exactly why the name is part of
// the hash and the position/size alone are not.
pub(crate) fn screen_identity(name: &str, x: i32, y: i32, w: i32, h: i32) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    (x, y, w, h).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

// One monitor as the pure, testable functions below need it: its identity
// plus its bounds. `tauri::Monitor` cannot be built outside a real window,
// so every live caller reduces its real monitor list to this shape first --
// which is also what lets resolve_float_spot below be unit-tested with a
// mocked screen list instead of a real one.
pub(crate) type ScreenRect = (String, i32, i32, i32, i32); // (identity, x, y, width, height)

pub(crate) fn screen_snapshot(app: &AppHandle) -> Vec<ScreenRect> {
    app.get_webview_window("main")
        .and_then(|w| w.available_monitors().ok())
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let p = m.position();
            let s = m.size();
            let name = m.name().cloned().unwrap_or_default();
            (screen_identity(&name, p.x, p.y, s.width as i32, s.height as i32), p.x, p.y, s.width as i32, s.height as i32)
        })
        .collect()
}

// The identity of whichever current screen contains (x, y), or empty if
// none does (point off every monitor, which a live cursor/window position
// should not normally be, but a probe failure must never panic).
pub(crate) fn screen_id_at(app: &AppHandle, x: i32, y: i32) -> String {
    screen_snapshot(app)
        .into_iter()
        .find(|(_, sx, sy, sw, sh)| x >= *sx && x < sx + sw && y >= *sy && y < sy + sh)
        .map(|(id, ..)| id)
        .unwrap_or_default()
}

// Where a saved floating position actually ends up today: unchanged if its
// screen is still there, parked on the main screen (the first entry of
// `screens`, always the primary monitor -- see screen_snapshot's caller)
// otherwise. NEVER mutates `pos` -- the caller keeps the stored position and
// screen id exactly as saved and only places the window at the returned
// point, so a screen that comes back finds its pill exactly where it left
// it. `taken` is every other window's current box, so several pills parked
// in the same pass fan out instead of stacking.
//
// A `pos.screen` of "" (saved before this field existed) falls back to
// plain coordinate containment -- today's behaviour -- so an ordinary
// upgrade never parks a button nothing has actually moved. Once a screen id
// is recorded, only that exact screen coming back counts as "still home";
// a different monitor that happens to cover the same point does not (a
// different identity with the same dimensions parks, per spec).
pub(crate) fn resolve_float_spot(pos: &Pos, w: f64, h: f64, screens: &[ScreenRect], taken: &[Rect]) -> (f64, f64, bool) {
    if screens.is_empty() {
        // A failed monitor probe must never look like "every screen is gone".
        return (pos.x as f64, pos.y as f64, false);
    }
    let still_home = screens.iter().any(|(id, sx, sy, sw, sh)| {
        let on_this_one = pos.x >= *sx && pos.x < sx + sw && pos.y >= *sy && pos.y < sy + sh;
        on_this_one && (pos.screen.is_empty() || *id == pos.screen)
    });
    if still_home {
        return (pos.x as f64, pos.y as f64, false);
    }
    let (mx, my, mw, mh) = (screens[0].1, screens[0].2, screens[0].3, screens[0].4);
    let work = Rect { x: mx as f64, y: my as f64, w: mw as f64, h: mh as f64 };
    let wish = Rect { x: work.x, y: work.y, w, h };
    let (x, y) = spread_free_positions(std::slice::from_ref(&wish), taken, work)
        .into_iter()
        .next()
        .unwrap_or((work.x, work.y));
    (x, y, true)
}

// Which parked ids have been parked longer than `cleanup_h` hours -- pure so
// the cleanup rule is testable without a real clock. `cleanup_h <= 0` is the
// switch's off position (the default): nothing ever ages out, matching
// floatParkCleanupH's own "0 = off" convention.
pub(crate) fn ids_past_park_cleanup(parked: &HashMap<String, std::time::Duration>, cleanup_h: f64) -> Vec<String> {
    if cleanup_h <= 0.0 {
        return Vec::new();
    }
    let limit = std::time::Duration::from_secs_f64(cleanup_h * 3600.0);
    parked.iter().filter(|(_, age)| **age >= limit).map(|(id, _)| id.clone()).collect()
}

// Bring the folder list back to its invariants: only real prompt ids, no prompt in two
// folders (the first folder listing it wins), a sane scale, and no folder left with
// fewer than two members: that one dissolves, and its pill simply reappears at the
// free position it kept all along. Runs on load and after every mutation. Idempotent.
pub(crate) fn normalize_float_folders(folders: &mut Vec<FloatFolder>, prompt_ids: &[String]) {
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
pub(crate) fn float_window_plan(settings: &Settings, prompt_ids: &[String]) -> (Vec<String>, Vec<String>) {
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

pub(crate) fn folder_name_of(name: &str) -> Result<String, String> {
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
pub(crate) struct FolderExpand(pub(crate) Mutex<HashMap<String, Rect>>);

// Pill/folder windows dismissed "for now" (their own context menu, not the
// permanent Remove/Dissolve). In memory only, like FolderExpand above: cleared
// on restart, never written to settings.
pub(crate) struct HiddenFloats(pub(crate) Mutex<std::collections::HashSet<String>>);

// A pill/folder currently parked (its saved screen is not among today's
// monitors), and when this run of the app first noticed. In memory only --
// the parked MARKER and the cleanup clock both reset on restart, never the
// underlying position or screen id in Settings, which nothing here ever
// touches. Presence in the map is what floating.js/folder.js ask for to
// show the "off a screen that is not here right now" marker; the age is
// what the floatParkCleanupH sweep compares against its own setting.
pub(crate) struct ParkedFloats(pub(crate) Mutex<HashMap<String, std::time::Instant>>);

// Records or clears one id's parked state. A fresh park keeps its own
// first-seen instant if it was already parked, so re-syncing a still-parked
// window (any other settings change re-runs sync_float_windows) never resets
// its own cleanup clock.
pub(crate) fn mark_parked(app: &AppHandle, id: &str, parked: bool) {
    if let Some(state) = app.try_state::<ParkedFloats>() {
        let mut map = state.0.lock().unwrap_or_else(|e| e.into_inner());
        if parked {
            map.entry(id.to_string()).or_insert_with(std::time::Instant::now);
        } else {
            map.remove(id);
        }
    }
}

// Whether a pill/folder is parked right now -- floating.js/folder.js ask this
// once on boot to decide whether to show the marker.
#[tauri::command]
fn is_float_parked(app: AppHandle, id: String) -> bool {
    app.try_state::<ParkedFloats>()
        .map(|s| s.0.lock().unwrap_or_else(|e| e.into_inner()).contains_key(&id))
        .unwrap_or(false)
}

pub(crate) fn folder_expansion(app: &AppHandle, id: &str) -> Option<Rect> {
    let state = app.try_state::<FolderExpand>()?;
    let map = state.0.lock().unwrap_or_else(|e| e.into_inner());
    map.get(id).copied()
}

pub(crate) fn set_folder_expansion(app: &AppHandle, id: &str, icon: Option<Rect>) {
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
pub(crate) fn folder_icon_rect(app: &AppHandle, id: &str, window: Rect) -> Rect {
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
pub(crate) fn window_rect_native(win: &tauri::WebviewWindow) -> Option<Rect> {
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

pub(crate) fn window_rect(win: &tauri::WebviewWindow) -> Option<Rect> {
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
pub(crate) fn work_rect_at(app: &AppHandle, x: f64, y: f64) -> Rect {
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
pub(crate) fn set_bounds_phys(win: &tauri::WebviewWindow, r: Rect) {
    let (x, y) = (r.x.round() as i32, r.y.round() as i32);
    let (w, h) = (r.w.round().max(1.0) as i32, r.h.round().max(1.0) as i32);
    #[cfg(windows)]
    if set_window_pos_phys(win, x, y, w, h) {
        return;
    }
    let _ = win.set_position(PhysicalPosition::new(x, y));
    let _ = win.set_size(PhysicalSize::new(w.max(1) as u32, h.max(1) as u32));
}

pub(crate) fn folder_box(scale: f64) -> f64 {
    FOLDER_BOX * if scale.is_finite() { scale.clamp(0.5, 3.0) } else { 1.0 }
}

// One folder window, built exactly like a pill: frameless, transparent, always on top,
// out of the taskbar and out of other apps' screen captures.
pub(crate) fn open_folder_window(app: &AppHandle, folder: &FloatFolder, taken: &mut Vec<Rect>) {
    let label = folder_label(&folder.id);
    if app.get_webview_window(&label).is_some() {
        return;
    }
    // Never call Tauri window APIs while holding the Db lock (deadlock risk).
    let (excluded, park_on) = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        (capture_excluded(&store.settings), store.settings.ui_flags.get("floatPark") != Some(&false))
    };
    let side = folder_box(folder.scale);
    let (x, y, parked) = if park_on {
        resolve_float_spot(&folder.pos, side, side, &screen_snapshot(app), taken)
    } else {
        let work = work_rect_at(app, folder.pos.x as f64, folder.pos.y as f64);
        let (x, y) = clamp_to_work(work, side, side, folder.pos.x as f64, folder.pos.y as f64);
        (x, y, false)
    };
    taken.push(Rect { x, y, w: side, h: side });
    mark_parked(app, &folder.id, parked);

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
        let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        let _ = win.show();

        let app2 = app.clone();
        let fid = folder.id.clone();
        win.on_window_event(move |event| match event {
            WindowEvent::Moved(p) => {
                // While the folder is open the window origin is NOT the icon, so the
                // stored position follows the icon, not the grown box. A drag is the
                // user placing the folder themselves, so it always becomes the new
                // real position AND ends parking.
                let off = folder_expansion(&app2, &fid).unwrap_or(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 });
                let icon_x = p.x + off.x.round() as i32;
                let icon_y = p.y + off.y.round() as i32;
                let screen = screen_id_at(&app2, icon_x, icon_y);
                if let Some(state) = app2.try_state::<Db>() {
                    // Fires for every pixel of a drag: never block the move loop.
                    if let Ok(mut store) = state.try_lock() {
                        if let Some(f) = store.settings.float_folders.iter_mut().find(|f| f.id == fid) {
                            f.pos = Pos { x: icon_x, y: icon_y, screen };
                        }
                    }
                }
                mark_parked(&app2, &fid, false);
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
// MUST NOT run while the store mutex is held: it creates and closes windows.
pub(crate) fn sync_float_windows(app: &AppHandle) {
    let (want_pills, want_folders, pills, folders) = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
        let (mut want_pills, mut want_folders) = float_window_plan(&store.settings, &ids);
        // "Hide for now" drops the id from the plan, which the stale-window
        // loop below then closes if it was already open: the same one filter
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
    // Every box already on screen, so a pill/folder opened below (parked or
    // not) fans out around what is already there instead of landing on it.
    let mut taken: Vec<Rect> = app.webview_windows().values().filter_map(window_rect).collect();
    for prompt in &pills {
        open_floating(app, prompt, &mut taken); // returns early when the window already exists
    }
    for folder in &folders {
        open_folder_window(app, folder, &mut taken);
    }
    // Whatever changed the plan also changed what an open folder shows.
    for label in app.webview_windows().keys() {
        if label.starts_with("folder-") {
            let _ = app.emit_to(label.as_str(), "folder-changed", ());
        }
    }
}

mod images;
use images::*;

// ---------- Prompt commands ----------

// Async: clones one prompt that may carry a large base64 image; off the UI thread.
// The editor, duplicate and draft-recovery paths all call this expecting the
// real, editable text -- never ciphertext -- so a confidential entry is
// decrypted here with the session key, or the call is refused outright while
// the vault is locked rather than handing back gibberish a save could then
// overwrite the real ciphertext with.
#[tauri::command]
async fn get_prompt(state: State<'_, Db>, id: String) -> Result<Option<Prompt>, String> {
    let store = lock(&state);
    let Some(p) = store.prompts.iter().find(|p| p.id == id) else {
        return Ok(None);
    };
    if !p.secret {
        return Ok(Some(p.clone()));
    }
    let key = secret_session_key(secret_timeout_min(&store.settings));
    let (text, variants) = reveal_secret_text(p, key)?;
    let mut plain = p.clone();
    plain.text = text;
    plain.variants = variants;
    Ok(Some(plain))
}

// First free [col,row] in row-major order; None if the grid is full.
pub(crate) fn first_free_cell(view: &View) -> Option<[u32; 2]> {
    first_free_cell_excluding(view, &[])
}

// The active view's grid-folder cells. Every caller that hands out a free cell in
// the active view needs these: they live outside view.layouts, so first_free_cell()
// alone cannot see them.
pub(crate) fn active_view_folder_cells(settings: &Settings) -> Vec<[u32; 2]> {
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
// coincide with a folder's cell: the tile would save fine but never draw, since
// the renderer gives a folder priority over whatever else sits on its cell.
pub(crate) fn first_free_cell_excluding(view: &View, extra: &[[u32; 2]]) -> Option<[u32; 2]> {
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
// counting `extra_occupied` (e.g. a grid folder's own cell, which lives
// outside `layout`) as taken too. All-or-nothing: if there isn't room for
// every id, nothing is placed and `layout` is left exactly as it was, so a caller
// that already removed these ids from somewhere else can react to the failure
// instead of stranding them.
// `folder_members` are ids already living inside a grid folder of this view --
// they need no cell of their own, so they never count against the need and
// never get a layout entry. A stray layout entry left over for one of them
// (e.g. from before it joined the folder) is dropped once there is room to
// process the batch, so an old, inconsistent save heals itself here instead of
// quietly wasting a cell the grid never shows as occupied.
pub(crate) fn place_ids_in_view(
    layout: &mut std::collections::HashMap<String, [u32; 2]>,
    ids: &[String],
    cols: u32,
    rows: u32,
    extra_occupied: &std::collections::HashSet<(u32, u32)>,
    folder_members: &std::collections::HashSet<String>,
) -> bool {
    let mut occupied: std::collections::HashSet<(u32, u32)> =
        layout.values().map(|c| (c[0], c[1])).collect();
    occupied.extend(extra_occupied.iter().copied());
    let need = ids
        .iter()
        .filter(|id| !folder_members.contains(id.as_str()) && !layout.contains_key(id.as_str()))
        .count();
    let free = (cols as usize * rows as usize).saturating_sub(occupied.len());
    if need > free {
        return false;
    }
    for id in ids {
        if folder_members.contains(id.as_str()) {
            layout.remove(id.as_str());
            continue;
        }
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

// Same accounting `place_ids_in_view` does internally, so a caller that just got
// `false` back can tell the user how many cells are still missing.
pub(crate) fn view_capacity_deficit(
    layout: &std::collections::HashMap<String, [u32; 2]>,
    ids: &[String],
    cols: u32,
    rows: u32,
    extra_occupied: &std::collections::HashSet<(u32, u32)>,
    folder_members: &std::collections::HashSet<String>,
) -> usize {
    let mut occupied: std::collections::HashSet<(u32, u32)> =
        layout.values().map(|c| (c[0], c[1])).collect();
    occupied.extend(extra_occupied.iter().copied());
    let need = ids
        .iter()
        .filter(|id| !folder_members.contains(id.as_str()) && !layout.contains_key(id.as_str()))
        .count();
    let free = (cols as usize * rows as usize).saturating_sub(occupied.len());
    need.saturating_sub(free)
}

// Drops `ids` from every layout size a view has ever been resized to, and from
// its authoritative `members` list -- what a cross-view move's source side does
// once the target has durably accepted the same ids (batch_prompts "moveView"),
// so a prompt dragged onto another view's tab never lingers behind in this one.
pub(crate) fn strip_ids_from_view(view: &mut View, ids: &[String]) {
    for layout in view.layouts.values_mut() {
        for id in ids {
            layout.remove(id);
        }
    }
    view.members.retain(|m| !ids.contains(m));
}

// Mutation + save core shared by batch_prompts' "addView", "removeView" and
// "moveView" actions -- kept free of AppHandle so the rollback below can be
// proven in a unit test. `save` commits `settings` exactly like save_settings
// does in production and reports whether it landed; on `false` this restores
// `settings.views` to what it held before the mutation and reports
// "save-failed", the same contract batch_prompts already gives its other
// actions for a failed write. A missing view is a silent no-op, matching what
// the inline arms did before this was pulled out.
pub(crate) fn apply_view_batch_action(
    settings: &mut Settings,
    action: &str,
    ids: &[String],
    view_id: &str,
    from_view_id: &str,
    mut save: impl FnMut(&Settings) -> bool,
) -> Result<(), String> {
    match action {
        "addView" | "removeView" => {
            // A grid folder occupies a cell via `tile_folders`, deliberately outside
            // `view.layouts` (it is not a layout entry, it is the folder's own `cell`
            // field) -- read it before `views` is borrowed mutably below.
            let folder_cells: std::collections::HashSet<(u32, u32)> = settings
                .tile_folders
                .iter()
                .filter(|f| f.view_id == view_id)
                .map(|f| (f.cell[0], f.cell[1]))
                .collect();
            let folder_members = folder_members_in_view(settings, view_id);
            let Some(vidx) = settings.views.iter().position(|v| v.id == view_id) else {
                return Ok(());
            };
            let before = settings.views.clone();
            let (cols, rows) = (settings.views[vidx].cols, settings.views[vidx].rows);
            let key = grid_key(cols, rows);
            let layout = settings.views[vidx].layouts.entry(key).or_default();
            if action == "removeView" {
                for id in ids {
                    layout.remove(id);
                }
                settings.views[vidx].members.retain(|m| !ids.contains(m));
            } else {
                if !place_ids_in_view(layout, ids, cols, rows, &folder_cells, &folder_members) {
                    let deficit = view_capacity_deficit(layout, ids, cols, rows, &folder_cells, &folder_members);
                    return Err(format!("view-full:{deficit}"));
                }
                for id in ids {
                    if !settings.views[vidx].members.iter().any(|m| m == id) {
                        settings.views[vidx].members.push(id.clone());
                    }
                }
            }
            if !save(settings) {
                settings.views = before;
                return Err("save-failed".into());
            }
            Ok(())
        }
        // doGridMove used to call addView then removeView as two separate
        // IPC round-trips (two separate saves) - a crash between them left the
        // prompt placed in both views. Both layouts now change in the SAME
        // in-memory settings before the single save below, so a failure (or a
        // crash) can only ever be seen before or after the whole move, never
        // mid-way.
        "moveView" => {
            let folder_cells: std::collections::HashSet<(u32, u32)> = settings
                .tile_folders
                .iter()
                .filter(|f| f.view_id == view_id)
                .map(|f| (f.cell[0], f.cell[1]))
                .collect();
            let folder_members = folder_members_in_view(settings, view_id);
            let to_idx = settings.views.iter().position(|v| v.id == view_id);
            let from_idx = settings.views.iter().position(|v| v.id == from_view_id);
            let (Some(to_idx), Some(from_idx)) = (to_idx, from_idx) else {
                return Ok(());
            };
            let before = settings.views.clone();
            let (cols, rows) = (settings.views[to_idx].cols, settings.views[to_idx].rows);
            let key = grid_key(cols, rows);
            let layout = settings.views[to_idx].layouts.entry(key).or_default();
            // All-or-nothing - only drop the source placement once the
            // target has durably accepted every id.
            if !place_ids_in_view(layout, ids, cols, rows, &folder_cells, &folder_members) {
                let deficit = view_capacity_deficit(layout, ids, cols, rows, &folder_cells, &folder_members);
                return Err(format!("view-full:{deficit}"));
            }
            for id in ids {
                if !settings.views[to_idx].members.iter().any(|m| m == id) {
                    settings.views[to_idx].members.push(id.clone());
                }
            }
            if to_idx != from_idx {
                strip_ids_from_view(&mut settings.views[from_idx], ids);
            }
            if !save(settings) {
                settings.views = before;
                return Err("save-failed".into());
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// "Store files" opt-in flag (off by default).
pub(crate) fn store_files_enabled(store: &Store) -> bool {
    store.settings.ui_flags.get("storeFiles").copied().unwrap_or(false)
}

// Staged copies that are on disk (or about to be) but not yet referenced by any
// prompt in the store. sweep_orphan_files treats them as referenced.
pub(crate) static PENDING_ATTACHMENTS: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);

pub(crate) fn pending_key(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

pub(crate) fn pending_attachments_contain(path: &str) -> bool {
    let guard = PENDING_ATTACHMENTS.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().is_some_and(|s| s.contains(&pending_key(path)))
}

// Owns the copies one add/update made before the store lock. While alive they are
// "pending" (never swept). Dropped: pending entries are released, and unless
// `commit` ran, the copies are removed again (validation failure, vanished id).
pub(crate) struct StagedCopies {
    pub(crate) paths: Vec<String>,
    pub(crate) committed: bool,
}

impl StagedCopies {
    pub(crate) fn new() -> Self {
        StagedCopies { paths: Vec::new(), committed: false }
    }
    pub(crate) fn commit(&mut self) {
        self.committed = true;
    }
    pub(crate) fn stage(&mut self, files_dir: &std::path::Path, path: &str) -> String {
        store_attached_file_in(files_dir, path, Some(self))
    }
}

impl Drop for StagedCopies {
    fn drop(&mut self) {
        // While unwinding, the in-memory prompt may already point at the staged path;
        // keep the file and let a later sweep collect a true orphan.
        if !self.committed && !std::thread::panicking() {
            for p in &self.paths {
                let _ = fs::remove_file(p);
            }
        }
        let mut guard = PENDING_ATTACHMENTS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(set) = guard.as_mut() {
            for p in &self.paths {
                set.remove(&pending_key(p));
            }
        }
    }
}

// Copy an attached file into <data>/files so the prompt keeps working even if the
// original is moved or deleted. Returns the stored path, or the original unchanged
// on any problem or if it is already inside the store (never loses the reference).
pub(crate) fn store_attached_file_in(files_dir: &std::path::Path, path: &str, staged: Option<&mut StagedCopies>) -> String {
    if path.is_empty() {
        return String::new();
    }
    let src = std::path::Path::new(path);
    if src.starts_with(files_dir) || !src.is_file() {
        return path.to_string();
    }
    if fs::create_dir_all(files_dir).is_err() {
        return path.to_string();
    }
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let dest = files_dir.join(format!("{}_{}", gen_id(), name));
    let dest_str = dest.to_string_lossy().to_string();
    // Registered BEFORE the copy so a sweep can never see the file unprotected.
    if let Some(st) = staged {
        PENDING_ATTACHMENTS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_with(Default::default)
            .insert(pending_key(&dest_str));
        st.paths.push(dest_str.clone());
    }
    match fs::copy(src, &dest) {
        Ok(_) => dest_str,
        Err(_) => {
            let _ = fs::remove_file(&dest);
            path.to_string()
        }
    }
}

// Remove copies in <data>/files that no prompt references any more. Gated by the
// cleanupFiles flag (default on); only ever touches files inside our own store
// folder, so a user's originals elsewhere are never affected.
//
// Split into a cheap in-memory step (the referenced-file set, computed
// while the store lock is held) and this slow filesystem sweep (run AFTER the
// lock is released). The combined version used to run the whole fs::read_dir +
// remove_file scan while still holding the store mutex, freezing every other
// window's command on a large attachments folder. Callers: build the set with
// `orphan_cleanup_enabled` + `orphan_referenced_set` inside their locked block,
// then call this once the lock is dropped.
pub(crate) fn sweep_orphan_files(app: &AppHandle, referenced: &std::collections::HashSet<String>) {
    sweep_orphan_files_in(&data_dir(app).join("files"), referenced, || {
        app.try_state::<Db>().map(|state| {
            let store = lock(&state);
            orphan_referenced_set(&store)
        })
    });
}

pub(crate) fn sweep_orphan_files_in(
    files_dir: &std::path::Path,
    referenced: &std::collections::HashSet<String>,
    live_now: impl FnOnce() -> Option<std::collections::HashSet<String>>,
) {
    let entries = match fs::read_dir(files_dir) {
        Ok(e) => e,
        Err(_) => return, // no store folder yet -> nothing to clean
    };
    // Compare case-insensitively with normalized separators (Windows) so a still
    // referenced file is never mistaken for an orphan and deleted.
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && !referenced.contains(&norm(&path.to_string_lossy())) && !pending_attachments_contain(&path.to_string_lossy()) {
            candidates.push(path);
        }
    }
    if candidates.is_empty() {
        return;
    }
    // `referenced` is a snapshot from before the lock was released, and add/update copy
    // a NEW attachment into this same folder BEFORE the prompt that references it is in
    // the store (StagedCopies registers those copies as pending for the whole window).
    // Re-check every candidate against the store's CURRENT state and the pending set right
    // before deleting it: one more cheap in-memory HashSet build (no filesystem work under
    // the lock, so the whole point of splitting this sweep out still holds).
    let Some(live) = live_now() else { return };
    for path in candidates {
        if !live.contains(&norm(&path.to_string_lossy())) && !pending_attachments_contain(&path.to_string_lossy()) {
            let _ = fs::remove_file(&path);
        }
    }
}

pub(crate) fn orphan_cleanup_enabled(store: &Store) -> bool {
    store.settings.ui_flags.get("cleanupFiles").copied().unwrap_or(true)
}

// Recordings sit in <data>/recordings from the moment WGC finishes writing
// until the post-capture menu decides attach/save/discard -- attach then
// either copies the file into <data>/files (storeFiles on, covered by
// sweep_orphan_files above) or leaves file_path pointing straight at this
// folder (storeFiles off). Either way a recording the user never decided on
// (app closed with the post-capture menu still open, or before it ever
// showed) would sit here forever with nothing else to clean it up. Age-gated
// like sweep_stale_temp_files, not an immediate sweep like sweep_orphan_files
// above -- a recording still open in that very menu is legitimately
// unreferenced by any prompt yet, so only files older than STALE_TEMP_FILE_AGE
// are fair game. Runs once at startup, the same "crash/abnormal-exit
// leftovers" spot sweep_stale_temp_files already runs from.
// An mp4 that never reached its stop path has no top-level "moov" atom --
// a crash/abnormal-exit leftover, not just "maybe still in use". Walks only the
// box headers (size + fourcc), never the payload; no ffmpeg needed. size==1
// means a 64-bit "largesize" follows; size==0 means the box runs to EOF.
pub(crate) fn mp4_has_moov(path: &std::path::Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = fs::File::open(path) else { return false };
    let Ok(len) = f.metadata().map(|m| m.len()) else { return false };
    let mut pos: u64 = 0;
    let mut header = [0u8; 8];
    while pos + 8 <= len {
        if f.seek(SeekFrom::Start(pos)).is_err() || f.read_exact(&mut header).is_err() {
            return false;
        }
        let size32 = u32::from_be_bytes(header[0..4].try_into().unwrap()) as u64;
        let is_moov = &header[4..8] == b"moov";
        let box_size = match size32 {
            1 => {
                let mut ext = [0u8; 8];
                if f.read_exact(&mut ext).is_err() {
                    return false;
                }
                u64::from_be_bytes(ext)
            }
            0 => len.saturating_sub(pos),
            n => n,
        };
        if box_size < 8 {
            return false; // malformed -- stop rather than loop forever
        }
        // A moov box's declared size running past the actual file length
        // means the process crashed or was killed mid-write, right after the
        // header landed but before its payload did -- accepting the header
        // alone as "moov present" let exactly the truncated recordings this
        // check exists to catch slip through as healthy.
        if is_moov {
            return pos.saturating_add(box_size) <= len;
        }
        pos = pos.saturating_add(box_size);
    }
    false
}

// Only mp4 gets the moov check -- the only format the recorder ever writes
// before an optional post-stop conversion. A non-mp4 file here is either
// mid-conversion output or something else, neither of which this sweep judges.
pub(crate) fn recording_is_broken(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mp4")) && !mp4_has_moov(path)
}

pub(crate) fn sweep_stale_recordings(
    app: &AppHandle,
    referenced: &std::collections::HashSet<String>,
    broken_sweep_on: bool,
    broken_grace: Duration,
    retention_age: Option<Duration>,
) -> (u32, u64) {
    sweep_stale_recordings_in(&data_dir(app).join("recordings"), referenced, &[], broken_sweep_on, broken_grace, retention_age)
}

// Split out from sweep_stale_recordings so the actual sweep logic is testable
// against a plain tempdir -- no AppHandle needed, same split as
// convert_recording/convert_recording_blocking above. Three independent
// reasons to remove a file: it is old AND nobody has decided on it yet, it is
// a broken (moov-less) mp4 past a much shorter grace period, or it is past
// the retention window (90 days unless changed) -- either way every path in
// `protected` (the currently recording session's own output + mic sidecar,
// plus the video editor's currently open source + its own sidecar, if any)
// is excluded unconditionally, never by age or state, because deleting a
// file mid-capture or out from under an open editor would be a far worse bug
// than the one this sweep exists to fix. The retention window only ever
// touches an UNREFERENCED file, same as the other two reasons -- a recording
// still attached to a saved prompt is never a candidate, no matter its age.
pub(crate) fn sweep_stale_recordings_in(
    dir: &Path,
    referenced: &std::collections::HashSet<String>,
    protected: &[PathBuf],
    broken_sweep_on: bool,
    broken_grace: Duration,
    retention_age: Option<Duration>,
) -> (u32, u64) {
    let Ok(entries) = fs::read_dir(dir) else { return (0, 0) };
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    let mut count = 0u32;
    let mut bytes = 0u64;
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_file() || protected.iter().any(|p| p == &path) || camera::is_kept_abandoned(&path) {
            continue;
        }
        // A "<name>.mic.wav" sidecar is referenced whenever its own
        // recording is -- the mp4 may already be attached to a prompt while
        // the editor has not yet consolidated the two into one two-track file.
        // A "<name>.cam.json" sidecar follows the same rule for its own
        // video (webm or mp4) -- without this, a camera recording's sidecar
        // aged out and vanished an hour after it was taken even though the
        // recording itself was still attached to a saved prompt.
        let is_referenced = referenced.contains(&norm(&path.to_string_lossy()))
            || mic_sidecar_video_path(&path).is_some_and(|v| referenced.contains(&norm(&v.to_string_lossy())))
            || keys_sidecar_video_path(&path).is_some_and(|v| referenced.contains(&norm(&v.to_string_lossy())))
            || camera::sidecar_video_path(&path).is_some_and(|v| referenced.contains(&norm(&v.to_string_lossy())));
        let Ok(meta) = e.metadata() else { continue };
        let age = meta.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok());
        let stale_undecided = !is_referenced && age.is_some_and(|a| a >= STALE_TEMP_FILE_AGE);
        let broken = broken_sweep_on && !is_referenced && age.is_some_and(|a| a >= broken_grace) && recording_is_broken(&path);
        let aged_out = !is_referenced && retention_age.is_some_and(|r| age.is_some_and(|a| a >= r));
        if stale_undecided || broken || aged_out {
            let size = meta.len();
            if fs::remove_file(&path).is_ok() {
                count += 1;
                bytes += size;
            }
        }
    }
    (count, bytes)
}

// Startup's own one-shot result (Db is not managed yet at that point in
// setup(), and no window exists to toast into) -- the frontend collects it
// once, early in its own init, via take_recording_sweep_notice().
pub(crate) static PENDING_SWEEP_NOTICE: Mutex<Option<(u32, u64)>> = Mutex::new(None);

#[tauri::command]
fn take_recording_sweep_notice() -> Option<(u32, u64)> {
    PENDING_SWEEP_NOTICE.lock().unwrap_or_else(|e| e.into_inner()).take()
}

// How long a recording may sit unfinished before the sweep treats it as broken
// (minutes), and how often the periodic sweep runs (minutes).
pub(crate) const REC_BROKEN_GRACE_MIN: f64 = 5.0;
pub(crate) const REC_SWEEP_EVERY_MIN: f64 = 60.0;

pub(crate) fn recording_sweep_grace(app: &AppHandle) -> (bool, Duration) {
    app.try_state::<Db>()
        .map(|s| {
            let store = s.lock().unwrap_or_else(|e| e.into_inner());
            let on = store.settings.ui_flags.get("recSweepBroken") != Some(&false);
            (on, Duration::from_secs((REC_BROKEN_GRACE_MIN * 60.0) as u64))
        })
        .unwrap_or((true, Duration::from_secs(5 * 60)))
}

// How long a KEPT recording (referenced or not) is allowed to age before
// an unreferenced one becomes fair game for the sweep above -- 0 disables this
// leg; a new install ships 90 days.
pub(crate) fn recording_retention_age(app: &AppHandle) -> Option<Duration> {
    let days = app
        .try_state::<Db>()
        .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_values.get("recordingRetentionDays").copied().unwrap_or(RECORDING_RETENTION_DAYS_DEFAULT))
        .unwrap_or(0.0);
    if days <= 0.0 {
        None
    } else {
        Some(Duration::from_secs(days as u64 * 86_400))
    }
}

// Runs every REC_SWEEP_EVERY_MIN minutes.
pub(crate) fn recording_sweep_interval() -> Duration {
    Duration::from_secs((REC_SWEEP_EVERY_MIN * 60.0) as u64)
}

// Shared by the periodic job and "once a recording ends": both run after
// Db/RecordingState are managed and a window exists, so both can just
// emit -- unlike the startup path above, which cannot.
pub(crate) fn sweep_recordings_and_notify(app: &AppHandle) {
    let (count, bytes) = sweep_recordings_now_result(app);
    if count > 0 {
        append_crash_log(&format!("recording sweep: removed {count} file(s), {bytes} bytes"));
        let _ = app.emit("recording-sweep-done", serde_json::json!({ "count": count, "bytes": bytes }));
    }
}

pub(crate) fn sweep_recordings_now_result(app: &AppHandle) -> (u32, u64) {
    let Some(state) = app.try_state::<Db>() else { return (0, 0) };
    let referenced = orphan_referenced_set(&lock(&state));
    let (broken_on, grace) = recording_sweep_grace(app);
    let (mut count, mut bytes) = sweep_stale_recordings_in(
        &data_dir(app).join("recordings"),
        &referenced,
        &protected_recording_paths(app),
        broken_on,
        grace,
        recording_retention_age(app),
    );
    let (c, b) = sweep_discarded_recordings(app);
    count += c;
    bytes += b;
    (count, bytes)
}

// A discarded recording sits in its folder's own "verworfen" subfolder
// -- unconditionally stale-eligible past recDiscardKeepH, no "referenced"
// check needed (a discarded recording is by definition attached to nothing).
// recDiscardKeepH <= 0 disables this leg entirely, keeping every discard
// around until someone clears it by hand. Runs against BOTH places a working
// recording can live (data_dir's own recordings folder, and the current
// recordingDir, default or custom) -- discard_recording_file itself accepts
// either, so the trash this sweeps must cover both too.
pub(crate) fn sweep_discarded_recordings(app: &AppHandle) -> (u32, u64) {
    let Some(state) = app.try_state::<Db>() else { return (0, 0) };
    let (keep_h, custom) = {
        let g = lock(&state);
        (g.settings.ui_values.get("recDiscardKeepH").copied().unwrap_or(24.0), g.settings.ui_texts.get("recordingDir").cloned().unwrap_or_default())
    };
    if keep_h <= 0.0 {
        return (0, 0);
    }
    let age = Duration::from_secs((keep_h * 3600.0) as u64);
    let default_dir = data_dir(app).join("recordings");
    let custom_dir = recording_dir(&custom);
    let mut count = 0u32;
    let mut bytes = 0u64;
    for dir in [default_dir.clone(), custom_dir].into_iter().collect::<std::collections::HashSet<_>>() {
        let (c, b) = discarded_stale_in(&dir.join("verworfen"), age);
        count += c;
        bytes += b;
    }
    (count, bytes)
}

// The discarded-recordings row's own on-demand trigger. sweep_recordings_now
// below answers for the recordings row instead -- it also runs this same
// sweep as part of its combined total, so a dedicated command here keeps the
// two rows from double-reporting or clearing each other's numbers.
#[tauri::command]
fn sweep_discarded_recordings_now(app: AppHandle) -> (u32, u64) {
    sweep_discarded_recordings(&app)
}

pub(crate) fn discarded_stale_in(dir: &Path, age: Duration) -> (u32, u64) {
    let Ok(entries) = fs::read_dir(dir) else { return (0, 0) };
    let mut count = 0u32;
    let mut bytes = 0u64;
    for e in entries.flatten() {
        let path = e.path();
        if !is_video_folder_file(&path) { continue; }
        let Ok(meta) = e.metadata() else { continue };
        let Some(elapsed) = meta.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok()) else { continue };
        if elapsed < age { continue; }
        let size = meta.len();
        if fs::remove_file(&path).is_ok() {
            count += 1;
            bytes += size;
            let _ = fs::remove_file(path.with_extension("mic.wav"));
            // A discarded take's key-overlay sidecar used to be
            // left behind here, orphaned in the trash forever once its video
            // was purged.
            let _ = fs::remove_file(path.with_extension("keys.json"));
            let _ = fs::remove_file(camera::sidecar_path(&path));
        }
    }
    (count, bytes)
}

// 281's "Jetzt aufräumen" -- runs the same sweep on demand and always reports
// what it did, even when that is nothing: an explicit request needs an answer
// either way, unlike the silent background sweep above.
#[tauri::command]
fn sweep_recordings_now(app: AppHandle) -> (u32, u64) {
    sweep_recordings_now_result(&app)
}

// Every path the sweep must never touch beyond what `referenced` already
// covers: the actively-recording session's own output and mic sidecar (a
// session in progress has no db entry yet), and the video editor's currently
// open source and its own not-yet-muxed-in sidecar -- neither used to be
// excluded, so a recording left open in the editor (or its sidecar, on any
// recording) for longer than the stale/broken grace period was fair game for
// the very next sweep.
pub(crate) fn protected_recording_paths(app: &AppHandle) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = active_recording_path(app) {
        out.push(p.with_extension("mic.wav"));
        #[cfg(windows)]
        out.push(keylog::sidecar_path(&p)); // the key-overlay sidecar, same lifetime as the mic one
        out.push(p);
    }
    if let Some(state) = app.try_state::<VideoEditorPending>() {
        let (src, _origin_dir, overlay) = state.lock().clone();
        if !src.is_empty() {
            let src = PathBuf::from(src);
            out.push(src.with_extension("mic.wav"));
            #[cfg(windows)]
            out.push(keylog::sidecar_path(&src));
            out.push(src);
        }
        if !overlay.is_empty() {
            let overlay = PathBuf::from(overlay);
            out.push(camera::sidecar_path(&overlay));
            out.push(overlay);
        }
    }
    out
}

#[cfg(windows)]
pub(crate) fn active_recording_path(app: &AppHandle) -> Option<PathBuf> {
    app.try_state::<RecordingState>()?.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.output_path.clone())
}
#[cfg(not(windows))]
pub(crate) fn active_recording_path(_app: &AppHandle) -> Option<PathBuf> {
    None
}

// A trashed prompt still owns its files until it is actually purged:
// leaving trash out of this scan would let an UNRELATED cleanup pass
// (triggered by some other prompt's add/delete) sweep away a file a trashed
// prompt still needs to be restorable.
pub(crate) fn orphan_referenced_set(store: &Store) -> std::collections::HashSet<String> {
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    store
        .prompts
        .iter()
        .chain(store.trash.iter())
        .flat_map(|p| [p.file_path.clone(), p.icon_path.clone()])
        .filter(|s| !s.is_empty())
        .map(|s| norm(&s))
        .collect()
}

// Async: persisting all prompts (base64 images) to SQLite must not block the UI
// thread: a sync command runs on it and froze the window while saving.
// Every field the frontend's editor form can set arrives as its own parameter,
// mirroring update_prompt's shape one to one -- a struct wrapper would just move
// the same field list one level down.
#[allow(clippy::too_many_arguments)]
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
    rotate_auto: Option<bool>,
) -> Result<Prompt, String> {
    // The slow steps (copying a large attachment, ffprobe, OCR) run before the
    // store lock is taken, so every other window keeps working meanwhile. The
    // lock is taken again only to validate and insert.
    let (store_files, ocr_index_on, max_chars) = {
        let store = lock(&state);
        (
            store_files_enabled(&store),
            store.settings.ui_flags.get("ocrIndex").copied().unwrap_or(false),
            ocr_index_max_chars(&store.settings),
        )
    };
    let mut fp = file_path.unwrap_or_default();
    let mut ip = icon_path.unwrap_or_default();
    let mut staged_copies = StagedCopies::new();
    if store_files {
        let files_dir = data_dir(&app).join("files");
        fp = staged_copies.stage(&files_dir, &fp);
        ip = staged_copies.stage(&files_dir, &ip);
    }
    // Measured once, right here, never again on a render.
    let media_source = if !fp.is_empty() { Some(fp.as_str()) } else if !ip.is_empty() { Some(ip.as_str()) } else { None };
    let (media_duration_ms, media_width, media_height, media_size_bytes) = match media_source {
        Some(p) => probe_media_meta(ffprobe_search_path(&app).as_deref(), Path::new(p)),
        None => (0, 0, 0, 0),
    };
    let image_value = image.unwrap_or_default();
    // Read once and indexed once, right here, on purpose: a search index that
    // costs an OCR pass on every render would be exactly the defect this is
    // meant not to be.
    let image_text = resolved_image_text(&image_value, ocr_index_on, |img| ocr_index_text(img, max_chars));
    let prompt = {
        let mut store = lock(&state);
        let tags = sanitize_tags(tags.unwrap_or_default(), &canonical_tags(&store.prompts))?;
        let notes = sanitize_notes(&notes.unwrap_or_default())?;
        // The OCR above ran against a flag snapshot: text only exists while the index is on.
        let image_text = if ocr_index_flag_on(&store) { image_text } else { String::new() };
        let variants = sanitize_variants(variants.unwrap_or_default());
        let rotate_mode = sanitize_rotate_mode(rotate_mode.unwrap_or_default(), &variants);
        let rotate_auto = sanitize_rotate_auto(rotate_auto.unwrap_or(false), &variants);
        let others: Vec<&Prompt> = store.prompts.iter().collect();
        let rec_stop_hk = active_rec_hotkey(&store.settings.rec_stop_hotkey, "recStopHotkey", &store.settings.ui_flags);
        let rec_pause_hk = active_rec_hotkey(&store.settings.rec_pause_hotkey, "recPauseHotkey", &store.settings.ui_flags);
        let hotkey = sanitize_hotkey(&hotkey.unwrap_or_default(), &store.settings.hotkey, rec_stop_hk, rec_pause_hk, &others)?;
        let hotkey_paste = if hotkey.is_empty() {
            String::new() // no binding, no action to remember for it
        } else {
            sanitize_hotkey_mode(&hotkey_paste.unwrap_or_default())
        };
        let prompt = Prompt {
            id: gen_id(),
            trashed_at: 0,
            name,
            text,
            color,
            image: image_value,
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
            rotate_auto,
            media_duration_ms,
            media_width,
            media_height,
            media_size_bytes,
            image_text,
            secret: false,
        };
        // A grid folder owns its cell OUTSIDE view.layouts, and the renderer gives a
        // folder priority: so a cell handed out without excluding them saves fine and
        // then never draws.
        let folder_cells = active_view_folder_cells(&store.settings);
        let view = store.settings.active_view_mut();
        if let Some(cell) = first_free_cell_excluding(view, &folder_cells) {
            let key = grid_key(view.cols, view.rows);
            view.layouts.entry(key).or_default().insert(prompt.id.clone(), cell);
        }
        view.members.push(prompt.id.clone());
        store.prompts.push(prompt.clone());
        if !save_prompts(&app, &store) {
            // Nothing actually reached disk (or the JSON fallback) - undo the
            // in-memory add and report it, instead of handing back an id the
            // editor will believe is safely saved. The layout entry above was
            // never persisted (save_settings below is skipped too), so it is
            // harmless to leave dangling in memory: no prompt id means nothing
            // ever draws it. `members` gets the same undo so it never names an
            // id that store.prompts no longer has.
            store.prompts.pop();
            store.settings.active_view_mut().members.retain(|m| *m != prompt.id);
            return Err("save-failed".into());
        }
        staged_copies.commit();
        save_settings(&app, &store.settings);
        prompt
    };
    rebuild_tray(&app);
    // Arm the new accelerator. The re-arm drops one the OS refuses, so the
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

// One example prompt as the frontend supplies it: name/text come from i18n.js
// (resolved to whatever language the UI is actually showing), the image is the
// one small embedded asset shared by all languages.
#[derive(Deserialize)]
pub(crate) struct NewExample {
    pub(crate) name: String,
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) image: String,
}

// Seeds the three onboarding examples exactly once, on a genuinely fresh install:
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
                rotate_auto: false,
                media_duration_ms: 0,
                media_width: 0,
                media_height: 0,
                media_size_bytes: 0,
                // Onboarding examples are never OCR-indexed -- ocrIndex ships off,
                // and seeding runs before the user has ever seen the switch.
                image_text: String::new(),
                secret: false,
            };
            let view = store.settings.active_view_mut();
            if let Some(cell) = first_free_cell(view) {
                let key = grid_key(view.cols, view.rows);
                view.layouts.entry(key).or_default().insert(prompt.id.clone(), cell);
            }
            view.members.push(prompt.id.clone());
            store.settings.example_prompt_ids.push(prompt.id.clone());
            store.settings.example_prompt_seeds.insert(
                prompt.id.clone(),
                example_seed_fingerprint(&prompt.name, &prompt.text, &prompt.image),
            );
            store.prompts.push(prompt.clone());
            created.push(prompt);
        }
        store.settings.examples_seeded = true;
        // A failed save must not hand the UI prompts that never reached disk:
        // roll every in-memory change back (view placement included) so a restart
        // does not silently disagree with what was just shown. The two saves are
        // rolled back independently: if save_prompts already landed the new prompts,
        // leaving them in store.prompts is what keeps memory matching disk: undoing
        // them here too would make the NEXT unrelated save silently erase a write
        // that already succeeded, and (since examples_seeded would go back to false)
        // let a retry seed a second, duplicate batch on top of the first.
        let prompts_ok = save_prompts(&app, &store);
        let settings_ok = save_settings(&app, &store.settings);
        if !prompts_ok || !settings_ok {
            let ids: std::collections::HashSet<String> = created.iter().map(|p| p.id.clone()).collect();
            if !prompts_ok {
                store.prompts.retain(|p| !ids.contains(&p.id));
            }
            if !settings_ok {
                store.settings.example_prompt_ids.retain(|id| !ids.contains(id));
                store.settings.example_prompt_seeds.retain(|id, _| !ids.contains(id));
                store.settings.examples_seeded = false;
                let view = store.settings.active_view_mut();
                let key = grid_key(view.cols, view.rows);
                if let Some(layout) = view.layouts.get_mut(&key) {
                    layout.retain(|id, _| !ids.contains(id));
                }
                view.members.retain(|id| !ids.contains(id));
            }
            return Err("save-failed".into());
        }
        created
    };
    rebuild_tray(&app); // Mirrors add_prompt: the tray's recent/fav sections can list these too
    Ok(created)
}

// Of `ids` (normally settings.example_prompt_ids), which are still unedited: no
// entry in `seeds` (old install, seeded before this field existed) counts as
// unchanged, same as an id whose prompt is already gone some other way. Shared by
// dismiss_example_prompts and example_prompts_pending so the two can never disagree.
pub(crate) fn examples_still_pristine<'a>(
    ids: impl IntoIterator<Item = &'a String>,
    seeds: &HashMap<String, u64>,
    prompts: &[Prompt],
) -> Vec<String> {
    ids.into_iter()
        .filter(|id| match seeds.get(*id) {
            None => true,
            Some(&seed) => prompts
                .iter()
                .find(|p| &p.id == *id)
                .map(|p| example_seed_fingerprint(&p.name, &p.text, &p.image) == seed)
                .unwrap_or(true),
        })
        .cloned()
        .collect()
}

// "Remove examples": deletes exactly the ids seed_example_prompts recorded and
// still not edited by the user, through the same batch-delete path (→ trash) a
// manual multi-select delete would use, so nothing about this is a special,
// less-safe deletion.
#[tauri::command]
async fn dismiss_example_prompts(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    let ids_to_delete = {
        let mut store = lock(&state);
        let ids = std::mem::take(&mut store.settings.example_prompt_ids);
        let seeds = std::mem::take(&mut store.settings.example_prompt_seeds);
        examples_still_pristine(&ids, &seeds, &store.prompts)
    };
    if ids_to_delete.is_empty() {
        return Ok(());
    }
    batch_prompts(app, state, ids_to_delete, "delete".to_string(), None, None, None, None, None)
        .await
        .map(|_| ())
}

// Count for the "Remove examples (n)" button label: how many seeded examples would
// actually be deleted right now, same rule dismiss_example_prompts itself applies.
#[tauri::command]
fn example_prompts_pending(state: State<Db>) -> usize {
    let store = lock(&state);
    examples_still_pristine(&store.settings.example_prompt_ids, &store.settings.example_prompt_seeds, &store.prompts).len()
}

// A save request carries the image only when the user actually touched it: no field
// means "leave the stored picture alone", and the only way to clear one is the
// editor's remove button, which sends an explicit empty string. Images no longer
// ride along with every read of the prompt list, so a save must never be able to
// mistake "not loaded here" for "the user deleted it".
pub(crate) fn merge_image(stored: &str, incoming: Option<String>) -> String {
    match incoming {
        Some(img) => img,
        None => stored.to_string(),
    }
}

// Async: same reason as add_prompt; the SQLite write stays off the UI thread.
// Same shape as add_prompt, for the same reason -- one parameter per editable field.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn update_prompt(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    name: String,
    text: String,
    color: String,
    // Absent = keep whatever is stored. The editor only sends this field when the
    // user actually changed the picture, so an image that never finished loading
    // can no longer come back as an empty string and erase the stored one; an
    // explicit `Some("")` is the "remove image" click and nothing else.
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
    // Option, unlike variants/rotate_mode: added in 90, the editor UI wiring
    // (frontend) lands only in 92; an interim caller that omits it must not
    // reset an unrelated field, so a missing value keeps the prompt's own.
    rotate_auto: Option<bool>,
) -> Result<Option<Prompt>, String> {
    let variants = sanitize_variants(variants);
    let rotate_mode = sanitize_rotate_mode(rotate_mode, &variants);
    // Copying an attachment, probing it and OCR-ing a new picture all happen
    // before the store lock is taken, against a short snapshot of what the
    // prompt holds now. The locked section below uses those results only when
    // the prompt still holds exactly that (a concurrent edit falls back to doing
    // the work inline, as before).
    let (store_files, staged_ocr_on, staged_ocr_max, staged_cur) = {
        let store = lock(&state);
        let cur = store.prompts.iter().find(|p| p.id == id).map(|p| (p.image.clone(), p.file_path.clone(), p.icon_path.clone()));
        (
            store_files_enabled(&store),
            store.settings.ui_flags.get("ocrIndex").copied().unwrap_or(false),
            ocr_index_max_chars(&store.settings),
            cur,
        )
    };
    let mut staged_copies = StagedCopies::new();
    let files_dir = data_dir(&app).join("files");
    let file_path = file_path.map(|p| if store_files { staged_copies.stage(&files_dir, &p) } else { p });
    let icon_path = icon_path.map(|p| if store_files { staged_copies.stage(&files_dir, &p) } else { p });
    let ffprobe = ffprobe_search_path(&app);
    let staged_image_text = staged_cur.as_ref().and_then(|(cur_img, _, _)| {
        image.as_deref().is_some_and(|v| v != cur_img).then(|| {
            resolved_image_text(&merge_image(cur_img, image.clone()), staged_ocr_on, |img| ocr_index_text(img, staged_ocr_max))
        })
    });
    let staged_media = staged_cur.as_ref().and_then(|(_, cur_fp, cur_ip)| {
        let changed = file_path.as_deref().is_some_and(|v| v != cur_fp) || icon_path.as_deref().is_some_and(|v| v != cur_ip);
        if !changed {
            return None;
        }
        let fp_now = file_path.clone().unwrap_or_else(|| cur_fp.clone());
        let ip_now = icon_path.clone().unwrap_or_else(|| cur_ip.clone());
        let source = if !fp_now.is_empty() { Some(fp_now) } else if !ip_now.is_empty() { Some(ip_now) } else { None };
        Some(match source {
            Some(mp) => probe_media_meta(ffprobe.as_deref(), Path::new(&mp)),
            None => (0, 0, 0, 0),
        })
    });
    let updated = {
        let mut store = lock(&state);
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
                let rec_stop_hk = active_rec_hotkey(&store.settings.rec_stop_hotkey, "recStopHotkey", &store.settings.ui_flags);
                let rec_pause_hk = active_rec_hotkey(&store.settings.rec_pause_hotkey, "recPauseHotkey", &store.settings.ui_flags);
                Some(sanitize_hotkey(&raw, &store.settings.hotkey, rec_stop_hk, rec_pause_hk, &others)?)
            }
            None => None,
        };
        // Read before the mutable prompt borrow below (a MutexGuard can't be
        // split into disjoint field borrows) -- only actually run if the
        // attachment turns out to have changed.
        let ocr_index_on = store.settings.ui_flags.get("ocrIndex").copied().unwrap_or(false);
        let ocr_max_chars = ocr_index_max_chars(&store.settings);
        // Read history settings before the mutable prompt borrow (a MutexGuard
        // can't be split into disjoint field borrows).
        let hist_on = history_enabled(&store.settings);
        let hist_cap = history_cap(&store.settings);
        // Same "read before the mutable prompt borrow" rule as ffprobe/ocr/hist
        // above -- needed below to re-encrypt a confidential entry's edited text
        // instead of overwriting its ciphertext with the editor's plain value.
        let secret_key = secret_session_key(secret_timeout_min(&store.settings));
        let found = store.prompts.iter_mut().find(|p| p.id == id);
        match found {
            Some(p) => {
                // Full pre-edit snapshot so a failed save can roll the live entry
                // back instead of leaving the UI showing unsaved changes as if they
                // had taken (they would silently revert on the next restart). Also
                // doubles as the version-history snapshot source below.
                let original = p.clone();
                // The staged results above are valid only for the state they were
                // computed against.
                let staged_valid = staged_cur.as_ref().is_some_and(|(i, f, c)| *i == p.image && *f == p.file_path && *c == p.icon_path);
                // A confidential entry's text/variants must never land in storage
                // as whatever the editor sent -- that is the plain text while
                // unlocked, so it is re-encrypted here before it touches p; while
                // locked there is no key to encrypt with, so the whole save is
                // refused rather than silently discarding the real ciphertext.
                let (text, variants) = if p.secret {
                    let Some(key) = secret_key else {
                        return Err("secret-locked".to_string());
                    };
                    let enc_variants = variants.iter().map(|v| secret_encrypt(v, &key)).collect();
                    (secret_encrypt(&text, &key), enc_variants)
                } else {
                    (text, variants)
                };
                p.name = name;
                p.text = text;
                p.color = color;
                p.variants = variants;
                p.rotate_mode = rotate_mode;
                if let Some(auto) = rotate_auto { p.rotate_auto = sanitize_rotate_auto(auto, &p.variants); }
                // Same rule as the attachment path above: only a real change to the
                // picture re-indexes it. Cleared the moment the picture is gone,
                // re-run (only while ocrIndex is on) the moment it is replaced --
                // an old picture's leftover text must never survive under a new one.
                let image_changed = image.as_deref().is_some_and(|v| v != p.image);
                p.image = merge_image(&p.image, image);
                if image_changed {
                    p.image_text = match &staged_image_text {
                        Some(t) if staged_valid && staged_ocr_on == ocr_index_on && staged_ocr_max == ocr_max_chars => t.clone(),
                        _ => resolved_image_text(&p.image, ocr_index_on, |img| ocr_index_text(img, ocr_max_chars)),
                    };
                }
                if let Some(si) = show_image { p.show_image = si; }
                if let Some(ci) = copy_image { p.copy_image = ci; }
                // A real change to either path re-measures the attachment -- a save
                // that never touched it (every other field edit) must not re-run
                // ffprobe/image::image_dimensions on every keystroke-triggered save.
                let attachment_changed = file_path.as_deref().is_some_and(|v| v != p.file_path)
                    || icon_path.as_deref().is_some_and(|v| v != p.icon_path);
                if let Some(fp) = file_path { p.file_path = fp; }
                if let Some(pp) = pdf_page { p.pdf_page = pp; }
                if let Some(ip) = icon_path { p.icon_path = ip; }
                if attachment_changed {
                    let (d, w, h, sz) = match staged_media {
                        Some(m) if staged_valid => m,
                        _ => {
                            let media_source = if !p.file_path.is_empty() { Some(p.file_path.as_str()) } else if !p.icon_path.is_empty() { Some(p.icon_path.as_str()) } else { None };
                            match media_source {
                                Some(mp) => probe_media_meta(ffprobe.as_deref(), Path::new(mp)),
                                None => (0, 0, 0, 0),
                            }
                        }
                    };
                    p.media_duration_ms = d;
                    p.media_width = w;
                    p.media_height = h;
                    p.media_size_bytes = sz;
                }
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
                // Only a real change re-arms; every save would otherwise churn
                // through unregister/register for nothing.
                let rebind = matches!(&hotkey, Some(h) if *h != p.hotkey);
                if let Some(h) = hotkey { p.hotkey = h; }
                if p.hotkey.is_empty() { p.hotkey_paste.clear(); } // no binding, no action
                let clone = p.clone();
                // Nothing reached disk (or the JSON fallback) - restore the pre-edit
                // snapshot instead of handing back an id the editor will believe is
                // safely saved.
                if !save_prompts(&app, &store) {
                    if let Some(slot) = store.prompts.iter_mut().find(|x| x.id == id) {
                        *slot = original;
                    }
                    return Err("save-failed".into());
                }
                staged_copies.commit();
                // Snapshot the PRE-edit name/text/variants only once the
                // edit is durably saved: inserting it before save_prompts (the old
                // order) left a version row for an edit that never actually happened
                // whenever the save then failed, and could push real history out past
                // hist_cap for nothing. Entries ARE content, so editing
                // only entry 2 of a rotating prompt (name/text untouched) must still
                // get a row or a restore could never bring that entry back. rotate_mode
                // is deliberately excluded (order/random is a setting, not content).
                if hist_on && (original.name != clone.name || original.text != clone.text || original.variants != clone.variants) {
                    match db_conn(&app) {
                        // A failure here loses one version snapshot, not the edit itself
                        // (which is already durably saved above): not worth failing the
                        // whole edit over, but silent would mean a restore point just
                        // vanishes with no trace of why.
                        Some(c) => {
                            if let Err(e) = db_add_version(&c, &id, now_secs(), &original.name, &original.text, &original.variants, hist_cap) {
                                append_crash_log(&format!("version history insert failed for prompt {id}: {e}"));
                            }
                        }
                        None => append_crash_log(&format!("version history insert skipped for prompt {id}: no DB connection")),
                    }
                }
                // Only sweep the old attachment once the new reference is durably
                // saved -- otherwise a failed save leaves the DB pointing at a file the
                // sweep just deleted, with no fallback to recover it from.
                // Only the cheap in-memory part runs while the lock is held; the
                // actual filesystem sweep happens below, once the lock is released.
                let orphan_scan = orphan_cleanup_enabled(&store).then(|| orphan_referenced_set(&store));
                let scale = float_scale_of(&store.settings, &id);
                Some((clone, scale, rebind, orphan_scan))
            }
            None => None,
        }
    };
    let (updated, scale, rebind, orphan_scan) = match updated {
        Some((p, s, r, o)) => (Some(p), s, r, o),
        None => (None, 1.0, false, None),
    };
    // The actual filesystem sweep, deferred until the store lock (dropped
    // when the block above ended) is no longer held.
    if let Some(referenced) = orphan_scan {
        sweep_orphan_files(&app, &referenced);
    }
    // Re-arm outside the lock, then hand back what is actually stored: the re-arm
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
        rebuild_tray(&app); // A rename shows up in the tray entries
        // An open pill switches between text pill and image box live.
        if let Some(win) = app.get_webview_window(&flabel(&p.id)) {
            let (w, h) = pill_dims(is_image_prompt(p), scale);
            let _ = win.set_size(tauri::LogicalSize::new(w, h));
        }
    }
    Ok(updated)
}

// List a prompt's version snapshots (newest first).
#[tauri::command]
async fn list_versions(app: AppHandle, prompt_id: String) -> Result<Vec<PromptVersion>, String> {
    Ok(db_conn(&app).map(|c| db_list_versions(&c, &prompt_id)).unwrap_or_default())
}

// The library's "last edited" sort mode. A prompt missing from the map has
// never had a version row (see db_last_edited): the frontend sorts those last.
#[tauri::command]
async fn last_edited_map(app: AppHandle) -> Result<std::collections::HashMap<String, u64>, String> {
    Ok(db_conn(&app).map(|c| db_last_edited(&c)).unwrap_or_default())
}

// Restore a snapshot. The current state is snapshotted first, so a restore is
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
        // Same "read before the mutable prompt borrow" rule as update_prompt.
        let secret_key = secret_session_key(secret_timeout_min(&store.settings));
        if let Some(p) = store.prompts.iter_mut().find(|p| p.id == pid) {
            let original = p.clone();
            // 35a: the undo snapshot of the CURRENT state must itself succeed before
            // overwriting it: silently proceeding on failure would make a restore
            // that can never itself be undone, breaking the "nothing is ever lost"
            // promise this command exists for.
            let snapshot_ok = db_conn(&app)
                .map(|c| db_add_version(&c, &pid, now_secs(), &p.name, &p.text, &p.variants, cap).is_ok())
                .unwrap_or(false);
            if !snapshot_ok {
                return Err("snapshot-failed".into());
            }
            // A snapshot taken before the entry ever became confidential holds
            // plain text; one taken afterwards holds ciphertext already readable
            // with the current vault key. Either way the entry stays confidential
            // after a restore, so a plain-text snapshot is encrypted here instead
            // of landing on disk as-is, and an already-ciphertext one is kept
            // rather than encrypted a second time.
            let (text, variants) = if p.secret {
                let Some(key) = secret_key else {
                    return Err("secret-locked".to_string());
                };
                let variants = variants.iter().map(|v| secret_encrypt_if_plain(v, &key)).collect();
                (secret_encrypt_if_plain(&text, &key), variants)
            } else {
                (text, variants)
            };
            p.name = name;
            p.text = text;
            // The restored list is our own history, but re-validate anyway (a
            // version predating MAX_ROTATE_VARIANTS, however unlikely, must not
            // slip a too-long list past the same floor every other entry point has).
            p.variants = sanitize_variants(variants);
            // A restore that lands on zero entries must not leave "random" mode
            // stranded on an empty list (same invariant as everywhere else).
            p.rotate_mode = sanitize_rotate_mode(p.rotate_mode.clone(), &p.variants);
            p.rotate_auto = sanitize_rotate_auto(p.rotate_auto, &p.variants);
            let clone = p.clone();
            // 35a/38: a failed save must roll the live entry back, same pattern as
            // update_prompt (7): otherwise the UI shows a restored prompt that
            // silently reverts to the pre-restore text on the next launch.
            if !save_prompts(&app, &store) {
                if let Some(slot) = store.prompts.iter_mut().find(|x| x.id == pid) {
                    *slot = original;
                }
                return Err("save-failed".into());
            }
            Some(clone)
        } else {
            None
        }
    };
    if let Some(p) = &restored {
        let _ = app.emit("prompt-updated", p.clone());
        rebuild_tray(&app);
    }
    Ok(restored)
}

// The prompt "restore as new" builds from a version snapshot. Name and text/variants
// come from the version itself; colour, image and tags come from the CURRENT prompt
// instead, because prompt_versions never stored them (see PromptVersion): there is
// nothing "older" to restore for any of the three. Everything else starts fresh, the
// same as any other new entry, so this stays a small addition next to add_prompt
// rather than a second copy of its whole field list.
pub(crate) fn build_restored_prompt(source: &Prompt, version_name: &str, version_text: String, version_variants: Vec<String>, suffix: &str) -> Prompt {
    let variants = sanitize_variants(version_variants);
    let rotate_mode = sanitize_rotate_mode(String::new(), &variants);
    let name = if suffix.is_empty() { version_name.to_string() } else { format!("{version_name} {suffix}") };
    Prompt {
        id: gen_id(),
        trashed_at: 0,
        name,
        text: version_text,
        color: source.color.clone(),
        image: source.image.clone(),
        show_image: source.show_image,
        copy_image: source.copy_image,
        file_path: String::new(),
        pdf_page: 0,
        icon_path: String::new(),
        caption: String::new(),
        caption_size: 0,
        font: String::new(),
        font_size: 0,
        favorite: false,
        tags: source.tags.clone(),
        notes: String::new(),
        hotkey: String::new(),
        hotkey_paste: String::new(),
        variants,
        rotate_mode,
        rotate_auto: false,
        // No attachment is carried over either (file_path/icon_path above), so
        // there is nothing for any of these to describe.
        media_duration_ms: 0,
        media_width: 0,
        media_height: 0,
        media_size_bytes: 0,
        image_text: String::new(),
        // "Restore as new" always starts a fresh, non-confidential entry -- the
        // snapshot's own text may or may not still be readable ciphertext
        // depending on when it was taken, and carrying `secret` over without the
        // vault key to match it would leave a confidential-flagged entry holding
        // whatever that old text literally was.
        secret: false,
    }
}

// Keeps a version snapshot AND the current entry: unlike restore_version (which
// overwrites), this adds a brand-new entry so the two can be compared side by side.
// The whole thing is one backend command, not a form the frontend prefills and the
// user then has to save by hand, so it reuses add_prompt's own grid-placement path
// and never fires an event add_prompt itself would not.
#[tauri::command]
async fn restore_version_as_new(app: AppHandle, state: State<'_, Db>, version_id: i64, suffix: String) -> Result<Option<Prompt>, String> {
    let created = {
        let mut store = lock(&state);
        let record = match db_conn(&app).and_then(|c| db_get_version(&c, version_id)) {
            Some(r) => r,
            None => return Ok(None),
        };
        let (pid, name, text, variants) = record;
        let Some(source) = store.prompts.iter().find(|p| p.id == pid).cloned() else {
            return Ok(None);
        };
        let prompt = build_restored_prompt(&source, &name, text, variants, &suffix);
        // Same placement as add_prompt: next free cell of the active view, which in
        // practice is the view the user was looking at when they opened this
        // prompt's history: landing the copy right where the original lives.
        let folder_cells = active_view_folder_cells(&store.settings);
        let view = store.settings.active_view_mut();
        if let Some(cell) = first_free_cell_excluding(view, &folder_cells) {
            let key = grid_key(view.cols, view.rows);
            view.layouts.entry(key).or_default().insert(prompt.id.clone(), cell);
        }
        view.members.push(prompt.id.clone());
        store.prompts.push(prompt.clone());
        if !save_prompts(&app, &store) {
            store.prompts.pop();
            store.settings.active_view_mut().members.retain(|m| *m != prompt.id);
            return Err("save-failed".into());
        }
        save_settings(&app, &store.settings);
        prompt
    };
    rebuild_tray(&app);
    Ok(Some(created))
}

// Star / unstar a prompt in the library.
#[tauri::command]
fn set_favorite(app: AppHandle, state: State<Db>, id: String, favorite: bool) -> Result<(), String> {
    let ok = {
        let mut store = lock(&state);
        match store.prompts.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                let previous = p.favorite;
                p.favorite = favorite;
                // A failed save must not leave the UI believing the star took:
                // roll the in-memory flag back so the next render shows the truth.
                if save_prompts(&app, &store) {
                    true
                } else {
                    if let Some(slot) = store.prompts.iter_mut().find(|x| x.id == id) {
                        slot.favorite = previous;
                    }
                    false
                }
            }
            None => true, // nothing to persist
        }
    };
    if !ok {
        return Err("save-failed".into());
    }
    rebuild_tray(&app); // The favourites section follows the stars
    Ok(())
}

// Async: keeps the prompt-table rewrite off the UI thread.
#[tauri::command]
// "Delete" moves a prompt into the trash table instead of removing it: media
// files and version history stay exactly as they were, untouched until a real
// purge (purge_prompt/empty_trash, or the retention sweep). Everything else here
// (views, floating, folders, hotkeys, tray) is unchanged from the old hard-delete:
// those are all app-level associations, not part of the prompt's own content, and
// a restored prompt gets a fresh placement rather than trying to replay them.
async fn delete_prompt(app: AppHandle, state: State<'_, Db>, id: String) -> Result<bool, String> {
    close_floating_window(&app, &id);
    let changed = {
        let mut store = lock(&state);
        let original_settings = store.settings.clone();
        let moved = match store.prompts.iter().position(|p| p.id == id) {
            Some(pos) => {
                let mut p = store.prompts.remove(pos);
                p.trashed_at = now_secs();
                store.trash.insert(0, p); // newest-deleted first
                Some(pos)
            }
            None => None,
        };
        for view in &mut store.settings.views {
            for layout in view.layouts.values_mut() {
                layout.remove(&id);
            }
            view.members.retain(|m| *m != id);
        }
        store.settings.floating.remove(&id);
        store.settings.float_scale.remove(&id);
        store.settings.video_prefs.remove(&id);
        store.normalize_folders(); // And out of whatever folder held it
        if let Some(pos) = moved {
            // prompts+trash now move in one transaction, so a failed write can no
            // longer strand the prompt in neither table. Report success only once it
            // actually committed - on failure, undo the in-memory move (the trash
            // entry AND the settings housekeeping above) instead of reporting success
            // for a move that never reached disk.
            if !save_prompts_and_trash(&app, &store) {
                if !store.trash.is_empty() {
                    let mut p = store.trash.remove(0);
                    p.trashed_at = 0;
                    let at = pos.min(store.prompts.len());
                    store.prompts.insert(at, p);
                }
                store.settings = original_settings;
                return Err("save-failed".into());
            }
            save_settings(&app, &store.settings);
        }
        moved.is_some()
    };
    if changed {
        rearm_hotkeys(&app); // A deleted prompt gives its accelerator back
        rebuild_tray(&app);
        sync_float_windows(&app); // A folder left with one member dissolves
        if let Some(conn) = db_conn(&app) {
            discard_editor_draft_if(&conn, |t| t == id);
        }
    }
    Ok(changed)
}

// The in-memory half of a batch delete: the prompts move to the trash (newest-deleted
// first, keeping the batch's own order) and every app-level association is let go:
// view membership and cells, pills, scale, video prefs, folders. Returns the untouched
// originals so the caller can undo the move if the write fails.
pub(crate) fn move_batch_to_trash(store: &mut Store, ids: &[String], now: u64) -> Vec<Prompt> {
    let idset: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let mut moved: Vec<Prompt> = Vec::new();
    store.prompts.retain(|p| {
        if idset.contains(p.id.as_str()) {
            moved.push(p.clone());
            false
        } else {
            true
        }
    });
    if moved.is_empty() {
        return moved;
    }
    // .cloned() (not .into_iter()) so `moved` still holds the untouched
    // (trashed_at: 0) originals if the save fails.
    for mut p in moved.iter().cloned().rev() {
        p.trashed_at = now;
        store.trash.insert(0, p);
    }
    for view in &mut store.settings.views {
        for layout in view.layouts.values_mut() {
            layout.retain(|k, _| !idset.contains(k.as_str()));
        }
        view.members.retain(|m| !idset.contains(m.as_str()));
    }
    for id in ids {
        store.settings.floating.remove(id);
        store.settings.float_scale.remove(id);
        store.settings.video_prefs.remove(id);
    }
    store.normalize_folders();
    moved
}

// Batch operations: apply one action to many prompts in a SINGLE store pass
// (no per-item save loop, so 100+ selected prompts never stall the UI). Returns the
// fresh state so the caller re-renders once. Actions: delete / favorite / color /
// addView / removeView / tag-add / tag-remove.
// One flag per independent batch action the library toolbar can trigger; splitting
// this into several commands would just move the argument count into call-site count.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn batch_prompts(
    app: AppHandle,
    state: State<'_, Db>,
    ids: Vec<String>,
    action: String,
    color: Option<String>,
    favorite: Option<bool>,
    view_id: Option<String>,
    from_view_id: Option<String>,
    tag: Option<String>,
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
            // Move to trash, same as the single-prompt delete_prompt command:
            // media files and version history stay untouched until a real purge.
            let original_settings = store.settings.clone();
            let moved = move_batch_to_trash(&mut store, &ids, now_secs());
            if !moved.is_empty() {
                // Same atomic prompts+trash write as delete_prompt/restore_prompt,
                // found while fixing those two: batch delete had the identical
                // stranded-prompt risk. On failure, undo the batch move instead of
                // reporting success for a save that never reached disk. The moved
                // prompts go back at the END of store.prompts rather than their exact
                // original scattered indices - this is a failure-recovery path only,
                // and the grid position the user actually sees lives in
                // view.layouts, not the prompts Vec's own order.
                if !save_prompts_and_trash(&app, &store) {
                    store.trash.drain(0..moved.len());
                    store.prompts.extend(moved);
                    store.settings = original_settings;
                    return Err("save-failed".into());
                }
                save_settings(&app, &store.settings);
                if let Some(conn) = db_conn(&app) {
                    discard_editor_draft_if(&conn, |t| idset.contains(t));
                }
            }
        }
        "favorite" => {
            let fav = favorite.unwrap_or(true);
            let previous: Vec<(String, bool)> = store
                .prompts
                .iter()
                .filter(|p| idset.contains(p.id.as_str()))
                .map(|p| (p.id.clone(), p.favorite))
                .collect();
            for p in store.prompts.iter_mut() {
                if idset.contains(p.id.as_str()) {
                    p.favorite = fav;
                }
            }
            // A failed save must roll every touched prompt back, same pattern as
            // the "delete" branch above: otherwise the UI shows a batch-favorite that
            // silently reverts for some or all of the selection on the next launch.
            if !save_prompts(&app, &store) {
                for (id, was) in previous {
                    if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
                        p.favorite = was;
                    }
                }
                return Err("save-failed".into());
            }
        }
        "color" => {
            let c = color.unwrap_or_default();
            let previous: Vec<(String, String)> = store
                .prompts
                .iter()
                .filter(|p| idset.contains(p.id.as_str()))
                .map(|p| (p.id.clone(), p.color.clone()))
                .collect();
            for p in store.prompts.iter_mut() {
                if idset.contains(p.id.as_str()) {
                    p.color.clone_from(&c);
                }
            }
            if !save_prompts(&app, &store) {
                for (id, was) in previous {
                    if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
                        p.color = was;
                    }
                }
                return Err("save-failed".into());
            }
        }
        "addView" | "removeView" => {
            let vid = view_id.unwrap_or_default();
            apply_view_batch_action(&mut store.settings, &action, &ids, &vid, "", |s| save_settings(&app, s))?;
        }
        // doGridMove used to call addView then removeView as two separate
        // IPC round-trips (two separate save_settings) - a crash between them left
        // the prompt placed in both views. Both layouts now change in the SAME
        // in-memory settings before the single save below, so a failure (or a
        // crash) can only ever be seen before or after the whole move, never
        // mid-way.
        "moveView" => {
            let to_id = view_id.unwrap_or_default();
            let from_id = from_view_id.unwrap_or_default();
            apply_view_batch_action(&mut store.settings, &action, &ids, &to_id, &from_id, |s| save_settings(&app, s))?;
        }
        // Adding a tag a prompt already carries, or removing one it never had, is
        // not an error -- both just leave that prompt untouched. Casing follows
        // whatever the library already uses for this tag (canonical_tags), same
        // rule the single-prompt editor applies.
        "tag-add" | "tag-remove" => {
            let name = valid_tag_name(&tag.unwrap_or_default())?;
            let known = canonical_tags(&store.prompts);
            let previous: Vec<(String, Vec<String>)> = store
                .prompts
                .iter()
                .filter(|p| idset.contains(p.id.as_str()))
                .map(|p| (p.id.clone(), p.tags.clone()))
                .collect();
            let result = if action == "tag-add" {
                add_tag_to_ids(&mut store.prompts, &idset, &name, &known)
            } else {
                remove_tag_from_ids(&mut store.prompts, &idset, &tag_key(&name));
                Ok(())
            };
            if let Err(e) = result {
                for (id, tags) in previous {
                    if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
                        p.tags = tags;
                    }
                }
                return Err(e);
            }
            if !save_prompts(&app, &store) {
                for (id, tags) in previous {
                    if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
                        p.tags = tags;
                    }
                }
                return Err("save-failed".into());
            }
        }
        _ => return Err("bad-action".into()),
    }
    // Return only the (small) settings: the caller mutates its local prompt list
    // in place, so we never re-serialise every prompt's base64 image over IPC.
    let settings = webview_settings(&store.settings);
    drop(store); // never re-arm shortcuts while the store mutex is held
    if action == "delete" {
        rearm_hotkeys(&app); // The deleted prompts give their accelerators back
        sync_float_windows(&app); // Folders left too small dissolve
    }
    // A floating button's own pill/folder window reads view membership too
    // (pill folders group by view) -- moving or copying a prompt between
    // views, including dragging a tile onto a view tab, must reach it the
    // same way any other settings change does.
    if matches!(action.as_str(), "addView" | "removeView" | "moveView") {
        notify_float_settings(&app);
    }
    rebuild_tray(&app); // One rebuild for the whole batch, not one per prompt
    Ok(settings)
}

// ---------- Trash ----------

// What one row of the trash dialog reads, and nothing more: the dialog draws no
// picture, so the stored images (hundreds of kilobytes each) never travel. `text`
// is a short preview; a confidential entry's preview stays empty, since its stored
// text is ciphertext.
#[derive(Serialize)]
pub(crate) struct TrashRow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) file_path: String,
    pub(crate) icon_path: String,
    pub(crate) copy_image: bool,
    pub(crate) has_image: bool,
    pub(crate) trashed_at: u64,
}

pub(crate) const TRASH_PREVIEW_CHARS: usize = 400;

pub(crate) fn trash_rows(trash: &[Prompt]) -> Vec<TrashRow> {
    trash
        .iter()
        .map(|p| TrashRow {
            id: p.id.clone(),
            name: p.name.clone(),
            text: if p.secret { String::new() } else { p.text.chars().take(TRASH_PREVIEW_CHARS).collect() },
            file_path: p.file_path.clone(),
            icon_path: p.icon_path.clone(),
            copy_image: p.copy_image,
            has_image: !p.image.is_empty(),
            trashed_at: p.trashed_at,
        })
        .collect()
}

#[tauri::command]
async fn list_trash(state: State<'_, Db>) -> Result<Vec<TrashRow>, String> {
    Ok(trash_rows(&lock(&state).trash))
}

// Moves a trashed prompt back onto the active view and persists it. On a failed
// write the store is put back exactly as it was: the rollback copy is the FULL
// settings (never the window copy, which lacks the receiver's password hash and
// TLS key), so a failed restore cannot strip them from the live store.
pub(crate) fn restore_trashed_core(store: &mut Store, id: &str, save: impl FnOnce(&Store) -> bool) -> Result<(), String> {
    let Some(pos) = store.trash.iter().position(|p| p.id == id) else {
        return Err("not-found".into());
    };
    let original_settings = store.settings.clone();
    let mut p = store.trash.remove(pos);
    let original_trashed_at = p.trashed_at;
    p.trashed_at = 0;
    let folder_cells = active_view_folder_cells(&store.settings);
    let view = store.settings.active_view_mut();
    if let Some(cell) = first_free_cell_excluding(view, &folder_cells) {
        let key = grid_key(view.cols, view.rows);
        view.layouts.entry(key).or_default().insert(p.id.clone(), cell);
    }
    view.members.push(p.id.clone());
    store.prompts.push(p);
    // Same atomic prompts+trash write as delete_prompt - on failure, undo the
    // in-memory move (including the layout cell claimed above) instead of
    // reporting success for a restore that never reached disk.
    if !save(store) {
        if let Some(mut p) = store.prompts.pop() {
            p.trashed_at = original_trashed_at;
            let at = pos.min(store.trash.len());
            store.trash.insert(at, p);
        }
        store.settings = original_settings;
        return Err("save-failed".into());
    }
    Ok(())
}

// Places the restored prompt in the ACTIVE view via the same first_free_cell scan
// add_prompt already uses for a brand-new prompt: if the view is full, the prompt
// still comes back (reachable through the library) just without a grid cell of its
// own: the same graceful fallback, never a hard failure or a silently dropped
// prompt. Every other app-level association (floating, folder membership, hotkey)
// was already let go at delete time and is not reconstructed.
#[tauri::command]
async fn restore_prompt(app: AppHandle, state: State<'_, Db>, id: String) -> Result<AppState, String> {
    let (app_state, restored) = {
        let mut store = lock(&state);
        restore_trashed_core(&mut store, &id, |st| save_prompts_and_trash(&app, st))?;
        save_settings(&app, &store.settings);
        let secret_key = secret_session_key(secret_timeout_min(&store.settings));
        (AppState { prompts: slim_prompts(&store.prompts, secret_key), settings: webview_settings(&store.settings) }, true)
    };
    if restored {
        rearm_hotkeys(&app); // The restored prompt's own hotkey, if any, is free again to re-claim
        rebuild_tray(&app);
    }
    Ok(app_state)
}

// Where a prompt sat when it was deleted, captured BEFORE the delete so an undo can put
// it back exactly: every view it belonged to with its position in the member list and
// its cell per grid size, its pill (position, scale), video prefs, and the grid and pill
// folder that held it. The trash row itself only keeps the prompt.
#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct ViewPlacement {
    pub(crate) view_id: String,
    pub(crate) index: usize,
    pub(crate) cells: HashMap<String, [u32; 2]>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub(crate) struct PromptPlacement {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) views: Vec<ViewPlacement>,
    #[serde(default)]
    pub(crate) floating: Option<Pos>,
    #[serde(default)]
    pub(crate) float_scale: Option<f64>,
    #[serde(default)]
    pub(crate) video_prefs: Option<VideoPrefs>,
    #[serde(default)]
    pub(crate) tile_folder: Option<TileFolder>,
    #[serde(default)]
    pub(crate) float_folder: Option<FloatFolder>,
}

pub(crate) fn capture_placements(settings: &Settings, ids: &[String]) -> Vec<PromptPlacement> {
    ids.iter()
        .map(|id| PromptPlacement {
            id: id.clone(),
            views: settings
                .views
                .iter()
                .filter_map(|v| {
                    let index = v.members.iter().position(|m| m == id)?;
                    let cells = v
                        .layouts
                        .iter()
                        .filter_map(|(k, layout)| layout.get(id).map(|c| (k.clone(), *c)))
                        .collect();
                    Some(ViewPlacement { view_id: v.id.clone(), index, cells })
                })
                .collect(),
            floating: settings.floating.get(id).cloned(),
            float_scale: settings.float_scale.get(id).copied(),
            video_prefs: settings.video_prefs.get(id).copied(),
            tile_folder: settings.tile_folders.iter().find(|f| f.members.contains(id)).cloned(),
            float_folder: settings.float_folders.iter().find(|f| f.members.contains(id)).cloned(),
        })
        .collect()
}

// Puts `id` on `cell` of the view's `key` grid; a cell that is taken (another tile, a
// grid folder) or lies outside the grid falls back to the first free cell of that grid.
pub(crate) fn place_on_cell_or_first_free(view: &mut View, key: &str, id: &str, cell: [u32; 2], folder_cells: &[[u32; 2]]) {
    let Some((cols, rows)) = key.split_once('x').and_then(|(c, r)| Some((c.parse::<u32>().ok()?, r.parse::<u32>().ok()?))) else {
        return;
    };
    let layout = view.layouts.entry(key.to_string()).or_default();
    let taken = |layout: &HashMap<String, [u32; 2]>, c: [u32; 2]| layout.values().any(|v| *v == c) || folder_cells.contains(&c);
    if cell[0] < cols && cell[1] < rows && !taken(layout, cell) {
        layout.insert(id.to_string(), cell);
        return;
    }
    for row in 0..rows {
        for col in 0..cols {
            if !taken(layout, [col, row]) {
                layout.insert(id.to_string(), [col, row]);
                return;
            }
        }
    }
}

// Undo for a delete: moves the trashed prompts back and re-applies what the snapshot
// holds. A failed write puts the store back exactly as it was (same full-settings
// rollback copy as restore_trashed_core). Ids that are no longer in the trash are
// skipped; none found at all is an error.
pub(crate) fn restore_placed_core(store: &mut Store, placements: &[PromptPlacement], save: impl FnOnce(&Store) -> bool) -> Result<usize, String> {
    let original_settings = store.settings.clone();
    let original_trash = store.trash.clone();
    let original_prompts = store.prompts.len();
    let mut restored: Vec<&PromptPlacement> = Vec::new();
    for pl in placements {
        if restored.iter().any(|r| r.id == pl.id) {
            continue;
        }
        let Some(pos) = store.trash.iter().position(|p| p.id == pl.id) else { continue };
        let mut p = store.trash.remove(pos);
        p.trashed_at = 0;
        store.prompts.push(p);
        restored.push(pl);
    }
    if restored.is_empty() {
        return Err("not-found".into());
    }
    let restored_ids: std::collections::HashSet<String> = restored.iter().map(|r| r.id.clone()).collect();
    // Folders first: a rebuilt grid folder takes its old cell back before any tile is
    // placed, so a tile never lands on the cell the folder is about to need.
    let live_ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    for pl in &restored {
        if let Some(snap) = &pl.tile_folder {
            restore_tile_folder(&mut store.settings, snap, &restored_ids, &live_ids);
        }
        if let Some(snap) = &pl.float_folder {
            restore_float_folder(&mut store.settings, snap, &restored_ids, &live_ids);
        }
    }
    for pl in &restored {
        for vp in &pl.views {
            let folder_cells: Vec<[u32; 2]> =
                store.settings.tile_folders.iter().filter(|f| f.view_id == vp.view_id).map(|f| f.cell).collect();
            let in_folder = store.settings.tile_folders.iter().any(|f| f.view_id == vp.view_id && f.members.contains(&pl.id));
            let Some(view) = store.settings.views.iter_mut().find(|v| v.id == vp.view_id) else { continue };
            if !view.members.contains(&pl.id) {
                let at = vp.index.min(view.members.len());
                view.members.insert(at, pl.id.clone());
            }
            if in_folder {
                continue; // a folder member owns no cell
            }
            for (key, cell) in &vp.cells {
                place_on_cell_or_first_free(view, key, &pl.id, *cell, &folder_cells);
            }
        }
        if let Some(pos) = &pl.floating {
            store.settings.floating.insert(pl.id.clone(), pos.clone());
        }
        if let Some(scale) = pl.float_scale {
            store.settings.float_scale.insert(pl.id.clone(), scale);
        }
        if let Some(prefs) = pl.video_prefs {
            store.settings.video_prefs.insert(pl.id.clone(), prefs);
        }
    }
    store.normalize_folders();
    if !save(store) {
        store.prompts.truncate(original_prompts);
        store.trash = original_trash;
        store.settings = original_settings;
        return Err("save-failed".into());
    }
    Ok(restored.len())
}

// A grid folder the delete dissolved (fewer than two members left) is rebuilt from its
// snapshot; one that survived just takes its members back at their old spots. A member
// inside a folder owns no cell, so its layout entries in that view are dropped again.
pub(crate) fn restore_tile_folder(settings: &mut Settings, snap: &TileFolder, restored: &std::collections::HashSet<String>, live_ids: &[String]) {
    if let Some(folder) = settings.tile_folders.iter_mut().find(|f| f.id == snap.id) {
        for (i, m) in snap.members.iter().enumerate() {
            if restored.contains(m) && !folder.members.contains(m) {
                let at = i.min(folder.members.len());
                folder.members.insert(at, m.clone());
            }
        }
    } else {
        let members: Vec<String> = snap
            .members
            .iter()
            .filter(|m| live_ids.contains(m) && !settings.tile_folders.iter().any(|f| f.members.contains(m)))
            .cloned()
            .collect();
        if members.len() < 2 {
            return;
        }
        let mut folder = snap.clone();
        folder.members = members;
        if let Some(view) = settings.views.iter().find(|v| v.id == folder.view_id) {
            let key = grid_key(view.cols, view.rows);
            let on_tile = view.layouts.get(&key).is_some_and(|l| l.iter().any(|(id, c)| *c == folder.cell && !folder.members.contains(id)));
            let on_folder = settings.tile_folders.iter().any(|f| f.view_id == folder.view_id && f.cell == folder.cell);
            if on_tile || on_folder {
                let others: Vec<[u32; 2]> = settings.tile_folders.iter().filter(|f| f.view_id == folder.view_id).map(|f| f.cell).collect();
                if let Some(c) = first_free_cell_excluding(view, &others) {
                    folder.cell = c;
                }
            }
        }
        settings.tile_folders.push(folder);
    }
    let Some(folder) = settings.tile_folders.iter().find(|f| f.id == snap.id) else { return };
    let (view_id, members) = (folder.view_id.clone(), folder.members.clone());
    if let Some(view) = settings.views.iter_mut().find(|v| v.id == view_id) {
        for layout in view.layouts.values_mut() {
            layout.retain(|id, _| !members.contains(id));
        }
    }
}

pub(crate) fn restore_float_folder(settings: &mut Settings, snap: &FloatFolder, restored: &std::collections::HashSet<String>, live_ids: &[String]) {
    if let Some(folder) = settings.float_folders.iter_mut().find(|f| f.id == snap.id) {
        for (i, m) in snap.members.iter().enumerate() {
            if restored.contains(m) && !folder.members.contains(m) {
                let at = i.min(folder.members.len());
                folder.members.insert(at, m.clone());
            }
        }
        return;
    }
    let members: Vec<String> = snap
        .members
        .iter()
        .filter(|m| live_ids.contains(m) && !settings.float_folders.iter().any(|f| f.members.contains(m)))
        .cloned()
        .collect();
    if members.len() >= 2 {
        let mut folder = snap.clone();
        folder.members = members;
        settings.float_folders.push(folder);
    }
}

// Read-only: the placement snapshot an undo needs, taken just before a batch delete.
#[tauri::command]
async fn placement_snapshot(state: State<'_, Db>, ids: Vec<String>) -> Result<Vec<PromptPlacement>, String> {
    Ok(capture_placements(&lock(&state).settings, &ids))
}

// Two writes that must both land: the first, then the second. When the first fails
// nothing is written; when the second fails the first is taken back, so the disk
// never holds placements for prompts that are still in the trash while the call
// reports an error.
pub(crate) fn save_pair_or_undo_first(first: impl FnOnce() -> bool, second: impl FnOnce() -> bool, undo_first: impl FnOnce()) -> bool {
    if !first() {
        return false;
    }
    if second() {
        return true;
    }
    undo_first();
    false
}

// Undo of a batch delete with the snapshot taken before it: the placements and
// the prompts are written together or not at all.
#[tauri::command]
async fn restore_prompts_batch(app: AppHandle, state: State<'_, Db>, placements: Vec<PromptPlacement>) -> Result<AppState, String> {
    let app_state = {
        let mut store = lock(&state);
        let original_settings = store.settings.clone();
        restore_placed_core(&mut store, &placements, |st| {
            save_pair_or_undo_first(
                || save_settings(&app, &st.settings),
                || save_prompts_and_trash(&app, st),
                || {
                    save_settings(&app, &original_settings);
                },
            )
        })?;
        let secret_key = secret_session_key(secret_timeout_min(&store.settings));
        AppState { prompts: slim_prompts(&store.prompts, secret_key), settings: webview_settings(&store.settings) }
    };
    rearm_hotkeys(&app);
    rebuild_tray(&app);
    sync_float_windows(&app); // pills and folders that came back
    Ok(app_state)
}

// Permanent delete: the trash entry, its media files and its whole version history
// are gone for good. This is the old hard-delete's own cleanup logic, just moved
// here instead of running at every "delete" click.
//
// Returns the orphan-file set to sweep instead of sweeping itself, so the
// caller can drop the store lock first: a big attachments folder used to stay
// locked out from every other window for the whole fs scan.
pub(crate) fn purge_ids(app: &AppHandle, store: &mut Store, ids: &[String]) -> Result<Option<std::collections::HashSet<String>>, String> {
    let idset: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let before = store.trash.clone();
    store.trash.retain(|p| !idset.contains(p.id.as_str()));

    // A failed trash save means the purged prompt is still coming back on
    // the next restart (trash has no JSON fallback the way save_prompts does): its
    // rotation position and version history must come back with it, not vanish
    // permanently for a purge that never actually persisted. Same guard shape as
    // the 102 fix on the orphan sweep just below.
    if save_trash(app, store) {
        let orphan_scan = orphan_cleanup_enabled(store).then(|| orphan_referenced_set(store));
        rotation_forget(&mut store.settings, ids); // no orphaned pointer/bag left behind
        save_settings(app, &store.settings);
        // save_trash above only returns true through a working connection, so
        // losing it between the two calls is a narrow window: but a purge that
        // leaves the prompt's text sitting in prompt_versions must never pass
        // unrecorded.
        match db_conn(app) {
            Some(c) => {
                for id in ids {
                    db_delete_versions(&c, id);
                }
            }
            None => append_crash_log(
                "purge: no database connection, version history of the purged prompts was left in place",
            ),
        }
        Ok(orphan_scan)
    } else {
        // A failed save already means the disk-side trash still has these rows and
        // they will reappear on restart: restore the in-memory copy to match now,
        // so this session's trash view (and any callers returning store.trash) does
        // not show them as gone in the meantime.
        store.trash = before;
        Err("save-failed".to_string())
    }
}

#[tauri::command]
async fn purge_prompt(app: AppHandle, state: State<'_, Db>, id: String) -> Result<Vec<TrashRow>, String> {
    let (trash, orphan_scan) = {
        let mut store = lock(&state);
        let orphan_scan = purge_ids(&app, &mut store, std::slice::from_ref(&id))?;
        (trash_rows(&store.trash), orphan_scan)
    };
    if let Some(referenced) = orphan_scan {
        sweep_orphan_files(&app, &referenced);
    }
    Ok(trash)
}

#[tauri::command]
async fn empty_trash(app: AppHandle, state: State<'_, Db>) -> Result<Vec<TrashRow>, String> {
    let (trash, orphan_scan) = {
        let mut store = lock(&state);
        let ids: Vec<String> = store.trash.iter().map(|p| p.id.clone()).collect();
        let orphan_scan = purge_ids(&app, &mut store, &ids)?;
        (trash_rows(&store.trash), orphan_scan)
    };
    if let Some(referenced) = orphan_scan {
        sweep_orphan_files(&app, &referenced);
    }
    Ok(trash)
}

// Drop anything past the retention window (0/off by settings default, same
// "0 = forever" convention as historyDays). Runs once at startup, mirroring
// prune_history's own startup sweep: never on a timer, so a long-idle app does
// not surprise-purge something the user only just noticed in the trash.
pub(crate) fn trash_retention_secs(settings: &Settings) -> Option<u64> {
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
pub(crate) fn stale_trash_ids(trash: &[Prompt], max_age: u64, now: u64) -> Vec<String> {
    trash
        .iter()
        .filter(|p| p.trashed_at > 0 && now.saturating_sub(p.trashed_at) > max_age)
        .map(|p| p.id.clone())
        .collect()
}

pub(crate) fn sweep_trash(app: &AppHandle, store: &mut Store) {
    let Some(max_age) = trash_retention_secs(&store.settings) else {
        return;
    };
    let stale = stale_trash_ids(&store.trash, max_age, now_secs());
    if !stale.is_empty() {
        // Startup-only, before the store is behind its mutex: nothing else can
        // be waiting on the lock yet, so sweeping inline here is fine.
        if let Ok(Some(referenced)) = purge_ids(app, store, &stale) {
            sweep_orphan_files(app, &referenced);
        }
    }
}

// The trash row's on-demand trigger -- the same rule sweep_trash
// applies at startup, but explicit and always answered (even "nothing to
// clean up"), same reasoning as sweep_recordings_now's own manual button.
// A live click carries none of the "long-idle app surprise-purges the trash"
// risk the startup-only comment above warns about, since the user asked for
// this exact sweep right now.
#[tauri::command]
fn sweep_trash_now(app: AppHandle, state: State<'_, Db>) -> (u32, u64) {
    // The orphan sweep locks the store itself, so it runs only after this guard is
    // gone (same shape as empty_trash): holding both froze the whole app.
    let (count, bytes, orphan_scan) = {
        let mut store = lock(&state);
        let Some(max_age) = trash_retention_secs(&store.settings) else {
            return (0, 0);
        };
        let stale = stale_trash_ids(&store.trash, max_age, now_secs());
        if stale.is_empty() {
            return (0, 0);
        }
        let stale_paths: Vec<String> = store
            .trash
            .iter()
            .filter(|p| stale.contains(&p.id))
            .map(|p| p.file_path.clone())
            .collect();
        let (_, bytes) = attachments_bytes(&stale_paths);
        let count = stale.len() as u32;
        let Ok(orphan_scan) = purge_ids(&app, &mut store, &stale) else {
            return (0, 0);
        };
        (count, bytes, orphan_scan)
    };
    if let Some(referenced) = orphan_scan {
        sweep_orphan_files(&app, &referenced);
    }
    (count, bytes)
}

// Factory reset: wipe all prompts AND all settings (views, theme, window,
// behaviour, fonts) and remove the autostart registry entry.
// Async: window closes + a full prompts/settings rewrite stay off the UI thread.
#[tauri::command]
async fn delete_all_data(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    let labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|l| l.starts_with("float-") || l.starts_with("folder-"))
        .cloned()
        .collect();
    for label in labels {
        if let Some(win) = app.get_webview_window(&label) {
            let _ = win.close();
        }
    }
    let mut incomplete = false;
    {
        let mut store = lock(&state);
        store.prompts.clear();
        store.trash.clear(); // a data wipe must not leave trashed prompts behind
        store.prompts_locked = false; // explicit wipe overrides the decrypt-failure lock
        store.normalize_folders(); // No prompts left, so no folder survives
        if !save_prompts(&app, &store) {
            incomplete = true;
        }
        if !save_trash(&app, &store) {
            incomplete = true;
        }
        save_settings(&app, &store.settings);
        // Data-only wipe: keep every setting, drop the derived data tables. Each
        // failure is logged and reported: the command used to always answer
        // Ok(()) here, so a locked file or a mid-wipe I/O error could leave
        // private rows on disk while the user was told everything was gone.
        match db_conn(&app) {
            Some(c) => {
                for (sql, table) in [
                    ("DELETE FROM prompt_versions", "prompt_versions"), // version history
                    ("DELETE FROM copy_log", "copy_log"),               // copy history
                    ("DELETE FROM clip_inbox", "clip_inbox"),           // clipboard inbox
                    ("DELETE FROM trash", "trash"),                     // trashed prompts
                    ("DELETE FROM edit_state", "edit_state"),           // unfinished editor work
                ] {
                    if let Err(e) = c.execute(sql, []) {
                        incomplete = true;
                        append_crash_log(&format!("delete_all_data: could not clear {table}: {e}"));
                    }
                }
            }
            None => {
                incomplete = true;
                append_crash_log("delete_all_data: no DB connection, auxiliary tables not cleared");
            }
        }
        // The trash DELETE above went straight to the table, and both saves emptied
        // the tables anyway: nothing is on disk for the next write to skip.
        store.forget_written_rows();
    }
    rearm_hotkeys(&app); // No prompts left to keep accelerators bound
    rebuild_tray(&app); // Nothing left to offer in the tray either
    if incomplete {
        return Err("delete-incomplete".into());
    }
    Ok(())
}

// The settings a reset leaves behind: every default, plus the values that are
// keys rather than preferences. The backup password would make every protected
// backup unreadable if dropped, the receiver's certificate belongs to this
// machine, and the vault's salt and verifier are what the confidential prompts
// the reset keeps are sealed with.
pub(crate) fn settings_after_reset(old: &Settings) -> Settings {
    let mut fresh = Settings::default();
    fresh.migrate();
    // A reset must never start deleting what the old settings kept: each retention
    // window stays at whichever of the old value and the default keeps longer.
    for (k, def) in [("historyDays", HISTORY_DAYS_DEFAULT), ("recordingRetentionDays", RECORDING_RETENTION_DAYS_DEFAULT)] {
        let was = old.ui_values.get(k).copied().unwrap_or(def);
        if was <= 0.0 || was > def {
            fresh.ui_values.insert(k.to_string(), was.max(0.0));
        }
    }
    if let Some(pw) = old.ui_texts.get(BACKUP_PW_KEY) {
        fresh.ui_texts.insert(BACKUP_PW_KEY.to_string(), pw.clone());
    }
    for (k, v) in remote_tls::tls_values(old) {
        fresh.ui_texts.insert(k, v);
    }
    carry_vault_keys(old, &mut fresh);
    fresh
}

// Reset every SETTING to its default (theme, views/layouts, toggles, expert values)
// while keeping the stored prompts intact. The front-end reloads so each default
// re-applies cleanly; cleared views make normalizeLayout re-place the kept prompts.
#[tauri::command]
async fn reset_settings(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    let labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|l| l.starts_with("float-") || l.starts_with("folder-"))
        .cloned()
        .collect();
    for label in labels {
        if let Some(win) = app.get_webview_window(&label) {
            let _ = win.close();
        }
    }
    let _ = apply_autostart(false, false);
    let effects = {
        let mut store = lock(&state);
        let old = store.settings.clone();
        let ocr_was_on = ocr_index_flag_on(&store);
        let prompts = std::mem::take(&mut store.prompts); // keep the prompts
        // The backup password is a key, not a preference: dropping it would make
        // every password-protected backup permanently unreadable.
        store.settings = settings_after_reset(&store.settings);
        SETTINGS_LOCKED.store(false, Ordering::Relaxed); // explicit reset overrides the corrupt-row lock
        load_pick_history(&store.settings); // reset clears the mirror too
        store.prompts = prompts;
        // The prompt rows themselves are untouched here, but the store around them
        // is rebuilt from defaults: start the row bookkeeping from scratch rather
        // than reasoning about what survived a factory reset.
        store.forget_written_rows();
        save_settings(&app, &store.settings);
        // Defaults switch the picture-text index off: erase its text as well.
        if ocr_was_on && clear_ocr_index_text(&mut store) {
            save_prompts_and_trash(&app, &store);
        }
        remote::reset_side_effects(&old, &store.settings)
    };
    remote::run_reset_side_effects(&app, &effects);
    // Defaults mean the screenshot switch is off: open windows go back to excluded.
    apply_capture_exclusion_all(&app, true);
    Ok(())
}

// The answer is kept per SOURCE FOLDER (never a single global flag), so a
// genuinely different old location is offered again even after an earlier one
// was settled. Stored in the same free-form map the expert menu's own text
// settings use, under a key namespace of its own.
pub(crate) const TAKEOVER_DECISION_PREFIX: &str = "takeoverDecision:";

pub(crate) fn takeover_decision_key(src: &std::path::Path) -> String {
    format!("{TAKEOVER_DECISION_PREFIX}{}", src.display())
}

// Whether this exact old location was already settled, whichever way. Pulled
// out of takeover_offer() (which needs an AppHandle) so the three answer paths
// (adopt, keep separate, never answered) are testable without one.
pub(crate) fn takeover_already_decided(ui_texts: &HashMap<String, String>, src: &std::path::Path) -> bool {
    ui_texts.contains_key(&takeover_decision_key(src))
}

// Records the answer only if asked to remember it. Skipping the record on
// `remember == false` is deliberate: closing the dialog without ticking that
// box must leave nothing behind, so the same folder is offered again next
// launch rather than being silently treated as settled.
pub(crate) fn takeover_record_decision(ui_texts: &mut HashMap<String, String>, src: &std::path::Path, adopt: bool, remember: bool) {
    if remember {
        ui_texts.insert(takeover_decision_key(src), if adopt { "adopt" } else { "skip" }.to_string());
    }
}

// The three things the offer dialog needs to say before anything is copied:
// where the old data is, how big it is, and roughly how much is in it.
#[derive(Serialize)]
pub(crate) struct TakeoverOffer {
    pub(crate) path: String,
    pub(crate) size: u64,
    // None when the database could not be opened (e.g. the other copy of the
    // app still has it open) -- the dialog falls back to naming it unknown
    // rather than blocking the offer on a count that is not essential to it.
    pub(crate) prompts: Option<usize>,
}

// Size and prompt count of a store folder, read without staging or copying it
// -- unlike a backup, a live store is a plain uncompressed data.db, so a
// direct read-only COUNT(*) is enough and never touches restore's read rail.
pub(crate) fn takeover_offer_info(src: &std::path::Path) -> TakeoverOffer {
    let size = STORE_FILES.iter().filter_map(|f| fs::metadata(src.join(f)).ok()).map(|m| m.len()).sum();
    let prompts = Connection::open(src.join("data.db"))
        .ok()
        .and_then(|c| c.query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get::<_, i64>(0)).ok())
        .map(|n| n as usize);
    TakeoverOffer { path: src.display().to_string(), size, prompts }
}

// Whether the store has nothing an adoption offer would overwrite: no prompts at
// all, or only the ones seed_example_prompts put there itself. Kept separate from
// takeover_offer() (which needs an AppHandle) so the condition itself is testable.
pub(crate) fn store_has_no_real_prompts(prompts: &[Prompt], example_prompt_ids: &[String]) -> bool {
    prompts.iter().all(|p| example_prompt_ids.contains(&p.id))
}

// Portable run, own store still empty (or seeded-examples-only), an installed store
// present elsewhere → the UI offers to adopt it, naming its size and prompt count
// so the question is answerable without opening the app twice. Nothing here
// copies or moves anything -- see takeover_apply for that, after an answer.
#[tauri::command]
fn takeover_offer(app: AppHandle, state: State<Db>) -> Option<TakeoverOffer> {
    if !is_portable() {
        return None;
    }
    {
        let store = lock(&state);
        if !store_has_no_real_prompts(&store.prompts, &store.settings.example_prompt_ids) {
            return None;
        }
    }
    let dir = data_dir(&app);
    let src = appdata_store_dir(&app, &dir)?;
    if takeover_already_decided(&lock(&state).settings.ui_texts, &src) {
        return None;
    }
    Some(takeover_offer_info(&src))
}

// Answer to that question. `adopt` copies the installed store beside the exe (the
// original stays untouched) and reloads it; either way, `remember` decides whether
// this exact old location is settled for future launches or asked about again --
// closing the dialog without ticking "do not ask again" must leave nothing decided.
#[tauri::command]
fn takeover_apply(app: AppHandle, state: State<'_, Db>, adopt: bool, remember: bool) -> Result<(), String> {
    let dir = data_dir(&app);
    let src = appdata_store_dir(&app, &dir);
    if adopt {
        let src = src.clone().ok_or("no-source")?;
        let mut landed = false;
        if let Err(e) = copy_store_files(&src, &dir, &mut landed) {
            // Whatever did arrive IS the store on disk now, while the one in memory
            // (and the row bookkeeping behind it) still describes the empty one it
            // replaced: so the next save would delete every adopted row as "gone".
            // The source in %APPDATA% is untouched either way; reload from what
            // actually landed before reporting the failure.
            if landed {
                let _ = fs::remove_file(dir.join("data.db-wal"));
                let _ = fs::remove_file(dir.join("data.db-shm"));
                let fresh = load_store(&app);
                *lock(&state) = fresh;
            }
            return Err(e);
        }
        // Same gap as set_data_dir -- an adopted store must bring its backup
        // history along too, or the Backup panel shows 0 snapshots right after.
        let src_backups = src.join("backups");
        if src_backups.is_dir() {
            if let Err(e) = copy_dir_all(&src_backups, &dir.join("backups")) {
                append_crash_log(&format!("takeover: could not copy backups/ ({e})"));
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
    if remember {
        if let Some(src) = src {
            let mut store = lock(&state);
            takeover_record_decision(&mut store.settings.ui_texts, &src, adopt, remember);
            save_settings(&app, &store.settings);
        }
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
// Async: fires on every drag/hide; keep the settings write off the UI thread.
#[tauri::command]
async fn set_layout(app: AppHandle, state: State<'_, Db>, layout: HashMap<String, [u32; 2]>) -> Result<(), String> {
    let mut store = lock(&state);
    let view = store.settings.active_view_mut();
    let key = grid_key(view.cols, view.rows);
    view.layouts.insert(key, layout);
    save_settings(&app, &store.settings);
    Ok(())
}

// Rebuilds one view's layout for its CURRENT grid size from `members`, the
// view's real membership list. Never removes a member: an id
// that finds no free cell simply gets no layout entry and shows up as
// overflow in the UI instead of silently disappearing. Replaces the old
// grow/shrink branching (which decided membership from the layouts
// themselves and, along three separate paths, could drop or resurrect an
// entry) with one unconditional recompute that cannot take a wrong branch
// because it no longer has one.
pub(crate) fn rebuild_view_layout(
    view: &mut View,
    folder_cells: &std::collections::HashSet<(u32, u32)>,
    folder_members: &std::collections::HashSet<String>,
) {
    let (cols, rows) = (view.cols, view.rows);
    let key = grid_key(cols, rows);
    // Keep whatever the current size already has on record, dropping only
    // entries for ids that are no longer members, now live inside a grid
    // folder instead of on their own, or no longer fit (the grid shrank
    // under them).
    let mut layout: HashMap<String, [u32; 2]> = view.layouts.get(&key).cloned().unwrap_or_default();
    layout.retain(|pid, cell| {
        view.members.iter().any(|m| m == pid)
            && !folder_members.contains(pid)
            && cell[0] < cols
            && cell[1] < rows
    });
    let mut occupied: std::collections::HashSet<(u32, u32)> = folder_cells.clone();
    occupied.extend(layout.values().map(|c| (c[0], c[1])));
    // A member without a cell yet prefers the spot it is remembered at from
    // ANY other saved size (so shrinking and growing back restores exactly
    // the old arrangement, and a member new to this size still lands close
    // to where the user last saw it) before falling back to the first free
    // cell. Growing always finds room for everyone; shrinking may not, and
    // the ones left over stay members without a cell (overflow).
    let mut remembered: HashMap<&str, [u32; 2]> = HashMap::new();
    for (k, other) in &view.layouts {
        if *k == key {
            continue;
        }
        for (pid, cell) in other {
            remembered.entry(pid.as_str()).or_insert(*cell);
        }
    }
    for pid in view.members.clone() {
        // Lives inside a grid folder's own cell, not as an individual tile.
        if folder_members.contains(&pid) {
            continue;
        }
        if layout.contains_key(&pid) {
            continue;
        }
        if let Some(cell) = remembered.get(pid.as_str()) {
            if cell[0] < cols && cell[1] < rows && !occupied.contains(&(cell[0], cell[1])) {
                layout.insert(pid.clone(), *cell);
                occupied.insert((cell[0], cell[1]));
                continue;
            }
        }
        'find: for row in 0..rows {
            for col in 0..cols {
                if !occupied.contains(&(col, row)) {
                    layout.insert(pid.clone(), [col, row]);
                    occupied.insert((col, row));
                    break 'find;
                }
            }
        }
    }
    view.layouts.insert(key, layout);
}

// Change the active view's grid dimensions. The arrangement saved for the new
// size (if any) is restored automatically because layouts are keyed per size;
// membership itself (`members`) never changes here, so no entry can appear or
// disappear from the view just because its grid did.
#[tauri::command]
fn set_view_grid(app: AppHandle, state: State<Db>, id: String, cols: u32, rows: u32) -> Settings {
    let mut store = lock(&state);
    let cols = cols.clamp(GRID_MIN, GRID_MAX);
    let rows = rows.clamp(GRID_MIN, GRID_MAX);
    // A folder whose cell no longer fits the shrunk grid moves to the first free
    // cell in this view instead of landing off-grid and becoming unreachable -
    // it loses at most its own cell, same as a single entry would.
    // normalize_tile_folders below resolves any collision the move creates.
    let mut taken_cells: Vec<[u32; 2]> = Vec::new();
    for folder in store
        .settings
        .tile_folders
        .iter_mut()
        .filter(|f| f.view_id == id)
    {
        if folder.cell[0] >= cols || folder.cell[1] >= rows {
            'find: for row in 0..rows {
                for col in 0..cols {
                    if !taken_cells.contains(&[col, row]) {
                        folder.cell = [col, row];
                        break 'find;
                    }
                }
            }
        }
        taken_cells.push(folder.cell);
    }
    let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    let view_ids: Vec<String> = store.settings.views.iter().map(|v| v.id.clone()).collect();
    normalize_tile_folders(&mut store.settings.tile_folders, &ids, &view_ids);
    let folder_cells: std::collections::HashSet<(u32, u32)> = store
        .settings
        .tile_folders
        .iter()
        .filter(|f| f.view_id == id)
        .map(|f| (f.cell[0], f.cell[1]))
        .collect();
    let folder_members: std::collections::HashSet<String> = store
        .settings
        .tile_folders
        .iter()
        .filter(|f| f.view_id == id)
        .flat_map(|f| f.members.iter().cloned())
        .collect();
    let Some(view) = store.settings.views.iter_mut().find(|v| v.id == id) else {
        return webview_settings(&store.settings);
    };
    view.cols = cols;
    view.rows = rows;
    rebuild_view_layout(view, &folder_cells, &folder_members);
    save_settings(&app, &store.settings);
    webview_settings(&store.settings)
}

// ---------- Tag commands ----------

pub(crate) const MAX_TAG_LEN: usize = 60;
pub(crate) const MAX_TAGS_PER_PROMPT: usize = 50;

pub(crate) fn tag_key(name: &str) -> String {
    name.trim().to_lowercase()
}

// Whether a prompt carries the reserved archive tag -- a plain tag match,
// never a separate state field, so every other tag-aware code path (search,
// export, backup, sync) already treats an archived prompt like any other one
// without having to know this concept exists at all.
pub(crate) fn is_archived(prompt: &Prompt, archive_tag_key: Option<&str>) -> bool {
    match archive_tag_key {
        Some(key) => prompt.tags.iter().any(|t| tag_key(t) == key),
        None => false,
    }
}

// Commas separate tags in the TXT/CSV exports and control characters would break the
// line-based TXT format, so neither may appear inside a name.
pub(crate) fn tag_chars_ok(name: &str) -> bool {
    !name.contains(',') && !name.chars().any(|c| c.is_control())
}

// The casing each known tag is already stored with, so a tag typed as "work" joins
// an existing "Work" instead of splitting into a second chip.
pub(crate) fn canonical_tags(prompts: &[Prompt]) -> HashMap<String, String> {
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
pub(crate) fn sanitize_tags(input: Vec<String>, known: &HashMap<String, String>) -> Result<Vec<String>, String> {
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

// ---------- Private notes ----------

// Generous ceiling: notes are a scratchpad, not a second prompt body. Rejecting
// instead of truncating keeps the user's text from being silently cut in half.
pub(crate) const MAX_NOTES_LEN: usize = 10_000;

// Normalize line endings and enforce the ceiling. Notes may contain newlines:
// the TXT exporter escapes them (see notes_escape).
pub(crate) fn sanitize_notes(raw: &str) -> Result<String, String> {
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
pub(crate) fn notes_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

pub(crate) fn notes_unescape(s: &str) -> String {
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
pub(crate) fn valid_tag_name(name: &str) -> Result<String, String> {
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
pub(crate) fn valid_tag_color(color: &str) -> Result<String, String> {
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
pub(crate) fn rename_tag_in(prompts: &mut [Prompt], old_key: &str, new: &str) -> bool {
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

// Move `old_key`'s color onto `new_key`, but only if `new_key` doesn't already
// have one -- the surviving tag's own color always wins. This is what makes
// renaming onto an existing tag a real merge rather than a plain rename: the
// same rename_tag command handles both, this rule is the only thing distinguishing
// "merge" from "rename" at the color layer.
pub(crate) fn transfer_tag_color(colors: &mut HashMap<String, String>, old_key: &str, new_key: &str) {
    if let Some(color) = colors.remove(old_key) {
        colors.entry(new_key.to_string()).or_insert(color);
    }
}

// Drop one tag key from every prompt. Returns whether anything changed.
pub(crate) fn remove_tag_from(prompts: &mut [Prompt], key: &str) -> bool {
    let mut changed = false;
    for p in prompts {
        let before = p.tags.len();
        p.tags.retain(|t| tag_key(t) != key);
        changed |= p.tags.len() != before;
    }
    changed
}

// Batch "add tag": only touches the listed ids, and only those that don't already
// carry it (case-insensitive) -- already having it is not an error, just a no-op
// for that one prompt. `known` supplies the casing an existing tag is stored with,
// same as the single-prompt editor's own sanitize_tags call.
pub(crate) fn add_tag_to_ids(
    prompts: &mut [Prompt],
    ids: &std::collections::HashSet<&str>,
    name: &str,
    known: &HashMap<String, String>,
) -> Result<(), String> {
    let key = tag_key(name);
    for p in prompts.iter_mut() {
        if !ids.contains(p.id.as_str()) || p.tags.iter().any(|t| tag_key(t) == key) {
            continue;
        }
        let mut next = p.tags.clone();
        next.push(name.to_string());
        p.tags = sanitize_tags(next, known)?;
    }
    Ok(())
}

// Batch "remove tag": only touches the listed ids. Not carrying the tag is not an
// error, just a no-op for that one prompt.
pub(crate) fn remove_tag_from_ids(prompts: &mut [Prompt], ids: &std::collections::HashSet<&str>, key: &str) {
    for p in prompts.iter_mut() {
        if ids.contains(p.id.as_str()) {
            p.tags.retain(|t| tag_key(t) != key);
        }
    }
}

// Replace `old` with `new` in every prompt and move the color entry, in ONE store
// pass: a tag on hundreds of prompts must not turn into hundreds of round trips.
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
    rename_tag_in_store(
        &mut store,
        &old_key,
        &new,
        &new_key,
        &mut |st| save_prompts(&app, st),
        &mut |settings| save_settings(&app, settings),
    )?;
    Ok(webview_settings(&store.settings))
}

// The two-step write behind rename_tag with the saves injected, so the rollback is
// testable. The prompts are saved first; when the settings save then fails, the
// prompts are put back AND written again, because the rename is already on disk
// and the caller is told the whole call failed.
pub(crate) fn rename_tag_in_store(
    store: &mut Store,
    old_key: &str,
    new: &str,
    new_key: &str,
    save_prompts_fn: &mut dyn FnMut(&Store) -> bool,
    save_settings_fn: &mut dyn FnMut(&Settings) -> bool,
) -> Result<(), String> {
    // Snapshot everything this command can touch before mutating, so a failed
    // save can put it back exactly instead of reporting success for a rename that
    // never reached disk.
    let original_tags: Vec<(String, Vec<String>)> = store
        .prompts
        .iter()
        .filter(|p| p.tags.iter().any(|t| tag_key(t) == old_key))
        .map(|p| (p.id.clone(), p.tags.clone()))
        .collect();
    let prompts_changed = rename_tag_in(&mut store.prompts, old_key, new);
    let restore_prompts = |store: &mut Store, original_tags: Vec<(String, Vec<String>)>| {
        for (id, tags) in original_tags {
            if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
                p.tags = tags;
            }
        }
    };
    if prompts_changed && !save_prompts_fn(store) {
        restore_prompts(store, original_tags);
        return Err("save-failed".into());
    }
    let original_old_color = store.settings.tag_colors.get(old_key).cloned();
    let original_new_color = store.settings.tag_colors.get(new_key).cloned();
    transfer_tag_color(&mut store.settings.tag_colors, old_key, new_key);
    if !save_settings_fn(&store.settings) {
        match original_old_color {
            Some(c) => { store.settings.tag_colors.insert(old_key.to_string(), c); }
            None => { store.settings.tag_colors.remove(old_key); }
        }
        match original_new_color {
            Some(c) => { store.settings.tag_colors.insert(new_key.to_string(), c); }
            None => { store.settings.tag_colors.remove(new_key); }
        }
        if prompts_changed {
            restore_prompts(store, original_tags);
            let _ = save_prompts_fn(store);
        }
        return Err("save-failed".into());
    }
    Ok(())
}

// Remove a tag from every prompt plus its color entry. Idempotent.
#[tauri::command]
async fn delete_tag(app: AppHandle, state: State<'_, Db>, name: String) -> Result<Settings, String> {
    let key = tag_key(&valid_tag_name(&name)?);
    let mut store = lock(&state);
    let original_tags: Vec<(String, Vec<String>)> = store
        .prompts
        .iter()
        .filter(|p| p.tags.iter().any(|t| tag_key(t) == key))
        .map(|p| (p.id.clone(), p.tags.clone()))
        .collect();
    let prompts_changed = remove_tag_from(&mut store.prompts, &key);
    if prompts_changed && !save_prompts(&app, &store) {
        for (id, tags) in original_tags {
            if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
                p.tags = tags;
            }
        }
        return Err("save-failed".into());
    }
    let original_color = store.settings.tag_colors.remove(&key);
    if !save_settings(&app, &store.settings) {
        if let Some(c) = original_color {
            store.settings.tag_colors.insert(key, c);
        }
        // The prompts without the tag are already on disk: put them back there too.
        if prompts_changed {
            for (id, tags) in original_tags {
                if let Some(p) = store.prompts.iter_mut().find(|p| p.id == id) {
                    p.tags = tags;
                }
            }
            let _ = save_prompts(&app, &store);
        }
        return Err("save-failed".into());
    }
    Ok(webview_settings(&store.settings))
}

#[derive(Deserialize)]
pub(crate) struct TagColor {
    pub(crate) name: String,
    pub(crate) color: String,
}

// Every name and colour is validated before anything is stored, so one bad entry
// rejects the whole call instead of leaving half the tags coloured.
pub(crate) fn apply_tag_colors(settings: &mut Settings, entries: Vec<TagColor>) -> Result<(), String> {
    let checked: Vec<(String, String)> = entries
        .into_iter()
        .map(|e| Ok((tag_key(&valid_tag_name(&e.name)?), valid_tag_color(&e.color)?)))
        .collect::<Result<_, String>>()?;
    for (key, color) in checked {
        if color.is_empty() {
            settings.tag_colors.remove(&key);
        } else {
            settings.tag_colors.insert(key, color);
        }
    }
    Ok(())
}

// "Colour every tag at once": one lock and one settings write for the whole set.
// Doing it tag by tag through set_tag_color meant a full immediate write per tag,
// which on a large library is that many disk writes back to back.
#[tauri::command]
fn set_tag_colors(app: AppHandle, state: State<Db>, entries: Vec<TagColor>) -> Result<Settings, String> {
    let mut store = lock(&state);
    apply_tag_colors(&mut store.settings, entries)?;
    save_settings(&app, &store.settings);
    Ok(webview_settings(&store.settings))
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
    Ok(webview_settings(&store.settings))
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
        members: Vec::new(),
    };
    store.settings.active_view.clone_from(&view.id);
    store.settings.views.push(view);
    save_settings(&app, &store.settings);
    Ok(webview_settings(&store.settings))
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
    webview_settings(&store.settings)
}

// Set a view's tab color (hex); empty string clears it back to default.
#[tauri::command]
fn set_view_color(app: AppHandle, state: State<Db>, id: String, color: String) -> Settings {
    let mut store = lock(&state);
    if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == id) {
        view.color = color.trim().to_string();
    }
    save_settings(&app, &store.settings);
    webview_settings(&store.settings)
}

// The tab bar's order, and a group's own member order, are both nothing
// but `settings.views`' own order (groupViews on the frontend filters that
// same array; it is never sorted separately) -- so reordering either one is
// one generic reshuffle of this Vec, never a merge/group operation, and it
// can never touch a View's own fields (`members`/`layouts` ride along
// untouched with their View no matter where it moves to).
// `order` is the caller's desired full id sequence. An id in `order` that no
// longer exists is skipped; a view left out of `order` (a stale caller, or
// one added by another window mid-drag) keeps its old relative order,
// appended after everything the caller did name -- a mismatch can reorder
// less than intended but can never drop a view.
pub(crate) fn reorder_views_by_id(mut views: Vec<View>, order: &[String]) -> Vec<View> {
    let mut reordered: Vec<View> = Vec::with_capacity(views.len());
    for id in order {
        if let Some(pos) = views.iter().position(|v| &v.id == id) {
            reordered.push(views.remove(pos));
        }
    }
    reordered.append(&mut views);
    reordered
}

#[tauri::command]
fn reorder_views(app: AppHandle, state: State<Db>, order: Vec<String>) -> Settings {
    let mut store = lock(&state);
    store.settings.views = reorder_views_by_id(std::mem::take(&mut store.settings.views), &order);
    save_settings(&app, &store.settings);
    webview_settings(&store.settings)
}

// Moves every tag colour equal to an old palette hex to its new one. Returns
// whether anything changed.
pub(crate) fn remap_tag_colors(colors: &mut HashMap<String, String>, map: &HashMap<String, String>) -> bool {
    let mut changed = false;
    for c in colors.values_mut() {
        if let Some(nc) = map.get(c.as_str()) {
            *c = nc.clone();
            changed = true;
        }
    }
    changed
}

// Recolor saved data when the global palette changes: every prompt/view whose
// color equals an old palette hex is moved to the new one (pairs = [[old,new],...]).
#[tauri::command]
fn remap_colors(app: AppHandle, state: State<Db>, pairs: Vec<(String, String)>) -> Result<(), String> {
    if pairs.is_empty() {
        return Ok(());
    }
    let map: HashMap<String, String> = pairs.into_iter().filter(|(o, n)| o != n).collect();
    if map.is_empty() {
        return Ok(());
    }
    let mut store = lock(&state);
    // Snapshot every color this can touch before mutating, so a failed save can
    // put it back exactly instead of reporting success for a remap that never
    // reached disk (a whole theme's tiles would then look remapped until restart).
    let original_prompt_colors: Vec<(String, String)> = store
        .prompts
        .iter()
        .filter(|p| map.contains_key(&p.color))
        .map(|p| (p.id.clone(), p.color.clone()))
        .collect();
    let mut prompts_changed = false;
    for p in &mut store.prompts {
        if let Some(nc) = map.get(&p.color) {
            p.color = nc.clone();
            prompts_changed = true;
        }
    }
    let restore_prompt_colors = |store: &mut Store| {
        for (id, c) in &original_prompt_colors {
            if let Some(p) = store.prompts.iter_mut().find(|p| p.id == *id) {
                p.color = c.clone();
            }
        }
    };
    if prompts_changed && !save_prompts(&app, &store) {
        restore_prompt_colors(&mut store);
        return Err("save-failed".into());
    }
    let original_view_colors: Vec<(String, String)> = store
        .settings
        .views
        .iter()
        .filter(|v| map.contains_key(&v.color))
        .map(|v| (v.id.clone(), v.color.clone()))
        .collect();
    let mut views_changed = false;
    for v in &mut store.settings.views {
        if let Some(nc) = map.get(&v.color) {
            v.color = nc.clone();
            views_changed = true;
        }
    }
    // Tag colours are picked from the same palette swatches, so a tag set to an old
    // palette hue would otherwise keep the old hex and read as a custom colour.
    let original_tag_colors = store.settings.tag_colors.clone();
    views_changed |= remap_tag_colors(&mut store.settings.tag_colors, &map);
    if views_changed && !save_settings(&app, &store.settings) {
        store.settings.tag_colors = original_tag_colors;
        for (id, c) in original_view_colors {
            if let Some(v) = store.settings.views.iter_mut().find(|v| v.id == id) {
                v.color = c;
            }
        }
        // The prompts reached disk already: put their colours back there too.
        if prompts_changed {
            restore_prompt_colors(&mut store);
            let _ = save_prompts(&app, &store);
        }
        return Err("save-failed".into());
    }
    Ok(())
}

// The settings half of deleting a view, apart from the lock and the save so it is
// testable. A grid folder of the deleted view goes with it, and it must go now:
// left behind until the next start it would still claim its members in
// normalize_tile_folders' one-folder-per-prompt rule and starve a new folder.
pub(crate) fn remove_view_from_settings(settings: &mut Settings, id: &str, prompt_ids: &[String]) -> Result<(), String> {
    if settings.views.len() <= 1 {
        return Err("cannot delete the last view".to_string());
    }
    settings.views.retain(|v| v.id != id);
    if settings.active_view == id {
        let first_id = settings.views[0].id.clone();
        settings.active_view.clone_from(&first_id);
    }
    let view_ids: Vec<String> = settings.views.iter().map(|v| v.id.clone()).collect();
    normalize_tile_folders(&mut settings.tile_folders, prompt_ids, &view_ids);
    Ok(())
}

#[tauri::command]
fn delete_view(app: AppHandle, state: State<Db>, id: String) -> Result<Settings, String> {
    let mut store = lock(&state);
    let prompt_ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    remove_view_from_settings(&mut store.settings, &id, &prompt_ids)?;
    save_settings(&app, &store.settings);
    Ok(webview_settings(&store.settings))
}

#[tauri::command]
fn set_active_view(app: AppHandle, state: State<Db>, id: String) -> Settings {
    let mut store = lock(&state);
    if store.settings.views.iter().any(|v| v.id == id) {
        store.settings.active_view = id;
    }
    save_settings(&app, &store.settings);
    webview_settings(&store.settings)
}

// ---------- Tile folder commands ----------

// Bring the grid folders back to their invariants: only real prompts and real views,
// no prompt in two folders (the first folder listing it wins), no two folders on the
// same cell of the same view (the first one keeps it, the next is moved aside by the
// renderer because its cell is cleared to the origin only if free), and no folder
// left with fewer than two members: that one dissolves and its prompts become tiles
// again. Runs on load and after every mutation. Idempotent.
// `view_ids` rather than `&[View]`: every caller only ever needs to check
// membership, so passing ids instead of whole views spares a clone of every view's
// layout maps at each call site.
pub(crate) fn normalize_tile_folders(folders: &mut Vec<TileFolder>, prompt_ids: &[String], view_ids: &[String]) {
    let mut taken: Vec<String> = Vec::new();
    for folder in folders.iter_mut() {
        // A hand-edited/imported cell far outside any real grid (up to u32::MAX)
        // must not reach the collision loop below: saturating_add stops advancing
        // once it hits the cap, which would spin the loop forever with the store
        // mutex held (app dead on startup).
        folder.cell[0] = folder.cell[0].min(GRID_MAX);
        folder.cell[1] = folder.cell[1].min(GRID_MAX);
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
        // Bounded even if every cell in a full grid is already taken: the clamp
        // above keeps this from ever needing more than GRID_MAX steps in practice,
        // but the guard is what actually guarantees termination.
        for _ in 0..=GRID_MAX {
            if !cells.iter().any(|(v, c)| v == &folder.view_id && *c == folder.cell) {
                break;
            }
            folder.cell[0] = folder.cell[0].saturating_add(1);
        }
        cells.push((folder.view_id.clone(), folder.cell));
    }
    folders.truncate(MAX_TILE_FOLDERS);
}

// Create (empty id) or update one grid folder in a single pass: name, colour, cell
// and the full member list. Prompts handed in are pulled out of every other folder.
// One parameter per field the grid-folder editor can set.
#[allow(clippy::too_many_arguments)]
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
    // A folder may stay unnamed: it then shows the member count alone.
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
    Ok(webview_settings(&store.settings))
}

// The layout step of taking a prompt out of a grid folder onto `cell`. Whoever
// already sits on that cell moves to the first free cell (folder cells count as
// taken); with no free cell left the drop is refused and nothing changes. A cell
// outside the grid places nothing, so the prompt stays library-only.
pub(crate) fn place_taken_out_member(
    view: &mut View,
    folder_cells: &[[u32; 2]],
    id: &str,
    cell: [u32; 2],
) -> Result<(), ()> {
    if cell[0] >= view.cols || cell[1] >= view.rows {
        return Ok(());
    }
    let key = grid_key(view.cols, view.rows);
    let occupants: Vec<String> = view
        .layouts
        .get(&key)
        .map(|l| l.iter().filter(|(_, c)| **c == cell).map(|(k, _)| k.clone()).collect())
        .unwrap_or_default();
    let mut moved: Vec<(String, [u32; 2])> = Vec::new();
    let mut scratch = view.clone();
    for occupant in occupants {
        let Some(free) = first_free_cell_excluding(&scratch, folder_cells) else {
            return Err(());
        };
        scratch.layouts.entry(key.clone()).or_default().insert(occupant.clone(), free);
        moved.push((occupant, free));
    }
    let layout = view.layouts.entry(key).or_default();
    for (occupant, free) in moved {
        layout.insert(occupant, free);
    }
    layout.insert(id.to_string(), cell);
    Ok(())
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
    // Every folder cell of that view lives outside view.layouts; a displaced
    // occupant must not be handed one of them.
    let folder_cells: Vec<[u32; 2]> = store
        .settings
        .tile_folders
        .iter()
        .filter(|f| f.view_id == view_id)
        .map(|f| f.cell)
        .collect();
    // Decide the placement first: a drop that cannot be honoured leaves the
    // member in its folder instead of half-moving it.
    if let Some(view) = store.settings.views.iter_mut().find(|v| v.id == view_id) {
        if place_taken_out_member(view, &folder_cells, &id, cell).is_err() {
            return Err("no free cell".to_string());
        }
    }
    for folder in store.settings.tile_folders.iter_mut() {
        folder.members.retain(|m| m != &id);
    }
    let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
    let view_ids: Vec<String> = store.settings.views.iter().map(|v| v.id.clone()).collect();
    normalize_tile_folders(&mut store.settings.tile_folders, &ids, &view_ids);
    save_settings(&app, &store.settings);
    Ok(webview_settings(&store.settings))
}

// Dissolve a grid folder: its prompts become tiles again, nothing is deleted. Each
// member lands on the first free cell of the folder's view: the folder's own cell is
// free again by the time this runs, so it is usually one of them. Cells held by any
// OTHER folder still in that view are excluded too (they live outside view.layouts,
// so first_free_cell alone would not see them: a member could otherwise be handed a
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
    webview_settings(&store.settings)
}

mod settings_cmds;
use settings_cmds::*;

// ---- Usage statistics ----
#[derive(Serialize)]
pub(crate) struct StatBar {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) count: u32,
}
#[derive(Serialize)]
pub(crate) struct ViewCount {
    pub(crate) name: String,
    pub(crate) count: usize,
}
// One day's copy count. `ts` is that day's UTC midnight (unix seconds): the
// frontend already has locale-aware date formatting via Intl, no reason to
// duplicate it (or pull in a date crate) on the Rust side for a label string.
#[derive(Serialize)]
pub(crate) struct DayCount {
    pub(crate) ts: u64,
    pub(crate) count: usize,
}
#[derive(Serialize)]
pub(crate) struct UsageStats {
    pub(crate) total_prompts: usize,
    pub(crate) total_copies: u64,
    pub(crate) copies7: u64,
    pub(crate) copies30: u64,
    pub(crate) used_count: usize,
    pub(crate) avg_copies: f64,
    pub(crate) total_chars: u64,
    pub(crate) recent_name: String,
    pub(crate) recent_ts: u64,
    pub(crate) longest_name: String,
    pub(crate) longest_chars: usize,
    pub(crate) types: Vec<ViewCount>,
    pub(crate) top: Vec<StatBar>,
    pub(crate) never_used: Vec<StatBar>,
    pub(crate) per_view: Vec<ViewCount>,
    pub(crate) daily: Vec<DayCount>, // last 14 UTC days, oldest first, zero-filled
    pub(crate) history_on: bool,
    pub(crate) tools: Vec<ToolBar>,       // most-used tools, name = the bare tool key (frontend translates it)
    pub(crate) tools_never_used: Vec<String>, // tool keys with a zero counter
}

#[derive(Serialize)]
pub(crate) struct ToolBar {
    pub(crate) name: String,
    pub(crate) count: u32,
    pub(crate) last_used: u64,
}

// ---- Most-used tools ----
// The functions this counts, each counted in the
// SAME usage/last_used maps a prompt copy uses (see record_tool_usage below),
// under a key namespace no real prompt id can ever collide with -- prompt ids
// come from gen_id(), never the fixed "tool:" prefix. This array is also the
// single source of truth a test checks the counted keys against, so a new
// tool call site can never silently go uncounted without a test noticing.
pub(crate) const TOOL_KEYS: [&str; 13] = [
    "screenshot",
    "screenshot_editor",
    "ocr",
    "recording_start",
    "video_editor",
    "video_export",
    "dictation",
    "color_pick",
    "chain",
    "rotation_advance",
    "folder_create",
    "duplicate_finder",
    "backup_create",
];

pub(crate) fn tool_usage_key(tool: &str) -> String {
    format!("tool:{tool}")
}

// Same "must be explicitly turned on" reading as the frontend's own shipsOn:
// an opt-in flag defaults to off, so only an explicit `true` counts as on.
pub(crate) fn tool_usage_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("usageStats") == Some(&true)
}

// The actual increment, split out from the command below so it is directly
// testable without a mock Tauri runtime (State<'_, Db>' cannot be constructed
// in a unit test, same reasoning as export_prompts elsewhere in this file).
pub(crate) fn apply_tool_usage(settings: &mut Settings, tool: &str) {
    let key = tool_usage_key(tool);
    let slot = settings.usage.entry(key.clone()).or_insert(0);
    *slot += 1;
    settings.last_used.insert(key, now_secs());
}

// Record one use of a tool -- called by the UI right after the action
// succeeds. Off the moment usage statistics are off: unlike record_copy's
// usage/last_used (always collected, only copyHistory's own row log is a
// privacy toggle), this counter set exists purely for the dashboard and has
// no other purpose, so nothing about it should survive the switch being off.
#[tauri::command]
async fn record_tool_usage(state: State<'_, Db>, tool: String) -> Result<(), String> {
    if !TOOL_KEYS.contains(&tool.as_str()) {
        return Err("unknown-tool".to_string());
    }
    let mut store = lock(&state);
    if !tool_usage_enabled(&store.settings) {
        return Ok(());
    }
    apply_tool_usage(&mut store.settings, &tool);
    save_settings_soon();
    Ok(())
}

// (top-N ToolBars, never-used tool keys) from the same usage/last_used maps
// usage_stats already reads for prompts -- top_n mirrors the dashboard's own
// "how many rows" field, the never-used list is always complete (it is short
// by definition: at most 13 entries).
pub(crate) fn tool_usage_stats(usage: &HashMap<String, u32>, last_used: &HashMap<String, u64>, top_n: usize) -> (Vec<ToolBar>, Vec<String>) {
    let mut used: Vec<(&str, u32, u64)> = TOOL_KEYS
        .iter()
        .filter_map(|k| {
            let n = usage.get(&tool_usage_key(k)).copied().unwrap_or(0);
            if n > 0 {
                let lu = last_used.get(&tool_usage_key(k)).copied().unwrap_or(0);
                Some((*k, n, lu))
            } else {
                None
            }
        })
        .collect();
    used.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let tools: Vec<ToolBar> = used
        .into_iter()
        .take(top_n)
        .map(|(k, n, lu)| ToolBar { name: k.to_string(), count: n, last_used: lu })
        .collect();
    let never_used: Vec<String> = TOOL_KEYS
        .iter()
        .filter(|k| usage.get(&tool_usage_key(k)).copied().unwrap_or(0) == 0)
        .map(|k| k.to_string())
        .collect();
    (tools, never_used)
}

// Coarse type of a prompt for the statistics breakdown.
pub(crate) fn prompt_stat_type(p: &Prompt) -> &'static str {
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
    // The copy journal lives in SQLite and owes nothing to the in-memory store,
    // so it is read BEFORE the store is locked. Opening the database and running
    // three queries while holding that lock made every copy and every save wait
    // for the statistics dialog to finish opening.
    let now = now_secs();
    let cutoff30 = now.saturating_sub(30 * 86400);
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
    // An archived-and-unused prompt is not flagged here too -- the user
    // already set it aside on purpose, so it does not need a second nudge.
    let archive_tag_key = store.settings.ui_texts.get("archiveTag").map(|s| tag_key(s));
    let mut never_used: Vec<StatBar> = store
        .prompts
        .iter()
        // Never copied: not "not lately", so the list means exactly what it says.
        .filter(|p| usage.get(&p.id).copied().unwrap_or(0) == 0)
        .filter(|p| !is_archived(p, archive_tag_key.as_deref()))
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
    let history_on = store.settings.ui_flags.get("copyHistory") != Some(&false);
    let (tools, tools_never_used) = tool_usage_stats(usage, last_used, top_n);
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
        tools,
        tools_never_used,
        daily,
        history_on,
    })
}

// ---- Usage statistics export ----
// One row per prompt (not just the dashboard's top-N) plus the same daily
// counts the dashboard charts. A name/tag counter is not a substitute for a
// spreadsheet someone can sort and filter on their own. `last_used`/`ts` stay
// raw unix seconds: the dashboard already formats them through Intl on the JS
// side, and duplicating a calendar calculation here for a plain text export
// would be a second place to get the leap-year math wrong.
#[derive(Serialize)]
pub(crate) struct StatsExportEntry {
    pub(crate) name: String,
    pub(crate) copies: u32,
    pub(crate) last_used: u64,
}

// Tool-usage rows join this export once per-tool usage counters exist; an
// empty slice until then keeps the JSON shape stable across both features.
pub(crate) fn stats_export_entries(store: &Store) -> Vec<StatsExportEntry> {
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
    let mut entries: Vec<StatsExportEntry> = store
        .prompts
        .iter()
        .map(|p| StatsExportEntry {
            name: name_of(p),
            copies: usage.get(&p.id).copied().unwrap_or(0),
            last_used: last_used.get(&p.id).copied().unwrap_or(0),
        })
        .collect();
    entries.sort_by(|a, b| b.copies.cmp(&a.copies).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    entries
}

pub(crate) fn stats_to_csv(entries: &[StatsExportEntry], daily: &[DayCount], tools: &[StatsExportEntry]) -> String {
    let mut rows = vec![csv_row(&["name", "copies", "last_used"])];
    for e in entries {
        rows.push(csv_row(&[&e.name, &e.copies.to_string(), &e.last_used.to_string()]));
    }
    rows.push(String::new());
    rows.push(csv_row(&["date_ts", "copies"]));
    for d in daily {
        rows.push(csv_row(&[&d.ts.to_string(), &d.count.to_string()]));
    }
    if !tools.is_empty() {
        rows.push(String::new());
        rows.push(csv_row(&["tool", "uses", "last_used"]));
        for t in tools {
            rows.push(csv_row(&[&t.name, &t.copies.to_string(), &t.last_used.to_string()]));
        }
    }
    rows.join("\r\n")
}

pub(crate) fn stats_to_txt(entries: &[StatsExportEntry], daily: &[DayCount], tools: &[StatsExportEntry]) -> String {
    use std::fmt::Write;
    let mut out = String::from("Prompts\n=======\n");
    for e in entries {
        let _ = writeln!(out, "{}: {} copies, last used {}", e.name, e.copies, e.last_used);
    }
    out.push_str("\nDaily copies\n============\n");
    for d in daily {
        let _ = writeln!(out, "{}: {}", d.ts, d.count);
    }
    if !tools.is_empty() {
        out.push_str("\nTool usage\n==========\n");
        for t in tools {
            let _ = writeln!(out, "{}: {} uses, last used {}", t.name, t.copies, t.last_used);
        }
    }
    out
}

pub(crate) fn stats_to_json(entries: &[StatsExportEntry], daily: &[DayCount], tools: &[StatsExportEntry]) -> String {
    #[derive(Serialize)]
    struct StatsExport<'a> {
        version: u32,
        entries: &'a [StatsExportEntry],
        daily: &'a [DayCount],
        tools: &'a [StatsExportEntry],
    }
    serde_json::to_string_pretty(&StatsExport { version: 1, entries, daily, tools }).unwrap_or_default()
}

// Same shape as export_prompts (format select -> save dialog), fed from the
// statistics dashboard's own button instead of the settings export row.
// The dashboard is where the data is on screen, so the "export it" action
// stays next to it rather than in an unrelated dialog.
#[tauri::command]
async fn export_stats(app: AppHandle, state: State<'_, Db>, format: String) -> Result<usize, String> {
    let daily = db_conn(&app).map(|c| db_daily_copies(&c, 14)).unwrap_or_default();
    let (entries, tools) = {
        let store = lock(&state);
        let entries = stats_export_entries(&store);
        // Every tool, not just the dashboard's top-N -- same "the export holds
        // everything, the dashboard only shows the top slice" rule the entries
        // above already follow.
        let tools: Vec<StatsExportEntry> = TOOL_KEYS
            .iter()
            .map(|k| StatsExportEntry {
                name: (*k).to_string(),
                copies: store.settings.usage.get(&tool_usage_key(k)).copied().unwrap_or(0),
                last_used: store.settings.last_used.get(&tool_usage_key(k)).copied().unwrap_or(0),
            })
            .collect();
        (entries, tools)
    };
    let count = entries.len();
    let content = match format.as_str() {
        "csv" => stats_to_csv(&entries, &daily, &tools),
        "txt" => stats_to_txt(&entries, &daily, &tools),
        "json" => stats_to_json(&entries, &daily, &tools),
        _ => return Err("unsupported-export-format".to_string()),
    };
    let file = file_dialog(&app)
        .set_file_name(format!("usage-stats.{format}"))
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

// ---- Storage breakdown (statistics/backups window, "Speicher" section) ----
// One row per storage kind the app itself writes to disk, plus the two rows a
// retention overview adds beside these once it exists. `key` is a stable id
// the frontend switches on; `path` is what an "open folder" button reveals,
// left empty for a row with no single folder to point at.
#[derive(Serialize, Clone)]
pub(crate) struct StorageItem {
    pub(crate) key: String,
    pub(crate) path: String,
    pub(crate) bytes: u64,
    pub(crate) count: u64,
    // The scan stopped at the entry cap -- the numbers above are a lower
    // bound, not the true total. Kept apart from `denied` so the frontend can
    // word the two situations differently.
    pub(crate) capped: bool,
    // The folder exists but could not be listed (a permissions problem) --
    // reported as such rather than silently shown as an empty, empty-looking
    // 0 B row.
    pub(crate) denied: bool,
}

#[derive(Serialize, Clone)]
pub(crate) struct StorageBreakdown {
    pub(crate) items: Vec<StorageItem>,
    pub(crate) total_bytes: u64,
    pub(crate) measured_at: u64,
}

pub(crate) fn file_size_or_zero(path: &std::path::Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

// (count, bytes) of every distinct, still-existing attachment path -- two
// prompts sharing the same file (a rotating entry's variants, say) must count
// that file once, not twice, and a path that no longer resolves contributes
// nothing rather than erroring the whole readout.
pub(crate) fn attachments_bytes(file_paths: &[String]) -> (u64, u64) {
    let mut seen = std::collections::HashSet::new();
    let mut bytes = 0u64;
    let mut count = 0u64;
    for path in file_paths {
        if path.is_empty() || !seen.insert(path.as_str()) {
            continue;
        }
        if let Ok(meta) = fs::metadata(path) {
            bytes += meta.len();
            count += 1;
        }
    }
    (count, bytes)
}

// (count, bytes, capped, denied) for one folder, one level deep -- matches
// video_folder_stats_in's precedent of never descending into subfolders. The
// scan stops once `cap` files have been counted, so a folder with millions of
// small files cannot hang the window open. `read_dir` is injected so a
// permissions failure can be exercised in a test without changing real ACLs.
pub(crate) fn scan_dir_stats_with(
    dir: &std::path::Path,
    cap: usize,
    read_dir: impl Fn(&std::path::Path) -> std::io::Result<std::fs::ReadDir>,
) -> (u64, u64, bool, bool) {
    let entries = match read_dir(dir) {
        Ok(e) => e,
        // A folder nothing has written to yet (the recordings folder before a
        // first capture, say) is simply empty, never an error.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (0, 0, false, false),
        Err(e) => return (0, 0, false, e.kind() == std::io::ErrorKind::PermissionDenied),
    };
    let mut count = 0u64;
    let mut bytes = 0u64;
    let mut capped = false;
    for entry in entries.flatten() {
        if count as usize >= cap {
            capped = true;
            break;
        }
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                count += 1;
                bytes += meta.len();
            }
        }
    }
    (count, bytes, capped, false)
}

pub(crate) fn scan_dir_stats(dir: &std::path::Path, cap: usize) -> (u64, u64, bool, bool) {
    scan_dir_stats_with(dir, cap, |p| std::fs::read_dir(p))
}

// A runaway folder (a sync target pointed at something huge, say) stops
// counting here rather than freezing the window open while it walks on.
pub(crate) const STORAGE_SCAN_CAP: usize = 20_000;

#[tauri::command]
async fn storage_breakdown(app: AppHandle, state: State<'_, Db>, force: bool) -> Result<StorageBreakdown, String> {
    if !force {
        if let Some(cached) = STORAGE_CACHE.lock().unwrap().clone() {
            return Ok(cached);
        }
    }
    // The edit-state byte count lives in SQLite, so it is read before the
    // store is locked, same reasoning as usage_stats' copy_log reads.
    let (edit_state_count, edit_state_bytes) = db_conn(&app)
        .map(|c| {
            let count = c.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get::<_, i64>(0)).unwrap_or(0) as u64;
            let bytes = c
                .query_row("SELECT COALESCE(SUM(LENGTH(ops)), 0) FROM edit_state", [], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as u64;
            (count, bytes)
        })
        .unwrap_or((0, 0));
    let (file_paths, screenshot_custom, recording_custom) = {
        let store = lock(&state);
        (
            store.prompts.iter().map(|p| p.file_path.clone()).collect::<Vec<_>>(),
            store.settings.ui_texts.get("screenshotDir").cloned().unwrap_or_default(),
            store.settings.ui_texts.get("recordingDir").cloned().unwrap_or_default(),
        )
    };
    let dd = data_dir(&app);
    let mut items = Vec::new();

    // Database + its write-ahead log (and shared-memory file, when SQLite has
    // one open right now).
    {
        let db = dd.join("data.db");
        let wal = dd.join("data.db-wal");
        let shm = dd.join("data.db-shm");
        let bytes = file_size_or_zero(&db) + file_size_or_zero(&wal) + file_size_or_zero(&shm);
        let count = [&db, &wal, &shm].iter().filter(|p| p.exists()).count() as u64;
        items.push(StorageItem { key: "database".into(), path: db.to_string_lossy().into_owned(), bytes, count, capped: false, denied: false });
    }

    // Attachments: a prompt's file only ever keeps its original path (the
    // import/export notes on file_path explain why), so there is no single
    // app folder to scan -- this sums whatever those paths currently point to.
    {
        let (count, bytes) = attachments_bytes(&file_paths);
        items.push(StorageItem { key: "attachments".into(), path: String::new(), bytes, count, capped: false, denied: false });
    }

    // Screenshots
    {
        let target = screenshot_dir(&screenshot_custom);
        let (count, bytes, capped, denied) = scan_dir_stats(&target, STORAGE_SCAN_CAP);
        items.push(StorageItem { key: "screenshots".into(), path: target.to_string_lossy().into_owned(), bytes, count, capped, denied });
    }

    // Recordings, plus their own "verworfen" discard trash as a separate row --
    // the same folder the expert menu's recording-folder readout already reads.
    {
        let target = recording_dir(&recording_custom);
        let (count, bytes) = video_folder_stats_in(&target);
        items.push(StorageItem { key: "recordings".into(), path: target.to_string_lossy().into_owned(), bytes, count, capped: false, denied: false });
        let discard = target.join("verworfen");
        let (dcount, dbytes) = video_folder_stats_in(&discard);
        items.push(StorageItem { key: "discarded_recordings".into(), path: discard.to_string_lossy().into_owned(), bytes: dbytes, count: dcount, capped: false, denied: false });
    }

    // Backups
    {
        let target = backups_dir(&app);
        let (count, bytes, capped, denied) = scan_dir_stats(&target, STORAGE_SCAN_CAP);
        items.push(StorageItem { key: "backups".into(), path: target.to_string_lossy().into_owned(), bytes, count, capped, denied });
    }

    // Temporary files: the scratch folder OCR, the language-pack install and
    // the update stager all write into.
    {
        let target = std::env::temp_dir().join(brand::TEMP_DIR);
        let (count, bytes, capped, denied) = scan_dir_stats(&target, STORAGE_SCAN_CAP);
        items.push(StorageItem { key: "temp_files".into(), path: target.to_string_lossy().into_owned(), bytes, count, capped, denied });
    }

    // Edit states: `ops` is the JSON blob actually held in the database; the
    // row's own `size` column is the edited file's size, not storage cost.
    items.push(StorageItem {
        key: "edit_states".into(),
        path: String::new(),
        bytes: edit_state_bytes,
        count: edit_state_count,
        capped: false,
        denied: false,
    });

    // Error log
    {
        let path = crash_log_path();
        let bytes = file_size_or_zero(&path);
        items.push(StorageItem {
            key: "crash_log".into(),
            path: path.to_string_lossy().into_owned(),
            bytes,
            count: u64::from(path.exists()),
            capped: false,
            denied: false,
        });
    }

    let total_bytes = items.iter().map(|i| i.bytes).sum();
    let fresh = StorageBreakdown { items, total_bytes, measured_at: now_secs() };
    *STORAGE_CACHE.lock().unwrap() = Some(fresh.clone());
    Ok(fresh)
}

// Cached across calls -- reopening the statistics/backups window should not
// re-walk every folder every time; the "measure again" button passes force=true.
pub(crate) static STORAGE_CACHE: Mutex<Option<StorageBreakdown>> = Mutex::new(None);

// Opens a storage-breakdown row's own folder -- the path travels from
// storage_breakdown's own computation, never from anything the user typed.
#[tauri::command]
fn open_storage_folder(path: String) -> Result<(), String> {
    open_explorer_at(&path)
}

// One row per kept-data category (trash, copy history, clipboard inbox,
// recordings, discarded recordings, temporary exports, edit states) for the
// statistics/backups window's own overview -- distinct from StorageBreakdown
// above, which is disk USAGE grouped by folder; this is RETENTION grouped by
// what a setting actually governs. Deliberately reuses the same counting each
// category already had (video_folder_stats_in for the two recording rows,
// the same SQL every other command in this file already runs against
// copy_log/clip_inbox/edit_state) rather than inventing a second way to
// measure the same folder or table.
#[derive(Serialize, Clone)]
pub(crate) struct RetentionItem {
    pub(crate) key: String,
    pub(crate) count: u64,
    pub(crate) bytes: u64,
}

#[derive(Serialize, Clone)]
pub(crate) struct RetentionOverview {
    pub(crate) items: Vec<RetentionItem>,
    pub(crate) measured_at: u64,
}

pub(crate) static RETENTION_CACHE: Mutex<Option<RetentionOverview>> = Mutex::new(None);

#[tauri::command]
async fn retention_overview(app: AppHandle, state: State<'_, Db>, force: bool) -> Result<RetentionOverview, String> {
    if !force {
        if let Some(cached) = RETENTION_CACHE.lock().unwrap().clone() {
            return Ok(cached);
        }
    }
    let (trash_count, trash_bytes, recording_custom) = {
        let store = lock(&state);
        let trash_paths: Vec<String> = store.trash.iter().map(|p| p.file_path.clone()).collect();
        let (_, bytes) = attachments_bytes(&trash_paths);
        (store.trash.len() as u64, bytes, store.settings.ui_texts.get("recordingDir").cloned().unwrap_or_default())
    };
    let (copy_count, copy_bytes) = db_conn(&app)
        .map(|c| {
            c.query_row("SELECT COUNT(*), COALESCE(SUM(LENGTH(id)), 0) FROM copy_log", [], |r| {
                Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64))
            })
            .unwrap_or((0, 0))
        })
        .unwrap_or((0, 0));
    let (clip_count, clip_bytes) = db_conn(&app)
        .map(|c| {
            c.query_row("SELECT COUNT(*), COALESCE(SUM(LENGTH(text)), 0) FROM clip_inbox", [], |r| {
                Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64))
            })
            .unwrap_or((0, 0))
        })
        .unwrap_or((0, 0));
    let rec_dir = recording_dir(&recording_custom);
    let (rec_count, rec_bytes) = video_folder_stats_in(&rec_dir);
    let (discarded_count, discarded_bytes) = video_folder_stats_in(&rec_dir.join("verworfen"));
    let (temp_count, temp_bytes) = clip_export_temp_stats();
    let (edit_count, edit_bytes) = db_conn(&app)
        .map(|c| {
            let count = c.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get::<_, i64>(0)).unwrap_or(0) as u64;
            let bytes = c
                .query_row("SELECT COALESCE(SUM(LENGTH(ops)), 0) FROM edit_state", [], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as u64;
            (count, bytes)
        })
        .unwrap_or((0, 0));
    let items = vec![
        RetentionItem { key: "trash".into(), count: trash_count, bytes: trash_bytes },
        RetentionItem { key: "copyHistory".into(), count: copy_count, bytes: copy_bytes },
        RetentionItem { key: "clipInbox".into(), count: clip_count, bytes: clip_bytes },
        RetentionItem { key: "recordings".into(), count: rec_count, bytes: rec_bytes },
        RetentionItem { key: "discardedRecordings".into(), count: discarded_count, bytes: discarded_bytes },
        RetentionItem { key: "tempExports".into(), count: temp_count, bytes: temp_bytes },
        RetentionItem { key: "editStates".into(), count: edit_count, bytes: edit_bytes },
    ];
    let fresh = RetentionOverview { items, measured_at: now_secs() };
    *RETENTION_CACHE.lock().unwrap() = Some(fresh.clone());
    Ok(fresh)
}

// ---- Duplicate finder ----
#[derive(Serialize)]
pub(crate) struct DupMember {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) usage: u32,
    pub(crate) views: Vec<String>,
}
#[derive(Serialize)]
pub(crate) struct DupGroup {
    pub(crate) score: f64,
    pub(crate) members: Vec<DupMember>,
}

pub(crate) fn normalize_dup(s: &str) -> String {
    s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}
pub(crate) fn dup_trigrams(s: &str) -> std::collections::HashSet<[char; 3]> {
    let chars: Vec<char> = s.chars().collect();
    let mut set = std::collections::HashSet::new();
    for w in chars.windows(3) {
        set.insert([w[0], w[1], w[2]]);
    }
    set
}
pub(crate) fn dup_dice(a: &std::collections::HashSet<[char; 3]>, b: &std::collections::HashSet<[char; 3]>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.iter().filter(|t| b.contains(*t)).count();
    2.0 * inter as f64 / (a.len() + b.len()) as f64
}
pub(crate) fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

// True if some previously-ignored group contains BOTH ids - a pair only
// counts as ignored when some single ignore_dups call covered them together,
// not just because each independently belongs to some (possibly different)
// ignored group.
pub(crate) fn dup_pair_is_ignored(groups: &[Vec<String>], a: &str, b: &str) -> bool {
    groups.iter().any(|g| g.iter().any(|id| id == a) && g.iter().any(|id| id == b))
}

// The few fields the duplicate scan reads. Cloning whole prompts under the store
// lock would copy every stored image (hundreds of kilobytes each) for nothing.
// A confidential entry is left out here: its stored text is ciphertext.
pub(crate) struct DupSource {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) text: String,
}

pub(crate) fn dup_sources(prompts: &[Prompt]) -> Vec<DupSource> {
    prompts
        .iter()
        .filter(|p| !p.secret)
        .map(|p| DupSource { id: p.id.clone(), name: p.name.clone(), text: p.text.clone() })
        .collect()
}

// Find near-duplicate prompts by trigram/Dice similarity. Rayon-parallel over the
// upper-triangle of pairs; 1000 prompts finish well under a second.
#[tauri::command]
async fn find_duplicates(state: State<'_, Db>, threshold: f64) -> Result<Vec<DupGroup>, String> {
    use rayon::prelude::*;
    use std::collections::{HashMap as Map, HashSet as Set};
    // Clone the (few) fields this scan needs and drop the lock before the O(n^2)
    // comparison below - this used to hold the store mutex for the whole scan,
    // blocking every other command (add/update/delete, all of which also need this
    // same mutex) for however long comparing a few hundred prompts took. The result
    // is a snapshot - a prompt changed concurrently mid-scan might be very slightly
    // stale in this one report, an acceptable trade for a read-only finder.
    let (prompts, dup_ignored, views, usage) = {
        let store = lock(&state);
        (
            dup_sources(&store.prompts),
            store.settings.dup_ignored.clone(),
            store.settings.views.clone(),
            store.settings.usage.clone(),
        )
    };
    // Floor, not just a 0..1 clamp: at a threshold of 0 every pair passes and the
    // edge list below becomes the full n^2 triangle. The setting itself never goes
    // below 70 %, so this only bounds what a malformed call could ask for.
    let thr = (threshold / 100.0).clamp(0.5, 1.0);
    // A rotating prompt is compared on entry 1 (p.text) only, deliberately:
    // two prompts that happen to share their FIRST entry but differ in the rest are
    // not "near duplicates" of each other, and comparing every entry pair would
    // turn this already-O(n^2) scan into O(n^2 * entries).
    let items: Vec<(&DupSource, String)> = prompts
        .iter()
        // Confidential entries were already left out of the snapshot (their stored
        // text is ciphertext, never a real body to compare).
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
    let edges: Vec<(usize, usize, f64)> = (0..n)
        .into_par_iter()
        .flat_map(|i| {
            let gi = &grams[i];
            let mut local = Vec::new();
            for j in (i + 1)..n {
                let d = dup_dice(gi, &grams[j]);
                if d >= thr && !dup_pair_is_ignored(&dup_ignored, &items[i].0.id, &items[j].0.id) {
                    local.push((i, j, d));
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
    for v in &views {
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
                        usage: usage.get(&p.id).copied().unwrap_or(0),
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

// ---- Similar-on-save hint ----
// How long the after-save similarity scan may run before it gives up and
// reports nothing rather than risk being felt as a stall. find_duplicates'
// own O(n^2) scan above does not need a budget like this one -- that one is
// an explicit "scan now" action the user already chose to wait for, while
// this one runs after every ordinary save, unasked.
pub(crate) const SIMILAR_HINT_BUDGET: std::time::Duration = std::time::Duration::from_millis(40);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SimilarHit {
    pub(crate) id: String,
    pub(crate) name: String,
}

// Whether two normalized lengths could possibly clear `thr` under trigram
// Dice similarity. Dice can never exceed 2*min/(min+max), even at a perfect
// overlap of the shorter one into the longer, so a pair whose lengths differ
// enough is provably impossible and can be skipped before ever paying for
// dup_trigrams on it -- the "only against entries of similar length" half of
// the time budget.
pub(crate) fn dup_length_could_match(a: usize, b: usize, thr: f64) -> bool {
    if a == 0 || b == 0 {
        return false;
    }
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    2.0 * lo as f64 / (hi + lo) as f64 >= thr
}

// The same trigram/Dice calculation find_duplicates uses above, run against
// just the ONE prompt that was saved instead of every pair in the store.
// Pure and synchronous, so it is callable straight from a unit test without a
// Db/AppHandle; the command below only adds the store lookup around it.
// Time-boxed (SIMILAR_HINT_BUDGET) and length-filtered, so a 10,000-entry
// store does not turn an ordinary save into a stall -- callers run it AFTER
// the save already returned, so even reaching the budget only delays the
// hint, never the save itself.
pub(crate) fn find_similar_prompt(target: &Prompt, others: &[&Prompt], threshold: f64) -> Option<SimilarHit> {
    let start = std::time::Instant::now();
    let thr = (threshold / 100.0).clamp(0.5, 1.0);
    let base = if !target.text.is_empty() { target.text.as_str() } else { target.name.as_str() };
    let target_norm = normalize_dup(base);
    let target_len = target_norm.chars().count();
    if target_len < 3 {
        return None;
    }
    let target_grams = dup_trigrams(&target_norm);
    let mut best: Option<(SimilarHit, f64)> = None;
    for p in others {
        if start.elapsed() > SIMILAR_HINT_BUDGET {
            break;
        }
        let base = if !p.text.is_empty() { p.text.as_str() } else { p.name.as_str() };
        let norm = normalize_dup(base);
        let len = norm.chars().count();
        if len < 3 || !dup_length_could_match(target_len, len, thr) {
            continue;
        }
        let d = dup_dice(&target_grams, &dup_trigrams(&norm));
        if d >= thr && best.as_ref().map_or(true, |(_, b)| d > *b) {
            best = Some((
                SimilarHit { id: p.id.clone(), name: if p.name.is_empty() { p.text.chars().take(40).collect() } else { p.name.clone() } },
                d,
            ));
        }
    }
    best.map(|(hit, _)| hit)
}

// Called from the frontend right after add_prompt/update_prompt already
// returned -- never awaited as part of saving, so a slow scan can only ever
// delay the hint toast, not the save. A missing id (deleted again in the
// instant between save and this call) is simply "nothing to report", not an
// error worth surfacing.
#[tauri::command]
async fn similar_to_saved(state: State<'_, Db>, id: String, threshold: f64) -> Result<Option<SimilarHit>, String> {
    // Snapshot and drop the lock before the scan, same tradeoff find_duplicates
    // makes above and for the same reason -- this must not block every other
    // command for the length of the (bounded, but still real) comparison.
    let (target, others) = {
        let store = lock(&state);
        let Some(target) = store.prompts.iter().find(|p| p.id == id).cloned() else {
            return Ok(None);
        };
        let others: Vec<Prompt> = store.prompts.iter().filter(|p| p.id != id).cloned().collect();
        (target, others)
    };
    let others_ref: Vec<&Prompt> = others.iter().collect();
    Ok(find_similar_prompt(&target, &others_ref, threshold))
}

// Mark this whole group of prompt ids as "not duplicates" (of each other).
// One entry for the whole group instead of every pairwise combination - see the
// dup_ignored field comment for why.
#[tauri::command]
async fn ignore_dups(app: AppHandle, state: State<'_, Db>, ids: Vec<String>) -> Result<(), String> {
    if ids.len() < 2 {
        return Ok(()); // nothing to ignore a duplicate relationship between
    }
    let mut store = lock(&state);
    let mut sorted = ids;
    sorted.sort();
    sorted.dedup();
    // Skip an exact repeat of an already-stored group (e.g. a double-click) so this
    // cannot grow unbounded from the same action fired twice.
    if !store.settings.dup_ignored.iter().any(|g| {
        let mut existing = g.clone();
        existing.sort();
        existing == sorted
    }) {
        store.settings.dup_ignored.push(sorted);
    }
    save_settings(&app, &store.settings);
    Ok(())
}

mod backups;
use backups::*;

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

// The ink rule a satellite window fetches once at load, before the first
// theme-changed event arrives.
#[tauri::command]
fn ink_prefs(state: State<Db>) -> InkPrefs {
    ink_prefs_of(&lock(&state).settings)
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
        webview_settings(&store.settings)
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

// async like every other file-picker command (pick_file_path, pick_folder,
// pick_backup_file) -- the native save/open dialog can sit open for as long as
// the user takes, and a plain fn would block the dispatch thread (and every
// OTHER command waiting on it) for the whole time instead of just this one.
#[tauri::command]
async fn export_custom_theme(app: AppHandle, state: State<'_, Db>, id: String) -> Result<(), String> {
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
// is never trusted more than a value typed into the editor by hand: a malformed
// or hand-edited JSON file gets the exact same rejection a bad manual entry would.
// async: same reasoning as export_custom_theme above.
#[tauri::command]
async fn import_custom_theme(app: AppHandle, state: State<'_, Db>) -> Result<Settings, String> {
    let file = file_dialog(&app).add_filter("JSON", &["json"]).pick_file();
    let Some(path) = file else {
        return Err("canceled".to_string());
    };
    let content = String::from_utf8(read_file_capped(&path, MAX_THEME_BYTES)?)
        .map_err(|_| "invalid theme file".to_string())?;
    let parsed: ThemeExport = serde_json::from_str(&content).map_err(|_| "invalid theme file".to_string())?;
    save_custom_theme(app, state, String::new(), parsed.name, parsed.base, parsed.vars)
}

// The single place that decides what text a copy puts on the clipboard. The
// private notes are deliberately not part of it: every copy path goes through
// here (or through copy_text with an already-resolved body), so a note can never
// leak into the clipboard.
pub(crate) fn copy_payload(p: &Prompt) -> &str {
    &p.text
}

// Absolute ceiling regardless of the expert value rotateMaxEntries (a UI-level
// soft cap on how many the editor lets you ADD): this is the hard backend floor
// against a corrupted or hand-edited settings/import file.
pub(crate) const MAX_ROTATE_VARIANTS: usize = 49; // + entry 0 (text) = 50 total, matching rotateMaxEntries' own max

// Trims, drops empty entries and enforces the hard ceiling: "leere Einträge
// werden beim Speichern verworfen" applies to add_prompt/update_prompt alike.
pub(crate) fn sanitize_variants(list: Vec<String>) -> Vec<String> {
    list.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(MAX_ROTATE_VARIANTS)
        .collect()
}

// Only "random" is ever stored as anything other than "" (order); a prompt
// with no variants has nothing to rotate, so its mode is meaningless: keeping it
// empty avoids a stale mode surviving in storage past the point it can affect anything.
pub(crate) fn sanitize_rotate_mode(mode: String, variants: &[String]) -> String {
    if variants.is_empty() {
        String::new()
    } else if mode == "random" {
        "random".to_string()
    } else {
        String::new()
    }
}

// Same reasoning as sanitize_rotate_mode: without variants, auto-advance
// has nothing to advance through, so it can never survive as a stored true.
pub(crate) fn sanitize_rotate_auto(auto: bool, variants: &[String]) -> bool {
    auto && !variants.is_empty()
}

// "text" is entry 0 (every non-rotating consumer stays correct unchanged),
// `variants` hold entries 1..n.
pub(crate) fn rotation_total(p: &Prompt) -> usize {
    1 + p.variants.len()
}

pub(crate) fn rotation_entry_text(p: &Prompt, idx: usize) -> String {
    if idx == 0 {
        copy_payload(p).to_string() // Keep the one place that defines "entry 0"
    } else {
        p.variants.get(idx - 1).cloned().unwrap_or_else(|| copy_payload(p).to_string())
    }
}

// Which entry the NEXT copy would use, without touching any state: so a tile's
// "next: n/total" badge and the actual copy always agree, no matter how many times
// this is called between two real copies. The global "rotatingPrompts" flag off, or
// a prompt with no variants, always resolves to entry 0: the list stays saved on
// the prompt, it is just not served.
// A stored pointer/bag that no longer fits the current entry count (edited since)
// is never trusted blindly: `% total` keeps a stale `rotate_pos` in range, and any
// bag containing an out-of-range index is treated as absent (discarded on the next
// rotation_advance) instead of ever indexing past the end.
// Alongside `idx` (the absolute entry position, used to resolve the text),
// also returns `pos`: a 1-based "draw number within the current cycle", for a
// badge that counts up 1..total and restarts, instead of random mode's `idx`
// jumping to an arbitrary absolute position each time. Order mode's `idx` is
// already such a natural count (the pointer only ever moves forward), so
// `pos` there is just `idx + 1`; random mode derives it from how many entries
// remain undrawn in the current shuffle bag (`total - bag.len() + 1`): a
// freshly reshuffled, still-untouched bag always holds exactly `total`
// entries, so the very first upcoming draw of a cycle is always `1`.
pub(crate) fn rotation_next(p: &Prompt, settings: &Settings) -> (usize, usize, String) {
    let total = rotation_total(p);
    let (idx, pos) = if p.variants.is_empty() || settings.ui_flags.get("rotatingPrompts") == Some(&false) {
        (0, 1)
    } else if p.rotate_mode == "random" {
        match settings.rotate_bag.get(&p.id) {
            Some(bag) if !bag.is_empty() && bag.iter().all(|&i| (i as usize) < total) => {
                (bag[0] as usize, total - bag.len() + 1)
            }
            _ => (0, 1),
        }
    } else {
        let idx = settings.rotate_pos.get(&p.id).copied().unwrap_or(0) as usize % total;
        (idx, idx + 1)
    };
    (idx, pos, rotation_entry_text(p, idx))
}

// Advances the pointer past the entry rotation_next() just reported: call ONLY
// once the copy that used it actually reached the clipboard, so a failed copy never
// burns an entry. Random mode pops the bag's shown front and, once that empties it
// (or the bag was stale/absent, per rotation_next's own validity check), reshuffles
// a fresh permutation of every OTHER index with the just-shown one placed last: a
// full cycle (every entry exactly once) always separates two showings of the same
// entry, including across the reshuffle boundary.
pub(crate) fn rotation_advance(p: &Prompt, settings: &mut Settings) {
    // Same gate as rotation_next(): while the feature is globally off the pointer
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

// Drops both rotation entries once a prompt is gone for good: called from the
// same places that already purge per-prompt state (`purge_ids`), not from the
// soft "move to trash" step (a trashed-then-restored prompt keeps its position).
pub(crate) fn rotation_forget(settings: &mut Settings, ids: &[String]) {
    for id in ids {
        settings.rotate_pos.remove(id);
        settings.rotate_bag.remove(id);
    }
}

// The single place that decides whether a text copy goes to the clipboard
// normally or opens an auto-advance session (Windows only: delayed rendering is a
// Win32 mechanism, see mod clipwatch). `id` is None for a text copy that never
// carries rotation state (e.g. a resolved placeholder fill copied ad hoc).
#[cfg(windows)]
pub(crate) fn put_text(state: &State<Db>, id: Option<&str>, text: &str) -> bool {
    if let Some(id) = id {
        let auto = {
            let store = lock(state);
            store.prompts.iter().find(|p| p.id == id)
                .map(|p| auto_eligible(p, &store.settings))
                .unwrap_or(false)
        };
        if auto {
            clipwatch::arm(id, text, Vec::new());
            return true;
        }
    }
    note_own_copy(text);
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())).is_ok()
}
#[cfg(not(windows))]
pub(crate) fn put_text(_state: &State<Db>, _id: Option<&str>, text: &str) -> bool {
    note_own_copy(text);
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())).is_ok()
}

// The most-called command in the app (every tile/pill/hotkey copy goes through it),
// so it clones only the ONE field the chosen branch actually needs: not the whole
// Prompt, which on an image prompt meant cloning that base64 blob on every
// text-branch check too.
pub(crate) enum CopyPayload {
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
            // rotation_next() itself falls back to p.text when there is nothing
            // to rotate, so a plain prompt goes through this exact same branch.
            // A confidential entry is decrypted here with the session key, or the
            // copy is refused outright while the vault is locked -- ciphertext
            // must never reach the clipboard as if it were the real text.
            Some(p) => {
                let key = secret_session_key(secret_timeout_min(&store.settings));
                let (text, variants) = reveal_secret_text(p, key)?;
                let mut plain = p.clone();
                plain.text = text;
                plain.variants = variants;
                CopyPayload::Text(rotation_next(&plain, &store.settings).2)
            }
            None => CopyPayload::Missing,
        }
    };
    let is_text = matches!(&payload, CopyPayload::Text(_));
    let ok = match payload {
        CopyPayload::Image(img) => copy_image_to_clipboard(&img),
        CopyPayload::File(path) => set_clipboard_file(&path),
        // Put_text is the one place that decides plain copy vs. auto-session.
        CopyPayload::Text(text) => put_text(&state, Some(&id), &text),
        CopyPayload::Missing => false,
    };
    // Only a successful TEXT copy may consume a rotation entry: a failed copy
    // must leave the pointer exactly where it was, and an Image/File payload never
    // showed any entry at all (the copy_image/file_path branches above short-circuit
    // before rotation_next ever runs) so it must not silently advance it either.
    if ok && is_text {
        let mut store = lock(&state);
        if let Some(pos) = store.prompts.iter().position(|p| p.id == id) {
            let p = store.prompts[pos].clone();
            rotation_advance(&p, &mut store.settings);
            // The pointer survives a restart. Collected rather than written per
            // copy: it is already correct in memory for the next copy.
            save_settings_soon();
            emit_rotation_changed(&app, &p, &store.settings);
        }
    }
    Ok(ok)
}

// What rotation_peek() reports for a "next: n/total" badge or a copy path that
// needs the resolved text itself (chaining, drag-out, hotkeys, tray, quick-launch)
// BEFORE it goes through variable resolution: those all peek, resolve, copy_text,
// then commit, so a cancelled variable dialog never burns an entry.
#[derive(Serialize)]
pub(crate) struct RotationPeek {
    pub(crate) index: usize,
    // 1-based "draw number within the current cycle": see rotation_next.
    pub(crate) pos: usize,
    pub(crate) total: usize,
    pub(crate) text: String,
}

#[tauri::command]
fn rotation_peek(state: State<Db>, id: String) -> Option<RotationPeek> {
    let store = lock(&state);
    let p = store.prompts.iter().find(|p| p.id == id)?;
    // A locked confidential entry has no peek at all -- the "next" badge simply
    // does not show rather than reveal ciphertext or the wrong text.
    let key = secret_session_key(secret_timeout_min(&store.settings));
    let (text, variants) = reveal_secret_text(p, key).ok()?;
    let mut plain = p.clone();
    plain.text = text;
    plain.variants = variants;
    let (index, pos, text) = rotation_next(&plain, &store.settings);
    Some(RotationPeek { index, pos, total: rotation_total(&plain), text })
}

// Every window's tile/pill/folder-member badge stays live off ONE event, fired
// wherever the pointer actually moves (copy_prompt's own success path, this command)
//: the same "backend decides, frontend just reflects" split as prompt-updated.
#[derive(Serialize, Clone)]
pub(crate) struct RotationChanged {
    pub(crate) id: String,
    pub(crate) index: usize,
    // 1-based "draw number within the current cycle": see rotation_next.
    pub(crate) pos: usize,
    pub(crate) total: usize,
    pub(crate) text: String,
}

pub(crate) fn emit_rotation_changed(app: &AppHandle, p: &Prompt, settings: &Settings) {
    if p.variants.is_empty() {
        return;
    }
    // Same rule as rotation_peek: a locked confidential entry emits nothing
    // rather than send ciphertext through a window event.
    let key = secret_session_key(secret_timeout_min(settings));
    let Ok((text, variants)) = reveal_secret_text(p, key) else {
        return;
    };
    let mut plain = p.clone();
    plain.text = text;
    plain.variants = variants;
    let (index, pos, text) = rotation_next(&plain, settings);
    let _ = app.emit(
        "rotation-changed",
        RotationChanged { id: plain.id.clone(), index, pos, total: rotation_total(&plain), text },
    );
}

#[tauri::command]
fn rotation_commit(app: AppHandle, state: State<Db>, id: String) {
    let mut store = lock(&state);
    if let Some(pos) = store.prompts.iter().position(|p| p.id == id) {
        let p = store.prompts[pos].clone();
        rotation_advance(&p, &mut store.settings);
        // The hottest settings write in the app: this runs on every paste of an
        // auto-advancing prompt, so it is collected instead of hitting the disk.
        save_settings_soon();
        emit_rotation_changed(&app, &p, &store.settings);
    }
}

// May this prompt start an auto-session (advance on every paste, not just
// on copy)? Image/file prompts have no rotation_next text path at all, so they
// are excluded here rather than relying on callers to check separately.
pub(crate) fn auto_eligible(p: &Prompt, s: &Settings) -> bool {
    s.ui_flags.get("rotatingPrompts") != Some(&false)
        && !p.variants.is_empty()
        && p.rotate_auto
        && s.ui_flags.get("rotateAuto") != Some(&false)
        && p.image.is_empty()
        && p.file_path.is_empty()
}

// Editor's "From the start" button: back to entry 1, same as a freshly
// created rotating prompt, without touching the entries themselves.
#[tauri::command]
fn rotation_reset(app: AppHandle, state: State<Db>, id: String) {
    let mut store = lock(&state);
    rotation_forget(&mut store.settings, std::slice::from_ref(&id));
    save_settings(&app, &store.settings);
}

// The placeholder round's entry point. The frontend resolves every
// entry's placeholders ONCE (union of variables across all of them, one dialog)
// and hands back `resolved` in absolute entry order; this arms entry
// rotation_next() currently points at and advances the pointer: replacing what
// copy_prompt would otherwise have done for this one copy, so it must never be
// called together with copy_prompt for the same click.
#[tauri::command]
fn rotation_arm_session(app: AppHandle, state: State<Db>, id: String, resolved: Vec<String>) -> bool {
    let mut store = lock(&state);
    let Some(p) = store.prompts.iter().find(|p| p.id == id).cloned() else {
        return false;
    };
    if !auto_eligible(&p, &store.settings) {
        return false;
    }
    let (idx, _, raw) = rotation_next(&p, &store.settings);
    let text = resolved.get(idx).cloned().unwrap_or(raw);
    rotation_advance(&p, &mut store.settings);
    save_settings_soon(); // pointer only: collected, see SETTINGS_QUIET_MS
    emit_rotation_changed(&app, &p, &store.settings);
    drop(store);
    #[cfg(windows)]
    clipwatch::arm(&id, &text, resolved);
    #[cfg(not(windows))]
    {
        let _ = resolved;
        note_own_copy(&text);
        let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(text));
    }
    true
}

// The tile menu / editor Auto toggle switching off while its session is the
// one running: end it immediately instead of waiting for the next paste/copy.
// A no-op if `id` is not the active session (stale click, already ended itself).
#[tauri::command]
fn rotation_end_auto(id: String) {
    #[cfg(windows)]
    clipwatch::end_if(&id);
    #[cfg(not(windows))]
    let _ = id;
}

// Put arbitrary text on the clipboard: used after filling prompt placeholders,
// chains, drag-out, hotkeys, tray and quick-launch. `id` is the source
// prompt when there is one, so an auto-eligible prompt still opens its session
// even when the actual copy went through variable resolution first: a plain
// copy_prompt call for it would have double-resolved the placeholders.
#[tauri::command]
async fn copy_text(state: State<'_, Db>, text: String, id: Option<String>) -> Result<bool, String> {
    Ok(put_text(&state, id.as_deref(), &text))
}

// The CF_HTML clipboard payload, built by hand rather than through
// clipboard-win's own header writer, so the byte math has a plain function a
// unit test can call directly.
// Reference: https://learn.microsoft.com/windows/win32/dataxchg/html-clipboard-format
// Every offset is a BYTE position into this string's UTF-8 encoding (never a
// char or UTF-16 count, which is what the format itself requires and what
// makes a multi-byte character in the fragment worth its own test).
pub(crate) const CF_HTML_BODY_HEAD: &str = "<html>\r\n<body>\r\n<!--StartFragment-->";
pub(crate) const CF_HTML_BODY_FOOT: &str = "<!--EndFragment-->\r\n</body>\r\n</html>";

pub(crate) fn cf_html_header(start_html: usize, end_html: usize, start_fragment: usize, end_fragment: usize) -> String {
    format!(
        "Version:0.9\r\nStartHTML:{start_html:0>10}\r\nEndHTML:{end_html:0>10}\r\nStartFragment:{start_fragment:0>10}\r\nEndFragment:{end_fragment:0>10}\r\n"
    )
}

pub(crate) fn build_cf_html(fragment: &str) -> String {
    // The header's own placeholders are fixed-width (10 digits each), so its
    // byte length is already known before any real offset is computed.
    let header_len = cf_html_header(0, 0, 0, 0).len();
    let start_html = header_len;
    let start_fragment = header_len + CF_HTML_BODY_HEAD.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + CF_HTML_BODY_FOOT.len();
    let header = cf_html_header(start_html, end_html, start_fragment, end_fragment);
    format!("{header}{CF_HTML_BODY_HEAD}{fragment}{CF_HTML_BODY_FOOT}")
}

// Puts both the plain Markdown text (CF_UNICODETEXT) and its rendered form
// (CF_HTML) on the clipboard in one go, so an app that only reads CF_HTML sees
// formatting and one that only reads plain text sees exactly what a normal
// copy would have given it. `html` is already escaped and sanitised by the
// caller (the same renderer the Markdown preview uses), so this never
// inspects or trusts it beyond handing it to Windows unchanged. set_string
// empties the clipboard and claims CF_UNICODETEXT; the CF_HTML payload is
// then added under its own registered format WITHOUT clearing what was just
// set; the two calls together are what makes one clipboard entry carry both.
#[cfg(windows)]
pub(crate) fn put_formatted(text: &str, html: &str) -> bool {
    let Ok(_clip) = clipboard_win::Clipboard::new_attempts(10) else {
        return false;
    };
    if clipboard_win::raw::set_string(text).is_err() {
        return false;
    }
    let Some(html_fmt) = clipboard_win::formats::Html::new() else {
        return false;
    };
    let payload = build_cf_html(html);
    clipboard_win::raw::set_without_clear(html_fmt.code(), payload.as_bytes()).is_ok()
}
#[cfg(not(windows))]
pub(crate) fn put_formatted(_text: &str, _html: &str) -> bool {
    false
}

#[tauri::command]
async fn copy_formatted(text: String, html: String) -> Result<bool, String> {
    let ok = put_formatted(&text, &html);
    if ok {
        note_own_copy(&text);
    }
    Ok(ok)
}

// `{#{clipboard}#}`: the clipboard's current text for the placeholder resolver.
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
pub(crate) const COPY_HISTORY_MAX: usize = 1_000_000;

// One copy-history entry: which prompt + when (unix seconds).
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct CopyEntry {
    pub(crate) id: String,
    pub(crate) ts: u64,
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Retention window in seconds from the expert "history retention" setting
// (days). Default 30 days for a new install; 0 or negative = forever. Stores
// older than this default are pinned to 0 in migrate().
pub(crate) const HISTORY_DAYS_DEFAULT: f64 = 30.0;
pub(crate) const RECORDING_RETENTION_DAYS_DEFAULT: f64 = 90.0;
pub(crate) fn history_max_age(settings: &Settings) -> Option<u64> {
    let days = settings.ui_values.get("historyDays").copied().unwrap_or(HISTORY_DAYS_DEFAULT);
    if days <= 0.0 {
        None
    } else {
        Some(days as u64 * 86_400)
    }
}

// History length is an expert value (default 100, ceiling COPY_HISTORY_MAX), paired
// with the retention-window cutoff: the two limits `db_trim_copy` always applies
// together. One place for both, so startup trimming and live recording can't drift.
pub(crate) fn history_cap_and_cutoff(settings: &Settings) -> (usize, Option<u64>) {
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
pub(crate) fn prune_history(settings: &mut Settings) {
    if let Some(max_age) = history_max_age(settings) {
        let now = now_secs();
        settings.copy_log.retain(|e| now.saturating_sub(e.ts) <= max_age);
    }
}

// The copy-history row's on-demand trigger. db_trim_copy already runs
// after every insert, but only against the cap/cutoff live at THAT moment --
// lowering historyMax or historyDays does not retroactively trim until the
// next copy, so this exists to apply whatever the settings say right now.
// Row count before/after (never a byte estimate stored nowhere -- copy_log
// only ever holds a prompt id and a timestamp) is the honest "size" figure.
#[tauri::command]
fn trim_copy_history_now(app: AppHandle, state: State<'_, Db>) -> (u32, u64) {
    let Some(conn) = db_conn(&app) else { return (0, 0) };
    let (cap, cutoff) = { history_cap_and_cutoff(&lock(&state).settings) };
    let before: (i64, i64) = conn
        .query_row("SELECT COUNT(*), COALESCE(SUM(LENGTH(id)), 0) FROM copy_log", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap_or((0, 0));
    if db_trim_copy(&conn, cap, cutoff).is_err() {
        return (0, 0);
    }
    let after: (i64, i64) = conn
        .query_row("SELECT COUNT(*), COALESCE(SUM(LENGTH(id)), 0) FROM copy_log", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap_or(before);
    ((before.0 - after.0).max(0) as u32, (before.1 - after.1).max(0) as u64)
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
        let usage = {
            let slot = store.settings.usage.entry(id.clone()).or_insert(0);
            *slot += 1;
            *slot
        };
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
        // Usage count + "last used" stamp only: collected, see SETTINGS_QUIET_MS.
        // The history row itself still goes into its own table below, untouched.
        save_settings_soon();
        // The main window's own settings.usage/last_used mirror is only ever
        // refreshed by a handful of explicit refetches (opening the journal,
        // "copy again") -- a copy recorded from a floating pill, the folder
        // window or the tray left it stale everywhere else, so "most used" /
        // "recently used" library sort silently froze at whatever it read last.
        let _ = app.emit_to(
            "main",
            "usage-changed",
            serde_json::json!({ "id": id, "usage": usage, "lastUsed": if ts > 0 { Some(ts) } else { None } }),
        );
        (CopyEntry { id, ts }, cap, cutoff, dedup)
    };
    // The history itself lives in its own table: appended outside the lock so a
    // huge log never blocks other DB work and never bloats the settings blob.
    if let Some(conn) = db_conn(&app) {
        if dedup {
            let last: Option<String> = conn
                .query_row("SELECT id FROM copy_log ORDER BY rowid DESC LIMIT 1", [], |r| r.get(0))
                .ok();
            if last.as_deref() == Some(entry.id.as_str()) {
                return Ok(()); // identical consecutive copy: don't add another row
            }
        }
        // An insert failure used to be swallowed here and still report success,
        // so the entry was silently missing from the journal with no trace anywhere.
        if let Err(e) = db_append_copy(&conn, &entry, cap, cutoff) {
            append_crash_log(&format!("copy history append failed: {e}"));
            return Err("history-failed".into());
        }
    }
    rebuild_tray(&app); // The "recently used" section follows every copy
    Ok(())
}

// The counters go only once the log rows are gone: a delete that failed leaves
// both the rows and the counters as they were, so the two never disagree.
pub(crate) fn clear_history_core(delete_log: impl FnOnce() -> bool, settings: &mut Settings) -> bool {
    if !delete_log() {
        return false;
    }
    settings.usage.clear();
    settings.last_used.clear();
    true
}

// Wipe copy history + usage counters (privacy / journal "clear").
#[tauri::command]
async fn clear_copy_history(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    // A "clear" the user asked for is a privacy action -- a DB error here must
    // not be reported as success while the log rows are still on disk.
    let (cleared, saved) = {
        let mut store = lock(&state);
        let cleared = clear_history_core(
            || match db_conn(&app) {
                Some(conn) => conn.execute("DELETE FROM copy_log", []).is_ok(),
                None => false,
            },
            &mut store.settings,
        );
        (cleared, cleared && save_settings(&app, &store.settings))
    };
    rebuild_tray(&app); // A wiped history empties the "recently used" section
    if !cleared {
        append_crash_log("clear_copy_history: copy_log could not be cleared");
        return Err("delete-incomplete".into());
    }
    if !saved {
        return Err("save-failed".into());
    }
    Ok(())
}

// One row as sent back by the frontend's own pre-clear snapshot (recent_copies
// already returns the same shape, just with an extra rowid the frontend drops).
#[derive(Deserialize)]
pub(crate) struct CopyLogRow {
    pub(crate) ts: u64,
    pub(crate) id: String,
}

// The insert loop behind restore_copy_history, kept apart from the command so it
// can be exercised with a bare in-memory connection. One transaction: a row that
// fails leaves the table exactly as it was, never a half-restored history.
pub(crate) fn db_restore_copy_log(conn: &Connection, rows: &[CopyLogRow]) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for row in rows {
        tx.execute("INSERT INTO copy_log(ts, id) VALUES(?1, ?2)", params![row.ts as i64, row.id])?;
    }
    tx.commit()
}

// Undo for "clear copy history": writes the row-level log and the two counter
// maps back. The frontend is the one holding the snapshot (it read it through
// recent_copies before calling clear_copy_history), so this only ever replays
// what genuinely existed a moment ago, through the same insert clause a live
// copy uses -- never a raw table swap. The counters are written only after the
// rows are committed, so a failed restore changes nothing.
#[tauri::command]
async fn restore_copy_history(
    app: AppHandle,
    state: State<'_, Db>,
    entries: Vec<CopyLogRow>,
    usage: HashMap<String, u32>,
    last_used: HashMap<String, u64>,
) -> Result<(), String> {
    let Some(conn) = db_conn(&app) else {
        return Err("history-failed".into());
    };
    if db_restore_copy_log(&conn, &entries).is_err() {
        append_crash_log("restore_copy_history: the rows could not be reinserted");
        return Err("history-failed".into());
    }
    let saved = {
        let mut store = lock(&state);
        store.settings.usage = usage;
        store.settings.last_used = last_used;
        save_settings(&app, &store.settings)
    };
    rebuild_tray(&app);
    if !saved {
        return Err("save-failed".into());
    }
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

// Remove one journal entry by its exact row: never "all copies of this
// prompt", so it behaves the same in grouped and ungrouped view.
#[tauri::command]
async fn delete_copy_entry(app: AppHandle, rowid: i64) -> bool {
    db_conn(&app)
        .map(|c| c.execute("DELETE FROM copy_log WHERE rowid = ?1", params![rowid]).is_ok())
        .unwrap_or(false)
}

// The boxes float_recall spreads out, in physical pixels: every pill (in a stable
// id order) and then every folder, each sized by its own scale and the screen's
// scale factor. Returns the pill ids in the same order as their boxes.
pub(crate) fn recall_wishes(settings: &Settings, prompts: &[Prompt], work: Rect, dpi: f64) -> (Vec<String>, Vec<Rect>) {
    let mut wishes: Vec<Rect> = Vec::new();
    let mut pill_ids: Vec<String> = settings.floating.keys().cloned().collect();
    pill_ids.sort(); // stable fan-out order across runs
    for id in &pill_ids {
        let scale = float_scale_of(settings, id);
        let (w, h) = match prompts.iter().find(|p| &p.id == id) {
            Some(p) => pill_dims(is_image_prompt(p), scale),
            None => pill_dims(false, scale),
        };
        wishes.push(Rect { x: work.x, y: work.y, w: w * dpi, h: h * dpi });
    }
    for folder in &settings.float_folders {
        let side = folder_box(folder.scale) * dpi;
        wishes.push(Rect { x: work.x, y: work.y, w: side, h: side });
    }
    (pill_ids, wishes)
}

// Expert action "Bring every floating window back to the main screen": the
// emergency exit once a run of monitor changes has left positions and screen
// identities not worth untangling by hand. Every pill and folder, parked or
// not, gets a fresh spread-out spot on the main screen and forgets its old
// screen identity -- there is nothing left to return to after this runs.
#[tauri::command]
async fn float_recall(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    {
        let mut store = lock(&state);
        let screens = screen_snapshot(&app);
        let (main_id, mx, my, mw, mh) = screens
            .first()
            .cloned()
            .unwrap_or((String::new(), 0, 0, 1920, 1080));
        let work = Rect { x: mx as f64, y: my as f64, w: mw as f64, h: mh as f64 };
        // The work area is in physical pixels, the box sizes in logical ones: scale
        // them by the main screen's own factor, as the folder dissolve does.
        let dpi = app
            .get_webview_window("main")
            .and_then(|w| w.available_monitors().ok())
            .and_then(|m| m.first().map(|m| m.scale_factor()))
            .unwrap_or(1.0);
        let (pill_ids, wishes) = recall_wishes(&store.settings, &store.prompts, work, dpi);
        let spots = spread_free_positions(&wishes, &[], work);

        for (id, (x, y)) in pill_ids.iter().zip(spots.iter()) {
            store.settings.floating.insert(
                id.clone(),
                Pos { x: x.round() as i32, y: y.round() as i32, screen: main_id.clone() },
            );
        }
        for (folder, (x, y)) in store.settings.float_folders.iter_mut().zip(spots.iter().skip(pill_ids.len())) {
            folder.pos = Pos { x: x.round() as i32, y: y.round() as i32, screen: main_id.clone() };
        }

        save_settings(&app, &store.settings);
    }
    if let Some(state) = app.try_state::<ParkedFloats>() {
        state.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    sync_float_windows(&app);
    notify_float_settings(&app);
    Ok(())
}

// "Hide for now": a pill or folder id (both flat strings from different id
// spaces, never colliding), dismissed until the app restarts. NOT the permanent
// unpin (toggle_floating/dissolve_float_folder): the entry in settings.floating
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
    // "Pinned" is what `settings.floating` says, never whether a window happens to be
    // open right now: a pill inside a folder has no window of its own, and one that was
    // dismissed with "hide until restart" is still pinned with its window already gone.
    // Deciding from the window would re-open such a pill on a click the menu had just
    // labelled "remove". The folder check is what takes a member out of its folder too.
    let pinned = {
        let store = lock(&state);
        store.settings.floating.contains_key(&id)
            || (folders_on(&store.settings)
                && store.settings.float_folders.iter().any(|f| f.members.contains(&id)))
    };
    if pinned {
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
        // The main window keeps its own settings.floating mirror for the tile
        // context menu's pin/unpin label (see cacheFloatingState in main.js) --
        // notify_float_settings deliberately excludes it, so a pill unpinning
        // ITSELF here would otherwise leave that mirror stale until the next
        // full get_state, showing "Remove" on a tile that has no pill anymore.
        let _ = app.emit_to("main", "floating-pin-changed", serde_json::json!({ "id": id, "pinned": false }));
        Ok(false)
    } else {
        let prompt = {
            let store = lock(&state);
            store.prompts.iter().find(|p| p.id == id).cloned()
        };
        match prompt {
            Some(p) => {
                // HiddenFloats is never cleared anywhere else: re-pinning a pill
                // that was previously "hidden until restart" must take it out, or the
                // next sync_float_windows (triggered by literally any other settings
                // change) immediately closes it again.
                if let Some(hs) = app.try_state::<HiddenFloats>() {
                    hs.0.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                }
                let mut taken: Vec<Rect> = app.webview_windows().values().filter_map(window_rect).collect();
                open_floating(&app, &p, &mut taken);
                let _ = app.emit_to("main", "floating-pin-changed", serde_json::json!({ "id": id, "pinned": true }));
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
pub(crate) fn set_window_pos_phys(win: &tauri::WebviewWindow, x: i32, y: i32, w: i32, h: i32) -> bool {
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
pub(crate) fn set_float_bounds_native(win: &tauri::WebviewWindow, x: f64, y: f64, w: f64, h: f64) -> bool {
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
pub(crate) const ALLOW_SCREENSHOT_KEY: &str = "allowScreenshot";

pub(crate) fn capture_excluded(settings: &Settings) -> bool {
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
pub(crate) fn set_capture_exclusion(win: &tauri::WebviewWindow, excluded: bool) {
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
pub(crate) fn set_capture_exclusion(_win: &tauri::WebviewWindow, _excluded: bool) {}

// The window's current display affinity as the system reports it (0 = none,
// 0x11 = excluded from capture), or None when it cannot be read.
#[cfg(windows)]
pub(crate) fn display_affinity(win: &tauri::WebviewWindow) -> Option<u32> {
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowDisplayAffinity(hwnd: isize, affinity: *mut u32) -> i32;
    }
    let h = win.hwnd().ok()?;
    let mut aff: u32 = 0;
    let ok = unsafe { GetWindowDisplayAffinity(h.0 as isize, &mut aff) };
    if ok != 0 { Some(aff) } else { None }
}

#[cfg(not(windows))]
pub(crate) fn display_affinity(_win: &tauri::WebviewWindow) -> Option<u32> {
    None
}

// The recording bar and the region outline stay out of every capture whatever
// the privacy setting says: a recording must never contain its own controls.
pub(crate) fn always_excluded(label: &str) -> bool {
    label == "rec-bar" || label == "rec-outline"
}

// Whether a window has to be capture-excluded, given the state the privacy
// setting alone asks for. Every loop that applies the affinity to all windows
// goes through this, so the setting can never clear a permanent exclusion.
pub(crate) fn wants_capture_exclusion(label: &str, setting_excluded: bool) -> bool {
    always_excluded(label) || setting_excluded
}

// Pure: does a readback agree with what was asked for? An unreadable affinity
// counts as agreeing, since there is nothing to compare.
pub(crate) fn affinity_matches(want_excluded: bool, readback: Option<u32>) -> bool {
    match readback {
        Some(a) => (a != 0) == want_excluded,
        None => true,
    }
}

// Pure: whether the app's own screenshot, colour picker and recording picker
// hide the app windows before freezing the desktop. With the privacy setting
// allowing screenshots the app stays visible in its own tools.
pub(crate) fn capture_hides_app_windows(setting_excluded: bool) -> bool {
    setting_excluded
}

// Apply the privacy setting to every open window and read the result back.
// A window that still reports the wrong state gets the call once more. Returns
// true when a window that must be capturable still reports exclusion after
// that (some Windows builds keep it), so the caller can tell the user.
pub(crate) fn apply_capture_exclusion_all(app: &AppHandle, setting_excluded: bool) -> bool {
    let mut stuck = false;
    for (label, win) in app.webview_windows() {
        let want = wants_capture_exclusion(&label, setting_excluded);
        set_capture_exclusion(&win, want);
        if !affinity_matches(want, display_affinity(&win)) {
            set_capture_exclusion(&win, want);
            if !affinity_matches(want, display_affinity(&win)) && !want {
                stuck = true;
            }
        }
    }
    if stuck {
        let _ = app.emit_to("main", "capture-restart-notice", ());
    }
    stuck
}

mod rec_bar;
use rec_bar::*;

// ---- Auto-paste: track the last EXTERNAL foreground window so a copied prompt
// can be pasted straight into it. A WinEvent hook records every foreground change;
// WINEVENT_SKIPOWNPROCESS drops our own windows, so the stored handle is always the
// app the user was working in (covers grid tiles, library rows AND the no-activate
// floating pills, whose click never steals foreground).
#[cfg(windows)]
mod autopaste;

// Paste the current clipboard into the last external window. The delay lets the
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

// Tunable knobs for the auto-advance clipboard session (the expert menu exposes these in the
// expert menu; defaults here are what ships until a user overrides them there).
#[cfg(windows)]
#[derive(Clone, Copy)]
pub(crate) struct AutoCfg {
    pub(crate) settle_ms: u32,
    pub(crate) cooldown_ms: u32,
    pub(crate) input_ms: u32,
    pub(crate) rearm_max: u32,
    pub(crate) timeout_min: u32,
}

// Fixed timings of the automatic cycling: how long the clipboard must be quiet
// before a paste counts, the window for typed input, and how often a session
// may re-arm itself.
#[cfg(windows)]
pub(crate) const ROTATE_AUTO_SETTLE_MS: u32 = 350;
#[cfg(windows)]
pub(crate) const ROTATE_AUTO_INPUT_MS: u32 = 3000;
#[cfg(windows)]
pub(crate) const ROTATE_AUTO_REARM_MAX: u32 = 12;

#[cfg(windows)]
pub(crate) fn auto_cfg(settings: &Settings) -> AutoCfg {
    let v = |key: &str, def: f64| settings.ui_values.get(key).copied().unwrap_or(def).max(0.0);
    AutoCfg {
        settle_ms: ROTATE_AUTO_SETTLE_MS,
        cooldown_ms: v("rotateAutoCooldownMs", 250.0) as u32,
        input_ms: ROTATE_AUTO_INPUT_MS,
        rearm_max: ROTATE_AUTO_REARM_MAX,
        timeout_min: v("rotateAutoTimeoutMin", 60.0) as u32,
    }
}

// "Did this WM_RENDERFORMAT answer a real Ctrl+V / right-click-Paste, or just a
// clipboard-history spy / our own read?" Pure and table-tested (cargo test --lib):
// the Win32 plumbing around it is only provable on a real VM run.
#[cfg(windows)]
pub(crate) fn counts_as_paste(now: u32, armed_at: u32, last_paste: u32, last_input: u32, self_read: bool, cfg: &AutoCfg) -> bool {
    if self_read {
        return false;
    }
    if now.wrapping_sub(armed_at) < cfg.settle_ms {
        return false;
    }
    if last_paste != 0 && now.wrapping_sub(last_paste) < cfg.cooldown_ms {
        return false;
    }
    if now.wrapping_sub(last_input) > cfg.input_ms {
        return false;
    }
    true
}

// A DWORD-valued clipboard opt-out format (CanIncludeInClipboardHistory,
// CanUploadToCloudClipboard) means "present and explicitly 0" = excluded, a nonzero
// value means the source allows it, and absence means the source never said either
// way. A value that could not be read at all (format announced but the memory block
// came back empty) is treated the same as an explicit 0 -- the same fail-closed
// choice as the secret heuristic below: a copy skipped by mistake costs nothing, a
// secret kept by mistake costs a lot. Pure so the value-reading rule has its own
// test without touching a real clipboard.
pub(crate) fn dword_format_excludes(available: bool, value: Option<u32>) -> bool {
    available && value.unwrap_or(0) == 0
}

// Nothing else this app's own UI ever copies is a single unspaced 8-64 character
// string mixing three of the four character classes below, so the "not a word from
// our own dictionary" carve-out only has to rule out the product name itself.
pub(crate) fn is_own_ui_word(candidate: &str) -> bool {
    candidate.trim().eq_ignore_ascii_case(brand::DISPLAY_NAME)
}

// Shortest copied text the clipboard collector may take for a password.
pub(crate) const CLIP_SECRET_MIN_LEN: usize = 8;

// A best-effort guess that freshly copied text is a password rather than a prompt, a
// sentence or an address: exactly one line, `min_len` to 64 characters, no
// whitespace, at least three of the four character classes, and not the one word the
// app's own UI could otherwise trip this on. Getting this wrong toward "skip it" only
// costs one entry that never reaches the inbox; getting it wrong the other way puts a
// password into a history file, so every ambiguous case below returns true.
pub(crate) fn looks_like_password(text: &str, min_len: usize) -> bool {
    if text.lines().count() != 1 {
        return false;
    }
    let t = text.trim();
    let len = t.chars().count();
    if len < min_len.max(1) || len > 64 {
        return false;
    }
    if t.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    let has_lower = t.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = t.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let has_symbol = t.chars().any(|c| !c.is_ascii_alphanumeric());
    let classes = [has_lower, has_upper, has_digit, has_symbol].iter().filter(|b| **b).count();
    if classes < 3 {
        return false;
    }
    if is_own_ui_word(t) {
        return false;
    }
    true
}

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

// No-op own-copy note on non-Windows.
#[cfg(windows)]
pub(crate) fn note_own_copy(text: &str) {
    clipwatch::note_own(text);
}
#[cfg(not(windows))]
pub(crate) fn note_own_copy(_text: &str) {}

// List / dismiss / clear the clipboard inbox.
#[tauri::command]
async fn clip_inbox_list(app: AppHandle) -> Result<Vec<ClipItem>, String> {
    Ok(db_conn(&app).map(|c| db_list_clips(&c)).unwrap_or_default())
}
#[tauri::command]
async fn clip_inbox_dismiss(app: AppHandle, id: i64) -> Result<(), String> {
    // Privacy-sensitive rows: a swallowed failure here used to always answer
    // Ok(()) while the entry stayed on disk.
    let ok = db_conn(&app).is_some_and(|c| c.execute("DELETE FROM clip_inbox WHERE id = ?1", params![id]).is_ok());
    if !ok {
        append_crash_log(&format!("clip_inbox_dismiss: row {id} could not be removed"));
        return Err("delete-incomplete".into());
    }
    Ok(())
}
#[tauri::command]
async fn clip_inbox_clear(app: AppHandle) -> Result<(), String> {
    let ok = db_conn(&app).is_some_and(|c| c.execute("DELETE FROM clip_inbox", []).is_ok());
    if !ok {
        append_crash_log("clip_inbox_clear: clip_inbox could not be cleared");
        return Err("delete-incomplete".into());
    }
    Ok(())
}

#[derive(Deserialize)]
pub(crate) struct ClipRestoreItem {
    pub(crate) ts: u64,
    pub(crate) text: String,
}

// Undo for "clear clipboard inbox": re-inserts the rows the frontend snapshotted
// (through the existing clip_inbox_list) before calling clip_inbox_clear, oldest
// first, through db_add_clip -- the same capped insert a live watcher hit uses,
// so a restore right after the cap changed still ends up with a valid inbox
// instead of one longer than the setting now allows.
#[tauri::command]
async fn restore_clip_inbox(app: AppHandle, state: State<'_, Db>, items: Vec<ClipRestoreItem>) -> Result<(), String> {
    let cap = lock(&state).settings.ui_values.get("clipInboxMax").copied().unwrap_or(50.0).clamp(10.0, 200.0) as usize;
    let Some(conn) = db_conn(&app) else {
        return Err("history-failed".into());
    };
    if db_restore_clips(&conn, &items, cap).is_err() {
        append_crash_log("restore_clip_inbox: the items could not be reinserted");
        return Err("history-failed".into());
    }
    Ok(())
}

// The insert loop behind restore_clip_inbox in one transaction: an item that
// fails leaves the inbox as it was instead of a partial restore.
pub(crate) fn db_restore_clips(conn: &Connection, items: &[ClipRestoreItem], cap: usize) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for item in items {
        db_add_clip(&tx, item.ts, &item.text, cap)?;
    }
    tx.commit()
}

// Unfinished screenshot/video editor work, remembered so closing the window by
// accident does not lose it. One row per (kind, target) pair -- kind is "snip"
// or "video", target the absolute source path, so `id` is simply their join and
// an upsert never needs a separate existence check. `ops` is whatever JSON the
// editor itself produced; this side never parses it, so a version embedded IN
// that JSON (the caller's job, not this table's) is free to change without a
// migration here. size/mtime are the file's own stat at save time -- load only
// ever matches on the exact triple, so a changed source file simply misses
// rather than restoring onto content it was never drawn against.
// The (size, mtime) triple save_edit_state/load_edit_state key on -- a plain
// stat, not opened or read, so this stays cheap enough to call on every open.
#[tauri::command]
async fn stat_file(path: String) -> Result<(i64, i64), String> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok((meta.len() as i64, mtime))
}

// Rows are keyed by a hash of the file path and carry the path and the ops
// sealed, so the table no longer lists recording paths or text overlay
// contents in clear. A row an earlier version wrote (plain "kind:path" id,
// plain columns) is still found through its legacy id and read as is.
pub(crate) fn edit_state_id(kind: &str, target: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{kind}:{:x}", Sha256::digest(target.as_bytes()))
}

pub(crate) fn edit_state_legacy_id(kind: &str, target: &str) -> String {
    format!("{kind}:{target}")
}

pub(crate) fn edit_state_write(conn: &Connection, kind: &str, target: &str, size: i64, mtime: i64, ops: &str, updated_at: i64) -> Result<(), String> {
    conn.execute(
        "INSERT INTO edit_state(id, kind, target, size, mtime, ops, updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(id) DO UPDATE SET target=excluded.target, size=excluded.size, mtime=excluded.mtime, ops=excluded.ops, updated_at=excluded.updated_at",
        params![edit_state_id(kind, target), kind, enc_str(target), size, mtime, enc_str(ops), updated_at],
    )
    .map_err(|e| e.to_string())?;
    // The state now lives under the hashed id; drop an old plain-path row for it.
    let _ = conn.execute("DELETE FROM edit_state WHERE id = ?1", params![edit_state_legacy_id(kind, target)]);
    Ok(())
}

// Rewrites every row whose target or ops are still in clear through the sealing
// writer, keeping each row's own timestamp. Returns how many rows it rewrote; a
// second run finds nothing left to do.
pub(crate) fn edit_state_seal_all(conn: &Connection) -> usize {
    type Row = (String, String, i64, i64, String, String, i64);
    let rows: Vec<Row> = {
        let Ok(mut st) = conn.prepare("SELECT kind, target, size, mtime, ops, id, updated_at FROM edit_state WHERE target NOT LIKE 'enc:v1:%' OR ops NOT LIKE 'enc:v1:%'") else {
            return 0;
        };
        let Ok(it) = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))) else {
            return 0;
        };
        it.flatten().collect()
    };
    let mut done = 0;
    for (kind, target, size, mtime, ops, old_id, updated_at) in rows {
        let plain_target = dec_str(&target);
        let plain_ops = dec_str(&ops);
        if edit_state_write(conn, &kind, &plain_target, size, mtime, &plain_ops, updated_at).is_ok() {
            // A row that sat under some other id (legacy form) is dropped once the hashed one exists.
            if old_id != edit_state_id(&kind, &plain_target) {
                let _ = conn.execute("DELETE FROM edit_state WHERE id = ?1", params![old_id]);
            }
            done += 1;
        }
    }
    done
}

// (row id, ops as stored, decrypted) for the exact (size, mtime) triple, hashed id first.
pub(crate) fn edit_state_read(conn: &Connection, kind: &str, target: &str, size: i64, mtime: i64) -> Option<(String, String)> {
    for id in [edit_state_id(kind, target), edit_state_legacy_id(kind, target)] {
        let row = conn
            .query_row(
                "SELECT ops, target, updated_at FROM edit_state WHERE id = ?1 AND size = ?2 AND mtime = ?3",
                params![id, size, mtime],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
            )
            .ok();
        if let Some((ops, stored_target, updated_at)) = row {
            let plain_ops = dec_str(&ops);
            // A row an earlier version wrote in clear is sealed the first time it is
            // read: rewritten under the hashed id (the legacy row is dropped), keeping
            // its own timestamp so the retention clock does not restart.
            if !ops.starts_with(ENC_PREFIX) || !stored_target.starts_with(ENC_PREFIX) {
                let _ = edit_state_write(conn, kind, target, size, mtime, &plain_ops, updated_at);
                let hashed = edit_state_id(kind, target);
                return Some((hashed, plain_ops));
            }
            return Some((id, plain_ops));
        }
    }
    None
}

pub(crate) fn edit_state_discard(conn: &Connection, kind: &str, target: &str) -> Result<(), String> {
    for id in [edit_state_id(kind, target), edit_state_legacy_id(kind, target)] {
        conn.execute("DELETE FROM edit_state WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn save_edit_state(app: AppHandle, kind: String, target: String, size: i64, mtime: i64, ops: String) -> Result<(), String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    let updated_at = (now_ms() / 1000) as i64;
    edit_state_write(&conn, &kind, &target, size, mtime, &ops, updated_at)
}
#[tauri::command]
async fn load_edit_state(app: AppHandle, kind: String, target: String, size: i64, mtime: i64) -> Result<Option<String>, String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    let Some((id, ops)) = edit_state_read(&conn, &kind, &target, size, mtime) else { return Ok(None) };
    // Malformed JSON, or a "v" this build does not recognise, must never reach the
    // editor -- it would try to replay operations it cannot make sense of. Decided
    // here rather than in JS, so there is exactly one place "usable" is defined;
    // discarded silently to the caller (an empty editor is the safe outcome) but
    // logged, so a real corruption pattern is still visible somewhere.
    let is_known_version = edit_state_ops_version(&ops) == Some(1);
    if !is_known_version {
        // The file name only: the full path would put the recording location back into
        // the log that sealing the row keeps out of the database.
        let shown = std::path::Path::new(&target).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        append_crash_log(&format!("load_edit_state: discarding unreadable saved state for {shown}"));
        let _ = conn.execute("DELETE FROM edit_state WHERE id = ?1", params![id]);
        return Ok(None);
    }
    Ok(Some(ops))
}
// A minimal sniff, not a full parse of every op inside -- this table's whole point
// is staying opaque to the individual op shapes JS invents, it only ever needs to
// know "can any of this be understood at all".
pub(crate) fn edit_state_ops_version(ops: &str) -> Option<i64> {
    serde_json::from_str::<serde_json::Value>(ops)
        .ok()?
        .get("v")?
        .as_i64()
}
#[tauri::command]
async fn discard_edit_state(app: AppHandle, kind: String, target: String) -> Result<(), String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    edit_state_discard(&conn, &kind, &target)
}

// ---- Editor draft autosave ----
// A single row (id = "current"), because only one prompt editor modal is ever
// open at a time -- no need for a table keyed per entry. prompt_id is the target
// entry's id for an edit, or "neu" for a still-unsaved new entry, matching the
// modal's own "create" mode having no real id yet. Only the text side is
// drafted (name/text/variants); an attached image is never re-derived from a
// crash draft, it stays with the prompt itself.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditorDraft {
    pub(crate) prompt_id: String,
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) variants: String,
    pub(crate) updated_at: i64,
}

// The text side is sealed like a prompt is: a draft holds whatever the user
// was typing, and a plain row would leak it into data.db and every backup.
// Old unsealed rows still read back (dec_str passes plaintext through).
pub(crate) fn editor_draft_write(conn: &Connection, prompt_id: &str, name: &str, text: &str, variants: &str, updated_at: i64) -> Result<(), String> {
    conn.execute(
        "INSERT INTO editor_drafts(id, prompt_id, name, text, variants, updated_at) VALUES('current',?1,?2,?3,?4,?5)
         ON CONFLICT(id) DO UPDATE SET prompt_id=excluded.prompt_id, name=excluded.name, text=excluded.text, variants=excluded.variants, updated_at=excluded.updated_at",
        params![prompt_id, enc_str(name), enc_str(text), enc_str(variants), updated_at],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub(crate) fn editor_draft_read(conn: &Connection) -> Option<EditorDraft> {
    let (prompt_id, raw_name, raw_text, raw_variants, updated_at) = conn
        .query_row(
            "SELECT prompt_id, name, text, variants, updated_at FROM editor_drafts WHERE id = 'current'",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?)),
        )
        .ok()?;
    let (name, text, variants) = (dec_str(&raw_name), dec_str(&raw_text), dec_str(&raw_variants));
    // A sealed draft that no longer decrypts (data.db restored on another account or
    // machine) reads back empty: that is no draft, never an empty one to "restore".
    let sealed = [&raw_name, &raw_text, &raw_variants].iter().any(|v| v.starts_with(ENC_PREFIX));
    if sealed && name.is_empty() && text.is_empty() && variants.is_empty() {
        let _ = conn.execute("DELETE FROM editor_drafts WHERE id = 'current'", []);
        return None;
    }
    // An old draft written in clear is sealed on this first read.
    if ![&raw_name, &raw_text, &raw_variants].iter().all(|v| v.is_empty() || v.starts_with(ENC_PREFIX)) {
        let _ = editor_draft_write(conn, &prompt_id, &name, &text, &variants, updated_at);
    }
    Some(EditorDraft { prompt_id, name, text, variants, updated_at })
}

#[tauri::command]
async fn save_editor_draft(app: AppHandle, prompt_id: String, name: String, text: String, variants: String) -> Result<(), String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    let updated_at = (now_ms() / 1000) as i64;
    editor_draft_write(&conn, &prompt_id, &name, &text, &variants, updated_at)
}

#[tauri::command]
async fn load_editor_draft(app: AppHandle) -> Result<Option<EditorDraft>, String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    Ok(editor_draft_read(&conn))
}

#[tauri::command]
async fn discard_editor_draft(app: AppHandle) -> Result<(), String> {
    let conn = db_conn(&app).ok_or_else(|| "db unavailable".to_string())?;
    conn.execute("DELETE FROM editor_drafts WHERE id = 'current'", []).map_err(|e| e.to_string())?;
    Ok(())
}

// Shared by delete_prompt and batch_prompts's "delete" branch: a draft outlives
// the modal that wrote it, so it must not outlive the entry it belongs to
// either. `matches` decides on the draft's own prompt_id, so a single-delete and
// a batch-delete can both reuse this without building a one-off HashSet lookup.
pub(crate) fn discard_editor_draft_if(conn: &Connection, matches: impl Fn(&str) -> bool) {
    let target: Option<String> = conn
        .query_row("SELECT prompt_id FROM editor_drafts WHERE id = 'current'", [], |r| r.get(0))
        .ok();
    if let Some(target) = target {
        if matches(&target) {
            let _ = conn.execute("DELETE FROM editor_drafts WHERE id = 'current'", []);
        }
    }
}

// Periodic cleanup for edit_state, run through the existing job scheduler rather
// than a hand-rolled thread (same reasoning as recording-sweep above): a row
// disappears once its own source file is gone -- deleted from the library, swept
// out of the recordings folder, or moved to the "verworfen" folder and then aged
// out of it -- without this needing its own hook into any of those three call
// sites. keep_days = 0 means "never expire by age" (same convention as
// trashRetentionDays), a missing file is removed on the next tick regardless.
// `exists` is a parameter so a test can run this against paths that were never
// meant to be real files on the runner's own disk.
pub(crate) fn sweep_edit_state_conn(conn: &Connection, exists: impl Fn(&str) -> bool, keep_days: f64) -> usize {
    let cutoff = (now_ms() / 1000) as i64 - (keep_days.max(0.0) * 86400.0) as i64;
    let mut stmt = match conn.prepare("SELECT id, target, updated_at FROM edit_state") {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let rows: Vec<(String, String, i64)> = match stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))) {
        Ok(it) => it.filter_map(|r| r.ok()).collect(),
        Err(_) => return 0,
    };
    drop(stmt);
    let mut removed = 0usize;
    for (id, target, updated_at) in rows {
        let target = dec_str(&target);
        let gone = !exists(&target);
        let expired = keep_days > 0.0 && updated_at < cutoff;
        if (gone || expired) && conn.execute("DELETE FROM edit_state WHERE id = ?1", params![id]).is_ok() {
            removed += 1;
        }
    }
    removed
}

// Re-read every round, same as recording_sweep_interval, so a changed value takes
// effect without a restart.
pub(crate) fn edit_state_keep_days(app: &AppHandle) -> f64 {
    app.try_state::<Db>()
        .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_values.get("editStateKeepDays").copied().unwrap_or(30.0))
        .unwrap_or(30.0)
        .max(0.0)
}

pub(crate) fn sweep_edit_state_tick(app: &AppHandle) {
    let Some(conn) = db_conn(app) else { return };
    sweep_edit_state_conn(&conn, |p| std::path::Path::new(p).exists(), edit_state_keep_days(app));
}

// The edit-states row's on-demand trigger -- runs the same rule the
// periodic tick applies, so a click always removes exactly what the next
// silent tick would have. No byte figure: sweep_edit_state_conn only ever
// reports a row count, and this reuses it unchanged rather than risking its
// own tested return shape for a "nice to have" number.
#[tauri::command]
fn sweep_edit_states_now(app: AppHandle) -> u32 {
    let Some(conn) = db_conn(&app) else { return 0 };
    sweep_edit_state_conn(&conn, |p| std::path::Path::new(p).exists(), edit_state_keep_days(&app)) as u32
}

// Emergency purge: something sensitive (a password) ended up in the clipboard inbox
// and may be sitting inside one or more ROTATING SNAPSHOTS too, DPAPI-sealed but
// still on disk. clip_inbox_clear only ever touched the live DB: this walks every
// backup folder, opens its snapshot, deletes clip_inbox rows if any, and VACUUMs so
// the bytes are actually overwritten (a bare DELETE leaves them in SQLite's free
// pages until something else reuses that space). Returns how many snapshots had
// something to remove, for the confirmation toast.
#[tauri::command]
// Returns (purged, failed). This is an emergency privacy wipe: a snapshot this
// could not even open, or a scrub that could not be confirmed written back, must
// never be folded into "nothing to purge here"; the caller has to be able to
// tell the user which backups may still hold what they just tried to erase.
async fn purge_clip_inbox_from_backups(app: AppHandle) -> Result<(usize, usize), String> {
    Ok(purge_clip_inbox_from_backups_impl(&backups_dir(&app)))
}

// Empties clip_inbox in one snapshot and repacks it so the freed pages are
// overwritten. Some(rows removed) only when both steps worked: a delete whose
// repack failed leaves the old bytes in the free pages, so it is no scrub.
pub(crate) fn scrub_clip_inbox(conn: &Connection) -> Option<i64> {
    let removed = conn.execute("DELETE FROM clip_inbox", []).ok()? as i64;
    if removed > 0 && conn.execute("VACUUM", []).is_err() {
        return None;
    }
    Some(removed)
}

pub(crate) fn purge_clip_inbox_from_backups_impl(dir: &std::path::Path) -> (usize, usize) {
    let mut purged = 0usize;
    let mut failed = 0usize;
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let folder = entry.path();
        if !folder.is_dir() {
            continue;
        }
        let Some(db) = backup_db_file(&folder) else { continue };
        let is_gz = db.extension().is_some_and(|e| e == "gz");
        let staged_tmp = folder.join("purge.tmp");
        let staged = if is_gz {
            match gunzip_to(&db, &staged_tmp) {
                Ok(()) => staged_tmp.clone(),
                // Could not even look inside -- must count as a failure, not as
                // "nothing to purge", since it might hold exactly what this is
                // meant to scrub.
                Err(_) => {
                    failed += 1;
                    continue;
                }
            }
        } else {
            db.clone()
        };
        let mut removed = 0i64;
        let mut inspected = false;
        if let Ok(conn) = Connection::open(&staged) {
            let has_table: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='clip_inbox'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if has_table > 0 {
                // None here means "could not confirm the rows are actually gone"
                // (the delete or the repack failed) -- inspected stays false,
                // counted as a failure below.
                if let Some(n) = scrub_clip_inbox(&conn) {
                    inspected = true;
                    removed = n;
                }
            } else {
                inspected = true; // genuinely nothing to remove, not a failure
            }
        }
        if !inspected {
            failed += 1;
        } else if removed > 0 {
            let repacked = if is_gz { gzip_to(&staged, &db) } else { Ok(()) };
            if repacked.is_ok() {
                purged += 1;
            } else {
                // Scrubbed in the decompressed copy, but it never made it back
                // into the actual backup file -- the original rows are untouched.
                failed += 1;
            }
        }
        if is_gz {
            let _ = fs::remove_file(&staged_tmp);
        }
    }
    (purged, failed)
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
            // out the exact box it wants: a live edge-resize computes the origin from
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

// ---------- folder commands ----------

// Repositions a folder pill the same way a mouse drag does -- used by the
// folder window's own arrow-key move (folder.js), which has no native OS drag
// gesture to hand this off to the way startDragging() does for a mouse. The
// existing Moved handler on the folder window (see build_folder_window) is
// what actually persists the new position, exactly as it does for a drag;
// this only has to move the window and let that handler fire.
#[tauri::command]
async fn set_folder_position(app: AppHandle, id: String, x: f64, y: f64, clamp: Option<bool>) -> Result<(), String> {
    if [x, y].iter().all(|v: &f64| v.is_finite()) {
        if let Some(win) = app.get_webview_window(&folder_label(&id)) {
            let (x, y) = if clamp.unwrap_or(false) {
                let dpi = win.scale_factor().unwrap_or(1.0);
                let size = window_rect(&win).map(|r| (r.w, r.h)).unwrap_or((0.0, 0.0));
                let work = work_rect_at(&app, x * dpi, y * dpi);
                let (cx, cy) = clamp_to_work(work, size.0, size.1, x * dpi, y * dpi);
                (cx / dpi, cy / dpi)
            } else {
                (x, y)
            };
            let _ = win.set_position(tauri::LogicalPosition::new(x, y));
        }
    }
    Ok(())
}

// What the dragged pill covers: the id it would merge into, every other pill/folder
// box (for the live highlight), and the dragged pill's own rect.
pub(crate) type DragHit = (String, Vec<(String, Rect)>, Rect);

// What the dragged pill currently covers: every other pill/folder box plus the one
// `merge_target` picks. ONE hit function for both the live highlight and the drop, so
// what lights up while dragging is exactly what letting go would merge.
pub(crate) fn drag_hit(app: &AppHandle, id: &str, radius: f64) -> Option<DragHit> {
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
pub(crate) fn hit_label(hit: &str) -> String {
    let (kind, key) = hit.split_at(1);
    if kind == "f" {
        folder_label(key)
    } else {
        flabel(key)
    }
}

// Live merge feedback while a pill is dragged. The OS move loop owns the pointer, so
// no webview sees the motion: the backend polls the dragged window instead and lights
// up the window that would swallow it. (generation, currently lit window label)
pub(crate) struct DragHint(pub(crate) Mutex<(u64, Option<String>)>);

pub(crate) const HINT_TICK_MS: u64 = 70;
pub(crate) const HINT_MAX_TICKS: u32 = 700; // ~50 s guard: no drag runs longer

// Move the highlight, telling the old target to drop it. Emits only on a change.
pub(crate) fn light_hint(app: &AppHandle, label: Option<String>) {
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
    let handle = app;
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
// check has to run HERE: the pills are separate OS windows, so only the backend knows
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
        // The new folder takes the target pill's place: centred on it, so it lands
        // where the user aimed: and its icon size mirrors that pill's size.
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
            pos: Pos { x: x.round() as i32, y: y.round() as i32, ..Default::default() },
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
                        folder.members.push(id);
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
pub(crate) struct FolderView {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) scale: f64,
    pub(crate) cols: u32,
    pub(crate) members: Vec<Prompt>,
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
pub(crate) struct FolderGeom {
    pub(crate) side: String,
    pub(crate) icon_x: f64,
    pub(crate) icon_y: f64,
    pub(crate) pop_x: f64,
    pub(crate) pop_y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

// Open the folder by growing its OWN window around the icon: no second window is
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
        folder.pos = Pos { x: icon.x.round() as i32, y: icon.y.round() as i32, ..Default::default() };
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
                f.pos.clone()
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
            Some(folder) => folder.name.clone_from(&name),
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
// MAIN window (confirmDialog) and applied from there: never a native message box.
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
// Each one asks for the position it kept all along, but they are fanned apart first:
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
                let pos = store.settings.floating.get(mid).cloned().unwrap_or_else(|| folder.pos.clone());
                Some((
                    mid.clone(),
                    Rect { x: pos.x as f64, y: pos.y as f64, w: w * dpi, h: h * dpi },
                ))
            })
            .collect();
        (members, folder.pos)
    };
    // Pills and folders already on screen are obstacles: this folder's own window is
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
                .insert(mid.clone(), Pos { x: x.round() as i32, y: y.round() as i32, ..Default::default() });
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
            .insert(id, Pos { x: nx.round() as i32, y: ny.round() as i32, ..Default::default() });
        let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
        normalize_float_folders(&mut store.settings.float_folders, &ids);
        save_settings(&app, &store.settings);
    }
    sync_float_windows(&app);
    Ok(())
}

// Persist the per-prompt video player state (volume, mute, loop).
// Async: can fire while scrubbing; keep the settings write off the UI thread.
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

mod snip;
use snip::*;

mod demo;
use demo::*;

mod capture;
use capture::*;

// Open the project's GitHub page in the default browser (app logo → repo,
// version label → releases). The URLs are hardcoded: the webview can never open
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

// The third-party license list is generated at build time (build.rs) from the real
// dependency manifests and embedded here as a plain JSON string; the frontend parses
// it. Regenerating THIRD-PARTY-NOTICES.md from the same source happens in build.rs
// too, so the two never show a different list.
pub(crate) static THIRD_PARTY_LICENSES_JSON: &str =
    include_str!(concat!(env!("OUT_DIR"), "/third_party_licenses.json"));

#[tauri::command]
fn third_party_notices() -> &'static str {
    THIRD_PARTY_LICENSES_JSON
}

// The window's minimum logical size at a given webview zoom factor: growing the
// 680x460 floor by the same factor keeps at least 680x460 CSS pixels visible
// (zoom scales CSS pixels inside a fixed window frame), capped to the monitor
// so this never asks for a window bigger than the screen.
pub(crate) fn zoom_min_size(factor: f64, cap: (u32, u32)) -> (u32, u32) {
    let min_w = ((MAIN_WIN_MIN_W as f64 * factor).round() as u32).min(cap.0);
    let min_h = ((MAIN_WIN_MIN_H as f64 * factor).round() as u32).min(cap.1);
    (min_w, min_h)
}

// Scale the whole main-window UI with the browser's native zoom (like Ctrl +/-).
// Native zoom keeps event coordinates and getBoundingClientRect in one coordinate
// space, so (unlike a CSS `zoom` on <body>) it never desyncs popup positioning
// or grid hit-testing. Clamped to the slider's 75: 300% range (the slider,
// ui/main.js EXPERT_TABS uiScale, has always had a 75% floor).
#[tauri::command]
fn set_ui_zoom(app: AppHandle, factor: f64) -> Result<(), String> {
    let f = factor.clamp(0.75, 3.0);
    if let Some(win) = app.get_webview_window("main") {
        win.set_zoom(f).map_err(|e| e.to_string())?;
        // See zoom_min_size: without this, the fixed 680x460 OS minimum leaves
        // fewer and fewer CSS pixels visible as zoom climbs (250% => 272x184,
        // header/composer become unreachable). Grow the window's minimum by the
        // same factor: and the window itself, if it is currently smaller than
        // that: so the header/composer stay reachable at any zoom level.
        // The window may sit on any monitor by now (this runs live, not just at
        // startup): cap against the monitor it is actually on, not always the
        // primary one, falling back to primary if that can't be read either.
        let cap = monitor_cap(win.current_monitor().ok().flatten().or_else(|| win.primary_monitor().ok().flatten()));
        let (min_w, min_h) = zoom_min_size(f, cap);
        let _ = win.set_min_size(Some(tauri::LogicalSize::new(min_w, min_h)));
        if let Ok(size) = win.outer_size() {
            let scale = win.scale_factor().unwrap_or(1.0);
            // Round like zoom_min_size does (not truncate): otherwise a window
            // whose true logical size sits right at the new minimum can read 1px
            // under it here, and the set_size call below would then shave that
            // truncated value off the OTHER (already-fine) dimension too.
            let cur_w = (size.width as f64 / scale).round() as u32;
            let cur_h = (size.height as f64 / scale).round() as u32;
            if cur_w < min_w || cur_h < min_h {
                let _ = win.set_size(tauri::LogicalSize::new(cur_w.max(min_w), cur_h.max(min_h)));
            }
        }
    }
    Ok(())
}

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

// ---------- Window geometry ----------

pub(crate) fn point_on_monitor(monitors: &[tauri::Monitor], x: i32, y: i32) -> bool {
    monitors.iter().any(|m| {
        let p = m.position();
        let s = m.size();
        x >= p.x && x < p.x + s.width as i32 && y >= p.y && y < p.y + s.height as i32
    })
}

// NOTE: WindowGeom.width/height are LOGICAL pixels (DPI-independent); a
// physically stored size grew by the monitor's scale factor on every start.
// Position stays physical (global desktop coordinates).

// Keep in sync with tauri.conf.json's main-window minWidth/minHeight. The OS/WebView2
// minSize constraint only guards interactive resize-dragging, not a programmatic
// set_size() call: resolve_geometry's first-start branch has to floor itself.
pub(crate) const MAIN_WIN_MIN_W: u32 = 680;
pub(crate) const MAIN_WIN_MIN_H: u32 = 460;

// Center a window of the given LOGICAL size on the primary monitor.
pub(crate) fn centered_on_primary(main: &tauri::WebviewWindow, width: u32, height: u32) -> WindowGeom {
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

// A monitor's usable size in logical (DPI-independent) pixels, or effectively
// "no cap" if none is given (lookup failed): the multiplication that feeds
// zoom_min_size is already bounded by the 75-300% zoom clamp, so an unread
// monitor never produces a runaway size, just an un-clamped (bounded) one.
pub(crate) fn monitor_cap(monitor: Option<tauri::window::Monitor>) -> (u32, u32) {
    monitor
        .map(|m| {
            let s = m.size();
            (
                (s.width as f64 / m.scale_factor()) as u32,
                (s.height as f64 / m.scale_factor()) as u32,
            )
        })
        .unwrap_or((u32::MAX, u32::MAX))
}

// First start: 50% of the primary monitor, centered. Afterwards the saved size
// is kept (capped at the monitor); if its monitor is gone, re-center.
pub(crate) fn resolve_geometry(main: &tauri::WebviewWindow, saved: Option<WindowGeom>) -> WindowGeom {
    // Hard cap: the window can never start larger than the primary monitor.
    let cap = monitor_cap(main.primary_monitor().ok().flatten());
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
pub(crate) fn update_geom<F: FnOnce(&mut WindowGeom)>(handle: &AppHandle, f: F) {
    if let Some(state) = handle.try_state::<Db>() {
        // try_lock, never block: this runs on the event-loop thread for every move/
        // resize tick. If the store is busy (e.g. a backup VACUUM holds the lock), skip
        // this update: the next move/resize event carries the current geometry anyway.
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
            get_prompt,
            add_prompt,
            seed_example_prompts,
            dismiss_example_prompts,
            example_prompts_pending,
            update_prompt,
            video_editor::backfill_media_meta,
            set_favorite,
            delete_prompt,
            batch_prompts,
            list_trash,
            restore_prompt,
            placement_snapshot,
            restore_prompts_batch,
            purge_prompt,
            empty_trash,
            logging::log_error,
            logging::list_error_log,
            logging::clear_error_log,
            list_versions,
            last_edited_map,
            restore_version,
            restore_version_as_new,
            usage_stats,
            export_stats,
            storage_breakdown,
            open_storage_folder,
            retention_overview,
            sweep_trash_now,
            trim_copy_history_now,
            snip::sweep_clip_export_temp_now,
            sweep_edit_states_now,
            sweep_discarded_recordings_now,
            record_tool_usage,
            find_duplicates,
            similar_to_saved,
            ignore_dups,
            backups::list_backups,
            backups::open_backups_folder,
            backups::create_backup,
            backups::delete_backup,
            backups::restore_backup,
            backups::backup_contents,
            clip_inbox_list,
            clip_inbox_dismiss,
            clip_inbox_clear,
            restore_clip_inbox,
            purge_clip_inbox_from_backups,
            stat_file,
            save_edit_state,
            load_edit_state,
            discard_edit_state,
            save_editor_draft,
            load_editor_draft,
            discard_editor_draft,
            open_repo,
            third_party_notices,
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
            set_tag_colors,
            add_view,
            rename_view,
            set_view_color,
            reorder_views,
            remap_colors,
            delete_view,
            set_active_view,
            snip::copy_image_data,
            save_tile_folder,
            take_out_of_tile_folder,
            delete_tile_folder,
            settings_cmds::get_settings,
            settings_cmds::get_state,
            settings_cmds::prompt_image,
            vault::secret_vault_status,
            vault::secret_vault_unlock,
            vault::secret_vault_lock,
            vault::set_prompt_secret,
            settings_cmds::launcher_items,
            current_theme,
            theme_vars,
            ink_prefs,
            set_theme,
            save_custom_theme,
            delete_custom_theme,
            export_custom_theme,
            import_custom_theme,
            launcher_select,
            launcher_close,
            launcher_ready,
            tray_window_plan,
            tray_window_activate,
            tray_window_close,
            tray_open_launcher,
            tray_launcher_prefill,
            copy_prompt,
            rotation_peek,
            rotation_commit,
            rotation_reset,
            rotation_arm_session,
            rotation_end_auto,
            copy_text,
            copy_formatted,
            clipboard_text,
            record_copy,
            clear_copy_history,
            restore_copy_history,
            recent_copies,
            delete_copy_entry,
            toggle_floating,
            hide_float_for_now,
            float_recall,
            is_float_parked,
            set_float_scale,
            resize_float_pill,
            resize_float_media,
            set_float_bounds,
            set_folder_position,
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
            set_hotkey,
            check_hotkey,
            set_hotkey_capture,
            focus_main_window,
            open_var_window,
            var_window_payload,
            var_window_result,
            var_window_fit,
            os_user_name,
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
            takeover_offer,
            takeover_apply,
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
            paste_into_previous,
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
            set_voice_hotkey,
            set_rec_stop_hotkey,
            set_rec_pause_hotkey,
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
            take_recording_sweep_notice,
            sweep_recordings_now,
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
