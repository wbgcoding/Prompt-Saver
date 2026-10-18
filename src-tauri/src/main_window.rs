use super::*;

// Bring the main window back from the tray at the saved size and position, validated by
// resolve_geometry() so a monitor unplugged meanwhile cannot restore it off every screen.
// A window that no longer exists is rebuilt first.
pub(crate) fn show_main(app: &AppHandle) {
    park_idle_snip_overlay(app);
    let win = show_main_seam(app.get_webview_window("main"), || build_main_window(app));
    let Some(win) = win else { return };
    // This runs on the event-loop thread (tray click, global shortcut): while a backup
    // or import holds the store, waiting here would freeze every window. The window
    // then simply comes back where it is.
    let geom = app
        .try_state::<Db>()
        .and_then(|state| try_lock_ignoring_poison(&state).map(|store| store.settings.window));
    if let Some(geom) = geom {
        let g = resolve_geometry(&win, geom);
        // A window that is still maximized (hidden to the tray) comes back as it is.
        if !(g.maximized && win.is_maximized().unwrap_or(false)) {
            let _ = win.set_size(tauri::LogicalSize::new(g.width, g.height));
            let _ = win.set_position(PhysicalPosition::new(g.x, g.y));
        }
    }
    let _ = win.show();
    // After the show: a maximize on the hidden window is lost when it appears.
    if geom.flatten().is_some_and(|g| g.maximized) && !win.is_maximized().unwrap_or(false) {
        let _ = win.maximize();
    }
    let _ = win.unminimize();
    let _ = win.set_focus();
    reapply_taskbar_state(app);
}

// The decision show_main runs on, split out so a test can pass a counter instead of a real
// window and prove a missing window triggers a build.
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

// The one place the main window is built and wired up: setup() uses it for the window Tauri
// created from tauri.conf.json, show_main() to replace a destroyed one. Same label, geometry,
// decorations and event handlers either way.
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
    // show_main reaches this on the event-loop thread: a busy store (backup, import)
    // must not freeze every window, so the window is dressed from defaults and the
    // page applies the real settings once it loads.
    let settings = app
        .try_state::<Db>()
        .and_then(|s| try_lock_ignoring_poison(&s).map(|store| store.settings.clone()))
        .unwrap_or_default();
    set_capture_exclusion(&win, capture_excluded(&settings));
    if settings.always_on_top {
        let _ = win.set_always_on_top(true);
    }
    // The frameless, transparent window would show through until the frontend paints, so it is
    // dressed with the theme up front.
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
        // Only a visible window's geometry is the user's: the size and position set on the hidden
        // window at start would otherwise clear the remembered maximized state before it is applied.
        WindowEvent::Moved(p) => {
            if p.x > -30000 && p.y > -30000 && main_is_visible(&handle2) {
                let maximized = main_is_maximized(&handle2);
                update_geom(&handle2, |g| geom_after_move(g, p.x, p.y, maximized));
            }
        }
        WindowEvent::Resized(s) => {
            if s.width > 0 && s.height > 0 && main_is_visible(&handle2) {
                // Store LOGICAL pixels: physical values re-scaled
                // by the monitor factor on every start.
                let scale = handle2
                    .get_webview_window("main")
                    .and_then(|w| w.scale_factor().ok())
                    .unwrap_or(1.0);
                let logical = s.to_logical::<f64>(scale);
                let maximized = main_is_maximized(&handle2);
                update_geom(&handle2, |g| {
                    geom_after_resize(g, logical.width.round() as u32, logical.height.round() as u32, maximized)
                });
            }
        }
        WindowEvent::ThemeChanged(_) => {
            // Only the "system" preference follows the OS; a custom theme always
            // inherits a fixed built-in base. The event loop never waits on the store:
            // when a backup or import holds it, a worker thread takes the turn.
            let apply = |app: &AppHandle, settings: &Settings| {
                if settings.theme == "system" {
                    push_theme(app, settings);
                }
            };
            if let Some(state) = handle2.try_state::<Db>() {
                match state.try_lock() {
                    Ok(guard) => {
                        let settings = guard.settings.clone();
                        drop(guard);
                        apply(&handle2, &settings);
                    }
                    Err(std::sync::TryLockError::Poisoned(e)) => {
                        let settings = e.into_inner().settings.clone();
                        apply(&handle2, &settings);
                    }
                    Err(std::sync::TryLockError::WouldBlock) => {
                        let app3 = handle2.clone();
                        std::thread::spawn(move || {
                            if let Some(state) = app3.try_state::<Db>() {
                                let settings = state.lock().unwrap_or_else(|e| e.into_inner()).settings.clone();
                                apply(&app3, &settings);
                            }
                        });
                    }
                }
            }
        }
        WindowEvent::CloseRequested { api, .. } => {
            // Portable always closes fully; installed honours the tray setting.
            let minimize = close_minimizes(is_portable());
            if minimize {
                api.prevent_close();
                // Hide instead of quitting; a hidden window never keeps listening.
                voice_shutdown(&handle2);
                if let Some(w) = handle2.get_webview_window("main") {
                    let _ = w.hide();
                }
            } else {
                // Never vanish while an export, conversion, download, backup or import is running:
                // request_app_quit asks the main window once per quit attempt (QUIT_CONFIRMED lets the
                // answered quit through).
                api.prevent_close();
                request_app_quit(&handle2);
            }
        }
        WindowEvent::Destroyed => {
            // No process without a main window: exit right away, with a watchdog in case exit() never
            // completes (a stuck shutdown handler). The watchdog starts before anything that can wait
            // on the store lock, so a backup or import holding it cannot delay it.
            let deadline = SystemTime::now() + WATCHDOG_GRACE;
            let hard_deadline = SystemTime::now() + WATCHDOG_JOB_GRACE;
            std::thread::spawn(move || loop {
                if watchdog_deadline_reached(SystemTime::now(), deadline) {
                    // A finishing export, conversion or backup gets the longer grace: killing it would leave
                    // a half-written file.
                    let busy = bg_ops_running_count() > 0
                        && !watchdog_deadline_reached(SystemTime::now(), hard_deadline);
                    if !busy {
                        append_crash_log("watchdog: forced the process to end after the main window closed");
                        std::process::exit(0);
                    }
                }
                std::thread::sleep(Duration::from_millis(200));
            });
            if let Some(state) = handle2.try_state::<Db>() {
                let store = state.lock().unwrap_or_else(|e| e.into_inner());
                save_settings(&handle2, &store.settings);
            }
            voice_shutdown(&handle2);
            handle2.exit(0);
        }
        _ => {}
    });
    Ok(win)
}

// The close handler's decision, read from the mirrored flag so the event-loop
// thread never waits on the store lock.
pub(crate) fn close_minimizes(portable: bool) -> bool {
    !portable && crate::store::MINIMIZE_TO_TRAY.load(std::sync::atomic::Ordering::Relaxed)
}

// How long the Destroyed-event watchdog gives handle.exit(0) to actually end
// the process before forcing it.
pub(crate) const WATCHDOG_GRACE: Duration = Duration::from_secs(5);

// The longer ceiling used while a background job is still finishing, so the
// forced exit never truncates an export, conversion or backup mid-write.
pub(crate) const WATCHDOG_JOB_GRACE: Duration = Duration::from_secs(120);

// Pure decision for the watchdog thread, split out so a test can use fixed instants.
pub(crate) fn watchdog_deadline_reached(now: SystemTime, deadline: SystemTime) -> bool {
    now >= deadline
}

// Global-hotkey action: open the standalone quick-launcher instead of raising the main
// window. Replaces any launcher already open so a second press never leaves two fighting
// over the focus.
pub(crate) fn summon_launcher(app: &AppHandle) {
    LAUNCHER_PREFILL.lock().unwrap_or_else(|e| e.into_inner()).clear();
    open_launcher_window(app, false);
}

// The launcher window, shared by the hotkey and the tray filter row. `deferred` builds it
// hidden without taking focus: the tray keeps the typist's field until the launcher page has
// loaded and calls launcher_ready, since keystrokes sent earlier are lost. A launcher that
// never reports in is revealed after LAUNCHER_REVEAL_FALLBACK anyway.
pub(crate) fn open_launcher_window(app: &AppHandle, deferred: bool) {
    if let Some(win) = app.get_webview_window(LAUNCHER_WIN_LABEL) {
        let _ = win.close();
        // close() only queues the teardown, and building under the same label before
        // it ran fails, which would leave no launcher at all. The rebuild waits for
        // the label off this thread: it may be the event loop that runs the teardown.
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            for _ in 0..50 {
                if app2.get_webview_window(LAUNCHER_WIN_LABEL).is_none() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            build_launcher_window(&app2, deferred);
        });
        return;
    }
    build_launcher_window(app, deferred);
}

fn build_launcher_window(app: &AppHandle, deferred: bool) {
    let gen = LAUNCHER_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    // Event-loop thread: never wait on the store. Busy means "keep it out of screen
    // captures", the cautious default.
    let excluded = {
        let state: State<Db> = app.state();
        try_lock_ignoring_poison(&state).map(|store| capture_excluded(&store.settings)).unwrap_or(true)
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

// Open the project's GitHub page in the default browser. The URLs are hardcoded so the
// webview can never open an arbitrary address.
#[tauri::command]
pub(crate) fn open_repo(page: Option<String>) -> Result<(), String> {
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

// Generated at build time (build.rs) from the real dependency manifests and embedded as a
// JSON string for the frontend; build.rs writes THIRD-PARTY-NOTICES.md from the same source.
pub(crate) static THIRD_PARTY_LICENSES_JSON: &str =
    include_str!(concat!(env!("OUT_DIR"), "/third_party_licenses.json"));

#[tauri::command]
pub(crate) fn third_party_notices() -> &'static str {
    THIRD_PARTY_LICENSES_JSON
}

// The window's minimum logical size at a webview zoom factor: the 680x460 floor grown by the
// factor keeps that many CSS pixels visible, capped to the monitor.
pub(crate) fn zoom_min_size(factor: f64, cap: (u32, u32)) -> (u32, u32) {
    let min_w = ((MAIN_WIN_MIN_W as f64 * factor).round() as u32).min(cap.0);
    let min_h = ((MAIN_WIN_MIN_H as f64 * factor).round() as u32).min(cap.1);
    (min_w, min_h)
}

// Scale the main-window UI with the webview's native zoom (like Ctrl +/-). Unlike a CSS
// `zoom` on <body>, it keeps event coordinates and getBoundingClientRect in one space, so
// popup placement and grid hit-testing stay correct. Clamped to the slider's 75-300% range.
#[tauri::command]
pub(crate) fn set_ui_zoom(app: AppHandle, factor: f64) -> Result<(), String> {
    let f = factor.clamp(0.75, 3.0);
    if let Some(win) = app.get_webview_window("main") {
        win.set_zoom(f).map_err(|e| e.to_string())?;
        // Grow the window's minimum by the zoom factor, and the window itself if it is smaller, so
        // the header and composer stay reachable (the fixed OS minimum shows fewer CSS pixels as
        // zoom climbs). Capped against the monitor the window is on now, falling back to primary.
        let cap = monitor_cap(win.current_monitor().ok().flatten().or_else(|| win.primary_monitor().ok().flatten()));
        let (min_w, min_h) = zoom_min_size(f, cap);
        let _ = win.set_min_size(Some(tauri::LogicalSize::new(min_w, min_h)));
        if let Ok(size) = win.outer_size() {
            let scale = win.scale_factor().unwrap_or(1.0);
            // Round like zoom_min_size, not truncate: a size right at the new minimum could otherwise
            // read 1px under it and shave the other, already fine dimension too.
            let cur_w = (size.width as f64 / scale).round() as u32;
            let cur_h = (size.height as f64 / scale).round() as u32;
            if cur_w < min_w || cur_h < min_h {
                let _ = win.set_size(tauri::LogicalSize::new(cur_w.max(min_w), cur_h.max(min_h)));
            }
        }
    }
    Ok(())
}

// ---------- Window geometry ----------

pub(crate) fn point_on_monitor(monitors: &[tauri::Monitor], x: i32, y: i32) -> bool {
    monitors.iter().any(|m| {
        let p = m.position();
        let s = m.size();
        x >= p.x && x < p.x + s.width as i32 && y >= p.y && y < p.y + s.height as i32
    })
}

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
        return WindowGeom { x, y, width, height, maximized: false };
    }
    WindowGeom { x: 100, y: 100, width, height, maximized: false }
}

// A monitor's usable size in logical pixels, or effectively no cap when none is given
// (lookup failed). The zoom clamp already bounds the multiplication fed to zoom_min_size.
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
            let mut c = centered_on_primary(main, g.width, g.height);
            c.maximized = g.maximized;
            return c;
        }
    }
    // First start: 50% x 50% of the primary screen (logical), centered.
    let (width, height) = (
        (cap.0 / 2).max(MAIN_WIN_MIN_W).min(cap.0),
        (cap.1 / 2).max(MAIN_WIN_MIN_H).min(cap.1),
    );
    centered_on_primary(main, width, height)
}

// Whether the main window is maximized right now (false when it cannot be asked).
fn main_is_visible(handle: &AppHandle) -> bool {
    handle.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

fn main_is_maximized(handle: &AppHandle) -> bool {
    handle.get_webview_window("main").and_then(|w| w.is_maximized().ok()).unwrap_or(false)
}

// A maximized window only sets the flag: its bounds are the work area, not the user's size, so the
// stored normal bounds stay for the next un-maximize or start.
pub(crate) fn geom_after_move(g: &mut WindowGeom, x: i32, y: i32, maximized: bool) {
    g.maximized = maximized;
    if !maximized {
        g.x = x;
        g.y = y;
    }
}

pub(crate) fn geom_after_resize(g: &mut WindowGeom, width: u32, height: u32, maximized: bool) {
    g.maximized = maximized;
    if !maximized {
        g.width = width;
        g.height = height;
    }
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
            maximized: false,
        });
        f(&mut g);
        store.settings.window = Some(g);
    }
}
