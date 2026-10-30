// Camera recording. A getUserMedia preview window whose stream is encoded in the page by
// MediaRecorder (webm, VP8+Opus: the one container every WebView2 release plays back the
// same way) and handed to this module only as finished bytes.
//
// The one piece that has to live in Rust is the WebView2 permission prompt: nothing answers
// it on its own, so a plain getUserMedia call would hang with no error.
// install_permission_handler is that answer, for camera and microphone only, in this one
// window.

use crate::images::base64_decode;
use crate::{lock, recording_dir, sidecar_path, Db, RecordingState, CAM_SIDECAR_EXT};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub struct CamSession {
    pub output_path: PathBuf,
    // The screen recording active when this take began, if any, carried to the finish result
    // so the frontend can decide on a dual-open (camera overlay on that recording) without a
    // second round trip (it may have stopped by then). The alignment offset is not stored
    // here: camera_screen_elapsed_ms reads it live when the camera actually starts.
    pub companion_path: Option<PathBuf>,
    // Set by camera_mark_finishing the instant camera.js's onstop handler begins, before it
    // awaits the chunks still in flight. Without it, Alt+F4, taskbar close or an app quit in
    // that window looks like an abandoned session and deletes a take that is practically done.
    // Carries the two values camera_finish_recording takes, so the window-destroyed handler can
    // finish the sidecar itself when the graceful IPC call never runs.
    pub finishing: Option<(u64, Option<u64>)>,
}

pub struct CameraState(pub Mutex<Option<CamSession>>);

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
pub struct CamSidecar {
    pub start_ms: u64,
    // The screen recording's pause-aware active-timeline position when this camera take
    // started: already the final overlay offset, not a wall-clock value. None when no screen
    // recording was running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay_start_ms: Option<u64>,
}

// Sits next to the recording with its own extension, like the key-overlay sidecar (shared
// sidecar_path), so Save/Copy/discard moving or dropping a `.keys.json` needs the same
// one-line addition for `.cam.json`. The mirror of rec_engine.rs's mic_sidecar_video_path:
// a `.cam.json` next to a video the library still references must count as referenced, or
// the sweep removes it as an orphan after an hour. The camera's container is webm, never
// mp4, so both are tried.
pub fn sidecar_video_path(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".cam.json")?;
    for ext in ["webm", "mp4"] {
        let candidate = path.with_file_name(format!("{stem}.{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

// rec-cam-YYYY-MM-DD_HHMMSS.<ext>, with a "-2", "-3", ... suffix only when the plain name
// is taken (two takes in the same second, or a restart after a file was not cleaned up).
// It is the screen recorder's rec_filename shape plus one segment, so a folder listing
// sorts the two together and both pass the "rec-" prefix discard_recording_file requires
// before deleting. n=0 is the plain name.
pub fn cam_filename(when: (i32, u32, u32, u32, u32, u32), ext: &str, n: u32) -> String {
    let (year, month, day, hour, minute, second) = when;
    if n == 0 {
        format!("rec-cam-{year:04}-{month:02}-{day:02}_{hour:02}{minute:02}{second:02}.{ext}")
    } else {
        format!("rec-cam-{year:04}-{month:02}-{day:02}_{hour:02}{minute:02}{second:02}-{}.{ext}", n + 1)
    }
}

#[cfg(windows)]
fn local_time_parts() -> (i32, u32, u32, u32, u32, u32) {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let st = unsafe { GetLocalTime() };
    (st.wYear as i32, st.wMonth as u32, st.wDay as u32, st.wHour as u32, st.wMinute as u32, st.wSecond as u32)
}

// Claims a filename atomically (create_new never truncates an existing file) and tries the
// next suffixed name on a collision instead of overwriting a same-second take.
fn create_unique(dir: &Path, mut name_for: impl FnMut(u32) -> String) -> std::io::Result<PathBuf> {
    for n in 0..1000 {
        let path = dir.join(name_for(n));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no free camera recording filename"))
}

#[derive(Serialize)]
pub struct CamBeginResult {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    companion_path: Option<String>,
}



// A screen recording counts as in flight while it has a session OR while a start/stop/cancel
// holds the start claim (the session is out of the state during the teardown).
fn screen_recording_in_flight(has_session: bool, start_claim_held: bool) -> bool {
    has_session || start_claim_held
}

// Pure, so the dual-open decision has its own test: a companion still recording is
// mid-write, not a file to open, and one with no path (or a vanished file) has nothing to
// align against.
fn resolve_finish_companion(
    companion: Option<PathBuf>,
    screen_still_active: bool,
    file_exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if screen_still_active {
        return None;
    }
    companion.filter(|p| file_exists(p))
}

// The screen recording's pause-aware active-timeline position, read live when
// MediaRecorder actually starts (camera.js calls this right before recorder.start()). A
// wall-clock difference cannot stand in: a pause in the screen recording advances
// wall-clock time but not its output timeline, overstating the offset. The returned value
// already is the overlay's start offset (written straight into the sidecar by
// camera_finish_recording, read back by addAlignedCameraOverlay in ui/videoOverlays.js).
#[tauri::command]
pub fn camera_screen_elapsed_ms(rstate: State<'_, RecordingState>) -> Option<u64> {
    #[cfg(windows)]
    {
        let guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(|s| crate::recording::elapsed_ms(s).max(0) as u64)
    }
}

// Starts a new camera take: picks the file name and remembers whether a screen recording
// is running (and its start) so the finish result can decide on a dual-open. The camera's
// own start comes from the caller in camera_finish_recording, when MediaRecorder really
// started.
#[tauri::command]
pub async fn camera_begin_recording(
    db: State<'_, Db>,
    rstate: State<'_, RecordingState>,
    cam: State<'_, CameraState>,
    ext: String,
) -> Result<CamBeginResult, String> {
    if ext != "webm" && ext != "mp4" {
        return Err("unsupported container".to_string());
    }
    let custom = lock(&db).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    let dir = recording_dir(&custom);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (y, mo, d, h, mi, s) = local_time_parts();
    let companion_path = {
        #[cfg(windows)]
        {
            let guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
            guard.as_ref().map(|s| s.output_path.clone())
        }
    };
    // Checked and filled under one lock, so a second begin can never replace a live session
    // (and orphan its file) in between.
    let mut slot = cam.0.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_some() {
        return Err("a camera recording is already active".to_string());
    }
    let path = create_unique(&dir, |n| cam_filename((y, mo, d, h, mi, s), &ext, n)).map_err(|e| e.to_string())?;
    *slot = Some(CamSession { output_path: path.clone(), companion_path: companion_path.clone(), finishing: None });
    Ok(CamBeginResult {
        path: path.to_string_lossy().to_string(),
        companion_path: companion_path.map(|p| p.to_string_lossy().to_string()),
    })
}

// Removes and returns the active session only when `path` is exactly its
// output file. A stale or forged path leaves the live session untouched, so a
// later correct call still finds it.
fn take_matching(slot: &mut Option<CamSession>, path: &Path) -> Result<CamSession, String> {
    match slot.as_ref() {
        None => Err("no active camera recording".to_string()),
        Some(s) if s.output_path != path => Err("path does not match the active camera recording".to_string()),
        Some(_) => Ok(slot.take().expect("session checked above")),
    }
}

// MediaRecorder hands the page a Blob in timesliced chunks; each is appended as it arrives
// so a long take never sits in renderer memory. Bytes travel as base64 (camera.js encodes,
// this decodes): about a third bigger than raw, instead of three to four times for a JSON
// number array.
// base64_decode answers malformed input with an empty Vec, so an empty result from non-empty
// input is the error; an empty chunk stays a valid no-op.
pub(crate) fn decode_camera_chunk(b64: &str) -> Result<Vec<u8>, String> {
    let bytes = base64_decode(b64);
    if bytes.is_empty() && !b64.trim().is_empty() {
        return Err("invalid camera chunk".to_string());
    }
    Ok(bytes)
}

#[tauri::command]
pub async fn camera_append_chunk(cam: State<'_, CameraState>, path: String, bytes_b64: String) -> Result<(), String> {
    // Decoded before the lock is taken: the synchronous finish commands wait on it.
    let bytes = decode_camera_chunk(&bytes_b64)?;
    // The lock covers the session check only, not the disk write: a slow target (a share, a
    // sleeping disk) must not hold up the finish commands that wait on this lock. Opening
    // without `create` fails cleanly when a racing discard already removed the file.
    let output_path = {
        let guard = cam.0.lock().unwrap_or_else(|e| e.into_inner());
        let session = guard.as_ref().ok_or_else(|| "no active camera recording".to_string())?;
        if session.output_path != Path::new(&path) {
            return Err("path does not match the active camera recording".to_string());
        }
        session.output_path.clone()
    };
    let mut f = OpenOptions::new().append(true).open(&output_path).map_err(|e| e.to_string())?;
    f.write_all(&bytes).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct CamFinishResult {
    path: String,
    // Only set when the companion screen recording has already stopped by the time this take
    // finishes: one still recording is mid-write, and the editor's dual-open must wait for a
    // playable file. The alignment offset does not travel here; it lives in the camera's
    // sidecar (see camera_finish_recording), which the editor reads directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    companion_path: Option<String>,
}

// Writes the sidecar for a session that is finishing one way or another: shared by
// camera_finish_recording's graceful path and the window-destroyed fallback (see
// CamSession::finishing). cam_start_ms is the real MediaRecorder start (Date.now() in
// camera.js when recorder.start() ran), not a begin-time guess. screen_elapsed_ms is
// camera_screen_elapsed_ms's value read at that same moment: the final, pause-aware
// overlay offset, written straight into the sidecar. A failed write is returned: without the
// sidecar the editor would silently align the overlay at offset 0.
pub(crate) fn finalize_session(session: &CamSession, cam_start_ms: u64, screen_elapsed_ms: Option<u64>) -> Result<(), String> {
    let sidecar = CamSidecar { start_ms: cam_start_ms, overlay_start_ms: screen_elapsed_ms };
    let json = serde_json::to_string(&sidecar).map_err(|e| e.to_string())?;
    fs::write(sidecar_path(&session.output_path, CAM_SIDECAR_EXT), json).map_err(|e| {
        // The take is out of its slot and now unreferenced: the abandoned marker keeps the
        // recordings sweep from deleting a finished recording the user was told is kept.
        let _ = fs::write(abandoned_marker_path(&session.output_path), b"");
        e.to_string()
    })
}

#[tauri::command(async)]
pub fn camera_finish_recording(
    cam: State<'_, CameraState>,
    rstate: State<'_, RecordingState>,
    path: String,
    cam_start_ms: u64,
    screen_elapsed_ms: Option<u64>,
) -> Result<CamFinishResult, String> {
    let session = take_matching(&mut cam.0.lock().unwrap_or_else(|e| e.into_inner()), Path::new(&path))?;
    finalize_session(&session, cam_start_ms, screen_elapsed_ms)?;
    let screen_still_active = {
        #[cfg(windows)]
        {
            // The stop and cancel commands take the session out of the state BEFORE the encoder is
            // finalised (or the file moved to the discard area), and hold the start claim until
            // they are done: until then the file is not a recording to open.
            screen_recording_in_flight(
                rstate.0.lock().unwrap_or_else(|e| e.into_inner()).is_some(),
                crate::rec_cmds::REC_STARTING.load(std::sync::atomic::Ordering::SeqCst),
            )
        }
    };
    let companion_path = resolve_finish_companion(session.companion_path, screen_still_active, |p| p.is_file());
    Ok(CamFinishResult {
        path: session.output_path.to_string_lossy().to_string(),
        companion_path: companion_path.map(|p| p.to_string_lossy().to_string()),
    })
}

// Pure, so a mismatched path being ignored (like a finish/discard racing a session that
// moved on) is testable without a Tauri harness.
fn mark_finishing(session: &mut Option<CamSession>, path: &Path, cam_start_ms: u64, screen_elapsed_ms: Option<u64>) {
    if let Some(s) = session.as_mut() {
        if s.output_path == path {
            s.finishing = Some((cam_start_ms, screen_elapsed_ms));
        }
    }
}

// Called the instant onstop begins, before camera.js awaits the chunks in flight: marks the
// session as finishing, so a window destroyed meanwhile finishes the take instead of
// deleting it (see CamSession::finishing and the Destroyed handler in open_camera_window).
#[tauri::command]
pub fn camera_mark_finishing(cam: State<'_, CameraState>, path: String, cam_start_ms: u64, screen_elapsed_ms: Option<u64>) {
    let mut guard = cam.0.lock().unwrap_or_else(|e| e.into_inner());
    mark_finishing(&mut guard, Path::new(&path), cam_start_ms, screen_elapsed_ms);
}

// The cancel path: bytes on disk are discarded with the slot, like keylog::discard,
// leaving neither a half-written video nor a sidecar.
#[tauri::command]
pub fn camera_discard_recording(cam: State<'_, CameraState>, path: String) -> Result<(), String> {
    let session = take_matching(&mut cam.0.lock().unwrap_or_else(|e| e.into_inner()), Path::new(&path))?;
    discard_session(&session);
    Ok(())
}

// Sits next to a take whose recording failed part way but whose partial file
// is kept for the user. The recordings sweep skips both the marker and the
// video it names, however old, so "the raw file was kept" stays true.
pub fn abandoned_marker_path(output_path: &Path) -> PathBuf {
    output_path.with_extension("abandoned")
}

pub fn is_kept_abandoned(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "abandoned") || abandoned_marker_path(path).is_file()
}

// Releases the session WITHOUT touching the file: used when the page knows a
// take is damaged (a chunk write failed) but the partial file may still be the
// only copy of the recording. No delete, no sidecar.
#[tauri::command(async)]
pub fn camera_abandon_recording(cam: State<'_, CameraState>, path: String) -> Result<(), String> {
    let session = take_matching(&mut cam.0.lock().unwrap_or_else(|e| e.into_inner()), Path::new(&path))?;
    // The marker tells the recordings sweep to leave this damaged take alone
    // (it would otherwise age out as an unreferenced file).
    fs::write(abandoned_marker_path(&session.output_path), b"").map_err(|e| e.to_string())
}

fn discard_session(session: &CamSession) {
    let _ = fs::remove_file(&session.output_path);
    let _ = fs::remove_file(sidecar_path(&session.output_path, CAM_SIDECAR_EXT));
}

// What a window that went away under a live session leaves behind. The close button, Escape and
// the stop button keep a running take, so Alt+F4, the taskbar close and an app quit must too: a
// take with bytes on disk is kept with the abandoned marker (the recordings sweep would
// otherwise delete it as unreferenced after an hour), and one that was already finishing also
// gets its sidecar. Only a reserved file that never received a byte is removed.
fn settle_session_on_close(session: &CamSession) {
    let has_bytes = fs::metadata(&session.output_path).is_ok_and(|m| m.len() > 0);
    if !has_bytes {
        discard_session(session);
        return;
    }
    if let Some((cam_start_ms, screen_elapsed_ms)) = session.finishing {
        // Nobody is left to tell: the window is already gone.
        let _ = finalize_session(session, cam_start_ms, screen_elapsed_ms);
    }
    let _ = fs::write(abandoned_marker_path(&session.output_path), b"");
}

// The files of the live camera take, for the recordings sweep: it sits unreferenced for as long
// as the take runs, and a long pause moves no chunks, so its modification time stops too.
pub(crate) fn live_session_paths(slot: &Option<CamSession>) -> Vec<PathBuf> {
    match slot {
        Some(s) => vec![s.output_path.clone(), sidecar_path(&s.output_path, CAM_SIDECAR_EXT)],
        None => Vec::new(),
    }
}

pub fn read_sidecar(path: &Path) -> Option<CamSidecar> {
    let data = fs::read_to_string(sidecar_path(path, CAM_SIDECAR_EXT)).ok()?;
    serde_json::from_str(&data).ok()
}

// The editor's counterpart to keylog's read_key_sidecar: the durable source of truth for a
// camera take's start, read by addAlignedCameraOverlay in ui/videoOverlays.js.
#[tauri::command]
pub fn read_camera_sidecar(path: String) -> Option<CamSidecar> {
    read_sidecar(Path::new(&path))
}

// ---------- window + WebView2 permission handling ----------

const CAMERA_WIN_LABEL: &str = "camera";
const PERMISSION_READY_EVENT: &str = "camera-permission-ready";

// True once the permission handler is attached to the open camera window. The one-shot event
// can fire before the page has registered its listener, so the page also asks for this flag
// after registering and never has to guess with a timer.
static CAM_PERMISSION_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub fn camera_permission_ready() -> bool {
    CAM_PERMISSION_READY.load(std::sync::atomic::Ordering::SeqCst)
}

// True when the handler could not be attached after the window opened (WebView2 refused the
// core object or the subscription). Same one-shot-event-plus-flag shape as the ready answer, so
// the page can say so instead of sitting on an empty preview.
static CAM_PERMISSION_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const PERMISSION_FAILED_EVENT: &str = "camera-permission-failed";

#[tauri::command]
pub fn camera_permission_failed() -> bool {
    CAM_PERMISSION_FAILED.load(std::sync::atomic::Ordering::SeqCst)
}

fn report_permission_failure(app: &AppHandle) {
    CAM_PERMISSION_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = app.emit_to(CAMERA_WIN_LABEL, PERMISSION_FAILED_EVENT, ());
}

#[tauri::command]
pub async fn open_camera_window(app: AppHandle, db: State<'_, Db>) -> Result<(), String> {
    let enabled = lock(&db).settings.ui_flags.get("cameraRecord") == Some(&true);
    if !enabled {
        return Err("cameraRecord is off".to_string());
    }
    if let Some(win) = app.get_webview_window(CAMERA_WIN_LABEL) {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }
    let excluded = {
        let store = lock(&db);
        crate::capture_excluded(&store.settings)
    };
    // A new window starts unanswered, whatever the previous one reached. Reset before the build,
    // so its page can never read the previous window's answer.
    CAM_PERMISSION_READY.store(false, std::sync::atomic::Ordering::SeqCst);
    CAM_PERMISSION_FAILED.store(false, std::sync::atomic::Ordering::SeqCst);
    let win = WebviewWindowBuilder::new(&app, CAMERA_WIN_LABEL, WebviewUrl::App("camera.html".into()))
        .title(crate::brand::DISPLAY_NAME)
        .inner_size(420.0, 620.0)
        .min_inner_size(360.0, 520.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .resizable(true)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    crate::set_capture_exclusion(&win, excluded);
    // Without its handler this window must not survive: the next open would just show it.
    if let Err(e) = install_permission_handler(&app, &win) {
        let _ = win.destroy();
        return Err(e);
    }
    // Alt+F4, the taskbar close or an app quit skip camera.js's close-button handler and would
    // leave a half-written file and an active session. This is the window's last chance to settle
    // the session from its state: the take is kept, never deleted (see settle_session_on_close).
    let cleanup_app = app.clone();
    win.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            // The next window starts unanswered, whatever this one reached.
            CAM_PERMISSION_READY.store(false, std::sync::atomic::Ordering::SeqCst);
            CAM_PERMISSION_FAILED.store(false, std::sync::atomic::Ordering::SeqCst);
            if let Some(state) = cleanup_app.try_state::<CameraState>() {
                let taken = state.0.lock().unwrap_or_else(|e| e.into_inner()).take();
                if let Some(session) = taken {
                    // This handler runs on the event loop: the disk work happens off it, so a
                    // recording folder on a slow or sleeping share cannot stall every window.
                    tauri::async_runtime::spawn_blocking(move || settle_session_on_close(&session));
                }
            }
        }
    });
    let _ = win.show();
    let _ = win.set_focus();
    Ok(())
}

// WebView2's PermissionRequestedEventArgs::Uri is the requesting origin, which depending
// on the runtime arrives bare or as a full URL with the page's path. Tauri's
// WebviewUrl::App resolves every page to a fixed scheme+host, so an exact compare against
// that origin (not `contains`) is what makes this safe: a substring check would block a
// bare origin and accept a look-alike host with "camera.html" somewhere in the URL.
const APP_ORIGINS: [&str; 2] = ["https://tauri.localhost", "http://tauri.localhost"];

fn permission_uri_is_camera_page(uri: &str) -> bool {
    for origin in APP_ORIGINS {
        let Some(rest) = uri.strip_prefix(origin) else { continue };
        // The character after the origin must end it cleanly (a path, query, fragment or nothing);
        // otherwise it was a prefix of a longer host such as
        // "https://tauri.localhost.evil.example/camera.html".
        if !(rest.is_empty() || rest.starts_with(['/', '?', '#'])) {
            return false;
        }
        let path = rest.split(['?', '#']).next().unwrap_or("");
        return path.is_empty() || path == "/" || path == "/camera.html";
    }
    false
}

// Answers the WebView2 camera/microphone permission prompt for this window only: exactly
// those two kinds, from exactly this app's camera page, nothing else. with_webview runs its
// closure asynchronously on the main thread, so camera.js waits for
// camera-permission-ready (emitted once the handler is attached) before calling
// getUserMedia; earlier it would race the unanswered prompt and hang.
#[cfg(windows)]
fn install_permission_handler(app: &AppHandle, win: &tauri::WebviewWindow) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2PermissionRequestedEventArgs3, COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA,
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DENY,
    };
    use webview2_com::PermissionRequestedEventHandler;
    use windows_core::Interface as _;
    let app_ready = app.clone();
    let app_check = app.clone();
    win.with_webview(move |webview| {
        let Ok(core) = (unsafe { webview.controller().CoreWebView2() }) else {
            // No working permission answer: leaving the prompt unanswered is safer than allowing every
            // kind. The page is told, so it does not wait on a camera that will never be asked for.
            report_permission_failure(&app_ready);
            return;
        };
        let mut token = 0i64;
        let handler = PermissionRequestedEventHandler::create(Box::new(move |_sender, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            unsafe { args.PermissionKind(&mut kind)? };
            // Every refusal below (wrong kind, switch off, wrong origin) sets an explicit DENY instead
            // of returning: untouched state lets WebView2 show its own prompt UI, and an acceptance
            // there could be saved into the shared profile.
            let allow = if kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA || kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE {
                // Re-checked on each request, not just at window open: the switch can be turned off while
                // the window is open, and the next getUserMedia call must stop being auto-allowed.
                let enabled = app_check
                    .try_state::<Db>()
                    .map(|db| lock(&db).settings.ui_flags.get("cameraRecord") == Some(&true))
                    .unwrap_or(false);
                if enabled {
                    let mut uri_ptr = windows_core::PWSTR::null();
                    let uri = if unsafe { args.Uri(&mut uri_ptr) }.is_ok() { webview2_com::take_pwstr(uri_ptr) } else { String::new() };
                    permission_uri_is_camera_page(&uri)
                } else {
                    false
                }
            } else {
                false
            };
            let state = if allow { COREWEBVIEW2_PERMISSION_STATE_ALLOW } else { COREWEBVIEW2_PERMISSION_STATE_DENY };
            unsafe { args.SetState(state)? };
            // Never let WebView2 remember this decision in the shared profile: every app window shares
            // one WebView2 environment, so a saved allow or deny could answer a later, different
            // request without this handler (installed on the camera window only) running.
            if let Ok(args3) = args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() {
                unsafe {
                    let _ = args3.SetSavesInProfile(false);
                }
            }
            Ok(())
        }));
        if unsafe { core.add_PermissionRequested(&handler, &mut token) }.is_ok() {
            CAM_PERMISSION_READY.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = app_ready.emit_to(CAMERA_WIN_LABEL, PERMISSION_READY_EVENT, ());
        } else {
            report_permission_failure(&app_ready);
        }
    })
    .map_err(|e| e.to_string())
}


#[cfg(test)]
mod tests {
    // The append command must not run on the main thread (it is called every
    // second with a base64 chunk), and because an async command can overlap with
    // itself, camera.js has to chain the calls so the chunks stay in order.
    #[test]
    fn camera_append_chunk_is_async_and_the_page_orders_its_calls() {
        let rs = include_str!("camera.rs");
        assert!(rs.contains("pub async fn camera_append_chunk("), "camera_append_chunk must be an async command");
        let js = include_str!("../../ui/camera.js");
        assert!(js.contains("const append = appendChain"), "camera.js must start each append after the previous one");
    }

    use super::*;

    #[test]
    fn cam_filename_matches_the_screen_recorder_shape_with_its_own_prefix() {
        assert_eq!(cam_filename((2026, 9, 25, 8, 5, 3), "webm", 0), "rec-cam-2026-09-25_080503.webm");
    }

    #[test]
    fn cam_filename_gets_a_suffix_on_a_collision() {
        assert_eq!(cam_filename((2026, 9, 25, 8, 5, 3), "webm", 1), "rec-cam-2026-09-25_080503-2.webm");
        assert_eq!(cam_filename((2026, 9, 25, 8, 5, 3), "webm", 2), "rec-cam-2026-09-25_080503-3.webm");
    }

    #[test]
    fn create_unique_claims_the_plain_name_when_nothing_is_there() {
        let dir = std::env::temp_dir().join(format!("psc-cam-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let p = create_unique(&dir, |n| cam_filename((2026, 1, 1, 0, 0, 0), "webm", n)).unwrap();
        assert_eq!(p.file_name().unwrap().to_str().unwrap(), "rec-cam-2026-01-01_000000.webm");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_unique_falls_back_to_a_suffix_on_a_collision_without_touching_the_first_file() {
        let dir = std::env::temp_dir().join(format!("psc-cam-test2-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let name_for = |n: u32| cam_filename((2026, 1, 1, 0, 0, 0), "webm", n);
        let first = create_unique(&dir, name_for).unwrap();
        fs::write(&first, b"already recording").unwrap();
        let second = create_unique(&dir, name_for).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(&first).unwrap(), b"already recording");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sidecar_video_path_finds_the_webm_sibling() {
        let dir = std::env::temp_dir().join(format!("psc-cam-test3-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let video = dir.join("rec-cam-1.webm");
        fs::write(&video, b"x").unwrap();
        let sidecar = dir.join("rec-cam-1.cam.json");
        assert_eq!(sidecar_video_path(&sidecar), Some(video));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sidecar_video_path_is_none_when_neither_container_exists() {
        let dir = std::env::temp_dir().join(format!("psc-cam-test4-{}", std::process::id()));
        let sidecar = dir.join("rec-cam-1.cam.json");
        assert_eq!(sidecar_video_path(&sidecar), None);
    }

    #[test]
    fn a_still_recording_companion_never_opens_as_a_dual_source() {
        let companion = resolve_finish_companion(Some(PathBuf::from("C:/r/rec-1.mp4")), true, |_| true);
        assert!(companion.is_none());
    }

    #[test]
    fn a_finished_companion_whose_file_vanished_is_dropped_too() {
        let companion = resolve_finish_companion(Some(PathBuf::from("C:/r/rec-1.mp4")), false, |_| false);
        assert!(companion.is_none());
    }

    #[test]
    fn a_finished_companion_with_its_file_still_present_opens_the_dual_source() {
        let companion = resolve_finish_companion(Some(PathBuf::from("C:/r/rec-1.mp4")), false, |_| true);
        assert_eq!(companion, Some(PathBuf::from("C:/r/rec-1.mp4")));
    }

    #[test]
    fn no_companion_at_all_stays_none_regardless_of_screen_state() {
        assert!(resolve_finish_companion(None, false, |_| true).is_none());
    }

    #[test]
    fn a_bare_origin_with_no_path_at_all_still_counts_as_the_camera_page() {
        // WebView2 documents Uri as the requesting ORIGIN: some runtime
        // versions hand back just that, no path, for a top-level document.
        assert!(permission_uri_is_camera_page("https://tauri.localhost"));
        assert!(permission_uri_is_camera_page("https://tauri.localhost/"));
    }

    #[test]
    fn a_full_url_to_camera_html_matches() {
        assert!(permission_uri_is_camera_page("https://tauri.localhost/camera.html"));
        assert!(permission_uri_is_camera_page("http://tauri.localhost/camera.html"));
    }

    #[test]
    fn a_query_string_on_the_camera_page_is_ignored() {
        assert!(permission_uri_is_camera_page("https://tauri.localhost/camera.html?x=1"));
    }

    #[test]
    fn a_look_alike_host_never_matches() {
        // A merely similar host must never be treated as the real app.
        assert!(!permission_uri_is_camera_page("https://tauri.localhost.evil.example/camera.html"));
        assert!(!permission_uri_is_camera_page("https://nottauri.localhost/camera.html"));
    }

    #[test]
    fn a_different_page_of_the_same_app_never_matches() {
        assert!(!permission_uri_is_camera_page("https://tauri.localhost/index.html"));
        assert!(!permission_uri_is_camera_page("https://tauri.localhost/video.html"));
    }

    #[test]
    fn a_camera_only_take_never_reads_the_screen_sidecar_path() {
        // The two sidecar kinds: .cam.json here, .keys.json for the key
        // overlay: must never collide on the same extension, or moving one
        // recording's sidecar would silently clobber the other's.
        let cam = sidecar_path(Path::new("C:/data/recordings/rec-1.webm"), CAM_SIDECAR_EXT);
        let keys = cam.with_extension("");
        assert_ne!(cam.extension().unwrap(), keys.extension().unwrap_or_default());
    }

    #[test]
    fn the_sidecar_shape_carries_the_start_timestamp_and_the_overlay_offset() {
        let sidecar = CamSidecar { start_ms: 12345, overlay_start_ms: Some(2_000) };
        let json = serde_json::to_value(sidecar).unwrap();
        let keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["overlay_start_ms", "start_ms"]);
    }

    #[test]
    fn a_camera_only_take_omits_the_overlay_offset_entirely() {
        // No companion at all: the field must not even appear, so an older
        // reader (or a human eyeballing the file) sees a plain, congruent
        // camera-only take rather than a spurious zero offset.
        let sidecar = CamSidecar { start_ms: 12345, overlay_start_ms: None };
        let json = serde_json::to_value(sidecar).unwrap();
        let keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["start_ms"]);
    }

    #[test]
    fn marking_a_session_finishing_stores_both_values_for_the_matching_path() {
        let mut session = Some(CamSession {
            output_path: PathBuf::from("C:/r/rec-cam-1.webm"),
            companion_path: None,
            finishing: None,
        });
        mark_finishing(&mut session, Path::new("C:/r/rec-cam-1.webm"), 5_000, Some(1_200));
        assert_eq!(session.unwrap().finishing, Some((5_000, Some(1_200))));
    }

    #[test]
    fn marking_a_session_finishing_is_ignored_for_a_path_that_does_not_match() {
        // A stale mark-finishing call (already superseded by a discard, or
        // for a different take entirely) must never resurrect or mutate a
        // session it no longer refers to.
        let mut session = Some(CamSession {
            output_path: PathBuf::from("C:/r/rec-cam-1.webm"),
            companion_path: None,
            finishing: None,
        });
        mark_finishing(&mut session, Path::new("C:/r/rec-cam-OTHER.webm"), 5_000, None);
        assert!(session.unwrap().finishing.is_none());
    }

    fn session_at(p: &str) -> Option<CamSession> {
        Some(CamSession { output_path: PathBuf::from(p), companion_path: None, finishing: None })
    }

    #[test]
    fn a_mismatched_path_leaves_the_active_session_in_place() {
        let mut slot = session_at("C:/r/rec-cam-1.webm");
        assert!(take_matching(&mut slot, Path::new("C:/r/rec-cam-OTHER.webm")).is_err());
        assert_eq!(slot.as_ref().unwrap().output_path, PathBuf::from("C:/r/rec-cam-1.webm"));
    }

    #[test]
    fn the_matching_path_removes_exactly_that_session() {
        let mut slot = session_at("C:/r/rec-cam-1.webm");
        let taken = take_matching(&mut slot, Path::new("C:/r/rec-cam-1.webm")).unwrap();
        assert_eq!(taken.output_path, PathBuf::from("C:/r/rec-cam-1.webm"));
        assert!(slot.is_none());
    }

    #[test]
    fn taking_from_an_empty_slot_reports_no_active_recording() {
        let mut slot: Option<CamSession> = None;
        assert_eq!(take_matching(&mut slot, Path::new("C:/r/x.webm")).err().unwrap(), "no active camera recording");
    }

    #[test]
    fn discarding_a_session_removes_the_reserved_file_and_its_sidecar() {
        let dir = std::env::temp_dir().join(format!("psc-cam-test5-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let video = dir.join("rec-cam-1.webm");
        fs::write(&video, b"x").unwrap();
        fs::write(sidecar_path(&video, CAM_SIDECAR_EXT), b"{}").unwrap();
        let mut slot = Some(CamSession { output_path: video.clone(), companion_path: None, finishing: None });
        let taken = take_matching(&mut slot, &video).unwrap();
        discard_session(&taken);
        assert!(!video.exists());
        assert!(!sidecar_path(&video, CAM_SIDECAR_EXT).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_abandoned_marker_protects_the_video_and_itself() {
        let dir = std::env::temp_dir().join(format!("psc-cam-test7-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let video = dir.join("rec-cam-1.webm");
        let other = dir.join("rec-cam-2.webm");
        fs::write(&video, b"partial").unwrap();
        fs::write(&other, b"x").unwrap();
        assert!(!is_kept_abandoned(&video));
        fs::write(abandoned_marker_path(&video), b"").unwrap();
        assert!(is_kept_abandoned(&video));
        assert!(is_kept_abandoned(&abandoned_marker_path(&video)));
        assert!(!is_kept_abandoned(&other));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn abandoning_needs_a_matching_path_and_keeps_the_file() {
        let dir = std::env::temp_dir().join(format!("psc-cam-test6-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let video = dir.join("rec-cam-1.webm");
        fs::write(&video, b"partial").unwrap();
        let mut slot = Some(CamSession { output_path: video.clone(), companion_path: None, finishing: None });
        assert!(take_matching(&mut slot, &dir.join("other.webm")).is_err());
        assert!(slot.is_some());
        take_matching(&mut slot, &video).unwrap();
        assert!(slot.is_none());
        assert_eq!(fs::read(&video).unwrap(), b"partial");
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn a_window_closed_under_a_running_take_keeps_the_file() {
        let dir = std::env::temp_dir().join(format!("psc-cam-close-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let video = dir.join("rec-cam-1.webm");
        fs::write(&video, b"ten minutes of video").unwrap();
        let session = CamSession { output_path: video.clone(), companion_path: None, finishing: None };
        settle_session_on_close(&session);
        assert!(video.exists(), "Alt+F4 during a take must not delete it");
        assert!(is_kept_abandoned(&video), "the sweep must leave the kept take alone");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_window_closed_while_finishing_keeps_the_file_and_writes_the_sidecar() {
        let dir = std::env::temp_dir().join(format!("psc-cam-close2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let video = dir.join("rec-cam-1.webm");
        fs::write(&video, b"video").unwrap();
        let session = CamSession { output_path: video.clone(), companion_path: None, finishing: Some((5_000, Some(1_200))) };
        settle_session_on_close(&session);
        assert!(video.exists());
        assert!(is_kept_abandoned(&video), "a finished-by-fallback take must survive the sweep too");
        let sidecar = read_sidecar(&video).expect("sidecar written");
        assert_eq!((sidecar.start_ms, sidecar.overlay_start_ms), (5_000, Some(1_200)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_window_closed_before_the_first_chunk_removes_the_empty_reservation() {
        let dir = std::env::temp_dir().join(format!("psc-cam-close3-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let video = dir.join("rec-cam-1.webm");
        fs::write(&video, b"").unwrap();
        let session = CamSession { output_path: video.clone(), companion_path: None, finishing: None };
        settle_session_on_close(&session);
        assert!(!video.exists(), "a file that never got a byte is not a recording");
        assert!(!is_kept_abandoned(&video));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_close_handler_settles_the_session_off_the_event_loop() {
        let rs = include_str!("camera.rs");
        let at = rs.find("if matches!(event, WindowEvent::Destroyed)").expect("destroyed handler");
        let arm = &rs[at..at + 1200];
        assert!(arm.contains("spawn_blocking(move || settle_session_on_close(&session))"), "{arm}");
        assert!(!arm.contains("discard_session("), "the close handler must not delete a take");
    }

    #[test]
    fn the_live_camera_take_is_listed_for_the_recordings_sweep() {
        use crate::prompts::sweep_stale_recordings_in;
        use std::collections::HashSet;
        use std::time::{Duration, SystemTime};
        let dir = std::env::temp_dir().join(format!("psc-cam-sweep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let video = dir.join("rec-cam-1.webm");
        let stray = dir.join("rec-cam-2.webm");
        for f in [&video, &stray] {
            fs::write(f, b"video").unwrap();
            let old = SystemTime::now() - Duration::from_secs(3 * 3600);
            fs::OpenOptions::new().write(true).open(f).unwrap().set_modified(old).unwrap();
        }
        let live = session_at(&video.to_string_lossy());
        let protected = live_session_paths(&live);
        let (count, _) = sweep_stale_recordings_in(&dir, &HashSet::new(), &protected, false, Duration::from_secs(60), None);
        assert_eq!(count, 1, "only the take that is not live goes");
        assert!(video.exists(), "a paused live take is not stale");
        assert!(!stray.exists());
        assert!(live_session_paths(&None).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_companion_screen_recording_is_in_flight_during_its_teardown() {
        assert!(screen_recording_in_flight(true, false));
        assert!(screen_recording_in_flight(false, true), "stop/cancel hold the claim while the encoder closes");
        assert!(!screen_recording_in_flight(false, false));
        let rs = include_str!("camera.rs");
        assert!(rs.contains("crate::rec_cmds::REC_STARTING.load("), "the finish must read the start claim");
    }

    #[test]
    fn a_permission_handler_that_cannot_attach_tells_the_page() {
        let rs = include_str!("camera.rs");
        let rs = &rs[..rs.find("#[cfg(test)]").unwrap()];
        assert_eq!(rs.matches("report_permission_failure(&app_ready);").count(), 2, "both failure paths report");
        assert!(include_str!("lib.rs").contains("camera::camera_permission_failed"));
    }
}
