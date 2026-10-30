use super::*;

fn locked_copies(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("data.db.locked"))
        .collect();
    v.sort();
    v
}

#[test]
fn locked_database_copies_are_kept_per_incident_newest_two_and_aged_out() {
    let root = std::env::temp_dir().join(format!("{}-test-locked-rotate", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("data.db"), b"rows").unwrap();
    let day = 86_400u64;
    let t0 = 1_700_000_000u64;
    keep_locked_backup(&root, t0, None);
    assert_eq!(locked_copies(&root).len(), 1, "the first incident leaves a timestamped copy");
    keep_locked_backup(&root, t0 + 60, None);
    assert_eq!(locked_copies(&root).len(), 1, "a relaunch within the hour is the same incident");
    keep_locked_backup(&root, t0 + 2 * day, None);
    keep_locked_backup(&root, t0 + 4 * day, None);
    let left = locked_copies(&root);
    assert_eq!(left.len(), 2, "only the newest two copies stay: {left:?}");
    assert!(left.iter().all(|n| !n.ends_with(&format!(".{t0}.bak"))), "the oldest copy rotated out");
    // A clean launch keeps the copies (they may be the only pre-lock rows) until they are 90 days old.
    prune_locked_backups(&root, t0 + 5 * day);
    assert_eq!(locked_copies(&root).len(), 2, "a clean launch does not delete a young copy");
    prune_locked_backups(&root, t0 + 4 * day + 91 * day);
    assert!(locked_copies(&root).is_empty(), "copies older than 90 days are removed at launch");
    let _ = std::fs::remove_dir_all(&root);
}

// Rows committed since the last checkpoint live only in data.db-wal; a bare copy of data.db
// would not hold them.
#[test]
fn the_locked_copy_holds_rows_still_in_the_wal() {
    let root = std::env::temp_dir().join(format!("{}-test-locked-wal", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let conn = db_conn_at(&root).unwrap();
    conn.execute("PRAGMA wal_autocheckpoint=0", []).ok();
    conn.execute("INSERT INTO meta(key, value) VALUES('wal_probe', '1')", []).unwrap();
    keep_locked_backup(&root, 1_700_000_000, Some(&conn));
    let copy = Connection::open(root.join("data.db.locked.1700000000.bak")).unwrap();
    let n: i64 = copy.query_row("SELECT COUNT(*) FROM meta WHERE key='wal_probe'", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "the safety copy must contain the committed row");
    drop(copy);
    drop(conn);
    let _ = std::fs::remove_dir_all(&root);
}

fn version_count(conn: &Connection, id: &str) -> usize {
    db_list_versions(conn, id).len()
}

#[test]
fn an_undo_snapshot_trims_only_after_the_restore_saved() {
    let conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    for i in 0..5u64 {
        db_add_version(&conn, "p", i + 1, "N", &format!("v{i}"), &[], 5).unwrap();
    }
    assert_eq!(version_count(&conn, "p"), 5);
    // A restore that fails: the snapshot goes in without trimming and comes out again.
    db_insert_version(&conn, "p", 9, "N", "current", &[]).unwrap();
    db_drop_latest_version(&conn, "p");
    let texts: Vec<String> = db_list_versions(&conn, "p").into_iter().map(|v| v.text).collect();
    assert_eq!(texts.len(), 5, "a failed restore loses no history row");
    assert!(texts.contains(&"v0".to_string()), "the oldest real version survives the failed attempt");
    // A restore that saved: the trim follows.
    db_insert_version(&conn, "p", 10, "N", "current", &[]).unwrap();
    db_trim_versions(&conn, "p", 5).unwrap();
    assert_eq!(version_count(&conn, "p"), 5);
}

#[test]
fn a_version_row_with_an_unreadable_column_is_left_untouched() {
    let mut conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    db_add_version(&conn, "p1", 10, "Name", "Old {#{a}#}", &["Entry".to_string()], 20).unwrap();
    // The variants column becomes something that looks encrypted but cannot be read here.
    conn.execute("UPDATE prompt_versions SET variants = ?1", params![format!("{ENC_PREFIX}AAAA")]).unwrap();
    let before: (String, String) = conn
        .query_row("SELECT text, variants FROM prompt_versions", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!(db_upgrade_version_rows(&mut conn).unwrap(), 0);
    let after: (String, String) = conn
        .query_row("SELECT text, variants FROM prompt_versions", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!(before, after, "a row with an unreadable column is never rewritten");
}

#[test]
#[cfg(windows)]
fn restoring_a_version_with_an_unreadable_column_is_refused() {
    let conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    db_add_version(&conn, "p1", 10, "", "Entry", &["v".to_string()], 20).unwrap();
    let id = db_list_versions(&conn, "p1")[0].id;
    // A legitimately empty name reads back as empty and is still restorable.
    assert!(db_get_version(&conn, id).is_some());
    // Text that looks sealed but cannot be read here must not come back as "".
    conn.execute("UPDATE prompt_versions SET text = ?1", params![format!("{ENC_PREFIX}AAAA")]).unwrap();
    assert!(db_get_version(&conn, id).is_none());
    assert!(dec_str_strict(&format!("{ENC_PREFIX}AAAA")).is_none());
    assert_eq!(dec_str_strict("plain").as_deref(), Some("plain"));
}

#[test]
fn an_unreadable_settings_file_is_reported_to_the_caller_that_locks_settings_writes() {
    let dir = std::env::temp_dir().join(format!("{}-test-fallback-settings-lock", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (_, unreadable) = load_store_from_json_parts(&dir);
    assert!(!unreadable, "no settings file is a fresh install, not a corrupt one");
    std::fs::write(dir.join("settings.json"), format!("{ENC_PREFIX}not-readable")).unwrap();
    let (store, unreadable) = load_store_from_json_parts(&dir);
    assert!(unreadable);
    assert!(store.prompts_locked);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_plaintext_trash_row_is_deleted_by_a_purge_and_sealed_by_a_keep() {
    let conn = row_test_db();
    for id in ["gone", "kept"] {
        conn.execute("INSERT INTO trash VALUES(?1, 0, ?2)", params![id, serde_json::to_string(&test_prompt(id)).unwrap()]).unwrap();
    }
    let mut written = WrittenRows::new();
    let (trash, failures) = db_load_trash(&conn, &mut written);
    assert_eq!((trash.len(), failures), (2, 0));
    let kept: Vec<Prompt> = trash.into_iter().filter(|p| p.id == "kept").collect();
    let mut conn = conn;
    db_write_trash(&mut conn, &kept, &mut written).unwrap();
    let rows = stored_rows(&conn, "trash");
    assert_eq!(rows.len(), 1, "the purged plaintext row must not survive");
    assert_eq!(rows[0].0, "kept");
    assert!(rows[0].2.starts_with(ENC_PREFIX), "the kept row is sealed by the write");
}

#[test]
fn a_plaintext_prompt_row_loaded_from_the_db_is_deleted_by_a_later_save() {
    let mut conn = row_test_db();
    for id in ["gone", "kept"] {
        conn.execute("INSERT INTO prompts VALUES(?1, 0, ?2)", params![id, serde_json::to_string(&test_prompt(id)).unwrap()]).unwrap();
    }
    let mut written = WrittenRows::new();
    let (prompts, _, plain, failures, _) = db_load(&conn, &mut written);
    assert_eq!((prompts.len(), plain, failures), (2, true, 0));
    assert_eq!(written.get(&row_key("prompts", "gone")), Some(&STALE_ROW_HASH), "every loaded row is known to this process");
    let kept: Vec<Prompt> = prompts.into_iter().filter(|p| p.id == "kept").collect();
    db_write_prompts(&mut conn, &kept, &mut written).unwrap();
    let rows = stored_rows(&conn, "prompts");
    assert_eq!(rows.len(), 1, "the deleted plaintext row must not survive the save");
    assert_eq!(rows[0].0, "kept");
    assert!(rows[0].2.starts_with(ENC_PREFIX), "the kept row is sealed by the write");
}

#[test]
fn a_busy_store_is_skipped_by_the_event_loop_lock_and_a_poisoned_one_is_taken_over() {
    let m = std::sync::Arc::new(Mutex::new(5));
    {
        let held = m.lock().unwrap();
        assert!(try_lock_ignoring_poison(&m).is_none(), "held by another: no waiting");
        drop(held);
    }
    assert_eq!(*try_lock_ignoring_poison(&m).unwrap(), 5);
    let m2 = m.clone();
    let _ = std::thread::spawn(move || {
        let _g = m2.lock().unwrap();
        panic!("poison it");
    })
    .join();
    assert_eq!(*try_lock_ignoring_poison(&m).unwrap(), 5);
}

#[test]
fn a_locked_settings_row_counts_as_written_for_a_pair_but_a_real_failure_does_not() {
    assert!(settings_pair_written(true, false));
    assert!(settings_pair_written(false, true), "a locked row must not block prompt writes");
    assert!(!settings_pair_written(false, false), "a real write failure undoes the pair");
}

#[test]
fn a_failed_restore_takes_its_undo_snapshot_back_out() {
    let conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    db_add_version(&conn, "p1", 10, "Name", "first", &[], 20).unwrap();
    db_add_version(&conn, "p2", 11, "Other", "keep", &[], 20).unwrap();
    // The snapshot the restore took of the live state.
    db_add_version(&conn, "p1", 12, "Name", "live", &[], 20).unwrap();
    db_drop_latest_version(&conn, "p1");
    let texts: Vec<String> = db_list_versions(&conn, "p1").into_iter().map(|v| v.text).collect();
    assert_eq!(texts, ["first"]);
    assert_eq!(db_list_versions(&conn, "p2").len(), 1, "another prompt's history is untouched");
}

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
        assert_eq!(owners, 1, "only the claimant that creates the takeover marker owns it (round {round})");
        let _ = fs::remove_dir_all(&dir);
    }
}

// The slow-writer case the read-back alone could not settle: another claimant has won the
// marker for this dead lock but its lock write has not landed yet.
#[test]
fn a_takeover_marker_held_by_another_claimant_keeps_the_folder_read_only() {
    let dir = lock_test_dir("marker-held");
    let path = data_dir_lock_path(&dir);
    let old = lock_info("OLD", "old", 7, 1);
    write_data_dir_lock(&path, &old);
    fs::write(takeover_marker_path(&path, &lock_content_key(&old)), b"").unwrap();
    let t0 = Instant::now();
    let mut obs = LockObserver::new(t0);
    let me = LockMe { host: "B", pid: 1, id: "idb" };
    assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, t0, Duration::ZERO), LockOutcome::ReadOnly(_)));
    let later = t0 + Duration::from_secs(DATA_DIR_LOCK_STALE_SECS + 1);
    assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, later, Duration::ZERO), LockOutcome::ReadOnly(h) if h == "OLD"));
    assert_eq!(read_data_dir_lock(&path).unwrap().id, "old", "the lock was not written over");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_marker_left_by_a_claimant_that_died_mid_takeover_is_cleared_after_another_stale_limit() {
    let dir = lock_test_dir("marker-dead");
    let path = data_dir_lock_path(&dir);
    let old = lock_info("OLD", "old", 7, 1);
    write_data_dir_lock(&path, &old);
    let marker = takeover_marker_path(&path, &lock_content_key(&old));
    fs::write(&marker, b"").unwrap();
    let t0 = Instant::now();
    let mut obs = LockObserver::new(t0);
    let me = LockMe { host: "B", pid: 1, id: "idb" };
    let stale = Duration::from_secs(DATA_DIR_LOCK_STALE_SECS);
    let _ = claim_data_dir_lock(&path, &me, &mut obs, t0, Duration::ZERO);
    let lost_at = t0 + stale + Duration::from_secs(1);
    assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, lost_at, Duration::ZERO), LockOutcome::ReadOnly(_)));
    let almost = lost_at + stale - Duration::from_secs(1);
    assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, almost, Duration::ZERO), LockOutcome::ReadOnly(_)));
    assert!(matches!(claim_data_dir_lock(&path, &me, &mut obs, lost_at + stale, Duration::ZERO), LockOutcome::Owned));
    assert_eq!(read_data_dir_lock(&path).unwrap().id, "idb");
    remove_takeover_markers(&path);
    assert!(!marker.exists(), "the owner clears the markers");
    assert!(path.exists(), "the lock itself stays");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn takeover_markers_count_as_app_data_files() {
    let path = data_dir_lock_path(Path::new("C:\\data"));
    let name = takeover_marker_path(&path, "OLD|1|old|7|1").file_name().unwrap().to_string_lossy().to_string();
    assert!(crate::snip::is_app_data_file(&name, false));
    assert!(!crate::snip::is_app_data_file("datadir.lock.json.notes.txt", false));
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
