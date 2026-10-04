use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Duration;

static LAST_FG: AtomicIsize = AtomicIsize::new(0);

#[link(name = "user32")]
extern "system" {
    fn SetWinEventHook(min: u32, max: u32, hmod: isize, cb: WinEventProc, pid: u32, tid: u32, flags: u32) -> isize;
    fn GetForegroundWindow() -> isize;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
    fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
}
type WinEventProc = unsafe extern "system" fn(isize, u32, isize, i32, i32, u32, u32);

const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const VK_CONTROL: u8 = 0x11;
const VK_RETURN: u8 = 0x0D;
const VK_V: u8 = 0x56;
const OBJID_WINDOW: i32 = 0;

unsafe extern "system" fn on_foreground(_h: isize, _e: u32, hwnd: isize, id_object: i32, _c: i32, _t: u32, _ms: u32) {
    if id_object == OBJID_WINDOW && hwnd != 0 {
        LAST_FG.store(hwnd, Ordering::Relaxed);
    }
}

// Install on the calling (main/UI) thread; its OUTOFCONTEXT callback is delivered
// by that thread's message loop, which tao already pumps. Handle is leaked for
// the app lifetime (unhooked automatically at process exit).
pub fn install_hook() {
    unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, 0,
            on_foreground, 0, 0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
    }
}

// Bring the stored external window to the foreground; returns its handle so the
// caller can wait out the settle delay before sending the keystrokes.
pub fn begin_paste() -> Result<isize, String> {
    let hwnd = LAST_FG.load(Ordering::Relaxed);
    if hwnd == 0 || unsafe { IsWindow(hwnd) } == 0 {
        return Err("no-target".into());
    }
    unsafe { SetForegroundWindow(hwnd); }
    Ok(hwnd)
}

// After the delay: verify the target really took focus (elevated/UIPI windows
// refuse it), then synthesize Ctrl+V and, optionally, Enter.
pub fn finish_paste(hwnd: isize, send_enter: bool) -> Result<(), String> {
    if unsafe { GetForegroundWindow() } != hwnd {
        return Err("focus-failed".into());
    }
    unsafe {
        keybd_event(VK_CONTROL, 0, 0, 0);
        keybd_event(VK_V, 0, 0, 0);
        keybd_event(VK_V, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_CONTROL, 0, KEYEVENTF_KEYUP, 0);
        if send_enter {
            std::thread::sleep(Duration::from_millis(20));
            keybd_event(VK_RETURN, 0, 0, 0);
            keybd_event(VK_RETURN, 0, KEYEVENTF_KEYUP, 0);
        }
    }
    Ok(())
}
