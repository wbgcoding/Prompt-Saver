use super::*;

// ---- recording, media and clipboard fixes ----

#[cfg(windows)]
#[test]
fn a_locked_vault_delivers_no_confidential_text() {
    assert_eq!(clipwatch::deliverable_text(true, false, "secret"), "");
    assert_eq!(clipwatch::deliverable_text(true, true, "secret"), "secret");
    assert_eq!(clipwatch::deliverable_text(false, false, "plain"), "plain");
}

#[test]
fn a_shutdown_during_voice_prepare_cancels_the_pending_start() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let (claiming, cancel) = (AtomicBool::new(true), AtomicBool::new(false));
    voice_cmds::mark_pending_start_cancelled(&claiming, &cancel);
    assert!(cancel.load(Ordering::SeqCst), "a start that is still preparing must be cancelled");
    let (idle, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    voice_cmds::mark_pending_start_cancelled(&idle, &cancel);
    assert!(!cancel.load(Ordering::SeqCst), "no pending start, nothing to cancel");
}

#[test]
fn the_editor_opens_only_known_files() {
    let root = std::env::temp_dir().join(format!("ps-editopen-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let rec_dir = root.join("recs");
    let elsewhere = root.join("elsewhere");
    for d in [&rec_dir, &elsewhere] {
        fs::create_dir_all(d).unwrap();
    }
    let attached = elsewhere.join("clip.mp4");
    let foreign = elsewhere.join("private.mp4");
    for f in [&attached, &foreign] {
        fs::write(f, b"v").unwrap();
    }
    let mut p = test_prompt("a");
    p.file_path = attached.to_string_lossy().to_string();
    let paths = attached_media_paths(&[p, test_prompt("b")]);
    assert_eq!(paths, vec![attached.to_string_lossy().to_string()], "empty paths are not sources");
    let dirs = [rec_dir];
    let temp_root = root.join("tmp");
    assert!(export_source_is_allowed(&dirs, &temp_root, &paths, &attached));
    assert!(!export_source_is_allowed(&dirs, &temp_root, &paths, &foreign), "an arbitrary readable file");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn only_clip_export_files_go_to_the_clipboard() {
    let root = std::env::temp_dir().join(format!("ps-clipexp-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let own = root.join("own");
    let other = root.join("other");
    for d in [&own, &other] {
        fs::create_dir_all(d).unwrap();
    }
    let name = format!("{}-clipexport-1.mp4", brand::TEMP_PREFIX);
    let good = own.join(&name);
    let wrong_dir = other.join(&name);
    let wrong_name = own.join("data.db");
    for f in [&good, &wrong_dir, &wrong_name] {
        fs::write(f, b"x").unwrap();
    }
    assert!(is_clip_export_file(&own, &good));
    assert!(!is_clip_export_file(&own, &wrong_dir), "right name in another folder");
    assert!(!is_clip_export_file(&own, &wrong_name), "some other file in the temp folder");
    assert!(!is_clip_export_file(&own, &own.join(format!("..{}other{}{}", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR, name))));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn native_helpers_live_in_the_per_user_app_folder() {
    let base = std::ffi::OsString::from("C:/Users/x/AppData/Local");
    let dir = local_app_dir_in(Some(base)).unwrap();
    assert!(dir.starts_with("C:/Users/x/AppData/Local") && dir.ends_with(brand::DATA_FOLDER));
    assert!(local_app_dir_in(None).is_none());
    assert!(local_app_dir_in(Some(std::ffi::OsString::new())).is_none());
    if let Some(p) = pdfium_cache_path() {
        assert!(!p.starts_with(std::env::temp_dir()), "the shared temp folder is not trusted for a loaded DLL");
    }
}

#[test]
fn media_tools_are_searched_only_in_app_owned_folders() {
    let exe = PathBuf::from("C:/app");
    let local = PathBuf::from("C:/local/app");
    assert_eq!(media_tool_dirs(Some(exe.clone()), Some(local.clone())), vec![exe.clone(), local.clone()]);
    assert_eq!(media_tool_dirs(None, Some(local.clone())), vec![local]);
    assert!(media_tool_dirs(None, None).is_empty());
    // The data folder is not a parameter any more: a planted copy there cannot be found.
    let src = include_str!("../ffmpeg.rs");
    let discovery = src.split("pub(crate) fn find_media_tool").nth(1).unwrap().split("pub(crate) fn ffmpeg_search_path").next().unwrap();
    assert!(!discovery.contains("data_dir"));
}

#[test]
fn a_network_path_is_refused_before_any_file_check() {
    let root = std::env::temp_dir().join(format!("ps-recloc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let allowed = [root.clone()];
    let missing = root.join("rec-gone.mp4");
    assert!(is_recording_source_location(&allowed, &missing), "the location check never needs the file");
    assert!(!is_own_recording_source(&allowed, &missing));
    assert!(!is_recording_source_location(&allowed, Path::new(r"\host\share\rec-1.mp4")));
    assert!(!is_recording_source_location(&allowed, Path::new("//host/share/rec-1.mp4")));
    let _ = fs::remove_dir_all(&root);
}

// Structural fix (the overlay window cannot be built without a running app), so the check
// reads the source: a failed overlay build must end the whole session, not just clear a flag.
#[test]
fn a_failed_overlay_build_ends_the_whole_snip_session() {
    let src = include_str!("../snip.rs");
    let arm = src.split("match build_snip_window(&app) {").nth(1).unwrap().split("Ok(w) => w,").nth(1).unwrap();
    let arm = arm.split("return Err(e);").next().unwrap();
    assert!(arm.contains("close_all_snip(&app)"));
    assert!(!arm.contains("SNIP_ACTIVE.store(false"), "clearing one flag leaves the rest of the session behind");
}

#[test]
fn a_new_recording_never_takes_an_existing_name() {
    let root = std::env::temp_dir().join(format!("ps-recname-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let name = rec_filename(2026, 10, 18, 12, 0, 0);
    assert_eq!(free_rec_path(&root, &name), root.join(&name));
    fs::write(root.join(&name), b"x").unwrap();
    assert_eq!(free_rec_path(&root, &name), root.join("rec-2026-10-18_120000-2.mp4"));
    fs::write(root.join("rec-2026-10-18_120000-2.mp4"), b"x").unwrap();
    assert_eq!(free_rec_path(&root, &name), root.join("rec-2026-10-18_120000-3.mp4"));
    let _ = fs::remove_dir_all(&root);
}
