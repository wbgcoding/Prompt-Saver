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

pub(crate) fn sync_health_lock() -> std::sync::MutexGuard<'static, SyncHealth> {
    sync_state().health.lock().unwrap_or_else(|e| e.into_inner())
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

// What a lock file says about its holder: the computer, the process and when it took the lock.
// A bare number is the older format (a process id only), read as a holder on another computer
// because nothing says which computer it was.
#[derive(Debug, PartialEq)]
pub(crate) enum SyncLockHolder {
    Here { pid: u32, since_ns: u128 },
    Elsewhere,
    Unreadable,
}

pub(crate) fn sync_lock_holder(text: &str) -> SyncLockHolder {
    let mut lines = text.lines().map(str::trim);
    let Some(first) = lines.next() else { return SyncLockHolder::Unreadable };
    if first.parse::<u32>().is_ok() && lines.next().is_none() {
        return SyncLockHolder::Elsewhere;
    }
    let (Some(pid), Some(since)) = (lines.next().and_then(|l| l.parse::<u32>().ok()), lines.next().and_then(|l| l.parse::<u128>().ok())) else {
        return SyncLockHolder::Unreadable;
    };
    if first.is_empty() {
        return SyncLockHolder::Unreadable;
    }
    if first.eq_ignore_ascii_case(&local_host_name()) {
        SyncLockHolder::Here { pid, since_ns: since }
    } else {
        SyncLockHolder::Elsewhere
    }
}

#[cfg(windows)]
mod winproc {
    #[repr(C)]
    #[derive(Default)]
    pub(super) struct FileTime {
        pub lo: u32,
        pub hi: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        pub(super) fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        pub(super) fn CloseHandle(h: isize) -> i32;
        pub(super) fn GetLastError() -> u32;
        pub(super) fn GetProcessTimes(h: isize, creation: *mut FileTime, exit: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
    }
    pub(super) const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    pub(super) const ERROR_ACCESS_DENIED: u32 = 5;
}

// Whether a process with this id exists. A process this user may not open (another account's,
// or a protected one) exists all the same.
#[cfg(windows)]
pub(crate) fn pid_is_alive(pid: u32) -> bool {
    use winproc::*;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        CloseHandle(h);
        true
    }
}

// When the process with this id started, as unix nanoseconds. None when it cannot be told.
#[cfg(windows)]
pub(crate) fn pid_started_ns(pid: u32) -> Option<u128> {
    use winproc::*;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            return None;
        }
        let (mut c, mut e, mut k, mut u) = (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
        let ok = GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        let ticks = ((c.hi as u128) << 32) | c.lo as u128; // 100 ns since 1601
        ticks.checked_sub(116_444_736_000_000_000).map(|t| t * 100)
    }
}

pub(crate) fn sync_lock_is_stale(lock_path: &Path) -> bool {
    let Ok(meta) = fs::metadata(lock_path) else { return true }; // gone already
    // A lock dated in the future (the other computer's clock runs ahead) is not old.
    let Some(age) = meta.modified().ok().and_then(|m| m.elapsed().ok()).map(|d| d.as_secs()) else { return false };
    let text = fs::read(lock_path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    match sync_lock_holder(&text) {
        SyncLockHolder::Here { pid, since_ns } => {
            if age < SYNC_LOCK_STALE_SECS {
                return false;
            }
            if !pid_is_alive(pid) {
                return true;
            }
            // The id may have gone to a newer process since the holder died.
            pid_started_ns(pid).is_some_and(|started| started > since_ns.saturating_add(2_000_000_000))
        }
        // Whether a process on another computer lives cannot be asked; only time tells.
        SyncLockHolder::Elsewhere => age >= SYNC_LOCK_FOREIGN_STALE_SECS,
        SyncLockHolder::Unreadable => age >= SYNC_LOCK_STALE_SECS,
    }
}

// Atomic create (never check then create, which races between instances). A held,
// non-stale lock means "leave this file for the next poll", not a blocking wait: this runs
// in a background task with no UI. The lock names this computer, process and moment, and
// that text is what `sync_lock_release` checks before it deletes.
pub(crate) fn sync_lock_acquire(md_path: &Path) -> bool {
    let lock_path = sync_lock_path(md_path);
    let since = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let token = format!("{}\n{}\n{}", local_host_name(), std::process::id(), since);
    let mut taken = sync_lock_create(&lock_path, &token);
    if !taken && sync_lock_is_stale(&lock_path) {
        let dead = fs::read(&lock_path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        taken = sync_lock_take_over(&lock_path, &dead, &token);
    }
    if taken {
        sync_state().locks.lock().unwrap_or_else(|e| e.into_inner()).insert(sync_seen_key(&lock_path), token);
    }
    taken
}

// Creates the lock file with its text. A write that fails (disk full) must not leave an empty
// lock behind: it would block the file for the whole stale limit.
fn sync_lock_create(lock_path: &Path, token: &str) -> bool {
    let Ok(mut f) = fs::OpenOptions::new().write(true).create_new(true).open(lock_path) else { return false };
    if std::io::Write::write_all(&mut f, token.as_bytes()).is_ok() {
        return true;
    }
    drop(f);
    let _ = fs::remove_file(lock_path);
    false
}

const SYNC_CLAIM_INFIX: &str = ".claim-";

// One claim file per dead lock text, the same name on every computer, so its exclusive create
// lets only one of them replace that lock.
pub(crate) fn sync_claim_path(lock_path: &Path, dead: &str) -> PathBuf {
    let hash = crate::ffmpeg::hex_lower(&crate::ffmpeg::sha256(dead.as_bytes())[..8]);
    let mut s = lock_path.as_os_str().to_os_string();
    s.push(format!("{SYNC_CLAIM_INFIX}{hash}"));
    PathBuf::from(s)
}

// Wins the claim, or not. One left by a computer that died inside the takeover is cleared once
// it is a stale limit old; the rename lets only one process clear it.
fn sync_claim_win(claim: &Path) -> bool {
    let create = || fs::OpenOptions::new().write(true).create_new(true).open(claim);
    match create() {
        Ok(_) => return true,
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return false,
        Err(_) => {}
    }
    let old = fs::metadata(claim).ok().and_then(|m| m.modified().ok()).and_then(|m| m.elapsed().ok());
    if old.is_none_or(|d| d.as_secs() < SYNC_LOCK_STALE_SECS) {
        return false;
    }
    let mut aside = claim.as_os_str().to_os_string();
    aside.push(format!(".stale-{}-{}", local_host_name(), std::process::id()));
    let aside = PathBuf::from(aside);
    if fs::rename(claim, &aside).is_err() {
        return false;
    }
    let _ = fs::remove_file(&aside);
    create().is_ok()
}

// Replaces a lock judged stale. Two computers can judge the same dead lock at the same time;
// the claim for that text lets one of them in, and it looks at the lock again before removing
// it: a claimant that was already through has put a fresh lock there, which must stay.
pub(crate) fn sync_lock_take_over(lock_path: &Path, dead: &str, token: &str) -> bool {
    let claim = sync_claim_path(lock_path, dead);
    if !sync_claim_win(&claim) {
        return false;
    }
    let now = fs::read(lock_path).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
    let taken = match now {
        Some(text) if text != dead => false,
        Some(_) => {
            let _ = fs::remove_file(lock_path);
            sync_lock_create(lock_path, token)
        }
        None => sync_lock_create(lock_path, token),
    };
    let _ = fs::remove_file(&claim);
    taken
}

// Removes the lock only if it is still the one this instance took: a lock taken over after
// ours went stale belongs to someone else by now.
pub(crate) fn sync_lock_release(md_path: &Path) {
    let lock_path = sync_lock_path(md_path);
    let Some(mine) = sync_state().locks.lock().unwrap_or_else(|e| e.into_inner()).remove(&sync_seen_key(&lock_path)) else { return };
    let now = fs::read(&lock_path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    if now == mine {
        let _ = fs::remove_file(&lock_path);
    }
}
