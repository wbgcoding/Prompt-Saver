use super::*;

macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

// ---- last release pass ----

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
    assert!(!is_recording_source_location(&[rec.clone()], Path::new(r"\\nas\videos\other\rec-1.mp4")));
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
