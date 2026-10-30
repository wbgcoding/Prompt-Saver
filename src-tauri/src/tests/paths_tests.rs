use super::*;

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn a_drive_letter_is_local_whatever_it_maps_to() {
    for letter in 'A'..='Z' {
        for p in [
            format!(r"{letter}:\share\a.pdf"),
            format!("{letter}:/share/a.pdf"),
            format!(r"\\?\{letter}:\share\a.pdf"),
            format!(r"\??\{letter}:\share\a.pdf"),
        ] {
            assert!(!is_network_path(&p), "{p:?} names a connection the user made");
        }
    }
    // Names that reach a host stay refused.
    for p in [r"\\nas\share\a.pdf", "//nas/share/a.pdf", r"\\?\UNC\nas\share\a.pdf", r"\\?\GLOBALROOT\Device\Mup\nas\share"] {
        assert!(is_network_path(p), "{p:?}");
    }
    let src = source!("../paths.rs");
    assert!(!src.contains("fn drive_is_remote"), "the drive type is no longer asked");
}

#[test]
fn the_apps_own_network_folders_pass_the_path_gate() {
    let data = PathBuf::from(r"\\nas\ps\data");
    let rec = PathBuf::from(r"\\nas\videos\rec");
    let own = [data.clone(), rec.clone()];
    // Inside an own folder that is a share: not foreign, whatever the case or separator.
    for p in [
        r"\\nas\ps\data\files\a.pdf",
        r"\\NAS\PS\DATA\files\a.pdf",
        "//nas/ps/data/files/a.pdf",
        r"\\?\UNC\nas\ps\data\files\a.pdf",
        r"\\nas\videos\rec\rec-1.mp4",
    ] {
        assert!(is_network_path(p), "{p:?} is still a network spelling");
        assert!(!is_foreign_network_path(p, &own), "{p:?} lies in an own folder");
    }
    // Everything else on a share stays refused, including climbs out of the own folder.
    for p in [
        r"\\nas\ps\other\a.pdf",
        r"\\nas\ps\data2\a.pdf",
        r"\\evil\ps\data\a.pdf",
        r"\\nas\ps\data\..\other\a.pdf",
        "//nas/ps/data/../other/a.pdf",
        r"\\?\GLOBALROOT\Device\Mup\nas\ps\data\a.pdf",
        r"\\.\UNC\nas\ps\data\a.pdf\..\..\x",
    ] {
        assert!(is_foreign_network_path(p, &own), "{p:?} must stay refused");
    }
    // A local own folder never vouches for a share path.
    let local = [PathBuf::from(r"C:\data")];
    assert!(is_foreign_network_path(r"\\nas\ps\data\a.pdf", &local));
    assert!(is_foreign_network_path(r"\\nas\ps\data\a.pdf", &[]));
    // The attachment gate uses the same rule.
    assert!(!attachment_path_refused(&own, &data, r"\\nas\ps\data\files\a.pdf"));
    assert!(attachment_path_refused(&own, &data, r"\\nas\ps\other\a.pdf"));
    // The recording gate refuses a foreign share on text before it asks the disk anything.
    assert!(!is_recording_source_location(std::slice::from_ref(&rec), Path::new(r"\\nas\videos\other\rec-1.mp4")));
}

#[test]
fn every_network_gate_that_can_meet_an_own_folder_asks_the_own_dir_rule() {
    let copy = source!("../copy.rs");
    let prompts = source!("../prompts.rs");
    let video = source!("../video_editor.rs");
    let images = source!("../images.rs");
    assert!(copy.contains("attachment_path_refused(&own_dirs"));
    assert!(prompts.matches("attachment_path_refused(&own_dirs").count() >= 2, "add and update");
    assert!(video.contains("is_foreign_network_path(&req.src_path, &own_dirs)"));
    assert!(video.contains("is_foreign_network_path(&src_path, &own_dirs)"));
    assert!(images.contains("is_foreign_network_path(path, own_dirs)"));
}

#[test]
fn only_the_apps_own_store_files_are_refused_inside_the_data_folder() {
    let root = std::env::temp_dir().join(format!("ps-ownfiles-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    for name in ["data.db", "data.db-wal", "data.db-shm", "prompts.json", "settings.json", "datapath", "data.db.tmp", "backup.psb"] {
        fs::write(root.join(name), b"x").unwrap();
        assert!(attachment_path_refused(&[], &root, &root.join(name).to_string_lossy()), "{name} is the app's own");
    }
    // A bare demo file, the user's own document and a portable install's files stay attachable.
    for name in ["demo.png", "notes.pdf", "Manual.pdf", "clip.mp4"] {
        fs::write(root.join(name), b"x").unwrap();
        assert!(!attachment_path_refused(&[], &root, &root.join(name).to_string_lossy()), "{name} is the user's");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_update_does_not_re_vet_the_path_the_prompt_already_holds() {
    let src = source!("../prompts.rs");
    let at = src.find("pub(crate) async fn update_prompt").unwrap();
    let body = &src[at..];
    let body = &body[..body.find("\n}\n").unwrap()];
    let vet = body.find("attachment_path_refused(&own_dirs").expect("still vets a new value");
    let held = body[..vet + 400].find("held != Some(p)").expect("compares with the stored value");
    assert!(held > vet - 400 && body.contains("keep_as_is[0]") && body.contains("keep_as_is[1]"));
    // The kept value is neither copied nor probed.
    assert!(body.contains("store_files && !keep_as_is[0]"));
    assert!(body.contains("!attachment_path_refused(&own_dirs, &store_dir, p)"), "probe source is vetted");
    // A new prompt has nothing held, so it keeps the plain gate.
    let add = &src[src.find("pub(crate) async fn add_prompt").unwrap()..at];
    assert!(!add.contains("keep_as_is"));
}

#[test]
fn a_file_on_another_computer_gets_a_translated_message() {
    let map = source!("../../../ui/errors.js");
    assert!(map.contains("\"source-not-allowed\": \"errSourceNotAllowed\""));
    for lang in ["en", "de", "es", "it", "pt", "pl", "zh", "ja", "nl", "tr", "ro"] {
        let path = format!("{}/../ui/i18n/{lang}.js", env!("CARGO_MANIFEST_DIR"));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("    errSourceNotAllowed: \""), "{lang} lacks the message");
    }
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
    for p in [r"\??\C:\clips\a.png", "/rooted/only.png", r"\single\lead.png", "C:/clips/clip.mp4"] {
        assert!(!is_network_path(p), "{p:?} is local");
    }
}

#[test]
fn a_verbatim_or_nt_path_is_local_only_for_a_drive_letter_or_a_volume_guid() {
    for p in [
        r"\\?\GLOBALROOT\Device\Mup\host\share\x.mp4",
        r"\??\GLOBALROOT\Device\Mup\host\share\x.mp4",
        r"\\?\globalroot\Device\LanmanRedirector\host\share",
        r"\\?\Device\Mup\host\share",
        r"\\?\Volume{not-a-guid}\x",
        r"\\?\",
    ] {
        assert!(is_network_path(p), "{p:?} must count as a network path");
    }
    for p in [
        r"\\?\C:\clips\a.png",
        r"\??\C:\clips\a.png",
        r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\clips\a.png",
    ] {
        assert!(!is_network_path(p), "{p:?} is local");
    }
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

#[test]
fn write_bytes_atomic_replaces_the_target_and_leaves_no_temp_file() {
    let dir = std::env::temp_dir().join(format!("ps-atomic-ok-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("export.psb");
    write_bytes_atomic(&target, b"first").unwrap();
    write_bytes_atomic(&target, b"second, longer").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"second, longer");
    let names: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["export.psb".to_string()], "no temp file is left beside the target");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_atomic_write_leaves_the_old_file_untouched() {
    let dir = std::env::temp_dir().join(format!("ps-atomic-fail-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("export.psb");
    fs::write(&target, b"the old backup").unwrap();
    // A read-only target cannot be replaced, so the rename step fails after the
    // new bytes were already written to the temp file.
    let mut perms = fs::metadata(&target).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&target, perms).unwrap();
    let result = write_bytes_atomic(&target, b"a much longer replacement that must not appear");
    let mut perms = fs::metadata(&target).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(&target, perms).unwrap();
    assert!(result.is_err(), "replacing a read-only file must fail");
    assert_eq!(fs::read(&target).unwrap(), b"the old backup", "the old file is byte-identical");
    let names: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["export.psb".to_string()], "the failed attempt removed its temp file");
    let _ = fs::remove_dir_all(&dir);
}

// A file another program holds open without delete sharing cannot be replaced, but an in-place
// write would still go through and could leave a half-written backup behind.
#[cfg(windows)]
#[test]
fn the_strict_atomic_write_leaves_a_file_held_open_without_delete_sharing_untouched() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = std::env::temp_dir().join(format!("ps-atomic-held-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("backup.psb");
    fs::write(&target, b"the previous backup").unwrap();
    // Share read and write, but not delete: the swap is refused, an in-place write is not.
    let held = fs::OpenOptions::new().read(true).share_mode(1 | 2).open(&target).unwrap();
    let result = write_bytes_atomic_strict(&target, b"a replacement that must not appear");
    assert!(result.is_err(), "a refused replace must surface as an error");
    assert_eq!(fs::read(&target).unwrap(), b"the previous backup", "the old file stays byte-identical");
    drop(held);
    let names: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["backup.psb".to_string()], "the failed attempt removed its temp file");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn only_our_own_temp_names_are_recognised() {
    assert!(is_atomic_temp_name("settings.json.tmp-p1760000000000000000_12"));
    assert!(is_atomic_temp_name("a.md.tmp-p1_0"));
    for other in ["notes.tmp", "x.tmp-abc", "x.tmp-p12", "x.tmp-p_3", "x.tmp-p1_", ".tmp-p1_2", "settings.json", "a.tmp-p1_2.bak", "a.tmp-q1_2"] {
        assert!(!is_atomic_temp_name(other), "{other} is not ours");
    }
}

#[test]
fn the_atomic_write_falls_back_to_writing_in_place_when_the_replace_is_refused() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = std::env::temp_dir().join(format!("ps-atomic-fallback-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("export.json");
    fs::write(&target, b"old content that is longer").unwrap();
    // Another program holds the file open and shares reading and writing but not
    // deleting or renaming, so a rename over it is refused while a write is not.
    let _holder = fs::OpenOptions::new().read(true).share_mode(1 | 2).open(&target).unwrap();
    write_bytes_atomic(&target, b"new").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    let names: Vec<String> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["export.json".to_string()], "no temp file is left");
    drop(_holder);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_atomic_write_replaces_the_file_a_symlink_points_at() {
    let dir = std::env::temp_dir().join(format!("ps-atomic-link-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let real = dir.join("real.json");
    let link = dir.join("link.json");
    fs::write(&real, b"before").unwrap();
    if std::os::windows::fs::symlink_file(&real, &link).is_err() {
        // Creating a symlink needs developer mode or admin rights; without them there is nothing to test.
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    write_bytes_atomic(&link, b"after").unwrap();
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "the link itself stays a link");
    assert_eq!(fs::read(&real).unwrap(), b"after", "the file behind the link is what changed");
    let _ = fs::remove_dir_all(&dir);
}

// No behavioural red: the old text check and a failing canonicalize both end in `false`; the
// difference is that canonicalize contacts the host first. So the contrast is on the source.
#[test]
fn the_recording_source_check_refuses_every_network_spelling_by_text() {
    let src = source!("../media_files.rs");
    let body = src.split("pub(crate) fn is_recording_source_location").nth(1).unwrap().split("\n}\n").next().unwrap();
    assert!(body.contains("is_foreign_network_path(&raw"), "the one shared gate decides");
    assert!(!body.contains("starts_with(r\"\\\\\")"), "no private two-prefix check");
    let allowed = vec![std::env::temp_dir()];
    for p in [r"/\host\share\rec-1.mp4", r"\??\UNC\host\share\rec-1.mp4", r"\?\UNC\host\share\rec-1.mp4", r"\.\GLOBALROOT\Device\x\rec-1.mp4"] {
        assert!(!is_recording_source_location(&allowed, Path::new(p)), "{p}");
    }
}

#[test]
fn an_attachment_path_naming_the_store_or_a_network_share_is_refused() {
    let root = std::env::temp_dir().join(format!("ps-attach-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let data = root.join("data");
    fs::create_dir_all(data.join("files")).unwrap();
    fs::write(data.join("data.db"), b"db").unwrap();
    fs::write(data.join("data.db-wal"), b"wal").unwrap();
    fs::write(data.join("settings.json"), b"{}").unwrap();
    fs::write(data.join("files").join("copy.pdf"), b"pdf").unwrap();
    fs::write(root.join("mine.txt"), b"x").unwrap();
    // The store's own files directly in the data folder never feed a copy or an editor.
    assert!(attachment_path_refused(&[], &data, &data.join("data.db").to_string_lossy()));
    assert!(attachment_path_refused(&[], &data, &data.join("data.db-wal").to_string_lossy()));
    assert!(attachment_path_refused(&[], &data, &data.join("settings.json").to_string_lossy()));
    // A dot-dot hop and a different spelling of the same file are the same file.
    let hop = format!("{}{}files{}..{}data.db", data.display(), std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR);
    assert!(attachment_path_refused(&[], &data, &hop));
    // The app's own attachment copies and the user's own files stay fine.
    assert!(!attachment_path_refused(&[], &data, &data.join("files").join("copy.pdf").to_string_lossy()));
    assert!(!attachment_path_refused(&[], &data, &root.join("mine.txt").to_string_lossy()));
    // A missing file is not this gate's business (a moved file must stay re-savable), and empty means none.
    assert!(!attachment_path_refused(&[], &data, &root.join("gone.txt").to_string_lossy()));
    assert!(!attachment_path_refused(&[], &data, ""));
    // A network spelling is refused by text, before the disk is asked anything.
    assert!(attachment_path_refused(&[], &data, r"\\host\share\a.mp4"));
    assert!(attachment_path_refused(&[], &data, "//host/share/a.mp4"));
    let _ = fs::remove_dir_all(&root);

    // Every path that stores or uses an attachment asks the gate before touching it.
    let prompts = source!("../prompts.rs");
    for f in ["pub(crate) async fn update_prompt", "pub(crate) async fn add_prompt"] {
        let at = prompts.find(f).unwrap_or_else(|| panic!("{f}"));
        let body = &prompts[at..];
        let body = &body[..body.find("\n}\n").unwrap()];
        let gate = body.find("attachment_path_refused(").unwrap_or_else(|| panic!("{f} asks the gate"));
        let stage = body.find("staged_copies.stage(").expect("stage");
        assert!(gate < stage, "{f} checks before a copy is made");
    }
    let copy = source!("../copy.rs");
    let at = copy.find("pub(crate) async fn copy_prompt").unwrap();
    assert!(copy[at..].contains("attachment_path_refused("), "the file clipboard path asks again at use");
    let images = source!("../images.rs");
    let at = images.find("pub(crate) fn set_clipboard_file(path: &str, own_dirs").unwrap();
    let head = &images[at..at + 300];
    assert!(head.find("is_foreign_network_path(").is_some_and(|n| n < head.find(".exists()").unwrap()), "text check before exists()");
}

fn legacy_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}-test-legacy-{name}-{}", brand::SLUG, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// An unclean exit leaves committed rows only in data.db-wal; the migrated database must hold them.
#[test]
fn the_legacy_migration_carries_the_write_ahead_log() {
    let root = legacy_test_dir("wal");
    let (old, new) = (root.join("old"), root.join("new"));
    fs::create_dir_all(&old).unwrap();
    fs::create_dir_all(&new).unwrap();
    let conn = db_conn_at(&old).unwrap();
    conn.execute("PRAGMA wal_autocheckpoint=0", []).ok();
    conn.execute("INSERT INTO meta(key, value) VALUES('wal_probe', '1')", []).unwrap();
    assert!(old.join("data.db-wal").exists());
    assert!(copy_legacy_store(&old, &new));
    drop(conn);
    let moved = db_conn_at(&new).unwrap();
    let n: i64 = moved.query_row("SELECT COUNT(*) FROM meta WHERE key='wal_probe'", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "the row that only lived in the log arrives");
    drop(moved);
    let _ = fs::remove_dir_all(&root);
}

// A copy that fails halfway must not leave a data.db behind: its presence ends the migration.
#[test]
fn a_failed_legacy_copy_leaves_no_database_and_can_retry() {
    let root = legacy_test_dir("fail");
    let (old, new) = (root.join("old"), root.join("new"));
    fs::create_dir_all(&old).unwrap();
    fs::create_dir_all(&new).unwrap();
    fs::write(old.join("data.db"), b"rows").unwrap();
    fs::create_dir_all(old.join("settings.json")).unwrap(); // exists, but cannot be copied as a file
    assert!(!copy_legacy_store(&old, &new));
    assert!(!new.join("data.db").exists(), "no data.db, so the next start migrates again");
    assert!(!new.join("data.db.migrating").exists());
    fs::remove_dir_all(old.join("settings.json")).unwrap();
    assert!(copy_legacy_store(&old, &new));
    assert_eq!(fs::read(new.join("data.db")).unwrap(), b"rows");
    let _ = fs::remove_dir_all(&root);
}

// The text of one function, from its signature to the closing brace at column 0.
fn fn_body<'a>(src: &'a str, signature: &str) -> &'a str {
    let at = src.find(signature).unwrap_or_else(|| panic!("{signature} not found"));
    let rest = &src[at..];
    &rest[..rest.find("\n}\n").unwrap_or(rest.len())]
}

// The network check on the path text must come before any call that would open the path.
fn assert_text_check_first(body: &str, name: &str, opens: &[&str]) {
    let gate = body
        .find("is_foreign_network_path(")
        .or_else(|| body.find("is_network_path("))
        .or_else(|| body.find("is_own_recording_source("))
        .unwrap_or_else(|| panic!("{name} has no network check"));
    for call in opens {
        if let Some(open) = body.find(call) {
            assert!(gate < open, "{name}: the network check must come before {call}");
        }
    }
}

#[test]
fn the_clipboard_file_and_its_previews_check_the_path_text_first() {
    let images = source!("../images.rs");
    for (sig, opens) in [
        ("fn get_clipboard_file_path(", &["grant_picked_file", "allow_file"][..]),
        ("fn load_image_file(", &["spawn_blocking", "image_file_preview"][..]),
        ("fn pdf_preview(", &["pdfium_lib_path", "load_pdf_from_file"][..]),
        ("fn attachment_text_preview(", &["spawn_blocking", "read_bytes_preview"][..]),
    ] {
        let body = fn_body(&images, sig);
        assert!(body.contains("is_foreign_network_path("), "{sig} asks the own-dir rule");
        assert_text_check_first(body, sig, opens);
    }
}

#[test]
fn the_recording_commands_check_the_path_text_before_they_open_it() {
    let media = source!("../media_files.rs");
    for sig in ["fn save_recording_as(", "fn save_recording_now(", "fn copy_recording_to_clipboard("] {
        let body = fn_body(&media, sig);
        assert!(body.contains("is_foreign_network_path(&src_path"), "{sig} asks the own-dir rule");
        assert_text_check_first(body, sig, &[".is_file()", "spawn_blocking", "place_in_dir("]);
    }
    let discard = fn_body(&media, "fn discard_recording_file_in(");
    assert_text_check_first(discard, "discard_recording_file_in", &["and_then(|d| d.canonicalize()", "path.is_file()"]);
    assert!(discard.contains("is_foreign_network_path("));
    let camera = source!("../camera.rs");
    let at = camera.find("pub fn read_camera_sidecar(").unwrap();
    assert!(camera[..at].trim_end().ends_with("#[tauri::command(async)]"), "off the main thread");
    assert_text_check_first(fn_body(&camera, "pub fn read_camera_sidecar("), "read_camera_sidecar", &["read_sidecar("]);
}

#[test]
fn the_editor_commands_check_the_path_text_before_they_open_it() {
    let video = source!("../video_editor.rs");
    for sig in [
        "fn probe_recording(",
        "fn probe_extra_video_source(",
        "fn read_key_sidecar(",
        "fn generate_filmstrip(",
        "fn generate_waveform_peaks(",
        "fn extract_still_frame(",
        "fn mux_mic_track(",
    ] {
        let body = fn_body(&video, sig);
        assert!(body.contains("is_foreign_network_path("), "{sig} asks the own-dir rule");
        assert_text_check_first(body, sig, &[".is_file()", "spawn_blocking", "sidecar_path("]);
    }
    // The mic mux decides on both texts before it asks whether either file exists.
    let mux = fn_body(&video, "fn mux_mic_track(");
    let gate = mux.find("is_foreign_network_path(&sidecar_path").unwrap();
    assert!(gate < mux.find("is_own_mic_sidecar(").unwrap());
    assert!(mux.find("is_own_mic_sidecar(").unwrap() < mux.find(".is_file()").unwrap());
    assert!(mux.find("is_own_recording_source(").unwrap() < mux.find(".is_file()").unwrap());
}

#[test]
fn a_graph_source_on_a_share_is_judged_on_its_text_before_it_is_opened() {
    let graph = source!("../export_graph.rs");
    let body = fn_body(&graph, "fn validate_export_graph(");
    assert_text_check_first(body, "validate_export_graph", &["canonical(&path)"]);
    assert!(body.contains("is_network_path(&path)"));
    // Nothing is named by the request: a share in a graph is refused on its text alone.
    for g in [
        "movie='//nas/share/x.mp4'[o];[0:v][o]overlay[vout]",
        r"amovie='\\nas\share\x.mp4'[a]",
        r"movie='\\?\UNC\nas\share\x.mp4'[o]",
    ] {
        assert!(validate_export_graph(g, &[], &[]).is_err(), "{g}");
    }
    // The request's own paths feed the text step.
    let request = fn_body(&graph, "fn validate_export_request_graphs(");
    assert!(request.contains("allowed_texts.extend(media_paths"));
}
