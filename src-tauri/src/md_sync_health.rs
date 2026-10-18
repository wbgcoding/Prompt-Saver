use super::*;

// ---- How the folder itself is doing (shown in the expert menu) ----
#[derive(Clone, Default)]
pub(crate) struct SyncHealth {
    /// Unix seconds of the last round that could read the folder.
    pub(crate) last_ok: u64,
    /// Files seen in that round.
    pub(crate) files: usize,
    pub(crate) fails: u32,
    pub(crate) paused: bool,
    /// Plain-language reason the poll is paused, for the status line.
    pub(crate) reason: String,
}

pub(crate) static SYNC_HEALTH: std::sync::OnceLock<Mutex<SyncHealth>> = std::sync::OnceLock::new();

pub(crate) fn sync_health() -> &'static Mutex<SyncHealth> {
    SYNC_HEALTH.get_or_init(|| Mutex::new(SyncHealth::default()))
}

pub(crate) fn sync_health_lock() -> std::sync::MutexGuard<'static, SyncHealth> {
    sync_health().lock().unwrap_or_else(|e| e.into_inner())
}

// Returns true exactly on the round that flips the pause on, so the log line is
// written once instead of on every failed round.
pub(crate) fn sync_health_note_fail(h: &mut SyncHealth, max: u32, reason: &str) -> bool {
    h.fails = h.fails.saturating_add(1);
    h.reason = reason.to_string();
    if h.paused || h.fails < max {
        return false;
    }
    h.paused = true;
    true
}

pub(crate) fn sync_health_note_ok(h: &mut SyncHealth, files: usize, now: u64) {
    h.fails = 0;
    h.paused = false;
    h.reason.clear();
    h.files = files;
    h.last_ok = now;
}

// Switching to a folder that was written fine ends whatever the old one caused: the
// pause and its reason describe a folder the app no longer uses.
pub(crate) fn sync_health_after_dir_change(h: &mut SyncHealth) {
    h.fails = 0;
    h.paused = false;
    h.reason.clear();
}

// Wakes the poll loop out of its (possibly stretched) sleep, so a new folder is read
// at once instead of after up to ten intervals.
pub(crate) static SYNC_WAKE: tokio::sync::Notify = tokio::sync::Notify::const_new();

// A paused poll still comes back, just far less often.
pub(crate) fn sync_wait_secs(paused: bool, poll: u64) -> u64 {
    if paused {
        poll.saturating_mul(SYNC_PAUSED_FACTOR)
    } else {
        poll
    }
}

pub(crate) fn sync_lock_path(md_path: &Path) -> PathBuf {
    let mut s = md_path.as_os_str().to_os_string();
    s.push(".lock");
    PathBuf::from(s)
}

pub(crate) fn pid_from_lock(lock_path: &Path) -> Option<u32> {
    fs::read_to_string(lock_path).ok()?.trim().parse().ok()
}

#[cfg(windows)]
pub(crate) fn pid_is_alive(pid: u32) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn CloseHandle(h: isize) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            false
        } else {
            CloseHandle(h);
            true
        }
    }
}
#[cfg(not(windows))]
pub(crate) fn pid_is_alive(_pid: u32) -> bool {
    true // conservative off Windows: never treat a lock as stale
}

pub(crate) fn sync_lock_is_stale(lock_path: &Path) -> bool {
    let Ok(meta) = fs::metadata(lock_path) else { return true }; // gone already
    let old_enough = meta
        .modified()
        .and_then(|m| m.elapsed().map_err(std::io::Error::other))
        .map(|d| d.as_secs() >= SYNC_LOCK_STALE_SECS)
        .unwrap_or(true);
    if !old_enough {
        return false;
    }
    match pid_from_lock(lock_path) {
        Some(pid) => !pid_is_alive(pid),
        None => true, // unreadable/garbage lock content: treat as abandoned
    }
}

// Atomic create (never check then create, which races between instances). A held,
// non-stale lock means "leave this file for the next poll", not a blocking wait: this runs
// in a background task with no UI.
pub(crate) fn sync_lock_acquire(md_path: &Path) -> bool {
    let lock_path = sync_lock_path(md_path);
    let try_create = |p: &Path| {
        fs::OpenOptions::new().write(true).create_new(true).open(p).map(|mut f| {
            let _ = std::io::Write::write_all(&mut f, std::process::id().to_string().as_bytes());
        })
    };
    if try_create(&lock_path).is_ok() {
        return true;
    }
    if sync_lock_is_stale(&lock_path) {
        let _ = fs::remove_file(&lock_path);
        return try_create(&lock_path).is_ok();
    }
    false
}

pub(crate) fn sync_lock_release(md_path: &Path) {
    let _ = fs::remove_file(sync_lock_path(md_path));
}
