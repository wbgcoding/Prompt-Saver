use super::*;

// The checks below compare multi-line source text; a checkout with Windows line endings
// must read the same as one with Unix endings.
macro_rules! source {
    ($path:literal) => {
        include_str!($path).replace("\r\n", "\n")
    };
}

#[test]
fn copy_mic_sidecar_alongside_brings_the_microphone_track_to_the_open_editor_copy() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-mic-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("rec-x-copy.mp4");
    fs::write(&src, b"video").unwrap();
    copy_mic_sidecar_alongside(&src, &dest);
    assert!(!dest.with_extension("mic.wav").exists(), "no sidecar, nothing to copy");
    fs::write(src.with_extension("mic.wav"), b"RIFF").unwrap();
    copy_mic_sidecar_alongside(&src, &dest);
    assert_eq!(fs::read(dest.with_extension("mic.wav")).unwrap(), b"RIFF");
    assert!(src.with_extension("mic.wav").is_file(), "the original stays untouched");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_oversized_mic_sidecar_is_not_copied_to_the_open_editor_copy() {
    let dir = std::env::temp_dir().join(format!("ps-openedit-mic-big-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rec-x.mp4");
    let dest = dir.join("copy.mp4");
    fs::write(&src, b"video").unwrap();
    fs::write(src.with_extension("mic.wav"), vec![0u8; 2 * 1024 * 1024]).unwrap();
    copy_mic_sidecar_capped(&src, &dest, 1.0);
    assert!(!dest.with_extension("mic.wav").exists(), "past the ceiling, the sidecar stays behind");
    copy_mic_sidecar_capped(&src, &dest, 4.0);
    assert!(dest.with_extension("mic.wav").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_editor_work_copy_is_recognised_only_inside_its_own_temp_folders() {
    let root = std::env::temp_dir().join(format!("ps-openedit-root-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let work = root.join(format!("{}-openedit-1-2", brand::TEMP_PREFIX));
    let other = root.join("somewhere-else");
    fs::create_dir_all(&work).unwrap();
    fs::create_dir_all(work.join("nested")).unwrap();
    fs::create_dir_all(&other).unwrap();
    let copy = work.join("My clip.mp4");
    let stray = other.join("My clip.mp4");
    let nested = work.join("nested").join("My clip.mp4");
    for f in [&copy, &stray, &nested] {
        fs::write(f, b"v").unwrap();
    }
    assert!(is_editor_work_copy_in(&root, &copy));
    assert!(!is_editor_work_copy_in(&root, &stray), "another folder under the temp root is not a work folder");
    assert!(!is_editor_work_copy_in(&root, &nested), "only the work folder's own files count");
    assert!(!is_editor_work_copy_in(&root, &work.join("missing.mp4")));
    assert!(!is_editor_work_copy_in(&other, &copy), "a different temp root does not match");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn video_folder_cleanup_takes_the_sidecars_of_a_deleted_recording() {
    let dir = std::env::temp_dir().join(format!("ps-video-folder-cleanup-sidecars-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let stale = dir.join("rec-old.mp4");
    let fresh = dir.join("rec-new.mp4");
    fs::write(&stale, b"old").unwrap();
    fs::write(&fresh, b"new").unwrap();
    for v in [&stale, &fresh] {
        fs::write(v.with_extension("mic.wav"), b"w").unwrap();
        fs::write(sidecar_path(v, KEYS_SIDECAR_EXT), b"{}").unwrap();
        fs::write(sidecar_path(v, CAM_SIDECAR_EXT), b"{}").unwrap();
    }
    backdate(&stale, Duration::from_secs(40 * 24 * 3600));
    let dir_s = dir.to_string_lossy().to_string();
    assert_eq!(video_folder_cleanup_impl(&dir_s, 30, &["rec-old.mp4".to_string()], &dir), 1);
    assert!(!stale.with_extension("mic.wav").exists());
    assert!(!sidecar_path(&stale, KEYS_SIDECAR_EXT).exists());
    assert!(!sidecar_path(&stale, CAM_SIDECAR_EXT).exists());
    assert!(fresh.with_extension("mic.wav").exists(), "a kept recording keeps its sidecars");
    assert!(sidecar_path(&fresh, KEYS_SIDECAR_EXT).exists());
    let _ = fs::remove_dir_all(&dir);
}

pub(super) fn part_info(dir: &Path, name: &str) -> RecordingPartInfo {
    let path = dir.join(name);
    fs::write(&path, b"video").unwrap();
    RecordingPartInfo { path: path.to_string_lossy().to_string(), mic_wav_path: None }
}

#[test]
fn place_recording_parts_moves_each_part_with_its_name() {
    let base = std::env::temp_dir().join(format!("ps-test-place-parts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let src = base.join("recordings");
    let dest = base.join("saved");
    fs::create_dir_all(&src).unwrap();
    let mut parts = vec![part_info(&src, "rec-2026-01-01-00-00-00_part2.mp4"), part_info(&src, "notes.mp4")];
    fs::write(src.join("rec-2026-01-01-00-00-00_part2.mic.wav"), b"mic").unwrap();
    let (saved, failed) = place_recording_parts(&mut parts, std::slice::from_ref(&src), &dest);
    assert_eq!((saved, failed), (1, 1), "a file that is no recording of ours is refused");
    assert!(dest.join("rec-2026-01-01-00-00-00_part2.mp4").is_file());
    assert!(dest.join("rec-2026-01-01-00-00-00_part2.mic.wav").is_file(), "the sidecar travels along");
    assert_eq!(parts.len(), 1, "a saved part leaves the list, a failed one stays");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn discard_recording_parts_in_moves_parts_to_the_holding_folder() {
    let dir = std::env::temp_dir().join(format!("ps-test-discard-parts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut parts = vec![part_info(&dir, "rec-2026-01-01-00-00-00_part2.mp4"), part_info(&dir, "rec-2026-01-01-00-00-00_part3.mp4")];
    let moved = discard_recording_parts_in(&mut parts, std::slice::from_ref(&dir));
    assert_eq!(moved, 2);
    assert!(parts.is_empty());
    assert!(dir.join("verworfen").join("rec-2026-01-01-00-00-00_part3.mp4").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_removed_take_takes_every_sidecar_with_it() {
    let dir = std::env::temp_dir().join(format!("ps-take-sidecars-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    // A file where the holding folder would go makes the move fail, so the take is removed.
    fs::write(dir.join("verworfen"), b"").unwrap();
    let video = dir.join("rec-take.mp4");
    fs::write(&video, b"v").unwrap();
    for ext in ["mic.wav", KEYS_SIDECAR_EXT, CAM_SIDECAR_EXT] {
        fs::write(sidecar_path(&video, ext), b"s").unwrap();
    }
    discard_or_remove_take(&video);
    assert!(!video.exists());
    for ext in ["mic.wav", KEYS_SIDECAR_EXT, CAM_SIDECAR_EXT] {
        assert!(!sidecar_path(&video, ext).exists(), "{ext} was left behind");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sidecar_path_sits_next_to_the_recording_as_its_own_extension() {
    assert_eq!(
        sidecar_path(Path::new("C:/data/recordings/rec-1.mp4"), KEYS_SIDECAR_EXT),
        PathBuf::from("C:/data/recordings/rec-1.keys.json")
    );
    assert_eq!(
        sidecar_path(Path::new("C:/data/recordings/rec-cam-1.webm"), CAM_SIDECAR_EXT),
        PathBuf::from("C:/data/recordings/rec-cam-1.cam.json")
    );
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

#[test]
fn a_tiff_reaches_the_editor_as_a_png() {
    let dir = std::env::temp_dir().join(format!("ps-tiff-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("scan.tif");
    image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255])).save_with_format(&file, image::ImageFormat::Tiff).unwrap();
    let (kind, fmt) = detect_media_kind(&file, None).expect("a tiff is an image");
    assert_eq!(kind, "image");
    let url = editor_image_data_url(&file, fmt).expect("converts");
    let png = file.with_file_name("plain.png");
    image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255])).save(&png).unwrap();
    let plain = editor_image_data_url(&png, Some(image::ImageFormat::Png)).expect("passes through");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(url.starts_with("data:image/png;base64,"), "the webview cannot decode tiff");
    assert_eq!(decode_image_mem(&data_url_bytes(&url)).map(|i| (i.width(), i.height())), Some((3, 2)));
    assert!(plain.starts_with("data:image/png;base64,"));
}

#[test]
fn every_folder_dialog_title_is_translated_in_every_language() {
    let purposes = ["screenshot", "videoExport", "recording", "sync", "data"];
    let en: Vec<&str> = purposes.iter().map(|p| crate::snip::folder_picker_title(p, "en").unwrap()).collect();
    for lang in crate::model::LANG_CODES {
        let titles: Vec<&str> = purposes.iter().map(|p| crate::snip::folder_picker_title(p, lang).unwrap()).collect();
        for (i, t) in titles.iter().enumerate() {
            assert_ne!(*t, en[i], "{lang} still shows the English title for {}", purposes[i]);
            assert!(!t.is_empty());
        }
        let mut sorted = titles.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), purposes.len(), "{lang} reuses one title for two purposes");
    }
}

// Structural fix, no red unit test: the old spelling produced the same path as the constant today.
#[test]
fn the_key_sidecar_is_always_named_through_the_shared_builder() {
    let src = source!("../media_files.rs");
    assert!(!src.contains("with_extension(\"keys.json\")"));
    assert_eq!(crate::media_files::sidecar_path(Path::new("a/rec.mp4"), crate::media_files::KEYS_SIDECAR_EXT), Path::new("a/rec.keys.json"));
}

// Structural fix, no red unit test: a modal dialog cannot be opened in a test.
#[test]
fn the_media_dialogs_wait_on_a_blocking_thread() {
    for (name, src, anchors) in [
        ("images", source!("../images.rs"), &["fn pick_file_path", "fn pick_media_file_path"][..]),
        ("media_files", source!("../media_files.rs"), &["fn save_recording_as", "fn pick_export_folder"][..]),
        ("snip", source!("../snip.rs"), &["fn save_screenshot_as", "fn pick_folder"][..]),
    ] {
        for anchor in anchors {
            let at = src.find(anchor).unwrap_or_else(|| panic!("{name}: {anchor}"));
            let body = &src[at..];
            let end = body.find("
}
").unwrap();
            let body = &body[..end];
            assert!(body.contains("spawn_blocking"), "{name} {anchor} must open its dialog on a blocking thread");
        }
    }
}

#[test]
fn a_running_parts_save_is_marked_in_flight_and_quit_waits_for_it() {
    let part = |n: &str| RecordingPartInfo { path: n.to_string(), mic_wav_path: None };
    let shared = std::sync::Mutex::new(vec![part("ps-q6-inflight.mp4")]);
    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    std::thread::scope(|s| {
        let shared = &shared;
        s.spawn(move || {
            with_parts_outside_lock(shared, |parts| {
                started_tx.send(()).unwrap();
                go_rx.recv().unwrap();
                parts.clear();
            })
        });
        started_rx.recv().unwrap();
        // The list is empty while the copy runs, yet the sweep must still see the part as taken.
        assert!(shared.lock().unwrap().is_empty());
        assert!(protected_take_paths().contains(&PathBuf::from("ps-q6-inflight.mp4")));
        // Quit gives up only after its bounded wait, and goes on as soon as the save is done.
        assert!(!wait_for_part_saves(std::time::Duration::from_millis(100)), "a save is still running");
        go_tx.send(()).unwrap();
        assert!(wait_for_part_saves(std::time::Duration::from_secs(20)));
    });
    // Both quit routes wait: the app's exit event and the take saved on quit.
    let lib = source!("../lib.rs");
    assert!(lib.contains("wait_for_part_saves(REC_PARTS_QUIT_WAIT)"));
    let media = source!("../media_files.rs");
    let quit = media.split("pub(crate) fn save_take_on_quit").nth(1).unwrap().split("pub(crate) async fn save_recording_parts").next().unwrap();
    assert!(quit.contains("wait_for_part_saves(REC_PARTS_QUIT_WAIT)"));
}

#[test]
fn a_clip_export_path_is_judged_by_its_parent_before_the_file_is_touched() {
    let root = std::env::temp_dir().join(format!("ps-clipexp-parent-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let temp = root.join("tmp");
    let other = root.join("other");
    fs::create_dir_all(&temp).unwrap();
    fs::create_dir_all(&other).unwrap();
    let name = format!("{}-clipexport-1.mp4", brand::TEMP_PREFIX);
    fs::write(temp.join(&name), b"v").unwrap();
    fs::write(other.join(&name), b"v").unwrap();
    assert!(is_clip_export_file(&temp, &temp.join(&name)));
    assert!(!is_clip_export_file(&temp, &other.join(&name)), "same name in a foreign folder");
    assert!(!is_clip_export_file(&temp, &temp.join("..").join("other").join(&name)), "a dot-dot hop");
    assert!(!is_clip_export_file(&temp, Path::new("//host/share/x")), "network text");
    let _ = fs::remove_dir_all(&root);
    // The disk is asked about the parent and the network spelling is judged on text first;
    // the file itself is only touched once the parent has matched.
    let src = source!("../media_files.rs");
    let at = src.find("pub(crate) fn is_clip_export_file").unwrap();
    let body = &src[at..];
    let body = &body[..body.find("\n}\n").unwrap()];
    let file_at = body.find(".is_file()").expect("still checks the file");
    let parent_at = body.find("canonicalize()").expect("parent check");
    assert!(body.find("is_network_path(").is_some_and(|n| n < parent_at), "text check first");
    assert!(parent_at < file_at, "the parent decides before the file is touched");
}

// The dialog backend needs a single-threaded COM apartment. A pool thread that already joined
// the multithreaded one (audio does) would make every dialog return "nothing picked".
#[cfg(windows)]
#[test]
fn dialogs_run_on_a_thread_without_a_com_apartment() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let fresh = floating::on_dialog_thread(|| unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() });
    assert_eq!(fresh, Some(true), "the dialog thread must be free to pick its own apartment");
}

#[test]
fn every_native_dialog_call_runs_inside_the_dialog_thread_helper() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
        for call in [".pick_folder()", ".pick_file()", ".pick_files()", ".save_file()"] {
            for (at, _) in src.match_indices(call) {
                let head = &src[..at];
                let fn_start = head.rfind("\nfn ").max(head.rfind("\nasync fn ")).max(head.rfind(" fn ")).unwrap_or(0);
                assert!(head[fn_start..].contains("on_dialog_thread("), "{}: {call} must run inside on_dialog_thread", path.display());
                checked += 1;
            }
        }
    }
    assert!(checked >= 16, "the scan must actually see the dialog calls, saw {checked}");
}
