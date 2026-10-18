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
    for p in [r"\??\C:\Users\x\a.png", "/rooted/only.png", r"\single\lead.png", "C:/Users/x/clip.mp4"] {
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
