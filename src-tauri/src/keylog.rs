use super::append_crash_log;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::mpsc::{channel, sync_channel, Receiver, RecvTimeoutError, Sender, SyncSender};
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

// How long a single UI Automation call may hang before it gives up --
// Without this, a target application that stops pumping
// messages (hung, debugger-broken, a modal native dialog on a
// different apartment) stalls the consumer thread indefinitely, which
// in turn stalls stop() waiting for it to join. Generous enough for a
// slow but healthy app, short enough that a genuinely hung one never
// blocks a recording from stopping for more than about two seconds.
const UIA_TIMEOUT_MS: u32 = 2000;

static CLOCK_START: OnceLock<Instant> = OnceLock::new();

// A monotonic milliseconds clock, immune to a wall-clock jump (NTP, DST,
// the user setting the system clock back). `now_ms()` (SystemTime) can
// move backward; if it did, a later check could look "older" than an
// earlier one and state_at/the verdict algorithm below would silently
// pick a stale entry over a fresh one. EVERY keylog timestamp -- the
// hook, the UI Automation handler, the consumer's checks, ActiveClock,
// and the `start_abs_ms`/cutoff instants a caller outside this module
// passes in -- is stamped from this SAME clock; mixing it with
// `now_ms()` anywhere in here would silently corrupt every comparison.
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
    // The hold time as it stood when this recording
    // started, carried along so the editor's default preset render
    // matches what was configured at capture time without a second
    // settings lookup. `#[serde(default)]` so an older sidecar (before
    // this field existed) still deserializes.
    #[serde(default = "default_hold_ms")]
    pub hold_ms: u32,
}

fn default_hold_ms() -> u32 {
    HOLD_MS
}

// A hard ceiling on how many events one sidecar part can hold. Without
// it an unusually long recording (or a stuck key repeat flooding downs)
// grows `sidecar.events` without bound for as long as the capture runs.
// Reaching it stops adding events for the rest of this part and leaves
// one gap in their place instead, so the overlay still shows something
// happened here rather than silently truncating with no visible sign.
const EVENTS_CAP: usize = 200_000;

// Pure state machine turning a raw (vk, scan, down, t_ms, blocked-at-
// that-instant) stream into the stored sidecar shape. Deliberately free
// of any Windows API so it is directly unit-testable without a real
// hook or a real focused control. `blocked` covers both reasons a key
// must never be stored: a password field was focused at that instant,
// or the key was synthetic (OS-injected) input rather than something a
// person typed -- either one lands in a gap, never as an event.
#[derive(Default)]
struct Recorder {
    sidecar: KeySidecar,
    in_gap: bool,
    // Whether the most recent DOWN seen for a given (vk, scan code) was
    // itself accepted (not blocked). A key-up is judged independently
    // of its down by nothing but its own timestamp otherwise -- which
    // lets a gapped down (password key held, focus moves to a clean
    // field, key released later) hand off a clean-looking up that was
    // never actually typed anywhere safe. An up is only ever stored
    // alongside a down this same Recorder actually accepted; one for a
    // (vk, scan) pair never seen here at all (held from before this
    // capture, or from before this part) is treated the same as a
    // tainted one. Removed on the matching up, so key repeat (several
    // downs, one eventual up) tracks only the most recent down's state.
    open_downs: std::collections::HashMap<(u32, u32), bool>,
}

// A defensive ceiling on `open_downs`. Ordinary typing never holds more
// than a handful of keys at once, so this is not a normal-use limit --
// it exists only so a flood of distinct (vk, scan) pairs (a malicious
// or malfunctioning injector, say) cannot grow the map without bound
// for the rest of the capture.
const OPEN_DOWNS_CAP: usize = 4_096;

impl Recorder {
    fn record(&mut self, vk: u32, scan: u32, down: bool, t_ms: u64, blocked: bool) {
        let final_blocked = if down {
            if blocked {
                // A blocked down never proves anything a later up could
                // rely on -- remove any stale entry instead of storing
                // one, so a flood of blocked (e.g. injected) downs never
                // grows the map at all. A missing entry already reads
                // back as tainted (see `open_downs`'s own doc), so this
                // changes nothing about how the eventual up resolves.
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
        // Once the event cap is reached, treat every further key the
        // same as a blocked one: extend the open gap (or open one)
        // rather than push another event, so `sidecar.events` never
        // grows past EVENTS_CAP no matter how long the part runs.
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

// Monotonic-clock elapsed-since-recording-start pause tracking -- pure
// and independent of any Windows API, so pause/resume behaviour is
// directly unit-testable. keylog used to keep recording straight
// through a pause with raw offsets, so a paused stretch (and anything
// typed during it) leaked into the sidecar at the wrong time. A key
// struck while paused is dropped outright here -- pause means "we know
// nothing should be recorded," which is a different, stronger statement
// than a gap ("we do not know the focus state"). Every timestamp here
// comes from `mono_ms()`, never `now_ms()` -- see `mono_ms`'s own doc.
#[derive(Default)]
struct ActiveClock {
    start_abs_ms: u64,
    // Every paused span so far, in monotonic ms: a closed one once
    // resume() runs, at most one still-open one (`None` end) while
    // actually paused.
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

    // Whether the monotonic instant `t_ms` fell inside a paused span --
    // judged against the key's OWN timestamp, not against "are we paused
    // right now", so a key queued up just before a pause is never
    // mistakenly dropped because the consumer got to it late.
    fn is_paused_at(&self, t_ms: u64) -> bool {
        self.pauses.iter().any(|(s, e)| t_ms >= *s && e.map_or(true, |e| t_ms < e))
    }

    // Active-recording-elapsed ms at `t_ms`: monotonic elapsed since
    // start minus every paused span up to that point -- the same clock
    // basis the video track itself uses, so a stored key timestamp lines
    // up with the frame it was actually struck during.
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

// How long after a key a focus check must have STARTED before that
// check is old enough to actually speak for the key (the "settle"
// delay), and how far around every focus-change arrival nothing is ever
// trusted at all (the "guard"/"blind" window). Both replace the earlier
// both-sides rule, which judged a key from only two points -- the state
// right before it and one recheck run for it -- and so could still
// clear a key on a side that was never actually re-examined at all: the
// per-key recheck ran at CONSUMPTION time, which can lag well behind the
// key itself, and everything the focus did in the meantime was invisible
// to a two-point check.
const SETTLE_MS: u64 = 300;
const GUARD_MS: u64 = 300;

// How often the consumer re-checks focus identity while any key is
// still pending, so a focus change UI Automation's own event missed (or
// never fires at all, see the handler-registration failure path) is
// still caught inside a fraction of the settle window rather than only
// once that window's own deadline finally arrives.
const FOCUS_POLL_MS: u64 = 50;

// A hard cap on how many raw keys can sit in `pending` waiting on a
// verdict at once. Under ordinary typing this never gets close -- keys
// resolve within SETTLE_MS of their own timestamp. Reaching it means
// verdicts have stopped keeping up with the raw key rate for some
// reason this thread cannot diagnose from here, and the safe answer is
// the same one as any other doubt: gap everything rather than let the
// queue (and the memory behind it) grow without bound.
const PENDING_CAP: usize = 2_000;

// How long the consumer can sit idle (nothing pending) before waking up
// on its own just to prune the ledger. Keeps an idle or paused capture
// from silently accumulating focus-changed churn for the rest of the
// recording with nothing ever shrinking it back down.
const IDLE_PRUNE_MS: u64 = 1_000;

// What the focused control was, at the instant a check ran. `Password`
// is the strictest tier (see `verdict`: gaps absolutely everything,
// shortcuts included). `Text` is reached only by a POSITIVE text-entry
// identification -- an Edit or Document control type, an editable
// ValuePattern, or any TextPattern at all. `NonText` is reached only by
// a POSITIVE classification that the control is structurally incapable
// of taking text input (the allow-list in `is_non_text_control_type`)
// AND nothing about it also proved editable text. `Unknown` is
// everything else: no focused element, a failed control-type or
// pattern read, or a control type that is neither positively text nor
// on the non-text allow-list (an unrecognized type, a proxy element, a
// bare Pane/Window/Custom/Group). Product decision (2026-09-26): `Text`
// and `NonText` resolve to the same verdict (see `verdict` below) -- a
// text field is no longer restricted to shortcuts by default, only a
// confirmed password field or an `Unknown` classification still gaps.
// The Text/NonText distinction stays in the classifier because nothing
// downstream of `is_password_field` needs to change to keep proving
// "not a password field" the same rigorous way; it simply no longer
// changes what gets recorded once positively identified as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldClass {
    Password,
    Text,
    NonText,
    Unknown,
}

// The two shapes a resolved key's fate can take. `Gap` is unconditional
// (injected input, the ledger's own fail_closed flag, a blind window, a
// confirmed password field, an `Unknown` classification, or a dirty
// guard window). `Allow` covers both the Text and NonText tiers -- see
// FieldClass's own doc for why they no longer differ here. `resolve_
// verdict` still applies the separate, OPT-IN `keyOverlayShortcutsOnly`
// filter on top of an Allow, regardless of which tier produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Gap,
    Allow,
}

// Ordered log of every completed focus check (`checks`, each stamped by
// the instant it STARTED, not when it finished) and every focus-change
// event's own arrival (`arrivals`), both on the `mono_ms()` clock.
// Replaces the old per-HWND "not password" cache -- Chromium, Edge,
// Electron, WebView2, WPF and UWP all host an entire page/form in ONE
// HWND, so caching an answer by HWND alone let a password field focused
// right after a username field in the same HWND go undetected forever,
// the cache never saw a "new" HWND to invalidate on -- and also the
// earlier `FocusTimeline`'s two-point both-sides rule (see `verdict`
// below for why a single "before" and "after" point was not enough).
#[derive(Default)]
struct Ledger {
    checks: Vec<(u64, FieldClass)>, // (start_ms, class)
    arrivals: Vec<u64>,
    // Set once and never cleared for the lifetime of a capture: the
    // focus-changed handler could not be registered, so no event will
    // ever arrive to invalidate a stale check. See `gap_everything`.
    fail_closed: bool,
}

// A hard ceiling on the ledger's combined size, on top of the ordinary
// GUARD_MS-based pruning. Pruning alone assumes the loop keeps running
// it regularly; an automated or malicious flood of focus changes (or
// pruning simply losing that race for any other reason) could still
// grow the ledger without bound in between. Reaching this cap is
// treated exactly like any other source of doubt: gap everything and
// throw the history away, rather than let memory keep growing.
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

    // Called once, the moment registering the focus-changed handler is
    // known to have failed: from here on every key of this session is
    // gapped outright, the same fate as a confirmed password field,
    // because nothing can ever again prove a key clean.
    fn gap_everything(&mut self) {
        self.fail_closed = true;
    }

    // Checked on every push, from either thread that can add to the
    // ledger (the consumer's own scheduled checks and the UI Automation
    // focus-changed handler both call push_check/push_arrival) -- not
    // just once per consumer loop tick, so a flood of focus-changed
    // events alone, with no keys pending and nothing driving the main
    // loop, still gets caught. Once fail_closed is set, nothing here is
    // ever read again (see `verdict`), so dropping it outright is safe,
    // not just convenient.
    fn enforce_cap(&mut self) {
        if self.checks.len() + self.arrivals.len() > LEDGER_HARD_CAP {
            self.gap_everything();
            self.checks.clear();
            self.checks.shrink_to_fit();
            self.arrivals.clear();
            self.arrivals.shrink_to_fit();
        }
    }

    // The EARLIEST completed check that started late enough (t +
    // SETTLE_MS or later) to actually speak for a key struck at `t`.
    // None means no such check exists YET: the caller must keep
    // waiting for one, never assume "probably fine" just because time
    // is short. `checks` stays sorted by start time (see push_check),
    // so the earliest qualifying one is found with a binary search
    // instead of a linear scan.
    fn qualifying_check(&self, t: u64) -> Option<(u64, FieldClass)> {
        let need = t.saturating_add(SETTLE_MS);
        let idx = self.checks.partition_point(|(start, _)| *start < need);
        self.checks.get(idx).copied()
    }

    // Drops every check and arrival that can no longer matter to any
    // key still pending or yet to arrive: `qualifying_check` never
    // looks at anything before a key's own SETTLE_MS deadline, and
    // `window_is_dirty`/`in_blind_window` never look earlier than
    // GUARD_MS before a key's own timestamp -- so nothing older than
    // the oldest still-pending key minus GUARD_MS can ever be read
    // again. Without this a capture running for the length of a whole
    // recording keeps every focus change and every scheduled check it
    // ever saw, and every single verdict scans all of it.
    fn prune(&mut self, cutoff_ms: u64) {
        let ci = self.checks.partition_point(|(t, _)| *t < cutoff_ms);
        self.checks.drain(0..ci);
        let ai = self.arrivals.partition_point(|t| *t < cutoff_ms);
        self.arrivals.drain(0..ai);
    }

    // Whether a "password" or "unknown" check, or a focus-change
    // arrival, lies anywhere in [from, to] -- checked separately from
    // the single qualifying check above, because a check that itself
    // said "not password" can still be sitting right next to a focus
    // change (or another, dirtier check) that happened in between and
    // that the qualifying check alone, being a single point, cannot
    // see. Unknown counts as dirty here for the same reason Password
    // does: a check the ledger could not positively clear is doubt
    // sitting inside the window, not something a later, cleaner-looking
    // check is allowed to paper over.
    fn window_is_dirty(&self, from: u64, to: u64) -> bool {
        let bad_check = self
            .checks
            .iter()
            .any(|(t, class)| matches!(class, FieldClass::Password | FieldClass::Unknown) && *t >= from && *t <= to);
        let bad_arrival = self.arrivals.iter().any(|t| *t >= from && *t <= to);
        bad_check || bad_arrival
    }

    // The blind window around any focus-change arrival: GUARD_MS
    // before it through SETTLE_MS after -- unconditional, regardless of
    // what any individual check says. UI Automation's own notification
    // can itself lag the real focus change by a similar margin
    // (Chromium/Electron/WebView2 accessibility trees update
    // asynchronously), so a check landing just outside this window is
    // not automatically trustworthy either -- rules 1 and 2 still apply
    // on top of it.
    fn in_blind_window(&self, t: u64) -> bool {
        self.arrivals.iter().any(|a| t >= a.saturating_sub(GUARD_MS) && t <= a.saturating_add(SETTLE_MS))
    }
}

// The binding verdict algorithm. `Some(Verdict::Gap)` is unconditional,
// `Some(Verdict::Allow)` is a record subject only to the opt-in
// shortcuts-only filter (see `resolve_verdict`) -- both the Text and
// NonText tiers reach it, see FieldClass's own doc -- and `None` means
// not enough information exists YET (the caller must keep the key
// pending and wait for a later check) -- never treated as a record,
// only ever eventually resolved to a gap if nothing ever qualifies (see
// `verdict_at_stop`). `Unknown` gaps the same as `Password`: a failed
// read or an unrecognized control type is doubt, not a positive
// text-entry finding, and doubt has never been enough to record.
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

// A key's VK/scan-code/extended-flag identity plus the modifier state it
// carried, bundled purely to keep resolve_verdict/verdict_at_stop's own
// argument count down -- see RawKey for what each field means and
// is_alt_numpad_entry for why scan and extended matter alongside the VK.
#[derive(Clone, Copy)]
struct KeyId {
    vk: u32,
    scan: u32,
    extended: bool,
    ctrl: bool,
    alt: bool,
    win: bool,
}

// Turns a `Verdict` into the final gap/record boolean `Recorder::record`
// takes. `Gap` stays gapped no matter what. An `Allow` is still gapped
// when `keyOverlayShortcutsOnly` is on and this specific key (and the
// modifier state it carried) is not itself a shortcut or navigation key
// -- see `is_kept_by_shortcuts_only` for exactly what that allow-list
// covers. The setting is absolute: it applies on top of the field-class
// verdict, never instead of the password gap above it.
fn resolve_verdict(v: Verdict, shortcuts_only: bool, id: KeyId) -> bool {
    match v {
        Verdict::Gap => true,
        Verdict::Allow => shortcuts_only && !is_kept_by_shortcuts_only(id.vk, id.scan, id.extended, id.ctrl, id.alt, id.win),
    }
}

// Called only once the capture is stopping and no further check will
// ever arrive: a key `verdict` still cannot decide is gapped, the same
// as any other doubt, never assumed safe just because time ran out.
fn verdict_at_stop(ledger: &Ledger, t: u64, injected: bool, id: KeyId, shortcuts_only: bool) -> bool {
    match verdict(ledger, t, injected) {
        Some(v) => resolve_verdict(v, shortcuts_only, id),
        None => true,
    }
}

// finish_and_continue_recording's cutoff: once a switch instant has been
// set, everything at or after it belongs to the switch itself or to the
// next part, never to this capture.
fn past_cutoff(t_ms: u64, cutoff: Option<u64>) -> bool {
    cutoff.is_some_and(|c| t_ms >= c)
}

struct RawKey {
    vk: u32,
    // The hardware scan code alongside the virtual key -- left/right
    // Ctrl, Shift and Alt can share a VK code while remaining genuinely
    // different physical keys, so down/up matching (see Recorder)
    // identifies a key by the pair, not the VK alone.
    scan: u32,
    // LLKHF_EXTENDED: set for the dedicated nav-cluster/arrow keys and
    // the right-hand Ctrl/Alt, clear for the numpad's own keys -- the
    // one signal (alongside the scan code itself) that tells a numpad
    // digit typed with NumLock off apart from the same VK code arriving
    // from the real Insert/Delete/arrow cluster. See NEW-2's
    // is_alt_numpad_entry.
    extended: bool,
    down: bool,
    t_ms: u64,
    // LLKHF_INJECTED: a password manager's auto-type, a
    // remote-control tool, or any other synthetic SendInput stream sets
    // this. Real typing hits a password field first and gets gapped by
    // that check anyway; injected input skips straight to being gapped
    // here since it never went through a person's fingers at all.
    injected: bool,
    // Ctrl/Alt/Win held at this exact key's own hook-time timestamp --
    // see ModifierState. Used only by the opt-in shortcuts-only filter
    // (is_kept_by_shortcuts_only); irrelevant, but always present, when
    // that setting is off.
    ctrl: bool,
    alt: bool,
    win: bool,
    // Set after arrival, by the consumer's own focus
    // poll, when this key's own timestamp falls inside a span the poll
    // cannot vouch for (a detected focus change, or the poll itself
    // running late enough to mean the consumer was stuck) -- see
    // mark_forced_gaps. Once set this key gaps unconditionally,
    // bypassing the ledger-based verdict entirely, because the span
    // that made it suspect can be far wider than any single arrival's
    // own blind window.
    forced_gap: bool,
}

// Tab, Shift+Tab (same VK code as Tab -- the shift state never changes
// it), Enter and Esc all commonly move focus on their own, without
// UI Automation firing its own notification for it right away: a form
// submitting on Enter, a dialog closing on Esc, a field-to-field Tab.
// The consumer counts the key-down itself as an arrival, at the hook's
// own timestamp, so its blind window covers the moment focus actually
// moves instead of only whenever UI Automation gets around to saying so.
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;

fn is_focus_moving_key(vk: u32) -> bool {
    matches!(vk, VK_TAB | VK_RETURN | VK_ESCAPE)
}

// The non-character keys the shortcuts-only filter still keeps
// regardless of any modifier: they navigate or edit structurally
// (cursor movement, deleting, paging) rather than producing a
// character, and F-keys are conventionally shortcuts even bare.
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

// Ctrl, Alt/Menu, Win and Shift, generic plus left/right variants --
// used both to track live modifier state (see ModifierState) and to
// keep a bare modifier key-press itself from ever counting as its own
// "combo" in `is_kept_by_shortcuts_only`.
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

// NEW-2: the numpad's own physical keys, whichever VK they currently
// send. With NumLock on they send VK_NUMPAD0..VK_DIVIDE directly; with
// it off the very same physical keys send the nav-cluster VKs instead
// (VK_INSERT, VK_END, arrows, ...) while keeping their own non-extended
// scan code -- the real Insert/Delete/arrow-cluster keys send those same
// VKs too, but always with LLKHF_EXTENDED set. So scan code plus the
// extended flag, not the VK alone, is what actually identifies a numpad
// key here.
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

// Whether this key, struck while Alt alone is held, is (or could be) a
// step of an Alt+Numpad Unicode/code-page character entry rather than an
// ordinary Alt+key shortcut -- the classic Alt+<decimal> sequence, or
// EnableHexNumpad's hex-digit variant, both compose an arbitrary
// character out of a run of individually plausible-looking Alt+key
// combinations. Matches on the VK (NumLock on) OR the scan code (NumLock
// off, arriving as a nav-cluster VK) -- either alone is enough.
fn is_alt_numpad_entry(vk: u32, scan: u32, extended: bool) -> bool {
    is_numpad_vk(vk) || is_numpad_scan(scan, extended)
}

// The `keyOverlayShortcutsOnly` allow-list: a non-character key kept
// unconditionally, or a non-modifier key held together with Ctrl, Alt
// or Win -- EXCEPT bare Ctrl+Alt with no Win, which Windows uses to
// emulate AltGr (e.g. Ctrl+Alt+Q types "@" on a German keyboard) and
// must be treated as ordinary typing, not a shortcut, and EXCEPT Alt
// alone over a numpad key, which composes a character instead (see
// is_alt_numpad_entry) and is checked before the nav-key allow below so
// it cannot be shadowed by VK_INSERT/VK_LEFT/etc sharing those VK codes
// with NumLock off. A lone modifier press (no other key held with it)
// is never itself a shortcut. Everything else -- letters, digits,
// punctuation, space, Shift+char, dead keys -- falls through and is
// dropped. Applied absolutely, regardless of which FieldClass produced
// the Allow this filters (see `resolve_verdict`) -- a password field is
// still gapped outright before this is ever reached.
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

// Tracks which of Ctrl, Alt and Win are currently held, purely from the
// hook thread's own serialized stream of key-down/up events -- Windows
// calls a low-level hook's procedure one event at a time on the thread
// that installed it, so nothing here races against itself. Reset to
// "nothing held" every time the hook is (re)installed (see
// `install_hook`): while unhooked (paused, or before the first key)
// this thread sees no events at all, so any state carried across that
// gap could just as easily be stale as correct -- and stale-true would
// fail OPEN (wrongly treating a plain character as part of a shortcut),
// which is the one direction that must never happen here.
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

// A synthetic (SendInput) modifier press or release must never update the
// tracked state: an injected Ctrl-down with no matching real key on the
// keyboard would otherwise let a script hold a modifier "on" in this
// hook's eyes forever, or mask a real modifier's own up.
fn should_track_modifier_event(injected: bool) -> bool {
    !injected
}

// NEW-1: the tracked state alone can go stale -- a key-up the hook never
// saw (the matching window was elevated under UIPI, or the up happened
// on the secure desktop during Win+L) leaves `tracked` stuck true with
// no event ever able to clear it. Confirmed here against the live
// keyboard state (GetAsyncKeyState) before either is trusted: only
// "tracked says held AND the hardware says held right now" counts as
// held. Both sides fail closed -- `tracked` starts false and only a real
// down sets it, and `async_down` defaults to false wherever the OS call
// cannot be made (see the only caller, in hook_proc) -- so any doubt
// resolves to "not held", never to a shortcut being wrongly kept.
fn confirm_modifier(tracked: bool, async_down: bool) -> bool {
    tracked && async_down
}

// Bounds how many raw keys can sit in the channel between the hook
// thread and the consumer before the hook starts dropping them. A
// bounded `SyncSender` paired with `try_send` (never `send`) means the
// hook thread -- which every keystroke on the whole desktop passes
// through -- can never block waiting for room, only drop the newest
// event and move on; a dropped key-down simply never taints anything
// (see the down/up matching in `Recorder`), and a dropped key-up is no
// different from one the consumer already treats as unmatched.
const RAW_CHANNEL_CAP: usize = 4_096;

// The hook callback is a bare extern "system" fn -- it cannot capture a
// closure, so the channel it feeds has to be a process-global. Read once
// per event and cloned immediately, never held across the send: the
// callback must return quickly.
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
            // Update the shared modifier state (if this event IS one of
            // Ctrl/Alt/Win, and is not itself injected -- see
            // should_track_modifier_event) before snapshotting it, so
            // this key's own down/up is already reflected -- and so a
            // companion key arriving right after sees it too.
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
            // NEW-1: confirmed against the live keyboard, not trusted on
            // the tracked state alone -- see confirm_modifier.
            let ctrl = confirm_modifier(tracked_ctrl, GetAsyncKeyState(VK_CONTROL as i32) < 0);
            let alt = confirm_modifier(tracked_alt, GetAsyncKeyState(VK_MENU as i32) < 0);
            let win = confirm_modifier(tracked_win, GetAsyncKeyState(VK_LWIN as i32) < 0 || GetAsyncKeyState(VK_RWIN as i32) < 0);
            if let Ok(guard) = raw_tx_slot().lock() {
                if let Some(tx) = guard.as_ref() {
                    // try_send: this callback must never block, a full
                    // channel just means the newest event is dropped.
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
        // WH_KEYBOARD_LL only ever fires while its installing thread pumps
        // a message loop -- this loop exists to keep that true and to
        // notice the WM_QUIT stop() posts to unwind it.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
        let _ = UnhookWindowsHookEx(hook);
    }
}

// The currently focused HWND, resolved the same way for both the
// password check below and the focus-identity poll: `hwndFocus` from
// the foreground thread's own GUI info when it has one, the foreground
// window itself otherwise.
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

// The CURRENT focused control's classification, resolved fresh every
// call -- no cache of any kind, HWND-keyed or otherwise (see Ledger's
// doc for why that used to be unsafe). Called both by the UIA
// focus-changed event handler and by the consumer's own scheduled
// checks, so there is exactly one code path deciding a field's class no
// matter which of the two triggered it.
fn classify_focused_field(uia: &Option<IUIAutomation>) -> FieldClass {
    classify_identity(&focus_identity(uia))
}

// A focus "identity" cheap enough to poll every FOCUS_POLL_MS while keys
// are pending: the currently focused HWND alongside the UI Automation
// element behind it, if any. Compared through `focus_identity_changed`
// using UI Automation's own CompareElements -- its documented way to
// tell whether two references name the same underlying control, a true
// runtime-id comparison rather than a property one two different
// controls could share.
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
        // No UI Automation object at all to compare elements with --
        // the HWND check above is already everything available.
        (Some(_), Some(_), None) => false,
    }
}

// FAIL CLOSED throughout: any doubt (UI Automation unreachable, the call
// erroring, no focused window at all) is treated as a password field.
// Never record anything while in doubt. Takes the focused element
// ALREADY fetched by the caller (see classify_identity) rather than
// fetching its own -- see NEW-4.
fn is_password_field(hwnd: HWND, el: Option<&IUIAutomationElement>) -> bool {
    let uia_password = el.and_then(|el| unsafe { el.CurrentIsPassword() }.ok());
    match uia_password {
        None => return true, // UI Automation unreachable -- doubt means password.
        Some(b) if b.as_bool() => return true,
        Some(_) => {}
    }
    // UI Automation says no, but some legacy edit controls never expose
    // Password through it -- the classic style is the second signal.
    if hwnd.0.is_null() {
        return true;
    }
    (unsafe { GetWindowLongW(hwnd, GWL_STYLE) } & ES_PASSWORD) != 0
}

// Whether the focused element can be POSITIVELY ruled out as a
// text-entry control -- the only way FieldClass::NonText is ever
// reached. Everything else, including every shape of doubt, falls to
// FieldClass::Text in `classify_field`.
//
// NEW-3: this used to be "not Edit/Document and exposes no
// ValuePattern/TextPattern" -- absence of evidence, not a positive
// classification. A custom-drawn terminal (PuTTY/KiTTY), an RDP/VNC
// remote session window, or any Qt/Java/game window with no
// accessibility provider at all surfaces as a bare Pane/Window/Custom
// element with none of those patterns either, and used to be
// (mis)classified NonText -- typed characters, including an SSH or RDP
// password, went through in full. Inverted to a POSITIVE allow-list
// instead: only a control type that structurally cannot accept text
// input counts as non-text; a Pane, Window, Custom, Group, an unknown
// type, or any CurrentControlType/GetFocusedElement failure all fall
// through to Text, same as before.
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

// Positive classification of a non-password focused element into Text,
// NonText or Unknown. Takes the element ALREADY fetched by the caller
// (see classify_identity) rather than fetching its own -- see NEW-4.
//
// Text requires actual positive evidence: an Edit or Document control
// type, an editable ValuePattern, or any TextPattern at all (even a
// read-only one -- its mere presence means editable text cannot be
// ruled out). NonText requires a control type on the structurally-
// incapable-of-text allow-list AND no pattern read proved otherwise.
// Anything else -- no element, a failed control-type or pattern read,
// or a control type on neither list -- is Unknown. This used to
// collapse every one of those doubt cases to Text (a failed read, an
// unrecognized control type, a bare Pane/Window/Custom/Group with no
// pattern support all looked identical to a normal Edit box), which let
// an unclassifiable focused control record in full. Doubt now gaps,
// exactly like a confirmed password field, and Text is reached only by
// a genuine positive identification.
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
            Ok(_) => {}                                                       // read-only ValuePattern -- keep checking
            Err(_) => return FieldClass::Unknown,                             // failed read -- doubt, not a positive finding
        }
    }
    if unsafe { el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }.is_ok() {
        return FieldClass::Text; // a TextPattern exists at all -- cannot rule out editable text
    }
    if is_non_text_control_type(control_type) {
        FieldClass::NonText
    } else {
        FieldClass::Unknown
    }
}

// The full classification, given an identity ALREADY fetched once by
// the caller (see focus_identity) -- NEW-4: is_password_field and
// classify_control used to each call GetFocusedElement independently,
// so a focus change landing between the two reads could hand them two
// different elements without either read itself failing.
// FieldClass::Password takes priority over everything (the existing,
// unchanged `is_password_field` fail-closed rule); everything else goes
// through `classify_control` -- see its own doc for the Text/NonText/
// Unknown split.
fn classify_identity(identity: &(HWND, Option<IUIAutomationElement>)) -> FieldClass {
    let (hwnd, el) = identity;
    if is_password_field(*hwnd, el.as_ref()) {
        return FieldClass::Password;
    }
    classify_control(el.as_ref())
}

// NEW-4: a check is stamped as having STARTED at some instant, but
// GetFocusedElement and the CurrentIsPassword/CurrentControlType/
// GetCurrentPatternAs reads that follow are not guaranteed fast --
// any of them can block up to UIA_TIMEOUT_MS. If the focused identity
// changed between the first read (before classify_identity) and the
// last (after it), the element(s) this check actually queried may
// already belong to the field the user left, not the one now focused,
// and nothing it concluded can be trusted. That is treated exactly
// like any other doubt: forced to FieldClass::Password, the same
// fail-closed answer as a confirmed password field -- and the caller
// is told so (the returned bool) so it can also push an arrival at the
// check's own start instant, opening a real blind window around it
// the same as a genuine focus-changed event would.
fn classify_checked(uia: &Option<IUIAutomation>) -> (FieldClass, bool) {
    let before = focus_identity(uia);
    let class = classify_identity(&before);
    let after = focus_identity(uia);
    let changed = focus_identity_changed(uia, &before, &after);
    (if changed { FieldClass::Password } else { class }, changed)
}

// The UI Automation focus-changed event's own handler object: the OS
// calls this the moment the focused ELEMENT changes
// anywhere on the desktop, in or out of process, same HWND or not. Each
// firing re-resolves the password state through the exact same
// `focused_password` path as the consumer's own scheduled checks and
// records both its arrival and its result in the shared ledger -- an
// in-page field switch (one HWND, two different controls) invalidates
// just as reliably as switching windows, because nothing here is keyed
// by HWND at all.
#[implement(IUIAutomationFocusChangedEventHandler)]
struct FocusHandler {
    ledger: Arc<Mutex<Ledger>>,
    uia: IUIAutomation,
}

impl IUIAutomationFocusChangedEventHandler_Impl for FocusHandler_Impl {
    fn HandleFocusChangedEvent(&self, _sender: Option<&IUIAutomationElement>) -> windows::core::Result<()> {
        // The arrival instant is recorded FIRST, before the check
        // itself runs -- the blind window and the dirty-window scan
        // both key off this arrival time, never off whenever
        // CurrentIsPassword happens to finish (it can take up to
        // UIA_TIMEOUT_MS). Deliberately ignores `_sender` and
        // re-derives through the GetForegroundWindow/GetGUIThreadInfo
        // path instead -- one source of truth, and it also picks up
        // the ES_PASSWORD legacy fallback that a bare element
        // reference cannot.
        //
        // The timestamp itself is read while the ledger lock is
        // already held, and pushed before it is released -- otherwise
        // this thread could be preempted between reading the time and
        // making the arrival visible, and the consumer could resolve a
        // key against a check that started after that instant while
        // the arrival itself was still missing from the ledger.
        let arrived_at = {
            let mut guard = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
            let now = mono_ms();
            guard.push_arrival(now);
            now
        };
        let class = classify_focused_field(&Some(self.uia.clone()));
        // This check is recorded as having STARTED at the arrival
        // instant, not whenever it happened to finish -- it
        // conceptually began the moment focus was noticed.
        self.ledger.lock().unwrap_or_else(|e| e.into_inner()).push_check(arrived_at, class);
        Ok(())
    }
}

// How long the consumer should block in `recv_timeout` while a key is
// pending: never longer than whichever comes first, the pending key's
// own settle deadline or the next focus-poll tick, and never negative
// (both can already be in the past when this runs a little late, in
// which case it returns 0 -- an immediate wake, not a panic from
// underflow).
fn recv_wait_ms(now: u64, pending_front_t_ms: u64, next_poll_due: u64) -> u64 {
    pending_front_t_ms.saturating_add(SETTLE_MS).min(next_poll_due).saturating_sub(now)
}

// Whether the pending key at the front of the queue has reached its own
// SETTLE_MS deadline and a fresh settle check should run for it. A pure
// predicate over absolute time so it gives the same answer regardless
// of whether this loop tick was woken by an incoming key or a timeout
// -- see consumer_thread_main's own comment on why that distinction
// used to matter and must not.
fn settle_check_due(pending_front_t_ms: Option<u64>, now: u64) -> bool {
    pending_front_t_ms.is_some_and(|t| now >= t.saturating_add(SETTLE_MS))
}

// Whether the ~50ms focus poll is due: only meaningful while something
// is actually pending (nothing to invalidate a verdict for otherwise),
// and again a pure predicate over absolute time, independent of which
// branch woke the loop up this tick.
fn focus_poll_due(pending_is_empty: bool, now: u64, next_poll_due: u64) -> bool {
    !pending_is_empty && now >= next_poll_due
}

// A poll running this much later than its own scheduled
// deadline means the consumer thread was stuck somewhere in between --
// a slow UI Automation call (up to UIA_TIMEOUT_MS) is the only thing on
// this thread that can block for any real length of time. Treated as
// "unknown for the whole stalled span", same as an actually-detected
// identity change, because a long enough stall can hide a focus change
// focus_identity_changed itself would otherwise have caught on time --
// an A -> B -> A flip landing entirely inside the stall still compares
// as "unchanged" at the two endpoints the poll actually got to read.
fn poll_overran(now: u64, next_poll_due: u64) -> bool {
    now.saturating_sub(next_poll_due) > FOCUS_POLL_MS
}

// The safe half of the empty->non-empty transition (see
// classify_idle_transition, which is what actually decides whether
// this is the branch to take): rearms the poll deadline from the
// key's OWN hook-time timestamp (`raw_t_ms`), never from mono_ms() at
// the moment this thread gets around to receiving it, so a poll that
// runs shortly afterward is judged against when the key was actually
// captured rather than against a next_poll_due left stale by however
// long pending had been sitting empty for a completely unrelated
// reason. last_focus itself is deliberately left untouched: it keeps
// whatever was last confirmed, so a poll that later runs still
// compares against a real baseline rather than a read taken here that
// could already be on the far side of a change this thread never got
// to see.
fn idle_to_busy_reset(raw_t_ms: u64) -> (u64, u64) {
    (raw_t_ms, raw_t_ms.saturating_add(FOCUS_POLL_MS))
}

// What the empty->non-empty transition does with the arriving key.
// Rearm is the ordinary case: nothing to hide, the poll baseline
// simply moves forward. ForceGap is chosen when this key cannot be
// trusted to a fresh baseline at all, and instead has to be judged
// against the real span [span_from, span_to] the same way a genuinely
// busy consumer's stall already is (see mark_forced_gaps).
enum IdleTransition {
    Rearm { last_focus_at: u64, next_poll_due: u64 },
    ForceGap { span_from: u64, span_to: u64 },
}

// Decides which half of the empty->non-empty transition applies.
//
// `recv_entered_at` is the instant this thread most recently called
// rx.recv_timeout with pending genuinely empty; `last_observed_at` is
// the last instant focus was ACTUALLY looked at (a poll tick that ran
// to completion, or that same blocking-recv instant -- see
// last_observed_at's own doc). Rearming from the key's own timestamp
// (idle_to_busy_reset) is only safe when this thread was truly idle,
// waiting in that recv call, WHEN THE KEY ARRIVED -- i.e. raw_t_ms is
// no earlier than recv_entered_at. If raw_t_ms already precedes
// recv_entered_at, the key was sitting in the channel before this
// thread even started waiting for it: something else had this thread
// busy, not blocked, and pending being empty during that stretch
// means nothing was there to poll for or classify (see focus_poll_due
// and settle_check_due, both gated on pending being non-empty), so no
// check ran the whole time. A short busy stretch right after a real
// observation is not itself dangerous -- there was barely any window
// for a focus round trip to hide in -- so that case still rearms
// normally; only once the silence since last_observed_at exceeds the
// same stall allowance poll_overran already enforces elsewhere does
// this key get force-gapped across the real, unshortened span back to
// last_observed_at instead of a fresh one starting at raw_t_ms, which
// would otherwise erase the only evidence that nothing was watched
// since the busy stretch began.
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

// Whether a key's own timestamp falls inside a span this
// poll cannot vouch for, widened by GUARD_MS on both sides the same way
// every other blind window here is -- used to mark still-pending keys
// forced_gap (see mark_forced_gaps) rather than trust a single point-in
// -time arrival to cover a span that can be far wider than SETTLE_MS.
fn key_in_widened_span(t_ms: u64, span_from: u64, span_to: u64) -> bool {
    t_ms >= span_from.saturating_sub(GUARD_MS) && t_ms <= span_to.saturating_add(GUARD_MS)
}

// Marks every still-pending key whose own timestamp falls inside
// [span_from, span_to] (already widened by GUARD_MS) forced_gap -- see
// RawKey's own doc. Called once per poll tick that could not vouch for
// the whole interval since the last one it confirmed unchanged.
fn mark_forced_gaps(pending: &mut VecDeque<RawKey>, span_from: u64, span_to: u64) {
    for raw in pending.iter_mut() {
        if key_in_widened_span(raw.t_ms, span_from, span_to) {
            raw.forced_gap = true;
        }
    }
}

// Resolves as many keys at the FRONT of `pending` as the ledger's
// current state allows, in order, and stops at the first one `verdict`
// cannot decide yet. FIFO is safe here: a later key always needs an
// even-later qualifying check than an earlier one (SETTLE_MS is added
// to the key's OWN timestamp), so the front of this queue can never be
// stuck behind a later key that happens to resolve first.
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
        // A cutoff can be set AFTER a key was already queued here (the
        // switch instant is only known once finish_and_continue_recording
        // actually runs) -- checked fresh on every resolution pass, not
        // only once at push time.
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

// The UI Automation classes to try, in order: CUIAutomation8 is the one
// that serves IUIAutomation2 everywhere, the original class only on some
// builds.
pub(crate) fn uia_class_order() -> [windows::core::GUID; 2] {
    [CUIAutomation8, CUIAutomation]
}

// Returns the first class for which `try_class` produces a ready object;
// later classes are not touched once one worked.
pub(crate) fn first_ready_uia<T>(classes: &[windows::core::GUID], try_class: impl FnMut(&windows::core::GUID) -> Option<T>) -> Option<T> {
    classes.iter().find_map(try_class)
}

fn consumer_thread_main(rx: Receiver<RawKey>, clock: Arc<Mutex<ActiveClock>>, cutoff: Arc<Mutex<Option<u64>>>, shortcuts_only: bool) -> KeySidecar {
    // MTA, not STA: an apartment-threaded COM object needs its own
    // message pump for cross-thread calls (the focus-changed event
    // above fires from a different thread) to ever be delivered, and
    // this thread runs no pump beyond its own blocking channel
    // recv()/recv_timeout() loop. MTA calls land directly instead.
    //
    // Every step from here through the two timeout setters must
    // actually succeed before this thread makes a single further UIA
    // call: SetConnectionTimeout/SetTransactionTimeout exist precisely
    // to keep a hung target application from blocking a UIA call (and
    // therefore stop()) forever, so if they cannot be confirmed to be
    // in effect, no UIA call is safe to make at all -- `uia` is thrown
    // away and every check falls back to `focused_password`'s own
    // fail-closed answer for a missing UI Automation object.
    let co_init_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let mut uia: Option<IUIAutomation> = None;
    let mut uia_ready = false;
    if co_init_ok {
        // CUIAutomation8 first: on some Windows 11 builds the plain
        // CUIAutomation class does not answer for IUIAutomation2
        // (E_NOINTERFACE) while CUIAutomation8 does. The first class that
        // yields a fully bounded object wins; none -> the gap-everything
        // path below stays the last resort.
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
        // Same fail-closed outcome as a failed handler registration
        // below, reached here instead because there is no bounded UIA
        // object to even try registering a handler on.
        ledger.lock().unwrap_or_else(|e| e.into_inner()).gap_everything();
        append_crash_log("key overlay: could not set up UI Automation with a bounded timeout, gapping every key this session");
    }
    // Confirm the focus state once, synchronously, before the first key
    // can possibly be judged -- see Ledger::qualifying_check: a key with
    // no check starting at or after its own SETTLE_MS deadline has no
    // qualifying check at all yet, and is never guessed at.
    ledger.lock().unwrap_or_else(|e| e.into_inner()).push_check(mono_ms(), classify_focused_field(&uia));
    let focus_handler: Option<IUIAutomationFocusChangedEventHandler> =
        uia.as_ref().map(|uia| FocusHandler { ledger: ledger.clone(), uia: uia.clone() }.into());
    if let (Some(uia), Some(handler)) = (&uia, &focus_handler) {
        if unsafe { uia.AddFocusChangedEventHandler(None, handler) }.is_err() {
            // With no focus-changed event ever going to arrive, a check
            // that happens to land in the quiet stretch between two
            // real focus moves has nothing left to invalidate it -- the
            // scheduled checks and the focus poll below both still run,
            // but neither is a substitute for the event itself catching
            // an in-page field switch the instant it happens. Gapping
            // every key for the rest of this session is the only answer
            // that never depends on a signal already known to be
            // unavailable.
            ledger.lock().unwrap_or_else(|e| e.into_inner()).gap_everything();
            append_crash_log("key overlay: could not register the UI Automation focus-changed handler, gapping every key this session");
        }
    }

    let mut rec = Recorder::default();
    // Keys not yet resolved by the verdict algorithm -- see
    // `resolve_pending`. A key sits here until a check has STARTED at
    // least SETTLE_MS after it, which is inherently a delay: the
    // consumer cannot decide a key's fate the instant it arrives.
    let mut pending: VecDeque<RawKey> = VecDeque::new();
    // The focus identity as of the last poll tick, so the tick after a
    // real change can tell it happened. Established fresh here, before
    // any key can possibly be pending, so the very first poll compares
    // against a real baseline rather than an assumed one.
    let mut last_focus = focus_identity(&uia);
    // NEW-4: the instant `last_focus` was last confirmed unchanged --
    // when the poll below detects a change, the real change could have
    // happened anywhere between this instant and now, so the arrival is
    // stamped here (the last known-good moment), not at whenever this
    // tick's own (possibly slow) GetFocusedElement call returns. Starts
    // alongside `last_focus` for the same reason: the first poll needs
    // a real baseline, not an assumed one.
    let mut last_focus_at = mono_ms();
    // The next absolute instant the focus poll is due, tracked
    // independently of whatever wakes the loop up -- see the
    // unconditional check/poll block below for why this has to be an
    // absolute deadline rather than "did the Timeout arm just fire".
    let mut next_poll_due = mono_ms().saturating_add(FOCUS_POLL_MS);
    // The last instant focus was ACTUALLY looked at -- by a poll tick
    // that ran to completion, or by this thread entering the blocking
    // recv below with pending genuinely empty (nothing yet existed to
    // hide while sitting there). Unlike last_focus_at, this is never
    // rearmed just because a key arrived; see classify_idle_transition
    // for why the distinction matters. Starts alongside last_focus for
    // the same reason both of those do.
    let mut last_observed_at = mono_ms();
    loop {
        // Block until either the next raw key, or a timeout -- while
        // keys are pending the timeout is also the ~50ms focus poll
        // below, never longer, so a focus change UI Automation's own
        // event missed is still caught quickly. With nothing pending
        // there is nothing to poll for, but the wait still cannot be
        // unbounded: the focus-changed handler keeps firing on its own
        // thread regardless of pause state or whether any key is
        // waiting, so the loop still needs to wake up periodically
        // purely to prune what it has added (see the prune call at the
        // bottom of this loop) -- otherwise an idle or paused capture
        // sitting through a lot of automated focus churn would grow the
        // ledger for as long as nothing else woke this thread up.
        // Captured before the call below, whichever arm runs -- the
        // None arm is the one that matters for
        // classify_idle_transition: reaching it means this thread was
        // actually about to block with nothing pending, not busy doing
        // something else, so that instant doubles as a real
        // observation (see last_observed_at's own doc).
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
                    // Nothing happened here as far as the recording is
                    // concerned -- dropped outright, not even a gap (a
                    // gap means doubt; a pause means certainty that this
                    // moment is not part of the active recording at
                    // all). In practice the hook is uninstalled for the
                    // whole pause too (see KeyCapture::hook), so this
                    // only ever catches the sliver of events already in
                    // flight the instant pause() tore it down.
                } else {
                    // See `is_focus_moving_key`: a Tab, Shift+Tab, Enter
                    // or Esc key-down is itself treated as a focus
                    // arrival, stamped at the hook's own timestamp
                    // rather than whenever this thread gets around to
                    // seeing it.
                    if raw.down && is_focus_moving_key(raw.vk) {
                        ledger.lock().unwrap_or_else(|e| e.into_inner()).push_arrival(raw.t_ms);
                    }
                    if pending.is_empty() {
                        // Idle->busy transition -- see
                        // classify_idle_transition for the two
                        // outcomes: either this key can safely rearm
                        // the poll baseline from its own hook
                        // timestamp, or it was already sitting in the
                        // channel while this thread was busy with
                        // something other than waiting for it, for too
                        // long since focus was last actually observed
                        // to trust an endpoint comparison alone.
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
                        // The queue growing this large means verdicts
                        // have stopped keeping up with the raw key rate
                        // for some reason this thread cannot diagnose --
                        // treated exactly like any other unresolvable
                        // doubt: gap everything outright (resolved
                        // below by the ordinary resolve_pending call,
                        // which drains the whole backlog in one pass
                        // once fail_closed is set) rather than let
                        // memory grow without bound.
                        ledger.lock().unwrap_or_else(|e| e.into_inner()).gap_everything();
                        append_crash_log("key overlay: pending queue exceeded its cap, gapping every key this session");
                    }
                    pending.push_back(raw);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        // Both of these run on their OWN absolute deadline every single
        // iteration, whether it was an incoming key or a timeout that
        // woke this one up -- never gated to only the Timeout arm.
        // Hardware key repeat can deliver `Ok(raw)` well under
        // FOCUS_POLL_MS apart; gating either check to the Timeout arm
        // let a long enough burst of repeats starve both of them, so a
        // check landing after the burst ended could end up "confirming"
        // a field that had actually gone dirty and clean again while
        // nothing was watching in between.
        let now = mono_ms();
        if settle_check_due(pending.front().map(|front| front.t_ms), now) {
            // NEW-4: classify_checked fetches the focused element ONCE
            // and confirms the identity has not moved on between before
            // and after -- a plain classify_focused_field here would
            // still be stamped as having started at `now` while its own
            // reads (which can block up to UIA_TIMEOUT_MS) run later,
            // so a focus change landing in that gap used to go
            // completely unseen by this check. A confirmed change is
            // recorded as an extra arrival at `now` too (the check's own
            // start), opening a real blind window around it exactly
            // like a genuine focus-changed event would.
            let (class, invalidated) = classify_checked(&uia);
            let mut guard = ledger.lock().unwrap_or_else(|e| e.into_inner());
            guard.push_check(now, class);
            if invalidated {
                guard.push_arrival(now);
            }
        }
        // The ~50ms focus poll: while any key is still waiting on a
        // verdict, check whether the focused HWND or element has moved
        // since the last tick and, if so, record it as an arrival the
        // same as a UI Automation notification would be. This is what
        // catches a focus change during a quiet stretch with no key at
        // all driving a settle check, and it is also the fallback for
        // whenever AddFocusChangedEventHandler never fired to begin
        // with.
        if focus_poll_due(pending.is_empty(), now, next_poll_due) {
            let cur = focus_identity(&uia);
            let changed = focus_identity_changed(&uia, &last_focus, &cur);
            // A stalled poll is treated exactly like a
            // detected change -- see poll_overran's own doc for why a
            // stall alone, with no change ever actually observed, is
            // still not safe to treat as "confirmed unchanged".
            let stalled = poll_overran(now, next_poll_due);
            if changed || stalled {
                // The real change (or, if only stalled, whatever this
                // poll simply could not see) could have happened
                // ANYWHERE in (last_focus_at, now], not just at one of
                // the two endpoints -- a slow classify_checked or a
                // stuck UI Automation call can make that span far wider
                // than SETTLE_MS/GUARD_MS could ever cover from a
                // single point arrival. Every key still pending inside
                // the span, widened by GUARD_MS on both sides, is
                // marked to gap outright regardless of what the
                // ledger's own verdict would otherwise say, and both
                // ends are also recorded as ordinary arrivals for
                // whatever key is still to come.
                // mark_forced_gaps widens by GUARD_MS on both sides
                // itself (key_in_widened_span) -- the raw endpoints go
                // in here unwidened, or the span would double-widen.
                mark_forced_gaps(&mut pending, last_focus_at, now);
                let mut guard = ledger.lock().unwrap_or_else(|e| e.into_inner());
                guard.push_arrival(last_focus_at);
                guard.push_arrival(now);
            }
            last_focus = cur;
            last_focus_at = now;
            next_poll_due = now.saturating_add(FOCUS_POLL_MS);
            // A poll that ran to completion, whatever it found, is
            // itself a real observation -- see last_observed_at's own
            // doc and classify_idle_transition.
            last_observed_at = now;
        }
        resolve_pending(&mut pending, &mut rec, &clock, &cutoff, &ledger, shortcuts_only);
        // Nothing before the oldest still-pending key's own GUARD_MS
        // lookback can ever be read again -- see Ledger::prune.
        let prune_before = pending.front().map(|k| k.t_ms).unwrap_or_else(mono_ms).saturating_sub(GUARD_MS);
        ledger.lock().unwrap_or_else(|e| e.into_inner()).prune(prune_before);
    }
    // The channel is gone -- stop() already uninstalled the hook and no
    // further check will ever run, so nothing still pending gets to
    // wait any longer. A hung target application must not turn each
    // leftover key into its own live UIA timeout right when the user is
    // waiting for the recording to actually stop; `verdict_at_stop`
    // still honors a qualifying check that already completed in time,
    // but never starts a new one.
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
    // Both COM objects must release while the apartment they were
    // created in is still alive -- dropping them only implicitly, after
    // CoUninitialize below, would run their Release() against an
    // already-torn-down apartment.
    drop(focus_handler);
    drop(uia);
    if co_init_ok {
        // Only ever balanced against a CoInitializeEx that actually
        // succeeded -- calling this after a failed init would be its
        // own COM misuse on top of whatever made init fail.
        unsafe {
            CoUninitialize();
        }
    }
    rec.sidecar
}

// The hook half of a capture, torn down on pause and rebuilt on resume
// -- see KeyCapture::hook.
struct HookHandle {
    thread: JoinHandle<()>,
    thread_id: u32,
}

fn install_hook() -> Option<HookHandle> {
    // See ModifierState's own doc: whatever this said while unhooked
    // cannot be trusted, and "nothing held" is the only safe assumption
    // to restart from -- a stale true would fail open, not closed.
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
    // None while paused: pausing UNINSTALLS the low-level hook rather
    // than merely dropping events past it, so nothing about the pause
    // window ever depends on the consumer catching everything the OS
    // still delivers -- the OS delivers nothing at all. The consumer
    // thread, its Recorder-in-progress, the pending-keys queue, the
    // Ledger and the UIA focus-changed registration all stay alive
    // across a pause/resume, untouched; only the hook comes down and
    // goes back up.
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
    // A failure here used to leave the hook installed and
    // running forever (its thread detached with no way back to it,
    // RAW_TX still set) while the caller saw no capture at all -- the
    // hook must come down the same way stop() brings it down whenever
    // the consumer that was supposed to read it never gets to exist.
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

// How long stop() waits for the consumer thread before giving up on it
// -- generous for a real UIA round-trip, but never unbounded. A hung
// provider must never be able to freeze the app's own "stop recording"
// action just because this thread happened to be blocked inside a UIA
// call when it was asked to.
const STOP_JOIN_BOUND_MS: u64 = 5_000;

// Polls `is_finished()` rather than blocking on `join()` itself, which
// has no timeout of its own. On a timeout the handle is simply dropped
// (never joined), which detaches the thread -- it keeps running to
// completion on its own instead of the caller waiting on it. Generic
// and Windows-API-free on purpose, so the bound itself is directly
// testable with an ordinary thread.
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
    // Already unhooked when stop() catches a still-paused capture --
    // nothing to tear down there, the consumer is just blocked on
    // recv() waiting for the channel to close, which the next line does.
    if let Some(hook) = capture.hook.take() {
        uninstall_hook(hook);
    }
    // Dropping the sender (clearing the slot) closes the channel, which
    // ends the consumer thread's recv() loop so it returns the finished
    // sidecar below -- unless it is stuck somewhere that never reads
    // the channel at all, which is exactly what the bound below is for.
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

// The pause/resume hotkeys and buttons drive these, all through one
// shared path (see do_pause_recording/do_resume_recording) so no pause
// trigger can ever forget to call them. A no-op when no capture is
// running (keyOverlay off, or nothing to pause yet). Pausing actually
// UNINSTALLS the keyboard hook rather than just filtering its output --
// "no hook while paused" is a stronger, simpler guarantee than "hook
// installed but promised to drop everything".
pub fn pause() {
    let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(cap) = guard.as_mut() {
        cap.clock.lock().unwrap_or_else(|e| e.into_inner()).pause(mono_ms());
        if let Some(hook) = cap.hook.take() {
            uninstall_hook(hook);
        }
    }
}

pub fn resume() {
    let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(cap) = guard.as_mut() {
        cap.clock.lock().unwrap_or_else(|e| e.into_inner()).resume(mono_ms());
        if cap.hook.is_none() {
            cap.hook = install_hook();
            if cap.hook.is_none() {
                // Fails safe: no hook means no keys at all get recorded
                // from here on, same as still being paused, rather than
                // recording resuming silently half-broken.
                append_crash_log("key overlay: could not reinstall the keyboard hook on resume");
            }
        }
    }
}

// finish_and_continue_recording's own boundary marker: every key from
// `t_ms` onward belongs to the switch itself, or to the part about to
// open, never to the part this capture is still finishing up for. A
// no-op when no capture is running.
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

// Ships off, like keyOverlay itself: absent means the plain default
// policy (everything but a password field recorded in full). Read once
// when a capture starts, same as enabled/hold_ms -- see maybe_start.
pub fn shortcuts_only(flags: &std::collections::HashMap<String, bool>) -> bool {
    flags.get("keyOverlayShortcutsOnly") == Some(&true)
}

// Never a second hook: a capture already running for this process is left
// alone rather than layering a new one on top. `start_abs_ms` is the
// clock's zero point, always on the `mono_ms()` clock -- ordinary
// starts pass `keylog::mono_ms()`, but finish_and_continue_recording
// passes the instant its OWN new capture actually started instead, so
// the new part's offsets start counting from when video capture really
// began, not from whenever this call happens to run relative to it.
pub fn maybe_start(is_enabled: bool, hold_ms: u32, start_abs_ms: u64, shortcuts_only: bool) {
    if !is_enabled {
        return;
    }
    let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_some() {
        return;
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
mod tests {
    use super::*;
    // Only exercised by the NEW-3 Pane/Custom test below -- imported
    // here, not in the outer module's own `use`, so a non-test build
    // does not carry an unused-import warning for them.
    use windows::Win32::UI::Accessibility::{UIA_CustomControlTypeId, UIA_PaneControlTypeId};

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
            // Every one of these is a distinct (vk, scan) pair and every
            // one is blocked (an injected flood, say) -- before this
            // fix each one still inserted `false` into open_downs,
            // growing it by one per blocked key with nothing ever
            // removing the entry (no up ever follows a blocked down in
            // this scenario). Now a blocked down removes instead of
            // inserting, so the map must stay empty throughout.
            rec.record(0x41, scan, true, scan as u64, true);
        }
        assert_eq!(rec.open_downs.len(), 0, "a blocked down must never be stored in open_downs");
    }

    #[test]
    fn open_downs_past_its_cap_is_cleared_rather_than_growing_further() {
        let mut rec = Recorder::default();
        for scan in 0..(OPEN_DOWNS_CAP as u32) {
            // Real, accepted downs (not blocked) for distinct keys, none
            // followed by an up -- the only way open_downs actually
            // grows. Stays right at the cap.
            rec.record(0x41, scan, true, scan as u64, false);
        }
        assert_eq!(rec.open_downs.len(), OPEN_DOWNS_CAP);
        // One more accepted down past the cap: defensively cleared
        // rather than left to grow past it.
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
        // One more key, past the cap: no new event, and it opens (or
        // extends) a gap instead of growing the vector further.
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
        assert!(!enabled(&flags)); // absent -- "aus" is the shipped default
        maybe_start(enabled(&flags), 400, mono_ms(), shortcuts_only(&flags));
        assert!(slot().lock().unwrap().is_none());
    }

    #[test]
    fn the_sidecar_shape_carries_no_text_field_at_all() {
        // KeyEvent has exactly three fields (vk, down, t_ms) and Gap two
        // (start_ms, end_ms) -- neither has anywhere a character, a typed
        // string, or a window title could land. Serializing a sample and
        // checking the key set is a belt-and-braces proof of that shape.
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

    // An injected (synthetic) key is gapped exactly like a
    // password field, regardless of what focus reports -- the consumer
    // records `verdict`'s answer before calling record(), so the one
    // gap-producing branch already tested above for passwords is the
    // same branch this exercises for injected input.
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

    // --- Down/up taint (Recorder::open_downs): an up is only ever
    // stored alongside a down this same Recorder actually accepted.

    #[test]
    fn an_up_is_dropped_when_its_matching_down_was_gapped_even_if_the_up_itself_looks_clean() {
        // The scenario: a password key's down gets gapped, then a
        // pause/part switch moves focus to a normal field, and the up
        // -- released well after, its OWN verdict now clean -- must
        // still be dropped, tainted by the down it belongs to.
        let mut rec = Recorder::default();
        rec.record(0x41, 0x1E, true, 100, true); // down, gapped (password)
        rec.record(0x41, 0x1E, false, 900, false); // up, its own verdict now clean
        assert!(rec.sidecar.events.is_empty(), "a tainted down must take its up down with it");
        assert_eq!(rec.sidecar.gaps, vec![Gap { start_ms: 100, end_ms: 900 }]);
    }

    #[test]
    fn an_up_with_no_matching_down_in_this_capture_is_dropped_too() {
        // A key already held before this capture (or this part) began:
        // no down was ever accepted here, so the up alone proves
        // nothing about how it was typed.
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
        // Left and right Ctrl can share a VK code while being genuinely
        // different physical keys -- one being gapped must never taint
        // the other's own down/up pair.
        let mut rec = Recorder::default();
        rec.record(0x11, 0x1D, true, 0, true); // left ctrl down, gapped
        rec.record(0x11, 0x9D, true, 10, false); // right ctrl down, clean
        rec.record(0x11, 0x9D, false, 20, false); // right ctrl up, clean, matches its own down
        assert_eq!(rec.sidecar.events.len(), 2, "the clean scan code's pair must record despite the other one being gapped");
    }

    // --- The binding verdict algorithm (Ledger + verdict): a delayed
    // verdict with a settle check, a guard window scanned for anything
    // dirty in between, and a blind window around every focus change.
    // All pure, so directly testable without a real hook, a real
    // window or real UI Automation.

    #[test]
    fn verdict_is_pending_until_a_check_has_started_late_enough_to_speak_for_the_key() {
        // A key at t=0 needs a check that STARTED at t>=300 (SETTLE_MS).
        // A check that started before that deadline is not enough, no
        // matter what it says -- the caller must keep waiting.
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
        // Tab into a password field, then type the first character
        // before UI Automation's own focus-changed event has even
        // arrived -- the arrival's blind window still gaps it, because
        // the window reaches GUARD_MS BEFORE the arrival too.
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
        // Tab in, type a PIN, Tab out -- even once the consumer catches
        // up and finds "not password" checks on BOTH sides of a PIN
        // character (because it only got around to checking well after
        // the Tab-out), the blind windows around the two arrivals still
        // gap every character in between.
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
        // An Unknown check sitting inside the window between a key and
        // its qualifying check must dirty the window exactly like a
        // Password check does -- before this fix window_is_dirty only
        // ever looked for Password, so an Unknown check right next to a
        // later, clean-looking Text check was silently ignored and the
        // key was allowed through anyway.
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

    // --- NEW-1: a modifier the hook tracked as held must still be
    // confirmed against the live keyboard before it can keep a key.

    #[test]
    fn a_tracked_modifier_with_no_live_confirmation_is_not_held() {
        // The stale-Alt scenario: Alt got tracked "down" but its up
        // happened on an elevated window or the secure desktop, so the
        // hook never saw it -- GetAsyncKeyState now says it is actually
        // up, and that must win.
        assert!(!confirm_modifier(true, false), "a stale tracked modifier with no live confirmation must resolve to not held");
    }

    #[test]
    fn a_tracked_and_live_modifier_is_held() {
        assert!(confirm_modifier(true, true), "a modifier both tracked and live must still count as held");
    }

    #[test]
    fn an_untracked_modifier_is_never_held_even_if_live_state_is_stale() {
        // GetAsyncKeyState alone is not trusted either -- both sides
        // must agree, so a false tracked state always wins to "not
        // held" regardless of what the live read says.
        assert!(!confirm_modifier(false, true));
    }

    #[test]
    fn an_injected_modifier_event_is_never_tracked() {
        assert!(!should_track_modifier_event(true), "a synthetic modifier press must never update the tracked state");
        assert!(should_track_modifier_event(false), "a real modifier press must still be tracked");
    }

    // --- keyOverlayShortcutsOnly: an absolute allow-list filter, tested
    // directly against is_kept_by_shortcuts_only (production code no
    // longer restricts a Text-classified field on its own -- see
    // FieldClass's own doc for the 2026-09-26 policy change).

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
        // Ctrl+Alt with no Win is Windows' AltGr emulation (Ctrl+Alt+Q
        // types "@" on a German keyboard) -- it types a character and
        // must be dropped exactly like plain typing, not treated as a
        // Ctrl+Alt shortcut.
        assert!(!is_kept_by_shortcuts_only(0x51, 0, false, true, true, false), "AltGr must still be dropped, not treated as a shortcut");
    }

    // --- NEW-2: Alt+Numpad / EnableHexNumpad character entry must be
    // dropped, never kept as an "Alt+key shortcut".

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
        // NumLock off: the same physical Numpad 6 key now arrives as
        // VK_LEFT (0x25) with scan code 0x4B and NOT extended -- the
        // scan code is what actually identifies it, since VK_LEFT alone
        // is an ordinary, always-kept nav key.
        assert!(!is_kept_by_shortcuts_only(VK_LEFT, 0x4B, false, false, true, false), "a numpad-origin nav VK with NumLock off must still be dropped under Alt");
    }

    #[test]
    fn shortcuts_only_drops_alt_numpad_digit_key_up_too() {
        // The down/up pair must agree -- a kept down but dropped up (or
        // vice versa) would still leave a recoverable half of the key in
        // the sidecar's own down/up matching. Both a down and an up carry
        // the same vk/scan/extended/ctrl/alt/win here, so one call proves
        // both sides agree.
        assert!(!is_kept_by_shortcuts_only(0x66, 0x4D, false, false, true, false), "the down is dropped");
        assert!(!is_kept_by_shortcuts_only(0x66, 0x4D, false, false, true, false), "and so is the matching up, same inputs either way");
    }

    #[test]
    fn shortcuts_only_keeps_the_real_nav_cluster_key_regardless_of_the_numpad_check() {
        // The genuine Insert/Delete/arrow cluster sends the same VKs as
        // the numpad's NumLock-off keys, but always with LLKHF_EXTENDED
        // set -- Alt+Left from the real arrow cluster must stay a kept
        // nav key exactly as before, not be swept up by NEW-2.
        assert!(is_kept_by_shortcuts_only(VK_LEFT, 0x4B, true, false, true, false), "the real, extended arrow key must still be kept under Alt");
    }

    #[test]
    fn shortcuts_only_keeps_alt_with_an_ordinary_letter() {
        assert!(is_kept_by_shortcuts_only(0x41, 0x1E, false, false, true, false), "Alt+A is an ordinary shortcut, not a numpad scan or VK");
    }

    #[test]
    fn shortcuts_only_does_not_treat_ctrl_alt_numpad_as_numpad_entry() {
        // The numpad check only applies to Alt held ALONE -- Ctrl+Alt is
        // AltGr territory (see shortcuts_only_drops_altgr) and any other
        // combination is an ordinary shortcut, not Alt+Numpad entry, so
        // this must fall through to the normal ctrl/alt/win shortcut
        // rule instead of the numpad-specific one.
        assert!(is_kept_by_shortcuts_only(0x66, 0x4D, false, true, true, true), "Ctrl+Alt+Win+Numpad6 is an ordinary shortcut combo, not character entry");
    }

    // --- NEW-3: only a structurally non-text UIA control type may ever
    // resolve to FieldClass::NonText. Since 2026-09-26 both Text and
    // NonText reach the same Verdict::Allow (see FieldClass's own doc),
    // so that distinction no longer changes what gets recorded by
    // default -- it is kept only because the same UIA read pipeline
    // still has to prove "not a password field" either way. A control
    // type on neither list -- a bare Pane/Custom element such as PuTTY
    // or an RDP/VNC session, a failed read, or no element at all -- is
    // FieldClass::Unknown, which gaps (see classify_control's own doc).

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
        // PuTTY/KiTTY, an RDP/VNC remote session window, or any
        // Qt/Java/game window with no accessibility provider at all
        // surfaces as a bare Pane or Custom element -- neither is
        // allow-listed, so a positive NonText classification is out.
        assert!(!is_non_text_control_type(UIA_PaneControlTypeId));
        assert!(!is_non_text_control_type(UIA_CustomControlTypeId));
        // Such an element also never proved a positive Text
        // identification (no Edit/Document type, no editable
        // ValuePattern, no TextPattern), so classify_control resolves it
        // to Unknown, not Text -- see the HIGH finding this closes: an
        // unclassifiable control used to collapse to Text/Allow, which
        // let an SSH or RDP password typed into such a window through
        // in full. Unknown gaps instead, the same as a confirmed
        // password field.
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

    // --- NEW-4: a check's own reads (GetFocusedElement and the
    // properties queried on it) must never be trusted once the focused
    // identity has moved on since the check started.

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
        // NEW-4: takes the caller's already-fetched element now, rather
        // than fetching its own -- a missing element (UIA unavailable,
        // or the fetch itself failed) must still resolve to the same
        // fail-closed Password answer as before.
        assert!(is_password_field(HWND(1 as *mut std::ffi::c_void), None));
    }

    #[test]
    fn classify_control_is_unknown_with_no_element_to_query() {
        // Before this fix, no element to query fell through to
        // FieldClass::Text (a positive-sounding tier with no positive
        // evidence behind it). It must land in Unknown instead, which
        // gaps just like doubt anywhere else in this module.
        assert_eq!(classify_control(None), FieldClass::Unknown, "no element to classify is doubt, never a positive Text finding");
    }

    #[test]
    fn classify_identity_is_password_when_the_shared_element_is_missing() {
        let identity = (HWND(1 as *mut std::ffi::c_void), None);
        assert_eq!(classify_identity(&identity), FieldClass::Password);
    }

    #[test]
    fn classify_checked_fails_closed_and_reports_no_change_with_no_uia_object() {
        // With no UI Automation object at all, focus_identity never
        // touches a live element (see focus_identity), so the only
        // thing that could make classify_checked see a "changed"
        // identity between its own before/after reads is the real
        // foreground HWND itself moving in the instant between two
        // back-to-back calls -- not expected inside a test process, so
        // this exercises the ordinary fail-closed path deterministically:
        // still Password (see is_password_field's own fail-closed rule)
        // and not reported as invalidated.
        assert_eq!(classify_checked(&None), (FieldClass::Password, false));
    }

    #[test]
    fn a_late_detected_focus_change_after_a_stall_gaps_the_key_typed_during_it() {
        // A settle check that (falsely, from the stall's point of view)
        // reports the OLD field as still non-text, arriving well after
        // the key below -- this is the "later check sees the button
        // field, stable" false negative this closes. On its
        // own this stale check would satisfy `verdict`'s Allow branch.
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

        // The poll last confirmed focus unchanged at 1000, then stalled
        // in a slow UI Automation call and only got to detect the real
        // change (or simply ran late enough to count as one) at 2000 --
        // the key above landed inside that whole stalled span, which is
        // exactly what mark_forced_gaps and the poll block's own
        // push_arrival pair are meant to cover.
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
        assert!(!poll_overran(1049, 1000)); // 49ms late -- still within FOCUS_POLL_MS
        assert!(!poll_overran(1050, 1000)); // exactly FOCUS_POLL_MS late -- not yet over
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
        // Requirement (a): the idle gap alone, with the key consumed
        // right after it was captured (no scheduling delay on top),
        // must not force-gap it. next_poll_due lands at raw.t_ms + 50,
        // so a receive that happens at essentially the same instant
        // never reads as an overrun.
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
        // Requirement (b): the idle->busy reset only rearms the
        // deadline and leaves last_focus untouched, so a poll that
        // later runs and genuinely detects a changed identity still
        // gaps the key through the ordinary mark_forced_gaps path --
        // exactly as it does for a key that was already pending.
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
        // Requirement (c): a poll that was already armed because a key
        // was already pending must still treat its own overrun as a
        // stall -- idle_to_busy_reset only ever runs on the
        // empty->non-empty edge and must not weaken this ordinary case.
        assert!(poll_overran(2000, 1000));
        let mut pending: VecDeque<RawKey> = VecDeque::new();
        pending.push_back(make_raw_key(1200));
        mark_forced_gaps(&mut pending, 1000, 2000);
        assert!(pending[0].forced_gap);
    }

    #[test]
    fn a_delayed_receive_with_no_detected_change_still_forces_the_gap() {
        // The HIGH this closes: focus moves A -> password/text -> A
        // and back again entirely inside the gap between this key's
        // own hook timestamp (1200) and the moment this thread actually
        // gets around to receiving it (1400, a 200ms scheduling delay
        // with no UIA event ever arriving) -- the round trip completes
        // silently, so an endpoint comparison alone would read as
        // "unchanged" no matter when it runs. Anchoring the rearmed
        // deadline to raw.t_ms instead of the receive time is what
        // makes poll_overran alone, with no detected change at all,
        // still catch this: the effective deadline (1200 + 50 = 1250)
        // is already more than one full FOCUS_POLL_MS behind by the
        // time anything gets checked.
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
        // The follow-up HIGH this closes: this thread was busy with
        // something else (not blocked in recv) while pending stayed
        // empty, so neither the poll nor a settle check ever ran (both
        // are gated on pending being non-empty) -- focus could go
        // A -> password/text -> A and back, silently, entirely inside
        // that stretch. The key is captured near the end of it (1250),
        // this thread only gets around to calling recv at 1300 and
        // receives it almost immediately after (1310). Rearming from
        // the key's own timestamp alone (idle_to_busy_reset) would
        // erase the fact that nothing was observed since 1000, hiding
        // the whole stretch. Because raw_t_ms (1250) precedes
        // recv_entered_at (1300) and the silence since last_observed_at
        // (1000) already exceeds the ordinary stall allowance,
        // classify_idle_transition force-gaps across the real span
        // back to 1000 instead of a fresh one starting at 1250.
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
        // The genuine idle case must stay green: this thread was truly
        // blocked in recv (nothing pending, nothing to hide) when the
        // key arrived, so raw_t_ms is no earlier than recv_entered_at
        // and classify_idle_transition takes the ordinary Rearm branch,
        // exactly like idle_to_busy_reset always did on its own.
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
        // busy_before_recv alone is not enough to force-gap: a short
        // stretch right after a real observation has barely any window
        // for a round trip to hide in, so this still takes the
        // ordinary Rearm branch -- only a silence past the stall
        // allowance escalates to ForceGap.
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
        // A failed or unknown classification (see classify_control) now
        // resolves to FieldClass::Unknown, never Text -- Text requires a
        // positive text-entry finding. Unknown gaps unconditionally,
        // the same fail-closed answer as a confirmed password field.
        // Proven here through the same verdict() pipeline the
        // production code calls, not just the resolve_verdict helper
        // alone.
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

    // --- keyOverlayShortcutsOnly applied through resolve_verdict: an
    // absolute filter on top of the verdict, regardless of which field
    // class produced the Allow, and never relaxing a password gap.

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
        // Tab into a password field, type, Tab out within 300ms -- and
        // UI Automation never raises a focus-changed event for either
        // move at all (no handler, or the app just never fires one).
        // The consumer treats the Tab-out key-down itself as the
        // arrival, stamped at the hook's own timestamp, so its blind
        // window still gaps the key exactly as a real UIA notification
        // would have.
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
        // One key already resolved by a qualifying check, one still
        // waiting on its own.
        let resolved_key = 5_000;
        ledger.push_check(resolved_key + SETTLE_MS, FieldClass::NonText);
        let pending_key = 5_200;
        let before_resolved = verdict(&ledger, resolved_key, false);
        let before_pending = verdict(&ledger, pending_key, false);
        let checks_before = ledger.checks.len();
        let arrivals_before = ledger.arrivals.len();

        // The oldest still-pending key is `pending_key` -- the consumer
        // prunes to pending_key - GUARD_MS, exactly as the loop in
        // consumer_thread_main does every iteration.
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
        // A thread that runs well past the bound -- standing in for a
        // consumer stuck inside a hung UIA call. join_bounded must
        // return quickly with None rather than waiting for it, letting
        // the thread finish on its own in the background.
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
        // The bug: a settle check or a focus poll used to run only in
        // the Timeout arm of the consumer's recv match, so a key-repeat
        // burst under FOCUS_POLL_MS apart -- which keeps hitting the
        // Ok(raw) arm instead -- could starve both indefinitely. The
        // fix makes both a pure function of absolute time, so simulating
        // several Ok arrivals in a row (never a Timeout at all) must
        // still show both becoming due once enough time has passed.
        let front_t_ms = 0u64;
        let mut next_poll_due = FOCUS_POLL_MS;
        // Three "arrivals" 10ms apart, well under FOCUS_POLL_MS -- a
        // burst that would previously never let the Timeout arm fire.
        for now in [10u64, 20, 30] {
            assert!(!settle_check_due(Some(front_t_ms), now), "not due yet at t={now}");
            assert!(!focus_poll_due(false, now, next_poll_due), "not due yet at t={now}");
        }
        // The settle deadline (SETTLE_MS after the key) is reached
        // despite every one of those ticks arriving through Ok, not a
        // Timeout.
        let settle_due_at = front_t_ms + SETTLE_MS;
        assert!(settle_check_due(Some(front_t_ms), settle_due_at), "must become due exactly at its own deadline");
        // Likewise the poll deadline, independently.
        assert!(focus_poll_due(false, next_poll_due, next_poll_due));
        next_poll_due += FOCUS_POLL_MS;
        assert!(!focus_poll_due(false, next_poll_due - 1, next_poll_due), "not due again until the next tick");
    }

    #[test]
    fn recv_wait_targets_whichever_deadline_is_sooner_and_never_goes_negative() {
        // The settle deadline is sooner: front=100 -> settle at 400,
        // poll at 1000 -> wait for the settle deadline.
        assert_eq!(recv_wait_ms(0, 100, 1_000), 400);
        // The poll deadline is sooner.
        assert_eq!(recv_wait_ms(0, 10_000, 50), 50);
        // Both deadlines already passed -- an immediate wake, never a
        // panic from an underflowing subtraction.
        assert_eq!(recv_wait_ms(10_000, 0, 0), 0);
    }

    #[test]
    fn exceeding_the_ledger_hard_cap_fails_closed_and_drops_history() {
        let mut ledger = Ledger::default();
        // A clean check well within the settle window, so a key right
        // now would otherwise record normally -- proving the eventual
        // gap comes from the cap, not from an ordinary dirty window.
        ledger.push_check(0, FieldClass::NonText);
        assert_eq!(verdict(&ledger, 0, false), None, "not yet, no qualifying check exists"); // sanity: still pure, unaffected by the flood below
        for i in 0..LEDGER_HARD_CAP {
            // Arrivals far enough apart that none of them land in each
            // other's blind window and mask the point being tested --
            // only the cap itself should be doing anything here.
            ledger.push_arrival((i as u64) * 10_000);
        }
        assert!(ledger.fail_closed, "the cap must have tripped fail_closed");
        assert!(ledger.checks.is_empty() && ledger.arrivals.is_empty(), "history must actually be dropped, not just capped");
        assert_eq!(verdict(&ledger, 0, false), Some(Verdict::Gap), "every verdict from here on must gap, regardless of any earlier clean check");
        assert_eq!(verdict(&ledger, 50_000_000, false), Some(Verdict::Gap), "including a key far outside every arrival's own window");
    }

    // --- ActiveClock: pause drops events, and offsets use the
    // active-recording clock, not wall time.

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
        // Never resumed -- anything after the pause started must still
        // read as paused, not just the interval up to "now".
        assert!(clock.is_paused_at(100));
        assert!(clock.is_paused_at(10_000));
    }

    #[test]
    fn active_clock_subtracts_paused_spans_from_the_elapsed_offset() {
        let mut clock = ActiveClock::new(0);
        clock.pause(1_000);
        clock.resume(3_000); // 2000ms paused
        // A moment 4000ms after start, 2000ms of which was paused, is
        // 2000ms of ACTIVE recording time -- the same basis the video
        // track's own clock uses, per RecordClock's own invariant.
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
