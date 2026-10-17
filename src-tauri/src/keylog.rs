use super::append_crash_log;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::mpsc::{channel, sync_channel, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::atomic::{AtomicBool, Ordering};
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
        self.pauses.iter().any(|(s, e)| t_ms >= *s && e.map_or(true, |e| t_ms < e))
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
    fn HandleFocusChangedEvent(&self, _sender: Option<&IUIAutomationElement>) -> windows::core::Result<()> {
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

// Mirrors the keyOverlay flag for the running capture: set when a capture starts and by the
// flag switch, so resume never re-hooks the keyboard after the flag went off.
static FLAG_ON: AtomicBool = AtomicBool::new(false);

// Pure: whether resume should put the keyboard hook back.
fn hook_install_wanted(flag_on: bool, hook_present: bool) -> bool {
    flag_on && !hook_present
}

// Called when the keyOverlay flag is switched. Off ends a running capture right away, without
// a sidecar.
pub fn flag_changed(enabled: bool) {
    FLAG_ON.store(enabled, Ordering::SeqCst);
    if !enabled {
        discard();
    }
}

pub fn resume() {
    let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(cap) = guard.as_mut() {
        cap.clock.lock().unwrap_or_else(|e| e.into_inner()).resume(mono_ms());
        if hook_install_wanted(FLAG_ON.load(Ordering::SeqCst), cap.hook.is_some()) {
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

fn write_sidecar(output_path: &Path, sidecar: &KeySidecar) {
    if let Ok(json) = serde_json::to_string(sidecar) {
        // Sealed like the rest of the local store: the events are the keys
        // typed during the recording. Readers pass old plain files through.
        let _ = fs::write(super::sidecar_path(output_path, super::KEYS_SIDECAR_EXT), super::enc_str(&json));
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
    FLAG_ON.store(true, Ordering::SeqCst);
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
mod tests {
    use super::*;
    // Only used by the Pane/Custom test below; imported here so a non-test build has no
    // unused-import warning.
    use windows::Win32::UI::Accessibility::{UIA_CustomControlTypeId, UIA_PaneControlTypeId};

    #[test]
    fn resume_reinstalls_the_hook_only_while_the_flag_is_on() {
        assert!(hook_install_wanted(true, false));
        assert!(!hook_install_wanted(false, false));
        assert!(!hook_install_wanted(true, true));
        assert!(!hook_install_wanted(false, true));
    }

    #[test]
    fn recorder_stores_identifiers_with_timestamps_only() {
        let mut rec = Recorder::default();
        rec.record(0x41, 0, true, 0, false);
        rec.record(0x41, 0, false, 120, false);
        assert_eq!(
            rec.sidecar.events,
            vec![KeyEvent { vk: 0x41, down: true, t_ms: 0 }, KeyEvent { vk: 0x41, down: false, t_ms: 120 }]
        );
        assert!(rec.sidecar.gaps.is_empty());
    }

    #[test]
    fn a_blocked_down_never_grows_open_downs() {
        let mut rec = Recorder::default();
        for scan in 0..(OPEN_DOWNS_CAP as u32 * 2) {
            // Distinct blocked downs (an injected flood) must not grow open_downs: a blocked down
            // removes instead of inserting.
            rec.record(0x41, scan, true, scan as u64, true);
        }
        assert_eq!(rec.open_downs.len(), 0, "a blocked down must never be stored in open_downs");
    }

    #[test]
    fn open_downs_past_its_cap_is_cleared_rather_than_growing_further() {
        let mut rec = Recorder::default();
        for scan in 0..(OPEN_DOWNS_CAP as u32) {
            // Accepted downs for distinct keys with no up are the only way open_downs grows; it stays
            // at the cap.
            rec.record(0x41, scan, true, scan as u64, false);
        }
        assert_eq!(rec.open_downs.len(), OPEN_DOWNS_CAP);
        // One more accepted down past the cap clears the map.
        rec.record(0x41, OPEN_DOWNS_CAP as u32, true, OPEN_DOWNS_CAP as u64, false);
        assert_eq!(rec.open_downs.len(), 0, "open_downs must be cleared once it grows past the defensive cap");
    }

    #[test]
    fn events_stop_growing_past_the_cap_and_leave_a_gap_instead() {
        let mut rec = Recorder::default();
        for i in 0..(EVENTS_CAP / 2) {
            let t = (i * 2) as u64;
            rec.record(0x41, 0, true, t, false);
            rec.record(0x41, 0, false, t + 1, false);
        }
        assert_eq!(rec.sidecar.events.len(), EVENTS_CAP, "must reach the cap exactly, not exceed it");
        // A key past the cap adds no event and extends a gap instead.
        rec.record(0x43, 0, true, EVENTS_CAP as u64, false);
        assert_eq!(rec.sidecar.events.len(), EVENTS_CAP, "an event past the cap must never be stored");
        assert_eq!(rec.sidecar.gaps.len(), 1, "the overlay must show a gap where events stopped, not silent truncation");
        rec.record(0x43, 0, false, EVENTS_CAP as u64 + 10, false);
        assert_eq!(rec.sidecar.events.len(), EVENTS_CAP);
        assert_eq!(rec.sidecar.gaps.len(), 1, "further capped keys extend the same gap rather than opening new ones");
    }

    #[test]
    fn a_password_field_produces_a_gap_instead_of_events() {
        let mut rec = Recorder::default();
        rec.record(0x41, 0, true, 0, false);
        rec.record(0x42, 0, true, 100, true); // focus moves into a password field
        rec.record(0x42, 0, false, 150, true);
        rec.record(0x43, 0, true, 400, false); // focus leaves it again
        assert_eq!(rec.sidecar.events.len(), 2);
        assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 100, end_ms: 150 }]);
    }

    #[test]
    fn back_to_back_password_periods_extend_one_gap_rather_than_opening_a_new_one() {
        let mut rec = Recorder::default();
        rec.record(0x41, 0, true, 0, true);
        rec.record(0x41, 0, false, 10, true);
        rec.record(0x42, 0, true, 20, true);
        assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 0, end_ms: 20 }]);
    }

    #[test]
    fn a_recording_entirely_in_a_password_field_writes_no_events_at_all() {
        let mut rec = Recorder::default();
        rec.record(0x41, 0, true, 0, true);
        rec.record(0x41, 0, false, 50, true);
        assert!(rec.sidecar.events.is_empty());
        assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 0, end_ms: 50 }]);
    }

    #[test]
    fn the_switch_off_default_never_starts_a_capture() {
        let flags = std::collections::HashMap::new();
        assert!(!enabled(&flags)); // absent: off is the shipped default
        maybe_start(enabled(&flags), 400, mono_ms(), shortcuts_only(&flags));
        assert!(slot().lock().unwrap().is_none());
    }

    #[test]
    fn the_sidecar_shape_carries_no_text_field_at_all() {
        // KeyEvent has three fields (vk, down, t_ms) and Gap two, with nowhere for a character,
        // string or window title to land. Checking the serialized key set proves the shape.
        let sidecar = KeySidecar {
            events: vec![KeyEvent { vk: 0x41, down: true, t_ms: 0 }],
            gaps: vec![Gap { start_ms: 10, end_ms: 20 }],
            hold_ms: 400,
        };
        let json = serde_json::to_value(&sidecar).unwrap();
        let event = &json["events"][0];
        let mut keys: Vec<&str> = event.as_object().unwrap().keys().map(|s| s.as_str()).collect();
        keys.sort();
        assert_eq!(keys, vec!["down", "t_ms", "vk"]);
        let gap = &json["gaps"][0];
        let mut gap_keys: Vec<&str> = gap.as_object().unwrap().keys().map(|s| s.as_str()).collect();
        gap_keys.sort();
        assert_eq!(gap_keys, vec!["end_ms", "start_ms"]);
    }

    #[test]
    fn written_sidecar_is_sealed_on_disk_and_parses_back() {
        let dir = std::env::temp_dir().join(format!("ps-keys-sealed-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let video = dir.join("rec-sealed.mp4");
        let sc = KeySidecar { hold_ms: 777, ..Default::default() };
        write_sidecar(&video, &sc);
        let raw = fs::read_to_string(crate::sidecar_path(&video, crate::KEYS_SIDECAR_EXT)).unwrap();
        assert!(!raw.contains("hold_ms") && !raw.starts_with('{'), "the sidecar must not sit on disk as plain JSON");
        assert_eq!(crate::parse_key_sidecar(&raw), Some(sc));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_old_plain_sidecar_still_parses() {
        let parsed = crate::parse_key_sidecar(r#"{"events":[],"gaps":[],"hold_ms":500}"#).unwrap();
        assert_eq!(parsed.hold_ms, 500);
    }

    #[test]
    fn past_cutoff_only_fires_once_a_cutoff_is_actually_set() {
        assert!(!past_cutoff(1_000, None)); // no switch in progress: nothing is cut off
        assert!(!past_cutoff(999, Some(1_000))); // strictly before the switch instant
        assert!(past_cutoff(1_000, Some(1_000))); // AT the switch instant: already cut off
        assert!(past_cutoff(1_500, Some(1_000))); // after the switch instant
    }

    // An injected key gaps exactly like a password field: the consumer records verdict's
    // answer before calling record().
    #[test]
    fn an_injected_key_is_gapped_the_same_way_a_password_field_is() {
        let mut rec = Recorder::default();
        rec.record(0x41, 0, true, 0, false);
        let injected_or_password = true; // stands for verdict()'s Some(true)
        rec.record(0x56, 0, true, 50, injected_or_password); // a password-manager auto-type
        rec.record(0x56, 0, false, 55, injected_or_password);
        rec.record(0x42, 0, true, 200, false);
        assert_eq!(rec.sidecar.events.len(), 2);
        assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 50, end_ms: 55 }]);
    }

    // Down/up taint (Recorder::open_downs): an up is stored only alongside a down this
    // Recorder accepted.

    #[test]
    fn an_up_is_dropped_when_its_matching_down_was_gapped_even_if_the_up_itself_looks_clean() {
        // A password key's down is gapped, focus later moves to a normal field, and the up (clean
        // on its own) is still dropped.
        let mut rec = Recorder::default();
        rec.record(0x41, 0x1E, true, 100, true); // down, gapped (password)
        rec.record(0x41, 0x1E, false, 900, false); // up, its own verdict now clean
        assert!(rec.sidecar.events.is_empty(), "a tainted down must take its up down with it");
        assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 100, end_ms: 900 }]);
    }

    #[test]
    fn an_up_with_no_matching_down_in_this_capture_is_dropped_too() {
        // A key held from before this capture has no accepted down, so its up proves nothing.
        let mut rec = Recorder::default();
        rec.record(0x41, 0x1E, false, 100, false); // up only, own verdict clean
        assert!(rec.sidecar.events.is_empty());
        assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 100, end_ms: 100 }]);
    }

    #[test]
    fn a_clean_down_and_its_matching_up_are_both_stored_normally() {
        let mut rec = Recorder::default();
        rec.record(0x41, 0x1E, true, 0, false);
        rec.record(0x41, 0x1E, false, 120, false);
        assert_eq!(rec.sidecar.events.len(), 2);
        assert!(rec.sidecar.gaps.is_empty());
    }

    #[test]
    fn different_scan_codes_under_the_same_vk_are_tainted_independently() {
        // Left and right Ctrl share a VK; gapping one must not taint the other's pair.
        let mut rec = Recorder::default();
        rec.record(0x11, 0x1D, true, 0, true); // left ctrl down, gapped
        rec.record(0x11, 0x9D, true, 10, false); // right ctrl down, clean
        rec.record(0x11, 0x9D, false, 20, false); // right ctrl up, clean, matches its own down
        assert_eq!(rec.sidecar.events.len(), 2, "the clean scan code's pair must record despite the other one being gapped");
    }

    // The verdict algorithm (Ledger + verdict): a delayed verdict with a settle check, a guard
    // window scanned for dirty checks, and a blind window around every focus change. All pure.

    #[test]
    fn verdict_is_pending_until_a_check_has_started_late_enough_to_speak_for_the_key() {
        // A key at t=0 needs a check that started at t>=300 (SETTLE_MS); an earlier one is not
        // enough.
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::NonText);
        assert_eq!(verdict(&ledger, 0, false), None, "a same-instant check does not qualify");
        ledger.push_check(299, FieldClass::NonText);
        assert_eq!(verdict(&ledger, 0, false), None, "still one ms short of the settle deadline");
        ledger.push_check(300, FieldClass::NonText);
        assert_eq!(verdict(&ledger, 0, false), Some(Verdict::Allow), "a check starting exactly at the deadline qualifies");
    }

    #[test]
    fn verdict_records_normal_typing_once_a_stable_field_has_a_qualifying_check() {
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::NonText); // the capture-start confirmation
        ledger.push_check(400, FieldClass::NonText); // a later check, no focus change in between
        assert_eq!(verdict(&ledger, 50, false), Some(Verdict::Allow));
    }

    #[test]
    fn verdict_gaps_a_key_typed_right_before_the_ui_automation_event_arrives() {
        // Tab into a password field and type before the UI Automation event arrives: the blind
        // window reaches GUARD_MS before the arrival, so the key still gaps.
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::NonText);
        let key_t = 250;
        let arrival = 260; // the Tab's own event arrives just after the key
        ledger.push_arrival(arrival);
        ledger.push_check(arrival, FieldClass::Password); // the field turned out to be a password field
        ledger.push_check(700, FieldClass::Password); // any later check while still in the field
        assert_eq!(verdict(&ledger, key_t, false), Some(Verdict::Gap));
    }

    #[test]
    fn verdict_gaps_every_char_of_a_lagging_consumers_tab_in_pin_tab_out() {
        // Tab in, type a PIN, Tab out. Even when a lagging consumer finds "not password" checks on
        // both sides of a PIN character, the blind windows around the two arrivals gap it.
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::NonText);
        let tab_in = 10;
        ledger.push_arrival(tab_in);
        ledger.push_check(tab_in, FieldClass::Password);
        let pin_chars = [50u64, 70, 90, 110];
        let tab_out = 200;
        ledger.push_arrival(tab_out);
        ledger.push_check(tab_out, FieldClass::NonText);
        ledger.push_check(900, FieldClass::NonText); // the lagging consumer's own late recheck
        for t in pin_chars {
            assert!(verdict(&ledger, t, false) == Some(Verdict::Gap), "PIN char at {t} must gap");
        }
    }

    #[test]
    fn window_is_dirty_treats_an_unknown_check_as_dirty_too() {
        // An Unknown check between a key and its qualifying check dirties the window like a
        // Password check.
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::Text); // stable before any of this
        ledger.push_check(100, FieldClass::Unknown); // classification failed partway through
        ledger.push_check(400, FieldClass::Text); // the qualifying check for the key below
        assert_eq!(
            verdict(&ledger, 50, false),
            Some(Verdict::Gap),
            "an Unknown check inside the window must gap the key, not be ignored by window_is_dirty"
        );
    }

    #[test]
    fn verdict_at_stop_gaps_a_key_with_no_qualifying_check_instead_of_waiting_forever() {
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::NonText); // far too early to qualify for a key at t=1000
        assert_eq!(verdict(&ledger, 1_000, false), None, "still pending under the normal algorithm");
        assert!(verdict_at_stop(&ledger, 1_000, false, KeyId { vk: 0x41, scan: 0, extended: false, ctrl: false, alt: false, win: false }, false), "stopping must resolve doubt as a gap, never as recorded");
    }

    #[test]
    fn verdict_at_stop_still_honors_a_qualifying_check_that_already_completed() {
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::NonText);
        ledger.push_check(400, FieldClass::NonText);
        assert!(!verdict_at_stop(&ledger, 50, false, KeyId { vk: 0x41, scan: 0, extended: false, ctrl: false, alt: false, win: false }, false), "a real, already-completed answer is not thrown away just because the capture is stopping");
    }

    // A modifier the hook tracked as held must be confirmed against the live keyboard before
    // it keeps a key.

    #[test]
    fn a_tracked_modifier_with_no_live_confirmation_is_not_held() {
        // Alt tracked down but released on an elevated window or the secure desktop:
        // GetAsyncKeyState says up and wins.
        assert!(!confirm_modifier(true, false), "a stale tracked modifier with no live confirmation must resolve to not held");
    }

    #[test]
    fn a_tracked_and_live_modifier_is_held() {
        assert!(confirm_modifier(true, true), "a modifier both tracked and live must still count as held");
    }

    #[test]
    fn an_untracked_modifier_is_never_held_even_if_live_state_is_stale() {
        // Both sides must agree: an untracked modifier is never held, whatever the live read says.
        assert!(!confirm_modifier(false, true));
    }

    #[test]
    fn an_injected_modifier_event_is_never_tracked() {
        assert!(!should_track_modifier_event(true), "a synthetic modifier press must never update the tracked state");
        assert!(should_track_modifier_event(false), "a real modifier press must still be tracked");
    }

    // keyOverlayShortcutsOnly: an absolute allow-list, tested directly against
    // is_kept_by_shortcuts_only.

    #[test]
    fn shortcuts_only_drops_a_plain_letter() {
        assert!(!is_kept_by_shortcuts_only(0x41, 0, false, false, false, false), "a plain letter key must be dropped under shortcuts-only");
    }

    #[test]
    fn shortcuts_only_keeps_ctrl_c() {
        assert!(is_kept_by_shortcuts_only(0x43, 0, false, true, false, false), "Ctrl+C is a shortcut, not typed text, so it must record");
    }

    #[test]
    fn shortcuts_only_drops_altgr() {
        // Ctrl+Alt without Win is AltGr (Ctrl+Alt+Q types "@" on a German keyboard): typing, not a
        // shortcut.
        assert!(!is_kept_by_shortcuts_only(0x51, 0, false, true, true, false), "AltGr must still be dropped, not treated as a shortcut");
    }

    // Alt+Numpad and hex numpad character entry is dropped, never kept as an Alt+key shortcut.

    #[test]
    fn shortcuts_only_drops_alt_numpad_digit_with_numlock_on() {
        // NumLock on: Numpad 6 arrives as VK_NUMPAD6 (0x66).
        assert!(!is_kept_by_shortcuts_only(0x66, 0x4D, false, false, true, false), "an Alt+Numpad digit composes a character, it is not a shortcut");
    }

    #[test]
    fn shortcuts_only_drops_alt_numpad_plus_with_numlock_on() {
        assert!(!is_kept_by_shortcuts_only(0x6B, 0x4E, false, false, true, false), "Numpad + starts EnableHexNumpad entry and must be dropped too");
    }

    #[test]
    fn shortcuts_only_drops_alt_numpad_digit_with_numlock_off_by_its_scan_code() {
        // NumLock off: the same key arrives as VK_LEFT (0x25) with scan code 0x4B, not extended.
        // The scan code identifies it.
        assert!(!is_kept_by_shortcuts_only(VK_LEFT, 0x4B, false, false, true, false), "a numpad-origin nav VK with NumLock off must still be dropped under Alt");
    }

    #[test]
    fn shortcuts_only_drops_alt_numpad_digit_key_up_too() {
        // Down and up carry the same vk/scan/flags here, so one call proves both sides agree.
        assert!(!is_kept_by_shortcuts_only(0x66, 0x4D, false, false, true, false), "the down is dropped");
        assert!(!is_kept_by_shortcuts_only(0x66, 0x4D, false, false, true, false), "and so is the matching up, same inputs either way");
    }

    #[test]
    fn shortcuts_only_keeps_the_real_nav_cluster_key_regardless_of_the_numpad_check() {
        // The real arrow cluster sends the same VKs but with LLKHF_EXTENDED set, so Alt+Left stays
        // a kept nav key.
        assert!(is_kept_by_shortcuts_only(VK_LEFT, 0x4B, true, false, true, false), "the real, extended arrow key must still be kept under Alt");
    }

    #[test]
    fn shortcuts_only_keeps_alt_with_an_ordinary_letter() {
        assert!(is_kept_by_shortcuts_only(0x41, 0x1E, false, false, true, false), "Alt+A is an ordinary shortcut, not a numpad scan or VK");
    }

    #[test]
    fn shortcuts_only_does_not_treat_ctrl_alt_numpad_as_numpad_entry() {
        // The numpad check applies to Alt held alone; Ctrl+Alt is AltGr territory and other
        // combinations are ordinary shortcuts.
        assert!(is_kept_by_shortcuts_only(0x66, 0x4D, false, true, true, true), "Ctrl+Alt+Win+Numpad6 is an ordinary shortcut combo, not character entry");
    }

    // Only a structurally non-text control type may resolve to NonText. A type on neither list
    // (a bare Pane/Custom element such as PuTTY or an RDP/VNC session), a failed read or no
    // element is Unknown, which gaps (see classify_control).

    #[test]
    fn every_allow_listed_control_type_is_non_text() {
        for ct in [
            UIA_ButtonControlTypeId,
            UIA_CheckBoxControlTypeId,
            UIA_RadioButtonControlTypeId,
            UIA_MenuItemControlTypeId,
            UIA_TabItemControlTypeId,
            UIA_ListItemControlTypeId,
            UIA_TreeItemControlTypeId,
            UIA_SliderControlTypeId,
            UIA_ScrollBarControlTypeId,
            UIA_HyperlinkControlTypeId,
            UIA_ImageControlTypeId,
            UIA_ToolBarControlTypeId,
            UIA_ThumbControlTypeId,
        ] {
            assert!(is_non_text_control_type(ct));
        }
    }

    #[test]
    fn a_putty_like_pane_or_custom_control_gaps_instead_of_recording() {
        // PuTTY, RDP/VNC windows and Qt/Java/game windows without an accessibility provider
        // surface as a bare Pane or Custom element, which is not allow-listed.
        assert!(!is_non_text_control_type(UIA_PaneControlTypeId));
        assert!(!is_non_text_control_type(UIA_CustomControlTypeId));
        // Such an element never proved a positive Text identification either, so it resolves to
        // Unknown and gaps like a password field. An SSH or RDP password typed there is not
        // recorded.
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::Unknown);
        ledger.push_check(400, FieldClass::Unknown);
        let v = verdict(&ledger, 50, false).expect("a qualifying check exists");
        assert_eq!(v, Verdict::Gap);
        assert!(
            resolve_verdict(v, false, KeyId { vk: 0x41, scan: 0x1E, extended: false, ctrl: false, alt: false, win: false }),
            "an unclassifiable Pane/Custom window's typed letters must be gapped, never recorded"
        );
    }

    // A check's reads must not be trusted once the focused identity has moved on since the
    // check started.

    #[test]
    fn a_changed_hwnd_is_a_changed_identity_even_with_no_element_on_either_side() {
        let a = (HWND(1 as *mut std::ffi::c_void), None);
        let b = (HWND(2 as *mut std::ffi::c_void), None);
        assert!(focus_identity_changed(&None, &a, &b), "a different HWND alone is enough, no UIA object needed to see it");
    }

    #[test]
    fn the_same_hwnd_with_no_element_on_either_side_is_not_a_changed_identity() {
        let a = (HWND(1 as *mut std::ffi::c_void), None);
        let b = (HWND(1 as *mut std::ffi::c_void), None);
        assert!(!focus_identity_changed(&None, &a, &b));
    }

    #[test]
    fn is_password_field_fails_closed_with_no_element_to_query() {
        // A missing element (UIA unavailable or the fetch failed) resolves to the fail-closed
        // Password answer.
        assert!(is_password_field(HWND(1 as *mut std::ffi::c_void), None));
    }

    #[test]
    fn classify_control_is_unknown_with_no_element_to_query() {
        // No element to query is Unknown (which gaps), not Text.
        assert_eq!(classify_control(None), FieldClass::Unknown, "no element to classify is doubt, never a positive Text finding");
    }

    #[test]
    fn classify_identity_is_password_when_the_shared_element_is_missing() {
        let identity = (HWND(1 as *mut std::ffi::c_void), None);
        assert_eq!(classify_identity(&identity), FieldClass::Password);
    }

    #[test]
    fn classify_checked_fails_closed_and_reports_no_change_with_no_uia_object() {
        // With no UI Automation object, focus_identity touches no live element, so classify_checked
        // takes the ordinary fail-closed path: Password, not reported as invalidated.
        assert_eq!(classify_checked(&None), (FieldClass::Password, false));
    }

    #[test]
    fn a_late_detected_focus_change_after_a_stall_gaps_the_key_typed_during_it() {
        // A stale check reporting the old field as non-text, arriving well after the key, would
        // satisfy verdict's Allow branch on its own.
        let mut ledger = Ledger::default();
        ledger.push_check(1600, FieldClass::NonText);
        assert_eq!(verdict(&ledger, 1200, false), Some(Verdict::Allow));

        let mut pending: VecDeque<RawKey> = VecDeque::new();
        pending.push_back(RawKey {
            vk: 0x41,
            scan: 0,
            extended: false,
            down: true,
            t_ms: 1200,
            injected: false,
            ctrl: false,
            alt: false,
            win: false,
            forced_gap: false,
        });

        // The poll last confirmed focus at 1000, stalled in a slow UI Automation call and detected
        // the change at 2000. The key above landed inside that span, which mark_forced_gaps and
        // push_arrival cover.
        mark_forced_gaps(&mut pending, 1000, 2000);
        ledger.push_arrival(1000);
        ledger.push_arrival(2000);

        let mut rec = Recorder::default();
        let clock = Mutex::new(ActiveClock::new(0));
        let cutoff: Mutex<Option<u64>> = Mutex::new(None);
        let ledger = Mutex::new(ledger);
        resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

        assert!(pending.is_empty());
        assert!(rec.sidecar.events.is_empty());
        assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 1200, end_ms: 1200 }]);
    }

    #[test]
    fn a_poll_that_overran_its_deadline_counts_as_a_stall_even_with_no_detected_change() {
        assert!(!poll_overran(1049, 1000)); // 49ms late, still within FOCUS_POLL_MS
        assert!(!poll_overran(1050, 1000)); // exactly FOCUS_POLL_MS late, not yet over
        assert!(poll_overran(1051, 1000)); // over by more than one full poll interval
    }

    #[test]
    fn key_in_widened_span_covers_guard_ms_on_both_sides_of_the_span() {
        assert!(!key_in_widened_span(699, 1000, 2000)); // just outside, even widened
        assert!(key_in_widened_span(700, 1000, 2000)); // span start - GUARD_MS
        assert!(key_in_widened_span(1500, 1000, 2000)); // inside the span itself
        assert!(key_in_widened_span(2300, 1000, 2000)); // span end + GUARD_MS
        assert!(!key_in_widened_span(2301, 1000, 2000)); // just outside, even widened
    }

    #[test]
    fn mark_forced_gaps_only_touches_keys_inside_the_widened_span() {
        let mut pending: VecDeque<RawKey> = VecDeque::new();
        let make = |t_ms: u64| RawKey {
            vk: 0x41,
            scan: 0,
            extended: false,
            down: true,
            t_ms,
            injected: false,
            ctrl: false,
            alt: false,
            win: false,
            forced_gap: false,
        };
        pending.push_back(make(500)); // before the widened span
        pending.push_back(make(1500)); // inside
        pending.push_back(make(2500)); // after the widened span
        mark_forced_gaps(&mut pending, 1000, 2000);
        let flags: Vec<bool> = pending.iter().map(|k| k.forced_gap).collect();
        assert_eq!(flags, vec![false, true, false]);
    }

    fn make_raw_key(t_ms: u64) -> RawKey {
        RawKey {
            vk: 0x41,
            scan: 0,
            extended: false,
            down: true,
            t_ms,
            injected: false,
            ctrl: false,
            alt: false,
            win: false,
            forced_gap: false,
        }
    }

    #[test]
    fn idle_to_busy_reset_anchors_the_deadline_to_the_keys_own_hook_timestamp() {
        let (last_focus_at, next_poll_due) = idle_to_busy_reset(1200);
        assert_eq!(last_focus_at, 1200);
        assert_eq!(next_poll_due, 1250);
    }

    #[test]
    fn first_key_after_an_idle_gap_consumed_promptly_is_kept() {
        // An idle gap alone, with the key consumed right after capture, must not force-gap it:
        // next_poll_due lands at raw.t_ms + 50.
        let (_, next_poll_due) = idle_to_busy_reset(1200);
        let now = 1200;
        assert!(!poll_overran(now, next_poll_due));

        let mut ledger = Ledger::default();
        ledger.push_check(1600, FieldClass::NonText);
        assert_eq!(verdict(&ledger, 1200, false), Some(Verdict::Allow));

        let mut pending: VecDeque<RawKey> = VecDeque::new();
        pending.push_back(make_raw_key(1200));

        let mut rec = Recorder::default();
        let clock = Mutex::new(ActiveClock::new(0));
        let cutoff: Mutex<Option<u64>> = Mutex::new(None);
        let ledger = Mutex::new(ledger);
        resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

        assert!(pending.is_empty());
        assert!(rec.sidecar.gaps.is_empty(), "an idle gap with a prompt receive must not turn into a recorded gap");
    }

    #[test]
    fn first_key_after_an_idle_gap_with_a_focus_change_on_the_next_poll_is_gapped() {
        // The idle-to-busy reset only rearms the deadline and leaves last_focus alone, so a later
        // poll that detects a changed identity still gaps the key through mark_forced_gaps.
        let (last_focus_at, _) = idle_to_busy_reset(1200);

        let mut ledger = Ledger::default();
        ledger.push_check(1600, FieldClass::NonText);

        let mut pending: VecDeque<RawKey> = VecDeque::new();
        pending.push_back(make_raw_key(1200));
        mark_forced_gaps(&mut pending, last_focus_at, 1300);
        ledger.push_arrival(last_focus_at);
        ledger.push_arrival(1300);

        let mut rec = Recorder::default();
        let clock = Mutex::new(ActiveClock::new(0));
        let cutoff: Mutex<Option<u64>> = Mutex::new(None);
        let ledger = Mutex::new(ledger);
        resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

        assert!(pending.is_empty());
        assert!(rec.sidecar.events.is_empty());
        assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 1200, end_ms: 1200 }]);
    }

    #[test]
    fn a_genuine_stall_with_a_key_already_pending_still_forces_the_gap() {
        // A poll armed because a key was already pending still treats its own overrun as a stall;
        // idle_to_busy_reset must not weaken that.
        assert!(poll_overran(2000, 1000));
        let mut pending: VecDeque<RawKey> = VecDeque::new();
        pending.push_back(make_raw_key(1200));
        mark_forced_gaps(&mut pending, 1000, 2000);
        assert!(pending[0].forced_gap);
    }

    #[test]
    fn a_delayed_receive_with_no_detected_change_still_forces_the_gap() {
        // Focus goes A, password/text, A inside the gap between this key's hook timestamp (1200)
        // and its receipt (1400), with no UIA event. An endpoint comparison would read
        // "unchanged"; anchoring the deadline to raw.t_ms makes poll_overran catch it (1250 is
        // more than one poll interval behind).
        let (last_focus_at, next_poll_due) = idle_to_busy_reset(1200);
        let now = 1400;
        assert!(poll_overran(now, next_poll_due), "a consumption delay this long must read as an overrun on its own");

        let mut pending: VecDeque<RawKey> = VecDeque::new();
        pending.push_back(make_raw_key(1200));
        mark_forced_gaps(&mut pending, last_focus_at, now);

        let mut ledger = Ledger::default();
        ledger.push_check(1600, FieldClass::NonText);
        ledger.push_arrival(last_focus_at);
        ledger.push_arrival(now);

        let mut rec = Recorder::default();
        let clock = Mutex::new(ActiveClock::new(0));
        let cutoff: Mutex<Option<u64>> = Mutex::new(None);
        let ledger = Mutex::new(ledger);
        resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

        assert!(pending.is_empty());
        assert!(rec.sidecar.events.is_empty());
        assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 1200, end_ms: 1200 }]);
    }

    #[test]
    fn a_key_captured_while_busy_before_recv_with_a_stale_observation_is_gapped() {
        // The thread was busy (not in recv) while pending stayed empty, so neither the poll nor a
        // settle check ran, and focus could flip and return unseen. The key is captured at 1250,
        // recv is entered at 1300. Since raw_t_ms precedes recv_entered_at and the silence since
        // last_observed_at (1000) exceeds the stall allowance, classify_idle_transition
        // force-gaps back to 1000.
        let outcome = classify_idle_transition(1250, 1300, 1000, 1310);
        let (span_from, span_to) = match outcome {
            IdleTransition::ForceGap { span_from, span_to } => (span_from, span_to),
            IdleTransition::Rearm { .. } => panic!("expected ForceGap for a stale, busy-before-recv key"),
        };
        assert_eq!((span_from, span_to), (1000, 1310));

        let mut ledger = Ledger::default();
        ledger.push_check(1600, FieldClass::NonText);
        ledger.push_arrival(span_from);
        ledger.push_arrival(span_to);

        let mut pending: VecDeque<RawKey> = VecDeque::new();
        let mut raw = make_raw_key(1250);
        raw.forced_gap = true;
        pending.push_back(raw);

        let mut rec = Recorder::default();
        let clock = Mutex::new(ActiveClock::new(0));
        let cutoff: Mutex<Option<u64>> = Mutex::new(None);
        let ledger = Mutex::new(ledger);
        resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

        assert!(pending.is_empty());
        assert!(rec.sidecar.events.is_empty());
        assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 1250, end_ms: 1250 }]);
    }

    #[test]
    fn idle_then_prompt_key_still_rearms_and_is_kept() {
        // The genuine idle case: the thread was blocked in recv when the key arrived (raw_t_ms >=
        // recv_entered_at), so the ordinary Rearm branch applies.
        let recv_entered_at = 1150;
        let last_observed_at = 1150; // set when this thread entered that same recv call
        let raw_t_ms = 1200; // arrived while genuinely waiting
        let now = 1200;
        let outcome = classify_idle_transition(raw_t_ms, recv_entered_at, last_observed_at, now);
        let (last_focus_at, next_poll_due) = match outcome {
            IdleTransition::Rearm { last_focus_at, next_poll_due } => (last_focus_at, next_poll_due),
            IdleTransition::ForceGap { .. } => panic!("expected Rearm for a genuinely idle receive"),
        };
        assert_eq!((last_focus_at, next_poll_due), (1200, 1250));

        let mut ledger = Ledger::default();
        ledger.push_check(1600, FieldClass::NonText);
        assert_eq!(verdict(&ledger, 1200, false), Some(Verdict::Allow));

        let mut pending: VecDeque<RawKey> = VecDeque::new();
        pending.push_back(make_raw_key(1200));

        let mut rec = Recorder::default();
        let clock = Mutex::new(ActiveClock::new(0));
        let cutoff: Mutex<Option<u64>> = Mutex::new(None);
        let ledger = Mutex::new(ledger);
        resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, false);

        assert!(pending.is_empty());
        assert!(rec.sidecar.gaps.is_empty(), "a genuinely idle receive must not turn into a recorded gap");
    }

    #[test]
    fn a_brief_busy_stretch_right_after_an_observation_still_rearms() {
        // Busy before recv alone does not force-gap: a short stretch after a real observation
        // barely hides anything. Only silence past the stall allowance escalates to ForceGap.
        let outcome = classify_idle_transition(1210, 1230, 1200, 1235);
        match outcome {
            IdleTransition::Rearm { last_focus_at, next_poll_due } => {
                assert_eq!((last_focus_at, next_poll_due), (1210, 1260));
            }
            IdleTransition::ForceGap { .. } => panic!("expected Rearm for a fresh observation"),
        }
    }

    #[test]
    fn an_unresolved_classification_is_unknown_and_gaps() {
        // A failed or unknown classification resolves to Unknown and gaps unconditionally, proven
        // through the production verdict() pipeline.
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::Unknown);
        ledger.push_check(400, FieldClass::Unknown);
        let v = verdict(&ledger, 50, false).expect("a qualifying check exists");
        assert_eq!(v, Verdict::Gap);
        assert!(resolve_verdict(v, false, KeyId { vk: 0x41, scan: 0, extended: false, ctrl: false, alt: false, win: false }), "an unresolved classification must gap, never record as if it were confirmed text");
    }

    #[test]
    fn a_nontext_field_keeps_plain_letter_keys() {
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::NonText);
        ledger.push_check(400, FieldClass::NonText);
        let v = verdict(&ledger, 50, false).expect("a qualifying check exists");
        assert_eq!(v, Verdict::Allow);
        assert!(!resolve_verdict(v, false, KeyId { vk: 0x41, scan: 0, extended: false, ctrl: false, alt: false, win: false }), "a NonText field's plain typing must keep recording as before");
    }

    #[test]
    fn a_password_field_drops_ctrl_c_too() {
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::Password);
        ledger.push_check(400, FieldClass::Password);
        let v = verdict(&ledger, 50, false).expect("a qualifying check exists");
        assert_eq!(v, Verdict::Gap);
        assert!(resolve_verdict(v, false, KeyId { vk: 0x43, scan: 0, extended: false, ctrl: true, alt: false, win: false }), "shortcuts are blocked in password fields too, not just typed characters");
    }

    // keyOverlayShortcutsOnly applied through resolve_verdict: an absolute filter on top of
    // the verdict, whatever field class produced the Allow, and never relaxing a password gap.

    #[test]
    fn shortcuts_only_drops_a_letter_under_every_allow_producing_field_class() {
        for class in [FieldClass::NonText, FieldClass::Text] {
            let mut ledger = Ledger::default();
            ledger.push_check(0, class);
            ledger.push_check(400, class);
            let v = verdict(&ledger, 50, false).expect("a qualifying check exists");
            assert_eq!(v, Verdict::Allow);
            assert!(
                resolve_verdict(v, true, KeyId { vk: 0x41, scan: 0x1E, extended: false, ctrl: false, alt: false, win: false }),
                "a plain letter must be dropped under shortcuts-only regardless of field class"
            );
        }
    }

    #[test]
    fn shortcuts_only_keeps_ctrl_c_enter_and_f5_through_resolve_verdict() {
        const VK_F5: u32 = 0x74;
        assert!(!resolve_verdict(Verdict::Allow, true, KeyId { vk: 0x43, scan: 0, extended: false, ctrl: true, alt: false, win: false }), "Ctrl+C stays");
        assert!(!resolve_verdict(Verdict::Allow, true, KeyId { vk: VK_RETURN, scan: 0, extended: false, ctrl: false, alt: false, win: false }), "Enter stays");
        assert!(!resolve_verdict(Verdict::Allow, true, KeyId { vk: VK_F5, scan: 0, extended: false, ctrl: false, alt: false, win: false }), "F5 stays");
    }

    #[test]
    fn shortcuts_only_never_relaxes_a_password_gap() {
        assert!(
            resolve_verdict(Verdict::Gap, true, KeyId { vk: 0x43, scan: 0, extended: false, ctrl: true, alt: false, win: false }),
            "a password verdict stays gapped no matter what the shortcuts-only filter would otherwise allow"
        );
    }

    #[test]
    fn shortcuts_only_off_never_changes_the_base_verdict() {
        assert!(!resolve_verdict(Verdict::Allow, false, KeyId { vk: 0x41, scan: 0, extended: false, ctrl: false, alt: false, win: false }), "the setting off must not change existing behavior at all");
    }

    #[test]
    fn shortcuts_only_flag_round_trips_through_json_like_settings_storage() {
        let mut flags = std::collections::HashMap::new();
        flags.insert("keyOverlayShortcutsOnly".to_string(), true);
        let json = serde_json::to_string(&flags).unwrap();
        let restored: std::collections::HashMap<String, bool> = serde_json::from_str(&json).unwrap();
        assert!(shortcuts_only(&restored));
        assert!(!shortcuts_only(&std::collections::HashMap::new()), "absent, default off");
    }

    #[test]
    fn a_tab_key_down_counts_as_a_synthetic_arrival_that_gaps_a_key_with_no_uia_event_at_all() {
        // Tab into a password field, type, Tab out within 300ms, with no UI Automation event for
        // either move. The Tab-out key-down counts as an arrival at the hook's timestamp, so its
        // blind window still gaps the key.
        assert!(is_focus_moving_key(VK_TAB));
        assert!(is_focus_moving_key(VK_RETURN));
        assert!(is_focus_moving_key(VK_ESCAPE));
        assert!(!is_focus_moving_key(0x41)); // an ordinary letter key never counts
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::NonText); // the field looked clean before any of this
        let key_t = 100;
        let tab_out_t = 150; // well within 300ms of the key
        ledger.push_arrival(tab_out_t); // what the hook-time synthetic arrival does
        ledger.push_check(700, FieldClass::NonText); // the consumer's own later recheck, also clean
        assert_eq!(verdict(&ledger, key_t, false), Some(Verdict::Gap), "the synthetic arrival's blind window must still gap the key");
    }

    #[test]
    fn handler_registration_failure_gaps_every_key_regardless_of_checks() {
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::NonText);
        ledger.push_check(400, FieldClass::NonText);
        assert_eq!(verdict(&ledger, 50, false), Some(Verdict::Allow), "before the failure, a clean qualifying check still records");
        ledger.gap_everything();
        assert_eq!(verdict(&ledger, 50, false), Some(Verdict::Gap), "once the handler failed to register, nothing gets recorded again");
        assert_eq!(verdict(&ledger, 1_000_000, false), Some(Verdict::Gap), "including any key far later, however clean its own checks look");
    }

    #[test]
    fn pruning_drops_only_entries_no_pending_or_future_key_could_ever_need_again() {
        let mut ledger = Ledger::default();
        ledger.push_check(0, FieldClass::NonText); // ancient, long since resolved
        ledger.push_arrival(50); // ancient focus change, long since resolved
        ledger.push_check(50, FieldClass::Password);
        // One key already resolved by a qualifying check, one still waiting.
        let resolved_key = 5_000;
        ledger.push_check(resolved_key + SETTLE_MS, FieldClass::NonText);
        let pending_key = 5_200;
        let before_resolved = verdict(&ledger, resolved_key, false);
        let before_pending = verdict(&ledger, pending_key, false);
        let checks_before = ledger.checks.len();
        let arrivals_before = ledger.arrivals.len();

        // Prune to the oldest pending key minus GUARD_MS, as the consumer loop does.
        ledger.prune(pending_key.saturating_sub(GUARD_MS));

        assert!(ledger.checks.len() < checks_before, "the ancient entries must actually be gone");
        assert!(ledger.arrivals.len() < arrivals_before, "the ancient arrival must actually be gone");
        assert_eq!(verdict(&ledger, resolved_key, false), before_resolved, "pruning must never change an already-settled verdict");
        assert_eq!(verdict(&ledger, pending_key, false), before_pending, "pruning must never change a still-pending verdict either");
    }

    #[test]
    fn join_bounded_returns_the_result_once_the_thread_actually_finishes() {
        let handle = thread::spawn(|| {
            thread::sleep(Duration::from_millis(20));
            42
        });
        assert_eq!(join_bounded(handle, Duration::from_millis(2_000)), Some(42));
    }

    #[test]
    fn join_bounded_gives_up_and_detaches_a_thread_that_outlives_the_bound() {
        // A thread running well past the bound stands in for a consumer stuck in a hung UIA call;
        // join_bounded must return None quickly.
        let handle = thread::spawn(|| {
            thread::sleep(Duration::from_millis(500));
            "done"
        });
        let started = Instant::now();
        assert_eq!(join_bounded(handle, Duration::from_millis(50)), None);
        assert!(started.elapsed() < Duration::from_millis(500), "must give up at the bound, not wait for the thread");
    }

    #[test]
    fn settle_and_poll_deadlines_are_never_starved_by_a_burst_of_ok_arrivals() {
        // Key-repeat bursts keep hitting the Ok(raw) arm, so a settle check or focus poll must not
        // depend on the Timeout arm: both are pure functions of absolute time.
        let front_t_ms = 0u64;
        let mut next_poll_due = FOCUS_POLL_MS;
        // Three arrivals 10ms apart, well under FOCUS_POLL_MS.
        for now in [10u64, 20, 30] {
            assert!(!settle_check_due(Some(front_t_ms), now), "not due yet at t={now}");
            assert!(!focus_poll_due(false, now, next_poll_due), "not due yet at t={now}");
        }
        // The settle deadline is reached even though every tick arrived through Ok.
        let settle_due_at = front_t_ms + SETTLE_MS;
        assert!(settle_check_due(Some(front_t_ms), settle_due_at), "must become due exactly at its own deadline");
        // Likewise the poll deadline, independently.
        assert!(focus_poll_due(false, next_poll_due, next_poll_due));
        next_poll_due += FOCUS_POLL_MS;
        assert!(!focus_poll_due(false, next_poll_due - 1, next_poll_due), "not due again until the next tick");
    }

    #[test]
    fn recv_wait_targets_whichever_deadline_is_sooner_and_never_goes_negative() {
        // The settle deadline is sooner (front=100, settle at 400, poll at 1000).
        assert_eq!(recv_wait_ms(0, 100, 1_000), 400);
        // The poll deadline is sooner.
        assert_eq!(recv_wait_ms(0, 10_000, 50), 50);
        // Both deadlines already passed: an immediate wake, no underflow.
        assert_eq!(recv_wait_ms(10_000, 0, 0), 0);
    }

    #[test]
    fn exceeding_the_ledger_hard_cap_fails_closed_and_drops_history() {
        let mut ledger = Ledger::default();
        // A clean check within the settle window, so any later gap comes from the cap, not a dirty
        // window.
        ledger.push_check(0, FieldClass::NonText);
        assert_eq!(verdict(&ledger, 0, false), None, "not yet, no qualifying check exists"); // sanity: still pure, unaffected by the flood below
        for i in 0..LEDGER_HARD_CAP {
            // Arrivals far enough apart that none lands in another's blind window.
            ledger.push_arrival((i as u64) * 10_000);
        }
        assert!(ledger.fail_closed, "the cap must have tripped fail_closed");
        assert!(ledger.checks.is_empty() && ledger.arrivals.is_empty(), "history must actually be dropped, not just capped");
        assert_eq!(verdict(&ledger, 0, false), Some(Verdict::Gap), "every verdict from here on must gap, regardless of any earlier clean check");
        assert_eq!(verdict(&ledger, 50_000_000, false), Some(Verdict::Gap), "including a key far outside every arrival's own window");
    }

    // ActiveClock: pause drops events, and offsets use the active-recording clock, not wall
    // time.

    #[test]
    fn active_clock_with_no_pause_is_plain_elapsed_wall_time() {
        let clock = ActiveClock::new(1_000);
        assert!(!clock.is_paused_at(1_500));
        assert_eq!(clock.active_ms(1_500), 500);
    }

    #[test]
    fn active_clock_marks_a_key_struck_during_a_closed_pause_as_paused() {
        let mut clock = ActiveClock::new(0);
        clock.pause(100);
        clock.resume(300);
        assert!(!clock.is_paused_at(50));
        assert!(clock.is_paused_at(150));
        assert!(clock.is_paused_at(299));
        assert!(!clock.is_paused_at(300));
        assert!(!clock.is_paused_at(400));
    }

    #[test]
    fn active_clock_marks_a_key_struck_during_a_still_open_pause_as_paused() {
        let mut clock = ActiveClock::new(0);
        clock.pause(100);
        // Never resumed: everything after the pause started reads as paused.
        assert!(clock.is_paused_at(100));
        assert!(clock.is_paused_at(10_000));
    }

    #[test]
    fn active_clock_subtracts_paused_spans_from_the_elapsed_offset() {
        let mut clock = ActiveClock::new(0);
        clock.pause(1_000);
        clock.resume(3_000); // 2000ms paused
        // 4000ms after start with 2000ms paused is 2000ms of active time, the same basis as the
        // video track's clock.
        assert_eq!(clock.active_ms(4_000), 2_000);
    }

    #[test]
    fn active_clock_ignores_a_pause_that_starts_after_the_queried_instant() {
        let mut clock = ActiveClock::new(0);
        clock.pause(5_000);
        clock.resume(6_000);
        assert_eq!(clock.active_ms(1_000), 1_000, "a later pause must not affect an earlier timestamp's offset");
    }

    #[test]
    fn active_clock_a_pause_called_twice_in_a_row_does_not_open_a_second_span() {
        let mut clock = ActiveClock::new(0);
        clock.pause(100);
        clock.pause(150); // e.g. a stale double-click of the pause button
        clock.resume(300);
        assert_eq!(clock.pauses.len(), 1);
        assert_eq!(clock.active_ms(400), 200);
    }
}
