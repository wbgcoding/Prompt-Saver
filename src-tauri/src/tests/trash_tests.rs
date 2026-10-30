use super::*;

#[test]
fn the_wipe_repack_waits_out_a_connection_that_briefly_holds_the_write_lock() {
    let dir = std::env::temp_dir().join(format!("{}-test-scrub-busy", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("data.db");
    let conn = Connection::open(&db).unwrap();
    conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(())).unwrap();
    conn.execute_batch("CREATE TABLE prompts(id TEXT PRIMARY KEY, data TEXT NOT NULL); INSERT INTO prompts VALUES ('a', 'x');").unwrap();
    conn.execute("DELETE FROM prompts", []).unwrap();
    conn.execute_batch("PRAGMA busy_timeout=0;").unwrap();
    let (locked_tx, locked_rx) = std::sync::mpsc::channel();
    let other = db.clone();
    let holder = std::thread::spawn(move || {
        let c2 = Connection::open(&other).unwrap();
        c2.execute_batch("BEGIN IMMEDIATE;").unwrap();
        locked_tx.send(()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(400));
        c2.execute_batch("COMMIT;").unwrap();
    });
    locked_rx.recv().unwrap();
    assert!(scrub_freed_pages(&conn), "a short busy spell must not fail the repack");
    holder.join().unwrap();
    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_wipe_counts_as_incomplete_only_while_rows_are_left() {
    let conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    assert!(!data_wipe_rows_left(&conn), "an empty store has no rows left");
    db_add_version(&conn, "a", 1, "N", "t", &[], 5).unwrap();
    assert!(data_wipe_rows_left(&conn), "a surviving row keeps the wipe incomplete");
    wipe_data_tables(&conn);
    assert!(!data_wipe_rows_left(&conn));
}

#[test]
fn the_wipe_repack_gives_up_within_its_budget_and_finishes_on_the_next_start() {
    let dir = std::env::temp_dir().join(format!("{}-test-scrub-budget", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("data.db");
    let conn = Connection::open(&db).unwrap();
    conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(())).unwrap();
    conn.execute_batch("CREATE TABLE prompts(id TEXT PRIMARY KEY, data TEXT NOT NULL); INSERT INTO prompts VALUES ('a', 'x');").unwrap();
    conn.execute("DELETE FROM prompts", []).unwrap();
    // The app's own connections wait up to 3 s per statement for a lock.
    conn.execute_batch("PRAGMA busy_timeout=3000;").unwrap();
    let (locked_tx, locked_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let other = db.clone();
    let holder = std::thread::spawn(move || {
        let c2 = Connection::open(&other).unwrap();
        c2.execute_batch("BEGIN IMMEDIATE;").unwrap();
        locked_tx.send(()).unwrap();
        let _ = release_rx.recv_timeout(std::time::Duration::from_secs(20));
        c2.execute_batch("COMMIT;").unwrap();
    });
    locked_rx.recv().unwrap();
    // A writer that never lets go: the repack stops at its budget instead of waiting on
    // 25 tries of 3 s each.
    let started = std::time::Instant::now();
    assert!(!scrub_freed_pages_within(&conn, std::time::Duration::from_millis(300)));
    assert!(started.elapsed() < std::time::Duration::from_secs(2), "the wait is capped, took {:?}", started.elapsed());
    // The unfinished scrub leaves a marker; the next start finishes it once the writer is gone.
    assert!(!finish_pending_scrub_in(&dir, &conn), "no marker, nothing to do");
    mark_scrub_pending(&dir);
    assert!(scrub_pending_marker(&dir).exists());
    release_tx.send(()).unwrap();
    holder.join().unwrap();
    assert!(finish_pending_scrub_in(&dir, &conn));
    assert!(!scrub_pending_marker(&dir).exists(), "the marker goes once the scrub worked");
    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);

    // delete_all_data marks an unfinished scrub, and the start-up runs the pending one.
    let trash = include_str!("../trash.rs").replace("\r\n", "\n");
    let at = trash.find("pub(crate) async fn delete_all_data").unwrap();
    let body = &trash[at..];
    let body = &body[..body.find("\n}\n").unwrap()];
    assert!(body.contains("mark_scrub_pending("), "an unfinished repack is remembered");
    assert!(include_str!("../lib.rs").replace("\r\n", "\n").contains("finish_pending_scrub("), "the start-up finishes it");
}

#[test]
fn a_reset_keeps_a_trash_that_never_empties_itself() {
    let mut old = Settings::default();
    old.ui_values.insert("trashRetentionDays".into(), 0.0);
    assert_eq!(trash_retention_secs(&settings_after_reset(&old)), None, "never-purge survives a reset");
    old.ui_values.insert("trashRetentionDays".into(), 365.0);
    assert_eq!(trash_retention_secs(&settings_after_reset(&old)), Some(365 * 86_400));
    old.ui_values.insert("trashRetentionDays".into(), 7.0);
    assert_eq!(trash_retention_secs(&settings_after_reset(&old)), Some(30 * 86_400), "a shorter window takes the default");
}

// The installed app can be running in the tray: its newest rows may exist only in data.db-wal,
// and a plain file copy of data.db would miss them.
#[test]
fn adopting_a_running_store_takes_a_snapshot_with_its_wal_rows() {
    let root = std::env::temp_dir().join(format!("{}-test-adopt-live-{}", brand::SLUG, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (src, dst) = (root.join("src"), root.join("dst"));
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    let live = db_conn_at(&src).unwrap();
    live.execute("PRAGMA wal_autocheckpoint=0", []).ok();
    live.execute("INSERT INTO meta(key, value) VALUES('wal_probe', '1')", []).unwrap();
    let mut landed = false;
    copy_live_store_files(&src, &dst, &mut landed).unwrap();
    assert!(landed);
    assert!(!dst.join("data.db.adopting").exists(), "the staging file is gone");
    let copy = Connection::open(dst.join("data.db")).unwrap();
    let n: i64 = copy.query_row("SELECT COUNT(*) FROM meta WHERE key='wal_probe'", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "the row that only lived in the log is in the adopted database");
    // A source that is no database lands nothing, so the caller keeps the store it has.
    let bad = root.join("bad");
    let bad_dst = root.join("bad_dst");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::create_dir_all(&bad_dst).unwrap();
    std::fs::write(bad.join("data.db"), b"not a database at all, just text").unwrap();
    let mut landed = false;
    assert!(copy_live_store_files(&bad, &bad_dst, &mut landed).is_err());
    assert!(!landed && !bad_dst.join("data.db").exists());
    drop(copy);
    drop(live);
    let _ = std::fs::remove_dir_all(&root);
}

// The empty-library check and the swap run under one hold of the store lock, so a prompt added in
// between cannot be replaced by the adopted database.
#[test]
fn adopting_checks_and_swaps_under_one_store_lock() {
    let src = include_str!("../trash.rs").replace("\r\n", "\n");
    let at = src.find("pub(crate) fn takeover_apply").unwrap();
    let body = &src[at..];
    let body = &body[..body.find("\n}\n").unwrap()];
    let adopt = &body[body.find("if adopt {").unwrap()..body.find("if remember {").unwrap()];
    assert_eq!(adopt.matches("lock(&state)").count(), 1, "one acquisition for check and swap");
    assert!(adopt.find("lock(&state)").unwrap() < adopt.find("store_has_no_real_prompts").unwrap());
    assert!(adopt.find("store_has_no_real_prompts").unwrap() < adopt.find("copy_live_store_files").unwrap());
    assert!(adopt.find("copy_live_store_files").unwrap() < adopt.find("*store = fresh").unwrap());
}
