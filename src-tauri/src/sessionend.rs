use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
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

unsafe extern "system" fn wndproc(hwnd: isize, msg: u32, w: usize, l: isize) -> isize {
    if msg == WM_QUERYENDSESSION || msg == WM_ENDSESSION {
        // A panic must not unwind across this extern "system" boundary (→ abort).
        let _ = std::panic::catch_unwind(|| {
            let app = APP.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(app) = app {
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
    // SetWindowLongPtrW wants the callback's address as a plain isize -- that is
    // the whole point of this cast, not an accident.
    #[allow(clippy::fn_to_numeric_cast)]
    let ours = wndproc as WndProc as isize;
    let prev = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, ours) };
    // A failed swap returns 0 and leaves the original in place: nothing to undo.
    PREV_PROC.store(prev, Ordering::Release);
}
