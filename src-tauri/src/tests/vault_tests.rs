use super::*;

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
fn a_confidential_prompt_reaches_the_window_in_the_new_spelling() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut p = test_prompt("a");
    p.secret = true;
    p.text = secret_encrypt("Hallo {#{Kunde}#} am {#{/date\\}#}", &key);
    p.variants = vec![secret_encrypt("Hi {#{Kunde}#}", &key)];
    let slim = slim_prompts(&[p], Some(key));
    assert_eq!(slim[0].text, "Hallo {?Kunde} am {?@date}");
    assert_eq!(slim[0].variants, vec!["Hi {?Kunde}".to_string()]);
    assert!(!find_var_tokens(&slim[0].text).is_empty(), "the window must see a placeholder it can resolve");
}

#[test]
fn a_damaged_fixed_key_backup_is_corrupt_not_password_protected() {
    let mut fixed = backup_encrypt(b"some backup body", None).expect("encrypt");
    let last = fixed.len() - 1;
    fixed[last] ^= 0xff;
    assert_eq!(backup_decrypt(&fixed, None).unwrap_err(), "corrupt backup");
    assert_eq!(backup_decrypt(&fixed, Some("pw")).unwrap_err(), "corrupt backup");
    let mut locked = backup_encrypt(b"some backup body", Some("pw")).expect("encrypt");
    let last = locked.len() - 1;
    locked[last] ^= 0xff;
    assert!(backup_decrypt(&locked, Some("pw")).unwrap_err().to_lowercase().contains("password"));
}

#[test]
fn a_data_only_import_writes_the_vault_keys_it_carried_in() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let (settings, p, _) = vault_fixture();
    for (foreign, expect_settings_written) in [(true, true), (false, false)] {
        let mut store = vault_store(settings.clone(), p.clone());
        let mut seen = None;
        run_import(&mut store, foreign_backup(foreign), true, false, |_, prompts, settings| {
            seen = Some((prompts, settings));
            true
        })
        .unwrap();
        assert_eq!(seen, Some((true, expect_settings_written)), "foreign vault keys: {foreign}");
    }
}

#[test]
fn a_locked_confidential_rotation_has_no_plain_prompt() {
    let key = secret_derive_key("pw", b"0123456789abcdef").expect("derive");
    let mut p = test_prompt("a");
    p.secret = true;
    p.text = secret_encrypt("entry one", &key);
    p.variants = vec![secret_encrypt("entry two", &key)];
    assert!(revealed_prompt(&p, None).is_none(), "a locked entry must not yield ciphertext");
    let plain = revealed_prompt(&p, Some(key)).expect("unlocked");
    let settings = Settings::default();
    assert_eq!(rotation_next(&plain, &settings).2, "entry one");
}

#[test]
fn an_import_over_the_decrypt_lock_replaces_rows_this_process_never_read() {
    let mut conn = import_test_db();
    let store = import_test_store(&mut conn);
    // A row sealed under another account: on disk, never loaded, never in the map.
    conn.execute("INSERT INTO prompts VALUES('foreign', 9, 'sealed')", []).unwrap();
    let incoming = vec![test_prompt("new1")];
    // A plain write spares it (another instance's unseen work) ...
    db_write_import(&mut conn, &store, &incoming, true, false, false).unwrap();
    let still: i64 = conn.query_row("SELECT COUNT(*) FROM prompts WHERE id = 'foreign'", [], |r| r.get(0)).unwrap();
    assert_eq!(still, 1);
    // ... a replacing write does not.
    db_write_import(&mut conn, &store, &incoming, true, false, true).unwrap();
    let ids: Vec<String> = conn.prepare("SELECT id FROM prompts ORDER BY ord").unwrap().query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
    assert_eq!(ids, vec!["new1".to_string()]);
}

#[test]
fn a_failed_replacing_import_does_not_teach_the_map_about_unread_rows() {
    let mut conn = import_test_db();
    let store = import_test_store(&mut conn);
    conn.execute("INSERT INTO prompts VALUES('foreign', 9, 'sealed')", []).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_prompts BEFORE INSERT ON prompts BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    assert!(db_write_import(&mut conn, &store, &[test_prompt("new1")], true, false, true).is_err());
    assert!(!store.written.borrow().contains_key(&row_key("prompts", "foreign")));
}

#[test]
fn history_snapshots_of_a_confidential_entry_open_with_the_key_and_plain_ones_pass_through() {
    let key = [7u8; 32];
    let sealed = secret_encrypt("the real text", &key);
    assert_eq!(reveal_if_sealed(&sealed, &key), "the real text");
    assert_eq!(reveal_if_sealed("written before it was confidential", &key), "written before it was confidential");
    // A different key opens nothing: the value comes back untouched, never as garbage text.
    assert_eq!(reveal_if_sealed(&sealed, &[9u8; 32]), sealed);
}

#[test]
fn an_old_sealed_snapshot_is_opened_or_refused_never_shown_as_ciphertext() {
    let key = [7u8; 32];
    let sealed = secret_encrypt("the real text", &key);
    let variants = vec![secret_encrypt("variant", &key), "plain variant".to_string()];
    assert!(looks_sealed(&sealed));
    assert!(!looks_sealed("just some words"));
    // Entry made plain again: the key still opens its older sealed snapshots.
    let (t, v) = open_snapshot(&sealed, &variants, Some(key)).unwrap();
    assert_eq!(t, "the real text");
    assert_eq!(v, vec!["variant".to_string(), "plain variant".to_string()]);
    // No key: a sealed text or variant is refused, plain content passes.
    assert_eq!(open_snapshot(&sealed, &[], None), Err("secret-locked".to_string()));
    assert_eq!(open_snapshot("plain", &variants, None), Err("secret-locked".to_string()));
    assert_eq!(open_snapshot("plain", &["x".to_string()], None), Ok(("plain".to_string(), vec!["x".to_string()])));
}

#[test]
fn json_round_trip_keeps_a_confidential_entry_sealed() {
    let key = [7u8; 32];
    let mut p = test_prompt("s1");
    p.name = "Vault".into();
    p.secret = true;
    p.text = secret_encrypt("hidden text", &key);
    p.variants = vec![secret_encrypt("hidden variant", &key)];
    let mut plain = test_prompt("s2");
    plain.name = "Plain".into();
    plain.text = "visible".into();
    let json = to_json(&[p.clone(), plain]);
    assert_eq!(json.matches("secret").count(), 1, "the flag is written for the confidential entry only");
    let data = parse_json_data(&json).unwrap();
    let mut store = bare_import_store();
    assert_eq!(append_imported_prompts(&mut store, data.prompts), 2);
    let back = store.prompts.iter().find(|x| x.name == "Vault").unwrap();
    assert!(back.secret, "the entry arrives confidential");
    assert_eq!(back.text, p.text, "its text stays sealed");
    assert_eq!(secret_decrypt(&back.text, &key).unwrap(), "hidden text");
    assert!(!store.prompts.iter().find(|x| x.name == "Plain").unwrap().secret);
}

#[test]
fn a_dropped_secret_session_leaves_no_key_bytes() {
    let mut sess = std::mem::ManuallyDrop::new(SecretSession {
        key: [0xA5; 32],
        unlocked_at: std::time::Instant::now(),
    });
    unsafe { std::mem::ManuallyDrop::drop(&mut sess) };
    assert_eq!(sess.key, [0u8; 32], "the key must be zeroed on drop");
}

// The shuffle behind the random rotation order: each of the 24 orders of four items comes up
// about equally often, so no entry is favoured. 10 000 shuffles give about 417 per order with a
// spread near 20; the bounds sit far outside that.
#[test]
fn the_secure_shuffle_is_roughly_uniform_over_four_items() {
    let mut counts = std::collections::HashMap::new();
    for _ in 0..10_000 {
        let mut v = [0u8, 1, 2, 3];
        shuffle_random(&mut v);
        *counts.entry(v).or_insert(0u32) += 1;
    }
    assert_eq!(counts.len(), 24, "every order must be reachable");
    for (order, n) in &counts {
        assert!((280..=560).contains(n), "order {order:?} came up {n} times of 10000");
    }
    let mut empty: [u8; 0] = [];
    shuffle_random(&mut empty);
    let mut one = [7u8];
    shuffle_random(&mut one);
    assert_eq!(one, [7]);
}

#[test]
fn random_below_stays_in_range_and_covers_it() {
    assert_eq!(random_below(0), 0);
    assert_eq!(random_below(1), 0);
    let mut seen = [false; 36];
    for _ in 0..2_000 {
        let v = random_below(36);
        assert!(v < 36);
        seen[v] = true;
    }
    assert!(seen.iter().all(|&s| s), "all 36 values show up in 2000 draws");
    let mut a = [0u8; 32];
    let mut b = [0u8; 32];
    os_random(&mut a);
    os_random(&mut b);
    assert_ne!(a, b, "two draws of 32 bytes differ");
}
