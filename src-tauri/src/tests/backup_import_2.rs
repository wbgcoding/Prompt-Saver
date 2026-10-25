use super::*;

#[test]
fn a_full_import_that_fails_on_the_settings_write_leaves_everything_as_it_was() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut conn = import_test_db();
    let mut store = import_test_store(&mut conn);
    let before_prompts = serde_json::to_string(&store.prompts).unwrap();
    let before_settings = serde_json::to_string(&store.settings).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_settings BEFORE INSERT ON meta BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    SETTINGS_LOCKED.store(true, Ordering::Relaxed);
    store.prompts_locked = true;

    let r = import_with(&mut conn, &mut store, true, true);

    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(serde_json::to_string(&store.prompts).unwrap(), before_prompts, "memory keeps the old prompts");
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before_settings, "memory keeps the old settings");
    assert!(store.prompts_locked, "the decrypt lock is put back");
    assert!(SETTINGS_LOCKED.load(Ordering::Relaxed), "the corrupt-row lock is put back");
    SETTINGS_LOCKED.store(false, Ordering::Relaxed);
    let (ids, marker) = disk_state(&conn);
    assert_eq!(ids, vec!["old1".to_string(), "old2".to_string()], "the prompt half must not have committed");
    assert_eq!(marker, "OLD");
}

#[test]
fn a_full_import_that_fails_on_the_prompt_write_leaves_everything_as_it_was() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut conn = import_test_db();
    let mut store = import_test_store(&mut conn);
    let before_prompts = serde_json::to_string(&store.prompts).unwrap();
    let before_settings = serde_json::to_string(&store.settings).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_prompts BEFORE INSERT ON prompts BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();

    let r = import_with(&mut conn, &mut store, true, true);

    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(serde_json::to_string(&store.prompts).unwrap(), before_prompts);
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before_settings);
    let (ids, marker) = disk_state(&conn);
    assert_eq!(ids, vec!["old1".to_string(), "old2".to_string()]);
    assert_eq!(marker, "OLD", "the settings half must not have committed");
}

#[test]
fn a_full_import_commits_both_halves_and_a_reopened_store_agrees() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut conn = import_test_db();
    let mut store = import_test_store(&mut conn);
    let (count, _, _, _) = import_with(&mut conn, &mut store, true, true).unwrap();
    assert_eq!(count, 1);
    assert_eq!(store.prompts.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["new1"]);
    let (ids, marker) = disk_state(&conn);
    assert_eq!(ids, vec!["new1".to_string()], "the replaced prompts are gone from disk too");
    assert_eq!(marker, "NEW");
}

#[test]
fn a_data_only_import_never_writes_the_settings_row_and_a_settings_only_one_never_the_prompts() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut conn = import_test_db();
    let mut store = import_test_store(&mut conn);
    // A failing settings write cannot matter to a data-only import.
    conn.execute_batch("CREATE TRIGGER fail_settings BEFORE INSERT ON meta BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    import_with(&mut conn, &mut store, true, false).unwrap();
    let (ids, marker) = disk_state(&conn);
    assert_eq!(ids, vec!["new1".to_string()]);
    assert_eq!(marker, "OLD");
    conn.execute_batch("DROP TRIGGER fail_settings;").unwrap();

    let mut store = import_test_store_again(&mut conn);
    conn.execute_batch("CREATE TRIGGER fail_prompts BEFORE INSERT ON prompts BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    import_with(&mut conn, &mut store, false, true).unwrap();
    let (ids, marker) = disk_state(&conn);
    assert_eq!(ids, vec!["old1".to_string(), "old2".to_string()], "a settings-only import leaves the prompt rows alone");
    assert_eq!(marker, "NEW");
}

// Puts the disk back to old1, old2 plus a settings row marked OLD.
pub(super) fn import_test_store_again(conn: &mut Connection) -> Store {
    conn.execute_batch("DELETE FROM prompts; DELETE FROM meta;").unwrap();
    import_test_store(conn)
}

#[test]
fn a_failed_settings_only_import_puts_back_a_hotkey_it_had_dropped() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut conn = import_test_db();
    let mut store = import_test_store(&mut conn);
    store.prompts[0].hotkey = "CommandOrControl+Alt+K".into();
    store.prompts[1].hotkey = "CommandOrControl+Alt+J".into();
    let mut backup = import_test_backup();
    backup.settings.hotkey = "CommandOrControl+Alt+K".into(); // collides with old1
    let before = serde_json::to_string(&store.prompts).unwrap();
    let r = run_import(&mut store, backup, false, true, |_, prompts, _| {
        assert!(prompts, "a dropped accelerator changes prompt rows, so they are written too");
        false
    });
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(serde_json::to_string(&store.prompts).unwrap(), before, "memory matches the disk again");
}

// ---- the confidential vault keys survive every path that keeps the prompts ----

// A settings block with a vault set up for password "pw", plus one prompt
// sealed with its key.
pub(super) fn vault_fixture() -> (Settings, Prompt, [u8; 32]) {
    let mut settings = Settings::default();
    let salt = b"0123456789abcdef";
    let key = secret_derive_key("pw", salt).expect("derive");
    let verify = secret_encrypt(SECRET_VERIFY_MARKER, &key);
    assert_eq!(secret_vault_first_setup(&mut settings, salt, verify, |_| true), Ok(true));
    let mut p = test_prompt("secret");
    p.secret = true;
    p.text = secret_encrypt("the hidden text", &key);
    (settings, p, key)
}

// The same password must still open the prompt with what the settings hold.
pub(super) fn still_opens(settings: &Settings, p: &Prompt) -> bool {
    let salt = settings.ui_texts.get(SECRET_SALT_KEY).cloned().unwrap_or_default();
    let verify = settings.ui_texts.get(SECRET_VERIFY_KEY).cloned().unwrap_or_default();
    match secret_vault_verify("pw", &salt, &verify) {
        Ok(key) => reveal_secret_text(p, Some(key)).map(|(t, _)| t == "the hidden text").unwrap_or(false),
        Err(_) => false,
    }
}

#[test]
fn a_reset_keeps_the_vault_so_a_confidential_prompt_still_opens() {
    let (mut old, p, _) = vault_fixture();
    old.theme = "ocean".into();
    old.ui_texts.insert(BACKUP_PW_KEY.into(), "sealed".into());
    assert!(still_opens(&old, &p));
    let fresh = settings_after_reset(&old);
    assert_eq!(fresh.theme, default_theme(), "everything else is reset");
    assert_eq!(fresh.ui_texts.get(BACKUP_PW_KEY).map(String::as_str), Some("sealed"));
    assert!(still_opens(&fresh, &p), "the same password must open the prompt after a reset");
}

#[test]
fn a_reset_never_shortens_what_the_old_settings_kept() {
    let mut old = Settings { retention_defaults_v2: false, ..Settings::default() };
    old.migrate(); // an install from before the 30/90-day defaults: keep forever
    let fresh = settings_after_reset(&old);
    assert_eq!(history_max_age(&fresh), None, "a reset must not start pruning the copy history");
    assert_eq!(fresh.ui_values.get("recordingRetentionDays"), Some(&0.0));
    // A window longer than the default is kept as well; a shorter one takes the default.
    let mut old = Settings::default();
    old.ui_values.insert("historyDays".into(), 365.0);
    old.ui_values.insert("recordingRetentionDays".into(), 7.0);
    let fresh = settings_after_reset(&old);
    assert_eq!(fresh.ui_values.get("historyDays"), Some(&365.0));
    assert!(!fresh.ui_values.contains_key("recordingRetentionDays"));
    // A fresh install resets to the defaults.
    let fresh = settings_after_reset(&Settings::default());
    assert!(!fresh.ui_values.contains_key("historyDays"));
    assert!(!fresh.ui_values.contains_key("recordingRetentionDays"));
}

pub(super) fn vault_store(settings: Settings, p: Prompt) -> Store {
    Store { prompts: vec![p], trash: Vec::new(), settings, prompts_locked: false, written: Default::default() }
}

pub(super) fn foreign_backup(with_salt: bool) -> Backup {
    let mut settings = Settings::default();
    settings.ui_texts.insert("marker".into(), "THEIRS".into());
    if with_salt {
        settings.ui_texts.insert(SECRET_SALT_KEY.into(), "Zm9yZWlnbi1zYWx0".into());
        settings.ui_texts.insert(SECRET_VERIFY_KEY.into(), "Zm9yZWlnbi12ZXJpZnk=".into());
    }
    Backup { prompts: vec![test_prompt("theirs")], settings }
}

#[test]
fn a_settings_only_import_keeps_the_local_vault_whatever_the_file_carries() {
    for with_salt in [true, false] {
        let (settings, p, _) = vault_fixture();
        let mut store = vault_store(settings, p.clone());
        apply_import(&mut store, foreign_backup(with_salt), false, true);
        assert_eq!(store.settings.ui_texts.get("marker").map(String::as_str), Some("THEIRS"), "the settings did come in");
        assert!(still_opens(&store.settings, &p), "file with salt: {with_salt}");
    }
}

#[test]
fn a_settings_file_cannot_replace_the_local_vault() {
    let (mut live, p, _) = vault_fixture();
    let mut file = SettingsOnlyExport::default();
    file.ui_texts.insert(SECRET_SALT_KEY.into(), "Zm9yZWlnbi1zYWx0".into());
    file.ui_texts.insert(SECRET_VERIFY_KEY.into(), "Zm9yZWlnbi12ZXJpZnk=".into());
    file.ui_texts.insert("copyFont".into(), "Consolas".into());
    strip_denied_import_keys(&mut file);
    merge_settings_only(&mut live, file, |_| true).unwrap();
    assert_eq!(live.ui_texts.get("copyFont").map(String::as_str), Some("Consolas"));
    assert!(still_opens(&live, &p));
}

#[test]
fn a_full_import_with_data_takes_the_backups_own_vault_keys() {
    let (settings, p, _) = vault_fixture();
    let mut store = vault_store(settings, p);
    let mut backup = foreign_backup(true);
    backup.prompts = vec![test_prompt("theirs")];
    apply_import(&mut store, backup, true, true);
    assert_eq!(store.settings.ui_texts.get(SECRET_SALT_KEY).map(String::as_str), Some("Zm9yZWlnbi1zYWx0"));
    assert_eq!(store.settings.ui_texts.get(SECRET_VERIFY_KEY).map(String::as_str), Some("Zm9yZWlnbi12ZXJpZnk="));
}

// A settings-only restore of a stored snapshot merges the settings row in the
// database itself; the vault keys of the live row must come through it.
#[test]
fn a_settings_only_restore_keeps_the_local_vault() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-vault-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-vault-staged.db", brand::SLUG));
    let (live, p, _) = vault_fixture();
    {
        let conn = full_schema_db(&target, "target");
        db_write_settings(&conn, &live).unwrap();
    }
    {
        let conn = full_schema_db(&staged, "staged");
        let mut theirs = Settings::default();
        theirs.ui_texts.insert("marker".into(), "THEIRS".into());
        theirs.ui_texts.insert(SECRET_SALT_KEY.into(), "Zm9yZWlnbi1zYWx0".into());
        theirs.ui_texts.insert(SECRET_VERIFY_KEY.into(), "Zm9yZWlnbi12ZXJpZnk=".into());
        db_write_settings(&conn, &theirs).unwrap();
    }
    restore_selective(&target, &staged, "settings").unwrap();
    let conn = Connection::open(&target).unwrap();
    let (_, restored, _, _, _) = db_load(&conn, &mut WrittenRows::new());
    let restored = restored.expect("settings row");
    assert_eq!(restored.ui_texts.get("marker").map(String::as_str), Some("THEIRS"), "the backup settings did come in");
    assert!(still_opens(&restored, &p), "the local vault keys must survive");
    drop(conn);
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

// ---- review follow-ups: vault follows the prompts, stale fallbacks, snapshot restores ----

#[test]
fn a_data_import_takes_the_backups_vault_keys_or_keeps_local_ones_when_it_has_none() {
    let (settings, p, _) = vault_fixture();
    let mut store = vault_store(settings.clone(), p.clone());
    apply_import(&mut store, foreign_backup(true), true, false);
    assert_eq!(store.settings.ui_texts.get(SECRET_SALT_KEY).map(String::as_str), Some("Zm9yZWlnbi1zYWx0"), "keys follow the prompts");
    assert_eq!(store.settings.ui_texts.get("marker"), None, "data scope still leaves the other settings alone");

    let mut store = vault_store(settings.clone(), p.clone());
    apply_import(&mut store, foreign_backup(false), true, false);
    assert!(still_opens(&store.settings, &p), "a backup without keys keeps the local ones");
    let mut store = vault_store(settings, p.clone());
    apply_import(&mut store, foreign_backup(false), true, true);
    assert!(still_opens(&store.settings, &p), "full import of a backup without keys keeps the local ones");
    assert_eq!(store.settings.ui_texts.get("marker").map(String::as_str), Some("THEIRS"));
}

#[test]
fn carrying_vault_keys_from_a_source_without_them_changes_nothing() {
    let (with, _, _) = vault_fixture();
    let mut to = with.clone();
    carry_vault_keys(&Settings::default(), &mut to);
    assert_eq!(to.ui_texts.get(SECRET_SALT_KEY), with.ui_texts.get(SECRET_SALT_KEY));
    assert!(has_vault_keys(&to));
}

#[test]
fn an_import_warns_when_trashed_confidential_prompts_lose_their_key() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let (settings, p, _) = vault_fixture();
    for (trash_secret, expect) in [(true, true), (false, false)] {
        let mut store = vault_store(settings.clone(), p.clone());
        let mut t = test_prompt("trashed");
        t.secret = trash_secret;
        store.trash.push(t);
        let r = run_import(&mut store, foreign_backup(true), true, true, |_, _, _| true).unwrap();
        assert_eq!(r.3, expect, "trash holds a confidential prompt: {trash_secret}");
    }
    let mut store = vault_store(settings, p);
    let mut t = test_prompt("trashed");
    t.secret = true;
    store.trash.push(t);
    let r = run_import(&mut store, foreign_backup(false), true, true, |_, _, _| true).unwrap();
    assert!(!r.3, "keys unchanged, nothing to warn about");
}

#[test]
fn an_import_that_swaps_the_vault_keys_ends_the_unlocked_session() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let (settings, p, _) = vault_fixture();
    for (foreign, expect_locked) in [(true, true), (false, false)] {
        *SECRET_SESSION.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(SecretSession { key: [7u8; 32], unlocked_at: std::time::Instant::now() });
        let mut store = vault_store(settings.clone(), p.clone());
        run_import(&mut store, foreign_backup(foreign), true, true, |_, _, _| true).unwrap();
        assert_eq!(secret_session_key(0).is_none(), expect_locked, "foreign keys: {foreign}");
    }
    *SECRET_SESSION.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

#[test]
fn adopting_another_store_ends_the_session_only_when_its_vault_differs() {
    let _serial = SETTINGS_LOCK_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let (settings, _, _) = vault_fixture();
    let foreign = foreign_backup(true).settings;
    for (new, expect_locked) in [(&foreign, true), (&settings, false)] {
        *SECRET_SESSION.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(SecretSession { key: [7u8; 32], unlocked_at: std::time::Instant::now() });
        end_session_if_vault_changed(&settings, new);
        assert_eq!(secret_session_key(0).is_none(), expect_locked);
    }
    *SECRET_SESSION.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

#[test]
fn a_data_restore_moves_the_vault_keys_along_with_the_prompts() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-vaultdata-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-vaultdata-staged.db", brand::SLUG));
    let (live, _, _) = vault_fixture();
    {
        let conn = full_schema_db(&target, "target");
        db_write_settings(&conn, &live).unwrap();
    }
    {
        let conn = full_schema_db(&staged, "staged");
        let mut theirs = Settings::default();
        theirs.ui_texts.insert("marker".into(), "THEIRS".into());
        theirs.ui_texts.insert(SECRET_SALT_KEY.into(), "Zm9yZWlnbi1zYWx0".into());
        theirs.ui_texts.insert(SECRET_VERIFY_KEY.into(), "Zm9yZWlnbi12ZXJpZnk=".into());
        db_write_settings(&conn, &theirs).unwrap();
    }
    restore_selective(&target, &staged, "data").unwrap();
    let conn = Connection::open(&target).unwrap();
    let after = read_settings_row(&conn, "main").expect("settings row");
    assert_eq!(after.ui_texts.get(SECRET_SALT_KEY).map(String::as_str), Some("Zm9yZWlnbi1zYWx0"));
    assert_eq!(after.ui_texts.get("marker"), None, "the other settings stay local");
    drop(conn);
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

// An old snapshot must not bring back an old receiver password, switch or
// allowed ranges, whichever scope restores its settings.
pub(super) fn old_snapshot_settings() -> Settings {
    let mut old = Settings { auto_update: false, ..Settings::default() };
    old.ui_flags.insert("remoteApi".into(), true);
    old.ui_flags.insert("clipWatcher".into(), true);
    old.ui_texts.insert("remotePasswordHash".into(), "old-hash".into());
    old.ui_texts.insert("remoteAllowedRanges".into(), "10.0.0.0/8".into());
    old.ui_texts.insert("marker".into(), "SNAPSHOT".into());
    old.skipped_versions = vec!["9.9.9".into()];
    old
}

pub(super) fn local_machine_settings() -> Settings {
    let mut live = Settings { auto_update: true, ..Settings::default() };
    live.ui_flags.insert("remoteApi".into(), false);
    live.ui_texts.insert("remotePasswordHash".into(), "new-hash".into());
    live
}

pub(super) fn assert_machine_values_are_local(s: &Settings) {
    assert_eq!(s.ui_flags.get("remoteApi"), Some(&false));
    assert_eq!(s.ui_texts.get("remotePasswordHash").map(String::as_str), Some("new-hash"));
    assert!(!s.ui_texts.contains_key("remoteAllowedRanges"));
    assert!(!s.ui_flags.contains_key("clipWatcher"));
    assert!(s.auto_update, "the update choice stays");
    assert!(s.skipped_versions.is_empty());
    assert_eq!(s.ui_texts.get("marker").map(String::as_str), Some("SNAPSHOT"), "ordinary settings do come back");
}

#[test]
fn a_snapshot_restore_keeps_this_machines_network_and_credential_values() {
    let dir = std::env::temp_dir();
    for scope in ["settings", "all"] {
        let target = dir.join(format!("{}-test-restore-machine-{scope}-target.db", brand::SLUG));
        let staged = dir.join(format!("{}-test-restore-machine-{scope}-staged.db", brand::SLUG));
        {
            let conn = full_schema_db(&target, "target");
            db_write_settings(&conn, &local_machine_settings()).unwrap();
        }
        {
            let conn = full_schema_db(&staged, "staged");
            db_write_settings(&conn, &old_snapshot_settings()).unwrap();
        }
        if scope == "settings" {
            restore_selective(&target, &staged, scope).unwrap();
        } else {
            keep_machine_in_staged(&target, &staged).unwrap();
            std::fs::rename(&staged, &target).unwrap();
        }
        let conn = Connection::open(&target).unwrap();
        assert_machine_values_are_local(&read_settings_row(&conn, "main").expect("settings row"));
        drop(conn);
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(&staged);
    }
}

#[test]
fn a_full_import_keeps_the_update_choices_and_the_clipboard_watcher_of_this_machine() {
    let mut store = Store { prompts: vec![], trash: Vec::new(), settings: local_machine_settings(), prompts_locked: false, written: Default::default() };
    apply_import(&mut store, Backup { prompts: vec![], settings: old_snapshot_settings() }, false, true);
    assert_machine_values_are_local(&store.settings);
}

#[test]
fn a_successful_write_removes_the_stale_fallback_file_and_a_reload_matches() {
    let dir = std::env::temp_dir().join(format!("{}-test-stale-fallback", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let stale = vec![test_prompt("stale")];
    assert!(write_sealed_json(&dir.join("prompts.json"), &stale));
    assert!(write_sealed_json(&dir.join("settings.json"), &Settings::default()));
    let mut conn = import_test_db();
    let fresh = vec![test_prompt("fresh")];
    db_write_prompts(&mut conn, &fresh, &mut WrittenRows::new()).unwrap();
    drop_stale_fallback(&dir, "prompts.json");
    drop_stale_fallback(&dir, "settings.json");
    drop_stale_fallback(&dir, "never-there.json"); // absent file: no effect, no panic
    assert!(!dir.join("prompts.json").exists() && !dir.join("settings.json").exists());
    let (loaded, _, _, _, _) = db_load(&conn, &mut WrittenRows::new());
    assert_eq!(loaded.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["fresh"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_settings_export_carries_no_password_hash_or_sealed_backup_password() {
    let mut s = Settings::default();
    s.ui_texts.insert("remotePasswordHash".into(), "hash".into());
    s.ui_texts.insert(BACKUP_PW_KEY.into(), "sealed".into());
    s.ui_texts.insert("copyFont".into(), "Consolas".into());
    let payload = settings_only_payload(&s);
    assert!(!payload.ui_texts.contains_key("remotePasswordHash"));
    assert!(!payload.ui_texts.contains_key(BACKUP_PW_KEY));
    assert!(payload.ui_texts.contains_key("copyFont"));
}

#[test]
fn a_window_copy_of_the_settings_has_no_leftover_receiver_secret() {
    let mut s = Settings::default();
    for k in ["remoteTlsKey", "remoteTlsCert", "remoteTlsCa", REMOTE_PASSWORD_KEY] {
        s.ui_texts.insert(k.into(), "secret".into());
    }
    s.ui_texts.insert(BACKUP_PW_KEY.into(), "sealed".into());
    s.ui_texts.insert("copyFont".into(), "Consolas".into());
    let copy = webview_settings(&s);
    for k in ["remoteTlsKey", "remoteTlsCert", "remoteTlsCa", REMOTE_PASSWORD_KEY, BACKUP_PW_KEY] {
        assert!(!copy.ui_texts.contains_key(k), "{k} must stay in the backend");
    }
    assert!(copy.ui_texts.contains_key("copyFont"), "ordinary rows still reach the window");
    assert!(s.ui_texts.contains_key(BACKUP_PW_KEY), "the stored settings keep it");
}

// A full "data" import must actually delete the prompts it replaces ON DISK,
// not just in the in-memory list apply_import mutates directly. The two tests
// above only ever check store.prompts, which is always correct (apply_import
// assigns it outright) regardless of this bug: it takes a real write_rows
// round-trip through a DB to see whether the OLD rows a smaller import leaves
// behind actually get deleted, or silently survive to reappear after restart.
#[test]
fn import_all_data_scope_deletes_the_prompts_it_replaces_from_disk() {
    let mut conn = row_test_db();
    let mut written = WrittenRows::new();
    // "This process" genuinely loaded two prompts before the import: the exact
    // condition write_rows' delete guard requires to treat their disappearance
    // as a real, user-initiated removal rather than another instance's unseen work.
    let original = numbered_prompts(2);
    db_write_prompts(&mut conn, &original, &mut written).unwrap();
    assert_eq!(stored_rows(&conn, "prompts").len(), 2);

    let mut store = Store {
        prompts: original,
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: std::cell::RefCell::new(written),
    };
    // A one-prompt backup, with an id neither original prompt had.
    let backup = Backup { prompts: vec![test_prompt("imported-only")], settings: Settings::default() };
    apply_import(&mut store, backup, true, false);
    assert_eq!(store.prompts.len(), 1, "the in-memory list is always replaced outright");

    // The save import_all performs right after apply_import, using the SAME
    // store.written apply_import just (did not) clear.
    let mut written = store.written.into_inner();
    db_write_prompts(&mut conn, &store.prompts, &mut written).unwrap();

    let rows = stored_rows(&conn, "prompts");
    assert_eq!(rows.len(), 1, "the two replaced prompts must actually be deleted from disk, not just from memory");
    assert_eq!(rows[0].0, "imported-only");
}

#[test]
fn purge_clip_inbox_from_backups_scrubs_every_snapshot() {
    let dir = std::env::temp_dir().join(format!("{}-test-purge-backups", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // A: gzipped snapshot (current format) with clip_inbox rows + a prompt that
    // must survive untouched.
    let folder_a = dir.join("20260101-000000");
    std::fs::create_dir_all(&folder_a).unwrap();
    let raw_a = dir.join("raw_a.db");
    {
        let conn = full_schema_db(&raw_a, "a");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('p1', 0, '{}');
                 INSERT INTO clip_inbox(ts, text) VALUES(1, 'secret-a');
                 INSERT INTO clip_inbox(ts, text) VALUES(2, 'secret-b');",
        )
        .unwrap();
    }
    gzip_to(&raw_a, &folder_a.join("data.db.gz")).unwrap();
    let _ = std::fs::remove_file(&raw_a);

    // B: plain (pre-2.6, ungzipped) snapshot; also has clip_inbox rows.
    let folder_b = dir.join("20260102-000000");
    std::fs::create_dir_all(&folder_b).unwrap();
    {
        let conn = full_schema_db(&folder_b.join("data.db"), "b");
        conn.execute_batch("INSERT INTO clip_inbox(ts, text) VALUES(3, 'secret-c');").unwrap();
    }

    // C: nothing in its clip_inbox; must not be counted as purged.
    let folder_c = dir.join("20260103-000000");
    std::fs::create_dir_all(&folder_c).unwrap();
    let raw_c = dir.join("raw_c.db");
    {
        full_schema_db(&raw_c, "c");
    }
    gzip_to(&raw_c, &folder_c.join("data.db.gz")).unwrap();
    let _ = std::fs::remove_file(&raw_c);

    // D: a snapshot claiming to be gzipped that is not; gunzip must fail on
    // it, and that must be counted as a failure (it might hold clip_inbox
    // rows this run could never even look at), never silently skipped.
    let folder_d = dir.join("20260104-000000");
    std::fs::create_dir_all(&folder_d).unwrap();
    std::fs::write(folder_d.join("data.db.gz"), b"not actually gzip data").unwrap();

    let (purged, failed) = purge_clip_inbox_from_backups_impl(&dir);
    assert_eq!(purged, 2, "only A and B actually had clip_inbox rows to remove");
    assert_eq!(failed, 1, "D could not be opened and must be reported, not silently treated as clean");

    let unpacked_a = dir.join("check_a.db");
    gunzip_to(&folder_a.join("data.db.gz"), &unpacked_a).unwrap();
    let conn = Connection::open(&unpacked_a).unwrap();
    let clip_count: i64 = conn.query_row("SELECT COUNT(*) FROM clip_inbox", [], |r| r.get(0)).unwrap();
    assert_eq!(clip_count, 0, "A's clip_inbox must be emptied");
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "p1", "the purge must never touch other tables");

    let conn = Connection::open(folder_b.join("data.db")).unwrap();
    let clip_count: i64 = conn.query_row("SELECT COUNT(*) FROM clip_inbox", [], |r| r.get(0)).unwrap();
    assert_eq!(clip_count, 0, "B's (ungzipped) clip_inbox must be emptied too");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restore_selective_skips_a_table_missing_from_an_older_backup() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-oldbackup-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-oldbackup-staged.db", brand::SLUG));
    {
        let conn = full_schema_db(&target, "target");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('live', 0, '{}');
                 INSERT INTO trash VALUES('live-trashed', 0, '{}');
                 INSERT INTO meta VALUES('settings', 'LIVE-SETTINGS');",
        )
        .unwrap();
    }
    {
        // Pre-91 backup: no trash table at all.
        let _ = std::fs::remove_file(&staged);
        let conn = Connection::open(&staged).unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
                 CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL, variants TEXT NOT NULL DEFAULT '');
                 CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
                 INSERT INTO prompts VALUES('backed-up', 0, '{}');",
        )
        .unwrap();
    }
    restore_selective(&target, &staged, "data").unwrap();
    let conn = Connection::open(&target).unwrap();
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "backed-up", "prompts (present in the old backup) must still be replaced");
    let trash_id: String = conn.query_row("SELECT id FROM trash", [], |r| r.get(0)).unwrap();
    assert_eq!(trash_id, "live-trashed", "trash (absent from the old backup) must be left alone, not wiped");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

// Every release before 3.1.0 wrote prompt_versions without its `variants`
// column. Restoring only the data out of such a backup used to fail on the
// column count alone and roll the whole thing back (prompts, trash and all),
// leaving the user with a backup they could not get their prompts out of.
#[test]
fn restore_selective_accepts_a_backup_table_that_predates_a_column() {
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-oldcolumns-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-oldcolumns-staged.db", brand::SLUG));
    {
        let conn = full_schema_db(&target, "target");
        conn.execute_batch(
            "INSERT INTO prompts VALUES('live', 0, '{}');
                 INSERT INTO prompt_versions(prompt_id, ts, name, text, variants)
                     VALUES('live', 1, 'Live', 'live text', 'a');",
        )
        .unwrap();
    }
    {
        let _ = std::fs::remove_file(&staged);
        let conn = Connection::open(&staged).unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
                 CREATE TABLE prompt_versions(id INTEGER PRIMARY KEY AUTOINCREMENT, prompt_id TEXT NOT NULL, ts INTEGER NOT NULL, name TEXT NOT NULL, text TEXT NOT NULL);
                 CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);
                 INSERT INTO prompts VALUES('backed-up', 0, '{}');
                 INSERT INTO prompt_versions(prompt_id, ts, name, text) VALUES('backed-up', 7, 'Older', 'older text');",
        )
        .unwrap();
    }

    restore_selective(&target, &staged, "data").expect("a backup without the newer column must still restore");

    let conn = Connection::open(&target).unwrap();
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "backed-up", "the prompts must come from the backup");
    let (pid, name, text, variants): (String, String, String, String) = conn
        .query_row("SELECT prompt_id, name, text, variants FROM prompt_versions", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    assert_eq!((pid.as_str(), name.as_str(), text.as_str()), ("backed-up", "Older", "older text"));
    assert_eq!(variants, "", "the column the backup never had must fall back to its default");

    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn restore_selective_rolls_back_everything_if_one_table_fails() {
    // "prompts" comes first in the copy order and would succeed on its own;
    // "trash" is made incompatible (its `data` column, which the live table
    // requires and has no default for, is missing) so its INSERT fails: proving
    // the whole thing runs in ONE transaction, not one commit per table.
    let dir = std::env::temp_dir();
    let target = dir.join(format!("{}-test-restore-rollback-target.db", brand::SLUG));
    let staged = dir.join(format!("{}-test-restore-rollback-staged.db", brand::SLUG));
    {
        let conn = full_schema_db(&target, "target");
        conn.execute_batch("INSERT INTO prompts VALUES('live', 0, '{}');").unwrap();
    }
    {
        let _ = std::fs::remove_file(&staged);
        let conn = Connection::open(&staged).unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL);
                 INSERT INTO prompts VALUES('backed-up', 0, '{}');
                 INSERT INTO trash VALUES('backed-up-trashed', 0);",
        )
        .unwrap();
    }
    let result = restore_selective(&target, &staged, "data");
    assert!(result.is_err(), "a column-count mismatch on trash must surface as an error");
    let conn = Connection::open(&target).unwrap();
    let prompt_id: String = conn.query_row("SELECT id FROM prompts", [], |r| r.get(0)).unwrap();
    assert_eq!(prompt_id, "live", "prompts must roll back too, even though its own copy succeeded");
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&staged);
}

#[test]
fn backup_decrypt_passes_through_legacy_plaintext() {
    // Old .json backups have no PSB1 header: the caller falls back to plain JSON.
    assert!(backup_decrypt(br#"{"prompts":[]}"#, None).expect("no error").is_none());
}

#[test]
fn copy_log_caps_groups_and_prunes() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX idx_copy_log_ts ON copy_log(ts);",
    )
    .unwrap();
    // Cap 3: only the three newest events survive, newest-first by insertion.
    for i in 0..5u64 {
        db_append_copy(&conn, &CopyEntry { id: format!("p{i}"), ts: 100 + i }, 3, None).unwrap();
    }
    let recent: Vec<String> = db_recent_copies(&conn, 10, false).into_iter().map(|e| e.id).collect();
    assert_eq!(recent, ["p4", "p3", "p2"]);
    // Grouped: a repeated id collapses to one row at its most recent position.
    db_append_copy(&conn, &CopyEntry { id: "p4".into(), ts: 200 }, 3, None).unwrap();
    let grouped = db_recent_copies(&conn, 10, true);
    assert_eq!(grouped[0].id, "p4");
    assert_eq!(grouped.iter().filter(|e| e.id == "p4").count(), 1);
    // Retention cutoff drops entries older than the window.
    db_append_copy(&conn, &CopyEntry { id: "old".into(), ts: 50 }, 100, Some(150)).unwrap();
    assert!(db_recent_copies(&conn, 100, false).iter().all(|e| e.id != "old"));
    // Gapped rowids (e.g. after an age-prune) must never let the cap be exceeded.
    conn.execute("DELETE FROM copy_log", []).unwrap();
    for i in 0..10u64 {
        db_append_copy(&conn, &CopyEntry { id: format!("g{i}"), ts: 0 }, 5, None).unwrap();
    }
    conn.execute("DELETE FROM copy_log WHERE id IN ('g7','g8')", []).unwrap();
    for i in 10..15u64 {
        db_append_copy(&conn, &CopyEntry { id: format!("g{i}"), ts: 0 }, 5, None).unwrap();
    }
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM copy_log", [], |r| r.get(0)).unwrap();
    assert!(n <= 5, "cap exceeded after gaps: {n}");
}

#[test]
fn restore_copy_history_reinserts_every_snapshotted_row() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);").unwrap();
    // The snapshot comes from recent_copies, newest first.
    let rows = vec![
        CopyLogRow { ts: 20, id: "b".into() },
        CopyLogRow { ts: 10, id: "a".into() },
    ];
    db_restore_copy_log(&conn, &rows).unwrap();
    // db_recent_copies orders by rowid DESC: the newest copy (b) must come first again,
    // the order the journal showed before the clear.
    let restored: Vec<String> = db_recent_copies(&conn, 10, false).into_iter().map(|e| e.id).collect();
    assert_eq!(restored, ["b", "a"]);
    let ts: Vec<u64> = db_recent_copies(&conn, 10, false).into_iter().map(|e| e.ts).collect();
    assert_eq!(ts, [20, 10]);
}

#[test]
fn restore_clip_inbox_keeps_the_newest_within_the_cap() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE clip_inbox(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, text TEXT NOT NULL);",
    )
    .unwrap();
    // Five snapshotted rows, oldest first: the order restore_clip_inbox
    // feeds them in after the frontend reverses clip_inbox_list's own
    // newest-first listing: capped at 3 like a real clipInboxMax setting.
    for i in 0..5u64 {
        db_add_clip(&conn, i, &format!("t{i}"), 3).unwrap();
    }
    let texts: Vec<String> = db_list_clips(&conn).into_iter().map(|c| c.text).collect();
    assert_eq!(texts, ["t4", "t3", "t2"]);
}

#[test]
fn daily_copies_are_zero_filled_and_bucketed() {
    // A day with no copies must still appear (as 0), not be skipped:
    // otherwise a gap in the chart would read as "the app broke that day"
    // instead of "nothing was copied".
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX idx_copy_log_ts ON copy_log(ts);",
    )
    .unwrap();
    const DAY: u64 = 86400;
    let today = now_secs() / DAY * DAY;
    // Two copies today, three two days ago, none yesterday, one outside the window.
    db_append_copy(&conn, &CopyEntry { id: "a".into(), ts: today + 10 }, 100, None).unwrap();
    db_append_copy(&conn, &CopyEntry { id: "b".into(), ts: today + 20 }, 100, None).unwrap();
    for i in 0..3u64 {
        db_append_copy(&conn, &CopyEntry { id: format!("c{i}"), ts: today - 2 * DAY + i }, 100, None).unwrap();
    }
    db_append_copy(&conn, &CopyEntry { id: "old".into(), ts: today - 20 * DAY }, 100, None).unwrap();
    let daily = db_daily_copies(&conn, 14);
    assert_eq!(daily.len(), 14, "must always return exactly `days` buckets");
    assert_eq!(daily.last().unwrap().ts, today, "the newest bucket is today");
    assert_eq!(daily.last().unwrap().count, 2);
    assert_eq!(daily[daily.len() - 2].count, 0, "yesterday had nothing - must be 0, not missing");
    assert_eq!(daily[daily.len() - 3].count, 3, "two days ago");
    assert!(daily.iter().all(|d| d.ts % DAY == 0), "every bucket is a UTC day boundary");
}

#[test]
fn grouped_journal_row_id_and_timestamp_come_from_the_same_copy() {
    // A clock that went backwards (or timestamps switched off mid-run) makes
    // the newest row by rowid NOT the newest by ts. Picking both with separate
    // aggregates then paired the newest row id with an older row's time, so the
    // delete button and the date shown belonged to different copies.
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX idx_copy_log_ts ON copy_log(ts);",
    )
    .unwrap();
    db_append_copy(&conn, &CopyEntry { id: "p1".into(), ts: 900 }, 100, None).unwrap();
    db_append_copy(&conn, &CopyEntry { id: "p1".into(), ts: 100 }, 100, None).unwrap();

    let grouped = db_recent_copies(&conn, 10, true);
    assert_eq!(grouped.len(), 1, "one row per prompt");
    let row = &grouped[0];
    let real_ts: i64 = conn
        .query_row("SELECT ts FROM copy_log WHERE rowid = ?1", params![row.rowid], |r| r.get(0))
        .unwrap();
    assert_eq!(
        row.ts as i64, real_ts,
        "the timestamp shown must be the one on the row the delete button targets"
    );
    assert_eq!(row.ts, 100, "the newest row by id is the second copy, ts 100");
}

#[test]
fn journal_rowid_deletes_exactly_one_entry() {
    // The rowid db_recent_copies() now returns has to target the exact row:
    // never "every copy of this prompt", in either view mode.
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);
             CREATE INDEX idx_copy_log_ts ON copy_log(ts);",
    )
    .unwrap();
    db_append_copy(&conn, &CopyEntry { id: "p1".into(), ts: 100 }, 100, None).unwrap();
    db_append_copy(&conn, &CopyEntry { id: "p1".into(), ts: 200 }, 100, None).unwrap();
    db_append_copy(&conn, &CopyEntry { id: "p2".into(), ts: 150 }, 100, None).unwrap();
    // Ungrouped: three distinct rows, delete-by-rowid removes exactly one.
    let ungrouped = db_recent_copies(&conn, 10, false);
    assert_eq!(ungrouped.len(), 3);
    let target = ungrouped.iter().find(|e| e.ts == 100).unwrap().rowid;
    conn.execute("DELETE FROM copy_log WHERE rowid = ?1", params![target]).unwrap();
    let after = db_recent_copies(&conn, 10, false);
    assert_eq!(after.len(), 2, "deleted the wrong number of rows");
    assert!(after.iter().any(|e| e.ts == 200), "the OTHER p1 copy must survive");
    assert!(after.iter().any(|e| e.ts == 150));
    // Grouped: p1's rowid is its most-recent (ts=200) copy; deleting it must
    // remove only that one row, not every copy_log row for p1.
    let grouped = db_recent_copies(&conn, 10, true);
    let p1 = grouped.iter().find(|e| e.id == "p1").unwrap();
    assert_eq!(p1.ts, 200);
    conn.execute("DELETE FROM copy_log WHERE rowid = ?1", params![p1.rowid]).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM copy_log WHERE id = 'p1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "p1 only had that one row left, both must not survive a single delete");
}

pub(super) fn sample_prompts() -> Vec<Prompt> {
    vec![
        Prompt {
            id: "p1".into(),
            trashed_at: 0,
            name: "Mail".into(),
            text: "Hello\nWorld".into(),
            color: "#ef4444".into(),
            image: "data:image/png;base64,QUJD".into(),
            show_image: true,
            copy_image: true,
            file_path: String::new(),
            pdf_page: 0,
            icon_path: String::new(),
            caption: String::new(),
            caption_size: 0,
            font: "mono".into(),
            font_size: 24,
            favorite: false,
            tags: vec!["Work".into(), "E-Mail".into()],
            notes: "Ask Anna first.\nDeadline: Friday".into(),
            hotkey: "CommandOrControl+Shift+1".into(),
            hotkey_paste: "paste".into(),
            variants: Vec::new(),
            rotate_mode: String::new(),
            rotate_auto: false,
            media_duration_ms: 0,
            media_width: 0,
            media_height: 0,
            media_size_bytes: 0,
            image_text: String::new(),
            secret: false,
        },
        Prompt {
            id: "p2".into(),
            trashed_at: 0,
            name: "Doc".into(),
            text: "Doc".into(),
            color: String::new(),
            image: String::new(),
            show_image: true,
            copy_image: false,
            file_path: "C:\\tmp\\report.pdf".into(),
            pdf_page: 3,
            icon_path: "C:\\tmp\\clip.mp4".into(),
            caption: "Mein Untertitel".into(),
            caption_size: 18,
            font: String::new(),
            font_size: 1,
            favorite: false,
            tags: Vec::new(),
            notes: String::new(),
            hotkey: String::new(),
            hotkey_paste: String::new(),
            variants: Vec::new(),
            rotate_mode: String::new(),
            rotate_auto: false,
            media_duration_ms: 0,
            media_width: 0,
            media_height: 0,
            media_size_bytes: 0,
            image_text: String::new(),
            secret: false,
        },
    ]
}

#[test]
fn copy_log_cap_keeps_exactly_cap_rows_after_deletions() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE copy_log(ts INTEGER NOT NULL, id TEXT NOT NULL);").unwrap();
    for i in 0..5u64 {
        db_append_copy(&conn, &CopyEntry { id: format!("p{i}"), ts: 0 }, 5, None).unwrap();
    }
    // Two deletions leave rowid gaps; the next appends must still keep a full cap of rows.
    conn.execute("DELETE FROM copy_log WHERE id IN ('p1','p2')", []).unwrap();
    for i in 5..9u64 {
        db_append_copy(&conn, &CopyEntry { id: format!("p{i}"), ts: 0 }, 5, None).unwrap();
    }
    let ids: Vec<String> = db_recent_copies(&conn, 10, false).into_iter().map(|e| e.id).collect();
    assert_eq!(ids, ["p8", "p7", "p6", "p5", "p4"]);
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
fn only_the_reserved_names_are_self_filling() {
    assert!(is_dyn_var_name("@date"));
    assert!(is_dyn_var_name("@uuid"));
    assert!(!is_dyn_var_name("@kunde"), "@ plus an unreserved word is an ordinary variable");
    assert!(!is_dyn_var_name("date"));
    assert!(!is_dyn_var_name("@"));
    let none = HashMap::new();
    assert_eq!(resolve_static_vars("{?@kunde=Meier} {?@date}", &none, &none), "Meier {?@date}");
    // The list the frontend uses must be the one used here.
    let js = include_str!("../../../ui/promptVars.js");
    let start = js.find("export const DYN_VARS = [").expect("DYN_VARS in promptVars.js");
    let body = &js[start..js[start..].find("];").map(|i| start + i).unwrap()];
    let mut js_names: Vec<String> = body.split('"').skip(1).step_by(2).map(|s| s.to_string()).collect();
    js_names.sort();
    let mut rust_names: Vec<String> = DYN_NAMES.iter().map(|s| s.to_string()).collect();
    rust_names.sort();
    assert_eq!(js_names, rust_names, "DYN_VARS (ui/promptVars.js) and DYN_NAMES (store.rs) must list the same names");
}

#[test]
fn a_new_empty_snapshot_gets_its_names_cache_at_creation() {
    let dir = stream_test_dir("empty-names");
    {
        let conn = Connection::open(dir.join("data.db")).unwrap();
        conn.execute_batch("CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);").unwrap();
    }
    let root = dir.join("backups");
    let guard_src = std::sync::Mutex::new(0u8);
    let name = backup_released(guard_src.lock().unwrap(), &dir, &root, 5, None, || {}).unwrap();
    assert!(root.join(&name).join(BACKUP_NAMES_FILE).exists(), "written when the snapshot is made, not at the first listing");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn backup_names_cache_is_written_for_an_empty_snapshot() {
    let dir = std::env::temp_dir().join(format!("{}-test-empty-names-cache", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    let folder = dir.join("20260101-000000");
    std::fs::create_dir_all(&folder).unwrap();
    let raw = dir.join("raw.db");
    drop(full_schema_db(&raw, "e"));
    gzip_to(&raw, &folder.join("data.db.gz")).unwrap();
    let _ = std::fs::remove_file(&raw);

    assert!(backup_prompt_names(&folder).is_empty());
    assert!(folder.join(BACKUP_NAMES_FILE).exists(), "an empty snapshot is cached too, not unpacked on every listing");

    // A snapshot that cannot be read at all must not get a cached empty list.
    let bad = dir.join("20260102-000000");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("data.db.gz"), b"not gzip").unwrap();
    assert!(backup_prompt_names(&bad).is_empty());
    assert!(!bad.join(BACKUP_NAMES_FILE).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn folder_import_upgrades_the_retired_placeholder_spelling() {
    let item = folder_file_item("Note".to_string(), vec![], "Title\nHi {#{name}#} on {#{date}#}");
    assert_eq!(item.text, "Hi {?name} on {?@date}");
    let item = folder_file_item("Note".to_string(), vec![], &format!("one{}Dear {{#{{name}}#}}", VARIANT_SEP));
    assert_eq!(item.variants, vec!["Dear {?name}".to_string()]);
}

pub(super) fn bare_import_store() -> Store {
    Store { prompts: Vec::new(), trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() }
}

#[test]
fn import_ignores_a_secret_flag_on_text_that_is_not_sealed() {
    let json = r#"[{"name":"Fake","text":"plain words","secret":true}]"#;
    let data = parse_json_data(json).unwrap();
    let mut store = bare_import_store();
    append_imported_prompts(&mut store, data.prompts);
    assert!(!store.prompts[0].secret, "plain text with the flag set is imported as a plain entry");
}

#[test]
fn csv_and_txt_exports_never_carry_confidential_entries() {
    assert!(secrets_exportable("json", true));
    assert!(!secrets_exportable("json", false));
    assert!(!secrets_exportable("csv", true));
    assert!(!secrets_exportable("txt", true));
}

#[test]
fn import_drops_only_the_invalid_tags() {
    let known = HashMap::new();
    let long = "x".repeat(MAX_TAG_LEN + 1);
    let tags = lenient_import_tags(vec!["Clients, 2025".into(), "Work".into(), long, "work".into(), "  ".into(), "Home".into()], &known);
    assert_eq!(tags, vec!["Work".to_string(), "Home".to_string()], "good tags survive, bad and duplicate ones go");
}

#[test]
fn import_truncates_an_over_long_note_on_a_char_boundary() {
    let note = "\u{e4}".repeat(MAX_NOTES_LEN + 25);
    let kept = lenient_import_note(&note);
    assert_eq!(kept.chars().count(), MAX_NOTES_LEN);
    assert!(kept.chars().all(|c| c == '\u{e4}'));
    assert_eq!(lenient_import_note("short note"), "short note");
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
