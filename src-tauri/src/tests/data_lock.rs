use super::*;

// ---- Data folder lock --------------------------------------------------

pub(super) fn ddlock_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ps-ddlock-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// These tests all read and write the same two globals with real filesystem
// I/O in between, so the default parallel test runner can interleave them.
// Serialize just this group, the same way BG_OPS_TEST_LOCK already does
// for its own shared state.
pub(super) static DDLOCK_TEST_LOCK: Mutex<()> = Mutex::new(());

// Saves/restores the two globals the way settings_locked_command_reports_the_flag
// does for SETTINGS_LOCKED: these tests are the only ones in the suite that
// touch them, but the same courtesy avoids surprising a future one.
pub(super) struct DdlockGuard {
    _serial: std::sync::MutexGuard<'static, ()>,
    was_readonly: bool,
    was_owned: bool,
    was_host: Option<String>,
}
impl DdlockGuard {
    pub(super) fn capture() -> Self {
        Self {
            _serial: DDLOCK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner()),
            was_readonly: DATA_DIR_READONLY.load(Ordering::Relaxed),
            was_owned: DATA_DIR_LOCK_OWNED.load(Ordering::Relaxed),
            was_host: DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    }
}
impl Drop for DdlockGuard {
    fn drop(&mut self) {
        DATA_DIR_READONLY.store(self.was_readonly, Ordering::Relaxed);
        DATA_DIR_LOCK_OWNED.store(self.was_owned, Ordering::Relaxed);
        *DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()) = self.was_host.clone();
    }
}

#[test]
fn acquire_data_dir_lock_reads_a_fresh_foreign_lock_as_read_only() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("fresh-foreign");
    write_data_dir_lock(
        &data_dir_lock_path(&dir),
        &DataDirLockInfo { host: "OTHER-PC".to_string(), pid: 999, seen: unix_now(), id: String::new(), beat: 0 },
    );
    acquire_data_dir_lock(&dir, true);
    assert!(DATA_DIR_READONLY.load(Ordering::Relaxed), "a fresh foreign lock must open the store read-only");
    assert_eq!(
        DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).as_deref(),
        Some("OTHER-PC"),
        "the read-only banner needs the other machine's name"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn acquire_data_dir_lock_does_not_trust_the_other_machines_clock() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("stale-foreign");
    let path = data_dir_lock_path(&dir);
    // A timestamp far in the past says nothing: this process has not watched the
    // lock stand still for the stale limit, so it is read-only for now.
    let old_seen = unix_now().saturating_sub(DATA_DIR_LOCK_STALE_SECS + 10);
    write_data_dir_lock(&path, &DataDirLockInfo { host: "OTHER-PC".to_string(), pid: 999, seen: old_seen, id: "x".into(), beat: 3 });
    acquire_data_dir_lock(&dir, true);
    assert!(DATA_DIR_READONLY.load(Ordering::Relaxed));
    assert_eq!(read_data_dir_lock(&path).unwrap().host, "OTHER-PC", "the foreign lock is left alone");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn acquire_data_dir_lock_never_treats_its_own_hostname_as_foreign() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("own-host");
    // Same host, different (long-gone) pid and a timestamp that would count as
    // stale too: neither should matter: a crash-and-restart on THIS machine
    // is not the two-machine case this guards against, and single-instance
    // already rules out a second live process here.
    write_data_dir_lock(
        &data_dir_lock_path(&dir),
        &DataDirLockInfo { host: local_host_name(), pid: 424_242, seen: unix_now().saturating_sub(DATA_DIR_LOCK_STALE_SECS + 999), id: String::new(), beat: 0 },
    );
    acquire_data_dir_lock(&dir, true);
    assert!(!DATA_DIR_READONLY.load(Ordering::Relaxed), "this machine's own old lock must never lock this machine out");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn acquire_data_dir_lock_does_nothing_while_the_toggle_is_off() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("toggle-off");
    write_data_dir_lock(
        &data_dir_lock_path(&dir),
        &DataDirLockInfo { host: "OTHER-PC".to_string(), pid: 1, seen: unix_now(), id: String::new(), beat: 0 },
    );
    acquire_data_dir_lock(&dir, false);
    assert!(!DATA_DIR_READONLY.load(Ordering::Relaxed), "dataDirLock off must restore today's behaviour: no read-only state");
    assert!(DATA_DIR_LOCK_HOST.lock().unwrap_or_else(|e| e.into_inner()).is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn refresh_data_dir_lock_keeps_an_owned_lock_fresh() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("owned-refresh");
    acquire_data_dir_lock(&dir, true); // claims it -- no prior lock file
    assert!(!DATA_DIR_READONLY.load(Ordering::Relaxed));
    let path = data_dir_lock_path(&dir);
    // Back-date the file the way a real clock tick would leave it just before
    // the next scheduler round is due.
    write_data_dir_lock(&path, &DataDirLockInfo { host: local_host_name(), pid: std::process::id(), seen: unix_now() - 25, id: lock_claim_id().to_string(), beat: 0 });
    refresh_data_dir_lock(&dir);
    let refreshed = read_data_dir_lock(&path).expect("refresh must not delete the lock file");
    assert!(unix_now().saturating_sub(refreshed.seen) < 5, "a round while owned must bring the timestamp back to now");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn refresh_data_dir_lock_never_touches_a_foreign_lock_while_read_only() {
    let _g = DdlockGuard::capture();
    let dir = ddlock_test_dir("readonly-refresh");
    let path = data_dir_lock_path(&dir);
    let foreign_seen = unix_now();
    write_data_dir_lock(&path, &DataDirLockInfo { host: "OTHER-PC".to_string(), pid: 1, seen: foreign_seen, id: String::new(), beat: 0 });
    acquire_data_dir_lock(&dir, true); // becomes read-only against the fresh foreign lock
    assert!(DATA_DIR_READONLY.load(Ordering::Relaxed));
    refresh_data_dir_lock(&dir); // must be a no-op: this file is not ours to touch
    let after = read_data_dir_lock(&path).expect("a read-only round must not delete the other machine's lock");
    assert_eq!(after.host, "OTHER-PC");
    assert_eq!(after.seen, foreign_seen, "a read-only process must never rewrite the lock it does not own");
    let _ = fs::remove_dir_all(&dir);
}

// Eight paths: one for each provider's folder-name convention, one OneDrive
// path recognised only through its environment variable (business tenants
// rename the visible folder), and two ordinary local paths that must stay
// unrecognised.
#[test]
fn sync_folder_hint_recognises_known_cloud_folders_and_rejects_plain_ones() {
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\OneDrive\PromptSaver")), Some("OneDrive"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\Dropbox\PromptSaver")), Some("Dropbox"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\Google Drive\PromptSaver")), Some("Google Drive"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\GoogleDrive\PromptSaver")), Some("Google Drive"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\iCloudDrive\PromptSaver")), Some("iCloud Drive"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Sync\Nextcloud\PromptSaver")), Some("Nextcloud"));
    assert_eq!(sync_folder_hint(Path::new(r"D:\Data\PromptSaver")), None);
    assert_eq!(sync_folder_hint(Path::new(r"D:\Backups\PromptSaver")), None);

    // The env-var path: a business tenant's OneDrive root is often renamed in
    // Explorer (e.g. "BGCoding") with nothing resembling "onedrive" in the
    // visible name, so only the environment variable catches it.
    let guard = EnvVarGuard::set("OneDrive", r"D:\Company\BGCoding");
    assert_eq!(sync_folder_hint(Path::new(r"D:\Company\BGCoding\PromptSaver")), Some("OneDrive"));
    drop(guard);
}

// Minimal set/restore-on-drop wrapper: std::env::set_var is unsafe to call
// concurrently with a read elsewhere in the same process, but nothing else
// in this suite reads OneDrive/OneDriveConsumer/OneDriveCommercial, and the
// restore on Drop keeps a panic from leaking the override into later tests.
pub(super) struct EnvVarGuard {
    key: &'static str,
    prior: Option<String>,
}
impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prior = std::env::var(key).ok();
        unsafe { std::env::set_var(key, value) };
        Self { key, prior }
    }
}
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

#[test]
fn changing_one_prompt_rewrites_exactly_one_row() {
    let mut conn = row_test_db();
    let mut prompts = numbered_prompts(200);
    let mut written = WrittenRows::new();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();
    let before = stored_rows(&conn, "prompts");
    if sealing_is_randomised() {
        assert_ne!(
            enc_str("probe"),
            enc_str("probe"),
            "sealing must be randomised, or comparing blobs proves nothing below"
        );
    }
    prompts[42].name = "Renamed".into();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();
    let after = stored_rows(&conn, "prompts");
    assert_eq!(after.len(), before.len());
    for (b, a) in before.iter().zip(after.iter()) {
        assert_eq!(a.0, b.0, "order must not change");
        assert_eq!(a.1, b.1, "positions must not change");
        if a.0 == "id042" {
            assert_ne!(a.2, b.2, "the edited prompt must actually be rewritten");
        } else {
            assert_eq!(a.2, b.2, "an untouched prompt must keep the exact bytes it had");
        }
    }
    assert_eq!(stored_prompt(&after[42].2).name, "Renamed");
}

#[test]
fn reordering_prompts_moves_them_without_rewriting_any_content() {
    let mut conn = row_test_db();
    let mut prompts = numbered_prompts(20);
    let mut written = WrittenRows::new();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();
    let before: HashMap<String, String> =
        stored_rows(&conn, "prompts").into_iter().map(|(id, _, data)| (id, data)).collect();
    prompts.reverse();
    db_write_prompts(&mut conn, &prompts, &mut written).unwrap();
    let after = stored_rows(&conn, "prompts");
    assert_eq!(
        after.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        prompts.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        "the stored order must follow the new list"
    );
    for (i, row) in after.iter().enumerate() {
        assert_eq!(row.1, i as i64, "ord must be the row's new position");
        assert_eq!(&row.2, before.get(&row.0).unwrap(), "a move must not re-seal the content");
    }
}

#[test]
fn trashing_and_restoring_moves_exactly_one_row_between_the_tables() {
    let mut conn = row_test_db();
    let mut written = WrittenRows::new();
    let prompts = numbered_prompts(3);
    db_write_prompts_and_trash(&mut conn, &prompts, &[], &mut written).unwrap();
    let before = stored_rows(&conn, "prompts");

    let mut live = prompts.clone();
    let trashed = live.remove(1);
    db_write_prompts_and_trash(&mut conn, &live, std::slice::from_ref(&trashed), &mut written).unwrap();
    let kept = stored_rows(&conn, "prompts");
    assert_eq!(kept.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), vec!["id000", "id002"]);
    assert_eq!(kept[0].2, before[0].2, "the prompt before the hole must not be rewritten");
    assert_eq!(kept[1].1, 1, "the prompt after the hole must close the gap");
    let bin = stored_rows(&conn, "trash");
    assert_eq!(bin.len(), 1);
    assert_eq!(as_json(&stored_prompt(&bin[0].2)), as_json(&trashed), "the trashed prompt must arrive whole");

    db_write_prompts_and_trash(&mut conn, &prompts, &[], &mut written).unwrap();
    let restored = stored_rows(&conn, "prompts");
    assert_eq!(
        restored.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        vec!["id000", "id001", "id002"]
    );
    assert_eq!(
        as_json(&stored_prompt(&restored[1].2)),
        as_json(&prompts[1]),
        "the restored prompt must come back whole"
    );
    assert!(stored_rows(&conn, "trash").is_empty(), "the trash row must be gone again");
}

#[test]
fn a_replaced_prompt_set_is_written_in_full_once_the_row_map_is_cleared() {
    // The trap this guards: the map says "id X on disk holds this content", a
    // restore/import then replaces the table behind the writer's back, and the
    // in-memory prompt still hashes to what the map remembers: so without
    // clearing, the row is skipped and the user's data never reaches the disk.
    let mut conn = row_test_db();
    let mut written = WrittenRows::new();
    let mine = numbered_prompts(3);
    db_write_prompts(&mut conn, &mine, &mut written).unwrap();

    // What a restore does: put someone else's rows straight into the table.
    let theirs: Vec<Prompt> = mine
        .iter()
        .map(|p| Prompt { name: format!("{} (from the backup)", p.name), ..p.clone() })
        .collect();
    conn.execute("DELETE FROM prompts", []).unwrap();
    {
        let mut ins = conn
            .prepare("INSERT INTO prompts(id, ord, data) VALUES(?1, ?2, ?3)")
            .unwrap();
        for (i, p) in theirs.iter().enumerate() {
            ins.execute(params![p.id, i as i64, enc_str(&serde_json::to_string(p).unwrap())]).unwrap();
        }
    }

    // Stale map: every row still hashes to what it did before, so the write is
    // skipped entirely and the table keeps the foreign content.
    let mut stale = written.clone();
    db_write_prompts(&mut conn, &mine, &mut stale).unwrap();
    assert_eq!(
        stored_rows(&conn, "prompts").iter().map(|r| stored_prompt(&r.2).name).collect::<Vec<_>>(),
        theirs.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        "a stale map really does skip the write - which is why every wholesale replacement clears it"
    );

    // Cleared map (what Store::forget_written_rows does): the set on disk ends
    // up exactly the set in memory.
    written.clear();
    db_write_prompts(&mut conn, &mine, &mut written).unwrap();
    let stored: Vec<String> =
        stored_rows(&conn, "prompts").iter().map(|r| as_json(&stored_prompt(&r.2))).collect();
    assert_eq!(
        stored,
        mine.iter().map(as_json).collect::<Vec<_>>(),
        "after a full replacement the stored set must match the new one exactly"
    );
}

#[test]
fn the_first_save_after_forgetting_the_row_map_still_deletes_a_removed_prompt() {
    // forget_written_rows must void the hashes but keep the keys: a prompt moved to
    // the trash right after e.g. a settings reset is only deleted from `prompts`
    // when this process still knows it was there.
    let mut conn = row_test_db();
    let all = numbered_prompts(3);
    let mut written = WrittenRows::new();
    db_write_prompts(&mut conn, &all, &mut written).unwrap();
    let store = Store {
        prompts: all.clone(),
        trash: Vec::new(),
        settings: Settings::default(),
        prompts_locked: false,
        written: std::cell::RefCell::new(written),
    };
    store.forget_written_rows();

    let kept: Vec<Prompt> = all[..2].to_vec();
    let trash = vec![all[2].clone()];
    {
        let mut w = store.written.borrow_mut();
        db_write_prompts_and_trash(&mut conn, &kept, &trash, &mut w).unwrap();
    }
    assert_eq!(
        stored_rows(&conn, "prompts").iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
        vec!["id000", "id001"],
        "the removed prompt must leave the prompts table on the first save after a forget"
    );
    assert_eq!(stored_rows(&conn, "trash").len(), 1);
}

// A settings write that once fell back to settings.json leaves that file behind
// while prompts.json (which nothing wrote) stays absent. If the database is
// then unopenable for one launch, the prompts are invisible but very much still
// on disk, so the store has to refuse prompt writes regardless: write_rows itself
// now only deletes a row it can prove it once knew about (see the
// allowMultipleInstances fix), so an empty `written` map on its own no longer
// wipes the table: but a partially-known list (some rows genuinely loaded, one
// instance's own explicit delete mixed in) still relies entirely on the lock,
// which is why it stays unconditional here. Whether settings.json happens to be
// lying there says nothing about that, which is exactly what this used to hinge on.
#[test]
fn a_missing_prompts_json_beside_a_database_locks_prompt_writes() {
    let dir = std::env::temp_dir().join(format!("{}-test-fallback-lock", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // The real prompts live in the database this launch cannot open.
    let db = dir.join("data.db");
    let mine = numbered_prompts(3);
    {
        let mut conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);",
        )
        .unwrap();
        db_write_prompts(&mut conn, &mine, &mut WrittenRows::new()).unwrap();
    }
    // Only settings.json is on disk, left by an earlier failed settings write.
    let mut leftover = Settings::default();
    leftover.ui_texts.insert("marker".into(), "FROM-FILE".into());
    assert!(write_sealed_json(&dir.join("settings.json"), &leftover));
    assert!(!dir.join("prompts.json").exists());

    let store = load_store_from_json(&dir);
    assert_eq!(
        store.settings.ui_texts.get("marker").map(String::as_str),
        Some("FROM-FILE"),
        "the settings file is still the best source there is and must be read"
    );
    assert!(store.prompts.is_empty(), "there is nothing readable to load the prompts from");
    assert!(
        store.prompts_locked,
        "an empty prompt list with a database behind it must never be saveable"
    );

    // Even bypassing the lock, write_rows' own "only delete a row I can prove I
    // knew about" guard now protects this exact case too: `store.written` is
    // empty (this Store never read the table), so nothing gets deleted. The
    // lock remains the real, necessary defense for a partially-known list.
    {
        let mut conn = Connection::open(&db).unwrap();
        db_write_prompts(&mut conn, &store.prompts, &mut store.written.borrow_mut()).unwrap();
        assert_eq!(
            stored_rows(&conn, "prompts").len(),
            3,
            "write_rows must not delete rows it never learned about, even with the lock bypassed"
        );
    }

    // A genuinely fresh folder has no database behind it, so nothing is hidden
    // and writes must stay open.
    let fresh = std::env::temp_dir().join(format!("{}-test-fallback-fresh", brand::SLUG));
    let _ = std::fs::remove_dir_all(&fresh);
    std::fs::create_dir_all(&fresh).unwrap();
    assert!(
        !load_store_from_json(&fresh).prompts_locked,
        "a first launch with no data at all must be able to save"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&fresh);
}

// A data-folder move first writes the live state into the folder it is leaving,
// and the cleanup offered afterwards deletes exactly that folder. So a flush
// that could not write (a folder gone read-only, a permission change) has to
// abort the whole move: copying the now-stale files on and then authorising the
// deletion of the original is how the newest prompts would go for good.
#[test]
fn a_failed_flush_aborts_the_data_folder_move_and_authorises_no_cleanup() {
    let root = std::env::temp_dir().join(format!("{}-test-datadir-move", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    let current = root.join("old");
    let new = root.join("new");
    let pointer = root.join("pointer");
    for d in [&current, &new, &pointer] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(current.join("data.db"), b"the real store").unwrap();
    *LAST_OLD_DATA_DIR.lock().unwrap() = None;

    let mut rebound = false;
    {
        let mut refuses = || -> Result<(), String> { Err("folder is read-only".to_string()) };
        let mut rebind = || rebound = true;
        let moved = move_store_dir(&current, &new, &pointer, &mut refuses, &mut rebind);
        assert!(moved.is_err(), "a flush that could not persist the live state must fail the move");
    }
    assert!(!rebound, "the process must keep writing to the folder it never left");
    assert!(!new.join("data.db").exists(), "nothing may be copied to the new folder");
    assert!(
        !pointer.join("datapath").exists(),
        "future launches must still be pointed at the folder that holds the data"
    );
    assert!(
        LAST_OLD_DATA_DIR.lock().unwrap().is_none(),
        "the folder still holding the only copy must never be offered for deletion"
    );

    // The same move with a flush that works goes all the way through.
    {
        let mut works = || -> Result<(), String> { Ok(()) };
        let mut rebind = || rebound = true;
        let adopted = move_store_dir(&current, &new, &pointer, &mut works, &mut rebind).unwrap();
        assert!(!adopted, "a store copied across is the session's own: nothing to reload");
    }
    assert!(rebound, "the live store must be told it is writing to a different file now");
    assert!(new.join("data.db").exists(), "the store must arrive in the new folder");
    assert_eq!(
        std::fs::read_to_string(pointer.join("datapath")).unwrap(),
        new.to_string_lossy(),
        "the next launch must find the new folder"
    );
    assert_eq!(
        *LAST_OLD_DATA_DIR.lock().unwrap(),
        Some(current.clone()),
        "only a completed move may authorise cleaning up the old folder"
    );

    *LAST_OLD_DATA_DIR.lock().unwrap() = None;
    let _ = std::fs::remove_dir_all(&root);
}

// Picking a folder that already holds a store adopts it as it is. The caller has to
// reload the in-memory store from there, so the move must say that it happened and
// must leave the adopted files alone.
#[test]
fn moving_into_a_folder_with_a_store_reports_the_adoption_and_keeps_its_files() {
    let root = std::env::temp_dir().join(format!("{}-test-datadir-adopt", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    let current = root.join("old");
    let new = root.join("new");
    let pointer = root.join("pointer");
    for d in [&current, &new, &pointer] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(current.join("data.db"), b"this session").unwrap();
    std::fs::write(new.join("data.db"), b"the other library").unwrap();
    let mut works = || -> Result<(), String> { Ok(()) };
    let mut rebind = || {};
    let adopted = move_store_dir(&current, &new, &pointer, &mut works, &mut rebind).unwrap();
    assert!(adopted, "an existing store in the target must be reported as adopted");
    assert_eq!(std::fs::read(new.join("data.db")).unwrap(), b"the other library");
    *LAST_OLD_DATA_DIR.lock().unwrap() = None;
    let _ = std::fs::remove_dir_all(&root);
}

// Demo mode: switching on builds the sample store in its own subfolder and
// redirects the canonical folder at it, switching off puts the redirect back
// exactly as it was, and the real database, sitting right next to the
// redirect file the whole time, is never opened by either direction.
#[test]
fn demo_mode_round_trip_never_touches_the_real_store() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-roundtrip", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();
    let real_db = canonical.join("data.db");
    std::fs::write(&real_db, b"the real store, byte for byte").unwrap();
    let real_bytes_before = std::fs::read(&real_db).unwrap();

    // No redirect exists yet: enabling must create one.
    assert!(!canonical.join("datapath").exists());
    let demo = enter_demo_mode(&canonical, None).unwrap();
    assert_eq!(demo, canonical.join(DEMO_SUBFOLDER));
    assert!(demo.join("data.db").exists(), "enabling must create the sample store");
    assert_eq!(
        std::fs::read_to_string(canonical.join("datapath")).unwrap(),
        demo.to_string_lossy(),
        "the canonical folder must now redirect to the demo subfolder"
    );
    assert_eq!(
        std::fs::read(&real_db).unwrap(),
        real_bytes_before,
        "the real database must be untouched by enabling demo mode"
    );

    exit_demo_mode(&canonical).unwrap();
    assert!(
        !canonical.join("datapath").exists(),
        "disabling must remove the redirect there was none of before"
    );
    assert!(
        !canonical.join(DEMO_POINTER_BACKUP).exists(),
        "the backup marker must be cleaned up once restored"
    );
    assert_eq!(
        std::fs::read(&real_db).unwrap(),
        real_bytes_before,
        "the real database must still be untouched after disabling demo mode"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// A user who had already redirected their data elsewhere (the "move data
// folder" expert action) must get that exact redirect back, not an empty one,
// when demo mode is switched off again.
#[test]
fn demo_mode_restores_a_pre_existing_custom_redirect() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-custom-redirect", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();
    let custom = canonical.join("elsewhere");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(canonical.join("datapath"), custom.to_string_lossy().as_bytes()).unwrap();

    enter_demo_mode(&canonical, None).unwrap();
    assert_ne!(
        std::fs::read_to_string(canonical.join("datapath")).unwrap(),
        custom.to_string_lossy(),
        "entering demo mode must point away from the custom folder"
    );

    exit_demo_mode(&canonical).unwrap();
    assert_eq!(
        std::fs::read_to_string(canonical.join("datapath")).unwrap(),
        custom.to_string_lossy(),
        "the original custom redirect must come back exactly"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// Re-entering demo mode (switch off, then back on in the same session)
// rebuilds the sample store from nothing rather than keeping whatever a
// previous demo session left behind.
#[test]
fn demo_mode_rebuilds_fresh_on_every_enable() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-rebuild", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();

    let demo = enter_demo_mode(&canonical, None).unwrap();
    std::fs::write(demo.join("left-over.txt"), b"a change made during the demo session").unwrap();
    exit_demo_mode(&canonical).unwrap();

    enter_demo_mode(&canonical, None).unwrap();
    assert!(
        !demo.join("left-over.txt").exists(),
        "a fresh enable must wipe whatever a previous demo session left behind"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// The sample store itself: every prompt kind the spec asks for is present,
// and the sample library and settings load back exactly as any other store's
// would through the normal reader.
pub(super) fn demo_structure(lang: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-lang-{tag}", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let prompts = demo_prompts(&dir, None, lang);
    let settings = demo_settings(false, lang);
    let _ = std::fs::remove_dir_all(&dir);
    let ids = prompts.iter().map(|p| p.id.clone()).collect();
    let shape = prompts
        .iter()
        .map(|p| (!p.image.is_empty(), p.show_image, p.variants.len(), !p.file_path.is_empty(), p.favorite, p.tags.len()))
        .collect();
    let views = settings.views.iter().map(|v| (v.id.clone(), v.members.clone())).collect();
    let folders = settings.tile_folders.iter().map(|f| format!("{}|{:?}", f.id, f.members)).collect();
    let ids: Vec<String> = ids;
    let shape: Vec<(bool, bool, usize, bool, bool, usize)> = shape;
    let views: Vec<(String, Vec<String>)> = views;
    let folders: Vec<String> = folders;
    format!("{ids:?}{shape:?}{views:?}{folders:?}")
}

#[test]
fn demo_content_has_the_same_structure_in_every_language() {
    let base = demo_structure("en", "base");
    for lang in demo_text::DEMO_LANGS {
        assert_eq!(demo_structure(lang, lang), base, "{lang}: ids and structure must not change with the language");
    }
}

#[test]
fn demo_text_is_complete_dash_free_and_keeps_the_placeholder_in_every_language() {
    let en = demo_text::demo_text("en");
    for lang in demo_text::DEMO_LANGS {
        let d = demo_text::demo_text(lang);
        let all = [
            d.tag_demo, d.tag_favorite, d.welcome_name, d.welcome_text, d.image_name, d.image_text, d.rotating_name,
            d.rotating_text, d.rotating_a, d.rotating_b, d.placeholder_name, d.placeholder_text, d.favorite_name,
            d.favorite_text, d.chain_a_name, d.chain_a_text, d.chain_b_name, d.chain_b_text, d.attach_name,
            d.attach_file, d.attach_body, d.folder_a_name, d.folder_a_text, d.folder_b_name, d.folder_b_text,
            d.video_name, d.video_file, d.view_home, d.view_team, d.folder,
        ];
        for t in all {
            assert!(!t.trim().is_empty(), "{lang}: empty text");
            assert!(!t.contains('\u{2014}') && !t.contains('\u{2013}'), "{lang}: dash in {t:?}");
            assert!(!t.contains("_name") && !t.contains("_text") && !t.contains("demo."), "{lang}: raw key in {t:?}");
        }
        assert!(d.placeholder_text.contains("{?") && d.placeholder_text.contains("}"), "{lang}: placeholder syntax");
        for f in [d.attach_file, d.video_file] {
            assert!(!f.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']), "{lang}: {f:?} is not a file name");
        }
        assert!(d.attach_file.ends_with(".txt") && d.video_file.ends_with(".mp4"), "{lang}: file extensions");
        if lang != "en" {
            assert_ne!(d.welcome_text, en.welcome_text, "{lang} is still the English text");
        }
    }
}

#[test]
fn demo_text_falls_back_to_english_and_ignores_a_region() {
    let en = demo_text::demo_text("en");
    for odd in ["", "xx", "klingon", "fr-FR"] {
        assert_eq!(demo_text::demo_text(odd).welcome_name, en.welcome_name, "{odd:?} must fall back to EN");
    }
    assert_eq!(demo_text::demo_text("pt-BR").welcome_name, demo_text::demo_text("pt").welcome_name);
    assert_eq!(demo_text::demo_text("zh_CN").welcome_name, demo_text::demo_text("zh").welcome_name);
    assert_eq!(demo_text::demo_text("DE").welcome_name, "Willkommen");
}

#[test]
fn demo_settings_keep_the_interface_language() {
    assert_eq!(demo_settings(false, "en").language, "en");
    assert_eq!(demo_settings(false, "fr").language, "fr");
}

#[test]
fn demo_german_content_is_what_it_was_before_the_table() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-de-same", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let prompts = demo_prompts(&dir, None, "de");
    let by = |id: &str| prompts.iter().find(|p| p.id == id).unwrap();
    assert_eq!(by("demo-welcome").name, "Willkommen");
    assert_eq!(by("demo-welcome").text, "Das hier ist ein Beispieleintrag im Vorführmodus. Ein Klick kopiert ihn wie jeden anderen.");
    assert_eq!(by("demo-image").name, "Bild-Kachel");
    assert_eq!(by("demo-rotating").variants, vec!["Guten Tag!".to_string(), "Guten Abend!".to_string()]);
    assert_eq!(by("demo-placeholder").text, "Hallo {?Name}, vielen Dank für deine Nachricht.");
    assert_eq!(by("demo-favorite").tags, vec!["Demo".to_string(), "Favorit".to_string()]);
    assert_eq!(by("demo-attachment").file_path, "Anhang.txt");
    assert_eq!(std::fs::read_to_string(dir.join("Anhang.txt")).unwrap(), "Eine harmlose Beispieldatei für den Vorführmodus.\n");
    assert_eq!(demo_settings(false, "de").tile_folders[0].name, "Beispielordner");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn demo_store_rebuilt_in_another_language_holds_only_that_language() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-switch", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();
    assert!(dir.join("Anhang.txt").is_file());
    build_demo_store(&dir, None, "en").unwrap();
    assert!(!dir.join("Anhang.txt").exists(), "the German attachment must be gone after a rebuild");
    assert!(dir.join("Attachment.txt").is_file());
    let conn = db_conn_at(&dir).unwrap();
    let mut written = WrittenRows::new();
    let (prompts, settings, _p, _f, _l) = db_load(&conn, &mut written);
    assert_eq!(prompts.iter().find(|p| p.id == "demo-welcome").unwrap().name, "Welcome");
    assert_eq!(settings.unwrap().tile_folders[0].name, "Sample folder");
    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn demo_store_contains_every_sample_kind() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-contents", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();

    let conn = db_conn_at(&dir).unwrap();
    let mut written = WrittenRows::new();
    let (prompts, settings, _plain, _fails, _locked) = db_load(&conn, &mut written);
    let settings = settings.expect("the sample settings row must load back");
    let by_id: HashMap<&str, &Prompt> = prompts.iter().map(|p| (p.id.as_str(), p)).collect();

    assert!(by_id.get("demo-image").is_some_and(|p| !p.image.is_empty()), "an image tile");
    assert!(by_id.get("demo-welcome").unwrap().image.is_empty(), "a tile without an image");
    assert!(!by_id.get("demo-rotating").unwrap().variants.is_empty(), "a rotating prompt");
    assert!(by_id.get("demo-placeholder").unwrap().text.contains("{?"), "a self-fill placeholder");
    assert!(by_id.get("demo-favorite").unwrap().favorite, "a favorite");
    assert!(!by_id.get("demo-attachment").unwrap().file_path.is_empty(), "an attachment");
    assert!(!by_id.get("demo-welcome").unwrap().tags.is_empty(), "a tagged prompt");

    assert_eq!(settings.views.len(), 2, "two views");
    assert_eq!(settings.tile_folders.len(), 1, "one grid folder");
    assert_eq!(settings.tile_folders[0].members.len(), 2);
    assert_eq!(settings.ui_flags.get("demoMode"), Some(&true));

    let _ = std::fs::remove_dir_all(&dir);
}

// Finding 1 (HIGH, adversarial audit): disable_demo_mode flushes pending
// demo settings into the demo dir, under the store lock, BEFORE it flips
// the "datapath" redirect back to the real one: otherwise app.restart()'s
// own RunEvent::Exit flush (or the 250ms settings_flush_tick) can resolve
// data_dir() AFTER the flip and overwrite the real data.db with demo
// settings. This proves the write-then-flip half directly: a settings
// write made while the redirect still points at the demo dir lands there,
// never in the real store sitting right next to it.
#[test]
fn demo_settings_written_before_the_flip_land_in_the_demo_dir_not_the_real_one() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-exit-flush", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();
    let real_db = canonical.join("data.db");
    std::fs::write(&real_db, b"the real store, byte for byte").unwrap();
    let real_bytes_before = std::fs::read(&real_db).unwrap();

    let demo = enter_demo_mode(&canonical, None).unwrap();

    // Simulate the flush disable_demo_mode does before it calls
    // exit_demo_mode: at this point the redirect still points at `demo`.
    let conn = db_conn_at(&demo).unwrap();
    let changed = Settings { theme: "changed-during-demo".into(), ..Default::default() };
    db_write_settings(&conn, &changed).unwrap();

    exit_demo_mode(&canonical).unwrap();

    assert_eq!(
        std::fs::read(&real_db).unwrap(),
        real_bytes_before,
        "a settings write made before the redirect flip must never reach the real data.db"
    );
    let demo_conn = db_conn_at(&demo).unwrap();
    let mut written = WrittenRows::new();
    let (_p, settings, ..) = db_load(&demo_conn, &mut written);
    assert_eq!(
        settings.map(|s| s.theme),
        Some("changed-during-demo".to_string()),
        "the flush must have actually landed in the demo store"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// The other half of finding 1: DEMO_TRANSITION_LOCKED is the flag
// data_dir_write_blocked() AND db_conn() both check first (the second
// review's own point: db_conn() is the actual single choke point, not
// just the four functions that also call data_dir_write_blocked()) --
// this reads it back exactly like settings_locked_command_reports_the_flag
// and settings_write_failed_command_reports_the_flag do for their own
// flags (db_conn/data_dir_write_blocked themselves need a real AppHandle,
// which no unit test in this file constructs).
#[test]
fn demo_transition_locked_flag_round_trips() {
    let before = DEMO_TRANSITION_LOCKED.load(Ordering::Relaxed);
    DEMO_TRANSITION_LOCKED.store(true, Ordering::Relaxed);
    assert!(DEMO_TRANSITION_LOCKED.load(Ordering::Relaxed));
    DEMO_TRANSITION_LOCKED.store(false, Ordering::Relaxed);
    assert!(!DEMO_TRANSITION_LOCKED.load(Ordering::Relaxed));
    DEMO_TRANSITION_LOCKED.store(before, Ordering::Relaxed);
}

// Kept alive a few seconds and spawned only as this same test binary re-
// invoking itself (see other_instance_running_detects_a_live_pid_and_prunes_a_dead_one
// below), so instance_pid_is_this_exe's image-path check has something
// genuinely matching current_exe() to find, not an unrelated process.
#[test]
#[ignore]
fn __sleep_helper_for_instance_test() {
    std::thread::sleep(std::time::Duration::from_secs(3));
}

// other_instance_running must tell a real second live process apart from
// the allowMultipleInstances setting: see its own comment for why the
// flag was not trustworthy evidence. Uses this test binary's own pid
// (always alive) as the "other" instance and a definitely-dead one (a
// huge pid unlikely to exist, plus this process's own pid is excluded so
// it can never see itself).
#[test]
#[cfg(windows)]
fn other_instance_running_detects_a_live_pid_and_prunes_a_dead_one() {
    let canonical = std::env::temp_dir().join(format!("{}-test-instance-registry", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    assert!(!other_instance_running(&canonical), "an empty/missing registry has nothing to find");

    register_running_instance(&canonical);
    assert!(
        !other_instance_running(&canonical),
        "a process must never count itself as a second instance"
    );

    // A pid a genuinely different copy of THIS exe holds right now: a
    // third review finding requires instance_pid_is_this_exe to confirm
    // the pid's image path, not merely that some process holds it, so
    // the "other" instance here must actually be this same test binary
    // re-invoked as a child, kept alive by the sleep helper above.
    let exe = std::env::current_exe().unwrap();
    let mut child = std::process::Command::new(&exe)
        .args(["tests::__sleep_helper_for_instance_test", "--exact", "--ignored", "--test-threads=1"])
        .spawn()
        .unwrap();
    let other_pid = child.id();
    std::fs::write(instances_dir(&canonical).join(other_pid.to_string()), b"").unwrap();
    assert!(other_instance_running(&canonical), "a different LIVE copy of this exe must be detected");

    // An obviously stale entry (not a real pid on this machine) must be
    // pruned rather than perma-blocking every future enable/disable.
    std::fs::write(instances_dir(&canonical).join("999999999"), b"").unwrap();
    let _ = other_instance_running(&canonical); // one pass to prune it
    assert!(
        !instances_dir(&canonical).join("999999999").exists(),
        "a dead pid's marker must be pruned opportunistically"
    );

    let _ = child.kill();
    let _ = child.wait();
    unregister_running_instance(&canonical);
    assert!(
        !instances_dir(&canonical).join(std::process::id().to_string()).exists(),
        "unregister must remove this process's own marker"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// A pid reused by an unrelated process (Windows reuses freed pids; the
// window between a crash/task-kill and its stale entry getting pruned
// can span a whole later session) must not read as "still our instance"
// forever: a third review finding. cmd.exe here stands in for that
// unrelated process: it is definitely alive, and definitely not a copy
// of this exe, so instance_pid_is_this_exe must refuse it even though
// pid_is_alive alone would accept it. The entry must NOT be pruned for
// that alone, though (a fourth review finding): pruning here as well as
// on a dead pid would delete a REAL second instance's own marker the
// moment the image check merely fails to confirm it (a permission edge
// case, or a bug in that check), permanently losing this instance's
// spot in the registry with nothing to notice and re-register it.
#[test]
#[cfg(windows)]
fn other_instance_running_refuses_a_pid_reused_by_an_unrelated_process() {
    let canonical = std::env::temp_dir().join(format!("{}-test-instance-registry-reused-pid", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(instances_dir(&canonical)).unwrap();

    let mut child = std::process::Command::new("cmd").args(["/C", "ping -n 3 127.0.0.1 >NUL"]).spawn().unwrap();
    let other_pid = child.id();
    std::fs::write(instances_dir(&canonical).join(other_pid.to_string()), b"").unwrap();
    assert!(
        !other_instance_running(&canonical),
        "a live pid that is not a copy of this exe must never count as another instance"
    );
    assert!(
        instances_dir(&canonical).join(other_pid.to_string()).exists(),
        "a live pid must never be pruned on the image check alone, only a dead one"
    );

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&canonical);
}

// A real second instance running from the exact same exe must still be
// recognized when only its casing differs (a fourth review finding:
// Windows paths are case-insensitive, an exact PathBuf == PathBuf compare
// is not). Uses this test binary's own current_exe() with an uppercased
// copy of its path string, since a real differently-cased path to the
// same file cannot be constructed portably in a test.
#[test]
fn same_exe_path_ignores_casing() {
    let exe = std::env::current_exe().unwrap();
    let upper = PathBuf::from(exe.to_string_lossy().to_uppercase());
    assert!(same_exe_path(&exe, &upper), "the same file with different casing must still match");
}

// Two genuinely different files must never match regardless of casing.
#[test]
fn same_exe_path_refuses_a_different_file() {
    let exe = std::env::current_exe().unwrap();
    let other = std::env::temp_dir().join(format!("{}-test-same-exe-path-unrelated.exe", brand::SLUG));
    assert!(!same_exe_path(&exe, &other));
}

// Finding 2 (HIGH, portable mode): build_demo_store must never delete a
// pre-existing folder at the demo subfolder's path unless it carries the
// marker this function itself writes: portable installs put the demo
// subfolder beside the exe, where a user could already have an unrelated
// folder there (named "demo" under the old, generic subfolder name; the
// marker check protects the new app-specific name just as well against
// any other collision).
#[test]
fn build_demo_store_refuses_to_wipe_an_unmarked_existing_folder() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-unmarked-folder", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sentinel = dir.join("not-mine.txt");
    std::fs::write(&sentinel, b"a user's own file, nothing to do with demo mode").unwrap();

    let result = build_demo_store(&dir, None, "de");
    assert!(result.is_err(), "an existing, unmarked folder must never be wiped: {result:?}");
    assert!(sentinel.exists(), "the user's own file must survive");

    let _ = std::fs::remove_dir_all(&dir);
}

// A folder build_demo_store itself created (carries the marker) must still
// be rebuildable, or every enable after the very first would fail outright.
#[test]
fn build_demo_store_rebuilds_its_own_marked_folder() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-marked-folder", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();
    std::fs::write(dir.join("left-over.txt"), b"session leftover").unwrap();

    build_demo_store(&dir, None, "de").unwrap();
    assert!(!dir.join("left-over.txt").exists(), "a marked folder must still be wiped and rebuilt");
    assert!(dir.join(DEMO_MARKER_FILE).is_file(), "the rebuild must leave the marker in place");

    let _ = std::fs::remove_dir_all(&dir);
}

// Finding 3 (N3, second review): a locked file elsewhere in the folder
// (data.db held open by something else) must never cost the marker its
// life: the old order (remove_dir_all the whole folder, including the
// marker, THEN fail on the locked file) could leave a folder that still
// exists but no longer carries proof this app made it, refusing every
// later enable forever.
#[test]
#[cfg(windows)]
fn build_demo_store_keeps_its_marker_when_a_file_is_locked() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = std::env::temp_dir().join(format!("{}-test-demo-locked-file", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();

    // share_mode(0) denies every other handle: including this same
    // process's own attempt to remove or reopen it: any access at all.
    let locked = std::fs::OpenOptions::new().read(true).share_mode(0).open(dir.join("data.db")).unwrap();

    // The rebuild's own data.db step will fail while this handle is open;
    // what matters here is only that the marker survives it regardless.
    let _ = build_demo_store(&dir, None, "de");
    assert!(
        dir.join(DEMO_MARKER_FILE).is_file(),
        "the marker must survive a locked file elsewhere in the folder"
    );

    drop(locked);
    // Once unlocked, a later enable must not be refused forever either.
    assert!(build_demo_store(&dir, None, "de").is_ok(), "a retry once the lock clears must succeed");

    let _ = std::fs::remove_dir_all(&dir);
}

// Finding 3 (MEDIUM): entering demo mode a second time without an exit in
// between (a crash mid-session, or a relaunch that never reached
// exit_demo_mode) must never overwrite an already-recorded backup with the
// demo path itself: once that happens there is nothing left to restore
// the real custom redirect from.
#[test]
fn demo_pointer_backup_survives_a_second_enable_without_an_exit() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-backup-vanish", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();
    let custom = canonical.join("elsewhere");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(canonical.join("datapath"), custom.to_string_lossy().as_bytes()).unwrap();

    enter_demo_mode(&canonical, None).unwrap();
    assert_eq!(
        std::fs::read_to_string(canonical.join(DEMO_POINTER_BACKUP)).unwrap(),
        custom.to_string_lossy(),
        "the first enable must save the real custom redirect as the backup"
    );

    // Re-enter without exiting first: the pointer already points at the
    // demo folder itself.
    enter_demo_mode(&canonical, None).unwrap();
    assert_eq!(
        std::fs::read_to_string(canonical.join(DEMO_POINTER_BACKUP)).unwrap(),
        custom.to_string_lossy(),
        "a second enable must never overwrite the backup with the demo path itself"
    );

    exit_demo_mode(&canonical).unwrap();
    assert_eq!(
        std::fs::read_to_string(canonical.join("datapath")).unwrap(),
        custom.to_string_lossy(),
        "switching off must restore the real custom redirect, not the canonical (empty) store"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// The other half of finding 3: if the backup is missing (an exit already
// ran, or nothing ever wrote one) and the redirect points somewhere that is
// NOT the demo folder, exit_demo_mode must leave it alone rather than
// deleting it and falling back to the canonical (empty) store.
#[test]
fn exit_demo_mode_leaves_an_unrelated_redirect_alone_when_the_backup_is_missing() {
    let canonical = std::env::temp_dir().join(format!("{}-test-demo-no-backup", brand::SLUG));
    let _ = std::fs::remove_dir_all(&canonical);
    std::fs::create_dir_all(&canonical).unwrap();
    let custom = canonical.join("elsewhere");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(canonical.join("datapath"), custom.to_string_lossy().as_bytes()).unwrap();
    assert!(!canonical.join(DEMO_POINTER_BACKUP).exists());

    exit_demo_mode(&canonical).unwrap();

    assert_eq!(
        std::fs::read_to_string(canonical.join("datapath")).unwrap(),
        custom.to_string_lossy(),
        "an unrelated redirect must survive when there is no demo backup to restore"
    );

    let _ = std::fs::remove_dir_all(&canonical);
}

// Finding 5 (LOW): the sample attachment row must never store an absolute
// path: an installed build's data dir sits under
// %APPDATA%\<Windows account name>, and demo mode's whole point is to be
// shown to other people (screenshots, exports).
#[test]
fn demo_attachment_path_is_relative_not_absolute() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-relative-path", brand::SLUG));
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();

    let conn = db_conn_at(&dir).unwrap();
    let mut written = WrittenRows::new();
    let (prompts, ..) = db_load(&conn, &mut written);
    let attachment = prompts.iter().find(|p| p.id == "demo-attachment").unwrap();
    assert!(!attachment.file_path.is_empty());
    assert!(
        Path::new(&attachment.file_path).is_relative(),
        "the stored file_path must be relative, not an absolute path containing the user's own account name: {}",
        attachment.file_path
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// The other half of finding 5 (second review): load_store() resolves the
// demo attachment's relative file_path to a real absolute path in memory
// so the UI can actually use it: but the very next unrelated save must
// never bake that absolute path back into the database. strip_data_dir_prefix
// is what save_prompts (and save_trash/save_prompts_and_trash) run every
// prompt through first for exactly this reason: a full round trip
// (relative on disk -> absolute in memory, simulating load_store's own
// resolution loop -> stripped again before persisting) must land back on
// the same relative value it started from, not the resolved one.
#[test]
fn strip_data_dir_prefix_reverses_load_stores_own_resolution() {
    let dir = std::env::temp_dir().join(format!("{}-test-demo-strip-prefix", brand::SLUG)).join(DEMO_SUBFOLDER);
    let _ = std::fs::remove_dir_all(&dir);
    build_demo_store(&dir, None, "de").unwrap();

    let conn = db_conn_at(&dir).unwrap();
    let mut written = WrittenRows::new();
    let (mut prompts, ..) = db_load(&conn, &mut written);
    let original = prompts.iter().find(|p| p.id == "demo-attachment").unwrap().file_path.clone();
    assert!(Path::new(&original).is_relative(), "sanity: db_load must still hand back the relative row");

    // Mirror load_store()'s own resolution loop.
    for p in prompts.iter_mut() {
        if !p.file_path.is_empty() && Path::new(&p.file_path).is_relative() {
            p.file_path = dir.join(&p.file_path).to_string_lossy().to_string();
        }
    }
    let resolved = prompts.iter().find(|p| p.id == "demo-attachment").unwrap().file_path.clone();
    assert!(Path::new(&resolved).is_absolute(), "sanity: the in-memory copy must now be absolute");

    // What save_prompts runs the resolved prompts through right before
    // db_write_prompts/write_sealed_json ever see them.
    let stripped = strip_data_dir_prefix(&dir, true, &prompts);
    let round_tripped = stripped.iter().find(|p| p.id == "demo-attachment").unwrap().file_path.clone();
    assert_eq!(
        round_tripped, original,
        "a save made after load_store()'s own resolution must never persist the absolute path"
    );
    assert!(
        Path::new(&round_tripped).is_relative(),
        "the persisted value must be relative again: {round_tripped}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// A real (non-demo) attachment's absolute path, whose parent is NOT the
// data dir, must never be touched by strip_data_dir_prefix: it is
// already exactly what the file picker produced.
#[test]
fn strip_data_dir_prefix_leaves_an_unrelated_absolute_path_alone() {
    let dir = std::env::temp_dir().join(format!("{}-test-strip-prefix-unrelated", brand::SLUG));
    let real = Prompt { file_path: "C:\\Users\\someone\\Documents\\report.pdf".to_string(), ..test_prompt("real") };
    let out = strip_data_dir_prefix(&dir, true, std::slice::from_ref(&real));
    assert_eq!(out[0].file_path, real.file_path);
}

// A trashed demo attachment must resolve to an absolute path exactly like
// a live one: a third review finding: load_store used to only run this
// loop over `prompts`, so a demo attachment deleted during the session
// restored from trash with the bare, unresolvable filename db_load_trash
// handed back, instead of a path under the live data dir.
#[test]
fn resolve_relative_file_paths_covers_trash_rows_too() {
    let dir = std::env::temp_dir().join(format!("{}-test-resolve-relative-trash", brand::SLUG));
    let mut trash = vec![Prompt { file_path: "Anhang.txt".to_string(), ..test_prompt("trashed-attachment") }];
    resolve_relative_file_paths(&dir, &mut trash);
    assert_eq!(trash[0].file_path, dir.join("Anhang.txt").to_string_lossy().to_string());
    assert!(Path::new(&trash[0].file_path).is_absolute());
}

// Portable mode's data dir is the exe folder; a custom data dir is common
// too. A real attachment the file picker placed directly inside either
// (its absolute path's parent equals the data dir, same as the demo
// sample row's) must never be stripped down to a bare filename outside
// demo mode: it would resolve against the wrong folder after any later
// data-dir move or portable/installed takeover (a third review finding:
// the fix was gating on dir shape alone, so it fired for every user, not
// only in demo). Named literally `DEMO_SUBFOLDER` on purpose (a fourth
// review finding): the first attempt at this fix compared the folder's
// NAME to `DEMO_SUBFOLDER`, which still misfired for a real data dir
// that happens to be named that: only `is_demo` (the caller's own
// exact-path comparison against `default_data_dir(app).join(DEMO_
// SUBFOLDER)`) may gate this, never the shape of `dir` alone.
#[test]
fn strip_data_dir_prefix_leaves_a_real_attachment_alone_outside_demo() {
    let dir = std::env::temp_dir().join(format!("{}-test-strip-prefix-non-demo", brand::SLUG)).join(DEMO_SUBFOLDER);
    let real = Prompt { file_path: dir.join("report.pdf").to_string_lossy().to_string(), ..test_prompt("real") };
    assert!(Path::new(&real.file_path).parent() == Some(dir.as_path()), "sanity: parent must equal dir");
    let out = strip_data_dir_prefix(&dir, false, std::slice::from_ref(&real));
    assert_eq!(out[0].file_path, real.file_path, "a non-demo data dir must never be stripped, even named exactly like the demo subfolder");
    assert!(Path::new(&out[0].file_path).is_absolute(), "the real attachment path must stay absolute");
}

// Adopting an installed store copies its files one by one. If the database
// arrives and a later file does not (a full disk, a locked file), the folder
// already holds the adopted store while the process still has the empty one it
// replaced. write_rows' "only delete a row I can prove I knew about" guard (the
// allowMultipleInstances fix) now keeps a save with an empty `written` map from
// wiping those adopted rows either way: but the copy still has to report that
// something landed, so the caller reloads from what is really there instead of
// silently carrying on with a stale, now-wrong in-memory empty list.
#[test]
fn a_half_copied_takeover_reports_what_already_landed() {
    let root = std::env::temp_dir().join(format!("{}-test-takeover-partial", brand::SLUG));
    let _ = std::fs::remove_dir_all(&root);
    let src = root.join("installed");
    let dst = root.join("portable");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();

    let mine = numbered_prompts(3);
    let seed_db = |path: &std::path::Path| {
        let mut conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE prompts(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);
                 CREATE TABLE trash(id TEXT PRIMARY KEY, ord INTEGER NOT NULL, data TEXT NOT NULL);",
        )
        .unwrap();
        db_write_prompts(&mut conn, &mine, &mut WrittenRows::new()).unwrap();
    };
    seed_db(&src.join("data.db"));
    std::fs::write(src.join("settings.json"), b"sealed settings").unwrap();
    // A folder where the second file wants to go: the copy fails there, after
    // the database has already arrived.
    std::fs::create_dir_all(dst.join("settings.json")).unwrap();

    let mut landed = false;
    assert!(
        copy_store_files(&src, &dst, &mut landed).is_err(),
        "a file that cannot be written must fail the adoption"
    );
    assert!(landed, "the database arrived before the failure and must be reported as such");

    // With the store left as it was: the empty prompt list plus its (equally
    // empty) row map used to delete every adopted row on the next save. Now
    // write_rows itself refuses: it has no record of ever having seen these
    // rows, so it leaves them alone.
    {
        let mut conn = Connection::open(dst.join("data.db")).unwrap();
        db_write_prompts(&mut conn, &[], &mut WrittenRows::new()).unwrap();
        assert_eq!(
            stored_rows(&conn, "prompts").len(),
            3,
            "an empty written map must not be read as license to delete unseen rows"
        );
    }

    // What reloading first gives instead: the same save keeps every row.
    std::fs::copy(src.join("data.db"), dst.join("data.db")).unwrap();
    let mut conn = Connection::open(dst.join("data.db")).unwrap();
    let mut written = WrittenRows::new();
    let (reloaded, _settings, _mig, fails, _settings_unreadable) = db_load(&conn, &mut written);
    assert_eq!(fails, 0, "the adopted rows must be readable");
    db_write_prompts(&mut conn, &reloaded, &mut written).unwrap();
    assert_eq!(
        stored_rows(&conn, "prompts").iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
        mine.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
        "a save after the reload must leave the adopted rows exactly where they are"
    );

    drop(conn);
    let _ = std::fs::remove_dir_all(&root);
}
