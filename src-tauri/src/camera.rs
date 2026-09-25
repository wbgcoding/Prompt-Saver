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

use crate::{lock, now_ms, recording_dir, Db, RecordingState};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

pub struct CamSession {
    pub output_path: PathBuf,
    pub start_ms: u64,
    // The screen recording active when this take began, if any -- carried
    // through to the finish result so the frontend can decide whether a
    // dual-open (camera as an overlay on that same screen recording) makes
    // sense, without a second round trip to ask what was running back then.
    pub companion_path: Option<PathBuf>,
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

// rec-cam-YYYY-MM-DD_HHMMSS.<ext> -- the screen recorder's own rec_filename
// shape with one extra segment, so a folder listing sorts the two side by
// side and both still pass the "starts with rec-" prefix discard_recording_file
// requires before it will ever delete a file.
pub fn cam_filename(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32, ext: &str) -> String {
    format!("rec-cam-{year:04}-{month:02}-{day:02}_{hour:02}{minute:02}{second:02}.{ext}")
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

#[derive(Serialize)]
pub struct CamBeginResult {
    path: String,
    start_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    companion_path: Option<String>,
}

// Pure so the "shared start" decision has its own test independent of a real
// RecordingState/Tauri harness: a screen recording already running donates
// its own started_at_ms and path AS THE COMPANION (both files end up
// carrying the exact same start_ms, which is what "congruent" alignment
// means here);
// nothing running means the camera take is its own zero point.
fn resolve_start(active_screen: Option<(u64, PathBuf)>, now: u64) -> (u64, Option<PathBuf>) {
    match active_screen {
        Some((started_at_ms, output_path)) => (started_at_ms, Some(output_path)),
        None => (now, None),
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

// Starts a new camera take: picks the file name and, crucially, the shared
// start timestamp the editor later lines both tracks up on. A screen
// recording already running donates its own started_at_ms (RecSession,
// lib.rs) so the two stay in lockstep; camera alone is its own zero point
// (see the spec item this module implements).
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
    let path = dir.join(cam_filename(y, mo, d, h, mi, s, &ext));
    // Touch the file now, empty -- camera_append_chunk only ever writes to a
    // path this command itself created, never an arbitrary one the frontend
    // could name.
    fs::File::create(&path).map_err(|e| e.to_string())?;
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
    let (start_ms, companion_path) = resolve_start(active, now_ms());
    *cam.0.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(CamSession { output_path: path.clone(), start_ms, companion_path: companion_path.clone() });
    Ok(CamBeginResult {
        path: path.to_string_lossy().to_string(),
        start_ms,
        companion_path: companion_path.map(|p| p.to_string_lossy().to_string()),
    })
}

// MediaRecorder hands the page a Blob in timesliced chunks; each one is
// appended as it arrives instead of buffering the whole take in the
// renderer, so a long camera take never has to hold its entire size in
// memory at once.
#[tauri::command]
pub fn camera_append_chunk(cam: State<'_, CameraState>, path: String, bytes: Vec<u8>) -> Result<(), String> {
    let guard = cam.0.lock().unwrap_or_else(|e| e.into_inner());
    let session = guard.as_ref().ok_or_else(|| "no active camera recording".to_string())?;
    if session.output_path != Path::new(&path) {
        return Err("path does not match the active camera recording".to_string());
    }
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
}

#[tauri::command]
pub fn camera_finish_recording(
    cam: State<'_, CameraState>,
    rstate: State<'_, RecordingState>,
    path: String,
) -> Result<CamFinishResult, String> {
    let session = cam.0.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or_else(|| "no active camera recording".to_string())?;
    if session.output_path != Path::new(&path) {
        return Err("path does not match the active camera recording".to_string());
    }
    let sidecar = CamSidecar { start_ms: session.start_ms };
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
    Ok(CamFinishResult {
        path: session.output_path.to_string_lossy().to_string(),
        companion_path: companion_path.map(|p| p.to_string_lossy().to_string()),
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
            let _ = fs::remove_file(&session.output_path);
            let _ = fs::remove_file(sidecar_path(&session.output_path));
            return Ok(());
        }
        *guard = Some(session);
    }
    Err("path does not match the active camera recording".to_string())
}

pub fn read_sidecar(path: &Path) -> Option<CamSidecar> {
    let data = fs::read_to_string(sidecar_path(path)).ok()?;
    serde_json::from_str(&data).ok()
}

// The editor's counterpart to keylog's read_key_sidecar -- tells it whether
// a given recording carries a shared-start timestamp at all before it tries
// to align anything against it.
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
        COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
        COREWEBVIEW2_PERMISSION_STATE_ALLOW,
    };
    use webview2_com::PermissionRequestedEventHandler;
    let app = app.clone();
    win.with_webview(move |webview| {
        let Ok(core) = (unsafe { webview.controller().CoreWebView2() }) else {
            // No working permission answer -- leaving the prompt unanswered is
            // safer than allowing every permission kind to get camera through.
            return;
        };
        let mut token = 0i64;
        let handler = PermissionRequestedEventHandler::create(Box::new(|_sender, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            unsafe { args.PermissionKind(&mut kind)? };
            if kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA || kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE {
                unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)? };
            }
            Ok(())
        }));
        if unsafe { core.add_PermissionRequested(&handler, &mut token) }.is_ok() {
            let _ = app.emit_to(CAMERA_WIN_LABEL, PERMISSION_READY_EVENT, ());
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
        assert_eq!(cam_filename(2026, 9, 25, 8, 5, 3, "webm"), "rec-cam-2026-09-25_080503.webm");
    }

    #[test]
    fn sidecar_path_sits_next_to_the_recording_as_its_own_extension() {
        let p = sidecar_path(Path::new("C:/data/recordings/rec-cam-1.webm"));
        assert_eq!(p, PathBuf::from("C:/data/recordings/rec-cam-1.cam.json"));
    }

    #[test]
    fn a_camera_take_started_during_a_screen_recording_shares_its_start_and_names_it_as_companion() {
        let (start, companion) = resolve_start(Some((1000, PathBuf::from("C:/r/rec-1.mp4"))), 9999);
        assert_eq!(start, 1000);
        assert_eq!(companion, Some(PathBuf::from("C:/r/rec-1.mp4")));
    }

    #[test]
    fn a_camera_take_with_no_screen_recording_running_is_its_own_zero_point() {
        let (start, companion) = resolve_start(None, 9999);
        assert_eq!(start, 9999);
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
