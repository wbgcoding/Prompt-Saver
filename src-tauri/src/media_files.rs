use super::*;

#[tauri::command]
pub(crate) fn video_editor_source(pending: State<VideoEditorPending>) -> Option<VideoEditorSource> {
    let (path, origin_dir, overlay_path) = pending.lock().clone();
    let overlay_path = if overlay_path.is_empty() { None } else { Some(overlay_path) };
    if path.is_empty() { None } else { Some(VideoEditorSource { path, origin_dir, overlay_path }) }
}

// ---- Open an existing file (not a fresh capture/recording) in whichever
// editor its actual content calls for. ----

// Largest file, in megabytes, the editors will open.
pub(crate) const EDITOR_OPEN_MAX_MB: f64 = 2048.0;

// True once a file is past the ceiling -- kept as its own pure
// function so the boundary (exactly at the limit still opens) is unit
// testable without a running app.
pub(crate) fn exceeds_open_edit_limit(bytes: u64, max_mb: f64) -> bool {
    let cap = ((max_mb.max(1.0)) as u64).saturating_mul(1024 * 1024);
    bytes > cap
}

// Content, never the extension, decides what opens: a real image (any of the
// formats the `image` crate reads, sniffed the same way decode_image_file
// already does) wins first since it is by far the cheaper check; only then is
// ffprobe asked whether this is a video it can read. `ffprobe` is None in a
// unit test that has no bundled
// ffprobe.exe, which correctly makes every video case fall through to "not
// media" there rather than false-negative on the image branch.
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

#[derive(Serialize)]
#[serde(tag = "kind")]
pub(crate) enum OpenEditResult {
    #[serde(rename = "image")]
    Image { data_url: String, origin_dir: String, name: String },
    #[serde(rename = "video")]
    Video { copied_path: String, origin_dir: String, name: String },
}

// A video opened this way is a COPY (see open_file_in_editor below), and
// has_keys_sidecar/read_key_sidecar only ever look for "<video>.keys.json"
// right beside whatever path the editor was actually handed -- without this,
// reopening a past recording through "open an existing file" (the composer
// button's right-click, or a library attachment) silently lost its typed
// keys even though the original recording's own sidecar was still sitting
// untouched next to the source file. A source with no sidecar (the setting
// was off, or the recording predates this feature) leaves the copy without
// one too, same as it always did.
// A ceiling on the key-sidecar JSON this app will ever copy along with a
// working copy. EVENTS_CAP already bounds how large a sidecar this app's own
// capture can produce, so a file well past that is either corrupted or from
// somewhere else entirely -- either way not worth reading into memory just to
// carry it into a throwaway editor folder.
#[cfg(windows)]
pub(crate) const KEY_SIDECAR_MAX_COPY_BYTES: u64 = 16 * 1024 * 1024;

// Whether `path` is safe to carry along as a key-overlay sidecar: present,
// under the size ceiling, and parseable as the same KeySidecar shape
// read_key_sidecar itself expects. Split out as its own pure function (no
// destination involved) so a test can prove an oversized or malformed file is
// rejected without going through open_file_in_editor's whole command.
#[cfg(windows)]
pub(crate) fn key_sidecar_is_safe_to_copy(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else { return false };
    if !meta.is_file() || meta.len() > KEY_SIDECAR_MAX_COPY_BYTES {
        return false;
    }
    fs::read_to_string(path).ok().and_then(|data| parse_key_sidecar(&data)).is_some()
}

#[cfg(windows)]
pub(crate) fn copy_keys_sidecar_alongside(src: &Path, dest: &Path) {
    let src_sidecar = keylog::sidecar_path(src);
    if key_sidecar_is_safe_to_copy(&src_sidecar) {
        let _ = fs::copy(&src_sidecar, keylog::sidecar_path(dest));
    }
}
#[cfg(not(windows))]
pub(crate) fn copy_keys_sidecar_alongside(_src: &Path, _dest: &Path) {}

// Right-click "Open a file in the editor" (the screenshot/record buttons, and
// an image/video attachment in the library) all land here. The original file
// is never touched: an image is read into a data URL straight into the same
// #snip-modal flow a fresh capture uses, a video is copied once into its own
// throwaway working folder (swept by sweep_stale_temp_files) and THAT copy is
// what open_video_editor is pointed at.
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
            let bytes = fs::read(&p).map_err(|e| e.to_string())?;
            let mime = fmt.map(image_format_mime).unwrap_or("image/png");
            let data_url = format!("data:{};base64,{}", mime, base64_encode(&bytes));
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
            fs::copy(&p, &dest).map_err(|e| e.to_string())?;
            copy_keys_sidecar_alongside(&p, &dest);
            Ok(OpenEditResult::Video { copied_path: dest.to_string_lossy().to_string(), origin_dir, name })
        }
        _ => Err("not-media".to_string()),
    }
}

// Target folder for a plain "Save": a custom expert path, else
// Videos\<app>. Mirrors screenshot_dir. The raw capture itself always keeps
// landing in <data>/recordings until explicitly saved/copied -- 281's sweep
// stays in charge of whatever is still undecided there.
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

// Default recording folder path (shown greyed in the expert menu / the
// result window's folder line when no custom folder is set).
#[tauri::command]
pub(crate) fn default_recording_dir() -> String {
    recording_dir("").to_string_lossy().to_string()
}

// The recording folder's own canonical form -- the custom `recordingDir`
// setting, or the default `Videos\<app>` folder when none is set -- resolved
// by Rust from its own settings rather than trusted from a caller's raw IPC
// argument. Every command below that lists, cleans or deletes recordings
// checks its `dir` parameter against this before touching anything, so a
// call made with some other folder string can only ever read/delete inside
// the one folder this app actually keeps recordings in.
pub(crate) fn configured_recording_dir(state: &State<'_, Db>) -> PathBuf {
    let custom = lock(state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    recording_dir(&custom)
}

// Whether `candidate` really is `configured`: canonicalized when the folder
// exists (defeats a short-name, trailing-dot or symlink mismatch), a plain
// case-insensitive comparison when it does not exist yet -- there is nothing
// to list, clean or delete from a folder that was never created either way,
// so a name-only comparison costs nothing extra there.
pub(crate) fn path_matches_configured_dir(candidate: &Path, configured: &Path) -> bool {
    match (fs::canonicalize(candidate), fs::canonicalize(configured)) {
        (Ok(a), Ok(b)) => a == b,
        _ => {
            let norm = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_ascii_lowercase();
            norm(candidate) == norm(configured)
        }
    }
}

// Only the extensions this app itself ever writes into the recording folder --
// the user could point it at any folder they like (including one already full
// of unrelated files), so both the size readout and the cleanup below must
// never so much as count, let alone delete, anything this app did not create.
pub(crate) const VIDEO_FOLDER_EXTS: [&str; 8] = ["mp4", "webm", "gif", "avi", "mov", "mkv", "webp", "apng"];

pub(crate) fn is_video_folder_file(path: &std::path::Path) -> bool {
    // The extension check alone still leaves every OTHER file with a matching
    // extension in a shared folder (a real video the user already had there,
    // or one from some other app) open to being counted and swept up by the
    // automatic cleanup below -- discard_recording_file_in already treats its
    // "rec-" stem as the actual proof a file is this app's own recording;
    // stats/cleanup/the discard trash listing now require the same proof,
    // rather than trusting the extension on its own.
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

// (file count, total bytes) of every recording in the folder -- a missing
// folder (nothing saved there yet) is simply empty, not an error. Includes
// the "verworfen" discard-trash subfolder too: those files still sit on
// this same drive, under this same folder, so leaving them out would make the
// displayed folder size quietly wrong the moment anything gets discarded.
// `configured` is Rust's own resolved recording folder (see
// `configured_recording_dir`); `dir` is only ever trusted once it is proven
// to be that same folder, kept apart from the tauri::command wrapper below so
// it can be unit-tested without a running app.
pub(crate) fn video_folder_stats_impl(dir: &str, configured: &Path) -> (u64, u64) {
    let base = std::path::Path::new(dir);
    if !path_matches_configured_dir(base, configured) {
        return (0, 0);
    }
    let (c1, b1) = video_folder_stats_in(base);
    let (c2, b2) = video_folder_stats_in(&base.join("verworfen"));
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

// Shared by the preview and the real delete, so what the user confirms is
// exactly what gets removed -- never a second, independently computed list.
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

#[tauri::command]
pub(crate) fn video_folder_cleanup_preview(state: State<'_, Db>, dir: String, age_days: u32) -> Vec<VideoFolderFile> {
    video_folder_cleanup_preview_impl(&dir, age_days, &configured_recording_dir(&state))
}

// Deletes exactly the files video_folder_cleanup_preview just listed -- never
// re-scans with different criteria, so the confirmed list and the
// actually-deleted files can never drift apart (e.g. a file aging past the
// threshold between preview and confirm just stays for next time, not a bug).
pub(crate) fn video_folder_cleanup_impl(dir: &str, age_days: u32, configured: &Path) -> usize {
    if !path_matches_configured_dir(std::path::Path::new(dir), configured) {
        return 0;
    }
    let stale = video_folder_stale(dir, age_days);
    let mut n = 0;
    for (path, _) in stale {
        if fs::remove_file(&path).is_ok() { n += 1; }
    }
    n
}

#[tauri::command]
pub(crate) fn video_folder_cleanup(state: State<'_, Db>, dir: String, age_days: u32) -> usize {
    video_folder_cleanup_impl(&dir, age_days, &configured_recording_dir(&state))
}

// The batch-export dialog needs each selected recording's real duration
// to build export_recordings_batch's per-item total_ms and to sum the disk
// space check -- reuses the same ffprobe path probe_media_meta already uses
// for a library attachment. A file that cannot be probed (missing, corrupt)
// comes back as 0, the same "unknown" fallback probe_media_meta uses
// everywhere else, rather than failing the whole batch over one bad file.
pub(crate) fn video_folder_file_durations_ms_impl(ffprobe: Option<&Path>, dir: &Path, names: &[String]) -> Vec<u64> {
    use rayon::prelude::*;
    // Probed in parallel (each one is a child process); collect keeps the order of `names`.
    names.par_iter().map(|name| probe_media_meta(ffprobe, &dir.join(name)).0).collect()
}

#[tauri::command]
pub(crate) async fn video_folder_file_durations_ms(app: AppHandle, dir: String, names: Vec<String>) -> Vec<u64> {
    let ffprobe = ffprobe_search_path(&app);
    // Off the async runtime: one process spawn per file would otherwise hold a worker.
    tauri::async_runtime::spawn_blocking(move || video_folder_file_durations_ms_impl(ffprobe.as_deref(), Path::new(&dir), &names))
        .await
        .unwrap_or_default()
}

// Never overwrite an existing file at the target: "<stem>.<ext>", then
// "<stem> (2).<ext>", "<stem> (3).<ext>", etc. Shared by every 280 button
// that writes into a folder instead of a dialog-chosen exact path.
pub(crate) fn unique_dest_path(dir: &std::path::Path, stem: &str, ext: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    candidate
}

// Same naming scheme as unique_dest_path, but the free name is claimed the
// moment it is found instead of merely observed: unique_dest_path only checks
// candidate.exists(), and a file that appears at that name between the check
// and a later fs::rename onto it gets silently replaced, since rename onto an
// existing file overwrites on Windows. create_new opens the name atomically
// (the OS refuses if it already exists) and moves straight to the next
// numbered candidate on that exact race, so the name this function returns is
// always one nothing else could have claimed first. The empty placeholder
// file it leaves behind still needs to be replaced (or removed on failure) by
// the caller, same as before.
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

// fs::rename fails across drives (recordingDir can be anywhere the user
// picks, unlike the fixed same-drive <data>/recordings) -- fall back to
// copy+remove there.
pub(crate) fn move_file(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    fs::copy(src, dst)?;
    fs::remove_file(src)
}

// Save/Save As/Copy relocate the video but used to leave its mic
// sidecar (a dual-audio capture not yet muxed by the editor) behind in the
// recordings folder, where it had no owner left and was later swept as an
// orphan -- silently losing the microphone track on every recording saved
// before it was ever opened in the editor. Best-effort: the primary move
// already succeeded by the time this runs, so a sidecar move failure is
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
    // The key-overlay sidecar (if this take ever wrote one) travels
    // the same way, for the same reason -- Save/Save As/Copy relocate the
    // video file and would otherwise orphan it at the old path.
    #[cfg(windows)]
    {
        let old_keys = keylog::sidecar_path(old_video);
        if old_keys.is_file() {
            let new_keys = keylog::sidecar_path(new_video);
            if old_keys != new_keys {
                if let Err(e) = move_file(&old_keys, &new_keys) {
                    eprintln!("failed to relocate key-overlay sidecar: {e}");
                }
            }
        }
    }
    // Same again for the camera sidecar, on whichever of the two recordings
    // (screen or camera) is the one actually being relocated here.
    let old_cam = camera::sidecar_path(old_video);
    if old_cam.is_file() {
        let new_cam = camera::sidecar_path(new_video);
        if old_cam != new_cam {
            if let Err(e) = move_file(&old_cam, &new_cam) {
                eprintln!("failed to relocate camera sidecar: {e}");
            }
        }
    }
}

// Shared by Save/Copy: if the file already sits in the target folder
// (e.g. Copy running after a Save already placed it there), leave its name
// alone; otherwise move it in under a name that never overwrites anything.
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
    let dest = unique_dest_path(dir, stem, ext);
    move_file(src, &dest)?;
    move_mic_sidecar_alongside(src, &dest);
    Ok(dest)
}

// Post-capture "Save as..." for a recording -- the file already sits in the
// recordings folder (raw or converted); this moves it wherever the user
// picks, same shape as save_screenshot_as but from a real file instead of a
// data URL. Moves rather than copies so the raw capture does not linger as a
// duplicate in <data>/recordings once it has a permanent home -- the
// frontend updates its own pending-path to the new location afterwards, so
// a later Attach/Copy on the same result still finds the file.
#[tauri::command]
pub(crate) async fn save_recording_as(app: AppHandle, state: State<'_, Db>, win: tauri::Window, src_path: String) -> Result<Option<String>, String> {
    let src = PathBuf::from(&src_path);
    if !src.is_file() {
        return Err("recording file not found".to_string());
    }
    // move_file below relocates this file wherever the save dialog points --
    // refuse to move anything that is not proven to be either this app's own
    // recording, sitting in a recording folder, or a file this same
    // session's own export_video call actually wrote (an export can land
    // outside any recording folder once export_dest_dir_is_authorized has
    // cleared it, so the recording-folder rule alone would refuse "Save as"
    // right after a perfectly legitimate export).
    if !is_own_recording_source(&recording_source_dirs(&app, &state), &src) && !is_known_export_output(&src) {
        return Err("source-not-allowed".to_string());
    }
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("mp4").to_string();
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| format!("Recording.{ext}"));
    let result = file_dialog_for(&app, win.label()).add_filter(ext.to_uppercase(), &[ext.as_str()]).set_file_name(&name).save_file();
    let _ = win.set_focus();
    match result {
        Some(p) => {
            move_tracked_export_output(&src, &p).map_err(|e| e.to_string())?;
            move_mic_sidecar_alongside(&src, &p);
            Ok(Some(p.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

// The folder picker used by the video editor's "choose folder"
// export target and the recordings batch-export dialog. A plain wrapper
// around the same picker `pick_folder` uses, except this one also remembers
// the folder as an authorized export destination -- `pick_folder` itself
// stays untouched since it is shared with picks that have nothing to do with
// exporting (attachments, imports, the recordings folder setting, ...) and
// must not widen what an export command will accept.
#[tauri::command]
pub(crate) async fn pick_export_folder(app: AppHandle, win: tauri::Window) -> Option<String> {
    let result = file_dialog_for(&app, win.label()).pick_folder();
    let _ = win.set_focus();
    if let Some(dir) = &result {
        // remember_export_dest_dir is what actually authorizes exporting into
        // this folder right away (export_dest_dir_is_authorized checks it
        // directly, not a purpose-bound grant); the grant below is the
        // separate, one-time authority set_video_export_dir needs if the
        // caller goes on to save this folder as the standing preference.
        remember_export_dest_dir(dir);
        grant_picked_dir(dir, "videoExport");
    }
    result.map(|p| p.to_string_lossy().to_string())
}

// 280's plain "Save" -- writes straight into recordingDir without a dialog
// (the common case); save_recording_as above keeps the free dialog for
// everything else.
#[tauri::command]
pub(crate) async fn save_recording_now(app: AppHandle, state: State<'_, Db>, src_path: String) -> Result<String, String> {
    let src = PathBuf::from(&src_path);
    if !src.is_file() {
        return Err("recording file not found".to_string());
    }
    // place_in_dir below moves the file -- refuse to move anything that is not
    // proven to be this app's own recording, sitting where its own capture or
    // an earlier Save/Copy left it.
    if !is_own_recording_source(&recording_source_dirs(&app, &state), &src) {
        return Err("source-not-allowed".to_string());
    }
    let custom = lock(&state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    let dest = place_in_dir(&src, &recording_dir(&custom)).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

// 280's "Copy" -- the video counterpart of the screenshot result dialog's
// clipboard copy. A video cannot be a bitmap, so the file itself is the
// clipboard payload (the same CF_HDROP path copy_prompt already uses for any
// file-attached prompt); it is moved into recordingDir first so the pasted
// reference still resolves after the window closes (see discard_recording_file
// and closeRecModal on the frontend: a copy alone does not count as "saving",
// so closing without also using Save/Save as.../Attach still deletes it).
//
// The move happens before the clipboard write and is not undone if that write
// fails (undoing it would just trade one race for another) -- so the new
// location is always returned alongside a success flag rather than folded
// into the Err case, which used to strand the frontend pointing at the
// now-nonexistent original path: every later action (Save, Discard, a retry)
// would fail with "file not found" even though the recording still exists,
// safely, at `dest`.
#[tauri::command]
pub(crate) async fn copy_recording_to_clipboard(app: AppHandle, state: State<'_, Db>, src_path: String) -> Result<(String, bool), String> {
    let src = PathBuf::from(&src_path);
    if !src.is_file() {
        return Err("recording file not found".to_string());
    }
    // Same move as save_recording_now above -- the source must already be
    // proven to be this app's own recording before place_in_dir relocates it.
    if !is_own_recording_source(&recording_source_dirs(&app, &state), &src) {
        return Err("source-not-allowed".to_string());
    }
    let custom = lock(&state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    let dest = place_in_dir(&src, &recording_dir(&custom)).map_err(|e| e.to_string())?;
    let dest_str = dest.to_string_lossy().to_string();
    let clip_ok = set_clipboard_file(&dest_str);
    Ok((dest_str, clip_ok))
}

// The video editor's "Copy to clipboard" export target -- a throwaway
// file in the app's own temp folder rather than anywhere the user chose, so
// it can be swept away automatically (see sweep_clip_export_temp below)
// without ever touching a real save.
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

// Thin wrapper so the frontend does not need a Windows-only code path of its
// own -- same CF_HDROP mechanism copy_recording_to_clipboard already uses.
#[tauri::command]
pub(crate) async fn copy_file_to_clipboard(path: String) -> bool {
    set_clipboard_file(&path)
}

// recDiscardKeepH has no expert-menu row yet (same "future menu writes it
// directly" state as recCanvasMode/recDiskWarnMb above) -- 0 disables the
// whole discard-trash cleanup, keeping everything in "verworfen" forever.
pub(crate) fn discard_keep_hours(state: &State<'_, Db>) -> f64 {
    lock(state).settings.ui_values.get("recDiscardKeepH").copied().unwrap_or(24.0)
}

// The two folders a genuine recording can sit in while a recording command is
// still allowed to act on it by an IPC path: the app's own capture staging
// folder (<data>/recordings, where a fresh recording always lands first) and
// the currently configured recordingDir (where Save/Copy leave it before the
// result window closes). Shared by discard/convert/save/copy below so they
// all draw the line in exactly the same place.
pub(crate) fn recording_source_dirs(app: &AppHandle, state: &State<'_, Db>) -> [PathBuf; 2] {
    let custom = lock(state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    [data_dir(app).join("recordings"), recording_dir(&custom)]
}

// Whether `path` is a recording this app itself produced, sitting in one of
// `allowed`'s folders, rather than an arbitrary file named in a recording
// command's IPC argument -- the same "rec-" stem plus allowed-parent-folder
// proof discard_recording_file_in below already requires, reused here so
// convert_recording/save_recording_now/copy_recording_to_clipboard cannot be
// made to move, replace or feed ffmpeg an unrelated file just because a
// subverted renderer sent that file's path as `src_path`.
pub(crate) fn is_own_recording_source(allowed: &[PathBuf], path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !name.starts_with("rec-") || !path.is_file() {
        return false;
    }
    let Some(parent) = path.parent().and_then(|d| d.canonicalize().ok()) else { return false };
    allowed.iter().any(|d| d.canonicalize().map(|c| c == parent).unwrap_or(false))
}

// Whether `sidecar` is exactly the mic capture belonging to `video` -- the
// only sidecar mux_mic_track may consume and delete. Checked apart from
// is_own_recording_source (which only proves `video` itself is ours) so a
// second, unrelated path cannot ride along just because it happens to sit in
// an allowed folder too.
pub(crate) fn is_own_mic_sidecar(video: &Path, sidecar: &Path) -> bool {
    sidecar == video.with_extension("mic.wav")
}

// Discard a not-yet-saved recording. Scoped to the app's own recordings
// folder OR the current recordingDir (280's Copy/Save move the working file
// there before the window closes) -- never an arbitrary webview-supplied
// path, same defense-in-depth pattern as delete_screenshot_file. Moves into
// a "verworfen" subfolder of whichever of those two it was actually in,
// rather than deleting outright, so a wrong click can still be undone --
// see restore_discarded_recording and the sweep in sweep_recordings_now_result,
// which is what eventually clears this subfolder back out.
// Split out so the actual move logic has a real unit test against a plain
// tempdir, no AppHandle/Db needed -- same split as sweep_stale_recordings_in
// above. Returns the file's new path in "verworfen" on success.
pub(crate) fn discard_recording_file_in(allowed: &[PathBuf], path: &std::path::Path) -> Option<PathBuf> {
    // The directory check alone let this touch ANY file directly inside
    // recordingDir -- often the user's normal Videos folder, shared with files
    // this app never touched. delete_screenshot_file's own comment already
    // calls its filename-prefix check "defense in depth" against exactly this;
    // this command claimed to follow the same pattern but was missing that
    // half. Every real recording (raw, saved, converted, or moved by Copy)
    // keeps the "rec-" stem `rec_filename`/`place_in_dir` give it -- only
    // Save As lets the user pick a different name, and that always sets
    // recSavedOrAttached first, taking the file out of discard's reach.
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !name.starts_with("rec-") {
        return None;
    }
    let parent = path.parent().and_then(|d| d.canonicalize().ok())?;
    let in_allowed = allowed.iter().any(|d| d.canonicalize().map(|c| c == parent).unwrap_or(false));
    if !in_allowed || !path.is_file() {
        return None;
    }
    let trash = parent.join("verworfen");
    fs::create_dir_all(&trash).ok()?;
    let dest = trash.join(name);
    move_file(path, &dest).ok()?;
    // A discarded recording takes its mic and key-overlay sidecars (if any)
    // with it -- best-effort, since most recordings have neither. Both only
    // ever exist next to an mp4 (the screen recorder's own container).
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if ext.eq_ignore_ascii_case("mp4") {
            let sidecar = path.with_extension("mic.wav");
            if sidecar.is_file() {
                let _ = move_file(&sidecar, &dest.with_extension("mic.wav"));
            }
            #[cfg(windows)]
            {
                let keys = keylog::sidecar_path(path);
                if keys.is_file() {
                    let _ = move_file(&keys, &keylog::sidecar_path(&dest));
                }
            }
        }
    }
    // The camera sidecar takes the same trip, regardless of container --
    // camera recordings are webm, not mp4, so this cannot sit inside the
    // ext-gated block above.
    let cam_sidecar = camera::sidecar_path(path);
    if cam_sidecar.is_file() {
        let _ = move_file(&cam_sidecar, &camera::sidecar_path(&dest));
    }
    Some(dest)
}

// Discard a not-yet-saved recording. Scoped to the app's own recordings
// folder OR the current recordingDir (280's Copy/Save move the working file
// there before the window closes) -- never an arbitrary webview-supplied
// path, same defense-in-depth pattern as delete_screenshot_file. Moves into
// a "verworfen" subfolder of whichever of those two it was actually in,
// rather than deleting outright, so a wrong click can still be undone --
// see restore_discarded_recording and the sweep in sweep_recordings_now_result,
// which is what eventually clears this subfolder back out.
#[tauri::command]
pub(crate) fn discard_recording_file(app: AppHandle, state: State<'_, Db>, path: String) -> Option<f64> {
    let custom = lock(&state).settings.ui_texts.get("recordingDir").cloned().unwrap_or_default();
    let allowed = [data_dir(&app).join("recordings"), recording_dir(&custom)];
    discard_recording_file_in(&allowed, std::path::Path::new(&path))?;
    Some(discard_keep_hours(&state))
}

// Bare filename only, never a path -- every caller below reaches into a
// folder's own "verworfen" subfolder with a name taken from that same
// folder's listing, but this guard stops a hand-crafted "../x" from ever
// escaping it, the same defense-in-depth discard_recording_file already
// applies to the file it moves in.
pub(crate) fn is_bare_filename(name: &str) -> bool {
    !name.is_empty() && std::path::Path::new(name).file_name().is_some_and(|f| f == std::ffi::OsStr::new(name))
}

// The Aufnahmeordner dialog's own listing of what is currently sitting in
// the trash, waiting out its retention window -- same shape and same
// extension filter as video_folder_stats/video_folder_cleanup_preview above.
pub(crate) fn list_discarded_recordings_impl(dir: &str, configured: &Path) -> Vec<VideoFolderFile> {
    if !path_matches_configured_dir(std::path::Path::new(dir), configured) {
        return Vec::new();
    }
    let trash = std::path::Path::new(dir).join("verworfen");
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
    let trash = std::path::Path::new(dir).join("verworfen");
    let src = trash.join(name);
    let dst = std::path::Path::new(dir).join(name);
    // Never overwrites a same-named file that has appeared at the old spot
    // since the discard -- restoring silently on top of someone else's
    // recording would be a worse surprise than the restore just failing.
    if !src.is_file() || dst.exists() || move_file(&src, &dst).is_err() {
        return false;
    }
    let sidecar_src = src.with_extension("mic.wav");
    if sidecar_src.is_file() {
        let _ = move_file(&sidecar_src, &dst.with_extension("mic.wav"));
    }
    // The key-overlay sidecar used to stay behind in the trash on
    // a restore, orphaned at the old path forever.
    let keys_src = src.with_extension("keys.json");
    if keys_src.is_file() {
        let _ = move_file(&keys_src, &dst.with_extension("keys.json"));
    }
    let cam_sidecar_src = camera::sidecar_path(&src);
    if cam_sidecar_src.is_file() {
        let _ = move_file(&cam_sidecar_src, &camera::sidecar_path(&dst));
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
    let trash = std::path::Path::new(dir).join("verworfen");
    let path = trash.join(name);
    if !path.is_file() {
        return false;
    }
    let _ = fs::remove_file(path.with_extension("mic.wav"));
    // A manual "delete for good" used to leave the key-overlay
    // sidecar sitting in the trash forever once its video was gone.
    let _ = fs::remove_file(path.with_extension("keys.json"));
    let _ = fs::remove_file(camera::sidecar_path(&path));
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
