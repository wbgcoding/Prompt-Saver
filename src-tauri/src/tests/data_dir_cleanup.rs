use super::*;

fn touch(p: &Path) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, b"x").unwrap();
}

#[test]
fn cleaning_the_old_data_folder_removes_every_app_entry_and_only_those() {
    let dir = std::env::temp_dir().join(format!("ps-test-clean-old-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for f in [
        "data.db", "data.db-wal", "data.db-shm", "prompts.json", "settings.json",
        "settings.json.tmp", "prompts.json.tmp-p123_4", "data.db.locked.bak", "data.db.locked.170000.bak",
        "export-staging.txt", "datadir.lock.json", "datapath", "backup.psb", "OLD.PSB",
        "backups/2026-01-01_10-00-00/data.db", "files/a.png", "recordings/clip.mp4",
    ] {
        touch(&dir.join(f));
    }
    let foreign = dir.join("my notes.txt");
    touch(&foreign);
    let foreign_sub = dir.join("photos").join("keep.jpg");
    touch(&foreign_sub);
    assert!(remove_app_data_in(&dir, false));
    let left: Vec<String> = fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    let mut left = left;
    left.sort();
    assert_eq!(left, vec!["my notes.txt".to_string(), "photos".to_string()]);
    assert!(foreign.exists() && foreign_sub.exists());
    // A folder emptied this way goes too; one with foreign content stays.
    let empty = std::env::temp_dir().join(format!("ps-test-clean-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&empty);
    touch(&empty.join("data.db"));
    assert!(remove_app_data_in(&empty, false));
    assert!(!empty.exists());
    assert!(dir.exists());
    // The pointer in the canonical folder is the redirect to the new location: it stays.
    let canon = std::env::temp_dir().join(format!("ps-test-clean-canon-{}", std::process::id()));
    let _ = fs::remove_dir_all(&canon);
    touch(&canon.join("datapath"));
    touch(&canon.join("data.db"));
    assert!(remove_app_data_in(&canon, true));
    assert!(canon.join("datapath").exists() && !canon.join("data.db").exists());
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&canon);
}

fn media_test_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ps-test-media-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn prompt_with(file: &str, icon: &str) -> Prompt {
    Prompt { file_path: file.to_string(), icon_path: icon.to_string(), ..Default::default() }
}

fn fwd(s: &str) -> String {
    s.replace(char::from(92), "/")
}

#[test]
fn a_move_copies_attachments_and_recordings_and_runs_the_rebase() {
    let _serial = DDLOCK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = media_test_dir("copy");
    let (current, new, pointer) = (root.join("old"), root.join("new"), root.join("pointer"));
    touch(&current.join("data.db"));
    touch(&current.join("files/a/b.png"));
    touch(&current.join("recordings/clip.mp4"));
    fs::create_dir_all(&pointer).unwrap();
    *LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let mut ran = false;
    let adopted = move_store_dir(&current, &new, &pointer, &mut || Ok(()), &mut || {}, &mut || {
        ran = true;
        Ok(())
    })
    .unwrap();
    assert!(!adopted && ran);
    assert!(new.join("files/a/b.png").is_file() && new.join("recordings/clip.mp4").is_file());
    assert!(new.join("data.db").is_file() && pointer.join("datapath").is_file());
    assert_eq!(LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()).as_deref(), Some(current.as_path()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_failed_media_copy_aborts_before_the_pointer_is_written() {
    let _serial = DDLOCK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = media_test_dir("abort");
    let (current, new, pointer) = (root.join("old"), root.join("new"), root.join("pointer"));
    touch(&current.join("data.db"));
    touch(&current.join("files/a.png"));
    // A plain file where the target folder has to go makes the copy fail.
    touch(&new.join("files"));
    fs::create_dir_all(&pointer).unwrap();
    *LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let mut rebased = false;
    let moved = move_store_dir(&current, &new, &pointer, &mut || Ok(()), &mut || {}, &mut || {
        rebased = true;
        Ok(())
    });
    assert!(moved.is_err());
    assert!(!rebased, "nothing may be rewritten for a move that did not happen");
    assert!(!pointer.join("datapath").exists(), "launches must stay on the old folder");
    assert!(!new.join("data.db").exists(), "a retry must not mistake the half copy for a library");
    assert!(LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()).is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_target_inside_the_media_folders_is_refused() {
    let _serial = DDLOCK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = media_test_dir("inside");
    let (current, pointer) = (root.join("old"), root.join("pointer"));
    touch(&current.join("data.db"));
    touch(&current.join("files/a.png"));
    fs::create_dir_all(&pointer).unwrap();
    let new = current.join("Files").join("nested");
    *LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = None;
    assert!(move_store_dir(&current, &new, &pointer, &mut || Ok(()), &mut || {}, &mut || Ok(())).is_err());
    assert!(!pointer.join("datapath").exists() && LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()).is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_failed_rebase_keeps_the_old_folder_out_of_the_cleanup() {
    let _serial = DDLOCK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = media_test_dir("keep");
    let (current, new, pointer) = (root.join("old"), root.join("new"), root.join("pointer"));
    touch(&current.join("data.db"));
    touch(&current.join("files/a.png"));
    fs::create_dir_all(&pointer).unwrap();
    *LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let moved = move_store_dir(&current, &new, &pointer, &mut || Ok(()), &mut || {}, &mut || Err("locked".to_string()));
    assert_eq!(moved, Ok(false), "the move itself stands");
    assert!(pointer.join("datapath").is_file());
    assert!(LAST_OLD_DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()).is_none(), "the cleanup must be refused");
    assert!(current.join("files/a.png").is_file());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn rebasing_handles_case_and_separator_variants_and_leaves_foreign_paths() {
    let old = Path::new("C:/Users/x/AppData/Roaming/Prompt-Saver");
    let new = Path::new("D:/Data/PS");
    let r = |p: &str| rebase_media_path(p, old, new).map(|s| fwd(&s));
    assert_eq!(r("C:/Users/x/AppData/Roaming/Prompt-Saver/files/a/b.png").as_deref(), Some("D:/Data/PS/files/a/b.png"));
    let back = ["C:", "Users", "x", "AppData", "Roaming", "Prompt-Saver", "files", "b.png"].join(&char::from(92).to_string());
    assert_eq!(r(&back).as_deref(), Some("D:/Data/PS/files/b.png"));
    assert_eq!(r("c:/USERS/X/appdata/roaming/PROMPT-SAVER/Recordings/r.mp4").as_deref(), Some("D:/Data/PS/recordings/r.mp4"));
    assert_eq!(r("C:/Users/x/AppData/Roaming/Prompt-Saver/files").as_deref(), Some("D:/Data/PS/files"));
    assert_eq!(r("C:/Users/x/AppData/Roaming/Prompt-Saver/files/../data.db"), None);
    assert_eq!(r("C:/Users/x/AppData/Roaming/Prompt-Saver/backups/x.psb"), None);
    assert_eq!(r("C:/Users/x/AppData/Roaming/Prompt-Saver-2/files/a.png"), None);
    assert_eq!(r("C:/Users/x/Pictures/files/a.png"), None);
    assert_eq!(r(""), None);
}

#[test]
fn rebasing_the_store_covers_prompts_trash_and_settings_texts() {
    let old = Path::new("C:/data/old");
    let new = Path::new("D:/data/new");
    let mut prompts = vec![prompt_with("C:/data/old/files/a.png", ""), prompt_with("E:/mine/b.png", "c:/DATA/old/files/i.png")];
    let mut trash = vec![prompt_with("C:/data/old/recordings/r.mp4", "")];
    let mut settings = Settings::default();
    settings.ui_texts.insert("recordingDir".to_string(), "C:/data/old/recordings".to_string());
    settings.ui_texts.insert("screenshotDir".to_string(), "E:/shots".to_string());
    let done = rebase_store_media(&mut prompts, &mut trash, &mut settings, old, new);
    assert_eq!(done, MediaRebase { prompts: true, trash: true, settings: true });
    assert_eq!(fwd(&prompts[0].file_path), "D:/data/new/files/a.png");
    assert_eq!(prompts[1].file_path, "E:/mine/b.png");
    assert_eq!(fwd(&prompts[1].icon_path), "D:/data/new/files/i.png");
    assert_eq!(fwd(&trash[0].file_path), "D:/data/new/recordings/r.mp4");
    assert_eq!(fwd(&settings.ui_texts["recordingDir"]), "D:/data/new/recordings");
    assert_eq!(settings.ui_texts["screenshotDir"], "E:/shots");
    // Nothing left to do is reported as nothing to save.
    let again = rebase_store_media(&mut prompts, &mut trash, &mut settings, old, new);
    assert_eq!(again, MediaRebase::default());
}

#[test]
fn saved_editor_state_follows_the_file_to_the_new_data_folder() {
    let conn = edit_state_test_conn();
    let old = Path::new("C:/data/old");
    let new = Path::new("D:/data/new");
    edit_state_write(&conn, "video", "C:/data/old/recordings/r.mp4", 10, 20, "{\"v\":1}", 77).unwrap();
    edit_state_write(&conn, "video", "E:/clips/other.mp4", 1, 2, "{\"v\":2}", 78).unwrap();
    assert_eq!(rebase_edit_state(&conn, old, new), Ok(1));
    let moved = rebase_media_path("C:/data/old/recordings/r.mp4", old, new).unwrap();
    assert_eq!(edit_state_read(&conn, "video", &moved, 10, 20).map(|r| r.1).as_deref(), Some("{\"v\":1}"));
    assert!(edit_state_read(&conn, "video", "C:/data/old/recordings/r.mp4", 10, 20).is_none());
    assert!(edit_state_read(&conn, "video", "E:/clips/other.mp4", 1, 2).is_some());
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM edit_state", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 2);
}
