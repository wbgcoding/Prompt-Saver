use super::*;

// ---------- ffmpeg: fetched on demand for video export/conversion ----------
//
// ffmpeg runs as a separate subprocess, never linked into this binary, so it carries its own
// licence. The pinned build (gyan.dev's "essentials" release) is GPLv3: MP4/H.264 output is a
// requirement and only the GPL build includes libx264. Shipping it as a separate subprocess
// only requires the licence text and a source pointer (see THIRD-PARTY-NOTICES.md), as OBS
// Studio does, and does not affect this app's own licensing.
pub(crate) const FFMPEG_URL: &str =
    "https://github.com/GyanD/codexffmpeg/releases/download/9.0.1/ffmpeg-9.0.1-essentials_build.zip";
pub(crate) const FFMPEG_SHA256: &str = "fec81ae03971d9dd4be3ebe02e263bd2ec1d789483f931bdba5f5715e65da2e9";
pub(crate) const FFMPEG_ZIP_ENTRY: &str = "ffmpeg-9.0.1-essentials_build/bin/ffmpeg.exe";
// The video editor needs structured probing (duration, fps, dimensions, stream count for the
// mic-mux check) that ffmpeg's banner text cannot give reliably but ffprobe's JSON can. Same
// zip and already verified hash (FFMPEG_SHA256 covers the whole archive), so a second entry
// needs no extra pinned hash.
pub(crate) const FFPROBE_ZIP_ENTRY: &str = "ffmpeg-9.0.1-essentials_build/bin/ffprobe.exe";
// The zip is ~106 MB (it also carries ffplay.exe, never extracted); the cap is generous so a
// legitimate download is never mistaken for a truncation attempt.
pub(crate) const FFMPEG_ZIP_MAX_BYTES: u64 = 160 * 1024 * 1024;

// Set by `cancel_ffmpeg_download` and polled between chunks; a cancelled or failed download
// leaves nothing behind, so no resumable retry is needed.
pub(crate) static FFMPEG_DOWNLOAD_CANCEL: AtomicBool = AtomicBool::new(false);

// Held for the whole download: a second start (another window) is refused instead of running
// a second 100 MB transfer that shares one cancel flag.
pub(crate) static FFMPEG_DOWNLOAD_RUNNING: AtomicBool = AtomicBool::new(false);

pub(crate) struct FlagClaim(&'static AtomicBool);
impl FlagClaim {
    pub(crate) fn take(flag: &'static AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).ok().map(|_| Self(flag))
    }
}
impl Drop for FlagClaim {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub(crate) fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

// Both hashes are public and pinned, so this is not timing-attack relevant, but a
// constant-time compare is required, so it never short-circuits on the first differing byte.
pub(crate) fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// The app-owned folders a media tool may live in, in search order: next to the exe, then the
// per-user app folder. Never the data folder: that one can be a shared or network folder, so
// anyone able to write there could plant an executable that runs as the local user.
pub(crate) fn media_tool_dirs(exe_dir: Option<PathBuf>, local_dir: Option<PathBuf>) -> Vec<PathBuf> {
    exe_dir.into_iter().chain(local_dir).collect()
}

// Discovery order: next to the exe, then the per-user app folder, then PATH; first hit wins.
// Never touches the network (only `download_ffmpeg` does, after explicit user consent). The
// installer's bundled copy lands next to the exe, so an installed build never reaches the
// "not found" case.
pub(crate) fn find_media_tool(name: &str) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let dirs = media_tool_dirs(exe_dir, local_app_dir());
    if let Some(p) = dirs.iter().map(|d| d.join(name)).find(|p| p.is_file()) {
        return Some(p);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).map(|dir| dir.join(name)).find(|p| p.is_file())
    })
}
pub(crate) fn ffmpeg_search_path(_app: &AppHandle) -> Option<PathBuf> {
    find_media_tool("ffmpeg.exe")
}
// Alongside ffmpeg.exe once download_ffmpeg_blocking has run, with the same discovery order.
// An install from before ffprobe support has only ffmpeg.exe; probing and editor features
// degrade to "ffmpeg-missing" until the user re-runs the idempotent download.
pub(crate) fn ffprobe_search_path(_app: &AppHandle) -> Option<PathBuf> {
    find_media_tool("ffprobe.exe")
}

// Reports where ffmpeg was found (if anywhere) so the frontend knows whether to offer the
// download; never called on app start, only when a feature that needs ffmpeg is reached.
#[tauri::command]
pub(crate) async fn ffmpeg_status(app: AppHandle) -> Option<String> {
    ffmpeg_search_path(&app).map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub(crate) fn cancel_ffmpeg_download() {
    FFMPEG_DOWNLOAD_CANCEL.store(true, Ordering::SeqCst);
}

// Where a downloaded ffmpeg.exe is written: beside the exe when that folder is writable (the
// portable case), otherwise the per-user app folder. Never the data folder, PATH, a system
// directory or elevated.
pub(crate) fn ffmpeg_install_dir() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let probe = dir.join(format!(".wtest-{}", std::process::id()));
            if fs::write(&probe, b"").is_ok() {
                let _ = fs::remove_file(&probe);
                return Some(dir.to_path_buf());
            }
        }
    }
    local_app_dir()
}

// Download, verify, extract and stage ffmpeg.exe. The frontend must already have obtained
// explicit consent through `confirmDialog()`; this command is a plain mechanism with no
// consent check, so the UI layer is the one place that decides when it may run. Progress is
// reported via the "ffmpeg-download-progress" event ({downloaded, total}); a cancellation or
// failure leaves nothing behind (nothing is written until the hash is verified).
#[tauri::command]
pub(crate) async fn download_ffmpeg(app: AppHandle) -> Result<String, String> {
    let _claim = FlagClaim::take(&FFMPEG_DOWNLOAD_RUNNING).ok_or_else(|| "ffmpeg-busy".to_string())?;
    // Only the claim holder may clear a cancel request.
    FFMPEG_DOWNLOAD_CANCEL.store(false, Ordering::SeqCst);
    push_bg_op(&app, "ffmpeg-download", "ffmpeg-download", "indeterminate", None, None);
    let app_done = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || download_ffmpeg_blocking(&app))
        .await
        .map_err(|e| e.to_string())?;
    push_bg_op(&app_done, "ffmpeg-download", "ffmpeg-download", if result.is_ok() { "done" } else { "error" }, None, None);
    result
}

// Sanity-check this is a zip (local file header "PK\x03\x04") before trusting it further (a
// format guard, not authentication), then verify the pinned SHA-256, constant-time compared.
// A mismatch (truncated body, tampered mirror, wrong asset) is rejected and nothing is
// written, mirroring the updater's fail-closed rule (install_update_blocking). Taking the
// expected hash as a parameter makes a wrong-hash rejection provable in a unit test (see
// `ffmpeg_wrong_hash_is_rejected`).
pub(crate) fn verify_ffmpeg_zip_against(bytes: &[u8], want_hex: &str) -> Result<(), String> {
    if bytes.len() < 4 || &bytes[0..2] != b"PK" {
        return Err("downloaded file is not a valid archive".to_string());
    }
    use sha2::{Digest, Sha256};
    let got = Sha256::digest(bytes);
    let want = hex_decode(want_hex).ok_or_else(|| "bad pinned hash".to_string())?;
    if !ct_eq(got.as_slice(), &want) {
        return Err("ffmpeg download hash mismatch, rejected".to_string());
    }
    Ok(())
}

pub(crate) fn verify_ffmpeg_zip(bytes: &[u8]) -> Result<(), String> {
    verify_ffmpeg_zip_against(bytes, FFMPEG_SHA256)
}

// The HTTP client for the large downloads (ffmpeg, the installer, the WebView2 bootstrapper).
// ureq's overall request timeout would also bound reading the body, so a slow link could never
// finish a big file; the connect and each single read are bounded instead, which still ends a
// stalled transfer.
pub(crate) fn download_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(60))
        .build()
}

pub(crate) fn download_ffmpeg_blocking(app: &AppHandle) -> Result<String, String> {
    let resp = download_agent().get(FFMPEG_URL)
        .set("User-Agent", brand::USER_AGENT)
        .call()
        .map_err(|e| format!("download: {}", e))?;
    let total: u64 = resp.header("Content-Length").and_then(|s| s.parse().ok()).unwrap_or(0);
    // Reserve the announced size once instead of regrowing a 100 MB buffer by doubling; a lying
    // header is capped by the limit the loop enforces.
    let mut bytes = Vec::with_capacity(total.min(FFMPEG_ZIP_MAX_BYTES) as usize);
    use std::io::Read;
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 256 * 1024];
    // Progress goes out only when the shown value changes (whole percent, or every MiB without a
    // length): every report locks the store, updates the taskbar and wakes every window, which
    // thousands of times per download slowed the transfer itself.
    let mut last_report: Option<u64> = None;
    loop {
        if FFMPEG_DOWNLOAD_CANCEL.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }
        let n = reader.read(&mut buf).map_err(|e| format!("read: {}", e))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        if bytes.len() as u64 > FFMPEG_ZIP_MAX_BYTES {
            return Err("download too large / truncated".to_string());
        }
        let done = bytes.len() as u64;
        let pct = (done * 100).checked_div(total).map(|v| v.min(100) as u32);
        let step = pct.map(u64::from).unwrap_or(done >> 20);
        if last_report != Some(step) {
            last_report = Some(step);
            let _ = app.emit("ffmpeg-download-progress", serde_json::json!({ "downloaded": done, "total": total }));
            push_bg_op(app, "ffmpeg-download", "ffmpeg-download", if pct.is_some() { "running" } else { "indeterminate" }, pct, None);
        }
    }
    if FFMPEG_DOWNLOAD_CANCEL.load(Ordering::SeqCst) {
        return Err("cancelled".to_string());
    }
    let _ = app.emit(
        "ffmpeg-download-progress",
        serde_json::json!({ "downloaded": bytes.len() as u64, "total": total, "stage": "extract" }),
    );
    push_bg_op(app, "ffmpeg-download", "ffmpeg-download", "indeterminate", None, None);
    verify_ffmpeg_zip(&bytes)?;
    let dir = ffmpeg_install_dir().ok_or_else(|| "target dir: no per-user folder".to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("target dir: {}", e))?;
    install_ffmpeg_from_zip(&bytes, &dir)
}

// Inflates one archive entry straight into a staged file beside its final name, so
// decompression and disk writes overlap instead of holding the whole 100 MB executable in
// memory first. The staged file is removed on any failure, including a body that is not a
// Windows executable.
fn extract_entry_to(bytes: &[u8], entry_name: &str, staged: &Path) -> Result<(), String> {
    use std::io::{Read, Write};
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("zip: {}", e))?;
    let mut entry = archive.by_name(entry_name).map_err(|_| "missing".to_string())?;
    let mut head = [0u8; 2];
    entry.read_exact(&mut head).map_err(|_| "invalid-exe".to_string())?;
    if &head != b"MZ" {
        return Err("invalid-exe".to_string());
    }
    let mut write = || -> std::io::Result<()> {
        let mut out = std::io::BufWriter::with_capacity(1 << 20, fs::File::create(staged)?);
        out.write_all(&head)?;
        std::io::copy(&mut entry, &mut out)?;
        out.into_inner().map_err(|e| e.into_error())?.sync_all()
    };
    write().map_err(|e| {
        let _ = fs::remove_file(staged);
        format!("extract: {}", e)
    })
}

// Extracts ffmpeg.exe and (best-effort) ffprobe.exe from a verified archive into `dir`, both
// in parallel, each staged under a unique temp name in the same directory and renamed into
// place: an interrupted write never leaves a half-written executable at the real name, and a
// same-directory rename is atomic on NTFS. A build without ffprobe.exe still converts via
// ffmpeg.exe alone, just without video-editor probing until a fresh download.
pub(crate) fn install_ffmpeg_from_zip(bytes: &[u8], dir: &Path) -> Result<String, String> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let staged = dir.join(format!(".ffmpeg-{}-{}.tmp", std::process::id(), stamp));
    let probe_staged = dir.join(format!(".ffprobe-{}-{}.tmp", std::process::id(), stamp));
    let (exe, probe) = std::thread::scope(|s| {
        let probe = s.spawn(|| extract_entry_to(bytes, FFPROBE_ZIP_ENTRY, &probe_staged));
        let exe = extract_entry_to(bytes, FFMPEG_ZIP_ENTRY, &staged);
        (exe, probe.join().unwrap_or_else(|_| Err("extract: panicked".to_string())))
    });
    if let Err(e) = exe {
        if probe.is_ok() {
            let _ = fs::remove_file(&probe_staged);
        }
        return Err(match e.as_str() {
            "missing" => "ffmpeg.exe not found inside the downloaded archive".to_string(),
            "invalid-exe" => "extracted file is not a valid executable".to_string(),
            _ => e,
        });
    }
    let final_path = dir.join("ffmpeg.exe");
    fs::rename(&staged, &final_path).map_err(|e| {
        let _ = fs::remove_file(&staged);
        if probe.is_ok() {
            let _ = fs::remove_file(&probe_staged);
        }
        format!("install: {}", e)
    })?;
    if probe.is_ok() && fs::rename(&probe_staged, dir.join("ffprobe.exe")).is_err() {
        let _ = fs::remove_file(&probe_staged);
    }
    Ok(final_path.to_string_lossy().to_string())
}

// ---------- Video conversion (ffmpeg) ----------
//
// MP4/H.264 is what the native recorder writes; a different target format or the lossless
// quality step is produced by this separate background pass with a cancelable progress
// indicator, never inside the recorder. The source MP4 is deleted only once the new file is
// confirmed on disk, so a failed or cancelled run leaves the original recording intact.

pub(crate) static REC_CONVERT_CANCEL: AtomicBool = AtomicBool::new(false);

#[derive(serde::Serialize, Clone)]
pub(crate) struct ConvertProgressPayload {
    pub(crate) ms: u32,
}

#[tauri::command]
pub(crate) fn cancel_recording_convert() {
    REC_CONVERT_CANCEL.store(true, Ordering::SeqCst);
}

// The crop rect a window recording's stop_recording_now just computed, waiting for the
// frontend's automatic post-capture crop step. Only one recording result flow is open at a
// time (as REC_CONVERT_CANCEL assumes). Keyed by path so a crop request for a superseded
// recording (the result modal was discarded and a new recording started) is refused.
// (recording path, crop rect as x, y, w, h)
pub(crate) type PendingRecCrop = Option<(PathBuf, (u32, u32, u32, u32))>;
pub(crate) static PENDING_REC_CROP: Mutex<PendingRecCrop> = Mutex::new(None);
pub(crate) static REC_CROP_CANCEL: AtomicBool = AtomicBool::new(false);

// Every part a "finish and continue" chain has closed, oldest first. Read when the recording
// stops for real (stop_recording_now) to fill RecordingStoppedPayload.parts, and cleared when
// a new recording starts so a finished chain never leaks into the next result. Empty for a
// plain recording. Each entry keeps its mic sidecar path (both system and mic audio active),
// so a cancel can remove every part's leftover files, not just its video.
pub(crate) static REC_SESSION_PARTS: Mutex<Vec<RecordingPartInfo>> = Mutex::new(Vec::new());

// One entry per interim continuation-part crop running in the background. A cancel of the
// whole take signals each to stop and waits for it before deleting that part's file, so an
// ffmpeg process still reading the source cannot lose that race against the delete. Each
// thread removes its own entry when done, cancelled or not.
pub(crate) static REC_INTERIM_CROPS: Mutex<Vec<(Arc<AtomicBool>, std::thread::JoinHandle<()>)>> = Mutex::new(Vec::new());

#[derive(serde::Serialize, Clone)]
pub(crate) struct CropProgressPayload {
    pub(crate) ms: u32,
}

#[tauri::command]
pub(crate) fn cancel_recording_crop() {
    REC_CROP_CANCEL.store(true, Ordering::SeqCst);
}

// Trims a window recording down to the union of every position the window occupied, dropping
// the black margin its fixed multi-display canvas leaves. Reuses run_ffmpeg_to_staged so a
// cancel or failure leaves the original recording untouched; it is a quality nicety a window
// recording never fails to complete without. Progress arrives as "recording-crop-progress"
// ({ms}), shaped like convert_recording's progress event.
#[tauri::command]
pub(crate) async fn crop_recording_to_used_area(app: AppHandle, path: String) -> Result<String, String> {
    let src = PathBuf::from(&path);
    let rect = {
        let mut pending = PENDING_REC_CROP.lock().unwrap_or_else(|e| e.into_inner());
        match pending.take() {
            Some((p, r)) if p == src => r,
            other => {
                *pending = other;
                return Err("nothing to crop".to_string());
            }
        }
    };
    let ffmpeg = ffmpeg_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    REC_CROP_CANCEL.store(false, Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || {
        crop_recording_blocking(&ffmpeg, &src, rect, || REC_CROP_CANCEL.load(Ordering::SeqCst), |ms| {
            let _ = app.emit("recording-crop-progress", CropProgressPayload { ms });
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// crf 16 (not convert_recording's lossless crf 0): a crop-only pass has no reason to balloon
// the file for a difference nobody can see. Audio is copied untouched, since cropping never
// touches timing and re-encoding it would only risk sync drift.
pub(crate) fn crop_recording_blocking(
    ffmpeg: &Path, src: &Path, rect: (u32, u32, u32, u32), is_cancelled: impl Fn() -> bool, mut on_progress: impl FnMut(u32),
) -> Result<String, String> {
    // libx264 requires even dimensions; the offset is floored to even too so chroma stays aligned
    // on the 4:2:0 subsampling grid.
    let (x, y, w, h) = rect;
    let (x, y) = (x & !1, y & !1);
    let (w, h) = ((w & !1).max(2), (h & !1).max(2));
    let args = vec![
        "-vf".to_string(), format!("crop={w}:{h}:{x}:{y}"),
        "-c:v".to_string(), "libx264".to_string(), "-crf".to_string(), "16".to_string(), "-preset".to_string(), "veryfast".to_string(),
        "-c:a".to_string(), "copy".to_string(),
    ];
    let staged = run_ffmpeg_to_staged(ffmpeg, src, src, &[], &args, is_cancelled, &mut on_progress)?;
    fs::rename(&staged, src).map_err(|e| {
        let _ = fs::remove_file(&staged);
        let text = e.to_string();
        ffmpeg_error_json(classify_ffmpeg_error(&text), None, &text)
    })?;
    Ok(src.to_string_lossy().to_string())
}

// `format`: "mp4" | "webm" | "gif". `lossless` means something only for "mp4" (a CRF-0
// re-encode of the same source, the lossless quality step); "webm"/"gif" use fixed settings.
// Progress arrives as "recording-convert-progress" ({ms}: encoded output time so far); the
// frontend knows the total duration from its capture timer and turns it into a percentage.
#[tauri::command]
pub(crate) async fn convert_recording(
    app: AppHandle,
    state: State<'_, Db>,
    src_path: String,
    format: String,
    lossless: bool,
) -> Result<String, String> {
    if !is_video_format(&format) {
        return Err("unsupported format".to_string());
    }
    let src = PathBuf::from(&src_path);
    if !src.is_file() {
        return Err(ffmpeg_error_json(FfmpegErrorKey::Missing, None, ""));
    }
    // ffmpeg re-encodes this file in place of the source, so an IPC caller that is not the
    // recording UI must not be able to point that at an arbitrary file by naming it in src_path.
    if !is_own_recording_source(&recording_source_dirs(&app, &state), &src) {
        return Err("source-not-allowed".to_string());
    }
    let ffmpeg = ffmpeg_search_path(&app).ok_or_else(|| "ffmpeg-missing".to_string())?;
    REC_CONVERT_CANCEL.store(false, Ordering::SeqCst);
    push_bg_op(&app, "convert", "convert", "indeterminate", None, Some(src_path.clone()));
    let app_done = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        convert_recording_blocking(&ffmpeg, &src, &format, lossless, || REC_CONVERT_CANCEL.load(Ordering::SeqCst), |ms| {
            let _ = app.emit("recording-convert-progress", ConvertProgressPayload { ms });
        })
    })
    .await
    .map_err(|e| e.to_string())?;
    push_bg_op(&app_done, "convert", "convert", if result.is_ok() { "done" } else { "error" }, None, None);
    result
}

// A request to "convert" mp4 -> mp4 without the lossless step would overwrite the source with
// an identical copy, so it is rejected as a caller mistake rather than run through ffmpeg. The
// lossless step is the exception (its point is an mp4 -> mp4 re-encode), so same-extension
// alone cannot be the guard.
pub(crate) fn convert_is_noop(same_extension: bool, format: &str, lossless: bool) -> bool {
    same_extension && !(format == "mp4" && lossless)
}

// The formats a recording or an export can be written as. Each key is also the
// file extension it writes.
pub(crate) const VIDEO_FORMATS: [&str; 4] = ["mp4", "webm", "gif", "avi"];

pub(crate) fn is_video_format(format: &str) -> bool {
    VIDEO_FORMATS.contains(&format)
}

// ---------- ffmpeg errors in plain language, one classifier for every caller ----------
//
// ffmpeg's own log lines ("moov atom not found", "Invalid data found when processing input")
// diagnose for someone who knows what they mean, not a message to show a user. Every ffmpeg
// (and ffmpeg-adjacent Windows IO) failure goes through classify_ffmpeg_error() to one of
// these stable keys, translated in all eleven languages; the exact wording stays reachable
// as a collapsible "technical details" line, never the headline. A user-triggered cancel is
// handled separately (see is_cancelled() below) and never reaches this classifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FfmpegErrorKey {
    Truncated,
    Unreadable,
    Missing,
    Denied,
    NoSpace,
    OddSize,
    TooLarge,
    Timeout,
    Other,
}

impl FfmpegErrorKey {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            FfmpegErrorKey::Truncated => "ffErrTruncated",
            FfmpegErrorKey::Unreadable => "ffErrUnreadable",
            FfmpegErrorKey::Missing => "ffErrMissing",
            FfmpegErrorKey::Denied => "ffErrDenied",
            FfmpegErrorKey::NoSpace => "ffErrNoSpace",
            FfmpegErrorKey::OddSize => "ffErrOddSize",
            FfmpegErrorKey::TooLarge => "ffErrTooLarge",
            FfmpegErrorKey::Timeout => "ffErrTimeout",
            FfmpegErrorKey::Other => "ffErrOther",
        }
    }
}

// Ordered by specificity: a truncated file's log also contains generic "Invalid data" noise
// further down, so the more precise match is tried first. Covers ffmpeg's own AVERROR text
// (fixed English POSIX-style strings on every platform) and the std::io::Error wording Rust
// produces for a failed spawn, wait or rename, since both reach this function from the call
// sites below.
pub(crate) fn classify_ffmpeg_error(text: &str) -> FfmpegErrorKey {
    if text.contains("moov atom not found") {
        FfmpegErrorKey::Truncated
    } else if text.contains("No such file or directory")
        || text.contains("cannot find the file")
        || text.contains("cannot find the path")
    {
        FfmpegErrorKey::Missing
    } else if text.contains("Permission denied") || text.contains("Access is denied") {
        FfmpegErrorKey::Denied
    } else if text.contains("No space left on device") || text.contains("There is not enough space") {
        FfmpegErrorKey::NoSpace
    } else if text.contains("Unknown encoder") || text.contains("Encoder not found") {
        FfmpegErrorKey::Other
    } else if text.contains("not divisible by 2") || text.contains("dimensions not divisible") {
        FfmpegErrorKey::OddSize
    } else if text.contains("Cannot allocate memory") || text.contains("dimensions too large") {
        FfmpegErrorKey::TooLarge
    } else if text.contains("Invalid data found when processing input") {
        FfmpegErrorKey::Unreadable
    } else {
        FfmpegErrorKey::Other
    }
}

// Capped in lines rather than characters, so a run with many short diagnostic lines is not
// cut off mid-sentence; a sane bound on the collapsible "technical details" text, not a
// user-facing setting.
pub(crate) const FFMPEG_ERROR_TAIL_LINES: usize = 20;

pub(crate) fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

#[derive(serde::Serialize)]
pub(crate) struct FfmpegErrorPayload {
    pub(crate) key: &'static str,
    pub(crate) code: Option<i32>,
    pub(crate) raw: String,
}

// The one place an ffmpeg failure becomes the string a Tauri command returns: a small JSON
// envelope the frontend's shared parseFfmpegError() unwraps, instead of a copy of this
// classify-and-format logic per call site.
pub(crate) fn ffmpeg_error_json(key: FfmpegErrorKey, code: Option<i32>, raw_text: &str) -> String {
    let payload = FfmpegErrorPayload { key: key.as_str(), code, raw: tail_lines(raw_text, FFMPEG_ERROR_TAIL_LINES) };
    serde_json::to_string(&payload).unwrap_or_else(|_| key.as_str().to_string())
}

// A Windows job object that kills everything assigned to it when its last handle closes,
// which includes this process ending for any reason. Without it a quit or crash during an
// export leaves ffmpeg alive and still writing its staged file into the user's folder.
pub(crate) struct KillOnCloseJob(pub(crate) windows::Win32::Foundation::HANDLE);

// A job handle is a plain kernel object handle, usable from any thread.
unsafe impl Send for KillOnCloseJob {}
unsafe impl Sync for KillOnCloseJob {}

impl KillOnCloseJob {
    pub(crate) fn new() -> Option<Self> {
        use windows::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        unsafe {
            let handle = CreateJobObjectW(None, None).ok()?;
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if set.is_err() {
                let _ = windows::Win32::Foundation::CloseHandle(handle);
                return None;
            }
            Some(KillOnCloseJob(handle))
        }
    }

    pub(crate) fn assign(&self, child: &std::process::Child) -> bool {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::System::JobObjects::AssignProcessToJobObject;
        let process = windows::Win32::Foundation::HANDLE(child.as_raw_handle());
        unsafe { AssignProcessToJobObject(self.0, process).is_ok() }
    }
}

impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

// The app's own job, created on first use and never closed before the process ends. None when
// the OS refused: children then run unsupervised.
pub(crate) fn app_job() -> Option<&'static KillOnCloseJob> {
    static JOB: std::sync::OnceLock<Option<KillOnCloseJob>> = std::sync::OnceLock::new();
    JOB.get_or_init(KillOnCloseJob::new).as_ref()
}

// Spawns `cmd` and puts the child into `job`. A failed assignment is not an error:
// the child still runs, it just is not covered.
pub(crate) fn spawn_in_job(cmd: &mut std::process::Command, job: Option<&KillOnCloseJob>) -> std::io::Result<std::process::Child> {
    let child = cmd.spawn()?;
    if let Some(job) = job {
        if !job.assign(&child) {
            append_crash_log("could not put a helper process into the app job object");
        }
    }
    Ok(child)
}

// The error text a helper that ran out of time hands to the window (mapped in its error
// table); the io error itself carries `ErrorKind::TimedOut`.
pub(crate) const FFMPEG_TIMEOUT_ERR: &str = "ffmpeg-timeout";

// Waits for `child` for at most `timeout`, polling every 50 ms. Past the limit the child is
// killed and reaped and the result is a `TimedOut` error: a stalled network file or a hung
// drive must not leave a command pending for good.
pub(crate) fn wait_with_deadline(child: &mut std::process::Child, timeout: Duration) -> std::io::Result<std::process::ExitStatus> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, FFMPEG_TIMEOUT_ERR));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

// What a failed helper run says: the timeout sentinel as is, anything else as the
// "could not start" text the window already knows.
pub(crate) fn tool_run_error(tool: &str, e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::TimedOut {
        FFMPEG_TIMEOUT_ERR.to_string()
    } else {
        format!("could not start {tool}: {e}")
    }
}

// `Command::status()` for a helper that should die with the app, with a time limit.
pub(crate) fn status_in_job(cmd: &mut std::process::Command, timeout: Duration) -> std::io::Result<std::process::ExitStatus> {
    let mut child = spawn_in_job(cmd, app_job())?;
    wait_with_deadline(&mut child, timeout)
}

// `Command::output()` for a helper that should die with the app, with a time limit. Both
// pipes are drained on their own threads while the child runs (a full pipe would stall it);
// after a timeout those threads end when the killed child's pipes close.
pub(crate) fn output_in_job(cmd: &mut std::process::Command, timeout: Duration) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = spawn_in_job(cmd, app_job())?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        pipe.map(|mut p| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = p.read_to_end(&mut buf);
                buf
            })
        })
    };
    let out_reader = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err_reader = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let status = wait_with_deadline(&mut child, timeout)?;
    let collect = |h: Option<std::thread::JoinHandle<Vec<u8>>>| h.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
    Ok(std::process::Output { status, stdout: collect(out_reader), stderr: collect(err_reader) })
}

// Where run_ffmpeg_to_staged notes the staged files it has in flight, one path per line, so a
// start after a killed run can remove what the dead process left in the user's folders. Set
// once at start-up; without it nothing is recorded.
pub(crate) static EXPORT_JOURNAL: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
pub(crate) static EXPORT_JOURNAL_LOCK: Mutex<()> = Mutex::new(());

// One line per staged file: `host<TAB>pid<TAB>path`. The data folder may be shared between
// machines, so a line is only acted on by the host that wrote it.
pub(crate) fn journal_lines(journal: &Path) -> Vec<String> {
    fs::read_to_string(journal)
        .map(|t| t.lines().filter(|l| !l.is_empty()).map(String::from).collect())
        .unwrap_or_default()
}

pub(crate) fn journal_line(host: &str, pid: u32, staged: &Path) -> String {
    format!("{}\t{}\t{}", host.replace(['\t', '\r', '\n'], " "), pid, staged.to_string_lossy())
}

pub(crate) fn parse_journal_line(line: &str) -> Option<(&str, u32, &str)> {
    let mut it = line.splitn(3, '\t');
    let host = it.next()?;
    let pid = it.next()?.parse().ok()?;
    let path = it.next()?;
    Some((host, pid, path))
}

// Atomic replace: a reader never sees a half-written list.
pub(crate) fn journal_write(journal: &Path, lines: &[String]) {
    if lines.is_empty() {
        let _ = fs::remove_file(journal);
    } else {
        let tmp = journal.with_extension(format!("tmp{}", std::process::id()));
        if fs::write(&tmp, lines.join("\n") + "\n").is_ok() && fs::rename(&tmp, journal).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }
}

// Read-modify-write across processes: the in-process mutex covers threads, a create-new lock
// file covers a second local instance. A lock older than ten seconds belongs to a dead run
// and is taken over; if it cannot be had within a second the change runs unlocked rather than
// blocking an export.
pub(crate) fn with_journal_lock<R>(journal: &Path, f: impl FnOnce() -> R) -> R {
    let _guard = EXPORT_JOURNAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let lock = journal.with_extension("lock");
    let mut held = false;
    for _ in 0..20 {
        if fs::OpenOptions::new().write(true).create_new(true).open(&lock).is_ok() {
            held = true;
            break;
        }
        let stale = fs::metadata(&lock)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(10));
        if stale {
            let _ = fs::remove_file(&lock);
        } else {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let out = f();
    if held {
        let _ = fs::remove_file(&lock);
    }
    out
}

pub(crate) fn export_journal_add(journal: &Path, staged: &Path) {
    with_journal_lock(journal, || {
        let mut lines = journal_lines(journal);
        lines.push(journal_line(&local_host_name(), std::process::id(), staged));
        journal_write(journal, &lines);
    });
}

pub(crate) fn export_journal_remove(journal: &Path, staged: &Path) {
    with_journal_lock(journal, || {
        let mine = journal_line(&local_host_name(), std::process::id(), staged);
        let lines: Vec<String> = journal_lines(journal).into_iter().filter(|l| *l != mine).collect();
        journal_write(journal, &lines);
    });
}

// The process id in a `.export-<pid>-<stamp>.<ext>` staged name, and only for a name of
// exactly that shape: the journal is never trusted to name other files.
pub(crate) fn staged_export_pid(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix(".export-")?;
    let (pid, tail) = rest.split_once('-')?;
    let (stamp, ext) = tail.split_once('.')?;
    if stamp.is_empty() || !stamp.bytes().all(|b| b.is_ascii_digit()) || ext.is_empty() {
        return None;
    }
    pid.parse().ok()
}

// Only a local absolute path may be deleted from a journal line: a UNC, other network or
// relative path would make every start talk to a host the journal's writer picked.
pub(crate) fn is_local_absolute_path(raw: &str) -> bool {
    if raw.starts_with("\\\\") || raw.starts_with("//") {
        return false;
    }
    let b = raw.as_bytes();
    if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' || (b[2] != b'\\' && b[2] != b'/') {
        return false;
    }
    let root: Vec<u16> = format!("{}:\\", b[0] as char).encode_utf16().chain(std::iter::once(0)).collect();
    const DRIVE_REMOTE: u32 = 4;
    unsafe { windows::Win32::Storage::FileSystem::GetDriveTypeW(windows::core::PCWSTR(root.as_ptr())) != DRIVE_REMOTE }
}

// Removes the staged files of runs on this host whose process is gone and forgets them. Lines
// of other hosts are kept verbatim (a pid means nothing there); a file whose process is still
// alive stays listed. Callers skip this in a read-only instance.
pub(crate) fn sweep_stale_exports(journal: &Path, host: &str, is_alive: impl Fn(u32) -> bool) {
    with_journal_lock(journal, || {
        let mut keep: Vec<String> = Vec::new();
        for line in journal_lines(journal) {
            let Some((line_host, pid, raw)) = parse_journal_line(&line) else {
                continue; // not a line this version wrote: forgotten, never deleted
            };
            if line_host != host {
                keep.push(line);
                continue;
            }
            if !is_local_absolute_path(raw) {
                continue; // network or relative: forgotten, never deleted
            }
            let path = PathBuf::from(raw);
            match staged_export_pid(&path) {
                Some(name_pid) if name_pid == pid && (pid == std::process::id() || is_alive(pid)) => keep.push(line),
                Some(name_pid) if name_pid == pid => {
                    let _ = fs::remove_file(&path);
                }
                _ => {} // not one of ours: dropped from the list, never deleted
            }
        }
        journal_write(journal, &keep);
    });
}

// Generous on purpose: a last-resort net against a hung ffmpeg process, not a
// quality-of-service limit, so it must never fire on a large export that is just slow.
pub(crate) const FFMPEG_MAX_RUN_SECS: u64 = 30 * 60;

// Shared by convert_recording and the video editor's export (one export path): spawns ffmpeg
// with the given output-side arguments, streams `-progress pipe:1` into `on_progress`, tails
// stderr for the error message on failure, and stages the result under a unique temp name in
// `dst`'s own directory. The caller renames it into place only once it is confirmed complete,
// so a failed or cancelled run never leaves a half-written file at any real name. Neither
// caller talks to the ffmpeg process directly.
pub(crate) fn run_ffmpeg_to_staged(
    ffmpeg: &Path,
    src: &Path,
    dst: &Path,
    input_args: &[String],
    output_args: &[String],
    is_cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(u32),
) -> Result<PathBuf, String> {
    use std::os::windows::process::CommandExt;
    let ext = dst.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let staged = dst.with_file_name(format!(".export-{}-{}.{}", std::process::id(), stamp, ext));
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(["-y", "-hide_banner"]);
    cmd.args(input_args);
    cmd.arg("-i").arg(src).args(["-progress", "pipe:1", "-nostats"]);
    cmd.args(output_args);
    cmd.arg(&staged);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let journal = EXPORT_JOURNAL.get();
    if let Some(journal) = journal {
        export_journal_add(journal, &staged);
    }
    let forget_staged = || {
        if let Some(journal) = journal {
            export_journal_remove(journal, &staged);
        }
    };
    let mut child = match spawn_in_job(&mut cmd, app_job()) {
        Ok(c) => c,
        Err(e) => {
            forget_staged();
            let text = e.to_string();
            return Err(ffmpeg_error_json(classify_ffmpeg_error(&text), None, &text));
        }
    };

    // ffmpeg's normal log on stderr is drained on its own thread so a full OS pipe buffer can
    // never block the stdout progress reader below; only its text is kept (tailed below) for the
    // error message on a failure.
    let stderr_thread = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            String::from_utf8_lossy(&buf).into_owned()
        })
    });

    // stdout carries `-progress pipe:1`'s machine-readable "key=value" lines, one flush per line,
    // unlike the carriage-return-redrawn stats ffmpeg prints to stderr. `out_time_ms` is ffmpeg's
    // name for a value that is actually in microseconds, hence the /1000 below. A thread reads the
    // lines and hands the parsed values over a channel, so this loop wakes every 200 ms whether or
    // not ffmpeg printed anything: a cancel or the FFMPEG_MAX_RUN_SECS limit takes effect even
    // while ffmpeg is silent (probing a huge input, a stalled source), not only at its next
    // progress line.
    let start = std::time::Instant::now();
    let mut timed_out = false;
    if let Some(stdout) = child.stdout.take() {
        let (tx, rx) = std::sync::mpsc::channel::<u32>();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Some(us) = line.strip_prefix("out_time_ms=").and_then(|v| v.trim().parse::<i64>().ok()) {
                    if tx.send((us / 1000).max(0) as u32).is_err() {
                        break;
                    }
                }
            }
        });
        loop {
            match rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(ms) => on_progress(ms),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if is_cancelled() {
                let _ = child.kill();
                break;
            }
            if start.elapsed().as_secs() > FFMPEG_MAX_RUN_SECS {
                timed_out = true;
                let _ = child.kill();
                break;
            }
        }
    }

    let status = match child.wait() {
        Ok(s) => s,
        Err(e) => {
            forget_staged();
            let text = e.to_string();
            return Err(ffmpeg_error_json(classify_ffmpeg_error(&text), None, &text));
        }
    };
    let stderr_text = stderr_thread.and_then(|t| t.join().ok()).unwrap_or_default();
    // ffmpeg is gone: from here the staged file is either handed back or removed by this function.
    forget_staged();

    if is_cancelled() {
        let _ = fs::remove_file(&staged);
        return Err("cancelled".to_string());
    }
    if timed_out {
        let _ = fs::remove_file(&staged);
        return Err(ffmpeg_error_json(FfmpegErrorKey::Timeout, status.code(), &stderr_text));
    }
    if !status.success() {
        let _ = fs::remove_file(&staged);
        let key = classify_ffmpeg_error(&stderr_text);
        return Err(ffmpeg_error_json(key, status.code(), &stderr_text));
    }
    if !staged.is_file() {
        return Err(ffmpeg_error_json(FfmpegErrorKey::Other, None, "ffmpeg reported success but wrote no output"));
    }
    Ok(staged)
}

// `is_cancelled` and `on_progress` are plain closures rather than the process-global flag and
// an `AppHandle`, since this function only polls and reports with them. That keeps it free of
// Tauri's runtime and REC_CONVERT_CANCEL and testable against a real ffmpeg binary with its
// own local cancel flag.
pub(crate) fn convert_recording_blocking(
    ffmpeg: &Path, src: &Path, format: &str, lossless: bool, is_cancelled: impl Fn() -> bool, mut on_progress: impl FnMut(u32),
) -> Result<String, String> {
    let same_extension = src.extension().is_some_and(|e| e.eq_ignore_ascii_case(format));
    if convert_is_noop(same_extension, format, lossless) {
        return Err("nothing to convert".to_string());
    }
    // The real, final name; for mp4 + lossless this is the same path as `src` (that step replaces
    // it in place). ffmpeg is never pointed at it directly: run_ffmpeg_to_staged writes a
    // uniquely-named staged file in the same directory first, so a source that is also the
    // destination is never read and written by one process at once, and a failed or cancelled
    // conversion never leaves a half-written file at either name.
    let dst = src.with_extension(format);
    let mut args: Vec<String> = Vec::new();
    match format {
        "mp4" => {
            args.extend(["-c:v", "libx264"].map(String::from));
            if lossless {
                args.extend(["-crf", "0", "-preset", "veryfast"].map(String::from));
            } else {
                args.extend(["-crf", "18", "-preset", "medium"].map(String::from));
            }
            args.extend(["-c:a", "aac", "-b:a", "192k"].map(String::from));
        }
        "webm" => {
            args.extend(["-c:v", "libvpx-vp9", "-crf", "30", "-b:v", "0", "-c:a", "libopus"].map(String::from));
        }
        "gif" => {
            // Capped at 30s, palette-quantised, no audio: a straight per-frame GIF encode is huge and
            // visibly banded.
            args.extend([
                "-t", "30", "-an", "-vf",
                "fps=15,scale=iw:-1:flags=lanczos,split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer",
            ].map(String::from));
        }
        // avi: the mp4 codec family in a container older programs handle better.
        "avi" => {
            args.extend(["-c:v", "mpeg4", "-q:v", "5", "-c:a", "libmp3lame", "-b:a", "192k"].map(String::from));
        }
        _ => unreachable!(),
    }
    let staged = match run_ffmpeg_to_staged(ffmpeg, src, &dst, &[], &args, is_cancelled, &mut on_progress) {
        Ok(p) => p,
        Err(e) => {
            append_crash_log(&format!("recording convert failed ({format}): {e}"));
            return Err(e);
        }
    };
    // Only now, with the converted file confirmed on disk, does the real name get touched: rename
    // the staged file into place (which also replaces `src` for the mp4 + lossless case, since
    // `dst == src`), then remove the old source if it is a different file from what sits at `dst`.
    fs::rename(&staged, &dst).map_err(|e| {
        let _ = fs::remove_file(&staged);
        let text = e.to_string();
        ffmpeg_error_json(classify_ffmpeg_error(&text), None, &text)
    })?;
    if dst != src {
        let _ = fs::remove_file(src);
    }
    Ok(dst.to_string_lossy().to_string())
}
