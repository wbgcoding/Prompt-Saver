use super::*;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ps-mm-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn media_kind_parses_only_its_two_names() {
    assert_eq!(MediaKind::parse("recordings"), Ok(MediaKind::Recordings));
    assert_eq!(MediaKind::parse("screenshots"), Ok(MediaKind::Screenshots));
    assert!(MediaKind::parse("").is_err());
    assert!(MediaKind::parse("Recordings").is_err());
    assert!(MediaKind::parse("..").is_err());
    assert_eq!(MediaKind::Recordings.as_str(), "recordings");
}

#[test]
fn recording_names_must_be_bare_own_names() {
    let k = MediaKind::Recordings;
    assert!(is_media_name(k, "rec-2026-01-01_10-00-00.mp4"));
    assert!(is_media_name(k, "rec-x.GIF"));
    for bad in [
        "", "..", ".", "../rec-a.mp4", "..\\rec-a.mp4", "sub/rec-a.mp4", "sub\\rec-a.mp4", "C:rec-a.mp4", "C:\\rec-a.mp4",
        "rec-a.mp4:stream", "rec-a?.mp4", "rec-a*.mp4", "rec-\"a\".mp4", "rec-a\n.mp4", "holiday.mp4", "rec-a.txt", "rec-a.mic.wav",
    ] {
        assert!(!is_media_name(k, bad), "{bad:?} must be refused");
    }
}

#[test]
fn screenshot_names_must_be_bare_own_names() {
    let k = MediaKind::Screenshots;
    assert!(is_media_name(k, "Screenshot-2026-01-01_10-00-00.png"));
    assert!(is_media_name(k, "Screenshot-Code-2026-01-01_10-00-00.JPG"));
    for bad in [
        "", "..", "../Screenshot-a.png", "..\\Screenshot-a.png", "x/Screenshot-a.png", "C:\\Screenshot-a.png", "Screenshot-a.png:x",
        "Screenshot 2026-01-01 at 10.00.00.png", "Screenshot-a.gif", "Screenshot-a.txt", "photo.png", "rec-a.mp4", "Screenshot-a|b.png",
    ] {
        assert!(!is_media_name(k, bad), "{bad:?} must be refused");
    }
    assert!(!is_media_name(MediaKind::Recordings, "Screenshot-a.png"));
    assert!(!is_media_name(MediaKind::Screenshots, "rec-a.mp4"));
}

#[test]
fn list_media_in_lists_only_own_files_of_the_kind() {
    let dir = scratch("list");
    for n in ["rec-a.mp4", "rec-b.webm", "other.mp4", "rec-c.txt", "Screenshot-1.png"] {
        fs::write(dir.join(n), b"data").unwrap();
    }
    fs::create_dir_all(dir.join("rec-folder.mp4")).unwrap();
    fs::create_dir_all(dir.join("verworfen")).unwrap();
    fs::write(dir.join("verworfen").join("rec-old.mp4"), b"x").unwrap();
    let mut names: Vec<String> = list_media_in(MediaKind::Recordings, &dir).into_iter().map(|f| f.name).collect();
    names.sort();
    assert_eq!(names, ["rec-a.mp4", "rec-b.webm"], "foreign files, folders and the holding folder stay out");
    let shots: Vec<String> = list_media_in(MediaKind::Screenshots, &dir).into_iter().map(|f| f.name).collect();
    assert_eq!(shots, ["Screenshot-1.png"]);
    let a = list_media_in(MediaKind::Recordings, &dir).into_iter().find(|f| f.name == "rec-a.mp4").unwrap();
    assert_eq!((a.size, a.ext.as_str()), (4, "mp4"));
    assert!(a.modified_ms > 0);
    assert!(list_media_in(MediaKind::Recordings, &dir.join("missing")).is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn poster_cache_key_follows_path_and_modification_time() {
    let a = poster_cache_key("C:\\v\\rec-a.mp4", 1000);
    assert_eq!(a, poster_cache_key("C:\\v\\rec-a.mp4", 1000), "stable for the same file state");
    assert_ne!(a, poster_cache_key("C:\\v\\rec-a.mp4", 1001), "an edited file gets a new poster");
    assert_ne!(a, poster_cache_key("C:\\v\\rec-b.mp4", 1000));
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()), "a file-name-safe key");
}

#[test]
fn poster_seek_stays_inside_short_clips() {
    assert_eq!(poster_seek_secs(0), 0.0);
    assert!((poster_seek_secs(2000) - 0.2).abs() < 1e-9);
    assert_eq!(poster_seek_secs(600_000), 1.0);
}

#[test]
fn an_image_poster_is_a_320_px_wide_jpg_and_is_reused() {
    let dir = scratch("poster");
    let name = "Screenshot-poster-test.png";
    image::RgbaImage::from_pixel(800, 400, image::Rgba([200, 30, 30, 255])).save(dir.join(name)).unwrap();
    let first = media_poster_blocking(MediaKind::Screenshots, None, None, &dir, name, || false).expect("a poster");
    let (w, h) = image::image_dimensions(&first).unwrap();
    assert_eq!((w, h), (320, 160));
    let again = media_poster_blocking(MediaKind::Screenshots, None, None, &dir, name, || false).unwrap();
    assert_eq!(first, again, "the cached file is reused");
    assert!(media_poster_blocking(MediaKind::Screenshots, None, None, &dir, "../x.png", || false).is_none());
    assert!(media_poster_blocking(MediaKind::Screenshots, None, None, &dir, "Screenshot-missing.png", || false).is_none());
    assert!(media_poster_blocking(MediaKind::Recordings, None, None, &dir, "rec-a.mp4", || false).is_none(), "no ffmpeg, no video poster");
    let _ = fs::remove_file(&first);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_stale_poster_request_gives_up_without_working() {
    let dir = scratch("poster-stale");
    let name = "Screenshot-poster-stale.png";
    image::RgbaImage::from_pixel(640, 480, image::Rgba([10, 90, 200, 255])).save(dir.join(name)).unwrap();
    // All slots taken: a request that is stale answers None instead of waiting for one.
    let held: Vec<_> = (0..POSTER_PARALLEL).map(|_| acquire_poster_slot(|| false).expect("free slot")).collect();
    assert!(media_poster_blocking(MediaKind::Screenshots, None, None, &dir, name, || true).is_none());
    drop(held);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn media_probe_reads_image_sizes_and_refuses_foreign_names() {
    let dir = scratch("probe");
    image::RgbaImage::from_pixel(30, 20, image::Rgba([0, 0, 0, 255])).save(dir.join("Screenshot-p.png")).unwrap();
    fs::write(dir.join("notes.png"), b"x").unwrap();
    let names = vec!["Screenshot-p.png".to_string(), "notes.png".to_string(), "../Screenshot-p.png".to_string()];
    let out = media_probe_in(MediaKind::Screenshots, None, &dir, &names);
    assert_eq!(out.len(), 3);
    assert_eq!((out[0].width, out[0].height), (30, 20));
    assert_eq!((out[1].width, out[1].height, out[1].duration_ms), (0, 0, 0));
    assert_eq!((out[2].width, out[2].height), (0, 0));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discarding_recordings_goes_through_the_holding_folder_and_can_be_undone() {
    let dir = scratch("discard-rec");
    fs::write(dir.join("rec-a.mp4"), b"aaaa").unwrap();
    fs::write(dir.join("rec-a.mic.wav"), b"mic").unwrap();
    fs::write(dir.join("holiday.mp4"), b"keep").unwrap();
    let names = vec!["rec-a.mp4".to_string(), "holiday.mp4".to_string(), "../rec-a.mp4".to_string(), "rec-missing.mp4".to_string()];
    assert_eq!(media_discard_in(MediaKind::Recordings, &dir, &names), 1, "only the own, existing recording moves");
    assert!(!dir.join("rec-a.mp4").exists());
    assert!(dir.join("holiday.mp4").exists(), "a foreign video is never touched");
    assert!(dir.join("verworfen").join("rec-a.mp4").is_file());
    assert!(dir.join("verworfen").join("rec-a.mic.wav").is_file(), "the sidecar goes along");
    let listed = list_discarded_in(MediaKind::Recordings, &dir);
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].name.as_str(), listed[0].size), ("rec-a.mp4", 4));
    assert!(!media_restore_in(MediaKind::Recordings, &dir, "../rec-a.mp4"));
    assert!(media_restore_in(MediaKind::Recordings, &dir, "rec-a.mp4"));
    assert!(dir.join("rec-a.mp4").is_file());
    assert!(dir.join("rec-a.mic.wav").is_file());
    assert_eq!(media_discard_in(MediaKind::Recordings, &dir, &["rec-a.mp4".to_string()]), 1);
    assert!(media_delete_now_in(MediaKind::Recordings, &dir, "rec-a.mp4"));
    assert!(!dir.join("verworfen").join("rec-a.mp4").exists());
    assert!(!media_delete_now_in(MediaKind::Recordings, &dir, "rec-a.mp4"), "already gone");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discarding_screenshots_goes_through_their_own_holding_folder() {
    let dir = scratch("discard-shot");
    fs::write(dir.join("Screenshot-a.png"), b"1234").unwrap();
    fs::write(dir.join("Screenshot 2026-01-01 at 10.00.png"), b"win").unwrap();
    fs::write(dir.join("rec-a.mp4"), b"vid").unwrap();
    let names: Vec<String> = ["Screenshot-a.png", "Screenshot 2026-01-01 at 10.00.png", "rec-a.mp4", "..\\Screenshot-a.png"].map(String::from).to_vec();
    assert_eq!(media_discard_in(MediaKind::Screenshots, &dir, &names), 1, "a Windows-style name and a recording stay put");
    assert!(dir.join("Screenshot 2026-01-01 at 10.00.png").is_file());
    assert!(dir.join("rec-a.mp4").is_file());
    let held = dir.join("verworfen").join("Screenshot-a.png");
    assert!(held.is_file());
    let listed = list_discarded_in(MediaKind::Screenshots, &dir);
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].name.as_str(), listed[0].size), ("Screenshot-a.png", 4));
    // The holding folder is not part of the normal listing.
    assert!(list_media_in(MediaKind::Screenshots, &dir).iter().all(|f| f.name != "Screenshot-a.png"));
    // A same-named file at the old spot blocks the restore: nothing is overwritten.
    fs::write(dir.join("Screenshot-a.png"), b"new").unwrap();
    assert!(!media_restore_in(MediaKind::Screenshots, &dir, "Screenshot-a.png"));
    assert_eq!(fs::read(dir.join("Screenshot-a.png")).unwrap(), b"new");
    fs::remove_file(dir.join("Screenshot-a.png")).unwrap();
    assert!(media_restore_in(MediaKind::Screenshots, &dir, "Screenshot-a.png"));
    assert_eq!(fs::read(dir.join("Screenshot-a.png")).unwrap(), b"1234");
    assert_eq!(media_discard_in(MediaKind::Screenshots, &dir, &["Screenshot-a.png".to_string()]), 1);
    assert!(!media_delete_now_in(MediaKind::Screenshots, &dir, "../Screenshot-a.png"));
    assert!(media_delete_now_in(MediaKind::Screenshots, &dir, "Screenshot-a.png"));
    assert!(list_discarded_in(MediaKind::Screenshots, &dir).is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_discarded_screenshot_starts_its_keep_window_at_the_discard() {
    let dir = scratch("shot-clock");
    let src = dir.join("Screenshot-old.png");
    fs::write(&src, b"x").unwrap();
    let f = fs::OpenOptions::new().write(true).open(&src).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(30 * 24 * 3600)).unwrap();
    drop(f);
    assert_eq!(media_discard_in(MediaKind::Screenshots, &dir, &["Screenshot-old.png".to_string()]), 1);
    let trash = discard_trash_dir(&dir);
    assert_eq!(discarded_screenshots_stale_in(&trash, Duration::from_secs(24 * 3600)), (0, 0), "just discarded: not stale");
    let held = trash.join("Screenshot-old.png");
    let f = fs::OpenOptions::new().write(true).open(&held).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(3 * 24 * 3600)).unwrap();
    drop(f);
    fs::write(trash.join("notes.txt"), b"foreign").unwrap();
    assert_eq!(discarded_screenshots_stale_in(&trash, Duration::from_secs(24 * 3600)), (1, 1));
    assert!(!held.exists());
    assert!(trash.join("notes.txt").exists(), "only own screenshot names are swept");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_manager_window_size_keeps_to_the_screen_and_its_minimum() {
    assert_eq!(manager_default_size((1920, 1080)), (1100, 720));
    assert_eq!(manager_default_size((1024, 600)), (1024, 600));
    assert_eq!(manager_clamped_size(300, 200, (1920, 1080)), (640, 420), "never below the minimum");
    assert_eq!(manager_clamped_size(5000, 5000, (1920, 1080)), (1920, 1080), "never above the screen");
    assert_eq!(manager_clamped_size(900, 600, (1920, 1080)), (900, 600));
}

#[test]
fn every_manager_command_is_registered_and_reaches_only_validated_names() {
    let lib = include_str!("../lib.rs");
    let src = include_str!("../media_manager.rs");
    for cmd in [
        "open_media_manager", "list_media", "media_probe", "media_poster", "media_reveal", "media_play", "media_copy_file",
        "media_copy_image", "media_discard", "media_list_discarded", "media_restore", "media_delete_now", "media_open_in_main",
        "media_export_in_main", "media_open_setting",
    ] {
        assert!(lib.contains(&format!("media_manager::{cmd},")), "{cmd} is not in the handler list");
    }
    // The page never names a folder or a path: no command takes a `dir` or `path` argument.
    for line in src.lines().filter(|l| l.contains("pub(crate) fn media_") || l.contains("pub(crate) async fn media_")) {
        assert!(!line.contains("dir:") && !line.contains("path:"), "path-taking command: {line}");
    }
}
