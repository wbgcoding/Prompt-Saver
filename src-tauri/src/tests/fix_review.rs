use super::*;

// ---- second-round review fixes ----

#[test]
fn an_update_uninstall_keeps_the_downloaded_ffmpeg() {
    let nsi = include_str!("../../installer.nsi");
    // The silent uninstall of the old version always precedes a fresh install, so it
    // carries a keep marker, and the uninstall section reads it before deleting the tools.
    let call = nsi.find("StrCpy $R1 \"$R1 _?=$4\"").expect("uninstall call");
    let before = &nsi[..call];
    assert!(before.contains("/KEEPTOOLS"), "the pre-install uninstall passes the keep marker");
    let un = nsi.find("Section Uninstall").unwrap();
    let del = nsi[un..].find("ffmpeg.exe\"").expect("ffmpeg delete") + un;
    let guard = &nsi[un..del];
    assert!(guard.contains("$KeepTools"), "the ffmpeg delete is guarded by the keep marker");
    assert!(nsi.contains("GetOptions} $CMDLINE \"/KEEPTOOLS\""), "un.onInit reads the marker");
}

fn raw_version_text(conn: &Connection, id: &str) -> String {
    let raw: String = conn
        .query_row("SELECT text FROM prompt_versions WHERE prompt_id = ?1", [id], |r| r.get(0))
        .unwrap();
    dec_str(&raw)
}

#[test]
fn unlocking_the_vault_seals_every_confidential_prompts_plain_version_rows() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    db_add_version(&conn, "sec", 1, "N", "plain secret body", &[], 20).unwrap();
    db_add_version(&conn, "open", 2, "N", "plain open body", &[], 20).unwrap();
    let mut sec = test_prompt("sec");
    sec.secret = true;
    let open = test_prompt("open");
    assert_eq!(seal_confidential_versions(&mut conn, &[sec.clone(), open.clone()], &key), 1);
    assert!(!raw_version_text(&conn, "sec").contains("plain secret body"));
    assert_eq!(raw_version_text(&conn, "open"), "plain open body", "a plain prompt keeps its history");
    // Idempotent: a second pass leaves the sealed text readable and untouched.
    seal_confidential_versions(&mut conn, &[sec], &key);
    let v = db_list_versions(&conn, "sec");
    assert_eq!(open_snapshot(&v[0].text, &v[0].variants, Some(key)).unwrap().0, "plain secret body");
}

#[test]
fn sealing_version_rows_overwrites_the_freed_pages() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    db_add_version(&conn, "a", 1, "N", "old body", &[], 20).unwrap();
    let before: i64 = conn.query_row("PRAGMA secure_delete", [], |r| r.get(0)).unwrap();
    assert_eq!(before, 0);
    db_seal_versions(&mut conn, "a", &key).unwrap();
    let after: i64 = conn.query_row("PRAGMA secure_delete", [], |r| r.get(0)).unwrap();
    assert_eq!(after, 1, "the plain pages are zeroed when the rows are rewritten");
}

#[test]
fn a_synced_file_cannot_name_a_network_path() {
    let p = md_sync_1::sync_test_prompt();
    let md = prompt_to_md(&p, &SyncPlacement::default());
    assert!(md.contains("file: \n") && md.contains("icon: \n"), "the fixture writes empty media lines");
    let net = md.replace("file: \n", "file: \\\\host\\share\\x.mp4\n").replace("icon: \n", "icon: //host/share/i.png\n");
    let got = apply_md_to_prompt(&p, &net).expect("valid frontmatter");
    assert_eq!(got.file_path, "", "a network file path from the folder is cleared");
    assert_eq!(got.icon_path, "", "a network icon path from the folder is cleared");
    let local = md.replace("file: \n", "file: C:/clips/a.mp4\n").replace("icon: \n", "icon: C:/clips/i.png\n");
    let got = apply_md_to_prompt(&p, &local).unwrap();
    assert_eq!((got.file_path.as_str(), got.icon_path.as_str()), ("C:/clips/a.mp4", "C:/clips/i.png"));
}

#[test]
fn a_backup_import_counts_the_network_links_it_removes() {
    let mut a = test_prompt("a");
    a.file_path = r"\\nas\share\a.mp4".into();
    a.icon_path = "//nas/share/i.png".into();
    let mut b = test_prompt("b");
    b.file_path = "C:/local/b.mp4".into();
    assert_eq!(count_network_links(&[a.clone(), b.clone()]), 2);
    assert_eq!(clear_network_paths(&mut a), 2, "the clearing reports how many links it cut");
    assert_eq!(clear_network_paths(&mut b), 0);
    assert_eq!(count_network_links(&[a, b]), 0);
}

#[test]
fn every_leading_separator_pair_and_nt_prefix_counts_as_a_network_path() {
    for p in [
        r"\/host/share/x.mp4",
        r"/\host\share\x.mp4",
        r"\??\UNC\host\share\x.mp4",
        r"\??\unc/host/share",
        r"\\?\UNC\host\share",
        "//?/UNC/host/share",
        r"\/?\UNC\host\share",
        r"  \\host\share",
    ] {
        assert!(is_network_path(p), "{p:?} must count as a network path");
    }
    for p in [r"\??\C:\clips\a.png", "/rooted/only.png", r"\single\lead.png", "C:/clips/clip.mp4"] {
        assert!(!is_network_path(p), "{p:?} is local");
    }
}

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
    keep_locked_backup(&root, t0);
    assert_eq!(locked_copies(&root).len(), 1, "the first incident leaves a timestamped copy");
    keep_locked_backup(&root, t0 + 60);
    assert_eq!(locked_copies(&root).len(), 1, "a relaunch within the hour is the same incident");
    keep_locked_backup(&root, t0 + 2 * day);
    keep_locked_backup(&root, t0 + 4 * day);
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

#[test]
fn the_startup_seed_removes_a_plaintext_file_of_a_confidential_prompt_itself() {
    let dir = md_sync_1::sync_tmp_dir("seed-removes-secret");
    let mut p = md_sync_1::sync_test_prompt();
    std::fs::write(dir.join("p1.md"), prompt_to_md(&p, &SyncPlacement::default())).unwrap();
    let foreign = dir.join("p9.md");
    std::fs::write(&foreign, "just my own notes, no header").unwrap();
    p.secret = true;
    let mut other = md_sync_1::sync_test_prompt();
    other.id = "p9".into();
    other.secret = true;
    sync_seed_seen(&dir, &[p, other], &Settings::default());
    assert!(!dir.join("p1.md").exists(), "the seed itself removes our plaintext copy");
    assert!(foreign.exists(), "a file that is not ours is never touched");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_update_switch_save_restores_the_previous_values() {
    let mut s = Settings { auto_update: false, update_consent: UpdateConsent::No, ..Default::default() };
    assert_eq!(set_update_choice(&mut s, true, |_| false), Err("save-failed".to_string()));
    assert!(!s.auto_update, "the background check must not see a switch that failed to save");
    assert_eq!(s.update_consent, UpdateConsent::No);
    assert!(set_update_choice(&mut s, true, |_| true).is_ok());
    assert!(s.auto_update && s.update_consent == UpdateConsent::Yes);
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
fn the_stats_and_theme_file_dialogs_wait_on_a_blocking_thread() {
    let stats = include_str!("../stats.rs");
    let theme = include_str!("../theme.rs");
    for (src, name) in [(stats, "export_stats"), (theme, "export_custom_theme"), (theme, "import_custom_theme")] {
        let body = diff_writes::fn_text(src, name);
        assert!(body.contains("spawn_blocking"), "{name} must not hold an async worker for the dialog");
        assert!(body.contains("file_dialog("), "{name} still opens the dialog");
    }
}

#[test]
fn the_skip_list_accepts_a_pre_release_tag() {
    let mut list = Vec::new();
    assert!(add_skipped_version(&mut list, "1.2.3-beta.1".to_string()), "a pre-release tag is stored");
    assert!(add_skipped_version(&mut list, "2.0.0-rc.2".to_string()));
    assert_eq!(list, ["1.2.3-beta.1", "2.0.0-rc.2"]);
    for bad in ["beta", "-1.2.3", "1.2.3-", "1.2.3 beta", "1.2.3_beta", "1.2.3-<b>", "1.2.3-é", "1.2.3-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"] {
        assert!(!add_skipped_version(&mut list, bad.to_string()), "{bad:?} is not version-shaped");
    }
}

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
fn a_verbatim_or_nt_path_is_local_only_for_a_drive_letter_or_a_volume_guid() {
    for p in [
        r"\\?\GLOBALROOT\Device\Mup\host\share\x.mp4",
        r"\??\GLOBALROOT\Device\Mup\host\share\x.mp4",
        r"\\?\globalroot\Device\LanmanRedirector\host\share",
        r"\\?\Device\Mup\host\share",
        r"\\?\Volume{not-a-guid}\x",
        r"\\?\",
    ] {
        assert!(is_network_path(p), "{p:?} must count as a network path");
    }
    for p in [
        r"\\?\C:\clips\a.png",
        r"\??\C:\clips\a.png",
        r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\clips\a.png",
    ] {
        assert!(!is_network_path(p), "{p:?} is local");
    }
}

#[test]
fn a_failed_seal_keeps_the_history_and_a_sealed_row_is_not_rewritten() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut conn = Connection::open_in_memory().unwrap();
    assert!(db_conn_init_schema(&conn));
    db_add_version(&conn, "sec", 1, "N", "plain secret body", &[], 20).unwrap();
    let mut sec = test_prompt("sec");
    sec.secret = true;

    // A write that fails (busy in the field, a trigger here) must not wipe the history.
    conn.execute_batch(
        "CREATE TRIGGER no_update BEFORE UPDATE ON prompt_versions BEGIN SELECT RAISE(ABORT, 'busy'); END;",
    )
    .unwrap();
    seal_confidential_versions(&mut conn, &[sec.clone()], &key);
    let kept: i64 = conn.query_row("SELECT COUNT(*) FROM prompt_versions", [], |r| r.get(0)).unwrap();
    assert_eq!(kept, 1, "a failed seal leaves the rows for the next unlock");
    conn.execute_batch("DROP TRIGGER no_update;").unwrap();

    // Once sealed, a later unlock writes nothing.
    seal_confidential_versions(&mut conn, &[sec.clone()], &key);
    conn.execute_batch(
        "CREATE TABLE upd_log (n INTEGER);
         CREATE TRIGGER log_update AFTER UPDATE ON prompt_versions BEGIN INSERT INTO upd_log VALUES (1); END;",
    )
    .unwrap();
    seal_confidential_versions(&mut conn, &[sec], &key);
    let writes: i64 = conn.query_row("SELECT COUNT(*) FROM upd_log", [], |r| r.get(0)).unwrap();
    assert_eq!(writes, 0, "an already sealed row is left alone");
}

#[test]
fn deleting_my_data_also_removes_the_per_user_local_tool_folder() {
    let nsi = include_str!("../../installer.nsi").replace("\r\n", "\n");
    let un = nsi.find("Section Uninstall").unwrap();
    let branch = &nsi[un..];
    let roaming = branch.find("RmDir /r \"$APPDATA\\${DATAFOLDER}\"").expect("roaming delete");
    let local = branch.find("RmDir /r \"$LOCALAPPDATA\\${DATAFOLDER}\"").expect("local data folder delete");
    assert!(local > roaming && local - roaming < 300, "the local folder is removed beside the roaming one");
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
