use super::*;

// ---------- Disk space check (recording start, mid-recording, export) ----------

// Walks up to the nearest existing ancestor: the destination folder may not exist
// yet (a fresh recordings or export folder), but the drive it will land on always
// does. GetDiskFreeSpaceExW resolves whichever drive the given path sits on --
// never a fixed "C:\" -- so a target on a secondary drive is checked correctly.
#[cfg(windows)]
pub(crate) fn free_space_bytes(path: &std::path::Path) -> std::io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let mut probe = path;
    while !probe.exists() {
        match probe.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => probe = parent,
            _ => break,
        }
    }
    // The call wants a directory: a live recording file resolves to its folder.
    if probe.is_file() {
        if let Some(parent) = probe.parent().filter(|p| !p.as_os_str().is_empty()) {
            probe = parent;
        }
    }
    let wide: Vec<u16> = probe.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free_available = 0u64;
    unsafe { GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut free_available), None, None) }
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(free_available)
}

// The drive letter a path resolves under ("C:"), for the disk-space warning
// message. Empty when the path has no such prefix -- never happens for the
// absolute paths this app always deals in; a message missing the drive is
// harmless either way.
pub(crate) fn drive_label(path: &std::path::Path) -> String {
    match path.components().next() {
        Some(std::path::Component::Prefix(p)) => p.as_os_str().to_string_lossy().into_owned(),
        _ => String::new(),
    }
}

// Bitrate times time, with a flat surcharge for whichever audio track(s) are
// active (matches REC_AUDIO_KBPS) -- good enough for a warning
// message, not a byte-exact prediction.
pub(crate) fn estimate_recording_seconds_remaining(free_bytes: u64, video_kbps: u32, audio_active: bool) -> u64 {
    let total_kbps = video_kbps as u64 + if audio_active { 192 } else { 0 };
    let bytes_per_sec = (total_kbps * 1000) / 8;
    if bytes_per_sec == 0 {
        return 0;
    }
    free_bytes / bytes_per_sec
}

#[derive(serde::Serialize)]
pub(crate) struct RecDiskWarning {
    pub(crate) drive: String,
    pub(crate) free_mb: u64,
    pub(crate) est_seconds: u64,
}

// Pure so the threshold decision itself has a real unit test, not just its
// AppHandle/Db wiring (same reasoning as estimate_export_bytes above it).
pub(crate) fn recording_disk_warning(drive: String, free_bytes: u64, warn_mb: f64, video_kbps: u32, audio_active: bool) -> Option<RecDiskWarning> {
    let free_mb = free_bytes / (1024 * 1024);
    if free_mb as f64 >= warn_mb {
        return None;
    }
    Some(RecDiskWarning { drive, free_mb, est_seconds: estimate_recording_seconds_remaining(free_bytes, video_kbps, audio_active) })
}

// Free space below this many MB triggers the low-disk warning.
pub(crate) const REC_DISK_WARN_MB: f64 = 500.0;
// Bitrate of the recorded audio tracks.
pub(crate) const REC_AUDIO_KBPS: u32 = 192;

#[cfg(windows)]
#[tauri::command(async)]
pub(crate) fn check_recording_disk_space(app: AppHandle, db: State<Db>) -> Option<RecDiskWarning> {
    let dir = data_dir(&app).join("recordings");
    let free = free_space_bytes(&dir).ok()?;
    let (warn_mb, video_kbps, audio_active) = {
        let g = db.lock().unwrap_or_else(|e| e.into_inner());
        (
            REC_DISK_WARN_MB,
            g.settings.ui_values.get("recBitrateKbps").copied().unwrap_or(12000.0).clamp(1000.0, 50000.0) as u32,
            g.settings.ui_flags.get("recAudioSystem") != Some(&false) || g.settings.ui_flags.get("recAudioMic") != Some(&false),
        )
    };
    recording_disk_warning(drive_label(&dir), free, warn_mb, video_kbps, audio_active)
}
#[cfg(not(windows))]
#[tauri::command(async)]
pub(crate) fn check_recording_disk_space(app: AppHandle, db: State<Db>) -> Option<RecDiskWarning> {
    let _ = (app, db);
    None
}

#[derive(serde::Serialize)]
pub(crate) struct ExportDiskWarning {
    pub(crate) drive: String,
    pub(crate) free_mb: u64,
    pub(crate) needed_mb: u64,
}

// needed_bytes already includes whatever headroom the caller wants (see
// check_export_disk_space: the estimate plus a working-file margin) -- this
// function only compares it against what is actually free.
pub(crate) fn export_disk_warning(drive: String, free_bytes: u64, needed_bytes: u64) -> Option<ExportDiskWarning> {
    let free_mb = free_bytes / (1024 * 1024);
    let needed_mb = needed_bytes / (1024 * 1024);
    if free_mb >= needed_mb {
        return None;
    }
    Some(ExportDiskWarning { drive, free_mb, needed_mb })
}

#[cfg(windows)]
#[tauri::command]
pub(crate) fn check_export_disk_space(dest_path: String, format: String, video_kbps: Option<u32>, lossless_quality: bool, duration_ms: u64) -> Option<ExportDiskWarning> {
    let dest = std::path::Path::new(&dest_path);
    let free = free_space_bytes(dest).ok()?;
    let est = estimate_export_bytes(&format, video_kbps, lossless_quality, duration_ms);
    let needed = est.bytes.or(est.high_bytes).unwrap_or(0);
    // A generous margin rather than a byte-exact one -- covers filesystem
    // overhead and any working file ffmpeg creates alongside the final output.
    let needed_with_margin = needed + needed / 5;
    export_disk_warning(drive_label(dest), free, needed_with_margin)
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) fn check_export_disk_space(dest_path: String, format: String, video_kbps: Option<u32>, lossless_quality: bool, duration_ms: u64) -> Option<ExportDiskWarning> {
    let _ = (dest_path, format, video_kbps, lossless_quality, duration_ms);
    None
}

// The batch-export dialog's own version of the check above: the same
// per-file estimate, summed over every selected recording instead of just
// one, because a batch's real risk is running out of space partway through
// the third or fourth file, not the first.
#[cfg(windows)]
#[tauri::command]
pub(crate) fn check_export_disk_space_batch(dest_dir: String, format: String, video_kbps: Option<u32>, lossless_quality: bool, durations_ms: Vec<u64>) -> Option<ExportDiskWarning> {
    let dest = std::path::Path::new(&dest_dir);
    let free = free_space_bytes(dest).ok()?;
    let needed: u64 = durations_ms
        .iter()
        .map(|d| {
            let est = estimate_export_bytes(&format, video_kbps, lossless_quality, *d);
            est.bytes.or(est.high_bytes).unwrap_or(0)
        })
        .sum();
    let needed_with_margin = needed + needed / 5;
    export_disk_warning(drive_label(dest), free, needed_with_margin)
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) fn check_export_disk_space_batch(dest_dir: String, format: String, video_kbps: Option<u32>, lossless_quality: bool, durations_ms: Vec<u64>) -> Option<ExportDiskWarning> {
    let _ = (dest_dir, format, video_kbps, lossless_quality, durations_ms);
    None
}

// The target half of what start_recording resolves from the picker -- its
// own function since finish_and_continue_recording never resolves a target
// this way at all (it reopens against RecSession.target, whatever the
// running session is ACTUALLY capturing right now, not a fresh pick).
#[cfg(windows)]
pub(crate) fn resolve_recording_target(
    target_kind: &str, target_id: i64, region_x: i32, region_y: i32, region_w: u32, region_h: u32,
) -> Result<recording::RecTarget, String> {
    match target_kind {
        "monitor" => Ok(recording::RecTarget::Monitor(target_id as isize)),
        "window" => Ok(recording::RecTarget::Window(target_id as isize)),
        "region" => Ok(recording::RecTarget::Region {
            monitor: target_id as isize, x: region_x, y: region_y, w: region_w, h: region_h,
        }),
        _ => Err("invalid recording target".to_string()),
    }
}

// Everything BUT the target that start_recording reads from the settings
// store to build one recording::start() call -- pulled out so
// finish_and_continue_recording can reopen with EXACTLY the same resolved
// settings the just-finished part used, instead of duplicating this whole
// block a second time and risking the two drifting apart. Settings are
// re-read fresh here rather than copied from the finishing session on
// purpose: a value the user changed WHILE recording (bitrate, mic device,
// canvas mode, ...) is meant to take effect starting at the next part, the
// same way it already would have at the very first start_recording call.
#[cfg(windows)]
pub(crate) fn resolve_recording_settings(db: &State<Db>) -> (bool, recording::AudioConfig, u32, u32, String, (u32, u32), f64, f64, f64) {
    // recCursor ships ON -- only an explicit false switches it off.
    // recAudioSystem and recAudioMic both ship ON now -- the mic is
    // captured by default too (into its own track, see start()'s both_active
    // path), just not audible unless the user brings it up in the editor
    // afterward. A stored user value (from before this change, or an explicit
    // opt-out) still wins either way.
    let audio = {
        let g = db.lock().unwrap_or_else(|e| e.into_inner());
        recording::AudioConfig {
            system: g.settings.ui_flags.get("recAudioSystem") != Some(&false),
            mic: g.settings.ui_flags.get("recAudioMic") != Some(&false),
            mic_device_id: g.settings.ui_texts.get("recMicDevice").cloned().unwrap_or_default(),
            kbps: REC_AUDIO_KBPS,
        }
    };
    let show_cursor = db.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_flags.get("recCursor") != Some(&false);
    // The quality step (small/high/lossless) only picks the slider's own default in
    // the UI -- the native H.264 encoder always takes this bitrate as-is. "Lossless"
    // is produced afterward by convert_recording's CRF-0 re-encode, so this value's
    // exact size barely matters on that step (see the recQuality tooltip).
    let video_kbps =
        db.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_values.get("recBitrateKbps").copied().unwrap_or(12000.0).clamp(1000.0, 50000.0) as u32;
    // Every monitor gets capped to this fps regardless of its actual refresh rate
    // unless overridden here -- a 60/144Hz screen otherwise always recorded at 30.
    let video_fps =
        db.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_values.get("recFps").copied().unwrap_or(30.0).clamp(10.0, 60.0) as u32;
    // The canvas a Window recording uses. Defaults to the window's own size
    // (today's original behaviour, resize-unsafe) rather than every display --
    // the "every display" candidate chain is opt-in via this same setting,
    // offered reactively the first time a recording actually hits the resize
    // problem (see the recording-captured payload's resize_crop_at_ms). No
    // expert-menu row for this setting yet (a future menu writes it directly).
    let canvas_mode = {
        let raw = db.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_texts.get("recCanvasMode").cloned().unwrap_or_else(|| "window".to_string());
        sanitize_canvas_mode(&raw).to_string()
    };
    // One fixed ceiling governs both the width and the height of the canvas.
    let canvas_ceiling = (REC_CANVAS_MAX_PX, REC_CANVAS_MAX_PX);
    // The mid-recording disk-space watch's own threshold -- see
    // check_recording_disk_space for the same value used before the start.
    let disk_warn_mb = REC_DISK_WARN_MB;
    // Optional safety nets, both off (0) unless the user sets them -- unrelated
    // to recMaxMb's own older, display-only ETA threshold (see rec_size_eta),
    // which keeps its own separate 8000 default so adding these rows changes
    // nothing about what is already shown today.
    let max_min = db.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_values.get("recMaxMin").copied().unwrap_or(0.0).max(0.0);
    let max_mb = db.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_values.get("recMaxMb").copied().unwrap_or(0.0).max(0.0);
    (show_cursor, audio, video_kbps, video_fps, canvas_mode, canvas_ceiling, disk_warn_mb, max_min, max_mb)
}

// start_recording's own check-then-act window: rec_can_start passes while
// RecordingState is still empty, then the actual capture start runs on a
// blocking thread and only publishes the session into RecordingState once it
// returns -- a second start racing through that gap used to see the same
// empty state and pass the same check, so both requests ran the capture
// start. Claimed together with the rec_can_start check, under the same
// RecordingState lock, and released by RecStartingGuard on every exit path
// (an early return, a panic, or a normal finish) so a refused or failed start
// never leaves the next one locked out for good.
#[cfg(windows)]
pub(crate) static REC_STARTING: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
pub(crate) struct RecStartingGuard;
#[cfg(windows)]
impl Drop for RecStartingGuard {
    fn drop(&mut self) {
        REC_STARTING.store(false, Ordering::SeqCst);
    }
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn start_recording(
    app: AppHandle,
    rstate: State<'_, RecordingState>,
    db: State<'_, Db>,
    target_kind: String,
    target_id: i64,
    region_x: i32,
    region_y: i32,
    region_w: u32,
    region_h: u32,
) -> Result<(), String> {
    {
        let guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
        let phase = guard.as_ref().map(|s| s.phase).unwrap_or(RecPhase::Idle);
        if !rec_can_start(phase) {
            return Err("recording-busy".to_string());
        }
        if REC_STARTING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Err("recording-busy".to_string());
        }
    }
    let _rec_starting_guard = RecStartingGuard;
    let target = resolve_recording_target(&target_kind, target_id, region_x, region_y, region_w, region_h)?;
    let (show_cursor, audio, video_kbps, video_fps, canvas_mode, canvas_ceiling, disk_warn_mb, max_min, max_mb) = resolve_recording_settings(&db);
    // Read once, before the blocking capture start -- keyOverlay's
    // default is off, so the common case installs no hook at all.
    let (key_overlay_on, key_overlay_hold_ms, key_overlay_shortcuts_only) = {
        let g = db.lock().unwrap_or_else(|e| e.into_inner());
        (keylog::enabled(&g.settings.ui_flags), keylog::HOLD_MS, keylog::shortcuts_only(&g.settings.ui_flags))
    };
    let dir = data_dir(&app).join("recordings");
    let session = tauri::async_runtime::spawn_blocking(move || {
        let out_path = recording::new_output_path(&dir)?;
        recording::start(
            target, out_path, audio, &canvas_mode,
            recording::RecStartOptions { show_cursor, video_kbps, video_fps, canvas_ceiling, disk_warn_mb, max_min, max_mb },
        )
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    let session = match session {
        Ok(s) => s,
        Err(e) => {
            // A refused start (the encoder rejected the type, the target is
            // gone) must leave nothing behind: no control bar, and the
            // composer's record button back to idle. recording::start has
            // already deleted the empty output file.
            super::rec_bar::hide_rec_bar(app.clone());
            let _ = app.emit("recording-cancelled", ());
            return Err(e);
        }
    };
    // Started BEFORE the session is published: a stop/cancel racing in the
    // instant after publish used to see an already-live recording with no
    // key-overlay capture yet, since maybe_start ran only after that publish
    // -- the hook ended up installed with nothing recording (the stop found
    // no session-scoped capture to tear down its own way) and the NEXT
    // recording's own maybe_start then silently no-op'd against it, "never a
    // second hook" mistaking the orphan for an already-running one.
    keylog::maybe_start(key_overlay_on, key_overlay_hold_ms, keylog::mono_ms(), key_overlay_shortcuts_only);
    // Region only -- Monitor/Window recordings keep the OS capture border
    // and never get an outline window at all (see outline_rect_for_target).
    let outline = {
        let g = db.lock().unwrap_or_else(|e| e.into_inner());
        rec_outline_request(&g.settings, recording::outline_rect_for_target(target))
    };
    if let Some((rect, color)) = outline {
        show_rec_outline(&app, rect, &color);
    }
    *rstate.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(session);
    // A fresh recording never inherits an earlier chain's already-closed
    // parts -- only finish_and_continue_recording ever adds to this list.
    REC_SESSION_PARTS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    // The stop/pause hotkeys are only ever live while a recording runs --
    // this is the transition into that state. Same for the tray's two extra rows.
    rearm_hotkeys(&app);
    apply_tray_menu(&app);
    Ok(())
}
#[cfg(not(windows))]
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn start_recording(
    app: AppHandle,
    rstate: State<'_, RecordingState>,
    db: State<'_, Db>,
    target_kind: String,
    target_id: i64,
    region_x: i32,
    region_y: i32,
    region_w: u32,
    region_h: u32,
) -> Result<(), String> {
    let _ = (app, rstate, db, target_kind, target_id, region_x, region_y, region_w, region_h);
    Err("recording is only available on Windows".to_string())
}

// Swaps a running recording's own source rather than starting a new
// one -- the control bar's target-switch button, only ever enabled (see
// RecordingStatus::target_switch_enabled) for a window recording using a
// placement canvas. Only a "window" pick is accepted: a monitor or region
// target never fits an already-built placement canvas the same way, and this
// feature never rebuilds the encoder to accommodate one. A failure here
// leaves the current recording completely untouched and is also broadcast as
// an event, since the caller (the separate control-bar window) has nowhere
// of its own to show it.
#[cfg(windows)]
#[tauri::command]
pub(crate) async fn switch_recording_target(
    app: AppHandle,
    rstate: State<'_, RecordingState>,
    target_kind: String,
    target_id: i64,
) -> Result<(), String> {
    if target_kind != "window" {
        let _ = app.emit("recording-target-switch-failed", "recording-target-switch-not-supported");
        return Err("recording-target-switch-not-supported".to_string());
    }
    {
        let guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
        let phase = guard.as_ref().map(|s| s.phase).unwrap_or(RecPhase::Idle);
        if phase != RecPhase::Recording {
            let _ = app.emit("recording-target-switch-failed", "recording-target-switch-not-supported");
            return Err("recording-target-switch-not-supported".to_string());
        }
    }
    let new_hwnd = target_id as isize;
    // The window's OWN rect only names its corner -- what target_fits_canvas
    // needs is the display it sits on, the same shape monitor_rect_at already
    // gives window_canvas_candidates for the "current display" tier.
    let new_monitor = external_window_rect(new_hwnd).and_then(|(l, t, _, _)| monitor_rect_at(l, t));
    // spawn_blocking cannot borrow the request-scoped State -- re-fetched from
    // the cloned, 'static AppHandle instead, same as stop_recording_now does.
    let app2 = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let rstate = app2.state::<RecordingState>();
        let mut guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
        let session = guard.as_mut().ok_or_else(|| "no active recording".to_string())?;
        recording::switch_target(session, new_hwnd, new_monitor)
    })
    .await
    .map_err(|e| e.to_string())?;
    match &result {
        Err(e) => {
            let _ = app.emit("recording-target-switch-failed", e);
        }
        Ok(()) => {
            // The control bar follows (and auto-stops with) the window it was
            // started for: it has to move on to the new one, or closing the old
            // window would end a recording that no longer shows it.
            *REC_BAR_TARGET.lock().unwrap_or_else(|e| e.into_inner()) = ("window".to_string(), new_hwnd);
            let following = REC_BAR_FOLLOWING.load(Ordering::SeqCst);
            spawn_rec_bar_follow(app.clone(), new_hwnd);
            REC_BAR_FOLLOWING.store(following, Ordering::SeqCst);
            if following {
                if let Some(win) = app.get_webview_window("rec-bar") {
                    position_rec_bar(&win, "window", new_hwnd);
                }
            }
        }
    }
    result
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) async fn switch_recording_target(app: AppHandle, rstate: State<'_, RecordingState>, target_kind: String, target_id: i64) -> Result<(), String> {
    let _ = (app, rstate, target_kind, target_id);
    Err("recording is only available on Windows".to_string())
}

#[cfg(windows)]
pub(crate) enum ContinueOutcome {
    Continued(Box<recording::RecSession>, RecordingContinuedPayload),
    // The switch itself failed after the old file was already closed for
    // good -- there is no way back to "still recording" from there, so this
    // names the finished take instead of a failed no-op (the String is the
    // error the command itself still returns to the caller).
    StoppedInstead(String),
}

// The "finish and continue" button: closes the running file exactly like
// Stop does, then immediately opens the next one with the SAME target, audio
// sources and canvas/resolution settings -- so a long recording can be split
// into playable parts without setting anything up again. No target/settings
// parameters are taken at all: both come straight off the session that is
// being closed (RecSession.target, kept current across a live target switch;
// the rest exactly as resolve_recording_settings resolved it at that
// session's own start), never re-read from the store, so a value the user
// changes mid-recording only takes effect starting at the NEXT recording,
// same as before this button existed.
#[cfg(windows)]
#[tauri::command]
pub(crate) async fn finish_and_continue_recording(
    app: AppHandle,
    rstate: State<'_, RecordingState>,
    db: State<'_, Db>,
) -> Result<RecordingContinuedPayload, String> {
    // Read once, same as start_recording -- keyOverlay's setting is not
    // re-resolved off the closing session (it carries no copy of it), and
    // restarting capture for the continuation part needs it.
    let (key_overlay_on, key_overlay_hold_ms, key_overlay_shortcuts_only) = {
        let g = db.lock().unwrap_or_else(|e| e.into_inner());
        (keylog::enabled(&g.settings.ui_flags), keylog::HOLD_MS, keylog::shortcuts_only(&g.settings.ui_flags))
    };
    let session = {
        let mut guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
        let s = guard.take().ok_or_else(|| "no active recording".to_string())?;
        if !rec_can_finish(s.phase) {
            *guard = Some(s);
            return Err("nothing to stop".to_string());
        }
        // The riskiest moment for a file switch is right after the encoder
        // has barely begun -- the frontend keeps this same button dead for
        // the first five seconds too; this is the backend's own refusal in
        // case that guard is ever bypassed (a stale button state, a direct
        // call).
        if now_ms().saturating_sub(s.started_at_ms) < 5_000 {
            *guard = Some(s);
            return Err("recording-too-young-to-continue".to_string());
        }
        // The state stays empty for the whole stop-and-restart below, which looks
        // idle to a start request: holding the start claim until the new session
        // is published keeps a second capture from starting in that gap.
        if REC_STARTING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            *guard = Some(s);
            return Err("recording-busy".to_string());
        }
        s
    };
    let _rec_starting_guard = RecStartingGuard;
    // Every part after the first keeps the very FIRST file's own name stem,
    // never the previous part's already-numbered one -- read once, before
    // this call's own finished part is pushed further down.
    let base_path = {
        let parts = REC_SESSION_PARTS.lock().unwrap_or_else(|e| e.into_inner());
        match parts.first() {
            Some(p) => PathBuf::from(p.path.as_str()),
            None => session.output_path.clone(),
        }
    };
    let target = session.target;
    let (show_cursor, audio, video_kbps, video_fps, canvas_mode, canvas_ceiling, disk_warn_mb, max_min, max_mb) = (
        session.show_cursor, session.audio_config.clone(), session.video_kbps, session.video_fps,
        session.canvas_mode.clone(), session.canvas_ceiling, session.disk_warn_mb, session.max_min, session.max_mb,
    );
    let app2 = app.clone();
    let closure_result: Result<ContinueOutcome, String> = tauri::async_runtime::spawn_blocking(move || {
        let switch_start = std::time::Instant::now();
        // Mark the switch instant before touching the old capture at all --
        // every key from here on belongs to the switch itself or to the part
        // about to open, never to the part that is about to close. Fixing
        // this at the exact instant it happens (rather than after
        // recording::stop returns, which can take a moment to flush the
        // encoder) is what keeps the old part's own last keys from bleeding
        // in and the new part's clock from starting late.
        let switch_instant_ms = keylog::mono_ms();
        keylog::cut_off_at(switch_instant_ms);
        let (path, dropped, encode_error, mic_wav_path, canvas_note, crop_rect, canvas_size, resize_crop_at_ms) =
            recording::stop(session)?;
        let needs_crop = crop_rect.is_some_and(|(x, y, w, h)| x > 0 || y > 0 || w < canvas_size.0 || h < canvas_size.1);
        let part_number = REC_SESSION_PARTS.lock().unwrap_or_else(|e| e.into_inner()).len() as u32 + 2;
        let out_path = recording::continuation_output_path(&base_path, part_number);
        match recording::start(
            target, out_path.clone(), audio, &canvas_mode,
            recording::RecStartOptions { show_cursor, video_kbps, video_fps, canvas_ceiling, disk_warn_mb, max_min, max_mb },
        ) {
            Ok(new_session) => {
                // The new part's own clock zero point: the instant its
                // capture actually came up, not `switch_instant_ms` (that
                // one only marks when the OLD capture was cut off, taken
                // BEFORE recording::stop -- encoder/capture setup inside
                // recording::start above can easily take longer than that,
                // which used to make every part-2-onward sidecar's offsets
                // early by the whole switch duration). Keys typed in
                // between the two instants are never captured by either
                // side at all: the old capture is cut off, and this one's
                // hook does not exist until maybe_start below installs it.
                let new_start_ms = keylog::mono_ms();
                // Close out the part that just finished -- its own
                // sidecar, timed from ITS OWN start -- before the new capture
                // for the continuation part begins. The old code never
                // stopped/restarted keylog across this switch at all: the one
                // hook+consumer pair (maybe_start is a no-op while a capture
                // is already running) just kept going, so every earlier
                // part's sidecar was empty and the very LAST part's sidecar
                // ended up holding the whole chain's keys, timed from the
                // FIRST part's start.
                keylog::maybe_stop_and_write(&path);
                keylog::maybe_start(key_overlay_on, key_overlay_hold_ms, new_start_ms, key_overlay_shortcuts_only);
                // The switch worked -- this part is now an interim one with no
                // result dialog of its own, so its crop (if any) runs right away
                // in the background instead of waiting on
                // PENDING_REC_CROP/REC_CROP_CANCEL, which stay reserved for the
                // take's FINAL file and its own visible cancel button. A real,
                // per-job cancel flag (not "never cancelled") lets a whole-take
                // cancel stop this cleanly instead of racing its own delete
                // against ffmpeg still reading the very file being removed.
                if needs_crop {
                    if let Some(ffmpeg) = ffmpeg_search_path(&app2) {
                        let crop_path = path.clone();
                        let rect = crop_rect.unwrap();
                        let cancel_flag = Arc::new(AtomicBool::new(false));
                        let cancel_flag2 = cancel_flag.clone();
                        let spawned = std::thread::Builder::new().name("rec-continue-crop".into()).spawn(move || {
                            let _ = crop_recording_blocking(&ffmpeg, &crop_path, rect, move || cancel_flag2.load(Ordering::SeqCst), |_| {});
                            let me = std::thread::current().id();
                            REC_INTERIM_CROPS.lock().unwrap_or_else(|e| e.into_inner()).retain(|(_, h)| h.thread().id() != me);
                        });
                        match spawned {
                            Ok(handle) => {
                                // The thread removes its own entry when it ends, which can
                                // happen before this push: finished ones are dropped here
                                // so none stays in the list for good.
                                let mut crops = REC_INTERIM_CROPS.lock().unwrap_or_else(|e| e.into_inner());
                                crops.retain(|(_, h)| !h.is_finished());
                                if !handle.is_finished() {
                                    crops.push((cancel_flag, handle));
                                }
                            }
                            Err(_) => append_crash_log("recording: could not start the interim-part crop thread"),
                        }
                    }
                }
                REC_SESSION_PARTS.lock().unwrap_or_else(|e| e.into_inner()).push(RecordingPartInfo {
                    path: path.to_string_lossy().to_string(),
                    mic_wav_path: mic_wav_path.map(|p| p.to_string_lossy().to_string()),
                });
                Ok(ContinueOutcome::Continued(Box::new(new_session), RecordingContinuedPayload {
                    finished_part_path: path.to_string_lossy().to_string(),
                    finished_part_dropped_frames: dropped,
                    finished_part_encode_error: encode_error,
                    new_path: out_path.to_string_lossy().to_string(),
                    part_number,
                    gap_ms: switch_start.elapsed().as_millis() as u64,
                }))
            }
            Err(start_err) => {
                // This file becomes the take's final one after all -- same
                // crop/result path a real stop uses, with every earlier part
                // folded in. Same for the key-overlay sidecar.
                keylog::maybe_stop_and_write(&path);
                if needs_crop {
                    let mut pending = PENDING_REC_CROP.lock().unwrap_or_else(|e| e.into_inner());
                    *pending = Some((path.clone(), crop_rect.unwrap()));
                }
                let parts = std::mem::take(&mut *REC_SESSION_PARTS.lock().unwrap_or_else(|e| e.into_inner()));
                let payload = RecordingStoppedPayload {
                    path: path.to_string_lossy().to_string(),
                    dropped_frames: dropped,
                    encode_error,
                    mic_wav_path: mic_wav_path.map(|p| p.to_string_lossy().to_string()),
                    canvas_note: canvas_note.map(str::to_string),
                    needs_crop,
                    resize_crop_at_ms,
                    parts,
                };
                let _ = app2.emit("recording-captured", payload);
                Ok(ContinueOutcome::StoppedInstead(format!("recording-continue-failed-stopped:{start_err}")))
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    match closure_result {
        Err(e) => {
            // recording::stop itself failed -- practically unreachable (it
            // folds every internal failure into encode_error instead), but if
            // it ever does, the old session is already gone either way.
            hide_rec_outline(&app);
            rearm_hotkeys(&app);
            apply_tray_menu(&app);
            Err(e)
        }
        Ok(ContinueOutcome::Continued(new_session, payload)) => {
            *rstate.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(*new_session);
            Ok(payload)
        }
        Ok(ContinueOutcome::StoppedInstead(err)) => {
            // Nothing is recording any more, so the region outline goes too.
            hide_rec_outline(&app);
            rearm_hotkeys(&app);
            apply_tray_menu(&app);
            sweep_recordings_and_notify(&app);
            Err(err)
        }
    }
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) async fn finish_and_continue_recording(app: AppHandle, rstate: State<'_, RecordingState>, db: State<'_, Db>) -> Result<RecordingContinuedPayload, String> {
    let _ = (app, rstate, db);
    Err("recording is only available on Windows".to_string())
}

#[cfg(windows)]
#[tauri::command]
pub(crate) fn pause_recording(app: AppHandle, rstate: State<'_, RecordingState>) -> Result<(), String> {
    {
        let mut guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
        let session = guard.as_mut().ok_or_else(|| "no active recording".to_string())?;
        if !rec_can_pause(session.phase) {
            return Err("not recording".to_string());
        }
        do_pause_recording(session);
    }
    apply_tray_menu(&app); // the tray's Pause row switches to Resume
    Ok(())
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) fn pause_recording(app: AppHandle, rstate: State<'_, RecordingState>) -> Result<(), String> {
    let _ = (app, rstate);
    Err("recording is only available on Windows".to_string())
}

#[cfg(windows)]
#[tauri::command]
pub(crate) fn resume_recording(app: AppHandle, rstate: State<'_, RecordingState>) -> Result<(), String> {
    {
        let mut guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
        let session = guard.as_mut().ok_or_else(|| "no active recording".to_string())?;
        if !rec_can_resume(session.phase) {
            return Err("not paused".to_string());
        }
        do_resume_recording(session);
    }
    apply_tray_menu(&app); // the tray's Resume row switches back to Pause
    Ok(())
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) fn resume_recording(app: AppHandle, rstate: State<'_, RecordingState>) -> Result<(), String> {
    let _ = (app, rstate);
    Err("recording is only available on Windows".to_string())
}

// Mid-recording mute for either source, from the rec bar's own two audio
// buttons. `source` is "system" or "mic"; muting a source that was never
// active this session is a no-op (there is no queue to flip).
#[cfg(windows)]
#[tauri::command]
pub(crate) fn set_audio_muted(rstate: State<'_, RecordingState>, source: String, muted: bool) -> Result<(), String> {
    let guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
    let session = guard.as_ref().ok_or_else(|| "no active recording".to_string())?;
    let queue = match source.as_str() {
        "system" => &session.system_queue,
        "mic" => &session.mic_queue,
        _ => return Err(format!("unknown audio source: {source}")),
    };
    match queue {
        Some(q) => {
            q.set_muted(muted);
            Ok(())
        }
        None => Err(format!("{source} audio is not active this recording")),
    }
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) fn set_audio_muted(rstate: State<'_, RecordingState>, source: String, muted: bool) -> Result<(), String> {
    let _ = (rstate, source, muted);
    Err("recording is only available on Windows".to_string())
}

// Shared by the Stop-button command below and the auto-stop path (the
// rec-bar's follow thread, when the recorded window itself closes mid-
// recording): keep the file instead of leaving a dead session running
// against a capture source that is already gone.
// Plain blocking call, not async -- the follow thread is already its own
// dedicated OS thread, so there is no tokio worker to protect here; the
// actual command below still off-loads through spawn_blocking for that.
#[cfg(windows)]
pub(crate) fn stop_recording_now(app: &AppHandle) -> Result<RecordingStoppedPayload, String> {
    let rstate = app.state::<RecordingState>();
    let session = {
        let mut guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
        let s = guard.take().ok_or_else(|| "no active recording".to_string())?;
        if !rec_can_finish(s.phase) {
            *guard = Some(s);
            return Err("nothing to stop".to_string());
        }
        s
    };
    // The outline window (region recordings only, see show_rec_outline) goes
    // the moment the session is taken, on every end path this function serves
    // (button, hotkey, auto-stop), even one where recording::stop fails below.
    hide_rec_outline(app);
    let (path, dropped, encode_error, mic_wav_path, canvas_note, crop_rect, canvas_size, resize_crop_at_ms) = recording::stop(session)?;
    // Stops the key-overlay hook (if it was running at all) and writes
    // its sidecar next to this recording.
    keylog::maybe_stop_and_write(&path);
    // A crop is only worth running when the union of every placed frame is
    // strictly smaller than the canvas it was placed on -- a window that
    // used the whole canvas leaves nothing to trim.
    let needs_crop = crop_rect.is_some_and(|(x, y, w, h)| x > 0 || y > 0 || w < canvas_size.0 || h < canvas_size.1);
    if needs_crop {
        let mut pending = PENDING_REC_CROP.lock().unwrap_or_else(|e| e.into_inner());
        *pending = Some((path.clone(), crop_rect.unwrap()));
    }
    // Every earlier part a "finish and continue" chain already closed --
    // empty for a plain recording, since nothing but that button ever adds
    // to this list. Taken (not just read) so the next recording starts clean
    // even if start_recording's own clear were ever skipped.
    let parts = std::mem::take(&mut *REC_SESSION_PARTS.lock().unwrap_or_else(|e| e.into_inner()));
    let payload = RecordingStoppedPayload {
        path: path.to_string_lossy().to_string(),
        dropped_frames: dropped,
        encode_error,
        mic_wav_path: mic_wav_path.map(|p| p.to_string_lossy().to_string()),
        canvas_note: canvas_note.map(str::to_string),
        needs_crop,
        resize_crop_at_ms,
        parts,
    };
    // The stop/pause hotkeys give up their combination the instant nothing
    // is recording, on every end path this function serves (button, hotkey,
    // auto-stop). Same for the tray's two extra rows.
    rearm_hotkeys(app);
    apply_tray_menu(app);
    // The moment a previous crash's leftover is provably not the file
    // that just finished -- this recording is gone from RecordingState by now.
    sweep_recordings_and_notify(app);
    // Mirrors the screenshot flow's "snip-captured": the recording bar is its
    // own window, so the main window learns the recording is ready to attach
    // through this event rather than a direct call across windows.
    let _ = app.emit("recording-captured", payload.clone());
    Ok(payload)
}

#[cfg(windows)]
#[tauri::command]
pub(crate) async fn stop_recording(app: AppHandle, rstate: State<'_, RecordingState>) -> Result<RecordingStoppedPayload, String> {
    let _ = &rstate; // accessed via the AppHandle inside stop_recording_now, shared with the auto-stop path
    tauri::async_runtime::spawn_blocking(move || stop_recording_now(&app))
        .await
        .map_err(|e| e.to_string())?
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) async fn stop_recording(app: AppHandle, rstate: State<'_, RecordingState>) -> Result<RecordingStoppedPayload, String> {
    let _ = (app, rstate);
    Err("recording is only available on Windows".to_string())
}

#[cfg(windows)]
#[tauri::command]
pub(crate) async fn cancel_recording(app: AppHandle, rstate: State<'_, RecordingState>) -> Result<(), String> {
    let session = {
        let mut guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
        let s = guard.take().ok_or_else(|| "no active recording".to_string())?;
        if !rec_can_finish(s.phase) {
            *guard = Some(s);
            return Err("nothing to cancel".to_string());
        }
        s
    };
    // Same as a normal stop: the outline window (if this was a region
    // recording) never survives a cancel, even one whose cleanup fails below.
    hide_rec_outline(&app);
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        recording::cancel(session)?;
        // A cancelled take never gets a key-overlay sidecar either.
        keylog::discard();
        // A cancel discards the WHOLE take, not just its current file -- every
        // earlier part a "finish and continue" chain already closed goes with
        // it. Every interim crop still running against one of those files is
        // signalled and joined FIRST, so deleting them can never race an
        // ffmpeg process still reading the very file being removed (a delete
        // that fails because the file is still open, or one that "succeeds"
        // only for the crop's own rename to recreate it right after).
        for (flag, handle) in std::mem::take(&mut *REC_INTERIM_CROPS.lock().unwrap_or_else(|e| e.into_inner())) {
            flag.store(true, Ordering::SeqCst);
            let _ = handle.join();
        }
        // Best-effort from here: a missing file (already moved/deleted by the
        // user) is not a reason to fail an otherwise-successful cancel.
        for part in std::mem::take(&mut *REC_SESSION_PARTS.lock().unwrap_or_else(|e| e.into_inner())) {
            remove_recording_part_files(&part);
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    // Give the stop/pause combination back up now that nothing is recording,
    // and drop the tray's two extra rows.
    rearm_hotkeys(&app);
    apply_tray_menu(&app);
    sweep_recordings_and_notify(&app); // same moment as a normal stop, see above
    // The composer's record button has no other way to learn a cancel
    // happened (that click lives in the rec-bar, a different window) --
    // symmetric with stop's own "recording-captured".
    let _ = app.emit("recording-cancelled", ());
    Ok(())
}
#[cfg(not(windows))]
#[tauri::command]
pub(crate) async fn cancel_recording(app: AppHandle, rstate: State<'_, RecordingState>) -> Result<(), String> {
    let _ = (app, rstate);
    Err("recording is only available on Windows".to_string())
}
