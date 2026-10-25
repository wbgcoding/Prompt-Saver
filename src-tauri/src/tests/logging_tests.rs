use super::*;

#[test]
fn a_panic_log_line_keeps_only_the_location_and_the_message_length() {
    let msg = "no such prompt for hunter2 and friends";
    let line = panic_log_line(Some(("src/x.rs", 12, 5)), msg);
    assert!(!line.contains("hunter2") && !line.contains("no such"), "unquoted data stays out too: {line}");
    assert!(line.contains("x.rs:12:5") && line.contains(&msg.chars().count().to_string()), "{line}");
    let none = panic_log_line(None, msg);
    assert!(!none.contains("hunter2") && none.contains(&msg.chars().count().to_string()), "{none}");
}

#[test]
fn a_stale_crash_log_lock_is_reclaimed_at_once() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-stale-{}", std::process::id()));
    let lock_path = path.with_extension("log.lock");
    fs::write(&lock_path, std::process::id().to_string()).unwrap();
    let old = SystemTime::now() - Duration::from_secs(60);
    fs::OpenOptions::new().write(true).open(&lock_path).unwrap().set_modified(old).unwrap();
    let began = std::time::Instant::now();
    let lock = CrashLogLock::acquire(&path);
    assert!(lock.held, "an old marker must not block the log");
    // Well under the 2 s wait for a live owner, with room for a loaded parallel test run.
    assert!(began.elapsed() < Duration::from_millis(1000), "took {:?}", began.elapsed());
    drop(lock);
    assert!(!lock_path.exists());
}

#[test]
fn a_marker_whose_owner_is_gone_is_reclaimed_before_it_is_old() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-dead-{}", std::process::id()));
    let lock_path = path.with_extension("log.lock");
    // A process id far above anything the system hands out: no such process.
    let dead_pid: u32 = 0x7FFF_FFF0;
    fs::write(&lock_path, dead_pid.to_string()).unwrap();
    let began = std::time::Instant::now();
    let lock = CrashLogLock::acquire(&path);
    assert!(lock.held);
    assert!(began.elapsed() < Duration::from_millis(500), "took {:?}", began.elapsed());
    drop(lock);
}

#[test]
fn a_fresh_marker_of_a_live_process_is_waited_for_the_full_deadline() {
    let path = std::env::temp_dir().join(format!("ps-crashlog-live-{}", std::process::id()));
    let lock_path = path.with_extension("log.lock");
    fs::write(&lock_path, std::process::id().to_string()).unwrap();
    let began = std::time::Instant::now();
    let lock = CrashLogLock::acquire(&path);
    assert!(!lock.held, "a live owner's marker is respected");
    assert!(began.elapsed() >= Duration::from_millis(1900), "gave up after {:?}", began.elapsed());
    drop(lock);
    assert!(lock_path.exists(), "someone else's marker is not removed");
    let _ = fs::remove_file(&lock_path);
}
