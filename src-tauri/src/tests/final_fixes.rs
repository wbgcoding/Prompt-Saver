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
fn a_synced_file_keeps_a_network_path_as_written() {
    let mut p = sync_blank_prompt("p1".into());
    p.file_path = String::new();
    let md = format!("---\nname: x\ncolor: \ntags: \nnotes: \nfile: {}\nicon: //nas/ps/i.png\n---\nbody\n", r"\\nas\ps\a.pdf");
    let out = apply_md_to_prompt(&p, &md).unwrap();
    assert_eq!(out.file_path, r"\\nas\ps\a.pdf");
    assert_eq!(out.icon_path, "//nas/ps/i.png");
    let src = source!("../md_sync.rs");
    assert!(!src.contains("is_network_path"), "the sync reader no longer blanks a path");
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
fn the_main_window_may_start_a_resize_drag() {
    let caps = source!("../../capabilities/default.json");
    assert!(caps.contains("\"core:window:allow-start-resize-dragging\""));
}

#[test]
fn a_very_long_waveform_never_passes_the_canvas_cap() {
    let video = source!("../../../ui/video.js");
    assert!(video.contains("Math.min(window.devicePixelRatio || 1, WAVE_MAX_PX / width)"));
    assert!(!video.contains("Math.max(1, WAVE_MAX_PX / width)"));
}

#[test]
fn a_voice_key_counts_as_taken_only_while_its_switches_are_on() {
    let mut settings = Settings { voice_hotkey: "Ctrl+Alt+V".to_string(), ..Default::default() };
    let sc = parse_accel("Ctrl+Alt+V").unwrap();
    // The hotkey switch is not on yet, so nothing uses the key.
    assert!(!slot_key_taken(&settings, &[], HotkeySlot::App, sc));
    settings.ui_flags.insert("voiceHotkey".to_string(), true);
    assert!(slot_key_taken(&settings, &[], HotkeySlot::App, sc));
    settings.ui_flags.insert("voice".to_string(), false);
    assert!(!slot_key_taken(&settings, &[], HotkeySlot::App, sc));
    settings.ui_flags.remove("voice");
    // The voice slot never collides with itself.
    assert!(!slot_key_taken(&settings, &[], HotkeySlot::Voice, sc));
}
