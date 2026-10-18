use super::*;

// ---------- Floating windows ----------

pub(crate) fn flabel(id: &str) -> String {
    format!("float-{}", id)
}

// New pills spawn at the top-left of the primary monitor, cascaded slightly
// so several pills never fully overlap.
pub(crate) fn default_pos(app: &AppHandle, index: usize) -> Pos {
    let off = 28 * (index as i32 % 8);
    let base = app
        .get_webview_window("main")
        .and_then(|w| w.primary_monitor().ok().flatten())
        .map(|m| {
            let p = m.position();
            Pos { x: p.x + 24, y: p.y + 24, ..Default::default() }
        })
        .unwrap_or(Pos { x: 24, y: 24, ..Default::default() });
    let x = base.x + off;
    let y = base.y + off;
    Pos { x, y, screen: screen_id_at(app, x, y) }
}

// One range for floating-button size, shared by the writer, the reader and the UI.
pub(crate) const FLOAT_SCALE_MIN: f64 = 0.3;
pub(crate) const FLOAT_SCALE_MAX: f64 = 8.0;

pub(crate) fn float_scale_of(settings: &Settings, id: &str) -> f64 {
    let s = settings.float_scale.get(id).copied().unwrap_or(1.0);
    // Same range set_float_scale accepts, so a stored scale is never clamped differently on load.
    if s.is_finite() { s.clamp(FLOAT_SCALE_MIN, FLOAT_SCALE_MAX) } else { 1.0 }
}

// Pill window size: square box for image pills, classic pill for text.
pub(crate) fn pill_dims(is_image: bool, scale: f64) -> (f64, f64) {
    if is_image {
        (FLOAT_IMG * scale, FLOAT_IMG * scale)
    } else {
        (FLOAT_W * scale, FLOAT_H * scale)
    }
}

// Gif/video attachments render straight from their path (no stored preview).
// Keep in sync with GIF_EXT / VIDEO_EXT in ui/media.js.
pub(crate) fn media_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    [".gif", ".mp4", ".m4v", ".mov", ".webm", ".ogv", ".ogg", ".ogm"]
        .iter()
        .any(|e| lower.ends_with(e))
}

pub(crate) fn is_image_prompt(p: &Prompt) -> bool {
    p.show_image && (!p.image.is_empty() || media_path(&p.file_path) || media_path(&p.icon_path))
}

// Every file dialog is parented to the window that opened it: only that window is blocked,
// so a second dialog never stacks on top and a satellite window (e.g. the video editor)
// keeps focus.
pub(crate) fn file_dialog_for(app: &AppHandle, label: &str) -> rfd::FileDialog {
    let dlg = rfd::FileDialog::new();
    let chosen = resolve_dialog_parent_label(|l| app.get_webview_window(l).is_some(), label);
    match chosen.and_then(|l| app.get_webview_window(l)) {
        Some(win) => dlg.set_parent(&win),
        None => dlg,
    }
}

// Pure label choice: the requested window if open, else "main", else nobody. Kept apart
// from the AppHandle lookup so it is unit-testable.
pub(crate) fn resolve_dialog_parent_label(exists: impl Fn(&str) -> bool, label: &str) -> Option<&str> {
    if exists(label) {
        Some(label)
    } else if exists("main") {
        Some("main")
    } else {
        None
    }
}

pub(crate) fn file_dialog(app: &AppHandle) -> rfd::FileDialog {
    file_dialog_for(app, "main")
}

// Pill and folder windows are built hidden and shown here once the page has loaded: a
// window shown at creation sits on screen as an empty rectangle while WebView2 starts.
fn reveal_when_loaded(win: tauri::WebviewWindow, payload: tauri::webview::PageLoadPayload<'_>) {
    if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
        reveal_float_window(&win);
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Reveal {
    Now,
    AfterSnip,
    Skip,
}

// What the reveal of a freshly built pill or folder does: nothing when it is already
// up, wait for the end of a running snip session (it would sit in the capture or on
// the overlay), else show it now.
pub(crate) fn reveal_decision(visible: bool, snip_active: bool) -> Reveal {
    if visible {
        Reveal::Skip
    } else if snip_active {
        Reveal::AfterSnip
    } else {
        Reveal::Now
    }
}

// Put a label into the set the end of a snip session brings back (once).
pub(crate) fn queue_for_snip_restore(hid: &mut Vec<String>, label: &str) {
    if !hid.iter().any(|l| l == label) {
        hid.push(label.to_string());
    }
}

// Show without taking the foreground from the app the user is typing in; the window
// was built always-on-top and stays so.
pub(crate) fn show_without_focus(win: &tauri::WebviewWindow) {
    #[cfg(windows)]
    {
        const SW_SHOWNOACTIVATE: i32 = 4;
        if let Ok(h) = win.hwnd() {
            show_window(h.0 as isize, SW_SHOWNOACTIVATE);
            let _ = win.set_always_on_top(true);
            return;
        }
    }
    let _ = win.show();
}

fn reveal_float_window(win: &tauri::WebviewWindow) {
    match reveal_decision(win.is_visible().unwrap_or(false), SNIP_ACTIVE.load(Ordering::SeqCst)) {
        Reveal::Skip => {}
        Reveal::Now => show_without_focus(win),
        Reveal::AfterSnip => {
            queue_for_snip_restore(&mut SNIP_HID_APP.lock().unwrap_or_else(|e| e.into_inner()), win.label());
            // The session may have ended between the check and the queueing, with the
            // restore pass already gone: nobody would bring the window back then.
            if !SNIP_ACTIVE.load(Ordering::SeqCst) {
                SNIP_HID_APP.lock().unwrap_or_else(|e| e.into_inner()).retain(|l| l != win.label());
                show_without_focus(win);
            }
        }
    }
}

// The page-load event is the normal reveal; if it never arrives (a stalled
// WebView2 start) the window still comes up after a few seconds.
const FLOAT_REVEAL_FALLBACK_MS: u64 = 3000;
fn spawn_reveal_fallback(app: &AppHandle, label: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(FLOAT_REVEAL_FALLBACK_MS)).await;
        if let Some(win) = app.get_webview_window(&label) {
            reveal_float_window(&win);
        }
    });
}

// The main screen's scale factor, the one factor every park spot calculation uses to
// turn logical box sizes into the physical pixels the work areas are measured in.
fn main_scale_factor(app: &AppHandle) -> f64 {
    let win = app.get_webview_window("main");
    // The primary monitor, the one the park spots are measured on (screen_snapshot puts it first).
    win.as_ref()
        .and_then(|w| w.primary_monitor().ok().flatten())
        .or_else(|| win.as_ref().and_then(|w| w.available_monitors().ok()).and_then(|m| m.into_iter().next()))
        .map(|m| m.scale_factor())
        .unwrap_or(1.0)
}

// Labels whose window was asked to close. close() only queues the teardown, and building
// under the same label before it ran fails, so a pill wanted again meanwhile must wait for
// the label to go. The state tells whether the entry has a waiting task, and whether that
// task gave up while the window still existed (the Destroyed event then replays the plan).
pub(crate) static CLOSING_FLOATS: Mutex<Vec<(String, CloseWatch)>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CloseWatch {
    Idle,
    Waiting,
    GaveUp,
}

// Pure list steps, so the hand-over between the waiter and the Destroyed event is testable.
// True when the caller is the one to start the waiting task.
pub(crate) fn watch_claim(list: &mut [(String, CloseWatch)], label: &str) -> bool {
    match list.iter_mut().find(|(l, _)| l == label) {
        Some(e) if e.1 == CloseWatch::Idle => {
            e.1 = CloseWatch::Waiting;
            true
        }
        _ => false,
    }
}

// The window is gone: drop the entry. True when a waiter already gave up on it, so nobody
// else is going to replay the plan.
pub(crate) fn watch_destroyed(list: &mut Vec<(String, CloseWatch)>, label: &str) -> bool {
    let gave_up = list.iter().any(|(l, s)| l == label && *s == CloseWatch::GaveUp);
    list.retain(|(l, _)| l != label);
    gave_up
}

// The waiter's wait ran out with the window still there. False when the entry is already
// gone (the window was destroyed in between), so the waiter must replay the plan itself.
pub(crate) fn watch_give_up(list: &mut [(String, CloseWatch)], label: &str) -> bool {
    match list.iter_mut().find(|(l, _)| l == label) {
        Some(e) => {
            e.1 = CloseWatch::GaveUp;
            true
        }
        None => false,
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum ReopenStep {
    Build,
    Keep,
    WaitThenSync,
}

pub(crate) fn reopen_step(exists: bool, closing: bool) -> ReopenStep {
    match (exists, closing) {
        (false, _) => ReopenStep::Build,
        (true, false) => ReopenStep::Keep,
        (true, true) => ReopenStep::WaitThenSync,
    }
}

pub(crate) fn close_tracked(label: &str, win: &tauri::WebviewWindow) {
    {
        let mut c = CLOSING_FLOATS.lock().unwrap_or_else(|e| e.into_inner());
        if !c.iter().any(|(l, _)| l == label) {
            c.push((label.to_string(), CloseWatch::Idle));
        }
    }
    let _ = win.close();
}

pub(crate) fn is_closing(label: &str) -> bool {
    CLOSING_FLOATS.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|(l, _)| l == label)
}

fn forget_closing(label: &str) {
    CLOSING_FLOATS.lock().unwrap_or_else(|e| e.into_inner()).retain(|(l, _)| l != label);
}

// The window is gone: a label that is never opened again (deleted prompt, dissolved folder)
// must not stay in the list for the rest of the process. A waiter that gave up while the
// teardown was still running left the reopen undone, so the plan is replayed once here, off
// the event loop that runs this handler.
pub(crate) fn float_destroyed(app: &AppHandle, label: &str) {
    note_app_placement(label, None);
    let gave_up = watch_destroyed(&mut CLOSING_FLOATS.lock().unwrap_or_else(|e| e.into_inner()), label);
    if gave_up {
        let app2 = app.clone();
        let label = label.to_string();
        tauri::async_runtime::spawn(async move {
            wait_label_gone(&app2, &label).await;
            sync_float_windows(&app2);
        });
    }
}

// True when the caller must not build now. A window that is still closing gets one task that
// waits off this thread (it may be the event loop that runs the teardown) and then replays
// the plan; a live window is simply kept.
fn defer_while_closing(app: &AppHandle, label: &str) -> bool {
    let exists = app.get_webview_window(label).is_some();
    let step = reopen_step(exists, is_closing(label));
    if step == ReopenStep::Build {
        forget_closing(label);
        return false;
    }
    if step == ReopenStep::WaitThenSync {
        let first = watch_claim(&mut CLOSING_FLOATS.lock().unwrap_or_else(|e| e.into_inner()), label);
        if first {
            let app2 = app.clone();
            let label = label.to_string();
            tauri::async_runtime::spawn(async move {
                wait_label_gone(&app2, &label).await;
                if app2.get_webview_window(&label).is_none() {
                    forget_closing(&label);
                } else if watch_give_up(&mut CLOSING_FLOATS.lock().unwrap_or_else(|e| e.into_inner()), &label) {
                    // Still tearing down: the Destroyed event replays the plan.
                    return;
                }
                sync_float_windows(&app2);
            });
        }
    }
    true
}

// Bounded wait until a closing window's label is free again.
pub(crate) async fn wait_label_gone(app: &AppHandle, label: &str) {
    for _ in 0..50 {
        if app.get_webview_window(label).is_none() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

// `taken` collects the box this pill actually lands in (real or parked) so
// the next call in the same sync_float_windows pass fans its own park spot
// out from this one instead of landing on top of it.
pub(crate) fn open_floating(app: &AppHandle, prompt: &Prompt, taken: &mut Vec<Rect>) {
    let label = flabel(&prompt.id);
    if defer_while_closing(app, &label) {
        return;
    }

    // Never call Tauri window APIs while holding the Db lock (deadlock risk).
    let (saved, scale, count, excluded) = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        (
            store.settings.floating.get(&prompt.id).cloned(),
            float_scale_of(&store.settings, &prompt.id),
            store.settings.floating.len(),
            capture_excluded(&store.settings),
        )
    };
    let is_new = saved.is_none();
    let pos = saved.unwrap_or_else(|| default_pos(app, count));
    if is_new {
        let state: State<Db> = app.state();
        let mut store = lock(&state);
        store.settings.floating.insert(prompt.id.clone(), pos.clone());
        save_settings(app, &store.settings);
    }

    let (pw, ph) = pill_dims(is_image_prompt(prompt), scale);
    // Spots and `taken` are physical pixels, the window sizes logical ones.
    let dpi = main_scale_factor(app);
    let (x, y, parked) = resolve_float_spot(&pos, pw * dpi, ph * dpi, &screen_snapshot(app), taken);
    taken.push(Rect { x, y, w: pw * dpi, h: ph * dpi });
    mark_parked(app, &prompt.id, parked);

    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("floating.html".into()))
        .title(&prompt.name)
        .inner_size(pw, ph)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .on_page_load(reveal_when_loaded)
        .build();

    if let Ok(win) = win {
        // Transparent native backdrop so resizing never flashes an opaque rectangle behind the
        // rounded pill.
        let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
        set_capture_exclusion(&win, excluded);
        let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        note_app_placement(&label, parked.then(|| (x.round() as i32, y.round() as i32)));
        spawn_reveal_fallback(app, label.clone());

        let app2 = app.clone();
        let pid = prompt.id.clone();
        let wlabel = label.clone();
        win.on_window_event(move |event| match event {
            WindowEvent::Moved(p) => {
                // Fires for every pixel of a drag, so never block on a busy store; the next event carries
                // the position. A drag is the user placing the pill, so it becomes the real position and
                // ends parking. The app's own placement of a parked pill is not a drag.
                if !move_is_user_drag(&wlabel, (p.x, p.y)) {
                    return;
                }
                let screen = screen_id_at(&app2, p.x, p.y);
                if let Some(state) = app2.try_state::<Db>() {
                    if let Ok(mut store) = state.try_lock() {
                        if store.settings.floating.contains_key(&pid) {
                            store.settings.floating.insert(pid.clone(), Pos { x: p.x, y: p.y, screen });
                        }
                    }
                }
                mark_parked(&app2, &pid, false);
            }
            WindowEvent::Destroyed => {
                float_destroyed(&app2, &wlabel);
                save_settings_off_event_loop(&app2);
            }
            _ => {}
        });
    }
}

// Saves the settings from a window-destroyed event. The event loop must not wait on
// the store: when a backup or import holds it, a worker thread takes the turn instead
// and every window that closes meanwhile stays responsive.
pub(crate) fn save_settings_off_event_loop(app: &AppHandle) {
    let Some(state) = app.try_state::<Db>() else { return };
    match state.try_lock() {
        Ok(store) => {
            save_settings(app, &store.settings);
        }
        Err(std::sync::TryLockError::Poisoned(e)) => {
            save_settings(app, &e.into_inner().settings);
        }
        Err(std::sync::TryLockError::WouldBlock) => {
            let app2 = app.clone();
            std::thread::spawn(move || {
                if let Some(state) = app2.try_state::<Db>() {
                    let store = state.lock().unwrap_or_else(|e| e.into_inner());
                    save_settings(&app2, &store.settings);
                }
            });
        }
    };
}

pub(crate) fn close_floating_window(app: &AppHandle, id: &str) {
    let label = flabel(id);
    if let Some(win) = app.get_webview_window(&label) {
        close_tracked(&label, &win);
    }
}

// ---------- floating pill folders ----------

pub(crate) fn folder_label(id: &str) -> String {
    format!("folder-{}", id)
}

// Expert settings the backend needs. The feature ships ON; the two parameters use
// the same defaults and ranges as their sliders in the expert menu.
pub(crate) fn folders_on(settings: &Settings) -> bool {
    settings.ui_flags.get("folders") != Some(&false)
}

// Pill and folder windows read the settings once, when they open; this tells the open ones
// to re-read after an expert change.
pub(crate) fn notify_float_settings(app: &AppHandle) {
    for label in app.webview_windows().keys() {
        if label.starts_with("float-") || label.starts_with("folder-") {
            let _ = app.emit_to(label.as_str(), "settings-changed", ());
        }
    }
}
// How close two floating buttons have to be when released before they merge.
pub(crate) const MERGE_RADIUS: f64 = 40.0;
// Columns of an open button folder's grid.
pub(crate) const FOLDER_COLS: u32 = 3;

// A box in PHYSICAL desktop pixels: the one coordinate space every monitor shares,
// whatever its DPI. Sizes are converted per window with its own scale factor, so a
// mixed-DPI setup stays correct.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

impl Rect {
    pub(crate) fn right(&self) -> f64 {
        self.x + self.w
    }
    pub(crate) fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub(crate) fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub(crate) fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
}

// Which pill the just-dropped one landed on: the nearest centre within `radius` whose box
// also overlaps. The radius alone would match a pill that merely sits nearby, the overlap
// alone a long text pill whose far end touches; a short nudge matches nothing, so it
// creates no folder. A distance exactly equal to the radius counts as a hit.
pub(crate) fn merge_target(dragged: Rect, others: &[(String, Rect)], radius: f64) -> Option<&str> {
    let (cx, cy) = dragged.center();
    let mut best: Option<(f64, &str)> = None;
    for (id, rect) in others {
        if !dragged.overlaps(rect) {
            continue;
        }
        let (ox, oy) = rect.center();
        let dist = ((cx - ox).powi(2) + (cy - oy).powi(2)).sqrt();
        if dist > radius {
            continue;
        }
        if best.map_or(true, |(b, _)| dist < b) {
            best = Some((dist, id.as_str()));
        }
    }
    best.map(|(_, id)| id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FolderSide {
    Below,
    Above,
    Right,
    Left,
}

impl FolderSide {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            FolderSide::Below => "below",
            FolderSide::Above => "above",
            FolderSide::Right => "right",
            FolderSide::Left => "left",
        }
    }
}

// Where an expanded folder ends up: the window that has to cover both the icon and
// the popover, plus where each of them sits inside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FolderLayout {
    pub(crate) side: FolderSide,
    pub(crate) win: Rect,
    pub(crate) icon: (f64, f64), // icon offset inside the window
    pub(crate) pop: (f64, f64),  // popover offset inside the window
}

// Grow the folder window around its icon. The icon never moves (it is what the user
// clicked): the popover goes on the first side with room (below, above, right, left) and
// slides along its free axis until it lies inside the work area. All values are physical
// pixels of one monitor: pass that monitor's work rect and sizes scaled by its factor.
pub(crate) fn folder_layout(icon: Rect, pop_w: f64, pop_h: f64, gap: f64, work: Rect) -> FolderLayout {
    let side = if icon.bottom() + gap + pop_h <= work.bottom() {
        FolderSide::Below
    } else if icon.y - gap - pop_h >= work.y {
        FolderSide::Above
    } else if icon.right() + gap + pop_w <= work.right() {
        FolderSide::Right
    } else if icon.x - gap - pop_w >= work.x {
        FolderSide::Left
    } else if icon.y - work.y > work.bottom() - icon.bottom() {
        // Nothing fits (a grid taller than the screen): keep the roomier half, so the
        // first rows stay reachable instead of the popover hanging off the edge.
        FolderSide::Above
    } else {
        FolderSide::Below
    };
    // A popover larger than the work area can only start at its edge.
    let fit = |v: f64, lo: f64, hi: f64| if hi < lo { lo } else { v.max(lo).min(hi) };
    // The popover is aligned with the icon's leading edge, never centred: a centred one is
    // wider than the icon on both sides, so the window origin would move left and the icon
    // jump right one frame later. Aligned, the origin stays put whenever the popover fits
    // below/right.
    let pop = match side {
        FolderSide::Below | FolderSide::Above => {
            let x = fit(icon.x, work.x, work.right() - pop_w);
            let y = if side == FolderSide::Below {
                icon.bottom() + gap
            } else {
                icon.y - gap - pop_h
            };
            Rect { x, y: fit(y, work.y, work.bottom() - pop_h), w: pop_w, h: pop_h }
        }
        FolderSide::Right | FolderSide::Left => {
            let y = fit(icon.y, work.y, work.bottom() - pop_h);
            let x = if side == FolderSide::Right {
                icon.right() + gap
            } else {
                icon.x - gap - pop_w
            };
            Rect { x: fit(x, work.x, work.right() - pop_w), y, w: pop_w, h: pop_h }
        }
    };
    let x = icon.x.min(pop.x);
    let y = icon.y.min(pop.y);
    let win = Rect {
        x,
        y,
        w: icon.right().max(pop.right()) - x,
        h: icon.bottom().max(pop.bottom()) - y,
    };
    FolderLayout {
        side,
        win,
        icon: (icon.x - x, icon.y - y),
        pop: (pop.x - x, pop.y - y),
    }
}

// Keep a dropped pill fully inside the work area it was dropped on. A window larger
// than the screen is pinned to the top-left corner instead of being pushed off it.
pub(crate) fn clamp_to_work(work: Rect, w: f64, h: f64, x: f64, y: f64) -> (f64, f64) {
    let fit = |v: f64, lo: f64, hi: f64| if hi < lo { lo } else { v.max(lo).min(hi) };
    (
        fit(x, work.x, work.right() - w),
        fit(y, work.y, work.bottom() - h),
    )
}

// Fan a set of pills apart so none ends up hidden under another. Each pill's old position
// is a wish; pills dropped onto each other formed the folder, so a dissolved folder's
// wishes coincide. A taken box pushes the next one outward on a growing ring (step a bit
// over half a pill) until it lands free, then it is clamped to the work area. Pure and
// deterministic.
pub(crate) fn spread_free_positions(wishes: &[Rect], taken: &[Rect], work: Rect) -> Vec<(f64, f64)> {
    const RINGS: i32 = 10;
    let mut placed: Vec<Rect> = taken.to_vec();
    let mut out = Vec::with_capacity(wishes.len());
    for wish in wishes {
        let step = (wish.w.max(wish.h) * 0.55).max(24.0);
        let at = |dx: i32, dy: i32| {
            let (x, y) = clamp_to_work(
                work,
                wish.w,
                wish.h,
                wish.x + dx as f64 * step,
                wish.y + dy as f64 * step,
            );
            Rect { x, y, w: wish.w, h: wish.h }
        };
        let mut spot = at(0, 0);
        'rings: for ring in 0..=RINGS {
            for dy in -ring..=ring {
                for dx in -ring..=ring {
                    // Only the outer edge of each ring is new.
                    if ring > 0 && dx.abs() != ring && dy.abs() != ring {
                        continue;
                    }
                    let cand = at(dx, dy);
                    if !placed.iter().any(|p| p.overlaps(&cand)) {
                        spot = cand;
                        break 'rings;
                    }
                }
            }
        }
        placed.push(spot);
        out.push((spot.x, spot.y));
    }
    out
}

// A screen's identity: name, resolution and virtual-desktop offset, hashed to a short
// stable string. The name is part of the hash so a different physical monitor landing at
// an old one's position never compares equal.
pub(crate) fn screen_identity(name: &str, x: i32, y: i32, w: i32, h: i32) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    (x, y, w, h).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

// One monitor as the pure functions below need it: identity plus bounds. tauri::Monitor
// cannot be built outside a real window, so live callers reduce their monitor list to this
// shape, which also lets resolve_float_spot be tested with a mocked list.
pub(crate) type ScreenRect = (String, i32, i32, i32, i32); // (identity, x, y, width, height)

fn monitor_rect(m: &tauri::Monitor) -> ScreenRect {
    let p = m.position();
    let s = m.size();
    let name = m.name().cloned().unwrap_or_default();
    (screen_identity(&name, p.x, p.y, s.width as i32, s.height as i32), p.x, p.y, s.width as i32, s.height as i32)
}

// The monitor list with the primary one first. The OS enumeration order carries no such
// promise, yet the park and recall code treats the first entry as the main screen. An
// unknown primary leaves the order as it was.
pub(crate) fn primary_first(mut screens: Vec<ScreenRect>, primary_id: &str) -> Vec<ScreenRect> {
    if let Some(i) = screens.iter().position(|(id, ..)| id == primary_id) {
        let primary = screens.remove(i);
        screens.insert(0, primary);
    }
    screens
}

pub(crate) fn screen_snapshot(app: &AppHandle) -> Vec<ScreenRect> {
    let Some(win) = app.get_webview_window("main") else {
        return Vec::new();
    };
    let list: Vec<ScreenRect> = win.available_monitors().unwrap_or_default().iter().map(monitor_rect).collect();
    let primary = win.primary_monitor().ok().flatten().map(|m| monitor_rect(&m).0).unwrap_or_default();
    primary_first(list, &primary)
}

// The identity of the screen containing (x, y), or empty if none does (a probe failure
// must never panic).
pub(crate) fn screen_id_at(app: &AppHandle, x: i32, y: i32) -> String {
    screen_snapshot(app)
        .into_iter()
        .find(|(_, sx, sy, sw, sh)| x >= *sx && x < sx + sw && y >= *sy && y < sy + sh)
        .map(|(id, ..)| id)
        .unwrap_or_default()
}

// Where a saved floating position ends up today: unchanged if its screen is still there,
// else parked on the main screen (the first entry of `screens`, which screen_snapshot makes
// the primary monitor). Never mutates `pos`: the caller keeps the stored position and screen id and
// only places the window at the returned point, so a screen that comes back finds its pill
// where it was. `taken` is every other window's box, so several pills parked in one pass
// fan out instead of stacking.
//
// A `pos.screen` of "" (saved before the field existed) falls back to plain coordinate
// containment, so an upgrade never parks an unmoved button. Once a screen id is recorded,
// only that exact screen counts as home; a different monitor covering the same point parks.
pub(crate) fn resolve_float_spot(pos: &Pos, w: f64, h: f64, screens: &[ScreenRect], taken: &[Rect]) -> (f64, f64, bool) {
    if screens.is_empty() {
        // A failed monitor probe must never look like "every screen is gone".
        return (pos.x as f64, pos.y as f64, false);
    }
    let still_home = screens.iter().any(|(id, sx, sy, sw, sh)| {
        let on_this_one = pos.x >= *sx && pos.x < sx + sw && pos.y >= *sy && pos.y < sy + sh;
        on_this_one && (pos.screen.is_empty() || *id == pos.screen)
    });
    if still_home {
        return (pos.x as f64, pos.y as f64, false);
    }
    let (mx, my, mw, mh) = (screens[0].1, screens[0].2, screens[0].3, screens[0].4);
    let work = Rect { x: mx as f64, y: my as f64, w: mw as f64, h: mh as f64 };
    let wish = Rect { x: work.x, y: work.y, w, h };
    let (x, y) = spread_free_positions(std::slice::from_ref(&wish), taken, work)
        .into_iter()
        .next()
        .unwrap_or((work.x, work.y));
    (x, y, true)
}

// Which parked ids have been parked longer than `cleanup_h` hours. Pure so the rule is
// testable without a clock; `cleanup_h <= 0` is the off position (floatParkCleanupH's
// "0 = off").
pub(crate) fn ids_past_park_cleanup(parked: &HashMap<String, std::time::Duration>, cleanup_h: f64) -> Vec<String> {
    if cleanup_h <= 0.0 {
        return Vec::new();
    }
    let limit = std::time::Duration::from_secs_f64(cleanup_h * 3600.0);
    parked.iter().filter(|(_, age)| **age >= limit).map(|(id, _)| id.clone()).collect()
}

// Restore the folder list's invariants: only real prompt ids, no prompt in two folders
// (the first wins), a sane scale, and no folder with fewer than two members (it dissolves
// and its pill reappears at the free position it kept). Runs on load and after every
// mutation; idempotent.
pub(crate) fn normalize_float_folders(folders: &mut Vec<FloatFolder>, prompt_ids: &[String]) {
    normalize_float_folders_min(folders, prompt_ids, 2);
}

// The shared body; `min_members` is 2 everywhere except after a sync batch (see md_sync).
pub(crate) fn normalize_float_folders_min(folders: &mut Vec<FloatFolder>, prompt_ids: &[String], min_members: usize) {
    let mut taken: Vec<String> = Vec::new();
    for folder in folders.iter_mut() {
        folder.scale = if folder.scale.is_finite() {
            folder.scale.clamp(0.5, 3.0)
        } else {
            1.0
        };
        folder.members.retain(|id| {
            let ok = prompt_ids.iter().any(|p| p == id) && !taken.contains(id);
            if ok {
                taken.push(id.clone());
            }
            ok
        });
    }
    folders.retain(|f| f.members.len() >= min_members.max(1));
    folders.truncate(MAX_FLOAT_FOLDERS);
}

// Which pill windows and which folder windows should be open right now. While the
// feature is off no folder exists on screen and every member is a plain pill again,
// which is exactly how switching it off restores the free positions.
pub(crate) fn float_window_plan(settings: &Settings, prompt_ids: &[String]) -> (Vec<String>, Vec<String>) {
    let on = folders_on(settings);
    let mut inside: Vec<&str> = Vec::new();
    if on {
        for folder in &settings.float_folders {
            for member in &folder.members {
                inside.push(member.as_str());
            }
        }
    }
    let pills = prompt_ids
        .iter()
        .filter(|id| settings.floating.contains_key(*id) && !inside.contains(&id.as_str()))
        .cloned()
        .collect();
    let folders = if on {
        settings.float_folders.iter().map(|f| f.id.clone()).collect()
    } else {
        Vec::new()
    };
    (pills, folders)
}

pub(crate) fn folder_name_of(name: &str) -> Result<String, String> {
    let name: String = name
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string();
    if name.chars().count() > MAX_FOLDER_NAME {
        return Err(format!("folder name longer than {} characters", MAX_FOLDER_NAME));
    }
    Ok(name)
}

// Where the icon sits inside an EXPANDED folder window (offset + size, physical px).
// No entry = collapsed, i.e. the window IS the icon. In memory only: an open folder
// never survives a restart.
pub(crate) struct FolderExpand(pub(crate) Mutex<HashMap<String, Rect>>);

// Pill/folder windows dismissed "for now" (their own context menu, not the
// permanent Remove/Dissolve). In memory only, like FolderExpand above: cleared
// on restart, never written to settings.
pub(crate) struct HiddenFloats(pub(crate) Mutex<std::collections::HashSet<String>>);

// Where a window was last placed by the app itself, per label. The window's own Moved event
// fires for that placement too, and it must not count as the user dragging the pill: that
// would overwrite the stored home of a parked pill with its park spot.
static PLACED_BY_APP: Mutex<Vec<(String, (i32, i32))>> = Mutex::new(Vec::new());

// Remembers (or, with None, forgets) the spot the app just moved a window to.
fn note_app_placement(label: &str, spot: Option<(i32, i32)>) {
    let mut v = PLACED_BY_APP.lock().unwrap_or_else(|e| e.into_inner());
    v.retain(|(l, _)| l != label);
    if let Some(s) = spot {
        v.push((label.to_string(), s));
    }
}

// Whether a Moved event is the user dragging. An event that lands on the spot the app set
// is not; the first event anywhere else is, and it drops the remembered spot.
pub(crate) fn moved_is_user_drag(placed: &mut Vec<(String, (i32, i32))>, label: &str, at: (i32, i32)) -> bool {
    match placed.iter().position(|(l, _)| l == label) {
        Some(i) if placed[i].1 == at => false,
        Some(i) => {
            placed.remove(i);
            true
        }
        None => true,
    }
}

fn move_is_user_drag(label: &str, at: (i32, i32)) -> bool {
    moved_is_user_drag(&mut PLACED_BY_APP.lock().unwrap_or_else(|e| e.into_inner()), label, at)
}

// A pill/folder currently parked (its saved screen is not among today's monitors) and when
// this run first noticed. In memory only: the marker and cleanup clock reset on restart,
// while the position and screen id in Settings are never touched. floating.js/folder.js ask
// for presence to show the marker; the age is what the floatParkCleanupH sweep compares.
pub(crate) struct ParkedFloats(pub(crate) Mutex<HashMap<String, std::time::Instant>>);

// Records or clears one id's parked state. An already-parked id keeps its first-seen
// instant, so re-syncing (any settings change) never resets its cleanup clock.
pub(crate) fn mark_parked(app: &AppHandle, id: &str, parked: bool) {
    if let Some(state) = app.try_state::<ParkedFloats>() {
        let mut map = state.0.lock().unwrap_or_else(|e| e.into_inner());
        if parked {
            map.entry(id.to_string()).or_insert_with(std::time::Instant::now);
        } else {
            map.remove(id);
        }
    }
}

// Whether a pill/folder is parked right now: floating.js/folder.js ask this
// once on boot to decide whether to show the marker.
#[tauri::command]
pub(crate) fn is_float_parked(app: AppHandle, id: String) -> bool {
    app.try_state::<ParkedFloats>()
        .map(|s| s.0.lock().unwrap_or_else(|e| e.into_inner()).contains_key(&id))
        .unwrap_or(false)
}

pub(crate) fn folder_expansion(app: &AppHandle, id: &str) -> Option<Rect> {
    let state = app.try_state::<FolderExpand>()?;
    let map = state.0.lock().unwrap_or_else(|e| e.into_inner());
    map.get(id).copied()
}

pub(crate) fn set_folder_expansion(app: &AppHandle, id: &str, icon: Option<Rect>) {
    if let Some(state) = app.try_state::<FolderExpand>() {
        let mut map = state.0.lock().unwrap_or_else(|e| e.into_inner());
        match icon {
            Some(r) => {
                map.insert(id.to_string(), r);
            }
            None => {
                map.remove(id);
            }
        }
    }
}

// The folder icon's box on screen, whether the window is collapsed (window = icon) or
// expanded (icon sits at a stored offset inside the grown window).
pub(crate) fn folder_icon_rect(app: &AppHandle, id: &str, window: Rect) -> Rect {
    match folder_expansion(app, id) {
        Some(off) => Rect {
            x: window.x + off.x,
            y: window.y + off.y,
            w: off.w,
            h: off.h,
        },
        None => window,
    }
}

// A window's box in physical pixels. On Windows this reads the HWND directly: the live
// merge hint polls it while the OS move loop owns the pointer, and a plain Win32 read
// needs nothing from the stalled event loop.
#[cfg(windows)]
pub(crate) fn window_rect_native(win: &tauri::WebviewWindow) -> Option<Rect> {
    #[repr(C)]
    struct RectW { left: i32, top: i32, right: i32, bottom: i32 }
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowRect(hwnd: isize, r: *mut RectW) -> i32;
    }
    let hwnd = win.hwnd().ok()?.0 as isize;
    let mut r = RectW { left: 0, top: 0, right: 0, bottom: 0 };
    if unsafe { GetWindowRect(hwnd, &mut r) } == 0 {
        return None;
    }
    Some(Rect {
        x: r.left as f64,
        y: r.top as f64,
        w: (r.right - r.left) as f64,
        h: (r.bottom - r.top) as f64,
    })
}

pub(crate) fn window_rect(win: &tauri::WebviewWindow) -> Option<Rect> {
    #[cfg(windows)]
    if let Some(r) = window_rect_native(win) {
        return Some(r);
    }
    let p = win.outer_position().ok()?;
    let s = win.outer_size().ok()?;
    Some(Rect {
        x: p.x as f64,
        y: p.y as f64,
        w: s.width as f64,
        h: s.height as f64,
    })
}

// Work area (screen minus taskbar) of the monitor under a physical point. Falls back
// to the primary monitor when the point lies in dead space between screens.
pub(crate) fn work_rect_at(app: &AppHandle, x: f64, y: f64) -> Rect {
    let win = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next());
    let monitor = win.as_ref().and_then(|w| {
        w.monitor_from_point(x, y)
            .ok()
            .flatten()
            .or_else(|| w.primary_monitor().ok().flatten())
    });
    match monitor {
        Some(m) => {
            let area = m.work_area();
            Rect {
                x: area.position.x as f64,
                y: area.position.y as f64,
                w: area.size.width as f64,
                h: area.size.height as f64,
            }
        }
        None => Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 },
    }
}

// Move and resize in one OS call, in physical pixels (two calls leave a visibly wrong
// intermediate frame).
pub(crate) fn set_bounds_phys(win: &tauri::WebviewWindow, r: Rect) {
    let (x, y) = (r.x.round() as i32, r.y.round() as i32);
    let (w, h) = (r.w.round().max(1.0) as i32, r.h.round().max(1.0) as i32);
    #[cfg(windows)]
    if set_window_pos_phys(win, x, y, w, h) {
        return;
    }
    let _ = win.set_position(PhysicalPosition::new(x, y));
    let _ = win.set_size(PhysicalSize::new(w.max(1) as u32, h.max(1) as u32));
}

pub(crate) fn folder_box(scale: f64) -> f64 {
    FOLDER_BOX * if scale.is_finite() { scale.clamp(0.5, 3.0) } else { 1.0 }
}

// One folder window, built exactly like a pill: frameless, transparent, always on top,
// out of the taskbar and out of other apps' screen captures.
pub(crate) fn open_folder_window(app: &AppHandle, folder: &FloatFolder, taken: &mut Vec<Rect>) {
    let label = folder_label(&folder.id);
    if defer_while_closing(app, &label) {
        return;
    }
    // Never call Tauri window APIs while holding the Db lock (deadlock risk).
    let excluded = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        capture_excluded(&store.settings)
    };
    let side = folder_box(folder.scale);
    let dpi = main_scale_factor(app); // physical spots, logical size
    let (x, y, parked) = resolve_float_spot(&folder.pos, side * dpi, side * dpi, &screen_snapshot(app), taken);
    taken.push(Rect { x, y, w: side * dpi, h: side * dpi });
    mark_parked(app, &folder.id, parked);

    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("folder.html".into()))
        .title(&folder.name)
        .inner_size(side, side)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .on_page_load(reveal_when_loaded)
        .build();

    if let Ok(win) = win {
        let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
        set_capture_exclusion(&win, excluded);
        let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        note_app_placement(&label, parked.then(|| (x.round() as i32, y.round() as i32)));
        spawn_reveal_fallback(app, label.clone());

        let app2 = app.clone();
        let fid = folder.id.clone();
        let wlabel = label.clone();
        win.on_window_event(move |event| match event {
            WindowEvent::Moved(p) => {
                // While the folder is open the window origin is not the icon, so the stored position
                // follows the icon. A drag is the user placing the folder, so it becomes the real position
                // and ends parking. The app's own placement of a parked folder is not a drag.
                if !move_is_user_drag(&wlabel, (p.x, p.y)) {
                    return;
                }
                let off = folder_expansion(&app2, &fid).unwrap_or(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 });
                let icon_x = p.x + off.x.round() as i32;
                let icon_y = p.y + off.y.round() as i32;
                let screen = screen_id_at(&app2, icon_x, icon_y);
                if let Some(state) = app2.try_state::<Db>() {
                    // Fires for every pixel of a drag: never block the move loop.
                    if let Ok(mut store) = state.try_lock() {
                        if let Some(f) = store.settings.float_folders.iter_mut().find(|f| f.id == fid) {
                            f.pos = Pos { x: icon_x, y: icon_y, screen };
                        }
                    }
                }
                mark_parked(&app2, &fid, false);
            }
            WindowEvent::Destroyed => {
                float_destroyed(&app2, &wlabel);
                set_folder_expansion(&app2, &fid, None);
                save_settings_off_event_loop(&app2);
            }
            _ => {}
        });
    }
}

// Open/close pill and folder windows until the desktop matches the stored plan. The
// single place that turns settings into windows, so every mutation just calls it.
// MUST NOT run while the store mutex is held: it creates and closes windows.
pub(crate) fn sync_float_windows(app: &AppHandle) {
    // Window queries before the store lock (never call window APIs while holding it).
    let live: std::collections::HashSet<String> = app.webview_windows().keys().cloned().collect();
    let (want_pills, want_folders, pills, folders) = {
        let state: State<Db> = app.state();
        let store = lock(&state);
        let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
        let (mut want_pills, mut want_folders) = float_window_plan(&store.settings, &ids);
        // "Hide for now" drops the id from the plan, so the stale-window loop below closes an
        // open window and it stays closed until the next launch.
        if let Some(hidden) = app.try_state::<HiddenFloats>() {
            let hidden = hidden.0.lock().unwrap_or_else(|e| e.into_inner());
            want_pills.retain(|id| !hidden.contains(id));
            want_folders.retain(|id| !hidden.contains(id));
        }
        let pills: Vec<Prompt> = store
            .prompts
            .iter()
            .filter(|p| want_pills.contains(&p.id))
            // open_floating would return at once for a pill whose window is up and not closing;
            // skipping it here keeps its (possibly large) picture from being cloned on every sync.
            .filter(|p| {
                let label = flabel(&p.id);
                reopen_step(live.contains(&label), is_closing(&label)) != ReopenStep::Keep
            })
            .cloned()
            .collect();
        let folders: Vec<FloatFolder> = store
            .settings
            .float_folders
            .iter()
            .filter(|f| want_folders.contains(&f.id))
            .cloned()
            .collect();
        (want_pills, want_folders, pills, folders)
    };
    for (label, win) in app.webview_windows() {
        let stale = match (label.strip_prefix("float-"), label.strip_prefix("folder-")) {
            (Some(id), _) => !want_pills.iter().any(|w| w == id),
            (_, Some(id)) => !want_folders.iter().any(|w| w == id),
            _ => false,
        };
        if stale {
            close_tracked(&label, &win);
        }
    }
    // Every box already on screen, so a pill/folder opened below (parked or
    // not) fans out around what is already there instead of landing on it.
    let mut taken: Vec<Rect> = app.webview_windows().values().filter_map(window_rect).collect();
    for prompt in &pills {
        open_floating(app, prompt, &mut taken); // returns early when the window already exists
    }
    for folder in &folders {
        open_folder_window(app, folder, &mut taken);
    }
    // Whatever changed the plan also changed what an open folder shows.
    for label in app.webview_windows().keys() {
        if label.starts_with("folder-") {
            let _ = app.emit_to(label.as_str(), "folder-changed", ());
        }
    }
}

// The boxes float_recall spreads out, in physical pixels: every pill (in a stable
// id order) and then every folder, each sized by its own scale and the screen's
// scale factor. Returns the pill ids in the same order as their boxes.
pub(crate) fn recall_wishes(settings: &Settings, prompts: &[Prompt], work: Rect, dpi: f64) -> (Vec<String>, Vec<Rect>) {
    let mut wishes: Vec<Rect> = Vec::new();
    let mut pill_ids: Vec<String> = settings.floating.keys().cloned().collect();
    pill_ids.sort(); // stable fan-out order across runs
    for id in &pill_ids {
        let scale = float_scale_of(settings, id);
        let (w, h) = match prompts.iter().find(|p| &p.id == id) {
            Some(p) => pill_dims(is_image_prompt(p), scale),
            None => pill_dims(false, scale),
        };
        wishes.push(Rect { x: work.x, y: work.y, w: w * dpi, h: h * dpi });
    }
    for folder in &settings.float_folders {
        let side = folder_box(folder.scale) * dpi;
        wishes.push(Rect { x: work.x, y: work.y, w: side, h: side });
    }
    (pill_ids, wishes)
}

// Expert action "Bring every floating window back to the main screen": the emergency exit
// when monitor changes have left positions not worth untangling. Every pill and folder,
// parked or not, gets a fresh spread-out spot on the main screen and forgets its old
// screen identity.
#[tauri::command]
pub(crate) async fn float_recall(app: AppHandle, state: State<'_, Db>) -> Result<(), String> {
    // Monitor and window queries wait for the main thread, so they run before
    // the store lock is taken: the main thread may be waiting on that lock.
    let screens = screen_snapshot(&app);
    let (main_id, mx, my, mw, mh) = screens
        .first()
        .cloned()
        .unwrap_or((String::new(), 0, 0, 1920, 1080));
    let work = Rect { x: mx as f64, y: my as f64, w: mw as f64, h: mh as f64 };
    // The work area is in physical pixels, the box sizes in logical ones: scale
    // them by the main screen's own factor, as the folder dissolve does.
    let dpi = main_scale_factor(&app);
    let mut moves: Vec<(String, i32, i32)> = Vec::new();
    {
        let mut store = lock(&state);
        let (pill_ids, wishes) = recall_wishes(&store.settings, &store.prompts, work, dpi);
        let spots = spread_free_positions(&wishes, &[], work);
        for (id, (x, y)) in pill_ids.iter().zip(spots.iter()) {
            moves.push((flabel(id), x.round() as i32, y.round() as i32));
        }
        for (folder, (x, y)) in store.settings.float_folders.iter().zip(spots.iter().skip(pill_ids.len())) {
            moves.push((folder_label(&folder.id), x.round() as i32, y.round() as i32));
        }

        for (id, (x, y)) in pill_ids.iter().zip(spots.iter()) {
            store.settings.floating.insert(
                id.clone(),
                Pos { x: x.round() as i32, y: y.round() as i32, screen: main_id.clone() },
            );
        }
        for (folder, (x, y)) in store.settings.float_folders.iter_mut().zip(spots.iter().skip(pill_ids.len())) {
            folder.pos = Pos { x: x.round() as i32, y: y.round() as i32, screen: main_id.clone() };
        }

        save_settings(&app, &store.settings);
    }
    if let Some(state) = app.try_state::<ParkedFloats>() {
        state.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    // A window that is already open keeps its place through sync_float_windows, so it
    // is moved here; an open folder's origin sits off its icon by the grown box.
    for (label, x, y) in moves {
        if let Some(win) = app.get_webview_window(&label) {
            let off = label
                .strip_prefix("folder-")
                .and_then(|id| folder_expansion(&app, id))
                .unwrap_or(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 });
            let _ = win.set_position(PhysicalPosition::new(x - off.x.round() as i32, y - off.y.round() as i32));
        }
    }
    sync_float_windows(&app);
    notify_float_settings(&app);
    Ok(())
}

// "Hide for now": a pill or folder id (separate id spaces, never colliding), dismissed
// until restart. Not the permanent unpin (toggle_floating/dissolve_float_folder):
// settings.floating / float_folders stay untouched.
#[tauri::command]
pub(crate) async fn hide_float_for_now(app: AppHandle, id: String) {
    if let Some(state) = app.try_state::<HiddenFloats>() {
        state.0.lock().unwrap_or_else(|e| e.into_inner()).insert(id);
    }
    sync_float_windows(&app);
}

// Must stay async: window creation from a sync command deadlocks on Windows
// (sync commands run on the main thread, which WebView2 needs free).
#[tauri::command]
pub(crate) async fn toggle_floating(app: AppHandle, state: State<'_, Db>, id: String) -> Result<bool, String> {
    // "Pinned" is what `settings.floating` says, never whether a window is open: a pill inside
    // a folder has no window, and one hidden until restart is still pinned. Deciding from the
    // window would reopen such a pill on a click the menu labelled "remove". The folder check
    // also takes a member out of its folder.
    let pinned = {
        let store = lock(&state);
        store.settings.floating.contains_key(&id)
            || (folders_on(&store.settings)
                && store.settings.float_folders.iter().any(|f| f.members.contains(&id)))
    };
    if pinned {
        {
            let mut store = lock(&state);
            store.settings.floating.remove(&id);
            store.settings.float_scale.remove(&id);
            for folder in store.settings.float_folders.iter_mut() {
                folder.members.retain(|m| m != &id);
            }
            let ids: Vec<String> = store.prompts.iter().map(|p| p.id.clone()).collect();
            normalize_float_folders(&mut store.settings.float_folders, &ids);
            save_settings(&app, &store.settings);
        }
        close_floating_window(&app, &id);
        sync_float_windows(&app); // a folder left with one member dissolves
        // The main window keeps its own settings.floating mirror for the tile menu's pin/unpin
        // label (cacheFloatingState in gridFolders.js); notify_float_settings excludes it, so
        // without this event the mirror would show "Remove" on a tile whose pill is gone.
        let _ = app.emit_to("main", "floating-pin-changed", serde_json::json!({ "id": id, "pinned": false }));
        Ok(false)
    } else {
        let prompt = {
            let store = lock(&state);
            store.prompts.iter().find(|p| p.id == id).cloned()
        };
        match prompt {
            Some(p) => {
                // HiddenFloats is cleared nowhere else: re-pinning must take the id out, or the next
                // sync_float_windows (any settings change) closes the pill again.
                if let Some(hs) = app.try_state::<HiddenFloats>() {
                    hs.0.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                }
                // An unpin just before closes the window asynchronously: build only once its label is free.
                wait_label_gone(&app, &flabel(&id)).await;
                // Record the pin before building: open_floating may defer while the old window is
                // still tearing down, and the replay builds only what settings.floating lists.
                // The monitor probe in default_pos runs before the store lock is taken.
                let count = lock(&state).settings.floating.len();
                let spot = default_pos(&app, count);
                {
                    let mut store = lock(&state);
                    if !store.settings.floating.contains_key(&id) {
                        store.settings.floating.insert(id.clone(), spot);
                        save_settings(&app, &store.settings);
                    }
                }
                let mut taken: Vec<Rect> = app.webview_windows().values().filter_map(window_rect).collect();
                open_floating(&app, &p, &mut taken);
                let _ = app.emit_to("main", "floating-pin-changed", serde_json::json!({ "id": id, "pinned": true }));
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

// Size factor of one pill; persists the choice. resize=false leaves the
// window alone (the menu flow sizes it itself, avoiding a visible jump).
#[tauri::command]
pub(crate) async fn set_float_scale(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    scale: f64,
    resize: Option<bool>,
) -> Result<(), String> {
    let scale = if scale.is_finite() { scale.clamp(FLOAT_SCALE_MIN, FLOAT_SCALE_MAX) } else { 1.0 };
    let is_img = {
        let mut store = lock(&state);
        store.settings.float_scale.insert(id.clone(), scale);
        save_settings(&app, &store.settings);
        store.prompts.iter().find(|p| p.id == id).map(is_image_prompt).unwrap_or(false)
    };
    if resize.unwrap_or(true) {
        if let Some(win) = app.get_webview_window(&flabel(&id)) {
            let (w, h) = pill_dims(is_img, scale);
            let _ = win.set_size(tauri::LogicalSize::new(w, h));
        }
    }
    Ok(())
}

// Text pills grow with their label: the frontend measures the text and requests
// the matching window box (width and height, kept pill-shaped so it never rounds
// into a circle).
#[tauri::command]
pub(crate) async fn resize_float_pill(
    app: AppHandle,
    state: State<'_, Db>,
    id: String,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let scale = float_scale_of(&lock(&state).settings, &id);
    if let Some(win) = app.get_webview_window(&flabel(&id)) {
        let w = if width.is_finite() { width.clamp(80.0, 8000.0) } else { FLOAT_W * scale };
        let h = if height.is_finite() { height.clamp(40.0, 8000.0) } else { FLOAT_H * scale };
        let _ = win.set_size(tauri::LogicalSize::new(w, h));
    }
    Ok(())
}

// Grow the pill window while its context menu is open; shrink back on close.
// Media pills: the window matches the media's aspect ratio (no invisible
// click area beyond the visible video/image).
#[tauri::command]
pub(crate) async fn resize_float_media(app: AppHandle, id: String, width: f64, height: f64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&flabel(&id)) {
        if width.is_finite() && height.is_finite() {
            let w = width.clamp(48.0, 4000.0);
            let h = height.clamp(48.0, 4000.0);
            let _ = win.set_size(tauri::LogicalSize::new(w, h));
        }
    }
    Ok(())
}

// Move AND resize a floating window in ONE OS call so the resize stays smooth.
// Tauri's separate set_position + set_size leave a one-frame intermediate state
// (new position, old size) that flickers the edges while dragging a grip.
#[cfg(windows)]
pub(crate) fn set_window_pos_phys(win: &tauri::WebviewWindow, x: i32, y: i32, w: i32, h: i32) -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    }
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    let hwnd = match win.hwnd() {
        Ok(h) => h.0 as isize,
        Err(_) => return false,
    };
    unsafe { SetWindowPos(hwnd, 0, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) != 0 }
}

#[cfg(windows)]
pub(crate) fn set_float_bounds_native(win: &tauri::WebviewWindow, x: f64, y: f64, w: f64, h: f64) -> bool {
    let s = win.scale_factor().unwrap_or(1.0);
    set_window_pos_phys(
        win,
        (x * s).round() as i32,
        (y * s).round() as i32,
        (w * s).round() as i32,
        (h * s).round() as i32,
    )
}

// Expert privacy flag (Datenschutz tab), phrased from the user's side: "allow
// screenshots of this app". It ships OFF, so the window stays out of screen captures
// until someone deliberately lets them through.
pub(crate) const ALLOW_SCREENSHOT_KEY: &str = "allowScreenshot";

pub(crate) fn capture_excluded(settings: &Settings) -> bool {
    settings.ui_flags.get(ALLOW_SCREENSHOT_KEY) != Some(&true)
}

// Keep our own windows out of other capture tools (Snipping Tool, Print Screen, Game Bar,
// our window picker): an excluded window stays visible to the user but is omitted from
// captures. Our own snip hides these windows first, so it is unaffected.
// WDA_EXCLUDEFROMCAPTURE needs Windows 10 2004+; on older builds the call fails and the
// window stays capturable. No WDA_MONITOR fallback, since a black rectangle in screenshots
// is worse.
#[cfg(windows)]
pub(crate) fn set_capture_exclusion(win: &tauri::WebviewWindow, excluded: bool) {
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowDisplayAffinity(hwnd: isize, affinity: u32) -> i32;
    }
    const WDA_NONE: u32 = 0x0;
    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;
    if let Ok(h) = win.hwnd() {
        let aff = if excluded { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
        unsafe {
            SetWindowDisplayAffinity(h.0 as isize, aff);
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn set_capture_exclusion(_win: &tauri::WebviewWindow, _excluded: bool) {}

// The window's current display affinity as the system reports it (0 = none,
// 0x11 = excluded from capture), or None when it cannot be read.
#[cfg(windows)]
pub(crate) fn display_affinity(win: &tauri::WebviewWindow) -> Option<u32> {
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowDisplayAffinity(hwnd: isize, affinity: *mut u32) -> i32;
    }
    let h = win.hwnd().ok()?;
    let mut aff: u32 = 0;
    let ok = unsafe { GetWindowDisplayAffinity(h.0 as isize, &mut aff) };
    if ok != 0 { Some(aff) } else { None }
}

#[cfg(not(windows))]
pub(crate) fn display_affinity(_win: &tauri::WebviewWindow) -> Option<u32> {
    None
}

// The recording bar and the region outline stay out of every capture whatever
// the privacy setting says: a recording must never contain its own controls.
pub(crate) fn always_excluded(label: &str) -> bool {
    label == "rec-bar" || label == "rec-outline"
}

// Whether a window has to be capture-excluded, given the state the privacy
// setting alone asks for. Every loop that applies the affinity to all windows
// goes through this, so the setting can never clear a permanent exclusion.
pub(crate) fn wants_capture_exclusion(label: &str, setting_excluded: bool) -> bool {
    always_excluded(label) || setting_excluded
}

// Pure: does a readback agree with what was asked for? An unreadable affinity
// counts as agreeing, since there is nothing to compare.
pub(crate) fn affinity_matches(want_excluded: bool, readback: Option<u32>) -> bool {
    match readback {
        Some(a) => (a != 0) == want_excluded,
        None => true,
    }
}

// Pure: whether the app's own screenshot, colour picker and recording picker
// hide the app windows before freezing the desktop. With the privacy setting
// allowing screenshots the app stays visible in its own tools.
pub(crate) fn capture_hides_app_windows(setting_excluded: bool) -> bool {
    setting_excluded
}

// Apply the privacy setting to every open window and read the result back.
// A window that still reports the wrong state gets the call once more. Returns
// true when a window that must be capturable still reports exclusion after
// that (some Windows builds keep it), so the caller can tell the user.
pub(crate) fn apply_capture_exclusion_all(app: &AppHandle, setting_excluded: bool) -> bool {
    let mut stuck = false;
    for (label, win) in app.webview_windows() {
        let want = wants_capture_exclusion(&label, setting_excluded);
        set_capture_exclusion(&win, want);
        if !affinity_matches(want, display_affinity(&win)) {
            set_capture_exclusion(&win, want);
            if !affinity_matches(want, display_affinity(&win)) && !want {
                stuck = true;
            }
        }
    }
    if stuck {
        let _ = app.emit_to("main", "capture-restart-notice", ());
    }
    stuck
}
