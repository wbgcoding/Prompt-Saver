use super::*;

// ---------- Standalone variable prompt (hotkey / tray copies) ----------
//
// A copy started from OUTSIDE the app must not drag the whole window forward just to
// ask for a placeholder. This small always-on-top window asks, and nothing else moves.
// The main window still owns every variable rule (which tokens need asking, what the
// reserved names resolve to, what happens after the copy): this window is a form:
// it pulls the pending payload, hands back the typed values, and closes.
pub(crate) const VAR_WIN_LABEL: &str = "vars-0";
pub(crate) const VAR_WIN_W: f64 = 340.0; // logical px; keep in sync with .vars-card in floating.css
pub(crate) const VAR_ROW_H: f64 = 70.0;
pub(crate) const VAR_WIN_CHROME: f64 = 104.0; // title + buttons + padding
pub(crate) const VAR_WIN_MAX_H: f64 = 720.0; // the work area caps it further on small screens
pub(crate) const VAR_ROWS_MAX: u32 = 24;
// The payload is one prompt's placeholder list, never its body.
pub(crate) const VAR_PAYLOAD_MAX: usize = 64 * 1024;

// The pending request. Some(..) = a window is waiting for an answer; taking it is
// what makes an answer (or a closed window) fire exactly once. The generation is
// what keeps a *replaced* window honest: its Destroyed event arrives after the next
// request is already installed, and without the number it would cancel that one.
pub(crate) struct VarPrompt {
    pub(crate) pending: Mutex<Option<(u64, String)>>,
    pub(crate) next_gen: AtomicU64,
}

impl VarPrompt {
    pub(crate) fn new() -> Self {
        Self { pending: Mutex::new(None), next_gen: AtomicU64::new(1) }
    }
    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, Option<(u64, String)>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// Quick-launcher window (fuzzy search, instant copy).
pub(crate) const LAUNCHER_WIN_LABEL: &str = "launcher";
// Text the tray filter row hands over: launcher_items takes it once and clears it, and a
// hotkey summon clears it first so that path never inherits a stale query.
pub(crate) static LAUNCHER_PREFILL: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
// Bumped for every launcher window that gets built, so a reveal timer of an earlier
// window never shows a newer one before its page has loaded.
pub(crate) static LAUNCHER_GEN: AtomicU64 = AtomicU64::new(0);
pub(crate) const LAUNCHER_REVEAL_FALLBACK: Duration = Duration::from_millis(1500);
pub(crate) const LAUNCHER_WIN_W: f64 = 560.0; // logical px; keep in sync with .launcher-card in floating.css
pub(crate) const LAUNCHER_WIN_H: f64 = 420.0;

// Shared by every cursor-anchored popup window (launcher, var prompt): the monitor's
// work area under the cursor plus that monitor's own scale factor, so a second
// monitor at another DPI still gets correctly sized physical pixels.
pub(crate) fn work_and_dpi_at_cursor(app: &AppHandle) -> (Rect, f64, PhysicalPosition<f64>) {
    let cursor = app.cursor_position().unwrap_or(PhysicalPosition::new(0.0, 0.0));
    let work = work_rect_at(app, cursor.x, cursor.y);
    let dpi = app
        .get_webview_window("main")
        .and_then(|w| w.monitor_from_point(cursor.x, cursor.y).ok().flatten())
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    (work, dpi, cursor)
}

// Centred horizontally on the cursor's monitor, sitting in the upper third the way
// every other quick-launcher (Spotlight, PowerToys Run, Alfred) does: a search box
// that pops up at screen centre reads as a dialog, not a launcher.
pub(crate) fn launcher_rect(app: &AppHandle) -> Rect {
    let (work, dpi, _cursor) = work_and_dpi_at_cursor(app);
    let w = LAUNCHER_WIN_W * dpi;
    let h = LAUNCHER_WIN_H * dpi;
    let x = work.x + (work.w - w) / 2.0;
    let y = work.y + (work.h - h) * 0.28;
    let (x, y) = clamp_to_work(work, w, h, x, y);
    Rect { x, y, w, h }
}

// Window box for `rows` fields, centred on the cursor and kept inside that monitor's
// work area. Physical pixels, so a second monitor at another DPI lands correctly.
pub(crate) fn var_window_rect(app: &AppHandle, rows: u32) -> Rect {
    let (work, dpi, cursor) = work_and_dpi_at_cursor(app);
    let w = VAR_WIN_W * dpi;
    let h = (VAR_WIN_CHROME + VAR_ROW_H * rows as f64).min(VAR_WIN_MAX_H) * dpi;
    let (x, y) = clamp_to_work(work, w, h, cursor.x - w / 2.0, cursor.y - h / 2.0);
    Rect { x, y, w, h }
}

// Open the small window for one fill-in request. Replaces a still-open one, so a
// second hotkey never leaves two forms fighting over the same copy.
#[tauri::command]
pub(crate) async fn open_var_window(
    app: AppHandle,
    pending: State<'_, VarPrompt>,
    payload: String,
    rows: u32,
) -> Result<(), String> {
    if payload.trim().is_empty() || payload.len() > VAR_PAYLOAD_MAX {
        return Err("bad payload".to_string());
    }
    let rows = rows.clamp(1, VAR_ROWS_MAX);
    if let Some(win) = app.get_webview_window(VAR_WIN_LABEL) {
        // Release the copy that window was holding before its slot is overwritten,
        // otherwise the first request waits for an answer that can never arrive.
        if pending.lock().take().is_some() {
            let _ = app.emit_to("main", "vars-filled", Option::<String>::None);
        }
        let _ = win.close();
    }
    // Never hold the store mutex while a window is built (deadlock shape).
    let excluded = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        capture_excluded(&store.settings)
    };
    let gen = pending.next_gen.fetch_add(1, Ordering::Relaxed);
    *pending.lock() = Some((gen, payload));
    let rect = var_window_rect(&app, rows);
    let win = WebviewWindowBuilder::new(&app, VAR_WIN_LABEL, WebviewUrl::App("vars.html".into()))
        .title(brand::DISPLAY_NAME)
        .inner_size(VAR_WIN_W, VAR_WIN_W) // placeholder: set_bounds_phys owns the real box
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(true)
        .visible(false) // sized in physical pixels first, so nothing flashes at the wrong size
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    set_capture_exclusion(&win, excluded);
    set_bounds_phys(&win, rect);
    let _ = win.show();
    let _ = win.set_focus();

    // A window closed by anything other than an answer (app quit, Esc racing the
    // command) must still release the copy waiting on it.
    win.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            if let Some(p) = app.try_state::<VarPrompt>() {
                // Only cancel the request this window was built for. A newer one has
                // a newer generation and belongs to the window that replaced this.
                let mut slot = p.lock();
                if slot.as_ref().is_some_and(|(g, _)| *g == gen) {
                    slot.take();
                    drop(slot);
                    let _ = app.emit_to("main", "vars-filled", Option::<String>::None);
                }
            }
        }
    });
    Ok(())
}

// The window asks for its own request once it has booted (a push would race the
// webview's first frame).
#[tauri::command]
pub(crate) fn var_window_payload(pending: State<VarPrompt>) -> String {
    pending.lock().as_ref().map(|(_, p)| p.clone()).unwrap_or_default()
}

// Hand the typed values back to the main window (None = cancelled) and close.
#[tauri::command]
pub(crate) fn var_window_result(app: AppHandle, pending: State<VarPrompt>, values: Option<String>) -> Result<(), String> {
    if values.as_ref().is_some_and(|v| v.len() > VAR_PAYLOAD_MAX) {
        return Err("bad values".to_string());
    }
    // Take first: the close below fires Destroyed, which must not answer twice.
    let waiting = pending.lock().take();
    if waiting.is_some() {
        let _ = app.emit_to("main", "vars-filled", values);
    }
    if let Some(win) = app.get_webview_window(VAR_WIN_LABEL) {
        let _ = win.close();
    }
    Ok(())
}

// The rows estimate in var_window_rect is a guess (a remember switch, a long name or a
// larger system font all add height). Once the form has rendered it reports the
// height it really needs, in logical px, and the window grows or shrinks to that,
// staying centred where it was and inside its monitor's work area.
#[tauri::command]
pub(crate) fn var_window_fit(app: AppHandle, height: f64) -> Result<(), String> {
    let Some(win) = app.get_webview_window(VAR_WIN_LABEL) else { return Ok(()) };
    if !height.is_finite() || height <= 0.0 {
        return Err("bad height".to_string());
    }
    let dpi = win.scale_factor().unwrap_or(1.0);
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) else { return Ok(()) };
    let (x, w, old_h) = (pos.x as f64, size.width as f64, size.height as f64);
    let mid_y = pos.y as f64 + old_h / 2.0;
    let work = work_rect_at(&app, x + w / 2.0, mid_y);
    let h = (height.min(VAR_WIN_MAX_H) * dpi).min(work.h).max(1.0);
    if (h - old_h).abs() < 1.0 {
        return Ok(());
    }
    let (x, y) = clamp_to_work(work, w, h, x, mid_y - h / 2.0);
    set_bounds_phys(&win, Rect { x, y, w, h });
    Ok(())
}

// The {?@username} placeholder. The account name never reaches a log: it is
// resolved straight into the copied text.
#[tauri::command]
pub(crate) fn os_user_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect()
}
