use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;
use tauri::AppHandle;

const GWLP_WNDPROC: i32 = -4;
const WM_QUERYENDSESSION: u32 = 0x0011;
const WM_ENDSESSION: u32 = 0x0016;

type WndProc = unsafe extern "system" fn(isize, u32, usize, isize) -> isize;

#[link(name = "user32")]
extern "system" {
    fn SetWindowLongPtrW(hwnd: isize, index: i32, new: isize) -> isize;
    fn CallWindowProcW(prev: isize, hwnd: isize, msg: u32, w: usize, l: isize) -> isize;
}

// The procedure we displaced; also the "already chained" marker.
static PREV_PROC: AtomicIsize = AtomicIsize::new(0);
static APP: Mutex<Option<AppHandle>> = Mutex::new(None);

// How long the session-ending message waits for a running take to be closed. Windows
// gives the app only a few seconds before it is terminated; the encoder's index is
// written first inside the stop, so even a stop that outlives the wait has the part
// that matters behind it.
const SHUTDOWN_STOP_WAIT: Duration = Duration::from_secs(3);
static STOP_STARTED: AtomicBool = AtomicBool::new(false);
static STOP_DONE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

// What a session message asks of us. WM_QUERYENDSESSION only asks whether the session may end
// (another app can still veto it), so a running take must not be stopped for it. Only a
// WM_ENDSESSION with a non-zero wParam means the session really ends; a zero wParam is the
// shutdown being cancelled.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum SessionAction {
    None,
    FlushOnly,
    StopAndFlush,
}

pub(crate) fn session_action(msg: u32, w: usize) -> SessionAction {
    match msg {
        WM_QUERYENDSESSION => SessionAction::FlushOnly,
        WM_ENDSESSION if w != 0 => SessionAction::StopAndFlush,
        _ => SessionAction::None,
    }
}

// The stop runs on a worker thread (the window thread must not sit in an encoder flush);
// a repeated call only waits for it again.
#[cfg(windows)]
fn finish_recording(app: &AppHandle) {
    let starting = !STOP_STARTED.load(Ordering::Acquire);
    if starting {
        if !super::recording_active(app) {
            return;
        }
        STOP_STARTED.store(true, Ordering::Release);
        *STOP_DONE.0.lock().unwrap_or_else(|e| e.into_inner()) = false;
        let app = app.clone();
        std::thread::spawn(move || {
            if let Ok(take) = super::stop_recording_now(&app) {
                super::save_take_on_quit(&app, &take);
            }
            *STOP_DONE.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
            STOP_STARTED.store(false, Ordering::Release);
            STOP_DONE.1.notify_all();
        });
    }
    let done = STOP_DONE.0.lock().unwrap_or_else(|e| e.into_inner());
    let _ = STOP_DONE.1.wait_timeout_while(done, SHUTDOWN_STOP_WAIT, |done| !*done);
}

unsafe extern "system" fn wndproc(hwnd: isize, msg: u32, w: usize, l: isize) -> isize {
    let action = session_action(msg, w);
    if action != SessionAction::None {
        // A panic must not unwind across this extern "system" boundary (→ abort).
        let _ = std::panic::catch_unwind(|| {
            let app = APP.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(app) = app {
                #[cfg(windows)]
                {
                    if action == SessionAction::StopAndFlush {
                        finish_recording(&app);
                    }
                }
                super::flush_settings_now(&app);
            }
        });
    }
    let prev = PREV_PROC.load(Ordering::Acquire);
    if prev == 0 {
        return 0;
    }
    // Everything else, including the session messages, is answered by the
    // original procedure: this hook only ever adds the flush.
    CallWindowProcW(prev, hwnd, msg, w, l)
}

// Chains onto the main window once; a second call is a no-op. Called from the
// thread that owns the window, so no message can be dispatched between the
// swap and PREV_PROC being published.
pub fn hook(app: &AppHandle, hwnd: isize) {
    if hwnd == 0 || PREV_PROC.load(Ordering::Acquire) != 0 {
        return;
    }
    *APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app.clone());
    // SetWindowLongPtrW wants the callback's address as a plain isize: that is
    // the whole point of this cast, not an accident.
    #[allow(clippy::fn_to_numeric_cast)]
    let ours = wndproc as WndProc as isize;
    let prev = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, ours) };
    // A failed swap returns 0 and leaves the original in place: nothing to undo.
    PREV_PROC.store(prev, Ordering::Release);
}
