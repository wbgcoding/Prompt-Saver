use super::*;

// ---------- Background operations: one source for the taskbar icon AND
// the running-jobs list in the main window's header ----------
//
// Export, recording conversion, the ffmpeg download, a backup and an import can
// each run for a while with the window sitting in the background. Every one of
// them reports its own state into this small registry instead of touching the
// taskbar icon directly, so two running at once -- or one finishing while
// another starts -- combine into one sane reading instead of the second simply
// overwriting the first's icon state. The same registry backs the header
// popover's list of jobs, so the two views can never disagree about what is
// actually running.
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
    // Feature grouping ("export", "convert", "ffmpeg-download", ...) shown to
    // the frontend for its label/icon/cancel-command lookup. Distinct from the
    // registry's own key (see bg_ops() below), which is unique PER JOB so a
    // batch of same-kind jobs (several exports at once) each get their own
    // row instead of overwriting one shared "export" entry.
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

// What the frontend actually reads (`list_bg_ops`) -- OpPhase itself has no
// serde impl, so this is the flattened, JSON-friendly shape. `id` is the
// registry's own unique key (see bg_ops() below); `kind` is the feature
// grouping ("export", "convert", "ffmpeg-download", ...) the frontend already
// keys its label/cancel-command lookup on.
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

// Pure and window-free on purpose, so the state machine (several operations at
// once, one erroring, all finishing) can be unit-tested without a real HWND.
// A finished job stays in the map for the popover but never affects the icon:
// any error still running wins outright; otherwise no active job is "off";
// otherwise any indeterminate operation wins over a plain percentage;
// otherwise the shown percentage is the average across every running operation.
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

// The one place `bg_ops()` is ever written. Deliberately free of any
// AppHandle/window dependency: an export's own progress closure calls this
// directly (see export_video_blocking's caller), so a job keeps reporting
// correctly even once the window that started it is gone -- proved with no
// window at all by export_progress_survives_a_closed_window below.
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
            let entry = ops.entry(id.to_string()).or_insert_with(|| BgOp::fresh(kind));
            entry.phase = OpPhase::Indeterminate;
            entry.finished_at_ms = None;
            if target.is_some() {
                entry.target = target;
            }
        }
        _ => {
            let entry = ops.entry(id.to_string()).or_insert_with(|| BgOp::fresh(kind));
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

#[cfg(windows)]
pub(crate) fn apply_taskbar_state(app: &AppHandle, state: &str, value: Option<u8>) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList, TBPF_ERROR, TBPF_INDETERMINATE, TBPF_NOPROGRESS, TBPF_NORMAL};
    let Some(win) = app.get_webview_window("main") else { return };
    let Ok(hwnd) = win.hwnd() else { return };
    let hwnd = HWND(hwnd.0);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(taskbar): windows::core::Result<ITaskbarList3> = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) else {
            return;
        };
        match state {
            "off" => {
                let _ = taskbar.SetProgressState(hwnd, TBPF_NOPROGRESS);
            }
            "indeterminate" => {
                let _ = taskbar.SetProgressState(hwnd, TBPF_INDETERMINATE);
            }
            "error" => {
                let _ = taskbar.SetProgressState(hwnd, TBPF_ERROR);
            }
            _ => {
                let _ = taskbar.SetProgressValue(hwnd, value.unwrap_or(0) as u64, 100);
                let _ = taskbar.SetProgressState(hwnd, TBPF_NORMAL);
            }
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn apply_taskbar_state(_app: &AppHandle, _state: &str, _value: Option<u8>) {}

// `op` names which feature is reporting ("export", "convert", "ffmpeg-download",
// "backup", "import"); `state` is "running" (with a 0-100 `value`),
// "indeterminate", "error", or "done" (marks this operation's own entry done,
// kept for bgDoneKeepMin minutes rather than dropped right away). `target` is
// an optional destination file/folder for the header popover's "open folder"
// button; omitted or None leaves a previously reported target unchanged.
// Every caller is expected to eventually send "done" or "error" even on its
// own cancel/failure path -- a taskbar icon stuck mid-progress is worse than
// none. The registry itself is updated regardless of the taskbarProgress
// switch below -- that switch only controls the Windows icon, never whether
// the header's own running-jobs list sees the job.
//
// This is deliberately just a thin AppHandle-holder around push_bg_op, kept
// as its own function so the frontend still has a single, stable command
// name to invoke -- the internal Rust progress loops below (export_video,
// convert_recording, download_ffmpeg) call push_bg_op directly instead of
// going through this command, so their own reporting survives a closed window.
#[tauri::command]
pub(crate) fn set_taskbar_progress(app: AppHandle, op: String, state: String, value: Option<u32>, target: Option<String>) {
    // This command's callers (backup, import, ...) only ever run one instance
    // at a time, so the job's id and its kind are the same string -- unlike
    // export_video's own push_bg_op calls below, which give a batch of exports
    // one id each while keeping "export" as their shared kind.
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
    // One shared broadcast point for every state change (running tick, done,
    // error) -- the main window's header popover listens for this instead of
    // polling, so a job finishing is reflected there even when the window that
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
    if enabled {
        apply_taskbar_state(app, agg_state, agg_value);
    } else {
        apply_taskbar_state(app, "off", None);
    }
}

#[tauri::command(async)]
pub(crate) fn list_bg_ops(state_db: State<Db>) -> Vec<BgOpInfo> {
    list_bg_ops_impl(bg_done_keep_min(&state_db))
}

// Cancels every cancelable kind of running job unconditionally -- each of the
// three is a cheap no-op when its own job is not actually running. Backup and
// import have no cancel primitive of their own (interrupting a database write
// mid-flight is the genuinely risky case), so quitting with one of those still
// running is gated by asking first (see the CloseRequested handler below),
// never by trying to abort it here.
pub(crate) fn cancel_all_bg_ops() {
    cancel_all_exports();
    cancel_recording_convert();
    cancel_ffmpeg_download();
    cancel_ocr_index_backfill();
}

// Opens `path` itself if it is a folder, otherwise its parent -- shared by
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
#[tauri::command]
pub(crate) fn open_bg_op_folder(path: String) -> Result<(), String> {
    open_explorer_at(&path)
}

// Set once the main window's own confirm dialog answers "quit anyway" to a
// bg-quit-requested prompt -- the close() that follows re-enters
// CloseRequested a second time, and this flag lets that second pass through
// instead of asking again. Never reset: a fresh app run starts at false.
pub(crate) static QUIT_CONFIRMED: AtomicBool = AtomicBool::new(false);

// The app's one real shutdown sequence -- same three steps whether it runs
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

// The one gate every full-quit path goes through -- the main window's close
// button AND the tray menu's "Quit", so a job started from either entry point
// gets the same "N jobs are still running, cancel them and quit?" question
// instead of the tray simply skipping it. Shows the main window first: the
// confirm dialog lives in its webview, so asking while it is still hidden
// (minimized to tray) would leave the question unseen.
pub(crate) fn request_app_quit(app: &AppHandle) {
    let running = bg_ops_running_count();
    if running > 0 && !QUIT_CONFIRMED.load(Ordering::SeqCst) {
        show_main(app);
        let _ = app.emit_to("main", "bg-quit-requested", running);
    } else {
        do_full_exit(app);
    }
}

// Called only after the main window's own dialog asked "N jobs are still
// running, cancel them and quit?" and the user said yes. Cancels every
// cancelable job (backup/import have no cancel of their own -- see
// cancel_all_bg_ops) and closes the main window again, this time for real.
#[tauri::command]
pub(crate) async fn confirm_quit_with_running_ops(app: AppHandle) {
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
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.close();
    }
}
