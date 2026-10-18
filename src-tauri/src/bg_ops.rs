use super::*;

// ---------- Background operations: one source for the taskbar icon AND
// the running-jobs list in the main window's header ----------
//
// Export, recording conversion, the ffmpeg download, a backup and an import can each run
// for a while with the window in the background. Each reports its state into this small
// registry instead of touching the taskbar icon directly, so two running at once, or one
// finishing while another starts, combine into one reading instead of overwriting each
// other. The same registry backs the header popover's job list, so the two views cannot
// disagree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum OpPhase {
    Running(u8), // a percent value, never above 100
    Indeterminate,
    Error,
    // Kept around (not removed) for bgDoneKeepMin minutes so a finished job
    // still shows in the header popover after it completes.
    Done,
}

#[derive(Clone, Debug)]
pub(crate) struct BgOp {
    // Feature grouping ("export", "convert", "ffmpeg-download", ...) the frontend uses for its
    // label/icon/cancel-command lookup. Distinct from the registry key (see bg_ops() below),
    // which is unique per job so a batch of same-kind jobs gets one row each.
    pub(crate) kind: String,
    pub(crate) phase: OpPhase,
    pub(crate) target: Option<String>, // destination file/folder, for the popover's "open folder"
    pub(crate) started_at_ms: u64,
    pub(crate) finished_at_ms: Option<u64>,
}

impl BgOp {
    pub(crate) fn fresh(kind: &str) -> Self {
        Self { kind: kind.to_string(), phase: OpPhase::Indeterminate, target: None, started_at_ms: now_ms(), finished_at_ms: None }
    }
    pub(crate) fn state_str(&self) -> &'static str {
        match self.phase {
            OpPhase::Running(_) => "running",
            OpPhase::Indeterminate => "indeterminate",
            OpPhase::Error => "error",
            OpPhase::Done => "done",
        }
    }
    pub(crate) fn pct(&self) -> Option<u8> {
        match self.phase {
            OpPhase::Running(v) => Some(v),
            _ => None,
        }
    }
}

// What the frontend reads (`list_bg_ops`): OpPhase has no serde impl, so this is the
// flattened JSON shape. `id` is the registry's unique key (see bg_ops() below); `kind` is
// the feature grouping.
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct BgOpInfo {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) target: Option<String>,
    pub(crate) state: String,
    pub(crate) pct: Option<u8>,
    pub(crate) started_at_ms: u64,
    pub(crate) finished_at_ms: Option<u64>,
}

pub(crate) fn bg_ops() -> &'static Mutex<HashMap<String, BgOp>> {
    use std::sync::OnceLock;
    static OPS: OnceLock<Mutex<HashMap<String, BgOp>>> = OnceLock::new();
    OPS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// Pure and window-free so the state machine (several operations at once, one erroring, all
// finishing) is unit-testable without an HWND. A finished job stays in the map for the
// popover but never affects the icon: any running error wins outright; otherwise no active
// job is "off"; otherwise any indeterminate operation wins over a plain percentage;
// otherwise the percentage is the average across running operations.
pub(crate) fn aggregate_taskbar_state(ops: &HashMap<String, BgOp>) -> (&'static str, Option<u8>) {
    let active: Vec<&BgOp> = ops.values().filter(|o| o.phase != OpPhase::Done).collect();
    if active.iter().any(|o| o.phase == OpPhase::Error) {
        return ("error", None);
    }
    if active.is_empty() {
        return ("off", None);
    }
    if active.iter().any(|o| o.phase == OpPhase::Indeterminate) {
        return ("indeterminate", None);
    }
    let sum: u32 = active.iter().map(|o| o.pct().unwrap_or(0) as u32).sum();
    ("normal", Some((sum / active.len() as u32) as u8))
}

// A finished job survives for `keep_min` minutes so the header popover still
// shows it, then is swept away on the next read or write.
pub(crate) fn prune_finished_ops(ops: &mut HashMap<String, BgOp>, keep_min: f64) {
    let cutoff = now_ms().saturating_sub((keep_min.max(0.0) * 60_000.0) as u64);
    ops.retain(|_, o| o.finished_at_ms.map_or(true, |f| f >= cutoff));
}

pub(crate) fn bg_ops_running_count() -> usize {
    bg_ops()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .filter(|o| matches!(o.phase, OpPhase::Running(_) | OpPhase::Indeterminate))
        .count()
}

pub(crate) fn bg_done_keep_min(state_db: &State<Db>) -> f64 {
    state_db
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .settings
        .ui_values
        .get("bgDoneKeepMin")
        .copied()
        .unwrap_or(10.0)
        .max(0.0)
}

// The entry a progress or start report writes to. The ids are fixed ("backup", "import"),
// so a second run finds the first one's finished entry and replaces it with a fresh one;
// otherwise the new run would keep the old start time (which the UI keys its once-per-run
// "done" toast on) and finish silently.
pub(crate) fn entry_for_run<'a>(ops: &'a mut std::collections::HashMap<String, BgOp>, id: &str, kind: &str) -> &'a mut BgOp {
    if ops.get(id).is_some_and(|o| o.finished_at_ms.is_some()) {
        ops.insert(id.to_string(), BgOp::fresh(kind));
    }
    ops.entry(id.to_string()).or_insert_with(|| BgOp::fresh(kind))
}

// The one place `bg_ops()` is written. Free of any AppHandle/window dependency: an export's
// progress closure calls it directly (see export_video_blocking's caller), so a job keeps
// reporting after the window that started it is gone (proved by
// export_progress_survives_a_closed_window).
pub(crate) fn record_bg_op(id: &str, kind: &str, state: &str, value: Option<u32>, target: Option<String>, keep_min: f64) -> (&'static str, Option<u8>) {
    let mut ops = bg_ops().lock().unwrap_or_else(|e| e.into_inner());
    match state {
        "done" => {
            if let Some(existing) = ops.get_mut(id) {
                existing.phase = OpPhase::Done;
                existing.finished_at_ms = Some(now_ms());
            }
        }
        "error" => {
            let entry = ops.entry(id.to_string()).or_insert_with(|| BgOp::fresh(kind));
            entry.phase = OpPhase::Error;
            entry.finished_at_ms = Some(now_ms());
            if target.is_some() {
                entry.target = target;
            }
        }
        "indeterminate" => {
            let entry = entry_for_run(&mut ops, id, kind);
            entry.phase = OpPhase::Indeterminate;
            entry.finished_at_ms = None;
            if target.is_some() {
                entry.target = target;
            }
        }
        _ => {
            let entry = entry_for_run(&mut ops, id, kind);
            entry.phase = OpPhase::Running(value.unwrap_or(0).min(100) as u8);
            entry.finished_at_ms = None;
            if target.is_some() {
                entry.target = target;
            }
        }
    }
    prune_finished_ops(&mut ops, keep_min);
    aggregate_taskbar_state(&ops)
}

pub(crate) fn list_bg_ops_impl(keep_min: f64) -> Vec<BgOpInfo> {
    let mut ops = bg_ops().lock().unwrap_or_else(|e| e.into_inner());
    prune_finished_ops(&mut ops, keep_min);
    let mut list: Vec<BgOpInfo> = ops
        .iter()
        .map(|(id, o)| BgOpInfo {
            id: id.clone(),
            kind: o.kind.clone(),
            target: o.target.clone(),
            state: o.state_str().to_string(),
            pct: o.pct(),
            started_at_ms: o.started_at_ms,
            finished_at_ms: o.finished_at_ms,
        })
        .collect();
    list.sort_by_key(|o| o.started_at_ms);
    list
}

// Remembers what the taskbar icon was last told, so a progress tick that would
// change nothing never reaches the UI thread. The window handle is part of the
// key: a rebuilt main window starts with a clean icon.
#[derive(Default)]
pub(crate) struct TaskbarThrottle {
    last: Option<(isize, String, Option<u8>)>,
}

impl TaskbarThrottle {
    // True when (state, percent) differs from the last applied pair. Only a
    // running state carries a percent; the other states ignore it.
    pub(crate) fn should_apply(&mut self, hwnd: isize, state: &str, value: Option<u8>) -> bool {
        let value = if matches!(state, "off" | "indeterminate" | "error") { None } else { value };
        let key = (hwnd, state.to_string(), value);
        if self.last.as_ref() == Some(&key) {
            return false;
        }
        self.last = Some(key);
        true
    }

    // Forgets the last pair so the next tick is applied again (after a failed apply).
    pub(crate) fn reset(&mut self) {
        self.last = None;
    }

    // The decision for one tick: a pending retry request (a failed apply, or the
    // taskbar button having been recreated) forgets the last pair first.
    pub(crate) fn decide(&mut self, retry: &AtomicBool, hwnd: isize, state: &str, value: Option<u8>) -> bool {
        if retry.swap(false, Ordering::SeqCst) {
            self.reset();
        }
        self.should_apply(hwnd, state, value)
    }
}

static TASKBAR_THROTTLE: Mutex<TaskbarThrottle> = Mutex::new(TaskbarThrottle { last: None });
// Set by the main thread when an apply failed; the next call forgets its last pair.
// A flag instead of locking the throttle there: when the caller already is the
// main thread the closure runs inline while the throttle is still held.
static TASKBAR_RETRY: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
thread_local! {
    // The one taskbar object, created on the main thread (it is apartment
    // threaded, so every call has to run there anyway) and initialised once.
    static TASKBAR: std::cell::RefCell<Option<windows::Win32::UI::Shell::ITaskbarList3>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(windows)]
pub(crate) fn apply_taskbar_state(app: &AppHandle, state: &str, value: Option<u8>) {
    let Some(win) = app.get_webview_window("main") else { return };
    let Ok(hwnd) = win.hwnd() else { return };
    let key = hwnd.0 as isize;
    let state = state.to_string();
    // Decide and queue under one lock so two workers cannot reach the UI thread
    // in a different order than their decisions.
    let mut throttle = TASKBAR_THROTTLE.lock().unwrap_or_else(|e| e.into_inner());
    if !throttle.decide(&TASKBAR_RETRY, key, &state, value) {
        return;
    }
    let queued = app.run_on_main_thread(move || {
        if !apply_taskbar_on_main(key, &state, value) {
            TASKBAR_RETRY.store(true, Ordering::SeqCst);
        }
    });
    if queued.is_err() {
        throttle.reset();
    }
}

// Runs on the main thread only. False when the taskbar object is unavailable.
#[cfg(windows)]
fn apply_taskbar_on_main(hwnd: isize, state: &str, value: Option<u8>) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList, TBPF_ERROR, TBPF_INDETERMINATE, TBPF_NOPROGRESS, TBPF_NORMAL};
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    TASKBAR.with(|cell| unsafe {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            // The event loop thread is normally initialised already; a refusal
            // for a different mode is harmless.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let Ok(taskbar): windows::core::Result<ITaskbarList3> = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) else {
                return false;
            };
            if taskbar.HrInit().is_err() {
                return false;
            }
            *slot = Some(taskbar);
        }
        let Some(taskbar) = slot.as_ref() else { return false };
        let ok = match state {
            "off" => taskbar.SetProgressState(hwnd, TBPF_NOPROGRESS).is_ok(),
            "indeterminate" => taskbar.SetProgressState(hwnd, TBPF_INDETERMINATE).is_ok(),
            "error" => taskbar.SetProgressState(hwnd, TBPF_ERROR).is_ok(),
            _ => {
                let a = taskbar.SetProgressValue(hwnd, value.unwrap_or(0) as u64, 100).is_ok();
                let b = taskbar.SetProgressState(hwnd, TBPF_NORMAL).is_ok();
                a && b
            }
        };
        if !ok {
            // The shell most likely restarted: build a fresh object next time.
            *slot = None;
        }
        ok
    })
}

#[cfg(not(windows))]
pub(crate) fn apply_taskbar_state(_app: &AppHandle, _state: &str, _value: Option<u8>) {}

// `op` names the reporting feature ("export", "convert", "ffmpeg-download", "backup",
// "import"); `state` is "running" (with a 0-100 `value`), "indeterminate", "error", or
// "done" (marks this operation's entry done, kept for bgDoneKeepMin minutes). `target` is an
// optional destination file/folder for the header popover's "open folder" button; None
// leaves a previously reported target unchanged. Every caller is expected to send "done" or
// "error" eventually, also on its cancel/failure path: a taskbar icon stuck mid-progress is
// worse than none. The registry is updated regardless of the taskbarProgress switch, which
// only controls the Windows icon.
//
// A thin AppHandle-holder around push_bg_op, kept as its own function so the frontend has
// one stable command name; the Rust progress loops (export_video, convert_recording,
// download_ffmpeg) call push_bg_op directly, so their reporting survives a closed window.
// Async: a sync command runs on the main thread, and push_bg_op takes the store lock, which
// a backup or import can hold for a long time.
#[tauri::command]
pub(crate) async fn set_taskbar_progress(app: AppHandle, op: String, state: String, value: Option<u32>, target: Option<String>) {
    // This command's callers (backup, import, ...) run one instance at a time, so job id and
    // kind are the same string; export_video's push_bg_op calls give a batch one id each under
    // the shared kind "export".
    push_bg_op(&app, &op, &op, &state, value, target);
}

// Updates the shared registry AND the real taskbar icon in one call --
// exactly what the set_taskbar_progress command above does, but usable from
// anywhere in Rust that already holds an AppHandle, with no window or
// command-invocation round trip required. `id` is this job's own unique
// registry key; `kind` is the feature grouping shown to the frontend.
pub(crate) fn push_bg_op(app: &AppHandle, id: &str, kind: &str, state: &str, value: Option<u32>, target: Option<String>) {
    let keep_min = app.try_state::<Db>().map(|s| bg_done_keep_min(&s)).unwrap_or(10.0);
    let (agg_state, agg_value) = record_bg_op(id, kind, state, value, target, keep_min);
    // One broadcast point for every state change (running tick, done, error): the header
    // popover listens instead of polling, so a job finishing shows even when the window that
    // started it (the video editor, say) is the one whose own promise resolves.
    let _ = app.emit("bg-ops-changed", ());
    let enabled = app
        .try_state::<Db>()
        .map(|s| {
            s.lock()
                .unwrap_or_else(|e| e.into_inner())
                .settings
                .ui_flags
                .get("taskbarProgress")
                .copied()
                .unwrap_or(true)
        })
        .unwrap_or(true);
    apply_taskbar_aggregate(app, enabled, agg_state, agg_value);
}

fn apply_taskbar_aggregate(app: &AppHandle, enabled: bool, state: &str, value: Option<u8>) {
    if enabled {
        apply_taskbar_state(app, state, value);
    } else {
        apply_taskbar_state(app, "off", None);
    }
}

// The main window's taskbar button starts without progress whenever it is created
// anew (shown again after hiding to the tray). Applies the current state once more
// regardless of what was applied before.
pub(crate) fn reapply_taskbar_state(app: &AppHandle) {
    let (state, value) = {
        let ops = bg_ops().lock().unwrap_or_else(|e| e.into_inner());
        aggregate_taskbar_state(&ops)
    };
    let enabled = app
        .try_state::<Db>()
        // Runs on the event-loop thread (show_main): a store held by a backup or
        // import counts as "on" instead of blocking every window.
        .and_then(|s| {
            try_lock_ignoring_poison(&s).map(|store| store.settings.ui_flags.get("taskbarProgress").copied().unwrap_or(true))
        })
        .unwrap_or(true);
    TASKBAR_RETRY.store(true, Ordering::SeqCst);
    apply_taskbar_aggregate(app, enabled, state, value);
}

#[tauri::command(async)]
pub(crate) fn list_bg_ops(state_db: State<Db>) -> Vec<BgOpInfo> {
    list_bg_ops_impl(bg_done_keep_min(&state_db))
}

// Cancels every cancelable kind of running job unconditionally; each is a cheap no-op when
// its job is not running. Backup and import have no cancel primitive (interrupting a
// database write mid-flight is the risky case), so quitting with one running is gated by
// asking first (see the CloseRequested handler), never by aborting it here.
pub(crate) fn cancel_all_bg_ops() {
    cancel_all_exports();
    cancel_recording_convert();
    cancel_ffmpeg_download();
    cancel_ocr_index_backfill();
}

// Opens `path` itself if it is a folder, otherwise its parent: shared by
// every "open folder" button whose path was computed by this app itself
// (a job's own recorded target, a storage-breakdown row's own location),
// never one typed or picked by the user directly.
pub(crate) fn open_explorer_at(path: &str) -> Result<(), String> {
    let p = PathBuf::from(path);
    let dir = if p.is_dir() { p } else { p.parent().map(|d| d.to_path_buf()).unwrap_or(p) };
    if !dir.is_dir() {
        return Err("folder not found".to_string());
    }
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// Reveals the folder for a path already recorded as some job's own `target`.
#[tauri::command(async)]
pub(crate) fn open_bg_op_folder(path: String) -> Result<(), String> {
    open_explorer_at(&path)
}

// Set once the main window's confirm dialog answers "quit anyway" to a bg-quit-requested
// prompt, so later quit attempts during shutdown pass without asking again. Never reset.
pub(crate) static QUIT_CONFIRMED: AtomicBool = AtomicBool::new(false);

// The app's one real shutdown sequence: same three steps whether it runs
// straight from CloseRequested (nothing was running) or from
// confirm_quit_with_running_ops (the user chose to quit anyway).
pub(crate) fn do_full_exit(handle: &AppHandle) {
    if let Some(state) = handle.try_state::<Db>() {
        let store = state.lock().unwrap_or_else(|e| e.into_inner());
        save_settings(handle, &store.settings);
    }
    #[cfg(windows)]
    clipwatch::stop();
    voice_shutdown(handle);
    handle.exit(0);
}

// The one gate every full-quit path goes through (the main window's close button and the
// tray menu's "Quit"), so a job started from either gets the same "N jobs are still
// running, cancel them and quit?" question. Shows the main window first: the confirm dialog
// lives in its webview, so asking while it is hidden (minimized to tray) would leave the
// question unseen.
pub(crate) fn request_app_quit(app: &AppHandle) {
    // A live recording is lost as an unplayable file when the process just
    // ends, so it counts as a running job and gets the same question.
    let running = quit_blockers(bg_ops_running_count(), recording_active(app) || REC_STARTING.load(Ordering::SeqCst));
    if running > 0 && !QUIT_CONFIRMED.load(Ordering::SeqCst) {
        show_main(app);
        let _ = app.emit_to("main", "bg-quit-requested", running);
    } else {
        do_full_exit(app);
    }
}

pub(crate) fn quit_blockers(jobs: usize, recording: bool) -> usize {
    jobs + usize::from(recording)
}

// Called only after the main window's own dialog asked "N jobs are still
// running, cancel them and quit?" and the user said yes. Cancels every
// cancelable job (backup/import have no cancel of their own: see
// cancel_all_bg_ops) and then quits for real.
#[tauri::command]
pub(crate) async fn confirm_quit_with_running_ops(app: AppHandle) {
    // The take is finished properly first (the encoder writes its index), and
    // only then does the shutdown go on.
    #[cfg(windows)]
    {
        let app2 = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            // A "finish and continue" switch in flight holds the session out of the
            // state for a moment; wait it out before asking for the stop.
            for _ in 0..25 {
                if recording_active(&app2) || !REC_STARTING.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            if recording_active(&app2) {
                if let Ok(take) = stop_recording_now(&app2) {
                    save_take_on_quit(&app2, &take);
                }
            }
        })
        .await;
    }
    cancel_all_bg_ops();
    // The workers only see the cancel flag on their next progress line; give them a
    // moment to stop ffmpeg and remove their staged file before the process ends
    // (the job object kills whatever is still running after that). Async, so the
    // wait never blocks the window's own thread.
    for _ in 0..10 {
        if bg_ops_running_count() == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    QUIT_CONFIRMED.store(true, Ordering::SeqCst);
    // Straight into the shutdown: closing the window instead would only hide it
    // while minimize-to-tray is on, and the app would keep running.
    do_full_exit(&app);
}
