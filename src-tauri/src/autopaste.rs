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
    fn GetAsyncKeyState(vk: i32) -> i16;
}
type WinEventProc = unsafe extern "system" fn(isize, u32, isize, i32, i32, u32, u32);

const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const VK_CONTROL: u8 = 0x11;
const VK_RETURN: u8 = 0x0D;
const VK_V: u8 = 0x56;
const VK_SHIFT: i32 = 0x10;
const VK_CONTROL_KEY: i32 = 0x11;
const VK_MENU: i32 = 0x12;
const VK_LWIN: i32 = 0x5B;
const VK_RWIN: i32 = 0x5C;
// How long the paste waits for the hotkey's own modifiers to come up, and how often it looks.
const MODIFIER_WAIT_POLLS: u32 = 50;
const MODIFIER_POLL_MS: u64 = 10;
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
    // The hotkey that triggered the paste usually still has its modifiers down. Sent
    // on top of them, Ctrl+V reaches the target as Ctrl+Alt+V or Ctrl+Shift+V, and the
    // closing Ctrl-up releases the physical key. Pasted anyway after the bounded wait.
    paste_after_wait(
        hwnd,
        || unsafe { GetForegroundWindow() },
        || {
            wait_for_modifiers_up(
                |vk| unsafe { GetAsyncKeyState(vk) } < 0, // high bit: physically down now
                || std::thread::sleep(Duration::from_millis(MODIFIER_POLL_MS)),
            );
        },
        || unsafe {
            keybd_event(VK_CONTROL, 0, 0, 0);
            keybd_event(VK_V, 0, 0, 0);
            keybd_event(VK_V, 0, KEYEVENTF_KEYUP, 0);
            keybd_event(VK_CONTROL, 0, KEYEVENTF_KEYUP, 0);
            if send_enter {
                std::thread::sleep(Duration::from_millis(20));
                keybd_event(VK_RETURN, 0, 0, 0);
                keybd_event(VK_RETURN, 0, KEYEVENTF_KEYUP, 0);
            }
        },
    )
}

// The wait for the modifiers can take up to half a second, long enough for the user
// to switch windows: the keystrokes are only sent when the target is still in front
// after it, never to whatever window took over meanwhile.
fn paste_after_wait(target: isize, foreground: impl Fn() -> isize, wait: impl FnOnce(), send: impl FnOnce()) -> Result<(), String> {
    wait();
    if foreground() != target {
        return Err("focus-failed".into());
    }
    send();
    Ok(())
}

// Polls until Ctrl, Alt, Shift and Win are all up, at most MODIFIER_WAIT_POLLS looks.
// Returns whether they came up in time.
fn wait_for_modifiers_up(is_down: impl Fn(i32) -> bool, pause: impl Fn()) -> bool {
    let any_down = || [VK_CONTROL_KEY, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN].iter().any(|&vk| is_down(vk));
    for _ in 0..MODIFIER_WAIT_POLLS {
        if !any_down() {
            return true;
        }
        pause();
    }
    !any_down()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn the_keys_are_only_sent_while_the_target_is_still_in_front_after_the_wait() {
        let sent = Cell::new(false);
        // Focus moved on while the modifiers were being waited out.
        let fg = Cell::new(7isize);
        let r = paste_after_wait(7, || fg.get(), || fg.set(9), || sent.set(true));
        assert_eq!(r, Err("focus-failed".to_string()));
        assert!(!sent.get());
        // Still in front: sent.
        fg.set(7);
        assert!(paste_after_wait(7, || fg.get(), || {}, || sent.set(true)).is_ok());
        assert!(sent.get());
    }

    #[test]
    fn the_paste_waits_for_held_modifiers_and_goes_ahead_after_the_deadline() {
        let polls = Cell::new(0u32);
        // Alt is released after three looks.
        let released = wait_for_modifiers_up(|vk| vk == VK_MENU && polls.get() < 3, || polls.set(polls.get() + 1));
        assert!(released);
        assert_eq!(polls.get(), 3);
        // Ctrl never comes up: the wait is bounded.
        let polls = Cell::new(0u32);
        let released = wait_for_modifiers_up(|vk| vk == VK_CONTROL_KEY, || polls.set(polls.get() + 1));
        assert!(!released);
        assert_eq!(polls.get(), MODIFIER_WAIT_POLLS);
        // Nothing held: no wait at all.
        let polls = Cell::new(0u32);
        assert!(wait_for_modifiers_up(|_| false, || polls.set(polls.get() + 1)));
        assert_eq!(polls.get(), 0);
    }
}
