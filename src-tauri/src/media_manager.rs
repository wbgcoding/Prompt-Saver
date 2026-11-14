use super::*;
use std::sync::{Condvar, MutexGuard};

// The media manager: one window that lists the app's own recordings or screenshots as a tile
// grid with previews, sort and filters. Every command resolves the folder in Rust from the
// settings and takes bare file names, never a folder or a path from the page, so a subverted
// page can only reach the one folder each kind lives in.

pub(crate) const MANAGER_WIN_LABEL: &str = "manager";
const MANAGER_WIN_W: u32 = 1100;
const MANAGER_WIN_H: u32 = 720;
const MANAGER_WIN_MIN_W: u32 = 640;
const MANAGER_WIN_MIN_H: u32 = 420;

// Width of a poster picture. The tile grid shows them at about 200 px; the extra width keeps a
// scaled display sharp.
const POSTER_WIDTH: u32 = 320;
// Posters made at the same moment (one ffmpeg process each).
pub(crate) const POSTER_PARALLEL: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MediaKind {
    Recordings,
    Screenshots,
}

impl MediaKind {
    pub(crate) fn parse(s: &str) -> Result<MediaKind, String> {
        match s {
            "recordings" => Ok(MediaKind::Recordings),
            "screenshots" => Ok(MediaKind::Screenshots),
            _ => Err("unknown media kind".to_string()),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            MediaKind::Recordings => "recordings",
            MediaKind::Screenshots => "screenshots",
        }
    }
}

// The folder a kind lives in, read from this app's own settings.
pub(crate) fn media_root(state: &State<'_, Db>, kind: MediaKind) -> PathBuf {
    match kind {
        MediaKind::Recordings => configured_recording_dir(state),
        MediaKind::Screenshots => {
            let custom = lock(state).settings.ui_texts.get("screenshotDir").cloned().unwrap_or_default();
            crate::snip::screenshot_dir(&custom)
        }
    }
}

// A single file name this app writes for `kind`: nothing path-like (separators, "..", a drive
// prefix, a stream suffix) and the name pattern of its own files. Foreign files in the same
// folder (Windows' own "Screenshot 2026-..." files, a user's other videos) never qualify.
pub(crate) fn is_media_name(kind: MediaKind, name: &str) -> bool {
    if !is_bare_filename(name) || name.chars().any(|c| c.is_control() || "\\/:*?\"<>|".contains(c)) {
        return false;
    }
    match kind {
        MediaKind::Recordings => is_bare_recording_name(name),
        MediaKind::Screenshots => {
            let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            crate::snip::is_own_screenshot_name(name, &ext)
        }
    }
}

#[derive(Serialize, Debug, PartialEq)]
pub(crate) struct MediaFile {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) size: u64,
    pub(crate) modified_ms: u64,
    pub(crate) ext: String,
}

fn modified_ms_of(meta: &fs::Metadata) -> u64 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn files_in(kind: MediaKind, dir: &Path) -> Vec<MediaFile> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
        if !is_media_name(kind, &name) {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let ext = Path::new(&name).extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        out.push(MediaFile { path: e.path().to_string_lossy().to_string(), size: meta.len(), modified_ms: modified_ms_of(&meta), ext, name });
    }
    out
}

pub(crate) fn list_media_in(kind: MediaKind, root: &Path) -> Vec<MediaFile> {
    files_in(kind, root)
}

#[derive(Serialize, Debug, PartialEq)]
pub(crate) struct MediaProbe {
    pub(crate) name: String,
    pub(crate) duration_ms: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

// Duration and pixel size per name, probed in parallel (a video costs one process each). A name
// that is not one of this kind's file names, or a file that cannot be read, answers 0.
pub(crate) fn media_probe_in(kind: MediaKind, ffprobe: Option<&Path>, root: &Path, names: &[String]) -> Vec<MediaProbe> {
    crate::parallel::par_map(names, |name| {
        let (duration_ms, width, height) = if !is_media_name(kind, name) {
            (0, 0, 0)
        } else if kind == MediaKind::Screenshots {
            image::image_dimensions(root.join(name)).map(|(w, h)| (0, w, h)).unwrap_or((0, 0, 0))
        } else {
            let (d, w, h, _) = probe_media_meta(ffprobe, &root.join(name));
            (d, w, h)
        };
        MediaProbe { name: name.clone(), duration_ms, width, height }
    })
}

// ---------------------------------------------------------------- posters

// Counts the manager's kind switches and closings: a poster waiting for a free slot that sees a
// newer value belongs to a grid that is gone and gives up. Separate from the editor's filmstrip
// counter, which a poster run must never cancel.
pub(crate) static MEDIA_POSTER_GEN: AtomicU64 = AtomicU64::new(0);

struct PosterSlots {
    busy: Mutex<usize>,
    freed: Condvar,
}

static POSTER_SLOTS: PosterSlots = PosterSlots { busy: Mutex::new(0), freed: Condvar::new() };

pub(crate) struct PosterSlot;

impl Drop for PosterSlot {
    fn drop(&mut self) {
        *POSTER_SLOTS.busy.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
        POSTER_SLOTS.freed.notify_one();
    }
}

// Waits for one of POSTER_PARALLEL slots; None as soon as `stale` answers true.
pub(crate) fn acquire_poster_slot(stale: impl Fn() -> bool) -> Option<PosterSlot> {
    let mut busy: MutexGuard<usize> = POSTER_SLOTS.busy.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if stale() {
            return None;
        }
        if *busy < POSTER_PARALLEL {
            *busy += 1;
            return Some(PosterSlot);
        }
        busy = POSTER_SLOTS.freed.wait_timeout(busy, Duration::from_millis(200)).unwrap_or_else(|e| e.into_inner()).0;
    }
}

// The cache file name of a poster: the file's full path and its last-write time, so an edited
// or replaced file never shows the old picture.
pub(crate) fn poster_cache_key(path: &str, mtime_ms: u128) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    mtime_ms.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub(crate) fn poster_cache_dir() -> PathBuf {
    std::env::temp_dir().join(brand::TEMP_DIR).join("media-posters")
}

fn poster_cache_file(src: &Path) -> Option<PathBuf> {
    let mtime = fs::metadata(src).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis();
    Some(poster_cache_dir().join(format!("{}.jpg", poster_cache_key(&src.to_string_lossy(), mtime))))
}

// A 320 px wide JPG of a screenshot. Written beside its name and renamed, so a half-written
// picture is never taken for a finished one.
fn make_image_poster(src: &Path, dest: &Path) -> bool {
    let Some(img) = crate::images::decode_image_file(&src.to_string_lossy()) else { return false };
    let (w, h) = (img.width().max(1), img.height().max(1));
    let img = if w > POSTER_WIDTH {
        img.resize_exact(POSTER_WIDTH, ((h as u64 * POSTER_WIDTH as u64) / w as u64).max(1) as u32, FilterType::Triangle)
    } else {
        img
    };
    let rgb = image::DynamicImage::ImageRgb8(img.to_rgb8());
    let mut bytes = Vec::new();
    if rgb.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 80)).is_err() {
        return false;
    }
    let part = dest.with_extension("part");
    fs::write(&part, bytes).is_ok() && fs::rename(&part, dest).is_ok()
}

// The moment a video poster is taken: a tenth of the clip, at most one second in, so a short
// clip does not seek past its end and a long one skips a black first frame.
pub(crate) fn poster_seek_secs(duration_ms: u64) -> f64 {
    if duration_ms == 0 {
        return 0.0;
    }
    (duration_ms as f64 / 1000.0 / 10.0).min(1.0)
}

fn make_video_poster(ffmpeg: &Path, src: &Path, dest: &Path, duration_ms: u64) -> bool {
    use std::os::windows::process::CommandExt;
    let part = dest.with_extension("part.jpg");
    let grab = |seek: f64| -> bool {
        let _ = fs::remove_file(&part);
        let mut cmd = std::process::Command::new(ffmpeg);
        cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-ss"])
            .arg(format!("{seek:.3}"))
            .arg("-i")
            .arg(src)
            .args(["-frames:v", "1", "-vf", &format!("scale={POSTER_WIDTH}:-2"), "-q:v", "5"])
            .arg(&part);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
        status_in_job(&mut cmd, Duration::from_secs(20)).is_ok_and(|s| s.success()) && part.is_file()
    };
    let seek = poster_seek_secs(duration_ms);
    // A seek past the last decodable frame exits cleanly without a picture: try the first frame.
    let made = grab(seek) || (seek > 0.0 && grab(0.0));
    if made && fs::rename(&part, dest).is_ok() {
        return true;
    }
    let _ = fs::remove_file(&part);
    false
}

// The cached poster of one file, made when missing. None: no ffmpeg (a video), an unreadable
// file, or the grid it was asked for is gone.
pub(crate) fn media_poster_blocking(
    kind: MediaKind, ffmpeg: Option<&Path>, ffprobe: Option<&Path>, root: &Path, name: &str, stale: impl Fn() -> bool,
) -> Option<String> {
    if !is_media_name(kind, name) {
        return None;
    }
    let src = root.join(name);
    if !src.is_file() {
        return None;
    }
    let dest = poster_cache_file(&src)?;
    if dest.is_file() {
        return Some(dest.to_string_lossy().to_string());
    }
    if kind == MediaKind::Recordings && ffmpeg.is_none() {
        return None;
    }
    fs::create_dir_all(poster_cache_dir()).ok()?;
    let _slot = acquire_poster_slot(stale)?;
    let made = match kind {
        MediaKind::Screenshots => make_image_poster(&src, &dest),
        MediaKind::Recordings => {
            let duration_ms = if ffprobe.is_some() { probe_media_meta(ffprobe, &src).0 } else { 0 };
            make_video_poster(ffmpeg?, &src, &dest, duration_ms)
        }
    };
    made.then(|| dest.to_string_lossy().to_string())
}

// ---------------------------------------------------------------- discard, restore, delete

fn trash_file(root: &Path, name: &str) -> PathBuf {
    discard_trash_dir(root).join(name)
}

// A screenshot into "verworfen" inside the screenshot folder: the recordings' discard, with
// this kind's own name test (discard_recording_file_in only knows the recording stem).
pub(crate) fn discard_screenshot_in(root: &Path, name: &str) -> Option<PathBuf> {
    if !is_media_name(MediaKind::Screenshots, name) {
        return None;
    }
    let src = root.join(name);
    if !src.is_file() {
        return None;
    }
    let trash = discard_trash_dir(root);
    fs::create_dir_all(&trash).ok()?;
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("Screenshot");
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let dest = reserve_unique_dest_path(&trash, stem, ext).ok()?;
    if move_file(&src, &dest).is_err() {
        let _ = fs::remove_file(&dest);
        return None;
    }
    // The holding-area sweep ages a file by its write time: stamp the moment of the discard.
    if let Ok(f) = fs::OpenOptions::new().write(true).open(&dest) {
        let _ = f.set_modified(SystemTime::now());
    }
    Some(dest)
}

pub(crate) fn restore_screenshot_in(root: &Path, name: &str) -> bool {
    if !is_media_name(MediaKind::Screenshots, name) {
        return false;
    }
    let src = trash_file(root, name);
    let dst = root.join(name);
    // Never over a same-named file that appeared at the old spot since.
    src.is_file() && !dst.exists() && move_file(&src, &dst).is_ok()
}

pub(crate) fn delete_discarded_screenshot_in(root: &Path, name: &str) -> bool {
    if !is_media_name(MediaKind::Screenshots, name) {
        return false;
    }
    let path = trash_file(root, name);
    path.is_file() && fs::remove_file(path).is_ok()
}

// Moves each listed name to the holding folder; the number that moved.
pub(crate) fn media_discard_in(kind: MediaKind, root: &Path, names: &[String]) -> usize {
    names
        .iter()
        .filter(|name| match kind {
            MediaKind::Recordings => {
                is_media_name(kind, name) && discard_recording_file_in(&[root.to_path_buf()], root, &root.join(name)).is_some()
            }
            MediaKind::Screenshots => discard_screenshot_in(root, name).is_some(),
        })
        .count()
}

pub(crate) fn media_restore_in(kind: MediaKind, root: &Path, name: &str) -> bool {
    match kind {
        MediaKind::Recordings => is_media_name(kind, name) && restore_discarded_recording_impl(&root.to_string_lossy(), name, root),
        MediaKind::Screenshots => restore_screenshot_in(root, name),
    }
}

pub(crate) fn media_delete_now_in(kind: MediaKind, root: &Path, name: &str) -> bool {
    match kind {
        MediaKind::Recordings => is_media_name(kind, name) && delete_discarded_recording_now_impl(&root.to_string_lossy(), name, root),
        MediaKind::Screenshots => delete_discarded_screenshot_in(root, name),
    }
}

pub(crate) fn list_discarded_in(kind: MediaKind, root: &Path) -> Vec<DiscardedRecording> {
    files_in(kind, &discard_trash_dir(root)).into_iter().map(|f| DiscardedRecording { name: f.name, size: f.size }).collect()
}

// The screenshots' holding folder ages out like the recordings' one: whatever sits in it past
// `age` goes. (count, bytes).
pub(crate) fn discarded_screenshots_stale_in(dir: &Path, age: Duration) -> (u32, u64) {
    let mut count = 0u32;
    let mut bytes = 0u64;
    for f in files_in(MediaKind::Screenshots, dir) {
        let elapsed = SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_millis(f.modified_ms))
            .and_then(|t| SystemTime::now().duration_since(t).ok());
        if elapsed.is_none_or(|e| e < age) {
            continue;
        }
        if fs::remove_file(Path::new(&f.path)).is_ok() {
            count += 1;
            bytes += f.size;
        }
    }
    (count, bytes)
}

// ---------------------------------------------------------------- commands

fn kind_and_root(state: &State<'_, Db>, kind: &str) -> Result<(MediaKind, PathBuf), String> {
    let kind = MediaKind::parse(kind)?;
    let root = media_root(state, kind);
    Ok((kind, root))
}

// A validated file of the manager's folder: the name is one of this kind's own file names and
// the file exists there.
fn existing_media_file(state: &State<'_, Db>, kind: &str, name: &str) -> Result<(MediaKind, PathBuf, PathBuf), String> {
    let (kind, root) = kind_and_root(state, kind)?;
    if !is_media_name(kind, name) {
        return Err("not-a-media-file".to_string());
    }
    let file = root.join(name);
    if !file.is_file() {
        return Err("media-file-not-found".to_string());
    }
    Ok((kind, root, file))
}

#[tauri::command(async)]
pub(crate) fn list_media(state: State<'_, Db>, kind: String) -> Result<Vec<MediaFile>, String> {
    let (kind, root) = kind_and_root(&state, &kind)?;
    Ok(list_media_in(kind, &root))
}

#[tauri::command]
pub(crate) async fn media_probe(app: AppHandle, kind: String, names: Vec<String>) -> Result<Vec<MediaProbe>, String> {
    let (kind, root) = kind_and_root(&app.state::<Db>(), &kind)?;
    let ffprobe = ffprobe_search_path(&app);
    // Off the async runtime: one process spawn per recording would hold a worker.
    tauri::async_runtime::spawn_blocking(move || media_probe_in(kind, ffprobe.as_deref(), &root, &names))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn media_poster(app: AppHandle, kind: String, name: String) -> Result<Option<String>, String> {
    let (kind, root) = kind_and_root(&app.state::<Db>(), &kind)?;
    let ffmpeg = ffmpeg_search_path(&app);
    let ffprobe = ffprobe_search_path(&app);
    let my_gen = MEDIA_POSTER_GEN.load(Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || {
        media_poster_blocking(kind, ffmpeg.as_deref(), ffprobe.as_deref(), &root, &name, || MEDIA_POSTER_GEN.load(Ordering::SeqCst) != my_gen)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub(crate) fn media_reveal(state: State<'_, Db>, kind: String, name: String) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let (_, _, file) = existing_media_file(&state, &kind, &name)?;
    // Explorer's own quoting: the normal argument escaping breaks a path with spaces.
    std::process::Command::new(crate::webview2::windows_dir_exe("explorer.exe"))
        .raw_arg(format!("/select,\"{}\"", file.display()))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// Opens the file with the program Windows has for its type (the system player or viewer).
#[tauri::command(async)]
pub(crate) fn media_play(state: State<'_, Db>, kind: String, name: String) -> Result<(), String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let (_, _, file) = existing_media_file(&state, &kind, &name)?;
    let code = unsafe { ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from(file.as_os_str()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    let code = code.0 as isize;
    if code <= 32 {
        return Err(format!("could not open the file (ShellExecuteW code {code})"));
    }
    Ok(())
}

// The file itself on the clipboard, like Ctrl+C in Explorer.
#[tauri::command(async)]
pub(crate) fn media_copy_file(state: State<'_, Db>, kind: String, name: String) -> Result<bool, String> {
    let (_, _, file) = existing_media_file(&state, &kind, &name)?;
    Ok(set_clipboard_file(&file.to_string_lossy(), &[]))
}

// A screenshot's picture on the clipboard.
#[tauri::command(async)]
pub(crate) fn media_copy_image(state: State<'_, Db>, name: String) -> Result<(), String> {
    let (kind, _, file) = existing_media_file(&state, MediaKind::Screenshots.as_str(), &name)?;
    debug_assert_eq!(kind, MediaKind::Screenshots);
    let url = editor_image_data_url(&file, image::ImageFormat::from_path(&file).ok())?;
    crate::images::copy_image_to_clipboard_as(&url, false).map_err(str::to_string)
}

// The video editor / screenshot editor lives behind the main window's open-file route: the
// manager asks the main window to open the file, with a path Rust built from a validated name.
#[tauri::command(async)]
pub(crate) fn media_open_in_main(app: AppHandle, state: State<'_, Db>, kind: String, name: String) -> Result<(), String> {
    let (kind, _, file) = existing_media_file(&state, &kind, &name)?;
    let _ = app.emit_to("main", "manager-open-file", serde_json::json!({ "kind": kind.as_str(), "path": file.to_string_lossy() }));
    Ok(())
}

// The batch export dialog is the main window's: it gets the recording folder and the names.
#[tauri::command(async)]
pub(crate) fn media_export_in_main(app: AppHandle, state: State<'_, Db>, names: Vec<String>) -> Result<(), String> {
    let root = media_root(&state, MediaKind::Recordings);
    let names: Vec<String> = names.into_iter().filter(|n| is_media_name(MediaKind::Recordings, n) && root.join(n).is_file()).collect();
    if names.is_empty() {
        return Ok(());
    }
    show_main(&app);
    let _ = app.emit_to("main", "manager-export", serde_json::json!({ "dir": root.to_string_lossy(), "names": names }));
    Ok(())
}

// The manager's empty state can point at the setting behind it; only listed keys are forwarded.
const OPENABLE_SETTINGS: [&str; 1] = ["screenshotSave"];

#[tauri::command(async)]
pub(crate) fn media_open_setting(app: AppHandle, key: String) -> Result<(), String> {
    if !OPENABLE_SETTINGS.contains(&key.as_str()) {
        return Err("unknown setting".to_string());
    }
    show_main(&app);
    let _ = app.emit_to("main", "manager-open-setting", key);
    Ok(())
}

// Tells the main window the folder changed, so a size readout there is not stale.
fn notify_folder_changed(app: &AppHandle, kind: MediaKind) {
    let _ = app.emit_to("main", "manager-changed", kind.as_str());
}

// Moves the named files to the holding folder; how many moved and the hours they stay there.
#[tauri::command(async)]
pub(crate) fn media_discard(app: AppHandle, state: State<'_, Db>, kind: String, names: Vec<String>) -> Result<(usize, f64), String> {
    let (kind, root) = kind_and_root(&state, &kind)?;
    let n = media_discard_in(kind, &root, &names);
    if n > 0 {
        notify_folder_changed(&app, kind);
    }
    Ok((n, discard_keep_hours(&state)))
}

#[tauri::command(async)]
pub(crate) fn media_list_discarded(state: State<'_, Db>, kind: String) -> Result<Vec<DiscardedRecording>, String> {
    let (kind, root) = kind_and_root(&state, &kind)?;
    Ok(list_discarded_in(kind, &root))
}

#[tauri::command(async)]
pub(crate) fn media_restore(app: AppHandle, state: State<'_, Db>, kind: String, name: String) -> Result<bool, String> {
    let (kind, root) = kind_and_root(&state, &kind)?;
    let ok = media_restore_in(kind, &root, &name);
    if ok {
        notify_folder_changed(&app, kind);
    }
    Ok(ok)
}

#[tauri::command(async)]
pub(crate) fn media_delete_now(app: AppHandle, state: State<'_, Db>, kind: String, name: String) -> Result<bool, String> {
    let (kind, root) = kind_and_root(&state, &kind)?;
    let ok = media_delete_now_in(kind, &root, &name);
    if ok {
        notify_folder_changed(&app, kind);
    }
    Ok(ok)
}

// ---------------------------------------------------------------- window

// The size a first start opens at: the design size, capped at the screen.
pub(crate) fn manager_default_size(cap: (u32, u32)) -> (u32, u32) {
    (MANAGER_WIN_W.min(cap.0), MANAGER_WIN_H.min(cap.1))
}

// A remembered size, floored at the window's minimum (never above the screen) and capped at the screen.
pub(crate) fn manager_clamped_size(w: u32, h: u32, cap: (u32, u32)) -> (u32, u32) {
    (w.max(MANAGER_WIN_MIN_W).min(cap.0), h.max(MANAGER_WIN_MIN_H).min(cap.1))
}

fn manager_geometry(win: &tauri::WebviewWindow, saved: Option<WindowGeom>) -> WindowGeom {
    let cap = monitor_cap(win.primary_monitor().ok().flatten());
    match saved.filter(|g| g.width > 0 && g.height > 0) {
        Some(g) => {
            let (w, h) = manager_clamped_size(g.width, g.height, cap);
            resolve_geometry(win, Some(WindowGeom { width: w, height: h, ..g }))
        }
        None => {
            let (w, h) = manager_default_size(cap);
            centered_on_primary(win, w, h)
        }
    }
}

// A move or resize of the open window, kept in memory like the main window's and written to
// disk when the window closes. A busy store drops the tick: the close writes the real state.
fn remember_manager_geometry(app: &AppHandle, ev: GeomEvent) {
    let Some(state) = app.try_state::<Db>() else { return };
    let Ok(mut store) = state.try_lock() else { return };
    let mut g = store.settings.manager_window.unwrap_or(WindowGeom { x: 0, y: 0, width: 0, height: 0, maximized: false });
    match ev {
        GeomEvent::Moved(x, y, maximized) => geom_after_move(&mut g, x, y, maximized),
        GeomEvent::Resized(w, h, maximized) => geom_after_resize(&mut g, w, h, maximized),
    }
    store.settings.manager_window = Some(g);
}

fn manager_window_event(app: &AppHandle, win: &tauri::WebviewWindow, event: &WindowEvent) {
    // A hidden window reports its first sizing: only what the user can see counts.
    let visible = win.is_visible().unwrap_or(false);
    match event {
        WindowEvent::Moved(p) if visible && p.x > -30000 && p.y > -30000 => {
            remember_manager_geometry(app, GeomEvent::Moved(p.x, p.y, win.is_maximized().unwrap_or(false)));
        }
        WindowEvent::Resized(s) if visible && s.width > 0 && s.height > 0 => {
            let scale = win.scale_factor().unwrap_or(1.0);
            let logical = s.to_logical::<f64>(scale);
            remember_manager_geometry(app, GeomEvent::Resized(logical.width.round() as u32, logical.height.round() as u32, win.is_maximized().unwrap_or(false)));
        }
        WindowEvent::CloseRequested { .. } => {
            if visible && !win.is_minimized().unwrap_or(false) {
                if let (Ok(p), Ok(s)) = (win.outer_position(), win.inner_size()) {
                    let scale = win.scale_factor().unwrap_or(1.0);
                    let logical = s.to_logical::<f64>(scale);
                    let maximized = win.is_maximized().unwrap_or(false);
                    remember_manager_geometry(app, GeomEvent::Moved(p.x, p.y, maximized));
                    remember_manager_geometry(app, GeomEvent::Resized(logical.width.round() as u32, logical.height.round() as u32, maximized));
                }
            }
            save_settings_off_event_loop(app);
        }
        WindowEvent::Destroyed => {
            MEDIA_POSTER_GEN.fetch_add(1, Ordering::SeqCst);
            crate::rec_bar::rec_hid_forget(MANAGER_WIN_LABEL);
        }
        _ => {}
    }
}

// Opens the manager on `kind`, or brings the open one forward and switches it to that kind.
#[tauri::command]
pub(crate) async fn open_media_manager(app: AppHandle, db: State<'_, Db>, kind: String) -> Result<(), String> {
    let kind = MediaKind::parse(&kind)?;
    if let Some(win) = app.get_webview_window(MANAGER_WIN_LABEL) {
        MEDIA_POSTER_GEN.fetch_add(1, Ordering::SeqCst);
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        let _ = app.emit_to(MANAGER_WIN_LABEL, "manager-kind", kind.as_str());
        return Ok(());
    }
    // Never call Tauri window APIs while holding the Db lock (deadlock risk).
    let (excluded, saved) = {
        let store = lock(&db);
        (capture_excluded(&store.settings), store.settings.manager_window)
    };
    let url = format!("manager.html?kind={}", kind.as_str());
    let win = WebviewWindowBuilder::new(&app, MANAGER_WIN_LABEL, WebviewUrl::App(url.into()))
        .title(brand::DISPLAY_NAME)
        .inner_size(MANAGER_WIN_W as f64, MANAGER_WIN_H as f64)
        .min_inner_size(MANAGER_WIN_MIN_W as f64, MANAGER_WIN_MIN_H as f64)
        .decorations(false)
        .transparent(true)
        .resizable(true)
        .visible(false)
        // Shown once the page has loaded: a window shown at creation sits on screen as an empty
        // transparent frame while WebView2 loads the scripts and styles.
        .on_page_load(|win, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                let _ = win.show();
                let _ = win.set_focus();
            }
        })
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    set_capture_exclusion(&win, excluded);
    let geom = manager_geometry(&win, saved);
    let _ = win.set_size(tauri::LogicalSize::new(geom.width, geom.height));
    let _ = win.set_position(PhysicalPosition::new(geom.x, geom.y));
    if saved.is_some_and(|g| g.maximized) {
        let _ = win.maximize();
    }
    let event_app = app.clone();
    let event_win = win.clone();
    win.on_window_event(move |event| manager_window_event(&event_app, &event_win, event));
    // A failed page load must never leave an invisible window.
    let fallback = win.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(2000)).await;
        if matches!(fallback.is_visible(), Ok(false)) {
            let _ = fallback.show();
            let _ = fallback.set_focus();
        }
    });
    Ok(())
}
