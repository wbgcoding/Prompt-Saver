use super::*;

// Colour picker: same freeze and overlay as the snipping tool, in pick mode. The picked
// value goes to the clipboard only, with no prompt or copy history.

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

// Cells across the picker loupe: odd, so one cell is exactly the picked pixel.
pub(crate) const PICK_LOUPE_CELLS: u32 = 15;

pub(crate) fn pick_cfg(settings: &Settings) -> PickCfg {
    PickCfg {
        zoom: settings.ui_values.get("loupeZoom").copied().unwrap_or(4.0).clamp(4.0, 16.0) as u32,
        format: color_format(settings.ui_texts.get("pickerFormat").map(String::as_str).unwrap_or("hex")).to_string(),
        cells: PICK_LOUPE_CELLS,
    }
}

pub(crate) fn rgb_to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

// The last picked colours, newest first. Recorded here, not in the overlay page, because
// this side knows what reached the clipboard and a reloaded webview cannot lose it. In
// memory for the session, mirrored into ui_texts["pickHistory"] (comma-joined hex) so it
// survives a restart.
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

// RGB to HSL as rounded integers (degrees, percent, percent). Grey reports hue 0; 360 is
// written as 0.
// clippy::float_cmp: each == below compares a value from f64::max/min against one of its
// own unmodified inputs, which is exact identity.
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

// Longest tile edge the overlay may ask for: generous for the loupe, and the cap that stops
// a bogus request from allocating a desktop-sized buffer.
pub(crate) const PICK_TILE_MAX: u32 = 512;

// Hand the overlay one patch of the frozen desktop as packed RGB. The loupe caches a patch
// and asks again only near its edge; shipping the whole desktop cost ~58 MB on multi-monitor
// setups and made the picker slow to open. Empty outside a pick session or for a rect not
// fully on the frame.
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
