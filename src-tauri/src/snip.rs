use super::*;

#[path = "snip_picker.rs"]
pub(crate) mod picker;
pub(crate) use picker::*;

// Snipping tool: open_snip freezes the active monitor and shows a transparent overlay for
// the user to mark a region; capture_region crops the frozen image, copies it to the
// clipboard, saves a PNG and hands the crop to the main UI.

// One frozen monitor: its capture plus its global physical origin and size. image is an Arc
// so the background JPEG encode and the stored capture share one buffer instead of two
// ~80 MB copies; readers only borrow it.
pub(crate) struct MonitorCap {
    pub(crate) image: Arc<image::RgbaImage>,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}
pub(crate) struct SnipState(pub(crate) Mutex<Vec<MonitorCap>>);

// Arms the next capture_region call to run OCR instead of a normal screenshot. Single-shot:
// close_all_snip resets it when any snip session ends, so it never leaks into a later
// screenshot.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnipMode {
    Capture,
    Ocr,
}
pub(crate) struct SnipModeState(pub(crate) Mutex<SnipMode>);

// Overlay preview prepared in the background as soon as the desktop is frozen, so the JPEG
// encode overlaps WebView creation. None = encoding, Some(None) = failed, Some(Some) =
// ready; each ready entry carries its session generation.
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

// True while a snip session is on screen. The overlay window is kept alive and reused, so
// this flag, not the window's existence, tells open_snip a snip is in progress.
pub(crate) static SNIP_ACTIVE: AtomicBool = AtomicBool::new(false);

// True once the running session's overlay was put on screen (snip_present or the
// watchdog). The reveal watchdog keys off this, not off the window's visibility: on
// the reuse path the parked overlay already counts as visible.
pub(crate) static SNIP_PRESENTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn watchdog_should_reveal(session_active: bool, presented: bool) -> bool {
    session_active && !presented
}

// Bumped by begin_capture whenever a session starts. A capture still running after its
// session was cancelled compares the number it saw at its start and must then not copy,
// save, announce or close anything.
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

// The running recording-target pick is a live target switch, not a new recording. Read once
// by recorder_target_picked so the start and swap listeners never both react.
pub(crate) static REC_SWITCH_PICK: AtomicBool = AtomicBool::new(false);

// Labels of the app windows begin_capture actually hid (those visible beforehand), so the
// session end restores exactly those: a window hidden on purpose is never popped up by a
// snip.
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

// Bring a snip's result (editor, result dialog, OCR text) to the main window. The restore
// pass does not show a main window that was hidden before the snip, so these paths show it
// themselves.
pub(crate) fn reveal_main_for_snip_result(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

// End the current snip session: hide (never close) the reusable overlay so its warm WebView2
// survives for the next snip, restore the app's windows and drop the temp preview.
pub(crate) fn close_all_snip(app: &AppHandle) {
    for (label, win) in app.webview_windows() {
        if label.starts_with("snip-") {
            park_snip_window(&win);
        }
    }
    restore_app_windows_after_snip(app);
    // Ending a session invalidates every capture still running for it. Done before the preview
    // is removed: an encode finishing between the two would pass its generation check and
    // register a file nobody removes.
    SNIP_GEN.fetch_add(1, Ordering::SeqCst);
    remove_snip_preview();
    SNIP_ACTIVE.store(false, Ordering::SeqCst);
    SNIP_PRESENTED.store(false, Ordering::SeqCst);
    PICK_ACTIVE.store(false, Ordering::SeqCst);
    // The frozen geometry belongs to the ended session.
    if let Some(state) = app.try_state::<SnipState>() {
        state.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    // Every path that ends a session funnels through here, so OCR mode cannot outlive the
    // session it was armed for.
    if let Some(m) = app.try_state::<SnipModeState>() {
        *m.0.lock().unwrap_or_else(|e| e.into_inner()) = SnipMode::Capture;
    }
}

// Hide the app's own windows (main and floating pills) around a snip so a screenshot never
// contains the app. Returns the labels that were visible and are now hidden: the restore
// set, and begin_capture skips its settle delay when it is empty.
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

// A folder window is always-on-top like a pill; an open folder would otherwise show up in
// the snip capture.
pub(crate) fn is_snip_hidden_label(label: &str) -> bool {
    label == "main" || label.starts_with("float-") || label.starts_with("folder-")
}

// Show the windows hide_app_windows put away. The main window also leaves minimized state
// and gets focus, so ending a snip never leaves it behind other windows.
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

// Leftovers from a crash mid-session sit in the OS temp dir and nothing else cleans them:
// the snip preview only deletes its own in-memory-tracked predecessor, and update.cmd leaves
// its staging folder. Swept at startup and then slowly.
// A snip preview lives minutes, so anything older than this was abandoned, while a file
// another running instance ("Allow multiple instances") uses is far younger.
pub(crate) const STALE_TEMP_FILE_AGE: Duration = Duration::from_secs(3600);
// How often the sweep repeats while the app keeps running.
pub(crate) const STALE_TEMP_SWEEP_SECS: u64 = 6 * 3600;

pub(crate) fn sweep_stale_temp_files() {
    sweep_stale_temp_files_in(&std::env::temp_dir());
}

// The sweep over an explicit temp root, so a test can point it at a folder of
// its own instead of the shared one.
pub(crate) fn sweep_stale_temp_files_in(dir: &Path) {
    // This process's own live preview is exempt whatever its age: a modal can legitimately stay
    // open past the stale age.
    let live_preview = SNIP_PREVIEW_FILE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let snip_prefix = format!("{}-snip-", brand::TEMP_PREFIX);
    let update_prefix = format!("{}-update-", brand::SLUG);
    // Each holds a staged ~8 MB MicrosoftEdgeWebview2Setup.exe that nothing else deletes.
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

    // OCR leftovers (crop/page image, install-result json) live one level down in the app's
    // TEMP_DIR subfolder. Only ocr-* names are touched here; the versioned pdfium cache lives in
    // the per-user app folder and is swept by version in sweep_stale_pdfium_dlls.
    let ocr_dir = dir.join(brand::TEMP_DIR);
    // Voice dictation leftovers (install/listing results, scripts, DISM logs) live in the same
    // folder.
    let ocr_prefix = format!("{}-ocr-", brand::TEMP_PREFIX);
    let voice_prefix = format!("{}-voice-", brand::TEMP_PREFIX);
    // Working copies of a file opened for editing from outside the app (never the original),
    // one directory per open. The editor never deletes them when it closes.
    let openedit_prefix = format!("{}-openedit-", brand::TEMP_PREFIX);
    // Still frames cut out of a video land in the same folder; a crash or kill
    // mid-extraction leaves the frame behind.
    let still_prefix = format!("{}-still-", brand::TEMP_PREFIX);
    // One pass over the folder for all four kinds; the prefixes never overlap.
    if let Ok(entries) = fs::read_dir(&ocr_dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let is_ours = name.starts_with(&ocr_prefix) || name.starts_with(&voice_prefix);
            let is_ocr_png = is_ours && name.ends_with(".png");
            // Result files of the two elevated PowerShell runs (install, list) and the DISM log. Each
            // script deletes itself, so a .ps1 here was left by a hard kill.
            let is_elevated_leftover = is_ours && (name.ends_with(".json") || name.ends_with(".ps1") || name.ends_with(".log"));
            if (is_ocr_png || is_elevated_leftover) && is_stale(&e) {
                let _ = fs::remove_file(e.path());
            } else if name.starts_with(&openedit_prefix) && is_stale(&e) {
                let _ = fs::remove_dir_all(e.path());
            } else if name.starts_with(&still_prefix) && e.path().is_file() && is_stale(&e) {
                let _ = fs::remove_file(e.path());
            }
        }
    }

    // Each (recording, thumbnail count) pair gets its own cache subdirectory that nothing
    // revisits once the recording is gone, renamed or re-encoded. The flat-file loop above
    // never matches directories.
    let thumb_dir = ocr_dir.join("video-thumbs");
    if let Ok(entries) = fs::read_dir(&thumb_dir) {
        for e in entries.flatten() {
            if is_stale(&e) {
                let _ = fs::remove_dir_all(e.path());
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

// The "Copy to clipboard" export's throwaway files, in the same TEMP_DIR subfolder.
// Separate from sweep_stale_temp_files because its max age (CLIP_EXPORT_TTL_SECS) differs
// from STALE_TEMP_FILE_AGE. Returns (count, bytes) removed, so the temporary-exports
// cleanup button can report what it did.
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

// Amount readout for the folder sweep_clip_export_temp cleans: every -clipexport- file
// regardless of age.
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

// How long a throwaway clip-export file survives before the sweep removes it.
pub(crate) const CLIP_EXPORT_TTL_SECS: u64 = 60 * 60;

// The temporary-exports row's on-demand trigger.
#[tauri::command]
pub(crate) fn sweep_clip_export_temp_now() -> (u32, u64) {
    sweep_clip_export_temp(clip_export_ttl())
}

pub(crate) fn clip_export_ttl() -> Duration {
    Duration::from_secs(CLIP_EXPORT_TTL_SECS)
}

// How often the clip-export sweep re-checks: finer than STALE_TEMP_SWEEP_SECS.
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
    // Run the overlay as a recording-target picker: same hover and drag mechanics, but a click
    // or drag resolves a recording target instead of capturing an image.
    pub(crate) record: bool,
    // Stitched-canvas origin in true virtual-desktop physical pixels. The overlay's coordinates
    // are canvas-relative; the recorder needs absolute ones to resolve a monitor.
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
    // Reject non-positive or absurdly large rects so w*h*4 stays a sane allocation. 300M px is
    // about 1.2 GB RGBA, far above any real desktop.
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
        // GDI returns BGRA; convert to RGBA with opaque alpha, in parallel (a multi-monitor capture
        // is tens of millions of pixels).
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

// A monitor's global physical origin to where its capture starts in the stitched image. The
// virtual desktop origin is negative once a monitor sits left of or above the primary, so
// this shift, not a DPI scale, addresses the right byte. Every frozen coordinate is
// physical.
pub(crate) fn stitch_offset(global: i32, origin: i32) -> i64 {
    (global - origin) as i64
}

// Where the idle, pre-warmed snip overlay sits: a few pixels far outside every monitor with
// an opaque dark backdrop, so if anything shows it outside a snip it is an invisible speck,
// not a default 800x600 white window.
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

// Hidden, tiny and far off-screen: the overlay's resting place between snips. A session
// ends with this too, so a hidden overlay never keeps the last snip's desktop size; the
// next snip sets its bounds before revealing.
pub(crate) fn park_snip_window(win: &tauri::WebviewWindow) {
    let spec = snip_prewarm_spec();
    let _ = win.hide();
    let _ = win.set_size(PhysicalSize::new(spec.width, spec.height));
    let _ = win.set_position(PhysicalPosition::new(spec.x, spec.y));
}

// Build the hidden snip overlay: borderless WS_POPUP, no shadow, always-on-top, born tiny
// and off-screen (snip_prewarm_spec). Created once and reused so the WebView2 cold start
// is paid once. A Destroyed handler clears the session and restores the app's windows if
// the overlay is ever torn down.
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

// Freeze the desktop and open one overlay spanning all screens. The frontend maps the
// cursor by ratio against the displayed frozen image, so selection is pixel-exact on any
// DPI mix and can cross monitors. The preview JPEG is encoded on a background thread right
// after the freeze, concurrently with WebView creation, and the overlay is revealed only
// once its image has decoded (snip_present), so it never flashes blank.
#[tauri::command]
pub(crate) async fn open_snip(
    app: AppHandle,
    state: State<'_, SnipState>,
    preview: State<'_, SnipPreview>,
) -> Result<(), String> {
    begin_capture(app, state, preview, false, false).await
}

// The same freeze and overlay in colour-pick mode. The frozen frame is taken before the
// overlay shows, so the overlay is never in its own capture.
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

// The same overlay as a recording-target picker: a click on a window or a dragged region
// resolves the recording target. No on/off check: the screenRecord feature toggle already
// gates the button that calls this.
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

// The same picker opened from the running recording's control bar to swap its source.
// Differs from open_recorder only in the flag recorder_target_picked reads to mark its
// result as a switch.
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
            // A session that is still preparing has nothing on screen to bring
            // forward: revealing now would show the previous session's frozen
            // image and geometry over the desktop.
            if !SNIP_PRESENTED.load(Ordering::SeqCst) {
                return Ok(());
            }
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
            let cfg = if pick { pick_cfg(settings) } else { PickCfg { zoom: 8, format: "hex".to_string(), cells: PICK_LOUPE_CELLS } };
            let hist_max = if pick { pick_history_max(settings) } else { PICK_HISTORY_DEFAULT };
            (cfg, hist_max, snip_delay, capture_hides_app_windows(capture_excluded(settings)))
        })
        .unwrap_or((PickCfg { zoom: 8, format: "hex".to_string(), cells: PICK_LOUPE_CELLS }, PICK_HISTORY_DEFAULT, 0, true));
    // Wake the overlay's WebView2 (off-screen) so a long-idle, background-throttled webview
    // un-throttles during the freeze and encode below. The reuse path re-parks it full
    // off-screen, then emits.
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
    // Arc so the preview encode (worker thread) and the stored capture share one buffer.
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

    // Reuse the overlay if it exists. A long-idle hidden WebView2 is heavily
    // background-throttled (clamped timers, paused rAF/IPC), which is why the first snip lagged
    // behind a retry. Wake it without a flash: show it 2x2 off-screen, which resumes its JS at
    // full speed; snip_present then sizes and moves it on-screen.
    if let Some(win) = app.get_webview_window("snip-0") {
        // Park it at full size just off-screen, shown so the webview un-throttles and paints.
        // snip_present then only moves it on-screen, with no resize or DPI re-assert, so the reveal
        // cannot flicker.
        let _ = win.set_size(PhysicalSize::new(total_w, total_h));
        let _ = win.set_position(PhysicalPosition::new(min_x, min_y + total_h as i32 + 100));
        let _ = win.show();
        let _ = app.emit_to("snip-0", "snip-begin", ());
        spawn_reveal_watchdog(app.clone(), gen);
        return Ok(());
    }

    // The startup pre-warm has not built the overlay yet: build it now, paying the WebView2
    // cold start once. SNIP_ACTIVE is already set, so the page's load-time snip_should_begin
    // check drives this session.
    let win = match build_snip_window(&app) {
        Ok(w) => w,
        Err(e) => {
            // Build failed: end the session the way every other path does, so the preview
            // encode, the frozen desktop, the pick flag and an armed OCR mode go with it.
            if snip_session_current(gen) {
                close_all_snip(&app);
            }
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
        // Already full size (parked off-screen on reuse), so this moves it on-screen in one shot
        // with no resize or re-assert. set_size covers the cold path.
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

// The colour picker shows the frozen desktop as the reference for the colour it copies, so
// its background must keep full chroma: below quality 90 the encoder subsamples chroma and
// the image looks washed out. 92 keeps 4:4:4.
pub(crate) const PICK_Q: u8 = 92;
// The screenshot overlay uses the same full-chroma quality, since coloured text smears under
// subsampled chroma. Fixed, not a setting.
pub(crate) const SNIP_PREVIEW_Q: u8 = PICK_Q;
// Saved-button JPEG quality: crisp text but still a fraction of a PNG's size and encode
// time. Default for the expert-tunable snipSaveQuality (read in finalize_capture), so a
// screenshot and its post-edit re-export share one visible quality.
pub(crate) const SNIP_SAVE_Q: u8 = 90;

// Encode RGBA bytes to a JPEG for the overlay preview. jpeg-encoder = SIMD baseline JPEG,
// much faster than the image crate's. Returns the raw JPEG bytes.
pub(crate) fn encode_snip_jpeg(rgba: &[u8], w: u32, h: u32, quality: u8) -> Option<Vec<u8>> {
    // JPEG sides are 16 bit: a larger capture is refused, not wrapped around.
    let (w16, h16) = (u16::try_from(w).ok()?, u16::try_from(h).ok()?);
    let mut buf = Vec::<u8>::new();
    jpeg_encoder::Encoder::new(&mut buf, quality)
        .encode(rgba, w16, h16, jpeg_encoder::ColorType::Rgba)
        .ok()?;
    Some(buf)
}

// File extension and data-URL mime for the screenshot format setting, in one place. Any
// other value (an old or hand-edited setting) falls back to png.
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
// when opening it for editing: distinct from screenshot_format_mime above,
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

// Encodes a screenshot in the format the setting names. JPEG keeps the fast SIMD path
// above; the others go through the image crate and are written lossless (its WebP encoder
// writes only lossless VP8L), so quality applies to JPEG only (see
// screenshot_format_needs_quality).
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

// Whether the quality slider means anything for this format: JPEG only (see
// encode_screenshot).
pub(crate) fn screenshot_format_needs_quality(format: &str) -> bool {
    format == "jpeg"
}

// A screenshot's data URL is PNG or whatever format was set when it was captured. "Save
// as" and a later "save to folder" can ask for a different format, so both decode back to
// pixels and re-encode instead of writing the existing bytes under a wrong extension. Only
// landing on jpeg is lossy, and only once.
pub(crate) fn reencode_screenshot_bytes(bytes: &[u8], format: &str, quality: u8) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    encode_screenshot(&img.into_rgba8().into_raw(), w, h, format, quality).ok_or_else(|| "encode failed".to_string())
}

// Hand the overlay its frozen-desktop preview. The encode started in open_snip right after
// the freeze, so the slot is usually already filled. index is kept for the existing
// single-overlay call signature.
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

// The recording overlay's window click or region drag resolved to a target: close the
// overlay (same cleanup as a screenshot) and hand the target to the main window, which runs
// the countdown and opens the control bar. x/y/width/height are stitched-canvas-relative
// for "window" (unused) and absolute virtual-desktop physical pixels for "region" (the
// caller adds SnipBg's origin).
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
    // A pick opened from the running recording's bar (open_recorder_switch) swaps that
    // recording's source instead of starting a second one (see REC_SWITCH_PICK).
    if REC_SWITCH_PICK.swap(false, Ordering::SeqCst) {
        payload["forSwitch"] = serde_json::json!(true);
    }
    close_all_snip(&app);
    state.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let _ = app.emit("recorder-target-picked", payload);
    Ok(())
}

// Which monitor a region belongs to (its HMONITOR) and the region's origin relative to that
// monitor's top-left, which RecTarget::Region needs since WGC captures a whole monitor and
// the crop happens in software. Raw extern calls like set_capture_exclusion and
// window_is_protected, instead of more windows crate features for two calls.
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

// Arms (or disarms) OCR mode for the next capture_region call, right before the frontend
// opens the overlay for the OCR menu's "Screenshot" entry. close_all_snip ensures it never
// stays armed past that session.
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
// "" on failure. Never re-encodes: the caller decides the format up front.
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

// Save a screenshot data URL into Pictures\Screenshots now (result dialog: folder-saving was
// enabled after the shot, or an edit changed the image after an auto-save). Returns the
// path. format overrides the screenshotFormat setting for this one save.
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
// (the save dialog's own format picker): None falls back to the setting.
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
    // A file opened for editing from outside the app suggests its own origin folder and stem
    // (not its extension, which follows the format the dialog offers) instead of the usual
    // "Screenshot-<stamp>" name.
    let name = match default_name {
        Some(n) if !n.trim().is_empty() => format!("{}.{}", n.trim(), ext),
        _ => format!("Screenshot-{}.{}", local_stamp(), ext),
    };
    let dialog_label = win.label().to_string();
    let dir = default_dir.filter(|d| !d.trim().is_empty());
    // The dialog is modal: it waits on a blocking thread, not on an async worker.
    let picked = tauri::async_runtime::spawn_blocking(move || {
        crate::floating::on_dialog_thread(move || {
        let mut dlg = file_dialog_for(&app, &dialog_label).add_filter(ext.to_uppercase(), &[ext]).set_file_name(&name);
        if let Some(dir) = dir {
            dlg = dlg.set_directory(dir);
        }
        dlg.save_file()
        })
        .flatten()
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = win.set_focus();
    match picked {
        Some(p) => {
            fs::write(&p, &out_bytes).map_err(|e| e.to_string())?;
            Ok(Some(p.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

// Delete a previously auto-saved screenshot (result dialog: folder-saving turned off). Only
// removes our own Screenshot-* files in one of the five screenshot formats, or the legacy
// <brand>-*.png an older version wrote.
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

// Screenshot editor "Copy" / re-apply to clipboard after "Apply": the edited image never
// goes through finalize_capture, so it has its own thin path onto copy_image_to_clipboard.
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

// How long a folder picked in a native dialog stays usable by the set_*_dir command it was
// picked for: long enough to cover a confirm dialog (data-dir move, sync-dir switch), not
// forever, so a subverted renderer replaying an old pick gets the same error as one that
// never showed a picker.
pub(crate) const PICKER_GRANT_TTL: Duration = Duration::from_secs(10 * 60);
// Bounded so a compromised renderer spamming the picker cannot grow this
// forever; the oldest grant is dropped once the cap is hit.
pub(crate) const PICKER_GRANT_MAX: usize = 16;

// Every folder the user picked through pick_folder/pick_export_folder, each spendable once
// by the matching set_*_dir command: the only authority those commands accept besides their
// built-in default, so a subverted renderer cannot call a setter with any path it likes.
// Each grant also carries the purpose of the setting it was picked for (screenshot,
// videoExport, recording, sync, data, export) and authorizes no other setter.
pub(crate) static PICKER_GRANTS: Mutex<Vec<(PathBuf, String, SystemTime)>> = Mutex::new(Vec::new());

pub(crate) fn picker_grant_key(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// A dialog result is proof the user picked this folder just now for `purpose`
//: recorded so the matching setter call that follows can spend it.
pub(crate) fn grant_picked_dir(path: &Path, purpose: &str) {
    let key = picker_grant_key(path);
    let mut grants = PICKER_GRANTS.lock().unwrap_or_else(|e| e.into_inner());
    grants.retain(|(_, _, at)| at.elapsed().map(|e| e < PICKER_GRANT_TTL).unwrap_or(true));
    if grants.len() >= PICKER_GRANT_MAX {
        grants.remove(0);
    }
    grants.push((key, purpose.to_string(), SystemTime::now()));
}

// Spends a grant for dir if one exists for purpose and has not expired. Consuming it means
// only the setter call that follows the picker can use it: a captured dir string cannot be
// replayed, and a grant for another purpose never matches.
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

// Shared gate for every set_*_dir command: an empty string or the feature's built-in default
// always resets it; any other path must be one the user just picked for this setting's
// purpose, never a string the renderer made up or a grant for another setting.
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

// The native dialog title for each purpose pick_folder accepts, in every UI language (the
// dialog can open before any page string is loaded), so the user sees what the folder will be
// used for. An unknown purpose has no title on purpose: pick_folder refuses
// it before any dialog opens.
pub(crate) fn folder_picker_title(purpose: &str, lang: &str) -> Option<&'static str> {
    // Order: screenshot, videoExport, recording, sync, data.
    let titles: [&'static str; 5] = match lang {
        "de" => ["Screenshot-Ordner wählen", "Exportordner für Videos wählen", "Aufnahmeordner wählen", "Sync-Ordner wählen", "Datenordner wählen"],
        "es" => ["Elegir la carpeta de capturas de pantalla", "Elegir la carpeta de exportación de vídeo", "Elegir la carpeta de grabaciones", "Elegir la carpeta de sincronización", "Elegir la carpeta de datos"],
        "it" => ["Scegli la cartella degli screenshot", "Scegli la cartella di esportazione dei video", "Scegli la cartella delle registrazioni", "Scegli la cartella di sincronizzazione", "Scegli la cartella dei dati"],
        "pt" => ["Escolher a pasta das capturas de ecrã", "Escolher a pasta de exportação de vídeo", "Escolher a pasta das gravações", "Escolher a pasta de sincronização", "Escolher a pasta de dados"],
        "pl" => ["Wybierz folder zrzutów ekranu", "Wybierz folder eksportu wideo", "Wybierz folder nagrań", "Wybierz folder synchronizacji", "Wybierz folder danych"],
        "zh" => ["选择截图文件夹", "选择视频导出文件夹", "选择录制文件夹", "选择同步文件夹", "选择数据文件夹"],
        "ja" => ["スクリーンショットのフォルダーを選択", "動画の書き出し先フォルダーを選択", "録画フォルダーを選択", "同期フォルダーを選択", "データフォルダーを選択"],
        "nl" => ["Kies de map voor screenshots", "Kies de exportmap voor video's", "Kies de opnamemap", "Kies de synchronisatiemap", "Kies de gegevensmap"],
        "tr" => ["Ekran görüntüsü klasörünü seçin", "Video dışa aktarma klasörünü seçin", "Kayıt klasörünü seçin", "Eşitleme klasörünü seçin", "Veri klasörünü seçin"],
        "ro" => ["Alegeți folderul pentru capturi de ecran", "Alegeți folderul de export video", "Alegeți folderul pentru înregistrări", "Alegeți folderul de sincronizare", "Alegeți folderul de date"],
        _ => ["Choose the screenshot folder", "Choose the video export folder", "Choose the recording folder", "Choose the sync folder", "Choose the data folder"],
    };
    let idx = match purpose {
        "screenshot" => 0,
        "videoExport" => 1,
        "recording" => 2,
        "sync" => 3,
        "data" => 4,
        _ => return None,
    };
    Some(titles[idx])
}

// Native folder picker (expert screenshot-dir and data-dir options). purpose names the
// setting the pick is for, so its grant can be spent only by that setter (see
// PICKER_GRANTS). An unknown purpose gets no dialog and no grant: folder_picker_title is the
// only source of truth, so a renderer cannot invent a purpose.
#[tauri::command]
pub(crate) async fn pick_folder(app: AppHandle, win: tauri::Window, state: State<'_, Db>, purpose: String) -> Result<Option<String>, String> {
    let lang = resolve_lang(&lock(&state).settings.language);
    let Some(title) = folder_picker_title(&purpose, lang) else {
        return Ok(None);
    };
    let label = win.label().to_string();
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::floating::on_dialog_thread(move || {
        file_dialog_for(&app, &label).set_title(title).pick_folder().map(|p| p.to_string_lossy().to_string())
        })
        .flatten()
    })
    .await
    .map_err(|e| e.to_string())?;
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

// Plain recursive copy, used to bring backups/ along on a data-dir move or adopt so the
// Backup panel is not empty at the new location.
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

// Folds the WAL into data.db. A busy answer (another connection still reads an old
// snapshot) means rows may remain only in the -wal file, which the single-file copy
// of the move would miss and the later cleanup would delete: retried briefly, then
// the move is aborted.
pub(crate) fn checkpoint_wal(c: &Connection, tries: u32) -> Result<(), String> {
    for attempt in 0..tries.max(1) {
        let busy: i64 = c
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if busy == 0 {
            return Ok(());
        }
        if attempt + 1 < tries {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    Err("the data folder is still in use".to_string())
}

// Whether delete_data_dir may clean `dir`: only the folder the last move came from, and
// never the folder the app is using right now, whatever the remembered one says.
pub(crate) fn data_dir_cleanup_allowed(dir: &Path, last_old: Option<&Path>, live: &Path) -> bool {
    if path_matches_configured_dir(dir, live) {
        return false;
    }
    last_old.is_some_and(|p| p == dir)
}

// Subfolders of the data folder that hold files stored rows refer to by absolute path:
// attachments (files/) and raw recordings not saved elsewhere yet (recordings/).
const MEDIA_SUBDIRS: [&str; 2] = ["files", "recordings"];

// A path split into its components for comparison: either separator, no empty or "." parts, and
// a leading verbatim marker dropped. Case is kept; callers compare it folded, like Windows does.
pub(crate) fn split_path_parts(p: &str) -> Vec<String> {
    let s = p.replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s);
    // The verbatim spelling of a share names the same host and share as the plain one, so its
    // parts start at the server too.
    let s = match s.get(..4) {
        Some(head) if head.eq_ignore_ascii_case("unc/") => &s[4..],
        _ => s,
    };
    s.split('/').filter(|c| !c.is_empty() && *c != ".").map(str::to_string).collect()
}

fn parts_start_with(path: &[String], prefix: &[String]) -> bool {
    path.len() >= prefix.len() && path.iter().zip(prefix).all(|(a, b)| a.to_lowercase() == b.to_lowercase())
}

// Whether `child` is `parent` or lies below it, compared on spelling only (case and separators
// ignored), for folders that may not exist.
pub(crate) fn path_is_inside_dir(child: &Path, parent: &Path) -> bool {
    let p = split_path_parts(&parent.to_string_lossy());
    !p.is_empty() && parts_start_with(&split_path_parts(&child.to_string_lossy()), &p)
}

// Moves a stored absolute path from `<old>/files` or `<old>/recordings` onto the same place
// below `new`. None when the path is somewhere else (a foreign file stays where it is), or when
// it climbs out with ".." (it would resolve to another file under the new root).
pub(crate) fn rebase_media_path(path: &str, old: &Path, new: &Path) -> Option<String> {
    let old_parts = split_path_parts(&old.to_string_lossy());
    if old_parts.is_empty() {
        return None;
    }
    let parts = split_path_parts(path);
    for sub in MEDIA_SUBDIRS {
        let mut prefix = old_parts.clone();
        prefix.push(sub.to_string());
        if !parts_start_with(&parts, &prefix) {
            continue;
        }
        let rest = &parts[prefix.len()..];
        if rest.iter().any(|c| c == "..") {
            return None;
        }
        let mut out = new.join(sub);
        for c in rest {
            out.push(c);
        }
        return Some(out.to_string_lossy().to_string());
    }
    None
}

// Which parts of the store a rebase changed, so only those are written again.
#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct MediaRebase {
    pub(crate) prompts: bool,
    pub(crate) trash: bool,
    pub(crate) settings: bool,
}

fn rebase_field(field: &mut String, old: &Path, new: &Path) -> bool {
    match rebase_media_path(field, old, new) {
        Some(moved) if moved != *field => {
            *field = moved;
            true
        }
        _ => false,
    }
}

fn rebase_prompt_rows(rows: &mut [Prompt], old: &Path, new: &Path) -> bool {
    let mut changed = false;
    for p in rows.iter_mut() {
        changed |= rebase_field(&mut p.file_path, old, new);
        changed |= rebase_field(&mut p.icon_path, old, new);
    }
    changed
}

// Every persisted field that can name a file below the old data folder: the attachment and
// icon paths of prompts and trash, and the settings' free-text options (a custom recording or
// screenshot folder can be one of the app's own subfolders). Pure, so a test can run it
// without an AppHandle.
pub(crate) fn rebase_store_media(
    prompts: &mut [Prompt],
    trash: &mut [Prompt],
    settings: &mut Settings,
    old: &Path,
    new: &Path,
) -> MediaRebase {
    let mut out = MediaRebase {
        prompts: rebase_prompt_rows(prompts, old, new),
        trash: rebase_prompt_rows(trash, old, new),
        settings: false,
    };
    for v in settings.ui_texts.values_mut() {
        out.settings |= rebase_field(v, old, new);
    }
    out
}

// The disk half of a data-folder move, free of the AppHandle so the order of its steps is
// testable: flush the live state into the folder being left, copy the store across, point
// future launches at the new folder, and only then mark the old folder as the one
// delete_data_dir may clean. Any failed step aborts the move with that permission unset,
// since until the copy is complete the old folder may hold the only copy. flush persists the
// live store into `current`; rebind runs once the process writes to a different file.
// Returns whether the target already held a store that was adopted as is; the in-memory
// store then describes the other library and must be reloaded. After a copy (not an adoption)
// rebase repoints the stored attachment paths at the new folder; if it fails, the old folder
// is not offered for cleanup, since files/ and recordings/ there are what those rows still name.
pub(crate) fn move_store_dir(
    current: &Path,
    new: &Path,
    pointer_dir: &Path,
    flush: &mut dyn FnMut() -> Result<(), String>,
    rebind: &mut dyn FnMut(),
    rebase: &mut dyn FnMut() -> Result<(), String>,
) -> Result<bool, String> {
    // The same folder under another spelling (casing, a trailing separator) is no
    // move: treating it as one would mark the live folder as the redundant old copy.
    if path_matches_configured_dir(new, current) {
        return Ok(false);
    }
    flush()?;
    // Adopt an existing store in the target, otherwise copy ours across, aborting before the
    // redirect pointer is written so a launch is never pointed at a truncated store. All store
    // files are copied: a prompts.json or settings.json from a failed DB write may be the only
    // copy of that change, and delete_data_dir removes the app's data from the old folder right after.
    let adopted = new.join("data.db").exists();
    if !adopted {
        // Attachments and unsaved recordings come first and are critical: stored rows name them
        // by absolute path and the cleanup deletes the old copies. Copying them before the store
        // also means a failure leaves no data.db behind that a retry would take for a library
        // to adopt.
        for sub in MEDIA_SUBDIRS {
            let from = current.join(sub);
            if !from.is_dir() {
                continue;
            }
            // Copying a directory into its own descendant would recurse forever, and the
            // cleanup would delete the new folder along with it.
            if path_is_inside_dir(new, &from) {
                return Err(format!("the new folder lies inside {sub}/, which has to be copied"));
            }
            copy_dir_all(&from, &new.join(sub)).map_err(|e| format!("copy {sub}/: {e}"))?;
        }
        let mut landed = false;
        copy_store_files(current, new, &mut landed).map_err(|e| format!("copy store: {}", e))?;
        // Bring the local backup history along, or the Backup panel shows no snapshots at the new
        // location while the old backups/ folder is offered for deletion. Best-effort; the store
        // copy above is the critical part.
        let old_backups = current.join("backups");
        // The new folder may itself sit inside old_backups; copying a directory into its own
        // descendant would recurse forever.
        if old_backups.is_dir() && !path_is_inside_dir(new, &old_backups) {
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
    if !adopted {
        if let Err(e) = rebase() {
            // The rows could not be repointed (save failed, or a locked store cannot be rewritten),
            // so they still name files/ and recordings/ in the old folder: refuse its cleanup.
            *LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = None;
            append_crash_log(&format!("data dir move: attachment paths not updated, old folder kept ({e})"));
        }
    }
    Ok(adopted)
}

// Redirect the store to a new folder (applies on next launch). Adopts an existing
// store in the target if present, otherwise copies the current one across. The old
// data is left untouched; the caller may offer to delete it. Returns the old path.
#[tauri::command]
pub(crate) async fn set_data_dir(app: AppHandle, dir: String) -> Result<String, String> {
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        return Err("empty path".to_string());
    }
    // A live capture writes into the folder being left: moving it under the encoder would lose
    // the take. Checked before the pick is consumed, so the user can retry after stopping.
    if recording_active(&app) || REC_STARTING.load(Ordering::SeqCst) {
        return Err("data-move-recording".to_string());
    }
    // One move at a time: a second pick while the first still copies would race it on the same files.
    if DATA_MOVE_BUSY.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("data-move-busy".to_string());
    }
    let guard = DataMoveGuard;
    if !consume_granted_dir(Path::new(trimmed), "data") {
        return Err("dir-not-picked".to_string());
    }
    let new = PathBuf::from(trimmed);
    // The copy of a large library, the checkpoint and the rebase block for as long as the disk
    // takes, so they run off the async runtime. The guard moves along and is released when the
    // work ends, whichever way it ends.
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        move_data_dir_blocking(&app, new)
    })
    .await
    .map_err(|e| e.to_string())?
}

// Lets exactly one data folder move run at a time; set by set_data_dir, cleared on drop.
pub(crate) static DATA_MOVE_BUSY: AtomicBool = AtomicBool::new(false);

pub(crate) struct DataMoveGuard;

impl Drop for DataMoveGuard {
    fn drop(&mut self) {
        DATA_MOVE_BUSY.store(false, Ordering::SeqCst);
    }
}

// The disk part of set_data_dir, in its fixed order: flush, copy, pointer write, rebase.
fn move_data_dir_blocking(app: &AppHandle, new: PathBuf) -> Result<String, String> {
    let state = app.state::<Db>();
    fs::create_dir_all(&new).map_err(|e| e.to_string())?;
    let current = data_dir(&app);
    let old = current.to_string_lossy().to_string();
    if path_matches_configured_dir(&new, &current) {
        return Ok(old);
    }
    // Flush the live state into the folder being left; a failure fails the whole move, since
    // the copy would otherwise carry a stale store while the cleanup deletes the only place the
    // newer state existed.
    let mut flush = || -> Result<(), String> {
        {
            let store = lock(&state);
            // A locked store refuses prompt writes on purpose; its unreadable rows are still in the
            // file and the copy brings them along. That is not a failed flush.
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
            checkpoint_wal(&c, 10)?;
        }
        Ok(())
    };
    // From the next db_conn on, "the database" is a different file (just copied or already
    // there), which the old folder's row hashes do not describe.
    let mut rebind = || lock(&state).forget_written_rows();
    // After the pointer write data_dir() answers with the new folder, so these saves land there.
    let mut rebase = || -> Result<(), String> {
        {
            let mut guard = lock(&state);
            let store = &mut *guard;
            if store.prompts_locked {
                return Err("the stored prompts are unreadable and cannot be rewritten".to_string());
            }
            let changed = rebase_store_media(&mut store.prompts, &mut store.trash, &mut store.settings, &current, &new);
            if (changed.prompts && !save_prompts(&app, store))
                || (changed.trash && !save_trash(&app, store))
                || (changed.settings && !save_settings(&app, &store.settings))
            {
                return Err("the updated paths could not be saved".to_string());
            }
        }
        // Saved editor work names its source file too. Losing it only costs the saved edits, so a
        // failure here does not keep the old folder.
        if let Some(c) = db_conn(&app) {
            if let Err(e) = rebase_edit_state(&c, &current, &new) {
                append_crash_log(&format!("data dir move: saved editor state not updated ({e})"));
            }
        }
        Ok(())
    };
    let adopted = move_store_dir(&current, &new, &default_data_dir(&app), &mut flush, &mut rebind, &mut rebase)?;
    if adopted {
        // The folder already held a library, and data_dir() points here: the in-memory store from
        // the folder just left would be written over it (or merge both libraries). Load what is
        // really in the new folder.
        let fresh = load_store(&app);
        let mut store = lock(&state);
        end_session_if_vault_changed(&store.settings, &fresh.settings);
        *store = fresh;
    }
    // Re-run the lock decision for the folder just entered, like a fresh load_store(), so a
    // move into or out of a folder someone else holds takes effect at once.
    let enabled = data_dir_lock_enabled(&lock(&state).settings);
    acquire_data_dir_lock(&new, enabled);
    Ok(old)
}

// Whether a file name is something the app itself wrote into its data folder: the store
// files, their atomic-write leftovers, the locked-database copies, the export journal, the
// folder lock, the redirect pointer and vault backups. Anything else is the user's.
pub(crate) fn is_app_data_file(name: &str, keep_pointer: bool) -> bool {
    let lower = name.to_lowercase();
    if lower == "datapath" {
        return !keep_pointer;
    }
    if lower.ends_with(".psb") {
        return true;
    }
    let known = ["data.db", "data.db-wal", "data.db-shm", "prompts.json", "settings.json", "export-staging.txt", DATA_DIR_LOCK_FILE];
    if known.iter().any(|k| lower == *k) {
        return true;
    }
    if lower == "data.db.locked.bak" || (lower.starts_with("data.db.locked.") && lower.ends_with(".bak")) {
        return true;
    }
    for k in ["data.db", "prompts.json", "settings.json"] {
        if lower == format!("{k}.tmp") || (is_atomic_temp_name(name) && lower.starts_with(&format!("{k}.tmp-"))) {
            return true;
        }
    }
    false
}

// Removes what the app created inside a data folder and nothing else: the store files and
// their leftovers, vault backups, the pointer (unless it is the live redirect), and the
// backups/, files/ and recordings/ subfolders. A file or folder the app did not create stays,
// and the folder itself goes only when it is empty afterwards. True when everything went.
pub(crate) fn remove_app_data_in(dir: &Path, keep_pointer: bool) -> bool {
    let Ok(rd) = fs::read_dir(dir) else { return false };
    let mut ok = true;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let path = e.path();
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if matches!(name.to_lowercase().as_str(), "backups" | "files" | "recordings") {
                ok &= fs::remove_dir_all(&path).is_ok();
            }
        } else if is_app_data_file(&name, keep_pointer) {
            ok &= fs::remove_file(&path).is_ok();
        }
    }
    if ok && fs::read_dir(dir).map(|mut r| r.next().is_none()).unwrap_or(false) {
        let _ = fs::remove_dir(dir);
    }
    ok
}

// Delete what the app stored in a folder (old location after a move). Only ever the folder
// the last move came from, and only entries the app created there (see remove_app_data_in).
#[tauri::command]
pub(crate) async fn delete_data_dir(app: AppHandle, path: String) -> bool {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return false;
    }
    // Only ever clean the folder set_data_dir just moved away from: never an
    // arbitrary webview-supplied directory that happens to hold same-named files.
    let allowed = {
        let last_old = LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone();
        data_dir_cleanup_allowed(&dir, last_old.as_deref(), &data_dir(&app))
    };
    if !allowed {
        return false;
    }
    // The pointer in the canonical folder is what sends the next launch to the new location.
    let keep_pointer = path_matches_configured_dir(&dir, &default_data_dir(&app));
    remove_app_data_in(&dir, keep_pointer)
}
