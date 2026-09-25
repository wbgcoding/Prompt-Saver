// Camera recording. A getUserMedia preview window whose stream is
// encoded client-side by the browser's own MediaRecorder (webm, VP8+Opus --
// the one container every WebView2 release plays back the same way) and
// handed to this module only as finished bytes, the same division of labour
// the screen recorder never had to make because it captures through Windows
// Graphics Capture instead of a web API.
//
// The one piece that genuinely has to live in Rust is the WebView2
// permission prompt: nothing ever answers it on its own, so a plain
// getUserMedia call from the page just hangs forever with no error and no
// picture. install_permission_handler below is that answer, camera-only
// and microphone-only, and only for this one window.

use crate::{lock, recording_dir, Db, RecordingState};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub struct CamSession {
    pub output_path: PathBuf,
    // The screen recording active when this take began, if any -- carried
    // through to the finish result so the frontend can decide whether a
    // dual-open (camera as an overlay on that same screen recording) makes
    // sense, and to compute the alignment offset, without a second round
    // trip to ask what was running back then (it may already have stopped).
    pub companion_path: Option<PathBuf>,
    pub companion_start_ms: Option<u64>,
}

pub struct CameraState(pub Mutex<Option<CamSession>>);

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
pub struct CamSidecar {
    pub start_ms: u64,
}

// Sits next to the recording as its own extension, exactly like the
// key-overlay sidecar (keylog::sidecar_path) -- so a Save/Copy/discard that
// already knows how to move or drop a `.keys.json` alongside its video needs
// the same one-line addition for `.cam.json`, never a parallel code path.
pub fn sidecar_path(output_path: &Path) -> PathBuf {
    output_path.with_extension("cam.json")
}

// The mirror of lib.rs's own mic_sidecar_video_path, for the same reason: a
// `.cam.json` sitting in the recordings folder next to a video the library
// still references must count as referenced too, or the background sweep
// treats it as an ownerless orphan and removes it after an hour -- even
// though the video it belongs to is still very much in use. The camera's
// container is webm, never mp4, so this tries both instead of assuming one.
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

// rec-cam-YYYY-MM-DD_HHMMSS.<ext>, optionally with a "-2", "-3", ... suffix
// -- the screen recorder's own rec_filename shape with one extra segment, so
// a folder listing sorts the two side by side and both still pass the
// "starts with rec-" prefix discard_recording_file requires before it will
// ever delete a file. The suffix only ever appears when the plain name is
// already taken (two takes starting in the same second, or one immediately
// restarted after the last one's file did not get cleaned up) -- n=0 keeps
// today's exact filename so nothing else about it changes.
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
#[cfg(not(windows))]
fn local_time_parts() -> (i32, u32, u32, u32, u32, u32) {
    (1970, 1, 1, 0, 0, 0)
}

// Claims a filename atomically (create_new -- never truncates a file that is
// already there) and tries the next suffixed name on a collision instead of
// silently overwriting whatever a same-second take already left behind.
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

// Pure so the "does a companion exist, and what is its own start" decision
// has its own test independent of a real RecordingState/Tauri harness: a
// screen recording already running donates its own started_at_ms and path
// AS THE COMPANION; the camera's own start is never borrowed from it (that
// used to conflate the two into a single shared value, which only happened
// to look right when the camera's own real start landed exactly on the
// screen recording's) -- nothing running means there is no companion at all.
fn resolve_companion(active_screen: Option<(u64, PathBuf)>) -> (Option<u64>, Option<PathBuf>) {
    match active_screen {
        Some((started_at_ms, output_path)) => (Some(started_at_ms), Some(output_path)),
        None => (None, None),
    }
}

// Pure so the "does the dual-open make sense yet" decision has its own test
// too: a companion still recording is mid-write, not a real file to open,
// and one whose recording ended without a companion path at all (or whose
// file vanished since) has nothing to align against.
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

// The editor's own alignment math: where the camera overlay starts on the
// MAIN (screen recording) timeline. Both timestamps are the same wall-clock
// epoch (now_ms() on the Rust side, Date.now() on the JS side, same machine
// so no clock-sync concern), so a later camera start is a positive offset.
// A camera that somehow started BEFORE the screen recording is clamped to 0
// -- the overlay video kind always plays its source from its own beginning
// wherever it starts on the timeline (see ui/video.js's `movie=` filter), so
// there is no way to also trim its leading edge without a source in-point
// feature this op shape does not have yet; 0 is congruent for every camera
// take that started at or after the screen recording, which is the normal
// case (the camera window is opened and started after the screen recording
// already began).
fn overlay_start_ms(cam_start_ms: u64, companion_start_ms: u64) -> u64 {
    cam_start_ms.saturating_sub(companion_start_ms)
}

// The command form addAlignedCameraOverlay (ui/video.js) actually calls,
// after it has read the camera's own real start back from the sidecar
// (read_camera_sidecar) and been handed the screen recording's own start
// through VideoEditorSource.overlay_start_ms -- one place doing the
// subtraction and the clamp instead of a second copy of overlay_start_ms's
// logic living in JS.
#[tauri::command]
pub fn camera_overlay_start_ms(cam_start_ms: u64, companion_start_ms: u64) -> u64 {
    overlay_start_ms(cam_start_ms, companion_start_ms)
}

// Starts a new camera take: picks the file name and remembers whether a
// screen recording is already running (and its own start) so the finish
// result can decide on a dual-open and its alignment. The camera's own
// actual start is never guessed here -- see camera_finish_recording, which
// takes it from the caller at the moment MediaRecorder really started.
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
    let path = create_unique(&dir, |n| cam_filename((y, mo, d, h, mi, s), &ext, n)).map_err(|e| e.to_string())?;
    let active = {
        #[cfg(windows)]
        {
            let guard = rstate.0.lock().unwrap_or_else(|e| e.into_inner());
            guard.as_ref().map(|s| (s.started_at_ms, s.output_path.clone()))
        }
        #[cfg(not(windows))]
        {
            let _ = &rstate;
            None
        }
    };
    let (companion_start_ms, companion_path) = resolve_companion(active);
    *cam.0.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(CamSession { output_path: path.clone(), companion_path: companion_path.clone(), companion_start_ms });
    Ok(CamBeginResult {
        path: path.to_string_lossy().to_string(),
        companion_path: companion_path.map(|p| p.to_string_lossy().to_string()),
    })
}

// MediaRecorder hands the page a Blob in timesliced chunks; each one is
// appended as it arrives instead of buffering the whole take in the
// renderer, so a long camera take never has to hold its entire size in
// memory at once. Bytes travel as base64 (camera.js encodes, this decodes)
// instead of a JSON array of numbers -- about a third bigger than raw
// instead of three to four times bigger, for every chunk of every take.
#[tauri::command]
pub fn camera_append_chunk(cam: State<'_, CameraState>, path: String, bytes_b64: String) -> Result<(), String> {
    let guard = cam.0.lock().unwrap_or_else(|e| e.into_inner());
    let session = guard.as_ref().ok_or_else(|| "no active camera recording".to_string())?;
    if session.output_path != Path::new(&path) {
        return Err("path does not match the active camera recording".to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(bytes_b64).map_err(|e| e.to_string())?;
    let mut f = OpenOptions::new().append(true).open(&session.output_path).map_err(|e| e.to_string())?;
    f.write_all(&bytes).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct CamFinishResult {
    path: String,
    // Only set when the companion screen recording has ALREADY stopped by
    // the time this camera take finishes -- still recording means its file
    // is mid-write, and the editor's dual-open must wait for it to be a
    // real, playable file rather than open a half-finished one.
    #[serde(skip_serializing_if = "Option::is_none")]
    companion_path: Option<String>,
    // The screen recording's own start, handed back so the frontend can pass
    // it through to open_video_editor -- the editor re-derives the camera's
    // own start from the sidecar this call just wrote (the durable, on-disk
    // value) rather than trusting a second copy of the same number carried
    // across the whole call chain in memory.
    #[serde(skip_serializing_if = "Option::is_none")]
    companion_start_ms: Option<u64>,
}

// cam_start_ms is the real MediaRecorder start (Date.now() in camera.js, the
// moment recorder.start() actually ran) -- not a value this module invented
// at begin time, which is what let the overlay always land at startMs 0
// regardless of how long the camera took to actually start recording.
#[tauri::command]
pub fn camera_finish_recording(
    cam: State<'_, CameraState>,
    rstate: State<'_, RecordingState>,
    path: String,
    cam_start_ms: u64,
) -> Result<CamFinishResult, String> {
    let session = cam.0.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or_else(|| "no active camera recording".to_string())?;
    if session.output_path != Path::new(&path) {
        return Err("path does not match the active camera recording".to_string());
    }
    let sidecar = CamSidecar { start_ms: cam_start_ms };
    if let Ok(json) = serde_json::to_string(&sidecar) {
        let _ = fs::write(sidecar_path(&session.output_path), json);
    }
    let screen_still_active = {
        #[cfg(windows)]
        {
            rstate.0.lock().unwrap_or_else(|e| e.into_inner()).is_some()
        }
        #[cfg(not(windows))]
        {
            let _ = &rstate;
            false
        }
    };
    let companion_path = resolve_finish_companion(session.companion_path, screen_still_active, |p| p.is_file());
    let companion_start_ms = if companion_path.is_some() { session.companion_start_ms } else { None };
    Ok(CamFinishResult {
        path: session.output_path.to_string_lossy().to_string(),
        companion_path: companion_path.map(|p| p.to_string_lossy().to_string()),
        companion_start_ms,
    })
}

// The cancel path: whatever bytes made it to disk are discarded with the
// slot, same as keylog::discard -- a cancelled take leaves neither a
// half-written video nor a sidecar behind for the library to trip over.
#[tauri::command]
pub fn camera_discard_recording(cam: State<'_, CameraState>, path: String) -> Result<(), String> {
    let mut guard = cam.0.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(session) = guard.take() {
        if session.output_path == Path::new(&path) {
            discard_session(&session);
            return Ok(());
        }
        *guard = Some(session);
    }
    Err("path does not match the active camera recording".to_string())
}

fn discard_session(session: &CamSession) {
    let _ = fs::remove_file(&session.output_path);
    let _ = fs::remove_file(sidecar_path(&session.output_path));
}

pub fn read_sidecar(path: &Path) -> Option<CamSidecar> {
    let data = fs::read_to_string(sidecar_path(path)).ok()?;
    serde_json::from_str(&data).ok()
}

// The editor's counterpart to keylog's read_key_sidecar: the durable, on-disk
// source of truth for a camera take's own start, read back by
// addAlignedCameraOverlay in ui/video.js when it lines the overlay up
// against the screen recording's own start.
#[tauri::command]
pub fn read_camera_sidecar(path: String) -> Option<CamSidecar> {
    read_sidecar(Path::new(&path))
}

// ---------- window + WebView2 permission handling ----------

const CAMERA_WIN_LABEL: &str = "camera";
const PERMISSION_READY_EVENT: &str = "camera-permission-ready";

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
    install_permission_handler(&app, &win)?;
    // Alt+F4, the taskbar close, or the app quitting outright all skip
    // camera.js's own close-button handler (discardRecording), leaving a
    // half-written file and an active session behind with nothing to ever
    // clean it up. This is the window's own last chance to do the same
    // cleanup the close button does, straight from the session state.
    let cleanup_app = app.clone();
    win.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            if let Some(state) = cleanup_app.try_state::<CameraState>() {
                if let Some(session) = state.0.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    discard_session(&session);
                }
            }
        }
    });
    let _ = win.show();
    let _ = win.set_focus();
    Ok(())
}

// Answers the WebView2 camera/microphone permission prompt for this window
// only, allowing exactly those two kinds and nothing else. with_webview runs
// its closure on the main thread asynchronously, so camera.js waits for the
// camera-permission-ready event (emitted once the handler is actually
// attached) before it ever calls getUserMedia -- calling it any earlier
// would race the still-unanswered prompt and hang with no error.
#[cfg(windows)]
fn install_permission_handler(app: &AppHandle, win: &tauri::WebviewWindow) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2PermissionRequestedEventArgs3, COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA,
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
    };
    use webview2_com::PermissionRequestedEventHandler;
    use windows_core::Interface as _;
    let app_ready = app.clone();
    let app_check = app.clone();
    win.with_webview(move |webview| {
        let Ok(core) = (unsafe { webview.controller().CoreWebView2() }) else {
            // No working permission answer -- leaving the prompt unanswered is
            // safer than allowing every permission kind to get camera through.
            return;
        };
        let mut token = 0i64;
        let handler = PermissionRequestedEventHandler::create(Box::new(move |_sender, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            unsafe { args.PermissionKind(&mut kind)? };
            if kind != COREWEBVIEW2_PERMISSION_KIND_CAMERA && kind != COREWEBVIEW2_PERMISSION_KIND_MICROPHONE {
                return Ok(());
            }
            // Re-checked here, not just once when the window was opened: the
            // switch can be turned off while this window still sits open, and
            // the next getUserMedia call must stop being auto-allowed the
            // moment it is.
            let enabled = app_check
                .try_state::<Db>()
                .map(|db| lock(&db).settings.ui_flags.get("cameraRecord") == Some(&true))
                .unwrap_or(false);
            if !enabled {
                return Ok(());
            }
            // Only the camera page itself may ever get an automatic allow --
            // this handler is attached to the camera window's own controller
            // and nothing else should ever be requesting through it, but a
            // request from any other origin is refused outright rather than
            // trusted on that assumption alone.
            let mut uri_ptr = windows_core::PWSTR::null();
            let uri = if unsafe { args.Uri(&mut uri_ptr) }.is_ok() { webview2_com::take_pwstr(uri_ptr) } else { String::new() };
            if !uri.contains("camera.html") {
                return Ok(());
            }
            unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)? };
            // Never let WebView2 remember this decision in the shared
            // profile -- every app window shares one WebView2 environment, so
            // a saved "allow" here could let a getUserMedia call from a
            // DIFFERENT window (or a later run of the app) succeed silently,
            // without this handler -- installed on the camera window's
            // controller only -- ever running again.
            if let Ok(args3) = args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() {
                unsafe {
                    let _ = args3.SetSavesInProfile(false);
                }
            }
            Ok(())
        }));
        if unsafe { core.add_PermissionRequested(&handler, &mut token) }.is_ok() {
            let _ = app_ready.emit_to(CAMERA_WIN_LABEL, PERMISSION_READY_EVENT, ());
        }
    })
    .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn install_permission_handler(_app: &AppHandle, _win: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
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
    fn sidecar_path_sits_next_to_the_recording_as_its_own_extension() {
        let p = sidecar_path(Path::new("C:/data/recordings/rec-cam-1.webm"));
        assert_eq!(p, PathBuf::from("C:/data/recordings/rec-cam-1.cam.json"));
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
    fn a_camera_take_started_during_a_screen_recording_names_its_start_and_path_as_the_companion() {
        let (start, companion) = resolve_companion(Some((1000, PathBuf::from("C:/r/rec-1.mp4"))));
        assert_eq!(start, Some(1000));
        assert_eq!(companion, Some(PathBuf::from("C:/r/rec-1.mp4")));
    }

    #[test]
    fn a_camera_take_with_no_screen_recording_running_has_no_companion_at_all() {
        let (start, companion) = resolve_companion(None);
        assert!(start.is_none());
        assert!(companion.is_none());
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
    fn overlay_start_is_the_gap_between_the_two_real_starts() {
        // Camera started 3.2s after the screen recording -- the overlay must
        // not appear at the very start of the main timeline, or it would show
        // 3.2s of screen before the camera existed as if it had been there
        // the whole time.
        assert_eq!(overlay_start_ms(13_200, 10_000), 3_200);
    }

    #[test]
    fn overlay_start_is_zero_when_both_start_together() {
        assert_eq!(overlay_start_ms(10_000, 10_000), 0);
    }

    #[test]
    fn overlay_start_never_goes_negative_when_the_camera_starts_first() {
        // The op shape has no source in-point to trim the camera's own
        // leading edge -- clamped to 0 instead of underflowing.
        assert_eq!(overlay_start_ms(9_000, 10_000), 0);
    }

    #[test]
    fn a_camera_only_take_never_reads_the_screen_sidecar_path() {
        // The two sidecar kinds -- .cam.json here, .keys.json for the key
        // overlay -- must never collide on the same extension, or moving one
        // recording's sidecar would silently clobber the other's.
        let cam = sidecar_path(Path::new("C:/data/recordings/rec-1.webm"));
        let keys = cam.with_extension("");
        assert_ne!(cam.extension().unwrap(), keys.extension().unwrap_or_default());
    }

    #[test]
    fn the_sidecar_shape_carries_only_the_shared_start_timestamp() {
        let sidecar = CamSidecar { start_ms: 12345 };
        let json = serde_json::to_value(sidecar).unwrap();
        let keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["start_ms"]);
    }
}
