use super::*;

// ---- diff writes: a save only re-seals the rows that really changed ----

pub(super) fn row_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);",
    )
    .unwrap();
    conn
}

// (id, ord, stored blob) in stored order: the blob byte-for-byte, which is how
// these tests tell a rewritten row from an untouched one.
pub(super) fn stored_rows(conn: &Connection, table: &str) -> Vec<(String, i64, String)> {
    conn.prepare(&format!("SELECT id, ord, data FROM {table} ORDER BY ord"))
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .flatten()
        .collect()
}

pub(super) fn numbered_prompts(n: usize) -> Vec<Prompt> {
    (0..n)
        .map(|i| Prompt { name: format!("Name {i}"), ..test_prompt(&format!("id{i:03}")) })
        .collect()
}

pub(super) fn stored_prompt(blob: &str) -> Prompt {
    serde_json::from_str(&dec_str(blob)).unwrap()
}

// Prompt carries no PartialEq/Debug (nothing in the app compares two of them),
// so whole-prompt comparisons in these tests go through its json.
pub(super) fn as_json(p: &Prompt) -> String {
    serde_json::to_string(p).unwrap()
}

// Comparing blobs only proves nothing was re-sealed because sealing is
// randomised: the same text sealed twice comes out as different bytes. Assert
// that rather than assume it: where sealing is unavailable the rows are stored
// as plain text and the comparison is weaker, so the check is skipped there.
pub(super) fn sealing_is_randomised() -> bool {
    enc_str("probe").starts_with(ENC_PREFIX)
}

// The whole point of the flag is that it describes the DISK, not an intention:
// whatever enc_str actually produced is what the window is told. Before this,
// a plaintext fallback was reported nowhere at all and the answer would have
// been a hardcoded "sealed" no matter what came back.
#[test]
fn store_sealed_reports_what_a_write_actually_did() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let really_sealed = enc_str("probe").starts_with(ENC_PREFIX);
    assert_eq!(
        store_sealed(),
        really_sealed,
        "the reported state must match what enc_str wrote, in both directions"
    );
}

// Serializes every test that reads or mutates the process-global sealing state
// (STORE_SEALED, SEAL_NOTIFY) against each other: cargo test runs tests on
// separate threads by default, and these are the only globals any test
// deliberately flips. FORCE_PLAINTEXT_FALLBACK is thread-local (see enc_str)
// precisely so it needs no lock to stay isolated from unrelated tests.
pub(super) static SEAL_TEST_LOCK: Mutex<()> = Mutex::new(());

// A row written through the real store writer must never carry its plaintext
// content on disk: proves the writer path, not just enc_str in isolation
// (already covered by the plain round-trip test above it).
#[test]
fn a_sealed_write_never_puts_the_plaintext_marker_on_disk() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !sealing_is_randomised() {
        return; // DPAPI unavailable on this machine -- nothing to prove here
    }
    let mut conn = row_test_db();
    const MARKER: &str = "PLAINTEXT-MARKER-206-must-never-reach-disk";
    let mut prompts = numbered_prompts(1);
    prompts[0].text = MARKER.into();
    let mut written = WrittenRows::new();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();
    let rows = stored_rows(&conn, "prompts");
    let blob = &rows[0].2;
    assert!(blob.starts_with(ENC_PREFIX), "a sealed write must carry the enc prefix");
    assert!(!blob.contains(MARKER), "the plaintext marker must never appear in the stored bytes: {blob}");
}

// The fallback must flip the flag and notify exactly once, even when a
// single save floods many rows through it: not once per row.
// Restores the process-global sealing state on drop: including on a panic
// partway through the test: so one failed assertion here can never leave
// state stuck for every other test that runs after it. FORCE_PLAINTEXT_FALLBACK
// is thread-local, but cargo test pools and reuses OS threads across tests, so
// a later test landing on this same thread would still inherit a stale `true`
// without this reset.
pub(super) struct SealOverrideGuard {
    pub(super) prior_sealed: bool,
}
impl Drop for SealOverrideGuard {
    fn drop(&mut self) {
        FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(false));
        STORE_SEALED.store(self.prior_sealed, Ordering::Relaxed);
        *SEAL_NOTIFY.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

#[test]
fn a_flood_of_unsealed_writes_flips_the_flag_and_notifies_exactly_once() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Install the counter BEFORE either flag flips. Flipping FORCE first left a
    // window where an unrelated, unguarded test's own enc_str call could catch
    // this test's one true-to-false transition while SEAL_NOTIFY was still the
    // previous (cleared) value: the flag still ended up unsealed, but nothing
    // was there to count it, so the assertion below saw zero calls.
    let calls = Arc::new(AtomicU64::new(0));
    let counter = calls.clone();
    *SEAL_NOTIFY.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(Arc::new(move || { counter.fetch_add(1, Ordering::Relaxed); }));
    let prior_sealed = STORE_SEALED.swap(true, Ordering::Relaxed);
    FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(true));
    let _restore = SealOverrideGuard { prior_sealed };

    let mut conn = row_test_db();
    let prompts = numbered_prompts(50);
    let mut written = WrittenRows::new();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();

    assert!(!store_sealed(), "the flag must report unsealed once the fallback ran");
    assert_eq!(calls.load(Ordering::Relaxed), 1, "the notify must fire exactly once, not once per row");
}

// "A repaired profile re-seals on the next save" (the warning dialog's own
// words) is a promise about the ROW being written, not about STORE_SEALED --
// that flag stays sticky for the rest of the session on purpose (see its
// comment: an old plaintext row is not retroactively sealed just because a
// later write succeeded, so un-warning the user here would be misleading).
// enc_str re-checks DPAPI on every single call with no persisted "give up"
// state, so the row-level promise needs no new machinery: only a test that
// FORCE_PLAINTEXT_FALLBACK really is a per-call toggle and not a latch.
#[test]
fn a_write_after_a_forced_failure_is_sealed_again_once_dpapi_is_available() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !sealing_is_randomised() {
        return; // DPAPI unavailable on this machine -- nothing to prove here
    }
    let prior_sealed = STORE_SEALED.swap(true, Ordering::Relaxed);
    FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(true));
    let _restore = SealOverrideGuard { prior_sealed };

    let unsealed = enc_str("written during the outage");
    assert!(!unsealed.starts_with(ENC_PREFIX), "the forced failure must fall back to plaintext");

    // The profile is "repaired": nothing but the thread-local override changes.
    FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(false));
    let resealed = enc_str("written after the profile was repaired");
    assert!(resealed.starts_with(ENC_PREFIX), "a write with DPAPI available again must come out sealed");
}

// A cargo test run must never touch the real error.log: dozens of call
// sites fire during entirely unrelated tests, not just the ones that test
// logging directly, so this has to hold for the redirect mechanism itself.
#[test]
fn append_crash_log_never_writes_the_real_appdata_file() {
    let real_path = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(DATA_FOLDER)
        .join("error.log");
    let before = fs::metadata(&real_path).ok().map(|m| (m.len(), m.modified().ok()));

    assert_ne!(crash_log_path(), real_path, "a test build must redirect away from the real file");
    const MARKER: &str = "PROBE-crash-log-redirect-never-writes-the-real-appdata-file";
    append_crash_log(MARKER);

    let after = fs::metadata(&real_path).ok().map(|m| (m.len(), m.modified().ok()));
    assert_eq!(before, after, "the real error.log must be untouched by a test run");

    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(redirected.contains(MARKER), "the message must land in the redirected file instead");
}

// Every writer the unit tests reach (the error log, the data folder fallbacks)
// must resolve into a per-run location, never into the owner's real roaming
// folder, and writing through them must leave the real error.log untouched.
#[test]
fn test_runs_never_resolve_into_the_real_data_folder() {
    let real_dir = PathBuf::from(test_env::real_appdata().expect("APPDATA is set on Windows")).join(DATA_FOLDER);
    let real_log = real_dir.join("error.log");
    let stamp = |p: &Path| fs::metadata(p).ok().map(|m| (m.len(), m.modified().ok()));
    let before = stamp(&real_log);
    assert_ne!(std::env::var_os("APPDATA"), test_env::real_appdata(), "the test process must run with a sandboxed APPDATA");

    for (what, path) in [
        ("crash_log_path", crash_log_path()),
        ("default_data_dir_headless", default_data_dir_headless()),
        ("data_dir_headless", data_dir_headless()),
    ] {
        assert!(!path.starts_with(&real_dir), "{what} resolved into the real data folder: {}", path.display());
    }

    // The writers: a plain log line, the startup scrub, and the call sites that
    // fire inside unrelated tests (sealing without DPAPI, a failed conversion).
    append_crash_log("PROBE-guard-plain-line");
    scrub_user_paths_from_crash_log();
    append_crash_log("DPAPI encryption unavailable - probe line from the guard test");
    append_crash_log("recording convert failed (webm): probe line from the guard test");

    assert_eq!(before, stamp(&real_log), "the real error.log must be untouched by the unit tests");
    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(redirected.contains("PROBE-guard-plain-line"), "the lines must land in the per-run file");
}

// Tests write fixed-name scratch files straight into the temp folder; two runs
// at once (two checkouts) or a file an earlier run left behind must never meet
// there, so each test process gets a temp folder named after its own pid.
#[test]
fn test_runs_use_a_temp_folder_of_their_own() {
    let temp = std::env::temp_dir();
    let own = format!("ps-testenv-{}", std::process::id());
    assert!(
        temp.components().any(|c| c.as_os_str().to_string_lossy() == own),
        "the temp folder must sit inside this process's sandbox, got {}",
        temp.display()
    );
    assert!(temp.is_dir(), "the sandbox temp folder must exist");
}

// A data folder outside the profile (a custom location the user picked, or any
// other drive-rooted path this run never resolved to a known folder) is never
// a folder the named placeholders recognise: the structural catch-all must
// still keep it out of the log, down to just the file name.
#[test]
fn append_crash_log_scrubs_a_path_outside_any_known_folder() {
    const MARKER: &str = "PROBE-crash-log-scrub-outside-profile";
    append_crash_log(&format!(r"{MARKER}: could not read D:\Privat\Kunde\x.png"));

    // Checked as a whole-file substring rather than a reconstructed "line":
    // dozens of other tests append to this same shared redirected file at the
    // same time, so only a contiguous match proves anything here.
    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(!redirected.contains(r"D:\Privat\Kunde"), "an unrecognised drive path must never reach the log");
    assert!(
        redirected.contains(&format!("{MARKER}: could not read x.png")),
        "the file name itself may stay, only the folder must go: {redirected}"
    );
}

// The configured sync folder is exactly the kind of user-data folder this
// point exists for: a full path most users will not pick from inside their
// profile. It must come out replaced by its own readable placeholder, not
// merely shortened.
#[test]
fn append_crash_log_scrubs_the_configured_sync_folder() {
    const MARKER: &str = "PROBE-crash-log-scrub-sync-folder";
    let sync_dir = PathBuf::from(r"E:\CloudSync\PromptFolders");
    let prior = LAST_KNOWN_SYNC_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone();
    *LAST_KNOWN_SYNC_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(sync_dir.clone());

    append_crash_log(&format!("{MARKER}: the sync folder could not be created: {}", sync_dir.display()));

    *LAST_KNOWN_SYNC_DIR.lock().unwrap_or_else(|e| e.into_inner()) = prior;

    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(
        !redirected.to_ascii_lowercase().contains("cloudsync"),
        "the sync folder path must never reach the log"
    );
    assert!(
        redirected.contains(&format!("{MARKER}: the sync folder could not be created: <sync>")),
        "the named sync placeholder must take its place: {redirected}"
    );
}

// A message that never carried a path must reach the log exactly as written --
// scrubbing must never mangle ordinary text.
#[test]
fn append_crash_log_leaves_a_message_without_any_path_unchanged() {
    const MARKER: &str = "PROBE-crash-log-scrub-no-path";
    let msg = format!("{MARKER}: persist failed, no path involved at all");
    append_crash_log(&msg);

    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(redirected.contains(&msg), "a path-free message must reach the log exactly as written: {redirected}");
}

// A folder name with a space in it used to survive scrubbing: splitting on
// spaces before deciding what looks like a path left every space-separated
// piece past the first one (here "Folder\x.mp4") sitting in the log in the
// clear. The whole path, spaces and all, must come out as just the file name.
#[test]
fn shorten_stray_paths_removes_a_folder_name_that_contains_a_space() {
    let out = shorten_stray_paths(r"path=D:\Private Folder\x.mp4");
    assert_eq!(out, "path=x.mp4");
    assert!(!out.contains("Private"), "no fragment of the folder name may remain: {out}");
    assert!(!out.contains("Folder"), "no fragment of the folder name may remain: {out}");
}

// The same path, quoted, must shorten to just the file name inside the
// quotes rather than treating the quote character as part of the path or
// leaving the quotes describing something that is no longer there.
#[test]
fn shorten_stray_paths_removes_a_quoted_path_with_spaces() {
    let out = shorten_stray_paths(r#"could not open "D:\Private Folder\x.mp4" for writing"#);
    assert_eq!(out, r#"could not open "x.mp4" for writing"#);
}

#[test]
fn append_crash_log_scrubs_a_folder_name_with_a_space_in_it() {
    const MARKER: &str = "PROBE-crash-log-scrub-space-in-folder";
    append_crash_log(&format!(r"{MARKER}: path=D:\Private Folder\x.mp4"));

    let redirected = fs::read_to_string(crash_log_path()).unwrap_or_default();
    assert!(!redirected.contains("Private Folder"), "a spaced folder name must never reach the log: {redirected}");
    assert!(
        redirected.contains(&format!("{MARKER}: path=x.mp4")),
        "the file name itself may stay, only the folder must go: {redirected}"
    );
}

// record_error_log/ERROR_LOG is a shared static: serialise against other
// tests touching it, same reasoning as SEAL_TEST_LOCK above.
pub(super) static ERROR_LOG_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn record_error_log_keeps_the_cap_and_drops_the_oldest() {
    let _guard = ERROR_LOG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner()).clear();
    for i in 0..7 {
        record_error_log(&format!("message {i}"), "test", None, 5);
    }
    let log = ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(log.len(), 5, "the buffer must never grow past the configured cap");
    assert_eq!(log.first().unwrap().text, "message 6", "the newest entry must be first");
    assert_eq!(log.last().unwrap().text, "message 2", "the oldest entries beyond the cap must have been dropped");
}

#[test]
fn sanitize_error_log_origin_keeps_the_known_set_and_falls_back_to_other() {
    assert_eq!(sanitize_error_log_origin("settings"), "settings");
    assert_eq!(sanitize_error_log_origin("main"), "main");
    assert_eq!(sanitize_error_log_origin("remoteApi"), "remoteApi");
    assert_eq!(sanitize_error_log_origin("urlScheme"), "urlScheme", "link handler lines keep their own origin");
    assert_eq!(sanitize_error_log_origin("video"), "video", "the video editor reports under its own name");
    assert_eq!(sanitize_error_log_origin("snip"), "snip", "the screenshot overlay reports under its own name");
    // Ids whose dialogs left the markup are no longer allow-listed.
    for gone in ["clip-modal", "hotkeys-modal", "journal", "licenses-modal", "vars-modal", "ve-confirm", "whats-new-modal"] {
        assert_eq!(sanitize_error_log_origin(gone), "other", "{gone}");
    }
    assert_eq!(sanitize_error_log_origin("some prompt text smuggled in as an origin"), "other");
    assert_eq!(sanitize_error_log_origin(""), "other");
}

#[test]
fn every_overlay_id_in_the_main_window_is_an_allowed_error_origin() {
    let html = include_str!("../../../ui/index.html");
    let mut missing = Vec::new();
    for line in html.lines() {
        let line = line.trim_start();
        if !line.starts_with("<div id=\"") || !line.contains("class=\"overlay") {
            continue;
        }
        let id = line["<div id=\"".len()..].split('"').next().unwrap();
        if sanitize_error_log_origin(id) != id {
            missing.push(id.to_string());
        }
    }
    assert!(missing.is_empty(), "overlay ids missing from the error origin list: {missing:?}");
}

#[test]
fn the_stats_caches_survive_a_poisoned_lock() {
    let src = include_str!("../stats.rs");
    for name in ["STORAGE_CACHE", "RETENTION_CACHE"] {
        let bare = format!("{name}.lock().unwrap()");
        assert!(!src.contains(&bare), "{name} must recover a poisoned lock with into_inner");
    }
}

#[test]
fn record_error_log_sanitizes_an_origin_outside_the_known_set() {
    let _guard = ERROR_LOG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner()).clear();
    record_error_log("message", "not-a-real-modal-id", None, 5);
    let log = ERROR_LOG.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(log.first().unwrap().origin, "other", "an origin outside the fixed set must never reach the stored entry as-is");
}

#[test]
fn scrub_error_log_text_shortens_a_path_to_its_file_name() {
    let msg = r"could not open D:\Projects\ClientData\secret-notes.txt";
    let clean = scrub_error_log_text(msg);
    assert!(clean.contains("secret-notes.txt"), "the file name must survive the scrub: {clean}");
    assert!(!clean.contains(r"D:\Projects\ClientData"), "the folder part of the path must not survive: {clean}");
}

#[test]
fn scrub_error_log_text_never_writes_a_long_text_whole() {
    // Stands in for prompt content that ended up interpolated into a
    // message: no call site does this today, but the scrub must hold even
    // if one someday does.
    let long_text = "a".repeat(400);
    let clean = scrub_error_log_text(&long_text);
    assert!(clean.len() < long_text.len(), "a long text must be collapsed, never written out whole: {clean}");
    assert!(!clean.contains(&long_text), "the original long text must never appear verbatim in the log");
}

// A log grown past the cap must keep its newest lines and lose only the
// oldest: deleting the whole file (the old behaviour) would throw away the
// real failure a user just hit along with everything before it. Runs against
// its own private file, never the shared redirected crash log, so nothing
// else appending concurrently can move this test's goalposts.
#[test]
fn rotate_log_file_keeps_the_newest_lines_and_drops_only_the_oldest() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-rotate-test-{}", std::process::id()));
    let lines: Vec<String> = (0..50).map(|i| format!("line {i:03} of the log before rotation")).collect();
    let body = lines.iter().map(|l| format!("{l}\n")).collect::<String>();
    fs::write(&path, &body).unwrap();
    let full_len = body.len();

    // Cap well under the full size but well over one line, so several of the
    // oldest lines must go and several of the newest must survive intact.
    let cap = full_len / 3;
    rotate_log_file(&path, cap);

    let kept = fs::read_to_string(&path).unwrap();
    assert!(kept.len() <= cap, "the kept tail must not exceed the cap: {} > {cap}", kept.len());
    assert!(!kept.is_empty(), "some of the log must survive a rotation");
    assert!(
        kept.starts_with("line "),
        "the tail must start on a whole line, not a fragment of one that was cut: {kept:?}"
    );
    assert!(
        lines.last().map(|l| kept.contains(l.as_str())).unwrap_or(false),
        "the newest line must survive the rotation"
    );
    assert!(
        !lines.first().map(|l| kept.contains(l.as_str())).unwrap_or(false),
        "the oldest line must be the one that gets dropped"
    );

    let _ = fs::remove_file(&path);
}

// A file already under the cap must be left completely alone: rotation is
// only for a log that actually outgrew its budget.
#[test]
fn rotate_log_file_leaves_a_file_under_the_cap_untouched() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-rotate-small-test-{}", std::process::id()));
    fs::write(&path, "short log, well under any reasonable cap\n").unwrap();
    let before = fs::read_to_string(&path).unwrap();

    rotate_log_file(&path, 256 * 1024);

    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(before, after, "a file under the cap must not be rewritten at all");
    let _ = fs::remove_file(&path);
}

// A line appended while a rotation is under way must never be lost. Proven
// with a real second thread rather than a mocked lock: this thread holds the
// crash log's own lock first (standing in for a truncation already in
// progress), a channel confirms the appending thread has started only once
// it is actually about to block on that same lock, and only then does this
// thread finish the truncation and let go: so the append is genuinely
// serialised after it, never racing it.
#[test]
fn a_concurrent_append_during_truncation_is_never_lost() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-lock-test-{}", std::process::id()));
    let lock_path = path.with_extension("log.lock");
    let _ = fs::remove_file(&lock_path);
    let lines: Vec<String> = (0..50).map(|i| format!("line {i:03} of the log before rotation")).collect();
    let body = lines.iter().map(|l| format!("{l}\n")).collect::<String>();
    fs::write(&path, &body).unwrap();
    let cap = body.len() / 3;

    let held = CrashLogLock::acquire(&path);
    assert!(held.held, "the test itself must hold the real lock to stand in for a truncation in progress");

    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let path2 = path.clone();
    let appender = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        append_line_locked(&path2, usize::MAX, "line NEW appended during truncation");
    });
    started_rx.recv().unwrap();
    // Give the appending thread a moment to reach the lock and start
    // waiting on it before this thread finishes the "truncation" and lets go.
    std::thread::sleep(Duration::from_millis(100));
    rotate_log_file_locked(&path, cap);
    drop(held);
    appender.join().unwrap();

    let kept = fs::read_to_string(&path).unwrap();
    assert!(
        kept.contains("line NEW appended during truncation"),
        "an append that happened during a truncation must still be in the file: {kept:?}"
    );
    assert!(
        !lines.first().map(|l| kept.contains(l.as_str())).unwrap_or(false),
        "the truncation itself must still have taken effect: {kept:?}"
    );

    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&lock_path);
}

// The side file rotation writes the kept remainder to before renaming it into
// place must never survive the rotation, win or lose.
#[test]
fn rotate_log_file_leaves_no_side_file_behind() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-rotate-sidefile-test-{}", std::process::id()));
    let tmp = path.with_extension("log.tmp");
    let _ = fs::remove_file(&tmp);
    let body: String = (0..50).map(|i| format!("line {i:03} of the log before rotation\n")).collect();
    fs::write(&path, &body).unwrap();

    rotate_log_file(&path, body.len() / 3);

    assert!(!tmp.exists(), "the side file used during rotation must not be left behind");
    let _ = fs::remove_file(&path);
}

// A line carrying a Windows user-profile path: exactly what an unremapped
// panic location from a dependency crate bakes in: keeps its other content;
// only the path on it is replaced, the line as a whole is never deleted.
#[test]
fn strip_user_path_lines_replaces_the_path_but_keeps_the_line() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-scrub-test-{}", std::process::id()));
    let contents = "\
[t=1 v1.0] a clean line with no path in it at all\n\
[t=2 v1.0] panicked at C:\\Users\\example\\.cargo\\registry\\src\\index.crates.io\\somecrate-1.2.3\\src\\lib.rs:42\n\
[t=3 v1.0] another clean line, describing an ordinary failure\n";
    fs::write(&path, contents).unwrap();

    strip_user_path_lines(&path);

    let cleaned = fs::read_to_string(&path).unwrap();
    assert!(!cleaned.to_ascii_lowercase().contains(r"\users\"), "the profile path must be gone: {cleaned:?}");
    assert_eq!(cleaned.lines().count(), 3, "the line must stay, only the path on it is replaced: {cleaned:?}");
    assert!(cleaned.contains("panicked at"), "the rest of the panic line must survive: {cleaned:?}");
    assert!(cleaned.contains("lib.rs:42"), "the file name and line number must survive: {cleaned:?}");
    assert!(cleaned.contains("a clean line with no path in it at all"), "an unrelated line must survive");
    assert!(cleaned.contains("another clean line, describing an ordinary failure"), "an unrelated line must survive");
    let _ = fs::remove_file(&path);
}

// A file with nothing to remove must not be rewritten at all: the healthy,
// common case should cost one read and nothing else.
#[test]
fn strip_user_path_lines_leaves_a_clean_file_untouched() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-scrub-clean-test-{}", std::process::id()));
    fs::write(&path, "nothing here reveals anyone's account name\n").unwrap();
    let before = fs::read_to_string(&path).unwrap();

    strip_user_path_lines(&path);

    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(before, after, "a clean file must not be rewritten");
    let _ = fs::remove_file(&path);
}

// A single invalid UTF-8 byte anywhere in the file (a raw OS error message,
// a rotation cut mid multi-byte character) used to make `fs::read_to_string`
// fail outright, which skipped scrubbing every other line in the file along
// with the broken one. Reading raw bytes and decoding line by line must keep
// scrubbing every well-formed line regardless of what one broken line holds.
#[test]
fn strip_user_path_lines_still_scrubs_other_lines_past_one_invalid_utf8_byte() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-scrub-badutf8-{}", std::process::id()));
    let mut contents = Vec::new();
    contents.extend_from_slice(b"[t=1 v1.0] broken byte follows: \xFF end of line\n");
    contents.extend_from_slice(b"[t=2 v1.0] panicked at C:\\Users\\example\\lib.rs:42");
    contents.push(b'\n');
    fs::write(&path, &contents).unwrap();

    strip_user_path_lines(&path);

    let cleaned = fs::read(&path).unwrap();
    let cleaned_text = String::from_utf8_lossy(&cleaned);
    assert_eq!(cleaned_text.lines().count(), 2, "both lines must survive, invalid byte and all: {cleaned_text:?}");
    assert!(!cleaned_text.to_ascii_lowercase().contains(r"\users\"), "the profile path on the OTHER line must still be scrubbed: {cleaned_text:?}");
    assert!(cleaned_text.contains("lib.rs:42"), "the rest of the second line must survive: {cleaned_text:?}");
    let _ = fs::remove_file(&path);
}

// A plaintext-fallback value whose own text happens to start with the literal
// marker ENC_PREFIX ("enc:v1:") must still round-trip intact, not be misread
// as corrupt ciphertext and erased to "".
#[test]
fn plaintext_fallback_of_text_starting_with_enc_prefix_round_trips() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prior_sealed = STORE_SEALED.swap(true, Ordering::Relaxed);
    FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(true));
    let _restore = SealOverrideGuard { prior_sealed };

    let text = "enc:v1:this looks like ciphertext but is not";
    let stored = enc_str(text);
    assert_ne!(stored, text, "the collision must be escaped, not written raw");
    assert_eq!(dec_str(&stored), text, "the original text must come back unchanged");
}

// A backup that could never be restored (past MAX_INFLATED) must be refused at
// creation time, not silently produced and only discovered useless on the day
// it is actually needed.
#[test]
fn a_store_past_the_restore_ceiling_refuses_to_back_up() {
    assert!(!too_large_to_restore(MAX_INFLATED), "exactly at the ceiling must still be backable up");
    assert!(too_large_to_restore(MAX_INFLATED + 1), "one byte past the ceiling must refuse a backup nobody could ever restore");
}

// The test above only proves the pure ceiling function itself; it never proves
// do_backup's real production path (vacuum_and_check_size) actually runs VACUUM
// INTO the destination it then measures. MAX_INFLATED is 512 MB: deliberately
// far past any real store, and deliberately too large to manufacture in a fast
// unit test, so the reject path itself stays unexercised here; this proves the
// wiring on the path that IS practical to run for real: a normal-sized source
// actually gets vacuumed into `tmp`, not silently skipped or pointed at `src`.
#[test]
fn vacuum_and_check_size_writes_a_real_vacuumed_copy_at_tmp() {
    let dir = std::env::temp_dir().join(format!("{}-test-vacuum-size", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("data.db");
    let tmp = dir.join("data.db.tmp");
    {
        let conn = Connection::open(&src).unwrap();
        conn.execute_batch("CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL); INSERT INTO prompts VALUES('a', 0, 'x');").unwrap();
    }
    assert!(!tmp.exists());
    let result = vacuum_and_check_size(&src, &tmp);
    assert!(result.is_ok(), "a normal-sized store must not be rejected: {result:?}");
    assert!(tmp.exists(), "VACUUM INTO must have actually written the destination file");
    let conn = Connection::open(&tmp).unwrap();
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "the vacuumed copy must contain the real data, not an empty placeholder");
    let _ = std::fs::remove_dir_all(&dir);
}

pub(super) fn stream_test_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}-test-stream-{tag}", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// Deterministic, poorly compressible bytes, well past the 8 KiB copy buffer.
pub(super) fn stream_test_bytes(len: usize) -> Vec<u8> {
    let mut x: u32 = 0x1234_5678;
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 24) as u8
        })
        .collect()
}

#[test]
fn gzip_and_gunzip_stream_a_file_larger_than_the_copy_buffer() {
    let dir = stream_test_dir("roundtrip");
    let raw = dir.join("raw.db");
    let data = stream_test_bytes(3 * 1024 * 1024 + 17);
    std::fs::write(&raw, &data).unwrap();
    let gz = dir.join("data.db.gz");
    gzip_to(&raw, &gz).unwrap();
    assert!(!dir.join("data.db.gz.part").exists(), "the temp file must be renamed away");
    let back = dir.join("back.db");
    gunzip_to(&gz, &back).unwrap();
    assert_eq!(std::fs::read(&back).unwrap(), data, "the round trip must be byte exact");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gzip_to_leaves_no_part_file_when_the_source_is_missing() {
    let dir = stream_test_dir("gzfail");
    let gz = dir.join("data.db.gz");
    assert!(gzip_to(&dir.join("absent.db"), &gz).is_err());
    assert!(!gz.exists());
    assert!(!dir.join("data.db.gz.part").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gunzip_refuses_output_one_byte_above_the_cap_and_removes_it() {
    let dir = stream_test_dir("cap");
    let raw = dir.join("raw.db");
    let data = stream_test_bytes(100_000);
    std::fs::write(&raw, &data).unwrap();
    let gz = dir.join("data.db.gz");
    gzip_to(&raw, &gz).unwrap();
    let out = dir.join("out.db");
    gunzip_to_capped(&gz, &out, data.len() as u64).expect("exactly at the cap is allowed");
    assert_eq!(std::fs::read(&out).unwrap().len(), data.len());
    let err = gunzip_to_capped(&gz, &out, data.len() as u64 - 1).unwrap_err();
    assert!(err.contains("expands"), "{err}");
    assert!(!out.exists(), "a refused inflate must not leave partial output");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gunzip_of_a_truncated_snapshot_fails_and_leaves_nothing() {
    let dir = stream_test_dir("trunc");
    let raw = dir.join("raw.db");
    std::fs::write(&raw, stream_test_bytes(200_000)).unwrap();
    let gz = dir.join("data.db.gz");
    gzip_to(&raw, &gz).unwrap();
    let bytes = std::fs::read(&gz).unwrap();
    std::fs::write(&gz, &bytes[..bytes.len() / 2]).unwrap();
    let out = dir.join("out.db");
    assert!(gunzip_to(&gz, &out).is_err(), "a cut-off stream must never count as restored");
    assert!(!out.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

// The store lock must be free while the copy is being packed, and still held
// while the point-in-time copy itself is taken.
#[test]
fn backup_releases_the_store_lock_before_packing() {
    let dir = stream_test_dir("lock");
    {
        let conn = Connection::open(dir.join("data.db")).unwrap();
        conn.execute_batch("CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL); INSERT INTO prompts VALUES('a', 0, 'x');").unwrap();
    }
    let root = dir.join("backups");
    let store = std::sync::Mutex::new(0u8);
    let guard = store.lock().unwrap();
    let mut free_while_paused = false;
    let name = backup_released(guard, &dir, &root, 5, None, || {
        free_while_paused = store.try_lock().is_ok();
    })
    .unwrap();
    assert!(free_while_paused, "packing must run with the store lock released");
    let folder = root.join(&name);
    assert!(folder.join(BACKUP_DB_GZ).exists());
    assert!(!folder.join("data.db.tmp").exists(), "the raw copy must not be left behind");
    let check = dir.join("check.db");
    gunzip_to(&folder.join(BACKUP_DB_GZ), &check).unwrap();
    let n: i64 = Connection::open(&check).unwrap().query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1);
    // A failing snapshot frees the lock too.
    let guard = store.lock().unwrap();
    let err = backup_released(guard, &dir.join("nowhere"), &root, 5, None, || {}).unwrap_err();
    assert_eq!(err, "no-store");
    assert!(store.try_lock().is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

// Stand-in for one stored setting plus one registry value.
pub(super) struct TwoStateWorld {
    setting: bool,
    os: Option<String>,
    save_fails: bool,
    os_fails: bool,
    restore_save_fails: bool,
}

pub(super) fn run_two_state(w: &mut TwoStateWorld, want: bool) -> Result<(), String> {
    let prior = w.setting;
    let prior_os = w.os.clone();
    let os_cell = std::cell::RefCell::new(w.os.clone());
    let (os_fails, save_fails, restore_save_fails) = (w.os_fails, w.save_fails, w.restore_save_fails);
    let saved = std::cell::Cell::new(prior);
    let mut setting = prior;
    let result = two_state_change(
        &mut setting,
        prior_os,
        |s| {
            *s = want;
            if save_fails {
                false
            } else {
                saved.set(want);
                true
            }
        },
        |s, persist| {
            *s = prior;
            if !persist {
                return true;
            }
            if restore_save_fails {
                false
            } else {
                saved.set(prior);
                true
            }
        },
        || {
            // A failing registry write may still have changed the value.
            *os_cell.borrow_mut() = Some(format!("new-{want}"));
            if os_fails { Err("registry".to_string()) } else { Ok(()) }
        },
        |p| {
            *os_cell.borrow_mut() = p;
            Ok(())
        },
    );
    w.setting = saved.get();
    w.os = os_cell.into_inner();
    result
}

#[test]
fn two_state_change_success_changes_both_sides() {
    let mut w = TwoStateWorld { setting: false, os: None, save_fails: false, os_fails: false, restore_save_fails: false };
    run_two_state(&mut w, true).unwrap();
    assert!(w.setting);
    assert_eq!(w.os.as_deref(), Some("new-true"));
}

#[test]
fn two_state_change_keeps_both_old_when_the_save_fails() {
    let mut w = TwoStateWorld { setting: false, os: Some("old".into()), save_fails: true, os_fails: false, restore_save_fails: false };
    assert_eq!(run_two_state(&mut w, true).unwrap_err(), "save-failed");
    assert!(!w.setting);
    assert_eq!(w.os.as_deref(), Some("old"), "the registry is not touched when the setting did not save");
}

#[test]
fn two_state_change_keeps_both_old_when_the_registry_fails_after_the_save() {
    let mut w = TwoStateWorld { setting: false, os: Some("old".into()), save_fails: false, os_fails: true, restore_save_fails: false };
    assert_eq!(run_two_state(&mut w, true).unwrap_err(), "registry");
    assert!(!w.setting, "the saved setting is put back");
    assert_eq!(w.os.as_deref(), Some("old"), "a half-applied registry change is put back");
    let mut w = TwoStateWorld { setting: true, os: None, save_fails: false, os_fails: true, restore_save_fails: false };
    assert!(run_two_state(&mut w, false).is_err());
    assert!(w.setting);
    assert_eq!(w.os, None);
}

#[test]
fn registry_owner_tells_this_copy_from_another() {
    let here = r"C:\Apps\Prompt Saver\prompt-saver.exe";
    let exists = |p: &str| p.to_lowercase().contains("other");
    assert_eq!(registry_owner(None, here, exists), RegistryOwner::Missing);
    assert_eq!(registry_owner(Some(r#""C:\Apps\Prompt Saver\prompt-saver.exe" --minimized"#), here, exists), RegistryOwner::ThisExe);
    assert_eq!(registry_owner(Some(r#""c:/apps/prompt saver/PROMPT-SAVER.EXE" "%1""#), here, exists), RegistryOwner::ThisExe);
    assert_eq!(registry_owner(Some(r#""D:\other\prompt-saver.exe""#), here, exists), RegistryOwner::OtherExe);
    assert_eq!(registry_owner(Some(r#""E:\moved\prompt-saver.exe""#), here, exists), RegistryOwner::Gone);
    assert_eq!(registry_owner(Some(""), here, exists), RegistryOwner::Missing);
}

#[test]
fn boot_reconcile_only_writes_and_never_removes() {
    use RegistryOwner::*;
    for owner in [Missing, ThisExe, Gone] {
        assert!(boot_reconcile_writes(true, false, owner));
        assert!(!boot_reconcile_writes(false, false, owner), "a setting that reads as off must not delete an entry at boot");
        assert!(!boot_reconcile_writes(true, true, owner), "demo mode skips the pass");
    }
    assert!(!boot_reconcile_writes(true, false, OtherExe), "another copy's entry is left alone");
}

#[test]
fn switching_off_leaves_another_copys_entry_alone() {
    use RegistryOwner::*;
    assert!(skip_registry_removal(false, OtherExe));
    assert!(!skip_registry_removal(false, ThisExe));
    assert!(!skip_registry_removal(false, Gone));
    assert!(!skip_registry_removal(false, Missing));
    assert!(!skip_registry_removal(true, OtherExe), "switching on is never skipped");
}

pub(super) fn fake_snapshot(root: &std::path::Path, name: &str) {
    let f = root.join(name);
    std::fs::create_dir_all(&f).unwrap();
    std::fs::write(f.join(BACKUP_DB_GZ), b"x").unwrap();
}

#[test]
fn rotation_ignores_an_incomplete_folder_and_keeps_good_snapshots() {
    let dir = stream_test_dir("rotate-incomplete");
    fake_snapshot(&dir, "2026-01-01_10-00-00");
    fake_snapshot(&dir, "2026-01-02_10-00-00");
    // Newer than both, but still being packed: only the raw copy exists.
    let pending = dir.join("2026-01-03_10-00-00");
    std::fs::create_dir_all(&pending).unwrap();
    std::fs::write(pending.join("data.db.tmp"), b"raw").unwrap();
    rotate_backups_at(&dir, 2, None, None);
    assert!(dir.join("2026-01-01_10-00-00").exists(), "a good snapshot must not be dropped for an incomplete folder");
    assert!(dir.join("2026-01-02_10-00-00").exists());
    assert!(pending.exists(), "the folder being packed is not rotation's to delete");
    assert_eq!(list_backups_impl_at(&dir).len(), 2, "the list shows complete snapshots only");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_restore_safety_snapshot_never_rotates_away_the_chosen_snapshot() {
    let dir = stream_test_dir("rotate-protect");
    let root = dir.join("backups");
    fake_snapshot(&root, "2026-01-01_10-00-00");
    fake_snapshot(&root, "2026-01-02_10-00-00");
    fake_snapshot(&root, "2026-01-03_10-00-00");
    {
        let conn = Connection::open(dir.join("data.db")).unwrap();
        conn.execute_batch("CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);")
            .unwrap();
    }
    // The list is full: the safety snapshot pushes the oldest out, which is
    // exactly the one being restored.
    do_backup_protecting_at(&dir, &root, 2, None, "2026-01-01_10-00-00").unwrap();
    assert!(root.join("2026-01-01_10-00-00").exists(), "the snapshot being restored must survive the rotation");
    assert!(!root.join("2026-01-02_10-00-00").exists(), "other old snapshots still rotate out");
    // Without a protected name the oldest goes, as before.
    do_backup_at(&dir, &root, 2, None).unwrap();
    assert!(!root.join("2026-01-01_10-00-00").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn startup_cleanup_removes_leftovers_and_keeps_complete_snapshots() {
    let dir = stream_test_dir("cleanup");
    fake_snapshot(&dir, "2026-01-01_10-00-00");
    let good = dir.join("2026-01-01_10-00-00");
    std::fs::write(good.join("data.db.tmp"), b"raw").unwrap();
    std::fs::write(good.join("data.db.gz.part"), b"half").unwrap();
    for stray in ["count.tmp", "names.tmp", "purge.tmp"] {
        std::fs::write(good.join(stray), b"unpacked library").unwrap();
    }
    let broken = dir.join("2026-01-02_10-00-00");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("data.db.tmp"), b"raw").unwrap();
    // Too fresh to be sure it is abandoned: untouched.
    cleanup_incomplete_backups(&dir, Duration::from_secs(3600));
    assert!(broken.exists());
    assert!(good.join("data.db.tmp").exists());
    cleanup_incomplete_backups(&dir, Duration::ZERO);
    assert!(!broken.exists(), "a folder without a snapshot is removed");
    assert!(!good.join("data.db.tmp").exists());
    assert!(!good.join("data.db.gz.part").exists());
    for stray in ["count.tmp", "names.tmp", "purge.tmp"] {
        assert!(!good.join(stray).exists(), "{stray} is swept");
    }
    assert!(good.join(BACKUP_DB_GZ).exists(), "a complete snapshot is never touched");
    let _ = std::fs::remove_dir_all(&dir);
}

// allowMultipleInstances lets two full processes share one data directory.
// write_rows must not delete a row this process never learned about (added by
// another instance after this process's own load) just because that row is
// absent from this process's in-memory list.
#[test]
fn write_rows_preserves_a_row_unknown_to_this_process() {
    let mut conn = row_test_db();
    // "Process A" loads and writes its initial two prompts.
    let mut written_a = WrittenRows::new();
    let initial = numbered_prompts(2);
    db_write_prompts(&mut conn, &initial, &mut written_a).unwrap();

    // "Process B" independently adds a third prompt, never seen by A.
    let foreign = Prompt { name: "Foreign".into(), ..test_prompt("foreign") };
    let mut written_b = WrittenRows::new();
    let mut b_prompts = initial.clone();
    b_prompts.push(foreign.clone());
    db_write_prompts(&mut conn, &b_prompts, &mut written_b).unwrap();

    // A now saves again with its own (still two-item) list, unaware of B's row.
    db_write_prompts(&mut conn, &initial, &mut written_a).unwrap();

    let ids: Vec<String> = stored_rows(&conn, "prompts").into_iter().map(|(id, _, _)| id).collect();
    assert!(ids.contains(&"foreign".to_string()), "another instance's unseen row must survive: {ids:?}");
    assert_eq!(ids.len(), 3, "A's save must not touch B's row at all: {ids:?}");
}

// A genuinely fresh install (no settings row at all yet) must never be flagged
// as unreadable: only a row that IS there but cannot be read back correctly.
#[test]
fn db_load_does_not_flag_a_settings_row_that_was_never_written() {
    let conn = row_test_db();
    conn.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);").unwrap();
    let (_prompts, settings, _plain, _fails, settings_unreadable) = db_load(&conn, &mut WrittenRows::new());
    assert!(settings.is_none(), "no row means no settings to return");
    assert!(!settings_unreadable, "no row means first run, not corruption");
}

// A settings row that IS present but will not decrypt/parse must be flagged --
// this is the case that has to lock writes so a rewrite can never overwrite the
// real (corrupt but present) row with Settings::default().
#[test]
fn db_load_flags_a_settings_row_that_will_not_parse() {
    let conn = row_test_db();
    conn.execute_batch(
        "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO meta VALUES('settings', 'not valid json and not an enc: blob either');",
    )
    .unwrap();
    let (_prompts, settings, _plain, _fails, settings_unreadable) = db_load(&conn, &mut WrittenRows::new());
    assert!(settings.is_none(), "an unparsable row must not be handed back as real settings");
    assert!(settings_unreadable, "a present-but-corrupt row must be flagged");
}

// A properly sealed, well-formed row must never be flagged, whatever its content.
#[test]
fn db_load_does_not_flag_a_readable_settings_row() {
    let conn = row_test_db();
    conn.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);").unwrap();
    let s = Settings { theme: "ocean".into(), ..Default::default() };
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('settings', ?1)",
        params![enc_str(&serde_json::to_string(&s).unwrap())],
    )
    .unwrap();
    let (_prompts, settings, _plain, _fails, settings_unreadable) = db_load(&conn, &mut WrittenRows::new());
    assert_eq!(settings.map(|s| s.theme), Some("ocean".to_string()));
    assert!(!settings_unreadable, "a readable row must not be flagged");
}

// The command the window polls at startup must report exactly what the flag
// holds: this is the read side of the same lock write_settings_outcome checks
// before ever touching disk (untestable here directly: it needs a real
// AppHandle, which no unit test in this file constructs).
#[test]
fn settings_locked_command_reports_the_flag() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let before = SETTINGS_LOCKED.load(Ordering::Relaxed);
    SETTINGS_LOCKED.store(true, Ordering::Relaxed);
    assert!(settings_locked());
    SETTINGS_LOCKED.store(false, Ordering::Relaxed);
    assert!(!settings_locked());
    SETTINGS_LOCKED.store(before, Ordering::Relaxed);
}

// Same shape as settings_locked_command_reports_the_flag above: the actual
// flip lives in write_settings_outcome, which needs a real AppHandle no unit test
// here constructs, so this proves the read side: whatever the flag holds is
// exactly what the window's startup poll gets back, in both directions.
#[test]
fn settings_write_failed_command_reports_the_flag() {
    let before = SETTINGS_WRITE_FAILED.load(Ordering::Relaxed);
    SETTINGS_WRITE_FAILED.store(true, Ordering::Relaxed);
    assert!(settings_write_failed());
    SETTINGS_WRITE_FAILED.store(false, Ordering::Relaxed);
    assert!(!settings_write_failed());
    SETTINGS_WRITE_FAILED.store(before, Ordering::Relaxed);
}

#[test]
fn a_panic_log_line_never_quotes_the_data_the_panic_choked_on() {
    let secret = "my confidential prompt text";
    let msg = format!("byte index 3 is not a char boundary; it is inside 'é' (bytes 2..4) of `{secret}`");
    let line = panic_log_line(Some(("src/x.rs", 12, 5)), &msg);
    assert!(!line.contains("confidential") && !line.contains(secret), "{line}");
    assert!(line.contains("x.rs:12:5"), "the location must stay: {line}");
    let unwrap = format!("called `Result::unwrap()` on an `Err` value: \"{secret}\"");
    assert!(!panic_log_line(None, &unwrap).contains("confidential"));
    assert!(panic_log_line(None, &"a".repeat(500)).len() < 120);
}

// The text of the first fn called `name` (from its signature to the next top-level item), for
// source assertions about which thread a command runs its blocking work on.
fn fn_text<'a>(src: &'a str, name: &str) -> &'a str {
    let at = src.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("fn {name} not found"));
    let rest = &src[at..];
    let end = rest[3..].find("\n}\n").map(|i| i + 3).unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn heavy_backup_commands_run_off_the_async_runtime() {
    let src = include_str!("../backups.rs");
    for name in ["backup_contents", "restore_backup"] {
        assert!(fn_text(src, name).contains("spawn_blocking"), "{name} must not inflate or lock on an async worker");
    }
}

#[test]
fn forwarded_backup_restore_runs_off_the_async_runtime() {
    let src = include_str!("../cli_callers.rs");
    assert!(fn_text(src, "run_in_process_inner").contains("spawn_blocking"), "the restore must not block the pipe handler's worker");
}

#[test]
fn modal_file_dialogs_wait_on_a_blocking_thread() {
    let cases: [(&str, &str, &[&str]); 3] = [
        (include_str!("../import_export.rs"), "import_export", &["export_prompts", "preview_import", "preview_import_folder"]),
        (include_str!("../vault.rs"), "vault", &["export_all", "pick_backup_file", "export_settings_only", "pick_settings_import_file"]),
        (include_str!("../remote.rs"), "remote", &["remote_tls_save_cert"]),
    ];
    for (src, file, names) in cases {
        for name in names {
            assert!(fn_text(src, name).contains("spawn_blocking"), "{file}::{name} shows a modal dialog on an async worker");
        }
    }
}

#[test]
fn folder_setters_run_off_the_ui_thread() {
    let src = include_str!("../update.rs");
    for name in ["set_screenshot_dir", "set_video_export_dir", "set_recording_dir"] {
        let at = src.find(&format!("fn {name}(")).unwrap();
        let before = &src[..at];
        let attr = before.rsplit('\n').nth(1).unwrap_or("");
        assert_eq!(attr.trim(), "#[tauri::command(async)]", "{name} must run as an async command");
    }
}

#[test]
fn log_error_reads_its_flag_without_cloning_the_settings() {
    let src = include_str!("../logging.rs");
    assert!(!fn_text(src, "log_error").contains("settings.clone()"));
}

#[test]
fn the_webview2_staging_folder_must_be_new() {
    let src = include_str!("../webview2.rs");
    assert!(src.contains("fs::create_dir(&dir)"));
    assert!(!src.contains("create_dir_all(&dir)"));
}

#[test]
fn the_uninstaller_removes_the_downloaded_ffmpeg_before_the_folder() {
    let nsi = include_str!("../../installer.nsi");
    let rm = nsi.find("RMDir \"$INSTDIR\"\n").or_else(|| nsi.find("RMDir \"$INSTDIR\"\r\n")).expect("RMDir of the install folder");
    for exe in ["ffmpeg.exe", "ffprobe.exe"] {
        let del = nsi.find(&format!("Delete \"$INSTDIR\\{exe}\"")).unwrap_or_else(|| panic!("{exe} is not deleted"));
        assert!(del < rm, "{exe} must go before the non-recursive RMDir");
    }
}

#[test]
fn every_crate_version_is_pinned_exactly() {
    let toml = include_str!("../../Cargo.toml");
    let mut in_deps = false;
    let mut loose = Vec::new();
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t.contains("dependencies");
            continue;
        }
        if !in_deps || t.starts_with('#') {
            continue;
        }
        // `name = "1.2"` or `name = { version = "1.2", ... }`
        let spec = if let Some(i) = t.find("version = \"") { &t[i + 11..] } else if let Some((_, v)) = t.split_once(" = \"") { v } else { continue };
        if !spec.starts_with('=') {
            loose.push(t.to_string());
        }
    }
    assert!(loose.is_empty(), "crates must be pinned with =x.y.z: {loose:?}");
}

#[test]
fn both_launch_routes_find_the_link_through_one_helper() {
    let lib = include_str!("../lib.rs");
    assert!(!lib.contains("starts_with(\"promptsaver://\")"), "the scheme match must live in remote::find_scheme_arg only");
    assert_eq!(lib.matches("remote::find_scheme_arg(").count(), 2);
}

#[test]
fn the_update_switches_report_a_failed_save() {
    let src = include_str!("../update.rs");
    for name in ["set_auto_update", "set_update_consent"] {
        let body = fn_text(src, name);
        assert!(body.contains("-> Result<(), String>"), "{name} must return a Result");
        assert!(body.contains("if !save_settings("), "{name} must check the save");
        assert!(body.contains("\"save-failed\""), "{name} must report save-failed");
    }
}
