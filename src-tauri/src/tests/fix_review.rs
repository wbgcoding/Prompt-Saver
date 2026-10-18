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
