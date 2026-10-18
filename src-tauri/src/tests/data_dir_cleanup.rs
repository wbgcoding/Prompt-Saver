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
