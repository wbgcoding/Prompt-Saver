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
