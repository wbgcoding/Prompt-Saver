use super::*;

// The sidecar files beside a recording, each as an extension of the video's name (`rec.mp4`
// -> `rec.keys.json`). One builder for all of them, so a Save/Copy/discard that moves or
// drops one kind handles the others the same way.
pub(crate) const KEYS_SIDECAR_EXT: &str = "keys.json";
pub(crate) const CAM_SIDECAR_EXT: &str = "cam.json";

pub(crate) fn sidecar_path(video: &Path, ext: &str) -> PathBuf {
    video.with_extension(ext)
}

#[tauri::command]
pub(crate) fn video_editor_source(pending: State<VideoEditorPending>) -> Option<VideoEditorSource> {
    // A source still offered to an open editor wins: a page that boots in the meantime never got
    // the load event and has to start on it.
    let offered = VE_PENDING_REQUEST.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let (path, origin_dir, overlay_path) = offered.unwrap_or_else(|| pending.lock().clone());
    let overlay_path = if overlay_path.is_empty() { None } else { Some(overlay_path) };
    if path.is_empty() { None } else { Some(VideoEditorSource { path, origin_dir, overlay_path, request_id: None }) }
}

// ---- Open an existing file in whichever editor its content calls for ----

// Largest file, in megabytes, the editors will open.
pub(crate) const EDITOR_OPEN_MAX_MB: f64 = 2048.0;

// True once a file is past the ceiling. Pure, so the boundary (exactly at the limit still
// opens) is testable without a running app.
pub(crate) fn exceeds_open_edit_limit(bytes: u64, max_mb: f64) -> bool {
    let cap = ((max_mb.max(1.0)) as u64).saturating_mul(1024 * 1024);
    bytes > cap
}

// Content, never the extension, decides what opens: a real image (any format the `image`
// crate reads, sniffed as decode_image_file does) wins first as the cheaper check, then
// ffprobe is asked whether it is a readable video. `ffprobe` is None in a unit test without
// the bundled ffprobe.exe, so every video case falls through to "not media" there.
pub(crate) fn detect_media_kind(path: &Path, ffprobe: Option<&Path>) -> Option<(&'static str, Option<image::ImageFormat>)> {
    if decode_image_file(&path.to_string_lossy()).is_some() {
        let fmt = image::ImageReader::open(path)
            .ok()
            .and_then(|r| r.with_guessed_format().ok())
            .and_then(|r| r.format());
        return Some(("image", fmt));
    }
    if let Some(ffprobe) = ffprobe {
        if probe_recording_blocking(ffprobe, path).is_ok() {
            return Some(("video", None));
        }
    }
    None
}

// The data URL the editor loads. The webview decodes png, jpeg, gif, webp, bmp and ico itself,
// so those travel as the file's own bytes; any other format the image crate reads (TIFF, TGA ...)
// is re-encoded to PNG, otherwise the editor's image element fails and nothing opens.
pub(crate) fn editor_image_data_url(path: &Path, fmt: Option<image::ImageFormat>) -> Result<String, String> {
    use image::ImageFormat as F;
    if matches!(fmt, Some(F::Png | F::Jpeg | F::Gif | F::WebP | F::Bmp | F::Ico)) {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let mime = fmt.map(image_format_mime).unwrap_or("image/png");
        return Ok(format!("data:{};base64,{}", mime, base64_encode(&bytes)));
    }
    let img = decode_image_oriented(image::ImageReader::open(path).map_err(|e| e.to_string())?).ok_or_else(|| "not-media".to_string())?;
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(format!("data:image/png;base64,{}", base64_encode(&png.into_inner())))
}

#[derive(Serialize)]
#[serde(tag = "kind")]
pub(crate) enum OpenEditResult {
    #[serde(rename = "image")]
    Image { data_url: String, origin_dir: String, name: String },
    #[serde(rename = "video")]
    Video { copied_path: String, origin_dir: String, name: String },
}

// Ceiling on the key-sidecar JSON copied along with a working copy. EVENTS_CAP bounds what
// this app's own capture produces, so a file well past it is corrupted or foreign and not
// worth reading into memory.
#[cfg(windows)]
pub(crate) const KEY_SIDECAR_MAX_COPY_BYTES: u64 = 16 * 1024 * 1024;

// Whether `path` is safe to carry along as a key-overlay sidecar: present, under the size
// ceiling, and parseable as the KeySidecar shape read_key_sidecar expects. Pure (no
// destination involved) so a test can prove an oversized or malformed file is rejected.
#[cfg(windows)]
pub(crate) fn key_sidecar_is_safe_to_copy(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else { return false };
    if !meta.is_file() || meta.len() > KEY_SIDECAR_MAX_COPY_BYTES {
        return false;
    }
    fs::read_to_string(path).ok().and_then(|data| parse_key_sidecar(&data)).is_some()
}

// A video opened this way is a copy (see open_file_in_editor), and has_keys_sidecar and
// read_key_sidecar only look for "<video>.keys.json" beside the path the editor was handed,
// so the original recording's sidecar is copied along. A source with no sidecar leaves the
// copy without one too.
#[cfg(windows)]
pub(crate) fn copy_keys_sidecar_alongside(src: &Path, dest: &Path) {
    let src_sidecar = sidecar_path(src, KEYS_SIDECAR_EXT);
    if key_sidecar_is_safe_to_copy(&src_sidecar) {
        let _ = fs::copy(&src_sidecar, sidecar_path(dest, KEYS_SIDECAR_EXT));
    }
}
#[cfg(not(windows))]
pub(crate) fn copy_keys_sidecar_alongside(_src: &Path, _dest: &Path) {}

// A dual-audio recording the editor has not muxed yet keeps its microphone track in
// "<name>.mic.wav" beside the video. The working copy needs it too, or the export drops the
// microphone. Held to the same size ceiling as the video, and a copy that fails halfway is
// not left behind.
pub(crate) fn copy_mic_sidecar_alongside(src: &Path, dest: &Path) {
    copy_mic_sidecar_capped(src, dest, EDITOR_OPEN_MAX_MB);
}

pub(crate) fn copy_mic_sidecar_capped(src: &Path, dest: &Path, max_mb: f64) {
    let src_mic = src.with_extension("mic.wav");
    let dest_mic = dest.with_extension("mic.wav");
    let Ok(meta) = fs::metadata(&src_mic) else { return };
    if !meta.is_file() || exceeds_open_edit_limit(meta.len(), max_mb) {
        return;
    }
    if fs::copy(&src_mic, &dest_mic).is_err() {
        let _ = fs::remove_file(&dest_mic);
    }
}

// Whether `path` is a file inside one of the throwaway working folders open_file_in_editor
// makes (directly under the app's temp folder). Its copy carries the opened file's name, so
// it cannot pass is_own_recording_source, yet the editor has to mux the mic track into it.
pub(crate) fn is_editor_work_copy(path: &Path) -> bool {
    is_editor_work_copy_in(&std::env::temp_dir().join(brand::TEMP_DIR), path)
}

pub(crate) fn is_editor_work_copy_in(temp_root: &Path, path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let Some(dir) = path.parent().and_then(|d| d.canonicalize().ok()) else { return false };
    let Ok(root) = temp_root.canonicalize() else { return false };
    dir.parent() == Some(root.as_path())
        && dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(&format!("{}-openedit-", brand::TEMP_PREFIX)))
}

// Right-click "Open a file in the editor" (screenshot/record buttons, library attachments)
// lands here. The original is never touched: an image is read into a data URL for the same
// #snip-modal flow a fresh capture uses, and a video is copied once into its own throwaway
// working folder (swept by sweep_stale_temp_files), which open_video_editor is pointed at.
#[tauri::command]
pub(crate) async fn open_file_in_editor(app: AppHandle, path: String) -> Result<OpenEditResult, String> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err("file-missing".to_string());
    }
    let max_mb = EDITOR_OPEN_MAX_MB;
    let len = fs::metadata(&p).map_err(|e| e.to_string())?.len();
    if exceeds_open_edit_limit(len, max_mb) {
        return Err(format!("too-large:{}", max_mb.max(1.0) as u64));
    }
    let origin_dir = p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string();
    let ffprobe = ffprobe_search_path(&app);
    let kind = tauri::async_runtime::spawn_blocking({
        let p = p.clone();
        let ffprobe = ffprobe.clone();
        move || detect_media_kind(&p, ffprobe.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?;
    match kind {
        Some(("image", fmt)) => {
            // Reading (up to the size ceiling) and a possible re-encode: off the async runtime.
            let data_url = tauri::async_runtime::spawn_blocking({
                let p = p.clone();
                move || editor_image_data_url(&p, fmt)
            })
            .await
            .map_err(|e| e.to_string())??;
            Ok(OpenEditResult::Image { data_url, origin_dir, name })
        }
        Some(("video", _)) => {
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .filter(|e| VIDEO_FOLDER_EXTS.contains(&e.as_str()))
                .unwrap_or_else(|| "mp4".to_string());
            let base = clean_app_name(&name);
            let base = if base.is_empty() { "video".to_string() } else { base };
            let work_dir = std::env::temp_dir().join(brand::TEMP_DIR).join(format!("{}-openedit-{}-{}", brand::TEMP_PREFIX, local_stamp(), std::process::id()));
            fs::create_dir_all(&work_dir).map_err(|e| e.to_string())?;
            let dest = work_dir.join(format!("{base}.{ext}"));
            // A copy of up to the size ceiling plus its sidecars: off the async runtime's threads.
            tauri::async_runtime::spawn_blocking({
                let p = p.clone();
                let dest = dest.clone();
                move || {
                    fs::copy(&p, &dest).map_err(|e| e.to_string())?;
                    copy_keys_sidecar_alongside(&p, &dest);
                    copy_mic_sidecar_alongside(&p, &dest);
                    Ok::<(), String>(())
                }
            })
            .await
            .map_err(|e| e.to_string())??;
            Ok(OpenEditResult::Video { copied_path: dest.to_string_lossy().to_string(), origin_dir, name })
        }
        _ => Err("not-media".to_string()),
    }
}

// Target folder for a plain "Save": a custom expert path, else Videos\<app>. Mirrors
// screenshot_dir. The raw capture stays in <data>/recordings until explicitly saved or
// copied, and the sweep stays in charge of whatever is undecided there.
pub(crate) fn recording_dir(custom: &str) -> PathBuf {
    let c = custom.trim();
    if !c.is_empty() {
        return PathBuf::from(c);
    }
    let base = std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("Videos").join(brand::DISPLAY_NAME)
}

// Default recording folder path (shown greyed in the expert menu and the result window's
// folder line when no custom folder is set).
#[tauri::command]
pub(crate) fn default_recording_dir() -> String {
    recording_dir("").to_string_lossy().to_string()
}

// The recording folder's canonical form: the custom `recordingDir` setting, or the default
// `Videos\<app>`, resolved by Rust from its own settings rather than trusted from a caller's
// IPC argument. Every command that lists, cleans or deletes recordings checks its `dir`
// against this first, so a call with another folder string can only touch the one folder
// this app keeps recordings in.
pub(crate) fn configured_recording_dir(state: &State<'_, Db>) -> PathBuf {
    let custom = lock(state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    recording_dir(&custom)
}

// Whether `candidate` really is `configured`: canonicalized when the folder exists (defeats a
// short-name, trailing-dot or symlink mismatch), a case-insensitive comparison when it does
// not exist yet, since there is nothing to list, clean or delete in a folder never created.
pub(crate) fn path_matches_configured_dir(candidate: &Path, configured: &Path) -> bool {
    match (fs::canonicalize(candidate), fs::canonicalize(configured)) {
        (Ok(a), Ok(b)) => a == b,
        _ => {
            let norm = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_ascii_lowercase();
            norm(candidate) == norm(configured)
        }
    }
}

// Only the extensions this app writes into the recording folder. The user can point it at
// any folder (even one full of unrelated files), so the size readout and cleanup must never
// count or delete anything this app did not create.
pub(crate) const VIDEO_FOLDER_EXTS: [&str; 8] = ["mp4", "webm", "gif", "avi", "mov", "mkv", "webp", "apng"];

pub(crate) fn is_video_folder_file(path: &std::path::Path) -> bool {
    // The extension alone would still let any other file with a matching extension (a real video
    // the user already had there, or one from another app) be counted and swept up by the
    // automatic cleanup. discard_recording_file_in treats the "rec-" stem as the proof a file is
    // this app's recording, and stats, cleanup and the discard trash listing require the same.
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.starts_with("rec-")
        && path.is_file()
        && path
            .extension()
            .map(|e| VIDEO_FOLDER_EXTS.contains(&e.to_string_lossy().to_lowercase().as_str()))
            .unwrap_or(false)
}

pub(crate) fn video_folder_stats_in(dir: &std::path::Path) -> (u64, u64) {
    let Ok(entries) = fs::read_dir(dir) else { return (0, 0) };
    let mut count = 0u64;
    let mut bytes = 0u64;
    for e in entries.flatten() {
        let path = e.path();
        if !is_video_folder_file(&path) { continue; }
        if let Ok(meta) = e.metadata() {
            count += 1;
            bytes += meta.len();
        }
    }
    (count, bytes)
}

// (file count, total bytes) of every recording in the folder; a missing folder is empty, not
// an error. It includes the "verworfen" discard-trash subfolder, which sits on the same
// drive, so leaving it out would make the displayed folder size wrong after any discard.
// `configured` is Rust's resolved recording folder (see `configured_recording_dir`); `dir`
// is trusted only once proven to be that folder. Kept apart from the tauri::command wrapper
// so it is unit-testable without a running app.
pub(crate) fn video_folder_stats_impl(dir: &str, configured: &Path) -> (u64, u64) {
    let base = std::path::Path::new(dir);
    if !path_matches_configured_dir(base, configured) {
        return (0, 0);
    }
    let (c1, b1) = video_folder_stats_in(base);
    let (c2, b2) = video_folder_stats_in(&base.join(crate::stats::DISCARD_SUBDIR));
    (c1 + c2, b1 + b2)
}

#[tauri::command(async)]
pub(crate) fn video_folder_stats(state: State<'_, Db>, dir: String) -> (u64, u64) {
    video_folder_stats_impl(&dir, &configured_recording_dir(&state))
}

#[derive(Serialize)]
pub(crate) struct VideoFolderFile {
    pub(crate) name: String,
    pub(crate) size: u64,
}

// Shared by the preview and the real delete, so what the user confirms is exactly what is
// removed.
pub(crate) fn video_folder_stale(dir: &str, age_days: u32) -> Vec<(std::path::PathBuf, VideoFolderFile)> {
    let age = Duration::from_secs(u64::from(age_days) * 24 * 3600);
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if !is_video_folder_file(&path) { continue; }
        let Ok(meta) = e.metadata() else { continue };
        let Ok(modified) = meta.modified() else { continue };
        let Ok(elapsed) = SystemTime::now().duration_since(modified) else { continue };
        if elapsed < age { continue; }
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        out.push((path, VideoFolderFile { name, size: meta.len() }));
    }
    out
}

pub(crate) fn video_folder_cleanup_preview_impl(dir: &str, age_days: u32, configured: &Path) -> Vec<VideoFolderFile> {
    if !path_matches_configured_dir(std::path::Path::new(dir), configured) {
        return Vec::new();
    }
    video_folder_stale(dir, age_days).into_iter().map(|(_, f)| f).collect()
}

#[tauri::command(async)]
pub(crate) fn video_folder_cleanup_preview(state: State<'_, Db>, dir: String, age_days: u32) -> Vec<VideoFolderFile> {
    video_folder_cleanup_preview_impl(&dir, age_days, &configured_recording_dir(&state))
}

// A single file name of the kind this app writes into the recording folder: no path part, the
// "rec-" stem and a recording extension. IPC callers pass names, so anything path-like
// (separators, "..", a drive prefix) is out.
pub(crate) fn is_bare_recording_name(name: &str) -> bool {
    let p = Path::new(name);
    p.file_name().and_then(|n| n.to_str()) == Some(name)
        && name.starts_with("rec-")
        && p.extension().is_some_and(|e| VIDEO_FOLDER_EXTS.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

// Deletes only files the confirmation dialog listed (`names`) that are still stale: a file
// that crossed the age threshold after the list was shown stays for next time.
pub(crate) fn video_folder_cleanup_impl(dir: &str, age_days: u32, names: &[String], configured: &Path) -> usize {
    if !path_matches_configured_dir(std::path::Path::new(dir), configured) {
        return 0;
    }
    let stale = video_folder_stale(dir, age_days);
    let mut n = 0;
    for (path, file) in stale {
        if !is_bare_recording_name(&file.name) || !names.contains(&file.name) {
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            n += 1;
            // The sidecars only mean something next to their video; left behind they would have no owner.
            let _ = fs::remove_file(path.with_extension("mic.wav"));
            let _ = fs::remove_file(sidecar_path(&path, KEYS_SIDECAR_EXT));
            let _ = fs::remove_file(sidecar_path(&path, CAM_SIDECAR_EXT));
        }
    }
    n
}

#[tauri::command(async)]
pub(crate) fn video_folder_cleanup(state: State<'_, Db>, dir: String, age_days: u32, names: Vec<String>) -> usize {
    video_folder_cleanup_impl(&dir, age_days, &names, &configured_recording_dir(&state))
}

// The batch-export dialog needs each selected recording's real duration for
// export_recordings_batch's per-item total_ms and the disk-space check; this reuses the
// ffprobe path probe_media_meta uses. A file that cannot be probed (missing, corrupt) comes
// back as 0, probe_media_meta's "unknown", rather than failing the batch. `dir` must be the
// configured recording folder and every name a bare recording file name; anything else
// reads as 0 without being probed.
pub(crate) fn video_folder_file_durations_ms_impl(ffprobe: Option<&Path>, dir: &Path, names: &[String], configured: &Path) -> Vec<u64> {
    use rayon::prelude::*;
    let dir_ok = path_matches_configured_dir(dir, configured);
    // Probed in parallel (each one is a child process); collect keeps the order of `names`.
    names
        .par_iter()
        .map(|name| if dir_ok && is_bare_recording_name(name) { probe_media_meta(ffprobe, &dir.join(name)).0 } else { 0 })
        .collect()
}

#[tauri::command]
pub(crate) async fn video_folder_file_durations_ms(app: AppHandle, dir: String, names: Vec<String>) -> Vec<u64> {
    let ffprobe = ffprobe_search_path(&app);
    let configured = configured_recording_dir(&app.state::<Db>());
    // Off the async runtime: one process spawn per file would otherwise hold a worker.
    tauri::async_runtime::spawn_blocking(move || {
        video_folder_file_durations_ms_impl(ffprobe.as_deref(), Path::new(&dir), &names, &configured)
    })
    .await
    .unwrap_or_default()
}

// Never overwrite an existing file at the target: "<stem>.<ext>", then "<stem> (2).<ext>",
// "<stem> (3).<ext>", and so on. Shared by every result-dialog button that writes into a
// folder rather than a dialog-chosen path.
// The free name is claimed when found: a plain exists() check lets a file appear at that
// name before a later fs::rename onto it, which overwrites on Windows. create_new claims the
// name atomically and moves to the next candidate on that race. The empty placeholder it
// leaves must be replaced (or removed on failure) by the caller.
pub(crate) fn reserve_unique_dest_path(dir: &std::path::Path, stem: &str, ext: &str) -> std::io::Result<PathBuf> {
    let mut n: u32 = 1;
    loop {
        let candidate = if n == 1 { dir.join(format!("{stem}.{ext}")) } else { dir.join(format!("{stem} ({n}).{ext}")) };
        match fs::OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(_) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e),
        }
    }
}

// fs::rename fails across drives (recordingDir can be anywhere, unlike the same-drive
// <data>/recordings), so fall back to copy+remove.
pub(crate) fn move_file(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    fs::copy(src, dst)?;
    fs::remove_file(src)
}

// Save/Save As/Copy relocate the video, so its mic sidecar (a dual-audio capture not yet
// muxed by the editor) must follow, or it has no owner and is swept as an orphan, losing the
// microphone track. Best-effort: the primary move already succeeded, so a sidecar failure is
// logged, not propagated.
pub(crate) fn move_mic_sidecar_alongside(old_video: &std::path::Path, new_video: &std::path::Path) {
    let old_sidecar = old_video.with_extension("mic.wav");
    if old_sidecar.is_file() {
        let new_sidecar = new_video.with_extension("mic.wav");
        if old_sidecar != new_sidecar {
            if let Err(e) = move_file(&old_sidecar, &new_sidecar) {
                eprintln!("failed to relocate mic sidecar: {e}");
            }
        }
    }
    // The key-overlay sidecar (if this take wrote one) travels the same way, or it would be
    // orphaned at the old path.
    #[cfg(windows)]
    {
        let old_keys = sidecar_path(old_video, KEYS_SIDECAR_EXT);
        if old_keys.is_file() {
            let new_keys = sidecar_path(new_video, KEYS_SIDECAR_EXT);
            if old_keys != new_keys {
                if let Err(e) = move_file(&old_keys, &new_keys) {
                    eprintln!("failed to relocate key-overlay sidecar: {e}");
                }
            }
        }
    }
    // Same for the camera sidecar, on whichever recording (screen or camera) is being relocated.
    let old_cam = sidecar_path(old_video, CAM_SIDECAR_EXT);
    if old_cam.is_file() {
        let new_cam = sidecar_path(new_video, CAM_SIDECAR_EXT);
        if old_cam != new_cam {
            if let Err(e) = move_file(&old_cam, &new_cam) {
                eprintln!("failed to relocate camera sidecar: {e}");
            }
        }
    }
}

// Shared by Save/Copy: a file already in the target folder (e.g. Copy after a Save) keeps its
// name; otherwise it moves in under a name that never overwrites anything.
pub(crate) fn place_in_dir(src: &std::path::Path, dir: &std::path::Path) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let same_dir = src
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .zip(dir.canonicalize().ok())
        .map(|(a, b)| a == b)
        .unwrap_or(false);
    if same_dir {
        return Ok(src.to_path_buf());
    }
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let stem = src.file_stem().and_then(|e| e.to_str()).unwrap_or("Recording");
    // The name is claimed up front, so a second move at the same moment takes the next free one
    // instead of replacing this file.
    let dest = reserve_unique_dest_path(dir, stem, ext)?;
    if let Err(e) = move_file(src, &dest) {
        let _ = fs::remove_file(&dest);
        return Err(e);
    }
    move_mic_sidecar_alongside(src, &dest);
    Ok(dest)
}

// Post-capture "Save as..." for a recording already in the recordings folder (raw or
// converted): moves it where the user picks, like save_screenshot_as but from a real file.
// It moves rather than copies so the raw capture does not linger in <data>/recordings; the
// frontend updates its pending path afterwards so a later Attach/Copy still finds the file.
#[tauri::command]
pub(crate) async fn save_recording_as(app: AppHandle, state: State<'_, Db>, win: tauri::Window, src_path: String) -> Result<Option<String>, String> {
    let src = PathBuf::from(&src_path);
    if !src.is_file() {
        return Err("recording file not found".to_string());
    }
    // move_file relocates this file wherever the save dialog points, so refuse anything not
    // proven to be this app's own recording in a recording folder, or a file this session's
    // export_video wrote. An export can land outside any recording folder once
    // export_dest_dir_is_authorized cleared it, so the folder rule alone would refuse "Save as"
    // right after a legitimate export.
    if !is_own_recording_source(&recording_source_dirs(&app, &state), &src) && !is_known_export_output(&src) {
        return Err("source-not-allowed".to_string());
    }
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("mp4").to_string();
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| format!("Recording.{ext}"));
    // The dialog is modal: it waits on a blocking thread, not on an async worker.
    let dialog_app = app.clone();
    let dialog_label = win.label().to_string();
    let result = tauri::async_runtime::spawn_blocking(move || {
        file_dialog_for(&dialog_app, &dialog_label).add_filter(ext.to_uppercase(), &[ext.as_str()]).set_file_name(&name).save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = win.set_focus();
    match result {
        Some(p) => {
            move_tracked_export_output(&src, &p).map_err(|e| e.to_string())?;
            move_mic_sidecar_alongside(&src, &p);
            *LAST_SAVE_AS_DIR.lock().unwrap_or_else(|e| e.into_inner()) = p.parent().map(Path::to_path_buf);
            Ok(Some(p.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

// Folder of the latest "Save as" the user picked, so the earlier parts of the same take can
// follow the last file there without the renderer naming a destination.
pub(crate) static LAST_SAVE_AS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

// Moves each part into `dest_dir` (name kept, so the part suffix stays) with its
// sidecars. A part that was saved leaves the list; one that could not be moved
// stays for another try. Returns (saved, failed).
pub(crate) fn place_recording_parts(parts: &mut Vec<RecordingPartInfo>, allowed: &[PathBuf], dest_dir: &Path) -> (u32, u32) {
    let (mut saved, mut failed) = (0u32, 0u32);
    parts.retain(|part| {
        let src = Path::new(&part.path);
        if !is_own_recording_source(allowed, src) {
            failed += 1;
            return true;
        }
        match place_in_dir(src, dest_dir) {
            Ok(_) => {
                saved += 1;
                false
            }
            Err(_) => {
                failed += 1;
                true
            }
        }
    });
    (saved, failed)
}

// Quit during a recording: the take is stopped first and no result dialog will ask what to
// do with it, so it is saved as the dialog's Save would (last file and earlier parts into
// the recordings folder) rather than left in staging, where the stale sweep deletes it.
pub(crate) fn save_take_on_quit(app: &AppHandle, take: &RecordingStoppedPayload) {
    let state = app.state::<Db>();
    let allowed = recording_source_dirs(app, &state);
    let custom = lock(&state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    let dest = recording_dir(&custom);
    // A Save from the result dialog may still be copying earlier parts: let it finish first, or
    // the exit would kill the copy and leave the parts in staging for the sweep.
    if !wait_for_part_saves(REC_PARTS_QUIT_WAIT) {
        append_crash_log("quit: a recording part save was still running");
    }
    let src = Path::new(&take.path);
    if src.is_file() && is_own_recording_source(&allowed, src) {
        if let Err(e) = place_in_dir(src, &dest) {
            append_crash_log(&format!("quit: recording not saved: {e}"));
        }
    }
    let mut parts = REC_RESULT_PARTS.lock().unwrap_or_else(|e| e.into_inner());
    let (_, failed) = place_recording_parts(&mut parts, &allowed, &dest);
    if failed > 0 {
        append_crash_log(&format!("quit: {failed} recording part(s) not saved"));
    }
}

// Save/Attach/Edit/Save as in the result dialog: the earlier parts of a "finish and continue"
// take go where the last file went. `beside_save_as` picks the folder of the latest Save as
// instead of the recordings folder.
#[tauri::command]
pub(crate) async fn save_recording_parts(app: AppHandle, state: State<'_, Db>, beside_save_as: bool) -> Result<(u32, u32), String> {
    let allowed = recording_source_dirs(&app, &state);
    let custom = lock(&state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    let dest = if beside_save_as {
        LAST_SAVE_AS_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(|| recording_dir(&custom))
    } else {
        recording_dir(&custom)
    };
    // A part can be many gigabytes and the move copies across drives: the list is taken out so
    // the lock is not held for that, and the worker thread does the copying.
    tauri::async_runtime::spawn_blocking(move || {
        with_parts_outside_lock(&REC_RESULT_PARTS, |parts| place_recording_parts(parts, &allowed, &dest))
    })
    .await
    .map_err(|e| e.to_string())
}

// How long a quit waits for a running parts save before it goes on.
pub(crate) const REC_PARTS_QUIT_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

// Held for as long as a parts save runs: a second Save waits for the first instead of finding an
// empty list, and the quit path waits on it too.
static REC_PARTS_GATE: Mutex<()> = Mutex::new(());

// True when no parts save is running, or the one that was finished within `limit`.
pub(crate) fn wait_for_part_saves(limit: std::time::Duration) -> bool {
    let start = std::time::Instant::now();
    loop {
        match REC_PARTS_GATE.try_lock() {
            Err(std::sync::TryLockError::WouldBlock) => {}
            _ => return true,
        }
        if start.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

// Keeps the taken parts visible to the stale sweep while they are copied.
struct PartsInFlight;
impl Drop for PartsInFlight {
    fn drop(&mut self) {
        REC_PARTS_SAVING.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

// Runs `work` on the shared list's current contents without keeping the lock: the entries are
// taken out first, and whatever `work` leaves in the list (parts that could not be moved) is
// put back in front of anything added meanwhile. The taken parts stay listed as in flight (so
// the stale sweep leaves them alone) and a gate lets the quit path wait for the save.
pub(crate) fn with_parts_outside_lock<R>(
    shared: &Mutex<Vec<RecordingPartInfo>>,
    work: impl FnOnce(&mut Vec<RecordingPartInfo>) -> R,
) -> R {
    let _gate = REC_PARTS_GATE.lock().unwrap_or_else(|e| e.into_inner());
    let mut mine = std::mem::take(&mut *shared.lock().unwrap_or_else(|e| e.into_inner()));
    *REC_PARTS_SAVING.lock().unwrap_or_else(|e| e.into_inner()) = mine.clone();
    let _mark = PartsInFlight;
    let result = work(&mut mine);
    if !mine.is_empty() {
        let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
        mine.append(&mut guard);
        *guard = mine;
    }
    result
}

// Discard in the result dialog: the earlier parts go to the discard folder like the last
// file. Returns how many were moved.
pub(crate) fn discard_recording_parts_in(parts: &mut Vec<RecordingPartInfo>, allowed: &[PathBuf]) -> u32 {
    let mut moved = 0u32;
    parts.retain(|part| {
        let ok = discard_recording_file_in(allowed, Path::new(&part.path)).is_some();
        if ok {
            moved += 1;
        }
        !ok
    });
    moved
}

#[tauri::command]
pub(crate) fn discard_recording_parts(app: AppHandle, state: State<'_, Db>) -> u32 {
    let allowed = recording_source_dirs(&app, &state);
    let mut parts = REC_RESULT_PARTS.lock().unwrap_or_else(|e| e.into_inner());
    discard_recording_parts_in(&mut parts, &allowed)
}

// The folder picker for the video editor's "choose folder" export target and the batch-export
// dialog. A wrapper around the picker `pick_folder` uses that also remembers the folder as an
// authorized export destination; `pick_folder` stays untouched since other picks
// (attachments, imports, the recordings folder setting) must not widen what an export
// accepts.
#[tauri::command]
pub(crate) async fn pick_export_folder(app: AppHandle, win: tauri::Window) -> Option<String> {
    let label = win.label().to_string();
    let result = tauri::async_runtime::spawn_blocking(move || file_dialog_for(&app, &label).pick_folder()).await.ok().flatten();
    let _ = win.set_focus();
    if let Some(dir) = &result {
        // remember_export_dest_dir authorizes exporting into this folder right away
        // (export_dest_dir_is_authorized checks it directly); the grant below is the separate
        // one-time authority set_video_export_dir needs if the caller saves this folder as the
        // standing preference.
        remember_export_dest_dir(dir);
        grant_picked_dir(dir, "videoExport");
    }
    result.map(|p| p.to_string_lossy().to_string())
}

// Plain "Save": writes straight into recordingDir without a dialog (the common case);
// save_recording_as keeps the free dialog for everything else.
#[tauri::command]
pub(crate) async fn save_recording_now(app: AppHandle, state: State<'_, Db>, src_path: String) -> Result<String, String> {
    let src = PathBuf::from(&src_path);
    if !src.is_file() {
        return Err("recording file not found".to_string());
    }
    // place_in_dir moves the file, so refuse anything not proven to be this app's own recording
    // where its capture or an earlier Save/Copy left it.
    if !is_own_recording_source(&recording_source_dirs(&app, &state), &src) {
        return Err("source-not-allowed".to_string());
    }
    let custom = lock(&state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    let dest = place_in_dir(&src, &recording_dir(&custom)).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

// "Copy", the video counterpart of the screenshot dialog's clipboard copy. A video cannot be
// a bitmap, so the file itself is the clipboard payload (the CF_HDROP path copy_prompt uses
// for any file-attached prompt). It is moved into recordingDir first so the pasted reference
// still resolves after the window closes (see discard_recording_file and closeRecModal on
// the frontend: a copy alone does not count as "saving", so closing without Save, Save
// as... or Attach still deletes it).
//
// The move happens before the clipboard write and is not undone if that write fails, so the
// new location is always returned with a success flag instead of folded into the Err case.
// Otherwise the frontend would keep pointing at the vanished original path and every later
// action would fail with "file not found" although the recording still exists at `dest`.
#[tauri::command]
pub(crate) async fn copy_recording_to_clipboard(app: AppHandle, state: State<'_, Db>, src_path: String) -> Result<(String, bool), String> {
    let src = PathBuf::from(&src_path);
    if !src.is_file() {
        return Err("recording file not found".to_string());
    }
    // Same as save_recording_now: the source must be proven this app's own recording before
    // place_in_dir relocates it.
    if !is_own_recording_source(&recording_source_dirs(&app, &state), &src) {
        return Err("source-not-allowed".to_string());
    }
    let custom = lock(&state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    let dest = place_in_dir(&src, &recording_dir(&custom)).map_err(|e| e.to_string())?;
    let dest_str = dest.to_string_lossy().to_string();
    let clip_ok = set_clipboard_file(&dest_str);
    Ok((dest_str, clip_ok))
}

// The video editor's "Copy to clipboard" export target: a throwaway file in the app's temp
// folder rather than anywhere the user chose, so it can be swept automatically (see
// sweep_clip_export_temp) without touching a real save.
pub(crate) fn clip_export_path_impl(ext: &str) -> Result<String, String> {
    let dir = std::env::temp_dir().join(brand::TEMP_DIR);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe_ext = if !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric()) { ext } else { "mp4" };
    let name = format!("{}-clipexport-{}.{safe_ext}", brand::TEMP_PREFIX, gen_id());
    Ok(dir.join(name).to_string_lossy().to_string())
}

#[tauri::command]
pub(crate) async fn clip_export_path(ext: String) -> Result<String, String> {
    clip_export_path_impl(&ext)
}

// Thin wrapper so the frontend needs no Windows-only code path; same CF_HDROP mechanism as
// copy_recording_to_clipboard.
#[tauri::command]
pub(crate) async fn copy_file_to_clipboard(path: String) -> bool {
    let own_temp_dir = std::env::temp_dir().join(brand::TEMP_DIR);
    let p = Path::new(&path);
    if !is_clip_export_file(&own_temp_dir, p) && !is_known_export_output(p) {
        return false;
    }
    set_clipboard_file(&path)
}

// The per-user, non-roaming app folder (LocalAppData, then the app name): where native
// helpers the app loads or runs (pdfium, ffmpeg) are kept. Unlike the temp folder it is not
// writable by other users' processes, and unlike the data folder it never holds user content
// or follows a redirected store. None when the profile folder is not set.
pub(crate) fn local_app_dir() -> Option<PathBuf> {
    local_app_dir_in(std::env::var_os("LOCALAPPDATA"))
}

pub(crate) fn local_app_dir_in(base: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let base = base.filter(|b| !b.is_empty())?;
    Some(PathBuf::from(base).join(brand::DATA_FOLDER))
}

// Whether `path` is a file clip_export_path_impl hands out: a clip-export name directly in the
// app's temp folder. The parent is compared canonicalized so a `..` hop cannot slip out.
pub(crate) fn is_clip_export_file(temp_root: &Path, path: &Path) -> bool {
    let prefix = format!("{}-clipexport-", brand::TEMP_PREFIX);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !name.starts_with(&prefix) || !path.is_file() {
        return false;
    }
    let Some(parent) = path.parent().and_then(|d| d.canonicalize().ok()) else { return false };
    temp_root.canonicalize().map(|c| c == parent).unwrap_or(false)
}

// recDiscardKeepH has no expert-menu row yet (like recCanvasMode above); 0 disables the
// whole discard-trash cleanup, keeping everything in "verworfen" forever.
pub(crate) fn discard_keep_hours(state: &State<'_, Db>) -> f64 {
    lock(state).settings.ui_values.get("recDiscardKeepH").copied().unwrap_or(24.0)
}

// The two folders a genuine recording can sit in while a recording command may act on it by
// an IPC path: the capture staging folder (<data>/recordings, where a fresh recording first
// lands) and the configured recordingDir (where Save/Copy leave it before the result window
// closes). Shared by discard/convert/save/copy so they draw the line in one place.
pub(crate) fn recording_source_dirs(app: &AppHandle, state: &State<'_, Db>) -> [PathBuf; 2] {
    let custom = lock(state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    [data_dir(app).join("recordings"), recording_dir(&custom)]
}

// Whether `path` is a recording this app produced, in one of `allowed`'s folders, rather than
// an arbitrary file named in a recording command's IPC argument. It is the same "rec-" stem
// plus allowed-parent proof discard_recording_file_in requires, so convert_recording,
// save_recording_now and copy_recording_to_clipboard cannot be made to move, replace or feed
// ffmpeg an unrelated file by a subverted renderer.
pub(crate) fn is_own_recording_source(allowed: &[PathBuf], path: &Path) -> bool {
    is_recording_source_location(allowed, path) && path.is_file()
}

// The location half of is_own_recording_source: the "rec-" stem and an allowed parent, decided
// without ever asking the file system about the file itself. A network (UNC) path is refused
// on its text alone (every spelling, through the one shared gate), so a refused caller path
// never makes this machine contact a remote host.
pub(crate) fn is_recording_source_location(allowed: &[PathBuf], path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let raw = path.to_string_lossy();
    if !name.starts_with("rec-") || is_network_path(&raw) {
        return false;
    }
    let Some(parent) = path.parent().and_then(|d| d.canonicalize().ok()) else { return false };
    allowed.iter().any(|d| d.canonicalize().map(|c| c == parent).unwrap_or(false))
}

// Whether `sidecar` is exactly the mic capture belonging to `video`, the only sidecar
// mux_mic_track may consume and delete. Checked apart from is_own_recording_source (which
// proves only `video`) so an unrelated path in an allowed folder cannot ride along.
pub(crate) fn is_own_mic_sidecar(video: &Path, sidecar: &Path) -> bool {
    sidecar == video.with_extension("mic.wav")
}

// Split out so the move logic has a unit test against a plain tempdir, no AppHandle/Db
// needed. Returns the file's new path in "verworfen" on success.
pub(crate) fn discard_recording_file_in(allowed: &[PathBuf], path: &std::path::Path) -> Option<PathBuf> {
    // The directory check alone would let this touch any file directly inside recordingDir, often
    // the user's normal Videos folder, shared with files this app never made. Every real
    // recording (raw, saved, converted, or moved by Copy) keeps the "rec-" stem that
    // rec_filename/place_in_dir give it; only Save As lets the user pick another name, and that
    // sets recSavedOrAttached first, taking the file out of discard's reach.
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !name.starts_with("rec-") {
        return None;
    }
    let parent = path.parent().and_then(|d| d.canonicalize().ok())?;
    let in_allowed = allowed.iter().any(|d| d.canonicalize().map(|c| c == parent).unwrap_or(false));
    if !in_allowed || !path.is_file() {
        return None;
    }
    let trash = parent.join(crate::stats::DISCARD_SUBDIR);
    fs::create_dir_all(&trash).ok()?;
    let dest = trash.join(name);
    move_file(path, &dest).ok()?;
    // A discarded recording takes its mic and key-overlay sidecars (if any) with it, best-effort
    // since most recordings have neither. Both exist only next to an mp4.
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if ext.eq_ignore_ascii_case("mp4") {
            let sidecar = path.with_extension("mic.wav");
            if sidecar.is_file() {
                let _ = move_file(&sidecar, &dest.with_extension("mic.wav"));
            }
            #[cfg(windows)]
            {
                let keys = sidecar_path(path, KEYS_SIDECAR_EXT);
                if keys.is_file() {
                    let _ = move_file(&keys, &sidecar_path(&dest, KEYS_SIDECAR_EXT));
                }
            }
        }
    }
    // The camera sidecar takes the same trip regardless of container (camera recordings are webm),
    // so it cannot sit inside the ext-gated block above.
    let cam_sidecar = sidecar_path(path, CAM_SIDECAR_EXT);
    if cam_sidecar.is_file() {
        let _ = move_file(&cam_sidecar, &sidecar_path(&dest, CAM_SIDECAR_EXT));
    }
    Some(dest)
}

// A cancelled take goes to its folder's holding area like a discarded one, so a mistaken
// Cancel can be undone. A file that cannot be moved there is removed with every sidecar.
pub(crate) fn discard_or_remove_take(path: &std::path::Path) {
    let moved = path.parent().and_then(|dir| discard_recording_file_in(&[dir.to_path_buf()], path)).is_some();
    if !moved {
        let _ = fs::remove_file(path);
        for ext in ["mic.wav", KEYS_SIDECAR_EXT, CAM_SIDECAR_EXT] {
            let _ = fs::remove_file(sidecar_path(path, ext));
        }
    }
}

// Discard a not-yet-saved recording. Scoped to the app's recordings folder or the current
// recordingDir (Copy/Save move the working file there before the window closes), never an
// arbitrary webview-supplied path (defense in depth, as delete_screenshot_file). It moves
// into a "verworfen" subfolder of whichever it was in rather than deleting, so a wrong click
// can be undone (see restore_discarded_recording; the sweep in sweep_recordings_now_result
// eventually clears the subfolder).
#[tauri::command]
pub(crate) fn discard_recording_file(app: AppHandle, state: State<'_, Db>, path: String) -> Option<f64> {
    let custom = lock(&state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    let allowed = [data_dir(&app).join("recordings"), recording_dir(&custom)];
    discard_recording_file_in(&allowed, std::path::Path::new(&path))?;
    Some(discard_keep_hours(&state))
}

// Bare filename only, never a path. Callers reach into a folder's "verworfen" subfolder with a
// name from that folder's own listing; this stops a hand-crafted "../x" from escaping it.
pub(crate) fn is_bare_filename(name: &str) -> bool {
    !name.is_empty() && std::path::Path::new(name).file_name().is_some_and(|f| f == std::ffi::OsStr::new(name))
}

// The recordings-folder dialog's listing of what sits in the trash waiting out its retention
// window; same shape and extension filter as video_folder_stats/video_folder_cleanup_preview.
pub(crate) fn list_discarded_recordings_impl(dir: &str, configured: &Path) -> Vec<VideoFolderFile> {
    if !path_matches_configured_dir(std::path::Path::new(dir), configured) {
        return Vec::new();
    }
    let trash = std::path::Path::new(dir).join(crate::stats::DISCARD_SUBDIR);
    let Ok(entries) = fs::read_dir(&trash) else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if !is_video_folder_file(&path) { continue; }
        let Ok(meta) = e.metadata() else { continue };
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        out.push(VideoFolderFile { name, size: meta.len() });
    }
    out
}

#[tauri::command(async)]
pub(crate) fn list_discarded_recordings(state: State<'_, Db>, dir: String) -> Vec<VideoFolderFile> {
    list_discarded_recordings_impl(&dir, &configured_recording_dir(&state))
}

pub(crate) fn restore_discarded_recording_impl(dir: &str, name: &str, configured: &Path) -> bool {
    if !is_bare_filename(name) {
        return false;
    }
    if !path_matches_configured_dir(std::path::Path::new(dir), configured) {
        return false;
    }
    let trash = std::path::Path::new(dir).join(crate::stats::DISCARD_SUBDIR);
    let src = trash.join(name);
    let dst = std::path::Path::new(dir).join(name);
    // Never overwrites a same-named file that appeared at the old spot since the discard:
    // restoring on top of someone else's recording would be worse than the restore failing.
    if !src.is_file() || dst.exists() || move_file(&src, &dst).is_err() {
        return false;
    }
    let sidecar_src = src.with_extension("mic.wav");
    if sidecar_src.is_file() {
        let _ = move_file(&sidecar_src, &dst.with_extension("mic.wav"));
    }
    // The key-overlay sidecar leaves the trash with its video instead of staying orphaned.
    let keys_src = sidecar_path(&src, KEYS_SIDECAR_EXT);
    if keys_src.is_file() {
        let _ = move_file(&keys_src, &sidecar_path(&dst, KEYS_SIDECAR_EXT));
    }
    let cam_sidecar_src = sidecar_path(&src, CAM_SIDECAR_EXT);
    if cam_sidecar_src.is_file() {
        let _ = move_file(&cam_sidecar_src, &sidecar_path(&dst, CAM_SIDECAR_EXT));
    }
    true
}

#[tauri::command]
pub(crate) fn restore_discarded_recording(state: State<'_, Db>, dir: String, name: String) -> bool {
    restore_discarded_recording_impl(&dir, &name, &configured_recording_dir(&state))
}

pub(crate) fn delete_discarded_recording_now_impl(dir: &str, name: &str, configured: &Path) -> bool {
    if !is_bare_filename(name) {
        return false;
    }
    if !path_matches_configured_dir(std::path::Path::new(dir), configured) {
        return false;
    }
    let trash = std::path::Path::new(dir).join(crate::stats::DISCARD_SUBDIR);
    let path = trash.join(name);
    if !path.is_file() {
        return false;
    }
    let _ = fs::remove_file(path.with_extension("mic.wav"));
    // "Delete for good" removes the key-overlay sidecar with its video.
    let _ = fs::remove_file(sidecar_path(&path, KEYS_SIDECAR_EXT));
    let _ = fs::remove_file(sidecar_path(&path, CAM_SIDECAR_EXT));
    fs::remove_file(&path).is_ok()
}

#[tauri::command]
pub(crate) fn delete_discarded_recording_now(state: State<'_, Db>, dir: String, name: String) -> bool {
    delete_discarded_recording_now_impl(&dir, &name, &configured_recording_dir(&state))
}

// Called by the frontend once the first render + text fit is complete.
#[tauri::command]
pub(crate) fn show_main_window(app: AppHandle) {
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
pub(crate) async fn edit_prompt_request(app: AppHandle, id: String) -> Result<(), String> {
    show_main(&app);
    let _ = app.emit("edit-prompt", id);
    Ok(())
}
