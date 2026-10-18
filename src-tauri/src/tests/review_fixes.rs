use super::*;

// ---- review fixes: confidential read path, @name list, version-row safety ----

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
fn window_capture_picks_the_monitor_under_its_centre() {
    let mons = [(0, 0, 1920u32, 1080u32), (1920, 0, 1920, 1080), (-1280, 100, 1280, 1024)];
    assert_eq!(monitor_index_for_rect(&mons, 100, 100, 800, 600), 0);
    assert_eq!(monitor_index_for_rect(&mons, 2500, 200, 800, 600), 1);
    assert_eq!(monitor_index_for_rect(&mons, -1000, 300, 600, 400), 2);
    // A window nowhere near any screen keeps the primary capture.
    assert_eq!(monitor_index_for_rect(&mons, 9000, 9000, 100, 100), 0);
}

#[test]
fn folder_import_upgrades_the_retired_placeholder_spelling() {
    let item = folder_file_item("Note".to_string(), vec![], "Title\nHi {#{name}#} on {#{date}#}");
    assert_eq!(item.text, "Hi {?name} on {?@date}");
    let item = folder_file_item("Note".to_string(), vec![], &format!("one{}Dear {{#{{name}}#}}", VARIANT_SEP));
    assert_eq!(item.variants, vec!["Dear {?name}".to_string()]);
}

#[test]
fn sync_poll_adds_an_unplaced_new_file_to_the_view_members() {
    let mut p = sync_test_prompt();
    p.id = "fresh".into();
    let content = prompt_to_md(&p, &SyncPlacement::default());
    let mut store = sync_apply_store(Vec::new(), Vec::new());
    store.settings.views = vec![test_view("v1")];
    store.settings.active_view = "v1".into();
    let (changed, settings_changed) = sync_apply(&mut store, "fresh", content, true);
    assert!(changed && settings_changed);
    let view = store.settings.active_view_mut();
    assert!(view.members.iter().any(|m| m == "fresh"), "the new prompt must belong to the view, not only sit on a cell");
    let key = grid_key(view.cols, view.rows);
    assert!(view.layouts.get(&key).is_some_and(|l| l.contains_key("fresh")));
}

#[test]
fn copy_mic_sidecar_alongside_brings_the_microphone_track_to_the_open_editor_copy() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-mic-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("rec-x-copy.mp4");
    fs::write(&src, b"video").unwrap();
    copy_mic_sidecar_alongside(&src, &dest);
    assert!(!dest.with_extension("mic.wav").exists(), "no sidecar, nothing to copy");
    fs::write(src.with_extension("mic.wav"), b"RIFF").unwrap();
    copy_mic_sidecar_alongside(&src, &dest);
    assert_eq!(fs::read(dest.with_extension("mic.wav")).unwrap(), b"RIFF");
    assert!(src.with_extension("mic.wav").is_file(), "the original stays untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_oversized_mic_sidecar_is_not_copied_to_the_open_editor_copy() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-mic-big-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("copy.mp4");
    fs::write(&src, b"video").unwrap();
    fs::write(src.with_extension("mic.wav"), vec![0u8; 2 * 1024 * 1024]).unwrap();
    copy_mic_sidecar_capped(&src, &dest, 1.0);
    assert!(!dest.with_extension("mic.wav").exists(), "past the ceiling, the sidecar stays behind");
    copy_mic_sidecar_capped(&src, &dest, 4.0);
    assert!(dest.with_extension("mic.wav").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_editor_work_copy_is_recognised_only_inside_its_own_temp_folders() {
    let root = std::env::temp_dir().join(format!("ps-openedit-root-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let work = root.join(format!("{}-openedit-1-2", brand::TEMP_PREFIX));
    let other = root.join("somewhere-else");
    fs::create_dir_all(&work).unwrap();
    fs::create_dir_all(work.join("nested")).unwrap();
    fs::create_dir_all(&other).unwrap();
    let copy = work.join("My clip.mp4");
    let stray = other.join("My clip.mp4");
    let nested = work.join("nested").join("My clip.mp4");
    for f in [&copy, &stray, &nested] {
        fs::write(f, b"v").unwrap();
    }
    assert!(is_editor_work_copy_in(&root, &copy));
    assert!(!is_editor_work_copy_in(&root, &stray), "another folder under the temp root is not a work folder");
    assert!(!is_editor_work_copy_in(&root, &nested), "only the work folder's own files count");
    assert!(!is_editor_work_copy_in(&root, &work.join("missing.mp4")));
    assert!(!is_editor_work_copy_in(&other, &copy), "a different temp root does not match");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn video_folder_cleanup_takes_the_sidecars_of_a_deleted_recording() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-cleanup-sidecars-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let stale = dir.join("rec-old.mp4");
    let fresh = dir.join("rec-new.mp4");
    fs::write(&stale, b"old").unwrap();
    fs::write(&fresh, b"new").unwrap();
    for v in [&stale, &fresh] {
        fs::write(v.with_extension("mic.wav"), b"w").unwrap();
        fs::write(sidecar_path(v, KEYS_SIDECAR_EXT), b"{}").unwrap();
        fs::write(sidecar_path(v, CAM_SIDECAR_EXT), b"{}").unwrap();
    }
    backdate(&stale, Duration::from_secs(40 * 24 * 3600));
    let dir_s = dir.to_string_lossy().to_string();
    assert_eq!(video_folder_cleanup_impl(&dir_s, 30, &["rec-old.mp4".to_string()], &dir), 1);
    assert!(!stale.with_extension("mic.wav").exists());
    assert!(!sidecar_path(&stale, KEYS_SIDECAR_EXT).exists());
    assert!(!sidecar_path(&stale, CAM_SIDECAR_EXT).exists());
    assert!(fresh.with_extension("mic.wav").exists(), "a kept recording keeps its sidecars");
    assert!(sidecar_path(&fresh, KEYS_SIDECAR_EXT).exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn visible_canvas_rect_never_leaves_the_canvas() {
    // Fully inside.
    assert_eq!(visible_canvas_rect(100, 50, 800, 600, 1920, 1080), Some((100, 50, 800, 600)));
    // Hanging over the right and bottom edge: only the visible part counts.
    assert_eq!(visible_canvas_rect(1500, 900, 800, 600, 1920, 1080), Some((1500, 900, 420, 180)));
    // Hanging over the left and top edge.
    assert_eq!(visible_canvas_rect(-300, -100, 1000, 500, 1920, 1080), Some((0, 0, 700, 400)));
    // Entirely off the canvas.
    assert_eq!(visible_canvas_rect(2000, 10, 800, 600, 1920, 1080), None);
    assert_eq!(visible_canvas_rect(-900, 10, 800, 600, 1920, 1080), None);
}

#[test]
fn a_paused_tick_does_not_count_toward_the_recording_minute_limit() {
    let mut elapsed = 0u64;
    for paused in [false, false, true, true, true, false] {
        elapsed += active_tick_secs(paused);
    }
    assert_eq!(elapsed, 3);
    assert!(!recording_limit_reached(elapsed, 0, 1.0, 0.0), "a long pause must not reach the limit");
}

#[test]
fn a_refused_start_removes_the_mic_sidecar_with_the_empty_output() {
    let dir = std::env::temp_dir().join(format!("ps-refused-start-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let out = dir.join("rec-x.mp4");
    fs::write(&out, b"").unwrap();
    fs::write(out.with_extension("mic.wav"), b"RIFF").unwrap();
    recording::discard_start_leftovers(&out);
    assert!(!out.exists());
    assert!(!out.with_extension("mic.wav").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn duplicate_report_view_names_come_from_members_not_old_layouts() {
    let mut a = test_view("a");
    a.members = vec!["p1".into(), "p2".into(), "p1".into()];
    // p3 only sits in an old saved layout of a grid size nobody uses now.
    a.layouts.entry("2x2".into()).or_default().insert("p3".into(), [0, 0]);
    let mut b = test_view("b");
    b.members = vec!["p2".into()];
    let views = vec![a, b];
    let names = view_names_by_prompt(&views);
    assert_eq!(names.get("p1").cloned(), Some(vec!["A".to_string()]));
    assert_eq!(names.get("p2").cloned(), Some(vec!["A".to_string(), "B".to_string()]));
    assert!(!names.contains_key("p3"));
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
fn a_data_wipe_leaves_no_prompt_placed_in_any_view() {
    let mut s = Settings::default();
    let mut v = test_view("a");
    v.members = vec!["p1".into()];
    v.layouts.entry("3x3".into()).or_default().insert("p1".into(), [0, 0]);
    s.views = vec![v];
    s.floating.insert("p1".into(), Pos::default());
    clear_prompt_placements(&mut s);
    assert!(s.views[0].members.is_empty());
    assert!(s.views[0].layouts.is_empty());
    assert!(s.floating.is_empty());
    assert_eq!(s.views.len(), 1, "the view itself stays");
    assert_eq!(first_free_cell_excluding(&s.views[0], &[]), Some([0, 0]), "the first cell is free again");
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
fn a_trim_with_only_an_end_still_limits_the_export_length() {
    assert_eq!(trim_duration_secs(Some(2000), Some(5000)), Some(3.0));
    assert_eq!(trim_duration_secs(None, Some(5000)), Some(5.0));
    assert_eq!(trim_duration_secs(Some(0), Some(5000)), Some(5.0));
    assert_eq!(trim_duration_secs(Some(2000), None), None);
    assert_eq!(trim_duration_secs(Some(5000), Some(5000)), None);
    assert_eq!(trim_duration_secs(Some(6000), Some(5000)), None);
}

#[test]
fn shrinking_a_grid_never_moves_a_folder_onto_one_that_still_fits() {
    // The folder that no longer fits comes FIRST in the list: it must not take the
    // cell the in-bounds one holds.
    let mut folders = vec![
        test_tile_folder("far", "v1", [5, 5], &["a", "b"]),
        test_tile_folder("near", "v1", [0, 0], &["c", "d"]),
        test_tile_folder("other-view", "v2", [9, 9], &["e", "f"]),
    ];
    fit_folders_to_grid(&mut folders, "v1", 3, 3);
    assert_eq!(folders[1].cell, [0, 0], "the folder that fits stays where it was");
    assert_eq!(folders[0].cell, [1, 0], "the moved one takes the first free cell");
    assert_eq!(folders[2].cell, [9, 9], "another view's folder is not touched");
}

#[test]
fn a_folder_edit_that_leaves_another_folder_with_one_member_keeps_that_member_on_the_grid() {
    // "old" held a, b and c; the edit moved a and b into "new", leaving c alone.
    let mut settings = Settings {
        views: vec![test_view("v1")],
        tile_folders: vec![
            test_tile_folder("old", "v1", [1, 0], &["c"]),
            test_tile_folder("new", "v1", [0, 0], &["a", "b"]),
        ],
        ..Settings::default()
    };
    let ids: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    finish_tile_folder_edit(&mut settings, &ids);
    assert_eq!(settings.tile_folders.len(), 1, "the one-member folder is dissolved");
    assert_eq!(settings.tile_folders[0].id, "new");
    let view = &settings.views[0];
    let key = grid_key(view.cols, view.rows);
    assert_eq!(view.layouts.get(&key).and_then(|l| l.get("c")).copied(), Some([1, 0]), "c is a tile again, on the old folder's cell");
}

#[test]
fn taking_a_prompt_out_never_drops_it_on_a_cell_a_folder_keeps() {
    let mut view = test_view("v");
    view.cols = 3;
    view.rows = 1;
    let folders = vec![
        test_tile_folder("origin", "v", [0, 0], &["x", "a", "b"]),
        test_tile_folder("other", "v", [1, 0], &["c", "d"]),
    ];
    let cells: Vec<[u32; 2]> = folders.iter().map(|f| f.cell).collect();
    // Onto another folder's cell, and onto the origin folder that keeps two members.
    assert_eq!(take_out_target_cell(&view, &folders, &cells, "x", [1, 0]), Ok([2, 0]));
    assert_eq!(take_out_target_cell(&view, &folders, &cells, "x", [0, 0]), Ok([2, 0]));
    // A free cell is used as asked.
    assert_eq!(take_out_target_cell(&view, &folders, &cells, "x", [2, 0]), Ok([2, 0]));
    // The origin folder that would dissolve may be dropped on (its last member is settled aside).
    let small = vec![test_tile_folder("origin", "v", [0, 0], &["x", "a"])];
    assert_eq!(take_out_target_cell(&view, &small, &[[0, 0]], "x", [0, 0]), Ok([0, 0]));
    // No free cell at all: refused.
    let mut full = test_view("v");
    full.cols = 2;
    full.rows = 1;
    let two = vec![
        test_tile_folder("origin", "v", [0, 0], &["x", "a", "b"]),
        test_tile_folder("other", "v", [1, 0], &["c", "d"]),
    ];
    let cells: Vec<[u32; 2]> = two.iter().map(|f| f.cell).collect();
    assert_eq!(take_out_target_cell(&full, &two, &cells, "x", [1, 0]), Err(()));
}

// A portable copy whose exe folder cannot take a file must fall back to AppData:
// `create_dir_all` succeeds on an existing read-only folder, so the choice rests
// on a create-and-delete probe that leaves nothing behind.
#[test]
fn dir_is_writable_probes_with_a_real_file() {
    let dir = std::env::temp_dir().join(format!("ps-writable-probe-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    assert!(dir_is_writable(&dir), "a creatable folder is usable");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0, "the probe file is removed again");
    // A path whose parent is a plain file can be neither created nor written to.
    let file = dir.join("plain-file");
    fs::write(&file, b"x").unwrap();
    assert!(!dir_is_writable(&file.join("sub")), "a folder under a file is not usable");
    assert!(!dir_is_writable(&file), "a file is not a usable data folder");
    let _ = fs::remove_dir_all(&dir);
}

// Switching the network filter off must never leave it on after a failed restart:
// the receiver would come back bound beyond loopback right after the user closed it.
#[test]
fn a_failed_narrowing_restart_does_not_restore_the_wide_flag() {
    assert_eq!(remote::flag_after_failed_restart(false, Some(true)), Some(false));
    assert_eq!(remote::flag_after_failed_restart(false, None), Some(false));
    // A failed widening still goes back to what it was.
    assert_eq!(remote::flag_after_failed_restart(true, Some(false)), Some(false));
    assert_eq!(remote::flag_after_failed_restart(true, None), None);
}

// The WebView2 bootstrapper gate runs only a Microsoft-signed file: a valid
// signature from anybody else must not pass.
#[test]
fn only_a_microsoft_signature_passes_the_bootstrapper_gate() {
    assert!(is_microsoft_signer("Microsoft Corporation"));
    assert!(!is_microsoft_signer("Microsoft Corporation Ltd"));
    assert!(!is_microsoft_signer("Evil Corp"));
    assert!(!is_microsoft_signer(""));
    // The unsigned test binary itself is refused.
    let me = std::env::current_exe().unwrap();
    assert!(!signed_by_microsoft(&me));
    // A real Microsoft-signed program, where this machine has one, is accepted.
    for p in [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ] {
        if Path::new(p).exists() {
            assert!(signed_by_microsoft(Path::new(p)), "{p}");
        }
    }
}

// The voice combination outranks a prompt binding in the plan, so a prompt bound to
// it would be stored as fine and never fire: it is refused when set and dropped when
// a backup brings it in, while the voice hotkey is actually registered.
#[test]
fn a_prompt_hotkey_cannot_take_the_active_voice_combination() {
    let others: Vec<&Prompt> = Vec::new();
    assert_eq!(sanitize_hotkey("Ctrl+Alt+V", "", "ctrl+alt+v", "", "", &others), Err("hotkey-taken".into()));
    assert_eq!(sanitize_hotkey("Ctrl+Alt+V", "", "", "", "", &others).unwrap(), "Ctrl+Alt+V");

    let mut s = Settings { voice_hotkey: "Ctrl+Alt+V".to_string(), ..Settings::default() };
    assert_eq!(active_voice_hotkey(&s.voice_hotkey, &s.ui_flags), "", "off until its own switch is on");
    s.ui_flags.insert("voiceHotkey".to_string(), true);
    assert_eq!(active_voice_hotkey(&s.voice_hotkey, &s.ui_flags), "Ctrl+Alt+V");
    s.ui_flags.insert("voice".to_string(), false);
    assert_eq!(active_voice_hotkey(&s.voice_hotkey, &s.ui_flags), "", "and the voice feature itself");
    s.ui_flags.insert("voice".to_string(), true);

    let mut p = sample_prompts().remove(0);
    p.hotkey = "ctrl+alt+v".to_string();
    let mut prompts = vec![p];
    let reserved = reserved_hotkeys(&s);
    let reserved: Vec<&str> = reserved.iter().map(String::as_str).collect();
    assert_eq!(drop_colliding_hotkeys("", &reserved, &mut prompts), 1);
    assert!(prompts[0].hotkey.is_empty());
}

#[test]
fn prune_history_keeps_entries_stored_without_a_timestamp() {
    let now = now_secs();
    let mut settings = Settings {
        copy_log: vec![
            CopyEntry { id: "undated".into(), ts: 0 },
            CopyEntry { id: "fresh".into(), ts: now },
            CopyEntry { id: "old".into(), ts: now - 3 * 86_400 },
        ],
        ..Settings::default()
    };
    settings.ui_values.insert("historyDays".into(), 1.0);
    prune_history(&mut settings);
    let ids: Vec<&str> = settings.copy_log.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, vec!["undated", "fresh"]);
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
fn the_custom_tray_window_shows_an_ampersand_once() {
    assert_eq!(tray_plain_label(&tray_label("A & B", TRAY_LABEL_MAX)), "A & B");
    assert_eq!(tray_plain_label(&tray_label("a && b", TRAY_LABEL_MAX)), "a && b");
}

#[test]
fn a_prompt_placed_on_the_grid_becomes_a_view_member() {
    let mut view: View = serde_json::from_str(r#"{"id":"v","name":"V"}"#).expect("a minimal view");
    view.members = vec!["a".into()];
    let layout: HashMap<String, [u32; 2]> =
        [("a".to_string(), [0, 0]), ("b".to_string(), [1, 0]), ("gone".to_string(), [2, 0])].into_iter().collect();
    let known: std::collections::HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    seat_layout_ids_as_members(&mut view, &layout, &known);
    assert_eq!(view.members, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn a_stale_layout_drops_the_ids_of_deleted_prompts() {
    let layout: HashMap<String, [u32; 2]> =
        [("a".to_string(), [0, 0]), ("gone".to_string(), [1, 0])].into_iter().collect();
    let known: std::collections::HashSet<String> = ["a".to_string()].into_iter().collect();
    let kept = layout_of_known(layout, &known);
    assert_eq!(kept.len(), 1);
    assert!(kept.contains_key("a"));
}

#[test]
fn renaming_a_tag_to_another_casing_keeps_its_colour() {
    let mut prompts = vec![test_prompt("a")];
    prompts[0].tags = vec!["css".to_string()];
    assert!(rename_tag_in(&mut prompts, "css", "CSS"));
    assert_eq!(prompts[0].tags, vec!["CSS".to_string()]);
    let mut colors = HashMap::from([("css".to_string(), "#ff0000".to_string())]);
    transfer_tag_color(&mut colors, "css", "css");
    assert_eq!(colors.get("css").map(String::as_str), Some("#ff0000"));
}

#[test]
fn cancelling_interim_crops_does_not_deadlock_on_their_own_list_lock() {
    let list: Arc<Mutex<InterimCrops>> = Arc::new(Mutex::new(Vec::new()));
    let flag = Arc::new(AtomicBool::new(false));
    let (flag2, list2) = (flag.clone(), list.clone());
    // Mirrors a crop thread: runs until signalled, then takes the list lock to drop its own entry.
    let handle = std::thread::spawn(move || {
        while !flag2.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let me = std::thread::current().id();
        list2.lock().unwrap().retain(|(_, h)| h.thread().id() != me);
    });
    list.lock().unwrap().push((flag, handle));
    let (tx, rx) = std::sync::mpsc::channel();
    let list3 = list.clone();
    std::thread::spawn(move || {
        stop_and_join_interim_crops(&list3);
        let _ = tx.send(());
    });
    assert!(rx.recv_timeout(std::time::Duration::from_secs(10)).is_ok(), "joining the crops hung");
    assert!(list.lock().unwrap().is_empty());
}

#[test]
fn recording_timestamps_never_go_backwards() {
    let mut last = None;
    assert_eq!(recording::strictly_increasing_ts(&mut last, 100), 100);
    // A hold resend stamped ahead of a frame that arrived meanwhile.
    assert_eq!(recording::strictly_increasing_ts(&mut last, 400), 400);
    assert_eq!(recording::strictly_increasing_ts(&mut last, 330), 401);
    assert_eq!(recording::strictly_increasing_ts(&mut last, 401), 402);
    assert_eq!(recording::strictly_increasing_ts(&mut last, 900), 900);
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
fn a_converted_recordings_sidecars_stay_referenced_through_the_sweep() {
    let dir = std::env::temp_dir().join(format!("ps-test-rec-converted-sidecars-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let video = dir.join("2026-09-04_07-00-00.webm");
    let mic = dir.join("2026-09-04_07-00-00.mic.wav");
    let keys = dir.join("2026-09-04_07-00-00.keys.json");
    for f in [&video, &mic, &keys] {
        fs::write(f, b"data").unwrap();
        backdate(f, Duration::from_secs(45 * 60));
    }
    assert_eq!(mic_sidecar_video_path(&mic), Some(video.clone()));
    assert_eq!(keys_sidecar_video_path(&keys), Some(video.clone()));
    let mut referenced = std::collections::HashSet::new();
    referenced.insert(video.to_string_lossy().replace('/', "\\").to_lowercase());
    let (count, _) = sweep_stale_recordings_in(&dir, &referenced, &[], false, Duration::from_secs(600), Some(Duration::from_secs(30 * 60)));
    assert_eq!(count, 0, "the sidecars of a kept converted recording must survive");
    assert!(mic.exists() && keys.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_request_is_judged_by_the_password_and_ranges_in_force_when_it_arrives() {
    let mut settings = Settings::default();
    settings.ui_flags.insert("remoteNetworkFilter".into(), true);
    settings.ui_texts.insert("remoteAllowedRanges".into(), "192.168.1.0/24".into());
    settings.ui_texts.insert("remotePasswordHash".into(), "old-hash".into());
    let peer: std::net::IpAddr = "192.168.1.20".parse().unwrap();
    assert_eq!(remote::live_access(&remote::ReceiverAccess::from_settings(&settings), peer), Some(Some("old-hash".to_string())));
    // The password was regenerated while the connection was open.
    settings.ui_texts.insert("remotePasswordHash".into(), "new-hash".into());
    assert_eq!(remote::live_access(&remote::ReceiverAccess::from_settings(&settings), peer), Some(Some("new-hash".to_string())));
    // The ranges were narrowed past this peer.
    settings.ui_texts.insert("remoteAllowedRanges".into(), "192.168.2.0/24".into());
    assert_eq!(remote::live_access(&remote::ReceiverAccess::from_settings(&settings), peer), None);
    // Loopback stays allowed whatever the ranges say.
    assert!(remote::live_access(&remote::ReceiverAccess::from_settings(&settings), "127.0.0.1".parse().unwrap()).is_some());
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

#[cfg(windows)]
#[test]
fn a_paused_recording_does_not_spend_the_switch_first_frame_budget() {
    assert_eq!(recording::switch_wait_budget_after_tick(2000, true), 2000);
    assert_eq!(recording::switch_wait_budget_after_tick(2000, false), 1990);
    assert_eq!(recording::switch_wait_budget_after_tick(5, false), 0);
}

#[test]
fn an_export_graph_may_only_read_the_source_and_the_media_the_request_names() {
    let dir = std::env::temp_dir().join(format!("ps-export-graph-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec.mp4");
    let badge = dir.join("badge.png");
    let secret = dir.join("notes.txt");
    for p in [&src, &badge, &secret] {
        fs::write(p, b"x").unwrap();
    }
    // A path spelled the way ui/video.js writes it into a graph.
    let ff = |p: &Path| {
        let c = fs::canonicalize(p).unwrap().to_string_lossy().to_string();
        c.trim_start_matches(r"\\?\").replace('\\', "/").replace(':', r"\:")
    };
    let media = vec![badge.to_string_lossy().to_string()];
    let check = |g: &str| validate_export_request_graphs(&src, &media, &[g]);
    // The shapes ui/video.js builds.
    assert!(check("[0:v]fps=30,trim=start=0.000:end=2.000,setpts=PTS-STARTPTS[v0];[v0]scale=320:240,fade=t=in:st=0:d=1.000[vout]").is_ok());
    assert!(check("[0:v]crop=32:32:0:0[vout]").is_ok());
    assert!(check(r"[0:v]drawtext=fontfile='C\:/fonts/a.ttf':x=1:y=2:fontsize=20:fontcolor=0xffffff@1.00:expansion=none:text='hi, there'[vout]").is_ok());
    assert!(check("[0:a]volume=1.00,atempo=1.3[aout]").is_ok());
    assert!(check(&format!("movie='{}'[i0];[0:v][i0]overlay=1:2:enable='between(t,0.000,2.000)'[vout]", ff(&badge))).is_ok());
    assert!(check(&format!("amovie='{}':loop=2[a1];[a1]anull[aout]", ff(&badge))).is_ok());
    assert!(check(&format!("movie='{}'[i0]", ff(&src))).is_ok(), "the source itself is fine");
    assert!(check(&format!("movie=filename='{}'[i0]", ff(&badge))).is_ok());
    // File-reading filters and options, and filters the editor never builds.
    assert!(check(r"[0:v]drawtext=textfile='C\:/Users/x/.ssh/config':fontsize=20[vout]").is_err());
    assert!(check("[0:v]drawtext=fontsize=20:TextFile=x[vout]").is_err());
    assert!(check(r"[0:v]drawtext=/text=C\:/Users/x/secrets.txt[vout]").is_err());
    assert!(check("[0:v]drawtext=fontsize=20:/fontfile=x[vout]").is_err());
    assert!(check("[0:v]scale=/w=x[vout]").is_err());
    assert!(check(r"[0:v]drawtext=a.ttf:hi:C\:/Users/x/secrets.txt[vout]").is_err());
    assert!(check("[0:v]drawtext=a.ttf:hi[vout]").is_ok());
    assert!(check(r"[0:v]subtitles='C\:/x/y.srt'[vout]").is_err());
    assert!(check("[0:v]sendcmd=f=cmds.txt[vout]").is_err());
    assert!(check("[0:v]zmq[vout]").is_err());
    assert!(check(r"[0:v]null[vout];[0:a]ametadata=mode=print:file=C\:/x[aout]").is_err());
    // A movie source that is not the source or a named media file, or has extra options.
    assert!(check(&format!("movie='{}'[i0]", ff(&secret))).is_err());
    assert!(check(r"movie='C\:/Users/x/.ssh/config'[i0]").is_err());
    assert!(check(&format!("movie='{}':lavfi[i0]", ff(&badge))).is_err());
    assert!(check(&format!("movie='{}':format_name=tty[i0]", ff(&badge))).is_err());
    // Quoting cannot hide a second filter: a quoted comma stays part of the value.
    assert!(check("[0:v]scale='iw,textfile=x'[vout]").is_ok());
    assert!(check("[0:v]scale=1:1,movie=x[vout]").is_err());
    // Media named by the request must carry a media extension.
    assert!(validate_export_request_graphs(&src, &[secret.to_string_lossy().to_string()], &["[0:v]null[vout]"]).is_err());
    let _ = fs::remove_dir_all(&dir);
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
fn a_copy_restored_from_a_confidential_entry_is_confidential_and_sealed() {
    let key = [7u8; 32];
    let mut source = test_prompt("src");
    source.secret = true;
    let mut copy = build_restored_prompt(&source, "n", "plain body".to_string(), vec!["plain entry".to_string()], "");
    assert!(!copy.secret);
    seal_restored_prompt(&mut copy, &key);
    assert!(copy.secret);
    assert_ne!(copy.text, "plain body");
    assert_eq!(secret_decrypt(&copy.text, &key).unwrap(), "plain body");
    assert_eq!(secret_decrypt(&copy.variants[0], &key).unwrap(), "plain entry");
}

#[test]
fn the_live_data_folder_is_never_cleaned_whatever_was_remembered() {
    // LAST_OLD_DATA_DIR is one global; the other tests that set it hold the same lock.
    let _serial = DDLOCK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("ps-test-live-dir-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let respelled = PathBuf::from(dir.to_string_lossy().to_uppercase());
    // Remembered as the old folder, but it is the one in use (same folder, other casing).
    assert!(!data_dir_cleanup_allowed(&dir, Some(&dir), &respelled));
    let other = dir.join("elsewhere");
    fs::create_dir_all(&other).unwrap();
    assert!(data_dir_cleanup_allowed(&other, Some(&other), &dir));
    assert!(!data_dir_cleanup_allowed(&other, None, &dir));
    // A move onto the same folder under another spelling is no move at all.
    let mut flushed = false;
    let mut flush = || { flushed = true; Ok(()) };
    let mut rebind = || {};
    *LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = None;
    assert_eq!(move_store_dir(&dir, &respelled, &dir.join("pointer"), &mut flush, &mut rebind, &mut || Ok(())), Ok(false));
    assert!(!flushed);
    assert!(LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()).is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_busy_wal_checkpoint_aborts_the_move_instead_of_losing_rows() {
    let dir = std::env::temp_dir().join(format!("ps-test-wal-busy-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.db");
    let writer = Connection::open(&path).unwrap();
    writer.query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0)).unwrap();
    writer.execute_batch("CREATE TABLE t(a); INSERT INTO t VALUES(1);").unwrap();
    assert!(checkpoint_wal(&writer, 1).is_ok(), "an idle database checkpoints cleanly");
    writer.execute_batch("INSERT INTO t VALUES(2);").unwrap();
    let reader = Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN; SELECT * FROM t;").unwrap();
    writer.execute_batch("INSERT INTO t VALUES(3);").unwrap();
    assert!(checkpoint_wal(&writer, 1).is_err(), "a reader holding the log makes the checkpoint busy");
    reader.execute_batch("COMMIT;").unwrap();
    assert!(checkpoint_wal(&writer, 1).is_ok());
    drop((writer, reader));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_second_hotkey_rearm_asks_the_running_one_to_repeat_instead_of_overlapping() {
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
    let (running, again) = (AtomicBool::new(false), AtomicBool::new(false));
    assert!(claim_rearm(&running, &again), "the first caller runs");
    again.store(false, SeqCst); // the runner clears the request at the top of its loop
    assert!(!claim_rearm(&running, &again), "a caller during a run does not start its own");
    assert!(again.load(SeqCst), "it leaves a request for the runner");
    // The request is stored before the claim: a runner that releases the flag between
    // the two still finds it, so a late caller can never be lost.
    again.store(false, SeqCst);
    running.store(false, SeqCst);
    assert!(claim_rearm(&running, &again), "after the run ended the next caller runs again");
    assert!(again.load(SeqCst), "the request is already in place when the claim is decided");
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
fn a_second_run_under_the_same_op_id_gets_its_own_start_time() {
    let mut ops: HashMap<String, BgOp> = HashMap::new();
    let first_start = entry_for_run(&mut ops, "backup", "backup").started_at_ms;
    ops.get_mut("backup").unwrap().phase = OpPhase::Done;
    ops.get_mut("backup").unwrap().finished_at_ms = Some(now_ms());
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second = entry_for_run(&mut ops, "backup", "backup");
    assert!(second.started_at_ms > first_start, "a finished entry is replaced");
    assert!(second.finished_at_ms.is_none());
    let start = second.started_at_ms;
    // A run still going keeps its entry across progress reports.
    assert_eq!(entry_for_run(&mut ops, "backup", "backup").started_at_ms, start);
}

#[test]
fn start_minimized_never_repoints_another_copys_autostart_entry() {
    assert!(start_minimized_rewrites_entry(true, RegistryOwner::ThisExe));
    assert!(start_minimized_rewrites_entry(true, RegistryOwner::Gone));
    assert!(start_minimized_rewrites_entry(true, RegistryOwner::Missing));
    assert!(!start_minimized_rewrites_entry(true, RegistryOwner::OtherExe));
    assert!(!start_minimized_rewrites_entry(false, RegistryOwner::ThisExe));
}

pub(super) fn part_info(dir: &Path, name: &str) -> RecordingPartInfo {
    let path = dir.join(name);
    fs::write(&path, b"video").unwrap();
    RecordingPartInfo { path: path.to_string_lossy().to_string(), mic_wav_path: None }
}

#[test]
fn result_parts_survive_the_sweep_even_when_old() {
    let dir = std::env::temp_dir().join(format!("ps-test-result-parts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let part = part_info(&dir, "rec-2026-01-01-00-00-00.mp4");
    let part_mic = dir.join("rec-2026-01-01-00-00-00.mic.wav");
    fs::write(&part_mic, b"mic").unwrap();
    let stray = dir.join("rec-2026-01-01-00-00-09.mp4");
    fs::write(&stray, b"video").unwrap();
    for f in [Path::new(&part.path), part_mic.as_path(), stray.as_path()] {
        backdate(f, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    }
    *REC_RESULT_PARTS.lock().unwrap() = vec![part.clone()];
    let protected = protected_take_paths();
    REC_RESULT_PARTS.lock().unwrap().clear();
    let empty = std::collections::HashSet::new();
    let (count, _) = sweep_stale_recordings_in(&dir, &empty, &protected, false, Duration::from_secs(60), None);
    assert_eq!(count, 1, "only the file that is no part of the open take goes");
    assert!(Path::new(&part.path).exists(), "an earlier part waiting in the result dialog stays");
    assert!(part_mic.exists(), "and so does its mic track");
    assert!(!stray.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn place_recording_parts_moves_each_part_with_its_name() {
    let base = std::env::temp_dir().join(format!("ps-test-place-parts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let src = base.join("recordings");
    let dest = base.join("saved");
    fs::create_dir_all(&src).unwrap();
    let mut parts = vec![part_info(&src, "rec-2026-01-01-00-00-00_part2.mp4"), part_info(&src, "notes.mp4")];
    fs::write(src.join("rec-2026-01-01-00-00-00_part2.mic.wav"), b"mic").unwrap();
    let (saved, failed) = place_recording_parts(&mut parts, std::slice::from_ref(&src), &dest);
    assert_eq!((saved, failed), (1, 1), "a file that is no recording of ours is refused");
    assert!(dest.join("rec-2026-01-01-00-00-00_part2.mp4").is_file());
    assert!(dest.join("rec-2026-01-01-00-00-00_part2.mic.wav").is_file(), "the sidecar travels along");
    assert_eq!(parts.len(), 1, "a saved part leaves the list, a failed one stays");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn discard_recording_parts_in_moves_parts_to_the_holding_folder() {
    let dir = std::env::temp_dir().join(format!("ps-test-discard-parts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut parts = vec![part_info(&dir, "rec-2026-01-01-00-00-00_part2.mp4"), part_info(&dir, "rec-2026-01-01-00-00-00_part3.mp4")];
    let moved = discard_recording_parts_in(&mut parts, std::slice::from_ref(&dir));
    assert_eq!(moved, 2);
    assert!(parts.is_empty());
    assert!(dir.join("verworfen").join("rec-2026-01-01-00-00-00_part3.mp4").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn pending_result_file_survives_the_sweep_even_when_old() {
    let dir = std::env::temp_dir().join(format!("ps-test-pending-result-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let shown = dir.join("rec-2026-01-01-00-00-00.mp4");
    let shown_keys = dir.join("rec-2026-01-01-00-00-00.keys.json");
    let other = dir.join("rec-2026-01-01-00-00-05.mp4");
    for f in [&shown, &shown_keys, &other] {
        fs::write(f, b"video").unwrap();
        backdate(f, STALE_TEMP_FILE_AGE + Duration::from_secs(60));
    }
    set_pending_recording(Some(shown.to_string_lossy().to_string()));
    let protected = protected_take_paths();
    set_pending_recording(None);
    assert!(protected_take_paths().iter().all(|p| p != &shown), "closing the dialog releases the file");
    let empty = std::collections::HashSet::new();
    let (count, _) = sweep_stale_recordings_in(&dir, &empty, &protected, false, Duration::from_secs(60), None);
    assert_eq!(count, 1);
    assert!(shown.exists() && shown_keys.exists(), "the file an open dialog shows stays with its sidecar");
    assert!(!other.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn editor_decision_follows_the_editors_answer() {
    use std::sync::mpsc::channel;
    let short = Duration::from_millis(50);
    let (tx, rx) = channel::<String>();
    tx.send("accepted".into()).unwrap();
    assert_eq!(wait_for_editor_decision(&rx, short, short), EditorLoadDecision::Accepted);
    tx.send("refused".into()).unwrap();
    assert_eq!(wait_for_editor_decision(&rx, short, short), EditorLoadDecision::Refused);
    // A page that is still booting never answers.
    assert_eq!(wait_for_editor_decision(&rx, short, short), EditorLoadDecision::NoAnswer);
    // An unsaved-edits question gets the long window; an unanswered one is a refusal.
    tx.send("asking".into()).unwrap();
    tx.send("refused".into()).unwrap();
    assert_eq!(wait_for_editor_decision(&rx, short, short), EditorLoadDecision::Refused);
    tx.send("asking".into()).unwrap();
    assert_eq!(wait_for_editor_decision(&rx, short, short), EditorLoadDecision::Refused);
}

#[test]
fn follow_thread_keeps_the_bar_while_a_stop_is_refused_mid_switch() {
    assert!(follow_stop_settled(true, false, false), "a stop that went through closes the bar");
    assert!(!follow_stop_settled(false, false, true), "a continue in flight keeps it for a retry");
    assert!(!follow_stop_settled(false, true, false), "a session that refused the stop keeps it");
    assert!(follow_stop_settled(false, false, false), "nothing recording any more closes it");
}

#[test]
fn ffmpeg_download_claim_admits_one_caller_at_a_time() {
    static FLAG: AtomicBool = AtomicBool::new(false);
    let first = FlagClaim::take(&FLAG).expect("the first caller gets the claim");
    assert!(FlagClaim::take(&FLAG).is_none(), "a second start is refused while the first runs");
    drop(first);
    assert!(FlagClaim::take(&FLAG).is_some(), "the claim is free again once the holder is done");
}

pub(super) fn bare_import_store() -> Store {
    Store { prompts: Vec::new(), trash: Vec::new(), settings: Settings::default(), prompts_locked: false, written: Default::default() }
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
fn update_waits_for_a_recording_or_a_running_job() {
    assert_eq!(update_blocker(true, false, 0), Some("recording-active"));
    assert_eq!(update_blocker(false, true, 0), Some("recording-active"), "a file switch still counts as recording");
    assert_eq!(update_blocker(false, false, 2), Some("jobs-running"));
    assert_eq!(update_blocker(true, false, 2), Some("recording-active"));
    assert_eq!(update_blocker(false, false, 0), None);
}

#[test]
fn quit_counts_a_live_recording_as_a_running_job() {
    assert_eq!(quit_blockers(0, false), 0, "nothing running quits at once");
    assert_eq!(quit_blockers(0, true), 1, "a recording alone asks first");
    assert_eq!(quit_blockers(2, true), 3);
}

#[test]
fn portable_update_points_at_the_release_page_of_the_offered_version() {
    let url = format!("{}/releases/download/v9.9.9/{}-setup.exe", repo_url(), brand::SLUG);
    assert_eq!(release_page_for(&url), Some(format!("{}/releases/tag/v9.9.9", repo_url())));
    assert_eq!(release_page_for("https://example.com/releases/download/v9.9.9/x-setup.exe"), None, "only our own repo");
    assert_eq!(release_page_for(&format!("{}/releases/download/../x/y.exe", repo_url())), None);
}

#[test]
fn recording_status_stats_the_file_after_the_session_lock_is_released() {
    let src = include_str!("../rec_bar.rs").replace("\r\n", "\n");
    let start = src.find("pub(crate) async fn recording_status(").unwrap();
    let body = &src[start..src[start..].find("#[cfg(not(windows))]").map(|i| start + i).unwrap()];
    let lock_at = body.find("rstate.0.lock()").unwrap();
    let scope_end = body.find("\n    };\n    let bytes").expect("the lock lives in its own block");
    let stat_at = body.find("metadata(&output_path)").expect("stats a copied path");
    assert!(lock_at < scope_end && scope_end < stat_at, "metadata must run after the lock block ends");
    assert!(!body.contains("metadata(&session"), "never stat through the locked session");
}

#[test]
fn export_source_is_allowed_only_for_own_recordings_work_copies_and_the_open_editor_file() {
    let root = std::env::temp_dir().join(format!("ps-exportsrc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let rec_dir = root.join("recs");
    let temp_root = root.join("tmp");
    let work = temp_root.join(format!("{}-openedit-1-2", brand::TEMP_PREFIX));
    let elsewhere = root.join("elsewhere");
    for d in [&rec_dir, &work, &elsewhere] {
        fs::create_dir_all(d).unwrap();
    }
    let rec = rec_dir.join("rec-1.mp4");
    let copy = work.join("clip.mp4");
    let opened = elsewhere.join("opened.mp4");
    let foreign = elsewhere.join("private.mp4");
    for f in [&rec, &copy, &opened, &foreign] {
        fs::write(f, b"v").unwrap();
    }
    let dirs = [rec_dir.clone()];
    let editor = vec![String::new(), opened.to_string_lossy().to_string()];
    assert!(export_source_is_allowed(&dirs, &[], &temp_root, &editor, &rec));
    assert!(export_source_is_allowed(&dirs, &[], &temp_root, &[], &copy), "an editor work copy");
    assert!(export_source_is_allowed(&dirs, &[], &temp_root, &editor, &opened), "the file the editor was handed");
    assert!(!export_source_is_allowed(&dirs, &[], &temp_root, &editor, &foreign), "any other readable video");
    assert!(!export_source_is_allowed(&dirs, &[], &temp_root, &[], &opened), "nothing handed to the editor");
    assert!(!export_source_is_allowed(&dirs, &[], &temp_root, &[String::new()], &foreign));
    assert!(!export_source_is_allowed(&dirs, &[], &temp_root, &editor, &elsewhere.join("gone.mp4")));
    let _ = fs::remove_dir_all(&root);
}

// ---- Settings row and prompt rows written as one unit ----
//
// The writers are stood in for by closures: `settings_fails` makes the settings write return
// false (a real write failure, not a locked row), `rows_called` records whether the prompt
// rows were written at all.

pub(super) fn paired_fixture() -> Store {
    let mut settings = Settings::default();
    settings.migrate();
    Store { prompts: Vec::new(), trash: Vec::new(), settings, prompts_locked: false, written: Default::default() }
}

pub(super) fn paired_save<'a>(settings_fails: bool, rows_fail: bool, rows_called: &'a std::cell::Cell<bool>, undone: &'a std::cell::Cell<bool>) -> impl Fn(&Store, &Settings) -> bool + 'a {
    move |_st, _orig| {
        save_pair_or_undo_first(
            || !settings_fails,
            || {
                rows_called.set(true);
                !rows_fail
            },
            || undone.set(true),
        )
    }
}

#[test]
fn a_locked_settings_row_counts_as_written_for_a_pair_but_a_real_failure_does_not() {
    assert!(settings_pair_written(true, false));
    assert!(settings_pair_written(false, true), "a locked row must not block prompt writes");
    assert!(!settings_pair_written(false, false), "a real write failure undoes the pair");
}

#[test]
fn a_failed_settings_write_leaves_a_new_prompt_out_of_the_store_and_the_grid() {
    let mut store = paired_fixture();
    let before = serde_json::to_string(&store.settings).unwrap();
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    let p = test_prompt("n1");
    let r = add_placed_prompt_core(&mut store, &p, paired_save(true, false, &rows, &undone));
    assert_eq!(r, Err("save-failed".to_string()));
    assert!(store.prompts.is_empty());
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before, "no cell, no member left behind");
    assert!(!rows.get(), "the prompt rows are not written when the settings write failed");
    // And when only the rows fail, the settings written first are taken back.
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    let r = add_placed_prompt_core(&mut store, &p, paired_save(false, true, &rows, &undone));
    assert_eq!(r, Err("save-failed".to_string()));
    assert!(rows.get() && undone.get());
    assert!(store.prompts.is_empty());
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before);
    // Both writes landing keeps the prompt and its seat.
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    add_placed_prompt_core(&mut store, &p, paired_save(false, false, &rows, &undone)).unwrap();
    assert_eq!(store.prompts.len(), 1);
    assert!(store.settings.active_view_mut().members.iter().any(|m| m == "n1"));
}

#[test]
fn a_failed_settings_write_keeps_a_deleted_prompt_in_the_library_with_its_placement() {
    let mut store = paired_fixture();
    let p = test_prompt("d1");
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    add_placed_prompt_core(&mut store, &p, paired_save(false, false, &rows, &undone)).unwrap();
    store.settings.floating.insert("d1".into(), Pos { x: 1, y: 2, ..Default::default() });
    let before = serde_json::to_string(&store.settings).unwrap();
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    let r = delete_prompt_core(&mut store, "d1", 9, paired_save(true, false, &rows, &undone));
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(store.prompts.len(), 1);
    assert!(store.trash.is_empty());
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before, "view, cell and pill still name the prompt");
    assert!(!rows.get());
    // The batch variant behaves the same.
    let ids = vec!["d1".to_string()];
    let r = delete_batch_core(&mut store, &ids, 9, paired_save(true, false, &rows, &undone));
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(store.prompts.len(), 1);
    assert!(store.trash.is_empty());
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before);
    // A pair that lands moves it to the trash.
    assert_eq!(delete_batch_core(&mut store, &ids, 9, paired_save(false, false, &rows, &undone)), Ok(true));
    assert!(store.prompts.is_empty() && store.trash.len() == 1);
}

#[test]
fn a_failed_settings_write_leaves_a_restored_prompt_in_the_trash() {
    let mut store = paired_fixture();
    let mut p = test_prompt("t1");
    p.trashed_at = 5;
    store.trash.push(p);
    let before = serde_json::to_string(&store.settings).unwrap();
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    let r = restore_trashed_core(&mut store, "t1", |st| paired_save(true, false, &rows, &undone)(st, &Settings::default()));
    assert_eq!(r, Err("save-failed".to_string()));
    assert_eq!(store.trash.len(), 1);
    assert_eq!(store.trash[0].trashed_at, 5);
    assert!(store.prompts.is_empty());
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before);
    assert!(!rows.get());
}

#[test]
fn a_failed_settings_write_seeds_no_example_prompts() {
    let mut store = paired_fixture();
    let before = serde_json::to_string(&store.settings).unwrap();
    let (rows, undone) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
    let created = vec![test_prompt("e1"), test_prompt("e2")];
    let r = seed_examples_core(&mut store, created.clone(), paired_save(true, false, &rows, &undone));
    assert_eq!(r.err(), Some("save-failed".to_string()));
    assert!(store.prompts.is_empty());
    assert!(!store.settings.examples_seeded);
    assert_eq!(serde_json::to_string(&store.settings).unwrap(), before);
    assert!(!rows.get(), "nothing reached the prompt table, so a retry cannot duplicate");
    let seeded = seed_examples_core(&mut store, created, paired_save(false, false, &rows, &undone)).unwrap();
    assert_eq!(seeded.len(), 2);
    assert!(store.settings.examples_seeded && store.prompts.len() == 2);
}

#[test]
fn leftover_picture_text_is_cleared_at_boot_only_when_the_index_is_off_and_the_set_is_writable() {
    let _epoch = OCR_EPOCH_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = secret_test_store(vec![image_text_prompt("p1", "read from a picture")]);
    store.settings.ui_flags.insert("ocrIndex".into(), true);
    assert!(!heal_ocr_text_at_boot(&mut store), "index on: the text is wanted");
    assert!(!store.prompts[0].image_text.is_empty());

    store.settings.ui_flags.insert("ocrIndex".into(), false);
    store.prompts_locked = true;
    assert!(!heal_ocr_text_at_boot(&mut store), "locked prompt set is never rewritten");
    assert!(!store.prompts[0].image_text.is_empty());
    store.prompts_locked = false;

    store.settings.ui_flags.insert("demoMode".into(), true);
    assert!(!heal_ocr_text_at_boot(&mut store), "demo mode keeps its sample library");
    assert!(!store.prompts[0].image_text.is_empty());
    store.settings.ui_flags.remove("demoMode");

    assert!(heal_ocr_text_at_boot(&mut store), "index off with stored text: cleared");
    assert!(store.prompts[0].image_text.is_empty());
    assert!(!heal_ocr_text_at_boot(&mut store), "nothing left to clear");
}

#[test]
fn clip_region_to_frame_keeps_the_pixels_that_lie_on_the_monitor() {
    // Inside: unchanged.
    assert_eq!(recording::clip_region_to_frame(100, 50, 400, 300, 1920, 1080), Some((100, 50, 400, 300)));
    // Negative origin: the visible part keeps its size minus the overhang.
    assert_eq!(recording::clip_region_to_frame(-100, -20, 400, 300, 1920, 1080), Some((0, 0, 300, 280)));
    // Overhang past the right/bottom edge.
    assert_eq!(recording::clip_region_to_frame(1800, 1000, 400, 300, 1920, 1080), Some((1800, 1000, 120, 80)));
    // No overlap at all.
    assert_eq!(recording::clip_region_to_frame(2000, 0, 100, 100, 1920, 1080), None);
    assert_eq!(recording::clip_region_to_frame(-500, 0, 100, 100, 1920, 1080), None);
    assert_eq!(recording::clip_region_to_frame(0, 0, 0, 100, 1920, 1080), None);
}

#[test]
fn crop_and_flip_rows_bottom_up_copies_the_overlap_of_a_crop_that_overhangs_the_frame() {
    // 2x2 frame, pixels 1..4 (first byte distinct), crop 3 wide starting at x=1.
    let raw: Vec<u8> = vec![1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
    let out = crop_and_flip_rows_bottom_up(&raw, 2, 2, 8, 1, 0, 3, 2);
    // Bottom-up: row 1 (pixel 4) first, then row 0 (pixel 2); the missing columns stay black.
    assert_eq!(out, vec![4, 4, 4, 4, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn record_clock_never_steps_backwards_while_another_thread_pauses_and_resumes() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let clock = std::sync::Arc::new(RecordClock::new());
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let toggler = {
        let (clock, stop) = (clock.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                clock.pause();
                clock.resume();
            }
        })
    };
    let mut last = 0i64;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_millis(400) {
        let now = clock.now_ticks();
        assert!(now >= last, "ticks went back from {last} to {now}");
        last = now;
    }
    stop.store(true, Ordering::SeqCst);
    toggler.join().unwrap();
}

#[test]
fn session_messages_only_stop_a_recording_when_the_session_really_ends() {
    use sessionend::{session_action, SessionAction};
    const QUERY: u32 = 0x0011;
    const END: u32 = 0x0016;
    assert_eq!(session_action(QUERY, 0), SessionAction::FlushOnly);
    assert_eq!(session_action(QUERY, 1), SessionAction::FlushOnly);
    assert_eq!(session_action(END, 1), SessionAction::StopAndFlush);
    assert_eq!(session_action(END, 0), SessionAction::None);
    assert_eq!(session_action(0x0001, 1), SessionAction::None);
}

#[test]
fn export_dest_is_usable_refuses_device_names_bad_characters_and_the_source_itself() {
    let dir = std::env::temp_dir().join(format!("ps-batch-dest-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("clip.mp4");
    std::fs::write(&src, b"x").unwrap();
    assert!(export_dest_is_usable(&dir.join("out.mp4"), &src));
    assert!(!export_dest_is_usable(&dir.join("CON"), &src));
    assert!(!export_dest_is_usable(&dir.join("bad?.mp4"), &src));
    assert!(!export_dest_is_usable(&dir.join("CLIP.MP4"), &src));
    assert!(!export_dest_is_usable(&src, &src));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_destroyed_float_window_leaves_the_closing_list_and_resyncs_only_after_a_gave_up_waiter() {
    let mut list = vec![("a".to_string(), CloseWatch::Idle), ("b".to_string(), CloseWatch::Idle), ("c".to_string(), CloseWatch::Idle)];
    // Never waited on: forgotten, nothing to replay.
    assert!(!watch_destroyed(&mut list, "a"));
    // The waiter is still polling and will replay the plan itself.
    assert!(watch_claim(&mut list, "b"));
    assert!(!watch_claim(&mut list, "b"));
    assert!(!watch_destroyed(&mut list, "b"));
    // The waiter gave up with the window alive: the Destroyed event replays once.
    assert!(watch_claim(&mut list, "c"));
    assert!(watch_give_up(&mut list, "c"));
    assert!(!watch_claim(&mut list, "c"));
    assert!(watch_destroyed(&mut list, "c"));
    assert!(list.is_empty());
    assert!(!watch_destroyed(&mut list, "c"));
    // The window vanished before the waiter gave up: the waiter replays itself.
    assert!(!watch_give_up(&mut list, "c"));
}

#[test]
fn reopen_step_builds_only_when_the_label_is_free_and_waits_for_a_closing_window() {
    assert_eq!(reopen_step(false, false), ReopenStep::Build);
    assert_eq!(reopen_step(false, true), ReopenStep::Build);
    assert_eq!(reopen_step(true, false), ReopenStep::Keep);
    assert_eq!(reopen_step(true, true), ReopenStep::WaitThenSync);
}

// ---- the vault key is wiped when its session ends ----

#[test]
fn a_dropped_secret_session_leaves_no_key_bytes() {
    let mut sess = std::mem::ManuallyDrop::new(SecretSession {
        key: [0xA5; 32],
        unlocked_at: std::time::Instant::now(),
    });
    unsafe { std::mem::ManuallyDrop::drop(&mut sess) };
    assert_eq!(sess.key, [0u8; 32], "the key must be zeroed on drop");
}

// ---- a screenshot re-encode refuses a header that promises a huge picture ----

#[test]
fn reencode_refuses_a_png_header_claiming_huge_dimensions() {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(b"IHDR");
    ihdr.extend_from_slice(&60_000u32.to_be_bytes());
    ihdr.extend_from_slice(&60_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut crc = flate2::Crc::new();
    crc.update(&ihdr);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(&ihdr);
    png.extend_from_slice(&crc.sum().to_be_bytes());
    let began = std::time::Instant::now();
    assert!(reencode_screenshot_bytes(&png, "png", 90).is_err());
    assert!(began.elapsed() < Duration::from_secs(2), "took {:?}", began.elapsed());
}

// ---- a take that cannot reach the holding area leaves no sidecar behind ----

#[test]
fn a_removed_take_takes_every_sidecar_with_it() {
    let dir = std::env::temp_dir().join(format!("ps-take-sidecars-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    // A file where the holding folder would go makes the move fail, so the take is removed.
    fs::write(dir.join("verworfen"), b"").unwrap();
    let video = dir.join("rec-take.mp4");
    fs::write(&video, b"v").unwrap();
    for ext in ["mic.wav", KEYS_SIDECAR_EXT, CAM_SIDECAR_EXT] {
        fs::write(sidecar_path(&video, ext), b"s").unwrap();
    }
    discard_or_remove_take(&video);
    assert!(!video.exists());
    for ext in ["mic.wav", KEYS_SIDECAR_EXT, CAM_SIDECAR_EXT] {
        assert!(!sidecar_path(&video, ext).exists(), "{ext} was left behind");
    }
    let _ = fs::remove_dir_all(&dir);
}

// ---- a confidential copy carries the clipboard history and cloud opt-outs ----

#[cfg(windows)]
#[test]
fn only_a_confidential_copy_opts_out_of_clipboard_history() {
    assert!(clipwatch::private_formats(false).is_empty());
    let secret = clipwatch::private_formats(true);
    for name in ["CanIncludeInClipboardHistory", "CanUploadToCloudClipboard", "ExcludeClipboardContentFromMonitorProcessing"] {
        assert!(secret.contains(&name), "{name} missing");
    }
}

// ---- an update installs only with the publisher's signature ----

#[test]
fn an_update_signature_must_match_the_digest_and_the_key() {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let key = b64.encode(ring::signature::KeyPair::public_key(&pair).as_ref());
    let digest = [7u8; 32];
    let sig = b64.encode(pair.sign(&digest).as_ref());
    assert!(update_signature_valid(&digest, &sig, &key));
    let mut other = digest;
    other[0] ^= 1;
    assert!(!update_signature_valid(&other, &sig, &key));
    assert!(!update_signature_valid(&digest, "", &key));
    assert!(!update_signature_valid(&digest, &sig, UPDATE_PUBLIC_KEY_B64));
}

// The helper script is a batch file: `%NAME%` expands even inside quotes, so a user folder
// named with percent signs must not run an environment lookup, and a quote cannot be escaped.
#[test]
fn update_script_escapes_percent_and_refuses_quotes() {
    let s = update_script_content(Path::new("C:\\t\\%TEMP%\\setup.exe"), Path::new("C:\\a\\x.exe")).unwrap();
    assert!(s.contains("\"C:\\t\\%%TEMP%%\\setup.exe\" /S"));
    assert!(s.contains("del \"%~f0\""));
    // The unique stage folder does not stay behind in the temp folder.
    assert!(s.trim_end().ends_with("rmdir \"C:\\t\\%%TEMP%%\""));
    assert!(update_script_content(Path::new("C:\\t\\a\"b.exe"), Path::new("C:\\a\\x.exe")).is_err());
    assert!(update_script_content(Path::new("C:\\t\\s.exe"), Path::new("C:\\a\nx.exe")).is_err());
}

// The renderer may send anything as a version to skip; only version-shaped text is stored
// and the list stays bounded.
#[test]
fn the_skip_list_takes_only_version_shaped_text_and_stays_bounded() {
    let mut list = Vec::new();
    assert!(add_skipped_version(&mut list, "1.2.3".to_string()));
    assert!(!add_skipped_version(&mut list, "1.2.3".to_string()));
    for bad in ["", "abc", "1.2.3
", &"1".repeat(33), "<script>"] {
        assert!(!add_skipped_version(&mut list, bad.to_string()), "{bad:?}");
    }
    for n in 0..40 {
        add_skipped_version(&mut list, format!("2.{n}.0"));
    }
    assert_eq!(list.len(), 20);
    assert_eq!(list.last().unwrap(), "2.39.0");
    assert!(!list.contains(&"1.2.3".to_string()));
}

#[test]
fn the_app_placing_a_parked_pill_is_not_a_user_drag() {
    // open_floating's own set_position fires Moved too; counting it as a drag wrote the park
    // spot over the stored home and ended parking.
    let mut placed = vec![("float-a".to_string(), (40, 60))];
    assert!(!moved_is_user_drag(&mut placed, "float-a", (40, 60)), "the app's own placement");
    assert!(!moved_is_user_drag(&mut placed, "float-a", (40, 60)), "a repeated event for it");
    assert!(moved_is_user_drag(&mut placed, "float-a", (45, 61)), "a real drag");
    assert!(moved_is_user_drag(&mut placed, "float-a", (40, 60)), "after a drag the old spot counts again");
    assert!(moved_is_user_drag(&mut placed, "float-b", (40, 60)), "a window the app never parked");
}
#[test]
fn a_maximized_window_keeps_its_normal_bounds() {
    // Moved/Resized used to store the full work-area bounds, so quitting while maximized
    // restored a non-maximized window at that size and lost the user's own.
    let mut g = WindowGeom { x: 100, y: 80, width: 900, height: 600, maximized: false };
    geom_after_move(&mut g, 0, 0, true);
    geom_after_resize(&mut g, 1920, 1040, true);
    assert!(g.maximized);
    assert_eq!((g.x, g.y, g.width, g.height), (100, 80, 900, 600));
    geom_after_resize(&mut g, 820, 540, false);
    geom_after_move(&mut g, 120, 90, false);
    assert!(!g.maximized);
    assert_eq!((g.x, g.y, g.width, g.height), (120, 90, 820, 540));
}
#[test]
fn old_saved_geometry_without_the_flag_loads_as_not_maximized() {
    let g: WindowGeom = serde_json::from_str(r#"{"x":1,"y":2,"width":3,"height":4}"#).unwrap();
    assert!(!g.maximized);
}
#[test]
fn the_quit_save_does_not_wait_behind_a_busy_store() {
    // do_full_exit used a blocking lock on the event-loop thread; a backup holding the store
    // froze every window. With the lock held the helper must report busy instead of waiting.
    let m = Mutex::new(7);
    let guard = m.lock().unwrap();
    let mut ran = false;
    assert!(!with_store_if_free(&m, |_| ran = true));
    assert!(!ran);
    drop(guard);
    assert!(with_store_if_free(&m, |v| assert_eq!(*v, 7)));
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
