use super::*;

// ---- Selective backup restore ----

// restore_selective opens real files via Connection::open (ATTACH DATABASE
// needs a path, an in-memory DB can't be attached across connections), so these
// tests write actual throwaway .db files under the OS temp dir and remove them
// when done. `tag` keeps parallel test runs from colliding on the same names.
pub(super) fn full_schema_db(path: &std::path::Path, tag: &str) -> Connection {
    let _ = std::fs::remove_file(path);
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
             CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);",
    )
    .unwrap();
    let _ = tag; // only distinguishes the caller's temp filename, not the schema
    conn
}

// A readable-but-foreign SQLite file (empty, wrong schema, a stray .db the user
// picked) must never pass as a restorable backup: for scope "all" it would be
// renamed straight onto the live data.db.
#[test]
fn stage_and_validate_backup_accepts_a_real_backup_and_rejects_a_foreign_db() {
    let root = std::env::temp_dir().join(format!("{}-test-stage-validate", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let real = root.join("real.db");
    full_schema_db(&real, "real");
    let staged = stage_and_validate_backup(&real, &root).expect("a real backup must be accepted");
    assert!(staged.exists());
    let _ = std::fs::remove_file(&staged);

    let foreign = root.join("foreign.db");
    let _ = std::fs::remove_file(&foreign);
    Connection::open(&foreign).unwrap().execute_batch("CREATE TABLE unrelated(x INTEGER);").unwrap();
    let err = stage_and_validate_backup(&foreign, &root).unwrap_err();
    assert_eq!(err, "backup-schema-mismatch");
    assert!(!staged.exists(), "a rejected file must not be left staged behind");

    let _ = std::fs::remove_dir_all(&root);
}

// ---- Backup content viewer ----

#[test]
fn backup_contents_from_matches_the_actual_content_of_a_created_backup() {
    let root = std::env::temp_dir().join(format!("{}-test-backup-contents-counts", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let db_path = root.join("staged.db");
    let mut conn = full_schema_db(&db_path, "counts");

    let mut prompts = numbered_prompts(3);
    // Same tag in two different castings: the header count is distinct tags,
    // never distinct spellings.
    prompts[0].tags = vec!["Work".into()];
    prompts[1].tags = vec!["work".into()];
    prompts[2].tags = vec!["Personal".into()];
    db_write_prompts(&mut conn, &prompts, &mut WrittenRows::new()).unwrap();

    let settings = Settings { views: vec![test_view("v1"), test_view("v2")], ..Settings::default() };
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('settings', ?1)",
        params![enc_str(&serde_json::to_string(&settings).unwrap())],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_versions(prompt_id, ts, name, text, variants) VALUES(?1, ?2, 'N', 'T', '')",
        params![prompts[0].id, 1_700_000_000_i64],
    )
    .unwrap();

    let out = backup_contents_from(&conn, &root);
    assert_eq!(out.entries, 3, "entries must count every prompt actually in the snapshot");
    assert_eq!(out.views, 2, "views must come from the snapshot's own settings row");
    assert_eq!(out.tags, 2, "tags must be distinct keys, not distinct castings");
    assert_eq!(out.last_change, Some(1_700_000_000), "the newest edit anywhere must be reported");
    assert_eq!(out.items.len(), 3, "every prompt must appear in the detail list");
    let edited = out.items.iter().find(|i| i.id == prompts[0].id).unwrap();
    assert_eq!(edited.date, Some(1_700_000_000), "an edited prompt must carry its own edit date");
    let untouched = out.items.iter().find(|i| i.id == prompts[1].id).unwrap();
    assert_eq!(untouched.date, None, "a prompt with no version row was never edited");

    drop(conn);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn backup_contents_from_reports_encryption_correctly_and_incorrectly() {
    let _guard = SEAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !sealing_is_randomised() {
        return; // DPAPI unavailable on this machine -- nothing to prove here
    }
    let root = std::env::temp_dir().join(format!("{}-test-backup-contents-encrypted", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    // Correct: every row sealed the normal way.
    let sealed_path = root.join("sealed.db");
    let mut sealed_conn = full_schema_db(&sealed_path, "sealed");
    db_write_prompts(&mut sealed_conn, &numbered_prompts(2), &mut WrittenRows::new()).unwrap();
    let sealed_out = backup_contents_from(&sealed_conn, &root);
    assert!(sealed_out.encrypted, "a normally written snapshot must report itself as encrypted");
    drop(sealed_conn);

    // Incorrect: the rare fallback where DPAPI itself refused, so a row landed
    // in plain text: same case enc_str's own comment describes.
    let prior_sealed = STORE_SEALED.swap(true, Ordering::Relaxed);
    FORCE_PLAINTEXT_FALLBACK.with(|f| f.set(true));
    let _restore = SealOverrideGuard { prior_sealed };
    let plain_path = root.join("plain.db");
    let mut plain_conn = full_schema_db(&plain_path, "plain");
    db_write_prompts(&mut plain_conn, &numbered_prompts(2), &mut WrittenRows::new()).unwrap();
    let plain_out = backup_contents_from(&plain_conn, &root);
    assert!(!plain_out.encrypted, "a snapshot with a plaintext-fallback row must report itself as not encrypted");
    drop(plain_conn);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn restore_selective_data_scope_leaves_settings_untouched() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-data-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-data-staged.db", brand::SLUG));
    {
        let conn = full_schema_db(&target, "target");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('live', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'LIVE-SETTINGS');
                 INSERT INTO meta VALUES('copy_log_migrated', '1');",
        )
        .unwrap();
    }
    {
        let conn = full_schema_db(&staged, "staged");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('backed-up', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'BACKUP-SETTINGS');",
        )
        .unwrap();
    }
    restore_selective(&target, &staged, "data").unwrap();
    let conn = Connection::open(&target).unwrap();
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "backed-up", "data scope must replace the prompts table");
    let settings: String = conn.query_row("SELECT value FROM meta WHERE key='settings'", [], |r| r.get(0)).unwrap();
    assert_eq!(settings, "LIVE-SETTINGS", "data scope must never touch settings");
    let migrated: String = conn.query_row("SELECT value FROM meta WHERE key='copy_log_migrated'", [], |r| r.get(0)).unwrap();
    assert_eq!(migrated, "1", "data scope must never touch the migration marker");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn restore_selective_flushes_the_wal_while_another_connection_is_open() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-wal-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-wal-staged.db", brand::SLUG));
    let wal = dir.join(format!("{}-test-restore-wal-target.db-wal", brand::SLUG));
    let other = {
        let conn = full_schema_db(&target, "target");
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(())).unwrap();
        conn.execute_batch("INSERT INTO prompts VALUES('live', 0, '{}');").unwrap();
        conn
    };
    {
        let conn = full_schema_db(&staged, "staged");
        conn.execute_batch("INSERT INTO prompts VALUES('backed-up', 0, '{}');").unwrap();
    }
    restore_selective(&target, &staged, "data").unwrap();
    // The other connection is still open, so nothing may be left waiting in the WAL.
    let wal_len = std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0);
    assert_eq!(wal_len, 0, "the restored rows must be in the main file, not in the WAL");
    let id: String = other.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(id, "backed-up");
    drop(other);
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.join(format!("{}-test-restore-wal-target.db{ext}", brand::SLUG)));
    }
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn restore_selective_settings_scope_leaves_data_untouched() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-settings-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-settings-staged.db", brand::SLUG));
    {
        let conn = full_schema_db(&target, "target");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('live', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'LIVE-SETTINGS');
                 INSERT INTO meta VALUES('copy_log_migrated', '1');",
        )
        .unwrap();
    }
    {
        let conn = full_schema_db(&staged, "staged");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('backed-up', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'BACKUP-SETTINGS');",
        )
        .unwrap();
    }
    restore_selective(&target, &staged, "settings").unwrap();
    let conn = Connection::open(&target).unwrap();
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "live", "settings scope must never touch prompts");
    let settings: String = conn.query_row("SELECT value FROM meta WHERE key='settings'", [], |r| r.get(0)).unwrap();
    assert_eq!(settings, "BACKUP-SETTINGS", "settings scope must apply the backup's settings");
    let migrated: String = conn.query_row("SELECT value FROM meta WHERE key='copy_log_migrated'", [], |r| r.get(0)).unwrap();
    assert_eq!(migrated, "1", "settings scope must never touch the migration marker");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

// ---- import_all scope ----

pub(super) fn test_prompt(id: &str) -> Prompt {
    serde_json::from_str(&format!(r#"{{"id":"{id}","name":"N","text":"T"}}"#)).unwrap()
}

#[test]
fn dup_length_could_match_uses_the_real_threshold_not_a_fixed_margin() {
    // Equal lengths: the theoretical Dice ceiling is 1.0, always possible.
    assert!(dup_length_could_match(50, 50, 0.99));
    // A 2:1 length ratio caps Dice at 2*50/150 = 0.667: below a 70% floor.
    assert!(!dup_length_could_match(50, 100, 0.70));
    // Just inside: lengths 90 and 100 cap at 2*90/190 = 0.947, above 0.90.
    assert!(dup_length_could_match(90, 100, 0.90));
    assert!(!dup_length_could_match(0, 10, 0.70), "an empty side can never match");
}

#[test]
fn find_similar_prompt_crosses_the_threshold_exactly_at_the_computed_score() {
    let mut a = test_prompt("a");
    a.text = "Please write a short friendly email about the quarterly report".into();
    let mut b = test_prompt("b");
    b.text = "Please write a short friendly email about the quarterly results".into(); // one word differs
    let mut c = test_prompt("c");
    c.text = "Completely different content about something else entirely, not close at all".into();

    // The real score between the near-duplicate pair, computed the same way
    // find_similar_prompt does: so "just above" and "just below" below are
    // exact, not guessed.
    let score = dup_dice(&dup_trigrams(&normalize_dup(&a.text)), &dup_trigrams(&normalize_dup(&b.text)));
    assert!(score > 0.5 && score < 1.0, "fixture must be a realistic near-duplicate, got {score}");

    let others = vec![&b, &c];
    let just_below = (score * 100.0).floor() - 1.0;
    let hit = find_similar_prompt(&a, &others, just_below);
    assert_eq!(hit.map(|h| h.id), Some("b".to_string()), "a threshold just below the real score must still match");

    let just_above = (score * 100.0).ceil() + 1.0;
    let hit2 = find_similar_prompt(&a, &others, just_above);
    assert!(hit2.is_none(), "a threshold just above the real score must find nothing");
}

#[test]
fn find_similar_prompt_ignores_an_unrelated_entry_and_itself() {
    let mut a = test_prompt("a");
    a.text = "Please write a short friendly email about the quarterly report".into();
    let mut c = test_prompt("c");
    c.text = "Completely different content about something else entirely, not close at all".into();
    let hit = find_similar_prompt(&a, &[&c], 70.0);
    assert!(hit.is_none(), "nothing above the threshold exists, so no hit should be reported");
}

#[test]
fn encode_snip_jpeg_refuses_a_side_beyond_sixteen_bits() {
    let tiny = vec![255u8; 2 * 2 * 4];
    assert!(encode_snip_jpeg(&tiny, 2, 2, 90).is_some());
    assert!(encode_snip_jpeg(&tiny, 65_536 + 2, 2, 90).is_none());
    assert!(encode_snip_jpeg(&tiny, 2, 65_536 + 2, 90).is_none());
}

#[test]
fn similar_inputs_leaves_confidential_prompts_out_on_both_sides() {
    let mut a = test_prompt("a");
    a.text = "Please write a short friendly email".into();
    let mut b = test_prompt("b");
    b.text = "Please write a short friendly email today".into();
    let mut s = test_prompt("s");
    s.secret = true;
    s.text = "Please write a short friendly email".into();
    let all = vec![a, b, s];
    let (target, others) = similar_inputs(&all, "a").expect("a plain target");
    assert_eq!(target.id, "a");
    assert_eq!(others.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["b"]);
    assert!(similar_inputs(&all, "s").is_none(), "a confidential target gets no hint");
    assert!(similar_inputs(&all, "missing").is_none());
}

#[test]
fn similar_inputs_copies_only_what_the_comparison_reads() {
    let mut a = test_prompt("a");
    a.text = "Please write a short friendly email".into();
    a.image = "data:image/png;base64,AAAA".into();
    let mut b = test_prompt("b");
    b.name = "Mail".into();
    b.text = "Please write a short friendly email today".into();
    b.image = "data:image/png;base64,BBBB".into();
    b.notes = "a long note".into();
    let all = vec![a, b];
    let (target, others) = similar_inputs(&all, "a").expect("a plain target");
    assert_eq!((target.id.as_str(), target.text.as_str()), ("a", "Please write a short friendly email"));
    assert_eq!((others[0].id.as_str(), others[0].name.as_str()), ("b", "Mail"));
    assert_eq!(others[0].text, "Please write a short friendly email today");
    assert!(target.image.is_empty() && others[0].image.is_empty(), "image data must stay out of the copy");
    assert!(others[0].notes.is_empty());
}

#[test]
fn stats_display_name_never_falls_back_to_the_text_of_a_confidential_prompt() {
    let mut p = test_prompt("p1");
    p.name = String::new();
    p.text = "plain body".into();
    assert_eq!(stats_display_name(&p), "plain body");
    p.secret = true;
    assert_eq!(stats_display_name(&p), "p1");
    p.name = "Named".into();
    assert_eq!(stats_display_name(&p), "Named");
}

#[test]
fn find_similar_prompt_stays_within_budget_at_ten_thousand_entries() {
    let mut target = test_prompt("target");
    target.text = "Please write a short friendly email about the quarterly report".into();
    let others: Vec<Prompt> = (0..10_000)
        .map(|i| {
            let mut p = test_prompt(&format!("o{i}"));
            p.text = format!("Unrelated filler content number {i} that shares nothing meaningful with the target text at all");
            p
        })
        .collect();
    let refs: Vec<&Prompt> = others.iter().collect();
    let start = std::time::Instant::now();
    let hit = find_similar_prompt(&target, &refs, 85.0);
    let elapsed = start.elapsed();
    assert!(hit.is_none(), "none of the filler text should cross an 85% threshold");
    assert!(elapsed < std::time::Duration::from_millis(200), "10,000-entry scan took {elapsed:?}, must stay well under a saving-feels-slow margin");
}

#[test]
fn dismiss_example_prompts_keeps_only_the_ones_the_user_never_touched() {
    let mut a = test_prompt("a");
    let mut b = test_prompt("b");
    let c = test_prompt("c");
    let mut seeds = HashMap::new();
    for p in [&mut a, &mut b] {
        seeds.insert(p.id.clone(), example_seed_fingerprint(&p.name, &p.text, &p.image));
    }
    seeds.insert(c.id.clone(), example_seed_fingerprint(&c.name, &c.text, &c.image));
    b.text = "the user's own rewrite".to_string(); // edited after seeding
    let ids = vec![a.id.clone(), b.id.clone(), c.id.clone()];
    let prompts = vec![a, b, c];

    let pristine = examples_still_pristine(&ids, &seeds, &prompts);

    assert_eq!(pristine, vec!["a".to_string(), "c".to_string()], "the edited one must be excluded, order preserved");
}

#[test]
fn dismiss_example_prompts_treats_a_missing_seed_as_unchanged() {
    // Pre-421 installs have example_prompt_ids but no example_prompt_seeds entry
    // for them: must still be eligible for removal, matching old behaviour.
    let p = test_prompt("legacy");
    let ids = vec![p.id.clone()];
    let pristine = examples_still_pristine(&ids, &HashMap::new(), &[p]);
    assert_eq!(pristine, vec!["legacy".to_string()]);
}

#[test]
fn import_all_data_scope_leaves_settings_untouched() {
    let mut store = Store {
        prompts: vec![test_prompt("live")],
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: true, // must be cleared once data is actually replaced
        written: Default::default(),
    };
    store.settings.ui_texts.insert("marker".into(), "LIVE".into());
    let mut backup_settings = Settings::default();
    backup_settings.ui_texts.insert("marker".into(), "BACKUP".into());
    let backup = Backup { prompts: vec![test_prompt("backed-up")], settings: backup_settings };

    let (count, dropped, pw_dropped) = apply_import(&mut store, backup, true, false);

    assert_eq!(store.prompts.len(), 1, "data scope must replace the prompts list");
    assert_eq!(store.prompts[0].id, "backed-up");
    assert_eq!(count, 1);
    assert_eq!(dropped, 0);
    assert!(!pw_dropped, "settings were never touched, so no backup password can have been dropped");
    assert!(!store.prompts_locked, "a full data import supersedes any decrypt lock");
    assert_eq!(
        store.settings.ui_texts.get("marker").map(String::as_str),
        Some("LIVE"),
        "data scope must never touch settings"
    );
}

// An import taken before a prompt was deleted brings it back as a live prompt: the trash must
// not keep a second entry with that id, or restoring it and deleting it again both collide.
#[test]
fn an_import_that_brings_back_a_trashed_prompt_drops_its_trash_entry() {
    let mut store = Store {
        prompts: vec![test_prompt("live")],
        trash: vec![test_prompt("back"), test_prompt("stays")],
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    };
    let backup = Backup { prompts: vec![test_prompt("back")], settings: Settings::default() };
    apply_import(&mut store, backup, true, false);
    let trash_ids: Vec<&str> = store.trash.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(trash_ids, vec!["stays"], "only the entry that came back as a live prompt leaves the trash");

    // A settings-only import leaves prompts and trash as they are.
    let backup = Backup { prompts: vec![test_prompt("stays")], settings: Settings::default() };
    apply_import(&mut store, backup, false, true);
    assert_eq!(store.trash.len(), 1);
}

// A store already holding the same id in both lists (from such an import) still restores.
#[test]
fn restoring_a_trash_entry_whose_id_is_live_gives_it_a_new_id() {
    let migrated_settings = || {
        let mut s = Settings::default();
        s.migrate();
        s
    };
    let mut store = Store {
        prompts: vec![test_prompt("x")],
        trash: vec![test_prompt("x")],
        settings: migrated_settings(),
        prompts_locked: false,
        written: Default::default(),
    };
    restore_trashed_core(&mut store, "x", |st| {
        let mut ids: Vec<&str> = st.prompts.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        ids.len() == st.prompts.len()
    })
    .expect("the restore must not collide with the live prompt");
    assert_eq!(store.prompts.len(), 2);
    assert!(store.trash.is_empty());

    // A failed write puts the entry back under its own id.
    let mut store = Store {
        prompts: vec![test_prompt("x")],
        trash: vec![test_prompt("x")],
        settings: migrated_settings(),
        prompts_locked: false,
        written: Default::default(),
    };
    assert!(restore_trashed_core(&mut store, "x", |_| false).is_err());
    assert_eq!(store.trash[0].id, "x");
    assert_eq!(store.prompts.len(), 1);
}

#[test]
fn import_all_settings_scope_leaves_data_untouched() {
    let mut store = Store {
        prompts: vec![test_prompt("live")],
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    };
    store.settings.ui_texts.insert("marker".into(), "LIVE".into());
    let mut backup_settings = Settings::default();
    backup_settings.ui_texts.insert("marker".into(), "BACKUP".into());
    backup_settings.ui_texts.insert(BACKUP_PW_KEY.into(), "foreign-pw".into());
    let backup = Backup { prompts: vec![test_prompt("backed-up")], settings: backup_settings };

    let (count, dropped, pw_dropped) = apply_import(&mut store, backup, false, true);

    assert_eq!(store.prompts.len(), 1, "settings scope must never touch prompts");
    assert_eq!(store.prompts[0].id, "live");
    assert_eq!(count, 1);
    assert_eq!(dropped, 0);
    assert!(pw_dropped, "the backup carried its own backup password, which must be reported as dropped");
    assert_eq!(
        store.settings.ui_texts.get("marker").map(String::as_str),
        Some("BACKUP"),
        "settings scope must apply the backup's settings"
    );
    assert!(
        !store.settings.ui_texts.contains_key(BACKUP_PW_KEY),
        "the foreign backup password must not survive - this machine had none to restore"
    );
}

// Finding 1 (security pass 348): a full backup restore used to overwrite
// the folder-authority settings outright, so a hostile backup file could
// point videoExportDir/recordingDir/screenshotDir/syncDir at any folder it
// liked and have export/cleanup/sync commands trust it from then on. This
// machine's own folders must survive a settings-scope import exactly like
// the backup password above.
#[test]
fn import_all_settings_scope_keeps_this_machines_own_folder_settings() {
    let mut store = Store {
        prompts: vec![test_prompt("live")],
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: Default::default(),
    };
    store.settings.ui_texts.insert("recordingDir".into(), "D:\\mine\\recordings".into());
    store.settings.ui_texts.insert("videoExportDir".into(), "D:\\mine\\exports".into());
    store.settings.ui_texts.insert("screenshotDir".into(), "D:\\mine\\shots".into());
    store.settings.ui_texts.insert("syncDir".into(), "D:\\mine\\sync".into());
    let mut backup_settings = Settings::default();
    backup_settings.ui_texts.insert("recordingDir".into(), "C:\\Windows\\System32".into());
    backup_settings.ui_texts.insert("videoExportDir".into(), "C:\\Windows\\System32".into());
    backup_settings.ui_texts.insert("screenshotDir".into(), "C:\\Windows\\System32".into());
    backup_settings.ui_texts.insert("syncDir".into(), "C:\\Windows\\System32".into());
    backup_settings.ui_texts.insert("marker".into(), "BACKUP".into());
    let backup = Backup { prompts: vec![test_prompt("backed-up")], settings: backup_settings };

    apply_import(&mut store, backup, false, true);

    assert_eq!(
        store.settings.ui_texts.get("marker").map(String::as_str),
        Some("BACKUP"),
        "an ordinary setting must still come from the backup"
    );
    for (key, own_value) in [
        ("recordingDir", "D:\\mine\\recordings"),
        ("videoExportDir", "D:\\mine\\exports"),
        ("screenshotDir", "D:\\mine\\shots"),
        ("syncDir", "D:\\mine\\sync"),
    ] {
        assert_eq!(
            store.settings.ui_texts.get(key).map(String::as_str),
            Some(own_value),
            "{key} must keep this machine's own folder, never the backup's",
        );
    }
}

// Finding 1 (security pass 348): set_ui_text is the generic string setter
// every ordinary expert-menu row uses: a folder-authority key must never
// be reachable through it, only through its own dedicated command.
#[test]
fn is_protected_folder_key_covers_every_folder_setting_and_nothing_else() {
    assert!(is_protected_folder_key("screenshotDir"));
    assert!(is_protected_folder_key("videoExportDir"));
    assert!(is_protected_folder_key("recordingDir"));
    assert!(is_protected_folder_key("syncDir"));
    assert!(!is_protected_folder_key("copyFont"), "an ordinary string setting must stay reachable");
    assert!(!is_protected_folder_key("dataDir"), "dataDir has its own separate command and storage, not a ui_texts key");
}

// The editor writes the last tool size and tool a moment after every change; those two rows
// are read by the editor alone, so they must not broadcast a settings change to the float windows.
#[test]
fn editor_only_text_rows_do_not_notify_the_float_windows() {
    assert!(!text_write_notifies("snipToolSizes"));
    assert!(!text_write_notifies("snipLastTool"));
    assert!(text_write_notifies("copyFont"), "an ordinary text setting still notifies");
    assert!(text_write_notifies("snipTextBack"), "only the two remembered-state rows are quiet");
}

#[test]
fn outside_settings_that_switch_ocr_index_off_erase_the_picture_text() {
    let _epoch = OCR_EPOCH_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = Store { prompts: vec![sync_test_prompt()], trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() };
    store.prompts[0].image_text = "read from a picture".into();
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    assert!(!clear_ocr_text_if_switched_off(true, &mut store), "still on: nothing to erase");
    assert!(!store.prompts[0].image_text.is_empty());
    store.settings.ui_flags.insert("ocrIndex".into(), false);
    assert!(!clear_ocr_text_if_switched_off(false, &mut store), "was already off: left alone");
    assert!(clear_ocr_text_if_switched_off(true, &mut store));
    assert!(store.prompts[0].image_text.is_empty());
}

#[test]
fn a_full_import_that_switches_ocr_index_off_erases_prompt_and_trash_text() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = Store { prompts: vec![sync_test_prompt()], trash: vec![sync_test_prompt()], settings: Settings::default(), prompts_locked: false, written: Default::default() };
    store.prompts[0].image_text = "live".into();
    store.trash[0].image_text = "trashed".into();
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    let mut incoming = import_test_backup();
    incoming.prompts[0].image_text = "kept by data".into();
    let mut wrote_prompts = false;
    run_import(&mut store, incoming, false, true, |_, prompts, _| {
        wrote_prompts = prompts;
        true
    })
    .unwrap();
    assert!(wrote_prompts, "cleared prompt rows must be persisted");
    assert!(store.prompts.iter().all(|p| p.image_text.is_empty()));
    assert!(store.trash.iter().all(|p| p.image_text.is_empty()));
    // A data-only import leaves the switch alone and the text with it.
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    store.trash[0].image_text = "trashed".into();
    run_import(&mut store, import_test_backup(), true, false, |_, _, _| true).unwrap();
    assert_eq!(store.trash[0].image_text, "trashed");
}

#[test]
fn the_generic_text_setter_refuses_every_row_with_a_dedicated_writer() {
    for k in ["remoteAllowedRanges", "remotePasswordHash", "backupPw", "secretVaultSalt", "secretVaultVerify", "remoteTlsKey", "screenshotDir", "syncDir"] {
        assert!(is_generic_text_write_denied(k), "{k} must not be writable generically");
    }
    for k in ["copyFont", "palette", "librarySort", "recentSearches", "globalSearchRecent"] {
        assert!(!is_generic_text_write_denied(k), "{k} stays reachable");
    }
}

// Finding 1 (security pass 348): a settings-only import file is untrusted
// (it could be a hostile file, not necessarily the user's own export) --
// its folder-authority rows must be dropped before the diff/apply ever see
// them, so neither preview nor apply can be talked into changing them.
#[test]
fn strip_denied_import_keys_removes_every_folder_setting_and_nothing_else() {
    let mut incoming = SettingsOnlyExport::default();
    incoming.ui_texts.insert("screenshotDir".into(), "C:\\evil".into());
    incoming.ui_texts.insert("videoExportDir".into(), "C:\\evil".into());
    incoming.ui_texts.insert("recordingDir".into(), "C:\\evil".into());
    incoming.ui_texts.insert("syncDir".into(), "C:\\evil".into());
    incoming.ui_texts.insert("copyFont".into(), "Consolas".into());

    strip_denied_import_keys(&mut incoming);

    for key in PROTECTED_FOLDER_KEYS {
        assert!(!incoming.ui_texts.contains_key(key), "{key} must be stripped from an incoming settings file");
    }
    assert_eq!(incoming.ui_texts.get("copyFont").map(String::as_str), Some("Consolas"), "an ordinary setting must survive untouched");
}

const OLD_REMOTE_SECRETS: [&str; 4] = ["remotePasswordHash", "remoteTlsKey", "remoteTlsCert", "remoteTlsCa"];

#[test]
fn settings_only_payload_omits_every_leftover_receiver_secret() {
    let mut s = Settings::default();
    for k in OLD_REMOTE_SECRETS {
        s.ui_texts.insert(k.to_string(), "value".into());
    }
    s.ui_texts.insert("copyFont".into(), "Consolas".into());
    let payload = settings_only_payload(&s);
    for k in OLD_REMOTE_SECRETS {
        assert!(!payload.ui_texts.contains_key(k), "{k} must not be exported");
    }
    assert_eq!(payload.ui_texts.get("copyFont").map(String::as_str), Some("Consolas"));
    let json = serde_json::to_string(&payload).unwrap();
    assert!(!json.contains("remoteTls") && !json.contains("remotePassword"));
    let mut incoming = payload;
    incoming.ui_texts.insert("remoteTlsKey".into(), "planted".into());
    strip_denied_import_keys(&mut incoming);
    assert!(!incoming.ui_texts.contains_key("remoteTlsKey"));
}

#[test]
fn strip_remote_secrets_removes_exactly_the_four_secret_rows() {
    let mut texts = std::collections::HashMap::new();
    for k in OLD_REMOTE_SECRETS {
        texts.insert(k.to_string(), "value".to_string());
    }
    texts.insert("copyFont".into(), "Consolas".into());
    texts.insert(BACKUP_PW_KEY.into(), "sealed".into());
    strip_remote_secrets(&mut texts);
    let mut left: Vec<&str> = texts.keys().map(String::as_str).collect();
    left.sort_unstable();
    let mut want = vec!["copyFont", BACKUP_PW_KEY];
    want.sort_unstable();
    assert_eq!(left, want, "only the leftover receiver secrets go; every other row stays");
    for k in OLD_REMOTE_SECRETS {
        assert!(is_remote_secret_key(k));
    }
    assert!(!is_remote_secret_key("remotePort") && !is_remote_secret_key("copyFont"));
}

#[test]
fn a_full_import_never_brings_in_a_foreign_receiver_secret() {
    let mut store = Store { prompts: vec![], trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() };
    let mut foreign = Settings::default();
    for k in OLD_REMOTE_SECRETS {
        foreign.ui_texts.insert(k.to_string(), "theirs".into());
    }
    let backup = Backup { prompts: vec![], settings: foreign };
    apply_import(&mut store, backup, false, true);
    for k in OLD_REMOTE_SECRETS {
        assert!(!store.settings.ui_texts.contains_key(k), "{k} must not arrive from a file");
    }
}

// A hostile file plants every key that could open a listener, register
// with the OS or swap a secret. A full import keeps this machine's own
// value for each of them (or the absence of one).
pub(super) fn hostile_settings() -> Settings {
    let mut s = Settings { autostart: true, ..Settings::default() };
    for k in ["remoteApi", "remoteNetworkFilter", "remoteTlsShortLived", "urlScheme", "urlSchemeOpenAllowed", "urlSchemeCopyAllowed", "demoMode", "allowMultipleInstances", "promptSync"] {
        s.ui_flags.insert(k.into(), true);
    }
    s.ui_flags.insert("copyFlash".into(), false);
    s.ui_values.insert("remotePort".into(), 4444.0);
    s.ui_values.insert("remoteRateLimit".into(), 0.0);
    s.ui_values.insert("tileGap".into(), 7.0);
    for k in ["remotePasswordHash", "remoteAllowedRanges", BACKUP_PW_KEY, "screenshotDir"] {
        s.ui_texts.insert(k.into(), "planted".into());
    }
    for k in ["remoteTlsKey", "remoteTlsCert", "remoteTlsCa"] {
        s.ui_texts.insert(k.to_string(), "planted".into());
    }
    s.ui_texts.insert("copyFont".into(), "Consolas".into());
    s
}

// The three switches that grant consent or guard the data folder: a file,
// backup or snapshot says whatever it likes, this machine keeps its own.
pub(super) const CONSENT_KEYS: [&str; 3] = ["voiceOnlineOk", "dataDirLock", "clipSkipSecrets"];

pub(super) fn local_consent_settings() -> Settings {
    let mut s = Settings::default();
    s.ui_flags.insert("voiceOnlineOk".into(), false);
    s.ui_flags.insert("dataDirLock".into(), true);
    s.ui_flags.insert("clipSkipSecrets".into(), true);
    s
}

pub(super) fn hostile_consent_settings() -> Settings {
    let mut s = Settings::default();
    s.ui_flags.insert("voiceOnlineOk".into(), true);
    s.ui_flags.insert("dataDirLock".into(), false);
    s.ui_flags.insert("clipSkipSecrets".into(), false);
    s.ui_flags.insert("copyFlash".into(), false);
    s
}

pub(super) fn assert_consent_kept(s: &Settings, what: &str) {
    assert_eq!(s.ui_flags.get("voiceOnlineOk"), Some(&false), "{what}: voiceOnlineOk");
    assert_eq!(s.ui_flags.get("dataDirLock"), Some(&true), "{what}: dataDirLock");
    assert_eq!(s.ui_flags.get("clipSkipSecrets"), Some(&true), "{what}: clipSkipSecrets");
}

#[test]
fn a_settings_file_cannot_set_consent_or_lock_switches() {
    let mut live = local_consent_settings();
    let mut file = SettingsOnlyExport::default();
    for k in CONSENT_KEYS {
        file.ui_flags.insert(k.into(), k == "voiceOnlineOk");
    }
    file.ui_flags.insert("copyFlash".into(), false);
    let skipped = strip_denied_import_keys(&mut file);
    assert_eq!(skipped, 3, "the three rows count as skipped");
    merge_settings_only(&mut live, file, |_| true).unwrap();
    assert_consent_kept(&live, "settings-only import");
    assert_eq!(live.ui_flags.get("copyFlash"), Some(&false), "ordinary rows still apply");
}

#[test]
fn a_full_import_cannot_set_consent_or_lock_switches() {
    let mut store = Store { prompts: vec![], trash: Vec::new(), settings: local_consent_settings(), prompts_locked: false, written: Default::default() };
    apply_import(&mut store, Backup { prompts: vec![], settings: hostile_consent_settings() }, false, true);
    assert_consent_kept(&store.settings, "full import");
    assert_eq!(store.settings.ui_flags.get("copyFlash"), Some(&false));
    // A machine that never had the rows does not receive them either.
    let mut bare = Store { prompts: vec![], trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() };
    apply_import(&mut bare, Backup { prompts: vec![], settings: hostile_consent_settings() }, false, true);
    for k in CONSENT_KEYS {
        assert!(!bare.settings.ui_flags.contains_key(k), "{k} must not arrive");
    }
}

pub(super) fn consent_restore_pair(tag: &str, local: &Settings, incoming: Option<&Settings>) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-consent-{tag}-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-consent-{tag}-staged.db", brand::SLUG));
    let t = full_schema_db(&target, "target");
    db_write_settings(&t, local).unwrap();
    let s = full_schema_db(&staged, "staged");
    if let Some(i) = incoming {
        db_write_settings(&s, i).unwrap();
    }
    (target, staged)
}

#[test]
fn a_settings_restore_cannot_set_consent_or_lock_switches() {
    let (target, staged) = consent_restore_pair("sel", &local_consent_settings(), Some(&hostile_consent_settings()));
    restore_selective(&target, &staged, "settings").unwrap();
    let after = read_settings_row(&Connection::open(&target).unwrap(), "main").unwrap();
    assert_consent_kept(&after, "settings-scope restore");
    assert_eq!(after.ui_flags.get("copyFlash"), Some(&false));
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn a_whole_snapshot_restore_cannot_set_consent_or_lock_switches() {
    // Shared by the in-app restore and the command-line standalone restore.
    let (target, staged) = consent_restore_pair("all", &local_consent_settings(), Some(&hostile_consent_settings()));
    keep_machine_in_staged(&target, &staged).unwrap();
    let after = read_settings_row(&Connection::open(&staged).unwrap(), "main").unwrap();
    assert_consent_kept(&after, "whole-snapshot restore");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn a_whole_snapshot_restore_onto_a_machine_without_settings_drops_the_switches() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-consent-none-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-consent-none-staged.db", brand::SLUG));
    let _ = full_schema_db(&target, "target");
    let s = full_schema_db(&staged, "staged");
    db_write_settings(&s, &hostile_consent_settings()).unwrap();
    drop(s);
    keep_machine_in_staged(&target, &staged).unwrap();
    let after = read_settings_row(&Connection::open(&staged).unwrap(), "main").unwrap();
    for k in CONSENT_KEYS {
        assert!(!after.ui_flags.contains_key(k), "{k} must not arrive");
    }
    assert_eq!(after.ui_flags.get("copyFlash"), Some(&false));
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn a_settings_restore_onto_a_machine_without_settings_drops_the_switches() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-consent-selnone-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-consent-selnone-staged.db", brand::SLUG));
    let t = full_schema_db(&target, "target");
    drop(t);
    let s = full_schema_db(&staged, "staged");
    let mut hostile = hostile_consent_settings();
    hostile.ui_flags.insert("remoteApi".into(), true);
    hostile.autostart = true;
    db_write_settings(&s, &hostile).unwrap();
    drop(s);
    restore_selective(&target, &staged, "settings").unwrap();
    let after = read_settings_row(&Connection::open(&target).unwrap(), "main").unwrap();
    for k in CONSENT_KEYS.iter().chain(["remoteApi"].iter()) {
        assert!(!after.ui_flags.contains_key(*k), "{k} must not arrive");
    }
    assert!(!after.autostart, "a restore must not register autostart on a bare machine");
    assert_eq!(after.ui_flags.get("copyFlash"), Some(&false));
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn a_snapshot_restore_onto_a_machine_without_settings_drops_the_switches() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-consent-stagednone-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-consent-stagednone-staged.db", brand::SLUG));
    let _ = std::fs::remove_file(&target);
    let s = full_schema_db(&staged, "staged");
    let mut hostile = hostile_consent_settings();
    hostile.autostart = true;
    db_write_settings(&s, &hostile).unwrap();
    drop(s);
    keep_machine_in_staged(&target, &staged).unwrap();
    let after = read_settings_row(&Connection::open(&staged).unwrap(), "main").unwrap();
    for k in CONSENT_KEYS {
        assert!(!after.ui_flags.contains_key(k), "{k} must not arrive");
    }
    assert!(!after.autostart, "a restore must not register autostart on a bare machine");
    assert_eq!(after.ui_flags.get("copyFlash"), Some(&false));
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn the_cli_refusal_and_the_import_denylist_are_one_list() {
    for k in CONSENT_KEYS {
        assert!(is_import_denied_flag(k), "{k} must be on the import denylist");
    }
}

#[test]
fn a_full_import_cannot_plant_network_or_machine_settings() {
    let mut store = Store { prompts: vec![], trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() };
    store.settings.ui_flags.insert("remoteApi".into(), false);
    store.settings.ui_texts.insert("remotePasswordHash".into(), "mine".into());
    let backup = Backup { prompts: vec![], settings: hostile_settings() };
    apply_import(&mut store, backup, false, true);
    let s = &store.settings;
    for k in ["remoteNetworkFilter", "remoteTlsShortLived", "urlScheme", "urlSchemeOpenAllowed", "urlSchemeCopyAllowed", "demoMode", "allowMultipleInstances", "promptSync"] {
        assert!(!s.ui_flags.contains_key(k), "{k} must not be imported");
    }
    assert_eq!(s.ui_flags.get("remoteApi"), Some(&false), "this machine's own value stays");
    assert_eq!(s.ui_texts.get("remotePasswordHash").map(String::as_str), Some("mine"));
    assert!(!s.ui_texts.contains_key("remoteAllowedRanges"));
    assert!(!s.ui_values.contains_key("remotePort") && !s.ui_values.contains_key("remoteRateLimit"));
    assert!(!s.autostart, "a file cannot switch the autostart registration on");
    assert_eq!(s.ui_values.get("tileGap"), Some(&7.0), "ordinary settings still arrive");
    assert_eq!(s.ui_texts.get("copyFont").map(String::as_str), Some("Consolas"));
    assert_eq!(s.ui_flags.get("copyFlash"), Some(&false));
}

#[test]
fn a_settings_file_previews_every_denied_key_as_skipped_and_changes_none() {
    let hostile = hostile_settings();
    let mut incoming = SettingsOnlyExport { ui_flags: hostile.ui_flags.clone(), ui_values: hostile.ui_values.clone(), ui_texts: hostile.ui_texts.clone() };
    incoming.ui_texts.insert(SECRET_SALT_KEY.into(), "foreign".into());
    incoming.ui_texts.insert(SECRET_VERIFY_KEY.into(), "foreign".into());
    let skipped = strip_denied_import_keys(&mut incoming);
    // 9 flags, 2 values, 4 texts + 3 certificate values + 2 vault keys
    assert_eq!(skipped, 9 + 2 + 4 + 3, "the vault rows are dropped silently");
    assert_eq!(incoming.ui_flags.keys().collect::<Vec<_>>(), vec!["copyFlash"]);
    assert_eq!(incoming.ui_values.keys().collect::<Vec<_>>(), vec!["tileGap"]);
    assert_eq!(incoming.ui_texts.keys().collect::<Vec<_>>(), vec!["copyFont"]);
    let mut live = Settings::default();
    live.ui_flags.insert("remoteApi".into(), false);
    let mut again = SettingsOnlyExport { ui_flags: hostile.ui_flags, ui_values: hostile.ui_values, ui_texts: hostile.ui_texts };
    strip_denied_import_keys(&mut again);
    let _ = merge_settings_only(&mut live, again, |_| true).unwrap();
    let after: serde_json::Value = serde_json::from_str(&serde_json::to_string(&live).unwrap()).unwrap();
    assert_eq!(after["ui_flags"]["remoteApi"], serde_json::json!(false));
    assert!(after["ui_flags"].get("remoteNetworkFilter").is_none());
    assert!(after["ui_texts"].get("remotePasswordHash").is_none());
}

pub(super) fn theme_file(theme_gap: f64) -> SettingsOnlyExport {
    let mut f = SettingsOnlyExport::default();
    f.ui_values.insert("tileGap".into(), theme_gap);
    f.ui_flags.insert("trayRecent".into(), true);
    f.ui_texts.insert("copyFont".into(), "Consolas".into());
    f
}

#[test]
fn a_settings_import_that_cannot_be_saved_changes_nothing() {
    let mut live = Settings::default();
    live.ui_values.insert("tileGap".into(), 3.0);
    let before = serde_json::to_string(&live).unwrap();
    let mut saw = 0;
    let r = merge_settings_only(&mut live, theme_file(9.0), |_| {
        saw += 1;
        false
    });
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(saw, 1, "exactly one write was attempted");
    assert_eq!(serde_json::to_string(&live).unwrap(), before, "memory must equal what disk still holds");
}

#[test]
fn a_saved_settings_import_is_what_was_written_and_survives_a_reload() {
    let mut live = Settings::default();
    live.ui_values.insert("tileGap".into(), 3.0);
    live.ui_texts.insert("untouched".into(), "keep".into());
    let mut written = None;
    let changed = merge_settings_only(&mut live, theme_file(9.0), |s| {
        written = Some(serde_json::to_string(s).unwrap());
        true
    })
    .unwrap();
    assert_eq!(changed, 3);
    assert_eq!(written.as_deref(), Some(serde_json::to_string(&live).unwrap().as_str()), "what was saved is what is live");
    let reloaded: Settings = serde_json::from_str(&written.unwrap()).unwrap();
    assert_eq!(reloaded.ui_values.get("tileGap"), Some(&9.0));
    assert_eq!(reloaded.ui_flags.get("trayRecent"), Some(&true));
    assert_eq!(reloaded.ui_texts.get("untouched").map(String::as_str), Some("keep"), "a row the file never mentions stays");
    assert!(capture_excluded(&reloaded) == capture_excluded(&live), "the capture state follows the same settings");
}

// ---- a full import commits both halves together or neither ----

// Serializes the tests that set or read the corrupt-row lock flag.
pub(super) static SETTINGS_LOCK_TEST: Mutex<()> = Mutex::new(());

pub(super) fn import_test_db() -> Connection {
    let conn = row_test_db();
    conn.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);").unwrap();
    conn
}

// A store whose disk holds exactly its prompts "old1", "old2" and a settings
// row marked "OLD".
pub(super) fn import_test_store(conn: &mut Connection) -> Store {
    let mut settings = Settings::default();
    settings.ui_texts.insert("marker".into(), "OLD".into());
    let store = Store {
        prompts: vec![test_prompt("old1"), test_prompt("old2")],
        trash: Vec::new(),
        settings,
        prompts_locked: false,
        written: Default::default(),
    };
    db_write_prompts(conn, &store.prompts, &mut store.written.borrow_mut()).unwrap();
    db_write_settings(conn, &store.settings).unwrap();
    store
}

pub(super) fn import_test_backup() -> Backup {
    let mut settings = Settings::default();
    settings.ui_texts.insert("marker".into(), "NEW".into());
    Backup { prompts: vec![test_prompt("new1")], settings }
}

pub(super) fn disk_state(conn: &Connection) -> (Vec<String>, String) {
    let (prompts, settings, _, _, _) = db_load(conn, &mut WrittenRows::new());
    let marker = settings.and_then(|s| s.ui_texts.get("marker").cloned()).unwrap_or_default();
    (prompts.iter().map(|p| p.id.clone()).collect(), marker)
}

pub(super) fn import_with(conn: &mut Connection, store: &mut Store, want_data: bool, want_settings: bool) -> Result<(usize, usize, bool, bool), String> {
    run_import(store, import_test_backup(), want_data, want_settings, |st, prompts, settings| {
        db_write_import(conn, st, &st.prompts, prompts, settings, false).is_ok()
    })
}

#[test]
fn imported_rows_never_keep_a_network_path() {
    let unc = "\\\\host.example\\share\\x.mp4".to_string();
    let verbatim = "\\\\?\\UNC\\host.example\\share\\x.png".to_string();
    assert!(is_network_path(&unc));
    assert!(is_network_path("//host.example/share/x.mp4"));
    assert!(is_network_path(&verbatim));
    assert!(!is_network_path("C:/Users/x/clip.mp4"));
    assert!(!is_network_path(""));
    assert!(!is_network_path("assets/demo.png"));

    let mut store = Store { prompts: Vec::new(), trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() };
    let item = ImportedPrompt {
        name: "Pack".to_string(),
        text: "body".to_string(),
        color: String::new(),
        font: String::new(),
        font_size: 0,
        file_path: unc.clone(),
        pdf_page: 0,
        icon_path: "//host.example/share/i.mp4".to_string(),
        caption: String::new(),
        caption_size: 0,
        show_image: false,
        copy_image: false,
        image: verbatim.clone(),
        tags: Vec::new(),
        notes: String::new(),
        variants: Vec::new(),
        rotate_mode: String::new(),
        favorite: false,
        secret: false,
    };
    assert_eq!(append_imported_prompts(&mut store, vec![item]), 1);
    let p = &store.prompts[0];
    assert!(p.file_path.is_empty() && p.icon_path.is_empty() && p.image.is_empty());
    assert_eq!(p.name, "Pack", "the row itself stays");

    let mut hostile = test_prompt("h");
    hostile.file_path = unc;
    hostile.icon_path = "C:/ok/local.mp4".to_string();
    let backup = Backup { prompts: vec![hostile], settings: Settings::default() };
    apply_import(&mut store, backup, true, false);
    assert!(store.prompts[0].file_path.is_empty(), "a full backup is cleared too");
    assert_eq!(store.prompts[0].icon_path, "C:/ok/local.mp4");
}

#[test]
fn a_wiped_library_leaves_no_old_row_bytes_in_the_database_or_its_log() {
    let dir = std::env::temp_dir().join(format!("ps-wipe-scrub-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let db = dir.join("data.db");
    let conn = Connection::open(&db).unwrap();
    conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(())).unwrap();
    conn.execute_batch("CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);").unwrap();
    let marker = "ZQXJ-confidential-marker-ZQXJ";
    for i in 0..40 {
        conn.execute("INSERT INTO prompts VALUES (?1, ?1, ?2)", rusqlite::params![i, format!("{marker} {i}")]).unwrap();
    }
    conn.execute("DELETE FROM prompts", []).unwrap();
    let holds_marker = || {
        ["data.db", "data.db-wal"].iter().any(|n| {
            fs::read(dir.join(n)).map(|b| b.windows(marker.len()).any(|w| w == marker.as_bytes())).unwrap_or(false)
        })
    };
    assert!(holds_marker(), "the fixture must show the leftover first");
    assert!(scrub_freed_pages(&conn));
    assert!(!holds_marker(), "deleted rows must not survive in free pages or the log");
    drop(conn);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_remove_examples_count_skips_examples_already_deleted_by_hand() {
    let a = test_prompt("a");
    let b = test_prompt("b");
    let ids = vec!["a".to_string(), "b".to_string(), "gone".to_string()];
    let prompts = vec![a, b];
    assert_eq!(examples_pending_ids(&ids, &HashMap::new(), &prompts), vec!["a".to_string(), "b".to_string()]);
    // dismiss itself still sees the missing id, which is harmless there
    assert_eq!(examples_still_pristine(&ids, &HashMap::new(), &prompts).len(), 3);
}
