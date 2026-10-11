use super::*;

// Bring the main window back from the tray: always at the saved size/position.
// Routed through resolve_geometry() (the same validation the startup path
// uses) instead of trusting the saved geometry raw -- a monitor unplugged
// while the app sat minimized in the tray must not be able to restore the
// window off every remaining screen. A window that is simply gone (the process
// kept running with zero top-level windows, the never-comes-back state a
// crashed-out webview or a bug can leave behind) is rebuilt first instead of
// leaving the call a silent no-op.
pub(crate) fn show_main(app: &AppHandle) {
    park_idle_snip_overlay(app);
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
    reapply_taskbar_state(app);
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
                // Never vanish while an export, conversion, ffmpeg
                // download, backup or import is still going --
                // request_app_quit asks the main window first,
                // exactly once per quit attempt (QUIT_CONFIRMED
                // lets a quit through once the user has answered).
                api.prevent_close();
                request_app_quit(&handle2);
            }
        }
        WindowEvent::Destroyed => {
            // No process without a main window: end this one right away, and
            // back that up with a watchdog in case exit() itself never
            // completes (a stuck shutdown handler, for instance) -- the same
            // silent-zombie shape the bug this guards against started from.
            // The watchdog starts before anything that can wait on the store
            // lock, so a backup or import holding it cannot keep it from running.
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

// Open the project's GitHub page in the default browser (app logo → repo,
// version label → releases). The URLs are hardcoded: the webview can never open
// an arbitrary address.
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

// The third-party license list is generated at build time (build.rs) from the real
// dependency manifests and embedded here as a plain JSON string; the frontend parses
// it. Regenerating THIRD-PARTY-NOTICES.md from the same source happens in build.rs
// too, so the two never show a different list.
pub(crate) static THIRD_PARTY_LICENSES_JSON: &str =
    include_str!(concat!(env!("OUT_DIR"), "/third_party_licenses.json"));

#[tauri::command]
pub(crate) fn third_party_notices() -> &'static str {
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
// ui/expertConfig.js EXPERT_VALUES uiScale, has always had a 75% floor).
#[tauri::command]
pub(crate) fn set_ui_zoom(app: AppHandle, factor: f64) -> Result<(), String> {
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
