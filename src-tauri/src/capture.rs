use super::*;

// Local wall-clock stamp for screenshot names: "YYYY-MM-DD_HH-MM-SS" (filename-safe).
#[cfg(windows)]
pub(crate) fn local_stamp() -> String {
    #[repr(C)]
    struct SystemTimeW { year: u16, month: u16, dow: u16, day: u16, hour: u16, min: u16, sec: u16, ms: u16 }
    #[link(name = "kernel32")]
    extern "system" { fn GetLocalTime(st: *mut SystemTimeW); }
    let mut st = SystemTimeW { year: 0, month: 0, dow: 0, day: 0, hour: 0, min: 0, sec: 0, ms: 0 };
    unsafe { GetLocalTime(&mut st) };
    format!("{:04}-{:02}-{:02}_{:02}-{:02}-{:02}", st.year, st.month, st.day, st.hour, st.min, st.sec)
}
#[cfg(not(windows))]
pub(crate) fn local_stamp() -> String { now_secs().to_string() }

// App label for a window shot: drop ".exe" + any path, strip characters illegal in
// a filename, trim. Empty if nothing usable.
pub(crate) fn clean_app_name(raw: &str) -> String {
    let base = raw.rsplit(['\\', '/']).next().unwrap_or(raw);
    let stem = base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base);
    stem.chars().filter(|c| !"<>:\"/\\|?*".contains(*c)).collect::<String>().trim().to_string()
}

// Encode as JPEG, copy to the clipboard, optionally archive to the folder, close the
// overlay and notify the UI. JPEG (not PNG): a full-res PNG data URL stalled the save.
// The encode/clipboard/file-write work runs in spawn_blocking, the same shape as its
// sibling finalize_ocr_capture -- this used to run inline on the async worker thread,
// blocking whichever other command happened to land on it for however long the encode
// and disk write take.
pub(crate) fn discard_cancelled_file(path: &str) {
    if !path.is_empty() {
        let _ = fs::remove_file(path);
    }
}

pub(crate) async fn finalize_capture(app: &AppHandle, gen: u64, crop: image::RgbaImage, app_name: Option<String>) -> Result<(), String> {
    let (cw, ch) = crop.dimensions();
    if cw == 0 || ch == 0 {
        return Err("empty region".to_string());
    }
    if !snip_session_current(gen) {
        return Err("cancelled".to_string());
    }

    // "Screenshot" + a precise timestamp (+ the source app for a window shot): the
    // button name and the optional folder file share the same stamp.
    let stamp = local_stamp();
    let label = app_name.as_deref().map(clean_app_name).filter(|s| !s.is_empty());
    let (name, stem) = match &label {
        Some(a) => (format!("Screenshot {} {}", a, stamp), format!("Screenshot-{}-{}", a, stamp)),
        None => (format!("Screenshot {}", stamp), format!("Screenshot-{}", stamp)),
    };

    // Default: temp-only; the shot lives only as an in-app prompt (encrypted,
    // removed when the prompt is deleted). The folder copy is opt-in and honors
    // a custom screenshot directory (expert menu).
    let (save_to_folder, custom_dir, save_q, format) = app
        .try_state::<Db>()
        .map(|s| {
            let g = s.lock().unwrap_or_else(|e| e.into_inner());
            (
                g.settings.ui_flags.get("screenshotSave") == Some(&true),
                g.settings.ui_texts.get("screenshotDir").cloned().unwrap_or_default(),
                g.settings.ui_values.get("snipSaveQuality").copied().unwrap_or(f64::from(SNIP_SAVE_Q)).clamp(50.0, 100.0) as u8,
                g.settings.ui_texts.get("screenshotFormat").cloned().unwrap_or_else(|| "png".to_string()),
            )
        })
        .unwrap_or((false, String::new(), SNIP_SAVE_Q, "png".to_string()));

    let (data_url, path) = tauri::async_runtime::spawn_blocking(move || -> Result<(String, String), String> {
        // The data URL feeds an <img>/canvas on the JS side (the result dialog's
        // preview, then the editor if it opens); every offered format is one a
        // browser decodes, so the same bytes also go to the folder copy.
        let data_bytes = encode_screenshot(crop.as_raw(), cw, ch, &format, save_q).ok_or("encode failed")?;
        let data_url = format!("data:{};base64,{}", screenshot_format_mime(&format), base64_encode(&data_bytes));
        // The encode can take a while: a cancel that came in meanwhile wins.
        if !snip_session_current(gen) {
            return Err("cancelled".to_string());
        }
        let _ = arboard::Clipboard::new().and_then(|mut c| {
            c.set_image(arboard::ImageData {
                width: cw as usize,
                height: ch as usize,
                bytes: crop.into_raw().into(),
            })
        });
        let path = if save_to_folder {
            save_screenshot(&data_bytes, &screenshot_dir(&custom_dir), &stem, screenshot_format_ext(&format))
        } else {
            String::new()
        };
        Ok((data_url, path))
    })
    .await
    .map_err(|e| e.to_string())??;
    if !snip_session_current(gen) {
        // The cancel landed after the write: the folder file is this call's own, take it back.
        discard_cancelled_file(&path);
        return Err("cancelled".to_string());
    }

    close_all_snip(app);
    reveal_main_for_snip_result(app);
    app.state::<SnipState>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    let _ = app.emit("snip-captured", SnipResult { data_url, path, name });
    Ok(())
}

// OCR settings live in the same expert-menu ui_values/ui_flags as everything
// else. ocr_image/ocr_region/ocr_image_data receive them straight from the frontend
// (the UI calls those directly and already knows the live values via val()/flag()),
// but capture_region's OCR branch has no such call site of its own -- it reads them
// here instead, the same way finalize_capture just above reads screenshotSave and
// screenshotDir off the Db state rather than needing them passed in.
// A crop smaller than this (either side) is upscaled before recognition.
pub(crate) const OCR_UPSCALE_UNDER_PX: u32 = 300;

pub(crate) fn ocr_settings_from(app: &AppHandle) -> (u32, bool, String, bool) {
    app.try_state::<Db>()
        .map(|s| {
            let g = s.lock().unwrap_or_else(|e| e.into_inner());
            let join = g.settings.ui_flags.get("ocrJoinLines").copied().unwrap_or(true);
            let lang = g.settings.ui_texts.get("ocrLang").cloned().unwrap_or_else(|| "auto".to_string());
            (OCR_UPSCALE_UNDER_PX, join, lang, true)
        })
        .unwrap_or((OCR_UPSCALE_UNDER_PX, true, "auto".to_string(), true))
}

// The OCR counterpart to finalize_capture, taken when set_snip_mode armed the
// session. Recognition can fail (e.g. no language pack installed) -- unlike
// finalize_capture, that failure runs BEFORE close_all_snip, so the overlay stays
// open and the caller's showError() can offer a retry with the freeze still in
// place, instead of silently ending the session on a failed recognition.
pub(crate) async fn finalize_ocr_capture(app: &AppHandle, state: &State<'_, SnipState>, gen: u64, crop: image::RgbaImage) -> Result<(), String> {
    let (cw, ch) = crop.dimensions();
    if cw == 0 || ch == 0 {
        return Err("empty region".to_string());
    }
    if !snip_session_current(gen) {
        return Err("cancelled".to_string());
    }
    let (upscale, join, lang, auto_pick) = ocr_settings_from(app);
    #[cfg(windows)]
    {
        let result = tauri::async_runtime::spawn_blocking(move || ocr::recognize_crop(crop, upscale, join, &lang, auto_pick))
            .await
            .map_err(|e| e.to_string())??;
        if !snip_session_current(gen) {
            return Err("cancelled".to_string());
        }
        close_all_snip(app);
        reveal_main_for_snip_result(app);
        state.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let _ = app.emit("ocr-captured", result);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (crop, upscale, join, lang, auto_pick, gen, state, app);
        Err("ocr is only available on Windows".to_string())
    }
}

// Crop an image at a physical-pixel rect (clamped to its bounds).
pub(crate) fn crop_rgba(img: &image::RgbaImage, x: i32, y: i32, width: u32, height: u32) -> Option<image::RgbaImage> {
    let (iw, ih) = img.dimensions();
    // Both edges are clamped on their own: a rect that starts left of or above the
    // image loses that part, it does not reach further right or down instead.
    let x0 = (x as i64).max(0);
    let y0 = (y as i64).max(0);
    let x1 = (x as i64 + width as i64).min(iw as i64);
    let y1 = (y as i64 + height as i64).min(ih as i64);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(image::imageops::crop_imm(img, x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32).to_image())
}

// Crop the frozen monitor capture at a physical-pixel rect.
pub(crate) fn crop_frozen(
    state: &State<SnipState>,
    index: usize,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Option<image::RgbaImage> {
    let guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
    crop_rgba(&guard.get(index)?.image, x, y, width, height)
}

// Min/max luminance over a 4×4 sample grid (used by the capture quality checks).
pub(crate) fn luma_range(img: &image::RgbaImage) -> Option<(i32, i32)> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let (mut min_l, mut max_l) = (255i32, 0i32);
    for gy in 0..4u32 {
        for gx in 0..4u32 {
            let px = (gx * (w - 1) / 3).min(w - 1);
            let py = (gy * (h - 1) / 3).min(h - 1);
            let p = img.get_pixel(px, py);
            let l = (p[0] as i32 + p[1] as i32 + p[2] as i32) / 3;
            min_l = min_l.min(l);
            max_l = max_l.max(l);
        }
    }
    Some((min_l, max_l))
}

// True when a direct PrintWindow capture looks blocked: near black OR perfectly
// uniform. Used only to decide whether to fall back to the frozen screen, where a
// uniform-but-valid window still ends up captured.
pub(crate) fn capture_looks_bad(img: &image::RgbaImage) -> bool {
    match luma_range(img) {
        Some((min_l, max_l)) => max_l < 12 || max_l - min_l < 4,
        None => true,
    }
}

// True when the FINAL crop is unusable: empty or pure black. Protected windows
// (Task Manager, secured dialogs) that even the frozen screen could not capture
// end up black; such a defective image must never be saved (we error instead).
pub(crate) fn capture_is_blank(img: &image::RgbaImage) -> bool {
    match luma_range(img) {
        Some((_, max_l)) => max_l < 12,
        None => true,
    }
}

// Crop the frozen capture (physical pixels) and finalize -- as a saved screenshot,
// or run it through OCR instead when the mode was armed via set_snip_mode.
// One parameter per capture option the snip toolbar can set at once.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn capture_region(
    app: AppHandle,
    state: State<'_, SnipState>,
    snip_mode: State<'_, SnipModeState>,
    index: usize,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let gen = SNIP_GEN.load(Ordering::SeqCst);
    let crop = crop_frozen(&state, index, x, y, width, height).ok_or("empty region")?;
    let ocr_armed = *snip_mode.0.lock().unwrap_or_else(|e| e.into_inner()) == SnipMode::Ocr;
    if ocr_armed {
        return finalize_ocr_capture(&app, &state, gen, crop).await;
    }
    finalize_capture(&app, gen, crop, None).await
}

// Top-level windows stacked ABOVE the target that overlap its rect (global
// physical px), topmost first. These are what hide the target on the frozen
// desktop; the capture workaround minimizes them. Windows without a caption
// (overlays, shell surfaces, our own title-less snip overlay) are skipped.
#[cfg(windows)]
pub(crate) fn occluders_above(id: u32, tx: i32, ty: i32, tw: i32, th: i32) -> Vec<isize> {
    #[repr(C)]
    struct RectW { left: i32, top: i32, right: i32, bottom: i32 }
    #[link(name = "user32")]
    extern "system" {
        fn GetTopWindow(hwnd: isize) -> isize;
        fn GetWindow(hwnd: isize, cmd: u32) -> isize;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn IsIconic(hwnd: isize) -> i32;
        fn GetWindowRect(hwnd: isize, r: *mut RectW) -> i32;
        fn GetWindowTextLengthW(hwnd: isize) -> i32;
    }
    const GW_HWNDNEXT: u32 = 2;
    let (tl, tt, tr, tb) = (tx, ty, tx + tw, ty + th);
    let mut out = Vec::new();
    let mut found = false;
    unsafe {
        let mut h = GetTopWindow(0);
        while h != 0 {
            if h as u32 == id {
                found = true;
                break; // everything after the target sits behind it
            }
            if IsWindowVisible(h) != 0 && IsIconic(h) == 0 && GetWindowTextLengthW(h) > 0 {
                let mut r = RectW { left: 0, top: 0, right: 0, bottom: 0 };
                if GetWindowRect(h, &mut r) != 0
                    && r.left < tr && r.right > tl && r.top < tb && r.bottom > tt
                {
                    out.push(h);
                }
            }
            h = GetWindow(h, GW_HWNDNEXT);
        }
    }
    // Target's z-position unknown (cloaked/child): don't minimize blindly.
    if !found { out.clear(); }
    out
}

#[cfg(windows)]
pub(crate) fn show_window(hwnd: isize, cmd: i32) {
    #[link(name = "user32")]
    extern "system" {
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    }
    unsafe { ShowWindow(hwnd, cmd); }
}

// True when the window belongs to a higher-integrity / elevated process (e.g.
// Task Manager). Such windows block PrintWindow, which can return a misleading
// part-black image instead of failing cleanly: so we skip the direct path for
// them and capture via the frozen desktop / minimize workaround, which work
// regardless of elevation. Heuristic: a medium-integrity caller cannot open an
// elevated process for PROCESS_QUERY_INFORMATION (access denied).
#[cfg(windows)]
pub(crate) fn window_is_protected(id: u32) -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn CloseHandle(h: isize) -> i32;
    }
    const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(id as isize, &mut pid);
        if pid == 0 {
            return false;
        }
        let h = OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
        if h == 0 {
            true
        } else {
            CloseHandle(h);
            false
        }
    }
}

// Capture the chosen window. PrintWindow first: it grabs the window's own
// content, excluding anything stacked on top (clean for normal windows).
// Protected/elevated windows (e.g. Task Manager) block PrintWindow. If nothing
// covers the target, crop the frozen desktop (taken with our windows hidden):
// a GDI grab contains even elevated windows, with no flicker. If other windows
// DO cover it, the freeze has them baked in, so run the workaround: hide our
// overlay, minimize the covering windows, grab the now-clear area live, then
// restore them.
#[tauri::command]
pub(crate) async fn capture_window(
    app: AppHandle,
    state: State<'_, SnipState>,
    snip_mode: State<'_, SnipModeState>,
    id: u32,
) -> Result<(), String> {
    // Every one of this function's four capture paths must branch on the armed
    // OCR mode (set_snip_mode) before calling finalize_capture() unconditionally --
    // otherwise clicking a WINDOW while OCR mode is armed silently produces a
    // normal screenshot instead of running OCR.
    let gen = SNIP_GEN.load(Ordering::SeqCst);
    let ocr_armed = *snip_mode.0.lock().unwrap_or_else(|e| e.into_inner()) == SnipMode::Ocr;
    // Source app name for the button label (best-effort; empty for unknown windows).
    let app_name = xcap::Window::all()
        .ok()
        .and_then(|ws| ws.into_iter().find(|w| w.id() == id))
        .map(|w| w.app_name().to_string());
    let protected = {
        #[cfg(windows)]
        {
            window_is_protected(id)
        }
        #[cfg(not(windows))]
        {
            false
        }
    };
    // 1. Direct PrintWindow: best for normal windows (own content, no overlays).
    // Skipped for elevated windows: PrintWindow is unreliable there and can pass
    // the quality check with a wrong image, so route them to the robust paths.
    let direct = if protected {
        None
    } else {
        xcap::Window::all()
            .ok()
            .and_then(|ws| ws.into_iter().find(|w| w.id() == id))
            .and_then(|w| w.capture_image().ok())
            .filter(|im| !capture_looks_bad(im))
    };
    if let Some(im) = direct {
        return if ocr_armed { finalize_ocr_capture(&app, &state, gen, im).await } else { finalize_capture(&app, gen, im, app_name.clone()).await };
    }

    // Target's global physical rect + frozen-image origin.
    let rect = xcap::Window::all()
        .ok()
        .and_then(|ws| ws.into_iter().find(|w| w.id() == id))
        .map(|w| (w.x(), w.y(), w.width() as i32, w.height() as i32));
    let (wx, wy, ww, wh) = match rect {
        Some(r) => r,
        None => return Err("blocked".to_string()),
    };
    let origin = {
        let g = state.0.lock().unwrap_or_else(|e| e.into_inner());
        g.first().map(|c| (c.x, c.y))
    };

    #[cfg(windows)]
    {
        let occ = occluders_above(id, wx, wy, ww, wh);

        // 2. Unobstructed: the frozen desktop already shows the target cleanly.
        if occ.is_empty() {
            if let Some((ox, oy)) = origin {
                if let Some(im) = crop_frozen(&state, 0, wx - ox, wy - oy, ww as u32, wh as u32) {
                    if !capture_is_blank(&im) {
                        return if ocr_armed { finalize_ocr_capture(&app, &state, gen, im).await } else { finalize_capture(&app, gen, im, app_name.clone()).await };
                    }
                }
            }
        }

        // 3. Workaround: minimize the covering windows, grab the cleared area live.
        const SW_SHOWMINNOACTIVE: i32 = 7;
        const SW_SHOWNOACTIVATE: i32 = 4;
        if !snip_session_current(gen) {
            return Err("cancelled".to_string());
        }
        set_snip_overlay_hidden(&app, true);
        for &o in &occ {
            show_window(o, SW_SHOWMINNOACTIVE);
        }
        // Let the minimize animation finish (so the area is truly clear) and the
        // desktop recompose before grabbing.
        tokio::time::sleep(std::time::Duration::from_millis(if occ.is_empty() { 40 } else { 280 })).await;
        let grab = capture_screen_rect(wx, wy, ww, wh);
        for &o in occ.iter().rev() {
            show_window(o, SW_SHOWNOACTIVATE);
        }
        match grab {
            Some(im) if !capture_is_blank(&im) => {
                let res = if ocr_armed { finalize_ocr_capture(&app, &state, gen, im).await } else { finalize_capture(&app, gen, im, app_name.clone()).await };
                // The overlay was hidden for the grab; a failed OCR or save reports its
                // error inside that overlay, so it has to come back with the error.
                // A cancelled session must not get its overlay back (a new one may own it now).
                show_overlay_again_on_error(res, || {
                    if snip_session_current(gen) {
                        set_snip_overlay_hidden(&app, false);
                    }
                })
            }
            _ => {
                if snip_session_current(gen) {
                    set_snip_overlay_hidden(&app, false);
                }
                Err("blocked".to_string())
            }
        }
    }
    #[cfg(not(windows))]
    {
        let img = origin
            .and_then(|(ox, oy)| crop_frozen(&state, 0, wx - ox, wy - oy, ww as u32, wh as u32))
            .filter(|im| !capture_is_blank(im));
        match img {
            Some(im) => {
                if ocr_armed { finalize_ocr_capture(&app, &state, gen, im).await } else { finalize_capture(&app, gen, im, app_name.clone()).await }
            }
            None => Err("blocked".to_string()),
        }
    }
}

// Passes `res` through, calling `show` first when it is an error.
pub(crate) fn show_overlay_again_on_error<T>(res: Result<T, String>, show: impl FnOnce()) -> Result<T, String> {
    if res.is_err() {
        show();
    }
    res
}
