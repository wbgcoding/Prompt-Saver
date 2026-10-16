use super::*;

// ---- Recording control bar: a small always-on-top window with its own
// WDA_EXCLUDEFROMCAPTURE (unconditional -- unlike the privacy setting the pill
// windows use, the bar must NEVER appear in the recording, full stop), pinned
// top-centre of the recorded monitor or to the top-right corner of the
// recorded window, following it while it moves. ----

#[cfg(windows)]
pub(crate) fn monitor_bounds(hmonitor: isize) -> Option<(i32, i32, i32, i32)> {
    #[repr(C)]
    struct RectI { left: i32, top: i32, right: i32, bottom: i32 }
    #[repr(C)]
    struct MonitorInfoW { size: u32, monitor: RectI, work: RectI, flags: u32 }
    #[link(name = "user32")]
    extern "system" {
        fn GetMonitorInfoW(hmonitor: isize, info: *mut MonitorInfoW) -> i32;
    }
    let mut info = MonitorInfoW {
        size: std::mem::size_of::<MonitorInfoW>() as u32,
        monitor: RectI { left: 0, top: 0, right: 0, bottom: 0 },
        work: RectI { left: 0, top: 0, right: 0, bottom: 0 },
        flags: 0,
    };
    if unsafe { GetMonitorInfoW(hmonitor, &mut info) } == 0 {
        return None;
    }
    Some((info.monitor.left, info.monitor.top, info.monitor.right, info.monitor.bottom))
}

#[cfg(windows)]
pub(crate) fn primary_hmonitor() -> isize {
    #[link(name = "user32")]
    extern "system" {
        fn MonitorFromWindow(hwnd: isize, flags: u32) -> isize;
    }
    const MONITOR_DEFAULTTOPRIMARY: u32 = 1;
    unsafe { MonitorFromWindow(0, MONITOR_DEFAULTTOPRIMARY) }
}

#[cfg(windows)]
pub(crate) fn external_window_rect(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    #[repr(C)]
    struct RectW { left: i32, top: i32, right: i32, bottom: i32 }
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowRect(hwnd: isize, r: *mut RectW) -> i32;
        fn IsWindow(hwnd: isize) -> i32;
    }
    if unsafe { IsWindow(hwnd) } == 0 {
        return None;
    }
    let mut r = RectW { left: 0, top: 0, right: 0, bottom: 0 };
    if unsafe { GetWindowRect(hwnd, &mut r) } == 0 {
        return None;
    }
    Some((r.left, r.top, r.right, r.bottom))
}

#[cfg(windows)]
pub(crate) fn position_rec_bar(win: &tauri::WebviewWindow, target_kind: &str, hwnd_or_monitor: isize) {
    let size = win.outer_size().unwrap_or(PhysicalSize::new(320, 56));
    if target_kind == "window" {
        if let Some((left, top, right, _bottom)) = external_window_rect(hwnd_or_monitor) {
            let x = (right - size.width as i32 - 12).max(left);
            let y = top + 12;
            let _ = win.set_position(PhysicalPosition::new(x, y));
            return;
        }
    }
    let hmon = if hwnd_or_monitor == 0 { primary_hmonitor() } else { hwnd_or_monitor };
    if let Some((left, top, right, _bottom)) = monitor_bounds(hmon) {
        let x = left + (right - left) / 2 - size.width as i32 / 2;
        let y = top + 16;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
}

// Re-asserts the bar's position every 200ms while a window target moves or
// resizes, stopping itself once the bar is hidden/closed or the recorded
// window itself is gone -- Tauri has no "external window moved" event, so this
// is a poll loop, same shape as the scheduler module's periodic jobs.
#[cfg(windows)]
// Bumped on every spawn_rec_bar_follow call so an old follow thread notices
// it has been superseded (a hide+show within one 200ms poll tick spawns a
// new thread for a new hwnd before the old one has seen the bar hidden) and
// exits immediately instead of fighting the new thread over positioning or
// auto-stopping the wrong (new) session when its own stale hwnd closes.
pub(crate) static REC_BAR_FOLLOW_GEN: AtomicU64 = AtomicU64::new(0);

// Once the user has dragged the bar themselves, the 200ms poll must stop
// re-snapping it back to the tracked window's corner -- reset true at the
// start of every follow thread (a fresh recording gets the auto-follow back),
// flipped false by rec_bar_mark_moved() the moment a drag begins. The window
// closing/stopping the recording is unaffected: that check runs regardless.
pub(crate) static REC_BAR_FOLLOWING: AtomicBool = AtomicBool::new(true);

#[tauri::command]
pub(crate) fn rec_bar_mark_moved() {
    REC_BAR_FOLLOWING.store(false, Ordering::SeqCst);
}

pub(crate) fn spawn_rec_bar_follow(app: AppHandle, hwnd: isize) {
    let my_gen = REC_BAR_FOLLOW_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    REC_BAR_FOLLOWING.store(true, Ordering::SeqCst);
    let _ = std::thread::Builder::new().name("rec-bar-follow".into()).spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if REC_BAR_FOLLOW_GEN.load(Ordering::SeqCst) != my_gen {
            break;
        }
        let Some(win) = app.get_webview_window("rec-bar") else { break };
        if !win.is_visible().unwrap_or(false) {
            break;
        }
        if external_window_rect(hwnd).is_none() {
            // The recorded window itself closed mid-recording: the capture source
            // is gone, so auto-stop (keeping the file so far) rather than leaving
            // a dead session running and the bar stuck at its last position.
            let _ = stop_recording_now(&app);
            let _ = win.hide();
            break;
        }
        if REC_BAR_FOLLOWING.load(Ordering::SeqCst) {
            position_rec_bar(&win, "window", hwnd);
        }
    });
}

// Cheap insurance against a future regression of the same shape as the
// show_rec_bar bug -- if the bar's window exists but is not visible while a
// recording is still running, the user would otherwise be locked out with no
// way to stop it again. Registered unconditionally at startup; the
// recording_active() check makes every tick a no-op outside an actual
// recording, so the always-on cost is one cheap state read every few seconds.
pub(crate) fn rec_bar_heal_tick(app: &AppHandle) {
    if !recording_active(app) {
        return;
    }
    if let Some(win) = app.get_webview_window("rec-bar") {
        if !win.is_visible().unwrap_or(true) {
            append_crash_log("rec-bar: hidden while a recording is running, showing it again");
            let _ = win.show();
        }
    }
}

// Remembers the target the bar is currently anchored to, so fit_rec_bar can
// re-run the same placement show_rec_bar used after a resize without the
// frontend having to resend a target it never itself tracked.
pub(crate) static REC_BAR_TARGET: Mutex<(String, isize)> = Mutex::new((String::new(), 0));

// Async on purpose: a synchronous command runs on the main thread, and building
// a webview window from there never returns -- the window gets created, but
// every step after build() (capture exclusion, positioning, show) is skipped,
// leaving an invisible bar and a recording with no way to stop it.
#[tauri::command]
pub(crate) async fn show_rec_bar(app: AppHandle, target_kind: String, id: i64) -> Result<(), String> {
    #[cfg(windows)]
    {
        let label = "rec-bar";
        let win = match app.get_webview_window(label) {
            Some(w) => w,
            None => WebviewWindowBuilder::new(&app, label, WebviewUrl::App("recbar.html".into()))
                .title("")
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .shadow(false)
                .inner_size(320.0, 56.0)
                .visible(false)
                .build()
                .map_err(|e| e.to_string())?,
        };
        let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
        // Unconditional -- not gated by the privacy screenshot-exclusion setting.
        // The bar must never appear in the recording no matter what.
        set_capture_exclusion(&win, true);
        *REC_BAR_TARGET.lock().unwrap_or_else(|e| e.into_inner()) = (target_kind.clone(), id as isize);
        position_rec_bar(&win, &target_kind, id as isize);
        let _ = win.show();
        // A reused bar page still holds the last recording's state.
        let _ = app.emit_to(label, "rec-bar-reset", ());
        if target_kind == "window" {
            spawn_rec_bar_follow(app.clone(), id as isize);
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, target_kind, id);
        Err("recording is only available on Windows".to_string())
    }
}

// The bar's own content decides its size now: a fixed 320x56 window used to
// clip stop-btn/cancel-btn off the right edge for anyone with an optional
// icon shown. recbar.js measures its real content box after every layout
// change and asks for exactly that. Re-running position_rec_bar keeps the bar
// anchored the same way show_rec_bar first placed it instead of leaving it
// pinned at its old top-left corner as the width changes.
#[tauri::command]
pub(crate) fn fit_rec_bar(app: AppHandle, width: f64, height: f64) {
    #[cfg(windows)]
    {
        if let Some(win) = app.get_webview_window("rec-bar") {
            let _ = win.set_size(tauri::LogicalSize::new(width, height));
            let (target_kind, id) = REC_BAR_TARGET.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if !target_kind.is_empty() {
                position_rec_bar(&win, &target_kind, id);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (app, width, height);
    }
}

#[tauri::command]
pub(crate) fn hide_rec_bar(app: AppHandle) {
    if let Some(w) = app.get_webview_window("rec-bar") {
        let _ = w.hide();
    }
}

// A region recording's own outline, standing in for the Windows capture
// border a region target now switches off (recording::border_setting_for_target):
// WGC captures the whole monitor and the app crops it down, so the OS border
// would ring the entire screen rather than the recorded area. Click-through,
// always-on-top and capture-excluded like the rec bar, but built fresh per
// recording and destroyed rather than reused -- unlike the bar's fixed shape,
// a region's rect is different every time. No-op (best-effort) on Monitor and
// Window recordings, which keep the OS border and never call this at all.
// Never focused: it has nothing to click or type into, and showing it must
// not steal keyboard focus away from whatever app is being recorded.
// Bumped for every outline window that is shown or torn down, so a re-apply
// queued for an earlier outline can never touch the next recording's one.
pub(crate) static REC_OUTLINE_GEN: AtomicU64 = AtomicU64::new(0);

pub(crate) fn outline_closure_is_current(gen: u64, current: u64) -> bool {
    gen == current
}

// The outline's shipped frame colour, the one recoutline.html falls back to.
pub(crate) const REC_OUTLINE_DEFAULT_COLOR: &str = "#2563eb";

// The `recOutline` switch ships on: only an explicit false hides the frame.
pub(crate) fn rec_outline_wanted(settings: &Settings) -> bool {
    settings.ui_flags.get("recOutline").copied().unwrap_or(true)
}

// The stored colour, or the default when it is missing or not a plain
// `#rrggbb`. The value ends up inside a script string handed to the outline
// window, so nothing but six hex digits may ever pass.
pub(crate) fn rec_outline_color(settings: &Settings) -> String {
    match settings.ui_texts.get("recOutlineColor") {
        Some(c) if c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit()) => c.to_ascii_lowercase(),
        _ => REC_OUTLINE_DEFAULT_COLOR.to_string(),
    }
}

// What a recording start asks the outline for: the region and the colour, or
// nothing when the target has no outline (monitor and window recordings) or
// the user switched it off.
pub(crate) fn rec_outline_request(settings: &Settings, rect: Option<(i32, i32, u32, u32)>) -> Option<((i32, i32, u32, u32), String)> {
    if !rec_outline_wanted(settings) {
        return None;
    }
    rect.map(|r| (r, rec_outline_color(settings)))
}

// Runs in the outline window before its page, so the frame has its colour from
// the first paint. The page reads it from the `--outline-color` variable.
pub(crate) fn rec_outline_init_script(color: &str) -> String {
    format!(
        "(function(){{var c='{color}';var a=function(){{document.documentElement.style.setProperty('--outline-color',c);}};if(document.readyState==='loading'){{document.addEventListener('DOMContentLoaded',a);}}else{{a();}}}})();"
    )
}

#[cfg(windows)]
pub(crate) fn show_rec_outline(app: &AppHandle, rect: (i32, i32, u32, u32), color: &str) {
    let label = "rec-outline";
    if app.get_webview_window(label).is_some() {
        return;
    }
    let gen = REC_OUTLINE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let win = match WebviewWindowBuilder::new(app, label, WebviewUrl::App("recoutline.html".into()))
        .initialization_script(rec_outline_init_script(color))
        .title("")
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .focused(false)
        .build()
    {
        Ok(w) => w,
        // A missing outline is a cosmetic loss, never worth failing an
        // already-started recording over.
        Err(_) => return,
    };
    let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    // Unconditional, like the rec bar: it must never appear in the recording
    // no matter what the privacy screenshot-exclusion setting says. Applied
    // while the window is still hidden, before any placement below.
    set_capture_exclusion(&win, true);
    let _ = win.set_ignore_cursor_events(true); // click-through: never steals input from whatever is under it
    // The window is built on whichever monitor the system picks and then moved
    // to the region's own monitor, which can have another scale factor. tao
    // answers the resulting DPI change by resizing the window to the system's
    // suggested rect, which rescales its physical size. So the rect is applied
    // again after every scale or size event, deferred to the main thread (a
    // re-apply inside the event itself runs before tao's own resize and is
    // undone by it). The pixel guard keeps that from looping.
    let want = rect;
    let app_evt = app.clone();
    win.on_window_event(move |event| {
        if matches!(event, WindowEvent::ScaleFactorChanged { .. } | WindowEvent::Resized(_)) {
            let app2 = app_evt.clone();
            let _ = app_evt.run_on_main_thread(move || {
                if !outline_closure_is_current(gen, REC_OUTLINE_GEN.load(Ordering::SeqCst)) {
                    return;
                }
                if let Some(w) = app2.get_webview_window("rec-outline") {
                    place_rec_outline(&w, want);
                }
            });
        }
    });
    place_rec_outline(&win, want);
    let _ = win.show();
    // Showing can trigger the DPI settle too; one more deferred check covers
    // a settle that lands after the event handler above was already queued.
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if !outline_closure_is_current(gen, REC_OUTLINE_GEN.load(Ordering::SeqCst)) {
            return;
        }
        if let Some(w) = app2.get_webview_window("rec-outline") {
            place_rec_outline(&w, want);
        }
    });
}

// Put the outline window on its requested rect and confirm it stuck: if the
// window is more than a couple of pixels off afterwards (a DPI boundary crossed
// by the first move), apply the rect once more, which no longer crosses one.
#[cfg(windows)]
pub(crate) fn place_rec_outline(win: &tauri::WebviewWindow, rect: (i32, i32, u32, u32)) {
    let (x, y, w, h) = rect;
    let r = Rect { x: x as f64, y: y as f64, w: w as f64, h: h as f64 };
    for _ in 0..2 {
        let actual = match (win.outer_position(), win.outer_size()) {
            (Ok(p), Ok(s)) => Some((p.x, p.y, s.width, s.height)),
            _ => None,
        };
        if let Some(a) = actual {
            if !outline_rect_needs_reapply(a, rect) {
                return;
            }
        }
        set_bounds_phys(win, r);
    }
}

// Pure: whether a window sitting at `actual` has to be moved or resized to
// reach `want`. Anything within OUTLINE_TOLERANCE_PX on every edge counts as
// placed, so a rounding difference never starts a move/resize loop.
pub(crate) const OUTLINE_TOLERANCE_PX: i64 = 2;
pub(crate) fn outline_rect_needs_reapply(actual: (i32, i32, u32, u32), want: (i32, i32, u32, u32)) -> bool {
    let d = |a: i64, b: i64| (a - b).abs() > OUTLINE_TOLERANCE_PX;
    d(actual.0 as i64, want.0 as i64)
        || d(actual.1 as i64, want.1 as i64)
        || d(actual.2 as i64, want.2 as i64)
        || d(actual.3 as i64, want.3 as i64)
}

// Torn down (not just hidden) on stop, cancel and auto-stop -- see
// stop_recording_now and cancel_recording.
#[cfg(windows)]
pub(crate) fn hide_rec_outline(app: &AppHandle) {
    REC_OUTLINE_GEN.fetch_add(1, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window("rec-outline") {
        let _ = w.close();
    }
}

#[derive(serde::Serialize)]
pub(crate) struct RecordingStatus {
    pub(crate) paused: bool,
    pub(crate) elapsed_ms: i64,
    pub(crate) bytes: u64,
    pub(crate) audio_system: bool,
    pub(crate) audio_mic: bool,
    // true once the mic capture thread hit AUDCLNT_E_DEVICE_INVALIDATED
    // (unplugged mid-recording) -- the recording itself keeps running with
    // silence in place of the mic track; the control bar surfaces this once
    // as a warning state on the mic indicator the first time it flips true.
    pub(crate) mic_lost: bool,
    // The same for the system (loopback) capture: no render device, or the
    // stream could not start. The track is silence; the bar says so.
    pub(crate) system_lost: bool,
    // Whether the rec bar's own mute buttons currently have each source
    // silenced. Always false for a source that was never active this session.
    pub(crate) audio_system_muted: bool,
    pub(crate) audio_mic_muted: bool,
    // Set once an unrecoverable encoder write failure happened (e.g. disk
    // full) -- the feeder has already stopped itself at this point, so the
    // control bar's own poll uses this to stop the session properly instead
    // of leaving it looking like it is still recording.
    pub(crate) encode_error: Option<String>,
    // False when the last rearm_hotkeys() could not actually claim the
    // stop/pause combination (another app already holds it) -- surfaced here
    // rather than silently, so the bar can warn instead of the user believing
    // in a binding that never fires.
    pub(crate) stop_hotkey_ok: bool,
    pub(crate) pause_hotkey_ok: bool,
    // recBarSize (Vorgabe an): whether the bar should show the size at all
    // right now -- false below REC_BAR_SIZE_FROM_MB, or with the switch off.
    pub(crate) show_size: bool,
    // Seconds left at the observed average rate (bytes so far / elapsed so
    // far -- never estimated from the configured bitrate, see rec_bar_size_state)
    // before recMaxMb is reached. Only Some(..) past three quarters of that
    // ceiling; None both below it and with show_size false.
    pub(crate) size_eta_secs: Option<u64>,
    // recTargetSwitch (Vorgabe an): whether the bar's target-switch button is
    // enabled at all right now -- a window recording using the "every
    // display" or "current display" canvas, with the master flag on. A
    // canvas-mode "monitor" recording still needs the actual pick to land on
    // the same screen, only checkable once something has actually been
    // picked, so a false here already means the button itself is disabled;
    // a true here does not yet guarantee any given pick will succeed.
    pub(crate) target_switch_enabled: bool,
    // The mid-recording half of the low-disk warning (the before-start check is
    // check_recording_disk_space) -- Some at most once per recording, the
    // instant free space first drops below the threshold. Never causes a stop.
    pub(crate) disk_low: Option<(u64, u64)>,
}

// recBarSize's own threshold logic: whether to show the size at all, and
// once past three quarters of recMaxMb, an ETA to that ceiling from the
// recording's own observed average rate so far -- pure and window-free so
// this has a real unit test, not just its recording_status wiring (same
// reasoning as estimate_export_bytes/recording_disk_warning above).
// recMaxMb <= 0 disables the ETA half only, size still shows past show_from_mb.
pub(crate) fn rec_bar_size_state(bytes: u64, elapsed_ms: i64, show_from_mb: f64, max_mb: f64) -> (bool, Option<u64>) {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb < show_from_mb {
        return (false, None);
    }
    if max_mb <= 0.0 || mb < max_mb * 0.75 {
        return (true, None);
    }
    let elapsed_secs = elapsed_ms as f64 / 1000.0;
    if elapsed_secs <= 0.0 {
        return (true, None);
    }
    let rate = bytes as f64 / elapsed_secs;
    if rate <= 0.0 {
        return (true, None);
    }
    let remaining_bytes = (max_mb * 1024.0 * 1024.0 - bytes as f64).max(0.0);
    (true, Some((remaining_bytes / rate) as u64))
}

// The file size from which the recording bar shows its size readout (MB).
pub(crate) const REC_BAR_SIZE_FROM_MB: f64 = 500.0;

// The four recording-bar settings recording_status needs, read out of an
// already-held store guard so the try_lock fallback below can share the code
// with the normal path instead of duplicating the field list.
pub(crate) fn rec_bar_settings(store: &Store) -> (bool, f64, f64, bool) {
    (
        store.settings.ui_flags.get("recBarSize") != Some(&false),
        REC_BAR_SIZE_FROM_MB,
        store.settings.ui_values.get("recMaxMb").copied().unwrap_or(8000.0),
        store.settings.ui_flags.get("recTargetSwitch") != Some(&false),
    )
}

// Read-only status for the control bar's live timer + file-size readout;
// polled from the frontend rather than pushed, so a bar that briefly misses a
// tick self-corrects on the next one instead of drifting.
//
// Async and never waiting on the store: the recording bar polls this twice a
// second; as a sync command waiting on the store lock it froze every window
// while a backup ran.
#[cfg(windows)]
#[tauri::command]
pub(crate) async fn recording_status(
    rstate: State<'_, RecordingState>,
    db: State<'_, Db>,
) -> Result<Option<RecordingStatus>, String> {
    // Read first, and never block: a backup holds the store lock across its
    // whole snapshot, and waiting for it here would freeze every window right
    // along with the poll that used to wait on it.
    let (bar_size_on, show_from_mb, max_mb, target_switch_on) = match db.try_lock() {
        Ok(g) => rec_bar_settings(&g),
        Err(std::sync::TryLockError::Poisoned(e)) => rec_bar_settings(&e.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => (true, REC_BAR_SIZE_FROM_MB, 8000.0, true),
    };
    let guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
    let Some(session) = guard.as_ref() else { return Ok(None) };
    let bytes = std::fs::metadata(&session.output_path).map(|m| m.len()).unwrap_or(0);
    let elapsed_ms = recording::elapsed_ms(session);
    let (show_size, size_eta_secs) =
        if bar_size_on { rec_bar_size_state(bytes, elapsed_ms, show_from_mb, max_mb) } else { (false, None) };
    let target_switch_enabled = target_switch_on && recording::can_switch_target(session);
    Ok(Some(RecordingStatus {
        paused: session.phase == RecPhase::Paused,
        elapsed_ms,
        bytes,
        audio_system: session.audio_system,
        audio_mic: session.audio_mic,
        mic_lost: session.mic_queue.as_ref().map(|q| q.is_lost()).unwrap_or(false),
        system_lost: session.system_queue.as_ref().map(|q| q.is_lost()).unwrap_or(false),
        audio_system_muted: session.system_queue.as_ref().map(|q| q.is_muted()).unwrap_or(false),
        audio_mic_muted: session.mic_queue.as_ref().map(|q| q.is_muted()).unwrap_or(false),
        encode_error: recording::encode_error(session),
        stop_hotkey_ok: REC_STOP_HOTKEY_OK.load(std::sync::atomic::Ordering::Relaxed),
        pause_hotkey_ok: REC_PAUSE_HOTKEY_OK.load(std::sync::atomic::Ordering::Relaxed),
        show_size,
        size_eta_secs,
        target_switch_enabled,
        disk_low: recording::take_disk_low(session),
    }))
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) async fn recording_status(
    rstate: State<'_, RecordingState>,
    db: State<'_, Db>,
) -> Result<Option<RecordingStatus>, String> {
    let _ = (rstate, db);
    Ok(None)
}

#[derive(serde::Serialize)]
pub(crate) struct MicDeviceInfo {
    pub(crate) id: String,
    pub(crate) name: String,
}

// Backs the recMicDevice expert dropdown. Best-effort (see
// audio::list_mic_devices): a broken audio stack yields an empty list, and
// the frontend falls back to "system default" rather than showing an error.
#[cfg(windows)]
#[tauri::command]
pub(crate) fn list_mic_devices() -> Vec<MicDeviceInfo> {
    recording::list_mic_devices().into_iter().map(|d| MicDeviceInfo { id: d.id, name: d.name }).collect()
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) fn list_mic_devices() -> Vec<MicDeviceInfo> {
    Vec::new()
}
