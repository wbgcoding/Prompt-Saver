use super::{clip_inbox_cap, db_add_clip, db_conn, lock, now_secs, text_hash, Db};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

static WATCHER: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
static WATCHER_APP: Mutex<Option<AppHandle>> = Mutex::new(None);
static WATCHER_HWND: AtomicIsize = AtomicIsize::new(0);
// Set by stop() so a thread that has not yet entered its message loop tears down
// immediately instead of blocking join() forever (fast on/off toggle deadlock).
static WATCHER_STOP: AtomicBool = AtomicBool::new(false);
static LAST_OWN: AtomicU64 = AtomicU64::new(0);
static LAST_INSERTED: AtomicU64 = AtomicU64::new(0);

// Auto-advance session state. Ticks are GetTickCount() units (u32, wraps every ~49.7 days,
// so comparisons use wrapping_sub). SELF_READ brackets our own clipboard reads so they are
// never mistaken for a paste; OWN_EMPTY brackets our own EmptyClipboard calls so
// WM_DESTROYCLIPBOARD can tell a re-arm from another app taking ownership.
static SELF_READ: AtomicBool = AtomicBool::new(false);
static OWN_EMPTY: AtomicBool = AtomicBool::new(false);
static SESSION: Mutex<Option<AutoSession>> = Mutex::new(None);

struct AutoSession {
    id: String,
    // Frontend-resolved text per absolute entry index; empty = no
    // placeholder round happened, fall back to the raw rotation_next() text.
    resolved: Vec<String>,
    text: String,
    // A confidential entry: its decrypted text may only be delivered while the
    // vault key is still live.
    secret: bool,
    armed_at: u32,
    session_start: u32,
    last_paste: u32,
    uncounted_rearms: u32,
}

const WM_DESTROY: u32 = 0x0002;
const WM_CLIPBOARDUPDATE: u32 = 0x031D;
const WM_RENDERFORMAT: u32 = 0x0305;
const WM_RENDERALLFORMATS: u32 = 0x0306;
const WM_DESTROYCLIPBOARD: u32 = 0x0307;
const WM_APP_ARM: u32 = 0x8001;
const WM_APP_REARM: u32 = 0x8002;
// Force-end the active session (e.g. the tile's Auto switch flipped off) and deliver real data.
const WM_APP_END: u32 = 0x8003;
const CF_UNICODETEXT: u32 = 13;
// A copied text past this many characters never reaches the inbox: it would sit
// in the database for good. UTF-8 needs at most four bytes a character, so a text
// longer than four times this in bytes is out without counting characters.
pub(crate) const CLIP_INBOX_MAX_CHARS: usize = 200_000;
const GMEM_MOVEABLE: u32 = 0x0002;
const HWND_MESSAGE: isize = -3;

#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}
type WndProc = unsafe extern "system" fn(isize, u32, usize, isize) -> isize;
#[repr(C)]
struct WndClassW {
    style: u32,
    wndproc: Option<WndProc>,
    cls_extra: i32,
    wnd_extra: i32,
    hinstance: isize,
    hicon: isize,
    hcursor: isize,
    hbr: isize,
    menu: *const u16,
    class_name: *const u16,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(c: *const WndClassW) -> u16;
    fn CreateWindowExW(ex: u32, class: *const u16, name: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: isize, menu: isize, inst: isize, param: *const core::ffi::c_void) -> isize;
    fn DefWindowProcW(hwnd: isize, msg: u32, w: usize, l: isize) -> isize;
    fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn AddClipboardFormatListener(hwnd: isize) -> i32;
    fn RemoveClipboardFormatListener(hwnd: isize) -> i32;
    fn PostMessageW(hwnd: isize, msg: u32, w: usize, l: isize) -> i32;
    fn PostQuitMessage(code: i32);
    fn DestroyWindow(hwnd: isize) -> i32;
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
    fn IsClipboardFormatAvailable(format: u32) -> i32;
    fn OpenClipboard(hwnd: isize) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, hmem: isize) -> isize;
    fn GetClipboardData(format: u32) -> isize;
    fn GetClipboardOwner() -> isize;
}

#[repr(C)]
struct LastInputInfo {
    cb_size: u32,
    dw_time: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
    fn GlobalFree(hmem: isize) -> isize;
    fn GlobalLock(hmem: isize) -> *mut core::ffi::c_void;
    fn GlobalUnlock(hmem: isize) -> i32;
    fn GlobalSize(hmem: isize) -> usize;
    fn GetTickCount() -> u32;
}
#[link(name = "user32")]
extern "system" {
    fn GetLastInputInfo(info: *mut LastInputInfo) -> i32;
}

fn tick() -> u32 {
    unsafe { GetTickCount() }
}

// GetLastInputInfo failing (returning 0) reads as "ancient" rather than "just now": the
// input-recency gate in counts_as_paste must fail closed, not open, if Windows
// ever can't answer.
fn last_input_tick() -> u32 {
    let mut info = LastInputInfo { cb_size: std::mem::size_of::<LastInputInfo>() as u32, dw_time: 0 };
    unsafe {
        if GetLastInputInfo(&mut info) != 0 {
            info.dw_time
        } else {
            0
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// Record the hash of text WE just put on the clipboard, so the watcher ignores it.
pub fn note_own(text: &str) {
    LAST_OWN.store(text_hash(text.trim()), Ordering::Relaxed);
}

// The value of a DWORD-payload clipboard format, or None if absent or unreadable. Only
// called after the source has closed the clipboard, never while we hold it open.
fn read_dword_format(fmt: u32) -> Option<u32> {
    if fmt == 0 || unsafe { IsClipboardFormatAvailable(fmt) } == 0 {
        return None;
    }
    unsafe {
        if OpenClipboard(0) == 0 {
            return None;
        }
        let h = GetClipboardData(fmt);
        let value = if h != 0 {
            // The block belongs to another program: it may be smaller than a DWORD.
            let big_enough = GlobalSize(h) >= std::mem::size_of::<u32>();
            let ptr = if big_enough { GlobalLock(h) } else { std::ptr::null_mut() };
            let v = if ptr.is_null() { None } else { Some(std::ptr::read_unaligned(ptr as *const u32)) };
            if !ptr.is_null() { GlobalUnlock(h); }
            v
        } else {
            None
        };
        CloseClipboard();
        value
    }
}

// The most bytes a CF_UNICODETEXT block may take for a text of CLIP_INBOX_MAX_CHARS
// characters: four bytes for each character that needs a surrogate pair, plus the
// terminating NUL.
pub(crate) const CLIP_INBOX_MAX_UNICODE_BYTES: usize = CLIP_INBOX_MAX_CHARS * 4 + 2;

// Whether a clipboard text block of this size may be read at all. A block past the
// limit holds more characters than the inbox keeps, so it is never copied into a
// String (the copy of a few hundred megabytes is the cost this avoids).
pub(crate) fn text_block_within_limit(bytes: usize) -> bool {
    bytes <= CLIP_INBOX_MAX_UNICODE_BYTES
}

// The size in bytes of the clipboard's text block, read without copying it, or None
// when the clipboard could not be opened or holds no text block. Only called after
// the source has closed the clipboard.
fn clipboard_text_block_size() -> Option<usize> {
    unsafe {
        if OpenClipboard(0) == 0 {
            return None;
        }
        let h = GetClipboardData(CF_UNICODETEXT);
        let size = if h != 0 { Some(GlobalSize(h)) } else { None };
        CloseClipboard();
        size
    }
}

// Whether a copied text belongs in the inbox: long enough, not absurdly long,
// and not shaped like a password while secret skipping is on. The cheap byte
// check runs before any character counting or scanning.
pub(crate) fn should_store(text: &str, min: usize, skip_secrets: bool) -> bool {
    let trimmed = text.trim();
    if trimmed.len() > CLIP_INBOX_MAX_CHARS * 4 {
        return false;
    }
    let chars = trimmed.chars().count();
    if chars < min || chars > CLIP_INBOX_MAX_CHARS {
        return false;
    }
    !(skip_secrets && super::looks_like_password(trimmed, super::CLIP_SECRET_MIN_LEN))
}

fn on_clipboard_update() {
    // Our own delayed-rendering promise is never inbox material, and reading it would itself
    // trigger WM_RENDERFORMAT.
    if unsafe { GetClipboardOwner() } == WATCHER_HWND.load(Ordering::Acquire) {
        return;
    }
    // Poison-tolerant locks: a panic elsewhere must never turn a clipboard event
    // into a process abort (this runs inside an extern "system" wndproc).
    let app = match WATCHER_APP.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        Some(a) => a,
        None => return,
    };
    let (min, cap, enabled, skip_secrets) = {
        let state = app.state::<Db>();
        let store = lock(&state);
        let enabled = store.settings.ui_flags.get("clipWatcher") == Some(&true);
        let min = store.settings.ui_values.get("clipMinChars").copied().unwrap_or(3.0).max(1.0) as usize;
        let cap = clip_inbox_cap(&store.settings);
        // At least one widely used password manager sets neither this nor the two OS
        // formats below, so the heuristic further down carries the real weight --
        // this stays on unless a user deliberately turns it off.
        let skip_secrets = store.settings.ui_flags.get("clipSkipSecrets") != Some(&false);
        (min, cap, enabled, skip_secrets)
    };
    if !enabled {
        return;
    }
    // Honour the opt-out formats password managers set: a plain "present at all" marker
    // (KeePass, 1Password, ...) and Windows' own clipboard-history/cloud-sync opt-outs, where an
    // explicit 0 means "do not include". Sources setting none of them rely on the heuristic below.
    unsafe {
        let excl = RegisterClipboardFormatW(wide("ExcludeClipboardContentFromMonitorProcessing").as_ptr());
        if excl != 0 && IsClipboardFormatAvailable(excl) != 0 {
            return;
        }
        for name in ["CanIncludeInClipboardHistory", "CanUploadToCloudClipboard"] {
            let fmt = RegisterClipboardFormatW(wide(name).as_ptr());
            let available = fmt != 0 && IsClipboardFormatAvailable(fmt) != 0;
            if super::dword_format_excludes(available, read_dword_format(fmt)) {
                return;
            }
        }
    }
    // Text only; image/file payloads are ignored. Bracketed with SELF_READ in case ownership
    // changed since the guard above. The block is measured first; an unmeasurable one (clipboard
    // busy) is left to the reader below, which has its own retry.
    if let Some(bytes) = clipboard_text_block_size() {
        if !text_block_within_limit(bytes) {
            return;
        }
    }
    SELF_READ.store(true, Ordering::Release);
    let read = arboard::Clipboard::new().and_then(|mut c| c.get_text());
    SELF_READ.store(false, Ordering::Release);
    let text = match read {
        Ok(t) => t,
        Err(_) => return,
    };
    if !should_store(&text, min, skip_secrets) {
        return;
    }
    let trimmed = text.trim();
    let h = text_hash(trimmed);
    if h == LAST_OWN.load(Ordering::Relaxed) || h == LAST_INSERTED.load(Ordering::Relaxed) {
        return;
    }
    LAST_INSERTED.store(h, Ordering::Relaxed);
    if let Some(conn) = db_conn(&app) {
        let _ = db_add_clip(&conn, now_secs(), trimmed, cap);
    }
    let _ = app.emit("clip-inbox-changed", ());
}

// Writes `text` as CF_UNICODETEXT into the clipboard bracket the caller has open. Ownership
// of the GMEM_MOVEABLE block passes to Windows only when SetClipboardData succeeds; on every
// other path it is still ours and must be freed, or a rotating prompt leaks one per paste.
unsafe fn set_unicode_text(text: &str) -> bool {
    let units = wide(text);
    let bytes = units.len() * std::mem::size_of::<u16>();
    let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes);
    if hmem == 0 {
        return false;
    }
    let ptr = GlobalLock(hmem);
    if ptr.is_null() {
        GlobalFree(hmem);
        return false;
    }
    std::ptr::copy_nonoverlapping(units.as_ptr(), ptr as *mut u16, units.len());
    GlobalUnlock(hmem);
    if SetClipboardData(CF_UNICODETEXT, hmem) == 0 {
        GlobalFree(hmem);
        return false;
    }
    true
}

// The formats a password manager sets so a copy stays out of Win+V history, the cloud
// clipboard and clipboard managers. Only a confidential prompt's copy carries them.
pub(crate) const PRIVATE_FORMATS: [&str; 3] = [
    "ExcludeClipboardContentFromMonitorProcessing",
    "CanIncludeInClipboardHistory",
    "CanUploadToCloudClipboard",
];

pub(crate) fn private_formats(secret: bool) -> &'static [&'static str] {
    if secret { &PRIVATE_FORMATS } else { &[] }
}

// Adds the opt-out formats (DWORD 0) to the clipboard bracket the caller has open.
unsafe fn set_private_markers() {
    for name in PRIVATE_FORMATS {
        let fmt = RegisterClipboardFormatW(wide(name).as_ptr());
        if fmt == 0 {
            continue;
        }
        let hmem = GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of::<u32>());
        if hmem == 0 {
            continue;
        }
        let ptr = GlobalLock(hmem);
        if ptr.is_null() {
            GlobalFree(hmem);
            continue;
        }
        *(ptr as *mut u32) = 0;
        GlobalUnlock(hmem);
        if SetClipboardData(fmt, hmem) == 0 {
            GlobalFree(hmem);
        }
    }
}

// A plain (not auto-advancing) copy of a confidential prompt: the text plus the opt-out
// formats in one clipboard bracket. Retries briefly while another app holds the clipboard.
pub(crate) fn write_private_text(text: &str) -> bool {
    for _ in 0..10 {
        if unsafe { OpenClipboard(0) } != 0 {
            unsafe {
                EmptyClipboard();
                let ok = set_unicode_text(text);
                set_private_markers();
                CloseClipboard();
                return ok;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

// Drives the tile badge's is-auto class.
#[derive(serde::Serialize, Clone)]
struct RotationAuto {
    id: String,
    active: bool,
}

fn emit_active(id: &str, active: bool) {
    if let Some(app) = WATCHER_APP.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        let _ = app.emit("rotation-auto", RotationAuto { id: id.to_string(), active });
    }
}

// The app's own text read (placeholder `{?@clipboard}`): rendering a pending promise
// for it is not a paste.
pub(crate) fn read_text_self() -> String {
    SELF_READ.store(true, Ordering::Release);
    let read = arboard::Clipboard::new().and_then(|mut c| c.get_text());
    SELF_READ.store(false, Ordering::Release);
    read.unwrap_or_default()
}

// (Re-)claim ownership and promise CF_UNICODETEXT without delivering data yet; used for the
// first arm (is_first) and every re-arm after a render. Must run on the window thread (posted
// here, never called directly).
fn open_and_promise(hwnd: isize, is_first: bool) {
    // A session that ended while this message was queued (a foreign copy, the
    // end message, the timeout) must not empty the clipboard it no longer owns.
    if SESSION.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
        return;
    }
    let opened = unsafe { OpenClipboard(hwnd) != 0 };
    if !opened {
        // Another app has the clipboard open: do not spin, end the session. On the first arm nothing
        // has reached the real clipboard yet, so fall back to a plain untracked write (as arm() does
        // when the watcher is not running). A failed re-arm keeps the text already delivered, but
        // must tell the UI the session is gone, or the tile badge stays "active".
        let dropped = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(session) = dropped {
            if is_first {
                if session.secret {
                    write_private_text(&session.text);
                } else {
                    let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(session.text));
                }
            } else {
                emit_active(&session.id, false);
            }
        }
        return;
    }
    let secret = SESSION.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.secret);
    unsafe {
        OWN_EMPTY.store(true, Ordering::Release);
        EmptyClipboard();
        OWN_EMPTY.store(false, Ordering::Release);
        SetClipboardData(CF_UNICODETEXT, 0);
        if secret {
            set_private_markers();
        }
        CloseClipboard();
    }
    let now = tick();
    let id = {
        let mut g = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        let Some(session) = g.as_mut() else { return };
        session.armed_at = now;
        if is_first {
            session.session_start = now;
        }
        session.id.clone()
    };
    if is_first {
        emit_active(&id, true);
    }
}

// Answers WM_RENDERFORMAT: the pasting app already has the clipboard open,
// so this delivers directly (no Open/CloseClipboard) then judges, off the clock,
// whether that render was a real paste and whether to keep tracking.
fn handle_render_format() {
    let now = tick();
    let self_read = SELF_READ.load(Ordering::Acquire);
    let last_input = last_input_tick();
    let app = WATCHER_APP.lock().unwrap_or_else(|e| e.into_inner()).clone();

    let mut g = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let Some(session) = g.as_mut() else {
        drop(g);
        unsafe { set_unicode_text("") };
        return;
    };
    if session.secret {
        let live = app.as_ref().is_some_and(|app| {
            let state = app.state::<Db>();
            let store = lock(&state);
            super::secret_session_key(super::secret_timeout_min(&store.settings)).is_some()
        });
        if !live {
            // The vault timed out since this text was decrypted: hand the paste
            // nothing, and end the session so no later render can leak it either.
            let id = session.id.clone();
            *g = None;
            drop(g);
            unsafe { set_unicode_text("") };
            emit_active(&id, false);
            return;
        }
    }
    unsafe { set_unicode_text(&session.text) }; // must answer before anything else

    let Some(app) = app else {
        *g = None;
        return;
    };
    let (rearm, blocked, id) = {
        let state = app.state::<Db>();
        let mut store = lock(&state);
        let cfg = super::auto_cfg();
        let timed_out = cfg.timeout_min != 0
            && now.wrapping_sub(session.session_start) >= cfg.timeout_min.saturating_mul(60_000);
        let counted = !timed_out
            && super::counts_as_paste(now, session.armed_at, session.last_paste, last_input, self_read, &cfg);
        let mut rearm = false;
        let mut blocked = false;
        if counted {
            // Only the rotation fields are cloned (a tile with a picture is large).
            let eligible = store.prompts.iter().find(|p| p.id == session.id)
                .filter(|p| super::auto_eligible(p, &store.settings))
                .map(super::rotation_core);
            // A prompt gone or turned off mid-session ends the session quietly.
            if let Some(p) = eligible {
                // A confidential entry is stored encrypted: decrypt it first, and
                // end the session when the vault locked meanwhile, so ciphertext
                // never becomes the next paste.
                let key = super::secret_session_key(super::secret_timeout_min(&store.settings));
                if let Some(plain) = super::revealed_prompt(&p, key) {
                    let (idx, _, raw) = super::rotation_next(&plain, &store.settings);
                    let next = session.resolved.get(idx).cloned().unwrap_or(raw);
                    super::rotation_advance(&p, &mut store.settings);
                    // Once per paste: collected, never a disk write per Ctrl+V.
                    super::save_settings_soon();
                    super::emit_rotation_changed(&app, &p, &store.settings);
                    session.text = next;
                    session.last_paste = now;
                    session.uncounted_rearms = 0;
                    rearm = true;
                }
            }
        } else if !timed_out {
            session.uncounted_rearms += 1;
            if session.uncounted_rearms <= cfg.rearm_max {
                rearm = true;
            } else {
                blocked = true;
            }
        }
        (rearm, blocked, session.id.clone())
    };

    if rearm {
        let hwnd = WATCHER_HWND.load(Ordering::Acquire);
        if hwnd != 0 {
            unsafe { PostMessageW(hwnd, WM_APP_REARM, 0, 0) };
        }
    } else {
        *g = None;
        drop(g);
        emit_active(&id, false);
        if blocked {
            #[derive(serde::Serialize, Clone)]
            struct RotationAutoBlocked {
                id: String,
            }
            let _ = app.emit("rotation-auto-blocked", RotationAutoBlocked { id });
        }
    }
}

// WM_RENDERALLFORMATS: Windows asking us to render everything we still owe
// before the owner window (or the whole process) goes away. Unlike RENDERFORMAT
// we must open the clipboard ourselves, and this never counts as a paste.
fn handle_render_all(hwnd: isize) {
    let text = SESSION.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.text.clone());
    let Some(text) = text else { return };
    unsafe {
        if OpenClipboard(hwnd) != 0 {
            set_unicode_text(&text);
            CloseClipboard();
        }
    }
    let id = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take().map(|s| s.id);
    if let Some(id) = id {
        emit_active(&id, false);
    }
}

// WM_DESTROYCLIPBOARD: someone else just called EmptyClipboard. Our own
// re-arm does that too, so OWN_EMPTY tells the two apart; only a real foreign
// takeover ends the session here.
fn handle_destroy_clipboard() {
    if OWN_EMPTY.load(Ordering::Acquire) {
        return;
    }
    let id = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take().map(|s| s.id);
    if let Some(id) = id {
        emit_active(&id, false);
    }
}

// Force-end the session for `id` (the tile's Auto switch flipped off, or
// the prompt was deleted/edited out of eligibility) without waiting for a paste.
// Delivers the last armed text as real data: never leaves a dangling promise.
fn handle_app_end() {
    let ended = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some(session) = ended else { return };
    let hwnd = WATCHER_HWND.load(Ordering::Acquire);
    unsafe {
        if hwnd != 0 && OpenClipboard(hwnd) != 0 {
            OWN_EMPTY.store(true, Ordering::Release);
            EmptyClipboard();
            OWN_EMPTY.store(false, Ordering::Release);
            set_unicode_text(&session.text);
            CloseClipboard();
        }
    }
    emit_active(&session.id, false);
}

// Called from the normal copy path (put_text) once a prompt is judged auto-eligible.
// `resolved` is the frontend's once-per-copy placeholder resolution; empty for a prompt
// without placeholders, where each render falls back to the raw rotation_next() text.
pub fn arm(id: &str, text: &str, resolved: Vec<String>, secret: bool) {
    note_own(text);
    let old_id = {
        let mut g = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        let old = g.take().map(|s| s.id);
        *g = Some(AutoSession {
            id: id.to_string(),
            resolved,
            text: text.to_string(),
            secret,
            armed_at: 0,
            session_start: 0,
            last_paste: 0,
            uncounted_rearms: 0,
        });
        old
    };
    if let Some(old_id) = old_id {
        if old_id != id {
            emit_active(&old_id, false);
        }
    }
    let hwnd = WATCHER_HWND.load(Ordering::Acquire);
    if hwnd != 0 {
        unsafe { PostMessageW(hwnd, WM_APP_ARM, 0, 0) };
    } else {
        // The message window is not running: fall back to a normal, untracked
        // copy rather than silently dropping the text.
        *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if secret {
            write_private_text(text);
        } else {
            let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string()));
        }
    }
}

// The tile menu / editor toggle flips Auto off, or the prompt no longer
// qualifies: end its session immediately if it is the one currently running.
pub fn end_if(id: &str) {
    let matches = SESSION.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.id == id).unwrap_or(false);
    if !matches {
        return;
    }
    let hwnd = WATCHER_HWND.load(Ordering::Acquire);
    if hwnd != 0 {
        unsafe { PostMessageW(hwnd, WM_APP_END, 0, 0) };
    }
}

// The global rotatingPrompts/rotateAuto feature switch flipped off: end
// whatever session happens to be running, regardless of its id.
pub fn end_any() {
    let running = SESSION.lock().unwrap_or_else(|e| e.into_inner()).is_some();
    if !running {
        return;
    }
    let hwnd = WATCHER_HWND.load(Ordering::Acquire);
    if hwnd != 0 {
        unsafe { PostMessageW(hwnd, WM_APP_END, 0, 0) };
    }
}

unsafe extern "system" fn wndproc(hwnd: isize, msg: u32, w: usize, l: isize) -> isize {
    match msg {
        WM_CLIPBOARDUPDATE => {
            // A panic must not unwind across this extern "system" boundary (→ abort).
            let _ = std::panic::catch_unwind(on_clipboard_update);
            0
        }
        WM_RENDERFORMAT => {
            let _ = std::panic::catch_unwind(handle_render_format);
            0
        }
        WM_RENDERALLFORMATS => {
            let _ = std::panic::catch_unwind(|| handle_render_all(hwnd));
            0
        }
        WM_DESTROYCLIPBOARD => {
            let _ = std::panic::catch_unwind(handle_destroy_clipboard);
            0
        }
        WM_APP_ARM => {
            let _ = std::panic::catch_unwind(|| open_and_promise(hwnd, true));
            0
        }
        WM_APP_REARM => {
            let _ = std::panic::catch_unwind(|| open_and_promise(hwnd, false));
            0
        }
        WM_APP_END => {
            let _ = std::panic::catch_unwind(handle_app_end);
            0
        }
        WM_DESTROY => {
            *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = None; // safety net
            RemoveClipboardFormatListener(hwnd);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

pub fn start(app: AppHandle) {
    let mut g = WATCHER.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() {
        return; // already running
    }
    WATCHER_STOP.store(false, Ordering::Release);
    *WATCHER_APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app);
    let handle = std::thread::spawn(|| unsafe {
        let class = wide(crate::brand::WINDOW_CLASS);
        let name = wide("psclip");
        let wc = WndClassW {
            style: 0,
            wndproc: Some(wndproc),
            cls_extra: 0,
            wnd_extra: 0,
            hinstance: 0,
            hicon: 0,
            hcursor: 0,
            hbr: 0,
            menu: std::ptr::null(),
            class_name: class.as_ptr(),
        };
        RegisterClassW(&wc); // ok if already registered from a previous cycle
        let hwnd = CreateWindowExW(0, class.as_ptr(), name.as_ptr(), 0, 0, 0, 0, 0, HWND_MESSAGE, 0, 0, std::ptr::null());
        if hwnd == 0 {
            return;
        }
        AddClipboardFormatListener(hwnd);
        WATCHER_HWND.store(hwnd, Ordering::Release);
        // stop() may have fired before the window existed (its PostMessage would
        // have been lost): tear down now instead of blocking on GetMessageW forever.
        if WATCHER_STOP.load(Ordering::Acquire) {
            RemoveClipboardFormatListener(hwnd);
            DestroyWindow(hwnd);
            WATCHER_HWND.store(0, Ordering::Release);
            return;
        }
        let mut msg: Msg = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    *g = Some(handle);
}

pub fn stop() {
    // Signal first: a thread still starting up will see this right after it
    // publishes its hwnd and exit on its own (see start()).
    WATCHER_STOP.store(true, Ordering::Release);
    let hwnd = WATCHER_HWND.swap(0, Ordering::AcqRel);
    if hwnd != 0 {
        // WM_CLOSE -> DefWindowProc destroys -> WM_DESTROY -> PostQuitMessage.
        unsafe { PostMessageW(hwnd, 0x0010, 0, 0); }
    }
    if let Some(h) = WATCHER.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = h.join();
    }
    *WATCHER_APP.lock().unwrap_or_else(|e| e.into_inner()) = None;
}
