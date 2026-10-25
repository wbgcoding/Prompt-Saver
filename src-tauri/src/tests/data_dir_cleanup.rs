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

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn moving_the_data_folder_is_refused_while_a_recording_runs() {
    let src = source!("../snip.rs");
    let at = src.find("pub(crate) async fn set_data_dir").unwrap();
    let body = &src[at..];
    let rec = body.find("recording_active(&app) || REC_STARTING.load(Ordering::SeqCst)").expect("recording check");
    let grant = body.find("consume_granted_dir(Path::new(trimmed), \"data\")").unwrap();
    assert!(rec < grant, "the pick must survive a refusal");
    assert!(body.contains("\"data-move-recording\""));
    // A code of its own: the update message for the same running recording would mislead here.
    let map = source!("../../../ui/errors.js");
    assert!(map.contains("\"data-move-recording\": \"errDataMoveRecording\""));
    assert!(map.contains("\"recording-active\": \"errUpdateRecordingActive\""));
    for lang in ["en", "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro"] {
        let text = fs::read_to_string(format!("{}/../ui/i18n/{lang}.js", env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(text.contains("    errDataMoveRecording: \""), "{lang}");
    }
}

#[test]
fn the_data_folder_move_runs_off_the_runtime_and_one_at_a_time() {
    let src = source!("../snip.rs");
    let at = src.find("pub(crate) async fn set_data_dir").unwrap();
    let body = &src[at..];
    let body = &body[..body.find("fn move_data_dir_blocking").unwrap()];
    let busy = body.find("\"data-move-busy\"").expect("a second call is refused");
    let grant = body.find("consume_granted_dir").unwrap();
    let spawn = body.find("tauri::async_runtime::spawn_blocking").expect("disk work is off the runtime");
    assert!(busy < grant && grant < spawn);
    assert!(body.contains("let _guard = guard;"), "the guard is released when the work ends");
    // The step order inside the blocking part is the one the abort rules rely on.
    let work = &src[src.find("fn move_data_dir_blocking").unwrap()..];
    let work = &work[..work.find("\n}\n").unwrap()];
    let flush = work.find("let mut flush").unwrap();
    let rebase = work.find("let mut rebase").unwrap();
    let mv = work.find("move_store_dir(&current, &new").unwrap();
    assert!(flush < rebase && rebase < mv);
    // The guard really is exclusive.
    assert!(DATA_MOVE_BUSY.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok());
    {
        let _g = DataMoveGuard;
        assert!(DATA_MOVE_BUSY.load(Ordering::SeqCst));
    }
    assert!(!DATA_MOVE_BUSY.load(Ordering::SeqCst), "dropping the guard frees the slot");
    let map = source!("../../../ui/errors.js");
    assert!(map.contains("\"data-move-busy\": \"errDataMoveBusy\""));
    let menu = source!("../../../ui/expertMenu.js");
    assert!(menu.contains("dataDirMoving = true;") && menu.contains("dataDirMoving = false;"));
    assert!(menu.contains("btn.disabled = true") && menu.contains("btn.disabled = false"));
    for lang in ["en", "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro"] {
        let text = fs::read_to_string(format!("{}/../ui/i18n/{lang}.js", env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(text.contains("    errDataMoveBusy: \""), "{lang}");
    }
}

#[test]
fn the_backups_self_copy_guard_ignores_case() {
    let src = source!("../snip.rs");
    assert!(src.contains("old_backups.is_dir() && !path_is_inside_dir(new, &old_backups)"));
    assert!(!src.contains("!new.starts_with(&old_backups)"));
    let old = Path::new(r"C:\Data\backups");
    assert!(path_is_inside_dir(Path::new(r"c:\data\BACKUPS\sub"), old));
    assert!(!path_is_inside_dir(Path::new(r"C:\Data\backups2"), old));
}

#[test]
fn a_verbatim_share_is_the_same_place_as_the_plain_one() {
    assert_eq!(split_path_parts(r"\\?\UNC\nas\ps\data"), split_path_parts(r"\\nas\ps\data"));
    assert_eq!(split_path_parts(r"\\?\unc\NAS\ps"), vec!["NAS".to_string(), "ps".to_string()]);
    assert_eq!(split_path_parts(r"\\?\C:\x\y"), split_path_parts(r"C:\x\y"));
    // A folder that merely starts with "unc" is no prefix.
    assert_eq!(split_path_parts("uncle/x"), vec!["uncle".to_string(), "x".to_string()]);
    let data = Path::new(r"\\nas\ps\data");
    assert!(path_is_inside_dir(Path::new(r"\\?\UNC\nas\ps\data\files\a.pdf"), data));
    assert!(!path_is_inside_dir(Path::new(r"\\?\UNC\nas\ps\data2\a.pdf"), data));
    let own = [PathBuf::from(r"\\nas\ps\data")];
    assert!(!is_foreign_network_path(r"\\?\UNC\nas\ps\data\files\a.pdf", &own));
    assert!(is_foreign_network_path(r"\\?\UNC\nas\ps\other\a.pdf", &own));
    assert!(is_foreign_network_path(r"\\?\UNC\evil\ps\data\a.pdf", &own));
    assert!(is_foreign_network_path(r"\\?\UNC\nas\ps\data\..\other\a.pdf", &own));
}

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
fn the_startup_sweep_removes_only_old_temp_leftovers() {
    let dir = std::env::temp_dir().join(format!("ps-tmpsweep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let old_time = SystemTime::now() - Duration::from_secs(7200);
    let make = |name: &str, old: bool| {
        let f = fs::File::create(dir.join(name)).unwrap();
        if old {
            f.set_modified(old_time).unwrap();
        }
    };
    make("settings.json.tmp-p1760000000000000000_1", true);
    make("a.md.tmp-p1760000000000000000_2", true);
    make("fresh.json.tmp-p1760000000000000000_3", false);
    make("notes.tmp", true);
    make("export.psb", true);
    make("mine.tmp-abc", true);
    fs::create_dir(dir.join("sub.tmp-p1_1")).unwrap();
    assert_eq!(sweep_atomic_temps(&dir, Duration::from_secs(3600)), 2);
    let mut left: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["export.psb", "fresh.json.tmp-p1760000000000000000_3", "mine.tmp-abc", "notes.tmp", "sub.tmp-p1_1"]);
    let _ = fs::remove_dir_all(&dir);
}

// Structural fix, no red unit test: the sweep results stay the same, only the directory reads drop.
#[test]
fn the_startup_sweep_reads_the_app_temp_folder_once() {
    let src = source!("../snip.rs");
    let body = src.split("pub(crate) fn sweep_stale_temp_files_in").nth(1).unwrap().split("pub(crate) fn remove_own_temp_leftovers_in").next().unwrap();
    assert_eq!(body.matches("fs::read_dir(&ocr_dir)").count(), 1);
}
