use super::*;

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn the_main_window_may_start_a_resize_drag() {
    let caps = source!("../../capabilities/default.json");
    assert!(caps.contains("\"core:window:allow-start-resize-dragging\""));
}

#[test]
fn the_system_locale_is_a_non_empty_language_tag() {
    let tag = crate::model::system_locale();
    assert!(!tag.is_empty(), "no locale name came back");
    assert!(tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "odd locale tag: {tag}");
    assert!(tag.chars().next().is_some_and(|c| c.is_ascii_alphabetic()));
}

#[test]
fn the_first_entry_of_a_multi_string_buffer_is_the_display_language() {
    let enc = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    // "en-US\0de-DE\0\0" as GetUserPreferredUILanguages fills it.
    let mut buf = enc("en-US");
    buf.push(0);
    buf.extend(enc("de-DE"));
    buf.extend([0, 0]);
    assert_eq!(crate::model::first_multi_sz(&buf), "en-US");
    // A single entry, a buffer with no terminator and an empty list.
    assert_eq!(crate::model::first_multi_sz(&enc("fr-FR\0\0")), "fr-FR");
    assert_eq!(crate::model::first_multi_sz(&enc("ja-JP")), "ja-JP");
    assert_eq!(crate::model::first_multi_sz(&[0, 0]), "");
    assert_eq!(crate::model::first_multi_sz(&[]), "");
}

#[test]
fn start_minimized_never_repoints_another_copys_autostart_entry() {
    assert!(start_minimized_rewrites_entry(true, RegistryOwner::ThisExe));
    assert!(start_minimized_rewrites_entry(true, RegistryOwner::Gone));
    assert!(start_minimized_rewrites_entry(true, RegistryOwner::Missing));
    assert!(!start_minimized_rewrites_entry(true, RegistryOwner::OtherExe));
    assert!(!start_minimized_rewrites_entry(false, RegistryOwner::ThisExe));
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

// Structural fix, no red unit test: counting real window listings needs a live desktop.
#[test]
fn a_window_click_lists_the_windows_once() {
    let src = source!("../capture.rs");
    let body = &src[src.find("pub(crate) async fn capture_window(").unwrap()..src.find("pub(crate) struct WindowProbe").unwrap()];
    assert_eq!(body.matches("list_windows()").count(), 0, "capture_window must go through probe_window");
    let missing = crate::capture::probe_window(u32::MAX, true);
    assert!(missing.app_name.is_none() && missing.direct.is_none() && missing.rect.is_none());
}

#[test]
fn a_failed_key_sidecar_write_is_an_error_without_the_path() {
    let root = std::env::temp_dir().join(format!("ps-keyside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let out = root.join("missing").join("rec-1.mp4");
    let err = crate::keylog::try_write_sidecar(&out, &Default::default()).unwrap_err();
    assert!(err.starts_with("write failed"), "{err}");
    assert!(!err.contains("missing"), "the reason must not carry the path: {err}");
    std::fs::create_dir_all(root.join("ok")).unwrap();
    assert!(crate::keylog::try_write_sidecar(&root.join("ok").join("rec-1.mp4"), &Default::default()).is_ok());
    let _ = std::fs::remove_dir_all(&root);
}

// A window closed maximized came back at its normal size: the start set the normal bounds on the
// still hidden window, whose resize event cleared the remembered flag, and the maximize done
// while hidden was lost on the show. Geometry is now only tracked for a visible window and the
// maximize waits for the show.
#[test]
fn maximized_main_window_is_restored_after_the_show() {
    let lib = source!("../lib.rs");
    let setup = &lib[lib.find("build_main_window(&handle)").expect("startup build")..];
    let setup = &setup[..setup.find("start_hidden").unwrap_or(setup.len()).min(1500)];
    assert!(!setup.contains(".maximize()"), "no maximize on the hidden window at start");
    let mw = source!("../main_window.rs");
    for ev in ["WindowEvent::Moved(p) => {", "WindowEvent::Resized(s) => {"] {
        let at = mw.find(ev).expect("geometry handler");
        assert!(mw[at..at + 160].contains("main_is_visible("), "{ev} ignores a hidden window");
    }
    let show = &mw[mw.find("pub(crate) fn show_main(").unwrap()..];
    let show = &show[..show.find("\n}\n").unwrap()];
    assert!(show.find("win.show()").unwrap() < show.find(".maximize()").unwrap(), "maximize after the show");
    let mf = source!("../media_files.rs");
    let reveal = &mf[mf.find("pub(crate) fn show_main_window(").unwrap()..];
    let reveal = &reveal[..reveal.find("\n}\n").unwrap()];
    let shown = reveal.find("w.show()").unwrap();
    assert!(shown < reveal.find("w.maximize()").expect("first reveal maximizes"));
    assert!(reveal.find(".maximized)").unwrap() < shown, "the flag is read before the show resets it");
    assert!(show.find("g.maximized").unwrap() < show.find("win.show()").unwrap(), "same in show_main");
}

#[test]
fn a_key_sidecar_overwrite_goes_through_the_atomic_helper_and_leaves_no_temp_file() {
    let root = std::env::temp_dir().join(format!("ps-keyside-atomic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let out = root.join("rec-1.mp4");
    for _ in 0..2 {
        crate::keylog::try_write_sidecar(&out, &Default::default()).unwrap();
    }
    let names: Vec<String> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    assert_eq!(names.len(), 1, "only the sidecar remains: {names:?}");
    let src = source!("../keylog.rs");
    let body = &src[src.find("pub(crate) fn try_write_sidecar").unwrap()..];
    let body = &body[..body.find("\n}\n").unwrap()];
    assert!(body.contains("write_bytes_atomic_strict") && !body.contains("fs::write("));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_pointer_the_screenshot_save_as_and_the_cli_export_are_written_atomically() {
    let snip = source!("../snip.rs");
    let mv = &snip[snip.find("fn move_store_dir").unwrap()..];
    let mv = &mv[..mv.find("\n}\n").unwrap()];
    assert!(mv.contains("write_bytes_atomic(&pointer_dir.join(\"datapath\")") && !mv.contains("fs::write(pointer_dir"));
    let sa = &snip[snip.find("fn save_screenshot_as").unwrap()..];
    let sa = &sa[..sa.find("\n}\n").unwrap()];
    assert!(sa.contains("write_bytes_atomic(&p, &out_bytes)") && !sa.contains("fs::write(&p"));
    let cli = source!("../cli.rs");
    let ex = &cli[cli.find("fn cmd_export").unwrap()..];
    let ex = &ex[..ex.find("\n}\n").unwrap()];
    assert!(ex.contains("write_bytes_atomic(&out_path") && !ex.contains("fs::write(&out_path"));
}
