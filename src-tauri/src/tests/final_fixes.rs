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
    assert_eq!(prompts.matches("attachment_path_refused(&own_dirs").count(), 2, "add and update");
    assert!(video.contains("is_foreign_network_path(&req.src_path, &own_dirs)"));
    assert!(video.contains("is_foreign_network_path(&src_path, &own_dirs)"));
    assert!(images.contains("is_foreign_network_path(path, own_dirs)"));
}
