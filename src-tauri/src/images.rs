use super::*;

// ---------- Image helpers ----------

pub(crate) const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

// Reverse lookup built at compile time: base64_decode runs over whole images and every sealed
// settings/prompt line at startup, so the lookup has to be O(1) per byte.
pub(crate) const fn build_b64_decode_table() -> [u8; 256] {
    let mut table = [0xFFu8; 256];
    let mut i = 0;
    while i < 64 {
        table[B64_ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
}
pub(crate) const B64_DECODE_TABLE: [u8; 256] = build_b64_decode_table();

pub(crate) fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((*c.get(1).unwrap_or(&0) as u32) << 8)
            | (*c.get(2).unwrap_or(&0) as u32);
        out.push(B64_ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(B64_ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 { B64_ALPHABET[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if c.len() > 2 { B64_ALPHABET[(n & 63) as usize] as char } else { '=' });
    }
    out
}

pub(crate) fn base64_decode(s: &str) -> Vec<u8> {
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=' && !b.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for c in bytes.chunks(4) {
        // Any byte outside the alphabet means the input is corrupt: fail cleanly
        // instead of dropping it and silently shifting every following group.
        let mut v = [0u32; 4];
        for (i, &b) in c.iter().enumerate() {
            let p = B64_DECODE_TABLE[b as usize];
            if p == 0xFF { return Vec::new(); }
            v[i] = p as u32;
        }
        if c.len() >= 2 { out.push(((v[0] << 2) | (v[1] >> 4)) as u8); }
        if c.len() >= 3 { out.push(((v[1] << 4) | (v[2] >> 2)) as u8); }
        if c.len() >= 4 { out.push(((v[2] << 6) |  v[3]      ) as u8); }
    }
    out
}

// Cap decode dimensions so a malformed or decompression-bomb image fails
// gracefully (None) instead of OOM-crashing the app. Generous enough for any
// real photo or screenshot; image's default 512 MiB alloc cap still applies.
pub(crate) const MAX_DECODE_DIM: u32 = 30_000;

pub(crate) fn decode_image<R: std::io::BufRead + std::io::Seek>(reader: image::ImageReader<R>) -> Option<image::DynamicImage> {
    let mut reader = reader.with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIM);
    limits.max_image_height = Some(MAX_DECODE_DIM);
    reader.limits(limits);
    reader.decode().ok()
}

// Decode for text recognition: the picture as a viewer shows it, upright. Phone
// photos are stored sideways with an EXIF orientation flag that the decoder only
// reports, it does not apply it.
pub(crate) fn decode_image_oriented<R: std::io::BufRead + std::io::Seek>(reader: image::ImageReader<R>) -> Option<image::DynamicImage> {
    decode_image_with_orientation(reader).map(|(img, _)| img)
}

// The upright image plus the transform that was applied (NoTransforms when the file
// carries none), so a caller can tell whether the original pixels differ.
pub(crate) fn decode_image_with_orientation<R: std::io::BufRead + std::io::Seek>(
    reader: image::ImageReader<R>,
) -> Option<(image::DynamicImage, image::metadata::Orientation)> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIM);
    limits.max_image_height = Some(MAX_DECODE_DIM);
    decode_image_with_orientation_limits(reader, limits)
}

pub(crate) fn decode_image_with_orientation_limits<R: std::io::BufRead + std::io::Seek>(
    reader: image::ImageReader<R>,
    limits: image::Limits,
) -> Option<(image::DynamicImage, image::metadata::Orientation)> {
    let mut reader = reader.with_guessed_format().ok()?;
    let mut budget = limits.clone();
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    // from_decoder skips the allocation reservation that ImageReader::decode makes,
    // so charge the decoded size against the cap here.
    budget.reserve(image::ImageDecoder::total_bytes(&decoder)).ok()?;
    let orientation = image::ImageDecoder::orientation(&mut decoder).unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).ok()?;
    img.apply_orientation(orientation);
    Some((img, orientation))
}

pub(crate) fn decode_image_mem_oriented(bytes: &[u8]) -> Option<image::DynamicImage> {
    decode_image_oriented(image::ImageReader::new(std::io::Cursor::new(bytes)))
}

pub(crate) fn decode_image_mem(bytes: &[u8]) -> Option<image::DynamicImage> {
    decode_image(image::ImageReader::new(std::io::Cursor::new(bytes)))
}

pub(crate) fn decode_image_file(path: &str) -> Option<image::DynamicImage> {
    decode_image(image::ImageReader::open(path).ok()?)
}

pub(crate) fn copy_image_to_clipboard(data_url: &str) -> bool {
    let bytes = data_url_bytes(data_url);
    if bytes.is_empty() { return false; }
    let img = match decode_image_mem(&bytes) {
        Some(img) => img.to_rgba8(),
        None => return false,
    };
    let (w, h) = img.dimensions();
    let img_data = arboard::ImageData {
        width: w as usize,
        height: h as usize,
        bytes: img.into_raw().into(),
    };
    arboard::Clipboard::new().and_then(|mut c| c.set_image(img_data)).is_ok()
}

pub(crate) fn scale_and_encode(img: image::DynamicImage) -> String {
    // High quality: generous max size + Lanczos filtering keeps tiles sharp.
    const MAX: u32 = 1024;
    let img = if img.width() > MAX || img.height() > MAX {
        img.resize(MAX, MAX, FilterType::Lanczos3)
    } else {
        img
    };
    let mut buf = std::io::Cursor::new(Vec::<u8>::new());
    if img.write_to(&mut buf, image::ImageFormat::Png).is_ok() {
        format!("data:image/png;base64,{}", base64_encode(buf.get_ref()))
    } else {
        String::new()
    }
}

// Async so dialogs and clipboard reads run off the main thread; the read and image conversion
// run on a blocking worker so they never occupy the async runtime.
#[tauri::command]
pub(crate) async fn get_clipboard_image() -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut cb = arboard::Clipboard::new().ok()?;
        let data = cb.get_image().ok()?;
        let bytes: Vec<u8> = data.bytes.into_owned();
        let img = image::RgbaImage::from_raw(data.width as u32, data.height as u32, bytes)?;
        let result = scale_and_encode(image::DynamicImage::ImageRgba8(img));
        if result.is_empty() { None } else { Some(result) }
    })
    .await
    .ok()
    .flatten()
}

// ---------- File clipboard (CF_HDROP) ----------

// Put a file on the clipboard, exactly like Ctrl+C in Explorer.
#[cfg(windows)]
pub(crate) fn set_clipboard_file(path: &str) -> bool {
    if !std::path::Path::new(path).exists() {
        return false;
    }
    // DROPFILES header (20 bytes) + UTF-16 path + double NUL terminator.
    let wide: Vec<u16> = path.encode_utf16().chain([0, 0]).collect();
    let mut data = vec![0u8; 20 + wide.len() * 2];
    data[0] = 20; // pFiles: offset of the path list
    data[16] = 1; // fWide: UTF-16
    for (i, w) in wide.iter().enumerate() {
        let b = w.to_le_bytes();
        data[20 + i * 2] = b[0];
        data[21 + i * 2] = b[1];
    }
    let Ok(_clip) = clipboard_win::Clipboard::new_attempts(10) else {
        return false;
    };
    const CF_HDROP: u32 = 15;
    clipboard_win::raw::empty().is_ok() && clipboard_win::raw::set(CF_HDROP, &data).is_ok()
}

#[cfg(not(windows))]
pub(crate) fn set_clipboard_file(_path: &str) -> bool {
    false
}

// First file currently on the clipboard (copied in Explorer), if any.
#[tauri::command]
pub(crate) async fn get_clipboard_file_path() -> Option<String> {
    #[cfg(windows)]
    {
        clipboard_win::get_clipboard::<Vec<String>, _>(clipboard_win::formats::FileList)
            .ok()?
            .into_iter()
            .next()
    }
    #[cfg(not(windows))]
    None
}

#[tauri::command]
pub(crate) async fn pick_file_path(app: AppHandle, win: tauri::Window) -> Option<String> {
    let label = win.label().to_string();
    let result = file_dialog_for(&app, &label)
        .pick_file()
        .map(|p| p.to_string_lossy().to_string());
    let _ = win.set_focus();
    result
}

// Opening an existing file for editing uses one filter for both kinds, since
// which editor actually opens is decided by open_file_in_editor's own content
// sniff, not by which extension the user picked here.
#[tauri::command]
pub(crate) async fn pick_media_file_path(app: AppHandle, win: tauri::Window) -> Option<String> {
    let label = win.label().to_string();
    let result = file_dialog_for(&app, &label)
        .add_filter(
            "Images and videos",
            &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "mp4", "webm", "avi", "mov", "mkv", "apng"],
        )
        .pick_file()
        .map(|p| p.to_string_lossy().to_string());
    let _ = win.set_focus();
    result
}

// Preview for an attached file that happens to be an image.
#[tauri::command]
pub(crate) async fn load_image_file(path: String) -> Option<String> {
    let img = decode_image_file(&path)?;
    let result = scale_and_encode(img);
    if result.is_empty() { None } else { Some(result) }
}

// pdfium is only needed for PDF previews, and it is a plain DLL: so it travels
// inside the exe and is unpacked on first use. That keeps the portable build a
// single self-contained file instead of "exe plus a DLL you must not lose".
pub(crate) static PDFIUM_DLL: &[u8] = include_bytes!("../pdfium.dll");

// Where the embedded copy is unpacked. Versioned, so a newer build never binds
// against a stale DLL left behind by an older one.
pub(crate) fn pdfium_cache_path() -> PathBuf {
    std::env::temp_dir()
        .join(brand::TEMP_DIR)
        .join(format!("pdfium-{APP_VERSION}.dll"))
}

// Each app version writes its own pdfium-<version>.dll (so an update never loads a stale
// binary); this removes the previous versions' copies. Best-effort, called when the current
// cache path is about to be used, and a failure never blocks the pdfium lookup.
pub(crate) fn sweep_stale_pdfium_dlls(current: &std::path::Path) {
    let Some(dir) = current.parent() else { return };
    let Some(keep) = current.file_name() else { return };
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name != keep && name_str.starts_with("pdfium-") && name_str.ends_with(".dll") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

// A DLL next to the exe wins (lets a user drop in a newer pdfium); otherwise the
// embedded copy is written to the cache path once and reused.
pub(crate) fn pdfium_lib_path(app: &AppHandle) -> Option<PathBuf> {
    let name = "pdfium.dll";
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        candidates.push(res.join(name));
    }
    if let Some(found) = candidates.into_iter().find(|p| p.exists()) {
        return Some(found);
    }
    let cached = pdfium_cache_path();
    sweep_stale_pdfium_dlls(&cached);
    // The cache path is predictable and lives in a world-writable %TEMP%, so the file
    // found there is only trusted when its CONTENT matches the copy compiled into this
    // exe: a length check alone would let any process running as this user pre-plant a
    // same-size DLL and get it loaded as native code. Rewritten otherwise, which also
    // repairs a truncated write from an interrupted first run.
    if fs::read(&cached).is_ok_and(|got| got == PDFIUM_DLL) {
        return Some(cached);
    }
    fs::create_dir_all(cached.parent()?).ok()?;
    if let Err(e) = fs::write(&cached, PDFIUM_DLL) {
        // Release builds have no console (windows_subsystem = "windows"), so this has
        // to reach the rolling log or it is lost. The preview is skipped, app runs on.
        append_crash_log(&format!("pdfium unpack failed: {e}"));
        return None;
    }
    Some(cached)
}

// Render one page of a PDF to a preview image (data URL), or None if pdfium is unavailable or
// the file cannot be read (the caller then keeps a plain attachment). `page` is clamped to
// the real page count here: the stored number and the file can disagree (a shorter PDF
// re-attached at the same path), and clamping shows the last page instead of nothing. Loading
// and rendering run on a blocking worker; pdfium's types are not `Send`, so they are created
// and dropped inside the closure and only the `PathBuf` and `page` cross the boundary.
#[tauri::command]
pub(crate) async fn pdf_preview(app: AppHandle, path: String, page: Option<u32>) -> Option<String> {
    use pdfium_render::prelude::*;
    let lib = pdfium_lib_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let bindings = Pdfium::bind_to_library(lib).ok()?;
        let pdfium = Pdfium::new(bindings);
        let document = pdfium.load_pdf_from_file(&path, None).ok()?;
        let pages = document.pages();
        let count = pages.len();
        if count == 0 {
            return None;
        }
        let index = page.unwrap_or(0).min(count as u32 - 1);
        let page = pages.get(index as u16).ok()?;
        let config = PdfRenderConfig::new()
            .set_target_width(1000)
            .set_maximum_height(1400);
        let image = page.render_with_config(&config).ok()?.as_image();
        let result = scale_and_encode(image);
        if result.is_empty() { None } else { Some(result) }
    })
    .await
    .ok()
    .flatten()
}

// The four text attachment kinds a text preview is offered for. One list, one
// place, so the frontend extension check and the backend command never drift
// apart.
pub(crate) const TEXT_PREVIEW_EXT: [&str; 4] = ["txt", "md", "json", "csv"];

pub(crate) fn is_text_preview_ext(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| TEXT_PREVIEW_EXT.iter().any(|ext| ext.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

// Reads up to `max` bytes of a file. Unlike read_file_capped, which fails once the file
// exceeds the limit, this keeps the leading slice and reports whether the file continues, so
// a preview can show a truncation notice instead of an error.
pub(crate) fn read_bytes_preview(path: &std::path::Path, max: u64) -> Result<(Vec<u8>, bool), String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    file.take(max + 1).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let truncated = buf.len() as u64 > max;
    if truncated {
        buf.truncate(max as usize);
    }
    Ok((buf, truncated))
}

// Decodes a byte slice as UTF-8 text for the preview. A cut at an arbitrary byte offset can
// split a multi-byte character (a `Utf8Error` with no `error_len()`); that is trimmed back to
// the last full character. Any other UTF-8 error means the file is not text and is never
// guessed at.
pub(crate) fn decode_text_preview(mut buf: Vec<u8>) -> Option<String> {
    match std::str::from_utf8(&buf) {
        Ok(s) => Some(s.to_string()),
        Err(e) if e.error_len().is_none() => {
            buf.truncate(e.valid_up_to());
            String::from_utf8(buf).ok()
        }
        Err(_) => None,
    }
}

#[derive(Serialize)]
pub(crate) struct AttachTextPreview {
    pub(crate) text: String,
    pub(crate) truncated: bool,
    pub(crate) readable: bool,
}

// Plain-text preview for a .txt/.md/.json/.csv attachment, read with a size cap so a huge file
// cannot stall the UI. The caller puts it into the DOM as plain text, never as markup.
#[tauri::command]
pub(crate) async fn attachment_text_preview(path: String, max_kb: u32) -> Result<AttachTextPreview, String> {
    if !is_text_preview_ext(&path) {
        return Err("not a text attachment".into());
    }
    let max = (max_kb.clamp(1, 64) as u64) * 1024;
    tauri::async_runtime::spawn_blocking(move || -> Result<AttachTextPreview, String> {
        let p = std::path::Path::new(&path);
        let (bytes, truncated) = read_bytes_preview(p, max)?;
        match decode_text_preview(bytes) {
            Some(text) => Ok(AttachTextPreview { text, truncated, readable: true }),
            None => Ok(AttachTextPreview { text: String::new(), truncated: false, readable: false }),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

// IDs of prompts whose attached file or media icon is gone. Also invokable directly for a
// one-off recheck after an edit or when a floating pill reloads its prompt data; the ambient
// refresh is start_missing_files_poll below. Async with spawn_blocking: Path::exists() on a
// slow or unreachable network path blocks, and must not tie up an IPC dispatch worker.
#[tauri::command]
pub(crate) async fn missing_files(state: State<'_, Db>) -> Result<Vec<String>, String> {
    // Snapshot ids + paths under the lock, then stat OUTSIDE it: a slow or dead
    // drive must never block other DB operations behind this polled command.
    let entries: Vec<(String, String, String)> = {
        let g = lock(&state);
        g.prompts
            .iter()
            .map(|p| (p.id.clone(), p.file_path.clone(), p.icon_path.clone()))
            .collect()
    };
    tauri::async_runtime::spawn_blocking(move || {
        let gone = |path: &str| !path.is_empty() && !std::path::Path::new(path).exists();
        entries
            .into_iter()
            .filter(|(_, fp, ip)| gone(fp) || gone(ip))
            .map(|(id, _, _)| id)
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

// One backend timer runs the missing-files check and pushes the result to every window, so
// open pills do not each run their own full-library stat scan against a possibly slow path.
pub(crate) const MISSING_FILES_POLL_SECS: u64 = 5;
pub(crate) fn start_missing_files_poll(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            if let Some(state) = app.try_state::<Db>() {
                let ids = missing_files(state).await.unwrap_or_default();
                let _ = app.emit("missing-files", ids);
            }
            tokio::time::sleep(std::time::Duration::from_secs(MISSING_FILES_POLL_SECS)).await;
        }
    });
}
