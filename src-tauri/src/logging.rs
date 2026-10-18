use super::*;

// The error log's rotation cap in bytes (expert setting `errorLogKeepKb`). A global because
// dozens of `append_crash_log` call sites have no `State<Db>`: set at startup from the
// persisted setting and again by `set_ui_value("errorLogKeepKb", ..)`.
pub(crate) static ERROR_LOG_KEEP_BYTES: AtomicU64 = AtomicU64::new(256 * 1024);

// Name of the env var `crash_log_path` checks before the real %APPDATA% location. Checked
// unconditionally (not only in test builds) so a test proves the redirect on the exact
// value `append_crash_log` reads, not on a separate compile-time flag that could drift.
pub(crate) const CRASH_LOG_DIR_ENV: &str = "PROMPT_SAVER_TEST_CRASH_LOG_DIR";

// Where `append_crash_log` writes. `cargo test` must never touch the real file, and dozens
// of call sites fire during unrelated tests, so the first call from a test binary seeds
// `CRASH_LOG_DIR_ENV` to a per-process temp folder (the `ps-<name>test-<pid>` shape the
// recording tests use) and later calls just read it; `Once` keeps the seeding race-free.
pub(crate) fn crash_log_path() -> PathBuf {
    #[cfg(test)]
    {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            if std::env::var_os(CRASH_LOG_DIR_ENV).is_none() {
                let dir = std::env::temp_dir().join(format!("ps-crashlogtest-{}", std::process::id()));
                std::env::set_var(CRASH_LOG_DIR_ENV, &dir);
            }
        });
    }
    let dir = match std::env::var_os(CRASH_LOG_DIR_ENV) {
        Some(dir) => PathBuf::from(dir),
        None => std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(DATA_FOLDER),
    };
    dir.join("error.log")
}

// A cheap cross-process, cross-thread mutex for the crash log: an exclusively created
// marker file beside it (`create_new` is atomic against another thread and a second
// instance), cheaper than wiring up LockFileEx for a small, rarely rotated file.
// Best-effort like the rest of this module: the marker holds the owner's process id, so one
// left by a crash is recognised (owner gone, or the file too old) and reclaimed at once, and
// acquiring gives up after a short wait and proceeds unlocked rather than freeze the caller.
pub(crate) struct CrashLogLock {
    pub(crate) lock_path: PathBuf,
    pub(crate) held: bool,
}

// A marker older than this is stale whoever owns it: no log write takes that long.
const CRASH_LOCK_STALE_AFTER: Duration = Duration::from_secs(10);
// A marker without a readable process id gets this long to be filled in by its creator.
const CRASH_LOCK_UNPARSABLE_AFTER: Duration = Duration::from_secs(2);

fn process_is_running(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED};
    use windows::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // GetExitCodeProcess reports this while the process still runs.
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                // An exited process stays openable while anyone holds a handle to it.
                let mut code = 0u32;
                let alive = GetExitCodeProcess(handle, &mut code).is_err() || code == STILL_ACTIVE;
                let _ = CloseHandle(handle);
                alive
            }
            // A process this one may not open is still a process.
            Err(_) => GetLastError() == ERROR_ACCESS_DENIED,
        }
    }
}

// True when the marker exists and nothing alive owns it any more.
fn crash_lock_is_stale(lock_path: &Path) -> bool {
    let Ok(meta) = fs::metadata(lock_path) else { return false };
    let age = meta.modified().ok().and_then(|m| m.elapsed().ok()).unwrap_or(Duration::ZERO);
    if age > CRASH_LOCK_STALE_AFTER {
        return true;
    }
    match fs::read_to_string(lock_path).ok().and_then(|t| t.trim().parse::<u32>().ok()) {
        Some(pid) => !process_is_running(pid),
        None => age > CRASH_LOCK_UNPARSABLE_AFTER,
    }
}

impl CrashLogLock {
    fn try_claim(lock_path: &Path) -> bool {
        use std::io::Write;
        match fs::OpenOptions::new().write(true).create_new(true).open(lock_path) {
            Ok(mut f) => {
                let _ = write!(f, "{}", std::process::id());
                true
            }
            Err(_) => false,
        }
    }

    // Removes a stale marker; the next claim is the caller's to try.
    fn reclaim_if_stale(lock_path: &Path) -> bool {
        crash_lock_is_stale(lock_path) && fs::remove_file(lock_path).is_ok()
    }

    pub(crate) fn acquire(path: &Path) -> Self {
        let lock_path = path.with_extension("log.lock");
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        Self::reclaim_if_stale(&lock_path);
        loop {
            if Self::try_claim(&lock_path) {
                return Self { lock_path, held: true };
            }
            if std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            // Out of time: one last look for a marker that went stale while waiting.
            let held = Self::reclaim_if_stale(&lock_path) && Self::try_claim(&lock_path);
            return Self { lock_path, held };
        }
    }
}

impl Drop for CrashLogLock {
    fn drop(&mut self) {
        if self.held {
            let _ = fs::remove_file(&self.lock_path);
        }
    }
}

pub(crate) fn with_crash_log_lock<T>(path: &Path, f: impl FnOnce() -> T) -> T {
    let _lock = CrashLogLock::acquire(path);
    f()
}

// Trims `path` to its last `cap` bytes, cutting on a line boundary so the kept tail never
// opens on a fragment of a line. The remainder goes to a side file, flushed and synced,
// then swapped in with `fs::rename`, so a crash mid-write leaves the old complete file
// standing, and the rename never overlaps a concurrent append because both take
// `with_crash_log_lock`. This variant runs inside `append_crash_log`'s own lock and must
// not acquire it twice; `rotate_log_file` is the self-locking wrapper.
pub(crate) fn rotate_log_file_locked(path: &Path, cap: usize) {
    let Ok(contents) = fs::read(path) else { return };
    if contents.len() <= cap {
        return;
    }
    let cut_from = contents.len() - cap;
    let start = contents[cut_from..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|i| cut_from + i + 1)
        .unwrap_or(contents.len());
    let _ = write_bytes_atomic(path, &contents[start..]);
}

// Self-locking wrapper around the rotation, so a test can prove the trimming against a
// private file; `append_crash_log` uses `append_line_locked`, which shares one lock across
// rotation and append.
#[cfg(test)]
pub(crate) fn rotate_log_file(path: &Path, cap: usize) {
    with_crash_log_lock(path, || rotate_log_file_locked(path, cap));
}

// One line, written under the same lock as the rotation that may precede it. Takes an
// explicit path because unrelated tests append to the redirected crash log concurrently,
// and asserting exact content against that shared target would race. Shared by
// `append_crash_log` and its tests.
pub(crate) fn append_line_locked(path: &Path, cap: usize, line: &str) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    with_crash_log_lock(path, || {
        rotate_log_file_locked(path, cap);
        use std::io::Write;
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    });
}

// Append one line to a local crash log so a hard failure on a user's machine leaves
// something diagnosable. Privacy: the message is only the panic location and our own static
// panic text, never prompt content, and the file is never uploaded. Rotates past
// `errorLogKeepKb` (default 256 KB) by trimming the oldest bytes, so the newest failure is
// never the line thrown away. Best-effort: every step ignores failure.
pub(crate) fn append_crash_log(msg: &str) {
    let path = crash_log_path();
    let cap = ERROR_LOG_KEEP_BYTES.load(std::sync::atomic::Ordering::Relaxed) as usize;
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let clean = scrub_log_line(msg);
    append_line_locked(&path, cap, &format!("[t={} v{}] {}", secs, env!("CARGO_PKG_VERSION"), clean));
}

// The data and sync folders `append_crash_log`'s ~50 call sites have no `AppHandle`
// to resolve on the spot: `data_dir` and `sync_dir_path` record whatever they last
// resolved here instead, as a side effect, so a log line can be scrubbed without
// adding an `AppHandle` parameter to every one of those call sites.
pub(crate) static LAST_KNOWN_DATA_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
pub(crate) static LAST_KNOWN_SYNC_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

// Every folder this build knows the identity of, longest path first so a folder
// nested inside another (the sync folder inside the data folder, the data folder
// inside the user profile) is matched by its own, more specific placeholder before
// the outer one gets a chance to eat part of it.
pub(crate) fn known_path_placeholders() -> Vec<(PathBuf, &'static str)> {
    let mut out: Vec<(PathBuf, &'static str)> = Vec::new();
    if let Some(p) = LAST_KNOWN_SYNC_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        out.push((p, "<sync>"));
    }
    if let Some(p) = LAST_KNOWN_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        out.push((p, "<data>"));
    }
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf())) {
        out.push((exe_dir, "<app>"));
    }
    out.push((std::env::temp_dir(), "<temp>"));
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        out.push((home, "<user>"));
    }
    out.sort_by_key(|(p, _)| std::cmp::Reverse(p.as_os_str().len()));
    out
}

// Case-insensitive substring replace that keeps byte offsets valid across the
// swap: ASCII-only lowercasing never changes a string's byte length, so the
// indices found against the lowercased copy still cut the original in the right
// place.
pub(crate) fn replace_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    let lower_needle = needle.to_ascii_lowercase();
    let mut lower_rest = haystack.to_ascii_lowercase();
    let mut rest = haystack;
    let mut out = String::with_capacity(haystack.len());
    while let Some(idx) = lower_rest.find(&lower_needle) {
        out.push_str(&rest[..idx]);
        out.push_str(replacement);
        rest = &rest[idx + needle.len()..];
        lower_rest = lower_rest[idx + needle.len()..].to_string();
    }
    out.push_str(rest);
    out
}

// Swaps every folder this build recognises for its placeholder, tried both with
// backslashes and with the forward slashes a cross-platform crate's own error text
// sometimes uses in place of them.
pub(crate) fn replace_known_dirs(line: &str, dirs: &[(PathBuf, &'static str)]) -> String {
    let mut out = line.to_string();
    for (dir, placeholder) in dirs {
        let win = dir.to_string_lossy().into_owned();
        if win.is_empty() {
            continue;
        }
        out = replace_case_insensitive(&out, &win, placeholder);
        let alt = win.replace('\\', "/");
        if alt != win {
            out = replace_case_insensitive(&out, &alt, placeholder);
        }
    }
    out
}

// Whether `s` is shaped like a filesystem path worth shortening: drive-rooted
// (`D:\...`), a UNC share (`\\server\...`), or Unix-style with at least two
// segments (`/mnt/data/...`), and it actually carries a separator beyond that
// root so a bare `C:` or `/` is left alone.
pub(crate) fn looks_like_a_path(s: &str) -> bool {
    let bytes = s.as_bytes();
    let drive_rooted = bytes.len() > 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/');
    let unc = s.starts_with(r"\\") && s.len() > 2;
    let unix_like = s.starts_with('/') && s.matches('/').count() >= 2;
    drive_rooted || unc || unix_like
}

// Whether `line[at..]` starts a path, and if it is preceded by an ASCII
// letter or digit (so a bare "D:\" is only ever recognised at a real word
// boundary, never mid-word).
pub(crate) fn path_starts_at(line: &str, at: usize) -> bool {
    if at > 0 {
        let prev = line[..at].chars().next_back().unwrap_or(' ');
        if prev.is_ascii_alphanumeric() {
            return false;
        }
    }
    looks_like_a_path(&line[at..])
}

// Last, structural line of defence for `append_crash_log`: any drive-rooted, UNC or
// Unix-style path that slipped past the named placeholders (e.g. a data or sync folder this
// run never resolved) is cut down to its file name. A path can contain spaces ("D:\Private
// Folder\x.mp4"), so this scans the line once instead of splitting on whitespace: a path
// inside matching quotes runs to the closing quote, an unquoted one to the next quote
// character or the end of the line.
pub(crate) fn shorten_stray_paths(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        let ch = rest.chars().next().unwrap();
        if ch == '"' || ch == '\'' {
            let after_quote = i + ch.len_utf8();
            if let Some(rel_end) = line[after_quote..].find(ch) {
                let inner = &line[after_quote..after_quote + rel_end];
                if looks_like_a_path(inner) {
                    let name = inner.rsplit(['\\', '/']).find(|s| !s.is_empty()).unwrap_or(inner);
                    out.push(ch);
                    out.push_str(name);
                    out.push(ch);
                    i = after_quote + rel_end + ch.len_utf8();
                    continue;
                }
            }
        } else if path_starts_at(line, i) {
            let end = rest.find(['"', '\'']).map(|e| i + e).unwrap_or(line.len());
            let raw = &line[i..end];
            let trimmed = raw.trim_end_matches(|c: char| " .,;:)]}".contains(c));
            let suffix = &raw[trimmed.len()..];
            let name = trimmed.rsplit(['\\', '/']).find(|s| !s.is_empty()).unwrap_or(trimmed);
            out.push_str(name);
            out.push_str(suffix);
            i = end;
            continue;
        }
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

// One line's worth of cleanup for the crash log: named folders first (readable
// placeholders for the ones this build actually knows), then a structural
// catch-all for anything shaped like a path that is not one of them. Shared by
// `append_crash_log`, which scrubs as it writes, and `strip_user_path_lines`,
// which re-applies it once to whatever an older build already wrote.
pub(crate) fn scrub_log_line(line: &str) -> String {
    shorten_stray_paths(&replace_known_dirs(line, &known_path_placeholders()))
}

// Rewrites `path` with every line run through `scrub_log_line`, replacing a path in place
// rather than deleting the line (the rest is still a useful diagnostic). Takes an explicit
// path for the same testability reason as `rotate_log_file`. It reads raw bytes and decodes
// each line lossily: an older crash log can carry a stray non-UTF-8 byte (a raw OS error
// message, a multi-byte character cut by an old rotation), and a whole-file read_to_string
// would fail on it and skip scrubbing every other line. Rewritten atomically (temp file +
// rename) under the lock `append_crash_log` uses, so it never interleaves with a concurrent
// append.
pub(crate) fn strip_user_path_lines(path: &Path) {
    // Read, transform and swap all under the one lock: an append landing between a
    // read outside it and the rename would be overwritten by the older copy.
    with_crash_log_lock(path, || {
        let Ok(bytes) = fs::read(path) else { return };
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut changed = false;
        for raw_line in bytes.split_inclusive(|&b| b == b'\n') {
            let (body, had_newline) = match raw_line.strip_suffix(b"\n") {
                Some(b) => (b, true),
                None => (raw_line, false),
            };
            let text = String::from_utf8_lossy(body);
            let cleaned = scrub_log_line(&text);
            if cleaned != text {
                changed = true;
            }
            out.extend_from_slice(cleaned.as_bytes());
            if had_newline {
                out.push(b'\n');
            }
        }
        if !changed {
            return;
        }
        let _ = write_bytes_atomic(path, &out);
    });
}

// One-time cleanup: an older build's crash log can carry the build machine's user-profile
// path in a panic location from outside this workspace (a dependency compiles with its full
// source path); `build.bat`'s `--remap-path-prefix` only keeps it out of entries logged by
// later builds. Rewrites the file without those paths. That path belongs to the machine
// that built the exe, essentially never the one running it, so this checks the structural
// shape of a user path rather than today's account name. Best-effort; runs once at startup.
pub(crate) fn scrub_user_paths_from_crash_log() {
    strip_user_path_lines(&crash_log_path());
}

// ---------- Recent messages ----------
// A ring buffer of the errors shown to the user, so one that flashed by in a toast can be
// found afterwards. Filled from `errToast` on the frontend (where every failed command and
// background event ends up), never from the crash log, or a line would show twice. The
// remote-access receiver and promptsaver:// links (remote.rs) also record every access
// here, so a silent read or write from outside this window leaves a visible trace.
#[derive(Clone, serde::Serialize)]
pub(crate) struct ErrorLogEntry {
    pub(crate) ts: u64,
    pub(crate) text: String,
    pub(crate) origin: String,
    pub(crate) raw: Option<String>,
}

// How many recent error messages the in-app list keeps.
pub(crate) const ERROR_LOG_MAX: usize = 100;
pub(crate) static ERROR_LOG: Mutex<Vec<ErrorLogEntry>> = Mutex::new(Vec::new());

pub(crate) fn error_log_enabled(settings: &Settings) -> bool {
    settings.ui_flags.get("errorLog") != Some(&false) // on by default
}

// Same privacy bar as the crash log (`scrub_log_line`): a path collapses to its file name.
// No caller passes prompt content, but a stray long string is collapsed to a short stand-in
// so a future call site cannot leak one by accident.
pub(crate) const ERROR_LOG_TEXT_MAX_CHARS: usize = 160;

// The crash-log line for a panic. A std panic message can quote the data it choked on (a slice
// on a char boundary quotes the string, an unwrap prints the error's Debug), so only the
// location and the message up to its first quote or backtick are kept, capped in length.
pub(crate) fn panic_log_line(location: Option<(&str, u32, u32)>, payload: &str) -> String {
    let cut = payload.find(['`', '\'', '"']).unwrap_or(payload.len());
    let msg: String = payload[..cut].trim_end().chars().take(80).collect();
    match location {
        Some((file, line, col)) => scrub_log_line(&format!("panicked at {file}:{line}:{col}: {msg}")),
        None => scrub_log_line(&format!("panicked: {msg}")),
    }
}

pub(crate) fn scrub_error_log_text(s: &str) -> String {
    let path_clean = scrub_log_line(s);
    if path_clean.chars().count() > ERROR_LOG_TEXT_MAX_CHARS {
        let id: String = path_clean.chars().take(8).collect();
        format!("<{id}...>")
    } else {
        path_clean
    }
}

// Every value `origin` may carry: every overlay/modal id `currentErrorOrigin()` (core.js)
// can read off the open dialog, the two non-modal screens it falls back to, and the two
// backend-only origins (`remoteApi`, the remote-access receiver; `test`, used by this
// file's tests). `log_error` is an IPC command, so its `origin` is as untrusted as any
// caller string; an allow-list with a fixed "other" fallback keeps it from becoming a
// free-text field, as `scrub_error_log_text` does for `text`/`raw`.
pub(crate) const ERROR_LOG_ORIGINS: [&str; 34] = [
    "about-modal", "backup-modal", "batch-export-modal", "compare-modal", "confirm-modal",
    "dupes-modal", "error-log-modal", "global-search", "help", "import-folder-modal",
    "import-preview-modal", "library", "lightbox", "modal", "ocr-langs-modal", "ocr-modal",
    "rec-modal", "settings", "shortcuts-modal", "snip-edit", "snip-modal", "stats-modal", "tag-modal",
    "theme-modal", "tour-modal", "trash-modal", "update-modal", "ve-overlay-live",
    "video-cleanup-modal", "view-modal", "settings-expert", "main", "video", "snip",
];

pub(crate) fn sanitize_error_log_origin(origin: &str) -> &'static str {
    if let Some(&known) = ERROR_LOG_ORIGINS.iter().find(|&&known| known == origin) {
        return known;
    }
    match origin {
        "remoteApi" => "remoteApi",
        "urlScheme" => "urlScheme",
        "test" => "test",
        _ => "other",
    }
}

// Newest first; anything past `max` is dropped from the old end.
pub(crate) fn record_error_log(text: &str, origin: &str, raw: Option<&str>, max: usize) {
    let mut log = ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner());
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    log.insert(
        0,
        ErrorLogEntry {
            ts,
            text: scrub_error_log_text(text),
            origin: sanitize_error_log_origin(origin).to_string(),
            raw: raw.map(scrub_error_log_text),
        },
    );
    log.truncate(max);
}

// Called from `errToast` for every error message shown to the user. Silently a
// no-op while the switch is off: the buffer is also cleared the moment it is
// switched off (see `set_ui_flag`), so "off" always means an empty list, not
// just a paused one.
#[tauri::command(async)]
pub(crate) fn log_error(state: State<'_, Db>, text: String, origin: String, raw: Option<String>) {
    // Only the one flag is read: the settings (layouts, texts) are not copied for it.
    let enabled = error_log_enabled(&lock(&state).settings);
    if !enabled {
        return;
    }
    record_error_log(&text, &origin, raw.as_deref(), ERROR_LOG_MAX);
}

#[tauri::command]
pub(crate) fn list_error_log() -> Vec<ErrorLogEntry> {
    ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub(crate) fn clear_error_log() {
    ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner()).clear();
}
