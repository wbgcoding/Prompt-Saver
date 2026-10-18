use super::*;

// ---- the window-close decision never waits for the store ----

#[test]
fn close_decision_returns_while_the_store_is_locked() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let store = Arc::new(Mutex::new(Store {
        prompts: Vec::new(),
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    }));
    let held = store.lock().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut s = Settings { minimize_to_tray: false, ..Settings::default() };
        mirror_minimize_to_tray(&s);
        let off = close_minimizes(false);
        s.minimize_to_tray = true;
        mirror_minimize_to_tray(&s);
        let on = close_minimizes(false);
        let portable = close_minimizes(true);
        let _ = tx.send((off, on, portable));
    });
    let got = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the close decision must not wait for the store lock");
    drop(held);
    assert_eq!(got, (false, true, false));
}

#[test]
fn hot_path_commands_run_off_the_main_thread() {
    // Contrast: a sync command runs on the event-loop thread and freezes the
    // window while a backup holds the store. No red test exists for this (it
    // needs a live event loop), so the attribute itself is asserted.
    for (src, name) in [
        (include_str!("../logging.rs"), "log_error"),
        (include_str!("../settings_cmds.rs"), "get_settings"),
        (include_str!("../autostart.rs"), "set_always_on_top"),
        (include_str!("../theme.rs"), "current_theme"),
        (include_str!("../update.rs"), "set_ui_value"),
    ] {
        let at = src.find(&format!("fn {name}(")).expect(name);
        let head = &src[..at];
        let attr = head.rfind("#[tauri::command").expect(name);
        assert!(
            head[attr..].starts_with("#[tauri::command(async)]"),
            "{name} must be an async command"
        );
    }
}

#[test]
fn float_recall_queries_monitors_before_taking_the_store_lock() {
    // Structural fix (call order), so no red test exists: monitor queries wait
    // for the main thread, which may itself be waiting on the store lock. The
    // old body queried them after the lock; this pins the order.
    let src = include_str!("../floating.rs");
    let at = src.find("fn float_recall(").expect("float_recall");
    let end = src[at..].find("\n}").expect("end") + at;
    let body = &src[at..end];
    let lock_at = body.find("lock(&state)").expect("takes the store lock");
    // The scale factor comes from main_scale_factor, which does the monitor query.
    assert!(src.contains("fn main_scale_factor(") && src.contains("available_monitors("));
    for q in ["screen_snapshot(", "main_scale_factor("] {
        let q_at = body.find(q).unwrap_or_else(|| panic!("{q} missing"));
        assert!(q_at < lock_at, "{q} must run before the store lock is taken");
    }
}

// ---- the taskbar icon is only touched when its state changes ----

#[test]
fn taskbar_throttle_applies_a_repeated_percent_once() {
    let mut t = bg_ops::TaskbarThrottle::default();
    assert!(t.should_apply(7, "running", Some(40)), "the first tick is applied");
    assert!(!t.should_apply(7, "running", Some(40)), "the same percent again is skipped");
    assert!(t.should_apply(7, "running", Some(41)), "a new percent is applied");
    assert!(t.should_apply(7, "indeterminate", Some(41)), "a new state is applied");
    assert!(!t.should_apply(7, "indeterminate", Some(90)), "a state without a percent ignores it");
    assert!(t.should_apply(7, "off", None));
    assert!(!t.should_apply(7, "off", None), "off twice is one apply");
    assert!(t.should_apply(8, "off", None), "a rebuilt window handle is applied again");
    t.reset();
    assert!(t.should_apply(8, "off", None), "a reset retries the same pair");
}

// ---- the update check waits for an answer ----

#[test]
fn a_fresh_profile_never_checks_for_updates() {
    let s = Settings::default();
    assert_eq!(s.update_consent, UpdateConsent::Unasked);
    assert!(!update::update_check_allowed(&s));
    let mut yes = Settings { update_consent: UpdateConsent::Yes, ..Settings::default() };
    assert!(update::update_check_allowed(&yes));
    yes.auto_update = false;
    assert!(!update::update_check_allowed(&yes), "the switch still wins");
}

#[test]
fn stored_settings_without_the_consent_key_keep_their_update_choice() {
    let mut on: Settings = serde_json::from_str(r#"{"auto_update":true}"#).unwrap();
    assert_eq!(on.update_consent, UpdateConsent::Legacy);
    on.migrate();
    assert_eq!(on.update_consent, UpdateConsent::Yes);
    let mut off: Settings = serde_json::from_str(r#"{"auto_update":false}"#).unwrap();
    off.migrate();
    assert_eq!(off.update_consent, UpdateConsent::No);
    let mut fresh = Settings::default();
    fresh.migrate();
    assert_eq!(fresh.update_consent, UpdateConsent::Unasked, "migrate leaves an unanswered profile alone");
    let again: Settings = serde_json::from_str(&serde_json::to_string(&fresh).unwrap()).unwrap();
    assert_eq!(again.update_consent, UpdateConsent::Unasked, "the unanswered state survives a save");
}

#[test]
fn a_restore_keeps_the_local_update_consent_and_a_foreign_block_asks_again() {
    let local = Settings { update_consent: UpdateConsent::Yes, ..Settings::default() };
    let mut incoming = Settings { update_consent: UpdateConsent::No, ..Settings::default() };
    keep_machine_settings(&local, &mut incoming);
    assert_eq!(incoming.update_consent, UpdateConsent::Yes);
    let mut other = Settings { update_consent: UpdateConsent::Yes, ..Settings::default() };
    drop_machine_rows(&mut other);
    assert_eq!(other.update_consent, UpdateConsent::Unasked);
}

// ---- user-picked export files are replaced atomically ----

#[test]
fn write_bytes_atomic_replaces_the_target_and_leaves_no_temp_file() {
    let dir = std::env::temp_dir().join(format!("ps-atomic-ok-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("export.psb");
    write_bytes_atomic(&target, b"first").unwrap();
    write_bytes_atomic(&target, b"second, longer").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"second, longer");
    let names: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["export.psb".to_string()], "no temp file is left beside the target");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_atomic_write_leaves_the_old_file_untouched() {
    let dir = std::env::temp_dir().join(format!("ps-atomic-fail-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("export.psb");
    fs::write(&target, b"the old backup").unwrap();
    // A read-only target cannot be replaced, so the rename step fails after the
    // new bytes were already written to the temp file.
    let mut perms = fs::metadata(&target).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&target, perms).unwrap();
    let result = write_bytes_atomic(&target, b"a much longer replacement that must not appear");
    let mut perms = fs::metadata(&target).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(&target, perms).unwrap();
    assert!(result.is_err(), "replacing a read-only file must fail");
    assert_eq!(fs::read(&target).unwrap(), b"the old backup", "the old file is byte-identical");
    let names: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["export.psb".to_string()], "the failed attempt removed its temp file");
    let _ = fs::remove_dir_all(&dir);
}

// A file another program holds open without delete sharing cannot be replaced, but an in-place
// write would still go through and could leave a half-written backup behind.
#[cfg(windows)]
#[test]
fn the_strict_atomic_write_leaves_a_file_held_open_without_delete_sharing_untouched() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = std::env::temp_dir().join(format!("ps-atomic-held-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("backup.psb");
    fs::write(&target, b"the previous backup").unwrap();
    // Share read and write, but not delete: the swap is refused, an in-place write is not.
    let held = fs::OpenOptions::new().read(true).share_mode(1 | 2).open(&target).unwrap();
    let result = write_bytes_atomic_strict(&target, b"a replacement that must not appear");
    assert!(result.is_err(), "a refused replace must surface as an error");
    assert_eq!(fs::read(&target).unwrap(), b"the previous backup", "the old file stays byte-identical");
    drop(held);
    let names: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["backup.psb".to_string()], "the failed attempt removed its temp file");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn backup_export_writers_use_the_strict_replace() {
    let src = include_str!("../vault.rs");
    assert!(!src.contains("write_bytes_atomic(&path"), "a backup export must not fall back to an in-place write");
    assert_eq!(src.matches("write_bytes_atomic_strict(&path").count(), 2);
}

#[test]
fn user_picked_exports_never_write_in_place() {
    for (name, src) in [
        ("import_export.rs", include_str!("../import_export.rs")),
        ("vault.rs", include_str!("../vault.rs")),
        ("stats.rs", include_str!("../stats.rs")),
    ] {
        assert!(!src.contains("fs::write(&path"), "{name} writes a user-picked file in place instead of through write_bytes_atomic");
    }
}

// ---- the data-folder lock is claimed in one step ----

pub(super) fn lock_test_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ps-lock-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub(super) fn lock_info(host: &str, id: &str, beat: u64, seen: u64) -> DataDirLockInfo {
    DataDirLockInfo { host: host.into(), pid: 1, seen, id: id.into(), beat }
}

#[test]
fn two_starters_on_one_folder_never_both_own_it() {
    use std::sync::{Arc, Barrier};
    for round in 0..20 {
        let dir = lock_test_dir(&format!("race{round}"));
        let path = data_dir_lock_path(&dir);
        let gate = Arc::new(Barrier::new(2));
        let spawn = |host: &'static str, id: &'static str| {
            let (gate, path) = (gate.clone(), path.clone());
            std::thread::spawn(move || {
                let mut obs = LockObserver::new(Instant::now());
                gate.wait();
                claim_data_dir_lock(&path, &LockMe { host, pid: 1, id }, &mut obs, Instant::now(), Duration::from_millis(50))
            })
        };
        let (a, b) = (spawn("A", "ida"), spawn("B", "idb"));
        let (a, b) = (a.join().unwrap(), b.join().unwrap());
        let owners = [&a, &b].iter().filter(|o| matches!(o, LockOutcome::Owned)).count();
        assert_eq!(owners, 1, "exactly one starter owns the folder (round {round})");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn two_takeovers_of_one_dead_lock_leave_one_owner() {
    use std::sync::{Arc, Barrier};
    for round in 0..10 {
        let dir = lock_test_dir(&format!("takeover{round}"));
        let path = data_dir_lock_path(&dir);
        write_data_dir_lock(&path, &lock_info("OLD", "old", 7, 1));
        let t0 = Instant::now();
        let gate = Arc::new(Barrier::new(2));
        let spawn = |host: &'static str, id: &'static str| {
            let (gate, path) = (gate.clone(), path.clone());
            std::thread::spawn(move || {
                let mut obs = LockObserver::new(t0);
                let me = LockMe { host, pid: 1, id };
                // Watch the lock stand still for the whole limit, then race for it.
                assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, t0, Duration::ZERO), LockOutcome::ReadOnly(_)));
                gate.wait();
                let later = t0 + Duration::from_secs(DATA_DIR_LOCK_STALE_SECS + 1);
                claim_data_dir_lock(&path, &me, &mut obs, later, Duration::from_millis(150))
            })
        };
        let (a, b) = (spawn("A", "ida"), spawn("B", "idb"));
        let (a, b) = (a.join().unwrap(), b.join().unwrap());
        let owners = [&a, &b].iter().filter(|o| matches!(o, LockOutcome::Owned)).count();
        assert_eq!(owners, 1, "the read-back after the takeover settles it (round {round})");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_live_lock_with_a_changing_heartbeat_is_never_taken_over() {
    let dir = lock_test_dir("heartbeat");
    let path = data_dir_lock_path(&dir);
    let me = LockMe { host: "A", pid: 2, id: "ida" };
    let t0 = Instant::now();
    let mut obs = LockObserver::new(t0);
    // The owner's clock is 2 minutes ahead, then years behind; its beat moves on.
    for i in 0..12u64 {
        let seen = if i % 2 == 0 { now_secs() + 7200 } else { 5 };
        write_data_dir_lock(&path, &lock_info("B", "idb", i, seen));
        let now = t0 + Duration::from_secs(i * 40);
        assert!(
            matches!(claim_data_dir_lock(&path, &me, &mut obs, now, Duration::ZERO), LockOutcome::ReadOnly(h) if h == "B"),
            "round {i}: a lock that keeps changing is alive, whatever its timestamp says"
        );
    }
    assert_eq!(read_data_dir_lock(&path).unwrap().id, "idb", "the live owner's file was never rewritten");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_lock_that_stands_still_for_the_whole_limit_is_taken_over() {
    let dir = lock_test_dir("standstill");
    let path = data_dir_lock_path(&dir);
    let me = LockMe { host: "A", pid: 2, id: "ida" };
    let t0 = Instant::now();
    let mut obs = LockObserver::new(t0);
    write_data_dir_lock(&path, &lock_info("B", "idb", 4, now_secs()));
    let settle = Duration::ZERO;
    assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, t0, settle), LockOutcome::ReadOnly(_)));
    let almost = t0 + Duration::from_secs(DATA_DIR_LOCK_STALE_SECS - 1);
    assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, almost, settle), LockOutcome::ReadOnly(_)));
    let enough = t0 + Duration::from_secs(DATA_DIR_LOCK_STALE_SECS);
    assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, enough, settle), LockOutcome::Owned));
    assert_eq!(read_data_dir_lock(&path).unwrap().id, "ida");
    // A lock naming this machine is taken at once; an unparsable one waits like a foreign one.
    write_data_dir_lock(&path, &lock_info("A", "older-run", 1, 1));
    let mut obs = LockObserver::new(t0);
    assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, t0, settle), LockOutcome::Owned));
    fs::write(&path, b"{ not json").unwrap();
    let mut obs = LockObserver::new(t0);
    assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, t0, settle), LockOutcome::ReadOnly(_)));
    assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, enough, settle), LockOutcome::Owned));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_create_error_other_than_already_exists_is_not_ownership() {
    let dir = lock_test_dir("createerr");
    // The folder is gone, so create_new fails with NotFound: nothing is known about
    // the lock, and that must never read as "free".
    let path = dir.join("missing").join(DATA_DIR_LOCK_FILE);
    let me = LockMe { host: "A", pid: 2, id: "ida" };
    let mut obs = LockObserver::new(Instant::now());
    let r = claim_data_dir_lock(&path, &me, &mut obs, Instant::now(), Duration::ZERO);
    assert!(matches!(r, LockOutcome::ReadOnly(_)), "an unreadable folder is busy, not owned");
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_owner_refresh_never_overwrites_another_claim() {
    let dir = lock_test_dir("refresh-foreign");
    let path = data_dir_lock_path(&dir);
    let me = LockMe { host: "A", pid: 2, id: "ida" };
    write_data_dir_lock(&path, &lock_info("A", "ida", 1, 1));
    assert!(matches!(refresh_owned_lock(&path, &me, 2), RefreshOutcome::Kept));
    assert_eq!(read_data_dir_lock(&path).unwrap().beat, 2);
    write_data_dir_lock(&path, &lock_info("B", "idb", 9, 9));
    assert!(matches!(refresh_owned_lock(&path, &me, 3), RefreshOutcome::Lost(h) if h == "B"));
    let kept = read_data_dir_lock(&path).unwrap();
    assert_eq!((kept.id.as_str(), kept.beat), ("idb", 9), "the other claim is untouched");
    fs::write(&path, b"{ half").unwrap();
    assert!(matches!(refresh_owned_lock(&path, &me, 4), RefreshOutcome::Skipped));
    assert_eq!(fs::read(&path).unwrap(), b"{ half");
    fs::remove_file(&path).unwrap();
    assert!(matches!(refresh_owned_lock(&path, &me, 5), RefreshOutcome::Kept), "a vanished lock is put back");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_refresh_tick_drops_ownership_when_someone_else_took_the_lock() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("tick-drop");
    acquire_data_dir_lock(&dir, true);
    assert!(DATA_DIR_LOCK_OWNED.load(Ordering::Relaxed) && !DATA_DIR_READONLY.load(Ordering::Relaxed));
    let path = data_dir_lock_path(&dir);
    write_data_dir_lock(&path, &lock_info("OTHER-PC", "someone-else", 1, now_secs()));
    refresh_data_dir_lock(&dir);
    assert!(DATA_DIR_READONLY.load(Ordering::Relaxed), "the folder is no longer ours");
    assert!(!DATA_DIR_LOCK_OWNED.load(Ordering::Relaxed));
    assert_eq!(DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).as_deref(), Some("OTHER-PC"));
    assert_eq!(read_data_dir_lock(&path).unwrap().id, "someone-else", "their lock was not overwritten");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn refreshing_the_lock_never_shows_a_reader_a_broken_file() {
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
    use std::sync::Arc;
    let dir = std::env::temp_dir().join(format!("ps-lockrefresh-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = data_dir_lock_path(&dir);
    assert!(write_data_dir_lock(&path, &DataDirLockInfo { host: "A".into(), pid: 1, seen: now_secs(), id: String::new(), beat: 0 }));
    let done = Arc::new(AtomicBool::new(false));
    let reader = {
        let (path, done) = (path.clone(), done.clone());
        std::thread::spawn(move || {
            // `broken` = a file that opened but did not parse (a half-written lock).
            // `unopened` = the open itself was refused for a moment while the swap
            // ran; the caller treats that as "unknown right now", never as a lock.
            let (mut reads, mut broken, mut unopened) = (0u32, 0u32, 0u32);
            while !done.load(AtomicOrdering::Relaxed) {
                reads += 1;
                match fs::read_to_string(&path) {
                    Ok(text) => {
                        if serde_json::from_str::<DataDirLockInfo>(&text).is_err() {
                            broken += 1;
                        }
                    }
                    Err(_) => unopened += 1,
                }
            }
            (reads, broken, unopened)
        })
    };
    // A swap the system refuses while the reader has the file open is allowed to
    // fail (the old lock stays, the next tick retries); it must never fall back to
    // a write in place.
    let mut landed = 0u32;
    for n in 0..1000u64 {
        if write_data_dir_lock(&path, &DataDirLockInfo { host: "A".into(), pid: 1, seen: now_secs() + n, id: String::new(), beat: 0 }) {
            landed += 1;
        }
    }
    assert!(landed > 0, "no refresh landed at all");
    done.store(true, AtomicOrdering::Relaxed);
    let (reads, broken, unopened) = reader.join().unwrap();
    assert!(reads > 0);
    assert_eq!(broken, 0, "{broken} of {reads} reads found a half-written lock ({unopened} could not open it)");
    let _ = fs::remove_dir_all(&dir);
}

// A refused swap must leave the old lock untouched: the lock is read by other
// processes while it is rewritten, so it is never written in place.
#[cfg(windows)]
#[test]
fn a_refused_lock_swap_keeps_the_old_lock_instead_of_writing_in_place() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = std::env::temp_dir().join(format!("ps-lockstrict-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = data_dir_lock_path(&dir);
    let old = DataDirLockInfo { host: "A".into(), pid: 1, seen: 100, id: String::new(), beat: 0 };
    assert!(write_data_dir_lock(&path, &old));
    // Open for reading and writing but without delete sharing: the rename over it is refused.
    let held = fs::OpenOptions::new().read(true).share_mode(3).open(&path).unwrap();
    let newer = DataDirLockInfo { seen: 200, ..old.clone() };
    assert!(!write_data_dir_lock(&path, &newer), "a refused swap must report failure");
    assert_eq!(read_data_dir_lock(&path).map(|l| l.seen), Some(100), "the old lock must stay as it was");
    drop(held);
    assert!(write_data_dir_lock(&path, &newer), "the next round writes it");
    assert_eq!(read_data_dir_lock(&path).map(|l| l.seen), Some(200));
    let _ = fs::remove_dir_all(&dir);
}

// ---- numeric settings are held to their slider ranges ----

pub(super) fn clamped_values(entries: &[(&str, f64)]) -> HashMap<String, f64> {
    let mut s = Settings::default();
    for (k, v) in entries {
        s.ui_values.insert((*k).to_string(), *v);
    }
    s.migrate();
    s.ui_values
}

#[test]
fn a_stored_number_outside_its_slider_range_is_clamped_on_load() {
    let v = clamped_values(&[
        ("importFolderMax", 1e18),
        ("remoteRateLimit", 0.0),
        ("remotePort", -5.0),
        ("uiScale", 5000.0),
        ("historyMax", 5e9),
    ]);
    assert_eq!(v["importFolderMax"], 2000.0);
    assert_eq!(v["remoteRateLimit"], 0.0, "the unlimited sentinel stays");
    assert_eq!(v["remotePort"], 1024.0);
    assert_eq!(v["uiScale"], 300.0);
    assert_eq!(v["historyMax"], 1_000_000.0, "a stops entry is held to its last stop");
    let v = clamped_values(&[("remoteRateLimit", 1e9), ("hoverLift", 7.0)]);
    assert_eq!(v["remoteRateLimit"], 300.0);
    assert_eq!(v["hoverLift"], 7.0, "an in-range value is untouched");
}

#[test]
fn a_value_that_is_not_a_finite_number_is_dropped() {
    let v = clamped_values(&[("hoverLift", f64::NAN), ("tileRadius", f64::INFINITY), ("uiScale", f64::NEG_INFINITY)]);
    for k in ["hoverLift", "tileRadius", "uiScale"] {
        assert!(!v.contains_key(k), "{k} falls back to its default");
    }
}

#[test]
fn a_settings_import_clamps_the_numbers_it_brings() {
    let mut live = Settings::default();
    let file = SettingsOnlyExport {
        ui_flags: HashMap::new(),
        ui_values: HashMap::from([
            ("importFolderMax".to_string(), 1e18),
            ("remoteRateLimit".to_string(), 0.0),
            ("hoverLift".to_string(), f64::NAN),
        ]),
        ui_texts: HashMap::new(),
    };
    merge_settings_only(&mut live, file, |_| true).unwrap();
    assert_eq!(live.ui_values["importFolderMax"], 2000.0);
    assert_eq!(live.ui_values["remoteRateLimit"], 0.0);
    assert!(!live.ui_values.contains_key("hoverLift"));
}

#[test]
fn the_import_folder_scan_survives_an_unbounded_limit() {
    let dir = std::env::temp_dir().join(format!("ps-scanmax-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.md"), "x").unwrap();
    let (entries, truncated) = scan_import_folder(&dir, 3, usize::MAX);
    assert_eq!(entries.len(), 1);
    assert!(!truncated);
    let _ = fs::remove_dir_all(&dir);
}

// ---- a plain snapshot has the same size ceiling as an inflated one ----

#[test]
fn an_oversized_plain_snapshot_is_refused_before_any_copy() {
    let dir = std::env::temp_dir().join(format!("ps-bigplain-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let fake = dir.join("data.db");
    let f = fs::File::create(&fake).unwrap();
    f.set_len(MAX_INFLATED + 1).unwrap();
    drop(f);
    let err = stage_and_validate_backup(&fake, &dir).unwrap_err();
    assert_eq!(err, "backup expands to more than 512 MB");
    let leftovers: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(STAGED_RESTORE_PREFIX))
        .collect();
    assert!(leftovers.is_empty(), "no staged file remains: {leftovers:?}");
    let _ = fs::remove_dir_all(&dir);
}

// ---- the clipboard inbox skips huge texts ----

#[test]
fn the_clipboard_inbox_takes_up_to_the_character_limit_and_no_more() {
    use clipwatch::{should_store, CLIP_INBOX_MAX_CHARS};
    assert_eq!(CLIP_INBOX_MAX_CHARS, 200_000);
    // Spaces between letters keep the password heuristic out of this test.
    let exact = format!("{}aa", "a ".repeat(CLIP_INBOX_MAX_CHARS / 2 - 1));
    assert_eq!(exact.trim().chars().count(), CLIP_INBOX_MAX_CHARS);
    assert!(should_store(&exact, 3, false), "200 000 characters are kept");
    let over = format!("{exact}b");
    assert_eq!(over.trim().chars().count(), CLIP_INBOX_MAX_CHARS + 1);
    assert!(!should_store(&over, 3, false), "200 001 characters are skipped");
    // Multi-byte characters count once: 200 000 of them are 600 000 bytes, still in.
    assert!(should_store(&"€".repeat(CLIP_INBOX_MAX_CHARS), 3, false));
    assert!(!should_store(&"€".repeat(CLIP_INBOX_MAX_CHARS + 1), 3, false));
    // The byte shortcut: far past four bytes a character is out without counting.
    assert!(!should_store(&"a".repeat(CLIP_INBOX_MAX_CHARS * 4 + 1), 3, false));
    assert!(!should_store("ab", 3, false), "the minimum length still applies");
    assert!(should_store("abc", 3, false));
}

#[test]
fn an_import_refreshes_the_close_handlers_tray_setting() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    for (before, after) in [(true, false), (false, true)] {
        let mut store = Store {
            prompts: vec![sync_test_prompt()],
            trash: Vec::new(),
            settings: Settings { minimize_to_tray: before, ..Settings::default() },
            prompts_locked: false,
            written: Default::default(),
        };
        mirror_minimize_to_tray(&store.settings);
        let mut incoming = import_test_backup();
        incoming.settings.minimize_to_tray = after;
        // A failed write rolls everything back, the mirror included.
        run_import(&mut store, import_test_backup_with(after), false, true, |_, _, _| false).unwrap_err();
        assert_eq!(close_minimizes(false), before, "a rolled back import leaves the mirror alone");
        run_import(&mut store, incoming, false, true, |_, _, _| true).unwrap();
        assert_eq!(close_minimizes(false), after, "the imported setting must reach the close handler");
        assert_eq!(store.settings.minimize_to_tray, after);
    }
    mirror_minimize_to_tray(&Settings::default());
}

pub(super) fn import_test_backup_with(minimize: bool) -> Backup {
    let mut b = import_test_backup();
    b.settings.minimize_to_tray = minimize;
    b
}

#[test]
fn taskbar_throttle_retries_after_a_failed_apply_or_a_recreated_button() {
    let retry = AtomicBool::new(false);
    let mut t = bg_ops::TaskbarThrottle::default();
    assert!(t.decide(&retry, 5, "indeterminate", None));
    assert!(!t.decide(&retry, 5, "indeterminate", None), "unchanged state is skipped");
    // A failed apply (or the button coming back) asks for a retry: the very same
    // state goes out once more, then is skipped again.
    retry.store(true, Ordering::SeqCst);
    assert!(t.decide(&retry, 5, "indeterminate", None));
    assert!(!t.decide(&retry, 5, "indeterminate", None));
    assert!(!retry.load(Ordering::SeqCst), "the request is consumed");
}

// ---- retention selects are snapped on load and import ----

#[test]
fn an_imported_retention_that_would_purge_everything_is_snapped() {
    let import = |key: &str, v: f64| {
        let mut live = Settings::default();
        let file = SettingsOnlyExport {
            ui_flags: HashMap::new(),
            ui_values: HashMap::from([(key.to_string(), v)]),
            ui_texts: HashMap::new(),
        };
        merge_settings_only(&mut live, file, |_| true).unwrap();
        live
    };
    // Half a day used to become 0 seconds of retention: the next start purged the whole trash.
    let s = import("trashRetentionDays", 0.5);
    assert_eq!(trash_retention_secs(&s), None, "a fraction below one day means keep, not purge");
    assert_eq!(history_max_age(&import("historyDays", 0.5)), None);
    // A number that overflowed the day-to-seconds product wrapped to a tiny value.
    let s = import("trashRetentionDays", 2.14e14);
    assert_eq!(s.ui_values["trashRetentionDays"], 3650.0);
    assert_eq!(trash_retention_secs(&s), Some(3650 * 86_400));
    assert_eq!(import("trashRetentionDays", -3.0).ui_values["trashRetentionDays"], 0.0);
    // The free-entry field's whole numbers survive; a fraction is rounded.
    assert_eq!(import("trashRetentionDays", 14.0).ui_values["trashRetentionDays"], 14.0);
    assert_eq!(import("trashRetentionDays", 13.6).ui_values["trashRetentionDays"], 14.0);
    // A select with no zero preset never gets one.
    assert_eq!(import("backupIntervalH", 0.0).ui_values["backupIntervalH"], 1.0);
    // The same on load.
    let mut loaded = Settings::default();
    loaded.ui_values.insert("recordingRetentionDays".into(), 0.25);
    loaded.migrate();
    assert_eq!(loaded.ui_values["recordingRetentionDays"], 0.0);
}

#[test]
fn retention_arithmetic_cannot_overflow() {
    let mut s = Settings::default();
    s.ui_values.insert("trashRetentionDays".into(), 1e300);
    s.ui_values.insert("historyDays".into(), 1e300);
    assert_eq!(trash_retention_secs(&s), Some(u64::MAX));
    assert_eq!(history_max_age(&s), Some(u64::MAX));
}

// ---- a non-finite number never reaches the stored settings ----

#[test]
fn the_ui_value_setter_refuses_non_finite_numbers_and_clamps_the_rest() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(update::checked_ui_value("hoverLift", bad).is_err());
        assert!(update::checked_ui_value("someKeyWithoutARange", bad).is_err());
    }
    assert_eq!(update::checked_ui_value("importFolderMax", 1e18), Ok(2000.0));
    assert_eq!(update::checked_ui_value("trashRetentionDays", 0.5), Ok(0.0));
    assert_eq!(update::checked_ui_value("updateCheckH", 1e9), Ok(168.0));
    assert_eq!(update::checked_ui_value("hoverLift", 7.0), Ok(7.0));
    assert_eq!(update::checked_ui_value("someKeyWithoutARange", 12345.5), Ok(12345.5));
}

// ---- leftovers of an interrupted atomic write are swept ----

#[test]
fn only_our_own_temp_names_are_recognised() {
    assert!(is_atomic_temp_name("settings.json.tmp-p1760000000000000000_12"));
    assert!(is_atomic_temp_name("a.md.tmp-p1_0"));
    for other in ["notes.tmp", "x.tmp-abc", "x.tmp-p12", "x.tmp-p_3", "x.tmp-p1_", ".tmp-p1_2", "settings.json", "a.tmp-p1_2.bak", "a.tmp-q1_2"] {
        assert!(!is_atomic_temp_name(other), "{other} is not ours");
    }
}

#[test]
fn the_startup_sweep_removes_only_old_temp_leftovers() {
    let dir = std::env::temp_dir().join(format!("ps-tmpsweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let old_time = SystemTime::now() - Duration::from_secs(7200);
    let make = |name: &str, old: bool| {
        let f = fs::File::create(dir.join(name)).unwrap();
        if old {
            f.set_modified(old_time).unwrap();
        }
    };
    make("settings.json.tmp-p1760000000000000000_1", true);
    make("a.md.tmp-p1760000000000000000_2", true);
    make("fresh.json.tmp-p1760000000000000000_3", false);
    make("notes.tmp", true);
    make("export.psb", true);
    make("mine.tmp-abc", true);
    fs::create_dir(dir.join("sub.tmp-p1_1")).unwrap();
    assert_eq!(sweep_atomic_temps(&dir, Duration::from_secs(3600)), 2);
    let mut left: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["export.psb", "fresh.json.tmp-p1760000000000000000_3", "mine.tmp-abc", "notes.tmp", "sub.tmp-p1_1"]);
    let _ = fs::remove_dir_all(&dir);
}

// ---- the atomic replace keeps working where a replace is not allowed ----

#[test]
fn the_atomic_write_falls_back_to_writing_in_place_when_the_replace_is_refused() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = std::env::temp_dir().join(format!("ps-atomic-fallback-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("export.json");
    fs::write(&target, b"old content that is longer").unwrap();
    // Another program holds the file open and shares reading and writing but not
    // deleting or renaming, so a rename over it is refused while a write is not.
    let _holder = fs::OpenOptions::new().read(true).share_mode(1 | 2).open(&target).unwrap();
    write_bytes_atomic(&target, b"new").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    let names: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["export.json".to_string()], "no temp file is left");
    drop(_holder);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_atomic_write_replaces_the_file_a_symlink_points_at() {
    let dir = std::env::temp_dir().join(format!("ps-atomic-link-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let real = dir.join("real.json");
    let link = dir.join("link.json");
    fs::write(&real, b"before").unwrap();
    if std::os::windows::fs::symlink_file(&real, &link).is_err() {
        // Creating a symlink needs developer mode or admin rights; without them there is nothing to test.
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    write_bytes_atomic(&link, b"after").unwrap();
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "the link itself stays a link");
    assert_eq!(fs::read(&real).unwrap(), b"after", "the file behind the link is what changed");
    let _ = fs::remove_dir_all(&dir);
}

// ---- the clipboard text block is measured before it is copied ----

#[test]
fn a_clipboard_text_block_past_the_character_limit_in_bytes_is_never_read() {
    use clipwatch::{text_block_within_limit, CLIP_INBOX_MAX_CHARS, CLIP_INBOX_MAX_UNICODE_BYTES};
    assert_eq!(CLIP_INBOX_MAX_UNICODE_BYTES, CLIP_INBOX_MAX_CHARS * 4 + 2);
    assert!(text_block_within_limit(0));
    assert!(text_block_within_limit((CLIP_INBOX_MAX_CHARS + 1) * 2), "a limit-size text in UTF-16 plus its NUL fits");
    assert!(text_block_within_limit(CLIP_INBOX_MAX_UNICODE_BYTES));
    assert!(!text_block_within_limit(CLIP_INBOX_MAX_UNICODE_BYTES + 1));
    assert!(!text_block_within_limit(300 * 1024 * 1024), "a 300 MB block is refused unread");
}

// A folder change must not hold the store lock while it writes the library: the write
// closure here proves another thread can take the store mutex mid-write.
#[test]
fn set_sync_dir_core_writes_without_holding_the_store_lock() {
    let mut settings = Settings::default();
    settings.ui_flags.insert("promptSync".into(), true);
    let store = Mutex::new(Store {
        prompts: vec![sync_test_prompt()],
        trash: Vec::new(),
        settings,
        prompts_locked: false,
        written: Default::default(),
    });
    let saved = std::cell::Cell::new(0usize);
    let (old, new_dir, failures, enabled) = set_sync_dir_core(
        &store,
        "C:/sync-new",
        &|_| saved.set(saved.get() + 1),
        &|s| PathBuf::from(s.ui_texts.get("syncDir").cloned().unwrap_or_default()),
        |prompts, settings| {
            assert_eq!(prompts.len(), 1);
            assert!(sync_enabled(settings));
            let acquired = std::thread::scope(|sc| {
                sc.spawn(|| {
                    let start = std::time::Instant::now();
                    while start.elapsed() < Duration::from_secs(1) {
                        if store.try_lock().is_ok() {
                            return true;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    false
                })
                .join()
                .unwrap()
            });
            usize::from(!acquired)
        },
    );
    assert_eq!(failures, 0, "the store mutex was free while the folder write ran");
    assert!(enabled);
    assert_eq!(saved.get(), 1);
    assert_eq!(old, PathBuf::new());
    assert_eq!(new_dir, PathBuf::from("C:/sync-new"));
}

// ---- a failed recording start stops what it already started ----

pub(super) fn start_guard_spinner(flag: &Arc<AtomicBool>, done: &Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    let (flag, done) = (flag.clone(), done.clone());
    std::thread::spawn(move || {
        while !flag.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(5));
        }
        done.store(true, std::sync::atomic::Ordering::Relaxed);
    })
}

#[test]
fn start_guard_drop_ends_every_thread_it_holds_within_a_second() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let flag = Arc::new(AtomicBool::new(false));
    let (done_a, done_b, done_c) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
    // The third thread blocks on a channel only the wake action closes, the way the
    // feeder waits on its queue until a stop is requested.
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let blocked = {
        let done = done_c.clone();
        std::thread::spawn(move || {
            let _ = rx.recv();
            done.store(true, Ordering::Relaxed);
        })
    };
    let mut guard = recording::StartGuard::new(move || drop(tx));
    guard.add_flag(flag.clone());
    guard.audio_threads.push(start_guard_spinner(&flag, &done_a));
    guard.audio_threads.push(start_guard_spinner(&flag, &done_b));
    guard.feeder = Some(blocked);
    let began = std::time::Instant::now();
    drop(guard);
    assert!(began.elapsed() < Duration::from_secs(1), "dropping the guard waited too long");
    assert!(done_a.load(Ordering::Relaxed) && done_b.load(Ordering::Relaxed), "the flagged threads ended");
    assert!(done_c.load(Ordering::Relaxed), "the thread blocked on the channel was woken and ended");
}

#[test]
fn start_guard_after_take_joins_and_stops_nothing() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let flag = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let woke = Arc::new(AtomicBool::new(false));
    let mut guard = recording::StartGuard::new({
        let woke = woke.clone();
        move || woke.store(true, Ordering::Relaxed)
    });
    guard.add_flag(flag.clone());
    guard.audio_threads.push(start_guard_spinner(&flag, &done));
    let (audio, feeder, mic) = guard.take();
    drop(guard);
    assert!(!flag.load(Ordering::Relaxed), "a taken guard sets no stop flag");
    assert!(!woke.load(Ordering::Relaxed), "a taken guard runs no wake action");
    assert!(!done.load(Ordering::Relaxed), "the thread now belongs to the caller and still runs");
    assert_eq!((audio.len(), feeder.is_some(), mic.is_some()), (1, false, false));
    flag.store(true, Ordering::Relaxed);
    for t in audio {
        t.join().unwrap();
    }
    assert!(done.load(Ordering::Relaxed));
}

// ---- ffmpeg helpers give up after their time limit ----

pub(super) fn slow_child_command() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    // ping -n 11 runs for about ten seconds; the tests allow a loaded machine a few
    // seconds of slack and still tell a killed child from one that ran to the end.
    let mut cmd = std::process::Command::new("ping");
    cmd.args(["-n", "11", "127.0.0.1"]).creation_flags(0x0800_0000);
    cmd.stdin(std::process::Stdio::null());
    cmd
}

#[test]
fn status_in_job_kills_a_stalled_child_after_the_limit() {
    let began = std::time::Instant::now();
    let err = status_in_job(&mut slow_child_command(), Duration::from_secs(1)).expect_err("the limit must end the wait");
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    assert!(began.elapsed() < Duration::from_secs(7), "returned late: {:?}", began.elapsed());
    assert_eq!(tool_run_error("ffmpeg", &err), "ffmpeg-timeout");
}

#[test]
fn output_in_job_times_out_and_still_returns_a_quick_childs_output() {
    let began = std::time::Instant::now();
    let err = output_in_job(&mut slow_child_command(), Duration::from_secs(1)).expect_err("the limit must end the wait");
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    assert!(began.elapsed() < Duration::from_secs(7), "returned late: {:?}", began.elapsed());

    use std::os::windows::process::CommandExt;
    let mut quick = std::process::Command::new("cmd");
    quick.args(["/C", "echo hello"]).creation_flags(0x0800_0000);
    quick.stdin(std::process::Stdio::null());
    let out = output_in_job(&mut quick, Duration::from_secs(20)).expect("a quick child finishes");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("hello"));
}

#[test]
fn wait_with_deadline_leaves_no_child_running() {
    let mut child = slow_child_command().spawn().unwrap();
    let err = wait_with_deadline(&mut child, Duration::from_millis(300)).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    // Killed and reaped: the status is already known.
    assert!(child.try_wait().unwrap().is_some(), "the child was not reaped");
}

// ---- waveform peaks computed while the audio streams in ----

pub(super) fn waveform_test_pcm(samples: usize, seed: u64) -> Vec<u8> {
    let mut state = seed;
    let mut out = Vec::with_capacity(samples * 2);
    for i in 0..samples {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut v = (state >> 48) as i16;
        // A few exact extremes so the i16::MIN case is part of the data.
        if i % 1_000_003 == 7 {
            v = i16::MIN;
        }
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

pub(super) fn stream_peaks(pcm: &[u8], total_samples: u64, buckets: u32, seed: u64) -> Vec<f32> {
    let mut acc = PeakAccumulator::new(total_samples, buckets);
    let mut state = seed;
    let mut at = 0usize;
    while at < pcm.len() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let n = (1 + (state >> 40) as usize % 70_001).min(pcm.len() - at);
        acc.push(&pcm[at..at + n]);
        at += n;
    }
    acc.finish()
}

#[test]
fn streamed_peaks_equal_the_whole_buffer_peaks_for_one_hour_of_audio() {
    let samples = 3600 * WAVEFORM_SAMPLE_RATE as usize;
    let pcm = waveform_test_pcm(samples, 1);
    for buckets in [4000u32, 1234, 8] {
        let want = pcm_s16le_to_peaks(&pcm, buckets);
        let got = stream_peaks(&pcm, samples as u64, buckets, 99 + buckets as u64);
        assert_eq!(got, want, "{buckets} buckets");
    }
}

#[test]
fn streamed_peaks_match_on_uneven_and_tiny_inputs() {
    for (samples, buckets) in [(100_003usize, 777u32), (5, 8), (1, 8), (0, 8), (9_999, 4000), (65_537, 64)] {
        let pcm = waveform_test_pcm(samples, 7 + samples as u64);
        let want = pcm_s16le_to_peaks(&pcm, buckets);
        let got = stream_peaks(&pcm, samples as u64, buckets, 5);
        assert_eq!(got, want, "{samples} samples, {buckets} buckets");
    }
}

#[test]
fn streamed_peaks_fold_samples_past_the_announced_length_into_the_last_bucket() {
    let mut pcm = waveform_test_pcm(1000, 3);
    // A loud sample well past the announced 800.
    pcm.extend_from_slice(&i16::MAX.to_le_bytes());
    let got = stream_peaks(&pcm, 800, 8, 1);
    assert_eq!(got.len(), 8);
    assert!(got[7] > 0.99, "the overrun sample counts in the last bucket: {got:?}");
}

// ---- a crash-log lock left behind by a dead owner is taken over ----

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

// ---- the shared sidecar path builder ----

#[test]
fn sidecar_path_sits_next_to_the_recording_as_its_own_extension() {
    assert_eq!(
        sidecar_path(Path::new("C:/data/recordings/rec-1.mp4"), KEYS_SIDECAR_EXT),
        PathBuf::from("C:/data/recordings/rec-1.keys.json")
    );
    assert_eq!(
        sidecar_path(Path::new("C:/data/recordings/rec-cam-1.webm"), CAM_SIDECAR_EXT),
        PathBuf::from("C:/data/recordings/rec-cam-1.cam.json")
    );
}
