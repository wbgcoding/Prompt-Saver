use super::*;

// ---------- Snipping tool ----------
// open_snip freezes the active monitor, shows a transparent overlay window for
// the user to mark a region, then capture_region crops the frozen image,
// copies it to the clipboard, saves a PNG and hands the crop to the main UI.

// One frozen monitor: its capture plus its global physical origin and size.
// image is Arc'd so the background JPEG encode and the stored capture share one
// buffer instead of each owning an ~80 MB copy of the frozen desktop: every
// reader (crop_rgb/crop_rgba/sample_frozen) only ever borrows it.
pub(crate) struct MonitorCap {
    pub(crate) image: Arc<image::RgbaImage>,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}
pub(crate) struct SnipState(pub(crate) Mutex<Vec<MonitorCap>>);

// Arms the NEXT capture_region call to run OCR instead of a normal screenshot
// save/copy. Single-shot by construction -- close_all_snip resets it the moment
// any snip session actually ends (a successful capture, a cancel, or a window
// click), so it can never leak into a later, unrelated screenshot.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnipMode {
    Capture,
    Ocr,
}
pub(crate) struct SnipModeState(pub(crate) Mutex<SnipMode>);

// Overlay preview prepared in the background the instant the desktop is frozen,
// so the (heavy) JPEG encode overlaps WebView creation instead of running after
// it. None = still encoding, Some(None) = encode failed, Some(Some) = ready.
// Each ready entry carries the session generation its encode belonged to.
pub(crate) type SnipPreviewSlot = Option<(u64, Option<SnipBg>)>;
#[derive(Clone)]
pub(crate) struct SnipPreview(pub(crate) Arc<Mutex<SnipPreviewSlot>>);

// Store a finished preview encode, unless a newer session has started since: a slow
// encode of a cancelled snip must never become the next snip's background.
pub(crate) fn preview_slot_store<T>(slot: &mut Option<(u64, T)>, gen_now: u64, gen_of_encode: u64, value: T) -> bool {
    if gen_of_encode != gen_now {
        return false;
    }
    *slot = Some((gen_of_encode, value));
    true
}

// True while a snip session is on screen (between open_snip and capture/cancel).
// The overlay window itself is kept alive and reused, so this flag: not the
// window's existence: is what tells open_snip a snip is already in progress.
pub(crate) static SNIP_ACTIVE: AtomicBool = AtomicBool::new(false);

// True once the running session's overlay was put on screen (snip_present or the
// watchdog). The reveal watchdog keys off this, not off the window's visibility: on
// the reuse path the parked overlay already counts as visible.
pub(crate) static SNIP_PRESENTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn watchdog_should_reveal(session_active: bool, presented: bool) -> bool {
    session_active && !presented
}

// Bumped by begin_capture every time a session starts. A capture that is already
// running when the session is cancelled (and maybe a new one started) compares the
// number it saw at its start, so it can tell that its session is gone and must not
// copy, save, announce or close anything.
pub(crate) static SNIP_GEN: AtomicU64 = AtomicU64::new(0);

// True while the session a capture started in is still the running one.
pub(crate) fn finalize_allowed(gen_at_start: u64, gen_now: u64, active: bool) -> bool {
    active && gen_at_start == gen_now
}

pub(crate) fn snip_session_current(gen_at_start: u64) -> bool {
    finalize_allowed(gen_at_start, SNIP_GEN.load(Ordering::SeqCst), SNIP_ACTIVE.load(Ordering::SeqCst))
}

// The running session is a colour pick (loupe overlay) rather than a region
// snip. Both share the freeze + overlay pipeline; only the overlay's mode differs.
pub(crate) static PICK_ACTIVE: AtomicBool = AtomicBool::new(false);

// The running recording-target pick is for a live target switch on an
// already-running recording, not for starting a new one -- read once by
// recorder_target_picked to mark its emitted event so the two very different
// listeners (start a recording vs. swap its source) never both react to it.
pub(crate) static REC_SWITCH_PICK: AtomicBool = AtomicBool::new(false);

// The labels of the app windows begin_capture actually hid for this session (the
// ones that were visible beforehand), so the end of the session restores exactly
// those and nothing else: a snip started with screenshots allowed, or a window that
// was already hidden (main closed to the tray, a pill hidden on purpose), is never
// popped up by a snip.
pub(crate) static SNIP_HID_APP: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub(crate) fn restore_app_windows_after_snip(app: &AppHandle) {
    let hid = std::mem::take(&mut *SNIP_HID_APP.lock().unwrap_or_else(|e| e.into_inner()));
    if !hid.is_empty() {
        show_app_windows(app, &hid);
    }
}

// The hidden windows a snip session put away, as the restore set: only what was
// visible before the hide pass.
pub(crate) fn windows_to_restore(before: &[(String, bool)]) -> Vec<String> {
    before
        .iter()
        .filter(|(_, visible)| *visible)
        .map(|(label, _)| label.clone())
        .collect()
}

// Bring a snip's result to the main window (editor, result dialog, OCR text): the
// restore pass no longer shows a main window that was hidden before the snip, so
// the paths that hand something to main show it themselves.
pub(crate) fn reveal_main_for_snip_result(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

// End the current snip session: HIDE (never close) the reusable overlay so its
// warm WebView2 survives for the next snip, bring the app's own windows back, and
// drop the temp preview. Closing the overlay is what forced every snip to pay the
// full WebView2 cold-start: keeping it alive is the whole speed win.
pub(crate) fn close_all_snip(app: &AppHandle) {
    for (label, win) in app.webview_windows() {
        if label.starts_with("snip-") {
            park_snip_window(&win);
        }
    }
    restore_app_windows_after_snip(app);
    remove_snip_preview();
    // Ending a session invalidates every capture still running for it at once, so a
    // late one never ends, copies or announces on behalf of the next session.
    SNIP_GEN.fetch_add(1, Ordering::SeqCst);
    SNIP_ACTIVE.store(false, Ordering::SeqCst);
    SNIP_PRESENTED.store(false, Ordering::SeqCst);
    PICK_ACTIVE.store(false, Ordering::SeqCst);
    // Every path that ends a snip session (capture, cancel, window click)
    // funnels through here, so this is the one place that can guarantee OCR mode
    // never survives past the session it was armed for.
    if let Some(m) = app.try_state::<SnipModeState>() {
        *m.0.lock().unwrap_or_else(|e| e.into_inner()) = SnipMode::Capture;
    }
}

// Hide the app's own windows (the main window and floating pills) around a snip,
// so a screenshot never contains the app itself (this also reveals whatever the
// main window was covering, e.g. Task Manager). Returns the labels that were
// visible beforehand and are now hidden: that is the set the end of the session
// restores, and begin_capture skips its post-hide settle delay when it is empty.
pub(crate) fn hide_app_windows(app: &AppHandle) -> Vec<String> {
    let mut before: Vec<(String, bool)> = Vec::new();
    for (label, win) in app.webview_windows() {
        if is_snip_hidden_label(&label) {
            before.push((label, win.is_visible().unwrap_or(true)));
            let _ = win.hide();
        }
    }
    windows_to_restore(&before)
}

// A folder window is always-on-top exactly like a pill, and every other caller that
// hides float- windows for a screenshot already pairs it with folder-: missing it
// left an open folder sitting in the snip capture.
pub(crate) fn is_snip_hidden_label(label: &str) -> bool {
    label == "main" || label.starts_with("float-") || label.starts_with("folder-")
}

// Show the windows hide_app_windows put away. The main window also comes out of
// any minimized state and gets the focus, so finishing or cancelling a snip never
// leaves it behind other windows, which reads as the app having minimized itself.
pub(crate) fn show_app_windows(app: &AppHandle, labels: &[String]) {
    for label in labels {
        if let Some(win) = app.get_webview_window(label) {
            if label == "main" {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            } else {
                // A pill or folder must not take the focus from the app being typed in.
                show_without_focus(&win);
            }
        }
    }
}

// Hide (or re-show) only the snip overlay(s). Used by the window-capture
// workaround, which needs to grab the LIVE desktop: the overlay (showing the
// frozen image) would otherwise be what gets captured.
pub(crate) fn set_snip_overlay_hidden(app: &AppHandle, hidden: bool) {
    for (label, win) in app.webview_windows() {
        if label.starts_with("snip-") {
            let _ = if hidden { win.hide() } else { win.show() };
        }
    }
}

// The snip preview JPEG is written to a temp file and shown via the asset
// protocol: far quicker than passing a multi-MB base64 data URL through IPC and
// decoding it in the overlay. A rotating name avoids any stale webview cache.
pub(crate) static SNIP_PREVIEW_SEQ: AtomicU64 = AtomicU64::new(0);
pub(crate) static SNIP_PREVIEW_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

// None when the file could not be written or when the session `gen` belongs to is
// gone: a stale encode must never replace (and delete) the newer session's preview.
pub(crate) fn write_snip_preview(jpeg: &[u8], gen: u64) -> Option<String> {
    let seq = SNIP_PREVIEW_SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("{}-snip-{}.jpg", brand::TEMP_PREFIX, seq));
    let mut slot = SNIP_PREVIEW_FILE.lock().unwrap_or_else(|e| e.into_inner());
    if SNIP_GEN.load(Ordering::SeqCst) != gen {
        return None;
    }
    fs::write(&path, jpeg).ok()?;
    if let Some(old) = slot.replace(path.clone()) {
        let _ = fs::remove_file(old);
    }
    Some(path.to_string_lossy().into_owned())
}

// Crash-mid-session leftovers in the OS temp dir are never cleaned by anything
// else: the snip preview only ever deletes its own immediate predecessor (that
// bookkeeping is in-memory only, lost on a crash or a plain restart), and
// update.cmd deletes the installer + itself but never its own staging folder.
// Swept at startup and then on a slow cadence, since these are rare leftovers
// from an abnormal exit, not a live resource any running instance still needs.
// A snip preview lives only for the few minutes between taking a screenshot and
// saving/discarding it, so anything older than this was abandoned (a crash, a
// kill) rather than in active use: well short of catching a file another,
// concurrently running instance ("Allow multiple instances") is using right now.
pub(crate) const STALE_TEMP_FILE_AGE: Duration = Duration::from_secs(3600);
// How often the sweep repeats while the app keeps running.
pub(crate) const STALE_TEMP_SWEEP_SECS: u64 = 6 * 3600;

pub(crate) fn sweep_stale_temp_files() {
    sweep_stale_temp_files_in(&std::env::temp_dir());
}

// The sweep over an explicit temp root, so a test can point it at a folder of
// its own instead of the shared one.
pub(crate) fn sweep_stale_temp_files_in(dir: &Path) {
    // This process's own live preview is exempt whatever its age says. A modal
    // left open past the stale age is unusual but perfectly legitimate, and the
    // repeat sweeps mean the file is no longer guaranteed to be young.
    let live_preview = SNIP_PREVIEW_FILE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let snip_prefix = format!("{}-snip-", brand::TEMP_PREFIX);
    let update_prefix = format!("{}-update-", brand::SLUG);
    // Each of these dirs holds a staged ~8 MB MicrosoftEdgeWebview2Setup.exe that
    // nothing else ever deletes once the bootstrapper has run or been declined.
    let wv2_prefix = format!("{}-wv2-", brand::SLUG);
    let is_stale = |e: &fs::DirEntry| {
        e.metadata()
            .and_then(|m| m.modified())
            .and_then(|t| SystemTime::now().duration_since(t).map_err(std::io::Error::other))
            .is_ok_and(|age| age >= STALE_TEMP_FILE_AGE)
    };
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with(&snip_prefix) && name.ends_with(".jpg") {
                if is_stale(&e) && live_preview.as_deref() != Some(e.path().as_path()) {
                    let _ = fs::remove_file(e.path());
                }
            } else if (name.starts_with(&update_prefix) || name.starts_with(&wv2_prefix)) && is_stale(&e) {
                let _ = fs::remove_dir_all(e.path());
            }
        }
    }

    // OCR leftovers (a crop/page image and its install-result json) live one level
    // down, in the app's own TEMP_DIR subfolder -- the same folder the versioned
    // pdfium cache (`pdfium_cache_path`) lives in. Only the ocr-* names below are
    // ever touched here; the pdfium-<version>.dll cache is swept separately, by
    // version, in `sweep_stale_pdfium_dlls` -- never by age here.
    let ocr_dir = dir.join(brand::TEMP_DIR);
    if let Ok(entries) = fs::read_dir(&ocr_dir) {
        // 126 gave voice dictation its own CapKind::Speech leftovers in this same
        // folder ("-voice-..."), never added here -- an install/listing result,
        // script or DISM log for a language pack sat forever regardless of age.
        let ocr_prefix = format!("{}-ocr-", brand::TEMP_PREFIX);
        let voice_prefix = format!("{}-voice-", brand::TEMP_PREFIX);
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let is_ours = name.starts_with(&ocr_prefix) || name.starts_with(&voice_prefix);
            let is_ocr_png = is_ours && name.ends_with(".png");
            // Result files of the two elevated PowerShell runs (installing a pack,
            // listing the packs), and the DISM log/scratch redirect. Each
            // script deletes itself in its own `finally`, so a .ps1 here is only
            // ever one left behind by a hard kill mid-run.
            let is_elevated_leftover = is_ours && (name.ends_with(".json") || name.ends_with(".ps1") || name.ends_with(".log"));
            if (is_ocr_png || is_elevated_leftover) && is_stale(&e) {
                let _ = fs::remove_file(e.path());
            }
        }
    }

    // filmstrip_cache_dir's own comment used to claim this folder rode
    // along with the OCR/voice cleanup above -- it never did, since that loop
    // only ever matches flat files by name prefix, never a subdirectory. Each
    // (recording, requested thumbnail count) pair gets its own cache
    // subdirectory that nothing ever revisits once the recording is gone,
    // renamed, or re-encoded, so without this the folder only ever grows.
    let thumb_dir = ocr_dir.join("video-thumbs");
    if let Ok(entries) = fs::read_dir(&thumb_dir) {
        for e in entries.flatten() {
            if is_stale(&e) {
                let _ = fs::remove_dir_all(e.path());
            }
        }
    }

    // Working copies of a file opened for editing from outside the app
    // (never the original) -- one directory per open, holding exactly one
    // copy. The editor itself never deletes this once the window is closed,
    // so without this sweep the folder would only ever grow.
    let openedit_prefix = format!("{}-openedit-", brand::TEMP_PREFIX);
    // Still frames cut out of a video land in the same folder; a crash or kill
    // mid-extraction leaves the frame behind.
    let still_prefix = format!("{}-still-", brand::TEMP_PREFIX);
    if let Ok(entries) = fs::read_dir(&ocr_dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with(&openedit_prefix) && is_stale(&e) {
                let _ = fs::remove_dir_all(e.path());
            } else if name.starts_with(&still_prefix) && e.path().is_file() && is_stale(&e) {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

// Orderly exit: drop this process's own scratch files right away instead of
// leaving them for the age-based sweep. Only names carrying this pid are touched
// (working copies opened for editing, still frames), so a second running
// instance keeps its own files.
pub(crate) fn remove_own_temp_leftovers_in(temp_root: &Path, pid: u32) {
    let dir = temp_root.join(brand::TEMP_DIR);
    let openedit_prefix = format!("{}-openedit-", brand::TEMP_PREFIX);
    let openedit_suffix = format!("-{pid}");
    let still_prefix = format!("{}-still-{pid}-", brand::TEMP_PREFIX);
    let Ok(entries) = fs::read_dir(&dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(&openedit_prefix) && name.ends_with(&openedit_suffix) && e.path().is_dir() {
            let _ = fs::remove_dir_all(e.path());
        } else if name.starts_with(&still_prefix) && e.path().is_file() {
            let _ = fs::remove_file(e.path());
        }
    }
}

pub(crate) fn remove_own_temp_leftovers() {
    remove_snip_preview();
    remove_own_temp_leftovers_in(&std::env::temp_dir(), std::process::id());
}

// The "Copy to clipboard" export's own throwaway files, in the same
// TEMP_DIR subfolder the OCR/voice leftovers above use. Kept separate from
// sweep_stale_temp_files because its max age is user-configurable
// (videoClipTtlMin, default one hour) rather than the fixed
// STALE_TEMP_FILE_AGE every other class above uses.
// (count, bytes) removed -- the temporary-exports row's cleanup button
// reports what this actually did, same reasoning as sweep_recordings_now's
// own on-demand result.
pub(crate) fn sweep_clip_export_temp(age: Duration) -> (u32, u64) {
    sweep_clip_export_temp_in(&std::env::temp_dir().join(brand::TEMP_DIR), age)
}

// Same sweep over an explicit app temp folder (a test's own, not the shared one).
pub(crate) fn sweep_clip_export_temp_in(dir: &Path, age: Duration) -> (u32, u64) {
    let prefix = format!("{}-clipexport-", brand::TEMP_PREFIX);
    let Ok(entries) = fs::read_dir(dir) else { return (0, 0) };
    let mut count = 0u32;
    let mut bytes = 0u64;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with(&prefix) {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        let stale = meta
            .modified()
            .and_then(|t| SystemTime::now().duration_since(t).map_err(std::io::Error::other))
            .is_ok_and(|a| a >= age);
        if stale {
            let size = meta.len();
            if fs::remove_file(e.path()).is_ok() {
                count += 1;
                bytes += size;
            }
        }
    }
    (count, bytes)
}

// The overview row's amount readout for the same folder sweep_clip_export_temp cleans --
// every "-clipexport-" file regardless of age, so the overview row can show
// what is there right now, not just what the next sweep would remove.
pub(crate) fn clip_export_temp_stats() -> (u64, u64) {
    clip_export_temp_stats_in(&std::env::temp_dir().join(brand::TEMP_DIR))
}

// Same count over an explicit app temp folder (a test's own, not the shared one).
pub(crate) fn clip_export_temp_stats_in(dir: &Path) -> (u64, u64) {
    let prefix = format!("{}-clipexport-", brand::TEMP_PREFIX);
    let Ok(entries) = fs::read_dir(dir) else { return (0, 0) };
    let mut count = 0u64;
    let mut bytes = 0u64;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with(&prefix) {
            continue;
        }
        if let Ok(meta) = e.metadata() {
            count += 1;
            bytes += meta.len();
        }
    }
    (count, bytes)
}

pub(crate) fn clip_export_ttl_from(settings: &Settings) -> Duration {
    let min = settings.ui_values.get("videoClipTtlMin").copied().unwrap_or(60.0).max(1.0);
    Duration::from_secs((min * 60.0) as u64)
}

// The temporary-exports row's on-demand trigger.
#[tauri::command]
pub(crate) fn sweep_clip_export_temp_now(app: AppHandle) -> (u32, u64) {
    sweep_clip_export_temp(clip_export_ttl(&app))
}

// Reads the live expert setting so the periodic sweep always uses whatever
// the user has it set to right now, the same way update_check_interval does
// for its own re-timed job.
pub(crate) fn clip_export_ttl(app: &AppHandle) -> Duration {
    app.try_state::<Db>()
        .map(|s| clip_export_ttl_from(&s.lock().unwrap_or_else(|e| e.into_inner()).settings))
        .unwrap_or(Duration::from_secs(3600))
}

// How often the clip-export sweep re-checks -- finer than STALE_TEMP_SWEEP_SECS
// since videoClipTtlMin can be set as low as 5 minutes.
pub(crate) const CLIP_EXPORT_SWEEP_SECS: u64 = 5 * 60;

pub(crate) fn remove_snip_preview() {
    if let Some(p) = SNIP_PREVIEW_FILE.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = fs::remove_file(p);
    }
}

#[derive(Serialize, Clone)]
pub(crate) struct SnipBg {
    // Preview source: a temp-file path (is_file=true, loaded via convertFileSrc)
    // or a base64 data URL fallback (is_file=false).
    pub(crate) src: String,
    pub(crate) is_file: bool,
    // Full stitched dimensions (the display image may be downscaled); the
    // frontend maps coordinates against these, so crops stay full-resolution.
    pub(crate) width: u32,
    pub(crate) height: u32,
    // Run the overlay as a colour picker instead of a region selector, with
    // the loupe magnification and output notation the expert menu asks for.
    pub(crate) pick: bool,
    pub(crate) pick_zoom: u32,
    pub(crate) pick_format: String,
    // Loupe grid size, expert-tunable (kept odd by the frontend).
    pub(crate) pick_cells: u32,
    // The last picked colours as #rrggbb, newest first. Handed over with the
    // session so the overlay page never has to keep history state of its own.
    pub(crate) pick_history: Vec<String>,
    // Run the overlay as a recording-target picker instead: same window
    // hover/drag-region mechanics as a screenshot, but a click/drag resolves a
    // recording target instead of capturing an image.
    pub(crate) record: bool,
    // Stitched-canvas origin in true virtual-desktop physical pixels. The
    // overlay's own coordinates are canvas-relative (see SnipWindow); the
    // recorder needs true absolute coordinates to resolve which monitor a
    // region belongs to.
    pub(crate) origin_x: i32,
    pub(crate) origin_y: i32,
}

#[derive(Serialize, Clone)]
pub(crate) struct SnipResult {
    pub(crate) data_url: String,
    pub(crate) path: String,
    // Suggested button name: "Screenshot <app?> <timestamp>".
    pub(crate) name: String,
}

// A selectable top-level window in stitched-image physical pixels.
#[derive(Serialize)]
pub(crate) struct SnipWindow {
    pub(crate) id: u32,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

// ---- Fast screen capture via GDI (one BitBlt; no per-monitor DXGI setup) ----

// Capture a screen rectangle (global physical px) with GDI BitBlt. Immediate (no
// DXGI latency) and includes every visible window: elevated ones too.
#[cfg(windows)]
pub(crate) fn capture_screen_rect(x: i32, y: i32, w: i32, h: i32) -> Option<image::RgbaImage> {
    #[link(name = "user32")]
    extern "system" {
        fn GetDC(hwnd: isize) -> isize;
        fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(hdc: isize) -> isize;
        fn CreateCompatibleBitmap(hdc: isize, w: i32, h: i32) -> isize;
        fn SelectObject(hdc: isize, h: isize) -> isize;
        fn BitBlt(dst: isize, x: i32, y: i32, w: i32, h: i32, src: isize, sx: i32, sy: i32, rop: u32) -> i32;
        fn DeleteObject(h: isize) -> i32;
        fn DeleteDC(hdc: isize) -> i32;
        fn GetDIBits(hdc: isize, hbm: isize, start: u32, lines: u32, bits: *mut u8, bmi: *mut BmInfo, usage: u32) -> i32;
    }
    #[repr(C)]
    struct BmHeader {
        size: u32, width: i32, height: i32, planes: u16, bit_count: u16,
        compression: u32, size_image: u32, x_ppm: i32, y_ppm: i32, clr_used: u32, clr_important: u32,
    }
    #[repr(C)]
    struct BmInfo { header: BmHeader, colors: [u32; 3] }
    const SRCCOPY: u32 = 0x00CC_0020;
    // Reject non-positive or absurdly large rects: keep w*h*4 a sane allocation so
    // a spoofed/extreme virtual-screen metric can never abort the process on OOM.
    // 300M px ≈ 1.2 GB RGBA: far above any real multi-monitor desktop.
    if w <= 0 || h <= 0 || (w as u64) * (h as u64) > 300_000_000 {
        return None;
    }
    unsafe {
        let screen = GetDC(0);
        if screen == 0 {
            return None;
        }
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp);
        // SRCCOPY only (no CAPTUREBLT: it forces a slow full recomposite). The
        // DWM-composited desktop already includes every normal/elevated window.
        let blt_ok = BitBlt(mem, 0, 0, w, h, screen, x, y, SRCCOPY) != 0;
        // Deselect the bitmap before GetDIBits (required by the API).
        let _ = SelectObject(mem, old);
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let mut bmi = BmInfo {
            header: BmHeader {
                size: std::mem::size_of::<BmHeader>() as u32,
                width: w,
                height: -h, // negative => top-down rows
                planes: 1,
                bit_count: 32,
                compression: 0,
                size_image: 0,
                x_ppm: 0,
                y_ppm: 0,
                clr_used: 0,
                clr_important: 0,
            },
            colors: [0; 3],
        };
        let got = if blt_ok {
            GetDIBits(mem, bmp, 0, h as u32, buf.as_mut_ptr(), &mut bmi, 0)
        } else {
            0
        };
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(0, screen);
        if got == 0 {
            return None;
        }
        // GDI returns BGRA; convert to RGBA and force opaque alpha. Parallelized:
        // a multi-monitor capture is tens of millions of pixels, so a serial swap
        // was a big slice of the freeze time.
        use rayon::prelude::*;
        buf.par_chunks_exact_mut(4).for_each(|px| {
            px.swap(0, 2);
            px[3] = 255;
        });
        image::RgbaImage::from_raw(w as u32, h as u32, buf)
    }
}

// Whole virtual desktop via GDI → (image, origin_x, origin_y) global physical px.
#[cfg(windows)]
pub(crate) fn capture_desktop() -> Option<(image::RgbaImage, i32, i32)> {
    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(n: i32) -> i32;
    }
    const SM_XVIRTUALSCREEN: i32 = 76;
    const SM_YVIRTUALSCREEN: i32 = 77;
    const SM_CXVIRTUALSCREEN: i32 = 78;
    const SM_CYVIRTUALSCREEN: i32 = 79;
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let img = capture_screen_rect(vx, vy, vw, vh)?;
    Some((img, vx, vy))
}

// Freeze the whole desktop into one image (global physical px). Fast GDI path on
// Windows; per-monitor xcap stitch as a fallback (and on other platforms).
pub(crate) fn freeze_desktop() -> Result<(image::RgbaImage, i32, i32), String> {
    #[cfg(windows)]
    if let Some(res) = capture_desktop() {
        return Ok(res);
    }
    let mut caps = Vec::new();
    {
        let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
        for m in &monitors {
            if let Ok(image) = m.capture_image() {
                caps.push((image, m.x(), m.y(), m.width(), m.height()));
            }
        }
    }
    if caps.is_empty() {
        return Err("capture failed".to_string());
    }
    let min_x = caps.iter().map(|c| c.1).min().unwrap_or(0);
    let min_y = caps.iter().map(|c| c.2).min().unwrap_or(0);
    let max_x = caps.iter().map(|c| c.1 + c.3 as i32).max().unwrap_or(0);
    let max_y = caps.iter().map(|c| c.2 + c.4 as i32).max().unwrap_or(0);
    let total_w = (max_x - min_x).max(1) as u32;
    let total_h = (max_y - min_y).max(1) as u32;
    let mut canvas = image::RgbaImage::new(total_w, total_h);
    for (image, x, y, _, _) in &caps {
        image::imageops::replace(&mut canvas, image, stitch_offset(*x, min_x), stitch_offset(*y, min_y));
    }
    Ok((canvas, min_x, min_y))
}

// A monitor's global physical origin -> where its capture starts inside the
// stitched frozen image. The virtual desktop's origin is negative as soon as a
// monitor sits left of or above the primary, so this shift: not any DPI scale
// factor: is what makes a pixel on a differently scaled second monitor address
// the byte the capture actually holds. Every frozen coordinate is physical.
pub(crate) fn stitch_offset(global: i32, origin: i32) -> i64 {
    (global - origin) as i64
}

// ---------- colour picker ----------
// Same freeze + overlay as the snipping tool; the overlay just runs in pick mode.
// The picked value goes to the CLIPBOARD ONLY: no prompt, no copy history.

// Loupe magnification + output notation, straight from the expert menu.
pub(crate) struct PickCfg {
    pub(crate) zoom: u32,
    pub(crate) format: String,
    pub(crate) cells: u32,
}

// Only the three notations the expert dropdown offers; anything else is hex.
pub(crate) fn color_format(raw: &str) -> &'static str {
    match raw {
        "rgb" => "rgb",
        "hsl" => "hsl",
        _ => "hex",
    }
}

pub(crate) fn pick_cfg(settings: &Settings) -> PickCfg {
    PickCfg {
        zoom: settings.ui_values.get("loupeZoom").copied().unwrap_or(4.0).clamp(4.0, 16.0) as u32,
        format: color_format(settings.ui_texts.get("pickerFormat").map(String::as_str).unwrap_or("hex")).to_string(),
        cells: settings.ui_values.get("loupeCells").copied().unwrap_or(15.0).clamp(7.0, 25.0) as u32,
    }
}

pub(crate) fn rgb_to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

// The last picked colours, newest first. Recorded here rather than in the
// overlay page, because this is the side that knows what actually reached the
// clipboard: a hidden or reloaded webview cannot lose the list. Kept in memory
// for the running session, mirrored into `ui_texts["pickHistory"]` (comma-joined
// hex) so it survives a restart; the mirror is the only thing ever written to disk.
pub(crate) const PICK_HISTORY_DEFAULT: usize = 5;
pub(crate) const PICK_HISTORY_CAP: usize = 20;
pub(crate) static PICK_HISTORY: Mutex<Vec<[u8; 3]>> = Mutex::new(Vec::new());

// The user-configurable cap (expert value `pickHistoryN`, 0 = history off).
pub(crate) fn pick_history_max(settings: &Settings) -> usize {
    settings
        .ui_values
        .get("pickHistoryN")
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(PICK_HISTORY_DEFAULT as f64)
        .clamp(0.0, PICK_HISTORY_CAP as f64) as usize
}

pub(crate) fn hex_to_rgb(hex: &str) -> Option<[u8; 3]> {
    let d = hex.trim().strip_prefix('#')?;
    if d.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(d, 16).ok()?;
    Some([(n >> 16) as u8, ((n >> 8) & 0xff) as u8, (n & 0xff) as u8])
}

// Push a pick, newest first, skipping a repeat of the entry already on top.
pub(crate) fn record_pick(rgb: [u8; 3], max: usize) {
    let mut hist = PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    if hist.first() == Some(&rgb) {
        return;
    }
    hist.insert(0, rgb);
    hist.truncate(max);
}

pub(crate) fn pick_history_hex() -> Vec<String> {
    PICK_HISTORY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|c| rgb_to_hex(c[0], c[1], c[2]))
        .collect()
}

// One-time load from the persisted mirror: called once when the store loads.
pub(crate) fn load_pick_history(settings: &Settings) {
    let max = pick_history_max(settings);
    let mut hist = PICK_HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    hist.clear();
    if let Some(saved) = settings.ui_texts.get("pickHistory") {
        hist.extend(saved.split(',').filter_map(hex_to_rgb));
        hist.truncate(max);
    }
}

// RGB -> HSL as rounded integers (degrees, percent, percent). Grey has no hue, so
// it reports 0°/0%; the hue wraps so 360° is always written as 0°.
// clippy::float_cmp (pedantic): every `==` below compares a value straight out of
// f64::max/min against one of its own unmodified inputs -- max()/min() return one
// of their arguments bit-for-bit, no arithmetic happens in between, so this is
// exact identity, not the "drifted after computation" case the lint warns about.
#[allow(clippy::float_cmp)]
pub(crate) fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (u32, u32, u32) {
    let (rf, gf, bf) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0, 0, (l * 100.0).round() as u32);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == rf {
        ((gf - bf) / d).rem_euclid(6.0)
    } else if max == gf {
        (bf - rf) / d + 2.0
    } else {
        (rf - gf) / d + 4.0
    };
    let deg = (h * 60.0).round().rem_euclid(360.0);
    (deg as u32, (s * 100.0).round() as u32, (l * 100.0).round() as u32)
}

pub(crate) fn format_color(fmt: &str, r: u8, g: u8, b: u8) -> String {
    match color_format(fmt) {
        "rgb" => format!("rgb({},{},{})", r, g, b),
        "hsl" => {
            let (h, s, l) = rgb_to_hsl(r, g, b);
            format!("hsl({},{}%,{}%)", h, s, l)
        }
        _ => rgb_to_hex(r, g, b),
    }
}

// Read one pixel of the frozen desktop, addressed in stitched physical pixels.
// Out-of-bounds returns None instead of clamping, so a bogus coordinate can never
// silently copy the colour of some other pixel.
pub(crate) fn sample_frozen(caps: &[MonitorCap], x: i32, y: i32) -> Option<[u8; 3]> {
    let cap = caps.first()?;
    if x < 0 || y < 0 || x >= cap.width as i32 || y >= cap.height as i32 {
        return None;
    }
    let p = cap.image.get_pixel(x as u32, y as u32);
    Some([p[0], p[1], p[2]])
}

// One rectangular patch of the frozen desktop as packed RGB, row by row. The rect
// must lie fully inside the image: a caller that clamped badly gets nothing back
// rather than a silently shifted patch.
pub(crate) fn crop_rgb(img: &image::RgbaImage, x: u32, y: u32, w: u32, h: u32) -> Vec<u8> {
    let (iw, ih) = img.dimensions();
    // Checked, so a wild x/y from the overlay can never wrap into a valid-looking rect.
    let fits = |a: u32, len: u32, max: u32| a.checked_add(len).is_some_and(|end| end <= max);
    if w == 0 || h == 0 || !fits(x, w, iw) || !fits(y, h, ih) {
        return Vec::new();
    }
    let raw = img.as_raw();
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for row in y..y + h {
        let start = ((row * iw + x) * 4) as usize;
        for px in raw[start..start + (w * 4) as usize].chunks_exact(4) {
            out.extend_from_slice(&px[..3]);
        }
    }
    out
}

// What the main window needs to show the confirmation toast in the picked colour.
#[derive(Serialize, Clone)]
pub(crate) struct ColorPick {
    pub(crate) text: String,
    pub(crate) hex: String,
    // All three notations, pre-formatted: so the toast can offer them as
    // separate copy-this-format chips without duplicating format_color() in JS.
    pub(crate) rgb: String,
    pub(crate) hsl: String,
}

// Longest tile edge the overlay may ask for. The loupe needs a few dozen pixels
// around the cursor, so this is generous: but it is also the cap that stops a
// bogus request from allocating a desktop-sized buffer.
pub(crate) const PICK_TILE_MAX: u32 = 512;

// Hand the overlay ONE patch of the frozen desktop as packed RGB (3 bytes per
// pixel, row order). The loupe samples locally, so it caches a patch and only asks
// again when the cursor nears its edge: shipping the whole desktop instead cost a
// ~58 MB transfer on a multi-monitor setup, which is what made the picker slow to
// open. Empty outside a pick session or for a rect that is not fully on the frame.
#[tauri::command]
pub(crate) fn pick_tile(state: State<SnipState>, x: u32, y: u32, width: u32, height: u32) -> tauri::ipc::Response {
    if !PICK_ACTIVE.load(Ordering::SeqCst) || width > PICK_TILE_MAX || height > PICK_TILE_MAX {
        return tauri::ipc::Response::new(Vec::new());
    }
    let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = guard
        .first()
        .map(|c| crop_rgb(&c.image, x, y, width, height))
        .unwrap_or_default();
    tauri::ipc::Response::new(bytes)
}

// Sample the frozen desktop where the user clicked, format the value and put it on
// the clipboard. Deliberately no record_copy and no prompt: a picked colour is a
// clipboard result, nothing the app stores.
#[tauri::command]
pub(crate) async fn pick_color(app: AppHandle, state: State<'_, SnipState>, x: i32, y: i32) -> Result<String, String> {
    if !PICK_ACTIVE.load(Ordering::SeqCst) {
        return Err("no pick session".to_string());
    }
    let rgb = {
        let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        sample_frozen(&guard, x, y)
    }
    .ok_or("point outside the captured desktop")?;
    let fmt = app
        .try_state::<Db>()
        .map(|s| pick_cfg(&s.lock().unwrap_or_else(|e| e.into_inner()).settings).format)
        .unwrap_or_else(|| "hex".to_string());
    let text = format_color(&fmt, rgb[0], rgb[1], rgb[2]);
    note_own_copy(&text); // The picker's own copy must not land in the inbox
    // Close only once the value is really on the clipboard, so a failure leaves the
    // overlay up with its error banner instead of vanishing with nothing copied.
    if arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone())).is_err() {
        return Err("clipboard write failed".to_string());
    }
    // Only once the value really is on the clipboard, then mirror the
    // in-memory list into settings so it survives a restart.
    if let Some(db) = app.try_state::<Db>() {
        let mut store = db.lock().unwrap_or_else(|e| e.into_inner());
        record_pick(rgb, pick_history_max(&store.settings));
        store
            .settings
            .ui_texts
            .insert("pickHistory".to_string(), pick_history_hex().join(","));
        save_settings(&app, &store.settings);
    } else {
        record_pick(rgb, PICK_HISTORY_DEFAULT);
    }
    close_all_snip(&app);
    state.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let _ = app.emit(
        "color-picked",
        ColorPick {
            text: text.clone(),
            hex: format_color("hex", rgb[0], rgb[1], rgb[2]),
            rgb: format_color("rgb", rgb[0], rgb[1], rgb[2]),
            hsl: format_color("hsl", rgb[0], rgb[1], rgb[2]),
        },
    );
    Ok(text)
}

// The popover behind #picker-btn's right-click: the persisted history,
// respecting a cap the user may have lowered since the last pick.
#[tauri::command]
pub(crate) fn pick_history(app: AppHandle) -> Vec<String> {
    let max = app
        .try_state::<Db>()
        .map(|s| pick_history_max(&s.lock().unwrap_or_else(|e| e.into_inner()).settings))
        .unwrap_or(PICK_HISTORY_DEFAULT);
    let mut hex = pick_history_hex();
    hex.truncate(max);
    hex
}

// Re-copy one entry from that popover in the currently configured notation.
#[tauri::command]
pub(crate) fn copy_pick_history_entry(app: AppHandle, hex: String) -> Result<String, String> {
    let rgb = hex_to_rgb(&hex).ok_or("not a hex color")?;
    let fmt = app
        .try_state::<Db>()
        .map(|s| pick_cfg(&s.lock().unwrap_or_else(|e| e.into_inner()).settings).format)
        .unwrap_or_else(|| "hex".to_string());
    let text = format_color(&fmt, rgb[0], rgb[1], rgb[2]);
    note_own_copy(&text); // Must not land in the clipboard inbox either
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.clone()))
        .map_err(|_| "clipboard write failed".to_string())?;
    Ok(text)
}

// Where and how the idle, pre-warmed snip overlay sits: a few pixels, far outside
// every monitor, with an opaque dark native backdrop. Should anything ever show it
// while no snip is running, it is then a speck nobody can see, never the default
// 800x600 white rectangle at the system's default spot on the main screen.
pub(crate) struct SnipPrewarmSpec {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) color: (u8, u8, u8, u8),
}

pub(crate) fn snip_prewarm_spec() -> SnipPrewarmSpec {
    SnipPrewarmSpec { width: 2, height: 2, x: -32000, y: -32000, color: (0, 0, 0, 255) }
}

// Pure: an overlay that is on screen while no snip session owns it has to go away.
pub(crate) fn snip_idle_needs_parking(session_active: bool, visible: bool) -> bool {
    visible && !session_active
}

// Put the overlay away again when something showed it outside a session: tiny,
// off-screen and hidden, so the next snip still finds its warm webview.
pub(crate) fn park_idle_snip_overlay(app: &AppHandle) {
    let Some(win) = app.get_webview_window("snip-0") else { return };
    if !snip_idle_needs_parking(SNIP_ACTIVE.load(Ordering::SeqCst), win.is_visible().unwrap_or(false)) {
        return;
    }
    park_snip_window(&win);
}

// Hidden, tiny and far off-screen: the resting place of the overlay between
// snips. A session ends with this too, so a hidden overlay never keeps the full
// desktop size it had for the last snip; the next snip sets its bounds again
// before it reveals the window.
pub(crate) fn park_snip_window(win: &tauri::WebviewWindow) {
    let spec = snip_prewarm_spec();
    let _ = win.hide();
    let _ = win.set_size(PhysicalSize::new(spec.width, spec.height));
    let _ = win.set_position(PhysicalPosition::new(spec.x, spec.y));
}

// Build the (hidden) snip overlay window: borderless WS_POPUP, no shadow,
// always-on-top. It is born tiny and off-screen (snip_prewarm_spec); every real
// snip sets the full desktop bounds before it reveals the window. Created once
// and reused across snips so the WebView2 cold-start is paid a single time. A
// Destroyed handler clears the session and restores the app's windows if the
// overlay is ever torn down.
pub(crate) fn build_snip_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let spec = snip_prewarm_spec();
    let win = WebviewWindowBuilder::new(app, "snip-0", WebviewUrl::App("snip.html".into()))
        .title("")
        .inner_size(f64::from(spec.width), f64::from(spec.height))
        .position(f64::from(spec.x), f64::from(spec.y))
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    // Native backdrop dark instead of the default white, and the exact physical
    // geometry regardless of DPI scaling (the builder values are logical).
    let _ = win.set_background_color(Some(tauri::webview::Color(spec.color.0, spec.color.1, spec.color.2, spec.color.3)));
    let _ = win.set_size(PhysicalSize::new(spec.width, spec.height));
    let _ = win.set_position(PhysicalPosition::new(spec.x, spec.y));
    let excluded = app
        .try_state::<Db>()
        .map(|s| capture_excluded(&s.lock().unwrap_or_else(|e| e.into_inner()).settings))
        .unwrap_or(true);
    set_capture_exclusion(&win, excluded);
    let app_evt = app.clone();
    win.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) && SNIP_ACTIVE.load(Ordering::SeqCst) {
            SNIP_GEN.fetch_add(1, Ordering::SeqCst);
            SNIP_ACTIVE.store(false, Ordering::SeqCst);
            restore_app_windows_after_snip(&app_evt);
        }
    });
    Ok(win)
}

// Freeze the desktop and open a single overlay spanning all screens. The
// frontend maps the cursor by ratio against the displayed frozen image, so the
// selection is pixel-exact on any DPI mix AND can be dragged across monitors.
//
// Speed: the preview JPEG is encoded on a background thread the moment the
// desktop is frozen, so it runs CONCURRENTLY with WebView creation rather than
// serially after it; the overlay window is built hidden and only revealed once
// its frozen image has decoded (snip_present), so it never flashes up blank.
#[tauri::command]
pub(crate) async fn open_snip(
    app: AppHandle,
    state: State<'_, SnipState>,
    preview: State<'_, SnipPreview>,
) -> Result<(), String> {
    begin_capture(app, state, preview, false, false).await
}

// The very same freeze + overlay, opened in colour-pick mode. Reusing the
// pipeline is what makes the picker pixel-exact and DPI-correct for free: the
// frozen frame is taken BEFORE the overlay shows, so the overlay is never in its
// own capture source.
#[tauri::command]
pub(crate) async fn open_picker(
    app: AppHandle,
    state: State<'_, SnipState>,
    preview: State<'_, SnipPreview>,
) -> Result<(), String> {
    let on = app
        .try_state::<Db>()
        .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).settings.ui_flags.get("colorPicker") != Some(&false))
        .unwrap_or(true); // on by default: only an explicit false switches it off
    if !on {
        return Err("color picker is off".to_string());
    }
    REC_SWITCH_PICK.store(false, Ordering::SeqCst);
    begin_capture(app, state, preview, true, false).await
}

// The same overlay, opened as a recording-target picker: a click on a
// window or a dragged region resolves the recording target instead of
// capturing an image. No on/off check here -- the screenRecord feature toggle
// already gates whether the composer button that calls this even shows.
#[tauri::command]
pub(crate) async fn open_recorder(
    app: AppHandle,
    state: State<'_, SnipState>,
    preview: State<'_, SnipPreview>,
) -> Result<(), String> {
    if recording_active(&app) {
        return Err("recording-busy".to_string());
    }
    REC_SWITCH_PICK.store(false, Ordering::SeqCst);
    begin_capture(app, state, preview, false, true).await
}

// The same picker again, opened FROM the running recording's own control bar
// to swap its source instead of starting a new recording -- the only
// difference from open_recorder is the flag recorder_target_picked reads to
// mark its result as a switch, not a start.
#[tauri::command]
pub(crate) async fn open_recorder_switch(
    app: AppHandle,
    state: State<'_, SnipState>,
    preview: State<'_, SnipPreview>,
) -> Result<(), String> {
    REC_SWITCH_PICK.store(true, Ordering::SeqCst);
    begin_capture(app, state, preview, false, true).await
}

pub(crate) async fn begin_capture(
    app: AppHandle,
    state: State<'_, SnipState>,
    preview: State<'_, SnipPreview>,
    pick: bool,
    record: bool,
) -> Result<(), String> {
    // Atomically claim the session. If one is already active and its overlay still
    // exists, surface that instead of starting a second. (Stale flag with no
    // window (e.g. the overlay was destroyed) falls through: reclaim and start.)
    if SNIP_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        if app.get_webview_window("snip-0").is_some() {
            // Same bounds assert as the normal reveal; a bare show() would leave a
            // parked overlay at its parking spot.
            if !reveal_snip(&app).await {
                if let Some(win) = app.get_webview_window("snip-0") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            return Ok(());
        }
        SNIP_ACTIVE.store(true, Ordering::SeqCst);
    }
    SNIP_PRESENTED.store(false, Ordering::SeqCst);
    PICK_ACTIVE.store(pick, Ordering::SeqCst);
    let gen = SNIP_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    // Loupe/quality/delay settings are read once here, in both capture modes,
    // while nothing else holds the store.
    let (cfg, hist_max, snip_delay, hide_app) = app
        .try_state::<Db>()
        .map(|s| {
            let guard = s.lock().unwrap_or_else(|e| e.into_inner());
            let settings = &guard.settings;
            let snip_delay = settings.ui_values.get("snipDelayMs").copied().unwrap_or(0.0).clamp(0.0, 5000.0) as u64;
            let cfg = if pick { pick_cfg(settings) } else { PickCfg { zoom: 8, format: "hex".to_string(), cells: 15 } };
            let hist_max = if pick { pick_history_max(settings) } else { PICK_HISTORY_DEFAULT };
            (cfg, hist_max, snip_delay, capture_hides_app_windows(capture_excluded(settings)))
        })
        .unwrap_or((PickCfg { zoom: 8, format: "hex".to_string(), cells: 15 }, PICK_HISTORY_DEFAULT, 0, true));
    // Wake the overlay's WebView2 NOW (off-screen) so a long-idle, background-throttled
    // webview un-throttles DURING the freeze+encode below. Without this head start the
    // icon path (overlay idle a while) lags behind a retry (whose webview was just
    // on-screen, still awake). The reuse path re-parks it full off-screen, then emits.
    if let Some(win) = app.get_webview_window("snip-0") {
        let _ = win.set_size(PhysicalSize::new(2, 2));
        let _ = win.set_position(PhysicalPosition::new(-32000, -32000));
        let _ = win.show();
    }
    // Hide our own windows BEFORE freezing so they're never in the capture and
    // the target underneath (e.g. Task Manager behind the main window) is
    // revealed. A short settle lets the hide reach the screen first.
    // With screenshots of the app allowed, nothing is hidden at all.
    let hid = if hide_app { hide_app_windows(&app) } else { Vec::new() };
    let had_visible = !hid.is_empty();
    *SNIP_HID_APP.lock().unwrap_or_else(|e| e.into_inner()) = hid;
    // Nothing was on screen to hide, so there is nothing the freeze could catch
    // mid-hide either: the settle delay only matters when we just hid something.
    if had_visible {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    // An optional extra delay before the freeze, so an opened menu can be
    // captured too. Screenshot path only: the colour picker needs the frozen
    // desktop as fast as possible.
    if !pick && snip_delay > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(snip_delay)).await;
    }
    // The session may have been cancelled (and a newer one started) during the
    // waits above: then it is not ours any more and nothing below may run.
    if !snip_session_current(gen) {
        return Ok(());
    }
    let (canvas, min_x, min_y) = match freeze_desktop() {
        Ok(v) => v,
        Err(e) => {
            // The session never got going: hide the parked overlay, clear the
            // flags and bring back whatever was hidden for it.
            if snip_session_current(gen) {
                close_all_snip(&app);
            }
            return Err(e);
        }
    };
    if !snip_session_current(gen) {
        return Ok(());
    }
    let (total_w, total_h) = canvas.dimensions();
    // Arc'd so the preview encode (worker thread) and the stored capture (full-res
    // crops) share one buffer instead of each paying for their own ~80 MB copy.
    let canvas = Arc::new(canvas);
    let enc_img = canvas.clone();
    *state.0.lock().unwrap_or_else(|e| e.into_inner()) = vec![MonitorCap {
        image: canvas,
        x: min_x,
        y: min_y,
        width: total_w,
        height: total_h,
    }];
    let slot = preview.0.clone();
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = None;
    tauri::async_runtime::spawn_blocking(move || {
        // A newer session already owns the slot (and the preview file): nothing to do.
        if SNIP_GEN.load(Ordering::SeqCst) != gen {
            return;
        }
        // Encode the capture at NATIVE resolution: pixel-sharp overlay, no downscale.
        let bg = encode_snip_jpeg(enc_img.as_raw(), total_w, total_h, if pick { PICK_Q } else { SNIP_PREVIEW_Q }).map(|j| {
            let (src, is_file) = match write_snip_preview(&j, gen) {
                Some(path) => (path, true),
                None => (format!("data:image/jpeg;base64,{}", base64_encode(&j)), false),
            };
            SnipBg {
                src,
                is_file,
                width: total_w,
                height: total_h,
                pick,
                pick_zoom: cfg.zoom,
                pick_format: cfg.format,
                pick_cells: cfg.cells,
                pick_history: if pick {
                    let mut h = pick_history_hex();
                    h.truncate(hist_max);
                    h
                } else {
                    Vec::new()
                },
                record,
                origin_x: min_x,
                origin_y: min_y,
            }
        });
        let gen_now = SNIP_GEN.load(Ordering::SeqCst);
        preview_slot_store(&mut slot.lock().unwrap_or_else(|e| e.into_inner()), gen_now, gen, bg);
    });

    // Reuse the overlay if it already exists. A long-idle HIDDEN WebView2 is heavily
    // background-throttled (clamped timers, paused rAF/IPC): THAT, not cold-start, is
    // why the first snip lagged behind a retry (whose webview was shown moments ago).
    // So wake it WITHOUT a flash: show it 2×2 off-screen, which flips the page to
    // "visible" and resumes its JS at full speed. snip_present (fired on the frozen
    // image's onload, now prompt) sizes + moves it on-screen, so the reveal is instant
    // and flash-free. Same path for the first snip and every retry → same speed.
    if let Some(win) = app.get_webview_window("snip-0") {
        // Park it at FULL size but just off-screen (below everything), shown so the
        // webview un-throttles and paints. snip_present then only MOVES it on-screen:
        // no resize, no DPI re-assert: so the reveal can't flicker.
        let _ = win.set_size(PhysicalSize::new(total_w, total_h));
        let _ = win.set_position(PhysicalPosition::new(min_x, min_y + total_h as i32 + 100));
        let _ = win.show();
        let _ = app.emit_to("snip-0", "snip-begin", ());
        spawn_reveal_watchdog(app.clone(), gen);
        return Ok(());
    }

    // Pre-warm missed (first snip happened before startup pre-warm built it): build
    // the overlay now, paying the WebView2 cold-start once. SNIP_ACTIVE is already
    // set, so the page's load-time snip_should_begin check drives this session.
    let win = match build_snip_window(&app) {
        Ok(w) => w,
        Err(e) => {
            // Build failed: clear the session so the app's windows come back.
            SNIP_ACTIVE.store(false, Ordering::SeqCst);
            restore_app_windows_after_snip(&app);
            return Err(e);
        }
    };
    // Size/position the still-hidden window to cover the virtual desktop exactly
    // (size first, then position, so the final op is the position a resize could
    // otherwise nudge). snip_present re-asserts after the DPI settle, then shows.
    let _ = win.set_size(PhysicalSize::new(total_w, total_h));
    let _ = win.set_position(PhysicalPosition::new(min_x, min_y));
    spawn_reveal_watchdog(app.clone(), gen);
    Ok(())
}

// Show the overlay (idempotent): place it to the frozen-desktop geometry, reveal,
// and re-assert after the per-monitor DPI settle the show can trigger. Used by the
// page (snip_present, fast path after the frozen image decodes) and by a backend
// watchdog that force-reveals if the page somehow never asks.
pub(crate) async fn reveal_snip(app: &AppHandle) -> bool {
    let geom = {
        let state: State<SnipState> = app.state();
        let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.first().map(|c| (c.x, c.y, c.width, c.height))
    };
    let (min_x, min_y, total_w, total_h) = match geom {
        Some(g) => g,
        None => return false,
    };
    SNIP_PRESENTED.store(true, Ordering::SeqCst);
    if let Some(win) = app.get_webview_window("snip-0") {
        // Already full-size (parked off-screen on reuse), so this moves it on-screen in
        // one shot: no resize, no re-assert → no flicker. The frozen image is already
        // decoded (snip_present fires on its onload). set_size covers the cold path.
        let _ = win.set_size(PhysicalSize::new(total_w, total_h));
        let _ = win.set_position(PhysicalPosition::new(min_x, min_y));
        let _ = win.show();
        let _ = win.set_focus();
    }
    true
}

// Reveal the prepared overlay once its frozen image has decoded in the page, so
// the screenshot view appears already-painted instead of flashing blank first.
#[tauri::command]
pub(crate) async fn snip_present(app: AppHandle) -> Result<(), String> {
    reveal_snip(&app).await;
    Ok(())
}

// Belt-and-suspenders: if the page hasn't revealed the overlay shortly after a
// snip starts (e.g. a hidden WebView2 ever suspended its JS), force it visible so
// the screenshot view can never get stuck hidden. No-op once already shown.
pub(crate) fn spawn_reveal_watchdog(app: AppHandle, gen: u64) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        if snip_session_current(gen) && watchdog_should_reveal(true, SNIP_PRESENTED.load(Ordering::SeqCst)) {
            reveal_snip(&app).await;
        }
    });
}

// The overlay page asks this on load: begin a session immediately only if one is
// already active (the cold-build path), otherwise stay idle: this is what lets the
// window be pre-warmed at startup without auto-triggering a bogus snip. Reused
// windows are driven per-snip by the "snip-begin" event instead.
#[tauri::command]
pub(crate) fn snip_should_begin() -> bool {
    SNIP_ACTIVE.load(Ordering::SeqCst)
}

// Rect (x, y, width, height) the overlay hit-tests and highlights for a window:
// its visible frame when known, else the client rect xcap reports.
pub(crate) fn snip_window_rect(frame: Option<(i32, i32, i32, i32)>, client: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    frame.unwrap_or(client)
}

// The window's visible frame in global physical pixels (DWM extended frame
// bounds: no invisible resize border, but title bar and borders included).
#[cfg(windows)]
pub(crate) fn window_frame_rect(id: u32) -> Option<(i32, i32, i32, i32)> {
    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmGetWindowAttribute(hwnd: isize, attr: u32, out: *mut Rect, size: u32) -> i32;
    }
    const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
    let mut r = Rect::default();
    let hr = unsafe { DwmGetWindowAttribute(id as i32 as isize, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r, std::mem::size_of::<Rect>() as u32) };
    if hr != 0 || r.right <= r.left || r.bottom <= r.top {
        return None;
    }
    Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
}

#[cfg(not(windows))]
pub(crate) fn window_frame_rect(_id: u32) -> Option<(i32, i32, i32, i32)> {
    None
}

// Visible top-level windows overlapping the virtual desktop, topmost first, in
// stitched-image physical pixels: for hover highlight + single-window capture.
#[tauri::command]
pub(crate) fn snip_windows(state: State<SnipState>, index: usize) -> Vec<SnipWindow> {
    // Read the stitched origin/bounds, then drop the lock before the OS-wide
    // window enumeration so the snip state isn't held during the slow Win32 calls.
    let (ox, oy, ow, oh) = {
        let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        match guard.get(index) {
            Some(c) => (c.x, c.y, c.width as i32, c.height as i32),
            None => return Vec::new(),
        }
    };
    let mut out = Vec::new();
    if let Ok(windows) = xcap::Window::all() {
        for w in windows {
            if w.is_minimized() || w.title().is_empty() {
                continue;
            }
            // The hit-test rect is the window's visible frame (title bar and borders
            // included); xcap's own x/y/width/height is only the client area.
            let (gx, gy, ww, wh) = snip_window_rect(window_frame_rect(w.id()), (w.x(), w.y(), w.width() as i32, w.height() as i32));
            if ww <= 0 || wh <= 0 {
                continue;
            }
            // Keep only windows overlapping the virtual desktop.
            if gx >= ox + ow || gy >= oy + oh || gx + ww <= ox || gy + wh <= oy {
                continue;
            }
            // Local to the stitched image.
            out.push(SnipWindow {
                id: w.id(),
                x: gx - ox,
                y: gy - oy,
                width: ww as u32,
                height: wh as u32,
            });
        }
    }
    out
}

// The colour picker shows the frozen desktop as the reference for the colour it is
// about to copy, so its background must not carry JPEG's subsampled chroma: below
// quality 90 the encoder halves the colour resolution, which is exactly the "washed
// out" look. 92 keeps full chroma (4:4:4).
pub(crate) const PICK_Q: u8 = 92;
// The screenshot overlay shows the same frozen desktop at native resolution, and
// coloured text on it smears under subsampled chroma just the same, so it uses the
// same full-chroma quality. Fixed, not a setting.
pub(crate) const SNIP_PREVIEW_Q: u8 = PICK_Q;
// Saved-button JPEG quality: higher (crisp text) but still a fraction of a PNG's
// size + encode time: a full-res PNG data URL used to stall the whole app on save.
// Default for the expert-tunable snipSaveQuality (read in finalize_capture) --
// so a screenshot and its post-edit re-export share one visible quality.
pub(crate) const SNIP_SAVE_Q: u8 = 90;

// Encode RGBA bytes to a JPEG for the overlay preview. jpeg-encoder = SIMD baseline JPEG,
// much faster than the image crate's. Returns the raw JPEG bytes.
pub(crate) fn encode_snip_jpeg(rgba: &[u8], w: u32, h: u32, quality: u8) -> Option<Vec<u8>> {
    let mut buf = Vec::<u8>::new();
    jpeg_encoder::Encoder::new(&mut buf, quality)
        .encode(rgba, w as u16, h as u16, jpeg_encoder::ColorType::Rgba)
        .ok()?;
    Some(buf)
}

// The screenshot format setting's file extension and data-URL mime type --
// one place for both, so a fifth format never needs updating in two spots
// that could drift apart. Any value other than the four named ones (an old
// setting from before this list existed, a hand-edited settings file) falls
// back to png, the current default.
pub(crate) fn screenshot_format_ext(format: &str) -> &'static str {
    match format {
        "jpeg" => "jpg",
        "webp" => "webp",
        "bmp" => "bmp",
        _ => "png",
    }
}
pub(crate) fn screenshot_format_mime(format: &str) -> &'static str {
    match format {
        "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/png",
    }
}

// Mime for a format the `image` crate itself sniffed from a file's content
// when opening it for editing -- distinct from screenshot_format_mime above,
// which maps the SETTING's chosen output format, not a guessed input one.
pub(crate) fn image_format_mime(fmt: image::ImageFormat) -> &'static str {
    match fmt {
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Bmp => "image/bmp",
        image::ImageFormat::Tiff => "image/tiff",
        image::ImageFormat::Gif => "image/gif",
        image::ImageFormat::Ico => "image/x-icon",
        _ => "image/png",
    }
}

// Encodes a screenshot in whichever of the four formats the setting names.
// JPEG keeps the fast SIMD path above; the other four go through the image
// crate, already linked for thumbnails elsewhere. All four are written
// lossless -- the image crate's own WebP encoder only ever writes lossless
// VP8L, it has no lossy/quality mode to plug snipSaveQuality into, so quality
// only ever applies to JPEG (see screenshot_format_needs_quality below).
pub(crate) fn encode_screenshot(rgba: &[u8], w: u32, h: u32, format: &str, jpeg_quality: u8) -> Option<Vec<u8>> {
    if screenshot_format_needs_quality(format) {
        return encode_snip_jpeg(rgba, w, h, jpeg_quality);
    }
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec())?;
    let out_format = match format {
        "webp" => image::ImageFormat::WebP,
        "bmp" => image::ImageFormat::Bmp,
        _ => image::ImageFormat::Png,
    };
    let mut buf = Vec::<u8>::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut buf), out_format)
        .ok()?;
    Some(buf)
}

// Whether the quality slider means anything for this format -- JPEG only
// (see encode_screenshot's own comment on why WebP does not qualify too,
// despite the image crate technically supporting it as a target format).
pub(crate) fn screenshot_format_needs_quality(format: &str) -> bool {
    format == "jpeg"
}

// A screenshot's own data URL is always PNG or whatever the format setting
// already was when it was captured (see finalize_capture) -- both the "Save
// as" dialog and a later "save to folder" toggle can still ask for a
// DIFFERENT format than what is already encoded (the setting changed since
// capture, or the save dialog's own per-operation choice), so both decode
// back to raw pixels first and re-encode fresh rather than writing the
// existing bytes under a new, wrong extension. Lossless start to lossless
// end never loses anything; only landing on jpeg is ever lossy, exactly once.
pub(crate) fn reencode_screenshot_bytes(bytes: &[u8], format: &str, quality: u8) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    encode_screenshot(&img.into_rgba8().into_raw(), w, h, format, quality).ok_or_else(|| "encode failed".to_string())
}

// Hand the overlay its frozen-desktop preview. The encode was started in
// open_snip the instant the desktop was frozen; here we just wait for it (the
// slot is usually already filled, so this returns near-instantly) and hand it
// over. index is kept for the existing single-overlay call signature.
#[tauri::command]
pub(crate) async fn snip_background(preview: State<'_, SnipPreview>, index: usize) -> Result<Option<SnipBg>, String> {
    let _ = index;
    for _ in 0..2000 {
        let ready = preview.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
        // An entry of an older session counts as "still encoding".
        if let Some((g, bg)) = ready {
            if g == SNIP_GEN.load(Ordering::SeqCst) {
                return Ok(bg);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    Ok(None)
}

#[tauri::command]
pub(crate) fn snip_cancel(app: AppHandle, state: State<SnipState>) {
    close_all_snip(&app);
    state.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

// The recording overlay's window-click / region-drag resolved to a target:
// close the overlay (same cleanup as a real screenshot) and hand the
// target to the main window, which runs the countdown and opens the control
// bar. `x`/`y`/`width`/`height` are stitched-canvas-relative for "window"
// (unused there) and absolute virtual-desktop physical pixels for "region"
// (the caller adds SnipBg's origin_x/origin_y before calling this).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) fn recorder_target_picked(
    app: AppHandle,
    state: State<SnipState>,
    kind: String,
    id: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let mut payload = match kind.as_str() {
        "window" => serde_json::json!({ "kind": "window", "id": id }),
        "monitor" => {
            let (monitor, ..) = resolve_region_monitor(x, y, 1, 1)?;
            serde_json::json!({ "kind": "monitor", "monitor": monitor })
        }
        "region" => {
            if width == 0 || height == 0 {
                return Err("region has no size".to_string());
            }
            let (monitor, rx, ry) = resolve_region_monitor(x, y, width, height)?;
            serde_json::json!({ "kind": "region", "monitor": monitor, "x": rx, "y": ry, "width": width, "height": height })
        }
        _ => return Err("invalid recording target".to_string()),
    };
    // A pick opened from the running recording's own bar (open_recorder_switch)
    // is meant to swap that recording's source, not start a second one -- see
    // REC_SWITCH_PICK's own doc comment for why this needs a flag at all.
    if REC_SWITCH_PICK.swap(false, Ordering::SeqCst) {
        payload["forSwitch"] = serde_json::json!(true);
    }
    close_all_snip(&app);
    state.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let _ = app.emit("recorder-target-picked", payload);
    Ok(())
}

// Which monitor a region belongs to (its HMONITOR, per Win32 convention), and
// the region's origin relative to that monitor's own top-left -- what
// RecTarget::Region needs, since WGC captures a whole monitor and the crop
// happens in software. Same raw-extern style as set_capture_exclusion /
// window_is_protected, rather than pulling in more `windows` crate features
// for two calls used nowhere else.
#[cfg(windows)]
pub(crate) fn resolve_region_monitor(x: i32, y: i32, w: u32, h: u32) -> Result<(i64, i32, i32), String> {
    #[repr(C)]
    struct RectI { left: i32, top: i32, right: i32, bottom: i32 }
    #[link(name = "user32")]
    extern "system" {
        fn MonitorFromRect(rect: *const RectI, flags: u32) -> isize;
    }
    const MONITOR_DEFAULTTONEAREST: u32 = 2;

    let rect = RectI { left: x, top: y, right: x + w as i32, bottom: y + h as i32 };
    let hmon = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST) };
    if hmon == 0 {
        return Err("could not resolve a monitor for this region".to_string());
    }
    let (left, top, ..) = monitor_bounds(hmon).ok_or_else(|| "could not read monitor bounds".to_string())?;
    Ok((hmon as i64, x - left, y - top))
}
#[cfg(not(windows))]
pub(crate) fn resolve_region_monitor(x: i32, y: i32, _w: u32, _h: u32) -> Result<(i64, i32, i32), String> {
    Ok((0, x, y))
}

// Arms (or disarms) OCR mode for the very next capture_region call, called by
// the frontend right before it opens the capture overlay for the "Screenshot" entry
// of the OCR menu. close_all_snip is what guarantees this never stays armed past
// that one session.
#[tauri::command]
pub(crate) fn set_snip_mode(state: State<'_, SnipModeState>, mode: String) -> Result<(), String> {
    *state.0.lock().unwrap_or_else(|e| e.into_inner()) = if mode == "ocr" { SnipMode::Ocr } else { SnipMode::Capture };
    Ok(())
}

// Target folder for saved screenshots: a custom expert path, else Pictures\Screenshots.
pub(crate) fn screenshot_dir(custom: &str) -> PathBuf {
    let c = custom.trim();
    if !c.is_empty() {
        return PathBuf::from(c);
    }
    let base = std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("Pictures").join("Screenshots")
}

// Save already-encoded screenshot bytes into the given folder as "<stem>.<ext>".
// "" on failure. Never re-encodes -- the caller decides the format up front.
pub(crate) fn save_screenshot(bytes: &[u8], dir: &std::path::Path, stem: &str, ext: &str) -> String {
    use std::io::Write;
    let _ = fs::create_dir_all(dir);
    // Never replace a file that is already there (two shots in the same second
    // share a stem): take the next free " (2)", " (3)" name instead.
    for n in 1..1000u32 {
        let name = if n == 1 { format!("{}.{}", stem, ext) } else { format!("{} ({}).{}", stem, n, ext) };
        let path = dir.join(name);
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                return if f.write_all(bytes).is_ok() {
                    path.to_string_lossy().to_string()
                } else {
                    let _ = fs::remove_file(&path);
                    String::new()
                };
            }
            // Taken, or a directory of that name (PermissionDenied): try the next suffix.
            Err(_) => continue,
        }
    }
    String::new()
}

// Decode a "data:...;base64,..." URL to its raw image bytes (format-agnostic).
pub(crate) fn data_url_bytes(data_url: &str) -> Vec<u8> {
    let b64 = data_url.rsplit_once(',').map(|(_, b)| b).unwrap_or(data_url);
    base64_decode(b64.trim())
}

// Save a screenshot data URL into Pictures\Screenshots now (result dialog: the
// user enabled folder-saving after the shot was taken, or an edit changed the
// image after an earlier auto-save). Returns the path. `format` overrides the
// screenshotFormat setting for this one save -- None uses the setting as-is.
#[tauri::command]
pub(crate) async fn save_screenshot_now(state: State<'_, Db>, data_url: String, format: Option<String>) -> Result<Option<String>, String> {
    if !data_url_within_limit(&data_url, MAX_IMAGE_DATA_URL_BYTES) {
        return Err(format!(
            "image is too large ({} MB); the limit is {} MB",
            data_url.len() as u64 / (1024 * 1024),
            MAX_IMAGE_DATA_URL_BYTES / (1024 * 1024)
        ));
    }
    let bytes = data_url_bytes(&data_url);
    if bytes.is_empty() {
        return Ok(None);
    }
    let (custom, default_format, quality) = {
        let g = lock(&state);
        (
            g.settings.ui_texts.get("screenshotDir").cloned().unwrap_or_default(),
            g.settings.ui_texts.get("screenshotFormat").cloned().unwrap_or_else(|| "png".to_string()),
            g.settings.ui_values.get("snipSaveQuality").copied().unwrap_or(f64::from(SNIP_SAVE_Q)).clamp(50.0, 100.0) as u8,
        )
    };
    let format = format.unwrap_or(default_format);
    let out_bytes = tauri::async_runtime::spawn_blocking(move || reencode_screenshot_bytes(&bytes, &format, quality).map(|b| (b, format)))
        .await
        .map_err(|e| e.to_string())?;
    let (out_bytes, format) = out_bytes?;
    let path = save_screenshot(&out_bytes, &screenshot_dir(&custom), &format!("Screenshot-{}", local_stamp()), screenshot_format_ext(&format));
    Ok(if path.is_empty() { None } else { Some(path) })
}

// Save the screenshot anywhere via a native "Save as" dialog. Returns the chosen
// path, or None if the user cancelled. `format` is this one save's own choice
// (the save dialog's own format picker) -- None falls back to the setting.
#[tauri::command]
pub(crate) async fn save_screenshot_as(
    app: AppHandle,
    state: State<'_, Db>,
    win: tauri::Window,
    data_url: String,
    format: Option<String>,
    default_dir: Option<String>,
    default_name: Option<String>,
) -> Result<Option<String>, String> {
    if !data_url_within_limit(&data_url, MAX_IMAGE_DATA_URL_BYTES) {
        return Err(format!(
            "image is too large ({} MB); the limit is {} MB",
            data_url.len() as u64 / (1024 * 1024),
            MAX_IMAGE_DATA_URL_BYTES / (1024 * 1024)
        ));
    }
    let bytes = data_url_bytes(&data_url);
    if bytes.is_empty() {
        return Err("empty image".to_string());
    }
    let (default_format, quality) = {
        let g = lock(&state);
        (
            g.settings.ui_texts.get("screenshotFormat").cloned().unwrap_or_else(|| "png".to_string()),
            g.settings.ui_values.get("snipSaveQuality").copied().unwrap_or(f64::from(SNIP_SAVE_Q)).clamp(50.0, 100.0) as u8,
        )
    };
    let format = format.unwrap_or(default_format);
    let out_bytes = tauri::async_runtime::spawn_blocking(move || reencode_screenshot_bytes(&bytes, &format, quality).map(|b| (b, format)))
        .await
        .map_err(|e| e.to_string())??;
    let (out_bytes, format) = out_bytes;
    let ext = screenshot_format_ext(&format);
    // A file opened for editing from outside the app suggests its own origin
    // folder and stem (never the origin's own extension -- the edited copy keeps whatever
    // format the save dialog itself is offering) instead of the usual
    // "Screenshot-<stamp>" name built for a fresh capture.
    let name = match default_name {
        Some(n) if !n.trim().is_empty() => format!("{}.{}", n.trim(), ext),
        _ => format!("Screenshot-{}.{}", local_stamp(), ext),
    };
    let mut dlg = file_dialog_for(&app, win.label()).add_filter(ext.to_uppercase(), &[ext]).set_file_name(&name);
    if let Some(dir) = default_dir.filter(|d| !d.trim().is_empty()) {
        dlg = dlg.set_directory(dir);
    }
    let picked = dlg.save_file();
    let _ = win.set_focus();
    match picked {
        Some(p) => {
            fs::write(&p, &out_bytes).map_err(|e| e.to_string())?;
            Ok(Some(p.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

// Delete a previously auto-saved screenshot (result dialog: folder-saving turned
// off). Safety: only ever removes our own Screenshot-*.<one of the five
// screenshot formats> (or the legacy <brand>-*.png a much older version wrote,
// back when the format itself was never a choice) files.
#[tauri::command]
pub(crate) async fn delete_screenshot_file(state: State<'_, Db>, path: String) -> Result<bool, String> {
    let p = std::path::Path::new(&path);
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let legacy = !brand::LEGACY_SCREENSHOT_PREFIX.is_empty()
        && name.starts_with(brand::LEGACY_SCREENSHOT_PREFIX)
        && name.ends_with(".png");
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    let ours = (name.starts_with("Screenshot-") && matches!(ext, "jpg" | "png" | "webp" | "bmp" | "tiff")) || legacy;
    if !ours || !p.is_file() {
        return Ok(false);
    }
    // Defense in depth: only ever delete inside a folder we actually save screenshots
    // to (the configured/default screenshot dir, or temp): never an arbitrary location
    // a matching file name could otherwise point at.
    let parent = match p.parent().and_then(|d| d.canonicalize().ok()) {
        Some(d) => d,
        None => return Ok(false),
    };
    let custom = lock(&state).settings.ui_texts.get("screenshotDir").cloned().unwrap_or_default();
    let allowed = [screenshot_dir(&custom), std::env::temp_dir()];
    let in_allowed = allowed
        .iter()
        .any(|d| d.canonicalize().map(|c| c == parent).unwrap_or(false));
    Ok(in_allowed && fs::remove_file(p).is_ok())
}

// Screenshot editor "Copy" / re-apply-to-clipboard after "Apply": the
// edited image never goes through finalize_capture, so it needs its own thin
// path onto the existing copy_image_to_clipboard helper.
#[tauri::command]
pub(crate) fn copy_image_data(data_url: String) -> bool {
    if !data_url_within_limit(&data_url, MAX_IMAGE_DATA_URL_BYTES) {
        return false;
    }
    copy_image_to_clipboard(&data_url)
}

// Default screenshot folder path (shown greyed in the expert menu when no custom
// folder is set), so the user always sees where screenshots would go.
#[tauri::command]
pub(crate) fn default_screenshot_dir() -> String {
    screenshot_dir("").to_string_lossy().to_string()
}

// Same idea for the Markdown-sync folder: without this the expert row could only
// say "default", which tells the user nothing about where their files went.
#[tauri::command]
pub(crate) fn default_sync_dir(app: AppHandle) -> String {
    sync_default_dir(&app).to_string_lossy().to_string()
}

// How long a folder picked through a native dialog stays usable by the
// set_*_dir command it was picked for -- long enough to cover the confirm
// dialog some of those flows show (a data-dir move, a sync-dir switch)
// before the setter call lands, without leaving the grant valid forever: a
// subverted renderer replaying an old pick after this window closes gets the
// same stable error as one that never showed the user a picker at all.
pub(crate) const PICKER_GRANT_TTL: Duration = Duration::from_secs(10 * 60);
// Bounded so a compromised renderer spamming the picker cannot grow this
// forever; the oldest grant is dropped once the cap is hit.
pub(crate) const PICKER_GRANT_MAX: usize = 16;

// Every folder the user actually picked through pick_folder/pick_export_folder,
// each spendable exactly once by the matching set_*_dir command -- the only
// authority those commands accept besides their own built-in default, closing
// the gap where a subverted renderer could call a setter directly with any
// path it likes and have the backend write files there on its say alone.
// Each grant also carries the PURPOSE of the setting it was picked for
// (screenshot, videoExport, recording, sync, data, export): a folder picked
// for one setting no longer authorizes a different setter, so a subverted
// renderer cannot pick a folder through one dialog and spend the grant on an
// unrelated command that never showed the user that folder at all.
pub(crate) static PICKER_GRANTS: Mutex<Vec<(PathBuf, String, SystemTime)>> = Mutex::new(Vec::new());

pub(crate) fn picker_grant_key(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// A dialog result is proof the user picked this folder just now for `purpose`
// -- recorded so the matching setter call that follows can spend it.
pub(crate) fn grant_picked_dir(path: &Path, purpose: &str) {
    let key = picker_grant_key(path);
    let mut grants = PICKER_GRANTS.lock().unwrap_or_else(|e| e.into_inner());
    grants.retain(|(_, _, at)| at.elapsed().map(|e| e < PICKER_GRANT_TTL).unwrap_or(true));
    if grants.len() >= PICKER_GRANT_MAX {
        grants.remove(0);
    }
    grants.push((key, purpose.to_string(), SystemTime::now()));
}

// Spends a grant for `dir` if one exists for `purpose` and has not expired.
// Consuming it (rather than just checking) means only the legitimate setter
// call that follows the picker can ever use it -- a captured dir string
// cannot be replayed for a second write once that call has gone through, and
// a grant picked for a different purpose is never a match.
pub(crate) fn consume_granted_dir(dir: &Path, purpose: &str) -> bool {
    let key = picker_grant_key(dir);
    let mut grants = PICKER_GRANTS.lock().unwrap_or_else(|e| e.into_inner());
    grants.retain(|(_, _, at)| at.elapsed().map(|e| e < PICKER_GRANT_TTL).unwrap_or(true));
    if let Some(pos) = grants.iter().position(|(p, purp, _)| *p == key && purp == purpose) {
        grants.remove(pos);
        true
    } else {
        false
    }
}

// Shared gate for every set_*_dir command below: an empty string or the
// feature's own built-in default always resets it (nothing to authorize
// there), and any other path must be one the user just picked through
// pick_folder/pick_export_folder for THIS setting's own purpose in this same
// flow -- never a raw string the renderer made up on its own, and never a
// grant picked for a different setting.
pub(crate) fn accept_dir_setting(dir: &str, default_dir: &Path, purpose: &str) -> Result<String, String> {
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let candidate = Path::new(trimmed);
    if path_matches_configured_dir(candidate, default_dir) {
        return Ok(String::new());
    }
    if consume_granted_dir(candidate, purpose) {
        return Ok(trimmed.to_string());
    }
    Err("dir-not-picked".to_string())
}

// The native dialog's own title for each `purpose` pick_folder can be called
// for, so the user always sees what the folder they are about to choose will
// be used for -- the pick is informed consent, not a blind grant a renderer
// could point at any purpose string it likes. An unknown purpose has no
// title here on purpose: pick_folder below treats that as a reason to refuse
// the pick outright, before any dialog opens.
pub(crate) fn folder_picker_title(purpose: &str, lang: &str) -> Option<&'static str> {
    let de = lang == "de";
    match purpose {
        "screenshot" => Some(if de { "Screenshot-Ordner wählen" } else { "Choose the screenshot folder" }),
        "videoExport" => Some(if de { "Exportordner für Videos wählen" } else { "Choose the video export folder" }),
        "recording" => Some(if de { "Aufnahmeordner wählen" } else { "Choose the recording folder" }),
        "sync" => Some(if de { "Sync-Ordner wählen" } else { "Choose the sync folder" }),
        "data" => Some(if de { "Datenordner wählen" } else { "Choose the data folder" }),
        _ => None,
    }
}

// Native folder picker (used by the expert screenshot-dir + data-dir options).
// `purpose` names the setting this pick is for (screenshot, videoExport,
// recording, sync, data) so the grant it leaves behind can only be spent by
// that one setter -- see PICKER_GRANTS above. An unknown purpose gets no
// dialog and no grant at all: folder_picker_title is the only source of
// truth for which purposes exist, so a renderer cannot invent a new one and
// have the backend show a folder pick it never labeled for the user.
#[tauri::command]
pub(crate) async fn pick_folder(app: AppHandle, win: tauri::Window, state: State<'_, Db>, purpose: String) -> Result<Option<String>, String> {
    let lang = resolve_lang(&lock(&state).settings.language);
    let Some(title) = folder_picker_title(&purpose, lang) else {
        return Ok(None);
    };
    let result = file_dialog_for(&app, win.label())
        .set_title(title)
        .pick_folder()
        .map(|p| p.to_string_lossy().to_string());
    let _ = win.set_focus();
    if let Some(dir) = &result {
        grant_picked_dir(Path::new(dir), &purpose);
    }
    Ok(result)
}

// Current effective store location (shown in the expert menu).
#[tauri::command]
pub(crate) fn current_data_dir(app: AppHandle) -> String {
    data_dir(&app).to_string_lossy().to_string()
}

// The exact folder the last set_data_dir moved AWAY from: the only directory
// delete_data_dir is allowed to clean, so the webview can never point that command
// at another app's folder full of same-named files (settings.json / data.db).
pub(crate) static LAST_OLD_DATA_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

// Plain recursive copy: used to bring backups/ along on a data-dir move/adopt,
// which only ever copied the store files themselves (data.db/prompts.json/
// settings.json), leaving the Backup panel showing 0 snapshots at the new location.
pub(crate) fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

// Copy every file a store can consist of into another folder, stopping at the
// first failure. `landed` is set as soon as ANY of them arrived: a half-copied
// destination is no longer the store the caller has in memory, and a caller that
// keeps using the old one would write it straight over the files that did arrive.
pub(crate) fn copy_store_files(src: &Path, dst: &Path, landed: &mut bool) -> Result<(), String> {
    for f in STORE_FILES {
        let from = src.join(f);
        if from.exists() {
            fs::copy(&from, dst.join(f)).map_err(|e| e.to_string())?;
            *landed = true;
        }
    }
    Ok(())
}

// The disk half of a data-folder move, kept free of the AppHandle so the ORDER of
// its steps can be tested: flush the live state into the folder being left, copy
// the store across, point future launches at the new folder: and only once all of
// that has landed, mark the old folder as the one delete_data_dir may clean. Every
// step aborts the move, with that permission left unset: until the copy is
// complete, the old folder can still be the only place the current data exists, so
// offering to delete it would be offering to delete the data itself.
// `flush` persists the live store into `current`; `rebind` is called once the file
// the process will write to is a different one.
pub(crate) fn move_store_dir(
    current: &Path,
    new: &Path,
    pointer_dir: &Path,
    flush: &mut dyn FnMut() -> Result<(), String>,
    rebind: &mut dyn FnMut(),
) -> Result<(), String> {
    flush()?;
    // Adopt an existing store in the target; otherwise copy ours across. Abort
    // before writing the redirect pointer if the copy fails, so a launch can
    // never be pointed at a truncated store.
    // Every file the store can consist of, not just the database: a prompts.json or
    // settings.json left behind by a failed DB write is the ONLY copy of that change,
    // and delete_data_dir removes the old folder right after this.
    if !new.join("data.db").exists() {
        let mut landed = false;
        copy_store_files(current, new, &mut landed).map_err(|e| format!("copy store: {}", e))?;
        // Bring local backup history along -- otherwise the Backup panel shows 0
        // snapshots at the new location, and the old (still full) backups/ folder,
        // now invisible from there, is exactly what the next cleanup dialog offers
        // to delete. Best-effort: the store copy above is the critical part and has
        // already succeeded by this point.
        let old_backups = current.join("backups");
        // The new folder could itself be a descendant of old_backups (a user browsing
        // into it and picking, say, its own backups/ subfolder): copying a directory
        // into its own descendant recurses into what it just created and never stops.
        if old_backups.is_dir() && !new.starts_with(&old_backups) {
            if let Err(e) = copy_dir_all(&old_backups, &new.join("backups")) {
                append_crash_log(&format!("data dir move: could not copy backups/ ({e})"));
            }
        }
    }
    rebind();
    // Point future launches at the new folder (pointer lives in the canonical dir).
    fs::write(pointer_dir.join("datapath"), new.to_string_lossy().as_bytes())
        .map_err(|e| e.to_string())?;
    // Everything the move consists of has landed, so the folder we came from is now
    // a redundant copy: and the only one delete_data_dir is allowed to clean.
    *LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(current.to_path_buf());
    Ok(())
}

// Redirect the store to a new folder (applies on next launch). Adopts an existing
// store in the target if present, otherwise copies the current one across. The old
// data is left untouched; the caller may offer to delete it. Returns the old path.
#[tauri::command]
pub(crate) async fn set_data_dir(app: AppHandle, state: State<'_, Db>, dir: String) -> Result<String, String> {
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        return Err("empty path".to_string());
    }
    if !consume_granted_dir(Path::new(trimmed), "data") {
        return Err("dir-not-picked".to_string());
    }
    let new = PathBuf::from(trimmed);
    fs::create_dir_all(&new).map_err(|e| e.to_string())?;
    let current = data_dir(&app);
    let old = current.to_string_lossy().to_string();
    if new == current {
        return Ok(old);
    }
    // Flush the live state into the folder we are leaving, and treat a failure as a
    // failure of the whole move: the copy below would otherwise carry a stale store
    // to the new folder while the cleanup afterwards deletes the only place the
    // newer state could have been recovered from.
    let mut flush = || -> Result<(), String> {
        {
            let store = lock(&state);
            // A locked store refuses prompt writes on purpose: rows it could not
            // read are still in the file, and the copy below brings them along
            // untouched. That is not a failed flush.
            if !store.prompts_locked && !save_prompts(&app, &store) {
                return Err("the prompts could not be saved to the folder being left".to_string());
            }
            if !save_settings(&app, &store.settings) {
                return Err("the settings could not be saved to the folder being left".to_string());
            }
        }
        // Fold the WAL back into data.db so the single-file copy is complete
        // (recent copy_log appends can otherwise still live in data.db-wal).
        if let Some(c) = db_conn(&app) {
            let _ = c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }
        Ok(())
    };
    // From the next db_conn on, "the database" is a different file: either the
    // store just copied across or one already living there. Neither is described by
    // what the old folder's rows hashed to.
    let mut rebind = || lock(&state).forget_written_rows();
    move_store_dir(&current, &new, &default_data_dir(&app), &mut flush, &mut rebind)?;
    // The lock decision was made for the folder just left; re-run it for the one
    // just entered, exactly like a fresh load_store() would, so a move into (or
    // out of) a folder someone else already holds a fresh lock on takes effect
    // immediately instead of waiting for the next restart.
    let enabled = data_dir_lock_enabled(&lock(&state).settings);
    acquire_data_dir_lock(&new, enabled);
    Ok(old)
}

// Delete the store files in a folder (old location after a move). Only ever
// removes our known data files: never the folder itself or anything else.
#[tauri::command]
pub(crate) async fn delete_data_dir(path: String) -> bool {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return false;
    }
    // Only ever clean the folder set_data_dir just moved away from: never an
    // arbitrary webview-supplied directory that happens to hold same-named files.
    let allowed = LAST_OLD_DATA_DIR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|p| *p == dir);
    if !allowed {
        return false;
    }
    let mut ok = true;
    for f in ["data.db", "data.db-wal", "data.db-shm", "prompts.json", "settings.json"] {
        let p = dir.join(f);
        if p.exists() {
            ok &= fs::remove_file(&p).is_ok();
        }
    }
    ok
}
