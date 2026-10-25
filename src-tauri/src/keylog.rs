use super::append_crash_log;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc::{channel, sync_channel, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows::core::{implement, Interface};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement, IUIAutomationFocusChangedEventHandler,
    IUIAutomationFocusChangedEventHandler_Impl, IUIAutomationTextPattern, IUIAutomationValuePattern, UIA_CONTROLTYPE_ID,
    UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId, UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_HyperlinkControlTypeId,
    UIA_ImageControlTypeId, UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId, UIA_RadioButtonControlTypeId, UIA_ScrollBarControlTypeId,
    UIA_SliderControlTypeId, UIA_TabItemControlTypeId, UIA_TextPatternId, UIA_ThumbControlTypeId, UIA_ToolBarControlTypeId,
    UIA_TreeItemControlTypeId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetGUIThreadInfo, GetMessageW, GetWindowLongW, GetWindowThreadProcessId,
    PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, ES_PASSWORD, GUITHREADINFO, GWL_STYLE, KBDLLHOOKSTRUCT,
    LLKHF_EXTENDED, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

// Upper bound on one UI Automation call. A hung target app would otherwise stall the
// consumer thread, and with it stop(), which joins that thread.
const UIA_TIMEOUT_MS: u32 = 2000;

static CLOCK_START: OnceLock<Instant> = OnceLock::new();

// Monotonic milliseconds clock, immune to wall-clock jumps. Every keylog timestamp,
// including the instants callers pass in, must come from this clock; mixing in now_ms()
// corrupts every comparison.
pub fn mono_ms() -> u64 {
    CLOCK_START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct KeyEvent {
    pub vk: u32,
    pub down: bool,
    pub t_ms: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Gap {
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct KeySidecar {
    pub events: Vec<KeyEvent>,
    pub gaps: Vec<Gap>,
    // Hold time configured when the recording started. Defaults for older sidecars.
    #[serde(default = "default_hold_ms")]
    pub hold_ms: u32,
}

fn default_hold_ms() -> u32 {
    HOLD_MS
}

// Ceiling on events per sidecar part. Past it, a gap replaces further events so the
// overlay still shows that something happened.
const EVENTS_CAP: usize = 200_000;

// Pure state machine turning raw (vk, scan, down, t_ms, blocked) into the stored sidecar
// shape, free of Windows APIs so it is unit-testable. A blocked key (password field
// focused, or synthetic input) becomes a gap, never an event.
#[derive(Default)]
struct Recorder {
    sidecar: KeySidecar,
    in_gap: bool,
    // Whether the latest down of each (vk, scan) was accepted. An up is stored only for a
    // down accepted here; unknown or tainted pairs are treated as blocked. Removed on the
    // matching up.
    open_downs: std::collections::HashMap<(u32, u32), bool>,
}

// Defensive ceiling on open_downs against a flood of distinct keys.
const OPEN_DOWNS_CAP: usize = 4_096;

impl Recorder {
    fn record(&mut self, vk: u32, scan: u32, down: bool, t_ms: u64, blocked: bool) {
        let final_blocked = if down {
            if blocked {
                // A blocked down is removed, not stored; a missing entry already reads as tainted.
                self.open_downs.remove(&(vk, scan));
            } else {
                self.open_downs.insert((vk, scan), true);
                if self.open_downs.len() > OPEN_DOWNS_CAP {
                    self.open_downs.clear();
                }
            }
            blocked
        } else {
            let down_accepted = self.open_downs.remove(&(vk, scan)).unwrap_or(false);
            blocked || !down_accepted
        };
        // At the event cap every further key extends the open gap instead of adding an event.
        let at_cap = self.sidecar.events.len() >= EVENTS_CAP;
        if final_blocked || at_cap {
            if self.in_gap {
                if let Some(last) = self.sidecar.gaps.last_mut() {
                    last.end_ms = t_ms;
                }
            } else {
                self.sidecar.gaps.push(Gap { start_ms: t_ms, end_ms: t_ms });
                self.in_gap = true;
            }
        } else {
            self.in_gap = false;
            self.sidecar.events.push(KeyEvent { vk, down, t_ms });
        }
    }
}

// Pause tracking on the monotonic clock. A key struck while paused is dropped outright,
// which is stronger than a gap (focus state unknown).
#[derive(Default)]
struct ActiveClock {
    start_abs_ms: u64,
    // Paused spans in monotonic ms; an open span has a None end.
    pauses: Vec<(u64, Option<u64>)>,
}

impl ActiveClock {
    fn new(start_abs_ms: u64) -> Self {
        Self { start_abs_ms, pauses: Vec::new() }
    }

    fn pause(&mut self, now: u64) {
        if !matches!(self.pauses.last(), Some((_, None))) {
            self.pauses.push((now, None));
        }
    }

    fn resume(&mut self, now: u64) {
        if let Some(last) = self.pauses.last_mut() {
            if last.1.is_none() {
                last.1 = Some(now);
            }
        }
    }

    // Judged by the key's own timestamp, not the current state, so a key queued before a
    // pause is not dropped when the consumer is late.
    fn is_paused_at(&self, t_ms: u64) -> bool {
        self.pauses.iter().any(|(s, e)| t_ms >= *s && e.is_none_or(|e| t_ms < e))
    }

    // Active elapsed ms at t_ms with paused spans removed, the same basis as the video track.
    fn active_ms(&self, t_ms: u64) -> u64 {
        let mut paused = 0u64;
        for (s, e) in &self.pauses {
            if *s >= t_ms {
                continue;
            }
            let end = e.unwrap_or(t_ms).min(t_ms);
            paused += end.saturating_sub(*s);
        }
        t_ms.saturating_sub(self.start_abs_ms).saturating_sub(paused)
    }
}

// SETTLE_MS: a focus check must have started this long after a key to speak for it.
// GUARD_MS: nothing is trusted this close to a focus-change arrival.
const SETTLE_MS: u64 = 300;
const GUARD_MS: u64 = 300;

// Focus identity poll interval while keys are pending, to catch focus changes that UI
// Automation did not report.
const FOCUS_POLL_MS: u64 = 50;

// Cap on raw keys waiting for a verdict. Reaching it means verdicts are not keeping up,
// so everything is gapped.
const PENDING_CAP: usize = 2_000;

// Idle wake-up interval for pruning the ledger.
const IDLE_PRUNE_MS: u64 = 1_000;

// What the focused control was when a check ran. Password is the strictest tier. Text
// needs a positive text-entry identification (Edit/Document type, editable ValuePattern
// or any TextPattern). NonText needs a positive non-text classification (see
// is_non_text_control_type) with no sign of editable text. Unknown is everything else:
// no element, a failed read or an unrecognized type. Text and NonText resolve to the same
// verdict; Password and Unknown gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldClass {
    Password,
    Text,
    NonText,
    Unknown,
}

// A resolved key's fate. Gap is unconditional; Allow covers Text and NonText and is still
// subject to the opt-in shortcuts-only filter in resolve_verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Gap,
    Allow,
}

// Ordered log of completed focus checks (stamped with their start) and focus-change
// arrivals, both on the mono_ms() clock. Keyed by time, not HWND: Chromium, Electron,
// WebView2, WPF and UWP host a whole page in one HWND.
#[derive(Default)]
struct Ledger {
    checks: Vec<(u64, FieldClass)>, // (start_ms, class)
    arrivals: Vec<u64>,
    // Set when the focus-changed handler could not be registered: nothing can prove a key
    // clean, so everything gaps. See gap_everything.
    fail_closed: bool,
}

// Hard ceiling on ledger size on top of GUARD_MS pruning. Reaching it gaps everything and
// drops the history.
const LEDGER_HARD_CAP: usize = 20_000;

impl Ledger {
    fn push_check(&mut self, start_ms: u64, class: FieldClass) {
        let pos = self.checks.partition_point(|(t, _)| *t <= start_ms);
        self.checks.insert(pos, (start_ms, class));
        self.enforce_cap();
    }

    fn push_arrival(&mut self, arrival_ms: u64) {
        let pos = self.arrivals.partition_point(|t| *t <= arrival_ms);
        self.arrivals.insert(pos, arrival_ms);
        self.enforce_cap();
    }

    // Called when registering the focus-changed handler failed. Every key gaps from here on.
    fn gap_everything(&mut self) {
        self.fail_closed = true;
    }

    // Checked on every push, from both threads that add to the ledger, so a flood of focus
    // changes is caught even with no keys pending. After fail_closed nothing is read again,
    // so dropping the history is safe.
    fn enforce_cap(&mut self) {
        if self.checks.len() + self.arrivals.len() > LEDGER_HARD_CAP {
            self.gap_everything();
            self.checks.clear();
            self.checks.shrink_to_fit();
            self.arrivals.clear();
            self.arrivals.shrink_to_fit();
        }
    }

    // Earliest completed check that started at least SETTLE_MS after t (binary search, as
    // checks stay sorted by start). None means the caller must keep waiting.
    fn qualifying_check(&self, t: u64) -> Option<(u64, FieldClass)> {
        let need = t.saturating_add(SETTLE_MS);
        let idx = self.checks.partition_point(|(start, _)| *start < need);
        self.checks.get(idx).copied()
    }

    // Drops checks and arrivals older than the oldest pending key's GUARD_MS window, so
    // verdict scans stay short.
    fn prune(&mut self, cutoff_ms: u64) {
        let ci = self.checks.partition_point(|(t, _)| *t < cutoff_ms);
        self.checks.drain(0..ci);
        let ai = self.arrivals.partition_point(|t| *t < cutoff_ms);
        self.arrivals.drain(0..ai);
    }

    // Whether a Password or Unknown check, or a focus-change arrival, lies in [from, to]. The
    // qualifying check is a single point and cannot see a change right next to it.
    fn window_is_dirty(&self, from: u64, to: u64) -> bool {
        let bad_check = self
            .checks
            .iter()
            .any(|(t, class)| matches!(class, FieldClass::Password | FieldClass::Unknown) && *t >= from && *t <= to);
        let bad_arrival = self.arrivals.iter().any(|t| *t >= from && *t <= to);
        bad_check || bad_arrival
    }

    // Blind window around a focus-change arrival: GUARD_MS before through SETTLE_MS after,
    // unconditional. UI Automation can lag the real focus change (Chromium, Electron and
    // WebView2 update their accessibility trees asynchronously).
    fn in_blind_window(&self, t: u64) -> bool {
        self.arrivals.iter().any(|a| t >= a.saturating_sub(GUARD_MS) && t <= a.saturating_add(SETTLE_MS))
    }
}

// The binding verdict. Gap is unconditional; Allow is subject to the shortcuts-only
// filter; None means not enough information yet, so the key stays pending (verdict_at_stop
// gaps it at the end). Unknown gaps like Password.
fn verdict(ledger: &Ledger, t: u64, injected: bool) -> Option<Verdict> {
    if injected || ledger.fail_closed {
        return Some(Verdict::Gap);
    }
    if ledger.in_blind_window(t) {
        return Some(Verdict::Gap);
    }
    let (check_time, class) = ledger.qualifying_check(t)?;
    if class == FieldClass::Password || class == FieldClass::Unknown {
        return Some(Verdict::Gap);
    }
    if ledger.window_is_dirty(t.saturating_sub(GUARD_MS), check_time) {
        return Some(Verdict::Gap);
    }
    Some(match class {
        FieldClass::NonText | FieldClass::Text => Verdict::Allow,
        FieldClass::Password | FieldClass::Unknown => unreachable!("handled above"),
    })
}

// A key's VK, scan code and extended flag plus its modifier state, bundled to keep
// argument counts down.
#[derive(Clone, Copy)]
struct KeyId {
    vk: u32,
    scan: u32,
    extended: bool,
    ctrl: bool,
    alt: bool,
    win: bool,
}

// Turns a Verdict into the gap boolean for Recorder::record. An Allow is still gapped when
// keyOverlayShortcutsOnly is on and the key is not a shortcut or navigation key.
fn resolve_verdict(v: Verdict, shortcuts_only: bool, id: KeyId) -> bool {
    match v {
        Verdict::Gap => true,
        Verdict::Allow => shortcuts_only && !is_kept_by_shortcuts_only(id.vk, id.scan, id.extended, id.ctrl, id.alt, id.win),
    }
}

// Used once the capture is stopping: a key still undecided is gapped.
fn verdict_at_stop(ledger: &Ledger, t: u64, injected: bool, id: KeyId, shortcuts_only: bool) -> bool {
    match verdict(ledger, t, injected) {
        Some(v) => resolve_verdict(v, shortcuts_only, id),
        None => true,
    }
}

// Everything at or after the part-switch instant belongs to the next part.
fn past_cutoff(t_ms: u64, cutoff: Option<u64>) -> bool {
    cutoff.is_some_and(|c| t_ms >= c)
}

struct RawKey {
    vk: u32,
    // Scan code alongside the VK: left/right Ctrl, Shift and Alt share a VK, so down/up
    // matching uses the pair.
    scan: u32,
    // LLKHF_EXTENDED: set for the nav cluster and right-hand Ctrl/Alt, clear for the numpad.
    // With the scan code it tells a numpad key (NumLock off) from the real nav cluster.
    extended: bool,
    down: bool,
    t_ms: u64,
    // LLKHF_INJECTED: synthetic SendInput (password managers, remote control). Always gapped.
    injected: bool,
    // Modifiers held at this key's own timestamp; used only by the shortcuts-only filter.
    ctrl: bool,
    alt: bool,
    win: bool,
    // Set by the consumer's focus poll when the key falls in a span it cannot vouch for (a
    // focus change, or the poll ran late). Gaps unconditionally.
    forced_gap: bool,
}

// Tab, Enter and Esc often move focus without an immediate UI Automation event, so the
// consumer counts their key-down as a focus arrival to cover the blind window.
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;

fn is_focus_moving_key(vk: u32) -> bool {
    matches!(vk, VK_TAB | VK_RETURN | VK_ESCAPE)
}

// Non-character keys the shortcuts-only filter keeps regardless of modifier (cursor
// movement, deleting, paging, F-keys).
const VK_BACK: u32 = 0x08;
const VK_DELETE: u32 = 0x2E;
const VK_INSERT: u32 = 0x2D;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_HOME: u32 = 0x24;
const VK_END: u32 = 0x23;
const VK_PRIOR: u32 = 0x21; // Page Up
const VK_NEXT: u32 = 0x22; // Page Down
const VK_F1: u32 = 0x70;
const VK_F24: u32 = 0x87; // VK_F1..=VK_F24 is contiguous in Windows' VK space

fn is_nav_or_edit_key(vk: u32) -> bool {
    matches!(vk, VK_RETURN | VK_TAB | VK_ESCAPE | VK_BACK | VK_DELETE | VK_INSERT | VK_LEFT | VK_UP | VK_RIGHT | VK_DOWN | VK_HOME | VK_END | VK_PRIOR | VK_NEXT)
        || (VK_F1..=VK_F24).contains(&vk)
}

// Modifier VKs (generic and left/right), used for live modifier state and so a bare
// modifier press never counts as a combo.
const VK_CONTROL: u32 = 0x11;
const VK_LCONTROL: u32 = 0xA2;
const VK_RCONTROL: u32 = 0xA3;
const VK_MENU: u32 = 0x12;
const VK_LMENU: u32 = 0xA4;
const VK_RMENU: u32 = 0xA5;
const VK_LWIN: u32 = 0x5B;
const VK_RWIN: u32 = 0x5C;
const VK_SHIFT: u32 = 0x10;
const VK_LSHIFT: u32 = 0xA0;
const VK_RSHIFT: u32 = 0xA1;

fn is_ctrl_vk(vk: u32) -> bool {
    matches!(vk, VK_CONTROL | VK_LCONTROL | VK_RCONTROL)
}

fn is_alt_vk(vk: u32) -> bool {
    matches!(vk, VK_MENU | VK_LMENU | VK_RMENU)
}

fn is_win_vk(vk: u32) -> bool {
    matches!(vk, VK_LWIN | VK_RWIN)
}

fn is_shift_vk(vk: u32) -> bool {
    matches!(vk, VK_SHIFT | VK_LSHIFT | VK_RSHIFT)
}

fn is_modifier_vk(vk: u32) -> bool {
    is_ctrl_vk(vk) || is_alt_vk(vk) || is_win_vk(vk) || is_shift_vk(vk)
}

// With NumLock on the numpad sends VK_NUMPAD0..VK_DIVIDE; with it off it sends nav-cluster
// VKs but keeps a non-extended scan code (the real nav cluster is extended). So scan plus
// the extended flag identifies a numpad key, not the VK alone.
const VK_NUMPAD0: u32 = 0x60;
const VK_DIVIDE: u32 = 0x6F; // VK_NUMPAD0..=VK_DIVIDE is contiguous
const NUMPAD_SCAN_MIN: u32 = 0x47;
const NUMPAD_SCAN_MAX: u32 = 0x53; // inclusive of 0x4E, Numpad +

fn is_numpad_scan(scan: u32, extended: bool) -> bool {
    !extended && (NUMPAD_SCAN_MIN..=NUMPAD_SCAN_MAX).contains(&scan)
}

fn is_numpad_vk(vk: u32) -> bool {
    (VK_NUMPAD0..=VK_DIVIDE).contains(&vk)
}

// Whether Alt plus this key may be a step of Alt+Numpad character entry (decimal or hex).
// Matches the VK (NumLock on) or the scan code (NumLock off).
fn is_alt_numpad_entry(vk: u32, scan: u32, extended: bool) -> bool {
    is_numpad_vk(vk) || is_numpad_scan(scan, extended)
}

// The keyOverlayShortcutsOnly allow-list: navigation keys, or a non-modifier key with
// Ctrl, Alt or Win. Bare Ctrl+Alt is AltGr and counts as typing. Alt over a numpad key
// composes a character, so it is checked before the nav-key allow (they share VKs).
// Letters, digits, punctuation and Shift+char are dropped.
fn is_kept_by_shortcuts_only(vk: u32, scan: u32, extended: bool, ctrl: bool, alt: bool, win: bool) -> bool {
    if alt && !ctrl && !win && is_alt_numpad_entry(vk, scan, extended) {
        return false;
    }
    if is_nav_or_edit_key(vk) {
        return true;
    }
    if is_modifier_vk(vk) {
        return false;
    }
    let altgr = ctrl && alt && !win;
    (ctrl || alt || win) && !altgr
}

// Ctrl/Alt/Win state from the hook thread's serialized events. Reset when the hook is
// reinstalled, since a stale true would fail open.
#[derive(Default, Clone, Copy)]
struct ModifierState {
    ctrl: bool,
    alt: bool,
    win: bool,
}

static MODIFIERS: OnceLock<Mutex<ModifierState>> = OnceLock::new();

fn modifiers_slot() -> &'static Mutex<ModifierState> {
    MODIFIERS.get_or_init(|| Mutex::new(ModifierState::default()))
}

// A synthetic (SendInput) modifier event never updates the tracked state, or a script
// could hold a modifier "on" forever or mask a real modifier's up.
fn should_track_modifier_event(injected: bool) -> bool {
    !injected
}

// The tracked state can go stale (a key-up the hook never saw, e.g. an elevated window or
// the secure desktop during Win+L). A modifier counts as held only if the tracked state
// and GetAsyncKeyState agree; any doubt means not held.
fn confirm_modifier(tracked: bool, async_down: bool) -> bool {
    tracked && async_down
}

// Capacity of the channel between the hook thread and the consumer. The hook uses
// try_send and drops the newest event when full, so it never blocks the desktop's key
// path. A dropped down taints nothing; a dropped up reads as unmatched.
const RAW_CHANNEL_CAP: usize = 4_096;

// The hook callback is a bare extern "system" fn and cannot capture, so its channel is
// process-global. Cloned at once, never held across the send.
static RAW_TX: OnceLock<Mutex<Option<SyncSender<RawKey>>>> = OnceLock::new();

fn raw_tx_slot() -> &'static Mutex<Option<SyncSender<RawKey>>> {
    RAW_TX.get_or_init(|| Mutex::new(None))
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wparam.0 as u32;
        if matches!(msg, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP) {
            let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let injected = (info.flags.0 & LLKHF_INJECTED.0) != 0;
            let extended = (info.flags.0 & LLKHF_EXTENDED.0) != 0;
            let t_ms = mono_ms();
            // Update the shared modifier state before snapshotting it, so this key's own down/up and
            // a companion key arriving right after both see it.
            let (tracked_ctrl, tracked_alt, tracked_win) = {
                let mut state = modifiers_slot().lock().unwrap_or_else(|e| e.into_inner());
                if should_track_modifier_event(injected) {
                    if is_ctrl_vk(info.vkCode) {
                        state.ctrl = down;
                    }
                    if is_alt_vk(info.vkCode) {
                        state.alt = down;
                    }
                    if is_win_vk(info.vkCode) {
                        state.win = down;
                    }
                }
                (state.ctrl, state.alt, state.win)
            };
            // Confirmed against the live keyboard state (see confirm_modifier).
            let ctrl = confirm_modifier(tracked_ctrl, GetAsyncKeyState(VK_CONTROL as i32) < 0);
            let alt = confirm_modifier(tracked_alt, GetAsyncKeyState(VK_MENU as i32) < 0);
            let win = confirm_modifier(tracked_win, GetAsyncKeyState(VK_LWIN as i32) < 0 || GetAsyncKeyState(VK_RWIN as i32) < 0);
            if let Ok(guard) = raw_tx_slot().lock() {
                if let Some(tx) = guard.as_ref() {
                    // Never block here: a full channel drops the newest event.
                    let _ = tx.try_send(RawKey { vk: info.vkCode, scan: info.scanCode, extended, down, t_ms, injected, ctrl, alt, win, forced_gap: false });
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn hook_thread_main(ready_tx: Sender<Result<u32, String>>) {
    unsafe {
        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) {
            Ok(h) => h,
            Err(e) => {
                let _ = ready_tx.send(Err(e.to_string()));
                return;
            }
        };
        let tid = GetCurrentThreadId();
        let _ = ready_tx.send(Ok(tid));
        // WH_KEYBOARD_LL only fires while its installing thread pumps messages; this loop keeps
        // that true and unwinds on the WM_QUIT that stop() posts.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
        let _ = UnhookWindowsHookEx(hook);
    }
}

// The focused HWND: hwndFocus of the foreground thread when it has one, else the
// foreground window.
fn focused_hwnd() -> HWND {
    let fg = unsafe { GetForegroundWindow() };
    let mut gti = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
    let tid = unsafe { GetWindowThreadProcessId(fg, None) };
    if unsafe { GetGUIThreadInfo(tid, &mut gti) }.is_ok() && !gti.hwndFocus.is_invalid() {
        gti.hwndFocus
    } else {
        fg
    }
}

// The focused control's class, resolved fresh on every call (no cache, see Ledger). Used
// by both the UI Automation handler and the consumer's scheduled checks.
fn classify_focused_field(uia: &Option<IUIAutomation>) -> FieldClass {
    classify_identity(&focus_identity(uia))
}

// A focus identity cheap enough to poll every FOCUS_POLL_MS: the focused HWND plus its UI
// Automation element. Compared with CompareElements, a true runtime-id comparison.
fn focus_identity(uia: &Option<IUIAutomation>) -> (HWND, Option<IUIAutomationElement>) {
    let element = uia.as_ref().and_then(|uia| unsafe { uia.GetFocusedElement() }.ok());
    (focused_hwnd(), element)
}

fn focus_identity_changed(
    uia: &Option<IUIAutomation>,
    prev: &(HWND, Option<IUIAutomationElement>),
    cur: &(HWND, Option<IUIAutomationElement>),
) -> bool {
    if prev.0 != cur.0 {
        return true;
    }
    match (&prev.1, &cur.1, uia) {
        (None, None, _) => false,
        (Some(_), None, _) | (None, Some(_), _) => true,
        (Some(a), Some(b), Some(uia)) => !unsafe { uia.CompareElements(a, b) }.map(|same| same.as_bool()).unwrap_or(false),
        // No UI Automation object to compare with; the HWND check above is all there is.
        (Some(_), Some(_), None) => false,
    }
}

// Fails closed: any doubt (UI Automation unreachable, a failed call, no focused window)
// counts as a password field. Takes the element fetched once by the caller.
fn is_password_field(hwnd: HWND, el: Option<&IUIAutomationElement>) -> bool {
    let uia_password = el.and_then(|el| unsafe { el.CurrentIsPassword() }.ok());
    match uia_password {
        None => return true, // UI Automation unreachable: doubt means password.
        Some(b) if b.as_bool() => return true,
        Some(_) => {}
    }
    // Some legacy edit controls never expose Password through UI Automation; the classic
    // style is the second signal.
    if hwnd.0.is_null() {
        return true;
    }
    (unsafe { GetWindowLongW(hwnd, GWL_STYLE) } & ES_PASSWORD) != 0
}

// Whether the focused element is positively ruled out as text entry (the only route to
// FieldClass::NonText). A bare Pane/Window/Custom/Group, an unknown type or a failed read
// falls through, because custom-drawn terminals, remote-session windows and Qt/Java/game
// windows expose no accessibility patterns and must not count as non-text.
fn is_non_text_control_type(control_type: UIA_CONTROLTYPE_ID) -> bool {
    control_type == UIA_ButtonControlTypeId
        || control_type == UIA_CheckBoxControlTypeId
        || control_type == UIA_RadioButtonControlTypeId
        || control_type == UIA_MenuItemControlTypeId
        || control_type == UIA_TabItemControlTypeId
        || control_type == UIA_ListItemControlTypeId
        || control_type == UIA_TreeItemControlTypeId
        || control_type == UIA_SliderControlTypeId
        || control_type == UIA_ScrollBarControlTypeId
        || control_type == UIA_HyperlinkControlTypeId
        || control_type == UIA_ImageControlTypeId
        || control_type == UIA_ToolBarControlTypeId
        || control_type == UIA_ThumbControlTypeId
}

// Classifies a non-password element as Text, NonText or Unknown. Text needs positive
// evidence (Edit/Document type, editable ValuePattern, or any TextPattern). NonText needs
// an allow-listed control type and no pattern proving otherwise. Everything else,
// including doubt, is Unknown and gaps.
fn classify_control(el: Option<&IUIAutomationElement>) -> FieldClass {
    let Some(el) = el else { return FieldClass::Unknown };
    let Ok(control_type) = (unsafe { el.CurrentControlType() }) else {
        return FieldClass::Unknown;
    };
    if control_type == UIA_EditControlTypeId || control_type == UIA_DocumentControlTypeId {
        return FieldClass::Text;
    }
    if let Ok(value_pattern) = unsafe { el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) } {
        match unsafe { value_pattern.CurrentIsReadOnly() } {
            Ok(read_only) if !read_only.as_bool() => return FieldClass::Text, // editable ValuePattern
            Ok(_) => {} // read-only ValuePattern: keep checking
            Err(_) => return FieldClass::Unknown, // failed read: doubt, not a positive finding
        }
    }
    if unsafe { el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }.is_ok() {
        return FieldClass::Text; // any TextPattern: editable text cannot be ruled out
    }
    if is_non_text_control_type(control_type) {
        FieldClass::NonText
    } else {
        FieldClass::Unknown
    }
}

// The full classification of an identity fetched once by the caller, so a focus change
// between two reads cannot hand the checks two different elements. Password takes
// priority; everything else goes through classify_control.
fn classify_identity(identity: &(HWND, Option<IUIAutomationElement>)) -> FieldClass {
    let (hwnd, el) = identity;
    if is_password_field(*hwnd, el.as_ref()) {
        return FieldClass::Password;
    }
    classify_control(el.as_ref())
}

// A check is stamped with the instant it started, but the UI Automation reads that
// follow can block for up to UIA_TIMEOUT_MS. If the focused identity changed between the
// first and last read, the queried elements may belong to the field the user left, so the
// result is forced to Password and the bool tells the caller to push an arrival at the
// check's start.
fn classify_checked(uia: &Option<IUIAutomation>) -> (FieldClass, bool) {
    let before = focus_identity(uia);
    let class = classify_identity(&before);
    let after = focus_identity(uia);
    let changed = focus_identity_changed(uia, &before, &after);
    (if changed { FieldClass::Password } else { class }, changed)
}

// Handler for the UI Automation focus-changed event, fired whenever the focused element
// changes anywhere on the desktop. Each firing records an arrival and re-resolves the
// class into the shared ledger. Nothing is keyed by HWND, so an in-page field switch
// invalidates as reliably as a window switch.
#[implement(IUIAutomationFocusChangedEventHandler)]
struct FocusHandler {
    ledger: Arc<Mutex<Ledger>>,
    uia: IUIAutomation,
}

impl IUIAutomationFocusChangedEventHandler_Impl for FocusHandler_Impl {
    fn HandleFocusChangedEvent(&self, _sender: windows::core::Ref<IUIAutomationElement>) -> windows::core::Result<()> {
        // Record the arrival first: the blind window keys off it, not off when CurrentIsPassword
        // finishes (up to UIA_TIMEOUT_MS). `_sender` is ignored on purpose; re-deriving through
        // GetForegroundWindow/GetGUIThreadInfo is one source of truth and also covers the
        // ES_PASSWORD fallback. The time is read and pushed under the ledger lock so the consumer
        // never sees a later check without this arrival.
        let arrived_at = {
            let mut guard = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
            let now = mono_ms();
            guard.push_arrival(now);
            now
        };
        let class = classify_focused_field(&Some(self.uia.clone()));
        // The check counts as started at the arrival instant.
        self.ledger.lock().unwrap_or_else(|e| e.into_inner()).push_check(arrived_at, class);
        Ok(())
    }
}

// How long the consumer may block in recv_timeout while a key is pending: until the front
// key's settle deadline or the next focus poll, whichever is first. Saturates at 0.
fn recv_wait_ms(now: u64, pending_front_t_ms: u64, next_poll_due: u64) -> u64 {
    pending_front_t_ms.saturating_add(SETTLE_MS).min(next_poll_due).saturating_sub(now)
}

// Whether the front pending key has reached its settle deadline. A pure predicate over
// absolute time, so a wake by key or by timeout gives the same answer.
fn settle_check_due(pending_front_t_ms: Option<u64>, now: u64) -> bool {
    pending_front_t_ms.is_some_and(|t| now >= t.saturating_add(SETTLE_MS))
}

// Whether the ~50ms focus poll is due. Only meaningful while a key is pending.
fn focus_poll_due(pending_is_empty: bool, now: u64, next_poll_due: u64) -> bool {
    !pending_is_empty && now >= next_poll_due
}

// A poll this late means the consumer was stuck (a slow UI Automation call, up to
// UIA_TIMEOUT_MS). The whole stalled span is treated as unknown, since a focus flip from
// A to B and back inside the stall would compare as unchanged at both endpoints.
fn poll_overran(now: u64, next_poll_due: u64) -> bool {
    now.saturating_sub(next_poll_due) > FOCUS_POLL_MS
}

// Safe half of the idle-to-busy transition (chosen by classify_idle_transition): rearms
// the poll deadline from the key's own hook-time timestamp, not from when this thread
// receives it. last_focus stays untouched so a later poll compares against a real
// baseline.
fn idle_to_busy_reset(raw_t_ms: u64) -> (u64, u64) {
    (raw_t_ms, raw_t_ms.saturating_add(FOCUS_POLL_MS))
}

// What the idle-to-busy transition does with the arriving key. Rearm is the ordinary case.
// ForceGap is chosen when the key cannot be trusted to a fresh baseline and is judged
// against the real span [span_from, span_to] instead (see mark_forced_gaps).
enum IdleTransition {
    Rearm { last_focus_at: u64, next_poll_due: u64 },
    ForceGap { span_from: u64, span_to: u64 },
}

// Decides which half of the idle-to-busy transition applies. recv_entered_at is when this
// thread last began waiting in recv_timeout with nothing pending; last_observed_at is the
// last instant focus was actually looked at. Rearming from the key's timestamp is safe
// only if the thread was idle in that recv when the key arrived. A key that predates
// recv_entered_at sat in the channel while the thread was busy, and no check ran in that
// time (checks need a pending key). A short busy stretch after a real observation still
// rearms; once the silence exceeds the poll_overran allowance the key is force-gapped
// across the whole span back to last_observed_at.
fn classify_idle_transition(raw_t_ms: u64, recv_entered_at: u64, last_observed_at: u64, now: u64) -> IdleTransition {
    let busy_before_recv = raw_t_ms < recv_entered_at;
    let observation_due = last_observed_at.saturating_add(FOCUS_POLL_MS);
    let observation_stale = poll_overran(recv_entered_at, observation_due);
    if busy_before_recv && observation_stale {
        IdleTransition::ForceGap { span_from: last_observed_at, span_to: now }
    } else {
        let (last_focus_at, next_poll_due) = idle_to_busy_reset(raw_t_ms);
        IdleTransition::Rearm { last_focus_at, next_poll_due }
    }
}

// Whether a key's timestamp lies in a span the poll cannot vouch for, widened by GUARD_MS
// on both sides like every other blind window.
fn key_in_widened_span(t_ms: u64, span_from: u64, span_to: u64) -> bool {
    t_ms >= span_from.saturating_sub(GUARD_MS) && t_ms <= span_to.saturating_add(GUARD_MS)
}

// Marks pending keys inside [span_from, span_to] (already widened by GUARD_MS) forced_gap.
// Called once per poll tick that could not vouch for the interval since the last confirmed
// one.
fn mark_forced_gaps(pending: &mut VecDeque<RawKey>, span_from: u64, span_to: u64) {
    for raw in pending.iter_mut() {
        if key_in_widened_span(raw.t_ms, span_from, span_to) {
            raw.forced_gap = true;
        }
    }
}

// Resolves keys at the front of `pending` in order and stops at the first one verdict
// cannot decide. FIFO is safe because a later key always needs a later qualifying check.
fn resolve_pending(
    pending: &mut VecDeque<RawKey>,
    rec: &mut Recorder,
    clock: &Mutex<ActiveClock>,
    cutoff: &Mutex<Option<u64>>,
    ledger: &Mutex<Ledger>,
    shortcuts_only: bool,
) {
    while let Some(front) = pending.front() {
        let cutoff_ms = *cutoff.lock().unwrap_or_else(|e| e.into_inner());
        // The cutoff can be set after a key was queued, so it is checked on every pass.
        let v = if front.forced_gap || past_cutoff(front.t_ms, cutoff_ms) {
            Some(Verdict::Gap)
        } else {
            let guard = ledger.lock().unwrap_or_else(|e| e.into_inner());
            verdict(&guard, front.t_ms, front.injected)
        };
        let Some(v) = v else { break };
        let raw = pending.pop_front().expect("front already confirmed present above");
        let gap = resolve_verdict(v, shortcuts_only, KeyId { vk: raw.vk, scan: raw.scan, extended: raw.extended, ctrl: raw.ctrl, alt: raw.alt, win: raw.win });
        let t_ms = clock.lock().unwrap_or_else(|e| e.into_inner()).active_ms(raw.t_ms);
        rec.record(raw.vk, raw.scan, raw.down, t_ms, gap);
    }
}

// UI Automation classes to try in order: CUIAutomation8 serves IUIAutomation2
// everywhere, the original class only on some builds.
pub(crate) fn uia_class_order() -> [windows::core::GUID; 2] {
    [CUIAutomation8, CUIAutomation]
}

// First class for which try_class yields a ready object; later classes are not touched.
pub(crate) fn first_ready_uia<T>(classes: &[windows::core::GUID], try_class: impl FnMut(&windows::core::GUID) -> Option<T>) -> Option<T> {
    classes.iter().find_map(try_class)
}

fn consumer_thread_main(rx: Receiver<RawKey>, clock: Arc<Mutex<ActiveClock>>, cutoff: Arc<Mutex<Option<u64>>>, shortcuts_only: bool) -> KeySidecar {
    // MTA, not STA: an STA object needs a message pump for cross-thread calls (the
    // focus-changed event fires from another thread), and this thread pumps nothing but its
    // channel.
    //
    // Every step through the two timeout setters must succeed before any further UIA call,
    // since the timeouts keep a hung target from blocking UIA and stop(). Otherwise `uia` is
    // dropped and every check takes the fail-closed answer for a missing object.
    let co_init_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let mut uia: Option<IUIAutomation> = None;
    let mut uia_ready = false;
    if co_init_ok {
        // CUIAutomation8 first: on some Windows 11 builds the plain class does not answer
        // IUIAutomation2 (E_NOINTERFACE). The first fully bounded object wins.
        uia = first_ready_uia(&uia_class_order(), |clsid| {
            let candidate: IUIAutomation = unsafe { CoCreateInstance(clsid, None, CLSCTX_INPROC_SERVER) }.ok()?;
            let uia2 = candidate.cast::<IUIAutomation2>().ok()?;
            let bounded = unsafe { uia2.SetConnectionTimeout(UIA_TIMEOUT_MS).is_ok() && uia2.SetTransactionTimeout(UIA_TIMEOUT_MS).is_ok() };
            bounded.then_some(candidate)
        });
        uia_ready = uia.is_some();
    }
    if !uia_ready {
        uia = None;
    }
    let ledger = Arc::new(Mutex::new(Ledger::default()));
    if !uia_ready {
        // Same fail-closed outcome as a failed handler registration: there is no bounded UI
        // Automation object to register on.
        ledger.lock().unwrap_or_else(|e| e.into_inner()).gap_everything();
        append_crash_log("key overlay: could not set up UI Automation with a bounded timeout, gapping every key this session");
    }
    // Confirm the focus state once before the first key can be judged (see
    // Ledger::qualifying_check).
    ledger.lock().unwrap_or_else(|e| e.into_inner()).push_check(mono_ms(), classify_focused_field(&uia));
    let focus_handler: Option<IUIAutomationFocusChangedEventHandler> =
        uia.as_ref().map(|uia| FocusHandler { ledger: ledger.clone(), uia: uia.clone() }.into());
    if let (Some(uia), Some(handler)) = (&uia, &focus_handler) {
        if unsafe { uia.AddFocusChangedEventHandler(None, handler) }.is_err() {
            // Without the focus-changed event an in-page field switch between two checks goes
            // unnoticed (the scheduled checks and focus poll are no substitute), so every key gaps for
            // the rest of the session.
            ledger.lock().unwrap_or_else(|e| e.into_inner()).gap_everything();
            append_crash_log("key overlay: could not register the UI Automation focus-changed handler, gapping every key this session");
        }
    }

    let mut rec = Recorder::default();
    // Keys not yet resolved by the verdict algorithm (see resolve_pending). A key waits until
    // a check has started SETTLE_MS after it.
    let mut pending: VecDeque<RawKey> = VecDeque::new();
    // Focus identity as of the last poll tick, taken before any key can be pending so the
    // first poll has a real baseline.
    let mut last_focus = focus_identity(&uia);
    // When last_focus was last confirmed unchanged. A change found by the poll could have
    // happened anywhere since, so the arrival is stamped here, not when a slow
    // GetFocusedElement returns.
    let mut last_focus_at = mono_ms();
    // Next absolute instant the focus poll is due, tracked apart from whatever wakes the loop.
    let mut next_poll_due = mono_ms().saturating_add(FOCUS_POLL_MS);
    // The last instant focus was actually looked at: a completed poll tick, or entering the
    // blocking recv with nothing pending. Unlike last_focus_at, an arriving key does not
    // rearm it (see classify_idle_transition).
    let mut last_observed_at = mono_ms();
    loop {
        // Block until the next raw key or a timeout. While keys are pending the timeout is also
        // the ~50ms focus poll. With nothing pending the wait is still bounded, so the loop wakes
        // to prune the ledger that the focus-changed handler keeps filling on its own thread.
        // recv_entered_at is taken before either arm; in the idle arm it doubles as a real focus
        // observation (see last_observed_at).
        let recv_entered_at = mono_ms();
        let recv_result = match pending.front() {
            Some(front) => {
                let wait = recv_wait_ms(mono_ms(), front.t_ms, next_poll_due);
                rx.recv_timeout(Duration::from_millis(wait))
            }
            None => {
                last_observed_at = recv_entered_at;
                rx.recv_timeout(Duration::from_millis(IDLE_PRUNE_MS))
            }
        };
        match recv_result {
            Ok(mut raw) => {
                let paused = clock.lock().unwrap_or_else(|e| e.into_inner()).is_paused_at(raw.t_ms);
                if paused {
                    // Dropped outright, not even a gap: a gap means doubt, a pause means certainty. The hook
                    // is uninstalled during a pause too, so this only catches events in flight when pause()
                    // tore it down.
                } else {
                    // A Tab, Shift+Tab, Enter or Esc key-down counts as a focus arrival (see
                    // is_focus_moving_key), stamped at the hook's timestamp.
                    if raw.down && is_focus_moving_key(raw.vk) {
                        ledger.lock().unwrap_or_else(|e| e.into_inner()).push_arrival(raw.t_ms);
                    }
                    if pending.is_empty() {
                        // Idle-to-busy transition (see classify_idle_transition): rearm the poll baseline from
                        // this key's timestamp, or force-gap the key if it sat in the channel too long since focus
                        // was last observed.
                        let now = mono_ms();
                        match classify_idle_transition(raw.t_ms, recv_entered_at, last_observed_at, now) {
                            IdleTransition::Rearm { last_focus_at: at, next_poll_due: due } => {
                                last_focus_at = at;
                                next_poll_due = due;
                            }
                            IdleTransition::ForceGap { span_from, span_to } => {
                                raw.forced_gap = true;
                                let mut guard = ledger.lock().unwrap_or_else(|e| e.into_inner());
                                guard.push_arrival(span_from);
                                guard.push_arrival(span_to);
                            }
                        }
                    }
                    if pending.len() >= PENDING_CAP {
                        // Verdicts are not keeping up with the raw key rate: gap everything. The resolve_pending
                        // call below drains the backlog in one pass once fail_closed is set.
                        ledger.lock().unwrap_or_else(|e| e.into_inner()).gap_everything();
                        append_crash_log("key overlay: pending queue exceeded its cap, gapping every key this session");
                    }
                    pending.push_back(raw);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        // Both run on their own absolute deadlines every iteration, whether a key or a timeout
        // woke the loop. Gating them to the Timeout arm let a burst of key repeats starve both and
        // miss a field that went dirty and clean again unobserved.
        let now = mono_ms();
        if settle_check_due(pending.front().map(|front| front.t_ms), now) {
            // classify_checked reads the focused element once and confirms the identity did not move
            // meanwhile; a plain classify_focused_field would be stamped at `now` while its reads (up
            // to UIA_TIMEOUT_MS) run later. A confirmed change is also recorded as an arrival at `now`.
            let (class, invalidated) = classify_checked(&uia);
            let mut guard = ledger.lock().unwrap_or_else(|e| e.into_inner());
            guard.push_check(now, class);
            if invalidated {
                guard.push_arrival(now);
            }
        }
        // The ~50ms focus poll: while a key waits, check whether the focused HWND or element moved
        // since the last tick and record an arrival if so. It catches changes in quiet stretches
        // and is the fallback when the focus-changed event never fires.
        if focus_poll_due(pending.is_empty(), now, next_poll_due) {
            let cur = focus_identity(&uia);
            let changed = focus_identity_changed(&uia, &last_focus, &cur);
            // A stalled poll is treated like a detected change (see poll_overran).
            let stalled = poll_overran(now, next_poll_due);
            if changed || stalled {
                // The change (or whatever a stalled poll missed) could have happened anywhere in
                // (last_focus_at, now], far wider than SETTLE_MS/GUARD_MS cover from one point. Pending
                // keys inside the span are forced to gap, and both ends are recorded as arrivals for later
                // keys. mark_forced_gaps widens by GUARD_MS itself, so the endpoints go in unwidened.
                mark_forced_gaps(&mut pending, last_focus_at, now);
                let mut guard = ledger.lock().unwrap_or_else(|e| e.into_inner());
                guard.push_arrival(last_focus_at);
                guard.push_arrival(now);
            }
            last_focus = cur;
            last_focus_at = now;
            next_poll_due = now.saturating_add(FOCUS_POLL_MS);
            // A completed poll is a real observation, whatever it found.
            last_observed_at = now;
        }
        resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, shortcuts_only);
        // Nothing before the oldest pending key's GUARD_MS lookback is read again (see
        // Ledger::prune).
        let prune_before = pending.front().map(|k| k.t_ms).unwrap_or_else(mono_ms).saturating_sub(GUARD_MS);
        ledger.lock().unwrap_or_else(|e| e.into_inner()).prune(prune_before);
    }
    // The channel is gone: stop() already uninstalled the hook, so no further check will run.
    // Leftover keys must not each cost a live UIA timeout while the user waits for the
    // recording to stop; verdict_at_stop honors a check that already completed but never
    // starts a new one.
    for raw in pending {
        let gap = if raw.forced_gap {
            true
        } else {
            let ledger = ledger.lock().unwrap_or_else(|e| e.into_inner());
            verdict_at_stop(&ledger, raw.t_ms, raw.injected, KeyId { vk: raw.vk, scan: raw.scan, extended: raw.extended, ctrl: raw.ctrl, alt: raw.alt, win: raw.win }, shortcuts_only)
        };
        let t_ms = clock.lock().unwrap_or_else(|e| e.into_inner()).active_ms(raw.t_ms);
        rec.record(raw.vk, raw.scan, raw.down, t_ms, gap);
    }
    if let (Some(uia), Some(handler)) = (&uia, &focus_handler) {
        if unsafe { uia.RemoveFocusChangedEventHandler(handler) }.is_err() {
            append_crash_log("key overlay: could not remove the UI Automation focus-changed handler cleanly");
        }
    }
    // Release both COM objects while their apartment is still alive, before CoUninitialize.
    drop(focus_handler);
    drop(uia);
    if co_init_ok {
        // Balanced only against a CoInitializeEx that succeeded.
        unsafe {
            CoUninitialize();
        }
    }
    rec.sidecar
}

// The hook half of a capture, torn down on pause and rebuilt on resume (see
// KeyCapture::hook).
struct HookHandle {
    thread: JoinHandle<()>,
    thread_id: u32,
}

fn install_hook() -> Option<HookHandle> {
    // Reset to "nothing held": state from before an unhooked stretch could be stale, and a
    // stale true would fail open (see ModifierState).
    *modifiers_slot().lock().unwrap_or_else(|e| e.into_inner()) = ModifierState::default();
    let (ready_tx, ready_rx) = channel::<Result<u32, String>>();
    let thread = thread::Builder::new().name("key-overlay-hook".into()).spawn(move || hook_thread_main(ready_tx)).ok()?;
    match ready_rx.recv() {
        Ok(Ok(thread_id)) => Some(HookHandle { thread, thread_id }),
        _ => {
            let _ = thread.join();
            None
        }
    }
}

fn uninstall_hook(hook: HookHandle) {
    unsafe {
        let _ = PostThreadMessageW(hook.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
    }
    let _ = hook.thread.join();
}

pub struct KeyCapture {
    // None while paused. Pausing uninstalls the hook, so nothing depends on the consumer
    // catching events. The consumer thread, Recorder, pending queue, Ledger and UIA
    // registration stay alive across pause and resume.
    hook: Option<HookHandle>,
    consumer_thread: JoinHandle<KeySidecar>,
    hold_ms: u32,
    clock: Arc<Mutex<ActiveClock>>,
    cutoff: Arc<Mutex<Option<u64>>>,
}

fn start(hold_ms: u32, start_abs_ms: u64, shortcuts_only: bool) -> Option<KeyCapture> {
    let (raw_tx, raw_rx) = sync_channel::<RawKey>(RAW_CHANNEL_CAP);
    *raw_tx_slot().lock().unwrap_or_else(|e| e.into_inner()) = Some(raw_tx);
    let clock = Arc::new(Mutex::new(ActiveClock::new(start_abs_ms)));
    let cutoff = Arc::new(Mutex::new(None));
    let hook = match install_hook() {
        Some(h) => h,
        None => {
            *raw_tx_slot().lock().unwrap_or_else(|e| e.into_inner()) = None;
            return None;
        }
    };
    let clock2 = clock.clone();
    let cutoff2 = cutoff.clone();
    // If the consumer cannot start, the hook must come down the way stop() brings it down;
    // otherwise it would run forever with nothing reading it.
    let consumer_thread = match thread::Builder::new().name("key-overlay-consumer".into()).spawn(move || consumer_thread_main(raw_rx, clock2, cutoff2, shortcuts_only)) {
        Ok(t) => t,
        Err(_) => {
            uninstall_hook(hook);
            *raw_tx_slot().lock().unwrap_or_else(|e| e.into_inner()) = None;
            return None;
        }
    };
    Some(KeyCapture { hook: Some(hook), consumer_thread, hold_ms, clock, cutoff })
}

// How long stop() waits for the consumer thread: generous for a real UIA round trip, but
// bounded so a hung provider cannot freeze the app's stop action.
const STOP_JOIN_BOUND_MS: u64 = 5_000;

// Polls is_finished() rather than join(), which has no timeout. On timeout the handle is
// dropped, which detaches the thread. Free of Windows APIs so the bound is testable with
// an ordinary thread.
fn join_bounded<T: Send + 'static>(handle: JoinHandle<T>, bound: Duration) -> Option<T> {
    let deadline = Instant::now() + bound;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(20));
    }
    handle.join().ok()
}

fn stop(mut capture: KeyCapture) -> KeySidecar {
    // Already unhooked when a paused capture is stopped; the consumer waits in recv() for the
    // channel to close.
    if let Some(hook) = capture.hook.take() {
        uninstall_hook(hook);
    }
    // Dropping the sender closes the channel and ends the consumer's recv() loop, unless it is
    // stuck outside it, which is what the bound below is for.
    *raw_tx_slot().lock().unwrap_or_else(|e| e.into_inner()) = None;
    let mut sidecar = match join_bounded(capture.consumer_thread, Duration::from_millis(STOP_JOIN_BOUND_MS)) {
        Some(sidecar) => sidecar,
        None => {
            append_crash_log("key overlay: consumer thread did not stop in time, detaching it and reporting no captured keys");
            KeySidecar::default()
        }
    };
    sidecar.hold_ms = capture.hold_ms;
    sidecar
}

// Pause and resume, driven by the hotkeys and buttons through one shared path
// (do_pause_recording/do_resume_recording). Pausing uninstalls the hook, a stronger
// guarantee than a hook that promises to drop everything. No-op when no capture runs.
pub fn pause() {
    let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(cap) = guard.as_mut() {
        cap.clock.lock().unwrap_or_else(|e| e.into_inner()).pause(mono_ms());
        if let Some(hook) = cap.hook.take() {
            uninstall_hook(hook);
        }
    }
}

// Mirrors the keyOverlay flag for the capture: set by the flag switch, and claimed as on when
// a capture starts before any switch happened. Resume never re-hooks the keyboard after the
// flag went off, and a start that read the flag as on just before it was switched off is
// refused.
const FLAG_UNKNOWN: u8 = 0;
const FLAG_ON: u8 = 1;
const FLAG_OFF: u8 = 2;
static FLAG_STATE: AtomicU8 = AtomicU8::new(FLAG_UNKNOWN);

// Pure: whether resume should put the keyboard hook back.
fn hook_install_wanted(flag_on: bool, hook_present: bool) -> bool {
    flag_on && !hook_present
}

// Pure: the flag state after a capture start, or None when the live flag is off.
fn claim_start(state: u8) -> Option<u8> {
    if state == FLAG_OFF {
        None
    } else {
        Some(FLAG_ON)
    }
}

// Called when the keyOverlay flag is switched. Off ends a running capture right away, without
// a sidecar.
pub fn flag_changed(enabled: bool) {
    FLAG_STATE.store(if enabled { FLAG_ON } else { FLAG_OFF }, Ordering::SeqCst);
    if !enabled {
        discard();
    }
}

pub fn resume() {
    let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(cap) = guard.as_mut() {
        cap.clock.lock().unwrap_or_else(|e| e.into_inner()).resume(mono_ms());
        if hook_install_wanted(FLAG_STATE.load(Ordering::SeqCst) == FLAG_ON, cap.hook.is_some()) {
            cap.hook = install_hook();
            if cap.hook.is_none() {
                // Fails safe: no hook means no keys are recorded, same as still being paused.
                append_crash_log("key overlay: could not reinstall the keyboard hook on resume");
            }
        }
    }
}

// Boundary marker for finish_and_continue_recording: every key from t_ms onward belongs to
// the switch or the next part. No-op when no capture runs.
pub fn cut_off_at(t_ms: u64) {
    if let Some(cap) = slot().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        *cap.cutoff.lock().unwrap_or_else(|e| e.into_inner()) = Some(t_ms);
    }
}

// The error side is a path-free reason, safe for the crash log.
pub(crate) fn try_write_sidecar(output_path: &Path, sidecar: &KeySidecar) -> Result<(), String> {
    let json = serde_json::to_string(sidecar).map_err(|_| "serialize failed".to_string())?;
    // Sealed like the rest of the local store: the events are the keys
    // typed during the recording. Readers pass old plain files through.
    super::write_bytes_atomic(&super::sidecar_path(output_path, super::KEYS_SIDECAR_EXT), super::enc_str(&json).as_bytes())
        .map_err(|e| format!("write failed ({:?})", e.kind()))
}

fn write_sidecar(output_path: &Path, sidecar: &KeySidecar) {
    if let Err(why) = try_write_sidecar(output_path, sidecar) {
        append_crash_log(&format!("key overlay: could not save the key sidecar, the recording has no overlay: {why}"));
    }
}

static CAPTURE: OnceLock<Mutex<Option<KeyCapture>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<KeyCapture>> {
    CAPTURE.get_or_init(|| Mutex::new(None))
}

pub fn enabled(flags: &std::collections::HashMap<String, bool>) -> bool {
    flags.get("keyOverlay") == Some(&true)
}

// How long the key overlay shows a key press, in milliseconds.
pub const HOLD_MS: u32 = 400;

// Ships off; absent means the default policy (everything but a password field recorded in
// full). Read once when a capture starts.
pub fn shortcuts_only(flags: &std::collections::HashMap<String, bool>) -> bool {
    flags.get("keyOverlayShortcutsOnly") == Some(&true)
}

// Never a second hook: a running capture is left alone. start_abs_ms is the clock's zero
// point on the mono_ms() clock; finish_and_continue_recording passes the instant its new
// capture really started, so the new part's offsets count from when video capture began.
pub fn maybe_start(is_enabled: bool, hold_ms: u32, start_abs_ms: u64, shortcuts_only: bool) {
    if !is_enabled {
        return;
    }
    let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_some() {
        return;
    }
    // The caller read the flag earlier; the live state decides. flag_changed stores the state
    // before it discards under this same lock, so a switch-off either refuses this start or
    // discards the capture it installs.
    if let Err(live) = FLAG_STATE.compare_exchange(FLAG_UNKNOWN, FLAG_ON, Ordering::SeqCst, Ordering::SeqCst) {
        if claim_start(live).is_none() {
            return;
        }
    }
    *guard = start(hold_ms, start_abs_ms, shortcuts_only);
}

pub fn maybe_stop_and_write(output_path: &Path) {
    let capture = slot().lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(capture) = capture {
        let sidecar = stop(capture);
        write_sidecar(output_path, &sidecar);
    }
}

// A cancelled/discarded take never gets a sidecar file at all.
pub fn discard() {
    let capture = slot().lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(capture) = capture {
        let _ = stop(capture);
    }
}

#[cfg(test)]
#[path = "keylog_tests.rs"]
mod tests;
